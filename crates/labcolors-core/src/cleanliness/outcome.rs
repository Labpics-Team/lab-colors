//! Тотальный исход оценки чистоты для одного слота (раздел 6 контракта).
//!
//! Контракт требует, чтобы включённый корректор возвращал **названный** исход,
//! а не молча оставлял цвет как есть: молчание неотличимо от неработающего
//! корректора. Поэтому исход тотален — ситуации без исхода не существует, — а
//! правило разрешения приоритетов задано полным порядком без пропусков.

/// Тотальный исход оценки чистоты для одного слота (раздел 6 контракта).
///
/// Порядок объявления вариантов нормативным **не** является: контракт говорит
/// это прямо. Нормативны [`QualityOutcomeV1::priority`] и порядок
/// [`QualityOutcomeV1::ALL`].
///
/// Тип намеренно не выводит `Ord`: сравнение исходов между собой было бы
/// скрытым утверждением о порядке, а единственный объявленный порядок — это
/// приоритет правила разрешения, и он живёт в [`OutcomePriorityV1`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QualityOutcomeV1 {
    /// Вердикт предписывает движение; ограничений не встретилось.
    Improved,
    /// Вердикт предписывает движение; домен обрезан допуском смещения.
    ImprovedToAllowanceCap,
    /// Вердикт предписывает движение; домен обрезан ограничением уровней 1–2.
    PartiallyImproved,
    /// Для **каждого** допустимого кандидата существует применимый профиль с
    /// уверенно противоположным направлением.
    ///
    /// Это не обычная неопределённость: неуверенный профиль движение не
    /// блокирует. И это не расхождение профилей между собой — то ранг 10.
    UnchangedDirectionalConflict,
    /// Контекст вне объявленного `ScenarioSet` — нарушение входного контракта.
    UnchangedContextUnresolvable,
    /// Ни одного применимого профиля ступени `AutoActionAdmitted`.
    ///
    /// Это утверждение об **отсутствии допуска**, а не о выходе за область
    /// применимости: подмена одного другим запрещена разделом 5.7.
    UnchangedNoAdmittedProfile,
    /// Reference state не определён.
    UnchangedNoReferenceState,
    /// Состояние вне объявленной области поддержки корпуса.
    UnchangedOutsideEvidenceSupport,
    /// Знак не определён: объявленные популяции расходятся либо ухудшение
    /// неуверенно.
    UnchangedSignUnresolved,
    /// Существует кандидат, на котором профили из `P` дали противоположные
    /// знаки, и ни один кандидат не доминирует reference state.
    UnchangedProfileConflict,
    /// Кандидаты оценены, ни один не доминирует reference state.
    ///
    /// Остаточная ветвь фазы оценки: сюда приходит любой набор знаков, не
    /// попавший в ранги 9–11.
    UnchangedUndominated,
    /// Слот вне области движения: client anchor либо hard association binding.
    UnchangedImmutable,
    /// Бюджет перечисления кандидатов исчерпан.
    UnchangedEnumerationBudgetExhausted,
    /// Final emitted-occurrence recheck после движения не прошёл; откат.
    UnchangedFailedEmissionRecheck,
    /// Кандидат выбран, но не прошёл проверку неподвижности; движение отменено.
    UnchangedNotFixedPoint,
}

impl QualityOutcomeV1 {
    /// Полный перечень исходов **в порядке тотального приоритета** раздела 6.
    ///
    /// Порядок здесь нормативен и отличается от порядка объявления вариантов.
    pub const ALL: [Self; 15] = [
        Self::UnchangedImmutable,
        Self::UnchangedNoAdmittedProfile,
        Self::UnchangedNoReferenceState,
        Self::UnchangedOutsideEvidenceSupport,
        Self::UnchangedContextUnresolvable,
        Self::UnchangedEnumerationBudgetExhausted,
        Self::UnchangedFailedEmissionRecheck,
        Self::UnchangedNotFixedPoint,
        Self::UnchangedDirectionalConflict,
        Self::UnchangedProfileConflict,
        Self::UnchangedSignUnresolved,
        Self::UnchangedUndominated,
        Self::PartiallyImproved,
        Self::ImprovedToAllowanceCap,
        Self::Improved,
    ];

    /// Ранг правила разрешения (раздел 6).
    ///
    /// Отображение тотально: компилятор требует ветвь на каждый вариант.
    pub const fn priority(self) -> OutcomePriorityV1 {
        match self {
            Self::UnchangedImmutable => OutcomePriorityV1::ImmutableSlot,
            Self::UnchangedNoAdmittedProfile => OutcomePriorityV1::NoAdmittedProfile,
            Self::UnchangedNoReferenceState => OutcomePriorityV1::NoReferenceState,
            Self::UnchangedOutsideEvidenceSupport => OutcomePriorityV1::OutsideEvidenceSupport,
            Self::UnchangedContextUnresolvable => OutcomePriorityV1::ContextUnresolvable,
            Self::UnchangedEnumerationBudgetExhausted => {
                OutcomePriorityV1::EnumerationBudgetExhausted
            }
            Self::UnchangedFailedEmissionRecheck => OutcomePriorityV1::FailedEmissionRecheck,
            Self::UnchangedNotFixedPoint => OutcomePriorityV1::NotFixedPoint,
            Self::UnchangedDirectionalConflict => OutcomePriorityV1::DirectionalConflict,
            Self::UnchangedProfileConflict => OutcomePriorityV1::ProfileConflict,
            Self::UnchangedSignUnresolved => OutcomePriorityV1::SignUnresolved,
            Self::UnchangedUndominated => OutcomePriorityV1::Undominated,
            Self::PartiallyImproved => OutcomePriorityV1::PartiallyImproved,
            Self::ImprovedToAllowanceCap => OutcomePriorityV1::ImprovedToAllowanceCap,
            Self::Improved => OutcomePriorityV1::Improved,
        }
    }

    /// Стабильный reason-ключ certificate.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Improved => "improved",
            Self::ImprovedToAllowanceCap => "improved-to-allowance-cap",
            Self::PartiallyImproved => "partially-improved",
            Self::UnchangedDirectionalConflict => "unchanged-directional-conflict",
            Self::UnchangedContextUnresolvable => "unchanged-context-unresolvable",
            Self::UnchangedNoAdmittedProfile => "unchanged-no-admitted-profile",
            Self::UnchangedNoReferenceState => "unchanged-no-reference-state",
            Self::UnchangedOutsideEvidenceSupport => "unchanged-outside-evidence-support",
            Self::UnchangedSignUnresolved => "unchanged-sign-unresolved",
            Self::UnchangedProfileConflict => "unchanged-profile-conflict",
            Self::UnchangedUndominated => "unchanged-undominated",
            Self::UnchangedImmutable => "unchanged-immutable",
            Self::UnchangedEnumerationBudgetExhausted => "unchanged-enumeration-budget-exhausted",
            Self::UnchangedFailedEmissionRecheck => "unchanged-failed-emission-recheck",
            Self::UnchangedNotFixedPoint => "unchanged-not-fixed-point",
        }
    }

    /// Точный разбор reason-ключа. Алиасов и подстановок по умолчанию нет.
    pub fn parse(key: &str) -> Option<Self> {
        let mut index = 0;
        while index < Self::ALL.len() {
            let candidate = Self::ALL[index];
            if candidate.key().as_bytes() == key.as_bytes() {
                return Some(candidate);
            }
            index += 1;
        }
        None
    }

    /// Фаза разрешения, которую замыкает исход (раздел 6).
    pub const fn phase(self) -> OutcomePhaseV1 {
        self.priority().phase()
    }

    /// Движение, которое предписывает вердикт.
    ///
    /// Исход называет **вердикт оценки**, общей для `lint` и `auto`, а не факт
    /// записи байтов: применён ли вердикт — свойство режима (раздел 3).
    /// Поэтому здесь возвращается движение, которое выполнил бы `auto`. В
    /// режиме `lint` байты слота не меняются ни при каком вердикте.
    pub const fn verdict_movement(self) -> MovementV1 {
        self.priority().verdict_movement()
    }
}

/// Ранг правила разрешения раздела 6.
///
/// Это **не** мера чистоты и не величина улучшения: меньший ранг означает лишь
/// то, что правило срабатывает раньше. Здесь — в отличие от
/// [`QualityOutcomeV1`] — порядок объявления нормативен, и `Ord` его выражает.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum OutcomePriorityV1 {
    /// Ранг 1: слот вне области движения.
    ImmutableSlot = 1,
    /// Ранг 2: ни одного профиля ступени `AutoActionAdmitted`.
    NoAdmittedProfile = 2,
    /// Ранг 3: reference state не определён.
    NoReferenceState = 3,
    /// Ранг 4: состояние вне области поддержки корпуса.
    OutsideEvidenceSupport = 4,
    /// Ранг 5: контекст вне объявленного `ScenarioSet`.
    ContextUnresolvable = 5,
    /// Ранг 6: бюджет перечисления исчерпан.
    EnumerationBudgetExhausted = 6,
    /// Ранг 7: final emitted-occurrence recheck не прошёл.
    FailedEmissionRecheck = 7,
    /// Ранг 8: проверка неподвижности не прошла.
    NotFixedPoint = 8,
    /// Ранг 9: у каждого кандидата есть профиль с уверенно противоположным
    /// направлением.
    DirectionalConflict = 9,
    /// Ранг 10: два профиля разошлись по знаку на одном кандидате.
    ProfileConflict = 10,
    /// Ранг 11: знак не определён.
    SignUnresolved = 11,
    /// Ранг 12: доминирующего кандидата нет; остаточная ветвь фазы оценки.
    Undominated = 12,
    /// Ранг 13: вердикт предписывает движение, обрезанное ограничением уровней 1–2.
    PartiallyImproved = 13,
    /// Ранг 14: вердикт предписывает движение, обрезанное допуском смещения.
    ImprovedToAllowanceCap = 14,
    /// Ранг 15: вердикт предписывает движение без ограничений.
    Improved = 15,
}

impl OutcomePriorityV1 {
    /// Полный перечень рангов по возрастанию.
    pub const ALL: [Self; 15] = [
        Self::ImmutableSlot,
        Self::NoAdmittedProfile,
        Self::NoReferenceState,
        Self::OutsideEvidenceSupport,
        Self::ContextUnresolvable,
        Self::EnumerationBudgetExhausted,
        Self::FailedEmissionRecheck,
        Self::NotFixedPoint,
        Self::DirectionalConflict,
        Self::ProfileConflict,
        Self::SignUnresolved,
        Self::Undominated,
        Self::PartiallyImproved,
        Self::ImprovedToAllowanceCap,
        Self::Improved,
    ];

    /// Числовой ранг раздела 6, от 1 до 15.
    pub const fn rank(self) -> u8 {
        self as u8
    }

    /// Обратное отображение к [`QualityOutcomeV1::priority`].
    ///
    /// Тотально и без `Option`: значений вне диапазона 1..=15 не существует,
    /// потому что тип закрыт.
    pub const fn outcome(self) -> QualityOutcomeV1 {
        match self {
            Self::ImmutableSlot => QualityOutcomeV1::UnchangedImmutable,
            Self::NoAdmittedProfile => QualityOutcomeV1::UnchangedNoAdmittedProfile,
            Self::NoReferenceState => QualityOutcomeV1::UnchangedNoReferenceState,
            Self::OutsideEvidenceSupport => QualityOutcomeV1::UnchangedOutsideEvidenceSupport,
            Self::ContextUnresolvable => QualityOutcomeV1::UnchangedContextUnresolvable,
            Self::EnumerationBudgetExhausted => {
                QualityOutcomeV1::UnchangedEnumerationBudgetExhausted
            }
            Self::FailedEmissionRecheck => QualityOutcomeV1::UnchangedFailedEmissionRecheck,
            Self::NotFixedPoint => QualityOutcomeV1::UnchangedNotFixedPoint,
            Self::DirectionalConflict => QualityOutcomeV1::UnchangedDirectionalConflict,
            Self::ProfileConflict => QualityOutcomeV1::UnchangedProfileConflict,
            Self::SignUnresolved => QualityOutcomeV1::UnchangedSignUnresolved,
            Self::Undominated => QualityOutcomeV1::UnchangedUndominated,
            Self::PartiallyImproved => QualityOutcomeV1::PartiallyImproved,
            Self::ImprovedToAllowanceCap => QualityOutcomeV1::ImprovedToAllowanceCap,
            Self::Improved => QualityOutcomeV1::Improved,
        }
    }

    /// Фаза разрешения, которую замыкает ранг (раздел 6).
    pub const fn phase(self) -> OutcomePhaseV1 {
        match self {
            Self::ImmutableSlot
            | Self::NoAdmittedProfile
            | Self::NoReferenceState
            | Self::OutsideEvidenceSupport
            | Self::ContextUnresolvable
            | Self::EnumerationBudgetExhausted => OutcomePhaseV1::NoEvaluationResult,
            Self::FailedEmissionRecheck | Self::NotFixedPoint => OutcomePhaseV1::MovementCancelled,
            Self::DirectionalConflict
            | Self::ProfileConflict
            | Self::SignUnresolved
            | Self::Undominated => OutcomePhaseV1::MovementDeclined,
            Self::PartiallyImproved | Self::ImprovedToAllowanceCap | Self::Improved => {
                OutcomePhaseV1::Moved
            }
        }
    }

    /// Движение, которое предписывает вердикт этого ранга.
    pub const fn verdict_movement(self) -> MovementV1 {
        self.phase().verdict_movement()
    }
}

/// Фаза разрешения, которую замыкает исход (раздел 6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OutcomePhaseV1 {
    /// Ранги 1–6: оценка кандидатов результата не даёт.
    NoEvaluationResult,
    /// Ранги 7–8: отмена уже выбранного движения.
    MovementCancelled,
    /// Ранги 9–12: движение не состоялось по результату оценки.
    MovementDeclined,
    /// Ранги 13–15: вердикт предписывает движение; состоялось ли оно, решает режим.
    Moved,
}

impl OutcomePhaseV1 {
    /// Полный перечень фаз в порядке рангов.
    pub const ALL: [Self; 4] = [
        Self::NoEvaluationResult,
        Self::MovementCancelled,
        Self::MovementDeclined,
        Self::Moved,
    ];

    /// Движение, которое предписывает вердикт этой фазы.
    ///
    /// Отмена движения возвращает слот к reference state, поэтому вердикт фазы
    /// [`OutcomePhaseV1::MovementCancelled`] движения не предписывает: выбор
    /// кандидата был сделан и снят, а предписание осталось пустым.
    pub const fn verdict_movement(self) -> MovementV1 {
        match self {
            Self::NoEvaluationResult | Self::MovementCancelled | Self::MovementDeclined => {
                MovementV1::Unchanged
            }
            Self::Moved => MovementV1::Moved,
        }
    }
}

/// Наличие движения слота.
///
/// Тип служит **двум** ролям, и различать их обязан вызывающий, потому что
/// имена вариантов у них общие:
///
/// * как результат `verdict_movement` — это движение, которое вердикт
///   **предписывает**; выполнено оно или нет, зависит от режима;
/// * как результат `byte_movement` отчёта — это уже **факт** о байтах слота.
///
/// Одно значение обслуживает обе роли намеренно: они различаются не формой, а
/// вопросом, на который отвечают, и вводить два одинаковых типа значило бы
/// плодить преобразования там, где содержания не прибавляется. Раздел 3
/// контракта связывает роли: факт есть предписание, отфильтрованное режимом.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MovementV1 {
    /// Слот остаётся на reference state.
    Unchanged,
    /// Слот сдвигается.
    Moved,
}

// Тотальный приоритет раздела 6 проверяется компилятором, а не прозой.
//
// Из попарной различности рангов следует попарная различность элементов `ALL`,
// то есть `ALL` содержит ровно 15 различных вариантов в точном порядке
// контракта. Сравнение идёт по рангам: `PartialEq` в `const` недоступен.
const _: () = {
    assert!(QualityOutcomeV1::ALL.len() == 15);
    assert!(OutcomePriorityV1::ALL.len() == 15);

    let mut index = 0;
    while index < QualityOutcomeV1::ALL.len() {
        let outcome = QualityOutcomeV1::ALL[index];
        let priority = outcome.priority();

        assert!(
            priority.rank() as usize == index + 1,
            "ранг разошёлся с позицией в ALL"
        );
        assert!(
            priority.outcome().priority().rank() == priority.rank(),
            "priority и outcome не взаимно обратны"
        );
        assert!(
            OutcomePriorityV1::ALL[index].rank() as usize == index + 1,
            "OutcomePriorityV1::ALL разошёлся с собственными рангами"
        );

        index += 1;
    }
};
