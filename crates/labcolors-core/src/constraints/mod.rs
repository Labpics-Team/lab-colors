//! Приватная typed-связка physical target и evaluator-а.
//!
//! Модуль не является public registry: он лишь гарантирует, что assessment
//! сохраняет identity физического evidence и release реально вызванного
//! evaluator-а.

use crate::appearance::{ModeledSrgb8PointOccurrence, ResolvedOccurrence, VisiblePointBindingV1};

mod exact;
pub(crate) use exact::{
    ExactConstraintIdentityV1, ExactIdentityAssessmentV1, ExactIdentityCapabilityV1,
    ExactIdentityMismatchV1, ExactIdentityReleaseV1, ExactSrgb8IdentityV1,
};

#[cfg(test)]
mod wcag22;

#[cfg(test)]
pub(crate) use wcag22::{Wcag22Srgb8CapabilityV1, Wcag22Srgb8EvaluatorIdentityV1, Wcag22Srgb8V1};

/// Marker-ы недоступны внешним crate-ам: новые target/evaluator families
/// добавляются только вместе с code-owned physical adapter-ом.
mod private {
    pub trait EvaluatorSealed {}
}

pub(crate) trait Evaluator<Target>: private::EvaluatorSealed {
    type Invocation;
    type Identity;
    type Release;
    type Capability;
    type Assessment;
    type Error;

    fn identity(&self) -> Self::Identity;

    fn release(&self) -> Self::Release;

    fn capability(&self) -> Self::Capability;

    fn evaluate(
        &self,
        target: &Target,
        invocation: &Self::Invocation,
    ) -> Result<Self::Assessment, Self::Error>;
}

/// Один outcome вместе с exact physical binding и metadata действительно
/// вызванного evaluator-а. Один и тот же carrier используется для PASS и FAIL,
/// поэтому отказ не теряет provenance и не восстанавливает её вручную.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoundEvidence<Binding, Identity, Release, Capability, Invocation, Outcome> {
    binding: Binding,
    identity: Identity,
    release: Release,
    capability: Capability,
    invocation: Invocation,
    outcome: Outcome,
}

impl<Binding, Identity, Release, Capability, Invocation, Outcome>
    BoundEvidence<Binding, Identity, Release, Capability, Invocation, Outcome>
{
    pub(crate) fn binding(&self) -> &Binding {
        &self.binding
    }

    pub(crate) fn identity(&self) -> &Identity {
        &self.identity
    }

    pub(crate) fn release(&self) -> &Release {
        &self.release
    }

    pub(crate) fn capability(&self) -> &Capability {
        &self.capability
    }

    pub(crate) fn invocation(&self) -> &Invocation {
        &self.invocation
    }

    pub(crate) fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    pub(crate) fn into_outcome(self) -> Outcome {
        self.outcome
    }
}

pub(crate) type BoundAssessment<Binding, Identity, Release, Capability, Invocation, Assessment> =
    BoundEvidence<Binding, Identity, Release, Capability, Invocation, Assessment>;

pub(crate) type BoundFailure<Binding, Identity, Release, Capability, Invocation, Error> =
    BoundEvidence<Binding, Identity, Release, Capability, Invocation, Error>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundVerdict<Binding, Identity, Release, Capability, Invocation, Assessment, Error>
{
    Pass(BoundAssessment<Binding, Identity, Release, Capability, Invocation, Assessment>),
    Fail(BoundFailure<Binding, Identity, Release, Capability, Invocation, Error>),
}

pub(crate) type AssessmentResult<Evaluation> = BoundVerdict<
    VisiblePointBindingV1,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Identity,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Release,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Capability,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Invocation,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Assessment,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Error,
>;

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
    let verdict = evaluator.evaluate(&target, &invocation);
    let identity = evaluator.identity();
    let release = evaluator.release();
    let capability = evaluator.capability();
    match verdict {
        Ok(outcome) => BoundVerdict::Pass(BoundEvidence {
            binding,
            identity,
            release,
            capability,
            invocation,
            outcome,
        }),
        Err(outcome) => BoundVerdict::Fail(BoundEvidence {
            binding,
            identity,
            release,
            capability,
            invocation,
            outcome,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{BoundVerdict, Evaluator, ModeledSrgb8PointOccurrence, assess, private};
    use crate::appearance::PointOpacityOverSurfaceV1;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct SentinelError;

    struct FailingEvaluator;

    impl private::EvaluatorSealed for FailingEvaluator {}

    impl Evaluator<ModeledSrgb8PointOccurrence> for FailingEvaluator {
        type Invocation = ();
        type Identity = &'static str;
        type Release = &'static str;
        type Capability = &'static str;
        type Assessment = ();
        type Error = SentinelError;

        fn identity(&self) -> Self::Identity {
            "sentinel-law"
        }

        fn release(&self) -> Self::Release {
            "sentinel-v1"
        }

        fn capability(&self) -> Self::Capability {
            "sentinel-point"
        }

        fn evaluate(
            &self,
            _target: &ModeledSrgb8PointOccurrence,
            _invocation: &Self::Invocation,
        ) -> Result<Self::Assessment, Self::Error> {
            Err(SentinelError)
        }
    }

    #[test]
    fn evaluator_error_is_returned_without_report_or_fallback() {
        let occurrence = PointOpacityOverSurfaceV1::evaluate([1, 2, 3], 0.5, [4, 5, 6])
            .unwrap_or_else(|error| panic!("valid point occurrence rejected: {}", error.message()));
        let BoundVerdict::Fail(error) = assess(&occurrence, &FailingEvaluator, ()) else {
            panic!("failing evaluator unexpectedly passed");
        };
        assert_eq!(error.outcome(), &SentinelError);
        assert_eq!(error.identity(), &"sentinel-law");
        assert_eq!(error.release(), &"sentinel-v1");
        assert_eq!(error.capability(), &"sentinel-point");
        assert_eq!(error.invocation(), &());
    }
}
