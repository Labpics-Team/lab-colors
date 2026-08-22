use super::*;
use proptest::prelude::*;

fn evidence_fixture() -> SentimentEvidenceV1 {
    SentimentEvidenceV1 {
        appearance_mode_id: "prop".into(),
        adaptation_state: "photopic".into(),
        chromatic_context: None,
        luminance_context: None,
    }
}

proptest! {
    #[test]
    fn valence_confidence_round_trip(valence in -1.0f64..=1.0, confidence in 0.0f64..=1.0) {
        let profile = SentimentProfileV1::ContextualMood {
            domain: "prop-test".into(),
            mood_label: "neutral".into(),
        };
        let result = SentimentAssessmentV1::new(profile, evidence_fixture(), valence, confidence);
        prop_assert!(result.is_ok());
        let a = result.unwrap();
        prop_assert!(a.valence == valence);
        prop_assert!(a.confidence == confidence);
    }

    #[test]
    fn boundary_valences_accepted(v in prop_oneof![Just(-1.0f64), Just(0.0), Just(1.0)]) {
        let profile = SentimentProfileV1::Custom {
            custom_id: "boundary-test".into(),
        };
        let result = SentimentAssessmentV1::new(profile, evidence_fixture(), v, 0.5);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn boundary_confidences_accepted(c in prop_oneof![Just(0.0f64), Just(0.5), Just(1.0)]) {
        let profile = SentimentProfileV1::Custom {
            custom_id: "boundary-conf".into(),
        };
        let result = SentimentAssessmentV1::new(profile, evidence_fixture(), 0.0, c);
        prop_assert!(result.is_ok());
    }

    #[test]
    fn all_profile_variants_construct_with_valid_fields(
        variant in 0u8..4
    ) {
        let profile = match variant {
            0 => SentimentProfileV1::CulturalPreference {
                cultural_context: "en-US".into(),
                source_reference: "doi:test".into(),
            },
            1 => SentimentProfileV1::DemographicAffinity {
                cohort_id: "cohort-a".into(),
                sample_size: 50,
                methodology: "survey".into(),
            },
            2 => SentimentProfileV1::ContextualMood {
                domain: "healthcare".into(),
                mood_label: "calm".into(),
            },
            _ => SentimentProfileV1::Custom {
                custom_id: "org.example.test".into(),
            },
        };
        let result = SentimentAssessmentV1::new(profile, evidence_fixture(), 0.0, 0.5);
        prop_assert!(result.is_ok());
    }
}
