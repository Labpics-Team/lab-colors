//! Criterion benchmark for [`resolve_set`] — the grey-axis LUT's headline win.
//!
//! `resolve_set` runs ~13 `solve` calls per background (12 roles + the
//! max-contrast probe), each of which previously drove `match_lightness`
//! through a 64-iteration CAM16 bisection. The LUT replaces that bisection with
//! an O(1) table lookup for the neutral core, so this bench is the end-to-end
//! measure of the speed-up the chapter's "< 1 ms in WASM" exit-criterion builds
//! on (native here; the WASM measure lives in `perf-bench`).
//!
//! ## Running before / after
//!
//! ```text
//! # AFTER (LUT seed, the shipped path):
//! cargo bench -p labcolors-core --bench resolve_set
//!
//! # BEFORE (cold bisection, same call site, LUT seed disabled):
//! cargo bench -p labcolors-core --bench resolve_set --features bench-cold-bisection
//! ```
//!
//! Criterion prints the per-iteration time for each; the speed-up factor is the
//! ratio of the two medians.

use criterion::{Criterion, criterion_group, criterion_main};
use labcolors_core::{BgInput, RoleTable, ViewingConditions, resolve_set};
use std::hint::black_box;

/// Representative GREY backgrounds: white, black, and a mid grey — the extremes
/// and the middle of the grey axis. These hit the `greyfast` O(1) table.
const BACKGROUNDS: [&str; 3] = ["#FFFFFF", "#101012", "#7F7F7F"];

/// Representative CHROMATIC backgrounds — the path that does NOT hit greyfast and
/// today falls through to the full live solver (~1 ms / set, ~hundreds of CAM16
/// forwards). This is the baseline the chromatic fast path (`chromafast`) is
/// measured against: a cool blue, a warm terracotta, a green, and a saturated
/// magenta spanning the hue wheel, plus a near-neutral low-chroma tint.
const CHROMATIC_BACKGROUNDS: [&str; 5] = ["#2E6FB7", "#B5482E", "#3A8F5C", "#A23E8C", "#6E6E7A"];

fn bench_resolve_set(c: &mut Criterion) {
    let table = RoleTable::default();
    let srgb = ViewingConditions::srgb();
    let dim = ViewingConditions::dim_surround();

    let mut group = c.benchmark_group("resolve_set");
    for bg_hex in BACKGROUNDS {
        let bg = BgInput::solid(bg_hex).expect("valid bench background");
        // Light theme (sRGB VC) and dark theme (dim-surround VC) both hit a
        // precompiled table, so both exercise the LUT path when it is enabled.
        group.bench_function(format!("srgb/{bg_hex}"), |b| {
            b.iter(|| resolve_set(black_box(&bg), black_box(&table), black_box(&srgb)));
        });
        group.bench_function(format!("dim/{bg_hex}"), |b| {
            b.iter(|| resolve_set(black_box(&bg), black_box(&table), black_box(&dim)));
        });
    }
    group.finish();

    let mut chroma = c.benchmark_group("resolve_set_chromatic");
    for bg_hex in CHROMATIC_BACKGROUNDS {
        let bg = BgInput::solid(bg_hex).expect("valid chromatic bench background");
        chroma.bench_function(format!("srgb/{bg_hex}"), |b| {
            b.iter(|| resolve_set(black_box(&bg), black_box(&table), black_box(&srgb)));
        });
        chroma.bench_function(format!("dim/{bg_hex}"), |b| {
            b.iter(|| resolve_set(black_box(&bg), black_box(&table), black_box(&dim)));
        });
    }
    chroma.finish();
}

criterion_group!(benches, bench_resolve_set);
criterion_main!(benches);
