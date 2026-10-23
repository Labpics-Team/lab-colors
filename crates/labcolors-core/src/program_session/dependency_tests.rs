use super::*;
use proptest::prelude::*;
use std::collections::BTreeSet;

fn occurrence(index: usize) -> OccurrenceId {
    OccurrenceId::new(3 + index as u32 * 17)
}

fn dependencies(seeds: &[u8]) -> IndexedProgramDependenciesV1 {
    let len = seeds.len();
    IndexedProgramDependenciesV1 {
        paint_ids: (0..len).map(|i| (PaintId::new(i as u32), i)).collect(),
        occurrence_ids: (0..len).map(|i| (occurrence(i), i)).collect(),
        paint_dependencies: seeds
            .iter()
            .enumerate()
            .map(|(i, &seed)| {
                if i > 0 && seed & 1 != 0 {
                    IndexedPaintDependencyV1::Paint(usize::from(seed) % i)
                } else {
                    IndexedPaintDependencyV1::Target(usize::from(seed) % len)
                }
            })
            .collect(),
        surface_occurrences: seeds
            .iter()
            .enumerate()
            .map(|(i, &seed)| (i > 0 && seed & 2 != 0).then(|| usize::from(seed / 2) % i))
            .collect(),
        occurrence_paints: seeds.iter().map(|&seed| usize::from(seed) % len).collect(),
        occurrence_surfaces: (0..len).collect(),
    }
}

fn scratch(len: usize) -> ProgramDependencyScratchV1 {
    ProgramDependencyScratchV1 {
        targets: vec![false; len],
        paints: vec![false; len],
        surfaces: vec![false; len],
        occurrences: vec![false; len],
        queue: Vec::with_capacity(len * 4),
    }
}

// Отдельное представление множества и полный обход служат эталоном для
// переиспользуемых массивов. История прошлых вызовов в эталон не входит.
fn closure(index: &IndexedProgramDependenciesV1, roots: &[usize]) -> [Vec<bool>; 4] {
    let len = index.occurrence_ids.len();
    let mut reached = BTreeSet::new();
    let mut pending: Vec<_> = roots.iter().map(|&root| (3, root)).collect();
    while let Some((kind, node)) = pending.pop() {
        if !reached.insert((kind, node)) {
            continue;
        }
        match kind {
            0 => {}
            1 => pending.push(match index.paint_dependencies[node] {
                IndexedPaintDependencyV1::Target(target) => (0, target),
                IndexedPaintDependencyV1::Paint(paint) => (1, paint),
            }),
            2 => {
                if let Some(source) = index.surface_occurrences[node] {
                    pending.push((3, source));
                }
            }
            3 => {
                pending.push((1, index.occurrence_paints[node]));
                pending.push((2, index.occurrence_surfaces[node]));
            }
            _ => unreachable!(),
        }
    }
    std::array::from_fn(|kind| {
        (0..len)
            .map(|node| reached.contains(&(kind, node)))
            .collect()
    })
}

proptest! {
    #[test]
    fn reused_dependency_cones_match_an_independent_closure(
        seeds in prop::collection::vec(any::<u8>(), 1..96),
        updates in prop::collection::vec(prop::collection::vec(any::<u8>(), 0..8), 1..24),
    ) {
        let len = seeds.len();
        let index = dependencies(&seeds);
        let mut scratch = scratch(len);
        let mut assessed = vec![false; len];
        let mut constrained = vec![false; len];
        let mut expected_assessed = vec![false; len];
        let mut expected_constrained = vec![false; len];
        for update in updates {
            let roots: Vec<_> = update.iter().map(|&root| usize::from(root) % len).collect();
            let expected = closure(&index, &roots);
            let coverage = merge_visible_constraint_coverage(
                &index,
                &mut scratch,
                roots.iter().copied().map(occurrence),
                &mut constrained,
                Some(&mut assessed),
            );
            prop_assert!(coverage.is_ok());
            prop_assert_eq!(&scratch.targets, &expected[0]);
            prop_assert_eq!(&scratch.paints, &expected[1]);
            prop_assert_eq!(&scratch.surfaces, &expected[2]);
            prop_assert_eq!(&scratch.occurrences, &expected[3]);
            for i in 0..len {
                expected_constrained[i] |= expected[0][i];
                expected_assessed[i] |= expected[1][i];
            }
            prop_assert_eq!(&constrained, &expected_constrained);
            prop_assert_eq!(&assessed, &expected_assessed);
        }
    }
}

#[test]
fn an_incomplete_dependency_scan_does_not_taint_the_next_cone() -> Result<(), ProgramCompileError> {
    let index = dependencies(&[0, 1, 2, 3, 4, 5]);
    let mut scratch = scratch(6);
    scratch.scan(&index, [occurrence(5)])?;
    assert_eq!(
        scratch.scan(&index, [occurrence(2), OccurrenceId::new(u32::MAX)]),
        Err(ProgramCompileError::InternalInvariant),
    );
    scratch.scan(&index, [occurrence(0)])?;
    let expected = closure(&index, &[0]);
    assert_eq!(scratch.targets, expected[0]);
    assert_eq!(scratch.paints, expected[1]);
    assert_eq!(scratch.surfaces, expected[2]);
    assert_eq!(scratch.occurrences, expected[3]);
    scratch.scan(&index, [])?;
    assert!(
        scratch
            .targets
            .iter()
            .chain(&scratch.paints)
            .chain(&scratch.surfaces)
            .chain(&scratch.occurrences)
            .all(|&value| !value)
    );
    Ok(())
}
