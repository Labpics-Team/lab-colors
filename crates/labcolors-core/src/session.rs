//! Single lifecycle and observation owner for private F2/C8d full support.
//!
//! The closed state below owns the one current raw payload through either its
//! revision-bound report or its current `Unknown`. A separate raw head does not
//! exist; [`ObservationHeadViewV1`] is derived by borrow. At most one previous
//! verified report is retained and no transition builds a history chain.

use std::mem;

use crate::composition::CompositionProfileV1;
use crate::observation::{
    ObservationError, ObservationHeadViewV1, ObservationOwnerV1, ObservationSchemaMismatchV1,
    ObservationStreamId, ObservationUpdateInput, PreparedObservationUpdateV1,
    RevisionBoundUnknownV1, prepare_observation,
};
use crate::point_support::{
    BoundPointSupportRecheckV1, CompiledPointSupportRecheckV1, PointSupportDecisionV1,
    PointSupportEvaluationErrorV1, PointSupportViolationV1, VerifiedPointSupportV1,
};

/// Linear authority to consume and revision-bind an observation. The type is
/// visible to the evaluator only as a parameter; its private field and private
/// constructor make safe construction exclusive to this Session module.
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

#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(Clone))]
pub(crate) enum PointSupportSessionStateV1 {
    /// Initial state or a current Unknown without any previous verified report.
    Waiting {
        current_unknown: Option<RevisionBoundUnknownV1>,
    },
    Ready {
        current: VerifiedPointSupportV1,
    },
    Stale {
        previous: VerifiedPointSupportV1,
        current_unknown: RevisionBoundUnknownV1,
    },
    Failed {
        cause: PointSupportViolationV1,
        previous: Option<VerifiedPointSupportV1>,
    },
}

impl PointSupportSessionStateV1 {
    pub(crate) fn last_verified(&self) -> Option<&VerifiedPointSupportV1> {
        match self {
            Self::Waiting { .. } => None,
            Self::Ready { current } => Some(current),
            Self::Stale { previous, .. } => Some(previous),
            Self::Failed { previous, .. } => previous.as_ref(),
        }
    }
}

impl ObservationOwnerV1 for PointSupportSessionStateV1 {
    fn observation_head(&self) -> ObservationHeadViewV1<'_> {
        match self {
            Self::Waiting {
                current_unknown: None,
            } => ObservationHeadViewV1::Empty,
            Self::Waiting {
                current_unknown: Some(unknown),
            }
            | Self::Stale {
                current_unknown: unknown,
                ..
            } => ObservationHeadViewV1::Unknown(unknown),
            Self::Ready { current } => {
                ObservationHeadViewV1::Observed(current.report().observation())
            }
            Self::Failed { cause, .. } => {
                ObservationHeadViewV1::Observed(cause.report().observation())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PointSupportSessionUpdateErrorV1 {
    Observation(ObservationError),
    ObservationSchemaMismatch(ObservationSchemaMismatchV1),
    ResourceExhausted,
    InternalInvariant,
}

#[derive(Debug, PartialEq)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct PointSupportSessionV1 {
    stream: ObservationStreamId,
    recheck: BoundPointSupportRecheckV1,
    state: PointSupportSessionStateV1,
    #[cfg(test)]
    force_resource_failure: bool,
}

impl PointSupportSessionV1 {
    /// The compiled recheck owns the only canonical schema and is moved into
    /// the Session. No second schema or replacement Paint can enter updates.
    pub(crate) fn new(
        stream: ObservationStreamId,
        compiled: CompiledPointSupportRecheckV1,
    ) -> Self {
        Self {
            stream,
            recheck: compiled.into_session_recheck(),
            state: PointSupportSessionStateV1::Waiting {
                current_unknown: None,
            },
            #[cfg(test)]
            force_resource_failure: false,
        }
    }

    pub(crate) const fn state(&self) -> &PointSupportSessionStateV1 {
        &self.state
    }

    pub(crate) fn raw_head(&self) -> ObservationHeadViewV1<'_> {
        self.state.observation_head()
    }

    pub(crate) const fn composition_profile(&self) -> CompositionProfileV1 {
        self.recheck.composition_profile()
    }

    #[cfg(test)]
    pub(crate) fn force_next_resource_failure(&mut self) {
        self.force_resource_failure = true;
    }

    /// One transaction: prepare/canonicalize without mutation, transfer the
    /// exact observation under a Session-only permit to the consuming
    /// evaluator, then replace the closed owner with infallible moves only.
    pub(crate) fn update(
        &mut self,
        update: ObservationUpdateInput,
    ) -> Result<&PointSupportSessionStateV1, PointSupportSessionUpdateErrorV1> {
        let prepared = prepare_observation(
            &mut self.state,
            self.stream,
            self.recheck.surface_schema(),
            update,
        )
        .map_err(PointSupportSessionUpdateErrorV1::Observation)?;

        match prepared {
            PreparedObservationUpdateV1::Idempotent(prepared) => Ok(prepared.into_owner()),
            PreparedObservationUpdateV1::Unknown(prepared) => {
                let (state, unknown) = prepared.into_parts();
                let previous = take_last_verified(state);
                *state = match previous {
                    Some(previous) => PointSupportSessionStateV1::Stale {
                        previous,
                        current_unknown: unknown,
                    },
                    None => PointSupportSessionStateV1::Waiting {
                        current_unknown: Some(unknown),
                    },
                };
                Ok(state)
            }
            PreparedObservationUpdateV1::Observed(prepared) => {
                #[cfg(test)]
                if mem::take(&mut self.force_resource_failure) {
                    return Err(PointSupportSessionUpdateErrorV1::ResourceExhausted);
                }

                // Evaluation consumes the exact admitted observation and can
                // return only an already revision-bound decision. The mutable
                // owner is retained unchanged until that fallible work succeeds.
                let (state, observation) = prepared.into_parts();
                let decision = self
                    .recheck
                    .evaluate(observation, SessionObservationBindingPermitV1::mint())
                    .map_err(map_evaluation_error)?;
                let previous = take_last_verified(state);
                *state = match decision {
                    PointSupportDecisionV1::Verified(current) => {
                        PointSupportSessionStateV1::Ready { current }
                    }
                    PointSupportDecisionV1::Violation(cause) => {
                        PointSupportSessionStateV1::Failed { cause, previous }
                    }
                };
                Ok(state)
            }
        }
    }
}

fn map_evaluation_error(error: PointSupportEvaluationErrorV1) -> PointSupportSessionUpdateErrorV1 {
    match error {
        PointSupportEvaluationErrorV1::ObservationSchemaMismatch(mismatch) => {
            PointSupportSessionUpdateErrorV1::ObservationSchemaMismatch(mismatch)
        }
        PointSupportEvaluationErrorV1::ResourceExhausted => {
            PointSupportSessionUpdateErrorV1::ResourceExhausted
        }
        PointSupportEvaluationErrorV1::CompiledPlanInvariant
        | PointSupportEvaluationErrorV1::Wcag22Invariant
        | PointSupportEvaluationErrorV1::StabilityArithmeticInvariant => {
            PointSupportSessionUpdateErrorV1::InternalInvariant
        }
    }
}

/// Move exactly one retained verified witness out of the old closed owner.
fn take_last_verified(state: &mut PointSupportSessionStateV1) -> Option<VerifiedPointSupportV1> {
    match mem::replace(
        state,
        PointSupportSessionStateV1::Waiting {
            current_unknown: None,
        },
    ) {
        PointSupportSessionStateV1::Waiting { .. } => None,
        PointSupportSessionStateV1::Ready { current } => Some(current),
        PointSupportSessionStateV1::Stale { previous, .. } => Some(previous),
        PointSupportSessionStateV1::Failed { previous, .. } => previous,
    }
}

#[cfg(test)]
mod structural_tests {
    use super::SessionObservationBindingPermitV1;
    use crate::Srgb8;
    use crate::appearance::{EncodedPointPaintV1, OccurrenceId, PaintId, SurfaceInputPortId};
    use crate::composition::{AdmittedOpacityV1, CompositionProfileV1};
    use crate::observation::{
        ObservationHeadViewV1, ObservationOwnerV1, ObservationPayloadInput,
        ObservationSchemaMismatchV1, ObservationStreamId, ObservationUpdateInput,
        ObservedScenarioSetInput, PreparedObservationUpdateV1, Revision,
        RevisionBoundObservationV1, ScenarioId, ScenarioInput, SurfaceInputBinding,
        prepare_observation,
    };
    use crate::point_support::{
        CompiledPointSupportRecheckV1, PointSupportCriterionRequirementV1,
        PointSupportEvaluationErrorV1, PointSupportOccurrenceRequirementV1,
        PointSupportStabilityPolicyV1,
    };

    const STREAM: ObservationStreamId = ObservationStreamId::new(700);
    const REQUIRED_SURFACE: SurfaceInputPortId = SurfaceInputPortId::new(10);
    const WRONG_SURFACE: SurfaceInputPortId = SurfaceInputPortId::new(20);

    struct EmptyOwner;

    impl ObservationOwnerV1 for EmptyOwner {
        fn observation_head(&self) -> ObservationHeadViewV1<'_> {
            ObservationHeadViewV1::Empty
        }
    }

    fn wrong_schema_observation() -> RevisionBoundObservationV1 {
        let mut owner = EmptyOwner;
        let prepared = prepare_observation(
            &mut owner,
            STREAM,
            &[WRONG_SURFACE],
            ObservationUpdateInput {
                stream: STREAM,
                revision: Revision::new(1),
                payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
                    scenarios: vec![ScenarioInput {
                        id: ScenarioId::new(1),
                        bindings: vec![SurfaceInputBinding::new(
                            WRONG_SURFACE,
                            Srgb8::new([255; 3]),
                        )],
                    }],
                }),
            },
        )
        .unwrap();
        let PreparedObservationUpdateV1::Observed(prepared) = prepared else {
            panic!("fresh observed update must prepare an observation");
        };
        let (_owner, observation) = prepared.into_parts();
        observation
    }

    fn narrow_schema_observation() -> RevisionBoundObservationV1 {
        let mut owner = EmptyOwner;
        let prepared = prepare_observation(
            &mut owner,
            STREAM,
            &[REQUIRED_SURFACE],
            ObservationUpdateInput {
                stream: STREAM,
                revision: Revision::new(1),
                payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
                    scenarios: vec![ScenarioInput {
                        id: ScenarioId::new(1),
                        bindings: vec![SurfaceInputBinding::new(
                            REQUIRED_SURFACE,
                            Srgb8::new([255; 3]),
                        )],
                    }],
                }),
            },
        )
        .unwrap();
        let PreparedObservationUpdateV1::Observed(prepared) = prepared else {
            panic!("fresh observed update must prepare an observation");
        };
        let (_owner, observation) = prepared.into_parts();
        observation
    }

    #[test]
    fn consuming_evaluator_rejects_wrong_keyed_schema_before_composition() {
        let paint = EncodedPointPaintV1::from_admitted(
            PaintId::new(1),
            Srgb8::new([0; 3]),
            AdmittedOpacityV1::new(1.0).unwrap(),
        );
        let compiled = CompiledPointSupportRecheckV1::new(
            CompositionProfileV1::EncodedSrgb8SourceOverV1,
            vec![PointSupportOccurrenceRequirementV1::new(
                OccurrenceId::new(1),
                REQUIRED_SURFACE,
                paint,
                Some(Srgb8::new([0; 3])),
                PointSupportCriterionRequirementV1::NotRequested,
                PointSupportStabilityPolicyV1::Disabled,
            )],
        )
        .unwrap();
        let recheck = compiled.into_session_recheck();

        crate::composition::reset_source_over_evaluation_count();
        assert_eq!(
            recheck
                .evaluate(
                    wrong_schema_observation(),
                    SessionObservationBindingPermitV1::mint(),
                )
                .unwrap_err(),
            PointSupportEvaluationErrorV1::ObservationSchemaMismatch(
                ObservationSchemaMismatchV1::new(0, 0, Some(REQUIRED_SURFACE), Some(WRONG_SURFACE),),
            )
        );
        assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    }

    #[test]
    fn consuming_evaluator_rejects_narrow_schema_without_indexing_panic() {
        let paint = EncodedPointPaintV1::from_admitted(
            PaintId::new(1),
            Srgb8::new([0; 3]),
            AdmittedOpacityV1::new(1.0).unwrap(),
        );
        let compiled = CompiledPointSupportRecheckV1::new(
            CompositionProfileV1::EncodedSrgb8SourceOverV1,
            vec![
                PointSupportOccurrenceRequirementV1::new(
                    OccurrenceId::new(1),
                    REQUIRED_SURFACE,
                    paint,
                    Some(Srgb8::new([0; 3])),
                    PointSupportCriterionRequirementV1::NotRequested,
                    PointSupportStabilityPolicyV1::Disabled,
                ),
                PointSupportOccurrenceRequirementV1::new(
                    OccurrenceId::new(2),
                    WRONG_SURFACE,
                    paint,
                    Some(Srgb8::new([0; 3])),
                    PointSupportCriterionRequirementV1::NotRequested,
                    PointSupportStabilityPolicyV1::Disabled,
                ),
            ],
        )
        .unwrap();
        let recheck = compiled.into_session_recheck();

        crate::composition::reset_source_over_evaluation_count();
        assert_eq!(
            recheck
                .evaluate(
                    narrow_schema_observation(),
                    SessionObservationBindingPermitV1::mint(),
                )
                .unwrap_err(),
            PointSupportEvaluationErrorV1::ObservationSchemaMismatch(
                ObservationSchemaMismatchV1::new(0, 1, Some(WRONG_SURFACE), None),
            )
        );
        assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    }
}
