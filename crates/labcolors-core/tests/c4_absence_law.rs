//! Compile-fail and runtime tests enforcing C4 absence-law at the registry level.
//! These tests verify that legacy WCAG solver artifacts cannot leak through
//! the evaluator registry surface, and that the admission gate correctly
//! rejects non-compliant profiles at runtime.
//!
//! PORTABILITY NOTE: All string-absence searches use include_str! over source
//! files rather than shelling out to grep. This ensures tests pass on Windows
//! CI without requiring GNU coreutils.

// ---------------------------------------------------------------------------
// §5.1 String-Absence Tests (Compile-Time Invariants)
// ---------------------------------------------------------------------------

const REGISTRY_MOD: &str = include_str!("../src/evaluator_registry/mod.rs");

#[test]
fn no_wcag_module_in_registry() {
    assert!(
        !REGISTRY_MOD.contains("mod wcag;"),
        "C4 VIOLATION: 'mod wcag;' found in evaluator_registry/mod.rs"
    );
}

const CONSTRAINTS_MOD: &str = include_str!("../src/constraints/mod.rs");
const CONSTRAINTS_WCAG22: &str = include_str!("../src/constraints/wcag22.rs");
const CONSTRAINTS_EXACT: &str = include_str!("../src/constraints/exact.rs");
const CONSTRAINTS_RELATION: &str = include_str!("../src/constraints/relation.rs");

#[test]
fn no_wcag_ratio_type() {
    for (name, content) in &[
        ("constraints/mod.rs", CONSTRAINTS_MOD),
        ("constraints/wcag22.rs", CONSTRAINTS_WCAG22),
        ("constraints/exact.rs", CONSTRAINTS_EXACT),
        ("constraints/relation.rs", CONSTRAINTS_RELATION),
    ] {
        assert!(
            !content.contains("wcagRatio"),
            "C4 VIOLATION: wcagRatio type found in {}",
            name
        );
    }
}

#[test]
fn no_feasibility_dto() {
    assert!(
        !REGISTRY_MOD.contains("FeasibilityDto") && !REGISTRY_MOD.contains("feasibility_dto"),
        "C4 VIOLATION: feasibility DTO found in evaluator_registry/mod.rs"
    );
}

#[test]
fn no_evaluator_transport_family() {
    assert!(
        !REGISTRY_MOD.contains("TransportFamily") && !REGISTRY_MOD.contains("transport_family"),
        "C4 VIOLATION: evaluator-specific transport family found in evaluator_registry/mod.rs"
    );
}

// §5.2 Admission gate behavioral tests live in-crate at
// crates/labcolors-core/src/evaluator_registry/admission.rs because the
// evaluator_registry module is pub(crate). Integration tests cannot access
// private modules; the in-crate #[cfg(test)] block provides equivalent
// coverage for all rejection variants and the positive control.
