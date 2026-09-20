//! Independent cleanliness relation corpus (CLEAN-01).
//!
//! INV-04: cleanliness — частичный порядок, а не скаляр. Корпус хранит
//! попарные отношения между вердиктами как типизированные значения
//! (Cleaner/Dirtier/Incomparable) с привязкой к контексту оценки
//! (alpha и backdrop). Тотальный порядок над несравнимыми вердиктами —
//! ошибка, а не округление.
//!
//! Корпус — чистые данные и валидация; он не выносит вердикты и не
//! агрегирует оценки (агрегация живёт в alpha_aggregation, отношения —
//! здесь, они не смешиваются).

use crate::composition::AdmittedOpacityV1;

/// Контекст, в котором зафиксировано отношение: одно и то же сравнение
/// может быть несравнимо при другой alpha или другом фоне.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RelationContextV1 {
    /// Alpha-окно occurrence. `None` = полностью прозрачный вход
    /// (сравнение по потенциальалу, не по видимому вкладу).
    pub(crate) alpha: Option<AdmittedOpacityV1>,
    /// Идентификатор фона (например, hash контекста backdrop).
    pub(crate) backdrop_anchor: [u8; 32],
}

/// Отношение двух вердиктов в одном контексте.
/// Скалярного «счёта» нет: только направление либо несравнимость.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CleanRelationV1 {
    Cleaner,
    Dirtier,
    Incomparable,
}

impl CleanRelationV1 {
    /// Инверсия отношения: избыточные записи корпуса (A→B и B→A)
    /// обязаны быть согласованы, иначе это cycle-дефект данных.
    #[must_use]
    pub(crate) const fn inverse(self) -> Self {
        match self {
            Self::Cleaner => Self::Dirtier,
            Self::Dirtier => Self::Cleaner,
            Self::Incomparable => Self::Incomparable,
        }
    }
}

/// Ошибка корпуса или заявленного порядка.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelationCorpusErrorV1 {
    /// Матрица не согласована: A→B и B→A дают разные отношения.
    InconsistentMatrix,
    /// Вершины связаны циклом Cleaner-отношений.
    CycleDetected,
    /// Заявленный порядок содержит пару, несравнимую в корпусе.
    IncomparablePairForcedIntoTotalOrder,
    /// Пара заявленного порядка противоречит корпусу.
    ContradictsCorpus,
}

/// Стабильный идентификатор вердикта в корпусе.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct VerdictIdV1(pub(crate) u16);

/// Independent corpus: вердикты и попарные отношения по контекстам.
///
/// Отношения хранятся только в одном направлении (lo < hi по VerdictIdV1);
/// симметрия и ацикличность проверяются при вставке.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RelationCorpusV1 {
    entries: Vec<CorpusEntryV1>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CorpusEntryV1 {
    context: RelationContextV1,
    a: VerdictIdV1,
    b: VerdictIdV1,
    /// Отношение a к b в этом контексте.
    relation: CleanRelationV1,
}

impl RelationCorpusV1 {
    pub(crate) const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Вставляет отношение a→b в контексте. Канонизирует порядок пары,
    /// проверяет согласованность с уже вставленной обратной записью
    /// и ацикличность Cleaner-цепочек.
    pub(crate) fn insert(
        &mut self,
        context: RelationContextV1,
        a: VerdictIdV1,
        b: VerdictIdV1,
        relation: CleanRelationV1,
    ) -> Result<(), RelationCorpusErrorV1> {
        let (lo, hi, forward) = if a <= b {
            (a, b, relation)
        } else {
            (b, a, relation.inverse())
        };
        if let Some(existing) = self
            .entries
            .iter()
            .find(|e| e.context == context && e.a == lo && e.b == hi)
        {
            if existing.relation != forward {
                return Err(RelationCorpusErrorV1::InconsistentMatrix);
            }
            return Ok(());
        }
        let entry = CorpusEntryV1 {
            context,
            a: lo,
            b: hi,
            relation: forward,
        };
        if self.would_create_cycle(&entry) {
            return Err(RelationCorpusErrorV1::CycleDetected);
        }
        self.entries.push(entry);
        Ok(())
    }

    /// Ребро «tail Cleaner head» замыкает цикл, если в том же контексте уже
    /// существует Cleaner-путь head → tail. `Dirtier`-запись — то же ребро
    /// в перевёрнутом направлении, а не отдельный вид отношения.
    fn would_create_cycle(&self, candidate: &CorpusEntryV1) -> bool {
        let (tail, head) = match candidate.relation {
            CleanRelationV1::Cleaner => (candidate.a, candidate.b),
            CleanRelationV1::Dirtier => (candidate.b, candidate.a),
            CleanRelationV1::Incomparable => return false,
        };
        self.reaches(candidate.context, head, tail)
    }

    /// DFS: существует ли Cleaner-путь from → to в контексте.
    fn reaches(&self, context: RelationContextV1, from: VerdictIdV1, to: VerdictIdV1) -> bool {
        let mut stack = vec![from];
        let mut seen = Vec::new();
        while let Some(current) = stack.pop() {
            if current == to {
                return true;
            }
            if seen.contains(&current) {
                continue;
            }
            seen.push(current);
            for e in &self.entries {
                if e.context != context {
                    continue;
                }
                // Каждая запись — ребро «чище → грязнее»; направление зависит
                // от знака отношения, поэтому Dirtier читается наоборот.
                let (edge_from, edge_to) = match e.relation {
                    CleanRelationV1::Cleaner => (e.a, e.b),
                    CleanRelationV1::Dirtier => (e.b, e.a),
                    CleanRelationV1::Incomparable => continue,
                };
                if edge_from == current {
                    stack.push(edge_to);
                }
            }
        }
        false
    }

    /// Отношение a к b в контексте: корпус либо Incomparable по умолчанию
    /// (отсутствие записи — отсутствие отношения, а не равенство).
    #[must_use]
    pub(crate) fn relation(
        &self,
        context: RelationContextV1,
        a: VerdictIdV1,
        b: VerdictIdV1,
    ) -> CleanRelationV1 {
        if a == b {
            return CleanRelationV1::Incomparable;
        }
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        self.entries
            .iter()
            .find(|e| e.context == context && e.a == lo && e.b == hi)
            .map_or(CleanRelationV1::Incomparable, |e| {
                if a <= b {
                    e.relation
                } else {
                    e.relation.inverse()
                }
            })
    }

    /// Валидирует заявленный порядок: каждая соседняя пара обязана быть
    /// сравнима в корпусе в этом контексте и направлена от более чистого
    /// к более грязному. Тотальный порядок над несравнимыми вершинами —
    /// `IncomparablePairForcedIntoTotalOrder`.
    pub(crate) fn validate_claimed_cleaner_to_dirtier_order(
        &self,
        context: RelationContextV1,
        claimed: &[VerdictIdV1],
    ) -> Result<(), RelationCorpusErrorV1> {
        for pair in claimed.windows(2) {
            let [a, b] = [pair[0], pair[1]];
            match self.relation(context, a, b) {
                CleanRelationV1::Cleaner => {}
                CleanRelationV1::Incomparable => {
                    return Err(RelationCorpusErrorV1::IncomparablePairForcedIntoTotalOrder);
                }
                CleanRelationV1::Dirtier => {
                    return Err(RelationCorpusErrorV1::ContradictsCorpus);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(alpha: Option<f64>, anchor: u8) -> RelationContextV1 {
        RelationContextV1 {
            alpha: alpha.map(|a| AdmittedOpacityV1::new(a).expect("valid alpha")),
            backdrop_anchor: [anchor; 32],
        }
    }

    const V0: VerdictIdV1 = VerdictIdV1(0);
    const V1: VerdictIdV1 = VerdictIdV1(1);
    const V2: VerdictIdV1 = VerdictIdV1(2);

    fn chain_corpus() -> RelationCorpusV1 {
        let mut corpus = RelationCorpusV1::new();
        let c = ctx(Some(1.0), 0);
        corpus
            .insert(c, V0, V1, CleanRelationV1::Cleaner)
            .expect("insert v0->v1");
        corpus
            .insert(c, V1, V2, CleanRelationV1::Cleaner)
            .expect("insert v1->v2");
        corpus
    }

    #[test]
    fn chain_is_a_valid_cleaner_to_dirtier_order() {
        let corpus = chain_corpus();
        corpus
            .validate_claimed_cleaner_to_dirtier_order(ctx(Some(1.0), 0), &[V0, V1, V2])
            .expect("total chain must validate");
    }

    #[test]
    fn reversed_order_contradicts_corpus() {
        let corpus = chain_corpus();
        let err = corpus
            .validate_claimed_cleaner_to_dirtier_order(ctx(Some(1.0), 0), &[V2, V1, V0])
            .expect_err("reverse chain must fail");
        assert_eq!(err, RelationCorpusErrorV1::ContradictsCorpus);
    }

    #[test]
    fn incomparable_pair_rejects_total_order() {
        // V0→V1, но V2 не сравним ни с кем: any total order over all three
        // forcibly ranks incomparable vertices — wrong-total-order defect.
        let corpus = chain_corpus();
        let err = corpus
            .validate_claimed_cleaner_to_dirtier_order(
                ctx(Some(1.0), 0),
                &[V0, V1, V2, VerdictIdV1(9)],
            )
            .expect_err("incomparable vertex must break the claimed order");
        assert_eq!(
            err,
            RelationCorpusErrorV1::IncomparablePairForcedIntoTotalOrder
        );
    }

    #[test]
    fn explicit_incomparable_entry_also_blocks_total_order() {
        let mut corpus = chain_corpus();
        let c = ctx(Some(1.0), 0);
        corpus
            .insert(c, V0, VerdictIdV1(9), CleanRelationV1::Incomparable)
            .expect("insert incomparable");
        let err = corpus
            .validate_claimed_cleaner_to_dirtier_order(c, &[V0, VerdictIdV1(9)])
            .expect_err("explicit incomparability must reject ordering");
        assert_eq!(
            err,
            RelationCorpusErrorV1::IncomparablePairForcedIntoTotalOrder
        );
    }

    #[test]
    fn cycle_is_rejected_at_insert() {
        let mut corpus = chain_corpus();
        let c = ctx(Some(1.0), 0);
        let err = corpus
            .insert(c, V2, V0, CleanRelationV1::Cleaner)
            .expect_err("closing the cycle must be rejected");
        assert_eq!(err, RelationCorpusErrorV1::CycleDetected);
    }

    #[test]
    fn inconsistent_matrix_entry_is_rejected() {
        let mut corpus = RelationCorpusV1::new();
        let c = ctx(Some(0.5), 7);
        corpus
            .insert(c, V0, V1, CleanRelationV1::Cleaner)
            .expect("first insert");
        let err = corpus
            .insert(c, V1, V0, CleanRelationV1::Cleaner)
            .expect_err("opposite direction must be a matrix inconsistency");
        assert_eq!(err, RelationCorpusErrorV1::InconsistentMatrix);
    }

    #[test]
    fn different_context_does_not_inherit_relations() {
        let corpus = chain_corpus();
        // Другой фон: записи нет — несравнимо, тотальный порядок запрещён.
        let other = ctx(Some(1.0), 1);
        assert_eq!(
            corpus.relation(other, V0, V1),
            CleanRelationV1::Incomparable
        );
        let err = corpus
            .validate_claimed_cleaner_to_dirtier_order(other, &[V0, V1])
            .expect_err("order validated in a foreign context must fail");
        assert_eq!(
            err,
            RelationCorpusErrorV1::IncomparablePairForcedIntoTotalOrder
        );
    }

    #[test]
    fn different_alpha_does_not_inherit_relations() {
        let corpus = chain_corpus();
        let other = ctx(Some(0.5), 0);
        assert_eq!(
            corpus.relation(other, V0, V1),
            CleanRelationV1::Incomparable
        );
    }

    #[test]
    fn transparent_alpha_is_a_distinct_context_key() {
        let corpus = chain_corpus();
        let transparent = ctx(None, 0);
        assert_eq!(
            corpus.relation(transparent, V0, V1),
            CleanRelationV1::Incomparable
        );
    }

    #[test]
    fn missing_relation_means_incomparable_not_equality() {
        let corpus = chain_corpus();
        assert_eq!(
            corpus.relation(ctx(Some(1.0), 0), V0, VerdictIdV1(9)),
            CleanRelationV1::Incomparable
        );
        assert_eq!(
            corpus.relation(ctx(Some(1.0), 0), V0, V0),
            CleanRelationV1::Incomparable
        );
    }

    #[test]
    fn duplicate_consistent_insert_is_idempotent() {
        let mut corpus = chain_corpus();
        corpus
            .insert(ctx(Some(1.0), 0), V0, V1, CleanRelationV1::Cleaner)
            .expect("consistent duplicate is a no-op");
        assert_eq!(corpus.entries.len(), 2);
    }
}
