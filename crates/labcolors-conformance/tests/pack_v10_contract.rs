//! Pack 11 terminal C7c contract: recipe-ladder family and floorOverride
//! projection are absent; four permanent canonical families remain byte-pinned.

use labcolors_conformance::{FAMILY_FILES, MANIFEST_FILE, PACK_VERSION};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
#[allow(dead_code)]
#[path = "../../labcolors-core/src/sha256.rs"]
mod sha256;

const CANONICAL_FAMILY_SHA256: [(&str, &str); 4] = [
    (
        "contrasts.json",
        "57d99bb3138edba769a185af5589651ab1cd3140f92e5cf493be2f998b2f1145",
    ),
    (
        "alpha.json",
        "b9c71e26c96c977c51cb2ffc98ff8f24a24705105c1962479e72e687b1b05bb1",
    ),
    (
        "solve.json",
        "09cb198d63cc079384a4fdc5d6ae236f510e32d12214c97a063ca7a5d2f7dcf9",
    ),
    (
        "wcag22.json",
        "1fb5aff4",
    ),
];
fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("conformance")
        .join("vectors")
}
fn read(path: impl AsRef<Path>) -> Vec<u8> {
    std::fs::read(path.as_ref())
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.as_ref().display()))
}

#[test]
fn pack_v11_family_inventory_and_bytes_are_exact() {
    assert_eq!(PACK_VERSION, "11.0.0");
    assert_eq!(
        FAMILY_FILES.as_slice(),
        ["contrasts.json", "alpha.json", "solve.json", "wcag22.json"].as_slice()
    );
    let dir = vectors_dir();
    for (name, expected) in CANONICAL_FAMILY_SHA256 {
        assert_eq!(
            sha256::digest(&read(dir.join(name))).to_hex(),
            expected,
            "pack-11 bytes drifted: {name}"
        );
    }
    for removed in [
        "ladders.json",
        "wcag22-explicit-selection.json",
        "wcag22-feasibility.json",
        "muddiness.json",
    ] {
        assert!(
            !dir.join(removed).exists(),
            "removed family returned: {removed}"
        );
    }
    let manifest: serde_json::Value =
        serde_json::from_slice(&read(dir.join(MANIFEST_FILE))).expect("valid manifest");
    assert_eq!(manifest["packVersion"], "11.0.0");
    assert_eq!(manifest["counts"]["total"], 61);
    assert!(manifest["counts"].get("ladders").is_none());
}

#[test]
fn solve_failure_wire_is_exact_and_closed() {
    let vectors: Vec<serde_json::Value> =
        serde_json::from_slice(&read(vectors_dir().join("solve.json")))
            .expect("valid solve family JSON");
    let failures: Vec<_> = vectors
        .iter()
        .filter_map(|vector| {
            let outcome = &vector["outcome"];
            (outcome["kind"] == "failure").then_some(outcome)
        })
        .collect();
    assert!(
        !failures.is_empty(),
        "anti-vacuum: solve family has no failure"
    );
    let mut actual = BTreeSet::new();
    for failure in failures {
        let fields: BTreeSet<_> = failure
            .as_object()
            .expect("failure object")
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(fields, BTreeSet::from(["category", "code", "kind"]));
        actual.insert((
            failure["category"].as_str().expect("failure category"),
            failure["code"].as_str().expect("failure code"),
        ));
    }
    assert_eq!(
        actual,
        BTreeSet::from([
            ("unreachable", "below_contrast_floor"),
            ("unreachable", "exceeds_range"),
        ])
    );
    assert!(vectors.iter().all(|vector| matches!(
        vector["outcome"]["kind"].as_str(),
        Some("solved" | "failure")
    )));
}
