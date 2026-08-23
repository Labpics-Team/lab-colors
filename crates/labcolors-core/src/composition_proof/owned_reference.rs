use crate::sha256;

/// Binds a composition law proof to an owned root closure.
/// This is the primary link between mathematical composition guarantees
/// and the ownership tree that R3c certifies as human-clean.
/// Content-addressed via digest of (law_proof_digest, owned_root_id, support_domain_id).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged before PR-C field proof engine")]
pub(crate) struct OwnedCompositionReferenceV1 {
    /// Digest of the CompositionLawProofV1 this reference binds to.
    pub(crate) law_proof_digest: [u8; 32],
    /// Identifier of the owned root closure. Opaque; assigned by ownership registry.
    pub(crate) owned_root_id: String,
    /// Identifier of the support domain over which base acceptance was proven.
    pub(crate) support_domain_id: String,
    /// Content-addressed identity of this reference.
    pub(crate) digest: [u8; 32],
}

/// Errors returned when constructing an [`OwnedCompositionReferenceV1`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged")]
pub(crate) enum OwnedCompositionReferenceError {
    EmptyOwnedRootId,
    EmptySupportDomainId,
}

impl OwnedCompositionReferenceV1 {
    /// Construct a validated owned composition reference. Computes content-addressed digest.
    pub(crate) fn new(
        law_proof_digest: [u8; 32],
        owned_root_id: String,
        support_domain_id: String,
    ) -> Result<Self, OwnedCompositionReferenceError> {
        if owned_root_id.is_empty() {
            return Err(OwnedCompositionReferenceError::EmptyOwnedRootId);
        }
        if support_domain_id.is_empty() {
            return Err(OwnedCompositionReferenceError::EmptySupportDomainId);
        }

        let digest = Self::compute_digest(&law_proof_digest, &owned_root_id, &support_domain_id);

        Ok(Self {
            law_proof_digest,
            owned_root_id,
            support_domain_id,
            digest,
        })
    }

    fn compute_digest(
        law_proof_digest: &[u8; 32],
        owned_root_id: &str,
        support_domain_id: &str,
    ) -> [u8; 32] {
        let mut hasher = sha256::Hasher::new();
        hasher.update(b"OwnedCompositionReferenceV1:");
        hasher.update(law_proof_digest);
        hasher.update(b"|");
        hasher.update(owned_root_id.as_bytes());
        hasher.update(b"|");
        hasher.update(support_domain_id.as_bytes());
        *hasher.finalize().as_bytes()
    }
}
