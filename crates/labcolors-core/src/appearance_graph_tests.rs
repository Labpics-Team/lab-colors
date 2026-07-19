//! Контрактные тесты приватного point-render стержня.
//!
//! Граф владеет только физической топологией: Paint материализуется независимо
//! от подложки, Occurrence является его единственным применением к Surface, а
//! `surfaceFrom` лишь даёт видимому результату повторно используемую identity.
//! Словарь Pair/role и perception-утверждения сюда не входят.

use proptest::prelude::*;

use crate::Srgb8;
use crate::appearance::{
    AppearanceBindings, AppearanceGraphSpec, BindingError, ColorInputId, CompileError,
    CompositionProfileV1, OccurrenceId, OccurrenceSpec, OpacityInputId, PaintId, PaintSpec,
    SurfaceId, SurfaceInputPortId, SurfaceSpec,
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
        crate::constraints::ExactSrgb8IdentityV1.identity(),
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
    assert_eq!(solid.rgb(), [0xFF, 0xA1, 0x00]);
    assert_eq!(solid.opacity_bits(), 1.0f64.to_bits());
    let fill = rendered.paint(FILL_PAINT).unwrap();
    assert_eq!(fill.rgb(), [0xFF, 0xA1, 0x00]);
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
        first.paint(FILL_PAINT).unwrap().rgb(),
        second.paint(fill).unwrap().rgb()
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
        let invalid = f64::from_bits(bits);
        prop_assume!(!invalid.is_finite() || !(0.0..=1.0).contains(&invalid));
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
        let expected_message = crate::composition::validate_alpha(invalid).unwrap_err();
        prop_assert_eq!(
            graph.evaluate(&AppearanceBindings::new(
                vec![(SOURCE, Srgb8::new([1, 2, 3]))],
                vec![(CONTEXT, Srgb8::new([4, 5, 6]))],
                vec![(OPACITY, inner), (OTHER_OPACITY, outer)],
            )),
            Err(BindingError::OpacityOutOfDomain {
                input: expected_input,
                message: expected_message,
            })
        );
    }
}

#[test]
fn compile_rejects_duplicate_declarations_with_typed_errors() {
    assert_eq!(
        AppearanceGraphSpec::new(vec![SOURCE, SOURCE], vec![], vec![], vec![], vec![], vec![])
            .compile(),
        Err(CompileError::DuplicateColorInput { input: SOURCE })
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
        .compile(),
        Err(CompileError::DuplicateOpacityInput { input: OPACITY })
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
        .compile(),
        Err(CompileError::DuplicateSurfaceInputPort { input: CONTEXT })
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
        .compile(),
        Err(CompileError::DuplicatePaint { paint: SOLID_PAINT })
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
        .compile(),
        Err(CompileError::DuplicateSurface {
            surface: CONTEXT_SURFACE
        })
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
        .compile(),
        Err(CompileError::DuplicateOccurrence {
            occurrence: FILL_OCCURRENCE
        })
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
        missing_color,
        Err(CompileError::MissingPaintColorInput {
            paint: SOLID_PAINT,
            input: SOURCE,
        })
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
        missing_paint,
        Err(CompileError::MissingPaintSource {
            paint: FILL_PAINT,
            source: SOLID_PAINT,
        })
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
        missing_opacity,
        Err(CompileError::MissingPaintOpacityInput {
            paint: FILL_PAINT,
            input: OPACITY,
        })
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
        missing_surface_input,
        Err(CompileError::MissingSurfaceInputPort {
            surface: CONTEXT_SURFACE,
            input: CONTEXT,
        })
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
        missing_surface_occurrence,
        Err(CompileError::MissingSurfaceOccurrence {
            surface: DERIVED_SURFACE,
            occurrence: FILL_OCCURRENCE,
        })
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
        missing_occurrence_paint,
        Err(CompileError::MissingOccurrencePaint {
            occurrence: FILL_OCCURRENCE,
            paint: FILL_PAINT,
        })
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
        missing_occurrence_backdrop,
        Err(CompileError::MissingOccurrenceBackdrop {
            occurrence: FILL_OCCURRENCE,
            surface: CONTEXT_SURFACE,
        })
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
        paint_cycle,
        Err(CompileError::PaintCycle {
            paints: vec![cycle_a, cycle_b],
        })
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
        render_cycle,
        Err(CompileError::RenderCycle {
            surfaces: vec![cycle_surface],
            occurrences: vec![cycle_occurrence],
        })
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
fn evaluate_rejects_invalid_alpha_with_the_ssot_domain_text() {
    let graph = point_component(false, false).compile().unwrap();
    for bad_alpha in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.5] {
        let expected = crate::alpha::composite_over_srgb8([1, 2, 3], bad_alpha, [4, 5, 6])
            .expect_err("композитор обязан отвергать тот же домен alpha");
        assert_eq!(
            graph.evaluate(&bindings([1, 2, 3], bad_alpha, [4, 5, 6])),
            Err(BindingError::OpacityOutOfDomain {
                input: OPACITY,
                message: expected,
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
