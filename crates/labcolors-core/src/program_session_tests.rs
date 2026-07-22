use crate::Srgb8;
use crate::appearance::{
    AppearanceBindings, AppearanceGraphSpec, BindingError, ColorInputId, CompileError, OccurrenceId,
    OccurrenceSpec, OpacityInputId, PaintId, PaintSpec, SurfaceId, SurfaceInputPortId,
    SurfaceSpec,
};
use crate::composition::CompositionProfileV1;
use crate::program_session::{
    PACKED_ENCODED_SURFACE_PRESENT_TAG_V1, PACKED_ENCODED_SURFACE_UNAVAILABLE_TAG_V1,
    PACKED_ENCODED_SURFACE_UPDATE_MAGIC_V1, PackedEncodedSurfaceUpdateErrorV1,
    PointRenderEpochBuildErrorV1, PointRenderOwnerV1, PointRenderSessionStateV1,
    PointRenderSessionUpdateErrorV1,
};

const COLOR: ColorInputId = ColorInputId::new(1);
const SURFACE_PORT: SurfaceInputPortId = SurfaceInputPortId::new(2);
const OPACITY: OpacityInputId = OpacityInputId::new(3);
const SOLID: PaintId = PaintId::new(10);
const TRANSLUCENT: PaintId = PaintId::new(11);
const BACKDROP: SurfaceId = SurfaceId::new(20);
const VISIBLE: SurfaceId = SurfaceId::new(21);
const OCCURRENCE: OccurrenceId = OccurrenceId::new(30);

fn graph_spec() -> AppearanceGraphSpec {
    graph_spec_against(BACKDROP)
}

fn graph_spec_against(against: SurfaceId) -> AppearanceGraphSpec {
    AppearanceGraphSpec::new(
        vec![COLOR],
        vec![SURFACE_PORT],
        vec![OPACITY],
        vec![
            PaintSpec::Solid {
                id: SOLID,
                color: COLOR,
            },
            PaintSpec::Opacity {
                id: TRANSLUCENT,
                source: SOLID,
                opacity: OPACITY,
            },
        ],
        vec![
            SurfaceSpec::Input {
                id: BACKDROP,
                port: SURFACE_PORT,
            },
            SurfaceSpec::FromOccurrence {
                id: VISIBLE,
                occurrence: OCCURRENCE,
            },
        ],
        vec![OccurrenceSpec {
            id: OCCURRENCE,
            subject: TRANSLUCENT,
            against,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
}

fn cyclic_graph_spec() -> AppearanceGraphSpec {
    AppearanceGraphSpec::new(
        vec![COLOR],
        vec![SURFACE_PORT],
        vec![OPACITY],
        vec![
            PaintSpec::Solid {
                id: SOLID,
                color: COLOR,
            },
            PaintSpec::Opacity {
                id: TRANSLUCENT,
                source: SOLID,
                opacity: OPACITY,
            },
        ],
        vec![SurfaceSpec::FromOccurrence {
            id: VISIBLE,
            occurrence: OCCURRENCE,
        }],
        vec![OccurrenceSpec {
            id: OCCURRENCE,
            subject: TRANSLUCENT,
            against: VISIBLE,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
}

fn bindings(opacity: f64) -> AppearanceBindings {
    AppearanceBindings::new(
        vec![(COLOR, Srgb8::new([0; 3]))],
        vec![(SURFACE_PORT, Srgb8::new([0; 3]))],
        vec![(OPACITY, opacity)],
    )
}

fn point(revision: u64, rgb24: u32) -> Vec<u32> {
    vec![
        PACKED_ENCODED_SURFACE_UPDATE_MAGIC_V1,
        PACKED_ENCODED_SURFACE_PRESENT_TAG_V1,
        revision as u32,
        (revision >> 32) as u32,
        rgb24,
    ]
}

fn unavailable(revision: u64, reason: u32) -> Vec<u32> {
    vec![
        PACKED_ENCODED_SURFACE_UPDATE_MAGIC_V1,
        PACKED_ENCODED_SURFACE_UNAVAILABLE_TAG_V1,
        revision as u32,
        (revision >> 32) as u32,
        reason,
    ]
}

#[test]
fn point_update_executes_the_compiled_graph_and_commits_compact_occurrences() {
    let owner = PointRenderOwnerV1::new(graph_spec(), bindings(0.5)).unwrap();
    let mut session = owner.attach().unwrap();

    let PointRenderSessionStateV1::Ready { current } =
        session.update_packed(&point(1, 0xff_ff_ff)).unwrap()
    else {
        panic!("present encoded Surface signals must produce Ready");
    };
    assert_eq!(current.revision(), 1);
    assert_eq!(current.input_surface_signals_rgb24(), &[0xff_ff_ff]);
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
}

#[test]
fn successful_replace_revokes_old_sessions_without_a_numeric_generation() {
    let mut owner = PointRenderOwnerV1::new(graph_spec(), bindings(0.5)).unwrap();
    let mut old = owner.attach().unwrap();

    owner.replace(graph_spec(), bindings(0.25)).unwrap();
    assert_eq!(
        old.update_packed(&point(1, 0xff_ff_ff)),
        Err(PointRenderSessionUpdateErrorV1::ProgramExpired)
    );

    let mut current = owner.attach().unwrap();
    assert!(matches!(
        current.update_packed(&point(1, 0xff_ff_ff)).unwrap(),
        PointRenderSessionStateV1::Ready { .. }
    ));
}

#[test]
fn invalid_opacity_failed_replace_is_atomic_and_keeps_old_epoch_live() {
    let mut owner = PointRenderOwnerV1::new(graph_spec(), bindings(0.5)).unwrap();
    let mut session = owner.attach().unwrap();

    assert!(matches!(
        owner.replace(graph_spec(), bindings(1.25)),
        Err(PointRenderEpochBuildErrorV1::Bindings(
            BindingError::OpacityOutOfDomain { input: OPACITY, .. }
        ))
    ));
    let PointRenderSessionStateV1::Ready { current } =
        session.update_packed(&point(1, 0xff_ff_ff)).unwrap()
    else {
        panic!("failed replacement must not revoke the old epoch");
    };
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
}

#[test]
fn dangling_and_cyclic_failed_compiles_do_not_revoke_the_current_epoch() {
    let mut owner = PointRenderOwnerV1::new(graph_spec(), bindings(0.5)).unwrap();
    let mut session = owner.attach().unwrap();

    let missing = SurfaceId::new(999);
    assert!(matches!(
        owner.replace(graph_spec_against(missing), bindings(0.5)),
        Err(PointRenderEpochBuildErrorV1::Compile(
            CompileError::MissingOccurrenceBackdrop {
                occurrence: OCCURRENCE,
                surface
            }
        )) if surface == missing
    ));
    assert!(matches!(
        owner.replace(cyclic_graph_spec(), bindings(0.5)),
        Err(PointRenderEpochBuildErrorV1::Compile(
            CompileError::RenderCycle { .. }
        ))
    ));

    let PointRenderSessionStateV1::Ready { current } =
        session.update_packed(&point(1, 0xff_ff_ff)).unwrap()
    else {
        panic!("compile failures must leave the old strong epoch untouched");
    };
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
}

#[test]
fn dispose_revokes_sessions_and_prevents_new_attachment() {
    let mut owner = PointRenderOwnerV1::new(graph_spec(), bindings(0.5)).unwrap();
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
    let owner = PointRenderOwnerV1::new(graph_spec(), bindings(0.5)).unwrap();
    let mut session = owner.attach().unwrap();
    session.update_packed(&point(1, 0xff_ff_ff)).unwrap();

    let PointRenderSessionStateV1::Stale {
        previous,
        current_unavailable,
    } = session.update_packed(&unavailable(2, 91)).unwrap()
    else {
        panic!("unavailable Surface input after Ready must become Stale");
    };
    assert_eq!(previous.revision(), 1);
    assert_eq!(previous.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
    assert_eq!(current_unavailable.revision(), 2);
    assert_eq!(current_unavailable.reason(), 91);

    let PointRenderSessionStateV1::Stale { previous, .. } =
        session.update_packed(&unavailable(3, 92)).unwrap()
    else {
        panic!("a later unavailable update must remain Stale");
    };
    assert_eq!(previous.revision(), 1);
}

#[test]
fn malformed_lower_and_conflicting_updates_are_atomic_and_do_not_evaluate() {
    let owner = PointRenderOwnerV1::new(graph_spec(), bindings(0.5)).unwrap();
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

    let PointRenderSessionStateV1::Ready { current } = session.state() else {
        panic!("every rejected update must leave the committed state untouched");
    };
    assert_eq!(current.revision(), 5);
    assert_eq!(current.composited_occurrence_signals_rgb24(), &[0x80_80_80]);
}

#[test]
fn exact_replay_is_idempotent_but_a_new_revision_evaluates_again() {
    let owner = PointRenderOwnerV1::new(graph_spec(), bindings(0.5)).unwrap();
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
