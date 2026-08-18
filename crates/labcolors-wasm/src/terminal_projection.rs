//! JSON projection of permanent non-Program capabilities retained after C7c.

use std::fmt::Write as _;

use crate::error::BindingError;

fn push_str_lit(out: &mut String, value: &str) {
    out.push_str(&serde_json::to_string(value).expect("string serialization is infallible"));
}

fn push_key_array<'a>(out: &mut String, name: &str, keys: impl Iterator<Item = &'a str>) {
    out.push_str(",\"");
    out.push_str(name);
    out.push_str("\":[");
    for (index, key) in keys.enumerate() {
        if index > 0 {
            out.push(',');
        }
        push_str_lit(out, key);
    }
    out.push(']');
}

pub(crate) fn capability_manifest_json() -> String {
    let manifest = labcolors_core::numerical_capability_manifest_v2();
    let mut out = String::with_capacity(512);
    out.push_str("{\"schemaVersion\":");
    let _ = write!(out, "{}", manifest.schema_version);
    out.push_str(",\"coverage\":");
    push_str_lit(&mut out, manifest.coverage.key());
    out.push_str(",\"sites\":[");
    for (index, site) in manifest.sites.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"siteId\":");
        push_str_lit(&mut out, site.site_id.key());
        push_key_array(
            &mut out,
            "stableOutcomes",
            site.stable_outcomes.iter().map(|v| v.key()),
        );
        push_key_array(
            &mut out,
            "compatibilityReleases",
            site.compatibility_releases.iter().map(|v| v.key()),
        );
        push_key_array(
            &mut out,
            "evidenceClasses",
            site.evidence_classes.iter().map(|v| v.key()),
        );
        push_key_array(
            &mut out,
            "artifactIds",
            site.artifact_ids.iter().map(|v| v.key()),
        );
        push_key_array(&mut out, "boundIds", site.bound_ids.iter().map(|v| v.key()));
        push_key_array(&mut out, "proofIds", site.proof_ids.iter().map(|v| v.key()));
        push_key_array(
            &mut out,
            "runtimeAttestations",
            site.runtime_attestations.iter().map(|v| v.key()),
        );
        out.push('}');
    }
    out.push_str("],\"checksum\":");
    push_str_lit(&mut out, &manifest.checksum.hex());
    out.push('}');
    out
}
pub(crate) fn wcag22_json(
    assessment: &labcolors_core::wcag22::Wcag22AssessmentV1,
) -> Result<String, BindingError> {
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

    let hex = |bytes: [u8; 3]| format!("#{:02X}{:02X}{:02X}", bytes[0], bytes[1], bytes[2]);
    let criterion = criterion.key();
    let decision = match decision {
        Wcag22ApplicableDecisionV1::Pass => "pass",
        Wcag22ApplicableDecisionV1::Fail => "fail",
        _ => {
            return Err(BindingError::Internal {
                reason: "unknown core WCAG22 decision variant".to_string(),
            });
        }
    };

    Ok(format!(
        concat!(
            "{{\"kind\":\"evaluated\",\"profileId\":\"{}\",",
            "\"criterion\":\"{}\",\"foreground\":\"{}\",\"background\":\"{}\",",
            "\"foregroundLuminanceQ55\":{{\"lower\":\"{}\",\"upper\":\"{}\"}},",
            "\"backgroundLuminanceQ55\":{{\"lower\":\"{}\",\"upper\":\"{}\"}},",
            "\"q55Scale\":\"{}\",\"decision\":\"{}\",",
            "\"evidence\":{{\"kind\":\"canonical-finite-bounded\",",
            "\"artifactId\":\"{}\",\"artifactSha256\":\"{}\",",
            "\"boundId\":\"{}\",\"proofId\":\"{}\",",
            "\"proofSha256\":\"{}\",\"proofPayloadSha256\":\"{}\",",
            "\"generatorSha256\":\"{}\",\"verifierSha256\":\"{}\",",
            "\"profileChecksum\":\"{}\",\"profileSha256\":\"{}\"}}}}"
        ),
        profile_id.key(),
        criterion,
        hex(measurement.foreground),
        hex(measurement.background),
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
    ))
}
