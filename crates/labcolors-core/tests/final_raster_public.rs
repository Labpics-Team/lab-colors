//! Контрпримеры проходят публичный Core-вход; crate-private import отсутствует.
use labcolors_core::program_wire::{
    ProgramFinalRasterErrorV1 as E, ProgramFinalRasterInputV1 as Input,
    ProgramSnapshotStateV1 as State, compile_program_wire, final_raster_source_over_wire_v1,
};
fn input(
    width: u32,
    height: u32,
    dpr: u8,
    context: [u8; 32],
    source: &[[u8; 4]],
    destination: &[[u8; 4]],
) -> Result<Input, E> {
    Input::try_new(width, height, dpr, context, 1, source, 2, destination)
}
fn runtime() -> Result<labcolors_core::program_wire::ProgramSessionV1, E> {
    Ok(compile_program_wire(&final_raster_source_over_wire_v1(1,2,3,17,labcolors_core::program_wire::FinalRasterCleanlinessRequirementV1::MandatoryExactNominalSrgb8V2)).map_err(E::Runtime)?.instantiate(100).map_err(E::Runtime)?)
}
#[test]
fn public_complete_spatial_raster_has_independent_source_over_bytes() -> Result<(), E> {
    let mut session = runtime()?;
    let context = [7; 32];
    // Первый пиксель: 64 + round(128*127/255) = 128.
    // Второй: нулевой foreground оставляет opaque destination 64.
    let payload = input(
        2,
        1,
        2,
        context,
        &[[64, 64, 64, 128], [0, 0, 0, 0]],
        &[[128, 128, 128, 255], [64, 64, 64, 255]],
    )?;
    let snapshot = session.update_final_raster(1, &payload)?;
    assert_eq!(snapshot.state(), State::Ready);
    assert!(snapshot.assessment_report().is_none());
    let output = session.current_final_raster(session.content_identity(), 17, 1, context)?;
    assert_eq!(
        (
            output.width(),
            output.height(),
            output.device_pixel_ratio(),
            output.pixel_count()
        ),
        (2, 1, 2, 2)
    );
    assert_eq!(output.pixels().collect::<Vec<_>>(), vec![[128; 3], [64; 3]]);
    Ok(())
}
#[test]
fn final_dirty_pixel_rejects_complete_raster_instead_of_first_sample() -> Result<(), E> {
    let mut session = runtime()?;
    let context = [7; 32];
    let clean = [[0, 199, 63, 255]; 4];
    let destination = [[0, 0, 0, 255]; 4];
    assert_eq!(
        session
            .update_final_raster(1, &input(2, 2, 1, context, &clean, &destination)?)?
            .state(),
        State::Ready
    );
    let mut last_dirty = clean;
    last_dirty[3] = [0, 199, 64, 255];
    assert_eq!(
        session
            .update_final_raster(2, &input(2, 2, 1, context, &last_dirty, &destination)?)?
            .state(),
        State::Failed
    );
    assert_eq!(
        session.final_raster_refusal(),
        Some(E::ConventionRejected {
            pixel_index: 3,
            composite: [0, 199, 64],
            rejected_blue_interval: [64, 108],
        })
    );
    assert!(
        session
            .current_final_raster(session.content_identity(), 17, 1, context)
            .is_err()
    );
    assert!(
        session
            .current_final_raster(session.content_identity(), 17, 2, context)
            .is_err()
    );
    Ok(())
}
#[test]
fn complete_shape_and_premultiplication_precede_revision_admission() {
    for bad_length in [3, 5] {
        let source = vec![[0, 0, 0, 0]; bad_length];
        assert!(matches!(
            Input::try_new(2, 2, 1, [0; 32], 1, &source, 2, &[[0, 0, 0, 255]; 4]),
            Err(E::InvalidLength)
        ));
    }
    assert!(matches!(
        Input::try_new(0, 2, 1, [0; 32], 1, &[], 2, &[]),
        Err(E::InvalidExtent)
    ));
    assert!(matches!(
        Input::try_new(65537, 1, 1, [0; 32], 1, &[], 2, &[]),
        Err(E::ResourceExhausted)
    ));
    assert!(matches!(
        Input::try_new(1, 1, 5, [0; 32], 1, &[[0; 4]], 2, &[[0, 0, 0, 255]]),
        Err(E::InvalidDevicePixelRatio)
    ));
    assert!(matches!(
        Input::try_new(
            1,
            1,
            1,
            [0; 32],
            1,
            &[[128, 0, 0, 127]],
            2,
            &[[0, 0, 0, 255]]
        ),
        Err(E::InvalidPremultipliedPixel)
    ));
}
#[test]
fn idempotence_binds_every_byte_extent_dpr_and_context() -> Result<(), E> {
    let mut session = runtime()?;
    let context = [7; 32];
    let source = [[0, 0, 0, 0]; 4];
    let dest = [[64, 64, 64, 255]; 4];
    let payload = input(2, 2, 1, context, &source, &dest)?;
    session.update_final_raster(1, &payload)?;
    session.update_final_raster(1, &payload)?;
    let mut changed = dest;
    changed[3] = [65, 65, 65, 255];
    for altered in [
        input(2, 2, 1, context, &source, &changed)?,
        input(1, 4, 1, context, &source, &dest)?,
        input(2, 2, 2, context, &source, &dest)?,
        input(2, 2, 1, [8; 32], &source, &dest)?,
    ] {
        assert!(session.update_final_raster(1, &altered).is_err());
    }
    assert_eq!(
        session
            .current_final_raster(session.content_identity(), 17, 1, context)?
            .pixels()
            .collect::<Vec<_>>(),
        vec![[64; 3]; 4]
    );
    session.update_final_raster(2, &input(2, 2, 2, [8; 32], &source, &changed)?)?;
    assert!(matches!(
        session.current_final_raster(session.content_identity(), 17, 1, context),
        Err(E::ContextBinding) | Err(E::Stale)
    ));
    assert!(matches!(
        session.current_final_raster(session.content_identity(), 17, 2, context),
        Err(E::ContextBinding)
    ));
    assert!(matches!(
        session.current_final_raster([0; 32], 17, 2, [8; 32]),
        Err(E::ProgramBinding)
    ));
    assert!(matches!(
        session.current_final_raster(session.content_identity(), 18, 2, [8; 32]),
        Err(E::OutputBinding)
    ));
    assert!(session.update_final_raster(1, &payload).is_err());
    session.update_unknown(3, 55).map_err(E::Runtime)?;
    assert!(matches!(
        session.current_final_raster(session.content_identity(), 17, 2, [8; 32]),
        Err(E::Stale)
    ));
    session.update_final_raster(4, &payload)?;
    assert!(
        session
            .current_final_raster(session.content_identity(), 17, 4, context)
            .is_ok()
    );
    Ok(())
}
#[test]
fn transparent_final_pixels_cannot_reach_consumer() -> Result<(), E> {
    let mut session = runtime()?;
    let payload = input(1, 1, 1, [0; 32], &[[0; 4]], &[[64, 64, 64, 128]])?;
    assert_eq!(
        session.update_final_raster(1, &payload)?.state(),
        State::Failed
    );
    assert_eq!(
        session.final_raster_refusal(),
        Some(E::TransparentFinalPixel { pixel_index: 0 })
    );
    assert!(
        session
            .current_final_raster(session.content_identity(), 17, 1, [0; 32])
            .is_err()
    );
    Ok(())
}

#[test]
fn undeclared_cleanliness_cannot_become_implicit_mandatory() {
    let mut wire = final_raster_source_over_wire_v1(1,2,3,17,
        labcolors_core::program_wire::FinalRasterCleanlinessRequirementV1::MandatoryExactNominalSrgb8V2);
    for unsupported in [3, 255] {
        wire[11] = unsupported;
        assert!(compile_program_wire(&wire).is_err());
    }
}

#[test]
fn final_raster_policy_modes_bind_identity_and_preserve_locked_final_pixels() -> Result<(), E> {
    use labcolors_core::program_wire::{
        FinalRasterAssessmentVerdictV1 as V, FinalRasterCleanlinessAssessmentV1,
        FinalRasterCleanlinessRequirementV1 as C, FinalRasterCleanlinessViolationV1,
    };
    let policies = [
        C::Off,
        C::ReportOnlyExactNominalSrgb8V2,
        C::MandatoryExactNominalSrgb8V2,
    ];
    let mut identities = Vec::new();
    let source = [[0; 4]; 4];
    let destination = [
        [0, 199, 63, 255],
        [0, 199, 64, 255],
        [32, 32, 32, 255],
        [0, 199, 64, 255],
    ];
    for policy in policies {
        let wire = final_raster_source_over_wire_v1(1, 2, 3, 17, policy);
        assert_eq!(wire[11], policy as u8);
        let mut session = compile_program_wire(&wire)
            .map_err(E::Runtime)?
            .instantiate(100)
            .map_err(E::Runtime)?;
        let identity = session.content_identity();
        assert!(!identities.contains(&identity));
        identities.push(identity);
        let snapshot =
            session.update_final_raster(1, &input(2, 2, 1, [7; 32], &source, &destination)?)?;
        assert!(snapshot.assessment_report().is_none());
        if policy == C::MandatoryExactNominalSrgb8V2 {
            assert_eq!(snapshot.state(), State::Failed);
            assert!(matches!(
                session.final_raster_refusal(),
                Some(E::ConventionRejected { pixel_index: 1, .. })
            ));
            assert!(
                session
                    .current_final_raster(identity, 17, 1, [7; 32])
                    .is_err()
            );
            continue;
        }
        assert_eq!(snapshot.state(), State::Ready);
        assert!(session.final_raster_refusal().is_none());
        let output = session.current_final_raster(identity, 17, 1, [7; 32])?;
        assert_eq!(
            output.pixels().collect::<Vec<_>>(),
            vec![[0, 199, 63], [0, 199, 64], [32, 32, 32], [0, 199, 64]]
        );
        let report: FinalRasterCleanlinessAssessmentV1 = output.cleanliness_assessment();
        assert_eq!(report.requirement(), policy);
        if policy == C::Off {
            assert_eq!(report.verdict(), V::NotApplied);
            assert_eq!(report.checked_pixels(), 0);
            assert_eq!(report.violation_count(), 0);
            assert!(report.first_violation().is_none());
            assert!(report.profile_release().is_none());
        } else {
            assert_eq!(report.verdict(), V::Violation);
            assert_eq!(report.checked_pixels(), 4);
            assert_eq!(report.violation_count(), 2);
            let first: FinalRasterCleanlinessViolationV1 =
                report.first_violation().ok_or(E::CleanConvention)?;
            assert_eq!(first.pixel_index(), 1);
            assert_eq!(first.composite(), [0, 199, 64]);
            assert_eq!(first.rejected_blue_interval(), [64, 108]);
            assert!(report.profile_release().is_some());
        }
    }
    Ok(())
}

#[test]
fn final_raster_lint_rechecks_new_background_and_every_pixel_without_granting_stale_authority()
-> Result<(), E> {
    use labcolors_core::program_wire::{
        FinalRasterAssessmentVerdictV1 as V, FinalRasterCleanlinessRequirementV1 as C,
    };
    let mut session = compile_program_wire(&final_raster_source_over_wire_v1(
        1,
        2,
        3,
        17,
        C::ReportOnlyExactNominalSrgb8V2,
    ))
    .map_err(E::Runtime)?
    .instantiate(100)
    .map_err(E::Runtime)?;
    let source = [[0; 4]; 4];
    let background = [
        [0, 199, 63, 255],
        [0, 199, 63, 255],
        [0, 199, 63, 255],
        [0, 199, 64, 255],
    ];
    let old = session.update_final_raster(1, &input(2, 2, 1, [7; 32], &source, &background)?)?;
    assert_eq!(old.state(), State::Ready);
    let saved = session
        .current_final_raster(session.content_identity(), 17, 1, [7; 32])?
        .cleanliness_assessment();
    assert_eq!(saved.checked_pixels(), 4);
    assert_eq!(
        saved
            .first_violation()
            .ok_or(E::CleanConvention)?
            .pixel_index(),
        3
    );
    session.update_unknown(2, 55).map_err(E::Runtime)?;
    assert!(
        session
            .current_final_raster(session.content_identity(), 17, 1, [7; 32])
            .is_err()
    );
    let foreground = [[32, 32, 32, 255]; 4];
    session.update_final_raster(3, &input(2, 2, 1, [8; 32], &foreground, &background)?)?;
    assert!(
        session
            .current_final_raster(session.content_identity(), 17, 3, [7; 32])
            .is_err()
    );
    let fresh = session.current_final_raster(session.content_identity(), 17, 3, [8; 32])?;
    assert_eq!(fresh.pixels().collect::<Vec<_>>(), vec![[32; 3]; 4]);
    assert_eq!(fresh.cleanliness_assessment().verdict(), V::Pass);
    assert_eq!(fresh.cleanliness_assessment().checked_pixels(), 4);
    assert_eq!(fresh.cleanliness_assessment().violation_count(), 0);
    assert!(fresh.cleanliness_assessment().first_violation().is_none());
    assert_eq!(saved.verdict(), V::Violation);
    Ok(())
}

#[test]
fn final_raster_off_and_lint_keep_complete_opaque_domain_and_technical_quality_requirements()
-> Result<(), E> {
    use labcolors_core::program_wire::FinalRasterCleanlinessRequirementV1 as C;
    for policy in [C::Off, C::ReportOnlyExactNominalSrgb8V2] {
        let mut session =
            compile_program_wire(&final_raster_source_over_wire_v1(1, 2, 3, 17, policy))
                .map_err(E::Runtime)?
                .instantiate(100)
                .map_err(E::Runtime)?;
        let complete = input(1, 1, 1, [7; 32], &[[0; 4]], &[[32, 32, 32, 255]])?;
        session.update_final_raster(1, &complete)?;
        assert!(matches!(
            Input::try_new(1, 1, 1, [7; 32], 1, &[], 2, &[[32, 32, 32, 255]]),
            Err(E::InvalidLength)
        ));
        assert!(
            session
                .current_final_raster(session.content_identity(), 17, 1, [7; 32])
                .is_ok()
        );
        let translucent = input(1, 1, 1, [7; 32], &[[0; 4]], &[[32, 32, 32, 128]])?;
        let failed = session.update_final_raster(2, &translucent)?;
        assert_eq!(failed.state(), State::Failed);
        assert_eq!(
            session.final_raster_refusal(),
            Some(E::TransparentFinalPixel { pixel_index: 0 })
        );
        assert!(
            session
                .current_final_raster(session.content_identity(), 17, 2, [7; 32])
                .is_err()
        );
    }
    Ok(())
}
