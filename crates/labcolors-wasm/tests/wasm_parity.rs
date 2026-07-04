//! Headless-browser parity smoke: the binding's resolve must equal the native
//! `resolve_named_set` it wraps, role for role.
//!
//! Run with `wasm-pack test --headless --chrome` (D1 default from the chapter).
//! The engine is agnostic (ADR-0001 PR-c): it has no built-in table, so parity
//! is proven against a LOADED config (the frozen labui passport). Expectations
//! are GENERATED from the core's own `resolve_named_set` inside the same wasm
//! runtime — never hand-typed — so this test cannot drift from the engine and
//! stays correct when the role set grows.

#![cfg(target_arch = "wasm32")]

use labcolors_core::config::ThemeConfig;
use labcolors_core::semantic::NamedRoleTable;
use labcolors_core::{BgInput, Resolved, ViewingConditions, resolve_named_set};
use labcolors_wasm::LabColors;
use labcolors_wasm::config_dto::ConfigDto;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// The frozen labui passport — the config the built-in default table used to
/// hardcode. Both the boundary (via `loadConfig`) and the native expectation
/// (via `ConfigDto` → `ThemeConfig` → `compile_named_role_table`) read this one
/// SSOT, so the two sides cannot diverge on their input.
///
/// NOTE: this canonical fixture describes hued labels in the ratified
/// `text-anchor + floor + hue` form (#148 M1) — the TARGET passport style.
const LABUI_JSON: &str = include_str!("data/labui.config.json");

/// Snapshot of the passport labui ships in PRODUCTION
/// (`labui/packages/colors/labui.config.json` @ labui commit bd7b843 (#80),
/// sha256 f9bbf7e4… — point-in-time snapshot, refresh on passport changes).
/// It still describes hued labels via `ladder position label-*`, so the M1
/// text-anchor branches stay dormant on this path. Keeping BOTH fixtures under
/// parity closes the class "wasm tests exercise a recipe style production
/// never takes": every recipe style a real consumer uses must hold parity.
const LABUI_PROD_JSON: &str = include_str!("data/labui.config.prod.json");

/// Build the native role table for a passport through the same public path
/// `loadConfig` uses, so the parity oracle is the core's own compile, not a
/// parallel copy.
fn native_table(passport: &str) -> NamedRoleTable {
    let dto: ConfigDto = serde_json::from_str(passport).expect("passport parses");
    let cfg = ThemeConfig::try_from(dto).expect("DTO → ThemeConfig");
    cfg.compile_named_role_table().expect("passport compiles")
}

/// Build the native labui role table (canonical fixture).
fn native_labui_table() -> NamedRoleTable {
    native_table(LABUI_JSON)
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

/// Read the `message` of a rejected `JsError`. A `JsError` crosses as a JS
/// `Error` object, so the human text (carrying our stable code) is its
/// `.message` property, not the value's own string form.
fn error_message(err: wasm_bindgen::JsError) -> String {
    let value: JsValue = err.into();
    // A missing `.message` is itself a failure — the boundary contract is that
    // every rejected call carries one. Panic rather than mask it with "".
    get_str(&value, "message").expect("JsError must carry a .message property")
}

/// Shared parity assertion: for a passport, the binding's `resolveTheme`
/// must reproduce the native `resolve_named_set`, role for role. Expectations
/// come straight from the core inside the same wasm runtime — never hand-typed.
/// The native side derives its ViewingConditions from the SAME core enum the
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
    let table = native_table(passport);
    let vc = theme_vc(theme);
    let native = resolve_named_set(&bg, &table, &vc);

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

    for (name, resolved) in &native {
        let entry = get_obj(&roles, name);
        let kind = get_str(&entry, "kind").expect("every role has a kind");
        match resolved {
            Resolved::Color { solved, .. } => {
                assert_eq!(kind, "color", "{name} should be a colour");
                assert_eq!(
                    get_str(&entry, "hex").as_deref(),
                    Some(solved.hex()),
                    "{name} hex must match native"
                );
            }
            Resolved::None => {
                assert_eq!(kind, "none", "{name} should be the zero token");
            }
            Resolved::Unreachable(_) => {
                assert_eq!(kind, "unreachable", "{name} should be unreachable");
            }
            Resolved::Translucent(r) => {
                assert_eq!(kind, "translucent", "{name} should be translucent");
                assert_eq!(
                    get_str(&entry, "tintHex").as_deref(),
                    Some(r.tint_hex()),
                    "{name} tint must match native"
                );
            }
            Resolved::Glow(g) => {
                assert_eq!(kind, "glow", "{name} should be a glow");
                assert_eq!(
                    get_str(&entry, "coreHex").as_deref(),
                    Some(g.core_hex()),
                    "{name} glow core must match native"
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
fn resolve_theme_matches_native_named_resolve() {
    assert_parity(LABUI_JSON, "#FFFFFF", "light");
}

/// PRODUCTION passport snapshot (ladder-style hued labels) — the exact recipe
/// path labui takes today must hold parity on both of its theme anchors.
/// Guards the class: "a recipe style a real consumer uses is untested in wasm".
#[wasm_bindgen_test]
fn resolve_theme_matches_native_on_prod_passport() {
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
/// reproduces the native `resolve_named_set`'s own `(lc, wcag)` per role, accepts
/// the same shorthand hex forms as `resolveTheme`, and rejects a bad foreground.
#[wasm_bindgen_test]
fn recheck_contrast_boundary_matches_resolve_and_shares_hex_contract() {
    let bg = "#FFFFFF";
    let native = resolve_named_set(
        &BgInput::solid(bg).expect("white is valid"),
        &native_labui_table(),
        &ViewingConditions::srgb(),
    );
    let mut fgs: Vec<String> = Vec::new();
    let mut want: Vec<(f64, f64)> = Vec::new();
    for (_name, resolved) in &native {
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
