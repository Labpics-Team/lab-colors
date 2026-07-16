//! Contract tests for the private physical appearance graph (#307).
//!
//! The graph owns render topology, not client vocabulary. These tests therefore
//! use typed opaque handles and final sRGB8 values only. The warning example is
//! a finite counterexample discovered in the existing Lab UI fixture: it proves
//! that observing a foreground against the page instead of the rendered tinted
//! surface changes the legacy conformance result. Its numbers are witness data,
//! never production policy.

use proptest::prelude::*;

use crate::appearance::{
    AppearanceBindings, AppearanceGraphSpec, ColorInputId, CompositionProfileV1,
    ForegroundOccurrenceSpec, GraphError, OccurrenceId, OpacityInputId, ResolvedOccurrence,
    SourceOverCertificateV1, SurfaceId, SurfaceSpec,
};
use crate::solve::Floor;

const SOURCE: ColorInputId = ColorInputId::new(0);
const CONTEXT: ColorInputId = ColorInputId::new(1);
const OPACITY: OpacityInputId = OpacityInputId::new(0);
const CONTEXT_SURFACE: SurfaceId = SurfaceId::new(0);
const DERIVED_SURFACE: SurfaceId = SurfaceId::new(1);
const FOREGROUND: OccurrenceId = OccurrenceId::new(0);

#[test]
fn occurrence_contract_contains_only_physical_facts() {
    let graph = AppearanceGraphSpec::new(
        vec![SOURCE, CONTEXT],
        vec![],
        vec![SurfaceSpec::Input {
            id: CONTEXT_SURFACE,
            color: CONTEXT,
        }],
        vec![ForegroundOccurrenceSpec {
            id: FOREGROUND,
            identity_source: SOURCE,
            against: CONTEXT_SURFACE,
        }],
    )
    .compile()
    .unwrap();

    let rendered = graph
        .evaluate(&AppearanceBindings::new(
            vec![(SOURCE, [1, 2, 3]), (CONTEXT, [4, 5, 6])],
            vec![],
        ))
        .unwrap();

    let ResolvedOccurrence {
        id,
        identity_source,
        source,
        against,
        backdrop,
    } = *rendered.occurrence(FOREGROUND).unwrap();

    assert_eq!(
        (id, identity_source, source, against, backdrop),
        (FOREGROUND, SOURCE, [1, 2, 3], CONTEXT_SURFACE, [4, 5, 6],)
    );
}

fn atomic_component(surface_declarations_reversed: bool) -> AppearanceGraphSpec {
    let context = SurfaceSpec::Input {
        id: CONTEXT_SURFACE,
        color: CONTEXT,
    };
    let derived = SurfaceSpec::SourceOver {
        id: DERIVED_SURFACE,
        source: SOURCE,
        opacity: OPACITY,
        backdrop: CONTEXT_SURFACE,
        profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
    };
    let surfaces = if surface_declarations_reversed {
        vec![derived, context]
    } else {
        vec![context, derived]
    };

    AppearanceGraphSpec::new(
        vec![SOURCE, CONTEXT],
        vec![OPACITY],
        surfaces,
        vec![ForegroundOccurrenceSpec {
            id: FOREGROUND,
            identity_source: SOURCE,
            against: DERIVED_SURFACE,
        }],
    )
}

fn bindings(source: [u8; 3], opacity: f64, context: [u8; 3]) -> AppearanceBindings {
    AppearanceBindings::new(
        vec![(SOURCE, source), (CONTEXT, context)],
        vec![(OPACITY, opacity)],
    )
}

#[test]
fn warning_occurrence_targets_the_rendered_surface_not_the_page() {
    let graph = atomic_component(false).compile().unwrap();
    let rendered = graph
        .evaluate(&bindings([0xFF, 0xA1, 0x00], 0.122, [0xFF; 3]))
        .unwrap();

    assert_eq!(
        rendered.surface_rgb(DERIVED_SURFACE),
        Some([0xFF, 0xF4, 0xE0])
    );
    let occurrence = rendered.occurrence(FOREGROUND).unwrap();
    assert_eq!(occurrence.against, DERIVED_SURFACE);
    assert_eq!(occurrence.backdrop, [0xFF, 0xF4, 0xE0]);
    assert_ne!(occurrence.backdrop, [0xFF; 3]);

    // Non-vacuity: the known page-resolved warning foreground clears the legacy
    // UI floor on the page but not on the surface it is actually painted over.
    let foreground = [0xD2, 0x83, 0x00].map(|channel| f64::from(channel) / 255.0);
    let page = [1.0; 3];
    let surface = occurrence
        .backdrop
        .map(|channel| f64::from(channel) / 255.0);
    let legacy_ui_floor = Floor::AaUi.min_ratio().unwrap();
    assert!(crate::wcag::contrast_ratio(foreground, page) >= legacy_ui_floor);
    assert!(crate::wcag::contrast_ratio(foreground, surface) < legacy_ui_floor);

    assert_eq!(rendered.trace().input_surfaces, 1);
    assert_eq!(rendered.trace().source_over_edges, 1);
    assert_eq!(rendered.trace().foreground_occurrences, 1);
}

#[test]
fn compile_is_independent_of_declaration_order_for_the_same_handles() {
    let canonical = atomic_component(false).compile().unwrap();
    let reordered = atomic_component(true).compile().unwrap();
    assert_eq!(canonical, reordered);

    let values = bindings([19, 127, 241], 0.375, [247, 241, 233]);
    assert_eq!(canonical.evaluate(&values), reordered.evaluate(&values));
}

#[test]
fn unrelated_opaque_handles_do_not_change_the_physics() {
    let other_source = ColorInputId::new(700);
    let other_context = ColorInputId::new(42);
    let other_opacity = OpacityInputId::new(91);
    let other_context_surface = SurfaceId::new(800);
    let other_derived_surface = SurfaceId::new(12);
    let other_occurrence = OccurrenceId::new(501);
    let other = AppearanceGraphSpec::new(
        vec![other_context, other_source],
        vec![other_opacity],
        vec![
            SurfaceSpec::SourceOver {
                id: other_derived_surface,
                source: other_source,
                opacity: other_opacity,
                backdrop: other_context_surface,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
            SurfaceSpec::Input {
                id: other_context_surface,
                color: other_context,
            },
        ],
        vec![ForegroundOccurrenceSpec {
            id: other_occurrence,
            identity_source: other_source,
            against: other_derived_surface,
        }],
    )
    .compile()
    .unwrap();

    let source = [13, 89, 233];
    let context = [249, 245, 237];
    let opacity = 0.41;
    let first = atomic_component(false)
        .compile()
        .unwrap()
        .evaluate(&bindings(source, opacity, context))
        .unwrap();
    let second = other
        .evaluate(&AppearanceBindings::new(
            vec![(other_source, source), (other_context, context)],
            vec![(other_opacity, opacity)],
        ))
        .unwrap();

    assert_eq!(
        first.surface_rgb(DERIVED_SURFACE),
        second.surface_rgb(other_derived_surface)
    );
    assert_eq!(
        first.occurrence(FOREGROUND).unwrap().backdrop,
        second.occurrence(other_occurrence).unwrap().backdrop
    );
}

#[test]
fn graph_rejects_missing_occurrence_backdrop_and_cycles() {
    let missing = AppearanceGraphSpec::new(
        vec![SOURCE],
        vec![],
        vec![],
        vec![ForegroundOccurrenceSpec {
            id: FOREGROUND,
            identity_source: SOURCE,
            against: DERIVED_SURFACE,
        }],
    )
    .compile();
    assert_eq!(
        missing,
        Err(GraphError::MissingOccurrenceBackdrop {
            occurrence: FOREGROUND,
            surface: DERIVED_SURFACE,
        })
    );

    let cyclic = AppearanceGraphSpec::new(
        vec![SOURCE],
        vec![OPACITY],
        vec![
            SurfaceSpec::SourceOver {
                id: CONTEXT_SURFACE,
                source: SOURCE,
                opacity: OPACITY,
                backdrop: DERIVED_SURFACE,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
            SurfaceSpec::SourceOver {
                id: DERIVED_SURFACE,
                source: SOURCE,
                opacity: OPACITY,
                backdrop: CONTEXT_SURFACE,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
        ],
        vec![],
    )
    .compile();
    assert_eq!(
        cyclic,
        Err(GraphError::SurfaceCycle {
            surfaces: vec![CONTEXT_SURFACE, DERIVED_SURFACE],
        })
    );
}

proptest! {
    #[test]
    fn graph_source_over_equals_the_independent_compositor_for_neutral_and_chromatic_inputs(
        source in any::<[u8; 3]>(),
        context in any::<[u8; 3]>(),
        opacity in 0.0f64..=1.0f64,
    ) {
        let graph = atomic_component(false).compile().unwrap();
        let rendered = graph.evaluate(&bindings(source, opacity, context)).unwrap();
        let oracle = crate::alpha::composite_over_srgb8(source, opacity, context).unwrap();
        prop_assert_eq!(rendered.surface_rgb(DERIVED_SURFACE), Some(oracle));
        prop_assert_eq!(rendered.occurrence(FOREGROUND).unwrap().backdrop, oracle);
    }

    // Replayable-сертификат: независимое повторение операции из данных самого
    // сертификата даёт те же байты, что записанный выход, — на всём домене.
    #[test]
    fn source_over_certificate_replays_to_the_exact_recorded_bytes(
        source in any::<[u8; 3]>(),
        context in any::<[u8; 3]>(),
        opacity in 0.0f64..=1.0f64,
    ) {
        let graph = atomic_component(false).compile().unwrap();
        let rendered = graph.evaluate(&bindings(source, opacity, context)).unwrap();
        let certificates = rendered.certificates();
        prop_assert_eq!(certificates.len(), 1);
        let certificate = &certificates[0];
        let SourceOverCertificateV1 {
            profile,
            surface,
            source_input,
            source_rgb,
            backdrop_surface,
            backdrop_rgb,
            opacity_input,
            opacity_bits,
            output_rgb,
        } = certificate;
        prop_assert_eq!(*profile, CompositionProfileV1::EncodedSrgb8SourceOverV1);
        prop_assert_eq!(*surface, DERIVED_SURFACE);
        prop_assert_eq!(*source_input, SOURCE);
        prop_assert_eq!(*source_rgb, source);
        prop_assert_eq!(*backdrop_surface, CONTEXT_SURFACE);
        prop_assert_eq!(*backdrop_rgb, context);
        prop_assert_eq!(*opacity_input, OPACITY);
        prop_assert_eq!(*opacity_bits, opacity.to_bits());
        prop_assert_eq!(*output_rgb, rendered.surface_rgb(DERIVED_SURFACE).unwrap());
        prop_assert_eq!(certificate.replay(), Ok(*output_rgb));
    }
}

// ── Fail-closed контракт компиляции (§6.1 ТЗ #307) ────────────────────────────

#[test]
fn compile_rejects_duplicate_declarations_with_typed_errors() {
    let duplicate_color =
        AppearanceGraphSpec::new(vec![SOURCE, CONTEXT, SOURCE], vec![OPACITY], vec![], vec![])
            .compile();
    assert_eq!(
        duplicate_color,
        Err(GraphError::DuplicateColorInput { input: SOURCE })
    );

    let duplicate_opacity =
        AppearanceGraphSpec::new(vec![SOURCE], vec![OPACITY, OPACITY], vec![], vec![]).compile();
    assert_eq!(
        duplicate_opacity,
        Err(GraphError::DuplicateOpacityInput { input: OPACITY })
    );

    let duplicate_surface = AppearanceGraphSpec::new(
        vec![SOURCE, CONTEXT],
        vec![],
        vec![
            SurfaceSpec::Input {
                id: CONTEXT_SURFACE,
                color: CONTEXT,
            },
            SurfaceSpec::Input {
                id: CONTEXT_SURFACE,
                color: SOURCE,
            },
        ],
        vec![],
    )
    .compile();
    assert_eq!(
        duplicate_surface,
        Err(GraphError::DuplicateSurface {
            surface: CONTEXT_SURFACE,
        })
    );

    let duplicate_occurrence = AppearanceGraphSpec::new(
        vec![SOURCE, CONTEXT],
        vec![],
        vec![SurfaceSpec::Input {
            id: CONTEXT_SURFACE,
            color: CONTEXT,
        }],
        vec![
            ForegroundOccurrenceSpec {
                id: FOREGROUND,
                identity_source: SOURCE,
                against: CONTEXT_SURFACE,
            },
            ForegroundOccurrenceSpec {
                id: FOREGROUND,
                identity_source: CONTEXT,
                against: CONTEXT_SURFACE,
            },
        ],
    )
    .compile();
    assert_eq!(
        duplicate_occurrence,
        Err(GraphError::DuplicateOccurrence {
            occurrence: FOREGROUND,
        })
    );
}

#[test]
fn compile_rejects_every_missing_reference_with_typed_errors() {
    let missing_input_color = AppearanceGraphSpec::new(
        vec![],
        vec![],
        vec![SurfaceSpec::Input {
            id: CONTEXT_SURFACE,
            color: CONTEXT,
        }],
        vec![],
    )
    .compile();
    assert_eq!(
        missing_input_color,
        Err(GraphError::MissingSurfaceColorInput {
            surface: CONTEXT_SURFACE,
            input: CONTEXT,
        })
    );

    let missing_source = AppearanceGraphSpec::new(
        vec![CONTEXT],
        vec![OPACITY],
        vec![
            SurfaceSpec::Input {
                id: CONTEXT_SURFACE,
                color: CONTEXT,
            },
            SurfaceSpec::SourceOver {
                id: DERIVED_SURFACE,
                source: SOURCE,
                opacity: OPACITY,
                backdrop: CONTEXT_SURFACE,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
        ],
        vec![],
    )
    .compile();
    assert_eq!(
        missing_source,
        Err(GraphError::MissingSurfaceColorInput {
            surface: DERIVED_SURFACE,
            input: SOURCE,
        })
    );

    let missing_opacity = AppearanceGraphSpec::new(
        vec![SOURCE, CONTEXT],
        vec![],
        vec![
            SurfaceSpec::Input {
                id: CONTEXT_SURFACE,
                color: CONTEXT,
            },
            SurfaceSpec::SourceOver {
                id: DERIVED_SURFACE,
                source: SOURCE,
                opacity: OPACITY,
                backdrop: CONTEXT_SURFACE,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
        ],
        vec![],
    )
    .compile();
    assert_eq!(
        missing_opacity,
        Err(GraphError::MissingSurfaceOpacityInput {
            surface: DERIVED_SURFACE,
            input: OPACITY,
        })
    );

    let missing_backdrop = AppearanceGraphSpec::new(
        vec![SOURCE],
        vec![OPACITY],
        vec![SurfaceSpec::SourceOver {
            id: DERIVED_SURFACE,
            source: SOURCE,
            opacity: OPACITY,
            backdrop: CONTEXT_SURFACE,
            profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
        }],
        vec![],
    )
    .compile();
    assert_eq!(
        missing_backdrop,
        Err(GraphError::MissingSurfaceBackdrop {
            surface: DERIVED_SURFACE,
            backdrop: CONTEXT_SURFACE,
        })
    );

    let missing_occurrence_source = AppearanceGraphSpec::new(
        vec![CONTEXT],
        vec![],
        vec![SurfaceSpec::Input {
            id: CONTEXT_SURFACE,
            color: CONTEXT,
        }],
        vec![ForegroundOccurrenceSpec {
            id: FOREGROUND,
            identity_source: SOURCE,
            against: CONTEXT_SURFACE,
        }],
    )
    .compile();
    assert_eq!(
        missing_occurrence_source,
        Err(GraphError::MissingOccurrenceSource {
            occurrence: FOREGROUND,
            input: SOURCE,
        })
    );
}

// ── Fail-closed контракт исполнения (§6.2 ТЗ #307) ────────────────────────────

#[test]
fn evaluate_rejects_duplicate_missing_and_unexpected_bindings() {
    let graph = atomic_component(false).compile().unwrap();

    let duplicate_color = graph.evaluate(&AppearanceBindings::new(
        vec![
            (SOURCE, [1, 2, 3]),
            (CONTEXT, [4, 5, 6]),
            (SOURCE, [7, 8, 9]),
        ],
        vec![(OPACITY, 0.5)],
    ));
    assert_eq!(
        duplicate_color,
        Err(GraphError::DuplicateColorBinding { input: SOURCE })
    );

    let duplicate_opacity = graph.evaluate(&AppearanceBindings::new(
        vec![(SOURCE, [1, 2, 3]), (CONTEXT, [4, 5, 6])],
        vec![(OPACITY, 0.5), (OPACITY, 0.6)],
    ));
    assert_eq!(
        duplicate_opacity,
        Err(GraphError::DuplicateOpacityBinding { input: OPACITY })
    );

    let missing_color = graph.evaluate(&AppearanceBindings::new(
        vec![(SOURCE, [1, 2, 3])],
        vec![(OPACITY, 0.5)],
    ));
    assert_eq!(
        missing_color,
        Err(GraphError::MissingColorBinding { input: CONTEXT })
    );

    let missing_opacity = graph.evaluate(&AppearanceBindings::new(
        vec![(SOURCE, [1, 2, 3]), (CONTEXT, [4, 5, 6])],
        vec![],
    ));
    assert_eq!(
        missing_opacity,
        Err(GraphError::MissingOpacityBinding { input: OPACITY })
    );

    let unexpected_color = graph.evaluate(&AppearanceBindings::new(
        vec![
            (SOURCE, [1, 2, 3]),
            (CONTEXT, [4, 5, 6]),
            (ColorInputId::new(9), [7, 8, 9]),
        ],
        vec![(OPACITY, 0.5)],
    ));
    assert_eq!(
        unexpected_color,
        Err(GraphError::UnexpectedColorBinding {
            input: ColorInputId::new(9),
        })
    );

    let unexpected_opacity = graph.evaluate(&AppearanceBindings::new(
        vec![(SOURCE, [1, 2, 3]), (CONTEXT, [4, 5, 6])],
        vec![(OPACITY, 0.5), (OpacityInputId::new(9), 0.5)],
    ));
    assert_eq!(
        unexpected_opacity,
        Err(GraphError::UnexpectedOpacityBinding {
            input: OpacityInputId::new(9),
        })
    );
}

#[test]
fn evaluate_rejects_non_finite_and_out_of_range_alpha_with_the_ssot_domain_text() {
    let graph = atomic_component(false).compile().unwrap();
    for bad_alpha in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.1, 1.5] {
        let outcome = graph.evaluate(&bindings([1, 2, 3], bad_alpha, [4, 5, 6]));
        // Текст доменного отказа — дословно из SSOT-валидатора композитора:
        // потребители переносят его в публичные исходы без переформулировок.
        let expected = crate::alpha::composite_over_srgb8([1, 2, 3], bad_alpha, [4, 5, 6])
            .expect_err("домен α обязан отвергаться и композитором");
        assert_eq!(
            outcome,
            Err(GraphError::OpacityOutOfDomain {
                input: OPACITY,
                message: expected,
            }),
            "α={bad_alpha}: типизированный отказ с SSOT-текстом"
        );
    }
}

// ── Identity-ребро occurrence не декоративно ─────────────────────────────────

/// Occurrence несёт байты именно ОБЪЯВЛЕННОГО identity-источника, а не байты
/// source-входа композита: в этом компоненте они разные входы — подмена ребра
/// идентичности немедленно различима.
#[test]
fn occurrence_source_follows_the_declared_identity_edge_not_the_composite_source() {
    let identity = ColorInputId::new(7);
    let graph = AppearanceGraphSpec::new(
        vec![SOURCE, CONTEXT, identity],
        vec![OPACITY],
        vec![
            SurfaceSpec::Input {
                id: CONTEXT_SURFACE,
                color: CONTEXT,
            },
            SurfaceSpec::SourceOver {
                id: DERIVED_SURFACE,
                source: SOURCE,
                opacity: OPACITY,
                backdrop: CONTEXT_SURFACE,
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            },
        ],
        vec![ForegroundOccurrenceSpec {
            id: FOREGROUND,
            identity_source: identity,
            against: DERIVED_SURFACE,
        }],
    )
    .compile()
    .unwrap();

    let rendered = graph
        .evaluate(&AppearanceBindings::new(
            vec![
                (SOURCE, [10, 20, 30]),
                (CONTEXT, [200, 200, 200]),
                (identity, [111, 112, 113]),
            ],
            vec![(OPACITY, 0.25)],
        ))
        .unwrap();
    let occurrence = rendered.occurrence(FOREGROUND).unwrap();
    assert_eq!(occurrence.identity_source, identity);
    assert_eq!(occurrence.source, [111, 112, 113]);
    assert_ne!(occurrence.source, [10, 20, 30]);
}
