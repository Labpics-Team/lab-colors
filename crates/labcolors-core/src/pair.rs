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
//! # Кроссовер (level-3)
//!
//! Сторона пары — свойство ИДЕНТИЧНОСТИ СЕМЬИ: решается ОДИН РАЗ по
//! каноническому светлому якорю ([`pair_side`], смешанное доменное сравнение
//! достижимого контраста двух архетипов лейбла) и не флипается между
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
use crate::spaces::vc::ViewingConditions;

/// Y-порог кроссовера стороны пары «заливка × лейбл» — решается ОДИН РАЗ по
/// каноническому светлому якорю семьи.
///
/// Терминал **(a) DERIVED** — не выбран, а ИЗМЕРЕН из опубликованной модели.
///
/// # Что этот порог решает
/// Ахроматический терминал решения стороны. С главы #64 (level-3) [`pair_side`]
/// не сравнивает люминанс с порогом напрямую — он меряет достижимый |Lc| обоих
/// архетипов лейбла той же кривой; на серой оси H-K-член ≈ 0 и правило
/// редуцируется ровно к этому порогу (лок
/// `achromatic_reduction_matches_derived_crossover`). Порог обязан стоять там,
/// где обе стороны РАВНО читаемы, — в чисто-люминансном домене самой кривой.
///
/// # Почему именно это число (вывод, не выбор)
/// Перцептивная читаемость меряется контраст-ядром APCA
/// ([`crate::lpc::contrast_core`], опубликованный набор SAPC-8 `0.0.98G-4g`).
/// Кроссовер = фон, на котором |Lc| ЧЁРНОГО лейбла догоняет |Lc| БЕЛОГО. Он
/// найден бисекцией (корень единственный на [0.2, 0.6], проверено сменой знака):
/// **Y = 0.341955**. Это не подгонка — это математическое свойство опубликованных
/// экспонент; дрейф любой из них ломает лок
/// `pair_crossover_equals_measured_core_polarity_flip` (пиннинг деривации).
///
/// # Почему не соседние кандидаты
/// - НЕ `0.179` (`WHITE_WINS_Y`) — то WCAG-tie, легальный ПОЛ гарантии контраста,
///   а не перцептивный оптимум читаемости.
/// - НЕ `0.325` (полная метрика [`crate::lpc::lpc`], байт 155 серой оси) — та
///   внутри несёт CAM16/Y_hk-реконструкцию ЯРКОСТНОГО домена, чужую для
///   чисто-люминансного решения `pair_side` (ось читаемости — люминанс, ADR-0003).
///
/// # Согласованность и почему это ещё и ЛУЧШЕ прежнего 0.30
/// 0.341955 лежит ВНУТРИ интервала 10 Figma-якорей labui (0.246, 0.423) — вывод
/// не спорит с палитрой, а совпадает с ней (экспозиция 21.69%,
/// `exposure_pair_crossover`). Прежнее 0.30 было дизайн-тюнингом НИЖЕ модели: при
/// Y ∈ [0.30, 0.342) оно ставило ЧЕРНИЛЬНЫЙ лейбл там, где по модели БЕЛЫЙ
/// читается лучше. Вывод не только объективен — он исправляет эту недодачу
/// светлых лейблов.
// С главы #64 (level-3) `pair_side` не потребляет этот порог в РАНТАЙМЕ — он
// выводит кроссовер из достижимого |Lc| обоих архетипов лейбла той же кривой.
// Порог остаётся АХРОМАТИЧЕСКИМ ТЕРМИНАЛОМ правила: на серой оси H-K-член ≈ 0 и
// level-3 редуцируется ровно к нему (лок `achromatic_reduction_matches_derived_crossover`),
// а его деривация пиннится `pair_crossover_equals_measured_core_polarity_flip`.
// Константа СОХРАНЕНА живой (не удалена, не test-only): её отслеживает
// docs/empirical-inventory.md (строка 52, (a) DERIVED) и валидируют локи
// `exposure_locks`; `#[allow(dead_code)]` — честная декларация «намеренно не
// рантайм-вход при level-3», не заглушка ошибки.
// SSOT-TRACKED — Y-порог кроссовера, терминал (a) DERIVED (перелом contrast_core, бисекция), см. docs/empirical-inventory.md.
#[allow(dead_code)]
pub(crate) const PAIR_CROSSOVER_Y: f64 = 0.341955;

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

/// Выбрать сторону пары для кодированного якоря — level-3: смешанное доменное
/// сравнение достижимого контраста двух архетипов лейбла.
///
/// `Ink ⇔ |Lc(чёрный, Y_eff)| > |Lc(белый, Ys)|`,
///
/// где `Lc` — контраст-кривая SAPC-8 ([`crate::lpc`], `contrast_core`), `Ys` —
/// WCAG-люминанс display-байтов якоря, `Y_eff` — воспринимаемая яркость якоря
/// (Гельмгольц–Кольрауш: `solve::bg_luma`, серый эквивалент J_HK по
/// Hellwig 2022 под каноническим sRGB-окружением).
///
/// Декомпозиция доменов — следствие ADR-0003, не его обход:
///
/// - читаемость СВЕТЛОГО лейбла — люминансная величина (Mullen 1985: детальную
///   разборчивость несёт ахроматический канал): белая сторона меряется в Ys,
///   H-K к ней не допущен — класс «H-K топит белый» (15:0, отчёт V3) исключён
///   конструкцией;
/// - «поверхность выглядит светлой → ей место нести чернила» — суждение о
///   ЯРКОСТИ поверхности, законный дом H-K по тому же ADR: чернильная сторона
///   меряется от Y_eff.
///
/// На ахроматической оси Y_eff ≈ Ys (H-K-член ≈ 0), и правило редуцируется к
/// РАВНО-|Lc| кроссоверу самой кривой Y* ≈ 0.342 — порог не задаётся, а
/// выводится (лок `achromatic_reduction_matches_derived_crossover`). Насыщенные
/// фоны флипают в чернила РАНЬШЕ по Ys — hue-каверна полнотекстов таска #62;
/// величина сдвига следует hue-зависимости H-K (Hellwig f(h)·C^0.587), новых
/// констант нет: обе стороны — существующие функции движка.
///
/// Тай (равенство скоров) отдан свету: полярностная асимметрия чтения — белый
/// предпочтителен, пока чернила строго не выиграли.
pub fn pair_side(anchor_encoded: [f64; 3]) -> PairSide {
    let ys = wcag_y_encoded(anchor_encoded);
    let lin = [
        srgb_gamma_inv(anchor_encoded[0]),
        srgb_gamma_inv(anchor_encoded[1]),
        srgb_gamma_inv(anchor_encoded[2]),
    ];
    let y_eff = crate::solve::bg_luma(lin, &ViewingConditions::srgb());
    let ink_score = crate::lpc::contrast_core(0.0, y_eff).abs();
    let white_score = crate::lpc::contrast_core(1.0, ys).abs();
    if ink_score > white_score {
        PairSide::Ink
    } else {
        PairSide::Light
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

    /// (а) Тёмные/IC-якоря Figma носят чернильный лейбл СТЭНДАЛОН: их Ys
    /// (0.31–0.32) чуть выше, но воспринимаемая яркость (0.43–0.47, H-K)
    /// далеко за кроссовером — перцептивно это светлая пастель (полнотексты
    /// таска #62). Семейные стороны это не трогает (канон решает светлый якорь).
    #[test]
    fn figma_dark_anchors_take_ink_side() {
        for hex in ["#FF6161", "#5696FF", "#FF6482", "#409CFF"] {
            assert_eq!(
                pair_side(enc(hex)),
                PairSide::Ink,
                "{hex}: чернильная сторона стэндалон"
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

// ─────────────────────────────────────────────────────────────────────────────
// Научные локи главы #64 (level-3, `level3_locks`): валидация выведенного
// кроссовера и H-K-сдвига стороны (Hellwig f(h)·C^0.587). Ниже — локи
// терминала (a) DERIVED `PAIR_CROSSOVER_Y` (`exposure_locks`): деривация
// пиннится замером, консенсус-сет держит чистый зазор, экспозиция зоны
// флипа замерена.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod level3_locks {
    use super::{PairSide, encode_clamped, pair_side, wcag_y_encoded};
    use crate::exposure_support::LABUI_ANCHORS;
    use crate::spaces::oklab::oklab_to_srgb_linear;
    use crate::spaces::srgb::srgb_encoded_from_hex;

    /// Отставленный порог level-2 — нужен тестам как БАЗА СРАВНЕНИЯ (карта
    /// флипов корпуса против прежнего поведения), не как правило.
    const LEVEL2_CROSSOVER_Y_RETIRED: f64 = 0.30;

    fn enc(hex: &str) -> [f64; 3] {
        srgb_encoded_from_hex(hex).expect("тестовые hex-литералы валидны")
    }

    /// Равно-|Lc| кроссовер контраст-кривой для архетипов белого (1.0) и
    /// чёрного (0.0) — бисекция самой кривой, не константа.
    fn derived_equal_lc_crossover() -> f64 {
        let (mut lo, mut hi) = (0.05_f64, 0.95_f64);
        for _ in 0..80 {
            let mid = 0.5 * (lo + hi);
            let white = crate::lpc::contrast_core(1.0, mid).abs();
            let black = crate::lpc::contrast_core(0.0, mid).abs();
            if white > black {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        0.5 * (lo + hi)
    }

    /// Максимальная хрома Oklch внутри sRGB-куба для (L, h) — бисекция по C.
    fn max_chroma(l: f64, h_rad: f64) -> f64 {
        let inside = |c: f64| {
            let lin = oklab_to_srgb_linear([l, c * h_rad.cos(), c * h_rad.sin()]);
            lin.iter().all(|&v| (-1e-9..=1.0 + 1e-9).contains(&v))
        };
        let (mut lo, mut hi) = (0.0_f64, 0.5_f64);
        for _ in 0..48 {
            let mid = 0.5 * (lo + hi);
            if inside(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// Насыщенная ячейка: (L, h, доля максимальной хромы) → display-байты.
    fn swatch(l: f64, h_rad: f64, rel_c: f64) -> [f64; 3] {
        let c = rel_c * max_chroma(l, h_rad);
        encode_clamped(oklab_to_srgb_linear([l, c * h_rad.cos(), c * h_rad.sin()]))
    }

    /// (в) Ахроматическая редукция: на серой оси H-K-член ≈ 0, и флип
    /// pair_side обязан совпасть с выведенным равно-|Lc| кроссовером Y* самой
    /// кривой (Y* = 0.3420) с точностью остаточной колоримости CAM16 на
    /// нейтралях (неполная хроматическая адаптация; задекларированный резидуй
    /// `lpc`). Резидуй строго ОДНОнаправлен: C ≥ 0 ⇒ Y_eff ≥ Ys ⇒ флип может
    /// лечь только НИЖЕ Y* — измерено 0.3340 (сдвиг 0.008 < 0.01). Обе точки
    /// заметно выше отставленного level-2 порога 0.30 — сам подъём кроссовера
    /// от резидуя не зависит.
    #[test]
    fn achromatic_reduction_matches_derived_crossover() {
        let y_star = derived_equal_lc_crossover();
        eprintln!("выведенный равно-|Lc| кроссовер Y* = {y_star:.6}");
        let side_at = |g: f64| pair_side([g, g, g]);
        assert_eq!(side_at(0.0), PairSide::Light, "чёрный фон — светлый лейбл");
        assert_eq!(side_at(1.0), PairSide::Ink, "белый фон — чернила");
        let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
        for _ in 0..48 {
            let mid = 0.5 * (lo + hi);
            if side_at(mid) == PairSide::Light {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let flip_ys = crate::spaces::srgb::srgb_gamma_inv(0.5 * (lo + hi));
        eprintln!("флип pair_side на серой оси: Ys = {flip_ys:.6}");
        let residual = y_star - flip_ys;
        assert!(
            (0.0..0.01).contains(&residual),
            "ахроматическая редукция: флип {flip_ys:.4} против Y* {y_star:.4} \
             (резидуй {residual:.4} обязан быть в [0, 0.01))"
        );
        assert!(
            flip_ys > LEVEL2_CROSSOVER_Y_RETIRED,
            "подъём кроссовера против level-2 реален (флип {flip_ys:.4} > 0.30)"
        );
    }

    /// (г) Корпус 49 якорей labui: ЕДИНСТВЕННОЕ расхождение с level-2 —
    /// #4A8FFF (Brand dark): Ys=0.283, воспринимаемая яркость 0.42 —
    /// перцептивно светлая пастель, чернила стэндалон. Семейная сторона
    /// Brand не затронута: её решает канонический светлый #007AFF (Light,
    /// пин консенсус-теста). Консенсус-10 и якоря (а) закреплены отдельно.
    #[test]
    fn corpus_flips_vs_level2_are_named_and_explained() {
        let mut flips: Vec<&str> = Vec::new();
        for &hex in LABUI_ANCHORS {
            let e = enc(hex);
            let old = if wcag_y_encoded(e) < LEVEL2_CROSSOVER_Y_RETIRED {
                PairSide::Light
            } else {
                PairSide::Ink
            };
            if old != pair_side(e) {
                flips.push(hex);
            }
        }
        eprintln!("флипы корпуса против level-2: {flips:?}");
        assert_eq!(
            flips,
            vec!["#4A8FFF"],
            "ровно один поимённо объяснённый флип"
        );
        assert_eq!(pair_side(enc("#4A8FFF")), PairSide::Ink);
    }

    /// (д) Свип класса этюда V3 (10 оттенков × 24 средне-светлых тона,
    /// максимальная хрома): H-K-сдвиг стороны против чистой Ys-модели идёт
    /// ТОЛЬКО в направлении Light→Ink (насыщенная поверхность выглядит
    /// светлее — раньше готова нести чернила; f(h) Хеллвига всюду > 0) и не
    /// топит ни одну ячейку с ИЗВЕСТНОЙ белой конвенцией — пины V3 #007AFF,
    /// #0082FF, #FF0000. Контр-конвенционный класс «H-K топит белый» пуст.
    #[test]
    fn v3_sweep_hk_shift_never_sinks_conventional_white() {
        let y_star = derived_equal_lc_crossover();
        let (mut light_to_ink, mut ink_to_light) = (0usize, 0usize);
        for hue_step in 0..10 {
            let h = f64::from(hue_step) * std::f64::consts::TAU / 10.0;
            for tone in 0..24 {
                let l = 0.30 + 0.60 * f64::from(tone) / 23.0;
                let cell = swatch(l, h, 1.0);
                let pure = if wcag_y_encoded(cell) < y_star {
                    PairSide::Light
                } else {
                    PairSide::Ink
                };
                match (pure, pair_side(cell)) {
                    (PairSide::Light, PairSide::Ink) => light_to_ink += 1,
                    (PairSide::Ink, PairSide::Light) => ink_to_light += 1,
                    _ => {}
                }
            }
        }
        eprintln!("свип 240: сдвигов white→ink {light_to_ink}, обратных {ink_to_light}");
        assert_eq!(
            ink_to_light, 0,
            "H-K не имеет права «затемнять» поверхность"
        );
        assert!(
            light_to_ink > 0,
            "hue-каверна обязана существовать (таск #62)"
        );
        for hex in ["#007AFF", "#0082FF", "#FF0000"] {
            assert_eq!(
                pair_side(enc(hex)),
                PairSide::Light,
                "{hex}: известная конвенция — белый лейбл"
            );
        }
    }

    /// (е) Монотонность стороны: вдоль линии одного hue по возрастанию
    /// светлоты — не более одного переключения, и только Light→Ink. Три
    /// уровня относительной хромы, включая гамут-границу (каспы).
    #[test]
    fn at_most_one_switch_along_hue_lightness_lines() {
        for hue_step in 0..12 {
            let h = f64::from(hue_step) * std::f64::consts::TAU / 12.0;
            for rel_c in [0.35, 0.70, 1.0] {
                let mut prev = None;
                let mut switches = 0usize;
                for step in 0..=120 {
                    let l = 0.02 + 0.96 * f64::from(step) / 120.0;
                    let side = pair_side(swatch(l, h, rel_c));
                    if let Some(p) = prev {
                        if side != p {
                            switches += 1;
                            assert_eq!(
                                (p, side),
                                (PairSide::Light, PairSide::Ink),
                                "hue {hue_step}, C_rel {rel_c}: только Light→Ink"
                            );
                        }
                    }
                    prev = Some(side);
                }
                assert!(
                    switches <= 1,
                    "hue {hue_step}, C_rel {rel_c}: {switches} переключений стороны"
                );
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Локи терминала (a) DERIVED `PAIR_CROSSOVER_Y` (после #180: (e)→(a), значение =
// измеренный перелом ядра `contrast_core`). Статус и деривация НЕ трогаются
// главой #64: тесты предъявляют (1) чистый зазор консенсус-сета вокруг порога
// (робастность деривации — сторона инвариантна к точному значению в интервале),
// (2) пиннинг деривации замером (константа РАВНА перелому ядра, не цитата), (3)
// экспозицию зоны флипа; петля консистентности с продакшн-решением учитывает
// level-3 (H-K может только ПОДНЯТЬ якорь в чернила, поимённо).
#[cfg(test)]
mod exposure_locks {
    use super::{PAIR_CROSSOVER_Y, PairSide, pair_side, wcag_y_encoded};
    use crate::exposure_support::{LABUI_ANCHORS, band_exposure, enc_of, wcag_y};
    use crate::spaces::srgb::srgb_encoded_from_hex;

    fn y_of(hex: &str) -> f64 {
        wcag_y_encoded(srgb_encoded_from_hex(hex).unwrap())
    }

    /// Sensitivity (робастность деривации): на консенсус-сете labui сторона пары
    /// ИНВАРИАНТНА для любого порога в задекларированном интервале. Доказ.: max Y светлой стороны СТРОГО
    /// ниже min Y чернильной (чистый зазор), и PAIR_CROSSOVER_Y лежит в нём — значит
    /// точное значение внутри зазора нематериально для этой палитры.
    #[test]
    fn crossover_side_is_invariant_across_palette_gap() {
        let light = ["#007AFF", "#FF3B30", "#3E87FF", "#101012", "#5856D6"];
        let ink = ["#FFA100", "#34C759", "#FFD000", "#FFFFFF", "#5AC8FA"];
        let light_max = light.iter().map(|h| y_of(h)).fold(0.0f64, f64::max);
        let ink_min = ink.iter().map(|h| y_of(h)).fold(f64::INFINITY, f64::min);
        assert!(
            light_max < ink_min,
            "консенсус-сет должен иметь чистый зазор Y (light_max={light_max:.4} < ink_min={ink_min:.4})"
        );
        assert!(
            light_max < PAIR_CROSSOVER_Y && PAIR_CROSSOVER_Y < ink_min,
            "PAIR_CROSSOVER_Y={PAIR_CROSSOVER_Y} должен лежать в зазоре ({light_max:.4}, {ink_min:.4})"
        );
        // Любой порог в зазоре даёт то же разбиение — проверяем на границах зазора.
        for theta in [light_max + 1e-6, ink_min - 1e-6, PAIR_CROSSOVER_Y] {
            for h in light {
                assert!(y_of(h) < theta, "{h}: светлая сторона при θ={theta:.4}");
            }
            for h in ink {
                assert!(y_of(h) >= theta, "{h}: чернильная сторона при θ={theta:.4}");
            }
        }
    }

    /// (a)-ЯКОРЬ ИЗМЕРЯЕТСЯ И ПИННИТ КОНСТАНТУ: перелом полярности модели — фон, на
    /// котором чёрный лейбл догоняет белый по |Lc|. Два домена:
    /// 1. Чистое люминансное ядро ([`crate::lpc::contrast_core`], вход — сырой Y):
    ///    бисекция. Это домен ахроматического терминала, к которому level-3
    ///    `pair_side` редуцируется на серой оси, — потому его перелом = якорь
    ///    `PAIR_CROSSOVER_Y`.
    /// 2. Полная метрика ([`crate::lpc::lpc`], серая ось 8-бит, внутри Y_hk с
    ///    CAM16-реконструкцией): вторичный замер яркостного домена (byte 155,
    ///    Y≈0.325) — печатается для сравнения, но решению pair.rs чужд, не якорь.
    ///
    /// Числа печатаются и пиннятся снапшотом: дрейф любой опубликованной константы
    /// ядра сдвинет перелом и СЛОМАЕТ лок — значит `PAIR_CROSSOVER_Y` не может молча
    /// разойтись с деривацией (это и есть пиннинг (a) DERIVED). Перелом лежит внутри
    /// интервала 10 якорей (0.246, 0.423) — вывод согласован с палитрой. Провенанс —
    /// docs/empirical-inventory.md (строка 52).
    #[test]
    fn pair_crossover_equals_measured_core_polarity_flip() {
        use crate::lpc::{contrast_core, lpc};
        use crate::spaces::srgb::srgb_gamma_inv;
        // 1. Ядро: f(Y) = |Lc белого| − |Lc чёрного| меняет знак на [0.2, 0.6].
        let f = |y: f64| contrast_core(1.0, y).abs() - contrast_core(0.0, y).abs();
        assert!(
            f(0.2) > 0.0 && f(0.6) < 0.0,
            "предпосылка бисекции: белый выигрывает на 0.2, чёрный на 0.6"
        );
        // Единственность корня: знак f меняется РОВНО один раз на сетке
        // интервала — без этого бисекция могла бы сойтись к одному из
        // нескольких пересечений (замечание CodeRabbit, PR #177).
        let mut sign_changes = 0u32;
        let mut prev_positive = true;
        let mut y = 0.2_f64;
        while y <= 0.6 {
            let cur_positive = f(y) > 0.0;
            if cur_positive != prev_positive {
                sign_changes += 1;
                prev_positive = cur_positive;
            }
            y += 0.002;
        }
        assert_eq!(
            sign_changes, 1,
            "f обязана пересекать ноль ровно один раз на [0.2, 0.6]"
        );
        let (mut lo, mut hi) = (0.2_f64, 0.6_f64);
        for _ in 0..60 {
            let mid = 0.5 * (lo + hi);
            if f(mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let core_x = 0.5 * (lo + hi);
        // 2. Полная метрика: первый серый байт, где чёрный лейбл обгоняет белый.
        let mut flip_byte = None;
        for v in 1..=255u32 {
            let hex = format!("#{v:02X}{v:02X}{v:02X}");
            if lpc("#000000", &hex).abs() >= lpc("#FFFFFF", &hex).abs() {
                flip_byte = Some(v);
                break;
            }
        }
        let v = flip_byte.expect("на серой оси перелом обязан существовать");
        let y_at = |b: u32| srgb_gamma_inv(f64::from(b) / 255.0);
        let (y_below, y_flip) = (y_at(v - 1), y_at(v));
        eprintln!(
            "MODEL CROSSOVER: core Y={core_x:.6}; full-metric grey axis: byte {v} (Y in ({y_below:.4}, {y_flip:.4}])"
        );
        // Снапшоты замера — регрессионный якорь (a)-деривации (прежний
        // rustdoc-клейм «перелом ≈ 0.36» опровергнут этим замером 2026-07-07).
        assert!(
            (core_x - 0.341955).abs() < 5e-4,
            "ядро: перелом ушёл от снапшота 0.3420: {core_x:.6}"
        );
        assert_eq!(
            v, 155,
            "полная метрика: перелом серой оси ушёл от снапшота (byte {v}, Y={y_flip:.4})"
        );
        // (a) DERIVED: константа РАВНА измеренному перелому ядра — якорь = ядро
        // `contrast_core`, а не полная метрика (byte 155, Y≈0.325 несёт
        // CAM16/Y_hk-реконструкцию; она остаётся вторичным замером, не якорем).
        assert!(
            (PAIR_CROSSOVER_Y - core_x).abs() < 1e-5,
            "PAIR_CROSSOVER_Y={PAIR_CROSSOVER_Y} обязан РАВНЯТЬСЯ измеренному перелому ядра {core_x:.6}"
        );
        // И вывод согласован с палитрой: перелом лежит внутри интервала 10 якорей.
        assert!(
            (0.246..0.423).contains(&core_x) && (0.246..0.423).contains(&y_flip),
            "модельные предсказания обязаны лежать внутри интервала якорей"
        );
    }

    /// EXPOSURE: доля гаммы в зоне флипа PAIR_CROSSOVER_Y = доля цветов с Y в
    /// задекларированном интервале (0.246, 0.423) [красный/зелёный якоря labui].
    /// Числа печатаются в отчёт (docs/empirical-residue.md). 12 якорей 49-якорного
    /// паспорта лежат в зоне (тёмные/IC-варианты семей, напр. info dark #5696FF
    /// Y=0.31) — это НЕ флип-риск: сторона решается ОДИН РАЗ по каноническому
    /// СВЕТЛОМУ якорю семьи, а консенсус-сет канонических якорей имеет чистый
    /// зазор (`crossover_side_is_invariant_across_palette_gap`).
    #[test]
    fn exposure_pair_crossover() {
        let (lo, hi) = (0.246, 0.423);
        let (grid_pct, labui_hits) = band_exposure(wcag_y, lo, hi);
        eprintln!(
            "EXPOSURE PAIR_CROSSOVER_Y interval=({lo},{hi}) grid_flip={grid_pct:.2}% labui_in_zone={} {:?}",
            labui_hits.len(),
            labui_hits
        );
        // Консистентность с продакшн-решением при level-3 (глава #64): H-K
        // строго однонаправлен — против порога якорь может только ПОДНЯТЬСЯ в
        // чернила (цветная поверхность выглядит светлее), никогда — утонуть в
        // свет. Промоции поимённые и пиннятся снапшотом.
        let mut promoted: Vec<&str> = Vec::new();
        for &h in LABUI_ANCHORS {
            let enc = srgb_encoded_from_hex(h).unwrap();
            let prod = pair_side(enc);
            let reimpl = if wcag_y(enc_of(h)) < PAIR_CROSSOVER_Y {
                PairSide::Light
            } else {
                PairSide::Ink
            };
            match (reimpl, prod) {
                (PairSide::Ink, PairSide::Light) => {
                    panic!("{h}: level-3 утопил чернильный якорь в свет — H-K однонаправлен")
                }
                (PairSide::Light, PairSide::Ink) => promoted.push(h),
                _ => {}
            }
        }
        eprintln!("level-3 промоции против порога: {promoted:?}");
        // Снапшот измеренных промоций level-3 (H-K, глава #64): насыщенные
        // средне-светлые фоны, у которых чистый Ys < порога, но воспринимаемая
        // яркость (H-K) поднимает их над кроссовером → чернильный лейбл. Все
        // пять — блю/красно-розовые пастели; ни одна «известная конвенция»
        // (#007AFF и т.п.) не затронута (пин `v3_sweep_...`). H-K строго
        // однонаправлен: панику «утопил чернила в свет» выше ни один не вызвал.
        assert_eq!(
            promoted,
            vec!["#409CFF", "#4A8FFF", "#5696FF", "#FF6161", "#FF6482"],
            "промоции level-3 против ахроматического терминала — поимённый снапшот"
        );
    }
}
