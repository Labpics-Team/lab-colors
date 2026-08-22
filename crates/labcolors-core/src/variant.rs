//! V-06 (V4 Variants/Scopes) type foundation and enforcement wiring.
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

// ---------------------------------------------------------------------------
// PR2: Enforcement logic and session integration
// ---------------------------------------------------------------------------

/// Immutable binding of a validated variant key to a resolution scope.
///
/// A `SessionBindingV1` is constructed once at session instantiation and
/// observed read-only throughout the session lifetime. It does not own or
/// mutate any session state; it merely records the declared topology that
/// enforcement predicates inspect.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionBindingV1 {
    variant: VariantKeyV1,
    scope: ResolutionScopeV1,
}

#[allow(dead_code)]
impl SessionBindingV1 {
    /// Creates a new binding after validating the variant key.
    ///
    /// # Errors
    /// Returns [`VariantScopeErrorV1`] if the variant key is invalid.
    pub(crate) fn new(variant: VariantKeyV1, scope: ResolutionScopeV1) -> Self {
        Self { variant, scope }
    }

    /// Returns a reference to the bound variant key.
    pub(crate) fn variant(&self) -> &VariantKeyV1 {
        &self.variant
    }

    /// Returns the bound resolution scope.
    pub(crate) fn scope(&self) -> ResolutionScopeV1 {
        self.scope
    }
}

/// Read-only observation handle for variant/scope enforcement within a
/// `ProgramSession`.
///
/// This type provides enforcement predicates that validate consistency
/// between a session's declared binding and its runtime observations.
/// It never mutates session state; all checks are pure functions over
/// borrowed references.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct VariantEnforcementObserver<'a> {
    binding: &'a SessionBindingV1,
}

#[allow(dead_code)]
impl<'a> VariantEnforcementObserver<'a> {
    /// Wraps an existing session binding for read-only enforcement queries.
    pub(crate) fn new(binding: &'a SessionBindingV1) -> Self {
        Self { binding }
    }

    /// Returns the observed binding.
    pub(crate) fn binding(&self) -> &SessionBindingV1 {
        self.binding
    }

    /// Validates that the given candidate scope matches the binding's
    /// declared resolution scope.
    ///
    /// In `Program` scope, shared-token intersection requires joint
    /// resolution; an `Attachment` candidate against a `Program` binding
    /// is a hard consistency violation.
    ///
    /// # Errors
    /// Returns [`VariantScopeErrorV1::ScopeMismatch`] when the candidate
    /// scope contradicts the binding.
    pub(crate) fn check_scope_consistency(
        &self,
        candidate_scope: ResolutionScopeV1,
    ) -> Result<(), VariantScopeErrorV1> {
        if self.binding.scope != candidate_scope {
            return Err(VariantScopeErrorV1::ScopeMismatch {
                expected: self.binding.scope,
                actual: candidate_scope,
            });
        }
        Ok(())
    }

    /// Validates that a shared-token declaration is permitted under the
    /// current binding.
    ///
    /// Shared tokens are only meaningful in `Program` scope where joint
    /// intersection applies. Declaring them under `Attachment` scope is
    /// a configuration error because no cross-instance constraint exists
    /// to enforce.
    ///
    /// # Errors
    /// Returns [`VariantScopeErrorV1::SharedTokenInAttachmentScope`]
    /// when the binding uses `Attachment` scope.
    pub(crate) fn check_shared_token_permitted(&self) -> Result<(), VariantScopeErrorV1> {
        match self.binding.scope {
            ResolutionScopeV1::Program => Ok(()),
            ResolutionScopeV1::Attachment => Err(VariantScopeErrorV1::SharedTokenInAttachmentScope),
        }
    }

    /// Validates that two bindings targeting the same variant key agree
    /// on resolution scope.
    ///
    /// Within a single program epoch, multiple sessions may observe the
    /// same variant. If they disagree on scope, shared-token intersection
    /// semantics become ambiguous and the configuration is rejected.
    ///
    /// # Errors
    /// Returns [`VariantScopeErrorV1::ConflictingBindings`] when the
    /// scopes diverge for the same variant key.
    pub(crate) fn check_binding_compatible(
        &self,
        other: &SessionBindingV1,
    ) -> Result<(), VariantScopeErrorV1> {
        if self.binding.variant == other.variant && self.binding.scope != other.scope {
            return Err(VariantScopeErrorV1::ConflictingBindings {
                variant: self.binding.variant.clone(),
                scope_a: self.binding.scope,
                scope_b: other.scope,
            });
        }
        Ok(())
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
    /// Candidate scope does not match the binding's declared scope.
    ScopeMismatch {
        expected: ResolutionScopeV1,
        actual: ResolutionScopeV1,
    },
    /// Shared tokens declared under `Attachment` scope where no
    /// cross-instance intersection exists.
    SharedTokenInAttachmentScope,
    /// Two bindings for the same variant key disagree on resolution scope.
    ConflictingBindings {
        variant: VariantKeyV1,
        scope_a: ResolutionScopeV1,
        scope_b: ResolutionScopeV1,
    },
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
            Self::ScopeMismatch { expected, actual } => {
                write!(
                    f,
                    "resolution scope mismatch: expected {expected:?}, got {actual:?}"
                )
            }
            Self::SharedTokenInAttachmentScope => {
                write!(
                    f,
                    "shared tokens require Program scope; Attachment scope has no cross-instance intersection"
                )
            }
            Self::ConflictingBindings {
                variant,
                scope_a,
                scope_b,
            } => {
                write!(
                    f,
                    "conflicting bindings for variant '{}': {:?} vs {:?}",
                    variant.as_str(),
                    scope_a,
                    scope_b
                )
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

    // --- PR2: SessionBindingV1 ---

    #[test]
    fn session_binding_stores_variant_and_scope() {
        let key = VariantKeyV1::new("dark").unwrap();
        let binding = SessionBindingV1::new(key.clone(), ResolutionScopeV1::Program);
        assert_eq!(binding.variant(), &key);
        assert_eq!(binding.scope(), ResolutionScopeV1::Program);
    }

    #[test]
    fn session_binding_equality() {
        let a = SessionBindingV1::new(
            VariantKeyV1::new("dark").unwrap(),
            ResolutionScopeV1::Attachment,
        );
        let b = SessionBindingV1::new(
            VariantKeyV1::new("dark").unwrap(),
            ResolutionScopeV1::Attachment,
        );
        let c = SessionBindingV1::new(
            VariantKeyV1::new("dark").unwrap(),
            ResolutionScopeV1::Program,
        );
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // --- PR2: VariantEnforcementObserver ---

    #[test]
    fn enforcement_check_scope_consistency_passes_on_match() {
        let binding = SessionBindingV1::new(
            VariantKeyV1::new("dark").unwrap(),
            ResolutionScopeV1::Program,
        );
        let observer = VariantEnforcementObserver::new(&binding);
        assert!(
            observer
                .check_scope_consistency(ResolutionScopeV1::Program)
                .is_ok()
        );
    }

    #[test]
    fn enforcement_check_scope_consistency_fails_on_mismatch() {
        let binding = SessionBindingV1::new(
            VariantKeyV1::new("dark").unwrap(),
            ResolutionScopeV1::Program,
        );
        let observer = VariantEnforcementObserver::new(&binding);
        let err = observer
            .check_scope_consistency(ResolutionScopeV1::Attachment)
            .unwrap_err();
        assert!(matches!(
            err,
            VariantScopeErrorV1::ScopeMismatch {
                expected: ResolutionScopeV1::Program,
                actual: ResolutionScopeV1::Attachment,
            }
        ));
    }

    #[test]
    fn enforcement_shared_token_permitted_in_program_scope() {
        let binding = SessionBindingV1::new(
            VariantKeyV1::new("print").unwrap(),
            ResolutionScopeV1::Program,
        );
        let observer = VariantEnforcementObserver::new(&binding);
        assert!(observer.check_shared_token_permitted().is_ok());
    }

    #[test]
    fn enforcement_shared_token_rejected_in_attachment_scope() {
        let binding = SessionBindingV1::new(
            VariantKeyV1::new("dark").unwrap(),
            ResolutionScopeV1::Attachment,
        );
        let observer = VariantEnforcementObserver::new(&binding);
        let err = observer.check_shared_token_permitted().unwrap_err();
        assert_eq!(err, VariantScopeErrorV1::SharedTokenInAttachmentScope);
    }

    #[test]
    fn enforcement_compatible_bindings_same_variant_same_scope() {
        let a = SessionBindingV1::new(
            VariantKeyV1::new("dark").unwrap(),
            ResolutionScopeV1::Program,
        );
        let b = SessionBindingV1::new(
            VariantKeyV1::new("dark").unwrap(),
            ResolutionScopeV1::Program,
        );
        let observer = VariantEnforcementObserver::new(&a);
        assert!(observer.check_binding_compatible(&b).is_ok());
    }

    #[test]
    fn enforcement_conflicting_bindings_same_variant_different_scope() {
        let a = SessionBindingV1::new(
            VariantKeyV1::new("dark").unwrap(),
            ResolutionScopeV1::Program,
        );
        let b = SessionBindingV1::new(
            VariantKeyV1::new("dark").unwrap(),
            ResolutionScopeV1::Attachment,
        );
        let observer = VariantEnforcementObserver::new(&a);
        let err = observer.check_binding_compatible(&b).unwrap_err();
        assert!(matches!(
            err,
            VariantScopeErrorV1::ConflictingBindings { .. }
        ));
    }

    #[test]
    fn enforcement_different_variants_any_scope_is_compatible() {
        let a = SessionBindingV1::new(
            VariantKeyV1::new("dark").unwrap(),
            ResolutionScopeV1::Program,
        );
        let b = SessionBindingV1::new(
            VariantKeyV1::new("light").unwrap(),
            ResolutionScopeV1::Attachment,
        );
        let observer = VariantEnforcementObserver::new(&a);
        assert!(observer.check_binding_compatible(&b).is_ok());
    }

    #[test]
    fn enforcement_error_display_scope_mismatch() {
        let err = VariantScopeErrorV1::ScopeMismatch {
            expected: ResolutionScopeV1::Program,
            actual: ResolutionScopeV1::Attachment,
        };
        let msg = format!("{err}");
        assert!(msg.contains("mismatch"));
        assert!(msg.contains("Program"));
        assert!(msg.contains("Attachment"));
    }

    #[test]
    fn enforcement_error_display_shared_token() {
        let err = VariantScopeErrorV1::SharedTokenInAttachmentScope;
        let msg = format!("{err}");
        assert!(msg.contains("shared"));
        assert!(msg.contains("Attachment"));
    }

    #[test]
    fn enforcement_error_display_conflicting_bindings() {
        let err = VariantScopeErrorV1::ConflictingBindings {
            variant: VariantKeyV1::new("dark").unwrap(),
            scope_a: ResolutionScopeV1::Program,
            scope_b: ResolutionScopeV1::Attachment,
        };
        let msg = format!("{err}");
        assert!(msg.contains("dark"));
        assert!(msg.contains("conflict"));
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

        proptest! {
            #[test]
            fn prop_scope_consistency_reflexive(
                scope in prop_oneof![
                    Just(ResolutionScopeV1::Attachment),
                    Just(ResolutionScopeV1::Program),
                ]
            ) {
                let binding = SessionBindingV1::new(
                    VariantKeyV1::new("prop-test").unwrap(),
                    scope,
                );
                let observer = VariantEnforcementObserver::new(&binding);
                prop_assert!(observer.check_scope_consistency(scope).is_ok());
            }
        }

        #[test]
        fn prop_shared_token_always_permitted_in_program_scope() {
            let binding = SessionBindingV1::new(
                VariantKeyV1::new("prop-shared").unwrap(),
                ResolutionScopeV1::Program,
            );
            let observer = VariantEnforcementObserver::new(&binding);
            assert!(observer.check_shared_token_permitted().is_ok());
        }

        #[test]
        fn prop_shared_token_always_rejected_in_attachment_scope() {
            let binding = SessionBindingV1::new(
                VariantKeyV1::new("prop-no-shared").unwrap(),
                ResolutionScopeV1::Attachment,
            );
            let observer = VariantEnforcementObserver::new(&binding);
            assert!(observer.check_shared_token_permitted().is_err());
        }
    }
}
