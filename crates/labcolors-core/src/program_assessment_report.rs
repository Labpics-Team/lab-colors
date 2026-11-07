//! Owned-проекция финальных оценок Program. Вычисление остаётся у Core evaluator.

use super::ProgramPresentationCompositeV1;
use crate::Srgb8;
use crate::program::VerifiedOutputV1;
use crate::program_session::{
    CoreProgramEvaluatorsV1, CoreProgramPassEvidenceV1, CoreProgramViolationEvidenceV1,
    DeclaredSrgb8CleanSetViolationV1, ProgramConstraintPassEvidenceV1, ProgramConstraintResultV1,
    ProgramConstraintViolationEvidenceV1,
};
use crate::shared::Shared;

/// Сохранённый исход ограничения, без нового вычисления на стороне потребителя.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramAssessmentVerdictV1 {
    Pass,
    Violation,
}

/// Закрытое семейство публичных итогов. Численные внутренности evaluator не
/// становятся обязанностью клиента; Clean Set описывает только точную конвенцию.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramConstraintAssessmentV1 {
    ExactSrgb8 {
        verdict: ProgramAssessmentVerdictV1,
    },
    Wcag22Srgb8 {
        verdict: ProgramAssessmentVerdictV1,
    },
    IntrinsicUnary {
        verdict: ProgramAssessmentVerdictV1,
    },
    Relation {
        verdict: ProgramAssessmentVerdictV1,
    },
    DeclaredSrgb8CleanSetPass {
        visible: Srgb8,
    },
    DeclaredSrgb8CleanSetRejected {
        visible: Srgb8,
        rejected_blue_interval: [u8; 2],
    },
    DeclaredSrgb8CleanSetFinalOwnedDomainAbsent,
}

/// Итог одной клетки case × constraint выбранного или фиксированного состояния.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramAssessmentCellV1 {
    constraint_id: u32,
    case_index: usize,
    hard: bool,
    assessment: ProgramConstraintAssessmentV1,
}

impl ProgramAssessmentCellV1 {
    #[must_use]
    pub const fn constraint_id(self) -> u32 {
        self.constraint_id
    }

    #[must_use]
    pub const fn case_index(self) -> usize {
        self.case_index
    }

    #[must_use]
    pub const fn hard(self) -> bool {
        self.hard
    }

    #[must_use]
    pub const fn assessment(self) -> ProgramConstraintAssessmentV1 {
        self.assessment
    }
}

/// ID исходного сценария, сохранённый после схлопывания физически одинаковых cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgramScenarioProvenanceV1 {
    case_index: usize,
    scenario_id: u32,
}

impl ProgramScenarioProvenanceV1 {
    #[must_use]
    pub const fn case_index(self) -> usize {
        self.case_index
    }

    #[must_use]
    pub const fn scenario_id(self) -> u32 {
        self.scenario_id
    }
}

/// Исторический owned-отчёт одного Ready-снимка, а не capability публикации.
/// Клиент связывает constraint ID со своей декларацией той же content identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramAssessmentReportV1 {
    content_identity: [u8; 32],
    stream_id: u32,
    revision: u64,
    projection: ProgramReadyProjectionV1,
}

// Revision-bound envelope не входит в immutable body. Повторный Ready с теми
// же данными разделяет body, не удерживая observation-lease или Session arena.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ProgramReadyProjectionV1 {
    Inline {
        scenario: Option<ProgramScenarioProvenanceV1>,
        cell: Option<ProgramAssessmentCellV1>,
        composite: Option<ProgramPresentationCompositeV1>,
    },
    Shared(Shared<ProgramReadyProjectionBodyV1>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProgramReadyProjectionBodyV1 {
    scenario_provenance: Vec<ProgramScenarioProvenanceV1>,
    cells: Vec<ProgramAssessmentCellV1>,
    composites: Vec<ProgramPresentationCompositeV1>,
}

impl ProgramReadyProjectionV1 {
    pub(super) fn scenario_provenance(&self) -> &[ProgramScenarioProvenanceV1] {
        match self {
            Self::Inline { scenario, .. } => scenario.as_slice(),
            Self::Shared(body) => &body.scenario_provenance,
        }
    }

    pub(super) fn cells(&self) -> &[ProgramAssessmentCellV1] {
        match self {
            Self::Inline { cell, .. } => cell.as_slice(),
            Self::Shared(body) => &body.cells,
        }
    }

    pub(super) fn composites(&self) -> &[ProgramPresentationCompositeV1] {
        match self {
            Self::Inline { composite, .. } => composite.as_slice(),
            Self::Shared(body) => &body.composites,
        }
    }

    #[cfg(test)]
    pub(super) fn composites_mut_for_test(&mut self) -> &mut Vec<ProgramPresentationCompositeV1> {
        if let Self::Inline {
            scenario,
            cell,
            composite,
        } = self
        {
            *self = Self::Shared(Shared::new(ProgramReadyProjectionBodyV1 {
                scenario_provenance: scenario.as_slice().to_vec(),
                cells: cell.as_slice().to_vec(),
                composites: composite.as_slice().to_vec(),
            }));
        }
        let Self::Shared(body) = self else {
            unreachable!("the inline projection was converted to an owned test body");
        };
        &mut Shared::make_mut(body).composites
    }
}

impl ProgramAssessmentReportV1 {
    #[must_use]
    pub const fn content_identity(&self) -> [u8; 32] {
        self.content_identity
    }

    #[must_use]
    pub const fn stream_id(&self) -> u32 {
        self.stream_id
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn scenario_provenance(&self) -> &[ProgramScenarioProvenanceV1] {
        self.projection.scenario_provenance()
    }

    #[must_use]
    pub fn cells(&self) -> &[ProgramAssessmentCellV1] {
        self.projection.cells()
    }

    pub(super) fn from_projection(
        verified: VerifiedOutputV1<'_>,
        projection: ProgramReadyProjectionV1,
    ) -> Self {
        let observation = verified.observation();
        Self {
            content_identity: *verified.content_identity().as_bytes(),
            stream_id: observation.stream().value(),
            revision: observation.revision(),
            projection,
        }
    }
}

#[derive(Default)]
pub(super) struct ProgramAssessmentReportScratchV1 {
    scenario_provenance: Vec<ProgramScenarioProvenanceV1>,
    cells: Vec<ProgramAssessmentCellV1>,
}

impl ProgramAssessmentReportScratchV1 {
    pub(super) fn clear(&mut self) {
        self.scenario_provenance.clear();
        self.cells.clear();
    }

    /// Резервирование завершается до атомарного перехода Session или sink.
    pub(super) fn reserve(&mut self, cells: usize, scenarios: usize) -> Result<(), ()> {
        self.clear();
        self.cells.try_reserve_exact(cells).map_err(|_| ())?;
        self.scenario_provenance
            .try_reserve_exact(scenarios)
            .map_err(|_| ())
    }

    /// Подготовка выполняется на validated prospective evidence до commit.
    /// Scratch сохраняет capacity; только новое неодиночное body требует owned memory.
    pub(super) fn project_verified(
        &mut self,
        verified: VerifiedOutputV1<'_>,
        composites: &[ProgramPresentationCompositeV1],
        cached: Option<&ProgramReadyProjectionV1>,
    ) -> Result<ProgramReadyProjectionV1, ()> {
        self.clear();
        let observation = verified.observation();
        let mut provenance_count = 0_usize;
        for case in observation.physical_cases() {
            provenance_count = provenance_count
                .checked_add(case.provenance().count())
                .ok_or(())?;
        }
        self.scenario_provenance
            .try_reserve_exact(provenance_count)
            .map_err(|_| ())?;
        self.cells
            .try_reserve_exact(verified.cells().len())
            .map_err(|_| ())?;
        for (case_index, case) in observation.physical_cases().enumerate() {
            self.scenario_provenance
                .extend(
                    case.provenance()
                        .map(|scenario| ProgramScenarioProvenanceV1 {
                            case_index,
                            scenario_id: scenario.value(),
                        }),
                );
        }
        self.cells
            .extend(verified.cells().map(|cell| ProgramAssessmentCellV1 {
                constraint_id: cell.constraint().value(),
                case_index: cell.case_index(),
                hard: cell.is_hard(),
                assessment: copy_assessment(cell.result()),
            }));
        if let Some(cached) = cached {
            if cached.scenario_provenance() == self.scenario_provenance
                && cached.cells() == self.cells
                && cached.composites() == composites
            {
                return Ok(cached.clone());
            }
        }
        if self.scenario_provenance.len() <= 1 && self.cells.len() <= 1 && composites.len() <= 1 {
            return Ok(ProgramReadyProjectionV1::Inline {
                scenario: self.scenario_provenance.first().copied(),
                cell: self.cells.first().copied(),
                composite: composites.first().copied(),
            });
        }
        Ok(ProgramReadyProjectionV1::Shared(Shared::new(
            ProgramReadyProjectionBodyV1 {
                scenario_provenance: try_copy(&self.scenario_provenance)?,
                cells: try_copy(&self.cells)?,
                composites: try_copy(composites)?,
            },
        )))
    }
}

fn try_copy<T: Copy>(values: &[T]) -> Result<Vec<T>, ()> {
    let mut owned = Vec::new();
    owned.try_reserve_exact(values.len()).map_err(|_| ())?;
    owned.extend_from_slice(values);
    Ok(owned)
}

fn copy_assessment(
    assessment: &ProgramConstraintResultV1<CoreProgramEvaluatorsV1>,
) -> ProgramConstraintAssessmentV1 {
    let verdict = if assessment.is_violation() {
        ProgramAssessmentVerdictV1::Violation
    } else {
        ProgramAssessmentVerdictV1::Pass
    };
    match assessment {
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::VisibleUnary(
            CoreProgramPassEvidenceV1::ExactSrgb8(_),
        ))
        | ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::VisibleUnary(
                CoreProgramViolationEvidenceV1::ExactSrgb8(_),
            ),
        ) => ProgramConstraintAssessmentV1::ExactSrgb8 { verdict },
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::VisibleUnary(
            CoreProgramPassEvidenceV1::Wcag22Srgb8(_),
        ))
        | ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::VisibleUnary(
                CoreProgramViolationEvidenceV1::Wcag22Srgb8(_),
            ),
        ) => ProgramConstraintAssessmentV1::Wcag22Srgb8 { verdict },
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::IntrinsicUnary(_))
        | ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::IntrinsicUnary(_),
        ) => ProgramConstraintAssessmentV1::IntrinsicUnary { verdict },
        ProgramConstraintResultV1::Pass(ProgramConstraintPassEvidenceV1::Relation(_))
        | ProgramConstraintResultV1::Violation(ProgramConstraintViolationEvidenceV1::Relation(_)) => {
            ProgramConstraintAssessmentV1::Relation { verdict }
        }
        ProgramConstraintResultV1::Pass(
            ProgramConstraintPassEvidenceV1::DeclaredSrgb8CleanSet(value),
        ) => ProgramConstraintAssessmentV1::DeclaredSrgb8CleanSetPass {
            visible: value.visible(),
        },
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::DeclaredSrgb8CleanSet(
                DeclaredSrgb8CleanSetViolationV1::Rejected {
                    visible,
                    rejected_blue_interval,
                },
            ),
        ) => ProgramConstraintAssessmentV1::DeclaredSrgb8CleanSetRejected {
            visible: *visible,
            rejected_blue_interval: rejected_blue_interval.endpoints(),
        },
        ProgramConstraintResultV1::Violation(
            ProgramConstraintViolationEvidenceV1::DeclaredSrgb8CleanSet(
                DeclaredSrgb8CleanSetViolationV1::FinalOwnedDomainAbsent,
            ),
        ) => ProgramConstraintAssessmentV1::DeclaredSrgb8CleanSetFinalOwnedDomainAbsent,
    }
}
