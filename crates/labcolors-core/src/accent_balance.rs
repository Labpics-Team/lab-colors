//! Внутренний max-chroma-at-lightness recipe старого Glow-пути.
//!
//! Он строит непрерывный Oklab-кандидат на стене sRGB-гамута для переданных
//! lightness и hue. Это точная геометрическая операция, а не доказательство
//! узнаваемости или сохранения перцептивной идентичности после преобразований и
//! квантования. Публичный occurrence-контракт не экспортирует этот recipe.

use crate::lcs::LcsColor;
use crate::scale::{lcs_from_oklab_lch, max_chroma};
use crate::spaces::vc::ViewingConditions;

/// Результат применения закона баланса на требуемой светлоте.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BalancedAccent {
    /// Целевая Oklab-светлота (функциональный пол), на которой взят баланс.
    pub l_ok: f64,
    /// Взятая Oklab-хрома — РАВНА стене гамута `max_chroma(l_ok, hue_deg)`.
    pub c_ok: f64,
    /// Переданная hue-координата (градусы `[0, 360)`), скопированная без изменения.
    pub hue_deg: f64,
    /// Отрисованный цвет ([`LcsColor`] под переданными `vc`).
    pub color: LcsColor,
}

/// Закон баланса акцента: на требуемой Oklab-светлоте `target_l_ok` взять
/// МАКСИМАЛЬНУЮ хрому оттенка `hue_deg` в гамуте sRGB.
///
/// `target_l_ok` и `hue_deg` — геометрические параметры; `vc` задаёт viewing
/// conditions. Интерпретация этих координат не является частью примитива.
///
pub(crate) fn accent_balanced(
    target_l_ok: f64,
    hue_deg: f64,
    vc: &ViewingConditions,
) -> BalancedAccent {
    // ЗАКОН: максимум хромы на требуемой светлоте (не фикс-доля источника).
    let c_ok = max_chroma(target_l_ok, hue_deg);
    let color = lcs_from_oklab_lch(target_l_ok, c_ok, hue_deg, vc);
    BalancedAccent {
        l_ok: target_l_ok,
        c_ok,
        hue_deg,
        color,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ЯКОРЬ ЗАКОНА (и RED-proof primary): на ЛЮБОЙ светлоте и оттенке баланс
    /// берёт РОВНО стену гамута — не долю. Мутация примитива к фикс-дельте
    /// (`0.5 * max_chroma`, любая доля < 1) немедленно краснит это равенство.
    #[test]
    fn balanced_takes_the_gamut_wall_at_every_hue() {
        let vc = ViewingConditions::srgb();
        for l_step in 1..=9 {
            let l = f64::from(l_step) / 10.0;
            for h_step in 0..72 {
                let h = f64::from(h_step) * 5.0;
                let b = accent_balanced(l, h, &vc);
                let wall = max_chroma(l, h);
                assert!(
                    (b.c_ok - wall).abs() < 1e-12,
                    "баланс обязан брать стену гамута: L={l} h={h}° взято {} != max {wall}",
                    b.c_ok
                );
                assert!((b.l_ok - l).abs() < 1e-12 && (b.hue_deg - h).abs() < 1e-12);
                assert!(b.c_ok > 0.0, "стена гамута положительна внутри домена");
            }
        }
    }
}
