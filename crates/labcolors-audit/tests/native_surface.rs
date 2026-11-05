//! Проверяем реальные export-атрибуты, а не вручную отмеченный список готовности.
use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::path::Path;

use quote::ToTokens;
use syn::{Attribute, FnArg, ImplItem, Item, ItemStruct, Signature, Type, Visibility};

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

fn module_surface(path: &Path, binding: &str, output: &mut Surface) -> Vec<Item> {
    let source = std::fs::read_to_string(path).expect("binding module must be readable");
    let module = syn::parse_file(&source).expect("binding must parse as Rust");
    exported_items(&module.items, binding, output);
    let parent = path.parent().expect("module parent");
    let directory = match path.file_stem().and_then(|name| name.to_str()) {
        Some("lib" | "main" | "mod") => parent.to_path_buf(),
        Some(name) => parent.join(name),
        None => panic!("module name"),
    };
    let mut nested = Vec::new();
    for item in &module.items {
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
                nested.extend(module_surface(&file, binding, output));
            }
        }
    }
    let mut items = module.items;
    items.extend(nested);
    items
}

fn named_declarations<'a>(items: &'a [Item], name: &str, found: &mut Vec<&'a Item>) {
    for item in items {
        match item {
            Item::Fn(function) if function.sig.ident == name && !test_only(&function.attrs) => {
                found.push(item);
            }
            Item::Struct(record) if record.ident == name && !test_only(&record.attrs) => {
                found.push(item);
            }
            Item::Mod(module) if !test_only(&module.attrs) => {
                if let Some((_, items)) = &module.content {
                    named_declarations(items, name, found);
                }
            }
            _ => {}
        }
    }
}

fn declaration<'a>(items: &'a [Item], name: &str) -> Result<&'a Item, String> {
    let mut found = Vec::new();
    named_declarations(items, name, &mut found);
    match found.as_slice() {
        [item] => Ok(*item),
        _ => Err(format!(
            "missing or ambiguous projection declaration: {name}"
        )),
    }
}

fn projection_signature<'a>(
    items: &'a [Item],
    binding: &syn::Path,
    expected: &Signature,
) -> Result<&'a Signature, String> {
    let name = expected.ident.to_string();
    let Item::Fn(function) = declaration(items, &name)? else {
        return Err(format!("projected operation is not a function: {name}"));
    };
    if !matches!(function.vis, Visibility::Public(_))
        || !function.attrs.iter().any(|attribute| {
            attribute.path().to_token_stream().to_string() == binding.to_token_stream().to_string()
        })
        || function.sig.to_token_stream().to_string() != expected.to_token_stream().to_string()
    {
        return Err(format!(
            "missing export or divergent projected signature: {name}"
        ));
    }
    Ok(&function.sig)
}

fn projection_record<'a>(
    items: &'a [Item],
    expected: &ItemStruct,
) -> Result<&'a ItemStruct, String> {
    let name = expected.ident.to_string();
    let Item::Struct(record) = declaration(items, &name)? else {
        return Err(format!("projected binding is not a record: {name}"));
    };
    let record_path: syn::Path = syn::parse_quote!(uniffi::Record);
    let exported = record.attrs.iter().any(|attribute| {
        attribute.path().is_ident("derive")
            && attribute
                .parse_args_with(
                    syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
                )
                .is_ok_and(|paths| {
                    paths.iter().any(|path| {
                        path.to_token_stream().to_string()
                            == record_path.to_token_stream().to_string()
                    })
                })
    });
    if !matches!(record.vis, Visibility::Public(_))
        || !exported
        || !record.generics.params.is_empty()
        || record.generics.where_clause.is_some()
        || record.fields.to_token_stream().to_string()
            != expected.fields.to_token_stream().to_string()
    {
        return Err(format!(
            "missing export or divergent binding record: {name}"
        ));
    }
    Ok(record)
}

fn coverage_surfaces(
    portable: &Surface,
    native: &Surface,
    portable_items: &[Item],
    native_items: &[Item],
) -> Result<(Surface, Surface), String> {
    let portable_signature = projection_signature(
        portable_items,
        &syn::parse_quote!(wasm_bindgen),
        &syn::parse_quote! {
            fn attach_program_wire(
                bytes: &[u8],
                #[wasm_bindgen(unchecked_param_type = "number")] stream_id: JsValue,
                emission_bindings: &[u32],
                presentation_bindings: &[u32],
                host: &js_sys::Function,
            ) -> Result<ProgramAttachment, JsValue>
        },
    )?;
    let native_signature = projection_signature(
        native_items,
        &syn::parse_quote!(uniffi::export),
        &syn::parse_quote! {
            fn attach_program_wire_with_bindings(
                bytes: Vec<u8>,
                stream_id: u32,
                bindings: ProgramAttachmentBindings,
                host: Box<dyn ProgramPointSinkHost>,
            ) -> Result<Arc<ProgramAttachment>, LabColorsError>
        },
    )?;
    let bindings = projection_record(
        native_items,
        &syn::parse_quote! {
            pub struct ProgramAttachmentBindings {
                pub emissions: Vec<ProgramPointEmissionBinding>,
                pub presentations: Vec<ProgramPointPresentationBinding>,
            }
        },
    )?;
    projection_record(
        native_items,
        &syn::parse_quote! {
            pub struct ProgramPointEmissionBinding {
                pub slot: u32,
                pub sink_output: u32,
            }
        },
    )?;
    projection_record(
        native_items,
        &syn::parse_quote! {
            pub struct ProgramPointPresentationBinding {
                pub slot: u32,
                pub root: u32,
                pub occurrence: u32,
            }
        },
    )?;

    // Проверенная форма record объединяет две WASM-таблицы в один аргумент.
    // Нормализация относится только к покрытию API; семантику проверяет runtime conformance.
    let mut logical_portable = portable.clone();
    let mut logical_native = native.clone();
    let operation = format!("fn::{}", portable_signature.ident);
    let _ = logical_portable.insert(
        operation.clone(),
        portable_signature.inputs.len() - bindings.fields.len() + 1,
    );
    let _ = logical_native.insert(operation, native_signature.inputs.len());
    Ok((logical_portable, logical_native))
}

fn missing_operations<'a>(portable: &'a Surface, native: &Surface) -> Vec<&'a str> {
    portable
        .iter()
        .filter_map(|(key, arity)| (native.get(key) != Some(arity)).then_some(key.as_str()))
        .collect()
}

#[test]
fn native_surface_covers_every_exported_wasm_operation() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("audit crate must have a workspace directory");
    let mut portable = Surface::new();
    let mut native = Surface::new();
    let portable_items = module_surface(
        &root.join("labcolors-wasm/src/lib.rs"),
        "wasm_bindgen",
        &mut portable,
    );
    let native_items = module_surface(
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
    let (logical_portable, logical_native) =
        coverage_surfaces(&portable, &native, &portable_items, &native_items)
            .expect("native attachment binding projection must match its declared API");
    let missing = missing_operations(&logical_portable, &logical_native);
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
            .expect("native inventory output must be a new writable file");
        serde_json::to_writer(
            file,
            &serde_json::json!({"schemaVersion": 1, "operations": native}),
        )
        .expect("native operation inventory must be serializable");
    }
}

const PORTABLE_ATTACHMENT: &str = r#"
    #[wasm_bindgen] pub fn attach_program_wire(
        bytes: &[u8],
        #[wasm_bindgen(unchecked_param_type = "number")] stream_id: JsValue,
        emission_bindings: &[u32],
        presentation_bindings: &[u32],
        host: &js_sys::Function,
    ) -> Result<ProgramAttachment, JsValue> { unimplemented!() }
"#;

const NATIVE_ATTACHMENT: &str = r#"
    #[derive(Debug, Clone, uniffi::Record)]
    pub struct ProgramPointEmissionBinding { pub slot: u32, pub sink_output: u32, }
    #[derive(Debug, Clone, uniffi::Record)]
    pub struct ProgramPointPresentationBinding {
        pub slot: u32, pub root: u32, pub occurrence: u32,
    }
    #[derive(Debug, Clone, uniffi::Record)]
    pub struct ProgramAttachmentBindings {
        pub emissions: Vec<ProgramPointEmissionBinding>,
        pub presentations: Vec<ProgramPointPresentationBinding>,
    }
    #[uniffi::export] pub fn attach_program_wire(
        bytes: Vec<u8>, stream_id: u32, output_slot: u32, sink_output: u32,
        presentation_root: u32, occurrence: u32, host: Box<dyn ProgramPointSinkHost>,
    ) -> Result<Arc<ProgramAttachment>, LabColorsError> { unimplemented!() }
    #[uniffi::export] pub fn attach_program_wire_with_bindings(
        bytes: Vec<u8>,
        stream_id: u32,
        bindings: ProgramAttachmentBindings,
        host: Box<dyn ProgramPointSinkHost>,
    ) -> Result<Arc<ProgramAttachment>, LabColorsError> { unimplemented!() }
"#;

fn fixture_coverage(portable: &str, native: &str) -> Result<(Surface, Surface), String> {
    let portable = syn::parse_file(portable).expect("portable fixture must parse");
    let native = syn::parse_file(native).expect("native fixture must parse");
    let mut portable_surface = Surface::new();
    let mut native_surface = Surface::new();
    exported_items(&portable.items, "wasm_bindgen", &mut portable_surface);
    exported_items(&native.items, "export", &mut native_surface);
    coverage_surfaces(
        &portable_surface,
        &native_surface,
        &portable.items,
        &native.items,
    )
}

#[test]
fn attachment_coverage_accepts_typed_record_projection_without_rewriting_inventory() {
    let portable = syn::parse_file(PORTABLE_ATTACHMENT).expect("portable fixture must parse");
    let native = syn::parse_file(NATIVE_ATTACHMENT).expect("native fixture must parse");
    let mut portable_surface = Surface::new();
    let mut native_surface = Surface::new();
    exported_items(&portable.items, "wasm_bindgen", &mut portable_surface);
    exported_items(&native.items, "export", &mut native_surface);
    assert_eq!(portable_surface.get("fn::attach_program_wire"), Some(&5));
    assert_eq!(native_surface.get("fn::attach_program_wire"), Some(&7));
    assert_eq!(
        native_surface.get("fn::attach_program_wire_with_bindings"),
        Some(&4)
    );
    let original_native = native_surface.clone();
    let (logical_portable, logical_native) = coverage_surfaces(
        &portable_surface,
        &native_surface,
        &portable.items,
        &native.items,
    )
    .expect("typed binding projection must be covered");
    assert_eq!(logical_portable.get("fn::attach_program_wire"), Some(&4));
    assert!(missing_operations(&logical_portable, &logical_native).is_empty());
    assert_eq!(native_surface, original_native);

    let additional =
        format!("{PORTABLE_ATTACHMENT}\n#[wasm_bindgen] pub fn next_capability() {{}}");
    let (logical_portable, logical_native) = fixture_coverage(&additional, NATIVE_ATTACHMENT)
        .expect("binding projection must preserve unrelated exports");
    assert_eq!(
        missing_operations(&logical_portable, &logical_native),
        ["fn::next_capability"]
    );
}

#[test]
fn attachment_coverage_requires_actual_export_and_signature() {
    for (name, before, after) in [
        (
            "missing operation",
            "attach_program_wire_with_bindings",
            "other_operation",
        ),
        (
            "unexported operation",
            "#[uniffi::export] pub fn attach_program_wire_with_bindings",
            "pub fn attach_program_wire_with_bindings",
        ),
        (
            "unrelated export attribute",
            "#[uniffi::export] pub fn attach_program_wire_with_bindings",
            "#[other::export] pub fn attach_program_wire_with_bindings",
        ),
        (
            "private operation",
            "pub fn attach_program_wire_with_bindings",
            "fn attach_program_wire_with_bindings",
        ),
        (
            "test-only operation",
            "#[uniffi::export] pub fn attach_program_wire_with_bindings",
            "#[cfg(test)] #[uniffi::export] pub fn attach_program_wire_with_bindings",
        ),
        (
            "wrong binding type",
            "bindings: ProgramAttachmentBindings,",
            "bindings: Vec<u32>,",
        ),
        (
            "extra argument",
            "bindings: ProgramAttachmentBindings,",
            "bindings: ProgramAttachmentBindings, ignored: u32,",
        ),
        (
            "wrong return type",
            "Result<Arc<ProgramAttachment>, LabColorsError>",
            "Result<ProgramAttachment, LabColorsError>",
        ),
    ] {
        let changed = NATIVE_ATTACHMENT.replace(before, after);
        assert_ne!(
            changed, NATIVE_ATTACHMENT,
            "fixture mutation must apply: {name}"
        );
        assert!(
            fixture_coverage(PORTABLE_ATTACHMENT, &changed).is_err(),
            "{name}"
        );
    }
}

#[test]
fn attachment_coverage_rejects_malformed_binding_records() {
    for (name, before, after) in [
        ("missing record export", "Clone, uniffi::Record", "Clone"),
        (
            "extra aggregate field",
            "pub emissions: Vec<ProgramPointEmissionBinding>,",
            "pub emissions: Vec<ProgramPointEmissionBinding>, pub extra: u32,",
        ),
        (
            "wrong aggregate field",
            "pub presentations: Vec<ProgramPointPresentationBinding>,",
            "pub presentations: Vec<ProgramPointEmissionBinding>,",
        ),
        (
            "wrong emission column",
            "pub sink_output: u32",
            "pub output: u32",
        ),
        ("wrong column type", "pub root: u32", "pub root: u64"),
        (
            "swapped presentation columns",
            "pub root: u32, pub occurrence: u32,",
            "pub occurrence: u32, pub root: u32,",
        ),
        (
            "extra row column",
            "pub occurrence: u32,",
            "pub occurrence: u32, pub extra: u32,",
        ),
    ] {
        let changed = NATIVE_ATTACHMENT.replace(before, after);
        assert_ne!(
            changed, NATIVE_ATTACHMENT,
            "fixture mutation must apply: {name}"
        );
        assert!(
            fixture_coverage(PORTABLE_ATTACHMENT, &changed).is_err(),
            "{name}"
        );
    }
}

#[test]
fn attachment_coverage_rejects_divergent_portable_shapes() {
    for (name, before, after) in [
        (
            "wrong emission encoding",
            "emission_bindings: &[u32]",
            "emission_bindings: &[u64]",
        ),
        (
            "wrong presentation encoding",
            "presentation_bindings: &[u32]",
            "presentation_bindings: Vec<u32>",
        ),
        (
            "wrong stream argument",
            "stream_id: JsValue,",
            "stream_id: u64,",
        ),
        (
            "extra argument",
            "stream_id: JsValue,",
            "stream_id: JsValue, ignored: u32,",
        ),
        ("wrong host", "host: &js_sys::Function", "host: JsValue"),
    ] {
        let changed = PORTABLE_ATTACHMENT.replace(before, after);
        assert_ne!(
            changed, PORTABLE_ATTACHMENT,
            "fixture mutation must apply: {name}"
        );
        assert!(
            fixture_coverage(&changed, NATIVE_ATTACHMENT).is_err(),
            "{name}"
        );
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
    "#).expect("discovery fixture must be valid Rust");
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
    let additional = syn::parse_file("#[wasm_bindgen] pub fn next_capability() {}")
        .expect("added-operation fixture must parse");
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
