//! Criterion-измерение стоимости самого `labcolors_core::resolve_named_set` на
//! реальной таблице ролей labui.
//!
//! Таблица компилируется тем же путём, что использует `load_config`: паспорт
//! `labui.config.json` → `ConfigDto` → `ThemeConfig` → compile. Здесь намеренно
//! нет утверждений о попадании контрактного кэша: этот benchmark вызывает ядро
//! напрямую и измеряет работу, лежащую под кэшем. Настоящие hit/miss всей
//! WASM-границы измеряет `packages/colors/bench/wasm-boundary.bench.mjs`.
//!
//! Два сценария не маскируют повторения:
//! * `fixed_bg` — один фон принудительно решается заново на каждой итерации;
//! * `rotating_bg` — циклическая нагрузка из нескольких реальных фонов. После
//!   первого круга значения повторяются, поэтому это workload, а не «промах».
//!
//! Запуск: `cargo bench -p labcolors-wasm --bench resolve`.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use labcolors_core::{BgInput, NamedRoleTable, ViewingConditions, resolve_named_set};
use labcolors_wasm::config_dto::ConfigDto;

fn labui_table() -> NamedRoleTable {
    let json = include_str!("../tests/data/labui.config.json");
    let dto: ConfigDto = serde_json::from_str(json).expect("labui passport parses");
    let cfg = labcolors_core::config::ThemeConfig::try_from(dto).expect("DTO -> ThemeConfig");
    cfg.compile_named_role_table().expect("labui compiles")
}

/// Реальные типы подложек: нейтрали, средние glass/gradient и выборки изображения.
/// Массив конечен и цикличен; это свойство явно учитывается в интерпретации цифр.
const ROTATING_BGS: [&str; 12] = [
    "#FFFFFF", "#F7F8FA", "#EEF1F5", "#DCE3EC", "#B8C2D0", "#8A94A6", "#5C6472", "#3A3F4B",
    "#22262E", "#14171C", "#0B0D10", "#101012",
];

fn bench_resolve(c: &mut Criterion) {
    let table = labui_table();
    let vcs = [
        ("srgb", ViewingConditions::srgb()),
        ("dim", ViewingConditions::dim_surround()),
    ];

    // Фиксированный вход отделяет стоимость повторного solve от вариативности фона.
    let mut fixed = c.benchmark_group("resolve_named_set/fixed_bg");
    for (label, vc) in &vcs {
        for bg_hex in ["#FFFFFF", "#787880", "#101012"] {
            let bg = BgInput::solid(bg_hex).unwrap();
            fixed.bench_with_input(
                BenchmarkId::new(*label, bg_hex),
                &(&bg, vc),
                |b, (bg, vc)| {
                    b.iter(|| black_box(resolve_named_set(black_box(bg), &table, vc)));
                },
            );
        }
    }
    fixed.finish();

    // Циклический workload показывает цену смены входа, но не называется промахом.
    let mut rot = c.benchmark_group("resolve_named_set/rotating_bg");
    let bgs: Vec<BgInput> = ROTATING_BGS
        .iter()
        .map(|h| BgInput::solid(h).unwrap())
        .collect();
    for (label, vc) in &vcs {
        rot.bench_with_input(BenchmarkId::from_parameter(*label), vc, |b, vc| {
            let mut i = 0usize;
            b.iter(|| {
                let bg = &bgs[i % bgs.len()];
                i += 1;
                black_box(resolve_named_set(black_box(bg), &table, vc))
            });
        });
    }
    rot.finish();
}

criterion_group!(benches, bench_resolve);
criterion_main!(benches);
