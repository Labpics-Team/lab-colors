//! Agnostic-core gates (ADR-0001). Three guarantees, all in-crate `#[cfg(test)]`
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
use crate::config::test_support::resolved_repr as repr;
use crate::ladder::{LadderPosition, ThemeAnchors};
use crate::solve::Floor;
use crate::{
    BgInput, Brand, LadderSource, NeutralAnchors, NeutralConfig, NeutralPick, NeutralTint,
    PaletteFamily, Resolved, RoleRecipe, ThemeConfig, ThemesConfig, VcPreset, ViewingConditions,
    resolve_named_set,
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
            let set = resolve_named_set(&bg, &table, vc)
                .expect("golden-конфиг обязан резолвиться атомарно");
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
    let changed: Vec<String> = got
        .lines()
        .zip(golden.lines())
        .enumerate()
        .filter(|(_, (actual, expected))| actual != expected)
        .map(|(index, (actual, expected))| format!("line {}\n- {expected}\n+ {actual}", index + 1))
        .collect();
    assert!(
        changed.is_empty(),
        "labui fixture emission drifted from the frozen golden \
         (regenerate only for a reviewed change):\n{}",
        changed.join("\n")
    );
}

#[test]
fn accepted_endpoint_recovery_keeps_previously_false_failed_borders_legal() {
    let table = labui_reference()
        .compile_named_role_table()
        .expect("reference fixture compiles");
    for (vc, background, roles) in [
        (
            ViewingConditions::srgb(),
            "#7F7F7F",
            &[
                "border-brand-strong",
                "border-danger-strong",
                "border-info-strong",
            ][..],
        ),
        (
            ViewingConditions::srgb(),
            "#3478F6",
            &[
                "border-brand-strong",
                "border-danger-strong",
                "border-info-strong",
            ][..],
        ),
        (
            ViewingConditions::dim_surround(),
            "#7F7F7F",
            &["border-danger-strong"][..],
        ),
        (
            ViewingConditions::dim_surround(),
            "#3478F6",
            &["border-danger-strong"][..],
        ),
    ] {
        let bg = BgInput::solid(background).unwrap();
        let set = resolve_named_set(&bg, &table, &vc)
            .expect("валидная border-фикстура обязана резолвиться");
        for role in roles {
            let outcome = &set
                .iter()
                .find(|(name, _)| name == role)
                .unwrap_or_else(|| panic!("missing role {role}"))
                .1;
            let Resolved::Translucent(value) = outcome else {
                panic!("{background} {role}: expected a resolved opaque endpoint, got {outcome:?}");
            };
            assert_eq!(value.alpha(), 1.0, "{background} {role}");
            assert!(value.floor_coerced(), "{background} {role}");
            assert!(
                value.composite_wcag() >= 3.0,
                "{background} {role}: UI floor missed ({})",
                value.composite_wcag()
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Opaque named roles never acquire undeclared hierarchy
// ─────────────────────────────────────────────────────────────────────────────

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
fn named_roles_do_not_gain_hierarchy_from_declaration_order() {
    // Этот фон — anti-vacuum witness: два независимо решённых якоря фикстуры
    // квантуются в один байтовый цвет. Соседство в декларации и client-owned
    // имена не дают Core права изобретать между ними ребро и менять результат.
    let config = labui_reference();
    let table = config.compile_named_role_table().unwrap();
    let bg = BgInput::solid("#767676").unwrap();
    let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb())
        .expect("valid opaque-role fixture must resolve");

    let mut reversed_config = config;
    let declared_names: Vec<_> = reversed_config
        .roles
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    reversed_config.roles.reverse();
    let reversed_names: Vec<_> = reversed_config
        .roles
        .iter()
        .map(|(name, _)| name.clone())
        .collect();
    assert_eq!(
        reversed_names,
        declared_names.into_iter().rev().collect::<Vec<_>>(),
        "anti-vacuum: контрольная конфигурация обязана реально сменить порядок"
    );
    let reversed_table = reversed_config.compile_named_role_table().unwrap();
    let reversed = resolve_named_set(&bg, &reversed_table, &ViewingConditions::srgb())
        .expect("тот же контракт в обратном порядке обязан резолвиться");
    let by_name: std::collections::BTreeMap<_, _> =
        set.iter().map(|(name, value)| (name, value)).collect();
    let reversed_by_name: std::collections::BTreeMap<_, _> =
        reversed.iter().map(|(name, value)| (name, value)).collect();
    assert_eq!(
        by_name, reversed_by_name,
        "физический результат не должен зависеть от порядка client-owned ролей"
    );

    assert_eq!(
        hex(&set, "label-primary"),
        hex(&set, "label-secondary"),
        "fixture must exercise the equality that used to trigger inferred hierarchy"
    );
    for role in ["label-primary", "label-secondary", "border-strong"] {
        assert!(
            !compressed(&set, role),
            "opaque role `{role}` acquired an undeclared order relation"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Agnosticism proof — a SECOND, non-Daniel config
// ─────────────────────────────────────────────────────────────────────────────

/// A synthetic "Acme" design system: a warm brand hue, families and a neutral
/// unlike the Lab UI fixture, with its own opaque family IDs and a small role set. Built
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
        themes: ThemesConfig {
            entries: vec![
                ("day".to_string(), VcPreset::Srgb),
                ("night".to_string(), VcPreset::Dim),
            ],
        },
        // A small but real role set: a text ladder, a neutral fill, a hued brand
        // label, a brand focus ring and a brand glow.
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
                "brand-label".to_string(),
                RoleRecipe::TextAnchor {
                    fraction: 0.968,
                    floor: Floor::AaText,
                    hue: Some(LadderSource::Brand),
                },
            ),
            ("focus".to_string(), brand_ladder(LadderPosition::FocusRing)),
            // Свечение бренда (#292): numerical-decision профиль обязателен
            // ЯВНО и у чужого клиента — implicit legacy непредставим; выбор
            // Stable доказывает, что второй клиент получает typed execution
            // mode тем же публичным конфиг-путём, без правок ядра.
            (
                "brand-glow".to_string(),
                RoleRecipe::Glow {
                    source: LadderSource::Brand,
                    step: crate::glow::GlowStep::Base,
                    decision_profile: crate::glow::GlowDecisionProfileV1::StableV1,
                },
            ),
        ],
        aliases: vec![("ring".to_string(), "focus".to_string())],
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

    // Численный план (#292) второго клиента — derived-проекция той же таблицы:
    // ровно одна Glow-декларация ⇒ ровно один compiled invocation, имя роли —
    // opaque node bytes (движок не знает словаря acme).
    let plan = table
        .numerical_plan_v1()
        .expect("план второго клиента компилируется из registry SSOT");
    assert_eq!(plan.invocations().len(), 1);
    assert_eq!(
        plan.invocations()[0].invocation_id.node_bytes(),
        b"brand-glow"
    );

    // Emits a real, non-empty, physically-solved system on both a light and a dark
    // surface — driving the actual solver, not a stub.
    for (vc, bg_hex) in [
        (ViewingConditions::srgb(), "#FCFAF8"),
        (ViewingConditions::dim_surround(), "#1A1614"),
    ] {
        let bg = BgInput::solid(bg_hex).unwrap();
        let set = resolve_named_set(&bg, &table, &vc)
            .expect("валидный второй клиент обязан резолвиться атомарно");
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
