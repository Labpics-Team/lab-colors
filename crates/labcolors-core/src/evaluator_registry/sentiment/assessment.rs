use super::{SentimentEvidenceV1, SentimentProfileV1};
use serde::{Deserialize, Serialize};

/// Errors returned when constructing a [`SentimentAssessmentV1`] with invalid bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SentimentAssessmentError {
    ValenceNonFinite,
    ValenceOutOfRange,
    ConfidenceNonFinite,
    ConfidenceOutOfRange,
    EmptyCulturalContext,
    EmptySourceReference,
    EmptyCohortId,
    ZeroSampleSize,
    EmptyMethodology,
    EmptyDomain,
    EmptyMoodLabel,
    EmptyCustomId,
}

/// A single sentiment assessment produced by an admitted evaluator.
/// Carries the profile that generated it, the evidence payload, and
/// scalar valence/confidence for downstream weighting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SentimentAssessmentV1 {
    /// The profile that produced this assessment.
    pub profile: SentimentProfileV1,
    /// Evidence payload extracted from the LPC Appearance channel.
    pub evidence: SentimentEvidenceV1,
    /// Scalar valence in [-1.0, 1.0]. Negative = aversion, positive = affinity.
    /// NaN and infinities are construction-time errors.
    pub valence: f64,
    /// Confidence weight in [0.0, 1.0]. Zero means "no usable signal".
    /// Used by downstream composers to weight this assessment against others.
    pub confidence: f64,
}

impl SentimentAssessmentV1 {
    /// Construct a validated assessment. Returns error if valence/confidence
    /// are out of range or non-finite, or if profile fields are empty.
    pub fn new(
        profile: SentimentProfileV1,
        evidence: SentimentEvidenceV1,
        valence: f64,
        confidence: f64,
    ) -> Result<Self, SentimentAssessmentError> {
        // --- Numeric bound checks ---
        if !valence.is_finite() {
            return Err(SentimentAssessmentError::ValenceNonFinite);
        }
        if !(-1.0..=1.0).contains(&valence) {
            return Err(SentimentAssessmentError::ValenceOutOfRange);
        }
        if !confidence.is_finite() {
            return Err(SentimentAssessmentError::ConfidenceNonFinite);
        }
        if !(0.0..=1.0).contains(&confidence) {
            return Err(SentimentAssessmentError::ConfidenceOutOfRange);
        }

        // --- Profile-specific string/nonzero checks ---
        match &profile {
            SentimentProfileV1::CulturalPreference {
                cultural_context,
                source_reference,
            } => {
                if cultural_context.is_empty() {
                    return Err(SentimentAssessmentError::EmptyCulturalContext);
                }
                if source_reference.is_empty() {
                    return Err(SentimentAssessmentError::EmptySourceReference);
                }
            }
            SentimentProfileV1::DemographicAffinity {
                cohort_id,
                sample_size,
                methodology,
            } => {
                if cohort_id.is_empty() {
                    return Err(SentimentAssessmentError::EmptyCohortId);
                }
                if *sample_size == 0 {
                    return Err(SentimentAssessmentError::ZeroSampleSize);
                }
                if methodology.is_empty() {
                    return Err(SentimentAssessmentError::EmptyMethodology);
                }
            }
            SentimentProfileV1::ContextualMood { domain, mood_label } => {
                if domain.is_empty() {
                    return Err(SentimentAssessmentError::EmptyDomain);
                }
                if mood_label.is_empty() {
                    return Err(SentimentAssessmentError::EmptyMoodLabel);
                }
            }
            SentimentProfileV1::Custom { custom_id } => {
                if custom_id.is_empty() {
                    return Err(SentimentAssessmentError::EmptyCustomId);
                }
            }
        }

        Ok(Self {
            profile,
            evidence,
            valence,
            confidence,
        })
    }
}