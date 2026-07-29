//! Exact executable bitmap полного IEC sRGB8 domain.

use crate::Srgb8;
use crate::family::{
    CanonicalFamilyImageDigestV2, CanonicalFamilyImageStreamErrorV2,
    canonical_family_image_digest_from_ordered_members_v2,
};
#[cfg(test)]
use crate::family::{
    CanonicalFamilyImageErrorV2, FamilyDefinitionDigestV2, canonical_family_image_digest_v2,
    semantic_family_release_id_v2,
};
use crate::lcs_occurrence::{ColorSignal, OutputProfileId};

#[cfg(test)]
use super::{
    ENVELOPE_RELEASE_V2, MAGIC_V2, SIGNAL_DOMAIN_SRGB8_D65_V1, SIGNAL_ORDINAL_RGB_BIG_ENDIAN_V1,
};
use super::{EncodedFamilyArtifactV2, FamilyArtifactLoadErrorV1, HEADER_LEN_V2};

pub(super) const CODEC_RELEASE_V1: u8 = 0x01;
// sRGB8 имеет 2^24 сигналов; exact membership требует один bit
// на сигнал, поэтому размер не является configurable budget.
pub(crate) const PAYLOAD_LEN_V1: usize = 1_usize << 21;
const _: () = assert!(PAYLOAD_LEN_V1 == 2_097_152);

/// Исходная owned allocation одновременно является executable storage.
/// Header остаётся рядом с bitmap, чтобы admission не клонировал
/// и не переаллоцировал 2 MiB payload.
pub(super) struct RawBitmap24V1 {
    encoded: EncodedFamilyArtifactV2,
}

impl RawBitmap24V1 {
    pub(super) fn from_verified(encoded: EncodedFamilyArtifactV2) -> Self {
        debug_assert_eq!(encoded.0.len(), HEADER_LEN_V2 + PAYLOAD_LEN_V1);
        Self { encoded }
    }

    pub(super) fn contains(&self, signal: ColorSignal) -> bool {
        let ordinal = signal_ordinal(signal.srgb8().bytes());
        let byte = self.payload()[ordinal >> 3];
        byte & bit_mask(ordinal) != 0
    }

    fn payload(&self) -> &[u8] {
        &self.encoded.0[HEADER_LEN_V2..]
    }

    #[cfg(test)]
    pub(super) fn allocation_ptr(&self) -> *const u8 {
        self.encoded.0.as_ptr()
    }
}

pub(super) fn preflight(
    codec_release: u8,
    payload_len: u64,
) -> Result<(), FamilyArtifactLoadErrorV1> {
    if codec_release != CODEC_RELEASE_V1 {
        return Err(FamilyArtifactLoadErrorV1::UnsupportedCodec);
    }
    if payload_len != PAYLOAD_LEN_V1 as u64 {
        return Err(FamilyArtifactLoadErrorV1::CodecPayloadLengthMismatch {
            codec_release,
            expected: PAYLOAD_LEN_V1 as u64,
            actual: payload_len,
        });
    }
    Ok(())
}

pub(super) fn verify_image(
    payload: &[u8],
    expected_member_count: u64,
) -> Result<CanonicalFamilyImageDigestV2, FamilyArtifactLoadErrorV1> {
    debug_assert_eq!(payload.len(), PAYLOAD_LEN_V1);
    let actual_member_count = payload
        .iter()
        .copied()
        .map(|byte| u64::from(byte.count_ones()))
        .sum();
    if actual_member_count != expected_member_count {
        return Err(FamilyArtifactLoadErrorV1::MemberCountMismatch {
            expected: expected_member_count,
            actual: actual_member_count,
        });
    }
    canonical_family_image_digest_from_ordered_members_v2(
        OutputProfileId::Iec61966Srgb8D65V1,
        expected_member_count,
        RawBitmapMembersV1::new(payload),
    )
    .map_err(|error| match error {
        CanonicalFamilyImageStreamErrorV2::MemberCountMismatch { expected, actual } => {
            FamilyArtifactLoadErrorV1::MemberCountMismatch { expected, actual }
        }
        CanonicalFamilyImageStreamErrorV2::NonCanonicalAdmittedImage => {
            FamilyArtifactLoadErrorV1::InvalidCodecPayload
        }
    })
}

fn signal_ordinal([red, green, blue]: [u8; 3]) -> usize {
    (usize::from(red) << 16) + (usize::from(green) << 8) + usize::from(blue)
}

fn bit_mask(ordinal: usize) -> u8 {
    // MSB0 делает ordinal 0 старшим bit первого byte; этот
    // порядок сканируется как RGB big-endian без sort или decode.
    0x80_u8 >> (ordinal & 7)
}

struct RawBitmapMembersV1<'a> {
    bytes: core::iter::Enumerate<core::slice::Iter<'a, u8>>,
    current_byte_index: usize,
    remaining: u8,
}

impl<'a> RawBitmapMembersV1<'a> {
    fn new(payload: &'a [u8]) -> Self {
        Self {
            bytes: payload.iter().enumerate(),
            current_byte_index: 0,
            remaining: 0,
        }
    }
}

impl Iterator for RawBitmapMembersV1<'_> {
    type Item = ColorSignal;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.remaining == 0 {
                let (byte_index, byte) = self.bytes.next()?;
                self.current_byte_index = byte_index;
                self.remaining = *byte;
                if self.remaining == 0 {
                    continue;
                }
            }
            let bit = self.remaining.leading_zeros() as usize;
            self.remaining &= !(0x80_u8 >> bit);
            let ordinal = (self.current_byte_index << 3) + bit;
            let red = (ordinal >> 16) as u8;
            let green = (ordinal >> 8) as u8;
            let blue = ordinal as u8;
            return Some(ColorSignal::from_srgb8(Srgb8::new([red, green, blue])));
        }
    }
}

#[cfg(test)]
pub(super) fn set_member(payload: &mut [u8], rgb: [u8; 3], member: bool) {
    let ordinal = signal_ordinal(rgb);
    let mask = bit_mask(ordinal);
    if member {
        payload[ordinal >> 3] |= mask;
    } else {
        payload[ordinal >> 3] &= !mask;
    }
}

#[cfg(test)]
pub(crate) fn encode_for_test(
    definition: FamilyDefinitionDigestV2,
    members: &[ColorSignal],
) -> Result<
    (super::FamilyImageCertificateV2, EncodedFamilyArtifactV2),
    super::FamilyArtifactBuildErrorV1,
> {
    let mut canonical = Vec::new();
    canonical
        .try_reserve_exact(members.len())
        .map_err(|_| super::FamilyArtifactBuildErrorV1::ResourceExhausted)?;
    canonical.extend_from_slice(members);
    canonical.sort_unstable_by_key(|member| member.srgb8().bytes());
    canonical.dedup_by_key(|member| member.srgb8().bytes());
    let member_count = canonical.len() as u64;
    let image_digest =
        canonical_family_image_digest_v2(OutputProfileId::Iec61966Srgb8D65V1, &canonical).map_err(
            |CanonicalFamilyImageErrorV2::NonCanonicalAdmittedImage| {
                super::FamilyArtifactBuildErrorV1::NonCanonicalFixture
            },
        )?;
    let semantic_release = semantic_family_release_id_v2(definition, image_digest, member_count);
    let proof_artifact =
        super::fixture_proof_artifact_id(definition, image_digest, semantic_release, member_count);
    let verifier_identity = super::fixture_verifier_identity();
    let mut payload = Vec::new();
    payload
        .try_reserve_exact(PAYLOAD_LEN_V1)
        .map_err(|_| super::FamilyArtifactBuildErrorV1::ResourceExhausted)?;
    payload.resize(PAYLOAD_LEN_V1, 0);
    for member in canonical {
        set_member(&mut payload, member.srgb8().bytes(), true);
    }
    let mut certificate = super::FamilyImageCertificateV2 {
        envelope_release: ENVELOPE_RELEASE_V2,
        codec_release: CODEC_RELEASE_V1,
        signal_domain: SIGNAL_DOMAIN_SRGB8_D65_V1,
        signal_ordinal: SIGNAL_ORDINAL_RGB_BIG_ENDIAN_V1,
        proof_release: super::PROOF_RELEASE_EXPECTED_EXACT_IMAGE_V1,
        verifier_release: super::VERIFIER_RELEASE_EXPECTED_REPLAY_V1,
        proof_artifact,
        verifier_identity,
        definition_digest: definition,
        image_digest,
        semantic_release,
        payload_digest: super::payload_digest(&payload),
        member_count,
        payload_len: PAYLOAD_LEN_V1 as u64,
        artifact_receipt: super::FamilyArtifactReceiptIdV2::from_digest([0; 32]),
    };
    certificate.artifact_receipt = super::artifact_receipt(certificate);
    let total_len = HEADER_LEN_V2
        .checked_add(PAYLOAD_LEN_V1)
        .ok_or(super::FamilyArtifactBuildErrorV1::ResourceExhausted)?;
    let mut encoded = Vec::new();
    encoded
        .try_reserve_exact(total_len)
        .map_err(|_| super::FamilyArtifactBuildErrorV1::ResourceExhausted)?;
    encoded.extend_from_slice(MAGIC_V2);
    super::encode_certificate(&mut encoded, certificate);
    encoded.extend_from_slice(&payload);
    debug_assert_eq!(encoded.len(), total_len);
    Ok((
        certificate,
        EncodedFamilyArtifactV2(encoded.into_boxed_slice()),
    ))
}
