use super::support::{
    InMemoryPointSinkAdmissionErrorV1, InMemoryPointSinkErrorV1, TestHostBindingAxisV1,
    allocator_point_sink, authored_emission, authored_presentation, in_memory_point_sink,
};
use super::*;
use crate::Srgb8;
use crate::program::{
    AppearanceContextV1, ConstraintIdV1, DraftV1, JointChoiceV1, JointStateV1, PaintIdV1,
    ScenarioV1, SourceIdV1, StateKindV1, SurfaceIdV1, SurfaceInputPortIdV1, SurroundV1,
    TargetCandidateIdV1, TargetCandidateV1, TargetIdV1,
};
use crate::wcag22::Wcag22CriterionV1;
use proptest::prelude::*;

const SOURCE: SourceIdV1 = SourceIdV1::new(1);
const TARGET: TargetIdV1 = TargetIdV1::new(2);
const PAINT: PaintIdV1 = PaintIdV1::new(4);
const INPUT: SurfaceInputPortIdV1 = SurfaceInputPortIdV1::new(5);
const INPUT_SURFACE: SurfaceIdV1 = SurfaceIdV1::new(6);
const INNER: OccurrenceIdV1 = OccurrenceIdV1::new(8);
const TERMINAL: OccurrenceIdV1 = OccurrenceIdV1::new(9);
const MIDDLE: OccurrenceIdV1 = OccurrenceIdV1::new(15);
const ROOT: PresentationRootIdV1 = PresentationRootIdV1::new(51);
const OUTPUT_A: OutputSlotIdV1 = OutputSlotIdV1::new(12);
const OUTPUT_B: OutputSlotIdV1 = OutputSlotIdV1::new(13);

#[test]
fn terminal_stamp_is_a_fixed_two_word_copy_value() {
    const fn assert_copy<T: Copy>() {}
    assert_copy::<PointSinkStampV1>();
    assert_eq!(
        core::mem::size_of::<PointSinkStampV1>(),
        core::mem::size_of::<[u64; 2]>()
    );
}

#[test]
fn a_stale_copy_stamp_cannot_cross_a_sequential_sink_epoch() {
    let owner = owner(
        Srgb8::new([12, 34, 56]),
        Srgb8::new([12, 34, 56]),
        false,
        &[OUTPUT_A],
    );
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let unknown = UpdateV1::Unknown {
        revision: 1,
        reason_id: 77,
    };

    let (first, first_probe) = in_memory_point_sink(&[900]);
    let mut first = owner.attach(1, &emissions, &presentations, first).unwrap();
    first.update(unknown).unwrap();
    let stale = first_probe.stamp();
    drop(first);

    let (second, second_probe) = in_memory_point_sink(&[900]);
    let mut second = owner.attach(1, &emissions, &presentations, second).unwrap();
    second.update(unknown).unwrap();
    assert_ne!(second_probe.stamp(), stale);
    assert!(matches!(
        second.sink.prepare(PointSinkIntentV1::ConfirmExact {
            revision: 1,
            published_stamp: stale,
        }),
        Err(InMemoryPointSinkErrorV1::StampMismatch)
    ));
}

#[test]
fn cold_attach_failure_preserves_the_same_unbound_lease_for_retry() {
    let owner = owner(
        Srgb8::new([12, 34, 56]),
        Srgb8::new([12, 34, 56]),
        false,
        &[OUTPUT_A],
    );
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];

    let failure = match owner.attach(1, &emissions, &[], sink) {
        Ok(_) => panic!("incomplete presentation binding must fail"),
        Err(failure) => failure,
    };
    assert!(matches!(
        failure.cause(),
        &AttachmentCreateCauseV1::Contract(AttachmentCreateErrorV1::EmptyPresentations)
    ));
    assert!(probe.ambient_fallback_is_exposed());
    assert!(!probe.lease_was_dropped());

    let sink = failure.into_sink();
    let attachment = owner.attach(1, &emissions, &presentations, sink).unwrap();
    assert!(probe.is_closed());
    assert!(!probe.ambient_fallback_is_exposed());

    attachment.dispose();
    assert!(probe.is_closed());
    assert!(!probe.ambient_fallback_is_exposed());
}

#[test]
fn create_failure_debug_reports_the_typed_cause_without_sink_internals() {
    let owner = owner(
        Srgb8::new([12, 34, 56]),
        Srgb8::new([12, 34, 56]),
        false,
        &[OUTPUT_A],
    );
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];

    let (sink, _) = in_memory_point_sink(&[900]);
    let contract = match owner.attach(1, &emissions, &[], sink) {
        Ok(_) => panic!("contract failure was expected"),
        Err(failure) => failure,
    };
    let contract_debug = format!("{contract:?}");
    assert!(contract_debug.contains("AttachmentCreateFailureV1"));
    assert!(contract_debug.contains("Contract(EmptyPresentations)"));
    assert!(!contract_debug.contains("owned_scope"));
    assert!(!contract_debug.contains("TestSinkSharedV1"));

    let (sink, probe) = in_memory_point_sink(&[900]);
    probe.reject_next_admission();
    let admission = match owner.attach(2, &emissions, &presentations, sink) {
        Ok(_) => panic!("admission failure was expected"),
        Err(failure) => failure,
    };
    let admission_debug = format!("{admission:?}");
    assert!(admission_debug.contains("AttachmentCreateFailureV1"));
    assert!(admission_debug.contains("SinkAdmission(RejectedBeforeInstall)"));
    assert!(!admission_debug.contains("owned_scope"));
    assert!(!admission_debug.contains("TestSinkSharedV1"));
}

#[test]
fn failed_closed_admission_is_atomic_and_mints_epoch_only_after_install() {
    let owner = owner(
        Srgb8::new([12, 34, 56]),
        Srgb8::new([12, 34, 56]),
        false,
        &[OUTPUT_A],
    );
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];

    probe.reject_next_admission();
    let failure = match owner.attach(2, &emissions, &presentations, sink) {
        Ok(_) => panic!("pre-install admission fault must return the lease"),
        Err(failure) => failure,
    };
    assert!(matches!(
        failure.cause(),
        &AttachmentCreateCauseV1::SinkAdmission(
            InMemoryPointSinkAdmissionErrorV1::RejectedBeforeInstall
        )
    ));
    assert!(probe.ambient_fallback_is_exposed());
    assert_eq!(probe.admitted_stamp(), None);

    let sink = failure.into_sink();
    probe.reject_next_admission_after_install();
    let failure = match owner.attach(2, &emissions, &presentations, sink) {
        Ok(_) => panic!("post-install fault must roll the tombstone back"),
        Err(failure) => failure,
    };
    assert!(matches!(
        failure.cause(),
        &AttachmentCreateCauseV1::SinkAdmission(
            InMemoryPointSinkAdmissionErrorV1::RejectedAfterInstall
        )
    ));
    assert!(probe.ambient_fallback_is_exposed());
    assert_eq!(probe.admitted_stamp(), None);

    let attachment = owner
        .attach(2, &emissions, &presentations, failure.into_sink())
        .unwrap();
    let admitted = probe.stamp();
    assert_eq!(admitted.sequence(), 0);
    assert!(probe.is_closed());
    drop(attachment);
    assert!(probe.is_closed());
}

#[test]
fn initial_unknown_and_violation_keep_the_host_scope_closed() {
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let unknown = UpdateV1::Unknown {
        revision: 1,
        reason_id: 77,
    };

    let pass_owner = owner(
        Srgb8::new([0, 0, 0]),
        Srgb8::new([0, 0, 0]),
        false,
        &[OUTPUT_A],
    );
    let (sink, unknown_probe) = in_memory_point_sink(&[900]);
    let mut attachment = pass_owner
        .attach(3, &emissions, &presentations, sink)
        .unwrap();
    assert!(unknown_probe.is_closed());
    attachment.update(unknown).unwrap();
    assert!(unknown_probe.is_closed());
    assert!(!unknown_probe.ambient_fallback_is_exposed());

    let conflict_owner = owner(
        Srgb8::new([255, 0, 0]),
        Srgb8::new([0, 0, 0]),
        false,
        &[OUTPUT_A],
    );
    let (sink, violation_probe) = in_memory_point_sink(&[900]);
    let mut conflict = conflict_owner
        .attach(4, &emissions, &presentations, sink)
        .unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];
    conflict.update(observed(1, &scenarios)).unwrap();
    assert!(violation_probe.is_closed());
    assert!(!violation_probe.ambient_fallback_is_exposed());
}

#[test]
fn every_host_binding_axis_is_checked_before_sink_mutation() {
    let owner = owner(
        Srgb8::new([12, 34, 56]),
        Srgb8::new([12, 34, 56]),
        false,
        &[OUTPUT_A],
    );
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    for (index, axis) in [
        TestHostBindingAxisV1::Realm,
        TestHostBindingAxisV1::Root,
        TestHostBindingAxisV1::Scope,
        TestHostBindingAxisV1::Codec,
        TestHostBindingAxisV1::Capabilities,
        TestHostBindingAxisV1::Tombstone,
    ]
    .into_iter()
    .enumerate()
    {
        let (sink, probe) = in_memory_point_sink(&[900]);
        let mut attachment = owner
            .attach(5 + index as u32, &emissions, &presentations, sink)
            .unwrap();
        let initial_stamp = probe.stamp();
        probe.drift_host_binding(axis);
        let drifted_stamp = probe.stamp();
        assert_ne!(drifted_stamp, initial_stamp, "axis {axis:?}");
        assert!(matches!(
            attachment.update(UpdateV1::Unknown {
                revision: 1,
                reason_id: 77,
            }),
            Err(AttachmentUpdateErrorV1::SinkPrepare(
                InMemoryPointSinkErrorV1::BindingDrift
            ))
        ));
        assert_eq!(probe.stamp(), drifted_stamp, "axis {axis:?}");
        assert!(probe.is_closed(), "axis {axis:?}");
        assert_eq!(probe.intent_counts(), Default::default(), "axis {axis:?}");
        assert!(matches!(
            attachment.session.evidence().observation_head(),
            super::super::ObservationHeadV1::Empty
        ));
        // Generation монотонна: восстановление сырых host-фактов не может
        // воскресить полномочие уже привязанного closed lease.
        probe.restore_host_binding();
        let restored_facts_stamp = probe.stamp();
        assert_ne!(restored_facts_stamp, drifted_stamp, "axis {axis:?}");
        assert!(matches!(
            attachment.update(UpdateV1::Unknown {
                revision: 1,
                reason_id: 77,
            }),
            Err(AttachmentUpdateErrorV1::SinkPrepare(
                InMemoryPointSinkErrorV1::BindingDrift
            ))
        ));
        assert_eq!(probe.stamp(), restored_facts_stamp, "axis {axis:?}");
        assert_eq!(probe.intent_counts(), Default::default(), "axis {axis:?}");
        drop(attachment);
        assert!(probe.is_closed(), "axis {axis:?}");
        assert!(probe.foreign_scope_is_untouched(), "axis {axis:?}");
    }
}

#[test]
fn every_sink_intent_cas_checks_the_same_current_binding_stamp() {
    let owner = owner(
        Srgb8::new([12, 34, 56]),
        Srgb8::new([12, 34, 56]),
        false,
        &[OUTPUT_A],
    );
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let (sink, probe) = in_memory_point_sink(&[900]);
    let mut attachment = owner.attach(6, &emissions, &presentations, sink).unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];
    attachment.update(observed(1, &scenarios)).unwrap();
    let baseline_stamp = probe.stamp();
    let baseline_revision = probe.revision();
    let baseline_snapshot = probe.snapshot();
    let baseline_counts = probe.intent_counts();
    let (foreign, foreign_probe) = in_memory_point_sink(&[900]);
    let foreign = owner
        .attach(7, &emissions, &presentations, foreign)
        .unwrap();
    let foreign_stamp = foreign_probe.stamp();
    let foreign_transition = PointSinkMutationStampV1::new(foreign_stamp).unwrap();
    assert_ne!(foreign_stamp, probe.stamp());

    assert!(matches!(
        attachment.sink.prepare(PointSinkIntentV1::SetAll {
            revision: 1,
            stamp: foreign_transition,
            patch: &[],
        }),
        Err(InMemoryPointSinkErrorV1::StampMismatch)
    ));
    assert!(matches!(
        attachment.sink.prepare(PointSinkIntentV1::RevokeAll {
            revision: 1,
            stamp: foreign_transition,
        }),
        Err(InMemoryPointSinkErrorV1::StampMismatch)
    ));
    assert!(matches!(
        attachment.sink.prepare(PointSinkIntentV1::ConfirmExact {
            revision: 1,
            published_stamp: foreign_stamp,
        }),
        Err(InMemoryPointSinkErrorV1::StampMismatch)
    ));
    assert_eq!(probe.stamp(), baseline_stamp);
    assert_eq!(probe.revision(), baseline_revision);
    assert_eq!(probe.snapshot(), baseline_snapshot);
    assert_eq!(probe.intent_counts(), baseline_counts);
    assert!(!probe.is_busy());

    drop(foreign);
    drop(attachment);
    assert!(probe.is_closed());
    assert!(foreign_probe.is_closed());
}

#[test]
fn core_mints_the_exact_successor_for_every_mutating_intent() {
    let owner = owner(
        Srgb8::new([12, 34, 56]),
        Srgb8::new([12, 34, 56]),
        false,
        &[OUTPUT_A],
    );
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let (sink, probe) = in_memory_point_sink(&[900]);
    let mut attachment = owner.attach(8, &emissions, &presentations, sink).unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];

    let admission_stamp = probe.stamp();
    attachment.update(observed(1, &scenarios)).unwrap();
    let published_stamp = probe.stamp();
    assert_eq!(
        published_stamp.sequence(),
        admission_stamp.sequence().checked_add(1).unwrap()
    );
    assert_eq!(
        published_stamp.binding_epoch(),
        admission_stamp.binding_epoch()
    );

    attachment
        .update(UpdateV1::Unknown {
            revision: 2,
            reason_id: 77,
        })
        .unwrap();
    let revoked_stamp = probe.stamp();
    assert_eq!(
        revoked_stamp.sequence(),
        published_stamp.sequence().checked_add(1).unwrap()
    );
    assert_eq!(
        revoked_stamp.binding_epoch(),
        admission_stamp.binding_epoch()
    );

    attachment
        .update(UpdateV1::Unknown {
            revision: 2,
            reason_id: 77,
        })
        .unwrap();
    assert_eq!(probe.stamp(), revoked_stamp);
}

#[test]
fn exhausted_stamp_fails_before_sink_prepare_or_session_commit() {
    let owner = owner(
        Srgb8::new([12, 34, 56]),
        Srgb8::new([12, 34, 56]),
        false,
        &[OUTPUT_A],
    );
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let (sink, probe) = in_memory_point_sink(&[900]);
    let mut attachment = owner.attach(8, &emissions, &presentations, sink).unwrap();
    let exhausted = PointSinkStampV1::new(u64::MAX, probe.stamp().binding_epoch());
    probe.force_stamp_sequence(u64::MAX);
    attachment.expected_sink_stamp = exhausted;
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];

    assert!(matches!(
        attachment.update(observed(1, &scenarios)),
        Err(AttachmentUpdateErrorV1::InternalInvariant(
            AttachmentInvariantV1::SinkStampExhausted
        ))
    ));
    assert_eq!(probe.stamp(), exhausted);
    assert_eq!(probe.intent_counts(), Default::default());
    assert!(probe.is_closed());
    assert!(matches!(
        attachment.session.evidence().observation_head(),
        super::super::ObservationHeadV1::Empty
    ));
}

#[test]
fn closed_revoke_is_confirmable_from_one_expected_stamp_source_of_truth() {
    let owner = owner(
        Srgb8::new([12, 34, 56]),
        Srgb8::new([12, 34, 56]),
        false,
        &[OUTPUT_A],
    );
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let mut attachment = owner.attach(8, &emissions, &presentations, sink).unwrap();
    let admission_stamp = attachment.expected_sink_stamp;
    assert_eq!(attachment.committed_revision, None);

    let unknown = UpdateV1::Unknown {
        revision: 1,
        reason_id: 77,
    };
    attachment.update(unknown).unwrap();
    let revoked_stamp = attachment.expected_sink_stamp;
    assert_ne!(revoked_stamp, admission_stamp);
    assert_eq!(attachment.committed_revision, Some(1));
    assert!(probe.is_closed());
    assert_eq!(probe.intent_counts().revoke_all, 1);

    attachment.update(unknown).unwrap();
    assert_eq!(attachment.expected_sink_stamp, revoked_stamp);
    assert_eq!(attachment.committed_revision, Some(1));
    assert_eq!(probe.intent_counts().revoke_all, 1);
    assert_eq!(probe.intent_counts().confirm_exact, 1);

    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];
    let committed = attachment.update(observed(2, &scenarios)).unwrap();
    let output = committed.render_outputs().next().unwrap();
    assert_eq!(output.published_stamp().revision(), 2);
    assert_eq!(output.published_stamp().sink_stamp(), probe.stamp());
    assert_eq!(attachment.committed_revision, Some(2));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelSnapshotV1 {
    Closed,
    Published,
}

proptest! {
    #[test]
    fn admitted_state_machine_never_exposes_ambient_fallback(
        actions in prop::collection::vec(0_u8..11, 0..64),
    ) {
        let owner = owner(
            Srgb8::new([12, 34, 56]),
            Srgb8::new([12, 34, 56]),
            false,
            &[OUTPUT_A],
        );
        let (sink, probe) = in_memory_point_sink(&[900]);
        let emissions = [authored_emission(OUTPUT_A.value(), 900)];
        let presentations = [authored_presentation(
            OUTPUT_A.value(),
            ROOT.value(),
            INNER.value(),
        )];
        let mut attachment = owner
            .attach(9, &emissions, &presentations, sink)
            .unwrap();
        let values = [Srgb8::new([0, 0, 0])];
        let scenarios = [ScenarioV1::new(1, &values)];
        let mut revision = 0_u64;
        let mut model = ModelSnapshotV1::Closed;
        let mut binding_valid = true;

        for action in actions {
            if !binding_valid {
                let stamp = probe.stamp();
                let counts = probe.intent_counts();
                let result = attachment.update(UpdateV1::Unknown {
                    revision: revision + 1,
                    reason_id: 77,
                });
                prop_assert!(matches!(
                    result,
                    Err(AttachmentUpdateErrorV1::SinkPrepare(
                        InMemoryPointSinkErrorV1::BindingDrift
                    ))
                ));
                prop_assert_eq!(probe.stamp(), stamp);
                prop_assert_eq!(probe.intent_counts(), counts);
                prop_assert!(!probe.ambient_fallback_is_exposed());
                continue;
            }
            match action {
                0 => {
                    revision += 1;
                    attachment.update(observed(revision, &scenarios)).unwrap();
                    model = ModelSnapshotV1::Published;
                }
                1 => {
                    revision += 1;
                    attachment.update(UpdateV1::Unknown {
                        revision,
                        reason_id: 77,
                    }).unwrap();
                    model = ModelSnapshotV1::Closed;
                }
                2 => {
                    probe.reject_next_prepare();
                    let result = attachment.update(observed(revision + 1, &scenarios));
                    prop_assert!(matches!(
                        result,
                        Err(AttachmentUpdateErrorV1::SinkPrepare(
                            InMemoryPointSinkErrorV1::RejectedPrepare
                        ))
                    ));
                }
                3 => {
                    probe.reject_next_install();
                    let result = attachment.update(UpdateV1::Unknown {
                        revision: revision + 1,
                        reason_id: 77,
                    });
                    prop_assert!(matches!(
                        result,
                        Err(AttachmentUpdateErrorV1::SinkInstall(
                            InMemoryPointSinkErrorV1::RejectedInstall
                        ))
                    ));
                }
                4 => {
                    if revision == 0 {
                        revision = 1;
                        attachment.update(UpdateV1::Unknown {
                            revision,
                            reason_id: 77,
                        }).unwrap();
                        model = ModelSnapshotV1::Closed;
                    }
                    match model {
                        ModelSnapshotV1::Closed => {
                            attachment.update(UpdateV1::Unknown {
                                revision,
                                reason_id: 77,
                            }).unwrap();
                        }
                        ModelSnapshotV1::Published => {
                            attachment.update(observed(revision, &scenarios)).unwrap();
                        }
                    }
                }
                5..=10 => {
                    let axis = match action {
                        5 => TestHostBindingAxisV1::Realm,
                        6 => TestHostBindingAxisV1::Root,
                        7 => TestHostBindingAxisV1::Scope,
                        8 => TestHostBindingAxisV1::Codec,
                        9 => TestHostBindingAxisV1::Capabilities,
                        _ => TestHostBindingAxisV1::Tombstone,
                    };
                    probe.drift_host_binding(axis);
                    let result = attachment.update(UpdateV1::Unknown {
                        revision: revision + 1,
                        reason_id: 77,
                    });
                    prop_assert!(matches!(
                        result,
                        Err(AttachmentUpdateErrorV1::SinkPrepare(
                            InMemoryPointSinkErrorV1::BindingDrift
                        ))
                    ));
                    binding_valid = false;
                }
                _ => unreachable!("стратегия генерирует только действия 0..=10"),
            }

            prop_assert!(!probe.ambient_fallback_is_exposed());
            if binding_valid {
                prop_assert_eq!(probe.stamp(), attachment.expected_sink_stamp);
            } else {
                prop_assert_ne!(probe.stamp(), attachment.expected_sink_stamp);
            }
            prop_assert_eq!(attachment.committed_revision, (revision != 0).then_some(revision));
            match model {
                ModelSnapshotV1::Closed => {
                    prop_assert!(probe.is_closed());
                    prop_assert!(probe.snapshot().is_empty());
                }
                ModelSnapshotV1::Published => prop_assert_eq!(probe.snapshot().len(), 1),
            }
        }

        drop(attachment);
        prop_assert!(probe.is_closed());
        prop_assert!(!probe.ambient_fallback_is_exposed());
        if !binding_valid {
            prop_assert!(probe.foreign_scope_is_untouched());
        }
    }
}

#[test]
fn attachment_terminal_tail_has_no_allocator_events() {
    let owner = owner(
        Srgb8::new([12, 34, 56]),
        Srgb8::new([12, 34, 56]),
        false,
        &[OUTPUT_A],
    );
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let mut attachment = owner.attach(1, &emissions, &presentations, sink).unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(44, &values)];

    probe.checkpoint_next_terminal_tail();
    let ((), events) = crate::test_support::measured_allocator_events(|| {
        attachment.update(observed(1, &scenarios)).unwrap();
    });
    assert_eq!(events, crate::test_support::AllocatorEvents::default());

    let unknown = UpdateV1::Unknown {
        revision: 2,
        reason_id: 77,
    };
    probe.checkpoint_next_terminal_tail();
    let ((), events) = crate::test_support::measured_allocator_events(|| {
        attachment.update(unknown).unwrap();
    });
    assert_eq!(events, crate::test_support::AllocatorEvents::default());

    probe.checkpoint_next_terminal_tail();
    let ((), events) = crate::test_support::measured_allocator_events(|| {
        attachment.update(unknown).unwrap();
    });
    assert_eq!(events, crate::test_support::AllocatorEvents::default());
    assert_eq!(probe.intent_counts().set_all, 1);
    assert_eq!(probe.intent_counts().revoke_all, 1);
    assert_eq!(probe.intent_counts().confirm_exact, 1);
}

#[test]
fn warmed_attachment_complete_lifecycle_has_no_allocator_events() {
    let owner = allocator_owner();
    let (sink, probe) = allocator_point_sink(900);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let mut attachment = owner.attach(1, &emissions, &presentations, sink).unwrap();
    let white = [Srgb8::new([0xFF; 3])];
    let light = [Srgb8::new([0xF0; 3])];
    let black = [Srgb8::new([0; 3])];
    let ready_scenarios = [ScenarioV1::new(41, &white), ScenarioV1::new(42, &light)];
    let conflict_scenarios = [ScenarioV1::new(41, &white), ScenarioV1::new(43, &black)];

    // Отдельная заведомая аллокация доказывает работоспособность наблюдателя,
    // не превращая аллокацию холодного пути Core в вечный контракт.
    let (sentinel, observer_control) =
        crate::test_support::measured_allocator_events(|| Box::new(0_u8));
    assert_eq!(*sentinel, 0);
    assert_ne!(
        observer_control,
        crate::test_support::AllocatorEvents::default(),
        "the allocator observer must see a known sentinel allocation",
    );
    drop(sentinel);

    assert_eq!(
        attachment
            .update(observed(1, &ready_scenarios))
            .unwrap()
            .evidence()
            .kind(),
        StateKindV1::Ready,
    );

    // Три наблюдения равной мощности доказывают high-water автомата:
    // Ready A, Failed(cause B + previous A), затем prospective/committed Ready C.
    assert_eq!(
        attachment
            .update(observed(2, &conflict_scenarios))
            .unwrap()
            .evidence()
            .kind(),
        StateKindV1::Failed,
    );
    assert_eq!(
        attachment
            .update(observed(3, &ready_scenarios))
            .unwrap()
            .evidence()
            .kind(),
        StateKindV1::Ready,
    );

    let (failed, events) = crate::test_support::measured_allocator_events(|| {
        attachment
            .update(observed(4, &conflict_scenarios))
            .map(|committed| committed.evidence().kind())
    });
    assert_eq!(failed.unwrap(), StateKindV1::Failed);
    assert_eq!(
        events,
        crate::test_support::AllocatorEvents::default(),
        "warmed Ready -> Failed must reuse observation, report and output arenas",
    );

    let (ready, events) = crate::test_support::measured_allocator_events(|| {
        attachment
            .update(observed(5, &ready_scenarios))
            .map(|committed| committed.evidence().kind())
    });
    assert_eq!(ready.unwrap(), StateKindV1::Ready);
    assert_eq!(
        events,
        crate::test_support::AllocatorEvents::default(),
        "warmed Failed -> Ready must retire two witnesses without allocator traffic",
    );

    let unknown = UpdateV1::Unknown {
        revision: 6,
        reason_id: 77,
    };
    let (stale, events) = crate::test_support::measured_allocator_events(|| {
        attachment
            .update(unknown)
            .map(|committed| committed.evidence().kind())
    });
    assert_eq!(stale.unwrap(), StateKindV1::Stale);
    assert_eq!(
        events,
        crate::test_support::AllocatorEvents::default(),
        "Unknown -> Stale must retain the verified arena without allocation or release",
    );

    let (confirmed_unknown, events) = crate::test_support::measured_allocator_events(|| {
        attachment
            .update(unknown)
            .map(|committed| committed.evidence().kind())
    });
    assert_eq!(confirmed_unknown.unwrap(), StateKindV1::Stale);
    assert_eq!(
        events,
        crate::test_support::AllocatorEvents::default(),
        "ConfirmExact over a closed Unknown snapshot must not acquire an arena",
    );

    let (ready, events) = crate::test_support::measured_allocator_events(|| {
        attachment
            .update(observed(7, &ready_scenarios))
            .map(|committed| committed.evidence().kind())
    });
    assert_eq!(ready.unwrap(), StateKindV1::Ready);
    assert_eq!(events, crate::test_support::AllocatorEvents::default());

    let (confirmed_ready, events) = crate::test_support::measured_allocator_events(|| {
        attachment
            .update(observed(7, &ready_scenarios))
            .map(|committed| committed.evidence().kind())
    });
    assert_eq!(confirmed_ready.unwrap(), StateKindV1::Ready);
    assert_eq!(
        events,
        crate::test_support::AllocatorEvents::default(),
        "ConfirmExact over a published snapshot must not evaluate or acquire an arena",
    );

    let committed_entry = probe.entry();
    let committed_stamp = probe.stamp();
    probe.reject_next_prepare();
    let (rejected, events) = crate::test_support::measured_allocator_events(|| {
        attachment.update(observed(8, &ready_scenarios)).map(|_| ())
    });
    assert!(matches!(
        rejected,
        Err(AttachmentUpdateErrorV1::SinkPrepare(
            InMemoryPointSinkErrorV1::RejectedPrepare
        ))
    ));
    assert_eq!(events, crate::test_support::AllocatorEvents::default());
    assert_eq!(attachment.committed_revision, Some(7));
    assert_eq!(probe.revision(), Some(7));
    assert_eq!(probe.entry(), committed_entry);
    assert_eq!(probe.stamp(), committed_stamp);
    assert!(!probe.is_busy());

    probe.reject_next_install();
    let (rejected, events) = crate::test_support::measured_allocator_events(|| {
        attachment.update(observed(8, &ready_scenarios)).map(|_| ())
    });
    assert!(matches!(
        rejected,
        Err(AttachmentUpdateErrorV1::SinkInstall(
            InMemoryPointSinkErrorV1::RejectedInstall
        ))
    ));
    assert_eq!(events, crate::test_support::AllocatorEvents::default());
    assert_eq!(attachment.committed_revision, Some(7));
    assert_eq!(probe.revision(), Some(7));
    assert_eq!(probe.entry(), committed_entry);
    assert_eq!(probe.stamp(), committed_stamp);
    assert!(!probe.is_busy());

    probe.reject_next_install_after_swap();
    let (rejected, events) = crate::test_support::measured_allocator_events(|| {
        attachment.update(observed(8, &ready_scenarios)).map(|_| ())
    });
    assert!(matches!(
        rejected,
        Err(AttachmentUpdateErrorV1::SinkInstall(
            InMemoryPointSinkErrorV1::RejectedInstallAfterSwap
        ))
    ));
    assert_eq!(events, crate::test_support::AllocatorEvents::default());
    assert_eq!(attachment.committed_revision, Some(7));
    assert_eq!(probe.revision(), Some(7));
    assert_eq!(probe.entry(), committed_entry);
    assert_eq!(probe.stamp(), committed_stamp);
    assert!(!probe.is_busy());

    let (retried, events) = crate::test_support::measured_allocator_events(|| {
        attachment
            .update(observed(8, &ready_scenarios))
            .map(|committed| committed.evidence().kind())
    });
    assert_eq!(retried.unwrap(), StateKindV1::Ready);
    assert_eq!(
        events,
        crate::test_support::AllocatorEvents::default(),
        "all rejected prospective leases must return to the same warmed Session",
    );
    assert_eq!(attachment.committed_revision, Some(8));
    assert_eq!(probe.revision(), Some(8));
    assert!(!probe.is_busy());
}

#[test]
fn session_hot_path_contains_no_retired_per_update_storage_constructors() {
    let compact = crate::generic_boundary_tests::compact_production_syntax;
    let observation_source = include_str!("../../observation.rs");
    let observation = compact(observation_source);
    for retired in [
        "cases: cases.into_boxed_slice()",
        "values: values.into_boxed_slice()",
        "provenance: provenance.into_boxed_slice()",
        "backing: Rc::new(ObservationBackingV1",
    ] {
        let retired = compact(retired);
        assert!(
            !observation.contains(&retired),
            "observation hot path still constructs retired per-update storage: {retired}",
        );
    }
    assert!(
        crate::generic_boundary_tests::observation_backing_allocation_is_pool_scoped(
            observation_source,
        ),
        "ObservationBackingV1 may be allocated only by the three-slot pool constructor",
    );

    let evaluation = compact(include_str!("../../program_session.rs"));
    assert!(
        !evaluation.contains("ProgramReportBuffersV1"),
        "the retired selected/exhaustive report-buffer type must not return",
    );
    for retired in [
        "let mut selected = ProgramReportBuffersV1::empty()",
        "let mut exhaustive_conflict = ProgramReportBuffersV1::empty()",
        "let mut outputs = Vec::new()",
        "std::mem::replace(&mut self.selected, ProgramReportBuffersV1::empty())",
        "std::mem::take(&mut self.outputs)",
    ] {
        let retired = compact(retired);
        assert!(
            !evaluation.contains(&retired),
            "evaluation hot path still constructs or detaches retired per-update storage: {retired}",
        );
    }
}

fn allocator_owner() -> OwnerV1 {
    const BLACK: TargetCandidateIdV1 = TargetCandidateIdV1::new(101);
    const GRAY: TargetCandidateIdV1 = TargetCandidateIdV1::new(102);

    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Dim).unwrap();
    let mut draft = DraftV1::new();
    draft.push_source(SOURCE, Srgb8::new([0; 3]));
    draft.push_finite_target(
        TARGET,
        SOURCE,
        vec![
            TargetCandidateV1::new(BLACK, Srgb8::new([0; 3])),
            TargetCandidateV1::new(GRAY, Srgb8::new([0x80; 3])),
        ],
    );
    draft
        .set_joint_selection(vec![
            JointStateV1::new(vec![JointChoiceV1::new(TARGET, BLACK)]),
            JointStateV1::new(vec![JointChoiceV1::new(TARGET, GRAY)]),
        ])
        .unwrap();
    draft.push_surface_input_port(INPUT);
    draft.push_solid_paint(PAINT, TARGET);
    draft.push_input_surface(INPUT_SURFACE, INPUT);
    draft.push_source_over_occurrence(INNER, PAINT, INPUT_SURFACE, context);
    draft.push_point_presentation_root(ROOT, INNER);
    draft.push_point_presentation_target(ROOT, INNER);
    draft.push_wcag22_hard(
        ConstraintIdV1::new(10),
        INNER,
        Wcag22CriterionV1::Sc143TextDefault,
    );
    draft.push_output(OUTPUT_A, PAINT);
    draft.compile().unwrap()
}

fn owner(
    source: Srgb8,
    expected: Srgb8,
    extra_topology: bool,
    outputs: &[OutputSlotIdV1],
) -> OwnerV1 {
    owner_with_terminal_presentation(source, expected, extra_topology, outputs, false)
}

fn owner_with_terminal_presentation(
    source: Srgb8,
    expected: Srgb8,
    extra_topology: bool,
    outputs: &[OutputSlotIdV1],
    include_terminal: bool,
) -> OwnerV1 {
    let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Dim).unwrap();
    let inner_surface = SurfaceIdV1::new(7);
    let middle_surface = SurfaceIdV1::new(16);
    let has_middle = extra_topology || outputs.len() > 1;
    let mut draft = DraftV1::new();
    draft.push_source(SOURCE, source);
    draft.push_fixed_target(TARGET, SOURCE);
    draft.push_surface_input_port(INPUT);
    draft.push_solid_paint(PAINT, TARGET);
    draft.push_input_surface(INPUT_SURFACE, INPUT);
    draft.push_source_over_occurrence(INNER, PAINT, INPUT_SURFACE, context);
    draft.push_occurrence_surface(inner_surface, INNER);
    if has_middle {
        draft.push_source_over_occurrence(MIDDLE, PAINT, inner_surface, context);
        draft.push_occurrence_surface(middle_surface, MIDDLE);
        draft.push_source_over_occurrence(TERMINAL, PAINT, middle_surface, context);
    } else {
        draft.push_source_over_occurrence(TERMINAL, PAINT, inner_surface, context);
    }
    draft.push_point_presentation_root(ROOT, TERMINAL);
    draft.push_point_presentation_target(ROOT, INNER);
    if has_middle {
        draft.push_point_presentation_target(ROOT, MIDDLE);
    }
    if include_terminal {
        draft.push_point_presentation_target(ROOT, TERMINAL);
    }
    draft.push_exact_hard(ConstraintIdV1::new(10), INNER, expected);
    for output in outputs {
        draft.push_output(*output, PAINT);
    }
    draft.compile().unwrap()
}

fn observed<'a>(revision: u64, scenarios: &'a [ScenarioV1<'a>]) -> UpdateV1<'a> {
    UpdateV1::Observed {
        revision,
        scenarios,
    }
}

fn contract_error<L>(
    result: Result<Attachment<L::Closed>, AttachmentCreateFailureV1<L>>,
) -> AttachmentCreateErrorV1<L::OutputId>
where
    L: UnboundPointSinkLeaseV1,
{
    let failure = match result {
        Ok(_) => panic!("cold contract error was expected"),
        Err(failure) => failure,
    };
    match failure.into_parts().0 {
        AttachmentCreateCauseV1::Contract(cause) => cause,
        AttachmentCreateCauseV1::SinkAdmission(_) => {
            panic!("contract test reached host admission")
        }
    }
}

#[test]
fn attach_rejects_missing_extra_duplicate_and_accepts_reordered_sink_scope() {
    let owner = owner(
        Srgb8::new([0, 0, 0]),
        Srgb8::new([0, 0, 0]),
        false,
        &[OUTPUT_A, OUTPUT_B],
    );
    let emissions = [
        authored_emission(OUTPUT_B.value(), 901),
        authored_emission(OUTPUT_A.value(), 900),
    ];
    let presentations = [
        authored_presentation(OUTPUT_B.value(), ROOT.value(), MIDDLE.value()),
        authored_presentation(OUTPUT_A.value(), ROOT.value(), INNER.value()),
    ];

    let (missing, missing_probe) = in_memory_point_sink(&[900]);
    assert!(matches!(
        contract_error(owner.attach(1, &emissions, &presentations, missing)),
        AttachmentCreateErrorV1::SinkScopeCount {
            expected: 2,
            actual: 1
        }
    ));
    assert_eq!(missing_probe.revoke_count(), 0);
    assert!(missing_probe.ambient_fallback_is_exposed());

    let (extra, _) = in_memory_point_sink(&[900, 901, 902]);
    assert!(matches!(
        contract_error(owner.attach(1, &emissions, &presentations, extra)),
        AttachmentCreateErrorV1::SinkScopeCount {
            expected: 2,
            actual: 3
        }
    ));

    let (duplicate, _) = in_memory_point_sink(&[900, 900]);
    assert!(matches!(
        contract_error(owner.attach(1, &emissions, &presentations, duplicate)),
        AttachmentCreateErrorV1::DuplicateSinkScopeOutput { .. }
    ));

    let (reordered, reordered_probe) = in_memory_point_sink(&[901, 900]);
    let reordered = owner
        .attach(1, &emissions, &presentations, reordered)
        .unwrap();
    assert!(reordered_probe.is_closed());
    reordered.dispose();

    let duplicate_emission = [
        authored_emission(OUTPUT_A.value(), 900),
        authored_emission(OUTPUT_B.value(), 900),
    ];
    let (sink, _) = in_memory_point_sink(&[900, 901]);
    assert!(matches!(
        contract_error(owner.attach(1, &duplicate_emission, &presentations, sink)),
        AttachmentCreateErrorV1::SinkOutputAliased { .. }
    ));
}

#[test]
fn attach_requires_exact_bijection_over_compiled_presentations() {
    let owner = owner_with_terminal_presentation(
        Srgb8::new([0, 0, 0]),
        Srgb8::new([0, 0, 0]),
        false,
        &[OUTPUT_A, OUTPUT_B],
        true,
    );
    let emissions = [
        authored_emission(OUTPUT_A.value(), 900),
        authored_emission(OUTPUT_B.value(), 901),
    ];
    let omitted_terminal = [
        authored_presentation(OUTPUT_A.value(), ROOT.value(), INNER.value()),
        authored_presentation(OUTPUT_B.value(), ROOT.value(), MIDDLE.value()),
    ];
    let (sink, probe) = in_memory_point_sink(&[900, 901]);
    assert!(matches!(
        contract_error(owner.attach(2, &emissions, &omitted_terminal, sink)),
        AttachmentCreateErrorV1::PresentationCount {
            expected: 3,
            actual: 2
        }
    ));
    assert_eq!(probe.revoke_count(), 0);
    assert!(probe.ambient_fallback_is_exposed());
}

#[test]
fn alias_outputs_cannot_claim_the_same_compiled_presentation() {
    let owner = owner(
        Srgb8::new([0, 0, 0]),
        Srgb8::new([0, 0, 0]),
        false,
        &[OUTPUT_A, OUTPUT_B],
    );
    let emissions = [
        authored_emission(OUTPUT_A.value(), 900),
        authored_emission(OUTPUT_B.value(), 901),
    ];
    let duplicate_actual_target = [
        authored_presentation(OUTPUT_A.value(), ROOT.value(), INNER.value()),
        authored_presentation(OUTPUT_B.value(), ROOT.value(), INNER.value()),
    ];
    let (sink, _) = in_memory_point_sink(&[900, 901]);
    assert!(matches!(
        contract_error(owner.attach(3, &emissions, &duplicate_actual_target, sink)),
        AttachmentCreateErrorV1::DuplicatePresentation {
            root: ROOT,
            occurrence: INNER,
            first_output: OUTPUT_A,
            second_output: OUTPUT_B,
        }
    ));
}

#[test]
fn every_emission_requires_at_least_one_distinct_compiled_presentation() {
    let owner = owner(
        Srgb8::new([0, 0, 0]),
        Srgb8::new([0, 0, 0]),
        false,
        &[OUTPUT_A, OUTPUT_B],
    );
    let emissions = [
        authored_emission(OUTPUT_A.value(), 900),
        authored_emission(OUTPUT_B.value(), 901),
    ];
    let only_output_a = [
        authored_presentation(OUTPUT_A.value(), ROOT.value(), INNER.value()),
        authored_presentation(OUTPUT_A.value(), ROOT.value(), MIDDLE.value()),
    ];
    let (sink, _) = in_memory_point_sink(&[900, 901]);

    assert!(matches!(
        contract_error(owner.attach(4, &emissions, &only_output_a, sink)),
        AttachmentCreateErrorV1::MissingOutputPresentation { output: OUTPUT_B }
    ));
}

#[test]
fn duplicate_emission_output_has_its_exact_typed_error() {
    let owner = owner(
        Srgb8::new([0, 0, 0]),
        Srgb8::new([0, 0, 0]),
        false,
        &[OUTPUT_A, OUTPUT_B],
    );
    let duplicate_output = [
        authored_emission(OUTPUT_A.value(), 900),
        authored_emission(OUTPUT_A.value(), 901),
    ];
    let presentations = [
        authored_presentation(OUTPUT_A.value(), ROOT.value(), INNER.value()),
        authored_presentation(OUTPUT_B.value(), ROOT.value(), MIDDLE.value()),
    ];
    let (sink, _) = in_memory_point_sink(&[900, 901]);

    assert!(matches!(
        contract_error(owner.attach(5, &duplicate_output, &presentations, sink)),
        AttachmentCreateErrorV1::DuplicateEmissionOutput { output: OUTPUT_A }
    ));
}

#[test]
fn verified_snapshot_mints_attached_render_output_and_exact_confirm_only_for_idempotence() {
    let owner = owner(
        Srgb8::new([12, 34, 56]),
        Srgb8::new([12, 34, 56]),
        false,
        &[OUTPUT_A, OUTPUT_B],
    );
    let (sink, probe) = in_memory_point_sink(&[900, 901]);
    let emissions = [
        authored_emission(OUTPUT_B.value(), 901),
        authored_emission(OUTPUT_A.value(), 900),
    ];
    let presentations = [
        authored_presentation(OUTPUT_B.value(), ROOT.value(), MIDDLE.value()),
        authored_presentation(OUTPUT_A.value(), ROOT.value(), INNER.value()),
    ];
    let mut attachment = owner.attach(7, &emissions, &presentations, sink).unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(44, &values)];

    {
        let committed = attachment.update(observed(1, &scenarios)).unwrap();
        assert_eq!(committed.evidence().kind(), StateKindV1::Ready);
        let outputs: Vec<_> = committed.render_outputs().collect();
        assert_eq!(outputs.len(), 2);
        let output = outputs[0];
        assert_eq!(output.certificate().observation().revision(), 1);
        assert_eq!(output.output(), OUTPUT_A);
        assert_eq!(output.paint().source(), Srgb8::new([12, 34, 56]));
        assert_eq!(output.root(), ROOT);
        assert_eq!(output.occurrence(), INNER);
        assert_eq!(output.sink_output().value(), 900);
        assert_eq!(output.published_stamp().revision(), 1);
        assert_eq!(outputs[1].output(), OUTPUT_B);
        assert_eq!(outputs[1].occurrence(), MIDDLE);
        assert_eq!(outputs[1].sink_output().value(), 901);
    }
    assert_eq!(probe.intent_counts().set_all, 1);
    assert_eq!(probe.intent_counts().confirm_exact, 0);

    {
        let committed = attachment.update(observed(1, &scenarios)).unwrap();
        assert_eq!(committed.render_outputs().len(), 2);
    }
    assert_eq!(probe.intent_counts().set_all, 1);
    assert_eq!(probe.intent_counts().confirm_exact, 1);

    attachment.update(observed(2, &scenarios)).unwrap();
    assert_eq!(probe.intent_counts().set_all, 2);
    assert_eq!(probe.intent_counts().confirm_exact, 1);
    assert_eq!(probe.revision(), Some(2));
}

#[test]
fn one_emission_fans_out_to_every_distinct_attached_presentation() {
    let owner = owner(
        Srgb8::new([21, 22, 23]),
        Srgb8::new([21, 22, 23]),
        true,
        &[OUTPUT_A],
    );
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [
        authored_presentation(OUTPUT_A.value(), ROOT.value(), MIDDLE.value()),
        authored_presentation(OUTPUT_A.value(), ROOT.value(), INNER.value()),
    ];
    let mut attachment = owner.attach(70, &emissions, &presentations, sink).unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(44, &values)];

    let committed = attachment.update(observed(1, &scenarios)).unwrap();
    assert_eq!(probe.snapshot().len(), 1);
    assert_eq!(probe.snapshot()[0].output(), OUTPUT_A);
    let outputs: Vec<_> = committed.render_outputs().collect();
    assert_eq!(outputs.len(), 2);
    assert_eq!(outputs[0].occurrence(), INNER);
    assert_eq!(outputs[1].occurrence(), MIDDLE);
    assert!(
        outputs
            .iter()
            .all(|output| output.output() == OUTPUT_A && output.sink_output().value() == 900)
    );
}

#[test]
fn unknown_and_known_violation_revoke_the_complete_snapshot() {
    let pass_owner = owner(
        Srgb8::new([0, 0, 0]),
        Srgb8::new([0, 0, 0]),
        false,
        &[OUTPUT_A],
    );
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let mut attachment = pass_owner
        .attach(8, &emissions, &presentations, sink)
        .unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];
    attachment.update(observed(1, &scenarios)).unwrap();
    assert_eq!(probe.snapshot().len(), 1);

    let unknown = UpdateV1::Unknown {
        revision: 2,
        reason_id: 77,
    };
    let committed = attachment.update(unknown).unwrap();
    assert_eq!(committed.evidence().kind(), StateKindV1::Stale);
    assert_eq!(committed.render_outputs().len(), 0);
    assert!(probe.snapshot().is_empty());
    assert!(probe.is_closed());
    assert!(!probe.ambient_fallback_is_exposed());
    assert_eq!(probe.intent_counts().revoke_all, 1);
    attachment.update(unknown).unwrap();
    assert_eq!(probe.intent_counts().confirm_exact, 1);

    let conflict_owner = owner(
        Srgb8::new([255, 0, 0]),
        Srgb8::new([0, 0, 0]),
        false,
        &[OUTPUT_A],
    );
    let (sink, conflict_probe) = in_memory_point_sink(&[900]);
    let mut conflict = conflict_owner
        .attach(9, &emissions, &presentations, sink)
        .unwrap();
    let committed = conflict.update(observed(1, &scenarios)).unwrap();
    assert_eq!(committed.evidence().kind(), StateKindV1::Failed);
    assert_eq!(committed.render_outputs().len(), 0);
    assert!(conflict_probe.snapshot().is_empty());
    assert!(conflict_probe.is_closed());
    assert!(!conflict_probe.ambient_fallback_is_exposed());
    assert_eq!(conflict_probe.intent_counts().revoke_all, 1);
}

#[test]
fn rejected_install_keeps_session_snapshot_and_releases_busy() {
    let owner = owner(
        Srgb8::new([4, 5, 6]),
        Srgb8::new([4, 5, 6]),
        false,
        &[OUTPUT_A],
    );
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let mut attachment = owner.attach(10, &emissions, &presentations, sink).unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];
    attachment.update(observed(1, &scenarios)).unwrap();
    let snapshot = probe.snapshot();

    probe.reject_next_install();
    assert!(matches!(
        attachment.update(observed(2, &scenarios)),
        Err(AttachmentUpdateErrorV1::SinkInstall(
            InMemoryPointSinkErrorV1::RejectedInstall
        ))
    ));
    assert!(!probe.is_busy());
    assert_eq!(probe.revision(), Some(1));
    assert_eq!(probe.snapshot(), snapshot);
    match attachment.session.evidence().observation_head() {
        super::super::ObservationHeadV1::Observed { stream, revision } => {
            assert_eq!(stream.value(), 10);
            assert_eq!(revision, 1);
        }
        _ => panic!("rejected install must preserve the prior observed head"),
    }

    attachment.update(observed(1, &scenarios)).unwrap();
    assert_eq!(probe.intent_counts().confirm_exact, 1);
}

#[test]
fn every_fallible_sink_boundary_is_all_or_nothing() {
    let owner = owner(
        Srgb8::new([4, 5, 6]),
        Srgb8::new([4, 5, 6]),
        false,
        &[OUTPUT_A],
    );
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let mut attachment = owner.attach(73, &emissions, &presentations, sink).unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];
    let initial_stamp = probe.stamp();

    probe.reject_next_install_after_swap();
    assert!(matches!(
        attachment.update(observed(1, &scenarios)),
        Err(AttachmentUpdateErrorV1::SinkInstall(
            InMemoryPointSinkErrorV1::RejectedInstallAfterSwap
        ))
    ));
    assert!(probe.snapshot().is_empty());
    assert_eq!(probe.revision(), None);
    assert_eq!(probe.stamp(), initial_stamp);
    assert!(!probe.is_busy());
    assert!(matches!(
        attachment.session.evidence().observation_head(),
        super::super::ObservationHeadV1::Empty
    ));

    attachment.update(observed(1, &scenarios)).unwrap();
    let snapshot = probe.snapshot();
    let stamp = probe.stamp();

    probe.reject_next_prepare();
    assert!(matches!(
        attachment.update(observed(2, &scenarios)),
        Err(AttachmentUpdateErrorV1::SinkPrepare(
            InMemoryPointSinkErrorV1::RejectedPrepare
        ))
    ));
    assert_eq!(probe.snapshot(), snapshot);
    assert_eq!(probe.revision(), Some(1));
    assert_eq!(probe.stamp(), stamp);
    assert!(!probe.is_busy());
    assert!(probe.rejected_prepare_saw_busy());

    probe.reject_next_install_after_swap();
    let unknown = UpdateV1::Unknown {
        revision: 2,
        reason_id: 99,
    };
    assert!(matches!(
        attachment.update(unknown),
        Err(AttachmentUpdateErrorV1::SinkInstall(
            InMemoryPointSinkErrorV1::RejectedInstallAfterSwap
        ))
    ));
    assert_eq!(probe.snapshot(), snapshot);
    assert_eq!(probe.revision(), Some(1));
    assert_eq!(probe.stamp(), stamp);
    assert!(!probe.is_busy());
    match attachment.session.evidence().observation_head() {
        super::super::ObservationHeadV1::Observed { stream, revision } => {
            assert_eq!(stream.value(), 73);
            assert_eq!(revision, 1);
        }
        _ => panic!("fallible sink boundary must preserve the prior observed head"),
    }

    let committed = attachment.update(observed(1, &scenarios)).unwrap();
    assert_eq!(committed.render_outputs().len(), 1);
    assert_eq!(probe.intent_counts().confirm_exact, 1);
}

#[test]
fn installed_retirement_waits_for_the_next_preinstall_drain_and_retry_is_clean() {
    let owner = owner(
        Srgb8::new([4, 5, 6]),
        Srgb8::new([4, 5, 6]),
        false,
        &[OUTPUT_A],
    );
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let mut attachment = owner.attach(71, &emissions, &presentations, sink).unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];

    probe.panic_on_next_retirement_drop();
    let first = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let committed = attachment.update(observed(1, &scenarios)).unwrap();
        assert_eq!(committed.evidence().kind(), StateKindV1::Ready);
    }));
    assert!(
        first.is_ok(),
        "post-install finish must only park retirement"
    );
    assert_eq!(probe.retirement_drop_count(), 0);
    assert_eq!(probe.revision(), Some(1));
    assert!(!probe.is_busy());
    let installed_snapshot = probe.snapshot();

    let drain = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = attachment.update(observed(2, &scenarios));
    }));
    assert!(drain.is_err(), "hostile retirement destructor must run");
    assert_eq!(probe.retirement_drop_count(), 1);
    assert_eq!(probe.revision(), Some(1));
    assert_eq!(probe.snapshot(), installed_snapshot);
    assert!(!probe.is_busy());
    match attachment.session.evidence().observation_head() {
        super::super::ObservationHeadV1::Observed { stream, revision } => {
            assert_eq!(stream.value(), 71);
            assert_eq!(revision, 1);
        }
        _ => panic!("pre-install retirement panic must preserve Session"),
    }

    attachment.update(observed(2, &scenarios)).unwrap();
    assert_eq!(probe.revision(), Some(2));
    assert!(!probe.is_busy());
}

#[test]
fn source_guards_keep_the_post_install_tail_destructor_free() {
    let attachment_source = include_str!("../attachment.rs");
    let session_source = include_str!("../../session.rs");
    let support_source = include_str!("support.rs");

    let cold_prepare = attachment_source
        .find("PreparedAttachmentColdV1::try_new(")
        .expect("all fallible Core preparation must precede host admission");
    let admission = attachment_source
        .find("sink.try_admit_closed(permit)")
        .expect("Attachment must cross one closed admission seam");
    assert!(cold_prepare < admission);
    let post_admission_function = attachment_source
        .split("fn from_closed_admission(")
        .nth(1)
        .expect("post-admission построение должно иметь отдельную типизированную функцию");
    let post_admission_signature = post_admission_function
        .split("// POST_ADMISSION_TAIL_START_V1")
        .next()
        .expect("маркер начала post-admission tail должен следовать после сигнатуры");
    assert!(post_admission_signature.contains("-> Self"));
    let post_admission = post_admission_function
        .split("// POST_ADMISSION_TAIL_START_V1")
        .nth(1)
        .expect("маркер начала post-admission tail обязателен")
        .split("// POST_ADMISSION_TAIL_END_V1")
        .next()
        .expect("маркер конца post-admission tail обязателен");
    for forbidden in [
        "try_reserve",
        ".map_err",
        ".instantiate(",
        ".unwrap(",
        ".expect(",
        "panic!",
        "drop(",
    ] {
        assert!(
            !post_admission.contains(forbidden),
            "post-admission построение содержит fallible/destructive operation: {forbidden}",
        );
    }
    assert!(post_admission.contains("admission.into_parts()"));
    assert!(!post_admission.contains(".current_stamp("));
    assert!(attachment_source.contains("fn close_before_release(&mut self);"));
    assert!(attachment_source.contains("self.sink.close_before_release();"));

    assert!(
        attachment_source.contains("transition.commit_deferred()"),
        "Attachment must publish through the deferred Session commit seam",
    );
    assert!(
        !attachment_source.contains("let _ = transition.commit();"),
        "eager Session commit must not return to the post-install tail",
    );
    assert!(
        !attachment_source.contains("retired_session"),
        "Attachment must not duplicate Session-owned deferred retirement",
    );
    let session_owner = session_source
        .split("pub(crate) struct Session<Plan: SessionPlanV1> {")
        .nth(1)
        .expect("generic Session owner must exist")
        .split("impl<Plan: SessionPlanV1> Session<Plan>")
        .next()
        .expect("Session owner fields must precede its implementation");
    assert_eq!(
        session_owner
            .matches("deferred_retirement: Option<DeferredSessionRetirement<Plan>>,")
            .count(),
        1,
        "Session must own exactly one deferred-retirement slot",
    );
    let session_prepare = session_source
        .split("pub(crate) fn prepare_update(")
        .nth(1)
        .expect("Session must prepare updates")
        .split("/// Stream-affine `Unknown` admission")
        .next()
        .expect("prepare_update body must be bounded");
    assert!(
        session_prepare
            .find("self.drain_deferred_retirement();")
            .expect("Session prepare must drain deferred retirement")
            < session_prepare
                .find(".try_acquire_owner()")
                .expect("Session prepare must acquire the exact owner"),
        "deferred retirement must drain before owner acquisition and admission",
    );
    let deferred_commit = session_source
        .split("pub(crate) fn commit_deferred(self)")
        .nth(1)
        .expect("Session must expose its internal deferred-commit seam")
        .split("fn publish_session_transition")
        .next()
        .expect("commit_deferred body must precede publication helper");
    assert!(
        deferred_commit.contains("*deferred_retirement = Some(retirement);"),
        "deferred commit must park retirement inside Session",
    );

    let in_memory_sink = support_source
        .split("impl ClosedPointSinkLeaseV1 for ClosedInMemoryPointSinkLeaseV1 {")
        .nth(1)
        .expect("in-memory test sink must implement the closed lease")
        .split("impl PreparedPointSinkWriteV1 for InMemoryPreparedPointSinkWriteV1")
        .next()
        .expect("closed-lease implementation must precede prepared-write implementation");
    let sink_prepare = in_memory_sink
        .split("fn prepare<'lease>(")
        .nth(1)
        .expect("in-memory test sink must implement prepare")
        .split("fn close_before_release")
        .next()
        .expect("prepare body must precede close implementation");
    assert!(
        sink_prepare
            .find("drop(self.retired.take())")
            .expect("prepare must drain retired sink state")
            < sink_prepare
                .find("self.shared.busy.set(true)")
                .expect("prepare must acquire Busy"),
        "retired sink state must drain before Busy is acquired",
    );

    let in_memory_prepared = support_source
        .split("impl PreparedPointSinkWriteV1 for InMemoryPreparedPointSinkWriteV1")
        .nth(1)
        .expect("in-memory prepared sink must implement the prepared write")
        .split("impl Drop for InMemoryPreparedPointSinkWriteV1")
        .next()
        .expect("prepared-write implementation must precede prepared-sink Drop");
    let finish = in_memory_prepared
        .split("fn finish_after_session(mut self)")
        .nth(1)
        .expect("prepared sink must implement finish_after_session");
    assert!(
        !finish.contains("drop("),
        "post-install finish must not run a destructor",
    );
    assert!(
        !finish.contains("borrow_mut"),
        "post-install finish must not enter a fallible RefCell borrow",
    );
    for owning_take in [
        "_staging: self.staging.take()",
        "probe: self.retirement_probe.take()",
    ] {
        assert!(
            finish.contains(owning_take),
            "finish must park every owning field: {owning_take}"
        );
    }
    assert!(
        finish
            .find("self.lease.retired = Some(retirement)")
            .expect("finish must park retirement")
            < finish
                .find("self.lease.shared.busy.set(false)")
                .expect("finish must release Busy"),
        "finish must park every retired owner before releasing Busy",
    );
}

#[test]
fn dispose_revokes_before_hostile_retirement_destructor_runs() {
    let owner = owner(
        Srgb8::new([4, 5, 6]),
        Srgb8::new([4, 5, 6]),
        false,
        &[OUTPUT_A],
    );
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let mut attachment = owner.attach(72, &emissions, &presentations, sink).unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];

    probe.panic_on_next_retirement_drop();
    attachment.update(observed(1, &scenarios)).unwrap();
    assert_eq!(probe.retirement_drop_count(), 0);

    let disposed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        attachment.dispose();
    }));
    assert!(disposed.is_err(), "hostile retirement destructor must run");
    assert_eq!(probe.retirement_drop_count(), 1);
    assert_eq!(probe.revoke_count(), 1);
    assert!(probe.snapshot().is_empty());
    assert!(probe.is_closed());
    assert!(!probe.ambient_fallback_is_exposed());
    assert!(probe.revoked_before_lease_drop());
}

#[test]
fn same_ids_and_ordinals_cannot_pair_a_foreign_generation_token_with_the_pin() {
    let owner_a = owner(
        Srgb8::new([0, 0, 0]),
        Srgb8::new([0, 0, 0]),
        false,
        &[OUTPUT_A],
    );
    let owner_b = owner(
        Srgb8::new([255, 0, 0]),
        Srgb8::new([255, 0, 0]),
        true,
        &[OUTPUT_A],
    );
    let token_a = owner_a
        .compiled
        .bind_point_output_presentation(OUTPUT_A.into_core(), ROOT.into_core(), INNER.into_core())
        .unwrap();
    let token_b = owner_b
        .compiled
        .bind_point_output_presentation(OUTPUT_A.into_core(), ROOT.into_core(), INNER.into_core())
        .unwrap();
    assert_eq!(token_a.output(), token_b.output());
    assert_eq!(token_a.root(), token_b.root());
    assert_eq!(token_a.occurrence(), token_b.occurrence());
    assert_eq!(token_a.output_ordinal(), token_b.output_ordinal());
    assert_eq!(
        token_a.presentation_ordinal(),
        token_b.presentation_ordinal()
    );
    assert_ne!(
        owner_a.content_identity().as_bytes(),
        owner_b.content_identity().as_bytes()
    );

    // `OwnerV1::attach` принимает только authored IDs и вместе минтит token_b
    // с его pin, поэтому для token_a в API нет позиции.
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [
        authored_presentation(OUTPUT_A.value(), ROOT.value(), INNER.value()),
        authored_presentation(OUTPUT_A.value(), ROOT.value(), MIDDLE.value()),
    ];
    let mut attachment = owner_b
        .attach(11, &emissions, &presentations, sink)
        .unwrap();
    drop(owner_a);
    drop(owner_b);

    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];
    attachment.update(observed(1, &scenarios)).unwrap();
    assert_eq!(
        probe.snapshot()[0].paint().source(),
        Srgb8::new([255, 0, 0])
    );
}

#[test]
fn dispose_revokes_before_lease_session_and_owner_pin_release() {
    let owner = owner(
        Srgb8::new([1, 2, 3]),
        Srgb8::new([1, 2, 3]),
        false,
        &[OUTPUT_A],
    );
    let (sink, probe) = in_memory_point_sink(&[900]);
    let emissions = [authored_emission(OUTPUT_A.value(), 900)];
    let presentations = [authored_presentation(
        OUTPUT_A.value(),
        ROOT.value(),
        INNER.value(),
    )];
    let mut attachment = owner.attach(12, &emissions, &presentations, sink).unwrap();
    drop(owner);
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(1, &values)];
    attachment.update(observed(1, &scenarios)).unwrap();
    assert_eq!(probe.snapshot().len(), 1);

    attachment.dispose();
    assert!(probe.snapshot().is_empty());
    assert!(probe.is_closed());
    assert!(!probe.ambient_fallback_is_exposed());
    assert_eq!(probe.revoke_count(), 1);
    assert!(probe.revoked_before_lease_drop());
}
