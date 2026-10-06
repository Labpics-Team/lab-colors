//! Детерминированные истории настоящего Session/observation lifecycle.
//! Единственный подменяемый доменный вход — результат чистого evaluator-а:
//! Verified, Violation или ошибка. Цветовая математика здесь не утверждается.

use super::*;
use crate::Srgb8;
use crate::appearance::SurfaceInputPortId;
use crate::lcs_occurrence::ColorSignal;
use crate::observation::{
    ObservedScenarioSetInput, ScenarioId, ScenarioInput, SurfaceInputBinding,
    canonicalize_observation_schema,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Evidence(RevisionBoundObservationV1);
impl private::EvidenceSealed for Evidence {}
impl SessionEvidenceV1 for Evidence {
    fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.0
    }
}

struct Plan {
    schema: CanonicalObservationSchemaV1,
    alive: bool,
    outcome: u8,
    evaluations: u8,
    substitute: Option<Evidence>,
}
impl private::PlanSealed for Plan {}
impl SessionPlanV1 for Plan {
    type OwnerLease = ();
    type Verified = Evidence;
    type Violation = Evidence;
    type Error = ();
    fn try_acquire_owner(&self) -> Option<()> {
        self.alive.then_some(())
    }
    fn observation_schema<'a>(&'a self, _: &'a ()) -> &'a CanonicalObservationSchemaV1 {
        &self.schema
    }
    fn evaluate(
        &mut self,
        _: &(),
        observation: RevisionBoundObservationV1,
        _: Option<&Evidence>,
        permit: SessionObservationBindingPermitV1,
    ) -> Result<SessionDecision<Evidence, Evidence>, ()> {
        self.evaluations += 1;
        assert!(permit.stream() == observation.stream());
        assert!(permit.revision() == observation.revision());
        if self.outcome == 2 {
            return Err(());
        }
        let evidence = self.substitute.clone().unwrap_or(Evidence(observation));
        if self.outcome == 0 {
            Ok(SessionDecision::Verified(evidence))
        } else {
            Ok(SessionDecision::Violation(evidence))
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Input {
    rgb: [u8; 3],
    scenario: u32,
}
impl SchemaOrderedScenarioSourceV1 for Input {
    fn scenario_count(&self) -> usize {
        1
    }
    fn scenario_id(&self, _: usize) -> ScenarioId {
        ScenarioId::new(self.scenario)
    }
    fn value_count(&self, _: usize) -> usize {
        1
    }
    fn value(&self, _: usize, _: usize) -> Srgb8 {
        Srgb8::new(self.rgb)
    }
}

fn new_session(stream: u32) -> Session<Plan> {
    let plan = Plan {
        schema: canonicalize_observation_schema(vec![SurfaceInputPortId::new(1)]).unwrap(),
        alive: true,
        outcome: 0,
        evaluations: 0,
        substitute: None,
    };
    Session::new(ObservationStreamId::new(stream), plan)
}

const REVISIONS: [u64; 5] = [0, 1, 2, u64::MAX - 1, u64::MAX];
const ORIGINAL: Input = Input {
    rgb: [11, 127, 251],
    scenario: 0,
};

fn finish(prepared: PreparedSessionTransition<'_, Plan>, mode: u8) {
    match mode {
        0 => drop(prepared),
        1 => {
            prepared.commit();
        }
        _ => {
            prepared.commit_deferred();
        }
    }
}

fn prepare<'a>(
    session: &'a mut Session<Plan>,
    revision: u64,
    source: Input,
    scratch: &mut Vec<usize>,
    keyed: bool,
) -> SessionPrepareResult<'a, Plan> {
    if keyed {
        session.prepare_update(ObservationUpdateInput {
            stream: session.stream,
            revision: Revision::new(revision),
            payload: ObservationPayloadInput::Scenarios(ObservedScenarioSetInput {
                scenarios: vec![ScenarioInput {
                    id: ScenarioId::new(source.scenario),
                    bindings: vec![SurfaceInputBinding::new(
                        SurfaceInputPortId::new(1),
                        ColorSignal::from_srgb8(Srgb8::new(source.rgb)),
                    )],
                }],
            }),
        })
    } else {
        session.prepare_schema_ordered(Revision::new(revision), &source, scratch)
    }
}

#[test]
fn lifecycle_unknown_order_abort_and_owner_matrix() {
    for initial in [None, Some(0), Some(1), Some(u64::MAX)] {
        for revision in REVISIONS {
            for reason in [0, 1, u32::MAX] {
                for stream in [0, u32::MAX] {
                    for incoming_stream in [0, u32::MAX] {
                        for alive in [false, true] {
                            for mode in 0..3 {
                                let mut session = new_session(stream);
                                if let Some(initial) = initial {
                                    session
                                        .prepare_unknown(
                                            Revision::new(initial),
                                            UnknownReasonId::new(1),
                                        )
                                        .unwrap()
                                        .commit();
                                }
                                let old_head = session.raw_head.clone();
                                let old_state = session.state.clone();
                                session.plan.alive = alive;
                                let duplicate = initial == Some(revision) && reason == 1;
                                let allowed = alive
                                    && stream == incoming_stream
                                    && (initial.is_none_or(|previous| revision > previous)
                                        || duplicate);
                                {
                                    let result = session.prepare_update(ObservationUpdateInput {
                                        stream: ObservationStreamId::new(incoming_stream),
                                        revision: Revision::new(revision),
                                        payload: ObservationPayloadInput::Unknown(
                                            UnknownReasonId::new(reason),
                                        ),
                                    });
                                    assert_eq!(result.is_ok(), allowed);
                                    if let Ok(prepared) = result {
                                        finish(prepared, mode);
                                    }
                                }
                                if !allowed || mode == 0 || duplicate {
                                    assert_eq!(session.raw_head, old_head);
                                    assert_eq!(session.state, old_state);
                                } else {
                                    let ObservationHeadViewV1::Unknown(value) = session.raw_head()
                                    else {
                                        panic!("unknown head required")
                                    };
                                    assert_eq!(
                                        (
                                            value.stream().value(),
                                            value.revision().value(),
                                            value.reason().value()
                                        ),
                                        (stream, revision, reason)
                                    );
                                    assert!(matches!(session.state(), SessionState::Waiting));
                                }
                                assert_eq!(session.plan.evaluations, 0);
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn lifecycle_observed_order_cancel_and_failure_matrix() {
    for base in REVISIONS {
        for revision in REVISIONS {
            for source in [
                ORIGINAL,
                Input {
                    rgb: [0, 0, 0],
                    ..ORIGINAL
                },
                Input {
                    scenario: u32::MAX,
                    ..ORIGINAL
                },
            ] {
                for outcome in 0..3 {
                    for alive in [false, true] {
                        for mode in 0..3 {
                            for keyed in [false, true] {
                                let mut session = new_session(u32::MAX);
                                let mut scratch = Vec::new();
                                prepare(&mut session, base, ORIGINAL, &mut scratch, keyed)
                                    .unwrap()
                                    .commit();
                                let old_head = session.raw_head.clone();
                                let old_state = session.state.clone();
                                let previous = session.state.last_verified().unwrap().clone();
                                session.plan.alive = alive;
                                session.plan.outcome = outcome;
                                let duplicate = revision == base && source == ORIGINAL;
                                let allowed =
                                    alive && (duplicate || (revision > base && outcome != 2));
                                {
                                    let result = prepare(
                                        &mut session,
                                        revision,
                                        source,
                                        &mut scratch,
                                        keyed,
                                    );
                                    assert_eq!(result.is_ok(), allowed);
                                    if let Ok(prepared) = result {
                                        finish(prepared, mode);
                                    }
                                }
                                if !allowed || mode == 0 || duplicate {
                                    assert_eq!(session.raw_head, old_head);
                                    assert_eq!(session.state, old_state);
                                } else {
                                    let ObservationHeadViewV1::Observed(raw) = session.raw_head()
                                    else {
                                        panic!("observed head required")
                                    };
                                    assert_eq!(raw.revision().value(), revision);
                                    assert_eq!(
                                        raw.physical_values(0).unwrap()[0].srgb8().bytes(),
                                        source.rgb
                                    );
                                    match session.state() {
                                        SessionState::Ready { current } => {
                                            assert_eq!(outcome, 0);
                                            assert!(current.observation().is_same_binding_as(raw));
                                            assert_eq!(
                                                session.state.current_verified(),
                                                Some(current)
                                            );
                                        }
                                        SessionState::Failed {
                                            cause,
                                            previous: retained,
                                        } => {
                                            assert_eq!(outcome, 1);
                                            assert!(cause.observation().is_same_binding_as(raw));
                                            assert_eq!(*retained, Some(previous));
                                            assert!(session.state.current_verified().is_none());
                                        }
                                        _ => panic!("new observed result must be Ready or Failed"),
                                    }
                                }
                                assert_eq!(
                                    session.plan.evaluations,
                                    1 + u8::from(alive && revision > base)
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn lifecycle_rejects_saved_evidence_for_a_new_revision() {
    for outcome in 0..2 {
        for keyed in [false, true] {
            let mut session = new_session(1);
            let mut scratch = Vec::new();
            prepare(&mut session, 0, ORIGINAL, &mut scratch, keyed)
                .unwrap()
                .commit();
            let old_head = session.raw_head.clone();
            let old_state = session.state.clone();
            session.plan.substitute = session.state.last_verified().cloned();
            session.plan.outcome = outcome;
            let result = prepare(&mut session, 1, ORIGINAL, &mut scratch, keyed);
            assert!(matches!(
                result,
                Err(SessionUpdateError::EvidenceBindingInvariant)
            ));
            drop(result);
            assert_eq!(session.raw_head, old_head);
            assert_eq!(session.state, old_state);
        }
    }
}

#[test]
fn lifecycle_cancellation_does_not_consume_the_arena_pool() {
    for outcomes in 0..16 {
        for keyed in [false, true] {
            let mut session = new_session(1);
            let mut scratch = Vec::new();
            prepare(&mut session, 0, ORIGINAL, &mut scratch, keyed)
                .unwrap()
                .commit_deferred();
            for index in 0_u8..4 {
                session.plan.outcome = (outcomes >> index) & 1;
                drop(
                    prepare(
                        &mut session,
                        u64::from(index) + 1,
                        ORIGINAL,
                        &mut scratch,
                        keyed,
                    )
                    .unwrap(),
                );
                assert_eq!(session.raw_head().revision(), Some(Revision::new(0)));
            }
            session.plan.outcome = 0;
            prepare(&mut session, 5, ORIGINAL, &mut scratch, keyed)
                .unwrap()
                .commit();
            assert_eq!(session.raw_head().revision(), Some(Revision::new(5)));
        }
    }
}

#[test]
fn lifecycle_unknown_and_violation_never_reactivate_last_good() {
    for keyed in [false, true] {
        let mut session = new_session(1);
        let mut scratch = Vec::new();
        prepare(&mut session, 0, ORIGINAL, &mut scratch, keyed)
            .unwrap()
            .commit_deferred();
        session
            .prepare_unknown(Revision::new(1), UnknownReasonId::new(8))
            .unwrap()
            .commit_deferred();
        assert!(matches!(session.state(), SessionState::Stale { .. }));
        assert!(session.state.current_verified().is_none());
        assert_eq!(
            session
                .state
                .last_verified()
                .unwrap()
                .observation()
                .revision(),
            Revision::new(0)
        );
        session.plan.outcome = 1;
        prepare(&mut session, 2, ORIGINAL, &mut scratch, keyed)
            .unwrap()
            .commit_deferred();
        assert!(matches!(session.state(), SessionState::Failed { .. }));
        assert!(session.state.current_verified().is_none());
        assert_eq!(
            session
                .state
                .last_verified()
                .unwrap()
                .observation()
                .revision(),
            Revision::new(0)
        );
        session.plan.outcome = 0;
        prepare(&mut session, 3, ORIGINAL, &mut scratch, keyed)
            .unwrap()
            .commit();
        assert_eq!(
            session
                .state
                .current_verified()
                .unwrap()
                .observation()
                .revision(),
            Revision::new(3)
        );
    }
}
