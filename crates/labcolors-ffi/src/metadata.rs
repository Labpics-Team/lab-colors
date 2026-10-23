//! Только проекции метаданных Core. Байты payload и producer capability не экспортируются.
use crate::LabColorsError;

#[derive(Debug, Clone, uniffi::Record)]
pub struct NumericalCapabilitySite {
    pub site_id: String,
    pub stable_outcomes: Vec<String>,
    pub compatibility_releases: Vec<String>,
    pub evidence_classes: Vec<String>,
    pub artifact_ids: Vec<String>,
    pub bound_ids: Vec<String>,
    pub proof_ids: Vec<String>,
    pub runtime_attestations: Vec<String>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct NumericalCapabilityManifest {
    pub schema_version: u32,
    pub coverage: String,
    pub sites: Vec<NumericalCapabilitySite>,
    pub checksum: String,
}

#[uniffi::export]
pub fn numerical_capability_manifest() -> NumericalCapabilityManifest {
    let value = labcolors_core::numerical_capability_manifest_v2();
    NumericalCapabilityManifest {
        schema_version: value.schema_version,
        coverage: value.coverage.key().into(),
        checksum: value.checksum.hex(),
        sites: value.sites.iter().map(|site| NumericalCapabilitySite {
            site_id: site.site_id.key().into(),
            stable_outcomes: site.stable_outcomes.iter().map(|v| v.key().into()).collect(),
            compatibility_releases: site.compatibility_releases.iter().map(|v| v.key().into()).collect(),
            evidence_classes: site.evidence_classes.iter().map(|v| v.key().into()).collect(),
            artifact_ids: site.artifact_ids.iter().map(|v| v.key().into()).collect(),
            bound_ids: site.bound_ids.iter().map(|v| v.key().into()).collect(),
            proof_ids: site.proof_ids.iter().map(|v| v.key().into()).collect(),
            runtime_attestations: site.runtime_attestations.iter().map(|v| v.key().into()).collect(),
        }).collect(),
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct UntrustedCertificateEnvelope {
    pub schema_version: u16,
    pub operation: String,
    pub authority_kind: String,
    pub authority_version: u16,
    pub runtime_artifact_id: String,
    pub producer_revision: String,
    pub producer_content_identity: Vec<u8>,
    pub context_id: String,
    pub payload_type: String,
    pub payload_version: u16,
    pub payload_length: u32,
    pub payload_sha256: Vec<u8>,
    pub binding_sha256: Vec<u8>,
}

#[uniffi::export]
pub fn issue_source_certificate_envelope() -> Result<Vec<u8>, LabColorsError> {
    let refusal = |e: labcolors_core::certificate::CertificateProducerErrorV1| {
        LabColorsError::new(&format!("certificate_{}", e.code()), "issueSourceCertificateEnvelope")
    };
    labcolors_core::certificate::issue_source_certificate_v1()
        .map_err(refusal)?.try_to_bytes().map_err(refusal)
}

#[uniffi::export]
pub fn decode_certificate_envelope(bytes: Vec<u8>) -> Result<UntrustedCertificateEnvelope, LabColorsError> {
    use labcolors_core::certificate::{CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1, UntrustedEnvelopeV1};
    let value = UntrustedEnvelopeV1::decode(&bytes).map_err(|e| {
        LabColorsError::new(&format!("certificate_{}", e.code()), "decodeCertificateEnvelope")
    })?;
    let key = value.admission_key();
    Ok(UntrustedCertificateEnvelope {
        schema_version: CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1,
        operation: key.operation().key().into(),
        authority_kind: key.authority_kind().key().into(),
        authority_version: key.authority_version(),
        runtime_artifact_id: key.runtime_artifact_id().into(),
        producer_revision: key.producer_revision().into(),
        producer_content_identity: key.producer_content_identity().to_vec(),
        context_id: key.context_id().into(),
        payload_type: key.payload_type().key().into(),
        payload_version: key.payload_version(),
        payload_length: value.payload_len(),
        payload_sha256: value.payload_sha256().to_vec(),
        binding_sha256: value.binding_sha256().to_vec(),
    })
}
