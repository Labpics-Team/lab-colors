use crate::appearance::ModeledSrgb8PointOccurrence;
use crate::constraints::{
    Evaluator, HardClassifier, HardDecision, ProgramConstraintContentV1,
    ProgramPointEvaluatorContentV1, ProgramPointTargetV1, private,
};
use crate::numerics::NumericalDecisionEvidenceV1;
use crate::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22ClientDeclaredNotApplicableV1,
    Wcag22CriterionV1, Wcag22EvaluationErrorV1, Wcag22MeasurementV1, Wcag22ProfileIdV1,
    evaluate_wcag22_srgb8, wcag22_profile_v1,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wcag22Srgb8V1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wcag22Srgb8CapabilityV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wcag22Srgb8EvaluatorIdentityV1;

/// Applicable-only WCAG measurement. Private fields make report-only
/// `NotEvaluated` and a mismatched criterion unrepresentable after refinement.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ApplicableWcag22MeasurementV1 {
    profile_id: Wcag22ProfileIdV1,
    criterion: Wcag22CriterionV1,
    measurement: Wcag22MeasurementV1,
    decision: Wcag22ApplicableDecisionV1,
    evidence: NumericalDecisionEvidenceV1,
}

#[cfg(test)]
impl ApplicableWcag22MeasurementV1 {
    pub(crate) const fn profile_id(&self) -> Wcag22ProfileIdV1 {
        self.profile_id
    }

    pub(crate) const fn criterion(&self) -> Wcag22CriterionV1 {
        self.criterion
    }

    pub(crate) const fn measurement(&self) -> &Wcag22MeasurementV1 {
        &self.measurement
    }

    pub(crate) const fn decision(&self) -> Wcag22ApplicableDecisionV1 {
        self.decision
    }

    pub(crate) const fn evidence(&self) -> &NumericalDecisionEvidenceV1 {
        &self.evidence
    }
}

/// Refinement faults are data, never a colour verdict. `Kernel` preserves the
/// real evaluator error; the other variants reject report/protocol states
/// before the hard classifier can run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ApplicableWcag22EvaluationErrorV1 {
    Kernel(Wcag22EvaluationErrorV1),
    ReportOnly {
        profile_id: Wcag22ProfileIdV1,
        declaration: Wcag22ClientDeclaredNotApplicableV1,
    },
    CriterionMismatch {
        requested: Wcag22CriterionV1,
        evaluated: Wcag22CriterionV1,
    },
}

fn refine_applicable_measurement(
    requested: Wcag22CriterionV1,
    assessment: Wcag22AssessmentV1,
) -> Result<ApplicableWcag22MeasurementV1, ApplicableWcag22EvaluationErrorV1> {
    match assessment {
        Wcag22AssessmentV1::Evaluated {
            profile_id,
            criterion,
            measurement,
            decision,
            evidence,
        } => {
            if criterion != requested {
                return Err(ApplicableWcag22EvaluationErrorV1::CriterionMismatch {
                    requested,
                    evaluated: criterion,
                });
            }
            Ok(ApplicableWcag22MeasurementV1 {
                profile_id,
                criterion,
                measurement,
                decision,
                evidence,
            })
        }
        Wcag22AssessmentV1::NotEvaluated {
            profile_id,
            declaration,
        } => Err(ApplicableWcag22EvaluationErrorV1::ReportOnly {
            profile_id,
            declaration,
        }),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wcag22PassV1(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wcag22ViolationV1(());

impl private::EvaluatorSealed for Wcag22Srgb8V1 {}
impl private::HardClassifierSealed for Wcag22Srgb8V1 {}

impl Evaluator<ModeledSrgb8PointOccurrence> for Wcag22Srgb8V1 {
    type Invocation = Wcag22CriterionV1;
    type Identity = Wcag22Srgb8EvaluatorIdentityV1;
    type Release = Wcag22ProfileIdV1;
    type Capability = Wcag22Srgb8CapabilityV1;
    type Measurement = ApplicableWcag22MeasurementV1;
    type Error = ApplicableWcag22EvaluationErrorV1;

    fn identity(&self) -> Self::Identity {
        Wcag22Srgb8EvaluatorIdentityV1
    }

    fn release(&self) -> Self::Release {
        wcag22_profile_v1().profile_id
    }

    fn capability(&self) -> Self::Capability {
        Wcag22Srgb8CapabilityV1
    }

    fn evaluate(
        &self,
        target: &ModeledSrgb8PointOccurrence,
        invocation: &Self::Invocation,
    ) -> Result<Self::Measurement, Self::Error> {
        let assessment = evaluate_wcag22_srgb8(target.visible(), target.backdrop(), *invocation)
            .map_err(ApplicableWcag22EvaluationErrorV1::Kernel)?;
        refine_applicable_measurement(*invocation, assessment)
    }
}

impl Evaluator<ProgramPointTargetV1> for Wcag22Srgb8V1 {
    type Invocation = Wcag22CriterionV1;
    type Identity = Wcag22Srgb8EvaluatorIdentityV1;
    type Release = Wcag22ProfileIdV1;
    type Capability = Wcag22Srgb8CapabilityV1;
    type Measurement = ApplicableWcag22MeasurementV1;
    type Error = ApplicableWcag22EvaluationErrorV1;

    fn identity(&self) -> Self::Identity {
        Wcag22Srgb8EvaluatorIdentityV1
    }

    fn release(&self) -> Self::Release {
        wcag22_profile_v1().profile_id
    }

    fn capability(&self) -> Self::Capability {
        Wcag22Srgb8CapabilityV1
    }

    fn evaluate(
        &self,
        target: &ProgramPointTargetV1,
        invocation: &Self::Invocation,
    ) -> Result<Self::Measurement, Self::Error> {
        <Self as Evaluator<ModeledSrgb8PointOccurrence>>::evaluate(
            self,
            &target.encoded(),
            invocation,
        )
    }
}

impl ProgramPointEvaluatorContentV1 for Wcag22Srgb8V1 {
    fn program_constraint_content_v1(
        &self,
        invocation: Wcag22CriterionV1,
    ) -> ProgramConstraintContentV1 {
        ProgramConstraintContentV1::Wcag22Srgb8 {
            identity: <Self as Evaluator<ProgramPointTargetV1>>::identity(self),
            release: <Self as Evaluator<ProgramPointTargetV1>>::release(self),
            capability: <Self as Evaluator<ProgramPointTargetV1>>::capability(self),
            criterion: invocation,
        }
    }
}

impl HardClassifier<Wcag22CriterionV1, ApplicableWcag22MeasurementV1> for Wcag22Srgb8V1 {
    type Pass = Wcag22PassV1;
    type Violation = Wcag22ViolationV1;

    fn classify(
        &self,
        _invocation: &Wcag22CriterionV1,
        measurement: &ApplicableWcag22MeasurementV1,
    ) -> HardDecision<Self::Pass, Self::Violation> {
        match measurement.decision {
            Wcag22ApplicableDecisionV1::Pass => HardDecision::Pass(Wcag22PassV1(())),
            Wcag22ApplicableDecisionV1::Fail => HardDecision::Violation(Wcag22ViolationV1(())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genuine_not_evaluated_is_rejected_before_classification() {
        let declaration = Wcag22ClientDeclaredNotApplicableV1::try_new("decorative-divider")
            .expect("non-empty client declaration must be valid");
        let assessment = Wcag22AssessmentV1::NotEvaluated {
            profile_id: wcag22_profile_v1().profile_id,
            declaration,
        };

        let error = refine_applicable_measurement(Wcag22CriterionV1::Sc143TextDefault, assessment)
            .expect_err("report-only assessment cannot reach a hard classifier");
        let ApplicableWcag22EvaluationErrorV1::ReportOnly {
            profile_id,
            declaration,
        } = error
        else {
            panic!("NotEvaluated must retain its report-only payload");
        };

        assert_eq!(profile_id, wcag22_profile_v1().profile_id);
        assert_eq!(declaration.reason_id(), "decorative-divider");
    }

    #[test]
    fn evaluated_criterion_mismatch_is_a_typed_refinement_error() {
        let assessment =
            evaluate_wcag22_srgb8([0; 3], [0xFF; 3], Wcag22CriterionV1::Sc143TextLargeScale)
                .expect("control pair must evaluate");

        let error = refine_applicable_measurement(Wcag22CriterionV1::Sc143TextDefault, assessment)
            .expect_err("assessment for another criterion must be rejected");
        let ApplicableWcag22EvaluationErrorV1::CriterionMismatch {
            requested,
            evaluated,
        } = error
        else {
            panic!("criterion mismatch must not be reclassified");
        };

        assert_eq!(requested, Wcag22CriterionV1::Sc143TextDefault);
        assert_eq!(evaluated, Wcag22CriterionV1::Sc143TextLargeScale);
    }
}
