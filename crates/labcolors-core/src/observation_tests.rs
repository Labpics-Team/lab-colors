use crate::Srgb8;
use crate::appearance::{
    AppearanceBindings, AppearanceGraphSpec, BindingError, ColorInputId, CompositionProfileV1,
    OccurrenceId, OccurrenceSpec, PaintId, PaintSpec, PointOpacityError, PointOpacityOverSurfaceV1,
    ResolvedOccurrence, SurfaceId, SurfaceInputPortId, SurfaceSpec,
};
use crate::lcs_occurrence::ColorSignal;
use crate::observation::{
    CanonicalObservationSchemaV1, ObservationError, ObservationHeadViewV1, ObservationOwnerV1,
    ObservationPayloadInput, ObservationStreamId, ObservationUpdateInput, ObservedScenarioSetInput,
    PreparedObservationUpdateV1, Revision, RevisionBoundObservationV1, RevisionBoundUnknownV1,
    ScenarioId, ScenarioInput, SurfaceInputBinding, UnknownReasonId,
    canonicalize_observation_schema, prepare_observation,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct TestState {
    stream: ObservationStreamId,
    schema: CanonicalObservationSchemaV1,
    owner: TestOwner,
}

impl TestState {
    fn new(
        stream: ObservationStreamId,
        schema: Vec<SurfaceInputPortId>,
    ) -> Result<Self, ObservationError> {
        Ok(Self {
            stream,
            schema: canonicalize_observation_schema(schema)?,
            owner: TestOwner::Empty,
        })
    }

    fn prepare(
        &mut self,
        update: ObservationUpdateInput,
    ) -> Result<PreparedObservationUpdateV1<'_, TestOwner>, ObservationError> {
        prepare_observation(&mut self.owner, self.stream, &self.schema, update)
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

#[test]
fn authored_color_and_observed_surface_ports_are_distinct_through_execution() {
    let _sealed_adapter_contract: fn(
        [u8; 3],
        f64,
        [u8; 3],
    ) -> Result<ResolvedOccurrence, PointOpacityError> = PointOpacityOverSurfaceV1::evaluate;
    let color = ColorInputId::new(7);
    let surface_port = SurfaceInputPortId::new(7);
    let paint = PaintId::new(1);
    let surface = SurfaceId::new(1);
    let occurrence = OccurrenceId::new(1);
    let graph = AppearanceGraphSpec::new(
        vec![color],
        vec![surface_port],
        vec![],
        vec![PaintSpec::Solid { id: paint, color }],
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
            vec![(color, Srgb8::new([1, 2, 3]))],
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
            vec![(color, Srgb8::new([1, 2, 3]))],
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
                .unwrap()
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
        .map(|case_index| observation.physical_values(case_index).unwrap().to_vec())
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
    let first_values: &[ColorSignal] = observation.physical_values(0).unwrap();
    let second_values: &[ColorSignal] = observation.physical_values(1).unwrap();
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
        revision_bound(&left).physical_values(0),
        Some(&[ColorSignal::from_srgb8(Srgb8::new([7, 8, 9]))][..])
    );
    assert_eq!(
        revision_bound(&right).physical_values(0),
        Some(&[ColorSignal::from_srgb8(Srgb8::new([7, 8, 9]))][..])
    );

    let alternate_schema = canonicalize_observation_schema(vec![PORT_B]).unwrap();
    let before = left.clone();
    assert!(matches!(
        prepare_observation(
            &mut left.owner,
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
    let old_values = revision_bound(&state).physical_values(0).unwrap().to_vec();
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
