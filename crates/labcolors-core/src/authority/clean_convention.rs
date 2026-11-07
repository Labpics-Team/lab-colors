#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "CC используется внутренним оценщиком; публичный сертификат имеет отдельный контракт"
    )
)]

//! Допуск текущей конечной точки по явно выбранной объявленной конвенции.
//!
//! Выбор выпуска не является доказательством. Его область и вид допуска
//! проверяются до чтения текущей материализации и настоящего классификатора.
//! Квитанция сохраняет эти ограничения; она не удостоверяет пиксели браузера
//! или человеческое восприятие и не заменяет независимые ветви TQ/HCE.
//!
//! Внешний потребитель не может получить внутренний выбор либо полномочие:
//! ```compile_fail
//! use labcolors_core::authority::clean_convention::CleanConventionSelectionV1;
//! ```

use super::{
    AuthorityAdmissionErrorV1, AuthorityAdmissionOutcomeV1, AuthorityDescriptorV1,
    AuthorityExpectedCurrentV1, AuthorityIdV1, AuthorityOwnerCurrentV1, AuthorityPermitErrorV1,
    AuthorityStateV1, RendererObservationRequirementV1, hash_modeled_point_identity, issue_permit,
};
use crate::Srgb8;
use crate::clean_set::{
    ClosedRejectedBlueIntervalV1, EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2,
    ExactNominalSrgb8CleanSetDecisionV1, ExactNominalSrgb8CleanSetV2,
};
use crate::program_wire::{
    AttachedMaterializationAuthorityV1, ProgramAttachmentV1,
    ProgramMaterializationAuthorityErrorV1, ProgramPhysicalIdentityV1, ProgramPointSinkHostV1,
    ProgramRendererProvenanceV1,
};
use crate::sha256::Hasher;

// Единственный существующий pin технической квитанции. Её полноту и связь
// с классификатором/кодеком проверяет verify_clean_set_receipt.py, не новый реестр.
const TECHNICAL_RECEIPT_PIN: &[u8] =
    include_bytes!("../../contracts/clean-set-srgb8-v2/receipt-v2.sha256");

/// Запрошенная область. Неподдержанные области дают отказ, не точечную замену.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum CleanConventionScopeV1 {
    ModeledSrgb8Point = 1,
    WholeField = 2,
    ObservedPoint = 3,
}

/// Запрошенный вид допуска. Текущий выпуск поддерживает только первый вариант.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum CleanConventionAdmissionKindV1 {
    DeclaredPackagePolicyCandidate = 1,
    ProductionAuto = 2,
    HumanAction = 3,
}

/// Проверенный явный выбор. Нет Default, публичных полей или неявной конвенции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CleanConventionSelectionV1 {
    release: [u8; 32],
    scope: CleanConventionScopeV1,
    admission: CleanConventionAdmissionKindV1,
}

impl CleanConventionSelectionV1 {
    /// Принимает только существующий выпуск, точечную область и объявленную политику.
    pub(crate) fn select(
        release: [u8; 32],
        scope: CleanConventionScopeV1,
        admission: CleanConventionAdmissionKindV1,
    ) -> Result<Self, CleanConventionErrorV1> {
        if release != EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2 {
            return Err(CleanConventionErrorV1::UnsupportedRelease);
        }
        if scope != CleanConventionScopeV1::ModeledSrgb8Point {
            return Err(CleanConventionErrorV1::UnsupportedScope);
        }
        if admission != CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate {
            return Err(CleanConventionErrorV1::UnsupportedAdmission);
        }
        Ok(Self {
            release,
            scope,
            admission,
        })
    }

    pub(crate) fn select_final_raster() -> Self {
        Self {
            release: EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V2,
            scope: CleanConventionScopeV1::WholeField,
            admission: CleanConventionAdmissionKindV1::DeclaredPackagePolicyCandidate,
        }
    }

    pub(crate) const fn release(self) -> [u8; 32] {
        self.release
    }
    pub(crate) const fn scope(self) -> CleanConventionScopeV1 {
        self.scope
    }
    pub(crate) const fn admission(self) -> CleanConventionAdmissionKindV1 {
        self.admission
    }
}

/// Отказ ничего не записывает в AUTH и не изменяет attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CleanConventionErrorV1 {
    SelectionRequired,
    UnsupportedRelease,
    UnsupportedScope,
    UnsupportedAdmission,
    Materialization(ProgramMaterializationAuthorityErrorV1),
    UnsupportedPhysicalIdentity,
    RejectedByConvention {
        composite: Srgb8,
        interval: ClosedRejectedBlueIntervalV1,
    },
    Permit(AuthorityPermitErrorV1),
    Authority(AuthorityAdmissionErrorV1),
}

/// Закрытая квитанция успешного вызова, не автономный или бессрочный допуск.
/// Последующий оценщик должен сопоставить её с текущими attachment и AUTH.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CleanConventionReceiptV1 {
    outcome: AuthorityAdmissionOutcomeV1,
    selection: CleanConventionSelectionV1,
    materialization: AttachedMaterializationAuthorityV1,
    descriptor: AuthorityDescriptorV1,
}

impl CleanConventionReceiptV1 {
    pub(crate) const fn outcome(self) -> AuthorityAdmissionOutcomeV1 {
        self.outcome
    }
    pub(crate) const fn selection(self) -> CleanConventionSelectionV1 {
        self.selection
    }
    pub(crate) const fn materialization(self) -> AttachedMaterializationAuthorityV1 {
        self.materialization
    }
    pub(crate) const fn descriptor(self) -> AuthorityDescriptorV1 {
        self.descriptor
    }
}

/// Создаётся только после классификации конечного результата текущего владельца.
struct CleanConventionProofV1 {
    selection: CleanConventionSelectionV1,
    materialization: AttachedMaterializationAuthorityV1,
}

impl CleanConventionProofV1 {
    fn from_current<H: ProgramPointSinkHostV1>(
        selection: Option<CleanConventionSelectionV1>,
        attachment: &ProgramAttachmentV1<H>,
    ) -> Result<Self, CleanConventionErrorV1> {
        let selection = selection.ok_or(CleanConventionErrorV1::SelectionRequired)?;
        let materialization = attachment
            .current_materialization_authority()
            .map_err(CleanConventionErrorV1::Materialization)?;
        if materialization.physical_identity()
            != Some(ProgramPhysicalIdentityV1::EncodedSrgb8SourceOverV1)
        {
            return Err(CleanConventionErrorV1::UnsupportedPhysicalIdentity);
        }
        let composite = materialization.terminal_composite();
        if let ExactNominalSrgb8CleanSetDecisionV1::Rejected(interval) =
            ExactNominalSrgb8CleanSetV2.classify(composite)
        {
            return Err(CleanConventionErrorV1::RejectedByConvention {
                composite,
                interval,
            });
        }
        Ok(Self {
            selection,
            materialization,
        })
    }

    fn descriptor(&self) -> AuthorityDescriptorV1 {
        // Выпуск и вид допуска не меняются от смены наблюдения.
        let mut release = Hasher::new();
        release.update(b"labcolors.cc.release.v1\0");
        release.update(&self.selection.release);
        release.update(&[self.selection.admission as u8]);
        let release = *release.finalize().as_bytes();

        let mut applicability = Hasher::new();
        applicability.update(b"labcolors.cc.applicability.v1\0");
        applicability.update(&[self.selection.scope as u8]);
        applicability.update(&self.materialization.context().identity_bytes());
        // Только проверенный modeled source-over; наблюдение renderer не заявляется.
        applicability.update(&[1, 0]);
        let applicability = *applicability.finalize().as_bytes();

        let mut provenance = Hasher::new();
        provenance.update(b"labcolors.cc.provenance.v1\0");
        provenance.update(&release);
        provenance.update(&applicability);
        provenance.update(TECHNICAL_RECEIPT_PIN);
        hash_modeled_point_identity(self.materialization, &mut provenance);
        let provenance = *provenance.finalize().as_bytes();

        AuthorityDescriptorV1::from_verified_owner(
            AuthorityIdV1::CleanConvention,
            release,
            applicability,
            provenance,
        )
    }
}

impl AuthorityStateV1 {
    /// Единственный CC-вход: явный выбор, текущая точка, классификация, затем точный CAS.
    /// Сохранённая квитанция, снимок или переданный извне цвет не являются входом.
    pub(crate) fn admit_modeled_point_clean_convention<H: ProgramPointSinkHostV1>(
        &mut self,
        attachment: &ProgramAttachmentV1<H>,
        selection: Option<CleanConventionSelectionV1>,
        expected: AuthorityExpectedCurrentV1,
    ) -> Result<CleanConventionReceiptV1, CleanConventionErrorV1> {
        let proof = CleanConventionProofV1::from_current(selection, attachment)?;
        let descriptor = proof.descriptor();
        let owner_current = AuthorityOwnerCurrentV1::new(descriptor);
        let permit = issue_permit(
            &owner_current,
            descriptor,
            true,
            RendererObservationRequirementV1::NotRequired,
            ProgramRendererProvenanceV1::Unverified,
        )
        .map_err(CleanConventionErrorV1::Permit)?;
        let outcome = self
            .admit(descriptor, expected, permit)
            .map_err(CleanConventionErrorV1::Authority)?;
        Ok(CleanConventionReceiptV1 {
            outcome,
            selection: proof.selection,
            materialization: proof.materialization,
            descriptor,
        })
    }
}

#[cfg(test)]
#[path = "clean_convention_tests.rs"]
mod tests;

#[cfg(kani)]
mod proofs;

impl AuthorityStateV1 {
    /// Вычисляет полный отчёт без AUTH admission. Report-only не выдаёт pass permit.
    /// Все N пикселей участвуют в count; первый witness не заменяет проверку N−1.
    pub(crate) fn assess_program_final_raster_clean_convention(
        materialization: &crate::program_session::finite_field::FinalRasterMaterializationV1,
        current: &crate::observation::RevisionBoundObservationV1,
    ) -> Result<
        crate::field_effect::final_raster::FinalRasterCleanlinessAssessmentV1,
        crate::program_session::finite_field::FinalRasterRefusalV1,
    > {
        use crate::field_effect::final_raster::{
            FinalRasterCleanlinessAssessmentV1, FinalRasterCleanlinessRequirementV1 as C,
            FinalRasterCleanlinessViolationV1,
        };
        use crate::program_session::finite_field::FinalRasterRefusalV1 as E;
        if !materialization.observation().is_same_binding_as(current) {
            return Err(E::Stale);
        }
        let policy = materialization.cleanliness_requirement()?;
        if policy == C::Off {
            return Err(E::CleanConvention);
        }
        let payload = materialization.payload();
        let count =
            usize::try_from(u64::from(payload.extent.width()) * u64::from(payload.extent.height()))
                .map_err(|_| E::ResourceExhausted)?;
        if count != materialization.pixels().len() {
            return Err(E::CleanConvention);
        }
        let mut violation_count = 0_usize;
        let mut first_violation = None;
        for (pixel_index, pixel) in materialization.pixels().iter().enumerate() {
            if pixel.alpha() != 255 {
                return Err(E::Transparent { pixel_index });
            }
            let channels = pixel.channels();
            let composite = Srgb8::new([channels[0], channels[1], channels[2]]);
            if let ExactNominalSrgb8CleanSetDecisionV1::Rejected(interval) =
                ExactNominalSrgb8CleanSetV2.classify(composite)
            {
                violation_count = violation_count.checked_add(1).ok_or(E::ResourceExhausted)?;
                if first_violation.is_none() {
                    first_violation = Some(FinalRasterCleanlinessViolationV1::new(
                        pixel_index,
                        composite.bytes(),
                        interval.endpoints(),
                    ));
                }
            }
        }
        FinalRasterCleanlinessAssessmentV1::assessed(
            policy,
            count,
            violation_count,
            first_violation,
        )
        .map_err(|_| E::CleanConvention)
    }

    /// Полная pointwise принадлежность всех конечных opaque pixels nominal v2.
    /// Точка, среднее, centroid и исходные carrier цвета не могут выдать этот proof.
    pub(crate) fn admit_program_final_raster_clean_convention(
        &mut self,
        materialization: &crate::program_session::finite_field::FinalRasterMaterializationV1,
        current: &crate::observation::RevisionBoundObservationV1,
        expected: AuthorityExpectedCurrentV1,
    ) -> Result<
        AuthorityAdmissionOutcomeV1,
        crate::program_session::finite_field::FinalRasterRefusalV1,
    > {
        use crate::program_session::finite_field::FinalRasterRefusalV1 as E;
        if !materialization.observation().is_same_binding_as(current) {
            return Err(E::Stale);
        }
        use crate::field_effect::final_raster::FinalRasterCleanlinessRequirementV1 as C;
        if materialization.cleanliness_requirement()? != C::MandatoryExactNominalSrgb8V2 {
            return Err(E::CleanConvention);
        }
        let assessment =
            Self::assess_program_final_raster_clean_convention(materialization, current)?;
        if let Some(first) = assessment.first_violation() {
            return Err(E::Convention {
                pixel_index: first.pixel_index(),
                composite: Srgb8::new(first.composite()),
                interval: first.rejected_blue_interval(),
            });
        }
        let payload = materialization.payload();
        let selection = CleanConventionSelectionV1::select_final_raster();
        let mut release = Hasher::new();
        release.update(b"labcolors.cc.final-raster.release.v1\0");
        release.update(&selection.release);
        release.update(&[selection.admission as u8]);
        let release = *release.finalize().as_bytes();
        let mut applicability = Hasher::new();
        applicability.update(b"labcolors.cc.final-raster.applicability.v1\0");
        applicability.update(&[selection.scope as u8]);
        applicability.update(&payload.context);
        applicability.update(&payload.extent.width().to_le_bytes());
        applicability.update(&payload.extent.height().to_le_bytes());
        applicability.update(&[payload.dpr.value()]);
        let applicability = *applicability.finalize().as_bytes();
        let mut provenance = Hasher::new();
        provenance.update(b"labcolors.cc.final-raster.provenance.v1\0");
        provenance.update(&release);
        provenance.update(&applicability);
        provenance.update(TECHNICAL_RECEIPT_PIN);
        provenance.update(&materialization.subject());
        let descriptor = AuthorityDescriptorV1::from_verified_owner(
            AuthorityIdV1::CleanConvention,
            release,
            applicability,
            *provenance.finalize().as_bytes(),
        );
        let owner_current = AuthorityOwnerCurrentV1::new(descriptor);
        let permit = issue_permit(
            &owner_current,
            descriptor,
            true,
            RendererObservationRequirementV1::NotRequired,
            ProgramRendererProvenanceV1::Unverified,
        )
        .map_err(|_| E::CleanConvention)?;
        self.admit(descriptor, expected, permit)
            .map_err(|_| E::CleanConvention)
    }
}
