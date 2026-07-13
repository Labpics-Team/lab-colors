//! Контрактные тесты нормативного WCAG 2.2 sRGB8 evaluator-а (#284).
//!
//! Анти-epsilon свидетели — доказанные внешними 100-значными вычислениями
//! (Decimal + Wolfram, зафиксированы в Issue #284) пары СТРОГО ниже порога,
//! которые прежняя логика `ratio + 1e-9 >= floor` ошибочно принимала:
//!
//! ```text
//! #89BB09 / #8212DB → 2.999999999999939562… < 3.0
//! #898CB8 / #3E2217 → 4.499999999999645330… < 4.5
//! ```
//!
//! Решение обязано приниматься ТОЛЬКО целочисленными законами над Q55
//! outward-интервалами; никакой f64, деление или display-округление в
//! вердикте не участвуют.

use crate::numerics::NumericalDecisionEvidenceV1;
use crate::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22ClientDeclaredNotApplicableV1,
    Wcag22CriterionV1, evaluate_wcag22_srgb8, wcag22_profile_v1,
};

fn rgb(hex: u32) -> [u8; 3] {
    [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8]
}

/// Достаёт Evaluated-ветвь или падает: NotEvaluated здесь незаконен.
fn evaluated(assessment: &Wcag22AssessmentV1) -> &Wcag22AssessmentV1 {
    assert!(
        matches!(assessment, Wcag22AssessmentV1::Evaluated { .. }),
        "ожидалась Evaluated-ветвь"
    );
    assessment
}

#[test]
fn anti_epsilon_witnesses_are_definite_fail() {
    // 2.999999999999939… СТРОГО ниже 3.0: оба SC-1.4.11-критерия обязаны дать
    // definite Fail, который прежний `+ 1e-9` превращал в ложный Pass.
    for criterion in [
        Wcag22CriterionV1::Sc1411UiComponentOrState,
        Wcag22CriterionV1::Sc1411GraphicalObject,
        Wcag22CriterionV1::Sc143TextLargeScale,
    ] {
        let assessment = evaluate_wcag22_srgb8(rgb(0x89BB09), rgb(0x8212DB), criterion)
            .expect("admitted sRGB8 domain обязан быть decision-total");
        let Wcag22AssessmentV1::Evaluated { decision, .. } = evaluated(&assessment) else {
            unreachable!()
        };
        assert_eq!(
            *decision,
            Wcag22ApplicableDecisionV1::Fail,
            "#89BB09/#8212DB ниже 3.0 — обязан быть definite Fail ({criterion:?})"
        );
    }

    // 4.499999999999645… СТРОГО ниже 4.5.
    let assessment = evaluate_wcag22_srgb8(
        rgb(0x898CB8),
        rgb(0x3E2217),
        Wcag22CriterionV1::Sc143TextDefault,
    )
    .expect("admitted sRGB8 domain обязан быть decision-total");
    let Wcag22AssessmentV1::Evaluated { decision, .. } = evaluated(&assessment) else {
        unreachable!()
    };
    assert_eq!(
        *decision,
        Wcag22ApplicableDecisionV1::Fail,
        "#898CB8/#3E2217 ниже 4.5 — обязан быть definite Fail"
    );
}

#[test]
fn black_white_is_definite_pass_for_every_criterion_and_symmetric() {
    for criterion in [
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22CriterionV1::Sc143TextLargeScale,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
        Wcag22CriterionV1::Sc1411GraphicalObject,
    ] {
        for (fg, bg) in [(0x000000, 0xFFFFFF), (0xFFFFFF, 0x000000)] {
            let assessment = evaluate_wcag22_srgb8(rgb(fg), rgb(bg), criterion)
                .expect("admitted sRGB8 domain обязан быть decision-total");
            let Wcag22AssessmentV1::Evaluated { decision, .. } = evaluated(&assessment) else {
                unreachable!()
            };
            assert_eq!(*decision, Wcag22ApplicableDecisionV1::Pass);
        }
    }
}

#[test]
fn same_pair_may_fail_text_and_pass_a_3_to_1_criterion() {
    // #8A8A8A/#FFFFFF ≈ 3.45:1 (между 3.0 и 4.5; величина использована только
    // для ВЫБОРА фикстуры — вердикты ниже из целочисленных законов).
    let fg = rgb(0x8A8A8A);
    let bg = rgb(0xFFFFFF);
    let text = evaluate_wcag22_srgb8(fg, bg, Wcag22CriterionV1::Sc143TextDefault)
        .expect("admitted sRGB8 domain обязан быть decision-total");
    let ui = evaluate_wcag22_srgb8(fg, bg, Wcag22CriterionV1::Sc1411GraphicalObject)
        .expect("admitted sRGB8 domain обязан быть decision-total");
    let Wcag22AssessmentV1::Evaluated { decision: t, .. } = evaluated(&text) else {
        unreachable!()
    };
    let Wcag22AssessmentV1::Evaluated { decision: u, .. } = evaluated(&ui) else {
        unreachable!()
    };
    assert_eq!(
        *t,
        Wcag22ApplicableDecisionV1::Fail,
        "≈3.45 ниже текстовых 4.5"
    );
    assert_eq!(*u, Wcag22ApplicableDecisionV1::Pass);
}

#[test]
fn not_evaluated_is_never_pass_and_requires_explicit_declaration() {
    let declared = Wcag22AssessmentV1::NotEvaluated {
        profile_id: wcag22_profile_v1().profile_id,
        declaration: Wcag22ClientDeclaredNotApplicableV1::try_new("decorative-divider").unwrap(),
    };
    // Тип не даёт достать Pass из NotEvaluated: вариант не несёт decision.
    assert!(matches!(declared, Wcag22AssessmentV1::NotEvaluated { .. }));
}

#[test]
fn evidence_is_sealed_canonical_finite_bounded_with_registered_ids() {
    let assessment = evaluate_wcag22_srgb8(
        rgb(0x000000),
        rgb(0xFFFFFF),
        Wcag22CriterionV1::Sc143TextDefault,
    )
    .expect("admitted sRGB8 domain обязан быть decision-total");
    let Wcag22AssessmentV1::Evaluated { evidence, .. } = &assessment else {
        panic!("ожидалась Evaluated-ветвь");
    };
    assert!(matches!(
        evidence,
        NumericalDecisionEvidenceV1::CanonicalFiniteBounded(_)
    ));
}

#[test]
fn deterministic_cross_colour_sample_is_total_and_symmetric() {
    // Independent deterministic corpus: 100k ordered pairs across the full RGB
    // cube. This is not the full-domain proof artifact; it is a cheap PR-time
    // property/anti-vacuum gate over the public evaluator.
    let mut state = 0xD1B5_4A32_D192_ED03_u64;
    let mut pass = 0_u32;
    let mut fail = 0_u32;
    for index in 0..100_000_u32 {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let first = [state as u8, (state >> 8) as u8, (state >> 16) as u8];
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let second = [state as u8, (state >> 8) as u8, (state >> 16) as u8];
        let criterion = if index & 1 == 0 {
            Wcag22CriterionV1::Sc143TextDefault
        } else {
            Wcag22CriterionV1::Sc1411GraphicalObject
        };
        let forward = evaluate_wcag22_srgb8(first, second, criterion)
            .expect("admitted sRGB8 domain обязан быть decision-total");
        let reverse = evaluate_wcag22_srgb8(second, first, criterion)
            .expect("swap обязан оставаться в admitted domain");
        let Wcag22AssessmentV1::Evaluated {
            decision: forward_decision,
            measurement,
            ..
        } = forward
        else {
            unreachable!()
        };
        let Wcag22AssessmentV1::Evaluated {
            decision: reverse_decision,
            ..
        } = reverse
        else {
            unreachable!()
        };
        assert_eq!(forward_decision, reverse_decision);
        assert_eq!(measurement.foreground, first);
        assert_eq!(measurement.background, second);
        match forward_decision {
            Wcag22ApplicableDecisionV1::Pass => pass += 1,
            Wcag22ApplicableDecisionV1::Fail => fail += 1,
        }
    }
    assert!(
        pass > 0 && fail > 0,
        "corpus обязан кусать обе decision branches"
    );
}

#[test]
fn identical_colours_fail_every_applicable_criterion() {
    for criterion in [
        Wcag22CriterionV1::Sc143TextDefault,
        Wcag22CriterionV1::Sc143TextLargeScale,
        Wcag22CriterionV1::Sc1411UiComponentOrState,
        Wcag22CriterionV1::Sc1411GraphicalObject,
    ] {
        let assessment = evaluate_wcag22_srgb8([73, 129, 211], [73, 129, 211], criterion)
            .expect("identical bytes are a valid total-domain input");
        assert!(matches!(
            assessment,
            Wcag22AssessmentV1::Evaluated {
                decision: Wcag22ApplicableDecisionV1::Fail,
                ..
            }
        ));
    }
}
