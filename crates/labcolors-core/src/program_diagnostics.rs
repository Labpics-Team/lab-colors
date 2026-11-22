//! Data-only свидетельство конкретного отвергнутого авторского tuple.
//! Оно не является минимальным конфликтом или разрешением публиковать Paint.

use crate::Srgb8;

/// Физический объект нарушенного hard constraint. Идентификаторы принадлежат
/// исходной декларации; relation reference не подменяет всех её участников.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramDiagnosticSubjectV1 {
    Target {
        target_id: u32,
    },
    Occurrence {
        occurrence_id: u32,
    },
    IntrinsicRelation {
        reference_target_id: u32,
    },
    VisibleRelation {
        reference_occurrence_id: u32,
    },
    Presentation {
        root_id: u32,
        occurrence_id: u32,
        terminal_id: u32,
    },
}

/// Одно установленное нарушение конкретного attempt. HardConstraint связывает
/// authored requirement по ID; остальные варианты — условия самого target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProgramDiagnosticV1 {
    HardConstraint {
        constraint_id: u32,
        case_index: usize,
        subject: ProgramDiagnosticSubjectV1,
    },
    TargetDomain {
        target_id: u32,
        source: Srgb8,
    },
    ColorIdentity {
        target_id: u32,
        case_index: usize,
        occurrence_id: Option<u32>,
        actual: Srgb8,
    },
    MaxChange {
        target_id: u32,
        case_index: usize,
        occurrence_id: Option<u32>,
        actual: Srgb8,
        distance_squared: f64,
        maximum_distance: f64,
    },
}

/// Исходный RGB одного изменяемого target в диагностическом tuple.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramAttemptTargetV1 {
    pub(crate) target_id: u32,
    pub(crate) source: Srgb8,
}

impl ProgramAttemptTargetV1 {
    #[must_use]
    pub const fn target_id(self) -> u32 {
        self.target_id
    }
    #[must_use]
    pub const fn source(self) -> Srgb8 {
        self.source
    }
}

/// Свежая проверка authored source tuple на текущих физических cases.
/// Непустой список violations объясняет только этот attempt; другие значения
/// исчерпаны или ещё не исследованы согласно отдельному lifecycle результата.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgramAttemptWitnessV1 {
    pub(crate) case_scenario_ids: Vec<Vec<u32>>,
    pub(crate) targets: Vec<ProgramAttemptTargetV1>,
    pub(crate) violations: Vec<ProgramDiagnosticV1>,
}

impl ProgramAttemptWitnessV1 {
    /// Каждый внутренний массив — исходные scenario IDs одного canonical case.
    #[must_use]
    pub fn case_scenario_ids(&self) -> &[Vec<u32>] {
        &self.case_scenario_ids
    }
    #[must_use]
    pub fn targets(&self) -> &[ProgramAttemptTargetV1] {
        &self.targets
    }
    #[must_use]
    pub fn violations(&self) -> &[ProgramDiagnosticV1] {
        &self.violations
    }
}

/// Диагностика Unsatisfied/Incomplete. До первой попытки вычисляемого tuple
/// witness отсутствует. Условия без вычисляемых зависимостей проверяются
/// отдельно и могут дать свидетельство конфликта даже при нулевом бюджете
/// поиска. Дополнительная проверка witness не считается search visit.
#[derive(Debug, Clone, PartialEq)]
pub struct ProgramDiagnosticsV1 {
    pub(crate) search_visited: u64,
    pub(crate) search_budget: u64,
    pub(crate) witness: Option<ProgramAttemptWitnessV1>,
}

impl ProgramDiagnosticsV1 {
    #[must_use]
    pub const fn search_visited(&self) -> u64 {
        self.search_visited
    }
    #[must_use]
    pub const fn search_budget(&self) -> u64 {
        self.search_budget
    }
    #[must_use]
    pub const fn witness(&self) -> Option<&ProgramAttemptWitnessV1> {
        self.witness.as_ref()
    }

    /// Snapshot получает независимые буферы до commit подготовленного перехода.
    pub(crate) fn try_copy(&self) -> Result<Self, ()> {
        let witness = self
            .witness
            .as_ref()
            .map(|source| -> Result<_, ()> {
                let mut case_scenario_ids = Vec::new();
                case_scenario_ids
                    .try_reserve_exact(source.case_scenario_ids.len())
                    .map_err(|_| ())?;
                for case in &source.case_scenario_ids {
                    case_scenario_ids.push(copy_slice(case)?);
                }
                Ok(ProgramAttemptWitnessV1 {
                    case_scenario_ids,
                    targets: copy_slice(&source.targets)?,
                    violations: copy_slice(&source.violations)?,
                })
            })
            .transpose()?;
        Ok(Self {
            search_visited: self.search_visited,
            search_budget: self.search_budget,
            witness,
        })
    }
}

fn copy_slice<T: Copy>(source: &[T]) -> Result<Vec<T>, ()> {
    let mut result = Vec::new();
    result.try_reserve_exact(source.len()).map_err(|_| ())?;
    result.extend_from_slice(source);
    Ok(result)
}
