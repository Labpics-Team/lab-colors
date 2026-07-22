use crate::lcs_occurrence::{
    ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1, AppearanceContextSchemaReleaseId, CAM16_VIEW_RELEASE_V1,
    IEC_SRGB_D65_XYZ_FRAME_V1, OKLAB_VIEW_RELEASE_V1,
};
use crate::output_projection::{
    CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1, CssOklchHueSerializationReleaseIdV1,
    CssOklchNumberEncodingReleaseIdV1, DifferenceCalibrationReleaseIdV1, OKLCH_VIEW_RELEASE_V1,
    OutputGamutTreatmentV1,
};
use crate::release_registry::{
    RELEASE_REGISTRY_SCHEMA_VERSION_V1, RegisteredColorReleaseIdV1, RegisteredReleaseDescriptorV1,
    RegistryAchromaticLawV1, RegistryAdmittedDomainV1, RegistryAdmittedFrameV1,
    RegistryCoordinateUnitsV1, RegistryReferenceIdentityV1, ReleaseContextRequirementV1,
    ReleaseDependencyGraphV1, ReleaseRegistryAvailabilityV1, ReleaseRegistryClassV1,
    ReleaseRegistryDigestAlgorithmV1, ReleaseRegistryRecordV1,
    impossible_difference_calibration_release_v1, release_registry_canonical_bytes_v1,
    release_registry_digest_v1, release_registry_records_v1,
};

fn descriptor(release: RegisteredColorReleaseIdV1) -> RegisteredReleaseDescriptorV1 {
    release_registry_records_v1()
        .iter()
        .filter_map(|record| record.descriptor())
        .find(|descriptor| descriptor.release() == release)
        .expect("registered release descriptor")
}

#[test]
fn registry_contains_only_the_four_implemented_releases() {
    let releases: Vec<_> = release_registry_records_v1()
        .iter()
        .filter_map(|record| record.release())
        .collect();
    assert_eq!(
        releases,
        [
            RegisteredColorReleaseIdV1::Cam16View(CAM16_VIEW_RELEASE_V1),
            RegisteredColorReleaseIdV1::OklabView(OKLAB_VIEW_RELEASE_V1),
            RegisteredColorReleaseIdV1::OklchView(OKLCH_VIEW_RELEASE_V1),
            RegisteredColorReleaseIdV1::CssColor4OklchD65FromModeledSrgb8Solid(
                CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1,
            ),
        ],
    );
}

#[test]
fn difference_registry_is_explicitly_unavailable_and_has_no_fake_descriptor() {
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
    assert_eq!(row.descriptor(), None);

    let _: fn(DifferenceCalibrationReleaseIdV1) -> ! = impossible_difference_calibration_release_v1;
}

#[test]
fn appearance_descriptors_pin_only_code_owned_domain_units_hue_and_reference_facts() {
    let cam16 = descriptor(RegisteredColorReleaseIdV1::Cam16View(CAM16_VIEW_RELEASE_V1));
    assert_eq!(
        cam16.context_requirement(),
        ReleaseContextRequirementV1::ConsumesAppearanceContextV1(
            AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        ),
    );
    assert_eq!(
        cam16.context_requirement().schema_release(),
        Some(AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1),
    );
    assert_eq!(
        cam16.admitted_frame(),
        RegistryAdmittedFrameV1::Cie1931TwoDegreeXyzIecD65RelativeY1V1,
    );
    assert_eq!(cam16.admitted_frame().frame(), IEC_SRGB_D65_XYZ_FRAME_V1);
    assert_eq!(
        cam16.admitted_domain(),
        RegistryAdmittedDomainV1::FiniteNonNegativeXyzStimulusV1,
    );
    assert_eq!(
        cam16.coordinate_units(),
        RegistryCoordinateUnitsV1::Cam16CorrelatesUnitlessHueDegreesV1,
    );
    assert_eq!(
        cam16.achromatic_law(),
        RegistryAchromaticLawV1::HueUndefinedExactlyWhenCam16MIsZeroV1,
    );
    assert_eq!(
        cam16.reference_identity(),
        RegistryReferenceIdentityV1::LiEtAl2017Cie248Cam16ForwardV1,
    );
    assert_eq!(cam16.dependencies(), ReleaseDependencyGraphV1::DirectV1);

    let oklab = descriptor(RegisteredColorReleaseIdV1::OklabView(OKLAB_VIEW_RELEASE_V1));
    assert_eq!(
        oklab.context_requirement(),
        ReleaseContextRequirementV1::NoAppearanceContextConsumptionV1,
    );
    assert_eq!(oklab.context_requirement().schema_release(), None);
    assert_eq!(oklab.admitted_frame().frame(), IEC_SRGB_D65_XYZ_FRAME_V1);
    assert_eq!(
        oklab.admitted_domain(),
        RegistryAdmittedDomainV1::FiniteNonNegativeXyzStimulusV1,
    );
    assert_eq!(
        oklab.coordinate_units(),
        RegistryCoordinateUnitsV1::OklabCoordinatesUnitlessV1,
    );
    assert_eq!(
        oklab.achromatic_law(),
        RegistryAchromaticLawV1::NoHueCoordinateV1,
    );
    assert_eq!(
        oklab.reference_identity(),
        RegistryReferenceIdentityV1::Ottosson20210125OklabXyzD65V1,
    );
    assert_eq!(oklab.dependencies(), ReleaseDependencyGraphV1::DirectV1);

    let oklch = descriptor(RegisteredColorReleaseIdV1::OklchView(OKLCH_VIEW_RELEASE_V1));
    assert_eq!(
        oklch.context_requirement(),
        ReleaseContextRequirementV1::NoAppearanceContextConsumptionV1,
    );
    assert_eq!(oklch.admitted_frame().frame(), IEC_SRGB_D65_XYZ_FRAME_V1);
    assert_eq!(
        oklch.admitted_domain(),
        RegistryAdmittedDomainV1::FiniteOklabRectangularViewV1,
    );
    assert_eq!(
        oklch.coordinate_units(),
        RegistryCoordinateUnitsV1::OklchCoordinatesUnitlessHueDegreesV1,
    );
    assert_eq!(
        oklch.achromatic_law(),
        RegistryAchromaticLawV1::HueUndefinedExactlyWhenOklabAAndBAreZeroV1,
    );
    assert_eq!(
        oklch.reference_identity(),
        RegistryReferenceIdentityV1::Ottosson20210125OklabPolarV1,
    );
    assert_eq!(
        oklch.dependencies(),
        ReleaseDependencyGraphV1::OklchPolarFromOklabV1(OKLAB_VIEW_RELEASE_V1),
    );
}

#[test]
fn output_descriptor_binds_the_complete_typed_projection_dependency_chain() {
    let output = descriptor(
        RegisteredColorReleaseIdV1::CssColor4OklchD65FromModeledSrgb8Solid(
            CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1,
        ),
    );
    assert_eq!(
        output.context_requirement(),
        ReleaseContextRequirementV1::RetainsOccurrenceContextWithoutGeometryConsumptionV1,
    );
    assert_eq!(output.context_requirement().schema_release(), None);
    assert_eq!(output.admitted_frame().frame(), IEC_SRGB_D65_XYZ_FRAME_V1);
    assert_eq!(
        output.admitted_domain(),
        RegistryAdmittedDomainV1::ModeledIec61966Srgb8OccurrenceV1,
    );
    assert_eq!(
        output.coordinate_units(),
        RegistryCoordinateUnitsV1::CssColor4OklchPercentLightnessNumericChromaHueDegreesV1,
    );
    assert_eq!(
        output.achromatic_law(),
        RegistryAchromaticLawV1::ExactSourceGreyOrRectangularOriginSerializesHueZeroV1,
    );
    assert_eq!(
        output.reference_identity(),
        RegistryReferenceIdentityV1::CssColor4OklchD65V1,
    );

    let ReleaseDependencyGraphV1::CssColor4OklchD65V1(dependencies) = output.dependencies() else {
        panic!("output descriptor must carry its typed dependency graph");
    };
    assert_eq!(
        dependencies.modeled_source_binding(),
        ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1,
    );
    assert_eq!(dependencies.oklab_view(), OKLAB_VIEW_RELEASE_V1);
    assert_eq!(dependencies.oklch_view(), OKLCH_VIEW_RELEASE_V1);
    assert_eq!(
        dependencies.number_encoding(),
        CssOklchNumberEncodingReleaseIdV1::LPercent5C6Hue3V1,
    );
    assert_eq!(
        dependencies.hue_serialization(),
        CssOklchHueSerializationReleaseIdV1::ExactSourceGreyOrRectangularOriginToZeroV1,
    );
    assert_eq!(
        dependencies.gamut_treatment(),
        OutputGamutTreatmentV1::NoExplicitProjectionGamutMapV1,
    );
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
fn canonical_bytes_pin_schema_rows_descriptors_dependencies_and_order() {
    assert_eq!(RELEASE_REGISTRY_SCHEMA_VERSION_V1, 1);
    assert_eq!(
        release_registry_canonical_bytes_v1(),
        concat!(
            "labcolors.release-registry.canonical-binary.v1\0",
            "\0\x01\0\x05",
            "\x01\x01\0\x26cam16-li-et-al-2017-cie-248-forward-v1",
            "\x01\x02\x01\x01\x01\x02\x02\x02\0\0\0\0\0\0\0",
            "\x01\x01\0\x24oklab-ottosson-2021-01-25-xyz-d65-v1",
            "\x01\x01\0\x01\x01\x01\x01\x01\0\0\0\0\0\0\0",
            "\x01\x01\0\x27polar-from-ottosson-2021-01-25-oklab-v1",
            "\x01\x01\0\x01\x02\x03\x03\x03\x01\0\x01\0\0\0\0",
            "\x02\0\0\0",
            "\x03\x01\0\x3acss-color-4-oklch-d65-from-modeled-iec61966-srgb8-solid-v1",
            "\x01\x03\0\x01\x03\x04\x04\x04\x02\x01\x01\x01\x01\x01\x01",
        )
        .as_bytes(),
    );
}

#[test]
fn canonical_bytes_encode_every_typed_registry_row_and_descriptor_in_order() {
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

        if let Some(descriptor) = record.descriptor() {
            let fields = descriptor.canonical_fields();
            assert_eq!(&bytes[cursor..cursor + fields.len()], fields);
            cursor += fields.len();
        }
    }

    assert_eq!(cursor, bytes.len());
}

#[test]
fn digest_names_its_non_cryptographic_algorithm_and_covers_descriptor_bytes() {
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
    assert_eq!(digest.value(), 1_293_630_307);
}
