//! Двухслойный материал: опаковая тон-база + полупрозрачный тинт с ВЫВЕДЕННОЙ
//! альфой (композит-гарантия над коридором фонов).
//!
//! # Модель
//!
//! Материальная поверхность (стекло/акрил) — полупрозрачный тинт `01` над
//! непрозрачной базой `02`. В СОЛИД-режиме видно `01`-над-`02`; в GLASS-режиме
//! база отброшена и `01` лежит над ЖИВЫМ, авторски неизвестным фоном
//! (`backdrop-filter`). База и тинт — ОДИН тон `T` (семейно-оттеночный опаковый
//! цвет на целевой светлоте тира): база = `T` непрозрачна, тинт = `T` при альфе
//! `α`. Тогда солид-канон `01`-над-`02` = `α·T + (1−α)·T = T` — байт-точно при
//! любой `α` (композит `T` над `T` есть `T`). Единственная РЕШАЕМАЯ величина — `α`.
//!
//! # Выведенная альфа (не рукописная)
//!
//! `α` — минимальная плотность, при которой тинт над ХУДШИМ разрешённым фоном
//! остаётся в контракте базы: коммит-лейбл поверхности (ахроматический полюс
//! максимального контраста на тоне `T` — белый на тёмном `T`, чёрный на светлом)
//! держит пол читаемости ПО ВСЕМУ коридору достижимых фонов.
//!
//! Композит `α·T + (1−α)·B` афинно-возрастает по каждому каналу фона `B`, а
//! WCAG-светлота монотонно растёт по каждому каналу композита — поэтому по
//! осепараллельному коробу-коридору `[min, max]` светлота композита экстремальна
//! РОВНО на углах `min`/`max` (доказано `band_luminance`+property-тестом). Худший
//! контраст полюса берётся по этой достижимой полосе `[effLow, effHigh]`:
//! эквивалент α-граничной гарантии потребителя (`material-guarantee.ts`), но
//! ИНВЕРТИРОВАННОЙ — не «годен/негоден при данной α», а «минимальная годная α».
//!
//! Худший контраст монотонно растёт по `α` (полоса стягивается к `T`, полюс на
//! верной стороне), поэтому солвер — бисекция, как остальные полы движка
//! (`glow::solve_screen_alpha_for_dj`). На `α = 1` полоса вырождается в `L(T)`, а
//! полюс максимального контраста на ЛЮБОМ тоне даёт ≥ 4.58:1 (кроссовер чёрного и
//! белого полюса при `L ≈ 0.179`), поэтому для пола ≤ AA годная `α ∈ (0, 1]`
//! существует всегда; при более высоком поле (напр. AAA на среднем тоне)
//! честно возвращается `α = 1` с флагом [`MaterialAlpha::degraded`] — не молчание.
//!
//! # Пространство
//!
//! Композит — гамма-кодированный sRGB (device-пространство браузера/Figma), то же
//! измеренное пространство, что у [`crate::alpha`] (12 Figma-пар roundtrip). WCAG-
//! светлота меряется на кодированном тоне-тинте (квантованном до 8-битного hex —
//! эмитируемый цвет `01`), композит над фоном берётся ТОЧНО (без переквантования):
//! ровно так потребитель пересчитывает вердикт из эмитированных `01`/`02`, поэтому
//! гарантия ядра и re-check потребителя тождественны.

use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};
use crate::wcag::{ratio_from_luminances, relative_luminance};

/// Ахроматический полюс коммит-лейбла поверхности: цвет максимального контраста
/// на тоне (белый на тёмном тоне, чёрный на светлом). Механизм полярности
/// [`crate::pair`]/`commitLabel` в чистом, параметр-свободном виде.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pole {
    /// Чёрный лейбл (`L = 0`) — полюс максимального контраста на СВЕТЛОМ тоне.
    Black,
    /// Белый лейбл (`L = 1`) — полюс максимального контраста на ТЁМНОМ тоне.
    White,
}

impl Pole {
    /// WCAG-светлота полюса: `0.0` (чёрный) / `1.0` (белый).
    fn luminance(self) -> f64 {
        match self {
            Pole::Black => 0.0,
            Pole::White => 1.0,
        }
    }
}

/// Осепараллельный короб достижимых фонов: поканальный минимум и максимум.
///
/// `min`/`max` — кодированные углы `[0,1]³`. Материальный дефолт (стекло над
/// неизвестным живым фоном) — [`FULL`](Self::FULL) = `[чёрный, белый]`, худший
/// возможный коридор. Известная область (изображение/градиент под лейблом) — её
/// поканальные экстремумы (обобщение коридора, labui ADR-0004 Решение 2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackdropBox {
    /// Поканально-минимальный (темнейший) угол фона.
    pub min: [f64; 3],
    /// Поканально-максимальный (светлейший) угол фона.
    pub max: [f64; 3],
}

impl BackdropBox {
    /// Полный коридор `[чёрный, белый]` — материальный случай (неизвестный живой
    /// фон): худший возможный диапазон, самая консервативная гарантия.
    pub const FULL: BackdropBox = BackdropBox {
        min: [0.0, 0.0, 0.0],
        max: [1.0, 1.0, 1.0],
    };
}

/// Результат вывода альфы материала: плотность + вердикт гарантии.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialAlpha {
    /// Выведенная альфа тинта `01`, `(0, 1]`.
    pub alpha: f64,
    /// Худший WCAG-контраст коммит-полюса по коридору при выведенной `alpha`
    /// (`[1, 21]`). При не-деградации ≥ запрошенного пола.
    pub worst_contrast: f64,
    /// Коммит-полюс поверхности (полярность лейбла тона).
    pub pole: Pole,
    /// Даже при `α = 1` худший контраст ниже пола (напр. AAA-пол на среднем
    /// тоне): возвращена `α = 1` как ближайшая достижимая, гарантия НЕ выполнена.
    /// Честный флаг деградации контракта (закон 2 ADR-0002), не молчание.
    pub degraded: bool,
}

/// Валидный кодированный цвет: конечный и в `[0,1]³`.
fn is_encoded_rgb(v: [f64; 3]) -> bool {
    v.into_iter()
        .all(|x| x.is_finite() && (0.0..=1.0).contains(&x))
}

/// Квантовать кодированный цвет до 8-битной сетки дисплея (round-trip через hex —
/// то же представление, в котором браузер отдаёт пиксели и эмитируется `01`).
fn quantise(v: [f64; 3]) -> [f64; 3] {
    srgb_encoded_from_hex(&hex_from_srgb_encoded(v))
        .expect("hex собственного форматтера всегда валиден")
}

/// Коммит-полюс поверхности тона: полюс максимального WCAG-контраста на `L(tone)`.
///
/// Белый полюс на тёмном тоне, чёрный на светлом; граница — кроссовер
/// `L ≈ 0.1791` (там оба полюса дают ≈ 4.58:1). Тон квантуется (полюс — свойство
/// ЭМИТИРУЕМОГО тона). Мусор-вход отвергается.
pub fn committed_pole_encoded(tone: [f64; 3]) -> Option<Pole> {
    if !is_encoded_rgb(tone) {
        return None;
    }
    let l = relative_luminance(quantise(tone));
    // contrast(белый, L) > contrast(чёрный, L) ⇔ 1.05/(L+0.05) > (L+0.05)/0.05
    //                                          ⇔ (L+0.05)² < 0.0525.
    let s = l + 0.05;
    Some(if s * s < 1.05 * 0.05 {
        Pole::White
    } else {
        Pole::Black
    })
}

/// Достижимая полоса светлоты композита `[effLow, effHigh]`: тинт над двумя
/// углами короба-коридора. Углы — истинные экстремумы по монотонности (см.
/// шапку); `min`/`max` защищают порядок при перепутанном коробе.
fn band_luminance(tint_q: [f64; 3], alpha: f64, backdrop: &BackdropBox) -> (f64, f64) {
    debug_assert!(
        (0..3).all(|c| backdrop.min[c] <= backdrop.max[c]),
        "band_luminance: короб коридора не поканальный (min ≤ max нарушен) — \
         перепутанные углы дали бы неэкстремальную полосу"
    );
    let composite = |bg: [f64; 3]| {
        [
            alpha * tint_q[0] + (1.0 - alpha) * bg[0],
            alpha * tint_q[1] + (1.0 - alpha) * bg[1],
            alpha * tint_q[2] + (1.0 - alpha) * bg[2],
        ]
    };
    let la = relative_luminance(composite(backdrop.min));
    let lb = relative_luminance(composite(backdrop.max));
    (la.min(lb), la.max(lb))
}

/// Худший WCAG-контраст полюса по достижимой полосе `[lo, hi]` (честный вердикт,
/// не только на концах): если светлота полюса строго ВНУТРИ полосы — некий фон
/// доводит композит до неё → 1:1; иначе — ближний конец.
fn worst_contrast_of_band(pole: Pole, lo: f64, hi: f64) -> f64 {
    let p = pole.luminance();
    // Ветка «полюс строго ВНУТРИ полосы → 1:1» недостижима для АХРОМАТИЧЕСКОГО
    // полюса (`p ∈ {0, 1}`, полоса ⊆ `[0,1]`, крайние точки не «строго внутри»):
    // держится общей на случай будущего не-ахроматического коммит-полюса, а не
    // как мёртвый код (тест `committed_pole_maximises_contrast` фиксирует, что
    // сегодня полюс всегда ахроматический).
    if p > lo && p < hi {
        1.0
    } else {
        ratio_from_luminances(p, lo).min(ratio_from_luminances(p, hi))
    }
}

/// Худший WCAG-контраст коммит-полюса тинта `tone` при `alpha` над коридором.
///
/// Тон квантуется (эмитируемый `01`); композит над углами берётся точно.
/// `None` — мусор-вход (не кодированный тон / `alpha` вне `[0,1]`).
pub fn worst_contrast_encoded(
    tone: [f64; 3],
    alpha: f64,
    backdrop: &BackdropBox,
    pole: Pole,
) -> Option<f64> {
    if !(is_encoded_rgb(tone)
        && is_encoded_rgb(backdrop.min)
        && is_encoded_rgb(backdrop.max)
        && alpha.is_finite()
        && (0.0..=1.0).contains(&alpha))
    {
        return None;
    }
    let (lo, hi) = band_luminance(quantise(tone), alpha, backdrop);
    Some(worst_contrast_of_band(pole, lo, hi))
}

/// Вывести минимальную альфу тинта `01`: наименьшую плотность, при которой
/// композит тона над ХУДШИМ фоном коридора держит коммит-полюс на поле
/// `floor_ratio` (WCAG-отношение, напр. 4.5).
///
/// Бисекция по `α ∈ (0, 1]` (худший контраст монотонно растёт по `α`), замер на
/// квантованном тоне-тинте. Верхняя сторона брекета гарантирует
/// `worst_contrast ≥ floor_ratio` на возвращённой `α`. При недостижимости пола
/// даже на `α = 1` — честный `degraded` (не ошибка, не молчание).
///
/// `None` — вход вне домена: не кодированный `tone` / `floor_ratio` не в
/// `[1, 21]` (WCAG-отношение); ЛИБО тон лежит ВНЕ короба коридора со стороны
/// полюса, где худший контраст НЕ монотонен по `α` и бисекция неприменима (см.
/// ниже). Материальный путь (полный коридор `[чёрный, белый]`) в этот `None`
/// никогда не попадает — тон всегда в кубе.
pub fn solve_material_alpha_encoded(
    tone: [f64; 3],
    backdrop: &BackdropBox,
    floor_ratio: f64,
) -> Option<MaterialAlpha> {
    if !(is_encoded_rgb(tone)
        && is_encoded_rgb(backdrop.min)
        && is_encoded_rgb(backdrop.max)
        && floor_ratio.is_finite()
        && (1.0..=21.0).contains(&floor_ratio))
    {
        return None;
    }
    let tone_q = quantise(tone);
    let pole = committed_pole_encoded(tone_q)?;
    // Бисекция корректна ЛИШЬ когда худший контраст монотонно растёт по `α`, а это
    // держится только если тон лежит в коробе коридора со стороны полюса: у чёрного
    // полюса композит над темнейшим углом обязан СВЕТЛЕТЬ с ростом `α` (нужно
    // `tone ≥ min` поканально), у белого — композит над светлейшим углом обязан
    // ТЕМНЕТЬ (`tone ≤ max`). Полный коридор `[чёрный, белый]` гарантирует это
    // всегда (материальный путь); узкий коридор с тоном ВНЕ короба со стороны
    // полюса выводит из монотонного домена — тогда бисекция дала бы ложную `α`/
    // деградацию, поэтому честный `None` (дисциплина «мусор-вход → None»), а не
    // молчаливо неверный ответ. Обобщённый узкий коридор (ADR-0004) с тоном вне
    // короба потребует иного солвера (скан/тернарный поиск), не бисекции.
    let monotone = match pole {
        Pole::Black => (0..3).all(|c| tone_q[c] >= backdrop.min[c]),
        Pole::White => (0..3).all(|c| tone_q[c] <= backdrop.max[c]),
    };
    if !monotone {
        return None;
    }
    let worst_at = |alpha: f64| {
        let (lo, hi) = band_luminance(tone_q, alpha, backdrop);
        worst_contrast_of_band(pole, lo, hi)
    };

    // α = 1: полоса вырождается в L(tone) → худший контраст = контраст полюса на
    // тоне (солид-канон). Если и он ниже пола — честная деградация (ближайшее
    // достижимое), гарантия не выполнена.
    let opaque = worst_at(1.0);
    if opaque < floor_ratio {
        return Some(MaterialAlpha {
            alpha: 1.0,
            worst_contrast: opaque,
            pole,
            degraded: true,
        });
    }

    // Бисекция минимальной годной α. lo держит НЕгодную сторону, hi — годную.
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..60 {
        let mid = 0.5 * (lo + hi);
        if worst_at(mid) >= floor_ratio {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Some(MaterialAlpha {
        alpha: hi,
        worst_contrast: worst_at(hi),
        pole,
        degraded: false,
    })
}

/// Hex-обёртка [`solve_material_alpha_encoded`] над полным коридором
/// `[чёрный, белый]` (материальный случай — неизвестный живой фон).
///
/// # Errors
///
/// `Err` при невалидном hex или `floor_ratio` вне `[1, 21]`.
pub fn solve_material_alpha_hex(tone_hex: &str, floor_ratio: f64) -> Result<MaterialAlpha, String> {
    let tone = srgb_encoded_from_hex(tone_hex)?;
    solve_material_alpha_encoded(tone, &BackdropBox::FULL, floor_ratio)
        .ok_or_else(|| format!("floor_ratio вне домена [1,21]: {floor_ratio}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alpha::composite_over_encoded;
    use crate::wcag::contrast_ratio;

    const AA_TEXT: f64 = 4.5;

    fn enc(hex: &str) -> [f64; 3] {
        srgb_encoded_from_hex(hex).unwrap()
    }

    /// Солид-канон `01`-над-`02` байт-точно равен тону при ЛЮБОЙ α (композит `T`
    /// над `T` есть `T`) — фундамент дизайна: единственная решаемая величина α.
    #[test]
    fn solid_canon_is_tone_byte_exact_for_any_alpha() {
        for hex in ["#FFFFFF", "#787880", "#101012", "#3E87FF", "#B0B0B8"] {
            let t = enc(hex);
            let tq = quantise(t);
            for alpha in [0.01, 0.1, 0.5, 0.837, 1.0] {
                let solid = composite_over_encoded(tq, alpha, tq)
                    .expect("тестовые sRGB-каналы и alpha лежат в домене");
                assert_eq!(
                    hex_from_srgb_encoded(solid),
                    hex_from_srgb_encoded(tq),
                    "{hex}@{alpha}: солид-канон 01-над-02 разошёлся с тоном"
                );
            }
        }
    }

    /// На выведенной α худший фон держит пол, а ЧУТЬ НИЖЕ α — рвёт его: α есть
    /// ТОЧНАЯ граница годности (реальный солвер-пол, не подгонка).
    #[test]
    fn solved_alpha_is_the_exact_floor_boundary() {
        for hex in ["#E4E4E6", "#B0B0B8", "#35353A", "#2A2A30", "#5C5C5C"] {
            let m = solve_material_alpha_hex(hex, AA_TEXT).unwrap();
            assert!(!m.degraded, "{hex}: неожиданная деградация на AA");
            assert!(m.alpha > 0.0 && m.alpha <= 1.0, "{hex}: α вне (0,1]");
            let pole = m.pole;
            let at =
                |a: f64| worst_contrast_encoded(enc(hex), a, &BackdropBox::FULL, pole).unwrap();
            // На α держит пол (с крошечным допуском на брекет бисекции).
            assert!(
                at(m.alpha) >= AA_TEXT - 1e-9,
                "{hex}: на α={} худший контраст {} < пола",
                m.alpha,
                at(m.alpha)
            );
            // Чуть ниже α — рвёт (если α ещё не у самого пола нуля).
            if m.alpha > 1e-3 {
                assert!(
                    at(m.alpha * 0.98) < AA_TEXT,
                    "{hex}: годно НИЖЕ α={} — граница не минимальна",
                    m.alpha
                );
            }
        }
    }

    /// Худший контраст монотонно НЕ убывает по α (полоса стягивается к тону) —
    /// инвариант, на котором стоит бисекция. Свип на тонах обеих полярностей.
    #[test]
    fn worst_contrast_is_monotone_in_alpha() {
        for hex in ["#EDEDEF", "#C8C8CE", "#9A9AA2", "#40404A", "#1C1C1F"] {
            let tone = enc(hex);
            let pole = committed_pole_encoded(tone).unwrap();
            let mut prev = 0.0;
            for i in 1..=100 {
                let a = f64::from(i) / 100.0;
                let w = worst_contrast_encoded(tone, a, &BackdropBox::FULL, pole).unwrap();
                assert!(
                    w >= prev - 1e-9,
                    "{hex}: худший контраст упал на α={a}: {w} < {prev}"
                );
                prev = w;
            }
        }
    }

    /// Гарантия воспроизводима из эмитированных значений (гарантия (c) контракта):
    /// композит квантованного тинта над чёрным/белым точно даёт худший контраст,
    /// совпадающий с вердиктом солвера — потребитель пересчитает то же число.
    #[test]
    fn guarantee_recomputable_from_emitted_tint() {
        for hex in ["#E9E9EB", "#A0A0A8", "#313135"] {
            let m = solve_material_alpha_hex(hex, AA_TEXT).unwrap();
            let tint_q = quantise(enc(hex));
            // Пересчёт «в лоб» как у потребителя: композит над двумя углами.
            let over_black = composite_over_encoded(tint_q, m.alpha, [0.0; 3])
                .expect("решённый материал лежит в домене композитора");
            let over_white = composite_over_encoded(tint_q, m.alpha, [1.0; 3])
                .expect("решённый материал лежит в домене композитора");
            let pole_lum = m.pole.luminance();
            let recomputed = ratio_from_luminances(pole_lum, relative_luminance(over_black)).min(
                ratio_from_luminances(pole_lum, relative_luminance(over_white)),
            );
            assert!(
                (recomputed - m.worst_contrast).abs() < 1e-9,
                "{hex}: пересчёт {recomputed} != вердикту {}",
                m.worst_contrast
            );
            assert!(recomputed >= AA_TEXT - 1e-9, "{hex}: пересчёт ниже пола");
        }
    }

    /// Коммит-полюс = полюс максимального контраста: светлый тон → чёрный лейбл,
    /// тёмный → белый. Сверка против прямого WCAG-максимума.
    #[test]
    fn committed_pole_maximises_contrast() {
        for hex in [
            "#FFFFFF", "#EDEDEF", "#C0C0C0", "#808080", "#5C5C5C", "#303030", "#101012", "#000000",
            // Насыщенные хроматические тоны обеих полярностей (полюс — свойство
            // светлоты, но проверяем и на цвете).
            "#FFCC00", "#34C759", "#3E87FF", "#FF3B30", "#AF52DE", "#0A3A6B",
        ] {
            let tone = quantise(enc(hex));
            let pole = committed_pole_encoded(tone).unwrap();
            let c_black = contrast_ratio([0.0; 3], tone);
            let c_white = contrast_ratio([1.0; 3], tone);
            let want = if c_black >= c_white {
                Pole::Black
            } else {
                Pole::White
            };
            assert_eq!(pole, want, "{hex}: полюс не максимизирует контраст");
        }
    }

    /// AA-пол разрешим на ЛЮБОМ тоне (полюс максимального контраста даёт ≥ 4.58 на
    /// α=1) — теорема существования годной α ∈ (0,1]. Свип по всей серой оси И по
    /// насыщенным ХРОМАТИЧЕСКИМ тонам (теорема хрома-независима: max-контраст
    /// полюса — функция только светлоты, минимум 4.58 в кроссовере).
    #[test]
    fn aa_floor_always_solvable_no_degradation() {
        for i in 0..=255 {
            let g = f64::from(i) / 255.0;
            let m = solve_material_alpha_encoded([g, g, g], &BackdropBox::FULL, AA_TEXT).unwrap();
            assert!(
                !m.degraded,
                "серый {i}: AA обязан быть разрешим без деградации"
            );
            assert!(m.alpha > 0.0 && m.alpha <= 1.0);
        }
        // Насыщенные тоны разных светлот/оттенков — не только серые.
        for hex in [
            "#3E87FF", "#FF3B30", "#34C759", "#FFCC00", "#AF52DE", "#007AFF", "#B03030", "#0A3A6B",
        ] {
            let m = solve_material_alpha_hex(hex, AA_TEXT).unwrap();
            assert!(!m.degraded, "{hex}: AA обязан быть разрешим без деградации");
            assert!(m.alpha > 0.0 && m.alpha <= 1.0);
        }
    }

    /// Guard монотонности: узкий коридор с тоном ВНЕ короба со стороны полюса
    /// (где худший контраст НЕ монотонен по α) честно отвергается солвером — а не
    /// возвращает ложную α/деградацию. Контрпример из независимой верификации:
    /// тон `#8A8A8A` (Lum≈0.25, чёрный полюс) над коридором `[#B3B3B3, белый]`
    /// (все фоны СВЕТЛЕЕ тона) — `tone < min`, немонотонно.
    #[test]
    fn guard_rejects_non_monotone_narrow_corridor() {
        let tone = enc("#8A8A8A");
        assert_eq!(committed_pole_encoded(tone), Some(Pole::Black));
        let bad = BackdropBox {
            min: enc("#B3B3B3"),
            max: [1.0; 3],
        };
        // Немонотонный домен → честный None, не ложная деградация.
        assert!(
            solve_material_alpha_encoded(tone, &bad, 7.0).is_none(),
            "тон вне короба со стороны полюса обязан быть отвергнут (немонотонно)"
        );
        // Но worst_contrast_encoded НЕ предполагает монотонность — считает верно.
        assert!(worst_contrast_encoded(tone, 0.5, &bad, Pole::Black).is_some());
        // Монотонный узкий коридор (тон ВНУТРИ короба, tone ≥ min) — решается.
        let good = BackdropBox {
            min: enc("#202020"),
            max: [1.0; 3],
        };
        assert!(
            solve_material_alpha_encoded(tone, &good, AA_TEXT).is_some(),
            "тон в коробе — монотонный домен, обязан решиться"
        );
    }

    /// Тон дальше от фона (прим. более серый на светлой теме) требует ПЛОТНЕЕ α,
    /// чем тон ближе к фону — порядок тиров (base плотнее subtle) выводится
    /// физикой, не подбором. Светлая тема: белый фон, тон тем темнее, чем дальше.
    #[test]
    fn denser_tone_needs_higher_alpha_light_theme() {
        // Светлые тоны, убывающая светлота (subtle→base): база плотнее.
        let subtle = solve_material_alpha_hex("#E8E8EA", AA_TEXT).unwrap();
        let soft = solve_material_alpha_hex("#D8D8DC", AA_TEXT).unwrap();
        let base = solve_material_alpha_hex("#B4B4BC", AA_TEXT).unwrap();
        assert!(
            subtle.alpha < soft.alpha && soft.alpha < base.alpha,
            "порядок α нарушен: subtle {} soft {} base {}",
            subtle.alpha,
            soft.alpha,
            base.alpha
        );
    }

    /// Более высокий пол (AAA 7:1) может быть недостижим даже на α=1 (средний тон)
    /// — тогда честная деградация, а не ложное обещание.
    #[test]
    fn high_floor_degrades_honestly_on_mid_tone() {
        // Средний тон: полюс максимального контраста ≈ 4.6, ниже 7:1.
        let m = solve_material_alpha_encoded([0.42, 0.42, 0.42], &BackdropBox::FULL, 7.0).unwrap();
        assert!(
            m.degraded,
            "средний тон обязан деградировать на AAA-поле 7:1"
        );
        assert_eq!(
            m.alpha, 1.0,
            "деградация возвращает ближайшую достижимую α=1"
        );
        assert!(m.worst_contrast < 7.0);
    }

    /// Узкий коридор требует МЕНЬШЕ плотности, чем полный [чёрный, белый]:
    /// известная область фона (ADR-0004) — более щадящая гарантия.
    #[test]
    fn narrow_corridor_needs_less_alpha_than_full() {
        let tone = enc("#C8C8CE");
        let full = solve_material_alpha_encoded(tone, &BackdropBox::FULL, AA_TEXT).unwrap();
        // Узкий светлый коридор: фон только в [#C0.., #FF..].
        let narrow = BackdropBox {
            min: enc("#C0C0C0"),
            max: [1.0; 3],
        };
        let m = solve_material_alpha_encoded(tone, &narrow, AA_TEXT).unwrap();
        assert!(
            m.alpha <= full.alpha + 1e-12,
            "узкий коридор потребовал не меньше полного: {} vs {}",
            m.alpha,
            full.alpha
        );
    }

    /// Домен закреплён: мусор-входы отвергаются (молчаливый ответ был бы ложным
    /// обещанием разрешимости).
    #[test]
    fn out_of_domain_is_rejected() {
        assert!(
            solve_material_alpha_encoded([1.5, 0.0, 0.0], &BackdropBox::FULL, AA_TEXT).is_none()
        );
        assert!(
            solve_material_alpha_encoded([f64::NAN, 0.5, 0.5], &BackdropBox::FULL, AA_TEXT)
                .is_none()
        );
        assert!(solve_material_alpha_encoded([0.5; 3], &BackdropBox::FULL, 0.5).is_none());
        assert!(solve_material_alpha_encoded([0.5; 3], &BackdropBox::FULL, 25.0).is_none());
        assert!(solve_material_alpha_encoded([0.5; 3], &BackdropBox::FULL, f64::NAN).is_none());
        assert!(committed_pole_encoded([2.0, 0.0, 0.0]).is_none());
        assert!(worst_contrast_encoded([0.5; 3], 1.5, &BackdropBox::FULL, Pole::Black).is_none());
        assert!(solve_material_alpha_hex("нет", AA_TEXT).is_err());
    }
}
