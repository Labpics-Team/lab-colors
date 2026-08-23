use crate::sha256;

/// Proof that the source-over composition law holds over an entire owned domain.
/// Content-addressed: identity is the SHA-256 digest of the canonical serialization
/// of (profile_id, domain_descriptor, verification_method).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged before PR-C field proof engine")]
pub(crate) struct CompositionLawProofV1 {
    /// Identifier of the composition profile this proof applies to.
    /// Must match a registered CompositionProfileV1 identity.
    pub(crate) profile_id: String,
    /// Canonical descriptor of the owned domain over which the law was verified.
    /// Format is opaque to this type; interpreted by the field proof engine (PR-C).
    pub(crate) domain_descriptor: String,
    /// Method used to verify the composition law.
    pub(crate) verification_method: CompositionLawVerificationMethodV1,
    /// Content-addressed identity. Computed at construction time.
    pub(crate) digest: [u8; 32],
}

/// Discriminated verification method for composition law proofs.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged before PR-C field proof engine")]
pub(crate) enum CompositionLawVerificationMethodV1 {
    /// Exhaustive enumeration of all points in the owned domain.
    Enumerated,
    /// Analytic proof via interval arithmetic or symbolic bounds.
    AnalyticBounds,
    /// Verified against renderer-bound raster at specified precision.
    RasterVerified {
        /// Renderer profile identifier used for rasterization.
        renderer_profile_id: String,
        /// Pixel dimensions of the verification raster.
        width: u32,
        height: u32,
    },
}

/// Errors returned when constructing a [`CompositionLawProofV1`] with invalid inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged")]
pub(crate) enum CompositionLawProofError {
    EmptyProfileId,
    EmptyDomainDescriptor,
    EmptyRendererProfileId,
    ZeroRasterDimension,
}

impl CompositionLawProofV1 {
    /// Construct a validated composition law proof. Computes content-addressed digest.
    pub(crate) fn new(
        profile_id: String,
        domain_descriptor: String,
        verification_method: CompositionLawVerificationMethodV1,
    ) -> Result<Self, CompositionLawProofError> {
        if profile_id.is_empty() {
            return Err(CompositionLawProofError::EmptyProfileId);
        }
        if domain_descriptor.is_empty() {
            return Err(CompositionLawProofError::EmptyDomainDescriptor);
        }
        if let CompositionLawVerificationMethodV1::RasterVerified {
            ref renderer_profile_id,
            width,
            height,
        } = verification_method
        {
            if renderer_profile_id.is_empty() {
                return Err(CompositionLawProofError::EmptyRendererProfileId);
            }
            if width == 0 || height == 0 {
                return Err(CompositionLawProofError::ZeroRasterDimension);
            }
        }

        let digest = Self::compute_digest(&profile_id, &domain_descriptor, &verification_method);

        Ok(Self {
            profile_id,
            domain_descriptor,
            verification_method,
            digest,
        })
    }

    fn compute_digest(
        profile_id: &str,
        domain_descriptor: &str,
        method: &CompositionLawVerificationMethodV1,
    ) -> [u8; 32] {
        let mut hasher = sha256::Hasher::new();
        hasher.update(b"CompositionLawProofV1:");
        hasher.update(profile_id.as_bytes());
        hasher.update(b"|");
        hasher.update(domain_descriptor.as_bytes());
        hasher.update(b"|");
        // Canonical method tag for digest stability
        match method {
            CompositionLawVerificationMethodV1::Enumerated => hasher.update(b"enumerated"),
            CompositionLawVerificationMethodV1::AnalyticBounds => hasher.update(b"analytic_bounds"),
            CompositionLawVerificationMethodV1::RasterVerified {
                renderer_profile_id,
                width,
                height,
            } => {
                hasher.update(b"raster_verified:");
                hasher.update(renderer_profile_id.as_bytes());
                hasher.update(b":");
                hasher.update(&width.to_le_bytes());
                hasher.update(b":");
                hasher.update(&height.to_le_bytes());
            }
        }
        *hasher.finalize().as_bytes()
    }
}
