//! Структурная JS-проекция постоянных численных возможностей и WCAG-оценки Core.

use wasm_bindgen::JsValue;

use super::{
    append_projection_key, append_projection_value, project_capability_manifest,
    project_capability_site, project_wcag22_assessment, project_wcag22_evidence, report_array,
};
use crate::error::BindingError;

fn key_array<'a>(keys: impl Iterator<Item = &'a str>) -> Result<js_sys::Array, JsValue> {
    let array = report_array()?;
    for key in keys {
        append_projection_key(&array, key)?;
    }
    Ok(array)
}

pub(crate) fn capability_manifest() -> Result<JsValue, JsValue> {
    let manifest = labcolors_core::numerical_capability_manifest_v2();
    let sites = report_array()?;
    for site in &manifest.sites {
        let stable_outcomes = key_array(site.stable_outcomes.iter().map(|value| value.key()))?;
        let compatibility_releases =
            key_array(site.compatibility_releases.iter().map(|value| value.key()))?;
        let evidence_classes = key_array(site.evidence_classes.iter().map(|value| value.key()))?;
        let artifact_ids = key_array(site.artifact_ids.iter().map(|value| value.key()))?;
        let bound_ids = key_array(site.bound_ids.iter().map(|value| value.key()))?;
        let proof_ids = key_array(site.proof_ids.iter().map(|value| value.key()))?;
        let runtime_attestations =
            key_array(site.runtime_attestations.iter().map(|value| value.key()))?;
        let projected = project_capability_site(
            site.site_id.key(),
            &stable_outcomes,
            &compatibility_releases,
            &evidence_classes,
            &artifact_ids,
            &bound_ids,
            &proof_ids,
            &runtime_attestations,
        )?;
        append_projection_value(&sites, &projected)?;
    }
    project_capability_manifest(
        manifest.schema_version,
        manifest.coverage.key(),
        &sites,
        &manifest.checksum.hex(),
    )
}

pub(crate) fn wcag22(
    assessment: &labcolors_core::wcag22::Wcag22AssessmentV1,
) -> Result<JsValue, BindingError> {
    use labcolors_core::NumericalDecisionEvidenceV1;
    use labcolors_core::wcag22::{Wcag22ApplicableDecisionV1, Wcag22AssessmentV1};

    let Wcag22AssessmentV1::Evaluated {
        profile_id,
        criterion,
        measurement,
        decision,
        evidence,
        ..
    } = assessment
    else {
        return Err(BindingError::Internal {
            reason: "pair evaluator returned report-only NotEvaluated".to_string(),
        });
    };
    let NumericalDecisionEvidenceV1::CanonicalFiniteBounded(evidence_payload) = evidence else {
        return Err(BindingError::Internal {
            reason: "WCAG22 assessment carried a non-bounded evidence class".to_string(),
        });
    };
    let artifact_id = evidence_payload.artifact_id();
    let bound_id = evidence_payload.bound_id();
    let proof_id = evidence_payload.proof_id();
    let profile = labcolors_core::wcag22::wcag22_profile_v1();
    if profile.profile_id != *profile_id
        || profile.artifact_id != artifact_id
        || profile.bound_id != bound_id
        || profile.proof_id != proof_id
    {
        return Err(BindingError::Internal {
            reason: "WCAG22 assessment/profile evidence identities drifted".to_string(),
        });
    }
    let decision = match decision {
        Wcag22ApplicableDecisionV1::Pass => "pass",
        Wcag22ApplicableDecisionV1::Fail => "fail",
        _ => {
            return Err(BindingError::Internal {
                reason: "unknown core WCAG22 decision variant".to_string(),
            });
        }
    };
    let evidence = project_wcag22_evidence(
        artifact_id.key(),
        profile.artifact_sha256,
        bound_id.key(),
        proof_id.key(),
        profile.proof_sha256,
        profile.proof_payload_sha256,
        profile.generator_sha256,
        profile.verifier_sha256,
        profile.profile_checksum,
        profile.source_sha256,
    )
    .map_err(wcag22_projection_error)?;
    project_wcag22_assessment(
        profile_id.key(),
        criterion.key(),
        &measurement.foreground,
        &measurement.background,
        measurement.foreground_luminance.lower(),
        measurement.foreground_luminance.upper(),
        measurement.background_luminance.lower(),
        measurement.background_luminance.upper(),
        labcolors_core::wcag22::Wcag22LuminanceBoundsQ55V1::scale(),
        decision,
        &evidence,
    )
    .map_err(wcag22_projection_error)
}

fn wcag22_projection_error(_: JsValue) -> BindingError {
    BindingError::Internal {
        reason: "WCAG22 JS projection failed".to_string(),
    }
}
