//! Лестница допуска раздела 9 контракта.
//!
//! Опубликованная формула сама по себе не получает права менять цвет. Право
//! даёт только верхняя ступень, `AutoActionAdmitted`, и это единственная
//! ступень, разрешающая production `auto`.
//!
//! # Почему ступень 4 — необитаемый тип
//!
//! Контракт пишет `DispositionV1` плоским списком из семи значений. Здесь
//! четыре ступени допуска вынесены в отдельный тип-полезную-нагрузку
//! [`AdmittedLevelV1`]. Расхождение с формой контракта намеренное и покупает
//! два свойства, которые проверяет компилятор, а не документация:
//!
//! 1. «Допущенные уровни включают предыдущие» выражается `Ord` на ступенях, а
//!    не прозой;
//! 2. носитель верхней ступени [`AutoActionAdmissionV1`] **необитаем**, поэтому
//!    значения `AdmittedLevelV1::AutoAction` не существует. «Сегодня не
//!    допущено ни одного профиля уровня 4» перестаёт быть утверждением
//!    `EVIDENCE.md` и становится свойством типа.
//!
//! Множество значений при этом изоморфно контрактному списку в тот день, когда
//! ступень 4 станет обитаемой. До того дня оно строго меньше — и это честно.
//!
//! # Что произойдёт в день первого допуска
//!
//! Появление первого варианта в [`AutoActionAdmissionV1`] даёт **ровно одну**
//! ошибку компиляции — `E0004` в теле [`AutoActionAdmissionV1::absurd`], где
//! `match self {}` перестаёт быть исчерпывающим. Проверено исполнением, а не
//! рассуждением.
//!
//! Крейт в этот день не соберётся, и это единственная машинная гарантия.
//! Перечня мест компилятор **не выдаёт**, поэтому перечислить их обязан этот
//! комментарий; все они держатся инвариантом, а не типом:
//!
//! * [`AdmittedLevelV1::ALL`] и [`DispositionV1::ALL`] — оба массива придётся
//!   расширить осознанно, молча они останутся валидными;
//! * `derive(PartialOrd, Ord)` на носителе — заведёт порядок над
//!   свидетельствами допуска по порядку объявления, то есть по произволу;
//! * реестр `REGISTRY` и ветвь `ProfileRegistryRowV1::Admitted`;
//! * ветвь допущенного профиля в законе действия отчёта;
//! * ветвь `DispositionV1::movement_authority`, вызывающая `absurd()`: при
//!   переписывании `absurd` она молча вернёт неверный ответ;
//! * `ordering` в отчёте: он безусловно кладёт «профилей ступени 4 нет», и при
//!   допущенном профиле это стало бы ложным публичным утверждением;
//! * обоснование отсутствия `CleanlinessConfigError` в отчёте: оно держится на
//!   пустоте `P` и вместе с ней истекает;
//! * шапка модуля `cleanliness`: она утверждает наблюдаемый исход для движимого
//!   слота, и в тот день утверждение станет ложным.
//!
//! Инвариант, который обязан пережить этот день: единственный вход в
//! [`MovementAuthorityV1`] — свидетельство ступени 4, и ни одна research-метка
//! такого свидетельства не образует.

/// Ступень лестницы допуска раздела 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DispositionV1 {
    /// Кандидат: заявлен, экзамен не пройден.
    Candidate,
    /// Отклонён по результату экзамена.
    Rejected,
    /// Экзамен неприменим: домен корпуса не совпадает с доменом заявки.
    ExamNotApplicable,
    /// Допущен до названной ступени.
    Admitted(AdmittedLevelV1),
}

impl DispositionV1 {
    /// Полный перечень **обитаемых** значений.
    ///
    /// Ступени `AutoAction` здесь нет и быть не может: её носитель необитаем.
    /// Массив физически не способен её содержать — это и есть машинная запись
    /// «сегодня профилей уровня 4 нет».
    pub const ALL: [Self; 6] = [
        Self::Candidate,
        Self::Rejected,
        Self::ExamNotApplicable,
        Self::Admitted(AdmittedLevelV1::Math),
        Self::Admitted(AdmittedLevelV1::Correlate),
        Self::Admitted(AdmittedLevelV1::Decision),
    ];

    /// Стабильный ключ для certificate и `EVIDENCE.md`.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Candidate => "candidate",
            Self::Rejected => "rejected",
            Self::ExamNotApplicable => "exam-not-applicable",
            Self::Admitted(level) => level.key(),
        }
    }

    /// Вправе ли эта диспозиция двигать цвет.
    ///
    /// Сегодня ответ — `None` для любого значения, и это не соглашение, а
    /// следствие: ветвь ступени 4 связывает необитаемое свидетельство, поэтому
    /// исполнена быть не может. Она разрешается через
    /// [`AutoActionAdmissionV1::absurd`], а не конструирует право — иначе
    /// компилятор справедливо ругался бы на недостижимое выражение.
    pub const fn movement_authority(self) -> Option<MovementAuthorityV1> {
        match self {
            Self::Admitted(AdmittedLevelV1::AutoAction(admission)) => admission.absurd(),
            _ => None,
        }
    }
}

/// Ступени допуска 1–4 раздела 9.
///
/// Допущенные уровни включают предыдущие, и `Ord` выражает именно это
/// включение: `Math < Correlate < Decision < AutoAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AdmittedLevelV1 {
    /// Ступень 1: формула, геометрия, единицы, численные границы, оракул.
    Math,
    /// Ступень 2: прямой конструкт, сходимость источников, применимость и
    /// неопределённость. Разрешает `lint`, но не движение.
    Correlate,
    /// Ступень 3: валидированный закон классификации либо риска.
    Decision,
    /// Ступень 4 — **единственная**, разрешающая production `auto`.
    ///
    /// Носитель необитаем, поэтому значения этого варианта не существует.
    AutoAction(AutoActionAdmissionV1),
}

impl AdmittedLevelV1 {
    /// Полный перечень **обитаемых** ступеней.
    pub const ALL: [Self; 3] = [Self::Math, Self::Correlate, Self::Decision];

    /// Стабильный ключ ступени.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Math => "math-admitted",
            Self::Correlate => "correlate-admitted",
            Self::Decision => "decision-admitted",
            Self::AutoAction(_) => "auto-action-admitted",
        }
    }
}

/// Свидетельство допуска до `AutoActionAdmitted` (раздел 9, пункт 4).
///
/// Тип **необитаем**: ни один release не удовлетворяет условиям допуска —
/// опубликованные данные подтверждают направление выбора, `NonRegressing
/// protected outcomes` на полном declared support, действие строго bounded
/// внутри observed support, зарегистрированный sign-site и невакуумный допуск.
///
/// Экзамен на замороженном holdout кандидаты не прошли, поэтому вариантов
/// здесь нет. Это не заглушка: отсутствие варианта — и есть текущее состояние
/// доказательной базы, записанное так, что компилятор его проверяет.
///
/// # Обязательство на день первого допуска
///
/// `PartialOrd`/`Ord` выведены потому, что [`AdmittedLevelV1`] обязан быть
/// упорядочен (включение ступеней), а он этот тип содержит. Сегодня порядок
/// инертен — сравнивать нечего. Как только появится первый вариант, порядок
/// заведётся **молча** и будет задан порядком объявления, то есть произволом.
/// Свидетельства допуска сами по себе не упорядочены, поэтому в тот день
/// derive обязан быть пересмотрен, а не унаследован.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AutoActionAdmissionV1 {}

impl AutoActionAdmissionV1 {
    /// Значения не существует, поэтому вызов недостижим.
    ///
    /// Функция нужна затем, чтобы недостижимость выражалась типом `!`, а не
    /// подразумевалась комментарием: ветвь, разрешённая через `absurd`, ничего
    /// не конструирует и в недостижимое выражение не превращается.
    ///
    /// `match self {}` здесь — единственная во всём модуле точка, которая
    /// перестанет компилироваться, когда носитель станет обитаемым.
    pub const fn absurd(self) -> ! {
        match self {}
    }
}

/// Право двигать цвет.
///
/// Поле приватно, и единственный конструктор —
/// [`MovementAuthorityV1::from_admission`], принимающий свидетельство ступени
/// 4. Иных входов нет: ни `From`, ни `Default`, ни публичного литерала.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MovementAuthorityV1(AutoActionAdmissionV1);

impl MovementAuthorityV1 {
    /// Единственный конструктор — только из свидетельства ступени 4.
    pub const fn from_admission(admission: AutoActionAdmissionV1) -> Self {
        Self(admission)
    }

    /// Значения не существует, поэтому вызов недостижим.
    pub const fn absurd(self) -> ! {
        self.0.absurd()
    }
}

/// Research-метка `EVIDENCE.md` (раздел 9).
///
/// Описывает состояние исследования и **не даёт никаких прав**: конверсии в
/// [`AdmittedLevelV1`], [`AutoActionAdmissionV1`] и [`MovementAuthorityV1`] не
/// существует.
///
/// Прочность этих трёх запретов разная, и путать их не следует. Написать `impl
/// From<ResearchLabelV1>` можно для любого из трёх — тело `loop {}` соберётся и
/// для необитаемого типа. Различие не в написании impl, а в том, способен ли он
/// **вернуть значение**: для [`AutoActionAdmissionV1`] и [`MovementAuthorityV1`]
/// не способен ни один, потому что значения не существует. Для
/// [`AdmittedLevelV1`] способен: тип обитаем. Права двигать цвет это всё равно
/// не даёт — право берётся только со ступени 4, — но запрет там держится ревью,
/// а не невозможностью.
///
/// Список закрыт семью значениями: метка, которую нельзя назвать, не должна
/// существовать.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResearchLabelV1 {
    /// Опубликованный кандидат.
    PublishedCandidate,
    /// Опубликованный кандидат в профиль.
    PublishedProfileCandidate,
    /// Кандидат, объявленный после вскрытия данных.
    PostHocDeclaredCandidate,
    /// Цель протокола принятия решения.
    DecisionProtocolTarget,
    /// Гипотеза.
    Hypothesis,
    /// Разведка смежного конструкта.
    AdjacentConstructExploratory,
    /// Те же стимулы, другая целевая шкала.
    SameStimuliDifferentTarget,
}

impl ResearchLabelV1 {
    /// Полный перечень research-меток.
    pub const ALL: [Self; 7] = [
        Self::PublishedCandidate,
        Self::PublishedProfileCandidate,
        Self::PostHocDeclaredCandidate,
        Self::DecisionProtocolTarget,
        Self::Hypothesis,
        Self::AdjacentConstructExploratory,
        Self::SameStimuliDifferentTarget,
    ];

    /// Стабильный ключ метки.
    pub const fn key(self) -> &'static str {
        match self {
            Self::PublishedCandidate => "published-candidate",
            Self::PublishedProfileCandidate => "published-profile-candidate",
            Self::PostHocDeclaredCandidate => "post-hoc-declared-candidate",
            Self::DecisionProtocolTarget => "decision-protocol-target",
            Self::Hypothesis => "hypothesis",
            Self::AdjacentConstructExploratory => "adjacent-construct-exploratory",
            Self::SameStimuliDifferentTarget => "same-stimuli-different-target",
        }
    }
}

// Ни одна перечисленная диспозиция не даёт права двигать цвет.
//
// Область этой проверки ровно такова: она обходит `DispositionV1::ALL` и ничего
// не говорит о значениях вне массива. Расширять `ALL` она не заставляет —
// добавление варианта в носитель ступени 4 оставит её зелёной. Сторожем того
// дня служит не она, а `absurd()`: его `match self {}` перестанет быть
// исчерпывающим, и крейт не соберётся. Ошибка при этом одна и указывает на тело
// `absurd`, а не на точки вызова — перечень мест см. в шапке модуля.
const _: () = {
    let mut index = 0;
    while index < DispositionV1::ALL.len() {
        assert!(
            DispositionV1::ALL[index].movement_authority().is_none(),
            "право двигать цвет не может существовать без свидетельства ступени 4"
        );
        index += 1;
    }
};
