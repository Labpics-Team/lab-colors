//! Совместная оценка одной текущей точки без изменения исходного AUTH.
//!
//! Обе ветви повторно проверяют один attachment. Их свежие описания должны
//! совпасть с установленными полномочиями. Успех удерживает заимствования
//! источников; сохранённые отдельно байты не получают такое право.
//!
//! Внешний код не конструирует внутренний профиль или результат:
//! ```compile_fail
//! use labcolors_core::authority::evaluation::PointQualityProfileV1;
//! ```

use core::marker::PhantomData;

use super::clean_convention::{CleanConventionErrorV1, CleanConventionSelectionV1};
use super::technical_quality::TechnicalQualityAdmissionErrorV1;
use super::{
    AuthorityDescriptorV1, AuthorityExpectedCurrentV1, AuthorityIdV1, AuthorityRequireErrorV1,
    AuthorityStateV1,
};
use crate::program_wire::{
    AttachedMaterializationAuthorityV1, ProgramAttachmentV1, ProgramPointSinkHostV1,
};
use crate::sha256::Hasher;

/// Явный нечеловеческий профиль с двумя обязательными ветвями: TQ и CC.
/// Выпуск конвенции уже разобран единственным владельцем CC. Нет Default,
/// произвольного списка обязательств или конструктора из контрольной суммы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointQualityProfileV1 {
    convention: CleanConventionSelectionV1,
}

impl PointQualityProfileV1 {
    pub(crate) const fn declared_point(convention: CleanConventionSelectionV1) -> Self {
        Self { convention }
    }

    pub(crate) const fn convention(self) -> CleanConventionSelectionV1 {
        self.convention
    }

    /// Идентичность закона профиля, независимая от текущего RGB и ревизии.
    /// Строка выпуска фиксирует TQ текущей source-over точки и обязательный CC.
    pub(crate) fn identity(self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        hasher.update(b"labcolors.quality-profile.declared-modeled-srgb8-point.v1\0");
        hasher.update(&self.convention.release());
        hasher.update(&[
            self.convention.scope() as u8,
            self.convention.admission() as u8,
        ]);
        *hasher.finalize().as_bytes()
    }
}

/// Этот профиль не запрашивает человеческое доказательство.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HumanEvidenceEvaluationV1 {
    NotRequested,
}

/// Ошибка сохраняет ветвь и точную причину. Частичный успех не выдаётся.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PointEvaluationErrorV1 {
    ProfileRequired,
    Technical(TechnicalQualityAdmissionErrorV1),
    Convention(CleanConventionErrorV1),
    RequiredAuthority {
        authority: AuthorityIdV1,
        cause: AuthorityRequireErrorV1,
    },
    MissingProducedAuthority(AuthorityIdV1),
}

/// Результат для следующего потребителя, пока оба исходника неизменны.
/// Закрытые поля и lifetime не позволяют собрать успех из независимых квитанций
/// либо продолжить использовать его после изменения AUTH или attachment.
#[derive(Debug, PartialEq)]
pub(crate) struct CurrentPointEvaluationV1<'a> {
    profile: PointQualityProfileV1,
    materialization: AttachedMaterializationAuthorityV1,
    technical: AuthorityDescriptorV1,
    convention: AuthorityDescriptorV1,
    sources: PhantomData<&'a ()>,
}

impl CurrentPointEvaluationV1<'_> {
    pub(crate) const fn profile(&self) -> PointQualityProfileV1 {
        self.profile
    }
    pub(crate) const fn materialization(&self) -> AttachedMaterializationAuthorityV1 {
        self.materialization
    }
    pub(crate) const fn technical(&self) -> AuthorityDescriptorV1 {
        self.technical
    }
    pub(crate) const fn convention(&self) -> AuthorityDescriptorV1 {
        self.convention
    }
    /// Полная идентичность предмета через тот же кодировщик, что у TQ и CC.
    pub(crate) fn subject_identity(&self) -> [u8; 32] {
        let mut hasher = Hasher::new();
        super::hash_modeled_point_identity(self.materialization, &mut hasher);
        *hasher.finalize().as_bytes()
    }

    pub(crate) const fn human_evidence(&self) -> HumanEvidenceEvaluationV1 {
        HumanEvidenceEvaluationV1::NotRequested
    }
}

impl AuthorityStateV1 {
    /// Проверяет текущую точку и обе установленные ветви, не исправляя их молча.
    /// Один lifetime удерживает AUTH и attachment до последнего использования
    /// результата. Точки входа владельцев не вызывают host и не меняют attachment.
    pub(crate) fn evaluate_declared_modeled_point<'a, H: ProgramPointSinkHostV1>(
        &'a self,
        attachment: &'a ProgramAttachmentV1<H>,
        profile: Option<PointQualityProfileV1>,
    ) -> Result<CurrentPointEvaluationV1<'a>, PointEvaluationErrorV1> {
        let profile = profile.ok_or(PointEvaluationErrorV1::ProfileRequired)?;
        // Только локальная фиксированная область для существующих admission API.
        // Она не публикуется и не заменяет установленные ветви исходного AUTH.
        let mut fresh = Self::new();
        fresh
            .admit_modeled_point_technical_quality(attachment, AuthorityExpectedCurrentV1::Vacant)
            .map_err(PointEvaluationErrorV1::Technical)?;
        let checked = fresh
            .admit_modeled_point_clean_convention(
                attachment,
                Some(profile.convention()),
                AuthorityExpectedCurrentV1::Vacant,
            )
            .map_err(PointEvaluationErrorV1::Convention)?;
        let technical = fresh.read(AuthorityIdV1::TechnicalQuality).ok_or(
            PointEvaluationErrorV1::MissingProducedAuthority(AuthorityIdV1::TechnicalQuality),
        )?;
        let convention = checked.descriptor();
        self.require(technical)
            .map_err(|cause| PointEvaluationErrorV1::RequiredAuthority {
                authority: AuthorityIdV1::TechnicalQuality,
                cause,
            })?;
        self.require(convention)
            .map_err(|cause| PointEvaluationErrorV1::RequiredAuthority {
                authority: AuthorityIdV1::CleanConvention,
                cause,
            })?;
        Ok(CurrentPointEvaluationV1 {
            profile,
            materialization: checked.materialization(),
            technical,
            convention,
            sources: PhantomData,
        })
    }
}

#[cfg(test)]
#[path = "evaluation_tests.rs"]
mod tests;
