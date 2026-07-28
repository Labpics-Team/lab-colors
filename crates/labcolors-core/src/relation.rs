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

    /// Переносит уже доказанную topology через инъективную смену ID-типа.
    ///
    /// Используется только sealed lowering facade-ID → Core-ID. Closure обязана
    /// сохранять равенство и порядок; поэтому повторная public admission здесь
    /// создала бы второй источник истины того же инварианта.
    pub(crate) fn map_ordered<U>(self, mut map: impl FnMut(T) -> U) -> DirectedRelationV1<U>
    where
        U: Copy + Ord,
    {
        let reference = map(self.reference);
        let candidates = self
            .candidates
            .iter()
            .copied()
            .map(map)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        DirectedRelationV1 {
            reference,
            candidates,
        }
    }
}
