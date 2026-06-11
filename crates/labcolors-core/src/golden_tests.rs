//! Golden cross-validation of the colour pipeline against external
//! reference implementations:
//!
//! * the CIECAM16 forward path against colour-science (Python), and
//! * the perceptual-contrast curve against the published reference math
//!   on the achromatic axis (see [`contrast_core_matches_reference_on_grey_axis`]).
//!
//! Reference values generated 2026-06-10 with colour-science (Python):
//! `colour.appearance.XYZ_to_CIECAM16` at D65, `L_A = 64`, `Y_b = 20`,
//! surround Average resp. Dim, `discount_illuminant = False`; inputs via
//! `colour.sRGB_to_XYZ`, white point `xy = (0.3127, 0.3290)`.
//!
//! Tolerances cover the residual differences between the CSS Color 4
//! sRGB↔XYZ constants used here and colour-science's own derivation
//! (observed worst case across the table: |ΔJ| 0.004, |ΔM| 0.037,
//! |Δh| 0.08° — the hue tail only on near-achromatic colours, where the
//! hue angle is atan2 of model noise). A genuine regression (wrong
//! surround, units mix-up, matrix typo) blows these bounds by orders of
//! magnitude.

use crate::lpc::{cam16_jch_from_xyz, contrast_core};
use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;

const TOL_J: f64 = 0.01;
const TOL_M: f64 = 0.05;
const TOL_H_DEG: f64 = 0.15;

/// (hex, J, M, h°) — colour-science reference.
type Golden = (&'static str, f64, f64, f64);

const AVERAGE: [Golden; 12] = [
    (
        "#FF0000",
        46.286350626770606,
        103.28391138851904,
        27.404402166975032,
    ),
    (
        "#00FF00",
        79.26431694981305,
        97.81448138386348,
        141.80371267441325,
    ),
    (
        "#0000FF",
        25.271208691856113,
        78.73731063726906,
        282.87108092813014,
    ),
    (
        "#FFFFFF",
        100.00046713711697,
        1.545501438811922,
        209.67296579146105,
    ),
    (
        "#808080",
        43.305725087441154,
        1.0238966336053825,
        209.67429695202955,
    ),
    (
        "#787880",
        40.40683346416337,
        6.058635052053755,
        281.066383374154,
    ),
    (
        "#007AFF",
        42.880602062064696,
        63.769339904464935,
        265.95890184178467,
    ),
    (
        "#FFD700",
        82.0872358011999,
        58.5878532661407,
        96.23966382463577,
    ),
    (
        "#34C759",
        60.000509885775735,
        64.74176761657189,
        148.4567201493822,
    ),
    (
        "#101012",
        5.532647053285312,
        2.3690915388490237,
        281.8022487721104,
    ),
    (
        "#FF9500",
        64.42279364018762,
        57.38599407764123,
        62.42087343672503,
    ),
    (
        "#5856D6",
        34.497807570353054,
        57.96785871171937,
        286.4089036639489,
    ),
];

const DIM: [Golden; 12] = [
    (
        "#FF0000",
        51.74539084640614,
        98.79702480856254,
        27.4826611162173,
    ),
    (
        "#00FF00",
        81.97929575410213,
        90.99242198939255,
        142.1919929128466,
    ),
    (
        "#0000FF",
        30.849974885225063,
        79.34006093771785,
        282.6851849063455,
    ),
    (
        "#FFFFFF",
        100.00039876821765,
        2.6784920678646564,
        209.56165795636798,
    ),
    (
        "#808080",
        48.889705014167106,
        1.885432683347709,
        209.56436724107832,
    ),
    (
        "#787880",
        46.07752076362689,
        6.246721325901981,
        273.9675474848151,
    ),
    (
        "#007AFF",
        48.482938285031594,
        62.041975037721464,
        265.6376167353262,
    ),
    (
        "#FFD700",
        84.46532718887245,
        54.05141462047388,
        96.97442697593499,
    ),
    (
        "#34C759",
        64.61142183267079,
        61.57053638276709,
        148.95027006403305,
    ),
    (
        "#101012",
        8.41615018249882,
        2.8016425678959727,
        275.34959407640406,
    ),
    (
        "#FF9500",
        68.65632364660156,
        53.49214010163211,
        62.905131187983926,
    ),
    (
        "#5856D6",
        40.25320746117766,
        57.19210429387564,
        285.9779703590134,
    ),
];

fn check_table(vc: &ViewingConditions, table: &[Golden]) {
    for &(hex, j_ref, m_ref, h_ref) in table {
        let rgb = srgb_from_hex(hex).expect("golden table hex is valid");
        let (j, m, h) = cam16_jch_from_xyz(srgb_to_xyz(rgb), vc);
        let dh = ((h - h_ref + 180.0).rem_euclid(360.0) - 180.0).abs();
        assert!(
            (j - j_ref).abs() < TOL_J,
            "{hex} J: got {j}, reference {j_ref}"
        );
        assert!(
            (m - m_ref).abs() < TOL_M,
            "{hex} M: got {m}, reference {m_ref}"
        );
        assert!(dh < TOL_H_DEG, "{hex} h: got {h}, reference {h_ref}");
    }
}

#[test]
fn cam16_matches_colour_science_average_surround() {
    check_table(&ViewingConditions::srgb(), &AVERAGE);
}

#[test]
fn cam16_matches_colour_science_dim_surround() {
    check_table(&ViewingConditions::dim_surround(), &DIM);
}

/// (Y_fg, Y_bg, Lc) reference for the perceptual-contrast curve on the
/// achromatic axis.
///
/// `Y` is the relative luminance of an 8-bit grey swatch under the IEC
/// 61966-2-1 sRGB EOTF (CSS Color 4 luminance row); each value is shared
/// verbatim with the reference call. Reference `Lc` generated 2026-06-11
/// with the official `apca-w3` npm package v0.1.9 (`APCAcontrast(txtY, bgY)`,
/// SAPC-8 constants 0.0.98G-4g):
///
/// ```text
/// node -e "const {APCAcontrast}=require('apca-w3');
///   const e=v=>v<=0.04045?v/12.92:Math.pow((v+0.055)/1.055,2.4);
///   const y=b=>{const l=e(b/255);return 0.21263900587151027*l+0.715168678767756*l+0.07219231536073371*l;};
///   console.log(APCAcontrast(y(0),y(255)));"   // 106.04066682868873
/// ```
///
/// Grey-on-grey isolates the Helmholtz-Kohlrausch term out of the metric
/// (luminance is fed directly), so this validates the curve alone. Naming
/// and attribution policy: docs/decisions/apca-license.md.
type ContrastGolden = (f64, f64, f64);
const ACHROMATIC_CONTRAST: [ContrastGolden; 13] = [
    (0.0, 1.0, 106.04066682868873), // #000000 on #ffffff (BoW max)
    (1.0, 0.0, -107.8847261150986), // #ffffff on #000000 (WoB max)
    (0.05126945837404324, 1.0, 90.33357779023173), // #404040 on #ffffff
    (0.21586050011389923, 1.0, 63.72447786396962), // #808080 on #ffffff
    (1.0, 0.21586050011389923, -69.21596068658343), // #ffffff on #808080
    (0.014443843596092546, 0.5271151257058131, 66.36729437039027), // #202020 on #c0c0c0
    (0.35153259950043936, 0.029556834437808797, -45.3649219981398), // #a0a0a0 on #303030
    (0.11697066775851085, 0.7454042095403874, 60.45271381565166), // #606060 on #e0e0e0
    (
        0.21586050011389923,
        0.05126945837404324,
        -24.833313426389232,
    ), // #808080 on #404040
    (0.5775804404296506, 0.11697066775851085, -50.15676197758523), // #c8c8c8 on #606060
    (0.2788942634768104, 0.4341536361747489, 13.691098332113055), // #909090 on #b0b0b0 (just above clip)
    (0.21586050011389923, 0.23074004852434915, 0.0),              // #808080 on #848484 (loClip → 0)
    (0.21586050011389923, 0.21586050011389923, 0.0), // #808080 on #808080 (deltaYmin → 0)
];

#[test]
fn contrast_core_matches_reference_on_grey_axis() {
    // Tolerance 0.1 Lc: `contrast_core` is an independent Rust port of the
    // same published math, so agreement is to floating-point noise.
    const TOL_LC: f64 = 0.1;
    for &(y_fg, y_bg, lc_ref) in &ACHROMATIC_CONTRAST {
        let got = contrast_core(y_fg, y_bg);
        assert!(
            (got - lc_ref).abs() <= TOL_LC,
            "contrast_core({y_fg}, {y_bg}) = {got}, reference {lc_ref}"
        );
    }
}
