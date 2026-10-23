//! Проверяем реальные export-атрибуты, а не вручную отмеченный список готовности.
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::path::Path;

use syn::{Attribute, FnArg, ImplItem, Item, Signature, Type, Visibility};

type Surface = BTreeMap<String, usize>;

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
            && matches!(attribute.parse_args::<syn::Meta>(), Ok(syn::Meta::Path(path)) if path.is_ident("test"))
    })
}

fn insert(output: &mut Surface, owner: &str, signature: &Signature) {
    let key = format!("{owner}::{}", signature.ident);
    let arity = signature
        .inputs
        .iter()
        .filter(|arg| matches!(arg, FnArg::Typed(_)))
        .count();
    assert!(
        output.insert(key.clone(), arity).is_none(),
        "ambiguous exported operation: {key}"
    );
}

fn exported_items(items: &[Item], binding: &str, output: &mut Surface) {
    for item in items {
        match item {
            Item::Fn(function)
                if attribute(&function.attrs, binding)
                    && !test_only(&function.attrs)
                    && matches!(function.vis, Visibility::Public(_)) =>
            {
                insert(output, "fn", &function.sig);
            }
            Item::Impl(implementation)
                if attribute(&implementation.attrs, binding)
                    && !test_only(&implementation.attrs) =>
            {
                let Type::Path(path) = implementation.self_ty.as_ref() else {
                    panic!("exported receiver must have a named type");
                };
                let owner = path
                    .path
                    .segments
                    .last()
                    .expect("receiver name")
                    .ident
                    .to_string();
                for item in &implementation.items {
                    if let ImplItem::Fn(method) = item {
                        if matches!(method.vis, Visibility::Public(_)) && !test_only(&method.attrs)
                        {
                            insert(output, &owner, &method.sig);
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

fn module_surface(path: &Path, binding: &str, output: &mut Surface) {
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

fn missing_operations<'a>(portable: &'a Surface, native: &Surface) -> Vec<&'a str> {
    portable
        .iter()
        .filter_map(|(key, arity)| (native.get(key) != Some(arity)).then_some(key.as_str()))
        .collect()
}

#[test]
fn native_surface_covers_every_exported_wasm_operation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut portable = Surface::new();
    let mut native = Surface::new();
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
    assert_eq!(
        portable.get("fn::compile_program_wire"),
        Some(&2),
        "canonical product root is missing"
    );
    assert_eq!(portable.get("ProgramRuntime::update_observed"), Some(&4));
    let missing = missing_operations(&portable, &native);
    assert!(
        missing.is_empty(),
        "native adapter has missing operations or divergent arity: {missing:?}"
    );

    // Не условие запуска теста: сравнение выше выполняется всегда. Apple CI
    // дополнительно сверяет этот вывод с символами реально собранного SDK.
    if let Some(path) = std::env::var_os("LABCOLORS_NATIVE_SURFACE_OUTPUT") {
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .unwrap();
        serde_json::to_writer(
            file,
            &serde_json::json!({"schemaVersion": 1, "operations": native}),
        )
        .unwrap();
    }
}

#[test]
fn discovery_detects_added_operations_and_ignores_test_helpers() {
    let source = syn::parse_file(r#"
        #[wasm_bindgen] pub fn existing(value: u32) {}
        #[wasm_bindgen] impl Runtime { pub fn advance(&self, revision: u64) {} fn private(&self) {} }
        #[cfg(test)] mod tests { #[wasm_bindgen] pub fn fixture() {} }
        #[cfg(test)] #[wasm_bindgen] pub fn isolated_fixture() {}
        #[cfg(not(test))] #[wasm_bindgen] pub fn production() {}
    "#).unwrap();
    let mut found = Surface::new();
    exported_items(&source.items, "wasm_bindgen", &mut found);
    assert_eq!(
        found,
        Surface::from([
            ("fn::existing".to_string(), 1),
            ("Runtime::advance".to_string(), 1),
            ("fn::production".to_string(), 0)
        ])
    );
    let original = found.clone();
    let additional = syn::parse_file("#[wasm_bindgen] pub fn next_capability() {}").unwrap();
    exported_items(&additional.items, "wasm_bindgen", &mut found);
    assert_eq!(
        missing_operations(&found, &original),
        ["fn::next_capability"]
    );
    let mut changed_arity = original.clone();
    changed_arity.insert("fn::existing".into(), 2);
    assert_eq!(
        missing_operations(&original, &changed_arity),
        ["fn::existing"]
    );
    assert!(missing_operations(&original, &found).is_empty());
}
