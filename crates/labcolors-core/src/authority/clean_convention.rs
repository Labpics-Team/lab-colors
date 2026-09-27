#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "CC остаётся внутренним владельцем до EVAL; r16 запрещает экспорт через транспорт"
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
    ClosedRejectedBlueIntervalV1, EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1,
    ExactNominalSrgb8CleanSetDecisionV1, ExactNominalSrgb8CleanSetV1,
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
    include_bytes!("../../contracts/clean-set-srgb8-v1/receipt-v1.sha256");

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
        if release != EXACT_NOMINAL_SRGB8_CLEAN_SET_RELEASE_SHA256_V1 {
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
            ExactNominalSrgb8CleanSetV1.classify(composite)
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
