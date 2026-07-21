//! Revision-bound full-support assessment for replayable point occurrences.
//!
//! Public authority is staged as declaration -> compiled plan -> schema-bound
//! plan -> immutable observation revision -> report. WCAG criterion truth and
//! runtime stability remain separate policies over one source-over certificate.

use core::cmp::Ordering;

use crate::Srgb8;
use crate::appearance::{
    OccurrenceId, PointOpacityOverSurfaceV1, ResolvedOccurrence, SourceOverCertificateV1,
    SurfaceInputPortId,
};
use crate::composition::{AdmittedOpacityV1, OpacityAdmissionErrorV1};
use crate::numerics::{
    NumericalArtifactIdV2, NumericalBoundStatusV2, NumericalErrorBoundIdV2,
    NumericalEvidenceClassV2, NumericalFallbackStatusV1, NumericalProofIdV2, NumericalSiteIdV2,
    StableNumericalOutcomeV2, numerical_registry_v2,
};
use crate::observation::{ObservationSchemaV1, RevisionBoundObservationV1, ScenarioId};
use crate::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22CriterionV1,
    Wcag22EvaluationErrorV1, Wcag22MeasurementV1, Wcag22ProfileIdV1,
    evaluate_wcag22_srgb8, measure_wcag22_srgb8,
};

const DROP_BASIS_POINTS_SCALE: u16 = 10_000;
const STABILITY_SITE_ID: NumericalSiteIdV2 =
    NumericalSiteIdV2::PointSupportRetainedReferenceSurplusV1;
const STABILITY_ARTIFACT_ID: NumericalArtifactIdV2 =
    NumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55V1;
const STABILITY_BOUND_ID: NumericalErrorBoundIdV2 =
    NumericalErrorBoundIdV2::PointSupportReferenceSurplusQ55BpsV1;
const STABILITY_PROOF_ID: NumericalProofIdV2 =
    NumericalProofIdV2::PointSupportReferenceSurplusIntegerV1;

/// Client-owned immutable generation of one compiled support plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PointSupportPlanRevisionV1(u64);

impl PointSupportPlanRevisionV1 {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Accessibility request for one occurrence.
///
/// `ReportOnly` is intentionally absent until it can carry an admitted LPC
/// decision certificate bound to the same occurrence and context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportCriterionRequirementV1 {
    /// No assessment was requested. This makes no applicability or conformance
    /// claim and fabricates no pass.
    NotRequested,
    /// Evaluate and require one exact WCAG 2.2 success criterion.
    Required(Wcag22CriterionV1),
}

/// Exact declared reference-ratio anchor for runtime stability.
///
/// This policy is independent of WCAG applicability. A compiler may explicitly
/// choose the same numeric anchor as a criterion, but Core never derives one
/// from the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportStabilityAnchorV1 {
    Identity1,
    Ratio3,
    Ratio4Point5,
}

impl PointSupportStabilityAnchorV1 {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Identity1 => "ratio-1",
            Self::Ratio3 => "ratio-3",
            Self::Ratio4Point5 => "ratio-4.5",
        }
    }
}

/// Exact fraction of baseline surplus that may be discarded, in basis points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointSupportDropFractionV1(u16);

impl PointSupportDropFractionV1 {
    pub const NONE: Self = Self(0);
    pub const ALL: Self = Self(DROP_BASIS_POINTS_SCALE);

    pub fn try_from_basis_points(
        basis_points: u32,
    ) -> Result<Self, PointSupportAdmissionErrorV1> {
        if basis_points > u32::from(DROP_BASIS_POINTS_SCALE) {
            return Err(PointSupportAdmissionErrorV1::DropFractionOutsideBasisPointRange);
        }
        Ok(Self(basis_points as u16))
    }

    pub const fn basis_points(self) -> u16 {
        self.0
    }

    pub const fn retained_basis_points(self) -> u16 {
        DROP_BASIS_POINTS_SCALE - self.0
    }
}

/// Runtime stability policy for one occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportStabilityPolicyV1 {
    Disabled,
    /// Retain `1 - drop_fraction` of the baseline's conservative reference
    /// surplus above the explicit anchor.
    RetainBaselineReferenceSurplus {
        baseline_backdrop: Srgb8,
        anchor: PointSupportStabilityAnchorV1,
        drop_fraction: PointSupportDropFractionV1,
    },
}

/// One replayable source/straight-alpha occurrence bound to a surface input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointSupportOccurrenceV1 {
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    source: Srgb8,
    opacity: AdmittedOpacityV1,
    criterion: PointSupportCriterionRequirementV1,
    stability: PointSupportStabilityPolicyV1,
}

impl PointSupportOccurrenceV1 {
    pub fn try_new(
        occurrence: OccurrenceId,
        surface: SurfaceInputPortId,
        source: Srgb8,
        opacity: f64,
        criterion: PointSupportCriterionRequirementV1,
        stability: PointSupportStabilityPolicyV1,
    ) -> Result<Self, PointSupportAdmissionErrorV1> {
        let opacity = AdmittedOpacityV1::new(opacity).map_err(|error| match error {
            OpacityAdmissionErrorV1::NonFinite => PointSupportAdmissionErrorV1::OpacityNonFinite,
            OpacityAdmissionErrorV1::OutsideUnitInterval => {
                PointSupportAdmissionErrorV1::OpacityOutsideUnitInterval
            }
        })?;
        Ok(Self {
            occurrence,
            surface,
            source,
            opacity,
            criterion,
            stability,
        })
    }

    pub const fn occurrence(self) -> OccurrenceId {
        self.occurrence
    }

    pub const fn surface(self) -> SurfaceInputPortId {
        self.surface
    }

    pub const fn source(self) -> Srgb8 {
        self.source
    }

    pub const fn opacity(self) -> f64 {
        self.opacity.value()
    }

    pub const fn opacity_bits(self) -> u64 {
        self.opacity.bits()
    }

    pub const fn criterion(self) -> PointSupportCriterionRequirementV1 {
        self.criterion
    }

    pub const fn stability(self) -> PointSupportStabilityPolicyV1 {
        self.stability
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportAdmissionErrorV1 {
    OpacityNonFinite,
    OpacityOutsideUnitInterval,
    DropFractionOutsideBasisPointRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportPlanErrorV1 {
    EmptyOccurrences,
    DuplicateOccurrence(OccurrenceId),
    MissingSurfaceInput(SurfaceInputPortId),
    ResourceExhausted,
}

/// Declarative plan before it is bound to an admitted observation schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPointSupportPlanV1 {
    revision: PointSupportPlanRevisionV1,
    occurrences: Vec<PointSupportOccurrenceV1>,
}

impl CompiledPointSupportPlanV1 {
    pub fn try_new(
        revision: PointSupportPlanRevisionV1,
        occurrences: Vec<PointSupportOccurrenceV1>,
    ) -> Result<Self, PointSupportPlanErrorV1> {
        if occurrences.is_empty() {
            return Err(PointSupportPlanErrorV1::EmptyOccurrences);
        }
        let mut identities = Vec::new();
        identities
            .try_reserve_exact(occurrences.len())
            .map_err(|_| PointSupportPlanErrorV1::ResourceExhausted)?;
        identities.extend(occurrences.iter().map(|occurrence| occurrence.occurrence));
        identities.sort_unstable();
        if let Some(duplicate) = identities.windows(2).find(|window| window[0] == window[1]) {
            return Err(PointSupportPlanErrorV1::DuplicateOccurrence(duplicate[0]));
        }
        Ok(Self {
            revision,
            occurrences,
        })
    }

    pub const fn revision(&self) -> PointSupportPlanRevisionV1 {
        self.revision
    }

    pub fn occurrences(&self) -> &[PointSupportOccurrenceV1] {
        &self.occurrences
    }

    pub fn bind(
        self,
        schema: &ObservationSchemaV1,
    ) -> Result<BoundPointSupportPlanV1, PointSupportPlanErrorV1> {
        let mut surface_indices = Vec::new();
        surface_indices
            .try_reserve_exact(self.occurrences.len())
            .map_err(|_| PointSupportPlanErrorV1::ResourceExhausted)?;
        for occurrence in &self.occurrences {
            let index = schema
                .ports()
                .binary_search(&occurrence.surface)
                .map_err(|_| PointSupportPlanErrorV1::MissingSurfaceInput(occurrence.surface))?;
            surface_indices.push(index);
        }
        Ok(BoundPointSupportPlanV1 {
            plan: self,
            schema: schema.clone(),
            surface_indices,
        })
    }
}

/// Reusable plan whose surface references are prebound to one canonical schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundPointSupportPlanV1 {
    plan: CompiledPointSupportPlanV1,
    schema: ObservationSchemaV1,
    surface_indices: Vec<usize>,
}

impl BoundPointSupportPlanV1 {
    pub const fn revision(&self) -> PointSupportPlanRevisionV1 {
        self.plan.revision
    }

    pub fn occurrences(&self) -> &[PointSupportOccurrenceV1] {
        &self.plan.occurrences
    }

    pub const fn schema(&self) -> &ObservationSchemaV1 {
        &self.schema
    }

    pub fn evaluate(
        &self,
        observation: RevisionBoundObservationV1,
    ) -> Result<RevisionBoundPointSupportReportV1<'_>, PointSupportEvaluationErrorV1> {
        evaluate_bound_point_support(self, observation)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportEvaluationErrorV1 {
    ObservationSchemaMismatch,
    ResourceExhausted,
    Wcag22(Wcag22EvaluationErrorV1),
    Wcag22ProtocolInvariant,
    StabilityArithmeticInvariant,
    NumericalRegistryInvariant(String),
}

/// Criterion-free conservative lower reference-ratio distance witness.
///
/// It represents `ratio_lower - 1 = 20 * separated_gap_q55 /
/// offset_cleared_denominator_q55`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointSupportReferenceDistanceQ55V1 {
    orientation: PointSupportReferenceOrientationV1,
    separated_gap_q55: u64,
    offset_cleared_denominator_q55: u64,
}

impl PointSupportReferenceDistanceQ55V1 {
    pub const fn orientation(self) -> PointSupportReferenceOrientationV1 {
        self.orientation
    }

    pub const fn separated_gap_q55(self) -> u64 {
        self.separated_gap_q55
    }

    pub const fn offset_cleared_denominator_q55(self) -> u64 {
        self.offset_cleared_denominator_q55
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportReferenceOrientationV1 {
    ForegroundLighter,
    BackgroundLighter,
    Unseparated,
}

/// Exact signed rational; denominator is always nonzero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointSupportSignedRationalV1 {
    numerator: i128,
    denominator: u128,
}

impl PointSupportSignedRationalV1 {
    pub const fn numerator(self) -> i128 {
        self.numerator
    }

    pub const fn denominator(self) -> u128 {
        self.denominator
    }
}

/// Exact nonnegative rational; denominator is always nonzero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointSupportRationalV1 {
    numerator: u128,
    denominator: u128,
}

impl PointSupportRationalV1 {
    pub const fn numerator(self) -> u128 {
        self.numerator
    }

    pub const fn denominator(self) -> u128 {
        self.denominator
    }
}

/// Registry-sealed identity of the numerical law used by stability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointSupportStabilityNumericalEvidenceV1 {
    site_id: NumericalSiteIdV2,
    artifact_id: NumericalArtifactIdV2,
    bound_id: NumericalErrorBoundIdV2,
    proof_id: NumericalProofIdV2,
    _private: (),
}

impl PointSupportStabilityNumericalEvidenceV1 {
    pub const fn site_id(self) -> NumericalSiteIdV2 {
        self.site_id
    }

    pub const fn artifact_id(self) -> NumericalArtifactIdV2 {
        self.artifact_id
    }

    pub const fn bound_id(self) -> NumericalErrorBoundIdV2 {
        self.bound_id
    }

    pub const fn proof_id(self) -> NumericalProofIdV2 {
        self.proof_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportStabilityProfileV1 {
    Srgb8Q55RetainedReferenceSurplusBpsV1,
}

impl PointSupportStabilityProfileV1 {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Srgb8Q55RetainedReferenceSurplusBpsV1 => {
                "srgb8-q55-retained-reference-surplus-bps-v1"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportStabilityDecisionV1 {
    Retained,
    NotRetained,
}

/// Replay-complete evidence for one enabled stability invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointSupportStabilityEvidenceV1 {
    profile: PointSupportStabilityProfileV1,
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
    pub const fn profile(self) -> PointSupportStabilityProfileV1 {
        self.profile
    }

    pub const fn numerical_evidence(self) -> PointSupportStabilityNumericalEvidenceV1 {
        self.numerical_evidence
    }

    pub const fn baseline_composition(self) -> SourceOverCertificateV1 {
        self.baseline_composition
    }

    pub const fn baseline_measurement(self) -> Wcag22MeasurementV1 {
        self.baseline_measurement
    }

    pub const fn current_measurement(self) -> Wcag22MeasurementV1 {
        self.current_measurement
    }

    pub const fn anchor(self) -> PointSupportStabilityAnchorV1 {
        self.anchor
    }

    pub const fn drop_fraction(self) -> PointSupportDropFractionV1 {
        self.drop_fraction
    }

    pub const fn baseline_distance(self) -> PointSupportReferenceDistanceQ55V1 {
        self.baseline_distance
    }

    pub const fn current_distance(self) -> PointSupportReferenceDistanceQ55V1 {
        self.current_distance
    }

    pub const fn baseline_surplus(self) -> PointSupportSignedRationalV1 {
        self.baseline_surplus
    }

    pub const fn current_surplus(self) -> PointSupportSignedRationalV1 {
        self.current_surplus
    }

    pub const fn required_surplus(self) -> PointSupportRationalV1 {
        self.required_surplus
    }

    pub const fn decision(self) -> PointSupportStabilityDecisionV1 {
        self.decision
    }

    pub const fn failed(self) -> bool {
        matches!(self.decision, PointSupportStabilityDecisionV1::NotRetained)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportStabilityAssessmentV1 {
    Disabled,
    Evaluated(PointSupportStabilityEvidenceV1),
}

/// Exact applicable WCAG 2.2 result and its proof-bound evidence.
#[derive(Debug, Clone, PartialEq)]
pub struct PointSupportWcag22AssessmentV1 {
    profile_id: Wcag22ProfileIdV1,
    criterion: Wcag22CriterionV1,
    measurement: Wcag22MeasurementV1,
    decision: Wcag22ApplicableDecisionV1,
    evidence: crate::numerics::NumericalDecisionEvidenceV1,
}

impl PointSupportWcag22AssessmentV1 {
    pub const fn profile_id(&self) -> Wcag22ProfileIdV1 {
        self.profile_id
    }

    pub const fn criterion(&self) -> Wcag22CriterionV1 {
        self.criterion
    }

    pub const fn measurement(&self) -> &Wcag22MeasurementV1 {
        &self.measurement
    }

    pub const fn decision(&self) -> Wcag22ApplicableDecisionV1 {
        self.decision
    }

    pub const fn evidence(&self) -> &crate::numerics::NumericalDecisionEvidenceV1 {
        &self.evidence
    }

    pub const fn failed(&self) -> bool {
        matches!(self.decision, Wcag22ApplicableDecisionV1::Fail)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum PointSupportCriterionAssessmentV1 {
    NotRequested,
    Required(PointSupportWcag22AssessmentV1),
}

/// One physical/evaluator cell in canonical-case, declared-occurrence order.
#[derive(Debug, Clone, PartialEq)]
pub struct PointSupportCellV1 {
    case_index: usize,
    occurrence_index: usize,
    occurrence: OccurrenceId,
    surface: SurfaceInputPortId,
    composition: SourceOverCertificateV1,
    criterion: PointSupportCriterionAssessmentV1,
    stability: PointSupportStabilityAssessmentV1,
}

impl PointSupportCellV1 {
    pub const fn case_index(&self) -> usize {
        self.case_index
    }

    pub const fn occurrence_index(&self) -> usize {
        self.occurrence_index
    }

    pub const fn occurrence(&self) -> OccurrenceId {
        self.occurrence
    }

    pub const fn surface(&self) -> SurfaceInputPortId {
        self.surface
    }

    pub const fn composition(&self) -> SourceOverCertificateV1 {
        self.composition
    }

    pub const fn criterion(&self) -> &PointSupportCriterionAssessmentV1 {
        &self.criterion
    }

    pub const fn stability(&self) -> PointSupportStabilityAssessmentV1 {
        self.stability
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportCriterionAggregateV1 {
    NotRequested,
    AllRequiredPass,
    RequiredFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportStabilityAggregateV1 {
    Disabled,
    AllRetained,
    NotRetained,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PointSupportActionV1 {
    NoReconciliationRequired,
    ReconciliationRequired,
}

/// Full report bound to one plan generation and one admitted observation
/// revision. Its borrow prevents rebinding to a different plan.
#[derive(Debug, PartialEq)]
pub struct RevisionBoundPointSupportReportV1<'plan> {
    plan: &'plan BoundPointSupportPlanV1,
    observation: RevisionBoundObservationV1,
    cells: Vec<PointSupportCellV1>,
    criterion_aggregate: PointSupportCriterionAggregateV1,
    stability_aggregate: PointSupportStabilityAggregateV1,
    action: PointSupportActionV1,
    first_required_failure_index: Option<usize>,
    first_stability_failure_index: Option<usize>,
}

impl RevisionBoundPointSupportReportV1<'_> {
    pub const fn plan_revision(&self) -> PointSupportPlanRevisionV1 {
        self.plan.revision()
    }

    pub const fn plan(&self) -> &BoundPointSupportPlanV1 {
        self.plan
    }

    pub const fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }

    pub fn cells(&self) -> &[PointSupportCellV1] {
        &self.cells
    }

    pub const fn criterion_aggregate(&self) -> PointSupportCriterionAggregateV1 {
        self.criterion_aggregate
    }

    pub const fn stability_aggregate(&self) -> PointSupportStabilityAggregateV1 {
        self.stability_aggregate
    }

    pub const fn action(&self) -> PointSupportActionV1 {
        self.action
    }

    pub fn first_required_failure_cell(&self) -> Option<&PointSupportCellV1> {
        self.first_required_failure_index
            .and_then(|index| self.cells.get(index))
    }

    pub fn first_stability_failure_cell(&self) -> Option<&PointSupportCellV1> {
        self.first_stability_failure_index
            .and_then(|index| self.cells.get(index))
    }

    pub fn primary_failure_cell(&self) -> Option<&PointSupportCellV1> {
        self.first_required_failure_cell()
            .or_else(|| self.first_stability_failure_cell())
    }

    pub fn provenance(&self, cell_index: usize) -> Option<&[ScenarioId]> {
        let cell = self.cells.get(cell_index)?;
        self.observation.provenance(cell.case_index)
    }
}

#[derive(Debug, Clone, Copy)]
struct EnabledBaselineV1 {
    composition: SourceOverCertificateV1,
    measurement: Wcag22MeasurementV1,
    distance: PointSupportReferenceDistanceQ55V1,
    anchor: PointSupportStabilityAnchorV1,
    drop_fraction: PointSupportDropFractionV1,
    surplus: PointSupportSignedRationalV1,
    numerical_evidence: PointSupportStabilityNumericalEvidenceV1,
}

fn evaluate_bound_point_support<'plan>(
    plan: &'plan BoundPointSupportPlanV1,
    observation: RevisionBoundObservationV1,
) -> Result<RevisionBoundPointSupportReportV1<'plan>, PointSupportEvaluationErrorV1> {
    if observation.admitted_schema() != &plan.schema {
        return Err(PointSupportEvaluationErrorV1::ObservationSchemaMismatch);
    }

    let cell_count = observation
        .physical_case_count()
        .checked_mul(plan.plan.occurrences.len())
        .ok_or(PointSupportEvaluationErrorV1::ResourceExhausted)?;
    let mut cells = Vec::new();
    cells
        .try_reserve_exact(cell_count)
        .map_err(|_| PointSupportEvaluationErrorV1::ResourceExhausted)?;
    let mut baselines = Vec::new();
    baselines
        .try_reserve_exact(plan.plan.occurrences.len())
        .map_err(|_| PointSupportEvaluationErrorV1::ResourceExhausted)?;

    let stability_numerical_evidence = plan
        .plan
        .occurrences
        .iter()
        .any(|occurrence| {
            matches!(
                occurrence.stability,
                PointSupportStabilityPolicyV1::RetainBaselineReferenceSurplus { .. }
            )
        })
        .then(mint_stability_evidence)
        .transpose()?;

    for occurrence in &plan.plan.occurrences {
        let baseline = match occurrence.stability {
            PointSupportStabilityPolicyV1::Disabled => None,
            PointSupportStabilityPolicyV1::RetainBaselineReferenceSurplus {
                baseline_backdrop,
                anchor,
                drop_fraction,
            } => {
                let physical = PointOpacityOverSurfaceV1::evaluate_admitted(
                    occurrence.source.bytes(),
                    occurrence.opacity,
                    baseline_backdrop.bytes(),
                );
                let composition = *physical.certificate();
                let measurement =
                    measure_wcag22_srgb8(composition.output_rgb(), composition.backdrop_rgb());
                let distance = reference_distance(measurement)?;
                let surplus = anchor_surplus(anchor, distance)?;
                Some(EnabledBaselineV1 {
                    composition,
                    measurement,
                    distance,
                    anchor,
                    drop_fraction,
                    surplus,
                    numerical_evidence: stability_numerical_evidence
                        .ok_or(PointSupportEvaluationErrorV1::StabilityArithmeticInvariant)?,
                })
            }
        };
        baselines.push(baseline);
    }

    let mut criterion_requested = false;
    let mut stability_enabled = false;
    let mut first_required_failure_index = None;
    let mut first_stability_failure_index = None;

    for (case_index, case) in observation.set().cases().iter().enumerate() {
        for (occurrence_index, ((occurrence, &surface_index), baseline)) in plan
            .plan
            .occurrences
            .iter()
            .zip(plan.surface_indices.iter())
            .zip(baselines.iter())
            .enumerate()
        {
            let backdrop = case.bindings()[surface_index];
            let physical = PointOpacityOverSurfaceV1::evaluate_admitted(
                occurrence.source.bytes(),
                occurrence.opacity,
                backdrop.bytes(),
            );
            let criterion = required_assessment(&physical, occurrence.criterion)?;
            if matches!(
                occurrence.criterion,
                PointSupportCriterionRequirementV1::Required(_)
            ) {
                criterion_requested = true;
            }
            if matches!(
                &criterion,
                PointSupportCriterionAssessmentV1::Required(assessment) if assessment.failed()
            ) && first_required_failure_index.is_none()
            {
                first_required_failure_index = Some(cells.len());
            }

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
                    if matches!(decision, PointSupportStabilityDecisionV1::NotRetained)
                        && first_stability_failure_index.is_none()
                    {
                        first_stability_failure_index = Some(cells.len());
                    }
                    PointSupportStabilityAssessmentV1::Evaluated(
                        PointSupportStabilityEvidenceV1 {
                            profile: PointSupportStabilityProfileV1::Srgb8Q55RetainedReferenceSurplusBpsV1,
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
                        },
                    )
                }
            };
            cells.push(PointSupportCellV1 {
                case_index,
                occurrence_index,
                occurrence: occurrence.occurrence,
                surface: occurrence.surface,
                composition: *physical.certificate(),
                criterion,
                stability,
            });
        }
    }

    let criterion_aggregate = if first_required_failure_index.is_some() {
        PointSupportCriterionAggregateV1::RequiredFailure
    } else if criterion_requested {
        PointSupportCriterionAggregateV1::AllRequiredPass
    } else {
        PointSupportCriterionAggregateV1::NotRequested
    };
    let stability_aggregate = if first_stability_failure_index.is_some() {
        PointSupportStabilityAggregateV1::NotRetained
    } else if stability_enabled {
        PointSupportStabilityAggregateV1::AllRetained
    } else {
        PointSupportStabilityAggregateV1::Disabled
    };
    let action =
        if first_required_failure_index.is_some() || first_stability_failure_index.is_some() {
            PointSupportActionV1::ReconciliationRequired
        } else {
            PointSupportActionV1::NoReconciliationRequired
        };

    Ok(RevisionBoundPointSupportReportV1 {
        plan,
        observation,
        cells,
        criterion_aggregate,
        stability_aggregate,
        action,
        first_required_failure_index,
        first_stability_failure_index,
    })
}

fn required_assessment(
    occurrence: &ResolvedOccurrence,
    requirement: PointSupportCriterionRequirementV1,
) -> Result<PointSupportCriterionAssessmentV1, PointSupportEvaluationErrorV1> {
    let PointSupportCriterionRequirementV1::Required(requested) = requirement else {
        return Ok(PointSupportCriterionAssessmentV1::NotRequested);
    };
    let certificate = occurrence.certificate();
    let assessment = evaluate_wcag22_srgb8(
        certificate.output_rgb(),
        certificate.backdrop_rgb(),
        requested,
    )
    .map_err(PointSupportEvaluationErrorV1::Wcag22)?;
    match assessment {
        Wcag22AssessmentV1::Evaluated {
            profile_id,
            criterion,
            measurement,
            decision,
            evidence,
        } if criterion == requested => Ok(PointSupportCriterionAssessmentV1::Required(
            PointSupportWcag22AssessmentV1 {
                profile_id,
                criterion,
                measurement,
                decision,
                evidence,
            },
        )),
        Wcag22AssessmentV1::Evaluated { .. } | Wcag22AssessmentV1::NotEvaluated { .. } => {
            Err(PointSupportEvaluationErrorV1::Wcag22ProtocolInvariant)
        }
    }
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
    let numerator =
        numerator.ok_or(PointSupportEvaluationErrorV1::StabilityArithmeticInvariant)?;
    Ok(PointSupportSignedRationalV1 {
        numerator,
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
    let numerator = (baseline.numerator as u128)
        .checked_mul(u128::from(drop_fraction.retained_basis_points()))
        .ok_or(PointSupportEvaluationErrorV1::StabilityArithmeticInvariant)?;
    let denominator = baseline
        .denominator
        .checked_mul(u128::from(DROP_BASIS_POINTS_SCALE))
        .ok_or(PointSupportEvaluationErrorV1::StabilityArithmeticInvariant)?;
    Ok(PointSupportRationalV1 {
        numerator,
        denominator,
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

/// Exact fraction ordering without cross multiplication.
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

fn mint_stability_evidence(
) -> Result<PointSupportStabilityNumericalEvidenceV1, PointSupportEvaluationErrorV1> {
    let row = numerical_registry_v2()
        .iter()
        .find(|row| row.site_id == STABILITY_SITE_ID)
        .ok_or_else(|| {
            PointSupportEvaluationErrorV1::NumericalRegistryInvariant(
                "point-support stability site is absent".to_string(),
            )
        })?;
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
        return Err(PointSupportEvaluationErrorV1::NumericalRegistryInvariant(
            "point-support stability registry row drifted".to_string(),
        ));
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
    fn drop_basis_points_are_closed_and_exact() {
        assert_eq!(
            PointSupportDropFractionV1::try_from_basis_points(0).unwrap(),
            PointSupportDropFractionV1::NONE
        );
        assert_eq!(
            PointSupportDropFractionV1::try_from_basis_points(10_000).unwrap(),
            PointSupportDropFractionV1::ALL
        );
        assert_eq!(
            PointSupportDropFractionV1::try_from_basis_points(10_001),
            Err(PointSupportAdmissionErrorV1::DropFractionOutsideBasisPointRange)
        );
    }
}
