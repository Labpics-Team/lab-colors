use crate::lcs_occurrence::{
    AppearanceContextId, AppearanceContextReleaseId, ColorimetricFrameId,
    ColorimetricFrameReleaseId, HueAngle, HueState, LcsOccurrence, NumericDomainError,
    ObserveError, ObserverProfileId, ReferenceWhiteId, SurroundProfileId, TristimulusSample,
    TristimulusScale,
};

fn frame(scale: TristimulusScale) -> ColorimetricFrameId {
    ColorimetricFrameId::new(
        ObserverProfileId::Cie1931TwoDegreeV1,
        ReferenceWhiteId::Iec61966D65ChromaticityV1,
        scale,
        ColorimetricFrameReleaseId::XyzV1,
    )
}

fn context(frame: ColorimetricFrameId, la: f64) -> AppearanceContextId {
    AppearanceContextId::new(
        AppearanceContextReleaseId::Cam16V1,
        frame,
        la,
        0.2,
        SurroundProfileId::AverageV1,
    )
    .unwrap()
}

#[test]
fn xyz_and_context_numeric_admission_is_fail_closed() {
    let relative = frame(TristimulusScale::RelativeY1);
    for xyz in [
        [f64::NAN, 0.0, 0.0],
        [0.0, f64::INFINITY, 0.0],
        [0.0, 0.0, -f64::MIN_POSITIVE],
    ] {
        assert!(TristimulusSample::new(xyz, relative).is_err());
    }
    assert_eq!(
        AppearanceContextId::new(
            AppearanceContextReleaseId::Cam16V1,
            relative,
            0.0,
            0.2,
            SurroundProfileId::AverageV1,
        ),
        Err(NumericDomainError::NotPositive)
    );
    assert_eq!(
        AppearanceContextId::new(
            AppearanceContextReleaseId::Cam16V1,
            relative,
            64.0,
            0.0,
            SurroundProfileId::AverageV1,
        ),
        Err(NumericDomainError::NotPositive)
    );
    assert_eq!(
        AppearanceContextId::new(
            AppearanceContextReleaseId::Cam16V1,
            relative,
            64.0,
            1.01,
            SurroundProfileId::AverageV1,
        ),
        Err(NumericDomainError::AboveOne)
    );
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
    let relative = frame(TristimulusScale::RelativeY1);
    let mismatching = ColorimetricFrameId::new(
        ObserverProfileId::Cie1931TwoDegreeV1,
        ReferenceWhiteId::Iec61966D65ChromaticityV1,
        TristimulusScale::RelativeY1,
        ColorimetricFrameReleaseId::MutationSentinelV1,
    );
    let sample = TristimulusSample::new([0.1, 0.2, 0.3], relative).unwrap();
    assert_eq!(
        LcsOccurrence::observe(sample, context(mismatching, 64.0)),
        Err(ObserveError::FrameMismatch {
            stimulus: relative,
            context: mismatching,
        })
    );
}

#[test]
fn occurrence_identity_contains_only_sample_and_context() {
    let relative = frame(TristimulusScale::RelativeY1);
    let sample = TristimulusSample::new([0.1, 0.2, 0.3], relative).unwrap();
    let observed = LcsOccurrence::observe(sample, context(relative, 64.0)).unwrap();
    assert_eq!(observed.sample(), sample);
    assert_eq!(observed.context(), context(relative, 64.0));
}

#[test]
fn context_identity_changes_with_semantic_input_bits() {
    let relative = frame(TristimulusScale::RelativeY1);
    assert_ne!(context(relative, 64.0), context(relative, 32.0));
}

#[test]
fn hue_numeric_constructor_rejects_nonfinite_and_out_of_domain() {
    for invalid in [f64::NAN, f64::INFINITY, -0.01, 360.0] {
        assert!(HueAngle::new(invalid).is_err());
    }
    assert_eq!(HueAngle::new(-0.0).unwrap(), HueAngle::new(0.0).unwrap());
}
