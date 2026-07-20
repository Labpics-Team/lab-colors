use crate::Srgb8;
use crate::appearance::{
    AppearanceBindings, AppearanceGraphSpec, BindingError, ColorInputId, CompositionProfileV1,
    OccurrenceId, OccurrenceSpec, PaintId, PaintSpec, PointOpacityError, PointOpacityOverSurfaceV1,
    ResolvedOccurrence, SurfaceId, SurfaceInputPortId, SurfaceSpec,
};
use crate::observation::{
    ObservationError, ObservationHead, ObservationPayloadInput, ObservationState,
    ObservationStreamId, ObservationUpdateInput, ObservedScenarioSet, ObservedScenarioSetInput,
    PreparedObservationViewV1, Revision, RevisionBoundObservationV1, ScenarioId, ScenarioInput,
    SurfaceInputBinding, UnknownReasonId, UpdateDisposition,
};

const PORT_A: SurfaceInputPortId = SurfaceInputPortId::new(10);
const PORT_B: SurfaceInputPortId = SurfaceInputPortId::new(20);
const STREAM: ObservationStreamId = ObservationStreamId::new(7);

fn binding(port: SurfaceInputPortId, bytes: [u8; 3]) -> SurfaceInputBinding {
    SurfaceInputBinding {
        port,
        value: Srgb8::new(bytes),
    }
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

fn observed_set(state: &ObservationState) -> &ObservedScenarioSet {
    match state.head() {
        ObservationHead::Observed { set, .. } => set,
        head => panic!("expected Observed head, got {head:?}"),
    }
}

fn revision_bound(state: &ObservationState) -> RevisionBoundObservationV1 {
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
    let mut state = ObservationState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    state
        .apply(observed_update(
            STREAM,
            1,
            paired_set(([1, 2, 3], [4, 5, 6]), ([7, 8, 9], [10, 11, 12])),
        ))
        .unwrap();

    let cases: Vec<Vec<[u8; 3]>> = observed_set(&state)
        .cases()
        .iter()
        .map(|case| case.bindings().iter().copied().map(Srgb8::bytes).collect())
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
    let mut left = ObservationState::new(STREAM, vec![PORT_B, PORT_A]).unwrap();
    let mut right = ObservationState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    left.apply(observed_update(STREAM, 1, first)).unwrap();
    right.apply(observed_update(STREAM, 1, second)).unwrap();

    assert_eq!(left, right);
    let set = observed_set(&left);
    assert_eq!(set.cases().len(), 2);
    assert_eq!(
        set.cases()[0].provenance(),
        &[ScenarioId::new(3), ScenarioId::new(9)]
    );
    assert_eq!(
        set.physical_bindings(),
        vec![
            vec![Srgb8::new([1, 2, 3]), Srgb8::new([4, 5, 6])],
            vec![Srgb8::new([9, 8, 7]), Srgb8::new([6, 5, 4])],
        ]
    );
}

#[test]
fn malformed_payload_is_canonicalized_before_revision_and_never_moves_head() {
    let mut state = ObservationState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    state.apply(unknown_update(STREAM, 4, 1)).unwrap();
    let before = state.clone();
    let malformed = scenarios([scenario(1, [binding(PORT_A, [1; 3])])]);
    assert_eq!(
        state.apply(observed_update(STREAM, 2, malformed.clone())),
        Err(ObservationError::MissingSurfaceInputBinding {
            scenario: ScenarioId::new(1),
            input: PORT_B,
        })
    );
    assert_eq!(state, before);

    let corrected = scenarios([scenario(
        1,
        [binding(PORT_A, [1; 3]), binding(PORT_B, [2; 3])],
    )]);
    assert_eq!(
        state.apply(observed_update(STREAM, 5, corrected)),
        Ok(UpdateDisposition::Applied)
    );
    assert!(state.current_observation().is_some());
}

#[test]
fn initial_unknown_advances_only_raw_watermark_and_contains_no_previous() {
    let mut state = ObservationState::new(STREAM, vec![PORT_A]).unwrap();
    assert_eq!(state.head(), &ObservationHead::Empty);
    assert_eq!(
        state.apply(unknown_update(STREAM, 3, 11)),
        Ok(UpdateDisposition::Applied)
    );
    assert!(matches!(
        state.head(),
        ObservationHead::Unknown { revision, reason }
            if *revision == Revision::new(3) && *reason == UnknownReasonId::new(11)
    ));
    let unknown = state.current_unknown().unwrap();
    assert_eq!(unknown.stream(), STREAM);
    assert_eq!(unknown.revision(), Revision::new(3));
    assert_eq!(unknown.reason(), UnknownReasonId::new(11));
    assert!(state.current_observation().is_none());
}

#[test]
fn observed_to_unknown_forgets_raw_observation_instead_of_storing_prior() {
    let mut state = ObservationState::new(STREAM, vec![PORT_A]).unwrap();
    state
        .apply(observed_update(
            STREAM,
            1,
            scenarios([scenario(1, [binding(PORT_A, [3, 4, 5])])]),
        ))
        .unwrap();
    let old = revision_bound(&state);
    state.apply(unknown_update(STREAM, 2, 9)).unwrap();

    assert!(state.current_observation().is_none());
    assert!(matches!(state.head(), ObservationHead::Unknown { .. }));
    assert_eq!(old.revision(), Revision::new(1));
    assert_eq!(old.set().cases()[0].bindings(), &[Srgb8::new([3, 4, 5])]);
}

#[test]
fn same_revision_exact_payload_is_idempotent_and_conflict_is_rejected() {
    let set = scenarios([scenario(1, [binding(PORT_A, [255; 3])])]);
    let mut state = ObservationState::new(STREAM, vec![PORT_A]).unwrap();
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
fn lower_revision_and_foreign_stream_are_atomic_rejections() {
    let mut state = ObservationState::new(STREAM, vec![PORT_A]).unwrap();
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
fn prepare_does_not_mutate_and_commit_is_the_only_head_transition() {
    let mut state = ObservationState::new(STREAM, vec![PORT_A]).unwrap();
    let prepared = state
        .prepare(observed_update(
            STREAM,
            2,
            scenarios([scenario(1, [binding(PORT_A, [7, 8, 9])])]),
        ))
        .unwrap();
    assert_eq!(prepared.current_head(), &ObservationHead::Empty);
    let PreparedObservationViewV1::AppliedObserved(observation) = prepared.view() else {
        panic!("expected prepared Observed");
    };
    assert_eq!(observation.revision(), Revision::new(2));
    assert_eq!(prepared.commit(), UpdateDisposition::Applied);
    assert_eq!(revision_bound(&state).revision(), Revision::new(2));
}

#[test]
fn changing_tuple_correlation_changes_identity_even_with_same_marginals() {
    let mut first = ObservationState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    let mut second = ObservationState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
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
    let mut left = ObservationState::new(STREAM, vec![PORT_A]).unwrap();
    let mut right = ObservationState::new(other, vec![PORT_A]).unwrap();
    left.apply(observed_update(STREAM, 5, set.clone())).unwrap();
    right.apply(observed_update(other, 1, set)).unwrap();

    assert_eq!(revision_bound(&left).set(), revision_bound(&right).set());
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
fn invalid_schema_and_bindings_are_typed_and_deterministic() {
    assert_eq!(
        ObservationState::new(STREAM, vec![]),
        Err(ObservationError::EmptyCompiledSurfaceInputSchema)
    );
    assert_eq!(
        ObservationState::new(STREAM, vec![PORT_A, PORT_A]),
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
        let mut state = ObservationState::new(STREAM, vec![PORT_B, PORT_A]).unwrap();
        assert_eq!(
            state.apply(observed_update(STREAM, 1, invalid)),
            Err(ObservationError::DuplicateSurfaceInputBinding {
                scenario: ScenarioId::new(1),
                input: PORT_A,
            })
        );
        assert_eq!(state.head(), &ObservationHead::Empty);
    }
}
