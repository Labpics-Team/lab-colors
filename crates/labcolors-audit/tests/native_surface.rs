//! Проверяем реальные export-атрибуты, а не вручную отмеченный список готовности.
use std::collections::BTreeSet;
use std::path::Path;

use syn::{Attribute, ImplItem, Item, Type, Visibility};

fn attribute(attributes: &[Attribute], name: &str) -> bool {
    attributes.iter().any(|attribute| {
        attribute
            .path()
            .segments
            .last()
            .is_some_and(|part| part.ident == name)
    })
}

fn test_only(attributes: &[Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute.path().is_ident("cfg")
            && quote::quote!(#attribute)
                .to_string()
                .split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word == "test")
    })
}

fn exported_items(items: &[Item], binding: &str, output: &mut BTreeSet<String>) {
    for item in items {
        match item {
            Item::Fn(function)
                if attribute(&function.attrs, binding)
                    && matches!(function.vis, Visibility::Public(_)) =>
            {
                output.insert(format!("fn::{}", function.sig.ident));
            }
            Item::Impl(implementation) if attribute(&implementation.attrs, binding) => {
                let Type::Path(path) = implementation.self_ty.as_ref() else {
                    panic!("exported receiver must have a named type");
                };
                let owner = &path.path.segments.last().expect("receiver name").ident;
                for item in &implementation.items {
                    if let ImplItem::Fn(method) = item {
                        if matches!(method.vis, Visibility::Public(_)) {
                            output.insert(format!("{owner}::{}", method.sig.ident));
                        }
                    }
                }
            }
            Item::Mod(module) if !test_only(&module.attrs) => {
                if let Some((_, items)) = &module.content {
                    exported_items(items, binding, output);
                }
            }
            _ => {}
        }
    }
}

fn module_surface(path: &Path, binding: &str, output: &mut BTreeSet<String>) {
    let source = std::fs::read_to_string(path).expect("binding module must be readable");
    let module = syn::parse_file(&source).expect("binding must parse as Rust");
    exported_items(&module.items, binding, output);
    let parent = path.parent().expect("module parent");
    let directory = match path.file_stem().and_then(|name| name.to_str()) {
        Some("lib" | "main" | "mod") => parent.to_path_buf(),
        Some(name) => parent.join(name),
        None => panic!("module name"),
    };
    for item in module.items {
        if let Item::Mod(module) = item {
            if module.content.is_none() && !test_only(&module.attrs) {
                assert!(
                    !attribute(&module.attrs, "path"),
                    "custom binding module paths need explicit resolution"
                );
                let file = directory.join(format!("{}.rs", module.ident));
                let file = if file.is_file() {
                    file
                } else {
                    directory.join(module.ident.to_string()).join("mod.rs")
                };
                module_surface(&file, binding, output);
            }
        }
    }
}

#[test]
fn native_surface_covers_every_exported_wasm_operation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut portable = BTreeSet::new();
    let mut native = BTreeSet::new();
    module_surface(
        &root.join("labcolors-wasm/src/lib.rs"),
        "wasm_bindgen",
        &mut portable,
    );
    module_surface(
        &root.join("labcolors-ffi/src/lib.rs"),
        "export",
        &mut native,
    );
    assert!(
        portable.len() > 30,
        "an empty or partial discovery cannot certify coverage"
    );
    let missing: Vec<_> = portable.difference(&native).collect();
    assert!(
        missing.is_empty(),
        "native adapter is missing public product operations: {missing:?}"
    );
}

#[test]
fn discovery_detects_added_operations_and_ignores_test_helpers() {
    let source = syn::parse_file(
        r#"
        #[wasm_bindgen] pub fn existing() {}
        #[wasm_bindgen] impl Runtime { pub fn advance(&self) {} fn private(&self) {} }
        #[cfg(test)] mod tests { #[wasm_bindgen] pub fn fixture() {} }
    "#,
    )
    .unwrap();
    let mut found = BTreeSet::new();
    exported_items(&source.items, "wasm_bindgen", &mut found);
    assert_eq!(
        found,
        BTreeSet::from(["fn::existing".to_string(), "Runtime::advance".to_string()])
    );
    let additional = syn::parse_file("#[wasm_bindgen] pub fn next_capability() {}").unwrap();
    exported_items(&additional.items, "wasm_bindgen", &mut found);
    assert!(found.contains("fn::next_capability"));
    assert_eq!(found.len(), 3);
}
