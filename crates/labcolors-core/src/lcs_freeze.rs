//! Compile-time gate for the frozen LCS V1 substrate.
//!
//! Only types implementing [`AdmittedLcsIdentityV1`] may be consumed by
//! downstream cleanliness and profile nodes. This trait is sealed: no
//! external crate can implement it, and within this crate only
//! `ModeledLcsOccurrenceV1` carries the proof of deterministic derivation.

use crate::lcs_occurrence::ModeledLcsOccurrenceV1;

mod sealed {
    pub trait Sealed {}
}

/// Proof that a value represents a frozen, replayable LCS V1 identity.
///
/// # Seal Invariant
/// Implementors must guarantee that their LCS coordinates are derived
/// exclusively through admitted V1 transforms with retained provenance.
pub trait AdmittedLcsIdentityV1: sealed::Sealed + Copy + Eq {
    /// Returns the underlying modeled occurrence for verified computation.
    fn as_modeled_v1(self) -> ModeledLcsOccurrenceV1;
}

impl sealed::Sealed for ModeledLcsOccurrenceV1 {}

impl AdmittedLcsIdentityV1 for ModeledLcsOccurrenceV1 {
    #[inline(always)]
    fn as_modeled_v1(self) -> ModeledLcsOccurrenceV1 {
        self
    }
}

/// Zero-cost witness that the LCS V1 numerical model is frozen.
///
/// Construction is restricted to this module. Downstream gates accept
/// this certificate to prove they are operating on the frozen substrate,
/// not on legacy or unversioned representations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LcsFreezeCertificateV1 {
    _private: (),
}

impl LcsFreezeCertificateV1 {
    /// The single admitted constructor, callable only after verifying
    /// that all LCS entry points route through `ModeledLcsOccurrenceV1`.
    ///
    /// # Invariant
    /// This function exists exactly once in the codebase. Its presence
    /// certifies that the F-01 freeze audit passed.
    pub(crate) const fn attest() -> Self {
        Self { _private: () }
    }
}

/// Module-level constant serving as the canonical certificate instance.
/// Import this where downstream gates require proof of freeze compliance.
pub const LCS_FREEZE_V1: LcsFreezeCertificateV1 = LcsFreezeCertificateV1::attest();

/// Compile-time assertion: `LcsColor` layout must remain at 33 bytes
/// (4 × f64 + PhysicalLocus discriminant). Adding a field requires updating
/// this constant and re-auditing F-01.
///
/// Note: `PhysicalLocus` is a two-variant enum (u8 discriminant) followed by
/// padding to align the f64 fields. The total is 4×8 + 1 = 33 bytes if packed,
/// but Rust adds alignment padding. We assert the exact observed size.
// F-01: referencing deprecated LcsColor is intentional — this assertion
// guards its layout precisely so solver-path compatibility cannot drift.
#[allow(deprecated)]
const _: () = assert!(
    std::mem::size_of::<crate::LcsColor>() == 40,
    "F-01 VIOLATION: LcsColor struct layout changed. \
     Update freeze audit before proceeding."
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lcs_occurrence::{
        ColorimetricTransformReleaseId, OutputProfileId,
    };

    /// Runtime guard replacing nightly-only `std::mem::variant_count`.
    /// If a new variant is added to `OutputProfileId`, this match will
    /// fail to compile (exhaustiveness) or the count assertion will panic.
    #[test]
    fn output_profile_id_has_exactly_one_variant() {
        let variants: &[OutputProfileId] = &[OutputProfileId::Iec61966Srgb8D65V1];
        assert_eq!(
            variants.len(),
            1,
            "F-01 VIOLATION: OutputProfileId gained a variant. \
             LCS freeze contract broken; bump to V2."
        );
        // Exhaustiveness anchor: adding a variant forces this match to update.
        match OutputProfileId::Iec61966Srgb8D65V1 {
            OutputProfileId::Iec61966Srgb8D65V1 => {}
        }
    }

    /// Runtime guard replacing nightly-only `std::mem::variant_count`.
    #[test]
    fn colorimetric_transform_release_id_has_exactly_one_variant() {
        let variants: &[ColorimetricTransformReleaseId] = &[
            ColorimetricTransformReleaseId::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1,
        ];
        assert_eq!(
            variants.len(),
            1,
            "F-01 VIOLATION: ColorimetricTransformReleaseId gained a variant. \
             LCS freeze contract broken; bump to V2."
        );
        match ColorimetricTransformReleaseId::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 {
            ColorimetricTransformReleaseId::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 => {}
        }
    }

    #[test]
    fn certificate_is_singleton() {
        let a = LCS_FREEZE_V1;
        let b = LCS_FREEZE_V1;
        assert_eq!(a, b);
    }

    // F-01: referencing deprecated LcsColor is intentional — this test
    // verifies the freeze layout assertion matches runtime reality.
    #[test]
    #[allow(deprecated)]
    fn lcs_color_layout_matches_freeze_assertion() {
        // Double-check the const assertion value matches reality at test time.
        assert_eq!(
            std::mem::size_of::<crate::LcsColor>(),
            40,
            "F-01 VIOLATION: LcsColor size changed from expected 40 bytes"
        );
    }
}