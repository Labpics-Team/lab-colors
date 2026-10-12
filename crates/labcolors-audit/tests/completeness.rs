use labcolors_audit::{ArtifactClass, Disposition, DispositionedArtifact, RawArtifact, audit_gate};
use std::process::Command;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn assessed(disposition: Disposition) -> DispositionedArtifact {
    DispositionedArtifact {
        raw: RawArtifact {
            class: ArtifactClass::DecisionSite,
            module: "fixture/src/lib.rs".into(),
            line: 1,
            raw_key: "decision".into(),
            raw_value: None,
        },
        disposition,
        normalized_join_key: "DecisionSite::fixture/src/lib.rs::decision".into(),
    }
}

#[test]
fn an_empty_inventory_cannot_pass() {
    let verdict = audit_gate(&[]);
    assert!(!verdict.passed);
    assert_eq!(verdict.total_artifacts, 0);
}

#[test]
fn unassessed_artifacts_prevent_a_complete_verdict() {
    let covered = assessed(Disposition::Covered {
        evidence_key: "known-proof".into(),
    });
    assert!(audit_gate(std::slice::from_ref(&covered)).passed);
    let unknown = assessed(Disposition::NotAssessed {
        reason: "evidence unavailable".into(),
        replan_trigger: "source evidence becomes available".into(),
    });
    for inventory in [vec![unknown.clone()], vec![covered, unknown]] {
        let verdict = audit_gate(&inventory);
        assert!(!verdict.passed);
        assert_eq!(verdict.not_assessed_count, 1);
    }
}

#[test]
fn missing_source_root_is_an_input_failure() -> TestResult {
    let root = std::env::temp_dir().join(format!(
        "labcolors-missing-audit-root-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    assert!(!root.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_scanner"))
        .arg(root)
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    Ok(())
}

fn with_source_tree(operation: impl FnOnce(&std::path::Path) -> TestResult) -> TestResult {
    let root = std::env::temp_dir().join(format!(
        "labcolors-audit-input-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    std::fs::create_dir(&root)?;
    let result = (|| -> TestResult {
        std::fs::create_dir_all(root.join("crates/member/src"))?;
        std::fs::create_dir_all(root.join(".github/workflows"))?;
        std::fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"crates/member\"]\n",
        )?;
        std::fs::write(
            root.join("crates/member/Cargo.toml"),
            "[package]\nname = \"member\"\nversion = \"1.0.0\"\n",
        )?;
        std::fs::write(
            root.join("crates/member/src/lib.rs"),
            "pub fn sample() -> bool { true }\n",
        )?;
        std::fs::write(
            root.join(".github/workflows/ci.yml"),
            "on: push\njobs: {}\n",
        )?;
        operation(&root)
    })();
    let cleanup = std::fs::remove_dir_all(root);
    result?;
    cleanup?;
    Ok(())
}

fn rust_readers(root: &std::path::Path) -> Vec<Result<usize, labcolors_audit::AuditInputError>> {
    use labcolors_audit::extractors::*;
    vec![
        labcolors_audit::enumerate_production_artifacts(root).map(|items| items.len()),
        extract_public_api(root).map(|items| items.len()),
        extract_operations(root).map(|items| items.len()),
        extract_conformance_branches(root).map(|items| items.len()),
        extract_exports_metadata(root).map(|items| items.len()),
    ]
}

#[test]
fn healthy_sources_remain_available_to_every_reader() -> TestResult {
    with_source_tree(|root| {
        for result in rust_readers(root) {
            result?;
        }
        let inventory = labcolors_audit::enumerate_production_artifacts(root)?;
        assert!(
            inventory
                .iter()
                .any(|item| item.module == "crates/member/src/lib.rs")
        );
        assert!(
            inventory
                .iter()
                .any(|item| item.module == ".github/workflows/ci.yml")
        );
        Ok(())
    })
}

#[test]
fn invalid_utf8_cannot_disappear_from_any_rust_reader() -> TestResult {
    with_source_tree(|root| {
        let path = root.join("crates/member/src/lib.rs");
        std::fs::write(&path, [0xff, 0xfe])?;
        for result in rust_readers(root) {
            let error = result.expect_err("повреждённый обязательный файл");
            assert_eq!(error.path(), path);
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        }
        Ok(())
    })
}

#[test]
fn invalid_rust_cannot_disappear_from_any_rust_reader() -> TestResult {
    with_source_tree(|root| {
        let path = root.join("crates/member/src/lib.rs");
        std::fs::write(&path, "pub fn broken(")?;
        for result in rust_readers(root) {
            let error = result.expect_err("неразобранный обязательный исходник");
            assert_eq!(error.path(), path);
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        }
        Ok(())
    })
}

#[test]
fn missing_workflows_do_not_produce_a_partial_audit() -> TestResult {
    with_source_tree(|root| {
        std::fs::remove_dir_all(root.join(".github/workflows"))?;
        let error =
            labcolors_audit::run_full_audit(root).expect_err("обязательный каталог отсутствует");
        assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
        let output = Command::new(env!("CARGO_BIN_EXE_scanner"))
            .arg(root)
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        Ok(())
    })
}

#[test]
fn corrupt_cargo_manifest_is_an_input_failure_for_all_readers() -> TestResult {
    with_source_tree(|root| {
        let path = root.join("crates/member/Cargo.toml");
        std::fs::write(&path, "[package")?;
        let mut outcomes = rust_readers(root);
        outcomes
            .push(labcolors_audit::extractors::extract_dependencies(root).map(|items| items.len()));
        for outcome in outcomes {
            let error = outcome.expect_err("повреждённый манифест");
            assert_eq!(error.path(), path);
            assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        }
        Ok(())
    })
}

#[test]
fn unreadable_workflow_does_not_publish_partial_inventory() -> TestResult {
    with_source_tree(|root| {
        std::fs::write(root.join(".github/workflows/ci.yml"), [0xff])?;
        let output = Command::new(env!("CARGO_BIN_EXE_scanner"))
            .arg(root)
            .output()?;
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        Ok(())
    })
}

#[cfg(unix)]
#[test]
fn source_symlinks_are_rejected_without_following_them() -> TestResult {
    with_source_tree(|root| {
        let path = root.join("crates/member/src/loop.rs");
        std::os::unix::fs::symlink("loop.rs", &path)?;
        for outcome in rust_readers(root) {
            let error = outcome.expect_err("ссылка не является исходником");
            assert_eq!(error.path(), path);
        }
        Ok(())
    })
}
