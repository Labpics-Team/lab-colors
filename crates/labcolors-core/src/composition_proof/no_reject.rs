use crate::sha256;

/// Global proof that no new rejects were introduced by the restorative auto pass.
/// This certificate covers the entire owned root closure and asserts that every
/// point that was accepted before the auto pass remains accepted after.
/// Content-addressed via digest of (owned_root_id, pre_pass_snapshot_digest, post_pass_verification_id).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged before PR-D human-clean lift")]
pub(crate) struct NoIntroducedRejectCertificateV1 {
    /// Identifier of the owned root closure this certificate covers.
    pub(crate) owned_root_id: String,
    /// Digest of the pre-pass acceptance snapshot. Proves what was accepted before.
    pub(crate) pre_pass_snapshot_digest: [u8; 32],
    /// Identifier of the post-pass verification run. Opaque; assigned by verifier.
    pub(crate) post_pass_verification_id: String,
    /// Content-addressed identity.
    pub(crate) digest: [u8; 32],
}

/// Errors returned when constructing a [`NoIntroducedRejectCertificateV1`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged")]
pub(crate) enum NoIntroducedRejectError {
    EmptyOwnedRootId,
    EmptyPostPassVerificationId,
}

impl NoIntroducedRejectCertificateV1 {
    /// Construct a validated no-introduced-reject certificate. Computes content-addressed digest.
    pub(crate) fn new(
        owned_root_id: String,
        pre_pass_snapshot_digest: [u8; 32],
        post_pass_verification_id: String,
    ) -> Result<Self, NoIntroducedRejectError> {
        if owned_root_id.is_empty() {
            return Err(NoIntroducedRejectError::EmptyOwnedRootId);
        }
        if post_pass_verification_id.is_empty() {
            return Err(NoIntroducedRejectError::EmptyPostPassVerificationId);
        }

        let digest = Self::compute_digest(
            &owned_root_id,
            &pre_pass_snapshot_digest,
            &post_pass_verification_id,
        );

        Ok(Self {
            owned_root_id,
            pre_pass_snapshot_digest,
            post_pass_verification_id,
            digest,
        })
    }

    fn compute_digest(
        owned_root_id: &str,
        pre_pass_snapshot_digest: &[u8; 32],
        post_pass_verification_id: &str,
    ) -> [u8; 32] {
        let mut hasher = sha256::Hasher::new();
        hasher.update(b"NoIntroducedRejectCertificateV1:");
        hasher.update(owned_root_id.as_bytes());
        hasher.update(b"|");
        hasher.update(pre_pass_snapshot_digest);
        hasher.update(b"|");
        hasher.update(post_pass_verification_id.as_bytes());
        *hasher.finalize().as_bytes()
    }
}
