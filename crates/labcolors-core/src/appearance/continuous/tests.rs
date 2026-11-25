use super::*;
use crate::composition::AdmittedOpacityV1;
use std::cell::Cell;

fn color(channels: [f64; 3]) -> ContinuousSrgbV1 {
    ContinuousSrgbV1::new(channels).expect("test channels are finite and in encoded sRGB")
}

fn lift(value: EncodedPointPaintValueV1) -> ContinuousPointPaintValueV1 {
    ContinuousPointPaintValueV1::from_admitted(
        ContinuousSrgbV1::from_srgb8(value.source()),
        value.opacity(),
    )
}

fn nested_graph() -> Result<(CompiledAppearanceGraph, AdmittedAppearanceBindings), BindingError> {
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
    .expect("fixture declares an acyclic appearance graph");
    let bindings = graph.admit_bindings(&AppearanceBindings::new(
        [(244, 0.85), (1, 0.5), (33, 0.25)]
            .into_iter()
            .enumerate()
            .map(|(index, (source, alpha))| {
                (
                    PaintInputId::new(index as u32),
                    EncodedPointPaintValueV1::from_admitted(
                        Srgb8::new([source; 3]),
                        AdmittedOpacityV1::new(alpha)
                            .expect("declared opacity is in the unit interval"),
                    ),
                )
            })
            .collect(),
        vec![
            (SurfaceInputPortId::new(0), Srgb8::new([255; 3])),
            (SurfaceInputPortId::new(1), Srgb8::new([0; 3])),
        ],
        vec![],
    ))?;
    Ok((graph, bindings))
}

#[test]
fn nested_occurrences_preserve_fractional_composites_until_byte_realization()
-> Result<(), BindingError> {
    let (graph, bindings) = nested_graph()?;
    let final_slot = graph
        .bind_occurrence(OccurrenceId::new(1))
        .expect("nested graph contains the final occurrence");
    let mut workspace = graph.new_continuous_workspace()?;
    let mut prepared = graph.prepare_continuous_surface_cases(
        None,
        &bindings,
        |index| lift(bindings.paint_inputs[index].1),
        &mut workspace,
    )?;
    let view = prepared.evaluate_surface_inputs_canonical(|index| {
        ContinuousSrgbV1::from_srgb8(bindings.surfaces[index].1)
    });
    let final_point = view
        .occurrence_at(final_slot)
        .expect("the selected occurrence was evaluated");
    // Independent decimal arithmetic: (244*85+255*15)/100=245.65;
    // (245.65+1)/2=123.325. The byte graph instead yields 246, then 124.
    assert_eq!(final_point.backdrop().channels(), [245.65; 3]);
    assert_eq!(final_point.visible().channels(), [123.325; 3]);
    let mut byte_workspace = graph.new_workspace()?;
    let byte = graph.evaluate_admitted_into(&bindings, &mut byte_workspace)?;
    assert_eq!(
        byte.occurrence_at(final_slot)
            .expect("byte evaluation includes the same final occurrence")
            .visible(),
        [124; 3]
    );
    Ok(())
}

#[test]
fn continuous_sources_and_roots_are_not_replaced_with_admitted_byte_bindings()
-> Result<(), BindingError> {
    let (graph, bindings) = nested_graph()?;
    let first = graph
        .bind_occurrence(OccurrenceId::new(0))
        .expect("nested graph contains its first occurrence");
    let mut workspace = graph.new_continuous_workspace()?;
    let mut prepared = graph.prepare_continuous_surface_cases(
        None,
        &bindings,
        |_| {
            ContinuousPointPaintValueV1::from_admitted(
                color([12.25; 3]),
                AdmittedOpacityV1::new(0.5).expect("declared opacity is in the unit interval"),
            )
        },
        &mut workspace,
    )?;
    let view = prepared.evaluate_surface_inputs_canonical(|_| color([35.75; 3]));
    let actual = view
        .occurrence_at(first)
        .expect("the first occurrence was evaluated");
    assert_eq!(actual.backdrop().channels(), [35.75; 3]);
    assert_eq!(actual.visible().channels(), [24.0; 3]);
    Ok(())
}

#[test]
fn scoped_cases_share_continuous_results_without_evaluating_unrelated_nodes()
-> Result<(), BindingError> {
    let (graph, bindings) = nested_graph()?;
    let final_slot = graph
        .bind_occurrence(OccurrenceId::new(1))
        .expect("nested graph contains the final occurrence");
    let unrelated = graph
        .bind_occurrence(OccurrenceId::new(2))
        .expect("nested graph contains the unrelated occurrence");
    let mut compiler = AppearanceConeCompilerV1::new(&graph)?;
    compiler.occurrence(OccurrenceId::new(1))?;
    let cone = compiler.finish()?;
    let mut full_workspace = graph.new_continuous_workspace()?;
    let mut scoped_workspace = graph.new_continuous_workspace()?;
    let storage = scoped_workspace.storage_signature();
    // Leave all slots populated before entering the scoped view. Untouched
    // values must remain inaccessible even when their old data is still there.
    graph
        .prepare_continuous_surface_cases(
            None,
            &bindings,
            |index| lift(bindings.paint_inputs[index].1),
            &mut scoped_workspace,
        )?
        .evaluate_surface_inputs_canonical(|_| color([255.0; 3]));
    let source_reads = Cell::new(0);
    let surface_reads = Cell::new(0);
    {
        let mut full = graph.prepare_continuous_surface_cases(
            None,
            &bindings,
            |index| lift(bindings.paint_inputs[index].1),
            &mut full_workspace,
        )?;
        reset_paint_node_evaluation_count();
        let mut scoped = graph.prepare_continuous_surface_cases(
            Some(&cone),
            &bindings,
            |index| {
                assert!(index < 2, "unrelated source must stay outside the cone");
                source_reads.set(source_reads.get() + 1);
                lift(bindings.paint_inputs[index].1)
            },
            &mut scoped_workspace,
        )?;
        assert_eq!(paint_node_evaluation_count(), 2);
        for root in [0.0, 127.25, 255.0] {
            let expected = *full
                .evaluate_surface_inputs_canonical(|_| color([root; 3]))
                .occurrence_at(final_slot)
                .expect("the selected occurrence was evaluated");
            let view = scoped.evaluate_surface_inputs_canonical(|index| {
                assert_eq!(index, 0, "unrelated root must stay outside the cone");
                surface_reads.set(surface_reads.get() + 1);
                color([root; 3])
            });
            assert_eq!(
                *view
                    .occurrence_at(final_slot)
                    .expect("the selected occurrence was evaluated"),
                expected
            );
            assert!(view.occurrence_at(unrelated).is_none());
        }
        assert_eq!(paint_node_evaluation_count(), 2);
    }
    assert_eq!(source_reads.get(), 2);
    assert_eq!(surface_reads.get(), 3);
    assert_eq!(scoped_workspace.storage_signature(), storage);
    Ok(())
}

#[test]
fn both_kernels_preserve_the_compiled_opacity_multiplication_order() -> Result<(), BindingError> {
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
        .expect("fixture declares an acyclic appearance graph");
        let bindings = graph.admit_bindings(&AppearanceBindings::new(
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
        ))?;
        let mut continuous = graph.new_continuous_workspace()?;
        let mut byte = graph.new_workspace()?;
        graph.evaluate_admitted_into(&bindings, &mut byte)?;
        graph.prepare_continuous_surface_cases(
            None,
            &bindings,
            |index| lift(bindings.paint_inputs[index].1),
            &mut continuous,
        )?;
        let expected = order
            .into_iter()
            .fold(1.0, |alpha, index| alpha * factors[index as usize]);
        for index in 0..byte.paints.len() {
            assert_eq!(
                continuous.paints[index]
                    .expect("continuous workspace contains every prepared paint")
                    .opacity_bits(),
                byte.paints[index]
                    .expect("byte workspace contains every prepared paint")
                    .opacity_bits()
            );
        }
        let actual = continuous.paints[3]
            .expect("the final paint includes all three opacity factors")
            .opacity_bits();
        assert_eq!(actual, expected.to_bits());
        opacities.push(actual);
    }
    assert_ne!(opacities[0], opacities[1]);
    Ok(())
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
fn continuous_endpoints_preserve_subnormal_source_and_backdrop() -> Result<(), BindingError> {
    let (graph, bindings) = nested_graph()?;
    let first = graph
        .bind_occurrence(OccurrenceId::new(0))
        .expect("nested graph contains its first occurrence");
    let tiny = color([f64::from_bits(1); 3]);
    let mut workspace = graph.new_continuous_workspace()?;
    for (alpha, source, backdrop, expected) in [
        (1.0, tiny, color([255.0; 3]), tiny),
        (0.0, color([255.0; 3]), tiny, tiny),
    ] {
        let mut paints = graph.prepare_continuous_surface_cases(
            None,
            &bindings,
            |_| {
                ContinuousPointPaintValueV1::from_admitted(
                    source,
                    AdmittedOpacityV1::new(alpha)
                        .expect("declared opacity is in the unit interval"),
                )
            },
            &mut workspace,
        )?;
        assert_eq!(
            paints
                .evaluate_surface_inputs_canonical(|_| backdrop)
                .occurrence_at(first)
                .expect("the first occurrence was evaluated")
                .visible(),
            expected
        );
    }
    Ok(())
}

#[test]
fn continuous_preparation_rejects_foreign_cone_schema_and_shape_before_reading_sources()
-> Result<(), BindingError> {
    let (graph, bindings) = nested_graph()?;
    let (foreign_graph, compatible_bindings) = nested_graph()?;
    let mut compiler = AppearanceConeCompilerV1::new(&foreign_graph)?;
    compiler.occurrence(OccurrenceId::new(1))?;
    let foreign_cone = compiler.finish()?;
    let mut workspace = graph.new_continuous_workspace()?;
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
    graph.prepare_continuous_surface_cases(
        None,
        &compatible_bindings,
        |index| lift(compatible_bindings.paint_inputs[index].1),
        &mut workspace,
    )?;
    let different_graph = point_opacity_over_surface_declarative_spec()
        .compile()
        .expect("fixture declares an acyclic appearance graph");
    let mut wrong_shape = different_graph.new_continuous_workspace()?;
    assert!(matches!(
        graph.prepare_continuous_surface_cases(
            None,
            &bindings,
            |_| panic!("invalid workspace"),
            &mut wrong_shape,
        ),
        Err(BindingError::IncompatibleWorkspace)
    ));
    let wrong_bindings = different_graph.admit_bindings(&AppearanceBindings::new(
        vec![(
            PaintInputId::new(0),
            EncodedPointPaintValueV1::opaque(Srgb8::new([0; 3])),
        )],
        vec![(SurfaceInputPortId::new(0), Srgb8::new([255; 3]))],
        vec![(OpacityInputId::new(0), 0.5)],
    ))?;
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
    Ok(())
}
