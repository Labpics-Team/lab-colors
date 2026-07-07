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
/// Терминал **(e) DESIGN-CHOICE**: свободный параметр, объявленный честно — с
/// модельным якорем, интервалом и измеренной экспозицией. НЕ «перцептивный
/// консенсус» и не измеренный порог:
/// - **Интервал:** (0.246, 0.423), зажат 10 Figma-якорями labui (красный
///   #FF3B30 Y=0.246 → белая сторона; зелёный #34C759 Y=0.423 → чернильная);
///   внутри него сторона канонических якорей инвариантна
///   (`crossover_side_is_invariant_across_palette_gap`).
/// - **Модельный якорь (конфликт вывода):** перелом «чёрный обгоняет белого»
///   самой метрики ИЗМЕРЕН: 0.3420 (люминансное ядро) / байт 155, Y ≈ 0.325
///   (полная метрика на серой оси) — лок
///   `model_polarity_crossover_is_measured_not_recited`. Модель предсказывает
///   перелом ВЫШЕ, чем 0.30: дизайн-тюнинг сидит ниже модельного предсказания;
///   значение НЕ меняется (INV-1), расхождение задекларировано.
/// - **Экспозиция:** 21.69% гаммы в интервале (`exposure_pair_crossover`).
///   Полярностная асимметрия чтения (класс исследований, на которых построен
///   APCA) — качественное основание, не источник числа.
///
/// **MODEL-CONFLICT: OWNER DECISION PENDING.** Shipped `0.30` | модель-выведено
/// `0.3420` (ядро) / `≈0.325` (полная метрика) | зазор `+0.025..+0.042` (модель
/// ВЫШЕ shipped). Это НЕ закрытый вопрос замера (в отличие от
/// [`crate::scale::HUE_DRIFT_PENALTY_SLOPE`], где единственный строгий
/// кандидат уже измерен и отклонён) — здесь модельный якорь существует,
/// согласован и просто НЕ принят: владелец должен явно выбрать (1) перейти на
/// модель-выведенное значение (~0.325–0.342) → станет (a) DERIVED, реальные
/// цвета на границе Y∈(0.30, 0.342) сдвинут сторону лейбла, ИЛИ (2) оставить
/// 0.30 осознанным дизайн-тюнингом (текущее состояние). Ре-аудит
/// `science/reclassify-e-buckets` 2026-07-07 не решает это ЗА владельца.
///
/// Провенанс = 10 якорей одной палитры, 0 наблюдателей; правило «нижняя треть»
/// интервала даёт 0.305, 0.30 — округление внутри интервала. До явного решения
/// владельца по пункту выше терминал держится (e) DESIGN-CHOICE: психофизический
/// эксперимент не требуется и путём разрешения не является (решение владельца
/// 2026-07-07 — таксономия (a)/(b)/(c)/(e)).
// SSOT-TRACKED — Y-порог кроссовера стороны пары, терминал (e) design-choice (model-conflict: OWNER DECISION PENDING), см. docs/empirical-inventory.md.
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

// ─────────────────────────────────────────────────────────────────────────────
// Научные локи терминала (e) DESIGN-CHOICE для PAIR_CROSSOVER_Y. Значение НЕ
// меняется — тесты предъявляют: (1) классификационный выход (сторона пары) на
// реальной палитре labui НЕ зависит от точного порога внутри задекларированного
// интервала; (2) модельный якорь (перелом полярности самой метрики) ИЗМЕРЕН, а
// не процитирован; (3) доля гаммы в зоне флипа замерена.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod exposure_locks {
    use super::{PAIR_CROSSOVER_Y, PairSide, pair_side, wcag_y_encoded};
    use crate::exposure_support::{LABUI_ANCHORS, band_exposure, enc_of, wcag_y};
    use crate::spaces::srgb::srgb_encoded_from_hex;

    fn y_of(hex: &str) -> f64 {
        wcag_y_encoded(srgb_encoded_from_hex(hex).unwrap())
    }

    /// (c) Sensitivity: на консенсус-сете labui сторона пары ИНВАРИАНТНА для любого
    /// порога в задекларированном интервале. Доказ.: max Y светлой стороны СТРОГО
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

    /// (e)-ЯКОРЬ ИЗМЕРЯЕТСЯ, НЕ ЦИТИРУЕТСЯ: перелом полярности модели — фон, на
    /// котором чёрный текст догоняет белый по |Lc|. Два домена:
    /// 1. Чистое люминансное ядро ([`crate::lpc::contrast_core`], вход — сырой Y):
    ///    бисекция — асимметрия опубликованных экспонент как таковая.
    /// 2. Полная метрика ([`crate::lpc::lpc`], серая ось 8-бит, внутри Y_hk с
    ///    CAM16-реконструкцией): перелом между соседними серыми байтами — модельное
    ///    предсказание в домене решения pair.rs (WCAG-Y якоря).
    ///
    /// Оба числа печатаются и пиннятся снапшотом: дрейф любой константы ядра ломает
    /// лок, и якорь (e)-декларации не может молча устареть. Конфликт вывода:
    /// дизайн-значение `PAIR_CROSSOVER_Y` = 0.30 обязано сидеть НИЖЕ обоих модельных
    /// предсказаний и внутри интервала якорей (0.246, 0.423); значение не меняется
    /// (INV-1), расхождение задекларировано в docs/empirical-inventory.md (строка 52).
    #[test]
    fn model_polarity_crossover_is_measured_not_recited() {
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
        // Снапшоты замера — регрессионный якорь (e)-декларации. Прежний
        // rustdoc-клейм «перелом ≈ 0.36» ОПРОВЕРГНУТ этим замером (2026-07-07):
        // ядро даёт 0.3420, полная метрика — ещё ниже (байт 155, Y ≈ 0.325).
        assert!(
            (core_x - 0.341955).abs() < 5e-4,
            "ядро: перелом ушёл от снапшота 0.3420: {core_x:.6}"
        );
        assert_eq!(
            v, 155,
            "полная метрика: перелом серой оси ушёл от снапшота (byte {v}, Y={y_flip:.4})"
        );
        // Конфликт вывода: 0.30 НИЖЕ модели, внутри интервала якорей.
        assert!(
            PAIR_CROSSOVER_Y < core_x && PAIR_CROSSOVER_Y < y_below,
            "дизайн-значение обязано лежать ниже модельного предсказания"
        );
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
        // Консистентность реимплементированного предиката с продакшн-решением.
        for &h in LABUI_ANCHORS {
            let enc = srgb_encoded_from_hex(h).unwrap();
            let prod = pair_side(enc);
            let reimpl = if wcag_y(enc_of(h)) < PAIR_CROSSOVER_Y {
                PairSide::Light
            } else {
                PairSide::Ink
            };
            assert_eq!(
                prod, reimpl,
                "{h}: реимпл-предикат расходится с продакшн pair_side"
            );
        }
    }
}
