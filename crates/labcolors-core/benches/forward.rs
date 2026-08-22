#![allow(deprecated)]
//! Бенчмарк прямого хода CIECAM16 / round-trip — пер-цветовой горячий путь.
//!
//! `LcsColor::from_hex_with_vc` прогоняет всю прямую цепочку
//! (`hex → sRGB → XYZ → CAT16 → adapt → CAM16 (J, M, h) → UCS`) — примитив,
//! который резолв вызывает сотни раз на набор, а пер-кадровый recheck — раз на
//! каждый передний план. Round-trip дополнительно гоняет инверсию
//! (`LcsColor::to_xyz`).
//!
//! Бенч фиксирует wall-time этого пути, чтобы у хоистинга VC-констант — выноса
//! пер-VC трансценденталей `F_L^0.25` и `(1.64 − 0.29^n)^0.73` из пер-цветового
//! тела в `ViewingConditions::build` плюс CSE `hr.cos()/hr.sin()` в инверсии —
//! была ИЗМЕРЕННАЯ дельта, а не «на глаз». Изменение байт-идентично (под гейтом
//! bit-identity оракула `cam16::forward`), так что двигается только время, не
//! выход.
//!
//! Запуск: `cargo bench -p labcolors-core --bench forward`.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use labcolors_core::LcsColor;
use labcolors_core::ViewingConditions;

/// Разброс по кругу хью + ахроматическая ось + краевые точки гамута — та же
/// сетка, что пинит bit-identity оракул прямого хода; репрезентативна для
/// кандидатов резолва.
const GRID: [&str; 12] = [
    "#000000", "#FFFFFF", "#7F7F7F", "#787880", "#101012", "#FF0000", "#00FF00", "#0000FF",
    "#FF9500", "#34C759", "#007AFF", "#C71585",
];

fn bench_forward(c: &mut Criterion) {
    let mut group = c.benchmark_group("cam16_forward");
    for (label, vc) in [
        ("srgb", ViewingConditions::srgb()),
        ("dim", ViewingConditions::dim_surround()),
    ] {
        // Только прямой ход: hex → LcsColor (прямой CIECAM16 + UCS-рескейл).
        group.bench_with_input(BenchmarkId::new("from_hex", label), &vc, |b, vc| {
            b.iter(|| {
                for hex in GRID {
                    black_box(LcsColor::from_hex_with_vc(black_box(hex), vc).unwrap());
                }
            });
        });
        // Round-trip: прямой + обратный (`to_xyz`) ход — полный цикл восприятия.
        group.bench_with_input(BenchmarkId::new("round_trip", label), &vc, |b, vc| {
            b.iter(|| {
                for hex in GRID {
                    let lcs = LcsColor::from_hex_with_vc(black_box(hex), vc).unwrap();
                    black_box(lcs.to_hex_with_vc(vc));
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_forward);
criterion_main!(benches);
