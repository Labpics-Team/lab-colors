//! Агностичный примитив **БАЛАНСА АКЦЕНТА** — единый закон цветной детали
//! (директива владельца 2026-07-07).
//!
//! # Закон, который кодифицирует модуль
//!
//! У любой цветной/акцентной детали два КОНКУРИРУЮЩИХ требования:
//! **идентичность** (быть узнаваемой по своему оттенку) и **функция** (у
//! свечения — требуемая яркость центра, у текста — контраст-пол, у бейджа —
//! читаемая заливка). Наивно их мирят фикс-дельтой (например, «взять половину
//! хромы источника»), и на функциональной светлоте оттенок размывается к
//! near-white/near-black — идентичность теряется молча.
//!
//! Разрешение конфликта одно: найти светлоту `L`, удовлетворяющую ФУНКЦИИ, и на
//! ней взять **МАКСИМАЛЬНУЮ** хрому оттенка (`max_chroma` — стена гамута).
//! Тогда этот примитив отвечает на узкий геометрический вопрос: какой максимум
//! данного направления допускает sRGB на требуемой светлоте. Он не утверждает,
//! что максимум красивее или лучше сохраняет конкретный клиентский якорь.
//! Потеря оттенка фиксируется только как точное состояние выходной решётки:
//! эмитированный `#RRGGBB` стал ахроматическим (`R = G = B`). Перцептивный порог
//! без эксперимента с наблюдателями здесь не выдумывается.
//!
//! # Строгая агностичность (frozen northInvariant)
//!
//! Ноль labui-имён ролей, ноль хардкод-оттенков, ноль резерваций
//! (сентимент→оттенок): целевая светлота, оттенок и тема приходят ПАРАМЕТРАМИ.
//! Модуль не содержит порога «видимости оттенка»: такой порог был бы свойством
//! психофизического протокола, а не геометрии гамута. Флаг ниже выводится из
//! конечного sRGB8-результата без коэффициента и epsilon.

use crate::lcs::LcsColor;
use crate::scale::{lcs_from_oklab_lch, max_chroma};
use crate::spaces::vc::ViewingConditions;

/// Результат применения закона баланса на требуемой светлоте.
#[derive(Debug, Clone, PartialEq)]
pub struct BalancedAccent {
    /// Целевая Oklab-светлота (функциональный пол), на которой взят баланс.
    pub l_ok: f64,
    /// Взятая Oklab-хрома — РАВНА стене гамута `max_chroma(l_ok, hue_deg)`.
    pub c_ok: f64,
    /// Оттенок идентичности (градусы `[0, 360)`), сохранён по построению.
    pub hue_deg: f64,
    /// Отрисованный цвет ([`LcsColor`] под переданными `vc`).
    pub color: LcsColor,
    /// Точный флаг вырождения на выходе: после реальной sRGB8-квантизации цвет
    /// имеет одинаковые байты `R = G = B`, поэтому направления hue у
    /// эмитируемого значения уже нет. Это не заявление о пороге восприятия.
    pub hue_vanished: bool,
}

/// Проверяет потерю hue на том представлении, которое действительно получит
/// клиент. Сравнивать `M'` с придуманным epsilon нельзя: CAM16 может оставить
/// числовой opponent-шум у серого, тогда как равенство трёх байтов однозначно.
pub(crate) fn hue_vanished_on_srgb8(color: &LcsColor, vc: &ViewingConditions) -> bool {
    let hex = color.to_hex_with_vc(vc);
    let encoded = crate::spaces::srgb::srgb_encoded_from_hex(&hex)
        .expect("hex, эмитированный LcsColor, обязан разбираться ядром");
    encoded[0] == encoded[1] && encoded[1] == encoded[2]
}

/// Закон баланса акцента: на требуемой Oklab-светлоте `target_l_ok` взять
/// МАКСИМАЛЬНУЮ хрому оттенка `hue_deg` в гамуте sRGB.
///
/// `target_l_ok` — функциональный пол (для свечения — целевая яркость центра;
/// для текста/бейджа — светлота, дающая контраст). `hue_deg` — оттенок
/// идентичности. `vc` — тема (физическое условие просмотра, не роль). Всё —
/// параметры: ядро агностично.
///
/// Идентичность сохраняется по построению (хрома максимальна, оттенок задан);
/// вырождение у краёв гамута объявляется флагом [`BalancedAccent::hue_vanished`].
pub fn accent_balanced(target_l_ok: f64, hue_deg: f64, vc: &ViewingConditions) -> BalancedAccent {
    // ЗАКОН: максимум хромы на требуемой светлоте (не фикс-доля источника).
    let c_ok = max_chroma(target_l_ok, hue_deg);
    let color = lcs_from_oklab_lch(target_l_ok, c_ok, hue_deg, vc);
    let hue_vanished = hue_vanished_on_srgb8(&color, vc);
    BalancedAccent {
        l_ok: target_l_ok,
        c_ok,
        hue_deg,
        color,
        hue_vanished,
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

    /// Флаг обязан быть тождественен ахроматичности эмитированного sRGB8, а не
    /// зависеть от CAM16-шума или скрытого порога.
    #[test]
    fn hue_flag_equals_emitted_byte_achromaticity() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for l_step in 0..=10 {
                let l = f64::from(l_step) / 10.0;
                for h_step in 0..72 {
                    let h = f64::from(h_step) * 5.0;
                    let balanced = accent_balanced(l, h, &vc);
                    assert_eq!(
                        balanced.hue_vanished,
                        hue_vanished_on_srgb8(&balanced.color, &vc),
                        "L={l}, h={h}"
                    );
                }
            }
        }
    }

    /// В точках чёрного и белого радиус гамута равен нулю, поэтому выходной hue
    /// отсутствует точно, без понятия «почти исчез».
    #[test]
    fn flag_fires_at_exact_achromatic_endpoints() {
        let vc = ViewingConditions::srgb();
        for l in [0.0, 1.0] {
            for h_step in 0..72 {
                let h = f64::from(h_step) * 5.0;
                assert!(accent_balanced(l, h, &vc).hue_vanished, "L={l}, h={h}");
            }
        }
    }
}
