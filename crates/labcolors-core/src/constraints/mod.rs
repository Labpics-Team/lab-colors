//! Приватная typed-связка physical measurement, hard-classifier и evidence.
//!
//! Evaluator только измеряет modeled occurrence. Hard verdict появляется один
//! раз в sealed classifier-е, после чего source-specific binder атомарно
//! связывает результат с physical occurrence и metadata реально вызванного
//! evaluator-а.

use crate::Srgb8;
use crate::appearance::{ModeledSrgb8PointOccurrence, ResolvedOccurrence, VisiblePointBindingV1};

mod exact;
pub(crate) use exact::{
    ExactConstraintIdentityV1, ExactIdentityCapabilityV1, ExactIdentityReleaseV1,
    ExactPassEvidenceV1, ExactSrgb8IdentityV1, ExactViolationEvidenceV1,
};

#[cfg(test)]
pub(crate) use exact::ExactIdentityPassV1;

mod wcag22;

pub(crate) use wcag22::{
    ApplicableWcag22EvaluationErrorV1, ApplicableWcag22MeasurementV1, Wcag22PassV1, Wcag22Srgb8V1,
    Wcag22ViolationV1,
};

/// Seals недоступны внешним crate-ам: новые evaluator/classifier families
/// добавляются только вместе с code-owned physical adapter-ом.
mod private {
    pub trait EvaluatorSealed {}
    pub trait HardClassifierSealed {}
}

pub(crate) trait Evaluator<Target>: private::EvaluatorSealed {
    type Invocation;
    type Identity;
    type Release;
    type Capability;
    type Measurement;
    type Error;

    fn identity(&self) -> Self::Identity;

    fn release(&self) -> Self::Release;

    fn capability(&self) -> Self::Capability;

    fn evaluate(
        &self,
        target: &Target,
        invocation: &Self::Invocation,
    ) -> Result<Self::Measurement, Self::Error>;
}

/// Одно измерение вместе с exact physical binding и metadata действительно
/// вызванного evaluator-а. Поля закрыты: binding создаёт только адаптер того
/// physical source, из которого одновременно получены target и certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BoundEvidence<Binding, Identity, Release, Capability, Invocation, Measurement> {
    binding: Binding,
    identity: Identity,
    release: Release,
    capability: Capability,
    invocation: Invocation,
    measurement: Measurement,
}

impl<Binding, Identity, Release, Capability, Invocation, Measurement>
    BoundEvidence<Binding, Identity, Release, Capability, Invocation, Measurement>
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

    #[cfg(test)]
    pub(crate) fn measurement(&self) -> &Measurement {
        &self.measurement
    }
}

/// Raw measurement, доказанно отнесённое classifier-ом ровно к одному
/// несовместимому исходу. Закрытый payload подтверждает решение classifier-а,
/// но не может заменить исходное measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ClassifiedMeasurement<Measurement, Classification> {
    measurement: Measurement,
    classification: Classification,
}

impl<Measurement, Classification> ClassifiedMeasurement<Measurement, Classification> {
    fn new(measurement: Measurement, classification: Classification) -> Self {
        Self {
            measurement,
            classification,
        }
    }

    pub(crate) fn value(&self) -> &Measurement {
        &self.measurement
    }
}

/// Два несовместимых hard-решения после успешного измерения. Ошибка evaluator-а
/// остаётся внешним `Result::Err`: отдельный `Fault` появится только вместе с
/// первым реальным fallible consumer и его исполняемым контрактом.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HardDecision<Pass, Violation> {
    Pass(Pass),
    Violation(Violation),
}

/// Sealed hard-classifier — единственный слой, которому разрешено превращать
/// raw measurement и invocation в Pass/Violation.
pub(crate) trait HardClassifier<Invocation, Measurement>:
    private::HardClassifierSealed
{
    type Pass;
    type Violation;

    fn classify(
        &self,
        invocation: &Invocation,
        measurement: &Measurement,
    ) -> HardDecision<Self::Pass, Self::Violation>;
}

pub(crate) type PointInvocation<Evaluation> =
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Invocation;
pub(crate) type PointMeasurement<Evaluation> =
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Measurement;
pub(crate) type PointPass<Evaluation> =
    <Evaluation as HardClassifier<PointInvocation<Evaluation>, PointMeasurement<Evaluation>>>::Pass;
pub(crate) type PointViolation<Evaluation> = <Evaluation as HardClassifier<
    PointInvocation<Evaluation>,
    PointMeasurement<Evaluation>,
>>::Violation;

type BoundVisiblePointMeasurement<Evaluation, Measurement> = BoundEvidence<
    VisiblePointBindingV1,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Identity,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Release,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Capability,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Invocation,
    Measurement,
>;

pub(crate) type VisiblePointPassEvidence<Evaluation> = BoundVisiblePointMeasurement<
    Evaluation,
    ClassifiedMeasurement<PointMeasurement<Evaluation>, PointPass<Evaluation>>,
>;

pub(crate) type VisiblePointViolationEvidence<Evaluation> = BoundVisiblePointMeasurement<
    Evaluation,
    ClassifiedMeasurement<PointMeasurement<Evaluation>, PointViolation<Evaluation>>,
>;

/// Единственный binder hard-classifier-а для final visible point. Modeled
/// target и binding берутся из одного occurrence; metadata связывается только
/// после успешного measurement и классификации.
pub(crate) fn assess_visible_point_hard<Evaluation>(
    source: &ResolvedOccurrence,
    evaluator: &Evaluation,
    invocation: PointInvocation<Evaluation>,
) -> Result<
    HardDecision<VisiblePointPassEvidence<Evaluation>, VisiblePointViolationEvidence<Evaluation>>,
    <Evaluation as Evaluator<ModeledSrgb8PointOccurrence>>::Error,
>
where
    Evaluation: Evaluator<ModeledSrgb8PointOccurrence>
        + HardClassifier<PointInvocation<Evaluation>, PointMeasurement<Evaluation>>,
{
    let target = source.modeled_srgb8_point();
    let binding = source.visible_point_binding();
    let measurement = evaluator.evaluate(&target, &invocation)?;
    let classification = evaluator.classify(&invocation, &measurement);
    let identity = evaluator.identity();
    let release = evaluator.release();
    let capability = evaluator.capability();

    Ok(match classification {
        HardDecision::Pass(payload) => HardDecision::Pass(BoundEvidence {
            binding,
            identity,
            release,
            capability,
            invocation,
            measurement: ClassifiedMeasurement::new(measurement, payload),
        }),
        HardDecision::Violation(payload) => HardDecision::Violation(BoundEvidence {
            binding,
            identity,
            release,
            capability,
            invocation,
            measurement: ClassifiedMeasurement::new(measurement, payload),
        }),
    })
}

impl<Binding, Identity, Release, Capability, Classification>
    BoundEvidence<
        Binding,
        Identity,
        Release,
        Capability,
        Srgb8,
        ClassifiedMeasurement<Srgb8, Classification>,
    >
{
    pub(crate) fn target(&self) -> Srgb8 {
        self.invocation
    }

    pub(crate) fn actual(&self) -> Srgb8 {
        *self.measurement.value()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Evaluator, HardClassifier, HardDecision, ModeledSrgb8PointOccurrence,
        assess_visible_point_hard, private,
    };
    use crate::Srgb8;
    use crate::appearance::PointOpacityOverSurfaceV1;
    use core::convert::Infallible;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct SentinelPass(());

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    struct SentinelViolation(());

    struct SubstitutionAttemptEvaluator {
        measured: Srgb8,
        attempted_replacement: Srgb8,
    }

    impl private::EvaluatorSealed for SubstitutionAttemptEvaluator {}
    impl private::HardClassifierSealed for SubstitutionAttemptEvaluator {}

    impl Evaluator<ModeledSrgb8PointOccurrence> for SubstitutionAttemptEvaluator {
        type Invocation = Srgb8;
        type Identity = ();
        type Release = ();
        type Capability = ();
        type Measurement = Srgb8;
        type Error = Infallible;

        fn identity(&self) {}

        fn release(&self) {}

        fn capability(&self) {}

        fn evaluate(
            &self,
            _target: &ModeledSrgb8PointOccurrence,
            _invocation: &Self::Invocation,
        ) -> Result<Self::Measurement, Self::Error> {
            Ok(self.measured)
        }
    }

    impl HardClassifier<Srgb8, Srgb8> for SubstitutionAttemptEvaluator {
        type Pass = SentinelPass;
        type Violation = SentinelViolation;

        fn classify(
            &self,
            _invocation: &Srgb8,
            measurement: &Srgb8,
        ) -> HardDecision<Self::Pass, Self::Violation> {
            assert_eq!(*measurement, self.measured);
            let _forbidden_substitute = self.attempted_replacement;
            HardDecision::Pass(SentinelPass(()))
        }
    }

    #[test]
    fn classifier_payload_cannot_replace_the_evaluator_measurement() {
        let occurrence = PointOpacityOverSurfaceV1::evaluate([1, 2, 3], 0.5, [4, 5, 6])
            .unwrap_or_else(|error| panic!("valid point occurrence rejected: {}", error.message()));
        let evaluator = SubstitutionAttemptEvaluator {
            measured: Srgb8::new([0x80; 3]),
            attempted_replacement: Srgb8::new([0x00; 3]),
        };
        let Ok(HardDecision::Pass(evidence)) =
            assess_visible_point_hard(&occurrence, &evaluator, Srgb8::new([0x80; 3]))
        else {
            panic!("control classifier must return Pass");
        };

        assert_eq!(evidence.actual(), evaluator.measured);
        assert_ne!(evidence.actual(), evaluator.attempted_replacement);
    }
}
