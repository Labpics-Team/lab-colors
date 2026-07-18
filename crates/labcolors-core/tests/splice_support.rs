//! Atomic source splice used only inside an isolated temporary workspace.

use std::path::Path;

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
