//! Lint and format parity — smoke (R4 hygiene regime).
//!
//! BUG CLASS this guards: a test-plan change or marker commit introduces a
//! new lint warning or format deviation that `cargo clippy -D warnings` /
//! `cargo fmt --check` would catch on CI, but the engineer only runs
//! `cargo test` locally and ships the violation. Per the CLAUDE.md
//! "golangci-lint-version-must-match-ci" memory, the local tool version MUST
//! match the CI-pinned version (`RUST_TOOLCHAIN: 1.96.0`) to avoid false-green
//! results.
//!
//! This test shells out to `cargo +1.96.0 fmt --all --check` and
//! `cargo +1.96.0 clippy --workspace --all-targets -- -D warnings`, asserting
//! both exit clean. If either fails, the test reports the full output so the
//! author can fix the violation before pushing.
//!
//! Invariant: no new lint/format debt; zero-dep preserved (these tests add no
//! production dependencies — only `std::process::Command`).
//!
//! How this test bites (RED path):
//!   Introduce a trivially lint-able pattern (e.g. `let _ = vec![1, 2]; let _a
//!   = _a;`) in any workspace file → `clippy -D warnings` exits non-zero → the
//!   test fails reporting the violation. This is a RED-at-birth test (new
//!   behaviour), not a characterization lock.
//!
//! Regime: R4 smoke. These tests do NOT assert perceptual values (R1/R2/R3) —
//! they assert toolchain hygiene only (INV-7: regime separation).
//!
//! Isolation note: these tests spawn external processes and therefore run
//! notably slower than unit tests (~10–60 s depending on incremental cache).
//! They are intentionally placed last in the test suite by file naming
//! convention. They are NOT annotated `#[ignore]` because skipping them locally
//! defeats their purpose — a CI-matching toolchain is always installed in this
//! workspace (1.96.0 is the pinned stable).

use std::path::PathBuf;

/// Workspace root resolved hermeneutically off CARGO_MANIFEST_DIR.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// Run `cargo +<toolchain> <args...>` at the workspace root, returning
/// `(exit_success, combined_stdout_stderr)`.
fn run_cargo_with_toolchain(toolchain: &str, args: &[&str]) -> (bool, String) {
    let mut cmd_args = vec![format!("+{toolchain}")];
    cmd_args.extend(args.iter().map(|s| s.to_string()));

    let output = std::process::Command::new("cargo")
        .args(&cmd_args)
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "lint_parity: failed to spawn `cargo +{toolchain} {}`: {e}\n\
                 Is Rust toolchain {toolchain} installed? Run: \
                 `rustup toolchain install {toolchain}`",
                args.join(" ")
            )
        });

    let combined = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), combined)
}

/// The RUST_TOOLCHAIN pinned in CI — must match exactly to avoid local/CI
/// divergence (analogue of golangci-lint-version-must-match-ci in CLAUDE.md).
const CI_TOOLCHAIN: &str = "1.96.0";

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// Smoke: `cargo +1.96.0 fmt --all --check` passes on the workspace.
///
/// Bites (RED path): introduce any formatting deviation (e.g. wrong indentation,
/// missing trailing comma, incorrect import ordering) → `fmt --check` exits
/// non-zero → this test fails reporting the diff.
///
/// Uses the CI-pinned toolchain to match CI exactly.
#[test]
fn fmt_check_passes_on_ci_toolchain() {
    let (ok, out) = run_cargo_with_toolchain(CI_TOOLCHAIN, &["fmt", "--all", "--check"]);
    assert!(
        ok,
        "LINT PARITY FAILED — `cargo +{CI_TOOLCHAIN} fmt --all --check` failed. \
         The workspace has formatting deviations that CI will reject.\n{out}"
    );
}

/// Smoke: `cargo +1.96.0 clippy --workspace --all-targets -- -D warnings` passes.
///
/// Bites (RED path): introduce any clippy lint warning (e.g. unused import,
/// needless `clone()`, redundant closure) → clippy exits non-zero → this test
/// fails reporting the lint.
///
/// Uses the CI-pinned toolchain (`1.96.0`) with `-D warnings` exactly as CI
/// does, so a lint that only appears on 1.96.0 is caught locally, not silently
/// tolerated on a different stable.
#[test]
fn clippy_workspace_all_targets_d_warnings_passes_on_ci_toolchain() {
    let (ok, out) = run_cargo_with_toolchain(
        CI_TOOLCHAIN,
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    );
    assert!(
        ok,
        "LINT PARITY FAILED — `cargo +{CI_TOOLCHAIN} clippy --workspace --all-targets \
         -- -D warnings` failed. The workspace has lint warnings that CI treats as errors.\n{out}"
    );
}
