//! Замок честности пайплайна (интеграционный, гоняется `cargo test --workspace`).
//!
//! Смысл: доказать, что конвейер «стимул → синтетический отклик → подгонка →
//! bootstrap → вердикт» ВОССТАНАВЛИВАЕТ заложенную истину, а не подтверждает
//! произвольное значение. Без этого замка assertion-free анализ мог бы всегда
//! печатать «0.30 принято» — театр. Тест синтезирует наблюдателей с ИЗВЕСТНЫМ
//! PSE и логистическим шумом и требует, чтобы оценка следовала за данными.
//!
//! Классы (Фаулер): differential/synthetic-truth (В) + integration (А).

use psychophysics::analysis::{self, Decision, Session};
use psychophysics::stimulus::{Acceptance, DesignParams, build_session, families_or_fallback};
use psychophysics::synthetic::{self, Population};

const RESAMPLES: usize = 2000; // минимум протокола §1

fn calibration_manifest(seed: u64) -> psychophysics::stimulus::Manifest {
    build_session(
        &families_or_fallback(),
        DesignParams::default(),
        Acceptance::default(),
        "#101012",
        seed,
    )
}

#[test]
fn honesty_lock_recovers_true_pse_and_accepts() {
    // Истинный популяционный PSE = 0.30 (текущее значение константы).
    let manifest = calibration_manifest(20_250_706);
    let pop = Population::calibration_default(); // pse=0.30
    let sessions = synthetic::simulate_population(&manifest, pop, 18, 0xC0FFEE);

    let boot = analysis::bootstrap_pse(&sessions, RESAMPLES, 0x1234_5678).expect("bootstrap");
    let verdict = analysis::evaluate(&boot, &Acceptance::default());

    // 1) Восстановление истины в тесном допуске.
    assert!(
        (boot.pse - 0.30).abs() < 0.02,
        "восстановленный PSE={} далёк от истинного 0.30",
        boot.pse
    );
    // 2) Истинный PSE покрыт 95% CI (корректность интервала).
    assert!(
        boot.ci_lo <= 0.30 && 0.30 <= boot.ci_hi,
        "95% CI [{}, {}] не покрывает истинный 0.30",
        boot.ci_lo,
        boot.ci_hi
    );
    // 3) При N=18 точность достигает критерия приёмки.
    assert!(boot.ci_width < 0.03, "ширина CI={} ≥ 0.03", boot.ci_width);
    // 4) Вердикт — принять (PSE в интервале И CI узкий).
    assert_eq!(verdict.decision, Decision::Accept, "{}", verdict.reason);
}

#[test]
fn pipeline_localizes_biased_population_not_sticky() {
    // Диверсия: истинный PSE смещён к 0.40. Пайплайн ОБЯЗАН уйти от 0.30 к 0.40,
    // иначе оценка «залипает» и анализ бесполезен.
    let manifest = calibration_manifest(11);
    let pop = Population {
        pse: 0.40,
        slope: -45.0,
        pse_sd: 0.02,
    };
    let sessions = synthetic::simulate_population(&manifest, pop, 18, 0xBADF00D);

    let boot = analysis::bootstrap_pse(&sessions, RESAMPLES, 99).expect("bootstrap");
    assert!(
        (boot.pse - 0.40).abs() < 0.03,
        "смещённый PSE не восстановлен: {} (истина 0.40)",
        boot.pse
    );
    assert!(boot.pse > 0.35, "оценка залипла у 0.30: {}", boot.pse);
}

#[test]
fn pipeline_escalates_on_wide_ci() {
    // Мало наблюдателей с разнесёнными PSE → широкий CI → эскалация по точности.
    let manifest = calibration_manifest(7);
    // Два контролируемых наблюдателя: PSE 0.26 и 0.34 (внутри сетки, не вырождены).
    let a = Session {
        observer: "a".to_string(),
        points: synthetic::simulate_observer(&manifest, 0.26, -45.0, 1),
    };
    let b = Session {
        observer: "b".to_string(),
        points: synthetic::simulate_observer(&manifest, 0.34, -45.0, 2),
    };

    let boot = analysis::bootstrap_pse(&[a, b], RESAMPLES, 5).expect("bootstrap");
    let verdict = analysis::evaluate(&boot, &Acceptance::default());
    assert!(
        boot.ci_width >= 0.03,
        "ожидался широкий CI, получено {}",
        boot.ci_width
    );
    assert_eq!(verdict.decision, Decision::Escalate, "{}", verdict.reason);
    assert!(!verdict.ci_ok);
}

#[test]
fn end_to_end_generate_simulate_analyze_is_reproducible() {
    // Тот же seed → идентичный вердикт: детерминизм всего конвейера.
    let run = || {
        let m = calibration_manifest(303);
        let s = synthetic::simulate_population(&m, Population::calibration_default(), 16, 42);
        let boot = analysis::bootstrap_pse(&s, RESAMPLES, 7).expect("bootstrap");
        (boot.pse, boot.ci_lo, boot.ci_hi)
    };
    assert_eq!(run(), run(), "конвейер недетерминирован при фикс. seed");
}
