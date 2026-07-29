//! Контракт точного образа генератора family, которым владеет код.

use crate::Srgb8;
use crate::constraints::HardDecision;
use crate::family::{
    AdmittedFamilySetV1, CompleteFamilyGeneratorV1, FamilyImageErrorV1, FamilyImageProofReleaseV1,
    UnverifiedFamilyImageV1, admit_declared_family_image_v1, verify_complete_family_image_v1,
};
use crate::lcs_occurrence::ColorSignal;
use proptest::prelude::*;
use std::collections::BTreeSet;

fn signal(bytes: [u8; 3]) -> ColorSignal {
    ColorSignal::from_srgb8(Srgb8::new(bytes))
}

fn is_member(admitted: &AdmittedFamilySetV1, value: ColorSignal) -> bool {
    matches!(admitted.assess(value).1, HardDecision::Pass(_))
}

#[test]
fn verifier_rejects_both_missing_and_extraneous_image_members() {
    let generator = CompleteFamilyGeneratorV1::encoded_srgb8_equal_channel_axis_v1();
    let missing = signal([0, 0, 0]);
    let extraneous = signal([0, 0, 1]);
    let mut proposed = (1_u16..=255)
        .map(|value| {
            let value = value as u8;
            signal([value, value, value])
        })
        .collect::<Vec<_>>();
    proposed.push(extraneous);

    assert_eq!(
        verify_complete_family_image_v1(generator, UnverifiedFamilyImageV1::new(proposed)),
        Err(FamilyImageErrorV1::ImageMismatch {
            missing: Some(missing),
            extraneous: Some(extraneous),
        }),
    );
}

#[test]
fn exact_axis_and_chromatic_fixture_use_one_verifier_and_membership_law() {
    let axis = CompleteFamilyGeneratorV1::encoded_srgb8_equal_channel_axis_v1();
    let axis_image = UnverifiedFamilyImageV1::new(
        (0_u16..=255)
            .map(|value| {
                let value = value as u8;
                signal([value; 3])
            })
            .collect(),
    );
    let axis = verify_complete_family_image_v1(axis, axis_image).unwrap();
    let chromatic = CompleteFamilyGeneratorV1::encoded_srgb8_red_blue_diagonal_v1();
    let chromatic_image = UnverifiedFamilyImageV1::new(
        (0_u16..=255)
            .map(|value| {
                let value = value as u8;
                signal([value, 0, 255 - value])
            })
            .collect(),
    );
    let chromatic = verify_complete_family_image_v1(chromatic, chromatic_image).unwrap();

    assert_eq!(axis.certificate().member_count(), 256);
    assert_eq!(chromatic.certificate().member_count(), 256);
    assert_ne!(
        axis.certificate().family_content_identity(),
        chromatic.certificate().family_content_identity(),
    );
    for value in 0_u16..=255 {
        let value = value as u8;
        assert!(matches!(
            axis.assess(signal([value, value, value])).1,
            HardDecision::Pass(_),
        ));
        assert!(matches!(
            chromatic.assess(signal([value, 0, 255 - value])).1,
            HardDecision::Pass(_),
        ));
    }
}

#[test]
fn axis_membership_matches_the_full_srgb8_cube_oracle() {
    let generator = CompleteFamilyGeneratorV1::encoded_srgb8_equal_channel_axis_v1();
    let image = UnverifiedFamilyImageV1::new(
        (0_u16..=255)
            .map(|value| {
                let value = value as u8;
                signal([value; 3])
            })
            .collect(),
    );
    let admitted = verify_complete_family_image_v1(generator, image).unwrap();

    for red in 0_u16..=255 {
        for green in 0_u16..=255 {
            for blue in 0_u16..=255 {
                let bytes = [red as u8, green as u8, blue as u8];
                assert_eq!(
                    is_member(&admitted, signal(bytes)),
                    bytes[0] == bytes[1] && bytes[1] == bytes[2],
                    "full-domain disagreement at {bytes:?}",
                );
            }
        }
    }
}

#[test]
fn proposed_representation_order_and_duplicates_do_not_change_the_image() {
    let generator = CompleteFamilyGeneratorV1::encoded_srgb8_equal_channel_axis_v1();
    let canonical = UnverifiedFamilyImageV1::new(
        (0_u16..=255)
            .map(|value| {
                let value = value as u8;
                signal([value; 3])
            })
            .collect(),
    );
    let expected = verify_complete_family_image_v1(generator.clone(), canonical).unwrap();
    let mut reordered = (0_u16..=255)
        .rev()
        .flat_map(|value| {
            let value = value as u8;
            [signal([value; 3]), signal([value; 3])]
        })
        .collect::<Vec<_>>();
    reordered.rotate_left(73);
    let actual =
        verify_complete_family_image_v1(generator, UnverifiedFamilyImageV1::new(reordered))
            .unwrap();

    assert_eq!(
        expected.certificate().image_content_identity(),
        actual.certificate().image_content_identity(),
    );
    assert_eq!(
        expected.certificate().family_content_identity(),
        actual.certificate().family_content_identity(),
    );
    for value in [
        signal([0; 3]),
        signal([127; 3]),
        signal([255; 3]),
        signal([1, 2, 3]),
    ] {
        assert_eq!(is_member(&expected, value), is_member(&actual, value));
    }
}

#[test]
fn production_declared_set_is_nonempty_canonical_and_content_addressed() {
    assert_eq!(
        admit_declared_family_image_v1(Vec::new()),
        Err(FamilyImageErrorV1::EmptyGeneratorDomain),
    );
    let first = admit_declared_family_image_v1(vec![
        signal([9, 8, 7]),
        signal([1, 2, 3]),
        signal([9, 8, 7]),
    ])
    .unwrap();
    let second =
        admit_declared_family_image_v1(vec![signal([1, 2, 3]), signal([9, 8, 7])]).unwrap();

    assert_eq!(first.certificate(), second.certificate());
    assert_eq!(first.certificate().member_count(), 2);
    assert_eq!(
        first.certificate().proof_release(),
        FamilyImageProofReleaseV1::DeclaredImageIsDefinitionV1,
    );
    assert!(is_member(&first, signal([1, 2, 3])));
    assert!(!is_member(&first, signal([1, 2, 4])));
}

#[test]
fn declared_family_codec_matches_independent_sha256_golden_v1() {
    let admitted = admit_declared_family_image_v1(vec![
        signal([9, 8, 7]),
        signal([1, 2, 3]),
        signal([9, 8, 7]),
    ])
    .unwrap();
    let certificate = admitted.certificate();

    // These bytes come from an independent Python hashlib construction of the
    // documented V1 preimages. Changing them requires an explicit codec/release
    // decision; deriving them through this Rust path would make the oracle vacuous.
    assert_eq!(
        *certificate.generator_content_identity().as_bytes(),
        [
            0x5a, 0xa1, 0x0b, 0x2e, 0xa4, 0xa7, 0x6a, 0x55, 0x47, 0x9c, 0xa0, 0xec, 0xd8, 0xce,
            0xba, 0x04, 0x7e, 0x8d, 0xc4, 0x4d, 0xd4, 0x9f, 0x8b, 0x16, 0x08, 0x35, 0x3d, 0xfa,
            0xbe, 0xe8, 0x74, 0xf2,
        ],
    );
    assert_eq!(
        *certificate.image_content_identity().as_bytes(),
        [
            0xe0, 0x1b, 0xaa, 0xb6, 0x6b, 0x05, 0x8f, 0x96, 0xaa, 0x45, 0x12, 0xc1, 0x36, 0xfa,
            0x49, 0xdf, 0x71, 0x57, 0xb4, 0x19, 0x35, 0x56, 0xd0, 0x40, 0xf1, 0x60, 0xaf, 0x00,
            0x03, 0x29, 0xf3, 0x00,
        ],
    );
    assert_eq!(
        *certificate.family_content_identity().as_bytes(),
        [
            0x9d, 0xb4, 0x15, 0xca, 0xa4, 0x84, 0x0b, 0x3b, 0xb3, 0xd0, 0x94, 0x61, 0x11, 0x43,
            0x02, 0x9b, 0x80, 0xe1, 0x78, 0xbc, 0x58, 0x9d, 0x5b, 0x33, 0x5f, 0x02, 0x76, 0x44,
            0xf5, 0x97, 0x28, 0x83,
        ],
    );
}

#[test]
fn image_identity_changes_when_one_canonical_member_changes() {
    let first = admit_declared_family_image_v1(vec![signal([1, 2, 3]), signal([9, 8, 7])]).unwrap();
    let second =
        admit_declared_family_image_v1(vec![signal([1, 2, 4]), signal([9, 8, 7])]).unwrap();

    assert_eq!(
        first.certificate().generator_release(),
        second.certificate().generator_release(),
    );
    assert_ne!(
        first.certificate().image_content_identity(),
        second.certificate().image_content_identity(),
    );
    assert_ne!(
        first.certificate().family_content_identity(),
        second.certificate().family_content_identity(),
    );
}

#[test]
fn identical_image_under_distinct_generator_releases_has_distinct_provenance() {
    let image = (0_u16..=255)
        .map(|value| {
            let value = value as u8;
            signal([value; 3])
        })
        .collect::<Vec<_>>();
    let declared = admit_declared_family_image_v1(image.clone()).unwrap();
    let fixture_generator = CompleteFamilyGeneratorV1::encoded_srgb8_equal_channel_axis_v1();
    let fixture =
        verify_complete_family_image_v1(fixture_generator, UnverifiedFamilyImageV1::new(image))
            .unwrap();

    assert_eq!(
        declared.certificate().image_content_identity(),
        fixture.certificate().image_content_identity(),
    );
    assert_ne!(
        declared.certificate().generator_content_identity(),
        fixture.certificate().generator_content_identity(),
    );
    assert_ne!(
        declared.certificate().family_content_identity(),
        fixture.certificate().family_content_identity(),
    );
    assert_eq!(
        fixture.certificate().proof_release(),
        FamilyImageProofReleaseV1::ExhaustiveCanonicalImageComparisonV1,
    );
}

#[test]
fn proof_release_distinguishes_definition_from_exhaustive_verification() {
    let image = vec![signal([9, 8, 7]), signal([1, 2, 3])];
    let direct = admit_declared_family_image_v1(image.clone()).unwrap();
    let generator = CompleteFamilyGeneratorV1::try_declared_finite_image_v1(image).unwrap();
    let exhaustive = verify_complete_family_image_v1(
        generator,
        UnverifiedFamilyImageV1::new(vec![signal([1, 2, 3]), signal([9, 8, 7])]),
    )
    .unwrap();

    assert_eq!(
        direct.certificate().generator_content_identity(),
        exhaustive.certificate().generator_content_identity(),
    );
    assert_eq!(
        direct.certificate().image_content_identity(),
        exhaustive.certificate().image_content_identity(),
    );
    assert_ne!(
        direct.certificate().proof_release(),
        exhaustive.certificate().proof_release(),
    );
    assert_ne!(
        direct.certificate().family_content_identity(),
        exhaustive.certificate().family_content_identity(),
    );
}

#[test]
fn noninjective_unordered_generator_binds_preimage_and_canonical_image_separately() {
    let generator = CompleteFamilyGeneratorV1::noninjective_unordered_fixture_v1();
    let admitted = verify_complete_family_image_v1(
        generator,
        UnverifiedFamilyImageV1::new(vec![signal([10; 3]), signal([20; 3])]),
    )
    .unwrap();

    assert_eq!(admitted.certificate().preimage_count(), 4);
    assert_eq!(admitted.certificate().member_count(), 2);
    assert!(is_member(&admitted, signal([10; 3])));
    assert!(is_member(&admitted, signal([20; 3])));
    assert!(!is_member(&admitted, signal([15; 3])));
    admitted.verify().unwrap();
}

#[test]
fn generator_identity_binds_ordinal_mapping_within_one_release() {
    let first_generator = CompleteFamilyGeneratorV1::noninjective_unordered_fixture_v1();
    let second_generator = CompleteFamilyGeneratorV1::permuted_noninjective_unordered_fixture_v1();
    let proposed = || UnverifiedFamilyImageV1::new(vec![signal([10; 3]), signal([20; 3])]);
    let first = verify_complete_family_image_v1(first_generator, proposed()).unwrap();
    let second = verify_complete_family_image_v1(second_generator, proposed()).unwrap();

    assert_eq!(
        first.certificate().generator_release(),
        second.certificate().generator_release(),
    );
    assert_eq!(
        first.certificate().image_content_identity(),
        second.certificate().image_content_identity(),
    );
    assert_eq!(
        first.certificate().preimage_count(),
        second.certificate().preimage_count(),
    );
    assert_ne!(
        first.certificate().generator_content_identity(),
        second.certificate().generator_content_identity(),
    );
    assert_ne!(
        first.certificate().family_content_identity(),
        second.certificate().family_content_identity(),
    );
}

#[test]
fn generator_identity_binds_parameters_even_when_quantized_output_is_identical() {
    let first_generator = CompleteFamilyGeneratorV1::noninjective_fixture_with_provenance_v1(7);
    let second_generator = CompleteFamilyGeneratorV1::noninjective_fixture_with_provenance_v1(8);
    let proposed = || UnverifiedFamilyImageV1::new(vec![signal([10; 3]), signal([20; 3])]);
    let first = verify_complete_family_image_v1(first_generator, proposed()).unwrap();
    let second = verify_complete_family_image_v1(second_generator, proposed()).unwrap();

    assert_eq!(
        first.certificate().generator_release(),
        second.certificate().generator_release(),
    );
    assert_eq!(
        first.certificate().image_content_identity(),
        second.certificate().image_content_identity(),
    );
    assert_ne!(
        first.certificate().generator_content_identity(),
        second.certificate().generator_content_identity(),
    );
    assert_ne!(
        first.certificate().family_content_identity(),
        second.certificate().family_content_identity(),
    );
}

#[test]
fn exclusion_witnesses_cover_below_between_and_above() {
    let admitted = admit_declared_family_image_v1(vec![signal([10; 3]), signal([20; 3])]).unwrap();
    let expected_family = admitted.certificate().family_content_identity();
    for (query, insertion, lower, upper) in [
        (signal([0; 3]), 0, None, Some(signal([10; 3]))),
        (
            signal([15; 3]),
            1,
            Some(signal([10; 3])),
            Some(signal([20; 3])),
        ),
        (signal([30; 3]), 2, Some(signal([20; 3])), None),
    ] {
        let (measurement, HardDecision::Violation(proof)) = admitted.assess(query) else {
            panic!("every query is outside the declared set");
        };
        assert_eq!(measurement.family(), expected_family);
        assert_eq!(measurement.signal(), query);
        assert_eq!(proof.insertion_rank(), insertion);
        assert_eq!(proof.lower(), lower);
        assert_eq!(proof.upper(), upper);
    }

    let included = signal([10; 3]);
    let (measurement, HardDecision::Pass(_)) = admitted.assess(included) else {
        panic!("declared member must pass");
    };
    assert_eq!(measurement.family(), expected_family);
    assert_eq!(measurement.signal(), included);
}

#[test]
fn corrupted_admitted_storage_fails_closed_before_program_use() {
    let mut admitted = admit_declared_family_image_v1(vec![signal([1, 2, 3])]).unwrap();
    admitted.corrupt_first_member_for_test(signal([4, 5, 6]));

    assert_eq!(
        admitted.verify(),
        Err(FamilyImageErrorV1::CertificateMismatch)
    );
}

#[test]
fn corrupted_generated_image_fails_independent_generator_replay() {
    let generator = CompleteFamilyGeneratorV1::encoded_srgb8_equal_channel_axis_v1();
    let proposed = generator.clone().into_complete_output().unwrap();
    let mut admitted = verify_complete_family_image_v1(generator, proposed).unwrap();
    // Остаётся строго между прежними first и second members, поэтому отказ
    // доказывает replay generator-а, а не только проверку сортировки.
    admitted.corrupt_first_member_for_test(signal([0, 0, 1]));

    assert_eq!(
        admitted.verify(),
        Err(FamilyImageErrorV1::CertificateMismatch),
    );
}

#[test]
fn coherent_but_unadmitted_proof_release_fails_closed() {
    let generator = CompleteFamilyGeneratorV1::encoded_srgb8_equal_channel_axis_v1();
    let proposed = generator.clone().into_complete_output().unwrap();
    let mut admitted = verify_complete_family_image_v1(generator, proposed).unwrap();
    admitted.recertify_proof_for_test(FamilyImageProofReleaseV1::DeclaredImageIsDefinitionV1);

    assert_eq!(
        admitted.verify(),
        Err(FamilyImageErrorV1::CertificateMismatch),
    );
}

#[test]
fn coherent_but_wrong_preimage_count_fails_generator_replay() {
    let generator = CompleteFamilyGeneratorV1::encoded_srgb8_equal_channel_axis_v1();
    let proposed = generator.clone().into_complete_output().unwrap();
    let mut admitted = verify_complete_family_image_v1(generator, proposed).unwrap();
    admitted.recertify_preimage_count_for_test(257);

    assert_eq!(
        admitted.verify(),
        Err(FamilyImageErrorV1::CertificateMismatch),
    );
}

proptest! {
    #[test]
    fn declared_membership_matches_an_independent_btree_oracle(
        raw in prop::collection::vec(any::<[u8; 3]>(), 1..512),
        queries in prop::collection::vec(any::<[u8; 3]>(), 0..256),
    ) {
        let oracle = raw.iter().copied().collect::<BTreeSet<_>>();
        let admitted = admit_declared_family_image_v1(
            raw.into_iter().map(signal).collect(),
        ).unwrap();

        for query in queries {
            prop_assert_eq!(is_member(&admitted, signal(query)), oracle.contains(&query));
        }
    }
}

#[test]
fn repeated_membership_assessment_allocates_nothing() {
    let admitted = admit_declared_family_image_v1(vec![signal([100, 100, 100])]).unwrap();

    let (_, allocations) = crate::test_support::measured_allocations(|| {
        let mut checksum = 0_usize;
        for value in [
            signal([0, 0, 0]),
            signal([100, 100, 100]),
            signal([100, 100, 101]),
            signal([255, 255, 255]),
        ] {
            let (measurement, decision) = admitted.assess(value);
            checksum ^= measurement.signal().srgb8().bytes()[0] as usize;
            checksum ^= match decision {
                HardDecision::Pass(proof) => proof.rank(),
                HardDecision::Violation(proof) => proof.insertion_rank(),
            };
        }
        checksum
    });
    assert_eq!(allocations, 0);
}

#[test]
fn fixture_materializers_match_independent_formulas() {
    for generator in [
        CompleteFamilyGeneratorV1::encoded_srgb8_equal_channel_axis_v1(),
        CompleteFamilyGeneratorV1::encoded_srgb8_red_blue_diagonal_v1(),
    ] {
        let generated = generator.clone().into_complete_output().unwrap();
        let admitted = verify_complete_family_image_v1(generator.clone(), generated).unwrap();
        for value in 0_u16..=255 {
            let value = value as u8;
            let expected = match generator {
                CompleteFamilyGeneratorV1::EncodedSrgb8EqualChannelAxisV1 => signal([value; 3]),
                CompleteFamilyGeneratorV1::EncodedSrgb8RedBlueDiagonalV1 => {
                    signal([value, 0, 255 - value])
                }
                CompleteFamilyGeneratorV1::DeclaredFiniteImageV1 { .. } => unreachable!(),
                CompleteFamilyGeneratorV1::NonInjectiveUnorderedFixtureV1 { .. } => unreachable!(),
            };
            assert!(is_member(&admitted, expected));
        }
    }
}
