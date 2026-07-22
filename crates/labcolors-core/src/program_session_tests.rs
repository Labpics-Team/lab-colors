use crate::Srgb8;
use crate::appearance::{
    ColorInputId, OccurrenceId, OpacityInputId, PaintId, SurfaceId, SurfaceInputPortId,
};
use crate::program_session::{
    ColorInput, CompiledProgram, CompositionProfile, Occurrence, OpacityInput,
    PACKED_ENCODED_SURFACE_PRESENT_TAG_V1, PACKED_ENCODED_SURFACE_UNAVAILABLE_TAG_V1,
    PACKED_ENCODED_SURFACE_UPDATE_MAGIC_V1, PackedEncodedSurfaceUpdateErrorV1, Paint,
    PointRenderOwner, PointRenderSessionUpdateErrorV1, Program, ProgramCompileError, SessionState,
    SessionUpdateError, Surface, SurfaceSignal, SurfaceUpdate,
    canonical_occurrence_sequence_matches, canonical_surface_input_port_sequence_matches,
    check_render_node_count,
};

const COLOR: ColorInputId = ColorInputId::new(1);
const SURFACE_PORT: SurfaceInputPortId = SurfaceInputPortId::new(2);
const OPACITY: OpacityInputId = OpacityInputId::new(3);
const SOLID: PaintId = PaintId::new(10);
const TRANSLUCENT: PaintId = PaintId::new(11);
const BACKDROP: SurfaceId = SurfaceId::new(20);
const VISIBLE: SurfaceId = SurfaceId::new(21);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(30);

fn program(opacity: f64) -> Program {
    program_against(BACKDROP, opacity)
}

fn program_against(against: SurfaceId, opacity: f64) -> Program {
    Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        vec![SURFACE_PORT],
        vec![OpacityInput::new(OPACITY, opacity)],
        vec![
            Paint::Solid {
                id: SOLID,
                color: COLOR,
            },
            Paint::Opacity {
                id: TRANSLUCENT,
                source: SOLID,
                opacity: OPACITY,
            },
        ],
        vec![
            Surface::Input {
                id: BACKDROP,
                input: SURFACE_PORT,
            },
            Surface::FromOccurrence {
                id: VISIBLE,
                occurrence: OCCURRENCE,
            },
        ],
        vec![Occurrence::new(
            OCCURRENCE,
            TRANSLUCENT,
            against,
            CompositionProfile::EncodedSrgb8SourceOverV1,
        )],
    )
}

fn cyclic_program() -> Program {
    Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        vec![SURFACE_PORT],
        vec![OpacityInput::new(OPACITY, 0.5)],
        vec![
            Paint::Solid {
                id: SOLID,
                color: COLOR,
            },
            Paint::Opacity {
                id: TRANSLUCENT,
                source: SOLID,
                opacity: OPACITY,
            },
        ],
        vec![Surface::FromOccurrence {
            id: VISIBLE,
            occurrence: OCCURRENCE,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            TRANSLUCENT,
            VISIBLE,
            CompositionProfile::EncodedSrgb8SourceOverV1,
        )],
    )
}

fn compiled(opacity: f64) -> CompiledProgram {
    program(opacity).compile().unwrap()
}

fn point(revision: u64, rgb24: u32) -> [u32; 5] {
    [
        PACKED_ENCODED_SURFACE_UPDATE_MAGIC_V1,
        PACKED_ENCODED_SURFACE_PRESENT_TAG_V1,
        revision as u32,
        (revision >> 32) as u32,
        rgb24,
    ]
}

fn unavailable(revision: u64, reason: u32) -> [u32; 5] {
    [
        PACKED_ENCODED_SURFACE_UPDATE_MAGIC_V1,
        PACKED_ENCODED_SURFACE_UNAVAILABLE_TAG_V1,
        revision as u32,
        (revision >> 32) as u32,
        reason,
    ]
}

fn retained_signal_storage_pointers(state: &SessionState) -> (*const u32, *const u32) {
    let snapshot = match state {
        SessionState::Ready { current } => current,
        SessionState::Stale { previous, .. } => previous,
        SessionState::Waiting { .. } => {
            panic!("the allocation test requires a retained successful snapshot")
        }
    };
    (
        snapshot.input_surface_signals_rgb24().as_ptr(),
        snapshot.composited_occurrence_signals_rgb24().as_ptr(),
    )
}

#[test]
fn authored_and_runtime_values_preserve_exact_typed_bindings() {
    let color_value = Srgb8::new([0x12, 0x34, 0x56]);
    let color = ColorInput::new(COLOR, color_value);
    assert_eq!(color.id(), COLOR);
    assert_eq!(color.value(), color_value);

    let opacity = OpacityInput::new(OPACITY, 0.375);
    assert_eq!(opacity.id(), OPACITY);
    assert_eq!(opacity.value(), 0.375);

    let occurrence = Occurrence::new(
        OCCURRENCE,
        TRANSLUCENT,
        BACKDROP,
        CompositionProfile::EncodedSrgb8SourceOverV1,
    );
    assert_eq!(occurrence.id(), OCCURRENCE);
    assert_eq!(occurrence.subject(), TRANSLUCENT);
    assert_eq!(occurrence.against(), BACKDROP);
    assert_eq!(
        occurrence.composition(),
        CompositionProfile::EncodedSrgb8SourceOverV1
    );

    let surface_value = Srgb8::new([0xab, 0xcd, 0xef]);
    let signal = SurfaceSignal::new(SURFACE_PORT, surface_value);
    assert_eq!(signal.input(), SURFACE_PORT);
    assert_eq!(signal.value(), surface_value);
}

#[test]
fn empty_surface_schema_precedes_dangling_surface_input_port_analysis() {
    let declaration = Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        vec![],
        vec![OpacityInput::new(OPACITY, 0.5)],
        vec![Paint::Solid {
            id: SOLID,
            color: COLOR,
        }],
        vec![Surface::Input {
            id: BACKDROP,
            input: SURFACE_PORT,
        }],
        vec![Occurrence::new(
            OCCURRENCE,
            SOLID,
            BACKDROP,
            CompositionProfile::EncodedSrgb8SourceOverV1,
        )],
    );

    assert_eq!(
        declaration.compile().unwrap_err(),
        ProgramCompileError::EmptySurfaceSchema
    );
}

#[test]
fn empty_occurrence_set_precedes_dangling_occurrence_analysis() {
    let declaration = Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        vec![SURFACE_PORT],
        vec![],
        vec![Paint::Solid {
            id: SOLID,
            color: COLOR,
        }],
        vec![Surface::FromOccurrence {
            id: VISIBLE,
            occurrence: OCCURRENCE,
        }],
        vec![],
    );

    assert_eq!(
        declaration.compile().unwrap_err(),
        ProgramCompileError::EmptyOccurrenceSet
    );
}

#[test]
fn combined_render_cardinality_overflow_is_resource_exhaustion() {
    assert_eq!(check_render_node_count(usize::MAX - 1, 1), Ok(()));
    assert_eq!(
        check_render_node_count(usize::MAX, 1),
        Err(ProgramCompileError::ResourceExhausted)
    );
}

#[test]
fn canonical_sequence_firewall_rejects_reordering_relabeling_and_truncation() {
    let surface_input_ports = [SurfaceInputPortId::new(1), SurfaceInputPortId::new(2)];
    assert!(canonical_surface_input_port_sequence_matches(
        [SurfaceInputPortId::new(1), SurfaceInputPortId::new(2),],
        &surface_input_ports,
    ));
    assert!(!canonical_surface_input_port_sequence_matches(
        [SurfaceInputPortId::new(2), SurfaceInputPortId::new(1),],
        &surface_input_ports,
    ));
    assert!(!canonical_surface_input_port_sequence_matches(
        [SurfaceInputPortId::new(1)],
        &surface_input_ports,
    ));

    let occurrences = [OccurrenceId::new(10), OccurrenceId::new(20)];
    assert!(canonical_occurrence_sequence_matches(
        [OccurrenceId::new(10), OccurrenceId::new(20),],
        &occurrences,
    ));
    assert!(!canonical_occurrence_sequence_matches(
        [OccurrenceId::new(10), OccurrenceId::new(21),],
        &occurrences,
    ));
    assert!(!canonical_occurrence_sequence_matches(
        [OccurrenceId::new(10)],
        &occurrences,
    ));
}

#[test]
fn point_update_executes_the_compiled_graph_and_commits_compact_occurrences() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();

    let SessionState::Ready { current } = session.update_packed(&point(1, 0xff_ff_ff)).unwrap()
    else {
        panic!("present encoded Surface signals must produce Ready");
    };
    assert_eq!(current.revision(), 1);
    assert_eq!(current.input_surface_signals_rgb24(), &[0xff_ff_ff]);
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
}

#[test]
fn successful_replace_revokes_old_sessions_without_a_numeric_generation() {
    let mut owner = compiled(0.5).into_owner();
    let mut old = owner.attach().unwrap();

    owner.replace(compiled(0.25));
    assert_eq!(
        old.update_packed(&point(1, 0xff_ff_ff)),
        Err(PointRenderSessionUpdateErrorV1::ProgramExpired)
    );

    let mut current = owner.attach().unwrap();
    assert!(matches!(
        current.update_packed(&point(1, 0xff_ff_ff)).unwrap(),
        SessionState::Ready { .. }
    ));
}

#[test]
fn invalid_opacity_failed_replace_is_atomic_and_keeps_old_epoch_live() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();

    assert_eq!(
        program(1.25).compile().unwrap_err(),
        ProgramCompileError::OpacityOutOfDomain { input: OPACITY }
    );
    let SessionState::Ready { current } = session.update_packed(&point(1, 0xff_ff_ff)).unwrap()
    else {
        panic!("failed replacement must not revoke the old epoch");
    };
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
}

#[test]
fn dangling_and_cyclic_failed_compiles_do_not_revoke_the_current_epoch() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();

    let missing = SurfaceId::new(999);
    assert_eq!(
        program_against(missing, 0.5).compile().unwrap_err(),
        ProgramCompileError::MissingOccurrenceBackdrop {
            occurrence: OCCURRENCE,
            surface: missing,
        }
    );
    assert!(matches!(
        cyclic_program().compile(),
        Err(ProgramCompileError::RenderCycle { .. })
    ));

    let SessionState::Ready { current } = session.update_packed(&point(1, 0xff_ff_ff)).unwrap()
    else {
        panic!("compile failures must leave the old strong epoch untouched");
    };
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
}

#[test]
fn dispose_revokes_sessions_and_prevents_new_attachment() {
    let mut owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    owner.dispose();

    assert_eq!(
        session.update_packed(&unavailable(1, 7)),
        Err(PointRenderSessionUpdateErrorV1::ProgramExpired)
    );
    assert!(owner.attach().is_err());
}

#[test]
fn unavailable_after_ready_is_stale_and_retains_exactly_one_previous_snapshot() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    session.update_packed(&point(1, 0xff_ff_ff)).unwrap();

    let SessionState::Stale {
        previous,
        current_unavailable,
    } = session.update_packed(&unavailable(2, 91)).unwrap()
    else {
        panic!("unavailable Surface input after Ready must become Stale");
    };
    assert_eq!(previous.revision(), 1);
    assert_eq!(
        previous.composited_occurrence_signals_rgb24(),
        &[0x80_80_80]
    );
    assert_eq!(current_unavailable.revision(), 2);
    assert_eq!(current_unavailable.reason(), 91);

    let SessionState::Stale { previous, .. } = session.update_packed(&unavailable(3, 92)).unwrap()
    else {
        panic!("a later unavailable update must remain Stale");
    };
    assert_eq!(previous.revision(), 1);
}

#[test]
fn malformed_lower_and_conflicting_updates_are_atomic_and_do_not_evaluate() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    session.update_packed(&point(5, 0xff_ff_ff)).unwrap();
    crate::composition::reset_source_over_evaluation_count();

    let malformed = point(6, 0x01_ff_ff_ff);
    assert_eq!(
        session.update_packed(&malformed),
        Err(PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate(
            PackedEncodedSurfaceUpdateErrorV1::ReservedSignalByteNonZero {
                surface_index: 0,
                value: 0x01_ff_ff_ff,
            }
        ))
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    assert!(matches!(
        session.update_packed(&point(4, 0)),
        Err(PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate(
            PackedEncodedSurfaceUpdateErrorV1::RevisionOutOfOrder {
                current: 5,
                incoming: 4
            }
        ))
    ));
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    assert!(matches!(
        session.update_packed(&point(5, 0)),
        Err(PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate(
            PackedEncodedSurfaceUpdateErrorV1::RevisionConflict { revision: 5 }
        ))
    ));
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    let SessionState::Ready { current } = session.state() else {
        panic!("every rejected update must leave the committed state untouched");
    };
    assert_eq!(current.revision(), 5);
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
}

#[test]
fn exact_replay_is_idempotent_but_a_new_revision_evaluates_again() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    let payload = point(1, 0xff_ff_ff);
    crate::composition::reset_source_over_evaluation_count();

    session.update_packed(&payload).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    session.update_packed(&payload).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    session.update_packed(&point(2, 0xff_ff_ff)).unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 2);
}

#[test]
fn attached_session_reuses_buffers_for_every_update_state() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    let first = point(1, 0xff_ff_ff);
    let second = point(2, 0x20_40_60);
    let missing = unavailable(3, 91);
    let still_missing = unavailable(4, 92);
    let recovered = point(5, 0x20_40_60);

    let mut retained_storage = None;
    crate::composition::reset_source_over_evaluation_count();
    for (update, expected_evaluations) in [
        (first.as_slice(), 1),
        (first.as_slice(), 1),
        (second.as_slice(), 2),
        (missing.as_slice(), 2),
        (still_missing.as_slice(), 2),
        (recovered.as_slice(), 3),
    ] {
        let (result, allocations) =
            crate::test_support::measured_allocations(|| session.update_packed(update).map(|_| ()));
        assert!(result.is_ok());
        assert_eq!(
            allocations, 0,
            "attach must preallocate every fixed-cardinality Session buffer"
        );
        let current_storage = retained_signal_storage_pointers(session.state());
        if let Some(initial_storage) = retained_storage {
            assert_eq!(current_storage, initial_storage);
        } else {
            retained_storage = Some(current_storage);
        }
        assert_eq!(
            crate::composition::source_over_evaluation_count(),
            expected_evaluations
        );
    }

    let SessionState::Ready { current } = session.state() else {
        panic!("a successful observation after Stale must recover Ready");
    };
    assert_eq!(current.revision(), 5);
    assert_eq!(current.input_surface_signals_rgb24(), &[0x20_40_60]);
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x10_20_30]);
}

#[test]
fn rejected_update_preserves_cold_buffers_for_allocation_free_retry() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    let malformed = point(1, 0x01_ff_ff_ff);
    let valid = point(1, 0xff_ff_ff);
    crate::composition::reset_source_over_evaluation_count();

    let (rejected, rejected_allocations) =
        crate::test_support::measured_allocations(|| session.update_packed(&malformed).map(|_| ()));
    assert_eq!(
        rejected,
        Err(PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate(
            PackedEncodedSurfaceUpdateErrorV1::ReservedSignalByteNonZero {
                surface_index: 0,
                value: 0x01_ff_ff_ff,
            }
        ))
    );
    assert_eq!(rejected_allocations, 0);
    assert!(matches!(
        session.state(),
        SessionState::Waiting {
            current_unavailable: None
        }
    ));
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    let (accepted, accepted_allocations) =
        crate::test_support::measured_allocations(|| session.update_packed(&valid).map(|_| ()));
    assert!(accepted.is_ok());
    assert_eq!(accepted_allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
}

#[test]
fn waiting_unknown_chain_preserves_preallocated_buffers() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    let first_missing = unavailable(1, 91);
    let later_missing = unavailable(2, 92);
    let ready = point(3, 0xff_ff_ff);
    crate::composition::reset_source_over_evaluation_count();

    for (update, expected_evaluations) in [
        (first_missing.as_slice(), 0),
        (first_missing.as_slice(), 0),
        (later_missing.as_slice(), 0),
        (ready.as_slice(), 1),
    ] {
        let (result, allocations) =
            crate::test_support::measured_allocations(|| session.update_packed(update).map(|_| ()));
        assert!(result.is_ok());
        assert_eq!(allocations, 0);
        assert_eq!(
            crate::composition::source_over_evaluation_count(),
            expected_evaluations
        );
    }

    let SessionState::Ready { current } = session.state() else {
        panic!("the first admitted point after Waiting must commit Ready");
    };
    assert_eq!(current.revision(), 3);
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
}

#[test]
fn public_program_compile_owner_session_path_emits_typed_occurrence_values() {
    let compiled = compiled(0.5);
    assert_eq!(compiled.surface_input_ports(), &[SURFACE_PORT]);
    assert_eq!(compiled.occurrences(), &[OCCURRENCE]);

    let owner = PointRenderOwner::new(compiled);
    assert_eq!(owner.surface_input_ports(), Some(&[SURFACE_PORT][..]));
    assert_eq!(owner.occurrences(), Some(&[OCCURRENCE][..]));
    let mut session = owner.attach().unwrap();
    let surfaces = [SurfaceSignal::new(SURFACE_PORT, Srgb8::new([0xff; 3]))];
    let SessionState::Ready { current } = session
        .update(SurfaceUpdate::Present {
            revision: 11,
            surfaces: &surfaces,
        })
        .unwrap()
    else {
        panic!("typed present update must produce Ready");
    };

    assert_eq!(current.revision(), 11);
    assert_eq!(current.surfaces().collect::<Vec<_>>(), surfaces.to_vec());
    assert_eq!(
        current
            .occurrences()
            .map(|signal| (signal.occurrence(), signal.value()))
            .collect::<Vec<_>>(),
        vec![(OCCURRENCE, Srgb8::new([0x80; 3]))]
    );
    assert_eq!(current.occurrence(OCCURRENCE), Some(Srgb8::new([0x80; 3])));
    assert_eq!(current.occurrence(OccurrenceId::new(999)), None);
}

#[test]
fn typed_update_schema_rejection_is_atomic_and_allocation_free() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    let wrong = [SurfaceSignal::new(
        SurfaceInputPortId::new(999),
        Srgb8::new([0xff; 3]),
    )];
    crate::composition::reset_source_over_evaluation_count();

    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update(SurfaceUpdate::Present {
                revision: 1,
                surfaces: &wrong,
            })
            .map(|_| ())
    });
    assert_eq!(
        result,
        Err(SessionUpdateError::SurfaceInputPortMismatch {
            index: 0,
            expected: SURFACE_PORT,
            actual: SurfaceInputPortId::new(999),
        })
    );
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert!(matches!(
        session.state(),
        SessionState::Waiting {
            current_unavailable: None
        }
    ));
}

#[test]
fn typed_update_reuses_attach_storage_for_ready_stale_and_recovery() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    let white = [SurfaceSignal::new(SURFACE_PORT, Srgb8::new([0xff; 3]))];
    let black = [SurfaceSignal::new(SURFACE_PORT, Srgb8::new([0; 3]))];

    for update in [
        SurfaceUpdate::Present {
            revision: 1,
            surfaces: &white,
        },
        SurfaceUpdate::Unavailable {
            revision: 2,
            reason: 7,
        },
        SurfaceUpdate::Present {
            revision: 3,
            surfaces: &black,
        },
    ] {
        let (result, allocations) =
            crate::test_support::measured_allocations(|| session.update(update).map(|_| ()));
        assert!(result.is_ok());
        assert_eq!(allocations, 0);
    }
}

#[test]
fn dropping_the_only_owner_physically_expires_attached_sessions() {
    let mut session = {
        let owner = compiled(0.5).into_owner();
        owner.attach().unwrap()
    };
    let surfaces = [SurfaceSignal::new(SURFACE_PORT, Srgb8::new([0; 3]))];
    assert_eq!(
        session.update(SurfaceUpdate::Present {
            revision: 1,
            surfaces: &surfaces,
        }),
        Err(SessionUpdateError::ProgramExpired)
    );
}

#[test]
fn generic_program_module_has_no_recipe_or_ui_compatibility_surface() {
    let source = include_str!("program_session.rs");
    for forbidden in [
        "ThemeConfig",
        "RoleRecipe",
        "NamedRoleTable",
        "PairFill",
        "PairLabel",
        "AlphaAnalog",
        "resolve_named_set",
        "resolveTheme",
        "themeHandle",
    ] {
        assert!(
            !source.contains(forbidden),
            "generic Program module must not contain `{forbidden}`"
        );
    }
    for required in [
        "pub struct Program",
        "pub struct CompiledProgram",
        "pub struct PointRenderOwner",
        "pub struct Session",
        "pub fn compile(self)",
        "pub fn update(",
    ] {
        assert!(
            source.contains(required),
            "generic Program module must retain `{required}`"
        );
    }
}
