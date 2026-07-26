//! Тесты отчёта о качестве слота (разделы 3, 6 и 11 контракта).

use crate::cleanliness::outcome::{MovementV1, QualityOutcomeV1};
use crate::cleanliness::registry::AppearanceModeV1;
use crate::cleanliness::report::{
    ProfileWiseOrderingV1, QualityModeV1, SlotClassV1, SlotQualityV1, evaluate_slot_v1,
    slot_quality_v1,
};

fn report_of(
    mode: QualityModeV1,
    appearance_mode: AppearanceModeV1,
    slot_class: SlotClassV1,
) -> Option<crate::cleanliness::report::QualityReportV1> {
    match slot_quality_v1(mode, appearance_mode, slot_class) {
        SlotQualityV1::ConventionDisabled => None,
        SlotQualityV1::Reported(report) => Some(report),
    }
}

/// Сегодняшняя поставка целиком: девять пар «режим × класс слота» на каждом из
/// трёх appearance mode. Это RED раздела 12 — отсутствие профилей ступени 4
/// обязано кодироваться `UnchangedNoAdmittedProfile`, а не
/// `UnchangedOutsideEvidenceSupport`.
#[test]
fn shipping_configuration_reports_the_named_outcome_everywhere() {
    for appearance_mode in AppearanceModeV1::ALL {
        for mode in QualityModeV1::ALL {
            for slot_class in SlotClassV1::ALL {
                let quality = slot_quality_v1(mode, appearance_mode, slot_class);

                match (mode, slot_class) {
                    (QualityModeV1::Off, _) => {
                        assert_eq!(quality, SlotQualityV1::ConventionDisabled);
                    }
                    (_, SlotClassV1::Immutable) => {
                        let report = report_of(mode, appearance_mode, slot_class).unwrap();
                        assert_eq!(report.outcome(), QualityOutcomeV1::UnchangedImmutable);
                    }
                    (_, SlotClassV1::Movable) => {
                        let report = report_of(mode, appearance_mode, slot_class).unwrap();
                        assert_eq!(
                            report.outcome(),
                            QualityOutcomeV1::UnchangedNoAdmittedProfile,
                            "отсутствие допуска обязано называться своим именем"
                        );
                        assert_ne!(
                            report.outcome(),
                            QualityOutcomeV1::UnchangedOutsideEvidenceSupport,
                            "подмена отсутствия допуска выходом за support запрещена"
                        );
                    }
                }
            }
        }
    }
}

/// Ядро требования раздела 3: вердикт не зависит от режима. Тот же факт
/// проверяет const-ассерт модуля; здесь он записан как наблюдаемое поведение.
#[test]
fn lint_and_auto_agree_on_the_verdict() {
    for appearance_mode in AppearanceModeV1::ALL {
        for slot_class in SlotClassV1::ALL {
            let lint = report_of(QualityModeV1::Lint, appearance_mode, slot_class).unwrap();
            let auto = report_of(QualityModeV1::Auto, appearance_mode, slot_class).unwrap();

            assert_eq!(
                lint.outcome(),
                auto.outcome(),
                "lint и auto обязаны давать один вердикт: закон один"
            );
            assert_eq!(
                lint.outcome(),
                evaluate_slot_v1(appearance_mode, slot_class),
                "отчёт обязан нести вердикт закона действия, а не собственный"
            );
        }
    }
}

/// Различие режимов — ровно в применении, и только в нём.
#[test]
fn only_auto_applies_the_verdict() {
    assert!(!QualityModeV1::Off.applies_verdict());
    assert!(!QualityModeV1::Lint.applies_verdict());
    assert!(QualityModeV1::Auto.applies_verdict());

    for appearance_mode in AppearanceModeV1::ALL {
        for slot_class in SlotClassV1::ALL {
            let lint = report_of(QualityModeV1::Lint, appearance_mode, slot_class).unwrap();
            assert_eq!(
                lint.byte_movement(),
                MovementV1::Unchanged,
                "lint не двигает байты ни при каком вердикте"
            );
        }
    }
}

/// Движение байтов есть функция пары «исход и режим». Проверяется на всех
/// пятнадцати исходах, а не только на достижимых сегодня: иначе тест молчал бы
/// ровно о тех вариантах, ради которых правило и написано.
#[test]
fn byte_movement_is_a_function_of_outcome_and_mode() {
    for outcome in QualityOutcomeV1::ALL {
        let moves_under_auto = outcome.verdict_movement() == MovementV1::Moved;

        // В lint байты не двигаются ни при каком вердикте.
        assert!(
            !QualityModeV1::Lint.applies_verdict(),
            "lint не применяет вердикт"
        );

        // В auto байты двигаются ровно тогда, когда вердикт это предписывает.
        assert_eq!(
            QualityModeV1::Auto.applies_verdict() && moves_under_auto,
            moves_under_auto,
            "auto обязан применять предписание вердикта {outcome:?} без изменений"
        );
    }
}

#[test]
fn off_never_produces_an_outcome() {
    for appearance_mode in AppearanceModeV1::ALL {
        for slot_class in SlotClassV1::ALL {
            assert_eq!(
                slot_quality_v1(QualityModeV1::Off, appearance_mode, slot_class),
                SlotQualityV1::ConventionDisabled,
                "off не порождает значения QualityOutcome вовсе"
            );
        }
    }
}

/// Immutable-слот неприкосновенен при любом режиме, и его исход перехватывает
/// раньше отсутствия допуска: ранг 1 старше ранга 2.
#[test]
fn immutable_slots_outrank_the_missing_profile() {
    let immutable = QualityOutcomeV1::UnchangedImmutable.priority();
    let no_profile = QualityOutcomeV1::UnchangedNoAdmittedProfile.priority();
    assert!(immutable < no_profile);

    for appearance_mode in AppearanceModeV1::ALL {
        for mode in [QualityModeV1::Lint, QualityModeV1::Auto] {
            let report = report_of(mode, appearance_mode, SlotClassV1::Immutable).unwrap();
            assert_eq!(report.outcome(), QualityOutcomeV1::UnchangedImmutable);
        }
    }
}

/// Отчёт называет тот домен, по которому его спросили: иначе он приписал бы
/// вывод чужому корпусу, что раздел 9 запрещает.
#[test]
fn report_names_the_requested_appearance_mode() {
    for appearance_mode in AppearanceModeV1::ALL {
        let report = report_of(QualityModeV1::Lint, appearance_mode, SlotClassV1::Movable).unwrap();
        assert_eq!(report.appearance_mode(), appearance_mode);
        assert_eq!(
            report.ordering(),
            ProfileWiseOrderingV1::NoApplicableProfile(appearance_mode)
        );
    }
}

#[test]
fn mode_keys_are_pairwise_distinct() {
    for (index, mode) in QualityModeV1::ALL.iter().enumerate() {
        for other in &QualityModeV1::ALL[index + 1..] {
            assert_ne!(mode.key(), other.key());
        }
    }
}
