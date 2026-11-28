//! Контекстная виртуальная опора: яркость переносится, reference a/b сохраняются.
//!
//! Это численное предпочтение, не критерий WCAG или модель Cleanliness. Формулы
//! используют существующие матрицы Oklab (Ottosson, 2021) и relative luminance.
//! Виртуальная опора может лежать вне sRGB; gamut mapping здесь запрещён.

use crate::Srgb8;
use crate::constraints::TonalPolarityV1;
use crate::spaces::oklab::{LMS_TO_SRGB, OKLAB_TO_LMS};
use crate::spaces::srgb::{
    RELATIVE_LUMINANCE_BLUE_WEIGHT, RELATIVE_LUMINANCE_GREEN_WEIGHT, RELATIVE_LUMINANCE_RED_WEIGHT,
    relative_luminance_ratio, srgb_linear_from_srgb8,
};

/// Binary64 physical coordinate; normative pass/fail remains in the Q55 evaluator.
pub(crate) fn luminance(rgb: Srgb8) -> f64 {
    let linear = srgb_linear_from_srgb8(rgb);
    RELATIVE_LUMINANCE_RED_WEIGHT * linear[0]
        + RELATIVE_LUMINANCE_GREEN_WEIGHT * linear[1]
        + RELATIVE_LUMINANCE_BLUE_WEIGHT * linear[2]
}

fn q55_luminance(rgb: Srgb8) -> crate::wcag22::Wcag22LuminanceBoundsQ55V1 {
    crate::wcag22::measure_wcag22_srgb8(rgb.bytes(), [0; 3]).foreground_luminance
}

fn normative_luminance_bounds(rgb: Srgb8) -> [f64; 2] {
    let bounds = q55_luminance(rgb);
    let scale = crate::wcag22::Wcag22LuminanceBoundsQ55V1::scale() as f64;
    [
        next_down(bounds.lower() as f64 / scale).max(0.0),
        next_up(bounds.upper() as f64 / scale).min(1.0),
    ]
}

// Bridge the two luminance models without inventing a threshold tolerance.
// Both are sums of three independent byte contributions. Enclose the error
// of every contribution against the proof-bound Q55 artifact and account for
// the two rounded additions (absolute rounding error <= EPSILON below 2).
static LUMINANCE_MODEL_ERROR: std::sync::LazyLock<f64> = std::sync::LazyLock::new(|| {
    let mut total = 0.0;
    let mut max_sum = 0.0;
    for channel in 0..3 {
        let mut maximum = 0.0_f64;
        let mut channel_max = 0.0_f64;
        for byte in 0..=u8::MAX {
            let mut rgb = [0; 3];
            rgb[channel] = byte;
            let rgb = Srgb8::new(rgb);
            let actual = luminance(rgb);
            if !actual.is_finite() || actual < 0.0 {
                return f64::INFINITY;
            }
            channel_max = channel_max.max(actual);
            let [lo, hi] = normative_luminance_bounds(rgb);
            maximum = maximum.max(next_up((actual - lo).abs().max((hi - actual).abs())));
        }
        total = next_up(total + maximum);
        max_sum = next_up(max_sum + channel_max);
    }
    if max_sum >= 2.0 {
        return f64::INFINITY;
    }
    next_up(total + 2.0 * f64::EPSILON)
});

/// Necessary operational-luminance limits for a weak occurrence satisfying a
/// WCAG contrast floor on an uncertain common backdrop. On the lighter side
/// its luminance must be >= result[0]; on the darker side <= result[1].
/// Outward arithmetic retains every normative pass, including near thresholds.
pub(crate) fn wcag_weak_luminance_limits(
    backdrop: [Srgb8; 2],
    criterion: crate::wcag22::Wcag22CriterionV1,
) -> [f64; 2] {
    let twice_minimum = i128::from(crate::wcag22::twice_minimum_contrast(criterion));
    let scale = i128::from(crate::wcag22::Wcag22LuminanceBoundsQ55V1::scale());
    let offset = (twice_minimum - 2) * scale;
    let error = *LUMINANCE_MODEL_ERROR;
    // A passing normative contrast separates the luminances by at least
    // (minimum - 1) / 20. If the bridge cannot preserve that polarity,
    // the propagation must be disabled rather than guess the orientation.
    let gap = next_down((offset / 40) as f64 / scale as f64);
    if 2.0 * error >= gap {
        return [0.0, 1.0];
    }
    let lo = i128::from(q55_luminance(backdrop[0]).lower());
    let hi = i128::from(q55_luminance(backdrop[1]).upper());
    // Clearing denominators from (L + 1/20) / (D + 1/20) >= t/2
    // gives 40 L >= 20 t D + (t - 2) S, with S the Q55 scale.
    // Floor the necessary lighter limit and ceil the darker limit on this
    // integer lattice before crossing into binary64; negative limits are
    // intentional and prove the respective polarity infeasible.
    let factor = 20 * twice_minimum;
    let lighter_q55 = (factor * lo + offset) / 40;
    let darker_q55 = (40 * hi - offset + factor - 1).div_euclid(factor);
    let lighter = next_down(lighter_q55 as f64 / scale as f64);
    let darker = next_up(darker_q55 as f64 / scale as f64);
    [next_down(lighter - error), next_up(darker + error)]
}

pub(crate) fn contrast_ratio(visible: Srgb8, backdrop: Srgb8) -> f64 {
    relative_luminance_ratio(luminance(visible), luminance(backdrop))
}

pub(crate) fn polarity(visible: Srgb8, backdrop: Srgb8) -> TonalPolarityV1 {
    luminance_polarity(luminance(visible), luminance(backdrop))
}

pub(crate) fn luminance_polarity(visible: f64, backdrop: f64) -> TonalPolarityV1 {
    if visible < backdrop {
        TonalPolarityV1::Darker
    } else if visible > backdrop {
        TonalPolarityV1::Lighter
    } else {
        TonalPolarityV1::Equal
    }
}

/// Clipping is observable data, not a claim that reference strength is achievable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct VirtualReferencePointV1 {
    pub(crate) lab: [f64; 3],
    pub(crate) raw_luminance: f64,
    pub(crate) clipped_luminance: f64,
    pub(crate) upper_branch_floor_luminance: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ReferenceTransportV1 {
    reference_lab: [f64; 3],
    reference_backdrop_y: f64,
    reference_contrast: f64,
    reference_polarity: TonalPolarityV1,
    curve: UpperLuminanceCurveV1,
}

// All fields derive from admitted finite Srgb8 values; NaN is unrepresentable.
impl Eq for ReferenceTransportV1 {}

impl ReferenceTransportV1 {
    pub(crate) fn new(visible: Srgb8, backdrop: Srgb8) -> Self {
        let reference_lab = super::oklab(visible);
        Self {
            reference_lab,
            reference_backdrop_y: luminance(backdrop),
            reference_contrast: contrast_ratio(visible, backdrop),
            reference_polarity: polarity(visible, backdrop),
            curve: UpperLuminanceCurveV1::new(reference_lab[1], reference_lab[2]),
        }
    }

    pub(crate) const fn reference_lab(&self) -> [f64; 3] {
        self.reference_lab
    }

    pub(crate) const fn reference_contrast(&self) -> f64 {
        self.reference_contrast
    }

    pub(crate) fn reference_log_contrast(&self) -> f64 {
        self.reference_contrast.ln()
    }

    pub(crate) const fn reference_polarity(&self) -> TonalPolarityV1 {
        self.reference_polarity
    }

    pub(crate) fn lab_for(&self, backdrop: Srgb8, polarity: TonalPolarityV1) -> [f64; 3] {
        self.target_for(backdrop, polarity).lab
    }

    pub(crate) fn target_for(
        &self,
        backdrop: Srgb8,
        polarity: TonalPolarityV1,
    ) -> VirtualReferencePointV1 {
        self.target_for_luminance(luminance(backdrop), polarity)
    }

    pub(crate) fn target_for_luminance(
        &self,
        backdrop_y: f64,
        polarity: TonalPolarityV1,
    ) -> VirtualReferencePointV1 {
        debug_assert!(backdrop_y.is_finite() && (0.0..=1.0).contains(&backdrop_y));
        let raw_luminance = self.raw_luminance(backdrop_y, polarity);
        let clipped_luminance = raw_luminance.clamp(0.0, 1.0);
        let lab = if self.is_reference_point(backdrop_y, polarity) {
            // Published forward/inverse matrices are rounded independently.
            // Returning the actual authored point avoids a drift merely due
            // to their small round-trip discrepancy. Bounds include this point.
            self.reference_lab
        } else {
            [
                self.curve.inverse(clipped_luminance),
                self.reference_lab[1],
                self.reference_lab[2],
            ]
        };
        VirtualReferencePointV1 {
            lab,
            raw_luminance,
            clipped_luminance,
            upper_branch_floor_luminance: self.curve.floor_y.max(0.0),
        }
    }

    /// Encloses the operational point function, including the exact-reference
    /// exception. Invalid intervals are inconclusive, never proof of absence.
    pub(crate) fn virtual_lightness_bounds(
        &self,
        backdrop_y: [f64; 2],
        polarity: TonalPolarityV1,
    ) -> [f64; 2] {
        if !backdrop_y.iter().all(|v| v.is_finite())
            || backdrop_y[0] < 0.0
            || backdrop_y[1] > 1.0
            || backdrop_y[0] > backdrop_y[1]
        {
            return [f64::NEG_INFINITY, f64::INFINITY];
        }
        let mut bounds = backdrop_y.map(|y| {
            self.curve
                .inverse(self.raw_luminance(y, polarity).clamp(0.0, 1.0))
        });
        if backdrop_y[0] <= self.reference_backdrop_y
            && self.reference_backdrop_y <= backdrop_y[1]
            && self.is_reference_point(self.reference_backdrop_y, polarity)
        {
            bounds[0] = bounds[0].min(self.reference_lab[0]);
            bounds[1] = bounds[1].max(self.reference_lab[0]);
        }
        [next_down(bounds[0]), next_up(bounds[1])]
    }

    fn is_reference_point(&self, backdrop_y: f64, polarity: TonalPolarityV1) -> bool {
        backdrop_y == self.reference_backdrop_y
            && (polarity == self.reference_polarity || self.reference_contrast == 1.0)
    }

    fn raw_luminance(&self, backdrop_y: f64, polarity: TonalPolarityV1) -> f64 {
        if self.reference_contrast == 1.0 {
            // All three polarities coalesce exactly at unit strength. Avoid
            // the cancellation error of (y + .05) - .05 on only two branches.
            return backdrop_y;
        }
        match polarity {
            TonalPolarityV1::Darker => (backdrop_y + 0.05) / self.reference_contrast - 0.05,
            TonalPolarityV1::Lighter => (backdrop_y + 0.05) * self.reference_contrast - 0.05,
            // Equal is a real zero-strength branch. The caller must assess its
            // capacity against reference strength; it is not a free success.
            TonalPolarityV1::Equal => backdrop_y,
        }
    }
}

/// P(L) = A L³ + B L² + C L + D at fixed a/b. Rounded coefficients define this
/// numerical kernel; the source matrices stay in their existing single place.
#[derive(Debug, Clone, Copy, PartialEq)]
struct UpperLuminanceCurveV1 {
    coefficients: [f64; 4],
    lower_l: f64,
    upper_l: f64,
    floor_y: f64,
}

impl UpperLuminanceCurveV1 {
    fn new(a: f64, b: f64) -> Self {
        let weights = [
            RELATIVE_LUMINANCE_RED_WEIGHT,
            RELATIVE_LUMINANCE_GREEN_WEIGHT,
            RELATIVE_LUMINANCE_BLUE_WEIGHT,
        ];
        let lms_y: [f64; 3] = std::array::from_fn(|i| {
            weights[0] * LMS_TO_SRGB[0][i]
                + weights[1] * LMS_TO_SRGB[1][i]
                + weights[2] * LMS_TO_SRGB[2][i]
        });
        let shifts = OKLAB_TO_LMS.map(|row| row[1] * a + row[2] * b);
        let sum = |power: u32| {
            lms_y[0] * shifts[0].powi(power as i32)
                + lms_y[1] * shifts[1].powi(power as i32)
                + lms_y[2] * shifts[2].powi(power as i32)
        };
        let coefficients = [lms_y.iter().sum(), 3.0 * sum(1), 3.0 * sum(2), sum(3)];
        let [cubic, quadratic, linear, constant] = coefficients;
        // Cauchy's root bound covers P(L)-y for every y in [0,1].
        let upper_l = next_up(
            1.0 + next_up(
                quadratic
                    .abs()
                    .max(linear.abs())
                    .max(next_up(constant.abs() + 1.0))
                    / cubic,
            ),
        );
        let aa = Interval::point(cubic);
        let bb = Interval::point(quadratic);
        let cc = Interval::point(linear);
        let discriminant = bb.mul(bb).sub(Interval::point(3.0).mul(aa).mul(cc));
        let lower_l = if a == 0.0 && b == 0.0 {
            // Exact neutral polynomial has its zero at zero; no tiny artificial
            // chroma or stationary point is invented by outward rounding.
            0.0
        } else if discriminant.hi > 0.0 {
            // Right critical point, rounded upward, places every evaluated
            // inverse on the increasing branch, even at a nearly double root.
            bb.neg()
                .add(discriminant.nonnegative_sqrt())
                .div_positive(Interval::point(3.0).mul(aa))
                .hi
        } else {
            -upper_l
        };
        let mut curve = Self {
            coefficients,
            lower_l,
            upper_l,
            floor_y: 0.0,
        };
        curve.floor_y = curve.evaluate(lower_l);
        debug_assert!(lower_l <= upper_l);
        curve
    }

    fn evaluate(&self, lightness: f64) -> f64 {
        let [a, b, c, d] = self.coefficients;
        ((a * lightness + b) * lightness + c) * lightness + d
    }

    fn inverse(&self, y: f64) -> f64 {
        debug_assert!(y.is_finite() && (0.0..=1.0).contains(&y));
        if y <= self.floor_y {
            return self.lower_l;
        }
        let mut lower = self.lower_l;
        let mut upper = self.upper_l;
        // 64 halvings cap hot-path work. Adjacent binary64 endpoints terminate
        // before redundant work; exceptionally small roots can retain a span.
        // This fixed decision tree is monotone in y even if evaluating P at a
        // midpoint has rounding error: the first differing decision selects
        // ordered disjoint subintervals. Endpoint bounds therefore enclose
        // this *operational* function without a residual-to-error assumption.
        for _ in 0..64 {
            let middle = lower + (upper - lower) * 0.5;
            if middle == lower || middle == upper {
                break;
            }
            if self.evaluate(middle) <= y {
                lower = middle;
            } else {
                upper = middle;
            }
        }
        lower + (upper - lower) * 0.5
    }

    /// Independent enclosing arithmetic for the real polynomial with the
    /// stored binary64 coefficients. Used to qualify inversion error, not in
    /// the hot search path. No division by a derivative near the fold.
    #[cfg(test)]
    fn inverse_enclosure(&self, y: f64) -> [f64; 2] {
        let evaluate = |l| {
            self.coefficients[1..].iter().fold(
                Interval::point(self.coefficients[0]),
                |value, &coefficient| {
                    value
                        .mul(Interval::point(l))
                        .add(Interval::point(coefficient))
                },
            )
        };
        let floor = evaluate(self.lower_l);
        if y < floor.lo {
            return [next_down(self.lower_l), next_up(self.lower_l)];
        }
        let mut certified_lower = self.lower_l;
        let mut certified_upper = self.upper_l;
        // Search the two signs independently. An uncertain sign narrows only
        // the probe range; it never advances a claimed root bound.
        for seek_upper in [false, true] {
            let mut lower = self.lower_l;
            let mut upper = self.upper_l;
            for _ in 0..64 {
                let middle = lower + (upper - lower) * 0.5;
                if middle == lower || middle == upper {
                    break;
                }
                let value = evaluate(middle);
                if seek_upper {
                    if value.lo > y {
                        certified_upper = middle;
                        upper = middle;
                    } else {
                        lower = middle;
                    }
                } else if floor.hi < y && value.hi < y {
                    certified_lower = middle;
                    lower = middle;
                } else {
                    upper = middle;
                }
            }
        }
        // If y overlaps the floor enclosure, retain the saturated endpoint as
        // well as the possible upper-branch root. The tested inverse result is
        // never used to construct its own numerical reference interval.
        [next_down(certified_lower), next_up(certified_upper)]
    }
}

// Outward IEEE-754 basic operations. This is a numerical enclosure, not an
// acceptance epsilon. next_up/down were stabilized after this crate's MSRV.
fn next_up(value: f64) -> f64 {
    if value.is_nan() || value == f64::INFINITY {
        value
    } else if value == 0.0 {
        f64::from_bits(1)
    } else if value > 0.0 {
        f64::from_bits(value.to_bits() + 1)
    } else {
        f64::from_bits(value.to_bits() - 1)
    }
}

fn next_down(value: f64) -> f64 {
    -next_up(-value)
}

#[derive(Clone, Copy)]
struct Interval {
    lo: f64,
    hi: f64,
}

impl Interval {
    fn point(value: f64) -> Self {
        Self {
            lo: value,
            hi: value,
        }
    }
    fn neg(self) -> Self {
        Self {
            lo: -self.hi,
            hi: -self.lo,
        }
    }
    fn add(self, other: Self) -> Self {
        Self {
            lo: next_down(self.lo + other.lo),
            hi: next_up(self.hi + other.hi),
        }
    }
    fn sub(self, other: Self) -> Self {
        self.add(other.neg())
    }
    fn mul(self, other: Self) -> Self {
        let values = [
            self.lo * other.lo,
            self.lo * other.hi,
            self.hi * other.lo,
            self.hi * other.hi,
        ];
        Self {
            lo: next_down(values.into_iter().fold(f64::INFINITY, f64::min)),
            hi: next_up(values.into_iter().fold(f64::NEG_INFINITY, f64::max)),
        }
    }
    fn div_positive(self, other: Self) -> Self {
        debug_assert!(other.lo > 0.0);
        self.mul(Self {
            lo: next_down(1.0 / other.hi),
            hi: next_up(1.0 / other.lo),
        })
    }
    fn nonnegative_sqrt(self) -> Self {
        Self {
            lo: next_down(self.lo.max(0.0).sqrt()).max(0.0),
            hi: next_up(self.hi.max(0.0).sqrt()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normative_contrast_limits_retain_every_passing_neutral_pair_and_chromatic_sample() {
        use crate::wcag22::{
            Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22CriterionV1,
            evaluate_wcag22_srgb8,
        };

        let check = |foreground: Srgb8, backdrop: Srgb8, criterion| {
            let result = evaluate_wcag22_srgb8(foreground.bytes(), backdrop.bytes(), criterion)
                .expect("finite sRGB8 assessment");
            if matches!(
                result,
                Wcag22AssessmentV1::Evaluated {
                    decision: Wcag22ApplicableDecisionV1::Pass,
                    ..
                }
            ) {
                let [lighter, darker] = wcag_weak_luminance_limits([backdrop; 2], criterion);
                let y = luminance(foreground);
                match polarity(foreground, backdrop) {
                    TonalPolarityV1::Lighter => assert!(
                        lighter <= y,
                        "{foreground:?} {backdrop:?} {criterion:?}: {lighter} > {y}"
                    ),
                    TonalPolarityV1::Darker => assert!(
                        y <= darker,
                        "{foreground:?} {backdrop:?} {criterion:?}: {y} > {darker}"
                    ),
                    TonalPolarityV1::Equal => {
                        panic!("passing pair has equal operational luminance")
                    }
                }
            }
        };
        assert!(LUMINANCE_MODEL_ERROR.is_finite());
        for criterion in Wcag22CriterionV1::ALL {
            for foreground in 0..=u8::MAX {
                for backdrop in 0..=u8::MAX {
                    check(
                        Srgb8::new([foreground; 3]),
                        Srgb8::new([backdrop; 3]),
                        criterion,
                    );
                }
            }
            for sample in 0..=u32::from(u16::MAX) {
                let a = sample.wrapping_mul(0x9e37_79b9).to_le_bytes();
                let b = sample
                    .wrapping_mul(0x85eb_ca6b)
                    .wrapping_add(17)
                    .to_be_bytes();
                check(
                    Srgb8::new([a[0], a[1], a[2]]),
                    Srgb8::new([b[1], b[2], b[3]]),
                    criterion,
                );
            }
        }
    }

    #[test]
    fn neutral_inverse_has_independent_exact_cubic_cases() {
        let curve = UpperLuminanceCurveV1::new(0.0, 0.0);
        for (y, expected) in [(0.0, 0.0), (0.015625, 0.25), (0.125, 0.5), (1.0, 1.0)] {
            let [lower, upper] = curve.inverse_enclosure(y);
            assert!(
                lower <= expected && expected <= upper,
                "{y}: {lower}..{upper}"
            );
            assert!((curve.inverse(y) - expected).abs() <= 4.0 * f64::EPSILON);
        }
    }

    #[test]
    fn chromatic_inverse_matches_high_precision_polynomial_fixtures() {
        // Python Decimal, precision 90, 320 bisections of the polynomial whose
        // coefficients are the exact stored binary64 values. These are normal
        // roots, separate from the ill-conditioned stationary-point test.
        let fixtures = [
            ((-0.006939085856, 0.178478404679), 0.01, 0.2233931570766726),
            ((0.24, -0.31), 0.1, 0.5306450483495154),
            ((-0.23, 0.18), 1.0, 0.9722433518023028),
            ((0.1, 0.03), 1.0, 1.0125648987034876),
        ];
        for ((a, b), y, expected) in fixtures {
            let curve = UpperLuminanceCurveV1::new(a, b);
            let enclosure = curve.inverse_enclosure(y);
            assert!(enclosure[0] <= expected && expected <= enclosure[1]);
            // A few encoding units qualify the point kernel on these fixtures;
            // this is numerical error, never an allowed color/contrast loss.
            let lower = (0..4).fold(expected, |value, _| next_down(value));
            let upper = (0..4).fold(expected, |value, _| next_up(value));
            assert!(lower <= curve.inverse(y) && curve.inverse(y) <= upper);
        }
    }

    #[test]
    fn reference_is_exact_and_polarity_uses_luminance() {
        let blue = Srgb8::new([0, 0, 255]);
        let gray = Srgb8::new([80; 3]);
        assert!(super::super::oklab(blue)[0] > super::super::oklab(gray)[0]);
        let reference = ReferenceTransportV1::new(blue, gray);
        assert_eq!(reference.reference_polarity(), TonalPolarityV1::Darker);
        assert_eq!(
            reference.lab_for(gray, reference.reference_polarity()),
            reference.reference_lab()
        );
        assert_eq!(
            reference.reference_log_contrast(),
            reference.reference_contrast().ln()
        );
    }

    #[test]
    fn yellow_fold_saturates_instead_of_jumping_to_the_lower_root() {
        let lab = super::super::oklab(Srgb8::new([255, 208, 0]));
        let curve = UpperLuminanceCurveV1::new(lab[1], lab[2]);
        assert!(curve.floor_y > 0.0);
        assert!(curve.lower_l > 0.05 && curve.lower_l < 0.07);
        let below = curve.inverse(curve.floor_y - 1e-9);
        let above = curve.inverse(curve.floor_y + 1e-9);
        assert_eq!(below, curve.lower_l);
        let saturated = curve.inverse_enclosure(curve.floor_y - 1e-9);
        assert!(saturated[0] <= below && below <= saturated[1]);
        assert!(saturated[1] < below + 1e-12);
        assert!(above >= below && above - below < 0.001);
        for y in [
            next_down(curve.floor_y),
            curve.floor_y,
            next_up(curve.floor_y),
        ] {
            let enclosure = curve.inverse_enclosure(y);
            assert!(enclosure[0] <= curve.inverse(y) && curve.inverse(y) <= enclosure[1]);
            assert!(enclosure[1] - enclosure[0] < 1e-6, "{y}: {enclosure:?}");
        }
    }

    #[test]
    fn visible_chroma_is_not_gamut_compressed_or_lightness_clamped() {
        let reference =
            ReferenceTransportV1::new(Srgb8::new([0, 122, 255]), Srgb8::new([16, 16, 18]));
        let light = reference.target_for(Srgb8::new([255; 3]), TonalPolarityV1::Lighter);
        assert!(light.raw_luminance > 1.0);
        assert_eq!(light.clipped_luminance, 1.0);
        assert!(light.lab[0] > 1.0);
        assert_eq!(&light.lab[1..], &reference.reference_lab()[1..]);
        let dark = reference.target_for(Srgb8::new([0; 3]), TonalPolarityV1::Darker);
        assert!(dark.raw_luminance < 0.0);
        assert_eq!(&dark.lab[1..], &reference.reference_lab()[1..]);
    }

    #[test]
    fn equal_strength_has_one_finite_opponent_chroma_target() {
        let color = Srgb8::new([128, 96, 48]);
        let reference = ReferenceTransportV1::new(color, color);
        assert_eq!(reference.reference_contrast(), 1.0);
        assert_eq!(reference.reference_polarity(), TonalPolarityV1::Equal);
        for backdrop in [color, Srgb8::new([0; 3]), Srgb8::new([255; 3])] {
            let darker = reference.lab_for(backdrop, TonalPolarityV1::Darker);
            let lighter = reference.lab_for(backdrop, TonalPolarityV1::Lighter);
            assert_eq!(darker, lighter);
            assert_eq!(darker, reference.lab_for(backdrop, TonalPolarityV1::Equal));
            assert!(darker.iter().all(|value| value.is_finite()));
        }
    }

    #[test]
    fn bounds_cover_sampled_points_and_reference_exception() {
        let sources = [
            [0; 3],
            [255; 3],
            [16, 16, 18],
            [254, 254, 255],
            [255, 208, 0],
            [0, 122, 255],
        ];
        let backgrounds = [[0; 3], [255; 3], [80; 3], [0, 127, 255]];
        for source in sources {
            for background in backgrounds {
                let reference =
                    ReferenceTransportV1::new(Srgb8::new(source), Srgb8::new(background));
                for polarity in [
                    TonalPolarityV1::Darker,
                    TonalPolarityV1::Equal,
                    TonalPolarityV1::Lighter,
                ] {
                    let full = reference.virtual_lightness_bounds([0.0, 1.0], polarity);
                    for step in 0..=256 {
                        let y = f64::from(step) / 256.0;
                        let l = reference.target_for_luminance(y, polarity).lab[0];
                        let local = reference.virtual_lightness_bounds([y, y], polarity);
                        assert!(full[0] <= l && l <= full[1]);
                        assert!(local[0] <= l && l <= local[1]);
                    }
                    let y = reference.reference_backdrop_y;
                    let exact = reference.target_for_luminance(y, polarity).lab[0];
                    let local = reference.virtual_lightness_bounds(
                        [next_down(y).max(0.0), next_up(y).min(1.0)],
                        polarity,
                    );
                    assert!(local[0] <= exact && exact <= local[1]);
                }
            }
        }
    }
}
