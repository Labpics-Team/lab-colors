//! Воспроизводимый провенанс палитры акцентов: печатает для каждого из 10
//! семейств измеренный якорный hex (Figma «🔵 4.1 Primitives», Accent/*,
//! Light-mode) и выведенный из него якорный Oklab-оттенок.
//!
//! Вход — якорные hex из [`labcolors_core::Accent::anchor_hex`] (SSOT, замер
//! 2026-07-02 через figma-console MCP); Oklab-оттенок считается публичным
//! [`Accent::prototype_hue`] (тем же путём, что рампа акцента), а не второй
//! копией формулы. Числа этого вывода перенесены в столбец «Oklab h°»
//! таблицы `reference/labui-accent-primitives.md`.
//!
//! Запуск: `cargo run -p labcolors-core --example accent_provenance`

use labcolors_core::Accent;

fn main() {
    println!("семейство\tключ\tякорь(Light)\tOklab h°");
    for a in Accent::ALL {
        println!(
            "{a:?}\t{}\t{}\t{:.4}",
            a.key(),
            a.anchor_hex(),
            a.prototype_hue()
        );
    }
}
