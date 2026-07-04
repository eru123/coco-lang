//! Bytecode verification for `.cb` artifacts.
//!
//! `deserialize_chunk` validates the *structural* integrity of a `.cb` file
//! (magic, version, bounds, UTF-8). `verify_chunk` goes further: it walks the
//! decoded `Chunk`'s code and checks that the bytecode is *safe to execute* —
//! every opcode is known, every instruction fits within `code`, constant-pool
//! indices are in range, and jump targets land on instruction boundaries.
//!
//! This guards against a corrupted or hand-crafted `.cb` file driving the VM
//! into an out-of-bounds read or a misaligned instruction stream. The VM
//! itself trusts the chunk, so verification is the security boundary between
//! untrusted artifacts and execution.
//!
//! Verification is recursive: each `FnObj` constant carries its own `Chunk`,
//! which is verified in turn.

use crate::ir::{
    operand_bytes, read_i16, read_u16, OP_CONST, OP_DEFINE_GLOBAL, OP_LOAD_GLOBAL, OP_MAKE_CLOSURE,
    OP_MEMBER, OP_METHOD_CALL, OP_STORE_GLOBAL, OP_STORE_MEMBER, OP_SUPER_METHOD, OP_TYPE_IS,
};
use crate::ir::{Chunk, FnObj};
use crate::value::Value;

/// An error found by bytecode verification.
#[derive(Debug, Clone)]
pub struct VerifyError {
    pub message: String,
    /// Code offset where the error was detected, if applicable.
    pub offset: Option<usize>,
    /// Name of the function/chunk being verified (for nested FnObj context).
    pub context: Option<String>,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.context.as_deref(), self.offset) {
            (Some(ctx), Some(off)) => {
                write!(
                    f,
                    "verify error in {} at offset {}: {}",
                    ctx, off, self.message
                )
            }
            (Some(ctx), None) => write!(f, "verify error in {}: {}", ctx, self.message),
            (None, Some(off)) => write!(f, "verify error at offset {}: {}", off, self.message),
            (None, None) => write!(f, "verify error: {}", self.message),
        }
    }
}

impl std::error::Error for VerifyError {}

impl VerifyError {
    fn at(offset: usize, ctx: &VerifyCtx, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: Some(offset),
            context: ctx.name.clone(),
        }
    }

    fn plain(ctx: &VerifyCtx, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: None,
            context: ctx.name.clone(),
        }
    }
}

/// Carries the name of the chunk being verified (for nested-FnObj context).
struct VerifyCtx {
    name: Option<String>,
}

/// Verify a top-level `Chunk` (the script body). Returns `Ok(())` if the
/// bytecode is safe to execute, or the first `VerifyError` found.
pub fn verify_chunk(chunk: &Chunk) -> Result<(), VerifyError> {
    verify_chunk_named(chunk, None)
}

/// Verify a `Chunk` with an optional name (used for nested FnObj chunks so
/// error messages can point at the offending function).
fn verify_chunk_named(chunk: &Chunk, name: Option<String>) -> Result<(), VerifyError> {
    let ctx = VerifyCtx { name };
    verify_code(chunk, &ctx)?;
    // Verify each constant. FnObj constants carry nested chunks that must be
    // verified recursively; other constants are data and need no code check.
    for (i, value) in chunk.constants.iter().enumerate() {
        if let Value::FnObj(fo) = value {
            verify_fnobj(fo, &ctx).map_err(|e| {
                // Attach the constant index for better diagnostics.
                VerifyError {
                    message: format!("in constant #{}: {}", i, e.message),
                    offset: e.offset,
                    context: e.context,
                }
            })?;
        }
    }
    Ok(())
}

fn verify_fnobj(fo: &FnObj, parent: &VerifyCtx) -> Result<(), VerifyError> {
    let name = Some(fo.name.clone());
    let _ = parent; // context is carried by the recursive call's own name
    verify_chunk_named(&fo.chunk, name)
}

/// Walk the instruction stream and check each instruction.
fn verify_code(chunk: &Chunk, ctx: &VerifyCtx) -> Result<(), VerifyError> {
    let code = &chunk.code;
    let n = code.len();
    let mut ip = 0usize;

    // Collect instruction start offsets so we can validate that forward jump
    // targets land on an instruction boundary (not mid-operand).
    let mut instruction_starts: std::collections::HashSet<usize> = std::collections::HashSet::new();
    {
        let mut scan = 0usize;
        while scan < n {
            instruction_starts.insert(scan);
            let op = code[scan];
            match operand_bytes(op) {
                Some(ob) => scan += 1 + ob,
                None => {
                    return Err(VerifyError::at(
                        scan,
                        ctx,
                        format!("unknown opcode 0x{:02x}", op),
                    ))
                }
            }
        }
        // If scan overshoots n, the last instruction's operands were truncated.
        if scan != n {
            return Err(VerifyError::plain(
                ctx,
                format!(
                    "truncated instruction stream: {} bytes, last instruction overruns by {}",
                    n,
                    scan.saturating_sub(n)
                ),
            ));
        }
    }

    // Second pass: validate operand semantics (constant indices, jump targets).
    ip = 0;
    while ip < n {
        let op = code[ip];
        let ob = operand_bytes(op).expect("checked in first pass");
        let operand_start = ip + 1;
        let operand_end = operand_start + ob;
        // Bounds check (redundant given the first pass, but defensive).
        if operand_end > n {
            return Err(VerifyError::at(
                ip,
                ctx,
                format!("opcode 0x{:02x} operands run past end of code", op),
            ));
        }

        // Opcodes whose u16 operand is a constant-pool index.
        let const_index_op = matches!(
            op,
            OP_CONST
                | OP_LOAD_GLOBAL
                | OP_STORE_GLOBAL
                | OP_DEFINE_GLOBAL
                | OP_MAKE_CLOSURE
                | OP_MEMBER
                | OP_STORE_MEMBER
                | OP_TYPE_IS
        );
        if const_index_op {
            let idx = read_u16(&code[operand_start..operand_end]) as usize;
            if idx >= chunk.constants.len() {
                return Err(VerifyError::at(
                    ip,
                    ctx,
                    format!(
                        "constant index {} out of range (pool has {})",
                        idx,
                        chunk.constants.len()
                    ),
                ));
            }
        }

        // METHOD_CALL / SUPER_METHOD: u16 name_idx (constant) + u8 arg_count.
        if matches!(op, OP_METHOD_CALL | OP_SUPER_METHOD) {
            let name_idx = read_u16(&code[operand_start..operand_start + 2]) as usize;
            if name_idx >= chunk.constants.len() {
                return Err(VerifyError::at(
                    ip,
                    ctx,
                    format!(
                        "method-call name index {} out of range (pool has {})",
                        name_idx,
                        chunk.constants.len()
                    ),
                ));
            }
            // The name constant should be a String. If it's not, that's a
            // likely corruption — flag it.
            if !matches!(chunk.constants[name_idx], Value::String(_)) {
                return Err(VerifyError::at(
                    ip,
                    ctx,
                    format!("method-call name constant #{} is not a string", name_idx),
                ));
            }
        }

        // Forward jumps: OP_JUMP / OP_JUMP_IF_FALSE / OP_JUMP_IF_TRUE /
        // OP_POP_JUMP_IF_FALSE. The i16 offset is relative to the end of the
        // jump instruction. Target must land on an instruction boundary.
        if matches!(
            op,
            crate::ir::OP_JUMP
                | crate::ir::OP_JUMP_IF_FALSE
                | crate::ir::OP_JUMP_IF_TRUE
                | crate::ir::OP_POP_JUMP_IF_FALSE
        ) {
            let off = read_i16(&code[operand_start..operand_end]) as isize;
            let target = ip as isize + 1 + ob as isize + off;
            if target < 0 || (target as usize) > n {
                return Err(VerifyError::at(
                    ip,
                    ctx,
                    format!("jump target {} out of code bounds (0..{})", target, n),
                ));
            }
            // target == n is allowed (jump past end, e.g. to a implicit return).
            if (target as usize) < n && !instruction_starts.contains(&(target as usize)) {
                return Err(VerifyError::at(
                    ip,
                    ctx,
                    format!(
                        "jump target {} does not land on an instruction boundary",
                        target
                    ),
                ));
            }
        }

        // OP_LOOP: backward jump. Target = ip + 1 + ob - offset.
        if op == crate::ir::OP_LOOP {
            let off = read_i16(&code[operand_start..operand_end]) as isize;
            // LOOP offset is stored as a positive i16 representing backward
            // distance from the end of the LOOP instruction.
            let target = ip as isize + 1 + ob as isize - off;
            if target < 0 || (target as usize) >= n {
                return Err(VerifyError::at(
                    ip,
                    ctx,
                    format!("loop target {} out of code bounds (0..{})", target, n),
                ));
            }
            if !instruction_starts.contains(&(target as usize)) {
                return Err(VerifyError::at(
                    ip,
                    ctx,
                    format!(
                        "loop target {} does not land on an instruction boundary",
                        target
                    ),
                ));
            }
        }

        // OP_TRY_BEGIN: u16 handler offset (absolute instruction boundary).
        if op == crate::ir::OP_TRY_BEGIN {
            let handler = read_u16(&code[operand_start..operand_end]) as usize;
            if handler >= n {
                return Err(VerifyError::at(
                    ip,
                    ctx,
                    format!(
                        "try handler offset {} out of code bounds (0..{})",
                        handler, n
                    ),
                ));
            }
            if !instruction_starts.contains(&handler) {
                return Err(VerifyError::at(
                    ip,
                    ctx,
                    format!(
                        "try handler offset {} does not land on an instruction boundary",
                        handler
                    ),
                ));
            }
        }

        // OP_SELECT_TRY_RECV: u16 jump offset if channel empty (forward jump).
        if op == crate::ir::OP_SELECT_TRY_RECV {
            let off = read_u16(&code[operand_start..operand_end]) as usize;
            let target = ip + 1 + ob + off;
            if target > n {
                return Err(VerifyError::at(
                    ip,
                    ctx,
                    format!(
                        "select-try-recv jump target {} out of code bounds (0..{})",
                        target, n
                    ),
                ));
            }
            if target < n && !instruction_starts.contains(&target) {
                return Err(VerifyError::at(
                    ip,
                    ctx,
                    format!(
                        "select-try-recv jump target {} does not land on an instruction boundary",
                        target
                    ),
                ));
            }
        }

        ip = operand_end;
    }

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    fn make_chunk(code: Vec<u8>, constants: Vec<Value>) -> Chunk {
        Chunk {
            code,
            constants,
            lines: Vec::new(),
        }
    }

    #[test]
    fn verifies_minimal_valid_chunk() {
        // OP_NULL, OP_RETURN
        let chunk = make_chunk(vec![OP_NULL, OP_RETURN], vec![]);
        assert!(verify_chunk(&chunk).is_ok());
    }

    #[test]
    fn rejects_unknown_opcode() {
        // 0xFF is not a valid opcode.
        let chunk = make_chunk(vec![0xFF, OP_RETURN], vec![]);
        let err = verify_chunk(&chunk).unwrap_err();
        assert!(err.message.contains("unknown opcode"));
    }

    #[test]
    fn rejects_truncated_instruction() {
        // OP_CONST expects 2 operand bytes but only 1 follows.
        let chunk = make_chunk(vec![OP_CONST, 0], vec![Value::Int64(0)]);
        let err = verify_chunk(&chunk).unwrap_err();
        assert!(err.message.contains("truncated") || err.message.contains("past end"));
    }

    #[test]
    fn rejects_const_index_out_of_range() {
        // OP_CONST 5 — but the pool is empty.
        let chunk = make_chunk(vec![OP_CONST, 5, 0, OP_RETURN], vec![]);
        let err = verify_chunk(&chunk).unwrap_err();
        assert!(err.message.contains("constant index 5 out of range"));
    }

    #[test]
    fn accepts_const_index_in_range() {
        let chunk = make_chunk(vec![OP_CONST, 0, 0, OP_RETURN], vec![Value::Int64(42)]);
        assert!(verify_chunk(&chunk).is_ok());
    }

    #[test]
    fn rejects_jump_to_misaligned_offset() {
        // Construct a JUMP_IF_FALSE whose target lands mid-instruction (on the
        // second operand byte of a CONST). Layout:
        //   [0] CONST [2]0 [3]0     (3-byte instr, ends at 4)
        //   [4] JUMP_IF_FALSE [6]<off-lo> [7]<off-hi>   (ends at 8)
        //   [8] NULL [9] RETURN
        // We want target = 2 (mid-CONST). target = ip + 1 + ob + off = 4 + 1 + 2 + off = 7 + off.
        // So off = 2 - 7 = -5 → i16 bytes (little-endian) = 0xFB 0xFF.
        let mut code = vec![OP_CONST, 0, 0];
        code.push(OP_JUMP_IF_FALSE);
        code.push(0xFB); // -5 low byte
        code.push(0xFF); // -5 high byte
        code.push(OP_NULL);
        code.push(OP_RETURN);
        let chunk = make_chunk(code, vec![Value::Int64(0)]);
        let err = verify_chunk(&chunk).unwrap_err();
        assert!(
            err.message.contains("instruction boundary"),
            "expected boundary error, got: {}",
            err
        );
    }

    #[test]
    fn accepts_jump_to_instruction_boundary() {
        // CONST 0 (3 bytes), JUMP_IF_FALSE -> offset 0 (lands just after the jump,
        // i.e. on the NULL), NULL, RETURN.
        let code = vec![
            OP_CONST,
            0,
            0, // push const 0
            OP_JUMP_IF_FALSE,
            0,
            0, // jump 0 → lands on next instr (NULL)
            OP_NULL,
            OP_RETURN,
        ];
        let chunk = make_chunk(code, vec![Value::Int64(0)]);
        assert!(verify_chunk(&chunk).is_ok());
    }

    #[test]
    fn verifies_nested_fnobj_chunk() {
        // A FnObj constant whose own chunk is invalid (unknown opcode).
        let bad_inner = make_chunk(vec![0xFE, OP_RETURN], vec![]);
        let fo = FnObj {
            name: "bad".to_string(),
            arity: 0,
            chunk: bad_inner,
            is_async: false,
        };
        let outer = make_chunk(vec![OP_CONST, 0, 0, OP_RETURN], vec![Value::FnObj(fo)]);
        let err = verify_chunk(&outer).unwrap_err();
        assert!(err.message.contains("in constant #0"));
        assert!(err.message.contains("unknown opcode"));
    }

    #[test]
    fn rejects_method_call_non_string_name() {
        // METHOD_CALL with name_idx pointing at an Int constant.
        let code = vec![OP_METHOD_CALL, 0, 0, 1, OP_RETURN];
        let chunk = make_chunk(code, vec![Value::Int64(5)]);
        let err = verify_chunk(&chunk).unwrap_err();
        assert!(err.message.contains("not a string"));
    }

    #[test]
    fn real_compiled_chunk_verifies() {
        // Compile a real program and verify it — the compiler's output must
        // always pass verification.
        use crate::compiler::Compiler;
        let src = "fn fib(n: int): int { if n < 2 { return n; } return fib(n-1) + fib(n-2); } fn main(): int { return fib(10); }";
        let program = coco_parser::Parser::new(src).parse_program();
        let chunk = Compiler::new().compile_script(&program).expect("compile");
        assert!(
            verify_chunk(&chunk).is_ok(),
            "compiler output failed verification"
        );
    }
}
