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
//!   1. Injecting an unmarked `_AUDIT_PROBE` const INTO the real `semantic.rs`
//!      on disk (in a temporary copy / rename roundtrip so the original is safe).
//!   2. Spawning `cargo test -p labcolors-core --test empirical_inventory
//!      gate1_every_policy_const_is_marked` as a subprocess.
//!   3. Asserting the subprocess exits NON-ZERO and its stderr/stdout mentions
//!      `_AUDIT_PROBE` — proving the live gate bites on a real on-disk injection.
//!   4. Restoring `semantic.rs` unconditionally (via Drop-guard).
//!
//! Invariant: an unmarked policy const written to a real perceptual module file
//! always fails the live gate and names the const in the failure message.
//!
//! Regime: R4 regression (governance hygiene). NOT R3 (no value emitted) and NOT
//! R1 (no math). This test SOLELY proves the on-disk wiring of the scanner gate.
//!
//! How this test bites:
//!   * The subprocess runs GATE-1 against the real tree. If _AUDIT_PROBE is
//!     present and unmarked, gate1 exits with a test failure (non-zero exit) and
//!     names the probe in the message. If it does NOT fail, this test panics:
//!     "RED-proof FAILED — gate1 did not flag the on-disk probe".
//!   * After the restore, the subprocess runs green again (real-tree invariant).
//!
//! # ISOLATION CONTRACT — MUST READ BEFORE RUNNING
//!
//! This test **writes to the real `crates/labcolors-core/src/semantic.rs`** on
//! disk and spawns a nested `cargo test` subprocess against the same workspace.
//! Running it concurrently with other tests (the default `cargo test` mode) can
//! corrupt the source tree under concurrency or leave it permanently broken on a
//! hard abort (SIGKILL / CI timeout / power loss), because the Drop guard does
//! not run on SIGKILL.
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
//!        on_disk_audit_probe_goes_red_then_green_after_restore \
//!        -- --ignored --test-threads=1
//!      ```
//!
//!   `--test-threads=1` is MANDATORY: the test must be the ONLY test running in
//!   the same process to prevent concurrent test threads from observing the
//!   corrupted source.
//!
//! ## Why not just use a temp file?
//!
//! The subprocess spawns `cargo test -p labcolors-core --test empirical_inventory`,
//! which re-compiles `labcolors-core` from the REAL source directory. A copy
//! would not be read by Cargo. The spliced file must be the actual source.
//! The Drop guard + backup ensure correctness under panic; only SIGKILL defeats it.
//!
//! Isolation: the original `semantic.rs` is restored unconditionally via a
//! Drop guard, so even a panic in the middle leaves the working tree clean.

// Pull in the shared panic-safe splice + RestoreGuard from the support module.
// This closes the DRY/SRP fracture: both on_disk_audit_probe.rs and
// s2b_baseline_guards.rs previously maintained separate copies that had already
// diverged in their Drop panic-safety semantics (confirmed High defect).
// The canonical implementation in splice_support.rs is the single source of
// truth; Drop is panic-safe (logs errors, never re-panics during unwind).
#[path = "splice_support.rs"]
mod splice_support;

use serial_test::serial;
use std::path::PathBuf;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn semantic_path() -> PathBuf {
    crate_root().join("src").join("semantic.rs")
}

/// The CI-pinned Rust toolchain. The subprocess uses `cargo +<toolchain>` to
/// ensure the nested gate run uses the same toolchain as CI — avoiding the
/// local-vs-CI toolchain divergence noted in the golangci-lint-version-must-match-ci
/// memory. Must match `RUST_TOOLCHAIN` in `.github/workflows/ci.yml`.
const GATE_TOOLCHAIN: &str = "1.96.0";

/// Spawn `cargo +<GATE_TOOLCHAIN> test -p labcolors-core --test empirical_inventory <test_name>`
/// and return `(exit_success, combined_output_string)`.
fn run_gate_test(test_name: &str) -> (bool, String) {
    // Use the workspace root so cargo resolves the manifest correctly.
    let workspace_root = crate_root()
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap_or_else(|_| crate_root().join("..").join(".."));

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
        .current_dir(&workspace_root)
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

/// On-disk RED-proof: inject an unmarked `_AUDIT_PROBE` const into the REAL
/// `semantic.rs` file, assert GATE-1 exits RED and names the probe, then restore.
///
/// This test proves the LIVE gate (reading real files) bites on an on-disk
/// injection — the in-memory probe in `empirical_inventory.rs` cannot prove this
/// because it never writes to disk.
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
/// `#[serial]` serialises threads within this binary. It does NOT close the
/// cross-binary race with `s2b_baseline_guards::nested_subprocess_gate1_goes_red_on_injected_audit_probe`
/// (a separate binary). The actual cross-binary isolation guarantee rests on the
/// THREE independent gates documented in the module doc: `#[ignore]`, env-var
/// tripwire, and `--test-threads=1` in the documented invocation.
#[test]
#[ignore = "mutates the real src/semantic.rs on disk; run solo with \
            LABCOLORS_ON_DISK_PROBE_ENABLED=1 and --test-threads=1 \
            (see module doc for the exact command). Never run in default \
            `cargo test` or `cargo test --workspace`."]
#[serial]
#[cfg(not(miri))] // Miri cannot execute external subprocesses.
fn on_disk_audit_probe_goes_red_then_green_after_restore() {
    // Env-var tripwire: a CI runner that passes `--include-ignored` must still
    // explicitly set this env var to trigger the disk-mutating test. Without it
    // the test skips cleanly (not panics), so CI stays green on accidental
    // --include-ignored and the hazard is not silently unleashed.
    if std::env::var("LABCOLORS_ON_DISK_PROBE_ENABLED").as_deref() != Ok("1") {
        eprintln!(
            "on_disk_audit_probe: SKIPPED — set LABCOLORS_ON_DISK_PROBE_ENABLED=1 \
             and run with --test-threads=1 to enable this disk-mutating test. \
             See module doc for the exact invocation command."
        );
        return;
    }
    let original_path = semantic_path();
    let backup_path = original_path.with_extension("rs.on_disk_probe_backup");

    // Back up the original before any mutation.
    std::fs::copy(&original_path, &backup_path).unwrap_or_else(|e| {
        panic!("on_disk probe: cannot create backup of semantic.rs: {e}");
    });

    // Install the Drop guard IMMEDIATELY after backup — before any other fallible op.
    // Uses the canonical panic-safe RestoreGuard from splice_support (confirmed fix
    // for the High defect: the old local RestoreGuard used .expect() in Drop,
    // causing double-panic → process abort → semantic.rs left spliced on Windows
    // AV/indexer file-lock races or disk-full during test unwind).
    let _guard = splice_support::RestoreGuard {
        target: original_path.clone(),
        backup: backup_path.clone(),
    };

    // Verify the real tree is GREEN before the injection (pre-condition). If the
    // real tree is already RED, the RED-proof would be ambiguous.
    let (pre_green, pre_out) = run_gate_test("gate1_every_policy_const_is_marked");
    assert!(
        pre_green,
        "on_disk RED-proof pre-condition FAILED — gate1 is already RED on the real tree \
         before any injection. The probe cannot prove 'injection causes RED' unless the \
         tree starts GREEN.\n{pre_out}"
    );

    // Inject an unmarked const into the real file using the canonical shared
    // splice_into (atomic write: write to *.splice_tmp, rename over target).
    splice_support::splice_into(&original_path, "const _AUDIT_PROBE: f64 = 42.0;");

    // Run GATE-1 against the now-mutated on-disk file.
    let (injected_green, injected_out) = run_gate_test("gate1_every_policy_const_is_marked");
    assert!(
        !injected_green,
        "on_disk RED-proof FAILED — gate1 did NOT fail after injecting an unmarked \
         `_AUDIT_PROBE: f64 = 42.0` into the real semantic.rs. The live on-disk scanner \
         is not biting; a real magic-number addition would pass the gate silently.\n{injected_out}"
    );
    assert!(
        injected_out.contains("_AUDIT_PROBE"),
        "on_disk RED-proof FAILED — gate1 failed (non-zero exit) but the output does NOT \
         name `_AUDIT_PROBE`. The gate is failing for the WRONG reason (it must fail \
         specifically because of the probe, not a pre-existing issue).\n{injected_out}"
    );

    // Drop guard restores semantic.rs here (explicit drop for clarity, though
    // it happens automatically).
    drop(_guard);

    // Post-condition: the restored tree must be GREEN again.
    let (post_green, post_out) = run_gate_test("gate1_every_policy_const_is_marked");
    assert!(
        post_green,
        "on_disk RED-proof FAILED — gate1 is RED after the restore. The Drop guard may \
         not have run correctly, or the real tree was already RED pre-existing.\n{post_out}"
    );
}
