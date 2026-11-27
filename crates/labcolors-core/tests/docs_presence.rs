//! Проверяем доступность источников моделей и команды проверки зависимостей.
//! Rustdoc обнаруживает повреждённые ссылки, но не исчезновение этих пояснений.
//! Фактические зависимости проверяет Cargo metadata в CI; этот тест не
//! закрепляет прежний набор зависимостей через текст README.

use std::path::PathBuf;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    // CARGO_MANIFEST_DIR = crates/labcolors-core/
    // workspace root    = crates/labcolors-core/../../  = lab-colors/
    crate_root().join("..").join("..")
}

/// Read a file, panicking with a clear message if it is absent.
fn read(path: PathBuf) -> String {
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "docs-presence gate: cannot read {} ({e}); the documented \
             invariant of #29 is unverifiable without it",
            path.display()
        )
    })
}

/// Only the `//!` module-header lines of a source file.
fn module_header(source: &str) -> String {
    source
        .lines()
        .filter(|l| l.trim_start().starts_with("//!"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn dependency_inspection_is_reachable_from_readme() {
    const GUIDE: &str = "docs/how-to/develop.md";
    let root = workspace_root();
    let readme = read(root.join("README.md"));
    assert!(
        readme.contains(&format!("({GUIDE})")),
        "README.md must link to the development guide at {GUIDE}"
    );
    assert!(
        read(root.join(GUIDE)).contains("cargo tree -p labcolors-core --edges=no-dev"),
        "{GUIDE} must document how to inspect the actual runtime dependencies"
    );
}

#[test]
fn cam16_module_header_cites_sources() {
    let header = module_header(&read(
        crate_root().join("src").join("spaces").join("cam16.rs"),
    ));
    assert!(
        header.contains("10.1002/col.22131"),
        "cam16.rs module header (`//!`) lost the Li et al. 2017 DOI citation; \
         a citation buried in a fn doc does not satisfy #29"
    );
    assert!(
        header.contains("CIE 248:2022"),
        "cam16.rs module header (`//!`) lost the CIE 248:2022 formalisation \
         reference (#29)"
    );
}

#[test]
fn vc_module_header_cites_sources() {
    let header = module_header(&read(crate_root().join("src").join("spaces").join("vc.rs")));
    assert!(
        header.contains("CIECAM16 Table 1"),
        "vc.rs module header (`//!`) no longer attributes the surround \
         triplets to CIECAM16 Table 1 (#29)"
    );
    assert!(
        header.contains("10.1002/col.22131"),
        "vc.rs module header (`//!`) lost the Li et al. 2017 DOI citation (#29)"
    );
}
