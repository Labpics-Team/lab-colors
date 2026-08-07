//! Hostile-контракт transport artifact и exact RawBitmap24 codec.

use proptest::prelude::*;

use crate::Srgb8;
use crate::family::{
    CanonicalFamilyImageErrorV2, FamilyDeclarationV2, FamilyDefinitionDigestV2, FamilyId,
    canonical_family_image_digest_v2,
};
use crate::family_artifact::{
    AdmittedFamilyArtifactV2, EncodedFamilyArtifactV2, FAMILY_ARTIFACT_DECODER_CALLS,
    FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS, FAMILY_CERTIFICATE_RECORD_LEN_V2,
    FamilyArtifactBindErrorV2, FamilyArtifactBundleV2, FamilyArtifactContractErrorV2,
    FamilyArtifactLoadErrorV1, FamilyArtifactLoaderV1, FamilyImageCertificateV2,
    FixtureEnvelopeFieldV1, FixtureFamilyArtifactCodecV1, RAW_BITMAP24_PAYLOAD_LEN_V1,
    encode_fixture_family_artifact_v2, encode_raw_bitmap24_family_artifact_v2_for_test,
};
use crate::lcs_occurrence::ColorSignal;
use crate::lcs_occurrence::OutputProfileId;

fn signals(values: &[[u8; 3]]) -> Vec<ColorSignal> {
    values
        .iter()
        .copied()
        .map(Srgb8::new)
        .map(ColorSignal::from_srgb8)
        .collect()
}

fn definition() -> FamilyDefinitionDigestV2 {
    FamilyDefinitionDigestV2::from_fixture_bytes_v2(b"family-artifact-tests/blue-axis")
}

fn load_fixture(
    expected: FamilyImageCertificateV2,
    encoded: EncodedFamilyArtifactV2,
) -> Result<AdmittedFamilyArtifactV2, FamilyArtifactLoadErrorV1> {
    FamilyArtifactLoaderV1::load_fixture(expected, encoded).map_err(|failure| failure.cause())
}

#[test]
fn owned_transport_round_trips_without_copy_or_private_constructor() {
    let bytes = vec![1, 2, 3, 4].into_boxed_slice();
    let pointer = bytes.as_ptr();
    let encoded = EncodedFamilyArtifactV2::from_owned_bytes(bytes);
    let returned = encoded.into_bytes();

    assert_eq!(returned.as_ptr(), pointer);
    assert_eq!(&*returned, &[1, 2, 3, 4]);
}

#[test]
fn raw_bitmap_uses_rgb_big_endian_ordinal_and_msb_zero_at_every_boundary() {
    let members = signals(&[
        [0, 0, 0],
        [0, 0, 7],
        [0, 0, 8],
        [0, 0, 255],
        [0, 1, 0],
        [1, 0, 0],
        [127, 255, 255],
        [128, 0, 0],
        [255, 255, 255],
    ]);
    let (certificate, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();
    assert_eq!(encoded.payload_byte_for_test(0), 0x81);
    assert_eq!(encoded.payload_byte_for_test(1), 0x80);
    assert_eq!(encoded.payload_byte_for_test(31), 0x01);
    assert_eq!(encoded.payload_byte_for_test(32), 0x80);
    assert_eq!(encoded.payload_byte_for_test(8_192), 0x80);
    assert_eq!(encoded.payload_byte_for_test(1_048_575), 0x01);
    assert_eq!(encoded.payload_byte_for_test(1_048_576), 0x80);
    assert_eq!(
        encoded.payload_byte_for_test(RAW_BITMAP24_PAYLOAD_LEN_V1 - 1),
        0x01
    );
    let admitted = FamilyArtifactLoaderV1::load(certificate, encoded).unwrap();

    for member in members {
        assert!(
            admitted.contains(member),
            "missing boundary member {member:?}"
        );
    }
    for nonmember in signals(&[
        [0, 0, 1],
        [0, 0, 6],
        [0, 0, 9],
        [0, 1, 1],
        [1, 0, 1],
        [127, 255, 254],
        [128, 0, 1],
        [255, 255, 254],
    ]) {
        assert!(
            !admitted.contains(nonmember),
            "false boundary member {nonmember:?}"
        );
    }
}

#[test]
fn raw_bitmap_and_slice_oracle_share_the_independent_canonical_image_golden() {
    let members = signals(&[[0, 0, 1], [0, 0, 2], [0, 0, 255]]);
    let expected = [
        0x3c, 0xe1, 0x50, 0x04, 0xfc, 0x39, 0x7b, 0xad, 0x7c, 0x95, 0x5f, 0x28, 0x90, 0xcb, 0x9c,
        0x99, 0x01, 0x86, 0x0b, 0x93, 0x5a, 0x24, 0xc2, 0xb5, 0xb8, 0x56, 0xec, 0xb6, 0x60, 0xe5,
        0xcf, 0x20,
    ];
    let slice =
        canonical_family_image_digest_v2(OutputProfileId::Iec61966Srgb8D65V1, &members).unwrap();
    let (raw, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();

    // Golden получен независимым Python hashlib над явным V2 preimage.
    assert_eq!(slice.as_bytes(), &expected);
    assert_eq!(raw.image_digest().as_bytes(), &expected);
    FamilyArtifactLoaderV1::load(raw, encoded).unwrap();
}

#[test]
fn raw_and_fixture_codecs_preserve_semantics_but_not_transport_receipt() {
    let members = signals(&[[0, 0, 1], [0, 0, 2], [0, 0, 255]]);
    let (raw_certificate, raw_encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();
    let (fixture_certificate, fixture_encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();

    assert_eq!(
        raw_certificate.semantic_release(),
        fixture_certificate.semantic_release()
    );
    assert_eq!(
        raw_certificate.image_digest(),
        fixture_certificate.image_digest()
    );
    assert_ne!(
        raw_certificate.artifact_receipt(),
        fixture_certificate.artifact_receipt()
    );
    let raw = FamilyArtifactLoaderV1::load(raw_certificate, raw_encoded).unwrap();
    let fixture = load_fixture(fixture_certificate, fixture_encoded).unwrap();
    for member in members {
        assert!(raw.contains(member));
        assert!(fixture.contains(member));
    }
}

#[test]
fn canonical_image_digest_rejects_permutations_and_duplicates() {
    let first = ColorSignal::from_srgb8(Srgb8::new([0, 0, 1]));
    let second = ColorSignal::from_srgb8(Srgb8::new([0, 0, 2]));

    assert_eq!(
        canonical_family_image_digest_v2(OutputProfileId::Iec61966Srgb8D65V1, &[second, first],),
        Err(CanonicalFamilyImageErrorV2::NonCanonicalAdmittedImage),
    );
    assert_eq!(
        canonical_family_image_digest_v2(OutputProfileId::Iec61966Srgb8D65V1, &[first, first],),
        Err(CanonicalFamilyImageErrorV2::NonCanonicalAdmittedImage),
    );
}

#[test]
fn empty_exact_set_has_a_domain_bound_semantic_release_and_total_lookup() {
    let members = Vec::new();
    let image =
        canonical_family_image_digest_v2(OutputProfileId::Iec61966Srgb8D65V1, &members).unwrap();
    let (certificate, encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();

    assert_eq!(certificate.image_digest(), image);
    let artifact = load_fixture(certificate, encoded).unwrap();
    assert!(!artifact.contains(ColorSignal::from_srgb8(Srgb8::new([0; 3]))));
}

#[test]
fn two_encodings_keep_semantic_release_and_change_artifact_receipt() {
    let members = signals(&[[0, 0, 1], [0, 0, 2], [0, 0, 255]]);
    let (canonical_certificate, canonical_bytes) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();
    let (reversed_certificate, reversed_bytes) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::ReversedMembersV1,
    )
    .unwrap();

    assert_eq!(
        canonical_certificate.semantic_release(),
        reversed_certificate.semantic_release(),
    );
    assert_eq!(
        canonical_certificate.image_digest(),
        reversed_certificate.image_digest(),
    );
    assert_ne!(
        canonical_certificate.artifact_receipt(),
        reversed_certificate.artifact_receipt(),
    );

    let canonical = load_fixture(canonical_certificate, canonical_bytes).unwrap();
    let reversed = load_fixture(reversed_certificate, reversed_bytes).unwrap();
    for member in members {
        assert!(canonical.contains(member));
        assert!(reversed.contains(member));
    }
}

#[test]
fn semantic_and_receipt_codecs_have_independent_sha256_goldens() {
    let members = signals(&[[0, 0, 1], [0, 0, 2], [0, 0, 255]]);
    let (certificate, _) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();

    // Эти значения вычисляются независимым Python hashlib oracle над явно
    // специфицированными preimage; self-consistent Rust hash не считается proof.
    assert_eq!(
        certificate.semantic_release().as_bytes(),
        &[
            0x36, 0xfa, 0xda, 0x04, 0xed, 0xeb, 0x34, 0x08, 0x6a, 0x56, 0x83, 0xc8, 0xdb, 0xfe,
            0x4c, 0xe0, 0xb7, 0xba, 0xec, 0x8d, 0x99, 0x81, 0xd7, 0xc8, 0xba, 0xf8, 0x04, 0x16,
            0x5f, 0x9c, 0x5f, 0x49,
        ],
    );
    assert_eq!(
        certificate.proof_artifact().as_bytes(),
        &[
            0x09, 0x6d, 0x54, 0x67, 0x6e, 0xc3, 0xfd, 0x1e, 0x5e, 0x53, 0x83, 0x73, 0x02, 0xb9,
            0xd7, 0x07, 0xcb, 0x99, 0xbf, 0x07, 0x40, 0xe5, 0xc2, 0x48, 0x3d, 0x9e, 0x7d, 0xa0,
            0xa5, 0xb1, 0xbc, 0xb4,
        ],
    );
    assert_eq!(
        certificate.verifier_identity().as_bytes(),
        &[
            0xb1, 0x0b, 0xc6, 0xb7, 0x97, 0xc6, 0xc2, 0x0c, 0xcb, 0xe3, 0xed, 0x99, 0x79, 0xcb,
            0xd6, 0xa2, 0xdd, 0x98, 0xd1, 0xe4, 0xa2, 0xb3, 0x99, 0x9a, 0x07, 0x1b, 0x2f, 0x54,
            0x22, 0xbe, 0xed, 0x87,
        ],
    );
    assert_eq!(
        certificate.artifact_receipt().as_bytes(),
        &[
            0x4c, 0xef, 0xb1, 0xc4, 0xd6, 0xd0, 0x47, 0x6a, 0xb1, 0xc3, 0x89, 0x2e, 0x30, 0xd1,
            0x8f, 0xc0, 0x9e, 0x05, 0x25, 0xc7, 0x71, 0x9b, 0x19, 0xdd, 0x64, 0x1e, 0xe7, 0xdb,
            0xfa, 0xed, 0xc3, 0x45,
        ],
    );
}

#[test]
fn payload_corruption_is_rejected_before_decoder_dispatch() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, mut encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();
    encoded.flip_first_payload_bit_for_test();
    FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));

    assert_eq!(
        load_fixture(certificate, encoded).unwrap_err(),
        FamilyArtifactLoadErrorV1::PayloadDigestMismatch,
    );
    assert_eq!(FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get), 0);
}

#[test]
fn raw_payload_flip_is_rejected_before_bitmap_scan() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, mut encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();
    encoded.flip_first_payload_bit_for_test();

    assert_eq!(
        FamilyArtifactLoaderV1::load(certificate, encoded)
            .unwrap_err()
            .cause(),
        FamilyArtifactLoadErrorV1::PayloadDigestMismatch,
    );
}

#[test]
fn coherent_wrong_raw_length_is_typed_and_precedes_payload_hashing() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, mut encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();
    encoded.resize_payload_for_test(RAW_BITMAP24_PAYLOAD_LEN_V1 - 1);
    let certificate = encoded.reseal_payload_for_test(certificate);
    FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS.with(|calls| calls.set(0));

    assert_eq!(
        FamilyArtifactLoaderV1::load(certificate, encoded)
            .unwrap_err()
            .cause(),
        FamilyArtifactLoadErrorV1::CodecPayloadLengthMismatch {
            codec_release: 0x01,
            expected: RAW_BITMAP24_PAYLOAD_LEN_V1 as u64,
            actual: (RAW_BITMAP24_PAYLOAD_LEN_V1 - 1) as u64,
        },
    );
    assert_eq!(
        FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS.with(core::cell::Cell::get),
        0
    );
}

#[test]
fn resealed_raw_missing_or_extra_bit_is_a_member_count_mismatch() {
    let cases = [
        ([0, 0, 1], false, 2_u64, 1_u64),
        ([0, 0, 3], true, 2_u64, 3_u64),
    ];
    for (rgb, member, expected, actual) in cases {
        let members = signals(&[[0, 0, 1], [0, 0, 2]]);
        let (certificate, mut encoded) =
            encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();
        encoded.set_raw_bitmap_member_for_test(rgb, member);
        let certificate = encoded.reseal_payload_for_test(certificate);

        assert_eq!(
            FamilyArtifactLoaderV1::load(certificate, encoded)
                .unwrap_err()
                .cause(),
            FamilyArtifactLoadErrorV1::MemberCountMismatch { expected, actual },
        );
    }
}

#[test]
fn resealed_same_count_raw_substitution_is_an_image_mismatch() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, mut encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();
    encoded.set_raw_bitmap_member_for_test([0, 0, 1], false);
    encoded.set_raw_bitmap_member_for_test([0, 0, 3], true);
    let certificate = encoded.reseal_payload_for_test(certificate);

    assert_eq!(
        FamilyArtifactLoaderV1::load(certificate, encoded)
            .unwrap_err()
            .cause(),
        FamilyArtifactLoadErrorV1::ImageDigestMismatch,
    );
}

#[test]
fn envelope_discriminants_are_rejected_before_certificate_or_decoder_admission() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();
    let cases = [
        (
            encoded.clone().truncate_inside_header_for_test(),
            FamilyArtifactLoadErrorV1::HeaderTooShort,
        ),
        (
            encoded.clone().corrupt_magic_for_test(),
            FamilyArtifactLoadErrorV1::InvalidMagic,
        ),
        (
            encoded
                .clone()
                .corrupt_envelope_field_for_test(FixtureEnvelopeFieldV1::EnvelopeRelease),
            FamilyArtifactLoadErrorV1::UnsupportedEnvelope,
        ),
        (
            encoded
                .clone()
                .corrupt_envelope_field_for_test(FixtureEnvelopeFieldV1::SignalDomain),
            FamilyArtifactLoadErrorV1::UnsupportedSignalDomain,
        ),
        (
            encoded
                .clone()
                .corrupt_envelope_field_for_test(FixtureEnvelopeFieldV1::SignalOrdinal),
            FamilyArtifactLoadErrorV1::UnsupportedSignalOrdinal,
        ),
        (
            encoded
                .clone()
                .corrupt_envelope_field_for_test(FixtureEnvelopeFieldV1::ProofRelease),
            FamilyArtifactLoadErrorV1::UnsupportedProofRelease,
        ),
        (
            encoded.corrupt_envelope_field_for_test(FixtureEnvelopeFieldV1::VerifierRelease),
            FamilyArtifactLoadErrorV1::UnsupportedVerifierRelease,
        ),
    ];

    for (malformed, expected) in cases {
        FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));
        assert_eq!(load_fixture(certificate, malformed).unwrap_err(), expected);
        assert_eq!(FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get), 0);
    }
}

#[test]
fn receipt_semantic_codec_and_image_checks_reach_their_own_branches() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();

    let invalid_receipt = certificate.receipt_mismatch_for_test();
    FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));
    assert_eq!(
        load_fixture(
            invalid_receipt,
            encoded.clone().with_certificate_for_test(invalid_receipt),
        )
        .unwrap_err(),
        FamilyArtifactLoadErrorV1::ArtifactReceiptMismatch,
    );
    assert_eq!(FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get), 0);

    let invalid_semantic = certificate.semantic_mismatch_with_valid_receipt_for_test();
    FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));
    assert_eq!(
        load_fixture(
            invalid_semantic,
            encoded.clone().with_certificate_for_test(invalid_semantic),
        )
        .unwrap_err(),
        FamilyArtifactLoadErrorV1::SemanticReleaseMismatch,
    );
    assert_eq!(FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get), 0);

    let unsupported_codec = certificate.codec_with_valid_receipt_for_test(0x7f);
    FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));
    assert_eq!(
        load_fixture(
            unsupported_codec,
            encoded.clone().with_certificate_for_test(unsupported_codec),
        )
        .unwrap_err(),
        FamilyArtifactLoadErrorV1::UnsupportedCodec,
    );
    assert_eq!(FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get), 0);

    let invalid_image = certificate.image_mismatch_with_coherent_certificate_for_test();
    FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));
    assert_eq!(
        load_fixture(
            invalid_image,
            encoded.with_certificate_for_test(invalid_image),
        )
        .unwrap_err(),
        FamilyArtifactLoadErrorV1::ImageDigestMismatch,
    );
    assert_eq!(FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get), 1);

    let (reversed_certificate, reversed) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::ReversedMembersV1,
    )
    .unwrap();
    let wrong_decoder = reversed_certificate
        .codec_with_valid_receipt_for_test(FixtureFamilyArtifactCodecV1::CanonicalMembersV1 as u8);
    FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));
    assert_eq!(
        load_fixture(
            wrong_decoder,
            reversed.with_certificate_for_test(wrong_decoder),
        )
        .unwrap_err(),
        FamilyArtifactLoadErrorV1::InvalidCodecPayload,
    );
    assert_eq!(FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get), 1);
}

#[test]
fn impossible_srgb8_set_cardinality_is_rejected_before_decoder_dispatch() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();

    let impossible = certificate.member_count_with_coherent_certificate_for_test((1_u64 << 24) + 1);
    FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));
    assert_eq!(
        load_fixture(impossible, encoded.with_certificate_for_test(impossible),).unwrap_err(),
        FamilyArtifactLoadErrorV1::InvalidMemberCount,
    );
    assert_eq!(FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get), 0);
}

#[test]
fn full_srgb8_set_cardinality_reaches_codec_admission() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();
    let full = certificate.member_count_with_coherent_certificate_for_test(1_u64 << 24);
    let preflight_reached = core::cell::Cell::new(false);
    let decoder_reached = core::cell::Cell::new(false);

    let failure = FamilyArtifactLoaderV1::load_with_codec_for_test(
        full,
        encoded.with_certificate_for_test(full),
        |_codec, _count, _payload_len| {
            preflight_reached.set(true);
            Ok(())
        },
        |_codec, _count, _payload| {
            decoder_reached.set(true);
            Ok(Box::new([]))
        },
    )
    .unwrap_err();

    assert!(preflight_reached.get());
    assert!(decoder_reached.get());
    assert_eq!(
        failure.cause(),
        FamilyArtifactLoadErrorV1::MemberCountMismatch {
            expected: 1_u64 << 24,
            actual: 0,
        },
    );
}

#[test]
#[ignore = "full 24-bit domain oracle runs once in CI outside mutation tests"]
fn axis_membership_matches_the_full_srgb8_cube_oracle() {
    let members = (0_u16..=255)
        .map(|value| {
            let value = value as u8;
            ColorSignal::from_srgb8(Srgb8::new([value; 3]))
        })
        .collect::<Vec<_>>();
    let (certificate, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();
    let admitted = FamilyArtifactLoaderV1::load(certificate, encoded).unwrap();
    let mut visited = 0_u64;
    let mut observed_members = 0_u64;

    for red in 0_u16..=255 {
        for green in 0_u16..=255 {
            for blue in 0_u16..=255 {
                let bytes = [red as u8, green as u8, blue as u8];
                visited += 1;
                let actual = admitted.contains(ColorSignal::from_srgb8(Srgb8::new(bytes)));
                observed_members += u64::from(actual);
                assert_eq!(
                    actual,
                    bytes[0] == bytes[1] && bytes[1] == bytes[2],
                    "full-domain disagreement at {bytes:?}",
                );
            }
        }
    }
    assert_eq!(visited, 1_u64 << 24);
    assert_eq!(observed_members, 256);
}

#[test]
fn central_loader_rejects_a_decoder_that_lies_about_member_count() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();

    let failure = FamilyArtifactLoaderV1::load_with_decoder_for_test(
        certificate,
        encoded,
        |_codec, _declared_count, _payload| Ok(signals(&[[0, 0, 1]]).into_boxed_slice()),
    )
    .unwrap_err();

    assert_eq!(
        failure.cause(),
        FamilyArtifactLoadErrorV1::MemberCountMismatch {
            expected: 2,
            actual: 1,
        },
    );
}

#[test]
fn truncation_and_extension_fail_exact_length_before_decoder_dispatch() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();

    for malformed in [
        encoded.clone().truncate_one_byte_for_test(),
        encoded.extend_one_byte_for_test(),
    ] {
        FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));
        assert!(matches!(
            load_fixture(certificate, malformed),
            Err(FamilyArtifactLoadErrorV1::ExactLengthMismatch { .. }),
        ));
        assert_eq!(FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get), 0);
    }
}

#[test]
fn raw_truncation_and_extension_return_exact_length_mismatch_before_hashing() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();

    for malformed in [
        encoded.clone().truncate_one_byte_for_test(),
        encoded.extend_one_byte_for_test(),
    ] {
        FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS.with(|calls| calls.set(0));
        assert!(matches!(
            FamilyArtifactLoaderV1::load(certificate, malformed)
                .unwrap_err()
                .cause(),
            FamilyArtifactLoadErrorV1::ExactLengthMismatch { .. },
        ));
        assert_eq!(
            FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS.with(core::cell::Cell::get),
            0
        );
    }
}

#[test]
fn a_structurally_valid_foreign_certificate_is_not_a_generic_digest_error() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (expected, _) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();
    let (foreign, encoded) = encode_fixture_family_artifact_v2(
        FamilyDefinitionDigestV2::from_fixture_bytes_v2(b"foreign-definition"),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();
    assert_ne!(expected, foreign);
    FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));

    assert_eq!(
        load_fixture(expected, encoded).unwrap_err(),
        FamilyArtifactLoadErrorV1::ForeignCertificate,
    );
    assert_eq!(FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get), 0);
}

#[test]
fn admitted_storage_does_not_borrow_transport_bytes() {
    let member = ColorSignal::from_srgb8(Srgb8::new([0, 0, 255]));
    let (certificate, encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &[member],
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();

    let admitted = load_fixture(certificate, encoded).unwrap();

    assert!(admitted.contains(member));
}

#[test]
fn raw_admission_moves_the_original_allocation_into_executable_storage() {
    let members = signals(&[[0, 0, 1], [0, 0, 2], [255, 255, 255]]);
    let (certificate, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();
    let original = encoded.allocation_ptr_for_test();

    let (admitted, events) = crate::test_support::measured_allocator_events(|| {
        FamilyArtifactLoaderV1::load(certificate, encoded).unwrap()
    });

    assert_eq!(admitted.allocation_ptr_for_test(), Some(original));
    assert_eq!(events, crate::test_support::AllocatorEvents::default());
    let debug = format!("{admitted:?}");
    assert!(debug.starts_with("AdmittedFamilyArtifactV2 { semantic_release: "));
    assert!(debug.contains(", artifact_receipt: ") && debug.ends_with(", .. }"));
    assert!(
        !debug.contains("LCFAM2"),
        "Debug must not expose transport bytes"
    );
}

#[test]
fn raw_contains_and_assess_are_allocator_free_hot_lookups() {
    let members = signals(&[[0, 0, 1], [0, 0, 2], [255, 255, 255]]);
    let (certificate, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();
    let admitted = FamilyArtifactLoaderV1::load(certificate, encoded).unwrap();
    let hit = ColorSignal::from_srgb8(Srgb8::new([255, 255, 255]));
    let miss = ColorSignal::from_srgb8(Srgb8::new([255, 255, 254]));

    let ((contains_hit, contains_miss, (_, pass), (_, violation)), events) =
        crate::test_support::measured_allocator_events(|| {
            (
                admitted.contains(hit),
                admitted.contains(miss),
                admitted.assess(hit),
                admitted.assess(miss),
            )
        });

    assert!(contains_hit);
    assert!(!contains_miss);
    assert!(matches!(pass, crate::constraints::HardDecision::Pass(_)));
    assert!(matches!(
        violation,
        crate::constraints::HardDecision::Violation(_)
    ));
    assert_eq!(events, crate::test_support::AllocatorEvents::default());
}

#[test]
fn production_family_storage_has_no_decoded_member_box_or_binary_search_path() {
    let source = include_str!("family_artifact.rs");
    assert!(
        !source.contains("members: Box<[ColorSignal]>")
            && !source.contains("self.members\n            .binary_search_by_key"),
        "production admission must execute the owned bitmap instead of a second decoded set",
    );
    let raw = include_str!("family_artifact/raw_bitmap24.rs");
    assert!(!raw.contains("Box<[ColorSignal]>") && !raw.contains("binary_search"));
}

#[test]
fn every_certificate_identity_field_is_bound_before_decode() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();

    for mutant in certificate.identity_mutants_for_test() {
        FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));
        assert_eq!(
            load_fixture(mutant, encoded.clone()).unwrap_err(),
            FamilyArtifactLoadErrorV1::ForeignCertificate,
        );
        assert_eq!(FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get), 0);
    }
}

fn loaded_artifact(definition_suffix: &[u8]) -> AdmittedFamilyArtifactV2 {
    let definition = FamilyDefinitionDigestV2::from_fixture_bytes_v2(definition_suffix);
    let (certificate, encoded) = encode_fixture_family_artifact_v2(
        definition,
        &signals(&[[0, 0, 1], [0, 0, 2]]),
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();
    load_fixture(certificate, encoded).unwrap()
}

#[test]
fn exact_pool_binding_is_repairable_without_redecoding_retained_artifacts() {
    FAMILY_ARTIFACT_DECODER_CALLS.with(|calls| calls.set(0));
    let first = FamilyId::new(10);
    let second = FamilyId::new(20);
    let first_artifact = loaded_artifact(b"first");
    let first_semantic = first_artifact.semantic_release();
    let second_artifact = loaded_artifact(b"second");
    let second_semantic = second_artifact.semantic_release();
    let declarations = [
        FamilyDeclarationV2::new(first, first_semantic),
        FamilyDeclarationV2::new(second, second_semantic),
    ];

    let failure = FamilyArtifactBundleV2::from_artifacts(vec![first_artifact])
        .bind(&declarations)
        .unwrap_err();
    assert_eq!(
        failure.cause(),
        FamilyArtifactBindErrorV2::Contract(FamilyArtifactContractErrorV2::Missing {
            semantic: second_semantic,
        }),
    );
    assert_eq!(
        format!("{failure:?}"),
        format!(
            "FamilyArtifactBindFailureV2 {{ cause: {:?}, .. }}",
            failure.cause(),
        ),
        "owning bind failures must expose only their typed cause",
    );
    let (_, returned) = failure.into_parts();
    let mut retained = returned.into_artifacts();
    retained.push(second_artifact);
    let bound = FamilyArtifactBundleV2::from_artifacts(retained)
        .bind(&declarations)
        .unwrap();
    let mut execution = bound.execution_bindings();
    assert_eq!(execution.len(), 2);
    let first = execution.next().unwrap();
    assert_eq!(execution.len(), 1);
    let second = execution.next().unwrap();
    assert_ne!(first.semantic(), second.semantic());
    assert_eq!(execution.len(), 0);
    assert_eq!(execution.next(), None);
    assert_eq!(execution.next(), None, "the iterator must stay fused");
    assert_eq!(
        FAMILY_ARTIFACT_DECODER_CALLS.with(core::cell::Cell::get),
        2,
        "bundle repair must reuse both already decoded artifacts",
    );
}

#[test]
fn exact_pool_binding_rejects_extra_duplicate_and_wrong_semantic() {
    let first = FamilyId::new(10);
    let second = FamilyId::new(20);
    let first_artifact = loaded_artifact(b"first");
    let first_semantic = first_artifact.semantic_release();
    let second_artifact = loaded_artifact(b"second");
    let second_semantic = second_artifact.semantic_release();
    let declarations = [
        FamilyDeclarationV2::new(first, first_semantic),
        FamilyDeclarationV2::new(second, second_semantic),
    ];

    let extra_artifact = loaded_artifact(b"extra");
    let extra_semantic = extra_artifact.semantic_release();
    let failure = FamilyArtifactBundleV2::from_artifacts(vec![
        first_artifact,
        second_artifact,
        extra_artifact,
    ])
    .bind(&declarations)
    .unwrap_err();
    assert_eq!(
        failure.cause(),
        FamilyArtifactBindErrorV2::Contract(FamilyArtifactContractErrorV2::Extra {
            semantic: extra_semantic,
        }),
    );

    let duplicate_semantic = first_semantic;
    let failure = FamilyArtifactBundleV2::from_artifacts(vec![
        loaded_artifact(b"first"),
        loaded_artifact(b"first"),
        loaded_artifact(b"second"),
    ])
    .bind(&declarations)
    .unwrap_err();
    assert_eq!(
        failure.cause(),
        FamilyArtifactBindErrorV2::Contract(FamilyArtifactContractErrorV2::Duplicate {
            semantic: duplicate_semantic,
        }),
    );

    let failure = FamilyArtifactBundleV2::from_artifacts(vec![
        loaded_artifact(b"wrong"),
        loaded_artifact(b"second"),
    ])
    .bind(&declarations)
    .unwrap_err();
    assert_eq!(
        failure.cause(),
        FamilyArtifactBindErrorV2::Contract(FamilyArtifactContractErrorV2::Missing {
            semantic: first_semantic,
        }),
    );
}

#[test]
fn missing_error_order_is_invariant_under_opaque_id_rename_and_permutation() {
    let first_semantic = loaded_artifact(b"missing-first").semantic_release();
    let second_semantic = loaded_artifact(b"missing-second").semantic_release();
    let expected = first_semantic.min(second_semantic);
    let first = [
        FamilyDeclarationV2::new(FamilyId::new(1), first_semantic),
        FamilyDeclarationV2::new(FamilyId::new(2), second_semantic),
    ];
    let renamed_and_permuted = [
        FamilyDeclarationV2::new(FamilyId::new(900), second_semantic),
        FamilyDeclarationV2::new(FamilyId::new(3), first_semantic),
    ];

    for declarations in [&first[..], &renamed_and_permuted[..]] {
        let failure = FamilyArtifactBundleV2::empty()
            .bind(declarations)
            .unwrap_err();
        assert_eq!(
            failure.cause(),
            FamilyArtifactBindErrorV2::Contract(FamilyArtifactContractErrorV2::Missing {
                semantic: expected,
            }),
        );
    }
}

#[test]
fn one_semantic_artifact_serves_two_opaque_family_aliases() {
    let first = FamilyId::new(10);
    let alias = FamilyId::new(20);
    let artifact = loaded_artifact(b"shared");
    let semantic = artifact.semantic_release();
    let receipt = artifact.artifact_receipt();
    let declarations = [
        FamilyDeclarationV2::new(first, semantic),
        FamilyDeclarationV2::new(alias, semantic),
    ];
    let bound = FamilyArtifactBundleV2::from_artifacts(vec![artifact])
        .bind(&declarations)
        .unwrap();

    assert!(core::ptr::eq(
        bound.artifact(0).unwrap(),
        bound.artifact(1).unwrap(),
    ));
    let execution = bound.execution_bindings().collect::<Vec<_>>();
    assert_eq!(execution.len(), 1);
    assert!(
        execution
            .iter()
            .all(|binding| binding.semantic() == semantic && binding.receipt() == receipt)
    );
    assert_ne!(semantic.as_bytes(), receipt.as_bytes());
}

#[test]
fn production_loader_rejects_unreleased_codec_and_returns_exact_transport() {
    let members = signals(&[[0, 0, 1], [0, 0, 2]]);
    let (certificate, encoded) = encode_fixture_family_artifact_v2(
        definition(),
        &members,
        FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
    )
    .unwrap();
    let exact_transport = encoded.clone();
    FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS.with(|calls| calls.set(0));

    let failure = FamilyArtifactLoaderV1::load(certificate, encoded).unwrap_err();

    assert_eq!(failure.cause(), FamilyArtifactLoadErrorV1::UnsupportedCodec,);
    assert_eq!(
        format!("{failure:?}"),
        "FamilyArtifactLoadFailureV1 { cause: UnsupportedCodec, .. }",
        "owning failures must not dump transport bytes",
    );
    let (cause, returned) = failure.into_parts();
    assert_eq!(cause, FamilyArtifactLoadErrorV1::UnsupportedCodec);
    assert_eq!(returned, exact_transport);
    assert_eq!(
        FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS.with(core::cell::Cell::get),
        0,
        "unsupported codecs must fail before O(payload) hashing",
    );
}

// Слой lookup-only: потребитель держит доверенную запись сертификата из
// одного канала и недоверенные байты артефакта из другого. Без публичного
// разбора записи предъявить `expected` невозможно, и загрузчик недостижим.
#[test]
fn a_trusted_certificate_record_round_trips_and_admits_its_artifact() {
    let members = signals(&[[0, 0, 0], [1, 0, 0], [255, 255, 255]]);
    let (certificate, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();

    let bytes = encoded.clone().into_bytes();
    let record = &bytes[..FAMILY_CERTIFICATE_RECORD_LEN_V2];

    let trusted = FamilyImageCertificateV2::parse_trusted(record).unwrap();
    assert_eq!(trusted, certificate);
    // Запись несёт адрес определения: без него она говорит «артефакт цел»,
    // но не «это образ того региона, который я спрашивал».
    assert_eq!(trusted.definition_digest(), definition());
    assert_eq!(trusted.member_count(), members.len() as u64);

    // Запись доверенная, артефакт — нет: связывает их именно сравнение.
    let admitted = FamilyArtifactLoaderV1::load(trusted, encoded).unwrap();
    assert!(admitted.contains(ColorSignal::from_srgb8(Srgb8::new([1, 0, 0]))));
    assert!(!admitted.contains(ColorSignal::from_srgb8(Srgb8::new([2, 0, 0]))));
}

#[test]
fn a_trusted_certificate_record_is_validated_exactly_as_the_envelope_is() {
    let members = signals(&[[0, 0, 1]]);
    let (_, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();
    let bytes = encoded.into_bytes();
    let record = &bytes[..FAMILY_CERTIFICATE_RECORD_LEN_V2];

    assert_eq!(
        FamilyImageCertificateV2::parse_trusted(&record[..record.len() - 1]).unwrap_err(),
        FamilyArtifactLoadErrorV1::HeaderTooShort,
    );
    let mut extended = record.to_vec();
    extended.push(0);
    assert_eq!(
        FamilyImageCertificateV2::parse_trusted(&extended).unwrap_err(),
        FamilyArtifactLoadErrorV1::ExactLengthMismatch {
            expected: FAMILY_CERTIFICATE_RECORD_LEN_V2,
            actual: FAMILY_CERTIFICATE_RECORD_LEN_V2 + 1,
        },
    );

    let mut foreign_magic = record.to_vec();
    foreign_magic[0] ^= 1;
    assert_eq!(
        FamilyImageCertificateV2::parse_trusted(&foreign_magic).unwrap_err(),
        FamilyArtifactLoadErrorV1::InvalidMagic,
    );

    // Каждый дискриминант обязан отвергаться тем же типизированным отказом,
    // что и при разборе конверта: два валидатора разошлись бы.
    for (offset, expected) in [
        (8_usize, FamilyArtifactLoadErrorV1::UnsupportedEnvelope),
        (10, FamilyArtifactLoadErrorV1::UnsupportedSignalDomain),
        (11, FamilyArtifactLoadErrorV1::UnsupportedSignalOrdinal),
        (12, FamilyArtifactLoadErrorV1::UnsupportedProofRelease),
        (13, FamilyArtifactLoadErrorV1::UnsupportedVerifierRelease),
    ] {
        let mut drifted = record.to_vec();
        drifted[offset] ^= 0x40;
        assert_eq!(
            FamilyImageCertificateV2::parse_trusted(&drifted).unwrap_err(),
            expected,
            "offset {offset}",
        );
    }
}

#[test]
fn a_record_broken_in_its_own_channel_is_refused_at_the_door() {
    // Раньше такая запись проходила разбор и падала позже как «чужой
    // артефакт» — обвиняя payload в том, что сломана запись.
    let (_, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &signals(&[[0, 0, 1]]))
            .unwrap();
    let bytes = encoded.into_bytes();
    let record = &bytes[..FAMILY_CERTIFICATE_RECORD_LEN_V2];

    // artifact_receipt — последние 32 байта записи; он покрывает все
    // остальные поля, поэтому любая одиночная порча ловится именно им.
    let mut corrupt = record.to_vec();
    let last = corrupt.len() - 1;
    corrupt[last] ^= 1;
    assert_eq!(
        FamilyImageCertificateV2::parse_trusted(&corrupt).unwrap_err(),
        FamilyArtifactLoadErrorV1::ArtifactReceiptMismatch,
    );

    // Anti-vacuity: нетронутая запись проходит.
    assert!(FamilyImageCertificateV2::parse_trusted(record).is_ok());
}

#[test]
fn a_record_with_a_forged_semantic_release_is_refused_at_the_door() {
    // Порча semantic_release с ПЕРЕСЧИТАННЫМ поверх неё receipt проходит
    // receipt-чек и обязана упасть именно на пересчёте semantic release —
    // без него подделка доехала бы до артефакта и назвалась бы чужим.
    let members = signals(&[[0, 0, 1]]);
    let (certificate, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &members).unwrap();

    let forged = certificate.semantic_mismatch_with_valid_receipt_for_test();
    let bytes = encoded.with_certificate_for_test(forged).into_bytes();
    assert_eq!(
        FamilyImageCertificateV2::parse_trusted(&bytes[..FAMILY_CERTIFICATE_RECORD_LEN_V2])
            .unwrap_err(),
        FamilyArtifactLoadErrorV1::SemanticReleaseMismatch,
    );
}

#[test]
fn a_record_refuses_in_the_same_order_the_envelope_does() {
    // Слишком длинная запись с чужой магией обязана называть магию, как это
    // делает конверт: иначе два пути объясняют одну и ту же порчу по-разному.
    let (_, encoded) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &signals(&[[0, 0, 1]]))
            .unwrap();
    let bytes = encoded.into_bytes();
    let mut hostile = bytes[..FAMILY_CERTIFICATE_RECORD_LEN_V2].to_vec();
    hostile[0] ^= 1;
    hostile.push(0);
    assert_eq!(
        FamilyImageCertificateV2::parse_trusted(&hostile).unwrap_err(),
        FamilyArtifactLoadErrorV1::InvalidMagic,
    );
}

#[test]
fn a_trusted_record_of_one_artifact_never_admits_another() {
    // Через доверенную запись, а не через тестовый конструктор: иначе тест
    // проверял бы старый загрузчик и прошёл бы без этого среза целиком.
    let (_, first) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &signals(&[[0, 0, 1]]))
            .unwrap();
    let first_bytes = first.into_bytes();
    let first_certificate =
        FamilyImageCertificateV2::parse_trusted(&first_bytes[..FAMILY_CERTIFICATE_RECORD_LEN_V2])
            .unwrap();
    let (_, second) =
        encode_raw_bitmap24_family_artifact_v2_for_test(definition(), &signals(&[[0, 0, 2]]))
            .unwrap();

    // Anti-vacuity: обе стороны валидны по отдельности, связь ломает именно
    // несовпадение записи с артефактом.
    assert_eq!(
        FamilyArtifactLoaderV1::load(first_certificate, second)
            .unwrap_err()
            .cause(),
        FamilyArtifactLoadErrorV1::ForeignCertificate,
    );
}

proptest! {
    #[test]
    fn arbitrary_owned_transport_never_panics(
        header in proptest::collection::vec(any::<u8>(), 0..=256),
        payload in proptest::collection::vec(any::<u8>(), 0..=256),
    ) {
        let members = signals(&[[0, 0, 1]]);
        let (certificate, _) = encode_fixture_family_artifact_v2(
            definition(),
            &members,
            FixtureFamilyArtifactCodecV1::CanonicalMembersV1,
        )
        .unwrap();
        let mut bytes = header;
        bytes.extend_from_slice(&payload);

        let _ = FamilyArtifactLoaderV1::load(
            certificate,
            EncodedFamilyArtifactV2::from_raw_bytes_for_test(bytes),
        );
    }
}
