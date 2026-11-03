//! Сортировка холодной подготовки Program без аллокаций.
//! Один компактный цикл обслуживает разные типы деклараций и их идентичности.
use core::cmp::Ordering;

pub(crate) fn sort<T: Ord>(values: &mut [T]) {
    sort_by(values, Ord::cmp);
}

pub(crate) fn sort_by_key<T, K: Ord>(values: &mut [T], mut key: impl FnMut(&T) -> K) {
    sort_by(values, |left, right| key(left).cmp(&key(right)));
}

pub(crate) fn sort_by<T>(values: &mut [T], mut compare: impl FnMut(&T, &T) -> Ordering) {
    sort_indices(values.len(), &mut |left, right, swap| {
        if swap {
            values.swap(left, right);
            false
        } else {
            compare(&values[left], &values[right]).is_lt()
        }
    });
}

// Один цикл сортировки обслуживает все типы записей холодного компилятора.
// Передача операций по индексам сохраняет владение элементами и обходится без аллокаций.
#[inline(never)]
fn sort_indices(n: usize, operation: &mut dyn FnMut(usize, usize, bool) -> bool) {
    fn sift(
        mut root: usize,
        end: usize,
        operation: &mut dyn FnMut(usize, usize, bool) -> bool,
    ) {
        while root < end / 2 {
            let mut child = root * 2 + 1;
            if child + 1 < end && operation(child, child + 1, false) {
                child += 1;
            }
            if !operation(root, child, false) {
                break;
            }
            operation(root, child, true);
            root = child;
        }
    }
    // Сдвиг равных ключей в heapsort учитывает пропуск неизменившихся
    // клеток при уточнении идентичности, поэтому здесь порядок строгий.
    if (1..n).all(|index| operation(index - 1, index, false)) {
        return;
    }
    for root in (0..n / 2).rev() {
        sift(root, n, operation);
    }
    for end in (1..n).rev() {
        operation(0, end, true);
        sift(0, end, operation);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    #[test]
    fn all_small_multisets_match_the_standard_library() {
        for length in 0..=8 {
            for mut encoded in 0..3usize.pow(length) {
                let mut value = Vec::new();
                for _ in 0..length {
                    value.push(encoded % 3);
                    encoded /= 3;
                }
                let mut expected = value.clone();
                expected.sort_unstable();
                sort(&mut value);
                assert_eq!(value, expected);
                sort_by(&mut value, |a, b| b.cmp(a));
                expected.reverse();
                assert_eq!(value, expected);
            }
        }
    }

    #[test]
    fn strictly_ordered_inputs_need_only_adjacent_comparisons_and_no_swaps() {
        let mut observed = Vec::new();
        let mut expected = Vec::new();
        for length in [0_usize, 1, 2, 17, 4096] {
            let mut values: Vec<_> = (0..length).collect();
            let mut comparisons = 0;
            let mut swaps = 0;
            sort_indices(length, &mut |left, right, swap| {
                if swap {
                    swaps += 1;
                    values.swap(left, right);
                    false
                } else {
                    comparisons += 1;
                    values[left] < values[right]
                }
            });
            assert!(values.windows(2).all(|pair| pair[0] < pair[1]));
            observed.push((length, comparisons, swaps));
            expected.push((length, length.saturating_sub(1), 0));
        }
        assert_eq!(observed, expected);
    }

    #[test]
    fn array_shapes_have_bounded_comparisons_and_no_allocator_events() {
        for shape in 0..5 {
            let mut values = [0u32; 4096];
            for (i, value) in values.iter_mut().enumerate() {
                *value = match shape {
                    0 => i as u32,
                    1 => 4095 - i as u32,
                    2 => (i.min(4095 - i)) as u32,
                    3 => 7,
                    _ => (i as u32).wrapping_mul(2_654_435_761),
                };
            }
            let mut expected = values;
            expected.sort_unstable();
            let mut comparisons = 0usize;
            let (_, events) = crate::test_support::measured_allocator_events(|| {
                sort_by(&mut values, |a, b| {
                    comparisons += 1;
                    a.cmp(b)
                });
            });
            assert_eq!(values, expected);
            assert_eq!(events.alloc + events.realloc + events.dealloc, 0);
            assert!(
                comparisons <= 4 * values.len() * 12,
                "comparison bound {comparisons}"
            );
        }
    }

    #[test]
    fn projected_keys_keep_owned_records_and_their_complete_payload() {
        let mut records = vec![(2, "b"), (1, "a"), (2, "c"), (0, "z")];
        sort_by_key(&mut records, |row| (row.0, row.1));
        assert_eq!(records, vec![(0, "z"), (1, "a"), (2, "b"), (2, "c")]);
        let mut empty: [(); 0] = [];
        sort(&mut empty);
        let mut zero_sized = [(); 257];
        sort(&mut zero_sized);
    }

    #[test]
    fn comparator_failure_never_drops_or_duplicates_an_element() {
        #[derive(Debug)]
        struct Owned {
            id: usize,
            drops: Rc<Cell<usize>>,
        }
        impl Drop for Owned {
            fn drop(&mut self) {
                self.drops.set(self.drops.get() + 1);
            }
        }
        for stop in 0..100 {
            let drops = Rc::new(Cell::new(0));
            let mut records: Vec<_> = (0..17)
                .rev()
                .map(|id| Owned {
                    id,
                    drops: Rc::clone(&drops),
                })
                .collect();
            let mut calls = 0;
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                sort_by(&mut records, |a, b| {
                    if calls == stop {
                        panic!("controlled comparator failure");
                    }
                    calls += 1;
                    a.id.cmp(&b.id)
                });
            }));
            assert_eq!(drops.get(), 0);
            let mut ids: Vec<_> = records.iter().map(|row| row.id).collect();
            ids.sort_unstable();
            assert_eq!(ids, (0..17).collect::<Vec<_>>());
            drop(records);
            assert_eq!(drops.get(), 17);
        }
    }
}
