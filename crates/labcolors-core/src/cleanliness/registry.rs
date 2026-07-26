//! Mode-aware реестр профилей чистоты (раздел 9 контракта).
//!
//! Домены не смешиваются: профиль допускается для объявленного appearance mode
//! и на другой не переносится, а корпуса разных режимов численно не
//! объединяются. Поэтому реестр индексирован режимом, и у каждого режима —
//! собственная строка.
//!
//! # Почему отсутствие профиля — строка, а не пустой список
//!
//! Пустой срез и `Option::None` неотличимы от «режима не существует». Реестр
//! обязан различать «режим объявлен, профилей нет» и «режима нет вовсе»,
//! потому что первое — сегодняшняя поставка, а второе было бы ошибкой
//! конфигурации. Отсюда явный вариант
//! [`ProfileRegistryRowV1::NoAdmittedProfile`], несущий имя режима.

use super::admission::{AutoActionAdmissionV1, MovementAuthorityV1};

/// Объявленный appearance mode раздела 9.
///
/// У каждого домена собственный корпус и собственный admission. Структурное
/// наследование одного домена от другого запрещено: оно протащило бы границу,
/// величину эффекта и uncertainty, тогда как переносить разрешён только знак.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppearanceModeV1 {
    /// Поверхностные образцы.
    SurfaceRelatedV1,
    /// Экранный related-домен.
    DisplayRelatedV1,
    /// Изолированное свечение, индикаторы, dark-surround.
    UnrelatedLuminousV1,
}

impl AppearanceModeV1 {
    /// Полный перечень объявленных режимов.
    pub const ALL: [Self; 3] = [
        Self::SurfaceRelatedV1,
        Self::DisplayRelatedV1,
        Self::UnrelatedLuminousV1,
    ];

    /// Стабильный ключ режима.
    pub const fn key(self) -> &'static str {
        match self {
            Self::SurfaceRelatedV1 => "surface-related-v1",
            Self::DisplayRelatedV1 => "display-related-v1",
            Self::UnrelatedLuminousV1 => "unrelated-luminous-v1",
        }
    }
}

/// Строка реестра для одного appearance mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProfileRegistryRowV1 {
    /// Режим объявлен, профилей ступени `AutoActionAdmitted` для него нет.
    ///
    /// Это сегодняшняя поставка всех трёх режимов и **не** утверждение о
    /// выходе за область применимости: подмена одного другим запрещена
    /// разделом 5.7 и краснит RED раздела 12.
    NoAdmittedProfile(AppearanceModeV1),
    /// Режим имеет допущенный профиль ступени 4.
    Admitted(AdmittedProfileV1),
}

impl ProfileRegistryRowV1 {
    /// Режим, к которому относится строка.
    pub const fn mode(self) -> AppearanceModeV1 {
        match self {
            Self::NoAdmittedProfile(mode) => mode,
            Self::Admitted(profile) => profile.mode(),
        }
    }

    /// Право двигать цвет в этом режиме.
    ///
    /// Сегодня компилятор доказывает, что ответ — `None` для каждой достижимой
    /// строки: ветвь `Admitted` недостижима, потому что её носитель необитаем.
    pub const fn movement_authority(self) -> Option<MovementAuthorityV1> {
        match self {
            Self::NoAdmittedProfile(_) => None,
            Self::Admitted(profile) => Some(profile.authority()),
        }
    }
}

/// Дескриптор допущенного профиля ступени 4.
///
/// Необитаем, потому что несёт свидетельство допуска, которого не существует.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdmittedProfileV1 {
    mode: AppearanceModeV1,
    admission: AutoActionAdmissionV1,
}

impl AdmittedProfileV1 {
    /// Appearance mode, для которого профиль допущен.
    pub const fn mode(self) -> AppearanceModeV1 {
        self.mode
    }

    /// Право двигать цвет, вытекающее из свидетельства ступени 4.
    pub const fn authority(self) -> MovementAuthorityV1 {
        MovementAuthorityV1::from_admission(self.admission)
    }

    /// Значения не существует, поэтому вызов недостижим.
    pub const fn absurd(self) -> ! {
        self.admission.absurd()
    }
}

/// Полный реестр: ровно по одной строке на объявленный appearance mode.
pub const fn profile_registry_v1() -> &'static [ProfileRegistryRowV1; 3] {
    &REGISTRY
}

/// Строка реестра для названного режима.
///
/// Тотальна: строка существует для каждого объявленного режима, поэтому
/// `Option` здесь не нужен и был бы ложным обещанием, что режим может
/// отсутствовать.
pub const fn profile_registry_row_v1(mode: AppearanceModeV1) -> ProfileRegistryRowV1 {
    match mode {
        AppearanceModeV1::SurfaceRelatedV1 => REGISTRY[0],
        AppearanceModeV1::DisplayRelatedV1 => REGISTRY[1],
        AppearanceModeV1::UnrelatedLuminousV1 => REGISTRY[2],
    }
}

/// Право двигать цвет в названном режиме.
pub const fn movement_authority_v1(mode: AppearanceModeV1) -> Option<MovementAuthorityV1> {
    profile_registry_row_v1(mode).movement_authority()
}

/// Сегодняшний реестр: три объявленных режима, ни одного допущенного профиля.
///
/// Экзамен на замороженном holdout кандидаты не прошли, поэтому строк
/// `Admitted` нет. Это состояние доказательной базы, а не заглушка.
const REGISTRY: [ProfileRegistryRowV1; 3] = [
    ProfileRegistryRowV1::NoAdmittedProfile(AppearanceModeV1::SurfaceRelatedV1),
    ProfileRegistryRowV1::NoAdmittedProfile(AppearanceModeV1::DisplayRelatedV1),
    ProfileRegistryRowV1::NoAdmittedProfile(AppearanceModeV1::UnrelatedLuminousV1),
];

// Реестр обязан покрывать каждый объявленный режим ровно одной строкой, и
// строка обязана относиться к тому режиму, по которому её нашли. Иначе
// `profile_registry_row_v1` вернула бы чужую строку, а отчёт назвал бы чужой
// домен.
const _: () = {
    assert!(REGISTRY.len() == AppearanceModeV1::ALL.len());

    let mut index = 0;
    while index < AppearanceModeV1::ALL.len() {
        let mode = AppearanceModeV1::ALL[index];
        assert!(
            profile_registry_row_v1(mode).mode() as u8 == mode as u8,
            "строка реестра относится к другому режиму"
        );
        assert!(
            movement_authority_v1(mode).is_none(),
            "право двигать цвет не может существовать без профиля ступени 4"
        );
        index += 1;
    }
};
