//! Pack-контракт: pack 9 удаляет ровно `wcag22-feasibility.json`
//! (feasibility/protocol/compiler-линия вырезана из всех проекций; exact
//! evaluateWcag22 и Q55-доказательства сохранены), сохраняя байт-в-байт все
//! шесть оставшихся семейств pack 8.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use labcolors_conformance::{FAMILY_FILES, MANIFEST_FILE, PACK_VERSION};

#[allow(dead_code)]
#[path = "../../labcolors-core/src/sha256.rs"]
mod sha256;

const UNCHANGED_FAMILY_SHA256: [(&str, &str); 6] = [
    (
        "solve.json",
        "db04e50698cc3b10223f4005f74dd35cc5ae0a29988825e44db5c985aa9207af",
    ),
    (
        "contrasts.json",
        "57d99bb3138edba769a185af5589651ab1cd3140f92e5cf493be2f998b2f1145",
    ),
    (
        "ladders.json",
        "496f562e55ad8110aeb8a07042b1964ec9ff4d0f1e8c09e362d1b2d14c513036",
    ),
    (
        "alpha.json",
        "b9c71e26c96c977c51cb2ffc98ff8f24a24705105c1962479e72e687b1b05bb1",
    ),
    (
        "muddiness.json",
        "3c5497b251f04c089d33452b9bf0bfba7f4ef9a72dc496180ff42aad08377aa3",
    ),
    (
        "wcag22.json",
        "6e234fa3a0d4e2b21f515b8f4e6be76f223768821e0308e774c31a5ce7a1d826",
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
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.as_ref().display()))
}

#[test]
fn pack_v9_removes_only_the_feasibility_family() {
    assert_eq!(PACK_VERSION, "9.0.0");
    assert_eq!(
        FAMILY_FILES.as_slice(),
        [
            "contrasts.json",
            "ladders.json",
            "alpha.json",
            "solve.json",
            "muddiness.json",
            "wcag22.json",
        ]
        .as_slice()
    );

    let dir = vectors_dir();
    for (name, expected) in UNCHANGED_FAMILY_SHA256 {
        assert_eq!(
            sha256::digest(&read(dir.join(name))).to_hex(),
            expected,
            "pack-8 family bytes drifted during the feasibility removal: {name}"
        );
    }
    assert!(
        !dir.join("wcag22-explicit-selection.json").exists(),
        "the explicit-selection family must be gone, not regenerated"
    );
    assert!(
        !dir.join("wcag22-feasibility.json").exists(),
        "the feasibility family must be gone, not regenerated"
    );

    let manifest: serde_json::Value =
        serde_json::from_slice(&read(dir.join(MANIFEST_FILE))).expect("valid manifest JSON");
    assert_eq!(manifest["packVersion"], "9.0.0");
    assert!(
        manifest["counts"].get("wcag22ExplicitSelection").is_none()
            && manifest["counts"].get("wcag22Feasibility").is_none(),
        "manifest must not carry removed family counts"
    );
    assert_eq!(manifest["counts"]["total"], 90);
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
            ("unreachable", "floor_unreachable"),
        ])
    );
    assert!(vectors.iter().all(|vector| matches!(
        vector["outcome"]["kind"].as_str(),
        Some("solved" | "failure")
    )));
}
