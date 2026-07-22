use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::Srgb8;
use crate::appearance::SurfaceInputPortId;
use crate::lcs_occurrence::ColorSignal;
use crate::observation::{
    CanonicalObservationSchemaV1, ObservationError, ObservationHeadViewV1, ObservationPayloadInput,
    ObservationStreamId, ObservationUpdateInput, ObservedScenarioSetInput, Revision,
    RevisionBoundObservationV1, ScenarioId, ScenarioInput, SurfaceInputBinding, UnknownReasonId,
    canonicalize_observation_schema,
};
use crate::session::{
    Session, SessionDecision, SessionEvidenceV1, SessionObservationBindingPermitV1, SessionPlanV1,
    SessionState, SessionUpdateError, private as session_private,
};

const STREAM: ObservationStreamId = ObservationStreamId::new(31);
const FOREIGN_STREAM: ObservationStreamId = ObservationStreamId::new(32);
const SURFACE: SurfaceInputPortId = SurfaceInputPortId::new(21);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SentinelVerified {
    observation: RevisionBoundObservationV1,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SentinelViolation {
    observation: RevisionBoundObservationV1,
}

impl session_private::EvidenceSealed for SentinelVerified {}

impl SessionEvidenceV1 for SentinelVerified {
    fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }
}

impl session_private::EvidenceSealed for SentinelViolation {}

impl SessionEvidenceV1 for SentinelViolation {
    fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SentinelError {
    Forced,
    SchemaBackingMismatch,
    EmptyObservation,
}

#[derive(Debug, Clone)]
struct SentinelControl {
    evaluations: Rc<Cell<usize>>,
    fail_next: Rc<Cell<bool>>,
    substitute_next: Rc<RefCell<Option<RevisionBoundObservationV1>>>,
}

impl SentinelControl {
    fn evaluation_count(&self) -> usize {
        self.evaluations.get()
    }

    fn fail_next(&self) {
        self.fail_next.set(true);
    }

    fn substitute_next_with(&self, observation: RevisionBoundObservationV1) {
        *self.substitute_next.borrow_mut() = Some(observation);
    }
}

#[derive(Debug)]
struct SentinelPlan {
    schema: CanonicalObservationSchemaV1,
    control: SentinelControl,
}

impl session_private::PlanSealed for SentinelPlan {}

impl SessionPlanV1 for SentinelPlan {
    type Verified = SentinelVerified;
    type Violation = SentinelViolation;
    type Error = SentinelError;

    fn observation_schema(&self) -> &CanonicalObservationSchemaV1 {
        &self.schema
    }

    fn evaluate(
        &mut self,
        observation: RevisionBoundObservationV1,
        _permit: SessionObservationBindingPermitV1,
    ) -> Result<SessionDecision<Self::Verified, Self::Violation>, Self::Error> {
        self.control
            .evaluations
            .set(self.control.evaluations.get() + 1);
        if self.control.fail_next.replace(false) {
            return Err(SentinelError::Forced);
        }
        if !observation.shares_schema_backing_with(&self.schema) {
            return Err(SentinelError::SchemaBackingMismatch);
        }
        let first = observation
            .physical_values(0)
            .and_then(|values| values.first())
            .copied()
            .map(ColorSignal::srgb8)
            .ok_or(SentinelError::EmptyObservation)?;
        let observation = self
            .control
            .substitute_next
            .borrow_mut()
            .take()
            .unwrap_or(observation);
        if first == Srgb8::new([255; 3]) {
            Ok(SessionDecision::Verified(SentinelVerified { observation }))
        } else {
            Ok(SessionDecision::Violation(SentinelViolation {
                observation,
            }))
        }
    }
}

fn session() -> (
    Session<SentinelPlan>,
    SentinelControl,
    *const SurfaceInputPortId,
) {
    let schema = canonicalize_observation_schema(vec![SURFACE]).unwrap();
    let schema_ptr = schema.backing_ptr_for_test();
    let control = SentinelControl {
        evaluations: Rc::new(Cell::new(0)),
        fail_next: Rc::new(Cell::new(false)),
        substitute_next: Rc::new(RefCell::new(None)),
    };
    (
        Session::new(
            STREAM,
            SentinelPlan {
                schema,
                control: control.clone(),
            },
        ),
        control,
        schema_ptr,
    )
}

fn observed_update(revision: u64, value: [u8; 3]) -> ObservationUpdateInput {
    ObservationUpdateInput {
        stream: STREAM,
        revision: Revision::new(revision),
        payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
            scenarios: vec![ScenarioInput {
                id: ScenarioId::new(1),
                bindings: vec![SurfaceInputBinding::new(
                    SURFACE,
                    ColorSignal::from_srgb8(Srgb8::new(value)),
                )],
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
                bindings: Vec::new(),
            }],
        }),
    }
}

fn raw_observed(session: &Session<SentinelPlan>) -> &RevisionBoundObservationV1 {
    let ObservationHeadViewV1::Observed(observation) = session.raw_head() else {
        panic!("raw head must be Observed");
    };
    observation
}

fn verified_revision(
    state: &SessionState<SentinelVerified, SentinelViolation>,
) -> Option<Revision> {
    state
        .last_verified()
        .map(|verified| verified.observation.revision())
}

fn assert_shared_observation(
    raw: &RevisionBoundObservationV1,
    evidence: &RevisionBoundObservationV1,
) {
    assert_eq!(raw, evidence);
    assert_eq!(raw.backing_ptr_for_test(), evidence.backing_ptr_for_test());
    assert_eq!(raw.schema_ptr_for_test(), evidence.schema_ptr_for_test());
}

#[test]
fn construction_is_waiting_and_owns_no_raw_evidence() {
    let (session, control, _) = session();
    assert!(matches!(session.state(), SessionState::Waiting));
    assert_eq!(session.raw_head(), ObservationHeadViewV1::Empty);
    assert_eq!(control.evaluation_count(), 0);
}

#[test]
fn ready_failed_unknown_retains_exactly_one_verified_witness() {
    let (mut session, control, schema_ptr) = session();

    let current_observation = match session.update(observed_update(1, [255; 3])).unwrap() {
        SessionState::Ready { current } => current.observation.clone(),
        _ => panic!("white sentinel input must verify"),
    };
    assert_eq!(current_observation.revision(), Revision::new(1));
    assert_eq!(current_observation.schema_ptr_for_test(), schema_ptr);
    assert_shared_observation(raw_observed(&session), &current_observation);

    let (cause_observation, previous_revision) =
        match session.update(observed_update(2, [0; 3])).unwrap() {
            SessionState::Failed { cause, previous } => (
                cause.observation.clone(),
                previous.as_ref().unwrap().observation.revision(),
            ),
            _ => panic!("black sentinel input must violate"),
        };
    assert_eq!(cause_observation.revision(), Revision::new(2));
    assert_eq!(previous_revision, Revision::new(1));
    assert_shared_observation(raw_observed(&session), &cause_observation);

    let previous_revision = match session.update(unknown_update(3, 9)).unwrap() {
        SessionState::Stale { previous } => previous.observation.revision(),
        _ => panic!("Unknown after a verified result must become Stale"),
    };
    assert_eq!(previous_revision, Revision::new(1));
    let ObservationHeadViewV1::Unknown(unknown) = session.raw_head() else {
        panic!("Unknown belongs to the separate raw head");
    };
    assert_eq!(unknown.stream(), STREAM);
    assert_eq!(unknown.revision(), Revision::new(3));
    assert_eq!(unknown.reason(), UnknownReasonId::new(9));
    assert_eq!(control.evaluation_count(), 2);
}

#[test]
fn violation_without_verified_then_unknown_returns_to_waiting() {
    let (mut session, control, _) = session();
    assert!(matches!(
        session.update(observed_update(1, [0; 3])).unwrap(),
        SessionState::Failed { previous: None, .. }
    ));
    assert!(matches!(
        session.update(unknown_update(2, 7)).unwrap(),
        SessionState::Waiting
    ));
    assert_eq!(verified_revision(session.state()), None);
    assert_eq!(control.evaluation_count(), 1);
}

#[test]
fn exact_replay_is_idempotent_and_never_invokes_the_plan() {
    let (mut session, control, _) = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    let raw_backing = raw_observed(&session).backing_ptr_for_test();
    assert_eq!(control.evaluation_count(), 1);

    session.update(observed_update(1, [255; 3])).unwrap();
    assert_eq!(control.evaluation_count(), 1);
    assert_eq!(raw_observed(&session).backing_ptr_for_test(), raw_backing);
    let SessionState::Ready { current } = session.state() else {
        panic!("exact replay must retain Ready");
    };
    assert_shared_observation(raw_observed(&session), &current.observation);

    session.update(unknown_update(2, 11)).unwrap();
    session.update(unknown_update(2, 11)).unwrap();
    assert_eq!(control.evaluation_count(), 1);
    assert_eq!(session.raw_head().revision(), Some(Revision::new(2)));
}

#[test]
fn equal_content_at_a_higher_revision_rebinds_fresh_evidence() {
    let (mut session, control, _) = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    let first_backing = raw_observed(&session).backing_ptr_for_test();

    session.update(observed_update(2, [255; 3])).unwrap();
    assert_eq!(control.evaluation_count(), 2);
    assert_ne!(raw_observed(&session).backing_ptr_for_test(), first_backing);
    let SessionState::Ready { current } = session.state() else {
        panic!("higher revision must produce fresh Ready evidence");
    };
    assert_eq!(current.observation.revision(), Revision::new(2));
    assert_shared_observation(raw_observed(&session), &current.observation);
}

#[test]
fn rejected_admission_neither_invokes_plan_nor_mutates_closed_state() {
    let (mut session, control, _) = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    let raw_backing = raw_observed(&session).backing_ptr_for_test();

    let mut foreign = observed_update(2, [0; 3]);
    foreign.stream = FOREIGN_STREAM;
    assert_eq!(
        session.update(foreign),
        Err(SessionUpdateError::Observation(
            ObservationError::StreamMismatch {
                expected: STREAM,
                actual: FOREIGN_STREAM,
            }
        ))
    );
    assert_eq!(
        session.update(malformed_update(2)),
        Err(SessionUpdateError::Observation(
            ObservationError::MissingSurfaceInputBinding {
                scenario: ScenarioId::new(1),
                input: SURFACE,
            }
        ))
    );
    assert_eq!(
        session.update(observed_update(0, [255; 3])),
        Err(SessionUpdateError::Observation(
            ObservationError::RevisionOutOfOrder {
                current: Revision::new(1),
                incoming: Revision::new(0),
            }
        ))
    );
    assert_eq!(
        session.update(observed_update(1, [0; 3])),
        Err(SessionUpdateError::Observation(
            ObservationError::RevisionConflict {
                revision: Revision::new(1),
            }
        ))
    );

    assert_eq!(control.evaluation_count(), 1);
    assert_eq!(raw_observed(&session).backing_ptr_for_test(), raw_backing);
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));
}

#[test]
fn plan_failure_commits_neither_raw_head_nor_lifecycle_and_retry_is_fresh() {
    let (mut session, control, _) = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    let raw_backing = raw_observed(&session).backing_ptr_for_test();
    control.fail_next();

    assert_eq!(
        session.update(observed_update(2, [0; 3])),
        Err(SessionUpdateError::Plan(SentinelError::Forced))
    );
    assert_eq!(control.evaluation_count(), 2);
    assert_eq!(raw_observed(&session).backing_ptr_for_test(), raw_backing);
    assert_eq!(session.raw_head().revision(), Some(Revision::new(1)));
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));

    let (cause_observation, previous_revision) =
        match session.update(observed_update(2, [0; 3])).unwrap() {
            SessionState::Failed { cause, previous } => (
                cause.observation.clone(),
                previous.as_ref().unwrap().observation.revision(),
            ),
            _ => panic!("retry must re-prepare, re-evaluate and commit"),
        };
    assert_eq!(control.evaluation_count(), 3);
    assert_eq!(cause_observation.revision(), Revision::new(2));
    assert_eq!(previous_revision, Revision::new(1));
    assert_shared_observation(raw_observed(&session), &cause_observation);
}

#[test]
fn detached_plan_evidence_is_rejected_before_raw_or_lifecycle_commit() {
    let (mut session, control, _) = session();
    session.update(observed_update(1, [255; 3])).unwrap();
    let first_observation = raw_observed(&session).clone();
    control.substitute_next_with(first_observation);

    assert_eq!(
        session.update(observed_update(2, [255; 3])),
        Err(SessionUpdateError::EvidenceBindingInvariant)
    );
    assert_eq!(control.evaluation_count(), 2);
    assert_eq!(session.raw_head().revision(), Some(Revision::new(1)));
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));

    let current_observation = match session.update(observed_update(2, [255; 3])).unwrap() {
        SessionState::Ready { current } => current.observation.clone(),
        _ => panic!("a fresh retry must bind evidence from the current observation"),
    };
    assert_eq!(control.evaluation_count(), 3);
    assert_eq!(current_observation.revision(), Revision::new(2));
    assert_shared_observation(raw_observed(&session), &current_observation);
}

#[test]
fn session_source_contains_one_generic_update_owner_and_no_legacy_runtime() {
    let source = include_str!("session.rs");
    assert_eq!(source.matches("pub(crate) fn update(").count(), 1);
    for forbidden in [
        "PointSupportSessionV1",
        "PointSupportSessionStateV1",
        "PointSupportSessionUpdateErrorV1",
        "Weak<",
        "ProgramExpired",
        "ObservationStreamBinding",
        "SurfaceUpdate",
        "Box<dyn SessionPlanV1",
    ] {
        assert!(
            !source.contains(forbidden),
            "legacy or dual-runtime marker survived: {forbidden}"
        );
    }
}
