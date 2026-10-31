//! Проекция постоянных возможностей в собственные JavaScript-данные.

use wasm_bindgen::JsValue;

use crate::error::BindingError;

fn push_keys<'a>(
    site: &JsValue,
    field: &str,
    keys: impl Iterator<Item = &'a str>,
) -> Result<(), JsValue> {
    for key in keys {
        crate::capability_key_projection(site, field, key)?;
    }
    Ok(())
}

pub(crate) fn capability_manifest() -> Result<JsValue, JsValue> {
    let manifest = labcolors_core::numerical_capability_manifest_v2();
    let projected = crate::capability_manifest_projection(
        manifest.schema_version,
        manifest.coverage.key(),
        &manifest.checksum.hex(),
    )?;
    for site in manifest.sites.iter() {
        let projected_site = crate::capability_site_projection(&projected, site.site_id.key())?;
        push_keys(
            &projected_site,
            "stableOutcomes",
            site.stable_outcomes.iter().map(|value| value.key()),
        )?;
        push_keys(
            &projected_site,
            "compatibilityReleases",
            site.compatibility_releases.iter().map(|value| value.key()),
        )?;
        push_keys(
            &projected_site,
            "evidenceClasses",
            site.evidence_classes.iter().map(|value| value.key()),
        )?;
        push_keys(
            &projected_site,
            "artifactIds",
            site.artifact_ids.iter().map(|value| value.key()),
        )?;
        push_keys(
            &projected_site,
            "boundIds",
            site.bound_ids.iter().map(|value| value.key()),
        )?;
        push_keys(
            &projected_site,
            "proofIds",
            site.proof_ids.iter().map(|value| value.key()),
        )?;
        push_keys(
            &projected_site,
            "runtimeAttestations",
            site.runtime_attestations.iter().map(|value| value.key()),
        )?;
    }
    Ok(projected)
}

fn packed_rgb(bytes: [u8; 3]) -> u32 {
    u32::from_be_bytes([0, bytes[0], bytes[1], bytes[2]])
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

    crate::wcag22_projection(
        profile_id.key(),
        criterion.key(),
        packed_rgb(measurement.foreground),
        packed_rgb(measurement.background),
        measurement.foreground_luminance.lower(),
        measurement.foreground_luminance.upper(),
        measurement.background_luminance.lower(),
        measurement.background_luminance.upper(),
        labcolors_core::wcag22::Wcag22LuminanceBoundsQ55V1::scale(),
        decision,
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
    .map_err(|_| BindingError::Internal {
        // Сохраняем прежнюю публичную диагностику отказа проекции.
        reason: "WCAG22 projection не распарсился как JSON".to_string(),
    })
}
