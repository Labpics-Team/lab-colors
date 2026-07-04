//! Пара «заливка × лейбл» — выводимая поверхность с перцептивной полярностью.
//!
//! # Класс, который закрывает модуль
//!
//! На статичном фоне полярность текста обязана следовать достижимости
//! WCAG-пола ([`crate::semantic`], `choose_polarity`) — иначе система эмитит
//! нечитаемое по закону. Но у ВЫВОДИМОЙ поверхности (заливка бейджа, кнопки)
//! есть степень свободы, которой нет у фона страницы: заливку можно двинуть.
//! Зона конфликта — фоны с Y ∈ (0.183, ~0.30): перцептивно они ТЁМНЫЕ (белый
//! текст читается лучше — полярностная асимметрия чтения, класс исследований,
//! на которых построен APCA), но белый физически не достигает пола 4.5.
//! Фирменные заливки (синий #007AFF Y≈0.21, красный Y≈0.25) живут ровно там:
//! буква WCAG требовала чернил, глаз и Figma — белого.
//!
//! Закон пары: сторона лейбла выбирается ПЕРЦЕПТИВНЫМ кроссовером по якорю,
//! а заливка минимально двигается по светлоте (оттенок и хрома идентичности
//! сохраняются в Oklab), пока выбранная сторона не начнёт выигрывать штатный
//! `choose_polarity`. Дальше лейбл решается ОБЫЧНЫМ nested resolve на
//! выведенной заливке — пара не изобретает второй текстовый закон, она
//! готовит поверхность, на которой существующий закон даёт перцептивно
//! правильную сторону с легальным полом.
//!
//! # Кроссовер
//!
//! Сторона пары — свойство ИДЕНТИЧНОСТИ СЕМЬИ: решается ОДИН РАЗ по
//! каноническому светлому якорю (`PAIR_CROSSOVER_Y`) и не флипается между
//! темами/IC — иначе «Brand» носил бы белый лейбл в light и чернильный в
//! dark (тёмные якоря labui осветлены и перелезают порог: info dark
//! #5696FF Y=0.31). Пер-режимная заливка двигается ПОД выбранную сторону:
//! светлая сторона — утемнение до строгой победы белого; чернильная —
//! осветление до строгой победы чёрного (IC-якоря warning/success
//! проваливаются под границу: #C93400 Y=0.149 — без осветления штатная
//! полярность отдала бы белый и сторона флипнулась бы в IC).
//!
//! Белая сторона требует строгой победы белого в `choose_polarity`:
//! `(Y + 0.05)² < 1.05 · 0.05`, т.е. Y < 0.17913 (белый ≥ 4.58:1) — граница
//! выведена из самой формулы WCAG, не подобрана. Сдвиг для фирменных якорей
//! мал: #007AFF (0.211 → 0.179) — едва заметное утемнение при том же оттенке.

use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
use crate::spaces::srgb::srgb_gamma_inv;

/// Y-порог кроссовера стороны пары «заливка × лейбл» — решается ОДИН РАЗ по
/// каноническому светлому якорю семьи.
///
/// DESIGN-CHOICE, НЕ «перцептивный консенсус» и не измеренный порог. Значение
/// 0.30 выбрано ВНУТРИ перцептивно-мотивированного интервала (0.246, 0.423),
/// зажатого 10 Figma-якорями labui (красный #FF3B30 Y=0.246 → белая сторона;
/// зелёный #34C759 Y=0.423 → чернильная). Полярностная асимметрия чтения —
/// качественное основание (класс исследований, на которых построен APCA), НЕ
/// источник конкретного числа. Правило «нижняя треть» интервала даёт 0.305;
/// 0.30 — округление внутри интервала. Провенанс = 10 якорей ОДНОЙ палитры,
/// 0 наблюдателей; контролируемый эксперимент (N≥15 наблюдателей) — ОТКРЫТАЯ
/// задача. Тест `crossover_matches_palette_consensus` фиксирует лишь разделение
/// этих 10 якорей на заявленные стороны — калибровка к якорям, не независимая
/// валидация. Значение не меняется.
// SSOT-TRACKED — Y-порог кроссовера стороны пары (design-choice), см. docs/empirical-inventory.md.
pub(crate) const PAIR_CROSSOVER_Y: f64 = 0.30;

/// Строгая граница победы белой стороны в `choose_polarity`:
/// `(Y + 0.05)² < 1.05 · 0.05` ⇒ Y < 0.17913. Выведена из формулы WCAG (не
/// настройка); округлена ВНИЗ (консервативно): ниже неё белый и по ратио, и
/// по tie-break выигрывает штатную полярность.
pub(crate) const WHITE_WINS_Y: f64 = 0.179;

/// Строгая граница победы чернильной стороны — та же формула с другой
/// стороны: `(Y + 0.05)² > 1.05 · 0.05` ⇒ Y > 0.17913; округлена ВВЕРХ.
pub(crate) const BLACK_WINS_Y: f64 = 0.1795;

/// Итераций бисекции минимального сдвига: 48 делений пополам ≫ шага 8-битной
/// решётки, на которой квантуется каждый кандидат — сходимость до кванта.
const BISECTION_STEPS: usize = 48;

// Коэффициенты относительной яркости ITU-R BT.709 / WCAG 2.x (Rec.709 luma).
// Стандарт, не тюнинг: исключены из POLICY-инвентаря by-construction
// (`NUMERIC_METHOD_ALLOWLIST`, INV-3). Извлечены из inline-литералов, чтобы
// pair.rs прошёл GATE-5 без незадекларированных голых чисел (значения целы).
const WCAG_LUMA_R: f64 = 0.2126;
const WCAG_LUMA_G: f64 = 0.7152;
const WCAG_LUMA_B: f64 = 0.0722;

/// WCAG-люминанс кодированного (byte/255) sRGB.
fn wcag_y_encoded(rgb: [f64; 3]) -> f64 {
    let lin = [
        srgb_gamma_inv(rgb[0]),
        srgb_gamma_inv(rgb[1]),
        srgb_gamma_inv(rgb[2]),
    ];
    WCAG_LUMA_R * lin[0] + WCAG_LUMA_G * lin[1] + WCAG_LUMA_B * lin[2]
}

/// Сторона лейбла пары по перцептивному кроссоверу якоря.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairSide {
    /// Якорь перцептивно тёмный — лейбл светлый, заливка двигается до
    /// строгой победы белого в штатной полярности.
    Light,
    /// Якорь перцептивно светлый — лейбл чернильный, заливка не двигается
    /// (тёмная сторона уже выигрывает штатную полярность).
    Ink,
}

/// Выбрать сторону пары для кодированного якоря.
pub fn pair_side(anchor_encoded: [f64; 3]) -> PairSide {
    if wcag_y_encoded(anchor_encoded) < PAIR_CROSSOVER_Y {
        PairSide::Light
    } else {
        PairSide::Ink
    }
}

/// Заливка пары: пер-режимный якорь, минимально сдвинутый по L Oklab до
/// строгой победы СТОРОНЫ СЕМЬИ (a, b — оттенок/хрома идентичности — не
/// трогаются в Oklab-координатах; у края куба каналы честно клампятся).
///
/// Сторона приходит от канонического светлого якоря семьи ([`pair_side`]) и
/// одна на все режимы; движение пер-режимное: светлая сторона — утемнение до
/// `Y < WHITE_WINS_Y`, чернильная — осветление до `Y > BLACK_WINS_Y`
/// (IC-якоря проваливаются под границу). Якорь, уже дающий победу, не
/// двигается вовсе.
pub fn pair_fill(anchor_encoded: [f64; 3], side: PairSide) -> [f64; 3] {
    let y = wcag_y_encoded(anchor_encoded);
    let (needs_move, target_dark) = match side {
        PairSide::Light => (y >= WHITE_WINS_Y, true),
        PairSide::Ink => (y <= BLACK_WINS_Y, false),
    };
    if !needs_move {
        return anchor_encoded;
    }
    let lin = [
        srgb_gamma_inv(anchor_encoded[0]),
        srgb_gamma_inv(anchor_encoded[1]),
        srgb_gamma_inv(anchor_encoded[2]),
    ];
    let lab = srgb_linear_to_oklab(lin);
    let wins = |l: f64| {
        let cand = encode_clamped(oklab_to_srgb_linear([l, lab[1], lab[2]]));
        if target_dark {
            wcag_y_encoded(cand) < WHITE_WINS_Y
        } else {
            wcag_y_encoded(cand) > BLACK_WINS_Y
        }
    };
    // Бисекция минимального сдвига: инвариант — lo выигрывает, hi нет;
    // сходимся к ближайшей к якорю светлоте с победой стороны.
    let (mut lo, mut hi) = if target_dark {
        (0.0_f64, lab[0])
    } else {
        (1.0_f64, lab[0])
    };
    for _ in 0..BISECTION_STEPS {
        let mid = 0.5 * (lo + hi);
        if wins(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    encode_clamped(oklab_to_srgb_linear([lo, lab[1], lab[2]]))
}

/// Линейный sRGB → кодированный, КВАНТОВАННЫЙ в 8-битную решётку с клампом
/// в куб. Квантование обязательно: `choose_polarity` меряет display-байты,
/// и бисекция обязана сходиться на той же решётке — неквантованный кандидат
/// у границы после округления в hex выталкивался на грань tie-break
/// (красный #FF3B30 давал #E81A17 с Y ровно на границе → чернила).
fn encode_clamped(lin: [f64; 3]) -> [f64; 3] {
    let g = |v: f64| (crate::spaces::srgb::srgb_gamma(v).clamp(0.0, 1.0) * 255.0).round() / 255.0;
    [g(lin[0]), g(lin[1]), g(lin[2])]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};

    fn enc(hex: &str) -> [f64; 3] {
        srgb_encoded_from_hex(hex).expect("тестовые hex-литералы валидны")
    }

    /// Калибровка кроссовера консенсус-сетом якорей labui: перцептивно тёмные
    /// семьи получают белую сторону, светлые — чернильную. Красный и зелёный —
    /// зажимы интервала (0.246 < порог < 0.423).
    #[test]
    fn crossover_matches_palette_consensus() {
        for hex in ["#007AFF", "#FF3B30", "#3E87FF", "#101012", "#5856D6"] {
            assert_eq!(pair_side(enc(hex)), PairSide::Light, "{hex}: белая сторона");
        }
        for hex in ["#FFA100", "#34C759", "#FFD000", "#FFFFFF", "#5AC8FA"] {
            assert_eq!(
                pair_side(enc(hex)),
                PairSide::Ink,
                "{hex}: чернильная сторона"
            );
        }
    }

    /// Минимальность сдвига: заливка тёмной стороны кроссовера двигается до
    /// строгой победы белого и ни шагом дальше; оттенок/хрома целы.
    #[test]
    fn light_side_nudges_minimally_preserving_identity() {
        for hex in ["#007AFF", "#FF3B30", "#3E87FF"] {
            let anchor = enc(hex);
            let fill = pair_fill(anchor, PairSide::Light);
            let y = wcag_y_encoded(fill);
            assert!(
                y < WHITE_WINS_Y,
                "{hex}: белый выигрывает строго (Y={y:.4})"
            );
            assert!(
                y > WHITE_WINS_Y - 0.004,
                "{hex}: сдвиг минимален (Y={y:.4}, граница {WHITE_WINS_Y})"
            );
            // Оттенок и хрома идентичности: a, b Oklab якоря сохранены.
            let lab_a = srgb_linear_to_oklab([
                srgb_gamma_inv(anchor[0]),
                srgb_gamma_inv(anchor[1]),
                srgb_gamma_inv(anchor[2]),
            ]);
            let lab_f = srgb_linear_to_oklab([
                srgb_gamma_inv(fill[0]),
                srgb_gamma_inv(fill[1]),
                srgb_gamma_inv(fill[2]),
            ]);
            assert!(
                (lab_a[1] - lab_f[1]).abs() < 0.02 && (lab_a[2] - lab_f[2]).abs() < 0.02,
                "{hex}: оттенок/хрома сохранены (Δa={:.4}, Δb={:.4})",
                (lab_a[1] - lab_f[1]).abs(),
                (lab_a[2] - lab_f[2]).abs()
            );
        }
    }

    /// Якоря, уже дающие победу своей стороны, не двигаются вовсе.
    #[test]
    fn winning_anchors_stay_put() {
        for hex in ["#FFA100", "#34C759", "#FFFFFF"] {
            let anchor = enc(hex);
            assert_eq!(
                hex_from_srgb_encoded(pair_fill(anchor, PairSide::Ink)),
                hex_from_srgb_encoded(anchor),
                "{hex}: чернильная сторона, заливка не тронута"
            );
        }
        let dark = enc("#101012");
        assert_eq!(
            hex_from_srgb_encoded(pair_fill(dark, PairSide::Light)),
            hex_from_srgb_encoded(dark),
            "тёмный якорь светлой стороны не тронут"
        );
    }

    /// Сторона — идентичность семьи: канонический светлый якорь решает один
    /// раз, тёмные/IC-варианты семьи НЕ флипают её (info dark #5696FF Y=0.31
    /// перелезает кроссовер — контрпример оси A).
    #[test]
    fn side_is_family_canonical_not_per_vc() {
        // Канон info (light) — светлая сторона...
        assert_eq!(pair_side(enc("#3E87FF")), PairSide::Light);
        // ...а его тёмный якорь сам по себе ушёл бы в чернила: фиксируем,
        // что при семейной стороне заливка двигается, лейбл остаётся белым.
        let fill = pair_fill(enc("#5696FF"), PairSide::Light);
        assert!(
            wcag_y_encoded(fill) < WHITE_WINS_Y,
            "тёмный якорь info утемнён под светлую сторону семьи"
        );
    }

    /// Чернильная сторона осветляет провалившиеся под границу IC-якоря
    /// (warning light-ic #C93400 Y=0.149): без осветления штатная полярность
    /// отдала бы белый и сторона флипнулась бы в IC.
    #[test]
    fn ink_side_lightens_sunken_ic_anchors() {
        let fill = pair_fill(enc("#C93400"), PairSide::Ink);
        let y = wcag_y_encoded(fill);
        assert!(
            y > BLACK_WINS_Y,
            "IC-якорь осветлён до победы чернил (Y={y:.4})"
        );
        assert!(y < BLACK_WINS_Y + 0.006, "сдвиг минимален (Y={y:.4})");
    }

    /// Сквозной закон: на выведенной заливке ШТАТНАЯ полярность отдаёт
    /// светлый лейбл (мост к nested resolve — пара не изобретает второй
    /// текстовый закон).
    #[test]
    fn nudged_fill_wins_light_polarity_in_standard_law() {
        use crate::BgInput;
        use crate::semantic::{Resolved, Role, RoleTable, resolve};
        use crate::spaces::vc::ViewingConditions;
        for hex in ["#007AFF", "#FF3B30"] {
            let fill = pair_fill(enc(hex), PairSide::Light);
            let fill_hex = hex_from_srgb_encoded(fill);
            let bg = BgInput::solid(&fill_hex).unwrap();
            let table = RoleTable::default();
            let label = match resolve(&bg, Role::LabelPrimary, &table, &ViewingConditions::srgb()) {
                Resolved::Color { solved, .. } => solved,
                other => panic!("{hex}: лейбл обязан решиться цветом, получено {other:?}"),
            };
            // Светлая полярность: лейбл светлее заливки.
            let label_y = wcag_y_encoded(enc(label.hex()));
            assert!(
                label_y > wcag_y_encoded(fill),
                "{hex}: лейбл светлый на выведенной заливке ({} на {fill_hex})",
                label.hex()
            );
        }
    }
}
