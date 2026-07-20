use crate::Srgb8;
use crate::appearance::{
    AppearanceBindings, AppearanceGraphSpec, BindingError, ColorInputId, CompositionProfileV1,
    OccurrenceId, OccurrenceSpec, PaintId, PaintSpec, PointOpacityError, PointOpacityOverSurfaceV1,
    ResolvedOccurrence, SurfaceId, SurfaceInputPortId, SurfaceSpec,
};
use crate::observation::{
    Availability, ObservationError, ObservationHead, ObservationPayloadInput, ObservationSnapshot,
    ObservationState, ObservationStreamId, ObservationUpdateInput, ObservedScenarioSet,
    ObservedScenarioSetInput, Revision, RevisionBoundObservationV1, ScenarioId, ScenarioInput,
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
    match state.snapshot() {
        ObservationSnapshot::Ready { observation } => observation,
        snapshot => panic!("expected Ready snapshot, got {snapshot:?}"),
    }
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
    assert_eq!(
        graph.evaluate(&AppearanceBindings::new(
            vec![(color, Srgb8::new([1, 2, 3]))],
            vec![
                (surface_port, Srgb8::new([240, 241, 242])),
                (SurfaceInputPortId::new(8), Srgb8::new([0, 0, 0])),
            ],
            vec![],
        )),
        Err(BindingError::UnexpectedSurfaceInputBinding {
            input: SurfaceInputPortId::new(8),
        })
    );
    assert_eq!(
        graph.evaluate(&AppearanceBindings::new(
            vec![(color, Srgb8::new([1, 2, 3]))],
            vec![
                (surface_port, Srgb8::new([240, 241, 242])),
                (surface_port, Srgb8::new([0, 0, 0])),
            ],
            vec![],
        )),
        Err(BindingError::DuplicateSurfaceInputBinding {
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
    assert_eq!(cases.len(), 2);
    assert_eq!(
        cases,
        vec![vec![[1, 2, 3], [4, 5, 6]], vec![[7, 8, 9], [10, 11, 12]],]
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
        set.cases()[0]
            .bindings()
            .iter()
            .copied()
            .map(Srgb8::bytes)
            .collect::<Vec<_>>(),
        vec![[1, 2, 3], [4, 5, 6]]
    );
}

#[test]
fn invalid_permutations_return_the_same_deterministic_error() {
    let invalid_a = scenarios([
        scenario(2, [binding(PORT_A, [1, 1, 1])]),
        scenario(1, [binding(PORT_A, [2, 2, 2]), binding(PORT_A, [3, 3, 3])]),
    ]);
    let invalid_b = scenarios([
        scenario(1, [binding(PORT_A, [3, 3, 3]), binding(PORT_A, [2, 2, 2])]),
        scenario(2, [binding(PORT_A, [1, 1, 1])]),
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

#[test]
fn initial_unknown_advances_watermark_without_inventing_a_surface() {
    let mut state = ObservationState::new(STREAM, vec![PORT_A]).unwrap();
    assert_eq!(state.availability(), Availability::Waiting);
    assert_eq!(
        state.apply(unknown_update(STREAM, 3, 11)),
        Ok(UpdateDisposition::Applied)
    );
    assert_eq!(state.availability(), Availability::Waiting);
    assert!(matches!(
        state.head(),
        ObservationHead::Unknown {
            revision,
            reason,
            previous: None,
        } if *revision == Revision::new(3) && *reason == UnknownReasonId::new(11)
    ));

    let explicit_white = scenarios([scenario(1, [binding(PORT_A, [255; 3])])]);
    assert_eq!(
        state.apply(observed_update(STREAM, 2, explicit_white.clone())),
        Err(ObservationError::RevisionOutOfOrder {
            current: Revision::new(3),
            incoming: Revision::new(2),
        })
    );
    assert_eq!(
        state.apply(unknown_update(STREAM, 3, 11)),
        Ok(UpdateDisposition::Idempotent)
    );
    assert_eq!(
        state.apply(unknown_update(STREAM, 3, 12)),
        Err(ObservationError::RevisionConflict {
            revision: Revision::new(3),
        })
    );
    assert_eq!(
        state.apply(observed_update(STREAM, 4, explicit_white)),
        Ok(UpdateDisposition::Applied)
    );
    assert_eq!(state.availability(), Availability::Ready);
    assert_eq!(
        observed_set(&state).cases()[0].bindings(),
        &[Srgb8::new([255; 3])]
    );
}

#[test]
fn unknown_chain_preserves_prior_evidence_without_ready_fallback() {
    let ready = scenarios([scenario(1, [binding(PORT_A, [3, 4, 5])])]);
    let restored = scenarios([scenario(2, [binding(PORT_A, [6, 7, 8])])]);
    let mut state = ObservationState::new(STREAM, vec![PORT_A]).unwrap();
    state
        .apply(observed_update(STREAM, 2, ready.clone()))
        .unwrap();
    let prior_set = observed_set(&state).clone();
    state.apply(unknown_update(STREAM, 3, 1)).unwrap();
    assert_eq!(state.availability(), Availability::Stale);
    let ObservationHead::Unknown {
        previous: Some(previous),
        ..
    } = state.head()
    else {
        panic!("Ready must become stale evidence");
    };
    assert_eq!(previous.revision(), Revision::new(2));
    assert_eq!(previous.set(), &prior_set);
    assert!(state.current_set().is_none());

    assert!(matches!(
        state.apply(observed_update(STREAM, 2, ready)),
        Err(ObservationError::RevisionOutOfOrder { .. })
    ));
    state.apply(unknown_update(STREAM, 4, 2)).unwrap();
    let ObservationHead::Unknown {
        previous: Some(previous),
        ..
    } = state.head()
    else {
        panic!("Unknown chain lost prior evidence");
    };
    assert_eq!(previous.revision(), Revision::new(2));
    assert_eq!(previous.set(), &prior_set);

    state.apply(observed_update(STREAM, 5, restored)).unwrap();
    assert_eq!(state.availability(), Availability::Ready);
}

#[test]
fn malformed_update_does_not_consume_its_revision() {
    let malformed = scenarios([scenario(1, [binding(PORT_A, [1, 2, 3])])]);
    let corrected = scenarios([scenario(
        1,
        [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])],
    )]);
    let mut state = ObservationState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    assert_eq!(
        state.apply(observed_update(STREAM, 9, malformed)),
        Err(ObservationError::MissingSurfaceInputBinding {
            scenario: ScenarioId::new(1),
            input: PORT_B,
        })
    );
    assert_eq!(state.head(), &ObservationHead::Empty);
    assert_eq!(
        state.apply(observed_update(STREAM, 9, corrected)),
        Ok(UpdateDisposition::Applied)
    );
}

#[test]
fn raw_validation_rejects_empty_duplicate_and_unexpected_members() {
    let cases = [
        (scenarios([]), ObservationError::EmptyScenarioSet),
        (
            scenarios([
                scenario(1, [binding(PORT_A, [1, 2, 3])]),
                scenario(1, [binding(PORT_A, [4, 5, 6])]),
            ]),
            ObservationError::DuplicateScenarioId {
                scenario: ScenarioId::new(1),
            },
        ),
        (
            scenarios([scenario(
                1,
                [
                    binding(PORT_A, [1, 2, 3]),
                    binding(SurfaceInputPortId::new(99), [4, 5, 6]),
                ],
            )]),
            ObservationError::UnexpectedSurfaceInputBinding {
                scenario: ScenarioId::new(1),
                input: SurfaceInputPortId::new(99),
            },
        ),
    ];

    for (raw, expected) in cases {
        let mut state = ObservationState::new(STREAM, vec![PORT_A]).unwrap();
        assert_eq!(state.apply(observed_update(STREAM, 1, raw)), Err(expected));
        assert_eq!(state.head(), &ObservationHead::Empty);
    }
}

#[test]
fn same_revision_uses_full_canonical_identity() {
    let first = scenarios([scenario(
        9,
        [binding(PORT_B, [4, 5, 6]), binding(PORT_A, [1, 2, 3])],
    )]);
    let reordered = scenarios([scenario(
        9,
        [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])],
    )]);
    let renamed = scenarios([scenario(
        10,
        [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])],
    )]);
    let mut state = ObservationState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    state.apply(observed_update(STREAM, 1, first)).unwrap();
    assert_eq!(
        state.apply(observed_update(STREAM, 1, reordered)),
        Ok(UpdateDisposition::Idempotent)
    );
    let retained = state.clone();
    assert_eq!(
        state.apply(observed_update(STREAM, 1, renamed)),
        Err(ObservationError::RevisionConflict {
            revision: Revision::new(1),
        })
    );
    assert_eq!(state, retained);
}

#[test]
fn tuple_correlation_is_identity_even_when_marginals_match() {
    let correlated = paired_set(([1, 0, 0], [10, 0, 0]), ([2, 0, 0], [20, 0, 0]));
    let crossed = paired_set(([1, 0, 0], [20, 0, 0]), ([2, 0, 0], [10, 0, 0]));
    let mut left = ObservationState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    let mut right = ObservationState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    left.apply(observed_update(STREAM, 1, correlated)).unwrap();
    right.apply(observed_update(STREAM, 1, crossed)).unwrap();

    assert_ne!(observed_set(&left), observed_set(&right));
}

#[test]
fn ids_only_route_and_provenance_while_physical_evidence_stays_invariant() {
    let renamed_a = SurfaceInputPortId::new(110);
    let renamed_b = SurfaceInputPortId::new(120);
    let original = scenarios([scenario(
        1,
        [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])],
    )]);
    let renamed = scenarios([ScenarioInput {
        id: ScenarioId::new(99),
        bindings: vec![binding(renamed_b, [4, 5, 6]), binding(renamed_a, [1, 2, 3])],
    }]);
    let mut left = ObservationState::new(STREAM, vec![PORT_A, PORT_B]).unwrap();
    let mut right =
        ObservationState::new(ObservationStreamId::new(70), vec![renamed_b, renamed_a]).unwrap();
    left.apply(observed_update(STREAM, 1, original)).unwrap();
    right
        .apply(observed_update(ObservationStreamId::new(70), 1, renamed))
        .unwrap();

    assert_eq!(
        observed_set(&left).physical_bindings(),
        observed_set(&right).physical_bindings()
    );
    assert_ne!(observed_set(&left), observed_set(&right));
}

#[test]
fn stream_and_compiled_schema_are_immutable_and_watermarks_are_independent() {
    let mut first = ObservationState::new(STREAM, vec![PORT_B, PORT_A]).unwrap();
    let other_stream = ObservationStreamId::new(8);
    let mut second = ObservationState::new(other_stream, vec![PORT_A, PORT_B]).unwrap();
    let payload = paired_set(([1, 2, 3], [4, 5, 6]), ([7, 8, 9], [10, 11, 12]));

    assert_eq!(
        first.apply(observed_update(other_stream, 1, payload.clone())),
        Err(ObservationError::StreamMismatch {
            expected: STREAM,
            actual: other_stream,
        })
    );
    first
        .apply(observed_update(STREAM, 7, payload.clone()))
        .unwrap();
    second
        .apply(observed_update(other_stream, 1, payload))
        .unwrap();

    assert_eq!(first.stream(), STREAM);
    assert_eq!(second.stream(), other_stream);
    assert_eq!(first.compiled_surface_input_schema(), &[PORT_A, PORT_B]);
    assert_eq!(second.compiled_surface_input_schema(), &[PORT_A, PORT_B]);
    assert_eq!(
        observed_set(&first).physical_bindings(),
        observed_set(&second).physical_bindings()
    );
    assert!(matches!(
        first.head(),
        ObservationHead::Observed { revision, .. } if *revision == Revision::new(7)
    ));
    assert!(matches!(
        second.head(),
        ObservationHead::Observed { revision, .. } if *revision == Revision::new(1)
    ));
}

#[test]
fn admitted_state_owns_canonical_values_and_rejects_invalid_schemas() {
    assert_eq!(
        ObservationState::new(STREAM, vec![]),
        Err(ObservationError::EmptyCompiledSurfaceInputSchema)
    );
    assert_eq!(
        ObservationState::new(STREAM, vec![PORT_A, PORT_A]),
        Err(ObservationError::DuplicateCompiledSurfaceInputPort { input: PORT_A })
    );

    let mut caller = scenarios([scenario(1, [binding(PORT_A, [1, 2, 3])])]);
    let mut state = ObservationState::new(STREAM, vec![PORT_A]).unwrap();
    state
        .apply(observed_update(STREAM, 1, caller.clone()))
        .unwrap();
    caller.scenarios[0].bindings[0].value = Srgb8::new([9, 9, 9]);
    assert_eq!(
        observed_set(&state).cases()[0].bindings(),
        &[Srgb8::new([1, 2, 3])]
    );
}

#[test]
fn revision_bound_observation_clone_is_allocation_free_and_content_equal() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RevisionBoundObservationV1>();

    let state = ready_state_for_snapshot(
        STREAM,
        7,
        vec![PORT_B, PORT_A],
        vec![
            scenario(9, [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])]),
            scenario(3, [binding(PORT_B, [4, 5, 6]), binding(PORT_A, [1, 2, 3])]),
        ],
    );
    let (observation, snapshot_allocations) =
        crate::test_support::measured_allocations(|| revision_bound(&state));

    let (cloned, allocations) = crate::test_support::measured_allocations(|| observation.clone());

    assert_eq!(snapshot_allocations, 0);
    assert_eq!(allocations, 0);
    assert_eq!(cloned, observation);
    assert_eq!(cloned.stream(), STREAM);
    assert_eq!(cloned.revision(), Revision::new(7));
    assert_eq!(cloned.schema(), &[PORT_A, PORT_B]);
    assert_eq!(
        cloned.set().cases()[0].provenance(),
        &[ScenarioId::new(3), ScenarioId::new(9)]
    );
}

#[test]
fn revision_bound_observation_equality_covers_every_identity_component() {
    let make = |stream, revision, schema, scenarios| {
        revision_bound(&ready_state_for_snapshot(
            stream, revision, schema, scenarios,
        ))
    };
    let baseline_scenarios = || {
        vec![scenario(
            1,
            [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])],
        )]
    };
    let baseline = make(STREAM, 5, vec![PORT_A, PORT_B], baseline_scenarios());
    assert_eq!(
        baseline,
        make(STREAM, 5, vec![PORT_B, PORT_A], baseline_scenarios())
    );
    assert_ne!(
        baseline,
        make(
            ObservationStreamId::new(8),
            5,
            vec![PORT_A, PORT_B],
            baseline_scenarios()
        )
    );
    assert_ne!(
        baseline,
        make(STREAM, 6, vec![PORT_A, PORT_B], baseline_scenarios())
    );
    assert_ne!(
        baseline,
        make(
            STREAM,
            5,
            vec![PORT_A, SurfaceInputPortId::new(30)],
            vec![scenario(
                1,
                [
                    binding(PORT_A, [1, 2, 3]),
                    binding(SurfaceInputPortId::new(30), [4, 5, 6]),
                ],
            )],
        )
    );
    assert_ne!(
        baseline,
        make(
            STREAM,
            5,
            vec![PORT_A, PORT_B],
            vec![scenario(
                1,
                [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 7])],
            )],
        )
    );
    assert_ne!(
        baseline,
        make(
            STREAM,
            5,
            vec![PORT_A, PORT_B],
            vec![scenario(
                2,
                [binding(PORT_A, [1, 2, 3]), binding(PORT_B, [4, 5, 6])],
            )],
        )
    );
}

#[test]
fn revision_bound_observation_is_immutable_after_stream_advances() {
    let mut state = ready_state_for_snapshot(
        STREAM,
        1,
        vec![PORT_A],
        vec![scenario(1, [binding(PORT_A, [1, 2, 3])])],
    );
    let first = revision_bound(&state);
    state
        .apply(observed_update(
            STREAM,
            2,
            scenarios([scenario(2, [binding(PORT_A, [9, 8, 7])])]),
        ))
        .unwrap();
    let second = revision_bound(&state);

    assert_eq!(first.revision(), Revision::new(1));
    assert_eq!(first.set().cases()[0].bindings(), &[Srgb8::new([1, 2, 3])]);
    assert_eq!(second.revision(), Revision::new(2));
    assert_eq!(second.set().cases()[0].bindings(), &[Srgb8::new([9, 8, 7])]);
    assert_ne!(first, second);
}

fn ready_state_for_snapshot(
    stream: ObservationStreamId,
    revision: u64,
    schema: Vec<SurfaceInputPortId>,
    scenarios: Vec<ScenarioInput>,
) -> ObservationState {
    let mut state = ObservationState::new(stream, schema).unwrap();
    state
        .apply(observed_update(
            stream,
            revision,
            ObservedScenarioSetInput { scenarios },
        ))
        .unwrap();
    state
}
