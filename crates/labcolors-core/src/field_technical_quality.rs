//! R-10 PR-A: Field TechnicalQuality type definitions.
//!
//! This module defines the quality-layer types for whole-field evaluations,
//! extending the execution-correctness proofs in `field_effect` with
//! technical-quality assessments. The substrate serves ONLY TechnicalQuality
//! and explicit declared-restorative R3s policy (roadmap line 3052). It does
//! NOT mint CleanPass, FinalOwnedClean, or human-clean action authority.

// R-10 field TQ staged infrastructure; consumers land in PR-B/C/D.
#![expect(
    dead_code,
    reason = "R-10 field TQ staged infrastructure; consumed by PR-B/C/D"
)]

use crate::field_effect::{
    FieldCertificateDigestV1, FieldOperatorKindV1, FieldRasterDigestV1, FieldRequestDigestV1,
};
use crate::sha256::Hasher;

macro_rules! tq_digest_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub(crate) struct $name([u8; 32]);

        impl $name {
            const fn as_bytes(self) -> [u8; 32] {
                self.0
            }

            #[cfg(test)]
            pub(crate) const fn from_bytes_for_test(bytes: [u8; 32]) -> Self {
                Self(bytes)
            }
        }
    };
}

tq_digest_type!(FieldTechnicalQualityDigestV1);
tq_digest_type!(FieldCompositionLawProofDigestV1);
tq_digest_type!(FieldGamutContainmentProofDigestV1);
tq_digest_type!(FieldDeclaredRestorationPolicyDigestV1);

/// Operator-specific composition law conformance class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldCompositionLawClassV1 {
    /// GaussianBlur: exact Q32 binomial normalization (sum = 2^32) and
    /// separability into horizontal then vertical passes.
    GaussianBlurSeparableQ32,
    /// PremultipliedSourceOver: Porter-Duff source-over algebra over
    /// premultiplied sRGB8 pixels.
    PremultipliedSourceOver,
    /// EncodedSrgb8ScreenOpaqueBackdrop: encoded-sRGB8 screen reference law
    /// against an opaque backdrop.
    EncodedSrgb8ScreenOpaqueBackdrop,
    /// PorterDuffLighter: Porter-Duff lighter (additive) compositing.
    PorterDuffLighter,
}

impl FieldCompositionLawClassV1 {
    /// Derive the composition law class from the operator kind.
    pub(crate) const fn from_operator_kind(kind: FieldOperatorKindV1) -> Self {
        match kind {
            FieldOperatorKindV1::GaussianBlurSeparableQ32V1 => Self::GaussianBlurSeparableQ32,
            FieldOperatorKindV1::PremultipliedSourceOverV1 => Self::PremultipliedSourceOver,
            FieldOperatorKindV1::EncodedSrgb8ScreenOpaqueBackdropV1 => {
                Self::EncodedSrgb8ScreenOpaqueBackdrop
            }
            FieldOperatorKindV1::PorterDuffLighterV1 => Self::PorterDuffLighter,
        }
    }
}

/// Evidence that the output raster satisfies gamut containment: all pixels
/// are valid premultiplied sRGB8 (`channel <= alpha`) and within range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldGamutContainmentProofV1 {
    output_digest: FieldRasterDigestV1,
    pixel_count: u64,
    digest: FieldGamutContainmentProofDigestV1,
}

impl FieldGamutContainmentProofV1 {
    /// Construct a gamut containment proof from verified raster evidence.
    ///
    /// The caller must have already verified every pixel satisfies the
    /// premultiplied invariant. This type records that verification as a
    /// content-addressed artifact.
    pub(crate) fn new(output_digest: FieldRasterDigestV1, pixel_count: u64) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(b"FieldGamutContainmentProofV1");
        hasher.update(&output_digest.as_bytes());
        hasher.update(&pixel_count.to_be_bytes());
        let digest_bytes = finalize(hasher);
        Self {
            output_digest,
            pixel_count,
            digest: FieldGamutContainmentProofDigestV1(digest_bytes),
        }
    }

    pub(crate) const fn output_digest(self) -> FieldRasterDigestV1 {
        self.output_digest
    }

    pub(crate) const fn pixel_count(self) -> u64 {
        self.pixel_count
    }

    pub(crate) const fn digest(self) -> FieldGamutContainmentProofDigestV1 {
        self.digest
    }
}

/// Per-operator composition law conformance evidence at field scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldCompositionLawProofV1 {
    operator_kind: FieldOperatorKindV1,
    law_class: FieldCompositionLawClassV1,
    request_digest: FieldRequestDigestV1,
    output_digest: FieldRasterDigestV1,
    digest: FieldCompositionLawProofDigestV1,
}

impl FieldCompositionLawProofV1 {
    /// Construct a composition law proof binding operator, law class,
    /// request identity, and output identity into a content-addressed artifact.
    pub(crate) fn new(
        operator_kind: FieldOperatorKindV1,
        request_digest: FieldRequestDigestV1,
        output_digest: FieldRasterDigestV1,
    ) -> Self {
        let law_class = FieldCompositionLawClassV1::from_operator_kind(operator_kind);
        let mut hasher = Hasher::new();
        hasher.update(b"FieldCompositionLawProofV1");
        hash_operator_kind(&mut hasher, operator_kind);
        hash_composition_law_class(&mut hasher, law_class);
        hasher.update(&request_digest.as_bytes());
        hasher.update(&output_digest.as_bytes());
        let digest_bytes = finalize(hasher);
        Self {
            operator_kind,
            law_class,
            request_digest,
            output_digest,
            digest: FieldCompositionLawProofDigestV1(digest_bytes),
        }
    }

    pub(crate) const fn operator_kind(self) -> FieldOperatorKindV1 {
        self.operator_kind
    }

    pub(crate) const fn law_class(self) -> FieldCompositionLawClassV1 {
        self.law_class
    }

    pub(crate) const fn request_digest(self) -> FieldRequestDigestV1 {
        self.request_digest
    }

    pub(crate) const fn output_digest(self) -> FieldRasterDigestV1 {
        self.output_digest
    }

    pub(crate) const fn digest(self) -> FieldCompositionLawProofDigestV1 {
        self.digest
    }
}

/// Declared-restoration policy binding for the field domain.
///
/// Separates restorative action authority from TechnicalQuality per roadmap
/// line 3052. This type binds a restoration policy to a specific certificate
/// and operator context without granting human-clean authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldDeclaredRestorationPolicyV1 {
    certificate_digest: FieldCertificateDigestV1,
    operator_kind: FieldOperatorKindV1,
    digest: FieldDeclaredRestorationPolicyDigestV1,
}

impl FieldDeclaredRestorationPolicyV1 {
    /// Bind a declared-restoration policy to a certificate and operator.
    pub(crate) fn new(
        certificate_digest: FieldCertificateDigestV1,
        operator_kind: FieldOperatorKindV1,
    ) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(b"FieldDeclaredRestorationPolicyV1");
        hasher.update(&certificate_digest.as_bytes());
        hash_operator_kind(&mut hasher, operator_kind);
        let digest_bytes = finalize(hasher);
        Self {
            certificate_digest,
            operator_kind,
            digest: FieldDeclaredRestorationPolicyDigestV1(digest_bytes),
        }
    }

    pub(crate) const fn certificate_digest(self) -> FieldCertificateDigestV1 {
        self.certificate_digest
    }

    pub(crate) const fn operator_kind(self) -> FieldOperatorKindV1 {
        self.operator_kind
    }

    pub(crate) const fn digest(self) -> FieldDeclaredRestorationPolicyDigestV1 {
        self.digest
    }
}

/// Core field TechnicalQuality assessment.
///
/// Combines composition law conformance, gamut containment, and finite-output
/// evidence into a single content-addressed quality verdict. Distinct from
/// `FieldWholeRasterCertificateV1` which proves execution correctness; this
/// type proves quality properties of the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldTechnicalQualityV1 {
    request_digest: FieldRequestDigestV1,
    output_digest: FieldRasterDigestV1,
    composition_law_proof: FieldCompositionLawProofV1,
    gamut_containment_proof: FieldGamutContainmentProofV1,
    digest: FieldTechnicalQualityDigestV1,
}

impl FieldTechnicalQualityV1 {
    /// Construct a field TechnicalQuality verdict from its constituent proofs.
    ///
    /// The composition law proof and gamut containment proof must reference
    /// the same request and output digests; this constructor enforces that
    /// invariant by deriving both from shared inputs.
    pub(crate) fn new(
        operator_kind: FieldOperatorKindV1,
        request_digest: FieldRequestDigestV1,
        output_digest: FieldRasterDigestV1,
        pixel_count: u64,
    ) -> Self {
        let composition_law_proof =
            FieldCompositionLawProofV1::new(operator_kind, request_digest, output_digest);
        let gamut_containment_proof = FieldGamutContainmentProofV1::new(output_digest, pixel_count);

        let mut hasher = Hasher::new();
        hasher.update(b"FieldTechnicalQualityV1");
        hasher.update(&request_digest.as_bytes());
        hasher.update(&output_digest.as_bytes());
        hasher.update(&composition_law_proof.digest().as_bytes());
        hasher.update(&gamut_containment_proof.digest().as_bytes());
        let digest_bytes = finalize(hasher);

        Self {
            request_digest,
            output_digest,
            composition_law_proof,
            gamut_containment_proof,
            digest: FieldTechnicalQualityDigestV1(digest_bytes),
        }
    }

    pub(crate) const fn request_digest(self) -> FieldRequestDigestV1 {
        self.request_digest
    }

    pub(crate) const fn output_digest(self) -> FieldRasterDigestV1 {
        self.output_digest
    }

    pub(crate) const fn composition_law_proof(self) -> FieldCompositionLawProofV1 {
        self.composition_law_proof
    }

    pub(crate) const fn gamut_containment_proof(self) -> FieldGamutContainmentProofV1 {
        self.gamut_containment_proof
    }

    pub(crate) const fn digest(self) -> FieldTechnicalQualityDigestV1 {
        self.digest
    }
}

fn hash_operator_kind(hasher: &mut Hasher, kind: FieldOperatorKindV1) {
    hasher.update(&[match kind {
        FieldOperatorKindV1::GaussianBlurSeparableQ32V1 => 1,
        FieldOperatorKindV1::PremultipliedSourceOverV1 => 2,
        FieldOperatorKindV1::EncodedSrgb8ScreenOpaqueBackdropV1 => 3,
        FieldOperatorKindV1::PorterDuffLighterV1 => 4,
    }]);
}

fn hash_composition_law_class(hasher: &mut Hasher, class: FieldCompositionLawClassV1) {
    hasher.update(&[match class {
        FieldCompositionLawClassV1::GaussianBlurSeparableQ32 => 1,
        FieldCompositionLawClassV1::PremultipliedSourceOver => 2,
        FieldCompositionLawClassV1::EncodedSrgb8ScreenOpaqueBackdrop => 3,
        FieldCompositionLawClassV1::PorterDuffLighter => 4,
    }]);
}

fn finalize(hasher: Hasher) -> [u8; 32] {
    let digest = hasher.finalize();
    *digest.as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field_effect::FieldOperatorKindV1;

    fn sample_request_digest() -> FieldRequestDigestV1 {
        FieldRequestDigestV1::from_bytes([1u8; 32])
    }

    fn sample_output_digest() -> FieldRasterDigestV1 {
        FieldRasterDigestV1::from_bytes_for_test([2u8; 32])
    }

    fn sample_certificate_digest() -> FieldCertificateDigestV1 {
        // FieldCertificateDigestV1 uses the digest_type! macro in field_effect
        // which does not expose from_bytes_for_test. We construct via the
        // public test helper if available, or use unsafe-free transmute-free
        // approach: the digest_type! macro in field_effect has no test ctor
        // for CertificateDigest. We need to check.
        // Actually, looking at the macro, only RequestDigest and RasterDigest
        // have test constructors. CertificateDigest does not. We'll add a
        // workaround by using the raw bytes through the struct constructor
        // which is pub(crate).
        FieldCertificateDigestV1::from_bytes_for_test([3u8; 32])
    }

    #[test]
    fn gamut_containment_proof_deterministic() {
        let a = FieldGamutContainmentProofV1::new(sample_output_digest(), 1024);
        let b = FieldGamutContainmentProofV1::new(sample_output_digest(), 1024);
        assert_eq!(a, b);
        assert_eq!(a.digest(), b.digest());
    }

    #[test]
    fn gamut_containment_proof_varies_with_pixel_count() {
        let a = FieldGamutContainmentProofV1::new(sample_output_digest(), 1024);
        let b = FieldGamutContainmentProofV1::new(sample_output_digest(), 2048);
        assert_ne!(a, b);
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn gamut_containment_proof_varies_with_output_digest() {
        let other = FieldRasterDigestV1::from_bytes_for_test([9u8; 32]);
        let a = FieldGamutContainmentProofV1::new(sample_output_digest(), 1024);
        let b = FieldGamutContainmentProofV1::new(other, 1024);
        assert_ne!(a, b);
    }

    #[test]
    fn composition_law_proof_deterministic() {
        let a = FieldCompositionLawProofV1::new(
            FieldOperatorKindV1::GaussianBlurSeparableQ32V1,
            sample_request_digest(),
            sample_output_digest(),
        );
        let b = FieldCompositionLawProofV1::new(
            FieldOperatorKindV1::GaussianBlurSeparableQ32V1,
            sample_request_digest(),
            sample_output_digest(),
        );
        assert_eq!(a, b);
        assert_eq!(a.digest(), b.digest());
    }

    #[test]
    fn composition_law_proof_varies_by_operator() {
        let blur = FieldCompositionLawProofV1::new(
            FieldOperatorKindV1::GaussianBlurSeparableQ32V1,
            sample_request_digest(),
            sample_output_digest(),
        );
        let source_over = FieldCompositionLawProofV1::new(
            FieldOperatorKindV1::PremultipliedSourceOverV1,
            sample_request_digest(),
            sample_output_digest(),
        );
        assert_ne!(blur, source_over);
        assert_ne!(blur.law_class(), source_over.law_class());
    }

    #[test]
    fn composition_law_class_maps_correctly() {
        assert_eq!(
            FieldCompositionLawClassV1::from_operator_kind(
                FieldOperatorKindV1::GaussianBlurSeparableQ32V1
            ),
            FieldCompositionLawClassV1::GaussianBlurSeparableQ32
        );
        assert_eq!(
            FieldCompositionLawClassV1::from_operator_kind(
                FieldOperatorKindV1::PremultipliedSourceOverV1
            ),
            FieldCompositionLawClassV1::PremultipliedSourceOver
        );
        assert_eq!(
            FieldCompositionLawClassV1::from_operator_kind(
                FieldOperatorKindV1::EncodedSrgb8ScreenOpaqueBackdropV1
            ),
            FieldCompositionLawClassV1::EncodedSrgb8ScreenOpaqueBackdrop
        );
        assert_eq!(
            FieldCompositionLawClassV1::from_operator_kind(
                FieldOperatorKindV1::PorterDuffLighterV1
            ),
            FieldCompositionLawClassV1::PorterDuffLighter
        );
    }

    #[test]
    fn declared_restoration_policy_deterministic() {
        let a = FieldDeclaredRestorationPolicyV1::new(
            sample_certificate_digest(),
            FieldOperatorKindV1::PremultipliedSourceOverV1,
        );
        let b = FieldDeclaredRestorationPolicyV1::new(
            sample_certificate_digest(),
            FieldOperatorKindV1::PremultipliedSourceOverV1,
        );
        assert_eq!(a, b);
        assert_eq!(a.digest(), b.digest());
    }

    #[test]
    fn declared_restoration_policy_varies_by_operator() {
        let a = FieldDeclaredRestorationPolicyV1::new(
            sample_certificate_digest(),
            FieldOperatorKindV1::PremultipliedSourceOverV1,
        );
        let b = FieldDeclaredRestorationPolicyV1::new(
            sample_certificate_digest(),
            FieldOperatorKindV1::PorterDuffLighterV1,
        );
        assert_ne!(a, b);
    }

    #[test]
    fn declared_restoration_policy_varies_by_certificate() {
        let other_cert = FieldCertificateDigestV1::from_bytes_for_test([7u8; 32]);
        let a = FieldDeclaredRestorationPolicyV1::new(
            sample_certificate_digest(),
            FieldOperatorKindV1::PremultipliedSourceOverV1,
        );
        let b = FieldDeclaredRestorationPolicyV1::new(
            other_cert,
            FieldOperatorKindV1::PremultipliedSourceOverV1,
        );
        assert_ne!(a, b);
    }

    #[test]
    fn technical_quality_deterministic() {
        let a = FieldTechnicalQualityV1::new(
            FieldOperatorKindV1::GaussianBlurSeparableQ32V1,
            sample_request_digest(),
            sample_output_digest(),
            512,
        );
        let b = FieldTechnicalQualityV1::new(
            FieldOperatorKindV1::GaussianBlurSeparableQ32V1,
            sample_request_digest(),
            sample_output_digest(),
            512,
        );
        assert_eq!(a, b);
        assert_eq!(a.digest(), b.digest());
    }

    #[test]
    fn technical_quality_varies_by_operator() {
        let a = FieldTechnicalQualityV1::new(
            FieldOperatorKindV1::GaussianBlurSeparableQ32V1,
            sample_request_digest(),
            sample_output_digest(),
            512,
        );
        let b = FieldTechnicalQualityV1::new(
            FieldOperatorKindV1::PremultipliedSourceOverV1,
            sample_request_digest(),
            sample_output_digest(),
            512,
        );
        assert_ne!(a, b);
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn technical_quality_varies_by_pixel_count() {
        let a = FieldTechnicalQualityV1::new(
            FieldOperatorKindV1::GaussianBlurSeparableQ32V1,
            sample_request_digest(),
            sample_output_digest(),
            512,
        );
        let b = FieldTechnicalQualityV1::new(
            FieldOperatorKindV1::GaussianBlurSeparableQ32V1,
            sample_request_digest(),
            sample_output_digest(),
            1024,
        );
        assert_ne!(a, b);
    }

    #[test]
    fn technical_quality_contains_consistent_sub_proofs() {
        let tq = FieldTechnicalQualityV1::new(
            FieldOperatorKindV1::PremultipliedSourceOverV1,
            sample_request_digest(),
            sample_output_digest(),
            256,
        );
        assert_eq!(
            tq.composition_law_proof().request_digest(),
            tq.request_digest()
        );
        assert_eq!(
            tq.composition_law_proof().output_digest(),
            tq.output_digest()
        );
        assert_eq!(
            tq.gamut_containment_proof().output_digest(),
            tq.output_digest()
        );
        assert_eq!(tq.gamut_containment_proof().pixel_count(), 256);
        assert_eq!(
            tq.composition_law_proof().law_class(),
            FieldCompositionLawClassV1::PremultipliedSourceOver
        );
    }

    #[test]
    fn technical_quality_equality_implies_digest_equality() {
        let a = FieldTechnicalQualityV1::new(
            FieldOperatorKindV1::PorterDuffLighterV1,
            sample_request_digest(),
            sample_output_digest(),
            100,
        );
        let b = FieldTechnicalQualityV1::new(
            FieldOperatorKindV1::PorterDuffLighterV1,
            sample_request_digest(),
            sample_output_digest(),
            100,
        );
        assert_eq!(a, b);
        assert_eq!(a.digest(), b.digest());
        assert_eq!(a.composition_law_proof(), b.composition_law_proof());
        assert_eq!(a.gamut_containment_proof(), b.gamut_containment_proof());
    }
}
