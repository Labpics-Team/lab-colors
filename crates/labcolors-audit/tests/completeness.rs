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
    let covered = assessed(Disposition::Covered { evidence_key: "known-proof".into() });
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
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()
    ));
    assert!(!root.exists());
    let output = Command::new(env!("CARGO_BIN_EXE_scanner")).arg(root).output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    Ok(())
}
