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

/// Representative backgrounds: white, black, and a mid grey — the extremes and
/// the middle of the grey axis the LUT is built for.
const BACKGROUNDS: [&str; 3] = ["#FFFFFF", "#101012", "#7F7F7F"];

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
}

criterion_group!(benches, bench_resolve_set);
criterion_main!(benches);
