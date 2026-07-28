//! Каноническая алгебра направленных отношений между opaque ID.
//!
//! Модуль определяет только topology и admission. Он не приписывает ID
//! человеческий смысл и не выбирает evaluator, physical level или policy.

/// Ошибка admission направленного отношения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DirectedRelationErrorV1<T> {
    /// Направленное отношение обязано иметь хотя бы одного кандидата.
    EmptyCandidates,
    /// Один кандидат объявлен повторно.
    DuplicateCandidate { candidate: T },
    /// Reference не может одновременно быть собственным кандидатом.
    ReferenceInCandidates { reference: T },
}

/// Направленное отношение `reference → non-empty canonical candidates`.
///
/// Candidate declaration order не является семантикой. После admission набор
/// отсортирован и уникален, а reference доказанно в него не входит.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectedRelationV1<T> {
    reference: T,
    candidates: Box<[T]>,
}

impl<T> DirectedRelationV1<T>
where
    T: Copy + Ord,
{
    /// Парсит authored topology до помещения в Draft.
    pub(crate) fn try_new(
        reference: T,
        mut candidates: Vec<T>,
    ) -> Result<Self, DirectedRelationErrorV1<T>> {
        if candidates.is_empty() {
            return Err(DirectedRelationErrorV1::EmptyCandidates);
        }
        candidates.sort_unstable();
        if let Some(candidate) = candidates
            .windows(2)
            .find(|pair| pair[0] == pair[1])
            .map(|pair| pair[0])
        {
            return Err(DirectedRelationErrorV1::DuplicateCandidate { candidate });
        }
        if candidates.binary_search(&reference).is_ok() {
            return Err(DirectedRelationErrorV1::ReferenceInCandidates { reference });
        }
        Ok(Self {
            reference,
            candidates: candidates.into_boxed_slice(),
        })
    }

    pub(crate) const fn reference(&self) -> T {
        self.reference
    }

    pub(crate) fn candidates(&self) -> &[T] {
        &self.candidates
    }

    /// Переносит topology в другой ID-тип и заново доказывает её инварианты.
    ///
    /// Даже внутренний mapper нельзя считать инъективным по комментарию: типы
    /// обязаны не позволять ему создать повтор кандидата или reference.
    pub(crate) fn try_map<U>(
        self,
        mut map: impl FnMut(T) -> U,
    ) -> Result<DirectedRelationV1<U>, DirectedRelationErrorV1<U>>
    where
        U: Copy + Ord,
    {
        let reference = map(self.reference);
        let candidates = self.candidates.iter().copied().map(map).collect::<Vec<_>>();
        DirectedRelationV1::try_new(reference, candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remap_rechecks_reference_exclusion_instead_of_trusting_the_mapper() {
        let relation = DirectedRelationV1::try_new(0_u8, vec![1_u8]).unwrap();

        assert_eq!(
            relation.try_map(|_| 0_u16),
            Err(DirectedRelationErrorV1::ReferenceInCandidates { reference: 0 })
        );
    }

    #[test]
    fn remap_rechecks_candidate_uniqueness_instead_of_trusting_the_mapper() {
        let relation = DirectedRelationV1::try_new(0_u8, vec![1_u8, 2_u8]).unwrap();

        assert_eq!(
            relation.try_map(|id| u16::from(id != 0)),
            Err(DirectedRelationErrorV1::DuplicateCandidate { candidate: 1 })
        );
    }
}
