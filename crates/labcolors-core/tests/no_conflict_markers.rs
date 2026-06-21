//! Merge-conflict-marker guard — repository-hygiene regime.
//!
//! BUG CLASS this closes: an unresolved `git merge` / `git stash pop` conflict
//! is committed because *nothing asserts its absence*. When the markers land in
//! a `.rs` file they sometimes — but only sometimes — surface as a downstream
//! "unclosed delimiter" compile error; when they land in a `.md`, `.toml`,
//! `.json`, `.yaml`, or between two functions, the file still parses and the
//! markers commit **silently** (`git diff --stat` is empty because index ==
//! worktree, so `git status` invites a blind commit). This guard turns that
//! whole class into a deterministic, named RED — independent of file type and
//! independent of whether the conflict happens to also break a compiler.
//!
//! It is a pure-`std`, test-only consumer at the top of the dependency graph
//! (Clean: it depends on nothing in `src`; nothing in `src` depends on it). Zero
//! new deps — `labcolors-core` stays zero-dep (issue #29). It walks the workspace
//! tree hermetically (keyed off `CARGO_MANIFEST_DIR`, no CWD assumptions, no
//! shelling out to `git`), reads each candidate text file, and asserts no line is
//! a canonical conflict marker.
//!
//! A "conflict marker" is detected structurally, the same way `git` writes them:
//! a line whose FIRST non-whitespace run is exactly seven `<`, `=`, or `>`
//! characters, followed by end-of-line or a single space then a label. This is
//! the verbatim shape of `<<<<<<< `, `=======`, `>>>>>>> `. Detecting the exact
//! shape (not a substring) is what lets this guard scan its OWN source without a
//! false positive: every reference to a marker token below lives inside a string
//! literal, indented and quote-prefixed, so it never begins a line as a bare
//! 7-char run.

use std::path::{Path, PathBuf};

/// Workspace root = two directories above this crate (`crates/labcolors-core/`).
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root must resolve from CARGO_MANIFEST_DIR/../..")
}

/// Directory names pruned from the walk: build output, VCS internals, vendored
/// JS deps, and the generated wasm package. None are tracked source we author;
/// scanning them would be slow and could trip on third-party fixtures.
const PRUNED_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "pkg", // wasm-pack output (generated, gitignored)
    "dist",
];

/// File extensions whose bytes we scan. A conflict marker is only meaningful in
/// human-authored text; binary assets (images, fonts, wasm) are skipped. This is
/// an allowlist by design — a new text type is a deliberate, reviewable addition,
/// and an unknown binary type is never read as UTF-8.
const SCANNED_EXTENSIONS: &[&str] = &[
    "rs", "toml", "md", "json", "yaml", "yml", "js", "mjs", "cjs", "ts", "tsx", "css", "html",
    "sh", "txt", "lock",
];

/// True when `name` is a marker line of exactly `marker` (one of `<`, `=`, `>`)
/// repeated 7 times, optionally followed by a space + label. Mirrors git's own
/// `conflict-marker-size` default of 7.
fn is_marker_line(line: &str, marker: char) -> bool {
    let trimmed = line.trim_start();
    let run: String = trimmed.chars().take_while(|&c| c == marker).collect();
    if run.len() != 7 {
        return false;
    }
    // Char immediately after the 7-run must be end-of-line or a single space —
    // the exact shape git emits. Anything else (e.g. an 8th char, or `=======x`)
    // is not a real marker and is left alone.
    match trimmed[run.len()..].chars().next() {
        None => true,
        Some(' ') => true,
        Some(_) => false,
    }
}

/// Any of the three canonical conflict markers.
fn conflict_marker(line: &str) -> bool {
    is_marker_line(line, '<') || is_marker_line(line, '=') || is_marker_line(line, '>')
}

/// Recursively collect candidate text files under `dir`, pruning build/VCS dirs.
fn collect_text_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return, // unreadable dir is not our concern here.
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if PRUNED_DIRS.contains(&name) {
                continue;
            }
            collect_text_files(&path, out);
        } else if file_type.is_file() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_ascii_lowercase)
                .unwrap_or_default();
            if SCANNED_EXTENSIONS.contains(&ext.as_str()) {
                out.push(path);
            }
        }
    }
}

#[test]
fn no_unresolved_conflict_markers_in_tree() {
    let root = workspace_root();
    let mut files = Vec::new();
    collect_text_files(&root, &mut files);

    // Floor: the walk must see real source, or the guard is vacuous
    // (green-from-birth). This crate's own `src/` and `tests/` guarantee a
    // non-trivial corpus.
    assert!(
        files.len() > 5,
        "conflict-marker guard scanned only {} file(s) under {} — the walk is \
         mis-scoped (a near-empty scan proves nothing).",
        files.len(),
        root.display()
    );

    let mut offenders: Vec<String> = Vec::new();
    for path in &files {
        // A non-UTF-8 read just means "not the text we guard" — skip, don't panic.
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for (idx, line) in text.lines().enumerate() {
            if conflict_marker(line) {
                let rel = path.strip_prefix(&root).unwrap_or(path);
                offenders.push(format!("{}:{}", rel.display(), idx + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "CONFLICT-MARKER GUARD FAILED — {} line(s) carry an unresolved \
         git conflict marker (`<<<<<<<` / `=======` / `>>>>>>>`). Resolve the \
         conflict and remove every marker before committing:\n  {}",
        offenders.len(),
        offenders.join("\n  ")
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// RED-proof (INV-4): prove the detector actually bites. The matcher is exercised
// on synthetic lines — a green-from-birth matcher that always returned `false`
// would pass `no_unresolved_conflict_markers_in_tree` vacuously, so these probes
// flip the matcher GREEN→RED on each canonical marker shape and assert it stays
// GREEN on the look-alikes that must NOT trip it (incl. this guard's own
// marker-token references).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn matcher_flags_canonical_markers_and_ignores_lookalikes() {
    // Canonical markers git emits — MUST be flagged.
    assert!(conflict_marker("<<<<<<< HEAD"));
    assert!(conflict_marker("=======")); // bare, no label
    assert!(conflict_marker(">>>>>>> Stashed changes"));
    assert!(conflict_marker("<<<<<<< Updated upstream"));
    // Indented markers (git can emit these inside nested contexts) — MUST flag.
    assert!(conflict_marker("    ======="));

    // Look-alikes that must NOT be flagged:
    // - a markdown horizontal rule / table separator.
    assert!(!conflict_marker("|------|------|"));
    assert!(!conflict_marker("---"));
    // - a comment arrow / shorter or longer run.
    assert!(!conflict_marker("====== six")); // 6, not 7
    assert!(!conflict_marker("======== eight")); // 8, not 7
    assert!(!conflict_marker("=======x")); // 7 then non-space
    // - this guard's OWN doc/string references to markers (quoted, indented) —
    //   they never begin a line as a bare 7-char run, so they stay GREEN.
    assert!(!conflict_marker(
        "//! a line whose FIRST non-whitespace run is exactly seven `<`,"
    ));
    assert!(!conflict_marker(
        "    assert!(conflict_marker(\"<<<<<<< HEAD\"));"
    ));
}
