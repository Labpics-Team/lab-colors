//! Гейт агностичной production-поверхности (ADR-0001): поставляемый `src` не
//! содержит клиентских брендовых значений и showcase-типов. Встроенные якоря
//! живут только в `#[cfg(test)]`-оракулах.
//!
//! Защищаемый класс регрессии: встроенное бренд-значение, showcase-тип или
//! удалённая sentiment-физика тихо возвращаются в production-поверхность Core.
//! HIG/Figma-якоря (`#007AFF`, `#FF3B30`, …), `Role` и `RoleTable` допустимы
//! только в `#[cfg(test)]`-оракулах; `Accent` и sentiment-схема удалены.
//! Поведенческие тесты не замечают такую утечку, потому что значения продолжают
//! вычисляться. Этот гейт сканирует production-код без `cfg(test)` и комментариев
//! и превращает потерю агностичности в RED.
//!
//! Не считаются нарушением:
//! * ссылки на удалённый якорь в строковых и блок-комментариях: общий lexer
//!   маскирует комментарии, не принимая `//` внутри строки за их начало;
//! * блоки и файлы `#[cfg(test)] mod X;` с byte-identity-оракулами и фикстурами:
//!   они исключаются до сканирования.
//!
//! INV-4: GATE-тесты и `red_proof_*` вызывают одни scanner-функции поверх
//! `production_lines`. Мутацию детектора или удаления `cfg(test)` ловит
//! RED-proof, а не отдельная встроенная проверка.
//!
//! Реализация использует только `std`: `labcolors-core` остаётся без зависимостей.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

mod common;
use common::source::{production_code_lines, production_lines};
use common::src_dir;

// ─────────────────────────────────────────────────────────────────────────────
// Запрещённое содержимое production-кода.
// ─────────────────────────────────────────────────────────────────────────────

/// Клиентские литералы якорей, запрещённые в production-коде.
///
/// Сюда входят прежние акцентные seeds и референсная нейтральная шкала Lab UI.
/// Они допустимы в characterization под `#[cfg(test)]`, но поставляемый helper
/// Core не хранит клиентскую фикстуру ради воспроизведения калибровки.
const CLIENT_ANCHOR_HEXES: &[&str] = &[
    "#007AFF", // Apple HIG systemBlue — legacy Info anchor (now cited only in docs).
    "#FF9500", // Apple HIG systemOrange — legacy Warning anchor.
    "#FF3B30", // Figma Accent/Red — Danger.
    "#FFA100", // Figma Accent/Orange — Warning.
    "#FFD000", // Figma Accent/Yellow.
    "#34C759", // Figma Accent/Green — Success.
    "#5AC8FA", // Figma Accent/Teal.
    "#00C7BE", // Figma Accent/Mint.
    "#3E87FF", // Figma Accent/Blue — Info.
    "#5856D6", // Figma Accent/Indigo.
    "#AF52DE", // Figma Accent/Purple.
    "#FF2D55", // Figma Accent/Pink.
    "#101012", "#151518", "#212125", "#303136", "#44444B", "#5B5C64", "#787881", "#9698A2",
    "#B3B5BF", "#CDD0D9", "#E4E7ED", "#F6F8FA",
];

/// Швы клиентской калибровки, допустимые только в characterization-тестах.
const CLIENT_CALIBRATION_IDENTIFIERS: &[&str] = &[
    "tint_target_sweep_repro",
    "NEUTRAL_HUE_DEG",
    "TINT_TARGET_MP",
    "TINT_HUE_STIFFNESS",
];

/// Определения встроенных showcase-типов, допустимые только под `#[cfg(test)]`.
/// Совпадение в production-коде означает возврат showcase в API. Границы
/// идентификаторов исключают ложные срабатывания на `RoleChroma`, `RoleSpec` и
/// `NamedRoleTable`.
const SHOWCASE_DEFINITIONS: &[&str] = &[
    "enum Accent",
    "enum Sentiment",
    "enum Role",
    "struct RoleTable",
];

/// Точные идентификаторы удалённой встроенной sentiment-модели.
///
/// Общая лексика evidence и validators не запрещена. Список охраняет только
/// прежние физическую схему и resolver-поверхность, возврат которых заставил бы
/// Core снова интерпретировать клиентский смысл.
const RETIRED_SENTIMENT_IDENTIFIERS: &[&str] = &[
    "SentimentCategory",
    "SentimentsConfig",
    "SentimentCurve",
    "SentimentResolution",
    "UnknownSentiment",
    "resolve_config_sentiment_solid",
    "sentiment_solid_for_mode",
    "WARNING_HUE_FLOOR_DEG",
    "S_PERC_MIN",
    "NeighborZone",
    "hue_floor_deg",
    "preferred_side",
    "chroma_fraction",
    "CHROMA_FRACTION",
];

/// Фрагменты синтаксиса, где пунктуация входит в удалённый контракт.
const RETIRED_SENTIMENT_FRAGMENTS: &[&str] = &[
    "LadderSource::Sentiment",
    "pub mod sentiment",
    "mod sentiment;",
];

// ─────────────────────────────────────────────────────────────────────────────
// production_lines удаляет доказанно полные элементы под `#[cfg(test)]`, сохраняя
// нумерацию строк с единицы. Неоднозначный синтаксис остаётся видимым: гейт
// предпочитает ложный RED скрытому production-нарушению.
// ─────────────────────────────────────────────────────────────────────────────

/// True when `decl` (e.g. `enum Role`) appears in `code` bounded by non-identifier
/// characters on both sides — so `enum Role` does NOT match `enum RoleChroma`, and
/// `struct RoleTable` does NOT match `struct NamedRoleTable`.
fn contains_bounded(code: &str, needle: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = code[start..].find(needle) {
        let at = start + pos;
        let before_ok = code[..at]
            .chars()
            .last()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
        let after_ok = code[at + needle.len()..]
            .chars()
            .next()
            .is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
        if before_ok && after_ok {
            return true;
        }
        start = at + needle.len();
    }
    false
}

fn defines(code: &str, decl: &str) -> bool {
    contains_bounded(code, decl)
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared scanners — called verbatim by the GATE tests and the RED-proofs (INV-4).
// ─────────────────────────────────────────────────────────────────────────────

/// One production-code occurrence of a forbidden brand hex or showcase definition.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Site {
    module: String,
    line: usize,
    found: String,
}

/// Every brand-anchor hex literal in the PRODUCTION code of `source`
/// (`#[cfg(test)]` удалён, комментарии лексически замаскированы). Регистр не
/// различается: `#3e87ff` и `#3E87FF` — один якорь.
fn forbidden_hex_sites(module: &str, source: &str) -> Vec<Site> {
    let mut out = Vec::new();
    for (line, text) in production_code_lines(source) {
        let code = text.to_ascii_uppercase();
        for hex in CLIENT_ANCHOR_HEXES {
            if code.contains(&hex.to_ascii_uppercase()) {
                out.push(Site {
                    module: module.to_string(),
                    line,
                    found: (*hex).to_string(),
                });
            }
        }
    }
    out
}

/// Все швы клиентской калибровки в production-коде.
fn client_calibration_sites(module: &str, source: &str) -> Vec<Site> {
    let mut out = Vec::new();
    for (line, code) in production_code_lines(source) {
        for identifier in CLIENT_CALIBRATION_IDENTIFIERS {
            if contains_bounded(&code, identifier) {
                out.push(Site {
                    module: module.to_string(),
                    line,
                    found: (*identifier).to_string(),
                });
            }
        }
    }
    out
}

/// Все определения showcase-типов в production-коде `source`.
fn forbidden_definition_sites(module: &str, source: &str) -> Vec<Site> {
    let mut out = Vec::new();
    for (line, code) in production_code_lines(source) {
        for decl in SHOWCASE_DEFINITIONS {
            if defines(&code, decl) {
                out.push(Site {
                    module: module.to_string(),
                    line,
                    found: (*decl).to_string(),
                });
            }
        }
    }
    out
}

/// Все точные остатки удалённых sentiment-физики и схемы.
fn retired_sentiment_sites(module: &str, source: &str) -> Vec<Site> {
    let mut out = Vec::new();
    for (line, code) in production_code_lines(source) {
        for identifier in RETIRED_SENTIMENT_IDENTIFIERS {
            if contains_bounded(&code, identifier) {
                out.push(Site {
                    module: module.to_string(),
                    line,
                    found: (*identifier).to_string(),
                });
            }
        }
        for fragment in RETIRED_SENTIMENT_FRAGMENTS {
            if code.contains(fragment) {
                out.push(Site {
                    module: module.to_string(),
                    line,
                    found: (*fragment).to_string(),
                });
            }
        }
    }
    out
}

// ─────────────────────────────────────────────────────────────────────────────
// Production-file enumeration — every `src/**/*.rs` EXCEPT files that are declared
// as `#[cfg(test)] mod X;` (whole-file test modules: the relocated byte-identity
// oracles, the labui fixture, the config tests). The exclusion set is DERIVED from
// the tree, not hardcoded, so a future `#[cfg(test)] mod` is excluded automatically
// (self-maintaining — no gate edit needed when a test module is added).
// ─────────────────────────────────────────────────────────────────────────────

fn all_rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let entries = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read src dir {}: {e}", dir.display()));
    for entry in entries {
        let path = entry.expect("readable dir entry").path();
        if path.is_dir() {
            out.extend(all_rs_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// Parse a `mod NAME;` DECLARATION (not a `mod NAME { … }` definition). Returns the
/// module name, stripping a leading `pub`/`pub(crate)` visibility.
fn parse_mod_decl(trimmed: &str) -> Option<String> {
    let t = trimmed
        .strip_prefix("pub(crate) ")
        .or_else(|| trimmed.strip_prefix("pub "))
        .unwrap_or(trimmed);
    let rest = t.strip_prefix("mod ")?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() {
        return None;
    }
    // A declaration is terminated by `;`; a `{` would be an inline definition.
    if rest[name.len()..].trim_start().starts_with(';') {
        Some(name)
    } else {
        None
    }
}

/// The directory a module's children resolve into: the crate root (`src/`) for
/// `lib.rs`/`mod.rs`, else the sibling directory named after the module file's stem
/// (`config.rs` → `config/`).
fn children_dir(file: &Path) -> PathBuf {
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let dir = file.parent().expect("src file has a parent");
    if stem == "lib" || stem == "mod" {
        dir.to_path_buf()
    } else {
        dir.join(stem)
    }
}

/// Files that are whole-file `#[cfg(test)]` modules — excluded from the production
/// scan. Derived by finding every `#[cfg(test)]` immediately above a `mod NAME;`
/// declaration and resolving `NAME` to its file (`NAME.rs` or `NAME/mod.rs`) in the
/// declaring module's children directory.
fn cfg_test_module_files() -> BTreeSet<PathBuf> {
    let mut excluded = BTreeSet::new();
    for file in all_rs_files(&src_dir()) {
        let text = std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        let lines: Vec<&str> = text.lines().collect();
        for (i, l) in lines.iter().enumerate() {
            if !l.trim_start().starts_with("#[cfg(test)]") {
                continue;
            }
            let mut j = i + 1;
            while j < lines.len() {
                let tj = lines[j].trim_start();
                if tj.starts_with('#') || tj.is_empty() {
                    j += 1;
                } else {
                    break;
                }
            }
            if let Some(name) = lines.get(j).and_then(|l| parse_mod_decl(l.trim_start())) {
                let dir = children_dir(&file);
                excluded.insert(dir.join(format!("{name}.rs")));
                excluded.insert(dir.join(&name).join("mod.rs"));
            }
        }
    }
    excluded
}

/// Every production source file: all `src/**/*.rs` minus the cfg(test) module files.
fn production_src_files() -> Vec<PathBuf> {
    let excluded = cfg_test_module_files();
    all_rs_files(&src_dir())
        .into_iter()
        .filter(|f| !excluded.contains(f))
        .collect()
}

/// Module label for diagnostics — path relative to `src/` (e.g. `spaces/srgb.rs`).
fn module_label(file: &Path) -> String {
    file.strip_prefix(src_dir())
        .unwrap_or(file)
        .to_string_lossy()
        .replace('\\', "/")
}

// ─────────────────────────────────────────────────────────────────────────────
// GATE 1/2 — the live gates over the real tree.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn production_src_carries_no_client_anchor_hex() {
    let mut leaks = Vec::new();
    for file in production_src_files() {
        let source = std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        leaks.extend(forbidden_hex_sites(&module_label(&file), &source));
    }
    assert!(
        leaks.is_empty(),
        "AGNOSTIC-CLEANLINESS: client anchor hex leaked into PRODUCTION code (must be \
         `#[cfg(test)]`-only or supplied via ThemeConfig). Offending sites: {leaks:#?}"
    );
}

#[test]
fn production_src_carries_no_client_calibration_seam() {
    let mut leaks = Vec::new();
    for file in production_src_files() {
        let source = std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        leaks.extend(client_calibration_sites(&module_label(&file), &source));
    }
    assert!(
        leaks.is_empty(),
        "AGNOSTIC-CLEANLINESS: a client-specific calibration seam leaked into \
         PRODUCTION Core. Keep reproducibility fixtures under `#[cfg(test)]`. \
         Offending sites: {leaks:#?}"
    );
}

#[test]
fn production_src_defines_no_builtin_showcase_type() {
    let mut leaks = Vec::new();
    for file in production_src_files() {
        let source = std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        leaks.extend(forbidden_definition_sites(&module_label(&file), &source));
    }
    assert!(
        leaks.is_empty(),
        "AGNOSTIC-CLEANLINESS: a built-in showcase type DEFINITION re-entered PRODUCTION \
         code (a `#[cfg(test)]` was dropped). Offending sites: {leaks:#?}"
    );
}

#[test]
fn production_src_carries_no_retired_sentiment_physics_or_schema() {
    let mut leaks = Vec::new();
    for file in production_src_files() {
        let source = std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        leaks.extend(retired_sentiment_sites(&module_label(&file), &source));
    }
    let retired_module = src_dir().join("sentiment.rs");
    assert!(
        !retired_module.exists(),
        "AGNOSTIC-CLEANLINESS: deleted sentiment module returned at {}",
        retired_module.display()
    );
    assert!(
        leaks.is_empty(),
        "AGNOSTIC-CLEANLINESS: retired built-in sentiment physics/schema re-entered \
         PRODUCTION Core. Offending sites: {leaks:#?}"
    );
}

#[test]
fn red_proof_retired_sentiment_scanner_bites_on_every_retired_contract_token() {
    for identifier in RETIRED_SENTIMENT_IDENTIFIERS {
        let dirty = format!("fn probe(value: {identifier}) {{ let _ = value; }}\n");
        let hits = retired_sentiment_sites("probe.rs", &dirty);
        assert_eq!(
            hits.len(),
            1,
            "scanner must flag retired identifier {identifier:?}"
        );
        assert_eq!(hits[0].found, *identifier);
    }

    for fragment in RETIRED_SENTIMENT_FRAGMENTS {
        let dirty = format!("{fragment}\n");
        let hits = retired_sentiment_sites("probe.rs", &dirty);
        assert_eq!(
            hits.len(),
            1,
            "scanner must flag retired syntax fragment {fragment:?}"
        );
        assert_eq!(hits[0].found, *fragment);
    }
}

#[test]
fn red_proof_retired_sentiment_scanner_ignores_tests_comments_and_lookalikes() {
    let gated = "#[cfg(test)]\nmod t {\n    struct SentimentCurve;\n}\n";
    assert!(retired_sentiment_sites("probe.rs", gated).is_empty());

    let comments = "// SentimentCategory was deleted\n/// no LadderSource::Sentiment\n";
    assert!(retired_sentiment_sites("probe.rs", comments).is_empty());

    let lookalikes = "struct ClientSentimentCategory;\nfn chroma_fractional() {}\n";
    assert!(retired_sentiment_sites("probe.rs", lookalikes).is_empty());
}

#[test]
fn red_proof_cfg_test_stripper_ignores_non_code_braces_without_hiding_live_code() {
    let opening_in_string = "#[cfg(test)]\nmod t {\n    const S: &str = \"{\";\n}\n\
                             struct SentimentCurve;\n";
    let hits = retired_sentiment_sites("probe.rs", opening_in_string);
    assert_eq!(
        hits.len(),
        1,
        "a brace in a test string must not hide a later production violation"
    );

    let closing_in_test_text = "#[cfg(test)]\nmod t {\n    const S: &str = \"}\";\n\
                                /* } */\n    struct SentimentCurve;\n}\n";
    assert!(
        retired_sentiment_sites("probe.rs", closing_in_test_text).is_empty(),
        "a brace in test text must not end the guarded item early"
    );

    let non_braced_item = "#[cfg(test)]\nconst TEST_TEXT: &str = \"{\";\nstruct SentimentCurve;\n";
    assert_eq!(
        retired_sentiment_sites("probe.rs", non_braced_item).len(),
        1,
        "a brace in a test constant must not turn it into a braced item"
    );

    let next_line_brace = "#[cfg(test)]\nmod t\n{\n    struct SentimentCurve;\n}\n";
    assert!(
        retired_sentiment_sites("probe.rs", next_line_brace).is_empty(),
        "a guarded braced item may open on a later line"
    );

    let other_non_code = r####"#[cfg(test)]
mod t {
    const RAW: &str = r##"{"##;
    const CHAR: char = '{';
    /* { /* } */ { */
}
struct SentimentCurve;
"####;
    assert_eq!(
        retired_sentiment_sites("probe.rs", other_non_code).len(),
        1,
        "raw strings, chars, and nested block comments must not hide live code"
    );

    let fake_attribute_in_comment = "/*\n#[cfg(test)]\nmod fake {\n*/\nstruct SentimentCurve;\n";
    assert_eq!(
        retired_sentiment_sites("probe.rs", fake_attribute_in_comment).len(),
        1,
        "cfg-looking text inside a block comment is not an attribute"
    );

    let fake_attribute_in_raw = r####"const TEXT: &str = r##"
#[cfg(test)]
mod fake {
"##;
struct SentimentCurve;
"####;
    assert_eq!(
        retired_sentiment_sites("probe.rs", fake_attribute_in_raw).len(),
        1,
        "cfg-looking text inside a raw string is not an attribute"
    );

    let same_line_attribute = "#[cfg(test)] mod t {}\nstruct SentimentCurve;\n";
    assert_eq!(
        retired_sentiment_sites("probe.rs", same_line_attribute).len(),
        1,
        "an unsupported same-line cfg item must fail closed instead of hiding later code"
    );

    let live_suffix = "#[cfg(test)]\nmod t {} struct SentimentCurve;\n";
    assert_eq!(
        retired_sentiment_sites("probe.rs", live_suffix).len(),
        1,
        "production code after a guarded item on the same line must remain visible"
    );

    let same_line_followup_attribute =
        "#[cfg(test)]\n#[allow(dead_code)] mod t {}\nstruct SentimentCurve;\n";
    assert_eq!(
        retired_sentiment_sites("probe.rs", same_line_followup_attribute).len(),
        1,
        "a same-line follow-up attribute must not make the next production item guarded"
    );

    let block_comment_suffix = "#[cfg(test)]\nmod t { struct SentimentCurve; } /* rationale */\n";
    assert!(
        retired_sentiment_sites("probe.rs", block_comment_suffix).is_empty(),
        "a closed block-comment suffix is not production code"
    );

    let block_comment_on_marker =
        "#[cfg(test)] /* test fixture */\nmod t { struct SentimentCurve; }\n";
    assert!(
        retired_sentiment_sites("probe.rs", block_comment_on_marker).is_empty(),
        "a closed block comment after cfg(test) must not make the marker ambiguous"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// RED-proofs — prove the shared scanner BITES on a violation and is SILENT on the
// two legitimate cases (cfg(test) block, comment). A mutation that neuters the
// detector or the cfg(test) stripper turns one of these RED (INV-4).
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn red_proof_hex_scanner_bites_on_injected_anchor_in_code() {
    // A hex in live code (before an inline comment) is flagged.
    let dirty = "    let seed = \"#3E87FF\"; // injected brand anchor\n";
    let hits = forbidden_hex_sites("probe.rs", dirty);
    assert_eq!(
        hits.len(),
        1,
        "scanner must flag a brand hex in production code"
    );
    assert_eq!(hits[0].found, "#3E87FF");

    let scheme_literal = "const URL: &str = \"scheme://palette/#3E87FF\";\n";
    let hits = forbidden_hex_sites("probe.rs", scheme_literal);
    assert_eq!(
        hits.len(),
        1,
        "comment syntax inside a string must not hide a production anchor"
    );
    let raw_scheme_literal = r##"const URL: &str = r#"scheme://palette/#3E87FF"#;
"##;
    assert_eq!(
        forbidden_hex_sites("probe.rs", raw_scheme_literal).len(),
        1,
        "comment syntax inside a raw string must remain production code"
    );

    // Clean agnostic code has none.
    let clean = "    let set = resolve_named_set(bg, table, vc)?;\n";
    assert!(
        forbidden_hex_sites("probe.rs", clean).is_empty(),
        "agnostic code must be hex-clean"
    );
}

#[test]
fn red_proof_client_calibration_scanner_bites_and_respects_cfg_test() {
    for identifier in CLIENT_CALIBRATION_IDENTIFIERS {
        let dirty = format!("fn probe() {{ let _ = {identifier}; }}\n");
        let hits = client_calibration_sites("probe.rs", &dirty);
        assert_eq!(hits.len(), 1, "scanner must flag {identifier}");
        assert_eq!(hits[0].found, *identifier);

        let gated =
            format!("#[cfg(test)]\nmod t {{\n  fn probe() {{ let _ = {identifier}; }}\n}}\n");
        assert!(
            client_calibration_sites("probe.rs", &gated).is_empty(),
            "test-only calibration {identifier} must remain legal"
        );
    }
}

#[test]
fn red_proof_hex_scanner_is_silent_on_cfg_test_and_comments() {
    // Inside a `#[cfg(test)]` block: invisible (stripped).
    let gated = "#[cfg(test)]\nmod t {\n    const A: &str = \"#FF3B30\";\n}\n";
    assert!(
        forbidden_hex_sites("probe.rs", gated).is_empty(),
        "a hex inside #[cfg(test)] must NOT be flagged (stripper failed)"
    );

    // In a comment (line + doc): invisible (cut at `//`).
    let commented = "// the retired anchor was #007AFF\n/// доки: было #FF9500\n/* #3E87FF */\n";
    assert!(
        forbidden_hex_sites("probe.rs", commented).is_empty(),
        "a cited hex in a comment must NOT be flagged"
    );
}

#[test]
fn red_proof_definition_scanner_bites_on_ungated_enum() {
    // An un-gated showcase enum in production is flagged…
    let dirty = "pub enum Accent {\n    Red,\n    Blue,\n}\n";
    let hits = forbidden_definition_sites("probe.rs", dirty);
    assert_eq!(hits.len(), 1, "an un-gated `enum Accent` must be flagged");
    assert_eq!(hits[0].found, "enum Accent");

    // …but the same enum under `#[cfg(test)]` is not.
    let gated = "#[derive(Debug)]\n#[cfg(test)]\npub enum Accent {\n    Red,\n}\n";
    assert!(
        forbidden_definition_sites("probe.rs", gated).is_empty(),
        "a `#[cfg(test)]` showcase enum is allowed"
    );
}

#[test]
fn red_proof_definition_scanner_ignores_prefix_shared_production_types() {
    // Production types that merely share a prefix with a showcase type must NOT be
    // flagged — identifier boundaries, not substring.
    let production = "pub enum RoleChroma {\n    Neutral,\n}\npub struct NamedRoleTable;\n\
                      pub struct RoleSpec;\n";
    assert!(
        forbidden_definition_sites("probe.rs", production).is_empty(),
        "RoleChroma / NamedRoleTable / RoleSpec are production types, not the showcase \
         `Role`/`RoleTable` — must not be flagged"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Meta — the exclusion derivation itself must resolve the known test modules, so a
// silent break in `cfg_test_module_files` (which would make the gates scan test
// files and false-RED, or worse, scan nothing) is caught.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn cfg_test_module_exclusion_covers_the_relocated_oracles() {
    let excluded = cfg_test_module_files();
    for expected in [
        "accent_golden_tests.rs",
        "r3_byte_identity_tests.rs",
        "continuity_tests.rs",
        "dim_tinted_tests.rs",
        "config/preset.rs",
        "config/fixture.rs",
        "config/tests.rs",
    ] {
        let path = src_dir().join(expected);
        assert!(
            excluded.contains(&path),
            "cfg(test) module exclusion must cover {expected} (derivation drifted); \
             excluded set: {excluded:#?}"
        );
    }
    // And a genuine production module must NOT be excluded.
    assert!(
        !excluded.contains(&src_dir().join("semantic.rs")),
        "semantic.rs is production and must be scanned, not excluded"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Observer-fit (single-observer / N=1) calibration gate (Front B).
//
// BUG CLASS this guards: a constant CALIBRATED to ONE observer's perception
// (N=1, single rater, no reliability model) re-enters the PRODUCTION surface of
// the agnostic core. The engine must emit only values it can honestly ground in
// measurement or geometry; a single-observer DECLARED-CALIBRATION scalar — the
// removed `confidence` layer (`M_W` / `KAPPA_CORE` / `KAPPA_INTERIOR`) was
// exactly this — is observer-fit noise the agnostic contract forbids. Like the
// sibling gates above, the failure mode is INVISIBLE to behavioural tests: an
// observer-fit constant still WORKS, so every value/golden test stays green —
// the core has merely stopped being agnostic. This gate turns that regression
// RED by scanning the production (non-`cfg(test)`) `src` for the single-observer
// provenance SIGNATURE.
//
// WHY THE SIGNATURE, NOT BARE `DECLARED-CALIBRATION`: a `DECLARED-CALIBRATION`
// marker alone is legitimate — a design-choice knob carries it (e.g.
// `HUE_DRIFT_PENALTY_SLOPE`), and a RETIRED threshold is still CITED in comments (the
// removed M-03 light-escape). The forbidden class is specifically the
// SINGLE-OBSERVER fit, whose provenance always carries `N=1` /
// `single-observer` / `однонаблюдательск`. Keying on that signature makes the
// gate BITE on observer-fit re-entry yet stay SILENT on honest design-choice
// calibration and on historical citations (proved by the RED-proofs below).
//
// Comments are NOT cut here (unlike the hex/definition scanners): the provenance
// signature lives in the doc/annotation adjacent to the constant, and that is
// exactly what the gate reads. `#[cfg(test)]` items are still stripped, so the
// engine's own test oracles (which legitimately discuss N=1 provenance) are out
// of scope.
//
// INV: the GATE test and the two `red_proof_observer_fit_*` tests share the same
// `single_observer_calibration_sites` scanner, so a mutation to the detector is
// caught by a RED-proof, not merely asserted.
// ─────────────────────────────────────────────────────────────────────────────

/// Lowercased provenance substrings that unambiguously denote single-observer
/// (N=1) calibration. Matched as plain substrings; `n=1` is handled separately
/// as a bounded token (see `has_single_observer_marker`).
const SINGLE_OBSERVER_SUBSTR_MARKERS: &[&str] = &["single-observer", "однонаблюдательск"];

/// True when `lower` (an already-lowercased line) carries a single-observer
/// calibration signature: one of the plain substrings, or the bounded token
/// `n=1` — bounded so a word ENDING in `n` before `=1` (`GOLDEN=1`) is not a
/// false hit (the char before `n` must not be an identifier character).
fn has_single_observer_marker(lower: &str) -> bool {
    if SINGLE_OBSERVER_SUBSTR_MARKERS
        .iter()
        .any(|m| lower.contains(m))
    {
        return true;
    }
    let bytes = lower.as_bytes();
    lower.match_indices("n=1").any(|(at, _)| {
        at == 0 || {
            let prev = bytes[at - 1];
            !(prev.is_ascii_alphanumeric() || prev == b'_')
        }
    })
}

/// Every production-code line of `source` (`#[cfg(test)]` stripped, comments
/// KEPT) carrying a single-observer calibration signature.
fn single_observer_calibration_sites(module: &str, source: &str) -> Vec<Site> {
    let mut out = Vec::new();
    for (line, text) in production_lines(source) {
        if has_single_observer_marker(&text.to_lowercase()) {
            out.push(Site {
                module: module.to_string(),
                line,
                found: text.trim().to_string(),
            });
        }
    }
    out
}

#[test]
fn production_src_carries_no_single_observer_calibration() {
    let mut leaks = Vec::new();
    for file in production_src_files() {
        let source = std::fs::read_to_string(&file)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", file.display()));
        leaks.extend(single_observer_calibration_sites(
            &module_label(&file),
            &source,
        ));
    }
    assert!(
        leaks.is_empty(),
        "AGNOSTIC-CLEANLINESS: single-observer (N=1) calibration re-entered PRODUCTION code. \
         An observer-fit constant must be REMOVED or supplied by the consumer via config — the \
         agnostic core never emits a value fit to one rater's perception. Offending sites: \
         {leaks:#?}"
    );
}

#[test]
fn red_proof_observer_fit_scanner_bites_on_single_observer_signature() {
    // Every single-observer signature in production code is flagged.
    for dirty in [
        "pub const KAPPA: f64 = 0.34; // DECLARED-CALIBRATION N=1, single rater\n",
        "// однонаблюдательская калибровка владельца, reliability-модели нет\n",
        "/// derived from the owner's single-observer labelling (738 labels)\n",
    ] {
        let hits = single_observer_calibration_sites("probe.rs", dirty);
        assert_eq!(
            hits.len(),
            1,
            "scanner must flag the single-observer signature in {dirty:?}"
        );
    }
}

#[test]
fn red_proof_observer_fit_scanner_is_silent_on_cfg_test_bare_calibration_and_lookalikes() {
    // Inside a `#[cfg(test)]` block: invisible (stripped) — the engine's own test
    // oracles legitimately discuss N=1 provenance.
    let gated = "#[cfg(test)]\nmod t {\n    // N=1 single-observer calibration note\n}\n";
    assert!(
        single_observer_calibration_sites("probe.rs", gated).is_empty(),
        "a signature inside #[cfg(test)] must NOT be flagged (stripper failed)"
    );

    // A bare DECLARED-CALIBRATION marker WITHOUT the single-observer signature is a
    // legitimate design-choice knob or a historical citation — must stay silent.
    let bare = "// HUE_DRIFT_PENALTY_SLOPE is a DECLARED-CALIBRATION design choice\n\
                // the former M-03 light-escape threshold (DECLARED-CALIBRATION) was removed\n";
    assert!(
        single_observer_calibration_sites("probe.rs", bare).is_empty(),
        "bare DECLARED-CALIBRATION (no N=1 / single-observer) must NOT be flagged — else the \
         gate would false-RED on every honest design-choice knob and historical citation"
    );

    // Look-alikes that merely END in `n` before `=1` are not the token `n=1`.
    let lookalike = "    // regenerate with BLESS_LABUI_GOLDEN=1 for a reviewed change\n";
    assert!(
        single_observer_calibration_sites("probe.rs", lookalike).is_empty(),
        "`GOLDEN=1` must NOT match the bounded token `n=1`"
    );

    // Clean agnostic code has none.
    let clean = "    let set = resolve_named_set(bg, table, vc)?;\n";
    assert!(
        single_observer_calibration_sites("probe.rs", clean).is_empty(),
        "agnostic code must be observer-fit-clean"
    );
}
