use proptest::prelude::*;

use crate::Srgb8;
use crate::lcs_occurrence::{
    ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1, AdaptingLuminanceCdM2, AppearanceContextFieldV1,
    AppearanceContextId, AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio, ColorSignal,
    ColorimetricFrameId, ColorimetricFrameReleaseId, ColorimetricTransformReleaseId, HueAngle,
    HueState, IEC_SRGB_D65_XYZ_FRAME_V1, LcsOccurrence, MUTATION_SENTINEL_XYZ_FRAME_V1,
    ModeledTristimulusDerivationV1, NumericDomainError, ObserverProfileId,
    OccurrenceFormationError, ReferenceWhiteId, SurroundProfileId, TristimulusComponentV1,
    TristimulusDomainErrorV1, TristimulusSample, TristimulusScale,
    derive_modeled_tristimulus_v1,
};
use crate::spaces::srgb::{D65_WHITE, srgb_linear_from_srgb8, srgb_to_xyz};

fn context(frame: ColorimetricFrameId, la: f64) -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        frame,
        AdaptingLuminanceCdM2::try_new(la).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        SurroundProfileId::AverageV1,
    )
}

#[test]
fn xyz_and_context_numeric_admission_is_fail_closed() {
    let relative = IEC_SRGB_D65_XYZ_FRAME_V1;
    for xyz in [
        [f64::NAN, 0.0, 0.0],
        [0.0, f64::INFINITY, 0.0],
        [0.0, 0.0, -f64::MIN_POSITIVE],
    ] {
        assert!(TristimulusSample::try_from_xyz_for_test(xyz, relative).is_err());
    }

    let zero_la = AdaptingLuminanceCdM2::try_new(0.0).unwrap_err();
    assert_eq!(zero_la.field(), AppearanceContextFieldV1::AdaptingLuminanceCdM2);
    assert_eq!(zero_la.reason(), NumericDomainError::NotPositive);

    let zero_background = BackgroundLuminanceRatio::try_new(0.0).unwrap_err();
    assert_eq!(
        zero_background.field(),
        AppearanceContextFieldV1::BackgroundLuminanceRatio,
    );
    assert_eq!(zero_background.reason(), NumericDomainError::NotPositive);

    let high_background = BackgroundLuminanceRatio::try_new(1.01).unwrap_err();
    assert_eq!(
        high_background.field(),
        AppearanceContextFieldV1::BackgroundLuminanceRatio,
    );
    assert_eq!(high_background.reason(), NumericDomainError::AboveOne);

    for invalid in [f64::NAN, f64::INFINITY, -f64::MIN_POSITIVE, -0.0] {
        assert_eq!(
            AdaptingLuminanceCdM2::try_new(invalid)
                .unwrap_err()
                .field(),
            AppearanceContextFieldV1::AdaptingLuminanceCdM2,
        );
        assert_eq!(
            BackgroundLuminanceRatio::try_new(invalid)
                .unwrap_err()
                .field(),
            AppearanceContextFieldV1::BackgroundLuminanceRatio,
        );
    }
}

#[test]
fn hue_algebra_cannot_encode_exact_absence_as_zero_degrees() {
    assert_ne!(
        HueState::UndefinedExact,
        HueState::Defined(HueAngle::new(0.0).unwrap())
    );
}

#[test]
fn frame_mismatch_cannot_form_an_occurrence() {
    let relative = IEC_SRGB_D65_XYZ_FRAME_V1;
    let mismatching = MUTATION_SENTINEL_XYZ_FRAME_V1;
    let sample = derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new([
        0x44, 0x88, 0xCC,
    ])))
    .unwrap()
    .sample();
    assert_eq!(sample.frame(), relative);
    assert_eq!(
        LcsOccurrence::in_context(sample, context(mismatching, 64.0)),
        Err(OccurrenceFormationError::FrameMismatch {
            stimulus: relative,
            context: mismatching,
        })
    );
}

#[test]
fn occurrence_identity_contains_only_sample_and_context() {
    let relative = IEC_SRGB_D65_XYZ_FRAME_V1;
    let sample =
        TristimulusSample::try_from_xyz_for_test([0.1, 0.2, 0.3], relative).unwrap();
    let occurrence = LcsOccurrence::in_context(sample, context(relative, 64.0)).unwrap();
    assert_eq!(occurrence.sample(), sample);
    assert_eq!(occurrence.context(), context(relative, 64.0));
}

#[test]
fn context_identity_changes_with_semantic_input_bits() {
    let relative = IEC_SRGB_D65_XYZ_FRAME_V1;
    assert_ne!(context(relative, 64.0), context(relative, 32.0));

    let changed_background = AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        relative,
        AdaptingLuminanceCdM2::try_new(64.0).unwrap(),
        BackgroundLuminanceRatio::try_new(0.3).unwrap(),
        SurroundProfileId::AverageV1,
    );
    let changed_surround = AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        relative,
        AdaptingLuminanceCdM2::try_new(64.0).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        SurroundProfileId::DimV1,
    );
    assert_ne!(context(relative, 64.0), changed_background);
    assert_ne!(context(relative, 64.0), changed_surround);
}

#[test]
fn hue_numeric_constructor_rejects_nonfinite_and_out_of_domain() {
    for invalid in [f64::NAN, f64::INFINITY, -0.01, 360.0] {
        assert!(HueAngle::new(invalid).is_err());
    }
    assert_eq!(HueAngle::new(-0.0).unwrap(), HueAngle::new(0.0).unwrap());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn admitted_srgb8_lowering_preserves_existing_decode_and_matrix_bits(
        bytes in any::<[u8; 3]>(),
    ) {
        let signal = ColorSignal::from_srgb8(Srgb8::new(bytes));
        let derived = derive_modeled_tristimulus_v1(signal).unwrap();
        let expected = srgb_to_xyz(srgb_linear_from_srgb8(Srgb8::new(bytes)));

        prop_assert_eq!(
            derived.sample().xyz().map(f64::to_bits),
            expected.map(f64::to_bits),
        );
        prop_assert_eq!(derived.sample().frame(), IEC_SRGB_D65_XYZ_FRAME_V1);
    }
}

#[test]
fn f0_lowering_signature_accepts_only_one_profiled_signal() {
    let lower: fn(
        ColorSignal,
    ) -> Result<ModeledTristimulusDerivationV1, TristimulusDomainErrorV1> =
        derive_modeled_tristimulus_v1;
    assert!(lower(ColorSignal::from_srgb8(Srgb8::new([0; 3]))).is_ok());
}

#[test]
fn fixed_corpus_including_eotf_boundary_matches_existing_kernel_bits() {
    for bytes in [
        [0x00, 0x00, 0x00],
        [0xFF, 0xFF, 0xFF],
        [0xFF, 0x00, 0x00],
        [0x00, 0xFF, 0x00],
        [0x00, 0x00, 0xFF],
        [0x0A, 0x0B, 0x80],
        [0x44, 0x88, 0xCC],
    ] {
        let actual = derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new(bytes)))
            .unwrap()
            .sample()
            .xyz();
        let expected = srgb_to_xyz(srgb_linear_from_srgb8(Srgb8::new(bytes)));
        assert_eq!(actual.map(f64::to_bits), expected.map(f64::to_bits));
    }
}

#[test]
fn transform_release_v1_pins_a_pre_f0_binary64_vector() {
    // Recorded once from the pre-F0 decode-table + matrix operation order. This
    // is a release anti-drift vector, not a claim of measured or bounded
    // colorimetric evidence.
    let xyz = derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new([
        0x0A, 0x0B, 0x80,
    ])))
    .unwrap()
    .sample()
    .xyz();
    assert_eq!(
        xyz.map(f64::to_bits),
        [
            0x3FA5_334E_5BB6_D38E,
            0x3F93_11B4_45B1_935D,
            0x3FCA_5268_973F_D7F0,
        ],
    );
}

#[test]
fn every_srgb8_channel_code_has_finite_nonnegative_basis_contribution() {
    // The registered EOTF acts independently on each byte and the XYZ matrix is
    // a sum of three non-negative basis contributions. Exhausting 3 × 256 basis
    // inputs therefore covers the finite/non-negative invariant for every mixed
    // triplet without adding a 256³ debug-test loop; the property test above
    // separately exercises mixed-sum routing and exact operation order.
    for byte in u8::MIN..=u8::MAX {
        for channel in 0..3 {
            let mut bytes = [0_u8; 3];
            bytes[channel] = byte;
            let xyz = derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new(bytes)))
                .unwrap()
                .sample()
                .xyz();
            assert!(xyz.into_iter().all(|component| component.is_finite() && component >= 0.0));
        }
    }
}

#[test]
fn admitted_binding_is_one_closed_profile_transform_frame_tuple() {
    let binding = ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1;
    assert_eq!(
        binding.signal_output_profile(),
        crate::lcs_occurrence::OutputProfileId::Iec61966Srgb8D65V1,
    );
    assert_eq!(
        binding.transform_release(),
        ColorimetricTransformReleaseId::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1,
    );
    assert_eq!(binding.result_frame(), IEC_SRGB_D65_XYZ_FRAME_V1);
    assert_eq!(
        binding.result_frame().observer(),
        ObserverProfileId::Cie1931TwoDegreeV1,
    );
    assert_eq!(
        binding.result_frame().reference_white(),
        ReferenceWhiteId::Iec61966D65ChromaticityV1,
    );
    assert_eq!(
        binding.result_frame().scale(),
        TristimulusScale::RelativeY1,
    );
    assert_eq!(
        binding.result_frame().release(),
        ColorimetricFrameReleaseId::XyzV1,
    );
}

#[test]
fn black_is_positive_zero_and_white_tracks_the_existing_matrix() {
    let black = derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new([0; 3])))
        .unwrap()
        .sample()
        .xyz();
    assert_eq!(black.map(f64::to_bits), [0_u64; 3]);

    let white = derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new([255; 3])))
        .unwrap()
        .sample()
        .xyz();
    let matrix_white = srgb_to_xyz([1.0; 3]);
    assert_eq!(white.map(f64::to_bits), matrix_white.map(f64::to_bits));
    for (actual, reference) in white.into_iter().zip(D65_WHITE) {
        assert!((actual - reference).abs() <= f64::EPSILON);
    }
}

#[test]
fn modeled_derivation_is_content_bound_replayable_and_allocation_free() {
    let signal = ColorSignal::from_srgb8(Srgb8::new([0x0A, 0x0B, 0x80]));
    let (derived, lowering_allocations) =
        crate::test_support::measured_allocations(|| derive_modeled_tristimulus_v1(signal));
    let derived = derived.unwrap();
    let (replayed, replay_allocations) =
        crate::test_support::measured_allocations(|| derived.replay());

    assert_eq!(lowering_allocations, 0);
    assert_eq!(replay_allocations, 0);
    assert_eq!(replayed.unwrap(), derived.sample());
    assert_eq!(derived.provenance().source_signal(), signal);
    assert_eq!(
        derived.provenance().binding(),
        ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1,
    );

    let changed_signal = ColorSignal::from_srgb8(Srgb8::new([0x0B, 0x0B, 0x80]));
    let changed = derive_modeled_tristimulus_v1(changed_signal).unwrap();
    assert_ne!(derived.provenance(), changed.provenance());
    assert_ne!(derived.sample(), changed.sample());
    assert_eq!(derived.provenance().source_signal(), signal);
}

#[test]
fn raw_xyz_admission_is_test_only_component_qualified_and_fail_closed() {
    let frame = IEC_SRGB_D65_XYZ_FRAME_V1;
    for (xyz, component, reason) in [
        (
            [f64::NAN, 0.0, 0.0],
            TristimulusComponentV1::X,
            NumericDomainError::NonFinite,
        ),
        (
            [0.0, f64::INFINITY, 0.0],
            TristimulusComponentV1::Y,
            NumericDomainError::NonFinite,
        ),
        (
            [0.0, 0.0, -f64::MIN_POSITIVE],
            TristimulusComponentV1::Z,
            NumericDomainError::Negative,
        ),
    ] {
        let error = TristimulusSample::try_from_xyz_for_test(xyz, frame).unwrap_err();
        assert_eq!(error.component(), component);
        assert_eq!(error.reason(), reason);
    }

    let admitted = TristimulusSample::try_from_xyz_for_test([-0.0, 0.5, 1.25], frame).unwrap();
    assert_eq!(admitted.xyz()[0].to_bits(), 0.0_f64.to_bits());
    assert_eq!(admitted.xyz()[2], 1.25);
}

#[test]
fn appearance_context_identity_exposes_only_semantic_inputs() {
    let context = context(IEC_SRGB_D65_XYZ_FRAME_V1, 64.0);
    assert_eq!(
        context.schema_release(),
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
    );
    assert_eq!(context.frame(), IEC_SRGB_D65_XYZ_FRAME_V1);
    assert_eq!(context.adapting_luminance_cd_m2(), 64.0);
    assert_eq!(context.background_luminance_ratio(), 0.2);
    assert_eq!(context.surround_profile(), SurroundProfileId::AverageV1);
}
