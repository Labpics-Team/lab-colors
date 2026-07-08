//! Бинарь генерации UniFFI-биндингов. `uniffi_bindgen_main` доступен только под
//! фичей `uniffi/cli` (включается passthrough-фичей крейта `cli`), поэтому
//! реальный CLI собирается лишь при её включении:
//!
//! ```text
//! cargo run -p labcolors-ffi --features cli --bin uniffi-bindgen -- \
//!     generate --library target/debug/liblabcolors.dylib \
//!     --language swift --out-dir <dir>
//! ```
//!
//! Без фичи `cli` bin компилируется как заглушка — так воркспейс-гейты CI
//! (build/clippy/test/doc `--workspace`) собирают крейт, не притягивая clap.
//!
//! Version-lock: CLI компилируется из ТОЙ ЖЕ версии `uniffi`, что и биндинг —
//! рассинхрон CLI ⇄ библиотеки невозможен по построению.

#[cfg(feature = "cli")]
fn main() {
    uniffi::uniffi_bindgen_main()
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!(
        "uniffi-bindgen собран без фичи `cli`. Пересобери с \
         `--features cli`, чтобы генерировать биндинги."
    );
    std::process::exit(2);
}
