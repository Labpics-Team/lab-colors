//! Типизированные нативные копии уже вычисленных Core-оценок и witness.

use labcolors_core::program_wire as core;

use crate::LabColorsError;

/// Три канала конечного sRGB8 без отдельной heap-аллокации на каждый цвет.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ProgramRgb8 {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl From<labcolors_core::Srgb8> for ProgramRgb8 {
    fn from(value: labcolors_core::Srgb8) -> Self {
        let [red, green, blue] = value.bytes();
        Self { red, green, blue }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ProgramAssessmentVerdict {
    Pass,
    Violation,
}

impl From<core::ProgramAssessmentVerdictV1> for ProgramAssessmentVerdict {
    fn from(value: core::ProgramAssessmentVerdictV1) -> Self {
        match value {
            core::ProgramAssessmentVerdictV1::Pass => Self::Pass,
            core::ProgramAssessmentVerdictV1::Violation => Self::Violation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum ProgramConstraintAssessment {
    VisibleSeparation {
        verdict: ProgramAssessmentVerdict,
        visible: ProgramRgb8,
        backdrop: ProgramRgb8,
        distance: f64,
        minimum: f64,
        maximum: f64,
    },
    ExactSrgb8 {
        verdict: ProgramAssessmentVerdict,
    },
    Wcag22Srgb8 {
        verdict: ProgramAssessmentVerdict,
    },
    IntrinsicUnary {
        verdict: ProgramAssessmentVerdict,
    },
    Relation {
        verdict: ProgramAssessmentVerdict,
    },
    DeclaredSrgb8CleanSetPass {
        visible: ProgramRgb8,
    },
    DeclaredSrgb8CleanSetRejected {
        visible: ProgramRgb8,
        rejected_blue_lower: u8,
        rejected_blue_upper: u8,
    },
    DeclaredSrgb8CleanSetFinalOwnedDomainAbsent,
}

impl From<core::ProgramConstraintAssessmentV1> for ProgramConstraintAssessment {
    fn from(value: core::ProgramConstraintAssessmentV1) -> Self {
        use core::ProgramConstraintAssessmentV1 as C;
        match value {
            C::VisibleSeparation {
                verdict,
                measurement,
            } => Self::VisibleSeparation {
                verdict: verdict.into(),
                visible: measurement.visible().into(),
                backdrop: measurement.backdrop().into(),
                distance: measurement.distance(),
                minimum: measurement.minimum(),
                maximum: measurement.maximum(),
            },
            C::ExactSrgb8 { verdict } => Self::ExactSrgb8 {
                verdict: verdict.into(),
            },
            C::Wcag22Srgb8 { verdict } => Self::Wcag22Srgb8 {
                verdict: verdict.into(),
            },
            C::IntrinsicUnary { verdict } => Self::IntrinsicUnary {
                verdict: verdict.into(),
            },
            C::Relation { verdict } => Self::Relation {
                verdict: verdict.into(),
            },
            C::DeclaredSrgb8CleanSetPass { visible } => Self::DeclaredSrgb8CleanSetPass {
                visible: visible.into(),
            },
            C::DeclaredSrgb8CleanSetRejected {
                visible,
                rejected_blue_interval,
            } => Self::DeclaredSrgb8CleanSetRejected {
                visible: visible.into(),
                rejected_blue_lower: rejected_blue_interval[0],
                rejected_blue_upper: rejected_blue_interval[1],
            },
            C::DeclaredSrgb8CleanSetFinalOwnedDomainAbsent => {
                Self::DeclaredSrgb8CleanSetFinalOwnedDomainAbsent
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProgramAssessmentCell {
    pub constraint_id: u32,
    pub case_index: u64,
    pub hard: bool,
    pub assessment: ProgramConstraintAssessment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ProgramScenarioProvenance {
    pub case_index: u64,
    pub scenario_id: u32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProgramAssessmentReport {
    pub content_identity: Vec<u8>,
    pub stream_id: u32,
    pub revision: u64,
    pub scenario_provenance: Vec<ProgramScenarioProvenance>,
    pub cells: Vec<ProgramAssessmentCell>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ProgramDiagnosticSubject {
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

impl From<core::ProgramDiagnosticSubjectV1> for ProgramDiagnosticSubject {
    fn from(value: core::ProgramDiagnosticSubjectV1) -> Self {
        use core::ProgramDiagnosticSubjectV1 as C;
        match value {
            C::Target { target_id } => Self::Target { target_id },
            C::Occurrence { occurrence_id } => Self::Occurrence { occurrence_id },
            C::IntrinsicRelation {
                reference_target_id,
            } => Self::IntrinsicRelation {
                reference_target_id,
            },
            C::VisibleRelation {
                reference_occurrence_id,
            } => Self::VisibleRelation {
                reference_occurrence_id,
            },
            C::Presentation {
                root_id,
                occurrence_id,
                terminal_id,
            } => Self::Presentation {
                root_id,
                occurrence_id,
                terminal_id,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum ProgramDiagnostic {
    HardConstraint {
        constraint_id: u32,
        case_index: u64,
        subject: ProgramDiagnosticSubject,
    },
    TargetDomain {
        target_id: u32,
        source: ProgramRgb8,
    },
    ColorIdentity {
        target_id: u32,
        case_index: u64,
        occurrence_id: Option<u32>,
        actual: ProgramRgb8,
    },
    MaxChange {
        target_id: u32,
        case_index: u64,
        occurrence_id: Option<u32>,
        actual: ProgramRgb8,
        distance_squared: f64,
        maximum_distance: f64,
    },
}

impl From<core::ProgramDiagnosticV1> for ProgramDiagnostic {
    fn from(value: core::ProgramDiagnosticV1) -> Self {
        use core::ProgramDiagnosticV1 as C;
        match value {
            C::HardConstraint {
                constraint_id,
                case_index,
                subject,
            } => Self::HardConstraint {
                constraint_id,
                case_index: case_index as u64,
                subject: subject.into(),
            },
            C::TargetDomain { target_id, source } => Self::TargetDomain {
                target_id,
                source: source.into(),
            },
            C::ColorIdentity {
                target_id,
                case_index,
                occurrence_id,
                actual,
            } => Self::ColorIdentity {
                target_id,
                case_index: case_index as u64,
                occurrence_id,
                actual: actual.into(),
            },
            C::MaxChange {
                target_id,
                case_index,
                occurrence_id,
                actual,
                distance_squared,
                maximum_distance,
            } => Self::MaxChange {
                target_id,
                case_index: case_index as u64,
                occurrence_id,
                actual: actual.into(),
                distance_squared,
                maximum_distance,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Record)]
pub struct ProgramAttemptTarget {
    pub target_id: u32,
    pub source: ProgramRgb8,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProgramAttemptWitness {
    pub case_scenario_ids: Vec<Vec<u32>>,
    pub targets: Vec<ProgramAttemptTarget>,
    pub violations: Vec<ProgramDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProgramDiagnostics {
    pub search_visited: u64,
    pub search_budget: u64,
    pub witness: Option<ProgramAttemptWitness>,
}

fn storage<T>(length: usize, operation: &str) -> Result<Vec<T>, LabColorsError> {
    let mut output = Vec::new();
    output
        .try_reserve_exact(length)
        .map_err(|_| LabColorsError::new("program_resource_exhausted", operation))?;
    Ok(output)
}

fn copy_slice<T: Copy>(source: &[T], operation: &str) -> Result<Vec<T>, LabColorsError> {
    let mut output = storage(source.len(), operation)?;
    output.extend_from_slice(source);
    Ok(output)
}

pub(crate) fn assessment_report(
    report: Option<&core::ProgramAssessmentReportV1>,
) -> Result<Option<ProgramAssessmentReport>, LabColorsError> {
    let Some(report) = report else {
        return Ok(None);
    };
    let operation = "assessmentReport";
    let mut scenario_provenance = storage(report.scenario_provenance().len(), operation)?;
    scenario_provenance.extend(report.scenario_provenance().iter().map(|value| {
        ProgramScenarioProvenance {
            case_index: value.case_index() as u64,
            scenario_id: value.scenario_id(),
        }
    }));
    let mut cells = storage(report.cells().len(), operation)?;
    cells.extend(report.cells().iter().map(|value| ProgramAssessmentCell {
        constraint_id: value.constraint_id(),
        case_index: value.case_index() as u64,
        hard: value.hard(),
        assessment: value.assessment().into(),
    }));
    Ok(Some(ProgramAssessmentReport {
        content_identity: copy_slice(&report.content_identity(), operation)?,
        stream_id: report.stream_id(),
        revision: report.revision(),
        scenario_provenance,
        cells,
    }))
}

pub(crate) fn diagnostics(
    value: Option<&core::ProgramDiagnosticsV1>,
) -> Result<Option<ProgramDiagnostics>, LabColorsError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let operation = "diagnostics";
    let witness = value
        .witness()
        .map(|witness| -> Result<_, LabColorsError> {
            let mut case_scenario_ids = storage(witness.case_scenario_ids().len(), operation)?;
            for case in witness.case_scenario_ids() {
                case_scenario_ids.push(copy_slice(case, operation)?);
            }
            let mut targets = storage(witness.targets().len(), operation)?;
            targets.extend(witness.targets().iter().map(|target| ProgramAttemptTarget {
                target_id: target.target_id(),
                source: target.source().into(),
            }));
            let mut violations = storage(witness.violations().len(), operation)?;
            violations.extend(witness.violations().iter().map(|value| (*value).into()));
            Ok(ProgramAttemptWitness {
                case_scenario_ids,
                targets,
                violations,
            })
        })
        .transpose()?;
    Ok(Some(ProgramDiagnostics {
        search_visited: value.search_visited(),
        search_budget: value.search_budget(),
        witness,
    }))
}
