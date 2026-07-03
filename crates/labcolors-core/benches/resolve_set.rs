//! Criterion benchmark for [`resolve_set`] — the grey-axis LUT's headline win.
//!
//! `resolve_set` runs ~13 `solve` calls per background (12 roles + the
//! max-contrast probe), each of which previously drove `match_lightness`
//! through a 64-iteration CAM16 bisection. The LUT replaces that bisection with
//! an O(1) table lookup for the neutral core, so this bench is the end-to-end
//! measure of the speed-up the chapter's "< 1 ms in WASM" exit-criterion builds
//! on (native here; the WASM measure lives in `perf-bench`).
//!
//! ## WARM vs COLD — read this before trusting a number
//!
//! The `resolve_set` and `resolve_set_chromatic` groups below reuse ONE fixed
//! background across every `b.iter` iteration. The FIRST touch of a chromatic
//! background does a full live solve and primes the per-thread `chromafast` /
//! curve-plan memo; every subsequent iteration is a memo HIT. So those groups
//! measure the **warm O(1) memo**, not the solve — they under-report the true
//! cost of a never-before-seen chromatic background by ~2-3 orders of magnitude.
//! (Grey backgrounds hit the precomputed `greyfast` table and are genuinely
//! O(1); their warm number is honest.)
//!
//! The `resolve_set_cold` group is the honest chromatic cost: it feeds a fresh,
//! DISTINCT chromatic background to every timed call (generated in the untimed
//! `iter_batched` setup), so each call is a real COLD live solve that misses
//! both fast paths and exercises the `cusp_attracted_hue` → `max_chroma` hot
//! path this perf branch optimises.
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

use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use labcolors_core::{BgInput, RoleTable, ViewingConditions, resolve_set};
use std::cell::Cell;
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

/// The honest COLD chromatic path: every timed call resolves a fresh, distinct
/// chromatic background, so none of them hits the `chromafast` / curve-plan memo.
/// The background is synthesised in the untimed `iter_batched` setup from a
/// Knuth-hash of a monotonic counter (a near-bijection over the 24-bit colour
/// cube), so successive calls almost never repeat and each is a real live solve
/// through `cusp_attracted_hue` → `max_chroma`.
fn bench_resolve_set_cold(c: &mut Criterion) {
    let table = RoleTable::default();
    let srgb = ViewingConditions::srgb();
    let dim = ViewingConditions::dim_surround();

    // A distinct, reliably-chromatic background per call. The `^ 0x20` on the
    // blue channel keeps r,g,b from collapsing to a grey (which would divert to
    // the `greyfast` table instead of the live chromatic solve).
    let make_bg = |counter: &Cell<u64>| -> BgInput {
        let n = counter.get();
        counter.set(n.wrapping_add(1));
        let v = (n.wrapping_mul(2654435761) & 0x00FF_FFFF) as u32;
        let r = ((v >> 16) & 0xFF) as u8;
        let g = ((v >> 8) & 0xFF) as u8;
        let b = ((v & 0xFF) as u8) ^ 0x20;
        BgInput::solid(&format!("#{r:02X}{g:02X}{b:02X}")).expect("valid synthesised hex")
    };

    let mut group = c.benchmark_group("resolve_set_cold");
    for (label, vc) in [("srgb", &srgb), ("dim", &dim)] {
        let counter = Cell::new(0u64);
        group.bench_function(format!("{label}/chromatic_cold"), |b| {
            b.iter_batched(
                || make_bg(&counter),
                |bg| black_box(resolve_set(black_box(&bg), &table, vc)),
                BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(benches, bench_resolve_set, bench_resolve_set_cold);
criterion_main!(benches);
