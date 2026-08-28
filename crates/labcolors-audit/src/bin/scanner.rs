use std::path::PathBuf;

use labcolors_audit::{assign_dispositions, audit_gate, enumerate_production_artifacts};

fn main() {
    let source_root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    eprintln!(
        "AUD-01 scanner: enumerating artifacts in {}",
        source_root.display()
    );
    let raw = enumerate_production_artifacts(&source_root);
    eprintln!("  found {} raw artifacts", raw.len());

    let dispositioned = assign_dispositions(&raw);
    let verdict = audit_gate(&dispositioned);

    let json = serde_json::to_string_pretty(&verdict).expect("AuditVerdict serializes");
    println!("{json}");

    if !verdict.passed {
        eprintln!(
            "AUD-01 GATE FAILED: {} orphaned, {} defective (total {})",
            verdict.orphaned_count, verdict.defective_count, verdict.total_artifacts
        );
        std::process::exit(1);
    }

    if verdict.not_assessed_count > 0 {
        eprintln!(
            "AUD-01 gate passed with {} NotAssessed artifacts — coverage incomplete",
            verdict.not_assessed_count
        );
    } else {
        eprintln!(
            "AUD-01 gate passed: all {} artifacts accounted for",
            verdict.total_artifacts
        );
    }
}
