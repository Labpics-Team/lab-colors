//! Shared helper: panic-safe RestoreGuard and atomic splice for on-disk probe tests.
//!
//! # Why this module exists
//!
//! Two integration-test files (`on_disk_audit_probe.rs` and
//! `s2b_baseline_guards.rs`) both implement:
//!   * A backup + atomic-rename splice into `semantic.rs`
//!   * A Drop-based restore guard that reverts the splice
//!
//! Those two implementations diverged: the probe's `RestoreGuard::drop` used
//! `.expect()` on every fs operation (panicking in Drop → process abort during
//! test unwind), while the baseline guard used `let _ =` (swallowing errors,
//! preferable during unwind). That divergence was itself a confirmed High defect
//! (DRY/SRP fracture with already-diverged semantics).
//!
//! This module closes the class: one canonical implementation, panic-safe by
//! design, used by both callers. The rule: **Drop must never panic**. If a
//! restore op fails during unwind, log to stderr and leave the backup for human
//! recovery — do not re-panic.
//!
//! # Usage (in integration test files)
//!
//! ```rust,ignore
//! // At the top of the test file:
//! #[path = "splice_support.rs"]
//! mod splice_support;
//! use splice_support::{splice_into, RestoreGuard};
//! ```
//!
//! # Atomicity guarantee
//!
//! Both `splice_into` and `RestoreGuard::drop` use write-to-tmp + rename-over-target
//! semantics. The real `semantic.rs` is never left in a half-written state:
//! at any instant it is either the original (pre-rename) or the fully-written
//! spliced/restored version (post-rename). A crash between write and rename
//! leaves `*.splice_tmp` / `*.restore_tmp` as litter — both are safe to remove.
//!
//! # SIGKILL window
//!
//! A SIGKILL between the rename completing and the Drop guard running leaves
//! `semantic.rs` spliced with the backup file as the only recovery. This is
//! the unavoidable minimum window; the pre-splice backup ensures recoverability
//! and `RestoreGuard` is registered immediately after backup creation.

use std::path::{Path, PathBuf};

/// A Drop guard that restores `target` from `backup` on drop (even on panic).
///
/// The restore is **panic-safe**: if any fs operation fails during Drop, the
/// error is printed to stderr and the remaining operations are attempted, but
/// no panic is raised. This is correct for Drop: a second panic during unwind
/// aborts the process and skips all remaining cleanup. The backup file exists
/// for manual recovery regardless of whether the rename succeeded.
///
/// # Invariant upheld
///
/// `target` is always either the original content or the fully-written restore
/// content — never truncated — because we write to a `*.restore_tmp` sibling
/// first, then atomically rename over `target`.
///
/// Currently used by s2b_baseline_guards.rs (on_disk_audit_probe.rs uses
/// temp-dir isolation instead). The #[allow(dead_code)] covers scenarios where
/// a particular test file doesn't use it.
#[allow(dead_code)]
pub struct RestoreGuard {
    pub target: PathBuf,
    pub backup: PathBuf,
}

impl Drop for RestoreGuard {
    fn drop(&mut self) {
        if !self.backup.exists() {
            // Nothing to restore (backup was already cleaned up or never created).
            return;
        }
        // Atomic restore: copy backup → *.restore_tmp, then rename over target.
        //
        // PANIC POLICY: do NOT panic in Drop. If an op fails during unwind a
        // second panic aborts the process and skips remaining cleanup. We log
        // to stderr so a human can recover using the backup file.
        let tmp = self.target.with_extension("rs.restore_tmp");
        if let Err(e) = std::fs::copy(&self.backup, &tmp) {
            eprintln!(
                "[RestoreGuard] ERROR: failed to copy backup {:?} → {:?}: {e}\n\
                 Manual recovery: `cp {:?} {:?}`",
                self.backup, tmp, self.backup, self.target,
            );
            return; // Do not attempt rename without the tmp in place.
        }
        if let Err(e) = std::fs::rename(&tmp, &self.target) {
            eprintln!(
                "[RestoreGuard] ERROR: failed to rename {:?} → {:?}: {e}\n\
                 Manual recovery: `cp {:?} {:?}`",
                tmp, self.target, self.backup, self.target,
            );
            // Leave litter (tmp exists, target still spliced); backup is the recovery path.
            return;
        }
        if let Err(e) = std::fs::remove_file(&self.backup) {
            // Non-fatal: the backup is harmless litter. Log and continue.
            eprintln!(
                "[RestoreGuard] WARNING: failed to remove backup {:?}: {e} \
                 (harmless; remove manually)",
                self.backup,
            );
        }
    }
}

/// Splice `snippet` into `target_path` after the leading `//!` module-doc comment
/// block using an **atomic rename**, so the file is never left half-written.
///
/// # Algorithm
///
/// 1. Read `target_path`.
/// 2. Walk bytes to find the first line that is neither blank nor `//!` — the
///    insertion point (before the first item).
/// 3. Construct the spliced string: original[..insert_byte] + snippet + "\n" +
///    original[insert_byte..].
/// 4. Write to `*.splice_tmp` sibling.
/// 5. Rename `*.splice_tmp` atomically over `target_path`.
///
/// # Panics
///
/// Panics if reading, writing, or renaming fails — this is called before the
/// test body runs, so there is no unwind in progress and a panic is safe here.
pub fn splice_into(target_path: &Path, snippet: &str) {
    let original = std::fs::read_to_string(target_path)
        .unwrap_or_else(|e| panic!("splice_support: cannot read {:?}: {e}", target_path));

    let insert_byte = find_insert_byte(original.as_bytes());

    let mut spliced = String::with_capacity(original.len() + snippet.len() + 2);
    spliced.push_str(&original[..insert_byte]);
    spliced.push_str(snippet);
    spliced.push('\n');
    spliced.push_str(&original[insert_byte..]);

    let tmp = target_path.with_extension("rs.splice_tmp");
    std::fs::write(&tmp, &spliced)
        .unwrap_or_else(|e| panic!("splice_support: cannot write splice_tmp {:?}: {e}", tmp));
    std::fs::rename(&tmp, target_path).unwrap_or_else(|e| {
        panic!(
            "splice_support: cannot rename {:?} → {:?}: {e}",
            tmp, target_path
        )
    });
}

/// Walk bytes to find the insertion point: the start of the first line that is
/// neither blank nor an inner-doc comment (`//!`). Handles both LF and CRLF.
fn find_insert_byte(bytes: &[u8]) -> usize {
    let mut pos = 0usize;
    while pos < bytes.len() {
        let line_end = bytes[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|off| pos + off)
            .unwrap_or(bytes.len());
        let line_bytes = if line_end > pos && bytes[line_end - 1] == b'\r' {
            &bytes[pos..line_end - 1]
        } else {
            &bytes[pos..line_end]
        };
        let line = std::str::from_utf8(line_bytes).unwrap_or("");
        let t = line.trim_start();
        if !t.is_empty() && !t.starts_with("//!") {
            return pos;
        }
        pos = if line_end < bytes.len() {
            line_end + 1
        } else {
            bytes.len()
        };
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_point_skips_doc_comment_lines() {
        let src = b"//! module doc\n//! second line\nuse std;\n";
        assert_eq!(find_insert_byte(src), 31); // "use std;\n" starts at byte 31
    }

    #[test]
    fn insert_point_handles_empty_lines_in_doc_block() {
        let src = b"//! doc\n\n//! after blank\nfn foo() {}\n";
        // Blank line at byte 8 is neither `//!` nor non-empty-non-doc, but
        // the walk treats it as "blank" (t.is_empty()) so it continues.
        // "fn foo()" starts at byte 25.
        assert_eq!(find_insert_byte(src), 25);
    }

    #[test]
    fn insert_point_at_start_when_no_doc_comment() {
        let src = b"use std;\n";
        assert_eq!(find_insert_byte(src), 0);
    }

    #[test]
    fn insert_point_at_end_when_all_doc_comments() {
        let src = b"//! only docs\n";
        assert_eq!(find_insert_byte(src), 14); // len
    }

    #[test]
    fn insert_point_handles_crlf() {
        // CRLF line endings (Windows source files).
        let src = b"//! doc\r\nuse std;\r\n";
        assert_eq!(find_insert_byte(src), 9);
    }
}
