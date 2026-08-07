//! Content-bound transport boundary одного точного family image.
//!
//! Semantic release описывает семейство независимо от хранения. Artifact
//! receipt отдельно связывает envelope, codec и payload. Loader допускает bytes
//! в allocation-free executable RawBitmap24 view, не материализуя второе множество.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "V5b2a keeps the exact artifact loader private until the public family provider cutover"
    )
)]

use core::fmt;

mod raw_bitmap24;

#[cfg(test)]
pub(crate) use raw_bitmap24::PAYLOAD_LEN_V1 as RAW_BITMAP24_PAYLOAD_LEN_V1;
#[cfg(test)]
pub(crate) use raw_bitmap24::encode_for_test as encode_raw_bitmap24_family_artifact_v2_for_test;

use crate::family::{
    CanonicalFamilyImageDigestV2, FamilyDeclarationV2, FamilyDefinitionDigestV2,
    FamilyMembershipMeasurementV2, FamilyMembershipPassV1, FamilyMembershipViolationV1,
    SemanticFamilyReleaseIdV2, semantic_family_release_id_v2,
};
#[cfg(test)]
use crate::family::{CanonicalFamilyImageErrorV2, canonical_family_image_digest_v2};
use crate::lcs_occurrence::ColorSignal;
#[cfg(test)]
use crate::lcs_occurrence::OutputProfileId;
use crate::sha256::Hasher;

const MAGIC_V2: &[u8; 8] = b"LCFAM2\0\0";
const ENVELOPE_RELEASE_V2: u8 = 2;
const SIGNAL_DOMAIN_SRGB8_D65_V1: u8 = 1;
// Полная кардинальность sRGB8: большее exact-множество обязано повторить сигнал.
const MAX_SRGB8_MEMBER_COUNT_V1: u64 = 1 << 24;
const SIGNAL_ORDINAL_RGB_BIG_ENDIAN_V1: u8 = 1;
// Loader не минтит и не перепроверяет proof: он принимает только
// certificate, точно равный trusted expected value от внешнего registry.
const PROOF_RELEASE_EXPECTED_EXACT_IMAGE_V1: u8 = 1;
const VERIFIER_RELEASE_EXPECTED_REPLAY_V1: u8 = 1;
// magic + 6 release/domain tags + 6 SHA-256 identities + 2 u64 lengths + receipt.
// Любое изменение certificate layout обязано синхронно менять этот размер.
const HEADER_LEN_V2: usize = 254;
/// Доверенная запись сертификата — это ровно заголовок артефакта.
///
/// Загрузчик сверяет её байт в байт с тем, что несёт артефакт, поэтому запись
/// приходит по доверенному каналу, а payload — по любому. Отдельный формат для
/// неё не нужен и был бы вторым источником истины.
pub(crate) const FAMILY_CERTIFICATE_RECORD_LEN_V2: usize = HEADER_LEN_V2;
/// Тело сертификата — запись без магии.
///
/// Размер выражен типом, а не `debug_assert`: в release-сборке утверждение
/// вырезается, и третий вызывающий получил бы панику вместо отказа.
const CERTIFICATE_BODY_LEN_V2: usize = HEADER_LEN_V2 - 8;
const PAYLOAD_DIGEST_DOMAIN_V2: &[u8] = b"labcolors.family-artifact-payload.v2\0";
const RECEIPT_DOMAIN_V2: &[u8] = b"labcolors.family-artifact-receipt.v2\0";
const FIXTURE_PROOF_ARTIFACT_DOMAIN_V2: &[u8] = b"labcolors.family-artifact-fixture-proof.v2\0";
const FIXTURE_VERIFIER_IDENTITY_DOMAIN_V2: &[u8] =
    b"labcolors.family-artifact-fixture-verifier.v2\0";

#[cfg(test)]
pub(crate) use decoder_counter::FAMILY_ARTIFACT_DECODER_CALLS;
#[cfg(test)]
pub(crate) use decoder_counter::FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS;

#[cfg(test)]
mod decoder_counter {
    std::thread_local! {
        pub(crate) static FAMILY_ARTIFACT_DECODER_CALLS: core::cell::Cell<usize> =
            const { core::cell::Cell::new(0) };
        pub(crate) static FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS: core::cell::Cell<usize> =
            const { core::cell::Cell::new(0) };
    }
}

/// Content address конкретного artifact representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FamilyArtifactReceiptIdV2([u8; 32]);

impl FamilyArtifactReceiptIdV2 {
    pub(crate) const fn from_digest(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Content identity exact proof artifact, отдельно от proof algorithm release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyProofArtifactIdV2([u8; 32]);

impl FamilyProofArtifactIdV2 {
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Content identity независимо поставленного verifier implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyVerifierIdentityV2([u8; 32]);

impl FamilyVerifierIdentityV2 {
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Отдельный certificate semantic release и его transport representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyImageCertificateV2 {
    envelope_release: u8,
    codec_release: u8,
    signal_domain: u8,
    signal_ordinal: u8,
    proof_release: u8,
    verifier_release: u8,
    proof_artifact: FamilyProofArtifactIdV2,
    verifier_identity: FamilyVerifierIdentityV2,
    definition_digest: FamilyDefinitionDigestV2,
    image_digest: CanonicalFamilyImageDigestV2,
    semantic_release: SemanticFamilyReleaseIdV2,
    payload_digest: [u8; 32],
    member_count: u64,
    payload_len: u64,
    artifact_receipt: FamilyArtifactReceiptIdV2,
}

impl FamilyImageCertificateV2 {
    /// Разбирает доверенную запись сертификата, полученную извне.
    ///
    /// Это единственный способ предъявить загрузчику `expected`, не имея на
    /// руках самого артефакта, — и потому единственное место, где ядро может
    /// узнать, что считать доверенным. Само оно этого не решает: запись
    /// приходит от потребителя. Встроенный реестр семейств здесь означал бы
    /// возврат к именованным ролям внутри ядра.
    pub(crate) fn parse_trusted(bytes: &[u8]) -> Result<Self, FamilyArtifactLoadErrorV1> {
        // Same order the envelope refuses in: too short, then foreign magic,
        // then the exact length.  A record and an artifact that disagree
        // about which refusal comes first would make one of the two lie.
        if bytes.len() < FAMILY_CERTIFICATE_RECORD_LEN_V2 {
            return Err(FamilyArtifactLoadErrorV1::HeaderTooShort);
        }
        if bytes.get(..MAGIC_V2.len()) != Some(MAGIC_V2) {
            return Err(FamilyArtifactLoadErrorV1::InvalidMagic);
        }
        if bytes.len() != FAMILY_CERTIFICATE_RECORD_LEN_V2 {
            return Err(FamilyArtifactLoadErrorV1::ExactLengthMismatch {
                expected: FAMILY_CERTIFICATE_RECORD_LEN_V2,
                actual: bytes.len(),
            });
        }
        let Ok(body) = <&[u8; CERTIFICATE_BODY_LEN_V2]>::try_from(
            &bytes[MAGIC_V2.len()..FAMILY_CERTIFICATE_RECORD_LEN_V2],
        ) else {
            return Err(FamilyArtifactLoadErrorV1::HeaderTooShort);
        };
        let certificate = admit_certificate_discriminants_v2(decode_certificate(body))?;
        if usize::try_from(certificate.payload_len).is_err() {
            return Err(FamilyArtifactLoadErrorV1::ResourceExhausted);
        }
        // The envelope proves these two before it admits anything; a record
        // that skipped them would pass here and then be reported as a foreign
        // artifact — blaming the payload for a broken record.
        if artifact_receipt(certificate) != certificate.artifact_receipt {
            return Err(FamilyArtifactLoadErrorV1::ArtifactReceiptMismatch);
        }
        if semantic_family_release_id_v2(
            certificate.definition_digest,
            certificate.image_digest,
            certificate.member_count,
        ) != certificate.semantic_release
        {
            return Err(FamilyArtifactLoadErrorV1::SemanticReleaseMismatch);
        }
        Ok(certificate)
    }

    /// Адрес определения, которому обязан отвечать образ.
    ///
    /// Потребитель сверяет его с `ContextualRegionFamilyProviderV1`, иначе
    /// доверенная запись говорит лишь «этот артефакт цел», но не «этот
    /// артефакт — образ того региона, который я спрашивал».
    pub(crate) const fn definition_digest(self) -> FamilyDefinitionDigestV2 {
        self.definition_digest
    }

    pub(crate) const fn member_count(self) -> u64 {
        self.member_count
    }

    pub(crate) const fn semantic_release(self) -> SemanticFamilyReleaseIdV2 {
        self.semantic_release
    }

    pub(crate) const fn image_digest(self) -> CanonicalFamilyImageDigestV2 {
        self.image_digest
    }

    pub(crate) const fn artifact_receipt(self) -> FamilyArtifactReceiptIdV2 {
        self.artifact_receipt
    }

    pub(crate) const fn proof_artifact(self) -> FamilyProofArtifactIdV2 {
        self.proof_artifact
    }

    pub(crate) const fn verifier_identity(self) -> FamilyVerifierIdentityV2 {
        self.verifier_identity
    }

    #[cfg(test)]
    pub(crate) fn identity_mutants_for_test(self) -> Vec<Self> {
        let mut mutants = Vec::with_capacity(11);

        let mut codec = self;
        codec.codec_release = match codec.codec_release {
            0xF1 => 0xF2,
            _ => 0xF1,
        };
        mutants.push(codec);

        let mut definition = self;
        let mut definition_bytes = *definition.definition_digest.as_bytes();
        definition_bytes[0] ^= 1;
        definition.definition_digest = FamilyDefinitionDigestV2::from_digest(definition_bytes);
        mutants.push(definition);

        let mut image = self;
        let mut image_bytes = *image.image_digest.as_bytes();
        image_bytes[0] ^= 1;
        image.image_digest = CanonicalFamilyImageDigestV2::from_digest(image_bytes);
        mutants.push(image);

        let mut semantic = self;
        let mut semantic_bytes = *semantic.semantic_release.as_bytes();
        semantic_bytes[0] ^= 1;
        semantic.semantic_release = SemanticFamilyReleaseIdV2::from_digest(semantic_bytes);
        mutants.push(semantic);

        let mut payload = self;
        payload.payload_digest[0] ^= 1;
        mutants.push(payload);

        let mut member_count = self;
        member_count.member_count ^= 1;
        mutants.push(member_count);

        let mut payload_len = self;
        payload_len.payload_len ^= 1;
        mutants.push(payload_len);

        let mut proof = self;
        proof.proof_release ^= 1;
        mutants.push(proof);

        let mut proof_artifact = self;
        proof_artifact.proof_artifact.0[0] ^= 1;
        mutants.push(proof_artifact);

        let mut verifier_identity = self;
        verifier_identity.verifier_identity.0[0] ^= 1;
        mutants.push(verifier_identity);

        let mut receipt = self;
        receipt.artifact_receipt.0[0] ^= 1;
        mutants.push(receipt);
        mutants
    }

    #[cfg(test)]
    pub(crate) fn receipt_mismatch_for_test(mut self) -> Self {
        self.artifact_receipt.0[0] ^= 1;
        self
    }

    #[cfg(test)]
    pub(crate) fn semantic_mismatch_with_valid_receipt_for_test(mut self) -> Self {
        let mut bytes = *self.semantic_release.as_bytes();
        bytes[0] ^= 1;
        self.semantic_release = SemanticFamilyReleaseIdV2::from_digest(bytes);
        self.artifact_receipt = artifact_receipt(self);
        self
    }

    #[cfg(test)]
    pub(crate) fn image_mismatch_with_coherent_certificate_for_test(mut self) -> Self {
        let mut bytes = *self.image_digest.as_bytes();
        bytes[0] ^= 1;
        self.image_digest = CanonicalFamilyImageDigestV2::from_digest(bytes);
        self.semantic_release = semantic_family_release_id_v2(
            self.definition_digest,
            self.image_digest,
            self.member_count,
        );
        self.artifact_receipt = artifact_receipt(self);
        self
    }

    #[cfg(test)]
    pub(crate) fn codec_with_valid_receipt_for_test(mut self, codec_release: u8) -> Self {
        self.codec_release = codec_release;
        self.artifact_receipt = artifact_receipt(self);
        self
    }

    #[cfg(test)]
    pub(crate) fn member_count_with_coherent_certificate_for_test(
        mut self,
        member_count: u64,
    ) -> Self {
        self.member_count = member_count;
        self.semantic_release = semantic_family_release_id_v2(
            self.definition_digest,
            self.image_digest,
            self.member_count,
        );
        self.artifact_receipt = artifact_receipt(self);
        self
    }
}

/// Owned transport bytes. Admission never borrows host storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EncodedFamilyArtifactV2(Box<[u8]>);

impl EncodedFamilyArtifactV2 {
    /// Принимает exact owned bytes от transport adapter без заимствования.
    pub(crate) const fn from_owned_bytes(bytes: Box<[u8]>) -> Self {
        Self(bytes)
    }

    /// Возвращает те же bytes для исправления, хранения или другой admission.
    pub(crate) fn into_bytes(self) -> Box<[u8]> {
        self.0
    }

    #[cfg(test)]
    pub(crate) fn allocation_ptr_for_test(&self) -> *const u8 {
        self.0.as_ptr()
    }

    #[cfg(test)]
    pub(crate) fn payload_byte_for_test(&self, index: usize) -> u8 {
        self.0[HEADER_LEN_V2 + index]
    }

    #[cfg(test)]
    pub(crate) fn from_raw_bytes_for_test(bytes: Vec<u8>) -> Self {
        Self(bytes.into_boxed_slice())
    }

    #[cfg(test)]
    pub(crate) fn flip_first_payload_bit_for_test(&mut self) {
        self.0[HEADER_LEN_V2] ^= 1;
    }

    #[cfg(test)]
    pub(crate) fn truncate_one_byte_for_test(self) -> Self {
        let mut bytes = self.0.into_vec();
        bytes.pop();
        Self(bytes.into_boxed_slice())
    }

    #[cfg(test)]
    pub(crate) fn extend_one_byte_for_test(self) -> Self {
        let mut bytes = self.0.into_vec();
        bytes.push(0);
        Self(bytes.into_boxed_slice())
    }

    #[cfg(test)]
    pub(crate) fn with_certificate_for_test(
        mut self,
        certificate: FamilyImageCertificateV2,
    ) -> Self {
        let mut encoded = Vec::with_capacity(HEADER_LEN_V2 - MAGIC_V2.len());
        encode_certificate(&mut encoded, certificate);
        self.0[MAGIC_V2.len()..HEADER_LEN_V2].copy_from_slice(&encoded);
        self
    }

    #[cfg(test)]
    pub(crate) fn corrupt_magic_for_test(mut self) -> Self {
        self.0[0] ^= 1;
        self
    }

    #[cfg(test)]
    pub(crate) fn corrupt_envelope_field_for_test(mut self, field: FixtureEnvelopeFieldV1) -> Self {
        let offset = match field {
            FixtureEnvelopeFieldV1::EnvelopeRelease => 0,
            FixtureEnvelopeFieldV1::SignalDomain => 2,
            FixtureEnvelopeFieldV1::SignalOrdinal => 3,
            FixtureEnvelopeFieldV1::ProofRelease => 4,
            FixtureEnvelopeFieldV1::VerifierRelease => 5,
        };
        self.0[MAGIC_V2.len() + offset] ^= 1;
        self
    }

    #[cfg(test)]
    pub(crate) fn truncate_inside_header_for_test(self) -> Self {
        Self(self.0[..HEADER_LEN_V2 - 1].into())
    }

    #[cfg(test)]
    pub(crate) fn set_raw_bitmap_member_for_test(&mut self, rgb: [u8; 3], member: bool) {
        raw_bitmap24::set_member(&mut self.0[HEADER_LEN_V2..], rgb, member);
    }

    #[cfg(test)]
    pub(crate) fn resize_payload_for_test(&mut self, payload_len: usize) {
        let mut bytes = core::mem::take(&mut self.0).into_vec();
        bytes.resize(HEADER_LEN_V2 + payload_len, 0);
        self.0 = bytes.into_boxed_slice();
    }

    #[cfg(test)]
    pub(crate) fn reseal_payload_for_test(
        &mut self,
        mut certificate: FamilyImageCertificateV2,
    ) -> FamilyImageCertificateV2 {
        let payload = &self.0[HEADER_LEN_V2..];
        certificate.payload_len = payload.len() as u64;
        certificate.payload_digest = payload_digest(payload);
        certificate.artifact_receipt = artifact_receipt(certificate);
        let mut encoded = Vec::with_capacity(HEADER_LEN_V2 - MAGIC_V2.len());
        encode_certificate(&mut encoded, certificate);
        self.0[MAGIC_V2.len()..HEADER_LEN_V2].copy_from_slice(&encoded);
        certificate
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FixtureEnvelopeFieldV1 {
    EnvelopeRelease,
    SignalDomain,
    SignalOrdinal,
    ProofRelease,
    VerifierRelease,
}

/// Executable storage after all envelope, digest and semantic checks.
pub(crate) struct AdmittedFamilyArtifactV2 {
    certificate: FamilyImageCertificateV2,
    storage: ExecutableFamilyStorageV2,
    #[cfg(test)]
    drop_probe: Option<ArtifactDropProbeV1>,
}

enum ExecutableFamilyStorageV2 {
    RawBitmap24V1(raw_bitmap24::RawBitmap24V1),
    #[cfg(test)]
    FixtureMembersV1(Box<[ColorSignal]>),
}

impl fmt::Debug for AdmittedFamilyArtifactV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AdmittedFamilyArtifactV2")
            .field("semantic_release", &self.semantic_release())
            .field("artifact_receipt", &self.artifact_receipt())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[derive(Debug)]
struct ArtifactDropProbeV1(std::rc::Rc<core::cell::Cell<usize>>);

#[cfg(test)]
impl Drop for ArtifactDropProbeV1 {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

impl AdmittedFamilyArtifactV2 {
    pub(crate) const fn semantic_release(&self) -> SemanticFamilyReleaseIdV2 {
        self.certificate.semantic_release
    }

    pub(crate) const fn artifact_receipt(&self) -> FamilyArtifactReceiptIdV2 {
        self.certificate.artifact_receipt
    }

    pub(crate) fn contains(&self, signal: ColorSignal) -> bool {
        match &self.storage {
            ExecutableFamilyStorageV2::RawBitmap24V1(bitmap) => bitmap.contains(signal),
            #[cfg(test)]
            ExecutableFamilyStorageV2::FixtureMembersV1(members) => members
                .binary_search_by_key(&signal_key(signal), |member| signal_key(*member))
                .is_ok(),
        }
    }

    pub(crate) fn assess(
        &self,
        signal: ColorSignal,
    ) -> (
        FamilyMembershipMeasurementV2,
        crate::constraints::HardDecision<FamilyMembershipPassV1, FamilyMembershipViolationV1>,
    ) {
        #[cfg(test)]
        crate::family::FAMILY_MEMBERSHIP_ASSESS_CALLS.with(|calls| calls.set(calls.get() + 1));
        let measurement = FamilyMembershipMeasurementV2::new(self.semantic_release(), signal);
        let decision = if self.contains(signal) {
            crate::constraints::HardDecision::Pass(FamilyMembershipPassV1)
        } else {
            crate::constraints::HardDecision::Violation(FamilyMembershipViolationV1)
        };
        (measurement, decision)
    }

    #[cfg(test)]
    pub(crate) fn with_drop_counter_for_test(
        mut self,
        counter: std::rc::Rc<core::cell::Cell<usize>>,
    ) -> Self {
        self.drop_probe = Some(ArtifactDropProbeV1(counter));
        self
    }

    #[cfg(test)]
    pub(crate) fn allocation_ptr_for_test(&self) -> Option<*const u8> {
        match &self.storage {
            ExecutableFamilyStorageV2::RawBitmap24V1(bitmap) => Some(bitmap.allocation_ptr()),
            ExecutableFamilyStorageV2::FixtureMembersV1(_) => None,
        }
    }
}

/// Loader отказывается до decoder-а при любом transport/certificate mismatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyArtifactLoadErrorV1 {
    HeaderTooShort,
    InvalidMagic,
    UnsupportedEnvelope,
    UnsupportedSignalDomain,
    UnsupportedSignalOrdinal,
    UnsupportedProofRelease,
    UnsupportedVerifierRelease,
    InvalidMemberCount,
    ExactLengthMismatch {
        expected: usize,
        actual: usize,
    },
    ForeignCertificate,
    PayloadDigestMismatch,
    ArtifactReceiptMismatch,
    SemanticReleaseMismatch,
    UnsupportedCodec,
    CodecPayloadLengthMismatch {
        codec_release: u8,
        expected: u64,
        actual: u64,
    },
    InvalidCodecPayload,
    MemberCountMismatch {
        expected: u64,
        actual: u64,
    },
    ImageDigestMismatch,
    ResourceExhausted,
}

/// Неуспешный admission возвращает исходные owned bytes для исправления,
/// повторной попытки или точной диагностики без refetch/clone.
#[derive(PartialEq, Eq)]
pub(crate) struct FamilyArtifactLoadFailureV1 {
    cause: FamilyArtifactLoadErrorV1,
    encoded: EncodedFamilyArtifactV2,
}

impl fmt::Debug for FamilyArtifactLoadFailureV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FamilyArtifactLoadFailureV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}

impl FamilyArtifactLoadFailureV1 {
    pub(crate) const fn cause(&self) -> FamilyArtifactLoadErrorV1 {
        self.cause
    }

    pub(crate) fn into_parts(self) -> (FamilyArtifactLoadErrorV1, EncodedFamilyArtifactV2) {
        (self.cause, self.encoded)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FamilyArtifactLoaderV1;

impl FamilyArtifactLoaderV1 {
    pub(crate) fn load(
        expected: FamilyImageCertificateV2,
        encoded: EncodedFamilyArtifactV2,
    ) -> Result<AdmittedFamilyArtifactV2, FamilyArtifactLoadFailureV1> {
        let verified = Self::try_load_raw_bitmap24(expected, &encoded);
        match verified {
            Ok(certificate) => Ok(AdmittedFamilyArtifactV2 {
                certificate,
                storage: ExecutableFamilyStorageV2::RawBitmap24V1(
                    raw_bitmap24::RawBitmap24V1::from_verified(encoded),
                ),
                #[cfg(test)]
                drop_probe: None,
            }),
            Err(cause) => Err(FamilyArtifactLoadFailureV1 { cause, encoded }),
        }
    }

    #[cfg(test)]
    pub(crate) fn load_fixture(
        expected: FamilyImageCertificateV2,
        encoded: EncodedFamilyArtifactV2,
    ) -> Result<AdmittedFamilyArtifactV2, FamilyArtifactLoadFailureV1> {
        Self::load_fixture_with_codec(
            expected,
            encoded,
            preflight_fixture_payload,
            decode_fixture_payload,
        )
    }

    #[cfg(test)]
    pub(crate) fn load_with_decoder_for_test(
        expected: FamilyImageCertificateV2,
        encoded: EncodedFamilyArtifactV2,
        decoder: impl FnOnce(u8, u64, &[u8]) -> Result<Box<[ColorSignal]>, FamilyArtifactLoadErrorV1>,
    ) -> Result<AdmittedFamilyArtifactV2, FamilyArtifactLoadFailureV1> {
        Self::load_fixture_with_codec(expected, encoded, preflight_fixture_payload, decoder)
    }

    #[cfg(test)]
    pub(crate) fn load_with_codec_for_test(
        expected: FamilyImageCertificateV2,
        encoded: EncodedFamilyArtifactV2,
        preflight: impl FnOnce(u8, u64, u64) -> Result<(), FamilyArtifactLoadErrorV1>,
        decoder: impl FnOnce(u8, u64, &[u8]) -> Result<Box<[ColorSignal]>, FamilyArtifactLoadErrorV1>,
    ) -> Result<AdmittedFamilyArtifactV2, FamilyArtifactLoadFailureV1> {
        Self::load_fixture_with_codec(expected, encoded, preflight, decoder)
    }

    #[cfg(test)]
    fn load_fixture_with_codec(
        expected: FamilyImageCertificateV2,
        encoded: EncodedFamilyArtifactV2,
        preflight: impl FnOnce(u8, u64, u64) -> Result<(), FamilyArtifactLoadErrorV1>,
        decoder: impl FnOnce(u8, u64, &[u8]) -> Result<Box<[ColorSignal]>, FamilyArtifactLoadErrorV1>,
    ) -> Result<AdmittedFamilyArtifactV2, FamilyArtifactLoadFailureV1> {
        match Self::try_load_fixture_with_codec(expected, &encoded, preflight, decoder) {
            Ok(verified) => Ok(AdmittedFamilyArtifactV2 {
                certificate: verified.certificate,
                storage: ExecutableFamilyStorageV2::FixtureMembersV1(verified.decoded),
                drop_probe: None,
            }),
            Err(cause) => Err(FamilyArtifactLoadFailureV1 { cause, encoded }),
        }
    }

    fn try_load_raw_bitmap24(
        expected: FamilyImageCertificateV2,
        encoded: &EncodedFamilyArtifactV2,
    ) -> Result<FamilyImageCertificateV2, FamilyArtifactLoadErrorV1> {
        let parsed = Self::verify_bound_envelope(expected, encoded, |codec, _, payload_len| {
            raw_bitmap24::preflight(codec, payload_len)
        })?;
        let payload = &encoded.0[HEADER_LEN_V2..];
        let image = raw_bitmap24::verify_image(payload, parsed.certificate.member_count)?;
        if image != parsed.certificate.image_digest {
            return Err(FamilyArtifactLoadErrorV1::ImageDigestMismatch);
        }
        Ok(parsed.certificate)
    }

    #[cfg(test)]
    fn try_load_fixture_with_codec(
        expected: FamilyImageCertificateV2,
        encoded: &EncodedFamilyArtifactV2,
        preflight: impl FnOnce(u8, u64, u64) -> Result<(), FamilyArtifactLoadErrorV1>,
        decoder: impl FnOnce(u8, u64, &[u8]) -> Result<Box<[ColorSignal]>, FamilyArtifactLoadErrorV1>,
    ) -> Result<VerifiedFamilyArtifactEnvelopeV2, FamilyArtifactLoadErrorV1> {
        let parsed = Self::verify_bound_envelope(expected, encoded, preflight)?;
        let payload = &encoded.0[HEADER_LEN_V2..];
        VerifiedFamilyArtifactEnvelopeV2::decode(parsed, payload, decoder)
    }

    fn verify_bound_envelope(
        expected: FamilyImageCertificateV2,
        encoded: &EncodedFamilyArtifactV2,
        preflight: impl FnOnce(u8, u64, u64) -> Result<(), FamilyArtifactLoadErrorV1>,
    ) -> Result<ParsedFamilyArtifactEnvelopeV2, FamilyArtifactLoadErrorV1> {
        let parsed = ParsedFamilyArtifactEnvelopeV2::parse(&encoded.0)?;
        if parsed.certificate != expected {
            return Err(FamilyArtifactLoadErrorV1::ForeignCertificate);
        }
        preflight(
            parsed.certificate.codec_release,
            parsed.certificate.member_count,
            parsed.certificate.payload_len,
        )?;
        let payload = &encoded.0[HEADER_LEN_V2..];
        if payload_digest(payload) != parsed.certificate.payload_digest {
            return Err(FamilyArtifactLoadErrorV1::PayloadDigestMismatch);
        }
        if artifact_receipt(parsed.certificate) != parsed.certificate.artifact_receipt {
            return Err(FamilyArtifactLoadErrorV1::ArtifactReceiptMismatch);
        }
        let semantic = semantic_family_release_id_v2(
            parsed.certificate.definition_digest,
            parsed.certificate.image_digest,
            parsed.certificate.member_count,
        );
        if semantic != parsed.certificate.semantic_release {
            return Err(FamilyArtifactLoadErrorV1::SemanticReleaseMismatch);
        }
        Ok(parsed)
    }
}

struct ParsedFamilyArtifactEnvelopeV2 {
    certificate: FamilyImageCertificateV2,
}

impl ParsedFamilyArtifactEnvelopeV2 {
    fn parse(bytes: &[u8]) -> Result<Self, FamilyArtifactLoadErrorV1> {
        if bytes.len() < HEADER_LEN_V2 {
            return Err(FamilyArtifactLoadErrorV1::HeaderTooShort);
        }
        if bytes.get(..8) != Some(MAGIC_V2) {
            return Err(FamilyArtifactLoadErrorV1::InvalidMagic);
        }
        let certificate = admit_certificate_discriminants_v2(decode_certificate(
            <&[u8; CERTIFICATE_BODY_LEN_V2]>::try_from(&bytes[8..HEADER_LEN_V2])
                .map_err(|_| FamilyArtifactLoadErrorV1::HeaderTooShort)?,
        ))?;
        let payload_len = usize::try_from(certificate.payload_len)
            .map_err(|_| FamilyArtifactLoadErrorV1::ResourceExhausted)?;
        let expected_len = HEADER_LEN_V2
            .checked_add(payload_len)
            .ok_or(FamilyArtifactLoadErrorV1::ResourceExhausted)?;
        if bytes.len() != expected_len {
            return Err(FamilyArtifactLoadErrorV1::ExactLengthMismatch {
                expected: expected_len,
                actual: bytes.len(),
            });
        }
        Ok(Self { certificate })
    }
}

#[cfg(test)]
struct VerifiedFamilyArtifactEnvelopeV2 {
    certificate: FamilyImageCertificateV2,
    decoded: Box<[ColorSignal]>,
}

#[cfg(test)]
impl VerifiedFamilyArtifactEnvelopeV2 {
    fn decode(
        parsed: ParsedFamilyArtifactEnvelopeV2,
        payload: &[u8],
        decoder: impl FnOnce(u8, u64, &[u8]) -> Result<Box<[ColorSignal]>, FamilyArtifactLoadErrorV1>,
    ) -> Result<Self, FamilyArtifactLoadErrorV1> {
        let members = decoder(
            parsed.certificate.codec_release,
            parsed.certificate.member_count,
            payload,
        )?;
        let actual_count = members.len() as u64;
        if actual_count != parsed.certificate.member_count {
            return Err(FamilyArtifactLoadErrorV1::MemberCountMismatch {
                expected: parsed.certificate.member_count,
                actual: actual_count,
            });
        }
        let image = canonical_family_image_digest_v2(OutputProfileId::Iec61966Srgb8D65V1, &members)
            .map_err(|CanonicalFamilyImageErrorV2::NonCanonicalAdmittedImage| {
                FamilyArtifactLoadErrorV1::InvalidCodecPayload
            })?;
        if image != parsed.certificate.image_digest {
            return Err(FamilyArtifactLoadErrorV1::ImageDigestMismatch);
        }
        Ok(Self {
            certificate: parsed.certificate,
            decoded: members,
        })
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum FixtureFamilyArtifactCodecV1 {
    CanonicalMembersV1 = 0xF1,
    ReversedMembersV1 = 0xF2,
}

#[cfg(test)]
fn preflight_fixture_payload(
    codec: u8,
    member_count: u64,
    payload_len: u64,
) -> Result<(), FamilyArtifactLoadErrorV1> {
    match codec {
        value
            if value == FixtureFamilyArtifactCodecV1::CanonicalMembersV1 as u8
                || value == FixtureFamilyArtifactCodecV1::ReversedMembersV1 as u8 => {}
        _ => return Err(FamilyArtifactLoadErrorV1::UnsupportedCodec),
    }
    let expected_len = member_count
        .checked_mul(3)
        .ok_or(FamilyArtifactLoadErrorV1::ResourceExhausted)?;
    if payload_len != expected_len {
        return Err(FamilyArtifactLoadErrorV1::InvalidCodecPayload);
    }
    Ok(())
}

#[cfg(test)]
fn decode_fixture_payload(
    codec: u8,
    member_count: u64,
    payload: &[u8],
) -> Result<Box<[ColorSignal]>, FamilyArtifactLoadErrorV1> {
    let codec = match codec {
        value if value == FixtureFamilyArtifactCodecV1::CanonicalMembersV1 as u8 => {
            FixtureFamilyArtifactCodecV1::CanonicalMembersV1
        }
        value if value == FixtureFamilyArtifactCodecV1::ReversedMembersV1 as u8 => {
            FixtureFamilyArtifactCodecV1::ReversedMembersV1
        }
        _ => return Err(FamilyArtifactLoadErrorV1::UnsupportedCodec),
    };
    FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(calls.get() + 1));
    let count =
        usize::try_from(member_count).map_err(|_| FamilyArtifactLoadErrorV1::ResourceExhausted)?;
    let expected_len = count
        .checked_mul(3)
        .ok_or(FamilyArtifactLoadErrorV1::ResourceExhausted)?;
    if payload.len() != expected_len {
        return Err(FamilyArtifactLoadErrorV1::InvalidCodecPayload);
    }
    let mut members = Vec::new();
    members
        .try_reserve_exact(count)
        .map_err(|_| FamilyArtifactLoadErrorV1::ResourceExhausted)?;
    members.extend(
        payload.chunks_exact(3).map(|bytes| {
            ColorSignal::from_srgb8(crate::Srgb8::new([bytes[0], bytes[1], bytes[2]]))
        }),
    );
    let canonical = members
        .windows(2)
        .all(|pair| signal_key(pair[0]) < signal_key(pair[1]));
    match codec {
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1 if !canonical => {
            return Err(FamilyArtifactLoadErrorV1::InvalidCodecPayload);
        }
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1 => {}
        FixtureFamilyArtifactCodecV1::ReversedMembersV1 => {
            let reversed = members
                .windows(2)
                .all(|pair| signal_key(pair[0]) > signal_key(pair[1]));
            if !reversed {
                return Err(FamilyArtifactLoadErrorV1::InvalidCodecPayload);
            }
            members.reverse();
        }
    }
    Ok(members.into_boxed_slice())
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyArtifactBuildErrorV1 {
    ResourceExhausted,
    NonCanonicalFixture,
}

#[cfg(test)]
pub(crate) fn encode_fixture_family_artifact_v2(
    definition: FamilyDefinitionDigestV2,
    members: &[ColorSignal],
    codec: FixtureFamilyArtifactCodecV1,
) -> Result<(FamilyImageCertificateV2, EncodedFamilyArtifactV2), FamilyArtifactBuildErrorV1> {
    let mut canonical = Vec::new();
    canonical
        .try_reserve_exact(members.len())
        .map_err(|_| FamilyArtifactBuildErrorV1::ResourceExhausted)?;
    canonical.extend_from_slice(members);
    canonical.sort_unstable_by_key(|member| signal_key(*member));
    canonical.dedup_by_key(|member| signal_key(*member));
    let member_count = canonical.len() as u64;
    let image_digest =
        canonical_family_image_digest_v2(OutputProfileId::Iec61966Srgb8D65V1, &canonical).map_err(
            |CanonicalFamilyImageErrorV2::NonCanonicalAdmittedImage| {
                FamilyArtifactBuildErrorV1::NonCanonicalFixture
            },
        )?;
    let semantic_release = semantic_family_release_id_v2(definition, image_digest, member_count);
    let proof_artifact =
        fixture_proof_artifact_id(definition, image_digest, semantic_release, member_count);
    let verifier_identity = fixture_verifier_identity();
    let encoded_order: Box<dyn Iterator<Item = ColorSignal>> = match codec {
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1 => Box::new(canonical.iter().copied()),
        FixtureFamilyArtifactCodecV1::ReversedMembersV1 => {
            Box::new(canonical.iter().rev().copied())
        }
    };
    let payload_capacity = canonical
        .len()
        .checked_mul(3)
        .ok_or(FamilyArtifactBuildErrorV1::ResourceExhausted)?;
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(payload_capacity)
        .map_err(|_| FamilyArtifactBuildErrorV1::ResourceExhausted)?;
    for member in encoded_order {
        payload.extend_from_slice(&member.srgb8().bytes());
    }
    let payload_len = payload.len() as u64;
    let mut certificate = FamilyImageCertificateV2 {
        envelope_release: ENVELOPE_RELEASE_V2,
        codec_release: codec as u8,
        signal_domain: SIGNAL_DOMAIN_SRGB8_D65_V1,
        signal_ordinal: SIGNAL_ORDINAL_RGB_BIG_ENDIAN_V1,
        proof_release: PROOF_RELEASE_EXPECTED_EXACT_IMAGE_V1,
        verifier_release: VERIFIER_RELEASE_EXPECTED_REPLAY_V1,
        proof_artifact,
        verifier_identity,
        definition_digest: definition,
        image_digest,
        semantic_release,
        payload_digest: payload_digest(&payload),
        member_count,
        payload_len,
        artifact_receipt: FamilyArtifactReceiptIdV2([0; 32]),
    };
    certificate.artifact_receipt = artifact_receipt(certificate);
    let total_len = HEADER_LEN_V2
        .checked_add(payload.len())
        .ok_or(FamilyArtifactBuildErrorV1::ResourceExhausted)?;
    let mut encoded = Vec::new();
    encoded
        .try_reserve_exact(total_len)
        .map_err(|_| FamilyArtifactBuildErrorV1::ResourceExhausted)?;
    encoded.extend_from_slice(MAGIC_V2);
    encode_certificate(&mut encoded, certificate);
    encoded.extend_from_slice(&payload);
    debug_assert_eq!(encoded.len(), total_len);
    Ok((
        certificate,
        EncodedFamilyArtifactV2(encoded.into_boxed_slice()),
    ))
}

fn signal_key(signal: ColorSignal) -> [u8; 3] {
    signal.srgb8().bytes()
}

fn payload_digest(payload: &[u8]) -> [u8; 32] {
    #[cfg(test)]
    FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS.with(|calls| calls.set(calls.get() + 1));
    let mut hasher = Hasher::new();
    hasher.update(PAYLOAD_DIGEST_DOMAIN_V2);
    hasher.update(&(payload.len() as u64).to_be_bytes());
    hasher.update(payload);
    *hasher.finalize().as_bytes()
}

#[cfg(test)]
fn fixture_proof_artifact_id(
    definition: FamilyDefinitionDigestV2,
    image: CanonicalFamilyImageDigestV2,
    semantic: SemanticFamilyReleaseIdV2,
    member_count: u64,
) -> FamilyProofArtifactIdV2 {
    let mut hasher = Hasher::new();
    hasher.update(FIXTURE_PROOF_ARTIFACT_DOMAIN_V2);
    hasher.update(definition.as_bytes());
    hasher.update(image.as_bytes());
    hasher.update(semantic.as_bytes());
    hasher.update(&member_count.to_be_bytes());
    FamilyProofArtifactIdV2(*hasher.finalize().as_bytes())
}

#[cfg(test)]
fn fixture_verifier_identity() -> FamilyVerifierIdentityV2 {
    let mut hasher = Hasher::new();
    hasher.update(FIXTURE_VERIFIER_IDENTITY_DOMAIN_V2);
    hasher.update(&[
        PROOF_RELEASE_EXPECTED_EXACT_IMAGE_V1,
        VERIFIER_RELEASE_EXPECTED_REPLAY_V1,
    ]);
    FamilyVerifierIdentityV2(*hasher.finalize().as_bytes())
}

fn artifact_receipt(certificate: FamilyImageCertificateV2) -> FamilyArtifactReceiptIdV2 {
    let mut hasher = Hasher::new();
    hasher.update(RECEIPT_DOMAIN_V2);
    update_receipt_preimage(&mut hasher, certificate);
    FamilyArtifactReceiptIdV2(*hasher.finalize().as_bytes())
}

/// Receipt-slot исключён намеренно: иначе certificate должен содержать хеш
/// самого себя и admission станет циклическим, а не content-addressed.
fn update_receipt_preimage(hasher: &mut Hasher, certificate: FamilyImageCertificateV2) {
    hasher.update(&[
        certificate.envelope_release,
        certificate.codec_release,
        certificate.signal_domain,
        certificate.signal_ordinal,
        certificate.proof_release,
        certificate.verifier_release,
    ]);
    hasher.update(&certificate.proof_artifact.0);
    hasher.update(&certificate.verifier_identity.0);
    hasher.update(certificate.definition_digest.as_bytes());
    hasher.update(certificate.image_digest.as_bytes());
    hasher.update(certificate.semantic_release.as_bytes());
    hasher.update(&certificate.payload_digest);
    hasher.update(&certificate.member_count.to_be_bytes());
    hasher.update(&certificate.payload_len.to_be_bytes());
}

fn encode_certificate(output: &mut Vec<u8>, certificate: FamilyImageCertificateV2) {
    output.extend_from_slice(&[
        certificate.envelope_release,
        certificate.codec_release,
        certificate.signal_domain,
        certificate.signal_ordinal,
        certificate.proof_release,
        certificate.verifier_release,
    ]);
    output.extend_from_slice(&certificate.proof_artifact.0);
    output.extend_from_slice(&certificate.verifier_identity.0);
    output.extend_from_slice(certificate.definition_digest.as_bytes());
    output.extend_from_slice(certificate.image_digest.as_bytes());
    output.extend_from_slice(certificate.semantic_release.as_bytes());
    output.extend_from_slice(&certificate.payload_digest);
    output.extend_from_slice(&certificate.member_count.to_be_bytes());
    output.extend_from_slice(&certificate.payload_len.to_be_bytes());
    output.extend_from_slice(certificate.artifact_receipt.as_bytes());
}

/// Единственный допуск дискриминантов сертификата.
///
/// Конверт и доверенная запись обязаны судить по одному закону: два валидатора
/// разошлись бы, и запись стала бы принимать то, что конверт отвергает.
fn admit_certificate_discriminants_v2(
    certificate: FamilyImageCertificateV2,
) -> Result<FamilyImageCertificateV2, FamilyArtifactLoadErrorV1> {
    if certificate.envelope_release != ENVELOPE_RELEASE_V2 {
        return Err(FamilyArtifactLoadErrorV1::UnsupportedEnvelope);
    }
    if certificate.signal_domain != SIGNAL_DOMAIN_SRGB8_D65_V1 {
        return Err(FamilyArtifactLoadErrorV1::UnsupportedSignalDomain);
    }
    if certificate.signal_ordinal != SIGNAL_ORDINAL_RGB_BIG_ENDIAN_V1 {
        return Err(FamilyArtifactLoadErrorV1::UnsupportedSignalOrdinal);
    }
    if certificate.proof_release != PROOF_RELEASE_EXPECTED_EXACT_IMAGE_V1 {
        return Err(FamilyArtifactLoadErrorV1::UnsupportedProofRelease);
    }
    if certificate.verifier_release != VERIFIER_RELEASE_EXPECTED_REPLAY_V1 {
        return Err(FamilyArtifactLoadErrorV1::UnsupportedVerifierRelease);
    }
    if certificate.member_count > MAX_SRGB8_MEMBER_COUNT_V1 {
        return Err(FamilyArtifactLoadErrorV1::InvalidMemberCount);
    }
    Ok(certificate)
}

fn decode_certificate(bytes: &[u8; CERTIFICATE_BODY_LEN_V2]) -> FamilyImageCertificateV2 {
    fn take_32(bytes: &[u8], cursor: &mut usize) -> [u8; 32] {
        let start = *cursor;
        *cursor += 32;
        let mut value = [0; 32];
        value.copy_from_slice(&bytes[start..*cursor]);
        value
    }
    let mut cursor = 0;
    let envelope_release = bytes[cursor];
    cursor += 1;
    let codec_release = bytes[cursor];
    cursor += 1;
    let signal_domain = bytes[cursor];
    cursor += 1;
    let signal_ordinal = bytes[cursor];
    cursor += 1;
    let proof_release = bytes[cursor];
    cursor += 1;
    let verifier_release = bytes[cursor];
    cursor += 1;
    let proof_artifact = FamilyProofArtifactIdV2(take_32(bytes, &mut cursor));
    let verifier_identity = FamilyVerifierIdentityV2(take_32(bytes, &mut cursor));
    let definition_digest = FamilyDefinitionDigestV2::from_digest(take_32(bytes, &mut cursor));
    let image_digest = CanonicalFamilyImageDigestV2::from_digest(take_32(bytes, &mut cursor));
    let semantic_release = SemanticFamilyReleaseIdV2::from_digest(take_32(bytes, &mut cursor));
    let payload_digest = take_32(bytes, &mut cursor);
    let member_count = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
    cursor += 8;
    let payload_len = u64::from_be_bytes(bytes[cursor..cursor + 8].try_into().unwrap());
    cursor += 8;
    let artifact_receipt = FamilyArtifactReceiptIdV2::from_digest(take_32(bytes, &mut cursor));
    debug_assert_eq!(cursor, bytes.len());
    FamilyImageCertificateV2 {
        envelope_release,
        codec_release,
        signal_domain,
        signal_ordinal,
        proof_release,
        verifier_release,
        proof_artifact,
        verifier_identity,
        definition_digest,
        image_digest,
        semantic_release,
        payload_digest,
        member_count,
        payload_len,
        artifact_receipt,
    }
}

/// Loaded semantic artifacts ещё не сопоставлены ordinal-слотам Program.
///
/// Transport не повторяет client-owned `FamilyId → semantic`: один artifact
/// обслуживает все opaque aliases одного semantic release.
#[derive(Debug)]
pub(crate) struct FamilyArtifactBundleV2 {
    artifacts: Box<[AdmittedFamilyArtifactV2]>,
}

impl FamilyArtifactBundleV2 {
    pub(crate) fn empty() -> Self {
        Self {
            artifacts: Box::new([]),
        }
    }

    pub(crate) fn from_artifacts(artifacts: Vec<AdmittedFamilyArtifactV2>) -> Self {
        Self {
            artifacts: artifacts.into_boxed_slice(),
        }
    }

    /// Возвращает уже допущенный semantic pool для add/remove/replace без
    /// повторного payload decode.
    pub(crate) fn into_artifacts(self) -> Vec<AdmittedFamilyArtifactV2> {
        self.artifacts.into_vec()
    }

    pub(crate) fn bind(
        self,
        declarations: &[FamilyDeclarationV2],
    ) -> Result<BoundFamilyArtifactBundleV2, FamilyArtifactBindFailureV2> {
        let mut artifacts = self.artifacts.into_vec();
        artifacts.sort_unstable_by_key(AdmittedFamilyArtifactV2::semantic_release);
        if let Some(semantic) = artifacts
            .windows(2)
            .find(|pair| pair[0].semantic_release() == pair[1].semantic_release())
            .map(|pair| pair[0].semantic_release())
        {
            return Err(bind_failure(
                FamilyArtifactBindErrorV2::Contract(FamilyArtifactContractErrorV2::Duplicate {
                    semantic,
                }),
                artifacts,
            ));
        }
        let mut slots = Vec::new();
        if slots.try_reserve_exact(declarations.len()).is_err() {
            return Err(bind_failure(
                FamilyArtifactBindErrorV2::ResourceExhausted,
                artifacts,
            ));
        }
        let mut missing: Option<SemanticFamilyReleaseIdV2> = None;
        for declaration in declarations.iter().copied() {
            let semantic = declaration.semantic();
            match artifacts
                .binary_search_by_key(&semantic, AdmittedFamilyArtifactV2::semantic_release)
            {
                Ok(index) => slots.push(index),
                Err(_) => {
                    missing = Some(missing.map_or(semantic, |current| current.min(semantic)));
                }
            }
        }
        if let Some(semantic) = missing {
            return Err(bind_failure(
                FamilyArtifactBindErrorV2::Contract(FamilyArtifactContractErrorV2::Missing {
                    semantic,
                }),
                artifacts,
            ));
        }

        let mut used = Vec::new();
        if used.try_reserve_exact(artifacts.len()).is_err() {
            return Err(bind_failure(
                FamilyArtifactBindErrorV2::ResourceExhausted,
                artifacts,
            ));
        }
        used.resize(artifacts.len(), false);
        for artifact_index in slots.iter().copied() {
            used[artifact_index] = true;
        }
        if let Some((artifact_index, _)) = used.iter().enumerate().find(|(_, used)| !**used) {
            return Err(bind_failure(
                FamilyArtifactBindErrorV2::Contract(FamilyArtifactContractErrorV2::Extra {
                    semantic: artifacts[artifact_index].semantic_release(),
                }),
                artifacts,
            ));
        }
        Ok(BoundFamilyArtifactBundleV2 {
            artifacts: artifacts.into_boxed_slice(),
            family_artifact_indices: slots.into_boxed_slice(),
        })
    }
}

fn bind_failure(
    cause: FamilyArtifactBindErrorV2,
    artifacts: Vec<AdmittedFamilyArtifactV2>,
) -> FamilyArtifactBindFailureV2 {
    FamilyArtifactBindFailureV2 {
        cause,
        bundle: FamilyArtifactBundleV2::from_artifacts(artifacts),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyArtifactContractErrorV2 {
    Missing { semantic: SemanticFamilyReleaseIdV2 },
    Extra { semantic: SemanticFamilyReleaseIdV2 },
    Duplicate { semantic: SemanticFamilyReleaseIdV2 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyArtifactBindErrorV2 {
    Contract(FamilyArtifactContractErrorV2),
    ResourceExhausted,
}

/// Failed binding returns the same loaded storage without reload or decode.
pub(crate) struct FamilyArtifactBindFailureV2 {
    cause: FamilyArtifactBindErrorV2,
    bundle: FamilyArtifactBundleV2,
}

impl fmt::Debug for FamilyArtifactBindFailureV2 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FamilyArtifactBindFailureV2")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}

impl FamilyArtifactBindFailureV2 {
    pub(crate) const fn cause(&self) -> FamilyArtifactBindErrorV2 {
        self.cause
    }

    pub(crate) fn into_parts(self) -> (FamilyArtifactBindErrorV2, FamilyArtifactBundleV2) {
        (self.cause, self.bundle)
    }
}

/// Canonical family-index aligned executable storage of one Session generation.
#[derive(Debug)]
pub(crate) struct BoundFamilyArtifactBundleV2 {
    artifacts: Box<[AdmittedFamilyArtifactV2]>,
    family_artifact_indices: Box<[usize]>,
}

impl BoundFamilyArtifactBundleV2 {
    pub(crate) fn artifact(&self, family_index: usize) -> Option<&AdmittedFamilyArtifactV2> {
        let artifact_index = *self.family_artifact_indices.get(family_index)?;
        self.artifacts.get(artifact_index)
    }

    pub(crate) fn execution_bindings(&self) -> FamilyExecutionBindingsV2<'_> {
        FamilyExecutionBindingsV2 {
            artifacts: self.artifacts.iter(),
        }
    }

    pub(crate) fn into_unbound(self) -> FamilyArtifactBundleV2 {
        FamilyArtifactBundleV2 {
            artifacts: self.artifacts,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyExecutionBindingV2 {
    semantic: SemanticFamilyReleaseIdV2,
    receipt: FamilyArtifactReceiptIdV2,
}

impl FamilyExecutionBindingV2 {
    pub(crate) const fn semantic(self) -> SemanticFamilyReleaseIdV2 {
        self.semantic
    }

    pub(crate) const fn receipt(self) -> FamilyArtifactReceiptIdV2 {
        self.receipt
    }
}

pub(crate) struct FamilyExecutionBindingsV2<'a> {
    artifacts: core::slice::Iter<'a, AdmittedFamilyArtifactV2>,
}

impl Iterator for FamilyExecutionBindingsV2<'_> {
    type Item = FamilyExecutionBindingV2;

    fn next(&mut self) -> Option<Self::Item> {
        let artifact = self.artifacts.next()?;
        Some(FamilyExecutionBindingV2 {
            semantic: artifact.semantic_release(),
            receipt: artifact.artifact_receipt(),
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.artifacts.size_hint()
    }
}

impl ExactSizeIterator for FamilyExecutionBindingsV2<'_> {}
impl core::iter::FusedIterator for FamilyExecutionBindingsV2<'_> {}
