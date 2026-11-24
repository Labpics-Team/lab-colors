use super::*;
use crate::composition::AdmittedOpacityV1;
use std::cell::Cell;

fn color(channels: [f64; 3]) -> ContinuousSrgbV1 {
    ContinuousSrgbV1::new(channels).unwrap()
}

fn lift(value: EncodedPointPaintValueV1) -> ContinuousPointPaintValueV1 {
    ContinuousPointPaintValueV1::from_admitted(
        ContinuousSrgbV1::from_srgb8(value.source()),
        value.opacity(),
    )
}

fn nested_graph() -> (CompiledAppearanceGraph, AdmittedAppearanceBindings) {
    let graph = AppearanceGraphSpec::new(
        (0..3).map(PaintInputId::new).collect(),
        (0..2).map(SurfaceInputPortId::new).collect(),
        vec![],
        (0..3)
            .map(|id| PaintSpec::Input {
                id: PaintId::new(id),
                input: PaintInputId::new(id),
            })
            .collect(),
        vec![
            SurfaceSpec::Input {
                id: SurfaceId::new(0),
                port: SurfaceInputPortId::new(0),
            },
            SurfaceSpec::FromOccurrence {
                id: SurfaceId::new(1),
                occurrence: OccurrenceId::new(0),
            },
            SurfaceSpec::Input {
                id: SurfaceId::new(2),
                port: SurfaceInputPortId::new(1),
            },
        ],
        (0..3)
            .map(|id| OccurrenceSpec {
                id: OccurrenceId::new(id),
                subject: PaintId::new(id),
                against: SurfaceId::new(id),
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            })
            .collect(),
    )
    .compile()
    .unwrap();
    let bindings = graph
        .admit_bindings(&AppearanceBindings::new(
            [(244, 0.85), (1, 0.5), (33, 0.25)]
                .into_iter()
                .enumerate()
                .map(|(index, (source, alpha))| {
                    (
                        PaintInputId::new(index as u32),
                        EncodedPointPaintValueV1::from_admitted(
                            Srgb8::new([source; 3]),
                            AdmittedOpacityV1::new(alpha).unwrap(),
                        ),
                    )
                })
                .collect(),
            vec![
                (SurfaceInputPortId::new(0), Srgb8::new([255; 3])),
                (SurfaceInputPortId::new(1), Srgb8::new([0; 3])),
            ],
            vec![],
        ))
        .unwrap();
    (graph, bindings)
}

#[test]
fn nested_occurrences_preserve_fractional_composites_until_byte_realization() {
    let (graph, bindings) = nested_graph();
    let final_slot = graph.bind_occurrence(OccurrenceId::new(1)).unwrap();
    let mut workspace = graph.new_continuous_workspace().unwrap();
    let mut prepared = graph
        .prepare_continuous_surface_cases(
            None,
            &bindings,
            |index| lift(bindings.paint_inputs[index].1),
            &mut workspace,
        )
        .unwrap();
    let view = prepared.evaluate_surface_inputs_canonical(|index| {
        ContinuousSrgbV1::from_srgb8(bindings.surfaces[index].1)
    });
    let final_point = view.occurrence_at(final_slot).unwrap();
    // Independent decimal arithmetic: (244*85+255*15)/100=245.65;
    // (245.65+1)/2=123.325. The byte graph instead yields 246, then 124.
    assert_eq!(final_point.backdrop().channels(), [245.65; 3]);
    assert_eq!(final_point.visible().channels(), [123.325; 3]);
    let mut byte_workspace = graph.new_workspace().unwrap();
    let byte = graph
        .evaluate_admitted_into(&bindings, &mut byte_workspace)
        .unwrap();
    assert_eq!(byte.occurrence_at(final_slot).unwrap().visible(), [124; 3]);
}

#[test]
fn continuous_sources_and_roots_are_not_replaced_with_admitted_byte_bindings() {
    let (graph, bindings) = nested_graph();
    let first = graph.bind_occurrence(OccurrenceId::new(0)).unwrap();
    let mut workspace = graph.new_continuous_workspace().unwrap();
    let mut prepared = graph
        .prepare_continuous_surface_cases(
            None,
            &bindings,
            |_| {
                ContinuousPointPaintValueV1::from_admitted(
                    color([12.25; 3]),
                    AdmittedOpacityV1::new(0.5).unwrap(),
                )
            },
            &mut workspace,
        )
        .unwrap();
    let view = prepared.evaluate_surface_inputs_canonical(|_| color([35.75; 3]));
    let actual = view.occurrence_at(first).unwrap();
    assert_eq!(actual.backdrop().channels(), [35.75; 3]);
    assert_eq!(actual.visible().channels(), [24.0; 3]);
}

#[test]
fn scoped_cases_share_continuous_results_without_evaluating_unrelated_nodes() {
    let (graph, bindings) = nested_graph();
    let final_slot = graph.bind_occurrence(OccurrenceId::new(1)).unwrap();
    let unrelated = graph.bind_occurrence(OccurrenceId::new(2)).unwrap();
    let mut compiler = AppearanceConeCompilerV1::new(&graph).unwrap();
    compiler.occurrence(OccurrenceId::new(1)).unwrap();
    let cone = compiler.finish().unwrap();
    let mut full_workspace = graph.new_continuous_workspace().unwrap();
    let mut scoped_workspace = graph.new_continuous_workspace().unwrap();
    let storage = scoped_workspace.storage_signature();
    // Leave all slots populated before entering the scoped view. Untouched
    // values must remain inaccessible even when their old data is still there.
    graph
        .prepare_continuous_surface_cases(
            None,
            &bindings,
            |index| lift(bindings.paint_inputs[index].1),
            &mut scoped_workspace,
        )
        .unwrap()
        .evaluate_surface_inputs_canonical(|_| color([255.0; 3]));
    let source_reads = Cell::new(0);
    let surface_reads = Cell::new(0);
    {
        let mut full = graph
            .prepare_continuous_surface_cases(
                None,
                &bindings,
                |index| lift(bindings.paint_inputs[index].1),
                &mut full_workspace,
            )
            .unwrap();
        reset_paint_node_evaluation_count();
        let mut scoped = graph
            .prepare_continuous_surface_cases(
                Some(&cone),
                &bindings,
                |index| {
                    assert!(index < 2, "unrelated source must stay outside the cone");
                    source_reads.set(source_reads.get() + 1);
                    lift(bindings.paint_inputs[index].1)
                },
                &mut scoped_workspace,
            )
            .unwrap();
        assert_eq!(paint_node_evaluation_count(), 2);
        for root in [0.0, 127.25, 255.0] {
            let expected = *full
                .evaluate_surface_inputs_canonical(|_| color([root; 3]))
                .occurrence_at(final_slot)
                .unwrap();
            let view = scoped.evaluate_surface_inputs_canonical(|index| {
                assert_eq!(index, 0, "unrelated root must stay outside the cone");
                surface_reads.set(surface_reads.get() + 1);
                color([root; 3])
            });
            assert_eq!(*view.occurrence_at(final_slot).unwrap(), expected);
            assert!(view.occurrence_at(unrelated).is_none());
        }
        assert_eq!(paint_node_evaluation_count(), 2);
    }
    assert_eq!(source_reads.get(), 2);
    assert_eq!(surface_reads.get(), 3);
    assert_eq!(scoped_workspace.storage_signature(), storage);
}

#[test]
fn both_kernels_preserve_the_compiled_opacity_multiplication_order() {
    // Existing byte witness crosses a rounding seam when reassociated. Check
    // exact opacity bits before composition in both numeric interpretations.
    let factors = [
        f64::from_bits(0x3fc7_1b2a_949a_2779),
        f64::from_bits(0x3fc7_1b2a_949a_276d),
        f64::from_bits(0x3fc7_1b2a_949a_2768),
    ];
    let mut opacities = Vec::new();
    for order in [[0, 1, 2], [1, 2, 0]] {
        let mut paints = vec![PaintSpec::Input {
            id: PaintId::new(0),
            input: PaintInputId::new(0),
        }];
        paints.extend(
            order
                .into_iter()
                .enumerate()
                .map(|(index, input)| PaintSpec::Opacity {
                    id: PaintId::new(index as u32 + 1),
                    source: PaintId::new(index as u32),
                    opacity: OpacityInputId::new(input),
                }),
        );
        let graph = AppearanceGraphSpec::new(
            vec![PaintInputId::new(0)],
            vec![SurfaceInputPortId::new(0)],
            (0..3).map(OpacityInputId::new).collect(),
            paints,
            vec![SurfaceSpec::Input {
                id: SurfaceId::new(0),
                port: SurfaceInputPortId::new(0),
            }],
            vec![OccurrenceSpec {
                id: OccurrenceId::new(0),
                subject: PaintId::new(3),
                against: SurfaceId::new(0),
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            }],
        )
        .compile()
        .unwrap();
        let bindings = graph
            .admit_bindings(&AppearanceBindings::new(
                vec![(
                    PaintInputId::new(0),
                    EncodedPointPaintValueV1::opaque(Srgb8::new([255; 3])),
                )],
                vec![(SurfaceInputPortId::new(0), Srgb8::new([0; 3]))],
                factors
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| (OpacityInputId::new(index as u32), value))
                    .collect(),
            ))
            .unwrap();
        let mut continuous = graph.new_continuous_workspace().unwrap();
        let mut byte = graph.new_workspace().unwrap();
        graph.evaluate_admitted_into(&bindings, &mut byte).unwrap();
        graph
            .prepare_continuous_surface_cases(
                None,
                &bindings,
                |index| lift(bindings.paint_inputs[index].1),
                &mut continuous,
            )
            .unwrap();
        let expected = order
            .into_iter()
            .fold(1.0, |alpha, index| alpha * factors[index as usize]);
        for index in 0..byte.paints.len() {
            assert_eq!(
                continuous.paints[index].unwrap().opacity_bits(),
                byte.paints[index].unwrap().opacity_bits()
            );
        }
        let actual = continuous.paints[3].unwrap().opacity_bits();
        assert_eq!(actual, expected.to_bits());
        opacities.push(actual);
    }
    assert_ne!(opacities[0], opacities[1]);
}

#[test]
fn continuous_admission_rejects_nonfinite_and_out_of_range_without_clamping() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            ContinuousSrgbV1::new([0.0, value, 0.0]),
            Err(ContinuousColorAdmissionErrorV1::NonFinite { channel: 1 })
        );
    }
    for value in [-f64::from_bits(1), f64::from_bits(255.0_f64.to_bits() + 1)] {
        assert_eq!(
            ContinuousSrgbV1::new([value, 0.0, 0.0]),
            Err(ContinuousColorAdmissionErrorV1::OutsideEncodedSrgb { channel: 0 })
        );
    }
    assert_eq!(
        color([-0.0, f64::from_bits(1), 255.0])
            .channels()
            .map(f64::to_bits),
        [0, 1, 255.0_f64.to_bits()]
    );
}

#[test]
fn continuous_endpoints_preserve_subnormal_source_and_backdrop() {
    let (graph, bindings) = nested_graph();
    let first = graph.bind_occurrence(OccurrenceId::new(0)).unwrap();
    let tiny = color([f64::from_bits(1); 3]);
    let mut workspace = graph.new_continuous_workspace().unwrap();
    for (alpha, source, backdrop, expected) in [
        (1.0, tiny, color([255.0; 3]), tiny),
        (0.0, color([255.0; 3]), tiny, tiny),
    ] {
        let mut paints = graph
            .prepare_continuous_surface_cases(
                None,
                &bindings,
                |_| {
                    ContinuousPointPaintValueV1::from_admitted(
                        source,
                        AdmittedOpacityV1::new(alpha).unwrap(),
                    )
                },
                &mut workspace,
            )
            .unwrap();
        assert_eq!(
            paints
                .evaluate_surface_inputs_canonical(|_| backdrop)
                .occurrence_at(first)
                .unwrap()
                .visible(),
            expected
        );
    }
}

#[test]
fn continuous_preparation_rejects_foreign_cone_schema_and_shape_before_reading_sources() {
    let (graph, bindings) = nested_graph();
    let (foreign_graph, _) = nested_graph();
    let mut compiler = AppearanceConeCompilerV1::new(&foreign_graph).unwrap();
    compiler.occurrence(OccurrenceId::new(1)).unwrap();
    let foreign_cone = compiler.finish().unwrap();
    let mut workspace = graph.new_continuous_workspace().unwrap();
    let storage = workspace.storage_signature();
    assert!(matches!(
        graph.prepare_continuous_surface_cases(
            Some(&foreign_cone),
            &bindings,
            |_| panic!("invalid cone"),
            &mut workspace,
        ),
        Err(BindingError::IncompatibleEvaluationCone)
    ));
    let different_graph = point_opacity_over_surface_declarative_spec()
        .compile()
        .unwrap();
    let mut wrong_shape = different_graph.new_continuous_workspace().unwrap();
    assert!(matches!(
        graph.prepare_continuous_surface_cases(
            None,
            &bindings,
            |_| panic!("invalid workspace"),
            &mut wrong_shape,
        ),
        Err(BindingError::IncompatibleWorkspace)
    ));
    let wrong_bindings = different_graph
        .admit_bindings(&AppearanceBindings::new(
            vec![(
                PaintInputId::new(0),
                EncodedPointPaintValueV1::opaque(Srgb8::new([0; 3])),
            )],
            vec![(SurfaceInputPortId::new(0), Srgb8::new([255; 3]))],
            vec![(OpacityInputId::new(0), 0.5)],
        ))
        .unwrap();
    assert!(matches!(
        graph.prepare_continuous_surface_cases(
            None,
            &wrong_bindings,
            |_| panic!("invalid binding schema"),
            &mut workspace,
        ),
        Err(BindingError::IncompatibleAdmittedBindings)
    ));
    assert_eq!(workspace.storage_signature(), storage);
}
