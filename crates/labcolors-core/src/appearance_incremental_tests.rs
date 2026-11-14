use super::*;
use crate::composition::{
    AdmittedOpacityV1, reset_source_over_evaluation_count, source_over_evaluation_count,
};
use crate::test_support::{AllocatorEvents, measured_allocator_events};

fn branches(count: u32, depth: u32, shared_port: bool) -> CompiledAppearanceGraph {
    let mut paints = Vec::new();
    let mut surfaces = Vec::new();
    let mut occurrences = Vec::new();
    for branch in 0..count {
        paints.push(PaintSpec::Input {
            id: PaintId::new(branch),
            input: PaintInputId::new(branch),
        });
        let first = branch * depth;
        surfaces.push(SurfaceSpec::Input {
            id: SurfaceId::new(first),
            port: SurfaceInputPortId::new(if shared_port { 0 } else { branch }),
        });
        for level in 0..depth {
            let id = first + level;
            if level != 0 {
                surfaces.push(SurfaceSpec::FromOccurrence {
                    id: SurfaceId::new(id),
                    occurrence: OccurrenceId::new(id - 1),
                });
            }
            occurrences.push(OccurrenceSpec {
                id: OccurrenceId::new(id),
                subject: PaintId::new(branch),
                against: SurfaceId::new(id),
                profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
            });
        }
    }
    AppearanceGraphSpec::new(
        (0..count).map(PaintInputId::new).collect(),
        (0..if shared_port { 1 } else { count })
            .map(SurfaceInputPortId::new)
            .collect(),
        vec![],
        paints,
        surfaces,
        occurrences,
    )
    .compile()
    .unwrap()
}

fn admitted(
    graph: &CompiledAppearanceGraph,
    paints: &[(u32, [u8; 3], f64)],
    surfaces: &[[u8; 3]],
) -> AdmittedAppearanceBindings {
    graph
        .admit_bindings(&AppearanceBindings::new(
            paints
                .iter()
                .map(|&(id, source, alpha)| {
                    (
                        PaintInputId::new(id),
                        EncodedPointPaintValueV1::from_admitted(
                            Srgb8::new(source),
                            AdmittedOpacityV1::new(alpha).unwrap(),
                        ),
                    )
                })
                .collect(),
            surfaces
                .iter()
                .enumerate()
                .map(|(index, &rgb)| (SurfaceInputPortId::new(index as u32), Srgb8::new(rgb)))
                .collect(),
            vec![],
        ))
        .unwrap()
}

fn evaluate_owned(
    graph: &CompiledAppearanceGraph,
    bindings: &AdmittedAppearanceBindings,
    workspace: &mut AppearanceWorkspace,
) -> AppearanceEvaluation {
    graph
        .evaluate_admitted_into(bindings, workspace)
        .unwrap()
        .try_to_owned()
        .unwrap()
}

#[test]
fn sparse_changes_compose_only_the_changed_physical_inputs_without_allocating() {
    const COUNT: u32 = 32;
    let graph = branches(COUNT, 1, false);
    let paints: Vec<_> = (0..COUNT).map(|id| (id, [64; 3], 0.5)).collect();
    let mut values = vec![[128; 3]; COUNT as usize];
    let mut bindings = admitted(&graph, &paints, &values);
    let mut workspace = graph.new_workspace().unwrap();
    let storage = workspace.storage_signature();
    reset_source_over_evaluation_count();
    let historical = evaluate_owned(&graph, &bindings, &mut workspace);
    assert_eq!(source_over_evaluation_count(), COUNT as usize);

    for changed in 0..COUNT as usize {
        values[changed] = [130; 3];
        bindings
            .overwrite_surface_inputs_canonical(
                (0..COUNT).map(SurfaceInputPortId::new),
                |index| Srgb8::new(values[index]),
            )
            .unwrap();
        reset_source_over_evaluation_count();
        let (count, events) = measured_allocator_events(|| {
            let view = graph
                .evaluate_admitted_into(&bindings, &mut workspace)
                .unwrap();
            view.occurrences().count()
        });
        assert_eq!(count, COUNT as usize);
        assert_eq!(events, AllocatorEvents::default());
        assert_eq!(source_over_evaluation_count(), 1);
        assert_eq!(workspace.storage_signature(), storage);
    }
    reset_source_over_evaluation_count();
    graph
        .evaluate_admitted_into(&bindings, &mut workspace)
        .unwrap();
    assert_eq!(source_over_evaluation_count(), 0);
    assert!(historical.occurrences.iter().all(|resolved| {
        resolved.visible() == [96; 3] && resolved.certificate().backdrop_rgb() == [128; 3]
    }));
}

#[test]
fn equal_output_stops_downstream_composition_without_hiding_changed_provenance() {
    const DEPTH: u32 = 16;
    let graph = branches(1, DEPTH, false);
    let paints = [(0, [64; 3], 1.0)];
    let first = admitted(&graph, &paints, &[[128; 3]]);
    let changed = admitted(&graph, &paints, &[[192; 3]]);
    let mut workspace = graph.new_workspace().unwrap();
    let historical = evaluate_owned(&graph, &first, &mut workspace);
    reset_source_over_evaluation_count();
    let actual = evaluate_owned(&graph, &changed, &mut workspace);
    assert_eq!(source_over_evaluation_count(), 1);
    assert_eq!(historical.occurrences[0].visible(), actual.occurrences[0].visible());
    assert_ne!(historical.occurrences[0].certificate(), actual.occurrences[0].certificate());
    assert_eq!(&historical.occurrences[1..], &actual.occurrences[1..]);
    let expected = evaluate_owned(&graph, &changed, &mut graph.new_workspace().unwrap());
    assert_eq!(actual, expected);
    assert_eq!(historical.occurrences[0].certificate().backdrop_rgb(), [128; 3]);
}

#[test]
fn same_rgb_does_not_hide_source_alpha_or_backdrop_changes() {
    let graph = branches(1, 1, false);
    let mut workspace = graph.new_workspace().unwrap();
    let mut previous = None;
    for (source, alpha, backdrop) in [
        ([128; 3], 0.0, [128; 3]),
        ([192; 3], 0.0, [128; 3]),
        ([128; 3], 0.5, [128; 3]),
        ([128; 3], 1.0, [128; 3]),
        ([128; 3], 1.0, [192; 3]),
    ] {
        let bindings = admitted(&graph, &[(0, source, alpha)], &[backdrop]);
        reset_source_over_evaluation_count();
        let actual = evaluate_owned(&graph, &bindings, &mut workspace);
        assert_eq!(source_over_evaluation_count(), 1);
        let resolved = actual.occurrences[0];
        assert_eq!(resolved.visible(), [128; 3]);
        assert_eq!(resolved.certificate().subject_rgb(), source);
        assert_eq!(resolved.certificate().subject_opacity_bits(), alpha.to_bits());
        assert_eq!(resolved.certificate().backdrop_rgb(), backdrop);
        assert_ne!(previous, Some(resolved));
        previous = Some(resolved);
        reset_source_over_evaluation_count();
        assert_eq!(evaluate_owned(&graph, &bindings, &mut workspace), actual);
        assert_eq!(source_over_evaluation_count(), 0);
    }
}

#[test]
fn cached_execution_matches_fresh_workspace_across_shared_and_deep_sequences() {
    for (count, depth, shared) in [(1, 1, false), (8, 1, false), (3, 7, false), (8, 5, true)] {
        let graph = branches(count, depth, shared);
        let mut workspace = graph.new_workspace().unwrap();
        let mut paints: Vec<_> = (0..count).map(|id| (id, [64; 3], 0.5)).collect();
        let mut surfaces = vec![[128; 3]; if shared { 1 } else { count as usize }];
        let mut seed = 0x73c8_90ad_u32;
        for step in 0..128 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let index = seed as usize % surfaces.len();
            if step % 3 != 0 {
                surfaces[index] = [seed as u8, (seed >> 8) as u8, (seed >> 16) as u8];
            }
            if step % 7 == 0 {
                let index = seed as usize % paints.len();
                paints[index].1 = [(seed >> 24) as u8; 3];
                paints[index].2 = [0.0, 0.25, 0.5, 1.0][step / 7 % 4];
            }
            let bindings = admitted(&graph, &paints, &surfaces);
            let actual = evaluate_owned(&graph, &bindings, &mut workspace);
            let expected = evaluate_owned(&graph, &bindings, &mut graph.new_workspace().unwrap());
            assert_eq!(actual, expected, "branches={count}, depth={depth}, shared={shared}, step={step}");
        }
    }
}

#[test]
fn same_shape_graph_switch_invalidates_cache_and_rejected_schema_preserves_it() {
    let first = branches(3, 2, false);
    let second = branches(3, 2, true);
    let paints: Vec<_> = (0..3).map(|id| (id, [64; 3], 0.5)).collect();
    let first_bindings = admitted(&first, &paints, &[[128; 3]; 3]);
    let second_bindings = admitted(&second, &paints, &[[128; 3]]);
    let mut workspace = first.new_workspace().unwrap();
    let historical = evaluate_owned(&first, &first_bindings, &mut workspace);
    let storage = workspace.storage_signature();
    reset_source_over_evaluation_count();
    assert!(matches!(
        second.evaluate_admitted_into(&first_bindings, &mut workspace),
        Err(BindingError::IncompatibleAdmittedBindings)
    ));
    assert_eq!(evaluate_owned(&first, &first_bindings, &mut workspace), historical);
    assert_eq!(source_over_evaluation_count(), 0);

    let actual = evaluate_owned(&second, &second_bindings, &mut workspace);
    assert_eq!(source_over_evaluation_count(), 6);
    assert_eq!(workspace.storage_signature(), storage);
    let expected = evaluate_owned(&second, &second_bindings, &mut second.new_workspace().unwrap());
    assert_eq!(actual, expected);
    reset_source_over_evaluation_count();
    assert_eq!(evaluate_owned(&first, &first_bindings, &mut workspace), historical);
    assert_eq!(source_over_evaluation_count(), 6);
}
