use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::Srgb8;
use crate::appearance::SurfaceInputPortId;
use crate::lcs_occurrence::ColorSignal;
use crate::observation::{
    CanonicalObservationSchemaV1, OBSERVATION_ARENA_SLOT_COUNT_V1, ObservationError,
    ObservationHeadViewV1, ObservationPayloadInput, ObservationStreamId, ObservationUpdateInput,
    ObservedScenarioSetInput, Revision, RevisionBoundObservationV1, ScenarioId, ScenarioInput,
    SchemaOrderedScenarioSourceV1, SurfaceInputBinding, UnknownReasonId,
    canonicalize_observation_schema,
};
use crate::session::{
    PreparedSessionDispositionV1, Session, SessionDecision, SessionEvidenceV1,
    SessionObservationBindingPermitV1, SessionPlanV1, SessionState, SessionUpdateError,
    private as session_private,
};

/// Characterization tests spell an immediate commit explicitly through this
/// test-only trait while production retains no such authority.
type CommittedSessionStateResult<'session, Plan> = Result<
    &'session SessionState<<Plan as SessionPlanV1>::Verified, <Plan as SessionPlanV1>::Violation>,
    SessionUpdateError<<Plan as SessionPlanV1>::Error>,
>;

pub(crate) trait CommitSessionUpdateForTest<Plan: SessionPlanV1> {
    fn commit(&mut self, update: ObservationUpdateInput) -> CommittedSessionStateResult<'_, Plan>;

    fn commit_schema_ordered<Source: SchemaOrderedScenarioSourceV1>(
        &mut self,
        revision: Revision,
        source: &Source,
        order_scratch: &mut Vec<usize>,
    ) -> CommittedSessionStateResult<'_, Plan>;
}

impl<Plan: SessionPlanV1> CommitSessionUpdateForTest<Plan> for Session<Plan> {
    fn commit(&mut self, update: ObservationUpdateInput) -> CommittedSessionStateResult<'_, Plan> {
        self.prepare_update(update)
            .map(|prepared| prepared.commit().state())
    }

    fn commit_schema_ordered<Source: SchemaOrderedScenarioSourceV1>(
        &mut self,
        revision: Revision,
        source: &Source,
        order_scratch: &mut Vec<usize>,
    ) -> CommittedSessionStateResult<'_, Plan> {
        self.prepare_schema_ordered(revision, source, order_scratch)
            .map(|prepared| prepared.commit().state())
    }
}

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
    type OwnerLease = ();
    type Verified = SentinelVerified;
    type Violation = SentinelViolation;
    type Error = SentinelError;

    fn try_acquire_owner(&self) -> Option<Self::OwnerLease> {
        Some(())
    }

    fn observation_schema<'a>(
        &'a self,
        _owner: &'a Self::OwnerLease,
    ) -> &'a CanonicalObservationSchemaV1 {
        &self.schema
    }

    fn evaluate(
        &mut self,
        _owner: &Self::OwnerLease,
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

#[derive(Debug)]
struct ReplacingOwnerGeneration {
    schema: CanonicalObservationSchemaV1,
}

#[derive(Debug)]
struct ReplacingOwnerPlan {
    generation: std::rc::Weak<ReplacingOwnerGeneration>,
    owner_slot: Rc<RefCell<Option<Rc<ReplacingOwnerGeneration>>>>,
    evaluations: Rc<Cell<usize>>,
}

impl session_private::PlanSealed for ReplacingOwnerPlan {}

impl SessionPlanV1 for ReplacingOwnerPlan {
    type OwnerLease = Rc<ReplacingOwnerGeneration>;
    type Verified = SentinelVerified;
    type Violation = SentinelViolation;
    type Error = SentinelError;

    fn try_acquire_owner(&self) -> Option<Self::OwnerLease> {
        self.generation.upgrade()
    }

    fn observation_schema<'a>(
        &'a self,
        owner: &'a Self::OwnerLease,
    ) -> &'a CanonicalObservationSchemaV1 {
        &owner.schema
    }

    fn evaluate(
        &mut self,
        owner: &Self::OwnerLease,
        observation: RevisionBoundObservationV1,
        _permit: SessionObservationBindingPermitV1,
    ) -> Result<SessionDecision<Self::Verified, Self::Violation>, Self::Error> {
        self.evaluations.set(self.evaluations.get() + 1);
        assert!(
            observation.shares_schema_backing_with(&owner.schema),
            "admission and evaluation must use the same pinned generation"
        );
        let old_generation = self
            .owner_slot
            .borrow_mut()
            .replace(Rc::new(ReplacingOwnerGeneration {
                schema: canonicalize_observation_schema(vec![SURFACE]).unwrap(),
            }))
            .expect("the first owner generation must still be installed");
        assert!(Rc::ptr_eq(owner, &old_generation));
        drop(old_generation);
        assert!(
            self.generation.upgrade().is_some(),
            "the transaction lease must pin its starting owner generation"
        );
        Ok(SessionDecision::Verified(SentinelVerified { observation }))
    }
}

#[derive(Debug)]
struct DropOrderOwnerGeneration {
    schema: CanonicalObservationSchemaV1,
    alive: Rc<Cell<bool>>,
    events: Rc<RefCell<Vec<&'static str>>>,
}

impl Drop for DropOrderOwnerGeneration {
    fn drop(&mut self) {
        self.alive.set(false);
        self.events.borrow_mut().push("owner");
    }
}

#[derive(Debug)]
struct DropOrderEvidence {
    observation: RevisionBoundObservationV1,
    owner_alive: Rc<Cell<bool>>,
    events: Rc<RefCell<Vec<&'static str>>>,
}

impl Drop for DropOrderEvidence {
    fn drop(&mut self) {
        self.events.borrow_mut().push(if self.owner_alive.get() {
            "evidence"
        } else {
            "evidence-after-owner"
        });
    }
}

impl session_private::EvidenceSealed for DropOrderEvidence {}

impl SessionEvidenceV1 for DropOrderEvidence {
    fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }
}

#[derive(Debug)]
struct RetirementEvidence {
    observation: RevisionBoundObservationV1,
    panic_on_next_drop: Rc<Cell<bool>>,
    drops: Rc<Cell<usize>>,
}

impl Drop for RetirementEvidence {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
        if self.panic_on_next_drop.replace(false) {
            panic!("retired evidence observed the commit boundary");
        }
    }
}

impl session_private::EvidenceSealed for RetirementEvidence {}

impl SessionEvidenceV1 for RetirementEvidence {
    fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }
}

#[derive(Debug)]
struct RetirementPlan {
    schema: CanonicalObservationSchemaV1,
    panic_on_next_drop: Rc<Cell<bool>>,
    drops: Rc<Cell<usize>>,
}

impl session_private::PlanSealed for RetirementPlan {}

impl SessionPlanV1 for RetirementPlan {
    type OwnerLease = ();
    type Verified = RetirementEvidence;
    type Violation = RetirementEvidence;
    type Error = SentinelError;

    fn try_acquire_owner(&self) -> Option<Self::OwnerLease> {
        Some(())
    }

    fn observation_schema<'a>(
        &'a self,
        _owner: &'a Self::OwnerLease,
    ) -> &'a CanonicalObservationSchemaV1 {
        &self.schema
    }

    fn evaluate(
        &mut self,
        _owner: &Self::OwnerLease,
        observation: RevisionBoundObservationV1,
        _permit: SessionObservationBindingPermitV1,
    ) -> Result<SessionDecision<Self::Verified, Self::Violation>, Self::Error> {
        if !observation.shares_schema_backing_with(&self.schema) {
            return Err(SentinelError::SchemaBackingMismatch);
        }
        let first = observation
            .physical_values(0)
            .and_then(|values| values.first())
            .copied()
            .map(ColorSignal::srgb8)
            .ok_or(SentinelError::EmptyObservation)?;
        let evidence = RetirementEvidence {
            observation,
            panic_on_next_drop: Rc::clone(&self.panic_on_next_drop),
            drops: Rc::clone(&self.drops),
        };
        if first == Srgb8::new([255; 3]) {
            Ok(SessionDecision::Verified(evidence))
        } else {
            Ok(SessionDecision::Violation(evidence))
        }
    }
}

#[derive(Debug)]
struct DropOrderPlan {
    generation: std::rc::Weak<DropOrderOwnerGeneration>,
    owner_slot: Rc<RefCell<Option<Rc<DropOrderOwnerGeneration>>>>,
}

impl session_private::PlanSealed for DropOrderPlan {}

impl SessionPlanV1 for DropOrderPlan {
    type OwnerLease = Rc<DropOrderOwnerGeneration>;
    type Verified = DropOrderEvidence;
    type Violation = DropOrderEvidence;
    type Error = SentinelError;

    fn try_acquire_owner(&self) -> Option<Self::OwnerLease> {
        self.generation.upgrade()
    }

    fn observation_schema<'a>(
        &'a self,
        owner: &'a Self::OwnerLease,
    ) -> &'a CanonicalObservationSchemaV1 {
        &owner.schema
    }

    fn evaluate(
        &mut self,
        owner: &Self::OwnerLease,
        observation: RevisionBoundObservationV1,
        _permit: SessionObservationBindingPermitV1,
    ) -> Result<SessionDecision<Self::Verified, Self::Violation>, Self::Error> {
        let installed = self
            .owner_slot
            .borrow_mut()
            .take()
            .expect("the prepared transition must become the sole generation lease");
        assert!(Rc::ptr_eq(owner, &installed));
        drop(installed);
        Ok(SessionDecision::Verified(DropOrderEvidence {
            observation,
            owner_alive: Rc::clone(&owner.alive),
            events: Rc::clone(&owner.events),
        }))
    }
}

struct OneOrderedScenario {
    id: ScenarioId,
    value: Srgb8,
}

impl SchemaOrderedScenarioSourceV1 for OneOrderedScenario {
    fn scenario_count(&self) -> usize {
        1
    }

    fn scenario_id(&self, scenario_index: usize) -> ScenarioId {
        assert_eq!(scenario_index, 0);
        self.id
    }

    fn value_count(&self, scenario_index: usize) -> usize {
        assert_eq!(scenario_index, 0);
        1
    }

    fn value(&self, scenario_index: usize, binding_index: usize) -> Srgb8 {
        assert_eq!((scenario_index, binding_index), (0, 0));
        self.value
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

fn retirement_session() -> (Session<RetirementPlan>, Rc<Cell<bool>>, Rc<Cell<usize>>) {
    let panic_on_next_drop = Rc::new(Cell::new(false));
    let drops = Rc::new(Cell::new(0));
    (
        Session::new(
            STREAM,
            RetirementPlan {
                schema: canonicalize_observation_schema(vec![SURFACE]).unwrap(),
                panic_on_next_drop: Rc::clone(&panic_on_next_drop),
                drops: Rc::clone(&drops),
            },
        ),
        panic_on_next_drop,
        drops,
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

    let current_observation = match session.commit(observed_update(1, [255; 3])).unwrap() {
        SessionState::Ready { current } => current.observation.clone(),
        _ => panic!("white sentinel input must verify"),
    };
    assert_eq!(current_observation.revision(), Revision::new(1));
    assert_eq!(current_observation.schema_ptr_for_test(), schema_ptr);
    assert_shared_observation(raw_observed(&session), &current_observation);

    let (cause_observation, previous_revision) =
        match session.commit(observed_update(2, [0; 3])).unwrap() {
            SessionState::Failed { cause, previous } => (
                cause.observation.clone(),
                previous.as_ref().unwrap().observation.revision(),
            ),
            _ => panic!("black sentinel input must violate"),
        };
    assert_eq!(cause_observation.revision(), Revision::new(2));
    assert_eq!(previous_revision, Revision::new(1));
    assert_shared_observation(raw_observed(&session), &cause_observation);

    let previous_revision = match session.commit(unknown_update(3, 9)).unwrap() {
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
        session.commit(observed_update(1, [0; 3])).unwrap(),
        SessionState::Failed { previous: None, .. }
    ));
    assert!(matches!(
        session.commit(unknown_update(2, 7)).unwrap(),
        SessionState::Waiting
    ));
    assert_eq!(verified_revision(session.state()), None);
    assert_eq!(control.evaluation_count(), 1);
}

#[test]
fn exact_replay_is_idempotent_and_never_invokes_the_plan() {
    let (mut session, control, _) = session();
    session.commit(observed_update(1, [255; 3])).unwrap();
    let raw_backing = raw_observed(&session).backing_ptr_for_test();
    assert_eq!(control.evaluation_count(), 1);

    session.commit(observed_update(1, [255; 3])).unwrap();
    assert_eq!(control.evaluation_count(), 1);
    assert_eq!(raw_observed(&session).backing_ptr_for_test(), raw_backing);
    let SessionState::Ready { current } = session.state() else {
        panic!("exact replay must retain Ready");
    };
    assert_shared_observation(raw_observed(&session), &current.observation);

    session.commit(unknown_update(2, 11)).unwrap();
    session.commit(unknown_update(2, 11)).unwrap();
    assert_eq!(control.evaluation_count(), 1);
    assert_eq!(session.raw_head().revision(), Some(Revision::new(2)));
}

#[test]
fn schema_ordered_admission_reuses_the_prewarmed_canonical_schema_arenas() {
    let (mut session, control, schema_ptr) = session();
    let session_schema_handle_count = 1 + OBSERVATION_ARENA_SLOT_COUNT_V1;
    assert_eq!(
        session
            .plan()
            .observation_schema(&())
            .strong_count_for_test(),
        session_schema_handle_count,
    );
    let source = OneOrderedScenario {
        id: ScenarioId::new(1),
        value: Srgb8::new([255; 3]),
    };
    let mut order_scratch = Vec::new();

    let SessionState::Ready { current } = session
        .commit_schema_ordered(Revision::new(1), &source, &mut order_scratch)
        .unwrap()
    else {
        panic!("white sentinel input must verify");
    };
    assert_eq!(current.observation.schema_ptr_for_test(), schema_ptr);
    let observation_backing_ptr = current.observation.backing_ptr_for_test();
    assert_eq!(
        session
            .plan()
            .observation_schema(&())
            .strong_count_for_test(),
        session_schema_handle_count,
    );
    assert_eq!(control.evaluation_count(), 1);

    let idempotent_observation_backing_ptr = match session
        .commit_schema_ordered(Revision::new(1), &source, &mut order_scratch)
        .unwrap()
    {
        SessionState::Ready { current } => current.observation.backing_ptr_for_test(),
        _ => panic!("an exact schema-ordered replay must retain Ready"),
    };
    assert_eq!(idempotent_observation_backing_ptr, observation_backing_ptr);
    assert_eq!(
        session
            .plan()
            .observation_schema(&())
            .strong_count_for_test(),
        session_schema_handle_count,
    );
    assert_eq!(control.evaluation_count(), 1);

    let observation_clone = match session.state() {
        SessionState::Ready { current } => current.observation.clone(),
        _ => panic!("the verified observation must remain current"),
    };
    assert_eq!(observation_clone.schema_ptr_for_test(), schema_ptr);
    assert_eq!(
        session
            .plan()
            .observation_schema(&())
            .strong_count_for_test(),
        session_schema_handle_count,
        "cloning an observation must not clone the canonical schema Rc",
    );
    drop(observation_clone);

    let schema_probe = session.plan().observation_schema(&()).clone();
    assert_eq!(
        schema_probe.strong_count_for_test(),
        session_schema_handle_count + 1,
    );
    drop(session);
    assert_eq!(schema_probe.backing_ptr_for_test(), schema_ptr);
    assert_eq!(
        schema_probe.strong_count_for_test(),
        1,
        "dropping the Session must release all three persistent arena schema handles",
    );
}

#[test]
fn equal_content_at_a_higher_revision_rebinds_fresh_evidence() {
    let (mut session, control, _) = session();
    session.commit(observed_update(1, [255; 3])).unwrap();
    let first_backing = raw_observed(&session).backing_ptr_for_test();

    session.commit(observed_update(2, [255; 3])).unwrap();
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
    session.commit(observed_update(1, [255; 3])).unwrap();
    let raw_backing = raw_observed(&session).backing_ptr_for_test();

    let mut foreign = observed_update(2, [0; 3]);
    foreign.stream = FOREIGN_STREAM;
    assert_eq!(
        session.commit(foreign),
        Err(SessionUpdateError::Observation(
            ObservationError::StreamMismatch {
                expected: STREAM,
                actual: FOREIGN_STREAM,
            }
        ))
    );
    assert_eq!(
        session.commit(malformed_update(2)),
        Err(SessionUpdateError::Observation(
            ObservationError::MissingSurfaceInputBinding {
                scenario: ScenarioId::new(1),
                input: SURFACE,
            }
        ))
    );
    assert_eq!(
        session.commit(observed_update(0, [255; 3])),
        Err(SessionUpdateError::Observation(
            ObservationError::RevisionOutOfOrder {
                current: Revision::new(1),
                incoming: Revision::new(0),
            }
        ))
    );
    assert_eq!(
        session.commit(observed_update(1, [0; 3])),
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
    session.commit(observed_update(1, [255; 3])).unwrap();
    let raw_backing = raw_observed(&session).backing_ptr_for_test();
    control.fail_next();

    assert_eq!(
        session.commit(observed_update(2, [0; 3])),
        Err(SessionUpdateError::Plan(SentinelError::Forced))
    );
    assert_eq!(control.evaluation_count(), 2);
    assert_eq!(raw_observed(&session).backing_ptr_for_test(), raw_backing);
    assert_eq!(session.raw_head().revision(), Some(Revision::new(1)));
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));

    let (cause_observation, previous_revision) =
        match session.commit(observed_update(2, [0; 3])).unwrap() {
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
    session.commit(observed_update(1, [255; 3])).unwrap();
    let first_observation = raw_observed(&session).clone();
    control.substitute_next_with(first_observation);

    assert_eq!(
        session.commit(observed_update(2, [255; 3])),
        Err(SessionUpdateError::EvidenceBindingInvariant)
    );
    assert_eq!(control.evaluation_count(), 2);
    assert_eq!(session.raw_head().revision(), Some(Revision::new(1)));
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));

    let current_observation = match session.commit(observed_update(2, [255; 3])).unwrap() {
        SessionState::Ready { current } => current.observation.clone(),
        _ => panic!("a fresh retry must bind evidence from the current observation"),
    };
    assert_eq!(control.evaluation_count(), 3);
    assert_eq!(current_observation.revision(), Revision::new(2));
    assert_shared_observation(raw_observed(&session), &current_observation);
}

#[test]
fn successful_commit_publishes_new_pair_before_retiring_old_evidence() {
    let (mut session, panic_on_next_drop, drops) = retirement_session();
    session.commit(observed_update(1, [0; 3])).unwrap();
    assert!(matches!(session.state(), SessionState::Failed { .. }));
    assert_eq!(drops.get(), 0);

    let prepared = session
        .prepare_update(observed_update(2, [255; 3]))
        .unwrap();
    panic_on_next_drop.set(true);
    let retirement = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = prepared.commit();
    }));

    assert!(retirement.is_err(), "the hostile retirement probe must run");
    assert_eq!(session.raw_head().revision(), Some(Revision::new(2)));
    let SessionState::Ready { current } = session.state() else {
        panic!("retirement must begin only after publishing the new state");
    };
    assert_eq!(current.observation.revision(), Revision::new(2));
    assert_eq!(drops.get(), 1);
}

#[test]
fn deferred_commit_returns_before_hostile_retirement_destructor_runs() {
    let (mut session, panic_on_next_drop, drops) = retirement_session();
    session.commit(observed_update(1, [0; 3])).unwrap();
    assert!(matches!(session.state(), SessionState::Failed { .. }));

    let prepared = session
        .prepare_update(observed_update(2, [255; 3]))
        .unwrap();
    panic_on_next_drop.set(true);
    let committed =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| prepared.commit_deferred()));
    let view = committed.expect("deferred commit must only park retirement");
    assert_eq!(view.raw_head().revision(), Some(Revision::new(2)));
    assert!(matches!(view.state(), SessionState::Ready { .. }));
    assert_eq!(drops.get(), 0);

    let retirement = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = session.prepare_update(observed_update(3, [255; 3]));
    }));
    assert!(
        retirement.is_err(),
        "the hostile destructor must be deferred"
    );
    assert_eq!(drops.get(), 1);
}

#[test]
fn deferred_retirement_keeps_its_exact_owner_alive_through_old_evidence_drop() {
    let alive = Rc::new(Cell::new(true));
    let events = Rc::new(RefCell::new(Vec::new()));
    let generation = Rc::new(DropOrderOwnerGeneration {
        schema: canonicalize_observation_schema(vec![SURFACE]).unwrap(),
        alive: Rc::clone(&alive),
        events: Rc::clone(&events),
    });
    let generation_weak = Rc::downgrade(&generation);
    let owner_slot = Rc::new(RefCell::new(Some(Rc::clone(&generation))));
    let mut session = Session::new(
        STREAM,
        DropOrderPlan {
            generation: generation_weak.clone(),
            owner_slot: Rc::clone(&owner_slot),
        },
    );

    session
        .prepare_update(observed_update(1, [255; 3]))
        .unwrap()
        .commit();
    assert!(events.borrow().is_empty());

    *owner_slot.borrow_mut() = Some(Rc::clone(&generation));
    let prepared = session
        .prepare_update(observed_update(2, [255; 3]))
        .unwrap();
    drop(generation);
    assert!(alive.get());

    let view = prepared.commit_deferred();
    assert!(events.borrow().is_empty());
    assert!(alive.get());
    assert_eq!(view.raw_head().revision(), Some(Revision::new(2)));

    assert!(matches!(
        session.prepare_update(observed_update(3, [255; 3])),
        Err(SessionUpdateError::OwnerExpired)
    ));

    assert!(!alive.get());
    assert!(generation_weak.upgrade().is_none());
    assert_eq!(&*events.borrow(), &["evidence", "owner"]);
}

#[test]
fn dropping_prepared_transition_retires_only_pending_evidence() {
    let (mut session, _, drops) = retirement_session();
    session.commit(observed_update(1, [0; 3])).unwrap();

    let prepared = session
        .prepare_update(observed_update(2, [255; 3]))
        .unwrap();
    drop(prepared);

    assert_eq!(drops.get(), 1);
    assert_eq!(session.raw_head().revision(), Some(Revision::new(1)));
    let SessionState::Failed { cause, previous } = session.state() else {
        panic!("aborting prepare must preserve the committed violation");
    };
    assert_eq!(cause.observation.revision(), Revision::new(1));
    assert!(previous.is_none());
}

#[test]
fn prepared_disposition_borrows_the_exact_uncommitted_outcome() {
    let (mut session, _, _) = session();
    session.commit(observed_update(1, [255; 3])).unwrap();

    let idempotent = session
        .prepare_update(observed_update(1, [255; 3]))
        .unwrap();
    let PreparedSessionDispositionV1::Idempotent { raw_head, state } = idempotent.disposition()
    else {
        panic!("same observation must prepare an idempotent disposition");
    };
    assert_eq!(raw_head.revision(), Some(Revision::new(1)));
    assert!(matches!(state, SessionState::Ready { .. }));
    drop(idempotent);

    let unknown = session
        .prepare_unknown(Revision::new(2), UnknownReasonId::new(7))
        .unwrap();
    let PreparedSessionDispositionV1::Unknown(value) = unknown.disposition() else {
        panic!("unknown payload must stay typed before commit");
    };
    assert_eq!(value.revision(), Revision::new(2));
    drop(unknown);

    let verified = session
        .prepare_update(observed_update(2, [255; 3]))
        .unwrap();
    let PreparedSessionDispositionV1::Verified(value) = verified.disposition() else {
        panic!("verified outcome must stay typed before commit");
    };
    assert_eq!(value.observation().revision(), Revision::new(2));
    drop(verified);

    let violation = session.prepare_update(observed_update(2, [0; 3])).unwrap();
    let PreparedSessionDispositionV1::Violation(value) = violation.disposition() else {
        panic!("violation outcome must stay typed before commit");
    };
    assert_eq!(value.observation().revision(), Revision::new(2));
    drop(violation);
}

#[test]
fn dropping_prepared_observed_transition_preserves_committed_state_and_retry() {
    let (mut session, control, _) = session();
    session.commit(observed_update(1, [255; 3])).unwrap();
    let committed_backing = raw_observed(&session).backing_ptr_for_test();

    let prepared = session.prepare_update(observed_update(2, [0; 3])).unwrap();
    assert_eq!(control.evaluation_count(), 2);
    drop(prepared);

    assert_eq!(
        raw_observed(&session).backing_ptr_for_test(),
        committed_backing
    );
    assert_eq!(session.raw_head().revision(), Some(Revision::new(1)));
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));
    assert!(matches!(session.state(), SessionState::Ready { .. }));

    let prepared = session.prepare_update(observed_update(2, [0; 3])).unwrap();
    let (committed, allocations) = crate::test_support::measured_allocations(|| prepared.commit());
    assert_eq!(
        allocations, 0,
        "lifecycle commit must only move prepared values"
    );
    let SessionState::Failed { cause, previous } = committed.state() else {
        panic!("the retried violation must commit after the aborted prepare");
    };
    assert_eq!(cause.observation.revision(), Revision::new(2));
    assert_eq!(
        previous.as_ref().map(|value| value.observation.revision()),
        Some(Revision::new(1)),
    );
    assert_eq!(control.evaluation_count(), 3);
}

#[test]
fn dropping_prepared_unknown_does_not_move_previous_verified_evidence() {
    let (mut session, _, _) = session();
    session.commit(observed_update(1, [255; 3])).unwrap();
    let committed_backing = raw_observed(&session).backing_ptr_for_test();

    let prepared = session
        .prepare_unknown(Revision::new(2), UnknownReasonId::new(9))
        .unwrap();
    drop(prepared);

    assert_eq!(
        raw_observed(&session).backing_ptr_for_test(),
        committed_backing
    );
    assert!(matches!(session.state(), SessionState::Ready { .. }));
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));

    let committed = session
        .prepare_unknown(Revision::new(2), UnknownReasonId::new(9))
        .unwrap()
        .commit();
    assert!(matches!(committed.state(), SessionState::Stale { .. }));
    assert_eq!(committed.raw_head().revision(), Some(Revision::new(2)));
}

#[test]
fn abort_drops_pending_evidence_before_releasing_its_exact_owner_generation() {
    let alive = Rc::new(Cell::new(true));
    let events = Rc::new(RefCell::new(Vec::new()));
    let generation = Rc::new(DropOrderOwnerGeneration {
        schema: canonicalize_observation_schema(vec![SURFACE]).unwrap(),
        alive: Rc::clone(&alive),
        events: Rc::clone(&events),
    });
    let generation_weak = Rc::downgrade(&generation);
    let owner_slot = Rc::new(RefCell::new(Some(Rc::clone(&generation))));
    let mut session = Session::new(
        STREAM,
        DropOrderPlan {
            generation: generation_weak.clone(),
            owner_slot,
        },
    );
    drop(generation);

    let prepared = session
        .prepare_update(observed_update(1, [255; 3]))
        .unwrap();
    assert!(alive.get());
    assert!(generation_weak.upgrade().is_some());
    drop(prepared);

    assert!(!alive.get());
    assert!(generation_weak.upgrade().is_none());
    assert_eq!(&*events.borrow(), &["evidence", "owner"]);
    assert_eq!(session.raw_head(), ObservationHeadViewV1::Empty);
    assert!(matches!(session.state(), SessionState::Waiting));
}

#[test]
fn reentrant_owner_replacement_finishes_on_its_pinned_generation_then_expires() {
    let first_generation = Rc::new(ReplacingOwnerGeneration {
        schema: canonicalize_observation_schema(vec![SURFACE]).unwrap(),
    });
    let first_generation_weak = Rc::downgrade(&first_generation);
    let owner_slot = Rc::new(RefCell::new(Some(Rc::clone(&first_generation))));
    let evaluations = Rc::new(Cell::new(0));
    let mut session = Session::new(
        STREAM,
        ReplacingOwnerPlan {
            generation: first_generation_weak.clone(),
            owner_slot: Rc::clone(&owner_slot),
            evaluations: Rc::clone(&evaluations),
        },
    );
    drop(first_generation);

    let prepared = session
        .prepare_update(observed_update(1, [255; 3]))
        .unwrap();
    assert_eq!(evaluations.get(), 1);
    assert!(
        first_generation_weak.upgrade().is_some(),
        "the prepared transition must retain the exact evaluated generation",
    );
    let committed = prepared.commit();
    assert!(
        first_generation_weak.upgrade().is_none(),
        "commit must release its generation only after lifecycle publication",
    );
    let SessionState::Ready { current } = committed.state() else {
        panic!("the transaction pinned before replacement must commit");
    };
    assert_eq!(current.observation.revision(), Revision::new(1));
    assert!(owner_slot.borrow().is_some());

    assert_eq!(
        session.commit(observed_update(2, [0; 3])),
        Err(SessionUpdateError::OwnerExpired),
    );
    assert_eq!(evaluations.get(), 1);
    assert_eq!(session.raw_head().revision(), Some(Revision::new(1)));
    assert_eq!(verified_revision(session.state()), Some(Revision::new(1)));
}

#[test]
fn session_source_contains_one_linear_prepared_owner_and_no_immediate_authority() {
    let source = include_str!("session.rs");
    assert_eq!(source.matches("pub(crate) fn prepare_update(").count(), 1);
    assert_eq!(
        source
            .matches("pub(crate) struct PreparedSessionTransition<")
            .count(),
        1,
    );
    for forbidden in [
        "pub(crate) fn update(",
        "pub(crate) fn update_unknown(",
        "pub(crate) fn update_schema_ordered",
        "fn apply_prepared_update",
        "PointSupportSessionV1",
        "PointSupportSessionStateV1",
        "PointSupportSessionUpdateErrorV1",
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
