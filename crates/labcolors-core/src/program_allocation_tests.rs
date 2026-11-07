use crate::Srgb8;
use crate::program::wire::ProgramWireBuilderV1;
use crate::program_wire::{
    ProgramAttachmentUpdateErrorV1, ProgramObservationV1, ProgramPointSinkHostErrorV1,
    ProgramPointSinkHostV1, ProgramPointSinkIntentV1, ProgramPointSinkStampV1,
    ProgramRuntimeErrorV1, ProgramScenarioV1, ProgramSnapshotStateV1, compile_program_wire_v1,
};
use crate::test_support::{AllocatorEvents, measured_allocator_events};

fn wire() -> Vec<u8> {
    wire_with_outputs(1)
}

fn wire_with_outputs(count: u32) -> Vec<u8> {
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
        .exact_visible_unary(true, 10, 8, Srgb8::new([0x60; 3]));
    for slot in 17..17 + count {
        builder.output(slot, 4);
    }
    builder.finish().unwrap()
}

const RESULT_ALLOCATION: AllocatorEvents = AllocatorEvents {
    alloc: 0,
    realloc: 0,
    dealloc: 0,
};

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
            .unwrap()
            .instantiate(100)
            .unwrap();
        let mut ids: Vec<u32> = (0..count).rev().collect();
        let mut rgb = vec![0x80; count as usize * 3];
        let mut saved = Vec::with_capacity(8);
        // Все доступные слоты прогреваются отдельно от измеряемого вызова.
        for revision in 1..=4 {
            session
                .update_observed_view(
                    revision,
                    ProgramObservationV1::from_rgb8(&ids, &rgb, 1).unwrap(),
                )
                .unwrap();
        }
        for revision in [5, 5, 6, 7, 8, 9, 10, 11] {
            let (snapshot, events) = measured_allocator_events(|| {
                let input = ProgramObservationV1::from_rgb8(&ids, &rgb, 1).unwrap();
                session.update_observed_view(revision, input).unwrap()
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
            .unwrap()
            .attach(100, 17, 91, 9, 8, Host::default())
            .unwrap();
        let input = [ProgramScenarioV1::new(7, vec![Srgb8::new([0x80; 3])])];
        let ids = [7];
        let rgb = [0x80; 3];
        for revision in 1..=4 {
            attachment.update_observed(revision, &input).unwrap();
        }
        let mut previous = None;
        for revision in [5, 5, 6, 7] {
            let (snapshot, events) = measured_allocator_events(|| {
                if borrowed {
                    attachment.update_observed_view(
                        revision,
                        ProgramObservationV1::from_rgb8(&ids, &rgb, 1).unwrap(),
                    )
                } else {
                    attachment.update_observed(revision, &input)
                }
                .unwrap()
            });
            assert_eq!(snapshot.snapshot().state(), ProgramSnapshotStateV1::Ready);
            assert_eq!(
                snapshot.render().unwrap().terminal_composite(),
                Some(Srgb8::new([0x60; 3]))
            );
            assert_eq!(
                events, RESULT_ALLOCATION,
                "borrowed={borrowed}, revision={revision}"
            );
            if let Some(old) = previous.as_ref() {
                assert_eq!(old, snapshot.snapshot());
            }
            previous = Some(snapshot.snapshot().clone());
        }
    }
}

#[test]
fn borrowed_and_owned_inputs_share_admission_and_lifecycle() {
    let mut owned = compile_program_wire_v1(&wire())
        .unwrap()
        .instantiate(101)
        .unwrap();
    let mut borrowed = compile_program_wire_v1(&wire())
        .unwrap()
        .instantiate(101)
        .unwrap();
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
        let input = ProgramObservationV1::from_rgb8(ids, rgb, columns).unwrap();
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
            ProgramObservationV1::from_rgb8(&[3], &[0x80; 3], 1).unwrap()
        )
    );
}

#[test]
fn borrowed_resource_failure_does_not_publish_or_consume_the_revision() {
    let mut session = compile_program_wire_v1(&wire())
        .unwrap()
        .instantiate(100)
        .unwrap();
    let input = ProgramObservationV1::from_rgb8(&[7], &[0x80; 3], 1).unwrap();
    {
        let _failure = crate::program_session::fail_program_preflight_reservation_for_test(0);
        assert_eq!(
            session.update_observed_view(1, input),
            Err(ProgramRuntimeErrorV1::ResourceExhausted)
        );
    }
    assert_eq!(
        session.update_observed_view(1, input).unwrap().state(),
        ProgramSnapshotStateV1::Ready
    );
    let mut attachment = compile_program_wire_v1(&wire())
        .unwrap()
        .attach(100, 17, 91, 9, 8, Host::default())
        .unwrap();
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
            .unwrap()
            .render()
            .unwrap()
            .revision(),
        1
    );
}

#[test]
fn ready_failed_recovery_and_unknown_keep_the_warmed_allocation_budget() {
    let mut session = compile_program_wire_v1(&wire())
        .unwrap()
        .instantiate(100)
        .unwrap();
    let good = ProgramObservationV1::from_rgb8(&[7], &[0x80; 3], 1).unwrap();
    let bad = ProgramObservationV1::from_rgb8(&[7], &[0; 3], 1).unwrap();
    for revision in 1..=9 {
        session
            .update_observed_view(revision, if revision % 2 == 0 { bad } else { good })
            .unwrap();
    }
    for revision in 10..=20 {
        let (snapshot, events) = measured_allocator_events(|| {
            if revision % 3 == 0 {
                session.update_unknown(revision, 9)
            } else {
                session.update_observed_view(revision, if revision % 3 == 1 { good } else { bad })
            }
            .unwrap()
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
            .unwrap()
            .instantiate(100)
            .unwrap();
        let input = ProgramObservationV1::from_rgb8(&[7], &[0x80; 3], 1).unwrap();
        for revision in 1..=4 {
            session.update_observed_view(revision, input).unwrap();
        }
        let (snapshot, events) =
            measured_allocator_events(|| session.update_observed_view(5, input).unwrap());
        assert_eq!(
            events,
            AllocatorEvents {
                alloc: 1,
                realloc: 0,
                dealloc: 0
            }
        );
        assert_eq!(snapshot.outputs().len(), count as usize);
        let second = session.update_observed_view(6, input).unwrap();
        assert_eq!(snapshot, second);
        drop(session);
        for (index, output) in snapshot.outputs().iter().enumerate() {
            assert_eq!(output.slot(), 17 + index as u32);
            assert_eq!(output.source(), Srgb8::new([0x40; 3]));
        }
    }
}
