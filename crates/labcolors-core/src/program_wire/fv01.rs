use crate::Srgb8;

/// Provenance renderer-а, связанная с attached materialization authority.
///
/// FV-01 намеренно не утверждает наблюдение или измерение renderer-а. Такие
/// состояния требуют отдельного AUTH/TQ-контракта и не могут быть выведены из
/// факта успешной записи в sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RendererProvenanceV1 {
    /// Материализация установлена, но renderer не наблюдался как authority.
    Unverified,
}

/// Публичная проекция зарегистрированного surround-профиля appearance context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AppearanceSurroundV1 {
    /// Среднее окружение.
    Average,
    /// Приглушённое окружение.
    Dim,
    /// Тёмное окружение.
    Dark,
}

/// Типизированный отказ mint-а attached materialization authority.
///
/// Варианты намеренно не схлопываются в `None`: отсутствие authority всегда
/// должно оставлять машинно различимую причину.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttachedMaterializationAuthorityErrorV1 {
    /// Запрошенный слой не является терминальным presentation root.
    NonTerminalRoot,
    /// Root был потреблён downstream и потому не является терминальным.
    RootConsumedDownstream,
    /// Published revision не совпадает с revision доказавшего её сертификата.
    PublishedRevisionMismatch,
    /// Сертификат относится к другому каноническому содержанию Program.
    ProgramIdentityMismatch,
    /// Capability относится к другой owner-generation.
    ForeignOwnerGeneration,
    /// Sink stamp относится к другой binding epoch.
    ForeignBindingEpoch,
    /// Exact counterfactual-domain для point contribution отсутствует.
    MissingExactPointAbsenceProof,
}

/// Provisional pre-1.0 authority одной установленной point-materialization.
///
/// Тип непрозрачен: у него намеренно нет публичного конструктора. Его поля
/// сохраняют typed Core proofs, а не повторно сериализованные DTO. Поэтому
/// detached snapshot, Paint, sRGB8 и CSS-строка структурно не могут стать
/// входом mint-а. Production mint добавляется только на `Ready` attachment
/// commit seam и обязан валидировать revision, identity и sink binding epoch
/// до создания значения этого типа.
pub struct AttachedMaterializationAuthorityV1 {
    pub(super) content_identity: crate::program::ContentIdentityV9,
    pub(super) published_revision: u64,
    pub(super) sink_stamp: crate::program::attachment::PointSinkStampV1,
    pub(super) sink_binding_epoch: u64,
    pub(super) presentation_root: crate::program::PresentationRootIdV1,
    pub(super) terminal_occurrence: crate::program::OccurrenceIdV1,
    pub(super) appearance_context: crate::program::AppearanceContextV1,
    pub(super) point_domain: crate::appearance::ExactFinalOwnedPointDomainV1,
    pub(super) paint: crate::appearance::EncodedPointPaintV1,
    pub(super) renderer_provenance: RendererProvenanceV1,
}

impl AttachedMaterializationAuthorityV1 {
    /// Каноническая content identity Program, из которой получен capability.
    #[must_use]
    pub const fn content_identity(&self) -> [u8; 32] {
        *self.content_identity.as_bytes()
    }

    /// Revision, атомарно опубликованная attachment-ом.
    #[must_use]
    pub const fn published_revision(&self) -> u64 {
        self.published_revision
    }

    /// Последовательность установленного sink stamp.
    #[must_use]
    pub const fn sink_sequence(&self) -> u64 {
        self.sink_stamp.sequence()
    }

    /// Непрозрачная binding epoch sink-а, проверенная при mint-е.
    #[must_use]
    pub const fn sink_binding_epoch(&self) -> u64 {
        self.sink_binding_epoch
    }

    /// Скомпилированный terminal presentation root.
    #[must_use]
    pub const fn presentation_root(&self) -> u32 {
        self.presentation_root.value()
    }

    /// Terminal occurrence этого presentation root.
    #[must_use]
    pub const fn terminal_occurrence(&self) -> u32 {
        self.terminal_occurrence.value()
    }

    /// Допущенная адаптирующая яркость appearance context в кд/м².
    #[must_use]
    pub fn adapting_luminance_cd_m2(&self) -> f64 {
        self.appearance_context.adapting_luminance_cd_m2()
    }

    /// Допущенное отношение фоновой яркости `Y_b/Y_w` appearance context.
    #[must_use]
    pub fn background_luminance_ratio_yb_yw(&self) -> f64 {
        self.appearance_context.background_luminance_ratio_yb_yw()
    }

    /// Зарегистрированный surround-профиль appearance context.
    #[must_use]
    pub const fn appearance_surround(&self) -> AppearanceSurroundV1 {
        match self.appearance_context.surround() {
            crate::program::SurroundV1::Average => AppearanceSurroundV1::Average,
            crate::program::SurroundV1::Dim => AppearanceSurroundV1::Dim,
            crate::program::SurroundV1::Dark => AppearanceSurroundV1::Dark,
        }
    }

    /// Encoded-sRGB8 source установленного Paint до backdrop-композиции.
    #[must_use]
    pub const fn source(&self) -> Srgb8 {
        self.paint.value().source()
    }

    /// Straight alpha установленного Paint.
    #[must_use]
    pub fn opacity(&self) -> f64 {
        self.paint.value().opacity().value()
    }

    /// Exact final-owned composite, доказанный counterfactual replay.
    ///
    /// Конструктор authority обязан отвергать `Empty`, поэтому эта проекция
    /// total для любого публично достижимого значения capability.
    #[must_use]
    pub fn composite(&self) -> Srgb8 {
        match self.point_domain {
            crate::appearance::ExactFinalOwnedPointDomainV1::Singleton { visible } => {
                Srgb8::new(visible)
            }
            crate::appearance::ExactFinalOwnedPointDomainV1::Empty => {
                unreachable!("authority cannot be minted without exact point contribution")
            }
        }
    }

    /// Renderer provenance первого FV-среза.
    #[must_use]
    pub const fn renderer_provenance(&self) -> RendererProvenanceV1 {
        self.renderer_provenance
    }
}
