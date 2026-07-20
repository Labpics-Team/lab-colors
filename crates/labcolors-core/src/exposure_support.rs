//! Shared test-support for the constants EXPOSURE analysis (wave
//! `science/constants-objectivization`).
//!
//! # Что измеряет экспозиция
//!
//! Для каждой чувствительной константы класса (c)/(d) реестра
//! (`docs/empirical-inventory.md`) вопрос владельца — «вдруг оно красит
//! некорректно СЕЙЧАС?» — превращается в измеримую величину: прогнать константу
//! по её задекларированному (или провенанс-мотивированному) интервалу и
//! посчитать, какая доля входа МЕНЯЕТ классификационное решение (сторона лейбла,
//! ахроматическая подстановка, категория оттенка, класс контраста). Ноль
//! изменений поведения: продакшн-константы НЕ трогаются, тест лишь пере-вычисляет
//! РЕШАЮЩИЙ предикат при других значениях порога.
//!
//! Два плеча входа:
//! * `rgb_cube` — репрезентативная sRGB-гамма (метод сетки задокументирован ниже);
//! * `LABUI_ANCHORS` — 49 РЕАЛЬНЫХ якорей labui (замороженный паспорт
//!   `crates/labcolors-wasm/tests/data/labui.config.json`).

use crate::spaces::srgb::srgb_encoded_from_hex;

/// 49 реальных якорей Lab UI (бренд, семейства и нейтрали) из паспорта.
/// Дедуп-набор всех hex-литералов паспорта — «реальные входы» экспозиции.
pub(crate) const LABUI_ANCHORS: &[&str] = &[
    "#0040DD", "#0050CF", "#0071A4", "#007AFF", "#00C7BE", "#0C817B", "#101012", "#248A3D",
    "#30D158", "#30DB5B", "#34C759", "#3634A3", "#3C3C43", "#3E87FF", "#409CFF", "#4A8FFF",
    "#5696FF", "#5856D6", "#5AC8FA", "#5E5CE6", "#63E6E2", "#64D2FF", "#6CEBE7", "#70D7FF",
    "#787880", "#7D7AFF", "#8944AB", "#95C0FF", "#AF52DE", "#B0B0B9", "#B25000", "#BF5AF2",
    "#C93400", "#D30F45", "#D70015", "#DA8FFF", "#F6F8FA", "#FF2D55", "#FF3A3A", "#FF3B30",
    "#FF6161", "#FF6482", "#FF9008", "#FFA100", "#FFA940", "#FFD000", "#FFD426", "#FFD60A",
    "#FFFFFF",
];

/// Метод сетки: sRGB 8-битный куб, сэмплированный каждое 8-е кодовое значение по
/// каналу → 32³ = 32768 представимых (в гамме по построению) цветов.
pub(crate) fn rgb_cube(mut f: impl FnMut([u8; 3])) {
    let vals: Vec<u8> = (0u16..256).step_by(8).map(|v| v as u8).collect();
    for &r in &vals {
        for &g in &vals {
            for &b in &vals {
                f([r, g, b]);
            }
        }
    }
}

/// Размер сетки `rgb_cube` (для процентов).
pub(crate) fn grid_size() -> usize {
    let n = (0u16..256).step_by(8).count();
    n * n * n
}

/// 8-битные каналы hex-якоря.
pub(crate) fn enc_of(hex: &str) -> [u8; 3] {
    let s = srgb_encoded_from_hex(hex).expect("passport hex valid");
    [
        (s[0] * 255.0).round() as u8,
        (s[1] * 255.0).round() as u8,
        (s[2] * 255.0).round() as u8,
    ]
}

/// Экспозиция ПОРОГОВОЙ константы: доля куба (%) с per-цвет величиной `q` в
/// полуоткрытой полосе флипа `[lo, hi)` (решение `q < θ` меняется, пока θ ходит по
/// полосе), плюс якоря labui, попавшие в полосу. Флип-зона ≈ 0% ⇒ точное значение
/// нематериально; большая ⇒ выше приоритет явной декларации терминала в реестре
/// (интервал + экспозиция, docs/empirical-inventory.md).
pub(crate) fn band_exposure(q: impl Fn([u8; 3]) -> f64, lo: f64, hi: f64) -> (f64, Vec<String>) {
    let mut hits = 0usize;
    rgb_cube(|c| {
        let v = q(c);
        if v >= lo && v < hi {
            hits += 1;
        }
    });
    let pct = 100.0 * hits as f64 / grid_size() as f64;
    let mut labui = Vec::new();
    for &h in LABUI_ANCHORS {
        let v = q(enc_of(h));
        if v >= lo && v < hi {
            labui.push(h.to_string());
        }
    }
    (pct, labui)
}

/// M' (CAM16-UCS colorfulness) кодированного 8-битного цвета под указанными VC.
pub(crate) fn mp_srgb(rgb: [u8; 3], dim: bool) -> f64 {
    use crate::spaces::vc::ViewingConditions;
    let lin = [
        srgb_gamma_inv(rgb[0] as f64 / 255.0),
        srgb_gamma_inv(rgb[1] as f64 / 255.0),
        srgb_gamma_inv(rgb[2] as f64 / 255.0),
    ];
    let vc = if dim {
        ViewingConditions::dim_surround()
    } else {
        ViewingConditions::srgb()
    };
    crate::lcs::LcsColor::mp_of_linear_srgb(lin, &vc)
}
