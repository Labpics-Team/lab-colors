//! Структурная JS-проекция owned-отчёта. Ни классификатора, ни solver здесь нет.

use labcolors_core::program_wire::{
    ProgramAssessmentReportV1, ProgramAssessmentVerdictV1, ProgramConstraintAssessmentV1,
};
use wasm_bindgen::prelude::*;

use super::{
    report_accepted, report_array, report_array_push, report_cell, report_domain_absent,
    report_projection, report_rejected, report_scenario, report_summary,
};

/// Неудачное чтение не изменяет Rust snapshot и не имеет sink-эффекта.
pub(super) fn project(report: Option<&ProgramAssessmentReportV1>) -> Result<JsValue, JsValue> {
    let Some(report) = report else {
        return Ok(JsValue::NULL);
    };
    let scenario_provenance = report_array()?;
    for scenario in report.scenario_provenance() {
        let record = report_scenario(scenario.case_index(), scenario.scenario_id())?;
        report_array_push(&scenario_provenance, &record)?;
    }
    let cells = report_array()?;
    for cell in report.cells() {
        let assessment = project_assessment(cell.assessment())?;
        let record = report_cell(
            cell.constraint_id(),
            cell.case_index(),
            cell.hard(),
            &assessment,
        )?;
        report_array_push(&cells, &record)?;
    }
    report_projection(
        &report.content_identity(),
        report.stream_id(),
        report.revision(),
        &scenario_provenance,
        &cells,
    )
}

fn project_assessment(assessment: ProgramConstraintAssessmentV1) -> Result<JsValue, JsValue> {
    use ProgramConstraintAssessmentV1 as Assessment;
    let verdict_key = |verdict| match verdict {
        ProgramAssessmentVerdictV1::Pass => "pass",
        ProgramAssessmentVerdictV1::Violation => "violation",
    };
    match assessment {
        Assessment::ExactSrgb8 { verdict } => report_summary("exact-srgb8", verdict_key(verdict)),
        Assessment::Wcag22Srgb8 { verdict } => report_summary("wcag22-srgb8", verdict_key(verdict)),
        Assessment::IntrinsicUnary { verdict } => {
            report_summary("intrinsic-unary", verdict_key(verdict))
        }
        Assessment::Relation { verdict } => report_summary("relation", verdict_key(verdict)),
        Assessment::DeclaredSrgb8CleanSetPass { visible } => {
            let [red, green, blue] = visible.bytes();
            report_accepted(red, green, blue)
        }
        Assessment::DeclaredSrgb8CleanSetRejected {
            visible,
            rejected_blue_interval,
        } => {
            let [red, green, blue] = visible.bytes();
            let [lower, upper] = rejected_blue_interval;
            report_rejected(red, green, blue, lower, upper)
        }
        Assessment::DeclaredSrgb8CleanSetFinalOwnedDomainAbsent => report_domain_absent(),
    }
}
