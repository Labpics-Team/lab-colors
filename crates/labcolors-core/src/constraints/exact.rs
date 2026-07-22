use crate::Srgb8;
use crate::appearance::ModeledSrgb8PointOccurrence;
use crate::constraints::{
    Evaluator, HardClassifier, HardDecision, ProgramPointTargetV1, VisiblePointPassEvidence,
    VisiblePointViolationEvidence, private,
};
use core::convert::Infallible;

/// Структурная identity общего exact-закона финального point occurrence.
/// Она не содержит client ID, target bytes или выбранную alpha: эти значения
/// принадлежат конкретной invocation, а не закону проверки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactConstraintIdentityV1 {
    FinalSrgb8IdentityV1,
}

/// Версия формулы exact byte-identity evaluator-а.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactIdentityReleaseV1 {
    V1,
}

/// Узкая capability evaluator-а: только финальный modeled point occurrence в
/// encoded-sRGB8, не source Paint и не пиксели произвольного renderer-а.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactIdentityCapabilityV1 {
    FinalOccurrenceSrgb8IdentityV1,
}

/// Закрытые ZST payload-типы делают Pass и Violation несовместимыми, но не
/// позволяют classifier-у вернуть другое measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactIdentityPassV1(());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactIdentityViolationV1(());

pub(crate) type ExactPassEvidenceV1 = VisiblePointPassEvidence<ExactSrgb8IdentityV1>;
pub(crate) type ExactViolationEvidenceV1 = VisiblePointViolationEvidence<ExactSrgb8IdentityV1>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactSrgb8IdentityV1;

impl ExactSrgb8IdentityV1 {
    pub(crate) const IDENTITY: ExactConstraintIdentityV1 =
        ExactConstraintIdentityV1::FinalSrgb8IdentityV1;
}

impl private::EvaluatorSealed for ExactSrgb8IdentityV1 {}
impl private::HardClassifierSealed for ExactSrgb8IdentityV1 {}

impl Evaluator<ModeledSrgb8PointOccurrence> for ExactSrgb8IdentityV1 {
    type Invocation = Srgb8;
    type Identity = ExactConstraintIdentityV1;
    type Release = ExactIdentityReleaseV1;
    type Capability = ExactIdentityCapabilityV1;
    type Measurement = Srgb8;
    type Error = Infallible;

    fn identity(&self) -> Self::Identity {
        Self::IDENTITY
    }

    fn release(&self) -> Self::Release {
        ExactIdentityReleaseV1::V1
    }

    fn capability(&self) -> Self::Capability {
        ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1
    }

    fn evaluate(
        &self,
        occurrence: &ModeledSrgb8PointOccurrence,
        _invocation: &Self::Invocation,
    ) -> Result<Self::Measurement, Self::Error> {
        Ok(Srgb8::new(occurrence.visible()))
    }
}

impl Evaluator<ProgramPointTargetV1> for ExactSrgb8IdentityV1 {
    type Invocation = Srgb8;
    type Identity = ExactConstraintIdentityV1;
    type Release = ExactIdentityReleaseV1;
    type Capability = ExactIdentityCapabilityV1;
    type Measurement = Srgb8;
    type Error = Infallible;

    fn identity(&self) -> Self::Identity {
        Self::IDENTITY
    }

    fn release(&self) -> Self::Release {
        ExactIdentityReleaseV1::V1
    }

    fn capability(&self) -> Self::Capability {
        ExactIdentityCapabilityV1::FinalOccurrenceSrgb8IdentityV1
    }

    fn evaluate(
        &self,
        occurrence: &ProgramPointTargetV1,
        invocation: &Self::Invocation,
    ) -> Result<Self::Measurement, Self::Error> {
        <Self as Evaluator<ModeledSrgb8PointOccurrence>>::evaluate(
            self,
            &occurrence.encoded(),
            invocation,
        )
    }
}

impl HardClassifier<Srgb8, Srgb8> for ExactSrgb8IdentityV1 {
    type Pass = ExactIdentityPassV1;
    type Violation = ExactIdentityViolationV1;

    fn classify(
        &self,
        invocation: &Srgb8,
        measurement: &Srgb8,
    ) -> HardDecision<Self::Pass, Self::Violation> {
        let actual = *measurement;
        if actual == *invocation {
            HardDecision::Pass(ExactIdentityPassV1(()))
        } else {
            HardDecision::Violation(ExactIdentityViolationV1(()))
        }
    }
}
