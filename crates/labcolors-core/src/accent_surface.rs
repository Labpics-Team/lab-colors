//! Агностичный примитив акцентных **Background**-рамп (первый вертикальный
//! под-срез EC6+EC7).
//!
//! ЗАКОН ОДНОУРОВНЕВОСТИ. Из нейтральной surface-рампы Backgrounds, акцентного
//! оттенка, темы (viewing conditions) и материала выводится акцентная
//! Background-рампа, КАЖДАЯ ступень которой сидит на ТОЙ ЖЕ перцептивной
//! светлоте (CAM16-UCS J'), что одноимённая нейтральная ступень. Светлота не
//! решается заново — она наследуется из нейтрального скелета, поэтому
//! пер-уровневые ШАГИ светлоты акцентной рампы по построению равны шагам
//! нейтрали (`property_accent_surface.rs` доказывает это на ≥20 случайных
//! оттенках в обеих темах).
//!
//! # Строгая агностичность (frozen northInvariant)
//!
//! Ноль labui-имён ролей, ноль хардкод-оттенков, ноль резерваций
//! (сентимент→оттенок) в ядре: оттенок, нейтральная рампа и тема приходят
//! ПАРАМЕТРАМИ. Порядок выходной рампы = порядок входной (иерархия
//! Primary..Tertiary — семантика ПОТРЕБИТЕЛЯ, ядро лишь сохраняет порядок).
//! Гейты `agnostic_production_surface` это стерегут.
//!
//! # Хрома — ЕДИНЫЙ закон баланса, не второй рецепт (DRY)
//!
//! Тинт красится на ФИКСИРОВАННОЙ Oklab-светлоте нейтральной ступени; хрома —
//! НЕ отдельная ручка-доля `доля × max_chroma`, а ЕДИНЫЙ агностичный
//! примитив [`crate::accent_balance::accent_balanced`]: на этой светлоте берётся
//! СТЕНА ГАМУТА (`max_chroma`). Субтильность фона — следствие
//! ГЕОМЕТРИИ: у белого/чёрного гамут узок, поэтому near-white/near-black — не
//! молчаливая доля, а честный итог закона; когда даже стена оставляет оттенок
//! ниже перцептивного пола, примитив ставит [`BalancedAccent::hue_vanished`], а
//! не гасит цвет тихо. Стена гамута В ГАМУТЕ по построению ⇒ эмиссия не клипует
//! каналы и светлота не «съезжает» — тот класс дефекта, что ловит property-тест.
//!
//! # Материал — флаг, не ветка
//!
//! Деривация одна (солид на нужной светлоте); [`SurfaceMaterial`] лишь выбирает
//! ПРЕДСТАВЛЕНИЕ. `Alpha` переиспользует обратный ход [`crate::alpha`]: композит
//! `(tint, α)` над базой темы ПОБАЙТНО равен солиду, значит J' — а с ним
//! одноуровневость — наследуется алгеброй, а не переизобретается.

use crate::accent_balance::{BalancedAccent, accent_balanced};
use crate::alpha::resolve_alpha_analog_hex;
use crate::lcs::LcsColor;
use crate::scale::jp_to_oklab_l;
use crate::spaces::vc::ViewingConditions;

/// Материал выдачи ступени акцентного фона. Флаг представления — деривация
/// светлоты от материала не зависит.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceMaterial {
    /// Непрозрачный цвет.
    Solid,
    /// Полупрозрачный тинт над базой темы; композит == солиду (см. модульную доку).
    Alpha,
}

/// Одна отрисованная ступень акцентного фона.
#[derive(Debug, Clone, PartialEq)]
pub enum AccentSurface {
    /// Непрозрачный `#RRGGBB`.
    Solid { hex: String },
    /// Тинт `#RRGGBB` + фактическая α, чей композит над базой равен солиду.
    Alpha { tint_hex: String, alpha: f64 },
}

/// Акцентный фон ОДНОЙ ступени: баланс акцента `hue_deg` на перцептивной
/// светлоте `neutral_level` (та же J') — через ЕДИНЫЙ примитив
/// [`accent_balanced`] (стена гамута [`crate::scale::max_chroma`], НЕ доля).
///
/// Зеркалит светлотную часть [`crate::accent::AccentCurve::at`], но оттенок
/// ФИКСИРОВАН (фоны не дрейфуют оттенком, в отличие от акцентных лестниц).
/// Возвращает [`BalancedAccent`], а не голый цвет: у краёв гамута оттенок может
/// физически выродиться — вызывающий видит это флагом [`BalancedAccent::hue_vanished`]
/// примитива, а не молчаливым near-white (см. модульную доку).
fn accent_level(neutral_level: &LcsColor, hue_deg: f64, vc: &ViewingConditions) -> BalancedAccent {
    // Светлота наследуется из нейтральной ступени (одноуровневость по построению).
    let l_ok = jp_to_oklab_l(neutral_level.jp, vc);
    // ЕДИНЫЙ закон баланса: стена гамута на этой светлоте — без второго рецепта доли.
    accent_balanced(l_ok, hue_deg, vc)
}

/// Вывести акцентную Background-рампу из нейтральной surface-рампы.
///
/// Для каждой нейтральной ступени — акцентный фон на ТОЙ ЖЕ перцептивной
/// светлоте; порядок сохраняется (иерархия входа). Пер-уровневые шаги светлоты
/// выхода по построению равны шагам нейтрали — ЗАКОН ОДНОУРОВНЕВОСТИ. Хрома —
/// ЕДИНЫЙ закон баланса ([`accent_balanced`]), НЕ отдельная ручка; каждый
/// [`BalancedAccent`] несёт свой `hue_vanished` (честность вырождения у краёв).
///
/// `hue_deg` — акцентный оттенок (градусы, параметр); `vc` — тема (srgb=светлая,
/// dim=тёмная).
pub fn derive_accent_surface_ramp(
    neutral: &[LcsColor],
    hue_deg: f64,
    vc: &ViewingConditions,
) -> Vec<BalancedAccent> {
    neutral
        .iter()
        .map(|n| accent_level(n, hue_deg, vc))
        .collect()
}

/// Отрисовать ступень акцентного фона в выбранном материале.
///
/// `Solid` → hex цвета через `vc` кривой. `Alpha` → альфа-аналог над `base_hex`
/// с запрошенной прозрачностью `requested_alpha`; фактическая α поднимается до
/// разрешимой в гамуте, композит остаётся ТОЧНО равным солиду (см.
/// [`crate::alpha`]). База — Background-ступень темы, над которой лежит фон.
///
/// # Errors
///
/// `Err` при невалидном `base_hex`. `Alpha` над мусор-цветом невозможен — но
/// `color`/`vc` из нашей математики, поэтому солид-hex всегда валиден.
pub fn render_surface(
    color: &LcsColor,
    material: SurfaceMaterial,
    base_hex: &str,
    requested_alpha: f64,
    vc: &ViewingConditions,
) -> Result<AccentSurface, String> {
    let solid_hex = color.to_hex_with_vc(vc);
    match material {
        SurfaceMaterial::Solid => Ok(AccentSurface::Solid { hex: solid_hex }),
        SurfaceMaterial::Alpha => {
            let (tint_hex, alpha) =
                resolve_alpha_analog_hex(&solid_hex, requested_alpha, base_hex)?;
            Ok(AccentSurface::Alpha { tint_hex, alpha })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::neutral::NeutralCurve;

    /// Нейтральная surface-рампа из шести ступеней (образец для in-crate проверок).
    fn neutral_ramp(vc: &ViewingConditions) -> Vec<LcsColor> {
        let curve = NeutralCurve::with_vc(
            "#FFFFFF",
            "#787880",
            "#101012",
            &crate::neutral::CurveParams::default(),
            vc,
        )
        .unwrap();
        [0.15, 0.30, 0.45, 0.62, 0.80, 0.95]
            .iter()
            .map(|&t| curve.at(t))
            .collect()
    }

    /// GREEN: выведенная акцентная рампа держит светлотные шаги нейтрали.
    #[test]
    fn accent_ramp_inherits_neutral_lightness_steps() {
        let vc = ViewingConditions::srgb();
        let neutral = neutral_ramp(&vc);
        let accent = derive_accent_surface_ramp(&neutral, 264.0, &vc);
        assert_eq!(accent.len(), neutral.len());
        for i in 0..neutral.len() - 1 {
            let dn = neutral[i + 1].jp - neutral[i].jp;
            let da = accent[i + 1].color.jp - accent[i].color.jp;
            assert!(
                (da - dn).abs() <= 1.0,
                "шаг {i}: акцент Δ{da:.3} против нейтрали Δ{dn:.3}"
            );
        }
    }

    /// Хрома в гамуте: эмиссия акцента не выходит за sRGB-куб на всех ступенях.
    #[test]
    fn accent_ramp_stays_in_gamut() {
        let vc = ViewingConditions::srgb();
        let neutral = neutral_ramp(&vc);
        for hue in [12.0, 140.0, 264.0] {
            let accent = derive_accent_surface_ramp(&neutral, hue, &vc);
            for c in &accent {
                let rgb = crate::spaces::srgb::srgb_from_hex(&c.color.to_hex_with_vc(&vc)).unwrap();
                assert!(
                    rgb.iter().all(|&x| (-0.01..=1.01).contains(&x)),
                    "вне гамута на hue={hue}"
                );
            }
        }
    }

    /// Материал — флаг: Alpha-аналог композитится обратно в солид (одноуровневость
    /// наследуется алгеброй альфы).
    #[test]
    fn alpha_material_composites_back_to_solid() {
        let vc = ViewingConditions::srgb();
        let neutral = neutral_ramp(&vc);
        let accent = derive_accent_surface_ramp(&neutral, 264.0, &vc);
        let base_hex = "#FFFFFF";
        for surface in &accent {
            let color = &surface.color;
            let solid_hex = color.to_hex_with_vc(&vc);
            let AccentSurface::Alpha { tint_hex, alpha } =
                render_surface(color, SurfaceMaterial::Alpha, base_hex, 0.5, &vc).unwrap()
            else {
                panic!("Alpha материал обязан дать Alpha-вариант");
            };
            let recomposed = crate::alpha::composite_hex(&tint_hex, alpha, base_hex).unwrap();
            assert_eq!(recomposed, solid_hex, "альфа-композит разошёлся с солидом");
        }
    }
}
