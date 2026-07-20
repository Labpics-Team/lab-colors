use proptest::prelude::*;

use crate::Srgb8;
use crate::appearance::{EncodedPointPaintV1, OccurrenceId, PaintId, SurfaceInputPortId};
use crate::observation::{
    ObservationError, ObservationHead, ObservationPayloadInput, ObservationStreamId,
    ObservationUpdateInput, ObservedScenarioSetInput, Revision, ScenarioId, ScenarioInput,
    SurfaceInputBinding, UnknownReasonId,
};
use crate::recheck::{
    CompiledFixedRecheckV1, ExactOccurrenceRequirementV1, FixedRecheckBindErrorV1,
};
use crate::session::{
    ExactFixedSessionStateV1, FixedCandidateSessionV1, FixedSessionBuildErrorV1,
    FixedSessionUpdateErrorV1,
};

const PAINT: PaintId = PaintId::new(7);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(11);
const SURFACE: SurfaceInputPortId = SurfaceInputPortId::new(21);
const STREAM: ObservationStreamId = ObservationStreamId::new(31);
const TARGET: [u8; 3] = [128; 3];

fn candidate(id: PaintId) -> EncodedPointPaintV1 {
    EncodedPointPaintV1::from_admitted(
        id,
        Srgb8::new([0; 3]),
        crate::composition::AdmittedOpacityV1::new(0.5).unwrap(),
    )
}

fn requirement() -> CompiledFixedRecheckV1 {
    CompiledFixedRecheckV1::new(
        PAINT,
        vec![ExactOccurrenceRequirementV1::new(
            OCCURRENCE,
            SURFACE,
            Srgb8::new(TARGET),
        )],
    )
    .unwrap()
}

fn session() -> FixedCandidateSessionV1 {
    FixedCandidateSessionV1::new(STREAM, vec![SURFACE], requirement(), candidate(PAINT)).unwrap()
}

fn observed_update(revision: u64, backdrop: [u8; 3]) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: vec![ScenarioInput {
                id: ScenarioId::new(1),
                bindings: vec![SurfaceInputBinding {
                    port: SURFACE,
                    value: Srgb8::new(backdrop),
                }],
            }],
        }),
    }
}

fn unknown_update(revision: u64, reason: u32) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Unknown(UnknownReasonId::new(reason)),
    }
}

fn malformed_update(revision: u64) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: vec![ScenarioInput {
                id: ScenarioId::new(1),
                bindings: vec![],
            }],
        }),
    }
}

fn verified_revision(state: &ExactFixedSessionStateV1) -> Option<Revision> {
    state
        .last_verified()
        .map(|verified| verified.observation().revision())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateKind {
    Waiting,
    Ready,
    Stale,
    Failed,
}

fn state_kind(state: &ExactFixedSessionStateV1) -> StateKind {
    match state {
        ExactFixedSessionStateV1::Waiting => StateKind::Waiting,
        ExactFixedSessionStateV1::Ready { .. } => StateKind::Ready,
        ExactFixedSessionStateV1::Stale { .. } => StateKind::Stale,
        ExactFixedSessionStateV1::Failed { .. } => StateKind::Failed,
    }
}

#[test]
fn construction_prebinds_schema_candidate_and_requirement() {
    let session = session();
    assert_eq!(session.paint(), candidate(PAINT));
    assert_eq!(session.state(), &ExactFixedSessionStateV1::Waiting);
    assert_eq!(session.raw_head(), &ObservationHead::Empty);

    assert_eq!(
        FixedCandidateSessionV1::new(
            STREAM,
            vec![SURFACE],
            requirement(),
            candidate(PaintId::new(99)),
        ),
        Err(FixedSessionBuildErrorV1::Recheck(
            FixedRecheckBindErrorV1::PaintMismatch {
                expected: PAINT,
                actual: PaintId::new(99),
            }
        ))
    );
    assert_eq!(
        FixedCandidateSessionV1::new(
            STREAM,
            vec![SurfaceInputPortId::new(99)],
            requirement(),
            candidate(PAINT),
        ),
        Err(FixedSessionBuildErrorV1::Recheck(
            FixedRecheckBindErrorV1::MissingSurfacePort(SURFACE)
        ))
    );
    assert_eq!(
        FixedCandidateSessionV1::new(STREAM, vec![], requirement(), candidate(PAINT)),
        Err(FixedSessionBuildErrorV1::Observation(
            ObservationError::EmptyCompiledSurfaceInputSchema
        ))
    );
}

#[test]
fn ready_violation_unknown_preserves_exactly_one_verified_witness() {
    let mut session = session();
    let ExactFixedSessionStateV1::Ready { current } =
        session.update(observed_update(1, [255; 3])).unwrap()
    else {
        panic!("white backdrop must verify #808080 target");
    };
    assert_eq!(current.observation().revision(), Revision::new(1));

    let ExactFixedSessionStateV1::Failed { cause, previous } =
        session.update(observed_update(2, [0; 3])).unwrap()
    else {
        panic!("black backdrop must violate #808080 target");
    };
    assert_eq!(cause.observation().revision(), Revision::new(2));
    assert_eq!(
        previous.as_ref().unwrap().observation().revision(),
        Revision::new(1)
    );

    let ExactFixedSessionStateV1::Stale {
        previous,
        current_unknown,
    } = session.update(unknown_update(3, 9)).unwrap()
    else {
        panic!("unknown after prior verified result must become Stale");
    };
    assert_eq!(previous.observation().revision(), Revision::new(1));
    assert_eq!(current_unknown.stream(), STREAM);
    assert_eq!(current_unknown.revision(), Revision::new(3));
    assert_eq!(current_unknown.reason(), UnknownReasonId::new(9));
    assert!(matches!(
        session.raw_head(),
        ObservationHead::Unknown { .. }
    ));
}

#[test]
fn violation_without_prior_then_unknown_is_waiting() {
    let mut session = session();
    assert!(matches!(
        session.update(observed_update(1, [0; 3])).unwrap(),
        ExactFixedSessionStateV1::Failed { previous: None, .. }
    ));
    assert!(matches!(
        session.update(unknown_update(2, 1)).unwrap(),
        ExactFixedSessionStateV1::Waiting
    ));
    assert_eq!(verified_revision(session.state()), None);
}

#[test]
fn stale_and_failed_transitions_move_the_same_previous_without_history_chain() {
    let mut session = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    session.update(unknown_update(2, 1)).unwrap();
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));

    session.update(observed_update(3, [0; 3])).unwrap();
    let ExactFixedSessionStateV1::Failed { previous, .. } = session.state() else {
        panic!("Stale -> violation must be Failed");
    };
    assert_eq!(
        previous.as_ref().unwrap().observation().revision(),
        Revision::new(1)
    );

    session.update(unknown_update(4, 2)).unwrap();
    let ExactFixedSessionStateV1::Stale { previous, .. } = session.state() else {
        panic!("Failed(previous) -> Unknown must be Stale");
    };
    assert_eq!(previous.observation().revision(), Revision::new(1));

    session.update(observed_update(5, [255; 3])).unwrap();
    assert_eq!(state_kind(session.state()), StateKind::Ready);
    assert_eq!(verified_revision(session.state()), Some(Revision::new(5)));
}

#[test]
fn unknown_never_composites_and_idempotent_observed_never_rechecks() {
    let mut session = session();
    crate::composition::reset_source_over_evaluation_count();
    session.update(unknown_update(1, 1)).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    let update = observed_update(2, [255; 3]);
    session.update(update.clone()).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    let before = session.clone();
    session.update(update).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    assert_eq!(session, before);
}

#[test]
fn higher_revision_with_identical_physics_rechecks_and_rebinds_evidence() {
    let mut session = session();
    crate::composition::reset_source_over_evaluation_count();
    session.update(observed_update(1, [255; 3])).unwrap();
    session.update(observed_update(2, [255; 3])).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 2);
    assert_eq!(verified_revision(session.state()), Some(Revision::new(2)));
}

#[test]
fn rejected_updates_preserve_raw_and_lifecycle_and_do_not_call_evaluator() {
    let mut session = session();
    session.update(observed_update(2, [255; 3])).unwrap();
    for update in [
        malformed_update(3),
        observed_update(1, [255; 3]),
        ObservationUpdateInput {
            stream: ObservationStreamId::new(99),
            revision: Revision::new(3),
            payload: ObservationPayloadInput::Unknown(UnknownReasonId::new(1)),
        },
        observed_update(2, [0; 3]),
    ] {
        let raw_before = session.raw_head().clone();
        let state_before = session.state().clone();
        crate::composition::reset_source_over_evaluation_count();
        assert!(session.update(update).is_err());
        assert_eq!(crate::composition::source_over_evaluation_count(), 0);
        assert_eq!(session.raw_head(), &raw_before);
        assert_eq!(session.state(), &state_before);
    }
}

#[test]
fn resource_preflight_failure_is_atomic_and_retryable_at_same_revision() {
    let mut session = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    let raw_before = session.raw_head().clone();
    let state_before = session.state().clone();
    session.force_next_resource_failure();
    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        session.update(observed_update(2, [255; 3])),
        Err(FixedSessionUpdateErrorV1::ResourceExhausted)
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session.raw_head(), &raw_before);
    assert_eq!(session.state(), &state_before);

    session.update(observed_update(2, [255; 3])).unwrap();
    assert_eq!(verified_revision(session.state()), Some(Revision::new(2)));
}

proptest! {
    #[test]
    fn state_machine_matches_pure_last_verified_model(ops in prop::collection::vec(0u8..6, 1..80)) {
        let mut session = session();
        session.update(observed_update(1, [255; 3])).unwrap();
        let mut raw_revision = 1u64;
        let mut expected_kind = StateKind::Ready;
        let mut expected_verified = Some(Revision::new(1));
        let mut last_applied = observed_update(1, [255; 3]);

        for (next_revision, op) in (2u64..).zip(ops) {
            let raw_before = session.raw_head().clone();
            let state_before = session.state().clone();
            match op {
                0 => {
                    let update = observed_update(next_revision, [255; 3]);
                    session.update(update.clone()).unwrap();
                    raw_revision = next_revision;
                    expected_kind = StateKind::Ready;
                    expected_verified = Some(Revision::new(next_revision));
                    last_applied = update;
                }
                1 => {
                    let update = observed_update(next_revision, [0; 3]);
                    session.update(update.clone()).unwrap();
                    raw_revision = next_revision;
                    expected_kind = StateKind::Failed;
                    last_applied = update;
                }
                2 => {
                    let update = unknown_update(next_revision, u32::from(op));
                    session.update(update.clone()).unwrap();
                    raw_revision = next_revision;
                    expected_kind = if expected_verified.is_some() {
                        StateKind::Stale
                    } else {
                        StateKind::Waiting
                    };
                    last_applied = update;
                }
                3 => {
                    session.update(last_applied.clone()).unwrap();
                    prop_assert_eq!(session.raw_head(), &raw_before);
                    prop_assert_eq!(session.state(), &state_before);
                }
                4 => {
                    let rejected = if raw_revision == 0 {
                        unknown_update(0, 1)
                    } else {
                        observed_update(raw_revision, [17; 3])
                    };
                    prop_assert!(session.update(rejected).is_err());
                    prop_assert_eq!(session.raw_head(), &raw_before);
                    prop_assert_eq!(session.state(), &state_before);
                }
                _ => {
                    let mut rejected = unknown_update(next_revision, 1);
                    rejected.stream = ObservationStreamId::new(999);
                    prop_assert!(session.update(rejected).is_err());
                    prop_assert_eq!(session.raw_head(), &raw_before);
                    prop_assert_eq!(session.state(), &state_before);
                }
            }
            prop_assert_eq!(state_kind(session.state()), expected_kind);
            prop_assert_eq!(verified_revision(session.state()), expected_verified);
        }
    }
}
