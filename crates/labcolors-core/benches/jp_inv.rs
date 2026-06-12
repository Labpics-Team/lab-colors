//! Grey-axis J' → Oklab L inverse benchmark: analytic closed-form vs bisection.
//!
//! `jp_to_oklab_l` runs once per stretch point inside `AccentCurve::at` (and the
//! future cusp-accent paths) to anchor an accent's lightness on the neutral grey
//! axis. The reference path was a 64-iteration bisection, each step a full CAM16
//! grey-axis forward pass; the production path inverts the chain analytically
//! (J' → J in closed form, J → y via `lpc::y_hk`, then the identical grey Oklab
//! transform). This bench fixes the speedup factor between the two inverses under
//! both viewing conditions, and times the end-to-end `AccentCurve::sample_hex(13)`
//! that consumes them.
//!
//! Run: `cargo bench -p labcolors-core --bench jp_inv`.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use labcolors_core::ViewingConditions;
use labcolors_core::neutral::NeutralCurve;
use labcolors_core::scale::AccentCurve;
use labcolors_core::scale::bench_support::{jp_to_oklab_l_analytic, jp_to_oklab_l_bisect};

/// A spread of target J' values across the achromatic grey range that an accent
/// stretch walks through, from near-black to near-white.
const TARGETS: [f64; 8] = [3.0, 14.0, 28.0, 42.0, 56.0, 70.0, 84.0, 96.0];

fn bench_jp_inverse(c: &mut Criterion) {
    let mut group = c.benchmark_group("jp_to_oklab_l");
    for (label, vc) in [
        ("srgb", ViewingConditions::srgb()),
        ("dim", ViewingConditions::dim_surround()),
    ] {
        group.bench_with_input(BenchmarkId::new("analytic", label), &vc, |b, vc| {
            b.iter(|| {
                for &jp in &TARGETS {
                    black_box(jp_to_oklab_l_analytic(black_box(jp), vc));
                }
            });
        });
        group.bench_with_input(BenchmarkId::new("bisect", label), &vc, |b, vc| {
            b.iter(|| {
                for &jp in &TARGETS {
                    black_box(jp_to_oklab_l_bisect(black_box(jp), vc));
                }
            });
        });
    }
    group.finish();
}

/// End-to-end accent ladder: the production consumer of `jp_to_oklab_l`. Times
/// `sample_hex(13)` so the inverse's contribution to a real accent build is
/// visible, not just the micro-benchmark of the inverse in isolation.
fn bench_accent_sample_hex(c: &mut Criterion) {
    let neutral = NeutralCurve::new("#FFFFFF", "#787880", "#101012")
        .expect("the canonical neutral anchors are valid");
    let accent = AccentCurve::new("#007AFF", &neutral).expect("#007AFF is a valid accent seed");

    c.bench_function("accent_sample_hex_13", |b| {
        b.iter(|| black_box(accent.sample_hex(black_box(13))));
    });
}

criterion_group!(benches, bench_jp_inverse, bench_accent_sample_hex);
criterion_main!(benches);
