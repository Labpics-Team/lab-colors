//! Структурированные отказы терминальной C7c WASM-границы.

use core::fmt;

/// Ошибка browser boundary. Код — стабильная закрытая проекция; message несёт
/// диагностику без правдоподобного fallback-значения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingError {
    InvalidColor { reason: String },
    UnknownWcag22Criterion { requested: String },
    Internal { reason: String },
}

impl BindingError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidColor { .. } => "invalid_color",
            Self::UnknownWcag22Criterion { .. } => "unknown_wcag22_criterion",
            Self::Internal { .. } => "internal_error",
        }
    }
}

impl fmt::Display for BindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let reason = match self {
            Self::InvalidColor { reason } | Self::Internal { reason } => reason.as_str(),
            Self::UnknownWcag22Criterion { requested } => requested.as_str(),
        };
        write!(f, "{}: {reason}", self.code())
    }
}

impl std::error::Error for BindingError {}
