//! Итог уже выполненной проверки объявленного cleanliness-профиля.
//!
//! Счётчики описывают покрытие case × constraint, не чистоту в баллах, долю
//! пикселей или человеческое предпочтение. Отсутствие оценки не равно pass.

/// Соответствие текущему объявленному профилю, отдельно от Ready-состояния Program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramCleanlinessStatusV1 {
    /// Program не объявлял поддерживаемого cleanliness-профиля.
    NotAssessed,
    /// Каждая объявленная клетка проверена и соответствует профилю.
    Conforms,
    /// Хотя бы одна проверенная клетка нарушает профиль.
    Violates,
    /// Для части области нет оцениваемого конечного вклада.
    Unavailable,
}

/// Неизменяемая сводка сохранённых оценок финальных declared point domains.
///
/// Поддержан только versioned nominal sRGB8-профиль. Соответствие ему не
/// доказывает Cleaner/NotDirtier между различными цветами или качество целого
/// пространственного поля. Разные принятые цвета не объявляются равными.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProgramCleanlinessAssessmentV1 {
    accepted: usize,
    rejected: usize,
    unavailable: usize,
}

impl ProgramCleanlinessAssessmentV1 {
    /// Идентификатор единственного поддержанного сейчас профиля.
    pub const PROFILE_ID: &'static str = "nominal-srgb8-v2";
    /// Основание этого профиля; человеческая оценка из него не следует.
    pub const BASIS: &'static str = "declared-convention";
    /// Область сводки не расширяется до всего изображения или интерфейса.
    pub const SCOPE: &'static str = "declared-final-points";

    /// Итог относится ко всем объявленным клеткам, включая report-only.
    /// Известное нарушение сохраняется и при неполном покрытии других клеток.
    #[must_use]
    pub const fn status(self) -> ProgramCleanlinessStatusV1 {
        if self.rejected != 0 {
            ProgramCleanlinessStatusV1::Violates
        } else if self.unavailable != 0 {
            ProgramCleanlinessStatusV1::Unavailable
        } else if self.accepted != 0 {
            ProgramCleanlinessStatusV1::Conforms
        } else {
            ProgramCleanlinessStatusV1::NotAssessed
        }
    }

    /// Принятые клетки case × constraint; это счётчик проверок, не score.
    #[must_use]
    pub const fn accepted(self) -> usize {
        self.accepted
    }

    /// Клетки с сохранённым отрицательным вердиктом классификатора.
    #[must_use]
    pub const fn rejected(self) -> usize {
        self.rejected
    }

    /// Клетки без непустого конечного owned domain.
    #[must_use]
    pub const fn unavailable(self) -> usize {
        self.unavailable
    }

    pub(crate) fn accept(&mut self) {
        self.accepted += 1;
    }

    pub(crate) fn reject(&mut self) {
        self.rejected += 1;
    }

    pub(crate) fn omit_domain(&mut self) {
        self.unavailable += 1;
    }
}
