//! Private C8d full-support recheck for one already-compiled point role table.
//!
//! This module is not an authoring root. A private compiler lowers the current
//! recipe into fixed Paint occurrences plus requirements; the F2 Session is
//! the only production owner that may combine those requirements with admitted
//! observation revisions. Exact final-emission identity, WCAG and retained
//! stability are three independent axes. A plan must request at least one axis,
//! while an individual occurrence may remain a composition-only cell and
//! translucent support is not forced through exact identity.

use core::cmp::Ordering;

use crate::Srgb8;
use crate::appearance::{
    EncodedPointPaintV1, OccurrenceId, PaintId, PhysicalProgramIdentityV1,
    PointOpacityOverSurfaceV1, SourceOverCertificateV1, SurfaceInputPortId,
};
use crate::composition::CompositionProfileV1;
use crate::constraints::{
    ExactPassEvidenceV1, ExactSrgb8IdentityV1, ExactViolationEvidenceV1, HardDecision,
    VisiblePointPassEvidence, VisiblePointViolationEvidence, Wcag22Srgb8V1,
    assess_visible_point_hard,
};
use crate::numerics::{
    NumericalArtifactIdV2, NumericalBoundStatusV2, NumericalErrorBoundIdV2,
    NumericalEvidenceClassV2, NumericalFallbackStatusV1, NumericalProofIdV2, NumericalSiteIdV2,
    StableNumericalOutcomeV2, numerical_registry_v2,
};
use crate::observation::{ObservationSchemaMismatchV1, RevisionBoundObservationV1, ScenarioId};
use crate::session::SessionObservationBindingPermitV1;
use crate::wcag22::{Wcag22CriterionV1, Wcag22MeasurementV1, measure_wcag22_srgb8};

const DROP_BASIS_POINTS_SCALE: u16 = 10_000;
const STABILITY_SITE_ID: NumericalSiteIdV2 =
    NumericalSiteIdV2::PointSupportRetainedReferenceSurplusV1;
const STABILITY_ARTIFACT_ID: NumericalArtifactIdV2 =
    NumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55V1;
const STABILITY_BOUND_ID: NumericalErrorBoundIdV2 =
    NumericalErrorBoundIdV2::PointSupportReferenceSurplusQ55BpsV1;
const STABILITY_PROOF_ID: NumericalProofIdV2 =
    NumericalProofIdV2::PointSupportReferenceSurplusIntegerV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointSupportCriterionRequirementV1 {
    NotRequested,
    Required(Wcag22CriterionV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointSupportStabilityAnchorV1 {
    Identity1,
    Ratio3,
    Ratio4Point5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSupportDropFractionV1(u16);

impl PointSupportDropFractionV1 {
    pub(crate) const NONE: Self = Self(0);
    pub(crate) const ALL: Self = Self(DROP_BASIS_POINTS_SCALE);

    pub(crate) fn try_from_basis_points(
        basis_points: u32,
    ) -> Result<Self, PointSupportCompileErrorV1> {
        if basis_points > u32::from(DROP_BASIS_POINTS_SCALE) {
            return Err(PointSupportCompileErrorV1::DropFractionOutsideBasisPointRange);
        }
        Ok(Self(basis_points as u16))
    }

    pub(crate) const fn basis_points(self) -> u16 {
        self.0
    }

    const fn retained_basis_points(self) -> u16 {
        DROP_BASIS_POINTS_SCALE - self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointSupportStabilityPolicyV1 {
    Disabled,
    /// The declared anchor remains a hard floor. The drop fraction applies only
    /// to positive conservative baseline surplus above that floor.
    RetainBaselineReferenceSurplus {
        baseline_backdrop: Srgb8,
        anchor: PointSupportStabilityAnchorV1,
        drop_fraction: PointSupportDropFractionV1,
    },
}

/// One occurrence lowered from the private compiled program. The Paint is the
/// already-admitted shared physical value, never a second source/opacity DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSupportOccurrenceRequirementV1 {
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    paint: EncodedPointPaintV1,
    exact_invocation: Option<Srgb8>,
    criterion: PointSupportCriterionRequirementV1,
    stability: PointSupportStabilityPolicyV1,
}

impl PointSupportOccurrenceRequirementV1 {
    pub(crate) const fn new(
        occurrence: OccurrenceId,
        surface: SurfaceInputPortId,
        paint: EncodedPointPaintV1,
        exact_invocation: Option<Srgb8>,
        criterion: PointSupportCriterionRequirementV1,
        stability: PointSupportStabilityPolicyV1,
    ) -> Self {
        Self {
            occurrence,
            surface,
            paint,
            exact_invocation,
            criterion,
            stability,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PointSupportCompileErrorV1 {
    EmptyOccurrences,
    InactivePlan,
    DuplicateOccurrence(OccurrenceId),
    PaintDefinitionMismatch(PaintId),
    DropFractionOutsideBasisPointRange,
    CompositionProfileMismatch {
        expected: CompositionProfileV1,
        actual: CompositionProfileV1,
    },
    SurfaceSchemaInvariant,
    ResourceExhausted,
    StabilityArithmeticInvariant,
    NumericalRegistryInvariant,
}

/// Private compiler output for one role table. It owns its canonical surface
/// schema, but preserves declared occurrence order because that order is the
/// client-owned packed ordinal and witness order. Prebound surface indices are
/// only a cache: runtime keyed bindings remain truth and every cached position
/// is checked against its exact port before use.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct CompiledPointSupportRecheckV1 {
    physical_program: PhysicalProgramIdentityV1,
    composition_profile: CompositionProfileV1,
    occurrences: Vec<PointSupportOccurrenceRequirementV1>,
    surface_schema: Vec<SurfaceInputPortId>,
    surface_indices: Vec<usize>,
    baselines: Vec<Option<EnabledBaselineV1>>,
}

impl CompiledPointSupportRecheckV1 {
    pub(crate) fn new(
        composition_profile: CompositionProfileV1,
        occurrences: Vec<PointSupportOccurrenceRequirementV1>,
    ) -> Result<Self, PointSupportCompileErrorV1> {
        if occurrences.is_empty() {
            return Err(PointSupportCompileErrorV1::EmptyOccurrences);
        }
        if occurrences.iter().all(|requirement| {
            requirement.exact_invocation.is_none()
                && matches!(
                    requirement.criterion,
                    PointSupportCriterionRequirementV1::NotRequested
                )
                && matches!(
                    requirement.stability,
                    PointSupportStabilityPolicyV1::Disabled
                )
        }) {
            return Err(PointSupportCompileErrorV1::InactivePlan);
        }

        let mut paint_definitions = Vec::new();
        paint_definitions
            .try_reserve_exact(occurrences.len())
            .map_err(|_| PointSupportCompileErrorV1::ResourceExhausted)?;
        paint_definitions.extend(occurrences.iter().map(|occurrence| occurrence.paint));
        paint_definitions.sort_unstable_by_key(|paint| paint.id());
        if let Some(drift) = paint_definitions
            .windows(2)
            .find(|window| window[0].id() == window[1].id() && window[0] != window[1])
        {
            return Err(PointSupportCompileErrorV1::PaintDefinitionMismatch(
                drift[0].id(),
            ));
        }

        let mut occurrence_identities = Vec::new();
        occurrence_identities
            .try_reserve_exact(occurrences.len())
            .map_err(|_| PointSupportCompileErrorV1::ResourceExhausted)?;
        occurrence_identities.extend(occurrences.iter().map(|occurrence| occurrence.occurrence));
        occurrence_identities.sort_unstable();
        if let Some(duplicate) = occurrence_identities
            .windows(2)
            .find(|window| window[0] == window[1])
        {
            return Err(PointSupportCompileErrorV1::DuplicateOccurrence(
                duplicate[0],
            ));
        }

        let actual_profile = PointOpacityOverSurfaceV1::composition_profile();
        if composition_profile != actual_profile {
            return Err(PointSupportCompileErrorV1::CompositionProfileMismatch {
                expected: actual_profile,
                actual: composition_profile,
            });
        }

        let mut surface_schema = Vec::new();
        surface_schema
            .try_reserve_exact(occurrences.len())
            .map_err(|_| PointSupportCompileErrorV1::ResourceExhausted)?;
        surface_schema.extend(occurrences.iter().map(|occurrence| occurrence.surface));
        surface_schema.sort_unstable();
        surface_schema.dedup();

        let mut surface_indices = Vec::new();
        surface_indices
            .try_reserve_exact(occurrences.len())
            .map_err(|_| PointSupportCompileErrorV1::ResourceExhausted)?;
        for occurrence in &occurrences {
            let index = surface_schema
                .binary_search(&occurrence.surface)
                .map_err(|_| PointSupportCompileErrorV1::SurfaceSchemaInvariant)?;
            surface_indices.push(index);
        }

        let mut baselines = Vec::new();
        baselines
            .try_reserve_exact(occurrences.len())
            .map_err(|_| PointSupportCompileErrorV1::ResourceExhausted)?;
        let numerical_evidence = occurrences
            .iter()
            .any(|occurrence| {
                matches!(
                    occurrence.stability,
                    PointSupportStabilityPolicyV1::RetainBaselineReferenceSurplus { .. }
                )
            })
            .then(mint_stability_evidence)
            .transpose()?;

        // Every fallible allocation, index prebind and registry check has
        // completed before the first baseline composition.
        for occurrence in &occurrences {
            let baseline = match occurrence.stability {
                PointSupportStabilityPolicyV1::Disabled => None,
                PointSupportStabilityPolicyV1::RetainBaselineReferenceSurplus {
                    baseline_backdrop,
                    anchor,
                    drop_fraction,
                } => {
                    let physical = PointOpacityOverSurfaceV1::evaluate_admitted(
                        occurrence.paint.source().bytes(),
                        occurrence.paint.opacity(),
                        baseline_backdrop.bytes(),
                    );
                    let composition = *physical.certificate();
                    let measurement =
                        measure_wcag22_srgb8(composition.output_rgb(), composition.backdrop_rgb());
                    let distance = reference_distance(measurement)
                        .map_err(|_| PointSupportCompileErrorV1::StabilityArithmeticInvariant)?;
                    let surplus = anchor_surplus(anchor, distance)
                        .map_err(|_| PointSupportCompileErrorV1::StabilityArithmeticInvariant)?;
                    Some(EnabledBaselineV1 {
                        composition,
                        measurement,
                        distance,
                        anchor,
                        drop_fraction,
                        surplus,
                        numerical_evidence: numerical_evidence
                            .ok_or(PointSupportCompileErrorV1::NumericalRegistryInvariant)?,
                    })
                }
            };
            baselines.push(baseline);
        }

        Ok(Self {
            physical_program: PointOpacityOverSurfaceV1::physical_identity(),
            composition_profile,
            occurrences,
            surface_schema,
            surface_indices,
            baselines,
        })
    }

    pub(crate) fn surface_schema(&self) -> &[SurfaceInputPortId] {
        &self.surface_schema
    }

    pub(crate) fn into_session_recheck(self) -> BoundPointSupportRecheckV1 {
        let Self {
            physical_program,
            composition_profile,
            occurrences,
            surface_schema,
            surface_indices,
            baselines,
        } = self;
        BoundPointSupportRecheckV1 {
            physical_program,
            composition_profile,
            occurrences,
            surface_schema,
            surface_indices,
            baselines,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct BoundPointSupportRecheckV1 {
    physical_program: PhysicalProgramIdentityV1,
    composition_profile: CompositionProfileV1,
    occurrences: Vec<PointSupportOccurrenceRequirementV1>,
    surface_schema: Vec<SurfaceInputPortId>,
    surface_indices: Vec<usize>,
    baselines: Vec<Option<EnabledBaselineV1>>,
}

impl BoundPointSupportRecheckV1 {
    pub(crate) const fn composition_profile(&self) -> CompositionProfileV1 {
        self.composition_profile
    }

    pub(crate) fn surface_schema(&self) -> &[SurfaceInputPortId] {
        &self.surface_schema
    }

    pub(crate) fn evaluate(
        &self,
        observation: RevisionBoundObservationV1,
        _permit: SessionObservationBindingPermitV1,
    ) -> Result<PointSupportDecisionV1, PointSupportEvaluationErrorV1> {
        let assessment = evaluate_bound_point_support(self, &observation)?;
        Ok(assessment.bind(observation))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PointSupportEvaluationErrorV1 {
    ObservationSchemaMismatch(ObservationSchemaMismatchV1),
    CompiledPlanInvariant,
    ResourceExhausted,
    Wcag22Invariant,
    StabilityArithmeticInvariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointSupportReferenceOrientationV1 {
    ForegroundLighter,
    BackgroundLighter,
    Unseparated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSupportReferenceDistanceQ55V1 {
    orientation: PointSupportReferenceOrientationV1,
    separated_gap_q55: u64,
    offset_cleared_denominator_q55: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSupportSignedRationalV1 {
    numerator: i128,
    denominator: u128,
}

impl PointSupportSignedRationalV1 {
    pub(crate) const fn numerator(self) -> i128 {
        self.numerator
    }

    pub(crate) const fn denominator(self) -> u128 {
        self.denominator
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSupportRationalV1 {
    numerator: u128,
    denominator: u128,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSupportStabilityNumericalEvidenceV1 {
    site_id: NumericalSiteIdV2,
    artifact_id: NumericalArtifactIdV2,
    bound_id: NumericalErrorBoundIdV2,
    proof_id: NumericalProofIdV2,
    _private: (),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointSupportStabilityDecisionV1 {
    Retained,
    NotRetained,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSupportStabilityEvidenceV1 {
    numerical_evidence: PointSupportStabilityNumericalEvidenceV1,
    baseline_composition: SourceOverCertificateV1,
    baseline_measurement: Wcag22MeasurementV1,
    current_measurement: Wcag22MeasurementV1,
    anchor: PointSupportStabilityAnchorV1,
    drop_fraction: PointSupportDropFractionV1,
    baseline_distance: PointSupportReferenceDistanceQ55V1,
    current_distance: PointSupportReferenceDistanceQ55V1,
    baseline_surplus: PointSupportSignedRationalV1,
    current_surplus: PointSupportSignedRationalV1,
    required_surplus: PointSupportRationalV1,
    decision: PointSupportStabilityDecisionV1,
}

impl PointSupportStabilityEvidenceV1 {
    pub(crate) const fn baseline_composition(self) -> SourceOverCertificateV1 {
        self.baseline_composition
    }

    pub(crate) const fn anchor(self) -> PointSupportStabilityAnchorV1 {
        self.anchor
    }

    pub(crate) const fn drop_fraction(self) -> PointSupportDropFractionV1 {
        self.drop_fraction
    }

    pub(crate) const fn current_surplus(self) -> PointSupportSignedRationalV1 {
        self.current_surplus
    }

    pub(crate) const fn decision(self) -> PointSupportStabilityDecisionV1 {
        self.decision
    }

    const fn failed(self) -> bool {
        matches!(self.decision, PointSupportStabilityDecisionV1::NotRetained)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(
    clippy::large_enum_variant,
    reason = "inline stability evidence keeps evaluation allocation-complete before the first physical composition; heap indirection would break that evidence boundary"
)]
pub(crate) enum PointSupportStabilityAssessmentV1 {
    Disabled,
    Evaluated(PointSupportStabilityEvidenceV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointSupportExactAssessmentV1 {
    NotRequested,
    RequiredPass(ExactPassEvidenceV1),
    RequiredFailure(ExactViolationEvidenceV1),
}

impl PointSupportExactAssessmentV1 {
    pub(crate) const fn failed(self) -> bool {
        matches!(self, Self::RequiredFailure(_))
    }

    pub(crate) fn invocation(self) -> Option<Srgb8> {
        match self {
            Self::NotRequested => None,
            Self::RequiredPass(evidence) => Some(*evidence.invocation()),
            Self::RequiredFailure(evidence) => Some(*evidence.invocation()),
        }
    }

    pub(crate) fn actual(self) -> Option<Srgb8> {
        match self {
            Self::NotRequested => None,
            Self::RequiredPass(evidence) => Some(evidence.actual()),
            Self::RequiredFailure(evidence) => Some(evidence.actual()),
        }
    }
}

type Wcag22PassEvidenceV1 = VisiblePointPassEvidence<Wcag22Srgb8V1>;
type Wcag22FailureEvidenceV1 = VisiblePointViolationEvidence<Wcag22Srgb8V1>;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PointSupportWcag22AssessmentV1 {
    Pass(Wcag22PassEvidenceV1),
    Failure(Wcag22FailureEvidenceV1),
}

impl PointSupportWcag22AssessmentV1 {
    pub(crate) fn criterion(&self) -> Wcag22CriterionV1 {
        match self {
            Self::Pass(evidence) => *evidence.invocation(),
            Self::Failure(evidence) => *evidence.invocation(),
        }
    }

    const fn failed(&self) -> bool {
        matches!(self, Self::Failure(_))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PointSupportCriterionAssessmentV1 {
    NotRequested,
    Required(PointSupportWcag22AssessmentV1),
}

#[derive(Debug, Clone, PartialEq)]
struct PointSupportCellV1 {
    case_index: usize,
    occurrence_index: usize,
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    paint: EncodedPointPaintV1,
    composition: SourceOverCertificateV1,
    exact: PointSupportExactAssessmentV1,
    criterion: PointSupportCriterionAssessmentV1,
    stability: PointSupportStabilityAssessmentV1,
}

/// Borrowed cell and its exact scenario provenance. The provenance cannot be
/// queried through a second integer index or detached from this report view.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PointSupportCellViewV1<'report> {
    cell: &'report PointSupportCellV1,
    provenance: &'report [ScenarioId],
}

impl<'report> PointSupportCellViewV1<'report> {
    pub(crate) const fn case_index(self) -> usize {
        self.cell.case_index
    }

    pub(crate) const fn occurrence_index(self) -> usize {
        self.cell.occurrence_index
    }

    pub(crate) const fn occurrence(self) -> OccurrenceId {
        self.cell.occurrence
    }

    pub(crate) const fn surface(self) -> SurfaceInputPortId {
        self.cell.surface
    }

    pub(crate) const fn paint(self) -> EncodedPointPaintV1 {
        self.cell.paint
    }

    pub(crate) const fn provenance(self) -> &'report [ScenarioId] {
        self.provenance
    }

    pub(crate) const fn exact(self) -> PointSupportExactAssessmentV1 {
        self.cell.exact
    }

    pub(crate) const fn criterion(self) -> &'report PointSupportCriterionAssessmentV1 {
        &self.cell.criterion
    }

    pub(crate) const fn stability(self) -> PointSupportStabilityAssessmentV1 {
        self.cell.stability
    }

    pub(crate) fn composition(self) -> SourceOverCertificateV1 {
        self.cell.composition
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointSupportExactAggregateV1 {
    NotRequested,
    AllRequiredPass,
    RequiredFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointSupportCriterionAggregateV1 {
    NotRequested,
    AllRequiredPass,
    RequiredFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointSupportStabilityAggregateV1 {
    Disabled,
    AllRetained,
    NotRetained,
}

/// Private allocation-complete scratch result. The consuming evaluator borrows
/// its owned observation only while building this value, then binds that same
/// observation before any decision can leave this module.
#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(Clone))]
struct PointSupportAssessmentV1 {
    physical_program: PhysicalProgramIdentityV1,
    composition_profile: CompositionProfileV1,
    cells: Vec<PointSupportCellV1>,
    exact_aggregate: PointSupportExactAggregateV1,
    criterion_aggregate: PointSupportCriterionAggregateV1,
    stability_aggregate: PointSupportStabilityAggregateV1,
    first_exact_failure_index: Option<usize>,
    first_required_failure_index: Option<usize>,
    first_stability_failure_index: Option<usize>,
}

impl PointSupportAssessmentV1 {
    const fn has_failure(&self) -> bool {
        self.first_exact_failure_index.is_some()
            || self.first_required_failure_index.is_some()
            || self.first_stability_failure_index.is_some()
    }

    /// Bind by move only. There is no allocation, recomputation or other
    /// fallible work after the consuming evaluator has completed assessment.
    fn bind(self, observation: RevisionBoundObservationV1) -> PointSupportDecisionV1 {
        let failed = self.has_failure();
        let Self {
            physical_program,
            composition_profile,
            cells,
            exact_aggregate,
            criterion_aggregate,
            stability_aggregate,
            first_exact_failure_index,
            first_required_failure_index,
            first_stability_failure_index,
        } = self;
        let report = RevisionBoundPointSupportReportV1 {
            physical_program,
            composition_profile,
            observation,
            cells,
            exact_aggregate,
            criterion_aggregate,
            stability_aggregate,
            first_exact_failure_index,
            first_required_failure_index,
            first_stability_failure_index,
        };
        if failed {
            PointSupportDecisionV1::Violation(PointSupportViolationV1(report))
        } else {
            PointSupportDecisionV1::Verified(VerifiedPointSupportV1(report))
        }
    }
}

#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct RevisionBoundPointSupportReportV1 {
    physical_program: PhysicalProgramIdentityV1,
    composition_profile: CompositionProfileV1,
    observation: RevisionBoundObservationV1,
    cells: Vec<PointSupportCellV1>,
    exact_aggregate: PointSupportExactAggregateV1,
    criterion_aggregate: PointSupportCriterionAggregateV1,
    stability_aggregate: PointSupportStabilityAggregateV1,
    first_exact_failure_index: Option<usize>,
    first_required_failure_index: Option<usize>,
    first_stability_failure_index: Option<usize>,
}

impl RevisionBoundPointSupportReportV1 {
    pub(crate) const fn physical_program(&self) -> PhysicalProgramIdentityV1 {
        self.physical_program
    }

    pub(crate) const fn composition_profile(&self) -> CompositionProfileV1 {
        self.composition_profile
    }

    pub(crate) const fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }

    pub(crate) fn cells(
        &self,
    ) -> impl ExactSizeIterator<Item = PointSupportCellViewV1<'_>> + DoubleEndedIterator + '_ {
        self.cells.iter().map(|cell| self.view(cell))
    }

    pub(crate) const fn exact_aggregate(&self) -> PointSupportExactAggregateV1 {
        self.exact_aggregate
    }

    pub(crate) const fn criterion_aggregate(&self) -> PointSupportCriterionAggregateV1 {
        self.criterion_aggregate
    }

    pub(crate) const fn stability_aggregate(&self) -> PointSupportStabilityAggregateV1 {
        self.stability_aggregate
    }

    pub(crate) fn first_exact_failure(&self) -> Option<PointSupportCellViewV1<'_>> {
        self.first_exact_failure_index
            .and_then(|index| self.cells.get(index))
            .map(|cell| self.view(cell))
    }

    pub(crate) fn first_required_failure(&self) -> Option<PointSupportCellViewV1<'_>> {
        self.first_required_failure_index
            .and_then(|index| self.cells.get(index))
            .map(|cell| self.view(cell))
    }

    pub(crate) fn first_stability_failure(&self) -> Option<PointSupportCellViewV1<'_>> {
        self.first_stability_failure_index
            .and_then(|index| self.cells.get(index))
            .map(|cell| self.view(cell))
    }

    fn view<'report>(
        &'report self,
        cell: &'report PointSupportCellV1,
    ) -> PointSupportCellViewV1<'report> {
        PointSupportCellViewV1 {
            cell,
            provenance: self
                .observation
                .provenance(cell.case_index)
                .unwrap_or_else(|| unreachable!("cell case came from the same observation")),
        }
    }
}

#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct VerifiedPointSupportV1(RevisionBoundPointSupportReportV1);

impl VerifiedPointSupportV1 {
    pub(crate) const fn report(&self) -> &RevisionBoundPointSupportReportV1 {
        &self.0
    }
}

#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct PointSupportViolationV1(RevisionBoundPointSupportReportV1);

impl PointSupportViolationV1 {
    pub(crate) const fn report(&self) -> &RevisionBoundPointSupportReportV1 {
        &self.0
    }
}

#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(Clone))]
pub(crate) enum PointSupportDecisionV1 {
    Verified(VerifiedPointSupportV1),
    Violation(PointSupportViolationV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EnabledBaselineV1 {
    composition: SourceOverCertificateV1,
    measurement: Wcag22MeasurementV1,
    distance: PointSupportReferenceDistanceQ55V1,
    anchor: PointSupportStabilityAnchorV1,
    drop_fraction: PointSupportDropFractionV1,
    surplus: PointSupportSignedRationalV1,
    numerical_evidence: PointSupportStabilityNumericalEvidenceV1,
}

fn evaluate_bound_point_support(
    plan: &BoundPointSupportRecheckV1,
    observation: &RevisionBoundObservationV1,
) -> Result<PointSupportAssessmentV1, PointSupportEvaluationErrorV1> {
    if plan.occurrences.len() != plan.surface_indices.len()
        || plan.occurrences.len() != plan.baselines.len()
    {
        return Err(PointSupportEvaluationErrorV1::CompiledPlanInvariant);
    }
    observation
        .validate_surface_schema(&plan.surface_schema)
        .map_err(PointSupportEvaluationErrorV1::ObservationSchemaMismatch)?;

    let cell_count = observation
        .physical_case_count()
        .checked_mul(plan.occurrences.len())
        .ok_or(PointSupportEvaluationErrorV1::ResourceExhausted)?;
    let mut cells = Vec::new();
    cells
        .try_reserve_exact(cell_count)
        .map_err(|_| PointSupportEvaluationErrorV1::ResourceExhausted)?;

    let mut exact_requested = false;
    let mut criterion_requested = false;
    let mut stability_enabled = false;
    let mut first_exact_failure_index = None;
    let mut first_required_failure_index = None;
    let mut first_stability_failure_index = None;

    for (case_index, case) in observation.set().cases().iter().enumerate() {
        for (occurrence_index, requirement) in plan.occurrences.iter().enumerate() {
            let surface_index = *plan
                .surface_indices
                .get(occurrence_index)
                .ok_or(PointSupportEvaluationErrorV1::CompiledPlanInvariant)?;
            let baseline = plan
                .baselines
                .get(occurrence_index)
                .ok_or(PointSupportEvaluationErrorV1::CompiledPlanInvariant)?;
            let binding = case.bindings().get(surface_index).ok_or(
                PointSupportEvaluationErrorV1::ObservationSchemaMismatch(
                    ObservationSchemaMismatchV1::new(
                        case_index,
                        surface_index,
                        Some(requirement.surface),
                        None,
                    ),
                ),
            )?;
            if binding.port() != requirement.surface {
                return Err(PointSupportEvaluationErrorV1::ObservationSchemaMismatch(
                    ObservationSchemaMismatchV1::new(
                        case_index,
                        surface_index,
                        Some(requirement.surface),
                        Some(binding.port()),
                    ),
                ));
            }
            let backdrop = binding.value();
            let physical = PointOpacityOverSurfaceV1::evaluate_admitted(
                requirement.paint.source().bytes(),
                requirement.paint.opacity(),
                backdrop.bytes(),
            );

            let exact = match requirement.exact_invocation {
                None => PointSupportExactAssessmentV1::NotRequested,
                Some(invocation) => {
                    exact_requested = true;
                    match assess_visible_point_hard(&physical, &ExactSrgb8IdentityV1, invocation) {
                        Ok(HardDecision::Pass(evidence)) => {
                            PointSupportExactAssessmentV1::RequiredPass(evidence)
                        }
                        Ok(HardDecision::Violation(evidence)) => {
                            PointSupportExactAssessmentV1::RequiredFailure(evidence)
                        }
                        Err(error) => match error {},
                    }
                }
            };
            if exact.failed() && first_exact_failure_index.is_none() {
                first_exact_failure_index = Some(cells.len());
            }

            let criterion = match requirement.criterion {
                PointSupportCriterionRequirementV1::NotRequested => {
                    PointSupportCriterionAssessmentV1::NotRequested
                }
                PointSupportCriterionRequirementV1::Required(criterion) => {
                    criterion_requested = true;
                    let assessment =
                        assess_visible_point_hard(&physical, &Wcag22Srgb8V1, criterion)
                            .map_err(|_| PointSupportEvaluationErrorV1::Wcag22Invariant)?;
                    let assessment = match assessment {
                        HardDecision::Pass(evidence) => {
                            PointSupportWcag22AssessmentV1::Pass(evidence)
                        }
                        HardDecision::Violation(evidence) => {
                            PointSupportWcag22AssessmentV1::Failure(evidence)
                        }
                    };
                    if assessment.failed() && first_required_failure_index.is_none() {
                        first_required_failure_index = Some(cells.len());
                    }
                    PointSupportCriterionAssessmentV1::Required(assessment)
                }
            };

            let stability = match baseline {
                None => PointSupportStabilityAssessmentV1::Disabled,
                Some(baseline) => {
                    stability_enabled = true;
                    let composition = *physical.certificate();
                    let current_measurement =
                        measure_wcag22_srgb8(composition.output_rgb(), composition.backdrop_rgb());
                    let current_distance = reference_distance(current_measurement)?;
                    let current_surplus = anchor_surplus(baseline.anchor, current_distance)?;
                    let required_surplus =
                        required_surplus(baseline.surplus, baseline.drop_fraction)?;
                    let decision = classify_retained(current_surplus, required_surplus);
                    let evidence = PointSupportStabilityEvidenceV1 {
                        numerical_evidence: baseline.numerical_evidence,
                        baseline_composition: baseline.composition,
                        baseline_measurement: baseline.measurement,
                        current_measurement,
                        anchor: baseline.anchor,
                        drop_fraction: baseline.drop_fraction,
                        baseline_distance: baseline.distance,
                        current_distance,
                        baseline_surplus: baseline.surplus,
                        current_surplus,
                        required_surplus,
                        decision,
                    };
                    if evidence.failed() && first_stability_failure_index.is_none() {
                        first_stability_failure_index = Some(cells.len());
                    }
                    PointSupportStabilityAssessmentV1::Evaluated(evidence)
                }
            };

            cells.push(PointSupportCellV1 {
                case_index,
                occurrence_index,
                occurrence: requirement.occurrence,
                surface: requirement.surface,
                paint: requirement.paint,
                composition: *physical.certificate(),
                exact,
                criterion,
                stability,
            });
        }
    }

    Ok(PointSupportAssessmentV1 {
        physical_program: plan.physical_program,
        composition_profile: plan.composition_profile,
        cells,
        exact_aggregate: if first_exact_failure_index.is_some() {
            PointSupportExactAggregateV1::RequiredFailure
        } else if exact_requested {
            PointSupportExactAggregateV1::AllRequiredPass
        } else {
            PointSupportExactAggregateV1::NotRequested
        },
        criterion_aggregate: if first_required_failure_index.is_some() {
            PointSupportCriterionAggregateV1::RequiredFailure
        } else if criterion_requested {
            PointSupportCriterionAggregateV1::AllRequiredPass
        } else {
            PointSupportCriterionAggregateV1::NotRequested
        },
        stability_aggregate: if first_stability_failure_index.is_some() {
            PointSupportStabilityAggregateV1::NotRetained
        } else if stability_enabled {
            PointSupportStabilityAggregateV1::AllRetained
        } else {
            PointSupportStabilityAggregateV1::Disabled
        },
        first_exact_failure_index,
        first_required_failure_index,
        first_stability_failure_index,
    })
}

fn reference_distance(
    measurement: Wcag22MeasurementV1,
) -> Result<PointSupportReferenceDistanceQ55V1, PointSupportEvaluationErrorV1> {
    let foreground = measurement.foreground_luminance;
    let background = measurement.background_luminance;
    let scale = crate::wcag22::Wcag22LuminanceBoundsQ55V1::scale();
    let (orientation, gap, darker_upper) = if foreground.lower() > background.upper() {
        (
            PointSupportReferenceOrientationV1::ForegroundLighter,
            foreground.lower() - background.upper(),
            background.upper(),
        )
    } else if background.lower() > foreground.upper() {
        (
            PointSupportReferenceOrientationV1::BackgroundLighter,
            background.lower() - foreground.upper(),
            foreground.upper(),
        )
    } else {
        return Ok(PointSupportReferenceDistanceQ55V1 {
            orientation: PointSupportReferenceOrientationV1::Unseparated,
            separated_gap_q55: 0,
            offset_cleared_denominator_q55: scale,
        });
    };
    let denominator = darker_upper
        .checked_mul(20)
        .and_then(|value| value.checked_add(scale))
        .ok_or(PointSupportEvaluationErrorV1::StabilityArithmeticInvariant)?;
    Ok(PointSupportReferenceDistanceQ55V1 {
        orientation,
        separated_gap_q55: gap,
        offset_cleared_denominator_q55: denominator,
    })
}

fn anchor_surplus(
    anchor: PointSupportStabilityAnchorV1,
    distance: PointSupportReferenceDistanceQ55V1,
) -> Result<PointSupportSignedRationalV1, PointSupportEvaluationErrorV1> {
    let gap = i128::from(distance.separated_gap_q55);
    let denominator = i128::from(distance.offset_cleared_denominator_q55);
    let (numerator, rational_denominator) = match anchor {
        PointSupportStabilityAnchorV1::Identity1 => (
            gap.checked_mul(20),
            u128::from(distance.offset_cleared_denominator_q55),
        ),
        PointSupportStabilityAnchorV1::Ratio3 => (
            gap.checked_mul(20)
                .and_then(|value| value.checked_sub(denominator.checked_mul(2)?)),
            u128::from(distance.offset_cleared_denominator_q55),
        ),
        PointSupportStabilityAnchorV1::Ratio4Point5 => (
            gap.checked_mul(40)
                .and_then(|value| value.checked_sub(denominator.checked_mul(7)?)),
            u128::from(distance.offset_cleared_denominator_q55)
                .checked_mul(2)
                .ok_or(PointSupportEvaluationErrorV1::StabilityArithmeticInvariant)?,
        ),
    };
    Ok(PointSupportSignedRationalV1 {
        numerator: numerator.ok_or(PointSupportEvaluationErrorV1::StabilityArithmeticInvariant)?,
        denominator: rational_denominator,
    })
}

fn required_surplus(
    baseline: PointSupportSignedRationalV1,
    drop_fraction: PointSupportDropFractionV1,
) -> Result<PointSupportRationalV1, PointSupportEvaluationErrorV1> {
    if baseline.numerator <= 0 || drop_fraction == PointSupportDropFractionV1::ALL {
        return Ok(PointSupportRationalV1 {
            numerator: 0,
            denominator: 1,
        });
    }
    Ok(PointSupportRationalV1 {
        numerator: (baseline.numerator as u128)
            .checked_mul(u128::from(drop_fraction.retained_basis_points()))
            .ok_or(PointSupportEvaluationErrorV1::StabilityArithmeticInvariant)?,
        denominator: baseline
            .denominator
            .checked_mul(u128::from(DROP_BASIS_POINTS_SCALE))
            .ok_or(PointSupportEvaluationErrorV1::StabilityArithmeticInvariant)?,
    })
}

fn classify_retained(
    current: PointSupportSignedRationalV1,
    required: PointSupportRationalV1,
) -> PointSupportStabilityDecisionV1 {
    if current.numerator < 0 {
        return PointSupportStabilityDecisionV1::NotRetained;
    }
    if compare_nonnegative_rationals(
        current.numerator as u128,
        current.denominator,
        required.numerator,
        required.denominator,
    )
    .is_lt()
    {
        PointSupportStabilityDecisionV1::NotRetained
    } else {
        PointSupportStabilityDecisionV1::Retained
    }
}

fn compare_nonnegative_rationals(
    mut left_numerator: u128,
    mut left_denominator: u128,
    mut right_numerator: u128,
    mut right_denominator: u128,
) -> Ordering {
    debug_assert!(left_denominator != 0);
    debug_assert!(right_denominator != 0);
    let mut reversed = false;
    loop {
        let left_quotient = left_numerator / left_denominator;
        let right_quotient = right_numerator / right_denominator;
        if left_quotient != right_quotient {
            let order = left_quotient.cmp(&right_quotient);
            return if reversed { order.reverse() } else { order };
        }
        let left_remainder = left_numerator % left_denominator;
        let right_remainder = right_numerator % right_denominator;
        let terminal = match (left_remainder == 0, right_remainder == 0) {
            (true, true) => Some(Ordering::Equal),
            (true, false) => Some(Ordering::Less),
            (false, true) => Some(Ordering::Greater),
            (false, false) => None,
        };
        if let Some(order) = terminal {
            return if reversed { order.reverse() } else { order };
        }
        left_numerator = left_denominator;
        left_denominator = left_remainder;
        right_numerator = right_denominator;
        right_denominator = right_remainder;
        reversed = !reversed;
    }
}

fn mint_stability_evidence()
-> Result<PointSupportStabilityNumericalEvidenceV1, PointSupportCompileErrorV1> {
    let row = numerical_registry_v2()
        .iter()
        .find(|row| row.site_id == STABILITY_SITE_ID)
        .ok_or(PointSupportCompileErrorV1::NumericalRegistryInvariant)?;
    let valid = row.stable_outcomes == [StableNumericalOutcomeV2::CanonicalFiniteBounded]
        && row.compatibility_releases.is_empty()
        && row.evidence_classes == [NumericalEvidenceClassV2::CanonicalFiniteBounded]
        && row.artifact_ids == [STABILITY_ARTIFACT_ID]
        && row.bound_ids == [STABILITY_BOUND_ID]
        && row.proof_ids == [STABILITY_PROOF_ID]
        && row.runtime_attestations.is_empty()
        && row.bound_status == NumericalBoundStatusV2::Available
        && row.fallback_status == NumericalFallbackStatusV1::None;
    if !valid {
        return Err(PointSupportCompileErrorV1::NumericalRegistryInvariant);
    }
    Ok(PointSupportStabilityNumericalEvidenceV1 {
        site_id: STABILITY_SITE_ID,
        artifact_id: STABILITY_ARTIFACT_ID,
        bound_id: STABILITY_BOUND_ID,
        proof_id: STABILITY_PROOF_ID,
        _private: (),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_ordering_handles_equal_and_cross_product_extrema() {
        assert_eq!(compare_nonnegative_rationals(1, 3, 2, 6), Ordering::Equal);
        assert_eq!(
            compare_nonnegative_rationals(u128::MAX - 1, u128::MAX, 1, 2),
            Ordering::Greater
        );
        assert_eq!(
            compare_nonnegative_rationals(1, u128::MAX, 2, u128::MAX),
            Ordering::Less
        );
    }

    #[test]
    fn exact_reference_witness_components_remain_typed_and_nonzero() {
        let distance = PointSupportReferenceDistanceQ55V1 {
            orientation: PointSupportReferenceOrientationV1::ForegroundLighter,
            separated_gap_q55: 9,
            offset_cleared_denominator_q55: 5,
        };
        assert_eq!(
            distance.orientation,
            PointSupportReferenceOrientationV1::ForegroundLighter
        );

        let current = anchor_surplus(PointSupportStabilityAnchorV1::Ratio3, distance).unwrap();
        assert_eq!(current.numerator(), 170);
        assert_eq!(current.denominator(), 5);

        let required = required_surplus(
            current,
            PointSupportDropFractionV1::try_from_basis_points(2_500).unwrap(),
        )
        .unwrap();
        assert_eq!(required.numerator, 1_275_000);
        assert_eq!(required.denominator, 50_000);
        assert_eq!(
            classify_retained(current, required),
            PointSupportStabilityDecisionV1::Retained
        );
    }
}
