//! Proof-bound production kernel for WCAG 2.2 final-sRGB8 assessment.
//!
//! The independent verifier pins this whole source file: byte lookup,
//! luminance assembly, criterion mapping, both threshold orientations,
//! terminal decision, strict transport parsing and sealed evidence minting.

use crate::wcag22_evidence::mint_wcag22_evidence;

use super::{
    Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22CriterionV1, Wcag22EvaluationErrorV1,
    Wcag22LuminanceBoundsQ55V1, Wcag22MeasurementV1, wcag22_profile_v1,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ThresholdV1 {
    Three,
    FourAndHalf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OrientedDecisionV1 {
    Pass,
    Fail,
    Unresolved,
}

fn threshold(criterion: Wcag22CriterionV1) -> ThresholdV1 {
    match criterion {
        Wcag22CriterionV1::Sc143TextDefault => ThresholdV1::FourAndHalf,
        Wcag22CriterionV1::Sc143TextLargeScale
        | Wcag22CriterionV1::Sc1411UiComponentOrState
        | Wcag22CriterionV1::Sc1411GraphicalObject => ThresholdV1::Three,
    }
}

pub(super) fn luminance_bounds(rgb: [u8; 3]) -> Wcag22LuminanceBoundsQ55V1 {
    let red = super::q55_data::WEIGHTED_CONTRIBUTION_BOUNDS[0][usize::from(rgb[0])];
    let green = super::q55_data::WEIGHTED_CONTRIBUTION_BOUNDS[1][usize::from(rgb[1])];
    let blue = super::q55_data::WEIGHTED_CONTRIBUTION_BOUNDS[2][usize::from(rgb[2])];
    Wcag22LuminanceBoundsQ55V1 {
        lower: red[0] + green[0] + blue[0],
        upper: red[1] + green[1] + blue[1],
    }
}

fn classify_orientation(
    lighter: Wcag22LuminanceBoundsQ55V1,
    darker: Wcag22LuminanceBoundsQ55V1,
    threshold: ThresholdV1,
) -> OrientedDecisionV1 {
    let scale = u128::from(super::q55_data::Q55_SCALE);
    let light_lower = u128::from(lighter.lower);
    let light_upper = u128::from(lighter.upper);
    let dark_lower = u128::from(darker.lower);
    let dark_upper = u128::from(darker.upper);
    // With S = Q55_SCALE, clearing denominators in
    // (L + 0.05S) / (D + 0.05S) gives
    // 3:1 => 10L >= 30D + S and 4.5:1 => 40L >= 180D + 7S.
    // Pass uses L_lower/D_upper; Fail uses the strict reverse inequality with
    // L_upper/D_lower, so neither branch relies on rounded display ratios.
    let (passes, fails) = match threshold {
        ThresholdV1::Three => (
            10 * light_lower >= 30 * dark_upper + scale,
            10 * light_upper < 30 * dark_lower + scale,
        ),
        ThresholdV1::FourAndHalf => (
            40 * light_lower >= 180 * dark_upper + 7 * scale,
            40 * light_upper < 180 * dark_lower + 7 * scale,
        ),
    };
    match (passes, fails) {
        (true, false) => OrientedDecisionV1::Pass,
        (false, true) => OrientedDecisionV1::Fail,
        (false, false) | (true, true) => OrientedDecisionV1::Unresolved,
    }
}

fn classify_pair(
    foreground: Wcag22LuminanceBoundsQ55V1,
    background: Wcag22LuminanceBoundsQ55V1,
    criterion: Wcag22CriterionV1,
) -> Option<Wcag22ApplicableDecisionV1> {
    let threshold = threshold(criterion);
    let forward = classify_orientation(foreground, background, threshold);
    let reverse = classify_orientation(background, foreground, threshold);
    if matches!(forward, OrientedDecisionV1::Pass) || matches!(reverse, OrientedDecisionV1::Pass) {
        Some(Wcag22ApplicableDecisionV1::Pass)
    } else if matches!(forward, OrientedDecisionV1::Fail)
        && matches!(reverse, OrientedDecisionV1::Fail)
    {
        Some(Wcag22ApplicableDecisionV1::Fail)
    } else {
        None
    }
}

/// Evaluate one final foreground/background sRGB8 occurrence.
///
/// Applicability is explicit in `criterion`; Core never infers it from token,
/// role, typography name or polarity. The function is fail-closed and cannot
/// panic for public byte input.
pub fn evaluate_wcag22_srgb8(
    foreground: [u8; 3],
    background: [u8; 3],
    criterion: Wcag22CriterionV1,
) -> Result<Wcag22AssessmentV1, Wcag22EvaluationErrorV1> {
    let foreground_luminance = luminance_bounds(foreground);
    let background_luminance = luminance_bounds(background);
    let decision = classify_pair(foreground_luminance, background_luminance, criterion).ok_or(
        Wcag22EvaluationErrorV1::ArtifactInvariantViolation {
            criterion,
            foreground,
            background,
        },
    )?;
    let profile = wcag22_profile_v1();
    let evidence =
        mint_wcag22_evidence().map_err(Wcag22EvaluationErrorV1::EvidenceRegistryMismatch)?;
    Ok(Wcag22AssessmentV1::Evaluated {
        profile_id: profile.profile_id,
        criterion,
        measurement: Wcag22MeasurementV1 {
            foreground,
            background,
            foreground_luminance,
            background_luminance,
        },
        decision,
        evidence,
    })
}

/// Parse two exact `#RRGGBB` transports and evaluate their final byte values.
///
/// The parser is the core sRGB SSOT; adapters must call this function rather
/// than reconstruct WCAG math or hex parsing in JavaScript/Swift.
pub fn evaluate_wcag22_hex(
    foreground: &str,
    background: &str,
    criterion: Wcag22CriterionV1,
) -> Result<Wcag22AssessmentV1, Wcag22EvaluationErrorV1> {
    let parse = |field, value: &str| {
        if value.len() != 7 || !value.starts_with('#') {
            return Err(Wcag22EvaluationErrorV1::InvalidSrgb8 {
                field,
                reason: format!("expected exactly #RRGGBB, got {value:?}"),
            });
        }
        crate::srgb8::hex_bytes(value)
            .map_err(|reason| Wcag22EvaluationErrorV1::InvalidSrgb8 { field, reason })
    };
    let foreground = parse("foreground", foreground)?;
    let background = parse("background", background)?;
    evaluate_wcag22_srgb8(foreground, background, criterion)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(value: u64) -> Wcag22LuminanceBoundsQ55V1 {
        Wcag22LuminanceBoundsQ55V1 {
            lower: value,
            upper: value,
        }
    }

    #[test]
    fn synthetic_integer_boundaries_bite_both_threshold_laws() {
        let scale = super::super::q55_data::Q55_SCALE;
        let black = point(0);

        let first_three_pass = scale.div_ceil(10);
        assert_eq!(
            classify_orientation(point(first_three_pass), black, ThresholdV1::Three),
            OrientedDecisionV1::Pass
        );
        assert_eq!(
            classify_orientation(point(first_three_pass - 1), black, ThresholdV1::Three),
            OrientedDecisionV1::Fail
        );

        let first_four_half_pass = (7 * scale).div_ceil(40);
        assert_eq!(
            classify_orientation(point(first_four_half_pass), black, ThresholdV1::FourAndHalf,),
            OrientedDecisionV1::Pass
        );
        assert_eq!(
            classify_orientation(
                point(first_four_half_pass - 1),
                black,
                ThresholdV1::FourAndHalf,
            ),
            OrientedDecisionV1::Fail
        );
    }

    #[test]
    fn one_failed_and_one_unresolved_orientation_is_not_a_pair_fail() {
        let scale = super::super::q55_data::Q55_SCALE;
        let black = point(0);
        let straddling_three = Wcag22LuminanceBoundsQ55V1 {
            lower: scale / 10,
            upper: scale.div_ceil(10),
        };
        assert_eq!(
            classify_orientation(black, straddling_three, ThresholdV1::Three),
            OrientedDecisionV1::Fail
        );
        assert_eq!(
            classify_orientation(straddling_three, black, ThresholdV1::Three),
            OrientedDecisionV1::Unresolved
        );
        assert_eq!(
            classify_pair(
                black,
                straddling_three,
                Wcag22CriterionV1::Sc1411GraphicalObject,
            ),
            None
        );
    }
}
