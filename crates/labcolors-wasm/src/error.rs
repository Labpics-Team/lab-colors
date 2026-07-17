//! Structured, matchable errors for the binding boundary.
//!
//! Inside Rust, errors are `thiserror` enums callers can match on. At the JS
//! boundary, whole-call failures become ordinary `Error` objects whose message
//! has the stable `"<code>: <message>"` form; there is no separate JS `code`
//! свойство. Верхний адаптер бросает без разматывания Rust-паники.
//! Пост-префлайтовые rejected/unsupported/internal исходы набора — контрактный
//! дрейф: мапятся в `internal_error` и никогда не становятся данными роли.

use thiserror::Error;

fn expected_wcag22_criterion_keys() -> &'static str {
    labcolors_core::wcag22::Wcag22CriterionV1::WIRE_KEY_MENU
}

/// A reason a binding call could not produce a result.
///
/// Допущенных пер-ролевых `unreachable | unresolved` здесь нет: это успешные
/// данные роли (см. [`crate::dto`]). Этот enum — про отказ вызова ЦЕЛИКОМ.
#[derive(Error, Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum BindingError {
    /// The background hex string was not a valid `#RRGGBB` colour. Carries the
    /// core's own parse reason so the caller learns exactly what was wrong.
    #[error("invalid background colour: {reason}")]
    InvalidBackground {
        /// The reason the core's hex parser rejected the input.
        reason: String,
    },

    /// A non-background colour argument was not valid encoded sRGB hex.
    #[error("invalid colour: {reason}")]
    InvalidColor {
        /// The core parse/domain reason.
        reason: String,
    },

    /// The config JSON was rejected: parse error, unknown menu item, or a
    /// core validation/compile error (the full preflight message is carried).
    #[error("invalid config: {reason}")]
    InvalidConfig {
        /// The parse/validation reason, verbatim.
        reason: String,
    },

    /// `resolve_theme` was called before any config was loaded. The engine is
    /// agnostic (ADR-0001 PR-c): it carries no built-in design system, so a
    /// resolve has nothing to emit until `load_config` supplies one. Honest,
    /// matchable failure — never a panic and never a silent built-in default.
    #[error("no config loaded: call load_config before resolve_theme or recheck")]
    ConfigRequired,

    /// A core-generated value violated an internal postcondition or the adapter
    /// could not represent a known/forward core variant without losing meaning.
    /// Включает пост-префлайтовые rejection/unsupported исходы набора,
    /// отказы проекции/oklch-сериализации и stable-Glow recheck-отказы
    /// уже после валидации публичных входов. Никогда не вина клиента.
    #[error("internal error: {reason}")]
    Internal {
        /// The internal postcondition, projection, or contract mismatch.
        reason: String,
    },

    /// The theme key is absent from the loaded config's `themes` dictionary
    /// (в частности, ЛЮБОЙ ключ при пустом словаре). Словарь тем принадлежит
    /// клиенту; встроенных имён у движка нет.
    #[error("unknown theme: '{requested}' (not declared in the loaded config's themes dictionary)")]
    UnknownTheme {
        /// The unrecognised theme key the caller passed.
        requested: String,
    },

    /// WCAG 2.2 criterion transport is outside the closed public menu.
    #[error(
        "unknown WCAG22 criterion: '{requested}' (expected {expected})",
        expected = expected_wcag22_criterion_keys()
    )]
    UnknownWcag22Criterion {
        /// Unrecognised criterion key.
        requested: String,
    },
}

impl BindingError {
    /// The stable, machine-readable code for this error — the contract a JS
    /// caller switches on. These never change without a versioned migration.
    pub fn code(&self) -> &'static str {
        match self {
            BindingError::InvalidBackground { .. } => "invalid_background",
            BindingError::InvalidColor { .. } => "invalid_color",
            BindingError::InvalidConfig { .. } => "invalid_config",
            BindingError::ConfigRequired => "config_required",
            BindingError::UnknownTheme { .. } => "unknown_theme",
            BindingError::UnknownWcag22Criterion { .. } => "unknown_wcag22_criterion",
            BindingError::Internal { .. } => "internal_error",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_stable_and_distinct() {
        let errors = [
            BindingError::InvalidBackground { reason: "x".into() },
            BindingError::InvalidColor { reason: "x".into() },
            BindingError::InvalidConfig { reason: "x".into() },
            BindingError::ConfigRequired,
            BindingError::UnknownTheme {
                requested: "x".into(),
            },
            BindingError::UnknownWcag22Criterion {
                requested: "x".into(),
            },
            BindingError::Internal { reason: "x".into() },
        ];
        let codes: Vec<_> = errors.iter().map(BindingError::code).collect();
        assert_eq!(
            codes,
            [
                "invalid_background",
                "invalid_color",
                "invalid_config",
                "config_required",
                "unknown_theme",
                "unknown_wcag22_criterion",
                "internal_error"
            ]
        );
        // Distinctness, asserted directly so the test earns its name: a future
        // variant must not reuse an existing code.
        let unique: std::collections::HashSet<_> = codes.iter().collect();
        assert_eq!(unique.len(), codes.len(), "error codes must be distinct");
    }

    #[test]
    fn unknown_wcag22_criterion_lists_the_core_wire_menu() {
        let requested = "not-a-criterion";
        let expected = expected_wcag22_criterion_keys();
        let error = BindingError::UnknownWcag22Criterion {
            requested: requested.into(),
        };
        assert_eq!(
            error.to_string(),
            format!("unknown WCAG22 criterion: '{requested}' (expected {expected})")
        );
    }
}
