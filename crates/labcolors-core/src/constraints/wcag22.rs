use crate::appearance::ModeledSrgb8PointOccurrence;
use crate::constraints::{Evaluator, private};
use crate::wcag22::{
    Wcag22AssessmentV1, Wcag22CriterionV1, Wcag22EvaluationErrorV1, Wcag22ProfileIdV1,
    evaluate_wcag22_srgb8, wcag22_profile_v1,
};

pub(crate) struct Wcag22Srgb8V1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wcag22Srgb8CapabilityV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wcag22Srgb8EvaluatorIdentityV1;

impl private::EvaluatorSealed for Wcag22Srgb8V1 {}

impl Evaluator<ModeledSrgb8PointOccurrence> for Wcag22Srgb8V1 {
    type Invocation = Wcag22CriterionV1;
    type Identity = Wcag22Srgb8EvaluatorIdentityV1;
    type Release = Wcag22ProfileIdV1;
    type Capability = Wcag22Srgb8CapabilityV1;
    type Assessment = Wcag22AssessmentV1;
    type Error = Wcag22EvaluationErrorV1;

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
    ) -> Result<Self::Assessment, Self::Error> {
        evaluate_wcag22_srgb8(target.visible(), target.backdrop(), *invocation)
    }
}
