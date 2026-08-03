//! Материализация полного порядка конечного совместного пространства.
//!
//! Клиент не перечисляет декартовы кортежи. Компилятор сначала связывает одну
//! [`SelectionRelease`](crate::program_session::SelectionReleaseV1) с
//! каноническими target-доменами, а затем этот модуль материализует её точный
//! лексикографический порядок. Значения opaque ID, байты цвета и порядок
//! деклараций не становятся неявными критериями выбора.

use core::num::NonZeroUsize;

/// Канонический ординал кандидата внутри одного конечного target-домена.
///
/// Ординал назначается только после сортировки opaque candidate ID владельцем
/// Program. Он является compiled index, но никогда не selection policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FiniteDomainOrdinalV1(usize);

impl FiniteDomainOrdinalV1 {
    pub(crate) const fn new(index: usize) -> Self {
        Self(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0
    }
}

/// Непустой вектор мощностей канонических target-доменов.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NonEmptyFiniteDomainCardinalitiesV1 {
    first: NonZeroUsize,
    rest: Box<[NonZeroUsize]>,
}

impl NonEmptyFiniteDomainCardinalitiesV1 {
    pub(crate) fn new(first: NonZeroUsize, rest: Box<[NonZeroUsize]>) -> Self {
        Self { first, rest }
    }

    fn iter(&self) -> impl DoubleEndedIterator<Item = NonZeroUsize> + '_ {
        std::iter::once(self.first).chain(self.rest.iter().copied())
    }

    fn len(&self) -> usize {
        self.rest.len() + 1
    }

    fn get(&self, dimension: usize) -> Option<NonZeroUsize> {
        if dimension == 0 {
            Some(self.first)
        } else {
            self.rest.get(dimension - 1).copied()
        }
    }
}

/// Одна уже связанная objective: какой canonical target сравнивается и в
/// каком явном семантическом порядке идут его candidate ordinals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundFiniteTargetPreferenceV1 {
    dimension: usize,
    candidates: Box<[FiniteDomainOrdinalV1]>,
}

impl BoundFiniteTargetPreferenceV1 {
    pub(crate) fn new(dimension: usize, candidates: Box<[FiniteDomainOrdinalV1]>) -> Self {
        Self {
            dimension,
            candidates,
        }
    }
}

/// Непустая последовательность связанных objectives одного SelectionRelease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BoundFiniteSelectionReleaseV1 {
    first: BoundFiniteTargetPreferenceV1,
    rest: Box<[BoundFiniteTargetPreferenceV1]>,
}

impl BoundFiniteSelectionReleaseV1 {
    pub(crate) fn new(
        first: BoundFiniteTargetPreferenceV1,
        rest: Box<[BoundFiniteTargetPreferenceV1]>,
    ) -> Self {
        Self { first, rest }
    }

    fn iter(&self) -> impl DoubleEndedIterator<Item = &BoundFiniteTargetPreferenceV1> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }

    fn len(&self) -> usize {
        self.rest.len() + 1
    }
}

/// Полный compiler-owned порядок конечного декартова пространства.
///
/// Каждый tuple хранится в canonical target order. Порядок tuples получен
/// только из связанного SelectionRelease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdmittedFiniteJointOrderV1 {
    first: Box<[FiniteDomainOrdinalV1]>,
    rest: Box<[Box<[FiniteDomainOrdinalV1]>]>,
}

impl AdmittedFiniteJointOrderV1 {
    pub(crate) fn tuples(&self) -> impl Iterator<Item = &[FiniteDomainOrdinalV1]> + '_ {
        std::iter::once(self.first.as_ref()).chain(self.rest.iter().map(Box::as_ref))
    }

    pub(crate) fn state_count(&self) -> usize {
        self.rest.len() + 1
    }
}

/// Неуспех материализации после program-level binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FiniteJointCompilationErrorV1 {
    CardinalityOverflow,
    ResourceExhausted,
    InternalInvariant,
}

/// Материализует полный лексикографический порядок joint states.
///
/// Первый objective является наиболее значимым, последний меняется быстрее
/// всех. Вход должен быть полностью связан Program-компилятором: каждая
/// dimension и каждый её ordinal встречаются ровно один раз. Функция повторно
/// проверяет эти инварианты, чтобы compiler drift не стал executable state.
pub(crate) fn compile_finite_joint_order_v1(
    domain_lengths: &NonEmptyFiniteDomainCardinalitiesV1,
    release: &BoundFiniteSelectionReleaseV1,
) -> Result<AdmittedFiniteJointOrderV1, FiniteJointCompilationErrorV1> {
    use FiniteJointCompilationErrorV1 as Error;

    if release.len() != domain_lengths.len() {
        return Err(Error::InternalInvariant);
    }

    let dimension_count = domain_lengths.len();
    let mut seen_dimensions = Vec::new();
    seen_dimensions
        .try_reserve_exact(dimension_count)
        .map_err(|_| Error::ResourceExhausted)?;
    seen_dimensions.resize(dimension_count, false);

    for objective in release.iter() {
        let Some(domain_len) = domain_lengths.get(objective.dimension) else {
            return Err(Error::InternalInvariant);
        };
        if seen_dimensions[objective.dimension] {
            return Err(Error::InternalInvariant);
        }
        seen_dimensions[objective.dimension] = true;
        if objective.candidates.len() != domain_len.get() {
            return Err(Error::InternalInvariant);
        }

        let mut seen_ordinals = Vec::new();
        seen_ordinals
            .try_reserve_exact(domain_len.get())
            .map_err(|_| Error::ResourceExhausted)?;
        seen_ordinals.resize(domain_len.get(), false);
        for ordinal in &objective.candidates {
            let Some(seen) = seen_ordinals.get_mut(ordinal.index()) else {
                return Err(Error::InternalInvariant);
            };
            if *seen {
                return Err(Error::InternalInvariant);
            }
            *seen = true;
        }
    }
    if seen_dimensions.iter().any(|seen| !seen) {
        return Err(Error::InternalInvariant);
    }

    let state_count = domain_lengths.iter().try_fold(1usize, |count, domain_len| {
        count.checked_mul(domain_len.get())
    });
    let state_count = state_count.ok_or(Error::CardinalityOverflow)?;

    let mut tuples = Vec::new();
    tuples
        .try_reserve_exact(state_count)
        .map_err(|_| Error::ResourceExhausted)?;
    for state_index in 0..state_count {
        let mut tuple = Vec::new();
        tuple
            .try_reserve_exact(dimension_count)
            .map_err(|_| Error::ResourceExhausted)?;
        tuple.resize(dimension_count, FiniteDomainOrdinalV1::new(0));

        let mut remainder = state_index;
        for objective in release.iter().rev() {
            let radix = objective.candidates.len();
            let rank = remainder % radix;
            remainder /= radix;
            tuple[objective.dimension] = objective.candidates[rank];
        }
        if remainder != 0 {
            return Err(Error::InternalInvariant);
        }
        tuples.push(tuple.into_boxed_slice());
    }

    let mut tuples = tuples.into_iter();
    let first = tuples.next().ok_or(Error::InternalInvariant)?;
    let rest = tuples.collect::<Vec<_>>().into_boxed_slice();
    Ok(AdmittedFiniteJointOrderV1 { first, rest })
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn non_zero(value: usize) -> NonZeroUsize {
        NonZeroUsize::new(value).unwrap()
    }

    #[test]
    fn compiler_materializes_objective_order_not_dimension_order() {
        let domains = NonEmptyFiniteDomainCardinalitiesV1::new(
            non_zero(2),
            vec![non_zero(2)].into_boxed_slice(),
        );
        let release = BoundFiniteSelectionReleaseV1::new(
            BoundFiniteTargetPreferenceV1::new(
                1,
                vec![FiniteDomainOrdinalV1::new(1), FiniteDomainOrdinalV1::new(0)]
                    .into_boxed_slice(),
            ),
            vec![BoundFiniteTargetPreferenceV1::new(
                0,
                vec![FiniteDomainOrdinalV1::new(0), FiniteDomainOrdinalV1::new(1)]
                    .into_boxed_slice(),
            )]
            .into_boxed_slice(),
        );

        let tuples = compile_finite_joint_order_v1(&domains, &release)
            .unwrap()
            .tuples()
            .map(|tuple| {
                tuple
                    .iter()
                    .map(|ordinal| ordinal.index())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        assert_eq!(tuples, [[0, 1], [1, 1], [0, 0], [1, 0]]);
    }

    #[test]
    fn cardinality_overflow_is_typed_before_joint_state_allocation() {
        let dimension_count = usize::BITS as usize;
        let domains = NonEmptyFiniteDomainCardinalitiesV1::new(
            non_zero(2),
            vec![non_zero(2); dimension_count - 1].into_boxed_slice(),
        );
        let objectives = (0..dimension_count)
            .map(|dimension| {
                BoundFiniteTargetPreferenceV1::new(
                    dimension,
                    vec![FiniteDomainOrdinalV1::new(0), FiniteDomainOrdinalV1::new(1)]
                        .into_boxed_slice(),
                )
            })
            .collect::<Vec<_>>();
        let release = BoundFiniteSelectionReleaseV1::new(
            objectives[0].clone(),
            objectives[1..].to_vec().into_boxed_slice(),
        );

        assert_eq!(
            compile_finite_joint_order_v1(&domains, &release),
            Err(FiniteJointCompilationErrorV1::CardinalityOverflow)
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn every_small_release_compiles_to_the_exact_mixed_radix_total_order(
            lengths in prop::collection::vec(1usize..=4, 1..=4),
            seed in any::<u64>(),
        ) {
            let domains = NonEmptyFiniteDomainCardinalitiesV1::new(
                non_zero(lengths[0]),
                lengths[1..]
                    .iter()
                    .copied()
                    .map(non_zero)
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            );

            let mut dimensions = (0..lengths.len()).collect::<Vec<_>>();
            let dimension_rotation = (seed as usize) % dimensions.len();
            dimensions.rotate_left(dimension_rotation);
            if seed & 1 != 0 {
                dimensions.reverse();
            }

            let objectives = dimensions
                .iter()
                .copied()
                .map(|dimension| {
                    let mut candidates = (0..lengths[dimension])
                        .map(FiniteDomainOrdinalV1::new)
                        .collect::<Vec<_>>();
                    let rotation = ((seed >> (dimension % 32)) as usize) % candidates.len();
                    candidates.rotate_left(rotation);
                    if (seed >> ((dimension + 1) % 32)) & 1 != 0 {
                        candidates.reverse();
                    }
                    BoundFiniteTargetPreferenceV1::new(
                        dimension,
                        candidates.into_boxed_slice(),
                    )
                })
                .collect::<Vec<_>>();
            let release = BoundFiniteSelectionReleaseV1::new(
                objectives[0].clone(),
                objectives[1..].to_vec().into_boxed_slice(),
            );

            let order = compile_finite_joint_order_v1(&domains, &release).unwrap();
            let expected_count = lengths.iter().product::<usize>();
            prop_assert_eq!(order.state_count(), expected_count);

            let tuples = order.tuples().collect::<Vec<_>>();
            for (state_index, tuple) in tuples.iter().enumerate() {
                let mut reconstructed = 0usize;
                for objective in release.iter() {
                    let rank = objective
                        .candidates
                        .iter()
                        .position(|candidate| candidate == &tuple[objective.dimension])
                        .unwrap();
                    reconstructed = reconstructed
                        .checked_mul(objective.candidates.len())
                        .and_then(|value| value.checked_add(rank))
                        .unwrap();
                }
                prop_assert_eq!(reconstructed, state_index);
            }
            for pair in tuples.windows(2) {
                prop_assert_ne!(pair[0], pair[1]);
            }
        }
    }
}
