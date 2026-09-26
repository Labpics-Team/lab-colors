//! Целочисленный контраст: произвольные интервалы и значения внутри них.
//! Это не доказательство таблицы яркости: её независимо проверяет Q55-verifier.
use super::*;

fn criterion() -> Wcag22CriterionV1 {
    match kani::any::<u8>() % 4 {
        0 => Wcag22CriterionV1::Sc143TextDefault,
        1 => Wcag22CriterionV1::Sc143TextLargeScale,
        2 => Wcag22CriterionV1::Sc1411UiComponentOrState,
        _ => Wcag22CriterionV1::Sc1411GraphicalObject,
    }
}

fn direct_ratio_pass(a: u64, b: u64, criterion: Wcag22CriterionV1) -> bool {
    let scale = super::super::q55_data::Q55_SCALE;
    let light = a.max(b);
    let dark = a.min(b);
    // Независимая исходная дробь с 0.05, до алгебраического сокращения kernel.
    // Константы остаются явными для решателя, не превращаются в умножение
    // двух символических машинных слов после объединения ветвей.
    match criterion {
        Wcag22CriterionV1::Sc143TextDefault => 2 * (20 * light + scale) >= 9 * (20 * dark + scale),
        Wcag22CriterionV1::Sc143TextLargeScale
        | Wcag22CriterionV1::Sc1411UiComponentOrState
        | Wcag22CriterionV1::Sc1411GraphicalObject => 20 * light + scale >= 3 * (20 * dark + scale),
    }
}

#[kani::proof]
fn exact_points_are_total_symmetric_and_correct() {
    let a: u64 = kani::any();
    let b: u64 = kani::any();
    let limit = super::super::q55_data::Q55_SCALE + 3;
    if a > limit || b > limit {
        return;
    }
    let criterion = criterion();
    let left = Wcag22LuminanceBoundsQ55V1 { lower: a, upper: a };
    let right = Wcag22LuminanceBoundsQ55V1 { lower: b, upper: b };
    let expected = if direct_ratio_pass(a, b, criterion) {
        Wcag22ApplicableDecisionV1::Pass
    } else {
        Wcag22ApplicableDecisionV1::Fail
    };
    let result = classify_pair(left, right, criterion);
    assert!(
        result == Some(expected),
        "exact Q55 points must match the unsimplified contrast ratio"
    );
    assert!(
        classify_pair(right, left, criterion) == result,
        "contrast decision must be independent of polarity"
    );
    kani::cover!(
        result == Some(Wcag22ApplicableDecisionV1::Pass),
        "contrast passes"
    );
    kani::cover!(
        result == Some(Wcag22ApplicableDecisionV1::Fail),
        "contrast fails"
    );
}

#[kani::proof]
fn interval_verdict_is_sound_for_every_enclosed_point() {
    let [al, au, bl, bu, a, b]: [u64; 6] = kani::any();
    let limit = super::super::q55_data::Q55_SCALE + 3;
    if !(al <= a && a <= au && au <= limit && bl <= b && b <= bu && bu <= limit) {
        return;
    }
    let criterion = criterion();
    let left = Wcag22LuminanceBoundsQ55V1 {
        lower: al,
        upper: au,
    };
    let right = Wcag22LuminanceBoundsQ55V1 {
        lower: bl,
        upper: bu,
    };
    let result = classify_pair(left, right, criterion);
    let actual = direct_ratio_pass(a, b, criterion);
    match result {
        Some(Wcag22ApplicableDecisionV1::Pass) => {
            assert!(
                actual,
                "interval PASS must hold for every enclosed colour pair"
            );
        }
        Some(Wcag22ApplicableDecisionV1::Fail) => {
            assert!(
                !actual,
                "interval FAIL must hold for every enclosed colour pair"
            );
        }
        None => {}
    }
    assert!(
        classify_pair(right, left, criterion) == result,
        "interval classification must preserve polarity symmetry"
    );
    kani::cover!(
        result == Some(Wcag22ApplicableDecisionV1::Pass),
        "interval passes"
    );
    kani::cover!(
        result == Some(Wcag22ApplicableDecisionV1::Fail),
        "interval fails"
    );
    kani::cover!(result.is_none(), "overlapping threshold stays uncertain");
}
