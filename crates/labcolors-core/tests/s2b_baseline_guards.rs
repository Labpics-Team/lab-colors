//! s2b chapter-01 baseline guards — TDD RED/PIN phase.
//!
//! SCOPE: c01-tempdir-copy-atomic-serial (epic: s2b-isolation-safe-guards).
//! These tests are written BEFORE the production change (no tempdir copy, no
//! atomic write, no `#[serial]` on the probe yet). Their purpose is:
//!
//!   1. Pin the current (correct) behaviour so byte-identity can be proven
//!      after the rewrite.
//!   2. Prove the existing probe bites (is not vacuous) by running it solo in
//!      its documented invocation.
//!   3. Encode the real-tree-immutability invariant as a property test so that
//!      any future regression re-introducing a real-tree write is caught by class.
//!   4. Stress-test the probe under back-to-back parallelism (the exact race the
//!      tempdir fix must close).
//!   5. Prove the nested subprocess (the gate test run from a copy) compiles and
//!      runs correctly.
//!   6. Audit scope discipline: `#[serial]` applied ONLY to the probe; other test
//!      files byte-unchanged; diff limited to on_disk_audit_probe.rs + Cargo.toml.
//!   7. Supply-chain audit: `serial_test` is dev-only; `cargo audit` green.
//!   8. CI parity: fmt/clippy/test/audit all green on the 1.96.0 toolchain.
//!
//! HOW EACH TEST BITES (per-test proof):
//!
//!   Tests 1, 2, 3, 7, 8 — characterization / pin (GREEN at birth by design).
//!   Bite proof: deliberately BREAK the asserted invariant in a throwaway copy,
//!   confirm the test fails, then restore. Documented inline per test.
//!
//!   Test 4 (parallel stability) — property / fuzz (GREEN at birth).
//!   Bite proof: the old non-atomic+Drop code would leave `semantic.rs` dirty
//!   under SIGKILL. The test asserts `git status` is empty; a dirty-tree
//!   scenario makes it RED.
//!
//!   Test 5 (nested-subprocess integration) — integration (GREEN at birth).
//!   Bite proof: corrupt the nested invocation command (wrong test name) and
//!   the subprocess exits non-zero, making the assertion fail.
//!
//!   Test 6 (scope-discipline diff audit) — contract (GREEN: the scope invariants
//!   are now asserted against the REAL safety gates, not a literal token).
//!
//! ISOLATION: tests 2, 3, 4, 5b run the on-disk probe (or its subprocess) and
//! therefore carry the same isolation requirements:
//!   - `#[ignore]` — skipped by default `cargo test`
//!   - `LABCOLORS_ON_DISK_PROBE_ENABLED=1` env var — second opt-in tripwire
//!   - Run solo or with `--test-threads=1` until the tempdir fix lands (t2)
//!
//! The `#[serial]` annotation on the probe was closed by a prior commit. Its
//! contract is now verified by asserting the THREE real isolation invariants
//! (see test 6), not by a literal-string assertion that would circularly pin
//! an inert token.

// Pull in the canonical splice helpers to avoid duplicating the panic-safe
// RestoreGuard and splice_into logic (DRY fracture fix).
#[path = "splice_support.rs"]
mod splice_support;

use std::path::PathBuf;

// ─────────────────────────────────────────────────────────────────────────────
// Shared helpers
// ─────────────────────────────────────────────────────────────────────────────

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    crate_root().join("..").join("..")
}

fn semantic_path() -> PathBuf {
    crate_root().join("src").join("semantic.rs")
}

/// The CI-pinned Rust toolchain (matches RUST_TOOLCHAIN in ci.yml and
/// GATE_TOOLCHAIN in on_disk_audit_probe.rs).
const CI_TOOLCHAIN: &str = "1.96.0";

/// Check whether the on-disk probe opt-in env var is set.
fn probe_enabled() -> bool {
    std::env::var("LABCOLORS_ON_DISK_PROBE_ENABLED").as_deref() == Ok("1")
}

/// Check whether `cargo +<CI_TOOLCHAIN>` is available. If not, the test skips
/// rather than failing for tooling reasons unrelated to the code under test.
///
/// This mirrors the skip-if-missing guard in `supply_chain_cargo_audit_green`
/// and closes the class of "hardcoded toolchain without skip guard" defects
/// (confirmed High finding in the s2b scope review).
fn toolchain_available() -> bool {
    std::process::Command::new("cargo")
        .args([&format!("+{CI_TOOLCHAIN}"), "--version"])
        .current_dir(workspace_root())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Skip the calling test with a descriptive message when the pinned toolchain is
/// absent. Call this at the TOP of any test that invokes `cargo +<CI_TOOLCHAIN>`.
///
/// Not a macro so it cannot early-return for us — callers check the return value:
/// `if skip_if_toolchain_absent("test_name") { return; }`.
fn skip_if_toolchain_absent(test_name: &str) -> bool {
    if !toolchain_available() {
        eprintln!(
            "s2b_baseline/{test_name}: SKIPPED — Rust toolchain {CI_TOOLCHAIN} not found. \
             Install with: `rustup toolchain install {CI_TOOLCHAIN}`. \
             CI installs it automatically; this skip is acceptable locally."
        );
        true
    } else {
        false
    }
}

/// Run `cargo +<CI_TOOLCHAIN> test -p labcolors-core --test <binary> <test_name>
/// -- --nocapture` at the workspace root. Returns `(exit_success, combined_output)`.
///
/// Callers MUST call `skip_if_toolchain_absent` before calling this helper.
fn run_cargo_test(binary: &str, test_name: &str) -> (bool, String) {
    let output = std::process::Command::new("cargo")
        .args([
            &format!("+{CI_TOOLCHAIN}"),
            "test",
            "-p",
            "labcolors-core",
            "--test",
            binary,
            test_name,
            "--",
            "--nocapture",
        ])
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| panic!("s2b_baseline: failed to spawn `cargo test`: {e}"));

    let combined = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), combined)
}

/// Run `cargo +<CI_TOOLCHAIN> test -p labcolors-core --test on_disk_audit_probe
/// on_disk_audit_probe_goes_red_then_green_after_restore
/// -- --ignored --test-threads=1` with `LABCOLORS_ON_DISK_PROBE_ENABLED=1`.
/// Returns `(exit_success, combined_output)`.
///
/// Callers MUST call `skip_if_toolchain_absent` before calling this helper.
fn run_probe_solo() -> (bool, String) {
    let output = std::process::Command::new("cargo")
        .args([
            &format!("+{CI_TOOLCHAIN}"),
            "test",
            "-p",
            "labcolors-core",
            "--test",
            "on_disk_audit_probe",
            "on_disk_audit_probe_goes_red_then_green_after_restore",
            "--",
            "--ignored",
            "--test-threads=1",
        ])
        .env("LABCOLORS_ON_DISK_PROBE_ENABLED", "1")
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| panic!("s2b_baseline: failed to spawn probe subprocess: {e}"));

    let combined = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), combined)
}

/// Hash the bytes of `semantic.rs` — used to assert immutability.
fn semantic_hash() -> u64 {
    use std::hash::Hash;
    let bytes =
        std::fs::read(semantic_path()).expect("s2b_baseline: cannot read semantic.rs to hash it");
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    std::hash::Hasher::finish(&h)
}

/// Run `git status --porcelain crates/labcolors-core/src/` at the workspace
/// root. Returns the stdout (empty means clean, non-empty means dirty OR litter).
///
/// # Why the full src/ directory, not just semantic.rs
///
/// The previous path-scoped check (`git status --porcelain …/semantic.rs`)
/// only proved the target file was unmodified. Sidecar litter left by splice
/// or restore on abnormal exit (e.g. `*.splice_tmp`, `*.restore_tmp`,
/// `*.on_disk_probe_backup`) lives in the SAME directory but does NOT match
/// the single-file pathspec — so the check was GREEN while the working tree
/// was dirty. Scanning the whole `src/` directory catches both:
///   (1) modification to semantic.rs itself, and
///   (2) untracked sidecar files (now also covered by .gitignore, but the
///       test catch is the defence-in-depth layer that fires even without it).
fn git_status_src_dir() -> String {
    let output = std::process::Command::new("git")
        .args(["status", "--porcelain", "crates/labcolors-core/src/"])
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| panic!("s2b_baseline: cannot run `git status`: {e}"));
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn run_cargo_check(args: &[&str]) -> (bool, String) {
    let mut full_args = vec![format!("+{CI_TOOLCHAIN}")];
    full_args.extend(args.iter().map(|s| s.to_string()));

    let output = std::process::Command::new("cargo")
        .args(&full_args)
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| {
            panic!(
                "s2b_baseline/test8: cannot run `cargo +{CI_TOOLCHAIN} {}`: {e}",
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

// ─────────────────────────────────────────────────────────────────────────────
// Test 1 — baseline byte-identity snapshot (regression / characterization)
//
// REGIME: regression (characterization lock of already-correct behaviour).
// HOW IT BITES: GREEN at birth. Mutation proof: change any entry in
// R3_ACCENT_007AFF_GOLDEN or R3_RESOLVE_SET_SPOTS in r3_byte_identity.rs → the
// test that calls the same functions will fail. Here we additionally assert the
// file content of r3_byte_identity.rs and empirical_inventory.rs is byte-stable
// relative to the committed version (no drift introduced by this commit).
//
// Concrete bite: corrupt any golden constant in R3_ACCENT_007AFF_GOLDEN (e.g.
// change "#F4F8FF" → "#000000") → the r3_byte_identity tests that compare
// against those constants fail with the drifted value. This test then also fails
// because the file hash changes.
// ─────────────────────────────────────────────────────────────────────────────

/// Byte-identity snapshot: confirm that the r3_byte_identity and
/// empirical_inventory test files are byte-identical to the committed version.
///
/// This is the "before" anchor. After the production change (t2) the same two
/// files must produce byte-identical checksums — proving the rewrite touched only
/// `on_disk_audit_probe.rs` and `Cargo.toml` (scope discipline, test 6).
///
/// GREEN at birth (characterization). Bites on mutation: alter any byte in
/// either file → the hash changes → assertion fails.
///
/// Семантика анкора: лок = «чисто против HEAD», не вечная заморозка — ловит
/// незакоммиченный дрейф; закоммиченное состояние и есть анкор. r3 закрыт к
/// расширениям enum `Resolved` (`#[non_exhaustive]` + wildcard-паника в матче
/// r3: новый вариант не требует правки залоченного файла, но не проходит
/// golden молча).
#[test]
fn baseline_r3_and_empirical_inventory_files_are_git_clean() {
    // Both files must show no modification in `git status`.
    //
    // NOTE (ADR-0001 PR-c): `tests/r3_byte_identity.rs` was relocated into the crate
    // as `src/r3_byte_identity_tests.rs` (a `#[cfg(test)]` unit module) because its
    // subject — the built-in `resolve_set` oracle — moved behind `#[cfg(test)]` and
    // is no longer visible to an out-of-crate integration test. The r3 byte-identity
    // oracle is preserved verbatim at the new path; this s2b anchor (a completed
    // epic's scope pin) drops the stale path rather than pin a file that moved.
    for file in [
        "crates/labcolors-core/tests/empirical_inventory.rs",
        "crates/labcolors-core/tests/lint_parity.rs",
        "crates/labcolors-core/tests/ci_enforcement.rs",
        "crates/labcolors-core/tests/gate_green_smoke.rs",
    ] {
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain", file])
            .current_dir(workspace_root())
            .output()
            .unwrap_or_else(|e| panic!("s2b_baseline: cannot run `git status` for {file}: {e}"));
        let status = String::from_utf8_lossy(&output.stdout).trim().to_string();
        assert!(
            status.is_empty(),
            "BASELINE SNAPSHOT FAILED — `{file}` is dirty in git: '{status}'. \
             These files must be byte-identical to the committed version (scope discipline). \
             This is the s2b baseline anchor; any modification is a scope violation."
        );
    }
}

/// Baseline: `r3_byte_identity` test suite passes on the current tree.
///
/// Captures that the byte-identity oracles are GREEN before the production
/// change. After the change they must still be GREEN with the same outputs.
///
/// GREEN at birth (the goldens match the current computation). Bites on mutation:
/// change any golden constant in `r3_byte_identity.rs` → the `assert_eq!` fails
/// naming the drifted stop; this outer test then also fails because the subprocess
/// exits non-zero.
///
/// `#[ignore]`: this test spawns `cargo +1.96.0 test --test r3_byte_identity`,
/// which recompiles the workspace and contends on the build-directory lock under
/// the outer `cargo test --workspace`. Run manually before PR:
/// ```sh
/// cargo +1.96.0 test -p labcolors-core --test s2b_baseline_guards \
///   baseline_r3_byte_identity_tests_pass -- --ignored
/// ```
#[test]
#[ignore = "spawns nested cargo (+1.96.0 test) — contends on build-dir lock \
            and inflates CI wall-clock when run inside cargo test --workspace. \
            Run manually before PR (see test doc)."]
fn baseline_r3_byte_identity_tests_pass() {
    if skip_if_toolchain_absent("baseline_r3_byte_identity_tests_pass") {
        return;
    }
    // Run all r3 tests. Relocated (ADR-0001 PR-c) into the crate as the
    // `#[cfg(test)]` module `r3_byte_identity_tests`, so it is a `--lib` name
    // filter now, not a `--test` integration binary.
    let output = std::process::Command::new("cargo")
        .args([
            &format!("+{CI_TOOLCHAIN}"),
            "test",
            "-p",
            "labcolors-core",
            "--lib",
            "r3_byte_identity_tests",
            "--",
            "--nocapture",
        ])
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| panic!("s2b_baseline: cannot spawn r3_byte_identity run: {e}"));

    let combined = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "BASELINE SNAPSHOT FAILED — `cargo test --test r3_byte_identity` exited non-zero. \
         The byte-identity oracles must pass on the current tree before any production \
         change (t2). Failing now means a pre-existing regression exists.\n{combined}"
    );
}

/// Baseline: `empirical_inventory` gate tests pass on the current tree (excluding
/// the `#[ignore]`d probe test, which is verified separately in test 2).
///
/// GREEN at birth. Bites on mutation: remove a `// GROUNDED` marker from a
/// perceptual module → GATE-1 fails → this outer test exits non-zero.
///
/// `#[ignore]`: this test spawns `cargo +1.96.0 test --test empirical_inventory`,
/// which recompiles the workspace and contends on the build-directory lock. Run
/// manually before PR:
/// ```sh
/// cargo +1.96.0 test -p labcolors-core --test s2b_baseline_guards \
///   baseline_empirical_inventory_tests_pass -- --ignored
/// ```
#[test]
#[ignore = "spawns nested cargo (+1.96.0 test) — contends on build-dir lock \
            and inflates CI wall-clock when run inside cargo test --workspace. \
            Run manually before PR (see test doc)."]
fn baseline_empirical_inventory_tests_pass() {
    if skip_if_toolchain_absent("baseline_empirical_inventory_tests_pass") {
        return;
    }
    let output = std::process::Command::new("cargo")
        .args([
            &format!("+{CI_TOOLCHAIN}"),
            "test",
            "-p",
            "labcolors-core",
            "--test",
            "empirical_inventory",
            "--",
            "--nocapture",
        ])
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| panic!("s2b_baseline: cannot spawn empirical_inventory run: {e}"));

    let combined = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "BASELINE SNAPSHOT FAILED — `cargo test --test empirical_inventory` exited non-zero. \
         The governance gate must pass on the current tree before the production change. \
         A pre-existing failure blocks t2.\n{combined}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2 — existing-probe RED-proof solo (smoke)
//
// REGIME: smoke. The probe is NOT vacuous — it bites today.
// HOW IT BITES: GREEN at birth (the probe currently passes when run correctly).
// Mutation proof: remove the inner `!injected_green` assertion in
// `on_disk_audit_probe.rs` → the probe's RED-proof assertion disappears → the
// probe itself passes vacuously → BUT this outer test still calls the probe and
// asserts `exit_success`. So the outer (this test) stays GREEN. The inner
// mutation is detected by the probe's own structure, not by this test.
//
// The stronger bite is: change `splice_into_semantic` to inject nothing (no
// `_AUDIT_PROBE` const) → the gate-1 subprocess is still RED on the pre-existing
// tree → the probe's inner `injected_green` assertion fails → the probe exits
// non-zero → this test's `assert!(ok, ...)` fails.
//
// Isolation: #[ignore] + LABCOLORS_ON_DISK_PROBE_ENABLED=1 env var required.
// ─────────────────────────────────────────────────────────────────────────────

/// Smoke: the on-disk probe passes when run solo per its documented invocation.
///
/// This proves the live gate bites (is not vacuous) today, establishing the
/// behaviour that must be preserved verbatim after the rewrite (t2).
///
/// GREEN at birth (the probe currently passes). Bites on mutation: break the
/// probe's injection or restore logic → the probe exits non-zero → this assertion
/// fails.
///
/// # ISOLATION REQUIREMENT
/// Requires `LABCOLORS_ON_DISK_PROBE_ENABLED=1` and run with `--test-threads=1`
/// (or solo via `--test on_disk_audit_probe`).
#[test]
#[ignore = "runs the disk-mutating probe; set LABCOLORS_ON_DISK_PROBE_ENABLED=1 \
            and --test-threads=1. See on_disk_audit_probe.rs module doc."]
fn existing_probe_red_proof_solo_passes() {
    if skip_if_toolchain_absent("existing_probe_red_proof_solo_passes") {
        return;
    }
    if !probe_enabled() {
        eprintln!(
            "s2b_baseline/test2: SKIPPED — set LABCOLORS_ON_DISK_PROBE_ENABLED=1 \
             and run with --test-threads=1 to enable."
        );
        return;
    }

    let (ok, out) = run_probe_solo();
    assert!(
        ok,
        "SMOKE FAILED — the existing on-disk probe failed when run solo per its documented \
         invocation. The probe must PASS today to establish the baseline behaviour that \
         survives the t2 rewrite. This means either the probe is broken pre-existing, \
         or the invocation is wrong.\n\nProbe output:\n{out}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3 — real-tree immutability invariant (property)
//
// REGIME: property. Closes the CLASS of real-tree-write regressions, not just
// the single input.
// HOW IT BITES: GREEN at birth (the probe's Drop guard currently restores the
// file on non-SIGKILL exit).
// Mutation proof: remove the `RestoreGuard` Drop impl in
// `on_disk_audit_probe.rs` → the probe leaves `semantic.rs` dirty after running
// → `git_status_src_dir()` returns a non-empty string → the assertion fails.
//
// Isolation: #[ignore] + LABCOLORS_ON_DISK_PROBE_ENABLED=1.
// ─────────────────────────────────────────────────────────────────────────────

/// Property: after the probe runs, `src/semantic.rs` is byte-identical to the
/// committed version (no modification in `git status`).
///
/// Encodes the structural invariant "no test execution leaves the real source
/// tree modified". GREEN at birth. Bites on mutation: remove the `RestoreGuard`
/// Drop restore from `on_disk_audit_probe.rs` → the probe leaves `semantic.rs`
/// dirty → `git status --porcelain` is non-empty → this test fails.
///
/// # ISOLATION REQUIREMENT
/// Requires `LABCOLORS_ON_DISK_PROBE_ENABLED=1` and run with `--test-threads=1`.
#[test]
#[ignore = "runs the disk-mutating probe; set LABCOLORS_ON_DISK_PROBE_ENABLED=1 \
            and --test-threads=1. See on_disk_audit_probe.rs module doc."]
fn real_tree_immutability_invariant_after_probe() {
    if skip_if_toolchain_absent("real_tree_immutability_invariant_after_probe") {
        return;
    }
    if !probe_enabled() {
        eprintln!(
            "s2b_baseline/test3: SKIPPED — set LABCOLORS_ON_DISK_PROBE_ENABLED=1 \
             and run with --test-threads=1 to enable."
        );
        return;
    }

    // Record the hash before the probe.
    let hash_before = semantic_hash();

    // Run the probe via subprocess (so the Drop guard in the probe binary runs).
    let (ok, out) = run_probe_solo();
    assert!(
        ok,
        "real-tree immutability test: probe subprocess failed — cannot assert \
         immutability if the probe itself is broken.\n{out}"
    );

    // After the probe, `git status` on `semantic.rs` must be empty.
    let status = git_status_src_dir();
    assert!(
        status.is_empty(),
        "REAL-TREE IMMUTABILITY FAILED — `git status --porcelain \
         crates/labcolors-core/src/` is non-empty after the probe ran: \
         '{status}'. The probe left the real source tree modified or littered \
         sidecar files (*.splice_tmp / *.restore_tmp / *.on_disk_probe_backup). \
         The RestoreGuard Drop restore must have failed or was removed."
    );

    // The byte hash must match (belt-and-suspenders over `git status`).
    let hash_after = semantic_hash();
    assert_eq!(
        hash_before, hash_after,
        "REAL-TREE IMMUTABILITY FAILED — semantic.rs byte hash changed after the probe \
         (before: {hash_before:#016x}, after: {hash_after:#016x}). The file content \
         was modified and not fully restored."
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4 — parallel back-to-back probe stability (fuzz)
//
// REGIME: fuzz / stress. Demonstrates the race IS closed (or exposes it if not).
// HOW IT BITES: GREEN at birth if the restore is correct (sequential runs under
// --test-threads=1 always restore). Bites under REAL concurrency: run two
// processes simultaneously → the Drop guard races → one leaves the file dirty.
// The test runs >=5 sequential runs and asserts clean after each.
// Under the old non-atomic code without #[serial], running concurrently (not
// sequential) corrupts the tree — but we cannot safely demonstrate that here
// pre-t2 without actually corrupting the CI tree. Instead, the test asserts the
// sequential-stability property, which is necessary (but not sufficient) for
// correctness. The race class is documented but demonstrated only via the
// non-atomic window analysis, not by actually triggering corruption.
//
// After t2 lands, this test should also be run without --test-threads=1 to
// prove the race is closed under true parallelism.
//
// Isolation: #[ignore] + LABCOLORS_ON_DISK_PROBE_ENABLED=1.
// ─────────────────────────────────────────────────────────────────────────────

/// Fuzz: run the probe ≥5 times back-to-back under sequential mode and assert
/// `git status` on `semantic.rs` is empty after every iteration.
///
/// This stresses the exact restore correctness that the old non-atomic+Drop code
/// could race on under concurrency. Sequential runs must always be clean.
///
/// GREEN at birth (the Drop guard restores sequentially). Bites on mutation:
/// remove the Drop restore → the second iteration starts with dirty
/// `semantic.rs` → `git status` is non-empty → assertion fails.
///
/// # ISOLATION REQUIREMENT
/// Requires `LABCOLORS_ON_DISK_PROBE_ENABLED=1` and run solo with
/// `--test-threads=1`.
#[test]
#[ignore = "runs the disk-mutating probe 5× back-to-back; \
            set LABCOLORS_ON_DISK_PROBE_ENABLED=1 and --test-threads=1."]
fn parallel_back_to_back_probe_stability() {
    if skip_if_toolchain_absent("parallel_back_to_back_probe_stability") {
        return;
    }
    if !probe_enabled() {
        eprintln!(
            "s2b_baseline/test4: SKIPPED — set LABCOLORS_ON_DISK_PROBE_ENABLED=1 \
             and run with --test-threads=1 to enable."
        );
        return;
    }

    const RUNS: usize = 5;
    for i in 0..RUNS {
        let (ok, out) = run_probe_solo();
        assert!(
            ok,
            "PARALLEL STABILITY FAILED — probe subprocess failed on iteration {i}/{RUNS}: \
             \n{out}"
        );

        let status = git_status_src_dir();
        assert!(
            status.is_empty(),
            "PARALLEL STABILITY FAILED — `git status --porcelain \
             crates/labcolors-core/src/` is non-empty after iteration \
             {i}/{RUNS}: '{status}'. The probe left the real source tree \
             modified or littered sidecar files. Under back-to-back runs the \
             restore must be complete and all litter cleaned before the next \
             iteration starts."
        );
    }

    // Also run once via `--include-ignored` to prove the probe is still `#[ignore]`d
    // (if it lost its `#[ignore]`, this invocation would run it without --test-threads=1
    // in the *outer* test process, which is exactly the hazard we document).
    // Here we just check the probe is still #[ignore]d by reading the source.
    let probe_src =
        std::fs::read_to_string(crate_root().join("tests").join("on_disk_audit_probe.rs"))
            .expect("s2b_baseline: cannot read on_disk_audit_probe.rs");
    assert!(
        probe_src.contains("#[ignore"),
        "PARALLEL STABILITY FAILED — `on_disk_audit_probe.rs` no longer contains \
         `#[ignore`. The probe must be `#[ignore]`d to be skipped by default \
         `cargo test` (opt-in invariant)."
    );

    // Confirm the LABCOLORS_ON_DISK_PROBE_ENABLED env-var tripwire is present.
    assert!(
        probe_src.contains("LABCOLORS_ON_DISK_PROBE_ENABLED"),
        "PARALLEL STABILITY FAILED — `on_disk_audit_probe.rs` no longer checks \
         `LABCOLORS_ON_DISK_PROBE_ENABLED`. The env-var second tripwire must be \
         preserved (opt-in invariant)."
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5 — nested-subprocess gate from tempdir copy (integration)
//
// REGIME: integration. Proves the nested `cargo test --test empirical_inventory`
// compiles and runs from a copy of the workspace in a tempdir — the contract the
// t2 production change must satisfy.
//
// NOTE: Pre-t2, we do NOT have the tempdir copy infrastructure yet. This test
// instead validates the INTERFACE CONTRACT: that `cargo test --test
// empirical_inventory gate1_every_policy_const_is_marked` compiles and passes
// from the real workspace root (the pre-t2 baseline). The test will be
// superseded in t2 to run from a tempdir.
//
// HOW IT BITES (pre-t2 baseline):
//   GREEN at birth (gate1 passes on the current clean tree).
//   Mutation proof: corrupt a `// GROUNDED` marker in semantic.rs → gate1 exits
//   non-zero → this test's `assert!(ok, ...)` fails.
//
// Post-t2 (when the tempdir copy lands), this test will be replaced or extended
// to run the same assertion against `current_dir(<tempdir>)` — same contract,
// different staging mechanism.
// ─────────────────────────────────────────────────────────────────────────────

/// Integration (pre-t2 baseline): prove that `cargo test --test empirical_inventory
/// gate1_every_policy_const_is_marked` compiles and passes.
///
/// This establishes the interface contract the nested subprocess must satisfy
/// before and after the production change. Post-t2 the same invocation will be
/// made from a tempdir copy; the contract (GREEN on an unmodified tree) is
/// identical.
///
/// GREEN at birth. Bites on mutation: remove a `// GROUNDED` marker from any
/// perceptual module → gate1 exits non-zero → this test fails.
///
/// `#[ignore]`: spawns `cargo +1.96.0 test --test empirical_inventory`, which
/// recompiles the workspace and contends on the build-directory lock. Run
/// manually before PR:
/// ```sh
/// cargo +1.96.0 test -p labcolors-core --test s2b_baseline_guards \
///   nested_subprocess_gate1_passes_on_clean_tree -- --ignored
/// ```
#[test]
#[ignore = "spawns nested cargo (+1.96.0 test) — contends on build-dir lock. \
            Run manually before PR (see test doc)."]
fn nested_subprocess_gate1_passes_on_clean_tree() {
    if skip_if_toolchain_absent("nested_subprocess_gate1_passes_on_clean_tree") {
        return;
    }
    let (ok, out) = run_cargo_test("empirical_inventory", "gate1_every_policy_const_is_marked");
    assert!(
        ok,
        "INTEGRATION FAILED — nested subprocess `cargo +{CI_TOOLCHAIN} test \
         --test empirical_inventory gate1_every_policy_const_is_marked` exited \
         non-zero on the clean tree. This is the interface contract the production \
         change (t2) must preserve.\n\nOutput:\n{out}"
    );
}

/// Integration: the nested subprocess exits RED when the source contains an
/// unmarked `_AUDIT_PROBE` const — exactly the contract the on-disk probe tests.
///
/// This validates the subprocess interface: it must name the probe const in its
/// output when the tree is injected. GREEN at birth (gate1 bites on injection).
/// Bites on mutation: change `gate1_every_policy_const_is_marked` to always exit
/// 0 → this test fails because `injected_ok` would be `true`.
///
/// NOTE: This test INJECTS into and then RESTORES the real semantic.rs. It uses
/// the shared panic-safe `splice_support::RestoreGuard` and
/// `splice_support::splice_into` (canonical implementations). Run with
/// --test-threads=1 until t2 lands.
#[test]
#[ignore = "injects into and restores the real semantic.rs; run with \
            LABCOLORS_ON_DISK_PROBE_ENABLED=1 and --test-threads=1."]
fn nested_subprocess_gate1_goes_red_on_injected_audit_probe() {
    if skip_if_toolchain_absent("nested_subprocess_gate1_goes_red_on_injected_audit_probe") {
        return;
    }
    if !probe_enabled() {
        eprintln!(
            "s2b_baseline/test5b: SKIPPED — set LABCOLORS_ON_DISK_PROBE_ENABLED=1 \
             and run with --test-threads=1 to enable."
        );
        return;
    }

    let original_path = semantic_path();
    let backup_path = original_path.with_extension("rs.s2b_test5_backup");

    std::fs::copy(&original_path, &backup_path).unwrap_or_else(|e| {
        panic!("s2b_baseline/test5: cannot back up semantic.rs: {e}");
    });

    // Use the canonical panic-safe RestoreGuard from splice_support.
    // This closes the DRY fracture: the old local `Restore` struct was a
    // separate implementation that had the CORRECT panic-safe semantics
    // (`let _ =` on errors) — now both callers share one canonical Drop.
    let _guard = splice_support::RestoreGuard {
        target: original_path.clone(),
        backup: backup_path.clone(),
    };

    // Splice using the canonical shared helper (closes the duplicated splice
    // logic that was already byte-identical but lived in two places with
    // different tmp filenames — now one path, one source of truth).
    splice_support::splice_into(&original_path, "const _AUDIT_PROBE: f64 = 42.0;");

    // The nested subprocess must exit RED and name _AUDIT_PROBE.
    let (injected_ok, injected_out) =
        run_cargo_test("empirical_inventory", "gate1_every_policy_const_is_marked");
    assert!(
        !injected_ok,
        "INTEGRATION FAILED — nested subprocess did NOT fail after injecting \
         `_AUDIT_PROBE: f64 = 42.0` into semantic.rs. The gate must exit RED on \
         an unmarked const — the nested-subprocess interface is broken.\n{injected_out}"
    );
    assert!(
        injected_out.contains("_AUDIT_PROBE"),
        "INTEGRATION FAILED — gate1 exited RED but did NOT name `_AUDIT_PROBE` in \
         its output. The failure is for the wrong reason.\n{injected_out}"
    );

    // Drop guard restores semantic.rs here.
    drop(_guard);

    // Post-restore: gate1 must be GREEN again.
    let (restored_ok, restored_out) =
        run_cargo_test("empirical_inventory", "gate1_every_policy_const_is_marked");
    assert!(
        restored_ok,
        "INTEGRATION FAILED — gate1 is still RED after the restore. The backup/restore \
         mechanism is broken.\n{restored_out}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 6 — scope-discipline diff audit (contract)
//
// REGIME: contract. Asserts the production diff (t2) is scope-limited.
//
// The previous implementation asserted `probe_src.contains("#[serial]")` as the
// scope-discipline marker. That was a circular dead contract (confirmed High
// defect): `#[serial]` was retained SOLELY because this test asserted its
// literal presence, while its actual cross-binary isolation value was nil (it is
// a process-local mutex; the cross-binary race is closed by the three OTHER
// gates). The `#[serial]`-literal assertion was coupling between two places with
// no functional payoff — a guard that protects the token, not the invariant.
//
// The fix: assert the REAL isolation invariants that actually protect the probe:
//   1. `#[ignore]` — probe is skipped by default `cargo test`.
//   2. `LABCOLORS_ON_DISK_PROBE_ENABLED` env-var — second tripwire.
//   3. `--test-threads=1` documentation — cross-binary isolation note.
// These are the three gates the module doc mandates; asserting them closes the
// CLASS of "opt-in gate silently removed" regressions.
//
// `#[serial]` itself is harmless and may remain on the probe (it correctly
// serialises threads within the probe binary if future tests are added); it is
// just not the contract this test pins.
//
// HOW IT BITES:
//   Remove `#[ignore]` from `on_disk_audit_probe.rs` → assertion (1) fails.
//   Remove the `LABCOLORS_ON_DISK_PROBE_ENABLED` check → assertion (2) fails.
//   Remove `--test-threads=1` from the module doc → assertion (3) fails.
//   Add `#[serial]` to lint_parity.rs → the scope-creep assertions below fail.
// ─────────────────────────────────────────────────────────────────────────────

/// Contract: the on-disk probe preserves its THREE real isolation gates — the
/// mechanisms that actually prevent the disk-mutating test from running
/// unsafely in default `cargo test`.
///
/// This replaces the previous literal `#[serial]`-token assertion (a circular
/// dead contract) with assertions on the functional safety invariants.
///
/// GREEN. Bites on mutation: remove `#[ignore]`, or remove the env-var check,
/// or remove the `--test-threads=1` requirement from the docs → the respective
/// assertion fails.
#[test]
fn scope_discipline_probe_real_isolation_gates_present() {
    let probe_src =
        std::fs::read_to_string(crate_root().join("tests").join("on_disk_audit_probe.rs"))
            .expect("s2b_baseline/test6: cannot read on_disk_audit_probe.rs");

    // Gate 1: `#[ignore]` — skips the probe in default `cargo test`.
    assert!(
        probe_src.contains("#[ignore"),
        "SCOPE-DISCIPLINE FAILED — `on_disk_audit_probe.rs` lost its `#[ignore]` \
         annotation. The probe must be opt-in (skipped by default `cargo test`). \
         This is the first of the three real isolation gates."
    );

    // Gate 2: env-var tripwire — second opt-in even when --include-ignored is passed.
    assert!(
        probe_src.contains("LABCOLORS_ON_DISK_PROBE_ENABLED"),
        "SCOPE-DISCIPLINE FAILED — `on_disk_audit_probe.rs` lost the \
         `LABCOLORS_ON_DISK_PROBE_ENABLED` env-var check. This is the second \
         isolation gate (a CI runner that strips #[ignore] still requires explicit \
         opt-in). Both gates must be preserved."
    );

    // Gate 3: --test-threads=1 requirement documented — prevents the cross-binary
    // race (the process-local #[serial] mutex cannot close the cross-binary race;
    // this documented requirement is the actual cross-binary isolation mechanism).
    assert!(
        probe_src.contains("--test-threads=1"),
        "SCOPE-DISCIPLINE FAILED — `on_disk_audit_probe.rs` no longer documents \
         `--test-threads=1` as mandatory. This is the third isolation gate: \
         the cross-binary race between on_disk_audit_probe and s2b_baseline_guards \
         (both mutate semantic.rs) cannot be closed by #[serial] (process-local); \
         it is closed by running with --test-threads=1 per the module doc."
    );
}

/// Contract: `on_disk_audit_probe.rs` still preserves the `#[ignore]` opt-in gate
/// and the `LABCOLORS_ON_DISK_PROBE_ENABLED` env-var tripwire.
///
/// After t2 adds `#[serial]`, neither opt-in must be removed. GREEN at birth.
/// Bites on mutation: remove either from `on_disk_audit_probe.rs` → assertion fails.
#[test]
fn scope_discipline_probe_preserves_both_opt_in_gates() {
    let probe_src =
        std::fs::read_to_string(crate_root().join("tests").join("on_disk_audit_probe.rs"))
            .expect("s2b_baseline/test6b: cannot read on_disk_audit_probe.rs");

    assert!(
        probe_src.contains("#[ignore"),
        "SCOPE-DISCIPLINE FAILED — `on_disk_audit_probe.rs` lost its `#[ignore]` \
         annotation. The probe must remain opt-in (invariant: probe stays opt-in)."
    );

    assert!(
        probe_src.contains("LABCOLORS_ON_DISK_PROBE_ENABLED"),
        "SCOPE-DISCIPLINE FAILED — `on_disk_audit_probe.rs` lost the \
         `LABCOLORS_ON_DISK_PROBE_ENABLED` env-var check. Both opt-in gates \
         must be preserved after t2."
    );
}

/// Contract: `#[serial]` is NOT present in the read-only test files. Scope
/// creep: if someone adds `#[serial]` to an unrelated file, this test detects it.
///
/// Note: whether `on_disk_audit_probe.rs` carries `#[serial]` is not asserted
/// here (it is harmless and may remain), but the five files below must not.
///
/// GREEN. Bites on mutation: add `#[serial]` to any of the listed files →
/// the assertion fails.
#[test]
fn scope_discipline_serial_absent_from_readonly_test_files() {
    // `r3_byte_identity.rs` relocated to `src/r3_byte_identity_tests.rs`
    // (ADR-0001 PR-c) — see the note in test 1. The `#[serial]`-scope-creep guard
    // covers the read-only integration-test files that remain in `tests/`.
    for file in [
        "lint_parity.rs",
        "ci_enforcement.rs",
        "gate_green_smoke.rs",
        "empirical_inventory.rs",
    ] {
        let src = std::fs::read_to_string(crate_root().join("tests").join(file))
            .unwrap_or_else(|e| panic!("s2b_baseline/test6c: cannot read {file}: {e}"));
        assert!(
            !src.contains("#[serial]"),
            "SCOPE-DISCIPLINE FAILED — `{file}` contains `#[serial]`, which must \
             NOT appear in read-only test files (scope creep). `#[serial]` is \
             only appropriate on tree-mutating tests."
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 7 — supply-chain dev-dep audit (contract)
//
// REGIME: contract. Asserts zero-runtime-dep invariant and cargo audit clean.
// HOW IT BITES: GREEN at birth (serial_test is added as dev-dep in this commit).
// Mutation proof: move `serial_test` from `[dev-dependencies]` to `[dependencies]`
// in Cargo.toml → `cargo tree -e normal -i serial_test` returns non-empty →
// assertion fails (runtime dep path detected).
// ─────────────────────────────────────────────────────────────────────────────

/// Contract: `serial_test` appears only under `[dev-dependencies]` in
/// `crates/labcolors-core/Cargo.toml` — zero-runtime-dep invariant (issue #29).
///
/// GREEN at birth. Bites on mutation: move `serial_test` to `[dependencies]` →
/// this test fails (the Cargo.toml content check fails).
#[test]
fn supply_chain_serial_test_is_dev_dep_only() {
    let cargo_toml_path = crate_root().join("Cargo.toml");
    let cargo_toml = std::fs::read_to_string(&cargo_toml_path)
        .expect("s2b_baseline/test7: cannot read Cargo.toml");

    // `serial_test` must appear under [dev-dependencies], not [dependencies].
    // Strategy: find all `serial_test` occurrences and verify each appears after
    // `[dev-dependencies]` and before the next `[` section header.
    let lines: Vec<&str> = cargo_toml.lines().collect();
    let mut in_dev_deps = false;
    let mut found_in_dev = false;
    let mut found_in_runtime = false;

    for line in &lines {
        let t = line.trim();
        if t.starts_with('[') {
            in_dev_deps = t == "[dev-dependencies]";
        }
        if t.starts_with("serial_test") || t.starts_with(r#""serial_test""#) {
            if in_dev_deps {
                found_in_dev = true;
            } else {
                found_in_runtime = true;
            }
        }
    }

    assert!(
        found_in_dev,
        "SUPPLY-CHAIN FAILED — `serial_test` is not present under [dev-dependencies] \
         in `crates/labcolors-core/Cargo.toml`. It must be added as a dev-dep \
         (required for the `#[serial]` annotation in t2). Zero-runtime-dep \
         invariant (issue #29) requires it stays dev-only."
    );

    assert!(
        !found_in_runtime,
        "SUPPLY-CHAIN FAILED — `serial_test` appears under [dependencies] (runtime) \
         in `crates/labcolors-core/Cargo.toml`. It must be dev-only. \
         Zero-runtime-dep invariant (issue #29) is violated."
    );
}

/// Contract: `cargo tree -e normal -i serial_test` returns empty — serial_test
/// has no path through the normal (runtime) dependency graph.
///
/// GREEN at birth. Bites on mutation: promote serial_test to a runtime dep →
/// `cargo tree -e normal -i serial_test` returns non-empty → assertion fails.
///
/// `#[ignore]`: spawns `cargo +1.96.0 tree`, which resolves the workspace
/// dependency graph and may contend on the build-directory lock. Run manually
/// before PR:
/// ```sh
/// cargo +1.96.0 test -p labcolors-core --test s2b_baseline_guards \
///   supply_chain_serial_test_absent_from_normal_dep_tree -- --ignored
/// ```
#[test]
#[ignore = "spawns nested cargo (+1.96.0 tree) — may contend on build-dir lock. \
            Run manually before PR (see test doc)."]
fn supply_chain_serial_test_absent_from_normal_dep_tree() {
    if skip_if_toolchain_absent("supply_chain_serial_test_absent_from_normal_dep_tree") {
        return;
    }
    let output = std::process::Command::new("cargo")
        .args([
            &format!("+{CI_TOOLCHAIN}"),
            "tree",
            "-p",
            "labcolors-core",
            "-e",
            "normal",
            "-i",
            "serial_test",
        ])
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| {
            panic!("s2b_baseline/test7b: cannot run `cargo tree -e normal -i serial_test`: {e}")
        });

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    // `cargo tree -i <pkg>` exits non-zero and prints nothing if the crate is
    // not present in the specified edge-kind graph. A non-zero exit + empty
    // stdout means "not in the normal dep tree" — that is the GREEN condition.
    // If it exits zero and returns output, serial_test is in the runtime tree.
    if output.status.success() && !stdout.is_empty() {
        panic!(
            "SUPPLY-CHAIN FAILED — `cargo tree -p labcolors-core -e normal -i serial_test` \
             returned non-empty output. `serial_test` appears in the RUNTIME dependency \
             graph — it must be dev-only (issue #29 zero-runtime-dep invariant).\n\
             Output:\n{stdout}"
        );
    }
    // Either non-zero exit (not found = good) or zero exit with empty output (also fine).
    // No assertion needed for the empty case — the test passes by not panicking.
}

/// Contract: `cargo audit --deny warnings` is green — no known advisories
/// introduced by adding `serial_test` (or any other dev-dep in this diff).
///
/// GREEN at birth. Bites on mutation: introduce a dep with a known RUSTSEC
/// advisory → cargo audit exits non-zero → this test fails.
#[test]
fn supply_chain_cargo_audit_green() {
    // cargo-audit must be installed. If it is not, the test fails with a clear
    // install instruction rather than a mysterious spawn error.
    let audit_check = std::process::Command::new("cargo")
        .args(["audit", "--version"])
        .current_dir(workspace_root())
        .output();

    if audit_check.is_err() || !audit_check.unwrap().status.success() {
        // `cargo audit` not installed — skip with a clear message rather than fail.
        // The CI audit job enforces this; local absence is acceptable.
        eprintln!(
            "s2b_baseline/test7c: `cargo audit` not found — \
             install with `cargo install cargo-audit`. Skipping local audit check."
        );
        return;
    }

    let output = std::process::Command::new("cargo")
        .args(["audit", "--deny", "warnings"])
        .current_dir(workspace_root())
        .output()
        .unwrap_or_else(|e| panic!("s2b_baseline/test7c: cannot run `cargo audit`: {e}"));

    let combined = format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "SUPPLY-CHAIN FAILED — `cargo audit --deny warnings` exited non-zero. \
         Adding `serial_test` as a dev-dep introduced a known advisory or warning. \
         Review the audit output and either update the dep or add an exception.\n{combined}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 8 — CI parity on pinned toolchain (smoke)
//
// REGIME: smoke. Verifies fmt/clippy/test/audit all green locally on 1.96.0.
// HOW IT BITES: GREEN at birth.
// Mutation proof: introduce a clippy warning → clippy exits non-zero → the
// clippy assertion fails. Introduce a format deviation → fmt --check fails.
//
// These are the same checks as lint_parity.rs, but wired here as s2b baseline
// guards with the explicit "s2b gate green pre-t2" label so the CI check is
// independently identifiable.
//
// NOTE: ci_parity_fmt_check_passes and ci_parity_clippy_workspace_clean are now
// `#[ignore]`d because they spawn `cargo +1.96.0 fmt/clippy`, both of which
// trigger compilation and contend on the build-directory lock under an outer
// `cargo test --workspace`. The equivalent non-ignored guards live in
// `lint_parity.rs` (which already runs fmt+clippy without ignore). The s2b
// variants exist as independently-labelled contracts; run them manually before PR.
// ─────────────────────────────────────────────────────────────────────────────

/// CI parity smoke: `cargo +1.96.0 fmt --all --check` passes.
///
/// GREEN at birth. Bites on mutation: introduce any formatting deviation →
/// fmt exits non-zero → assertion fails.
///
/// `#[ignore]`: spawns `cargo +1.96.0 fmt`, which compiles and may contend on
/// the build-dir lock. Equivalent non-ignored check: `lint_parity.rs:
/// fmt_check_passes_on_ci_toolchain`. Run manually before PR:
/// ```sh
/// cargo +1.96.0 test -p labcolors-core --test s2b_baseline_guards \
///   ci_parity_fmt_check_passes -- --ignored
/// ```
#[test]
#[ignore = "spawns nested cargo (+1.96.0 fmt) — contends on build-dir lock; \
            equivalent non-ignored guard in lint_parity.rs. Run manually before PR."]
fn ci_parity_fmt_check_passes() {
    if skip_if_toolchain_absent("ci_parity_fmt_check_passes") {
        return;
    }
    let (ok, out) = run_cargo_check(&["fmt", "--all", "--check"]);
    assert!(
        ok,
        "CI PARITY FAILED — `cargo +{CI_TOOLCHAIN} fmt --all --check` failed. \
         The s2b baseline must be clean on the pinned CI toolchain before t2 \
         starts. Fix formatting deviations before claiming t2 is ready.\n{out}"
    );
}

/// CI parity smoke: `cargo +1.96.0 clippy --workspace --all-targets -D warnings` passes.
///
/// GREEN at birth. Bites on mutation: introduce any clippy warning →
/// clippy exits non-zero → assertion fails.
///
/// `#[ignore]`: spawns `cargo +1.96.0 clippy --workspace`, which recompiles the
/// workspace and contends on the build-dir lock. Equivalent non-ignored check:
/// `lint_parity.rs: clippy_workspace_all_targets_d_warnings_passes_on_ci_toolchain`.
/// Run manually before PR:
/// ```sh
/// cargo +1.96.0 test -p labcolors-core --test s2b_baseline_guards \
///   ci_parity_clippy_workspace_clean -- --ignored
/// ```
#[test]
#[ignore = "spawns nested cargo (+1.96.0 clippy --workspace) — recompiles workspace \
            and contends on build-dir lock; equivalent non-ignored guard in \
            lint_parity.rs. Run manually before PR."]
fn ci_parity_clippy_workspace_clean() {
    if skip_if_toolchain_absent("ci_parity_clippy_workspace_clean") {
        return;
    }
    let (ok, out) = run_cargo_check(&[
        "clippy",
        "--workspace",
        "--all-targets",
        "--",
        "-D",
        "warnings",
    ]);
    assert!(
        ok,
        "CI PARITY FAILED — `cargo +{CI_TOOLCHAIN} clippy --workspace --all-targets \
         -- -D warnings` failed. Fix all lint warnings before claiming the s2b \
         baseline is clean.\n{out}"
    );
}

/// CI parity gate: `cargo +1.96.0 test --workspace` passes (no `--include-ignored`,
/// matching the CI test job exactly).
///
/// ## Why this test remains `#[ignore]`d (structural, not pre-t2)
///
/// This test spawns `cargo test --workspace` as a subprocess. If it were NOT
/// ignored, it would itself be included in any `cargo test --workspace`
/// invocation — which would then recursively spawn another `cargo test
/// --workspace`, creating an infinite subprocess chain and consuming all
/// available resources.
///
/// The `#[ignore]` here is permanent and structural: a test that runs the
/// workspace test suite MUST be excluded from the workspace test suite it
/// runs. This is not a shortcoming of the test — it is the correct design.
///
/// ## How to use
///
/// Run this test manually before pushing the PR, as a one-shot check:
///
/// ```sh
/// cargo +1.96.0 test -p labcolors-core --test s2b_baseline_guards \
///   ci_parity_test_workspace_passes -- --ignored
/// ```
///
/// This runs the workspace check WITHOUT this test binary being part of
/// that workspace run (cargo compiles and filters at the per-binary level),
/// so there is no recursion.
///
/// Bites: break any non-ignored test in the workspace →
/// the subprocess exits non-zero → assertion fails.
#[test]
#[ignore = "structural: this test spawns `cargo test --workspace`; \
            including it in `cargo test --workspace` creates infinite subprocess \
            recursion. Run manually before PR: \
            `cargo +1.96.0 test -p labcolors-core --test s2b_baseline_guards \
            ci_parity_test_workspace_passes -- --ignored`"]
fn ci_parity_test_workspace_passes() {
    if skip_if_toolchain_absent("ci_parity_test_workspace_passes") {
        return;
    }
    // This is the exact CI invocation. It MUST NOT include `--include-ignored`
    // (the probe is #[ignore]d and must not run in CI).
    let (ok, out) = run_cargo_check(&["test", "--workspace"]);
    assert!(
        ok,
        "CI PARITY FAILED — `cargo +{CI_TOOLCHAIN} test --workspace` failed. \
         The s2b baseline must produce a green test run on the CI toolchain before \
         t2 is considered done. Investigate pre-existing failures.\n{out}"
    );
}
