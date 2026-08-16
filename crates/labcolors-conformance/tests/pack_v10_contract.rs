//! Pack-контракт: pack 10 не содержит удалённые legacy-семейства и закрепляет
//! точные текущие байты пяти канонических семейств. WCAG-family несёт текущую
//! proof lineage; это identity текущего pack, а не обещание byte-совместимости
//! с pack 9.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use labcolors_conformance::{FAMILY_FILES, MANIFEST_FILE, PACK_VERSION};

#[allow(dead_code)]
#[path = "../../labcolors-core/src/sha256.rs"]
mod sha256;

const CANONICAL_FAMILY_SHA256: [(&str, &str); 5] = [
    (
        "solve.json",
        "1b34059c1d398e3dca04e13c0333fffe71fbd26061205450d845d95510755d77",
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
        "wcag22.json",
        "836b7f90ab3807072155d8e38633cf6bab7ec6ad7a0ee436831acd8536df6db7",
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
fn pack_v10_family_inventory_and_bytes_are_exact() {
    assert_eq!(PACK_VERSION, "10.0.0");
    assert_eq!(
        FAMILY_FILES.as_slice(),
        [
            "contrasts.json",
            "ladders.json",
            "alpha.json",
            "solve.json",
            "wcag22.json",
        ]
        .as_slice()
    );

    let dir = vectors_dir();
    for (name, expected) in CANONICAL_FAMILY_SHA256 {
        assert_eq!(
            sha256::digest(&read(dir.join(name))).to_hex(),
            expected,
            "pack-10 canonical family bytes drifted without explicit regeneration: {name}"
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
    assert!(
        !dir.join("muddiness.json").exists(),
        "the muddiness family must be gone, not regenerated"
    );

    let manifest: serde_json::Value =
        serde_json::from_slice(&read(dir.join(MANIFEST_FILE))).expect("valid manifest JSON");
    assert_eq!(manifest["packVersion"], "10.0.0");
    assert!(
        manifest["counts"].get("wcag22ExplicitSelection").is_none()
            && manifest["counts"].get("wcag22Feasibility").is_none()
            && manifest["counts"].get("muddiness").is_none(),
        "manifest must not carry removed family counts"
    );
    assert_eq!(manifest["counts"]["total"], 86);
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
