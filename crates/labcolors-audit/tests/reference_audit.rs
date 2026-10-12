use labcolors_audit::{ArtifactClass, Disposition, DispositionedArtifact, RawArtifact, audit_gate};

/// RED-proof тест: синтетический сирота обнаруживается гейтом.
///
/// Инвариант: если артефакт помечен как Orphaned, audit_gate обязан
/// вернуть passed=false с ненулевым orphaned_count. Это базовый контракт
/// пайплайна — нарушение означает, что стадия gate не выполняет свою функцию.
#[test]
fn synthetic_orphan_is_detected_by_gate() {
    let orphan = RawArtifact {
        class: ArtifactClass::PublicRustApi,
        module: "synthetic::probe".into(),
        line: 1,
        raw_key: "orphan_probe_fn".into(),
        raw_value: None,
    };

    let dispositioned = vec![DispositionedArtifact {
        raw: orphan,
        disposition: Disposition::Orphaned {
            reason: "synthetic probe: no evidence in GRAPH-01".into(),
        },
        normalized_join_key: "PublicRustApi::synthetic::probe::orphan_probe_fn".into(),
    }];

    let verdict = audit_gate(&dispositioned);

    assert!(
        !verdict.passed,
        "gate must fail when orphaned artifacts exist"
    );
    assert_eq!(
        verdict.orphaned_count, 1,
        "exactly one orphan should be counted"
    );
    assert_eq!(verdict.total_artifacts, 1);
}

/// Неполная оценка настоящего дерева сохраняет найденные артефакты и явный отказ.
#[test]
fn real_tree_reports_its_unassessed_inventory() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("корень workspace");
    let (verdict, entries) = labcolors_audit::run_full_audit(root).expect("читаемое дерево");
    assert!(!entries.is_empty());
    assert_eq!(verdict.total_artifacts, entries.len());
    assert!(verdict.not_assessed_count > 0);
    assert!(!verdict.passed);
}

/// Контроль контракта: Defective тоже ломает гейт.
#[test]
fn defective_artifact_fails_gate() {
    let dispositioned = vec![DispositionedArtifact {
        raw: RawArtifact {
            class: ArtifactClass::WasmBoundary,
            module: "ffi::probe".into(),
            line: 42,
            raw_key: "broken_abi".into(),
            raw_value: Some("mismatched signature".into()),
        },
        disposition: Disposition::Defective {
            defect: "ABI mismatch between native and wasm signatures".into(),
        },
        normalized_join_key: "WasmBoundary::ffi::probe::broken_abi".into(),
    }];

    let verdict = audit_gate(&dispositioned);

    assert!(!verdict.passed);
    assert_eq!(verdict.defective_count, 1);
}

/// Неоценённый артефакт сохраняет пробел покрытия в итоговом отказе.
#[test]
fn not_assessed_prevents_a_complete_verdict() {
    let dispositioned = vec![DispositionedArtifact {
        raw: RawArtifact {
            class: ArtifactClass::SemanticBranch,
            module: "color::oklch".into(),
            line: 7,
            raw_key: "chroma_clamp".into(),
            raw_value: None,
        },
        disposition: Disposition::NotAssessed {
            reason: "evidence pending from GRAPH-01 next revision".into(),
            replan_trigger: "GRAPH-01 r6 conformance data".into(),
        },
        normalized_join_key: "SemanticBranch::color::oklch::chroma_clamp".into(),
    }];

    let verdict = audit_gate(&dispositioned);

    assert!(!verdict.passed, "NotAssessed prevents a complete verdict");
    assert_eq!(verdict.not_assessed_count, 1);
    assert_eq!(verdict.orphaned_count, 0);
    assert_eq!(verdict.defective_count, 0);
}

/// EXT-07 floor test: WasmBoundary extractor finds >= 5 #[wasm_bindgen] sites.
#[test]
fn ext07_wasm_boundary_floor() {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("audit crate has grandparent workspace root");

    let raw = labcolors_audit::enumerate_production_artifacts(source_root)
        .expect("читаемый исходный fixture");
    let wasm_count = raw
        .iter()
        .filter(|a| a.class == ArtifactClass::WasmBoundary)
        .count();

    assert!(
        wasm_count >= 5,
        "Expected >= 5 WasmBoundary artifacts (#[wasm_bindgen] sites), got {}",
        wasm_count
    );
}

/// EXT-07 floor test: NativeBoundary extractor finds >= 7 #[uniffi::export] sites.
#[test]
fn ext07_native_boundary_floor() {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("audit crate has grandparent workspace root");

    let raw = labcolors_audit::enumerate_production_artifacts(source_root)
        .expect("читаемый исходный fixture");
    let native_count = raw
        .iter()
        .filter(|a| a.class == ArtifactClass::NativeBoundary)
        .count();

    assert!(
        native_count >= 7,
        "Expected >= 7 NativeBoundary artifacts (#[uniffi::export] sites), got {}",
        native_count
    );
}

/// EXT-07 sabotage: phantom WasmBoundary artifact is detected.
#[test]
fn ext07_wasm_boundary_no_phantom() {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("audit crate has grandparent workspace root");

    let raw = labcolors_audit::enumerate_production_artifacts(source_root)
        .expect("читаемый исходный fixture");
    for artifact in raw
        .iter()
        .filter(|a| a.class == ArtifactClass::WasmBoundary)
    {
        let full_path = source_root.join(&artifact.module);
        assert!(
            full_path.exists(),
            "Phantom WasmBoundary artifact: {} does not exist",
            artifact.module
        );
    }
}

/// EXT-07 sabotage: phantom NativeBoundary artifact is detected.
#[test]
fn ext07_native_boundary_no_phantom() {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("audit crate has grandparent workspace root");

    let raw = labcolors_audit::enumerate_production_artifacts(source_root)
        .expect("читаемый исходный fixture");
    for artifact in raw
        .iter()
        .filter(|a| a.class == ArtifactClass::NativeBoundary)
    {
        let full_path = source_root.join(&artifact.module);
        assert!(
            full_path.exists(),
            "Phantom NativeBoundary artifact: {} does not exist",
            artifact.module
        );
    }
}

// ── r7-g2: ParallelSsot characterization ────────────────────────────────

/// r7-g2 floor test: ParallelSsot extractor finds >= 15 SSOT-TRACKED/GROUNDED markers.
///
/// RED-proof: if collect_parallel_ssot were stubbed or deleted, this count
/// drops to zero and the assertion fails. Baseline established from live
/// codebase scan on 2026-08-31 (22 markers found).
#[test]
fn ext_parallel_ssot_floor() {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("audit crate has grandparent workspace root");

    let raw = labcolors_audit::enumerate_production_artifacts(source_root)
        .expect("читаемый исходный fixture");
    let ssot_count = raw
        .iter()
        .filter(|a| a.class == ArtifactClass::ParallelSsot)
        .count();

    assert!(
        ssot_count >= 15,
        "Expected >= 15 ParallelSsot artifacts (SSOT-TRACKED/GROUNDED markers), got {}",
        ssot_count
    );
}

/// r7-g2 sabotage: phantom ParallelSsot artifact references nonexistent file.
///
/// Every ParallelSsot artifact's module field must point to an existing
/// source file. If someone fabricates entries with fake paths, this test
/// catches them.
#[test]
fn ext_parallel_ssot_no_phantom() {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("audit crate has grandparent workspace root");

    let raw = labcolors_audit::enumerate_production_artifacts(source_root)
        .expect("читаемый исходный fixture");
    for artifact in raw
        .iter()
        .filter(|a| a.class == ArtifactClass::ParallelSsot)
    {
        let full_path = source_root.join(&artifact.module);
        assert!(
            full_path.exists(),
            "Phantom ParallelSsot artifact: {} does not exist",
            artifact.module
        );
    }
}

/// r7-g2 sabotage: ParallelSsot raw_key format is validated.
///
/// Every ParallelSsot artifact must have raw_key matching the documented
/// format: `ssot-tracked:L{line}` or `grounded:L{line}`. Fabricated entries
/// with arbitrary keys are caught here.
#[test]
fn ext_parallel_ssot_key_format() {
    let source_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("audit crate has grandparent workspace root");

    let raw = labcolors_audit::enumerate_production_artifacts(source_root)
        .expect("читаемый исходный fixture");
    for artifact in raw
        .iter()
        .filter(|a| a.class == ArtifactClass::ParallelSsot)
    {
        let valid = artifact.raw_key.starts_with("ssot-tracked:L")
            || artifact.raw_key.starts_with("grounded:L");
        assert!(
            valid,
            "ParallelSsot raw_key '{}' does not match expected format \
             (ssot-tracked:L{{n}} or grounded:L{{n}})",
            artifact.raw_key
        );
    }
}
