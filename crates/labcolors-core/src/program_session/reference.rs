//! Immutable authored composition, evaluated by the same compositor as runtime.

use super::*;
#[cfg(test)]
use crate::appearance::{ContinuousPointPaintValueV1, ContinuousSrgbV1};

pub(super) struct ReferenceCompositionV1<C = Srgb8> {
    occurrences: Vec<(OccurrenceId, C, C)>,
}

impl<C: Copy> ReferenceCompositionV1<C> {
    pub(super) fn occurrence(&self, id: OccurrenceId) -> Option<(C, C)> {
        self.occurrences
            .binary_search_by_key(&id, |entry| entry.0)
            .ok()
            .map(|index| (self.occurrences[index].1, self.occurrences[index].2))
    }
}

fn validate_inputs<'inputs>(
    reference_inputs: &'inputs mut Option<Vec<(SurfaceInputPortId, Srgb8)>>,
    targets: &[Target],
    graph: &CompiledAppearanceGraph,
) -> Result<Option<&'inputs [(SurfaceInputPortId, Srgb8)]>, ProgramCompileError> {
    let Some(inputs) = reference_inputs else {
        return Ok(None);
    };
    let invalid = || {
        ProgramCompileError::InvalidComputedAssignment(
            computed_assignment::ComputedAdmissionErrorV1::InvalidReferenceComposition,
        )
    };
    inputs.sort_unstable_by_key(|entry| entry.0);
    if !inputs
        .iter()
        .map(|entry| entry.0)
        .eq(graph.surface_input_ports())
    {
        return Err(invalid());
    }
    // Finite declarations have no authored source anchor. Arbitrarily choosing
    // one finite member would silently invent the reference composition.
    if targets
        .iter()
        .any(|target| matches!(target.intent, TargetIntentV1::Finite(_)))
    {
        return Err(invalid());
    }
    Ok(Some(inputs))
}

fn collect<C>(
    declarations: &[Occurrence],
    point_at: impl Fn(OccurrenceId) -> Option<(C, C)>,
) -> Result<ReferenceCompositionV1<C>, ProgramCompileError> {
    let mut occurrences = Vec::new();
    occurrences
        .try_reserve_exact(declarations.len())
        .map_err(|_| ProgramCompileError::ResourceExhausted)?;
    for declaration in declarations {
        let (visible, backdrop) =
            point_at(declaration.id).ok_or(ProgramCompileError::InternalInvariant)?;
        occurrences.push((declaration.id, visible, backdrop));
    }
    occurrences.sort_unstable_by_key(|entry| entry.0);
    Ok(ReferenceCompositionV1 { occurrences })
}

pub(super) fn compile<E: ProgramConstraintEvaluatorSetV1>(
    program: &mut Program<E>,
    graph: &CompiledAppearanceGraph,
    bindings: &AdmittedAppearanceBindings,
) -> Result<Option<ReferenceCompositionV1>, ProgramCompileError> {
    let Some(inputs) = validate_inputs(&mut program.reference_inputs, &program.targets, graph)?
    else {
        return Ok(None);
    };
    let mut workspace = graph.new_workspace().map_err(map_binding_compile_error)?;
    let mut paints = graph
        .prepare_paints_for_surface_cases(
            bindings,
            inputs.iter().map(|entry| entry.0),
            &mut workspace,
        )
        .map_err(map_binding_compile_error)?;
    let evaluation = paints.evaluate_surface_inputs_canonical(|index| inputs[index].1);
    collect(&program.occurrences, |id| {
        let point = evaluation.occurrence(id)?;
        Some((
            Srgb8::new(point.visible()),
            Srgb8::new(point.certificate().backdrop_rgb()),
        ))
    })
    .map(Some)
}

/// Authored appearance before RGB8 realization. Current and reference use the
/// same continuous interpreter, including every nested SurfaceFrom edge.
/// Ordered byte selection continues to call `compile` alone.
#[cfg(test)]
fn compile_continuous<E: ProgramConstraintEvaluatorSetV1>(
    program: &mut Program<E>,
    graph: &CompiledAppearanceGraph,
    bindings: &AdmittedAppearanceBindings,
) -> Result<Option<ReferenceCompositionV1<ContinuousSrgbV1>>, ProgramCompileError> {
    let Some(inputs) = validate_inputs(&mut program.reference_inputs, &program.targets, graph)?
    else {
        return Ok(None);
    };
    let sources = bindings.paint_inputs_canonical();
    let mut workspace = graph
        .new_continuous_workspace()
        .map_err(map_binding_compile_error)?;
    let mut paints = graph
        .prepare_continuous_surface_cases(
            None,
            bindings,
            |index| {
                let source = sources[index].1;
                ContinuousPointPaintValueV1::from_admitted(
                    ContinuousSrgbV1::from_srgb8(source.source()),
                    source.opacity(),
                )
            },
            &mut workspace,
        )
        .map_err(map_binding_compile_error)?;
    let evaluation = paints
        .evaluate_surface_inputs_canonical(|index| ContinuousSrgbV1::from_srgb8(inputs[index].1));
    collect(&program.occurrences, |id| {
        let point = evaluation.occurrence_at(graph.bind_occurrence(id)?)?;
        Some((point.visible(), point.backdrop()))
    })
    .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lcs_occurrence::{
        AdaptingLuminanceCdM2, AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio,
        IEC_SRGB_D65_XYZ_FRAME_V1, SurroundProfileId,
    };

    fn nested_program() -> Program<Wcag22Srgb8V1> {
        let context = AppearanceContextId::from_inputs(
            AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
            IEC_SRGB_D65_XYZ_FRAME_V1,
            AdaptingLuminanceCdM2::try_new(64.0).expect("64 cd/m² is a valid adapting luminance"),
            BackgroundLuminanceRatio::try_new(0.2).expect("0.2 is a valid background ratio"),
            SurroundProfileId::AverageV1,
        );
        let mut program = Program::new(
            [244, 1]
                .into_iter()
                .enumerate()
                .map(|(index, value)| {
                    Source::new(
                        SourceId::new(index as u32),
                        ColorSignal::from_srgb8(Srgb8::new([value; 3])),
                    )
                })
                .collect(),
            (0..2)
                .map(|id| Target::fixed(TargetId::new(id), SourceId::new(id)))
                .collect(),
            ObservationGroup::new(ObservationGroupId::new(0), vec![SurfaceInputPortId::new(0)]),
            vec![
                OpacityInput::new(OpacityInputId::new(0), 0.85),
                OpacityInput::new(OpacityInputId::new(1), 0.5),
            ],
            (0..2)
                .flat_map(|id| {
                    [
                        Paint::Solid {
                            id: PaintId::new(2 * id),
                            target: TargetId::new(id),
                        },
                        Paint::Opacity {
                            id: PaintId::new(2 * id + 1),
                            source: PaintId::new(2 * id),
                            opacity: OpacityInputId::new(id),
                        },
                    ]
                })
                .collect(),
            vec![
                Surface::Input {
                    id: SurfaceId::new(0),
                    input: SurfaceInputPortId::new(0),
                },
                Surface::FromOccurrence {
                    id: SurfaceId::new(1),
                    occurrence: OccurrenceId::new(0),
                },
            ],
            (0..2)
                .map(|id| {
                    Occurrence::new(
                        OccurrenceId::new(id),
                        PaintId::new(2 * id + 1),
                        SurfaceId::new(id),
                        CompositionProfile::EncodedSrgb8SourceOverV1,
                        context,
                    )
                })
                .collect(),
            ConstraintSet::new(vec![], vec![]),
            vec![OutputBinding::new(OutputSlotId::new(0), PaintId::new(3))],
            Wcag22Srgb8V1,
        );
        program.reference_inputs = Some(vec![(SurfaceInputPortId::new(0), Srgb8::new([255; 3]))]);
        program
    }

    fn physical(
        program: &mut Program<Wcag22Srgb8V1>,
    ) -> Result<(CompiledAppearanceGraph, AdmittedAppearanceBindings), ProgramCompileError> {
        canonicalize_sources_and_targets(program)?;
        let graph = lower_graph(program)?.compile().map_err(map_compile_error)?;
        let bindings = graph
            .admit_bindings(&lower_bindings(program)?)
            .map_err(map_binding_compile_error)?;
        Ok((graph, bindings))
    }

    #[test]
    fn nested_reference_matches_continuous_current_and_preserves_ordered_byte_reference()
    -> Result<(), ProgramCompileError> {
        let mut program = nested_program();
        let (graph, bindings) = physical(&mut program)?;
        let occurrence = OccurrenceId::new(1);
        crate::appearance::reset_paint_node_evaluation_count();
        let byte =
            compile(&mut program, &graph, &bindings)?.expect("fixture declares reference inputs");
        assert_eq!(
            crate::appearance::paint_node_evaluation_count(),
            4,
            "ordered compilation must not eagerly evaluate a second reference"
        );
        assert_eq!(
            byte.occurrence(occurrence),
            Some((Srgb8::new([124; 3]), Srgb8::new([246; 3])))
        );
        let reference = compile_continuous(&mut program, &graph, &bindings)?
            .expect("fixture declares reference inputs");
        let expected = reference
            .occurrence(occurrence)
            .expect("nested reference contains the child occurrence");
        assert_eq!(expected.0.channels(), [123.325; 3]);
        assert_eq!(expected.1.channels(), [245.65; 3]);

        let mut workspace = graph
            .new_continuous_workspace()
            .map_err(map_binding_compile_error)?;
        let mut paints = graph
            .prepare_continuous_surface_cases(
                None,
                &bindings,
                |index| {
                    let input = bindings.paint_inputs_canonical()[index].1;
                    ContinuousPointPaintValueV1::from_admitted(
                        ContinuousSrgbV1::from_srgb8(input.source()),
                        input.opacity(),
                    )
                },
                &mut workspace,
            )
            .map_err(map_binding_compile_error)?;
        let view = paints.evaluate_surface_inputs_canonical(|_| {
            ContinuousSrgbV1::from_srgb8(Srgb8::new([255; 3]))
        });
        let current = view
            .occurrence_at(
                graph
                    .bind_occurrence(occurrence)
                    .expect("fixture binds the child occurrence"),
            )
            .expect("evaluated view contains the child occurrence");
        assert_eq!((current.visible(), current.backdrop()), expected);
        Ok(())
    }

    #[test]
    fn both_reference_interpretations_share_authored_input_and_finite_admission()
    -> Result<(), ProgramCompileError> {
        let invalid = |result| {
            matches!(
                result,
                Err(ProgramCompileError::InvalidComputedAssignment(
                    computed_assignment::ComputedAdmissionErrorV1::InvalidReferenceComposition
                ))
            )
        };
        let mut program = nested_program();
        let (graph, bindings) = physical(&mut program)?;
        for inputs in [
            vec![],
            vec![(SurfaceInputPortId::new(99), Srgb8::new([255; 3]))],
            vec![(SurfaceInputPortId::new(0), Srgb8::new([255; 3])); 2],
        ] {
            program.reference_inputs = Some(inputs);
            crate::appearance::reset_paint_node_evaluation_count();
            assert!(invalid(
                compile(&mut program, &graph, &bindings).map(|_| ())
            ));
            assert!(invalid(
                compile_continuous(&mut program, &graph, &bindings).map(|_| ())
            ));
            assert_eq!(crate::appearance::paint_node_evaluation_count(), 0);
        }
        program.reference_inputs = Some(vec![(SurfaceInputPortId::new(0), Srgb8::new([255; 3]))]);
        program.targets[0] = Target::finite(
            TargetId::new(0),
            FinitePaintDomainV1::try_new(vec![TargetCandidateV1::new(
                TargetCandidateId::new(0),
                EncodedPointPaintValueV1::opaque(Srgb8::new([244; 3])),
            )])
            .expect("fixture declares one finite paint"),
        );
        assert!(invalid(
            compile(&mut program, &graph, &bindings).map(|_| ())
        ));
        assert!(invalid(
            compile_continuous(&mut program, &graph, &bindings).map(|_| ())
        ));
        assert_eq!(crate::appearance::paint_node_evaluation_count(), 0);
        program.reference_inputs = None;
        assert!(compile(&mut program, &graph, &bindings)?.is_none());
        assert!(compile_continuous(&mut program, &graph, &bindings)?.is_none());
        assert_eq!(crate::appearance::paint_node_evaluation_count(), 0);
        Ok(())
    }
}
