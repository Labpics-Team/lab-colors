use crate::Srgb8;
use crate::program::wire::ProgramWireBuilderV1;
use crate::program_wire::{
    ProgramAssessmentVerdictV1, ProgramAttachmentUpdateErrorV1, ProgramConstraintAssessmentV1,
    ProgramObservationV1, ProgramPointSinkHostErrorV1, ProgramPointSinkHostV1,
    ProgramPointSinkIntentV1, ProgramPointSinkStampV1, ProgramRuntimeErrorV1, ProgramScenarioV1,
    ProgramSnapshotStateV1, ProgramSnapshotV1, compile_program_wire_v1,
};
use crate::test_support::{AllocatorEvents, measured_allocator_events};

fn wire() -> Vec<u8> {
    wire_with_outputs(1)
}

fn wire_with_outputs(count: u32) -> Vec<u8> {
    wire_with_constraint(count, true)
}

fn wire_with_constraint(count: u32, hard: bool) -> Vec<u8> {
    let mut builder = ProgramWireBuilderV1::new();
    builder
        .source(1, Srgb8::new([0x40; 3]))
        .fixed_target(2, 1)
        .surface_input_port(6)
        .opacity_input(5, 0.5)
        .solid_paint(3, 2)
        .opacity_paint(4, 3, 5)
        .input_surface(7, 6)
        .source_over_occurrence(8, 4, 7, 64.0, 0.2, 2)
        .presentation_root(9, 8)
        .presentation_target(9, 8)
        .exact_visible_unary(hard, 10, 8, Srgb8::new([0x60; 3]));
    for slot in 17..17 + count {
        builder.output(slot, 4);
    }
    builder
        .finish()
        .expect("the fixture must encode a valid Program")
}

const RESULT_ALLOCATION: AllocatorEvents = AllocatorEvents {
    alloc: 0,
    realloc: 0,
    dealloc: 0,
};

fn assert_same_result_body(previous: &ProgramSnapshotV1, current: &ProgramSnapshotV1) {
    assert_eq!(previous.snapshot_version(), current.snapshot_version());
    assert_eq!(previous.state(), current.state());
    assert_eq!(previous.outputs(), current.outputs());
    let before = previous
        .assessment_report()
        .expect("Ready has its owned report");
    let after = current
        .assessment_report()
        .expect("Ready has its owned report");
    assert_eq!(before.content_identity(), after.content_identity());
    assert_eq!(before.stream_id(), after.stream_id());
    assert_eq!(before.scenario_provenance(), after.scenario_provenance());
    assert_eq!(before.cells(), after.cells());
    // Этот публично объявленный Program имеет ровно root 9: проверяем каждую
    // provenance, а не один удобный сценарий или только выбранный Paint.
    for scenario in before.scenario_provenance() {
        assert_eq!(
            previous.presentation_composite_rgb(9, scenario.scenario_id()),
            current.presentation_composite_rgb(9, scenario.scenario_id()),
        );
    }
    assert_eq!(before.revision(), previous.revision());
    assert_eq!(after.revision(), current.revision());
    if previous.revision() == current.revision() {
        assert_eq!(previous, current);
    } else {
        assert_ne!(previous, current);
    }
}

#[derive(Default)]
struct Host {
    stamp: Option<ProgramPointSinkStampV1>,
}
impl ProgramPointSinkHostV1 for Host {
    fn try_install(
        &mut self,
        intent: ProgramPointSinkIntentV1,
    ) -> Result<(), ProgramPointSinkHostErrorV1> {
        if self
            .stamp
            .is_some_and(|stamp| stamp != intent.expected_stamp())
        {
            return Err(ProgramPointSinkHostErrorV1::Rejected);
        }
        self.stamp = Some(intent.desired_stamp());
        Ok(())
    }
}

#[test]
fn observation_shape_is_checked_without_allocating_or_reading_missing_values() {
    let ids = [9, 7, 5, 3];
    let bytes = [0x80; 48];
    for rows in 0..=4 {
        for columns in 0..=4 {
            for length in 0..=48 {
                let (result, events) = measured_allocator_events(|| {
                    ProgramObservationV1::from_rgb8(&ids[..rows], &bytes[..length], columns)
                });
                assert_eq!(result.is_ok(), length == rows * columns * 3);
                assert_eq!(events, AllocatorEvents::default());
            }
        }
    }
    for columns in [usize::MAX, usize::MAX / 3 + 1, usize::MAX / 4] {
        let (result, events) =
            measured_allocator_events(|| ProgramObservationV1::from_rgb8(&ids, &[], columns));
        assert!(result.is_err());
        assert_eq!(events, AllocatorEvents::default());
    }
}

#[test]
fn warmed_single_output_update_needs_no_heap() {
    for count in [1, 8, 32] {
        let mut session = compile_program_wire_v1(&wire())
            .expect("the fixture Program must compile")
            .instantiate(100)
            .expect("the fixture Session must instantiate");
        let mut ids: Vec<u32> = (0..count).rev().collect();
        let mut rgb = vec![0x80; count as usize * 3];
        let mut saved = Vec::with_capacity(8);
        // Все доступные слоты прогреваются отдельно от измеряемого вызова.
        for revision in 1..=4 {
            session
                .update_observed_view(
                    revision,
                    ProgramObservationV1::from_rgb8(&ids, &rgb, 1)
                        .expect("the RGB8 fixture must have its declared rectangular shape"),
                )
                .expect("the valid update must return an owned snapshot");
        }
        for revision in [5, 5, 6, 7, 8, 9, 10, 11] {
            let (snapshot, events) = measured_allocator_events(|| {
                let input = ProgramObservationV1::from_rgb8(&ids, &rgb, 1)
                    .expect("the RGB8 fixture must have its declared rectangular shape");
                session
                    .update_observed_view(revision, input)
                    .expect("the valid update must return an owned snapshot")
            });
            assert_eq!(
                events, RESULT_ALLOCATION,
                "rows={count}, revision={revision}"
            );
            assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
            assert_eq!(snapshot.outputs().len(), 1);
            saved.push(snapshot);
        }
        ids.fill(999);
        rgb.fill(0);
        drop(session);
        for snapshot in saved {
            assert_eq!(snapshot.state(), ProgramSnapshotStateV1::Ready);
            assert_eq!(snapshot.outputs()[0].slot(), 17);
            assert_eq!(snapshot.outputs()[0].source(), Srgb8::new([0x40; 3]));
            assert_eq!(snapshot.outputs()[0].opacity(), 0.5);
        }
    }
}

#[test]
fn attachment_snapshot_is_inline_and_independent() {
    for borrowed in [false, true] {
        let mut attachment = compile_program_wire_v1(&wire())
            .expect("the fixture Program must compile")
            .attach(100, 17, 91, 9, 8, Host::default())
            .expect("the fixture sink binding must attach");
        let input = [ProgramScenarioV1::new(7, vec![Srgb8::new([0x80; 3])])];
        let ids = [7];
        let rgb = [0x80; 3];
        for revision in 1..=4 {
            attachment
                .update_observed(revision, &input)
                .expect("the valid update must return an owned snapshot");
        }
        let mut previous = None;
        for revision in [5, 5, 6, 7] {
            let (snapshot, events) = measured_allocator_events(|| {
                if borrowed {
                    attachment.update_observed_view(
                        revision,
                        ProgramObservationV1::from_rgb8(&ids, &rgb, 1)
                            .expect("the RGB8 fixture must have its declared rectangular shape"),
                    )
                } else {
                    attachment.update_observed(revision, &input)
                }
                .expect("the valid update must return an owned snapshot")
            });
            assert_eq!(snapshot.snapshot().state(), ProgramSnapshotStateV1::Ready);
            assert_eq!(
                snapshot
                    .render()
                    .expect("a ready attachment must publish its render")
                    .terminal_composite(),
                Some(Srgb8::new([0x60; 3]))
            );
            assert_eq!(
                events, RESULT_ALLOCATION,
                "borrowed={borrowed}, revision={revision}"
            );
            assert_eq!(snapshot.snapshot().revision(), revision);
            assert_eq!(
                snapshot
                    .snapshot()
                    .assessment_report()
                    .expect("Ready has its exact revision report")
                    .revision(),
                revision,
            );
            if let Some(old) = previous.as_ref() {
                assert_same_result_body(old, snapshot.snapshot());
            }
            previous = Some(snapshot.snapshot().clone());
        }
    }
}

#[test]
fn borrowed_and_owned_inputs_share_admission_and_lifecycle() {
    let mut owned = compile_program_wire_v1(&wire())
        .expect("the fixture Program must compile")
        .instantiate(101)
        .expect("the fixture Session must instantiate");
    let mut borrowed = compile_program_wire_v1(&wire())
        .expect("the fixture Program must compile")
        .instantiate(101)
        .expect("the fixture Session must instantiate");
    let cases: &[(u64, &[u32], &[u8], usize)] = &[
        (1, &[9, 3], &[0x80; 6], 1),
        (1, &[3, 9], &[0x80; 6], 1),
        (1, &[9, 3], &[0x00; 6], 1), // conflicting same revision
        (2, &[3, 3], &[0x80; 6], 1), // duplicate ID
        (2, &[], &[], 1),
        (2, &[3], &[], 0),           // wrong schema
        (2, &[3], &[0x00; 3], 1),    // failed with previous evidence
        (3, &[3, 9], &[0x80; 6], 1), // recovery may use the third slot
        (4, &[5, 7], &[0x80, 0x80, 0x80, 0, 0, 0], 1),
        (5, &[7, 5], &[0x80; 6], 1),
    ];
    for &(revision, ids, rgb, columns) in cases {
        let rows: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(row, id)| {
                let surfaces = rgb[row * columns * 3..(row + 1) * columns * 3]
                    .chunks_exact(3)
                    .map(|bytes| Srgb8::new([bytes[0], bytes[1], bytes[2]]))
                    .collect();
                ProgramScenarioV1::new(*id, surfaces)
            })
            .collect();
        let input = ProgramObservationV1::from_rgb8(ids, rgb, columns)
            .expect("the RGB8 fixture must have its declared rectangular shape");
        assert_eq!(
            owned.update_observed(revision, &rows),
            borrowed.update_observed_view(revision, input)
        );
    }
    assert_eq!(owned.update_unknown(6, 99), borrowed.update_unknown(6, 99));
    assert_eq!(
        owned.update_observed(7, &[ProgramScenarioV1::new(3, vec![Srgb8::new([0x80; 3])])]),
        borrowed.update_observed_view(
            7,
            ProgramObservationV1::from_rgb8(&[3], &[0x80; 3], 1)
                .expect("the RGB8 fixture must have its declared rectangular shape")
        )
    );
}

#[test]
fn borrowed_resource_failure_does_not_publish_or_consume_the_revision() {
    let mut session = compile_program_wire_v1(&wire())
        .expect("the fixture Program must compile")
        .instantiate(100)
        .expect("the fixture Session must instantiate");
    let input = ProgramObservationV1::from_rgb8(&[7], &[0x80; 3], 1)
        .expect("the RGB8 fixture must have its declared rectangular shape");
    {
        let _failure = crate::program_session::fail_program_preflight_reservation_for_test(0);
        assert_eq!(
            session.update_observed_view(1, input),
            Err(ProgramRuntimeErrorV1::ResourceExhausted)
        );
    }
    assert_eq!(
        session
            .update_observed_view(1, input)
            .expect("the valid update must return an owned snapshot")
            .state(),
        ProgramSnapshotStateV1::Ready
    );
    let mut attachment = compile_program_wire_v1(&wire())
        .expect("the fixture Program must compile")
        .attach(100, 17, 91, 9, 8, Host::default())
        .expect("the fixture sink binding must attach");
    {
        let _failure = crate::program_session::fail_program_preflight_reservation_for_test(0);
        assert_eq!(
            attachment.update_observed_view(1, input),
            Err(ProgramAttachmentUpdateErrorV1::ResourceExhausted)
        );
    }
    assert!(attachment.current_render().is_none());
    assert_eq!(
        attachment
            .update_observed_view(1, input)
            .expect("the valid update must return an owned snapshot")
            .render()
            .expect("a ready attachment must publish its render")
            .revision(),
        1
    );
}

#[test]
fn ready_failed_recovery_and_unknown_keep_the_warmed_allocation_budget() {
    let mut session = compile_program_wire_v1(&wire())
        .expect("the fixture Program must compile")
        .instantiate(100)
        .expect("the fixture Session must instantiate");
    let good = ProgramObservationV1::from_rgb8(&[7], &[0x80; 3], 1)
        .expect("the RGB8 fixture must have its declared rectangular shape");
    let bad = ProgramObservationV1::from_rgb8(&[7], &[0; 3], 1)
        .expect("the RGB8 fixture must have its declared rectangular shape");
    for revision in 1..=9 {
        session
            .update_observed_view(revision, if revision % 2 == 0 { bad } else { good })
            .expect("the valid update must return an owned snapshot");
    }
    for revision in 10..=20 {
        let (snapshot, events) = measured_allocator_events(|| {
            if revision % 3 == 0 {
                session.update_unknown(revision, 9)
            } else {
                session.update_observed_view(revision, if revision % 3 == 1 { good } else { bad })
            }
            .expect("the valid update must return an owned snapshot")
        });
        assert_eq!(events.alloc, 0, "revision={revision}");
        assert_eq!(events.realloc, 0, "revision={revision}");
        assert_eq!(
            snapshot.state(),
            match revision % 3 {
                0 => ProgramSnapshotStateV1::Stale,
                1 => ProgramSnapshotStateV1::Ready,
                _ => ProgramSnapshotStateV1::Failed,
            }
        );
    }
}

#[test]
fn multiple_outputs_use_one_exact_owned_buffer_without_reallocation() {
    for count in [2, 8, 64] {
        let mut session = compile_program_wire_v1(&wire_with_outputs(count))
            .expect("the fixture Program must compile")
            .instantiate(100)
            .expect("the fixture Session must instantiate");
        let input = ProgramObservationV1::from_rgb8(&[7], &[0x80; 3], 1)
            .expect("the RGB8 fixture must have its declared rectangular shape");
        for revision in 1..=4 {
            session
                .update_observed_view(revision, input)
                .expect("the valid update must return an owned snapshot");
        }
        let (snapshot, events) = measured_allocator_events(|| {
            session
                .update_observed_view(5, input)
                .expect("the valid update must return an owned snapshot")
        });
        assert_eq!(
            events,
            AllocatorEvents {
                alloc: 1,
                realloc: 0,
                dealloc: 0
            }
        );
        assert_eq!(snapshot.outputs().len(), count as usize);
        let second = session
            .update_observed_view(6, input)
            .expect("the valid update must return an owned snapshot");
        assert_eq!(snapshot.revision(), 5);
        assert_eq!(second.revision(), 6);
        assert_same_result_body(&snapshot, &second);
        drop(session);
        assert_eq!(snapshot.revision(), 5);
        assert_eq!(
            snapshot
                .assessment_report()
                .expect("old Ready report")
                .revision(),
            5
        );
        for (index, output) in snapshot.outputs().iter().enumerate() {
            assert_eq!(output.slot(), 17 + index as u32);
            assert_eq!(output.source(), Srgb8::new([0x40; 3]));
        }
    }
}

#[test]
fn identical_ready_projection_reuses_owned_body_beyond_small_scenario_counts() {
    for rows in [33_u32, 129] {
        let compiled = compile_program_wire_v1(&wire()).expect("the declared Program must compile");
        let content_identity = compiled.content_identity();
        let mut session = compiled
            .instantiate(100)
            .expect("the Session must instantiate");
        let mut ids: Vec<_> = (0..rows).rev().collect();
        let rgb = vec![0x80; rows as usize * 3];
        for revision in 1..=4 {
            session
                .update_observed_view(
                    revision,
                    ProgramObservationV1::from_rgb8(&ids, &rgb, 1)
                        .expect("rectangular observations"),
                )
                .expect("the update must certify the declared output");
        }
        let mut retained = Vec::with_capacity(36);
        for revision in 5..=40 {
            let (snapshot, events) = measured_allocator_events(|| {
                session
                    .update_observed_view(
                        revision,
                        ProgramObservationV1::from_rgb8(&ids, &rgb, 1)
                            .expect("rectangular observations"),
                    )
                    .expect("the update must return an independent snapshot")
            });
            assert_eq!(
                events, RESULT_ALLOCATION,
                "rows={rows}, revision={revision}"
            );
            assert_eq!(snapshot.revision(), revision);
            let report = snapshot
                .assessment_report()
                .expect("Ready has its complete report");
            assert_eq!(report.content_identity(), content_identity);
            assert_eq!(report.stream_id(), 100);
            assert_eq!(report.revision(), revision);
            assert_eq!(report.scenario_provenance().len(), rows as usize);
            assert_eq!(report.cells().len(), 1);
            assert_eq!(report.cells()[0].constraint_id(), 10);
            assert_eq!(report.cells()[0].case_index(), 0);
            assert!(report.cells()[0].hard());
            assert_eq!(
                report.cells()[0].assessment(),
                ProgramConstraintAssessmentV1::ExactSrgb8 {
                    verdict: ProgramAssessmentVerdictV1::Pass,
                }
            );
            for (id, provenance) in report.scenario_provenance().iter().enumerate() {
                assert_eq!(provenance.scenario_id(), id as u32);
                assert_eq!(provenance.case_index(), 0);
                assert_eq!(
                    snapshot.presentation_composite_rgb(9, id as u32),
                    Some(Srgb8::new([0x60; 3]))
                );
            }
            if let Some(previous) = retained.last() {
                assert_same_result_body(previous, &snapshot);
            }
            retained.push(snapshot);
        }
        ids.iter_mut().for_each(|id| *id += 1000);
        let changed = session
            .update_observed_view(
                41,
                ProgramObservationV1::from_rgb8(&ids, &rgb, 1)
                    .expect("rectangular changed provenance"),
            )
            .expect("different provenance owns its distinct data");
        assert_eq!(
            changed
                .assessment_report()
                .expect("changed Ready report")
                .scenario_provenance()[0]
                .scenario_id(),
            1000
        );
        assert_eq!(changed.presentation_composite_rgb(9, 0), None);
        assert_eq!(
            changed.presentation_composite_rgb(9, 1000),
            Some(Srgb8::new([0x60; 3]))
        );
        drop(session);
        for (index, snapshot) in retained.iter().enumerate() {
            assert_eq!(snapshot.revision(), index as u64 + 5);
            let report = snapshot
                .assessment_report()
                .expect("historical owned report survives Session");
            assert_eq!(report.revision(), snapshot.revision());
            assert_eq!(report.scenario_provenance().len(), rows as usize);
            assert_eq!(report.scenario_provenance()[0].scenario_id(), 0);
            assert_eq!(
                snapshot.presentation_composite_rgb(9, 0),
                Some(Srgb8::new([0x60; 3]))
            );
            assert_eq!(snapshot.presentation_composite_rgb(9, 1000), None);
        }
    }
}

#[test]
fn changed_ready_projection_owns_all_cells_composites_and_provenance() {
    let mut session = compile_program_wire_v1(&wire_with_constraint(1, false))
        .expect("report-only Program must compile")
        .instantiate(100)
        .expect("Session must instantiate");
    let mut ids: Vec<_> = (0..33).collect();
    let mut rgb = vec![0x80; ids.len() * 3];
    for revision in 1..=4 {
        session
            .update_observed_view(
                revision,
                ProgramObservationV1::from_rgb8(&ids, &rgb, 1).expect("rectangular observations"),
            )
            .expect("report-only constraint permits the declared Paint");
    }
    let before = session
        .update_observed_view(
            5,
            ProgramObservationV1::from_rgb8(&ids, &rgb, 1).expect("rectangular observations"),
        )
        .expect("initial Ready projection");
    rgb.fill(255);
    let (changed, events) = measured_allocator_events(|| {
        session
            .update_observed_view(
                6,
                ProgramObservationV1::from_rgb8(&ids, &rgb, 1)
                    .expect("rectangular changed observations"),
            )
            .expect("changed report-only projection remains Ready")
    });
    assert!(
        events.alloc > 0,
        "new multi-record data require genuine owned storage"
    );
    assert_eq!(events.realloc, 0);
    assert_eq!(before.outputs(), changed.outputs());
    assert_eq!(
        before.presentation_composite_rgb(9, 0),
        Some(Srgb8::new([0x60; 3]))
    );
    assert_eq!(
        changed.presentation_composite_rgb(9, 0),
        Some(Srgb8::new([0xA0; 3]))
    );
    assert_eq!(
        before.assessment_report().expect("initial report").cells()[0].assessment(),
        ProgramConstraintAssessmentV1::ExactSrgb8 {
            verdict: ProgramAssessmentVerdictV1::Pass
        }
    );
    assert_eq!(
        changed.assessment_report().expect("changed report").cells()[0].assessment(),
        ProgramConstraintAssessmentV1::ExactSrgb8 {
            verdict: ProgramAssessmentVerdictV1::Violation
        }
    );
    ids.iter_mut().for_each(|id| *id += 1000);
    let provenance = session
        .update_observed_view(
            7,
            ProgramObservationV1::from_rgb8(&ids, &rgb, 1)
                .expect("changed provenance remains rectangular"),
        )
        .expect("changed provenance has its own Ready body");
    assert_eq!(
        provenance
            .assessment_report()
            .expect("new provenance report")
            .scenario_provenance()[0]
            .scenario_id(),
        1000
    );
    assert_eq!(
        changed
            .assessment_report()
            .expect("previous provenance report")
            .scenario_provenance()[0]
            .scenario_id(),
        0
    );
    drop(session);
    assert_eq!(before.revision(), 5);
    assert_eq!(changed.revision(), 6);
    assert_eq!(provenance.revision(), 7);
    assert_eq!(
        before.presentation_composite_rgb(9, 0),
        Some(Srgb8::new([0x60; 3]))
    );
    assert_eq!(
        changed.presentation_composite_rgb(9, 0),
        Some(Srgb8::new([0xA0; 3]))
    );
    assert_eq!(changed.presentation_composite_rgb(9, 1000), None);
    assert_eq!(provenance.presentation_composite_rgb(9, 0), None);
    assert_eq!(
        provenance.presentation_composite_rgb(9, 1000),
        Some(Srgb8::new([0xA0; 3]))
    );
}
