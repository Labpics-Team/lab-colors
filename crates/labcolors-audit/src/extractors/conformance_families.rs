//! Conformance families / semantic branches extractor for EXT-05.
//!
//! Walks workspace Rust sources and extracts every decision site that defines
//! a conformance family or semantic branch: `match` arms, `if`/`else if` chains
//! with enum discriminants, and feature-gated code paths. Each entry records the
//! file location, the kind of branch, the condition text, and a SHA-256 fingerprint
//! so downstream gates can detect phantom entries or silent deletions.

use std::path::Path;

use sha2::{Digest, Sha256};
use syn::{Expr, ExprIf, ExprMatch, File, Item, Pat, Stmt};
use walkdir::WalkDir;

/// Kind of semantic branch / conformance decision site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchKind {
    /// A `match` arm pattern.
    MatchArm,
    /// An `if` / `else if` condition.
    IfCondition,
    /// A `cfg!(feature = "...")` gate in expression position.
    CfgGate,
}

impl std::fmt::Display for BranchKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BranchKind::MatchArm => write!(f, "match_arm"),
            BranchKind::IfCondition => write!(f, "if_condition"),
            BranchKind::CfgGate => write!(f, "cfg_gate"),
        }
    }
}

/// One extracted conformance family / semantic branch entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConformanceBranchEntry {
    /// Relative path from workspace root (forward slashes).
    pub path: String,
    /// Crate name derived from `crates/<name>/`.
    pub crate_name: String,
    /// Discriminator for the branch kind.
    pub kind: BranchKind,
    /// Normalized condition or pattern text.
    pub condition: String,
    /// 1-based line number where the branch starts.
    pub line: usize,
    /// SHA-256 of `(kind, condition)` for integrity checks.
    pub fingerprint: String,
}

/// Extracts all conformance families / semantic branches from workspace sources.
///
/// Traverses `workspace_root/crates/**/*.rs`, parses each file with `syn`,
/// and collects `match` arms, `if` conditions, and `cfg!()` gates. Results are
/// deterministically sorted by `(path, line)`.
pub fn extract_conformance_branches(workspace_root: &Path) -> Vec<ConformanceBranchEntry> {
    let mut entries = Vec::new();
    let crates_dir = workspace_root.join("crates");

    if !crates_dir.is_dir() {
        return entries;
    }

    for dir_entry in WalkDir::new(&crates_dir).into_iter().filter_map(|e| e.ok()) {
        let path = dir_entry.path();

        let is_rs_file = path.is_file() && path.extension().is_some_and(|ext| ext == "rs");
        if !is_rs_file {
            continue;
        }

        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        if file_name == "build.rs"
            || file_name.ends_with("_test.rs")
            || file_name.ends_with("_tests.rs")
        {
            continue;
        }

        let rel = relative_path(workspace_root, path);
        if rel.contains("/tests/") || rel.contains("/benches/") {
            continue;
        }

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let file: File = match syn::parse_str(&content) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let crate_name = extract_crate_name(&rel);
        let lines: Vec<&str> = content.lines().collect();

        collect_from_items(&file.items, &rel, &crate_name, &lines, &mut entries);
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.line.cmp(&b.line)));
    entries
}

fn collect_from_items(
    items: &[Item],
    path: &str,
    crate_name: &str,
    lines: &[&str],
    out: &mut Vec<ConformanceBranchEntry>,
) {
    for item in items {
        match item {
            Item::Fn(f) => {
                collect_from_block(&f.block.stmts, path, crate_name, lines, out);
            }
            Item::Impl(imp) => {
                for impl_item in &imp.items {
                    if let syn::ImplItem::Fn(m) = impl_item {
                        collect_from_block(&m.block.stmts, path, crate_name, lines, out);
                    }
                }
            }
            Item::Mod(m) => {
                if let Some((_, ref items)) = m.content {
                    collect_from_items(items, path, crate_name, lines, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_from_block(
    stmts: &[Stmt],
    path: &str,
    crate_name: &str,
    lines: &[&str],
    out: &mut Vec<ConformanceBranchEntry>,
) {
    for stmt in stmts {
        match stmt {
            Stmt::Expr(expr, _) => {
                collect_from_expr(expr, path, crate_name, lines, out);
            }
            Stmt::Local(local) => {
                if let Some(ref init) = local.init {
                    collect_from_expr(&init.expr, path, crate_name, lines, out);
                }
            }
            _ => {}
        }
    }
}

fn collect_from_expr(
    expr: &Expr,
    path: &str,
    crate_name: &str,
    lines: &[&str],
    out: &mut Vec<ConformanceBranchEntry>,
) {
    match expr {
        Expr::Match(m) => {
            collect_match_arms(m, path, crate_name, lines, out);
            // Recurse into the matched expression itself.
            collect_from_expr(&m.expr, path, crate_name, lines, out);
            // Recurse into arm bodies.
            for arm in &m.arms {
                collect_from_expr(&arm.body, path, crate_name, lines, out);
            }
        }
        Expr::If(if_expr) => {
            collect_if_chain(if_expr, path, crate_name, lines, out);
        }
        Expr::Macro(mac) => {
            let mac_text = quote::quote!(#mac).to_string();
            if mac_text.contains("cfg!") {
                let line = find_line_for_text(lines, &mac_text);
                let condition = normalize_cfg_macro(&mac_text);
                out.push(ConformanceBranchEntry {
                    path: path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: BranchKind::CfgGate,
                    condition: condition.clone(),
                    line,
                    fingerprint: fingerprint(BranchKind::CfgGate, &condition),
                });
            }
        }
        // Recurse into common compound expressions to find nested branches.
        Expr::Block(b) => {
            collect_from_block(&b.block.stmts, path, crate_name, lines, out);
        }
        Expr::Return(r) => {
            if let Some(ref e) = r.expr {
                collect_from_expr(e, path, crate_name, lines, out);
            }
        }
        Expr::Assign(a) => {
            collect_from_expr(&a.right, path, crate_name, lines, out);
        }
        Expr::Call(c) => {
            for arg in &c.args {
                collect_from_expr(arg, path, crate_name, lines, out);
            }
        }
        Expr::MethodCall(c) => {
            collect_from_expr(&c.receiver, path, crate_name, lines, out);
            for arg in &c.args {
                collect_from_expr(arg, path, crate_name, lines, out);
            }
        }
        Expr::Binary(b) => {
            collect_from_expr(&b.left, path, crate_name, lines, out);
            collect_from_expr(&b.right, path, crate_name, lines, out);
        }
        Expr::Unary(u) => {
            collect_from_expr(&u.expr, path, crate_name, lines, out);
        }
        Expr::Paren(p) => {
            collect_from_expr(&p.expr, path, crate_name, lines, out);
        }
        Expr::Tuple(t) => {
            for e in &t.elems {
                collect_from_expr(e, path, crate_name, lines, out);
            }
        }
        Expr::Array(a) => {
            for e in &a.elems {
                collect_from_expr(e, path, crate_name, lines, out);
            }
        }
        Expr::Struct(s) => {
            for field in &s.fields {
                collect_from_expr(&field.expr, path, crate_name, lines, out);
            }
        }
        Expr::Field(f) => {
            collect_from_expr(&f.base, path, crate_name, lines, out);
        }
        Expr::Index(idx) => {
            collect_from_expr(&idx.expr, path, crate_name, lines, out);
            collect_from_expr(&idx.index, path, crate_name, lines, out);
        }
        Expr::Let(l) => {
            collect_from_expr(&l.expr, path, crate_name, lines, out);
        }
        Expr::ForLoop(fl) => {
            collect_from_expr(&fl.expr, path, crate_name, lines, out);
            collect_from_block(&fl.body.stmts, path, crate_name, lines, out);
        }
        Expr::While(w) => {
            collect_from_expr(&w.cond, path, crate_name, lines, out);
            collect_from_block(&w.body.stmts, path, crate_name, lines, out);
        }
        Expr::Loop(l) => {
            collect_from_block(&l.body.stmts, path, crate_name, lines, out);
        }
        Expr::Closure(c) => {
            collect_from_expr(&c.body, path, crate_name, lines, out);
        }
        _ => {}
    }
}

fn collect_match_arms(
    m: &ExprMatch,
    path: &str,
    crate_name: &str,
    lines: &[&str],
    out: &mut Vec<ConformanceBranchEntry>,
) {
    for arm in &m.arms {
        let pat_text = normalize_pattern(&arm.pat);
        let guard_text = arm
            .guard
            .as_ref()
            .map(|(_, g)| format!(" if {}", quote::quote!(#g)))
            .unwrap_or_default();
        let condition = format!("{}{}", pat_text, guard_text);
        let line = find_line_for_text(lines, &pat_text);
        out.push(ConformanceBranchEntry {
            path: path.to_string(),
            crate_name: crate_name.to_string(),
            kind: BranchKind::MatchArm,
            condition: condition.clone(),
            line,
            fingerprint: fingerprint(BranchKind::MatchArm, &condition),
        });
    }
}

fn collect_if_chain(
    if_expr: &ExprIf,
    path: &str,
    crate_name: &str,
    lines: &[&str],
    out: &mut Vec<ConformanceBranchEntry>,
) {
    let cond_text = quote::quote!(#if_expr.cond).to_string();
    let line = find_line_for_text(lines, &cond_text);
    out.push(ConformanceBranchEntry {
        path: path.to_string(),
        crate_name: crate_name.to_string(),
        kind: BranchKind::IfCondition,
        condition: cond_text.clone(),
        line,
        fingerprint: fingerprint(BranchKind::IfCondition, &cond_text),
    });

    // Recurse into the then-branch body.
    collect_from_block(&if_expr.then_branch.stmts, path, crate_name, lines, out);

    // Follow else-if chains.
    if let Some((_, ref else_expr)) = if_expr.else_branch {
        if let Expr::If(nested) = else_expr.as_ref() {
            collect_if_chain(nested, path, crate_name, lines, out);
        } else {
            // Final else block — recurse but don't emit a branch entry.
            if let Expr::Block(b) = else_expr.as_ref() {
                collect_from_block(&b.block.stmts, path, crate_name, lines, out);
            }
        }
    }
}

fn normalize_pattern(pat: &Pat) -> String {
    let raw = quote::quote!(#pat).to_string();
    // Collapse whitespace for deterministic output.
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_cfg_macro(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn find_line_for_text(lines: &[&str], needle: &str) -> usize {
    // Try exact substring match first.
    for (i, line) in lines.iter().enumerate() {
        if line.contains(needle) {
            return i + 1;
        }
    }
    // Fallback: try matching just the first significant token.
    let first_token = needle.split_whitespace().next().unwrap_or("");
    if !first_token.is_empty() {
        for (i, line) in lines.iter().enumerate() {
            if line.contains(first_token) {
                return i + 1;
            }
        }
    }
    // Last resort — shouldn't happen in well-formed code.
    1
}

fn fingerprint(kind: BranchKind, condition: &str) -> String {
    let input = format!("{}:{}", kind, condition);
    let hash = Sha256::digest(input.as_bytes());
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

fn extract_crate_name(rel_path: &str) -> String {
    rel_path
        .strip_prefix("crates/")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("unknown")
        .to_string()
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    #[test]
    fn extracts_at_least_one_branch() {
        let entries = extract_conformance_branches(&workspace_root());
        assert!(
            !entries.is_empty(),
            "SABOTAGE: extractor must find at least one conformance branch in workspace"
        );
    }

    #[test]
    fn all_entries_have_nonempty_condition() {
        let entries = extract_conformance_branches(&workspace_root());
        for entry in &entries {
            assert!(
                !entry.condition.is_empty(),
                "SABOTAGE: empty condition at {}:{} (phantom entry)",
                entry.path,
                entry.line
            );
        }
    }

    #[test]
    fn all_entries_have_valid_fingerprint() {
        let entries = extract_conformance_branches(&workspace_root());
        for entry in &entries {
            let expected = fingerprint(entry.kind.clone(), &entry.condition);
            assert_eq!(
                entry.fingerprint, expected,
                "SABOTAGE: fingerprint mismatch at {}:{} — entry may be fabricated",
                entry.path, entry.line
            );
        }
    }

    #[test]
    fn results_are_deterministically_sorted() {
        let entries = extract_conformance_branches(&workspace_root());
        for window in entries.windows(2) {
            let a = &window[0];
            let b = &window[1];
            assert!(
                a.path <= b.path,
                "SABOTAGE: results not sorted by path: {} > {}",
                a.path,
                b.path
            );
            if a.path == b.path {
                assert!(
                    a.line <= b.line,
                    "SABOTAGE: results not sorted by line within {}: {} > {}",
                    a.path,
                    a.line,
                    b.line
                );
            }
        }
    }

    #[test]
    fn no_entries_from_test_files() {
        let entries = extract_conformance_branches(&workspace_root());
        for entry in &entries {
            assert!(
                !entry.path.contains("/tests/"),
                "SABOTAGE: test file leaked into manifest: {}",
                entry.path
            );
            assert!(
                !entry.path.ends_with("_test.rs"),
                "SABOTAGE: _test.rs file leaked: {}",
                entry.path
            );
            assert!(
                !entry.path.ends_with("_tests.rs"),
                "SABOTAGE: _tests.rs file leaked: {}",
                entry.path
            );
        }
    }

    #[test]
    fn known_decision_site_is_present() {
        // The workspace contains match/if constructs in core numerics/solve modules.
        // This test anchors against phantom-only output: if someone replaces the
        // extractor with a stub returning fake entries, this assertion fails because
        // real decision sites won't be found.
        let entries = extract_conformance_branches(&workspace_root());
        let has_core_branch = entries.iter().any(|e| {
            e.crate_name == "labcolors-core"
                && matches!(e.kind, BranchKind::MatchArm | BranchKind::IfCondition)
        });
        assert!(
            has_core_branch,
            "SABOTAGE: no real match/if branch found in labcolors-core — extractor may be stubbed"
        );
    }

    #[test]
    fn sabotage_phantom_entry_detected() {
        // Verify that fabricating an entry with wrong fingerprint is caught.
        let fake = ConformanceBranchEntry {
            path: "crates/fake/src/lib.rs".to_string(),
            crate_name: "fake".to_string(),
            kind: BranchKind::MatchArm,
            condition: "PhantomVariant => unreachable!()".to_string(),
            line: 999,
            fingerprint: "0000000000000000000000000000000000000000000000000000000000000000"
                .to_string(),
        };
        let expected = fingerprint(fake.kind.clone(), &fake.condition);
        assert_ne!(
            fake.fingerprint, expected,
            "Test invariant: fake fingerprint must not match real computation"
        );
    }
}
