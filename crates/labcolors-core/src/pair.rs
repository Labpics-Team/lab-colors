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
//! [`PAIR_CROSSOVER_Y`] — VC-независимый порог по WCAG-люминансу якоря
//! (стабильность полярности между темами, то же свойство, что у
//! `choose_polarity`). Ниже порога поверхность перцептивно тёмная → белая
//! сторона; выше — светлая → чернильная. Чернильная сторона уже выигрывает
//! штатную полярность на светлых якорях (жёлтый Y≈0.47, зелёный Y≈0.42) —
//! заливка не двигается вовсе.
//!
//! Белая сторона требует строгой победы белого в `choose_polarity`:
//! `(Y + 0.05)² < 1.05 · 0.05`, т.е. Y < 0.17913 (белый ≥ 4.58:1) — граница
//! выведена из самой формулы WCAG, не подобрана. Сдвиг для фирменных якорей
//! мал: #007AFF (0.211 → 0.179) — едва заметное утемнение при том же оттенке.

use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
use crate::spaces::srgb::srgb_gamma_inv;

/// Перцептивный кроссовер стороны пары — порог WCAG-люминанса якоря.
///
/// Обоснование: полярностная асимметрия чтения (тёмный текст выигрывает
/// только на действительно светлых фонах; класс исследований APCA, кроссовер
/// восприятия ≈ Y 0.30–0.36). Значение зажато консенсус-кейсами палитры
/// labui: красный #FF3B30 (Y = 0.246) обязан получить белый, зелёный #34C759
/// (Y = 0.423) — чернила; интервал (0.246, 0.423), выбрана нижняя треть —
/// ближе к перцептивным замерам. Калибруется тестом по всем 10 семьям.
// SSOT-TRACKED — перцептивный кроссовер стороны пары, см. docs/empirical-inventory.md.
pub(crate) const PAIR_CROSSOVER_Y: f64 = 0.30;

/// Строгая граница победы белой стороны в `choose_polarity`:
/// `(Y + 0.05)² < 1.05 · 0.05`. Выведена из формулы WCAG (не настройка):
/// ниже неё белый и по ратио, и по tie-break выигрывает штатную полярность.
pub(crate) const WHITE_WINS_Y: f64 = 0.179;

/// WCAG-люминанс кодированного (byte/255) sRGB.
fn wcag_y_encoded(rgb: [f64; 3]) -> f64 {
    let lin = [
        srgb_gamma_inv(rgb[0]),
        srgb_gamma_inv(rgb[1]),
        srgb_gamma_inv(rgb[2]),
    ];
    0.2126 * lin[0] + 0.7152 * lin[1] + 0.0722 * lin[2]
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

/// Заливка пары: якорь, минимально сдвинутый до победы выбранной стороны.
///
/// Светлая сторона: бисекция по L Oklab (a, b — оттенок и хрома идентичности —
/// не трогаются) до `Y < WHITE_WINS_Y`; уже тёмный якорь возвращается как
/// есть. Чернильная сторона: якорь как есть (светлые якоря уже отдают тёмную
/// полярность штатному закону).
pub fn pair_fill(anchor_encoded: [f64; 3]) -> [f64; 3] {
    match pair_side(anchor_encoded) {
        PairSide::Ink => anchor_encoded,
        PairSide::Light => {
            if wcag_y_encoded(anchor_encoded) < WHITE_WINS_Y {
                return anchor_encoded;
            }
            let lin = [
                srgb_gamma_inv(anchor_encoded[0]),
                srgb_gamma_inv(anchor_encoded[1]),
                srgb_gamma_inv(anchor_encoded[2]),
            ];
            let lab = srgb_linear_to_oklab(lin);
            // Бисекция минимального утемнения: инвариант — lo даёт победу
            // белого, hi нет; сходимся к максимальной светлоте с победой.
            let mut lo = 0.0_f64;
            let mut hi = lab[0];
            for _ in 0..48 {
                let mid = 0.5 * (lo + hi);
                let cand = encode_clamped(oklab_to_srgb_linear([mid, lab[1], lab[2]]));
                if wcag_y_encoded(cand) < WHITE_WINS_Y {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            encode_clamped(oklab_to_srgb_linear([lo, lab[1], lab[2]]))
        }
    }
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
        srgb_encoded_from_hex(hex).unwrap()
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
            let fill = pair_fill(anchor);
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

    /// Чернильная сторона и уже-тёмные якоря не двигаются вовсе.
    #[test]
    fn ink_side_and_already_dark_anchors_stay_put() {
        for hex in ["#FFA100", "#34C759", "#101012", "#FFFFFF"] {
            let anchor = enc(hex);
            assert_eq!(
                hex_from_srgb_encoded(pair_fill(anchor)),
                hex_from_srgb_encoded(anchor),
                "{hex}: заливка не тронута"
            );
        }
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
            let fill = pair_fill(enc(hex));
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
