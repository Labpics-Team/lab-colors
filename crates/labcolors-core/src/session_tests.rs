use proptest::prelude::*;

use crate::Srgb8;
use crate::appearance::{EncodedPointPaintV1, OccurrenceId, PaintId, SurfaceInputPortId};
use crate::composition::{AdmittedOpacityV1, CompositionProfileV1};
use crate::observation::{
    ObservationError, ObservationHeadViewV1, ObservationPayloadInput, ObservationStreamId,
    ObservationUpdateInput, ObservedScenarioSetInput, Revision, ScenarioId, ScenarioInput,
    SurfaceInputBinding, UnknownReasonId,
};
use crate::point_support::{
    CompiledPointSupportRecheckV1, PointSupportCriterionRequirementV1,
    PointSupportOccurrenceRequirementV1, PointSupportStabilityPolicyV1,
};
use crate::session::{
    PointSupportSessionStateV1, PointSupportSessionUpdateErrorV1, PointSupportSessionV1,
};

const PAINT: PaintId = PaintId::new(7);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(11);
const SURFACE: SurfaceInputPortId = SurfaceInputPortId::new(21);
const STREAM: ObservationStreamId = ObservationStreamId::new(31);
const TARGET: [u8; 3] = [128; 3];

fn candidate() -> EncodedPointPaintV1 {
    EncodedPointPaintV1::from_admitted(
        PAINT,
        Srgb8::new([0; 3]),
        AdmittedOpacityV1::new(0.5).unwrap(),
    )
}

fn requirement() -> CompiledPointSupportRecheckV1 {
    CompiledPointSupportRecheckV1::new(
        CompositionProfileV1::EncodedSrgb8SourceOverV1,
        vec![PointSupportOccurrenceRequirementV1::new(
            OCCURRENCE,
            SURFACE,
            candidate(),
            Some(Srgb8::new(TARGET)),
            PointSupportCriterionRequirementV1::NotRequested,
            PointSupportStabilityPolicyV1::Disabled,
        )],
    )
    .unwrap()
}

fn session() -> PointSupportSessionV1 {
    PointSupportSessionV1::new(STREAM, requirement())
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

fn verified_revision(state: &PointSupportSessionStateV1) -> Option<Revision> {
    state
        .last_verified()
        .map(|verified| verified.report().observation().revision())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateKind {
    Waiting,
    Ready,
    Stale,
    Failed,
}

fn state_kind(state: &PointSupportSessionStateV1) -> StateKind {
    match state {
        PointSupportSessionStateV1::Waiting { .. } => StateKind::Waiting,
        PointSupportSessionStateV1::Ready { .. } => StateKind::Ready,
        PointSupportSessionStateV1::Stale { .. } => StateKind::Stale,
        PointSupportSessionStateV1::Failed { .. } => StateKind::Failed,
    }
}

#[test]
fn construction_uses_only_the_compiled_schema_and_profile() {
    let session = session();
    assert!(matches!(
        session.state(),
        PointSupportSessionStateV1::Waiting {
            current_unknown: None,
        }
    ));
    assert_eq!(session.raw_head(), ObservationHeadViewV1::Empty);
    assert_eq!(
        session.composition_profile(),
        CompositionProfileV1::EncodedSrgb8SourceOverV1
    );
}

#[test]
fn ready_violation_unknown_preserves_exactly_one_verified_witness() {
    let mut session = session();
    let PointSupportSessionStateV1::Ready { current } =
        session.update(observed_update(1, [255; 3])).unwrap()
    else {
        panic!("white backdrop must verify #808080 target");
    };
    assert_eq!(current.report().observation().revision(), Revision::new(1));
    assert_eq!(
        current.report().cells().next().unwrap().provenance(),
        &[ScenarioId::new(1)]
    );

    let PointSupportSessionStateV1::Failed { cause, previous } =
        session.update(observed_update(2, [0; 3])).unwrap()
    else {
        panic!("black backdrop must violate #808080 target");
    };
    assert_eq!(cause.report().observation().revision(), Revision::new(2));
    assert_eq!(
        previous.as_ref().unwrap().report().observation().revision(),
        Revision::new(1)
    );

    let PointSupportSessionStateV1::Stale {
        previous,
        current_unknown,
    } = session.update(unknown_update(3, 9)).unwrap()
    else {
        panic!("unknown after a verified result must become Stale");
    };
    let expected_unknown = *current_unknown;
    assert_eq!(previous.report().observation().revision(), Revision::new(1));
    assert_eq!(current_unknown.stream(), STREAM);
    assert_eq!(current_unknown.revision(), Revision::new(3));
    assert_eq!(current_unknown.reason(), UnknownReasonId::new(9));
    assert_eq!(
        session.raw_head(),
        ObservationHeadViewV1::Unknown(&expected_unknown)
    );
}

#[test]
fn violation_without_prior_then_unknown_is_waiting() {
    let mut session = session();
    assert!(matches!(
        session.update(observed_update(1, [0; 3])).unwrap(),
        PointSupportSessionStateV1::Failed { previous: None, .. }
    ));
    let PointSupportSessionStateV1::Waiting {
        current_unknown: Some(current_unknown),
    } = session.update(unknown_update(2, 1)).unwrap()
    else {
        panic!("unknown without a verified result must be retained by Waiting");
    };
    let expected_unknown = *current_unknown;
    assert_eq!(current_unknown.stream(), STREAM);
    assert_eq!(current_unknown.revision(), Revision::new(2));
    assert_eq!(current_unknown.reason(), UnknownReasonId::new(1));
    assert_eq!(
        session.raw_head(),
        ObservationHeadViewV1::Unknown(&expected_unknown)
    );
    assert_eq!(verified_revision(session.state()), None);
}

#[test]
fn stale_and_failed_transitions_move_one_previous_without_history() {
    let mut session = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    session.update(unknown_update(2, 1)).unwrap();
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));
    assert_eq!(session.raw_head().revision(), Some(Revision::new(2)));

    session.update(observed_update(3, [0; 3])).unwrap();
    assert_eq!(state_kind(session.state()), StateKind::Failed);
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));
    assert_eq!(session.raw_head().revision(), Some(Revision::new(3)));

    session.update(unknown_update(4, 2)).unwrap();
    assert_eq!(state_kind(session.state()), StateKind::Stale);
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));
    assert_eq!(session.raw_head().revision(), Some(Revision::new(4)));

    session.update(observed_update(5, [255; 3])).unwrap();
    assert_eq!(state_kind(session.state()), StateKind::Ready);
    assert_eq!(verified_revision(session.state()), Some(Revision::new(5)));
    assert_eq!(session.raw_head().revision(), Some(Revision::new(5)));
}

#[test]
fn unknown_idempotent_and_rejected_updates_never_evaluate() {
    let mut session = session();
    crate::composition::reset_source_over_evaluation_count();
    let unknown = unknown_update(1, 1);
    session.update(unknown.clone()).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    let before = session.clone();
    session.update(unknown).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session, before);

    let update = observed_update(2, [255; 3]);
    session.update(update.clone()).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    let before = session.clone();
    session.update(update).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    assert_eq!(session, before);

    let before = session.clone();
    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        session.update(malformed_update(1)),
        Err(PointSupportSessionUpdateErrorV1::Observation(
            ObservationError::RevisionOutOfOrder {
                current: Revision::new(2),
                incoming: Revision::new(1),
            },
        ))
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session, before);

    for rejected in [
        malformed_update(3),
        ObservationUpdateInput {
            stream: ObservationStreamId::new(99),
            revision: Revision::new(3),
            payload: ObservationPayloadInput::Unknown(UnknownReasonId::new(1)),
        },
        observed_update(2, [0; 3]),
    ] {
        let before = session.clone();
        crate::composition::reset_source_over_evaluation_count();
        assert!(session.update(rejected).is_err());
        assert_eq!(crate::composition::source_over_evaluation_count(), 0);
        assert_eq!(session, before);
    }
}

#[test]
fn resource_preflight_failure_is_atomic_and_retryable() {
    let mut session = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    let before = session.clone();
    session.force_next_resource_failure();
    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        session.update(observed_update(2, [255; 3])),
        Err(PointSupportSessionUpdateErrorV1::ResourceExhausted)
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session, before);
    assert_eq!(session.state(), before.state());
    assert_eq!(session.raw_head(), before.raw_head());
    assert_eq!(session.composition_profile(), before.composition_profile());

    session.update(observed_update(2, [255; 3])).unwrap();
    assert_eq!(verified_revision(session.state()), Some(Revision::new(2)));
}

proptest! {
    #[test]
    fn lifecycle_matches_pure_last_verified_model(ops in prop::collection::vec(0u8..5, 1..60)) {
        let mut session = session();
        session.update(observed_update(1, [255; 3])).unwrap();
        let mut raw_revision = 1u64;
        let mut expected_kind = StateKind::Ready;
        let mut expected_verified = Some(Revision::new(1));
        let mut last_applied = observed_update(1, [255; 3]);

        for (next_revision, op) in (2u64..).zip(ops) {
            let before = session.clone();
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
                    prop_assert_eq!(&session, &before);
                }
                _ => {
                    let rejected = observed_update(raw_revision, [17; 3]);
                    prop_assert!(session.update(rejected).is_err());
                    prop_assert_eq!(&session, &before);
                }
            }
            prop_assert_eq!(state_kind(session.state()), expected_kind);
            prop_assert_eq!(verified_revision(session.state()), expected_verified);
        }
    }
}
