// ============================================================================
// Optional numeric advisory shim
// ============================================================================
//
// This module is compiled only when the `apc-advisory` Cargo feature is enabled.
// It gives the VM cheap, non-breaking hooks into `coco_num` so we can record
// or enforce APC-style checks without changing the default runtime behavior.
//
// Default user builds keep this module disabled.

#[cfg(feature = "apc-advisory")]
pub fn opcode_entered(_op: &str) {
    let _ = opcode_entered_inner();
}

#[cfg(feature = "apc-advisory")]
fn opcode_entered_inner() -> Result<(), coco_num::APCError> {
    let _ = coco_num::Tier::T0;
    Ok(())
}

#[cfg(not(feature = "apc-advisory"))]
pub fn opcode_entered(_op: &str) {}
