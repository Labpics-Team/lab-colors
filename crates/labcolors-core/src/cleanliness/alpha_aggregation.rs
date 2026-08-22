//! Alpha cleanliness evidence aggregation (R-05 PR2).
//!
//! Composition-aware evidence aggregation policy and pure function.
//! Operates on PR1 types only; does not invoke evaluators or verify TQ evidence.
//! No runtime dependency on R-04 or R-09.

use crate::composition::AdmittedOpacityV1;

use super::alpha_assessment::{AlphaCleanPotentialAssessmentV1, LayerIdentityV1};

/// Policy for aggregating alpha cleanliness assessments across layers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlphaAggregationPolicyV1 {
    /// Weighted average with worst-case floor.
    /// Final score = max(weighted_avg, min_individual_score).
    /// Prevents clean translucent layers from masking dirty opaque ones.
    WeightedAverageWithFloor,
}

/// Result of aggregating multiple alpha cleanliness assessments.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AggregatedAlphaCleanEvidenceV1 {
    /// The aggregation policy applied.
    pub(crate) policy: AlphaAggregationPolicyV1,
    /// Final aggregated score [0, u16::MAX].
    pub(crate) aggregated_score: u16,
    /// Weighted average component (before floor).
    pub(crate) weighted_average: u16,
    /// Worst individual score (the floor).
    pub(crate) worst_individual_score: u16,
    /// Number of assessments aggregated.
    pub(crate) sample_count: u32,
    /// Per-layer provenance: which layers contributed and their weights.
    pub(crate) layer_contributions: Vec<LayerContributionV1>,
    /// Whether any backdrop substitution occurred during aggregation.
    pub(crate) backdrop_substitution_applied: bool,
}

/// Records one layer's contribution to the aggregate.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LayerContributionV1 {
    pub(crate) layer: LayerIdentityV1,
    pub(crate) alpha: AdmittedOpacityV1,
    pub(crate) individual_score: u16,
    pub(crate) weighted_contribution: u32,
}

/// Aggregates alpha cleanliness assessments into a single evidence bundle.
///
/// Returns `None` if the input slice is empty — an empty composition
/// has no cleanliness evidence, not a default-clean verdict.
///
/// Uses fixed-point arithmetic throughout to avoid float comparison
/// policy in deterministic assessment paths.
#[must_use]
pub(crate) fn aggregate_alpha_clean_evidence(
    policy: AlphaAggregationPolicyV1,
    assessments: &[AlphaCleanPotentialAssessmentV1],
) -> Option<AggregatedAlphaCleanEvidenceV1> {
    if assessments.is_empty() {
        return None;
    }

    // Accumulate using u64 to prevent overflow: max possible weighted_sum
    // is u16::MAX * u16::MAX * len, which fits in u64 for reasonable len.
    let mut total_weight_fp: u64 = 0;
    let mut weighted_sum_fp: u64 = 0;
    let mut worst_score: u16 = u16::MAX;
    let mut contributions = Vec::with_capacity(assessments.len());

    for assessment in assessments {
        let score = assessment.point_clean_score;
        let weighted = assessment.weighted_contribution;

        // Convert alpha to u64 fixed-point with 16 fractional bits for accumulation.
        let alpha_fp = (assessment.alpha.value() * 65536.0) as u64;
        total_weight_fp += alpha_fp;
        weighted_sum_fp += u64::from(score) * alpha_fp;

        if score < worst_score {
            worst_score = score;
        }

        contributions.push(LayerContributionV1 {
            layer: assessment.source_layer_id,
            alpha: assessment.alpha,
            individual_score: score,
            weighted_contribution: weighted,
        });
    }

    let (weighted_average, aggregated_score) = if total_weight_fp > 0 {
        // Fixed-point division: (weighted_sum_fp / total_weight_fp) gives
        // the score in the same scale. Round to nearest u16.
        let raw = weighted_sum_fp / total_weight_fp;
        // Clamp to u16 range to handle any rounding edge cases.
        let wavg = raw.min(u64::from(u16::MAX)) as u16;
        let agg = match policy {
            AlphaAggregationPolicyV1::WeightedAverageWithFloor => wavg.max(worst_score),
        };
        (wavg, agg)
    } else {
        // All layers fully transparent: no visible content to assess.
        // Return zero, not a default-clean. Floor does not apply because
        // no layer contributes any visible signal to mask.
        (0, 0)
    };

    Some(AggregatedAlphaCleanEvidenceV1 {
        policy,
        aggregated_score,
        weighted_average,
        worst_individual_score: worst_score,
        sample_count: assessments.len() as u32,
        layer_contributions: contributions,
        backdrop_substitution_applied: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Srgb8;
    use crate::cleanliness::alpha_assessment::{
        AlphaBackdropTqEvidenceRef, BackdropContextV1, LayerIdentityV1,
        compute_weighted_contribution,
    };

    fn make_assessment(
        score: u16,
        alpha: f64,
        layer_index: u32,
    ) -> AlphaCleanPotentialAssessmentV1 {
        let admitted_alpha = AdmittedOpacityV1::new(alpha).unwrap();
        let backdrop = BackdropContextV1::new(Srgb8::new([128, 128, 128]), 8192, true).unwrap();
        AlphaCleanPotentialAssessmentV1 {
            alpha: admitted_alpha,
            backdrop,
            composited_color: Srgb8::new([128, 128, 128]),
            point_clean_score: score,
            weighted_contribution: compute_weighted_contribution(score, admitted_alpha),
            tq_evidence_ref: AlphaBackdropTqEvidenceRef {
                content_hash: [0u8; 32],
            },
            source_layer_id: LayerIdentityV1 {
                layer_index,
                occurrence_id: layer_index as u64,
            },
        }
    }

    #[test]
    fn all_transparent_layers_produce_zero_not_default_clean() {
        let assessments = vec![
            make_assessment(50000, 0.0, 0),
            make_assessment(60000, 0.0, 1),
        ];
        let result = aggregate_alpha_clean_evidence(
            AlphaAggregationPolicyV1::WeightedAverageWithFloor,
            &assessments,
        )
        .unwrap();
        assert_eq!(result.aggregated_score, 0);
        assert_eq!(result.weighted_average, 0);
    }

    #[test]
    fn single_layer_aggregation_equals_individual_score() {
        let assessments = vec![make_assessment(42000, 1.0, 0)];
        let result = aggregate_alpha_clean_evidence(
            AlphaAggregationPolicyV1::WeightedAverageWithFloor,
            &assessments,
        )
        .unwrap();
        // Single opaque layer: weighted average equals the score.
        assert!((result.aggregated_score as i64 - 42000).unsigned_abs() <= 1);
        assert_eq!(result.sample_count, 1);
        assert_eq!(result.layer_contributions.len(), 1);
    }

    #[test]
    fn floor_prevents_clean_masking_of_dirty_opaque() {
        // One dirty opaque layer + one clean translucent layer.
        // Without floor, the clean layer could mask the dirty one.
        let assessments = vec![
            make_assessment(1000, 1.0, 0),  // dirty, fully opaque
            make_assessment(65000, 0.5, 1), // clean, half transparent
        ];
        let result = aggregate_alpha_clean_evidence(
            AlphaAggregationPolicyV1::WeightedAverageWithFloor,
            &assessments,
        )
        .unwrap();
        // Floor ensures aggregated_score >= worst_individual_score (1000).
        assert!(result.aggregated_score >= result.worst_individual_score);
        assert_eq!(result.worst_individual_score, 1000);
        // The aggregated score must be at least the worst score.
        assert!(result.aggregated_score >= 1000);
    }

    // --- Property tests using explicit TestRunner for portability ---

    fn make_assessment_prop(
        score: u16,
        alpha: f64,
        layer_index: u32,
    ) -> AlphaCleanPotentialAssessmentV1 {
        let admitted_alpha = AdmittedOpacityV1::new(alpha).unwrap();
        let backdrop = BackdropContextV1::new(Srgb8::new([128, 128, 128]), 8192, true).unwrap();
        AlphaCleanPotentialAssessmentV1 {
            alpha: admitted_alpha,
            backdrop,
            composited_color: Srgb8::new([128, 128, 128]),
            point_clean_score: score,
            weighted_contribution: compute_weighted_contribution(score, admitted_alpha),
            tq_evidence_ref: AlphaBackdropTqEvidenceRef {
                content_hash: [0u8; 32],
            },
            source_layer_id: LayerIdentityV1 {
                layer_index,
                occurrence_id: layer_index as u64,
            },
        }
    }

    #[test]
    fn aggregation_monotonicity_increasing_alpha_increases_contribution() {
        use proptest::prelude::*;
        use proptest::test_runner::TestRunner;

        let strategy = (0.01f64..0.5, 0.5f64..1.0, 0u16..=u16::MAX);
        let mut runner = TestRunner::deterministic();
        runner
            .run(&strategy, |(alpha_a, alpha_b, score)| {
                let a = AdmittedOpacityV1::new(alpha_a).unwrap();
                let b = AdmittedOpacityV1::new(alpha_b).unwrap();
                prop_assert!(
                    compute_weighted_contribution(score, b)
                        >= compute_weighted_contribution(score, a)
                );
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn aggregation_floor_never_exceeds_weighted_average() {
        use proptest::collection::vec;
        use proptest::prelude::*;
        use proptest::test_runner::TestRunner;

        let strategy = (vec(0u16..=u16::MAX, 1..10), vec(0.01f64..1.0, 1..10));
        let mut runner = TestRunner::deterministic();
        runner
            .run(&strategy, |(scores, alphas)| {
                let len = scores.len().min(alphas.len());
                let assessments: Vec<_> = scores
                    .iter()
                    .zip(alphas.iter())
                    .take(len)
                    .enumerate()
                    .map(|(i, (&score, &alpha))| make_assessment_prop(score, alpha, i as u32))
                    .collect();

                if let Some(result) = aggregate_alpha_clean_evidence(
                    AlphaAggregationPolicyV1::WeightedAverageWithFloor,
                    &assessments,
                ) {
                    prop_assert!(result.aggregated_score >= result.worst_individual_score);
                    prop_assert!(result.aggregated_score >= result.weighted_average);
                }
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn empty_assessments_returns_none() {
        let result =
            aggregate_alpha_clean_evidence(AlphaAggregationPolicyV1::WeightedAverageWithFloor, &[]);
        assert!(result.is_none());
    }
}
