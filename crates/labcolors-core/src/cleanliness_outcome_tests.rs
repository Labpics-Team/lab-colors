//! Тесты словаря исходов чистоты (раздел 6 контракта).

use crate::cleanliness::outcome::{
    MovementV1, OutcomePhaseV1, OutcomePriorityV1, QualityOutcomeV1,
};

/// Порядок объявления вариантов в типе — тот, в котором они написаны в
/// контракте, и он намеренно **отличается** от порядка приоритета.
const DECLARATION_ORDER: [QualityOutcomeV1; 15] = [
    QualityOutcomeV1::Improved,
    QualityOutcomeV1::ImprovedToAllowanceCap,
    QualityOutcomeV1::PartiallyImproved,
    QualityOutcomeV1::UnchangedDirectionalConflict,
    QualityOutcomeV1::UnchangedContextUnresolvable,
    QualityOutcomeV1::UnchangedNoAdmittedProfile,
    QualityOutcomeV1::UnchangedNoReferenceState,
    QualityOutcomeV1::UnchangedOutsideEvidenceSupport,
    QualityOutcomeV1::UnchangedSignUnresolved,
    QualityOutcomeV1::UnchangedProfileConflict,
    QualityOutcomeV1::UnchangedUndominated,
    QualityOutcomeV1::UnchangedImmutable,
    QualityOutcomeV1::UnchangedEnumerationBudgetExhausted,
    QualityOutcomeV1::UnchangedFailedEmissionRecheck,
    QualityOutcomeV1::UnchangedNotFixedPoint,
];

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

/// Биекция «тип ↔ приоритет»: ни одного исхода без ранга и ни одного ранга без
/// исхода. Это та проверка, которую нормативный документ провалил в трёх
/// последовательных ревью, поэтому она стоит здесь явно.
#[test]
fn every_declared_variant_appears_exactly_once_in_all() {
    for declared in DECLARATION_ORDER {
        let hits = QualityOutcomeV1::ALL
            .iter()
            .filter(|candidate| **candidate == declared)
            .count();
        assert_eq!(
            hits, 1,
            "{declared:?} обязан встречаться в ALL ровно один раз"
        );
    }
    assert_eq!(DECLARATION_ORDER.len(), QualityOutcomeV1::ALL.len());
}

/// Анти-вакуум для самой проверки порядка.
///
/// Если бы порядок объявления совпал с порядком приоритета, проверки выше
/// стали бы бессодержательными, и заметить это было бы некому. Тест фиксирует,
/// что порядки различны, чтобы будущее «косметическое» выравнивание краснело.
#[test]
fn declaration_order_differs_from_priority_order() {
    assert_ne!(
        DECLARATION_ORDER,
        QualityOutcomeV1::ALL,
        "порядок объявления обязан отличаться от приоритетного, иначе проверки порядка пусты"
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

/// Только фаза `Moved` двигает байты. Отмена движения (ранги 7–8) возвращает
/// слот к reference state, поэтому байты не меняются.
#[test]
fn only_the_moved_phase_changes_emitted_bytes() {
    for outcome in QualityOutcomeV1::ALL {
        let expected = match outcome.phase() {
            OutcomePhaseV1::Moved => MovementV1::Moved,
            _ => MovementV1::Unchanged,
        };
        assert_eq!(outcome.movement(), expected, "движение для {outcome:?}");
    }

    assert_eq!(
        QualityOutcomeV1::UnchangedFailedEmissionRecheck.movement(),
        MovementV1::Unchanged,
        "откат после провала recheck обязан оставлять байты нетронутыми"
    );
}

/// Все исходы, чей ключ начинается на `unchanged-`, обязаны не двигать байты, и
/// наоборот. Иначе имя исхода лгало бы о наблюдаемом факте.
#[test]
fn key_prefix_agrees_with_movement() {
    for outcome in QualityOutcomeV1::ALL {
        let claims_unchanged = outcome.key().starts_with("unchanged-");
        let moves = outcome.movement() == MovementV1::Moved;
        assert_ne!(
            claims_unchanged, moves,
            "{outcome:?}: имя и наблюдаемое движение разошлись"
        );
    }
}

/// Отсутствие допущенных профилей и выход за область поддержки — разные факты
/// о мире, и контракт запрещает подменять один другим (RED раздела 12).
#[test]
fn no_admitted_profile_is_distinct_from_outside_support() {
    let no_profile = QualityOutcomeV1::UnchangedNoAdmittedProfile;
    let outside = QualityOutcomeV1::UnchangedOutsideEvidenceSupport;

    assert_ne!(no_profile, outside);
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
