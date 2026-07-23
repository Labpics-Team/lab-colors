use proptest::prelude::*;

use crate::Srgb8;
use crate::lcs_occurrence::{
    ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1, AdaptingLuminanceCdM2, AppearanceContextFieldV1,
    AppearanceContextId, AppearanceContextSchemaReleaseId, AppearanceState,
    AppearanceStateDerivationErrorV1, AppearanceViewFieldV1, AppearanceViewReleaseIdV1,
    BackgroundLuminanceRatio, CAM16_UCS_VIEW_RELEASE_V1, CAM16_VIEW_RELEASE_V1, ColorSignal,
    ColorimetricFrameId, ColorimetricFrameReleaseId, ColorimetricTransformReleaseId, HueAngle,
    HueState, IEC_SRGB_D65_XYZ_FRAME_V1, LcsOccurrence, MUTATION_SENTINEL_XYZ_FRAME_V1,
    ModeledTristimulusDerivationV1, NumericDomainError, OKLAB_VIEW_RELEASE_V1, ObserverProfileId,
    OccurrenceFormationError, ReferenceWhiteId, SurroundProfileId, TristimulusComponentV1,
    TristimulusDomainErrorV1, TristimulusSample, TristimulusScale, derive_modeled_tristimulus_v1,
};
use crate::spaces::cam16::{ForwardCacheGuard, forward, forward_correlates_v1, ucs_j, ucs_m};
use crate::spaces::oklab::{srgb_linear_to_oklab, xyz_d65_to_oklab_v1};
use crate::spaces::srgb::{D65_WHITE, srgb_linear_from_srgb8, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;

fn context(frame: ColorimetricFrameId, la: f64) -> AppearanceContextId {
    context_with_surround(frame, la, SurroundProfileId::AverageV1)
}

fn context_with_surround(
    frame: ColorimetricFrameId,
    la: f64,
    surround: SurroundProfileId,
) -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        frame,
        AdaptingLuminanceCdM2::try_new(la).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        surround,
    )
}

fn occurrence_from_srgb8(bytes: [u8; 3], context: AppearanceContextId) -> LcsOccurrence {
    let sample = derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new(bytes)))
        .unwrap()
        .sample();
    LcsOccurrence::in_context(sample, context).unwrap()
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
    assert_eq!(
        zero_la.field(),
        AppearanceContextFieldV1::AdaptingLuminanceCdM2
    );
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
            AdaptingLuminanceCdM2::try_new(invalid).unwrap_err().field(),
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
    let sample =
        derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new([0x44, 0x88, 0xCC])))
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
    let sample = TristimulusSample::try_from_xyz_for_test([0.1, 0.2, 0.3], relative).unwrap();
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

    #[test]
    fn direct_xyz_oklab_projection_tracks_the_existing_srgb_kernel_without_routing_through_it(
        bytes in any::<[u8; 3]>(),
    ) {
        let linear = srgb_linear_from_srgb8(Srgb8::new(bytes));
        let legacy_srgb_projection = srgb_linear_to_oklab(linear);
        let direct_xyz_projection = xyz_d65_to_oklab_v1(srgb_to_xyz(linear));

        for component in 0..3 {
            prop_assert!(
                (direct_xyz_projection[component] - legacy_srgb_projection[component]).abs()
                    <= 2.0e-8,
                "component {component}: direct={} existing={}",
                direct_xyz_projection[component],
                legacy_srgb_projection[component],
            );
        }
    }
}

#[test]
fn f0_lowering_signature_accepts_only_one_profiled_signal() {
    let lower: fn(ColorSignal) -> Result<ModeledTristimulusDerivationV1, TristimulusDomainErrorV1> =
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
    let xyz =
        derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new([0x0A, 0x0B, 0x80])))
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
            assert!(
                xyz.into_iter()
                    .all(|component| component.is_finite() && component >= 0.0)
            );
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
    assert_eq!(binding.result_frame().scale(), TristimulusScale::RelativeY1,);
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

#[test]
fn appearance_state_is_one_way_views_of_the_same_occurrence_with_separate_releases() {
    let occurrence =
        occurrence_from_srgb8([0x00, 0x00, 0xFF], context(IEC_SRGB_D65_XYZ_FRAME_V1, 64.0));
    let state = AppearanceState::derive_v1(occurrence).unwrap();

    assert_eq!(state.occurrence(), occurrence);
    assert_eq!(state.oklab().release(), OKLAB_VIEW_RELEASE_V1);
    assert_eq!(state.cam16().release(), CAM16_VIEW_RELEASE_V1);
    assert_eq!(
        state.cam16_ucs().unwrap().release(),
        CAM16_UCS_VIEW_RELEASE_V1,
    );
    assert_eq!(
        state.occurrence().sample().frame().observer(),
        ObserverProfileId::Cie1931TwoDegreeV1,
    );
    assert_eq!(state.occurrence().context(), occurrence.context());

    let direct = xyz_d65_to_oklab_v1(occurrence.sample().xyz());
    assert_eq!(
        [state.oklab().l(), state.oklab().a(), state.oklab().b()].map(f64::to_bits),
        direct.map(f64::to_bits),
    );

    let expected_cam = forward_correlates_v1(occurrence.sample().xyz(), &ViewingConditions::srgb());
    let cam = state.cam16();
    assert_eq!(cam.j().to_bits(), expected_cam.j.to_bits());
    assert_eq!(cam.q().to_bits(), expected_cam.q.to_bits());
    assert_eq!(cam.c().to_bits(), expected_cam.c.to_bits());
    assert_eq!(cam.m().to_bits(), expected_cam.m.to_bits());
    assert_eq!(cam.s().to_bits(), expected_cam.s.to_bits());
    let HueState::Defined(cam_hue) = cam.hue() else {
        panic!("chromatic blue must have a CAM16 hue");
    };
    assert_eq!(cam_hue.degrees().to_bits(), expected_cam.h.to_bits());

    let oklab_hue = state.oklab().b().atan2(state.oklab().a()).to_degrees();
    let oklab_hue = if oklab_hue < 0.0 {
        oklab_hue + 360.0
    } else {
        oklab_hue
    };
    assert!(
        (cam_hue.degrees() - oklab_hue).abs() > 10.0,
        "CAM16 hue must not be copied from Oklab: CAM16={} Oklab={oklab_hue}",
        cam_hue.degrees(),
    );
}

#[test]
fn context_changes_only_the_contextual_cam16_view() {
    let sample =
        derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new([0x44, 0x88, 0xCC])))
            .unwrap()
            .sample();
    let average = AppearanceState::derive_v1(
        LcsOccurrence::in_context(
            sample,
            context_with_surround(
                IEC_SRGB_D65_XYZ_FRAME_V1,
                64.0,
                SurroundProfileId::AverageV1,
            ),
        )
        .unwrap(),
    )
    .unwrap();
    let dim = AppearanceState::derive_v1(
        LcsOccurrence::in_context(
            sample,
            context_with_surround(IEC_SRGB_D65_XYZ_FRAME_V1, 64.0, SurroundProfileId::DimV1),
        )
        .unwrap(),
    )
    .unwrap();

    assert_eq!(average.oklab(), dim.oklab());
    assert_ne!(average.cam16().j().to_bits(), dim.cam16().j().to_bits());
    assert_ne!(average.cam16().m().to_bits(), dim.cam16().m().to_bits());
    assert_ne!(
        average.cam16_ucs().unwrap().j_prime().to_bits(),
        dim.cam16_ucs().unwrap().j_prime().to_bits(),
    );
    assert_ne!(average.cam16_ucs().unwrap(), dim.cam16_ucs().unwrap());
    assert_ne!(average.occurrence(), dim.occurrence());
}

#[test]
fn every_registered_surround_maps_to_its_exact_cam16_kernel_tuple() {
    for (surround, vc) in [
        (SurroundProfileId::AverageV1, ViewingConditions::srgb()),
        (SurroundProfileId::DimV1, ViewingConditions::dim_surround()),
        (
            SurroundProfileId::DarkV1,
            ViewingConditions::dark_surround(),
        ),
    ] {
        let occurrence = occurrence_from_srgb8(
            [0x44, 0x88, 0xCC],
            context_with_surround(IEC_SRGB_D65_XYZ_FRAME_V1, 64.0, surround),
        );
        let actual = AppearanceState::derive_v1(occurrence).unwrap().cam16();
        let expected = forward_correlates_v1(occurrence.sample().xyz(), &vc);
        assert_eq!(
            [actual.j(), actual.q(), actual.c(), actual.m(), actual.s(),].map(f64::to_bits),
            [expected.j, expected.q, expected.c, expected.m, expected.s,].map(f64::to_bits),
            "surround {surround:?} must not mix registered tuple fields",
        );
        let HueState::Defined(actual_hue) = actual.hue() else {
            panic!("chromatic fixture must have hue under {surround:?}");
        };
        assert_eq!(actual_hue.degrees().to_bits(), expected.h.to_bits());

        let actual_ucs = AppearanceState::derive_v1(occurrence)
            .unwrap()
            .cam16_ucs()
            .unwrap();
        let expected_j_prime = ucs_j(expected.j);
        let expected_m_prime = ucs_m(expected.m);
        let expected_radians = expected.h.to_radians();
        assert_eq!(
            [
                actual_ucs.j_prime(),
                actual_ucs.a_prime(),
                actual_ucs.b_prime(),
            ]
            .map(f64::to_bits),
            [
                expected_j_prime,
                expected_m_prime * expected_radians.cos(),
                expected_m_prime * expected_radians.sin(),
            ]
            .map(f64::to_bits),
            "CAM16-UCS must be assembled from the same CAM16 view under {surround:?}",
        );
    }
}

#[test]
fn exact_zero_coordinate_has_no_invented_hue() {
    let state = AppearanceState::derive_v1(occurrence_from_srgb8(
        [0; 3],
        context(IEC_SRGB_D65_XYZ_FRAME_V1, 64.0),
    ))
    .unwrap();

    assert_eq!(
        [state.oklab().l(), state.oklab().a(), state.oklab().b()].map(f64::to_bits),
        [0; 3],
    );
    assert_eq!(state.cam16().j().to_bits(), 0);
    assert_eq!(state.cam16().q().to_bits(), 0);
    assert_eq!(state.cam16().c().to_bits(), 0);
    assert_eq!(state.cam16().m().to_bits(), 0);
    assert_eq!(state.cam16().s().to_bits(), 0);
    assert_eq!(state.cam16().hue(), HueState::UndefinedExact);
    let ucs = state.cam16_ucs().unwrap();
    assert_eq!(
        [ucs.j_prime(), ucs.a_prime(), ucs.b_prime()].map(f64::to_bits),
        [0; 3],
    );
}

#[test]
fn state_derivation_rejects_an_unregistered_frame_before_any_view_math() {
    let frame = MUTATION_SENTINEL_XYZ_FRAME_V1;
    let sample = TristimulusSample::try_from_xyz_for_test([0.1, 0.2, 0.3], frame).unwrap();
    let occurrence = LcsOccurrence::in_context(sample, context(frame, 64.0)).unwrap();

    assert_eq!(
        AppearanceState::derive_v1(occurrence).unwrap_err(),
        AppearanceStateDerivationErrorV1::UnsupportedFrame { frame },
    );
}

#[test]
fn state_derivation_rejects_nonfinite_derived_coordinates_with_release_and_field() {
    let frame = IEC_SRGB_D65_XYZ_FRAME_V1;
    let sample = TristimulusSample::try_from_xyz_for_test([f64::MAX; 3], frame).unwrap();
    let occurrence = LcsOccurrence::in_context(sample, context(frame, 64.0)).unwrap();

    assert_eq!(
        AppearanceState::derive_v1(occurrence).unwrap_err(),
        AppearanceStateDerivationErrorV1::NumericDomain {
            release: AppearanceViewReleaseIdV1::Oklab(OKLAB_VIEW_RELEASE_V1),
            field: AppearanceViewFieldV1::OklabL,
            reason: NumericDomainError::NonFinite,
        },
    );
}

#[test]
fn direct_oklab_release_pins_an_external_xyz_projection_vector() {
    // Fixed vector from the CSS Color 4 direct XYZ(D65) -> Oklab matrices for
    // the already-pinned `[0A, 0B, 80]` F0 tristimulus. Tolerance covers only
    // cross-libm cbrt ULPs; it is far below the direct-vs-legacy matrix delta.
    let state = AppearanceState::derive_v1(occurrence_from_srgb8(
        [0x0A, 0x0B, 0x80],
        context(IEC_SRGB_D65_XYZ_FRAME_V1, 64.0),
    ))
    .unwrap();
    let actual = [state.oklab().l(), state.oklab().a(), state.oklab().b()];
    let expected = [
        0.284_226_036_666_170_47,
        -0.009_591_562_466_562_426,
        -0.178_614_436_252_897_94,
    ];
    for component in 0..3 {
        assert!(
            (actual[component] - expected[component]).abs() <= 1.0e-14,
            "Oklab component {component} drifted: actual={} expected={}",
            actual[component],
            expected[component],
        );
    }
}

#[test]
fn cam16_release_pins_a_full_external_correlate_vector() {
    // colour-science CIECAM16, D65, L_A=64 cd/m², Y_b/Y_w=0.2, average
    // surround. Unlike the older J/M/h table this pins the newly exposed
    // Q/C/s correlates as well. The tolerances cover only the documented CSS
    // XYZ constant delta from colour-science's matrix derivation.
    let state = AppearanceState::derive_v1(occurrence_from_srgb8(
        [0x00, 0x00, 0xFF],
        context(IEC_SRGB_D65_XYZ_FRAME_V1, 64.0),
    ))
    .unwrap();
    let cam = state.cam16();
    let HueState::Defined(hue) = cam.hue() else {
        panic!("reference blue must retain a CAM16 hue");
    };

    for (name, actual, expected, tolerance) in [
        ("J", cam.j(), 25.271_208_691_856_113, 0.01),
        ("Q", cam.q(), 109.108_232_192_767_33, 0.1),
        ("C", cam.c(), 86.580_098_936_732_16, 0.1),
        ("M", cam.m(), 78.737_310_637_269_06, 0.05),
        ("s", cam.s(), 84.949_637_273_879, 0.1),
        ("h", hue.degrees(), 282.871_080_928_130_14, 0.15),
    ] {
        assert!(
            (actual - expected).abs() < tolerance,
            "CAM16 {name} drifted: actual={actual} expected={expected}",
        );
    }

    // The same independent reference vector projected through Li et al. 2017
    // CAM16-UCS. These are coordinates of this occurrence, not a distance claim.
    let ucs = state.cam16_ucs().unwrap();
    for (name, actual, expected, tolerance) in [
        ("J'", ucs.j_prime(), 36.503_620_495_334_07, 0.02),
        ("a'", ucs.a_prime(), 10.042_750_480_031_993, 0.1),
        ("b'", ucs.b_prime(), -43.950_878_225_240_46, 0.1),
    ] {
        assert!(
            (actual - expected).abs() < tolerance,
            "CAM16-UCS {name} drifted: actual={actual} expected={expected}",
        );
    }
}

#[test]
fn full_appearance_state_derivation_is_allocation_free() {
    let occurrence =
        occurrence_from_srgb8([0x44, 0x88, 0xCC], context(IEC_SRGB_D65_XYZ_FRAME_V1, 64.0));
    let (derived, allocations) =
        crate::test_support::measured_allocations(|| AppearanceState::derive_v1(occurrence));

    assert_eq!(allocations, 0);
    let derived = derived.unwrap();
    let (ucs, ucs_allocations) = crate::test_support::measured_allocations(|| derived.cam16_ucs());
    assert_eq!(ucs_allocations, 0);
    assert!(ucs.is_ok());
}

#[test]
fn appearance_state_bypasses_active_xyz_only_cache_for_each_context_without_allocating() {
    let frame = IEC_SRGB_D65_XYZ_FRAME_V1;
    let sample =
        derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new([0x44, 0x88, 0xCC])))
            .unwrap()
            .sample();
    let xyz = sample.xyz();

    // Freeze the per-context cache-free answers before activating the legacy
    // XYZ-only cache. These are independent of its ambient guard state.
    let expected_dim = forward_correlates_v1(xyz, &ViewingConditions::dim_surround());
    let expected_dark = forward_correlates_v1(xyz, &ViewingConditions::dark_surround());

    let dim_occurrence = LcsOccurrence::in_context(
        sample,
        context_with_surround(frame, 64.0, SurroundProfileId::DimV1),
    )
    .unwrap();
    let dark_occurrence = LcsOccurrence::in_context(
        sample,
        context_with_surround(frame, 64.0, SurroundProfileId::DarkV1),
    )
    .unwrap();

    let _guard = ForwardCacheGuard::activate();
    let cached_average = forward(xyz, &ViewingConditions::srgb());
    assert_ne!(cached_average.0.to_bits(), expected_dim.j.to_bits());
    assert_ne!(cached_average.0.to_bits(), expected_dark.j.to_bits());

    let (dim, dim_allocations) =
        crate::test_support::measured_allocations(|| AppearanceState::derive_v1(dim_occurrence));
    let (dark, dark_allocations) =
        crate::test_support::measured_allocations(|| AppearanceState::derive_v1(dark_occurrence));
    let dim = dim.unwrap().cam16();
    let dark = dark.unwrap().cam16();

    assert_eq!(dim_allocations, 0);
    assert_eq!(dark_allocations, 0);
    assert_eq!(
        [dim.j(), dim.q(), dim.c(), dim.m(), dim.s()].map(f64::to_bits),
        [
            expected_dim.j,
            expected_dim.q,
            expected_dim.c,
            expected_dim.m,
            expected_dim.s,
        ]
        .map(f64::to_bits),
    );
    assert_eq!(
        [dark.j(), dark.q(), dark.c(), dark.m(), dark.s()].map(f64::to_bits),
        [
            expected_dark.j,
            expected_dark.q,
            expected_dark.c,
            expected_dark.m,
            expected_dark.s,
        ]
        .map(f64::to_bits),
    );

    let HueState::Defined(dim_hue) = dim.hue() else {
        panic!("chromatic dim fixture must have CAM16 hue");
    };
    let HueState::Defined(dark_hue) = dark.hue() else {
        panic!("chromatic dark fixture must have CAM16 hue");
    };
    assert_eq!(dim_hue.degrees().to_bits(), expected_dim.h.to_bits());
    assert_eq!(dark_hue.degrees().to_bits(), expected_dark.h.to_bits());

    let expected_ucs = |expected: crate::spaces::cam16::Cam16CorrelatesV1| {
        let m_prime = ucs_m(expected.m);
        let radians = expected.h.to_radians();
        [
            ucs_j(expected.j),
            m_prime * radians.cos(),
            m_prime * radians.sin(),
        ]
        .map(f64::to_bits)
    };
    let actual_ucs = |view: crate::lcs_occurrence::Cam16UcsViewV1| {
        [view.j_prime(), view.a_prime(), view.b_prime()].map(f64::to_bits)
    };
    assert_eq!(
        actual_ucs(
            AppearanceState::derive_v1(dim_occurrence)
                .unwrap()
                .cam16_ucs()
                .unwrap(),
        ),
        expected_ucs(expected_dim),
    );
    assert_eq!(
        actual_ucs(
            AppearanceState::derive_v1(dark_occurrence)
                .unwrap()
                .cam16_ucs()
                .unwrap(),
        ),
        expected_ucs(expected_dark),
    );
}

#[test]
fn occurrence_views_have_no_ambient_preset_or_client_semantic_route() {
    let source = include_str!("lcs_occurrence.rs");
    for forbidden in [
        "ViewingConditions::srgb()",
        "ViewingConditions::dim_surround()",
        "ForwardCacheGuard",
        "PairFill",
        "Glow",
        "LabUI",
        "Compatibility",
        "Legacy",
    ] {
        assert!(
            !source.contains(forbidden),
            "occurrence/view foundation must not contain `{forbidden}`",
        );
    }
    for required in [
        "ViewingConditions::from_semantic_inputs_v1",
        "forward_correlates_v1(occurrence.sample().xyz(), &vc)",
        "derive_cam16_ucs_view_v1(self.cam16)",
    ] {
        assert!(
            source.contains(required),
            "explicit derived-view route must retain `{required}`",
        );
    }
}
