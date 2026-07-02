//! Headless-browser parity smoke: the binding's resolve must equal the native
//! `resolve_set` it wraps, role for role.
//!
//! Run with `wasm-pack test --headless --chrome` (D1 default from the chapter).
//! Expectations are GENERATED from the core's own `resolve_set` inside the same
//! wasm runtime — never hand-typed — so this test cannot drift from the engine
//! and stays correct when issue #59 grows the role set.

#![cfg(target_arch = "wasm32")]

use labcolors_core::{BgInput, Resolved, RoleTable, ViewingConditions, resolve_set};
use labcolors_wasm::LabColors;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

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

/// The binding's `resolveTheme("#FFFFFF","light")` must reproduce the native
/// `resolve_set` for the default table under sRGB viewing conditions, for every
/// role the core returns.
#[wasm_bindgen_test]
fn resolve_theme_matches_native_resolve_set() {
    // Expectations straight from the core, in the wasm runtime.
    let bg = BgInput::solid("#FFFFFF").expect("white is valid");
    let table = RoleTable::default();
    let vc = ViewingConditions::srgb();
    let native = resolve_set(&bg, &table, &vc);

    // The binding result for the same inputs.
    let engine = LabColors::new();
    let result: JsValue = engine
        .resolve_theme("#FFFFFF", "light")
        .expect("white/light resolves")
        .into();
    let roles = get_obj(&result, "roles");

    assert_eq!(
        get_str(&result, "theme").as_deref(),
        Some("light"),
        "theme echoed back"
    );

    for (role, resolved) in &native {
        let entry = get_obj(&roles, role.key());
        let kind = get_str(&entry, "kind").expect("every role has a kind");
        match resolved {
            Resolved::Color { solved, .. } => {
                assert_eq!(kind, "color", "{} should be a colour", role.key());
                assert_eq!(
                    get_str(&entry, "hex").as_deref(),
                    Some(solved.hex()),
                    "{} hex must match native",
                    role.key()
                );
            }
            Resolved::None => {
                assert_eq!(kind, "none", "{} should be the zero token", role.key());
            }
            Resolved::Unreachable(_) => {
                assert_eq!(kind, "unreachable", "{} should be unreachable", role.key());
            }
            // Translucent в дефолт-таблице не встречается (полупрозрачная граница уже открыта конфиг-путём);
            // будущий вариант обязан быть переучтён здесь шумно, не замаскирован.
            other => panic!(
                "неучтённый Resolved-вариант в wasm-парити ({}): {other:?}",
                role.key()
            ),
        }
    }
}

/// Reachable roles are mirrored into `vars` under their `--lab-` CSS name, and
/// the value there equals the role's css (oklch) — what css-injection consumes.
#[wasm_bindgen_test]
fn vars_mirror_reachable_roles_in_oklch() {
    let engine = LabColors::new();
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
/// reproduces the native `resolve_set`'s own `(lc, wcag)` per role, accepts the
/// same shorthand hex forms as `resolveTheme`, and rejects a bad foreground.
#[wasm_bindgen_test]
fn recheck_contrast_boundary_matches_resolve_and_shares_hex_contract() {
    let bg = "#FFFFFF";
    let native = resolve_set(
        &BgInput::solid(bg).expect("white is valid"),
        &RoleTable::default(),
        &ViewingConditions::srgb(),
    );
    let mut fgs: Vec<String> = Vec::new();
    let mut want: Vec<(f64, f64)> = Vec::new();
    for (_role, resolved) in &native {
        if let Resolved::Color { solved, .. } = resolved {
            fgs.push(solved.hex().to_string());
            want.push((solved.lc(), solved.wcag_ratio()));
        }
    }

    let engine = LabColors::new();
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
    // the same hex contract `resolveTheme` honours (`#123` == `#112233`).
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

/// An unknown theme name rejects with a structured error — not a panic.
/// (Previously tested "light-ic" as uncalibrated; IC themes are now fully
/// calibrated since d2c9340. This test now covers the unknown-theme path,
/// which is the remaining rejection path for invalid theme strings.)
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

/// A malformed background rejects with the invalid-background code.
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
