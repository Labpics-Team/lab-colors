//! Sole revision-bound runtime lifecycle for compiled point programs.
//!
//! [`Session`] owns the one concrete raw observation head and the one
//! evaluator lifecycle. A plan supplies only its canonical observation schema
//! and consuming evaluation; it cannot admit updates or commit lifecycle
//! state. The plan type is sealed and statically dispatched, so sharing this
//! lifecycle across compiled plans adds neither a runtime tag nor a trait
//! object.

use std::mem;

use crate::observation::{
    CanonicalObservationSchemaV1, ObservationArenaPoolV1, ObservationError, ObservationHeadViewV1,
    ObservationOwnerV1, ObservationPayloadInput, ObservationStreamId, ObservationUpdateInput,
    PreparedObservationUpdateV1, Revision, RevisionBoundObservationV1, RevisionBoundUnknownV1,
    SchemaOrderedScenarioSourceV1, UnknownReasonId, prepare_observation,
    prepare_schema_ordered_observation,
};

/// Crate-private sealing prevents an additional runtime owner from being
/// smuggled in through a public extension point.
pub(crate) mod private {
    pub(crate) trait PlanSealed {}
    pub(crate) trait EvidenceSealed {}
}

/// Linear authority to revision-bind one admitted observation to evaluator
/// evidence. Safe construction remains exclusive to this module.
pub(crate) struct SessionObservationBindingPermitV1 {
    stream: ObservationStreamId,
    revision: Revision,
}

impl SessionObservationBindingPermitV1 {
    const fn mint(observation: &RevisionBoundObservationV1) -> Self {
        Self {
            stream: observation.stream(),
            revision: observation.revision(),
        }
    }

    pub(crate) const fn stream(&self) -> ObservationStreamId {
        self.stream
    }

    pub(crate) const fn revision(&self) -> Revision {
        self.revision
    }
}

/// Complete result of evaluating one admitted observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionDecision<Verified, Violation> {
    Verified(Verified),
    Violation(Violation),
}

pub(crate) trait SessionEvidenceV1: private::EvidenceSealed {
    fn observation(&self) -> &RevisionBoundObservationV1;
}

impl<Verified, Violation> SessionDecision<Verified, Violation>
where
    Verified: SessionEvidenceV1,
    Violation: SessionEvidenceV1,
{
    fn observation(&self) -> &RevisionBoundObservationV1 {
        match self {
            Self::Verified(evidence) => evidence.observation(),
            Self::Violation(evidence) => evidence.observation(),
        }
    }
}

/// A compiled, statically dispatched evaluator used by the sole [`Session`]
/// lifecycle. Implementations own their per-Session scratch directly.
pub(crate) trait SessionPlanV1: private::PlanSealed {
    /// One owned lease over the exact compiled owner generation used by an
    /// update. Acquiring it is the first operation in the transaction, so an
    /// expired plan cannot admit raw state or reach physical execution.
    type OwnerLease;
    type Verified: SessionEvidenceV1;
    type Violation: SessionEvidenceV1;
    type Error;

    fn try_acquire_owner(&self) -> Option<Self::OwnerLease>;

    /// Return the canonical schema reached through the same owner lease that
    /// will authorize evaluation. Self-owned plans may return their own schema;
    /// weakly bound plans must derive it from the pinned generation.
    fn observation_schema<'a>(
        &'a self,
        owner: &'a Self::OwnerLease,
    ) -> &'a CanonicalObservationSchemaV1;

    fn evaluate(
        &mut self,
        owner: &Self::OwnerLease,
        observation: RevisionBoundObservationV1,
        previous: Option<&Self::Verified>,
        permit: SessionObservationBindingPermitV1,
    ) -> Result<SessionDecision<Self::Verified, Self::Violation>, Self::Error>;

    /// Возвращает хранилище retired evidence в точный план этой Session.
    /// План без reusable evidence storage сохраняет обычный owning drop.
    fn retire_verified(&mut self, evidence: Self::Verified) {
        drop(evidence);
    }

    /// Симметричный retirement несовместимой ветви violation evidence.
    fn retire_violation(&mut self, evidence: Self::Violation) {
        drop(evidence);
    }
}

/// Evaluator lifecycle. The current raw payload is deliberately not embedded
/// here: `Unknown` carries no evidence, while `Stale` retains at most one
/// previous verified witness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionState<Verified, Violation> {
    Waiting,
    Ready {
        current: Verified,
    },
    Stale {
        previous: Verified,
    },
    Failed {
        cause: Violation,
        previous: Option<Verified>,
    },
}

impl<Verified, Violation> SessionState<Verified, Violation> {
    pub(crate) fn last_verified(&self) -> Option<&Verified> {
        match self {
            Self::Waiting => None,
            Self::Ready { current } => Some(current),
            Self::Stale { previous } => Some(previous),
            Self::Failed { previous, .. } => previous.as_ref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SessionObservationHeadV1 {
    Empty,
    Unknown(RevisionBoundUnknownV1),
    Observed(RevisionBoundObservationV1),
}

impl ObservationOwnerV1 for SessionObservationHeadV1 {
    fn observation_head(&self) -> ObservationHeadViewV1<'_> {
        match self {
            Self::Empty => ObservationHeadViewV1::Empty,
            Self::Unknown(unknown) => ObservationHeadViewV1::Unknown(unknown),
            Self::Observed(observation) => ObservationHeadViewV1::Observed(observation),
        }
    }
}

/// An update failed before either raw-head or lifecycle commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SessionUpdateError<PlanError> {
    OwnerExpired,
    Observation(ObservationError),
    Plan(PlanError),
    EvidenceBindingInvariant,
}

type SessionPrepareResult<'session, Plan> = Result<
    PreparedSessionTransition<'session, Plan>,
    SessionUpdateError<<Plan as SessionPlanV1>::Error>,
>;

/// Immutable projection of one committed raw-head/lifecycle pair.
///
/// A prepared transition cannot construct this view until it is consumed by
/// [`PreparedSessionTransition::commit`].
pub(crate) struct SessionView<'session, Plan: SessionPlanV1> {
    raw_head: ObservationHeadViewV1<'session>,
    state: &'session SessionState<Plan::Verified, Plan::Violation>,
}

impl<Plan: SessionPlanV1> Copy for SessionView<'_, Plan> {}

impl<Plan: SessionPlanV1> Clone for SessionView<'_, Plan> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'session, Plan: SessionPlanV1> SessionView<'session, Plan> {
    pub(crate) const fn raw_head(self) -> ObservationHeadViewV1<'session> {
        self.raw_head
    }

    pub(crate) const fn state(self) -> &'session SessionState<Plan::Verified, Plan::Violation> {
        self.state
    }
}

enum PendingSessionTransition<Verified, Violation> {
    Idempotent,
    Unknown(RevisionBoundUnknownV1),
    Observed {
        raw_observation: RevisionBoundObservationV1,
        decision: SessionDecision<Verified, Violation>,
    },
}

/// Заимствованный prospective lifecycle для imperative shell до commit.
///
/// Generic Session открывает только evidence и provenance ревизии; она не
/// знает, будет ли вызывающий код render-ить, lint-ить, сохранять или
/// отбрасывать результат.
pub(crate) enum PreparedSessionDispositionV1<'a, Plan: SessionPlanV1> {
    Idempotent {
        raw_head: ObservationHeadViewV1<'a>,
        state: &'a SessionState<Plan::Verified, Plan::Violation>,
    },
    Unknown(&'a RevisionBoundUnknownV1),
    Verified(&'a Plan::Verified),
    Violation(&'a Plan::Violation),
}

struct DisplacedSessionState<Verified, Violation> {
    last_verified: Option<Verified>,
    discarded_violation: Option<Violation>,
}

/// Вытесненные значения уже опубликованного перехода, чьё уничтожение
/// вызывающий shell обязан отложить за границу физического commit.
///
/// Поля намеренно закрыты. Значение служит только линейным retirement-bundle;
/// owner объявлен последним и потому переживает raw/evidence при уничтожении.
pub(crate) struct DeferredSessionRetirement<Plan: SessionPlanV1> {
    retired_raw_head: Option<SessionObservationHeadV1>,
    retired_verified: Option<Plan::Verified>,
    retired_violation: Option<Plan::Violation>,
    displaced_placeholder: SessionState<Plan::Verified, Plan::Violation>,
    _owner: Plan::OwnerLease,
}

impl<Plan: SessionPlanV1> DeferredSessionRetirement<Plan> {
    /// Возвращает reusable storage до уничтожения raw provenance и exact owner.
    fn retire_into(mut self, plan: &mut Plan) {
        // Owner остаётся внутри `self` до завершения всех evidence/raw
        // деструкторов. При unwind порядок полей всё равно освобождает exact
        // owner последним и не зависит от порядка локальных переменных.
        if let Some(verified) = self.retired_verified.take() {
            plan.retire_verified(verified);
        }
        if let Some(violation) = self.retired_violation.take() {
            plan.retire_violation(violation);
        }
        drop(mem::replace(
            &mut self.displaced_placeholder,
            SessionState::Waiting,
        ));
        drop(self.retired_raw_head.take());
    }
}

/// Abort-guard: prospective evidence возвращает storage тому же Plan, пока
/// exact owner lease ещё жив. Сам Prepared token поэтому не нуждается в Drop.
struct PendingSessionTransitionGuard<'session, Plan: SessionPlanV1> {
    plan: &'session mut Plan,
    pending: Option<PendingSessionTransition<Plan::Verified, Plan::Violation>>,
    owner: Option<Plan::OwnerLease>,
}

impl<Plan: SessionPlanV1> PendingSessionTransitionGuard<'_, Plan> {
    fn take_parts(
        &mut self,
    ) -> (
        PendingSessionTransition<Plan::Verified, Plan::Violation>,
        Plan::OwnerLease,
    ) {
        let pending = self
            .pending
            .take()
            .unwrap_or_else(|| unreachable!("prepared transition owns one pending value"));
        let owner = self
            .owner
            .take()
            .unwrap_or_else(|| unreachable!("prepared transition owns one generation lease"));
        (pending, owner)
    }
}

impl<Plan: SessionPlanV1> Drop for PendingSessionTransitionGuard<'_, Plan> {
    fn drop(&mut self) {
        if let Some(pending) = self.pending.take() {
            retire_pending_transition(self.plan, pending);
        }
        drop(self.owner.take());
    }
}

/// Линейный, полностью вычисленный и ещё не опубликованный переход Session.
///
/// Drop отбрасывает только prospective data. Commit поглощает единственное
/// mutable-заимствование и публикует raw head и lifecycle перемещениями после
/// завершения всех fallible-операций.
#[must_use = "commit the prepared transition or drop it intentionally"]
pub(crate) struct PreparedSessionTransition<'session, Plan: SessionPlanV1> {
    raw_head: &'session mut SessionObservationHeadV1,
    state: &'session mut SessionState<Plan::Verified, Plan::Violation>,
    deferred_retirement: &'session mut Option<DeferredSessionRetirement<Plan>>,
    guard: PendingSessionTransitionGuard<'session, Plan>,
}

impl<'session, Plan: SessionPlanV1> PreparedSessionTransition<'session, Plan> {
    /// Возвращает fully evaluated prospective disposition без публикации.
    pub(crate) fn disposition(&self) -> PreparedSessionDispositionV1<'_, Plan> {
        match self
            .guard
            .pending
            .as_ref()
            .unwrap_or_else(|| unreachable!("prepared transition has not been consumed"))
        {
            PendingSessionTransition::Idempotent => PreparedSessionDispositionV1::Idempotent {
                raw_head: self.raw_head.observation_head(),
                state: self.state,
            },
            PendingSessionTransition::Unknown(unknown) => {
                PreparedSessionDispositionV1::Unknown(unknown)
            }
            PendingSessionTransition::Observed { decision, .. } => match decision {
                SessionDecision::Verified(verified) => {
                    PreparedSessionDispositionV1::Verified(verified)
                }
                SessionDecision::Violation(violation) => {
                    PreparedSessionDispositionV1::Violation(violation)
                }
            },
        }
    }

    /// Публикует один уже допущенный и вычисленный lifecycle-переход.
    ///
    /// Функция не возвращает ошибку и не выполняет admission, evaluation или
    /// allocation. Она не утверждает, что внешний sink принял output.
    pub(crate) fn commit(self) -> SessionView<'session, Plan> {
        let Self {
            raw_head,
            state,
            deferred_retirement: _,
            mut guard,
        } = self;
        // Token получен только после drain и эксклюзивно заимствует retirement
        // slot до commit, поэтому повторно проверить или заполнить его нельзя.
        let (pending, owner) = guard.take_parts();
        let (view, retirement) = publish_session_transition(raw_head, state, pending, owner);
        retirement.retire_into(guard.plan);
        drop(guard);
        view
    }

    /// Публикует пару raw-head/lifecycle и паркует всё вытесненное внутри той
    /// же Session без запуска пользовательских деструкторов.
    ///
    /// После входа в эту функцию выполняются только перемещения и записи в
    /// уже существующие слоты. Это вариант для imperative shell, который уже
    /// установил внешний снимок и обязан отложить retirement до следующего
    /// pre-install участка.
    pub(crate) fn commit_deferred(self) -> SessionView<'session, Plan> {
        let Self {
            raw_head,
            state,
            deferred_retirement,
            mut guard,
        } = self;
        // Та же эксклюзивная vacant-slot гарантия, что и у обычного commit.
        let (pending, owner) = guard.take_parts();
        let (view, retirement) = publish_session_transition(raw_head, state, pending, owner);
        *deferred_retirement = Some(retirement);
        drop(guard);
        view
    }
}

fn publish_session_transition<'session, Plan: SessionPlanV1>(
    raw_head: &'session mut SessionObservationHeadV1,
    state: &'session mut SessionState<Plan::Verified, Plan::Violation>,
    pending: PendingSessionTransition<Plan::Verified, Plan::Violation>,
    owner: Plan::OwnerLease,
) -> (SessionView<'session, Plan>, DeferredSessionRetirement<Plan>) {
    let (retired_raw_head, retired_verified, retired_violation, displaced_placeholder) =
        match pending {
            PendingSessionTransition::Idempotent => (None, None, None, SessionState::Waiting),
            PendingSessionTransition::Unknown(unknown) => {
                let DisplacedSessionState {
                    last_verified,
                    discarded_violation,
                } = displace_session_state(state);
                let next_state = match last_verified {
                    Some(previous) => SessionState::Stale { previous },
                    None => SessionState::Waiting,
                };
                let retired_raw_head =
                    mem::replace(raw_head, SessionObservationHeadV1::Unknown(unknown));
                let displaced_placeholder = mem::replace(state, next_state);
                (
                    Some(retired_raw_head),
                    None,
                    discarded_violation,
                    displaced_placeholder,
                )
            }
            PendingSessionTransition::Observed {
                raw_observation,
                decision,
            } => {
                let DisplacedSessionState {
                    last_verified,
                    discarded_violation,
                } = displace_session_state(state);
                let (next_state, retired_verified) = match decision {
                    SessionDecision::Verified(current) => {
                        (SessionState::Ready { current }, last_verified)
                    }
                    SessionDecision::Violation(cause) => (
                        SessionState::Failed {
                            cause,
                            previous: last_verified,
                        },
                        None,
                    ),
                };
                let retired_raw_head = mem::replace(
                    raw_head,
                    SessionObservationHeadV1::Observed(raw_observation),
                );
                let displaced_placeholder = mem::replace(state, next_state);
                (
                    Some(retired_raw_head),
                    retired_verified,
                    discarded_violation,
                    displaced_placeholder,
                )
            }
        };

    let view = SessionView {
        raw_head: raw_head.observation_head(),
        state,
    };
    let retirement = DeferredSessionRetirement {
        retired_raw_head,
        retired_verified,
        retired_violation,
        displaced_placeholder,
        _owner: owner,
    };
    (view, retirement)
}

/// The only production owner of revision admission and evaluator lifecycle.
/// `Plan` is monomorphized; there is no plan enum, dynamic dispatch or adapter.
/// A plan may keep only a weak reference to its compiled owner generation;
/// every update pins that exact generation before admission and releases it
/// after commit or rollback.
pub(crate) struct Session<Plan: SessionPlanV1> {
    stream: ObservationStreamId,
    observation_arenas: ObservationArenaPoolV1,
    raw_head: SessionObservationHeadV1,
    state: SessionState<Plan::Verified, Plan::Violation>,
    deferred_retirement: Option<DeferredSessionRetirement<Plan>>,
    // Plan может владеть большим executable artifact storage. Evidence и
    // retirement должны освободиться раньше него, поэтому Plan всегда последний.
    plan: Plan,
}

impl<Plan: SessionPlanV1> Session<Plan> {
    pub(crate) fn new(stream: ObservationStreamId, plan: Plan) -> Self {
        let owner = plan
            .try_acquire_owner()
            .unwrap_or_else(|| unreachable!("a Session is created from one live compiled owner"));
        let observation_arenas = ObservationArenaPoolV1::new(plan.observation_schema(&owner));
        drop(owner);
        Self {
            stream,
            observation_arenas,
            raw_head: SessionObservationHeadV1::Empty,
            state: SessionState::Waiting,
            deferred_retirement: None,
            plan,
        }
    }

    /// Retirement прошлого terminal install завершается до owner acquisition,
    /// admission, evaluator work и новой sink mutation.
    fn drain_deferred_retirement(&mut self) {
        if let Some(retirement) = self.deferred_retirement.take() {
            retirement.retire_into(&mut self.plan);
        }
    }

    pub(crate) const fn state(&self) -> &SessionState<Plan::Verified, Plan::Violation> {
        &self.state
    }

    pub(crate) fn raw_head(&self) -> ObservationHeadViewV1<'_> {
        self.raw_head.observation_head()
    }

    pub(crate) fn view(&self) -> SessionView<'_, Plan> {
        SessionView {
            raw_head: self.raw_head(),
            state: self.state(),
        }
    }

    pub(crate) const fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Prepare and evaluate one update without committing it. Admission and
    /// plan errors leave both the concrete raw head and lifecycle untouched;
    /// dropping the returned transition has the same property.
    /// Plan-local scratch may have been overwritten by a failed evaluation,
    /// but it is not observable lifecycle state and every evaluation must
    /// completely initialize the scratch it consumes.
    pub(crate) fn prepare_update(
        &mut self,
        update: ObservationUpdateInput,
    ) -> SessionPrepareResult<'_, Plan> {
        self.drain_deferred_retirement();
        let owner = self
            .plan
            .try_acquire_owner()
            .ok_or(SessionUpdateError::OwnerExpired)?;
        let schema = self.plan.observation_schema(&owner);
        let prepared = prepare_observation(
            &mut self.raw_head,
            &mut self.observation_arenas,
            self.stream,
            schema,
            update,
        )
        .map_err(SessionUpdateError::Observation)?;

        prepare_session_transition(
            &mut self.plan,
            &mut self.state,
            &mut self.deferred_retirement,
            owner,
            prepared,
        )
    }

    /// Stream-affine `Unknown` admission without re-exporting or duplicating
    /// the Session-owned stream identity at a package boundary.
    pub(crate) fn prepare_unknown(
        &mut self,
        revision: Revision,
        reason: UnknownReasonId,
    ) -> SessionPrepareResult<'_, Plan> {
        self.prepare_update(ObservationUpdateInput {
            stream: self.stream,
            revision,
            payload: ObservationPayloadInput::Unknown(reason),
        })
    }

    /// Package hot path for already schema-ordered point-sRGB8 scenarios.
    /// It shares the exact prepared lifecycle transition below without
    /// constructing keyed surface bindings or a second raw observation owner.
    pub(crate) fn prepare_schema_ordered<Source: SchemaOrderedScenarioSourceV1>(
        &mut self,
        revision: Revision,
        source: &Source,
        order_scratch: &mut Vec<usize>,
    ) -> SessionPrepareResult<'_, Plan> {
        self.drain_deferred_retirement();
        let owner = self
            .plan
            .try_acquire_owner()
            .ok_or(SessionUpdateError::OwnerExpired)?;
        let schema = self.plan.observation_schema(&owner);
        let prepared = prepare_schema_ordered_observation(
            &mut self.raw_head,
            &mut self.observation_arenas,
            self.stream,
            schema,
            revision,
            source,
            order_scratch,
        )
        .map_err(SessionUpdateError::Observation)?;

        prepare_session_transition(
            &mut self.plan,
            &mut self.state,
            &mut self.deferred_retirement,
            owner,
            prepared,
        )
    }
}

fn prepare_session_transition<'session, Plan: SessionPlanV1>(
    plan: &'session mut Plan,
    state: &'session mut SessionState<Plan::Verified, Plan::Violation>,
    deferred_retirement: &'session mut Option<DeferredSessionRetirement<Plan>>,
    owner: Plan::OwnerLease,
    prepared: PreparedObservationUpdateV1<'session, SessionObservationHeadV1>,
) -> SessionPrepareResult<'session, Plan> {
    let (raw_head, pending) = match prepared {
        PreparedObservationUpdateV1::Idempotent(prepared) => {
            (prepared.into_owner(), PendingSessionTransition::Idempotent)
        }
        PreparedObservationUpdateV1::Unknown(prepared) => {
            let (raw_head, unknown) = prepared.into_parts();
            (raw_head, PendingSessionTransition::Unknown(unknown))
        }
        PreparedObservationUpdateV1::Observed(prepared) => {
            // Clone only the small Rc-backed observation handle. Both the
            // committed raw head and returned evidence then share the exact
            // immutable observation backing.
            let (raw_head, observation) = prepared.into_parts();
            let raw_observation = observation.clone();
            let permit = SessionObservationBindingPermitV1::mint(&observation);
            let decision = plan
                .evaluate(&owner, observation, state.last_verified(), permit)
                .map_err(SessionUpdateError::Plan)?;
            if !decision.observation().is_same_binding_as(&raw_observation) {
                retire_session_decision(plan, decision);
                return Err(SessionUpdateError::EvidenceBindingInvariant);
            }
            (
                raw_head,
                PendingSessionTransition::Observed {
                    raw_observation,
                    decision,
                },
            )
        }
    };

    Ok(PreparedSessionTransition {
        raw_head,
        state,
        deferred_retirement,
        guard: PendingSessionTransitionGuard {
            plan,
            pending: Some(pending),
            owner: Some(owner),
        },
    })
}

fn retire_pending_transition<Plan: SessionPlanV1>(
    plan: &mut Plan,
    pending: PendingSessionTransition<Plan::Verified, Plan::Violation>,
) {
    match pending {
        PendingSessionTransition::Idempotent | PendingSessionTransition::Unknown(_) => {}
        PendingSessionTransition::Observed {
            raw_observation,
            decision,
        } => {
            retire_session_decision(plan, decision);
            drop(raw_observation);
        }
    }
}

fn retire_session_decision<Plan: SessionPlanV1>(
    plan: &mut Plan,
    decision: SessionDecision<Plan::Verified, Plan::Violation>,
) {
    match decision {
        SessionDecision::Verified(verified) => plan.retire_verified(verified),
        SessionDecision::Violation(violation) => plan.retire_violation(violation),
    }
}

/// Вытесняет старый lifecycle, не уничтожая evidence до установки следующей
/// пары raw-head/state.
fn displace_session_state<Verified, Violation>(
    state: &mut SessionState<Verified, Violation>,
) -> DisplacedSessionState<Verified, Violation> {
    match mem::replace(state, SessionState::Waiting) {
        SessionState::Waiting => DisplacedSessionState {
            last_verified: None,
            discarded_violation: None,
        },
        SessionState::Ready { current } => DisplacedSessionState {
            last_verified: Some(current),
            discarded_violation: None,
        },
        SessionState::Stale { previous } => DisplacedSessionState {
            last_verified: Some(previous),
            discarded_violation: None,
        },
        SessionState::Failed { cause, previous } => DisplacedSessionState {
            last_verified: previous,
            discarded_violation: Some(cause),
        },
    }
}
