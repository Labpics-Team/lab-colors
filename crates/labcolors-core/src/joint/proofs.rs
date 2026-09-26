//! Символьный допуск двух элементов с полными usize ordinal и точными отказами.
//! Вся матрица 2×2 независимо исчерпывается обычным тестом соседнего модуля.
use super::*;

#[kani::proof]
#[kani::unwind(4)]
fn joint_order_is_complete_unique_and_authored() {
    let input: [usize; 2] = kani::any();
    let two = NonZeroUsize::new(2).unwrap();
    let lengths = NonEmptyFiniteDomainCardinalitiesV1::new(two, Box::new([]));
    let tuples = vec![
        vec![FiniteDomainOrdinalV1::new(input[0])],
        vec![FiniteDomainOrdinalV1::new(input[1])],
    ];
    let valid = input[0] < 2 && input[1] < 2 && input[0] != input[1];
    let result = admit_finite_joint_order_v1(&lengths, tuples);
    assert!(
        result.is_ok() == valid,
        "joint order must admit exactly the complete nonduplicated product"
    );
    if let Ok(order) = result {
        assert!(order.state_count() == 2);
        for (position, row) in order.tuples().enumerate() {
            assert!(
                row.len() == 1 && row[0].index() == input[position],
                "joint admission must preserve authored tuple priority"
            );
        }
    }
    kani::cover!(valid, "complete product");
    kani::cover!(!valid, "invalid product");
    kani::cover!(valid && input[0] == 1, "nonlexicographic policy");
}

#[kani::proof]
#[kani::unwind(4)]
fn doubling_cardinality_cannot_wrap_into_empty_success() {
    let Some(a) = NonZeroUsize::new(kani::any()) else {
        return;
    };
    let two = NonZeroUsize::new(2).unwrap();
    let lengths = NonEmptyFiniteDomainCardinalitiesV1::new(a, vec![two].into_boxed_slice());
    let overflow = a.get() > usize::MAX / 2;
    let result = admit_finite_joint_order_v1(&lengths, Vec::new());
    let expected = if overflow {
        FiniteJointOrderErrorV1::CardinalityOverflow
    } else {
        FiniteJointOrderErrorV1::EmptyOrder
    };
    assert!(
        result == Err(FiniteJointOrderAdmissionErrorV1::Authored(expected)),
        "doubling cardinality overflow must remain distinct from empty authored order"
    );
    kani::cover!(overflow, "overflowing doubled cardinality");
    kani::cover!(!overflow, "representable doubled cardinality");
}
