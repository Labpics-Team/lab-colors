//! Candidate components of **Labpics Perceptual Contrast (LPC)**.
//!
//! The current APCA-shaped and H-K paths are characterized implementation
//! components, not the complete LPC definition and not evidence that LPC
//! outperforms APCA. Readability admission is owned by a versioned evaluator
//! profile with declared typography, observer and context applicability.

use crate::spaces::{cam16, vc::ViewingConditions};

#[cfg(test)]
use crate::Srgb8;
#[cfg(test)]
use crate::spaces::srgb::{srgb_from_hex, srgb_linear_from_srgb8, srgb_to_xyz};
#[cfg(test)]
use crate::srgb8::hex_bytes;

/// CIECAM16 correlates `(J, M, h)` for an XYZ stimulus.
///
/// `h` is the CAM16 hue angle in **degrees** `[0, 360)`. Thin re-export of the
/// shared [`cam16::forward`] pass — the single copy both `lcs` and `lpc` build on
/// (issue #19).
pub(crate) fn cam16_jch_from_xyz(xyz: [f64; 3], vc: &ViewingConditions) -> (f64, f64, f64) {
    cam16::forward(xyz, vc)
}

/// Chroma exponent in the Hellwig 2022 H-K lightness term
/// `J_HK = J + f(h) * C^0.587` (source: see [`hk_coeff`]).
const HK_CHROMA_EXPONENT: f64 = 0.587;

/// Hue-dependent Helmholtz-Kohlrausch coefficient `f(h)`, `h_cam_deg` in degrees.
///
/// Source: Hellwig, Stolitzka & Fairchild (2022), "Extending CIECAM02 and
/// CAM16 for the Helmholtz-Kohlrausch effect", Color Research & Application
/// 47(5), DOI 10.1002/col.22793: `J_HK = J + f(h) * C^0.587` where `C` is the
/// CAM16 chroma correlate. Coefficients verified against the colour-science
/// reference implementation (`hue_angle_dependency_Hellwig2022`).
///
/// `pub(crate)` so the external-reference-vector suite (`reference_vectors_deep`)
/// can pin the RHS coefficients directly to the Hellwig 2022 publication; not
/// part of the public API.
pub(crate) fn hk_coeff(h_cam_deg: f64) -> f64 {
    let h_cam = h_cam_deg.to_radians();
    -0.160 * h_cam.cos() + 0.132 * (2.0 * h_cam).cos() - 0.405 * h_cam.sin()
        + 0.080 * (2.0 * h_cam).sin()
        + 0.792
}

/// Gray luminance whose CAM16 lightness equals `j_hk`.
///
/// LPC supplies an H-K-adjusted `J` target; the physical inversion itself is
/// shared appearance geometry in [`cam16::gray_y`].
pub(crate) fn y_hk(j_hk: f64, vc: &ViewingConditions) -> f64 {
    cam16::gray_y(j_hk, vc)
}

/// Benchmark wrapper for the production CAM16 gray-axis inverse.
#[cfg(test)]
fn y_hk_analytic(j_hk: f64, vc: &ViewingConditions) -> f64 {
    cam16::gray_y_analytic(j_hk, vc)
}

/// Benchmark wrapper for the fixed-iteration CAM16 oracle.
#[cfg(test)]
fn y_hk_bisect(j_hk: f64, vc: &ViewingConditions) -> f64 {
    cam16::gray_y_bisect(j_hk, vc)
}

// Константы candidate-кривой транскрибированы из опубликованного набора
// SAPC-8 0.0.98G-4g. Имена в комментариях воспроизводят исходные
// идентификаторы, чтобы маппинг был аудируемым. Эта транскрипция сама по себе не
// является APCA conformance, complete LPC или evidence читаемости; комментарий
// также не делает правового вывода о допустимости дальнейшего распространения.
//
// Это ЕДИНСТВЕННЫЙ ИСТОЧНИК ИСТИНЫ для кривой контраста: и прямой `contrast_core`,
// и обратный решатель (`crate::solve`) читают значения здесь.
// Не переобъявляйте эти литералы нигде больше.

/// Порог мягкого зажима чёрного (`blkThrs`): яркость ниже этого значения поднимается.
// GROUNDED — APCA SAPC-8 `0.0.98G-4g` published set (docs/empirical-inventory.md).
pub(crate) const SOFT_CLAMP_THRESHOLD: f64 = 0.022;
/// Показатель степени мягкого зажима чёрного (`blkClmp`).
// GROUNDED — APCA SAPC-8 `0.0.98G-4g` published set (docs/empirical-inventory.md).
pub(crate) const SOFT_CLAMP_EXP: f64 = 1.414;
/// Показатель степени фона, нормальная полярность (`normBG`, bg > fg).
// GROUNDED — APCA SAPC-8 `0.0.98G-4g` published set (docs/empirical-inventory.md).
pub(crate) const EXP_BG_LIGHT: f64 = 0.56;
/// Показатель степени переднего плана, нормальная полярность (`normTXT`).
// GROUNDED — APCA SAPC-8 `0.0.98G-4g` published set (docs/empirical-inventory.md).
pub(crate) const EXP_FG_LIGHT: f64 = 0.57;
/// Background power-curve exponent, reverse polarity (`revBG`).
// GROUNDED — APCA SAPC-8 `0.0.98G-4g` published set (docs/empirical-inventory.md).
pub(crate) const EXP_BG_DARK: f64 = 0.65;
/// Foreground power-curve exponent, reverse polarity (`revTXT`).
// GROUNDED — APCA SAPC-8 `0.0.98G-4g` published set (docs/empirical-inventory.md).
pub(crate) const EXP_FG_DARK: f64 = 0.62;
/// Raw power-curve delta scale, shared by both polarities (`scaleBoW` == `scaleWoB`).
// GROUNDED — APCA SAPC-8 `0.0.98G-4g` published set (docs/empirical-inventory.md).
pub(crate) const CONTRAST_SCALE: f64 = 1.14;
/// Minimum luminance delta below which the pair reports no contrast (`deltaYmin`).
pub(crate) const DELTA_Y_MIN: f64 = 0.0005;
/// Low-contrast clip: scaled deltas inside ±`loClip` collapse to zero.
// GROUNDED — APCA SAPC-8 `0.0.98G-4g` published set (docs/empirical-inventory.md).
pub(crate) const LO_CLIP: f64 = 0.1;
/// Polarity offset pulled toward zero past the clip, normal polarity (`loBoWoffset`).
// GROUNDED — APCA SAPC-8 `0.0.98G-4g` published set (docs/empirical-inventory.md).
pub(crate) const LO_BOW_OFFSET: f64 = 0.027;
/// Polarity offset pulled toward zero past the clip, reverse polarity (`loWoBoffset`).
// GROUNDED — APCA SAPC-8 `0.0.98G-4g` published set (docs/empirical-inventory.md).
pub(crate) const LO_WOB_OFFSET: f64 = 0.027;
/// Maps the offset contrast to the ~[-108, 108] Lc output range.
pub(crate) const LC_SCALE: f64 = 100.0;

/// DERIVED — минимальный ненулевой |Lc|, который вообще может эмитить модель.
///
/// Прямо за клипом [`LO_CLIP`] выход стартует не с нуля, а со скачка: внутри
/// клипа `contrast_core` схлопывает контраст в `0`, а первый ненулевой отсчёт
/// равен `(LO_CLIP − LO_BOW_OFFSET) × LC_SCALE`. Обе полярности симметричны
/// ([`LO_WOB_OFFSET`] == [`LO_BOW_OFFSET`], пиннится
/// `polarity_offsets_are_symmetric`), поэтому модельный пол одинаков для
/// нормальной и обратной сторон — иначе он был бы минимумом по полярностям
/// `(LO_CLIP − max(LO_BOW_OFFSET, LO_WOB_OFFSET)) × LC_SCALE`.
///
/// Численно `(0.1 − 0.027) × 100 = 7.3`. Это НЕ независимая политика, а
/// алгебраическая идентичность из GROUNDED APCA `0.0.98G-4g` набора (те же
/// [`LO_CLIP`], [`LO_BOW_OFFSET`], [`LC_SCALE`]). Значение — литерал `7.3` (чистое
/// число для инвентаря, класс (a) DERIVED), но тождество с формулой обязано
/// держаться байт-равно: пиннится `model_lc_floor_is_the_published_clip_minimum`
/// (дрейф любого из трёх APCA-входов ломает лок). Прямое следствие issue #44:
/// решатель, целясь в декоративный контраст ниже 7.3, упирается в порог клипа и
/// возвращает ноль, поэтому `DECORATIVE_FLOOR_MIN` держится строго выше него.
/// Скан-инвариант — `no_pair_emits_contrast_below_model_floor`.
// Rust 1.85 не считает использование только в const-assert/test runtime-use;
// константа намеренно остаётся исполняемым provenance lock, не shipping knob.
#[allow(dead_code)]
// SSOT-TRACKED — (a) DERIVED = (LO_CLIP − LO_BOW_OFFSET) × LC_SCALE (issue #44), см. docs/empirical-inventory.md.
pub(crate) const MODEL_LC_FLOOR: f64 = 7.3;

/// Soft black clamp: lifts luminance below [`SOFT_CLAMP_THRESHOLD`] so the
/// contrast curve stays monotonic near black. Strictly increasing on `[0, T]`
/// and the identity above `T`, hence invertible — see [`soft_clamp_inv`].
pub(crate) fn soft_clamp(y: f64) -> f64 {
    if y < SOFT_CLAMP_THRESHOLD {
        y + (SOFT_CLAMP_THRESHOLD - y).powf(SOFT_CLAMP_EXP)
    } else {
        y
    }
}

/// Inverse of [`soft_clamp`]: recover the raw luminance from a clamped value.
///
/// Returns `None` when `clamped` is below `soft_clamp(0.0)` — reproducing it
/// would require a luminance darker than pure black, so the contrast that
/// implied it is physically unreachable.
pub(crate) fn soft_clamp_inv(clamped: f64) -> Option<f64> {
    if clamped >= SOFT_CLAMP_THRESHOLD {
        return Some(clamped);
    }
    if clamped < soft_clamp(0.0) {
        return None;
    }
    // On `[0, T)` the clamp is `y + (T − y)^E`, smooth and strictly increasing
    // with derivative `1 − E·(T − y)^(E−1)` bounded in ~`[0.71, 1]`, so a
    // bracket-safeguarded Newton converges to full `f64` precision in a handful
    // of steps instead of 64 bisections. The bracket `[lo, hi]` guards every
    // step: a Newton iterate that leaves it falls back to bisection, so this
    // converges to the *same* root the old fixed bisection found — the emitted
    // hex is bit-identical (locked by the golden grid; checked directly by
    // `soft_clamp_inv_matches_reference_bisection`).
    let t = SOFT_CLAMP_THRESHOLD;
    let mut lo = 0.0_f64;
    let mut hi = t;
    // Seed from above: the clamp only adds `(T − y)^E ≥ 0`, so the root sits at
    // or below `clamped`, inside the bracket.
    let mut y = clamped;
    for _ in 0..12 {
        let f = soft_clamp(y) - clamped;
        if f > 0.0 {
            hi = y;
        } else {
            lo = y;
        }
        if hi - lo <= f64::EPSILON * t {
            break;
        }
        // f'(y) = 1 − E·(T − y)^(E−1); bounded away from zero on the bracket.
        let deriv = 1.0 - SOFT_CLAMP_EXP * (t - y).powf(SOFT_CLAMP_EXP - 1.0);
        let next = y - f / deriv;
        // Safeguard: keep the iterate strictly inside the bracket, else bisect.
        y = if next > lo && next < hi {
            next
        } else {
            0.5 * (lo + hi)
        };
    }
    Some(y)
}

/// Candidate asymmetric power curve over a luminance-shaped scalar.
///
/// The branches and constants mirror the frozen SAPC-8 0.0.98G-4g candidate:
/// soft black clamp, polarity-dependent exponents, a minimum-luminance gate,
/// low-contrast clipping and polarity offsets. Current call sites feed more
/// than one luminance definition, including an H-K/CAM16-derived scalar. That
/// composition is characterized implementation behavior, not APCA conformance,
/// complete LPC or evidence of glyph readability.
///
/// `golden_tests::contrast_core_matches_reference_on_grey_axis` pins only the
/// scalar curve arithmetic. The curve is inverted by `crate::solve` to recover
/// a foreground scalar from a target.
pub(crate) fn contrast_core(y_fg: f64, y_bg: f64) -> f64 {
    let fg = soft_clamp(y_fg);
    let bg = soft_clamp(y_bg);

    if (bg - fg).abs() < DELTA_Y_MIN {
        return 0.0;
    }

    if bg > fg {
        // Dark-on-light (normal polarity).
        let sapc = (bg.powf(EXP_BG_LIGHT) - fg.powf(EXP_FG_LIGHT)) * CONTRAST_SCALE;
        if sapc < LO_CLIP {
            0.0
        } else {
            (sapc - LO_BOW_OFFSET) * LC_SCALE
        }
    } else {
        // Light-on-dark (reverse polarity).
        let sapc = (bg.powf(EXP_BG_DARK) - fg.powf(EXP_FG_DARK)) * CONTRAST_SCALE;
        if sapc > -LO_CLIP {
            0.0
        } else {
            (sapc + LO_WOB_OFFSET) * LC_SCALE
        }
    }
}

/// Hellwig 2022 H-K-corrected lightness for an XYZ stimulus:
/// `J_HK = J + f(h) * C^0.587`, with the chroma correlate `C = M / F_L^0.25`.
pub(crate) fn j_hk_from_xyz(xyz: [f64; 3], vc: &ViewingConditions) -> f64 {
    let (j, m, h) = cam16_jch_from_xyz(xyz, vc);
    j_hk_from_cam16(j, m, h, vc)
}

/// Hellwig 2022 H-K-corrected lightness from already-computed CIECAM16
/// correlates `(J, M, h)`. Splitting this out of [`j_hk_from_xyz`] lets a caller
/// that already ran [`cam16::forward`] (e.g. [`crate::solve`]'s `finish`, which
/// also needs the `LcsColor`) derive `J_HK` from the same forward pass instead
/// of running a second identical one on the same stimulus.
pub(crate) fn j_hk_from_cam16(j: f64, m: f64, h: f64, vc: &ViewingConditions) -> f64 {
    // `vc.fl_pow_025` == инлайновый `vc.fl.powf(0.25)` (пер-VC константа,
    // вынесенная в `ViewingConditions::build`), так что `chroma` байт-идентична.
    let chroma = m / vc.fl_pow_025;
    j + hk_coeff(h) * chroma.powf(HK_CHROMA_EXPONENT)
}

#[cfg(test)]
fn srgb8_to_y_hk(rgb: Srgb8, vc: &ViewingConditions) -> f64 {
    let rgb = srgb_linear_from_srgb8(rgb);
    let xyz = srgb_to_xyz(rgb);
    y_hk(j_hk_from_xyz(xyz, vc).max(0.0), vc)
}

/// Test-only characterization of the retired scalar apparent-contrast
/// candidate. Exact bytes make malformed transport unrepresentable; the
/// function is deliberately absent from the production/public LPC surface.
#[cfg(test)]
pub(crate) fn apparent_contrast_candidate_srgb8(fg: Srgb8, bg: Srgb8) -> f64 {
    apparent_contrast_candidate_srgb8_with_vc(fg, bg, &ViewingConditions::srgb())
}

/// Viewing-condition-specific form of
/// [`apparent_contrast_candidate_srgb8`]. This is characterization machinery,
/// not an admitted readability evaluator or a complete LPC definition.
#[cfg(test)]
pub(crate) fn apparent_contrast_candidate_srgb8_with_vc(
    fg: Srgb8,
    bg: Srgb8,
    vc: &ViewingConditions,
) -> f64 {
    let y_fg = srgb8_to_y_hk(fg, vc);
    let y_bg = srgb8_to_y_hk(bg, vc);
    contrast_core(y_fg, y_bg)
}

#[cfg(test)]
pub(crate) fn apparent_contrast_candidate_hex_for_test(
    fg_hex: &str,
    bg_hex: &str,
) -> Result<f64, String> {
    Ok(apparent_contrast_candidate_srgb8(
        Srgb8::new(hex_bytes(fg_hex)?),
        Srgb8::new(hex_bytes(bg_hex)?),
    ))
}

#[cfg(test)]
pub(crate) fn apparent_contrast_candidate_hex_with_vc_for_test(
    fg_hex: &str,
    bg_hex: &str,
    vc: &ViewingConditions,
) -> Result<f64, String> {
    Ok(apparent_contrast_candidate_srgb8_with_vc(
        Srgb8::new(hex_bytes(fg_hex)?),
        Srgb8::new(hex_bytes(bg_hex)?),
        vc,
    ))
}

/// Test-reference score of a foreground over a background computed in the
/// display-luminance domain `Ys` (WCAG relative luminance) rather than
/// the Helmholtz–Kohlrausch brightness domain `Y_hk`. С главы #64 (активация
/// ADR-0003) это домен, в котором считается Ys candidate score движка.
///
/// # Why a second domain exists
///
/// [`contrast_core`] carries the published SAPC-8 (`0.0.98G-4g`) constants, and
/// those constants were **calibrated with screen luminance `Ys` as their input**.
/// The retired scalar candidate instead fed the curve `Y_hk` — a *brightness*
/// estimate that lifts a saturated hue's effective luminance above its photometric value (`#007AFF`:
/// `Y 0.211 → Y_hk 0.346`). Applied to the frozen curve this changes its
/// polarity branch because it substitutes an out-of-domain input. The CH-4
/// characterization observed the resulting branch flips; it did not establish
/// a legibility preference. Restoring the formula's declared domain removes them.
/// Full rationale and blast radius:
/// `docs/decisions/0003-hk-scope.md` (variant A — accepted 2026-07-06).
///
/// # Argument domain
///
/// `fg`/`bg` are **display** (gamma-encoded) sRGB triples in `[0, 1]` — the same
/// domain [`crate::wcag::relative_luminance`] is defined on and the legal WCAG
/// floor is measured in. This low-level reference entry point assumes every
/// channel is finite and in `[0, 1]`; it does not parse or validate public text
/// input. `Ys` is display-referred, so this
/// contrast is viewing-condition invariant by construction: the dark-theme
/// surround compensation `Y_hk` carries is a brightness concern, kept off the
/// Ys candidate-score path.
///
/// ADR-0003: Ys candidate score движка
/// считает именно в этом домене — `solve::finish`/`meets_floor`, интервал фона,
/// recheck-примитивы (`semantic::measure_contrast`, `recheck_against*`) и
/// pointwise hard evaluators. Сам движок зовёт [`contrast_core`] +
/// [`crate::wcag::relative_luminance`] напрямую на уже
/// готовых скалярах; эта функция — только test-reference той же формулы (те же
/// функции, ноль новых констант). `Y_hk` остаётся отдельной appearance-
/// координатой, но не публичным scalar-LPC API.
#[cfg(test)]
pub(crate) fn ys_candidate_score_for_test(fg_display: [f64; 3], bg_display: [f64; 3]) -> f64 {
    contrast_core(
        crate::wcag::relative_luminance(fg_display),
        crate::wcag::relative_luminance(bg_display),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lpc(fg_hex: &str, bg_hex: &str) -> f64 {
        apparent_contrast_candidate_hex_for_test(fg_hex, bg_hex)
            .expect("test fixture must contain valid sRGB8 hex")
    }

    fn lpc_with_vc(fg_hex: &str, bg_hex: &str, vc: &ViewingConditions) -> f64 {
        apparent_contrast_candidate_hex_with_vc_for_test(fg_hex, bg_hex, vc)
            .expect("test fixture must contain valid sRGB8 hex")
    }

    #[test]
    fn black_on_white_matches_reference() {
        // Black and white are the exact luminance endpoints (Y_hk = 0 and 1),
        // so the H-K layer cannot shift them: LPC reproduces the canonical
        // achromatic reference (106.0407) bit-for-bit after the offset
        // alignment. Формула: APCA SAPC-8 версии 0.0.98G-4g.
        let lc = lpc("#000000", "#ffffff");
        assert!((lc - 106.04).abs() < 0.5, "LPC for black on white: {}", lc);
    }

    #[test]
    fn gray_on_white_mid_range() {
        let lc = lpc("#888888", "#ffffff");
        assert!((lc - 58.4).abs() < 1.0, "LPC for gray on white: {}", lc);
    }

    #[test]
    fn blue_on_white_below_achromatic() {
        // The H-K term lifts a saturated blue's perceived lightness, so its
        // contrast on white lands below a same-luminance grey would (≈ 68.7).
        let lc = lpc("#0000ff", "#ffffff");
        assert!(lc < 75.0, "LPC for blue on white should be < 75: {}", lc);
        assert!(lc > 60.0, "LPC for blue on white should be > 60: {}", lc);
    }

    #[test]
    fn polarity_swap_negates() {
        // The polarity offsets are symmetric (both pull toward zero), so the
        // residual asymmetry comes only from the exponent split.
        let lc1 = lpc("#000000", "#ffffff");
        let lc2 = lpc("#ffffff", "#000000");
        assert!((lc1 + lc2).abs() < 3.0, "polarity swap: {} vs {}", lc1, lc2);
    }

    #[test]
    fn neutral_hk_boost_is_small() {
        // A near-neutral grey carries a tiny residual CAM16 colourfulness
        // (incomplete chromatic adaptation), so the H-K term shifts Y_hk
        // only slightly: LPC ≈ 87.6, within ~1.5 Lc of the canonical
        // achromatic number for this luminance.
        let lc = lpc("#444444", "#ffffff");
        assert!((lc - 87.6).abs() < 1.0, "achromatic LPC: {}", lc);
    }

    #[test]
    fn readability_domain_keeps_white_on_saturated_backgrounds() {
        // CH-4 variant A (docs/decisions/0003-hk-scope.md). The H-K domain that
        // `lpc` reads monotonically sinks the white label on saturated
        // backgrounds — the V3 étude measured 15:0 white→black flips — because
        // `Y_hk` lifts the background's effective luminance out of the SAPC-8
        // calibration domain. The calibrated `Ys` domain does not. On each cell
        // below, the white label must WIN in the Ys domain (larger |Lc|), and
        // the current H-K domain must prefer black — documenting the flip.
        let enc = |hex: &str| crate::spaces::srgb::srgb_encoded_from_hex(hex).expect("valid hex");
        let white = enc("#FFFFFF");
        let black = enc("#000000");
        for bg_hex in ["#007AFF", "#0082FF", "#FF0000", "#00B087"] {
            let bg = enc(bg_hex);
            // Readability (Ys) domain: white wins — matches platform convention.
            let white_ys = ys_candidate_score_for_test(white, bg).abs();
            let black_ys = ys_candidate_score_for_test(black, bg).abs();
            assert!(
                white_ys > black_ys,
                "{bg_hex}: Ys domain must prefer white (|white|={white_ys} !> |black|={black_ys})"
            );
            // Current H-K domain (`lpc`): black wins — the flip variant A removes.
            let white_hk = lpc("#FFFFFF", bg_hex).abs();
            let black_hk = lpc("#000000", bg_hex).abs();
            assert!(
                black_hk > white_hk,
                "{bg_hex}: H-K domain flips to black (|black|={black_hk} !> |white|={white_hk})"
            );
        }
    }

    #[test]
    fn readability_domain_matches_hk_at_luminance_endpoints() {
        // Black and white are the luminance endpoints (Ys = 0/1 == Y_hk = 0/1),
        // so the two domains must agree there bit-for-bit: variant A moves only
        // the chromatic interior, never the achromatic endpoints. The canonical
        // black-on-white number (≈106.04) is preserved, so the WCAG legal floor
        // and the endpoint anchors the golden grid locks are untouched.
        let enc = |hex: &str| crate::spaces::srgb::srgb_encoded_from_hex(hex).expect("valid hex");
        let bw_ys = ys_candidate_score_for_test(enc("#000000"), enc("#FFFFFF"));
        let bw_hk = lpc("#000000", "#FFFFFF");
        assert!(
            (bw_ys - bw_hk).abs() < 1e-9,
            "endpoints must agree across domains: Ys={bw_ys} Y_hk={bw_hk}"
        );
        assert!(
            (bw_ys - 106.04).abs() < 0.5,
            "canonical black-on-white preserved: {bw_ys}"
        );
    }

    #[test]
    fn hk_domain_suppresses_white_contrast_on_saturated_bg() {
        // Mechanism the 15:0 flips ride on: the H-K lift eats the white label's
        // contrast on a saturated background (V3: on #007AFF the white label
        // drops from ~69.7 Lc to ~54.2 Lc as `Y_hk` climbs 0.211→0.346). The
        // calibrated Ys domain restores that suppressed contrast, so white-on-blue
        // reads as materially higher contrast there than in the H-K domain.
        let enc = |hex: &str| crate::spaces::srgb::srgb_encoded_from_hex(hex).expect("valid hex");
        let bg = "#007AFF";
        let white_ys = ys_candidate_score_for_test(enc("#FFFFFF"), enc(bg)).abs();
        let white_hk = lpc("#FFFFFF", bg).abs();
        assert!(
            white_ys > white_hk + 5.0,
            "Ys should restore the contrast H-K suppressed: Ys={white_ys} Y_hk={white_hk}"
        );
    }

    #[test]
    fn j_hk_matches_hellwig_reference() {
        // BUG CLASS: "self-consistent but wrong" — the J_HK pipeline (CAM16 J +
        // Hellwig-2022 H-K term) could agree with itself and with the inverse
        // solver yet drift from the published CIECAM16/Hellwig math, and every
        // internal round-trip test would still pass. This pins J_HK to an
        // EXTERNAL reference at 12 points spanning the hue circle.
        //
        // Reference computed with colour-science 0.4.7 (NOT hand-written):
        //   XYZ = sRGB(IEC 61966-2-1) → CIECAM16 XYZ_to_CIECAM16
        //         (XYZ_w = D65·100, L_A = 64, Y_b = 20, surround = Average),
        //   chroma C = M / F_L^0.25 with F_L the CIECAM16 luminance-adaptation
        //   factor for L_A = 64, and the hue coefficient
        //   f(h) = −0.160cos h + 0.132cos 2h − 0.405sin h + 0.080sin 2h + 0.792
        //   evaluated at the CAM16 hue, then J_HK = J + f(h)·C^0.587.
        // Reproduce with `scripts/jhk_golden_ref.py` (colour-science 0.4.7); its
        // output matches these pins and the three original anchors
        // (blue/red/gold) within 0.006.
        //
        // The grid deliberately covers the green / cyan / magenta / orange
        // sectors the original three-point test never touched — the zones where
        // a wrong f(h) or a wrong chroma exponent would diverge most. The
        // measured worst-case crate-vs-reference delta across all twelve is
        // 0.0043 Lc; the 0.05 budget is the documented sRGB-matrix / FL
        // micro-delta band (|dJ|<0.005, |dC|<0.05), >10× the observed drift, so
        // a real formula regression breaks it while round-off does not.
        let vc = ViewingConditions::srgb();
        for (hex, want) in [
            // existing anchors (unchanged): blue, red, gold
            ("#0000FF", 38.949467),
            ("#FF0000", 56.023889),
            ("#FFD700", 85.095269),
            // green sector
            ("#00FF00", 88.930558),
            ("#34C759", 68.618093),
            // cyan sector
            ("#00FFFF", 98.343680),
            ("#008B8B", 51.238150),
            // magenta sector
            ("#FF00FF", 68.208430),
            ("#C71585", 48.391467),
            // orange sector
            ("#FF9500", 68.405244),
            ("#FF7F00", 64.718227),
            // azure (info brand)
            ("#007AFF", 56.061369),
        ] {
            let rgb = srgb_from_hex(hex).expect("reference hex is valid");
            let got = j_hk_from_xyz(srgb_to_xyz(rgb), &vc);
            assert!(
                (got - want).abs() < 0.05,
                "{hex}: J_HK={got}, colour-science reference={want}, delta={}",
                (got - want).abs()
            );
        }
    }

    #[test]
    fn shortcuts_match_srgb_with_vc() {
        // The test-only sRGB characterization shortcut must be bit-identical
        // to the same typed bytes evaluated with an explicit standard VC.
        let srgb = ViewingConditions::srgb();
        assert_eq!(
            lpc("#0000FF", "#FFFFFF"),
            lpc_with_vc("#0000FF", "#FFFFFF", &srgb)
        );
    }

    #[test]
    fn dim_diverges_from_srgb() {
        // (a) The same chromatic pair resolved under dim surround must land on
        // a different Lc than under average surround: dark themes compute in
        // their own perceptual space (Bartleson–Breneman). A saturated green
        // carries the largest H-K term — the only VC-sensitive part when the
        // background is a luminance endpoint — so the gap clears the 1-Lc
        // contract tolerance, which is precisely why light colours cannot be
        // reused verbatim in a dark theme (issue #15).
        let srgb = ViewingConditions::srgb();
        let dim = ViewingConditions::dim_surround();
        let lc_srgb = lpc_with_vc("#00FF00", "#FFFFFF", &srgb);
        let lc_dim = lpc_with_vc("#00FF00", "#FFFFFF", &dim);
        assert!(
            (lc_srgb - lc_dim).abs() > 1.0,
            "dim VC should shift Lc meaningfully: srgb={lc_srgb} dim={lc_dim}"
        );
    }

    #[test]
    fn monotonic_in_fg_luminance_under_both_vcs() {
        // (b) On a fixed light background, darker foreground text yields higher
        // (more positive) contrast; this ordering must hold in every
        // perceptual space. Greys keep the H-K hue term out of the comparison.
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            let bg = "#FFFFFF";
            let dark = lpc_with_vc("#000000", bg, &vc);
            let mid = lpc_with_vc("#888888", bg, &vc);
            let light = lpc_with_vc("#CCCCCC", bg, &vc);
            assert!(
                dark > mid && mid > light,
                "monotonicity broken: dark={dark} mid={mid} light={light}"
            );
        }
    }

    #[test]
    fn polarity_swap_negates_under_both_vcs() {
        // (c) Swapping foreground and background flips the sign of the contrast
        // (near-symmetric magnitude) under both viewing conditions.
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            let lc1 = lpc_with_vc("#000000", "#FFFFFF", &vc);
            let lc2 = lpc_with_vc("#FFFFFF", "#000000", &vc);
            assert!(lc1 > 0.0 && lc2 < 0.0, "polarity signs: {lc1} vs {lc2}");
            assert!(
                (lc1 + lc2).abs() < 3.0,
                "polarity swap should near-negate: {lc1} vs {lc2}"
            );
        }
    }

    #[test]
    fn y_hk_analytic_matches_bisection_on_grid() {
        // Equivalence gate: the analytic inverse must reproduce the bisection
        // reference to better than the bisection's own resolution. Bisection
        // on [0,1] over 64 steps resolves Y to ~2^-65 ≈ 2.7e-20; the analytic
        // path is limited instead by f64 round-off in the Newton residual, so
        // we hold it to 1e-12 in Y — six orders below any perceptual or
        // contrast-curve significance, and the measured worst case is < 1e-11.
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            let mut max_dy = 0.0_f64;
            // Sweep J_HK across the full reachable range, including the
            // above-100 band (near-white chromatic colours, where the H-K term
            // lifts J past gray_j(1.0) = 100 and both paths must saturate Y=1).
            for n in 0..=4000 {
                let j_hk = n as f64 / 4000.0 * 104.0;
                let analytic = y_hk_analytic(j_hk, &vc);
                let bisect = y_hk_bisect(j_hk, &vc);
                max_dy = max_dy.max((analytic - bisect).abs());
            }
            assert!(
                max_dy < 1e-12,
                "analytic vs bisection max|dY| = {max_dy:e} exceeds 1e-12"
            );
        }
    }

    #[test]
    fn y_hk_analytic_endpoints_and_saturation() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            // J_HK = 0 → black (Y = 0).
            assert_eq!(y_hk_analytic(0.0, &vc), 0.0);
            // J_HK = gray_j(1.0) = 100 → white (Y = 1), within round-off.
            assert!((y_hk_analytic(cam16::gray_j(1.0, &vc), &vc) - 1.0).abs() < 1e-9);
            // J_HK above 100 (reachable for near-white chromatic colours) must
            // clamp to Y = 1, matching the bisection's [0,1] search interval.
            assert_eq!(y_hk_analytic(130.0, &vc), 1.0);
            // Round-trip: gray_j(y) → y_hk_analytic recovers y.
            for &y in &[0.01_f64, 0.18, 0.5, 0.9] {
                let recovered = y_hk_analytic(cam16::gray_j(y, &vc), &vc);
                assert!(
                    (recovered - y).abs() < 1e-12,
                    "round-trip y={y}: recovered {recovered}, |d|={}",
                    (recovered - y).abs()
                );
            }
        }
    }

    #[test]
    fn soft_clamp_inv_is_a_left_inverse_of_soft_clamp() {
        // BUG CLASS: silent inverse drift. `soft_clamp_inv` is the analytic
        // back-door the contrast solver uses to turn a clamped foreground
        // luminance back into a raw Y_hk (solve.rs `invert_contrast`). If the
        // bisection inside it ever loses agreement with the forward `soft_clamp`
        // — a changed threshold, exponent, or iteration count — every solve in
        // the near-black band silently lands on the wrong colour, yet no forward
        // test would notice because the forward curve alone stays consistent.
        // This pins the round-trip soft_clamp_inv(soft_clamp(y)) == y across the
        // entire clamped band [0, threshold], where the lift is active.
        //
        // Tolerance: the inverse is a 64-step bisection on [0, SOFT_CLAMP_THRESHOLD];
        // its residual is bounded by the interval width 2^-64 · 0.022 ≈ 1.2e-21,
        // but f64 round-off in `soft_clamp`'s powf dominates, so 1e-9 is a safe
        // honest bound (the measured worst case over the sweep is < 1e-10).
        let step = 1e-4;
        let mut y = 0.0_f64;
        let mut max_err = 0.0_f64;
        let mut samples = 0_usize;
        while y <= 0.05 + 1e-12 {
            let clamped = soft_clamp(y);
            let recovered = soft_clamp_inv(clamped)
                .expect("soft_clamp(y) for y>=0 is always >= soft_clamp(0), so invertible");
            let err = (recovered - y).abs();
            max_err = max_err.max(err);
            samples += 1;
            assert!(
                err < 1e-9,
                "round-trip y={y}: soft_clamp={clamped}, recovered={recovered}, err={err:e}"
            );
            y += step;
        }
        // The sweep must actually cross the threshold so both the lifted branch
        // (y < T) and the identity branch (y >= T) are exercised, not just one.
        assert!(
            samples >= 500,
            "sweep too coarse to be a property test: {samples} samples"
        );
        eprintln!("soft_clamp_inv round-trip: {samples} samples, max err = {max_err:e}");
    }

    #[test]
    fn soft_clamp_inv_matches_reference_bisection() {
        // BIT-IDENTITY GATE for the bisection→safeguarded-Newton swap. The new
        // inverse must converge to the *same* root the original fixed 64-step
        // bisection did, or a near-black solve could land on a different hex.
        // Reproduce the exact old algorithm here and assert agreement to ULP
        // scale across the whole clamped band; far below one 8-bit output step
        // (~3.9e-3), so the emitted hex is provably unchanged.
        fn reference_bisect(clamped: f64) -> f64 {
            let mut lo = 0.0_f64;
            let mut hi = SOFT_CLAMP_THRESHOLD;
            for _ in 0..64 {
                let mid = (lo + hi) * 0.5;
                if soft_clamp(mid) < clamped {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            (lo + hi) * 0.5
        }

        let floor = soft_clamp(0.0);
        let mut max_err = 0.0_f64;
        let mut samples = 0_usize;
        // Sweep the clamped-value domain [soft_clamp(0), T) densely.
        let span = SOFT_CLAMP_THRESHOLD - floor;
        for i in 0..=4000 {
            let clamped = floor + span * (i as f64 / 4000.0);
            // Stay strictly inside the lifted branch (>= T returns identity).
            if clamped >= SOFT_CLAMP_THRESHOLD {
                continue;
            }
            let newton = soft_clamp_inv(clamped).expect("clamped >= soft_clamp(0) is invertible");
            let bisect = reference_bisect(clamped);
            let err = (newton - bisect).abs();
            max_err = max_err.max(err);
            samples += 1;
            // Both methods are limited by `powf` round-off in `soft_clamp`
            // (measured worst case ~1.3e-12), so 1e-9 is the honest bound — the
            // same margin the round-trip sibling uses. Nine orders below one
            // 8-bit output step (~3.9e-3): the hex is provably unchanged.
            assert!(
                err < 1e-9,
                "clamped={clamped}: newton={newton}, bisect={bisect}, err={err:e}"
            );
        }
        assert!(samples >= 3000, "sweep too coarse: {samples} samples");
        eprintln!(
            "soft_clamp_inv vs reference bisection: {samples} samples, max err = {max_err:e}"
        );
    }

    #[test]
    fn soft_clamp_boundaries_are_exact() {
        // BUG CLASS: off-by-epsilon at the clamp seam. The boundaries are where
        // a regression hides: at the threshold the two branches must meet
        // continuously, and soft_clamp(0) is the hard floor below which the
        // inverse must refuse (a contrast implying a luminance darker than black
        // is physically unreachable — solve.rs leans on this returning None).

        // soft_clamp(0): black is lifted to exactly threshold^exp above zero.
        let at_zero = soft_clamp(0.0);
        let expected_zero = SOFT_CLAMP_THRESHOLD.powf(SOFT_CLAMP_EXP);
        assert!(
            (at_zero - expected_zero).abs() < 1e-15,
            "soft_clamp(0)={at_zero}, expected threshold^exp={expected_zero}"
        );
        assert!(
            at_zero > 0.0,
            "soft_clamp(0) must lift above zero: {at_zero}"
        );

        // Continuity at the threshold: the lifted branch meets the identity
        // branch (the (T - y)^exp term vanishes as y → T from below).
        let just_below = soft_clamp(SOFT_CLAMP_THRESHOLD - 1e-12);
        assert!(
            (just_below - SOFT_CLAMP_THRESHOLD).abs() < 1e-6,
            "discontinuity at threshold: soft_clamp(T-)={just_below} vs T={SOFT_CLAMP_THRESHOLD}"
        );
        // At and above the threshold soft_clamp is the identity.
        assert_eq!(soft_clamp(SOFT_CLAMP_THRESHOLD), SOFT_CLAMP_THRESHOLD);
        assert_eq!(soft_clamp(0.5), 0.5);

        // The inverse refuses anything below soft_clamp(0): unreachable, not a clip.
        assert_eq!(soft_clamp_inv(at_zero - 1e-9), None);
        // Exactly at soft_clamp(0) the inverse recovers black.
        let recovered_zero = soft_clamp_inv(at_zero).expect("soft_clamp(0) is invertible");
        assert!(
            recovered_zero.abs() < 1e-9,
            "soft_clamp_inv(soft_clamp(0)) should recover 0, got {recovered_zero}"
        );
    }

    #[test]
    fn model_lc_floor_is_the_published_clip_minimum() {
        // DERIVED IDENTITY (issue #44). The model's smallest non-zero output is
        // (LO_CLIP − LO_BOW_OFFSET) × LC_SCALE: inside the low-contrast clip the
        // curve collapses to 0, and the first sample past it is this value. The
        // const is stored as the literal 7.3 (a clean inventory number), but this
        // pins that literal to the DERIVATION — a drift in any of the three GROUNDED
        // APCA `0.0.98G-4g` inputs, or a hand-edit of the literal, breaks the lock.
        let derived = (LO_CLIP - LO_BOW_OFFSET) * LC_SCALE;
        assert!(
            (MODEL_LC_FLOOR - derived).abs() < 1e-12,
            "MODEL_LC_FLOOR literal {MODEL_LC_FLOOR} must equal the derived clip minimum \
             (LO_CLIP − LO_BOW_OFFSET) × LC_SCALE = {derived}"
        );
        assert!(
            (MODEL_LC_FLOOR - 7.3).abs() < 1e-12,
            "the derived clip minimum must be 7.3, got {MODEL_LC_FLOOR}"
        );
    }

    #[test]
    fn polarity_offsets_are_symmetric() {
        // The single-polarity MODEL_LC_FLOOR formula holds only because both
        // polarity offsets are equal; were they to diverge the floor would become
        // the minimum over polarities `(LO_CLIP − max(offset)) × LC_SCALE`. Pin the
        // equality the derivation rests on.
        assert_eq!(
            LO_WOB_OFFSET, LO_BOW_OFFSET,
            "polarity offsets must be equal for a single-polarity MODEL_LC_FLOOR"
        );
    }

    #[test]
    fn no_pair_emits_contrast_below_model_floor() {
        // MODEL INVARIANT + guard measurement (issue #44). Across quantised 8-bit
        // sRGB pairs the contrast curve emits either exactly 0 or a magnitude
        // ≥ MODEL_LC_FLOOR — never a value strictly inside the band (0, 7.3). This
        // is the empirical face of the algebraic identity above, and it measures
        // the two numbers QUANT_GUARD is sized against: the actual minimum non-zero
        // |Lc| the solver can emit, and the largest single-8-bit-step jump in |Lc|
        // across the clip boundary.
        let eps = 1e-9;
        let mut min_nonzero = f64::INFINITY;
        let mut max_clip_step = 0.0_f64;

        // Grey axis: adjacent-fg jumps across the clip + min non-zero.
        let grey = |i: u8| format!("#{i:02X}{i:02X}{i:02X}");
        for bg_i in 0u16..=255 {
            let bg = grey(bg_i as u8);
            let mut prev = lpc(&grey(0), &bg).abs();
            for fg_i in 1u16..=255 {
                let cur = lpc(&grey(fg_i as u8), &bg).abs();
                if cur > eps {
                    min_nonzero = min_nonzero.min(cur);
                }
                // A jump ACROSS the clip: exactly one side collapsed to 0.
                if (prev <= eps) != (cur <= eps) {
                    max_clip_step = max_clip_step.max((cur - prev).abs());
                }
                assert!(
                    cur <= eps || cur >= MODEL_LC_FLOOR - eps,
                    "grey fg={} bg={bg}: |Lc|={cur} lands in the forbidden band (0, {MODEL_LC_FLOOR})",
                    grey(fg_i as u8)
                );
                prev = cur;
            }
        }

        // Chromatic sample: the 32³ representable cube against a few backgrounds,
        // so the invariant is not a grey-axis artefact.
        for bg in ["#FFFFFF", "#000000", "#808080"] {
            crate::exposure_support::rgb_cube(|c| {
                let fg = format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2]);
                let v = lpc(&fg, bg).abs();
                if v > eps {
                    min_nonzero = min_nonzero.min(v);
                }
                assert!(
                    v <= eps || v >= MODEL_LC_FLOOR - eps,
                    "cube fg={fg} bg={bg}: |Lc|={v} in forbidden band (0, {MODEL_LC_FLOOR})"
                );
            });
        }

        assert!(
            min_nonzero >= MODEL_LC_FLOOR - eps,
            "measured minimum non-zero |Lc| {min_nonzero} is below MODEL_LC_FLOOR {MODEL_LC_FLOOR}"
        );
        eprintln!(
            "MODEL_LC_FLOOR scan: min non-zero |Lc| = {min_nonzero:.6}, \
             max single-8-bit-step jump across clip = {max_clip_step:.6} (QUANT_GUARD = 0.2)"
        );
    }
}
