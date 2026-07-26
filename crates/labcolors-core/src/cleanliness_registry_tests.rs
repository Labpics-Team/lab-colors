//! Тесты mode-aware реестра профилей чистоты (раздел 9 контракта).

use crate::cleanliness::registry::{
    AppearanceModeV1, ProfileRegistryRowV1, movement_authority_v1, profile_registry_row_v1,
    profile_registry_v1,
};

#[test]
fn registry_covers_every_declared_mode_exactly_once() {
    let registry = profile_registry_v1();
    assert_eq!(registry.len(), AppearanceModeV1::ALL.len());

    for mode in AppearanceModeV1::ALL {
        let rows = registry.iter().filter(|row| row.mode() == mode).count();
        assert_eq!(rows, 1, "{mode:?} обязан иметь ровно одну строку реестра");
    }
}

#[test]
fn lookup_returns_the_row_of_the_requested_mode() {
    for mode in AppearanceModeV1::ALL {
        assert_eq!(profile_registry_row_v1(mode).mode(), mode);
    }
}

/// Сегодняшняя поставка: три объявленных режима, ни одного допущенного
/// профиля. Тот же факт проверяет const-ассерт модуля.
#[test]
fn no_mode_authorises_movement_today() {
    for mode in AppearanceModeV1::ALL {
        assert!(
            movement_authority_v1(mode).is_none(),
            "{mode:?} не вправе двигать цвет"
        );
    }
}

/// Отсутствие профиля выражено **явной строкой**, а не пустотой: пустой список
/// неотличим от несуществующего режима.
#[test]
fn absence_is_a_named_row_not_an_empty_list() {
    for row in profile_registry_v1() {
        match row {
            ProfileRegistryRowV1::NoAdmittedProfile(mode) => {
                assert!(AppearanceModeV1::ALL.contains(mode));
            }
            ProfileRegistryRowV1::Admitted(profile) => {
                panic!("допущенных профилей сегодня нет, а найден {profile:?}")
            }
        }
    }
}

#[test]
fn mode_keys_are_pairwise_distinct() {
    for (index, mode) in AppearanceModeV1::ALL.iter().enumerate() {
        for other in &AppearanceModeV1::ALL[index + 1..] {
            assert_ne!(mode.key(), other.key());
        }
    }
}

/// Домены не смешиваются: ключи режимов не вправе совпадать с ключами ступеней
/// допуска, иначе отчёт спутал бы домен с уровнем права.
#[test]
fn mode_keys_never_collide_with_admission_keys() {
    use crate::cleanliness::admission::DispositionV1;

    for mode in AppearanceModeV1::ALL {
        for disposition in DispositionV1::ALL {
            assert_ne!(mode.key(), disposition.key());
        }
    }
}
