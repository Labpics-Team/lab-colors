use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use labcolors_core::ViewingConditions;
use labcolors_core::glow::{GlowDecisionProfileV1, solve_screen_alpha_for_dj};
use std::hint::black_box;

fn bench_glow_alpha(c: &mut Criterion) {
    let vc = ViewingConditions::dim_surround();
    let mut group = c.benchmark_group("glow_alpha");
    for (name, tint, background, target) in [
        ("base", "#4A8FFF", "#101012", 2.3006),
        ("bloom", "#FF3B30", "#101012", 13.3251),
        // Белый фон даёт точный no-op из одного состояния,
        // а не наихудший случай перебора.
        ("exact-noop", "#3E87FF", "#FFFFFF", 2.3006),
        // Длинный поток в доказанной границе ≤ 766 состояний. Соседний модульный
        // тест закрепляет недостижимость цели 101, поэтому legacy target/max
        // обязан прочитать поток до конца; tight максимума отдельно не заявлен.
        ("long-unreachable", "#FFFFFE", "#000101", 101.0),
    ] {
        group.bench_with_input(
            BenchmarkId::new("legacy", name),
            &(tint, background, target),
            |b, &(tint, background, target)| {
                b.iter(|| {
                    black_box(
                        solve_screen_alpha_for_dj(
                            black_box(tint),
                            black_box(background),
                            black_box(target),
                            GlowDecisionProfileV1::LegacyPlatformDependentV1.execution_mode(),
                            black_box(&vc),
                        )
                        .expect("benchmark inputs are valid"),
                    )
                });
            },
        );
        group.bench_with_input(
            BenchmarkId::new("stable", name),
            &(tint, background, target),
            |b, &(tint, background, target)| {
                b.iter(|| {
                    black_box(
                        solve_screen_alpha_for_dj(
                            black_box(tint),
                            black_box(background),
                            black_box(target),
                            GlowDecisionProfileV1::StableV1.execution_mode(),
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
