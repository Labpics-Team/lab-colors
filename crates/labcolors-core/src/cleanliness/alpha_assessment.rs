#![allow(dead_code)] // V1 cleanliness alpha-assessment API staged for future consumers.
//! Alpha cleanliness assessment types (R-05 PR1).
//!
//! Pure data types for alpha-weighted cleanliness evidence. No runtime
//! dependency on R-04 or R-09 evaluators — these types are mergeable
//! before those blockers land.

use crate::Srgb8;
use crate::composition::AdmittedOpacityV1;

/// Backdrop context for alpha cleanliness assessment.
///
/// Carries the physical backdrop color and its pre-computed luminance
/// for threshold lookups. The cleanliness baseline is the point-level
/// CleanPotential verdict for the backdrop itself — if the backdrop
/// is not clean, alpha assessment over it cannot be assessed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BackdropContextV1 {
    /// The backdrop color in encoded sRGB8.
    backdrop_color: Srgb8,
    /// Pre-computed relative luminance of the backdrop (0..1 scaled as u16).
    backdrop_luminance_fp: u16,
}

impl BackdropContextV1 {
    /// Constructs a validated backdrop context.
    ///
    /// Returns `None` if the backdrop did not pass point-level cleanliness,
    /// making alpha assessment over it structurally impossible rather than
    /// silently degraded.
    #[must_use]
    pub(crate) fn new(
        backdrop_color: Srgb8,
        backdrop_luminance_fp: u16,
        backdrop_clean: bool,
    ) -> Option<Self> {
        if !backdrop_clean {
            return None;
        }
        Some(Self {
            backdrop_color,
            backdrop_luminance_fp,
        })
    }

    #[must_use]
    pub(crate) const fn color(self) -> Srgb8 {
        self.backdrop_color
    }

    #[must_use]
    pub(crate) const fn luminance_fp(self) -> u16 {
        self.backdrop_luminance_fp
    }
}

/// Opaque reference to R-09 TQ evidence. Content-addressed so that
/// stale or invalidated evidence cannot be silently reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AlphaBackdropTqEvidenceRef {
    pub(crate) content_hash: [u8; 32],
}

/// Identifies the source layer within a composition stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LayerIdentityV1 {
    pub(crate) layer_index: u32,
    pub(crate) occurrence_id: u64,
}

/// Alpha-weighted cleanliness assessment for a single translucent occurrence.
///
/// This is NOT a verdict — it is one weighted evidence sample that feeds
/// into the aggregation policy. The final CleanPass/CleanDecision for an
/// alpha composition lives at the field/attachment level (R-06).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AlphaCleanPotentialAssessmentV1 {
    /// The opacity at which this assessment was made.
    pub(crate) alpha: AdmittedOpacityV1,
    /// The backdrop context used for this assessment.
    pub(crate) backdrop: BackdropContextV1,
    /// The composited color (tint over backdrop) that was assessed.
    pub(crate) composited_color: Srgb8,
    /// Point-level CleanPotential score for the composited color.
    /// Scaled [0, u16::MAX] where MAX = fully clean.
    pub(crate) point_clean_score: u16,
    /// Alpha-weighted contribution to aggregate cleanliness.
    /// Computed as `point_clean_score * alpha.value()` in fixed-point.
    pub(crate) weighted_contribution: u32,
    /// Reference to the R-09 TQ evidence that certified this composition
    /// is technically valid. Without this, the assessment is inadmissible.
    pub(crate) tq_evidence_ref: AlphaBackdropTqEvidenceRef,
    /// Provenance: which layer/occurrence produced this assessment.
    pub(crate) source_layer_id: LayerIdentityV1,
}

/// Computes the alpha-weighted contribution of a single cleanliness score.
///
/// Uses fixed-point arithmetic with u32 intermediates to avoid float
/// comparison policy in deterministic assessment paths. The result is
/// `score * alpha` scaled to preserve precision.
#[must_use]
pub(crate) fn compute_weighted_contribution(score: u16, alpha: AdmittedOpacityV1) -> u32 {
    // Fixed-point: multiply score by alpha represented as u16 fraction.
    // alpha_bits encodes f64 in [0,1]; we convert to a u32 fraction
    // with denominator 65536 for precise integer multiplication.
    let alpha_val = alpha.value();
    // Scale alpha to u32 fixed-point with 16 fractional bits.
    let alpha_fp = (alpha_val * 65536.0) as u32;
    let product = u32::from(score).saturating_mul(alpha_fp);
    product >> 16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backdrop_context_rejects_unclean_backdrop() {
        let color = Srgb8::new([128, 128, 128]);
        assert!(BackdropContextV1::new(color, 8192, false).is_none());
        assert!(BackdropContextV1::new(color, 8192, true).is_some());
    }

    #[test]
    fn weighted_contribution_is_monotonic_in_alpha() {
        let score: u16 = 50000;
        let alpha_low = AdmittedOpacityV1::new(0.25).unwrap();
        let alpha_high = AdmittedOpacityV1::new(0.75).unwrap();
        let contrib_low = compute_weighted_contribution(score, alpha_low);
        let contrib_high = compute_weighted_contribution(score, alpha_high);
        assert!(contrib_high > contrib_low);
    }

    #[test]
    fn weighted_contribution_at_opaque_equals_full_score() {
        let score: u16 = 50000;
        let opaque = AdmittedOpacityV1::OPAQUE;
        let contrib = compute_weighted_contribution(score, opaque);
        // At alpha=1.0, weighted contribution should equal the score
        // within fixed-point rounding tolerance.
        assert!((contrib as i64 - i64::from(score)).unsigned_abs() <= 1);
    }

    #[test]
    fn weighted_contribution_at_transparent_is_zero() {
        let score: u16 = 50000;
        let transparent = AdmittedOpacityV1::TRANSPARENT;
        let contrib = compute_weighted_contribution(score, transparent);
        assert_eq!(contrib, 0);
    }
}
