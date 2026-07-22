use std::cell::Cell;

use crate::Srgb8;
use crate::appearance::{
    ColorInputId, OccurrenceId, OpacityInputId, PaintId, SurfaceId, SurfaceInputPortId,
};
use crate::program_session::{
    ColorInput, CompiledProgram, CompositionProfile, Occurrence, OpacityInput, Paint,
    PointRenderOwner, Program, ProgramCompileError, SessionState, SessionUpdateError, Surface,
    SurfaceSignal, SurfaceUpdate, canonical_occurrence_sequence_matches,
    canonical_surface_input_port_sequence_matches, check_render_node_count,
};

const COLOR: ColorInputId = ColorInputId::new(1);
const SURFACE_PORT: SurfaceInputPortId = SurfaceInputPortId::new(2);
const OPACITY: OpacityInputId = OpacityInputId::new(3);
const SOLID: PaintId = PaintId::new(10);
const TRANSLUCENT: PaintId = PaintId::new(11);
const BACKDROP: SurfaceId = SurfaceId::new(20);
const VISIBLE: SurfaceId = SurfaceId::new(21);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(30);

const MULTI_PORT_A: SurfaceInputPortId = SurfaceInputPortId::new(10);
const MULTI_PORT_B: SurfaceInputPortId = SurfaceInputPortId::new(20);
const MULTI_PORT_C: SurfaceInputPortId = SurfaceInputPortId::new(30);
const MULTI_PORTS: [SurfaceInputPortId; 3] = [MULTI_PORT_A, MULTI_PORT_B, MULTI_PORT_C];
const MULTI_SURFACE_A: SurfaceId = SurfaceId::new(100);
const MULTI_SURFACE_B: SurfaceId = SurfaceId::new(101);
const MULTI_SURFACE_C: SurfaceId = SurfaceId::new(102);

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

fn multi_surface_program() -> Program {
    Program::new(
        vec![ColorInput::new(COLOR, Srgb8::new([0; 3]))],
        vec![MULTI_PORT_C, MULTI_PORT_A, MULTI_PORT_B],
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
        vec![
            Surface::Input {
                id: MULTI_SURFACE_C,
                input: MULTI_PORT_C,
            },
            Surface::Input {
                id: MULTI_SURFACE_A,
                input: MULTI_PORT_A,
            },
            Surface::Input {
                id: MULTI_SURFACE_B,
                input: MULTI_PORT_B,
            },
            Surface::FromOccurrence {
                id: VISIBLE,
                occurrence: OCCURRENCE,
            },
        ],
        vec![Occurrence::new(
            OCCURRENCE,
            TRANSLUCENT,
            MULTI_SURFACE_B,
            CompositionProfile::EncodedSrgb8SourceOverV1,
        )],
    )
}

fn multi_compiled() -> CompiledProgram {
    multi_surface_program().compile().unwrap()
}

struct ReadProbe<const N: usize> {
    values: [Srgb8; N],
    reads: Cell<[usize; N]>,
}

impl<const N: usize> ReadProbe<N> {
    fn new(values: [Srgb8; N]) -> Self {
        Self {
            values,
            reads: Cell::new([0; N]),
        }
    }

    fn read(&self, index: usize) -> Srgb8 {
        let mut reads = self.reads.get();
        reads[index] += 1;
        self.reads.set(reads);
        self.values[index]
    }

    fn reads(&self) -> [usize; N] {
        self.reads.get()
    }
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
fn canonical_present_executes_the_compiled_graph_and_commits_compact_occurrences() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();

    let SessionState::Ready { current } = session
        .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap()
    else {
        panic!("a complete canonical Surface set must produce Ready");
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
        old.update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3])),
        Err(SessionUpdateError::ProgramExpired)
    );

    let mut current = owner.attach().unwrap();
    assert!(matches!(
        current
            .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
            .unwrap(),
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
    let SessionState::Ready { current } = session
        .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap()
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

    let SessionState::Ready { current } = session
        .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap()
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
        session.update(SurfaceUpdate::Unavailable {
            revision: 1,
            reason: 7,
        }),
        Err(SessionUpdateError::ProgramExpired)
    );
    assert!(owner.attach().is_err());
}

#[test]
fn unavailable_after_ready_is_stale_and_retains_exactly_one_previous_snapshot() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    session
        .update_canonical_present(1, 1, |_| Srgb8::new([0xff; 3]))
        .unwrap();

    let SessionState::Stale {
        previous,
        current_unavailable,
    } = session
        .update(SurfaceUpdate::Unavailable {
            revision: 2,
            reason: 91,
        })
        .unwrap()
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

    let SessionState::Stale { previous, .. } = session
        .update(SurfaceUpdate::Unavailable {
            revision: 3,
            reason: 92,
        })
        .unwrap()
    else {
        panic!("a later unavailable update must remain Stale");
    };
    assert_eq!(previous.revision(), 1);
}

#[test]
fn borrowed_present_expired_epoch_reads_zero_and_allocates_zero() {
    let mut session = {
        let owner = compiled(0.5).into_owner();
        owner.attach().unwrap()
    };
    let reads = Cell::new(0);

    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update_canonical_present(1, 1, |_| {
                reads.set(reads.get() + 1);
                Srgb8::new([0xff; 3])
            })
            .map(|_| ())
    });

    assert_eq!(result, Err(SessionUpdateError::ProgramExpired));
    assert_eq!(reads.get(), 0);
    assert_eq!(allocations, 0);
    assert!(matches!(
        session.state(),
        SessionState::Waiting {
            current_unavailable: None
        }
    ));
}

#[test]
fn borrowed_present_schema_mismatch_reads_zero_and_allocates_zero() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    crate::composition::reset_source_over_evaluation_count();

    for actual in [0, 2] {
        let reads = Cell::new(0);
        let (result, allocations) = crate::test_support::measured_allocations(|| {
            session
                .update_canonical_present(1, actual, |_| {
                    reads.set(reads.get() + 1);
                    Srgb8::new([0xff; 3])
                })
                .map(|_| ())
        });
        assert_eq!(
            result,
            Err(SessionUpdateError::SurfaceInputPortLengthMismatch {
                expected: 1,
                actual,
            })
        );
        assert_eq!(reads.get(), 0);
        assert_eq!(allocations, 0);
    }

    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert!(matches!(
        session.state(),
        SessionState::Waiting {
            current_unavailable: None
        }
    ));
}

#[test]
fn borrowed_present_lower_revision_reads_zero_and_preserves_state() {
    let owner = compiled(0.5).into_owner();
    let mut session = owner.attach().unwrap();
    let white = Srgb8::new([0xff; 3]);
    session.update_canonical_present(5, 1, |_| white).unwrap();
    let storage = retained_signal_storage_pointers(session.state());
    crate::composition::reset_source_over_evaluation_count();
    let reads = Cell::new(0);

    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update_canonical_present(4, 1, |_| {
                reads.set(reads.get() + 1);
                Srgb8::new([0; 3])
            })
            .map(|_| ())
    });

    assert_eq!(
        result,
        Err(SessionUpdateError::RevisionOutOfOrder {
            current: 5,
            incoming: 4,
        })
    );
    assert_eq!(reads.get(), 0);
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(retained_signal_storage_pointers(session.state()), storage);
    let SessionState::Ready { current } = session.state() else {
        panic!("a lower revision must leave Ready untouched");
    };
    assert_eq!(current.revision(), 5);
    assert!(
        current
            .surfaces()
            .eq([SurfaceSignal::new(SURFACE_PORT, white)])
    );
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
    assert!(
        session
            .bound_surface_inputs_for_test()
            .eq([(SURFACE_PORT, white)])
    );
}

#[test]
fn same_revision_replay_and_conflict_read_every_value_without_mutation() {
    let compiled = multi_compiled();
    assert_eq!(compiled.surface_input_ports(), &MULTI_PORTS);
    let owner = compiled.into_owner();
    let mut session = owner.attach().unwrap();
    let committed_values = [
        Srgb8::new([0x10, 0x20, 0x30]),
        Srgb8::new([0xff; 3]),
        Srgb8::new([0x70, 0x80, 0x90]),
    ];
    session
        .update_canonical_present(5, MULTI_PORTS.len(), |index| committed_values[index])
        .unwrap();
    let storage = retained_signal_storage_pointers(session.state());
    crate::composition::reset_source_over_evaluation_count();

    let replay = ReadProbe::new(committed_values);
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update_canonical_present(5, MULTI_PORTS.len(), |index| replay.read(index))
            .map(|_| ())
    });
    assert_eq!(result, Ok(()));
    assert_eq!(replay.reads(), [1, 1, 1]);
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(retained_signal_storage_pointers(session.state()), storage);

    let mut conflicting_values = committed_values;
    conflicting_values[0] = Srgb8::new([0; 3]);
    let conflict = ReadProbe::new(conflicting_values);
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update_canonical_present(5, MULTI_PORTS.len(), |index| conflict.read(index))
            .map(|_| ())
    });
    assert_eq!(
        result,
        Err(SessionUpdateError::RevisionConflict { revision: 5 })
    );
    assert_eq!(conflict.reads(), [1, 1, 1]);
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(retained_signal_storage_pointers(session.state()), storage);

    let expected_surfaces = [
        SurfaceSignal::new(MULTI_PORT_A, committed_values[0]),
        SurfaceSignal::new(MULTI_PORT_B, committed_values[1]),
        SurfaceSignal::new(MULTI_PORT_C, committed_values[2]),
    ];
    let SessionState::Ready { current } = session.state() else {
        panic!("same-revision admission must retain Ready");
    };
    assert_eq!(current.revision(), 5);
    assert!(current.surfaces().eq(expected_surfaces));
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
    assert!(session.bound_surface_inputs_for_test().eq([
        (MULTI_PORT_A, committed_values[0]),
        (MULTI_PORT_B, committed_values[1]),
        (MULTI_PORT_C, committed_values[2]),
    ]));
}

#[test]
fn new_revision_reads_each_value_once_and_snapshots_binding_readback() {
    let owner = multi_compiled().into_owner();
    let mut session = owner.attach().unwrap();
    let initial_values = [
        Srgb8::new([0x10; 3]),
        Srgb8::new([0xff; 3]),
        Srgb8::new([0x30; 3]),
    ];
    session
        .update_canonical_present(1, MULTI_PORTS.len(), |index| initial_values[index])
        .unwrap();
    let storage = retained_signal_storage_pointers(session.state());
    let next_values = [
        Srgb8::new([0xa1, 0xa2, 0xa3]),
        Srgb8::new([0; 3]),
        Srgb8::new([0xc1, 0xc2, 0xc3]),
    ];
    let probe = ReadProbe::new(next_values);
    crate::composition::reset_source_over_evaluation_count();

    let (result, allocations) = crate::test_support::measured_allocations(|| {
        session
            .update_canonical_present(2, MULTI_PORTS.len(), |index| probe.read(index))
            .map(|_| ())
    });

    assert_eq!(result, Ok(()));
    assert_eq!(probe.reads(), [1, 1, 1]);
    assert_eq!(allocations, 0);
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    assert_eq!(retained_signal_storage_pointers(session.state()), storage);
    let expected_surfaces = [
        SurfaceSignal::new(MULTI_PORT_A, next_values[0]),
        SurfaceSignal::new(MULTI_PORT_B, next_values[1]),
        SurfaceSignal::new(MULTI_PORT_C, next_values[2]),
    ];
    let SessionState::Ready { current } = session.state() else {
        panic!("a new canonical revision must commit Ready");
    };
    assert_eq!(current.revision(), 2);
    assert!(current.surfaces().eq(expected_surfaces));
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0]);
    assert!(session.bound_surface_inputs_for_test().eq([
        (MULTI_PORT_A, next_values[0]),
        (MULTI_PORT_B, next_values[1]),
        (MULTI_PORT_C, next_values[2]),
    ]));
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
        concat!("PACK", "ED_ENCODED_"),
        concat!("Pack", "edEncoded"),
        "PointRenderSessionUpdateError",
        concat!("update_pa", "cked"),
        concat!("decode_encoded_", "surface_update"),
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
