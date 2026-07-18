//! Reference checks reachable through the PUBLIC API.
//!
//! Companion to the crate-internal `reference_vectors_deep` (which reaches
//! `pub(crate)` transforms). The checks combine published control points,
//! independently transcribed formulae and explicit cross-boundary identities.
//! Each assertion owns its source and oracle boundary; crate-private companion
//! checks live in `src/reference_vectors_deep.rs`.
//!
//! Sources:
//! * W3C WCAG 2.1 §1.4.3 / §1.4.11 — relative luminance & contrast ratio.
//! * Li et al. 2017, DOI 10.1002/col.22131 / CIE 248:2022 — CIECAM16 viewing
//!   conditions derivation.
//! * Björn Ottosson (2020) / W3C CSS Color 4 — Oklab white & primary hues.

// `spaces` is `pub(crate)`; these transforms are re-exported at the crate root.
use labcolors_core::{ViewingConditions, oklch_css_from_hex, oklch_from_hex, recheck_against};

/// WCAG contrast ratio of `fg` on `bg` under sRGB conditions, through the public
/// `recheck_against` (its `.1` is `wcag::contrast_ratio` on the quantised
/// display colours — bit-identical to the direct W3C formula on 8-bit inputs).
fn wcag_ratio(fg: &str, bg: &str) -> f64 {
    recheck_against(bg, &[fg], &ViewingConditions::srgb()).expect("valid hexes")[0].1
}

// ─────────────────────────────────────────────────────────────────────────────
// WCAG 2.1 — W3C published contrast ratios and luminance coefficients.
// ─────────────────────────────────────────────────────────────────────────────

/// The canonical published WCAG ratios: black-on-white is the 21:1 extreme, and
/// `#767676` on white is the textbook AA-text boundary (~4.54:1), with one
/// 8-bit step lighter (`#777777`) dropping below 4.5:1.
#[test]
fn wcag_published_ratios_via_public_api() {
    let bw = wcag_ratio("#000000", "#FFFFFF");
    assert!(
        (bw - 21.0).abs() < 1e-6,
        "black on white must be 21:1, got {bw}"
    );

    // W3C / WebAIM textbook AA-text boundary.
    let boundary = wcag_ratio("#767676", "#FFFFFF");
    assert!(
        (boundary - 4.5422).abs() < 1e-3,
        "#767676 on white must be ≈4.5422:1 (published), got {boundary}"
    );
    assert!(
        boundary >= 4.5,
        "#767676 must clear AA text (4.5:1), got {boundary}"
    );
    // One quantisation step lighter falls below.
    let below = wcag_ratio("#777777", "#FFFFFF");
    assert!(below < 4.5, "#777777 on white must be < 4.5:1, got {below}");
    assert!(
        (below - 4.4781).abs() < 1e-3,
        "#777777 published ≈4.4781, got {below}"
    );
}

/// Each WCAG luminance weight is isolated by a pure primary on black: the ratio
/// is `(K + 0.05) / 0.05`, so red → 5.252 pins `K_R = 0.2126`, green → 15.304
/// pins `K_G = 0.7152`, blue → 2.444 pins `K_B = 0.0722`.
#[test]
fn wcag_luminance_coefficients_isolated() {
    // (primary hex, weight, expected ratio on black).
    for (hex, k) in [
        ("#FF0000", 0.2126),
        ("#00FF00", 0.7152),
        ("#0000FF", 0.0722),
    ] {
        let ratio = wcag_ratio(hex, "#000000");
        let want = (k + 0.05) / 0.05;
        // Pure primaries are exact 8-bit; agreement is to formula round-off.
        assert!(
            (ratio - want).abs() < 1e-4,
            "{hex} on black: ratio {ratio} isolates weight {k} (expected {want})"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Oklab — Ottosson (2020) / CSS Color 4 published landmarks (public `oklch_from_hex`).
// ─────────────────────────────────────────────────────────────────────────────

/// sRGB white maps to Oklab `L = 1`, chroma ≈ 0 — Ottosson's defining constraint
/// (XYZ→Oklab table row 1). The published model leaves a ~6.5e-9 offset in L, so
/// the tolerance is 1e-6, not exact.
#[test]
fn oklab_white_is_l1_c0() {
    let [l, c, _h] = oklch_from_hex("#FFFFFF").expect("white is valid");
    assert!((l - 1.0).abs() < 1e-6, "white L must be 1, got {l}");
    assert!(c < 1e-3, "white chroma must be ≈0, got {c}");
}

/// The Oklab hue angles of the sRGB primaries are the published canonical values
/// (Ottosson / CSS Color 4 ecosystem): red ≈ 29.23°, green ≈ 142.5°,
/// blue ≈ 264.05°. Tolerance 1° absorbs rounding across sources while a wrong
/// matrix moves a hue by tens of degrees.
#[test]
fn oklch_primary_hues() {
    for (hex, want_h) in [("#FF0000", 29.23), ("#00FF00", 142.5), ("#0000FF", 264.05)] {
        let h = oklch_from_hex(hex).expect("primary is valid")[2];
        let dh = ((h - want_h + 180.0).rem_euclid(360.0) - 180.0).abs();
        assert!(
            dh < 1.0,
            "{hex} Oklab hue {h}°, published ≈{want_h}° (Δ {dh}°)"
        );
    }
    // Sign contract per Ottosson: red has a>0, blue has b<0.
    let [_, cr, hr] = oklch_from_hex("#FF0000").unwrap();
    assert!(cr * hr.to_radians().cos() > 0.0, "red must have a > 0");
    let [_, cb, hb] = oklch_from_hex("#0000FF").unwrap();
    assert!(cb * hb.to_radians().sin() < 0.0, "blue must have b < 0");
}

// ─────────────────────────────────────────────────────────────────────────────
// CIECAM16 viewing conditions — Li et al. 2017 / CIE 248:2022 (public fields).
// ─────────────────────────────────────────────────────────────────────────────

/// The crate's `ViewingConditions::srgb()` (L_A = 64, Y_b = 20, D65, average
/// surround) is rederived from an INDEPENDENT transcription of the published
/// CIECAM16 initialisation (Li et al. 2017 §2.1 / CIE 248:2022): `F_L`, `n`, `z`,
/// `N_bb`, the degree of adaptation `D`, `RGB_D`, and the achromatic white
/// response `A_w`. Reading the public fields, a typo in `vc.rs::build` breaks
/// this by orders of magnitude.
#[test]
fn cam16_viewing_conditions_derivation() {
    // Independent published transcription. D65 from chromaticity (0.3127, 0.3290).
    let (x, y) = (0.3127, 0.3290);
    let xyz_w = [100.0 * x / y, 100.0, 100.0 * (1.0 - x - y) / y];
    let (l_a, y_b, f) = (64.0_f64, 20.0_f64, 1.0_f64); // average surround F = 1.0

    let k = 1.0 / (5.0 * l_a + 1.0);
    let k4 = k * k * k * k;
    let fl = k4 * l_a + 0.1 * (1.0 - k4).powi(2) * (5.0 * l_a).cbrt();
    let n = y_b / 100.0;
    let nbb = 0.725 * n.powf(-0.2);
    let z = 1.48 + n.sqrt();

    // CAT16 white cone response (published forward matrix).
    let cat16 = [
        [0.401288, 0.650173, -0.051461],
        [-0.250268, 1.204414, 0.045854],
        [-0.002079, 0.048952, 0.953127],
    ];
    let mv = |m: [[f64; 3]; 3], v: [f64; 3]| {
        [
            m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
            m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
            m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        ]
    };
    let rgb_w = mv(cat16, xyz_w);
    let d = (f * (1.0 - (1.0 / 3.6) * ((-l_a - 42.0) / 92.0).exp())).clamp(0.0, 1.0);
    let rgb_d = [
        d * (100.0 / rgb_w[0]) + 1.0 - d,
        d * (100.0 / rgb_w[1]) + 1.0 - d,
        d * (100.0 / rgb_w[2]) + 1.0 - d,
    ];
    // Published post-adaptation compression.
    let adapt = |c: f64| {
        let t = (fl * c.abs() / 100.0).powf(0.42);
        c.signum() * 400.0 * t / (t + 27.13)
    };
    let rgb_aw = [
        adapt(rgb_w[0] * rgb_d[0]),
        adapt(rgb_w[1] * rgb_d[1]),
        adapt(rgb_w[2] * rgb_d[2]),
    ];
    // Achromatic white response. NOTE: this mirrors `vc.rs::build` exactly,
    // including its omission of the CIE `− 0.305` offset that the full CIECAM16
    // achromatic signal `A = (2R'+G'+B'/20 − 0.305)·N_bb` carries (the crate
    // drops it consistently in both `A_w` here and `A` in `cam16::forward`). So
    // this asserts TRANSCRIPTION PARITY with the crate's initialisation, not the
    // literal CIE constant; the crate's absolute CAM16 accuracy at these exact
    // parameters is anchored separately by the colour-science golden
    // (`golden_tests::cam16_matches_colour_science_*`).
    let aw = (2.0 * rgb_aw[0] + rgb_aw[1] + rgb_aw[2] / 20.0) * nbb;

    let vc = ViewingConditions::srgb();
    // Independent transcription of the SAME published formulas: agreement is to
    // float round-off, so any real divergence in `vc.rs` is caught at 1e-9.
    assert!((vc.fl() - fl).abs() < 1e-9, "F_L: {} vs {fl}", vc.fl());
    assert!((vc.n() - n).abs() < 1e-12, "n: {} vs {n}", vc.n());
    assert!((vc.z() - z).abs() < 1e-12, "z: {} vs {z}", vc.z());
    assert!((vc.nbb() - nbb).abs() < 1e-9, "N_bb: {} vs {nbb}", vc.nbb());
    assert!((vc.aw() - aw).abs() < 1e-6, "A_w: {} vs {aw}", vc.aw());
    for (i, (got, want)) in vc.rgb_d().into_iter().zip(rgb_d).enumerate() {
        assert!((got - want).abs() < 1e-9, "RGB_D[{i}]: {got} vs {want}");
    }

    // Published surround triplet (average): F = 1.0, c = 0.69, N_c = 1.0
    // (CIE 159:2004 Table 1, carried into CIE 248:2022).
    assert!(
        (vc.c() - 0.69).abs() < 1e-12,
        "average surround c must be 0.69"
    );
    assert!(
        (vc.nc() - 1.0).abs() < 1e-12,
        "average surround N_c must be 1.0"
    );
    // Dim surround (dark-theme) triplet: F = 0.9, c = 0.59, N_c = 0.9.
    let dim = ViewingConditions::dim_surround();
    assert!(
        (dim.c() - 0.59).abs() < 1e-12,
        "dim surround c must be 0.59"
    );
    assert!(
        (dim.nc() - 0.9).abs() < 1e-12,
        "dim surround N_c must be 0.9"
    );
}

// ═════════════════════════════════════════════════════════════════════════════
// JS↔Rust byte-parity bridge: the fixture the JS `reference-vectors.test.mjs`
// asserts `parseCssColor` against. The strings are EMITTED by the core's public
// `oklch_css_from_hex`; the JS decode of each must reproduce the seed bytes
// (byte-exact round-trip, proven for the core by `oklch::round_trip_is_byte_exact`).
// This file OWNS the seed set; the committed fixture is the artifact the JS test
// reads; `oklch_core_vectors_fixture_is_fresh` keeps it in lock-step with the
// live emitter, while `packages/colors/test/reference-vectors.test.mjs`
// independently decodes the committed strings back to the seed bytes.
// ═════════════════════════════════════════════════════════════════════════════

const FIXTURE_REL: &str = "/../../packages/colors/test/data/oklch-core-vectors.txt";

/// Deterministic seed set (hex, optional alpha) for the JS↔Rust parity fixture.
///
/// Covers the edges the JS parser must survive: L=0/1 (`#000000`/`#FFFFFF`), the
/// whole C=0 grey axis (256 greys, achromatic hue), a saturated hue lattice, a
/// seeded pseudo-random body, and a spread of translucent (alpha) emissions. The
/// LCG is fixed so the sequence is byte-reproducible across runs and platforms.
fn parity_seeds() -> Vec<(String, Option<f64>)> {
    let mut out: Vec<(String, Option<f64>)> = Vec::new();
    let hex = |r: u8, g: u8, b: u8| format!("#{r:02X}{g:02X}{b:02X}");

    // Edge: the two luminance endpoints, explicitly first.
    out.push((hex(0, 0, 0), None));
    out.push((hex(255, 255, 255), None));

    // C=0 edge: the full 8-bit grey axis (achromatic hue is numerically arbitrary
    // yet must round-trip). 256 vectors.
    for v in 0u16..=255 {
        let v = v as u8;
        out.push((hex(v, v, v), None));
    }

    // Saturated hue lattice: step 51 over each channel (6^3 = 216), spanning the
    // gamut corners and every primary/secondary edge (H = 0..360 coverage).
    for &r in &[0u8, 51, 102, 153, 204, 255] {
        for &g in &[0u8, 51, 102, 153, 204, 255] {
            for &b in &[0u8, 51, 102, 153, 204, 255] {
                out.push((hex(r, g, b), None));
            }
        }
    }

    // Seeded pseudo-random body — a fixed LCG (Numerical Recipes constants) so the
    // sequence is identical every run. Fill past 1000 total.
    let mut state: u64 = 0x1234_5678_9abc_def0;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as u32
    };
    while out.len() < 980 {
        let r = (next() & 0xFF) as u8;
        let g = (next() & 0xFF) as u8;
        let b = (next() & 0xFF) as u8;
        out.push((hex(r, g, b), None));
    }

    // Translucent edge: alpha emissions. Alphas chosen with ≤3 decimals so the
    // emitter's 4-dp-trimmed output round-trips exactly through `parseFloat`.
    let alphas = [0.05_f64, 0.122, 0.25, 0.361, 0.5, 0.8, 0.9, 1.0];
    for a in alphas {
        // Pair each alpha with a spread of colours (edges + a random one).
        for base in [
            hex(0, 0, 0),
            hex(255, 255, 255),
            hex(0x10, 0x10, 0x12),
            hex(0x3E, 0x87, 0xFF),
            hex(0xFF, 0x3B, 0x30),
            hex(
                (next() & 0xFF) as u8,
                (next() & 0xFF) as u8,
                (next() & 0xFF) as u8,
            ),
        ] {
            out.push((base, Some(a)));
        }
    }
    out
}

/// Render the seed set to the fixture line format `#RRGGBB|alpha|css`
/// (`alpha` = `-` for solids), one per line, via the LIVE core emitter.
fn render_fixture() -> String {
    let mut s = String::new();
    for (hex, alpha) in parity_seeds() {
        let css = oklch_css_from_hex(&hex, alpha).expect("seed hex is valid");
        let atok = match alpha {
            None => "-".to_string(),
            Some(a) => format!("{a}"),
        };
        s.push_str(&format!("{hex}|{atok}|{css}\n"));
    }
    s
}

/// GENERATOR (run once with `--ignored`): writes the committed JS-parity fixture
/// from the live core emitter. The committed file is the artifact; the anti-drift
/// test below guards it, and `reference-vectors.test.mjs` consumes it.
///
/// `cargo test -p labcolors-core --test reference_vectors emit_oklch_core_vectors_fixture -- --ignored`
#[test]
#[ignore]
fn emit_oklch_core_vectors_fixture() {
    let path = format!("{}{FIXTURE_REL}", env!("CARGO_MANIFEST_DIR"));
    std::fs::write(&path, render_fixture()).expect("write fixture");
    eprintln!("wrote {path} ({} vectors)", parity_seeds().len());
}

/// Structured view of a fixture line for tolerant comparison.
struct FxLine {
    hex: String,
    atok: String,
    l: f64,
    c: f64,
    h: f64,
}

/// Parse one `#RRGGBB|atok|oklch(L% C H[ / A])` fixture line.
fn parse_fixture_line(line: &str) -> FxLine {
    let mut parts = line.splitn(3, '|');
    let hex = parts.next().expect("hex field").to_string();
    let atok = parts.next().expect("alpha field").to_string();
    let css = parts.next().expect("css field");
    let inner = css
        .strip_prefix("oklch(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or_else(|| panic!("malformed css: {css}"));
    // Drop the alpha suffix (validated separately via `atok`).
    let lch = inner.split(" / ").next().unwrap();
    let nums: Vec<f64> = lch
        .split_whitespace()
        .map(|t| {
            t.trim_end_matches('%')
                .parse::<f64>()
                .expect("numeric component")
        })
        .collect();
    assert_eq!(nums.len(), 3, "expected L C H in {css}");
    FxLine {
        hex,
        atok,
        l: nums[0],
        c: nums[1],
        h: nums[2],
    }
}

/// ANTI-DRIFT: the committed fixture must match a fresh render from the live
/// `oklch_css_from_hex` — same count, same order, same seed hexes and alpha
/// tokens, and numeric components equal to within one printed-digit step.
///
/// Structural fields (hex, alpha token, line count/order) are compared EXACTLY.
/// The numeric L/C/H are compared with a tolerance of 2× the emitter's printed
/// granularity (L% `.5f`→2e-5, C `.6f`→2e-6, H `.3f`→2e-3): the ONLY thing that
/// can move a value inside that band is a last-digit rounding flip from a 1-ULP
/// `cbrt`/`atan2`/`powf` difference between the platform that generated the
/// committed file and the platform running CI — never a real change (a formula
/// or precision regression moves values by orders more, well outside the band).
/// The HARD correctness gate stays exact: `oklch::round_trip_is_byte_exact_*`
/// (Rust) and `reference-vectors.test.mjs` (JS decodes each committed string to
/// the seed bytes, platform-independently). If this fails, regenerate with
/// `emit_oklch_core_vectors_fixture --ignored` and re-run the JS parity.
#[test]
fn oklch_core_vectors_fixture_is_fresh() {
    let path = format!("{}{FIXTURE_REL}", env!("CARGO_MANIFEST_DIR"));
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("fixture {path} missing ({e}) — regenerate with emit_oklch_core_vectors_fixture --ignored")
    });
    let fresh = render_fixture();
    let c: Vec<&str> = committed.lines().collect();
    let f: Vec<&str> = fresh.lines().collect();
    assert!(
        f.len() >= 1000,
        "parity fixture must carry ≥1000 vectors, has {}",
        f.len()
    );
    assert_eq!(
        c.len(),
        f.len(),
        "fixture line count drifted from live emitter"
    );
    for (i, (a, b)) in c.iter().zip(f.iter()).enumerate() {
        let (ca, cb) = (parse_fixture_line(a), parse_fixture_line(b));
        let n = i + 1;
        // Structure: exact.
        assert_eq!(ca.hex, cb.hex, "line {n}: seed hex drifted");
        assert_eq!(ca.atok, cb.atok, "line {n}: alpha token drifted");
        // Numeric: within one printed-digit step (cross-platform ULP guard).
        assert!(
            (ca.l - cb.l).abs() < 2e-5,
            "line {n}: L% drifted {a} vs {b}"
        );
        assert!((ca.c - cb.c).abs() < 2e-6, "line {n}: C drifted {a} vs {b}");
        assert!((ca.h - cb.h).abs() < 2e-3, "line {n}: H drifted {a} vs {b}");
    }
}
