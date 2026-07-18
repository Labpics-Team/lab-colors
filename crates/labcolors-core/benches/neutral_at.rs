//! Neutral grey-axis curve sampling benchmark: `NeutralCurve::at`.
//!
//! `NeutralCurve::at` is the per-step primitive of every ladder build: the
//! neutral ramp samples it directly, and `AccentCurve::at` calls it once per
//! stretch point to anchor the accent's lightness. Besides the t-dependent
//! branch gamma, each call pays for the hue-endpoint recovery (`hue_purity`,
//! one `powf` per near-achromatic anchor per hue space) — work that is
//! constant per curve. This bench times `at` across a spread of interior `t`
//! (the exact-anchor short-circuits at 0/0.5/1 are deliberately avoided) under
//! both viewing conditions, so constructor-time caching of the constant part
//! shows up here as a direct speedup.
//!
//! Run: `cargo bench -p labcolors-core --bench neutral_at`.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use labcolors_core::neutral::NeutralCurve;
use labcolors_core::{CurvePosition, ViewingConditions};

/// Interior curve positions on both branches (light `t ≤ 0.5`, dark `t > 0.5`),
/// away from the exact-anchor short-circuits at 0, 0.5 and 1.
const TARGETS: [f64; 8] = [0.05, 0.15, 0.30, 0.45, 0.55, 0.70, 0.85, 0.95];

fn bench_neutral_at(c: &mut Criterion) {
    let mut group = c.benchmark_group("neutral_at");
    let positions = TARGETS.map(|t| CurvePosition::new(t).expect("bench positions are valid"));
    for (label, vc) in [
        ("srgb", ViewingConditions::srgb()),
        ("dim", ViewingConditions::dim_surround()),
    ] {
        let curve = NeutralCurve::with_vc("#FFFFFF", "#787880", "#101012", &vc)
            .expect("the canonical neutral anchors are valid");
        group.bench_with_input(BenchmarkId::new("interior", label), &curve, |b, curve| {
            b.iter(|| {
                for &position in &positions {
                    black_box(curve.at(black_box(position)));
                }
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_neutral_at);
criterion_main!(benches);
