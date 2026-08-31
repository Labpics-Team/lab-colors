//! Integration test harness for WASM-01 round-trip verification.
//!
//! Covers the public `program_wire` API surface that the WASM binding consumes:
//! compilation determinism, error classification, and session lifecycle.
//! Uses only public API; canonical bytes are embedded from the in-crate fixture.

use labcolors_core::Srgb8;
use labcolors_core::program_wire::{
    ProgramScenarioV1, ProgramWireCheckErrorV1, check_program_wire_v1, compile_program_wire_v1,
};

/// Canonical reference bytes extracted from the in-crate test fixture.
/// Minimal valid program wire that passes both decode and compile.
const CANONICAL_BYTES: &[u8] = &[
    0x4c, 0x43, 0x50, 0x57, 0x01, 0x00, 0xa7, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x0b, 0x00,
    0x00, 0x00, 0x14, 0x14, 0x14, 0x01, 0x00, 0x00, 0x00, 0x15, 0x00, 0x00, 0x00, 0x01, 0x0b, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x1f, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x29, 0x00, 0x00, 0x00, 0x01, 0x15,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x33, 0x00, 0x00, 0x00, 0x01, 0x1f, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x3d, 0x00, 0x00, 0x00, 0x29, 0x00, 0x00, 0x00, 0x33, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x50, 0x40, 0x9a, 0x99, 0x99, 0x99, 0x99, 0x99, 0xc9, 0x3f,
    0x01, 0x01, 0x00, 0x00, 0x00, 0x47, 0x00, 0x00, 0x00, 0x3d, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x47, 0x00, 0x00, 0x00, 0x3d, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x51, 0x00, 0x00,
    0x00, 0x09, 0x3d, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x5b,
    0x00, 0x00, 0x00, 0x29, 0x00, 0x00, 0x00,
];

#[test]
fn compile_program_wire_is_deterministic() {
    let first = compile_program_wire_v1(CANONICAL_BYTES).expect("first compile");
    let second = compile_program_wire_v1(CANONICAL_BYTES).expect("second compile");
    assert_eq!(
        first.content_identity(),
        second.content_identity(),
        "identical inputs must yield identical content identity",
    );
}

#[test]
fn corrupted_bytes_are_rejected_without_panic() {
    let mut bytes = CANONICAL_BYTES.to_vec();
    bytes[0] ^= 0xFF;
    let err = check_program_wire_v1(&bytes).expect_err("corrupted bytes must fail");
    assert!(
        matches!(err, ProgramWireCheckErrorV1::Wire { .. }),
        "expected Wire error, got {err:?}",
    );
}

#[test]
fn empty_input_is_rejected_as_wire_error() {
    let err = check_program_wire_v1(&[]).expect_err("empty input must fail");
    assert!(
        matches!(err, ProgramWireCheckErrorV1::Wire { .. }),
        "expected Wire error for empty slice, got {err:?}",
    );
}

#[test]
fn compiled_program_instantiates_and_updates() {
    let program = compile_program_wire_v1(CANONICAL_BYTES).expect("compile");
    let mut session = program.instantiate(1).expect("instantiate");

    let scenarios = [ProgramScenarioV1::new(
        1,
        vec![Srgb8::new([0xAA, 0xBB, 0xCC])],
    )];
    let snapshot = session
        .update_observed(1, &scenarios)
        .expect("observed update");

    assert_eq!(
        snapshot.outputs().len(),
        1,
        "canonical program must produce exactly one output",
    );
    let output = &snapshot.outputs()[0];
    assert_eq!(
        output.slot(),
        91,
        "output slot must match canonical wire binding"
    );
    assert_eq!(
        output.source(),
        Srgb8::new([20, 20, 20]),
        "output source must be the canonical sRGB8 value",
    );
    assert!(
        (output.opacity() - 1.0).abs() < f64::EPSILON,
        "output opacity must be exactly 1.0",
    );
}

#[test]
fn check_returns_32_byte_identity_for_valid_bytes() {
    let identity = check_program_wire_v1(CANONICAL_BYTES).expect("valid bytes must pass check");
    const GOLDEN_IDENTITY: [u8; 32] = [
        47, 72, 4, 222, 193, 177, 86, 197, 157, 173, 185, 8, 83, 214, 163, 22, 246, 136, 146, 44,
        124, 198, 145, 252, 2, 124, 49, 217, 37, 221, 139, 199,
    ];
    assert_eq!(
        identity, GOLDEN_IDENTITY,
        "identity must match the golden digest for CANONICAL_BYTES",
    );
}

#[test]
fn truncated_bytes_are_rejected_as_wire_error() {
    let truncated = &CANONICAL_BYTES[..CANONICAL_BYTES.len() / 2];
    let err = check_program_wire_v1(truncated).expect_err("truncated bytes must fail");
    assert!(
        matches!(err, ProgramWireCheckErrorV1::Wire { .. }),
        "expected Wire error for truncated input, got {err:?}",
    );
}
