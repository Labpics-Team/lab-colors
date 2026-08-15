//! Criterion bench of the solve cache-MISS path,
//! `labcolors_core::resolve_named_set` on the real labui role table.
//! Permanent counterpart of labcolors-core's `forward` bench: guards the
//! realtime resolve budget (caller-owned final-emission bisection is a dominant term).
//!
//! The table is compiled through the SAME path `load_config` uses: the frozen
//! `labui.config.json` passport → `ConfigDto` → `ThemeConfig` → compile. So the
//! bench measures the honest live sweep a `resolveTheme` cache-MISS runs, over
//! the real 100-role labui contract — not a synthetic table.
//!
//! Two scenarios:
//! * `fixed_bg` — one background, re-resolved. After iteration 1 the process
//!   curve-plan cache is warm; this is steady-state cost.
//! * `rotating_bg` — a rotating set of realistic design-system backgrounds
//!   (glass/blur/gradient averages), each a genuine curve-plan cache-MISS. This
//!   is the realtime "background varies constantly" cost the task targets.
//!
//! Run: `cargo +1.96.0 bench -p labcolors-wasm --bench resolve`.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use labcolors_core::{BgInput, NamedRoleTable, ViewingConditions, resolve_named_set};
use labcolors_wasm::config_dto::ConfigDto;

fn labui_table() -> NamedRoleTable {
    let json = include_str!("../tests/data/labui.config.json");
    let dto: ConfigDto = serde_json::from_str(json).expect("labui passport parses");
    let cfg = labcolors_core::config::ThemeConfig::try_from(dto).expect("DTO -> ThemeConfig");
    cfg.compile_named_role_table().expect("labui compiles")
}

/// Realistic design-system backgrounds a reactive surface varies across: solid
/// greys, tinted glass averages, gradient midpoints, image-sample means. Each is
/// a distinct curve-plan cache key, so rotating through them is a cache-MISS on
/// every iteration — the realtime scenario.
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

    // Scenario A: fixed background, steady-state (WARM curve-plan cache).
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

    // Scenario B: rotating backgrounds, cache-MISS on every resolve (realtime).
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
