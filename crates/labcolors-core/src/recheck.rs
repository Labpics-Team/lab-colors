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
    DisplayReadabilityCurveV1, DisplayReadabilityMeasurementV1, Evaluator,
    ExactConstraintIdentityV1, ExactIdentityCapabilityV1, ExactIdentityReleaseV1,
    ExactPassEvidenceV1, ExactSrgb8IdentityV1, ExactViolationEvidenceV1, HardClassifier,
    HardDecision, ReadabilityPassV1, ReadabilityPolarityV1, ReadabilityViolationV1,
    assess_visible_point_hard,
};
use crate::joint::{
    CandidateOrdinalV1, DeclaredTotalOrderV1, JointCandidateSetV1, PointwiseFullHardReportV1,
    PointwiseHardFeasibilityV1, PointwiseJointPointProgramV1, PointwiseJointReportErrorV1,
    PointwiseSelectedRecheckErrorV1, PointwiseVerifiedSelectionV1, SelectionPolicyErrorV1,
};
use crate::observation::{RevisionBoundObservationV1, ScenarioId};
use crate::solve::Floor;

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
            occurrences,
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
    occurrences: Vec<ExactOccurrenceEvidenceV1>,
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

// Full-support readability recheck bridge (C8d step 2).
//
// The exact recheck above proves backdrop-independent byte identity of one
// fixed Paint. The readability bridge reuses the SAME cases×occurrences loop and
// the SAME `PointOpacityOverSurfaceV1::evaluate_admitted` compositor, but swaps
// the predicate to the frozen display-domain readability curve
// (`DisplayReadabilityCurveV1`, F5) and evaluates a whole role table over the
// whole observed support in one pass. Every role is one occurrence descriptor:
// a solid role carries `AdmittedOpacityV1::OPAQUE` (its composite over any
// backdrop is its own source bytes), a translucent role carries its alpha and
// tint (its composite is the same source-over the solver used). The result is a
// typed per-case-per-occurrence verdict — a hard Pass/Violation against the role
// floor plus a continuous surplus and a typed polarity — so finite / shape /
// polarity are owned by Core, not re-derived by host float guards.
// ---------------------------------------------------------------------------

/// One role modeled as a single readability occurrence descriptor. Solid and
/// translucent share this exact shape: the [opacity](EncodedPointPaintV1) the
/// `paint` carries is the only thing that distinguishes them —
/// [`AdmittedOpacityV1::OPAQUE`](crate::composition::AdmittedOpacityV1::OPAQUE)
/// for solid (visible == source bytes over any backdrop), an admitted
/// `alpha < 1` for translucent (visible == the source-over composite).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ReadabilityOccurrenceV1 {
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    paint: EncodedPointPaintV1,
    floor: Floor,
}

impl ReadabilityOccurrenceV1 {
    /// One unified descriptor. `paint`'s opacity is the solid/translucent
    /// discriminator; `floor` is the role's readability floor the curve is
    /// classified against.
    pub(crate) const fn new(
        occurrence: OccurrenceId,
        surface: SurfaceInputPortId,
        paint: EncodedPointPaintV1,
        floor: Floor,
    ) -> Self {
        Self {
            occurrence,
            surface,
            paint,
            floor,
        }
    }
}

/// Canonical compiled requirement of a whole role table's readability recheck:
/// a de-duplicated, occurrence-sorted set of unified descriptors bound to one
/// physical program identity.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompiledReadabilityRecheckV1 {
    occurrences: Box<[ReadabilityOccurrenceV1]>,
}

impl CompiledReadabilityRecheckV1 {
    pub(crate) fn new(
        mut occurrences: Vec<ReadabilityOccurrenceV1>,
    ) -> Result<Self, FixedRecheckCompileErrorV1> {
        if occurrences.is_empty() {
            return Err(FixedRecheckCompileErrorV1::EmptyOccurrences);
        }
        occurrences.sort_unstable_by_key(|descriptor| descriptor.occurrence);
        if let Some(duplicate) = occurrences
            .windows(2)
            .find(|window| window[0].occurrence == window[1].occurrence)
        {
            return Err(FixedRecheckCompileErrorV1::DuplicateOccurrence(
                duplicate[0].occurrence,
            ));
        }
        Ok(Self {
            occurrences: occurrences.into_boxed_slice(),
        })
    }

    /// Construction-time binding: resolve each descriptor's surface port to an
    /// index into the immutable observation schema, once. No per-update ID
    /// search remains.
    pub(crate) fn bind(
        self,
        schema: &[SurfaceInputPortId],
    ) -> Result<BoundReadabilityRecheckV1, FixedRecheckBindErrorV1> {
        let mut surface_indices = Vec::new();
        surface_indices
            .try_reserve_exact(self.occurrences.len())
            .map_err(|_| FixedRecheckBindErrorV1::ResourceExhausted)?;
        for descriptor in &self.occurrences {
            let index = schema
                .binary_search(&descriptor.surface)
                .map_err(|_| FixedRecheckBindErrorV1::MissingSurfacePort(descriptor.surface))?;
            surface_indices.push(index);
        }
        Ok(BoundReadabilityRecheckV1 {
            requirement: self,
            schema: schema.to_vec().into_boxed_slice(),
            surface_indices: surface_indices.into_boxed_slice(),
        })
    }
}

/// Prebound full-support readability execution plan.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BoundReadabilityRecheckV1 {
    requirement: CompiledReadabilityRecheckV1,
    schema: Box<[SurfaceInputPortId]>,
    surface_indices: Box<[usize]>,
}

impl BoundReadabilityRecheckV1 {
    /// Recheck every descriptor over every observed case. Unlike the exact
    /// recheck, this does NOT stop at the first violation: the whole support is
    /// evaluated and every typed verdict is retained, so the controller can
    /// reduce a Pointwise-OR breach and pick a re-solve witness from the
    /// complete picture.
    pub(crate) fn recheck(
        &self,
        observation: RevisionBoundObservationV1,
    ) -> Result<ReadabilityRecheckReportV1, RecheckProtocolErrorV1> {
        if observation.schema() != self.schema.as_ref() {
            return Err(RecheckProtocolErrorV1::ObservationSchemaMismatch);
        }
        let set = observation.set();
        let verdict_count =
            checked_evidence_count(set.cases().len(), self.requirement.occurrences.len())?;
        let mut verdicts = Vec::new();
        verdicts
            .try_reserve_exact(verdict_count)
            .map_err(|_| RecheckProtocolErrorV1::ResourceExhausted)?;

        let evaluator = DisplayReadabilityCurveV1;
        for (case_index, case) in set.cases().iter().enumerate() {
            for (descriptor, &surface_index) in self
                .requirement
                .occurrences
                .iter()
                .zip(self.surface_indices.iter())
            {
                let backdrop = case.bindings()[surface_index];
                let occurrence = PointOpacityOverSurfaceV1::evaluate_admitted(
                    descriptor.paint.source().bytes(),
                    descriptor.paint.opacity(),
                    backdrop.bytes(),
                );
                let modeled = occurrence.modeled_srgb8_point();
                let measurement = match evaluator.evaluate(&modeled, &descriptor.floor) {
                    Ok(measurement) => measurement,
                    Err(error) => match error {},
                };
                let decision = evaluator.classify(&descriptor.floor, &measurement);
                verdicts.push(ReadabilityCaseVerdictV1 {
                    occurrence: descriptor.occurrence,
                    surface: descriptor.surface,
                    case_index,
                    measurement,
                    decision,
                });
            }
        }

        Ok(ReadabilityRecheckReportV1 {
            observation,
            verdicts,
        })
    }
}

/// One typed readability verdict of one descriptor against one observed case.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ReadabilityCaseVerdictV1 {
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    case_index: usize,
    measurement: DisplayReadabilityMeasurementV1,
    decision: HardDecision<ReadabilityPassV1, ReadabilityViolationV1>,
}

impl ReadabilityCaseVerdictV1 {
    pub(crate) const fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }

    pub(crate) const fn surface(&self) -> SurfaceInputPortId {
        self.surface
    }

    pub(crate) const fn measurement(&self) -> DisplayReadabilityMeasurementV1 {
        self.measurement
    }

    pub(crate) const fn decision(&self) -> HardDecision<ReadabilityPassV1, ReadabilityViolationV1> {
        self.decision
    }

    pub(crate) const fn is_violation(&self) -> bool {
        matches!(self.decision, HardDecision::Violation(_))
    }

    /// The continuous signed distance from the role floor (`>= 0` on a pass,
    /// `< 0` on a violation) — the scalar the controller thresholds for its
    /// `dropFraction` hysteresis.
    pub(crate) const fn surplus(&self) -> f64 {
        match self.decision {
            HardDecision::Pass(pass) => pass.surplus(),
            HardDecision::Violation(violation) => violation.surplus(),
        }
    }

    /// The typed polarity — decided in Core from the sign of the signed
    /// candidate curve, never a host `Math.abs`.
    pub(crate) const fn polarity(&self) -> ReadabilityPolarityV1 {
        match self.decision {
            HardDecision::Pass(pass) => pass.polarity(),
            HardDecision::Violation(violation) => violation.polarity(),
        }
    }
}

/// The full-support readability report: every typed verdict bound to one
/// immutable observation revision.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReadabilityRecheckReportV1 {
    observation: RevisionBoundObservationV1,
    verdicts: Vec<ReadabilityCaseVerdictV1>,
}

impl ReadabilityRecheckReportV1 {
    pub(crate) fn verdicts(&self) -> &[ReadabilityCaseVerdictV1] {
        &self.verdicts
    }

    /// Pointwise-OR breach over the whole support: `true` if any case×occurrence
    /// verdict is a violation.
    pub(crate) fn is_breached(&self) -> bool {
        self.verdicts
            .iter()
            .any(ReadabilityCaseVerdictV1::is_violation)
    }

    /// The observed scenario provenance behind the verdict at `verdict_index`.
    pub(crate) fn provenance(&self, verdict_index: usize) -> Option<&[ScenarioId]> {
        let case_index = self.verdicts.get(verdict_index)?.case_index;
        self.observation
            .set()
            .cases()
            .get(case_index)
            .map(|case| case.provenance())
    }
}

// ---------------------------------------------------------------------------
// V2a joint feasible-across-all-samples re-solve bridge (C8d step 3).
//
// The full-support recheck above proves ONE already-chosen candidate over the
// whole support. This bridge wires the generic V2a joint selection (joint.rs) so
// a re-solve returns exactly ONE candidate that is hard-feasible across the WHOLE
// observed scenario set — every backdrop sample admitted as its own case — or a
// typed Indeterminate when no jointly-feasible tuple exists. It never returns a
// candidate that breaks a sample: `classify` demands every case pass, so the
// least-margin / worst sample is only ever a post-hoc diagnostic witness, never a
// solve input (the Pointwise every-case law, DAG mandate `V2a + F2 -> C8d`).
//
// ANTI-DRIFT: joint.rs stays generic. The readability semantics live entirely in
// `DisplayReadabilityCurveV1` (constraints/readability.rs); this bridge only
// INSTANTIATES the existing generic `PointwiseJointPointProgramV1<E>` with
// `E = DisplayReadabilityCurveV1`, exactly as the exact path instantiates it with
// `ExactSrgb8IdentityV1`. No readability enum or import ever crosses into
// joint.rs — the readability curve reaches the joint engine only through the same
// sealed `Evaluator` + `HardClassifier` seam every other predicate uses.
// ---------------------------------------------------------------------------

/// The readability specialisation of the generic two-paint joint program: the
/// same `PointwiseJointPointProgramV1<E>` the exact path uses, instantiated with
/// the frozen display-domain readability curve as `E`.
pub(crate) type ReadabilityJointProgramV1 = PointwiseJointPointProgramV1<DisplayReadabilityCurveV1>;

/// The full hard report of a readability joint program over one immutable
/// observation revision.
pub(crate) type ReadabilityJointReportV1 =
    PointwiseFullHardReportV1<DisplayReadabilityCurveV1, RevisionBoundObservationV1>;

/// One re-verified readability joint selection: exactly one candidate, freshly
/// re-executed across every admitted sample on the same revision.
pub(crate) type ReadabilityJointVerifiedSelectionV1 =
    PointwiseVerifiedSelectionV1<DisplayReadabilityCurveV1, RevisionBoundObservationV1>;

/// Typed outcome of a joint readability re-solve evaluated across the WHOLE
/// observed scenario set.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum JointReadabilityResolutionV1 {
    /// Exactly one candidate, jointly feasible across every admitted backdrop
    /// sample and freshly re-verified on the same observation revision before it
    /// is handed back. A set-breaching candidate can never reach this variant.
    Feasible(Box<ReadabilityJointVerifiedSelectionV1>),
    /// No jointly-feasible candidate exists. The full hard report is retained so
    /// the breaching sample(s) can be identified after the fact — a diagnostic
    /// witness only, never fed back into the solve as a target.
    Indeterminate(Box<ReadabilityJointReportV1>),
}

/// Why a joint readability re-solve could not produce a typed resolution.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum JointReadabilityResolveErrorV1 {
    /// The joint program rejected the candidate domain or observation before any
    /// feasibility verdict could be formed.
    Report(PointwiseJointReportErrorV1<DisplayReadabilityCurveV1>),
    /// The declared tie-break was not a total order over the candidate domain.
    Policy(SelectionPolicyErrorV1),
    /// A candidate that classified feasible failed its fresh re-verify — an
    /// invariant drift between the selection pass and the recheck pass.
    ReverifyDrift,
    /// Cardinality overflow or an allocator refusal during the fresh re-verify.
    ResourceExhausted,
}

/// Re-solve across the whole observed scenario set: evaluate every candidate
/// tuple over every admitted backdrop sample, keep only the candidates that pass
/// EVERY sample, tie-break by the declared total order, then freshly re-verify
/// the winner across the whole set before returning it. When no candidate passes
/// every sample the result is a typed [`JointReadabilityResolutionV1::Indeterminate`],
/// never a target that breaks a sample.
pub(crate) fn resolve_across_all_samples(
    program: &ReadabilityJointProgramV1,
    candidates: JointCandidateSetV1,
    observation: RevisionBoundObservationV1,
    order: Vec<CandidateOrdinalV1>,
) -> Result<JointReadabilityResolutionV1, JointReadabilityResolveErrorV1> {
    let report = program
        .evaluate(candidates, observation)
        .map_err(JointReadabilityResolveErrorV1::Report)?;
    match report.classify() {
        PointwiseHardFeasibilityV1::Infeasible(report) => Ok(
            JointReadabilityResolutionV1::Indeterminate(Box::new(report)),
        ),
        PointwiseHardFeasibilityV1::NonEmpty(feasible) => {
            let policy = DeclaredTotalOrderV1::new(feasible.candidate_set(), order)
                .map_err(JointReadabilityResolveErrorV1::Policy)?;
            match feasible.select(policy).recheck() {
                Ok(verified) => Ok(JointReadabilityResolutionV1::Feasible(Box::new(verified))),
                Err(PointwiseSelectedRecheckErrorV1::Violation(_))
                | Err(PointwiseSelectedRecheckErrorV1::InvariantDrift) => {
                    Err(JointReadabilityResolveErrorV1::ReverifyDrift)
                }
                Err(PointwiseSelectedRecheckErrorV1::ResourceExhausted) => {
                    Err(JointReadabilityResolveErrorV1::ResourceExhausted)
                }
                // The readability curve is `Infallible`, so the evaluator arm is
                // uninhabited: it can never be constructed.
                Err(PointwiseSelectedRecheckErrorV1::Evaluator(error)) => match error {},
            }
        }
    }
}
