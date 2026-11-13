//! Регрессионный порог кэша свидетельства WCAG22.
//!
//! `mint_wcag22_evidence` мемоизирован через `OnceLock`; тест измеряет
//! публичный путь `evaluate_wcag22_srgb8`, в который входит кэшированный
//! mint. Без кэша стоимость mint (порядка сотен нс) выводит медиану далеко
//! за порог, поэтому тест ловит регрессию мемоизации, не привязываясь к
//! приватной функции.
//!
//! Тест игнорируется по умолчанию: абсолютные наносекунды чувствительны
//! к загрузке машины. Запуск:
//! `cargo test -p labcolors-core --release --test profile_regression -- --ignored`

use labcolors_core::wcag22::{Wcag22CriterionV1, evaluate_wcag22_srgb8};
use std::hint::black_box;
use std::time::Instant;

const WARMUP: usize = 10_000;
const BATCHES: usize = 2_000;
const PER_BATCH: usize = 1_000;
const MEDIAN_BUDGET_NS: f64 = 30.0;

#[test]
#[ignore = "timing-sensitive; run explicitly on an idle machine in release"]
fn cached_evidence_path_stays_fast() {
    let criterion = Wcag22CriterionV1::Sc1411UiComponentOrState;

    for index in 0..WARMUP {
        let foreground = [255 - (index % 2) as u8, 255, 255];
        black_box(
            evaluate_wcag22_srgb8(black_box([20, 20, 20]), black_box(foreground), criterion)
                .unwrap(),
        );
    }

    let mut samples = Vec::with_capacity(BATCHES);
    for batch in 0..BATCHES {
        let started = Instant::now();
        for index in 0..PER_BATCH {
            let foreground = [255 - ((batch + index) % 2) as u8, 255, 255];
            black_box(
                evaluate_wcag22_srgb8([20, 20, 20], foreground, criterion)
                    .expect("evaluation must succeed"),
            );
        }
        samples.push(started.elapsed().as_nanos() as f64 / PER_BATCH as f64);
    }

    samples.sort_by(|a, b| a.partial_cmp(b).expect("finite timings"));
    let median = samples[samples.len() / 2];
    println!(
        "profile_regression: median {median:.2} ns/eval over {} isolated evaluate_wcag22_srgb8 calls",
        BATCHES * PER_BATCH
    );
    assert!(
        median < MEDIAN_BUDGET_NS,
        "cached WCAG22 evidence path regressed: median {median:.2} ns/eval (budget {MEDIAN_BUDGET_NS})"
    );
}
