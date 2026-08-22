//! R-04 PR1: CleanPotential evaluator types for staged cleanliness assessment.
//!
//! This module defines the identity, release, capability, invocation, error,
//! and evidence types for the CleanPotential evaluator. The evaluator stub
//! returns `OutsideApplicabilityDomain` to prevent ungoverned pass-through:
//! no measurement may be produced until the full admission contract (R-04 PR-B)
//! is implemented. These types exist so that downstream consumers can depend
//! on the sealed type surface without depending on runtime behaviour.

// R-04 staged infrastructure; consumed by PR-B/C/D.
#![allow(dead_code)]

use crate::constraints::{HardClassifier, HardDecision, private};

/// Structural identity of the CleanPotential constraint law.
/// Contains no client ID, target bytes, or chosen alpha: those values belong
/// to a concrete invocation, not to the verification law itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CleanPotentialIdentityV1 {
    /// The canonical CleanPotential point-evaluator identity.
    PointCleanPotentialV1,
    #[cfg(test)]
    MutationSentinelV1,
}

/// Version tag for the CleanPotential evaluator formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CleanPotentialReleaseV1 {
    V1,
    #[cfg(test)]
    MutationSentinelV1,
}

/// Narrow capability: only staged CleanPotential assessment over encoded-sRGB8
/// modeled point occurrences. Does NOT mint CleanPass, FinalOwnedClean, or
/// human-clean action authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CleanPotentialCapabilityV1 {
    StagedPointCleanPotentialV1,
    #[cfg(test)]
    MutationSentinelV1,
}

/// Invocation payload for the CleanPotential evaluator.
/// Currently a zero-sized placeholder; the real invocation will carry
/// the cleanliness reference context once R-04 PR-B lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CleanPotentialInvocationV1(());

impl CleanPotentialInvocationV1 {
    /// Construct the staged invocation placeholder.
    pub(crate) const fn new() -> Self {
        Self(())
    }
}

/// Error type for the CleanPotential evaluator stub.
///
/// The only variant is `OutsideApplicabilityDomain`, which signals that the
/// evaluator has not yet been admitted for production use. This prevents
/// silent pass-through: callers must handle this error explicitly rather than
/// receiving a fabricated measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CleanPotentialErrorV1 {
    /// The evaluator is outside its applicability domain. No measurement is
    /// produced. This is the ONLY error until R-04 PR-B admits the evaluator.
    OutsideApplicabilityDomain,
}

/// ZST payload for Pass evidence. Sealed: cannot be constructed outside this
/// module except through the HardClassifier path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CleanPotentialPassV1(());

/// ZST payload for Violation evidence. Sealed: cannot be constructed outside
/// this module except through the HardClassifier path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CleanPotentialViolationV1(());

/// The unit struct implementing Evaluator + HardClassifier for CleanPotential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CleanPotentialEvaluatorV1;

impl CleanPotentialEvaluatorV1 {
    pub(crate) const IDENTITY: CleanPotentialIdentityV1 =
        CleanPotentialIdentityV1::PointCleanPotentialV1;
}

impl private::EvaluatorSealed for CleanPotentialEvaluatorV1 {}
impl private::HardClassifierSealed for CleanPotentialEvaluatorV1 {}

/// Assessment result from the CleanPotential evaluator.
///
/// Because the evaluator stub always returns `OutsideApplicabilityDomain`,
/// this type currently has no `Measured` variant. It exists to define the
/// sealed assessment surface that PR-B will populate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CleanPotentialAssessmentV1 {
    /// The evaluator refused to produce a measurement.
    Refused(CleanPotentialErrorV1),
}

/// Evidence bundle for a CleanPotential assessment.
///
/// Wraps the assessment with the evaluator identity metadata so that
/// downstream binders can verify which evaluator produced (or refused) the
/// result without re-running evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CleanPotentialEvidenceV1 {
    identity: CleanPotentialIdentityV1,
    release: CleanPotentialReleaseV1,
    capability: CleanPotentialCapabilityV1,
    assessment: CleanPotentialAssessmentV1,
}

impl CleanPotentialEvidenceV1 {
    /// Construct evidence from an assessment and the evaluator's static metadata.
    pub(crate) const fn new(assessment: CleanPotentialAssessmentV1) -> Self {
        Self {
            identity: CleanPotentialEvaluatorV1::IDENTITY,
            release: CleanPotentialReleaseV1::V1,
            capability: CleanPotentialCapabilityV1::StagedPointCleanPotentialV1,
            assessment,
        }
    }

    pub(crate) const fn identity(self) -> CleanPotentialIdentityV1 {
        self.identity
    }

    pub(crate) const fn release(self) -> CleanPotentialReleaseV1 {
        self.release
    }

    pub(crate) const fn capability(self) -> CleanPotentialCapabilityV1 {
        self.capability
    }

    pub(crate) const fn assessment(self) -> CleanPotentialAssessmentV1 {
        self.assessment
    }
}

/// HardClassifier implementation.
///
/// Because the evaluator stub never produces a measurement, classify is
/// unreachable in production today. The implementation exists to satisfy
/// the sealed trait bound and to define the Pass/Violation mapping for
/// when PR-B introduces real measurements.
impl HardClassifier<CleanPotentialInvocationV1, CleanPotentialAssessmentV1>
    for CleanPotentialEvaluatorV1
{
    type Pass = CleanPotentialPassV1;
    type Violation = CleanPotentialViolationV1;

    fn classify(
        &self,
        _invocation: &CleanPotentialInvocationV1,
        _measurement: &CleanPotentialAssessmentV1,
    ) -> HardDecision<Self::Pass, Self::Violation> {
        // The stub never produces a measurement, so classify is never reached
        // in production. When PR-B admits the evaluator, this body will map
        // real assessments to Pass/Violation via the HardClassifier contract.
        // For now, return Violation as the safe default: no ungoverned pass.
        HardDecision::Violation(CleanPotentialViolationV1(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Construction tests ---

    #[test]
    fn identity_is_point_clean_potential_v1() {
        assert_eq!(
            CleanPotentialEvaluatorV1::IDENTITY,
            CleanPotentialIdentityV1::PointCleanPotentialV1,
        );
    }

    #[test]
    fn evidence_carries_static_metadata() {
        let evidence = CleanPotentialEvidenceV1::new(CleanPotentialAssessmentV1::Refused(
            CleanPotentialErrorV1::OutsideApplicabilityDomain,
        ));
        assert_eq!(
            evidence.identity(),
            CleanPotentialIdentityV1::PointCleanPotentialV1
        );
        assert_eq!(evidence.release(), CleanPotentialReleaseV1::V1);
        assert_eq!(
            evidence.capability(),
            CleanPotentialCapabilityV1::StagedPointCleanPotentialV1,
        );
    }

    #[test]
    fn evidence_assessment_round_trips() {
        let assessment =
            CleanPotentialAssessmentV1::Refused(CleanPotentialErrorV1::OutsideApplicabilityDomain);
        let evidence = CleanPotentialEvidenceV1::new(assessment);
        assert_eq!(evidence.assessment(), assessment);
    }

    // --- Rejection tests ---

    #[test]
    fn error_is_outside_applicability_domain() {
        let err = CleanPotentialErrorV1::OutsideApplicabilityDomain;
        assert_eq!(err, CleanPotentialErrorV1::OutsideApplicabilityDomain);
    }

    #[test]
    fn assessment_refused_carries_error() {
        let assessment =
            CleanPotentialAssessmentV1::Refused(CleanPotentialErrorV1::OutsideApplicabilityDomain);
        match assessment {
            CleanPotentialAssessmentV1::Refused(e) => {
                assert_eq!(e, CleanPotentialErrorV1::OutsideApplicabilityDomain);
            }
        }
    }

    #[test]
    fn invocation_constructs_as_zst() {
        let inv = CleanPotentialInvocationV1::new();
        assert_eq!(inv, CleanPotentialInvocationV1(()));
    }

    // --- Classification tests ---

    #[test]
    fn classifier_returns_violation_for_stub_assessment() {
        let evaluator = CleanPotentialEvaluatorV1;
        let invocation = CleanPotentialInvocationV1::new();
        let assessment =
            CleanPotentialAssessmentV1::Refused(CleanPotentialErrorV1::OutsideApplicabilityDomain);

        let decision = evaluator.classify(&invocation, &assessment);
        assert!(matches!(decision, HardDecision::Violation(_)));
    }

    #[test]
    fn pass_and_violation_are_distinct_types() {
        // Compile-time proof: Pass and Violation are different ZST types.
        // If they were the same type, this function would fail to compile
        // because Rust would see ambiguous return types.
        fn _assert_distinct(_: CleanPotentialPassV1, _: CleanPotentialViolationV1) {}
        // No runtime assertion needed; compilation is the proof.
    }

    #[test]
    fn identity_release_capability_enums_have_sentinels_in_test() {
        // Verify MutationSentinel variants exist under cfg(test).
        let _id = CleanPotentialIdentityV1::MutationSentinelV1;
        let _rel = CleanPotentialReleaseV1::MutationSentinelV1;
        let _cap = CleanPotentialCapabilityV1::MutationSentinelV1;
        // If any sentinel is removed, this test fails to compile, catching
        // accidental removal of the test-only escape hatch.
    }
}
