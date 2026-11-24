//! Типизированные нативные копии уже вычисленных Core-оценок и witness.

use labcolors_core::program_wire as core;

use crate::LabColorsError;

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProgramFieldWitness {
    pub numerator: u16,
    pub denominator: u16,
    pub nominal: ProgramRgb8,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProgramFieldAssessment {
    pub input_index: u32,
    pub from: ProgramRgb8,
    pub to: ProgramRgb8,
    pub source_model: String,
    pub domain: String,
    pub rendering: String,
    pub region_count: u32,
    pub witnesses: Vec<ProgramFieldWitness>,
}

pub(crate) fn field_assessment(
    field: &core::ProgramFieldAssessmentV1,
) -> Result<ProgramFieldAssessment, LabColorsError> {
    let operation = "fieldAssessment";
    let mut witnesses = storage(field.witnesses().len(), operation)?;
    for witness in field.witnesses() {
        let (numerator, denominator) = witness.coordinate();
        witnesses.push(ProgramFieldWitness {
            numerator,
            denominator,
            nominal: witness.nominal().into(),
        });
    }
    let (from, to) = field.endpoints();
    let internal = |_| LabColorsError::new("program_internal", operation);
    Ok(ProgramFieldAssessment {
        input_index: u32::try_from(field.input_index()).map_err(internal)?,
        from: from.into(),
        to: to.into(),
        source_model: field.source_model().to_owned(),
        domain: field.domain().to_owned(),
        rendering: field.rendering().to_owned(),
        region_count: u32::try_from(field.region_count()).map_err(internal)?,
        witnesses,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ProgramTonalPolarity {
    Darker,
    Equal,
    Lighter,
}

impl From<core::TonalPolarityV1> for ProgramTonalPolarity {
    fn from(value: core::TonalPolarityV1) -> Self {
        match value {
            core::TonalPolarityV1::Darker => Self::Darker,
            core::TonalPolarityV1::Equal => Self::Equal,
            core::TonalPolarityV1::Lighter => Self::Lighter,
        }
    }
}

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
        distance_lower: f64,
        distance_upper: f64,
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
    NominalSrgb8VisiblePass {
        visible: ProgramRgb8,
    },
    NominalSrgb8VisibleRejected {
        visible: ProgramRgb8,
        rejected_blue_lower: u8,
        rejected_blue_upper: u8,
    },
    TonalHierarchy {
        verdict: ProgramAssessmentVerdict,
    },
}

impl From<core::ProgramConstraintAssessmentV1> for ProgramConstraintAssessment {
    fn from(value: core::ProgramConstraintAssessmentV1) -> Self {
        use core::ProgramConstraintAssessmentV1 as C;
        match value {
            C::NominalSrgb8VisiblePass { visible } => Self::NominalSrgb8VisiblePass {
                visible: visible.into(),
            },
            C::NominalSrgb8VisibleRejected {
                visible,
                rejected_blue_interval,
            } => Self::NominalSrgb8VisibleRejected {
                visible: visible.into(),
                rejected_blue_lower: rejected_blue_interval[0],
                rejected_blue_upper: rejected_blue_interval[1],
            },
            C::VisibleSeparation {
                verdict,
                measurement,
            } => Self::VisibleSeparation {
                verdict: verdict.into(),
                visible: measurement.visible().into(),
                backdrop: measurement.backdrop().into(),
                distance: measurement.distance(),
                distance_lower: measurement.distance_bounds()[0],
                distance_upper: measurement.distance_bounds()[1],
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
            C::TonalHierarchy { verdict } => Self::TonalHierarchy {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ProgramCleanlinessStatus {
    NotAssessed,
    Conforms,
    Violates,
    Unavailable,
}

/// Сводка уже сохранённых Core-оценок; счётчики не являются оценкой качества.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ProgramCleanlinessAssessment {
    pub status: ProgramCleanlinessStatus,
    /// Отсутствует при NotAssessed: профиль не объявлен.
    pub profile: Option<String>,
    pub basis: Option<String>,
    pub scope: Option<String>,
    pub accepted: u64,
    pub rejected: u64,
    pub unavailable: u64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProgramAssessmentReport {
    pub content_identity: Vec<u8>,
    pub stream_id: u32,
    pub revision: u64,
    pub scenario_provenance: Vec<ProgramScenarioProvenance>,
    pub cells: Vec<ProgramAssessmentCell>,
    pub cleanliness: ProgramCleanlinessAssessment,
    pub tonal_hierarchy: Vec<ProgramTonalHierarchyPair>,
    pub reference_selections: Vec<ProgramReferenceSelection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ProgramReferencePreference {
    ReferenceStrength,
    ExplicitTone,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ProgramReferenceCapacity {
    pub ratio_lower: f64,
    pub ratio_upper: f64,
    pub complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ProgramReferenceCase {
    pub case_index: u64,
    pub reference_visible: ProgramRgb8,
    pub reference_backdrop: ProgramRgb8,
    pub actual_visible: ProgramRgb8,
    pub actual_backdrop: ProgramRgb8,
    pub reference_contrast: f64,
    pub actual_contrast: f64,
    pub polarity: ProgramTonalPolarity,
    pub strength_loss: f64,
    pub virtual_distance_squared: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProgramReferenceSelection {
    pub target_id: u32,
    pub occurrence_id: u32,
    pub preference: ProgramReferencePreference,
    pub capacity: Option<ProgramReferenceCapacity>,
    pub cases: Vec<ProgramReferenceCase>,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct ProgramTonalHierarchyPair {
    pub constraint_id: u32,
    pub case_index: u64,
    pub stronger_occurrence_id: u32,
    pub weaker_occurrence_id: u32,
    pub required_ratio: f64,
    pub actual_ratio: f64,
    pub required_gap: f64,
    pub actual_gap: f64,
    pub stronger_polarity: ProgramTonalPolarity,
    pub weaker_polarity: ProgramTonalPolarity,
    pub satisfied: bool,
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
    ComputedVisibleRelation,
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
            C::ComputedVisibleRelation => Self::ComputedVisibleRelation,
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
    TonalHierarchy {
        constraint_id: u32,
        case_index: u64,
        stronger_occurrence_id: u32,
        weaker_occurrence_id: u32,
        required_ratio: f64,
        actual_ratio: f64,
        required_gap: f64,
        actual_gap: f64,
        stronger_polarity: ProgramTonalPolarity,
        weaker_polarity: ProgramTonalPolarity,
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
            C::TonalHierarchy {
                constraint_id,
                case_index,
                stronger_occurrence_id,
                weaker_occurrence_id,
                required_ratio,
                actual_ratio,
                required_gap,
                actual_gap,
                stronger_polarity,
                weaker_polarity,
            } => Self::TonalHierarchy {
                constraint_id,
                case_index: case_index as u64,
                stronger_occurrence_id,
                weaker_occurrence_id,
                required_ratio,
                actual_ratio,
                required_gap,
                actual_gap,
                stronger_polarity: stronger_polarity.into(),
                weaker_polarity: weaker_polarity.into(),
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

fn copy_text(source: &str, operation: &str) -> Result<String, LabColorsError> {
    let mut output = String::new();
    output
        .try_reserve_exact(source.len())
        .map_err(|_| LabColorsError::new("program_resource_exhausted", operation))?;
    output.push_str(source);
    Ok(output)
}

pub(crate) fn cleanliness_assessment(
    assessment: core::ProgramCleanlinessAssessmentV1,
    operation: &str,
) -> Result<ProgramCleanlinessAssessment, LabColorsError> {
    use core::{
        ProgramCleanlinessAssessmentV1 as Assessment, ProgramCleanlinessStatusV1 as Status,
    };
    let status = match assessment.status() {
        Status::NotAssessed => ProgramCleanlinessStatus::NotAssessed,
        Status::Conforms => ProgramCleanlinessStatus::Conforms,
        Status::Violates => ProgramCleanlinessStatus::Violates,
        Status::Unavailable => ProgramCleanlinessStatus::Unavailable,
    };
    let declared = status != ProgramCleanlinessStatus::NotAssessed;
    let metadata = |value| declared.then(|| copy_text(value, operation)).transpose();
    Ok(ProgramCleanlinessAssessment {
        status,
        profile: metadata(Assessment::PROFILE_ID)?,
        basis: metadata(Assessment::BASIS)?,
        scope: metadata(Assessment::SCOPE)?,
        accepted: assessment.accepted() as u64,
        rejected: assessment.rejected() as u64,
        unavailable: assessment.unavailable() as u64,
    })
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
    let mut tonal_hierarchy = storage(report.tonal_hierarchy().len(), operation)?;
    tonal_hierarchy.extend(report.tonal_hierarchy().iter().map(|pair| {
        let measurement = pair.measurement();
        ProgramTonalHierarchyPair {
            constraint_id: pair.constraint_id(),
            case_index: pair.case_index() as u64,
            stronger_occurrence_id: pair.stronger_occurrence_id(),
            weaker_occurrence_id: pair.weaker_occurrence_id(),
            required_ratio: measurement.required_ratio(),
            actual_ratio: measurement.actual_ratio(),
            required_gap: measurement.required_gap(),
            actual_gap: measurement.actual_gap(),
            stronger_polarity: measurement.stronger_polarity().into(),
            weaker_polarity: measurement.weaker_polarity().into(),
            satisfied: measurement.satisfied(),
        }
    }));
    Ok(Some(ProgramAssessmentReport {
        content_identity: copy_slice(&report.content_identity(), operation)?,
        stream_id: report.stream_id(),
        revision: report.revision(),
        scenario_provenance,
        cells,
        cleanliness: cleanliness_assessment(report.cleanliness(), operation)?,
        tonal_hierarchy,
        reference_selections: reference_selections(report.reference_selections(), operation)?,
    }))
}

pub(crate) fn reference_selections(
    selections: &[core::ProgramReferenceSelectionV1],
    operation: &str,
) -> Result<Vec<ProgramReferenceSelection>, LabColorsError> {
    let mut reference_selections = storage(selections.len(), operation)?;
    for selection in selections {
        let mut cases = storage(selection.cases().len(), operation)?;
        cases.extend(selection.cases().iter().map(|case| ProgramReferenceCase {
            case_index: case.case_index() as u64,
            reference_visible: case.reference_visible().into(),
            reference_backdrop: case.reference_backdrop().into(),
            actual_visible: case.actual_visible().into(),
            actual_backdrop: case.actual_backdrop().into(),
            reference_contrast: case.reference_contrast(),
            actual_contrast: case.actual_contrast(),
            polarity: case.polarity().into(),
            strength_loss: case.strength_loss(),
            virtual_distance_squared: case.virtual_distance_squared(),
        }));
        let preference = match selection.preference() {
            core::ProgramReferencePreferenceV1::ReferenceStrength => {
                ProgramReferencePreference::ReferenceStrength
            }
            core::ProgramReferencePreferenceV1::ExplicitTone => {
                ProgramReferencePreference::ExplicitTone
            }
        };
        reference_selections.push(ProgramReferenceSelection {
            target_id: selection.target_id(),
            occurrence_id: selection.occurrence_id(),
            preference,
            capacity: selection
                .capacity()
                .map(|capacity| ProgramReferenceCapacity {
                    ratio_lower: capacity.ratio_lower(),
                    ratio_upper: capacity.ratio_upper(),
                    complete: capacity.complete(),
                }),
            cases,
        });
    }
    Ok(reference_selections)
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
