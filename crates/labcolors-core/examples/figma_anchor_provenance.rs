//! Воспроизводимый протокол замера dJ'-якорей декоративных ролей из живых
//! значений Figma (файл 🧪Lab UI (v.1), коллекция «🔵 4.2 Semantic»).
//!
//! Числа на входе — сырые значения переменных Figma, снятые через
//! figma-console MCP (см. reference/labui-figma-structure.md — протокол,
//! дата замера, цепочки алиасов). Каждая роль — tint #787880 с альфой,
//! композитится над Backgrounds/Neutral/Primary своей темы, квантуется до
//! 8-битного hex (то, что реально показывает дисплей), и dJ' считается тем же
//! математическим путём, что `solve_dj` в движке:
//!
//! dJ' = |J'(fg) − J'(bg)|, где J' — CAM16-UCS lightness
//! (CIECAM16 forward → J' = 1.7·J/(1+0.007·J), Li et al. 2017), через
//! публичный `LcsColor::from_hex_with_vc` — тот же `cam16::forward` +
//! `cam16::ucs_j`, что внутри `solve::jp_of_linear`.
//!
//! Запуск: `cargo run -p labcolors-core --example figma_anchor_provenance`
//!
//! Композитинг проверяется в двух гипотезах (gamma-пространство — как рендерит
//! Figma; linear — физика света), а тёмная тема — под двумя VC (dim_surround —
//! как резолвит движок тёмную тему; srgb — как выглядел бы замер без
//! surround-разделения). Скрипт печатает все ветки; вывод о том, какая
//! комбинация воспроизводит константы semantic.rs, — в reference-документе.

use labcolors_core::{LcsColor, ViewingConditions};

/// Общий tint всех fill/border-ролей нейтральной лестницы: Neutral/Derivable/6
/// (#787880), живое значение из Figma (r,g,b = 120,120,128 / 255).
const TINT: [f64; 3] = [120.0 / 255.0, 120.0 / 255.0, 128.0 / 255.0];

/// (имя роли, альфа light, альфа dark) — живые альфы из Figma
/// (Neutral/Derivable/6/6@NN — NN и есть альфа в процентах).
const ROLES: [(&str, f64, f64); 6] = [
    ("Fills/Neutral/Primary   ", 0.20, 0.36),
    ("Fills/Neutral/Secondary ", 0.16, 0.32),
    ("Fills/Neutral/Tertiary  ", 0.12, 0.24),
    ("Fills/Neutral/Quaternary", 0.08, 0.16),
    ("Border/Neutral/Base     ", 0.16, 0.20),
    ("Border/Neutral/Soft     ", 0.08, 0.12),
];

/// Backgrounds/Neutral/Primary: light #FFFFFF, dark #101012 (живые значения).
const BG_LIGHT: [f64; 3] = [1.0, 1.0, 1.0];
const BG_DARK: [f64; 3] = [16.0 / 255.0, 16.0 / 255.0, 18.0 / 255.0];

/// Константы semantic.rs (light, dark) — для сравнения рядом, НЕ для подгонки.
const ANCHORS: [(f64, f64); 6] = [
    (7.93, 17.67),
    (6.41, 15.78),
    (4.63, 12.01),
    (3.15, 8.22),
    (6.41, 10.12),
    (3.15, 5.83),
];

fn srgb_gamma_decode(u: f64) -> f64 {
    if u <= 0.04045 {
        u / 12.92
    } else {
        ((u + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_gamma_encode(v: f64) -> f64 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Композит straight-alpha в gamma-пространстве (как композитит Figma на
/// канвасе без color management): c = a·fg + (1−a)·bg по кодированным каналам.
fn composite_gamma(fg: [f64; 3], alpha: f64, bg: [f64; 3]) -> [f64; 3] {
    [
        alpha * fg[0] + (1.0 - alpha) * bg[0],
        alpha * fg[1] + (1.0 - alpha) * bg[1],
        alpha * fg[2] + (1.0 - alpha) * bg[2],
    ]
}

/// Композит в линейном свете: каналы декодируются из gamma, смешиваются,
/// кодируются обратно (физически корректное смешение света).
fn composite_linear(fg: [f64; 3], alpha: f64, bg: [f64; 3]) -> [f64; 3] {
    let mix = |f: f64, b: f64| {
        let lin = alpha * srgb_gamma_decode(f) + (1.0 - alpha) * srgb_gamma_decode(b);
        srgb_gamma_encode(lin)
    };
    [mix(fg[0], bg[0]), mix(fg[1], bg[1]), mix(fg[2], bg[2])]
}

/// Квантование до дисплейного 8-битного hex — то, что реально видно на экране
/// и на чём движок честно меряет достигнутый dJ' (`solve_dj`, шаг 4).
fn hex_of(rgb: [f64; 3]) -> String {
    let q = |v: f64| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02X}{:02X}{:02X}", q(rgb[0]), q(rgb[1]), q(rgb[2]))
}

fn dj(fg_hex: &str, bg_hex: &str, vc: &ViewingConditions) -> f64 {
    let fg = LcsColor::from_hex_with_vc(fg_hex, vc).expect("valid fg hex");
    let bg = LcsColor::from_hex_with_vc(bg_hex, vc).expect("valid bg hex");
    (fg.jp - bg.jp).abs()
}

fn main() {
    let vc_light = ViewingConditions::srgb();
    let vc_dim = ViewingConditions::dim_surround();
    let bg_light_hex = hex_of(BG_LIGHT);
    let bg_dark_hex = hex_of(BG_DARK);

    println!("bg light = {bg_light_hex}, bg dark = {bg_dark_hex}");
    println!();
    println!(
        "{:<26} {:>7} {:>8} {:>8} | {:>7} {:>8} {:>8} {:>8} {:>8}",
        "role",
        "hexL",
        "gam:sRGB",
        "lin:sRGB",
        "hexD",
        "gam:dim",
        "lin:dim",
        "gam:sRGB",
        "lin:sRGB"
    );
    for (i, (name, a_light, a_dark)) in ROLES.iter().enumerate() {
        let (anchor_l, anchor_d) = ANCHORS[i];
        // light
        let lg = hex_of(composite_gamma(TINT, *a_light, BG_LIGHT));
        let ll = hex_of(composite_linear(TINT, *a_light, BG_LIGHT));
        let dj_lg = dj(&lg, &bg_light_hex, &vc_light);
        let dj_ll = dj(&ll, &bg_light_hex, &vc_light);
        // dark
        let dg = hex_of(composite_gamma(TINT, *a_dark, BG_DARK));
        let dl = hex_of(composite_linear(TINT, *a_dark, BG_DARK));
        let dj_dg_dim = dj(&dg, &bg_dark_hex, &vc_dim);
        let dj_dl_dim = dj(&dl, &bg_dark_hex, &vc_dim);
        let dj_dg_srgb = dj(&dg, &bg_dark_hex, &vc_light);
        let dj_dl_srgb = dj(&dl, &bg_dark_hex, &vc_light);
        println!(
            "{name} {lg} {dj_lg:9.4} {dj_ll:9.4} | {dg} {dj_dg_dim:9.4} {dj_dl_dim:9.4} {dj_dg_srgb:9.4} {dj_dl_srgb:9.4}   anchors: L={anchor_l:5.2} D={anchor_d:5.2}"
        );
    }
    println!();
    println!("отношения dark/light (gamma-композит, dark под dim_surround):");
    for (i, (name, a_light, a_dark)) in ROLES.iter().enumerate() {
        let (anchor_l, anchor_d) = ANCHORS[i];
        let lg = hex_of(composite_gamma(TINT, *a_light, BG_LIGHT));
        let dg = hex_of(composite_gamma(TINT, *a_dark, BG_DARK));
        let l = dj(&lg, &bg_light_hex, &vc_light);
        let d = dj(&dg, &bg_dark_hex, &vc_dim);
        println!(
            "{name} computed D/L = {:.3}   anchor D/L = {:.3}",
            d / l,
            anchor_d / anchor_l
        );
    }
}
