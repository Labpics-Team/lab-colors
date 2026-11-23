//! Структурная проекция immutable witness. Все решения уже приняты Core.

use super::{
    diagnostic_color, diagnostic_constraint, diagnostic_domain, diagnostic_subject,
    diagnostic_target, diagnostic_tonal_hierarchy, diagnostic_witness, diagnostics_projection,
    report_array, report_array_push,
};
use labcolors_core::program_wire::TonalPolarityV1;
use labcolors_core::program_wire::{
    ProgramDiagnosticSubjectV1, ProgramDiagnosticV1, ProgramDiagnosticsV1,
};
use wasm_bindgen::prelude::*;

pub(super) fn project(value: Option<&ProgramDiagnosticsV1>) -> Result<JsValue, JsValue> {
    let Some(value) = value else {
        return Ok(JsValue::NULL);
    };
    let witness = if let Some(witness) = value.witness() {
        let cases = report_array()?;
        for ids in witness.case_scenario_ids() {
            let case = report_array()?;
            for id in ids {
                report_array_push(&case, &JsValue::from(*id))?;
            }
            report_array_push(&cases, case.as_ref())?;
        }
        let targets = report_array()?;
        for target in witness.targets() {
            let [r, g, b] = target.source().bytes();
            report_array_push(&targets, &diagnostic_target(target.target_id(), r, g, b)?)?;
        }
        let violations = report_array()?;
        for violation in witness.violations() {
            report_array_push(&violations, &project_violation(*violation)?)?;
        }
        diagnostic_witness(&cases, &targets, &violations)?
    } else {
        JsValue::NULL
    };
    diagnostics_projection(value.search_visited(), value.search_budget(), &witness)
}

fn project_violation(value: ProgramDiagnosticV1) -> Result<JsValue, JsValue> {
    match value {
        ProgramDiagnosticV1::HardConstraint {
            constraint_id,
            case_index,
            subject,
        } => {
            let (kind, a, b, c) = match subject {
                ProgramDiagnosticSubjectV1::Target { target_id } => ("target", target_id, 0, 0),
                ProgramDiagnosticSubjectV1::Occurrence { occurrence_id } => {
                    ("occurrence", occurrence_id, 0, 0)
                }
                ProgramDiagnosticSubjectV1::IntrinsicRelation {
                    reference_target_id,
                } => ("intrinsic-relation", reference_target_id, 0, 0),
                ProgramDiagnosticSubjectV1::VisibleRelation {
                    reference_occurrence_id,
                } => ("visible-relation", reference_occurrence_id, 0, 0),
                ProgramDiagnosticSubjectV1::ComputedVisibleRelation => {
                    ("computed-visible-relation", 0, 0, 0)
                }
                ProgramDiagnosticSubjectV1::Presentation {
                    root_id,
                    occurrence_id,
                    terminal_id,
                } => ("presentation", root_id, occurrence_id, terminal_id),
            };
            diagnostic_constraint(
                constraint_id,
                case_index,
                &diagnostic_subject(kind, a, b, c)?,
            )
        }
        ProgramDiagnosticV1::TargetDomain { target_id, source } => {
            let [r, g, b] = source.bytes();
            diagnostic_domain(target_id, r, g, b)
        }
        ProgramDiagnosticV1::ColorIdentity {
            target_id,
            case_index,
            occurrence_id,
            actual,
        } => {
            let [r, g, b] = actual.bytes();
            diagnostic_color(
                "color-identity",
                target_id,
                case_index,
                occurrence_id,
                r,
                g,
                b,
                0.0,
                0.0,
            )
        }
        ProgramDiagnosticV1::MaxChange {
            target_id,
            case_index,
            occurrence_id,
            actual,
            distance_squared,
            maximum_distance,
        } => {
            let [r, g, b] = actual.bytes();
            diagnostic_color(
                "max-change",
                target_id,
                case_index,
                occurrence_id,
                r,
                g,
                b,
                distance_squared,
                maximum_distance,
            )
        }
        ProgramDiagnosticV1::TonalHierarchy {
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
        } => diagnostic_tonal_hierarchy(
            constraint_id,
            case_index,
            stronger_occurrence_id,
            weaker_occurrence_id,
            required_ratio,
            actual_ratio,
            required_gap,
            actual_gap,
            polarity_key(stronger_polarity),
            polarity_key(weaker_polarity),
        ),
    }
}

pub(super) const fn polarity_key(value: TonalPolarityV1) -> &'static str {
    match value {
        TonalPolarityV1::Darker => "darker",
        TonalPolarityV1::Equal => "equal",
        TonalPolarityV1::Lighter => "lighter",
    }
}
