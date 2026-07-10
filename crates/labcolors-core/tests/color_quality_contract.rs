use labcolors_core::{
    AppearanceContextV1, AppearanceMode, COLOR_QUALITY_AUDIT_MODEL_V1, ModelProvenance,
    ViewingConditions, WARM_DARK_INTERACTION_MODEL_V2, WarmDarkInteractionStatusV2,
    analyze_warm_dark_interaction_v2, audit_color_quality_v1,
};

fn nominal(mode: AppearanceMode) -> AppearanceContextV1 {
    AppearanceContextV1::nominal(mode, ViewingConditions::srgb())
}

#[test]
fn positive_research_signal_is_not_a_human_dirty_verdict() {
    let report = analyze_warm_dark_interaction_v2(
        "#6B6B2E",
        "#FFFFFF",
        &nominal(AppearanceMode::SurfaceLike),
    )
    .unwrap();

    assert_eq!(report.model, WARM_DARK_INTERACTION_MODEL_V2);
    assert_eq!(report.provenance, ModelProvenance::LabpicsHypothesis);
    assert_eq!(
        report.status,
        WarmDarkInteractionStatusV2::InteractionPositive
    );
    assert!(report.interaction_potential.unwrap() > 0.0);
}

#[test]
fn the_same_rgb_keeps_emissive_and_surface_requests_distinct() {
    let emissive = audit_color_quality_v1(
        "#6B6B2E",
        "#FFFFFF",
        None,
        &nominal(AppearanceMode::EmissiveUi),
    )
    .unwrap();
    let surface = audit_color_quality_v1(
        "#6B6B2E",
        "#FFFFFF",
        None,
        &nominal(AppearanceMode::SurfaceLike),
    )
    .unwrap();

    assert_eq!(emissive.model, COLOR_QUALITY_AUDIT_MODEL_V1);
    assert_eq!(surface.model, COLOR_QUALITY_AUDIT_MODEL_V1);
    assert_eq!(emissive.context.mode, AppearanceMode::EmissiveUi);
    assert_eq!(surface.context.mode, AppearanceMode::SurfaceLike);
    assert_ne!(emissive.context.mode, surface.context.mode);

    // Номинальный skeleton пока не заявляет mode-specific observer model.
    // Поэтому физические CAM16-UCS факты совпадают, а различие запроса не теряется.
    assert_eq!(
        emissive.appearance.jp.to_bits(),
        surface.appearance.jp.to_bits()
    );
    assert_eq!(
        emissive.appearance.mp.to_bits(),
        surface.appearance.mp.to_bits()
    );
}

#[test]
fn black_background_is_insufficient_context_not_proof_of_zero_interaction() {
    let report = analyze_warm_dark_interaction_v2(
        "#6B6B2E",
        "#000000",
        &nominal(AppearanceMode::SurfaceLike),
    )
    .unwrap();

    assert_eq!(
        report.status,
        WarmDarkInteractionStatusV2::InsufficientContext
    );
    assert_eq!(report.foreground_background_ratio, None);
    assert_eq!(report.interaction_potential, None);
}

#[test]
fn spatial_effect_is_not_misclassified_by_an_opaque_patch_model() {
    let report = analyze_warm_dark_interaction_v2(
        "#6B6B2E",
        "#FFFFFF",
        &nominal(AppearanceMode::SpatialEffect),
    )
    .unwrap();

    assert_eq!(
        report.status,
        WarmDarkInteractionStatusV2::NotApplicable(AppearanceMode::SpatialEffect)
    );
    assert_eq!(report.interaction_potential, None);
}
