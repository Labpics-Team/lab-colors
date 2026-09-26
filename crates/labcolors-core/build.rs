#[path = "build_support/source_descriptor.rs"]
mod source_descriptor;

fn main() {
    // Kani включает этот cfg только в проверочном компиляторе.
    println!("cargo:rustc-check-cfg=cfg(kani)");
    if source_descriptor::generate().is_err() {
        // Не включать source paths, Git diagnostics или прежний valid output в fallback.
        eprintln!("certificate source descriptor output unavailable");
        std::process::exit(1);
    }
}
