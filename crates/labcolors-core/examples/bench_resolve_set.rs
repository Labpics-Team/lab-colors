//! Micro-benchmark for `resolve_set` under the v2 (`Curve`) chroma policy.
//!
//! Mirrors the isolated verifier's method: `--release`, native, 10+ runs,
//! report the mean wall time of a single `resolve_set` call. The grid is the
//! golden background set × both viewing conditions; the reported number is the
//! mean over all (bg, vc) cells, averaged across runs. Run with:
//!
//! ```text
//! cargo run --release --example bench_resolve_set
//! ```

use std::hint::black_box;
use std::time::Instant;

use labcolors_core::{BgInput, RoleTable, ViewingConditions, resolve_set};

const BGS: [&str; 6] = [
    "#FFFFFF", "#F2F2F7", "#7F7F7F", "#1C1C1E", "#101012", "#3478F6",
];

fn main() {
    let table = RoleTable::default(); // v2 Curve policy by default
    let vcs = [
        ("srgb", ViewingConditions::srgb()),
        ("dim", ViewingConditions::dim_surround()),
    ];
    let bgs: Vec<BgInput> = BGS
        .iter()
        .map(|h| BgInput::solid(h).expect("valid bg hex"))
        .collect();

    let cells = bgs.len() * vcs.len();

    // Warm-up: prime caches / branch predictors, exclude from timing.
    for _ in 0..50 {
        for bg in &bgs {
            for (_, vc) in &vcs {
                black_box(resolve_set(black_box(bg), black_box(&table), black_box(vc)));
            }
        }
    }

    const RUNS: usize = 30;
    let mut per_call_us: Vec<f64> = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        let start = Instant::now();
        for bg in &bgs {
            for (_, vc) in &vcs {
                black_box(resolve_set(black_box(bg), black_box(&table), black_box(vc)));
            }
        }
        let elapsed = start.elapsed();
        per_call_us.push(elapsed.as_secs_f64() * 1e6 / cells as f64);
    }

    per_call_us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean_us = per_call_us.iter().sum::<f64>() / per_call_us.len() as f64;
    let median_us = per_call_us[per_call_us.len() / 2];
    let min_us = per_call_us[0];
    let max_us = per_call_us[per_call_us.len() - 1];

    println!("resolve_set v2 (Curve) — {RUNS} runs, {cells} cells/run:");
    println!("  mean   {:.3} ms  ({mean_us:.1} us)", mean_us / 1000.0);
    println!("  median {:.3} ms  ({median_us:.1} us)", median_us / 1000.0);
    println!("  min    {:.3} ms", min_us / 1000.0);
    println!("  max    {:.3} ms", max_us / 1000.0);
}
