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
    get_str(&value, "message").unwrap_or_default()
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
        }
    }
}

/// Reachable roles are mirrored into `vars` under their `--lab-` CSS name, and
/// the hex there equals the role's hex — the contract css-injection consumes.
#[wasm_bindgen_test]
fn vars_mirror_reachable_role_hexes() {
    let engine = LabColors::new();
    let result: JsValue = engine
        .resolve_theme("#FFFFFF", "light")
        .expect("resolves")
        .into();
    let vars = get_obj(&result, "vars");
    let roles = get_obj(&result, "roles");

    // text-primary is reachable on white; its var must equal its role hex.
    let tp = get_obj(&roles, "text-primary");
    let tp_hex = get_str(&tp, "hex").expect("primary is a colour");
    assert_eq!(
        get_str(&vars, "--lab-text-primary"),
        Some(tp_hex),
        "vars must mirror the role hex under the --lab- name"
    );
}

/// An uncalibrated theme rejects with a structured error — not a panic.
#[wasm_bindgen_test]
fn uncalibrated_theme_rejects_without_panic() {
    let engine = LabColors::new();
    // `JsResolvedTheme` is not `Debug`, so map the Ok arm away before unwrapping
    // the error — we only care that the call rejected and why.
    let err = engine
        .resolve_theme("#FFFFFF", "light-ic")
        .map(|_| ())
        .expect_err("light-ic is not calibrated");
    // The error message carries the stable code.
    let message = error_message(err);
    assert!(
        message.contains("theme_not_calibrated"),
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
