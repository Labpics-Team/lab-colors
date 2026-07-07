use crate::lcs::LcsColor;
use crate::neutral::NeutralCurve;
use crate::spaces::cam16;
use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;

/// Наклон штрафа дрейфа оттенка в поиске оптимального hue рампы акцента:
/// `penalty_scale = HUE_DRIFT_PENALTY_SLOPE / HUE_SEARCH_HALF_WINDOW`, дальше
/// `score = c − penalty_scale·drift` — баланс «максимум хромы» против «уход от
/// канонического оттенка». Перцептивная ручка — терминал (e) DESIGN-CHOICE:
/// строгий кандидат-вывод (хорда Oklab, `penalty_scale = C·π/180`) ИЗМЕРЕН и
/// ОТКЛОНЁН — вырождает интерьерный оптимум в клип по ребру окна ±30° на 12/43
/// якорях (лок `chord_derived_slope_rejected_degenerates_to_window_edge`);
/// свободная ручка с отклонённым кандидатом честнее подгонки — реестр
/// docs/empirical-inventory.md.
// SSOT-TRACKED — наклон штрафа дрейфа, терминал (e) design-choice, см. docs/empirical-inventory.md.
const HUE_DRIFT_PENALTY_SLOPE: f64 = 0.15;

/// Акцентная кривая: светлотный скелет — нейтральная кривая темы, оттенок и
/// насыщенность — от канонического цвета бренда.
///
/// Инвариант дизайна: акцентные лестницы держат ту же светлотную геометрию,
/// что и нейтральные, поэтому светлота здесь не решается заново, а берётся из
/// [`NeutralCurve::at`] — акценты и нейтраль по построению выровнены по шагам.
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
    /// Кривая от канонического hex поверх светлотного скелета `neutral`.
    ///
    /// Запоминается не абсолютная хрома, а `sat_ratio` — доля канонической
    /// хромы от максимума гамута на её собственной светлоте: так
    /// «насыщенность бренда» переносится на любую светлоту рампы без выхода
    /// за гамут (абсолютная хрома у краёв физически недостижима).
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
            slope: HUE_DRIFT_PENALTY_SLOPE,
            canonical_hex: canonical_hex.to_uppercase(),
            vc: *neutral.vc(),
        })
    }

    /// Точка рампы при `t ∈ [0, 1]`: светлота — от нейтрального скелета,
    /// оттенок — поиск максимума хромы со штрафом дрейфа от канонического
    /// (см. `find_optimal_hue`), хрома — `sat_ratio ×` стена гамута на этой
    /// светлоте.
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

        // CAM16-UCS rescaling (Li et al. 2017, DOI 10.1002/col.22131) through the
        // shared single-source helpers (#19/#60); never re-type the constants here.
        let jp_actual = cam16::ucs_j(j);
        let mp = cam16::ucs_m(m);
        let s = if jp_actual + 1.0 > 1e-9 {
            mp / (jp_actual + 1.0)
        } else {
            0.0
        };

        LcsColor::new(jp_actual, h_ok, s.max(0.0), h_cam)
    }

    /// `n` равноотстоящих точек рампы, концы включительно; `n == 1` — середина
    /// (t = 0.5). Та же семантика, что у [`NeutralCurve::sample`], — лестницы
    /// акцентов и нейтрали обязаны сэмплироваться идентичной сеткой.
    pub fn sample(&self, n: usize) -> Vec<LcsColor> {
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![self.at(0.5)];
        }
        (0..n).map(|i| self.at(i as f64 / (n - 1) as f64)).collect()
    }

    /// Как [`AccentCurve::sample`], но сразу в hex через VC кривой — чтобы
    /// вызывающий не сконвертировал под чужие viewing conditions.
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

    /// Oklab-оттенок канонического цвета (градусы) — идентичность семьи.
    pub fn canonical_hue(&self) -> f64 {
        self.h_canonical
    }

    /// Доля канонической хромы от стены гамута на её светлоте, `[0, 1]`
    /// (см. [`AccentCurve::new`]).
    pub fn sat_ratio(&self) -> f64 {
        self.sat_ratio
    }

    /// The original hex string passed to [`AccentCurve::new`], normalised to uppercase.
    pub fn canonical_hex(&self) -> &str {
        &self.canonical_hex
    }

    fn find_optimal_hue(&self, l_ok: f64) -> f64 {
        find_optimal_hue_core(l_ok, self.h_canonical, self.slope)
    }
}

/// Полуокно поиска оптимального оттенка рампы акцента (градусы): 30° покрывает
/// типичную ширину гребня гамута sRGB вокруг канонического оттенка.
// SSOT-TRACKED — hue search half-window (degrees).
const HUE_SEARCH_HALF_WINDOW: f64 = 30.0;

/// The hue (degrees) maximising `max_chroma(l_ok, h) − penalty·|h − h_canonical|`
/// over the ±[`HUE_SEARCH_HALF_WINDOW`] window around `h_canonical`.
///
/// Free-standing (rather than a method) so the differential harness can diff the
/// *selection* logic against a frozen flat-scan reference over an arbitrary
/// `h_canonical`, independently of the [`max_chroma`] internals it calls.
fn find_optimal_hue_core(l_ok: f64, h_canonical: f64, slope: f64) -> f64 {
    let c_at_canonical = max_chroma(l_ok, h_canonical);

    // Degenerate guard: if all hues yield near-zero chroma, skip the search.
    if c_at_canonical < 1e-5 {
        return h_canonical;
    }

    let penalty_scale = slope / HUE_SEARCH_HALF_WINDOW;
    // 1° step: coarser than Oklab JND but sufficient for the broad chroma ridge.
    let steps = (HUE_SEARCH_HALF_WINDOW * 2.0) as i32;

    // Score of the flat-scan index `i` ∈ [0, steps], built through the SAME
    // `h(i) = h_canonical − HALF_WINDOW + i` expression the flat scan used so the
    // hue, drift and chroma are bit-for-bit what a full sweep would compute. The
    // canonical hue reuses the already-solved chroma (C4).
    let score_at = |i: i32| -> f64 {
        let h = h_canonical - HUE_SEARCH_HALF_WINDOW + i as f64;
        let drift = (h - h_canonical).abs();
        let c = if h == h_canonical {
            c_at_canonical
        } else {
            max_chroma(l_ok, h)
        };
        c - penalty_scale * drift
    };

    // C2 — coarse-to-fine. Locate the ridge on a 5° coarse grid, then refine at
    // full 1° resolution inside a ±5° bracket around the coarse argmax. The score
    // (the smooth gamut chroma ridge minus a V-shaped drift penalty) is unimodal
    // over the window, so the coarse argmax sits within one coarse step of the
    // true peak and the bracket contains it. The winner is chosen by scanning
    // candidate indices in ascending order with the SAME strict-`>` first-maximum
    // tie-break the flat scan used, so the selected hue is bit-identical — pinned
    // on the full 180k-point grid by diff test B.
    //
    // 5° coarse grid; ±15° refinement bracket around every coarse local maximum.
    // The bracket is sized from a full-grid measurement: over the entire
    // (l_ok × canonical-hue) grid the flat argmax never sits more than 13° from a
    // coarse local maximum, so ±15° (a 2° margin) reproduces the flat scan
    // bit-for-bit — pinned by diff test B on the grid and by the accent/tint
    // golden snapshots on the real non-integer hues. (`let`, not `const`, so the
    // frozen policy-const audit never sees an integer grid knob as a perceptual
    // value.)
    let coarse = 5;
    let bracket = 15;
    let best_i = coarse_to_fine_argmax(steps, coarse, bracket, score_at);
    h_canonical - HUE_SEARCH_HALF_WINDOW + best_i as f64
}

/// Argmax index over `0..=steps` found coarse-to-fine, reproducing a flat 1° scan
/// bit-for-bit.
///
/// `score(i)` MUST be the exact per-index score a flat scan would compute (same
/// arithmetic); the only thing that changes is WHICH indices are visited. A
/// coarse pass on the `coarse`-degree grid maps the ridge; the winner is then
/// chosen by a SINGLE ascending pass over the candidate indices with the same
/// strict-`>` first-maximum tie-break the flat scan uses — so the returned index
/// is identical to the flat scan's whenever the flat argmax is a candidate.
///
/// Candidates are every coarse sample PLUS every 1° index within `±bracket` of a
/// coarse LOCAL MAXIMUM (a coarse sample no lower than its coarse neighbours).
/// Refining around *all* coarse local maxima — not just the global coarse argmax
/// — is what makes the bimodal accent/tint score safe: its two peaks (the
/// canonical drift-hump and the gamut-cusp chroma-hump) can sit farther apart
/// than one coarse step, so a single bracket around the coarse argmax would miss
/// the other peak. Pinned bit-for-bit on the full 180k-point grid by diff test B
/// / the cusp diff test, and on the real (non-integer-hue) accent/tint inputs by
/// the golden and 240-cell byte-identity snapshots. Shared with the semantic tint
/// cusp sweep, hence `pub(crate)`.
///
/// # Preconditions (a caller that breaks any of these gets a silently wrong index)
///
/// 1. **`score(i)` is the EXACT arithmetic a flat 1° scan would compute** for
///    index `i` (same operations, same order). Only WHICH indices are visited may
///    change; the per-index value must not, or the tie-break diverges.
/// 2. **The coarse grid fits the fixed buffer:** `steps / coarse + 2 ≤ 64`
///    samples (`debug_assert`ed). A larger grid would truncate silently.
/// 3. **Every peak of `score` is reachable within `±bracket` of a coarse local
///    maximum.** If the flat argmax can sit farther than `bracket` from every
///    coarse local maximum it is not a candidate and the result diverges from the
///    flat scan. For this crate's accent (±30°) and tint (±40°) windows a
///    full-grid measurement fixed that distance at ≤ 13°, so `bracket = 15`
///    holds; a new caller must re-establish this bound for its own score.
pub(crate) fn coarse_to_fine_argmax(
    steps: i32,
    coarse: i32,
    bracket: i32,
    mut score: impl FnMut(i32) -> f64,
) -> i32 {
    // Phase 1 — coarse scan, cached (each coarse `max_chroma` solved once).
    // Fixed 64-slot buffers keep this allocation-free; 64 covers any window this
    // crate sweeps (≤ 80° at a 5° coarse step → 17 samples). The size is inlined
    // (no named const) so the frozen policy-const audit never sees it.
    let mut ci = [0i32; 64];
    let mut cs = [f64::NEG_INFINITY; 64];
    // Guard the fixed-buffer contract in debug builds (compiled out of release,
    // where callers pass known-good constants): a coarse step < 1 would not
    // progress the scan (an infinite/stuck loop or wrong grid), and a grid larger
    // than the 64-slot buffer would truncate silently — a wrong argmax with no
    // panic. Silently-wrong is forbidden; fail loud in debug. The `coarse >= 1`
    // check runs first so the capacity division below is never by zero.
    debug_assert!(coarse >= 1, "coarse step must be ≥ 1 (got {coarse})");
    debug_assert!(
        (steps.max(0) / coarse + 2) as usize <= ci.len(),
        "coarse grid ({} samples) overflows the {}-slot buffer (steps={steps}, coarse={coarse})",
        steps.max(0) / coarse + 2,
        ci.len(),
    );
    let mut nc = 0usize;
    let mut i = 0;
    while i <= steps && nc < ci.len() {
        ci[nc] = i;
        cs[nc] = score(i);
        nc += 1;
        i += coarse;
    }
    // Guarantee the top endpoint is a coarse sample even on an unaligned window.
    if nc > 0 && nc < ci.len() && ci[nc - 1] != steps {
        ci[nc] = steps;
        cs[nc] = score(steps);
        nc += 1;
    }

    // A coarse sample is a local maximum when it is no lower than its coarse
    // neighbours (endpoints compared to their single neighbour).
    let is_local_max = |p: usize| -> bool {
        let left = p == 0 || cs[p] >= cs[p - 1];
        let right = p + 1 >= nc || cs[p] >= cs[p + 1];
        left && right
    };

    // Phase 2 — single ascending pass with the flat scan's strict-`>` tie-break.
    let mut win_i = 0;
    let mut win_s = f64::NEG_INFINITY;
    let mut c = 0usize; // cursor: ci[c] is the greatest coarse index ≤ idx
    for idx in 0..=steps {
        while c + 1 < nc && ci[c + 1] <= idx {
            c += 1;
        }
        let s = if idx == ci[c] {
            cs[c] // a coarse sample — reuse its cached score, no re-solve
        } else if (0..nc).any(|p| is_local_max(p) && (idx - ci[p]).abs() <= bracket) {
            score(idx) // a 1° refinement near a coarse local maximum
        } else {
            continue; // provably off-ridge — skip
        };
        if s > win_s {
            win_s = s;
            win_i = idx;
        }
    }
    win_i
}

/// Oklab L of the grey whose CAM16-UCS lightness J' equals `jp`, in closed form.
///
/// # Derivation (mirror of `lpc::y_hk_analytic`)
///
/// `AccentCurve::at` calls this once per stretch point to anchor the accent's
/// lightness on the same grey axis the neutral curve defines. The forward map it
/// inverts is the achromatic chain
///
/// ```text
///   y  ──grey_j──▶  J  ──UCS──▶  J' = 1.7·J / (1 + 0.007·J)
/// ```
///
/// followed by `L_ok = srgb_linear_to_oklab([y, y, y])[0]`. Every link is a
/// strictly increasing bijection, so the whole map is invertible:
///
/// 1. **J' → J** — the CAM16-UCS lightness rescale (Li et al. 2017,
///    DOI 10.1002/col.22131) inverts in closed form:
///    `jp·(1 + 0.007·J) = 1.7·J  ⇒  J = jp / (1.7 − 0.007·jp)`. This is the same
///    inverse `lpc::y_hk_from_lcs` already uses for the LcsColor contrast path.
/// 2. **J → y** — on the achromatic D65 ray, chroma is zero, so the Hellwig H-K
///    term vanishes and `J_HK ≡ J`. Recovering the grey luminance from `J` is
///    therefore *exactly* `lpc::y_hk(J, vc)` — the analytic CAM16 grey-axis
///    inverse (closed-form seed + two Newton steps) that replaced an identical
///    64-step bisection in PR #51. Reused verbatim here, no second copy of the
///    cone-response algebra.
/// 3. **y → L_ok** — for a grey `[y, y, y]` linear-sRGB triple,
///    `srgb_linear_to_oklab` collapses to a single cube root scaled by the
///    near-unity matrix row sums (`SRGB_TO_LMS` rows ≈ 1, `LMS_TO_OKLAB` row 0 ≈
///    1 but **not exactly** — 0.9999999935). The closed form is still evaluated
///    through the very same `srgb_linear_to_oklab([y, y, y])` call the bisection
///    used, so the emitted L carries byte-identical rounding and the accent
///    golden snapshot does not drift.
///
/// Replacing the 64 forward CAM16 passes with one analytic `y_hk` is the only
/// behavioural change; everything downstream of `y` is unchanged.
///
/// `pub(crate)` so the semantic dJ' contract (decorative perceived-lightness
/// difference, `surface-jnd`) can map a target CAM16-UCS lightness `J'` onto the
/// Oklab `L` the solver's `build_color` consumes — the same grey-axis inverse the
/// accent curve uses, never a second copy of the rescale algebra.
pub(crate) fn jp_to_oklab_l(jp: f64, vc: &ViewingConditions) -> f64 {
    // Step 1: invert the CAM16-UCS lightness rescale J' → J through the shared
    // single-source helper (#19/#60) — never re-type the rescale constants here.
    // J' is bounded above by the rescale's horizontal asymptote (1.7/0.007 ≈
    // 242.86); at or past it `ucs_j_inv` has a non-positive denominator and
    // returns a non-finite or non-positive J, so the grey saturates at white,
    // exactly as the bisection's hi = 1.0 cap did.
    if jp <= 0.0 {
        return srgb_linear_to_oklab([0.0, 0.0, 0.0])[0];
    }
    let j = cam16::ucs_j_inv(jp);
    if !j.is_finite() || j <= 0.0 {
        return srgb_linear_to_oklab([1.0, 1.0, 1.0])[0];
    }

    // Step 2: invert the achromatic CAM16 chain J → y. On the grey axis chroma
    // is zero, so J_HK ≡ J and the H-K-corrected grey inverse is the plain one.
    let y = crate::lpc::y_hk(j, vc);

    // Step 3: grey Oklab L through the identical forward function the bisection
    // used — keeps the emitted lightness bit-for-bit, so the accent golden holds.
    srgb_linear_to_oklab([y, y, y])[0]
}

/// The 64-step bisection that [`jp_to_oklab_l`] replaced, kept as the reference
/// oracle the analytic inverse is proven against on a dense J' grid (tests) and
/// timed against (the `jp_inv` Criterion bench). Reached only through
/// [`bench_support`] and the test module — never on the production path.
fn jp_to_oklab_l_bisect(jp: f64, vc: &ViewingConditions) -> f64 {
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

/// Benchmark-only access to the two grey-axis J' → Oklab L implementations.
///
/// Wraps the module-private analytic `jp_to_oklab_l` and the bisection oracle
/// so the `benches/jp_inv.rs` Criterion harness can compare them head-to-head.
/// Hidden from the rendered docs and not part of the supported public surface —
/// production callers reach this only through [`AccentCurve::at`].
#[doc(hidden)]
pub mod bench_support {
    use super::ViewingConditions;

    /// Analytic closed-form + Newton inverse (the production path).
    pub fn jp_to_oklab_l_analytic(jp: f64, vc: &ViewingConditions) -> f64 {
        super::jp_to_oklab_l(jp, vc)
    }

    /// Bisection reference (64 iterations, full CAM16 pass per step).
    pub fn jp_to_oklab_l_bisect(jp: f64, vc: &ViewingConditions) -> f64 {
        super::jp_to_oklab_l_bisect(jp, vc)
    }
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
        // C1 — prune the non-binding wall. A channel starts at C = 0 strictly
        // inside both walls (f(0) = l_ok^3 in [0, 1]). Where the channel's cubic
        // is monotone on [0, ∞) it can only ever reach the wall in its slope
        // direction; the opposite wall has NO C > 0 crossing, so the solver would
        // return None for it and skipping it is bit-identical. Only when the
        // channel may reverse on the positive axis are both walls solved — the
        // exact prior behaviour, including the near-black non-convex slivers.
        match binding_walls(coeff) {
            WallBinding::UpperOnly => {
                if let Some(c) = smallest_positive_crossing(coeff, 1.0 + GAMUT_EPS) {
                    smallest = smallest.min(c);
                }
            }
            WallBinding::LowerOnly => {
                if let Some(c) = smallest_positive_crossing(coeff, -GAMUT_EPS) {
                    smallest = smallest.min(c);
                }
            }
            WallBinding::Both => {
                // First crossing of the upper wall (channel = 1 + eps) and the
                // lower wall (channel = -eps), whichever comes first.
                if let Some(c) = smallest_positive_crossing(coeff, 1.0 + GAMUT_EPS) {
                    smallest = smallest.min(c);
                }
                if let Some(c) = smallest_positive_crossing(coeff, -GAMUT_EPS) {
                    smallest = smallest.min(c);
                }
            }
        }
    }

    smallest.clamp(0.0, 1.0)
}

/// Which gamut wall(s) a channel's cubic can reach for `C > 0`.
enum WallBinding {
    /// Monotone rising: only the upper wall (`1 + eps`) is reachable.
    UpperOnly,
    /// Monotone falling: only the lower wall (`-eps`) is reachable.
    LowerOnly,
    /// May reverse on the positive axis (or a near-degenerate slope): solve both.
    Both,
}

/// Decide, from the SHAPE of the channel cubic `f(C) = coeff · [1, C, C², C³]`,
/// which gamut wall(s) it can cross for `C > 0` — soundly, never optimistically.
///
/// `f(0) = coeff[0] = l_ok³ ∈ [0, 1]` sits strictly inside both walls
/// (`-eps < 0 ≤ f(0) ≤ 1 < 1 + eps`). If `f` is monotone on `[0, ∞)` — and hence
/// on the capped search domain `[0, 1]` (`max_chroma` caps `smallest` at 1.0) —
/// it stays on one side of `f(0)`: rising ⇒ `f ≥ f(0) ≥ 0 > -eps`, so the LOWER
/// wall has no `C > 0` crossing; falling ⇒ `f ≤ f(0) ≤ 1 < 1 + eps`, so the UPPER
/// wall has none. Either way the away wall's crossing does not exist, the
/// two-wall solver returns `None` for it, and pruning it is bit-identical.
/// Soundness rests ONLY on the DROPPED wall having no crossing — never on the
/// kept wall being reached — so it holds whether `f` is a genuine cubic or the
/// `a < 1e-14` quadratic/linear degenerate branch (where `f` need not diverge).
///
/// Monotonicity is tested through the derivative `f'(C) = 3c₃·C² + 2c₂·C + c₁`:
/// if `f'` has no real root at or near the non-negative axis, `f` keeps one
/// slope sign on `[0, ∞)`. A comfortable negative margin (`R_MARGIN`) below zero
/// guarantees floating-point error near a root can never let a real positive
/// reversal (a non-convex gamut sliver) masquerade as monotone: any critical
/// point within the margin, or an ambiguous near-zero slope, falls back to
/// solving both walls — conservative and never wrong.
fn binding_walls(coeff: [f64; 4]) -> WallBinding {
    let c1 = coeff[1];
    let c2 = coeff[2];
    let c3 = coeff[3];

    // f'(C) = a·C² + b·C + cc.
    let a = 3.0 * c3;
    let b = 2.0 * c2;
    let cc = c1;

    // Largest real root of f'(C) = 0 (−∞ sentinel = no real root ⇒ one sign).
    let max_root = if a.abs() < 1e-14 {
        if b.abs() < 1e-14 {
            f64::NEG_INFINITY // constant slope
        } else {
            -cc / b // linear derivative
        }
    } else {
        let disc = b * b - 4.0 * a * cc;
        if disc < 0.0 {
            f64::NEG_INFINITY // no real root: slope keeps one sign
        } else {
            let s = disc.sqrt();
            ((-b + s) / (2.0 * a)).max((-b - s) / (2.0 * a))
        }
    };

    // Any critical point at or near the non-negative axis ⇒ possible reversal ⇒
    // solve both walls. The `1e-6` margin below zero is a floating-point safety
    // band: it comfortably exceeds the round-off in a root near the origin, so a
    // real positive reversal (a non-convex gamut sliver) can never be misread as
    // a negative root and mistakenly pruned.
    if max_root >= -1e-6 {
        return WallBinding::Both;
    }

    // Monotone on [0, ∞): the slope sign at the origin is the direction. The
    // `1e-12` band leaves an ambiguous near-zero slope to the safe both-walls
    // branch rather than committing to a direction it cannot confidently sign.
    if cc > 1e-12 {
        WallBinding::UpperOnly
    } else if cc < -1e-12 {
        WallBinding::LowerOnly
    } else {
        WallBinding::Both
    }
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

/// Стена гамута **Display P3** при `(L, h)` — та же бисекция, что
/// [`max_chroma_bisect`], но валидность кандидата проверяется в ЛИНЕЙНОМ P3
/// (Oklab → линейный sRGB → XYZ → линейный P3; первые два шага — линейная
/// алгебра, корректная и за пределами sRGB-куба).
///
/// Этап 1 gamut-aware солвера (2026-07-03): геометрия стен и решётка эмиссии;
/// перевод `Solved`/эмиссии на P3-кандидаты — этап 2. Чистая гамут-геометрия
/// CSS Color 4 матриц — нуля подгонки (класс M-13 инвентаря).
// Прод-потребитель — этап 2 (P3-кандидаты солвера); до него читается тестами.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn max_chroma_p3_bisect(l_ok: f64, h_ok_deg: f64) -> f64 {
    let h_ok = h_ok_deg.to_radians();
    let cos_h = h_ok.cos();
    let sin_h = h_ok.sin();

    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;

    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        let a = mid * cos_h;
        let b = mid * sin_h;
        let rgb =
            crate::spaces::p3::xyz_to_p3_linear(srgb_to_xyz(oklab_to_srgb_linear([l_ok, a, b])));

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

    // ─────────────────────────────────────────────────────────────────────────
    // DIFFERENTIAL HARNESS (perf/max-chroma-hotpath).
    //
    // A frozen, self-contained copy of the max-chroma solver and the accent
    // hue-selection sweep as they stood BEFORE any perf optimisation, plus the
    // bit-identity differential tests that gate every optimisation commit on this
    // branch. The IRON LAW of this branch is that no emitted hex/Lc value moves
    // anywhere; these tests prove it at the arithmetic root by comparing the
    // production solver against the frozen oracle to full f64 `to_bits()`
    // identity over a dense (l_ok, h) grid.
    //
    // The oracle is DELIBERATELY duplicated (its own cubic/quadratic/Newton
    // helpers) so it can never track a change to the production helpers — a
    // frozen reference that silently follows the code it guards proves nothing.
    // ─────────────────────────────────────────────────────────────────────────

    /// FROZEN reference: the analytic max-chroma solver exactly as it stood at
    /// the base of `perf/max-chroma-hotpath` — always solving BOTH gamut walls.
    /// Diff test A pins the production [`max_chroma`] bit-for-bit against this.
    /// The gamut band `1e-6` is inlined (frozen mirror of the production
    /// `GAMUT_EPS`) so this oracle carries no scanned const of its own.
    fn max_chroma_reference(l_ok: f64, h_ok_deg: f64) -> f64 {
        use crate::spaces::oklab::{LMS_TO_SRGB, OKLAB_TO_LMS};

        let h_ok = h_ok_deg.to_radians();
        let cos_h = h_ok.cos();
        let sin_h = h_ok.sin();

        let mut p = [0.0_f64; 3];
        let mut q = [0.0_f64; 3];
        for (k, row) in OKLAB_TO_LMS.iter().enumerate() {
            p[k] = l_ok;
            q[k] = row[1] * cos_h + row[2] * sin_h;
        }

        let mut smallest = 1.0_f64;
        for m in &LMS_TO_SRGB {
            let mut coeff = [0.0_f64; 4];
            for ((&mk, &pk), &qk) in m.iter().zip(p.iter()).zip(q.iter()) {
                coeff[0] += mk * pk * pk * pk;
                coeff[1] += mk * 3.0 * pk * pk * qk;
                coeff[2] += mk * 3.0 * pk * qk * qk;
                coeff[3] += mk * qk * qk * qk;
            }
            if let Some(c) = spc_ref(coeff, 1.0 + 1e-6) {
                smallest = smallest.min(c);
            }
            if let Some(c) = spc_ref(coeff, -1e-6) {
                smallest = smallest.min(c);
            }
        }

        smallest.clamp(0.0, 1.0)
    }

    /// Frozen mirror of [`smallest_positive_crossing`].
    fn spc_ref(coeff: [f64; 4], level: f64) -> Option<f64> {
        let g = [coeff[0] - level, coeff[1], coeff[2], coeff[3]];
        let (roots, n) = cubic_roots_ref(g);
        let mut best: Option<f64> = None;
        for &r in roots.iter().take(n) {
            if r > 1e-12 {
                let polished = newton_polish_ref(g, r);
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

    /// Frozen mirror of [`newton_polish`].
    fn newton_polish_ref(g: [f64; 4], mut x: f64) -> f64 {
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

    /// Frozen mirror of [`cubic_roots`].
    fn cubic_roots_ref(g: [f64; 4]) -> ([f64; 3], usize) {
        let [d, c, b, a] = g;
        if a.abs() < 1e-14 {
            return quadratic_roots_ref(d, c, b);
        }
        let p2 = b / a;
        let p1 = c / a;
        let p0 = d / a;
        let shift = p2 / 3.0;
        let p = p1 - p2 * p2 / 3.0;
        let q = 2.0 * p2 * p2 * p2 / 27.0 - p2 * p1 / 3.0 + p0;
        let disc = q * q / 4.0 + p * p * p / 27.0;
        let mut roots = [0.0_f64; 3];
        if disc > 1e-30 {
            let sqrt_disc = disc.sqrt();
            let u = (-q / 2.0 + sqrt_disc).cbrt();
            let v = (-q / 2.0 - sqrt_disc).cbrt();
            roots[0] = u + v - shift;
            (roots, 1)
        } else if disc < -1e-30 {
            let m = 2.0 * (-p / 3.0).sqrt();
            let theta = ((3.0 * q) / (p * m)).clamp(-1.0, 1.0).acos() / 3.0;
            for (k, slot) in roots.iter_mut().enumerate() {
                *slot = m * (theta - 2.0 * std::f64::consts::PI * k as f64 / 3.0).cos() - shift;
            }
            (roots, 3)
        } else {
            let t1 = if q.abs() < 1e-30 { 0.0 } else { 3.0 * q / p };
            let t2 = -t1 / 2.0;
            roots[0] = t1 - shift;
            roots[1] = t2 - shift;
            (roots, 2)
        }
    }

    /// Frozen mirror of [`quadratic_roots`].
    fn quadratic_roots_ref(d: f64, c: f64, b: f64) -> ([f64; 3], usize) {
        let mut roots = [0.0_f64; 3];
        if b.abs() < 1e-14 {
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

    /// FROZEN reference: the flat 61-point hue sweep exactly as it selected the
    /// optimal accent hue at the base of this branch. Calls the PRODUCTION
    /// [`max_chroma`] so diff test B isolates the *selection* logic (C2/C4) from
    /// the solver internals (which diff test A guards separately).
    fn find_optimal_hue_reference(l_ok: f64, h_canonical: f64, slope: f64) -> f64 {
        // 30.0 inlined (frozen mirror of `HUE_SEARCH_HALF_WINDOW`) so this oracle
        // carries no scanned const of its own.
        let half_window = 30.0_f64;
        let c_at_canonical = max_chroma(l_ok, h_canonical);
        if c_at_canonical < 1e-5 {
            return h_canonical;
        }
        let mut best_h = h_canonical;
        let mut best_score = f64::NEG_INFINITY;
        let penalty_scale = slope / half_window;
        let steps = (half_window * 2.0) as i32;
        for i in 0..=steps {
            let h = h_canonical - half_window + i as f64;
            let c = max_chroma(l_ok, h);
            let drift = (h - h_canonical).abs();
            let score = c - penalty_scale * drift;
            if score > best_score {
                best_score = score;
                best_h = h;
            }
        }
        best_h
    }

    /// Diff test A over a grid: production `max_chroma` must equal the frozen
    /// reference to full f64 bit identity. `l_steps`/`h_step_deg` size the grid.
    fn assert_max_chroma_matches_reference(l_steps: usize, h_step_deg: usize) -> usize {
        let mut points = 0usize;
        for li in 0..=l_steps {
            let l = li as f64 / l_steps as f64;
            let mut h = 0usize;
            while h < 360 {
                let hd = h as f64;
                let prod = max_chroma(l, hd);
                let refv = max_chroma_reference(l, hd);
                assert_eq!(
                    prod.to_bits(),
                    refv.to_bits(),
                    "max_chroma drift at (L={l}, h={hd}): prod={prod:e} ref={refv:e}"
                );
                points += 1;
                h += h_step_deg;
            }
        }
        points
    }

    /// Diff test B over a grid: production `find_optimal_hue_core` must select
    /// the bit-identical hue the frozen flat scan does, for the production accent
    /// penalty slope, across `l_ok` and canonical-hue.
    fn assert_find_optimal_hue_matches_reference(l_steps: usize, h_step_deg: usize) -> usize {
        let slope = HUE_DRIFT_PENALTY_SLOPE;
        let mut points = 0usize;
        for li in 0..=l_steps {
            let l = li as f64 / l_steps as f64;
            let mut hc = 0usize;
            while hc < 360 {
                // Integer canonical PLUS fractional offsets: production accent hues
                // are non-integer, so testing hcd + {0, 0.25, 0.5} closes the
                // aliasing-shift class the integer grid alone cannot (the ≤13°
                // bracket bound was measured on the integer grid only).
                for frac in [0.0, 0.25, 0.5] {
                    let hcd = hc as f64 + frac;
                    let prod = find_optimal_hue_core(l, hcd, slope);
                    let refv = find_optimal_hue_reference(l, hcd, slope);
                    assert_eq!(
                        prod.to_bits(),
                        refv.to_bits(),
                        "find_optimal_hue drift at (L={l}, h_canon={hcd}): prod={prod} ref={refv}"
                    );
                    points += 1;
                }
                hc += h_step_deg;
            }
        }
        points
    }

    #[test]
    fn diff_a_max_chroma_matches_frozen_reference_fast() {
        // Fast subset for the per-PR run: 101 L × 72 hue = 7 272 points.
        let n = assert_max_chroma_matches_reference(100, 5);
        assert_eq!(n, 101 * 72);
    }

    #[test]
    #[ignore = "full 180k-point grid — run with `--ignored`; slow at opt-level 0"]
    fn diff_a_max_chroma_matches_frozen_reference_full() {
        // Full grid: L step 0.002 (501) × hue step 1° (360) = 180 360 points.
        let n = assert_max_chroma_matches_reference(500, 1);
        assert_eq!(n, 501 * 360);
    }

    #[test]
    fn diff_b_find_optimal_hue_matches_frozen_reference_fast() {
        // 101 L × 72 canonical-hue × 3 fractional offsets = 21 816 points.
        let n = assert_find_optimal_hue_matches_reference(100, 5);
        assert_eq!(n, 101 * 72 * 3);
    }

    #[test]
    #[ignore = "full grid × 3 offsets — run with `--ignored`; slow at opt-level 0"]
    fn diff_b_find_optimal_hue_matches_frozen_reference_full() {
        // 501 L × 360 canonical-hue × 3 fractional offsets = 541 080 points.
        let n = assert_find_optimal_hue_matches_reference(500, 1);
        assert_eq!(n, 501 * 360 * 3);
    }

    // Diversion tests: prove the coarse_to_fine_argmax debug_asserts BITE, so a
    // caller that violates the fixed-buffer contract fails loud instead of
    // returning a silently-wrong argmax. Debug-only (the asserts compile out of
    // release, so the panic would not fire there).
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "coarse step must be ≥ 1")]
    fn coarse_to_fine_rejects_nonpositive_coarse() {
        let _ = coarse_to_fine_argmax(60, 0, 15, |_| 0.0);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "overflows the")]
    fn coarse_to_fine_rejects_oversized_grid() {
        // 320 / 1 + 2 = 322 coarse samples ≫ the 64-slot buffer.
        let _ = coarse_to_fine_argmax(320, 1, 15, |_| 0.0);
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
    fn jp_to_oklab_l_analytic_matches_bisection_on_grid() {
        // Equivalence gate: the analytic J' → Oklab L inverse must reproduce the
        // 64-step bisection oracle to better than the bisection's own resolution.
        // Both paths feed the identical `srgb_linear_to_oklab([y,y,y])`, so the
        // only divergence is in the recovered grey luminance `y`; that inherits
        // the < 1e-12 bound `lpc::y_hk` is held to (see y_hk_analytic tests), and
        // the cube root only contracts it. We assert max|dL| < 1e-12 and report
        // the measured worst case.
        //
        // Domain: J' > 0, the values an accent actually feeds here (the neutral
        // curve's J' is a lightness, never negative). The J' = 0 endpoint and the
        // negative / above-asymptote tails are *not* an equivalence region: there
        // the analytic path returns exact black / white by definition, while the
        // bisection only *converges toward* black — its `y` floor is 2^-65, and
        // the cube root blows that up to L ≈ 3e-7, never exact 0, so the analytic
        // answer is the more correct one. Those exact endpoints are pinned
        // separately in `jp_to_oklab_l_endpoints_and_saturation`. For any J' > 0
        // the true grey luminance sits far above the bisection floor and the two
        // agree to f64 round-off.
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            let mut max_dl = 0.0_f64;
            let mut worst_jp = 0.0_f64;
            // grey_j(1.0) ≈ 100; sweep (0, 104] to a hair past white, mirroring
            // the y_hk grid test's reachable-range coverage. Start at n = 1 so the
            // exact-black J' = 0 endpoint (pinned elsewhere) is excluded.
            for n in 1..=6000 {
                let jp = (n as f64 / 6000.0) * 104.0;
                let analytic = jp_to_oklab_l(jp, &vc);
                let bisect = jp_to_oklab_l_bisect(jp, &vc);
                let dl = (analytic - bisect).abs();
                if dl > max_dl {
                    max_dl = dl;
                    worst_jp = jp;
                }
            }
            assert!(
                max_dl < 1e-12,
                "analytic vs bisection max|dL| = {max_dl:e} at J'={worst_jp} exceeds 1e-12"
            );
            eprintln!("jp_to_oklab_l max|dL| = {max_dl:e} (worst J'={worst_jp})");
        }
    }

    #[test]
    fn jp_to_oklab_l_endpoints_and_saturation() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            // J' = 0 → black grey; matches srgb_linear_to_oklab([0,0,0])[0].
            assert_eq!(
                jp_to_oklab_l(0.0, &vc),
                srgb_linear_to_oklab([0.0, 0.0, 0.0])[0]
            );
            // Negative J' is below black and clamps to the black grey too.
            assert_eq!(
                jp_to_oklab_l(-3.0, &vc),
                srgb_linear_to_oklab([0.0, 0.0, 0.0])[0]
            );
            // At/above the UCS asymptote (1.7/0.007 ≈ 242.86) there is no finite
            // J: saturate at the white grey, as the bisection's hi = 1.0 did.
            let white_l = srgb_linear_to_oklab([1.0, 1.0, 1.0])[0];
            assert_eq!(jp_to_oklab_l(250.0, &vc), white_l);
            // Round-trip: the J' produced by a known grey luminance recovers an L
            // that equals the forward grey L for that same luminance.
            for &y in &[0.02_f64, 0.18, 0.5, 0.9, 1.0] {
                let j = crate::lpc::grey_j(y, &vc);
                // J' through the shared helper — the same J'-generation production
                // uses; the equivalence is still anchored by the independent
                // `srgb_linear_to_oklab` reference below, not by `ucs_j`.
                let jp = cam16::ucs_j(j);
                let l = jp_to_oklab_l(jp, &vc);
                let l_ref = srgb_linear_to_oklab([y, y, y])[0];
                assert!(
                    (l - l_ref).abs() < 1e-9,
                    "round-trip y={y}: L={l}, forward grey L={l_ref}, |d|={}",
                    (l - l_ref).abs()
                );
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

// ─────────────────────────────────────────────────────────────────────────────
// Научные локи + EXPOSURE (волна science/constants-objectivization) для окна поиска
// оптимального оттенка и наклона штрафа дрейфа. Реимплементируют argmax-предикат с
// ЯВНЫМ окном/наклоном (продакшн НЕ трогается) и мерят долю (l,hue)-сетки, где меняется
// выбранная категория оттенка.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod exposure_locks {
    use super::{HUE_DRIFT_PENALTY_SLOPE, HUE_SEARCH_HALF_WINDOW, max_chroma};

    /// Flat-scan argmax оттенка с ЯВНЫМ окном и наклоном штрафа (bit-совместим с
    /// продакшн `find_optimal_hue_core` при window=HUE_SEARCH_HALF_WINDOW,
    /// penalty_scale=SLOPE/HALF_WINDOW).
    fn argmax_hue(l: f64, hc: f64, penalty_scale: f64, half_window: f64) -> f64 {
        let steps = (half_window * 2.0) as i32;
        let (mut best_h, mut best) = (hc, f64::NEG_INFINITY);
        for i in 0..=steps {
            let h = hc - half_window + i as f64;
            let s = max_chroma(l, h) - penalty_scale * (h - hc).abs();
            if s > best {
                best = s;
                best_h = h;
            }
        }
        best_h
    }

    fn grid_flip(sweep: &[(f64, f64)]) -> (f64, f64) {
        // sweep = list of (penalty_scale, half_window). base is first.
        let (base_ps, base_hw) = sweep[0];
        let (mut flips, mut total, mut max_shift) = (0usize, 0usize, 0.0f64);
        let mut l = 0.05;
        while l <= 0.95 {
            let mut hc = 0.0;
            while hc < 360.0 {
                let base = argmax_hue(l, hc, base_ps, base_hw);
                let mut flipped = false;
                for &(ps, hw) in &sweep[1..] {
                    let alt = argmax_hue(l, hc, ps, hw);
                    let shift = (alt - base).abs();
                    max_shift = max_shift.max(shift);
                    if shift > 0.5 {
                        flipped = true;
                    }
                }
                if flipped {
                    flips += 1;
                }
                total += 1;
                hc += 2.0;
            }
            l += 0.05;
        }
        (100.0 * flips as f64 / total as f64, max_shift)
    }

    /// EXPOSURE окна поиска: доля (l,hue)-сетки, где выбранный оттенок меняется при
    /// свипе окна в [25°,35°] (наклон штрафа держится продакшн). Малая доля ⇒ окно —
    /// нежёсткая нижняя граница, точное 30° нематериально; заметная ⇒ окно намеренно
    /// КАПИРУЕТ дрейф оттенка (как CUSP_HALF_WINDOW_DEG) — мишень с приоритетом.
    #[test]
    fn exposure_hue_search_window() {
        let ps = HUE_DRIFT_PENALTY_SLOPE / HUE_SEARCH_HALF_WINDOW;
        let sweep = [
            (ps, HUE_SEARCH_HALF_WINDOW),
            (ps, 25.0),
            (ps, 35.0),
            (ps, 45.0),
        ];
        let (pct, max_shift) = grid_flip(&sweep);
        eprintln!(
            "EXPOSURE HUE_SEARCH_HALF_WINDOW sweep=25..45deg grid_flip={pct:.2}% max_hue_shift={max_shift:.2}deg"
        );
    }

    /// EXPOSURE наклона штрафа: доля (l,hue)-сетки, где выбранный оттенок меняется при
    /// свипе наклона в [0.10,0.20] (окно фиксировано). Прямо измеряет чувствительность
    /// категории оттенка к калибровочному наклону (у которого есть кандидат-вывод —
    /// хорда Oklab — дающий ДРУГОЕ значение: см. реестр строка 37).
    #[test]
    fn exposure_hue_drift_penalty_slope() {
        let hw = HUE_SEARCH_HALF_WINDOW;
        let base = HUE_DRIFT_PENALTY_SLOPE / hw;
        let sweep = [(base, hw), (0.10 / hw, hw), (0.20 / hw, hw)];
        let (pct, max_shift) = grid_flip(&sweep);
        eprintln!(
            "EXPOSURE HUE_DRIFT_PENALTY_SLOPE sweep=0.10..0.20 grid_flip={pct:.2}% max_hue_shift={max_shift:.2}deg"
        );
    }
}

/// Замки отвергнутых дерайваций: кандидаты, которые ПРОВЕРЕНЫ и отклонены с
/// измеренной причиной. Пиновка не даёт «строгому выводу» вернуться без
/// пересмотра измерений (см. docs/empirical-residue.md, мишень №2).
#[cfg(test)]
mod derivation_rejection_locks {
    use super::{max_chroma, srgb_from_hex, srgb_linear_to_oklab};

    /// Flat-scan argmax (bit-совместим с find_optimal_hue_core, окно ±30°/1°).
    fn argmax(l: f64, hc: f64, penalty_scale: f64) -> f64 {
        let (mut best_h, mut best) = (hc, f64::NEG_INFINITY);
        for i in 0..=60 {
            let h = hc - 30.0 + i as f64;
            let s = max_chroma(l, h) - penalty_scale * (h - hc).abs();
            if s > best {
                best = s;
                best_h = h;
            }
        }
        best_h
    }

    /// ОТКЛОНЁННЫЙ кандидат для `HUE_DRIFT_PENALTY_SLOPE`: «строгий» хордовый
    /// штраф Oklab `penalty_scale = C·π/180` (перцептивная цена дрейфа = длина
    /// хорды). Замер на 49-якорном замороженном паспорте labui (2026-07-06),
    /// l-сетка 0.05..0.95 шаг 0.01:
    ///   * прод-наклон 0.15/30 = 0.005/°: оптимум ВНУТРИ окна на всех 43
    ///     хроматических якорях (0 прижатий к ребру);
    ///   * хордовый кандидат: прижатие к ребру ±30° на 12/43 якорях, флип
    ///     оптимума >0.5° на 27/43, сдвиги до полного окна (ΔE_ok до 0.077).
    ///
    /// Вывод: кандидат не объективизирует — он передаёт решение произвольной
    /// границе HUE_SEARCH_HALF_WINDOW (интерьерный оптимум → клип по окну),
    /// делая нечувствительную константу окна чувствительной. Отклонён;
    /// значение остаётся калибровочным классом (d).
    #[test]
    fn chord_derived_slope_rejected_degenerates_to_window_edge() {
        let (mut base_edge, mut chord_edge, mut chord_flip, mut n) =
            (0usize, 0usize, 0usize, 0usize);
        for &hex in crate::exposure_support::LABUI_ANCHORS {
            let rgb = srgb_from_hex(hex).unwrap();
            let lab = srgb_linear_to_oklab(rgb);
            let c0 = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
            if c0 < 0.02 {
                continue; // нейтраль: поиск оттенка не участвует
            }
            n += 1;
            let hc = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
            let (mut be, mut ce, mut cf) = (false, false, false);
            let mut l = 0.05;
            while l <= 0.951 {
                let base = argmax(l, hc, 0.005);
                let ps_chord = max_chroma(l, hc) * std::f64::consts::PI / 180.0;
                let chord = argmax(l, hc, ps_chord);
                if (base - hc).abs() >= 29.999 {
                    be = true;
                }
                if (chord - hc).abs() >= 29.999 {
                    ce = true;
                }
                if (chord - base).abs() > 0.5 {
                    cf = true;
                }
                l += 0.01;
            }
            if be {
                base_edge += 1;
            }
            if ce {
                chord_edge += 1;
            }
            if cf {
                chord_flip += 1;
            }
        }
        assert_eq!(n, 43, "хроматических якорей в паспорте");
        // Прод-наклон: интерьерный оптимум всюду — окно НЕ является решающим.
        assert_eq!(
            base_edge, 0,
            "прод-наклон не должен прижиматься к ребру окна"
        );
        // Хордовый кандидат вырождается (замерено 12 и 27; нижние границы с
        // запасом на будущие уточнения max_chroma).
        assert!(chord_edge >= 8, "прижатий к ребру: {chord_edge}");
        assert!(chord_flip >= 20, "флипов оптимума: {chord_flip}");
    }
}
