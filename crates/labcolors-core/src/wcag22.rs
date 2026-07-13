//! Нормативная оценка WCAG 2.2 для одной финальной пары sRGB8 (#284).
//!
//! Это additive-путь: он не меняет legacy `crate::wcag`, solver или adaptive
//! runtime. Решение принимается только целочисленными сравнениями над
//! закоммиченными Q55 outward-границами. `f64`, `powf`, epsilon и отображаемое
//! округление в verdict не участвуют.

use core::fmt;

use crate::numerics::{
    NumericalArtifactIdV2, NumericalDecisionEvidenceV1, NumericalErrorBoundIdV2, NumericalProofIdV2,
};

#[path = "wcag22/kernel.rs"]
mod kernel;
#[path = "wcag22/q55_data.rs"]
mod q55_data;

pub use kernel::{evaluate_wcag22_hex, evaluate_wcag22_srgb8};

const PROFILE_SOURCE_JSON: &str = include_str!("../contracts/wcag22-srgb8-v1.json");
const PROOF_SOURCE_JSON: &str = include_str!("../contracts/wcag22-srgb8-q55-proof-v1.json");
const PROOF_SOURCE_SHA256: &str =
    "56b73b0eac677f47885897c05f8bc5fb2a6c5eb4c162dd574f84a975a167122f";
const PROOF_PAYLOAD_SHA256: &str =
    "6f72876b525cc435030f61809a42f4e7719ac778106485f75723a914df517532";
const VERIFIER_SHA256: &str = "5146e9a79d462961fe607919f946508f009f441b9cdba2ec5208206b1853d3a0";

/// Идентификатор immutable normative profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Wcag22ProfileIdV1 {
    /// WCAG 2.2 Recommendation 2024-12-12 over final encoded sRGB8 bytes.
    Wcag22Srgb8ContrastV1,
}

impl Wcag22ProfileIdV1 {
    /// Стабильный wire key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Wcag22Srgb8ContrastV1 => "wcag22-srgb8-contrast-v1",
        }
    }
}

/// Связка normative source, finite artifact и bound law.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wcag22ProfileV1 {
    /// Версия JSON-схемы профиля.
    pub schema_version: u32,
    /// Semantic identity профиля.
    pub profile_id: Wcag22ProfileIdV1,
    /// Датированный normative source.
    pub recommendation: &'static str,
    /// Raw canonical machine-readable profile.
    pub source_json: &'static str,
    /// SHA-256 exact bytes [`Self::source_json`].
    pub source_sha256: &'static str,
    /// FNV-1a-32 of the typed, length-prefixed profile V1 preimage.
    pub profile_checksum: &'static str,
    /// Canonical finite Q55 artifact identity.
    pub artifact_id: NumericalArtifactIdV2,
    /// SHA-256 canonical binary table preimage.
    pub artifact_sha256: &'static str,
    /// SHA-256 exact generator source that produced the table.
    pub generator_sha256: &'static str,
    /// Registered outward-bound/decision law.
    pub bound_id: NumericalErrorBoundIdV2,
    /// Replayable full-domain proof identity.
    pub proof_id: NumericalProofIdV2,
    /// Raw canonical proof artifact.
    pub proof_json: &'static str,
    /// SHA-256 exact proof file bytes.
    pub proof_sha256: &'static str,
    /// SHA-256 canonical proof payload before its self-authenticating field.
    pub proof_payload_sha256: &'static str,
    /// SHA-256 independent verifier source.
    pub verifier_sha256: &'static str,
}

static WCAG22_PROFILE_V1: Wcag22ProfileV1 = Wcag22ProfileV1 {
    schema_version: 1,
    profile_id: Wcag22ProfileIdV1::Wcag22Srgb8ContrastV1,
    recommendation: "https://www.w3.org/TR/2024/REC-WCAG22-20241212/",
    source_json: PROFILE_SOURCE_JSON,
    source_sha256: q55_data::PROFILE_SOURCE_SHA256,
    profile_checksum: q55_data::PROFILE_CHECKSUM,
    artifact_id: NumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55V1,
    artifact_sha256: q55_data::ARTIFACT_SHA256,
    generator_sha256: q55_data::GENERATOR_SHA256,
    bound_id: NumericalErrorBoundIdV2::Wcag22Srgb8OutwardQ55V1,
    proof_id: NumericalProofIdV2::Wcag22Srgb8FullDomainQ55V1,
    proof_json: PROOF_SOURCE_JSON,
    proof_sha256: PROOF_SOURCE_SHA256,
    proof_payload_sha256: PROOF_PAYLOAD_SHA256,
    verifier_sha256: VERIFIER_SHA256,
};

/// Канонический профиль текущего evaluator-а.
#[must_use]
pub fn wcag22_profile_v1() -> &'static Wcag22ProfileV1 {
    &WCAG22_PROFILE_V1
}

/// Exact WCAG 2.2 success criterion declared for this use occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Wcag22CriterionV1 {
    /// SC 1.4.3, ordinary text: 4.5:1.
    Sc143TextDefault,
    /// SC 1.4.3, explicitly declared large-scale text: 3:1.
    Sc143TextLargeScale,
    /// SC 1.4.11, required visual information of a UI component/state: 3:1.
    Sc1411UiComponentOrState,
    /// SC 1.4.11, required visual information of a graphical object: 3:1.
    Sc1411GraphicalObject,
}

/// Q55 outward enclosure of one final colour's relative luminance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wcag22LuminanceBoundsQ55V1 {
    lower: u64,
    upper: u64,
}

impl Wcag22LuminanceBoundsQ55V1 {
    /// Inclusive lower bound, scaled by `2^55`.
    pub fn lower(self) -> u64 {
        self.lower
    }

    /// Inclusive upper bound, scaled by `2^55`.
    pub fn upper(self) -> u64 {
        self.upper
    }

    /// Fixed-point scale.
    pub fn scale() -> u64 {
        q55_data::Q55_SCALE
    }
}

/// Measurement payload retained with the threshold decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wcag22MeasurementV1 {
    /// Final foreground bytes supplied to the evaluator.
    pub foreground: [u8; 3],
    /// Final background bytes supplied to the evaluator.
    pub background: [u8; 3],
    /// Foreground relative-luminance enclosure.
    pub foreground_luminance: Wcag22LuminanceBoundsQ55V1,
    /// Background relative-luminance enclosure.
    pub background_luminance: Wcag22LuminanceBoundsQ55V1,
}

/// Total production decision for the admitted final-sRGB8 domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Wcag22ApplicableDecisionV1 {
    /// Exact threshold law is proved true in one orientation.
    Pass,
    /// Exact threshold law is proved false in both orientations.
    Fail,
}

/// Explicit report-layer declaration that no WCAG criterion applies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wcag22ClientDeclaredNotApplicableV1 {
    reason_id: String,
}

impl Wcag22ClientDeclaredNotApplicableV1 {
    /// Build a non-empty opaque declaration. Core does not interpret the ID.
    pub fn try_new(reason_id: impl Into<String>) -> Result<Self, Wcag22EvaluationErrorV1> {
        let reason_id = reason_id.into();
        if reason_id.is_empty() {
            return Err(Wcag22EvaluationErrorV1::EmptyNotApplicableReason);
        }
        Ok(Self { reason_id })
    }

    /// Opaque client-owned reason identity.
    pub fn reason_id(&self) -> &str {
        &self.reason_id
    }
}

/// Atomic evaluation/report result. `NotEvaluated` never carries a decision.
/// The proof-bearing `Evaluated` variant is externally inspectable but cannot
/// be constructed outside Core, so callers cannot reuse genuine evidence with
/// a forged criterion or reversed decision.
///
/// ```compile_fail
/// use labcolors_core::wcag22::{
///     Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22CriterionV1,
///     evaluate_wcag22_srgb8,
/// };
///
/// let genuine = evaluate_wcag22_srgb8(
///     [0, 0, 0],
///     [255, 255, 255],
///     Wcag22CriterionV1::Sc143TextDefault,
/// ).unwrap();
/// let Wcag22AssessmentV1::Evaluated {
///     profile_id,
///     criterion,
///     measurement,
///     evidence,
///     ..
/// } = genuine else { unreachable!() };
/// let _forged = Wcag22AssessmentV1::Evaluated {
///     profile_id,
///     criterion,
///     measurement,
///     decision: Wcag22ApplicableDecisionV1::Fail,
///     evidence,
/// };
/// ```
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Wcag22AssessmentV1 {
    /// One explicit occurrence criterion was evaluated.
    #[non_exhaustive]
    Evaluated {
        /// Immutable evaluator profile.
        profile_id: Wcag22ProfileIdV1,
        /// Client-declared criterion; never inferred from a token name.
        criterion: Wcag22CriterionV1,
        /// Exact finite measurement.
        measurement: Wcag22MeasurementV1,
        /// Total Pass/Fail decision on the admitted domain.
        decision: Wcag22ApplicableDecisionV1,
        /// Registry-sealed finite-bound evidence.
        evidence: NumericalDecisionEvidenceV1,
    },
    /// Report-only branch: the pair evaluator was intentionally not run.
    NotEvaluated {
        /// Profile the declaration refers to.
        profile_id: Wcag22ProfileIdV1,
        /// Explicit client-owned declaration.
        declaration: Wcag22ClientDeclaredNotApplicableV1,
    },
}

/// Fail-closed evaluator errors. No variant is a colour decision.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Wcag22EvaluationErrorV1 {
    /// Public hex input was not one exact `#RRGGBB` value.
    InvalidSrgb8 {
        /// Input field identity.
        field: &'static str,
        /// Parser reason without a fallback value.
        reason: String,
    },
    /// A report-layer NotApplicable declaration had no identity.
    EmptyNotApplicableReason,
    /// Shipped bounds failed to separate a threshold; release proof must forbid it.
    ArtifactInvariantViolation {
        /// Criterion whose threshold was unresolved.
        criterion: Wcag22CriterionV1,
        /// Exact foreground input.
        foreground: [u8; 3],
        /// Exact background input.
        background: [u8; 3],
    },
    /// Registry and evaluator artifact identities drifted.
    EvidenceRegistryMismatch(String),
}

impl fmt::Display for Wcag22EvaluationErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSrgb8 { field, reason } => {
                write!(formatter, "invalid WCAG22 {field}: {reason}")
            }
            Self::EmptyNotApplicableReason => {
                formatter.write_str("WCAG22 NotApplicable reason ID must be non-empty")
            }
            Self::ArtifactInvariantViolation {
                criterion,
                foreground,
                background,
            } => write!(
                formatter,
                "WCAG22 Q55 artifact failed to separate {criterion:?} for {foreground:?}/{background:?}"
            ),
            Self::EvidenceRegistryMismatch(message) => {
                write!(formatter, "WCAG22 numerical registry mismatch: {message}")
            }
        }
    }
}

impl std::error::Error for Wcag22EvaluationErrorV1 {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_binds_canonical_sources_and_artifact() {
        let profile = wcag22_profile_v1();
        assert_eq!(profile.profile_id.key(), "wcag22-srgb8-contrast-v1");
        assert_eq!(profile.source_json, PROFILE_SOURCE_JSON);
        assert_eq!(profile.source_sha256.len(), 64);
        assert_eq!(profile.profile_checksum, "152813fe");
        assert_eq!(profile.artifact_sha256.len(), 64);
        assert_eq!(profile.generator_sha256.len(), 64);
        assert_eq!(profile.proof_json, PROOF_SOURCE_JSON);
        assert_eq!(profile.proof_sha256.len(), 64);
        assert_eq!(profile.proof_payload_sha256.len(), 64);
        assert_eq!(profile.verifier_sha256.len(), 64);
        assert_eq!(profile.schema_version, 1);
    }

    #[test]
    fn empty_not_applicable_reason_is_rejected() {
        assert!(Wcag22ClientDeclaredNotApplicableV1::try_new("").is_err());
    }

    #[test]
    fn hex_transport_rejects_invalid_input_without_panic_or_fallback() {
        for invalid in ["not-a-colour", "FFFFFF", "##FFFFFF", "###FFFFFF"] {
            let error =
                evaluate_wcag22_hex(invalid, "#FFFFFF", Wcag22CriterionV1::Sc143TextDefault)
                    .unwrap_err();
            assert!(matches!(
                error,
                Wcag22EvaluationErrorV1::InvalidSrgb8 {
                    field: "foreground",
                    ..
                }
            ));
        }
    }

    #[test]
    fn hex_transport_accepts_exact_canonical_input() {
        let assessment =
            evaluate_wcag22_hex("#000000", "#FFFFFF", Wcag22CriterionV1::Sc143TextDefault)
                .expect("exact #RRGGBB transport must reach the proof-bound evaluator");
        assert!(matches!(
            assessment,
            Wcag22AssessmentV1::Evaluated {
                decision: Wcag22ApplicableDecisionV1::Pass,
                ..
            }
        ));
    }

    #[test]
    fn luminance_bounds_are_ordered_and_fit_the_declared_scale() {
        for rgb in [[0, 0, 0], [255, 255, 255], [137, 187, 9], [62, 34, 23]] {
            let bounds = kernel::luminance_bounds(rgb);
            assert!(bounds.lower <= bounds.upper);
            assert!(bounds.upper <= q55_data::Q55_SCALE + 3);
        }
    }
}
