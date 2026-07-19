//! Приватная typed-связка physical target и evaluator-а.
//!
//! Модуль не является public registry: он лишь гарантирует, что assessment
//! сохраняет identity физического evidence и release реально вызванного
//! evaluator-а.

#[cfg(test)]
use crate::appearance::{ModeledSrgb8PointOccurrence, ResolvedOccurrence, VisiblePointBindingV1};

mod exact;
pub(crate) use exact::{ExactConstraintIdentityV1, ExactIdentityMismatchV1, ExactSrgb8IdentityV1};

#[cfg(test)]
mod wcag22;

#[cfg(test)]
pub(crate) use wcag22::Wcag22Srgb8V1;

/// Marker-ы недоступны внешним crate-ам: новые target/evaluator families
/// добавляются только вместе с code-owned physical adapter-ом.
#[cfg(test)]
mod private {
    pub trait EvaluatorSealed {}
}

#[cfg(test)]
pub(crate) trait Evaluator<Target>: private::EvaluatorSealed {
    type Invocation;
    type Release;
    type Assessment;
    type Error;

    fn release(&self) -> Self::Release;

    fn evaluate(
        &self,
        target: &Target,
        invocation: Self::Invocation,
    ) -> Result<Self::Assessment, Self::Error>;
}

/// Assessment вместе с exact physical binding и evaluator release.
/// Поля закрыты, чтобы genuine result нельзя было пересвязать вручную.
#[derive(Debug, Clone, PartialEq)]
#[cfg(test)]
pub(crate) struct BoundAssessment<Binding, Release, Assessment> {
    binding: Binding,
    release: Release,
    assessment: Assessment,
}

#[cfg(test)]
impl<Binding, Release, Assessment> BoundAssessment<Binding, Release, Assessment> {
    pub(crate) fn binding(&self) -> &Binding {
        &self.binding
    }

    pub(crate) fn release(&self) -> &Release {
        &self.release
    }

    pub(crate) fn assessment(&self) -> &Assessment {
        &self.assessment
    }
}

#[cfg(test)]
pub(crate) type AssessmentResult<Evaluation> = Result<
    BoundAssessment<
        VisiblePointBindingV1,
        <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Release,
        <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Assessment,
    >,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Error,
>;

#[cfg(test)]
pub(crate) fn assess<Evaluation>(
    source: &ResolvedOccurrence,
    evaluator: &Evaluation,
    invocation: Evaluation::Invocation,
) -> AssessmentResult<Evaluation>
where
    Evaluation: Evaluator<ModeledSrgb8PointOccurrence>,
{
    let target = source.modeled_srgb8_point();
    let binding = source.visible_point_binding();
    let assessment = evaluator.evaluate(&target, invocation)?;
    Ok(BoundAssessment {
        binding,
        release: evaluator.release(),
        assessment,
    })
}

#[cfg(test)]
mod tests {
    use super::{Evaluator, ModeledSrgb8PointOccurrence, assess, private};
    use crate::appearance::PointOpacityOverSurfaceV1;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct SentinelError;

    struct FailingEvaluator;

    impl private::EvaluatorSealed for FailingEvaluator {}

    impl Evaluator<ModeledSrgb8PointOccurrence> for FailingEvaluator {
        type Invocation = ();
        type Release = &'static str;
        type Assessment = ();
        type Error = SentinelError;

        fn release(&self) -> Self::Release {
            "sentinel-v1"
        }

        fn evaluate(
            &self,
            _target: &ModeledSrgb8PointOccurrence,
            _invocation: Self::Invocation,
        ) -> Result<Self::Assessment, Self::Error> {
            Err(SentinelError)
        }
    }

    #[test]
    fn evaluator_error_is_returned_without_report_or_fallback() {
        let occurrence = PointOpacityOverSurfaceV1::evaluate([1, 2, 3], 0.5, [4, 5, 6])
            .unwrap_or_else(|error| panic!("valid point occurrence rejected: {}", error.message()));
        let error = assess(&occurrence, &FailingEvaluator, ())
            .expect_err("binder must preserve evaluator failure");
        assert_eq!(error, SentinelError);
    }
}
