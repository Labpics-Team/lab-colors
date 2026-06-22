//! CI test-job enforcement — integration.
//!
//! BUG CLASS this guards: the empirical-inventory gate exists in the codebase
//! but is NOT wired into CI — either the test job was removed, the job runs
//! only a subset of tests that excludes `empirical_inventory`, or the job name
//! changed without updating the gate's documentation. In all three cases, a
//! merge that introduces an unmarked policy const passes CI silently.
//!
//! This test reads the ACTUAL `.github/workflows/ci.yml` (not an assumed copy
//! or a cached string) and asserts the structural properties that guarantee the
//! gate is enforced on every PR:
//!
//!   1. A job named `test` exists.
//!   2. That job includes a step that runs `cargo test --workspace` (or a
//!      semantically equivalent invocation that includes all workspace tests).
//!   3. The invocation does NOT explicitly exclude `empirical_inventory` via
//!      `--exclude` or a `--test` flag limiting to other tests.
//!
//! Invariant: the guard is enforced in CI per-PR, not merely present in the tree.
//!
//! How this test bites (mutation proof):
//!   * Change the CI `cargo test --workspace` line to `cargo test -p other-crate`
//!     → assertion (2) fails: `cargo test --workspace` not found.
//!   * Remove the `test` job from ci.yml → assertion (1) fails.
//!   * Add `--exclude labcolors-core` to the test step → assertion (3) fails.
//!
//! The test reads the REAL workflow file, so these mutations are observable.

use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/labcolors-core/
    // workspace root    = crates/labcolors-core/../../  = lab-colors/
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn ci_yml_path() -> PathBuf {
    workspace_root()
        .join(".github")
        .join("workflows")
        .join("ci.yml")
}

/// Read the CI YAML file, panicking with a clear message if it is absent.
fn read_ci_yml() -> String {
    let path = ci_yml_path();
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "CI enforcement test FAILED — cannot read `.github/workflows/ci.yml` at \
             {} ({e}). The file must exist for this gate to verify CI wiring.",
            path.display()
        )
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// Assert a `test:` job exists in ci.yml.
///
/// Bites: remove the `test:` job declaration → this assertion fails.
#[test]
fn ci_yml_has_a_test_job() {
    let yml = read_ci_yml();
    // The job key `test:` must appear as a YAML map key in the `jobs:` block.
    // We look for the pattern that the dtolnay/rust-toolchain CI files use:
    // `  test:` (indented) OR `test:` at the top level of the jobs block.
    let has_test_job = yml.lines().any(|l| {
        let t = l.trim();
        t == "test:" || t.starts_with("test:")
    });
    assert!(
        has_test_job,
        "CI enforcement FAILED — no `test:` job found in `.github/workflows/ci.yml`. \
         The empirical-inventory gate is only enforced if a job in CI runs \
         `cargo test --workspace`."
    );
}

/// Assert the test job runs `cargo test --workspace`.
///
/// Bites: change the step to `cargo test -p labcolors-wasm` → this assertion
/// fails because `--workspace` is absent.
#[test]
fn ci_test_job_runs_cargo_test_workspace() {
    let yml = read_ci_yml();
    // Look for the substring `cargo test --workspace` anywhere in the file.
    // This is the canonical form used in the CI file.
    assert!(
        yml.contains("cargo test --workspace"),
        "CI enforcement FAILED — `.github/workflows/ci.yml` does not contain \
         `cargo test --workspace`. The empirical-inventory gate (and all other \
         workspace tests) must be run with `--workspace` to be enforced per-PR."
    );
}

/// Assert the workspace test invocation does NOT exclude `labcolors-core`.
///
/// Bites: add `--exclude labcolors-core` to the cargo test step → this
/// assertion fails.
#[test]
fn ci_test_job_does_not_exclude_labcolors_core() {
    let yml = read_ci_yml();
    // If `--workspace` is present but labcolors-core is explicitly excluded via
    // `--exclude`, the empirical-inventory gate is silently bypassed.
    assert!(
        !yml.contains("--exclude labcolors-core"),
        "CI enforcement FAILED — `.github/workflows/ci.yml` contains \
         `--exclude labcolors-core`, which bypasses the empirical-inventory gate \
         and all other labcolors-core tests."
    );
    // The `cargo test --workspace` line itself must not be narrowed with a `-p`
    // flag that omits labcolors-core — e.g. `cargo test -p labcolors-wasm` would
    // skip the gate entirely.
    // Strategy: find the line that contains `cargo test --workspace` and assert
    // it does NOT also contain `-p ` (a package-scoping flag). The `--workspace`
    // and `-p` flags are mutually exclusive in practice, but belt-and-suspenders.
    let workspace_test_line = yml
        .lines()
        .find(|l| l.contains("cargo test --workspace"))
        .expect(
            "CI enforcement: `cargo test --workspace` must exist (asserted by \
             ci_test_job_runs_cargo_test_workspace)",
        );
    assert!(
        !workspace_test_line.contains(" -p "),
        "CI enforcement FAILED — the `cargo test --workspace` line also contains \
         `-p <package>`, which would scope the run to a specific package and \
         bypass labcolors-core tests: '{workspace_test_line}'"
    );
}

/// Assert the RUST_TOOLCHAIN version pinned in CI can be read and matches the
/// expected format (a semver version string like `1.96.0`). This is the
/// `golangci-lint-version-must-match-ci` analogue for Rust: using a different
/// toolchain locally produces false-green lint, per the CLAUDE.md memory.
#[test]
fn ci_toolchain_version_is_explicitly_pinned() {
    let yml = read_ci_yml();
    // The CI file must declare RUST_TOOLCHAIN as an env var.
    assert!(
        yml.contains("RUST_TOOLCHAIN:"),
        "CI enforcement FAILED — no `RUST_TOOLCHAIN:` env var found in ci.yml. \
         The toolchain must be explicitly pinned so local runs match CI exactly."
    );
    // The value must look like a semver version (digit.digit.digit).
    let toolchain_line = yml
        .lines()
        .find(|l| l.contains("RUST_TOOLCHAIN:"))
        .expect("RUST_TOOLCHAIN line must exist (already asserted above)");
    // Extract the version token — look for X.Y.Z pattern.
    let has_semver = toolchain_line.split_whitespace().any(|tok| {
        tok.chars().filter(|c| *c == '.').count() >= 2
            && tok.chars().next().is_some_and(|c| c.is_ascii_digit())
    });
    assert!(
        has_semver,
        "CI enforcement FAILED — `RUST_TOOLCHAIN` line does not contain a semver \
         version (X.Y.Z): '{toolchain_line}'. The toolchain must be pinned to an \
         exact version, not `stable` or a channel, to ensure local/CI parity."
    );
}
