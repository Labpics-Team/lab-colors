use proptest::prelude::*;

use crate::Srgb8;
use crate::appearance::{EncodedPointPaintV1, OccurrenceId, PaintId, SurfaceInputPortId};
use crate::composition::{AdmittedOpacityV1, CompositionProfileV1};
use crate::observation::{
    ObservationError, ObservationHeadViewV1, ObservationPayloadInput, ObservationStreamId,
    ObservationUpdateInput, ObservedScenarioSetInput, Revision, RevisionBoundObservationV1,
    ScenarioId, ScenarioInput, SurfaceInputBinding, UnknownReasonId,
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

fn raw_observed(session: &PointSupportSessionV1) -> &RevisionBoundObservationV1 {
    let ObservationHeadViewV1::Observed(observation) = session.raw_head() else {
        panic!("the concrete raw head must be Observed");
    };
    observation
}

fn assert_shared_observation_backing(
    raw: &RevisionBoundObservationV1,
    report: &RevisionBoundObservationV1,
) {
    assert_eq!(raw, report);
    assert_eq!(
        raw.backing_ptr_for_test(),
        report.backing_ptr_for_test(),
        "raw head and report must share one immutable observation backing",
    );
    assert_eq!(
        raw.schema().as_ptr(),
        report.schema().as_ptr(),
        "raw head and report must share immutable schema backing",
    );
    assert_eq!(raw.physical_case_count(), report.physical_case_count());
    for case_index in 0..raw.physical_case_count() {
        let raw_values = raw
            .physical_values(case_index)
            .expect("raw case must exist");
        let report_values = report
            .physical_values(case_index)
            .expect("report case must exist");
        assert_eq!(raw_values, report_values);
        assert_eq!(
            raw_values.as_ptr(),
            report_values.as_ptr(),
            "raw head and report must share immutable value backing",
        );

        let raw_provenance = raw
            .provenance(case_index)
            .expect("raw provenance must exist");
        let report_provenance = report
            .provenance(case_index)
            .expect("report provenance must exist");
        assert_eq!(raw_provenance, report_provenance);
        assert_eq!(
            raw_provenance.as_ptr(),
            report_provenance.as_ptr(),
            "raw head and report must share immutable provenance backing",
        );
    }
}

fn observation_backing_signature(
    observation: &RevisionBoundObservationV1,
) -> (*const SurfaceInputPortId, *const Srgb8, *const ScenarioId) {
    assert_eq!(observation.physical_case_count(), 1);
    (
        observation.schema().as_ptr(),
        observation
            .physical_values(0)
            .expect("fixture must have one physical case")
            .as_ptr(),
        observation
            .provenance(0)
            .expect("fixture must have provenance")
            .as_ptr(),
    )
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
        PointSupportSessionStateV1::Waiting => StateKind::Waiting,
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
        PointSupportSessionStateV1::Waiting
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
    session.update(observed_update(1, [255; 3])).unwrap();
    let PointSupportSessionStateV1::Ready { current } = session.state() else {
        panic!("white backdrop must verify #808080 target");
    };
    assert_eq!(current.report().observation().revision(), Revision::new(1));
    assert_eq!(
        current.report().cells().next().unwrap().provenance(),
        &[ScenarioId::new(1)]
    );
    assert_shared_observation_backing(raw_observed(&session), current.report().observation());

    session.update(observed_update(2, [0; 3])).unwrap();
    let PointSupportSessionStateV1::Failed { cause, previous } = session.state() else {
        panic!("black backdrop must violate #808080 target");
    };
    assert_eq!(cause.report().observation().revision(), Revision::new(2));
    assert_eq!(
        previous.as_ref().unwrap().report().observation().revision(),
        Revision::new(1)
    );
    assert_shared_observation_backing(raw_observed(&session), cause.report().observation());

    session.update(unknown_update(3, 9)).unwrap();
    let PointSupportSessionStateV1::Stale { previous } = session.state() else {
        panic!("unknown after a verified result must become Stale");
    };
    assert_eq!(previous.report().observation().revision(), Revision::new(1));
    let ObservationHeadViewV1::Unknown(current_unknown) = session.raw_head() else {
        panic!("lifecycle state must not own the current raw Unknown");
    };
    assert_eq!(current_unknown.stream(), STREAM);
    assert_eq!(current_unknown.revision(), Revision::new(3));
    assert_eq!(current_unknown.reason(), UnknownReasonId::new(9));
}

#[test]
fn violation_without_prior_then_unknown_is_waiting() {
    let mut session = session();
    assert!(matches!(
        session.update(observed_update(1, [0; 3])).unwrap(),
        PointSupportSessionStateV1::Failed { previous: None, .. }
    ));
    session.update(unknown_update(2, 1)).unwrap();
    assert!(matches!(
        session.state(),
        PointSupportSessionStateV1::Waiting
    ));
    let ObservationHeadViewV1::Unknown(current_unknown) = session.raw_head() else {
        panic!("Waiting must not embed the current raw Unknown");
    };
    assert_eq!(current_unknown.stream(), STREAM);
    assert_eq!(current_unknown.revision(), Revision::new(2));
    assert_eq!(current_unknown.reason(), UnknownReasonId::new(1));
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
fn observation_clone_and_exact_replay_reuse_the_same_backing_without_allocation() {
    let mut session = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    let raw = raw_observed(&session);
    let raw_signature = observation_backing_signature(raw);
    let PointSupportSessionStateV1::Ready { current } = session.state() else {
        panic!("white backdrop must verify");
    };
    assert_shared_observation_backing(raw, current.report().observation());

    let (snapshot, clone_allocations) = crate::test_support::measured_allocations(|| raw.clone());
    assert_eq!(clone_allocations, 0);
    assert_eq!(snapshot, *raw);
    assert_eq!(observation_backing_signature(&snapshot), raw_signature);

    crate::composition::reset_source_over_evaluation_count();
    let exact_replay = observed_update(1, [255; 3]);
    let (result, replay_allocations) =
        crate::test_support::measured_allocations(|| session.update(exact_replay).map(|_| ()));
    assert_eq!(result, Ok(()));
    assert_eq!(replay_allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(
        observation_backing_signature(raw_observed(&session)),
        raw_signature,
    );
    let PointSupportSessionStateV1::Ready { current } = session.state() else {
        panic!("exact replay must retain Ready");
    };
    assert_shared_observation_backing(raw_observed(&session), current.report().observation());
}

#[test]
fn higher_revision_with_equal_content_rechecks_and_binds_new_evidence() {
    let mut session = session();
    crate::composition::reset_source_over_evaluation_count();
    session.update(observed_update(1, [255; 3])).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);

    session.update(observed_update(2, [255; 3])).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 2);
    assert_eq!(session.raw_head().revision(), Some(Revision::new(2)));
    let PointSupportSessionStateV1::Ready { current } = session.state() else {
        panic!("equal content at a higher revision must produce fresh Ready evidence");
    };
    assert_eq!(current.report().observation().revision(), Revision::new(2));
    assert_shared_observation_backing(raw_observed(&session), current.report().observation());
}

#[test]
fn newer_unknown_replaces_only_raw_head_and_retains_one_verified_witness() {
    let mut session = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    let verified = raw_observed(&session).clone();
    let verified_signature = observation_backing_signature(&verified);

    session.update(unknown_update(2, 10)).unwrap();
    let PointSupportSessionStateV1::Stale { previous } = session.state() else {
        panic!("Unknown after Ready must become Stale");
    };
    assert_eq!(previous.report().observation(), &verified);
    assert_eq!(
        observation_backing_signature(previous.report().observation()),
        verified_signature,
    );

    session.update(unknown_update(3, 11)).unwrap();
    let PointSupportSessionStateV1::Stale { previous } = session.state() else {
        panic!("a newer Unknown must remain Stale");
    };
    assert_eq!(previous.report().observation(), &verified);
    assert_eq!(
        observation_backing_signature(previous.report().observation()),
        verified_signature,
    );
    let ObservationHeadViewV1::Unknown(raw_unknown) = session.raw_head() else {
        panic!("the current Unknown belongs only to the raw head");
    };
    assert_eq!(raw_unknown.revision(), Revision::new(3));
    assert_eq!(raw_unknown.reason(), UnknownReasonId::new(11));
}

#[test]
fn unknown_idempotent_and_rejected_updates_never_evaluate() {
    let mut session = session();
    crate::composition::reset_source_over_evaluation_count();
    session.update(unknown_update(1, 1)).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    let before = session.clone();
    let exact_unknown_replay = unknown_update(1, 1);
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session.update(exact_unknown_replay).map(|_| ())
    });
    assert!(result.is_ok());
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session, before);

    session.update(observed_update(2, [255; 3])).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    let before = session.clone();
    let exact_observed_replay = observed_update(2, [255; 3]);
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session.update(exact_observed_replay).map(|_| ())
    });
    assert!(result.is_ok());
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    assert_eq!(session, before);

    let before = session.clone();
    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        session.update(malformed_update(1)),
        Err(PointSupportSessionUpdateErrorV1::Observation(
            ObservationError::MissingSurfaceInputBinding {
                scenario: ScenarioId::new(1),
                input: SURFACE,
            },
        )),
        "malformed payload admission must precede lower-revision comparison",
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session, before);

    let before = session.clone();
    assert_eq!(
        session.update(malformed_update(2)),
        Err(PointSupportSessionUpdateErrorV1::Observation(
            ObservationError::MissingSurfaceInputBinding {
                scenario: ScenarioId::new(1),
                input: SURFACE,
            },
        )),
        "malformed payload admission must precede same-revision equality",
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session, before);

    let before = session.clone();
    assert_eq!(
        session.update(malformed_update(3)),
        Err(PointSupportSessionUpdateErrorV1::Observation(
            ObservationError::MissingSurfaceInputBinding {
                scenario: ScenarioId::new(1),
                input: SURFACE,
            },
        )),
        "malformed payload admission must precede applying a higher revision",
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session, before);

    let before = session.clone();
    assert_eq!(
        session.update(ObservationUpdateInput {
            stream: ObservationStreamId::new(99),
            revision: Revision::new(1),
            payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
                scenarios: vec![ScenarioInput {
                    id: ScenarioId::new(1),
                    bindings: vec![],
                }],
            }),
        }),
        Err(PointSupportSessionUpdateErrorV1::Observation(
            ObservationError::StreamMismatch {
                expected: STREAM,
                actual: ObservationStreamId::new(99),
            },
        )),
        "stream affinity must be checked before parsing foreign payloads",
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session, before);

    let before = session.clone();
    let lower_revision = observed_update(1, [255; 3]);
    let (result, allocations) =
        crate::test_support::measured_allocations(|| session.update(lower_revision).map(|_| ()));
    assert_eq!(
        result,
        Err(PointSupportSessionUpdateErrorV1::Observation(
            ObservationError::RevisionOutOfOrder {
                current: Revision::new(2),
                incoming: Revision::new(1),
            },
        )),
    );
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session, before);

    let before = session.clone();
    let same_revision_conflict = observed_update(2, [0; 3]);
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session.update(same_revision_conflict).map(|_| ())
    });
    assert_eq!(
        result,
        Err(PointSupportSessionUpdateErrorV1::Observation(
            ObservationError::RevisionConflict {
                revision: Revision::new(2),
            },
        )),
    );
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session, before);
}

#[test]
fn synthetic_post_admission_pre_evaluation_failure_is_atomic_and_retryable() {
    let mut session = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    let raw_signature = observation_backing_signature(raw_observed(&session));
    let PointSupportSessionStateV1::Ready { current } = session.state() else {
        panic!("white backdrop must verify");
    };
    assert_shared_observation_backing(raw_observed(&session), current.report().observation());
    let before = session.clone();
    // This hook fires after observation admission and before evaluation. It
    // proves transaction rollback at that Session boundary, not recoverable
    // failure of every allocation performed by Rust's global allocator.
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
    assert_eq!(
        observation_backing_signature(raw_observed(&session)),
        raw_signature,
    );
    let PointSupportSessionStateV1::Ready { current } = session.state() else {
        panic!("synthetic pre-evaluation failure must retain Ready");
    };
    assert_shared_observation_backing(raw_observed(&session), current.report().observation());

    session.update(observed_update(2, [255; 3])).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    assert_eq!(verified_revision(session.state()), Some(Revision::new(2)));
    let PointSupportSessionStateV1::Ready { current } = session.state() else {
        panic!("retry must commit Ready");
    };
    assert_shared_observation_backing(raw_observed(&session), current.report().observation());
}

#[test]
fn evaluator_failure_is_atomic_and_retryable() {
    let mut session = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    let raw_signature = observation_backing_signature(raw_observed(&session));
    let before = session.clone();

    session.force_next_evaluator_failure();
    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        session.update(observed_update(2, [255; 3])),
        Err(PointSupportSessionUpdateErrorV1::InternalInvariant),
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(session, before);
    assert_eq!(session.state(), before.state());
    assert_eq!(session.raw_head(), before.raw_head());
    assert_eq!(
        observation_backing_signature(raw_observed(&session)),
        raw_signature,
    );
    let PointSupportSessionStateV1::Ready { current } = session.state() else {
        panic!("evaluator failure must retain Ready");
    };
    assert_shared_observation_backing(raw_observed(&session), current.report().observation());

    session.update(observed_update(2, [255; 3])).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    assert_eq!(verified_revision(session.state()), Some(Revision::new(2)));
    let PointSupportSessionStateV1::Ready { current } = session.state() else {
        panic!("retry after evaluator failure must commit Ready");
    };
    assert_shared_observation_backing(raw_observed(&session), current.report().observation());
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
