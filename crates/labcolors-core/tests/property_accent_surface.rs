//! Property-инвариант ЗАКОНА ОДНОУРОВНЕВОСТИ для акцентных Background-рамп
//! (Класс В, Фаулер): пер-уровневые ШАГИ светлоты (CAM16-UCS J') выведенной
//! акцентной рампы равны шагам нейтральной рампы на ВСЁМ пространстве оттенков в
//! ОБЕИХ темах — а не на выбранных примерах.
//!
//! # Что ловит инвариант (КЛАСС багов)
//!
//! «Акцентный фон съезжает по светлоте относительно одноимённого нейтрального» —
//! ровно инцидент, которого боится владелец: тихий дрифт полярности/деривации
//! ядра рушит контраст у labui. Наивная деривация (фиксированная хрома, клип у
//! стены гамута, или пер-уровневый светлотный офсет) роняет этот тест с
//! минимизированным контрпримером. Красящий примитив держит светлоту нейтрали
//! по построению → шаги совпадают.
//!
//! # RED-доказательство (не green-from-birth)
//!
//! `red_proof_per_level_lightness_drift_breaks_the_gate` строит НАМЕРЕННО
//! сломанную рампу (пер-уровневый дрейф светлоты) и утверждает, что детектор
//! нарушений СРАБАТЫВАЕТ — тот же идиом, что `one_levelness_tests::red_proof_*`
//! и `agnostic_cleanliness::red_proof_*`. Детектор кусается, значит зелёный на
//! реальной деривации — не театр.
//!
//! # Допуск (DECLARED)
//!
//! Шаги сравниваются по J' ЭМИТИРОВАННОГО цвета (перцептивная светлота, которой
//! оперирует весь движок). Акцент несёт хрому, и CAM16-J хроматического стимула
//! чуть отклоняется от серого на той же Oklab-светлоте — это ФИЗИЧЕСКИЙ вобл,
//! не нарушение закона. `TOL` — задекларированная полоса этого вобла, замерена
//! `measure_step_mismatch_across_hues_and_themes` (печатает фактический максимум).

use labcolors_core::accent_balance::BalancedAccent;
use labcolors_core::neutral::{CurveParams, NeutralCurve};
use labcolors_core::{LcsColor, ViewingConditions, derive_accent_surface_ramp};
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

/// Задекларированный допуск одноуровневости (J'): полоса хромового вобла CAM16-J
/// эмитированного цвета.
///
/// РЕЖИМ СТЕНЫ ГАМУТА (провенанс): после унификации деривации на единый примитив
/// баланса ([`accent_balanced`]) хрома фона = `max_chroma(L, hue)` (стена гамута),
/// а не субтильная доля. На МАКСИМАЛЬНОЙ хроме вклад Гельмгольца-Кольрауша в
/// CAM16-J максимален, поэтому вобл J' эмиссии относительно серого шире, чем под
/// прежней долей. Замер `measure_step_mismatch_across_hues_and_themes` (72 тона ×
/// 2 темы, печатает факт. максимум) держит худший |Δшаг| ≈ 1.07 J' у пурпурных
/// (hue≈305°) на мид-светлоте, где стена гамута наибольшая. TOL=1.5 — DECLARED
/// полоса с запасом над этим физическим максимумом (светлота НАСЛЕДУЕТСЯ из
/// нейтрали по построению; вобл — чисто хроматический CAM16-J, не дрейф Oklab-L).
const TOL: f64 = 1.5;

// ─────────────────────────────────────────────────────────────────────────────
// Нейтральные surface-рампы. Паттерн 2×3 (светлая: 2 осветления + база + 3
// затемнения) и 3×2 (тёмная: 3 осветления + база + 2 затемнения) — t-позиции
// документируют НАМЕРЕНИЕ иерархии; равенство шагов держится при любом наборе t.
// ─────────────────────────────────────────────────────────────────────────────

/// светлая: база=0.5; 2 ступени светлее, 3 темнее.
const LIGHT_TS: [f64; 6] = [0.18, 0.34, 0.50, 0.66, 0.80, 0.94];
/// тёмная: база=0.5; 3 ступени светлее, 2 темнее.
const DARK_TS: [f64; 6] = [0.10, 0.24, 0.38, 0.50, 0.68, 0.86];

fn neutral_ramp(vc: &ViewingConditions, ts: &[f64]) -> Vec<LcsColor> {
    // Нейтральные якоря — не brand-anchor (тест-код в любом случае вне скана
    // агностик-гейта, который стережёт только `src/`).
    let curve = NeutralCurve::with_vc("#FFFFFF", "#787880", "#101012", &CurveParams::default(), vc)
        .expect("нейтральная кривая строится");
    ts.iter().map(|&t| curve.at(t)).collect()
}

fn jps(ramp: &[LcsColor]) -> Vec<f64> {
    ramp.iter().map(|c| c.jp).collect()
}

/// J' эмитированных ступеней акцентной рампы (`BalancedAccent` несёт цвет + флаг).
fn accent_jps(ramp: &[BalancedAccent]) -> Vec<f64> {
    ramp.iter().map(|b| b.color.jp).collect()
}

/// Пер-уровневые нарушения одноуровневости: `|Δшаг акцента − Δшаг нейтрали| > tol`.
/// Пустой вектор = гейт зелёный.
fn step_violations(neutral_jp: &[f64], accent_jp: &[f64], tol: f64) -> Vec<String> {
    assert_eq!(
        neutral_jp.len(),
        accent_jp.len(),
        "рампы обязаны быть одной длины"
    );
    let mut out = Vec::new();
    for i in 0..neutral_jp.len().saturating_sub(1) {
        let dn = neutral_jp[i + 1] - neutral_jp[i];
        let da = accent_jp[i + 1] - accent_jp[i];
        if (da - dn).abs() > tol {
            out.push(format!(
                "шаг {i}: акцент Δ{da:.4} vs нейтраль Δ{dn:.4} (|Δ|={:.4} > {tol})",
                (da - dn).abs()
            ));
        }
    }
    out
}

/// Детерминированный proptest-раннер (фиксированный seed → воспроизводимо, без флака).
fn check<S: Strategy>(cases: u32, strategy: S, body: impl Fn(S::Value) -> Result<(), TestCaseError>)
where
    S::Value: std::fmt::Debug,
{
    let config = Config {
        cases,
        failure_persistence: None,
        ..Config::default()
    };
    let mut runner =
        TestRunner::new_with_rng(config, TestRng::deterministic_rng(RngAlgorithm::ChaCha));
    runner
        .run(&strategy, body)
        .expect("одноуровневость нарушена — см. минимизированный контрпример выше");
}

// ─────────────────────────────────────────────────────────────────────────────
// GREEN: реальная деривация одноуровнева с нейтралью в ОБЕИХ темах, ≥20 hue.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn accent_surface_ramp_is_one_level_with_neutral_light() {
    let vc = ViewingConditions::srgb();
    let neutral = neutral_ramp(&vc, &LIGHT_TS);
    let neutral_jp = jps(&neutral);
    check(64, 0.0f64..360.0, move |hue| {
        let accent = derive_accent_surface_ramp(&neutral, hue, &vc);
        let v = step_violations(&neutral_jp, &accent_jps(&accent), TOL);
        prop_assert!(v.is_empty(), "light hue={hue:.2}: {v:?}");
        Ok(())
    });
}

#[test]
fn accent_surface_ramp_is_one_level_with_neutral_dark() {
    let vc = ViewingConditions::dim_surround();
    let neutral = neutral_ramp(&vc, &DARK_TS);
    let neutral_jp = jps(&neutral);
    check(64, 0.0f64..360.0, move |hue| {
        let accent = derive_accent_surface_ramp(&neutral, hue, &vc);
        let v = step_violations(&neutral_jp, &accent_jps(&accent), TOL);
        prop_assert!(v.is_empty(), "dark hue={hue:.2}: {v:?}");
        Ok(())
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// RED-proof: детектор кусается на намеренно сломанной рампе.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn red_proof_per_level_lightness_drift_breaks_the_gate() {
    let vc = ViewingConditions::srgb();
    let neutral = neutral_ramp(&vc, &LIGHT_TS);
    let accent = derive_accent_surface_ramp(&neutral, 264.0, &vc);
    let n = jps(&neutral);
    let a = accent_jps(&accent);

    // Предусловие: реальная рампа зелёная (splice обязан флипать green→red).
    assert!(
        step_violations(&n, &a, TOL).is_empty(),
        "предусловие RED-proof: реальная акцентная рампа одноуровнева"
    );

    // Мутация: пер-уровневый светлотный дрейф (ступень += i×2 J') рушит шаги —
    // ровно класс «акцент съехал по светлоте», который гейт обязан ловить.
    let drifted: Vec<f64> = a
        .iter()
        .enumerate()
        .map(|(i, &jp)| jp + i as f64 * 2.0)
        .collect();
    let v = step_violations(&n, &drifted, TOL);
    assert!(
        !v.is_empty(),
        "пер-уровневый дрейф светлоты обязан уронить гейт одноуровневости"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Замер: печатает фактический максимум |Δшаг| — заземление DECLARED-допуска TOL.
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn measure_step_mismatch_across_hues_and_themes() {
    let mut worst = 0.0f64;
    let mut worst_ctx = String::new();
    for (label, vc, ts) in [
        ("light", ViewingConditions::srgb(), &LIGHT_TS),
        ("dark", ViewingConditions::dim_surround(), &DARK_TS),
    ] {
        let neutral = neutral_ramp(&vc, ts);
        let n = jps(&neutral);
        // 72 равноотстоящих оттенка, шаг 5° (хрома — стена гамута, не ручка).
        for k in 0..72 {
            let hue = k as f64 * 5.0;
            let accent = derive_accent_surface_ramp(&neutral, hue, &vc);
            let a = accent_jps(&accent);
            for i in 0..n.len() - 1 {
                let d = ((a[i + 1] - a[i]) - (n[i + 1] - n[i])).abs();
                if d > worst {
                    worst = d;
                    worst_ctx = format!("{label} hue={hue} step={i}");
                }
            }
        }
    }
    eprintln!(
        "ONE-LEVELNESS max|Δstep|={worst:.5} J' at [{worst_ctx}] (TOL={TOL}) — DECLARED-допуск хромового вобла"
    );
    assert!(
        worst <= TOL,
        "фактический вобл {worst:.5} превысил задекларированный TOL={TOL} — пересмотреть допуск"
    );
}
