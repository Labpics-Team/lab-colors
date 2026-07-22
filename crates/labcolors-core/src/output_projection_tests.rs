use std::any::TypeId;

use crate::Srgb8;
use crate::lcs_occurrence::{
    AdaptingLuminanceCdM2, AppearanceContextId, AppearanceContextSchemaReleaseId,
    BackgroundLuminanceRatio, ColorSignal, IEC_SRGB_D65_XYZ_FRAME_V1, LcsOccurrence,
    ModeledTristimulusDerivationV1, OKLAB_VIEW_RELEASE_V1, SurroundProfileId,
    derive_modeled_tristimulus_v1,
};
use crate::output_projection::{
    CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1, CssOklchHueSerializationReleaseIdV1,
    CssOklchNumberEncodingReleaseIdV1, DifferenceCalibrationReleaseIdV1, OKLCH_VIEW_RELEASE_V1,
    OutputGamutTreatmentV1, OutputProjectionErrorV1, OutputProjectionReleaseIdV1,
    OutputProjectionRequestV1, OutputProjectionV1, ProjectionSourceFormationErrorV1,
    ProjectionSourceV1, project_output_v1,
};
use crate::spaces::oklab::oklab_to_srgb_linear;
use crate::spaces::srgb::srgb8_from_linear;

fn context(adapting_luminance_cd_m2: f64) -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        IEC_SRGB_D65_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(adapting_luminance_cd_m2).unwrap(),
        BackgroundLuminanceRatio::try_new(0.2).unwrap(),
        SurroundProfileId::AverageV1,
    )
}

fn modeled(bytes: [u8; 3]) -> ModeledTristimulusDerivationV1 {
    derive_modeled_tristimulus_v1(ColorSignal::from_srgb8(Srgb8::new(bytes))).unwrap()
}

fn source(bytes: [u8; 3], adapting_luminance_cd_m2: f64) -> ProjectionSourceV1 {
    let modeled = modeled(bytes);
    let occurrence =
        LcsOccurrence::in_context(modeled.sample(), context(adapting_luminance_cd_m2)).unwrap();
    ProjectionSourceV1::bind(occurrence, modeled).unwrap()
}

fn project(bytes: [u8; 3], adapting_luminance_cd_m2: f64) -> OutputProjectionV1 {
    project_output_v1(OutputProjectionRequestV1::new(
        source(bytes, adapting_luminance_cd_m2),
        CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1,
    ))
    .unwrap()
}

fn difference_release_is_uninhabited(value: DifferenceCalibrationReleaseIdV1) -> ! {
    match value {}
}

#[test]
fn release_kinds_are_nominal_and_difference_registry_is_empty() {
    assert_ne!(
        TypeId::of::<OutputProjectionReleaseIdV1>(),
        TypeId::of::<DifferenceCalibrationReleaseIdV1>(),
    );
    let _: fn(DifferenceCalibrationReleaseIdV1) -> ! = difference_release_is_uninhabited;
    assert_eq!(
        CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1.key(),
        "css-color-4-oklch-d65-from-modeled-iec61966-srgb8-solid-v1",
    );
}

#[test]
fn projector_signature_has_no_alpha_metric_or_fallback_input() {
    let projector: fn(
        OutputProjectionRequestV1,
    ) -> Result<OutputProjectionV1, OutputProjectionErrorV1> = project_output_v1;
    assert!(
        projector(OutputProjectionRequestV1::new(
            source([0x44, 0x88, 0xCC], 64.0),
            CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1,
        ))
        .is_ok()
    );
}

#[test]
fn source_binding_rejects_bytes_from_an_unrelated_modeled_derivation() {
    let occurrence_model = modeled([0x44, 0x88, 0xCC]);
    let unrelated_model = modeled([0xCC, 0x88, 0x44]);
    let occurrence = LcsOccurrence::in_context(occurrence_model.sample(), context(64.0)).unwrap();

    assert_eq!(
        ProjectionSourceV1::bind(occurrence, unrelated_model),
        Err(ProjectionSourceFormationErrorV1::OccurrenceSampleMismatch {
            occurrence: occurrence_model.sample(),
            modeled: unrelated_model.sample(),
        }),
    );
}

#[test]
fn certificate_replays_source_view_formula_and_exact_css_bytes() {
    let projected = project([0x44, 0x88, 0xCC], 64.0);
    let certificate = projected.certificate();

    assert_eq!(certificate.replay().unwrap(), projected);
    assert_eq!(
        certificate.release(),
        CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1,
    );
    assert_eq!(certificate.oklab_release(), OKLAB_VIEW_RELEASE_V1);
    assert_eq!(certificate.oklch_release(), OKLCH_VIEW_RELEASE_V1);
    assert_eq!(
        certificate.oklch_release().key(),
        "polar-from-ottosson-2021-01-25-oklab-v1",
    );
    assert_eq!(
        certificate.number_encoding(),
        CssOklchNumberEncodingReleaseIdV1::LPercent5C6Hue3V1,
    );
    assert_eq!(
        certificate.hue_serialization(),
        CssOklchHueSerializationReleaseIdV1::ExactSourceGreyOrRectangularOriginToZeroV1,
    );
    assert_eq!(
        certificate.gamut_treatment(),
        OutputGamutTreatmentV1::NoExplicitProjectionGamutMapV1,
    );
    assert_eq!(
        certificate.source_signal().srgb8().bytes(),
        [0x44, 0x88, 0xCC]
    );
    assert_eq!(
        certificate.source_provenance(),
        certificate.source().modeled().provenance(),
    );
}

#[test]
fn occurrence_context_remains_in_certificate_identity_not_css_geometry() {
    let first = project([0x44, 0x88, 0xCC], 32.0);
    let second = project([0x44, 0x88, 0xCC], 64.0);

    assert_eq!(first.value(), second.value());
    assert_ne!(first.certificate(), second.certificate());
    assert_ne!(
        first.certificate().source_occurrence(),
        second.certificate().source_occurrence(),
    );
}

#[test]
fn solid_css_has_no_alpha_and_exact_encoded_greys_use_the_release_zero_convention() {
    for byte in u8::MIN..=u8::MAX {
        let projected = project([byte; 3], 64.0);
        let css = projected.value().as_str();
        let view = projected.oklch_view();
        match view.hue() {
            crate::lcs_occurrence::HueState::UndefinedExact => assert_eq!(view.c(), 0.0),
            crate::lcs_occurrence::HueState::Defined(_) => assert!(view.c() > 0.0),
            crate::lcs_occurrence::HueState::PowerlessBy(_) => {
                panic!("pure polar view introduced a powerless policy state")
            }
        }
        assert!(css.starts_with("oklch(") && css.ends_with(')'), "{css}");
        assert!(!css.contains('/'), "solid release emitted alpha: {css}");
        assert!(css.ends_with(" 0.000)"), "grey hue policy drifted: {css}");
        assert!(!css.contains("-0."), "signed zero escaped: {css}");
    }

    assert!(matches!(
        project([u8::MAX; 3], 64.0).oklch_view().hue(),
        crate::lcs_occurrence::HueState::Defined(_),
    ));
}

fn parse_solid_css(css: &str) -> [f64; 3] {
    let inner = css
        .strip_prefix("oklch(")
        .and_then(|value| value.strip_suffix(')'))
        .expect("registered solid CSS shape");
    assert!(!inner.contains('/'));
    let mut fields = inner.split_whitespace();
    let l = fields
        .next()
        .and_then(|value| value.strip_suffix('%'))
        .expect("percentage lightness")
        .parse::<f64>()
        .unwrap()
        / 100.0;
    let c = fields.next().unwrap().parse::<f64>().unwrap();
    let h = fields.next().unwrap().parse::<f64>().unwrap();
    assert!(fields.next().is_none());
    [l, c, h]
}

fn replay_css_through_existing_inverse_to_srgb8(css: &str) -> Srgb8 {
    let [l, c, h] = parse_solid_css(css);
    let (sin, cos) = h.to_radians().sin_cos();
    srgb8_from_linear(oklab_to_srgb_linear([l, c * cos, c * sin]))
}

#[test]
fn registered_precision_replays_a_non_aligned_lattice_through_existing_inverse_clamp_and_round() {
    // 16^3 points including both ends.  This is a deterministic regression
    // corpus, not a claim about every CSS implementation or all 16.7M inputs.
    let steps: Vec<u8> = (0_u16..=255).step_by(17).map(|value| value as u8).collect();
    assert_eq!(steps.first(), Some(&0));
    assert_eq!(steps.last(), Some(&255));

    for &red in &steps {
        for &green in &steps {
            for &blue in &steps {
                let expected = Srgb8::new([red, green, blue]);
                let projected = project(expected.bytes(), 64.0);
                assert_eq!(
                    replay_css_through_existing_inverse_to_srgb8(projected.value().as_str()),
                    expected,
                    "CSS byte replay drifted: {}",
                    projected.value().as_str(),
                );
            }
        }
    }
}

#[test]
fn fixed_css_values_pin_formula_order_and_solid_serialization() {
    for (bytes, expected) in [
        ([0x00, 0x00, 0x00], "oklch(0.00000% 0.000000 0.000)"),
        ([0xFF, 0xFF, 0xFF], "oklch(100.00000% 0.000000 0.000)"),
        ([0xFF, 0x00, 0x00], "oklch(62.79554% 0.257683 29.234)"),
    ] {
        assert_eq!(project(bytes, 64.0).value().as_str(), expected);
    }
}
