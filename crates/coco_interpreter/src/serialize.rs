//! Bytecode artifact serialization — the `.cb` (coco-build) format.
//!
//! A `Chunk` is serialized to a self-contained binary blob and written to a
//! `.cb` file by `coco build`. `coco run prog.cb` deserializes it back into a
//! `Chunk` and hands it to the VM, skipping parse + compile.
//!
//! ## Format
//!
//! ```text
//! magic:       4 bytes   b"COCO"
//! version:     u16       (little-endian; currently 1)
//! code_len:    u32       (little-endian)
//! code:        [u8; code_len]
//! const_count: u32       (little-endian)
//! constants:   [Value; const_count]   (each tagged, see below)
//! line_count:  u32       (little-endian)
//! lines:       [(u32 code_offset, u32 source_line); line_count]
//! ```
//!
//! A nested `FnObj` constant carries its own chunk body inline (code,
//! constants, lines) using the same layout as above but without the
//! magic/version preamble.
//!
//! ## Value encoding (1 tag byte + payload)
//!
//! | tag | variant            | payload                                              |
//! |-----|--------------------|------------------------------------------------------|
//! | 0x01| Int (BigInt)       | u8 sign (0=non-neg,1=neg), u32 limb-count, [u32; n]  |
//! | 0x02| Float (f64)        | 8 bytes IEEE-754 LE                                  |
//! | 0x03| String             | u32 byte-len, UTF-8 bytes                            |
//! | 0x04| Null               | (none)                                               |
//! | 0x05| FnObj              | u8 is_async, u16 arity, String name, nested Chunk    |
//!
//! Only these `Value` variants ever enter a `Chunk`'s constant pool (the
//! compiler builds List/Map/Channel/etc. at runtime via opcodes). Any other
//! variant is rejected at serialize time to fail loudly rather than silently
//! producing a corrupt artifact.

use num_bigint::BigInt;
use num_traits::{Signed, Zero};

use crate::ir::{Chunk, FnObj};
use crate::value::Value;

// --- Format constants ------------------------------------------------------

/// File magic: `b"COCO"`.
const MAGIC: &[u8; 4] = b"COCO";

/// Current serialization format version. Bump on any breaking change to the
/// on-disk layout; `deserialize_chunk` rejects mismatched versions.
const FORMAT_VERSION: u16 = 1;

// Value tag bytes.
const TAG_INT: u8 = 0x01;
const TAG_FLOAT: u8 = 0x02;
const TAG_STRING: u8 = 0x03;
const TAG_NULL: u8 = 0x04;
const TAG_FNOBJ: u8 = 0x05;

// --- Errors ----------------------------------------------------------------

/// An error produced while deserializing a `.cb` artifact.
#[derive(Debug, Clone)]
pub struct DeserializeError {
    /// Human-readable description of what went wrong.
    pub message: String,
    /// Byte offset in the input where the error was detected, if known.
    pub offset: Option<usize>,
}

impl DeserializeError {
    fn at(offset: usize, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: Some(offset),
        }
    }

    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            offset: None,
        }
    }
}

impl std::fmt::Display for DeserializeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.offset {
            Some(off) => write!(f, "cb deserialize error at byte {}: {}", off, self.message),
            None => write!(f, "cb deserialize error: {}", self.message),
        }
    }
}

impl std::error::Error for DeserializeError {}

// --- Public API ------------------------------------------------------------

/// Serialize a `Chunk` to the `.cb` binary format.
///
/// Returns the full artifact bytes (magic, version, code, constants, lines).
///
/// Returns an error if the constant pool contains a `Value` variant that is
/// not representable on disk (see the module docs for the supported set). The
/// compiler never emits such variants, so this is a defensive guard against
/// silent corruption.
pub fn serialize_chunk(chunk: &Chunk) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(16 + chunk.code.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    write_chunk_body(chunk, &mut out)?;
    Ok(out)
}

/// Deserialize a `.cb` artifact back into a `Chunk`.
///
/// After decoding the bytes, the chunk is passed through `verify_chunk` to
/// check that the bytecode is safe to execute (valid opcodes, in-range
/// constant indices, jumps landing on instruction boundaries). This makes
/// `deserialize_chunk` the security boundary between an untrusted `.cb` file
/// and the VM, which trusts the chunk.
pub fn deserialize_chunk(bytes: &[u8]) -> Result<Chunk, DeserializeError> {
    let mut r = Reader::new(bytes);
    let magic = r.take(4)?;
    if magic != MAGIC {
        return Err(DeserializeError::at(0, "not a .cb artifact (bad magic)"));
    }
    let version = r.read_u16()?;
    if version != FORMAT_VERSION {
        return Err(DeserializeError::new(format!(
            "unsupported .cb format version {}: this build supports version {}. \
             Rebuild the artifact with a matching `coco`.",
            version, FORMAT_VERSION
        )));
    }
    let chunk = read_chunk_body(&mut r)?;
    crate::verify::verify_chunk(&chunk)
        .map_err(|e| DeserializeError::new(format!("bytecode verification failed: {}", e)))?;
    Ok(chunk)
}

// --- Chunk body (shared by top-level and nested FnObj chunks) --------------

fn write_chunk_body(chunk: &Chunk, out: &mut Vec<u8>) -> Result<(), String> {
    // code
    write_u32(out, chunk.code.len() as u32);
    out.extend_from_slice(&chunk.code);

    // constants
    write_u32(out, chunk.constants.len() as u32);
    for value in &chunk.constants {
        serialize_value(value, out)?;
    }

    // lines
    write_u32(out, chunk.lines.len() as u32);
    for &(offset, line) in &chunk.lines {
        write_u32(out, offset as u32);
        write_u32(out, line as u32);
    }
    Ok(())
}

fn read_chunk_body(r: &mut Reader) -> Result<Chunk, DeserializeError> {
    let code_len = r.read_u32()? as usize;
    let code = r.take(code_len)?.to_vec();

    let const_count = r.read_u32()? as usize;
    let mut constants = Vec::with_capacity(const_count);
    for _ in 0..const_count {
        constants.push(deserialize_value(r)?);
    }

    let line_count = r.read_u32()? as usize;
    let mut lines = Vec::with_capacity(line_count);
    for _ in 0..line_count {
        let offset = r.read_u32()? as usize;
        let line = r.read_u32()? as usize;
        lines.push((offset, line));
    }

    Ok(Chunk {
        code,
        constants,
        lines,
    })
}

// --- Value (de)serialization -----------------------------------------------

fn serialize_value(value: &Value, out: &mut Vec<u8>) -> Result<(), String> {
    match value {
        // Int64 serializes through the same TAG_INT BigInt wire format (promoted
        // to BigInt first), so the on-disk format stays compatible and the
        // deserializer needs no separate tag. The result deserializes as
        // `Value::Int` (BigInt); the VM's `normalize_int` re-wraps to Int64
        // at runtime when the value fits.
        Value::Int64(n) => {
            out.push(TAG_INT);
            let big = BigInt::from(*n);
            let (neg, limbs) = bigint_to_le_limbs(&big);
            out.push(if neg { 1 } else { 0 });
            write_u32(out, limbs.len() as u32);
            for limb in &limbs {
                out.extend_from_slice(&limb.to_le_bytes());
            }
        }
        Value::Int(n) => {
            out.push(TAG_INT);
            // Sign-magnitude with u32 limbs, little-endian per limb.
            let (neg, limbs) = bigint_to_le_limbs(n);
            out.push(if neg { 1 } else { 0 });
            write_u32(out, limbs.len() as u32);
            for limb in &limbs {
                out.extend_from_slice(&limb.to_le_bytes());
            }
        }
        Value::Float(f) => {
            out.push(TAG_FLOAT);
            out.extend_from_slice(&f.to_le_bytes());
        }
        Value::String(s) => {
            out.push(TAG_STRING);
            write_u32(out, s.len() as u32);
            out.extend_from_slice(s.as_bytes());
        }
        Value::Null => {
            out.push(TAG_NULL);
        }
        Value::FnObj(fo) => {
            out.push(TAG_FNOBJ);
            out.push(if fo.is_async { 1 } else { 0 });
            let arity = u16::try_from(fo.arity).map_err(|_| {
                format!("FnObj arity {} exceeds u16 during serialization", fo.arity)
            })?;
            out.extend_from_slice(&arity.to_le_bytes());
            write_str(&fo.name, out);
            write_chunk_body(&fo.chunk, out)?;
        }
        // These variants never appear in the constant pool. If one ever does,
        // fail loudly rather than silently dropping it.
        other => {
            return Err(format!(
                "cannot serialize Value variant {:?} into a .cb constant pool \
                 (only Int/Float/String/Null/FnObj are representable)",
                other
            ));
        }
    }
    Ok(())
}

fn deserialize_value(r: &mut Reader) -> Result<Value, DeserializeError> {
    let tag = r.read_u8()?;
    match tag {
        TAG_INT => {
            let sign_byte = r.read_u8()?;
            let limb_count = r.read_u32()? as usize;
            let mut limbs = Vec::with_capacity(limb_count);
            for _ in 0..limb_count {
                limbs.push(r.read_u32()?);
            }
            let mut n = bigint_from_le_limbs(&limbs);
            if sign_byte != 0 {
                n = -n;
            }
            Ok(Value::Int(n))
        }
        TAG_FLOAT => {
            let bytes = r.take(8)?;
            let mut arr = [0u8; 8];
            arr.copy_from_slice(bytes);
            Ok(Value::Float(f64::from_le_bytes(arr)))
        }
        TAG_STRING => {
            let len = r.read_u32()? as usize;
            let bytes = r.take(len)?;
            let s = std::str::from_utf8(bytes).map_err(|e| {
                DeserializeError::at(r.pos, format!("invalid UTF-8 in string constant: {}", e))
            })?;
            Ok(Value::String(s.to_string()))
        }
        TAG_NULL => Ok(Value::Null),
        TAG_FNOBJ => {
            let is_async = r.read_u8()? != 0;
            let arity = r.read_u16()? as usize;
            let name = read_str(r)?;
            let chunk = read_chunk_body(r)?;
            Ok(Value::FnObj(FnObj {
                name,
                arity,
                chunk,
                is_async,
            }))
        }
        other => Err(DeserializeError::at(
            r.pos,
            format!("unknown constant tag 0x{:02x}", other),
        )),
    }
}

fn write_str(s: &str, out: &mut Vec<u8>) {
    write_u32(out, s.len() as u32);
    out.extend_from_slice(s.as_bytes());
}

fn read_str(r: &mut Reader) -> Result<String, DeserializeError> {
    let len = r.read_u32()? as usize;
    let bytes = r.take(len)?;
    let s = std::str::from_utf8(bytes)
        .map_err(|e| DeserializeError::at(r.pos, format!("invalid UTF-8: {}", e)))?;
    Ok(s.to_string())
}

// --- BigInt helpers --------------------------------------------------------

/// Split a `BigInt` into a sign flag and little-endian u32 magnitude limbs.
/// Zero is represented as sign=0 with zero limbs.
fn bigint_to_le_limbs(n: &BigInt) -> (bool, Vec<u32>) {
    if n.is_zero() {
        return (false, Vec::new());
    }
    let neg = n.is_negative();
    // iter_u32_digits yields magnitude limbs from least to most significant.
    let limbs: Vec<u32> = n.iter_u32_digits().collect();
    (neg, limbs)
}

/// Reconstruct a `BigInt` from little-endian u32 limbs (magnitude only).
fn bigint_from_le_limbs(limbs: &[u32]) -> BigInt {
    if limbs.is_empty() {
        return BigInt::from(0);
    }
    let mut n = BigInt::from(0u64);
    let mut shift: u64 = 0;
    for &limb in limbs {
        // Shift-and-add keeps the result exact for arbitrarily large values.
        let part = BigInt::from(limb) << shift;
        n += part;
        shift += 32;
    }
    n
}

// --- Output / input primitives ---------------------------------------------

fn write_u32(out: &mut Vec<u8>, val: u32) {
    out.extend_from_slice(&val.to_le_bytes());
}

/// A cursor over the input bytes that tracks position and bounds-checks reads.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.pos)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], DeserializeError> {
        if self.remaining() < n {
            return Err(DeserializeError::at(
                self.pos,
                format!(
                    "unexpected end of input: wanted {} bytes, have {}",
                    n,
                    self.remaining()
                ),
            ));
        }
        let slice = &self.bytes[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    fn read_u8(&mut self) -> Result<u8, DeserializeError> {
        Ok(self.take(1)?[0])
    }

    fn read_u16(&mut self) -> Result<u16, DeserializeError> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }

    fn read_u32(&mut self) -> Result<u32, DeserializeError> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::value_eq;
    use num_bigint::BigInt;

    /// Round-trip a single `Value` through serialize/deserialize.
    fn roundtrip_value(v: &Value) -> Value {
        let mut buf = Vec::new();
        serialize_value(v, &mut buf).expect("serialize");
        let mut r = Reader::new(&buf);
        deserialize_value(&mut r).expect("deserialize")
    }

    fn assert_value_eq(a: &Value, b: &Value) {
        assert!(
            value_eq(a, b),
            "values not equal after round-trip:\n  got:      {:?}\n  expected: {:?}",
            a,
            b
        );
    }

    #[test]
    fn roundtrip_int_small() {
        let v = Value::Int(BigInt::from(42));
        assert_value_eq(&roundtrip_value(&v), &v);
    }

    #[test]
    fn roundtrip_int_negative() {
        let v = Value::Int(BigInt::from(-12345));
        assert_value_eq(&roundtrip_value(&v), &v);
    }

    #[test]
    fn roundtrip_int_huge() {
        // Exceeds i64 — must round-trip exactly through bignum limbs.
        let n = BigInt::parse_bytes(b"123456789012345678901234567890", 10).unwrap();
        let v = Value::Int(n);
        assert_value_eq(&roundtrip_value(&v), &v);
    }

    #[test]
    fn roundtrip_int_zero() {
        let v = Value::Int(BigInt::from(0));
        assert_value_eq(&roundtrip_value(&v), &v);
    }

    #[test]
    fn roundtrip_float() {
        let v = Value::Float(3.141592653589793);
        assert_value_eq(&roundtrip_value(&v), &v);
    }

    #[test]
    fn roundtrip_float_special_values() {
        // NaN != NaN by normal equality; check the bit pattern round-trips.
        for f in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.0] {
            let back = roundtrip_value(&Value::Float(f));
            match back {
                Value::Float(g) => assert!(f.to_bits() == g.to_bits(), "bits differ for {}", f),
                _ => panic!("expected float"),
            }
        }
    }

    #[test]
    fn roundtrip_string() {
        let s = "héllo, 世界 🌍".to_string();
        let v = Value::String(s);
        assert_value_eq(&roundtrip_value(&v), &v);
    }

    #[test]
    fn roundtrip_empty_string() {
        let v = Value::String(String::new());
        assert_value_eq(&roundtrip_value(&v), &v);
    }

    #[test]
    fn roundtrip_null() {
        let v = Value::Null;
        assert_value_eq(&roundtrip_value(&v), &v);
    }

    #[test]
    fn roundtrip_fnobj() {
        let mut inner = Chunk::new();
        inner.code.extend_from_slice(&[1, 51]); // OP_NULL, OP_RETURN
        inner.constants.push(Value::Int(BigInt::from(7)));
        inner.constants.push(Value::String("name".to_string()));
        inner.lines.push((0, 10));
        let fo = FnObj {
            name: "greet".to_string(),
            arity: 2,
            chunk: inner,
            is_async: true,
        };
        let v = Value::FnObj(fo);
        let back = roundtrip_value(&v);
        match back {
            Value::FnObj(fo) => {
                assert_eq!(fo.name, "greet");
                assert_eq!(fo.arity, 2);
                assert!(fo.is_async);
                assert_eq!(fo.chunk.code, vec![1, 51]);
                assert_eq!(fo.chunk.constants.len(), 2);
                assert!(value_eq(&fo.chunk.constants[0], &Value::Int(BigInt::from(7))));
                assert_eq!(fo.chunk.lines, vec![(0, 10)]);
            }
            _ => panic!("expected FnObj"),
        }
    }

    #[test]
    fn roundtrip_fnobj_nested_fnobj() {
        // A FnObj whose constant pool contains another FnObj (recursive).
        let leaf = FnObj {
            name: "leaf".to_string(),
            arity: 0,
            chunk: {
                let mut c = Chunk::new();
                c.code.push(51); // OP_RETURN
                c
            },
            is_async: false,
        };
        let mut outer_chunk = Chunk::new();
        outer_chunk.code.extend_from_slice(&[0, 0, 0, 51]); // CONST 0, RETURN
        outer_chunk.constants.push(Value::FnObj(leaf));
        let outer = FnObj {
            name: "outer".to_string(),
            arity: 1,
            chunk: outer_chunk,
            is_async: false,
        };
        let v = Value::FnObj(outer);
        let back = roundtrip_value(&v);
        match back {
            Value::FnObj(fo) => {
                assert_eq!(fo.name, "outer");
                assert_eq!(fo.chunk.constants.len(), 1);
                match &fo.chunk.constants[0] {
                    Value::FnObj(inner) => assert_eq!(inner.name, "leaf"),
                    other => panic!("expected nested FnObj, got {:?}", other),
                }
            }
            _ => panic!("expected FnObj"),
        }
    }

    #[test]
    fn roundtrip_chunk_full() {
        let mut chunk = Chunk::new();
        chunk.code.extend_from_slice(&[0, 0, 0, 51]); // CONST 0, RETURN
        chunk.constants.push(Value::Int(BigInt::from(99)));
        chunk.constants.push(Value::String("hi".to_string()));
        chunk.lines.push((0, 1));
        chunk.lines.push((2, 2));

        let bytes = serialize_chunk(&chunk).expect("serialize");
        let back = deserialize_chunk(&bytes).expect("deserialize");
        assert_eq!(back.code, chunk.code);
        assert_eq!(back.constants.len(), 2);
        assert!(value_eq(&back.constants[0], &Value::Int(BigInt::from(99))));
        assert!(value_eq(&back.constants[1], &Value::String("hi".to_string())));
        assert_eq!(back.lines, chunk.lines);
    }

    #[test]
    fn reject_bad_magic() {
        let err = deserialize_chunk(b"NOPE\x01\x00\x00\x00\x00\x00").unwrap_err();
        assert!(err.message.contains("bad magic") || err.message.contains("not a .cb"));
    }

    #[test]
    fn reject_truncated() {
        // Magic + version but no code length.
        let err = deserialize_chunk(b"COCO\x01\x00").unwrap_err();
        assert!(err.message.contains("end of input"));
    }

    #[test]
    fn reject_future_version() {
        let mut buf = Vec::new();
        buf.extend_from_slice(MAGIC);
        buf.extend_from_slice(&999u16.to_le_bytes()); // future version
        write_u32(&mut buf, 0); // code_len = 0
        write_u32(&mut buf, 0); // const_count = 0
        write_u32(&mut buf, 0); // line_count = 0
        let err = deserialize_chunk(&buf).unwrap_err();
        assert!(err.message.contains("unsupported .cb format version"));
    }

    #[test]
    fn reject_unserializable_value() {
        // List never appears in the constant pool; serialize must refuse it.
        let v = Value::List(std::sync::Arc::new(coco_gc::CoW::new(vec![])));
        let mut buf = Vec::new();
        let err = serialize_value(&v, &mut buf).unwrap_err();
        assert!(err.contains("cannot serialize"));
    }

    #[test]
    fn bigint_helpers_roundtrip() {
        for n in [
            BigInt::from(0),
            BigInt::from(1),
            BigInt::from(-1),
            BigInt::from(u32::MAX as u64),
            BigInt::from(i64::MAX) + 1u32,
            BigInt::parse_bytes(b"99999999999999999999999999", 10).unwrap(),
            -BigInt::parse_bytes(b"99999999999999999999999999", 10).unwrap(),
        ] {
            let (neg, limbs) = bigint_to_le_limbs(&n);
            let back = bigint_from_le_limbs(&limbs);
            let reconstructed = if neg { -back } else { back };
            assert_eq!(reconstructed, n, "round-trip failed for {}", n);
        }
    }
}
