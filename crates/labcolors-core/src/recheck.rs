//! Revision-bound proof recheck одного уже заданного point Paint.
//!
//! Этот private-first срез не выбирает значение и не притворяется terminal
//! consumer-ом. Он связывает только уже существующие identity: статическую
//! физическую программу, authored Paint/Occurrence routing, реально вызванный
//! exact evaluator и атомарный observation snapshot. Session, output codec,
//! renderer capability и output ownership появятся только вместе с их
//! настоящим владельцем; поэтому данный модуль выпускает evidence recheck-а,
//! а не terminal-сертификат.

use crate::Srgb8;
use crate::appearance::{
    OccurrenceId, PaintId, PhysicalProgramIdentityV1, PointOpacityOverSurfaceV1,
    SourceOverCertificateV1, SurfaceInputPortId,
};
use crate::composition::{AdmittedOpacityV1, OpacityAdmissionErrorV1};
use crate::constraints::{
    ExactConstraintIdentityV1, ExactIdentityCapabilityV1, ExactIdentityReleaseV1,
    ExactPassEvidenceV1, ExactSrgb8IdentityV1, ExactViolationEvidenceV1, HardDecision,
    assess_visible_point_hard,
};
use crate::observation::{
    ObservationSnapshot, ObservationState, ObservationStreamId, ObservedScenarioSet,
    PriorObservation, Revision, ScenarioId,
};

/// Один immutable exact evaluator invocation, связанный с authored occurrence.
/// Identity/release/capability не дублируются здесь: proof получает их только
/// из binder-а действительно вызванного evaluator-а.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactOccurrenceRequirementV1 {
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    invocation: Srgb8,
}

impl ExactOccurrenceRequirementV1 {
    pub(crate) fn new(
        occurrence: OccurrenceId,
        surface: SurfaceInputPortId,
        invocation: Srgb8,
    ) -> Self {
        Self {
            occurrence,
            surface,
            invocation,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FixedRecheckCompileErrorV1 {
    EmptyOccurrences,
    DuplicateOccurrence(OccurrenceId),
}

/// Канонический compiled requirement одного fixed Paint. Это не общий registry:
/// единственный admitted evaluator здесь — exact final-occurrence identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompiledFixedRecheckV1 {
    physical_program: PhysicalProgramIdentityV1,
    paint: PaintId,
    occurrences: Box<[ExactOccurrenceRequirementV1]>,
}

impl CompiledFixedRecheckV1 {
    pub(crate) fn new(
        paint: PaintId,
        mut occurrences: Vec<ExactOccurrenceRequirementV1>,
    ) -> Result<Self, FixedRecheckCompileErrorV1> {
        if occurrences.is_empty() {
            return Err(FixedRecheckCompileErrorV1::EmptyOccurrences);
        }
        occurrences.sort_unstable_by_key(|requirement| requirement.occurrence);
        if let Some(duplicate) = occurrences
            .windows(2)
            .find(|window| window[0].occurrence == window[1].occurrence)
        {
            return Err(FixedRecheckCompileErrorV1::DuplicateOccurrence(
                duplicate[0].occurrence,
            ));
        }
        Ok(Self {
            physical_program: PointOpacityOverSurfaceV1::physical_identity(),
            paint,
            occurrences: occurrences.into_boxed_slice(),
        })
    }

    pub(crate) fn recheck(
        &self,
        observation: &ObservationState,
        paint: EncodedPaintCandidateV1,
    ) -> Result<FinalRecheckOutcomeV1, RecheckProtocolErrorV1> {
        if paint.id != self.paint {
            return Err(RecheckProtocolErrorV1::PaintMismatch {
                expected: self.paint,
                actual: paint.id,
            });
        }
        let snapshot = observation.snapshot();

        match snapshot {
            ObservationSnapshot::Waiting {
                stream,
                revision,
                schema,
            } => {
                self.validate_surface_schema(schema)?;
                Ok(FinalRecheckOutcomeV1::Waiting(WaitingRecheckV1 {
                    stream,
                    revision,
                }))
            }
            ObservationSnapshot::Stale {
                stream,
                revision,
                previous,
                schema,
            } => {
                self.validate_surface_schema(schema)?;
                Ok(FinalRecheckOutcomeV1::Stale(StaleRecheckV1 {
                    requirement: self.clone(),
                    paint,
                    stream,
                    schema: schema.to_vec().into_boxed_slice(),
                    current_revision: revision,
                    previous: previous.clone(),
                }))
            }
            ObservationSnapshot::Ready {
                stream,
                revision,
                set,
                schema,
            } => {
                let surface_indices = self.bind_surface_indices(schema)?;
                self.recheck_ready(stream, revision, set, schema, &surface_indices, paint)
            }
        }
    }

    fn surface_index(
        schema: &[SurfaceInputPortId],
        requirement: &ExactOccurrenceRequirementV1,
    ) -> Result<usize, RecheckProtocolErrorV1> {
        schema
            .binary_search(&requirement.surface)
            .map_err(|_| RecheckProtocolErrorV1::MissingSurfacePort(requirement.surface))
    }

    fn validate_surface_schema(
        &self,
        schema: &[SurfaceInputPortId],
    ) -> Result<(), RecheckProtocolErrorV1> {
        for requirement in &self.occurrences {
            Self::surface_index(schema, requirement)?;
        }
        Ok(())
    }

    fn bind_surface_indices(
        &self,
        schema: &[SurfaceInputPortId],
    ) -> Result<Vec<usize>, RecheckProtocolErrorV1> {
        let mut surface_indices = Vec::new();
        surface_indices
            .try_reserve_exact(self.occurrences.len())
            .map_err(|_| RecheckProtocolErrorV1::ResourceExhausted)?;
        for requirement in &self.occurrences {
            surface_indices.push(Self::surface_index(schema, requirement)?);
        }
        Ok(surface_indices)
    }

    fn recheck_ready(
        &self,
        stream: ObservationStreamId,
        revision: Revision,
        set: &ObservedScenarioSet,
        schema: &[SurfaceInputPortId],
        surface_indices: &[usize],
        paint: EncodedPaintCandidateV1,
    ) -> Result<FinalRecheckOutcomeV1, RecheckProtocolErrorV1> {
        let mut occurrences = Vec::new();
        let evidence_count = checked_evidence_count(set.cases().len(), self.occurrences.len())?;
        occurrences
            .try_reserve_exact(evidence_count)
            .map_err(|_| RecheckProtocolErrorV1::ResourceExhausted)?;
        for (case_index, case) in set.cases().iter().enumerate() {
            for (requirement, &surface_index) in self.occurrences.iter().zip(surface_indices) {
                let backdrop = case.bindings()[surface_index];
                let occurrence = PointOpacityOverSurfaceV1::evaluate_admitted(
                    paint.source.bytes(),
                    paint.opacity,
                    backdrop.bytes(),
                );
                let evidence = match assess_visible_point_hard(
                    &occurrence,
                    &ExactSrgb8IdentityV1,
                    requirement.invocation,
                ) {
                    Ok(HardDecision::Pass(evidence)) => evidence,
                    Ok(HardDecision::Violation(evidence)) => {
                        return Ok(FinalRecheckOutcomeV1::Violation(ExactViolationRecheckV1 {
                            occurrence: requirement.occurrence,
                            surface: requirement.surface,
                            physical_program: self.physical_program,
                            paint,
                            observation: FrozenObservationV1 {
                                stream,
                                revision,
                                schema: schema.to_vec().into_boxed_slice(),
                                set: set.clone(),
                            },
                            case_index,
                            evidence,
                        }));
                    }
                    Err(error) => match error {},
                };
                occurrences.push(ExactOccurrenceEvidenceV1 {
                    physical_program: self.physical_program,
                    occurrence: requirement.occurrence,
                    surface: requirement.surface,
                    case_index,
                    evidence,
                });
            }
        }

        Ok(FinalRecheckOutcomeV1::Verified(RevisionBoundRecheckV1 {
            requirement: self.clone(),
            paint,
            observation: FrozenObservationV1 {
                stream,
                revision,
                schema: schema.to_vec().into_boxed_slice(),
                set: set.clone(),
            },
            occurrences: occurrences.into_boxed_slice(),
        }))
    }
}

/// Вычисляет точную ёмкость до первого compositing-вызова. Переполнение и
/// отказ allocator-а не могут оставить частичный physical evidence.
pub(crate) fn checked_evidence_count(
    physical_cases: usize,
    requirements: usize,
) -> Result<usize, RecheckProtocolErrorV1> {
    physical_cases
        .checked_mul(requirements)
        .ok_or(RecheckProtocolErrorV1::ResourceExhausted)
}

/// Уже заданный encoded-sRGB8 Paint. Он не является selected/admitted output;
/// право на revision-bound verified evidence появляется только после recheck.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EncodedPaintCandidateV1 {
    id: PaintId,
    source: Srgb8,
    opacity: AdmittedOpacityV1,
}

impl EncodedPaintCandidateV1 {
    pub(crate) fn new(
        id: PaintId,
        source: Srgb8,
        opacity: f64,
    ) -> Result<Self, OpacityAdmissionErrorV1> {
        let opacity = AdmittedOpacityV1::new(opacity)?;
        Ok(Self {
            id,
            source,
            opacity,
        })
    }

    pub(crate) const fn id(self) -> PaintId {
        self.id
    }

    pub(crate) const fn source(self) -> Srgb8 {
        self.source
    }

    #[cfg(test)]
    pub(crate) const fn opacity_bits(self) -> u64 {
        self.opacity.bits()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrozenObservationV1 {
    stream: ObservationStreamId,
    revision: Revision,
    schema: Box<[SurfaceInputPortId]>,
    set: ObservedScenarioSet,
}

impl FrozenObservationV1 {
    pub(crate) const fn stream(&self) -> ObservationStreamId {
        self.stream
    }

    pub(crate) const fn revision(&self) -> Revision {
        self.revision
    }
}

/// Evidence одного действительно вызванного evaluator-а над одним финальным
/// occurrence. Emitted Paint хранится отдельно в [`RevisionBoundRecheckV1`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExactOccurrenceEvidenceV1 {
    physical_program: PhysicalProgramIdentityV1,
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    case_index: usize,
    evidence: ExactPassEvidenceV1,
}

impl ExactOccurrenceEvidenceV1 {
    pub(crate) const fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }

    pub(crate) const fn surface(&self) -> SurfaceInputPortId {
        self.surface
    }

    pub(crate) const fn physical_program(&self) -> PhysicalProgramIdentityV1 {
        self.physical_program
    }

    pub(crate) fn program_occurrence_binding(
        &self,
    ) -> crate::appearance::ProgramOccurrenceBindingV1 {
        self.evidence.binding().program_occurrence()
    }

    pub(crate) fn constraint(&self) -> ExactConstraintIdentityV1 {
        *self.evidence.identity()
    }

    pub(crate) fn release(&self) -> ExactIdentityReleaseV1 {
        *self.evidence.release()
    }

    pub(crate) fn capability(&self) -> ExactIdentityCapabilityV1 {
        *self.evidence.capability()
    }

    pub(crate) fn invocation(&self) -> Srgb8 {
        *self.evidence.invocation()
    }

    pub(crate) fn target(&self) -> Srgb8 {
        self.evidence.target()
    }

    pub(crate) fn actual(&self) -> Srgb8 {
        self.evidence.actual()
    }

    pub(crate) fn physical_certificate(&self) -> SourceOverCertificateV1 {
        self.evidence.binding().occurrence()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WaitingRecheckV1 {
    stream: ObservationStreamId,
    revision: Option<Revision>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExactViolationRecheckV1 {
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    physical_program: PhysicalProgramIdentityV1,
    paint: EncodedPaintCandidateV1,
    observation: FrozenObservationV1,
    case_index: usize,
    evidence: ExactViolationEvidenceV1,
}

impl ExactViolationRecheckV1 {
    pub(crate) const fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }

    pub(crate) const fn surface(&self) -> SurfaceInputPortId {
        self.surface
    }

    pub(crate) fn provenance(&self) -> &[ScenarioId] {
        self.observation.set.cases()[self.case_index].provenance()
    }

    pub(crate) const fn physical_program(&self) -> PhysicalProgramIdentityV1 {
        self.physical_program
    }

    pub(crate) const fn paint(&self) -> EncodedPaintCandidateV1 {
        self.paint
    }

    pub(crate) fn physical_certificate(&self) -> SourceOverCertificateV1 {
        self.evidence.binding().occurrence()
    }

    pub(crate) fn invocation(&self) -> Srgb8 {
        *self.evidence.invocation()
    }

    pub(crate) fn constraint(&self) -> ExactConstraintIdentityV1 {
        *self.evidence.identity()
    }

    pub(crate) fn release(&self) -> ExactIdentityReleaseV1 {
        *self.evidence.release()
    }

    pub(crate) fn capability(&self) -> ExactIdentityCapabilityV1 {
        *self.evidence.capability()
    }

    pub(crate) fn target(&self) -> Srgb8 {
        self.evidence.target()
    }

    pub(crate) fn actual(&self) -> Srgb8 {
        self.evidence.actual()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StaleRecheckV1 {
    requirement: CompiledFixedRecheckV1,
    paint: EncodedPaintCandidateV1,
    stream: ObservationStreamId,
    schema: Box<[SurfaceInputPortId]>,
    current_revision: Revision,
    previous: PriorObservation,
}

impl StaleRecheckV1 {
    /// Presentation может сохранить только прежнее verified evidence. Метод не
    /// вызывает evaluator, не читает `PriorObservation` как Ready и не создаёт
    /// current claim.
    pub(crate) fn hold(
        &self,
        previous: &RevisionBoundRecheckV1,
    ) -> Result<PresentationHoldV1, HoldErrorV1> {
        if previous.requirement != self.requirement
            || previous.paint != self.paint
            || previous.observation.stream != self.stream
            || previous.observation.schema != self.schema
            || previous.observation.revision != self.previous.revision()
            || &previous.observation.set != self.previous.set()
        {
            return Err(HoldErrorV1::PreviousEvidenceMismatch);
        }
        Ok(PresentationHoldV1 {
            previous: previous.clone(),
            current_revision: self.current_revision,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PresentationHoldV1 {
    previous: RevisionBoundRecheckV1,
    current_revision: Revision,
}

impl PresentationHoldV1 {
    pub(crate) fn previous(&self) -> &RevisionBoundRecheckV1 {
        &self.previous
    }

    pub(crate) const fn current_revision(&self) -> Revision {
        self.current_revision
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HoldErrorV1 {
    PreviousEvidenceMismatch,
}

/// Максимальный честный результат V1a: fixed Paint и все его exact final
/// occurrences связаны с одной неизменяемой stream revision. Терминальные
/// consumer-owned identities намеренно отсутствуют.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RevisionBoundRecheckV1 {
    requirement: CompiledFixedRecheckV1,
    paint: EncodedPaintCandidateV1,
    observation: FrozenObservationV1,
    occurrences: Box<[ExactOccurrenceEvidenceV1]>,
}

impl RevisionBoundRecheckV1 {
    pub(crate) const fn paint(&self) -> EncodedPaintCandidateV1 {
        self.paint
    }

    pub(crate) const fn observation(&self) -> &FrozenObservationV1 {
        &self.observation
    }

    pub(crate) fn occurrences(&self) -> &[ExactOccurrenceEvidenceV1] {
        &self.occurrences
    }

    pub(crate) fn provenance(&self, evidence_index: usize) -> Option<&[ScenarioId]> {
        let case_index = self.occurrences.get(evidence_index)?.case_index;
        self.observation
            .set
            .cases()
            .get(case_index)
            .map(|case| case.provenance())
    }

    pub(crate) fn reuse_for(
        &self,
        requirement: &CompiledFixedRecheckV1,
        observation: &ObservationState,
        paint: EncodedPaintCandidateV1,
    ) -> Result<(), ReuseErrorV1> {
        if &self.requirement != requirement {
            return Err(ReuseErrorV1::RequirementMismatch);
        }
        if self.paint != paint {
            return Err(ReuseErrorV1::PaintMismatch);
        }
        match observation.snapshot() {
            ObservationSnapshot::Ready {
                stream,
                schema,
                revision,
                set,
            } if stream == self.observation.stream
                && schema == self.observation.schema.as_ref()
                && revision == self.observation.revision
                && set == &self.observation.set =>
            {
                Ok(())
            }
            ObservationSnapshot::Ready { .. } => Err(ReuseErrorV1::ObservationMismatch),
            ObservationSnapshot::Waiting { .. } | ObservationSnapshot::Stale { .. } => {
                Err(ReuseErrorV1::ObservationUnavailable)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FinalRecheckOutcomeV1 {
    Waiting(WaitingRecheckV1),
    Stale(StaleRecheckV1),
    Violation(ExactViolationRecheckV1),
    Verified(RevisionBoundRecheckV1),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RecheckProtocolErrorV1 {
    PaintMismatch { expected: PaintId, actual: PaintId },
    MissingSurfacePort(SurfaceInputPortId),
    ResourceExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReuseErrorV1 {
    RequirementMismatch,
    PaintMismatch,
    ObservationMismatch,
    ObservationUnavailable,
}
