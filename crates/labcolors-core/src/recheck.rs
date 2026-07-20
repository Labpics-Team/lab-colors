//! Narrow revision-bound recheck одного уже заданного point Paint.
//!
//! Модуль не владеет raw admission или lifecycle. Construction один раз связывает
//! compiled requirements с immutable surface schema и fixed Paint; recheck принимает
//! только admitted [`RevisionBoundObservationV1`] и возвращает `Verified | Violation`.
//! `Waiting | Ready | Stale | Failed` принадлежат Session.

use crate::Srgb8;
use crate::appearance::{
    EncodedPointPaintV1, OccurrenceId, PaintId, PhysicalProgramIdentityV1,
    PointOpacityOverSurfaceV1, SourceOverCertificateV1, SurfaceInputPortId,
};
use crate::constraints::{
    ExactConstraintIdentityV1, ExactIdentityCapabilityV1, ExactIdentityReleaseV1,
    ExactPassEvidenceV1, ExactSrgb8IdentityV1, ExactViolationEvidenceV1, HardDecision,
    assess_visible_point_hard,
};
use crate::observation::{RevisionBoundObservationV1, ScenarioId};

/// Один immutable exact evaluator invocation, связанный с authored occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ExactOccurrenceRequirementV1 {
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    invocation: Srgb8,
}

impl ExactOccurrenceRequirementV1 {
    pub(crate) const fn new(
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

    /// Construction-time binding. После него update не ищет IDs и не принимает
    /// candidate/schema заново.
    pub(crate) fn bind(
        self,
        schema: &[SurfaceInputPortId],
        paint: EncodedPointPaintV1,
    ) -> Result<BoundFixedRecheckV1, FixedRecheckBindErrorV1> {
        if paint.id() != self.paint {
            return Err(FixedRecheckBindErrorV1::PaintMismatch {
                expected: self.paint,
                actual: paint.id(),
            });
        }
        let mut surface_indices = Vec::new();
        surface_indices
            .try_reserve_exact(self.occurrences.len())
            .map_err(|_| FixedRecheckBindErrorV1::ResourceExhausted)?;
        for requirement in &self.occurrences {
            let index = schema
                .binary_search(&requirement.surface)
                .map_err(|_| FixedRecheckBindErrorV1::MissingSurfacePort(requirement.surface))?;
            surface_indices.push(index);
        }
        Ok(BoundFixedRecheckV1 {
            requirement: self,
            paint,
            schema: schema.to_vec().into_boxed_slice(),
            surface_indices: surface_indices.into_boxed_slice(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FixedRecheckBindErrorV1 {
    PaintMismatch { expected: PaintId, actual: PaintId },
    MissingSurfacePort(SurfaceInputPortId),
    ResourceExhausted,
}

/// Prebound fixed-candidate execution plan. Его constructors принадлежат только
/// validated compiled requirement + Session schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundFixedRecheckV1 {
    requirement: CompiledFixedRecheckV1,
    paint: EncodedPointPaintV1,
    schema: Box<[SurfaceInputPortId]>,
    surface_indices: Box<[usize]>,
}

impl BoundFixedRecheckV1 {
    pub(crate) const fn paint(&self) -> EncodedPointPaintV1 {
        self.paint
    }

    pub(crate) fn recheck(
        &self,
        observation: RevisionBoundObservationV1,
    ) -> Result<FixedRecheckDecisionV1, RecheckProtocolErrorV1> {
        if observation.schema() != self.schema.as_ref() {
            return Err(RecheckProtocolErrorV1::ObservationSchemaMismatch);
        }
        let set = observation.set();
        let evidence_count =
            checked_evidence_count(set.cases().len(), self.requirement.occurrences.len())?;
        let mut occurrences = Vec::new();
        occurrences
            .try_reserve_exact(evidence_count)
            .map_err(|_| RecheckProtocolErrorV1::ResourceExhausted)?;

        for (case_index, case) in set.cases().iter().enumerate() {
            for (requirement, &surface_index) in self
                .requirement
                .occurrences
                .iter()
                .zip(self.surface_indices.iter())
            {
                let backdrop = case.bindings()[surface_index];
                let occurrence = PointOpacityOverSurfaceV1::evaluate_admitted(
                    self.paint.source().bytes(),
                    self.paint.opacity(),
                    backdrop.bytes(),
                );
                let evidence = match assess_visible_point_hard(
                    &occurrence,
                    &ExactSrgb8IdentityV1,
                    requirement.invocation,
                ) {
                    Ok(HardDecision::Pass(evidence)) => evidence,
                    Ok(HardDecision::Violation(evidence)) => {
                        return Ok(FixedRecheckDecisionV1::Violation(ExactViolationRecheckV1 {
                            occurrence: requirement.occurrence,
                            surface: requirement.surface,
                            physical_program: self.requirement.physical_program,
                            paint: self.paint,
                            observation: observation.clone(),
                            case_index,
                            evidence,
                        }));
                    }
                    Err(error) => match error {},
                };
                occurrences.push(ExactOccurrenceEvidenceV1 {
                    physical_program: self.requirement.physical_program,
                    occurrence: requirement.occurrence,
                    surface: requirement.surface,
                    case_index,
                    evidence,
                });
            }
        }

        Ok(FixedRecheckDecisionV1::Verified(RevisionBoundRecheckV1 {
            requirement: self.requirement.clone(),
            paint: self.paint,
            observation,
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
pub(crate) struct ExactViolationRecheckV1 {
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    physical_program: PhysicalProgramIdentityV1,
    paint: EncodedPointPaintV1,
    observation: RevisionBoundObservationV1,
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
        self.observation.set().cases()[self.case_index].provenance()
    }

    pub(crate) const fn physical_program(&self) -> PhysicalProgramIdentityV1 {
        self.physical_program
    }

    pub(crate) const fn paint(&self) -> EncodedPointPaintV1 {
        self.paint
    }

    pub(crate) const fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
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

/// Максимальный честный fixed-candidate результат: Paint и все exact final
/// occurrences связаны с одной immutable stream revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RevisionBoundRecheckV1 {
    requirement: CompiledFixedRecheckV1,
    paint: EncodedPointPaintV1,
    observation: RevisionBoundObservationV1,
    occurrences: Box<[ExactOccurrenceEvidenceV1]>,
}

impl RevisionBoundRecheckV1 {
    pub(crate) const fn paint(&self) -> EncodedPointPaintV1 {
        self.paint
    }

    pub(crate) const fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }

    pub(crate) fn occurrences(&self) -> &[ExactOccurrenceEvidenceV1] {
        &self.occurrences
    }

    pub(crate) fn provenance(&self, evidence_index: usize) -> Option<&[ScenarioId]> {
        let case_index = self.occurrences.get(evidence_index)?.case_index;
        self.observation
            .set()
            .cases()
            .get(case_index)
            .map(|case| case.provenance())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FixedRecheckDecisionV1 {
    Violation(ExactViolationRecheckV1),
    Verified(RevisionBoundRecheckV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecheckProtocolErrorV1 {
    ObservationSchemaMismatch,
    ResourceExhausted,
}
