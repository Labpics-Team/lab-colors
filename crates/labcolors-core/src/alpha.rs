//! Альфа-аналог солидного цвета: прямой и обратный ход straight-alpha
//! композита в ГАММА-КОДИРОВАННОМ sRGB.
//!
//! Пространство закона — измеренный факт, не выбор: Figma композитит на
//! канвасе попарно по кодированным каналам `c = α·t + (1−α)·b`, и ровно этот
//! путь воспроизводит все 12 семантических dJ'-якорей движка
//! (`reference/labui-figma-structure.md` §3–§4, воспроизводимо
//! `cargo run -p labcolors-core --example figma_anchor_provenance`); браузерный
//! композитинг CSS-альфы живёт в том же device-пространстве. Линейный свет
//! (внутренний `srgb_from_hex`) здесь не участвует — он для колориметрии, не
//! для наложения.
//!
//! # Зачем обратный ход
//!
//! Движок решает роли СОЛИДАМИ (контраст-корректными на данном фоне). Альфа-
//! аналог роли — пара `(tint, α)`, чей композит на том же фоне равен солиду:
//!
//! ```text
//! t = (c − (1−α)·b) / α        (по каналам, кодированные значения)
//! ```
//!
//! Композит инверсии равен солиду ПО ПОСТРОЕНИЮ, поэтому контраст, dJ' и
//! WCAG-статус альфа-аналога на каноническом фоне тождественны солиду —
//! AA-корректность наследуется алгеброй, а не проверяется заново (теорема
//! запинена тестом `inversion_identity_is_exact_on_continuous_values`).
//! На ином фоне композит другой — это и есть смысл альфы (адаптация к
//! подложке), гарантия формулируется для фона, на котором решён солид.
//!
//! # Разрешимость и границы квантования
//!
//! Тинт обязан лежать в гамуте `[0,1]³`. Поканальная алгебра нижней границы α:
//!
//! ```text
//! t ≥ 0  ⇔  α ≥ (b − c) / b        (канал с c < b; при b = 0 недостижимо, если c > 0 — но тогда c > b)
//! t ≤ 1  ⇔  α ≥ (c − b) / (1 − b)  (канал с c > b; при b = 1 симметрично)
//! ```
//!
//! [`min_alpha_encoded`] — максимум этих границ по каналам; ниже него
//! [`invert_composite_encoded`] честно возвращает `None`, а не клампит
//! (кламп молча сдвинул бы композит — подмена запрещена).
//!
//! Квантование: солид, прочитанный из 8-битного hex, несёт ошибку ≤ 0.5/255 на
//! канал; инверсия масштабирует её в 1/α раз, поэтому восстановленный тинт
//! отклоняется от истинного не более чем на `0.5/(255·α)` на канал. Граница
//! запинена тестом `quantisation_error_bound_is_honoured`; при малых α точное
//! побайтное восстановление тинта не гарантируется — гарантируется прямой ход
//! (композит канонического тинта воспроизводит опорный hex побайтно, 12/12
//! Figma-пар в `figma_neutral_ladder_pairs_roundtrip`).

use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};

/// Валидный кодированный канал/цвет: конечный и в `[0,1]` — домен всех
/// функций модуля (hex-обёртки гарантируют его по построению, byte/255).
fn is_encoded_rgb(v: [f64; 3]) -> bool {
    v.into_iter()
        .all(|x| x.is_finite() && (0.0..=1.0).contains(&x))
}

/// Прямой ход: straight-alpha композит `α·tint + (1−α)·bg` по кодированным
/// каналам — закон Figma/браузера (см. модульную документацию).
///
/// Домен: `tint`/`bg` — кодированные цвета в `[0,1]³`, `alpha ∈ [0,1]`;
/// вне домена — `debug_assert` (горячий путь чистой алгебры не платит за
/// проверки в релизе; строгие Option-контракты — у инверсии и `min_alpha`).
pub fn composite_over_encoded(tint: [f64; 3], alpha: f64, bg: [f64; 3]) -> [f64; 3] {
    debug_assert!(
        is_encoded_rgb(tint) && is_encoded_rgb(bg) && (0.0..=1.0).contains(&alpha),
        "composite_over_encoded: вход вне домена кодированного sRGB"
    );
    [
        alpha * tint[0] + (1.0 - alpha) * bg[0],
        alpha * tint[1] + (1.0 - alpha) * bg[1],
        alpha * tint[2] + (1.0 - alpha) * bg[2],
    ]
}

/// Обратный ход: тинт, чей композит с `alpha` на `bg` равен `solid`.
///
/// `None`, если вход вне домена (`solid`/`bg` не кодированные цвета `[0,1]³`
/// или не конечные), `alpha` не в `(0, 1]` (при α=0 инверсия вырождена —
/// композит не зависит от тинта), либо хотя бы один канал тинта выходит из
/// гамута `[0,1]` (α ниже [`min_alpha_encoded`]).
pub fn invert_composite_encoded(solid: [f64; 3], alpha: f64, bg: [f64; 3]) -> Option<[f64; 3]> {
    if !(is_encoded_rgb(solid) && is_encoded_rgb(bg) && alpha > 0.0 && alpha <= 1.0) {
        return None;
    }
    let mut tint = [0.0; 3];
    for c in 0..3 {
        let t = (solid[c] - (1.0 - alpha) * bg[c]) / alpha;
        // EPS: допуск на плавающую арифметику самой инверсии (не на квантование
        // входа) — значения, вышедшие за [0,1] на машинный шум, клампятся к
        // границе; настоящий выход из гамута отвергается.
        const EPS: f64 = 1e-12;
        if !(-EPS..=1.0 + EPS).contains(&t) {
            return None;
        }
        tint[c] = t.clamp(0.0, 1.0);
    }
    Some(tint)
}

/// Минимальная α, при которой инверсия `solid` над `bg` разрешима в гамуте
/// (все каналы тинта в `[0,1]`). Для `solid == bg` равна 0 (любой видимый
/// эффект отсутствует, тинт = фон при любой α).
///
/// `None` при входе вне домена (не кодированный цвет `[0,1]³` / не конечный) —
/// молчаливый ответ на мусор был бы ложным обещанием разрешимости.
pub fn min_alpha_encoded(solid: [f64; 3], bg: [f64; 3]) -> Option<f64> {
    if !is_encoded_rgb(solid) || !is_encoded_rgb(bg) {
        return None;
    }
    let mut lo = 0.0f64;
    for c in 0..3 {
        let (s, b) = (solid[c], bg[c]);
        let bound = if s < b {
            (b - s) / b // b > s ≥ 0 ⇒ b > 0, деление определено
        } else if s > b {
            (s - b) / (1.0 - b) // s > b ⇒ b < 1, деление определено
        } else {
            0.0
        };
        lo = lo.max(bound);
    }
    Some(lo.clamp(0.0, 1.0))
}

/// Hex-обёртка прямого хода: композит `tint_hex @ alpha` над `bg_hex`,
/// квантованный до 8-битного hex — цвет, который реально покажет дисплей.
///
/// # Errors
///
/// `Err` при невалидном hex на любом входе или `alpha` вне `[0,1]`/NaN —
/// публичная поверхность не пропускает мусор в release-алгебру (внутри только
/// `debug_assert`).
pub fn composite_hex(tint_hex: &str, alpha: f64, bg_hex: &str) -> Result<String, String> {
    if !(alpha.is_finite() && (0.0..=1.0).contains(&alpha)) {
        return Err(format!("alpha must be in [0,1], got {alpha}"));
    }
    let tint = srgb_encoded_from_hex(tint_hex)?;
    let bg = srgb_encoded_from_hex(bg_hex)?;
    Ok(hex_from_srgb_encoded(composite_over_encoded(
        tint, alpha, bg,
    )))
}

/// Hex-обёртка обратного хода: тинт для `solid_hex @ alpha` над `bg_hex`,
/// квантованный до hex; `Ok(None)` при неразрешимости в гамуте.
///
/// # Errors
///
/// `Err` при невалидном hex на любом входе.
pub fn invert_composite_hex(
    solid_hex: &str,
    alpha: f64,
    bg_hex: &str,
) -> Result<Option<String>, String> {
    let solid = srgb_encoded_from_hex(solid_hex)?;
    let bg = srgb_encoded_from_hex(bg_hex)?;
    Ok(invert_composite_encoded(solid, alpha, bg).map(hex_from_srgb_encoded))
}

/// Hex-обёртка [`min_alpha_encoded`].
///
/// # Errors
///
/// `Err` при невалидном hex на любом входе.
pub fn min_alpha_hex(solid_hex: &str, bg_hex: &str) -> Result<f64, String> {
    Ok(min_alpha_encoded(
        srgb_encoded_from_hex(solid_hex)?,
        srgb_encoded_from_hex(bg_hex)?,
    )
    .expect("hex-вход всегда в домене byte/255 — None недостижим по построению"))
}

/// Альфа-аналог солида: тинт + ФАКТИЧЕСКАЯ α.
///
/// Продуктовый слой поверх строгого закона: потребитель всегда получает
/// пригодный ответ, и ответ никогда не врёт о цвете.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlphaAnalog {
    /// Кодированный тинт `[0,1]³`.
    pub tint: [f64; 3],
    /// Фактическая α: запрошенная, если она разрешима, иначе минимально
    /// разрешимая (композит при ней остаётся точно равным солиду).
    pub alpha: f64,
}

/// Продуктовый резолвер: ближайший ПРИЕМЛЕМЫЙ альфа-аналог вместо отказа.
///
/// «Приблизить» можно двумя способами, и только один честен: кламп тинта при
/// запрошенной α тихо сдвинул бы композит (система соврала бы о цвете —
/// запрещённая подмена), а подъём α до [`min_alpha_encoded`] сохраняет
/// композит ПОБАЙТНО равным солиду — двигается только прозрачность, и
/// фактическая α возвращается явно ([`AlphaAnalog::alpha`]). Запрошенная α
/// клампится в `[0,1]` (α=0 вырожденна и поднимается до α_min как «слишком
/// низкая»).
///
/// `None` — только на входе вне домена (не кодированный цвет `[0,1]³` /
/// не конечный); для валидных цветов ответ существует всегда (в худшем
/// случае α=1, тинт=солид).
pub fn resolve_alpha_analog(
    solid: [f64; 3],
    requested_alpha: f64,
    bg: [f64; 3],
) -> Option<AlphaAnalog> {
    let floor = min_alpha_encoded(solid, bg)?; // None только на мусор-входах
    if !requested_alpha.is_finite() {
        return None;
    }
    let alpha = requested_alpha.clamp(0.0, 1.0).max(floor);
    // При α == floor == 0 солид равен фону: любой видимый эффект отсутствует,
    // тинт = фон (инверсия при α=0 вырожденна — отвечаем без неё).
    if alpha == 0.0 {
        return Some(AlphaAnalog { tint: bg, alpha });
    }
    let tint = invert_composite_encoded(solid, alpha, bg)
        .expect("α ≥ α_min по построению — инверсия разрешима");
    Some(AlphaAnalog { tint, alpha })
}

/// Hex-обёртка [`resolve_alpha_analog`]: `(tint_hex, фактическая α)`.
///
/// # Errors
///
/// `Err` при невалидном hex на любом входе.
pub fn resolve_alpha_analog_hex(
    solid_hex: &str,
    requested_alpha: f64,
    bg_hex: &str,
) -> Result<Option<(String, f64)>, String> {
    let solid = srgb_encoded_from_hex(solid_hex)?;
    let bg = srgb_encoded_from_hex(bg_hex)?;
    Ok(resolve_alpha_analog(solid, requested_alpha, bg)
        .map(|a| (hex_from_srgb_encoded(a.tint), a.alpha)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Живые Figma-пары нейтральной лестницы (`reference/labui-figma-structure.md`
    /// §2 — альфы и тинт, §4 — композиты; фоны Backgrounds/Neutral/Primary):
    /// (композит, α, фон). Тинт всех 12 пар — один: `#787880`.
    const TINT: &str = "#787880";
    const BG_LIGHT: &str = "#FFFFFF";
    const BG_DARK: &str = "#101012";
    const FIGMA_PAIRS: &[(&str, f64, &str)] = &[
        ("#E4E4E6", 0.20, BG_LIGHT), // Fills/Neutral/Primary light
        ("#35353A", 0.36, BG_DARK),  // Fills/Neutral/Primary dark
        ("#E9E9EB", 0.16, BG_LIGHT), // Fills/Neutral/Secondary light
        ("#313135", 0.32, BG_DARK),  // Fills/Neutral/Secondary dark
        ("#EFEFF0", 0.12, BG_LIGHT), // Fills/Neutral/Tertiary light
        ("#29292C", 0.24, BG_DARK),  // Fills/Neutral/Tertiary dark
        ("#F4F4F5", 0.08, BG_LIGHT), // Fills/Neutral/Quaternary light
        ("#212124", 0.16, BG_DARK),  // Fills/Neutral/Quaternary dark
        ("#E9E9EB", 0.16, BG_LIGHT), // Border/Neutral/Base light
        ("#252528", 0.20, BG_DARK),  // Border/Neutral/Base dark
        ("#F4F4F5", 0.08, BG_LIGHT), // Border/Neutral/Soft light
        ("#1C1C1F", 0.12, BG_DARK),  // Border/Neutral/Soft dark
    ];

    /// Прямой ход воспроизводит все 12 живых Figma-композитов побайтно —
    /// пространство закона (гамма-кодированный sRGB) подтверждено измерением,
    /// а не памятью.
    #[test]
    fn figma_neutral_ladder_pairs_roundtrip() {
        for (solid, alpha, bg) in FIGMA_PAIRS {
            let got = composite_hex(TINT, *alpha, bg).unwrap();
            assert_eq!(
                &got, solid,
                "композит {TINT}@{alpha} над {bg} разошёлся с Figma-композитом"
            );
        }
    }

    /// Обратный ход на живых парах: восстановленный тинт отклоняется от
    /// канонического не более чем на границу квантования 0.5/(255·α) на канал,
    /// а его композит воспроизводит опорный hex побайтно (что и является
    /// продуктовой гарантией: полупрозрачная пара красит ровно тот же цвет).
    #[test]
    fn inversion_recovers_figma_tint_within_quantisation_bound() {
        let true_tint = srgb_encoded_from_hex(TINT).unwrap();
        for (solid, alpha, bg) in FIGMA_PAIRS {
            let s = srgb_encoded_from_hex(solid).unwrap();
            let b = srgb_encoded_from_hex(bg).unwrap();
            let tint = invert_composite_encoded(s, *alpha, b)
                .unwrap_or_else(|| panic!("{solid}@{alpha}/{bg}: инверсия неразрешима"));
            let bound = 0.5 / (255.0 * alpha);
            for c in 0..3 {
                assert!(
                    (tint[c] - true_tint[c]).abs() <= bound + 1e-12,
                    "{solid}@{alpha}: канал {c} восстановлен с ошибкой {} > {bound}",
                    (tint[c] - true_tint[c]).abs()
                );
            }
            // Продуктовая гарантия: композит восстановленного тинта == опорный hex.
            let recomposed = hex_from_srgb_encoded(composite_over_encoded(tint, *alpha, b));
            assert_eq!(&recomposed, solid, "{solid}@{alpha}: re-композит разошёлся");
        }
    }

    /// Теорема тождества на непрерывных значениях: инверсия прямого хода
    /// возвращает исходный тинт с машинной точностью для любых α ≥ α_min —
    /// значит контраст альфа-аналога на каноническом фоне тождественен солиду
    /// по построению. Свип — детерминированная сетка кодированных значений.
    #[test]
    fn inversion_identity_is_exact_on_continuous_values() {
        let grid: Vec<f64> = (0..=10).map(|i| f64::from(i) / 10.0).collect();
        for &tr in &grid {
            for &tb in &grid {
                for &br in &grid {
                    let tint = [tr, 0.5, tb];
                    let bg = [br, 0.25, 0.9];
                    for alpha in [0.05, 0.2, 0.5, 0.85, 1.0] {
                        let solid = composite_over_encoded(tint, alpha, bg);
                        let back = invert_composite_encoded(solid, alpha, bg)
                            .expect("прямой ход всегда обратим при той же α");
                        for c in 0..3 {
                            assert!(
                                (back[c] - tint[c]).abs() < 1e-9,
                                "identity: канал {c}: {} != {}",
                                back[c],
                                tint[c]
                            );
                        }
                    }
                }
            }
        }
    }

    /// Аналитика α_min: на границе инверсия разрешима, чуть ниже — нет.
    /// Свип по сетке пар (solid, bg), исключая вырожденный случай solid == bg
    /// (там α_min = 0 и «чуть ниже» не существует).
    #[test]
    fn min_alpha_is_the_exact_feasibility_boundary() {
        let grid: Vec<f64> = (0..=8).map(|i| f64::from(i) / 8.0).collect();
        for &s in &grid {
            for &b in &grid {
                if (s - b).abs() < 1e-15 {
                    continue;
                }
                let solid = [s, 0.3, 0.7];
                let bg = [b, 0.3, 0.7];
                let a_min = min_alpha_encoded(solid, bg).expect("вход в домене");
                assert!(
                    invert_composite_encoded(solid, a_min.max(1e-9), bg).is_some(),
                    "solid={s}, bg={b}: неразрешимо на собственной α_min={a_min}"
                );
                if a_min > 1e-6 {
                    assert!(
                        invert_composite_encoded(solid, a_min * 0.999, bg).is_none(),
                        "solid={s}, bg={b}: разрешимо НИЖЕ α_min={a_min} — граница не точна"
                    );
                }
            }
        }
    }

    /// Вырожденные α честно отвергаются, кламп не подменяет ответ.
    #[test]
    fn degenerate_alpha_is_rejected_not_clamped() {
        let s = [0.5, 0.5, 0.5];
        let b = [0.9, 0.9, 0.9];
        assert!(invert_composite_encoded(s, 0.0, b).is_none());
        assert!(invert_composite_encoded(s, -0.1, b).is_none());
        assert!(invert_composite_encoded(s, 1.1, b).is_none());
        // α=1: тинт == солид, тривиально разрешимо.
        assert_eq!(invert_composite_encoded(s, 1.0, b), Some(s));
    }

    /// Продуктовый резолвер никогда не врёт о цвете: при разрешимой
    /// запрошенной α возвращает её саму; при неразрешимой — поднимает α ровно
    /// до α_min, и композит остаётся ТОЧНО равным солиду (двигается
    /// прозрачность, не цвет). Кламп-подмена тинта не происходит.
    #[test]
    fn resolver_moves_alpha_never_the_colour() {
        let grid: Vec<f64> = (0..=8).map(|i| f64::from(i) / 8.0).collect();
        for &s in &grid {
            for &b in &grid {
                let solid = [s, 0.4, 0.6];
                let bg = [b, 0.4, 0.6];
                let floor = min_alpha_encoded(solid, bg).expect("в домене");
                for requested in [0.0, 0.05, 0.3, 0.9, 1.0] {
                    let a = resolve_alpha_analog(solid, requested, bg).expect("в домене");
                    // Фактическая α: запрошенная, если разрешима, иначе ровно α_min.
                    let want = requested.max(floor);
                    assert!(
                        (a.alpha - want).abs() < 1e-12,
                        "solid={s},bg={b},req={requested}: α={} != {want}",
                        a.alpha
                    );
                    // Композит НИКОГДА не отклоняется от солида.
                    let c = if a.alpha == 0.0 {
                        bg // вырожденный случай solid==bg
                    } else {
                        composite_over_encoded(a.tint, a.alpha, bg)
                    };
                    for ch in 0..3 {
                        assert!(
                            (c[ch] - solid[ch]).abs() < 1e-9,
                            "solid={s},bg={b},req={requested}: композит уехал на канале {ch}"
                        );
                    }
                }
            }
        }
    }

    /// Публичная hex-поверхность не пропускает мусорную α в release-алгебру.
    #[test]
    fn composite_hex_rejects_out_of_range_alpha() {
        for bad in [f64::NAN, -0.1, 1.1, f64::INFINITY] {
            assert!(
                composite_hex("#787880", bad, "#FFFFFF").is_err(),
                "α={bad} обязана быть отвергнута"
            );
        }
    }

    /// Все три hex-обёртки публичной поверхности: roundtrip на живой Figma-паре,
    /// плюс честный Err (не паника) на невалидном hex — .expect в min_alpha_hex
    /// недостижим, парсинг падает раньше через `?`.
    #[test]
    fn hex_wrappers_roundtrip_and_reject_invalid_hex() {
        // Roundtrip: Fills/Neutral/Primary light (композит #E4E4E6 = #787880@0.20 над #FFFFFF).
        let tint = invert_composite_hex("#E4E4E6", 0.20, "#FFFFFF")
            .expect("валидный hex")
            .expect("α=0.20 разрешима");
        assert_eq!(composite_hex(&tint, 0.20, "#FFFFFF").unwrap(), "#E4E4E6");
        // min_alpha_hex: для равных цветов пол = 0; для контрастной пары > 0.
        assert_eq!(min_alpha_hex("#FFFFFF", "#FFFFFF").unwrap(), 0.0);
        assert!(min_alpha_hex("#101012", "#FFFFFF").unwrap() > 0.9);
        // resolve_alpha_analog_hex: неразрешимая α поднимается, композит равен солиду.
        let (tint2, actual) = resolve_alpha_analog_hex("#101012", 0.05, "#FFFFFF")
            .expect("валидный hex")
            .expect("цвета в домене");
        assert!(actual > 0.05, "α обязана подняться до разрешимой");
        assert_eq!(composite_hex(&tint2, actual, "#FFFFFF").unwrap(), "#101012");
        // Невалидный hex — Err на каждой обёртке (никаких паник).
        for f in [
            invert_composite_hex("ош", 0.5, "#FFFFFF").err(),
            min_alpha_hex("#12345", "#FFFFFF").err(),
            resolve_alpha_analog_hex("#GGGGGG", 0.5, "#FFFFFF").err(),
        ] {
            assert!(f.is_some(), "невалидный hex обязан дать Err");
        }
    }

    /// Резолвер отвергает только мусор; NaN-α — тоже мусор, не «поднять до α_min».
    #[test]
    fn resolver_rejects_only_out_of_domain() {
        let ok = [0.5, 0.5, 0.5];
        assert!(resolve_alpha_analog([1.5, 0.0, 0.0], 0.5, ok).is_none());
        assert!(resolve_alpha_analog(ok, f64::NAN, ok).is_none());
        // Запрошенная α вне [0,1] клампится, не отвергается (пригодный ответ).
        assert_eq!(
            resolve_alpha_analog(ok, 5.0, ok).map(|a| a.alpha),
            Some(1.0)
        );
    }

    /// Домен ядра закреплён: внегамутные и неконечные входы отвергаются
    /// (молчаливый ответ на мусор был бы ложным обещанием разрешимости).
    #[test]
    fn out_of_domain_inputs_are_rejected() {
        let ok = [0.5, 0.5, 0.5];
        for bad in [
            [1.5, 0.5, 0.5],
            [-0.1, 0.5, 0.5],
            [f64::NAN, 0.5, 0.5],
            [f64::INFINITY, 0.5, 0.5],
        ] {
            assert!(
                invert_composite_encoded(bad, 0.5, ok).is_none(),
                "{bad:?} как solid"
            );
            assert!(
                invert_composite_encoded(ok, 0.5, bad).is_none(),
                "{bad:?} как bg"
            );
            assert!(
                min_alpha_encoded(bad, ok).is_none(),
                "{bad:?} как solid (min_alpha)"
            );
            assert!(
                min_alpha_encoded(ok, bad).is_none(),
                "{bad:?} как bg (min_alpha)"
            );
        }
        assert!(invert_composite_encoded(ok, f64::NAN, ok).is_none());
    }

    /// Граница квантования из модульной документации подтверждается на
    /// наихудшем сдвиге: солид, смещённый на пол-кода (0.5/255), после
    /// инверсии отклоняет тинт ровно на 0.5/(255·α).
    #[test]
    fn quantisation_error_bound_is_honoured() {
        let tint = [0.4, 0.6, 0.2];
        let bg = [1.0, 1.0, 1.0];
        for alpha in [0.08, 0.2, 0.5] {
            let solid = composite_over_encoded(tint, alpha, bg);
            let shifted = [solid[0] + 0.5 / 255.0, solid[1], solid[2]];
            let back = invert_composite_encoded(shifted, alpha, bg)
                .expect("сдвиг на пол-кода не выбивает из гамута на этих значениях");
            let err = (back[0] - tint[0]).abs();
            let bound = 0.5 / (255.0 * alpha);
            assert!(
                (err - bound).abs() < 1e-9,
                "α={alpha}: ошибка {err} != границе {bound}"
            );
        }
    }
}
