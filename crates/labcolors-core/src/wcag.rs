//! Legacy WCAG 2.1 (2018) relative-luminance profile.
//!
//! This module preserves the product's existing continuous `0.03928` profile
//! while the canonical WCAG 2.2 finite-domain profile is implemented under
//! issue #284. The current W3C Recommendation uses `0.04045`; therefore this
//! module must not be described as current normative WCAG, cross-runtime exact,
//! or a legal-applicability decision. It remains independent of CAM16/LPC so the
//! technical formula can be audited and replaced as one unit.

/// WCAG 2.1 AA minimum contrast ratio for normal text (success criterion 1.4.3).
pub(crate) const AA_TEXT_RATIO: f64 = 4.5;

/// WCAG 2.1 AA minimum contrast ratio for UI components and graphical objects
/// (success criterion 1.4.11).
pub(crate) const AA_UI_RATIO: f64 = 3.0;

/// Split used by the original WCAG 2.1 (2018) continuous formula.
///
/// The current WCAG Recommendation uses `0.04045`. Issue #284 owns that
/// versioned migration; changing this constant in place would silently mutate a
/// characterized numerical profile.
const LEGACY_CHANNEL_SPLIT: f64 = 0.039_28;

/// First binary64 value on the power branch of the legacy split.
const LEGACY_CHANNEL_SPLIT_RIGHT: f64 = f64::from_bits(LEGACY_CHANNEL_SPLIT.to_bits() + 1);

const RED_WEIGHT: f64 = 0.2126;
const GREEN_WEIGHT: f64 = 0.7152;
const BLUE_WEIGHT: f64 = 0.0722;

/// Absolute headroom for the final three weighted binary64 operations in a
/// luminance interval. The material path is still legacy-platform-dependent
/// because `powf` has no repository-owned outward error bound.
const LUMINANCE_RANGE_MARGIN: f64 = 8.0 * f64::EPSILON;

/// Linearise one gamma-encoded sRGB channel in `[0, 1]` under the frozen legacy
/// WCAG 2.1 (2018) profile.
fn linearise(channel: f64) -> f64 {
    if channel <= LEGACY_CHANNEL_SPLIT {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// Characterized range of the legacy channel transfer over an ordered encoded
/// interval.
///
/// The legacy constants create a small downward discontinuity immediately to
/// the right of `0.03928`. Endpoint-only evaluation is therefore invalid. When
/// the interval crosses the split, both the linear value at the split and the
/// power-branch value at the first representable input above it participate in
/// the extrema. `powf` remains legacy-platform-dependent rather than soundly
/// outward-rounded.
fn linearised_channel_range(encoded_lo: f64, encoded_hi: f64) -> (f64, f64) {
    debug_assert!(
        encoded_lo.is_finite()
            && encoded_hi.is_finite()
            && (0.0..=1.0).contains(&encoded_lo)
            && (0.0..=1.0).contains(&encoded_hi)
            && encoded_lo <= encoded_hi
    );

    let mut lo = linearise(encoded_lo).min(linearise(encoded_hi));
    let mut hi = linearise(encoded_lo).max(linearise(encoded_hi));
    if encoded_lo <= LEGACY_CHANNEL_SPLIT && encoded_hi > LEGACY_CHANNEL_SPLIT {
        let at_split = linearise(LEGACY_CHANNEL_SPLIT);
        let right_of_split = linearise(LEGACY_CHANNEL_SPLIT_RIGHT);
        lo = lo.min(at_split).min(right_of_split);
        hi = hi.max(at_split).max(right_of_split);
    }
    (lo, hi)
}

/// Separable characterized luminance enclosure for three ordered encoded-sRGB
/// channel intervals.
///
/// All WCAG weights are positive, so channel minima and maxima combine without
/// enumerating the eight RGB corners. A small final absolute pad covers the
/// fixed binary64 multiply/add sequence; the branch-sensitive `powf` calls keep
/// this a platform characterization rather than a cross-runtime proof.
pub(crate) fn relative_luminance_range(encoded_lo: [f64; 3], encoded_hi: [f64; 3]) -> (f64, f64) {
    let channels = core::array::from_fn::<_, 3, _>(|channel| {
        linearised_channel_range(encoded_lo[channel], encoded_hi[channel])
    });
    let lower =
        (RED_WEIGHT * channels[0].0 + GREEN_WEIGHT * channels[1].0) + BLUE_WEIGHT * channels[2].0;
    let upper =
        (RED_WEIGHT * channels[0].1 + GREEN_WEIGHT * channels[1].1) + BLUE_WEIGHT * channels[2].1;
    (
        (lower - LUMINANCE_RANGE_MARGIN).max(0.0),
        (upper + LUMINANCE_RANGE_MARGIN).min(1.0),
    )
}

/// Frozen legacy WCAG 2.1 (2018) relative luminance of a gamma-encoded sRGB
/// colour `[r, g, b]` in `[0, 1]`.
pub(crate) fn relative_luminance(srgb: [f64; 3]) -> f64 {
    RED_WEIGHT * linearise(srgb[0])
        + GREEN_WEIGHT * linearise(srgb[1])
        + BLUE_WEIGHT * linearise(srgb[2])
}

/// The `[1, 21]` WCAG ratio of two relative luminances:
/// `(L_lighter + 0.05) / (L_darker + 0.05)`. Split out so both the gamma-encoded
/// path ([`contrast_ratio`]) and the linear-grid fast path share one formula.
pub(crate) fn ratio_from_luminances(la: f64, lb: f64) -> f64 {
    let (lighter, darker) = if la >= lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

/// Зафиксированное legacy-отношение контраста WCAG 2.1 (2018) между двумя
/// gamma-кодированными цветами sRGB в `[1, 21]`.
///
/// `(L_lighter + 0.05) / (L_darker + 0.05)`. Формула симметрична и не зависит от
/// полярности, в отличие от знаковой candidate-кривой. Поэтому величины
/// возвращаются отдельно и не объединяются.
pub(crate) fn contrast_ratio(a: [f64; 3], b: [f64; 3]) -> f64 {
    ratio_from_luminances(relative_luminance(a), relative_luminance(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// White is `[1,1,1]` (L = 1), black is `[0,0,0]` (L = 0): the canonical
    /// 21:1 extreme.
    #[test]
    fn black_on_white_is_twentyone_to_one() {
        let r = contrast_ratio([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]);
        assert!(
            (r - 21.0).abs() < 1e-9,
            "black on white must be 21:1, got {r}"
        );
    }

    #[test]
    fn ratio_is_symmetric() {
        let white = [1.0, 1.0, 1.0];
        let grey = [0.5, 0.5, 0.5];
        assert!((contrast_ratio(white, grey) - contrast_ratio(grey, white)).abs() < 1e-12);
    }

    #[test]
    fn identical_colours_are_one_to_one() {
        let c = [0.42, 0.13, 0.77];
        assert!((contrast_ratio(c, c) - 1.0).abs() < 1e-12);
    }

    /// `#767676` on white is the textbook AA-text boundary (~4.54:1).
    #[test]
    fn grey_boundary_matches_published_value() {
        let g = 0x76 as f64 / 255.0;
        let r = contrast_ratio([1.0, 1.0, 1.0], [g, g, g]);
        assert!(
            (r - 4.54).abs() < 0.05,
            "#767676 on white should be ~4.54:1, got {r}"
        );
    }

    /// One quantisation step lighter — `#777777` on white — falls below 4.5:1
    /// (~4.48): pins the AA boundary from below.
    #[test]
    fn next_grey_step_falls_below_aa() {
        let g = 0x77 as f64 / 255.0;
        let r = contrast_ratio([1.0, 1.0, 1.0], [g, g, g]);
        assert!(
            r < AA_TEXT_RATIO,
            "#777777 on white must be < 4.5:1, got {r}"
        );
        assert!(
            (r - 4.48).abs() < 0.05,
            "#777777 on white should be ~4.48:1, got {r}"
        );
    }

    /// `contrast_ratio` still equals its inlined `(L+0.05)/(L+0.05)` form after
    /// being refactored to delegate to [`ratio_from_luminances`] — a byte-for-byte
    /// guard that the split introduced no arithmetic change.
    #[test]
    fn contrast_ratio_matches_inlined_formula() {
        for &(a, b) in &[
            ([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]),
            ([0.42, 0.13, 0.77], [0.10, 0.20, 0.30]),
            ([0.03, 0.5, 0.9], [0.9, 0.5, 0.03]),
        ] {
            let la = relative_luminance(a);
            let lb = relative_luminance(b);
            let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
            let inlined = (hi + 0.05) / (lo + 0.05);
            assert_eq!(contrast_ratio(a, b).to_bits(), inlined.to_bits());
        }
    }

    #[test]
    fn legacy_channel_range_contains_both_sides_of_the_eotf_seam() {
        assert_eq!(
            LEGACY_CHANNEL_SPLIT_RIGHT.to_bits(),
            LEGACY_CHANNEL_SPLIT.to_bits() + 1
        );
        let linear_side = LEGACY_CHANNEL_SPLIT / 12.92;
        let power_side = ((LEGACY_CHANNEL_SPLIT_RIGHT + 0.055) / 1.055).powf(2.4);
        assert!(
            power_side < linear_side,
            "fixture must expose the legacy seam"
        );

        // A wide interval makes the power-side seam value an interior extremum;
        // endpoint-only evaluation cannot satisfy this assertion.
        let (lo, hi) = linearised_channel_range(LEGACY_CHANNEL_SPLIT, 0.5);
        assert!(lo <= power_side, "lower range omitted power side");
        assert!(hi >= linear_side, "upper range omitted linear side");
    }

    #[test]
    fn luminance_range_is_separable_and_bounded() {
        let (lo, hi) = relative_luminance_range(
            [LEGACY_CHANNEL_SPLIT, 0.25, 0.75],
            [LEGACY_CHANNEL_SPLIT_RIGHT, 0.5, 1.0],
        );
        assert!(lo.is_finite() && hi.is_finite());
        assert!((0.0..=1.0).contains(&lo));
        assert!((0.0..=1.0).contains(&hi));
        assert!(lo <= hi);

        for rgb in [
            [LEGACY_CHANNEL_SPLIT, 0.25, 0.75],
            [LEGACY_CHANNEL_SPLIT_RIGHT, 0.5, 1.0],
            [LEGACY_CHANNEL_SPLIT_RIGHT, 0.25, 0.75],
        ] {
            let actual = relative_luminance(rgb);
            assert!(
                lo <= actual && actual <= hi,
                "{actual} outside [{lo}, {hi}]"
            );
        }
    }
}
