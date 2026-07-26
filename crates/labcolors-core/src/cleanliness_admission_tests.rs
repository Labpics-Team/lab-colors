//! Тесты лестницы допуска чистоты (раздел 9 контракта).

use crate::cleanliness::admission::{
    AdmittedLevelV1, DispositionV1, MovementAuthorityV1, ResearchLabelV1,
};

/// Сегодняшнее состояние доказательной базы: ни одна достижимая диспозиция не
/// даёт права двигать цвет. Тот же факт проверяется const-ассертом в модуле;
/// здесь он записан ещё и как наблюдаемое поведение.
#[test]
fn no_reachable_disposition_authorises_movement() {
    for disposition in DispositionV1::ALL {
        assert!(
            disposition.movement_authority().is_none(),
            "{disposition:?} не вправе двигать цвет"
        );
    }
}

/// Массив достижимых значений физически не содержит ступени 4 — иначе он бы не
/// собрался, потому что её носитель необитаем.
#[test]
fn admitted_levels_stop_below_auto_action() {
    assert_eq!(AdmittedLevelV1::ALL.len(), 3);
    assert_eq!(DispositionV1::ALL.len(), 6);

    assert_eq!(
        AdmittedLevelV1::ALL,
        [
            AdmittedLevelV1::Math,
            AdmittedLevelV1::Correlate,
            AdmittedLevelV1::Decision
        ],
        "достижимые ступени обязаны исчерпываться первыми тремя"
    );

    // Ни одна достижимая ступень не вправе называться ступенью 4: иначе отчёт
    // заявил бы право, которого не существует.
    for level in AdmittedLevelV1::ALL {
        assert_ne!(level.key(), "auto-action-admitted");
    }
}

/// «Допущенные уровни включают предыдущие» — порядок обязан это выражать.
#[test]
fn admitted_levels_are_ordered_by_inclusion() {
    assert!(AdmittedLevelV1::Math < AdmittedLevelV1::Correlate);
    assert!(AdmittedLevelV1::Correlate < AdmittedLevelV1::Decision);
}

#[test]
fn disposition_keys_are_pairwise_distinct() {
    for (index, disposition) in DispositionV1::ALL.iter().enumerate() {
        for other in &DispositionV1::ALL[index + 1..] {
            assert_ne!(disposition.key(), other.key());
        }
    }
}

#[test]
fn research_labels_are_closed_at_seven_and_distinct() {
    assert_eq!(ResearchLabelV1::ALL.len(), 7);
    for (index, label) in ResearchLabelV1::ALL.iter().enumerate() {
        for other in &ResearchLabelV1::ALL[index + 1..] {
            assert_ne!(label.key(), other.key());
        }
    }
}

/// Research-метки и ступени допуска не пересекаются даже по ключам: метка,
/// случайно совпавшая по имени со ступенью, читалась бы как право.
#[test]
fn research_labels_never_collide_with_admission_keys() {
    for label in ResearchLabelV1::ALL {
        for disposition in DispositionV1::ALL {
            assert_ne!(
                label.key(),
                disposition.key(),
                "research-метка не вправе носить имя ступени допуска"
            );
        }
    }
}

/// Размер права равен размеру его носителя — то есть нулю обитаемых значений.
/// Если кто-то добавит обитаемое поле в обход свидетельства, тест покраснеет.
#[test]
fn movement_authority_carries_nothing_but_the_admission() {
    assert_eq!(
        core::mem::size_of::<MovementAuthorityV1>(),
        core::mem::size_of::<crate::cleanliness::admission::AutoActionAdmissionV1>(),
        "право обязано быть ровно свидетельством, без дополнительного состояния"
    );
}
