//! Заземление Backgrounds-лестниц (labui ADR-0002 §1): фактические dJ'-шаги
//! HIG-фонов, замеренные движком (CAM16-UCS J' под VC темы).
//!
//! Запуск: `cargo run -p labcolors-core --example bg_ladder_anchors`
//!
//! Закон владельца (2026-07-03): светлая тема — 2 тона × 3 применения
//! (белый ↔ еле отличимый серый; elevation тенями), тёмная — 3 тона × 2
//! (тёмный → светлее → ещё светлее; elevation осветлением). Чтобы шаги
//! «еле отличимо» стали КОНТРАКТНЫМИ dJ'-ступенями, а не рукописными hex,
//! ниже печатаются J' референсных HIG-якорей — измеренная база для выбора
//! целевых dJ' в конфиге (та же методика, что у FILL_*_DJ: якорь меряется,
//! значение дж-шага становится контрактом, hex выводится солвером).

use labcolors_core::{LcsColor, ViewingConditions};

fn main() {
    // HIG systemBackground / systemGroupedBackground (iOS 17, светлая тема):
    // 2 тона: базовый белый и grouped-серый.
    let light = [
        ("white  (bg-primary/tertiary; grouped-secondary)", "#FFFFFF"),
        ("grey   (grouped-primary/tertiary; bg-secondary)", "#F2F2F7"),
    ];
    // Тёмная тема: 3 тона (elevated-набор HIG).
    let dark = [
        ("base    (bg-primary / grouped-primary)", "#000000"),
        ("base+1  (secondary)", "#1C1C1E"),
        ("base+2  (tertiary)", "#2C2C2E"),
        // Elevated-вариант базы (модальные контексты HIG) — для полноты.
        ("elevated base", "#161618"),
    ];

    let vc_light = ViewingConditions::srgb();
    let vc_dark = ViewingConditions::dim_surround();

    println!("— светлая тема (average surround) —");
    let mut prev: Option<f64> = None;
    for (label, hex) in light {
        let jp = LcsColor::from_hex_with_vc(hex, &vc_light).unwrap().jp;
        let step = prev.map(|p| jp - p).unwrap_or(0.0);
        println!("{label:48} {hex}  J' = {jp:7.3}  ΔJ' от пред. = {step:+.3}");
        prev = Some(jp);
    }

    println!("\n— тёмная тема (dim surround) —");
    let mut prev: Option<f64> = None;
    for (label, hex) in dark {
        let jp = LcsColor::from_hex_with_vc(hex, &vc_dark).unwrap().jp;
        let step = prev.map(|p| jp - p).unwrap_or(0.0);
        println!("{label:48} {hex}  J' = {jp:7.3}  ΔJ' от пред. = {step:+.3}");
        prev = Some(jp);
    }

    // Пары СОБСТВЕННОГО Figma-якоря labui (colors-stub/figma-reference.css) —
    // по ADR-0001 конфиг labui меряет свои опоры, HIG выше — методологический референс.
    println!("\n— labui-якоря, светлая —");
    let mut prev: Option<f64> = None;
    for (label, hex) in [("bg-primary", "#FFFFFF"), ("bg-secondary", "#F7F8FA")] {
        let jp = LcsColor::from_hex_with_vc(hex, &vc_light).unwrap().jp;
        let step = prev.map(|p| jp - p).unwrap_or(0.0);
        println!("{label:12} {hex}  J' = {jp:7.3}  ΔJ' = {step:+.3}");
        prev = Some(jp);
    }
    println!("— labui-якоря, тёмная —");
    let mut prev: Option<f64> = None;
    for (label, hex) in [
        ("bg-primary", "#101012"),
        ("bg-secondary", "#1C1C1E"),
        ("bg-tertiary", "#242426"),
    ] {
        let jp = LcsColor::from_hex_with_vc(hex, &vc_dark).unwrap().jp;
        let step = prev.map(|p| jp - p).unwrap_or(0.0);
        println!("{label:12} {hex}  J' = {jp:7.3}  ΔJ' = {step:+.3}");
        prev = Some(jp);
    }

    println!(
        "\nсправочно: FILL_QUATERNARY_DJ (самый тонкий существующий контрактный шаг) = 3.15 (light) / 8.22 (dark)"
    );
}
