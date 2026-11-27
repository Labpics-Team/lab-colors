use core::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};

use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

use crate::Srgb8;
use crate::appearance::{
    AppearanceBindings, AppearanceGraphSpec, BindingError, CompositionProfileV1,
    EncodedPointPaintValueV1, OccurrenceId, OccurrenceSpec, PaintId, PaintInputId, PaintSpec,
    PointOpacityError, PointOpacityOverSurfaceV1, ResolvedOccurrence, SurfaceId,
    SurfaceInputPortId, SurfaceSpec,
};
use crate::gradient_domain::{OpaqueSrgb8AffineSegment, SegmentCoordinate, SegmentRegion};
use crate::lcs_occurrence::ColorSignal;
use crate::observation::{
    CanonicalObservationSchemaV1, ObservationArenaPoolV1, ObservationError, ObservationHeadViewV1,
    ObservationOwnerV1, ObservationPayloadInput, ObservationStreamId, ObservationUpdateInput,
    ObservedScenarioSetInput, PreparedObservationUpdateV1, Revision, RevisionBoundObservationV1,
    RevisionBoundUnknownV1, ScenarioId, ScenarioInput, SchemaOrderedFieldSourceV1,
    SchemaOrderedScenarioSourceV1, SurfaceInputBinding, UnknownReasonId,
    canonicalize_observation_schema, prepare_observation, prepare_schema_ordered_field_observation,
    prepare_schema_ordered_observation,
};

const PORT_A: SurfaceInputPortId = SurfaceInputPortId::new(10);
const PORT_B: SurfaceInputPortId = SurfaceInputPortId::new(20);
const STREAM: ObservationStreamId = ObservationStreamId::new(7);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UpdateDisposition {
    Applied,
    Idempotent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TestOwner {
    Empty,
    Unknown(RevisionBoundUnknownV1),
    Observed(RevisionBoundObservationV1),
}

impl ObservationOwnerV1 for TestOwner {
    fn observation_head(&self) -> ObservationHeadViewV1<'_> {
        match self {
            Self::Empty => ObservationHeadViewV1::Empty,
            Self::Unknown(unknown) => ObservationHeadViewV1::Unknown(unknown),
            Self::Observed(observation) => ObservationHeadViewV1::Observed(observation),
        }
    }
}

#[derive(Debug)]
struct TestState {
    stream: ObservationStreamId,
    schema: CanonicalObservationSchemaV1,
    arenas: ObservationArenaPoolV1,
    owner: TestOwner,
}

impl Clone for TestState {
    fn clone(&self) -> Self {
        let schema = self.schema.clone();
        Self {
            stream: self.stream,
            arenas: ObservationArenaPoolV1::new(&schema),
            schema,
            owner: self.owner.clone(),
        }
    }
}

impl PartialEq for TestState {
    fn eq(&self, other: &Self) -> bool {
        self.stream == other.stream && self.schema == other.schema && self.owner == other.owner
    }
}

impl Eq for TestState {}

impl TestState {
    fn new(
        stream: ObservationStreamId,
        schema: Vec<SurfaceInputPortId>,
    ) -> Result<Self, ObservationError> {
        let schema = canonicalize_observation_schema(schema)?;
        Ok(Self {
            stream,
            arenas: ObservationArenaPoolV1::new(&schema),
            schema,
            owner: TestOwner::Empty,
        })
    }

    fn prepare(
        &mut self,
        update: ObservationUpdateInput,
    ) -> Result<PreparedObservationUpdateV1<'_, TestOwner>, ObservationError> {
        prepare_observation(
            &mut self.owner,
            &mut self.arenas,
            self.stream,
            &self.schema,
            update,
        )
    }

    fn apply(
        &mut self,
        update: ObservationUpdateInput,
    ) -> Result<UpdateDisposition, ObservationError> {
        match self.prepare(update)? {
            PreparedObservationUpdateV1::Idempotent(prepared) => {
                let _owner = prepared.into_owner();
                Ok(UpdateDisposition::Idempotent)
            }
            PreparedObservationUpdateV1::Unknown(prepared) => {
                let (owner, unknown) = prepared.into_parts();
                *owner = TestOwner::Unknown(unknown);
                Ok(UpdateDisposition::Applied)
            }
            PreparedObservationUpdateV1::Observed(prepared) => {
                let (owner, observation) = prepared.into_parts();
                *owner = TestOwner::Observed(observation);
                Ok(UpdateDisposition::Applied)
            }
        }
    }

    fn head(&self) -> ObservationHeadViewV1<'_> {
        self.owner.observation_head()
    }

    fn current_observation(&self) -> Option<&RevisionBoundObservationV1> {
        match &self.owner {
            TestOwner::Observed(observation) => Some(observation),
            TestOwner::Empty | TestOwner::Unknown(_) => None,
        }
    }

    fn current_unknown(&self) -> Option<RevisionBoundUnknownV1> {
        match &self.owner {
            TestOwner::Unknown(unknown) => Some(*unknown),
            TestOwner::Empty | TestOwner::Observed(_) => None,
        }
    }
}

#[derive(Clone)]
struct DeclaredFieldSource {
    values: Vec<Srgb8>,
    varying: usize,
    segment: OpaqueSrgb8AffineSegment,
}

impl SchemaOrderedFieldSourceV1 for DeclaredFieldSource {
    fn value_count(&self) -> usize {
        self.values.len()
    }
    fn value(&self, index: usize) -> Srgb8 {
        assert_ne!(
            index, self.varying,
            "the descriptor is the sole varying source"
        );
        self.values[index]
    }
    fn field_count(&self) -> usize {
        1
    }

    fn field_input(&self, _ordinal: usize) -> usize {
        self.varying
    }
    fn field_stops(&self, _ordinal: usize) -> crate::field_context::FieldStops<'_> {
        crate::field_context::FieldStops::Affine(self.segment)
    }
}

fn apply_declared_field(
    state: &mut TestState,
    revision: u64,
    source: &DeclaredFieldSource,
) -> Result<UpdateDisposition, ObservationError> {
    match prepare_schema_ordered_field_observation(
        &mut state.owner,
        &mut state.arenas,
        state.stream,
        &state.schema,
        Revision::new(revision),
        source,
    )? {
        PreparedObservationUpdateV1::Idempotent(prepared) => {
            let _owner = prepared.into_owner();
            Ok(UpdateDisposition::Idempotent)
        }
        PreparedObservationUpdateV1::Observed(prepared) => {
            let (owner, observation) = prepared.into_parts();
            *owner = TestOwner::Observed(observation);
            Ok(UpdateDisposition::Applied)
        }
        PreparedObservationUpdateV1::Unknown(_) => panic!("a declared field is available"),
    }
}

#[test]
fn declared_field_keeps_exact_interior_tie_coordinate_and_complete_regions() {
    let mut state = TestState::new(STREAM, vec![PORT_A, PORT_B])
        .expect("distinct fixture ports form a valid schema");
    let source = DeclaredFieldSource {
        values: vec![Srgb8::new([0; 3]), Srgb8::new([19, 29, 39])],
        varying: 0,
        segment: OpaqueSrgb8AffineSegment::new([255, 0, 0], [0, 255, 0]),
    };
    apply_declared_field(&mut state, 1, &source)
        .expect("valid field at the current or next revision is admitted");
    let observation = revision_bound(&state);
    let field = observation
        .declared_field()
        .expect("declared admission retains its field descriptor");
    assert_eq!(
        (
            field.stops()[0].color().bytes(),
            field.stops()[1].color().bytes()
        ),
        source.segment.endpoints()
    );
    assert_eq!(observation.physical_case_count(), 511);
    assert_eq!(field.cell_count(), 513);
    let mut regions = 0;
    let mut tie = None;
    for case in 0..observation.physical_case_count() {
        let values = observation
            .physical_values(case)
            .expect("case index is within the admitted physical support");
        assert!(values.as_slice().is_none());
        assert_eq!(values.len(), 2);
        assert_eq!(values[1].srgb8(), source.values[1]);
        assert_eq!(values.iter().len(), 2);
        assert_eq!(
            values
                .iter()
                .next_back()
                .expect("the two-input field has a last value")
                .srgb8(),
            source.values[1]
        );
        assert_eq!(observation.provenance(case), Some([].as_slice()));
        for cell in field
            .cells(case)
            .expect("each admitted physical case has coordinate cells")
        {
            regions += 1;
            assert_eq!(cell.local.nominal(), values[0].srgb8().bytes());
            assert_eq!(
                source.segment.at(cell.local.representative()),
                cell.local.nominal()
            );
            if let SegmentRegion::Point(at) = cell.local.region() {
                if at.numerator() == 1 && at.denominator() == 2 {
                    tie = Some(cell.local.nominal());
                }
            }
        }
    }
    assert_eq!(regions, 513);
    assert_eq!(tie, Some([128, 128, 0]));
    assert!(field.cells(511).is_none());
    assert!(observation.physical_values(511).is_none());
}

#[test]
fn reversed_declared_field_conflicts_even_when_physical_support_is_equal() {
    let mut state =
        TestState::new(STREAM, vec![PORT_A]).expect("distinct fixture ports form a valid schema");
    let forward = DeclaredFieldSource {
        values: vec![Srgb8::new([0; 3])],
        varying: 0,
        segment: OpaqueSrgb8AffineSegment::new([255, 0, 0], [0, 255, 0]),
    };
    assert_eq!(
        apply_declared_field(&mut state, 1, &forward)
            .expect("valid field at the current or next revision is admitted"),
        UpdateDisposition::Applied
    );
    assert_eq!(
        apply_declared_field(&mut state, 1, &forward)
            .expect("valid field at the current or next revision is admitted"),
        UpdateDisposition::Idempotent
    );
    let mut reversed = forward.clone();
    reversed.segment = OpaqueSrgb8AffineSegment::new([0, 255, 0], [255, 0, 0]);
    assert_eq!(
        apply_declared_field(&mut state, 1, &reversed),
        Err(ObservationError::RevisionConflict {
            revision: Revision::new(1)
        }),
    );
    let old = revision_bound(&state).clone();
    apply_declared_field(&mut state, 2, &reversed)
        .expect("valid field at the current or next revision is admitted");
    let current = revision_bound(&state);
    assert_eq!(old.physical_case_count(), current.physical_case_count());
    for case in 0..old.physical_case_count() {
        assert_eq!(old.physical_values(case), current.physical_values(case));
    }
    assert_ne!(
        old.declared_field()
            .expect("declared admission retains its field descriptor")
            .stops(),
        current
            .declared_field()
            .expect("declared admission retains its field descriptor")
            .stops()
    );
}

#[test]
fn constant_declared_field_is_not_collapsed_into_a_finite_observation() {
    let mut state = TestState::new(STREAM, vec![PORT_A, PORT_B])
        .expect("distinct fixture ports form a valid schema");
    let source = DeclaredFieldSource {
        values: vec![Srgb8::new([7; 3]); 2],
        varying: 0,
        segment: OpaqueSrgb8AffineSegment::new([7; 3], [7; 3]),
    };
    apply_declared_field(&mut state, 1, &source)
        .expect("valid field at the current or next revision is admitted");
    let observation = revision_bound(&state);
    assert_eq!(observation.physical_case_count(), 1);
    let regions: Vec<_> = observation
        .declared_field()
        .expect("constant field remains a declared context")
        .cells(0)
        .expect("a constant field has cells for its single physical case")
        .iter()
        .map(|cell| cell.local.region())
        .collect();
    assert_eq!(
        regions,
        vec![
            SegmentRegion::Point(SegmentCoordinate::ZERO),
            SegmentRegion::OpenInterval {
                lower: SegmentCoordinate::ZERO,
                upper: SegmentCoordinate::ONE
            },
            SegmentRegion::Point(SegmentCoordinate::ONE),
        ]
    );
    let mut moved_input = source.clone();
    moved_input.varying = 1;
    assert_eq!(
        apply_declared_field(&mut state, 1, &moved_input),
        Err(ObservationError::RevisionConflict {
            revision: Revision::new(1)
        })
    );
    let finite = observed_update(
        STREAM,
        1,
        scenarios([scenario(
            1,
            [binding(PORT_A, [7; 3]), binding(PORT_B, [7; 3])],
        )]),
    );
    assert_eq!(
        state.apply(finite),
        Err(ObservationError::RevisionConflict {
            revision: Revision::new(1)
        })
    );

    let old = revision_bound(&state).clone();
    state
        .apply(observed_update(
            STREAM,
            2,
            scenarios([scenario(
                1,
                [binding(PORT_A, [7; 3]), binding(PORT_B, [7; 3])],
            )]),
        ))
        .expect("a newer finite observation replaces the declared field");
    assert_eq!(
        old.physical_values(0),
        revision_bound(&state).physical_values(0)
    );
    assert!(revision_bound(&state).declared_field().is_none());
    assert_eq!(
        apply_declared_field(&mut state, 2, &source),
        Err(ObservationError::RevisionConflict {
            revision: Revision::new(2)
        })
    );
}

#[test]
fn retained_field_descriptor_coordinates_and_constants_survive_pool_reuse() {
    let mut state = TestState::new(STREAM, vec![PORT_A, PORT_B])
        .expect("distinct fixture ports form a valid schema");
    let mut source = DeclaredFieldSource {
        values: vec![Srgb8::new([0; 3]), Srgb8::new([10; 3])],
        varying: 0,
        segment: OpaqueSrgb8AffineSegment::new([0; 3], [2; 3]),
    };
    apply_declared_field(&mut state, 1, &source)
        .expect("valid field at the current or next revision is admitted");
    let old = revision_bound(&state).clone();
    let descriptor = old
        .declared_field()
        .expect("declared admission retains its field descriptor")
        .stops()
        .to_vec();
    let cells = old
        .declared_field()
        .expect("declared admission retains its field descriptor")
        .cells(0)
        .expect("the retained first physical case has coordinate cells")
        .to_vec();
    for revision in 2..8 {
        source.values[1] = Srgb8::new([revision as u8; 3]);
        source.segment = OpaqueSrgb8AffineSegment::new([revision as u8; 3], [20; 3]);
        apply_declared_field(&mut state, revision, &source)
            .expect("valid field at the current or next revision is admitted");
        assert_eq!(old.revision(), Revision::new(1));
        assert_eq!(
            old.declared_field()
                .expect("declared admission retains its field descriptor")
                .stops(),
            descriptor
        );
        assert_eq!(
            old.declared_field()
                .expect("declared admission retains its field descriptor")
                .cells(0)
                .expect("the retained first physical case has coordinate cells"),
            cells
        );
        assert_eq!(
            old.physical_values(0)
                .expect("a retained nonempty field still has its first physical case")[1]
                .srgb8()
                .bytes(),
            [10; 3]
        );
    }
}

#[test]
fn declared_field_storage_grows_by_inputs_plus_support_not_their_product() {
    let measure = |input_count| {
        let mut state = TestState::new(
            STREAM,
            (0..input_count)
                .map(|i| SurfaceInputPortId::new(i as u32))
                .collect(),
        )
        .expect("generated fixture port identifiers are distinct");
        let source = DeclaredFieldSource {
            values: vec![Srgb8::new([17; 3]); input_count],
            varying: input_count - 1,
            segment: OpaqueSrgb8AffineSegment::new([255, 0, 0], [0, 255, 0]),
        };
        apply_declared_field(&mut state, 1, &source)
            .expect("valid field at the current or next revision is admitted");
        let observation = revision_bound(&state);
        let field = observation
            .declared_field()
            .expect("declared admission retains its field descriptor");
        assert_eq!(field.stored_value_count_for_test(), input_count + 511);
        assert_eq!(field.cell_count(), 513);
        for case in 0..observation.physical_case_count() {
            let values = observation
                .physical_values(case)
                .expect("case index is within the admitted physical support");
            assert_eq!(values.len(), input_count);
            assert_eq!(values.get(input_count), None);
        }
        field.stored_value_count_for_test()
    };
    assert_eq!(measure(4096) - measure(1), 4095);
}

#[test]
fn lifecycle_declared_field_preserves_revision_order_across_context_kinds() {
    let field = DeclaredFieldSource {
        values: vec![Srgb8::new([0; 3])],
        varying: 0,
        segment: OpaqueSrgb8AffineSegment::new([0; 3], [1; 3]),
    };
    for context in ["unknown", "finite", "field"] {
        let mut state = TestState::new(STREAM, vec![PORT_A])
            .expect("one distinct port forms a valid observation schema");
        match context {
            "unknown" => state.apply(unknown_update(STREAM, 2, 1)),
            "finite" => state.apply(observed_update(
                STREAM,
                2,
                scenarios([scenario(1, [binding(PORT_A, [0; 3])])]),
            )),
            "field" => apply_declared_field(&mut state, 2, &field),
            _ => unreachable!(),
        }
        .expect("a first valid context establishes revision two");
        let before = state.clone();
        assert_eq!(
            apply_declared_field(&mut state, 1, &field),
            Err(ObservationError::RevisionOutOfOrder {
                current: Revision::new(2),
                incoming: Revision::new(1),
            }),
            "stale field after {context}",
        );
        assert_eq!(state, before, "rejected field must retain {context}");
        assert_eq!(
            apply_declared_field(&mut state, 3, &field),
            Ok(UpdateDisposition::Applied),
            "a newer field after {context} must remain admissible",
        );
        assert_eq!(
            apply_declared_field(&mut state, 3, &field),
            Ok(UpdateDisposition::Idempotent),
            "the exact current field remains an idempotent replay",
        );
    }
}

#[test]
fn malformed_declared_field_is_rejected_before_revision_comparison() {
    let mut state =
        TestState::new(STREAM, vec![PORT_A]).expect("distinct fixture ports form a valid schema");
    let mut source = DeclaredFieldSource {
        values: vec![Srgb8::new([0; 3])],
        varying: 0,
        segment: OpaqueSrgb8AffineSegment::new([0; 3], [1; 3]),
    };
    apply_declared_field(&mut state, 2, &source)
        .expect("valid field at the current or next revision is admitted");
    source.varying = 1;
    assert_eq!(
        apply_declared_field(&mut state, 1, &source),
        Err(ObservationError::DeclaredFieldInputOutOfBounds {
            input_index: 1,
            input_count: 1
        })
    );
    source.values.clear();
    assert_eq!(
        apply_declared_field(&mut state, 1, &source),
        Err(ObservationError::DeclaredFieldValueCountMismatch {
            expected: 1,
            actual: 0
        })
    );
    assert_eq!(revision_bound(&state).revision(), Revision::new(2));
}

fn binding(port: SurfaceInputPortId, bytes: [u8; 3]) -> SurfaceInputBinding {
    SurfaceInputBinding::new(port, ColorSignal::from_srgb8(Srgb8::new(bytes)))
}

fn scenario(id: u32, bindings: impl IntoIterator<Item = SurfaceInputBinding>) -> ScenarioInput {
    ScenarioInput {
        id: ScenarioId::new(id),
        bindings: bindings.into_iter().collect(),
    }
}

fn scenarios(items: impl IntoIterator<Item = ScenarioInput>) -> ObservedScenarioSetInput {
    ObservedScenarioSetInput {
        scenarios: items.into_iter().collect(),
    }
}

fn observed_update(
    stream: ObservationStreamId,
    revision: u64,
    set: ObservedScenarioSetInput,
) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Scenarios(set),
    }
}

fn unknown_update(
    stream: ObservationStreamId,
    revision: u64,
    reason: u32,
) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Unknown(UnknownReasonId::new(reason)),
    }
}

fn paired_set(first: ([u8; 3], [u8; 3]), second: ([u8; 3], [u8; 3])) -> ObservedScenarioSetInput {
    scenarios([
        scenario(1, [binding(PORT_A, first.0), binding(PORT_B, first.1)]),
        scenario(2, [binding(PORT_A, second.0), binding(PORT_B, second.1)]),
    ])
}

fn revision_bound(state: &TestState) -> &RevisionBoundObservationV1 {
    state
        .current_observation()
        .expect("expected current admitted observation")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OrderedScenario {
    id: ScenarioId,
    values: Vec<Srgb8>,
}

#[derive(Debug)]
struct OrderedSource {
    scenarios: Vec<OrderedScenario>,
    scenario_id_reads: Cell<usize>,
    value_reads: Cell<usize>,
}

impl OrderedSource {
    fn new(scenarios: Vec<OrderedScenario>) -> Self {
        Self {
            scenarios,
            scenario_id_reads: Cell::new(0),
            value_reads: Cell::new(0),
        }
    }

    fn scenario_id_reads(&self) -> usize {
        self.scenario_id_reads.get()
    }

    fn value_reads(&self) -> usize {
        self.value_reads.get()
    }
}

impl SchemaOrderedScenarioSourceV1 for OrderedSource {
    fn scenario_count(&self) -> usize {
        self.scenarios.len()
    }

    fn scenario_id(&self, scenario_index: usize) -> ScenarioId {
        self.scenario_id_reads.set(self.scenario_id_reads.get() + 1);
        self.scenarios[scenario_index].id
    }

    fn value_count(&self, scenario_index: usize) -> usize {
        self.scenarios[scenario_index].values.len()
    }

    fn value(&self, scenario_index: usize, binding_index: usize) -> Srgb8 {
        self.value_reads.set(self.value_reads.get() + 1);
        self.scenarios[scenario_index].values[binding_index]
    }
}

struct UnrepresentableOrderedSource;

impl SchemaOrderedScenarioSourceV1 for UnrepresentableOrderedSource {
    fn scenario_count(&self) -> usize {
        usize::MAX
    }

    fn scenario_id(&self, _scenario_index: usize) -> ScenarioId {
        panic!("scratch preflight must precede every source access")
    }

    fn value_count(&self, _scenario_index: usize) -> usize {
        panic!("scratch preflight must precede every source access")
    }

    fn value(&self, _scenario_index: usize, _binding_index: usize) -> Srgb8 {
        panic!("scratch preflight must precede every source access")
    }
}

fn ordered_scenario(id: u32, values: impl IntoIterator<Item = [u8; 3]>) -> OrderedScenario {
    OrderedScenario {
        id: ScenarioId::new(id),
        values: values.into_iter().map(Srgb8::new).collect(),
    }
}

fn visit_permutations<T>(values: &mut [T], start: usize, visit: &mut impl FnMut(&[T])) {
    if start == values.len() {
        visit(values);
        return;
    }
    for index in start..values.len() {
        values.swap(start, index);
        visit_permutations(values, start + 1, visit);
        values.swap(start, index);
    }
}

fn prepare_ordered<'owner>(
    state: &'owner mut TestState,
    revision: u64,
    source: &OrderedSource,
    order_scratch: &mut Vec<usize>,
) -> Result<PreparedObservationUpdateV1<'owner, TestOwner>, ObservationError> {
    prepare_schema_ordered_observation(
        &mut state.owner,
        &mut state.arenas,
        state.stream,
        &state.schema,
        Revision::new(revision),
        source,
        order_scratch,
    )
}

#[test]
fn authored_paint_and_observed_surface_ports_are_distinct_through_execution() {
    let _sealed_adapter_contract: fn(
        [u8; 3],
        f64,
        [u8; 3],
    ) -> Result<ResolvedOccurrence, PointOpacityError> = PointOpacityOverSurfaceV1::evaluate;
    let paint_input = PaintInputId::new(7);
    let surface_port = SurfaceInputPortId::new(7);
    let paint = PaintId::new(1);
    let surface = SurfaceId::new(1);
    let occurrence = OccurrenceId::new(1);
    let graph = AppearanceGraphSpec::new(
        vec![paint_input],
        vec![surface_port],
        vec![],
        vec![PaintSpec::Input {
            id: paint,
            input: paint_input,
        }],
        vec![SurfaceSpec::Input {
            id: surface,
            port: surface_port,
        }],
        vec![OccurrenceSpec {
            id: occurrence,
            subject: paint,
            against: surface,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
    .compile()
    .expect("separate typed inputs must compile");

    let evaluation = graph
        .evaluate(&AppearanceBindings::new(
            vec![(
                paint_input,
                EncodedPointPaintValueV1::opaque(Srgb8::new([1, 2, 3])),
            )],
            vec![(surface_port, Srgb8::new([240, 241, 242]))],
            vec![],
        ))
        .expect("complete typed bindings must evaluate");
    assert_eq!(
        evaluation.occurrence(occurrence).unwrap().visible(),
        [1, 2, 3]
    );
    assert_eq!(
        graph.evaluate(&AppearanceBindings::new(
            vec![(
                paint_input,
                EncodedPointPaintValueV1::opaque(Srgb8::new([1, 2, 3])),
            )],
            vec![],
            vec![],
        )),
        Err(BindingError::MissingSurfaceInputBinding {
            input: surface_port,
        })
    );
}

#[test]
fn admission_preserves_correlated_tuples_without_cartesian_product() {
    let mut state = TestState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    state
        .apply(observed_update(
            STREAM,
            1,
            paired_set(([1, 2, 3], [4, 5, 6]), ([7, 8, 9], [10, 11, 12])),
        ))
        .unwrap();

    let observation = revision_bound(&state);
    assert_eq!(observation.schema(), &[PORT_A, PORT_B]);
    let cases: Vec<Vec<[u8; 3]>> = (0..observation.physical_case_count())
        .map(|case_index| {
            observation
                .physical_values(case_index)
                .expect("case index is within the admitted physical support")
                .iter()
                .copied()
                .map(ColorSignal::srgb8)
                .map(Srgb8::bytes)
                .collect()
        })
        .collect();
    assert_eq!(
        cases,
        vec![vec![[1, 2, 3], [4, 5, 6]], vec![[7, 8, 9], [10, 11, 12]]]
    );
    assert!(!cases.contains(&vec![[1, 2, 3], [10, 11, 12]]));
    assert!(!cases.contains(&vec![[7, 8, 9], [4, 5, 6]]));
}

#[test]
fn canonicalization_ignores_declaration_order_and_groups_duplicate_physics() {
    let first = scenarios([
        scenario(9, [binding(PORT_B, [4, 5, 6]), binding(PORT_A, [1, 2, 3])]),
        scenario(4, [binding(PORT_A, [9, 8, 7]), binding(PORT_B, [6, 5, 4])]),
        scenario(3, [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])]),
    ]);
    let second = scenarios([
        scenario(3, [binding(PORT_B, [4, 5, 6]), binding(PORT_A, [1, 2, 3])]),
        scenario(9, [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])]),
        scenario(4, [binding(PORT_B, [6, 5, 4]), binding(PORT_A, [9, 8, 7])]),
    ]);
    let mut left = TestState::new(STREAM, vec![PORT_B, PORT_A]).unwrap();
    let mut right = TestState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    left.apply(observed_update(STREAM, 1, first)).unwrap();
    right.apply(observed_update(STREAM, 1, second)).unwrap();

    assert_eq!(left, right);
    let observation = revision_bound(&left);
    assert_eq!(observation.schema(), &[PORT_A, PORT_B]);
    assert_eq!(observation.physical_case_count(), 2);
    assert_eq!(
        observation.provenance(0).unwrap(),
        &[ScenarioId::new(3), ScenarioId::new(9)]
    );
    assert_eq!(observation.provenance(1).unwrap(), &[ScenarioId::new(4)]);
    let physical_values: Vec<Vec<ColorSignal>> = (0..observation.physical_case_count())
        .map(|case_index| {
            observation
                .physical_values(case_index)
                .expect("case index is within the admitted physical support")
                .iter()
                .copied()
                .collect()
        })
        .collect();
    assert_eq!(
        physical_values,
        vec![
            vec![
                ColorSignal::from_srgb8(Srgb8::new([1, 2, 3])),
                ColorSignal::from_srgb8(Srgb8::new([4, 5, 6])),
            ],
            vec![
                ColorSignal::from_srgb8(Srgb8::new([9, 8, 7])),
                ColorSignal::from_srgb8(Srgb8::new([6, 5, 4])),
            ],
        ]
    );
}

#[test]
fn revision_bound_clone_is_allocation_free_and_shares_all_canonical_backing() {
    let mut state = TestState::new(STREAM, vec![PORT_B, PORT_A]).unwrap();
    state
        .apply(observed_update(
            STREAM,
            1,
            scenarios([
                scenario(9, [binding(PORT_B, [4, 5, 6]), binding(PORT_A, [1, 2, 3])]),
                scenario(3, [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])]),
            ]),
        ))
        .unwrap();

    let observation = revision_bound(&state);
    let backing_ptr = observation.backing_ptr_for_test();
    let schema_ptr = observation.schema_ptr_for_test();
    let (cloned, allocations) = crate::test_support::measured_allocations(|| observation.clone());

    assert_eq!(allocations, 0);
    assert_eq!(&cloned, observation);
    assert_eq!(cloned.backing_ptr_for_test(), backing_ptr);
    assert_eq!(cloned.schema_ptr_for_test(), schema_ptr);
    assert_eq!(cloned.schema(), observation.schema());
    assert_eq!(cloned.physical_values(0), observation.physical_values(0));
    assert_eq!(cloned.provenance(0), observation.provenance(0));
}

#[test]
fn retained_observations_survive_pool_exhaustion_and_reuse() -> Result<(), ObservationError> {
    let mut state = TestState::new(STREAM, vec![PORT_A])?;
    let update = |revision, channel| {
        observed_update(
            STREAM,
            revision,
            scenarios([scenario(1, [binding(PORT_A, [channel; 3])])]),
        )
    };
    state.apply(update(1, 10))?;
    let first = revision_bound(&state).clone();
    state.apply(update(2, 20))?;
    let second = revision_bound(&state).clone();
    state.apply(update(3, 30))?;
    let third = revision_bound(&state).clone();

    assert_eq!(
        state.apply(update(4, 40)),
        Err(ObservationError::InternalInvariant),
    );
    assert_eq!(revision_bound(&state).revision(), Revision::new(3));

    let released = second.backing_ptr_for_test();
    drop(second);
    state.apply(update(4, 40))?;
    assert_eq!(revision_bound(&state).backing_ptr_for_test(), released);
    for (observation, channel) in [(&first, 10), (&third, 30), (revision_bound(&state), 40)] {
        let expected = [ColorSignal::from_srgb8(Srgb8::new([channel; 3]))];
        assert_eq!(
            observation
                .physical_values(0)
                .expect("case index is within the admitted physical support")
                .as_slice(),
            Some(expected.as_slice())
        );
    }
    Ok(())
}

#[test]
fn switching_contexts_reuses_warm_buffers_of_both_kinds() -> Result<(), ObservationError> {
    let finite = OrderedSource::new(vec![
        ordered_scenario(1, [[10; 3], [20; 3]]),
        ordered_scenario(2, [[30; 3], [40; 3]]),
    ]);
    let field = DeclaredFieldSource {
        values: vec![Srgb8::new([0; 3]), Srgb8::new([19, 29, 39])],
        varying: 0,
        segment: OpaqueSrgb8AffineSegment::new([255, 0, 0], [0, 255, 0]),
    };
    for consecutive_fields in [true, false] {
        let mut state = TestState::new(STREAM, vec![PORT_A, PORT_B])?;
        let mut order_scratch = Vec::new();
        let mut revision = 0;
        let mut apply = |state: &mut TestState, is_field| -> Result<(), ObservationError> {
            revision += 1;
            if is_field {
                apply_declared_field(state, revision, &field)?;
            } else {
                let prepared = prepare_ordered(state, revision, &finite, &mut order_scratch)?;
                let PreparedObservationUpdateV1::Observed(prepared) = prepared else {
                    panic!("a higher revision prepares the finite fixture");
                };
                let (owner, observation) = prepared.into_parts();
                *owner = TestOwner::Observed(observation);
            }
            Ok(())
        };
        // Warm two backings of the repeated kind and one of the other kind.
        // No past revision is retained once the next update commits.
        for is_field in [
            !consecutive_fields,
            consecutive_fields,
            consecutive_fields,
            !consecutive_fields,
        ] {
            apply(&mut state, is_field)?;
        }
        let (result, events) = crate::test_support::measured_allocator_events(|| {
            for is_field in [consecutive_fields, consecutive_fields, !consecutive_fields] {
                apply(&mut state, is_field)?;
            }
            Ok::<_, ObservationError>(())
        });
        result?;
        assert_eq!(
            events,
            crate::test_support::AllocatorEvents::default(),
            "context switches must preserve warm buffers; consecutive_fields={consecutive_fields}",
        );
        assert_eq!(
            revision_bound(&state).declared_field().is_some(),
            !consecutive_fields,
        );
    }
    Ok(())
}

#[test]
fn mixed_retained_contexts_require_a_free_slot_before_replacing_its_kind()
-> Result<(), ObservationError> {
    let mut state = TestState::new(STREAM, vec![PORT_A])?;
    let finite = |revision, channel| {
        observed_update(
            STREAM,
            revision,
            scenarios([scenario(1, [binding(PORT_A, [channel; 3])])]),
        )
    };
    let field = DeclaredFieldSource {
        values: vec![Srgb8::new([0; 3])],
        varying: 0,
        segment: OpaqueSrgb8AffineSegment::new([255, 0, 0], [0, 255, 0]),
    };
    state.apply(finite(1, 10))?;
    let first = revision_bound(&state).clone();
    apply_declared_field(&mut state, 2, &field)?;
    let retained_field = revision_bound(&state).clone();
    state.apply(finite(3, 30))?;
    let current_finite = revision_bound(&state).clone();
    assert_eq!(
        apply_declared_field(&mut state, 4, &field),
        Err(ObservationError::InternalInvariant),
    );
    let released = first.backing_ptr_for_test();
    drop(first);
    apply_declared_field(&mut state, 4, &field)?;
    let current = revision_bound(&state);
    assert_eq!(current.backing_ptr_for_test(), released);
    assert_eq!(current_finite.revision(), Revision::new(3));
    assert_eq!(
        current_finite
            .physical_values(0)
            .expect("the finite case is retained")[0]
            .srgb8(),
        Srgb8::new([30; 3]),
    );
    assert_eq!(retained_field.revision(), Revision::new(2));
    assert_eq!(
        retained_field.physical_case_count(),
        current.physical_case_count()
    );
    for case in 0..current.physical_case_count() {
        assert_eq!(
            retained_field.physical_values(case),
            current.physical_values(case)
        );
    }
    Ok(())
}

#[test]
fn independent_equal_admissions_do_not_alias_observation_or_schema_backing() {
    let first = scenarios([
        scenario(9, [binding(PORT_B, [4, 5, 6]), binding(PORT_A, [1, 2, 3])]),
        scenario(3, [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])]),
    ]);
    let second = scenarios([
        scenario(3, [binding(PORT_B, [4, 5, 6]), binding(PORT_A, [1, 2, 3])]),
        scenario(9, [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])]),
    ]);
    let mut left = TestState::new(STREAM, vec![PORT_B, PORT_A]).unwrap();
    let mut right = TestState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    left.apply(observed_update(STREAM, 1, first)).unwrap();
    right.apply(observed_update(STREAM, 1, second)).unwrap();

    let left_observation = revision_bound(&left);
    let right_observation = revision_bound(&right);
    assert_eq!(left_observation, right_observation);
    assert!(
        !left_observation.shares_schema_backing_with(&right.schema),
        "equal schema values from another owner must not inherit authority",
    );
    assert_ne!(
        left_observation.backing_ptr_for_test(),
        right_observation.backing_ptr_for_test()
    );
    assert_ne!(
        left_observation.schema_ptr_for_test(),
        right_observation.schema_ptr_for_test()
    );
}

#[test]
fn schema_and_values_are_aligned_once_while_provenance_remains_complete() {
    let mut state = TestState::new(STREAM, vec![PORT_B, PORT_A]).unwrap();
    state
        .apply(observed_update(
            STREAM,
            1,
            scenarios([
                scenario(9, [binding(PORT_B, [4, 5, 6]), binding(PORT_A, [1, 2, 3])]),
                scenario(4, [binding(PORT_A, [9, 8, 7]), binding(PORT_B, [6, 5, 4])]),
                scenario(3, [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])]),
            ]),
        ))
        .unwrap();

    let observation = revision_bound(&state);
    let first_values = observation
        .physical_values(0)
        .expect("case index is within the admitted physical support")
        .as_slice()
        .expect("finite observations retain contiguous storage");
    let second_values = observation
        .physical_values(1)
        .expect("case index is within the admitted physical support")
        .as_slice()
        .expect("finite observations retain contiguous storage");
    assert_eq!(observation.schema(), &[PORT_A, PORT_B]);
    assert_eq!(
        first_values,
        &[
            ColorSignal::from_srgb8(Srgb8::new([1, 2, 3])),
            ColorSignal::from_srgb8(Srgb8::new([4, 5, 6])),
        ]
    );
    assert_eq!(
        second_values,
        &[
            ColorSignal::from_srgb8(Srgb8::new([9, 8, 7])),
            ColorSignal::from_srgb8(Srgb8::new([6, 5, 4])),
        ]
    );
    assert_eq!(
        observation.provenance(0),
        Some(&[ScenarioId::new(3), ScenarioId::new(9)][..])
    );
    assert_eq!(observation.provenance(1), Some(&[ScenarioId::new(4)][..]));
    assert_eq!(observation.physical_values(2), None);
    assert_eq!(observation.provenance(2), None);
}

#[test]
fn keyed_schema_is_intrinsic_to_revision_bound_observation_identity() {
    let mut left = TestState::new(STREAM, vec![PORT_A]).unwrap();
    let mut right = TestState::new(STREAM, vec![PORT_B]).unwrap();
    left.apply(observed_update(
        STREAM,
        1,
        scenarios([scenario(1, [binding(PORT_A, [7, 8, 9])])]),
    ))
    .unwrap();
    right
        .apply(observed_update(
            STREAM,
            1,
            scenarios([scenario(1, [binding(PORT_B, [7, 8, 9])])]),
        ))
        .unwrap();

    assert_ne!(revision_bound(&left), revision_bound(&right));
    assert_eq!(revision_bound(&left).schema(), &[PORT_A]);
    assert_eq!(revision_bound(&right).schema(), &[PORT_B]);
    assert_eq!(
        revision_bound(&left)
            .physical_values(0)
            .expect("case index is within the admitted physical support")
            .as_slice(),
        Some(&[ColorSignal::from_srgb8(Srgb8::new([7, 8, 9]))][..])
    );
    assert_eq!(
        revision_bound(&right)
            .physical_values(0)
            .expect("case index is within the admitted physical support")
            .as_slice(),
        Some(&[ColorSignal::from_srgb8(Srgb8::new([7, 8, 9]))][..])
    );

    let alternate_schema = canonicalize_observation_schema(vec![PORT_B]).unwrap();
    let mut alternate_arenas = ObservationArenaPoolV1::new(&alternate_schema);
    let before = left.clone();
    assert!(matches!(
        prepare_observation(
            &mut left.owner,
            &mut alternate_arenas,
            STREAM,
            &alternate_schema,
            observed_update(
                STREAM,
                1,
                scenarios([scenario(1, [binding(PORT_B, [7, 8, 9])])]),
            ),
        ),
        Err(ObservationError::RevisionConflict { revision })
            if revision == Revision::new(1)
    ));
    assert_eq!(left, before);
}

#[test]
fn stream_precedes_full_scenario_admission_which_precedes_revision_checks() {
    let mut state = TestState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    state.apply(unknown_update(STREAM, 4, 1)).unwrap();
    let before = state.clone();
    let malformed = || scenarios([scenario(1, [binding(PORT_A, [1; 3])])]);

    assert_eq!(
        state.apply(observed_update(
            ObservationStreamId::new(99),
            2,
            malformed(),
        )),
        Err(ObservationError::StreamMismatch {
            expected: STREAM,
            actual: ObservationStreamId::new(99),
        })
    );
    assert_eq!(state, before);

    assert_eq!(
        state.apply(observed_update(STREAM, 2, malformed())),
        Err(ObservationError::MissingSurfaceInputBinding {
            scenario: ScenarioId::new(1),
            input: PORT_B,
        })
    );
    assert_eq!(state, before);

    assert_eq!(
        state.apply(observed_update(STREAM, 4, malformed())),
        Err(ObservationError::MissingSurfaceInputBinding {
            scenario: ScenarioId::new(1),
            input: PORT_B,
        })
    );
    assert_eq!(state, before);

    let valid = || {
        scenarios([scenario(
            1,
            [binding(PORT_A, [1; 3]), binding(PORT_B, [2; 3])],
        )])
    };
    assert_eq!(
        state.apply(observed_update(STREAM, 2, valid())),
        Err(ObservationError::RevisionOutOfOrder {
            current: Revision::new(4),
            incoming: Revision::new(2),
        })
    );
    assert_eq!(state, before);

    assert_eq!(
        state.apply(observed_update(STREAM, 5, valid())),
        Ok(UpdateDisposition::Applied)
    );
    assert!(state.current_observation().is_some());
}

#[test]
fn initial_unknown_is_current_payload_and_exact_replay_is_idempotent() {
    let mut state = TestState::new(STREAM, vec![PORT_A]).unwrap();
    assert_eq!(state.head(), ObservationHeadViewV1::Empty);
    let prepared = state.prepare(unknown_update(STREAM, 3, 11)).unwrap();
    let PreparedObservationUpdateV1::Unknown(prepared) = prepared else {
        panic!("expected a prepared unknown payload");
    };
    assert_eq!(prepared.unknown().stream(), STREAM);
    assert_eq!(prepared.unknown().revision(), Revision::new(3));
    assert_eq!(prepared.unknown().reason(), UnknownReasonId::new(11));
    let (owner, unknown) = prepared.into_parts();
    assert_eq!(owner.observation_head(), ObservationHeadViewV1::Empty);
    *owner = TestOwner::Unknown(unknown);
    assert!(matches!(
        state.head(),
        ObservationHeadViewV1::Unknown(unknown)
            if unknown.revision() == Revision::new(3)
                && unknown.reason() == UnknownReasonId::new(11)
    ));
    let unknown = state.current_unknown().unwrap();
    assert_eq!(unknown.stream(), STREAM);
    assert_eq!(unknown.revision(), Revision::new(3));
    assert_eq!(unknown.reason(), UnknownReasonId::new(11));
    assert!(state.current_observation().is_none());

    let before = state.clone();
    assert_eq!(
        state.apply(unknown_update(STREAM, 3, 11)),
        Ok(UpdateDisposition::Idempotent)
    );
    assert_eq!(state, before);
    assert_eq!(
        state.apply(unknown_update(STREAM, 3, 12)),
        Err(ObservationError::RevisionConflict {
            revision: Revision::new(3),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn observed_to_unknown_replaces_raw_payload_instead_of_duplicating_it() {
    let mut state = TestState::new(STREAM, vec![PORT_A]).unwrap();
    state
        .apply(observed_update(
            STREAM,
            1,
            scenarios([scenario(1, [binding(PORT_A, [3, 4, 5])])]),
        ))
        .unwrap();
    let old_revision = revision_bound(&state).revision();
    let old_schema = revision_bound(&state).schema().to_vec();
    let old_values: Vec<_> = revision_bound(&state)
        .physical_values(0)
        .expect("case index is within the admitted physical support")
        .iter()
        .copied()
        .collect();
    state.apply(unknown_update(STREAM, 2, 9)).unwrap();

    assert!(state.current_observation().is_none());
    assert!(matches!(state.head(), ObservationHeadViewV1::Unknown(_)));
    assert_eq!(old_revision, Revision::new(1));
    assert_eq!(old_schema, vec![PORT_A]);
    assert_eq!(
        old_values,
        vec![ColorSignal::from_srgb8(Srgb8::new([3, 4, 5]))]
    );
}

#[test]
fn same_revision_exact_payload_is_idempotent_and_conflict_is_rejected() {
    let set = scenarios([scenario(1, [binding(PORT_A, [255; 3])])]);
    let mut state = TestState::new(STREAM, vec![PORT_A]).unwrap();
    assert_eq!(
        state.apply(observed_update(STREAM, 1, set.clone())),
        Ok(UpdateDisposition::Applied)
    );
    let before = state.clone();
    assert_eq!(
        state.apply(observed_update(STREAM, 1, set)),
        Ok(UpdateDisposition::Idempotent)
    );
    assert_eq!(state, before);
    assert_eq!(
        state.apply(unknown_update(STREAM, 1, 1)),
        Err(ObservationError::RevisionConflict {
            revision: Revision::new(1),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn same_revision_permuted_replay_is_idempotent_without_replacing_backing() {
    let first = scenarios([
        scenario(9, [binding(PORT_B, [4, 5, 6]), binding(PORT_A, [1, 2, 3])]),
        scenario(4, [binding(PORT_A, [9, 8, 7]), binding(PORT_B, [6, 5, 4])]),
        scenario(3, [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])]),
    ]);
    let replay = scenarios([
        scenario(3, [binding(PORT_B, [4, 5, 6]), binding(PORT_A, [1, 2, 3])]),
        scenario(9, [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])]),
        scenario(4, [binding(PORT_B, [6, 5, 4]), binding(PORT_A, [9, 8, 7])]),
    ]);
    let mut state = TestState::new(STREAM, vec![PORT_B, PORT_A]).unwrap();
    assert_eq!(
        state.apply(observed_update(STREAM, 7, first)),
        Ok(UpdateDisposition::Applied)
    );
    let before = state.clone();
    let backing_ptr = revision_bound(&state).backing_ptr_for_test();
    let schema_ptr = revision_bound(&state).schema_ptr_for_test();

    assert_eq!(
        state.apply(observed_update(STREAM, 7, replay)),
        Ok(UpdateDisposition::Idempotent)
    );
    assert_eq!(state, before);
    assert_eq!(revision_bound(&state).backing_ptr_for_test(), backing_ptr);
    assert_eq!(revision_bound(&state).schema_ptr_for_test(), schema_ptr);
}

#[test]
fn lower_revision_and_foreign_stream_are_atomic_rejections() {
    let mut state = TestState::new(STREAM, vec![PORT_A]).unwrap();
    state.apply(unknown_update(STREAM, 5, 1)).unwrap();
    let before = state.clone();
    assert_eq!(
        state.apply(unknown_update(STREAM, 4, 1)),
        Err(ObservationError::RevisionOutOfOrder {
            current: Revision::new(5),
            incoming: Revision::new(4),
        })
    );
    assert_eq!(state, before);
    assert_eq!(
        state.apply(unknown_update(ObservationStreamId::new(99), 6, 1)),
        Err(ObservationError::StreamMismatch {
            expected: STREAM,
            actual: ObservationStreamId::new(99),
        })
    );
    assert_eq!(state, before);
}

#[test]
fn prepare_does_not_mutate_and_binding_is_the_only_owner_transition() {
    let mut state = TestState::new(STREAM, vec![PORT_A]).unwrap();
    let prepared = state
        .prepare(observed_update(
            STREAM,
            2,
            scenarios([scenario(1, [binding(PORT_A, [7, 8, 9])])]),
        ))
        .unwrap();
    let PreparedObservationUpdateV1::Observed(prepared) = prepared else {
        panic!("expected prepared observation");
    };
    assert_eq!(prepared.observation().revision(), Revision::new(2));
    let (owner, observation) = prepared.into_parts();
    assert_eq!(owner.observation_head(), ObservationHeadViewV1::Empty);
    *owner = TestOwner::Observed(observation);
    assert_eq!(revision_bound(&state).revision(), Revision::new(2));
}

#[test]
fn changing_tuple_correlation_changes_identity_even_with_same_marginals() {
    let mut first = TestState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    let mut second = TestState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    first
        .apply(observed_update(
            STREAM,
            1,
            paired_set(([0; 3], [0; 3]), ([255; 3], [255; 3])),
        ))
        .unwrap();
    second
        .apply(observed_update(
            STREAM,
            1,
            paired_set(([0; 3], [255; 3]), ([255; 3], [0; 3])),
        ))
        .unwrap();
    assert_ne!(revision_bound(&first), revision_bound(&second));
}

#[test]
fn two_streams_have_independent_watermarks_and_same_physics() {
    let other = ObservationStreamId::new(8);
    let set = scenarios([scenario(1, [binding(PORT_A, [1, 2, 3])])]);
    let mut left = TestState::new(STREAM, vec![PORT_A]).unwrap();
    let mut right = TestState::new(other, vec![PORT_A]).unwrap();
    left.apply(observed_update(STREAM, 5, set.clone())).unwrap();
    right.apply(observed_update(other, 1, set)).unwrap();

    assert_eq!(
        revision_bound(&left).schema(),
        revision_bound(&right).schema()
    );
    assert_eq!(
        revision_bound(&left).physical_values(0),
        revision_bound(&right).physical_values(0)
    );
    assert_eq!(
        revision_bound(&left).provenance(0),
        revision_bound(&right).provenance(0)
    );
    assert_ne!(
        revision_bound(&left).stream(),
        revision_bound(&right).stream()
    );
    assert_ne!(
        revision_bound(&left).revision(),
        revision_bound(&right).revision()
    );
}

#[test]
fn empty_observed_scenario_set_is_rejected_without_moving_head() {
    let mut state = TestState::new(STREAM, vec![PORT_A]).unwrap();
    state.apply(unknown_update(STREAM, 1, 7)).unwrap();
    let before = state.clone();

    assert_eq!(
        state.apply(observed_update(
            STREAM,
            2,
            scenarios(Vec::<ScenarioInput>::new()),
        )),
        Err(ObservationError::EmptyScenarioSet)
    );
    assert_eq!(state, before);
}

#[test]
fn invalid_schema_and_bindings_are_typed_and_deterministic() {
    assert_eq!(
        TestState::new(STREAM, vec![]),
        Err(ObservationError::EmptyCompiledSurfaceInputSchema)
    );
    assert_eq!(
        TestState::new(STREAM, vec![PORT_A, PORT_A]),
        Err(ObservationError::DuplicateCompiledSurfaceInputPort { input: PORT_A })
    );

    let invalid_a = scenarios([
        scenario(2, [binding(PORT_A, [1; 3])]),
        scenario(1, [binding(PORT_A, [2; 3]), binding(PORT_A, [3; 3])]),
    ]);
    let invalid_b = scenarios([
        scenario(1, [binding(PORT_A, [3; 3]), binding(PORT_A, [2; 3])]),
        scenario(2, [binding(PORT_A, [1; 3])]),
    ]);
    for invalid in [invalid_a, invalid_b] {
        let mut state = TestState::new(STREAM, vec![PORT_B, PORT_A]).unwrap();
        assert_eq!(
            state.apply(observed_update(STREAM, 1, invalid)),
            Err(ObservationError::DuplicateSurfaceInputBinding {
                scenario: ScenarioId::new(1),
                input: PORT_A,
            })
        );
        assert_eq!(state.head(), ObservationHeadViewV1::Empty);
    }
}

#[test]
fn schema_ordered_duplicate_error_is_the_minimum_id_before_shape_and_revision() {
    let mut scenarios = vec![
        ordered_scenario(7, [[7; 3]]),
        ordered_scenario(3, [[3; 3]]),
        ordered_scenario(1, []),
        ordered_scenario(7, [[70; 3]]),
        ordered_scenario(3, [[30; 3]]),
    ];
    let mut state = TestState::new(STREAM, vec![PORT_A]).unwrap();
    state.apply(unknown_update(STREAM, 10, 1)).unwrap();

    let mut checked = 0;
    visit_permutations(&mut scenarios, 0, &mut |permutation| {
        let source = OrderedSource::new(permutation.to_vec());
        let mut order_scratch = Vec::new();
        let error = prepare_ordered(&mut state, 1, &source, &mut order_scratch)
            .err()
            .expect("duplicate IDs must be rejected");
        assert_eq!(
            error,
            ObservationError::DuplicateScenarioId {
                scenario: ScenarioId::new(3),
            }
        );
        checked += 1;
    });
    assert_eq!(checked, 120);
}

#[test]
fn schema_ordered_shape_error_is_the_minimum_id_before_revision() {
    let mut scenarios = vec![
        ordered_scenario(9, [[9; 3]]),
        ordered_scenario(5, []),
        ordered_scenario(2, [[2; 3], [20; 3]]),
    ];
    let mut state = TestState::new(STREAM, vec![PORT_A]).unwrap();
    state.apply(unknown_update(STREAM, 10, 1)).unwrap();

    visit_permutations(&mut scenarios, 0, &mut |permutation| {
        let source = OrderedSource::new(permutation.to_vec());
        let mut order_scratch = Vec::new();
        let error = prepare_ordered(&mut state, 1, &source, &mut order_scratch)
            .err()
            .expect("malformed scenario width must be rejected");
        assert_eq!(
            error,
            ObservationError::SchemaOrderedValueCountMismatch {
                scenario: ScenarioId::new(2),
                expected: 1,
                actual: 2,
            }
        );
    });
}

#[test]
fn schema_ordered_scenario_id_reads_stay_inside_a_sorting_envelope() {
    const SCENARIO_COUNT: usize = 1 << 10;
    const COLLIDING_ID_STRIDE: u32 = 1 << 11;
    const SORT_COUNT: usize = 2;
    const IDS_PER_COMPARISON: usize = 2;
    // Это mutation-budget, не коэффициент production API: запас остаётся
    // существенно ниже квадратичного collision-корпуса и меняется только по
    // измеренному дрейфу стратегии стандартной сортировки на поддерживаемом Rust.
    const COMPARISON_ENVELOPE_PER_LEVEL: usize = 8;

    let source = OrderedSource::new(
        (0..SCENARIO_COUNT)
            .rev()
            .map(|index| ordered_scenario(index as u32 * COLLIDING_ID_STRIDE, [[0x80; 3]]))
            .collect(),
    );
    let mut state = TestState::new(STREAM, vec![PORT_A]).unwrap();
    let mut order_scratch = Vec::new();
    let prepared = prepare_ordered(&mut state, 1, &source, &mut order_scratch)
        .expect("unique opaque IDs must be admitted");
    drop(prepared);

    let logarithmic_levels = usize::BITS as usize - (SCENARIO_COUNT - 1).leading_zeros() as usize;
    let read_budget = SORT_COUNT
        * IDS_PER_COMPARISON
        * COMPARISON_ENVELOPE_PER_LEVEL
        * SCENARIO_COUNT
        * logarithmic_levels;
    assert!(
        source.scenario_id_reads() <= read_budget,
        "{} ScenarioId reads exceeded the O(n log n) admission envelope of {read_budget}",
        source.scenario_id_reads(),
    );
}

fn schema_ordered_value_read_count(scenario_count: usize, binding_count: usize) -> usize {
    assert!(scenario_count > 0);
    assert!(binding_count > 0);
    let source = OrderedSource::new(
        (0..scenario_count)
            .rev()
            .map(|index| {
                let id = ((index * 5) % scenario_count) as u32;
                let key = Srgb8::new([(index >> 16) as u8, (index >> 8) as u8, index as u8]);
                let mut values = vec![Srgb8::new([0; 3]); binding_count];
                values[binding_count - 1] = key;
                OrderedScenario {
                    id: ScenarioId::new(id),
                    values,
                }
            })
            .collect(),
    );
    let schema = (0..binding_count)
        .map(|index| SurfaceInputPortId::new(index as u32 + 1))
        .collect();
    let mut state = TestState::new(STREAM, schema).unwrap();
    let mut order_scratch = Vec::new();
    let prepared = prepare_ordered(&mut state, 1, &source, &mut order_scratch)
        .expect("the bounded-work corpus must be valid");
    drop(prepared);
    source.value_reads()
}

#[test]
fn schema_ordered_value_reads_scale_linearly_with_width_and_n_log_n_with_count() {
    const SMALL_COUNT: usize = 1 << 6;
    const LARGE_COUNT: usize = 1 << 9;
    const WIDE_SCHEMA: usize = 1 << 3;
    // При восьмикратном росте n квадратичный mutant растёт в 64 раза, а n log n
    // здесь — в 12; двукратный запас допускает toolchain-дрейф, но не n².
    const N_LOG_N_HEADROOM: usize = 2;

    let small_narrow = schema_ordered_value_read_count(SMALL_COUNT, 1);
    let small_wide = schema_ordered_value_read_count(SMALL_COUNT, WIDE_SCHEMA);
    let large_narrow = schema_ordered_value_read_count(LARGE_COUNT, 1);
    let large_wide = schema_ordered_value_read_count(LARGE_COUNT, WIDE_SCHEMA);

    assert_eq!(small_wide, small_narrow * WIDE_SCHEMA);
    assert_eq!(large_wide, large_narrow * WIDE_SCHEMA);

    let small_levels = SMALL_COUNT.ilog2() as usize;
    let large_levels = LARGE_COUNT.ilog2() as usize;
    assert!(
        large_narrow * SMALL_COUNT * small_levels
            <= small_narrow * LARGE_COUNT * large_levels * N_LOG_N_HEADROOM,
        "value reads grew faster than the n log n envelope: {small_narrow} -> {large_narrow}",
    );
}

#[test]
fn schema_ordered_scratch_exhaustion_precedes_every_source_access() {
    let mut state = TestState::new(STREAM, vec![PORT_A]).unwrap();
    let mut order_scratch = Vec::new();

    assert!(matches!(
        prepare_schema_ordered_observation(
            &mut state.owner,
            &mut state.arenas,
            state.stream,
            &state.schema,
            Revision::new(1),
            &UnrepresentableOrderedSource,
            &mut order_scratch,
        ),
        Err(ObservationError::ResourceExhausted)
    ));
    assert_eq!(state.head(), ObservationHeadViewV1::Empty);
}

fn schema_ordered_admission_matches_oracle(
    raw: Vec<(u32, [u8; 6])>,
    shift: usize,
    reverse: bool,
) -> Result<(), TestCaseError> {
    let mut by_id = BTreeMap::new();
    for (id, bytes) in raw {
        let first_class = bytes[0] & 0b11;
        let second_class = bytes[1] & 0b1;
        by_id.insert(
            ScenarioId::new(id),
            [Srgb8::new([first_class; 3]), Srgb8::new([second_class; 3])],
        );
    }
    let mut scenarios = by_id
        .into_iter()
        .map(|(id, values)| OrderedScenario {
            id,
            values: values.to_vec(),
        })
        .collect::<Vec<_>>();
    let scenario_count = scenarios.len();
    scenarios.rotate_left(shift % scenario_count);
    if reverse {
        scenarios.reverse();
    }

    let mut oracle = BTreeMap::<Vec<Srgb8>, BTreeSet<ScenarioId>>::new();
    for scenario in &scenarios {
        oracle
            .entry(scenario.values.clone())
            .or_default()
            .insert(scenario.id);
    }
    let expected = oracle
        .into_iter()
        .map(|(values, provenance)| (values, provenance.into_iter().collect::<Vec<_>>()))
        .collect::<Vec<_>>();

    let source = OrderedSource::new(scenarios);
    let mut state = TestState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    let mut order_scratch = Vec::new();
    let prepared = prepare_ordered(&mut state, 1, &source, &mut order_scratch)
        .map_err(|error| TestCaseError::fail(format!("valid oracle input failed: {error:?}")))?;
    let PreparedObservationUpdateV1::Observed(prepared) = prepared else {
        return Err(TestCaseError::fail(
            "a fresh valid observation was not materialized",
        ));
    };
    let observation = prepared.observation();
    let actual = (0..observation.physical_case_count())
        .map(|case_index| {
            let values = observation
                .physical_values(case_index)
                .expect("case index originates from the admitted observation")
                .iter()
                .map(|value| value.srgb8())
                .collect::<Vec<_>>();
            let provenance = observation
                .provenance(case_index)
                .expect("case index originates from the admitted observation")
                .to_vec();
            (values, provenance)
        })
        .collect::<Vec<_>>();

    prop_assert_eq!(actual, expected);
    Ok(())
}

#[test]
fn schema_ordered_admission_matches_an_independent_ordered_map_oracle() {
    let config = Config {
        cases: 256,
        failure_persistence: None,
        ..Config::default()
    };
    let mut runner =
        TestRunner::new_with_rng(config, TestRng::deterministic_rng(RngAlgorithm::ChaCha));
    runner
        .run(
            &(
                prop::collection::vec((any::<u32>(), any::<[u8; 6]>()), 1..40),
                any::<usize>(),
                any::<bool>(),
            ),
            |(raw, shift, reverse)| schema_ordered_admission_matches_oracle(raw, shift, reverse),
        )
        .expect("deterministic schema-ordered differential property failed");
}
