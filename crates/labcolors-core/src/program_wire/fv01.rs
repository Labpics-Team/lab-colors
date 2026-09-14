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

/// Один exact physical-case результата attached materialization.
///
/// `composite` не вычисляется из Paint повторно: production mint получает его
/// только из `ExactFinalOwnedPointDomainV1::Singleton`, доказанного causal
/// replay той же revision и того же presentation root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachedMaterializationCaseV1 {
    case_index: usize,
    composite: Srgb8,
}

impl AttachedMaterializationCaseV1 {
    /// Канонический индекс physical case внутри revision-bound observation.
    #[must_use]
    pub const fn case_index(self) -> usize {
        self.case_index
    }

    /// Exact final-owned composite этого physical case.
    #[must_use]
    pub const fn composite(self) -> Srgb8 {
        self.composite
    }
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
    /// Для ожидаемого physical case нет exact counterfactual replay proof.
    MissingExactPointAbsenceProof,
    /// Exact replay доказал отсутствие final-owned вклада для physical case.
    NoFinalOwnedPointContribution,
    /// Causal proof относится к другому presentation root/target.
    CausalPresentationMismatch,
}

/// Внутренний owned proof одного physical case.
///
/// Хранится как typed causal-domain, а не только как RGB-проекция, чтобы
/// публичный capability не переживал потерю происхождения данных.
pub(super) struct AttachedMaterializationCaseProofV1 {
    pub(super) case_index: usize,
    pub(super) domain: crate::appearance::ExactFinalOwnedPointDomainV1,
}

/// Provisional pre-1.0 authority одной установленной point-materialization.
///
/// Тип непрозрачен: у него намеренно нет публичного конструктора. Его поля
/// сохраняют typed Core proofs, а не повторно сериализованные DTO. Поэтому
/// detached snapshot, Paint, sRGB8 и CSS-строка структурно не могут стать
/// входом mint-а. Production mint добавляется только на `Ready` attachment
/// commit seam и обязан валидировать revision, identity, owner generation и
/// sink binding epoch до создания значения этого типа.
pub struct AttachedMaterializationAuthorityV1 {
    pub(super) content_identity: crate::program::ContentIdentityV9,
    pub(super) published_revision: u64,
    // `PointSinkStampV1` уже включает typed `PointSinkBindingEpochV1`.
    // Отдельное числовое поле здесь создало бы второй источник истины.
    pub(super) sink_stamp: crate::program::attachment::PointSinkStampV1,
    pub(super) presentation_root: crate::program::PresentationRootIdV1,
    pub(super) terminal_occurrence: crate::program::OccurrenceIdV1,
    pub(super) appearance_context: crate::program::AppearanceContextV1,
    pub(super) cases: Box<[AttachedMaterializationCaseProofV1]>,
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

    /// Количество exact physical-case proofs, связанных с authority.
    #[must_use]
    pub const fn case_count(&self) -> usize {
        self.cases.len()
    }

    /// Exact causal projections в каноническом порядке physical cases.
    ///
    /// Production mint допускает только `Singleton`, поэтому `Empty` здесь
    /// означает нарушение внутреннего конструктора, а не пользовательский
    /// отказ. Пользовательские отказы происходят до создания authority.
    pub fn cases(&self) -> impl ExactSizeIterator<Item = AttachedMaterializationCaseV1> + '_ {
        self.cases.iter().map(|case| {
            let composite = match case.domain {
                crate::appearance::ExactFinalOwnedPointDomainV1::Singleton { visible } => {
                    Srgb8::new(visible)
                }
                crate::appearance::ExactFinalOwnedPointDomainV1::Empty => {
                    unreachable!("authority cannot retain an empty final-owned point domain")
                }
            };
            AttachedMaterializationCaseV1 {
                case_index: case.case_index,
                composite,
            }
        })
    }

    /// Renderer provenance первого FV-среза.
    #[must_use]
    pub const fn renderer_provenance(&self) -> RendererProvenanceV1 {
        self.renderer_provenance
    }
}
