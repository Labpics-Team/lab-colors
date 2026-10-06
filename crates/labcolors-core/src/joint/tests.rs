use super::*;

/// Все 4^4 последовательности пар, не случайная выборка перестановок.
#[test]
fn all_two_by_two_sequences_preserve_complete_authored_policy() {
    let two = NonZeroUsize::new(2).unwrap();
    let lengths = NonEmptyFiniteDomainCardinalitiesV1::new(two, vec![two].into_boxed_slice());
    let mut accepted = 0;
    for encoding in 0_u16..256 {
        let input: [[usize; 2]; 4] = core::array::from_fn(|i| {
            [
                usize::from((encoding >> (2 * i)) & 1),
                usize::from((encoding >> (2 * i + 1)) & 1),
            ]
        });
        let valid = (0..4).all(|i| (i + 1..4).all(|j| input[i] != input[j]));
        let tuples = input
            .iter()
            .map(|row| {
                row.iter()
                    .copied()
                    .map(FiniteDomainOrdinalV1::new)
                    .collect()
            })
            .collect();
        let result = admit_finite_joint_order_v1(&lengths, tuples);
        assert_eq!(result.is_ok(), valid, "input={input:?}");
        if let Ok(order) = result {
            accepted += 1;
            let actual: Vec<Vec<usize>> = order
                .tuples()
                .map(|row| row.iter().map(|v| v.index()).collect())
                .collect();
            assert_eq!(
                actual,
                input.iter().map(|row| row.to_vec()).collect::<Vec<_>>()
            );
        }
    }
    assert_eq!(
        accepted, 24,
        "допустимы ровно 4! авторских перестановок полного произведения"
    );
}

/// Этот конечный grammar-домен дешевле исчерпать напрямую, чем строить SMT-heap.
#[test]
fn every_small_shape_failure_is_typed_before_runtime() {
    let lengths =
        NonEmptyFiniteDomainCardinalitiesV1::new(NonZeroUsize::new(2).unwrap(), Box::new([]));
    for count in 0..4 {
        for arity in 0..3 {
            let tuples = (0..count)
                .map(|_| vec![FiniteDomainOrdinalV1::new(0); arity])
                .collect();
            let result = admit_finite_joint_order_v1(&lengths, tuples);
            let expected = if count == 0 {
                FiniteJointOrderErrorV1::EmptyOrder
            } else if count != 2 {
                FiniteJointOrderErrorV1::IncompleteOrder {
                    expected: 2,
                    actual: count,
                }
            } else if arity != 1 {
                FiniteJointOrderErrorV1::TupleArity {
                    tuple: 0,
                    expected: 1,
                    actual: arity,
                }
            } else {
                FiniteJointOrderErrorV1::DuplicateTuple {
                    first: 0,
                    duplicate: 1,
                }
            };
            assert_eq!(
                result,
                Err(FiniteJointOrderAdmissionErrorV1::Authored(expected))
            );
        }
    }
}
