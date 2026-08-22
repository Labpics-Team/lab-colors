use super::*;

fn cultural_profile() -> SentimentProfileV1 {
    SentimentProfileV1::CulturalPreference {
        cultural_context: "ja-JP".into(),
        source_reference: "10.1002/col.20044".into(),
    }
}

fn demographic_profile() -> SentimentProfileV1 {
    SentimentProfileV1::DemographicAffinity {
        cohort_id: "age_18-25_US".into(),
        sample_size: 200,
        methodology: "forced-choice pairwise comparison".into(),
    }
}

fn contextual_profile() -> SentimentProfileV1 {
    SentimentProfileV1::ContextualMood {
        domain: "food-ui".into(),
        mood_label: "warmth".into(),
    }
}

fn custom_profile() -> SentimentProfileV1 {
    SentimentProfileV1::Custom {
        custom_id: "org.test.v1".into(),
    }
}

fn evidence() -> SentimentEvidenceV1 {
    SentimentEvidenceV1 {
        appearance_mode_id: "standard-daylight".into(),
        adaptation_state: "photopic".into(),
        chromatic_context: Some("blue-dominant".into()),
        luminance_context: Some("daylight-equivalent".into()),
    }
}

// --- Happy paths ---

#[test]
fn valid_cultural_preference_assessment_constructs() {
    let result = SentimentAssessmentV1::new(cultural_profile(), evidence(), 0.72, 0.85);
    assert!(result.is_ok());
    let a = result.unwrap();
    assert_eq!(a.valence, 0.72);
    assert_eq!(a.confidence, 0.85);
}

#[test]
fn valid_demographic_affinity_assessment_constructs() {
    let result = SentimentAssessmentV1::new(demographic_profile(), evidence(), -0.3, 0.9);
    assert!(result.is_ok());
}

#[test]
fn valid_contextual_mood_assessment_constructs() {
    let result = SentimentAssessmentV1::new(contextual_profile(), evidence(), 0.5, 0.5);
    assert!(result.is_ok());
}

#[test]
fn valid_custom_assessment_constructs() {
    let result = SentimentAssessmentV1::new(custom_profile(), evidence(), 0.0, 1.0);
    assert!(result.is_ok());
}

// --- Valence gates ---

#[test]
fn valence_out_of_range_rejected() {
    assert_eq!(
        SentimentAssessmentV1::new(contextual_profile(), evidence(), 1.5, 0.5),
        Err(SentimentAssessmentError::ValenceOutOfRange)
    );
}

#[test]
fn valence_negative_out_of_range_rejected() {
    assert_eq!(
        SentimentAssessmentV1::new(contextual_profile(), evidence(), -1.5, 0.5),
        Err(SentimentAssessmentError::ValenceOutOfRange)
    );
}

#[test]
fn nan_valence_rejected() {
    assert_eq!(
        SentimentAssessmentV1::new(custom_profile(), evidence(), f64::NAN, 0.5),
        Err(SentimentAssessmentError::ValenceNonFinite)
    );
}

#[test]
fn inf_valence_rejected() {
    assert_eq!(
        SentimentAssessmentV1::new(custom_profile(), evidence(), f64::INFINITY, 0.5),
        Err(SentimentAssessmentError::ValenceNonFinite)
    );
}

#[test]
fn boundary_valence_positive_accepted() {
    assert!(SentimentAssessmentV1::new(contextual_profile(), evidence(), 1.0, 0.5).is_ok());
}

#[test]
fn boundary_valence_negative_accepted() {
    assert!(SentimentAssessmentV1::new(contextual_profile(), evidence(), -1.0, 0.5).is_ok());
}

// --- Confidence gates ---

#[test]
fn confidence_out_of_range_rejected() {
    assert_eq!(
        SentimentAssessmentV1::new(contextual_profile(), evidence(), 0.0, 1.5),
        Err(SentimentAssessmentError::ConfidenceOutOfRange)
    );
}

#[test]
fn negative_confidence_rejected() {
    assert_eq!(
        SentimentAssessmentV1::new(contextual_profile(), evidence(), 0.0, -0.1),
        Err(SentimentAssessmentError::ConfidenceOutOfRange)
    );
}

#[test]
fn nan_confidence_rejected() {
    assert_eq!(
        SentimentAssessmentV1::new(custom_profile(), evidence(), 0.0, f64::NAN),
        Err(SentimentAssessmentError::ConfidenceNonFinite)
    );
}

#[test]
fn zero_confidence_is_valid() {
    assert!(SentimentAssessmentV1::new(demographic_profile(), evidence(), -0.3, 0.0).is_ok());
}

// --- Profile field gates ---

#[test]
fn empty_cultural_context_rejected() {
    let profile = SentimentProfileV1::CulturalPreference {
        cultural_context: String::new(),
        source_reference: "doi:test".into(),
    };
    assert_eq!(
        SentimentAssessmentV1::new(profile, evidence(), 0.0, 0.5),
        Err(SentimentAssessmentError::EmptyCulturalContext)
    );
}

#[test]
fn empty_source_reference_rejected() {
    let profile = SentimentProfileV1::CulturalPreference {
        cultural_context: "ja-JP".into(),
        source_reference: String::new(),
    };
    assert_eq!(
        SentimentAssessmentV1::new(profile, evidence(), 0.0, 0.5),
        Err(SentimentAssessmentError::EmptySourceReference)
    );
}

#[test]
fn empty_cohort_id_rejected() {
    let profile = SentimentProfileV1::DemographicAffinity {
        cohort_id: String::new(),
        sample_size: 100,
        methodology: "test".into(),
    };
    assert_eq!(
        SentimentAssessmentV1::new(profile, evidence(), 0.0, 0.5),
        Err(SentimentAssessmentError::EmptyCohortId)
    );
}

#[test]
fn zero_sample_size_rejected() {
    let profile = SentimentProfileV1::DemographicAffinity {
        cohort_id: "test".into(),
        sample_size: 0,
        methodology: "test".into(),
    };
    assert_eq!(
        SentimentAssessmentV1::new(profile, evidence(), 0.0, 0.5),
        Err(SentimentAssessmentError::ZeroSampleSize)
    );
}

#[test]
fn empty_methodology_rejected() {
    let profile = SentimentProfileV1::DemographicAffinity {
        cohort_id: "test".into(),
        sample_size: 100,
        methodology: String::new(),
    };
    assert_eq!(
        SentimentAssessmentV1::new(profile, evidence(), 0.0, 0.5),
        Err(SentimentAssessmentError::EmptyMethodology)
    );
}

#[test]
fn empty_domain_rejected() {
    let profile = SentimentProfileV1::ContextualMood {
        domain: String::new(),
        mood_label: "warmth".into(),
    };
    assert_eq!(
        SentimentAssessmentV1::new(profile, evidence(), 0.0, 0.5),
        Err(SentimentAssessmentError::EmptyDomain)
    );
}

#[test]
fn empty_mood_label_rejected() {
    let profile = SentimentProfileV1::ContextualMood {
        domain: "food-ui".into(),
        mood_label: String::new(),
    };
    assert_eq!(
        SentimentAssessmentV1::new(profile, evidence(), 0.0, 0.5),
        Err(SentimentAssessmentError::EmptyMoodLabel)
    );
}

#[test]
fn empty_custom_id_rejected() {
    let profile = SentimentProfileV1::Custom {
        custom_id: String::new(),
    };
    assert_eq!(
        SentimentAssessmentV1::new(profile, evidence(), 0.0, 0.5),
        Err(SentimentAssessmentError::EmptyCustomId)
    );
}

// --- Serde round-trip ---

#[test]
fn serde_round_trip_cultural_preference() {
    let assessment = SentimentAssessmentV1::new(cultural_profile(), evidence(), 0.72, 0.85)
        .expect("valid construction");
    let bytes = rmp_serde::to_vec(&assessment).expect("serialize");
    let decoded: SentimentAssessmentV1 = rmp_serde::from_slice(&bytes).expect("deserialize");
    assert_eq!(assessment, decoded);
}

#[test]
fn serde_round_trip_all_variants() {
    let profiles = [
        cultural_profile(),
        demographic_profile(),
        contextual_profile(),
        custom_profile(),
    ];
    for profile in profiles {
        let assessment =
            SentimentAssessmentV1::new(profile, evidence(), 0.5, 0.5).expect("valid construction");
        let bytes = rmp_serde::to_vec(&assessment).expect("serialize");
        let decoded: SentimentAssessmentV1 = rmp_serde::from_slice(&bytes).expect("deserialize");
        assert_eq!(assessment, decoded);
    }
}
