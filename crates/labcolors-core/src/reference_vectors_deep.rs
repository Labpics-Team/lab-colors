//! External published reference vectors for the deepest colour-science layers.
//!
//! These pin the crate's transforms to CONTROL POINTS AND VECTORS PUBLISHED IN
//! STANDARDS / PEER-REVIEWED SOURCES, not to the crate's own output. They live
//! in-crate (not `tests/`) because the transforms they touch are `pub(crate)`
//! and invisible to an integration test. Public-API-reachable vectors are in
//! `tests/reference_vectors.rs`; the full map is `docs/verification-map.md`.
//!
//! Sources cited per test:
//! * IEC 61966-2-1:1999 — sRGB EOTF/OETF and primaries; also W3C CSS Color 4.
//! * Björn Ottosson, "A perceptual color space for image processing" (2020),
//!   <https://bottosson.github.io/posts/oklab/> — Oklab matrices + XYZ→Oklab table.
//! * Li, Li, Wang, Xu, Luo, Cui, Melgosa, Brill, Pointer (2017),
//!   DOI 10.1002/col.22131 — CAT16, CIECAM16 post-adaptation, CAM16-UCS.
//! * Hellwig, Stolitzka & Fairchild (2022), DOI 10.1002/col.22793 — H-K f(h).
//! * W3C WCAG 2.1 §1.4.3 — relative luminance linearisation threshold.

use crate::spaces::cam16::{adapt, ucs_j, ucs_j_inv, ucs_m, ucs_m_inv, unadapt};
use crate::spaces::cat16::{cone_to_xyz, xyz_to_cone};
use crate::spaces::oklab::srgb_linear_to_oklab;
use crate::spaces::srgb::{D65_WHITE, srgb_gamma, srgb_gamma_inv, srgb_to_xyz, xyz_to_srgb};
use crate::wcag::relative_luminance;

// ─────────────────────────────────────────────────────────────────────────────
// sRGB EOTF / OETF — IEC 61966-2-1:1999 §6.4 (also W3C CSS Color 4).
// ─────────────────────────────────────────────────────────────────────────────

/// The IEC transfer function's published breakpoints, slope and exponent
/// reproduce the standard's control values.
///
/// The decode branch boundary is `0.04045` with slope `1/12.92`; the encode
/// branch boundary is `0.0031308` with slope `12.92`; the curved branch is
/// `((c+0.055)/1.055)^2.4`. We assert the crate's `srgb_gamma_inv` reproduces
/// the linear-branch value AT the boundary (`0.04045/12.92`) and the endpoints,
/// plus the W3C CSS Color 4 worked value `decode(0.5) = 0.2140411...` — a
/// published control point independent of this crate.
#[test]
fn srgb_transfer_iec_control_points() {
    // Endpoints are fixed points of the EOTF (0→0, 1→1).
    assert_eq!(srgb_gamma_inv(0.0), 0.0, "decode(0) must be 0");
    assert!(
        (srgb_gamma_inv(1.0) - 1.0).abs() < 1e-12,
        "decode(1) must be 1, got {}",
        srgb_gamma_inv(1.0)
    );
    // At the breakpoint the linear branch is taken: 0.04045 / 12.92.
    let at_break = srgb_gamma_inv(0.040_45);
    assert!(
        (at_break - 0.040_45 / 12.92).abs() < 1e-15,
        "decode(0.04045) must be 0.04045/12.92 (linear branch), got {at_break}"
    );
    // W3C CSS Color 4 published sample: the linear light of encoded 0.5.
    // Tolerance 1e-6: the spec value is printed to 6 significant figures.
    assert!(
        (srgb_gamma_inv(0.5) - 0.214_041_140_5).abs() < 1e-6,
        "decode(0.5) must be the CSS Color 4 sample 0.2140411, got {}",
        srgb_gamma_inv(0.5)
    );
    // Encode side endpoints and CSS Color 4 sample inverse.
    assert_eq!(srgb_gamma(0.0), 0.0, "encode(0) must be 0");
    assert!(
        (srgb_gamma(1.0) - 1.0).abs() < 1e-12,
        "encode(1) must be 1, got {}",
        srgb_gamma(1.0)
    );
    assert!(
        (srgb_gamma(0.214_041_140_5) - 0.5).abs() < 1e-6,
        "encode(0.2140411) must be 0.5, got {}",
        srgb_gamma(0.214_041_140_5)
    );
}

/// The IEC piecewise EOTF/OETF is continuous across its breakpoint — to the tiny
/// residual the standard's ROUNDED constants leave.
///
/// IEC 61966-2-1 rounds the slope to `12.92` and the breakpoint to `0.04045`
/// (the exactly-continuous pair would be `12.9232…` / `0.03928…`). That rounding
/// leaves a documented micro-discontinuity of ≈2.3e-9 (decode) / ≈2.9e-8
/// (encode) at the join — far below one 8-bit quantum (3.9e-3). We pin it stays
/// within `1e-7`: a real slope/breakpoint regression (e.g. dropping the offset)
/// blows this by orders of magnitude.
#[test]
fn srgb_transfer_join_is_continuous() {
    const EPS: f64 = 1e-9;
    // Decode join at 0.04045.
    let d_lo = srgb_gamma_inv(0.040_45 - EPS); // linear branch
    let d_hi = srgb_gamma_inv(0.040_45 + EPS); // curved branch
    assert!(
        (d_lo - d_hi).abs() < 1e-7,
        "decode discontinuity at 0.04045: {d_lo} vs {d_hi}"
    );
    // Encode join at 0.0031308.
    let e_lo = srgb_gamma(0.003_130_8 - EPS); // linear branch
    let e_hi = srgb_gamma(0.003_130_8 + EPS); // curved branch
    assert!(
        (e_lo - e_hi).abs() < 1e-7,
        "encode discontinuity at 0.0031308: {e_lo} vs {e_hi}"
    );
    // EOTF∘OETF is the identity on a mid sample (both branches exercised).
    for x in [0.001_f64, 0.0031308, 0.05, 0.25, 0.5, 0.9, 1.0] {
        assert!(
            (srgb_gamma(srgb_gamma_inv(x)) - x).abs() < 1e-9,
            "round-trip broke at {x}"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// sRGB ↔ XYZ(D65) matrices — W3C CSS Color 4 / IEC 61966-2-1.
// ─────────────────────────────────────────────────────────────────────────────

/// The forward/inverse sRGB↔XYZ matrices are mutual inverses, so a colour
/// survives a round-trip through XYZ. (The matrices are the published CSS Color
/// 4 constants; a transposed row or a wrong digit breaks the inverse identity.)
#[test]
fn srgb_xyz_matrices_are_mutual_inverses() {
    for v in [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.2, 0.5, 0.8],
    ] {
        let back = xyz_to_srgb(srgb_to_xyz(v));
        for i in 0..3 {
            // Double-precision 3×3 round-trip: machine-epsilon accumulation only.
            assert!(
                (back[i] - v[i]).abs() < 1e-12,
                "srgb↔xyz round-trip drift ch{i}: {} vs {}",
                back[i],
                v[i]
            );
        }
    }
}

/// Linear sRGB white `[1,1,1]` maps to the D65 white point — the defining
/// property of the sRGB→XYZ matrix under IEC 61966-2-1.
#[test]
fn srgb_white_maps_to_d65() {
    let w = srgb_to_xyz([1.0, 1.0, 1.0]);
    for i in 0..3 {
        assert!(
            (w[i] - D65_WHITE[i]).abs() < 1e-9,
            "sRGB white component {i}: {} vs D65 {}",
            w[i],
            D65_WHITE[i]
        );
    }
}

/// `D65_WHITE` is the chromaticity-derived white of IEC 61966-2-1 / CSS Color 4:
/// from `(x, y) = (0.3127, 0.3290)`, `X = x/y`, `Y = 1`, `Z = (1−x−y)/y`.
#[test]
fn d65_white_derives_from_chromaticity() {
    let (x, y) = (0.3127, 0.3290);
    let expected = [x / y, 1.0, (1.0 - x - y) / y];
    for i in 0..3 {
        assert!(
            (D65_WHITE[i] - expected[i]).abs() < 1e-6,
            "D65_WHITE[{i}] = {} vs chromaticity-derived {}",
            D65_WHITE[i],
            expected[i]
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Oklab — Björn Ottosson (2020), XYZ→Oklab reference table.
// ─────────────────────────────────────────────────────────────────────────────

/// The crate's Oklab forward reproduces Ottosson's published XYZ→Oklab table
/// (post's "test values", rounded to three decimals).
///
/// The crate stores Ottosson's fused *linear-sRGB*→LMS matrix, so we reach the
/// canonical XYZ path by `xyz_to_srgb` (CSS Color 4 matrix) → `srgb_linear_to_oklab`
/// — mathematically `M1·XYZ` up to the tiny difference between the CSS Color 4
/// sRGB matrix and the one Ottosson fused. The four rows include the imaginary
/// XYZ primaries (out of the sRGB gamut, exercising the matrices on negatives).
///
/// Tolerance 1.5e-3: Ottosson prints three decimals (±5e-4 inherent), and the
/// CSS-vs-Ottosson matrix delta adds ≤4.4e-4 (measured); a real matrix
/// regression moves values by whole tenths.
#[test]
fn oklab_matches_ottosson_xyz_table() {
    // (X, Y, Z) → (L, a, b), Ottosson 2020 test table (Y normalised to 1).
    const TABLE: [([f64; 3], [f64; 3]); 4] = [
        ([0.950, 1.000, 1.089], [1.000, 0.000, 0.000]), // D65 white → L=1
        ([1.000, 0.000, 0.000], [0.450, 1.236, -0.019]),
        ([0.000, 1.000, 0.000], [0.922, -0.671, 0.263]),
        ([0.000, 0.000, 1.000], [0.153, -1.415, -0.449]),
    ];
    const TOL: f64 = 1.5e-3;
    for (xyz, want) in TABLE {
        let got = srgb_linear_to_oklab(xyz_to_srgb(xyz));
        for i in 0..3 {
            assert!(
                (got[i] - want[i]).abs() < TOL,
                "Ottosson XYZ {xyz:?} Lab[{i}]: got {}, published {}, delta {}",
                got[i],
                want[i],
                (got[i] - want[i]).abs()
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CAT16 — Li et al. 2017, DOI 10.1002/col.22131.
// ─────────────────────────────────────────────────────────────────────────────

/// The crate ships Li et al. 2017's *printed* forward and inverse CAT16 matrices
/// (not a runtime re-inversion); their product is the identity to the residual
/// the printed 8-decimal values carry (≈5.4e-9, absorbed by 8-bit output).
#[test]
fn cat16_printed_inverse_residual() {
    for v in [
        [95.05, 100.0, 108.88], // ~D65 white in XYZ·100
        [19.01, 20.0, 21.78],   // a mid sample
        [50.0, 0.0, 0.0],
    ] {
        let back = cone_to_xyz(xyz_to_cone(v));
        for i in 0..3 {
            // Residual of the printed pair is ≈5.4e-9 relative; on values ~100
            // that is ~5e-7 absolute. 1e-5 keeps a decade of honest margin and
            // is still 100× below one 8-bit XYZ·100 quantum.
            assert!(
                (back[i] - v[i]).abs() < 1e-5,
                "CAT16 printed-inverse residual ch{i}: {} vs {}",
                back[i],
                v[i]
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CIECAM16 post-adaptation compression — Li et al. 2017 / CIE 248:2022.
// ─────────────────────────────────────────────────────────────────────────────

/// `adapt` is the published post-adaptation compression
/// `400·(F_L·c/100)^0.42 / ((F_L·c/100)^0.42 + 27.13)`; `unadapt` inverts it.
/// This pins the published constants (400, 0.42, 27.13) into the crate function
/// via an independently-written copy of the closed form, and the round-trip.
#[test]
fn cam16_adapt_matches_published_closed_form() {
    // Independent transcription of the CIECAM16 post-adaptation nonlinearity.
    fn published_adapt(c: f64, fl: f64) -> f64 {
        let x = (fl * c.abs() / 100.0).powf(0.42);
        c.signum() * 400.0 * x / (x + 27.13)
    }
    let fl = 0.6839903845696502; // F_L for L_A = 64 (this crate's viewing cond.)
    for c in [0.5_f64, 5.0, 12.0, 40.0, 95.0, 250.0] {
        let got = adapt(c, fl);
        let want = published_adapt(c, fl);
        assert!(
            (got - want).abs() < 1e-12,
            "adapt({c}, {fl}) = {got}, published closed form {want}"
        );
        // Sign symmetry (the compression is odd in the cone response).
        assert!(
            (adapt(-c, fl) + got).abs() < 1e-12,
            "adapt must be odd in c"
        );
        // unadapt inverts adapt across the reachable range.
        let round = unadapt(got, fl);
        assert!(
            (round - c).abs() < 1e-9,
            "unadapt(adapt({c})) = {round}, expected {c}"
        );
    }
}

/// CAM16-UCS rescale uses the published constants (`1.7`, `0.007`, `0.0228`) and
/// is exactly invertible (Li et al. 2017 §CAM16-UCS).
#[test]
fn cam16_ucs_constants() {
    // Independent published closed forms.
    let want_jp = |j: f64| 1.7 * j / (1.0 + 0.007 * j);
    let want_mp = |m: f64| (1.0 + 0.0228 * m).ln() / 0.0228;
    for j in [1.0_f64, 12.5, 43.3, 100.0] {
        assert!(
            (ucs_j(j) - want_jp(j)).abs() < 1e-12,
            "ucs_j({j}) constant drift"
        );
        assert!(
            (ucs_j_inv(ucs_j(j)) - j).abs() < 1e-10,
            "ucs_j not invertible at {j}"
        );
    }
    for m in [0.5_f64, 6.0, 58.6, 103.0] {
        assert!(
            (ucs_m(m) - want_mp(m)).abs() < 1e-12,
            "ucs_m({m}) constant drift"
        );
        assert!(
            (ucs_m_inv(ucs_m(m)) - m).abs() < 1e-10,
            "ucs_m not invertible at {m}"
        );
    }
    // J'=50 reads as half-lightness only if the 1.7/0.007 pair is intact:
    // published sanity value ucs_j(43.30..) ≈ 55.6 is a monotone lift, not 1:1.
    assert!(ucs_j(50.0) > 50.0, "UCS lightness lift must raise J");
}

// ─────────────────────────────────────────────────────────────────────────────
// Helmholtz–Kohlrausch f(h) — Hellwig, Stolitzka & Fairchild (2022).
// ─────────────────────────────────────────────────────────────────────────────

/// The crate's `hk_coeff` is Hellwig-2022's hue-dependency
/// `f(h) = −0.160 cos h + 0.132 cos 2h − 0.405 sin h + 0.080 sin 2h + 0.792`
/// — NOT Nayatani/VAC, NOT Fairchild 1998. Coefficients confirmed verbatim
/// against the paper (DOI 10.1002/col.22793) and colour-science's
/// `hue_angle_dependency_Hellwig2022`.
#[test]
fn hk_coeff_matches_hellwig2022_published() {
    // Independent transcription of the published trigonometric polynomial.
    fn published_fh(h_deg: f64) -> f64 {
        let h = h_deg.to_radians();
        -0.160 * h.cos() + 0.132 * (2.0 * h).cos() - 0.405 * h.sin()
            + 0.080 * (2.0 * h).sin()
            + 0.792
    }
    // Spanning the hue circle plus the CAM16 primary hue angles.
    for h in [
        0.0_f64, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0, 27.4, 141.8, 282.9,
    ] {
        assert!(
            (crate::lpc::hk_coeff(h) - published_fh(h)).abs() < 1e-12,
            "hk_coeff({h}) drifted from Hellwig-2022 f(h)"
        );
    }
    // Published landmark magnitudes (evaluated from the paper's coefficients):
    // f(0°)=0.792−0.160+0.132=0.764; f(180°)=0.792+0.160+0.132=1.084.
    assert!((crate::lpc::hk_coeff(0.0) - 0.764).abs() < 1e-9);
    assert!((crate::lpc::hk_coeff(180.0) - 1.084).abs() < 1e-9);
}

// ─────────────────────────────────────────────────────────────────────────────
// Порог относительной яркости WCAG — действующий нормативный текст W3C.
// ─────────────────────────────────────────────────────────────────────────────

/// Erratum W3C от 2022-02-22 (PR #1780, включён в Recommendation мая 2025)
/// заменил исходный порог `0.03928` значением IEC `0.04045`. Проверка нужна не
/// только для 8-битных кодов, где ветви случайно совпадают, но и для непрерывного
/// API. Порог наблюдается через одноканальный цвет:
/// `relative_luminance([c,0,0]) = 0.2126·linearise(c)`.
#[test]
fn wcag_linearise_threshold_is_current_04045() {
    const KR: f64 = 0.2126; // коэффициент яркости красного канала
    // В самом действующем пороге WCAG/IEC выбирается линейная ветвь.
    let at = relative_luminance([0.040_45, 0.0, 0.0]) / KR;
    assert!(
        (at - 0.040_45 / 12.92).abs() < 1e-12,
        "linearise(0.04045) обязан выбрать 0.04045/12.92, получено {at}"
    );
    // Выше порога должна включиться степенная ветвь, а не линейная экстраполяция.
    let above = relative_luminance([0.05, 0.0, 0.0]) / KR;
    let curved = ((0.05 + 0.055) / 1.055_f64).powf(2.4);
    assert!(
        (above - curved).abs() < 1e-12,
        "linearise(0.05) обязан выбрать степенную ветвь: {above} против {curved}"
    );
    // Точка между старым и новым порогами отличает действующее правило от старого.
    let interior = 0.04;
    let got = relative_luminance([interior, 0.0, 0.0]) / KR;
    assert!(
        (got - interior / 12.92).abs() < 1e-12,
        "linearise(0.04) обязан использовать действующую линейную ветвь"
    );
    // Концы шкалы яркости обязаны остаться точными.
    assert!((relative_luminance([1.0, 1.0, 1.0]) - 1.0).abs() < 1e-9);
    assert_eq!(relative_luminance([0.0, 0.0, 0.0]), 0.0);
}
