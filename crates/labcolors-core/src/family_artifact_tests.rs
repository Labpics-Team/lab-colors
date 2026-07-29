//! Hostile-контракт transport artifact семейства до первого production codec.

use proptest::prelude::*;

use crate::Srgb8;
use crate::family::{
    CanonicalFamilyImageErrorV2, FamilyDeclarationV2, FamilyDefinitionDigestV2, FamilyId,
    canonical_family_image_digest_v2,
};
use crate::family_artifact::{
    AdmittedFamilyArtifactV2, EncodedFamilyArtifactV2, FAMILY_ARTIFACT_DECODER_CALLS,
    FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS, FamilyArtifactBindErrorV2, FamilyArtifactBundleV2,
    FamilyArtifactContractErrorV2, FamilyArtifactLoadErrorV1, FamilyArtifactLoaderV1,
    FamilyImageCertificateV2, FixtureEnvelopeFieldV1, FixtureFamilyArtifactCodecV1,
    encode_fixture_family_artifact_v2,
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
        FamilyArtifactLoadErrorV1::DecodedMemberCountMismatch {
            expected: 1_u64 << 24,
            actual: 0,
        },
    );
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
        FamilyArtifactLoadErrorV1::DecodedMemberCountMismatch {
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
