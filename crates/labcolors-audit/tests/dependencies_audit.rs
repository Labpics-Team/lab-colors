use labcolors_audit::extractors::{extract_dependencies, DependencyEntry};
use std::path::Path;

fn workspace_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
}

#[test]
fn dependencies_includes_syn_in_audit_crate() {
    let deps = extract_dependencies(workspace_root());
    let has_syn = deps.iter().any(|d| {
        d.crate_name == "labcolors-audit"
            && d.dep_name == "syn"
            && d.section == "dependencies"
    });
    assert!(has_syn, "Expected syn in labcolors-audit dependencies");
}

#[test]
fn dependencies_includes_toml_in_audit_crate() {
    let deps = extract_dependencies(workspace_root());
    let has_toml = deps.iter().any(|d| {
        d.crate_name == "labcolors-audit"
            && d.dep_name == "toml"
            && d.section == "dependencies"
    });
    assert!(has_toml, "Expected toml in labcolors-audit dependencies");
}

#[test]
fn dependencies_deterministic_order() {
    let first = extract_dependencies(workspace_root());
    let second = extract_dependencies(workspace_root());
    assert_eq!(first, second, "Two calls must produce identical output");

    // Verify sort invariant: (crate_name, section, dep_name) non-decreasing
    for window in first.windows(2) {
        let a = &window[0];
        let b = &window[1];
        let ord = a
            .crate_name
            .cmp(&b.crate_name)
            .then_with(|| a.section.cmp(&b.section))
            .then_with(|| a.dep_name.cmp(&b.dep_name));
        assert!(
            ord != std::cmp::Ordering::Greater,
            "Sort violated: {:?} > {:?}",
            a,
            b
        );
    }
}

#[test]
fn removed_dependency_detected() {
    let deps = extract_dependencies(workspace_root());
    let audit_deps: Vec<&DependencyEntry> = deps
        .iter()
        .filter(|d| d.crate_name == "labcolors-audit" && d.section == "dependencies")
        .collect();

    // Sabotage guard: if someone deletes all deps, this fails.
    // labcolors-audit must have at least serde, toml, syn, walkdir, etc.
    assert!(
        audit_deps.len() >= 4,
        "Expected at least 4 dependencies in labcolors-audit, found {}. \
         A dependency was likely removed without updating this test.",
        audit_deps.len()
    );
}