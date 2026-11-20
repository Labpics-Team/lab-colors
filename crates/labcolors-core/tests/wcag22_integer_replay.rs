//! Конечные RGB8-решения сверяются с исходной дробью контраста в u128.
//! Q55-таблица независимо проверяется full-domain verifier; здесь проверяются
//! её публичное применение, обе полярности и неизменность свидетельства.

use labcolors_core::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22CriterionV1, Wcag22LuminanceBoundsQ55V1,
    evaluate_wcag22_srgb8,
};

#[test]
fn final_byte_decisions_match_unsimplified_wide_ratio() {
    // Последние четыре пары — ближайшие pass/fail witnesses обоих порогов
    // из независимого wcag22-srgb8-q55-proof-v1.json.
    let mut pairs = vec![
        ([0; 3], [255; 3]),
        ([118; 3], [255; 3]),
        ([119; 3], [255; 3]),
        ([0x89, 0xbb, 0x09], [0x82, 0x12, 0xdb]),
        ([0x32, 0xf1, 0x20], [0xbf, 0x39, 0xc2]),
        ([0x89, 0x8c, 0xb8], [0x3e, 0x22, 0x17]),
        ([0xbe, 0x64, 0xdb], [0x48, 0x0b, 0x1d]),
    ];
    // Воспроизводимый корпус без зависимости от генератора production-таблицы.
    let mut state = 0xa076_1d64_78bd_642f_u64;
    for _ in 0..16_384 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bytes = state.to_le_bytes();
        pairs.push((
            [bytes[0], bytes[1], bytes[2]],
            [bytes[3], bytes[4], bytes[5]],
        ));
    }
    let scale = u128::from(Wcag22LuminanceBoundsQ55V1::scale());
    let mut evidence_seen = None;
    let mut passed = 0;
    let mut failed = 0;
    for (foreground, background) in pairs {
        for criterion in Wcag22CriterionV1::ALL {
            let assessment = evaluate_wcag22_srgb8(foreground, background, criterion)
                .expect("every final RGB8 pair is decidable");
            let Wcag22AssessmentV1::Evaluated {
                measurement,
                decision,
                evidence,
                ..
            } = assessment
            else {
                panic!("an explicit criterion must be evaluated");
            };
            assert_eq!(measurement.foreground, foreground);
            assert_eq!(measurement.background, background);
            let a = measurement.foreground_luminance;
            let b = measurement.background_luminance;
            let light_lower = u128::from(a.lower().max(b.lower()));
            let light_upper = u128::from(a.upper().max(b.upper()));
            let dark_lower = u128::from(a.lower().min(b.lower()));
            let dark_upper = u128::from(a.upper().min(b.upper()));
            let (numerator, denominator) = match criterion {
                Wcag22CriterionV1::Sc143TextDefault => (9, 2),
                Wcag22CriterionV1::Sc143TextLargeScale
                | Wcag22CriterionV1::Sc1411UiComponentOrState
                | Wcag22CriterionV1::Sc1411GraphicalObject => (3, 1),
                _ => panic!("the oracle must explicitly cover a new criterion"),
            };
            let lower_passes =
                denominator * (20 * light_lower + scale) >= numerator * (20 * dark_upper + scale);
            let upper_fails =
                denominator * (20 * light_upper + scale) < numerator * (20 * dark_lower + scale);
            assert_ne!(lower_passes, upper_fails, "the wide oracle must decide");
            assert_eq!(decision == Wcag22ApplicableDecisionV1::Pass, lower_passes);
            passed += usize::from(lower_passes);
            failed += usize::from(upper_fails);
            assert_eq!(*evidence_seen.get_or_insert(evidence), evidence);
            let reverse = evaluate_wcag22_srgb8(background, foreground, criterion)
                .expect("the reverse pair is decidable");
            assert!(matches!(reverse, Wcag22AssessmentV1::Evaluated {
                decision: reversed, evidence: reversed_evidence, ..
            } if reversed == decision && reversed_evidence == evidence));
        }
    }
    assert!(
        passed > 0 && failed > 0,
        "both threshold outcomes are exercised"
    );
}
