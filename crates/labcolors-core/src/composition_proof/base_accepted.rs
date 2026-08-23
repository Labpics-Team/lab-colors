use crate::sha256;

/// Proves that the base color is accepted over the entire support domain.
/// "Base accepted" means the base passes all admission predicates for every
/// point in the support domain under the specified evaluation context.
/// Content-addressed via digest of (support_domain_id, evaluation_context_id, predicate_set_id).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged before PR-C field proof engine")]
pub(crate) struct BaseAcceptedOnSupportCertificateV1 {
    /// Identifier of the support domain over which acceptance was proven.
    pub(crate) support_domain_id: String,
    /// Identifier of the evaluation context (renderer profile, output space, etc.).
    pub(crate) evaluation_context_id: String,
    /// Identifier of the predicate set applied. Opaque; defined by admission registry.
    pub(crate) predicate_set_id: String,
    /// Number of points/regions verified. Zero is invalid.
    pub(crate) verified_count: u64,
    /// Content-addressed identity.
    pub(crate) digest: [u8; 32],
}

/// Errors returned when constructing a [`BaseAcceptedOnSupportCertificateV1`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged")]
pub(crate) enum BaseAcceptedOnSupportError {
    EmptySupportDomainId,
    EmptyEvaluationContextId,
    EmptyPredicateSetId,
    ZeroVerifiedCount,
}

impl BaseAcceptedOnSupportCertificateV1 {
    /// Construct a validated base-accepted certificate. Computes content-addressed digest.
    pub(crate) fn new(
        support_domain_id: String,
        evaluation_context_id: String,
        predicate_set_id: String,
        verified_count: u64,
    ) -> Result<Self, BaseAcceptedOnSupportError> {
        if support_domain_id.is_empty() {
            return Err(BaseAcceptedOnSupportError::EmptySupportDomainId);
        }
        if evaluation_context_id.is_empty() {
            return Err(BaseAcceptedOnSupportError::EmptyEvaluationContextId);
        }
        if predicate_set_id.is_empty() {
            return Err(BaseAcceptedOnSupportError::EmptyPredicateSetId);
        }
        if verified_count == 0 {
            return Err(BaseAcceptedOnSupportError::ZeroVerifiedCount);
        }

        let digest = Self::compute_digest(
            &support_domain_id,
            &evaluation_context_id,
            &predicate_set_id,
            verified_count,
        );

        Ok(Self {
            support_domain_id,
            evaluation_context_id,
            predicate_set_id,
            verified_count,
            digest,
        })
    }

    fn compute_digest(
        support_domain_id: &str,
        evaluation_context_id: &str,
        predicate_set_id: &str,
        verified_count: u64,
    ) -> [u8; 32] {
        let mut hasher = sha256::Hasher::new();
        hasher.update(b"BaseAcceptedOnSupportCertificateV1:");
        hasher.update(support_domain_id.as_bytes());
        hasher.update(b"|");
        hasher.update(evaluation_context_id.as_bytes());
        hasher.update(b"|");
        hasher.update(predicate_set_id.as_bytes());
        hasher.update(b"|");
        hasher.update(&verified_count.to_le_bytes());
        *hasher.finalize().as_bytes()
    }
}
