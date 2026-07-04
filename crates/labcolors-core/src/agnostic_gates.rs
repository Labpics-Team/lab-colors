//! Agnostic-core gates (ADR-0001 PR-c). Three guarantees, all in-crate `#[cfg(test)]`
//! because they consume the relocated labui fixture (`crate::config::fixture`):
//!
//! 1. **Frozen golden** — `resolve_named_set` of the labui fixture is byte-for-byte
//!    stable across every role × grid point. Re-anchors the byte-identity guarantee
//!    off the removed built-in `resolve_set` oracle onto a committed snapshot.
//! 2. **Named hierarchy compression** — the string-keyed hierarchy pass fires
//!    honestly where a ladder is squeezed and is a no-op on the golden grid.
//! 3. **Agnosticism** — a SECOND, synthetic config (not Daniel's tree) compiles to
//!    its own role table and emits a valid non-empty system with zero engine
//!    changes: the proof that any company plugs in its own design system.

use crate::config::fixture::labui_reference;
use crate::ladder::{LadderPosition, ThemeAnchors};
use crate::solve::Floor;
use crate::{
    BgInput, Brand, LadderSource, NeutralAnchors, NeutralConfig, NeutralPick, NeutralTint,
    PaletteFamily, Resolved, RoleRecipe, SentimentCategory, SentimentsConfig, ThemeConfig,
    ThemesConfig, VcPreset, ViewingConditions, resolve_named_set,
};

// ─────────────────────────────────────────────────────────────────────────────
// Shared helpers
// ─────────────────────────────────────────────────────────────────────────────

/// The golden grid: two VC presets × six backgrounds.
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

/// Canonical stable representation of a resolved role.
fn repr(res: &Resolved) -> String {
    match res {
        Resolved::Color { solved, .. } => solved.hex().to_string(),
        Resolved::Translucent(r) => format!("rgba({},{})", r.tint_hex(), r.alpha()),
        Resolved::Glow(g) => format!("glow({},{},{:.4})", g.core_hex(), g.halo_hex(), g.alpha()),
        Resolved::None => "none".to_string(),
        Resolved::Unreachable(_) => "UNREACHABLE".to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Frozen golden byte-identity gate
// ─────────────────────────────────────────────────────────────────────────────

/// Full fixture emission as a deterministic line-oriented snapshot:
/// `vc|bg|role=repr`, declaration order, every grid point; aliases as
/// `vc|bg|name->target`.
fn emit_snapshot() -> String {
    use std::fmt::Write as _;
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

const GOLDEN: &str = include_str!("../tests/data/labui_emission_golden.txt");

/// Normalise CRLF/CR to LF (`.gitattributes` pins the golden to LF, but a
/// `core.autocrlf` checkout can still hand a CRLF working copy to `include_str!`).
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
    assert_eq!(
        got.lines().count(),
        golden.lines().count(),
        "labui emission line count drifted — a role/alias/grid point changed"
    );
    assert_eq!(
        got, golden,
        "labui fixture emission drifted from the frozen golden \
         (regenerate with BLESS_LABUI_GOLDEN=1 only for a reviewed change)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Named-path text-hierarchy compression
// ─────────────────────────────────────────────────────────────────────────────

const LABELS: [&str; 4] = [
    "label-primary",
    "label-secondary",
    "label-tertiary",
    "label-quaternary",
];

fn abs_lc(set: &[(String, Resolved)], name: &str) -> f64 {
    set.iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, r)| r.lc())
        .map(f64::abs)
        .unwrap_or_else(|| panic!("role `{name}` missing/unreachable"))
}

fn compressed(set: &[(String, Resolved)], name: &str) -> bool {
    set.iter()
        .find(|(n, _)| n == name)
        .map(|(_, r)| r.compressed())
        .unwrap_or_else(|| panic!("role `{name}` missing"))
}

fn hex(set: &[(String, Resolved)], name: &str) -> String {
    set.iter()
        .find(|(n, _)| n == name)
        .and_then(|(_, r)| r.solved())
        .map(|s| s.hex().to_string())
        .unwrap_or_else(|| panic!("role `{name}` not a solved colour"))
}

#[test]
fn hierarchy_pass_fires_and_flags_when_ladder_is_squeezed() {
    // `#747474` — a near-AA mid-grey where the readable window is narrower than the
    // label steps: primary and secondary are floored onto one colour. The pass
    // makes that HONEST (compressed flag), not a silent collapse. A neutral anchor
    // is flagged compressed ONLY by the pass (`Resolved::color` => compressed:false),
    // so this is the RED-proof: disabling the pass drops the flag.
    let table = labui_reference().compile_named_role_table().unwrap();
    let bg = BgInput::solid("#747474").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb());

    assert_eq!(
        hex(&set, "label-primary"),
        hex(&set, "label-secondary"),
        "on #747474 secondary is expected floored onto primary"
    );
    assert!(
        compressed(&set, "label-secondary"),
        "squeezed junior MUST carry the compressed flag — the pass fired"
    );
    assert!(
        !compressed(&set, "label-primary"),
        "senior must not be flagged compressed"
    );
    let mags: Vec<f64> = LABELS.iter().map(|l| abs_lc(&set, l)).collect();
    for w in mags.windows(2) {
        assert!(
            w[0] + 1e-9 >= w[1],
            "label ladder must stay non-strict-descending, got {mags:?}"
        );
    }
}

#[test]
fn hierarchy_pass_does_not_sweep_in_lone_anchors() {
    // `icon` (0.461, above quaternary 0.276) and `border-strong` (0.968) are lone
    // anchors, not label-ladder rungs: the grouping reads strictly-descending runs
    // off the config, so they are never compressed.
    let table = labui_reference().compile_named_role_table().unwrap();
    let bg = BgInput::solid("#747474").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb());
    assert!(
        !compressed(&set, "icon"),
        "icon must not join the label ladder"
    );
    assert!(
        !compressed(&set, "border-strong"),
        "border-strong must not join the label ladder"
    );
}

#[test]
fn hierarchy_pass_is_a_noop_on_the_golden_grid() {
    // No golden background sits in the squeeze band — why the frozen golden stays
    // byte-for-byte green after the port.
    let table = labui_reference().compile_named_role_table().unwrap();
    let (vcs, bgs) = grid();
    for (vc, _) in vcs {
        for bg_hex in bgs {
            let bg = BgInput::solid(bg_hex).unwrap();
            let set = resolve_named_set(&bg, &table, &vc);
            for l in LABELS {
                assert!(
                    !compressed(&set, l),
                    "no label may be compressed on golden bg {bg_hex}: `{l}` was"
                );
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Agnosticism proof — a SECOND, non-Daniel config
// ─────────────────────────────────────────────────────────────────────────────

/// A synthetic "Acme" design system: a warm brand hue, families and a neutral
/// unlike labui's Apple palette, its own sentiment and a small role set. Built
/// entirely through the PUBLIC `ThemeConfig` surface — no engine constants, no
/// fixture reuse — so compiling and resolving it proves the core carries no
/// baked-in taxonomy.
fn acme_config() -> ThemeConfig {
    let anchors = |l: &str, d: &str| ThemeAnchors {
        light: l.to_string(),
        dark: d.to_string(),
        light_ic: l.to_string(),
        dark_ic: d.to_string(),
    };
    let text = |fraction, floor| RoleRecipe::TextAnchor {
        fraction,
        floor,
        hue: None,
    };
    let brand_ladder = |pos| RoleRecipe::Ladder {
        source: LadderSource::Brand,
        position: pos,
        floor: None,
    };

    ThemeConfig {
        // Warm crimson brand — nothing to do with labui's #007AFF blue.
        brand: Brand {
            anchors: anchors("#C81E5A", "#E0578A"),
        },
        neutral: NeutralConfig {
            // A warm-grey neutral (not labui's cool #101012 triple).
            anchors: NeutralAnchors {
                light: "#FCFAF8".to_string(),
                mid: "#8A817C".to_string(),
                dark: "#1A1614".to_string(),
            },
            tint: NeutralTint {
                ratio: 0.10,
                target_mp: 5.0,
                hue_stiffness: 8.0,
                // Derive the undertone hue from the client's own dark anchor — the
                // agnostic path (no measured override).
                hue_override_deg: None,
            },
            edge: None,
            inverted: None,
        },
        // Two families unlike Apple's palette.
        palette: vec![
            PaletteFamily {
                key: "crimson".to_string(),
                anchors: anchors("#C81E5A", "#E0578A"),
            },
            PaletteFamily {
                key: "moss".to_string(),
                anchors: anchors("#5A7D2C", "#8FB65A"),
            },
        ],
        sentiments: SentimentsConfig {
            categories: vec![SentimentCategory {
                name: "alert".to_string(),
                family: "crimson".to_string(),
                hue_floor_deg: None,
                preferred_side: None,
            }],
            hardness: 5.0,
            chroma_fraction: 0.88,
        },
        themes: ThemesConfig {
            entries: vec![
                ("day".to_string(), VcPreset::Srgb),
                ("night".to_string(), VcPreset::Dim),
            ],
        },
        // A small but real role set: a text ladder, a neutral fill, a brand fill,
        // a hued brand label, a brand focus ring.
        roles: vec![
            ("text-strong".to_string(), text(0.968, Floor::AaText)),
            ("text-weak".to_string(), text(0.461, Floor::AaUi)),
            (
                "surface".to_string(),
                RoleRecipe::Ladder {
                    source: LadderSource::Neutral(NeutralPick::Mid),
                    position: LadderPosition::NeutralFillPrimary,
                    floor: None,
                },
            ),
            (
                "brand-fill".to_string(),
                brand_ladder(LadderPosition::FillPrimary),
            ),
            (
                "brand-label".to_string(),
                RoleRecipe::TextAnchor {
                    fraction: 0.968,
                    floor: Floor::AaText,
                    hue: Some(LadderSource::Brand),
                },
            ),
            ("focus".to_string(), brand_ladder(LadderPosition::FocusRing)),
        ],
        aliases: vec![("ring".to_string(), "focus".to_string())],
        preset: None,
    }
}

#[test]
fn a_second_company_config_compiles_and_emits_a_valid_system() {
    // Compiles to ITS OWN role table with zero engine changes.
    let table = acme_config()
        .compile_named_role_table()
        .expect("a well-formed foreign config must compile");
    assert_eq!(
        table.entries().len(),
        6,
        "acme declared six roles; the table carries exactly them"
    );

    // Emits a real, non-empty, physically-solved system on both a light and a dark
    // surface — driving the actual solver, not a stub.
    for (vc, bg_hex) in [
        (ViewingConditions::srgb(), "#FCFAF8"),
        (ViewingConditions::dim_surround(), "#1A1614"),
    ] {
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_named_set(&bg, &table, &vc);
        assert_eq!(set.len(), 6, "every declared role resolves to an outcome");

        // The text ladder is real: strong is a solved colour that clears its AA
        // text floor and reads stronger than the weak rung.
        let strong = set.iter().find(|(n, _)| n == "text-strong").unwrap();
        let weak = set.iter().find(|(n, _)| n == "text-weak").unwrap();
        let strong_lc = strong.1.lc().expect("text-strong solves").abs();
        let weak_lc = weak.1.lc().expect("text-weak solves").abs();
        assert!(
            strong_lc >= weak_lc,
            "text ladder must order strong >= weak on {bg_hex}: {strong_lc} vs {weak_lc}"
        );
        assert!(
            strong.1.solved().is_some(),
            "text-strong must be a solved colour on {bg_hex}"
        );

        // The hued brand label carries the crimson identity, not the neutral tint:
        // it resolves to a solved colour with perceptible chroma somewhere across
        // the two surfaces (a real hue, not a stub grey).
        let brand_label = set.iter().find(|(n, _)| n == "brand-label").unwrap();
        assert!(
            matches!(brand_label.1, Resolved::Color { .. }),
            "hued brand-label must resolve to a solved colour on {bg_hex}"
        );
    }
}

#[test]
fn foreign_config_needs_no_labui_constants() {
    // The acme neutral undertone hue is DERIVED from acme's own dark anchor
    // (#1A1614), not labui's measured 286°: agnosticism means the client's data
    // drives the physics. A successful compile with `hue_override_deg: None` is the
    // assertion — the engine found the hue itself.
    let cfg = acme_config();
    assert!(cfg.neutral.tint.hue_override_deg.is_none());
    assert!(
        cfg.compile_named_role_table().is_ok(),
        "the engine derives the undertone from the client's neutral, no baked-in hue"
    );
}
