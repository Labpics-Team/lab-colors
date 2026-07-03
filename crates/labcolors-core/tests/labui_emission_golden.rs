//! Frozen byte-identity gate for the labui fixture (ADR-0001 PR-c, point 2).
//!
//! The pre-agnostic byte-identity gate (`config::tests`) compared the string-keyed
//! path against the built-in `resolve_set(RoleTable::default())`. Once the built-in
//! showcase leaves the production API, that oracle is gone — so the guarantee is
//! re-anchored here to a **frozen golden emission**: the exact `resolve_named_set`
//! output of the labui fixture, every role × every grid point, captured while the
//! two paths were still provably byte-identical. After the removal, only the
//! *source* of the fixture moves; these bytes must not.
//!
//! Regenerate (only when a deliberate, reviewed emission change lands):
//!   BLESS_LABUI_GOLDEN=1 cargo +1.96.0 test --test labui_emission_golden -- --nocapture
//!
//! RED-proof: tamper one byte of `data/labui_emission_golden.txt` → the assert bites.

use std::fmt::Write as _;

use labcolors_core::config::labui_reference;
use labcolors_core::{BgInput, Resolved, ViewingConditions, resolve_named_set};

/// The golden grid: two VC presets × six backgrounds — the same 12 surfaces the
/// core 240-cell golden and the config byte-identity test walk.
fn grid() -> ([(ViewingConditions, &'static str); 2], [&'static str; 6]) {
    (
        [
            (ViewingConditions::srgb(), "srgb"),
            (ViewingConditions::dim_surround(), "dim"),
        ],
        [
            "#FFFFFF", "#F2F2F7", "#7F7F7F", "#1C1C1E", "#101012", "#3478F6",
        ],
    )
}

/// Canonical stable representation of a resolved role — hex / rgba / glow / none /
/// UNREACHABLE, identical to the representation the config byte-identity test uses.
fn repr(res: &Resolved) -> String {
    match res {
        Resolved::Color { solved, .. } => solved.hex().to_string(),
        Resolved::Translucent(r) => format!("rgba({},{})", r.tint_hex(), r.alpha()),
        Resolved::Glow(g) => format!("glow({},{},{:.4})", g.core_hex(), g.halo_hex(), g.alpha()),
        Resolved::None => "none".to_string(),
        Resolved::Unreachable(_) => "UNREACHABLE".to_string(),
        // `Resolved` is `#[non_exhaustive]`; a future variant must surface loudly
        // as a golden change, never be silently coerced into a known bucket.
        _ => "UNHANDLED_VARIANT".to_string(),
    }
}

/// Emit the full fixture emission as a deterministic, line-oriented snapshot:
/// `vc|bg|role=repr`, in declaration order, for every grid point. Aliases are
/// emitted too (`vc|bg|alias->target`) so the contract's reference roles are pinned.
fn emit_snapshot() -> String {
    let table = labui_reference()
        .compile_named_role_table()
        .expect("эталонная фикстура labui обязана компилироваться");
    let (vcs, bgs) = grid();
    let mut out = String::new();
    for (vc, vc_name) in &vcs {
        for bg_hex in bgs {
            let bg = BgInput::solid(bg_hex).expect("golden bg parses");
            let set = resolve_named_set(&bg, &table, vc);
            for (name, res) in &set {
                let _ = writeln!(out, "{vc_name}|{bg_hex}|{name}={}", repr(res));
            }
            for (name, target) in table.aliases() {
                let _ = writeln!(out, "{vc_name}|{bg_hex}|{name}->{target}");
            }
        }
    }
    out
}

const GOLDEN: &str = include_str!("data/labui_emission_golden.txt");

/// Normalise CRLF/CR to LF. The golden is stored LF (pinned by `.gitattributes`),
/// but `core.autocrlf` checkouts can still hand a CRLF working copy to
/// `include_str!`; line endings are not part of the colour contract, so the gate
/// compares emission content, not the platform's newline convention.
fn lf(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

#[test]
fn labui_fixture_emission_is_byte_identical_to_frozen_golden() {
    let got = emit_snapshot();

    if std::env::var("BLESS_LABUI_GOLDEN").is_ok() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/data/labui_emission_golden.txt"
        );
        std::fs::write(path, &got).expect("write golden");
        eprintln!("BLESSED labui golden ({} bytes) -> {path}", got.len());
        return;
    }

    let (got, golden) = (lf(&got), lf(GOLDEN));
    // The line count is pinned first so a role added/dropped from the fixture is a
    // loud, specific failure rather than a diff buried in the middle.
    assert_eq!(
        got.lines().count(),
        golden.lines().count(),
        "labui emission line count drifted — a role/alias/grid point changed"
    );
    // Byte-for-byte. The two-path equivalence that justified these values is the
    // config byte-identity test; here we only guard that the frozen bytes hold.
    assert_eq!(
        got, golden,
        "labui fixture emission drifted from the frozen golden \
         (regenerate with BLESS_LABUI_GOLDEN=1 only for a reviewed change)"
    );
}
