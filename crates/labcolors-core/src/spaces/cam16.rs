//! Core CIECAM16 nonlinear adaptation functions and the shared forward pass.
//!
//! These are the forward and inverse compressive transforms applied
//! to cone responses after chromatic adaptation, plus the single CIECAM16
//! forward pass `XYZ → (J, M, h)` that both [`crate::lcs`] and [`crate::lpc`]
//! build on, and the CAM16-UCS rescaling helpers (`J ↔ J'`, `M ↔ M'`).
//!
//! Source: the CAM16 post-adaptation compression of Li et al. 2017,
//! "Comprehensive color solutions: CAM16, CAT16, and CAM16-UCS",
//! DOI [10.1002/col.22131](https://doi.org/10.1002/col.22131), later
//! formalised as the CIECAM16 model in CIE 248:2022. (Not CIE 170-2:2015,
//! which is the cone-fundamental standard and does not specify CAM16.) The
//! constants are transcribed directly into the source; there is no runtime
//! dependency on a colour-science crate.

use crate::spaces::cat16;
use crate::spaces::srgb::D65_WHITE;
use crate::spaces::vc::ViewingConditions;

// Параметры нелинейного post-adaptation response и ахроматической суммы CAM16
// из Li et al. 2017, DOI 10.1002/col.22131. Это параметры опубликованной модели,
// а не настраиваемые product-пороги. Отдельный divisor сохраняет исходный
// порядок IEEE-операций в forward (`blue / 20`), а массив нужен аналитической
// инверсии, где веса исторически умножаются в цикле. Test-only literal-oracle
// ниже намеренно не использует эти константы: иначе общий дрейф production и
// «эталона» оставил бы bit-identity гейт зелёным.
const RESPONSE_SCALE: f64 = 400.0;
const RESPONSE_OFFSET: f64 = 27.13;
const RESPONSE_EXPONENT: f64 = 0.42;
const ONE: f64 = 1.0;
const ACHROMATIC_RED_WEIGHT: f64 = 2.0;
const ACHROMATIC_BLUE_DIVISOR: f64 = 20.0;
const ACHROMATIC_WEIGHTS: [f64; 3] = [ACHROMATIC_RED_WEIGHT, 1.0, 1.0 / ACHROMATIC_BLUE_DIVISOR];
const OPPONENT_A_M_WEIGHT: f64 = 12.0;
const OPPONENT_A_DIVISOR: f64 = 11.0;
const OPPONENT_B_DIVISOR: f64 = 9.0;
const ECCENTRICITY_SCALE: f64 = 0.25;
const ECCENTRICITY_PHASE_RADIANS: f64 = 2.0;
const ECCENTRICITY_OFFSET: f64 = 3.8;
const T_NUMERATOR_SCALE: f64 = 50_000.0;
const T_NUMERATOR_DIVISOR: f64 = 13.0;
const T_BLUE_WEIGHT: f64 = 1.05;
const T_DENOMINATOR_OFFSET: f64 = 0.305;
const COLORFULNESS_EXPONENT: f64 = 0.9;
const HUNDRED: f64 = 100.0;
const UCS_J_SCALE: f64 = 1.7;
const UCS_J_CURVATURE: f64 = 0.007;
const UCS_M_CURVATURE: f64 = 0.0228;

/// Точные числовые владельцы, используемые artifact-ом contextual-region.
///
/// Test-only view открывает константы, реально используемые production-математикой:
/// он не дублирует их значения и не добавляет данные или ветви в release-сборку.
#[cfg(test)]
pub(crate) fn contextual_region_formula_literals_v1() -> &'static [(&'static str, f64)] {
    &[
        ("one", ONE),
        ("p0_007", UCS_J_CURVATURE),
        ("p0_0228", UCS_M_CURVATURE),
        ("p0_25", ECCENTRICITY_SCALE),
        ("p0_305", T_DENOMINATOR_OFFSET),
        ("p0_42", RESPONSE_EXPONENT),
        ("p0_9", COLORFULNESS_EXPONENT),
        ("p1_05", T_BLUE_WEIGHT),
        ("p1_7", UCS_J_SCALE),
        ("two", ACHROMATIC_RED_WEIGHT),
        ("p3_8", ECCENTRICITY_OFFSET),
        ("nine", OPPONENT_B_DIVISOR),
        ("eleven", OPPONENT_A_DIVISOR),
        ("twelve", OPPONENT_A_M_WEIGHT),
        ("thirteen", T_NUMERATOR_DIVISOR),
        ("twenty", ACHROMATIC_BLUE_DIVISOR),
        ("hundred", HUNDRED),
        ("p27_13", RESPONSE_OFFSET),
        ("four_hundred", RESPONSE_SCALE),
        ("fifty_thousand", T_NUMERATOR_SCALE),
    ]
}

#[cfg(test)]
thread_local! {
    /// Test-only per-thread counter of [`forward`] invocations. Powers the
    /// deterministic `cam16_forwards_per_set_regression_guard` test, which pins
    /// the count of CIECAM16 forward passes a default `resolve_set` runs — the
    /// honest, noise-free "before/after" metric for the discrete-exactness perf
    /// work (wall-time on a loaded box is too variable to measure a few-percent
    /// delta). Thread-local, not a global atomic, so the test runner's parallel
    /// tests cannot pollute the count.
    pub(crate) static FORWARD_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Forward nonlinear adaptation.
///
/// Source: Li et al. 2017, DOI 10.1002/col.22131 (the CAM16 post-adaptation
/// compression), later formalised in CIE 248:2022. (Not CIE 170-2:2015, which
/// is the cone-fundamental standard and does not specify CAM16.)
pub(crate) fn adapt(c: f64, fl: f64) -> f64 {
    let x = fl * c.abs() / HUNDRED;
    let y = x.powf(RESPONSE_EXPONENT);
    // Магнитуда `400·y/(y+27.13)` ≥ 0 (y ≥ 0), а знак берётся у входа `c`. Раньше
    // это делалось умножением на `c.signum()` (∈ {−1, +1} на конечных входах);
    // `copysign` переносит знаковый бит `c` напрямую — байт-идентично для любого
    // конечного `c` (`1.0·X == X`, `−1.0·X` флипает только знаковый бит, ровно как
    // `copysign`; включая ±0.0), но без ветвящегося `signum` и лишнего умножения.
    // Вход `c` (линейная смесь конечных декодированных sRGB) на горячем пути и под
    // гейтом bit-identity (`forward_reference` / reference-векторы) всегда конечен.
    (RESPONSE_SCALE * y / (y + RESPONSE_OFFSET)).copysign(c)
}

/// Inverse nonlinear adaptation.
pub(crate) fn unadapt(a: f64, fl: f64) -> f64 {
    let x = a.abs();
    let y = (RESPONSE_OFFSET * x / (RESPONSE_SCALE - x)).max(0.0);
    // Магнитуда `100·y^(1/0.42)/fl` ≥ 0: `y ≥ 0` (`.max(0.0)`), `y^(1/0.42) ≥ 0`
    // (powf неотрицательной базы), `fl > 0` (VC-параметр по построению). Знак
    // берётся у входа `a` — зеркало прямого `adapt`. Раньше это делалось
    // умножением на `a.signum()` (∈ {−1, +1} на конечных `a`); `copysign` переносит
    // знаковый бит `a` напрямую. Байт-идентично: `1.0·X == X`, а `−1.0·100·P/fl`
    // флипает только знаковый бит (умножение на −1 и IEEE-деление сохраняют
    // магнитуду) — ровно как copysign; ±0.0 совпадают (signum(±0.0)=±1.0,
    // магнитуда 0.0 → copysign(0.0, ±0.0)=±0.0). Свип 0/6.86M расхождений; вход `a`
    // (инверсия конечного J'/M') на этом пути всегда конечен. Убирает ветвящийся
    // `signum` и лишнее умножение из пер-цветового обратного хода (`to_xyz`).
    (HUNDRED * y.powf(ONE / RESPONSE_EXPONENT) / fl).copysign(a)
}

/// CAM16 lightness `J` of a D65-axis stimulus with relative luminance `y`.
#[cfg(test)]
pub(crate) fn gray_j(y: f64, vc: &ViewingConditions) -> f64 {
    let xyz = [y * D65_WHITE[0], y, y * D65_WHITE[2]];
    forward(xyz, vc).0
}

/// Relative luminance on the D65 gray axis whose CAM16 lightness is `j`.
///
/// The gray-axis forward is monotonic. Its inverse uses an analytic seed and
/// two Newton steps over the exact three-channel CAM16 response, then clamps to
/// the physical `[0, 1]` interval. This is appearance-model geometry shared by
/// LCS and LPC; it is not part of either product policy.
pub(crate) fn gray_y(j: f64, vc: &ViewingConditions) -> f64 {
    gray_y_analytic(j, vc)
}

/// Closed-form seed plus Newton polish for [`gray_y`].
///
/// For the D65 gray stimulus, each adapted cone response is linear in `y`:
/// `lms_a[i] = k_i · y`. CAM16 then computes a weighted sum of three nonlinear
/// responses. Collapsing them to one effective scale provides the seed; Newton
/// polish evaluates the actual three terms and their derivative.
pub(crate) fn gray_y_analytic(j: f64, vc: &ViewingConditions) -> f64 {
    if j <= 0.0 {
        return 0.0;
    }

    let rgb_w = cat16::xyz_to_cone([
        D65_WHITE[0] * HUNDRED,
        D65_WHITE[1] * HUNDRED,
        D65_WHITE[2] * HUNDRED,
    ]);
    let k = [
        rgb_w[0] * vc.rgb_d[0],
        rgb_w[1] * vc.rgb_d[1],
        rgb_w[2] * vc.rgb_d[2],
    ];
    let w_sum = ACHROMATIC_WEIGHTS[0] + ACHROMATIC_WEIGHTS[1] + ACHROMATIC_WEIGHTS[2];

    let target = vc.aw * (j / HUNDRED).powf(ONE / (vc.c * vc.z)) / vc.nbb;
    let s = target / w_sum;
    if s <= 0.0 {
        return 0.0;
    }
    if s >= RESPONSE_SCALE {
        return 1.0;
    }

    let k_eff = (ACHROMATIC_WEIGHTS[0] * k[0]
        + ACHROMATIC_WEIGHTS[1] * k[1]
        + ACHROMATIC_WEIGHTS[2] * k[2])
        / w_sum;
    let p = RESPONSE_OFFSET * s / (RESPONSE_SCALE - s);
    let mut y = p.powf(ONE / RESPONSE_EXPONENT) * HUNDRED / (vc.fl * k_eff);

    let residual_slope = |y: f64| -> (f64, f64) {
        let mut f = 0.0;
        let mut df = 0.0;
        for i in 0..3 {
            let c = k[i] * y;
            let x = vc.fl * c / HUNDRED;
            let pp = x.powf(RESPONSE_EXPONENT);
            let denominator = pp + RESPONSE_OFFSET;
            f += ACHROMATIC_WEIGHTS[i] * RESPONSE_SCALE * pp / denominator;
            let dp = RESPONSE_EXPONENT * pp / c;
            df += ACHROMATIC_WEIGHTS[i] * k[i] * RESPONSE_SCALE * RESPONSE_OFFSET * dp
                / (denominator * denominator);
        }
        (f - target, df)
    };
    for _ in 0..2 {
        let (error, slope) = residual_slope(y);
        y -= error / slope;
    }

    y.clamp(0.0, ONE)
}

/// Fixed-iteration oracle for [`gray_y_analytic`].
#[cfg(test)]
pub(crate) fn gray_y_bisect(j: f64, vc: &ViewingConditions) -> f64 {
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        if gray_j(mid, vc) < j {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
}

// Per-`resolve_set` memoization of the `forward` pass, keyed on the input `XYZ`
// bit pattern. Within one `resolve_set` the viewing conditions are fixed, so the
// forward is a pure function of `XYZ` alone — and the curve refine fixed-point
// and the text-hierarchy pass re-measure the same candidate colours, making
// 25–33% of the forwards exact repeats (measured on the default table). The
// cache is live only for the span of a set (see `ForwardCacheGuard`) and cleared
// on entry and exit, so it never aliases across viewing conditions and cannot
// grow unbounded; outside that span (`active == false`) it is transparent. It
// returns the bit-identical tuple the math would have produced — pure
// memoization, no numeric movement.
thread_local! {
    static FORWARD_CACHE: std::cell::RefCell<ForwardCache> =
        std::cell::RefCell::new(ForwardCache {
            active: false,
            map: XyzMap::default(),
        });
}

type XyzMap =
    std::collections::HashMap<[u64; 3], (f64, f64, f64), std::hash::BuildHasherDefault<XyzHasher>>;

struct ForwardCache {
    active: bool,
    map: XyzMap,
}

/// A minimal multiply-xor hasher for the `[u64; 3]` `XYZ`-bits key.
///
/// The default `SipHash` on a 24-byte key costs more than the CIECAM16 forward
/// the cache is meant to save, erasing the win on native. The key is already
/// three near-random `f64` bit patterns, so a single multiply-xor round per word
/// disperses them well enough for a small per-set table (a few hundred entries,
/// no adversarial input — these are colour coordinates, not untrusted data).
#[derive(Default)]
struct XyzHasher(u64);

impl std::hash::Hasher for XyzHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        // Not on the hot path ([u64; 3] hashes via `write_u64`), but required by
        // the trait — fold any stray bytes in so the impl stays total.
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }
    fn write_u64(&mut self, i: u64) {
        self.0 = (self.0 ^ i).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

/// RAII activation of the [`FORWARD_CACHE`] for the lifetime of one `resolve_set`.
///
/// Activating clears any prior contents and enables caching; dropping restores
/// the previous active state and clears the map. Because the cache is keyed on
/// `XYZ` alone, it is correct only while a single viewing condition is in flight
/// — clearing on both edges guarantees that, even under (today never) nesting.
pub(crate) struct ForwardCacheGuard {
    prev_active: bool,
}

impl ForwardCacheGuard {
    /// Activate caching for the enclosing scope; the returned guard deactivates
    /// and clears it on drop.
    pub(crate) fn activate() -> Self {
        let prev_active = FORWARD_CACHE.with(|c| {
            let mut c = c.borrow_mut();
            let prev = c.active;
            c.active = true;
            c.map.clear();
            prev
        });
        Self { prev_active }
    }
}

impl Drop for ForwardCacheGuard {
    fn drop(&mut self) {
        let prev_active = self.prev_active;
        FORWARD_CACHE.with(|c| {
            let mut c = c.borrow_mut();
            c.active = prev_active;
            c.map.clear();
        });
    }
}

/// CIECAM16 correlates `(J, M, h)` for an XYZ stimulus (`Y` normalised to 1).
///
/// `h` is the CAM16 hue angle in **degrees** `[0, 360)`. This is the single
/// CIECAM16 forward pass: [`crate::lcs::LcsColor::from_xyz_with_hok`] applies the
/// CAM16-UCS rescale ([`ucs_j`] / [`ucs_m`]) on top of it, and
/// [`crate::lpc::cam16_jch_from_xyz`] is a thin re-export. Keeping one copy makes
/// a CAM16 matrix or surround change land in exactly one place (issue #19).
///
/// When the [`FORWARD_CACHE`] is active (inside a `resolve_set`) a repeated
/// `XYZ` is served from the table — the same bits, not a re-derivation — so the
/// `FORWARD_CALLS` counter and the per-set forward count reflect *distinct*
/// computations, the honest measure of real CAM16 work.
pub(crate) fn forward(xyz: [f64; 3], vc: &ViewingConditions) -> (f64, f64, f64) {
    let key = [xyz[0].to_bits(), xyz[1].to_bits(), xyz[2].to_bits()];
    if let Some(hit) = FORWARD_CACHE.with(|c| {
        let c = c.borrow();
        if c.active {
            c.map.get(&key).copied()
        } else {
            None
        }
    }) {
        return hit;
    }
    let result = forward_cache_free_v1(xyz, vc);
    FORWARD_CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.active {
            c.map.insert(key, result);
        }
    });
    result
}

/// Execute one CAM16 forward pass without consulting the legacy per-set cache.
///
/// The legacy cache is deliberately keyed only by XYZ because its
/// `resolve_set` owner holds one viewing condition for the entire guard scope.
/// An F0 appearance state instead carries its own immutable context, so it must
/// never inherit that ambient single-context assumption. Both paths still use
/// the same numeric owner below; this boundary changes caching only, not math or
/// operation order.
#[inline]
fn forward_cache_free_v1(xyz: [f64; 3], vc: &ViewingConditions) -> (f64, f64, f64) {
    #[cfg(test)]
    FORWARD_CALLS.with(|c| c.set(c.get() + 1));
    forward_compute(xyz, vc)
}

/// Complete correlates of the registered CAM16 forward view.
///
/// This is an internal numeric carrier, not an editable colour value or a
/// difference metric. Hue absence is classified by the occurrence layer after
/// all coordinates have passed its finite-domain admission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Cam16CorrelatesV1 {
    pub(crate) j: f64,
    pub(crate) q: f64,
    pub(crate) c: f64,
    pub(crate) m: f64,
    pub(crate) s: f64,
    pub(crate) h: f64,
}

/// Derive the full CAM16 correlate set from the same cache-free `J/M/h`
/// operation-order owner used on a cache miss by [`forward`].
///
/// `C`, `Q` and `s` are the published CAM16 correlates. The explicit `J = 0`
/// branch gives mathematical black `(C, M, Q, s) = 0` without evaluating the
/// otherwise indeterminate `C / sqrt(J / 100)` ratio.
pub(crate) fn forward_correlates_v1(xyz: [f64; 3], vc: &ViewingConditions) -> Cam16CorrelatesV1 {
    let (j, m, h) = forward_cache_free_v1(xyz, vc);
    let root_j = (j / 100.0).sqrt();
    let c = m / vc.fl_pow_025;
    let q = (4.0 / vc.c) * root_j * (vc.aw + 4.0) * vc.fl_pow_025;
    let alpha = if root_j == 0.0 { 0.0 } else { c / root_j };
    let s = 50.0 * (vc.c * alpha / (vc.aw + 4.0)).sqrt();
    Cam16CorrelatesV1 { j, q, c, m, s, h }
}

/// The CIECAM16 forward math itself (cache-free); see [`forward`].
fn forward_compute(xyz: [f64; 3], vc: &ViewingConditions) -> (f64, f64, f64) {
    let xyz = [xyz[0] * HUNDRED, xyz[1] * HUNDRED, xyz[2] * HUNDRED];

    let lms = cat16::xyz_to_cone(xyz);
    let lms_a = [
        lms[0] * vc.rgb_d[0],
        lms[1] * vc.rgb_d[1],
        lms[2] * vc.rgb_d[2],
    ];
    let lms_aa = [
        adapt(lms_a[0], vc.fl),
        adapt(lms_a[1], vc.fl),
        adapt(lms_a[2], vc.fl),
    ];

    let a = lms_aa[0] - OPPONENT_A_M_WEIGHT * lms_aa[1] / OPPONENT_A_DIVISOR
        + lms_aa[2] / OPPONENT_A_DIVISOR;
    let b = (lms_aa[0] + lms_aa[1] - ACHROMATIC_RED_WEIGHT * lms_aa[2]) / OPPONENT_B_DIVISOR;
    // `atan2` даёт угол в (−π, π], `to_degrees()` → (−180, 180]. На этом диапазоне
    // `rem_euclid(360)` тождественно сводится к одной условной прибавке 360: для
    // deg < 0 это deg + 360 (floor(deg/360) = −1), иначе deg без изменений. Замена
    // байт-идентична дорогому fmod-пути `rem_euclid` (проверено ULP-свипом по
    // знакам/величинам, включая −0.0 → −0.0 и границу 180.0), но убирает
    // floating-modulo из пер-цветового горячего пути. Значение `h` возвращается и
    // байт-гейтится оракулом `forward_reference` ниже.
    let deg = b.atan2(a).to_degrees();
    let h = if deg < 0.0 { deg + 360.0 } else { deg };
    let hr = h.to_radians();

    let e_hue =
        ECCENTRICITY_SCALE * ((hr + ECCENTRICITY_PHASE_RADIANS).cos() + ECCENTRICITY_OFFSET);
    let a_achrom =
        (ACHROMATIC_RED_WEIGHT * lms_aa[0] + lms_aa[1] + lms_aa[2] / ACHROMATIC_BLUE_DIVISOR)
            * vc.nbb;
    let j = HUNDRED * (a_achrom / vc.aw).powf(vc.c * vc.z);

    let u = (a * a + b * b).sqrt();
    // N_c · N_cb per the CAM16 `t` equation; N_cb = N_bb by construction (vc.rs
    // sets `ncb: nbb`), so `vc.ncb` is byte-identical to the prior `vc.nbb`.
    let t = (T_NUMERATOR_SCALE / T_NUMERATOR_DIVISOR) * e_hue * vc.nc * vc.ncb * u
        / (lms_aa[0] + lms_aa[1] + T_BLUE_WEIGHT * lms_aa[2] + T_DENOMINATOR_OFFSET);
    // `vc.t_inner` == `(1.64 - 0.29^n)^0.73`, `vc.fl_pow_025` == `fl^0.25` — обе
    // вынесены в `ViewingConditions::build` (пер-VC константы, считаются раз на
    // резолв, а не на каждый цвет). Те же операнды, тот же порядок умножения
    // слева-направо → байт-идентично инлайн-форме, которую оракул ниже
    // (`forward_reference`) по-прежнему расписывает явно.
    let m = t.powf(COLORFULNESS_EXPONENT) * (j / HUNDRED).sqrt() * vc.t_inner * vc.fl_pow_025;

    (j, m, h)
}

// ------------------------------------------------------------------
//  CAM16-UCS rescaling — Li et al. 2017, DOI 10.1002/col.22131.
// ------------------------------------------------------------------
//
//   J' = 1.7·J / (1 + 0.007·J),   M' = ln(1 + 0.0228·M) / 0.0228.
//
// These four helpers are the SINGLE SOURCE OF TRUTH for the CAM16-UCS
// coordinate rescale. The forward and inverse formulae are analytically mutual
// inverses; binary64 round-trips are validated within the `1e-12` tolerance in
// `ucs_rescale_round_trips`, not claimed bit-exact. They do not assign universal
// perceptual-attribute meaning to an individual J'/M' value: `lcs` stores the
// coordinates, `lpc` transforms them back to raw J/M, and the
// constants (`1.7`, `0.007`, `0.0228`) must never be re-typed inline anywhere
// else (previously duplicated across `lcs::from_xyz_with_hok`, `lcs::to_xyz`,
// and `lpc::y_hk_from_lcs`).

/// CAM16-UCS lightness rescale `J → J'`.
pub(crate) fn ucs_j(j: f64) -> f64 {
    UCS_J_SCALE * j / (ONE + UCS_J_CURVATURE * j)
}

/// Inverse CAM16-UCS lightness rescale `J' → J`.
pub(crate) fn ucs_j_inv(jp: f64) -> f64 {
    jp / (UCS_J_SCALE - UCS_J_CURVATURE * jp)
}

/// CAM16-UCS colourfulness rescale `M → M'`.
pub(crate) fn ucs_m(m: f64) -> f64 {
    (ONE + UCS_M_CURVATURE * m).ln() / UCS_M_CURVATURE
}

/// Inverse CAM16-UCS colourfulness rescale `M' → M`.
pub(crate) fn ucs_m_inv(mp: f64) -> f64 {
    (UCS_M_CURVATURE * mp).exp_m1() / UCS_M_CURVATURE
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz};

    // Независимые literal-oracles повторяют опубликованные CAM16-параметры, а
    // не production-константы. Это намеренная test-only дубликация: общий SSOT
    // для реализации не должен превращать проверку чисел в самоутверждение.
    fn adapt_reference(c: f64, fl: f64) -> f64 {
        let x = fl * c.abs() / 100.0;
        let y = x.powf(0.42);
        (400.0 * y / (y + 27.13)).copysign(c)
    }

    fn unadapt_reference(a: f64, fl: f64) -> f64 {
        let x = a.abs();
        let y = (27.13 * x / (400.0 - x)).max(0.0);
        (100.0 * y.powf(1.0 / 0.42) / fl).copysign(a)
    }

    fn gray_y_analytic_reference(j: f64, vc: &ViewingConditions) -> f64 {
        if j <= 0.0 {
            return 0.0;
        }

        let rgb_w = cat16::xyz_to_cone([
            D65_WHITE[0] * 100.0,
            D65_WHITE[1] * 100.0,
            D65_WHITE[2] * 100.0,
        ]);
        let k = [
            rgb_w[0] * vc.rgb_d[0],
            rgb_w[1] * vc.rgb_d[1],
            rgb_w[2] * vc.rgb_d[2],
        ];
        let weights = [2.0, 1.0, 1.0 / 20.0];
        let w_sum = weights[0] + weights[1] + weights[2];

        let target = vc.aw * (j / 100.0).powf(1.0 / (vc.c * vc.z)) / vc.nbb;
        let s = target / w_sum;
        if s <= 0.0 {
            return 0.0;
        }
        if s >= 400.0 {
            return 1.0;
        }

        let k_eff = (weights[0] * k[0] + weights[1] * k[1] + weights[2] * k[2]) / w_sum;
        let p = 27.13 * s / (400.0 - s);
        let mut y = p.powf(1.0 / 0.42) * 100.0 / (vc.fl * k_eff);

        let residual_slope = |y: f64| -> (f64, f64) {
            let mut f = 0.0;
            let mut df = 0.0;
            for i in 0..3 {
                let c = k[i] * y;
                let x = vc.fl * c / 100.0;
                let pp = x.powf(0.42);
                let denominator = pp + 27.13;
                f += weights[i] * 400.0 * pp / denominator;
                let dp = 0.42 * pp / c;
                df += weights[i] * k[i] * 400.0 * 27.13 * dp / (denominator * denominator);
            }
            (f - target, df)
        };
        for _ in 0..2 {
            let (error, slope) = residual_slope(y);
            y -= error / slope;
        }

        y.clamp(0.0, 1.0)
    }

    /// Frozen reference: the CIECAM16 forward math exactly as it stood inline in
    /// `lcs::from_xyz_with_hok` / `lpc::cam16_jch_from_xyz` before issue #19
    /// merged the two byte-identical copies into [`forward`]. This is the *old
    /// path*; the test below proves [`forward`] reproduces it bit-for-bit, so the
    /// dedup is a pure refactor with zero numeric movement.
    fn forward_reference(xyz: [f64; 3], vc: &ViewingConditions) -> (f64, f64, f64) {
        let xyz = [xyz[0] * 100.0, xyz[1] * 100.0, xyz[2] * 100.0];

        let lms = cat16::xyz_to_cone(xyz);
        let lms_a = [
            lms[0] * vc.rgb_d[0],
            lms[1] * vc.rgb_d[1],
            lms[2] * vc.rgb_d[2],
        ];
        let lms_aa = [
            adapt_reference(lms_a[0], vc.fl),
            adapt_reference(lms_a[1], vc.fl),
            adapt_reference(lms_a[2], vc.fl),
        ];

        let a = lms_aa[0] - 12.0 * lms_aa[1] / 11.0 + lms_aa[2] / 11.0;
        let b = (lms_aa[0] + lms_aa[1] - 2.0 * lms_aa[2]) / 9.0;
        let h = b.atan2(a).to_degrees().rem_euclid(360.0);
        let hr = h.to_radians();

        let e_hue = 0.25 * ((hr + 2.0).cos() + 3.8);
        let a_achrom = (2.0 * lms_aa[0] + lms_aa[1] + lms_aa[2] / 20.0) * vc.nbb;
        let j = 100.0 * (a_achrom / vc.aw).powf(vc.c * vc.z);

        let u = (a * a + b * b).sqrt();
        let t = (50000.0 / 13.0) * e_hue * vc.nc * vc.nbb * u
            / (lms_aa[0] + lms_aa[1] + 1.05 * lms_aa[2] + 0.305);
        let m = t.powf(0.9)
            * (j / 100.0).sqrt()
            * (1.64 - 0.29_f64.powf(vc.n)).powf(0.73)
            * vc.fl.powf(0.25);

        (j, m, h)
    }

    #[test]
    fn forward_is_bit_identical_to_pre_dedup_path() {
        // BIT-IDENTITY GATE (issue #19): the single shared forward must equal the
        // old inline copy to the last ULP — not "within tolerance". A non-zero
        // delta means the dedup silently moved the math and every downstream
        // golden would have to be re-baselined; this catches it at the source.
        // Grid spans the hue circle plus the achromatic axis and gamut extremes.
        const GRID: [&str; 18] = [
            "#000000", "#FFFFFF", "#7F7F7F", "#787880", "#101012", "#444444", "#FF0000", "#00FF00",
            "#0000FF", "#FFFF00", "#00FFFF", "#FF00FF", "#FF9500", "#34C759", "#007AFF", "#C71585",
            "#008B8B", "#FFD700",
        ];
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for hex in GRID {
                let xyz = srgb_to_xyz(srgb_from_hex(hex).expect("valid hex"));
                let (j, m, h) = forward(xyz, &vc);
                let (rj, rm, rh) = forward_reference(xyz, &vc);
                assert_eq!(j.to_bits(), rj.to_bits(), "{hex}: J drifted {j} vs {rj}");
                assert_eq!(m.to_bits(), rm.to_bits(), "{hex}: M drifted {m} vs {rm}");
                assert_eq!(h.to_bits(), rh.to_bits(), "{hex}: h drifted {h} vs {rh}");
            }
        }
    }

    #[test]
    fn response_helpers_are_bit_identical_to_the_literal_model() {
        let inputs = [
            -100.0_f64,
            -1.0,
            -f64::MIN_POSITIVE,
            -0.0,
            0.0,
            f64::MIN_POSITIVE,
            1.0,
            100.0,
        ];
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for value in inputs {
                assert_eq!(
                    adapt(value, vc.fl).to_bits(),
                    adapt_reference(value, vc.fl).to_bits(),
                    "adapt drifted for {value} at fl={}",
                    vc.fl
                );
                assert_eq!(
                    unadapt(value, vc.fl).to_bits(),
                    unadapt_reference(value, vc.fl).to_bits(),
                    "unadapt drifted for {value} at fl={}",
                    vc.fl
                );
            }
        }
    }

    #[test]
    fn gray_inverse_is_bit_identical_to_the_pre_ssot_path() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for j in [0.0_f64, f64::MIN_POSITIVE, 1.0, 25.0, 50.0, 75.0, 100.0] {
                assert_eq!(
                    gray_y_analytic(j, &vc).to_bits(),
                    gray_y_analytic_reference(j, &vc).to_bits(),
                    "gray inverse drifted for j={j} at fl={}",
                    vc.fl
                );
            }
        }
    }

    #[test]
    fn gray_inverse_uses_all_public_viewing_condition_discount_channels() {
        // `ViewingConditions` is publicly representable field-by-field.  A
        // gray-axis inverse therefore has to invert the exact three discounting
        // factors consumed by `forward`, not reconstruct a shared hidden `d`
        // from only the red channel.
        let mut vc = ViewingConditions::srgb();
        vc.rgb_d[1] *= 1.05;

        let expected_y = srgb_from_hex("#808080").unwrap()[0];
        let j = gray_j(expected_y, &vc);
        let analytic = gray_y_analytic(j, &vc);
        let oracle = gray_y_bisect(j, &vc);

        assert!(
            (analytic - oracle).abs() <= 2.0e-12,
            "analytic gray inverse {analytic:.17} disagrees with the same-VC bisection oracle {oracle:.17}"
        );
        assert!(
            (analytic - expected_y).abs() <= 2.0e-12,
            "same-VC gray round-trip moved Y: expected {expected_y:.17}, got {analytic:.17}"
        );
    }

    #[test]
    fn cache_returns_bit_identical_to_uncached_math() {
        // ISOLATED VERIFICATION of the per-set forward cache: with the cache
        // active, `forward` must return the exact bits `forward_compute` (the
        // cache-free math) produces — including on cache HITS (a repeated XYZ).
        // Independent of the resolve_set golden tests: it drives `forward`
        // directly with deliberate repeats. The guard is scoped per viewing
        // condition, mirroring resolve_set (the XYZ-only key is correct only
        // while one VC is in flight).
        use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz};
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            let _guard = ForwardCacheGuard::activate();
            for code in 0u32..=255 {
                let hex = format!("#{code:02X}{code:02X}{code:02X}");
                let xyz = srgb_to_xyz(srgb_from_hex(&hex).unwrap());
                let want = forward_compute(xyz, &vc);
                // First call misses and inserts; the rest are cache hits.
                for _ in 0..3 {
                    let got = forward(xyz, &vc);
                    assert_eq!(got.0.to_bits(), want.0.to_bits(), "{hex}: J");
                    assert_eq!(got.1.to_bits(), want.1.to_bits(), "{hex}: M");
                    assert_eq!(got.2.to_bits(), want.2.to_bits(), "{hex}: h");
                }
            }
        }
    }

    #[test]
    fn ucs_rescale_round_trips() {
        // The four UCS helpers are exact inverses across the reachable J/M range,
        // so `lcs` (stores J'/M') and `lpc` (decompresses to J/M) never disagree.
        for j in [0.0_f64, 1.0, 12.5, 50.0, 87.6, 100.0, 103.0] {
            let back = ucs_j_inv(ucs_j(j));
            assert!((back - j).abs() < 1e-12, "ucs_j round-trip j={j}: {back}");
        }
        for m in [0.0_f64, 0.5, 5.0, 20.0, 60.0, 120.0] {
            let back = ucs_m_inv(ucs_m(m));
            assert!((back - m).abs() < 1e-12, "ucs_m round-trip m={m}: {back}");
        }
    }
}
