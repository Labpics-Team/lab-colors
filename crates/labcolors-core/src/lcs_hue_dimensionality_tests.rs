//! Фальсификационные тесты для гипотезы issue #27.
//!
//! Предложенная там форма `h_lcs = f(h_cam)` предполагает, что одному CAM16-hue
//! соответствует один целевой Oklab-hue. Контрпример ниже держит CAM16-hue
//! одинаковым у двух хроматических стимулов, но их Oklab-hue различается больше
//! чем на 14°. Следовательно, поправка только от `h_cam` теряет как минимум ещё
//! одну координату состояния (светлоту/цветность) и не может быть SSOT-переходом.

use crate::spaces::{cam16, oklab, srgb, vc::ViewingConditions};

fn angular_distance_deg(a: f64, b: f64) -> f64 {
    ((a - b + 180.0).rem_euclid(360.0) - 180.0).abs()
}

fn oklab_chroma(rgb_linear: [f64; 3]) -> f64 {
    let lab = oklab::srgb_linear_to_oklab(rgb_linear);
    lab[1].hypot(lab[2])
}

#[test]
fn cam16_hue_alone_does_not_determine_oklab_hue() {
    let vc = ViewingConditions::srgb();

    // Первый стимул — точный 8-bit #000060.
    let dark_blue = [0.0, 0.0, srgb::srgb_gamma_inv(96.0 / 255.0)];

    // Второй стимул начинается от #8F97FF. Его encoded-R решён бисекцией так,
    // чтобы CAM16-hue совпал с #000060 при неизменных G=151/255 и B=1.
    // Это калибровочная точка, не продакшн-константа.
    let light_blue = [
        srgb::srgb_gamma_inv(0.559_909_592_465_325_3),
        srgb::srgb_gamma_inv(151.0 / 255.0),
        1.0,
    ];

    let dark_cam = cam16::forward(srgb::srgb_to_xyz(dark_blue), &vc);
    let light_cam = cam16::forward(srgb::srgb_to_xyz(light_blue), &vc);
    let dark_h_ok = oklab::oklab_hue(dark_blue);
    let light_h_ok = oklab::oklab_hue(light_blue);

    // Контрпример не опирается на atan2-шум near-neutral: обе точки явно
    // хроматические и по Oklab C, и по CAM16 M.
    assert!(oklab_chroma(dark_blue) > 0.14);
    assert!(oklab_chroma(light_blue) > 0.14);
    assert!(dark_cam.1 > 40.0);
    assert!(light_cam.1 > 40.0);

    let cam_gap = angular_distance_deg(dark_cam.2, light_cam.2);
    assert!(
        cam_gap < 1e-6,
        "калибровочная пара разошлась по CAM16-hue: {cam_gap}°"
    );

    let target_gap = angular_distance_deg(dark_h_ok, light_h_ok);
    assert!(
        target_gap > 14.0,
        "контрпример потерял разделение целевых hue: {target_gap}°"
    );

    // При одинаковом h_cam любая однозначная `f(h_cam)` обязана вернуть один
    // результат для обеих точек, тогда как фактические цели различаются >14°.
    // Значит, двухгармоническая δ(h_cam) из #27 не может одновременно сохранить
    // оба Oklab-hue независимо от качества фита коэффициентов.
}
