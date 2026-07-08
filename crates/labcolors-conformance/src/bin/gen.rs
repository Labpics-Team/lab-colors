//! Генератор conformance-векторов: пишет закоммиченные файлы пака из канона
//! ядра. Запуск:
//!
//! ```text
//! cargo run -p labcolors-conformance --bin gen
//! ```
//!
//! Пишет `conformance/vectors/*.json` + `manifest.json` в корне репозитория
//! (путь резолвится от `CARGO_MANIFEST_DIR`, не от cwd). Идемпотентен: тот же
//! канон → байт-идентичные файлы. Раннер-референс (`tests/reference_runner.rs`)
//! ловит расхождение закоммиченных файлов с этим выходом (гейт дрейфа).

use std::path::PathBuf;

use labcolors_conformance::{MANIFEST_FILE, Pack, to_canonical_json};

fn vectors_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR = <repo>/crates/labcolors-conformance → поднимаемся на
    // два уровня к корню репозитория, затем conformance/vectors.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("conformance")
        .join("vectors")
}

fn main() -> std::io::Result<()> {
    let dir = vectors_dir();
    std::fs::create_dir_all(&dir)?;

    let pack = Pack::generate();

    // Семейства.
    for (name, bytes) in pack.families() {
        let path = dir.join(name);
        std::fs::write(&path, bytes.as_bytes())?;
        println!("написано {}", path.display());
    }

    // Манифест — последним, он несёт дайджест над семействами.
    let manifest_bytes = to_canonical_json(&pack.manifest());
    let manifest_path = dir.join(MANIFEST_FILE);
    std::fs::write(&manifest_path, manifest_bytes.as_bytes())?;
    println!("написано {}", manifest_path.display());

    let c = pack.counts();
    println!(
        "готово: {} векторов (contrasts={}, ladders={}, alpha={}, solve={}, muddiness={}), дайджест={}",
        c.total,
        c.contrasts,
        c.ladders,
        c.alpha,
        c.solve,
        c.muddiness,
        pack.digest()
    );
    Ok(())
}
