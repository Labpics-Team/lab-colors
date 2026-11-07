//! Публичная конечная locked tuple через того же Program/Session owner.
use super::*;
use crate::field_effect::FieldExtentV1;
use crate::field_effect::final_raster::{FieldObservedPayloadV1, FinalRasterInputErrorV1};
use crate::program_session::finite_field::{FinalRasterMaterializationV1, FinalRasterRefusalV1};
use crate::shared::Shared;

/// Предел device pixels первого finite-raster профиля.
pub const MAX_FINAL_RASTER_PIXELS_V1: usize = 65_536;

pub use crate::field_effect::final_raster::{
    FinalRasterAssessmentVerdictV1, FinalRasterCleanlinessAssessmentV1,
    FinalRasterCleanlinessRequirementV1, FinalRasterCleanlinessViolationV1,
};

/// Декларация LCPW v4: locked premultiplied source-over → final opaque raster.
/// Допустимое множество — ровно переданная tuple. Цвета не корректируются.
/// Формат не утверждает соответствие браузерной композиции или человеческое восприятие.
#[must_use]
pub fn final_raster_source_over_wire_v1(
    source: u64,
    destination: u64,
    operator: u64,
    output: u32,
    cleanliness: FinalRasterCleanlinessRequirementV1,
) -> [u8; 40] {
    let mut bytes = [0; 40];
    bytes[..4].copy_from_slice(b"LCPW");
    bytes[4..6].copy_from_slice(&4_u16.to_le_bytes());
    bytes[6..10].copy_from_slice(&40_u32.to_le_bytes());
    bytes[10] = 1;
    bytes[11] = cleanliness as u8;
    bytes[12..20].copy_from_slice(&source.to_le_bytes());
    bytes[20..28].copy_from_slice(&destination.to_le_bytes());
    bytes[28..36].copy_from_slice(&operator.to_le_bytes());
    bytes[36..40].copy_from_slice(&output.to_le_bytes());
    bytes
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProgramFinalRasterErrorV1 {
    InvalidExtent,
    InvalidDevicePixelRatio,
    InvalidLength,
    InvalidPremultipliedPixel,
    PortBinding,
    ResourceExhausted,
    Runtime(ProgramRuntimeErrorV1),
    Unsupported,
    TransparentFinalPixel {
        pixel_index: usize,
    },
    ConventionRejected {
        pixel_index: usize,
        composite: [u8; 3],
        rejected_blue_interval: [u8; 2],
    },
    TechnicalQuality,
    CleanConvention,
    Stale,
    ProgramBinding,
    OutputBinding,
    ContextBinding,
}
impl From<FinalRasterInputErrorV1> for ProgramFinalRasterErrorV1 {
    fn from(value: FinalRasterInputErrorV1) -> Self {
        match value {
            FinalRasterInputErrorV1::Extent => Self::InvalidExtent,
            FinalRasterInputErrorV1::DevicePixelRatio => Self::InvalidDevicePixelRatio,
            FinalRasterInputErrorV1::Length => Self::InvalidLength,
            FinalRasterInputErrorV1::PremultipliedPixel => Self::InvalidPremultipliedPixel,
            FinalRasterInputErrorV1::PortBinding => Self::PortBinding,
            FinalRasterInputErrorV1::ResourceExhausted => Self::ResourceExhausted,
        }
    }
}
impl From<FinalRasterRefusalV1> for ProgramFinalRasterErrorV1 {
    fn from(value: FinalRasterRefusalV1) -> Self {
        match value {
            FinalRasterRefusalV1::Unsupported => Self::Unsupported,
            FinalRasterRefusalV1::Transparent { pixel_index } => {
                Self::TransparentFinalPixel { pixel_index }
            }
            FinalRasterRefusalV1::Convention {
                pixel_index,
                composite,
                interval,
            } => Self::ConventionRejected {
                pixel_index,
                composite: composite.bytes(),
                rejected_blue_interval: interval,
            },
            FinalRasterRefusalV1::TechnicalQuality => Self::TechnicalQuality,
            FinalRasterRefusalV1::CleanConvention => Self::CleanConvention,
            FinalRasterRefusalV1::Stale => Self::Stale,
            FinalRasterRefusalV1::ProgramBinding => Self::ProgramBinding,
            FinalRasterRefusalV1::OutputBinding => Self::OutputBinding,
            FinalRasterRefusalV1::ContextBinding => Self::ContextBinding,
            FinalRasterRefusalV1::ResourceExhausted => Self::ResourceExhausted,
        }
    }
}

impl ProgramFinalRasterErrorV1 {
    /// Стабильный код отказа; адаптеры не определяют собственное отображение.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidExtent => "program_field_extent",
            Self::InvalidDevicePixelRatio => "program_field_dpr",
            Self::InvalidLength => "program_field_length",
            Self::InvalidPremultipliedPixel => "program_field_premultiplied",
            Self::PortBinding => "program_field_ports",
            Self::ResourceExhausted | Self::Runtime(ProgramRuntimeErrorV1::ResourceExhausted) => {
                "program_resource_exhausted"
            }
            Self::Runtime(_) => "program_field_update",
            Self::Unsupported => "program_field_unsupported",
            Self::TransparentFinalPixel { .. } => "program_field_transparent",
            Self::ConventionRejected { .. } => "program_field_convention",
            Self::TechnicalQuality => "program_field_tq",
            Self::CleanConvention => "program_field_cc",
            Self::Stale => "program_field_stale",
            Self::ProgramBinding => "program_field_program",
            Self::OutputBinding => "program_field_output",
            Self::ContextBinding => "program_field_context",
        }
    }
}

/// Полный immutable row-major payload. Внешние массивы копируются после budget/shape checks.
pub struct ProgramFinalRasterInputV1(Shared<FieldObservedPayloadV1>);
impl ProgramFinalRasterInputV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        width: u32,
        height: u32,
        dpr: u8,
        context: [u8; 32],
        source_port: u64,
        source: &[[u8; 4]],
        destination_port: u64,
        destination: &[[u8; 4]],
    ) -> Result<Self, ProgramFinalRasterErrorV1> {
        let extent = FieldExtentV1::try_new(width, height)
            .map_err(|_| ProgramFinalRasterErrorV1::InvalidExtent)?;
        Ok(Self(Shared::new(FieldObservedPayloadV1::try_new(
            extent,
            dpr,
            context,
            source_port,
            source,
            destination_port,
            destination,
        )?)))
    }
}

/// Заимствование current final bytes удерживает Session от update/free.
/// Копия пикселей пригодна для диагностики; current authority заново получает owner.
pub struct ProgramCurrentFinalRasterV1<'a> {
    materialization: &'a FinalRasterMaterializationV1,
}
impl ProgramCurrentFinalRasterV1<'_> {
    #[must_use]
    pub fn width(&self) -> u32 {
        self.materialization.payload().extent.width()
    }
    #[must_use]
    pub fn height(&self) -> u32 {
        self.materialization.payload().extent.height()
    }
    #[must_use]
    pub fn device_pixel_ratio(&self) -> u8 {
        self.materialization.payload().dpr.value()
    }
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.materialization.revision()
    }
    #[must_use]
    pub fn output_slot(&self) -> u32 {
        self.materialization.output()
    }
    #[must_use]
    pub fn pixel_count(&self) -> usize {
        self.materialization.pixels().len()
    }
    #[must_use]
    pub fn cleanliness_assessment(&self) -> FinalRasterCleanlinessAssessmentV1 {
        self.materialization.cleanliness_assessment()
    }
    pub fn pixels(&self) -> impl ExactSizeIterator<Item = [u8; 3]> + '_ {
        self.materialization.pixels().iter().map(|pixel| {
            let rgba = pixel.channels();
            [rgba[0], rgba[1], rgba[2]]
        })
    }
}
impl ProgramSessionV1 {
    #[must_use]
    pub fn content_identity(&self) -> [u8; 32] {
        *self.owner.content_identity().as_bytes()
    }

    pub fn update_final_raster(
        &mut self,
        revision: u64,
        input: &ProgramFinalRasterInputV1,
    ) -> Result<ProgramSnapshotV1, ProgramFinalRasterErrorV1> {
        self.owner.validate_final_raster_input(&input.0)?;
        // Provenance хранится в field payload; point-клеток и сценариев нет.
        self.assessment_scratch
            .reserve(0, 0)
            .map_err(|_| ProgramFinalRasterErrorV1::ResourceExhausted)?;
        reserve_snapshot_items(&mut self.outputs_scratch, self.owner.output_slots().count())
            .map_err(|_| ProgramFinalRasterErrorV1::ResourceExhausted)?;
        let transition = self
            .owner
            .prepare_field_update(&mut self.session, revision, Shared::clone(&input.0))
            .map_err(|error| ProgramFinalRasterErrorV1::Runtime(map_runtime_update_error(error)))?;
        let evidence = transition.commit();
        snapshot_from_evidence_into(
            evidence,
            &mut self.outputs_scratch,
            None,
            self.owner.snapshot_version(),
            revision,
        )
        .map_err(ProgramFinalRasterErrorV1::Runtime)
    }
    #[must_use]
    pub fn final_raster_refusal(&self) -> Option<ProgramFinalRasterErrorV1> {
        self.owner
            .final_raster_refusal(&self.session)
            .map(Into::into)
    }
    /// Обе fresh ветви проверяются до выдачи bytes. Historical snapshot не принимается.
    pub fn current_final_raster(
        &self,
        content_identity: [u8; 32],
        output: u32,
        revision: u64,
        context: [u8; 32],
    ) -> Result<ProgramCurrentFinalRasterV1<'_>, ProgramFinalRasterErrorV1> {
        let materialization = self.owner.current_final_raster(
            &self.session,
            content_identity,
            output,
            revision,
            context,
        )?;
        Ok(ProgramCurrentFinalRasterV1 { materialization })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::{AuthorityExpectedCurrentV1, AuthorityStateV1};
    fn session() -> Result<ProgramSessionV1, ProgramFinalRasterErrorV1> {
        compile_program_wire(&final_raster_source_over_wire_v1(
            1,
            2,
            3,
            17,
            FinalRasterCleanlinessRequirementV1::MandatoryExactNominalSrgb8V2,
        ))
        .map_err(ProgramFinalRasterErrorV1::Runtime)?
        .instantiate(101)
        .map_err(ProgramFinalRasterErrorV1::Runtime)
    }
    fn payload(shade: u8) -> Result<ProgramFinalRasterInputV1, ProgramFinalRasterErrorV1> {
        ProgramFinalRasterInputV1::try_new(
            1,
            1,
            1,
            [0; 32],
            1,
            &[[0; 4]],
            2,
            &[[shade, shade, shade, 255]],
        )
    }
    #[test]
    fn final_raster_input_preserves_cross_invalid_error_order() {
        let invalid_pixel = [[255, 0, 0, 0]];
        for (width, dpr, source, destination, destination_port, expected) in [
            (
                0,
                5,
                &[][..],
                &[][..],
                1,
                ProgramFinalRasterErrorV1::InvalidExtent,
            ),
            (
                65_537,
                5,
                &[][..],
                &[][..],
                1,
                ProgramFinalRasterErrorV1::ResourceExhausted,
            ),
            (
                1,
                5,
                &[][..],
                &[][..],
                1,
                ProgramFinalRasterErrorV1::InvalidDevicePixelRatio,
            ),
            (
                1,
                1,
                &[][..],
                &[][..],
                1,
                ProgramFinalRasterErrorV1::InvalidLength,
            ),
            (
                1,
                1,
                &invalid_pixel[..],
                &invalid_pixel[..],
                1,
                ProgramFinalRasterErrorV1::PortBinding,
            ),
            (
                1,
                1,
                &invalid_pixel[..],
                &invalid_pixel[..],
                2,
                ProgramFinalRasterErrorV1::InvalidPremultipliedPixel,
            ),
        ] {
            assert!(matches!(
                ProgramFinalRasterInputV1::try_new(
                    width, 1, dpr, [0; 32], 1, source, destination_port, destination,
                ),
                Err(actual) if actual == expected
            ));
        }
    }

    #[test]
    fn final_raster_metadata_reservation_failure_preserves_exact_commit_and_authority()
    -> Result<(), ProgramFinalRasterErrorV1> {
        use crate::program_session::{
            fail_program_preflight_reservation_for_test,
            program_preflight_failure_remaining_for_test,
        };
        let mut session = session()?;
        let identity = session.content_identity();
        let previous = payload(64)?;
        session.update_final_raster(1, &previous)?;
        let next = payload(65)?;
        let (old_materialization, old_observation, old_certificate, old_subject) = {
            let materialization =
                session
                    .owner
                    .current_final_raster(&session.session, identity, 17, 1, [0; 32])?;
            (
                std::ptr::from_ref(materialization),
                materialization.observation().backing_ptr_for_test(),
                materialization.certificate().digest(),
                materialization.subject(),
            )
        };
        {
            let _failure = fail_program_preflight_reservation_for_test(0);
            session.update_final_raster(1, &previous)?;
            assert_eq!(program_preflight_failure_remaining_for_test(), Some(0));
            assert!(matches!(
                session.update_final_raster(2, &next),
                Err(ProgramFinalRasterErrorV1::Runtime(
                    ProgramRuntimeErrorV1::ResourceExhausted
                ))
            ));
            assert_eq!(program_preflight_failure_remaining_for_test(), None);
        }
        let retained =
            session
                .owner
                .current_final_raster(&session.session, identity, 17, 1, [0; 32])?;
        assert_eq!(std::ptr::from_ref(retained), old_materialization);
        assert_eq!(
            retained.observation().backing_ptr_for_test(),
            old_observation
        );
        assert_eq!(retained.certificate().digest(), old_certificate);
        assert_eq!(retained.subject(), old_subject);
        assert_eq!(
            session
                .current_final_raster(identity, 17, 1, [0; 32])?
                .pixels()
                .collect::<Vec<_>>(),
            vec![[64; 3]]
        );
        assert_eq!(session.final_raster_refusal(), None);
        {
            let _failure = fail_program_preflight_reservation_for_test(1);
            session.update_final_raster(2, &next)?;
            assert_eq!(program_preflight_failure_remaining_for_test(), Some(0));
        }
        assert_eq!(
            session
                .current_final_raster(identity, 17, 2, [0; 32])?
                .pixels()
                .collect::<Vec<_>>(),
            vec![[65; 3]]
        );
        Ok(())
    }

    #[test]
    fn final_raster_refusals_skip_success_storage_reservation()
    -> Result<(), ProgramFinalRasterErrorV1> {
        use crate::program_session::{
            fail_program_preflight_reservation_for_test,
            program_preflight_failure_remaining_for_test,
        };
        let mut session = session()?;
        session.update_final_raster(1, &payload(64)?)?;
        let transparent =
            ProgramFinalRasterInputV1::try_new(1, 1, 1, [0; 32], 1, &[[0; 4]], 2, &[[0; 4]])?;
        let rejected = ProgramFinalRasterInputV1::try_new(
            1,
            1,
            1,
            [0; 32],
            1,
            &[[0; 4]],
            2,
            &[[0, 199, 64, 255]],
        )?;
        let next = payload(65)?;
        {
            let _failure = fail_program_preflight_reservation_for_test(0);
            session.update_final_raster(2, &transparent)?;
            assert_eq!(
                session.final_raster_refusal(),
                Some(ProgramFinalRasterErrorV1::TransparentFinalPixel { pixel_index: 0 })
            );
            assert_eq!(program_preflight_failure_remaining_for_test(), Some(0));
            session.update_final_raster(3, &rejected)?;
            assert!(matches!(
                session.final_raster_refusal(),
                Some(ProgramFinalRasterErrorV1::ConventionRejected { pixel_index: 0, .. })
            ));
            assert_eq!(program_preflight_failure_remaining_for_test(), Some(0));
            assert!(matches!(
                session.update_final_raster(4, &next),
                Err(ProgramFinalRasterErrorV1::Runtime(
                    ProgramRuntimeErrorV1::ResourceExhausted
                ))
            ));
            assert_eq!(program_preflight_failure_remaining_for_test(), None);
            assert!(matches!(
                session.final_raster_refusal(),
                Some(ProgramFinalRasterErrorV1::ConventionRejected { pixel_index: 0, .. })
            ));
        }
        session.update_final_raster(4, &next)?;
        assert_eq!(session.final_raster_refusal(), None);
        assert_eq!(
            session
                .current_final_raster(session.content_identity(), 17, 4, [0; 32])?
                .pixels()
                .collect::<Vec<_>>(),
            vec![[65; 3]]
        );
        Ok(())
    }

    #[test]
    fn retired_final_raster_success_owners_release_all_three_observation_slots()
    -> Result<(), ProgramFinalRasterErrorV1> {
        let mut session = session()?;
        let ready = payload(64)?;
        let transparent =
            ProgramFinalRasterInputV1::try_new(1, 1, 1, [0; 32], 1, &[[0; 4]], 2, &[[0; 4]])?;
        // Failed удерживает cause + previous; следующий Ready требует третьего
        // слота. Повтор цикла обнаруживает pinned lease через публичный update.
        for (revision, refused) in (1_u64..).zip([false, true, false, false, true, false]) {
            session.update_final_raster(revision, if refused { &transparent } else { &ready })?;
            if refused {
                assert_eq!(
                    session.final_raster_refusal(),
                    Some(ProgramFinalRasterErrorV1::TransparentFinalPixel { pixel_index: 0 })
                );
            } else {
                assert_eq!(
                    session
                        .current_final_raster(session.content_identity(), 17, revision, [0; 32])?
                        .pixels()
                        .collect::<Vec<_>>(),
                    vec![[64; 3]]
                );
            }
        }
        Ok(())
    }

    #[test]
    fn no_partial_authority_combination_can_release_current_output()
    -> Result<(), ProgramFinalRasterErrorV1> {
        let mut session = session()?;
        session.update_final_raster(1, &payload(64)?)?;
        let materialization = session.owner.current_final_raster(
            &session.session,
            session.content_identity(),
            17,
            1,
            [0; 32],
        )?;
        let current = materialization.observation();
        let mut tq_only = AuthorityStateV1::new();
        tq_only
            .admit_program_final_raster_technical_quality(
                materialization,
                current,
                AuthorityExpectedCurrentV1::Vacant,
            )
            .map_err(|_| ProgramFinalRasterErrorV1::TechnicalQuality)?;
        assert_eq!(
            tq_only.evaluate_program_final_raster(materialization, current),
            Err(FinalRasterRefusalV1::CleanConvention)
        );
        let mut cc_only = AuthorityStateV1::new();
        cc_only
            .admit_program_final_raster_clean_convention(
                materialization,
                current,
                AuthorityExpectedCurrentV1::Vacant,
            )
            .map_err(|_| ProgramFinalRasterErrorV1::CleanConvention)?;
        assert_eq!(
            cc_only.evaluate_program_final_raster(materialization, current),
            Err(FinalRasterRefusalV1::TechnicalQuality)
        );
        cc_only
            .admit_program_final_raster_technical_quality(
                materialization,
                current,
                AuthorityExpectedCurrentV1::Vacant,
            )
            .map_err(|_| ProgramFinalRasterErrorV1::TechnicalQuality)?;
        assert!(
            cc_only
                .evaluate_program_final_raster(materialization, current)
                .is_ok()
        );
        Ok(())
    }
    #[test]
    fn final_raster_projections_reject_foreign_equal_content_owner_before_lookup()
    -> Result<(), ProgramFinalRasterErrorV1> {
        let mut current = session()?;
        let foreign = session()?;
        let identity = current.content_identity();
        assert_eq!(foreign.content_identity(), identity);
        assert!(matches!(
            foreign.owner.prepare_field_update(
                &mut current.session,
                2,
                Shared::clone(&payload(64)?.0),
            ),
            Err(crate::program::UpdateErrorV1::OwnerMismatch)
        ));
        assert!(matches!(
            foreign
                .owner
                .current_final_raster(&current.session, identity, 17, 1, [0; 32]),
            Err(FinalRasterRefusalV1::ProgramBinding)
        ));
        assert_eq!(
            foreign.owner.final_raster_refusal(&current.session),
            Some(FinalRasterRefusalV1::ProgramBinding)
        );

        current.update_final_raster(1, &payload(64)?)?;
        assert!(matches!(
            foreign
                .owner
                .current_final_raster(&current.session, identity, 17, 1, [0; 32]),
            Err(FinalRasterRefusalV1::ProgramBinding)
        ));
        assert_eq!(
            foreign.owner.final_raster_refusal(&current.session),
            Some(FinalRasterRefusalV1::ProgramBinding)
        );
        assert_eq!(current.final_raster_refusal(), None);
        assert_eq!(
            current
                .current_final_raster(identity, 17, 1, [0; 32])?
                .pixels()
                .collect::<Vec<_>>(),
            vec![[64; 3]]
        );
        Ok(())
    }

    #[test]
    fn owner_raster_projection_preserves_binding_refusal_and_recovery()
    -> Result<(), ProgramFinalRasterErrorV1> {
        let mut current = session()?;
        let identity = current.content_identity();
        assert!(matches!(
            current.current_final_raster(identity, 17, 1, [0; 32]),
            Err(ProgramFinalRasterErrorV1::Stale)
        ));
        current.update_final_raster(1, &payload(64)?)?;
        let wrong_port = ProgramFinalRasterInputV1::try_new(
            1,
            1,
            1,
            [0; 32],
            9,
            &[[0; 4]],
            2,
            &[[64, 64, 64, 255]],
        )?;
        assert!(matches!(
            current.owner.prepare_field_update(
                &mut current.session,
                2,
                Shared::clone(&wrong_port.0),
            ),
            Err(crate::program::UpdateErrorV1::Unsupported)
        ));
        let mut other_identity = identity;
        other_identity[0] ^= 1;
        for (content, output, revision, context, expected) in [
            (
                other_identity,
                17,
                1,
                [0; 32],
                ProgramFinalRasterErrorV1::ProgramBinding,
            ),
            (
                identity,
                18,
                1,
                [0; 32],
                ProgramFinalRasterErrorV1::OutputBinding,
            ),
            (identity, 17, 0, [0; 32], ProgramFinalRasterErrorV1::Stale),
            (
                identity,
                17,
                1,
                [1; 32],
                ProgramFinalRasterErrorV1::ContextBinding,
            ),
        ] {
            match current.current_final_raster(content, output, revision, context) {
                Err(actual) => assert_eq!(actual, expected),
                Ok(_) => panic!("mismatched current raster binding was accepted"),
            }
        }
        {
            let prepared = current
                .owner
                .prepare_update(
                    &mut current.session,
                    crate::program::UpdateV1::Unknown {
                        revision: 2,
                        reason_id: 0,
                    },
                )
                .map_err(|error| {
                    ProgramFinalRasterErrorV1::Runtime(map_runtime_update_error(error))
                })?;
            let _evidence = prepared.commit();
        }
        assert_eq!(current.final_raster_refusal(), None);
        assert!(matches!(
            current.current_final_raster(identity, 17, 1, [0; 32]),
            Err(ProgramFinalRasterErrorV1::Stale)
        ));
        current.update_final_raster(3, &payload(65)?)?;
        let transparent =
            ProgramFinalRasterInputV1::try_new(1, 1, 1, [0; 32], 1, &[[0; 4]], 2, &[[0; 4]])?;
        current.update_final_raster(4, &transparent)?;
        assert_eq!(
            current.final_raster_refusal(),
            Some(ProgramFinalRasterErrorV1::TransparentFinalPixel { pixel_index: 0 })
        );
        assert!(matches!(
            current.current_final_raster(identity, 17, 3, [0; 32]),
            Err(ProgramFinalRasterErrorV1::Stale)
        ));
        current.update_final_raster(5, &payload(66)?)?;
        assert_eq!(current.final_raster_refusal(), None);
        assert_eq!(
            current
                .current_final_raster(identity, 17, 5, [0; 32])?
                .pixels()
                .collect::<Vec<_>>(),
            vec![[66; 3]]
        );
        Ok(())
    }

    #[test]
    fn dropping_prepared_field_transition_preserves_exact_previous_commit()
    -> Result<(), ProgramFinalRasterErrorV1> {
        let mut session = session()?;
        session.update_final_raster(1, &payload(64)?)?;
        let next = payload(65)?;
        {
            let prepared = session
                .owner
                .prepare_field_update(&mut session.session, 2, Shared::clone(&next.0))
                .map_err(|error| {
                    ProgramFinalRasterErrorV1::Runtime(map_runtime_update_error(error))
                })?;
            drop(prepared);
        }
        {
            let old = session.current_final_raster(session.content_identity(), 17, 1, [0; 32])?;
            assert_eq!(old.pixels().collect::<Vec<_>>(), vec![[64; 3]]);
        }
        session.update_final_raster(2, &next)?;
        assert_eq!(
            session
                .current_final_raster(session.content_identity(), 17, 2, [0; 32])?
                .pixels()
                .collect::<Vec<_>>(),
            vec![[65; 3]]
        );
        Ok(())
    }
}
