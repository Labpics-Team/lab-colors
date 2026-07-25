//! Приватная typed-связка physical measurement, hard-classifier и evidence.
//!
//! Evaluator только измеряет modeled occurrence. Hard verdict появляется один
//! раз в sealed classifier-е, после чего source-specific binder атомарно
//! связывает результат с physical occurrence и metadata реально вызванного
//! evaluator-а.

use crate::Srgb8;
use crate::appearance::{ModeledSrgb8PointOccurrence, ResolvedOccurrence, VisiblePointBindingV1};
use crate::lcs_occurrence::ModeledLcsOccurrenceV1;
use crate::wcag22::{Wcag22CriterionV1, Wcag22ProfileIdV1};

mod exact;
pub(crate) use exact::{
    ExactConstraintIdentityV1, ExactIdentityCapabilityV1, ExactIdentityReleaseV1,
    ExactPassEvidenceV1, ExactSrgb8IdentityV1, ExactViolationEvidenceV1,
};

#[cfg(test)]
pub(crate) use exact::ExactIdentityPassV1;

mod wcag22;

pub(crate) use wcag22::{
    ApplicableWcag22EvaluationErrorV1, Wcag22Srgb8CapabilityV1, Wcag22Srgb8EvaluatorIdentityV1,
    Wcag22Srgb8V1,
};

#[cfg(test)]
pub(crate) use wcag22::{ApplicableWcag22MeasurementV1, Wcag22PassV1, Wcag22ViolationV1};

/// Test-only probe around the production Program WCAG evaluator. It records
/// each physical visible signal without changing measurement or
/// classification, allowing execution-count assertions at the Program
/// certification boundary.
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub(crate) struct CountingProgramWcag22Srgb8V1 {
    calls: std::rc::Rc<std::cell::RefCell<Vec<Srgb8>>>,
}

#[cfg(test)]
impl CountingProgramWcag22Srgb8V1 {
    pub(crate) fn calls(&self) -> Vec<Srgb8> {
        self.calls.borrow().clone()
    }
}

#[cfg(test)]
#[derive(Debug, Default)]
struct FinalRecheckMutantControlV1 {
    armed: std::cell::Cell<bool>,
    calls_after_arm: std::cell::Cell<usize>,
    force_current_violation: std::cell::Cell<bool>,
}

/// Test-only exact evaluator which can be armed to pass the next search call
/// and fail the immediately following final-recheck call.
#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub(crate) struct FinalRecheckMutantProgramEvaluatorV1 {
    control: std::rc::Rc<FinalRecheckMutantControlV1>,
}

#[cfg(test)]
impl FinalRecheckMutantProgramEvaluatorV1 {
    pub(crate) fn arm(&self) {
        self.control.calls_after_arm.set(0);
        self.control.force_current_violation.set(false);
        self.control.armed.set(true);
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MutantExactPassV1;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MutantExactViolationV1;

/// Seals недоступны внешним crate-ам: новые evaluator/classifier families
/// добавляются только вместе с code-owned physical adapter-ом.
mod private {
    pub trait EvaluatorSealed {}
    pub trait HardClassifierSealed {}
}

#[cfg(test)]
impl private::EvaluatorSealed for CountingProgramWcag22Srgb8V1 {}

#[cfg(test)]
impl private::HardClassifierSealed for CountingProgramWcag22Srgb8V1 {}

#[cfg(test)]
impl private::EvaluatorSealed for FinalRecheckMutantProgramEvaluatorV1 {}

#[cfg(test)]
impl private::HardClassifierSealed for FinalRecheckMutantProgramEvaluatorV1 {}

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
/// остаётся внешним `Result::Err` и не является ни Pass, ни Violation.
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

/// Program-only target: the exact encoded point used by physical evaluators
/// and the context-bound LCS occurrence derived from that same visible signal.
/// Neither half is optional, and construction remains inside the sole Program
/// execution path.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProgramPointTargetV1 {
    encoded: ModeledSrgb8PointOccurrence,
    modeled_lcs: ModeledLcsOccurrenceV1,
}

impl ProgramPointTargetV1 {
    pub(crate) const fn new(
        encoded: ModeledSrgb8PointOccurrence,
        modeled_lcs: ModeledLcsOccurrenceV1,
    ) -> Self {
        Self {
            encoded,
            modeled_lcs,
        }
    }

    pub(crate) const fn encoded(self) -> ModeledSrgb8PointOccurrence {
        self.encoded
    }

    pub(crate) const fn modeled_lcs(self) -> ModeledLcsOccurrenceV1 {
        self.modeled_lcs
    }
}

pub(crate) type ProgramPointInvocation<Evaluation> =
    <Evaluation as Evaluator<ProgramPointTargetV1>>::Invocation;
pub(crate) type ProgramPointMeasurement<Evaluation> =
    <Evaluation as Evaluator<ProgramPointTargetV1>>::Measurement;
pub(crate) type ProgramPointPass<Evaluation> = <Evaluation as HardClassifier<
    ProgramPointInvocation<Evaluation>,
    ProgramPointMeasurement<Evaluation>,
>>::Pass;
pub(crate) type ProgramPointViolation<Evaluation> = <Evaluation as HardClassifier<
    ProgramPointInvocation<Evaluation>,
    ProgramPointMeasurement<Evaluation>,
>>::Violation;

/// Sealed, statically dispatched evaluator family for context-bound Program
/// occurrences. Fixed point-support intentionally keeps its narrower encoded
/// target and therefore cannot silently satisfy an LCS-aware invocation.
pub(crate) trait ProgramPointEvaluatorV1:
    Sized
    + Evaluator<ProgramPointTargetV1>
    + HardClassifier<ProgramPointInvocation<Self>, ProgramPointMeasurement<Self>>
    + ProgramPointEvaluatorContentV1
{
}

impl<Evaluation> ProgramPointEvaluatorV1 for Evaluation where
    Evaluation: Sized
        + Evaluator<ProgramPointTargetV1>
        + HardClassifier<ProgramPointInvocation<Evaluation>, ProgramPointMeasurement<Evaluation>>
        + ProgramPointEvaluatorContentV1
{
}

/// Полное code-owned описание одного evaluator invocation для compile identity.
///
/// Здесь намеренно нет авторского constraint ID. Метаданные берутся из того же
/// закрытого определения evaluator-а, которое связывает runtime evidence, и не
/// могут разойтись с его identity, release или capability. Добавление либо
/// изменение production evaluator-а остаётся явной сменой схемы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramConstraintContentV1 {
    ExactSrgb8 {
        identity: ExactConstraintIdentityV1,
        release: ExactIdentityReleaseV1,
        capability: ExactIdentityCapabilityV1,
        expected: Srgb8,
    },
    Wcag22Srgb8 {
        identity: Wcag22Srgb8EvaluatorIdentityV1,
        release: Wcag22ProfileIdV1,
        capability: Wcag22Srgb8CapabilityV1,
        criterion: Wcag22CriterionV1,
    },
    #[cfg(test)]
    FinalRecheckMutantExactSrgb8 { expected: Srgb8 },
}

/// Внутрикрейтное описание generic test seam с одним evaluator-ом. Package
/// Program использует закрытое heterogeneous-множество, поэтому клиент не
/// может подменить descriptor.
pub(crate) trait ProgramPointEvaluatorContentV1: Evaluator<ProgramPointTargetV1> {
    fn program_constraint_content_v1(
        &self,
        invocation: ProgramPointInvocation<Self>,
    ) -> ProgramConstraintContentV1;
}

#[cfg(test)]
impl Evaluator<ProgramPointTargetV1> for CountingProgramWcag22Srgb8V1 {
    type Invocation = <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Invocation;
    type Identity = <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Identity;
    type Release = <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Release;
    type Capability = <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Capability;
    type Measurement = <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Measurement;
    type Error = <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Error;

    fn identity(&self) -> Self::Identity {
        <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::identity(&Wcag22Srgb8V1)
    }

    fn release(&self) -> Self::Release {
        <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::release(&Wcag22Srgb8V1)
    }

    fn capability(&self) -> Self::Capability {
        <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::capability(&Wcag22Srgb8V1)
    }

    fn evaluate(
        &self,
        target: &ProgramPointTargetV1,
        invocation: &Self::Invocation,
    ) -> Result<Self::Measurement, Self::Error> {
        self.calls
            .borrow_mut()
            .push(Srgb8::new(target.encoded().visible()));
        <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::evaluate(
            &Wcag22Srgb8V1,
            target,
            invocation,
        )
    }
}

#[cfg(test)]
impl ProgramPointEvaluatorContentV1 for CountingProgramWcag22Srgb8V1 {
    fn program_constraint_content_v1(
        &self,
        invocation: ProgramPointInvocation<Self>,
    ) -> ProgramConstraintContentV1 {
        ProgramConstraintContentV1::Wcag22Srgb8 {
            identity: self.identity(),
            release: self.release(),
            capability: self.capability(),
            criterion: invocation,
        }
    }
}

#[cfg(test)]
impl
    HardClassifier<
        <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Invocation,
        <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Measurement,
    > for CountingProgramWcag22Srgb8V1
{
    type Pass = <Wcag22Srgb8V1 as HardClassifier<
        <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Invocation,
        <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Measurement,
    >>::Pass;
    type Violation = <Wcag22Srgb8V1 as HardClassifier<
        <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Invocation,
        <Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Measurement,
    >>::Violation;

    fn classify(
        &self,
        invocation: &<Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Invocation,
        measurement: &<Wcag22Srgb8V1 as Evaluator<ProgramPointTargetV1>>::Measurement,
    ) -> HardDecision<Self::Pass, Self::Violation> {
        <Wcag22Srgb8V1 as HardClassifier<_, _>>::classify(&Wcag22Srgb8V1, invocation, measurement)
    }
}

#[cfg(test)]
impl Evaluator<ProgramPointTargetV1> for FinalRecheckMutantProgramEvaluatorV1 {
    type Invocation = Srgb8;
    type Identity = ();
    type Release = ();
    type Capability = ();
    type Measurement = Srgb8;
    type Error = core::convert::Infallible;

    fn identity(&self) -> Self::Identity {}

    fn release(&self) -> Self::Release {}

    fn capability(&self) -> Self::Capability {}

    fn evaluate(
        &self,
        target: &ProgramPointTargetV1,
        _invocation: &Self::Invocation,
    ) -> Result<Self::Measurement, Self::Error> {
        let force_violation = if self.control.armed.get() {
            let call = self.control.calls_after_arm.get();
            self.control.calls_after_arm.set(call + 1);
            if call == 1 {
                self.control.armed.set(false);
                true
            } else {
                false
            }
        } else {
            false
        };
        self.control.force_current_violation.set(force_violation);
        Ok(Srgb8::new(target.encoded().visible()))
    }
}

#[cfg(test)]
impl ProgramPointEvaluatorContentV1 for FinalRecheckMutantProgramEvaluatorV1 {
    fn program_constraint_content_v1(
        &self,
        invocation: ProgramPointInvocation<Self>,
    ) -> ProgramConstraintContentV1 {
        ProgramConstraintContentV1::FinalRecheckMutantExactSrgb8 {
            expected: invocation,
        }
    }
}

#[cfg(test)]
impl HardClassifier<Srgb8, Srgb8> for FinalRecheckMutantProgramEvaluatorV1 {
    type Pass = MutantExactPassV1;
    type Violation = MutantExactViolationV1;

    fn classify(
        &self,
        invocation: &Srgb8,
        measurement: &Srgb8,
    ) -> HardDecision<Self::Pass, Self::Violation> {
        if self.control.force_current_violation.replace(false) || invocation != measurement {
            HardDecision::Violation(MutantExactViolationV1)
        } else {
            HardDecision::Pass(MutantExactPassV1)
        }
    }
}

/// Program evidence binds the physical source-over certificate and the exact
/// modeled LCS provenance/context used by the evaluator in one non-forgeable
/// value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramVisiblePointBindingV1 {
    physical: VisiblePointBindingV1,
    modeled_lcs: ModeledLcsOccurrenceV1,
}

impl ProgramVisiblePointBindingV1 {
    pub(crate) const fn physical(self) -> VisiblePointBindingV1 {
        self.physical
    }

    pub(crate) const fn modeled_lcs(self) -> ModeledLcsOccurrenceV1 {
        self.modeled_lcs
    }
}

type BoundProgramPointMeasurement<Evaluation, Measurement> = BoundEvidence<
    ProgramVisiblePointBindingV1,
    <Evaluation as Evaluator<ProgramPointTargetV1>>::Identity,
    <Evaluation as Evaluator<ProgramPointTargetV1>>::Release,
    <Evaluation as Evaluator<ProgramPointTargetV1>>::Capability,
    <Evaluation as Evaluator<ProgramPointTargetV1>>::Invocation,
    Measurement,
>;

pub(crate) type ProgramVisiblePointPassEvidence<Evaluation> = BoundProgramPointMeasurement<
    Evaluation,
    ClassifiedMeasurement<ProgramPointMeasurement<Evaluation>, ProgramPointPass<Evaluation>>,
>;
pub(crate) type ProgramVisiblePointViolationEvidence<Evaluation> = BoundProgramPointMeasurement<
    Evaluation,
    ClassifiedMeasurement<ProgramPointMeasurement<Evaluation>, ProgramPointViolation<Evaluation>>,
>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramPointBindingMismatchV1 {
    physical: Srgb8,
    modeled: Srgb8,
}

impl ProgramPointBindingMismatchV1 {
    pub(crate) const fn physical(self) -> Srgb8 {
        self.physical
    }

    pub(crate) const fn modeled(self) -> Srgb8 {
        self.modeled
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProgramPointAssessmentErrorV1<EvaluatorError> {
    Binding(ProgramPointBindingMismatchV1),
    Evaluator(EvaluatorError),
}

pub(crate) type ProgramPointAssessmentResultV1<Evaluation> = Result<
    HardDecision<
        ProgramVisiblePointPassEvidence<Evaluation>,
        ProgramVisiblePointViolationEvidence<Evaluation>,
    >,
    ProgramPointAssessmentErrorV1<<Evaluation as Evaluator<ProgramPointTargetV1>>::Error>,
>;

pub(crate) fn assess_program_point_hard<Evaluation>(
    source: &ResolvedOccurrence,
    modeled_lcs: ModeledLcsOccurrenceV1,
    evaluator: &Evaluation,
    invocation: ProgramPointInvocation<Evaluation>,
) -> ProgramPointAssessmentResultV1<Evaluation>
where
    Evaluation: Evaluator<ProgramPointTargetV1>
        + HardClassifier<ProgramPointInvocation<Evaluation>, ProgramPointMeasurement<Evaluation>>,
{
    let physical = Srgb8::new(source.visible());
    let modeled = modeled_lcs.signal().srgb8();
    if physical != modeled {
        return Err(ProgramPointAssessmentErrorV1::Binding(
            ProgramPointBindingMismatchV1 { physical, modeled },
        ));
    }
    let target = ProgramPointTargetV1::new(source.modeled_srgb8_point(), modeled_lcs);
    let binding = ProgramVisiblePointBindingV1 {
        physical: source.visible_point_binding(),
        modeled_lcs: target.modeled_lcs(),
    };
    let measurement =
        <Evaluation as Evaluator<ProgramPointTargetV1>>::evaluate(evaluator, &target, &invocation)
            .map_err(ProgramPointAssessmentErrorV1::Evaluator)?;
    let classification = evaluator.classify(&invocation, &measurement);
    let identity = <Evaluation as Evaluator<ProgramPointTargetV1>>::identity(evaluator);
    let release = <Evaluation as Evaluator<ProgramPointTargetV1>>::release(evaluator);
    let capability = <Evaluation as Evaluator<ProgramPointTargetV1>>::capability(evaluator);

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
