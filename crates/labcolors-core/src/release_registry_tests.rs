use crate::lcs_occurrence::{CAM16_VIEW_RELEASE_V1, OKLAB_VIEW_RELEASE_V1};
use crate::output_projection::{
    CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1, DifferenceCalibrationReleaseIdV1,
    OKLCH_VIEW_RELEASE_V1,
};
use crate::release_registry::{
    RELEASE_REGISTRY_SCHEMA_VERSION_V1, RegisteredColorReleaseIdV1, ReleaseRegistryAvailabilityV1,
    ReleaseRegistryClassV1, ReleaseRegistryDigestAlgorithmV1, ReleaseRegistryRecordV1,
    impossible_difference_calibration_release_v1, release_registry_canonical_bytes_v1,
    release_registry_digest_v1, release_registry_records_v1,
};

#[test]
fn registry_contains_only_the_four_implemented_releases() {
    assert_eq!(
        release_registry_records_v1(),
        &[
            ReleaseRegistryRecordV1::Registered(RegisteredColorReleaseIdV1::Cam16View(
                CAM16_VIEW_RELEASE_V1,
            )),
            ReleaseRegistryRecordV1::Registered(RegisteredColorReleaseIdV1::OklabView(
                OKLAB_VIEW_RELEASE_V1,
            )),
            ReleaseRegistryRecordV1::Registered(RegisteredColorReleaseIdV1::OklchView(
                OKLCH_VIEW_RELEASE_V1,
            )),
            ReleaseRegistryRecordV1::DifferenceCalibrationUnavailable,
            ReleaseRegistryRecordV1::Registered(
                RegisteredColorReleaseIdV1::CssColor4OklchD65FromModeledSrgb8Solid(
                    CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1,
                ),
            ),
        ],
    );
}

#[test]
fn difference_registry_is_explicitly_unavailable_and_release_type_is_uninhabited() {
    let row = release_registry_records_v1()
        .iter()
        .copied()
        .find(|row| row.class() == ReleaseRegistryClassV1::DifferenceCalibration)
        .expect("difference availability row");
    assert_eq!(
        row,
        ReleaseRegistryRecordV1::DifferenceCalibrationUnavailable
    );
    assert_eq!(
        row.availability(),
        ReleaseRegistryAvailabilityV1::Unavailable
    );
    assert_eq!(row.release(), None);

    let _: fn(DifferenceCalibrationReleaseIdV1) -> ! = impossible_difference_calibration_release_v1;
}

#[test]
fn registered_rows_have_stable_exact_release_keys() {
    let keys: Vec<_> = release_registry_records_v1()
        .iter()
        .filter_map(|row| row.release().map(RegisteredColorReleaseIdV1::key))
        .collect();
    assert_eq!(
        keys,
        [
            "cam16-li-et-al-2017-cie-248-forward-v1",
            "oklab-ottosson-2021-01-25-xyz-d65-v1",
            "polar-from-ottosson-2021-01-25-oklab-v1",
            "css-color-4-oklch-d65-from-modeled-iec61966-srgb8-solid-v1",
        ],
    );
}

#[test]
fn canonical_bytes_pin_schema_order_availability_and_keys() {
    assert_eq!(RELEASE_REGISTRY_SCHEMA_VERSION_V1, 1);
    assert_eq!(
        release_registry_canonical_bytes_v1(),
        concat!(
            "labcolors.release-registry.canonical-binary.v1\0",
            "\0\x01\0\x05",
            "\x01\x01\0\x26cam16-li-et-al-2017-cie-248-forward-v1",
            "\x01\x01\0\x24oklab-ottosson-2021-01-25-xyz-d65-v1",
            "\x01\x01\0\x27polar-from-ottosson-2021-01-25-oklab-v1",
            "\x02\0\0\0",
            "\x03\x01\0\x3acss-color-4-oklch-d65-from-modeled-iec61966-srgb8-solid-v1",
        )
        .as_bytes(),
    );
}

#[test]
fn canonical_bytes_encode_every_typed_registry_row_in_order() {
    const MAGIC: &[u8] = b"labcolors.release-registry.canonical-binary.v1\0";

    let bytes = release_registry_canonical_bytes_v1();
    assert_eq!(&bytes[..MAGIC.len()], MAGIC);
    let mut cursor = MAGIC.len();
    let take_u16 = |cursor: &mut usize| {
        let value = u16::from_be_bytes([bytes[*cursor], bytes[*cursor + 1]]);
        *cursor += 2;
        value
    };

    assert_eq!(take_u16(&mut cursor), RELEASE_REGISTRY_SCHEMA_VERSION_V1);
    let records = release_registry_records_v1();
    assert_eq!(usize::from(take_u16(&mut cursor)), records.len());

    for record in records {
        assert_eq!(bytes[cursor], record.class().canonical_tag());
        cursor += 1;
        assert_eq!(bytes[cursor], record.availability().canonical_tag());
        cursor += 1;

        let key_length = usize::from(take_u16(&mut cursor));
        let expected_key = record
            .release()
            .map(RegisteredColorReleaseIdV1::key)
            .unwrap_or("")
            .as_bytes();
        assert_eq!(key_length, expected_key.len());
        assert_eq!(&bytes[cursor..cursor + key_length], expected_key);
        cursor += key_length;
    }

    assert_eq!(cursor, bytes.len());
}

#[test]
fn digest_names_its_non_cryptographic_algorithm_and_binds_canonical_bytes() {
    let digest = release_registry_digest_v1();
    assert_eq!(
        digest.algorithm(),
        ReleaseRegistryDigestAlgorithmV1::Fnv1a32V1,
    );
    assert_eq!(digest.algorithm().key(), "fnv1a-32-v1");
    assert_eq!(
        digest.value(),
        crate::fnv1a_32(release_registry_canonical_bytes_v1()),
    );
    assert_eq!(digest.value(), 3_103_457_152);
}
