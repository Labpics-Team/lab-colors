//! Эмиттер паспорта labui: канонический конфиг ядра → JSON границы + отпечаток.
//!
//! Единственный воспроизводимый путь регенерации `labui.config.json` поезда
//! (ручная сборка JSON дрейфует от ядра молча):
//!
//! ```text
//! cargo run -p labcolors-wasm --example emit_passport > labui.config.json
//! ```
//!
//! Отпечаток (пин `PASSPORT_FINGERPRINT` на стороне labui) печатается в stderr,
//! чтобы не смешиваться с JSON на stdout.

use labcolors_wasm::config_dto::{ConfigDto, fingerprint};

fn main() {
    let cfg = labcolors_core::config::labui_reference();
    let dto = ConfigDto::try_from(&cfg).expect("канонический конфиг сериализуем");
    println!(
        "{}",
        serde_json::to_string_pretty(&dto).expect("DTO без не-сериализуемых типов")
    );
    eprintln!("fingerprint: {:016x}", fingerprint(&dto));
}
