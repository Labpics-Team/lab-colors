use std::path::Path;

use crate::input::{self, AuditInputError};
use sha2::{Digest, Sha256};
use syn::{ImplItem, Item, ItemFn, ItemImpl, Visibility};

/// Одна извлечённая публичная операция (функция или метод).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationEntry {
    /// Относительный путь к файлу (через `/`).
    pub path: String,
    /// Имя крейта, извлечённое из пути `crates/<name>/`.
    pub crate_name: String,
    /// `"fn"` для свободной функции, `"method"` для метода в impl.
    pub kind: String,
    /// Имя функции/метода.
    pub name: String,
    /// Нормализованная сигнатура (без тел, без атрибутов).
    pub signature: String,
    /// SHA-256 от нормализованной сигнатуры.
    pub signature_sha256: String,
}

/// Извлекает все публичные операции из workspace.
///
/// Обходит `workspace_root/crates/**/*.rs`, парсит каждый файл через `syn`,
/// собирает `pub fn` (не геттеры/сеттеры) и публичные методы в `impl`.
/// Результат детерминированно отсортирован по `(path, name)`.
pub fn extract_operations(workspace_root: &Path) -> Result<Vec<OperationEntry>, AuditInputError> {
    let mut entries = Vec::new();
    for path in input::rust_files(workspace_root)? {
        let content = input::text(&path)?;
        let file = input::rust(&path, &content)?;
        let rel = input::relative(workspace_root, &path)?;
        let crate_name = extract_crate_name(&rel);
        for item in &file.items {
            match item {
                Item::Fn(f) if is_pub_fn(f) => {
                    let sig = normalize_signature(&f.sig);
                    entries.push(OperationEntry {
                        path: rel.clone(),
                        crate_name: crate_name.clone(),
                        kind: "fn".to_string(),
                        name: f.sig.ident.to_string(),
                        signature: sig.clone(),
                        signature_sha256: sha256_hex(&sig),
                    });
                }
                Item::Impl(imp) => {
                    collect_impl_methods(imp, &rel, &crate_name, &mut entries);
                }
                _ => {}
            }
        }
    }

    entries.sort_by(|a, b| a.path.cmp(&b.path).then_with(|| a.name.cmp(&b.name)));
    Ok(entries)
}

fn collect_impl_methods(
    imp: &ItemImpl,
    path: &str,
    crate_name: &str,
    out: &mut Vec<OperationEntry>,
) {
    for item in &imp.items {
        if let ImplItem::Fn(m) = item {
            if !matches!(m.vis, Visibility::Public(_)) {
                continue;
            }
            let name = m.sig.ident.to_string();
            // Исключаем геттеры/сеттеры по соглашению
            let is_accessor = name.starts_with("get_") || name.starts_with("set_");
            if is_accessor {
                continue;
            }
            let sig = normalize_signature(&m.sig);
            out.push(OperationEntry {
                path: path.to_string(),
                crate_name: crate_name.to_string(),
                kind: "method".to_string(),
                name,
                signature: sig.clone(),
                signature_sha256: sha256_hex(&sig),
            });
        }
    }
}

fn is_pub_fn(f: &ItemFn) -> bool {
    matches!(f.vis, Visibility::Public(_))
}

fn normalize_signature(sig: &syn::Signature) -> String {
    let tokens = quote::quote!(#sig);
    tokens.to_string()
}

fn sha256_hex(input: &str) -> String {
    let hash = Sha256::digest(input.as_bytes());
    hex_encode(&hash)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn extract_crate_name(rel_path: &str) -> String {
    rel_path
        .strip_prefix("crates/")
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("unknown")
        .to_string()
}
