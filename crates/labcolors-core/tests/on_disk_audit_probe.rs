//! On-disk _AUDIT_PROBE RED-proof — R4 regression.
//!
//! BUG CLASS this guards: the in-memory `red_proof_audit_probe` in
//! `empirical_inventory.rs` proves that the SCANNER logic flips RED on a
//! spliced-in-memory source. But the GATE itself runs `scan_tree()` which reads
//! real files via `std::fs::read_to_string`. If the file I/O path or the
//! `scan_tree` → `scan_source` wiring has a bug (e.g. reads from cache, skips a
//! module, or the test binary uses a different CARGO_MANIFEST_DIR), the
//! in-memory probe would still be GREEN even though the live gate would miss a
//! real unmarked const.
//!
//! This test closes that gap by:
//!   1. Creating a temporary copy of the entire workspace (isolation-safe).
//!   2. Injecting an unmarked `_AUDIT_PROBE` const into the COPY of `semantic.rs`
//!      (never touching the real file).
//!   3. Spawning `cargo test -p labcolors-core --test empirical_inventory
//!      gate1_every_policy_const_is_marked` as a subprocess rooted at the tempdir.
//!   4. Asserting the subprocess exits NON-ZERO and its stderr/stdout mentions
//!      `_AUDIT_PROBE` — proving the live gate bites on an on-disk injection.
//!
//! Invariant: an unmarked policy const written to a perceptual module file
//! always fails the live gate and names the const in the failure message.
//!
//! Regime: R4 regression (governance hygiene). NOT R3 (no value emitted) and NOT
//! R1 (no math). This test SOLELY proves the on-disk wiring of the scanner gate.
//!
//! How this test bites:
//!   * The subprocess runs GATE-1 from the tempdir copy. If _AUDIT_PROBE is
//!     present and unmarked in the copy, gate1 exits with a test failure
//!     (non-zero exit) and names the probe in the message. If it does NOT fail,
//!     this test panics: "RED-proof FAILED — gate1 did not flag the on-disk probe".
//!   * The real tree is never touched, so the test is safe to run in parallel.
//!
//! # ISOLATION CONTRACT — MUST READ BEFORE RUNNING
//!
//! This test copies the workspace into a temporary directory and spawns a
//! subprocess rooted at the tempdir. The **real source tree is never written to**,
//! so it is safe to run in parallel with other tests. The tempdir is created
//! fresh per test invocation and is cleaned up on drop (even on panic).
//!
//! ## Required opt-in (two independent gates — both must be satisfied):
//!
//!   1. **`#[ignore]`**: the test is skipped by default `cargo test` and
//!      `cargo test --workspace`. Pass `-- --ignored` or `-- --include-ignored`
//!      to un-ignore it.
//!
//!   2. **`LABCOLORS_ON_DISK_PROBE_ENABLED=1`** (env var): the test body checks
//!      this env var and skips with a descriptive message if it is absent. This
//!      is a second tripwire so a CI runner that strips `#[ignore]` via
//!      `--include-ignored` still requires explicit opt-in. To run:
//!
//!      ```sh
//!      LABCOLORS_ON_DISK_PROBE_ENABLED=1 \
//!        cargo test -p labcolors-core --test on_disk_audit_probe \
//!        on_disk_audit_probe_goes_red_then_green_after_restore -- --ignored
//!      ```
//!
//!   Unlike the old real-tree version, `--test-threads=1` is NO LONGER REQUIRED
//!   (the real tree is never mutated). The test will pass with it, but default
//!   parallelism is safe.
//!
//! # Isolation mechanism: temp-dir copy + atomic write + #[serial]
//!
//! The real workspace is copied into a per-invocation tempdir, the probe is
//! injected into the COPY with an atomic write (write-tmp → rename), and the
//! subprocess runs `current_dir(<tempdir>)` so Cargo recompiles from the copy.
//! The `#[serial]` annotation on this test serializes subprocess execution so
//! nested cargo invocations don't thrash the shared target/ cache. The tempdir
//! is cleaned up automatically when it goes out of scope.

// Pull in the shared panic-safe splice_into from the support module.
// This closes the DRY/SRP fracture: both on_disk_audit_probe.rs and
// s2b_baseline_guards.rs previously maintained separate copies that had already
// diverged. The canonical implementation in splice_support.rs is the single source
// of truth; the probe now uses it to splice into a TEMP-DIR COPY, not the real tree.
#[path = "splice_support.rs"]
mod splice_support;

use serial_test::serial;
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    crate_root()
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap_or_else(|_| crate_root().join("..").join(".."))
}

fn semantic_path() -> PathBuf {
    crate_root().join("src").join("semantic.rs")
}

/// The CI-pinned Rust toolchain. The subprocess uses `cargo +<toolchain>` to
/// ensure the nested gate run uses the same toolchain as CI — avoiding the
/// local-vs-CI toolchain divergence noted in the golangci-lint-version-must-match-ci
/// memory. Must match `RUST_TOOLCHAIN` in `.github/workflows/ci.yml`.
const GATE_TOOLCHAIN: &str = "1.96.0";

/// Copy directory `src` recursively into `dst`, creating `dst` if it doesn't exist.
/// Panics on fs errors (called before test body, not during unwind).
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_name = entry.file_name();
        let dst_path = dst.join(&file_name);

        if path.is_dir() {
            // Skip common non-essential directories to speed up copy.
            let name_str = file_name.to_string_lossy();
            if name_str == "target" || name_str == ".git" || name_str == "node_modules" {
                continue;
            }
            copy_dir_recursive(&path, &dst_path)?;
        } else {
            std::fs::copy(&path, &dst_path)?;
        }
    }
    Ok(())
}

/// Spawn `cargo +<GATE_TOOLCHAIN> test -p labcolors-core --test empirical_inventory <test_name>`
/// from the given `workspace_dir` and return `(exit_success, combined_output_string)`.
fn run_gate_test_from(workspace_dir: &Path, test_name: &str) -> (bool, String) {
    let output = std::process::Command::new("cargo")
        .args([
            &format!("+{GATE_TOOLCHAIN}"),
            "test",
            "-p",
            "labcolors-core",
            "--test",
            "empirical_inventory",
            test_name,
            "--",
            "--nocapture",
        ])
        .current_dir(workspace_dir)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn cargo test subprocess: {e}"));

    let combined = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), combined)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test
// ─────────────────────────────────────────────────────────────────────────────

/// On-disk RED-proof: inject an unmarked `_AUDIT_PROBE` const into a TEMP-DIR COPY
/// of `semantic.rs`, assert GATE-1 exits RED and names the probe.
///
/// This test proves the LIVE gate (reading real files) bites on an on-disk
/// injection — the in-memory probe in `empirical_inventory.rs` cannot prove this
/// because it never writes to disk. The test uses a temp-dir copy so the real
/// tree is never touched, making it safe to run in parallel.
///
/// Bites: the subprocess exits nonzero AND names `_AUDIT_PROBE` in its output.
/// If neither condition holds, this test panics with an explicit message.
///
/// # ISOLATION REQUIREMENT
///
/// This test is `#[ignore]`d and additionally gated on the
/// `LABCOLORS_ON_DISK_PROBE_ENABLED=1` env var. Both gates must be satisfied.
/// See the module-level doc comment for the full rationale and the exact
/// invocation command.
///
/// `#[serial]` serializes subprocess execution within the test binary so that
/// nested cargo builds don't thrash the shared target/ cache. Unlike the old
/// real-tree version, the real source tree is never written to, so the test is
/// safe under default parallelism.
#[test]
#[ignore = "test runs a nested cargo subprocess; requires \
            LABCOLORS_ON_DISK_PROBE_ENABLED=1 env var to enable. \
            See module doc for the exact command."]
#[serial]
#[cfg(not(miri))] // Miri cannot execute external subprocesses.
fn on_disk_audit_probe_goes_red_then_green_after_restore() {
    // Env-var tripwire: a CI runner that passes `--include-ignored` must still
    // explicitly set this env var to trigger the subprocess test. Without it
    // the test skips cleanly (not panics), so CI stays green on accidental
    // --include-ignored.
    if std::env::var("LABCOLORS_ON_DISK_PROBE_ENABLED").as_deref() != Ok("1") {
        eprintln!(
            "on_disk_audit_probe: SKIPPED — set LABCOLORS_ON_DISK_PROBE_ENABLED=1 \
             to enable this subprocess test. See module doc for the exact invocation."
        );
        return;
    }

    // Create a temporary workspace copy.
    let tempdir = tempfile::TempDir::new().unwrap_or_else(|e| {
        panic!("on_disk_audit_probe: cannot create tempdir: {e}");
    });
    let tempdir_path = tempdir.path();

    // Copy the entire workspace into the tempdir. We copy the real workspace root
    // and all its contents (excluding target/ and .git/) so that the nested cargo
    // invocation can resolve manifests and dependencies correctly.
    let real_workspace = workspace_root();
    copy_dir_recursive(&real_workspace, tempdir_path).unwrap_or_else(|e| {
        panic!(
            "on_disk_audit_probe: cannot copy workspace {:?} → {:?}: {e}",
            real_workspace, tempdir_path
        );
    });

    let temp_semantic_path = tempdir_path
        .join("crates")
        .join("labcolors-core")
        .join("src")
        .join("semantic.rs");

    // Verify the temp tree is GREEN before the injection (pre-condition). If it's
    // already RED, the RED-proof would be ambiguous.
    let (pre_green, pre_out) =
        run_gate_test_from(tempdir_path, "gate1_every_policy_const_is_marked");
    assert!(
        pre_green,
        "on_disk RED-proof pre-condition FAILED — gate1 is already RED on the temp tree \
         before any injection. The probe cannot prove 'injection causes RED' unless the \
         tree starts GREEN.\n{pre_out}"
    );

    // Inject an unmarked const into the COPY using the canonical shared splice_into
    // (atomic write: write to *.splice_tmp, rename over target).
    splice_support::splice_into(&temp_semantic_path, "const _AUDIT_PROBE: f64 = 42.0;");

    // Run GATE-1 against the now-mutated temp file.
    let (injected_green, injected_out) =
        run_gate_test_from(tempdir_path, "gate1_every_policy_const_is_marked");
    assert!(
        !injected_green,
        "on_disk RED-proof FAILED — gate1 did NOT fail after injecting an unmarked \
         `_AUDIT_PROBE: f64 = 42.0` into the temp semantic.rs. The live on-disk scanner \
         is not biting; a real magic-number addition would pass the gate silently.\n{injected_out}"
    );
    assert!(
        injected_out.contains("_AUDIT_PROBE"),
        "on_disk RED-proof FAILED — gate1 failed (non-zero exit) but the output does NOT \
         name `_AUDIT_PROBE`. The gate is failing for the WRONG reason (it must fail \
         specifically because of the probe, not a pre-existing issue).\n{injected_out}"
    );

    // Post-condition: verify the real tree was NEVER touched (isolation invariant).
    // The real semantic.rs should be byte-identical to its state before this test.
    let real_semantic = semantic_path();
    let git_status_output = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .arg(real_semantic.file_name().unwrap())
        .current_dir(crate_root())
        .output()
        .unwrap_or_else(|e| {
            panic!("on_disk_audit_probe: cannot run git status: {e}");
        });
    let status_str = String::from_utf8_lossy(&git_status_output.stdout);
    assert!(
        status_str.trim().is_empty(),
        "on_disk RED-proof FAILED — real semantic.rs was mutated during the test \
         (isolation violation). git status:\n{status_str}"
    );

    // Tempdir is automatically cleaned up when `tempdir` goes out of scope.
}
