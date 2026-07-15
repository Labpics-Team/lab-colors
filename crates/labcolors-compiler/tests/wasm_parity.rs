#![cfg(target_arch = "wasm32")]

use labcolors_compiler::{
    evaluate_wcag22_explicit_selection_v1, evaluate_wcag22_feasibility_v1,
    wcag22_explicit_selection_envelope_too_large_v1,
    wcag22_explicit_selection_max_request_bytes_v1, wcag22_feasibility_envelope_too_large_v1,
    wcag22_feasibility_max_request_bytes_v1,
};
use serde::Deserialize;
use wasm_bindgen::JsValue;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Vector {
    case_id: String,
    request_json: String,
    outcome_json: String,
}

fn json_text(value: &JsValue) -> String {
    js_sys::JSON::stringify(value)
        .expect("value is JSON-serializable")
        .as_string()
        .expect("JSON.stringify returns text")
}

#[wasm_bindgen_test]
fn compiler_replays_the_canonical_protocol_family_byte_exactly() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../conformance/vectors/wcag22-feasibility.json"
    ))
    .expect("canonical family parses");
    assert!(!vectors.is_empty(), "anti-vacuum: family is non-empty");

    for vector in vectors {
        let outcome: JsValue = evaluate_wcag22_feasibility_v1(vector.request_json.as_bytes())
            .expect("canonical projection")
            .into();
        assert_eq!(
            json_text(&outcome),
            vector.outcome_json,
            "{}",
            vector.case_id
        );
    }
}

#[wasm_bindgen_test]
fn compiler_rechecks_its_protocol_owned_envelope_ceiling() {
    let limit = wcag22_feasibility_max_request_bytes_v1();
    assert_eq!(u64::from(limit), labcolors_protocol::MAX_ENVELOPE_BYTES_V1);

    let raw: JsValue = evaluate_wcag22_feasibility_v1(&vec![b' '; limit as usize + 1])
        .expect("oversize failure is data")
        .into();
    let scalar: JsValue = wcag22_feasibility_envelope_too_large_v1(u64::from(limit) + 1)
        .expect("scalar oversize projection")
        .into();
    assert_eq!(json_text(&raw), json_text(&scalar));
}

#[wasm_bindgen_test]
fn compiler_replays_the_atomic_explicit_selection_family_byte_exactly() {
    let vectors: Vec<Vector> = serde_json::from_str(include_str!(
        "../../../conformance/vectors/wcag22-explicit-selection.json"
    ))
    .expect("canonical family parses");
    assert!(!vectors.is_empty(), "anti-vacuum: family is non-empty");

    for vector in vectors {
        let outcome: JsValue =
            evaluate_wcag22_explicit_selection_v1(vector.request_json.as_bytes())
                .expect("canonical projection")
                .into();
        assert_eq!(
            json_text(&outcome),
            vector.outcome_json,
            "{}",
            vector.case_id
        );
    }
}

#[wasm_bindgen_test]
fn compiler_rechecks_the_atomic_protocol_owned_envelope_ceiling() {
    let limit = wcag22_explicit_selection_max_request_bytes_v1();
    assert_eq!(
        u64::from(limit),
        labcolors_protocol::explicit_selection::MAX_EXPLICIT_SELECTION_ENVELOPE_BYTES_V1
    );

    let raw: JsValue = evaluate_wcag22_explicit_selection_v1(&vec![b' '; limit as usize + 1])
        .expect("oversize failure is data")
        .into();
    let scalar: JsValue = wcag22_explicit_selection_envelope_too_large_v1(u64::from(limit) + 1)
        .expect("scalar oversize projection")
        .into();
    assert_eq!(json_text(&raw), json_text(&scalar));
}
