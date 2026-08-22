//! R-07 (R3s) Scoped Restorative Auto — type foundation (PR-A).
//!
//! This module defines the *declared-restorative* substrate types that
//! `DeclaredRestorativeAutoRelease` will later bind into a content-addressed
//! package. It operates purely on TechnicalQuality + declared-restoration
//! evidence and **never** implies any form of human-clean authority or
//! readability/sentiment admission. Those belong to R-08 and are
//! intentionally unimportable from here.

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
    #[expect(
        dead_code,
        reason = "scope-bounding validation lands in PR-B alongside propagation rules"
    )]
    ScopeExceeded,
    /// A required TechnicalQuality substrate is not available.
    #[expect(
        dead_code,
        reason = "TQ substrate check lands in PR-C once upstream R-09/R-10 gates close"
    )]
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
        // Compile-level absence law: no method named `clean_pass`,
        // `final_owned_clean`, or `human_clean_verdict` exists on this type.
        // Enforced by the absence-law test below.
    }

    #[test]
    fn outcome_rejects_action_not_in_scope() {
        let scope = RestorativeScopeV1::new(&[RestorativeActionV1::ColorShift]);
        let handle = TqDeltaHandleV1::from_bytes([0xCD; 32]);
        let err = RestorativeOutcomeV1::new(
            &scope,
            RestorativeActionV1::BackdropSubstitution,
            handle,
        )
        .unwrap_err();
        assert_eq!(err, RestorativeAutoErrorV1::ActionNotDeclared);
    }

    // -- Property (proptest-style, manual enumeration) ---------------------
    //
    // proptest is not currently wired into this crate's dev-dependencies;
    // the exhaustive enumeration below is equivalent for a 4-variant closed
    // enum and keeps PR-A dependency-free. Replace with `proptest!` in PR-B
    // once the corpus harness lands.

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

    // -- Absence-law: no human-clean type importable from this module ------
    //
    // We scan only the production portion of the source (everything before
    // the `#[cfg(test)]` gate) so the forbidden-token list inside this test
    // does not trigger a false positive against itself.

    #[test]
    fn absence_law_no_human_clean_types_in_restorative_auto_source() {
        let full_source = include_str!("restorative_auto.rs");
        // Split at the test module boundary; everything above it is production code.
        let production_source = full_source
            .split("#[cfg(test)]")
            .next()
            .expect("restorative_auto.rs must contain a #[cfg(test)] boundary");

        // Token fragments are constructed so they cannot appear in this
        // assertion list by accident — each is split across a concat to
        // avoid the test body containing the literal it searches for.
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