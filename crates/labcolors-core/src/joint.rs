//! Допуск полного порядка над конечным произведением target-доменов.
//!
//! Модуль не исполняет solver и не знает evaluator families. Он лишь проверяет
//! объявленную клиентом политику порядка один раз до runtime и запечатывает
//! канонические ординалы для единственного исполнителя [`crate::program_session`].

use core::num::NonZeroUsize;

/// One canonical candidate ordinal inside a finite target domain.
///
/// The ordinal is assigned only after the owning Program has sorted the
/// target's opaque candidate IDs. It is therefore an internal compiled index,
/// never client identity or declaration-order policy.
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

/// Non-empty cardinality vector of the finite targets that own one joint
/// product. Requiring the first dimension here prevents a mathematically valid
/// zero-dimensional product from impersonating a Program joint selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NonEmptyFiniteDomainCardinalitiesV1 {
    first: NonZeroUsize,
    rest: Box<[NonZeroUsize]>,
}

impl NonEmptyFiniteDomainCardinalitiesV1 {
    pub(crate) fn new(first: NonZeroUsize, rest: Box<[NonZeroUsize]>) -> Self {
        Self { first, rest }
    }

    fn iter(&self) -> impl Iterator<Item = NonZeroUsize> + '_ {
        std::iter::once(self.first).chain(self.rest.iter().copied())
    }

    fn len(&self) -> usize {
        self.rest.len() + 1
    }
}

/// A fully admitted total order over the product of finite target domains.
/// Each tuple is stored in canonical target order. The tuple order is authored
/// policy; no target ID, candidate value, or declaration position becomes an
/// implicit tie-break.
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
        // `first` makes the admitted order structurally non-empty; the
        // remaining slice length is bounded by Rust's allocation limit.
        self.rest.len() + 1
    }
}

/// Failure to admit a declared finite joint selection order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FiniteJointOrderErrorV1 {
    CardinalityOverflow,
    EmptyOrder,
    TupleArity {
        tuple: usize,
        expected: usize,
        actual: usize,
    },
    OrdinalOutOfDomain {
        tuple: usize,
        dimension: usize,
        ordinal: usize,
        domain_len: usize,
    },
    DuplicateTuple {
        first: usize,
        duplicate: usize,
    },
    IncompleteOrder {
        expected: usize,
        actual: usize,
    },
}

/// Admission separates invalid authored policy from an inability to allocate
/// the proof table. Resource pressure is not evidence that client data is
/// malformed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FiniteJointOrderAdmissionErrorV1 {
    Authored(FiniteJointOrderErrorV1),
    ResourceExhausted,
    InternalInvariant,
}

/// Check and seal an explicit total order over a finite product domain.
///
/// This function deliberately does not synthesize lexicographic policy. A
/// caller must enumerate every tuple exactly once. Non-empty dimensions are
/// admitted by type; checked multiplication and fallible allocation happen
/// before the result can reach runtime.
pub(crate) fn admit_finite_joint_order_v1(
    domain_lengths: &NonEmptyFiniteDomainCardinalitiesV1,
    authored: Vec<Vec<FiniteDomainOrdinalV1>>,
) -> Result<AdmittedFiniteJointOrderV1, FiniteJointOrderAdmissionErrorV1> {
    use FiniteJointOrderAdmissionErrorV1 as AdmissionError;

    let mut expected = 1usize;
    for domain_len in domain_lengths.iter() {
        expected = expected
            .checked_mul(domain_len.get())
            .ok_or(AdmissionError::Authored(
                FiniteJointOrderErrorV1::CardinalityOverflow,
            ))?;
    }
    if authored.is_empty() {
        return Err(AdmissionError::Authored(
            FiniteJointOrderErrorV1::EmptyOrder,
        ));
    }
    if authored.len() != expected {
        return Err(AdmissionError::Authored(
            FiniteJointOrderErrorV1::IncompleteOrder {
                expected,
                actual: authored.len(),
            },
        ));
    }

    // The mixed-radix ordinal is a bijection over the admitted product. A
    // fallibly allocated first-seen table makes duplicate admission O(states ×
    // dimensions), rather than comparing every tuple with every earlier tuple.
    let mut first_seen = Vec::new();
    first_seen
        .try_reserve_exact(expected)
        .map_err(|_| AdmissionError::ResourceExhausted)?;
    first_seen.resize(expected, usize::MAX);

    let mut rest = Vec::new();
    rest.try_reserve_exact(authored.len() - 1)
        .map_err(|_| AdmissionError::ResourceExhausted)?;
    let mut first_tuple = None;
    for (tuple_index, tuple) in authored.into_iter().enumerate() {
        if tuple.len() != domain_lengths.len() {
            return Err(AdmissionError::Authored(
                FiniteJointOrderErrorV1::TupleArity {
                    tuple: tuple_index,
                    expected: domain_lengths.len(),
                    actual: tuple.len(),
                },
            ));
        }
        let mut mixed_radix_index = 0usize;
        for (dimension, (ordinal, domain_len)) in
            tuple.iter().zip(domain_lengths.iter()).enumerate()
        {
            let domain_len = domain_len.get();
            if ordinal.index() >= domain_len {
                return Err(AdmissionError::Authored(
                    FiniteJointOrderErrorV1::OrdinalOutOfDomain {
                        tuple: tuple_index,
                        dimension,
                        ordinal: ordinal.index(),
                        domain_len,
                    },
                ));
            }
            let next_index = mixed_radix_index
                .checked_mul(domain_len)
                .and_then(|index| index.checked_add(ordinal.index()))
                .ok_or(AdmissionError::InternalInvariant)?;
            mixed_radix_index = next_index;
        }
        debug_assert!(mixed_radix_index < expected);
        let first = &mut first_seen[mixed_radix_index];
        if *first != usize::MAX {
            return Err(AdmissionError::Authored(
                FiniteJointOrderErrorV1::DuplicateTuple {
                    first: *first,
                    duplicate: tuple_index,
                },
            ));
        }
        *first = tuple_index;
        let tuple = tuple.into_boxed_slice();
        if first_tuple.is_none() {
            first_tuple = Some(tuple);
        } else {
            rest.push(tuple);
        }
    }

    Ok(AdmittedFiniteJointOrderV1 {
        // Empty input returned above, so failure here reports compiler drift,
        // never malformed authored policy.
        first: first_tuple.ok_or(AdmissionError::InternalInvariant)?,
        rest: rest.into_boxed_slice(),
    })
}
