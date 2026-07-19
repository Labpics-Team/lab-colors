use crate::Srgb8;
use crate::appearance::ResolvedOccurrence;

/// Структурная identity единственного exact-ограничения AlphaAnalog v1.
/// Она не содержит client ID, target bytes или выбранную alpha: эти значения
/// принадлежат конкретной invocation, а не закону проверки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactConstraintIdentityV1 {
    FinalSrgb8IdentityV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactIdentityAssessmentV1 {
    target: Srgb8,
    actual: Srgb8,
}

impl ExactIdentityAssessmentV1 {
    pub(crate) const fn target(self) -> Srgb8 {
        self.target
    }

    pub(crate) const fn actual(self) -> Srgb8 {
        self.actual
    }
}

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

    pub(crate) fn evaluate(
        occurrence: &ResolvedOccurrence,
        target: Srgb8,
    ) -> Result<ExactIdentityAssessmentV1, ExactIdentityMismatchV1> {
        let actual = Srgb8::new(occurrence.visible());
        if actual != target {
            return Err(ExactIdentityMismatchV1 { target, actual });
        }
        Ok(ExactIdentityAssessmentV1 { target, actual })
    }
}
