//! R-07 PR-A: Scoped declared-restorative auto type definitions.
//!
//! Pure type layer for the R3s scoped restorative package auto subsystem.
//! These types define the content-addressed release structure, scope taxonomy,
//! evidence provenance, and decision outcomes that the runtime (PR-C) will
//! populate from TechnicalQuality substrates. No runtime integration here.

use crate::sha256;

// ---------------------------------------------------------------------------
// Identity domain separators
// ---------------------------------------------------------------------------

const RESTORATIVE_RELEASE_DOMAIN_V1: &[u8] = b"labcolors.restorative-auto-release.v1\0";
const RESTORATIVE_EVIDENCE_DOMAIN_V1: &[u8] = b"labcolors.restorative-evidence.v1\0";
const RESTORATIVE_DECISION_DOMAIN_V1: &[u8] = b"labcolors.restorative-decision.v1\0";

// ---------------------------------------------------------------------------
// RestorativeScopeV1
// ---------------------------------------------------------------------------

/// The relation-scoped target kind that a restorative evaluation addresses.
///
/// Each variant identifies a distinct substrate layer in the colour pipeline.
/// The scope determines which TechnicalQuality evidence is admissible and
/// which carrier manifest template applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RestorativeScopeKindV1 {
    /// A single point colour candidate.
    Point,
    /// An alpha/backdrop composition layer.
    Alpha,
    /// A whole-field or effect region.
    Field,
    /// A program attachment handoff site.
    Attachment,
}

impl RestorativeScopeKindV1 {
    /// Canonical byte tag for content-addressing.
    fn as_tag(self) -> u8 {
        match self {
            Self::Point => 0x01,
            Self::Alpha => 0x02,
            Self::Field => 0x03,
            Self::Attachment => 0x04,
        }
    }
}

/// A scoped target for restorative evaluation, combining a kind with an
/// opaque identifier that distinguishes instances within the same kind.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RestorativeScopeV1 {
    kind: RestorativeScopeKindV1,
    /// Opaque bytes identifying the specific target within its kind.
    /// For Point: canonical candidate key bytes.
    /// For Alpha: backdrop/layer identity.
    /// For Field: field region address.
    /// For Attachment: attachment wire slot identity.
    target_id: Box<[u8]>,
}

impl RestorativeScopeV1 {
    /// Constructs a new scope. Returns `None` if `target_id` is empty, since
    /// an empty identifier cannot distinguish targets.
    pub(crate) fn new(kind: RestorativeScopeKindV1, target_id: Box<[u8]>) -> Option<Self> {
        if target_id.is_empty() {
            return None;
        }
        Some(Self { kind, target_id })
    }

    pub(crate) fn kind(&self) -> RestorativeScopeKindV1 {
        self.kind
    }

    pub(crate) fn target_id(&self) -> &[u8] {
        &self.target_id
    }

    /// Writes the canonical representation into a hasher for content-addressing.
    fn hash_into(&self, hasher: &mut sha256::Hasher) {
        hasher.update(&[self.kind.as_tag()]);
        let len_bytes = (self.target_id.len() as u64).to_be_bytes();
        hasher.update(&len_bytes);
        hasher.update(&self.target_id);
    }
}

// ---------------------------------------------------------------------------
// RestorativeEvidenceV1
// ---------------------------------------------------------------------------

/// Provenance class of a restorative evidence record.
///
/// Distinguishes which substrate produced the evidence so that downstream
/// policy can enforce admissibility rules per scope kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RestorativeEvidenceProvenanceV1 {
    /// Evidence from the alpha/backdrop TechnicalQuality substrate (R-09).
    AlphaBackdropTechnicalQuality,
    /// Evidence from the field/effect TechnicalQuality substrate (R-10).
    FieldEffectTechnicalQuality,
    /// Evidence from the MovementContract / V5 machinery.
    MovementContract,
    /// Evidence from the package policy engine itself.
    PackagePolicyEngine,
}

impl RestorativeEvidenceProvenanceV1 {
    fn as_tag(self) -> u8 {
        match self {
            Self::AlphaBackdropTechnicalQuality => 0x01,
            Self::FieldEffectTechnicalQuality => 0x02,
            Self::MovementContract => 0x03,
            Self::PackagePolicyEngine => 0x04,
        }
    }
}

/// A single evidence record with provenance, binding it to the scope it
/// supports and carrying the raw evidence payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestorativeEvidenceV1 {
    scope: RestorativeScopeV1,
    provenance: RestorativeEvidenceProvenanceV1,
    /// Opaque evidence payload — interpretation depends on provenance.
    payload: Box<[u8]>,
}

impl RestorativeEvidenceV1 {
    /// Constructs a new evidence record. Returns `None` if `payload` is empty,
    /// since empty evidence cannot support any decision.
    pub(crate) fn new(
        scope: RestorativeScopeV1,
        provenance: RestorativeEvidenceProvenanceV1,
        payload: Box<[u8]>,
    ) -> Option<Self> {
        if payload.is_empty() {
            return None;
        }
        Some(Self {
            scope,
            provenance,
            payload,
        })
    }

    pub(crate) fn scope(&self) -> &RestorativeScopeV1 {
        &self.scope
    }

    pub(crate) fn provenance(&self) -> RestorativeEvidenceProvenanceV1 {
        self.provenance
    }

    pub(crate) fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Content-addressed identity of this evidence record.
    pub(crate) fn digest(&self) -> sha256::Digest {
        let mut hasher = sha256::Hasher::new();
        hasher.update(RESTORATIVE_EVIDENCE_DOMAIN_V1);
        self.scope.hash_into(&mut hasher);
        hasher.update(&[self.provenance.as_tag()]);
        let len_bytes = (self.payload.len() as u64).to_be_bytes();
        hasher.update(&len_bytes);
        hasher.update(&self.payload);
        hasher.finalize()
    }
}

// ---------------------------------------------------------------------------
// RestorativeDecisionV1
// ---------------------------------------------------------------------------

/// Outcome of a restorative evaluation against one scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum RestorativeDecisionOutcomeV1 {
    /// All applicable policy rules passed for this scope.
    Pass,
    /// At least one policy rule was violated; restoration cannot proceed.
    Violation,
    /// Evaluation deferred — required upstream evidence not yet available.
    Deferred,
}

impl RestorativeDecisionOutcomeV1 {
    fn as_tag(self) -> u8 {
        match self {
            Self::Pass => 0x01,
            Self::Violation => 0x02,
            Self::Deferred => 0x03,
        }
    }
}

/// A decision bound to a specific scope, recording outcome and the evidence
/// digests that supported it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RestorativeDecisionV1 {
    scope: RestorativeScopeV1,
    outcome: RestorativeDecisionOutcomeV1,
    /// Digests of the evidence records that informed this decision, in
    /// canonical sorted order. Empty only when outcome is Deferred with
    /// no evidence yet available.
    evidence_digests: Box<[sha256::Digest]>,
}

impl RestorativeDecisionV1 {
    pub(crate) fn new(
        scope: RestorativeScopeV1,
        outcome: RestorativeDecisionOutcomeV1,
        evidence_digests: Box<[sha256::Digest]>,
    ) -> Self {
        Self {
            scope,
            outcome,
            evidence_digests,
        }
    }

    pub(crate) fn scope(&self) -> &RestorativeScopeV1 {
        &self.scope
    }

    pub(crate) fn outcome(&self) -> RestorativeDecisionOutcomeV1 {
        self.outcome
    }

    pub(crate) fn evidence_digests(&self) -> &[sha256::Digest] {
        &self.evidence_digests
    }

    /// Content-addressed identity of this decision.
    pub(crate) fn digest(&self) -> sha256::Digest {
        let mut hasher = sha256::Hasher::new();
        hasher.update(RESTORATIVE_DECISION_DOMAIN_V1);
        self.scope.hash_into(&mut hasher);
        hasher.update(&[self.outcome.as_tag()]);
        let count = (self.evidence_digests.len() as u64).to_be_bytes();
        hasher.update(&count);
        for d in &self.evidence_digests {
            hasher.update(d.as_bytes());
        }
        hasher.finalize()
    }
}

// ---------------------------------------------------------------------------
// DeclaredRestorativeAutoReleaseV1
// ---------------------------------------------------------------------------

/// Content-addressed identity of a declared restorative auto release.
#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DeclaredRestorativeAutoReleaseIdentityV1([u8; 32]);

impl DeclaredRestorativeAutoReleaseIdentityV1 {
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// The published release type for R3s scoped declared-restorative package auto.
///
/// Binds together the non-overridable TechnicalKernel reference, the set of
/// scoped decisions, and the bound execution contract. Content-addressed:
/// any change to any field produces a different identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeclaredRestorativeAutoReleaseV1 {
    /// Schema version for forward compatibility.
    schema_version: u64,
    /// Reference to the TechnicalKernel this release is bound to.
    technical_kernel_digest: sha256::Digest,
    /// Ordered set of scoped decisions. Sorted by scope for canonical form.
    decisions: Box<[RestorativeDecisionV1]>,
    /// Digest of the BoundRestorativeExecutionContract governing this release.
    execution_contract_digest: sha256::Digest,
    /// Precomputed content-addressed identity.
    identity: DeclaredRestorativeAutoReleaseIdentityV1,
}

impl DeclaredRestorativeAutoReleaseV1 {
    /// Constructs a new release and computes its content-addressed identity.
    ///
    /// Returns `None` if `decisions` is empty — a release with no scoped
    /// decisions is vacuous and must not be admitted.
    pub(crate) fn new(
        schema_version: u64,
        technical_kernel_digest: sha256::Digest,
        decisions: Box<[RestorativeDecisionV1]>,
        execution_contract_digest: sha256::Digest,
    ) -> Option<Self> {
        if decisions.is_empty() {
            return None;
        }
        let identity = Self::compute_identity(
            schema_version,
            &technical_kernel_digest,
            &decisions,
            &execution_contract_digest,
        );
        Some(Self {
            schema_version,
            technical_kernel_digest,
            decisions,
            execution_contract_digest,
            identity,
        })
    }

    pub(crate) fn schema_version(&self) -> u64 {
        self.schema_version
    }

    pub(crate) fn technical_kernel_digest(&self) -> &sha256::Digest {
        &self.technical_kernel_digest
    }

    pub(crate) fn decisions(&self) -> &[RestorativeDecisionV1] {
        &self.decisions
    }

    pub(crate) fn execution_contract_digest(&self) -> &sha256::Digest {
        &self.execution_contract_digest
    }

    pub(crate) fn identity(&self) -> &DeclaredRestorativeAutoReleaseIdentityV1 {
        &self.identity
    }

    fn compute_identity(
        schema_version: u64,
        technical_kernel_digest: &sha256::Digest,
        decisions: &[RestorativeDecisionV1],
        execution_contract_digest: &sha256::Digest,
    ) -> DeclaredRestorativeAutoReleaseIdentityV1 {
        let mut hasher = sha256::Hasher::new();
        hasher.update(RESTORATIVE_RELEASE_DOMAIN_V1);
        hasher.update(&schema_version.to_be_bytes());
        hasher.update(technical_kernel_digest.as_bytes());
        let count = (decisions.len() as u64).to_be_bytes();
        hasher.update(&count);
        for decision in decisions {
            let d = decision.digest();
            hasher.update(d.as_bytes());
        }
        hasher.update(execution_contract_digest.as_bytes());
        let digest = hasher.finalize();
        DeclaredRestorativeAutoReleaseIdentityV1(*digest.as_bytes())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_scope_point() -> RestorativeScopeV1 {
        RestorativeScopeV1::new(
            RestorativeScopeKindV1::Point,
            vec![0xAA, 0xBB].into_boxed_slice(),
        )
        .expect("non-empty target_id")
    }

    fn sample_scope_alpha() -> RestorativeScopeV1 {
        RestorativeScopeV1::new(
            RestorativeScopeKindV1::Alpha,
            vec![0xCC, 0xDD].into_boxed_slice(),
        )
        .expect("non-empty target_id")
    }

    // -- Construction guards ------------------------------------------------

    #[test]
    fn scope_rejects_empty_target_id() {
        assert!(RestorativeScopeV1::new(RestorativeScopeKindV1::Point, Box::new([]),).is_none());
    }

    #[test]
    fn evidence_rejects_empty_payload() {
        let scope = sample_scope_point();
        assert!(
            RestorativeEvidenceV1::new(
                scope,
                RestorativeEvidenceProvenanceV1::AlphaBackdropTechnicalQuality,
                Box::new([]),
            )
            .is_none()
        );
    }

    #[test]
    fn release_rejects_empty_decisions() {
        let kernel = sha256::digest(b"kernel");
        let contract = sha256::digest(b"contract");
        assert!(
            DeclaredRestorativeAutoReleaseV1::new(1, kernel, Box::new([]), contract,).is_none()
        );
    }

    // -- Equality -----------------------------------------------------------

    #[test]
    fn scope_equality_by_kind_and_target() {
        let a = sample_scope_point();
        let b = sample_scope_point();
        assert_eq!(a, b);

        let c = sample_scope_alpha();
        assert_ne!(a, c);
    }

    #[test]
    fn evidence_equality_by_all_fields() {
        let scope = sample_scope_point();
        let a = RestorativeEvidenceV1::new(
            scope.clone(),
            RestorativeEvidenceProvenanceV1::FieldEffectTechnicalQuality,
            vec![1, 2, 3].into_boxed_slice(),
        )
        .expect("non-empty payload");
        let b = RestorativeEvidenceV1::new(
            scope,
            RestorativeEvidenceProvenanceV1::FieldEffectTechnicalQuality,
            vec![1, 2, 3].into_boxed_slice(),
        )
        .expect("non-empty payload");
        assert_eq!(a, b);
    }

    #[test]
    fn decision_equality_includes_outcome() {
        let scope = sample_scope_point();
        let pass = RestorativeDecisionV1::new(
            scope.clone(),
            RestorativeDecisionOutcomeV1::Pass,
            Box::new([]),
        );
        let violation = RestorativeDecisionV1::new(
            scope,
            RestorativeDecisionOutcomeV1::Violation,
            Box::new([]),
        );
        assert_ne!(pass, violation);
    }

    // -- Content-addressed identity -----------------------------------------

    #[test]
    fn evidence_digest_changes_with_payload() {
        let scope = sample_scope_point();
        let a = RestorativeEvidenceV1::new(
            scope.clone(),
            RestorativeEvidenceProvenanceV1::MovementContract,
            vec![10].into_boxed_slice(),
        )
        .expect("non-empty");
        let b = RestorativeEvidenceV1::new(
            scope,
            RestorativeEvidenceProvenanceV1::MovementContract,
            vec![20].into_boxed_slice(),
        )
        .expect("non-empty");
        assert_ne!(a.digest(), b.digest());
    }

    #[test]
    fn evidence_digest_stable_for_same_input() {
        let scope = sample_scope_point();
        let a = RestorativeEvidenceV1::new(
            scope.clone(),
            RestorativeEvidenceProvenanceV1::PackagePolicyEngine,
            vec![42].into_boxed_slice(),
        )
        .expect("non-empty");
        let b = RestorativeEvidenceV1::new(
            scope,
            RestorativeEvidenceProvenanceV1::PackagePolicyEngine,
            vec![42].into_boxed_slice(),
        )
        .expect("non-empty");
        assert_eq!(a.digest(), b.digest());
    }

    #[test]
    fn decision_digest_changes_with_outcome() {
        let scope = sample_scope_point();
        let pass = RestorativeDecisionV1::new(
            scope.clone(),
            RestorativeDecisionOutcomeV1::Pass,
            Box::new([]),
        );
        let deferred =
            RestorativeDecisionV1::new(scope, RestorativeDecisionOutcomeV1::Deferred, Box::new([]));
        assert_ne!(pass.digest(), deferred.digest());
    }

    #[test]
    fn release_identity_changes_with_decision() {
        let kernel = sha256::digest(b"kernel");
        let contract = sha256::digest(b"contract");

        let scope = sample_scope_point();
        let dec_pass = RestorativeDecisionV1::new(
            scope.clone(),
            RestorativeDecisionOutcomeV1::Pass,
            Box::new([]),
        );
        let dec_violation = RestorativeDecisionV1::new(
            scope,
            RestorativeDecisionOutcomeV1::Violation,
            Box::new([]),
        );

        let rel_a = DeclaredRestorativeAutoReleaseV1::new(
            1,
            kernel,
            vec![dec_pass].into_boxed_slice(),
            contract,
        )
        .expect("non-empty decisions");
        let rel_b = DeclaredRestorativeAutoReleaseV1::new(
            1,
            kernel,
            vec![dec_violation].into_boxed_slice(),
            contract,
        )
        .expect("non-empty decisions");

        assert_ne!(rel_a.identity(), rel_b.identity());
    }

    #[test]
    fn release_identity_stable_for_same_input() {
        let kernel = sha256::digest(b"kernel");
        let contract = sha256::digest(b"contract");
        let scope = sample_scope_point();
        let dec =
            RestorativeDecisionV1::new(scope, RestorativeDecisionOutcomeV1::Pass, Box::new([]));

        let a = DeclaredRestorativeAutoReleaseV1::new(
            1,
            kernel,
            vec![dec.clone()].into_boxed_slice(),
            contract,
        )
        .expect("non-empty");
        let b = DeclaredRestorativeAutoReleaseV1::new(
            1,
            kernel,
            vec![dec].into_boxed_slice(),
            contract,
        )
        .expect("non-empty");

        assert_eq!(a.identity(), b.identity());
    }

    #[test]
    fn release_identity_changes_with_schema_version() {
        let kernel = sha256::digest(b"kernel");
        let contract = sha256::digest(b"contract");
        let scope = sample_scope_point();
        let dec =
            RestorativeDecisionV1::new(scope, RestorativeDecisionOutcomeV1::Pass, Box::new([]));

        let v1 = DeclaredRestorativeAutoReleaseV1::new(
            1,
            kernel,
            vec![dec.clone()].into_boxed_slice(),
            contract,
        )
        .expect("non-empty");
        let v2 = DeclaredRestorativeAutoReleaseV1::new(
            2,
            kernel,
            vec![dec].into_boxed_slice(),
            contract,
        )
        .expect("non-empty");

        assert_ne!(v1.identity(), v2.identity());
    }

    #[test]
    fn release_identity_changes_with_kernel() {
        let kernel_a = sha256::digest(b"kernel-a");
        let kernel_b = sha256::digest(b"kernel-b");
        let contract = sha256::digest(b"contract");
        let scope = sample_scope_point();
        let dec =
            RestorativeDecisionV1::new(scope, RestorativeDecisionOutcomeV1::Pass, Box::new([]));

        let a = DeclaredRestorativeAutoReleaseV1::new(
            1,
            kernel_a,
            vec![dec.clone()].into_boxed_slice(),
            contract,
        )
        .expect("non-empty");
        let b = DeclaredRestorativeAutoReleaseV1::new(
            1,
            kernel_b,
            vec![dec].into_boxed_slice(),
            contract,
        )
        .expect("non-empty");

        assert_ne!(a.identity(), b.identity());
    }
}
