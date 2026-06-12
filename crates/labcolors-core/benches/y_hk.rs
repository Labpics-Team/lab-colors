//! Grey-axis inverse benchmark: analytic closed-form vs bisection.
//!
//! `y_hk` is the hottest link in `resolve_set`'s finishing path — it runs once
//! per resolved role to turn a target H-K-corrected lightness back into a grey
//! luminance. The reference path is a 64-iteration bisection, each iteration a
//! full CAM16 grey-axis forward pass (`grey_j`); the production path inverts the
//! chain analytically and polishes with two Newton steps. This bench fixes the
//! speedup factor between them under both viewing conditions.
//!
//! Run: `cargo bench -p labcolors-core --bench y_hk`.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use labcolors_core::ViewingConditions;
use labcolors_core::lpc::bench_support::{y_hk_analytic, y_hk_bisect};

/// A spread of target `J_HK` values across the achromatic range, including the
/// near-white band where `J_HK > 100` and both inverses must saturate at Y = 1.
const TARGETS: [f64; 8] = [2.0, 12.5, 28.0, 45.0, 58.4, 72.0, 88.0, 102.5];

fn bench_y_hk(c: &mut Criterion) {
    let mut group = c.benchmark_group("y_hk");
    for (label, vc) in [
        ("srgb", ViewingConditions::srgb()),
        ("dim", ViewingConditions::dim_surround()),
    ] {
        group.bench_with_input(BenchmarkId::new("analytic", label), &vc, |b, vc| {
            b.iter(|| {
                for &j in &TARGETS {
                    black_box(y_hk_analytic(black_box(j), vc));
                }
            });
        });
        group.bench_with_input(BenchmarkId::new("bisect", label), &vc, |b, vc| {
            b.iter(|| {
                for &j in &TARGETS {
                    black_box(y_hk_bisect(black_box(j), vc));
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_y_hk);
criterion_main!(benches);
