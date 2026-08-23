use crate::sha256;

/// Proof that a field's coverage has been verified over its entire domain.
/// Per roadmap §R3c: gradient stops/average/sample do NOT count as whole-field proof.
/// Only enumeration, analytic extrema, interval enclosure, or exact renderer-bound raster qualify.
/// Content-addressed via digest of (field_identity, method, coverage_descriptor).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged before PR-C field proof engine")]
pub(crate) struct WholeFieldCoverageProofV1 {
    /// Identity of the field whose coverage was proven.
    /// Opaque; assigned by field registry (field_effect.rs infrastructure).
    pub(crate) field_identity: String,
    /// Method used to prove whole-field coverage.
    pub(crate) method: WholeFieldCoverageMethodV1,
    /// Canonical descriptor of the coverage domain. Format depends on method.
    pub(crate) coverage_descriptor: String,
    /// Content-addressed identity.
    pub(crate) digest: [u8; 32],
}

/// Discriminated method for whole-field coverage verification.
/// Each variant carries method-specific evidence parameters.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged before PR-C field proof engine")]
pub(crate) enum WholeFieldCoverageMethodV1 {
    /// Exhaustive enumeration of all field occurrences.
    Enumeration {
        /// Total number of enumerated occurrences.
        occurrence_count: u64,
    },
    /// Analytic proof via extrema computation over the field's parameter space.
    AnalyticExtrema {
        /// Number of critical points analyzed.
        critical_point_count: u64,
    },
    /// Interval enclosure proof: field range enclosed within certified bounds.
    IntervalEnclosure {
        /// Number of sub-intervals in the enclosure.
        interval_count: u64,
    },
    /// Exact renderer-bound raster verification at specified resolution.
    RendererBoundRaster {
        /// Renderer profile identifier.
        renderer_profile_id: String,
        /// Raster width in pixels.
        width: u32,
        /// Raster height in pixels.
        height: u32,
    },
}

/// Errors returned when constructing a [`WholeFieldCoverageProofV1`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code, reason = "R-08 PR-A staged")]
pub(crate) enum WholeFieldCoverageError {
    EmptyFieldIdentity,
    EmptyCoverageDescriptor,
    ZeroOccurrenceCount,
    ZeroCriticalPointCount,
    ZeroIntervalCount,
    EmptyRendererProfileId,
    ZeroRasterDimension,
}

impl WholeFieldCoverageProofV1 {
    /// Construct a validated whole-field coverage proof. Computes content-addressed digest.
    pub(crate) fn new(
        field_identity: String,
        method: WholeFieldCoverageMethodV1,
        coverage_descriptor: String,
    ) -> Result<Self, WholeFieldCoverageError> {
        if field_identity.is_empty() {
            return Err(WholeFieldCoverageError::EmptyFieldIdentity);
        }
        if coverage_descriptor.is_empty() {
            return Err(WholeFieldCoverageError::EmptyCoverageDescriptor);
        }

        match &method {
            WholeFieldCoverageMethodV1::Enumeration { occurrence_count } => {
                if *occurrence_count == 0 {
                    return Err(WholeFieldCoverageError::ZeroOccurrenceCount);
                }
            }
            WholeFieldCoverageMethodV1::AnalyticExtrema {
                critical_point_count,
            } => {
                if *critical_point_count == 0 {
                    return Err(WholeFieldCoverageError::ZeroCriticalPointCount);
                }
            }
            WholeFieldCoverageMethodV1::IntervalEnclosure { interval_count } => {
                if *interval_count == 0 {
                    return Err(WholeFieldCoverageError::ZeroIntervalCount);
                }
            }
            WholeFieldCoverageMethodV1::RendererBoundRaster {
                renderer_profile_id,
                width,
                height,
            } => {
                if renderer_profile_id.is_empty() {
                    return Err(WholeFieldCoverageError::EmptyRendererProfileId);
                }
                if *width == 0 || *height == 0 {
                    return Err(WholeFieldCoverageError::ZeroRasterDimension);
                }
            }
        }

        let digest = Self::compute_digest(&field_identity, &method, &coverage_descriptor);

        Ok(Self {
            field_identity,
            method,
            coverage_descriptor,
            digest,
        })
    }

    fn compute_digest(
        field_identity: &str,
        method: &WholeFieldCoverageMethodV1,
        coverage_descriptor: &str,
    ) -> [u8; 32] {
        let mut hasher = sha256::Hasher::new();
        hasher.update(b"WholeFieldCoverageProofV1:");
        hasher.update(field_identity.as_bytes());
        hasher.update(b"|");
        match method {
            WholeFieldCoverageMethodV1::Enumeration { occurrence_count } => {
                hasher.update(b"enumeration:");
                hasher.update(&occurrence_count.to_le_bytes());
            }
            WholeFieldCoverageMethodV1::AnalyticExtrema {
                critical_point_count,
            } => {
                hasher.update(b"analytic_extrema:");
                hasher.update(&critical_point_count.to_le_bytes());
            }
            WholeFieldCoverageMethodV1::IntervalEnclosure { interval_count } => {
                hasher.update(b"interval_enclosure:");
                hasher.update(&interval_count.to_le_bytes());
            }
            WholeFieldCoverageMethodV1::RendererBoundRaster {
                renderer_profile_id,
                width,
                height,
            } => {
                hasher.update(b"renderer_bound_raster:");
                hasher.update(renderer_profile_id.as_bytes());
                hasher.update(b":");
                hasher.update(&width.to_le_bytes());
                hasher.update(b":");
                hasher.update(&height.to_le_bytes());
            }
        }
        hasher.update(b"|");
        hasher.update(coverage_descriptor.as_bytes());
        *hasher.finalize().as_bytes()
    }
}
