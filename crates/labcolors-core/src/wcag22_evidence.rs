//! Proof-bound terminal evidence projection for the WCAG 2.2 evaluator.
//!
//! The independent verifier hashes this whole module. It is deliberately small:
//! one canonical capability row, exact stable keys, mint preconditions and the
//! sealed evidence variant returned by the production kernel.

use crate::numerics::{
    NumericalArtifactIdV2, NumericalBoundStatusV2, NumericalDecisionEvidenceV1,
    NumericalErrorBoundIdV2, NumericalEvidenceClassV2, NumericalFallbackStatusV1,
    NumericalProofIdV2, NumericalSiteIdV2, NumericalSiteRecordV2, StableNumericalOutcomeV2,
    numerical_registry_v2,
};

/// Opaque terminal payload. External callers can inspect the registered typed
/// identities but cannot construct or alter the payload.
///
/// ```compile_fail
/// use labcolors_core::wcag22::{Wcag22AssessmentV1, Wcag22CriterionV1, evaluate_wcag22_srgb8};
/// use labcolors_core::{NumericalDecisionEvidenceV1, CanonicalFiniteBoundedEvidenceV1};
///
/// let assessment = evaluate_wcag22_srgb8(
///     [0, 0, 0],
///     [255, 255, 255],
///     Wcag22CriterionV1::Sc143TextDefault,
/// ).unwrap();
/// let Wcag22AssessmentV1::Evaluated { evidence, .. } = assessment else { unreachable!() };
/// let NumericalDecisionEvidenceV1::CanonicalFiniteBounded(payload) = evidence else {
///     unreachable!()
/// };
/// let _forged = CanonicalFiniteBoundedEvidenceV1 {
///     artifact_id: payload.artifact_id(),
///     bound_id: payload.bound_id(),
///     proof_id: payload.proof_id(),
///     _private: (),
/// };
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonicalFiniteBoundedEvidenceV1 {
    artifact_id: NumericalArtifactIdV2,
    bound_id: NumericalErrorBoundIdV2,
    proof_id: NumericalProofIdV2,
    _private: (),
}

impl CanonicalFiniteBoundedEvidenceV1 {
    /// Canonical finite artifact used by the evaluator.
    pub fn artifact_id(self) -> NumericalArtifactIdV2 {
        self.artifact_id
    }

    /// Registered outward-bound/decision law.
    pub fn bound_id(self) -> NumericalErrorBoundIdV2 {
        self.bound_id
    }

    /// Replayable full-domain proof identity.
    pub fn proof_id(self) -> NumericalProofIdV2 {
        self.proof_id
    }
}

const SITE_ID: NumericalSiteIdV2 = NumericalSiteIdV2::Wcag22Srgb8ContrastV1;
const ARTIFACT_ID: NumericalArtifactIdV2 = NumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55V1;
const BOUND_ID: NumericalErrorBoundIdV2 = NumericalErrorBoundIdV2::Wcag22Srgb8OutwardQ55V1;
const PROOF_ID: NumericalProofIdV2 = NumericalProofIdV2::Wcag22Srgb8FullDomainQ55V1;

fn validate_canonical_row(row: &NumericalSiteRecordV2) -> Result<(), String> {
    if row.site_id != SITE_ID {
        return Err("WCAG22 terminal evidence site identity drifted".to_string());
    }
    if row.site_id.key() != "wcag22-srgb8-contrast-v1" {
        return Err("WCAG22 terminal evidence site key drifted".to_string());
    }
    if row.stable_outcomes != [StableNumericalOutcomeV2::CanonicalFiniteBounded] {
        return Err("WCAG22 terminal evidence stable outcomes drifted".to_string());
    }
    if !row.compatibility_releases.is_empty() {
        return Err("WCAG22 terminal evidence admitted compatibility".to_string());
    }
    if row.evidence_classes != [NumericalEvidenceClassV2::CanonicalFiniteBounded] {
        return Err("WCAG22 terminal evidence class drifted".to_string());
    }
    if row.artifact_ids != [ARTIFACT_ID] {
        return Err("WCAG22 terminal evidence artifact identity drifted".to_string());
    }
    if ARTIFACT_ID.key() != "wcag22-srgb8-luminance-q55-v1" {
        return Err("WCAG22 terminal evidence artifact key drifted".to_string());
    }
    if row.bound_ids != [BOUND_ID] {
        return Err("WCAG22 terminal evidence bound identity drifted".to_string());
    }
    if BOUND_ID.key() != "wcag22-srgb8-outward-q55-v1" {
        return Err("WCAG22 terminal evidence bound key drifted".to_string());
    }
    if row.proof_ids != [PROOF_ID] {
        return Err("WCAG22 terminal evidence proof identity drifted".to_string());
    }
    if PROOF_ID.key() != "wcag22-srgb8-full-domain-q55-v1" {
        return Err("WCAG22 terminal evidence proof key drifted".to_string());
    }
    if !row.runtime_attestations.is_empty() {
        return Err("WCAG22 terminal evidence admitted a runtime attestation".to_string());
    }
    if row.bound_status != NumericalBoundStatusV2::Available {
        return Err("WCAG22 terminal evidence bound status drifted".to_string());
    }
    if row.fallback_status != NumericalFallbackStatusV1::None {
        return Err("WCAG22 terminal evidence admitted a fallback".to_string());
    }
    Ok(())
}

/// Mint the only terminal evidence admitted by the proof-bound WCAG kernel.
pub(crate) fn mint_wcag22_evidence() -> Result<NumericalDecisionEvidenceV1, String> {
    let row = numerical_registry_v2()
        .iter()
        .find(|row| row.site_id == SITE_ID)
        .ok_or_else(|| "WCAG22 site отсутствует в registry V2".to_string())?;
    validate_canonical_row(row)?;
    Ok(NumericalDecisionEvidenceV1::CanonicalFiniteBounded(
        CanonicalFiniteBoundedEvidenceV1 {
            artifact_id: ARTIFACT_ID,
            bound_id: BOUND_ID,
            proof_id: PROOF_ID,
            _private: (),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_row_mints_exact_terminal_evidence() {
        let evidence = mint_wcag22_evidence().expect("canonical WCAG row must mint evidence");
        assert!(matches!(
            evidence,
            NumericalDecisionEvidenceV1::CanonicalFiniteBounded(payload)
                if payload.artifact_id() == ARTIFACT_ID
                    && payload.bound_id() == BOUND_ID
                    && payload.proof_id() == PROOF_ID
        ));
    }

    #[test]
    fn unrelated_registered_row_cannot_mint_wcag_evidence() {
        let glow = numerical_registry_v2()
            .iter()
            .find(|row| row.site_id == NumericalSiteIdV2::GlowTargetOrMaximumV1)
            .expect("Glow row");
        assert!(validate_canonical_row(glow).is_err());
    }
}
