//! V-06 (V4 Variants/Scopes) type foundation.
//!
//! Resolution topology selectors for variant-aware token evaluation.
//! Staged before C-18 atomic cutover merge; items carry per-item
//! `#[allow(dead_code)]` so the module compiles cleanly under
//! `--all-targets` without unfulfilled lint expectations.

use std::fmt;

/// Validated opaque key identifying a configuration variant.
///
/// Rejects empty strings and the reserved `__` prefix namespace.
/// Cheap-clone (String-backed); not Copy.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct VariantKeyV1(String);

#[allow(dead_code)]
impl VariantKeyV1 {
    /// Validated constructor.
    ///
    /// # Errors
    /// - [`VariantScopeErrorV1::EmptyVariantKey`] if `key` is empty.
    /// - [`VariantScopeErrorV1::ReservedPrefix`] if `key` starts with `__`.
    pub(crate) fn new(key: impl Into<String>) -> Result<Self, VariantScopeErrorV1> {
        let s = key.into();
        if s.is_empty() {
            return Err(VariantScopeErrorV1::EmptyVariantKey);
        }
        if s.starts_with("__") {
            return Err(VariantScopeErrorV1::ReservedPrefix(s));
        }
        Ok(Self(s))
    }

    /// Returns the underlying key as a string slice.
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Resolution topology for token evaluation.
///
/// Determines visibility and intersection rules during constraint resolution.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ResolutionScopeV1 {
    /// Independent resolution per attachment instance. No cross-instance state.
    Attachment,
    /// Joint resolution across all occurrences in program scope.
    /// Shared tokens must satisfy constraint intersection.
    Program,
}

/// Opaque identifier for a conditional case block within a resolution pass.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CaseScopeIdV1(u64);

#[allow(dead_code)]
impl CaseScopeIdV1 {
    /// Constructs a case scope identifier from a raw u64 value.
    pub(crate) fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the underlying raw value.
    pub(crate) fn to_raw(self) -> u64 {
        self.0
    }
}

/// Sealed trait ensuring only legitimate environment selectors can participate
/// in variant selection. Prevents opaque runtime selectors from substituting
/// `EnvironmentProfile`.
#[allow(dead_code)]
pub(crate) trait EnvironmentSelector: private::Sealed + fmt::Debug {
    /// Returns the resolution scope selected by this environment.
    fn select_environment(&self) -> ResolutionScopeV1;
}

mod private {
    pub trait Sealed {}
}

/// Placeholder default environment that always selects independent
/// per-instance (`Attachment`) resolution.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct DefaultEnvironmentV1;

impl private::Sealed for DefaultEnvironmentV1 {}

impl EnvironmentSelector for DefaultEnvironmentV1 {
    fn select_environment(&self) -> ResolutionScopeV1 {
        ResolutionScopeV1::Attachment
    }
}

/// Closed matchable error type for variant/scope validation failures.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VariantScopeErrorV1 {
    /// The variant key string was empty.
    EmptyVariantKey,
    /// The variant key started with the reserved `__` prefix.
    ReservedPrefix(String),
    /// Generic invalid variant key (catch-all for future validation).
    InvalidVariantKey(String),
}

impl fmt::Display for VariantScopeErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyVariantKey => write!(f, "variant key must not be empty"),
            Self::ReservedPrefix(key) => {
                write!(f, "variant key '{key}' uses reserved '__' prefix")
            }
            Self::InvalidVariantKey(key) => {
                write!(f, "invalid variant key: '{key}'")
            }
        }
    }
}

impl std::error::Error for VariantScopeErrorV1 {}

#[cfg(test)]
mod tests {
    use super::*;

    // --- VariantKeyV1 acceptance ---

    #[test]
    fn variant_key_accepts_valid_keys() {
        assert!(VariantKeyV1::new("theme-a").is_ok());
        assert!(VariantKeyV1::new("dark_mode").is_ok());
        assert!(VariantKeyV1::new("a").is_ok());
        assert!(VariantKeyV1::new("print-v2").is_ok());
    }

    #[test]
    fn variant_key_rejects_empty_string() {
        let err = VariantKeyV1::new("").unwrap_err();
        assert_eq!(err, VariantScopeErrorV1::EmptyVariantKey);
    }

    #[test]
    fn variant_key_rejects_reserved_prefix() {
        let err = VariantKeyV1::new("__reserved").unwrap_err();
        assert_eq!(
            err,
            VariantScopeErrorV1::ReservedPrefix("__reserved".to_string())
        );
    }

    #[test]
    fn variant_key_rejects_double_underscore_prefix_variants() {
        assert!(matches!(
            VariantKeyV1::new("__internal").unwrap_err(),
            VariantScopeErrorV1::ReservedPrefix(_)
        ));
        assert!(matches!(
            VariantKeyV1::new("__").unwrap_err(),
            VariantScopeErrorV1::ReservedPrefix(_)
        ));
    }

    #[test]
    fn variant_key_equality_and_hash() {
        use std::collections::HashSet;
        let a = VariantKeyV1::new("dark").unwrap();
        let b = VariantKeyV1::new("dark").unwrap();
        let c = VariantKeyV1::new("light").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut set = HashSet::new();
        set.insert(a.clone());
        assert!(set.contains(&b));
        assert!(!set.contains(&c));
    }

    // --- ResolutionScopeV1 ---

    #[test]
    fn resolution_scope_exhaustive_match() {
        let attachment = ResolutionScopeV1::Attachment;
        let program = ResolutionScopeV1::Program;

        let label = match attachment {
            ResolutionScopeV1::Attachment => "attachment",
            ResolutionScopeV1::Program => "program",
        };
        assert_eq!(label, "attachment");

        let label = match program {
            ResolutionScopeV1::Attachment => "attachment",
            ResolutionScopeV1::Program => "program",
        };
        assert_eq!(label, "program");
    }

    #[test]
    fn resolution_scope_copy_clone_eq() {
        let a = ResolutionScopeV1::Attachment;
        let b = a; // Copy
        assert_eq!(a, b);

        let c = ResolutionScopeV1::Program; // Copy
        assert_eq!(c, ResolutionScopeV1::Program);
    }

    // --- CaseScopeIdV1 ---

    #[test]
    fn case_scope_id_ordering() {
        let a = CaseScopeIdV1::from_raw(1);
        let b = CaseScopeIdV1::from_raw(2);
        let c = CaseScopeIdV1::from_raw(1);
        assert!(a < b);
        assert!(b > a);
        assert_eq!(a, c);
    }

    #[test]
    fn case_scope_id_roundtrip() {
        let id = CaseScopeIdV1::from_raw(42);
        assert_eq!(id.to_raw(), 42);
    }

    // --- EnvironmentSelector / sealing ---

    #[test]
    fn default_environment_returns_attachment() {
        let env = DefaultEnvironmentV1;
        assert_eq!(env.select_environment(), ResolutionScopeV1::Attachment);
    }

    #[test]
    fn environment_selector_is_object_safe_for_default() {
        let env = DefaultEnvironmentV1;
        let selector: &dyn EnvironmentSelector = &env;
        assert_eq!(selector.select_environment(), ResolutionScopeV1::Attachment);
    }

    // --- VariantScopeErrorV1 Display ---

    #[test]
    fn error_display_messages() {
        let empty = VariantScopeErrorV1::EmptyVariantKey;
        assert!(format!("{empty}").contains("empty"));

        let reserved = VariantScopeErrorV1::ReservedPrefix("__bad".to_string());
        assert!(format!("{reserved}").contains("__bad"));
        assert!(format!("{reserved}").contains("reserved"));

        let invalid = VariantScopeErrorV1::InvalidVariantKey("nope".to_string());
        assert!(format!("{invalid}").contains("nope"));
    }

    // --- Property test (proptest) ---

    #[cfg(test)]
    mod property_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn prop_variant_key_accepts_valid_strings(
                s in "[a-zA-Z0-9_-]{1,64}"
                    .prop_filter("must not start with __", |s| !s.starts_with("__"))
            ) {
                let result = VariantKeyV1::new(s);
                prop_assert!(result.is_ok());
            }
        }
    }
}