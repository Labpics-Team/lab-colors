use crate::Srgb8;
use crate::appearance::ModeledSrgb8PointOccurrence;
use crate::constraints::{Evaluator, private};

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

/// PASS-marker без дублирования bytes: invocation хранит target, а physical
/// binding — actual. Создать marker может только sealed evaluator после exact
/// equality, поэтому несовпадающая success-пара непредставима и в памяти.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactIdentityAssessmentV1(());

/// Типизированный отказ exact-гейта. Он несёт только диагностическую пару и
/// никогда не выдаёт частично «проверенный» occurrence/evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactIdentityMismatchV1 {
    target: Srgb8,
    actual: Srgb8,
}

impl ExactIdentityMismatchV1 {
    pub(crate) const fn target(self) -> Srgb8 {
        self.target
    }

    pub(crate) const fn actual(self) -> Srgb8 {
        self.actual
    }
}

pub(crate) struct ExactSrgb8IdentityV1;

impl ExactSrgb8IdentityV1 {
    pub(crate) const IDENTITY: ExactConstraintIdentityV1 =
        ExactConstraintIdentityV1::FinalSrgb8IdentityV1;
}

impl private::EvaluatorSealed for ExactSrgb8IdentityV1 {}

impl Evaluator<ModeledSrgb8PointOccurrence> for ExactSrgb8IdentityV1 {
    type Invocation = Srgb8;
    type Identity = ExactConstraintIdentityV1;
    type Release = ExactIdentityReleaseV1;
    type Capability = ExactIdentityCapabilityV1;
    type Assessment = ExactIdentityAssessmentV1;
    type Error = ExactIdentityMismatchV1;

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
        target: &Self::Invocation,
    ) -> Result<ExactIdentityAssessmentV1, ExactIdentityMismatchV1> {
        let actual = Srgb8::new(occurrence.visible());
        if actual != *target {
            return Err(ExactIdentityMismatchV1 {
                target: *target,
                actual,
            });
        }
        Ok(ExactIdentityAssessmentV1(()))
    }
}
