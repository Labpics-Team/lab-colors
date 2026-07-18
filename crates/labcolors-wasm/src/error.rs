//! Structured, matchable errors for the binding boundary.
//!
//! Inside Rust, errors are `thiserror` enums callers can match on. At the JS
//! boundary, whole-call failures become ordinary `Error` objects whose message
//! has the stable `"<code>: <message>"` form. `output_conflict` дополнительно
//! несёт typed JS-поля `name`, `code` и непустой aggregate `conflicts`; прочие
//! ошибки не получают фиктивный payload. Верхний адаптер бросает без
//! разматывания Rust-паники.
//! Пост-префлайтовые rejected/unsupported/internal исходы набора — контрактный
//! дрейф: мапятся в `internal_error` и никогда не становятся данными роли.

use core::fmt;

use thiserror::Error;

fn expected_wcag22_criterion_keys() -> &'static str {
    labcolors_core::wcag22::Wcag22CriterionV1::WIRE_KEY_MENU
}

/// Один ordinary-Unreachable контракт роли в whole-resolve aggregate.
///
/// Категория отсутствует намеренно: сам enclosing [`BindingError::OutputConflict`]
/// означает ровно `Unreachable`, поэтому незаконный `Unresolved` в payload
/// непредставим. Кандидаты и CSS здесь также отсутствуют: ошибка не является
/// частичным снимком.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputConflict {
    role: String,
    code: &'static str,
    message: String,
}

impl OutputConflict {
    pub(crate) fn new(role: String, code: &'static str, message: String) -> Self {
        Self {
            role,
            code,
            message,
        }
    }

    pub(crate) fn role(&self) -> &str {
        &self.role
    }

    pub(crate) const fn code(&self) -> &'static str {
        self.code
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

/// Непустой ordered aggregate конфликтов.
///
/// Представление `first + rest` исключает пустой `output_conflict` типом, а
/// итератор сохраняет declaration order, полученный от Core.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputConflicts {
    first: OutputConflict,
    rest: Vec<OutputConflict>,
}

impl OutputConflicts {
    pub(crate) fn new(first: OutputConflict, rest: Vec<OutputConflict>) -> Self {
        Self { first, rest }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &OutputConflict> {
        core::iter::once(&self.first).chain(self.rest.iter())
    }
}

impl fmt::Display for OutputConflicts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, conflict) in self.iter().enumerate() {
            if index > 0 {
                f.write_str(", ")?;
            }
            write!(
                f,
                "'{}' ({})",
                conflict.role().escape_debug(),
                conflict.code()
            )?;
        }
        Ok(())
    }
}

/// A reason a binding call could not produce a result.
///
/// `unresolved` остаётся успешным локальным исходом роли (см. [`crate::dto`]);
/// ordinary `unreachable` означает, что полный снимок не существует, и
/// становится [`Self::OutputConflict`] всего вызова.
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
    /// agnostic (ADR-0001): it carries no built-in design system, so a
    /// resolve has nothing to emit until `load_config` supplies one. Honest,
    /// matchable failure — never a panic and never a silent built-in default.
    #[error("no config loaded: call load_config before resolve_theme or recheck")]
    ConfigRequired,

    /// Хотя бы одна обычная роль доказанно недостижима, поэтому полный output
    /// snapshot не существует. Aggregate непуст и следует declaration order;
    /// aliases не дублируют конфликт цели.
    #[error("unreachable output roles: {conflicts}")]
    OutputConflict {
        /// Непустой список client-owned role IDs и исходной диагностики Core.
        conflicts: OutputConflicts,
    },

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
            BindingError::OutputConflict { .. } => "output_conflict",
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
            BindingError::OutputConflict {
                conflicts: OutputConflicts::new(
                    OutputConflict {
                        role: "opaque-client-id".into(),
                        code: "exceeds_range",
                        message: "physical limit".into(),
                    },
                    Vec::new(),
                ),
            },
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
                "output_conflict",
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
    fn output_conflicts_are_non_empty_by_construction_and_preserve_order() {
        let conflict = |role: &str, target: f64| OutputConflict {
            role: role.into(),
            code: "exceeds_range",
            message: format!("target Lc {target:.2} exceeds the physical range"),
        };
        let conflicts = OutputConflicts::new(
            conflict("conflict-z", 50.0),
            vec![conflict("conflict-m", 51.0), conflict("conflict-a", 52.0)],
        );

        let roles: Vec<_> = conflicts.iter().map(OutputConflict::role).collect();
        assert_eq!(roles, ["conflict-z", "conflict-m", "conflict-a"]);
        assert_eq!(conflicts.iter().count(), 3);
    }

    #[test]
    fn output_conflict_display_escapes_client_role_without_changing_payload() {
        let raw = "роль\n\r\t\u{0001}\"\\";
        let conflicts = OutputConflicts::new(
            OutputConflict::new(
                raw.to_string(),
                "exceeds_range",
                "physical limit".to_string(),
            ),
            Vec::new(),
        );

        let rendered = conflicts.to_string();
        assert!(
            !rendered.chars().any(char::is_control),
            "human-readable errors must not admit client-controlled log lines",
        );
        assert!(rendered.contains("\\n"));
        assert!(rendered.contains("роль"));
        assert_eq!(conflicts.iter().next().unwrap().role(), raw);
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
