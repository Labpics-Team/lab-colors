//! Fixed, non-semantic certificate transport envelope (r13).
//!
//! This module owns framing and producer binding only.  It does not interpret
//! the payload as colour science, a materialization result, or an authority
//! result.  A decoded byte buffer is deliberately represented as
//! [`UntrustedEnvelopeV1`].  The trusted type can only be made by a producer
//! capability kept inside this crate.

use core::fmt;
use std::collections::HashMap;

use crate::sha256::Hasher;

/// Wire magic for `CertificateEnvelopeV1`.
pub const CERTIFICATE_ENVELOPE_MAGIC_V1: [u8; 4] = *b"LCEN";
/// The only schema version understood by this module.
pub const CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1: u16 = 1;
/// `IssueCertificate` operation selector.
pub const ISSUE_CERTIFICATE_OPERATION_V1: u8 = 0x01;
/// `GenericTypedCertificate` framing selector.
pub const GENERIC_TYPED_CERTIFICATE_AUTHORITY_KIND_V1: u8 = 0x00;
/// Non-semantic opaque transport payload selector.
pub const NON_SEMANTIC_TRANSPORT_PAYLOAD_TYPE_V1: u8 = 0x01;
/// Maximum complete envelope accepted by the decoder and WASM ingress.
pub const MAX_ENVELOPE_BYTES_V1: usize = 2_097_152;
/// Maximum producer payload accepted by the envelope.
pub const MAX_PAYLOAD_BYTES_V1: usize = 1_048_576;
/// Maximum runtime artifact identifier size in UTF-8 bytes.
pub const MAX_RUNTIME_ARTIFACT_ID_BYTES_V1: usize = 128;
/// Maximum producer revision size in UTF-8 bytes.
pub const MAX_PRODUCER_REVISION_BYTES_V1: usize = 40;
/// Maximum context identifier size in UTF-8 bytes.
pub const MAX_CONTEXT_ID_BYTES_V1: usize = 256;
/// Maximum serialized producer tuple size.
pub const MAX_TUPLE_BYTES_V1: usize = 1_024;
/// Maximum number of admitted keys held by one state instance.
pub const MAX_ADMISSION_ENTRIES_V1: usize = 1_024;
/// Maximum accounted canonical bytes and per-entry metadata.
pub const MAX_ADMISSION_BYTES_V1: usize = 8_388_608;
const ADMISSION_METADATA_BYTES_V1: usize = 128;
const WIRE_MAGIC_BYTES_V1: usize = 4;
const WIRE_U8_BYTES_V1: usize = 1;
const WIRE_U16_BYTES_V1: usize = 2;
const WIRE_U32_BYTES_V1: usize = 4;
const WIRE_DIGEST_BYTES_V1: usize = 32;
const PAYLOAD_DOMAIN_V1: &[u8] = b"labpics.colors/certificate-payload/v1\0";
const BINDING_DOMAIN_V1: &[u8] = b"labpics.colors/certificate-envelope/v1\0";

/// Closed error vocabulary for the certificate transport boundary.
///
/// Variants intentionally carry no input-derived data.  This keeps `Display`
/// and `Debug` suitable for logs without leaking identifiers, payload bytes,
/// or digests.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CertificateErrorV1 {
    /// The four-byte wire prefix is not `LCEN`.
    InvalidMagic,
    /// The schema is not version 1.
    UnsupportedSchema,
    /// The operation selector is not `IssueCertificate`.
    UnknownOperation,
    /// The authority selector is unknown or reserved for another node.
    UnknownAuthorityKind,
    /// The selected authority version is not supported.
    UnsupportedAuthorityVersion,
    /// A text field is not valid UTF-8.
    InvalidUtf8,
    /// A field length is zero, inconsistent, or otherwise non-canonical.
    InvalidLength,
    /// The input ended before the declared field was available.
    TruncatedInput,
    /// Bytes remain after the exact positional packet.
    TrailingBytes,
    /// The producer revision is not exactly forty lowercase hexadecimal bytes.
    NonCanonicalRevision,
    /// The payload type selector is not the r13 transport payload.
    InvalidPayloadType,
    /// The payload type version is not supported.
    UnsupportedPayloadVersion,
    /// The payload digest does not match the payload bytes.
    PayloadDigestMismatch,
    /// The binding digest does not match the canonical prefix.
    BindingDigestMismatch,
    /// Admission was attempted without the producer capability.
    MissingProducerAttestation,
    /// The producer capability does not bind the decoded envelope.
    ProducerBindingMismatch,
    /// The expected runtime artifact does not match the envelope.
    RuntimeArtifactMismatch,
    /// The expected producer revision does not match the envelope.
    ProducerRevisionMismatch,
    /// The expected content identity does not match the envelope.
    ContentIdentityMismatch,
    /// The expected context does not match the envelope.
    ContextMismatch,
    /// A global or field-specific byte limit was exceeded.
    ResourceLimitExceeded,
    /// The admission ledger has no room for a new key.
    AdmissionCapacityExceeded,
    /// Opaque relay or an unsupported conversion was requested.
    UnsupportedOpaque,
    /// An existing key was presented with a different binding or bytes.
    BindingConflict,
}

impl CertificateErrorV1 {
    /// Stable static machine code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidMagic => "invalid_magic",
            Self::UnsupportedSchema => "unsupported_schema",
            Self::UnknownOperation => "unknown_operation",
            Self::UnknownAuthorityKind => "unknown_authority_kind",
            Self::UnsupportedAuthorityVersion => "unsupported_authority_version",
            Self::InvalidUtf8 => "invalid_utf8",
            Self::InvalidLength => "invalid_length",
            Self::TruncatedInput => "truncated_input",
            Self::TrailingBytes => "trailing_bytes",
            Self::NonCanonicalRevision => "non_canonical_revision",
            Self::InvalidPayloadType => "invalid_payload_type",
            Self::UnsupportedPayloadVersion => "unsupported_payload_version",
            Self::PayloadDigestMismatch => "payload_digest_mismatch",
            Self::BindingDigestMismatch => "binding_digest_mismatch",
            Self::MissingProducerAttestation => "missing_producer_attestation",
            Self::ProducerBindingMismatch => "producer_binding_mismatch",
            Self::RuntimeArtifactMismatch => "runtime_artifact_mismatch",
            Self::ProducerRevisionMismatch => "producer_revision_mismatch",
            Self::ContentIdentityMismatch => "content_identity_mismatch",
            Self::ContextMismatch => "context_mismatch",
            Self::ResourceLimitExceeded => "resource_limit_exceeded",
            Self::AdmissionCapacityExceeded => "admission_capacity_exceeded",
            Self::UnsupportedOpaque => "unsupported_opaque",
            Self::BindingConflict => "binding_conflict",
        }
    }
}

impl fmt::Debug for CertificateErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl fmt::Display for CertificateErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for CertificateErrorV1 {}

/// The only operation admitted by the r13 envelope.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum CertificateOperationV1 {
    /// Issue one generic typed transport certificate.
    IssueCertificate,
}

impl CertificateOperationV1 {
    const fn wire(self) -> u8 {
        match self {
            Self::IssueCertificate => ISSUE_CERTIFICATE_OPERATION_V1,
        }
    }

    /// Stable human-readable projection.
    pub const fn key(self) -> &'static str {
        match self {
            Self::IssueCertificate => "issue-certificate",
        }
    }
}

impl fmt::Debug for CertificateOperationV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// Authority framing kinds understood by r13.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum CertificateAuthorityKindV1 {
    /// Generic framing; no authority result is implied.
    GenericTypedCertificate,
}

impl CertificateAuthorityKindV1 {
    const fn wire(self) -> u8 {
        match self {
            Self::GenericTypedCertificate => GENERIC_TYPED_CERTIFICATE_AUTHORITY_KIND_V1,
        }
    }

    /// Stable human-readable projection.
    pub const fn key(self) -> &'static str {
        match self {
            Self::GenericTypedCertificate => "generic-typed-certificate",
        }
    }
}

impl fmt::Debug for CertificateAuthorityKindV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// Payload framing types understood by r13.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum CertificatePayloadTypeV1 {
    /// Sealed producer-owned opaque transport body.
    NonSemanticTransportPayloadV1,
}

impl CertificatePayloadTypeV1 {
    const fn wire(self) -> u8 {
        match self {
            Self::NonSemanticTransportPayloadV1 => NON_SEMANTIC_TRANSPORT_PAYLOAD_TYPE_V1,
        }
    }

    /// Stable human-readable projection.
    pub const fn key(self) -> &'static str {
        match self {
            Self::NonSemanticTransportPayloadV1 => "non-semantic-transport-v1",
        }
    }
}

impl fmt::Debug for CertificatePayloadTypeV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// Exact producer tuple used for admission and replay classification.
///
/// The fields are public only through checked construction and accessors.  The
/// type is a selector, not proof of producer authenticity; authenticity comes
/// from the private producer capability.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct AdmissionKeyV1 {
    runtime_artifact_id: String,
    operation: CertificateOperationV1,
    context_id: String,
    producer_revision: String,
    producer_content_identity: [u8; 32],
    authority_kind: CertificateAuthorityKindV1,
    authority_version: u16,
    payload_type: CertificatePayloadTypeV1,
    payload_version: u16,
}

impl AdmissionKeyV1 {
    /// Builds a checked exact tuple for the r13 schema.
    pub fn try_new(
        runtime_artifact_id: &str,
        operation: CertificateOperationV1,
        context_id: &str,
        producer_revision: &str,
        producer_content_identity: [u8; 32],
    ) -> Result<Self, CertificateErrorV1> {
        validate_text(runtime_artifact_id, MAX_RUNTIME_ARTIFACT_ID_BYTES_V1)?;
        validate_text(context_id, MAX_CONTEXT_ID_BYTES_V1)?;
        if !is_canonical_revision(producer_revision.as_bytes()) {
            return Err(CertificateErrorV1::NonCanonicalRevision);
        }
        let key = Self {
            runtime_artifact_id: runtime_artifact_id.to_owned(),
            operation,
            context_id: context_id.to_owned(),
            producer_revision: producer_revision.to_owned(),
            producer_content_identity,
            authority_kind: CertificateAuthorityKindV1::GenericTypedCertificate,
            authority_version: CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1,
            payload_type: CertificatePayloadTypeV1::NonSemanticTransportPayloadV1,
            payload_version: CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1,
        };
        if key.serialized_tuple_bytes() > MAX_TUPLE_BYTES_V1 {
            return Err(CertificateErrorV1::ResourceLimitExceeded);
        }
        Ok(key)
    }

    fn try_clone_for_admission(&self) -> Result<Self, CertificateErrorV1> {
        Ok(Self {
            runtime_artifact_id: try_clone_string(&self.runtime_artifact_id)?,
            operation: self.operation,
            context_id: try_clone_string(&self.context_id)?,
            producer_revision: try_clone_string(&self.producer_revision)?,
            producer_content_identity: self.producer_content_identity,
            authority_kind: self.authority_kind,
            authority_version: self.authority_version,
            payload_type: self.payload_type,
            payload_version: self.payload_version,
        })
    }

    /// Runtime artifact identity.
    pub fn runtime_artifact_id(&self) -> &str {
        &self.runtime_artifact_id
    }

    /// Operation selector.
    pub const fn operation(&self) -> CertificateOperationV1 {
        self.operation
    }

    /// Explicit context identity.
    pub fn context_id(&self) -> &str {
        &self.context_id
    }

    /// Full immutable producer revision.
    pub fn producer_revision(&self) -> &str {
        &self.producer_revision
    }

    /// Producer content identity bytes.
    pub const fn producer_content_identity(&self) -> &[u8; 32] {
        &self.producer_content_identity
    }

    /// Authority framing selector.
    pub const fn authority_kind(&self) -> CertificateAuthorityKindV1 {
        self.authority_kind
    }

    /// Authority framing version.
    pub const fn authority_version(&self) -> u16 {
        self.authority_version
    }

    /// Payload framing selector.
    pub const fn payload_type(&self) -> CertificatePayloadTypeV1 {
        self.payload_type
    }

    /// Payload framing version.
    pub const fn payload_version(&self) -> u16 {
        self.payload_version
    }

    // The non-payload portion of `encode_prefix`: every fixed field must be
    // counted here, including the payload length and payload digest, because
    // this value is the bounded admission tuple size.
    fn serialized_tuple_bytes(&self) -> usize {
        WIRE_MAGIC_BYTES_V1
            + WIRE_U16_BYTES_V1
            + WIRE_U8_BYTES_V1
            + WIRE_U8_BYTES_V1
            + WIRE_U16_BYTES_V1
            + WIRE_U16_BYTES_V1
            + self.runtime_artifact_id.len()
            + WIRE_U16_BYTES_V1
            + self.producer_revision.len()
            + WIRE_DIGEST_BYTES_V1
            + WIRE_U16_BYTES_V1
            + self.context_id.len()
            + WIRE_U8_BYTES_V1
            + WIRE_U16_BYTES_V1
            + WIRE_U32_BYTES_V1
            + WIRE_DIGEST_BYTES_V1
    }
}

impl fmt::Debug for AdmissionKeyV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdmissionKeyV1")
            .field("field_count", &9_u8)
            .field("tuple_bytes", &self.serialized_tuple_bytes())
            .finish()
    }
}

/// Sealed producer-owned non-semantic payload.
///
/// There is intentionally no public constructor, `From<Vec<u8>>`, serde
/// implementation, or conversion from Program/colour/materialization values.
/// A canonical producer inside this crate must create it first.
pub struct NonSemanticTransportPayloadV1 {
    bytes: Box<[u8]>,
}

impl NonSemanticTransportPayloadV1 {
    /// Number of opaque payload bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Whether the sealed payload contains no bytes.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Payload bytes for the canonical producer while building an envelope.
    ///
    /// The bytes remain non-semantic; this accessor is not a parser or an
    /// authority conversion.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    // This constructor is intentionally dormant until a canonical producer is
    // wired in by a later node; exposing it publicly would cross the sealed
    // producer boundary, while removing it would leave the contract without
    // its only approved construction point.
    #[allow(dead_code)]
    pub(crate) fn from_canonical_producer_bytes(bytes: &[u8]) -> Result<Self, CertificateErrorV1> {
        if bytes.is_empty() {
            return Err(CertificateErrorV1::InvalidLength);
        }
        if bytes.len() > MAX_PAYLOAD_BYTES_V1 {
            return Err(CertificateErrorV1::ResourceLimitExceeded);
        }
        Ok(Self {
            bytes: bytes.to_vec().into_boxed_slice(),
        })
    }
}

impl fmt::Debug for NonSemanticTransportPayloadV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NonSemanticTransportPayloadV1")
            .field("byte_length", &self.bytes.len())
            .finish()
    }
}

/// Private producer capability.  It is not serializable and has no public
/// constructor; its values are made only by the canonical producer helper.
pub struct TrustedProducerAttestationV1 {
    key: AdmissionKeyV1,
    payload_sha256: [u8; 32],
    binding_sha256: [u8; 32],
}

impl fmt::Debug for TrustedProducerAttestationV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TrustedProducerAttestationV1")
            .field("present", &true)
            .finish()
    }
}

/// A trusted, fully canonical envelope created by the producer capability.
pub struct CertificateEnvelopeV1 {
    key: AdmissionKeyV1,
    payload: Box<[u8]>,
    payload_sha256: [u8; 32],
    binding_sha256: [u8; 32],
    canonical_bytes: Box<[u8]>,
}

impl CertificateEnvelopeV1 {
    /// Creates an envelope only from a sealed payload and matching producer
    /// attestation.
    pub fn issue_from_trusted_producer(
        payload: NonSemanticTransportPayloadV1,
        attestation: TrustedProducerAttestationV1,
    ) -> Result<Self, CertificateErrorV1> {
        let payload_sha256 = payload_digest(payload.as_bytes());
        if payload_sha256 != attestation.payload_sha256 {
            return Err(CertificateErrorV1::ProducerBindingMismatch);
        }
        let prefix = encode_prefix(&attestation.key, payload.as_bytes(), payload_sha256)?;
        let binding_sha256 = binding_digest(&prefix);
        if binding_sha256 != attestation.binding_sha256 {
            return Err(CertificateErrorV1::ProducerBindingMismatch);
        }
        let mut canonical_bytes = prefix;
        canonical_bytes.extend_from_slice(&binding_sha256);
        if canonical_bytes.len() > MAX_ENVELOPE_BYTES_V1 {
            return Err(CertificateErrorV1::ResourceLimitExceeded);
        }
        Ok(Self {
            key: attestation.key,
            payload: payload.bytes,
            payload_sha256,
            binding_sha256,
            canonical_bytes: canonical_bytes.into_boxed_slice(),
        })
    }

    /// The exact canonical wire bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }

    /// Copies the exact canonical wire bytes for a transport boundary.
    pub fn to_bytes(&self) -> Box<[u8]> {
        self.canonical_bytes.clone()
    }

    /// Exact producer tuple bound by the envelope.
    pub fn admission_key(&self) -> &AdmissionKeyV1 {
        &self.key
    }

    /// Payload digest bound by the envelope.
    pub const fn payload_sha256(&self) -> &[u8; 32] {
        &self.payload_sha256
    }

    /// Tuple binding digest bound by the envelope.
    pub const fn binding_sha256(&self) -> &[u8; 32] {
        &self.binding_sha256
    }
}

impl fmt::Debug for CertificateEnvelopeV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CertificateEnvelopeV1")
            .field("payload_length", &self.payload.len())
            .field("envelope_length", &self.canonical_bytes.len())
            .finish()
    }
}

/// A structurally valid envelope decoded from untrusted bytes.
///
/// This type cannot be used as a trusted certificate and carries no producer
/// capability.  Admission still requires an exact expected key and a matching
/// [`TrustedProducerAttestationV1`].
#[derive(PartialEq, Eq)]
pub struct UntrustedEnvelopeV1 {
    key: AdmissionKeyV1,
    payload_len: u32,
    payload_sha256: [u8; 32],
    binding_sha256: [u8; 32],
    canonical_bytes: Box<[u8]>,
}

impl UntrustedEnvelopeV1 {
    /// Decodes the exact positional r13 packet and verifies both digests.
    pub fn decode(bytes: &[u8]) -> Result<Self, CertificateErrorV1> {
        if bytes.len() > MAX_ENVELOPE_BYTES_V1 {
            return Err(CertificateErrorV1::ResourceLimitExceeded);
        }
        let mut reader = Reader::new(bytes);
        if reader.read_array::<4>()? != CERTIFICATE_ENVELOPE_MAGIC_V1 {
            return Err(CertificateErrorV1::InvalidMagic);
        }
        if reader.read_u16()? != CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1 {
            return Err(CertificateErrorV1::UnsupportedSchema);
        }
        if reader.read_u8()? != ISSUE_CERTIFICATE_OPERATION_V1 {
            return Err(CertificateErrorV1::UnknownOperation);
        }
        if reader.read_u8()? != GENERIC_TYPED_CERTIFICATE_AUTHORITY_KIND_V1 {
            return Err(CertificateErrorV1::UnknownAuthorityKind);
        }
        if reader.read_u16()? != CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1 {
            return Err(CertificateErrorV1::UnsupportedAuthorityVersion);
        }

        let runtime_artifact_id = reader.read_text(MAX_RUNTIME_ARTIFACT_ID_BYTES_V1)?;
        let producer_revision_bytes =
            reader.read_length_delimited(MAX_PRODUCER_REVISION_BYTES_V1)?;
        if !is_canonical_revision(producer_revision_bytes) {
            return Err(CertificateErrorV1::NonCanonicalRevision);
        }
        let producer_revision = core::str::from_utf8(producer_revision_bytes)
            .map_err(|_| CertificateErrorV1::InvalidUtf8)?;
        let producer_content_identity = reader.read_array::<32>()?;
        let context_id = reader.read_text(MAX_CONTEXT_ID_BYTES_V1)?;

        if reader.read_u8()? != NON_SEMANTIC_TRANSPORT_PAYLOAD_TYPE_V1 {
            return Err(CertificateErrorV1::InvalidPayloadType);
        }
        if reader.read_u16()? != CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1 {
            return Err(CertificateErrorV1::UnsupportedPayloadVersion);
        }
        let payload_length = reader.read_u32()?;
        let payload_length = usize::try_from(payload_length)
            .map_err(|_| CertificateErrorV1::ResourceLimitExceeded)?;
        if payload_length == 0 {
            return Err(CertificateErrorV1::InvalidLength);
        }
        if payload_length > MAX_PAYLOAD_BYTES_V1 {
            return Err(CertificateErrorV1::ResourceLimitExceeded);
        }
        let payload = reader.read_exact(payload_length)?;
        let payload_sha256 = reader.read_array::<32>()?;
        if payload_digest(payload) != payload_sha256 {
            return Err(CertificateErrorV1::PayloadDigestMismatch);
        }
        let binding_start = reader.offset;
        let binding_sha256 = reader.read_array::<32>()?;
        if binding_digest(&bytes[..binding_start]) != binding_sha256 {
            return Err(CertificateErrorV1::BindingDigestMismatch);
        }
        if !reader.is_finished() {
            return Err(CertificateErrorV1::TrailingBytes);
        }

        let key = AdmissionKeyV1::try_new(
            runtime_artifact_id,
            CertificateOperationV1::IssueCertificate,
            context_id,
            producer_revision,
            producer_content_identity,
        )?;
        Ok(Self {
            key,
            payload_len: payload_length as u32,
            payload_sha256,
            binding_sha256,
            canonical_bytes: bytes.to_vec().into_boxed_slice(),
        })
    }

    /// Exact producer tuple parsed from the untrusted bytes.
    pub fn admission_key(&self) -> &AdmissionKeyV1 {
        &self.key
    }

    /// Opaque payload byte length.
    pub fn payload_len(&self) -> u32 {
        self.payload_len
    }

    /// Payload digest after structural verification.
    pub const fn payload_sha256(&self) -> &[u8; 32] {
        &self.payload_sha256
    }

    /// Binding digest after structural verification.
    pub const fn binding_sha256(&self) -> &[u8; 32] {
        &self.binding_sha256
    }

    /// Exact decoded bytes retained for duplicate/conflict comparison.
    pub(crate) fn canonical_bytes(&self) -> &[u8] {
        &self.canonical_bytes
    }
}

impl fmt::Debug for UntrustedEnvelopeV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UntrustedEnvelopeV1")
            .field("payload_length", &self.payload_len)
            .field("envelope_length", &self.canonical_bytes.len())
            .finish()
    }
}

/// Idempotent result of an admission attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AdmissionOutcomeV1 {
    /// The key was inserted for the first time.
    Accepted,
    /// The exact same key, binding, and bytes were already inserted.
    DuplicateNoop,
}

struct AdmissionRecord {
    binding_sha256: [u8; 32],
    canonical_bytes: Vec<u8>,
}

/// Caller-owned, in-memory, single-writer replay ledger.
///
/// The API takes `&mut self`; a multi-threaded host must serialize access
/// outside this type.  The insertion point is after all validation and the
/// capacity check, so a refusal never changes prior state.
pub struct AdmissionStateV1 {
    records: HashMap<AdmissionKeyV1, AdmissionRecord>,
    accounted_bytes: usize,
}

impl AdmissionStateV1 {
    /// Creates an empty ledger for one runtime lifetime.
    pub fn new() -> Self {
        Self {
            records: HashMap::new(),
            accounted_bytes: 0,
        }
    }

    /// Number of distinct admitted keys.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether no key has been admitted.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Accounted canonical bytes plus bounded per-entry metadata.
    pub fn accounted_bytes(&self) -> usize {
        self.accounted_bytes
    }

    /// Admits one decoded envelope under an explicit expected tuple.
    pub fn admit(
        &mut self,
        envelope: &UntrustedEnvelopeV1,
        expected: &AdmissionKeyV1,
        attestation: Option<&TrustedProducerAttestationV1>,
    ) -> Result<AdmissionOutcomeV1, CertificateErrorV1> {
        let attestation = attestation.ok_or(CertificateErrorV1::MissingProducerAttestation)?;
        if attestation.key != *envelope.admission_key()
            || attestation.payload_sha256 != *envelope.payload_sha256()
            || attestation.binding_sha256 != *envelope.binding_sha256()
        {
            return Err(CertificateErrorV1::ProducerBindingMismatch);
        }
        compare_expected(envelope.admission_key(), expected)?;

        if let Some(record) = self.records.get(expected) {
            if record.binding_sha256 == *envelope.binding_sha256()
                && record.canonical_bytes.as_ref() == envelope.canonical_bytes()
            {
                return Ok(AdmissionOutcomeV1::DuplicateNoop);
            }
            return Err(CertificateErrorV1::BindingConflict);
        }

        if self.records.len() >= MAX_ADMISSION_ENTRIES_V1 {
            return Err(CertificateErrorV1::AdmissionCapacityExceeded);
        }
        let entry_bytes = envelope
            .canonical_bytes()
            .len()
            .checked_add(ADMISSION_METADATA_BYTES_V1)
            .ok_or(CertificateErrorV1::AdmissionCapacityExceeded)?;
        let next_bytes = self
            .accounted_bytes
            .checked_add(entry_bytes)
            .ok_or(CertificateErrorV1::AdmissionCapacityExceeded)?;
        if next_bytes > MAX_ADMISSION_BYTES_V1 {
            return Err(CertificateErrorV1::AdmissionCapacityExceeded);
        }

        self.records
            .try_reserve(1)
            .map_err(|_| CertificateErrorV1::AdmissionCapacityExceeded)?;
        let prepared_key = expected.try_clone_for_admission()?;
        let prepared_canonical_bytes = try_clone_bytes(envelope.canonical_bytes())?;
        self.records.insert(
            prepared_key,
            AdmissionRecord {
                binding_sha256: *envelope.binding_sha256(),
                canonical_bytes: prepared_canonical_bytes,
            },
        );
        self.accounted_bytes = next_bytes;
        Ok(AdmissionOutcomeV1::Accepted)
    }
}

impl Default for AdmissionStateV1 {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for AdmissionStateV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AdmissionStateV1")
            .field("entry_count", &self.records.len())
            .field("accounted_bytes", &self.accounted_bytes)
            .finish()
    }
}

fn compare_expected(
    actual: &AdmissionKeyV1,
    expected: &AdmissionKeyV1,
) -> Result<(), CertificateErrorV1> {
    if actual.runtime_artifact_id != expected.runtime_artifact_id {
        return Err(CertificateErrorV1::RuntimeArtifactMismatch);
    }
    if actual.producer_revision != expected.producer_revision {
        return Err(CertificateErrorV1::ProducerRevisionMismatch);
    }
    if actual.producer_content_identity != expected.producer_content_identity {
        return Err(CertificateErrorV1::ContentIdentityMismatch);
    }
    if actual.context_id != expected.context_id {
        return Err(CertificateErrorV1::ContextMismatch);
    }
    if actual != expected {
        return Err(CertificateErrorV1::ProducerBindingMismatch);
    }
    Ok(())
}

fn try_clone_string(value: &str) -> Result<String, CertificateErrorV1> {
    let mut cloned = String::new();
    cloned
        .try_reserve_exact(value.len())
        .map_err(|_| CertificateErrorV1::AdmissionCapacityExceeded)?;
    cloned.push_str(value);
    Ok(cloned)
}

fn try_clone_bytes(value: &[u8]) -> Result<Vec<u8>, CertificateErrorV1> {
    let mut cloned = Vec::new();
    cloned
        .try_reserve_exact(value.len())
        .map_err(|_| CertificateErrorV1::AdmissionCapacityExceeded)?;
    cloned.extend_from_slice(value);
    Ok(cloned)
}

fn validate_text(value: &str, max_bytes: usize) -> Result<(), CertificateErrorV1> {
    if value.is_empty() {
        return Err(CertificateErrorV1::InvalidLength);
    }
    if value.len() > max_bytes {
        return Err(CertificateErrorV1::ResourceLimitExceeded);
    }
    if value.as_bytes().contains(&0) {
        return Err(CertificateErrorV1::InvalidLength);
    }
    Ok(())
}

fn is_canonical_revision(bytes: &[u8]) -> bool {
    bytes.len() == MAX_PRODUCER_REVISION_BYTES_V1
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
}

fn payload_digest(payload: &[u8]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(PAYLOAD_DOMAIN_V1);
    hasher.update(payload);
    *hasher.finalize().as_bytes()
}

fn binding_digest(prefix: &[u8]) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(BINDING_DOMAIN_V1);
    hasher.update(prefix);
    *hasher.finalize().as_bytes()
}

fn encode_prefix(
    key: &AdmissionKeyV1,
    payload: &[u8],
    payload_sha256: [u8; 32],
) -> Result<Vec<u8>, CertificateErrorV1> {
    if payload.is_empty() {
        return Err(CertificateErrorV1::InvalidLength);
    }
    if payload.len() > MAX_PAYLOAD_BYTES_V1 {
        return Err(CertificateErrorV1::ResourceLimitExceeded);
    }
    let mut bytes = Vec::with_capacity(key.serialized_tuple_bytes() + payload.len() + 32);
    bytes.extend_from_slice(&CERTIFICATE_ENVELOPE_MAGIC_V1);
    bytes.extend_from_slice(&CERTIFICATE_ENVELOPE_SCHEMA_VERSION_V1.to_be_bytes());
    bytes.push(key.operation.wire());
    bytes.push(key.authority_kind.wire());
    bytes.extend_from_slice(&key.authority_version.to_be_bytes());
    write_text(&mut bytes, &key.runtime_artifact_id)?;
    write_text(&mut bytes, &key.producer_revision)?;
    bytes.extend_from_slice(&key.producer_content_identity);
    write_text(&mut bytes, &key.context_id)?;
    bytes.push(key.payload_type.wire());
    bytes.extend_from_slice(&key.payload_version.to_be_bytes());
    let payload_length =
        u32::try_from(payload.len()).map_err(|_| CertificateErrorV1::ResourceLimitExceeded)?;
    bytes.extend_from_slice(&payload_length.to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes.extend_from_slice(&payload_sha256);
    if bytes.len() > MAX_ENVELOPE_BYTES_V1 {
        return Err(CertificateErrorV1::ResourceLimitExceeded);
    }
    Ok(bytes)
}

fn write_text(bytes: &mut Vec<u8>, value: &str) -> Result<(), CertificateErrorV1> {
    let length =
        u16::try_from(value.len()).map_err(|_| CertificateErrorV1::ResourceLimitExceeded)?;
    if length == 0 {
        return Err(CertificateErrorV1::InvalidLength);
    }
    bytes.extend_from_slice(&length.to_be_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn read_exact(&mut self, length: usize) -> Result<&'a [u8], CertificateErrorV1> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(CertificateErrorV1::InvalidLength)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(CertificateErrorV1::TruncatedInput)?;
        self.offset = end;
        Ok(value)
    }

    fn read_array<const N: usize>(&mut self) -> Result<[u8; N], CertificateErrorV1> {
        let value = self.read_exact(N)?;
        let mut output = [0; N];
        output.copy_from_slice(value);
        Ok(output)
    }

    fn read_u8(&mut self) -> Result<u8, CertificateErrorV1> {
        Ok(self.read_array::<1>()?[0])
    }

    fn read_u16(&mut self) -> Result<u16, CertificateErrorV1> {
        Ok(u16::from_be_bytes(self.read_array::<2>()?))
    }

    fn read_u32(&mut self) -> Result<u32, CertificateErrorV1> {
        Ok(u32::from_be_bytes(self.read_array::<4>()?))
    }

    fn read_text(&mut self, max_bytes: usize) -> Result<&'a str, CertificateErrorV1> {
        let bytes = self.read_length_delimited(max_bytes)?;
        let value = core::str::from_utf8(bytes).map_err(|_| CertificateErrorV1::InvalidUtf8)?;
        validate_text(value, max_bytes)?;
        Ok(value)
    }

    fn read_length_delimited(&mut self, max_bytes: usize) -> Result<&'a [u8], CertificateErrorV1> {
        let length = usize::from(self.read_u16()?);
        if length > max_bytes {
            return Err(CertificateErrorV1::ResourceLimitExceeded);
        }
        self.read_exact(length)
    }

    fn is_finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

/// Producer-only constructor for a non-semantic payload.
///
/// This is `pub(crate)` so another canonical producer module in this crate can
/// use the same sealed boundary without giving JS, serde, or external Rust a
/// raw-byte constructor.
#[allow(dead_code)]
pub(crate) fn producer_payload_v1(
    bytes: &[u8],
) -> Result<NonSemanticTransportPayloadV1, CertificateErrorV1> {
    NonSemanticTransportPayloadV1::from_canonical_producer_bytes(bytes)
}

/// Producer-only attestation constructor.  It computes the exact two digests
/// over the same fixed wire prefix that the consumer later verifies.
#[allow(dead_code)]
pub(crate) fn producer_attestation_v1(
    key: AdmissionKeyV1,
    payload: &NonSemanticTransportPayloadV1,
) -> Result<TrustedProducerAttestationV1, CertificateErrorV1> {
    let payload_sha256 = payload_digest(payload.as_bytes());
    let prefix = encode_prefix(&key, payload.as_bytes(), payload_sha256)?;
    let binding_sha256 = binding_digest(&prefix);
    Ok(TrustedProducerAttestationV1 {
        key,
        payload_sha256,
        binding_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::thread;

    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";

    fn hex(bytes: &[u8]) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut output = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            output.push(char::from(DIGITS[usize::from(byte >> 4)]));
            output.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
        }
        output
    }

    fn fixture(
        context: &str,
        payload_bytes: &[u8],
    ) -> (
        AdmissionKeyV1,
        NonSemanticTransportPayloadV1,
        TrustedProducerAttestationV1,
        CertificateEnvelopeV1,
    ) {
        let key = AdmissionKeyV1::try_new(
            "labcolors-core-test",
            CertificateOperationV1::IssueCertificate,
            context,
            REVISION,
            [0x11; 32],
        )
        .unwrap();
        let payload = producer_payload_v1(payload_bytes).unwrap();
        let attestation = producer_attestation_v1(key.clone(), &payload).unwrap();
        let envelope = CertificateEnvelopeV1::issue_from_trusted_producer(
            producer_payload_v1(payload_bytes).unwrap(),
            producer_attestation_v1(key.clone(), &producer_payload_v1(payload_bytes).unwrap())
                .unwrap(),
        )
        .unwrap();
        (key, payload, attestation, envelope)
    }

    fn decode_fixture(envelope: &CertificateEnvelopeV1) -> UntrustedEnvelopeV1 {
        UntrustedEnvelopeV1::decode(envelope.as_bytes()).unwrap()
    }

    #[test]
    fn canonical_wire_round_trips_and_is_positional() {
        let (_, _, _, envelope) = fixture("context-v1", b"producer-body-v1");
        let decoded = decode_fixture(&envelope);
        assert_eq!(
            decoded.admission_key().runtime_artifact_id(),
            "labcolors-core-test"
        );
        assert_eq!(decoded.admission_key().context_id(), "context-v1");
        assert_eq!(decoded.payload_len(), b"producer-body-v1".len() as u32);
        assert_eq!(decoded.canonical_bytes(), envelope.as_bytes());
        assert_eq!(
            hex(envelope.as_bytes()),
            "4c43454e00010100000100136c6162636f6c6f72732d636f72652d746573740028303132333435363738396162636465663031323334353637383961626364656630313233343536371111111111111111111111111111111111111111111111111111111111111111000363747801000100000004626f6479f478fc587248720033a34fede7ce76aa10daf6d0e3370032bf89d6d0b7d8701c5d4de35c4fbb1e16da14deafff75d4156bd075d4027b0e4d073fe62c70f3ca37"
        );
        assert_eq!(
            UntrustedEnvelopeV1::decode(&[envelope.as_bytes(), &[0]].concat()),
            Err(CertificateErrorV1::TrailingBytes)
        );
    }

    #[test]
    fn independent_reference_decoder_agrees_with_wire_prefix() {
        let (_, _, _, envelope) = fixture("ctx", b"body");
        let bytes = envelope.as_bytes();
        let mut offset = 0;
        assert_eq!(&bytes[offset..offset + 4], b"LCEN");
        offset += 4;
        assert_eq!(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]), 1);
        offset += 2;
        assert_eq!(bytes[offset], 1);
        offset += 1;
        assert_eq!(bytes[offset], 0);
        offset += 1;
        assert_eq!(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]), 1);
        offset += 2;
        let runtime_length = usize::from(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]));
        offset += 2;
        assert_eq!(
            &bytes[offset..offset + runtime_length],
            b"labcolors-core-test"
        );
        offset += runtime_length;
        let revision_length = usize::from(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]));
        offset += 2;
        assert_eq!(revision_length, 40);
        assert!(
            bytes[offset..offset + revision_length]
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        );
        offset += revision_length;
        assert_eq!(&bytes[offset..offset + 32], &[0x11; 32]);
        offset += 32;
        let context_length = usize::from(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]));
        offset += 2;
        assert_eq!(&bytes[offset..offset + context_length], b"ctx");
        offset += context_length;
        assert_eq!(bytes[offset], 1);
        offset += 1;
        assert_eq!(u16::from_be_bytes([bytes[offset], bytes[offset + 1]]), 1);
        offset += 2;
        let payload_length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        offset += 4;
        assert_eq!(payload_length, 4);
        assert_eq!(&bytes[offset..offset + payload_length], b"body");
        offset += payload_length + 32;
        assert_eq!(bytes.len(), offset + 32);
        assert_eq!(
            UntrustedEnvelopeV1::decode(bytes)
                .unwrap()
                .canonical_bytes(),
            bytes
        );
    }

    #[test]
    fn producer_capability_is_required_and_matching() {
        let (key, _, attestation, envelope) = fixture("ctx", b"body");
        let decoded = decode_fixture(&envelope);
        let mut state = AdmissionStateV1::new();
        assert_eq!(
            state.admit(&decoded, &key, None),
            Err(CertificateErrorV1::MissingProducerAttestation)
        );
        assert_eq!(
            state.admit(&decoded, &key, Some(&attestation)),
            Ok(AdmissionOutcomeV1::Accepted)
        );
        assert_eq!(
            state.admit(&decoded, &key, Some(&attestation)),
            Ok(AdmissionOutcomeV1::DuplicateNoop)
        );
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn expected_tuple_mismatches_are_typed_and_state_preserving() {
        let (key, _, attestation, envelope) = fixture("ctx", b"body");
        let decoded = decode_fixture(&envelope);
        let mut state = AdmissionStateV1::new();
        for (runtime, revision, identity, context, expected_error) in [
            (
                "other-runtime",
                REVISION,
                [0x11; 32],
                "ctx",
                CertificateErrorV1::RuntimeArtifactMismatch,
            ),
            (
                "labcolors-core-test",
                "fedcba9876543210fedcba9876543210fedcba98",
                [0x11; 32],
                "ctx",
                CertificateErrorV1::ProducerRevisionMismatch,
            ),
            (
                "labcolors-core-test",
                REVISION,
                [0x22; 32],
                "ctx",
                CertificateErrorV1::ContentIdentityMismatch,
            ),
            (
                "labcolors-core-test",
                REVISION,
                [0x11; 32],
                "other-context",
                CertificateErrorV1::ContextMismatch,
            ),
        ] {
            let expected = AdmissionKeyV1::try_new(
                runtime,
                CertificateOperationV1::IssueCertificate,
                context,
                revision,
                identity,
            )
            .unwrap();
            assert_eq!(
                state.admit(&decoded, &expected, Some(&attestation)),
                Err(expected_error)
            );
            assert_eq!(state.len(), 0);
        }
        assert_eq!(
            state.admit(&decoded, &key, Some(&attestation)),
            Ok(AdmissionOutcomeV1::Accepted)
        );
    }

    #[test]
    fn payload_replacement_and_binding_omission_fail_closed() {
        let (_, _, _, envelope) = fixture("ctx", b"body");
        let mut payload_replacement = envelope.to_bytes().into_vec();
        let payload_offset = payload_replacement
            .windows(4)
            .position(|window| window == b"body")
            .unwrap();
        payload_replacement[payload_offset] ^= 1;
        assert_eq!(
            UntrustedEnvelopeV1::decode(&payload_replacement),
            Err(CertificateErrorV1::PayloadDigestMismatch)
        );

        let mut omitted_binding = envelope.to_bytes().into_vec();
        let binding_offset = omitted_binding.len() - 1;
        omitted_binding[binding_offset] ^= 1;
        assert_eq!(
            UntrustedEnvelopeV1::decode(&omitted_binding),
            Err(CertificateErrorV1::BindingDigestMismatch)
        );
    }

    #[test]
    fn selector_length_and_version_mutants_never_fallback() {
        let (_, _, _, envelope) = fixture("ctx", b"body");
        let mut unknown_operation = envelope.to_bytes().into_vec();
        unknown_operation[6] = 0x7f;
        assert_eq!(
            UntrustedEnvelopeV1::decode(&unknown_operation),
            Err(CertificateErrorV1::UnknownOperation)
        );

        let mut reserved_authority = envelope.to_bytes().into_vec();
        reserved_authority[7] = 0x01;
        assert_eq!(
            UntrustedEnvelopeV1::decode(&reserved_authority),
            Err(CertificateErrorV1::UnknownAuthorityKind)
        );

        let mut unsupported_authority_version = envelope.to_bytes().into_vec();
        unsupported_authority_version[8] = 0;
        unsupported_authority_version[9] = 2;
        assert_eq!(
            UntrustedEnvelopeV1::decode(&unsupported_authority_version),
            Err(CertificateErrorV1::UnsupportedAuthorityVersion)
        );

        let mut future_schema = envelope.to_bytes().into_vec();
        future_schema[4] = 0;
        future_schema[5] = 2;
        assert_eq!(
            UntrustedEnvelopeV1::decode(&future_schema),
            Err(CertificateErrorV1::UnsupportedSchema)
        );

        let mut invalid_utf8 = envelope.to_bytes().into_vec();
        invalid_utf8[12] = 0xff;
        assert_eq!(
            UntrustedEnvelopeV1::decode(&invalid_utf8),
            Err(CertificateErrorV1::InvalidUtf8)
        );

        let mut nul_text = envelope.to_bytes().into_vec();
        nul_text[12] = 0;
        assert_eq!(
            UntrustedEnvelopeV1::decode(&nul_text),
            Err(CertificateErrorV1::InvalidLength)
        );

        let mut invalid_payload_type = envelope.to_bytes().into_vec();
        let payload_type_offset = invalid_payload_type
            .windows(4)
            .position(|window| window == b"body")
            .unwrap()
            - 7;
        invalid_payload_type[payload_type_offset] = 0x7f;
        assert_eq!(
            UntrustedEnvelopeV1::decode(&invalid_payload_type),
            Err(CertificateErrorV1::InvalidPayloadType)
        );

        let mut unsupported_payload_version = envelope.to_bytes().into_vec();
        unsupported_payload_version[payload_type_offset + 1] = 0;
        unsupported_payload_version[payload_type_offset + 2] = 2;
        assert_eq!(
            UntrustedEnvelopeV1::decode(&unsupported_payload_version),
            Err(CertificateErrorV1::UnsupportedPayloadVersion)
        );

        let mut zero_payload_length = envelope.to_bytes().into_vec();
        zero_payload_length[payload_type_offset + 3..payload_type_offset + 7]
            .copy_from_slice(&0_u32.to_be_bytes());
        assert_eq!(
            UntrustedEnvelopeV1::decode(&zero_payload_length),
            Err(CertificateErrorV1::InvalidLength)
        );

        let mut non_canonical_revision = envelope.to_bytes().into_vec();
        let revision_offset = non_canonical_revision
            .windows(REVISION.len())
            .position(|window| window == REVISION.as_bytes())
            .unwrap();
        non_canonical_revision[revision_offset] = b'A';
        assert_eq!(
            UntrustedEnvelopeV1::decode(&non_canonical_revision),
            Err(CertificateErrorV1::NonCanonicalRevision)
        );
    }

    #[test]
    fn malformed_and_resource_inputs_are_bounded_and_redacted() {
        assert_eq!(
            UntrustedEnvelopeV1::decode(&[1, 2, 3]),
            Err(CertificateErrorV1::TruncatedInput)
        );
        assert_eq!(
            UntrustedEnvelopeV1::decode(&vec![0; MAX_ENVELOPE_BYTES_V1 + 1]),
            Err(CertificateErrorV1::ResourceLimitExceeded)
        );
        assert!(CertificateErrorV1::PayloadDigestMismatch.to_string().len() <= 256);
        let (_, _, attestation, envelope) = fixture("ctx", b"secret-body");
        assert!(!format!("{attestation:?}").contains("secret"));
        assert!(!format!("{envelope:?}").contains("secret"));
    }

    #[test]
    fn capacity_refuses_without_eviction_and_exact_duplicate_still_noops() {
        let (key, _, attestation, envelope) = fixture("ctx", b"body");
        let decoded = decode_fixture(&envelope);
        let mut state = AdmissionStateV1::new();
        assert_eq!(
            state.admit(&decoded, &key, Some(&attestation)),
            Ok(AdmissionOutcomeV1::Accepted)
        );
        let (_, _, conflicting_attestation, conflicting_envelope) = fixture("ctx", b"other-body");
        let conflicting = decode_fixture(&conflicting_envelope);
        assert_eq!(
            state.admit(&conflicting, &key, Some(&conflicting_attestation)),
            Err(CertificateErrorV1::BindingConflict)
        );
        assert_eq!(state.len(), 1);
        state.accounted_bytes = MAX_ADMISSION_BYTES_V1;
        assert_eq!(
            state.admit(&decoded, &key, Some(&attestation)),
            Ok(AdmissionOutcomeV1::DuplicateNoop)
        );
        let (second_key, _, second_attestation, second_envelope) = fixture("other", b"other-body");
        let second = decode_fixture(&second_envelope);
        assert_eq!(
            state.admit(&second, &second_key, Some(&second_attestation)),
            Err(CertificateErrorV1::AdmissionCapacityExceeded)
        );
        assert_eq!(state.len(), 1);
    }

    #[test]
    fn single_writer_model_has_one_accept_and_duplicate_replays() {
        let (key, _, attestation, envelope) = fixture("ctx", b"body");
        let decoded = Arc::new(decode_fixture(&envelope));
        let key = Arc::new(key);
        let attestation = Arc::new(attestation);
        let state = Arc::new(Mutex::new(AdmissionStateV1::new()));
        let mut handles = Vec::new();
        for _ in 0..8 {
            let state = Arc::clone(&state);
            let decoded = Arc::clone(&decoded);
            let key = Arc::clone(&key);
            let attestation = Arc::clone(&attestation);
            handles.push(thread::spawn(move || {
                state
                    .lock()
                    .unwrap()
                    .admit(&decoded, &key, Some(&attestation))
                    .unwrap()
            }));
        }
        let outcomes = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| **outcome == AdmissionOutcomeV1::Accepted)
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| **outcome == AdmissionOutcomeV1::DuplicateNoop)
                .count(),
            7
        );
        assert_eq!(state.lock().unwrap().len(), 1);
    }

    #[test]
    fn forged_attestation_cannot_be_recomputed_from_a_different_tuple() {
        let (key, _, _, envelope) = fixture("ctx", b"body");
        let decoded = decode_fixture(&envelope);
        let foreign_payload = producer_payload_v1(b"body").unwrap();
        let foreign_key = AdmissionKeyV1::try_new(
            "labcolors-core-test",
            CertificateOperationV1::IssueCertificate,
            "other-context",
            REVISION,
            [0x11; 32],
        )
        .unwrap();
        let foreign_attestation = producer_attestation_v1(foreign_key, &foreign_payload).unwrap();
        let mut state = AdmissionStateV1::new();
        assert_eq!(
            state.admit(&decoded, &key, Some(&foreign_attestation)),
            Err(CertificateErrorV1::ProducerBindingMismatch)
        );
        assert!(state.is_empty());
    }

    #[test]
    fn canonical_producer_revision_rejects_uppercase_and_abbreviated_values() {
        assert_eq!(
            AdmissionKeyV1::try_new(
                "runtime",
                CertificateOperationV1::IssueCertificate,
                "ctx",
                "0123456789ABCDEF0123456789abcdef01234567",
                [0; 32],
            ),
            Err(CertificateErrorV1::NonCanonicalRevision)
        );
        assert_eq!(
            AdmissionKeyV1::try_new(
                "runtime",
                CertificateOperationV1::IssueCertificate,
                "ctx",
                "0123456789abcdef",
                [0; 32],
            ),
            Err(CertificateErrorV1::NonCanonicalRevision)
        );
    }
}
