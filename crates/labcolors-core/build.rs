#[path = "build_support/source_descriptor.rs"]
mod source_descriptor;

fn main() {
    if source_descriptor::generate().is_err() {
        // Не включать source paths, Git diagnostics или прежний valid output в fallback.
        eprintln!("certificate source descriptor output unavailable");
        std::process::exit(1);
    }
}
