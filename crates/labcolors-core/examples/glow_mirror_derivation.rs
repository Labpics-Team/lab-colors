//! Зеркальная деривация контрактных шагов свечения (labui ADR-0002 §5,
//! решение владельца 2026-07-03): **glow-k (dark) := фактический
//! композит-шаг shadow-k (light)** — «свечение на тёмном обязано давать тот
//! же перцептивный шаг, что тень на светлом». Симметрия FX — деривация из
//! уже измеренных владельцем альф теней, не подгонка.
//!
//! Запуск: `cargo run -p labcolors-core --example glow_mirror_derivation`
//!
//! Стек теней компонуется честной альфа-композицией (оператор браузера,
//! как в композиционном контракте `fx_shadow_stack_composition_*`) слой за
//! слоем над светлым якорем паспорта; печатается суммарная |ΔJ'| каждого
//! уровня. Три ступени свечения из четырёх теневых: subtle := minor,
//! base := ambient, bloom := major; penumbra пропускается — это анатомическая
//! ступень именно тени (полутень), у излучения света её нет.

use labcolors_core::{BgInput, ViewingConditions, resolve_named_set};

fn main() {
    let cfg = labcolors_core::labui_reference();
    let table = cfg
        .compile_named_role_table()
        .expect("фикстура labui компилируется");
    let vc = ViewingConditions::srgb();
    let bg_hex = "#FFFFFF";
    let bg = BgInput::solid(bg_hex).expect("bg_hex — константный валидный литерал");
    let set = resolve_named_set(&bg, &table, &vc);

    let bg_jp = labcolors_core::LcsColor::from_hex_with_vc(bg_hex, &vc)
        .expect("bg_hex — константный валидный литерал")
        .jp;
    let mut state = labcolors_core::srgb_encoded_from_hex(bg_hex).expect("bg_hex валиден");

    println!("— композит-шаги стека теней на светлом якоре (зеркальные цели glow) —");
    for name in [
        "fx-shadow-minor",
        "fx-shadow-ambient",
        "fx-shadow-penumbra",
        "fx-shadow-major",
    ] {
        let (_, resolved) = set.iter().find(|(n, _)| n == name).expect("роль в наборе");
        let t = resolved.translucent().expect("тень — Translucent");
        let tint = labcolors_core::srgb_encoded_from_hex(t.tint_hex())
            .expect("tint_hex эмитирован собственным форматтером");
        state = labcolors_core::composite_over_encoded(tint, t.alpha(), state);
        let hex = labcolors_core::hex_from_srgb_encoded(state);
        let jp = labcolors_core::LcsColor::from_hex_with_vc(&hex, &vc)
            .expect("hex собственного форматтера всегда валиден")
            .jp;
        println!(
            "{name:22} стек-композит {hex}  |ΔJ'| от фона = {:.4}",
            (jp - bg_jp).abs()
        );
    }
    println!("\nотображение: glow-subtle := minor; glow-base := ambient; glow-bloom := major");

    // Решённые слои для прототипа labui (бренд тёмной темы на тёмной базе).
    let vc_dark = ViewingConditions::dim_surround();
    let brand_dark = "#4A8FFF";
    let (core, halo) = labcolors_core::glow_layers_from_source(brand_dark, &vc_dark)
        .expect("бренд-hex константен и валиден");
    println!("\n— решённые слои (бренд {brand_dark} на #101012, dim surround) —");
    println!("core (пересвет) = {core}; halo = {halo}");
    for (name, target) in [
        ("glow-subtle", labcolors_core::GLOW_SUBTLE_DJ),
        ("glow-base", labcolors_core::GLOW_BASE_DJ),
        ("glow-bloom", labcolors_core::GLOW_BLOOM_DJ),
    ] {
        let g =
            labcolors_core::solve_screen_alpha_for_dj(&halo, "#101012", target, &vc_dark).unwrap();
        println!(
            "{name:12} цель {target:7.4}  α = {:.4}  композит {}  достигнуто {:.4}{}",
            g.alpha,
            g.composite_hex,
            g.achieved_dj,
            if g.degraded {
                "  [ДЕГРАДАЦИЯ]"
            } else {
                ""
            }
        );
    }

    // ── Мост для labui-генератора (glow-волна поезда) ────────────────────────
    // Пока labui ест released-движок без kind:"glow", его generate-contract
    // эмитит glow-роли appended-блоком с ЭТИМИ решёнными значениями (провенанс:
    // этот пример, ветка поезда). При релизе 0.6.0 блок переезжает на живую
    // эмиссию — значения обязаны совпасть байт-в-байт.
    println!("\n— мост labui: fx-glow-* тёмной темы (набор фикстуры на #101012, dim) —");
    let bg_dark = BgInput::solid("#101012").expect("константный литерал");
    let set_dark = resolve_named_set(&bg_dark, &table, &vc_dark);
    for (name, resolved) in set_dark.iter() {
        if !name.starts_with("fx-glow-") {
            continue;
        }
        match resolved.glow() {
            Some(g) => println!(
                "{name:16} halo {}  core {}  α = {:.4}  |ΔJ'| = {:.4}{}",
                g.halo_hex(),
                g.core_hex(),
                g.alpha(),
                g.achieved_dj(),
                if g.degraded() {
                    "  [ДЕГРАДАЦИЯ]"
                } else {
                    ""
                }
            ),
            Option::None => println!("{name:16} не Glow-исход: {resolved:?}"),
        }
    }
    println!("\n— мост labui: fx-glow-* светлой темы (на #FFFFFF, srgb) —");
    // Свет над белым физически гаснет (screen-подъём некуда): роль может
    // решиться с деградацией или честно не решиться — печатаем как есть,
    // это и есть основание ADR-0002 §5 «светлая тема — border-reveal».
    let set_light = resolve_named_set(&bg, &table, &vc);
    for (name, resolved) in set_light.iter() {
        if !name.starts_with("fx-glow-") {
            continue;
        }
        match resolved.glow() {
            Some(g) => println!(
                "{name:16} halo {}  core {}  α = {:.4}  |ΔJ'| = {:.4}{}",
                g.halo_hex(),
                g.core_hex(),
                g.alpha(),
                g.achieved_dj(),
                if g.degraded() {
                    "  [ДЕГРАДАЦИЯ]"
                } else {
                    ""
                }
            ),
            Option::None => println!("{name:16} не решается над белым: {resolved:?}"),
        }
    }
}
