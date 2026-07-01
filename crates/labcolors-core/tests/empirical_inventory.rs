//! Empirical-inventory gate — R4 hygiene/governance regime (epic: empirical-inventory-gate).
//!
//! BUG CLASS this guards: *a numeric perceptual-POLICY constant lands with no
//! paper-trail* — no `// NEEDS-SCIENCE`/`// GROUNDED` marker and no row in the
//! SSOT `docs/empirical-inventory.md`. This is a governance regime
//! (R4): it asserts every policy literal is **marked + inventoried**. It does
//! NOT assert math (R1), derivation-identity (R2), or behavioural non-drift
//! (R3) — it never reads the magnitude of a value, only the presence of its
//! paper-trail (INV-7: regime separation).
//!
//! The gate is a pure-`std`, test-only consumer at the very top of the
//! dependency graph (Clean: it depends on `src`; nothing in `src` depends on
//! it). Zero new deps — `labcolors-core` stays zero-dep (issue #29).
//!
//! Layout of this file:
//! * `scan`     — the pure-std scanner (source → detected POLICY magnitudes:
//!   type-agnostic consts, `DjMagnitude` anchors, and `fn default()` field
//!   literals; non-perceptual numerics excluded by named allowlist).
//! * `ssot`     — the pure-std inventory (SSOT markdown → rows incl. value+module).
//! * `#[test]`s — GATE 1/2/3, join-key sanity, standards-exclusion,
//!   red_proof_audit_probe.
//!
//! Both detection paths are shared verbatim with `red_proof_audit_probe` so the
//! RED-proof is a true mirror of production behaviour (INV-4): GATE-1's scanner
//! via `scan_source`/`unmarked_after_splice`, and GATE-2's value/module-drift
//! comparison via `value_or_module_drift`. Neither gate's detection logic is
//! inlined-only, so a mutation to either is caught by the probe (not
//! green-from-birth).

use std::collections::BTreeSet;

mod common;

// ─────────────────────────────────────────────────────────────────────────────
// Audit surface — the 6 perceptual modules the detector scans.
// ─────────────────────────────────────────────────────────────────────────────

const PERCEPTUAL_MODULES: [&str; 6] = [
    "semantic.rs",
    "scale.rs",
    "sentiment.rs",
    "neutral.rs",
    "lpc.rs",
    "lcs.rs",
];

/// STANDARD const names — excluded **by construction** (INV-3). These are pinned
/// by an upstream standard / derivation-identity / pure numeric EPS, so they get
/// NO marker and NO inventory row. The detector subtracts this allowlist before
/// it ever requires a paper-trail, so a standard can never be flagged or
/// required-as-a-row.
const NUMERIC_METHOD_ALLOWLIST: &[&str] = &[
    // Hellwig 2022 H-K term (lpc.rs) — standard.
    "HK_CHROMA_EXPONENT",
    // APCA / WCAG standard scaling + identities (lpc.rs).
    "LC_SCALE",
    "DELTA_Y_MIN",
    // Derivation-identity (R2), recomputed not policy (sentiment.rs).
    "S_PERC_MIN",
    // Pure numeric epsilons — non-perceptual.
    "RATIO_BISECT_EPS",
    "RATIO_EPS",
    "FLOOR_EPS",
    "GAMUT_EPS",
    // Derived from a WCAG standard ratio, not an independent policy literal.
    "POLARITY_FLOOR_RATIO",
];

/// Known standard *names* that must NEVER appear as an inventory row — the
/// observable half of INV-3 (exclusion is enforced, not merely implied by
/// scoping). Checked directly against the SSOT rows.
const FORBIDDEN_STANDARD_ROW_NAMES: &[&str] = &[
    "HK_CHROMA_EXPONENT", // Hellwig 0.587
    "LC_SCALE",           // APCA
    "DELTA_Y_MIN",        // APCA
    "S_PERC_MIN",         // derivation-identity
    "RATIO_BISECT_EPS",   // numeric EPS
    "AA_TEXT_RATIO",      // WCAG
    "L_A",                // CIECAM16 L_A=64
    "YB",                 // UCS / Yb=20
    "D65",                // illuminant
];

// ─────────────────────────────────────────────────────────────────────────────
// Path resolution — делегировано в `common` (без дублирования между файлами).
// ─────────────────────────────────────────────────────────────────────────────

use common::{crate_root, inventory_path, src_dir};

fn read_module(file: &str) -> String {
    let path = src_dir().join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read perceptual module {}: {e}", path.display()))
}

// ─────────────────────────────────────────────────────────────────────────────
// STRUCTURAL (non-policy) const names — excluded from the marker requirement by
// name (INV-3, the same by-construction mechanism as the standards allowlist).
//
// These are NUMERIC consts the type-agnostic scanner *sees* but that are NOT
// perceptual-policy magnitudes: they are structural/algorithmic knobs (a cache
// capacity, an iteration count). They get NO marker and NO inventory row. The
// scanner still parses them so that a *future* non-`f64` policy threshold (an
// `f32`/`u32`/`i32` magnitude) that is NOT on this list goes RED and forces a
// human decision — closing the "type-gate bypass" class. Adding a name here is a
// deliberate, reviewable assertion that the const is non-perceptual.
// ─────────────────────────────────────────────────────────────────────────────
const STRUCTURAL_NONPOLICY_ALLOWLIST: &[&str] = &[
    // Upper bound on live curve-plan cache entries — a memory-footprint bound,
    // not a perceptual magnitude (semantic.rs).
    "CURVE_PLAN_CACHE_CAP",
    // Max curve-plan refinements after the achromatic probe — an iteration count,
    // not a perceptual magnitude (semantic.rs).
    "CURVE_REFINE_STEPS",
];

// ─────────────────────────────────────────────────────────────────────────────
// scan — the pure-std scanner. A detected POLICY magnitude is any numeric-bearing
// site across the audit surface that is NOT excluded by an allowlist:
//   1. `const NAME: <any-type> = …`         — type-agnostic (closes the type-gate).
//   2. `const NAME: DjMagnitude = …::new(…)` — a two-`f64` perceptual anchor.
//   3. `field: <float-literal>` inside a `fn default()` body — a Default-field
//      magnitude, synthesised into a stable join-name `<MODULE>_DEFAULT_<FIELD>`.
// Non-`f64` *standard* consts go through `NUMERIC_METHOD_ALLOWLIST`; non-`f64`
// *structural* consts through `STRUCTURAL_NONPOLICY_ALLOWLIST`. Everything else
// must carry a paper-trail. Shared verbatim by the gate and the RED-proof (INV-4).
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DetectedConst {
    module: String,
    /// 1-based line number of the detected site in the module.
    line: usize,
    name: String,
    /// The source magnitude(s), normalised for comparison against the SSOT `value`
    /// column: a single literal (`"0.10"`) or a `DjMagnitude` pair (`"7.93, 17.67"`).
    /// Empty only if a value could not be extracted (never for a real site).
    value: String,
    /// Whether a `// NEEDS-SCIENCE` or `// GROUNDED` marker sits within a 2-line
    /// lookback above the site.
    has_marker: bool,
    /// Whether the marker (if any) is the provisional `// NEEDS-SCIENCE` kind.
    needs_science: bool,
    /// The verbatim marker comment line (trimmed), if any — so GATE-4 can verify
    /// the *citation text* of a `// GROUNDED` marker, not merely its presence.
    /// Empty when `has_marker` is false.
    marker_line: String,
}

/// The kind of a parsed const declaration, so the scanner can branch on the
/// declared type without a second pass.
enum ConstDecl {
    /// A plain numeric const: `(name, declared_type, raw_value)`.
    Numeric {
        name: String,
        ty: String,
        value: String,
    },
    /// A `DjMagnitude` anchor: `(name, "light, dark")`.
    DjAnchor { name: String, value: String },
}

/// Strip leading `const` / `pub const` / `pub(crate) const`, returning the rest.
fn strip_const_prefix(t: &str) -> Option<&str> {
    t.strip_prefix("pub(crate) const ")
        .or_else(|| t.strip_prefix("pub const "))
        .or_else(|| t.strip_prefix("const "))
}

/// Take a leading Rust identifier off `s`.
fn take_ident(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

/// Normalise a `value` column so a single literal (`0.10`) or a comma-separated
/// `DjMagnitude` pair (`7.93, 17.67`) compares equal regardless of separator
/// spacing or suffixes. Splits on `,`, normalises each token, rejoins with
/// `", "` — matching exactly how the scanner builds a `DjAnchor` value.
fn normalise_value(raw: &str) -> String {
    raw.split(',')
        .map(normalise_num)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Normalise a numeric literal so SSOT `0.10` and source `0.10` (or `16_384` vs
/// `16384`) compare equal: drop `_` separators and a trailing type suffix.
fn normalise_num(raw: &str) -> String {
    let raw = raw.trim();
    // Cut at the first char that cannot be part of a numeric literal token.
    let token: String = raw
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
        .collect();
    let token = token.replace('_', "");
    // Strip a trailing type suffix (e.g. `1.5f64`, `3u32`).
    for suffix in ["f64", "f32", "usize", "u32", "u16", "u8", "i32", "i64"] {
        if let Some(stripped) = token.strip_suffix(suffix) {
            return stripped.to_string();
        }
    }
    token
}

/// Parse a `const …` declaration line (type-agnostic). Returns the declared kind,
/// or `None` if the line is not a const declaration.
fn parse_const_decl(line: &str) -> Option<ConstDecl> {
    let t = line.trim_start();
    let rest = strip_const_prefix(t)?;
    let name = take_ident(rest);
    if name.is_empty() {
        return None;
    }
    let after = rest[name.len()..].trim_start();
    let after = after.strip_prefix(':')?.trim_start();
    // Declared type = identifier run after the colon (e.g. `f64`, `usize`,
    // `DjMagnitude`).
    let ty = take_ident(after);
    if ty.is_empty() {
        return None;
    }
    if ty == "DjMagnitude" {
        // `… = DjMagnitude::new(7.93, 17.67);` → capture the two args.
        let value = line
            .split_once("::new(")
            .and_then(|(_, a)| a.split_once(')').map(|(args, _)| args))
            .map(|args| {
                args.split(',')
                    .map(normalise_num)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        return Some(ConstDecl::DjAnchor { name, value });
    }
    // Plain numeric const: capture the RHS literal after `=`.
    let value = line
        .split_once('=')
        .map(|(_, rhs)| normalise_num(rhs))
        .unwrap_or_default();
    Some(ConstDecl::Numeric { name, ty, value })
}

/// True for the numeric types whose consts are perceptual-policy by default
/// (i.e. require a paper-trail unless explicitly allowlisted as structural).
fn is_policy_numeric_type(ty: &str) -> bool {
    matches!(ty, "f64" | "f32")
}

/// Detect a `field: <float-literal>` line inside a `fn default()` body. Returns
/// `(field_name, normalised_value)`. Only *floating-point* literals (containing a
/// `.`) count — perceptual magnitudes in this domain are always fractional, while
/// bare integers in a default body are sizes/indices, not policy. A field bound to
/// a named const (e.g. `p_low: DEFAULT_HARDNESS`) is already covered by that
/// const's own row and is intentionally not re-detected here.
fn parse_default_field_literal(line: &str) -> Option<(String, String)> {
    let t = line.trim_start();
    let (field, rhs) = t.split_once(':')?;
    let field = field.trim();
    if field.is_empty() || !field.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let rhs = rhs.trim().trim_end_matches(',').trim();
    // Must be a bare float literal: starts with a digit (or sign) and contains a dot.
    let first = rhs.chars().next()?;
    if !(first.is_ascii_digit() || first == '-') || !rhs.contains('.') {
        return None;
    }
    // Reject anything with non-numeric tail (e.g. a method call `0.5.max(x)`).
    let value = normalise_num(rhs);
    if value.is_empty() || rhs[value.len().min(rhs.len())..].starts_with('.') {
        return None;
    }
    Some((field.to_string(), value))
}

/// 2-line lookback: a marker on either of the two physical lines immediately
/// above the site counts. The scan STOPS at an intervening const/field site, so a
/// marker cannot "bleed" onto a second, adjacent magnitude one line below an
/// already-marked one (which would let an untracked const hide directly beneath a
/// marked sibling — the marker-bleed false-negative). Each magnitude must own a
/// marker that is not separated from it by another magnitude.
fn marker_above(lines: &[&str], site_idx: usize) -> (bool, bool, String) {
    let mut has = false;
    let mut needs = false;
    let mut marker_line = String::new();
    for back in 1..=2 {
        if site_idx < back {
            break;
        }
        let prev = lines[site_idx - back].trim_start();
        if prev.contains("// NEEDS-SCIENCE") {
            has = true;
            needs = true;
            marker_line = prev.to_string();
            break;
        } else if prev.contains("// GROUNDED") {
            has = true;
            marker_line = prev.to_string();
            break;
        }
        // An intervening magnitude line consumes its own marker — stop here so it
        // is not credited to the site below it.
        if back == 1
            && (parse_const_decl(lines[site_idx - back]).is_some()
                || parse_default_field_literal(lines[site_idx - back]).is_some())
        {
            break;
        }
    }
    (has, needs, marker_line)
}

/// Scan a single module's source text into the detected POLICY magnitudes.
/// `allowlist` is injected so the RED-proof can reuse the exact scanner.
fn scan_source(module: &str, source: &str, allowlist: &[&str]) -> Vec<DetectedConst> {
    let lines: Vec<&str> = source.lines().collect();
    let module_stem = module.trim_end_matches(".rs").to_ascii_uppercase();
    let mut out = Vec::new();
    let mut in_default_body = false;
    let mut default_brace_depth: i32 = 0;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();

        // Track `fn default()` bodies so field-literal detection is scoped to them
        // (and never fires inside test constructors or other functions).
        if !in_default_body && trimmed.contains("fn default()") {
            in_default_body = true;
            default_brace_depth = 0;
        }
        if in_default_body {
            default_brace_depth += line.matches('{').count() as i32;
            default_brace_depth -= line.matches('}').count() as i32;
        }

        // 1+2. const declarations (type-agnostic, incl. DjMagnitude).
        if let Some(decl) = parse_const_decl(line) {
            match decl {
                ConstDecl::DjAnchor { name, value } => {
                    if allowlist.contains(&name.as_str()) {
                        continue;
                    }
                    let (has_marker, needs_science, marker_line) = marker_above(&lines, idx);
                    out.push(DetectedConst {
                        module: module.to_string(),
                        line: idx + 1,
                        name,
                        value,
                        has_marker,
                        needs_science,
                        marker_line,
                    });
                }
                ConstDecl::Numeric { name, ty, value } => {
                    if allowlist.contains(&name.as_str()) {
                        continue; // STANDARD — excluded by construction (INV-3).
                    }
                    if !is_policy_numeric_type(&ty) {
                        // Non-`f64`/`f32` numeric const: policy ONLY if not declared
                        // structural. A structural const is silently excluded; any
                        // OTHER non-float const must be triaged (added to one of the
                        // two allowlists) and so it surfaces as an unmarked policy
                        // site → GATE 1 RED. Non-numeric consts never reach here.
                        if STRUCTURAL_NONPOLICY_ALLOWLIST.contains(&name.as_str()) {
                            continue;
                        }
                        if !ty.chars().next().is_some_and(|c| c.is_ascii_lowercase())
                            || !matches!(
                                ty.as_str(),
                                "usize"
                                    | "u8"
                                    | "u16"
                                    | "u32"
                                    | "u64"
                                    | "i8"
                                    | "i16"
                                    | "i32"
                                    | "i64"
                                    | "f32"
                            )
                        {
                            // Not a primitive numeric type (e.g. a `&str`, a struct):
                            // out of the numeric audit surface entirely.
                            continue;
                        }
                    }
                    let (has_marker, needs_science, marker_line) = marker_above(&lines, idx);
                    out.push(DetectedConst {
                        module: module.to_string(),
                        line: idx + 1,
                        name,
                        value,
                        has_marker,
                        needs_science,
                        marker_line,
                    });
                }
            }
            continue;
        }

        // 3. Default-field float literals (scoped to a `fn default()` body).
        if in_default_body
            && default_brace_depth > 0
            && let Some((field, value)) = parse_default_field_literal(line)
        {
            let name = format!("{module_stem}_DEFAULT_{}", field.to_ascii_uppercase());
            if !allowlist.contains(&name.as_str()) {
                let (has_marker, needs_science, marker_line) = marker_above(&lines, idx);
                out.push(DetectedConst {
                    module: module.to_string(),
                    line: idx + 1,
                    name,
                    value,
                    has_marker,
                    needs_science,
                    marker_line,
                });
            }
        }

        if in_default_body && default_brace_depth <= 0 && line.contains('}') {
            in_default_body = false;
        }
    }
    out
}

/// Scan all 6 perceptual modules off the real tree.
fn scan_tree() -> Vec<DetectedConst> {
    let mut all = Vec::new();
    for module in PERCEPTUAL_MODULES {
        let source = read_module(module);
        all.extend(scan_source(module, &source, NUMERIC_METHOD_ALLOWLIST));
    }
    all
}

// ─────────────────────────────────────────────────────────────────────────────
// ssot — the pure-std inventory parser. One row per POLICY const, join-keyed on
// `(row#, name)`.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct InventoryRow {
    /// The declared join-key row number (first markdown column).
    row_num: usize,
    name: String,
    /// The documented magnitude (third column), normalised for comparison against
    /// the source literal. GATE 2 asserts this equals the value in use.
    value: String,
    /// The documented module (fourth column). GATE 2 asserts this equals the
    /// module the const is actually detected in.
    module: String,
    /// Marker column normalised: true == provisional (`NEEDS-SCIENCE`).
    provisional: bool,
}

/// Read + parse the SSOT. A missing file is itself a gate failure (the SSOT is a
/// tracked artifact), surfaced as an explicit, named panic — never a silent
/// skip.
fn read_inventory() -> Vec<InventoryRow> {
    let path = inventory_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "SSOT inventory missing at {} ({e}). \
             The empirical-inventory gate REQUIRES docs/empirical-inventory.md \
             (one row per POLICY const, join-keyed on (row#, name)).",
            path.display()
        )
    });
    parse_inventory(&text)
}

/// Parse the markdown table. A data row is a `|`-delimited line whose first cell
/// is a bare integer (the join-key row#). Header / separator / prose lines are
/// skipped. Columns: `row# | name | value | module | marker | rationale`.
fn parse_inventory(text: &str) -> Vec<InventoryRow> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('|') {
            continue;
        }
        let cells: Vec<&str> = trimmed
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim())
            .collect();
        if cells.len() < 5 {
            continue;
        }
        let Ok(row_num) = cells[0].parse::<usize>() else {
            continue; // header / separator row.
        };
        let name = cells[1].trim_matches('`').to_string();
        if name.is_empty() {
            continue;
        }
        // `value` (cell 2) and `module` (cell 3) are now load-bearing join data,
        // not decoration — GATE 2 compares them against source.
        let value = cells[2].trim_matches('`').to_string();
        let module = cells[3].trim_matches('`').to_string();
        let marker = cells[4].to_ascii_uppercase();
        let provisional = marker.contains("NEEDS-SCIENCE");
        rows.push(InventoryRow {
            row_num,
            name,
            value,
            module,
            provisional,
        });
    }
    rows
}

/// Rewrite a single cell of the SSOT markdown table for the data row whose `name`
/// column (cell 1) equals `name`, setting column `col` (0-based) to `new_value`,
/// and return the full mutated text. Used ONLY by the RED-proof to splice a
/// value/module drift into an in-memory copy of the real SSOT; the result is
/// re-parsed by `parse_inventory`, so it must preserve the `|`-delimited layout
/// `parse_inventory` reads. A no-op (row not found) returns the text unchanged so
/// the caller can assert the splice actually landed.
fn rewrite_inventory_cell(text: &str, name: &str, col: usize, new_value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        // Only data rows: `|`-delimited, first cell a bare integer, name cell match.
        // This mirrors `parse_inventory`'s row predicate so we never touch a header,
        // separator, or prose line.
        let is_data_row = trimmed.starts_with('|') && {
            let cells: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            cells.len() >= 5
                && cells[0].parse::<usize>().is_ok()
                && cells.get(1).map(|c| c.trim_matches('`')) == Some(name)
        };
        if is_data_row {
            let cells: Vec<&str> = trimmed
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            let rebuilt: Vec<String> = cells
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    if i == col {
                        new_value.to_string()
                    } else {
                        (*c).to_string()
                    }
                })
                .collect();
            out.push(format!("| {} |", rebuilt.join(" | ")));
        } else {
            out.push(line.to_string());
        }
    }
    let mut joined = out.join("\n");
    // Preserve a trailing newline if the original had one.
    if text.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

/// GATE-2's value/module drift-detection logic, extracted into a SINGLE shared
/// entry point. A row drifts when its documented `value`/`module` column does not
/// match the source const it joins to at `name`: the SSOT promise is "the
/// documented empirical value/location is the value/location in use" — a row may
/// silently drift (`0.10 → 0.20`, or claim the wrong file) while every set-join
/// stays green, so the value+module columns are load-bearing join data, not
/// decoration. Returns one human-readable string per drifted row (empty == in
/// sync).
///
/// This is the GATE-2 analogue of `scan_source`: the live gate
/// (`gate2_inventory_and_markers_are_in_sync`) and the RED-proof
/// (`red_proof_audit_probe`) both call it, so the probe exercises the *real*
/// comparison verbatim and cannot be green-from-birth (INV-4). Were this logic
/// inlined only in the gate, a mutation to the comparison would survive the
/// RED-proof — exactly the test-theater class this extraction closes.
fn value_or_module_drift(rows: &[InventoryRow], detected: &[DetectedConst]) -> Vec<String> {
    let by_name: std::collections::BTreeMap<&str, &DetectedConst> =
        detected.iter().map(|c| (c.name.as_str(), c)).collect();
    rows.iter()
        .filter_map(|r| {
            let c = by_name.get(r.name.as_str())?;
            let value_ok = normalise_value(&r.value) == c.value;
            let module_ok = r.module == c.module;
            if value_ok && module_ok {
                None
            } else {
                Some(format!(
                    "row {} `{}`: documented (value={}, module={}) vs source (value={}, module={})",
                    r.row_num, r.name, r.value, r.module, c.value, c.module
                ))
            }
        })
        .collect()
}

// ─────────────────────────────────────────────────────────────────────────────
// GATE-4 support — provenance-CITATION parsing + verification.
//
// BUG CLASS this closes: GATE-1 only checks a `// GROUNDED` marker is *present*
// (`marker_above` reads zero characters after the token), and GATE-2 only checks
// the SSOT value/module columns — but the *citation text itself* (the cited
// standard anchor and the cited document link) is never verified. A typo'd
// standard token, a fabricated symbol, or a dead/fabricated doc-link passes every
// gate green. GATE-4 turns each of those into a RED failure by asserting the
// citation is structurally TRUE against the cited document on disk.
//
// What a `// GROUNDED` marker must structurally contain, and what we verify:
//   1. A doc-link `(… docs/<file>.md …)` whose file EXISTS on disk.
//   2. At least one backtick-quoted citation ANCHOR token (e.g. ``0.0.98G-4g``),
//      and EVERY such anchor must be attested verbatim in that cited document.
// A `// NEEDS-SCIENCE` marker is provisional-by-definition (no upstream source),
// so it is out of scope here — only `// GROUNDED` claims a citable provenance.
// ─────────────────────────────────────────────────────────────────────────────

/// A parsed `// GROUNDED` citation: the cited document (relative to the workspace
/// root) and the backtick-quoted anchor token(s) the marker claims the document
/// attests. `None` when the marker is not a GROUNDED citation.
#[derive(Debug, Clone, PartialEq, Eq)]
struct GroundedCitation {
    /// Document path as written in the marker, e.g. `docs/empirical-inventory.md`.
    doc_rel_path: String,
    /// Every backtick-quoted anchor token in the marker (e.g. `0.0.98G-4g`). A
    /// GROUNDED marker with zero anchors is itself a defect (nothing to verify).
    anchors: Vec<String>,
}

/// Extract every backtick-quoted span from a marker comment (the citation anchors).
fn backtick_spans(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('`') {
            let token = after[..end].trim();
            if !token.is_empty() {
                out.push(token.to_string());
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    out
}

/// Parse a `// GROUNDED` marker line into its citation. Returns `None` for a
/// non-GROUNDED marker (e.g. `// NEEDS-SCIENCE`). The doc-link is the first
/// `docs/…\.md` token found anywhere on the line (parenthesised by convention,
/// but we do not require the parens so a reformatting cannot bypass the check).
fn parse_grounded_citation(marker_line: &str) -> Option<GroundedCitation> {
    if !marker_line.contains("// GROUNDED") {
        return None;
    }
    // Find a `docs/…\.md` path: scan whitespace/paren-delimited tokens, stripping
    // any trailing sentence punctuation (the `.md` extension is kept because the
    // token must END with it).
    let doc_rel_path = marker_line
        .split(|c: char| c.is_whitespace() || c == '(' || c == ')')
        .find_map(|t| {
            let t = t.trim_end_matches([',', ';']);
            if t.starts_with("docs/") && t.ends_with(".md") {
                Some(t.to_string())
            } else {
                None
            }
        })?;
    let anchors = backtick_spans(marker_line);
    Some(GroundedCitation {
        doc_rel_path,
        anchors,
    })
}

/// GATE-4's verification, extracted into a SINGLE shared entry point (the GATE-4
/// analogue of `scan_source` / `value_or_module_drift`): the live gate and the
/// RED-proof both call it, so the probe exercises the real check verbatim and
/// cannot be green-from-birth (INV-4). `read_doc` is injected so the RED-proof can
/// substitute a fabricated citation without touching the real tree.
///
/// Returns one human-readable defect string per GROUNDED const whose citation is
/// not structurally true: a malformed marker (no doc-link or no anchor), a cited
/// document that does not exist, or a cited anchor not attested in that document.
fn grounded_citation_defects(
    detected: &[DetectedConst],
    read_doc: &dyn Fn(&str) -> Option<String>,
) -> Vec<String> {
    let mut defects = Vec::new();
    for c in detected {
        if !c.marker_line.contains("// GROUNDED") {
            continue; // NEEDS-SCIENCE / unmarked: out of scope for provenance-truth.
        }
        let Some(cite) = parse_grounded_citation(&c.marker_line) else {
            defects.push(format!(
                "{} `{}`: GROUNDED marker has no `docs/…\\.md` citation link — provenance is unverifiable",
                c.module, c.name
            ));
            continue;
        };
        if cite.anchors.is_empty() {
            defects.push(format!(
                "{} `{}`: GROUNDED marker cites no backtick anchor token — nothing to verify against {}",
                c.module, c.name, cite.doc_rel_path
            ));
            continue;
        }
        let Some(doc_text) = read_doc(&cite.doc_rel_path) else {
            defects.push(format!(
                "{} `{}`: GROUNDED citation references `{}` which does not exist on disk (dead/fabricated link)",
                c.module, c.name, cite.doc_rel_path
            ));
            continue;
        };
        for anchor in &cite.anchors {
            // The doc-path itself appears as an anchor only if back-ticked; it is
            // verified by existence above, so skip it as a content anchor.
            if anchor == &cite.doc_rel_path {
                continue;
            }
            if !doc_text.contains(anchor.as_str()) {
                defects.push(format!(
                    "{} `{}`: GROUNDED cites anchor `{}` but it is NOT attested in {} (typo'd/fabricated provenance)",
                    c.module, c.name, anchor, cite.doc_rel_path
                ));
            }
        }
    }
    defects
}

/// Resolve a workspace-relative `docs/…` path against the real tree and read it.
/// `None` when the file is absent — exactly the dead-link signal GATE-4 reports.
fn read_workspace_doc(rel: &str) -> Option<String> {
    let path = crate_root().join("..").join("..").join(rel);
    std::fs::read_to_string(path).ok()
}

// ─────────────────────────────────────────────────────────────────────────────
// GATE 1 — untracked-const (differential). Every detected POLICY const must have
// a marker within a 2-line lookback. An unmarked const → RED naming the const.
// Closes the CLASS "magic number without a paper-trail".
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn gate1_every_policy_const_is_marked() {
    let detected = scan_tree();
    assert!(
        !detected.is_empty(),
        "GATE 1 scanned zero POLICY consts across {:?} — the detector is mis-scoped \
         (a green-from-birth scan proves nothing).",
        PERCEPTUAL_MODULES
    );

    let unmarked: Vec<String> = detected
        .iter()
        .filter(|c| !c.has_marker)
        .map(|c| format!("{}:{} const {}", c.module, c.line, c.name))
        .collect();

    assert!(
        unmarked.is_empty(),
        "GATE 1 FAILED — {} POLICY const(s) have no `// NEEDS-SCIENCE` / `// GROUNDED` \
         marker within a 2-line lookback (magic number without a paper-trail):\n  {}",
        unmarked.len(),
        unmarked.join("\n  ")
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// GATE 2 — allowlist-subset / stale-row (property). Every inventory row resolves
// at its (row#, name) to a real markered const, and every markered const has a
// row. Catches STALE rows after line-drift; survives reformatting because it
// keys on (row#, name), not byte offset (INV-6).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn gate2_inventory_and_markers_are_in_sync() {
    let detected = scan_tree();
    let rows = read_inventory();

    let markered: BTreeSet<&str> = detected
        .iter()
        .filter(|c| c.has_marker)
        .map(|c| c.name.as_str())
        .collect();
    let row_names: BTreeSet<&str> = rows.iter().map(|r| r.name.as_str()).collect();

    // Every markered const must have a row (no untracked marker).
    let markers_without_row: Vec<&str> = markered.difference(&row_names).copied().collect();
    // Every row must resolve to a markered const at its declared (row#, name).
    let rows_without_marker: Vec<&str> = row_names.difference(&markered).copied().collect();

    assert!(
        markers_without_row.is_empty() && rows_without_marker.is_empty(),
        "GATE 2 FAILED — marker↔inventory drift (INV-6, keyed on (row#, name)).\n  \
         markered consts with NO inventory row: {:?}\n  \
         inventory rows that resolve to NO markered const (STALE rows): {:?}",
        markers_without_row,
        rows_without_marker
    );

    // Each declared join-key (row#, name) must resolve to a real, currently
    // detected const name — the row# is a live join-key, not decoration.
    let detected_names: BTreeSet<&str> = detected.iter().map(|c| c.name.as_str()).collect();
    let unresolved: Vec<String> = rows
        .iter()
        .filter(|r| !detected_names.contains(r.name.as_str()))
        .map(|r| format!("row {} -> `{}`", r.row_num, r.name))
        .collect();
    assert!(
        unresolved.is_empty(),
        "GATE 2 FAILED — inventory row(s) do not resolve to any detected POLICY const \
         (stale after line-drift): {:?}",
        unresolved
    );

    // The documented `value` and `module` columns must MATCH the source: the SSOT
    // promise is "the documented empirical value/location is the value/location in
    // use". Without this, a row can drift (0.10 → 0.20, or claim the wrong file)
    // while all set-joins stay green. The comparison is the shared
    // `value_or_module_drift` (mirrored verbatim by the RED-proof, INV-4).
    let drift = value_or_module_drift(&rows, &detected);
    assert!(
        drift.is_empty(),
        "GATE 2 FAILED — SSOT value/module column does not match the source (documented \
         empirical value/location is NOT the value/location in use):\n  {}",
        drift.join("\n  ")
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// GATE 3 — unmarked-provisional contract (contract). The marker↔inventory
// contract holds *both ways*: every `// NEEDS-SCIENCE` const has a provisional
// inventory row, and every provisional row has a `// NEEDS-SCIENCE` const
// (INV-2).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn gate3_needs_science_contract_holds_both_ways() {
    let detected = scan_tree();
    let rows = read_inventory();

    let provisional_consts: BTreeSet<&str> = detected
        .iter()
        .filter(|c| c.needs_science)
        .map(|c| c.name.as_str())
        .collect();
    let provisional_rows: BTreeSet<&str> = rows
        .iter()
        .filter(|r| r.provisional)
        .map(|r| r.name.as_str())
        .collect();

    let const_without_provisional_row: Vec<&str> = provisional_consts
        .difference(&provisional_rows)
        .copied()
        .collect();
    let provisional_row_without_const: Vec<&str> = provisional_rows
        .difference(&provisional_consts)
        .copied()
        .collect();

    assert!(
        const_without_provisional_row.is_empty() && provisional_row_without_const.is_empty(),
        "GATE 3 FAILED — NEEDS-SCIENCE marker↔provisional-row contract broken (INV-2).\n  \
         `// NEEDS-SCIENCE` consts with NO provisional row: {:?}\n  \
         provisional rows with NO `// NEEDS-SCIENCE` const: {:?}",
        const_without_provisional_row,
        provisional_row_without_const
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// join-key sanity (unit). (row#, name) pairs in the SSOT are unique and every
// name resolves to a real detected const; no duplicate keys (INV-6).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn join_key_pairs_are_unique_and_resolve() {
    let rows = read_inventory();
    assert!(
        !rows.is_empty(),
        "join-key sanity FAILED — SSOT inventory parsed to zero rows."
    );

    let mut seen: BTreeSet<(usize, &str)> = BTreeSet::new();
    let mut duplicates: Vec<(usize, String)> = Vec::new();
    for r in &rows {
        if !seen.insert((r.row_num, r.name.as_str())) {
            duplicates.push((r.row_num, r.name.clone()));
        }
    }
    assert!(
        duplicates.is_empty(),
        "join-key sanity FAILED — duplicate (row#, name) keys in SSOT: {:?}",
        duplicates
    );

    // Row numbers themselves must be unique (a join-key is a primary key).
    let mut seen_nums: BTreeSet<usize> = BTreeSet::new();
    let dup_nums: Vec<usize> = rows
        .iter()
        .filter(|r| !seen_nums.insert(r.row_num))
        .map(|r| r.row_num)
        .collect();
    assert!(
        dup_nums.is_empty(),
        "join-key sanity FAILED — duplicate row# in SSOT: {:?}",
        dup_nums
    );

    // Every key must resolve to a currently-detected const name.
    let detected = scan_tree();
    let detected_names: BTreeSet<&str> = detected.iter().map(|c| c.name.as_str()).collect();
    let unresolved: Vec<String> = rows
        .iter()
        .filter(|r| !detected_names.contains(r.name.as_str()))
        .map(|r| format!("({}, {})", r.row_num, r.name))
        .collect();
    assert!(
        unresolved.is_empty(),
        "join-key sanity FAILED — (row#, name) keys that resolve to NO current source line: {:?}",
        unresolved
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// standards-exclusion assertion (contract, INV-3). Explicit, observable proof
// that no known standard name (Hellwig/APCA/UCS/L_A=64/Yb=20/D65/…) appears as
// an inventory row — exclusion is enforced, not merely implied by scoping.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn standards_are_excluded_from_inventory() {
    let rows = read_inventory();
    let row_names: BTreeSet<&str> = rows.iter().map(|r| r.name.as_str()).collect();

    let leaked: Vec<&str> = FORBIDDEN_STANDARD_ROW_NAMES
        .iter()
        .copied()
        .filter(|name| row_names.contains(name))
        .collect();

    assert!(
        leaked.is_empty(),
        "standards-exclusion FAILED (INV-3) — standard name(s) leaked into the POLICY \
         inventory as a row: {:?}. Standards are excluded by construction and must never \
         be marked NEEDS-SCIENCE or inventoried.",
        leaked
    );

    // And the allowlist itself must keep every standard OUT of the detected
    // POLICY set (the by-construction half of INV-3).
    let detected = scan_tree();
    let detected_names: BTreeSet<&str> = detected.iter().map(|c| c.name.as_str()).collect();
    let standard_in_policy: Vec<&str> = FORBIDDEN_STANDARD_ROW_NAMES
        .iter()
        .copied()
        .filter(|name| detected_names.contains(name))
        .collect();
    assert!(
        standard_in_policy.is_empty(),
        "standards-exclusion FAILED (INV-3) — standard const(s) reached the detected POLICY \
         set instead of the allowlist: {:?}",
        standard_in_policy
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// GATE 4 — grounded-provenance truth (contract). Every `// GROUNDED` const's
// CITATION must be structurally true against the cited document on disk: the
// cited `docs/…\.md` exists, and every backtick anchor it cites is attested
// verbatim in that document. Closes the CLASS "a GROUNDED marker asserts a
// fabricated/typo'd/dead-link provenance and still passes" — the blind spot
// GATE-1 (presence-only) and GATE-2 (SSOT value/module) leave open.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn gate4_grounded_citations_are_truthful() {
    let detected = scan_tree();

    // There must be at least one GROUNDED const, or this gate proves nothing
    // (green-from-birth): a scan with zero GROUNDED sites would pass vacuously.
    let grounded_count = detected
        .iter()
        .filter(|c| c.marker_line.contains("// GROUNDED"))
        .count();
    assert!(
        grounded_count > 0,
        "GATE 4 scanned zero `// GROUNDED` consts — the provenance-truth gate is \
         vacuous (mis-scoped scanner or no grounded constants to verify)."
    );

    let defects = grounded_citation_defects(&detected, &read_workspace_doc);
    assert!(
        defects.is_empty(),
        "GATE 4 FAILED — {} GROUNDED citation(s) are not structurally true (the cited \
         standard/symbol is fabricated, typo'd, or the cited document is missing):\n  {}",
        defects.len(),
        defects.join("\n  ")
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// red_proof_audit_probe (regression, INV-4). Splice an `_AUDIT_PROBE` literal
// into an IN-MEMORY copy of a module's source, run the *same* scanner, and
// assert GATE-1 deterministically goes RED naming the spliced literal. Then
// re-run the gate on the real tree and assert it is GREEN (tree/values
// untouched). Proves the guard bites — kills test-theater.
//
// Runs via verification-runner SOLO/worktree only if it ever mutates the tree;
// this implementation mutates ONLY an in-memory String, so it is hermetic.
// ─────────────────────────────────────────────────────────────────────────────

/// Splice `snippet` at the top of an in-memory copy of `semantic.rs`, run the
/// real scanner, and return the names of all UNMARKED detected sites. The helper
/// is the single mirror of GATE-1's detection path, so every sub-probe exercises
/// production behaviour verbatim (INV-4).
fn unmarked_after_splice(snippet: &str) -> Vec<String> {
    let original = read_module("semantic.rs");
    let mut spliced = String::with_capacity(original.len() + snippet.len() + 2);
    spliced.push_str(snippet);
    spliced.push('\n');
    spliced.push_str(&original);
    scan_source("semantic.rs", &spliced, NUMERIC_METHOD_ALLOWLIST)
        .into_iter()
        .filter(|c| !c.has_marker)
        .map(|c| c.name)
        .collect()
}

#[test]
fn red_proof_audit_probe() {
    // Each sub-probe is a DISTINCT detection path. A green-from-birth scanner that
    // saw only `const … : f64` would let every path except (1) through — so every
    // path must be proven to flip GREEN→RED, killing the Goodhart/mutation-survivor
    // weakness where the probe exercised only the f64 path.

    // 1. f64 const — the classic path.
    let v = unmarked_after_splice("const _AUDIT_PROBE: f64 = 42.0;");
    assert!(
        v.contains(&"_AUDIT_PROBE".to_string()),
        "RED-proof FAILED (f64 const path) — GATE-1 did not flag the unmarked f64 const. Saw: {v:?}"
    );

    // 2. DjMagnitude anchor const — invisible to a type==f64 gate.
    let v =
        unmarked_after_splice("const _AUDIT_PROBE_DJ: DjMagnitude = DjMagnitude::new(9.9, 8.8);");
    assert!(
        v.contains(&"_AUDIT_PROBE_DJ".to_string()),
        "RED-proof FAILED (DjMagnitude path) — GATE-1 did not flag an unmarked DjMagnitude anchor \
         (the type-gate bypass class is still open). Saw: {v:?}"
    );

    // 3. Non-`f64` numeric policy const NOT on either allowlist — must be RED, so a
    //    future `u32`/`f32` perceptual threshold cannot hide behind its type.
    let v = unmarked_after_splice("const _AUDIT_PROBE_U32: u32 = 7;");
    assert!(
        v.contains(&"_AUDIT_PROBE_U32".to_string()),
        "RED-proof FAILED (non-f64 path) — GATE-1 did not flag an un-allowlisted non-f64 numeric \
         const; the type-gate bypass for integer/f32 policy thresholds is still open. Saw: {v:?}"
    );

    // 4. Default-field float literal — invisible to any const-only gate.
    let snippet = "struct _ProbeP { x: f64 }\n\
                   impl Default for _ProbeP {\n    fn default() -> Self {\n        Self {\n            x: 4.2,\n        }\n    }\n}";
    let v = unmarked_after_splice(snippet);
    assert!(
        v.iter().any(|n| n.ends_with("_DEFAULT_X")),
        "RED-proof FAILED (Default-field path) — GATE-1 did not flag an unmarked `field: <float>` \
         literal in a `fn default()` body (the Default-field-literal class is still open). Saw: {v:?}"
    );

    // 5. The real tree must remain GREEN under GATE-1 — every splice was in-memory.
    let real_unmarked: Vec<String> = scan_tree()
        .iter()
        .filter(|c| !c.has_marker)
        .map(|c| format!("{}:{} {}", c.module, c.line, c.name))
        .collect();
    assert!(
        real_unmarked.is_empty(),
        "RED-proof FAILED — real tree is NOT GREEN under GATE-1 ({} unmarked POLICY site(s)); \
         the probe cannot prove 'splice flips green→red' until the real tree is green:\n  {}",
        real_unmarked.len(),
        real_unmarked.join("\n  ")
    );

    // 6. value-drift and module-drift must flip GATE-2 RED. Mutate ONE row in an
    //    IN-MEMORY copy of the real SSOT, RE-PARSE it via `parse_inventory`, and run
    //    the *same* `value_or_module_drift` the live gate runs — asserting it goes RED
    //    naming the mutated row. This mirrors GATE-1's `unmarked_after_splice`: the
    //    probe exercises production behaviour verbatim, so a mutation to GATE-2's
    //    comparison (e.g. inverting `value_ok && module_ok`) is caught here, not
    //    green-from-birth. The probe const must be a real, currently-detected,
    //    inventoried row so a genuine drift is observable.
    let detected = scan_tree();
    let probe_name = "DECORATIVE_FLOOR_MIN";
    let probe_const = detected
        .iter()
        .find(|c| c.name == probe_name)
        .unwrap_or_else(|| panic!("RED-proof needs `{probe_name}` to be detected"));

    let real_ssot = std::fs::read_to_string(inventory_path()).unwrap_or_else(|e| {
        panic!(
            "RED-proof cannot read the SSOT at {} ({e}).",
            inventory_path().display()
        )
    });

    // Floor: the UNMUTATED real SSOT must be GREEN under the shared comparison, so a
    // drift seen below is provably caused by the splice — not pre-existing noise.
    let baseline = value_or_module_drift(&parse_inventory(&real_ssot), &detected);
    assert!(
        baseline.is_empty(),
        "RED-proof FAILED — real SSOT is NOT GREEN under GATE-2 ({} drifted row(s)); the probe \
         cannot prove 'mutation flips green→red' until the real SSOT is in sync:\n  {}",
        baseline.len(),
        baseline.join("\n  ")
    );

    // (a) VALUE-drift: rewrite the probe row's `value` cell to a value that cannot
    // normalise back to the source magnitude. The shared comparison MUST flag it.
    let drifted_value_num = format!("{}99", probe_const.value); // e.g. "7.6" -> "7.699"
    assert!(
        normalise_value(&drifted_value_num) != probe_const.value,
        "RED-proof setup wrong — drifted value must not normalise back to the source value."
    );
    let value_drifted_ssot = rewrite_inventory_cell(&real_ssot, probe_name, 2, &drifted_value_num);
    assert_ne!(
        value_drifted_ssot, real_ssot,
        "RED-proof setup wrong — value-drift splice did not change the SSOT text (row `{probe_name}` not found)."
    );
    let value_drift = value_or_module_drift(&parse_inventory(&value_drifted_ssot), &detected);
    assert!(
        value_drift.iter().any(|d| d.contains(probe_name)),
        "RED-proof FAILED (GATE-2 value-drift path) — drifting row `{probe_name}`'s documented \
         value to `{drifted_value_num}` did NOT flip the GATE-2 comparison RED; the value-drift \
         detection class is green-from-birth. Saw: {value_drift:?}"
    );

    // (b) MODULE-drift: rewrite the probe row's `module` cell to a different (but
    // still real) perceptual module. The shared comparison MUST flag it.
    let other_module = PERCEPTUAL_MODULES
        .iter()
        .find(|m| **m != probe_const.module)
        .expect("RED-proof needs a second perceptual module to drift to");
    let module_drifted_ssot = rewrite_inventory_cell(&real_ssot, probe_name, 3, other_module);
    assert_ne!(
        module_drifted_ssot, real_ssot,
        "RED-proof setup wrong — module-drift splice did not change the SSOT text (row `{probe_name}` not found)."
    );
    let module_drift = value_or_module_drift(&parse_inventory(&module_drifted_ssot), &detected);
    assert!(
        module_drift.iter().any(|d| d.contains(probe_name)),
        "RED-proof FAILED (GATE-2 module-drift path) — drifting row `{probe_name}`'s documented \
         module to `{other_module}` did NOT flip the GATE-2 comparison RED; the module-drift \
         detection class is green-from-birth. Saw: {module_drift:?}"
    );

    // 7. GATE-4 provenance-truth must flip RED on a fabricated citation. Build a
    //    synthetic GROUNDED const carrying a citation and run the *same*
    //    `grounded_citation_defects` the live gate runs, so a mutation to GATE-4's
    //    comparison is caught here (INV-4), not green-from-birth.
    let probe_grounded = |marker: &str| DetectedConst {
        module: "lpc.rs".to_string(),
        line: 1,
        name: "_AUDIT_PROBE_GROUNDED".to_string(),
        value: "0.022".to_string(),
        has_marker: true,
        needs_science: false,
        marker_line: marker.to_string(),
    };

    // (a) TYPO'D ANCHOR: a real doc but a citation anchor the doc does not attest.
    //     `read_workspace_doc` is the real reader — the fabrication is the anchor.
    let typo = vec![probe_grounded(
        "// GROUNDED — APCA SAPC-8 `0.0.NONEXISTENT-Xg` published set (docs/empirical-inventory.md).",
    )];
    let d = grounded_citation_defects(&typo, &read_workspace_doc);
    assert!(
        d.iter()
            .any(|s| s.contains("_AUDIT_PROBE_GROUNDED") && s.contains("NOT attested")),
        "RED-proof FAILED (GATE-4 typo'd-anchor path) — a GROUNDED citation whose anchor is \
         absent from the cited document did NOT flip GATE-4 RED; fabricated provenance is \
         green-from-birth. Saw: {d:?}"
    );

    // (b) DEAD DOC-LINK: a citation referencing a document that does not exist.
    let dead = vec![probe_grounded(
        "// GROUNDED — APCA SAPC-8 `0.0.98G-4g` published set (docs/decisions/does-not-exist.md).",
    )];
    let d = grounded_citation_defects(&dead, &read_workspace_doc);
    assert!(
        d.iter()
            .any(|s| s.contains("_AUDIT_PROBE_GROUNDED") && s.contains("does not exist")),
        "RED-proof FAILED (GATE-4 dead-link path) — a GROUNDED citation referencing a missing \
         document did NOT flip GATE-4 RED; dead/fabricated links are green-from-birth. Saw: {d:?}"
    );

    // (c) MALFORMED MARKER: GROUNDED with no `docs/…\.md` link at all.
    let malformed = vec![probe_grounded(
        "// GROUNDED — some prose with no citation link.",
    )];
    let d = grounded_citation_defects(&malformed, &read_workspace_doc);
    assert!(
        d.iter()
            .any(|s| s.contains("_AUDIT_PROBE_GROUNDED") && s.contains("no `docs")),
        "RED-proof FAILED (GATE-4 malformed-marker path) — a GROUNDED marker with no citation \
         link did NOT flip GATE-4 RED; unverifiable provenance is green-from-birth. Saw: {d:?}"
    );

    // (d) The real tree must remain GREEN under GATE-4 — every probe above was
    //     synthetic; no real GROUNDED citation may be fabricated/typo'd/dead.
    let real_citation_defects = grounded_citation_defects(&detected, &read_workspace_doc);
    assert!(
        real_citation_defects.is_empty(),
        "RED-proof FAILED — real tree is NOT GREEN under GATE-4 ({} citation defect(s)); the \
         probe cannot prove 'fabrication flips green→red' until the real tree is green:\n  {}",
        real_citation_defects.len(),
        real_citation_defects.join("\n  ")
    );
}
