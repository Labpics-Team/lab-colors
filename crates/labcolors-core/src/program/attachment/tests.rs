use super::support::{
    InMemoryPointSinkErrorV1, authored_emission, authored_presentation, in_memory_point_sink,
};
use super::*;
use crate::Srgb8;
use crate::program::{
    AppearanceContextV1, ConstraintIdV1, DraftV1, PaintIdV1, ScenarioV1, SourceIdV1, StateKindV1,
    SurfaceIdV1, SurfaceInputPortIdV1, SurroundV1, TargetIdV1,
};

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

#[test]
fn attach_rejects_missing_extra_duplicate_and_reordered_sink_scope() {
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
        owner.attach(1, &emissions, &presentations, missing),
        Err(AttachmentCreateErrorV1::SinkScopeCount {
            expected: 2,
            actual: 1
        })
    ));
    assert_eq!(missing_probe.revoke_count(), 1);
    assert!(missing_probe.revoke_saw_exact_stamp());
    assert!(missing_probe.revoked_before_lease_drop());

    let (extra, _) = in_memory_point_sink(&[900, 901, 902]);
    assert!(matches!(
        owner.attach(1, &emissions, &presentations, extra),
        Err(AttachmentCreateErrorV1::SinkScopeCount {
            expected: 2,
            actual: 3
        })
    ));

    let (duplicate, _) = in_memory_point_sink(&[900, 900]);
    assert!(matches!(
        owner.attach(1, &emissions, &presentations, duplicate),
        Err(AttachmentCreateErrorV1::DuplicateSinkScopeOutput { .. })
    ));

    let (reordered, _) = in_memory_point_sink(&[901, 900]);
    assert!(matches!(
        owner.attach(1, &emissions, &presentations, reordered),
        Err(AttachmentCreateErrorV1::SinkScopeMismatch { ordinal: 0, .. })
    ));

    let duplicate_emission = [
        authored_emission(OUTPUT_A.value(), 900),
        authored_emission(OUTPUT_B.value(), 900),
    ];
    let (sink, _) = in_memory_point_sink(&[900, 901]);
    assert!(matches!(
        owner.attach(1, &duplicate_emission, &presentations, sink),
        Err(AttachmentCreateErrorV1::SinkOutputAliased { .. })
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
        owner.attach(2, &emissions, &omitted_terminal, sink),
        Err(AttachmentCreateErrorV1::PresentationCount {
            expected: 3,
            actual: 2
        })
    ));
    assert_eq!(probe.revoke_count(), 1);
    assert!(probe.revoked_before_lease_drop());
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
        owner.attach(3, &emissions, &duplicate_actual_target, sink),
        Err(AttachmentCreateErrorV1::DuplicatePresentation {
            root: ROOT,
            occurrence: INNER,
            first_output: OUTPUT_A,
            second_output: OUTPUT_B,
        })
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
        owner.attach(4, &emissions, &only_output_a, sink),
        Err(AttachmentCreateErrorV1::MissingOutputPresentation { output: OUTPUT_B })
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
        owner.attach(5, &duplicate_output, &presentations, sink),
        Err(AttachmentCreateErrorV1::DuplicateEmissionOutput { output: OUTPUT_A })
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
fn confirm_exact_rejects_a_sink_that_misreports_its_proposed_stamp() {
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
    let mut attachment = owner.attach(6, &emissions, &presentations, sink).unwrap();
    let values = [Srgb8::new([0, 0, 0])];
    let scenarios = [ScenarioV1::new(44, &values)];
    attachment.update(observed(1, &scenarios)).unwrap();
    let prior_snapshot = probe.snapshot();

    probe.misreport_next_confirm_proposed_stamp();
    assert!(matches!(
        attachment.update(observed(1, &scenarios)),
        Err(AttachmentUpdateErrorV1::InternalInvariant(
            AttachmentInvariantV1::ConfirmStampMismatch
        ))
    ));
    assert_eq!(probe.snapshot(), prior_snapshot);
    assert_eq!(probe.revision(), Some(1));
    assert!(!probe.is_busy());
    match attachment.session.evidence().observation_head() {
        super::super::ObservationHeadV1::Observed { stream, revision } => {
            assert_eq!(stream.value(), 6);
            assert_eq!(revision, 1);
        }
        _ => panic!("rejected confirm must preserve the prior observed head"),
    }

    let committed = attachment.update(observed(1, &scenarios)).unwrap();
    assert_eq!(committed.render_outputs().len(), 1);
    assert_eq!(probe.intent_counts().confirm_exact, 2);
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
    let support_source = include_str!("support.rs");

    assert!(attachment_source.contains("transition.commit_deferred()"));
    assert!(!attachment_source.contains("let _ = transition.commit();"));

    let session_drain = attachment_source
        .find("drop(self.retired_session.take())")
        .expect("Attachment must drain deferred Session retirement");
    let stamp_drain = attachment_source
        .find("drop(self.retired_stamp.take())")
        .expect("Attachment must drain deferred stamp retirement");
    let prepare = attachment_source
        .find(".prepare_update(update)")
        .expect("Attachment must prepare one Session transition");
    let install = attachment_source
        .find(".try_install()")
        .expect("Attachment must install the prepared sink transaction");
    assert!(session_drain < prepare && prepare < install);
    assert!(stamp_drain < prepare && prepare < install);

    let sink_prepare = support_source
        .split("fn prepare<'lease>(")
        .nth(1)
        .expect("test sink must implement prepare")
        .split("fn revoke_all_before_release")
        .next()
        .expect("prepare body must precede revoke implementation");
    assert!(
        sink_prepare
            .find("drop(self.retired.take())")
            .expect("prepare must drain retired sink state")
            < sink_prepare
                .find("self.shared.busy.set(true)")
                .expect("prepare must acquire Busy")
    );

    let finish = support_source
        .split("fn finish_after_session(mut self)")
        .nth(1)
        .expect("prepared sink must implement finish_after_session")
        .split("impl Drop for InMemoryPreparedPointSinkWriteV1")
        .next()
        .expect("finish body must precede prepared-sink Drop");
    assert!(!finish.contains("drop("));
    assert!(!finish.contains("borrow_mut"));
    for owning_take in [
        "_staging: self.staging.take()",
        "_retired_stamp: self.retired_stamp.take()",
        "_proposed: self.proposed.take()",
        "_base_stamp: self.base_stamp.take()",
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
                .expect("finish must release Busy")
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
    assert_eq!(probe.revoke_count(), 1);
    assert!(probe.revoke_saw_exact_stamp());
    assert!(probe.revoked_before_lease_drop());
}
