use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use labcolors_core::ViewingConditions;
use labcolors_core::glow::solve_screen_alpha_for_dj;
use std::hint::black_box;

fn bench_glow_alpha(c: &mut Criterion) {
    let vc = ViewingConditions::dim_surround();
    let mut group = c.benchmark_group("glow_alpha");
    for (name, tint, background, target) in [
        ("base", "#4A8FFF", "#101012", 2.3006),
        ("bloom", "#FF3B30", "#101012", 13.3251),
        ("unreachable", "#3E87FF", "#FFFFFF", 2.3006),
    ] {
        group.bench_with_input(
            BenchmarkId::new("solve", name),
            &(tint, background, target),
            |b, &(tint, background, target)| {
                b.iter(|| {
                    black_box(
                        solve_screen_alpha_for_dj(
                            black_box(tint),
                            black_box(background),
                            black_box(target),
                            black_box(&vc),
                        )
                        .expect("benchmark inputs are valid"),
                    )
                });
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_glow_alpha);
criterion_main!(benches);
