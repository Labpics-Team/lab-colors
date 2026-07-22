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
    ObservationStreamId, ObservationUpdateInput, PreparedObservationUpdateV1, Revision,
    RevisionBoundObservationV1, RevisionBoundUnknownV1, SchemaOrderedScenarioSourceV1,
    prepare_observation, prepare_schema_ordered_observation,
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

    fn observation_schema(&self) -> &CanonicalObservationSchemaV1;

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

type SessionUpdateResult<'session, Plan> = Result<
    &'session SessionState<<Plan as SessionPlanV1>::Verified, <Plan as SessionPlanV1>::Violation>,
    SessionUpdateError<<Plan as SessionPlanV1>::Error>,
>;

/// The only production owner of revision admission and evaluator lifecycle.
/// `Plan` is monomorphized; there is no plan enum, dynamic dispatch or adapter.
/// A plan may keep only a weak reference to its compiled owner generation;
/// every update pins that exact generation before admission and releases it
/// after commit or rollback.
#[derive(Debug)]
pub(crate) struct Session<Plan: SessionPlanV1> {
    stream: ObservationStreamId,
    schema: CanonicalObservationSchemaV1,
    plan: Plan,
    raw_head: SessionObservationHeadV1,
    state: SessionState<Plan::Verified, Plan::Violation>,
}

impl<Plan: SessionPlanV1> Session<Plan> {
    pub(crate) fn new(stream: ObservationStreamId, plan: Plan) -> Self {
        let schema = plan.observation_schema().clone();
        Self {
            stream,
            schema,
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

    /// Prepare, evaluate and commit one update transaction. Admission and plan
    /// errors leave both the concrete raw head and lifecycle state untouched.
    /// Plan-local scratch may have been overwritten by a failed evaluation,
    /// but it is not observable lifecycle state and every evaluation must
    /// completely initialize the scratch it consumes.
    pub(crate) fn update(
        &mut self,
        update: ObservationUpdateInput,
    ) -> SessionUpdateResult<'_, Plan> {
        let owner = self
            .plan
            .try_acquire_owner()
            .ok_or(SessionUpdateError::OwnerExpired)?;
        let prepared = prepare_observation(&mut self.raw_head, self.stream, &self.schema, update)
            .map_err(SessionUpdateError::Observation)?;

        apply_prepared_update(&mut self.plan, &mut self.state, &owner, prepared)
    }

    /// Package hot path for already schema-ordered point-sRGB8 scenarios.
    /// It shares the exact lifecycle transaction below without constructing
    /// keyed surface bindings or a second raw observation owner.
    pub(crate) fn update_schema_ordered<Source: SchemaOrderedScenarioSourceV1>(
        &mut self,
        revision: Revision,
        source: &Source,
        order_scratch: &mut Vec<usize>,
    ) -> SessionUpdateResult<'_, Plan> {
        let owner = self
            .plan
            .try_acquire_owner()
            .ok_or(SessionUpdateError::OwnerExpired)?;
        let prepared = prepare_schema_ordered_observation(
            &mut self.raw_head,
            self.stream,
            &self.schema,
            revision,
            source,
            order_scratch,
        )
        .map_err(SessionUpdateError::Observation)?;

        apply_prepared_update(&mut self.plan, &mut self.state, &owner, prepared)
    }
}

fn apply_prepared_update<'session, Plan: SessionPlanV1>(
    plan: &mut Plan,
    state: &'session mut SessionState<Plan::Verified, Plan::Violation>,
    owner: &Plan::OwnerLease,
    prepared: PreparedObservationUpdateV1<'_, SessionObservationHeadV1>,
) -> SessionUpdateResult<'session, Plan> {
    match prepared {
        PreparedObservationUpdateV1::Idempotent(prepared) => {
            let _raw_head = prepared.into_owner();
            Ok(state)
        }
        PreparedObservationUpdateV1::Unknown(prepared) => {
            let (raw_head, unknown) = prepared.into_parts();
            let next_state = match take_last_verified(state) {
                Some(previous) => SessionState::Stale { previous },
                None => SessionState::Waiting,
            };
            *raw_head = SessionObservationHeadV1::Unknown(unknown);
            *state = next_state;
            Ok(state)
        }
        PreparedObservationUpdateV1::Observed(prepared) => {
            // Clone only the small Rc-backed observation handle. Both the
            // committed raw head and returned evidence then share the exact
            // immutable observation backing.
            let (raw_head, observation) = prepared.into_parts();
            let next_raw_head = SessionObservationHeadV1::Observed(observation.clone());
            let decision = plan
                .evaluate(
                    owner,
                    observation,
                    SessionObservationBindingPermitV1::mint(),
                )
                .map_err(SessionUpdateError::Plan)?;
            let SessionObservationHeadV1::Observed(expected_observation) = &next_raw_head else {
                unreachable!("the pending raw head was constructed as Observed")
            };
            if !decision
                .observation()
                .is_same_binding_as(expected_observation)
            {
                return Err(SessionUpdateError::EvidenceBindingInvariant);
            }

            // All fallible work is complete. Commit with moves only.
            let previous = take_last_verified(state);
            let next_state = match decision {
                SessionDecision::Verified(current) => SessionState::Ready { current },
                SessionDecision::Violation(cause) => SessionState::Failed { cause, previous },
            };
            *raw_head = next_raw_head;
            *state = next_state;
            Ok(state)
        }
    }
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
