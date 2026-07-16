//! GATE: деривация текстовых долей labui из Ys-переноса Figma-якорей.
//!
//! Доли лейблов (`LABEL_*_FRACTION` в `semantic.rs`, литералы пресета labui в
//! `config/preset.rs` и wasm-фикстуре `labui.config.json`) — НЕ магические
//! числа: каждая обязана выводиться из замера. Генезис-якоря Даниила сняты с
//! Figma «Labels/Neutral» в легаси-домене Y_hk (102.6/66.5/48.9/29.3 на белом
//! при максимуме ~106). При миграции мерила читаемости на Ys (ADR-0003;
//! derivation locks ниже) якоря перенесены с инвариантом «цвет, а не Lc-число»:
//!
//! * primary/secondary/quaternary — Ys-замер ПРИНЯТЫХ владельцем hex'ов
//!   лестницы `#141414`/`#767676`/`#C2C2C2` (байт-идентичность эмиссии —
//!   приёмочный критерий ревью; цель солвера ложится ровно на байт);
//! * tertiary — эмиссия защищена полом WCAG 3:1 (`#949494`), поэтому якорь
//!   восстановлен побайтовой инверсией генезис-числа 48.9 в Y_hk → `#9C9C9C`.
//!
//! Если этот гейт упал — либо сдвинулось само мерило (Ys-конвейер), либо
//! кто-то тронул доли руками. И то и другое — не «подкрутить тест», а
//! перевывести доли по протоколу выше и синхронизировать все три носителя
//! (semantic.rs, preset.rs, labui.config.json) + README/whitepaper.

use labcolors_core::lpc::{lpc_readability_ys, lpc_with_vc};
use labcolors_core::{ViewingConditions, srgb_encoded_from_hex};

/// (роль, канонический якорь-hex, ожидаемая доля).
const EXPECTED: [(&str, &str, f64); 4] = [
    ("primary", "#141414", 0.97335917),
    ("secondary", "#767676", 0.64359014),
    ("tertiary", "#9C9C9C", 0.47572199),
    ("quaternary", "#C2C2C2", 0.29335999),
];

/// Генезис-якорь tertiary в Y_hk, единственный не представленный принятым
/// hex'ом эмиссии (пол 3:1 поднимает эмиссию до `#949494`).
const TERTIARY_GENESIS_YHK: f64 = 48.9;

fn ys_on_white(hex: &str) -> f64 {
    let f = srgb_encoded_from_hex(hex).unwrap();
    let w = srgb_encoded_from_hex("#FFFFFF").unwrap();
    lpc_readability_ys(f, w)
}

#[test]
fn fractions_derive_from_ys_anchor_transfer() {
    let max = {
        let b = srgb_encoded_from_hex("#000000").unwrap();
        let w = srgb_encoded_from_hex("#FFFFFF").unwrap();
        lpc_readability_ys(b, w)
    };
    // Знаменатель — Ys-максимум на белом (чёрный текст). Калибровка долей
    // выполнена при 106.0407; дрейф знаменателя = дрейф мерила.
    assert!(
        (max - 106.0407).abs() < 5e-5,
        "Ys-максимум на белом сдвинулся: {max} (калибровка долей была при 106.0407)"
    );

    for (role, hex, expected) in EXPECTED {
        let frac = ys_on_white(hex) / max;
        assert!(
            (frac - expected).abs() < 5e-9,
            "{role}: доля {frac:.8} != запечатанной {expected:.8} (якорь {hex})"
        );
    }

    // Иерархия долей строгая — санити переноса.
    assert!(EXPECTED.windows(2).all(|w| w[0].2 > w[1].2));
}

#[test]
fn tertiary_anchor_is_byte_inversion_of_genesis_yhk() {
    // Инверсия генезис-якоря 48.9 (Y_hk, на белом) по байтовой сетке серых
    // обязана давать ровно #9C9C9C — иначе легаси-мерило Y_hk дрейфнуло и
    // перенос требует пере-вывода.
    let vc = ViewingConditions::srgb();
    let mut best: (u8, f64) = (0, f64::MAX);
    for g in 0..=255_u8 {
        let hex = format!("#{g:02X}{g:02X}{g:02X}");
        let d = (lpc_with_vc(&hex, "#FFFFFF", &vc) - TERTIARY_GENESIS_YHK).abs();
        if d < best.1 {
            best = (g, d);
        }
    }
    assert_eq!(
        best.0, 0x9C,
        "инверсия генезис-якоря {TERTIARY_GENESIS_YHK} дала #{0:02X}{0:02X}{0:02X}, не #9C9C9C",
        best.0
    );
}
