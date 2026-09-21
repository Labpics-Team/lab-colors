//! Выпуск транспортного сертификата для встроенного дескриптора дерева Core.
//!
//! Идентичность исходного дерева проверяет сборщик. Этот модуль использует
//! только его неизменяемый результат; он не удостоверяет исполняемый файл,
//! параметры компиляции, отправителя или семантику цветовых вычислений.

use core::fmt;

use super::{
    AdmissionKeyV1, CertificateEnvelopeV1, CertificateErrorV1, CertificateOperationV1,
    TrustedProducerAttestationV1, is_canonical_revision, producer_attestation_v1,
    producer_payload_v1,
};
use crate::sha256::Hasher;

const DESCRIPTOR_BYTES_V1: usize = 47;
const DESCRIPTOR_PREFIX_V1: &[u8; 7] = b"LCST\x00\x01\x01";
const DESCRIPTOR_DOMAIN_V1: &[u8] = b"labpics.colors/core-source-tree-descriptor/v1\0";
const RUNTIME_PREFIX_V1: &[u8] = b"labcolors-core:source-tree-v1:";
const CONTEXT_ID_V1: &str = "core-source-tree-transport-v1";
const SOURCE_DESCRIPTOR_V1: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/certificate-source-tree-v1.bin"));

/// Отказ выпуска сертификата без данных из дескриптора или конверта.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CertificateProducerErrorV1 {
    /// Сборщик не предоставил доступную каноническую идентичность дерева Core.
    ProducerIdentityUnavailable,
    /// Типизированный отказ существующей границы транспортного конверта.
    Envelope(CertificateErrorV1),
}

impl CertificateProducerErrorV1 {
    /// Статический машинный код; не содержит путей, идентификаторов или байтов.
    pub const fn code(self) -> &'static str {
        match self {
            Self::ProducerIdentityUnavailable => "producer_identity_unavailable",
            Self::Envelope(error) => error.code(),
        }
    }
}

impl From<CertificateErrorV1> for CertificateProducerErrorV1 {
    fn from(error: CertificateErrorV1) -> Self {
        Self::Envelope(error)
    }
}

impl fmt::Debug for CertificateProducerErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl fmt::Display for CertificateProducerErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl std::error::Error for CertificateProducerErrorV1 {}

/// Канонический конверт и соответствующее закрытое свидетельство производителя.
pub struct SourceCertificateV1 {
    envelope: CertificateEnvelopeV1,
    attestation: TrustedProducerAttestationV1,
}

impl SourceCertificateV1 {
    /// Заимствует канонические байты конверта без передачи свидетельства.
    pub fn as_bytes(&self) -> &[u8] {
        self.envelope.as_bytes()
    }

    /// Копирует байты конверта с типизированным отказом при нехватке памяти.
    pub fn try_to_bytes(&self) -> Result<Vec<u8>, CertificateProducerErrorV1> {
        self.envelope.try_to_bytes().map_err(Into::into)
    }

    /// Заимствует точный кортеж производителя для сопоставления при допуске.
    pub fn admission_key(&self) -> &AdmissionKeyV1 {
        self.envelope.admission_key()
    }

    /// Заимствует непередаваемое через сериализацию свидетельство для допуска.
    pub const fn attestation(&self) -> &TrustedProducerAttestationV1 {
        &self.attestation
    }
}

impl fmt::Debug for SourceCertificateV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SourceCertificateV1 { .. }")
    }
}

/// Выпускает сертификат встроенного дерева исходников Core без входных параметров.
///
/// Источник идентичности — только результат сборщика. Пустой или повреждённый
/// дескриптор даёт `ProducerIdentityUnavailable`, сохраняя доступность декодера.
pub fn issue_source_certificate_v1() -> Result<SourceCertificateV1, CertificateProducerErrorV1> {
    issue_from_descriptor(SOURCE_DESCRIPTOR_V1)
}

struct CoreSourceTreeDescriptorV1<'a> {
    bytes: &'a [u8; DESCRIPTOR_BYTES_V1],
    revision: &'a str,
}

impl<'a> CoreSourceTreeDescriptorV1<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, CertificateProducerErrorV1> {
        let unavailable = CertificateProducerErrorV1::ProducerIdentityUnavailable;
        let bytes: &[u8; DESCRIPTOR_BYTES_V1] = bytes.try_into().map_err(|_| unavailable)?;
        let revision_bytes = &bytes[DESCRIPTOR_PREFIX_V1.len()..];
        if !bytes.starts_with(DESCRIPTOR_PREFIX_V1) || !is_canonical_revision(revision_bytes) {
            return Err(unavailable);
        }
        let revision = core::str::from_utf8(revision_bytes).map_err(|_| unavailable)?;
        Ok(Self { bytes, revision })
    }

    fn content_identity(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(DESCRIPTOR_DOMAIN_V1);
        hasher.update(self.bytes);
        *hasher.finalize().as_bytes()
    }
}

fn issue_from_descriptor(bytes: &[u8]) -> Result<SourceCertificateV1, CertificateProducerErrorV1> {
    let descriptor = CoreSourceTreeDescriptorV1::parse(bytes)?;
    let identity = descriptor.content_identity();
    let mut runtime_id = [0_u8; RUNTIME_PREFIX_V1.len() + 64];
    runtime_id[..RUNTIME_PREFIX_V1.len()].copy_from_slice(RUNTIME_PREFIX_V1);
    let hex = b"0123456789abcdef";
    for (byte, pair) in identity
        .iter()
        .zip(runtime_id[RUNTIME_PREFIX_V1.len()..].chunks_exact_mut(2))
    {
        pair[0] = hex[usize::from(byte >> 4)];
        pair[1] = hex[usize::from(byte & 0x0f)];
    }
    let runtime_id =
        core::str::from_utf8(&runtime_id).map_err(|_| CertificateErrorV1::InvalidUtf8)?;
    let key = AdmissionKeyV1::try_new(
        runtime_id,
        CertificateOperationV1::IssueCertificate,
        CONTEXT_ID_V1,
        descriptor.revision,
        identity,
    )?;
    let payload = producer_payload_v1(&[0x01])?;
    // Одно свидетельство расходуется при выпуске; второе создаём тем же закрытым
    // владельцем для последующего допуска, не вводя публичного копирования.
    let attestation = producer_attestation_v1(key.try_clone()?, &payload)?;
    let issuance_attestation = producer_attestation_v1(key, &payload)?;
    let envelope =
        CertificateEnvelopeV1::issue_from_trusted_producer(payload, issuance_attestation)?;
    Ok(SourceCertificateV1 {
        envelope,
        attestation,
    })
}

#[cfg(test)]
mod tests {
    use super::super::{AdmissionOutcomeV1, AdmissionStateV1, UntrustedEnvelopeV1};
    use super::*;

    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
    const DESCRIPTOR: &[u8] = b"LCST\x00\x01\x010123456789abcdef0123456789abcdef01234567";
    const RUNTIME: &str = "labcolors-core:source-tree-v1:833e06c7286019744c0480941a7837574fda7a4a5ec625fa3c54e110520c557f";
    const CONTENT_ID: &str = "833e06c7286019744c0480941a7837574fda7a4a5ec625fa3c54e110520c557f";
    // Эталон собран независимо через Python struct и hashlib.sha256.
    // Production encoder, хешер и поля выпущенного объекта его не задают.
    const ENVELOPE_HEX: &str = concat!(
        "4c43454e000101000001005e6c6162636f6c6f72732d636f72653a736f757263",
        "652d747265652d76313a38333365303663373238363031393734346330343830",
        "3934316137383337353734666461376134613565633632356661336335346531",
        "3130353230633535376600283031323334353637383961626364656630313233",
        "3435363738396162636465663031323334353637833e06c7286019744c048094",
        "1a7837574fda7a4a5ec625fa3c54e110520c557f001d636f72652d736f757263",
        "652d747265652d7472616e73706f72742d76310100010000000101590ce8e5cb",
        "bfe24ef73faef26f7f2d4fbdb7b837ba892a42fda4696726eb42fe0a6777afe5",
        "7b12d2ed20eb8e3d9f5ca910b0d25af54e822f34fd7467836206c8",
    );

    fn hex_bytes(value: &str) -> Vec<u8> {
        value
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    fn expected_key() -> AdmissionKeyV1 {
        AdmissionKeyV1::try_new(
            RUNTIME,
            CertificateOperationV1::IssueCertificate,
            "core-source-tree-transport-v1",
            REVISION,
            hex_bytes(CONTENT_ID).try_into().unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn source_certificate_matches_independent_exact_wire_vector() {
        let issued = issue_from_descriptor(DESCRIPTOR).unwrap();
        let expected = hex_bytes(ENVELOPE_HEX);
        assert_eq!(expected.len(), 283);
        assert_eq!(issued.as_bytes(), expected);
        assert_eq!(issued.try_to_bytes().unwrap(), expected);
        assert_eq!(issued.admission_key(), &expected_key());
        let decoded = UntrustedEnvelopeV1::decode(issued.as_bytes()).unwrap();
        assert_eq!(decoded.admission_key(), &expected_key());
        assert_eq!(decoded.payload_len(), 1);
        assert_eq!(
            decoded.payload_sha256().as_slice(),
            hex_bytes("590ce8e5cbbfe24ef73faef26f7f2d4fbdb7b837ba892a42fda4696726eb42fe")
        );
        assert_eq!(
            decoded.binding_sha256().as_slice(),
            hex_bytes("0a6777afe57b12d2ed20eb8e3d9f5ca910b0d25af54e822f34fd7467836206c8")
        );
    }

    #[test]
    fn unavailable_and_malformed_descriptors_never_issue() {
        let unavailable = CertificateProducerErrorV1::ProducerIdentityUnavailable;
        for length in 0..DESCRIPTOR.len() {
            assert_eq!(
                issue_from_descriptor(&DESCRIPTOR[..length]).unwrap_err(),
                unavailable
            );
        }
        let mut trailing = DESCRIPTOR.to_vec();
        trailing.push(0);
        assert_eq!(issue_from_descriptor(&trailing).unwrap_err(), unavailable);
        for offset in 0..7 {
            let mut malformed = DESCRIPTOR.to_vec();
            malformed[offset] ^= 0xff;
            assert_eq!(issue_from_descriptor(&malformed).unwrap_err(), unavailable);
        }
        for offset in 7..DESCRIPTOR.len() {
            for invalid in [b'A', b'g', b'/', 0, 0xff] {
                let mut malformed = DESCRIPTOR.to_vec();
                malformed[offset] = invalid;
                assert_eq!(issue_from_descriptor(&malformed).unwrap_err(), unavailable);
            }
        }
        let mut sha256_id = DESCRIPTOR.to_vec();
        sha256_id.extend_from_slice(b"0123456789abcdef01234567");
        assert_eq!(issue_from_descriptor(&sha256_id).unwrap_err(), unavailable);
    }

    #[test]
    fn issued_capability_requires_explicit_matching_expected_key_and_replays_once() {
        let issued = issue_from_descriptor(DESCRIPTOR).unwrap();
        let decoded = UntrustedEnvelopeV1::decode(issued.as_bytes()).unwrap();
        let expected = expected_key();
        let mut state = AdmissionStateV1::new();
        assert_eq!(
            state.admit(&decoded, &expected, None),
            Err(CertificateErrorV1::MissingProducerAttestation)
        );
        assert!(state.is_empty());
        assert_eq!(
            state.admit(&decoded, &expected, Some(issued.attestation())),
            Ok(AdmissionOutcomeV1::Accepted)
        );
        let prior_bytes = state.accounted_bytes();
        let wrong_context = AdmissionKeyV1::try_new(
            RUNTIME,
            CertificateOperationV1::IssueCertificate,
            "different-context",
            REVISION,
            hex_bytes(CONTENT_ID).try_into().unwrap(),
        )
        .unwrap();
        assert_eq!(
            state.admit(&decoded, &wrong_context, Some(issued.attestation())),
            Err(CertificateErrorV1::ContextMismatch)
        );
        let mut foreign_descriptor = DESCRIPTOR.to_vec();
        foreign_descriptor[7] = b'f';
        let foreign = issue_from_descriptor(&foreign_descriptor).unwrap();
        assert_eq!(
            state.admit(&decoded, &expected, Some(foreign.attestation())),
            Err(CertificateErrorV1::ProducerBindingMismatch)
        );
        assert_eq!(
            state.admit(&decoded, &expected, Some(issued.attestation())),
            Ok(AdmissionOutcomeV1::DuplicateNoop)
        );
        assert_eq!(state.len(), 1);
        assert_eq!(state.accounted_bytes(), prior_bytes);
    }

    #[test]
    fn source_debug_and_errors_are_static_and_redacted() {
        let issued = issue_from_descriptor(DESCRIPTOR).unwrap();
        assert_eq!(format!("{issued:?}"), "SourceCertificateV1 { .. }");
        assert_eq!(
            format!("{:?}", issued.attestation()),
            "TrustedProducerAttestationV1 { .. }"
        );
        for (error, code) in [
            (
                CertificateProducerErrorV1::ProducerIdentityUnavailable,
                "producer_identity_unavailable",
            ),
            (
                CertificateProducerErrorV1::from(CertificateErrorV1::ResourceLimitExceeded),
                "resource_limit_exceeded",
            ),
            (
                CertificateProducerErrorV1::from(CertificateErrorV1::InvalidUtf8),
                "invalid_utf8",
            ),
        ] {
            assert_eq!(error.code(), code);
            assert_eq!(format!("{error}"), code);
            assert_eq!(format!("{error:?}"), code);
        }
    }
}
