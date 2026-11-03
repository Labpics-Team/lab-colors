//! Owned-проекция финальных оценок Program. Вычисление остаётся у Core evaluator.

use crate::Srgb8;
use crate::program::VerifiedOutputV1;
use crate::program_session::{
    CoreProgramEvaluatorsV1, CoreProgramPassEvidenceV1, CoreProgramViolationEvidenceV1,
    DeclaredSrgb8CleanSetViolationV1, ProgramConstraintPassEvidenceV1, ProgramConstraintResultV1,
    ProgramConstraintViolationEvidenceV1,
};

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
    scenario_provenance: Vec<ProgramScenarioProvenanceV1>,
    cells: Vec<ProgramAssessmentCellV1>,
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
        &self.scenario_provenance
    }

    #[must_use]
    pub fn cells(&self) -> &[ProgramAssessmentCellV1] {
        &self.cells
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

    /// Ёмкости ограничены prospective bound владельца и числом входных сценариев.
    /// Копирование не аллоцирует, не переоценивает и не меняет выбранные Paint.
    pub(super) fn copy_verified(
        &mut self,
        verified: VerifiedOutputV1<'_>,
    ) -> ProgramAssessmentReportV1 {
        let observation = verified.observation();
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
        ProgramAssessmentReportV1 {
            content_identity: *verified.content_identity().as_bytes(),
            stream_id: observation.stream().value(),
            revision: observation.revision(),
            scenario_provenance: std::mem::take(&mut self.scenario_provenance),
            cells: std::mem::take(&mut self.cells),
        }
    }
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
