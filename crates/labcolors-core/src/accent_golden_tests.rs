//! Контракт 7 — эталонный снимок `AccentCurve`.
//!
//! Защищает от тихого дрейфа значений. Остальные тесты проверяют свойства:
//! попадание в гамму, монотонность J′, неотрицательную насыщенность и достижение
//! контраста. Они не фиксируют фактически эмитированные цвета. Изменение
//! коэффициента кривой, огибающей хромы, поиска оттенка или шкалы CAM16-UCS
//! может сдвинуть каждый образец на несколько байтов, не нарушив этих свойств.
//! Так выглядел дрейф Bracket-path LUT (#50/#53).
//! Поэтому здесь побайтно зафиксированы 13 точек одной репрезентативной кривой.
//!
//! Провал не обязательно означает ошибку: осознанная перекалибровка вправе
//! изменить снимок. Перед обновлением константы нужно проверить разницу и
//! подтвердить новую лестницу; невыбранный дрейф считается регрессией.
//!
//! Снимок получен 2026-06-12 через собственный `sample_hex(13)` кривой и её
//! унаследованные условия просмотра sRGB.

use crate::curve::ColorCurve;
use crate::neutral::NeutralCurve;
use crate::scale::AccentCurve;

/// Системная нейтральная лестница, на которой строится акцентная кривая.
fn neutral() -> NeutralCurve {
    NeutralCurve::new("#FFFFFF", "#787880", "#101012")
        .expect("the canonical neutral anchors are valid")
}

/// Зафиксированный результат
/// `AccentCurve::new("#007AFF", neutral).sample_hex(13)`.
/// Перекалибровка требует осознанного и проверенного изменения этой константы.
const ACCENT_007AFF_GOLDEN: [&str; 13] = [
    "#FFFFFF", "#F4F8FF", "#DAE9FF", "#B6D4FF", "#88B9FF", "#4F98FF", "#0A6CFF", "#0060FC",
    "#0C41FF", "#0500F9", "#0300C4", "#010089", "#000043",
];

#[test]
fn accent_curve_007af_sample_hex_13_matches_golden() {
    let neutral = neutral();
    let accent = AccentCurve::new("#007AFF", &neutral).expect("#007AFF is a valid accent seed");
    let got = accent.sample_hex(13);
    assert_eq!(
        got, ACCENT_007AFF_GOLDEN,
        "AccentCurve('#007AFF') ladder drifted from its golden snapshot. If this was a \
         deliberate recalibration, update ACCENT_007AFF_GOLDEN consciously; otherwise it is \
         a silent value regression."
    );
}

#[test]
fn golden_endpoints_anchor_to_white_and_near_black() {
    // Структурный страж не пропускает случайную замену всей константы, например
    // белым: лестница начинается с чистого белого и заканчивается почти чёрным.
    assert_eq!(ACCENT_007AFF_GOLDEN[0], "#FFFFFF");
    let luma = |hex: &str| -> u32 {
        let v = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap();
        ((v >> 16) & 0xFF) + ((v >> 8) & 0xFF) + (v & 0xFF)
    };
    assert!(
        luma(ACCENT_007AFF_GOLDEN[0]) > luma(ACCENT_007AFF_GOLDEN[12]),
        "golden ladder must darken from first to last stop"
    );
}
