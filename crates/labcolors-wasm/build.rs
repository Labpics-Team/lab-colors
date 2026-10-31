use std::{env, fs, io, path::PathBuf};

fn main() -> io::Result<()> {
    // Cargo also tracks the build dependency that owns this release projection.
    println!("cargo:rerun-if-changed=build.rs");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    let manifest = labcolors_conformance::generate_capability_manifest_v2();
    fs::write(
        output.join("numerical-capabilities.json"),
        serde_json::to_vec(&manifest)?,
    )
}
