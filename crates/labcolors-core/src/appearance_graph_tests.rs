//! Контрактные тесты приватного point-render стержня.
//!
//! Граф владеет только физической топологией: Paint материализуется независимо
//! от подложки, Occurrence является его единственным применением к Surface, а
//! `surfaceFrom` лишь даёт видимому результату повторно используемую identity.
//! Клиентский словарь и perception-утверждения сюда не входят.

use std::cell::Cell;

use proptest::prelude::*;

use crate::Srgb8;
use crate::appearance::{
    AdmittedAppearanceBindings, AppearanceBindings, AppearanceGraphSpec, BindingError,
    ColorInputId, CompileError, CompositionProfileV1, EncodedPointPaintV1, OccurrenceId,
    OccurrenceSpec, OpacityInputId, PaintId, PaintSpec, PointPresentationPathErrorV1, SurfaceId,
    SurfaceInputPortId, SurfaceSpec,
};
use crate::constraints::Evaluator;

const SOURCE: ColorInputId = ColorInputId::new(0);
const OTHER_SOURCE: ColorInputId = ColorInputId::new(2);
const CONTEXT: SurfaceInputPortId = SurfaceInputPortId::new(1);
const OTHER_CONTEXT: SurfaceInputPortId = SurfaceInputPortId::new(2);
const OPACITY: OpacityInputId = OpacityInputId::new(0);
const OTHER_OPACITY: OpacityInputId = OpacityInputId::new(1);
const SOLID_PAINT: PaintId = PaintId::new(70);
const FILL_PAINT: PaintId = PaintId::new(3);
const OTHER_PAINT: PaintId = PaintId::new(41);
const CONTEXT_SURFACE: SurfaceId = SurfaceId::new(90);
const DERIVED_SURFACE: SurfaceId = SurfaceId::new(2);
const FILL_OCCURRENCE: OccurrenceId = OccurrenceId::new(800);
const OTHER_OCCURRENCE: OccurrenceId = OccurrenceId::new(400);

fn point_component(reverse_paints: bool, reverse_surfaces: bool) -> AppearanceGraphSpec {
    let mut paints = vec![
        PaintSpec::Solid {
            id: SOLID_PAINT,
            color: SOURCE,
        },
        PaintSpec::Opacity {
            id: FILL_PAINT,
            source: SOLID_PAINT,
            opacity: OPACITY,
        },
    ];
    let mut surfaces = vec![
        SurfaceSpec::Input {
            id: CONTEXT_SURFACE,
            port: CONTEXT,
        },
        SurfaceSpec::FromOccurrence {
            id: DERIVED_SURFACE,
            occurrence: FILL_OCCURRENCE,
        },
    ];
    if reverse_paints {
        paints.reverse();
    }
    if reverse_surfaces {
        surfaces.reverse();
    }

    AppearanceGraphSpec::new(
        vec![SOURCE],
        vec![CONTEXT],
        vec![OPACITY],
        paints,
        surfaces,
        vec![OccurrenceSpec {
            id: FILL_OCCURRENCE,
            subject: FILL_PAINT,
            against: CONTEXT_SURFACE,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
}

fn bindings(source: [u8; 3], opacity: f64, context: [u8; 3]) -> AppearanceBindings {
    AppearanceBindings::new(
        vec![(SOURCE, Srgb8::new(source))],
        vec![(CONTEXT, Srgb8::new(context))],
        vec![(OPACITY, opacity)],
    )
}

fn terminal_chain() -> AppearanceGraphSpec {
    AppearanceGraphSpec::new(
        vec![SOURCE],
        vec![CONTEXT],
        vec![],
        vec![PaintSpec::Solid {
            id: SOLID_PAINT,
            color: SOURCE,
        }],
        vec![
            SurfaceSpec::Input {
                id: CONTEXT_SURFACE,
                port: CONTEXT,
            },
            SurfaceSpec::FromOccurrence {
                id: DERIVED_SURFACE,
                occurrence: FILL_OCCURRENCE,
            },
        ],
        vec![
            OccurrenceSpec {
                id: FILL_OCCURRENCE,
                subject: SOLID_PAINT,
                against: CONTEXT_SURFACE,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
            OccurrenceSpec {
                id: OTHER_OCCURRENCE,
                subject: SOLID_PAINT,
                against: DERIVED_SURFACE,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
        ],
    )
}

#[test]
fn presentation_root_authority_is_minted_only_for_a_terminal_occurrence() {
    let graph = terminal_chain().compile().unwrap();
    assert!(matches!(
        graph.compile_point_presentation_root(FILL_OCCURRENCE),
        Err(PointPresentationPathErrorV1::RootConsumedDownstream)
    ));
    let root = graph
        .compile_point_presentation_root(OTHER_OCCURRENCE)
        .unwrap();
    assert_eq!(root.terminal(), OTHER_OCCURRENCE);
    let path = graph
        .compile_point_presentation_path(FILL_OCCURRENCE, &root)
        .unwrap();
    assert_eq!(path.target(), FILL_OCCURRENCE);
    assert_eq!(path.root(), OTHER_OCCURRENCE);
    assert_eq!(path.len(), 2);
    assert!(path.belongs_to(&graph));
}

#[test]
fn presentation_root_authority_is_bound_to_its_compiled_graph() {
    let first = terminal_chain().compile().unwrap();
    let second = terminal_chain().compile().unwrap();
    let foreign_root = first
        .compile_point_presentation_root(OTHER_OCCURRENCE)
        .unwrap();
    assert!(matches!(
        second.compile_point_presentation_path(FILL_OCCURRENCE, &foreign_root),
        Err(PointPresentationPathErrorV1::IncompatibleRoot)
    ));
}

#[test]
fn presentation_path_reports_missing_root_and_target_at_the_graph_boundary() {
    let graph = terminal_chain().compile().unwrap();
    let missing = OccurrenceId::new(u32::MAX);
    assert!(matches!(
        graph.compile_point_presentation_root(missing),
        Err(PointPresentationPathErrorV1::MissingRoot)
    ));

    let root = graph
        .compile_point_presentation_root(OTHER_OCCURRENCE)
        .unwrap();
    assert!(matches!(
        graph.compile_point_presentation_path(missing, &root),
        Err(PointPresentationPathErrorV1::MissingTarget)
    ));
}

#[test]
fn presentation_path_rejects_a_target_outside_the_root_ancestry() {
    let graph = slot_component(false).compile().unwrap();
    let root = graph
        .compile_point_presentation_root(OTHER_OCCURRENCE)
        .unwrap();
    assert!(matches!(
        graph.compile_point_presentation_path(FILL_OCCURRENCE, &root),
        Err(PointPresentationPathErrorV1::TargetOutsideRootAncestry)
    ));
}

#[test]
fn compiled_appearance_graph_remains_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<crate::appearance::CompiledAppearanceGraph>();
}

fn slot_component(reverse_declarations: bool) -> AppearanceGraphSpec {
    let mut paints = vec![
        PaintSpec::Solid {
            id: SOLID_PAINT,
            color: SOURCE,
        },
        PaintSpec::Solid {
            id: FILL_PAINT,
            color: OTHER_SOURCE,
        },
    ];
    let mut occurrences = vec![
        OccurrenceSpec {
            id: FILL_OCCURRENCE,
            subject: FILL_PAINT,
            against: CONTEXT_SURFACE,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        },
        OccurrenceSpec {
            id: OTHER_OCCURRENCE,
            subject: SOLID_PAINT,
            against: CONTEXT_SURFACE,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        },
    ];
    if reverse_declarations {
        paints.reverse();
        occurrences.reverse();
    }

    AppearanceGraphSpec::new(
        vec![SOURCE, OTHER_SOURCE],
        vec![CONTEXT],
        vec![],
        paints,
        vec![SurfaceSpec::Input {
            id: CONTEXT_SURFACE,
            port: CONTEXT,
        }],
        occurrences,
    )
}

fn admitted_surface_triplet() -> AdmittedAppearanceBindings {
    let inputs = [
        SurfaceInputPortId::new(30),
        SurfaceInputPortId::new(10),
        SurfaceInputPortId::new(20),
    ];
    let graph = AppearanceGraphSpec::new(vec![], inputs.to_vec(), vec![], vec![], vec![], vec![])
        .compile()
        .unwrap();
    graph
        .admit_bindings(&AppearanceBindings::new(
            vec![],
            vec![
                (inputs[0], Srgb8::new([30; 3])),
                (inputs[1], Srgb8::new([10; 3])),
                (inputs[2], Srgb8::new([20; 3])),
            ],
            vec![],
        ))
        .unwrap()
}

#[test]
fn static_exact_program_is_declarative_topology_plus_typed_constraint() {
    let compiled = crate::appearance::point_opacity_over_surface_declarative_spec()
        .compile()
        .unwrap();
    assert!(crate::appearance::point_program_matches(&compiled));
    assert_eq!(
        crate::analog::ExactAlphaProgramV1::physical_identity(),
        crate::appearance::PhysicalProgramIdentityV1::SolidOpacityOverSurfaceEncodedSrgb8V1
    );
    assert_eq!(
        <crate::constraints::ExactSrgb8IdentityV1 as Evaluator<
            crate::appearance::ModeledSrgb8PointOccurrence,
        >>::identity(&crate::constraints::ExactSrgb8IdentityV1),
        crate::constraints::ExactConstraintIdentityV1::FinalSrgb8IdentityV1
    );
}

#[test]
fn canonical_paint_occurrence_surface_chain_evaluates_exactly() {
    let rendered = point_component(false, false)
        .compile()
        .unwrap()
        .evaluate(&bindings([0xFF, 0xA1, 0x00], 0.122, [0xFF; 3]))
        .unwrap();

    let solid = rendered.paint(SOLID_PAINT).unwrap();
    let _: &EncodedPointPaintV1 = solid;
    assert_eq!(solid.source().bytes(), [0xFF, 0xA1, 0x00]);
    assert_eq!(solid.opacity_bits(), 1.0f64.to_bits());
    let fill = rendered.paint(FILL_PAINT).unwrap();
    assert_eq!(fill.source().bytes(), [0xFF, 0xA1, 0x00]);
    assert_eq!(fill.opacity_bits(), 0.122f64.to_bits());
    let occurrence = rendered.occurrence(FILL_OCCURRENCE).unwrap();
    assert_eq!(occurrence.id(), FILL_OCCURRENCE);
    assert_eq!(occurrence.subject(), FILL_PAINT);
    assert_eq!(occurrence.against(), CONTEXT_SURFACE);
    assert_eq!(occurrence.backdrop(), [0xFF; 3]);
    assert_eq!(occurrence.visible(), [0xFF, 0xF4, 0xE0]);
    assert_eq!(
        rendered.surface_rgb(DERIVED_SURFACE),
        Some([0xFF, 0xF4, 0xE0])
    );
    assert_eq!(
        rendered.surface_rgb(DERIVED_SURFACE),
        Some(rendered.occurrence(FILL_OCCURRENCE).unwrap().visible())
    );
    assert_eq!(
        rendered
            .occurrence(FILL_OCCURRENCE)
            .unwrap()
            .certificate()
            .replay(),
        [0xFF, 0xF4, 0xE0]
    );
}

#[test]
fn compile_and_evaluate_ignore_declaration_order() {
    let canonical = point_component(false, false).compile().unwrap();
    let paints_reversed = point_component(true, false).compile().unwrap();
    let surfaces_reversed = point_component(false, true).compile().unwrap();
    let both_reversed = point_component(true, true).compile().unwrap();
    let values = bindings([19, 127, 241], 0.375, [247, 241, 233]);
    let expected = canonical.evaluate(&values).unwrap();
    assert_eq!(expected, paints_reversed.evaluate(&values).unwrap());
    assert_eq!(expected, surfaces_reversed.evaluate(&values).unwrap());
    assert_eq!(expected, both_reversed.evaluate(&values).unwrap());
}

#[test]
fn compiled_slots_bind_found_ids_and_reject_missing_ids() {
    let graph = point_component(false, false).compile().unwrap();

    assert!(graph.bind_paint(SOLID_PAINT).is_some());
    assert!(graph.bind_paint(FILL_PAINT).is_some());
    assert!(graph.bind_paint(PaintId::new(999)).is_none());
    assert!(graph.bind_occurrence(FILL_OCCURRENCE).is_some());
    assert!(graph.bind_occurrence(OccurrenceId::new(999)).is_none());
}

#[test]
fn compiled_slots_have_canonical_ordinals_across_declaration_permutations() {
    let canonical = slot_component(false).compile().unwrap();
    let reversed = slot_component(true).compile().unwrap();

    for paint in [FILL_PAINT, SOLID_PAINT] {
        assert_eq!(canonical.bind_paint(paint), reversed.bind_paint(paint));
    }
    for occurrence in [OTHER_OCCURRENCE, FILL_OCCURRENCE] {
        assert_eq!(
            canonical.bind_occurrence(occurrence),
            reversed.bind_occurrence(occurrence)
        );
    }
}

#[test]
fn occurrence_subject_is_an_exact_cold_lookup_across_declaration_permutations() {
    let canonical = slot_component(false).compile().unwrap();
    let reversed = slot_component(true).compile().unwrap();

    for (occurrence, subject) in [
        (FILL_OCCURRENCE, FILL_PAINT),
        (OTHER_OCCURRENCE, SOLID_PAINT),
    ] {
        assert_eq!(canonical.occurrence_subject(occurrence), Some(subject));
        assert_eq!(reversed.occurrence_subject(occurrence), Some(subject));
    }
    assert_eq!(
        canonical.occurrence_subject(OccurrenceId::new(u32::MAX)),
        None
    );
}

#[test]
fn evaluation_view_rejects_same_ordinal_slots_with_different_nominal_ids() {
    let compile_single = |paint, occurrence| {
        AppearanceGraphSpec::new(
            vec![SOURCE],
            vec![CONTEXT],
            vec![],
            vec![PaintSpec::Solid {
                id: paint,
                color: SOURCE,
            }],
            vec![SurfaceSpec::Input {
                id: CONTEXT_SURFACE,
                port: CONTEXT,
            }],
            vec![OccurrenceSpec {
                id: occurrence,
                subject: paint,
                against: CONTEXT_SURFACE,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            }],
        )
        .compile()
        .unwrap()
    };
    let graph = compile_single(FILL_PAINT, FILL_OCCURRENCE);
    let incompatible = compile_single(OTHER_PAINT, OTHER_OCCURRENCE);
    let incompatible_paint = incompatible.bind_paint(OTHER_PAINT).unwrap();
    let incompatible_occurrence = incompatible.bind_occurrence(OTHER_OCCURRENCE).unwrap();
    let admitted = graph
        .admit_bindings(&AppearanceBindings::new(
            vec![(SOURCE, Srgb8::new([10, 20, 30]))],
            vec![(CONTEXT, Srgb8::new([40, 50, 60]))],
            vec![],
        ))
        .unwrap();
    let mut workspace = graph.new_workspace().unwrap();
    let evaluated = graph
        .evaluate_admitted_into(&admitted, &mut workspace)
        .unwrap();

    assert!(evaluated.paint_at(incompatible_paint).is_none());
    assert!(evaluated.occurrence_at(incompatible_occurrence).is_none());
}

#[test]
fn prebound_view_lookup_returns_exact_values_and_allocates_nothing() {
    let graph = point_component(false, false).compile().unwrap();
    let admitted = graph
        .admit_bindings(&bindings([0xFF, 0xA1, 0x00], 0.122, [0xFF; 3]))
        .unwrap();
    let mut workspace = graph.new_workspace().unwrap();
    let evaluated = graph
        .evaluate_admitted_into(&admitted, &mut workspace)
        .unwrap();
    let paint_slot = graph.bind_paint(FILL_PAINT).unwrap();
    let occurrence_slot = graph.bind_occurrence(FILL_OCCURRENCE).unwrap();

    let (resolved, allocations) = crate::test_support::measured_allocations(|| {
        let paint = evaluated.paint_at(paint_slot);
        let occurrence = evaluated.occurrence_at(occurrence_slot);
        (
            paint.map(|paint| (paint.id(), paint.source(), paint.opacity_bits())),
            occurrence.map(|occurrence| {
                (
                    occurrence.id(),
                    occurrence.subject(),
                    occurrence.against(),
                    occurrence.backdrop(),
                    occurrence.visible(),
                    *occurrence.certificate(),
                )
            }),
        )
    });

    assert_eq!(allocations, 0);
    assert_eq!(
        resolved.0,
        Some((
            FILL_PAINT,
            Srgb8::new([0xFF, 0xA1, 0x00]),
            0.122f64.to_bits(),
        ))
    );
    let occurrence = resolved.1.unwrap();
    assert_eq!(occurrence.0, FILL_OCCURRENCE);
    assert_eq!(occurrence.1, FILL_PAINT);
    assert_eq!(occurrence.2, CONTEXT_SURFACE);
    assert_eq!(occurrence.3, [0xFF; 3]);
    assert_eq!(occurrence.4, [0xFF, 0xF4, 0xE0]);
    assert_eq!(occurrence.5.replay(), [0xFF, 0xF4, 0xE0]);
}

#[test]
fn complete_typed_id_renaming_does_not_change_physics() {
    let source = ColorInputId::new(700);
    let context = SurfaceInputPortId::new(42);
    let opacity = OpacityInputId::new(91);
    let solid = PaintId::new(901);
    let fill = PaintId::new(11);
    let context_surface = SurfaceId::new(800);
    let derived_surface = SurfaceId::new(12);
    let occurrence = OccurrenceId::new(501);
    let renamed = AppearanceGraphSpec::new(
        vec![source],
        vec![context],
        vec![opacity],
        vec![
            PaintSpec::Opacity {
                id: fill,
                source: solid,
                opacity,
            },
            PaintSpec::Solid {
                id: solid,
                color: source,
            },
        ],
        vec![
            SurfaceSpec::FromOccurrence {
                id: derived_surface,
                occurrence,
            },
            SurfaceSpec::Input {
                id: context_surface,
                port: context,
            },
        ],
        vec![OccurrenceSpec {
            id: occurrence,
            subject: fill,
            against: context_surface,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
    .compile()
    .unwrap();

    let source_rgb = [13, 89, 233];
    let context_rgb = [249, 245, 237];
    let alpha = 0.41;
    let first = point_component(false, false)
        .compile()
        .unwrap()
        .evaluate(&bindings(source_rgb, alpha, context_rgb))
        .unwrap();
    let second = renamed
        .evaluate(&AppearanceBindings::new(
            vec![(source, Srgb8::new(source_rgb))],
            vec![(context, Srgb8::new(context_rgb))],
            vec![(opacity, alpha)],
        ))
        .unwrap();

    assert_eq!(
        first.paint(FILL_PAINT).unwrap().source(),
        second.paint(fill).unwrap().source()
    );
    assert_eq!(
        first.paint(FILL_PAINT).unwrap().opacity_bits(),
        second.paint(fill).unwrap().opacity_bits()
    );
    assert_eq!(
        first.occurrence(FILL_OCCURRENCE).unwrap().visible(),
        second.occurrence(occurrence).unwrap().visible()
    );
    let first_occurrence = first.occurrence(FILL_OCCURRENCE).unwrap();
    let second_occurrence = second.occurrence(occurrence).unwrap();
    assert_eq!(
        first_occurrence.certificate(),
        second_occurrence.certificate(),
        "typed-ID rename must not change the ID-free physical proof"
    );
    assert_ne!(
        first_occurrence.program_occurrence_binding(),
        second_occurrence.program_occurrence_binding(),
        "program routing remains distinct provenance outside the physical proof"
    );
    assert_eq!(
        first.surface_rgb(DERIVED_SURFACE),
        second.surface_rgb(derived_surface)
    );
}

#[test]
fn equal_transport_numbers_remain_opaque_in_a_nested_occurrence_graph() {
    let first_color = ColorInputId::new(41);
    let second_color = ColorInputId::new(7);
    let surface_port = SurfaceInputPortId::new(41);
    let first_opacity = OpacityInputId::new(41);
    let second_opacity = OpacityInputId::new(7);
    let first_solid = PaintId::new(41);
    let first_modulated = PaintId::new(42);
    let second_solid = PaintId::new(7);
    let second_modulated = PaintId::new(8);
    let backdrop = SurfaceId::new(41);
    let derived = SurfaceId::new(7);
    let first_occurrence = OccurrenceId::new(41);
    let second_occurrence = OccurrenceId::new(7);

    let graph = AppearanceGraphSpec::new(
        vec![first_color, second_color],
        vec![surface_port],
        vec![first_opacity, second_opacity],
        vec![
            PaintSpec::Opacity {
                id: first_modulated,
                source: first_solid,
                opacity: first_opacity,
            },
            PaintSpec::Solid {
                id: second_solid,
                color: second_color,
            },
            PaintSpec::Opacity {
                id: second_modulated,
                source: second_solid,
                opacity: second_opacity,
            },
            PaintSpec::Solid {
                id: first_solid,
                color: first_color,
            },
        ],
        vec![
            SurfaceSpec::FromOccurrence {
                id: derived,
                occurrence: first_occurrence,
            },
            SurfaceSpec::Input {
                id: backdrop,
                port: surface_port,
            },
        ],
        vec![
            OccurrenceSpec {
                id: second_occurrence,
                subject: second_modulated,
                against: derived,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
            OccurrenceSpec {
                id: first_occurrence,
                subject: first_modulated,
                against: backdrop,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
        ],
    )
    .compile()
    .unwrap();
    let evaluated = graph
        .evaluate(&AppearanceBindings::new(
            vec![
                (first_color, Srgb8::new([200, 80, 40])),
                (second_color, Srgb8::new([30, 160, 230])),
            ],
            vec![(surface_port, Srgb8::new([20, 40, 60]))],
            vec![(first_opacity, 0.5), (second_opacity, 0.25)],
        ))
        .unwrap();

    let first = evaluated.occurrence(first_occurrence).unwrap();
    let second = evaluated.occurrence(second_occurrence).unwrap();
    assert_eq!(first.subject(), first_modulated);
    assert_eq!(first.against(), backdrop);
    assert_eq!(first.visible(), [110, 60, 50]);
    assert_eq!(evaluated.surface_rgb(derived), Some([110, 60, 50]));
    assert_eq!(second.subject(), second_modulated);
    assert_eq!(second.against(), derived);
    assert_eq!(second.backdrop(), first.visible());
    assert_eq!(second.visible(), [90, 85, 95]);
}

#[test]
fn occurrence_uses_the_declared_paint_not_an_unrelated_color_input() {
    let graph = AppearanceGraphSpec::new(
        vec![SOURCE, OTHER_SOURCE],
        vec![CONTEXT],
        vec![OPACITY],
        vec![
            PaintSpec::Solid {
                id: SOLID_PAINT,
                color: SOURCE,
            },
            PaintSpec::Opacity {
                id: FILL_PAINT,
                source: SOLID_PAINT,
                opacity: OPACITY,
            },
            PaintSpec::Solid {
                id: OTHER_PAINT,
                color: OTHER_SOURCE,
            },
        ],
        vec![SurfaceSpec::Input {
            id: CONTEXT_SURFACE,
            port: CONTEXT,
        }],
        vec![OccurrenceSpec {
            id: FILL_OCCURRENCE,
            subject: FILL_PAINT,
            against: CONTEXT_SURFACE,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
    .compile()
    .unwrap();
    let rendered = graph
        .evaluate(&AppearanceBindings::new(
            vec![
                (SOURCE, Srgb8::new([10, 20, 30])),
                (OTHER_SOURCE, Srgb8::new([111, 112, 113])),
            ],
            vec![(CONTEXT, Srgb8::new([200, 200, 200]))],
            vec![(OPACITY, 0.25)],
        ))
        .unwrap();
    let occurrence = rendered.occurrence(FILL_OCCURRENCE).unwrap();
    assert_eq!(occurrence.subject(), FILL_PAINT);
    assert_eq!(
        occurrence.visible(),
        crate::alpha::composite_over_srgb8([10, 20, 30], 0.25, [200; 3]).unwrap()
    );
    assert_ne!(occurrence.visible(), [111, 112, 113]);
}

#[test]
fn nested_opacity_materializes_once_by_multiplying_opacity() {
    let nested = PaintId::new(99);
    let graph = AppearanceGraphSpec::new(
        vec![SOURCE],
        vec![CONTEXT],
        vec![OTHER_OPACITY, OPACITY],
        vec![
            PaintSpec::Opacity {
                id: nested,
                source: FILL_PAINT,
                opacity: OTHER_OPACITY,
            },
            PaintSpec::Opacity {
                id: FILL_PAINT,
                source: SOLID_PAINT,
                opacity: OPACITY,
            },
            PaintSpec::Solid {
                id: SOLID_PAINT,
                color: SOURCE,
            },
        ],
        vec![SurfaceSpec::Input {
            id: CONTEXT_SURFACE,
            port: CONTEXT,
        }],
        vec![OccurrenceSpec {
            id: FILL_OCCURRENCE,
            subject: nested,
            against: CONTEXT_SURFACE,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
    .compile()
    .unwrap();
    let rendered = graph
        .evaluate(&AppearanceBindings::new(
            vec![(SOURCE, Srgb8::new([0; 3]))],
            vec![(CONTEXT, Srgb8::new([255; 3]))],
            vec![(OTHER_OPACITY, 0.5), (OPACITY, 0.5)],
        ))
        .unwrap();
    assert_eq!(
        rendered.paint(nested).unwrap().opacity_bits(),
        0.25f64.to_bits()
    );
    assert_eq!(
        rendered.occurrence(FILL_OCCURRENCE).unwrap().visible(),
        [191; 3]
    );
    assert_eq!(
        rendered
            .occurrence(FILL_OCCURRENCE)
            .unwrap()
            .certificate()
            .replay(),
        rendered.occurrence(FILL_OCCURRENCE).unwrap().visible()
    );
}

#[test]
fn nested_opacity_preserves_subnormal_and_rounds_underflow_to_positive_zero() {
    let nested = PaintId::new(99);
    let graph = AppearanceGraphSpec::new(
        vec![SOURCE],
        vec![CONTEXT],
        vec![OPACITY, OTHER_OPACITY],
        vec![
            PaintSpec::Solid {
                id: SOLID_PAINT,
                color: SOURCE,
            },
            PaintSpec::Opacity {
                id: FILL_PAINT,
                source: SOLID_PAINT,
                opacity: OPACITY,
            },
            PaintSpec::Opacity {
                id: nested,
                source: FILL_PAINT,
                opacity: OTHER_OPACITY,
            },
        ],
        vec![SurfaceSpec::Input {
            id: CONTEXT_SURFACE,
            port: CONTEXT,
        }],
        vec![OccurrenceSpec {
            id: FILL_OCCURRENCE,
            subject: nested,
            against: CONTEXT_SURFACE,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
    .compile()
    .unwrap();

    // IEEE-754 binary64, round-to-nearest ties-to-even: половина минимального
    // normal остаётся точным subnormal, а половина минимального subnormal
    // округляется к +0. Эти биты меняются только со сменой численного профиля.
    let cases = [
        (f64::MIN_POSITIVE, 0x0008_0000_0000_0000),
        (f64::from_bits(1), 0),
    ];
    for (inner, expected_bits) in cases {
        let rendered = graph
            .evaluate(&AppearanceBindings::new(
                vec![(SOURCE, Srgb8::new([255, 0, 0]))],
                vec![(CONTEXT, Srgb8::new([0; 3]))],
                vec![(OPACITY, inner), (OTHER_OPACITY, 0.5)],
            ))
            .unwrap();
        let occurrence = rendered.occurrence(FILL_OCCURRENCE).unwrap();

        assert_eq!(
            rendered.paint(nested).unwrap().opacity_bits(),
            expected_bits
        );
        assert_eq!(
            occurrence.certificate().subject_opacity_bits(),
            expected_bits
        );
        assert_eq!(occurrence.visible(), [0; 3]);
        assert_eq!(occurrence.certificate().replay(), [0; 3]);
    }
}

#[test]
fn opacity_constructor_edges_define_binary64_operation_order() {
    let alpha_a = OpacityInputId::new(10);
    let alpha_b = OpacityInputId::new(11);
    let alpha_c = OpacityInputId::new(12);
    // Это ULP-смещения +9, -3 и -8 от binary64(cbrt(1.5 / 255)) — границы
    // округления красного канала exact encoded-sRGB8 compositor между байтами
    // 1 и 2. Свидетель пересчитывается только при смене compositor-профиля,
    // его правила округления или контракта порядка binary64-операций.
    let values = [
        (alpha_a, f64::from_bits(0x3fc7_1b2a_949a_2779)),
        (alpha_b, f64::from_bits(0x3fc7_1b2a_949a_276d)),
        (alpha_c, f64::from_bits(0x3fc7_1b2a_949a_2768)),
    ];
    let render = |order: [OpacityInputId; 3]| {
        let first = PaintId::new(101);
        let second = PaintId::new(102);
        let third = PaintId::new(103);
        AppearanceGraphSpec::new(
            vec![SOURCE],
            vec![CONTEXT],
            vec![alpha_a, alpha_b, alpha_c],
            vec![
                PaintSpec::Solid {
                    id: SOLID_PAINT,
                    color: SOURCE,
                },
                PaintSpec::Opacity {
                    id: first,
                    source: SOLID_PAINT,
                    opacity: order[0],
                },
                PaintSpec::Opacity {
                    id: second,
                    source: first,
                    opacity: order[1],
                },
                PaintSpec::Opacity {
                    id: third,
                    source: second,
                    opacity: order[2],
                },
            ],
            vec![SurfaceSpec::Input {
                id: CONTEXT_SURFACE,
                port: CONTEXT,
            }],
            vec![OccurrenceSpec {
                id: FILL_OCCURRENCE,
                subject: third,
                against: CONTEXT_SURFACE,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            }],
        )
        .compile()
        .unwrap()
        .evaluate(&AppearanceBindings::new(
            vec![(SOURCE, Srgb8::new([255, 0, 0]))],
            vec![(CONTEXT, Srgb8::new([0; 3]))],
            values.to_vec(),
        ))
        .unwrap()
        .occurrence(FILL_OCCURRENCE)
        .unwrap()
        .visible()
    };

    // Это witness неассоциативности binary64, а не production-константы:
    // compiler обязан сохранять объявленный constructor order и не выполнять
    // алгебраически выглядящую, но побайтно ложную перегруппировку.
    assert_eq!(render([alpha_a, alpha_b, alpha_c]), [2, 0, 0]);
    assert_eq!(render([alpha_b, alpha_c, alpha_a]), [1, 0, 0]);
}

#[test]
fn one_paint_is_surface_agnostic_across_two_occurrences() {
    let black = SurfaceId::new(10);
    let white = SurfaceId::new(11);
    let on_black = OccurrenceId::new(10);
    let on_white = OccurrenceId::new(11);
    let graph = AppearanceGraphSpec::new(
        vec![SOURCE],
        vec![CONTEXT, OTHER_CONTEXT],
        vec![OPACITY],
        vec![
            PaintSpec::Solid {
                id: SOLID_PAINT,
                color: SOURCE,
            },
            PaintSpec::Opacity {
                id: FILL_PAINT,
                source: SOLID_PAINT,
                opacity: OPACITY,
            },
        ],
        vec![
            SurfaceSpec::Input {
                id: black,
                port: CONTEXT,
            },
            SurfaceSpec::Input {
                id: white,
                port: OTHER_CONTEXT,
            },
        ],
        vec![
            OccurrenceSpec {
                id: on_black,
                subject: FILL_PAINT,
                against: black,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
            OccurrenceSpec {
                id: on_white,
                subject: FILL_PAINT,
                against: white,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
        ],
    )
    .compile()
    .unwrap();
    let rendered = graph
        .evaluate(&AppearanceBindings::new(
            vec![(SOURCE, Srgb8::new([240, 60, 20]))],
            vec![
                (CONTEXT, Srgb8::new([0; 3])),
                (OTHER_CONTEXT, Srgb8::new([255; 3])),
            ],
            vec![(OPACITY, 0.5)],
        ))
        .unwrap();
    let first = rendered.occurrence(on_black).unwrap();
    let second = rendered.occurrence(on_white).unwrap();
    assert_eq!(first.subject(), FILL_PAINT);
    assert_eq!(second.subject(), FILL_PAINT);
    assert_ne!(first.visible(), second.visible());
    assert_eq!(first.certificate().replay(), first.visible());
    assert_eq!(second.certificate().replay(), second.visible());
}

#[test]
fn surface_from_reuses_visible_result_without_recompositing() {
    let black_color = ColorInputId::new(10);
    let red_color = ColorInputId::new(12);
    let page_port = SurfaceInputPortId::new(11);
    let black_solid = PaintId::new(10);
    let black_tint = PaintId::new(11);
    let red_solid = PaintId::new(12);
    let red_tint = PaintId::new(13);
    let black_alpha = OpacityInputId::new(10);
    let red_alpha = OpacityInputId::new(11);
    let page = SurfaceId::new(10);
    let first_surface = SurfaceId::new(11);
    let first = OccurrenceId::new(10);
    let second = OccurrenceId::new(11);
    let colors = vec![black_color, red_color];
    let surface_inputs = vec![page_port];
    let opacities = vec![black_alpha, red_alpha];
    let paints = vec![
        PaintSpec::Solid {
            id: black_solid,
            color: black_color,
        },
        PaintSpec::Opacity {
            id: black_tint,
            source: black_solid,
            opacity: black_alpha,
        },
        PaintSpec::Solid {
            id: red_solid,
            color: red_color,
        },
        PaintSpec::Opacity {
            id: red_tint,
            source: red_solid,
            opacity: red_alpha,
        },
    ];
    let surfaces = vec![
        SurfaceSpec::Input {
            id: page,
            port: page_port,
        },
        SurfaceSpec::FromOccurrence {
            id: first_surface,
            occurrence: first,
        },
    ];
    let occurrences = vec![
        OccurrenceSpec {
            id: first,
            subject: black_tint,
            against: page,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        },
        OccurrenceSpec {
            id: second,
            subject: red_tint,
            against: first_surface,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        },
    ];
    let graph = AppearanceGraphSpec::new(
        colors.clone(),
        surface_inputs.clone(),
        opacities.clone(),
        paints.clone(),
        surfaces.clone(),
        occurrences.clone(),
    )
    .compile()
    .unwrap();
    let mut reversed_occurrences = occurrences;
    reversed_occurrences.reverse();
    let reordered = AppearanceGraphSpec::new(
        colors,
        surface_inputs,
        opacities,
        paints,
        surfaces,
        reversed_occurrences,
    )
    .compile()
    .unwrap();
    let bindings = AppearanceBindings::new(
        vec![
            (black_color, Srgb8::new([0; 3])),
            (red_color, Srgb8::new([255, 0, 0])),
        ],
        vec![(page_port, Srgb8::new([255; 3]))],
        vec![(black_alpha, 0.25), (red_alpha, 0.5)],
    );
    let rendered = graph.evaluate(&bindings).unwrap();
    assert_eq!(rendered, reordered.evaluate(&bindings).unwrap());
    let first_occurrence = rendered.occurrence(first).unwrap();
    let second_occurrence = rendered.occurrence(second).unwrap();
    assert_eq!(first_occurrence.visible(), [191; 3]);
    assert_eq!(
        rendered.surface_rgb(first_surface),
        Some(first_occurrence.visible())
    );
    assert_eq!(second_occurrence.backdrop(), first_occurrence.visible());
    assert_eq!(second_occurrence.visible(), [223, 96, 96]);
    assert_ne!(second_occurrence.visible(), [255, 128, 128]);
    assert_eq!(
        first_occurrence.certificate().replay(),
        first_occurrence.visible()
    );
    assert_eq!(
        second_occurrence.certificate().replay(),
        second_occurrence.visible()
    );
}

proptest! {
    #[test]
    fn occurrence_equals_the_exact_compositor_for_arbitrary_point_inputs(
        source in any::<[u8; 3]>(),
        context in any::<[u8; 3]>(),
        opacity in 0.0f64..=1.0f64,
    ) {
        let rendered = point_component(false, false)
            .compile()
            .unwrap()
            .evaluate(&bindings(source, opacity, context))
            .unwrap();
        let oracle = crate::alpha::composite_over_srgb8(source, opacity, context).unwrap();
        prop_assert_eq!(rendered.occurrence(FILL_OCCURRENCE).unwrap().visible(), oracle);
        prop_assert_eq!(rendered.surface_rgb(DERIVED_SURFACE), Some(oracle));
    }

    #[test]
    fn occurrence_certificate_replays_to_the_visible_result(
        source in any::<[u8; 3]>(),
        context in any::<[u8; 3]>(),
        opacity in 0.0f64..=1.0f64,
    ) {
        let rendered = point_component(false, false)
            .compile()
            .unwrap()
            .evaluate(&bindings(source, opacity, context))
            .unwrap();
        let occurrence = rendered.occurrence(FILL_OCCURRENCE).unwrap();
        let certificate = occurrence.certificate();
        let program_occurrence = occurrence.program_occurrence_binding();
        prop_assert_eq!(certificate.profile(), CompositionProfileV1::EncodedSrgb8SourceOverV1);
        prop_assert_eq!(program_occurrence.occurrence(), FILL_OCCURRENCE);
        prop_assert_eq!(program_occurrence.subject(), FILL_PAINT);
        prop_assert_eq!(certificate.subject_rgb(), source);
        let canonical_opacity_bits = if opacity == 0.0 {
            0.0f64.to_bits()
        } else {
            opacity.to_bits()
        };
        prop_assert_eq!(certificate.subject_opacity_bits(), canonical_opacity_bits);
        prop_assert_eq!(program_occurrence.backdrop_surface(), CONTEXT_SURFACE);
        prop_assert_eq!(certificate.backdrop_rgb(), context);
        prop_assert_eq!(certificate.output_rgb(), rendered.occurrence(FILL_OCCURRENCE).unwrap().visible());
        prop_assert_eq!(certificate.replay(), certificate.output_rgb());
    }

    #[test]
    fn direct_encoded_paint_equals_graph_materialization(
        source in any::<[u8; 3]>(),
        context in any::<[u8; 3]>(),
        opacity in 0.0f64..=1.0f64,
    ) {
        let rendered = point_component(false, false)
            .compile()
            .unwrap()
            .evaluate(&bindings(source, opacity, context))
            .unwrap();
        let direct = EncodedPointPaintV1::from_admitted(
            FILL_PAINT,
            Srgb8::new(source),
            crate::composition::AdmittedOpacityV1::new(opacity).unwrap(),
        );

        prop_assert_eq!(*rendered.paint(FILL_PAINT).unwrap(), direct);
    }

    #[test]
    fn arbitrary_opacity_chain_is_one_paint_and_one_occurrence_composite(
        source in any::<[u8; 3]>(),
        context in any::<[u8; 3]>(),
        alphas in proptest::collection::vec(0.0f64..=1.0f64, 0..=8),
    ) {
        let opacity_inputs: Vec<OpacityInputId> = (0..alphas.len())
            .map(|index| OpacityInputId::new(index as u32))
            .collect();
        let mut paints = vec![PaintSpec::Solid {
            id: SOLID_PAINT,
            color: SOURCE,
        }];
        let mut subject = SOLID_PAINT;
        for (index, opacity) in opacity_inputs.iter().copied().enumerate() {
            let next = PaintId::new(1_000 + index as u32);
            paints.push(PaintSpec::Opacity {
                id: next,
                source: subject,
                opacity,
            });
            subject = next;
        }
        let graph = AppearanceGraphSpec::new(
            vec![SOURCE],
            vec![CONTEXT],
            opacity_inputs.clone(),
            paints,
            vec![SurfaceSpec::Input {
                id: CONTEXT_SURFACE,
                port: CONTEXT,
            }],
            vec![OccurrenceSpec {
                id: FILL_OCCURRENCE,
                subject,
                against: CONTEXT_SURFACE,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            }],
        )
        .compile()
        .unwrap();
        let effective = alphas.iter().copied().fold(1.0, |product, alpha| product * alpha);
        let rendered = graph
            .evaluate(&AppearanceBindings::new(
                vec![(SOURCE, Srgb8::new(source))],
                vec![(CONTEXT, Srgb8::new(context))],
                opacity_inputs
                    .iter()
                    .copied()
                    .zip(alphas.iter().copied())
                    .collect(),
            ))
            .unwrap();
        let occurrence = rendered.occurrence(FILL_OCCURRENCE).unwrap();
        let oracle = crate::alpha::composite_over_srgb8(source, effective, context).unwrap();
        prop_assert_eq!(rendered.paint(subject).unwrap().opacity_bits(), effective.to_bits());
        prop_assert_eq!(occurrence.visible(), oracle);
        prop_assert_eq!(occurrence.certificate().replay(), oracle);
    }

    #[test]
    fn invalid_alpha_is_rejected_at_its_exact_chain_input(
        bits in any::<u64>(),
        invalid_is_outer in any::<bool>(),
    ) {
        let sampled = f64::from_bits(bits);
        let invalid = if !sampled.is_finite() || !(0.0..=1.0).contains(&sampled) {
            sampled
        } else {
            2.0 + sampled
        };
        let outer_paint = PaintId::new(99);
        let graph = AppearanceGraphSpec::new(
            vec![SOURCE],
            vec![CONTEXT],
            vec![OPACITY, OTHER_OPACITY],
            vec![
                PaintSpec::Solid {
                    id: SOLID_PAINT,
                    color: SOURCE,
                },
                PaintSpec::Opacity {
                    id: FILL_PAINT,
                    source: SOLID_PAINT,
                    opacity: OPACITY,
                },
                PaintSpec::Opacity {
                    id: outer_paint,
                    source: FILL_PAINT,
                    opacity: OTHER_OPACITY,
                },
            ],
            vec![SurfaceSpec::Input {
                id: CONTEXT_SURFACE,
                port: CONTEXT,
            }],
            vec![OccurrenceSpec {
                id: FILL_OCCURRENCE,
                subject: outer_paint,
                against: CONTEXT_SURFACE,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            }],
        )
        .compile()
        .unwrap();
        let (inner, outer, expected_input) = if invalid_is_outer {
            (0.5, invalid, OTHER_OPACITY)
        } else {
            (invalid, 0.5, OPACITY)
        };
        let expected_reason =
            crate::composition::AdmittedOpacityV1::new(invalid).unwrap_err();
        prop_assert_eq!(
            graph.evaluate(&AppearanceBindings::new(
                vec![(SOURCE, Srgb8::new([1, 2, 3]))],
                vec![(CONTEXT, Srgb8::new([4, 5, 6]))],
                vec![(OPACITY, inner), (OTHER_OPACITY, outer)],
            )),
            Err(BindingError::OpacityOutOfDomain {
                input: expected_input,
                reason: expected_reason,
            })
        );
    }

    #[test]
    fn encoded_point_paint_totally_classifies_every_binary64_alpha(bits in any::<u64>()) {
        let alpha = f64::from_bits(bits);
        let admission = crate::composition::AdmittedOpacityV1::new(alpha);

        if !alpha.is_finite() {
            prop_assert_eq!(
                admission,
                Err(crate::composition::OpacityAdmissionErrorV1::NonFinite)
            );
        } else if !(0.0..=1.0).contains(&alpha) {
            prop_assert_eq!(
                admission,
                Err(crate::composition::OpacityAdmissionErrorV1::OutsideUnitInterval)
            );
        } else {
            let paint = EncodedPointPaintV1::from_admitted(
                FILL_PAINT,
                Srgb8::new([1, 2, 3]),
                admission.unwrap(),
            );
            let expected_bits = if alpha == 0.0 {
                0.0f64.to_bits()
            } else {
                bits
            };
            prop_assert_eq!(paint.id(), FILL_PAINT);
            prop_assert_eq!(paint.source(), Srgb8::new([1, 2, 3]));
            prop_assert_eq!(paint.opacity_bits(), expected_bits);
        }
    }
}

#[test]
fn compile_rejects_duplicate_declarations_with_typed_errors() {
    assert_eq!(
        AppearanceGraphSpec::new(vec![SOURCE, SOURCE], vec![], vec![], vec![], vec![], vec![])
            .compile()
            .unwrap_err(),
        CompileError::DuplicateColorInput { input: SOURCE }
    );
    assert_eq!(
        AppearanceGraphSpec::new(
            vec![],
            vec![],
            vec![OPACITY, OPACITY],
            vec![],
            vec![],
            vec![]
        )
        .compile()
        .unwrap_err(),
        CompileError::DuplicateOpacityInput { input: OPACITY }
    );
    assert_eq!(
        AppearanceGraphSpec::new(
            vec![],
            vec![CONTEXT, CONTEXT],
            vec![],
            vec![],
            vec![],
            vec![]
        )
        .compile()
        .unwrap_err(),
        CompileError::DuplicateSurfaceInputPort { input: CONTEXT }
    );
    assert_eq!(
        AppearanceGraphSpec::new(
            vec![SOURCE, OTHER_SOURCE],
            vec![],
            vec![],
            vec![
                PaintSpec::Solid {
                    id: SOLID_PAINT,
                    color: SOURCE,
                },
                PaintSpec::Solid {
                    id: SOLID_PAINT,
                    color: OTHER_SOURCE,
                },
            ],
            vec![],
            vec![]
        )
        .compile()
        .unwrap_err(),
        CompileError::DuplicatePaint { paint: SOLID_PAINT }
    );
    assert_eq!(
        AppearanceGraphSpec::new(
            vec![],
            vec![CONTEXT, OTHER_CONTEXT],
            vec![],
            vec![],
            vec![
                SurfaceSpec::Input {
                    id: CONTEXT_SURFACE,
                    port: CONTEXT,
                },
                SurfaceSpec::Input {
                    id: CONTEXT_SURFACE,
                    port: OTHER_CONTEXT,
                },
            ],
            vec![]
        )
        .compile()
        .unwrap_err(),
        CompileError::DuplicateSurface {
            surface: CONTEXT_SURFACE
        }
    );
    assert_eq!(
        AppearanceGraphSpec::new(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![
                OccurrenceSpec {
                    id: FILL_OCCURRENCE,
                    subject: FILL_PAINT,
                    against: CONTEXT_SURFACE,
                    profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
                },
                OccurrenceSpec {
                    id: FILL_OCCURRENCE,
                    subject: OTHER_PAINT,
                    against: DERIVED_SURFACE,
                    profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
                },
            ]
        )
        .compile()
        .unwrap_err(),
        CompileError::DuplicateOccurrence {
            occurrence: FILL_OCCURRENCE
        }
    );
}

#[test]
fn compile_rejects_every_dangling_canonical_edge() {
    let missing_color = AppearanceGraphSpec::new(
        vec![],
        vec![],
        vec![],
        vec![PaintSpec::Solid {
            id: SOLID_PAINT,
            color: SOURCE,
        }],
        vec![],
        vec![],
    )
    .compile();
    assert_eq!(
        missing_color.unwrap_err(),
        CompileError::MissingPaintColorInput {
            paint: SOLID_PAINT,
            input: SOURCE,
        }
    );

    let missing_paint = AppearanceGraphSpec::new(
        vec![],
        vec![],
        vec![OPACITY],
        vec![PaintSpec::Opacity {
            id: FILL_PAINT,
            source: SOLID_PAINT,
            opacity: OPACITY,
        }],
        vec![],
        vec![],
    )
    .compile();
    assert_eq!(
        missing_paint.unwrap_err(),
        CompileError::MissingPaintSource {
            paint: FILL_PAINT,
            source: SOLID_PAINT,
        }
    );

    let missing_opacity = AppearanceGraphSpec::new(
        vec![SOURCE],
        vec![],
        vec![],
        vec![
            PaintSpec::Solid {
                id: SOLID_PAINT,
                color: SOURCE,
            },
            PaintSpec::Opacity {
                id: FILL_PAINT,
                source: SOLID_PAINT,
                opacity: OPACITY,
            },
        ],
        vec![],
        vec![],
    )
    .compile();
    assert_eq!(
        missing_opacity.unwrap_err(),
        CompileError::MissingPaintOpacityInput {
            paint: FILL_PAINT,
            input: OPACITY,
        }
    );

    let missing_surface_input = AppearanceGraphSpec::new(
        vec![],
        vec![],
        vec![],
        vec![],
        vec![SurfaceSpec::Input {
            id: CONTEXT_SURFACE,
            port: CONTEXT,
        }],
        vec![],
    )
    .compile();
    assert_eq!(
        missing_surface_input.unwrap_err(),
        CompileError::MissingSurfaceInputPort {
            surface: CONTEXT_SURFACE,
            input: CONTEXT,
        }
    );

    let missing_surface_occurrence = AppearanceGraphSpec::new(
        vec![],
        vec![],
        vec![],
        vec![],
        vec![SurfaceSpec::FromOccurrence {
            id: DERIVED_SURFACE,
            occurrence: FILL_OCCURRENCE,
        }],
        vec![],
    )
    .compile();
    assert_eq!(
        missing_surface_occurrence.unwrap_err(),
        CompileError::MissingSurfaceOccurrence {
            surface: DERIVED_SURFACE,
            occurrence: FILL_OCCURRENCE,
        }
    );

    let missing_occurrence_paint = AppearanceGraphSpec::new(
        vec![],
        vec![CONTEXT],
        vec![],
        vec![],
        vec![SurfaceSpec::Input {
            id: CONTEXT_SURFACE,
            port: CONTEXT,
        }],
        vec![OccurrenceSpec {
            id: FILL_OCCURRENCE,
            subject: FILL_PAINT,
            against: CONTEXT_SURFACE,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
    .compile();
    assert_eq!(
        missing_occurrence_paint.unwrap_err(),
        CompileError::MissingOccurrencePaint {
            occurrence: FILL_OCCURRENCE,
            paint: FILL_PAINT,
        }
    );

    let missing_occurrence_backdrop = AppearanceGraphSpec::new(
        vec![SOURCE],
        vec![],
        vec![],
        vec![PaintSpec::Solid {
            id: SOLID_PAINT,
            color: SOURCE,
        }],
        vec![],
        vec![OccurrenceSpec {
            id: FILL_OCCURRENCE,
            subject: SOLID_PAINT,
            against: CONTEXT_SURFACE,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
    .compile();
    assert_eq!(
        missing_occurrence_backdrop.unwrap_err(),
        CompileError::MissingOccurrenceBackdrop {
            occurrence: FILL_OCCURRENCE,
            surface: CONTEXT_SURFACE,
        }
    );
}

#[test]
fn cycle_errors_contain_only_actual_cycle_members() {
    let cycle_a = PaintId::new(10);
    let cycle_b = PaintId::new(20);
    let dependent = PaintId::new(1);
    let paint_cycle = AppearanceGraphSpec::new(
        vec![],
        vec![],
        vec![OPACITY],
        vec![
            PaintSpec::Opacity {
                id: cycle_a,
                source: cycle_b,
                opacity: OPACITY,
            },
            PaintSpec::Opacity {
                id: cycle_b,
                source: cycle_a,
                opacity: OPACITY,
            },
            PaintSpec::Opacity {
                id: dependent,
                source: cycle_a,
                opacity: OPACITY,
            },
        ],
        vec![],
        vec![],
    )
    .compile();
    assert_eq!(
        paint_cycle.unwrap_err(),
        CompileError::PaintCycle {
            paints: vec![cycle_a, cycle_b],
        }
    );

    let cycle_surface = SurfaceId::new(10);
    let dependent_surface = SurfaceId::new(1);
    let cycle_occurrence = OccurrenceId::new(10);
    let dependent_occurrence = OccurrenceId::new(1);
    let render_cycle = AppearanceGraphSpec::new(
        vec![SOURCE],
        vec![],
        vec![],
        vec![PaintSpec::Solid {
            id: SOLID_PAINT,
            color: SOURCE,
        }],
        vec![
            SurfaceSpec::FromOccurrence {
                id: cycle_surface,
                occurrence: cycle_occurrence,
            },
            SurfaceSpec::FromOccurrence {
                id: dependent_surface,
                occurrence: dependent_occurrence,
            },
        ],
        vec![
            OccurrenceSpec {
                id: cycle_occurrence,
                subject: SOLID_PAINT,
                against: cycle_surface,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
            OccurrenceSpec {
                id: dependent_occurrence,
                subject: SOLID_PAINT,
                against: cycle_surface,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
        ],
    )
    .compile();
    assert_eq!(
        render_cycle.unwrap_err(),
        CompileError::RenderCycle {
            surfaces: vec![cycle_surface],
            occurrences: vec![cycle_occurrence],
        }
    );
}

#[test]
fn evaluate_rejects_duplicate_missing_and_unexpected_bindings() {
    let graph = point_component(false, false).compile().unwrap();
    assert_eq!(
        graph.evaluate(&AppearanceBindings::new(
            vec![
                (SOURCE, Srgb8::new([1, 2, 3])),
                (SOURCE, Srgb8::new([7, 8, 9])),
            ],
            vec![(CONTEXT, Srgb8::new([4, 5, 6]))],
            vec![(OPACITY, 0.5)],
        )),
        Err(BindingError::DuplicateColorBinding { input: SOURCE })
    );
    assert_eq!(
        graph.evaluate(&AppearanceBindings::new(
            vec![(SOURCE, Srgb8::new([1, 2, 3]))],
            vec![(CONTEXT, Srgb8::new([4, 5, 6]))],
            vec![(OPACITY, 0.5), (OPACITY, 0.6)],
        )),
        Err(BindingError::DuplicateOpacityBinding { input: OPACITY })
    );
    assert_eq!(
        graph.evaluate(&AppearanceBindings::new(
            vec![],
            vec![(CONTEXT, Srgb8::new([4, 5, 6]))],
            vec![(OPACITY, 0.5)],
        )),
        Err(BindingError::MissingColorBinding { input: SOURCE })
    );
    assert_eq!(
        graph.evaluate(&AppearanceBindings::new(
            vec![(SOURCE, Srgb8::new([1, 2, 3]))],
            vec![],
            vec![(OPACITY, 0.5)],
        )),
        Err(BindingError::MissingSurfaceInputBinding { input: CONTEXT })
    );
    assert_eq!(
        graph.evaluate(&AppearanceBindings::new(
            vec![(SOURCE, Srgb8::new([1, 2, 3]))],
            vec![(CONTEXT, Srgb8::new([4, 5, 6]))],
            vec![],
        )),
        Err(BindingError::MissingOpacityBinding { input: OPACITY })
    );
    assert_eq!(
        graph.evaluate(&AppearanceBindings::new(
            vec![
                (SOURCE, Srgb8::new([1, 2, 3])),
                (ColorInputId::new(9), Srgb8::new([7, 8, 9])),
            ],
            vec![(CONTEXT, Srgb8::new([4, 5, 6]))],
            vec![(OPACITY, 0.5)],
        )),
        Err(BindingError::UnexpectedColorBinding {
            input: ColorInputId::new(9),
        })
    );
    assert_eq!(
        graph.evaluate(&AppearanceBindings::new(
            vec![(SOURCE, Srgb8::new([1, 2, 3]))],
            vec![(CONTEXT, Srgb8::new([4, 5, 6]))],
            vec![(OPACITY, 0.5), (OpacityInputId::new(9), 0.5)],
        )),
        Err(BindingError::UnexpectedOpacityBinding {
            input: OpacityInputId::new(9),
        })
    );
    assert_eq!(
        graph.evaluate(&AppearanceBindings::new(
            vec![(SOURCE, Srgb8::new([1, 2, 3]))],
            vec![
                (CONTEXT, Srgb8::new([4, 5, 6])),
                (SurfaceInputPortId::new(9), Srgb8::new([7, 8, 9])),
            ],
            vec![(OPACITY, 0.5)],
        )),
        Err(BindingError::UnexpectedSurfaceInputBinding {
            input: SurfaceInputPortId::new(9),
        })
    );
}

#[test]
fn binding_admission_is_atomic_before_any_occurrence_is_evaluated() {
    let graph = point_component(false, false).compile().unwrap();

    crate::composition::reset_source_over_evaluation_count();
    let duplicate_surface = graph.evaluate(&AppearanceBindings::new(
        vec![(SOURCE, Srgb8::new([1, 2, 3]))],
        vec![
            (CONTEXT, Srgb8::new([4, 5, 6])),
            (CONTEXT, Srgb8::new([7, 8, 9])),
        ],
        vec![(OPACITY, 0.5)],
    ));
    assert_eq!(
        duplicate_surface,
        Err(BindingError::DuplicateSurfaceInputBinding { input: CONTEXT })
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    crate::composition::reset_source_over_evaluation_count();
    let invalid_opacity = graph.evaluate(&bindings([1, 2, 3], f64::NAN, [4, 5, 6]));
    assert_eq!(
        invalid_opacity,
        Err(BindingError::OpacityOutOfDomain {
            input: OPACITY,
            reason: crate::composition::OpacityAdmissionErrorV1::NonFinite,
        })
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);

    let evaluated = graph
        .evaluate(&bindings([1, 2, 3], 0.5, [4, 5, 6]))
        .unwrap();
    assert_eq!(crate::composition::source_over_evaluation_count(), 1);
    assert!(evaluated.occurrence(FILL_OCCURRENCE).is_some());
}

#[test]
fn evaluate_rejects_invalid_alpha_with_the_typed_admission_reason() {
    let graph = point_component(false, false).compile().unwrap();
    for bad_alpha in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.5] {
        let expected = crate::composition::AdmittedOpacityV1::new(bad_alpha)
            .expect_err("общий admission обязан отвергать alpha");
        assert_eq!(
            graph.evaluate(&bindings([1, 2, 3], bad_alpha, [4, 5, 6])),
            Err(BindingError::OpacityOutOfDomain {
                input: OPACITY,
                reason: expected,
            }),
            "alpha={bad_alpha}"
        );
    }
}

#[test]
fn signed_zero_opacity_has_one_canonical_state() {
    let graph = point_component(false, false).compile().unwrap();
    let positive = graph
        .evaluate(&bindings([1, 2, 3], 0.0, [4, 5, 6]))
        .unwrap();
    let negative = graph
        .evaluate(&bindings([1, 2, 3], -0.0, [4, 5, 6]))
        .unwrap();
    assert_eq!(positive, negative);
    assert_eq!(
        negative.paint(FILL_PAINT).unwrap().opacity_bits(),
        0.0f64.to_bits()
    );
}

#[test]
fn canonical_surface_overwrite_rejects_every_schema_drift_before_read_or_mutation() {
    let mut admitted = admitted_surface_triplet();
    let first = SurfaceInputPortId::new(10);
    let second = SurfaceInputPortId::new(20);
    let third = SurfaceInputPortId::new(30);
    let original = [
        (first, Srgb8::new([10; 3])),
        (second, Srgb8::new([20; 3])),
        (third, Srgb8::new([30; 3])),
    ];
    let reordered = [second, first, third];
    let relabelled = [first, SurfaceInputPortId::new(21), third];
    let truncated = [first, second];
    let extended = [first, second, third, SurfaceInputPortId::new(40)];

    for expected in [
        reordered.as_slice(),
        relabelled.as_slice(),
        truncated.as_slice(),
        extended.as_slice(),
    ] {
        let reads = Cell::new(0);
        let (result, allocations) = crate::test_support::measured_allocations(|| {
            admitted.overwrite_surface_inputs_canonical(expected.iter().copied(), |_| {
                reads.set(reads.get() + 1);
                Srgb8::new([0; 3])
            })
        });

        assert_eq!(result, Err(BindingError::IncompatibleAdmittedBindings));
        assert_eq!(reads.get(), 0);
        assert_eq!(allocations, 0);
        assert!(admitted.surface_inputs_canonical().eq(original));
    }
}

#[test]
fn canonical_surface_overwrite_reads_once_and_exposes_all_values_without_allocation() {
    let mut admitted = admitted_surface_triplet();
    let expected_inputs = [
        SurfaceInputPortId::new(10),
        SurfaceInputPortId::new(20),
        SurfaceInputPortId::new(30),
    ];

    admitted
        .overwrite_surface_inputs_canonical(expected_inputs, |index| Srgb8::new([index as u8; 3]))
        .unwrap();

    let values = [
        Srgb8::new([101, 102, 103]),
        Srgb8::new([111, 112, 113]),
        Srgb8::new([121, 122, 123]),
    ];
    let reads = Cell::new([0_usize; 3]);
    let (result, allocations) = crate::test_support::measured_allocations(|| {
        admitted.overwrite_surface_inputs_canonical(expected_inputs, |index| {
            let mut counts = reads.get();
            counts[index] += 1;
            reads.set(counts);
            values[index]
        })?;
        Ok::<_, BindingError>(
            admitted
                .surface_inputs_canonical()
                .eq(expected_inputs.into_iter().zip(values)),
        )
    });

    assert_eq!(result, Ok(true));
    assert_eq!(reads.get(), [1, 1, 1]);
    assert_eq!(allocations, 0);
}

#[test]
fn admitted_surface_runtime_exposes_only_canonical_bulk_seams() {
    let source = include_str!("appearance.rs");
    let forbidden = concat!("set_surface_", "input");
    assert!(
        !source.contains(forbidden),
        "per-port Surface mutation must not return after the canonical bulk cut"
    );
    for required in [
        "overwrite_surface_inputs_canonical",
        "surface_inputs_canonical",
    ] {
        assert!(
            source.contains(required),
            "canonical runtime seam `{required}` must remain"
        );
    }
}

#[test]
fn admitted_bindings_and_workspace_are_reused_without_storage_churn() {
    let graph = point_component(false, false).compile().unwrap();
    let mut admitted = graph
        .admit_bindings(&bindings([200, 80, 40], 0.5, [20, 40, 60]))
        .unwrap();
    let mut independent = admitted.try_clone_v1().unwrap();
    assert_eq!(independent, admitted);
    independent
        .overwrite_surface_inputs_canonical([CONTEXT], |_| Srgb8::new([1, 2, 3]))
        .unwrap();
    assert_ne!(independent, admitted);
    assert_eq!(admitted.opacity_bits(OPACITY), Some(0.5f64.to_bits()));
    assert_eq!(
        graph.occurrence_ids().collect::<Vec<_>>(),
        vec![FILL_OCCURRENCE]
    );
    assert_eq!(
        graph.surface_input_ports().collect::<Vec<_>>(),
        vec![CONTEXT]
    );

    let mut workspace = graph.new_workspace().unwrap();
    let storage = workspace.storage_signature();
    for (backdrop, expected) in [
        ([20, 40, 60], [110, 60, 50]),
        ([100, 100, 100], [150, 90, 70]),
    ] {
        admitted
            .overwrite_surface_inputs_canonical([CONTEXT], |_| Srgb8::new(backdrop))
            .unwrap();
        {
            let evaluated = graph
                .evaluate_admitted_into(&admitted, &mut workspace)
                .unwrap();
            assert_eq!(
                evaluated.paint(FILL_PAINT).unwrap().opacity_bits(),
                0.5f64.to_bits()
            );
            assert_eq!(evaluated.surface_rgb(CONTEXT_SURFACE), Some(backdrop));
            assert_eq!(
                evaluated.occurrence(FILL_OCCURRENCE).unwrap().visible(),
                expected
            );
            assert_eq!(
                evaluated
                    .occurrences()
                    .map(|occurrence| occurrence.id())
                    .collect::<Vec<_>>(),
                vec![FILL_OCCURRENCE]
            );
        }
        assert_eq!(workspace.storage_signature(), storage);
    }
}

#[test]
fn admitted_schema_and_workspace_shape_mismatches_fail_before_composition() {
    let graph = point_component(false, false).compile().unwrap();
    let admitted = graph
        .admit_bindings(&bindings([1, 2, 3], 0.5, [4, 5, 6]))
        .unwrap();
    let empty = AppearanceGraphSpec::new(vec![], vec![], vec![], vec![], vec![], vec![])
        .compile()
        .unwrap();
    let mut wrong_workspace = empty.new_workspace().unwrap();
    let wrong_storage = wrong_workspace.storage_signature();

    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        graph
            .evaluate_admitted_into(&admitted, &mut wrong_workspace)
            .unwrap_err(),
        BindingError::IncompatibleWorkspace
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(wrong_workspace.storage_signature(), wrong_storage);

    let other_source = ColorInputId::new(100);
    let other_context = SurfaceInputPortId::new(101);
    let other_opacity = OpacityInputId::new(102);
    let other_solid = PaintId::new(103);
    let other_paint = PaintId::new(104);
    let other_surface = SurfaceId::new(105);
    let other_derived = SurfaceId::new(106);
    let other_occurrence = OccurrenceId::new(107);
    let other = AppearanceGraphSpec::new(
        vec![other_source],
        vec![other_context],
        vec![other_opacity],
        vec![
            PaintSpec::Solid {
                id: other_solid,
                color: other_source,
            },
            PaintSpec::Opacity {
                id: other_paint,
                source: other_solid,
                opacity: other_opacity,
            },
        ],
        vec![
            SurfaceSpec::Input {
                id: other_surface,
                port: other_context,
            },
            SurfaceSpec::FromOccurrence {
                id: other_derived,
                occurrence: other_occurrence,
            },
        ],
        vec![OccurrenceSpec {
            id: other_occurrence,
            subject: other_paint,
            against: other_surface,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
    )
    .compile()
    .unwrap();
    let other_admitted = other
        .admit_bindings(&AppearanceBindings::new(
            vec![(other_source, Srgb8::new([1, 2, 3]))],
            vec![(other_context, Srgb8::new([4, 5, 6]))],
            vec![(other_opacity, 0.5)],
        ))
        .unwrap();
    let mut workspace = graph.new_workspace().unwrap();
    let storage = workspace.storage_signature();

    assert_eq!(
        graph
            .evaluate_admitted_into(&other_admitted, &mut workspace)
            .unwrap_err(),
        BindingError::IncompatibleAdmittedBindings
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
    assert_eq!(workspace.storage_signature(), storage);
}

#[test]
fn invalid_opacity_is_rejected_during_admission_before_any_composition() {
    let graph = point_component(false, false).compile().unwrap();
    crate::composition::reset_source_over_evaluation_count();
    assert_eq!(
        graph.admit_bindings(&bindings([1, 2, 3], f64::NAN, [4, 5, 6])),
        Err(BindingError::OpacityOutOfDomain {
            input: OPACITY,
            reason: crate::composition::OpacityAdmissionErrorV1::NonFinite,
        })
    );
    assert_eq!(crate::composition::source_over_evaluation_count(), 0);
}
