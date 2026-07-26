//! Отчёт о качестве слота (разделы 3, 6 и 11 контракта).
//!
//! # Один закон, различающийся последним шагом
//!
//! `lint` и `auto` — один закон. Оценка одна: тот же reference state, тот же
//! candidate domain, тот же допуск, те же знаки, то же правило доминирования и
//! тот же исход. Режим решает единственный вопрос — применить выбранное или
//! только сообщить о нём.
//!
//! Здесь это выражено структурно, а не комментарием: [`evaluate_slot_v1`]
//! **не принимает режим на вход**. Двух путей оценки не существует, потому что
//! существует ровно одна функция оценки, и она о режиме не знает. Ветвление по
//! режиму живёт исключительно в [`slot_quality_v1`], в шаге применения.
//!
//! Канал отчёта шире канала действия: `lint` дополнительно сообщает evidence
//! профилей уровней 1–3. Второй веткой оценки это не является — такое evidence
//! не входит в `P`, вердикта не меняет и в выбор кандидата не попадает.
//!
//! # Исход есть вердикт, а не факт записи байтов
//!
//! В `auto` вердикт применён, поэтому `Improved` означает, что байты слота
//! изменились. В `lint` тот же вердикт означает «`auto` изменил бы их здесь и
//! вот так», а слот остаётся нетронутым по определению режима. Поэтому
//! движение байтов вычисляется по паре «исход и режим» — см.
//! [`QualityReportV1::byte_movement`].

use super::outcome::{MovementV1, QualityOutcomeV1};
use super::registry::{AppearanceModeV1, movement_authority_v1};

/// Единственный переключатель раздела 3. Свойство объявленного слота
/// программы, а не глобальный аргумент вызова.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum QualityModeV1 {
    /// Human convention выключена; значения `QualityOutcome` не порождается
    /// вовсе. Обязательная physical correctness этим не затрагивается.
    Off,
    /// Состояние слота не меняется; вердикт вычисляется и сообщается.
    Lint,
    /// Вердикт вычисляется и применяется к solver-owned degrees of freedom.
    Auto,
}

impl QualityModeV1 {
    /// Полный перечень режимов.
    pub const ALL: [Self; 3] = [Self::Off, Self::Lint, Self::Auto];

    /// Стабильный ключ режима.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Lint => "lint",
            Self::Auto => "auto",
        }
    }

    /// Применяет ли режим вычисленный вердикт.
    ///
    /// Это **единственное**, чем режимы различаются в законе действия.
    pub const fn applies_verdict(self) -> bool {
        matches!(self, Self::Auto)
    }
}

/// Класс слота, различаемый разделом 3 и рангом 1 раздела 6.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SlotClassV1 {
    /// Client anchor либо hard association binding: неприкосновенен при любом
    /// режиме.
    Immutable,
    /// Слот, который `auto` вправе двигать при наличии полномочий.
    Movable,
}

impl SlotClassV1 {
    /// Полный перечень классов слота.
    pub const ALL: [Self; 2] = [Self::Immutable, Self::Movable];
}

/// Profile-wise ordering раздела 11.
///
/// Универсального score не существует (раздел 4), поэтому тип не несёт ни
/// одного числового поля и не может приобрести его незаметно.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProfileWiseOrderingV1 {
    /// Для этого режима не допущено ни одного профиля ступени 4.
    ///
    /// Утверждение об **отсутствии допуска**, а не о выходе за область
    /// применимости: подмена запрещена разделом 5.7.
    NoApplicableProfile(AppearanceModeV1),
}

/// Отчёт раздела 11 для одного слота.
///
/// Порядка не имеет намеренно: отчёт несёт вердикт, а вердикты между собой не
/// сравниваются — это было бы скрытым утверждением о порядке, которого раздел
/// 4 не допускает. Сравнивать следует приоритеты правила разрешения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QualityReportV1 {
    mode: QualityModeV1,
    appearance_mode: AppearanceModeV1,
    slot_class: SlotClassV1,
    outcome: QualityOutcomeV1,
    ordering: ProfileWiseOrderingV1,
}

impl QualityReportV1 {
    /// Режим, в котором получен отчёт.
    pub const fn mode(self) -> QualityModeV1 {
        self.mode
    }

    /// Appearance mode слота.
    pub const fn appearance_mode(self) -> AppearanceModeV1 {
        self.appearance_mode
    }

    /// Класс слота.
    pub const fn slot_class(self) -> SlotClassV1 {
        self.slot_class
    }

    /// Вердикт оценки.
    pub const fn outcome(self) -> QualityOutcomeV1 {
        self.outcome
    }

    /// Profile-wise ordering.
    pub const fn ordering(self) -> ProfileWiseOrderingV1 {
        self.ordering
    }

    /// Изменились ли emitted-байты слота.
    ///
    /// Функция пары «исход и режим», а не одного исхода: `lint` не применяет
    /// вердикт, поэтому байты не меняются ни при каком вердикте.
    pub const fn byte_movement(self) -> MovementV1 {
        if self.mode.applies_verdict() {
            self.outcome.verdict_movement()
        } else {
            MovementV1::Unchanged
        }
    }
}

/// Наблюдаемый результат режима качества на слоте. Тотален, без `Option`.
///
/// Порядка не имеет по той же причине, что и [`QualityReportV1`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SlotQualityV1 {
    /// `off`: человеческая конвенция выключена, отчёта по ней нет.
    ConventionDisabled,
    /// `lint` либо `auto`: тотальный отчёт.
    Reported(QualityReportV1),
}

/// Закон действия: вердикт для слота при текущем реестре.
///
/// **Режим на вход не принимается намеренно.** Это и есть структурная запись
/// требования раздела 3: закон один, и он о режиме не знает.
///
/// Вывод, а не постулат. Множество `P` профилей ступени 4 для любого режима
/// сегодня пусто (реестр), поэтому доминирование ложно для любого кандидата,
/// `L3` пусто, и по разделу 5.7 исход — `UnchangedNoAdmittedProfile`. Ранг 1
/// (`UnchangedImmutable`) пер-слотный и старше, поэтому перехватывает раньше.
pub const fn evaluate_slot_v1(
    appearance_mode: AppearanceModeV1,
    slot_class: SlotClassV1,
) -> QualityOutcomeV1 {
    match slot_class {
        SlotClassV1::Immutable => QualityOutcomeV1::UnchangedImmutable,
        SlotClassV1::Movable => match movement_authority_v1(appearance_mode) {
            None => QualityOutcomeV1::UnchangedNoAdmittedProfile,
            // Недостижимо сегодня: права двигать цвет не существует. Ветвь
            // существует затем, чтобы в день первого допуска компилятор
            // потребовал её дописать, а не чтобы молчать.
            Some(authority) => authority.absurd(),
        },
    }
}

/// Тотальный отчёт по слоту: оценка плюс шаг применения.
///
/// Ветвление по режиму находится **только** здесь и касается только того,
/// применяется ли вердикт. Сам вердикт получен [`evaluate_slot_v1`], которая о
/// режиме не знает.
pub const fn slot_quality_v1(
    mode: QualityModeV1,
    appearance_mode: AppearanceModeV1,
    slot_class: SlotClassV1,
) -> SlotQualityV1 {
    match mode {
        QualityModeV1::Off => SlotQualityV1::ConventionDisabled,
        QualityModeV1::Lint | QualityModeV1::Auto => SlotQualityV1::Reported(QualityReportV1 {
            mode,
            appearance_mode,
            slot_class,
            outcome: evaluate_slot_v1(appearance_mode, slot_class),
            ordering: ProfileWiseOrderingV1::NoApplicableProfile(appearance_mode),
        }),
    }
}

// Вердикт не зависит от режима — структурное требование раздела 3,
// зафиксированное на этапе компиляции. `evaluate_slot_v1` режим не принимает,
// поэтому `lint` и `auto` обязаны получать одно и то же значение.
const _: () = {
    let mut mode_index = 0;
    while mode_index < AppearanceModeV1::ALL.len() {
        let appearance_mode = AppearanceModeV1::ALL[mode_index];

        let mut class_index = 0;
        while class_index < SlotClassV1::ALL.len() {
            let slot_class = SlotClassV1::ALL[class_index];
            let verdict = evaluate_slot_v1(appearance_mode, slot_class);

            match slot_quality_v1(QualityModeV1::Lint, appearance_mode, slot_class) {
                SlotQualityV1::Reported(report) => {
                    assert!(
                        report.outcome().priority().rank() == verdict.priority().rank(),
                        "lint обязан сообщать тот же вердикт, что даёт закон действия"
                    );
                    assert!(
                        matches!(report.byte_movement(), MovementV1::Unchanged),
                        "lint не вправе двигать байты ни при каком вердикте"
                    );
                }
                SlotQualityV1::ConventionDisabled => {
                    panic!("lint обязан порождать отчёт")
                }
            }

            match slot_quality_v1(QualityModeV1::Auto, appearance_mode, slot_class) {
                SlotQualityV1::Reported(report) => {
                    assert!(
                        report.outcome().priority().rank() == verdict.priority().rank(),
                        "auto обязан применять тот же вердикт, что даёт закон действия"
                    );
                }
                SlotQualityV1::ConventionDisabled => {
                    panic!("auto обязан порождать отчёт")
                }
            }

            match slot_quality_v1(QualityModeV1::Off, appearance_mode, slot_class) {
                SlotQualityV1::ConventionDisabled => {}
                SlotQualityV1::Reported(_) => {
                    panic!("off не вправе порождать значение QualityOutcome")
                }
            }

            class_index += 1;
        }
        mode_index += 1;
    }
};
