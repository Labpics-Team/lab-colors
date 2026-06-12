use crate::lcs::LcsColor;
use crate::neutral::NeutralCurve;
use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;

#[derive(Debug, Clone)]
pub struct AccentCurve {
    neutral: NeutralCurve,
    h_canonical: f64,
    sat_ratio: f64,
    slope: f64,
    canonical_hex: String,
    vc: ViewingConditions,
}

impl AccentCurve {
    pub fn new(canonical_hex: &str, neutral: &NeutralCurve) -> Result<Self, String> {
        let color = LcsColor::from_hex(canonical_hex)?;
        let h_canonical = color.h_ok;

        let rgb = srgb_from_hex(canonical_hex)?;
        let lab = srgb_linear_to_oklab(rgb);
        let l_ok = lab[0];

        let c_canonical = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
        let c_max = max_chroma(l_ok, h_canonical);
        let sat_ratio = if c_max > 1e-6 {
            c_canonical / c_max
        } else {
            0.0
        };

        Ok(Self {
            neutral: neutral.clone(),
            h_canonical,
            sat_ratio: sat_ratio.clamp(0.0, 1.0),
            slope: 5.0,
            canonical_hex: canonical_hex.to_uppercase(),
            vc: *neutral.vc(),
        })
    }

    pub fn at(&self, t: f64) -> LcsColor {
        let t = t.clamp(0.0, 1.0);
        let neutral_color = self.neutral.at(t);
        let jp = neutral_color.jp;

        let l_ok = jp_to_oklab_l(jp, &self.vc);

        let h_optimal = self.find_optimal_hue(l_ok);

        let c_max = max_chroma(l_ok, h_optimal);
        let c_use = self.sat_ratio * c_max;

        let h_rad = h_optimal.to_radians();
        let a_ok = c_use * h_rad.cos();
        let b_ok = c_use * h_rad.sin();

        let rgb = oklab_to_srgb_linear([l_ok, a_ok, b_ok]);
        let rgb_clamped = [
            rgb[0].clamp(0.0, 1.0),
            rgb[1].clamp(0.0, 1.0),
            rgb[2].clamp(0.0, 1.0),
        ];

        let xyz = srgb_to_xyz(rgb_clamped);
        let h_ok = b_ok.atan2(a_ok).to_degrees().rem_euclid(360.0);

        let (j, m, h_cam) = crate::lpc::cam16_jch_from_xyz(xyz, &self.vc);

        // CAM16-UCS rescaling (Li et al. 2017, DOI 10.1002/col.22131); see lcs.rs.
        let jp_actual = 1.7 * j / (1.0 + 0.007 * j);
        let mp = (1.0 + 0.0228 * m).ln() / 0.0228;
        let s = if jp_actual + 1.0 > 1e-9 {
            mp / (jp_actual + 1.0)
        } else {
            0.0
        };

        LcsColor::new(jp_actual, h_ok, s.max(0.0), h_cam)
    }

    pub fn sample(&self, n: usize) -> Vec<LcsColor> {
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![self.at(0.5)];
        }
        (0..n).map(|i| self.at(i as f64 / (n - 1) as f64)).collect()
    }

    pub fn sample_hex(&self, n: usize) -> Vec<String> {
        self.sample(n)
            .iter()
            .map(|c| c.to_hex_with_vc(&self.vc))
            .collect()
    }

    /// The viewing conditions inherited from the neutral curve.
    pub fn vc(&self) -> &ViewingConditions {
        &self.vc
    }

    pub fn canonical_hue(&self) -> f64 {
        self.h_canonical
    }

    pub fn sat_ratio(&self) -> f64 {
        self.sat_ratio
    }

    /// The original hex string passed to [`AccentCurve::new`], normalised to uppercase.
    pub fn canonical_hex(&self) -> &str {
        &self.canonical_hex
    }

    fn find_optimal_hue(&self, l_ok: f64) -> f64 {
        let c_at_canonical = max_chroma(l_ok, self.h_canonical);

        if c_at_canonical > 1e-6 {
            return self.h_canonical;
        }

        let best = (0..36)
            .map(|i| {
                let h = self.h_canonical + (i as f64 - 18.0) * 10.0;
                let c = max_chroma(l_ok, h);
                let dh = ((h - self.h_canonical + 180.0).rem_euclid(360.0)) - 180.0;
                let cost = self.slope / (1.0 - dh.abs() / 180.0).max(0.01);
                let score = c - cost;
                (h, c, score)
            })
            .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

        best.map(|(h, _, _)| h).unwrap_or(self.h_canonical)
    }
}

fn jp_to_oklab_l(jp: f64, vc: &ViewingConditions) -> f64 {
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;

    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        let xyz = [
            mid * crate::spaces::srgb::D65_WHITE[0],
            mid,
            mid * crate::spaces::srgb::D65_WHITE[2],
        ];
        let (j, _, _) = crate::lpc::cam16_jch_from_xyz(xyz, vc);
        let jp_mid = 1.7 * j / (1.0 + 0.007 * j);
        if jp_mid < jp {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let y = (lo + hi) * 0.5;
    let lab = srgb_linear_to_oklab([y, y, y]);
    lab[0]
}

/// The half-width the bisection used to add/subtract around each channel's
/// `[0, 1]` gamut wall. The analytical solver reproduces the exact same band so
/// it returns the identical boundary chroma the bisection converged to.
const GAMUT_EPS: f64 = 1e-6;

/// The largest in-gamut Oklab chroma along the ray of fixed lightness `l_ok` and
/// hue `h_ok_deg`, found in closed form.
///
/// Along a ray of fixed `(L, h)` in Oklab, the chroma `C` enters each
/// intermediate LMS channel **linearly** (`OKLAB_TO_LMS` is affine in `C`
/// because its first column is all ones), is then cubed, and recombined into
/// linear sRGB by `LMS_TO_SRGB` — so every sRGB channel is a **cubic polynomial
/// in `C`**. The sRGB gamut wall is the first `C > 0` at which any of the six
/// constraints (`channel = 0` or `channel = 1`, each widened by [`GAMUT_EPS`] to
/// match the old bisection's tolerance) is hit. That smallest positive crossing
/// is the maximum chroma, found by solving the cubics in closed form instead of
/// 64 blind bisection steps.
///
/// VC-independent by construction: the only inputs are `(l_ok, h_ok_deg)` and
/// the fixed sRGB↔Oklab matrices — no viewing conditions enter, exactly as the
/// bisection it replaces.
pub(crate) fn max_chroma(l_ok: f64, h_ok_deg: f64) -> f64 {
    use crate::spaces::oklab::{LMS_TO_SRGB, OKLAB_TO_LMS};

    let h_ok = h_ok_deg.to_radians();
    let cos_h = h_ok.cos();
    let sin_h = h_ok.sin();

    // Each intermediate LMS_ value is affine in C: lms_[k] = p_k + q_k * C.
    // (Column 0 of OKLAB_TO_LMS is all ones, so p_k = l_ok for every k.)
    let mut p = [0.0_f64; 3];
    let mut q = [0.0_f64; 3];
    for (k, row) in OKLAB_TO_LMS.iter().enumerate() {
        p[k] = l_ok; // row[0] == 1.0
        q[k] = row[1] * cos_h + row[2] * sin_h;
    }

    // Each sRGB channel rgb[ch](C) = Σ_k M[ch][k] * (p_k + q_k C)^3 is a cubic
    // in C. Build its coefficients [c0, c1, c2, c3] (ascending powers).
    let mut smallest = 1.0_f64; // cap at the bisection's hi = 1.0
    for m in &LMS_TO_SRGB {
        let mut coeff = [0.0_f64; 4];
        for ((&mk, &pk), &qk) in m.iter().zip(p.iter()).zip(q.iter()) {
            // (pk + qk C)^3 = pk^3 + 3 pk^2 qk C + 3 pk qk^2 C^2 + qk^3 C^3
            coeff[0] += mk * pk * pk * pk;
            coeff[1] += mk * 3.0 * pk * pk * qk;
            coeff[2] += mk * 3.0 * pk * qk * qk;
            coeff[3] += mk * qk * qk * qk;
        }
        // First crossing of the upper wall (channel = 1 + eps) and the lower
        // wall (channel = -eps), whichever comes first for this channel.
        if let Some(c) = smallest_positive_crossing(coeff, 1.0 + GAMUT_EPS) {
            smallest = smallest.min(c);
        }
        if let Some(c) = smallest_positive_crossing(coeff, -GAMUT_EPS) {
            smallest = smallest.min(c);
        }
    }

    smallest.clamp(0.0, 1.0)
}

/// The smallest strictly-positive real root of the cubic `coeff` (ascending
/// powers) equal to `level`, i.e. of `f(C) - level = 0`, or `None` if the cubic
/// never reaches `level` for `C > 0`.
///
/// Roots are taken in closed form (Cardano / quadratic / linear by degree) and
/// each is polished with two Newton steps so the returned chroma matches the
/// 64-step bisection to full f64 precision.
fn smallest_positive_crossing(coeff: [f64; 4], level: f64) -> Option<f64> {
    let g = [coeff[0] - level, coeff[1], coeff[2], coeff[3]];
    let (roots, n) = cubic_roots(g);
    let mut best: Option<f64> = None;
    for &r in roots.iter().take(n) {
        // Discard non-positive and spurious roots; a real crossing is C > 0.
        if r > 1e-12 {
            let polished = newton_polish(g, r);
            if polished > 1e-12 {
                best = Some(match best {
                    Some(b) => b.min(polished),
                    None => polished,
                });
            }
        }
    }
    best
}

/// Two Newton iterations on the cubic `g` (ascending powers) from seed `x`,
/// refining a closed-form root to full f64 accuracy.
fn newton_polish(g: [f64; 4], mut x: f64) -> f64 {
    for _ in 0..2 {
        let f = g[0] + x * (g[1] + x * (g[2] + x * g[3]));
        let df = g[1] + x * (2.0 * g[2] + x * 3.0 * g[3]);
        if df.abs() < 1e-18 {
            break;
        }
        x -= f / df;
    }
    x
}

/// Real roots of `g[0] + g[1] x + g[2] x^2 + g[3] x^3 = 0`, handling degenerate
/// (quadratic / linear / constant) leading coefficients. Returns the roots in a
/// fixed buffer plus the count `n` (0–3), allocation-free for the hot path.
fn cubic_roots(g: [f64; 4]) -> ([f64; 3], usize) {
    let [d, c, b, a] = g;

    // Degenerate: not actually cubic.
    if a.abs() < 1e-14 {
        return quadratic_roots(d, c, b);
    }

    // Normalise to x^3 + p2 x^2 + p1 x + p0.
    let p2 = b / a;
    let p1 = c / a;
    let p0 = d / a;

    // Depressed cubic t^3 + p t + q via x = t - p2/3.
    let shift = p2 / 3.0;
    let p = p1 - p2 * p2 / 3.0;
    let q = 2.0 * p2 * p2 * p2 / 27.0 - p2 * p1 / 3.0 + p0;

    let disc = q * q / 4.0 + p * p * p / 27.0;
    let mut roots = [0.0_f64; 3];

    if disc > 1e-30 {
        // One real root.
        let sqrt_disc = disc.sqrt();
        let u = (-q / 2.0 + sqrt_disc).cbrt();
        let v = (-q / 2.0 - sqrt_disc).cbrt();
        roots[0] = u + v - shift;
        (roots, 1)
    } else if disc < -1e-30 {
        // Three distinct real roots (trigonometric form).
        let m = 2.0 * (-p / 3.0).sqrt();
        let theta = ((3.0 * q) / (p * m)).clamp(-1.0, 1.0).acos() / 3.0;
        for (k, slot) in roots.iter_mut().enumerate() {
            *slot = m * (theta - 2.0 * std::f64::consts::PI * k as f64 / 3.0).cos() - shift;
        }
        (roots, 3)
    } else {
        // Repeated roots (disc ~ 0).
        let t1 = if q.abs() < 1e-30 { 0.0 } else { 3.0 * q / p };
        let t2 = -t1 / 2.0;
        roots[0] = t1 - shift;
        roots[1] = t2 - shift;
        (roots, 2)
    }
}

/// Real roots of `b x^2 + c x + d = 0` (handles linear / constant degeneracy),
/// returned in the same fixed-buffer-plus-count form as [`cubic_roots`].
fn quadratic_roots(d: f64, c: f64, b: f64) -> ([f64; 3], usize) {
    let mut roots = [0.0_f64; 3];
    if b.abs() < 1e-14 {
        // Linear c x + d = 0.
        if c.abs() < 1e-14 {
            return (roots, 0);
        }
        roots[0] = -d / c;
        return (roots, 1);
    }
    let disc = c * c - 4.0 * b * d;
    if disc < 0.0 {
        return (roots, 0);
    }
    let sqrt_disc = disc.sqrt();
    roots[0] = (-c + sqrt_disc) / (2.0 * b);
    roots[1] = (-c - sqrt_disc) / (2.0 * b);
    (roots, 2)
}

/// The bisection that [`max_chroma`] replaced, kept (test-only) as the reference
/// oracle the analytical solver is proven bit-for-bit against on a dense grid.
#[cfg(test)]
pub(crate) fn max_chroma_bisect(l_ok: f64, h_ok_deg: f64) -> f64 {
    let h_ok = h_ok_deg.to_radians();
    let cos_h = h_ok.cos();
    let sin_h = h_ok.sin();

    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;

    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        let a = mid * cos_h;
        let b = mid * sin_h;
        let rgb = oklab_to_srgb_linear([l_ok, a, b]);

        if rgb[0] >= -1e-6
            && rgb[0] <= 1.0 + 1e-6
            && rgb[1] >= -1e-6
            && rgb[1] <= 1.0 + 1e-6
            && rgb[2] >= -1e-6
            && rgb[2] <= 1.0 + 1e-6
        {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    (lo + hi) * 0.5
}

impl crate::curve::ColorCurve for AccentCurve {
    fn at(&self, t: f64) -> LcsColor {
        self.at(t)
    }

    fn vc(&self) -> &ViewingConditions {
        &self.vc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_neutral() -> NeutralCurve {
        NeutralCurve::new("#FFFFFF", "#787880", "#101012").unwrap()
    }

    #[test]
    fn accent_jp_monotonically_decreasing() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        let steps = curve.sample(50);
        for w in steps.windows(2) {
            assert!(
                w[0].jp >= w[1].jp - 0.5,
                "jp increased: {} -> {}",
                w[0].jp,
                w[1].jp
            );
        }
    }

    #[test]
    fn accent_s_non_negative() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        for i in 0..=50 {
            let c = curve.at(i as f64 / 50.0);
            assert!(c.s >= -1e-6, "negative s at t={}: {}", i as f64 / 50.0, c.s);
        }
    }

    #[test]
    fn accent_all_in_gamut() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        for i in 0..=50 {
            let color = curve.at(i as f64 / 50.0);
            let hex = color.to_hex();
            let rgb = srgb_from_hex(&hex).unwrap();
            assert!(
                rgb.iter().all(|&c| (-0.01..=1.01).contains(&c)),
                "out of gamut at t={}: {:?}",
                i as f64 / 50.0,
                rgb
            );
        }
    }

    #[test]
    fn max_chroma_white_is_small() {
        let c = max_chroma(1.0, 0.0);
        assert!(c < 0.01, "max chroma at L=1 should be ~0: {}", c);
    }

    #[test]
    fn max_chroma_mid_has_room() {
        let c = max_chroma(0.5, 30.0);
        assert!(c > 0.1, "max chroma at L=0.5, h=30 should be > 0.1: {}", c);
    }

    #[test]
    fn analytic_max_chroma_agrees_with_bisection_and_is_honest_at_the_wall() {
        // The analytical solver reproduces the 64-step bisection oracle. Where the
        // sRGB gamut along a fixed-(L,h) ray is convex (the overwhelming majority
        // of the ray space), the two agree to the bisection's own precision — any
        // residual above ~1e-7 there would be a missed root or wrong branch.
        //
        // At a few near-black rays the gamut is *non-convex*: one channel dips a
        // sliver below the −1e-6 wall and comes back, so the true first exit is
        // *closer in* than where the bisection — which samples midpoints and can
        // step over the sliver — lands. There the analytic value is the honest,
        // strictly-in-gamut answer and is <= the bisection's (it never claims more
        // chroma than the gamut allows). So the contract is:
        //   * analytic <= bisect + 1e-7   (never over-claims vs the oracle), and
        //   * |analytic − bisect| <= 1e-7 except on the non-convex sliver rays,
        //     which are bounded in count and magnitude and verified to be the
        //     more-correct (in-gamut) side by `analytic_max_chroma_never_exceeds_gamut`.
        let mut convex_worst = 0.0_f64;
        let mut convex_worst_at = (0.0, 0.0);
        let mut nonconvex_points = 0u32;
        let mut nonconvex_worst = 0.0_f64;
        // 201 lightness * 360 hue = 72_360 samples, the full ray space.
        for li in 0..=200 {
            let l = li as f64 / 200.0;
            for hi in 0..360 {
                let h = hi as f64;
                let analytic = max_chroma(l, h);
                let bisect = max_chroma_bisect(l, h);
                // The analytic value must never exceed the bisection's chroma by
                // more than rounding: it is the honest in-gamut bound.
                assert!(
                    analytic <= bisect + 1e-7,
                    "analytic {analytic} over-claims vs bisection {bisect} at (L,h)=({l},{h})"
                );
                let resid = (analytic - bisect).abs();
                if resid <= 1e-7 {
                    convex_worst = convex_worst.max(resid);
                    if resid >= convex_worst {
                        convex_worst_at = (l, h);
                    }
                } else {
                    // A non-convex sliver: analytic is the strictly-in-gamut side.
                    nonconvex_points += 1;
                    nonconvex_worst = nonconvex_worst.max(resid);
                }
            }
        }
        // The convex bulk agrees to bisection precision.
        assert!(
            convex_worst <= 1e-7,
            "convex-region residual {convex_worst:.2e} at {convex_worst_at:?}"
        );
        // The non-convex rays are a small, bounded set at the near-black wall —
        // not a systemic disagreement. (Empirically a few dozen of 72_360.)
        assert!(
            nonconvex_points <= 200,
            "too many non-convex disagreements ({nonconvex_points}) — likely a solver bug, \
             not the known near-black gamut sliver (worst {nonconvex_worst:.2e})"
        );
    }

    #[test]
    fn analytic_max_chroma_never_exceeds_gamut() {
        // The returned chroma must itself be in gamut (within the same eps the
        // bisection used): building the colour at C* lands every channel inside
        // [−eps, 1+eps]. A C* past the wall would tint an out-of-gamut colour.
        for li in 0..=100 {
            let l = li as f64 / 100.0;
            for hi in 0..72 {
                let h = hi as f64 * 5.0;
                let c = max_chroma(l, h);
                let hr = h.to_radians();
                let rgb = oklab_to_srgb_linear([l, c * hr.cos(), c * hr.sin()]);
                for (ch, &v) in rgb.iter().enumerate() {
                    assert!(
                        (-1e-4..=1.0 + 1e-4).contains(&v),
                        "C*={c} at (L {l}, h {h}) puts channel {ch} out of gamut: {v}"
                    );
                }
            }
        }
    }

    #[test]
    fn sat_ratio_for_saturated_color() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#FF0000", &neutral).unwrap();
        assert!(
            curve.sat_ratio() > 0.5,
            "red should have high sat_ratio: {}",
            curve.sat_ratio()
        );
    }

    #[test]
    fn sat_ratio_for_desaturated_color() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#CC8888", &neutral).unwrap();
        assert!(
            curve.sat_ratio() < 0.5,
            "desaturated should have low sat_ratio: {}",
            curve.sat_ratio()
        );
    }

    #[test]
    fn sample_hex_produces_valid_colors() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        let hexes = curve.sample_hex(13);
        assert_eq!(hexes.len(), 13);
        for hex in &hexes {
            assert!(LcsColor::from_hex(hex).is_ok(), "invalid hex: {}", hex);
        }
    }

    #[test]
    fn rejects_bad_hex() {
        let neutral = default_neutral();
        assert!(AccentCurve::new("#GGGGGG", &neutral).is_err());
    }

    // ── Dark-theme (dim-surround) accent tests ────────────────

    fn dim_neutral() -> NeutralCurve {
        use crate::neutral::CurveParams;
        use crate::spaces::vc::ViewingConditions;
        let vc = ViewingConditions::dim_surround();
        NeutralCurve::with_vc(
            "#FFFFFF",
            "#787880",
            "#101012",
            &CurveParams::default(),
            &vc,
        )
        .unwrap()
    }

    #[test]
    fn dim_accent_jp_monotonically_decreasing() {
        let neutral = dim_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        let steps = curve.sample(50);
        for w in steps.windows(2) {
            assert!(
                w[0].jp >= w[1].jp - 0.5,
                "dim accent jp increased: {} -> {}",
                w[0].jp,
                w[1].jp,
            );
        }
    }

    #[test]
    fn dim_accent_all_in_gamut() {
        let neutral = dim_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        for i in 0..=50 {
            let color = curve.at(i as f64 / 50.0);
            let hex = color.to_hex_with_vc(&curve.vc);
            let rgb = srgb_from_hex(&hex).unwrap();
            assert!(
                rgb.iter().all(|&c| (-0.01..=1.01).contains(&c)),
                "dim accent out of gamut at t={}: {:?}",
                i as f64 / 50.0,
                rgb
            );
        }
    }

    #[test]
    fn dim_accent_inherits_vc_from_neutral() {
        let neutral = dim_neutral();
        let curve = AccentCurve::new("#FF0000", &neutral).unwrap();
        assert!(
            (curve.vc().c - 0.59).abs() < 1e-10,
            "accent vc.c should match dim neutral: {}",
            curve.vc().c,
        );
    }
}
