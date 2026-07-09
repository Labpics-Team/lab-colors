//! Акцентные поверхности на конечной sRGB8-лестнице.
//!
//! Единственный клиентский anchor задаёт обе координаты идентичности: CAM16 hue
//! и долю физического iso-HK-радиуса. Для каждого уже эмитируемого нейтрального
//! hex строится непрерывный идеал, после чего выбирается лучшая вершина содержащей
//! его sRGB8-ячейки. Поэтому одноуровневость измеряется после квантования, а
//! Oklab hue никогда не передаётся в формулу, ожидающую CAM16 hue.

use crate::alpha::resolve_alpha_analog_hex;
use crate::lcs::LcsColor;
use crate::scale::{IsoHkIdentity, iso_hk_identity_from_anchor, quantized_iso_hk_for_neutral};
use crate::spaces::oklab::srgb_linear_to_oklab;
use crate::spaces::srgb::srgb_from_hex;
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

/// Фактически эмитируемая ступень акцентной поверхности.
#[derive(Debug, Clone, PartialEq)]
pub struct AccentSurfaceLevel {
    /// Oklab L декодированного выходного hex, а не непрерывного идеала.
    pub l_ok: f64,
    /// Oklab C декодированного выходного hex.
    pub c_ok: f64,
    /// Oklab hue выходного hex; это отчётная координата, не вход CAM16-решателя.
    pub hue_deg: f64,
    /// Цвет построен повторным декодированием выбранного `#RRGGBB`.
    pub color: LcsColor,
    /// Точное состояние: эмитированный sRGB8 ахроматичен (`R = G = B`).
    /// Здесь нет универсального «порога различимости» без наблюдательских данных.
    pub hue_vanished: bool,
    /// H-K-уровень фактически эмитированной нейтральной ступени.
    pub target_hk: f64,
    /// H-K-уровень фактически выбранного акцентного hex.
    pub achieved_hk: f64,
    /// Неизменная доля физического iso-HK-радиуса клиентского anchor.
    pub chroma_ratio: f64,
}

fn accent_level(
    neutral_level: &LcsColor,
    identity: IsoHkIdentity,
    vc: &ViewingConditions,
) -> Result<AccentSurfaceLevel, String> {
    let resolved =
        quantized_iso_hk_for_neutral(neutral_level, identity.h_cam, identity.chroma_ratio, vc)
            .map_err(|error| format!("акцентный H-K-уровень недостижим: {error}"))?;
    let hex = resolved.color.to_hex_with_vc(vc);
    let rgb = srgb_from_hex(&hex)?;
    let lab = srgb_linear_to_oklab(rgb);
    let c_ok = lab[1].hypot(lab[2]);
    let hue_deg = if rgb[0] == rgb[1] && rgb[1] == rgb[2] {
        0.0
    } else {
        lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0)
    };
    Ok(AccentSurfaceLevel {
        l_ok: lab[0],
        c_ok,
        hue_deg,
        hue_vanished: rgb[0] == rgb[1] && rgb[1] == rgb[2],
        color: resolved.color,
        target_hk: resolved.target_hk,
        achieved_hk: resolved.achieved_hk,
        chroma_ratio: resolved.chroma_ratio,
    })
}

/// Вывести акцентную Background-рампу из нейтральной surface-рампы.
///
/// `anchor_hex` — единственный источник hue и относительной хромы. Функция
/// fallible, потому неверный anchor или несогласованные условия просмотра нельзя
/// превращать в частично построенную рампу.
pub fn derive_accent_surface_ramp(
    neutral: &[LcsColor],
    anchor_hex: &str,
    vc: &ViewingConditions,
) -> Result<Vec<AccentSurfaceLevel>, String> {
    let identity = iso_hk_identity_from_anchor(anchor_hex, vc)?;
    neutral
        .iter()
        .map(|level| accent_level(level, identity, vc))
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
                resolve_alpha_analog_hex(&solid_hex, requested_alpha, base_hex)?
                    .expect("солид/база из валидных hex ⇒ альфа-аналог разрешим");
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

    /// Метаданные одноуровневости пересчитываются из реально эмитированных hex.
    #[test]
    fn accent_ramp_measures_the_emitted_srgb8_levels() {
        let vc = ViewingConditions::srgb();
        let neutral = neutral_ramp(&vc);
        let accent = derive_accent_surface_ramp(&neutral, "#007AFF", &vc).unwrap();
        assert_eq!(accent.len(), neutral.len());
        for (neutral, accent) in neutral.iter().zip(&accent) {
            let neutral_hex = neutral.to_hex_with_vc(&vc);
            let accent_hex = accent.color.to_hex_with_vc(&vc);
            assert_eq!(
                accent.target_hk.to_bits(),
                crate::scale::emitted_perceived_lightness(&neutral_hex, &vc)
                    .unwrap()
                    .to_bits()
            );
            assert_eq!(
                accent.achieved_hk.to_bits(),
                crate::scale::emitted_perceived_lightness(&accent_hex, &vc)
                    .unwrap()
                    .to_bits()
            );
        }
    }

    /// Хрома в гамуте: эмиссия акцента не выходит за sRGB-куб на всех ступенях.
    #[test]
    fn accent_ramp_stays_in_gamut() {
        let vc = ViewingConditions::srgb();
        let neutral = neutral_ramp(&vc);
        for anchor in ["#FF3B30", "#34C759", "#007AFF"] {
            let accent = derive_accent_surface_ramp(&neutral, anchor, &vc).unwrap();
            for c in &accent {
                let rgb = crate::spaces::srgb::srgb_from_hex(&c.color.to_hex_with_vc(&vc)).unwrap();
                assert!(
                    rgb.iter().all(|&x| (0.0..=1.0).contains(&x)),
                    "вне гамута для anchor={anchor}"
                );
            }
        }
    }

    #[test]
    fn surface_preserves_anchor_radius_fraction_instead_of_forcing_the_wall() {
        let vc = ViewingConditions::srgb();
        let neutral = neutral_ramp(&vc);
        let saturated = derive_accent_surface_ramp(&neutral, "#FF3B30", &vc).unwrap();
        let muted = derive_accent_surface_ramp(&neutral, "#B36A65", &vc).unwrap();
        assert!(saturated[0].chroma_ratio > muted[0].chroma_ratio);
        assert!(muted[0].chroma_ratio < 1.0);
        assert!(
            muted
                .iter()
                .all(|level| level.chroma_ratio.to_bits() == muted[0].chroma_ratio.to_bits())
        );
    }

    /// Материал — флаг: Alpha-аналог композитится обратно в солид (одноуровневость
    /// наследуется алгеброй альфы).
    #[test]
    fn alpha_material_composites_back_to_solid() {
        let vc = ViewingConditions::srgb();
        let neutral = neutral_ramp(&vc);
        let accent = derive_accent_surface_ramp(&neutral, "#007AFF", &vc).unwrap();
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
