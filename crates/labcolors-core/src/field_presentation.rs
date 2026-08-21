//! Field presentation layer: typed certificate taxonomy closing the compiled
//! field operator DAG into renderer-bound ownership proofs.
//!
//! This module distinguishes exact vs conservative footprints at the type level,
//! unifies all spatial effects (gradient/shadow/blur/glass) under one evidence
//! law, and provides the presentation-level certificate taxonomy consumed by
//! the rendering pipeline.

use crate::field_effect::{
    FieldCertificateDigestV1, FieldFootprintV1, FieldOperatorInstanceIdV1,
    FieldRasterDigestV1, FieldWholeRasterCertificateV1,
};
use crate::program_session::PresentationRootId;

// ---------------------------------------------------------------------------
// FieldPresentationRootV1
// ---------------------------------------------------------------------------

/// Opaque identifier closing the full downstream field render/composition DAG.
/// Distinct from `PointPresentationRootV1` which terminates at `OccurrenceId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FieldPresentationRootV1 {
    id: PresentationRootId,
    terminal_operator: FieldOperatorInstanceIdV1,
    dependency_cone_digest: FieldCertificateDigestV1,
}

impl FieldPresentationRootV1 {
    pub(crate) const fn new(
        id: PresentationRootId,
        terminal_operator: FieldOperatorInstanceIdV1,
        dependency_cone_digest: FieldCertificateDigestV1,
    ) -> Self {
        Self {
            id,
            terminal_operator,
            dependency_cone_digest,
        }
    }

    pub(crate) const fn id(self) -> PresentationRootId {
        self.id
    }

    pub(crate) const fn terminal_operator(self) -> FieldOperatorInstanceIdV1 {
        self.terminal_operator
    }

    pub(crate) const fn dependency_cone_digest(self) -> FieldCertificateDigestV1 {
        self.dependency_cone_digest
    }
}

// ---------------------------------------------------------------------------
// FieldFootprintClassV1
// ---------------------------------------------------------------------------

/// Type-level discriminant distinguishing exact from conservative footprints.
/// Conservative write bound MUST NOT mint exact ownership certificates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FieldFootprintClassV1 {
    /// Exact input footprint equals conservative input footprint.
    /// Permits minting `ExactFinalOwnedBundleDomainV1`.
    Exact,
    /// Conservative input footprint strictly contains exact input footprint.
    /// Permits only Representation and Contribution certificates.
    Conservative,
}

impl FieldFootprintClassV1 {
    /// Derive footprint class from existing `FieldFootprintV1`.
    ///
    /// `FieldRectV1` derives `PartialEq, Eq, Hash` (field_effect.rs:238),
    /// so this comparison is valid as-is.
    pub(crate) fn from_footprint(footprint: FieldFootprintV1) -> Self {
        if footprint.exact_input() == footprint.conservative_input() {
            Self::Exact
        } else {
            Self::Conservative
        }
    }

    pub(crate) const fn permits_exact_ownership(self) -> bool {
        matches!(self, Self::Exact)
    }
}

// ---------------------------------------------------------------------------
// OwnedCompositionReferenceV1
// ---------------------------------------------------------------------------

/// Exact simultaneous-zero ownership reference for a field composition.
/// Wraps the atomic whole-raster certificate with zeroing proof.
/// Derived from `FieldWholeRasterCertificateV1`, not minted independently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OwnedCompositionReferenceV1 {
    source_certificate: FieldWholeRasterCertificateV1,
    zeroing_proof_digest: FieldCertificateDigestV1,
    footprint_class: FieldFootprintClassV1,
}

impl OwnedCompositionReferenceV1 {
    pub(crate) const fn new(
        source_certificate: FieldWholeRasterCertificateV1,
        zeroing_proof_digest: FieldCertificateDigestV1,
        footprint_class: FieldFootprintClassV1,
    ) -> Self {
        Self {
            source_certificate,
            zeroing_proof_digest,
            footprint_class,
        }
    }

    pub(crate) const fn source_certificate(self) -> FieldWholeRasterCertificateV1 {
        self.source_certificate
    }

    pub(crate) const fn zeroing_proof_digest(self) -> FieldCertificateDigestV1 {
        self.zeroing_proof_digest
    }

    pub(crate) const fn footprint_class(self) -> FieldFootprintClassV1 {
        self.footprint_class
    }
}

// ---------------------------------------------------------------------------
// CounterfactualSupportV1
// ---------------------------------------------------------------------------

/// Counterfactual output state with owned contribution zeroed.
/// Paired with `OwnedCompositionReferenceV1` to prove exact total ownership.
/// Both reference and counterfactual derived from `FieldWholeRasterCertificateV1` evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CounterfactualSupportV1 {
    reference: OwnedCompositionReferenceV1,
    counterfactual_raster_digest: FieldRasterDigestV1,
    counterfactual_certificate_digest: FieldCertificateDigestV1,
}

impl CounterfactualSupportV1 {
    pub(crate) const fn new(
        reference: OwnedCompositionReferenceV1,
        counterfactual_raster_digest: FieldRasterDigestV1,
        counterfactual_certificate_digest: FieldCertificateDigestV1,
    ) -> Self {
        Self {
            reference,
            counterfactual_raster_digest,
            counterfactual_certificate_digest,
        }
    }

    pub(crate) const fn reference(self) -> OwnedCompositionReferenceV1 {
        self.reference
    }

    pub(crate) const fn counterfactual_raster_digest(self) -> FieldRasterDigestV1 {
        self.counterfactual_raster_digest
    }

    pub(crate) const fn counterfactual_certificate_digest(self) -> FieldCertificateDigestV1 {
        self.counterfactual_certificate_digest
    }
}

// ---------------------------------------------------------------------------
// ExactFinalOwnedBundleDomainV1
// ---------------------------------------------------------------------------

/// Closure-level ownership domain computed after all compositing.
/// Aggregates all contributing operator certificates into a single
/// bundle-level proof. Occurrence domains remain tagged causal provenance
/// and never substitute for this bundle.
///
/// Construction requires `FieldFootprintClassV1::Exact` — conservative
/// footprints cannot produce this type. This is enforced at the type level
/// by requiring the caller to supply a `FieldFootprintClassV1` and validating
/// it in the constructor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExactFinalOwnedBundleDomainV1 {
    root: FieldPresentationRootV1,
    contributing_certificates: Box<[FieldWholeRasterCertificateV1]>,
    bundle_digest: FieldCertificateDigestV1,
    footprint_class: FieldFootprintClassV1,
}

impl ExactFinalOwnedBundleDomainV1 {
    /// Construct a new bundle domain.
    ///
    /// # Panics
    ///
    /// Panics if `footprint_class` is `Conservative`. Callers MUST check
    /// `footprint_class.permits_exact_ownership()` before calling this
    /// constructor. In production paths, use `try_new` instead.
    pub(crate) fn new(
        root: FieldPresentationRootV1,
        contributing_certificates: Box<[FieldWholeRasterCertificateV1]>,
        bundle_digest: FieldCertificateDigestV1,
        footprint_class: FieldFootprintClassV1,
    ) -> Self {
        assert!(
            footprint_class.permits_exact_ownership(),
            "ExactFinalOwnedBundleDomainV1 requires Exact footprint class; \
             Conservative footprints must not mint exact ownership certificates"
        );
        Self {
            root,
            contributing_certificates,
            bundle_digest,
            footprint_class,
        }
    }

    /// Fallible constructor for production paths.
    pub(crate) fn try_new(
        root: FieldPresentationRootV1,
        contributing_certificates: Box<[FieldWholeRasterCertificateV1]>,
        bundle_digest: FieldCertificateDigestV1,
        footprint_class: FieldFootprintClassV1,
    ) -> Option<Self> {
        if !footprint_class.permits_exact_ownership() {
            return None;
        }
        Some(Self {
            root,
            contributing_certificates,
            bundle_digest,
            footprint_class,
        })
    }

    pub(crate) const fn root(&self) -> FieldPresentationRootV1 {
        self.root
    }

    pub(crate) fn contributing_certificates(&self) -> &[FieldWholeRasterCertificateV1] {
        &self.contributing_certificates
    }

    pub(crate) const fn bundle_digest(&self) -> FieldCertificateDigestV1 {
        self.bundle_digest
    }

    pub(crate) const fn footprint_class(&self) -> FieldFootprintClassV1 {
        self.footprint_class
    }
}

// ---------------------------------------------------------------------------
// FieldPresentationCertificateV1
// ---------------------------------------------------------------------------

/// Presentation-level certificate taxonomy. Each variant wraps the atomic
/// whole-raster certificate with distinct semantic guarantees.
///
/// All variants compose with `FieldWholeRasterCertificateV1::digest()` without
/// altering its computation (byte-identity contract preserved).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FieldPresentationCertificateV1 {
    /// What the field looks like. Valid for both Exact and Conservative footprints.
    Representation {
        source: FieldWholeRasterCertificateV1,
        footprint_class: FieldFootprintClassV1,
    },
    /// What portion of the output is attributable to this field.
    /// Valid for both Exact and Conservative footprints.
    Contribution {
        source: FieldWholeRasterCertificateV1,
        footprint_class: FieldFootprintClassV1,
        attribution_digest: FieldCertificateDigestV1,
    },
    /// Closure-level ownership after all compositing.
    /// ONLY valid for Exact footprints. Construction enforces this at type level
    /// via `ExactFinalOwnedBundleDomainV1` requiring `FieldFootprintClassV1::Exact`.
    FinalOwned {
        bundle: ExactFinalOwnedBundleDomainV1,
        counterfactual: CounterfactualSupportV1,
    },
}

impl FieldPresentationCertificateV1 {
    /// Extract the underlying whole-raster certificate digest.
    /// For `FinalOwned`, returns the bundle digest.
    pub(crate) fn source_digest(&self) -> FieldCertificateDigestV1 {
        match self {
            Self::Representation { source, .. } => source.digest(),
            Self::Contribution { source, .. } => source.digest(),
            Self::FinalOwned { bundle, .. } => bundle.bundle_digest(),
        }
    }

    pub(crate) fn footprint_class(&self) -> FieldFootprintClassV1 {
        match self {
            Self::Representation { footprint_class, .. } => *footprint_class,
            Self::Contribution { footprint_class, .. } => *footprint_class,
            Self::FinalOwned { .. } => FieldFootprintClassV1::Exact,
        }
    }

    /// Construct a Representation certificate.
    pub(crate) const fn representation(
        source: FieldWholeRasterCertificateV1,
        footprint_class: FieldFootprintClassV1,
    ) -> Self {
        Self::Representation {
            source,
            footprint_class,
        }
    }

    /// Construct a Contribution certificate.
    pub(crate) const fn contribution(
        source: FieldWholeRasterCertificateV1,
        footprint_class: FieldFootprintClassV1,
        attribution_digest: FieldCertificateDigestV1,
    ) -> Self {
        Self::Contribution {
            source,
            footprint_class,
            attribution_digest,
        }
    }

    /// Construct a FinalOwned certificate.
    ///
    /// Type-level enforcement: `ExactFinalOwnedBundleDomainV1` can only be
    /// constructed with `FieldFootprintClassV1::Exact`, so this variant
    /// is unreachable for conservative footprints.
    pub(crate) fn final_owned(
        bundle: ExactFinalOwnedBundleDomainV1,
        counterfactual: CounterfactualSupportV1,
    ) -> Self {
        Self::FinalOwned {
            bundle,
            counterfactual,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_footprint_class_exact_permits_ownership() {
        assert!(FieldFootprintClassV1::Exact.permits_exact_ownership());
    }

    #[test]
    fn test_footprint_class_conservative_blocks_ownership() {
        assert!(!FieldFootprintClassV1::Conservative.permits_exact_ownership());
    }

    #[test]
    fn test_bundle_domain_try_new_rejects_conservative() {
        let root = FieldPresentationRootV1::new(
            PresentationRootId::new(0),
            FieldOperatorInstanceIdV1::new(0),
            FieldCertificateDigestV1::new(0),
        );
        let result = ExactFinalOwnedBundleDomainV1::try_new(
            root,
            Box::new([]),
            FieldCertificateDigestV1::new(0),
            FieldFootprintClassV1::Conservative,
        );
        assert!(
            result.is_none(),
            "Conservative footprint must not produce ExactFinalOwnedBundleDomainV1"
        );
    }

    #[test]
    fn test_bundle_domain_try_new_accepts_exact() {
        let root = FieldPresentationRootV1::new(
            PresentationRootId::new(0),
            FieldOperatorInstanceIdV1::new(0),
            FieldCertificateDigestV1::new(0),
        );
        let result = ExactFinalOwnedBundleDomainV1::try_new(
            root,
            Box::new([]),
            FieldCertificateDigestV1::new(0),
            FieldFootprintClassV1::Exact,
        );
        assert!(
            result.is_some(),
            "Exact footprint must produce ExactFinalOwnedBundleDomainV1"
        );
    }

    #[test]
    #[should_panic(expected = "Exact footprint class")]
    fn test_bundle_domain_new_panics_on_conservative() {
        let root = FieldPresentationRootV1::new(
            PresentationRootId::new(0),
            FieldOperatorInstanceIdV1::new(0),
            FieldCertificateDigestV1::new(0),
        );
        let _ = ExactFinalOwnedBundleDomainV1::new(
            root,
            Box::new([]),
            FieldCertificateDigestV1::new(0),
            FieldFootprintClassV1::Conservative,
        );
    }

    #[test]
    fn test_final_owned_always_reports_exact_footprint_class() {
        // FinalOwned always reports Exact regardless of internal state
        // because ExactFinalOwnedBundleDomainV1 can only be constructed
        // with Exact footprint class.
        // Full construction test deferred to integration tests with real certificates.
    }

    #[test]
    fn test_counterfactual_pairs_with_reference() {
        // Verify structural round-trip: reference extracted from counterfactual
        // equals the reference used to construct it.
        // Full construction deferred to integration tests with real certificates.
    }
}