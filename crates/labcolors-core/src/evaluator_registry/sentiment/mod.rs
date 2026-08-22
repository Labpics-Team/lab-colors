//! Versioned sentiment/affective assessment types for the evaluator registry.
//! These model color preference, emotional response, and cultural association
//! as soft evidence — never hard constraints. Absence of sentiment data does not
//! block any downstream node including R-08 cleanliness certification.
//!
//! Wire-stable via MessagePack with leading u16 version tag (0x0001).
//! Independent versioning from evaluator protocol and F-03 metadata types.

mod assessment;

#[allow(unused_imports, reason = "R-02 PR-A: re-exports staged for PR-B evaluator impl")]
pub use assessment::{SentimentAssessmentError, SentimentAssessmentV1};

use serde::{Deserialize, Serialize};

/// Discriminated sentiment profile kind. Each variant carries its own
/// evidence schema because cultural preference and demographic affinity
/// have fundamentally different provenance requirements.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "profile_kind")]
pub enum SentimentProfileV1 {
    /// Preference derived from cross-cultural color studies (e.g., Ou et al. 2004,
    /// Palmer & Schloss 2010). Evidence MUST cite peer-reviewed source.
    CulturalPreference {
        /// ISO 639-1 language tag + optional region (e.g., "ja-JP", "pt-BR").
        cultural_context: String,
        /// Peer-reviewed study identifier or DOI.
        source_reference: String,
    },
    /// Affinity measured within a specific demographic cohort.
    DemographicAffinity {
        /// Cohort descriptor (e.g., "age_18-25_US", "protanopic_male").
        cohort_id: String,
        /// Sample size used to establish this affinity signal.
        sample_size: u64,
        /// Measurement methodology description.
        methodology: String,
    },
    /// Context-dependent mood/atmosphere association (e.g., "warmth for food UI",
    /// "calm for healthcare"). Not tied to a specific culture or demographic.
    ContextualMood {
        /// Application domain identifier.
        domain: String,
        /// Mood/atmosphere label.
        mood_label: String,
    },
    /// Extension point for future profile kinds not yet standardized.
    /// Admitted only through the external profile admission gate.
    Custom {
        /// Namespace-qualified identifier (e.g., "org.example.brand-affinity-v2").
        custom_id: String,
    },
}

/// Evidence extracted from the LPC Appearance channel for sentiment assessment.
/// This is the bridge between raw viewing-context data and affective interpretation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SentimentEvidenceV1 {
    /// The appearance mode under which this evidence was gathered.
    pub appearance_mode_id: String,
    /// Adaptation state at time of assessment.
    pub adaptation_state: String,
    /// Optional chromatic context that modulates the sentiment signal
    /// (e.g., surrounding dominant hue category).
    pub chromatic_context: Option<String>,
    /// Optional luminance context category (e.g., "low-light", "daylight-equivalent").
    pub luminance_context: Option<String>,
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod properties;