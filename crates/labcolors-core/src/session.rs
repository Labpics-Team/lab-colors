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
    CanonicalObservationSchemaV1, ObservationError, ObservationHeadViewV1, ObservationOwnerV1,
    ObservationPayloadInput, ObservationStreamId, ObservationUpdateInput,
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
    _private: (),
}

impl SessionObservationBindingPermitV1 {
    const fn mint() -> Self {
        Self { _private: () }
    }

    #[cfg(test)]
    pub(crate) const fn for_test() -> Self {
        Self { _private: () }
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
        permit: SessionObservationBindingPermitV1,
    ) -> Result<SessionDecision<Self::Verified, Self::Violation>, Self::Error>;
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

/// Linear, fully evaluated Session transition that has not been published yet.
///
/// Dropping this value discards only prospective data. Committing consumes the
/// sole mutable borrow and publishes raw head and lifecycle with moves after
/// every fallible operation has completed.
#[must_use = "commit the prepared transition or drop it intentionally"]
pub(crate) struct PreparedSessionTransition<'session, Plan: SessionPlanV1> {
    raw_head: &'session mut SessionObservationHeadV1,
    state: &'session mut SessionState<Plan::Verified, Plan::Violation>,
    pending: PendingSessionTransition<Plan::Verified, Plan::Violation>,
    // Rust drops fields in declaration order. Keep the lease last so abort
    // destroys every prospective evidence value while its generation is live.
    owner: Plan::OwnerLease,
}

impl<'session, Plan: SessionPlanV1> PreparedSessionTransition<'session, Plan> {
    /// Publish one already admitted and evaluated lifecycle transition.
    ///
    /// This function has no failure return and performs no admission,
    /// evaluation or allocation. It does not claim that an external sink has
    /// accepted any output.
    pub(crate) fn commit(self) -> SessionView<'session, Plan> {
        let Self {
            raw_head,
            state,
            pending,
            owner,
        } = self;

        match pending {
            PendingSessionTransition::Idempotent => {}
            PendingSessionTransition::Unknown(unknown) => {
                let next_state = match take_last_verified(state) {
                    Some(previous) => SessionState::Stale { previous },
                    None => SessionState::Waiting,
                };
                *raw_head = SessionObservationHeadV1::Unknown(unknown);
                *state = next_state;
            }
            PendingSessionTransition::Observed {
                raw_observation,
                decision,
            } => {
                let previous = take_last_verified(state);
                let next_state = match decision {
                    SessionDecision::Verified(current) => SessionState::Ready { current },
                    SessionDecision::Violation(cause) => SessionState::Failed { cause, previous },
                };
                *raw_head = SessionObservationHeadV1::Observed(raw_observation);
                *state = next_state;
            }
        }

        // The exact generation remains pinned through both lifecycle moves.
        drop(owner);
        SessionView {
            raw_head: raw_head.observation_head(),
            state,
        }
    }
}

/// The only production owner of revision admission and evaluator lifecycle.
/// `Plan` is monomorphized; there is no plan enum, dynamic dispatch or adapter.
/// A plan may keep only a weak reference to its compiled owner generation;
/// every update pins that exact generation before admission and releases it
/// after commit or rollback.
#[derive(Debug)]
pub(crate) struct Session<Plan: SessionPlanV1> {
    stream: ObservationStreamId,
    plan: Plan,
    raw_head: SessionObservationHeadV1,
    state: SessionState<Plan::Verified, Plan::Violation>,
}

impl<Plan: SessionPlanV1> Session<Plan> {
    pub(crate) fn new(stream: ObservationStreamId, plan: Plan) -> Self {
        Self {
            stream,
            plan,
            raw_head: SessionObservationHeadV1::Empty,
            state: SessionState::Waiting,
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
        let owner = self
            .plan
            .try_acquire_owner()
            .ok_or(SessionUpdateError::OwnerExpired)?;
        let schema = self.plan.observation_schema(&owner);
        let prepared = prepare_observation(&mut self.raw_head, self.stream, schema, update)
            .map_err(SessionUpdateError::Observation)?;

        prepare_session_transition(&mut self.plan, &mut self.state, owner, prepared)
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
        let owner = self
            .plan
            .try_acquire_owner()
            .ok_or(SessionUpdateError::OwnerExpired)?;
        let schema = self.plan.observation_schema(&owner);
        let prepared = prepare_schema_ordered_observation(
            &mut self.raw_head,
            self.stream,
            schema,
            revision,
            source,
            order_scratch,
        )
        .map_err(SessionUpdateError::Observation)?;

        prepare_session_transition(&mut self.plan, &mut self.state, owner, prepared)
    }
}

fn prepare_session_transition<'session, Plan: SessionPlanV1>(
    plan: &mut Plan,
    state: &'session mut SessionState<Plan::Verified, Plan::Violation>,
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
            let decision = plan
                .evaluate(
                    &owner,
                    observation,
                    SessionObservationBindingPermitV1::mint(),
                )
                .map_err(SessionUpdateError::Plan)?;
            if !decision.observation().is_same_binding_as(&raw_observation) {
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
        pending,
        owner,
    })
}

/// Move exactly one retained verified witness out of the old closed owner.
fn take_last_verified<Verified, Violation>(
    state: &mut SessionState<Verified, Violation>,
) -> Option<Verified> {
    match mem::replace(state, SessionState::Waiting) {
        SessionState::Waiting => None,
        SessionState::Ready { current } => Some(current),
        SessionState::Stale { previous } => Some(previous),
        SessionState::Failed { previous, .. } => previous,
    }
}
