//! Docs-presence gate — integration (#29 / PR #212).
//!
//! BUG CLASS this guards: the doc gate (`RUSTDOCFLAGS="-D warnings" cargo doc`)
//! fails on BROKEN docs but not on ABSENT docs. A revert or refactor can
//! silently drop the zero-runtime-deps section from README.md or the CIECAM16
//! source citations from the module headers of `cam16.rs` / `vc.rs`, and CI
//! stays green — exactly the gap flagged in the verification verdict for
//! PR #212 ("doc-lint гейт не падает на ОТСУТСТВИИ доки").
//!
//! This test reads the REAL files from the working tree via
//! `std::fs::read_to_string` and asserts presence of the specific claims that
//! issue #29 required:
//!
//!   1. README.md has the `## Зависимости` section stating zero RUNTIME deps
//!      and the verifiable command `cargo tree -p labcolors-core --edges=no-dev`.
//!   2. `src/spaces/cam16.rs` cites its source IN THE MODULE HEADER (`//!`):
//!      DOI 10.1002/col.22131 and CIE 248:2022. Header lines only — before
//!      PR #212 the citation existed merely in the `adapt` fn doc, which is
//!      precisely the state issue #29 called insufficient, so scoping to `//!`
//!      is what makes revert→RED hold.
//!   3. `src/spaces/vc.rs` module header ties the surround presets to
//!      CIECAM16 Table 1 and the same DOI.
//!
//! How this test bites (mutation proof, verified against `main` pre-#212):
//!   * `git checkout main -- README.md src/spaces/{cam16,vc}.rs` → all three
//!     assertions fail (section absent; `//!` headers lack the citations).
//!   * Move the cam16 citation back into a fn doc comment → assertion 2 fails.

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
fn readme_documents_zero_runtime_deps() {
    let readme = read(workspace_root().join("README.md"));
    assert!(
        readme.contains("## Зависимости"),
        "README.md lost the `## Зависимости` section required by #29"
    );
    assert!(
        readme.contains("ноль рантайм-зависимостей"),
        "README.md no longer states the zero-runtime-deps claim (#29)"
    );
    assert!(
        readme.contains("cargo tree -p labcolors-core --edges=no-dev"),
        "README.md lost the verifiable `cargo tree --edges=no-dev` command \
         backing the zero-runtime-deps claim (#29)"
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
