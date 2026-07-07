//! Свечение — добавление СВЕТА, не наложение краски (labui ADR-0002 §5,
//! решение владельца 2026-07-03).
//!
//! # Модель
//!
//! Слой свечения — цвет G с непрозрачностью α и `mix-blend-mode: screen`
//! над непрозрачным фоном. CSS Compositing 1 (blend → simple alpha
//! compositing) в device-пространстве даёт покомпонентно:
//!
//! ```text
//! result = (1−α)·bg + α·screen(G, bg)
//!        = bg + α·G·(1−bg)          [screen(G,bg) = G + bg − G·bg]
//! ```
//!
//! Свойства по построению (не проверкой):
//! - **никогда не темнит**: α·G·(1−bg) ≥ 0 — вырождение «светлый тинт темнит
//!   светлый фон» у нормальной альфы здесь невозможно конструкцией;
//! - **монотонно и ЛИНЕЙНО по α** на канал — солвер интенсивности
//!   тривиально сходится (по перцептивной цели — бисекция, J' монотонен);
//! - асимметрия тем бесплатно: на белом (1−bg)=0 — свечение физически
//!   гаснет; на тёмном цветёт. Среда свечения — тёмная тема (закон
//!   асимметрии ADR-0002 §1).
//!
//! # Контрактные ступени (зеркальная деривация)
//!
//! **glow-k (dark) := фактический композит-шаг shadow-k (light)** — свечение
//! на тёмном обязано давать тот же перцептивный шаг |ΔJ'|, что тень на
//! светлом. Значения ИЗМЕРЕНЫ от
//! владельческих альф стека теней; отображение subtle:=minor, base:=ambient,
//! bloom:=major (penumbra — анатомическая ступень именно тени, у излучения
//! света полутени нет). Ноль придуманных чисел.
//!
//! # Анатомия (двухслойный bloom)
//!
//! core — малый радиус, светлота поднята к белому (пересвет центра — сигнатура
//! реального света); halo — большой радиус, оттенок источника на его светлоте.
//! Оттенок оба слоя наследуют от источника: свечение не имеет собственного
//! цвета.
//!
//! Центр подчинён ЕДИНОМУ ЗАКОНУ БАЛАНСА ([`crate::accent_balance`]): его
//! светлота — функциональный пол (полпути к белому по J'), а хрома на этой
//! светлоте — НЕ фикс-доля источника, а МАКСИМАЛЬНАЯ в гамуте
//! ([`crate::scale::max_chroma`]). Прежняя фикс-дельта «половина M'» размывала
//! центр к серому у пересвета (оттенок терялся); закон держит идентичность так
//! сильно, как позволяет гамут, а вырождение у стены белого честно флагуется
//! примитивом. Ноль новых констант: пол яркости — та же деривация «полпути к
//! белому», хрома — существующий `max_chroma`.

use crate::lcs::LcsColor;
use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};
use crate::spaces::vc::ViewingConditions;

/// Контрактный шаг glow-subtle (|ΔJ'| композита от фона): зеркало
/// стек-композита fx-shadow-minor на светлом якоре labui.
// SSOT-TRACKED — зеркальная деривация от владельческих альф теней.
pub const GLOW_SUBTLE_DJ: f64 = 0.8563;
/// Контрактный шаг glow-base: зеркало стек-композита fx-shadow-ambient.
// SSOT-TRACKED — зеркальная деривация от владельческих альф теней.
pub const GLOW_BASE_DJ: f64 = 2.3006;
/// Контрактный шаг glow-bloom: зеркало стек-композита fx-shadow-major.
// SSOT-TRACKED — зеркальная деривация от владельческих альф теней.
pub const GLOW_BLOOM_DJ: f64 = 13.3251;

/// Ступень контрактного стека свечения (зеркальная деривация, см. шапку).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlowStep {
    /// subtle := зеркало fx-shadow-minor.
    Subtle,
    /// base := зеркало fx-shadow-ambient.
    Base,
    /// bloom := зеркало fx-shadow-major.
    Bloom,
}

impl GlowStep {
    /// Целевой перцептивный шаг |ΔJ'| композита ступени.
    pub fn target_dj(self) -> f64 {
        match self {
            GlowStep::Subtle => GLOW_SUBTLE_DJ,
            GlowStep::Base => GLOW_BASE_DJ,
            GlowStep::Bloom => GLOW_BLOOM_DJ,
        }
    }

    /// Стабильный kebab-ключ ступени (граница конфига).
    pub fn key(self) -> &'static str {
        match self {
            GlowStep::Subtle => "subtle",
            GlowStep::Base => "base",
            GlowStep::Bloom => "bloom",
        }
    }

    /// Разбор kebab-ключа; неизвестная строка — ошибка вызывающего.
    ///
    /// # Errors
    ///
    /// `Err` с непринятой строкой.
    pub fn parse(raw: &str) -> Result<Self, String> {
        match raw {
            "subtle" => Ok(GlowStep::Subtle),
            "base" => Ok(GlowStep::Base),
            "bloom" => Ok(GlowStep::Bloom),
            other => Err(other.to_string()),
        }
    }
}

/// Screen-слой над непрозрачным фоном: `bg + α·G·(1−bg)` покомпонентно
/// (device-пространство, закон лестницы — композитит браузер).
pub fn screen_layer_over_encoded(glow: [f64; 3], alpha: f64, bg: [f64; 3]) -> [f64; 3] {
    debug_assert!((0.0..=1.0).contains(&alpha), "α вне [0,1]");
    [
        bg[0] + alpha * glow[0] * (1.0 - bg[0]),
        bg[1] + alpha * glow[1] * (1.0 - bg[1]),
        bg[2] + alpha * glow[2] * (1.0 - bg[2]),
    ]
}

/// Результат glow-солвера (ADR-0002 «честный результат», закон 2).
#[derive(Debug, Clone, PartialEq)]
pub struct GlowSolve {
    /// Интенсивность слоя `(0, 1]` — α screen-слоя.
    pub alpha: f64,
    /// Фактический |ΔJ'| композита от фона, замерен на эмитируемом hex.
    pub achieved_dj: f64,
    /// Композит `screen(tint, α)` над фоном, `#RRGGBB`.
    pub composite_hex: String,
    /// Деградация: цель недостижима даже при α = 1 (например, фон близок к
    /// белому — screen гаснет физически); возвращён ближайший достижимый
    /// результат с честным флагом, НЕ ошибка и НЕ молчание.
    pub degraded: bool,
}

/// Решить интенсивность screen-слоя под целевой перцептивный шаг.
///
/// Бисекция по α ∈ [0, 1]: J' композита монотонно растёт по α (screen
/// монотонен покомпонентно, J' монотонен по яркости стимула). Замер цели —
/// на КВАНТОВАННОМ композите (закон движка: честный шаг меряется на
/// отданном hex).
///
/// # Errors
///
/// `Err` — программный мусор (закон 3 ADR-0002): невалидный hex, цель ≤ 0.
pub fn solve_screen_alpha_for_dj(
    glow_tint_hex: &str,
    bg_hex: &str,
    target_dj: f64,
    vc: &ViewingConditions,
) -> Result<GlowSolve, String> {
    if target_dj.is_nan() || target_dj <= 0.0 {
        return Err(format!("целевой шаг вне домена: {target_dj}"));
    }
    let glow = srgb_encoded_from_hex(glow_tint_hex)?;
    let bg = srgb_encoded_from_hex(bg_hex)?;
    let bg_jp = LcsColor::from_hex_with_vc(bg_hex, vc)?.jp;

    let dj_at = |alpha: f64| -> Result<(f64, String), String> {
        let hex = hex_from_srgb_encoded(screen_layer_over_encoded(glow, alpha, bg));
        let jp = LcsColor::from_hex_with_vc(&hex, vc)?.jp;
        Ok(((jp - bg_jp).abs(), hex))
    };

    let (max_dj, max_hex) = dj_at(1.0)?;
    if max_dj < target_dj {
        // Честная деградация: ближайший достижимый шаг + флаг (закон 2).
        return Ok(GlowSolve {
            alpha: 1.0,
            achieved_dj: max_dj,
            composite_hex: max_hex,
            degraded: true,
        });
    }

    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..48 {
        let mid = 0.5 * (lo + hi);
        let (dj, _) = dj_at(mid)?;
        if dj < target_dj {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    // Верхняя сторона брекета: на 8-битной сетке шаг дискретен, и середина
    // могла бы лечь НИЖЕ цели без флага — hi по инварианту бисекции даёт
    // квантованный шаг ≥ цели (ближайший достижимый сверху, закон 2 ADR-0002).
    let alpha = hi;
    let (achieved_dj, composite_hex) = dj_at(alpha)?;
    Ok(GlowSolve {
        alpha,
        achieved_dj,
        composite_hex,
        degraded: false,
    })
}

/// Двухслойная анатомия свечения от источника: `(core_hex, halo_hex)`.
///
/// halo — сам источник (его оттенок на его светлоте); core — переэкспонирование
/// по ЗАКОНУ БАЛАНСА: светлота = функциональный пол «полпути к белому по J'»
/// (`J'core = (J'src + 100)/2`), а хрома на ней — МАКСИМАЛЬНАЯ в гамуте для
/// оттенка источника ([`crate::accent_balance::accent_balanced`]), не фикс-доля.
/// Оттенок (h_ok) — источника: свечение не имеет собственного цвета.
pub fn glow_layers_from_source(
    source_hex: &str,
    vc: &ViewingConditions,
) -> Result<(String, String), String> {
    let src = LcsColor::from_hex_with_vc(source_hex, vc)?;
    // Функциональный пол яркости центра: полпути к белому по J' (пересвет).
    let jp_core = (src.jp + 100.0) * 0.5;
    let l_core = crate::scale::jp_to_oklab_l(jp_core, vc);
    // ЗАКОН БАЛАНСА: на этой яркости — максимум хромы оттенка источника (не ×0.5,
    // размывавшая центр к серому). Идентичность держится, вырождение — флагом.
    let core = crate::accent_balance::accent_balanced(l_core, src.h_ok, vc).color;
    Ok((core.to_hex_with_vc(vc), source_hex.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Screen никогда не темнит — свойство конструкции, sweep-замок.
    #[test]
    fn screen_never_darkens_any_channel() {
        let tints = ["#3E87FF", "#FF3B30", "#FFFFFF", "#101012"];
        let bgs = ["#101012", "#1C1C1E", "#808080", "#F7F8FA", "#FFFFFF"];
        for tint_hex in tints {
            let tint = srgb_encoded_from_hex(tint_hex).unwrap();
            for bg_hex in bgs {
                let bg = srgb_encoded_from_hex(bg_hex).unwrap();
                for i in 0..=20 {
                    let a = f64::from(i) / 20.0;
                    let out = screen_layer_over_encoded(tint, a, bg);
                    for ch in 0..3 {
                        assert!(
                            out[ch] >= bg[ch] - 1e-12,
                            "screen затемнил канал {ch}: tint {tint_hex} @ {a} на {bg_hex}"
                        );
                        assert!(out[ch] <= 1.0 + 1e-12, "канал {ch} вне гамута");
                    }
                }
            }
        }
    }

    /// Солвер достигает контрактной цели на тёмном фоне (среда свечения).
    #[test]
    fn solver_hits_targets_on_dark() {
        let vc = ViewingConditions::dim_surround();
        for target in [GLOW_SUBTLE_DJ, GLOW_BASE_DJ, GLOW_BLOOM_DJ] {
            let g = solve_screen_alpha_for_dj("#3E87FF", "#101012", target, &vc).unwrap();
            assert!(!g.degraded, "цель {target} недостижима на #101012");
            // Солвер возвращает ВЕРХНЮЮ сторону брекета: достигнутое ≥ цели
            // (закон 2 ADR-0002 — ближайший достижимый СВЕРХУ), перелёт ограничен
            // шагом 8-битной сетки на тёмной базе (≲0.3 J' на канал-инкремент).
            assert!(
                g.achieved_dj >= target - 1e-9 && g.achieved_dj - target < 0.5,
                "цель {target}: достигнуто {:.4} (ожидалось [цель, цель+0.5))",
                g.achieved_dj
            );
            assert!(g.alpha > 0.0 && g.alpha <= 1.0);
        }
    }

    /// Ступени контракта строго возрастают и по цели, и по решённой α.
    #[test]
    fn glow_stack_is_strictly_progressive() {
        const { assert!(GLOW_SUBTLE_DJ < GLOW_BASE_DJ && GLOW_BASE_DJ < GLOW_BLOOM_DJ) };
        let vc = ViewingConditions::dim_surround();
        let mut prev = 0.0;
        for target in [GLOW_SUBTLE_DJ, GLOW_BASE_DJ, GLOW_BLOOM_DJ] {
            let g = solve_screen_alpha_for_dj("#FF3B30", "#101012", target, &vc).unwrap();
            assert!(
                g.alpha > prev,
                "α стека не прогрессивна: {} после {prev}",
                g.alpha
            );
            prev = g.alpha;
        }
    }

    /// На белом фоне свечение честно деградирует с флагом (физика screen),
    /// а не молчит и не ошибается — ADR-0002, закон 2.
    #[test]
    fn glow_on_white_degrades_honestly() {
        let vc = ViewingConditions::srgb();
        let g = solve_screen_alpha_for_dj("#3E87FF", "#FFFFFF", GLOW_BASE_DJ, &vc).unwrap();
        assert!(g.degraded, "на белом screen обязан гаснуть");
        assert!(g.achieved_dj < GLOW_BASE_DJ);
        assert_eq!(g.composite_hex, "#FFFFFF", "screen над белым — тождество");
    }

    /// Анатомия core по ЗАКОНУ БАЛАНСА: светлее источника (пересвет), оттенок
    /// унаследован, цвет центра ВЗЯТ ИЗ примитива баланса (не своя фикс-дельта),
    /// насыщенный источник не вырождается, и хрома центра ПО ПОСТРОЕНИЮ в гамуте
    /// (стена гамута — без тихого клипа, класс бага прежней ×0.5-доли).
    #[test]
    fn core_is_balanced_overexposed_source() {
        let vc = ViewingConditions::dim_surround();
        let (core_hex, halo_hex) = glow_layers_from_source("#FF3B30", &vc).unwrap();
        assert_eq!(halo_hex, "#FF3B30", "halo — сам источник");
        let src = LcsColor::from_hex_with_vc("#FF3B30", &vc).unwrap();
        let core = LcsColor::from_hex_with_vc(&core_hex, &vc).unwrap();
        assert!(core.jp > src.jp, "core светлее источника (пересвет)");
        let dh = (core.h_ok - src.h_ok + 180.0).rem_euclid(360.0) - 180.0;
        assert!(dh.abs() < 6.0, "оттенок унаследован: Δh = {dh:.2}°");

        // Центр ВЗЯТ из примитива баланса — доказательство wiring'а.
        let l_core = crate::scale::jp_to_oklab_l((src.jp + 100.0) * 0.5, &vc);
        let balanced = crate::accent_balance::accent_balanced(l_core, src.h_ok, &vc);
        assert_eq!(
            core_hex,
            balanced.color.to_hex_with_vc(&vc),
            "центр glow обязан быть цветом примитива баланса"
        );
        assert!(
            !balanced.hue_vanished,
            "насыщенный источник не вырождается в центре"
        );

        // Хрома баланса = стена гамута ⇒ эмиссия в гамуте, без тихого клипа
        // (прежняя ×0.5-доля могла запросить недостижимую красочность и молча
        // срезаться в to_hex). Round-trip красочности центра стабилен.
        let core_reparsed = LcsColor::from_hex_with_vc(&core_hex, &vc).unwrap();
        assert!(
            (core_reparsed.mp() - core.mp()).abs() < 0.5,
            "центр в гамуте: round-trip красочности стабилен (без тихого клипа)"
        );
    }
}
