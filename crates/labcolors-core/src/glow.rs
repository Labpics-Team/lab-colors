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
//! # Клиентские измерения LabUI
//!
//! **glow-k (dark) := фактический композит-шаг shadow-k (light)** — свечение
//! на тёмном обязано давать тот же перцептивный шаг |ΔJ'|, что тень на
//! светлом в конкретном клиентском пресете LabUI. Значения `GLOW_*` измерены
//! от его альф стека теней; это не универсальная шкала свечения библиотеки.
//! Для другого клиента целевой |ΔJ'| передаётся прямо в
//! [`solve_screen_alpha_for_dj`].
//!
//! # Анатомия (двухслойный bloom)
//!
//! halo — большой радиус и точный цвет источника. core — малый пересвеченный
//! радиус в точном белом пределе sRGB8. Из формулы screen приращение канала по
//! `G` равно `α·(1−bg) ≥ 0`; поэтому `[255, 255, 255]` — единственный
//! универсальный покомпонентный максимум конечной сетки, а не эстетическая
//! смесь со свободным коэффициентом. Целевой |ΔJ'| задаёт не цвет core, а
//! явный солвер прозрачности.
//!
//! У точного серого источника hue отсутствует как состояние, а не хранится
//! фиктивным углом. У хроматического источника белый предел честно отмечается
//! как коллапс hue в структурированном отчёте.

use crate::lcs::LcsColor;
use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};
use crate::spaces::vc::ViewingConditions;

/// Клиентское измерение LabUI для glow-subtle: |ΔJ'| зеркала
/// fx-shadow-minor на светлом якоре этого пресета.
// Значение принадлежит пресету LabUI; универсальный API принимает явный target_dj.
pub const GLOW_SUBTLE_DJ: f64 = 0.8563;
/// Клиентское измерение LabUI для glow-base: зеркало fx-shadow-ambient.
// Значение принадлежит пресету LabUI; универсальный API принимает явный target_dj.
pub const GLOW_BASE_DJ: f64 = 2.3006;
/// Клиентское измерение LabUI для glow-bloom: зеркало fx-shadow-major.
// Значение принадлежит пресету LabUI; универсальный API принимает явный target_dj.
pub const GLOW_BLOOM_DJ: f64 = 13.3251;

/// Ступень клиентского пресета LabUI (зеркальная деривация, см. шапку).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlowStep {
    /// LabUI subtle := зеркало fx-shadow-minor.
    Subtle,
    /// LabUI base := зеркало fx-shadow-ambient.
    Base,
    /// LabUI bloom := зеркало fx-shadow-major.
    Bloom,
}

impl GlowStep {
    /// Измеренный для LabUI перцептивный шаг |ΔJ'| композита ступени.
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

/// Точный покомпонентный максимум конечной сетки sRGB8 для screen-эмиссии.
const EMISSIVE_CLIP_CORE_HEX: &str = "#FFFFFF";

/// Структурированный отчёт о двух слоях свечения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlowLayers {
    /// Пересвеченный центр: точный эмиссионный предел sRGB8.
    pub core_hex: String,
    /// Большой ореол: исходный цвет без перекодирования и дрейфа байтов.
    pub halo_hex: String,
    /// `true`, только если у источника был hue, а белый core его утратил.
    /// Для серого источника `false`: отсутствовавший hue не мог коллапсировать.
    pub hue_collapsed: bool,
}

/// Строит структурированный отчёт о слоях свечения от источника.
///
/// Core равен точному белому пределу: на конечной сетке sRGB8 он максимизирует
/// каждый множитель `G` в screen-формуле и тем самым не прячет произвольную
/// долю яркости. Требуемый |ΔJ'| решается отдельно через прозрачность.
pub fn glow_layers_report_from_source(
    source_hex: &str,
    vc: &ViewingConditions,
) -> Result<GlowLayers, String> {
    let encoded = srgb_encoded_from_hex(source_hex)?;
    // Равенство точное: все три значения получены из целых байтов делением на
    // один знаменатель. Поэтому серый код не нуждается ни в epsilon, ни в hue.
    let source_hue_deg = if encoded[0] == encoded[1] && encoded[1] == encoded[2] {
        None
    } else {
        Some(LcsColor::from_hex_with_vc(source_hex, vc)?.h_ok)
    };

    Ok(GlowLayers {
        core_hex: EMISSIVE_CLIP_CORE_HEX.to_string(),
        halo_hex: source_hex.to_string(),
        // Белый core ахроматичен по построению; коллапс возможен только тогда,
        // когда у исходного стимула действительно существовало направление hue.
        hue_collapsed: source_hue_deg.is_some(),
    })
}

/// Совместимый адаптер для производственного вызывающего кода, который
/// исторически принимает кортеж. Новый код должен брать [`GlowLayers`] через
/// [`glow_layers_report_from_source`], чтобы не терять состояние оттенка.
pub fn glow_layers_from_source(
    source_hex: &str,
    vc: &ViewingConditions,
) -> Result<(String, String), String> {
    let report = glow_layers_report_from_source(source_hex, vc)?;
    Ok((report.core_hex, report.halo_hex))
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

    /// Пересвеченный core — точный белый предел sRGB8, а не эстетическая смесь.
    /// Хроматический источник честно сообщает о потере hue в этом пределе.
    #[test]
    fn core_is_exact_emissive_endpoint_and_reports_hue_collapse() {
        let vc = ViewingConditions::dim_surround();
        let report = glow_layers_report_from_source("#FF3B30", &vc).unwrap();

        assert_eq!(report.core_hex, "#FFFFFF", "core — предел эмиссии sRGB8");
        assert_eq!(report.halo_hex, "#FF3B30", "halo — сам источник");
        assert!(
            report.hue_collapsed,
            "у хроматического источника hue исчезает в белой точке"
        );

        let compatible = glow_layers_from_source("#FF3B30", &vc).unwrap();
        assert_eq!(
            compatible,
            (report.core_hex, report.halo_hex),
            "совместимый tuple-адаптер обязан сохранять оба цвета отчёта"
        );
    }

    /// Любой точный серый код идёт по ахроматической ветви: угол hue для него
    /// не создаётся, а белый core остаётся ахроматическим на всей сетке sRGB8.
    #[test]
    fn every_gray_source_stays_achromatic_without_fictitious_hue() {
        let vc = ViewingConditions::dim_surround();

        for byte in 0_u8..=u8::MAX {
            let source_hex = format!("#{byte:02X}{byte:02X}{byte:02X}");
            let report = glow_layers_report_from_source(&source_hex, &vc).unwrap();
            let core = srgb_encoded_from_hex(&report.core_hex).unwrap();

            assert_eq!(
                report.core_hex, "#FFFFFF",
                "серый {source_hex}: неверный core"
            );
            assert_eq!(
                report.halo_hex, source_hex,
                "halo обязан сохранить источник"
            );
            assert!(
                !report.hue_collapsed,
                "серый {source_hex} не имел hue, поэтому коллапс невозможен"
            );
            assert_eq!(
                core[0], core[1],
                "серый {source_hex}: core получил цветность"
            );
            assert_eq!(
                core[1], core[2],
                "серый {source_hex}: core получил цветность"
            );
        }
    }
}
