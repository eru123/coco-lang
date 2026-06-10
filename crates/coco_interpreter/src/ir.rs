//! Intermediate representation — bytecode instruction set, Chunk, and builder.
//!
//! The IR is a stack-based bytecode. The compiler emits instructions into a
//! `ChunkBuilder`, which produces an immutable `Chunk` ready for the VM.
//!
//! ## Instruction encoding
//!
//! Every instruction is 1 opcode byte followed by operand bytes:
//!
//! | Opcode              | Bytes | Operands                  |
//! |---------------------|-------|---------------------------|
//! | Const               | 3     | u16 constant-index        |
//! | Null / True / False | 1     | none                      |
//! | LoadLocal           | 3     | u16 slot-index            |
//! | StoreLocal          | 3     | u16 slot-index            |
//! | LoadGlobal          | 3     | u16 constant-index (name) |
//! | StoreGlobal         | 3     | u16 constant-index (name) |
//! | DefineGlobal        | 3     | u16 constant-index (name) |
//! | Add–Ge, BitAnd–Shr  | 1     | none                      |
//! | Neg / Not / BitNot  | 1     | none                      |
//! | Jump / JumpIfFalse  | 3     | i16 offset                |
//! | JumpIfTrue          | 3     | i16 offset                |
//! | PopJumpIfFalse      | 3     | i16 offset                |
//! | Loop                | 3     | i16 offset (negative)     |
//! | Call                | 2     | u8 arg-count              |
//! | Return              | 1     | none                      |
//! | MakeClosure         | 3     | u16 constant-index        |
//! | BuildList           | 3     | u16 element-count         |
//! | BuildMap            | 3     | u16 pair-count            |
//! | Index / StoreIndex  | 1     | none                      |
//! | Member / StoreMember| 3     | u16 constant-index (name) |
//! | Pop / Dup           | 1     | none                      |
//! | Throw               | 1     | none                      |
//! | TryBegin            | 3     | u16 handler-offset        |
//! | TryEnd / Catch      | 1     | none                      |
//! | AssignOp            | 2     | u8 op-kind                |

use crate::value::Value;

// ============================================================================
// Opcodes
// ============================================================================

pub const OP_CONST: u8 = 0;
pub const OP_NULL: u8 = 1;
pub const OP_TRUE: u8 = 2;
pub const OP_FALSE: u8 = 3;

pub const OP_LOAD_LOCAL: u8 = 4;
pub const OP_STORE_LOCAL: u8 = 5;
pub const OP_LOAD_GLOBAL: u8 = 6;
pub const OP_STORE_GLOBAL: u8 = 7;
pub const OP_DEFINE_GLOBAL: u8 = 8;

pub const OP_ADD: u8 = 10;
pub const OP_SUB: u8 = 11;
pub const OP_MUL: u8 = 12;
pub const OP_DIV: u8 = 13;
pub const OP_MOD: u8 = 14;
pub const OP_POW: u8 = 15;

pub const OP_EQ: u8 = 16;
pub const OP_NE: u8 = 17;
pub const OP_LT: u8 = 18;
pub const OP_GT: u8 = 19;
pub const OP_LE: u8 = 20;
pub const OP_GE: u8 = 21;

pub const OP_BIT_AND: u8 = 22;
pub const OP_BIT_OR: u8 = 23;
pub const OP_BIT_XOR: u8 = 24;
pub const OP_SHL: u8 = 25;
pub const OP_SHR: u8 = 26;

pub const OP_NEG: u8 = 30;
pub const OP_NOT: u8 = 31;
pub const OP_BIT_NOT: u8 = 32;

pub const OP_JUMP: u8 = 40;
pub const OP_JUMP_IF_FALSE: u8 = 41;
pub const OP_JUMP_IF_TRUE: u8 = 42;
pub const OP_POP_JUMP_IF_FALSE: u8 = 43;
pub const OP_LOOP: u8 = 44;

pub const OP_CALL: u8 = 50;
pub const OP_RETURN: u8 = 51;
pub const OP_MAKE_CLOSURE: u8 = 52;

pub const OP_BUILD_LIST: u8 = 60;
pub const OP_BUILD_MAP: u8 = 61;
pub const OP_INDEX: u8 = 62;
pub const OP_STORE_INDEX: u8 = 63;
pub const OP_MEMBER: u8 = 64;
pub const OP_STORE_MEMBER: u8 = 65;

pub const OP_POP: u8 = 70;
pub const OP_DUP: u8 = 71;

pub const OP_THROW: u8 = 80;
pub const OP_TRY_BEGIN: u8 = 81;
pub const OP_TRY_END: u8 = 82;
pub const OP_CATCH: u8 = 83;

// Async operations
pub const OP_ASYNC_CALL: u8 = 90;
pub const OP_AWAIT: u8 = 91;
pub const OP_LAZY_CALL: u8 = 92;
pub const OP_TRY: u8 = 93; // ? operator — propagate Err, unwrap Ok

/// Compound-assignment op sub-kinds carried by OP_ASSIGN_OP after the
/// right-hand side is already on the stack (left is below it).
pub const ASSIGN_OP_ADD: u8 = 0;
pub const ASSIGN_OP_SUB: u8 = 1;
pub const ASSIGN_OP_MUL: u8 = 2;
pub const ASSIGN_OP_DIV: u8 = 3;
pub const ASSIGN_OP_MOD: u8 = 4;
pub const ASSIGN_OP_POW: u8 = 5;

/// Return the human-readable name of an opcode byte.
pub fn opcode_name(op: u8) -> &'static str {
    match op {
        OP_CONST => "CONST",
        OP_NULL => "NULL",
        OP_TRUE => "TRUE",
        OP_FALSE => "FALSE",
        OP_LOAD_LOCAL => "LOAD_LOCAL",
        OP_STORE_LOCAL => "STORE_LOCAL",
        OP_LOAD_GLOBAL => "LOAD_GLOBAL",
        OP_STORE_GLOBAL => "STORE_GLOBAL",
        OP_DEFINE_GLOBAL => "DEFINE_GLOBAL",
        OP_ADD => "ADD",
        OP_SUB => "SUB",
        OP_MUL => "MUL",
        OP_DIV => "DIV",
        OP_MOD => "MOD",
        OP_POW => "POW",
        OP_EQ => "EQ",
        OP_NE => "NE",
        OP_LT => "LT",
        OP_GT => "GT",
        OP_LE => "LE",
        OP_GE => "GE",
        OP_BIT_AND => "BIT_AND",
        OP_BIT_OR => "BIT_OR",
        OP_BIT_XOR => "BIT_XOR",
        OP_SHL => "SHL",
        OP_SHR => "SHR",
        OP_NEG => "NEG",
        OP_NOT => "NOT",
        OP_BIT_NOT => "BIT_NOT",
        OP_JUMP => "JUMP",
        OP_JUMP_IF_FALSE => "JUMP_IF_FALSE",
        OP_JUMP_IF_TRUE => "JUMP_IF_TRUE",
        OP_POP_JUMP_IF_FALSE => "POP_JUMP_IF_FALSE",
        OP_LOOP => "LOOP",
        OP_CALL => "CALL",
        OP_RETURN => "RETURN",
        OP_MAKE_CLOSURE => "MAKE_CLOSURE",
        OP_BUILD_LIST => "BUILD_LIST",
        OP_BUILD_MAP => "BUILD_MAP",
        OP_INDEX => "INDEX",
        OP_STORE_INDEX => "STORE_INDEX",
        OP_MEMBER => "MEMBER",
        OP_STORE_MEMBER => "STORE_MEMBER",
        OP_POP => "POP",
        OP_DUP => "DUP",
        OP_THROW => "THROW",
        OP_TRY_BEGIN => "TRY_BEGIN",
        OP_TRY_END => "TRY_END",
        OP_CATCH => "CATCH",
        OP_ASYNC_CALL => "ASYNC_CALL",
        OP_AWAIT => "AWAIT",
        OP_LAZY_CALL => "LAZY_CALL",
        OP_TRY => "TRY",
        _ => "???",
    }
}

/// Return the number of operand bytes (excluding the opcode byte) for an
/// instruction, or None if the opcode is unknown.
pub fn operand_bytes(op: u8) -> Option<usize> {
    match op {
        OP_CONST
        | OP_LOAD_LOCAL
        | OP_STORE_LOCAL
        | OP_LOAD_GLOBAL
        | OP_STORE_GLOBAL
        | OP_DEFINE_GLOBAL
        | OP_JUMP
        | OP_JUMP_IF_FALSE
        | OP_JUMP_IF_TRUE
        | OP_POP_JUMP_IF_FALSE
        | OP_LOOP
        | OP_MAKE_CLOSURE
        | OP_BUILD_LIST
        | OP_BUILD_MAP
        | OP_MEMBER
        | OP_STORE_MEMBER
        | OP_TRY_BEGIN => Some(2),

        OP_CALL | OP_ASYNC_CALL | OP_LAZY_CALL => Some(1),

        OP_AWAIT | OP_TRY => Some(0),

        OP_NULL
        | OP_TRUE
        | OP_FALSE
        | OP_ADD
        | OP_SUB
        | OP_MUL
        | OP_DIV
        | OP_MOD
        | OP_POW
        | OP_EQ
        | OP_NE
        | OP_LT
        | OP_GT
        | OP_LE
        | OP_GE
        | OP_BIT_AND
        | OP_BIT_OR
        | OP_BIT_XOR
        | OP_SHL
        | OP_SHR
        | OP_NEG
        | OP_NOT
        | OP_BIT_NOT
        | OP_RETURN
        | OP_INDEX
        | OP_STORE_INDEX
        | OP_POP
        | OP_DUP
        | OP_THROW
        | OP_TRY_END
        | OP_CATCH => Some(0),

        _ => None,
    }
}

// ============================================================================
// Encoding helpers
// ============================================================================

/// Write a u16 in little-endian to a byte slice.
#[inline]
pub fn write_u16(bytes: &mut [u8], val: u16) {
    bytes[0] = val as u8;
    bytes[1] = (val >> 8) as u8;
}

/// Read a u16 in little-endian from a byte slice.
#[inline]
pub fn read_u16(bytes: &[u8]) -> u16 {
    bytes[0] as u16 | ((bytes[1] as u16) << 8)
}

/// Write an i16 in little-endian (same bits as u16).
#[inline]
pub fn write_i16(bytes: &mut [u8], val: i16) {
    write_u16(bytes, val as u16);
}

/// Read an i16 in little-endian from a byte slice.
#[inline]
pub fn read_i16(bytes: &[u8]) -> i16 {
    read_u16(bytes) as i16
}

// ============================================================================
// Function object — a compiled function stored in the constant pool
// ============================================================================

/// A compiled function: a named bytecode chunk with its arity.
///
/// Stored in a Chunk's constant pool as `Value::FnObj(FnObj)`.
/// The VM creates runtime closures from these via `MAKE_CLOSURE`.
#[derive(Debug, Clone)]
pub struct FnObj {
    pub name: String,
    pub arity: usize,
    pub chunk: Chunk,
    /// Whether this function was declared with `async`.
    pub is_async: bool,
}

// ============================================================================
// Chunk — immutable compiled bytecode unit
// ============================================================================

/// A compiled unit of bytecode — a function body, module, or script.
///
/// `code` is a flat byte array of [opcode, operands...] sequences.
/// `constants` holds all literal values and names referenced by index.
/// `lines` maps instruction offsets to source lines for debugging.
#[derive(Debug, Clone)]
pub struct Chunk {
    pub code: Vec<u8>,
    pub constants: Vec<Value>,
    /// Pairs of (code_offset, source_line).
    pub lines: Vec<(usize, usize)>,
}

impl Chunk {
    /// Create an empty chunk.
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            constants: Vec::new(),
            lines: Vec::new(),
        }
    }

    /// Return the source line for a given code offset (binary search).
    pub fn line_at(&self, offset: usize) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        // Lines are recorded in order; find the last entry whose offset <= `offset`.
        let mut lo = 0usize;
        let mut hi = self.lines.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self.lines[mid].0 <= offset {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo == 0 {
            self.lines[0].1
        } else {
            self.lines[lo - 1].1
        }
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ChunkBuilder — mutable builder with label-based backpatching
// ============================================================================

/// A label representing a not-yet-known code offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Label(pub usize);

/// Mutable builder for constructing a `Chunk`.
///
/// Provides label-based forward references: the compiler can emit jumps to
/// labels that haven't been placed yet, and `patch_label` resolves them when
/// the target position is known.
pub struct ChunkBuilder {
    code: Vec<u8>,
    constants: Vec<Value>,
    lines: Vec<(usize, usize)>,
    patches: Vec<Patch>,
    /// Maps label ids to their code offsets (populated by place_label).
    label_positions: std::collections::HashMap<usize, usize>,
    /// Monotonic counter for unique label ids.
    next_label_id: usize,
    current_line: usize,
}

struct Patch {
    /// Code offset where the jump operand needs to be filled.
    offset: usize,
    /// Label to resolve.
    label: Label,
    /// True for LOOP (backward jump, offset = -(distance+3)).
    is_loop: bool,
}

impl ChunkBuilder {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            constants: Vec::new(),
            lines: Vec::new(),
            patches: Vec::new(),
            label_positions: std::collections::HashMap::new(),
            next_label_id: 1,
            current_line: 0,
        }
    }

    /// Set the current source line. Subsequent emitted instructions are
    /// recorded as belonging to this line.
    pub fn set_line(&mut self, line: usize) {
        self.current_line = line;
    }

    /// Allocate a new unresolved label with a unique id.
    pub fn new_label(&mut self) -> Label {
        let id = self.next_label_id;
        self.next_label_id += 1;
        Label(id)
    }

    /// Place a label at the current code position and resolve any forward
    /// references to it.
    pub fn place_label(&mut self, label: Label) {
        let pos = self.code.len();
        self.label_positions.insert(label.0, pos);
        // Resolve all patches targeting this label.
        let mut i = 0;
        while i < self.patches.len() {
            if self.patches[i].label == label {
                let patch = &self.patches[i];
                let offset = if patch.is_loop {
                    // LOOP offsets count backwards from the start of the
                    // *next* instruction, which is at `pos`.
                    -((pos as i16) - (patch.offset as i16))
                } else {
                    // Forward jump: distance from end of jump instruction to target.
                    // patch.offset is at ip+1, so: target = ip + 3 + offset.
                    // We want target = pos, so offset = pos - ip - 3 = pos - (patch.offset - 1) - 3.
                    (pos as i16) - (patch.offset as i16) - 2
                };
                write_i16(&mut self.code[patch.offset..patch.offset + 2], offset);
            }
            i += 1;
        }
        self.patches.retain(|p| p.label != label);
    }

    /// Emit a raw opcode byte without operands.
    pub fn emit_op(&mut self, op: u8) {
        self.record_line();
        self.code.push(op);
    }

    /// Emit an opcode followed by a u16 operand.
    pub fn emit_op_u16(&mut self, op: u8, val: u16) {
        self.record_line();
        self.code.push(op);
        let start = self.code.len();
        self.code.push(0);
        self.code.push(0);
        write_u16(&mut self.code[start..start + 2], val);
    }

    /// Emit an opcode followed by a u8 operand.
    pub fn emit_op_u8(&mut self, op: u8, val: u8) {
        self.record_line();
        self.code.push(op);
        self.code.push(val);
    }

    /// Emit a jump instruction whose target is a label not yet placed.
    /// The operand will be patched when `place_label` is called.
    pub fn emit_jump(&mut self, jump_op: u8, label: Label) {
        self.record_line();
        self.code.push(jump_op);
        let operand_offset = self.code.len();
        self.code.push(0);
        self.code.push(0); // placeholder
        self.patches.push(Patch {
            offset: operand_offset,
            label,
            is_loop: jump_op == OP_LOOP,
        });
    }

    /// Emit a LOOP instruction (backward jump to an already-placed label).
    /// The caller must have already called `place_label` for `label`.
    pub fn emit_loop(&mut self, label: Label) {
        self.record_line();
        let target_pos = self
            .label_positions
            .get(&label.0)
            .copied()
            .expect("emit_loop: label not placed");
        let loop_start = self.code.len();
        self.code.push(OP_LOOP);
        let operand_offset = self.code.len();
        // LOOP offset: distance backward from END of LOOP instruction to target.
        let end_of_loop = loop_start + 3;
        let distance = (end_of_loop as i16) - (target_pos as i16);
        // Store as positive i16 for backward distance.
        self.code.push(0);
        self.code.push(0);
        write_i16(&mut self.code[operand_offset..operand_offset + 2], distance);
    }

    /// Add a constant and return its index.
    pub fn add_constant(&mut self, value: Value) -> u16 {
        // Deduplicate simple values
        if let Some(pos) = self.constants.iter().position(|c| values_eq(c, &value)) {
            return pos as u16;
        }
        let idx = self.constants.len();
        if idx > u16::MAX as usize {
            panic!("too many constants");
        }
        self.constants.push(value);
        idx as u16
    }

    /// Current code position (for labels).
    pub fn current_offset(&self) -> usize {
        self.code.len()
    }

    /// Consume the builder and produce an immutable Chunk.
    pub fn finish(self) -> Chunk {
        if !self.patches.is_empty() {
            panic!(
                "ChunkBuilder finished with {} unresolved patches",
                self.patches.len()
            );
        }
        Chunk {
            code: self.code,
            constants: self.constants,
            lines: self.lines,
        }
    }

    fn record_line(&mut self) {
        if self.lines.is_empty() || self.lines.last().map(|(_, l)| *l).unwrap_or(999) != self.current_line {
            self.lines.push((self.code.len(), self.current_line));
        }
    }
}

// ============================================================================
// Disassembler
// ============================================================================

/// Disassemble a chunk into a human-readable string.
pub fn disassemble(chunk: &Chunk, name: &str) -> String {
    let mut out = format!("== {} ==\n", name);
    let mut offset = 0;
    while offset < chunk.code.len() {
        offset = disassemble_instruction(chunk, offset, &mut out);
    }
    out
}

/// Disassemble a single instruction at `offset`. Returns the offset of the
/// next instruction.
pub fn disassemble_instruction(chunk: &Chunk, offset: usize, out: &mut String) -> usize {
    if offset >= chunk.code.len() {
        return offset;
    }
    out.push_str(&format!("{:04}  ", offset));

    // Print source line (only when it changes)
    if offset > 0 {
        let prev = chunk.line_at(offset.saturating_sub(1));
        let curr = chunk.line_at(offset);
        if prev == curr {
            out.push_str("   | ");
        } else {
            out.push_str(&format!("{:4} ", curr));
        }
    } else {
        out.push_str(&format!("{:4} ", chunk.line_at(0)));
    }

    let op = chunk.code[offset];
    let op_name = opcode_name(op);
    let operand_len = operand_bytes(op).unwrap_or(0);

    match operand_len {
        0 => {
            out.push_str(&format!("{}\n", op_name));
        }
        1 => {
            let val = chunk.code[offset + 1];
            out.push_str(&format!("{} {:>4}\n", op_name, val));
        }
        2 => {
            let val = read_u16(&chunk.code[offset + 1..offset + 3]);
            // For CONST, show the value
            if op == OP_CONST {
                if let Some(v) = chunk.constants.get(val as usize) {
                    out.push_str(&format!("{} {:>4}  ; {}\n", op_name, val, v));
                } else {
                    out.push_str(&format!("{} {:>4}  ; <invalid>\n", op_name, val));
                }
            } else if op == OP_JUMP || op == OP_JUMP_IF_FALSE || op == OP_JUMP_IF_TRUE || op == OP_POP_JUMP_IF_FALSE {
                let target = offset as i32 + 3 + val as i32;
                out.push_str(&format!("{} {:>4}  -> {}\n", op_name, val, target));
            } else if op == OP_LOOP {
                let target = offset as i32 + 3 - val as i32;
                out.push_str(&format!("{} {:>4}  -> {}\n", op_name, val, target));
            } else if op == OP_LOAD_GLOBAL || op == OP_STORE_GLOBAL || op == OP_DEFINE_GLOBAL || op == OP_MEMBER || op == OP_STORE_MEMBER {
                if let Some(Value::String(s)) = chunk.constants.get(val as usize) {
                    out.push_str(&format!("{} {:>4}  ; {}\n", op_name, val, s));
                } else {
                    out.push_str(&format!("{} {:>4}\n", op_name, val));
                }
            } else {
                out.push_str(&format!("{} {:>4}\n", op_name, val));
            }
        }
        _ => {
            out.push_str(&format!("{}\n", op_name));
        }
    }

    offset + 1 + operand_len
}

// ============================================================================
// Helpers
// ============================================================================

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => a == b,
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Null, Value::Null) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_builder_empty() {
        let b = ChunkBuilder::new();
        let chunk = b.finish();
        assert!(chunk.code.is_empty());
        assert!(chunk.constants.is_empty());
    }

    #[test]
    fn test_chunk_builder_constants() {
        let mut b = ChunkBuilder::new();
        let idx = b.add_constant(Value::Int(42));
        assert_eq!(idx, 0);
        // Dedup
        let idx2 = b.add_constant(Value::Int(42));
        assert_eq!(idx2, 0);
        let idx3 = b.add_constant(Value::String("hello".to_string()));
        assert_eq!(idx3, 1);
    }

    #[test]
    fn test_chunk_builder_simple_emit() {
        let mut b = ChunkBuilder::new();
        b.emit_op_u16(OP_CONST, 0);
        b.emit_op(OP_RETURN);
        let chunk = b.finish();
        assert_eq!(chunk.code.len(), 4);
        assert_eq!(chunk.code[0], OP_CONST);
        assert_eq!(read_u16(&chunk.code[1..3]), 0);
        assert_eq!(chunk.code[3], OP_RETURN);
    }

    #[test]
    fn test_chunk_builder_jump_backpatch() {
        let mut b = ChunkBuilder::new();
        let jump_label = b.new_label();

        b.set_line(1);
        b.emit_op(OP_NULL); // sentinel
        b.emit_jump(OP_JUMP_IF_FALSE, jump_label);
        b.emit_op_u16(OP_CONST, 0); // then-branch
        b.emit_op(OP_RETURN);
        b.place_label(jump_label);
        b.emit_op(OP_NULL);
        b.emit_op(OP_RETURN);

        let chunk = b.finish();
        // OP_NULL (1) + OP_JUMP_IF_FALSE(3) + OP_CONST(3) + OP_RETURN(1) + OP_NULL(1) + OP_RETURN(1)
        assert_eq!(chunk.code.len(), 10);
        // Jump should point past the then-branch to OP_NULL+OP_RETURN
        let jump_op = chunk.code[1];
        assert_eq!(jump_op, OP_JUMP_IF_FALSE);
        let jump_offset = read_i16(&chunk.code[2..4]);
        // Tracing: opcode at idx 0 (NULL), opcode at idx 1 (JUMP_IF_FALSE, operand at 2-3),
        // then-branch at 4-7 (CONST 3 + RETURN 1), label at pos=8.
        // patch.offset = 2 (ip+1). offset = 8 - 2 - 2 = 4.
        // target = 1 + 3 + 4 = 8.
        assert_eq!(jump_offset, 4);
    }

    #[test]
    fn test_disassemble() {
        let mut b = ChunkBuilder::new();
        let c = b.add_constant(Value::Int(42));
        b.set_line(1);
        b.emit_op_u16(OP_CONST, c);
        b.emit_op(OP_RETURN);
        let chunk = b.finish();
        let d = disassemble(&chunk, "test");
        assert!(d.contains("CONST"));
        assert!(d.contains("42"));
        assert!(d.contains("RETURN"));
    }
}
