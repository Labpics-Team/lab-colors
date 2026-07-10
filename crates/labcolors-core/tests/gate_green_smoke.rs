//! Smoke gate — R4 regime, entry condition for the RED-proof.
//!
//! BUG CLASS this guards: the empirical-inventory gate is present in the workspace
//! but vacuous — either the scanner is mis-scoped (detects zero consts) or the
//! gate module is not compiled into the test binary, so every subsequent gate test
//! is green-from-birth with nothing to check.
//!
//! Invariant: gate present => suite not vacuous. Specifically:
//!   1. The scanner detects at LEAST one POLICY const across the 6 perceptual
//!      modules (a zero count means the allowlist ate everything or a module path
//!      broke — either way the gate proves nothing).
//!   2. Every perceptual module file is readable (path resolution is hermetic).
//!   3. The SSOT inventory is readable and parses to at LEAST one row (an empty
//!      SSOT means the gate never fails on a missing row — also vacuous).
//!
//! How this test bites (mutation proof):
//!   Remove one module from PERCEPTUAL_MODULES_SMOKE → the detected count drops
//!   toward zero and the lower-bound assertion fails. Remove the SSOT file →
//!   the read_to_string panics with a named error (the `read_inventory` contract
//!   already guarantees this). The test is therefore NOT green-from-birth:
//!   a mis-scoped scanner drives it RED.
//!
//! Regime separation (INV-7): this smoke tests PRESENCE and WIRING only; it
//! asserts nothing about the MAGNITUDE of detected values or the correctness of
//! the SSOT. Those are GATE-1/2/3/4's job.

mod common;
use common::{inventory_path, src_dir};

// ─────────────────────────────────────────────────────────────────────────────
// Minimal re-declaration of the audit surface. Kept independent of the gate
// module (which is a test file, not a library) so this smoke compiles cleanly.
// ─────────────────────────────────────────────────────────────────────────────

const PERCEPTUAL_MODULES_SMOKE: [&str; 6] = [
    "semantic.rs",
    "scale.rs",
    "sentiment.rs",
    "neutral.rs",
    "lpc.rs",
    "lcs.rs",
];

/// Minimal policy-const counter: counts `const … : f64` / `f32` / DjMagnitude
/// lines that are NOT on the standard allowlist. This mirrors the type of scan
/// `empirical_inventory.rs` performs, but only needs to prove the count is
/// nonzero — it does not need to match the full gate logic exactly.
fn count_policy_consts_approx(source: &str) -> usize {
    // Standard names excluded by construction in the gate — same list.
    const STD: &[&str] = &[
        "HK_CHROMA_EXPONENT",
        "LC_SCALE",
        "DELTA_Y_MIN",
        "S_PERC_MIN",
        "RATIO_BISECT_EPS",
        "RATIO_EPS",
        "FLOOR_EPS",
        "GAMUT_EPS",
        "POLARITY_FLOOR_RATIO",
        // Structural non-policy
        "CURVE_PLAN_CACHE_CAP",
        "CURVE_REFINE_STEPS",
        "LIGHTNESS_SETTLE",
    ];

    let mut count = 0;
    for line in source.lines() {
        let t = line.trim_start();
        // A const line bearing an f64 / f32 / DjMagnitude type declaration.
        if (t.starts_with("const ")
            || t.starts_with("pub const ")
            || t.starts_with("pub(crate) const "))
            && (t.contains(": f64 =") || t.contains(": f32 =") || t.contains(": DjMagnitude ="))
        {
            // Extract the name — the run of ident chars after "const ".
            let after = t
                .trim_start_matches("pub(crate) const ")
                .trim_start_matches("pub const ")
                .trim_start_matches("const ");
            let name: String = after
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            if !STD.contains(&name.as_str()) {
                count += 1;
            }
        }
    }
    count
}

/// Count data rows in the SSOT markdown table (lines whose first `|`-cell is a
/// bare integer).
fn count_inventory_rows(text: &str) -> usize {
    text.lines()
        .filter(|l| {
            let t = l.trim();
            if !t.starts_with('|') {
                return false;
            }
            let cells: Vec<&str> = t.trim_matches('|').split('|').map(str::trim).collect();
            cells.len() >= 2 && cells[0].parse::<usize>().is_ok()
        })
        .count()
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// SMOKE: every perceptual module file is readable and the scanner detects at
/// least one policy const in the combined source. A zero count is a gate-wiring
/// defect: either the module list is wrong or the scanner ate every const via
/// the allowlists.
///
/// Bites (mutation proof): remove any entry from `PERCEPTUAL_MODULES_SMOKE`
/// and the cumulative count may drop to zero if that module held all the policy
/// consts — the `>= 1` assertion then fails. Add everything to the allowlist
/// and the count reaches zero — same failure. The test is not green-from-birth.
#[test]
fn scanner_detects_nonzero_policy_consts() {
    let mut total = 0usize;
    for module in PERCEPTUAL_MODULES_SMOKE {
        let path = src_dir().join(module);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read perceptual module {}: {e}", path.display()));
        let n = count_policy_consts_approx(&source);
        total += n;
    }
    assert!(
        total >= 1,
        "SMOKE FAILED — the policy-const scanner detected ZERO consts across {:?}. \
         Either every const is on the allowlist (the gate is vacuous) or a module path \
         broke (the gate never runs). Detected: {total}",
        PERCEPTUAL_MODULES_SMOKE
    );
}

/// SMOKE: the SSOT inventory exists on disk and contains at least one data row.
/// An empty or missing SSOT means GATE-2/3 never have a row to compare against —
/// both gates pass vacuously, which is a green-from-birth defect.
///
/// Bites (mutation proof): delete `empirical-inventory.md` → read_to_string fails
/// with a named panic (the gate contract). Truncate all data rows → count == 0
/// → assertion fails.
#[test]
fn ssot_inventory_is_present_and_nonempty() {
    let path = inventory_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "SMOKE FAILED — SSOT inventory missing at {} ({e}). \
             The empirical-inventory gate REQUIRES docs/empirical-inventory.md.",
            path.display()
        )
    });
    let rows = count_inventory_rows(&text);
    assert!(
        rows >= 1,
        "SMOKE FAILED — SSOT inventory at {} has ZERO data rows. \
         GATE-2 and GATE-3 would be vacuously green. Add at least one policy row.",
        path.display()
    );
}

/// SMOKE: marker count and inventory row count agree at the ORDER-OF-MAGNITUDE
/// level (both nonzero). This is a coarse coherence check — the precise 1:1
/// bijection is GATE-2's job. Here we only assert neither side is zero, which
/// would make the gate vacuous.
///
/// Bites (mutation proof): truncate all `// NEEDS-SCIENCE` / `// GROUNDED`
/// markers in the source → the marker count drops to zero while row count stays
/// positive → the lower-bound assertion on `marker_count` fails.
#[test]
fn marker_count_and_row_count_are_both_nonzero() {
    let mut marker_count = 0usize;
    for module in PERCEPTUAL_MODULES_SMOKE {
        let path = src_dir().join(module);
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        for line in source.lines() {
            if line.contains("// NEEDS-SCIENCE") || line.contains("// GROUNDED") {
                marker_count += 1;
            }
        }
    }
    assert!(
        marker_count >= 1,
        "SMOKE FAILED — zero `// NEEDS-SCIENCE` / `// GROUNDED` markers detected \
         across all 6 perceptual modules. The gate's marker-presence check (GATE-1) \
         would be vacuously green: every unmarked const would pass.",
    );

    let inv_path = inventory_path();
    let inv_text =
        std::fs::read_to_string(&inv_path).unwrap_or_else(|e| panic!("cannot read SSOT: {e}"));
    let row_count = count_inventory_rows(&inv_text);
    assert!(
        row_count >= 1,
        "SMOKE FAILED — SSOT has zero data rows. Marker count={marker_count}, row count=0 \
         → GATE-2 bijection is vacuously green.",
    );
}
