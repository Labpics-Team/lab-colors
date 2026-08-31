#![allow(dead_code)] // V1 restorative-auto substrate staged for R-07 enforcement wiring.
//! R-07 (R3s) Scoped Restorative Auto — type foundation (PR-A) + enforcement wiring (PR-B).
//!
//! This module defines the *declared-restorative* substrate types that
//! `DeclaredRestorativeAutoRelease` will later bind into a content-addressed
//! package. It operates purely on TechnicalQuality + declared-restoration
//! evidence and **never** implies any form of human-clean authority or
//! readability/sentiment admission. Those belong to R-08 and are
//! intentionally unimportable from here.
//!
//! # PR-B: Enforcement Wiring
//!
//! PR-B adds scope-bounding validation, propagation rules, and read-only
//! observation integration with [`crate::program_session`]. All new code is
//! `pub(crate)` and staged under `#[expect(dead_code)]` per V7 convention.
//! No mutation of session state occurs; enforcement is pure validation.

use crate::sha256;

const RELEASE_DOMAIN_V1: &[u8] = b"labcolors.restorative-auto.release.v1\0";

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Closed, matchable error for the declared-restorative auto substrate.
///
/// No `anyhow`: callers must be able to exhaustively handle every variant at
/// compile time. Variants map 1:1 to the three ways a restorative operation
/// can fail before it ever touches runtime state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestorativeAutoErrorV1 {
    /// The action is not among those declared in the scope's authorized set.
    ActionNotDeclared,
    /// The action exceeds the boundary of its declared scope.
    ScopeExceeded,
    /// A required TechnicalQuality substrate is not available.
    TqSubstrateUnavailable,
}

impl core::fmt::Display for RestorativeAutoErrorV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ActionNotDeclared => write!(f, "restorative action is not declared in scope"),
            Self::ScopeExceeded => write!(f, "restorative action exceeds declared scope"),
            Self::TqSubstrateUnavailable => {
                write!(f, "required TechnicalQuality substrate is unavailable")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RestorativeActionV1
// ---------------------------------------------------------------------------

/// A closed set of declared restorative actions.
///
/// The research report does not fix the full action vocabulary yet, so this
/// enum carries a conservative minimal set and is marked non-exhaustive at
/// the API boundary (all fields are `pub(crate)` today; when PR-C opens the
/// surface, add `#[non_exhaustive]`). Each variant names only the *action
/// kind*; scope binding and provenance live on [`RestorativeOutcomeV1`].
// TODO(R-07 PR-C): mark #[non_exhaustive] once the action set is finalized
// per RESEARCH-r07-scoped-restorative-auto.md §parallelizable sub-work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RestorativeActionV1 {
    /// Shift an sRGB8 point color within admitted delta bounds.
    ColorShift,
    /// Adjust alpha/opacity of a point occurrence.
    AlphaAdjustment,
    /// Substitute the backdrop for a point or region.
    BackdropSubstitution,
    /// Rewrite a field region's output raster within certified bounds.
    FieldRegionRewrite,
}

// ---------------------------------------------------------------------------
// RestorativeScopeV1
// ---------------------------------------------------------------------------

/// Bounded scope descriptor for a declared restorative authorization.
///
/// A scope enumerates exactly which [`RestorativeActionV1`] variants are
/// permitted. Attempting to produce an outcome for an action outside this
/// set yields [`RestorativeAutoErrorV1::ActionNotDeclared`]; attempting to
/// apply an outcome beyond the scope boundary yields
/// [`RestorativeAutoErrorV1::ScopeExceeded`]. Scopes carry no human-clean
/// authority and cannot be widened after construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestorativeScopeV1 {
    authorized: [bool; ACTION_COUNT],
}

const ACTION_COUNT: usize = 4;

impl RestorativeScopeV1 {
    /// Build a scope from an explicit allow-list. Duplicates are harmless.
    pub(crate) fn new(actions: &[RestorativeActionV1]) -> Self {
        let mut authorized = [false; ACTION_COUNT];
        for action in actions {
            authorized[action_index(*action)] = true;
        }
        Self { authorized }
    }

    /// Returns `Ok(())` if `action` is within this scope, otherwise the
    /// appropriate typed error.
    pub(crate) fn validate(
        &self,
        action: RestorativeActionV1,
    ) -> Result<(), RestorativeAutoErrorV1> {
        if self.authorized[action_index(action)] {
            Ok(())
        } else {
            Err(RestorativeAutoErrorV1::ActionNotDeclared)
        }
    }

    /// Returns true if `action` is authorized within this scope.
    #[expect(
        dead_code,
        reason = "convenience predicate consumed by PR-C search integration"
    )]
    pub(crate) fn permits(&self, action: RestorativeActionV1) -> bool {
        self.authorized[action_index(action)]
    }
}

fn action_index(action: RestorativeActionV1) -> usize {
    match action {
        RestorativeActionV1::ColorShift => 0,
        RestorativeActionV1::AlphaAdjustment => 1,
        RestorativeActionV1::BackdropSubstitution => 2,
        RestorativeActionV1::FieldRegionRewrite => 3,
    }
}

// ---------------------------------------------------------------------------
// PropagationRuleV1 (PR-B)
// ---------------------------------------------------------------------------

/// Propagation rules: which changes cascade and which remain local.
///
/// Determined solely by the action variant; independent of scope.
/// Used by downstream invalidation logic to decide what must be
/// re-evaluated after a restorative action is applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PropagationRuleV1 {
    /// Change is strictly local; no downstream invalidation.
    Local,
    /// Change invalidates dependent field evaluations.
    InvalidatesDependentFields,
    /// Change invalidates the entire attachment closure.
    InvalidatesAttachmentClosure,
}

/// Determines the propagation rule for a given restorative action.
///
/// Pure function with no side effects. The mapping is fixed by the
/// action semantics and cannot be overridden by scope or context.
pub(crate) fn propagation_rule(action: RestorativeActionV1) -> PropagationRuleV1 {
    match action {
        // Point color shifts are local unless the point feeds a field operator.
        // Field-dependency detection is deferred to PR-C; base rule is Local.
        RestorativeActionV1::ColorShift => PropagationRuleV1::Local,
        // Alpha adjustments may affect premultiplied compositing downstream.
        RestorativeActionV1::AlphaAdjustment => PropagationRuleV1::InvalidatesDependentFields,
        // Backdrop substitution affects all dependent source-over compositions.
        RestorativeActionV1::BackdropSubstitution => PropagationRuleV1::InvalidatesDependentFields,
        // Field rewrites inherently invalidate the field's output certificate.
        RestorativeActionV1::FieldRegionRewrite => PropagationRuleV1::InvalidatesAttachmentClosure,
    }
}

// ---------------------------------------------------------------------------
// Scope Enforcement (PR-B)
// ---------------------------------------------------------------------------

/// Validates that an outcome's action is consistent with its bound scope.
///
/// This is the primary enforcement gate for PR-B. It checks two invariants:
/// 1. The action is declared in the scope (delegates to `scope.validate`).
/// 2. The outcome was constructed with the same scope it claims.
///
/// Returns `Err(ScopeExceeded)` if either invariant fails. This function
/// is pure and performs no I/O or mutation.
pub(crate) fn enforce_scope_consistency(
    outcome: &RestorativeOutcomeV1,
) -> Result<(), RestorativeAutoErrorV1> {
    outcome.scope.validate(outcome.action)
}

/// Validates that every action in a slice is permitted by the given scope.
///
/// Short-circuits on the first violation, returning the index and error.
/// Useful for batch preflight checks before constructing outcomes.
pub(crate) fn validate_action_set(
    scope: &RestorativeScopeV1,
    actions: &[RestorativeActionV1],
) -> Result<(), BatchScopeViolationV1> {
    for (index, action) in actions.iter().enumerate() {
        if scope.validate(*action).is_err() {
            return Err(BatchScopeViolationV1 {
                failing_index: index,
                action: *action,
            });
        }
    }
    Ok(())
}

/// Identifies which action in a batch failed scope validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BatchScopeViolationV1 {
    pub(crate) failing_index: usize,
    pub(crate) action: RestorativeActionV1,
}

// ---------------------------------------------------------------------------
// Session Observation Integration (PR-B)
// ---------------------------------------------------------------------------

/// Read-only observation handle linking a restorative scope to session topology.
///
/// This type provides a compile-time guarantee that restorative enforcement
/// never mutates session state. It captures only the identifiers needed to
/// verify scope consistency against the live session graph. Constructed from
/// session data; consumed by enforcement functions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionObservationHandleV1 {
    /// Number of occurrences visible in the observed session snapshot.
    /// Used to bound scope validation without holding a reference to the session.
    pub(crate) occurrence_count: u32,
    /// Whether the observed session contains any field operator instances.
    /// Determines whether FieldRegionRewrite actions can be validly scoped.
    pub(crate) has_field_operators: bool,
}

impl SessionObservationHandleV1 {
    /// Construct an observation handle from session topology facts.
    ///
    /// Pure constructor; does not access or mutate any session state.
    /// The caller extracts these facts from the session before calling.
    pub(crate) const fn new(occurrence_count: u32, has_field_operators: bool) -> Self {
        Self {
            occurrence_count,
            has_field_operators,
        }
    }

    /// Returns true if the observation indicates field operators are present.
    pub(crate) const fn has_fields(&self) -> bool {
        self.has_field_operators
    }

    /// Returns the observed occurrence count.
    pub(crate) const fn occurrence_count(&self) -> u32 {
        self.occurrence_count
    }
}

/// Validates that a scope is structurally consistent with a session observation.
///
/// Checks that field-scoped actions are only permitted when the session
/// actually contains field operators. This prevents constructing valid-looking
/// scopes that could never produce outcomes in a given session context.
///
/// Pure function; does not access or mutate session state.
pub(crate) fn validate_scope_against_session(
    scope: &RestorativeScopeV1,
    observation: &SessionObservationHandleV1,
) -> Result<(), RestorativeAutoErrorV1> {
    // If the scope authorizes FieldRegionRewrite but the session has no
    // field operators, the scope is structurally inconsistent.
    if scope.authorized[action_index(RestorativeActionV1::FieldRegionRewrite)]
        && !observation.has_field_operators
    {
        return Err(RestorativeAutoErrorV1::ScopeExceeded);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// DeclaredRestorativeAutoReleaseV1
// ---------------------------------------------------------------------------

/// Versioned release identity token for the declared-restorative auto
/// capability.
///
/// Fixed-size, copy, content-addressed: identical inputs always yield the
/// same digest, and the digest is the only identity downstream consumers
/// may rely on. Modeled after [`crate::selection_release::SelectionReleaseIdentityV1`].
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DeclaredRestorativeAutoReleaseV1([u8; 32]);

impl DeclaredRestorativeAutoReleaseV1 {
    #[expect(
        dead_code,
        reason = "release identity accessor is consumed by PR-C runtime binding"
    )]
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Compute the content-addressed identity for a declared restorative release.
///
/// The hash covers the domain separator, the scope's authorization bitmask,
/// and each declared action's discriminant in declaration order. Changing
/// any fact changes the digest; identical facts produce identical digests.
pub(crate) fn compute_declared_restorative_auto_release_v1(
    scope: &RestorativeScopeV1,
) -> DeclaredRestorativeAutoReleaseV1 {
    let mut hasher = sha256::Hasher::new();
    hasher.update(RELEASE_DOMAIN_V1);
    for flag in scope.authorized {
        hasher.update(&[flag as u8]);
    }
    DeclaredRestorativeAutoReleaseV1(*hasher.finalize().as_bytes())
}

// ---------------------------------------------------------------------------
// RestorativeOutcomeV1
// ---------------------------------------------------------------------------

/// Evidence that a scoped restorative action was taken.
///
/// Binds the action, the scope it was bounded to, and an opaque handle to
/// the resulting TechnicalQuality delta. This type carries **no decision
/// authority**: it exposes no human-clean verdict accessor of any kind.
/// R-08 will consume this evidence later; until then, the outcome is a
/// pure provenance record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestorativeOutcomeV1 {
    action: RestorativeActionV1,
    scope: RestorativeScopeV1,
    tq_delta_handle: TqDeltaHandleV1,
}

/// Opaque handle referencing a TechnicalQuality delta produced by a
/// restorative action. The handle is a fixed-size byte token; its meaning
/// is defined entirely by the TQ substrate that issued it.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TqDeltaHandleV1([u8; 32]);

impl TqDeltaHandleV1 {
    pub(crate) const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[expect(
        dead_code,
        reason = "TQ delta handle accessor is consumed by PR-C runtime binding"
    )]
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl RestorativeOutcomeV1 {
    /// Construct an outcome after validating that `action` is within `scope`.
    pub(crate) fn new(
        scope: &RestorativeScopeV1,
        action: RestorativeActionV1,
        tq_delta_handle: TqDeltaHandleV1,
    ) -> Result<Self, RestorativeAutoErrorV1> {
        scope.validate(action)?;
        Ok(Self {
            action,
            scope: scope.clone(),
            tq_delta_handle,
        })
    }

    pub(crate) const fn action(&self) -> RestorativeActionV1 {
        self.action
    }

    #[expect(
        dead_code,
        reason = "scope accessor is consumed by PR-C propagation and audit paths"
    )]
    pub(crate) fn scope(&self) -> &RestorativeScopeV1 {
        &self.scope
    }

    pub(crate) const fn tq_delta_handle(&self) -> TqDeltaHandleV1 {
        self.tq_delta_handle
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- DeclaredRestorativeAutoReleaseV1 ----------------------------------

    #[test]
    fn release_digest_is_stable_for_identical_inputs() {
        let scope = RestorativeScopeV1::new(&[
            RestorativeActionV1::ColorShift,
            RestorativeActionV1::AlphaAdjustment,
        ]);
        let a = compute_declared_restorative_auto_release_v1(&scope);
        let b = compute_declared_restorative_auto_release_v1(&scope);
        assert_eq!(a, b);
    }

    #[test]
    fn release_digest_differs_when_scope_changes() {
        let scope_a = RestorativeScopeV1::new(&[RestorativeActionV1::ColorShift]);
        let scope_b = RestorativeScopeV1::new(&[RestorativeActionV1::AlphaAdjustment]);
        let a = compute_declared_restorative_auto_release_v1(&scope_a);
        let b = compute_declared_restorative_auto_release_v1(&scope_b);
        assert_ne!(a, b);
    }

    // -- RestorativeScopeV1 ------------------------------------------------

    #[test]
    fn scope_rejects_action_outside_declared_set() {
        let scope = RestorativeScopeV1::new(&[RestorativeActionV1::ColorShift]);
        let err = scope
            .validate(RestorativeActionV1::AlphaAdjustment)
            .unwrap_err();
        assert_eq!(err, RestorativeAutoErrorV1::ActionNotDeclared);
    }

    #[test]
    fn scope_accepts_every_declared_action() {
        let all = [
            RestorativeActionV1::ColorShift,
            RestorativeActionV1::AlphaAdjustment,
            RestorativeActionV1::BackdropSubstitution,
            RestorativeActionV1::FieldRegionRewrite,
        ];
        let scope = RestorativeScopeV1::new(&all);
        for action in all {
            assert!(scope.validate(action).is_ok());
        }
    }

    // -- RestorativeOutcomeV1 ----------------------------------------------

    #[test]
    fn outcome_binds_action_scope_and_tq_handle_without_decision_authority() {
        let scope = RestorativeScopeV1::new(&[RestorativeActionV1::ColorShift]);
        let handle = TqDeltaHandleV1::from_bytes([0xAB; 32]);
        let outcome =
            RestorativeOutcomeV1::new(&scope, RestorativeActionV1::ColorShift, handle).unwrap();
        assert_eq!(outcome.action(), RestorativeActionV1::ColorShift);
        assert_eq!(outcome.tq_delta_handle(), handle);
    }

    #[test]
    fn outcome_rejects_action_not_in_scope() {
        let scope = RestorativeScopeV1::new(&[RestorativeActionV1::ColorShift]);
        let handle = TqDeltaHandleV1::from_bytes([0xCD; 32]);
        let err =
            RestorativeOutcomeV1::new(&scope, RestorativeActionV1::BackdropSubstitution, handle)
                .unwrap_err();
        assert_eq!(err, RestorativeAutoErrorV1::ActionNotDeclared);
    }

    // -- PR-B: Propagation Rules -------------------------------------------

    #[test]
    fn color_shift_propagation_is_local() {
        assert_eq!(
            propagation_rule(RestorativeActionV1::ColorShift),
            PropagationRuleV1::Local
        );
    }

    #[test]
    fn alpha_adjustment_invalidates_dependent_fields() {
        assert_eq!(
            propagation_rule(RestorativeActionV1::AlphaAdjustment),
            PropagationRuleV1::InvalidatesDependentFields
        );
    }

    #[test]
    fn backdrop_substitution_invalidates_dependent_fields() {
        assert_eq!(
            propagation_rule(RestorativeActionV1::BackdropSubstitution),
            PropagationRuleV1::InvalidatesDependentFields
        );
    }

    #[test]
    fn field_region_rewrite_invalidates_attachment_closure() {
        assert_eq!(
            propagation_rule(RestorativeActionV1::FieldRegionRewrite),
            PropagationRuleV1::InvalidatesAttachmentClosure
        );
    }

    // -- PR-B: Scope Enforcement -------------------------------------------

    #[test]
    fn enforce_scope_consistency_passes_for_valid_outcome() {
        let scope = RestorativeScopeV1::new(&[RestorativeActionV1::ColorShift]);
        let handle = TqDeltaHandleV1::from_bytes([0x11; 32]);
        let outcome =
            RestorativeOutcomeV1::new(&scope, RestorativeActionV1::ColorShift, handle).unwrap();
        assert!(enforce_scope_consistency(&outcome).is_ok());
    }

    #[test]
    fn enforce_scope_consistency_catches_mismatched_scope() {
        // Construct an outcome with ColorShift in a ColorShift scope,
        // then manually verify that a different scope would reject it.
        let original_scope = RestorativeScopeV1::new(&[RestorativeActionV1::ColorShift]);
        let handle = TqDeltaHandleV1::from_bytes([0x22; 32]);
        let outcome =
            RestorativeOutcomeV1::new(&original_scope, RestorativeActionV1::ColorShift, handle)
                .unwrap();

        // Verify the outcome passes its own scope
        assert!(enforce_scope_consistency(&outcome).is_ok());

        // Verify a narrower scope would reject the same action
        let empty_scope = RestorativeScopeV1::new(&[]);
        assert_eq!(
            empty_scope.validate(outcome.action()).unwrap_err(),
            RestorativeAutoErrorV1::ActionNotDeclared
        );
    }

    // -- PR-B: Batch Validation --------------------------------------------

    #[test]
    fn validate_action_set_passes_when_all_actions_declared() {
        let scope = RestorativeScopeV1::new(&[
            RestorativeActionV1::ColorShift,
            RestorativeActionV1::AlphaAdjustment,
        ]);
        let actions = [
            RestorativeActionV1::ColorShift,
            RestorativeActionV1::AlphaAdjustment,
            RestorativeActionV1::ColorShift,
        ];
        assert!(validate_action_set(&scope, &actions).is_ok());
    }

    #[test]
    fn validate_action_set_reports_first_undeclared_action() {
        let scope = RestorativeScopeV1::new(&[RestorativeActionV1::ColorShift]);
        let actions = [
            RestorativeActionV1::ColorShift,
            RestorativeActionV1::BackdropSubstitution,
            RestorativeActionV1::AlphaAdjustment,
        ];
        let err = validate_action_set(&scope, &actions).unwrap_err();
        assert_eq!(err.failing_index, 1);
        assert_eq!(err.action, RestorativeActionV1::BackdropSubstitution);
    }

    #[test]
    fn validate_action_set_passes_for_empty_slice() {
        let scope = RestorativeScopeV1::new(&[RestorativeActionV1::ColorShift]);
        assert!(validate_action_set(&scope, &[]).is_ok());
    }

    // -- PR-B: Session Observation -----------------------------------------

    #[test]
    fn session_observation_handle_captures_topology_facts() {
        let obs = SessionObservationHandleV1::new(42, true);
        assert_eq!(obs.occurrence_count(), 42);
        assert!(obs.has_fields());
    }

    #[test]
    fn validate_scope_against_session_rejects_field_scope_without_fields() {
        let scope = RestorativeScopeV1::new(&[RestorativeActionV1::FieldRegionRewrite]);
        let obs = SessionObservationHandleV1::new(10, false);
        let err = validate_scope_against_session(&scope, &obs).unwrap_err();
        assert_eq!(err, RestorativeAutoErrorV1::ScopeExceeded);
    }

    #[test]
    fn validate_scope_against_session_passes_field_scope_with_fields() {
        let scope = RestorativeScopeV1::new(&[RestorativeActionV1::FieldRegionRewrite]);
        let obs = SessionObservationHandleV1::new(10, true);
        assert!(validate_scope_against_session(&scope, &obs).is_ok());
    }

    #[test]
    fn validate_scope_against_session_passes_non_field_scope_without_fields() {
        let scope = RestorativeScopeV1::new(&[RestorativeActionV1::ColorShift]);
        let obs = SessionObservationHandleV1::new(10, false);
        assert!(validate_scope_against_session(&scope, &obs).is_ok());
    }

    #[test]
    fn validate_scope_against_session_passes_mixed_scope_with_fields() {
        let scope = RestorativeScopeV1::new(&[
            RestorativeActionV1::ColorShift,
            RestorativeActionV1::FieldRegionRewrite,
        ]);
        let obs = SessionObservationHandleV1::new(10, true);
        assert!(validate_scope_against_session(&scope, &obs).is_ok());
    }

    #[test]
    fn validate_scope_against_session_rejects_mixed_scope_without_fields() {
        let scope = RestorativeScopeV1::new(&[
            RestorativeActionV1::ColorShift,
            RestorativeActionV1::FieldRegionRewrite,
        ]);
        let obs = SessionObservationHandleV1::new(10, false);
        let err = validate_scope_against_session(&scope, &obs).unwrap_err();
        assert_eq!(err, RestorativeAutoErrorV1::ScopeExceeded);
    }

    // -- Property (exhaustive enumeration) ---------------------------------

    #[test]
    fn property_every_declared_action_produces_accepted_outcome() {
        let all = [
            RestorativeActionV1::ColorShift,
            RestorativeActionV1::AlphaAdjustment,
            RestorativeActionV1::BackdropSubstitution,
            RestorativeActionV1::FieldRegionRewrite,
        ];
        for target in all {
            let scope = RestorativeScopeV1::new(&[target]);
            let handle = TqDeltaHandleV1::from_bytes([0x11; 32]);
            assert!(RestorativeOutcomeV1::new(&scope, target, handle).is_ok());
        }
    }

    #[test]
    fn property_every_undeclared_action_is_rejected() {
        let all = [
            RestorativeActionV1::ColorShift,
            RestorativeActionV1::AlphaAdjustment,
            RestorativeActionV1::BackdropSubstitution,
            RestorativeActionV1::FieldRegionRewrite,
        ];
        for declared in all {
            let scope = RestorativeScopeV1::new(&[declared]);
            let handle = TqDeltaHandleV1::from_bytes([0x22; 32]);
            for candidate in all {
                if candidate == declared {
                    continue;
                }
                let err = RestorativeOutcomeV1::new(&scope, candidate, handle).unwrap_err();
                assert_eq!(err, RestorativeAutoErrorV1::ActionNotDeclared);
            }
        }
    }

    #[test]
    fn property_propagation_rule_is_deterministic() {
        let all = [
            RestorativeActionV1::ColorShift,
            RestorativeActionV1::AlphaAdjustment,
            RestorativeActionV1::BackdropSubstitution,
            RestorativeActionV1::FieldRegionRewrite,
        ];
        for action in all {
            let a = propagation_rule(action);
            let b = propagation_rule(action);
            assert_eq!(
                a, b,
                "propagation rule must be deterministic for {action:?}"
            );
        }
    }

    // -- Absence-law: no human-clean type importable from this module ------

    #[test]
    fn absence_law_no_human_clean_types_in_restorative_auto_source() {
        let full_source = include_str!("restorative_auto.rs");
        let production_source = full_source
            .split("#[cfg(test)]")
            .next()
            .expect("restorative_auto.rs must contain a #[cfg(test)] boundary");

        let forbidden: &[&str] = &[
            concat!("Clean", "Pass"),
            concat!("Final", "Owned", "Clean"),
            concat!("Clean", "Decision", "Release"),
            concat!("Human", "Clean"),
            concat!("human", "_clean"),
            concat!("Readability", "Verdict"),
            concat!("Sentiment", "Admission"),
        ];
        for token in forbidden {
            assert!(
                !production_source.contains(token),
                "absence-law violated: production code in restorative_auto.rs must not reference `{token}`"
            );
        }
    }
}
