//! Declared field ingress and the scope of its revision-bound result.

use super::*;
use crate::field_context::{FieldStops, ProgramGradientStopV1};
use crate::gradient_domain::OpaqueSrgb8AffineSegment;
use crate::observation::SchemaOrderedFieldSourceV1;

/// One opaque encoded-sRGB affine input over the entire real interval [0, 1].
///
/// Other inputs are constant. Root RGB8 uses nearest half-up rounding; each
/// material uses the Program's existing composition law. This declaration
/// does not assert that a browser rasterizer has the same output support.
#[derive(Debug, Clone, Copy)]
pub struct ProgramDeclaredFieldV1<'a> {
    rgb8: &'a [u8],
    varying_input: usize,
    stops: FieldStops<'a>,
}

/// Malformed schema-ordered field declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramDeclaredFieldShapeErrorV1;

impl<'a> ProgramDeclaredFieldV1<'a> {
    /// Borrows one RGB triple per compiled input and the varying input's stops.
    /// The triple at `varying_input` is ignored: its source is `from` → `to`.
    /// No samples or subdivisions are accepted from the caller.
    pub fn from_rgb8(
        rgb8: &'a [u8],
        varying_input: usize,
        from: Srgb8,
        to: Srgb8,
    ) -> Result<Self, ProgramDeclaredFieldShapeErrorV1> {
        if rgb8.len() % 3 != 0 || varying_input >= rgb8.len() / 3 {
            return Err(ProgramDeclaredFieldShapeErrorV1);
        }
        Ok(Self {
            rgb8,
            varying_input,
            stops: FieldStops::Affine(OpaqueSrgb8AffineSegment::new(from.bytes(), to.bytes())),
        })
    }
    /// A continuous piecewise-affine line in encoded sRGB. Positions are finite,
    /// strictly increasing, start at zero and end at one. There is one RGB8
    /// triple per position and one constant RGB8 triple per compiled input.
    pub fn from_gradient_rgb8(
        rgb8: &'a [u8],
        varying_input: usize,
        positions: &'a [f64],
        colors: &'a [u8],
    ) -> Result<Self, ProgramDeclaredFieldShapeErrorV1> {
        if rgb8.len() % 3 != 0
            || varying_input >= rgb8.len() / 3
            || positions.len() < 2
            || positions.len().checked_mul(3) != Some(colors.len())
            || positions.first() != Some(&0.0)
            || positions.last() != Some(&1.0)
            || !positions.windows(2).all(|pair| pair[0] < pair[1])
        {
            return Err(ProgramDeclaredFieldShapeErrorV1);
        }
        Ok(Self {
            rgb8,
            varying_input,
            stops: FieldStops::Positioned { positions, colors },
        })
    }
}

impl SchemaOrderedFieldSourceV1 for ProgramDeclaredFieldV1<'_> {
    fn value_count(&self) -> usize {
        self.rgb8.len() / 3
    }

    fn value(&self, input: usize) -> Srgb8 {
        let offset = input * 3;
        Srgb8::new([
            self.rgb8[offset],
            self.rgb8[offset + 1],
            self.rgb8[offset + 2],
        ])
    }

    fn varying_input_index(&self) -> usize {
        self.varying_input
    }

    fn stops(&self) -> FieldStops<'_> {
        self.stops
    }
}

/// An exact reachable coordinate for a canonical physical case in a report.
/// It is a diagnostic witness, not the definition or extent of the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramFieldWitnessV1 {
    segment_index: usize,
    numerator: u16,
    denominator: u16,
    nominal: Srgb8,
}

impl ProgramFieldWitnessV1 {
    /// Index of the first stop of this witness segment.
    pub const fn segment_index(self) -> usize {
        self.segment_index
    }

    /// Exact local parameter t = numerator / denominator within this segment.
    pub const fn coordinate(self) -> (u16, u16) {
        (self.numerator, self.denominator)
    }

    /// Nominal root color at that coordinate.
    pub const fn nominal(self) -> Srgb8 {
        self.nominal
    }
}

/// Scope and coordinates of the declared-field result in this snapshot.
///
/// Ready means the entire declared model satisfies the Program. It is not a
/// claim about gradient rasterization. Consumers must retain this scope on
/// both positive and negative results; renderer coverage remains unknown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramFieldAssessmentV1 {
    varying_input: usize,
    stops: Vec<ProgramGradientStopV1>,
    region_count: usize,
    witnesses: Vec<ProgramFieldWitnessV1>,
}

impl ProgramFieldAssessmentV1 {
    /// Index in the compiled input schema.
    pub const fn input_index(&self) -> usize {
        self.varying_input
    }

    /// Authored endpoints. Reversal retains different coordinate provenance.
    pub fn endpoints(&self) -> (Srgb8, Srgb8) {
        (
            self.stops[0].color(),
            self.stops[self.stops.len() - 1].color(),
        )
    }

    /// All positive-width segments, in authored order.
    pub fn stops(&self) -> &[ProgramGradientStopV1] {
        &self.stops
    }

    /// Authoritative source model; materials keep the compiled Program law.
    pub const fn source_model(&self) -> &'static str {
        if self.stops.len() == 2 {
            "opaque-affine-encoded-srgb8-half-up-v1"
        } else {
            "opaque-piecewise-affine-encoded-srgb8-half-up-v1"
        }
    }

    /// All real positions between the declared stops are included in the model.
    pub const fn domain(&self) -> &'static str {
        "whole-field"
    }

    /// No portable renderer allowance is inferred from nominal coverage.
    pub const fn rendering(&self) -> &'static str {
        "unknown"
    }

    /// Number of point and open-interval regions in the exact partition.
    pub const fn region_count(&self) -> usize {
        self.region_count
    }

    /// One exact reachable coordinate per canonical case, in report order.
    pub fn witnesses(&self) -> &[ProgramFieldWitnessV1] {
        &self.witnesses
    }
}

pub(super) fn copy_field_assessment(
    evidence: crate::program::SnapshotEvidenceV1<'_>,
) -> Result<Option<ProgramFieldAssessmentV1>, ProgramRuntimeErrorV1> {
    let Some(observation) = evidence.observation() else {
        return Ok(None);
    };
    let Some(field) = observation.declared_field() else {
        return Ok(None);
    };
    let count = observation.physical_cases().len();
    let mut witnesses = Vec::new();
    witnesses
        .try_reserve_exact(count)
        .map_err(|_| ProgramRuntimeErrorV1::ResourceExhausted)?;
    for case in 0..count {
        let cell = field
            .witness(case)
            .ok_or(ProgramRuntimeErrorV1::InternalInvariant)?;
        let coordinate = cell.local.representative();
        witnesses.push(ProgramFieldWitnessV1 {
            segment_index: cell.segment_index,
            numerator: coordinate.numerator(),
            denominator: coordinate.denominator(),
            nominal: Srgb8::new(cell.local.nominal()),
        });
    }
    let mut stops = Vec::new();
    stops
        .try_reserve_exact(field.stops().len())
        .map_err(|_| ProgramRuntimeErrorV1::ResourceExhausted)?;
    stops.extend_from_slice(field.stops());
    Ok(Some(ProgramFieldAssessmentV1 {
        varying_input: field.varying_input_index(),
        stops,
        region_count: field.cell_count(),
        witnesses,
    }))
}

impl ProgramSessionV1 {
    /// Solves one tuple of paints for the complete declared nominal field.
    /// The returned snapshot explicitly reports unknown renderer coverage.
    pub fn update_declared_field(
        &mut self,
        revision: u64,
        field: ProgramDeclaredFieldV1<'_>,
    ) -> Result<ProgramSnapshotV1, ProgramRuntimeErrorV1> {
        self.prepare_declared_field(revision, field)
            .map(PreparedProgramUpdateV1::commit)
    }

    /// Prepares a complete model result without changing the accepted context.
    /// Dropping the proposal preserves the previous revision and composition.
    pub fn prepare_declared_field(
        &mut self,
        revision: u64,
        field: ProgramDeclaredFieldV1<'_>,
    ) -> Result<PreparedProgramUpdateV1<'_>, ProgramRuntimeErrorV1> {
        // Existing legacy point certificates do not describe continuous fields.
        if self.owner.snapshot_version() != 2 {
            return Err(ProgramRuntimeErrorV1::Update);
        }
        let transition = self
            .owner
            .prepare_schema_ordered_field_update(&mut self.session, revision, &field)
            .map_err(map_runtime_update_error)?;
        let evidence = transition.snapshot_evidence();
        let count = evidence
            .observation()
            .ok_or(ProgramRuntimeErrorV1::InternalInvariant)?
            .physical_cases()
            .len();
        Self::reserve_computed_snapshot(
            &self.owner,
            &mut self.outputs_scratch,
            &mut self.assessment_scratch,
            count,
        )?;
        let snapshot = snapshot_from_computed_evidence_into(
            evidence,
            &mut self.outputs_scratch,
            &mut self.assessment_scratch,
        )?;
        Ok(PreparedProgramUpdateV1 {
            transition,
            snapshot,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::wire::ProgramWireBuilderV1;

    fn session() -> ProgramSessionV1 {
        let mut builder = ProgramWireBuilderV1::new();
        builder
            .computed_gray_target(2, 0, 255, Srgb8::new([0; 3]), 2.0)
            .surface_input_port(6)
            .solid_paint(3, 2)
            .input_surface(7, 6)
            .source_over_occurrence(8, 3, 7, 64.0, 0.2, 1)
            .wcag22_visible_unary(true, 10, 8, 1)
            .output(17, 3);
        compile_program_wire(
            &builder
                .finish_assignment(&[2])
                .expect("computed fixture paint is a valid assignment output"),
        )
        .expect("the fixture graph and WCAG constraint compile")
        .instantiate(1)
        .expect("a compiled fixture instantiates a fresh session")
    }

    fn field(from: [u8; 3], to: [u8; 3]) -> ProgramDeclaredFieldV1<'static> {
        ProgramDeclaredFieldV1::from_rgb8(&[0; 3], 0, Srgb8::new(from), Srgb8::new(to))
            .expect("one RGB input and varying index zero define a valid field")
    }

    #[test]
    fn full_field_rejects_interior_failure_and_preserves_coordinate_diagnostics() {
        let mut runtime = session();
        let endpoints = runtime
            .update_observed(
                1,
                &[
                    ProgramScenarioV1::new(1, vec![Srgb8::new([255, 0, 0])]),
                    ProgramScenarioV1::new(2, vec![Srgb8::new([0, 255, 0])]),
                ],
            )
            .expect("fresh endpoint scenarios are admitted");
        assert_eq!(endpoints.state(), ProgramSnapshotStateV1::Ready);
        assert!(endpoints.field_assessment().is_none());
        let rejected = runtime
            .update_declared_field(2, field([255, 0, 0], [0, 255, 0]))
            .expect("a newer valid field produces a diagnostic snapshot");
        assert_eq!(rejected.state(), ProgramSnapshotStateV1::Unsatisfied);
        assert!(rejected.outputs().is_empty());
        let scope = rejected
            .field_assessment()
            .expect("declared-field snapshots retain their assessed domain");
        assert_eq!(scope.rendering(), "unknown");
        assert_eq!(scope.domain(), "whole-field");
        assert_eq!(scope.region_count(), 513);
        assert_eq!(scope.witnesses().len(), 511);
        assert!(
            rejected
                .diagnostics()
                .expect("an unsatisfied field retains its violation diagnostics")
                .witness()
                .is_some()
        );
        for witness in scope.witnesses() {
            let (n, d) = witness.coordinate();
            assert!(d > 0 && n <= d);
            // Independent integer channel interpolation, including half ties.
            let q = |value: u32| ((2 * value + u32::from(d)) / (2 * u32::from(d))) as u8;
            assert_eq!(
                witness.nominal().bytes(),
                [q(255 * u32::from(d - n)), q(255 * u32::from(n)), 0]
            );
        }
        assert_eq!(endpoints.state(), ProgramSnapshotStateV1::Ready);
        assert!(endpoints.field_assessment().is_none());
    }

    #[test]
    fn nominal_field_matches_full_finite_oracle_without_claiming_observed_pixels() {
        let declared = field([192; 3], [255; 3]);
        let mut modeled = session();
        let result = modeled
            .update_declared_field(1, declared)
            .expect("a fresh valid field is admitted");
        let mut finite = session();
        let points: Vec<_> = (192..=255)
            .map(|value| ProgramScenarioV1::new(u32::from(value), vec![Srgb8::new([value; 3])]))
            .collect();
        let oracle = finite
            .update_observed(1, &points)
            .expect("the finite byte oracle is a valid fresh observation");
        assert_eq!(result.state(), ProgramSnapshotStateV1::Ready);
        assert_eq!(result.outputs(), oracle.outputs());
        assert_eq!(result.optimality(), oracle.optimality());
        assert_eq!(
            result
                .field_assessment()
                .expect("modeled field admission retains its rendering qualification")
                .rendering(),
            "unknown"
        );
        assert!(oracle.field_assessment().is_none());
    }

    #[test]
    fn dropped_field_proposal_does_not_accept_revision_or_replace_previous_context() {
        let mut runtime = session();
        let original = runtime
            .update_observed(1, &[ProgramScenarioV1::new(1, vec![Srgb8::new([255; 3])])])
            .expect("the initial finite observation is valid");
        let retained = {
            let proposed = runtime
                .prepare_declared_field(2, field([192; 3], [255; 3]))
                .expect("a newer field can be prepared without committing");
            assert_eq!(proposed.snapshot().state(), ProgramSnapshotStateV1::Ready);
            proposed.snapshot().clone()
        };
        // A different context can reuse revision 2 because the proposal was dropped.
        let next = runtime
            .update_observed(2, &[ProgramScenarioV1::new(1, vec![Srgb8::new([224; 3])])])
            .expect("dropping the field proposal leaves its revision available");
        assert_eq!(next.state(), ProgramSnapshotStateV1::Ready);
        assert!(next.field_assessment().is_none());
        assert!(original.field_assessment().is_none());
        assert_eq!(
            retained
                .field_assessment()
                .expect("the retained proposal snapshot keeps its own field descriptor")
                .endpoints(),
            (Srgb8::new([192; 3]), Srgb8::new([255; 3]))
        );
    }

    #[test]
    fn reversal_is_not_a_replay_and_unknown_does_not_reuse_field_scope() {
        let mut runtime = session();
        let first = runtime
            .update_declared_field(1, field([192; 3], [255; 3]))
            .expect("the first field establishes revision one");
        assert_eq!(
            runtime.update_declared_field(1, field([255; 3], [192; 3])),
            Err(ProgramRuntimeErrorV1::Update)
        );
        let reversed = runtime
            .update_declared_field(2, field([255; 3], [192; 3]))
            .expect("a reversed field is admitted at a newer revision");
        assert_eq!(first.outputs(), reversed.outputs());
        assert_ne!(first.field_assessment(), reversed.field_assessment());
        assert!(
            runtime
                .update_unknown(3, 1)
                .expect("a newer unknown context is admitted")
                .field_assessment()
                .is_none()
        );
        assert!(first.field_assessment().is_some());
    }
}
