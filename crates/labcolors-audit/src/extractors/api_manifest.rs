use sha2::{Digest, Sha256};
use std::path::Path;
use syn::{Item, Visibility};
use walkdir::WalkDir;

/// Одна запись публичного API, извлечённая из исходников.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiManifestEntry {
    /// Относительный путь к файлу (forward-slash).
    pub path: String,
    /// Имя крейта (из Cargo.toml или имени директории).
    pub crate_name: String,
    /// Вид элемента: "fn", "struct", "enum", "trait", "type", "const".
    pub kind: String,
    /// Имя элемента.
    pub name: String,
    /// Видимость: "pub" или "pub(crate)" и т.д.
    pub visibility: String,
    /// Нормализованная сигнатура для diff.
    pub signature: String,
    /// Первая строка doc-комментария (без ///).
    pub doc_summary: String,
    /// SHA-256 сигнатуры для детекции изменений.
    pub signature_sha256: String,
}

/// Извлекает публичное API всех крейтов workspace.
///
/// Обходит `workspace_root/crates/*/src/**/*.rs`, парсит каждый файл через
/// syn, собирает pub-элементы (fn, struct, enum, trait, type, const).
/// Пропускает тестовые модули (`#[cfg(test)]`, пути с `/tests/`).
/// Включает элементы с `#[wasm_bindgen]`.
/// Результат отсортирован по (path, name) для детерминизма.
pub fn extract_public_api(workspace_root: &Path) -> Vec<ApiManifestEntry> {
    let crates_dir = workspace_root.join("crates");
    if !crates_dir.is_dir() {
        return Vec::new();
    }

    let mut entries = Vec::new();

    for crate_entry in std::fs::read_dir(&crates_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
    {
        let crate_path = crate_entry.path();
        if !crate_path.is_dir() {
            continue;
        }
        let crate_name = crate_entry.file_name().to_string_lossy().to_string();
        let src_dir = crate_path.join("src");
        if !src_dir.is_dir() {
            continue;
        }

        for walk_entry in WalkDir::new(&src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let file_path = walk_entry.path();
            if !file_path.is_file() {
                continue;
            }
            if file_path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }

            let rel = relative_path(workspace_root, file_path);

            // Пропускаем тестовые файлы и директории tests/
            if rel.contains("/tests/") || rel.contains("\\tests\\") {
                continue;
            }
            let file_name = file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if file_name.ends_with("_test.rs") || file_name.ends_with("_tests.rs") {
                continue;
            }

            let content = match std::fs::read_to_string(file_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let file = match syn::parse_file(&content) {
                Ok(f) => f,
                Err(_) => continue,
            };

            extract_items_from_file(&file.items, &rel, &crate_name, &mut entries);
        }
    }

    // Детерминированная сортировка
    entries.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.name.cmp(&b.name)));
    entries
}

fn extract_items_from_file(
    items: &[syn::Item],
    path: &str,
    crate_name: &str,
    out: &mut Vec<ApiManifestEntry>,
) {
    for item in items {
        // Пропускаем #[cfg(test)] модули
        if is_cfg_test(item) {
            continue;
        }

        match item {
            Item::Fn(f) if is_pub(&f.vis) => {
                let sig = normalize_fn_signature(&f.sig);
                let doc = extract_doc_summary(&f.attrs);
                push_entry(out, path, crate_name, "fn", &f.sig.ident, &f.vis, &sig, &doc);
            }
            Item::Struct(s) if is_pub(&s.vis) => {
                let sig = format!("struct {}", s.ident);
                let doc = extract_doc_summary(&s.attrs);
                push_entry(out, path, crate_name, "struct", &s.ident, &s.vis, &sig, &doc);
            }
            Item::Enum(e) if is_pub(&e.vis) => {
                let sig = format!("enum {}", e.ident);
                let doc = extract_doc_summary(&e.attrs);
                push_entry(out, path, crate_name, "enum", &e.ident, &e.vis, &sig, &doc);
            }
            Item::Trait(t) if is_pub(&t.vis) => {
                let sig = format!("trait {}", t.ident);
                let doc = extract_doc_summary(&t.attrs);
                push_entry(out, path, crate_name, "trait", &t.ident, &t.vis, &sig, &doc);
            }
            Item::Type(t) if is_pub(&t.vis) => {
                let sig = format!("type {}", t.ident);
                let doc = extract_doc_summary(&t.attrs);
                push_entry(out, path, crate_name, "type", &t.ident, &t.vis, &sig, &doc);
            }
            Item::Const(c) if is_pub(&c.vis) => {
                let sig = format!("const {}", c.ident);
                let doc = extract_doc_summary(&c.attrs);
                push_entry(out, path, crate_name, "const", &c.ident, &c.vis, &sig, &doc);
            }
            // Рекурсивно обрабатываем pub mod для вложенных pub-элементов
            Item::Mod(m) if is_pub(&m.vis) => {
                if let Some((_, items)) = &m.content {
                    extract_items_from_file(items, path, crate_name, out);
                }
            }
            _ => {}
        }
    }
}

fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

fn is_cfg_test(item: &Item) -> bool {
    let attrs = match item {
        Item::Fn(f) => &f.attrs,
        Item::Struct(s) => &s.attrs,
        Item::Enum(e) => &e.attrs,
        Item::Trait(t) => &t.attrs,
        Item::Type(t) => &t.attrs,
        Item::Const(c) => &c.attrs,
        Item::Mod(m) => &m.attrs,
        _ => return false,
    };
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .iter()
            .any(|seg| seg.ident == "cfg")
            && attr.meta.to_token_stream().to_string().contains("test")
    })
}

use quote::ToTokens;

fn normalize_fn_signature(sig: &syn::Signature) -> String {
    // Убираем пробельные различия: пересобираем через ToTokens
    sig.to_token_stream().to_string().replace("  ", " ")
}

fn extract_doc_summary(attrs: &[syn::Attribute]) -> String {
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit) = &expr_lit.lit {
                        let val = lit.value();
                        let trimmed = val.trim();
                        if !trimmed.is_empty() {
                            return trimmed.to_string();
                        }
                    }
                }
            }
        }
    }
    String::new()
}

#[allow(clippy::too_many_arguments)]
fn push_entry(
    out: &mut Vec<ApiManifestEntry>,
    path: &str,
    crate_name: &str,
    kind: &str,
    ident: &syn::Ident,
    vis: &Visibility,
    signature: &str,
    doc_summary: &str,
) {
    let vis_str = match vis {
        Visibility::Public(_) => "pub".to_string(),
        _ => vis.to_token_stream().to_string(),
    };
    let hash = sha256_hex(signature);
    out.push(ApiManifestEntry {
        path: path.to_string(),
        crate_name: crate_name.to_string(),
        kind: kind.to_string(),
        name: ident.to_string(),
        visibility: vis_str,
        signature: signature.to_string(),
        doc_summary: doc_summary.to_string(),
        signature_sha256: hash,
    });
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}