//! Replay нейтральной оси против независимого exact-оракула (#295).
//!
//! Оракул `scripts/verify_wcag22_neutral_axis.py` пересчитывает те же множества
//! рациональной арифметикой без Q55 и без Rust-вычислителя; его закоммиченный
//! артефакт запинен здесь по SHA-256, а сами множества replay-ится через
//! ПУБЛИЧНЫЙ exact-вычислитель `evaluate_wcag22_srgb8`. Изменение соседей,
//! критерия или домена меняет множество решений и требует пересчёта оракула.

// Включение по #[path] компилирует модуль заново в этом крейте: replay
// использует только digest/to_hex, остальная поверхность здесь не нужна.
#[path = "../src/sha256.rs"]
#[allow(dead_code)]
mod fixture_sha256;

use labcolors_core::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22CriterionV1, evaluate_wcag22_srgb8,
};

const ORACLE_FIXTURE: &str = include_str!("../contracts/wcag22-neutral-axis-oracle-v1.json");

fn decision(candidate: [u8; 3], adjacent: [u8; 3], criterion: Wcag22CriterionV1) -> bool {
    match evaluate_wcag22_srgb8(candidate, adjacent, criterion) {
        Ok(Wcag22AssessmentV1::Evaluated { decision, .. }) => {
            decision == Wcag22ApplicableDecisionV1::Pass
        }
        other => panic!("evaluator must stay total on byte input: {other:?}"),
    }
}

/// Полное решение по нейтральной оси: серые `v`, проходящие КАЖДОГО соседа
/// по КАЖДОМУ заявленному критерию.
fn neutral_axis_solution(adjacent: &[u8], criteria: &[Wcag22CriterionV1]) -> Vec<u8> {
    (0_u16..=255)
        .map(|v| v as u8)
        .filter(|v| {
            adjacent.iter().all(|n| {
                criteria
                    .iter()
                    .all(|criterion| decision([*v; 3], [*n; 3], *criterion))
            })
        })
        .collect()
}

fn grey_range(start: u8, end: u8) -> Vec<u8> {
    (start..=end).collect()
}

#[test]
fn production_replay_is_bound_to_the_exact_independent_oracle_fixture() {
    assert_eq!(
        fixture_sha256::digest(ORACLE_FIXTURE.as_bytes()).to_hex(),
        "af56e71febf2994a186a7d4b1e51d5297263220f4adbe482d8c7a7f3b155f8b2"
    );
}

#[test]
fn exact_4_5_solutions_are_7_2_and_proven_zero() {
    let criterion = [Wcag22CriterionV1::Sc143TextDefault];

    let seven = neutral_axis_solution(&[0x76], &criterion);
    let mut expected_7 = grey_range(0x00, 0x04);
    expected_7.extend(grey_range(0xFE, 0xFF));
    assert_eq!(seven, expected_7);

    let two = neutral_axis_solution(&[0x00, 0xFF], &criterion);
    assert_eq!(two, vec![0x75, 0x76]);

    let zero = neutral_axis_solution(&[0x00, 0xFF, 0x76], &criterion);
    assert!(zero.is_empty());
}

#[test]
fn exact_3_to_1_solutions_are_92_and_59_for_every_declared_criterion() {
    for criterion in [
        Wcag22CriterionV1::Sc143TextLargeScale,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
        Wcag22CriterionV1::Sc1411GraphicalObject,
    ] {
        let ninety_two = neutral_axis_solution(&[0x76], &[criterion]);
        let mut expected_92 = grey_range(0x00, 0x2D);
        expected_92.extend(grey_range(0xD2, 0xFF));
        assert_eq!(ninety_two, expected_92, "{criterion:?}");

        let fifty_nine = neutral_axis_solution(&[0x00, 0xFF], &[criterion]);
        assert_eq!(fifty_nine, grey_range(0x5A, 0x94), "{criterion:?}");
    }
}
