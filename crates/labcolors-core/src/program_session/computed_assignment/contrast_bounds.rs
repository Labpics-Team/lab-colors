//! Conservative WCAG region pruning for the physical source-over expression.
//!
//! The foreground and its own backdrop are correlated, even if the two
//! endpoints of their respective RGB boxes are far apart.

use crate::appearance::SourceOverCertificateV1;
use crate::composition::{AdmittedOpacityV1, CompositionProfileV1, source_over_channel_srgb8};
use crate::wcag22::{
    Wcag22CriterionV1, Wcag22LuminanceBoundsQ55V1, measure_wcag22_srgb8, twice_minimum_contrast,
};

// Shared by every selection profile that uses the physical region evaluator.
pub(super) const IDENTITY: &[u8] = b"labcolors.hard-region.coupled-source-over-q55.v1\0";

/// Upper bound for the oriented threshold residual, before subtracting the
/// constant term. With t = twice the minimum contrast and S = 2^55,
/// a passing pair necessarily satisfies 40L - 20tD >= (t - 2)S.
/// The Q55 kernel's outward luminance bounds preserve this necessary condition.
fn residual_upper(light: [u8; 3], dark: [u8; 3], t: i64) -> i64 {
    let m = measure_wcag22_srgb8(light, dark);
    40 * m.foreground_luminance.upper() as i64 - 20 * t * m.background_luminance.lower() as i64
}

pub(super) fn source_over_may_pass(
    lower: SourceOverCertificateV1,
    upper: SourceOverCertificateV1,
    criterion: Wcag22CriterionV1,
) -> Result<bool, ()> {
    let opacity = lower.subject_opacity();
    if opacity != upper.subject_opacity() || lower.profile() != upper.profile() {
        return Err(());
    }
    match lower.profile() {
        CompositionProfileV1::EncodedSrgb8SourceOverV1 => {}
    }
    let t = i64::from(twice_minimum_contrast(criterion));
    if opacity.bits() == 0 {
        // The visible occurrence equals its very own backdrop throughout the
        // region, so its contrast is exactly one.
        return Ok(t <= 2);
    }
    let required = (t - 2) * Wcag22LuminanceBoundsQ55V1::scale() as i64;
    let bg = (lower.backdrop_rgb(), upper.backdrop_rgb());
    let visible = (lower.output_rgb(), upper.output_rgb());
    let tint = (lower.subject_rgb(), upper.subject_rgb());
    let alpha = opacity.value();
    for (lighter_foreground, source, light, dark) in [
        (true, tint.1, visible.1, bg.0),
        (false, tint.0, bg.1, visible.0),
    ] {
        if residual_upper(light, dark, t) < required {
            continue;
        }
        if opacity == AdmittedOpacityV1::OPAQUE || bg.0 == bg.1 {
            return Ok(true);
        }

        // Cheap feasible-in-the-box witnesses avoid enumerating channels for
        // broad regions which already contain a sufficiently contrasting pair.
        let oriented = |visible, backdrop| {
            if lighter_foreground {
                residual_upper(visible, backdrop, t)
            } else {
                residual_upper(backdrop, visible, t)
            }
        };
        if [bg.0, bg.1].into_iter().any(|b| {
            let v = std::array::from_fn(|i| source_over_channel_srgb8(source[i], alpha, b[i]));
            oriented(v, b) >= required
        }) {
            return Ok(true);
        }

        // For each fixed backdrop byte, monotonic source-over makes the upper
        // tint endpoint optimal in the lighter-foreground orientation and the
        // lower tint endpoint optimal in the reverse orientation. Luminance is
        // additive across channels. Maximize each channel over the *same*
        // backdrop byte used by that channel's composited foreground.
        //
        // Independent channel maxima enclose the entire RGB box, including
        // correlated grayscale domains and nested computed backdrops. A zero
        // byte contributes exactly zero to Q55 luminance, so isolated-channel
        // measurements reuse the one proof-bound luminance implementation.
        let mut upper_residual = 0;
        for i in 0..3 {
            let maximum = (bg.0[i]..=bg.1[i])
                .map(|b| {
                    let mut visible = [0; 3];
                    let mut backdrop = [0; 3];
                    visible[i] = source_over_channel_srgb8(source[i], alpha, b);
                    backdrop[i] = b;
                    oriented(visible, backdrop)
                })
                .max()
                .ok_or(())?;
            upper_residual += maximum;
        }
        if upper_residual >= required {
            return Ok(true);
        }
    }
    // Both orientations are impossible for all physical pairs in the box.
    // The converse is not a WCAG assessment: final tuples still use the
    // normative kernel and its sealed evidence.
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wcag22::{Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, evaluate_wcag22_srgb8};

    fn cert(tint: [u8; 3], alpha: f64, backdrop: [u8; 3]) -> SourceOverCertificateV1 {
        SourceOverCertificateV1::compose(
            CompositionProfileV1::EncodedSrgb8SourceOverV1,
            tint,
            AdmittedOpacityV1::new(alpha).unwrap(),
            backdrop,
        )
    }

    fn passes(c: SourceOverCertificateV1, criterion: Wcag22CriterionV1) -> bool {
        matches!(
            evaluate_wcag22_srgb8(c.output_rgb(), c.backdrop_rgb(), criterion).unwrap(),
            Wcag22AssessmentV1::Evaluated {
                decision: Wcag22ApplicableDecisionV1::Pass,
                ..
            }
        )
    }

    #[test]
    fn transparent_and_near_transparent_regions_cannot_invent_contrast() {
        for alpha in [0.0, 0.001, 0.05, 0.1] {
            for criterion in [
                Wcag22CriterionV1::Sc143TextDefault,
                Wcag22CriterionV1::Sc143TextLargeScale,
            ] {
                assert!(
                    !source_over_may_pass(
                        cert([0; 3], alpha, [0; 3]),
                        cert([255; 3], alpha, [255; 3]),
                        criterion,
                    )
                    .unwrap(),
                    "alpha={alpha}, {criterion:?}"
                );
            }
        }
    }

    #[test]
    fn coupled_neutral_boxes_retain_every_passing_pair() {
        // Exhaust the neutral byte plane, independently evaluating each final
        // pair. Include quantization, both polarities and degenerate opacity.
        let criteria = [
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22CriterionV1::Sc143TextLargeScale,
        ];
        for alpha in [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0] {
            for s in (0..=255_u8).step_by(32) {
                for b in (0..=255_u8).step_by(32) {
                    let se = s.saturating_add(31);
                    let be = b.saturating_add(31);
                    let allowed = criteria.map(|criterion| {
                        source_over_may_pass(
                            cert([s; 3], alpha, [b; 3]),
                            cert([se; 3], alpha, [be; 3]),
                            criterion,
                        )
                        .unwrap()
                    });
                    for tint in s..=se {
                        for backdrop in b..=be {
                            let point = cert([tint; 3], alpha, [backdrop; 3]);
                            for (i, criterion) in criteria.into_iter().enumerate() {
                                assert!(
                                    allowed[i] || !passes(point, criterion),
                                    "alpha={alpha}, tint={tint}, backdrop={backdrop}, {criterion:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn coupled_chromatic_boxes_retain_independent_channel_choices() {
        // Interior mixed-channel values need not share one RGB endpoint.
        let sources = [[1, 38, 91], [241, 215, 164]];
        let backdrops = [[3, 18, 30], [223, 240, 251]];
        for alpha in [0.17, 0.35, 0.58, 0.81] {
            for criterion in [
                Wcag22CriterionV1::Sc143TextDefault,
                Wcag22CriterionV1::Sc143TextLargeScale,
            ] {
                let allowed = source_over_may_pass(
                    cert(sources[0], alpha, backdrops[0]),
                    cert(sources[1], alpha, backdrops[1]),
                    criterion,
                )
                .unwrap();
                for s in 0..27 {
                    for b in 0..27 {
                        let sample = |ends: [[u8; 3]; 2], ordinal| {
                            std::array::from_fn(|i| {
                                let lo = ends[0][i];
                                let hi = ends[1][i];
                                [lo, lo + (hi - lo) / 2, hi][ordinal / 3_usize.pow(i as u32) % 3]
                            })
                        };
                        let point = cert(sample(sources, s), alpha, sample(backdrops, b));
                        assert!(allowed || !passes(point, criterion));
                    }
                }
            }
        }
    }
}
