//! Структурная JS-проекция owned-отчёта. Ни классификатора, ни solver здесь нет.

use labcolors_core::program_wire::{
    ProgramAssessmentReportV1, ProgramAssessmentVerdictV1, ProgramCleanlinessAssessmentV1,
    ProgramCleanlinessStatusV1, ProgramConstraintAssessmentV1, ProgramReferencePreferenceV1,
};
use wasm_bindgen::prelude::*;

pub(super) fn project_field(
    field: Option<&labcolors_core::program_wire::ProgramFieldAssessmentV1>,
) -> Result<JsValue, JsValue> {
    let Some(field) = field else {
        return Ok(JsValue::NULL);
    };
    let stops = if field.stops().len() > 2 {
        let stops = super::report_array()?;
        for stop in field.stops() {
            let [r, g, b] = stop.color().bytes();
            super::report_array_push(
                &stops,
                &super::report_gradient_stop(stop.position(), r, g, b)?,
            )?;
        }
        Some(stops)
    } else {
        None
    };
    let witnesses = super::report_array()?;
    for witness in field.witnesses() {
        let (numerator, denominator) = witness.coordinate();
        let [r, g, b] = witness.nominal().bytes();
        let value = super::report_field_witness(
            stops.as_ref().map(|_| witness.segment_index() as u32),
            numerator,
            denominator,
            r,
            g,
            b,
        )?;
        super::report_array_push(&witnesses, &value)?;
    }
    let (from, to) = field.endpoints();
    let mut colors = [0; 6];
    colors[..3].copy_from_slice(&from.bytes());
    colors[3..].copy_from_slice(&to.bytes());
    super::report_field(
        field.input_index(),
        &colors,
        field.source_model(),
        field.domain(),
        field.rendering(),
        field.region_count(),
        &witnesses,
        stops,
    )
}

use super::{
    report_accepted, report_array, report_array_freeze, report_array_push, report_cell,
    report_cleanliness, report_domain_absent, report_projection, report_reference_capacity,
    report_reference_case, report_reference_selection, report_rejected, report_scenario,
    report_separation, report_summary, report_tonal_hierarchy,
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
    let cleanliness = project_cleanliness(Some(report.cleanliness()))?;
    let tonal_hierarchy = report_array()?;
    for pair in report.tonal_hierarchy() {
        let measurement = pair.measurement();
        let record = report_tonal_hierarchy(
            pair.constraint_id(),
            pair.case_index(),
            pair.stronger_occurrence_id(),
            pair.weaker_occurrence_id(),
            measurement.required_ratio(),
            measurement.actual_ratio(),
            measurement.required_gap(),
            measurement.actual_gap(),
            super::program_diagnostics_projection::polarity_key(measurement.stronger_polarity()),
            super::program_diagnostics_projection::polarity_key(measurement.weaker_polarity()),
            measurement.satisfied(),
        )?;
        report_array_push(&tonal_hierarchy, &record)?;
    }
    let reference_selections = reference_selections(report)?;
    let selection_components = selection_components(report)?;
    report_projection(
        &report.content_identity(),
        report.stream_id(),
        report.revision(),
        &scenario_provenance,
        &cells,
        &cleanliness,
        &tonal_hierarchy,
        &reference_selections,
        &selection_components,
    )
}

pub(super) fn project_selection_components(
    report: Option<&ProgramAssessmentReportV1>,
) -> Result<JsValue, JsValue> {
    let Some(report) = report else {
        return Ok(JsValue::NULL);
    };
    report_array_freeze(&selection_components(report)?)
}

fn selection_components(report: &ProgramAssessmentReportV1) -> Result<js_sys::Array, JsValue> {
    use labcolors_core::program_wire::ProgramSelectionModeV1;
    let components = report_array()?;
    for component in report.selection_components() {
        let mode = match component.mode() {
            ProgramSelectionModeV1::OrderedV1 => "ordered-v1",
            ProgramSelectionModeV1::JointPointV1 => "joint-point-v1",
            ProgramSelectionModeV1::OrderedReferenceV1 => "ordered-reference-v1",
        };
        let record = super::report_selection_component(mode, component.targets())?;
        report_array_push(&components, &record)?;
    }
    Ok(components)
}

/// Копирует только объяснение адаптации, без всех assessment cells.
pub(super) fn project_reference_selections(
    report: Option<&ProgramAssessmentReportV1>,
) -> Result<JsValue, JsValue> {
    let Some(report) = report else {
        return Ok(JsValue::NULL);
    };
    report_array_freeze(&reference_selections(report)?)
}

fn reference_selections(report: &ProgramAssessmentReportV1) -> Result<js_sys::Array, JsValue> {
    let selections = report_array()?;
    for selection in report.reference_selections() {
        let capacity = match selection.capacity() {
            Some(capacity) => report_reference_capacity(
                capacity.ratio_lower(),
                capacity.ratio_upper(),
                capacity.complete(),
            )?,
            None => JsValue::NULL,
        };
        let cases = report_array()?;
        for case in selection.cases() {
            let mut colors = [0_u8; 12];
            for (slot, rgb) in colors.chunks_exact_mut(3).zip([
                case.reference_visible(),
                case.reference_backdrop(),
                case.actual_visible(),
                case.actual_backdrop(),
            ]) {
                slot.copy_from_slice(&rgb.bytes());
            }
            let record = report_reference_case(
                case.case_index(),
                &colors,
                case.reference_contrast(),
                case.actual_contrast(),
                super::program_diagnostics_projection::polarity_key(case.polarity()),
                case.strength_loss(),
                case.virtual_distance_squared(),
            )?;
            report_array_push(&cases, &record)?;
        }
        let preference = match selection.preference() {
            ProgramReferencePreferenceV1::ReferenceStrength => "reference-strength",
            ProgramReferencePreferenceV1::ExplicitTone => "explicit-tone",
        };
        let record = report_reference_selection(
            selection.target_id(),
            selection.occurrence_id(),
            preference,
            &capacity,
            &cases,
        )?;
        report_array_push(&selections, &record)?;
    }
    Ok(selections)
}

/// Одна плоская immutable запись; полный массив клеток для обычного UI не копируется.
pub(super) fn project_cleanliness(
    assessment: Option<ProgramCleanlinessAssessmentV1>,
) -> Result<JsValue, JsValue> {
    let Some(assessment) = assessment else {
        return Ok(JsValue::NULL);
    };
    let status = match assessment.status() {
        ProgramCleanlinessStatusV1::NotAssessed => "not-assessed",
        ProgramCleanlinessStatusV1::Conforms => "conforms",
        ProgramCleanlinessStatusV1::Violates => "violates",
        ProgramCleanlinessStatusV1::Unavailable => "unavailable",
    };
    report_cleanliness(
        status,
        ProgramCleanlinessAssessmentV1::PROFILE_ID,
        ProgramCleanlinessAssessmentV1::BASIS,
        ProgramCleanlinessAssessmentV1::SCOPE,
        assessment.accepted(),
        assessment.rejected(),
        assessment.unavailable(),
    )
}

fn project_assessment(assessment: ProgramConstraintAssessmentV1) -> Result<JsValue, JsValue> {
    use ProgramConstraintAssessmentV1 as Assessment;
    let verdict_key = |verdict| match verdict {
        ProgramAssessmentVerdictV1::Pass => "pass",
        ProgramAssessmentVerdictV1::Violation => "violation",
    };
    match assessment {
        Assessment::NominalSrgb8VisiblePass { visible } => {
            let [red, green, blue] = visible.bytes();
            report_accepted("nominal-srgb8-visible", red, green, blue)
        }
        Assessment::NominalSrgb8VisibleRejected {
            visible,
            rejected_blue_interval,
        } => {
            let [red, green, blue] = visible.bytes();
            let [lower, upper] = rejected_blue_interval;
            report_rejected("nominal-srgb8-visible", red, green, blue, lower, upper)
        }
        Assessment::VisibleSeparation {
            verdict,
            measurement,
        } => {
            let [vr, vg, vb] = measurement.visible().bytes();
            let [br, bg, bb] = measurement.backdrop().bytes();
            report_separation(
                verdict_key(verdict),
                vr,
                vg,
                vb,
                br,
                bg,
                bb,
                measurement.distance(),
                measurement.distance_bounds()[0],
                measurement.distance_bounds()[1],
                measurement.minimum(),
                measurement.maximum(),
            )
        }
        Assessment::ExactSrgb8 { verdict } => report_summary("exact-srgb8", verdict_key(verdict)),
        Assessment::Wcag22Srgb8 { verdict } => report_summary("wcag22-srgb8", verdict_key(verdict)),
        Assessment::IntrinsicUnary { verdict } => {
            report_summary("intrinsic-unary", verdict_key(verdict))
        }
        Assessment::Relation { verdict } => report_summary("relation", verdict_key(verdict)),
        Assessment::TonalHierarchy { verdict } => {
            report_summary("tonal-hierarchy", verdict_key(verdict))
        }
        Assessment::DeclaredSrgb8CleanSetPass { visible } => {
            let [red, green, blue] = visible.bytes();
            report_accepted("declared-srgb8-clean-set", red, green, blue)
        }
        Assessment::DeclaredSrgb8CleanSetRejected {
            visible,
            rejected_blue_interval,
        } => {
            let [red, green, blue] = visible.bytes();
            let [lower, upper] = rejected_blue_interval;
            report_rejected("declared-srgb8-clean-set", red, green, blue, lower, upper)
        }
        Assessment::DeclaredSrgb8CleanSetFinalOwnedDomainAbsent => report_domain_absent(),
    }
}
