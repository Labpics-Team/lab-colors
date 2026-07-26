//! Тесты словаря исходов чистоты (раздел 6 контракта).

use crate::cleanliness::outcome::{
    MovementV1, OutcomePhaseV1, OutcomePriorityV1, QualityOutcomeV1,
};

/// Позиция варианта в объявлении типа.
///
/// Наблюдается **из самого типа**, а не из рукописного списка: у fieldless-enum
/// без явных дискриминантов `as u8` даёт индекс объявления. Это существенно —
/// второй рукописный список ничего бы не проверял, потому что автор, изменивший
/// порядок в типе, изменил бы и его.
fn declaration_index(outcome: QualityOutcomeV1) -> u8 {
    outcome as u8
}

#[test]
fn priority_ranks_are_contiguous_from_one() {
    for (index, priority) in OutcomePriorityV1::ALL.iter().enumerate() {
        assert_eq!(
            priority.rank() as usize,
            index + 1,
            "ранги обязаны идти подряд от 1 без пропусков"
        );
    }
}

#[test]
fn priority_and_outcome_are_mutually_inverse() {
    for outcome in QualityOutcomeV1::ALL {
        assert_eq!(outcome.priority().outcome(), outcome);
    }
    for priority in OutcomePriorityV1::ALL {
        assert_eq!(priority.outcome().priority(), priority);
    }
}

#[test]
fn all_is_ordered_by_priority() {
    for (index, outcome) in QualityOutcomeV1::ALL.iter().enumerate() {
        assert_eq!(outcome.priority().rank() as usize, index + 1);
    }
}

/// Биекция «позиция объявления ↔ элемент `ALL`»: каждая позиция объявления
/// встречается в `ALL` ровно один раз.
///
/// Сравнение идёт с индексами, наблюдаемыми из типа, а не с рукописным
/// списком, поэтому проверка не может стать тавтологией.
#[test]
fn every_declaration_position_appears_exactly_once_in_all() {
    let indices: Vec<u8> = QualityOutcomeV1::ALL
        .iter()
        .map(|outcome| declaration_index(*outcome))
        .collect();

    for position in 0..QualityOutcomeV1::ALL.len() as u8 {
        let hits = indices.iter().filter(|index| **index == position).count();
        assert_eq!(
            hits, 1,
            "позиция объявления {position} обязана встречаться в ALL ровно один раз"
        );
    }
}

/// Анти-вакуум для проверок порядка.
///
/// Если бы порядок объявления совпал с приоритетным, проверки порядка стали бы
/// бессодержательными. Тест берёт индексы объявления **из типа**: выравнивание
/// порядков сделает последовательность вдоль `ALL` тождественной `0..15`, и
/// тест покраснеет. Рукописный список этого поймать не мог — автор,
/// переставивший варианты, переставил бы и его.
#[test]
fn declaration_order_differs_from_priority_order() {
    let indices: Vec<u8> = QualityOutcomeV1::ALL
        .iter()
        .map(|outcome| declaration_index(*outcome))
        .collect();
    let identity: Vec<u8> = (0..QualityOutcomeV1::ALL.len() as u8).collect();

    assert_ne!(
        indices, identity,
        "порядок объявления обязан отличаться от приоритетного, иначе проверки порядка пусты"
    );
}

/// Сам наблюдатель порядка не вакуумен: индексы обязаны быть различны и лежать
/// в диапазоне типа. Без этого `declaration_index` мог бы вернуть константу, и
/// оба теста выше стали бы бессмысленными.
#[test]
fn declaration_indices_are_distinct_and_in_range() {
    let mut seen = [false; 15];
    for outcome in QualityOutcomeV1::ALL {
        let index = declaration_index(outcome) as usize;
        assert!(index < seen.len(), "индекс объявления вне диапазона типа");
        assert!(!seen[index], "индекс объявления {index} повторился");
        seen[index] = true;
    }
    assert!(
        seen.iter().all(|hit| *hit),
        "не все позиции объявления заняты"
    );
}

#[test]
fn reason_keys_round_trip() {
    for outcome in QualityOutcomeV1::ALL {
        assert_eq!(QualityOutcomeV1::parse(outcome.key()), Some(outcome));
    }
}

#[test]
fn reason_keys_are_pairwise_distinct() {
    for (index, outcome) in QualityOutcomeV1::ALL.iter().enumerate() {
        for other in &QualityOutcomeV1::ALL[index + 1..] {
            assert_ne!(
                outcome.key(),
                other.key(),
                "reason-ключи обязаны различаться"
            );
        }
    }
}

#[test]
fn parse_rejects_anything_but_an_exact_key() {
    assert_eq!(QualityOutcomeV1::parse(""), None);
    assert_eq!(QualityOutcomeV1::parse("improved "), None);
    assert_eq!(QualityOutcomeV1::parse("Improved"), None);
    assert_eq!(QualityOutcomeV1::parse("IMPROVED"), None);
    assert_eq!(QualityOutcomeV1::parse("unchanged"), None);
    assert_eq!(
        QualityOutcomeV1::parse("unchanged-below-output-resolution"),
        None
    );
}

/// Фазовая группировка раздела 6: ранги 1–6, 7–8, 9–12, 13–15.
#[test]
fn phases_partition_the_ranks_as_the_contract_states() {
    for priority in OutcomePriorityV1::ALL {
        let expected = match priority.rank() {
            1..=6 => OutcomePhaseV1::NoEvaluationResult,
            7..=8 => OutcomePhaseV1::MovementCancelled,
            9..=12 => OutcomePhaseV1::MovementDeclined,
            13..=15 => OutcomePhaseV1::Moved,
            rank => panic!("ранг {rank} вне объявленного диапазона 1..=15"),
        };
        assert_eq!(priority.phase(), expected, "фаза ранга {}", priority.rank());
    }
}

#[test]
fn phases_are_contiguous_and_ordered_by_rank() {
    let mut previous = OutcomePhaseV1::NoEvaluationResult;
    for priority in OutcomePriorityV1::ALL {
        let phase = priority.phase();
        assert!(
            phase >= previous,
            "фазы обязаны идти по возрастанию ранга без чередования"
        );
        previous = phase;
    }
    for phase in OutcomePhaseV1::ALL {
        assert!(
            OutcomePriorityV1::ALL
                .iter()
                .any(|priority| priority.phase() == phase),
            "{phase:?} обязана иметь хотя бы один ранг: фазы без ситуации не бывает"
        );
    }
}

/// Только фаза `Moved` предписывает движение. Отмена (ранги 7–8) возвращает
/// слот к reference state, поэтому вердикт движения не предписывает.
#[test]
fn only_the_moved_phase_prescribes_movement() {
    for outcome in QualityOutcomeV1::ALL {
        let expected = match outcome.phase() {
            OutcomePhaseV1::Moved => MovementV1::Moved,
            _ => MovementV1::Unchanged,
        };
        assert_eq!(
            outcome.verdict_movement(),
            expected,
            "предписанное движение для {outcome:?}"
        );
    }
}

/// Ключ, начинающийся на `unchanged-`, обязан отвечать вердикту без движения,
/// и наоборот. Иначе имя исхода лгало бы о том, что вердикт предписывает.
#[test]
fn key_prefix_agrees_with_movement() {
    for outcome in QualityOutcomeV1::ALL {
        let claims_unchanged = outcome.key().starts_with("unchanged-");
        let moves = outcome.verdict_movement() == MovementV1::Moved;
        assert_ne!(
            claims_unchanged, moves,
            "{outcome:?}: имя и предписанное движение разошлись"
        );
    }
}

/// Отсутствие допущенных профилей и выход за область поддержки — разные факты
/// о мире, и контракт запрещает подменять один другим (RED раздела 12).
#[test]
fn no_admitted_profile_is_distinct_from_outside_support() {
    let no_profile = QualityOutcomeV1::UnchangedNoAdmittedProfile;
    let outside = QualityOutcomeV1::UnchangedOutsideEvidenceSupport;

    assert_ne!(no_profile.key(), outside.key());
    assert_ne!(no_profile.priority(), outside.priority());
    assert!(
        no_profile.priority().rank() < outside.priority().rank(),
        "отсутствие допуска обязано разрешаться раньше выхода за support"
    );
}

/// `UnchangedUndominated` — остаточная ветвь фазы оценки: она обязана быть
/// последней в своей фазе, иначе «замыкание фазы» перестало бы быть верным.
#[test]
fn undominated_closes_the_evaluation_phase() {
    let undominated = QualityOutcomeV1::UnchangedUndominated;
    assert_eq!(undominated.phase(), OutcomePhaseV1::MovementDeclined);

    for priority in OutcomePriorityV1::ALL {
        if priority.phase() == OutcomePhaseV1::MovementDeclined {
            assert!(
                priority.rank() <= undominated.priority().rank(),
                "остаточная ветвь обязана иметь наибольший ранг в своей фазе"
            );
        }
    }
}
