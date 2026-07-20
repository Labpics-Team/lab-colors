//! Единственный lifecycle owner fixed-candidate F2.
//!
//! Session атомарно связывает raw observation watermark, prebound exact recheck и
//! последний verified evidence. Raw admission не хранит прошлое, recheck не знает
//! `Waiting | Ready | Stale | Failed`, а output/presentation остаются за O1.

use std::mem;

use crate::appearance::{EncodedPointPaintV1, SurfaceInputPortId};
use crate::observation::{
    ObservationError, ObservationHead, ObservationState, ObservationStreamId,
    ObservationUpdateInput, PreparedObservationViewV1, RevisionBoundUnknownV1,
};
use crate::recheck::{
    BoundFixedRecheckV1, CompiledFixedRecheckV1, ExactViolationRecheckV1, FixedRecheckBindErrorV1,
    FixedRecheckDecisionV1, RecheckProtocolErrorV1, RevisionBoundRecheckV1,
};

/// Session владеет ровно одним current lifecycle state и не хранит update history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExactFixedSessionStateV1 {
    Waiting,
    Ready {
        current: RevisionBoundRecheckV1,
    },
    Stale {
        previous: RevisionBoundRecheckV1,
        current_unknown: RevisionBoundUnknownV1,
    },
    Failed {
        cause: ExactViolationRecheckV1,
        previous: Option<RevisionBoundRecheckV1>,
    },
}

impl ExactFixedSessionStateV1 {
    pub(crate) fn last_verified(&self) -> Option<&RevisionBoundRecheckV1> {
        match self {
            Self::Waiting => None,
            Self::Ready { current } => Some(current),
            Self::Stale { previous, .. } => Some(previous),
            Self::Failed { previous, .. } => previous.as_ref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FixedSessionBuildErrorV1 {
    Observation(ObservationError),
    Recheck(FixedRecheckBindErrorV1),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FixedSessionUpdateErrorV1 {
    Observation(ObservationError),
    ResourceExhausted,
    InternalInvariant,
}

/// Один prebound fixed-candidate Session. Candidate, schema, evaluator invocation
/// и surface indices принимаются только construction-ом и не приходят в update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FixedCandidateSessionV1 {
    observation: ObservationState,
    recheck: BoundFixedRecheckV1,
    state: ExactFixedSessionStateV1,
    #[cfg(test)]
    force_resource_failure: bool,
}

impl FixedCandidateSessionV1 {
    pub(crate) fn new(
        stream: ObservationStreamId,
        schema: Vec<SurfaceInputPortId>,
        requirement: CompiledFixedRecheckV1,
        paint: EncodedPointPaintV1,
    ) -> Result<Self, FixedSessionBuildErrorV1> {
        let observation =
            ObservationState::new(stream, schema).map_err(FixedSessionBuildErrorV1::Observation)?;
        let recheck = requirement
            .bind(observation.compiled_surface_input_schema(), paint)
            .map_err(FixedSessionBuildErrorV1::Recheck)?;
        Ok(Self {
            observation,
            recheck,
            state: ExactFixedSessionStateV1::Waiting,
            #[cfg(test)]
            force_resource_failure: false,
        })
    }

    pub(crate) const fn state(&self) -> &ExactFixedSessionStateV1 {
        &self.state
    }

    pub(crate) const fn raw_head(&self) -> &ObservationHead {
        self.observation.head()
    }

    pub(crate) const fn paint(&self) -> EncodedPointPaintV1 {
        self.recheck.paint()
    }

    #[cfg(test)]
    pub(crate) fn force_next_resource_failure(&mut self) {
        self.force_resource_failure = true;
    }

    /// Одна логическая транзакция:
    /// prepare admission без mutation → exact recheck/preflight → build next state
    /// → infallible raw+session commit. Любой `Err` сохраняет оба SSOT byte-identical.
    pub(crate) fn update(
        &mut self,
        update: ObservationUpdateInput,
    ) -> Result<&ExactFixedSessionStateV1, FixedSessionUpdateErrorV1> {
        let prepared = self
            .observation
            .prepare(update)
            .map_err(FixedSessionUpdateErrorV1::Observation)?;

        match prepared.view() {
            PreparedObservationViewV1::Idempotent => Ok(&self.state),
            PreparedObservationViewV1::AppliedUnknown(unknown) => {
                let previous = take_last_verified(&mut self.state);
                let next = match previous {
                    Some(previous) => ExactFixedSessionStateV1::Stale {
                        previous,
                        current_unknown: unknown,
                    },
                    None => ExactFixedSessionStateV1::Waiting,
                };
                let disposition = prepared.commit();
                debug_assert_eq!(disposition, crate::observation::UpdateDisposition::Applied);
                self.state = next;
                Ok(&self.state)
            }
            PreparedObservationViewV1::AppliedObserved(observation) => {
                #[cfg(test)]
                if mem::take(&mut self.force_resource_failure) {
                    return Err(FixedSessionUpdateErrorV1::ResourceExhausted);
                }
                let decision = self
                    .recheck
                    .recheck(observation)
                    .map_err(map_recheck_error)?;
                let previous = take_last_verified(&mut self.state);
                let next = match decision {
                    FixedRecheckDecisionV1::Verified(current) => {
                        ExactFixedSessionStateV1::Ready { current }
                    }
                    FixedRecheckDecisionV1::Violation(cause) => {
                        ExactFixedSessionStateV1::Failed { cause, previous }
                    }
                };
                let disposition = prepared.commit();
                debug_assert_eq!(disposition, crate::observation::UpdateDisposition::Applied);
                self.state = next;
                Ok(&self.state)
            }
        }
    }
}

fn map_recheck_error(error: RecheckProtocolErrorV1) -> FixedSessionUpdateErrorV1 {
    match error {
        RecheckProtocolErrorV1::ResourceExhausted => FixedSessionUpdateErrorV1::ResourceExhausted,
        RecheckProtocolErrorV1::ObservationSchemaMismatch => {
            FixedSessionUpdateErrorV1::InternalInvariant
        }
    }
}

/// Переносит ровно один retained verified witness без clone и без history chain.
fn take_last_verified(state: &mut ExactFixedSessionStateV1) -> Option<RevisionBoundRecheckV1> {
    match mem::replace(state, ExactFixedSessionStateV1::Waiting) {
        ExactFixedSessionStateV1::Waiting => None,
        ExactFixedSessionStateV1::Ready { current } => Some(current),
        ExactFixedSessionStateV1::Stale { previous, .. } => Some(previous),
        ExactFixedSessionStateV1::Failed { previous, .. } => previous,
    }
}
