//! Приватная typed-связка physical measurement, hard-classifier и evidence.
//!
//! Evaluator только измеряет свой typed target. Hard verdict появляется один
//! раз в sealed classifier-е, после чего source-specific binder атомарно
//! связывает результат с physical occurrence и metadata реально вызванного
//! evaluator-а. Derived LCS target отделён от encoded point capability типом.

use crate::Srgb8;
use crate::appearance::{ModeledSrgb8PointOccurrence, ResolvedOccurrence, VisiblePointBindingV1};
use crate::lcs_occurrence::{
    AppearanceContextId, ColorSignal, ColorimetricTransformReleaseId,
    ModeledLcsOccurrenceFormationErrorV1, ModeledLcsOccurrenceReleaseId, ModeledLcsOccurrenceV1,
};
use crate::wcag22::{Wcag22CriterionV1, Wcag22ProfileIdV1};
use std::cell::OnceCell;

mod exact;
pub(crate) use exact::{
    ExactConstraintIdentityV1, ExactIdentityCapabilityV1, ExactIdentityReleaseV1,
    ExactPassEvidenceV1, ExactSrgb8IdentityV1, ExactViolationEvidenceV1,
};

mod family;
pub(crate) use family::{
    FamilyMembershipCapabilityV1, FamilyMembershipIdentityV1, FamilyMembershipReleaseV2,
    FamilyMembershipV2,
};

#[cfg(test)]
pub(crate) use exact::ExactIdentityPassV1;

mod relation;
pub(crate) use relation::{
    CompiledCoreIntrinsicUnaryInvocationV1, CoreIntrinsicUnaryInvocationV1,
    CoreIntrinsicUnaryMeasurementV1, CoreIntrinsicUnaryPassV1, CoreIntrinsicUnaryViolationV1,
    CoreRelationInvocationV1, CoreRelationMeasurementV1, CoreRelationPassV1,
    CoreRelationViolationV1, ExactSrgb8IntrinsicUnaryCapabilityV1,
    ExactSrgb8IntrinsicUnaryIdentityV1, ExactSrgb8IntrinsicUnaryReleaseV1,
    ExactSrgb8RelationCapabilityV1, ExactSrgb8RelationIdentityV1, ExactSrgb8RelationReleaseV1,
};

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

/// Test-only LCS-aware evaluator used to prove capability sharing and atomic
/// provenance before the first production LCS constraint family is admitted.
#[cfg(test)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct LcsProbeProgramEvaluatorV1;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LcsProbePassV1;

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

#[cfg(test)]
impl private::EvaluatorSealed for LcsProbeProgramEvaluatorV1 {}

#[cfg(test)]
impl private::HardClassifierSealed for LcsProbeProgramEvaluatorV1 {}

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

/// Program-only encoded point. Appearance context and every derived view stay
/// outside this capability, so an encoded evaluator cannot acquire either
/// dependency through its target type.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProgramPointTargetV1 {
    encoded: ModeledSrgb8PointOccurrence,
}

impl ProgramPointTargetV1 {
    pub(crate) const fn new(encoded: ModeledSrgb8PointOccurrence) -> Self {
        Self { encoded }
    }

    pub(crate) const fn encoded(self) -> ModeledSrgb8PointOccurrence {
        self.encoded
    }
}

/// Canonical physical Program occurrence. Target and evidence views are
/// projected from this single value, so evaluator input and final source-over
/// certificate cannot name different occurrences.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProgramPointOccurrenceV1 {
    encoded: ModeledSrgb8PointOccurrence,
    physical: VisiblePointBindingV1,
    context: AppearanceContextId,
}

impl ProgramPointOccurrenceV1 {
    pub(crate) fn from_resolved(source: &ResolvedOccurrence, context: AppearanceContextId) -> Self {
        Self {
            encoded: source.modeled_srgb8_point(),
            physical: source.visible_point_binding(),
            context,
        }
    }

    pub(crate) const fn target(self) -> ProgramPointTargetV1 {
        ProgramPointTargetV1::new(self.encoded)
    }

    pub(crate) const fn binding(self) -> ProgramVisiblePointBindingV1 {
        ProgramVisiblePointBindingV1 {
            physical: self.physical,
            context: self.context,
        }
    }
}

/// Exact executable dependencies of an LCS-aware Program constraint. The same
/// value is retained by evaluator evidence and compile-time content identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "the typed LCS capability precedes its first admitted Program evaluator"
)]
pub(crate) struct ProgramLcsDependencyReleaseV1 {
    modeled_lcs_release: ModeledLcsOccurrenceReleaseId,
    transform_release: ColorimetricTransformReleaseId,
}

#[allow(
    dead_code,
    reason = "the typed LCS capability precedes its first admitted Program evaluator"
)]
impl ProgramLcsDependencyReleaseV1 {
    pub(crate) const fn current() -> Self {
        Self {
            modeled_lcs_release: crate::lcs_occurrence::MODELED_LCS_OCCURRENCE_RELEASE_V1,
            transform_release: crate::lcs_occurrence::ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1
                .transform_release(),
        }
    }

    pub(crate) const fn modeled_lcs_release(self) -> ModeledLcsOccurrenceReleaseId {
        self.modeled_lcs_release
    }

    pub(crate) const fn transform_release(self) -> ColorimetricTransformReleaseId {
        self.transform_release
    }

    #[cfg(test)]
    pub(crate) const fn with_modeled_lcs_release_for_test(
        self,
        modeled_lcs_release: ModeledLcsOccurrenceReleaseId,
    ) -> Self {
        Self {
            modeled_lcs_release,
            ..self
        }
    }
}

/// LCS-aware target is a distinct capability, never an optional field on the
/// encoded target. Only the occurrence-scoped adapter can construct it.
#[derive(Debug, Clone, Copy)]
#[allow(
    dead_code,
    reason = "the typed LCS capability precedes its first admitted Program evaluator"
)]
pub(crate) struct ProgramLcsPointTargetV1 {
    encoded: ModeledSrgb8PointOccurrence,
    modeled_lcs: ModeledLcsOccurrenceV1,
}

#[allow(
    dead_code,
    reason = "the typed LCS capability precedes its first admitted Program evaluator"
)]
impl ProgramLcsPointTargetV1 {
    pub(crate) const fn encoded(self) -> ModeledSrgb8PointOccurrence {
        self.encoded
    }

    pub(crate) const fn modeled_lcs(self) -> ModeledLcsOccurrenceV1 {
        self.modeled_lcs
    }
}

/// One lazy derived LCS capability over a canonical physical occurrence.
/// Success and failure are memoized so repeated LCS-aware constraints cannot
/// repeat the sRGB8 -> XYZ derivation inside one evaluation scope.
#[derive(Debug)]
#[allow(
    dead_code,
    reason = "the typed LCS capability precedes its first admitted Program evaluator"
)]
pub(crate) struct ProgramLcsPointAdapterV1 {
    point: ProgramPointOccurrenceV1,
    modeled_lcs: OnceCell<Result<ModeledLcsOccurrenceV1, ModeledLcsOccurrenceFormationErrorV1>>,
}

#[allow(
    dead_code,
    reason = "the typed LCS capability precedes its first admitted Program evaluator"
)]
impl ProgramLcsPointAdapterV1 {
    pub(crate) const fn new(point: ProgramPointOccurrenceV1) -> Self {
        Self {
            point,
            modeled_lcs: OnceCell::new(),
        }
    }

    fn modeled_lcs(&self) -> Result<ModeledLcsOccurrenceV1, ModeledLcsOccurrenceFormationErrorV1> {
        *self.modeled_lcs.get_or_init(|| {
            ModeledLcsOccurrenceV1::from_signal_in_context(
                ColorSignal::from_srgb8(Srgb8::new(self.point.encoded.visible())),
                self.point.context,
            )
        })
    }
}

#[cfg(test)]
impl Evaluator<ProgramLcsPointTargetV1> for LcsProbeProgramEvaluatorV1 {
    type Invocation = u8;
    type Identity = ();
    type Release = ProgramLcsDependencyReleaseV1;
    type Capability = ();
    type Measurement = ModeledLcsOccurrenceV1;
    type Error = core::convert::Infallible;

    fn identity(&self) -> Self::Identity {}

    fn release(&self) -> Self::Release {
        ProgramLcsDependencyReleaseV1::current()
    }

    fn capability(&self) -> Self::Capability {}

    fn evaluate(
        &self,
        target: &ProgramLcsPointTargetV1,
        _invocation: &Self::Invocation,
    ) -> Result<Self::Measurement, Self::Error> {
        debug_assert_eq!(
            target.modeled_lcs().signal().srgb8(),
            Srgb8::new(target.encoded().visible())
        );
        Ok(target.modeled_lcs())
    }
}

#[cfg(test)]
impl HardClassifier<u8, ModeledLcsOccurrenceV1> for LcsProbeProgramEvaluatorV1 {
    type Pass = LcsProbePassV1;
    type Violation = core::convert::Infallible;

    fn classify(
        &self,
        _invocation: &u8,
        _measurement: &ModeledLcsOccurrenceV1,
    ) -> HardDecision<Self::Pass, Self::Violation> {
        HardDecision::Pass(LcsProbePassV1)
    }
}

#[cfg(test)]
impl LcsProbeProgramEvaluatorV1 {
    pub(crate) fn program_constraint_content_v1(self) -> ProgramConstraintContentV1 {
        ProgramConstraintContentV1::ModeledLcsProbe {
            release: <Self as Evaluator<ProgramLcsPointTargetV1>>::release(&self),
        }
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
    ExactSrgb8IntrinsicUnary {
        identity: ExactSrgb8IntrinsicUnaryIdentityV1,
        release: ExactSrgb8IntrinsicUnaryReleaseV1,
        capability: ExactSrgb8IntrinsicUnaryCapabilityV1,
        expected: Srgb8,
    },
    FamilyMembership {
        identity: FamilyMembershipIdentityV1,
        release: FamilyMembershipReleaseV2,
        capability: FamilyMembershipCapabilityV1,
    },
    ExactSrgb8Relation {
        identity: ExactSrgb8RelationIdentityV1,
        release: ExactSrgb8RelationReleaseV1,
        capability: ExactSrgb8RelationCapabilityV1,
    },
    #[cfg(test)]
    ModeledLcsProbe {
        release: ProgramLcsDependencyReleaseV1,
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
/// declared appearance context. A derived LCS view is a separate capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProgramVisiblePointBindingV1 {
    physical: VisiblePointBindingV1,
    context: AppearanceContextId,
}

impl ProgramVisiblePointBindingV1 {
    pub(crate) const fn physical(self) -> VisiblePointBindingV1 {
        self.physical
    }

    pub(crate) const fn context(self) -> AppearanceContextId {
        self.context
    }
}

/// Evidence binding available only to LCS-aware evaluators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(
    dead_code,
    reason = "the typed LCS capability precedes its first admitted Program evaluator"
)]
pub(crate) struct ProgramLcsVisiblePointBindingV1 {
    physical: ProgramVisiblePointBindingV1,
    modeled_lcs: ModeledLcsOccurrenceV1,
}

#[allow(
    dead_code,
    reason = "the typed LCS capability precedes its first admitted Program evaluator"
)]
impl ProgramLcsVisiblePointBindingV1 {
    pub(crate) const fn physical(self) -> ProgramVisiblePointBindingV1 {
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
#[allow(
    dead_code,
    reason = "the typed LCS capability precedes its first admitted Program evaluator"
)]
pub(crate) enum ProgramLcsPointAssessmentErrorV1<EvaluatorError> {
    Formation(ModeledLcsOccurrenceFormationErrorV1),
    Evaluator(EvaluatorError),
}

pub(crate) type ProgramLcsPointInvocation<Evaluation> =
    <Evaluation as Evaluator<ProgramLcsPointTargetV1>>::Invocation;
pub(crate) type ProgramLcsPointMeasurement<Evaluation> =
    <Evaluation as Evaluator<ProgramLcsPointTargetV1>>::Measurement;
pub(crate) type ProgramLcsPointPass<Evaluation> = <Evaluation as HardClassifier<
    ProgramLcsPointInvocation<Evaluation>,
    ProgramLcsPointMeasurement<Evaluation>,
>>::Pass;
pub(crate) type ProgramLcsPointViolation<Evaluation> = <Evaluation as HardClassifier<
    ProgramLcsPointInvocation<Evaluation>,
    ProgramLcsPointMeasurement<Evaluation>,
>>::Violation;

type BoundProgramLcsPointMeasurement<Evaluation, Measurement> = BoundEvidence<
    ProgramLcsVisiblePointBindingV1,
    <Evaluation as Evaluator<ProgramLcsPointTargetV1>>::Identity,
    <Evaluation as Evaluator<ProgramLcsPointTargetV1>>::Release,
    <Evaluation as Evaluator<ProgramLcsPointTargetV1>>::Capability,
    <Evaluation as Evaluator<ProgramLcsPointTargetV1>>::Invocation,
    Measurement,
>;

pub(crate) type ProgramLcsVisiblePointPassEvidence<Evaluation> = BoundProgramLcsPointMeasurement<
    Evaluation,
    ClassifiedMeasurement<ProgramLcsPointMeasurement<Evaluation>, ProgramLcsPointPass<Evaluation>>,
>;
pub(crate) type ProgramLcsVisiblePointViolationEvidence<Evaluation> =
    BoundProgramLcsPointMeasurement<
        Evaluation,
        ClassifiedMeasurement<
            ProgramLcsPointMeasurement<Evaluation>,
            ProgramLcsPointViolation<Evaluation>,
        >,
    >;
pub(crate) type ProgramLcsPointAssessmentResultV1<Evaluation> = Result<
    HardDecision<
        ProgramLcsVisiblePointPassEvidence<Evaluation>,
        ProgramLcsVisiblePointViolationEvidence<Evaluation>,
    >,
    ProgramLcsPointAssessmentErrorV1<<Evaluation as Evaluator<ProgramLcsPointTargetV1>>::Error>,
>;

/// The only LCS-aware Program binder. The target and its physical evidence are
/// minted together from one occurrence-scoped adapter after one memoized
/// derivation, so callers cannot pair equal bytes from different occurrences.
#[allow(
    dead_code,
    reason = "the typed LCS capability precedes its first admitted Program evaluator"
)]
pub(crate) fn assess_program_lcs_point_hard<Evaluation>(
    adapter: &ProgramLcsPointAdapterV1,
    evaluator: &Evaluation,
    invocation: ProgramLcsPointInvocation<Evaluation>,
) -> ProgramLcsPointAssessmentResultV1<Evaluation>
where
    Evaluation: Evaluator<ProgramLcsPointTargetV1>
        + HardClassifier<
            ProgramLcsPointInvocation<Evaluation>,
            ProgramLcsPointMeasurement<Evaluation>,
        >,
{
    let modeled_lcs = adapter
        .modeled_lcs()
        .map_err(ProgramLcsPointAssessmentErrorV1::Formation)?;
    let target = ProgramLcsPointTargetV1 {
        encoded: adapter.point.encoded,
        modeled_lcs,
    };
    let binding = ProgramLcsVisiblePointBindingV1 {
        physical: adapter.point.binding(),
        modeled_lcs,
    };
    let measurement = evaluator
        .evaluate(&target, &invocation)
        .map_err(ProgramLcsPointAssessmentErrorV1::Evaluator)?;
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProgramPointAssessmentErrorV1<EvaluatorError> {
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
    point: ProgramPointOccurrenceV1,
    evaluator: &Evaluation,
    invocation: ProgramPointInvocation<Evaluation>,
) -> ProgramPointAssessmentResultV1<Evaluation>
where
    Evaluation: Evaluator<ProgramPointTargetV1>
        + HardClassifier<ProgramPointInvocation<Evaluation>, ProgramPointMeasurement<Evaluation>>,
{
    let target = point.target();
    let binding = point.binding();
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
