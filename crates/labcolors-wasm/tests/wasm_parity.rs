//! Headless-browser contract smoke: JS-binding resolve must equal the core
//! `resolve_named_set` it wraps, role for role, внутри одного wasm runtime.
//!
//! Run with `wasm-pack test --headless --chrome` (D1 default from the chapter).
//! The engine is agnostic (ADR-0001 PR-c): it has no built-in table, so parity
//! is proven against a LOADED config (the frozen labui passport). Expectations
//! are GENERATED from the core's own `resolve_named_set` inside the same wasm
//! runtime — never hand-typed — so this test cannot drift from the engine and
//! stays correct when the role set grows.

#![cfg(target_arch = "wasm32")]

use labcolors_conformance::{
    AlphaVector, ContrastVector, DRIFT_TOL, LadderVector, Manifest, MuddinessVector, Pack,
    SolveOutcome, SolveVector, Wcag22Vector,
};
use labcolors_core::config::ThemeConfig;
use labcolors_core::semantic::NamedRoleTable;
use labcolors_core::{BgInput, Resolved, ViewingConditions, resolve_named_set};
use labcolors_wasm::LabColors;
use labcolors_wasm::config_dto::ConfigDto;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// The frozen labui passport — the config the built-in default table used to
/// hardcode. Both the boundary (via `loadConfig`) and the core expectation
/// (via `ConfigDto` → `ThemeConfig` → `compile_named_role_table`) read this one
/// SSOT, so the two sides cannot diverge on their input.
///
/// NOTE: this canonical fixture describes hued labels in the ratified
/// `text-anchor + floor + hue` form (#148 M1) — the TARGET passport style.
const LABUI_JSON: &str = include_str!("data/labui.config.json");

/// Snapshot of the passport labui ships in PRODUCTION
/// (`labui/packages/colors/labui.config.json`). Its VOCABULARY tracks the
/// dictionary canon (labui#92 — the `icon` role is gone → alias to
/// label-tertiary, `border-ghost`→`border-none`), but its label RECIPES are
/// DELIBERATELY kept in `ladder position label-*` form, so the M1 text-anchor
/// branches stay dormant on this path — that is how it differs from the
/// canonical `.json`. Keeping BOTH fixtures under parity closes the class "wasm
/// tests exercise a recipe style production never takes": every recipe style a
/// real consumer uses must hold parity. Refresh on passport changes.
const LABUI_PROD_JSON: &str = include_str!("data/labui.config.prod.json");

/// Build the core role table for a passport through the same public path
/// `loadConfig` uses, so the parity oracle is the core's own compile, not a
/// parallel copy.
fn core_table(passport: &str) -> NamedRoleTable {
    let dto: ConfigDto = serde_json::from_str(passport).expect("passport parses");
    let cfg = ThemeConfig::try_from(dto).expect("DTO → ThemeConfig");
    cfg.compile_named_role_table().expect("passport compiles")
}

/// Build the core labui role table (canonical fixture).
fn core_labui_table() -> NamedRoleTable {
    core_table(LABUI_JSON)
}

/// A boundary engine with the given passport loaded.
fn boundary_with(passport: &str) -> LabColors {
    let mut engine = LabColors::new();
    engine.load_config(passport).expect("passport loads");
    engine
}

/// A boundary engine with the canonical labui passport loaded.
fn boundary_with_labui() -> LabColors {
    boundary_with(LABUI_JSON)
}

/// Read a string property off a JS object, panicking with context on absence —
/// this is test scaffolding, where a missing field IS the failure.
fn get_str(obj: &JsValue, key: &str) -> Option<String> {
    js_sys::Reflect::get(obj, &JsValue::from_str(key))
        .ok()
        .and_then(|v| v.as_string())
}

fn get_obj(obj: &JsValue, key: &str) -> JsValue {
    js_sys::Reflect::get(obj, &JsValue::from_str(key)).expect("property present")
}

fn get_num(obj: &JsValue, key: &str) -> f64 {
    get_obj(obj, key)
        .as_f64()
        .unwrap_or_else(|| panic!("{key} must be a number"))
}

fn get_bool(obj: &JsValue, key: &str) -> bool {
    get_obj(obj, key)
        .as_bool()
        .unwrap_or_else(|| panic!("{key} must be a boolean"))
}

/// Read the `message` of a rejected `JsError`. A `JsError` crosses as a JS
/// `Error` object, so the human text (carrying our stable code) is its
/// `.message` property, not the value's own string form.
fn error_message(err: wasm_bindgen::JsError) -> String {
    let value: JsValue = err.into();
    // A missing `.message` is itself a failure — the boundary contract is that
    // every rejected call carries one. Panic rather than mask it with "".
    get_str(&value, "message").expect("JsError must carry a .message property")
}

fn approx_pack_number(actual: f64, expected: f64, context: &str) {
    assert!(
        (actual - expected).abs() <= DRIFT_TOL,
        "{context}: {actual} != {expected} within {DRIFT_TOL}"
    );
}

fn assert_hex_within_one(actual: &str, expected: &str, context: &str) {
    let channels = |hex: &str| {
        [1, 3, 5].map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
    };
    for (actual, expected) in channels(actual).into_iter().zip(channels(expected)) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "{context}: {actual} differs from {expected} by more than one LSB"
        );
    }
}

/// Every manifest-declared committed family is replayed inside the actual
/// wasm32 runtime.
/// Same-runtime core/boundary parity alone cannot detect a platform-specific
/// libm drift, so this anchors wasm32 independently to committed bytes/values.
#[wasm_bindgen_test]
fn committed_conformance_pack_replays_in_wasm32() {
    let fresh = Pack::generate().expect("canonical pack generation");
    let contrasts: Vec<ContrastVector> =
        serde_json::from_str(include_str!("../../../conformance/vectors/contrasts.json")).unwrap();
    assert_eq!(contrasts.len(), fresh.contrasts.len());
    for (committed, actual) in contrasts.iter().zip(&fresh.contrasts) {
        assert_eq!(
            (&committed.fg, &committed.bg, &committed.theme),
            (&actual.fg, &actual.bg, &actual.theme)
        );
        approx_pack_number(actual.lc, committed.lc, "contrast lc");
        approx_pack_number(actual.wcag_ratio, committed.wcag_ratio, "contrast wcag");
    }

    let ladders: Vec<LadderVector> =
        serde_json::from_str(include_str!("../../../conformance/vectors/ladders.json")).unwrap();
    assert_eq!(ladders.len(), fresh.ladders.len());
    for (committed, actual) in ladders.iter().zip(&fresh.ladders) {
        assert_eq!(committed.position, actual.position);
        approx_pack_number(actual.alpha_light, committed.alpha_light, "ladder light");
        approx_pack_number(actual.alpha_dark, committed.alpha_dark, "ladder dark");
    }

    let alpha: Vec<AlphaVector> =
        serde_json::from_str(include_str!("../../../conformance/vectors/alpha.json")).unwrap();
    assert_eq!(alpha.len(), fresh.alpha.len());
    for (committed, actual) in alpha.iter().zip(&fresh.alpha) {
        assert_eq!((&committed.tint, &committed.bg), (&actual.tint, &actual.bg));
        approx_pack_number(actual.alpha, committed.alpha, "alpha input");
        assert_eq!(actual.composite, committed.composite);
        approx_pack_number(actual.min_alpha, committed.min_alpha, "alpha minimum");
    }

    let solve: Vec<SolveVector> =
        serde_json::from_str(include_str!("../../../conformance/vectors/solve.json")).unwrap();
    assert_eq!(solve.len(), fresh.solve.len());
    for (committed, actual) in solve.iter().zip(&fresh.solve) {
        assert_eq!(
            (&committed.bg, committed.contract, &committed.theme),
            (&actual.bg, actual.contract, &actual.theme)
        );
        match (&committed.outcome, &actual.outcome) {
            (
                SolveOutcome::Solved {
                    hex: committed_hex,
                    lc: committed_lc,
                    wcag_ratio: committed_wcag,
                    floor_override: committed_floor,
                },
                SolveOutcome::Solved {
                    hex: actual_hex,
                    lc: actual_lc,
                    wcag_ratio: actual_wcag,
                    floor_override: actual_floor,
                },
            ) => {
                assert_hex_within_one(actual_hex, committed_hex, "solve hex");
                approx_pack_number(*actual_lc, *committed_lc, "solve lc");
                approx_pack_number(*actual_wcag, *committed_wcag, "solve wcag");
                assert_eq!(actual_floor, committed_floor);
            }
            (
                SolveOutcome::Failure {
                    category: committed_category,
                    code: committed_code,
                },
                SolveOutcome::Failure {
                    category: actual_category,
                    code: actual_code,
                },
            ) => {
                assert_eq!(actual_category, committed_category);
                assert_eq!(actual_code, committed_code);
            }
            pair => panic!("solve outcome class drift: {pair:?}"),
        }
    }

    let muddiness: Vec<MuddinessVector> =
        serde_json::from_str(include_str!("../../../conformance/vectors/muddiness.json")).unwrap();
    assert_eq!(muddiness.len(), fresh.muddiness.len());
    for (committed, actual) in muddiness.iter().zip(&fresh.muddiness) {
        assert_eq!(actual.hex, committed.hex);
        approx_pack_number(actual.score, committed.score, "muddiness");
    }

    let wcag22: Vec<Wcag22Vector> =
        serde_json::from_str(include_str!("../../../conformance/vectors/wcag22.json")).unwrap();
    assert_eq!(wcag22.len(), fresh.wcag22.len());
    for (committed, actual) in wcag22.iter().zip(&fresh.wcag22) {
        assert_eq!(actual, committed, "WCAG22 finite assessment drift");
    }

    let committed_manifest: Manifest =
        serde_json::from_str(include_str!("../../../conformance/vectors/manifest.json")).unwrap();
    let fresh_manifest = fresh.manifest();
    assert_eq!(committed_manifest.pack_version, fresh_manifest.pack_version);
    assert_eq!(committed_manifest.core_version, fresh_manifest.core_version);
    assert_eq!(committed_manifest.counts, fresh_manifest.counts);
    assert_eq!(
        committed_manifest.numerical_capabilities,
        fresh_manifest.numerical_capabilities
    );
    let replayed_total = contrasts.len()
        + ladders.len()
        + alpha.len()
        + solve.len()
        + muddiness.len()
        + wcag22.len();
    assert_eq!(
        committed_manifest.counts.total, replayed_total,
        "manifest total must equal every replayed committed family"
    );
}

/// The public WASM compatibility proxy is wired to the same frozen vectors as
/// every other delivery surface. The committed file is the oracle: this test
/// intentionally carries no second hand-written semantic threshold or score.
#[wasm_bindgen_test]
fn public_muddiness_binding_matches_committed_conformance_vectors() {
    let committed: Vec<MuddinessVector> =
        serde_json::from_str(include_str!("../../../conformance/vectors/muddiness.json")).unwrap();
    let manifest: Manifest =
        serde_json::from_str(include_str!("../../../conformance/vectors/manifest.json")).unwrap();
    assert!(
        manifest.counts.muddiness > 0,
        "compatibility corpus must be non-vacuous"
    );
    assert_eq!(committed.len(), manifest.counts.muddiness);

    let colors = LabColors::new();
    for vector in committed {
        let actual = colors
            .muddiness(&vector.hex)
            .unwrap_or_else(|error| panic!("{} must remain accepted: {error:?}", vector.hex));
        approx_pack_number(
            actual,
            vector.score,
            &format!("public muddiness {}", vector.hex),
        );
    }

    assert!(
        colors.muddiness("not_a_hex").is_err(),
        "invalid public input must reject rather than panic or fabricate a score"
    );
}

/// Shared parity assertion: for a passport, the binding's `resolveTheme`
/// must reproduce the core `resolve_named_set`, role for role. Expectations
/// come straight from the core inside the same wasm runtime — never hand-typed.
/// The core side derives its ViewingConditions from the SAME enum the
/// boundary resolves through (`Theme::viewing_conditions()`): a hardcoded
/// `srgb()` here silently diverges on any non-srgb theme (dark = dim surround)
/// — exactly the miss that kept the old light-only test blind to dim parity.
/// (String→Theme mapping is the boundary parser's contract, covered by its own
/// unit tests; the literals here mirror it 1:1.)
fn theme_vc(theme: &str) -> ViewingConditions {
    let t = match theme {
        "light" => labcolors_core::Theme::Light,
        "dark" => labcolors_core::Theme::Dark,
        "light-ic" => labcolors_core::Theme::LightIc,
        "dark-ic" => labcolors_core::Theme::DarkIc,
        other => panic!("test scaffolding: unmapped theme literal {other}"),
    };
    t.viewing_conditions()
}

fn assert_parity(passport: &str, bg_hex: &str, theme: &str) {
    let bg = BgInput::solid(bg_hex).expect("bg is valid");
    let table = core_table(passport);
    let vc = theme_vc(theme);
    let core_resolved =
        resolve_named_set(&bg, &table, &vc).expect("valid parity table resolves atomically");

    // The binding result for the same inputs, from a loaded config.
    let engine = boundary_with(passport);
    let result: JsValue = engine
        .resolve_theme(bg_hex, theme)
        .expect("bg/theme resolves")
        .into();
    let roles = get_obj(&result, "roles");

    assert_eq!(
        get_str(&result, "theme").as_deref(),
        Some(theme),
        "theme echoed back"
    );

    for (name, resolved) in &core_resolved {
        let entry = get_obj(&roles, name);
        let kind = get_str(&entry, "kind").expect("every role has a kind");
        match resolved {
            Resolved::Color { solved, .. } => {
                assert_eq!(kind, "color", "{name} should be a colour");
                assert_eq!(
                    get_str(&entry, "hex").as_deref(),
                    Some(solved.hex()),
                    "{name} hex must match core"
                );
            }
            Resolved::None => {
                assert_eq!(kind, "none", "{name} should be the zero token");
            }
            Resolved::Failure(failure) => {
                assert_eq!(kind, "failure", "{name} should be a typed failure");
                assert_eq!(
                    get_str(&entry, "category").as_deref(),
                    Some(failure.category().as_str()),
                    "{name} failure category must come from core"
                );
                assert_eq!(
                    get_str(&entry, "code").as_deref(),
                    Some(failure.code()),
                    "{name} failure code must come from core"
                );
                let message = failure.to_string();
                assert_eq!(
                    get_str(&entry, "message").as_deref(),
                    Some(message.as_str()),
                    "{name} failure explanation must be preserved"
                );
            }
            Resolved::Translucent(r) => {
                assert_eq!(kind, "translucent", "{name} should be translucent");
                assert_eq!(
                    get_str(&entry, "tintHex").as_deref(),
                    Some(r.tint_hex()),
                    "{name} tint must match core"
                );
            }
            Resolved::Glow(g) => {
                assert_eq!(kind, "glow", "{name} should be a glow");
                assert_eq!(
                    get_str(&entry, "coreHex").as_deref(),
                    Some(g.core_hex()),
                    "{name} glow core must match core"
                );
                assert_eq!(
                    get_str(&entry, "haloHex").as_deref(),
                    Some(g.halo_hex()),
                    "{name} glow halo must match core"
                );
                assert_eq!(get_num(&entry, "alpha").to_bits(), g.alpha().to_bits());
                assert_eq!(get_str(&entry, "alphaCss").as_deref(), Some(g.alpha_css()));
                assert_eq!(
                    get_str(&entry, "compositeProfile").as_deref(),
                    Some(g.composite_profile().key())
                );
                assert_eq!(
                    get_str(&entry, "compositeGuarantee").as_deref(),
                    Some(g.halo_composite_certificate().guarantee().key())
                );
                assert_eq!(
                    get_str(&entry, "layerRecipeProfile").as_deref(),
                    Some(g.layer_recipe_profile().key())
                );
                assert_eq!(
                    get_str(&entry, "appearanceDiagnosticProfile").as_deref(),
                    Some(g.appearance_diagnostic_profile().key())
                );
                assert_eq!(
                    get_str(&entry, "selectionDiagnosticProfile").as_deref(),
                    g.selection_diagnostic_profile()
                        .map(|profile| profile.key())
                );
                assert_eq!(
                    get_str(&entry, "decisionProfile").as_deref(),
                    Some(g.decision_profile().key())
                );
                // Атомарный исход (#292): wire-ключ guarantee проецируется из
                // decision_outcome(); интервального determinate-варианта больше
                // не существует (интервал живёт только в Indeterminate).
                let decision_guarantee = get_obj(&entry, "decisionGuarantee");
                assert_eq!(
                    get_str(&decision_guarantee, "kind").as_deref(),
                    Some(g.decision_outcome().guarantee_wire_key())
                );
                assert_eq!(
                    get_str(&entry, "constraintLayer").as_deref(),
                    Some(g.constraint_layer().key())
                );
                assert_eq!(
                    get_num(&entry, "targetDj").to_bits(),
                    g.target_dj().to_bits()
                );
                assert_eq!(
                    get_str(&entry, "targetStatus").as_deref(),
                    Some(g.target_status().key())
                );
                assert_eq!(
                    get_str(&entry, "haloCompositeHex").as_deref(),
                    Some(g.halo_composite_hex())
                );
                assert_eq!(
                    get_num(&entry, "haloAchievedDj").to_bits(),
                    g.halo_achieved_dj().to_bits()
                );
                assert_eq!(
                    get_str(&entry, "coreCompositeHex").as_deref(),
                    Some(g.core_composite_hex())
                );
                assert_eq!(
                    get_num(&entry, "coreAchievedDj").to_bits(),
                    g.core_achieved_dj().to_bits()
                );
                assert_eq!(
                    get_num(&entry, "achievedDj").to_bits(),
                    g.halo_achieved_dj().to_bits()
                );
                assert_eq!(
                    get_bool(&entry, "degraded"),
                    matches!(
                        g.target_status(),
                        labcolors_core::GlowTargetStatus::ExactNoopUnreachable
                            | labcolors_core::GlowTargetStatus::LegacyUnreachable
                    )
                );
            }
            // `Resolved` is `#[non_exhaustive]`: a future core variant must be
            // re-accounted here loudly, never masked.
            other => panic!("unmapped Resolved variant in wasm parity ({name}): {other:?}"),
        }
    }
}

/// Canonical labui passport (target M1 text-anchor style), white/light.
#[wasm_bindgen_test]
fn resolve_theme_matches_core_named_resolve() {
    assert_parity(LABUI_JSON, "#FFFFFF", "light");
}

/// PRODUCTION passport snapshot (ladder-style hued labels) — the exact recipe
/// path labui takes today must hold parity on both of its theme anchors.
/// Guards the class: "a recipe style a real consumer uses is untested in wasm".
#[wasm_bindgen_test]
fn resolve_theme_matches_core_on_prod_passport() {
    assert_parity(LABUI_PROD_JSON, "#FFFFFF", "light");
    assert_parity(LABUI_PROD_JSON, "#101012", "dark");
}

/// Reachable roles are mirrored into `vars` under their `--lab-` CSS name, and
/// the value there equals the role's css (oklch) — what css-injection consumes.
#[wasm_bindgen_test]
fn vars_mirror_reachable_roles_in_oklch() {
    let engine = boundary_with_labui();
    let result: JsValue = engine
        .resolve_theme("#FFFFFF", "light")
        .expect("resolves")
        .into();
    let vars = get_obj(&result, "vars");
    let roles = get_obj(&result, "roles");

    // label-primary is reachable on white; its var must equal the role's css
    // and carry the ONE emission form — oklch (hex stays a data field).
    let tp = get_obj(&roles, "label-primary");
    let tp_css = get_str(&tp, "css").expect("primary carries css");
    assert_eq!(
        get_str(&vars, "--lab-label-primary"),
        Some(tp_css.clone()),
        "vars must mirror the role css under the --lab- name"
    );
    // Строгая форма: ровно три компоненты, процент ТОЛЬКО у L, без альфы.
    let inner = tp_css
        .strip_prefix("oklch(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or_else(|| panic!("solid css must be oklch(...), got {tp_css}"));
    let parts: Vec<&str> = inner.split_whitespace().collect();
    assert_eq!(parts.len(), 3, "solid oklch has exactly L C H: {tp_css}");
    assert!(
        parts[0].ends_with('%') && parts[0].trim_end_matches('%').parse::<f64>().is_ok(),
        "L is a percentage: {tp_css}"
    );
    assert!(
        parts[1].parse::<f64>().is_ok() && parts[2].parse::<f64>().is_ok(),
        "C and H are bare numbers: {tp_css}"
    );
    assert!(
        get_str(&tp, "hex").is_some_and(|h| h.starts_with('#')),
        "hex stays as a data field"
    );
}

/// `recheckContrast` across the wasm boundary: the returned `Float64Array`
/// reproduces the core `resolve_named_set`'s own `(lc, wcag)` per role, accepts
/// the same shorthand hex forms as `resolveTheme`, and rejects a bad foreground.
#[wasm_bindgen_test]
fn recheck_contrast_boundary_matches_resolve_and_shares_hex_contract() {
    let bg = "#FFFFFF";
    let core_resolved = resolve_named_set(
        &BgInput::solid(bg).expect("white is valid"),
        &core_labui_table(),
        &ViewingConditions::srgb(),
    )
    .expect("valid recheck parity table resolves atomically");
    let mut fgs: Vec<String> = Vec::new();
    let mut want: Vec<(f64, f64)> = Vec::new();
    for (_name, resolved) in &core_resolved {
        if let Resolved::Color { solved, .. } = resolved {
            fgs.push(solved.hex().to_string());
            want.push((solved.lc(), solved.wcag_ratio()));
        }
    }

    let engine = boundary_with_labui();
    let flat = engine
        .recheck_contrast(bg, fgs.clone(), "light")
        .expect("rechecks");
    assert_eq!(
        flat.len(),
        want.len() * 2,
        "one (lc, wcag) pair per foreground"
    );
    for (i, (lc, wcag)) in want.iter().enumerate() {
        assert!(
            (flat[2 * i] - lc).abs() < 1e-9,
            "role {i} lc drift at the boundary"
        );
        assert!(
            (flat[2 * i + 1] - wcag).abs() < 1e-9,
            "role {i} wcag drift at the boundary"
        );
    }

    // Shorthand / missing-`#` foregrounds are accepted, identical to canonical —
    // the same hex contract `resolveTheme` honours (`#123` == `#112233`). recheck
    // is stateless, so no config is needed for this half.
    let canonical = engine
        .recheck_contrast(bg, vec!["#112233".to_string()], "light")
        .expect("canonical rechecks");
    for fg in ["#123", "112233"] {
        let got = engine
            .recheck_contrast(bg, vec![fg.to_string()], "light")
            .expect("shorthand rechecks");
        assert_eq!(got, canonical, "{fg}: must match the canonical spelling");
    }

    // A malformed foreground rejects with the stable code, never a panic.
    let err = engine
        .recheck_contrast(bg, vec!["zzz".to_string()], "light")
        .map(|_| ())
        .expect_err("garbage foreground rejects");
    assert!(
        error_message(err).contains("invalid_background"),
        "bad foreground must carry the stable code"
    );
}

#[wasm_bindgen_test]
fn stable_glow_exact_noop_recheck_crosses_the_wasm_boundary() {
    let engine = LabColors::new();
    assert!(
        engine
            .is_stable_glow_point_noop("#010000", "#FE0000")
            .expect("sub-LSB endpoint is exact")
    );
    assert!(
        engine
            .is_stable_glow_point_noop("#001", "fff")
            .expect("#RGB and bare spelling share resolveTheme normalization")
    );
    assert!(
        !engine
            .is_stable_glow_point_noop("#800000", "#FE0000")
            .expect("first crossing endpoint is non-noop")
    );

    let tint_error = engine
        .is_stable_glow_point_noop("xyz", "#FFFFFF")
        .expect_err("invalid tint rejects");
    assert!(error_message(tint_error).contains("invalid_color"));
    let background_error = engine
        .is_stable_glow_point_noop("#000000", "bad-background")
        .expect_err("invalid background rejects");
    assert!(error_message(background_error).contains("invalid_background"));
}

#[wasm_bindgen_test]
fn stable_glow_both_lawful_outcomes_cross_the_wasm_boundary() {
    let stable_json = LABUI_JSON.replacen("legacy-platform-dependent-v1", "stable-v1", 1);
    let engine = boundary_with(&stable_json);

    let white: JsValue = engine
        .resolve_theme("#FFFFFF", "light")
        .expect("exact no-op resolves")
        .into();
    let white_roles = get_obj(&white, "roles");
    let white_vars = get_obj(&white, "vars");
    let glow = get_obj(&white_roles, "fx-glow-brand");
    assert_eq!(get_str(&glow, "kind").as_deref(), Some("glow"));
    assert_eq!(
        get_str(&glow, "decisionProfile").as_deref(),
        Some("stable-v1")
    );
    assert_eq!(
        get_str(&glow, "layerRecipeProfile").as_deref(),
        Some("cam16-jprime-oklab-cusp-v1")
    );
    assert_eq!(
        get_str(&glow, "appearanceDiagnosticProfile").as_deref(),
        Some("cam16-ucs-jprime-li2017-v1"),
        "full resolved Glow must identify its core appearance measurement"
    );
    assert_eq!(
        get_str(&glow, "selectionDiagnosticProfile"),
        None,
        "exact no-op must not claim a selection diagnostic"
    );
    assert_eq!(
        get_str(&glow, "targetStatus").as_deref(),
        Some("exact-noop-unreachable")
    );
    let guarantee = get_obj(&glow, "decisionGuarantee");
    assert_eq!(get_str(&guarantee, "kind").as_deref(), Some("bit-exact"));
    for key in [
        "--lab-fx-glow-brand",
        "--lab-fx-glow-brand-core",
        "--lab-fx-glow-brand-alpha",
    ] {
        assert!(get_str(&white_vars, key).is_some(), "missing {key}");
    }

    let dark: JsValue = engine
        .resolve_theme("#101012", "dark")
        .expect("nontrivial stable point is a terminal outcome")
        .into();
    let dark_roles = get_obj(&dark, "roles");
    let dark_vars = get_obj(&dark, "vars");
    let indeterminate = get_obj(&dark_roles, "fx-glow-brand");
    assert_eq!(
        get_str(&indeterminate, "kind").as_deref(),
        Some("glow-indeterminate")
    );
    assert_eq!(
        get_str(&indeterminate, "reason").as_deref(),
        Some("sound-bound-unavailable")
    );
    for key in [
        "--lab-fx-glow-brand",
        "--lab-fx-glow-brand-core",
        "--lab-fx-glow-brand-alpha",
    ] {
        assert!(get_str(&dark_vars, key).is_none(), "unsafe {key}");
    }
}

/// `resolveTheme` before any `loadConfig` rejects with the stable
/// `config_required` code — the agnostic contract, surfaced across the boundary.
#[wasm_bindgen_test]
fn resolve_without_config_rejects_config_required() {
    let engine = LabColors::new();
    let err = engine
        .resolve_theme("#FFFFFF", "light")
        .map(|_| ())
        .expect_err("resolve before loadConfig must reject");
    let message = error_message(err);
    assert!(
        message.contains("config_required"),
        "error must carry the stable code, got: {message}"
    );
}

/// An unknown theme name rejects with a structured error — not a panic. Theme
/// parsing happens before the config check, so this holds with no config loaded.
#[wasm_bindgen_test]
fn unknown_theme_rejects_without_panic() {
    let engine = LabColors::new();
    // `JsResolvedTheme` is not `Debug`, so map the Ok arm away before unwrapping
    // the error — we only care that the call rejected and why.
    let err = engine
        .resolve_theme("#FFFFFF", "__not_a_theme__")
        .map(|_| ())
        .expect_err("unrecognised theme must reject");
    // The error message carries the stable code.
    let message = error_message(err);
    assert!(
        message.contains("unknown_theme"),
        "error must carry the stable code, got: {message}"
    );
}

/// A malformed background rejects with the invalid-background code. Hex
/// normalisation happens before the config check, so this holds with no config.
#[wasm_bindgen_test]
fn invalid_background_rejects() {
    let engine = LabColors::new();
    let err = engine
        .resolve_theme("not-a-hex", "light")
        .map(|_| ())
        .expect_err("garbage hex rejects");
    let message = error_message(err);
    assert!(
        message.contains("invalid_background"),
        "error must carry the stable code, got: {message}"
    );
}

/// A real core failure crosses as the single public role shape. The fixture is
/// deliberately solvable as a request but impossible on this background, so
/// the test cannot be satisfied by config validation or a synthetic DTO.
#[wasm_bindgen_test]
fn typed_role_failure_crosses_with_core_category_and_no_css_value() {
    let mut config: serde_json::Value =
        serde_json::from_str(LABUI_JSON).expect("canonical passport parses");
    config["roles"]
        .as_array_mut()
        .expect("roles is an array")
        .push(serde_json::json!({
            "name": "failure-probe",
            "recipe": {"kind": "decorative-lc", "magnitude": 50.0}
        }));
    let json = serde_json::to_string(&config).expect("extended passport serializes");
    let engine = boundary_with(&json);
    let result: JsValue = engine
        .resolve_theme("#808080", "light")
        .expect("per-role physical failure is a successful resolve")
        .into();
    let roles = get_obj(&result, "roles");
    let vars = get_obj(&result, "vars");
    let failure = get_obj(&roles, "failure-probe");

    assert_eq!(get_str(&failure, "kind").as_deref(), Some("failure"));
    assert_eq!(
        get_str(&failure, "category").as_deref(),
        Some("unreachable")
    );
    assert_eq!(get_str(&failure, "code").as_deref(), Some("exceeds_range"));
    assert!(
        get_str(&failure, "message").is_some_and(|message| message.contains("target Lc 50.00"))
    );
    assert_eq!(
        get_str(&vars, "--lab-failure-probe"),
        None,
        "failure must not fabricate a CSS fallback"
    );
}

/// Смоук границы конфига в живом wasm-рантайме: два РАЗНЫХ конфига дают разные
/// отпечатки, разные пространства ключей и разные эмиссии; полупрозрачная роль лестницы
/// доходит до JS-объекта с готовой css-строкой.
#[wasm_bindgen_test]
fn config_boundary_two_configs_diverge() {
    let acme = r##"{
      "brand": {"light": "#7C3AED", "dark": "#8B5CF6", "light_ic": "#5B21B6", "dark_ic": "#A78BFA"},
      "neutral": {
        "anchors": {"light": "#FFFFFF", "mid": "#7A7A82", "dark": "#17171A"},
        "tint": {"ratio": 0.1, "target_mp": 6.1, "hue_stiffness": 9.0}
      },
      "palette": [],
      "sentiments": {"categories": [], "hardness": 5.0, "chroma_fraction": 0.88},
      "themes": [{"name": "light", "preset": "srgb"}],
      "roles": [
        {"name": "accent-fill", "recipe": {"kind": "ladder", "source": {"kind": "brand"}, "position": "fill-primary"}},
        {"name": "body-text", "recipe": {"kind": "text-anchor", "fraction": 0.62, "floor": "aa-text"}}
      ]
    }"##;
    // Второй клиент: тот же контракт имён, другой бренд → другая эмиссия.
    let other = acme.replace("#7C3AED", "#0E7490");

    let mut colors = LabColors::new();
    let fp_a = colors.load_config(acme).expect("acme валиден");
    let set_a = colors.resolve_theme("#FFFFFF", "light").expect("резолв");
    let fp_b = colors.load_config(&other).expect("вариант валиден");
    let set_b = colors.resolve_theme("#FFFFFF", "light").expect("резолв");

    assert_ne!(fp_a, fp_b, "разные конфиги → разные отпечатки");

    let roles_a = get_obj(set_a.as_ref(), "roles");
    let accent_a = get_obj(&roles_a, "accent-fill");
    assert_eq!(get_str(&accent_a, "kind").as_deref(), Some("translucent"));
    let css_a = get_str(&accent_a, "css").expect("rgba несёт css");
    assert!(
        css_a.starts_with("oklch(") && css_a.contains(" / "),
        "css-эмиссия rgba — oklch со слэш-альфой: {css_a}"
    );

    let roles_b = get_obj(set_b.as_ref(), "roles");
    let accent_b = get_obj(&roles_b, "accent-fill");
    let css_b = get_str(&accent_b, "css").expect("rgba несёт css");
    assert_ne!(css_a, css_b, "другой бренд → другая эмиссия той же роли");

    // Пространство ключей — конфига, не встроенной таблицы.
    let keys = js_sys::Object::keys(&roles_b.clone().into());
    let mut keys: Vec<String> = keys.iter().filter_map(|k| k.as_string()).collect();
    keys.sort();
    assert_eq!(
        keys,
        ["accent-fill", "body-text"],
        "после загрузки конфига пространство ключей — РОВНО его контракт,          без примеси встроенной таблицы"
    );

    // Невалидный конфиг — структурная ошибка invalid_config.
    let err = colors.load_config("{").expect_err("битый JSON отклонён");
    let msg = js_sys::Reflect::get(&err.into(), &JsValue::from_str("message"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    assert!(msg.contains("invalid_config"), "код в сообщении: {msg}");
}
