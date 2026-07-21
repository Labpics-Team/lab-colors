//! Граница конфига: типы, которыми потребитель движка (дизайн-система) задаёт
//! свою семантику, и компиляция этой семантики в физическую [`NamedRoleTable`].
//!
//! Разделение зон (ADR-0001, `docs/decisions/0001-config-boundary.md`): движок не
//! знает ни одного имени роли и ни одного брендового значения. Всё, что меняется
//! при смене клиента студии, живёт здесь, в [`ThemeConfig`]; всё, что является
//! законом восприятия / математикой / WCAG, остаётся физикой ядра
//! ([`crate::semantic`], [`crate::solve`](mod@crate::solve)). Направление зависимостей — только
//! внутрь: этот модуль знает про доменные типы ядра ([`RoleSpec`],
//! [`RoleChroma`], [`DjMagnitude`], [`TextAnchor`], [`NamedRoleTable`]), а ядро
//! про конфиг не знает ничего.
//!
//! # Что этот модуль делает
//!
//! - Несёт типы конфига без сериализации ([`ThemeConfig`] и вложенные) — JSON-парсинг
//!   это забота границы WASM, не ядра.
//! - [`ThemeConfig::validate`] проверяет пределы КАЖДОЙ экспонируемой ручки: значение
//!   вне предела возвращает [`ConfigError`], а не тихо принимается. Клиент не может
//!   молча сломать различимость или WCAG-полы.
//! - [`ThemeConfig::compile_named_role_table`] компилирует роли в [`NamedRoleTable`],
//!   которую [`crate::semantic::resolve_named_set`] резолвит той же физикой, что и
//!   встроенную `crate::RoleTable`.
//!
//! # Рецепты лестницы и альфа-аналога
//!
//! [`RoleRecipe::Ladder`] (семейная/бренд-лестница)
//! компилируется в [`RoleSpec::Ladder`]: источник раскладывается в пер-темный
//! тинт-якорь ([`crate::ladder::LadderTint`]), позиция несёт альфу Figma-рампы.
//! [`RoleRecipe::AlphaAnalog`] компилируется в [`RoleSpec::AlphaAnalog`] (солид-
//! цель источника + запрошенная альфа, exact encoded-sRGB8 identity — [`crate::alpha`]).
//! Резолв обоих — [`crate::semantic::Resolved::Translucent`] (тинт×альфа напрямую + солид-
//! композит на фоне резолва для замера контраста). Исполняемый канон позиций и
//! альф — [`LadderPosition::ALL`] и [`LadderPosition::alpha_pair`].
//!
//! # Агностичность: конфиг несёт СВОЙ словарь ролей
//!
//! Ядро не знает ни одной роли дизайн-системы — [`ThemeConfig`] обязан нести
//! собственные `roles`/`aliases` (клиент вносит и значения, и семантику). Пустой
//! контракт (без `roles` и без `aliases`) отклоняется на загрузке —
//! [`ConfigError::EmptyContract`]. Текущая схема принимает фоновые якоря как есть;
//! произвольная graph/constraint-топология ещё не является публичным API.

use crate::Srgb8;
use crate::ladder::{LadderPosition, LadderTint, ThemeAnchors};
use crate::semantic::{
    DECORATIVE_FLOOR_MIN, DjMagnitude, NamedRoleTable, RoleChroma, RoleSpec, TextAnchor,
    validate_ladder_floor,
};
use crate::solve::Floor;

// ─────────────────────────────────────────────────────────────────────────────
// Пределы валидатора.
//
// Проверки доменов объявленных ручек разделены явно: доли и alpha имеют
// нормированный интервал, величины шага и красочности положительны, жёсткость
// неотрицательна, а hue принадлежит каноническому угловому интервалу. Эти
// проверки задают область входных типов, а не обосновывают перцептивный порог.
// Отдельная строка `DECORATIVE_FLOOR_BOUND` лишь форматирует числовой SSOT
// `DECORATIVE_FLOOR_MIN` из `semantic.rs`; `docs/empirical-inventory.md`
// хранит его provenance. Конфиг не определяет и не обосновывает эту политику.
// ─────────────────────────────────────────────────────────────────────────────

/// Доля от максимального контраста фона ([`TextAnchor`]) обязана лежать в `(0, 1]`.
///
/// `fraction · max` — целевой контраст текстовой роли. При `fraction ≤ 0` цель
/// нулевая (роль нечитаема), при `fraction > 1` цель превышает достижимый максимум
/// фона (всегда [`crate::SolveFailure`]). Верхняя граница включает `1.0` — это
/// физический максимум контраста фона; движок хранит это значение без переписи.
const FRACTION_MIN_EXCLUSIVE: f64 = 0.0;
/// Верхний предел доли текстового якоря (включительно).
const FRACTION_MAX_INCLUSIVE: f64 = 1.0;

/// dJ'-якорь декоративной роли обязан быть строго положительным.
///
/// dJ' — перцептивная разница светлоты (шаг `J'`), которую роль держит от своей
/// поверхности. `dj ≤ 0` означает «нет различимого шага» — вырожденная роль,
/// неотличимая от фона. Верхнего предела нет по величине как таковой: физика сама
/// вернёт [`crate::SolveFailure`], если якорь недостижим на данном фоне, — но
/// нулевой/отрицательный якорь это ошибка КОНФИГА, а не недостижимость.
const DJ_MIN_EXCLUSIVE: f64 = 0.0;

/// Человекочитаемая проекция [`DECORATIVE_FLOOR_MIN`] на границе конфига.
///
/// Rust не умеет `stringify!` значения именованной константы; тест ниже строит
/// ожидаемый текст из числового SSOT и не позволяет литералам разойтись.
const DECORATIVE_FLOOR_BOUND: &str = "magnitude ≥ 7.5 Lc (граница декоративной Lc-цели)";

// Lc-величина декоративной роли (тени) обязана лежать не ниже физического
// декоративного пола ядра. Единица — переходный Ys candidate score `Lc`; знак выбирает
// физика от фона, поэтому конфиг несёт величину (модуль). Значение ниже
// `DECORATIVE_FLOOR_MIN` попадает в квантованный low-contrast gap; ядро не
// переписывает такую декларацию в другой контракт, а отклоняет её на загрузке.

/// Целевая красочность подтона (`neutral.tint.target_mp`, CAM16-UCS `M'`) обязана
/// быть строго положительной.
///
/// `M'` — перцептивная красочность; отрицательная или нулевая цель вырождает
/// chromatic curve. Exact-gray anchor выбирает [`RoleChroma::Neutral`] до
/// применения этой ручки. Предел лишь отсекает нефизичное `≤ 0`.
const TARGET_MP_MIN_EXCLUSIVE: f64 = 0.0;

/// Жёсткость прижатия оттенка (`neutral.tint.hue_stiffness`) обязана быть
/// неотрицательной.
///
/// Штраф дрейфа оттенка масштабируется как `stiffness / 100` (см.
/// [`cusp_attracted_hue`](crate::semantic)). `stiffness = 0` — оттенок свободно
/// идёт к локальному каспу хромы (допустимо), рост — прижимает к каноническому.
/// Отрицательная жёсткость инвертировала бы штраф в награду за уход от канона —
/// нефизично.
const HUE_STIFFNESS_MIN_INCLUSIVE: f64 = 0.0;

/// Запрошенная альфа альфа-аналога (`roles.*.alpha`) обязана лежать в `(0, 1]`.
///
/// `≤ 0` — невидимая роль (вырождение), `> 1` — не альфа. Резолвер поднимает
/// фактическую α до `α_min`, если запрошенная ниже минимально-разрешимой в
/// гамуте ([`crate::alpha::resolve_alpha_analog`]) — но сам запрос должен быть
/// валидной альфой.
const ALPHA_MIN_EXCLUSIVE: f64 = 0.0;
/// Верхний предел запрошенной альфы (включительно; α = 1 = солид).
const ALPHA_MAX_INCLUSIVE: f64 = 1.0;

use crate::spaces::oklab::{HUE_DEG_MAX_EXCLUSIVE, HUE_DEG_MIN_INCLUSIVE, OklabHue, hue_of_srgb8};

// ─────────────────────────────────────────────────────────────────────────────
// Ошибки валидации конфига.
// ─────────────────────────────────────────────────────────────────────────────

/// Ошибка компиляции или валидации [`ThemeConfig`].
///
/// Матчится по вариантам (это часть публичного API ядра): потребитель различает
/// «невалидный hex», «ручка вне предела» и «ссылка на несуществующее семейство».
/// Реализована вручную (без `thiserror`) — крейт
/// `labcolors-core` держит ноль runtime-зависимостей, поэтому `Display`
/// повторяет стиль ручных ошибок ядра.
///
/// NB: `PartialEq` наследует IEEE-семантику `f64`-полей —
/// `OutOfBounds { value: NaN, .. }` НЕ равен самому себе (как и сам `f64`);
/// для сравнения NaN-ошибок матчитесь по варианту, не по равенству.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ConfigError {
    /// Невалидная hex-строка цвета (принимается только `#RRGGBB`). `field` — путь до поля в
    /// конфиге, `value` — то, что прислал клиент.
    InvalidHex { field: String, value: String },
    /// Невалидное имя роли или семейства: имена — стабильный CSS-контракт
    /// (`--lab-{имя}`), допустимо только `[a-z0-9-]+` и не пусто.
    InvalidName { field: String, value: String },
    /// Ссылка на семейство палитры, которого нет в `palette`.
    UnknownFamily {
        referenced_by: String,
        family: String,
    },
    /// Ссылка (алиас/alpha_analog) на роль, которой нет в `roles`.
    UnknownRole { referenced_by: String, role: String },
    /// Дубликат ключа в словаре конфига или итоговом CSS-namespace: повтор имени
    /// сделал бы lookup/эмиссию неоднозначными (какая запись выиграла — вопрос
    /// порядка, тихо).
    DuplicateKey {
        dictionary: &'static str,
        key: String,
    },
    /// Роль требует пер-темной нейтральной четвёрки (edge/inverted), которой в
    /// конфиге нет — дублирование одного края дало бы невидимую роль.
    MissingNeutralAnchors {
        referenced_by: String,
        field: &'static str,
    },
    /// Значение ручки вне допустимого предела. `handle` — путь до ручки, `bound` —
    /// человеко-читаемое описание нарушенного предела с обоснованием.
    OutOfBounds {
        handle: String,
        value: f64,
        bound: &'static str,
    },
    /// Контракт пуст: ни ролей, ни алиасов. Резолв не эмитил бы ни одной роли —
    /// молчаливый приём увёл бы дефект на использование. Отказ обязан быть НА
    /// ЗАГРУЗКЕ (`#[serde(default)]` на `roles` на границе WASM разрешает ОПУСТИТЬ
    /// словарь синтаксически, но не остаться совсем без контракта).
    EmptyContract,
    /// Словарь тем пуст. Симметрия с [`EmptyContract`](Self::EmptyContract):
    /// без единой темы `resolve`/`recheck` тотально неработоспособны (любой
    /// ключ — unknown), и дефект уехал бы на использование неотличимым от
    /// опечатки. Отказ на загрузке.
    EmptyThemes,
    /// `material`-рецепту передан `floor: zero` — у материала нет цели для вывода
    /// альфы без пола читаемости. Отказ на загрузке (а не молчаливая невидимая
    /// роль): материал обязан нести `aa-text` или `aa-ui`.
    MaterialFloorRequired { role: String },
    /// Readability-floor лестницы объявлен для полупрозрачной позиции либо как
    /// no-op `zero`. Публичный floor сейчас определён только для opaque
    /// occurrence; для translucent не объявлено, какую occurrence ограничивать
    /// и разрешено ли менять tint, alpha или оба параметра.
    InvalidLadderFloor { role: String, reason: &'static str },
    /// Рецепт и общетабличная chroma-policy противоречат друг другу. Ошибка
    /// принадлежит preflight: executable-таблица не должна откладывать её до
    /// конкретной темы или первого runtime-resolve.
    IncompatibleRolePolicy { role: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::InvalidHex { field, value } => {
                write!(f, "невалидный hex в поле `{field}`: {value:?}")
            }
            ConfigError::InvalidName { field, value } => write!(
                f,
                "невалидное имя в поле `{field}`: {value:?} (допустимо [a-z0-9-]+, не пусто)"
            ),
            ConfigError::UnknownRole {
                referenced_by,
                role,
            } => write!(
                f,
                "`{referenced_by}` ссылается на роль `{role}`, которой нет в roles"
            ),
            ConfigError::MissingNeutralAnchors {
                referenced_by,
                field,
            } => write!(
                f,
                "`{referenced_by}` требует пер-темной нейтральной четвёрки `{field}`, которой нет в конфиге"
            ),
            ConfigError::DuplicateKey { dictionary, key } => write!(
                f,
                "дубликат ключа `{key}` в словаре `{dictionary}` — lookup был бы неоднозначным"
            ),
            ConfigError::UnknownFamily {
                referenced_by,
                family,
            } => write!(
                f,
                "роль `{referenced_by}` ссылается на семейство `{family}`, которого нет в palette"
            ),
            ConfigError::OutOfBounds {
                handle,
                value,
                bound,
            } => write!(f, "ручка `{handle}` = {value} вне предела: {bound}"),
            ConfigError::EmptyContract => write!(f, "контракт пуст: передайте roles"),
            ConfigError::EmptyThemes => write!(f, "словарь тем пуст: передайте themes"),
            ConfigError::MaterialFloorRequired { role } => write!(
                f,
                "material-роль `{role}` требует пол читаемости (aa-text/aa-ui), получен zero-floor"
            ),
            ConfigError::InvalidLadderFloor { role, reason } => {
                write!(f, "ladder-роль `{role}` несёт невалидный floor: {reason}")
            }
            ConfigError::IncompatibleRolePolicy { role } => {
                write!(f, "material `{role}`: ")?;
                f.write_str(RoleSpec::INCOMPATIBLE_CHROMA_REASON)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

// ─────────────────────────────────────────────────────────────────────────────
// Типы конфига (без serde — JSON-парсинг живёт на границе WASM).
// ─────────────────────────────────────────────────────────────────────────────

/// Бренд — вход, не роль: пер-темные якорные hex. Оттенок движок выводит физикой;
/// лестница бренда ([`RoleRecipe::Ladder`] с [`LadderSource::Brand`]) эмитит
/// `rgba(якорь, α)` напрямую, а якорь берётся по теме резолва.
///
/// Пер-темность (а не один якорь + вывод) — из заземления
/// (`reference/labui-accent-primitives.md`, раздел «Якоря»: Brand light `#007AFF` /
/// dark `#4A8FFF` / light-ic `#0040DD` / dark-ic `#409CFF`): тёмный/IC-вариант
/// измерен, не выведен из светлого.
#[derive(Debug, Clone, PartialEq)]
pub struct Brand {
    /// Пер-темные якорные цвета бренда. Дефолта в ядре нет.
    pub anchors: ThemeAnchors,
}

/// Тройка якорей нейтральной шкалы: конфиг несёт ИЗМЕРЕННОЕ, движок выводит
/// производное (hue, кривую).
#[derive(Debug, Clone, PartialEq)]
pub struct NeutralAnchors {
    /// Светлый край нейтральной шкалы (labui: `#FFFFFF`).
    pub light: String,
    /// Середина нейтральной шкалы (labui: `#787880`).
    pub mid: String,
    /// Тёмный край нейтральной шкалы (labui: `#101012`).
    pub dark: String,
}

/// Ручки нейтрального подтона (политика силы и удержания оттенка).
#[derive(Debug, Clone, PartialEq)]
pub struct NeutralTint {
    /// Целевая перцептивная красочность CAM16-UCS `M'` (v2-кривая, «сила»): `> 0`.
    pub target_mp: f64,
    /// Жёсткость прижатия оттенка к каноническому (v2-кривая): `≥ 0`.
    pub hue_stiffness: f64,
    /// Явный оттенок подтона (градусы `[0, 360)`), если у потребителя есть
    /// ИЗМЕРЕННАЯ величина (labui: SSOT 286.0°). `None` — движок классифицирует
    /// точные sRGB8-байты тёмного якоря: равные каналы выбирают neutral policy,
    /// неравные выводят направление hue (`#101012` → 285.97°).
    pub hue_override_deg: Option<f64>,
}

/// Нейтраль: тройка якорей + ручки подтона.
#[derive(Debug, Clone, PartialEq)]
pub struct NeutralConfig {
    /// Тройка hex-якорей нейтральной шкалы.
    pub anchors: NeutralAnchors,
    /// Ручки подтона.
    pub tint: NeutralTint,
    /// Пер-темный «контурный» край нейтрали (контрастный теме: светлая тема —
    /// тёмный контур, тёмная — почти белый; labui: #101012 / #F6F8FA). Нужен
    /// ролям типа кольца фокуса; без поля [`NeutralPick::Edge`] даёт ошибку
    /// конфига, не выдуманное значение.
    pub edge: Option<crate::ladder::ThemeAnchors>,
    /// Пер-темный «инвертированный» средний тон (labui: #B0B0B9 / #3C3C43) —
    /// для свечения на инвертированной поверхности; без поля
    /// [`NeutralPick::Inverted`] даёт ошибку конфига.
    pub inverted: Option<crate::ladder::ThemeAnchors>,
}

/// Именованное семейство палитры: ключ + пер-темные якорные hex.
///
/// Якорь несётся отдельно для каждого режима (light/dark/light-ic/dark-ic):
/// заземление `reference/labui-accent-primitives.md`, раздел «Якоря», показывает, что тёмный и
/// IC-варианты Figma-примитивов `Accent/*` замерены, а не выведены из светлого
/// (Red light `#FF3B30` / dark `#FF3A3A` / light-ic `#D70015` / dark-ic `#FF6161`).
/// Лестница семейства ([`RoleRecipe::Ladder`] с [`LadderSource::Family`]) выбирает
/// якорь по теме резолва ([`ThemeAnchors::for_vc`]).
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteFamily {
    /// Стабильный ключ семейства (`[a-z0-9-]+`), напр. `red`.
    pub key: String,
    /// Пер-темные якорные цвета семейства.
    pub anchors: ThemeAnchors,
}

/// Пресет условий просмотра из ЗАКРЫТОГО физического меню движка.
///
/// Произвольных VC-чисел в конфиге нет — только выбор из четырёх калиброванных
/// режимов. Соответствие [`ViewingConditions`](crate::ViewingConditions):
/// `Srgb` → average surround, `Dim` → dim surround, `SrgbIc` / `DimIc` — то же с
/// флагом повышенного контраста (IC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcPreset {
    /// Average surround (светлая тема).
    Srgb,
    /// Dim surround (тёмная тема).
    Dim,
    /// Average surround с повышенным контрастом (IC).
    SrgbIc,
    /// Dim surround с повышенным контрастом (IC).
    DimIc,
}

impl VcPreset {
    /// Условия просмотра, под которыми ядро резолвит для этого пресета.
    pub fn viewing_conditions(self) -> crate::ViewingConditions {
        match self {
            VcPreset::Srgb => crate::ViewingConditions::srgb(),
            VcPreset::Dim => crate::ViewingConditions::dim_surround(),
            VcPreset::SrgbIc => crate::ViewingConditions::srgb_high_contrast(),
            VcPreset::DimIc => crate::ViewingConditions::dim_surround_high_contrast(),
        }
    }
}

/// Словарь тем: имя → пресет условий просмотра.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemesConfig {
    /// Пары `(имя темы, VC-пресет)` в порядке объявления.
    pub entries: Vec<(String, VcPreset)>,
}

/// Рецепт роли из закрытого физического меню текущего resolver-а.
///
/// Это текущая pre-cutover грамматика, не target IR и не extension point.
/// Новая физика не добавляется новым recipe variant. Вариант считается
/// мигрированным только после одностороннего lowering в общий compiled graph и
/// удаления его прежней исполняемой ветви.
///
/// Все рецепты компилируются в [`RoleSpec`]: текст/dJ'/Ys candidate score/zero — солвер-роли,
/// [`Ladder`](Self::Ladder) / [`AlphaAnalog`](Self::AlphaAnalog) — полупрозрачная эмиссия.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RoleRecipe {
    /// Текстовый/UI-якорь: доля от максимального контраста фона + WCAG-пол.
    TextAnchor {
        /// Доля максимума контраста, `(0, 1]`.
        fraction: f64,
        /// WCAG-пол читаемости.
        floor: Floor,
        /// Опциональный источник физической цветовой идентичности. `None` берёт
        /// neutral policy таблицы. `Some(source)` держит тот же контракт уровня
        /// (`fraction`/`floor`), а точные sRGB8-байты источника определяют режим:
        /// равные каналы остаются нейтральными, неравные задают направление hue.
        /// Ссылка валидируется как у лестницы; JSON/DTO —
        /// `{kind:"text-anchor", fraction, floor, hue?: source}`.
        hue: Option<LadderSource>,
    },
    /// Заякоренная декоративная роль: dJ'-шаг светлоты, отдельно light/dark по теме.
    DjAnchor {
        /// dJ'-якорь под светлое окружение (`> 0`).
        light: f64,
        /// dJ'-якорь под тёмное окружение (`> 0`).
        dark: f64,
    },
    /// Декоративная роль в переходной Ys candidate-score единице `Lc` (стек теней):
    /// величина, знак — от фона. Это не LPC/readability evidence.
    DecorativeLc {
        /// Величина `Lc` (`> 0`).
        magnitude: f64,
    },
    /// Ступень переходной лестницы: `rgba(якорь, α)` напрямую. `source` — откуда берётся тинт,
    /// `position` — позиция закрытого меню (несёт свою альфу; исполняемый канон —
    /// [`LadderPosition::ALL`]). Компилируется в [`RoleSpec::Ladder`].
    Ladder {
        /// Источник тинта: бренд, семейство палитры или нейтраль.
        source: LadderSource,
        /// Позиция закрытого меню с собственной парой alpha по контекстам.
        position: LadderPosition,
        /// Опциональный юридический пол UI. `None` эмитит тинт как есть;
        /// `Some(floor)` допустим только для solid-позиции (`α=1`, например
        /// `BorderStrong`): семейный solid обязан держать пол, иначе выполняется
        /// минимальный легальный сдвиг по кривой семьи с явным флагом.
        floor: Option<Floor>,
    },
    /// Свечение: screen-слои цвета источника, интенсивность
    /// решается солвером под контрактную ступень [`crate::glow::GlowStep`]
    /// (зеркальная деривация от стека теней) на фактическом фоне резолва.
    Glow {
        /// Источник цвета свечения (свечение не имеет собственного цвета).
        source: LadderSource,
        /// Контрактная ступень стека: subtle | base | bloom.
        step: crate::glow::GlowStep,
        /// Обязательный numerical-decision profile; implicit legacy запрещён.
        decision_profile: crate::glow::GlowDecisionProfileV1,
    },
    /// Frozen PairFill frontend до C7c. Источник эмитится opaque Paint через
    /// общий point occurrence; отдельной Pair-эвристики и скрытой роли нет.
    PairFill {
        /// Источник якоря: бренд, семейство или нейтраль.
        source: LadderSource,
    },
    /// Frozen PairLabel frontend до C7c. Label-кандидаты проверяются против
    /// фактически emitted opaque [`PairFill`](Self::PairFill) Surface общим
    /// joint hard-report и fresh recheck.
    PairLabel {
        /// Источник физической цветовой идентичности: бренд, семейство или нейтраль.
        source: LadderSource,
        /// Доля максимума контраста PairFill Surface `(0, 1]` (как у
        /// [`TextAnchor`](Self::TextAnchor)): низкая доля оставляет больше места
        /// для хромы источника у пола, высокая тянет к контрастному пределу.
        /// Точный серый source при любой доле остаётся нейтральным.
        fraction: f64,
        /// WCAG-пол против emitted PairFill Surface, не страницы.
        floor: Floor,
    },
    /// Альфа-аналог solid-источника через точечную композит-инверсию
    /// ([`crate::alpha`]): `(tint, α)`, чей композит на объявленном фоне равен
    /// solid-цели `of`. Компилируется в [`RoleSpec::AlphaAnalog`].
    AlphaAnalog {
        /// Источник солид-цели (бренд/семейство/нейтраль), чей аналог берётся.
        of: LadderSource,
        /// Запрошенная альфа `(0, 1]` (поднимается до `α_min`, если ниже).
        alpha: f64,
    },
    /// Переходная двухслойная point-композиция: tint с вычисленной alpha и
    /// opaque base на заданном численном шаге `J'`. Она не моделирует glass,
    /// blur или spatial field. Компилируется в [`RoleSpec::Material`].
    ///
    /// Тир (`base/muted/soft/subtle`) — величина `tone` (|ΔJ'|): крупнее =
    /// заметнее/плотнее. `source` задаёт физическую идентичность: точные равные
    /// sRGB8-каналы дают нейтральный материал, неравные — материал в направлении
    /// hue источника.
    Material {
        /// Источник физической цветовой идентичности: бренд/семейство/нейтраль.
        source: LadderSource,
        /// Целевой |ΔJ'| тона-базы под светлое окружение (`> 0`).
        tone_light: f64,
        /// Целевой |ΔJ'| тона-базы под тёмное окружение (`> 0`).
        tone_dark: f64,
        /// WCAG-пол читаемости, который держит выведенная α (`AaText`/`AaUi`;
        /// `None` невалиден — валидатор ловит [`ConfigError::MaterialFloorRequired`]).
        floor: Floor,
    },
    /// Явный ноль: «нет цвета здесь» ([`RoleSpec::Zero`]).
    Zero,
}

/// Суффиксы CSS-переменных, которые один объявленный рецепт резервирует.
///
/// Основное имя принадлежит клиентскому токену даже тогда, когда `Zero` не
/// эмитит значения: projection всё равно публикует его `cssVar`, и сателлит
/// другой роли не вправе занять это имя. Остальной shape выводится из рецепта
/// до резолва, иначе коллизия могла бы зависеть от фона (на одном роль
/// unreachable и «всё работает», на другом два писателя молча делят один
/// `--lab-*`). Исчерпывающий match заставляет каждый новый рецепт явно выбрать
/// свой namespace shape.
fn reserved_css_suffixes(recipe: &RoleRecipe) -> &'static [&'static str] {
    const PRIMARY: &[&str] = &[""];
    const GLOW: &[&str] = &["", "-core", "-alpha"];
    const MATERIAL: &[&str] = &["", "-01", "-02"];

    match recipe {
        RoleRecipe::Glow { .. } => GLOW,
        RoleRecipe::Material { .. } => MATERIAL,
        RoleRecipe::TextAnchor { .. }
        | RoleRecipe::DjAnchor { .. }
        | RoleRecipe::DecorativeLc { .. }
        | RoleRecipe::Ladder { .. }
        | RoleRecipe::PairFill { .. }
        | RoleRecipe::PairLabel { .. }
        | RoleRecipe::AlphaAnalog { .. }
        | RoleRecipe::Zero => PRIMARY,
    }
}

/// Зарезервировать один shape эмиссии в общем namespace.
///
/// Отдельный примитив не знает сегодняшних суффиксов и потому не опирается на
/// случайное свойство, что `-core/-alpha/-01/-02` пока не пересекаются друг с
/// другом: будущая derived↔derived коллизия попадёт в тот же гард.
fn reserve_css_names(
    reserved: &mut std::collections::BTreeSet<String>,
    name: &str,
    suffixes: &[&str],
) -> Result<(), ConfigError> {
    for suffix in suffixes {
        let key = format!("--lab-{name}{suffix}");
        if !reserved.insert(key.clone()) {
            return Err(ConfigError::DuplicateKey {
                dictionary: "reserved CSS namespace",
                key,
            });
        }
    }
    Ok(())
}

/// Источник тинта лестницы/альфа-аналога: откуда берётся якорный цвет.
///
/// Тинт bg-независим (это якорь источника), только пер-темен. Для [`Family`](Self::Family)
/// У [`Family`](Self::Family) `key` — непрозрачная ссылка на семейство конфига;
/// валидатор проверяет только существование и никогда не выводит смысл из имени.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum LadderSource {
    /// Бренд-вход конфига (пер-темный якорь [`Brand`]).
    Brand,
    /// Семейство палитры по ключу (пер-темный якорь [`PaletteFamily`]).
    Family(String),
    /// Нейтральный тинт из [`NeutralConfig::anchors`] — семейство `Neutral/Derivable`
    /// стаба labui (`rgb(120 120 128 / …)` = `neutral.anchors.mid`). Скелетон и
    /// нейтральные fill/border/glow/focus-роли берут ЭТОТ источник, НЕ семейство
    /// палитры. Какой из трёх нейтральных якорей — задаёт [`NeutralPick`].
    Neutral(NeutralPick),
}

/// Какой якорь нейтральной шкалы берёт [`LadderSource::Neutral`] как тинт.
///
/// Нейтральная лестница labui тинтуется РАЗНЫМИ якорями по роли (заземление —
/// стаб `contract.css`): скелетон/тинты — средним (`neutral.anchors.mid`,
/// `#787880`, стаб `rgb(120 120 128 / …)`); нейтральное свечение — светлым краем
/// (`#FFFFFF`, стаб `rgb(255 255 255 / 0.522)`); нейтральный фокус — тёмным краем
/// (`#101012`, стаб `rgb(16 16 18)` на светлой теме). Выбор здесь держит тинт
/// пер-темными данными, а не веткой физики.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NeutralPick {
    /// Средний якорь `neutral.anchors.mid` (`#787880`) — скелетон, нейтральные тинты.
    Mid,
    /// Контурный край, контрастный теме ([`NeutralConfig::edge`]) — кольцо фокуса.
    Edge,
    /// Инвертированный средний тон ([`NeutralConfig::inverted`]) — свечение на
    /// инвертированной поверхности.
    Inverted,
    /// Светлый край `neutral.anchors.light` (`#FFFFFF`) — нейтральное свечение.
    Light,
    /// Тёмный край `neutral.anchors.dark` (`#101012`) — нейтральный фокус.
    Dark,
}

/// Полный конфиг темы потребителя (без сериализации — она на границе WASM).
///
/// Внешние крейты собирают конфиг через [`ThemeConfig::new`](Self::new), а не
/// struct-литералом. `#[non_exhaustive]` запрещает внешний struct-литерал, но не
/// обещает, что обязательный аргумент конструктора никогда не изменится; поля
/// остаются `pub` для чтения и мутации.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ThemeConfig {
    /// Бренд-вход.
    pub brand: Brand,
    /// Нейтральная шкала + подтон.
    pub neutral: NeutralConfig,
    /// Семейства палитры.
    pub palette: Vec<PaletteFamily>,
    /// Словарь тем.
    pub themes: ThemesConfig,
    /// Роли: имя (`[a-z0-9-]+`) → рецепт, в порядке объявления.
    pub roles: Vec<(String, RoleRecipe)>,
    /// Компонентные алиасы: имя → существующая роль.
    pub aliases: Vec<(String, String)>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Валидация.
// ─────────────────────────────────────────────────────────────────────────────

/// Проверить, что строка — валидный `[a-z0-9-]+` и не пуста.
fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// Проверить, что hex парсится ядром (только `#RRGGBB`).
fn check_hex(field: &str, value: &str) -> Result<(), ConfigError> {
    crate::spaces::srgb::srgb_from_hex(value)
        .map(|_| ())
        .map_err(|_| ConfigError::InvalidHex {
            field: field.to_string(),
            value: value.to_string(),
        })
}

/// Проверить, что все четыре пер-темных якоря — валидный hex (`field.light` …).
fn check_theme_anchors(field: &str, a: &ThemeAnchors) -> Result<(), ConfigError> {
    check_hex(&format!("{field}.light"), &a.light)?;
    check_hex(&format!("{field}.dark"), &a.dark)?;
    check_hex(&format!("{field}.light_ic"), &a.light_ic)?;
    check_hex(&format!("{field}.dark_ic"), &a.dark_ic)
}

/// Проверить, что имя валидно, иначе [`ConfigError::InvalidName`].
fn check_name(field: &str, value: &str) -> Result<(), ConfigError> {
    if is_valid_name(value) {
        Ok(())
    } else {
        Err(ConfigError::InvalidName {
            field: field.to_string(),
            value: value.to_string(),
        })
    }
}

/// Проверить `min < value ≤ max` (доля/хрома-стиль пределов).
fn check_in_excl_incl(
    handle: &str,
    value: f64,
    min_excl: f64,
    max_incl: f64,
    bound: &'static str,
) -> Result<(), ConfigError> {
    if value > min_excl && value <= max_incl {
        Ok(())
    } else {
        Err(ConfigError::OutOfBounds {
            handle: handle.to_string(),
            value,
            bound,
        })
    }
}

/// Проверить `value > min` (строго положительно). Неконечные значения (±∞,
/// NaN) отвергаются всегда: открытый сверху предел — не лазейка для мусора.
fn check_gt(
    handle: &str,
    value: f64,
    min_excl: f64,
    bound: &'static str,
) -> Result<(), ConfigError> {
    if value.is_finite() && value > min_excl {
        Ok(())
    } else {
        Err(ConfigError::OutOfBounds {
            handle: handle.to_string(),
            value,
            bound,
        })
    }
}

/// Проверить `value ≥ min`. Неконечные значения отвергаются всегда.
fn check_ge(
    handle: &str,
    value: f64,
    min_incl: f64,
    bound: &'static str,
) -> Result<(), ConfigError> {
    if value.is_finite() && value >= min_incl {
        Ok(())
    } else {
        Err(ConfigError::OutOfBounds {
            handle: handle.to_string(),
            value,
            bound,
        })
    }
}

impl ThemeConfig {
    /// Собрать конфиг из полного набора полей.
    ///
    /// [`ThemeConfig`] помечен `#[non_exhaustive]`, поэтому внешние крейты
    /// (включая WASM-границу) собирают его через этот конструктор, а не
    /// struct-литералом.
    pub fn new(
        brand: Brand,
        neutral: NeutralConfig,
        palette: Vec<PaletteFamily>,
        themes: ThemesConfig,
        roles: Vec<(String, RoleRecipe)>,
        aliases: Vec<(String, String)>,
    ) -> Self {
        ThemeConfig {
            brand,
            neutral,
            palette,
            themes,
            roles,
            aliases,
        }
    }

    /// Провалидировать конфиг как ПОЛНЫЙ preflight: `Ok` гарантирует, что
    /// [`compile_named_role_table`](Self::compile_named_role_table) вернёт `Ok`.
    ///
    /// Гарантия держится по построению: `validate` — это компиляция с
    /// отброшенным результатом (единый код-путь; паритет validate/compile не
    /// может разъехаться, потому что второго списка проверок не существует).
    /// Ловится и структурное (hex, имена, ссылки, пределы ручек), и
    /// деривационное (например, ссылка роли на отсутствующую
    /// edge/inverted-четвёрку). Первая найденная ошибка возвращается сразу —
    /// клиент чинит по одной.
    ///
    /// # Errors
    ///
    /// Та же [`ConfigError`], которую вернула бы компиляция.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.compile_named_role_table().map(drop)
    }

    /// Структурная фаза валидации: hex, имена, ссылки на семейства/источники
    /// лестницы, дубликаты словарей и пределы каждой экспонируемой ручки.
    /// НЕ полный preflight: деривационные ошибки (например, ссылка роли на
    /// отсутствующую edge/inverted-четвёрку) всплывают только в фазе компиляции —
    /// полноту даёт [`validate`](Self::validate).
    fn validate_syntactic(&self) -> Result<(), ConfigError> {
        // Бренд: пер-темная четвёрка hex.
        check_theme_anchors("brand.anchors", &self.brand.anchors)?;

        // Нейтраль: тройка hex.
        check_hex("neutral.anchors.light", &self.neutral.anchors.light)?;
        check_hex("neutral.anchors.mid", &self.neutral.anchors.mid)?;
        check_hex("neutral.anchors.dark", &self.neutral.anchors.dark)?;

        // Пер-темные края нейтрали: hex валидируется, если четвёрка задана —
        // даже без ссылающихся ролей (задекларированные данные обязаны быть
        // валидными: мёртвый битый hex всплыл бы позже дорогой загадкой).
        if let Some(edge) = &self.neutral.edge {
            check_theme_anchors("neutral.edge", edge)?;
        }
        if let Some(inverted) = &self.neutral.inverted {
            check_theme_anchors("neutral.inverted", inverted)?;
        }

        // Нейтраль: ручки подтона.
        check_gt(
            "neutral.tint.target_mp",
            self.neutral.tint.target_mp,
            TARGET_MP_MIN_EXCLUSIVE,
            "target_mp > 0 (целевая красочность CAM16-UCS M')",
        )?;
        check_ge(
            "neutral.tint.hue_stiffness",
            self.neutral.tint.hue_stiffness,
            HUE_STIFFNESS_MIN_INCLUSIVE,
            "hue_stiffness ≥ 0 (жёсткость прижатия оттенка к каноническому)",
        )?;

        // Палитра: имена + пер-темная четвёрка hex каждого семейства.
        for fam in &self.palette {
            let field = format!("palette[{}].key", fam.key);
            check_name(&field, &fam.key)?;
            let anchors_field = format!("palette[{}].anchors", fam.key);
            check_theme_anchors(&anchors_field, &fam.anchors)?;
        }

        if let Some(hue) = self.neutral.tint.hue_override_deg {
            if !(hue.is_finite() && (HUE_DEG_MIN_INCLUSIVE..HUE_DEG_MAX_EXCLUSIVE).contains(&hue)) {
                return Err(ConfigError::OutOfBounds {
                    handle: "neutral.tint.hue_override_deg".to_string(),
                    value: hue,
                    bound: "0 ≤ hue < 360 (явный оттенок подтона по модулю 360°)",
                });
            }
        }

        // Дубликаты ключей всех словарей: повтор имени = неоднозначный lookup.
        fn check_unique<'a, I: Iterator<Item = &'a str>>(
            dictionary: &'static str,
            keys: I,
        ) -> Result<(), ConfigError> {
            let mut seen = std::collections::BTreeSet::new();
            for k in keys {
                if !seen.insert(k) {
                    return Err(ConfigError::DuplicateKey {
                        dictionary,
                        key: k.to_string(),
                    });
                }
            }
            Ok(())
        }
        check_unique("palette", self.palette.iter().map(|f| f.key.as_str()))?;
        check_unique(
            "themes",
            self.themes.entries.iter().map(|(n, _)| n.as_str()),
        )?;
        check_unique("roles", self.roles.iter().map(|(n, _)| n.as_str()))?;
        check_unique("aliases", self.aliases.iter().map(|(n, _)| n.as_str()))?;
        // Алиас не может затенять роль: одно имя — одна сущность эмиссии.
        for (alias, _) in &self.aliases {
            if self.roles.iter().any(|(rname, _)| rname == alias) {
                return Err(ConfigError::DuplicateKey {
                    dictionary: "roles∪aliases",
                    key: alias.clone(),
                });
            }
        }

        // Темы: имена.
        for (name, _preset) in &self.themes.entries {
            let field = format!("themes.{name}");
            check_name(&field, name)?;
        }

        // Роли: имена + пределы ручек каждого рецепта.
        for (name, recipe) in &self.roles {
            let field = format!("roles.{name}");
            check_name(&field, name)?;
            self.validate_recipe(name, recipe)?;
        }

        // Алиасы: имя алиаса валидно, цель существует среди ролей.
        for (alias, target) in &self.aliases {
            let field = format!("aliases.{alias}");
            check_name(&field, alias)?;
            if !self.roles.iter().any(|(rname, _)| rname == target) {
                return Err(ConfigError::UnknownRole {
                    referenced_by: format!("aliases.{alias}"),
                    role: target.clone(),
                });
            }
        }

        // Роль резервирует не только собственный `--lab-{name}`: Glow и Material
        // создают сателлиты. Алиас клонирует outcome цели и потому создаёт тот же
        // набор уже под СВОИМ именем. Проверяем итоговый namespace целиком до
        // резолва/JSON, чтобы порядок писателей никогда не решал, чьё значение
        // молча победит. Один общий set ловит role↔satellite, alias↔satellite и
        // любые будущие satellite↔satellite пересечения без списка частных пар.
        let mut reserved = std::collections::BTreeSet::new();

        for (name, recipe) in &self.roles {
            reserve_css_names(&mut reserved, name, reserved_css_suffixes(recipe))?;
        }
        for (alias, target) in &self.aliases {
            let target_recipe = self
                .roles
                .iter()
                .find(|(name, _)| name == target)
                .map(|(_, recipe)| recipe)
                .ok_or_else(|| ConfigError::UnknownRole {
                    referenced_by: format!("aliases.{alias}"),
                    role: target.clone(),
                })?;
            reserve_css_names(&mut reserved, alias, reserved_css_suffixes(target_recipe))?;
        }

        Ok(())
    }

    /// Провалидировать пределы ручек одного рецепта роли.
    fn validate_recipe(&self, role: &str, recipe: &RoleRecipe) -> Result<(), ConfigError> {
        match recipe {
            RoleRecipe::TextAnchor { fraction, hue, .. } => {
                check_in_excl_incl(
                    &format!("roles.{role}.fraction"),
                    *fraction,
                    FRACTION_MIN_EXCLUSIVE,
                    FRACTION_MAX_INCLUSIVE,
                    "0 < fraction ≤ 1 (доля максимального контраста фона)",
                )?;
                // Явный source-slot обязан ссылаться на существующий физический
                // источник; chromatic/neutral режим определяется позже из байтов.
                if let Some(source) = hue {
                    self.check_ladder_source(role, source)?;
                }
                Ok(())
            }
            RoleRecipe::DjAnchor { light, dark } => {
                check_gt(
                    &format!("roles.{role}.light"),
                    *light,
                    DJ_MIN_EXCLUSIVE,
                    "dj > 0 (перцептивный шаг светлоты; ≤ 0 = нет различимого шага)",
                )?;
                check_gt(
                    &format!("roles.{role}.dark"),
                    *dark,
                    DJ_MIN_EXCLUSIVE,
                    "dj > 0 (перцептивный шаг светлоты; ≤ 0 = нет различимого шага)",
                )
            }
            RoleRecipe::DecorativeLc { magnitude } => check_ge(
                &format!("roles.{role}.magnitude"),
                *magnitude,
                DECORATIVE_FLOOR_MIN,
                DECORATIVE_FLOOR_BOUND,
            ),
            RoleRecipe::Ladder {
                source,
                position,
                floor,
            } => {
                self.check_ladder_source(role, source)?;
                let (alpha_light, alpha_dark) = position.alpha_pair();
                validate_ladder_floor(alpha_light, alpha_dark, *floor).map_err(|reason| {
                    ConfigError::InvalidLadderFloor {
                        role: role.to_string(),
                        reason,
                    }
                })
            }
            // Ступень — закрытый enum, числовой валидации не требует; источник — как у лестницы.
            RoleRecipe::Glow { source, .. } => self.check_ladder_source(role, source),
            RoleRecipe::PairFill { source } => self.check_ladder_source(role, source),
            RoleRecipe::PairLabel {
                source, fraction, ..
            } => {
                self.check_ladder_source(role, source)?;
                check_in_excl_incl(
                    &format!("roles.{role}.fraction"),
                    *fraction,
                    FRACTION_MIN_EXCLUSIVE,
                    FRACTION_MAX_INCLUSIVE,
                    "0 < fraction ≤ 1 (доля максимального контраста PairFill Surface)",
                )
            }
            RoleRecipe::AlphaAnalog { of, alpha } => {
                self.check_ladder_source(role, of)?;
                check_in_excl_incl(
                    &format!("roles.{role}.alpha"),
                    *alpha,
                    ALPHA_MIN_EXCLUSIVE,
                    ALPHA_MAX_INCLUSIVE,
                    "0 < alpha ≤ 1 (запрошенная непрозрачность альфа-аналога)",
                )
            }
            RoleRecipe::Material {
                source,
                tone_light,
                tone_dark,
                floor,
            } => {
                self.check_ladder_source(role, source)?;
                check_gt(
                    &format!("roles.{role}.tone_light"),
                    *tone_light,
                    DJ_MIN_EXCLUSIVE,
                    "dj > 0 (перцептивный шаг тона; ≤ 0 = нет различимой поверхности)",
                )?;
                check_gt(
                    &format!("roles.{role}.tone_dark"),
                    *tone_dark,
                    DJ_MIN_EXCLUSIVE,
                    "dj > 0 (перцептивный шаг тона; ≤ 0 = нет различимой поверхности)",
                )?;
                // Материал обязан нести пол читаемости — без него α не выводима.
                if matches!(floor, Floor::None) {
                    return Err(ConfigError::MaterialFloorRequired {
                        role: role.to_string(),
                    });
                }
                Ok(())
            }
            RoleRecipe::Zero => Ok(()),
        }
    }

    /// Проверить, что источник лестницы разрешим: [`LadderSource::Family`]
    /// ссылается на существующее семейство `palette`; [`LadderSource::Brand`]
    /// всегда разрешим (бренд — обязательный вход конфига).
    fn check_ladder_source(&self, role: &str, source: &LadderSource) -> Result<(), ConfigError> {
        match source {
            LadderSource::Brand => Ok(()),
            LadderSource::Family(key) => {
                if self.palette.iter().any(|f| &f.key == key) {
                    Ok(())
                } else {
                    Err(ConfigError::UnknownFamily {
                        referenced_by: format!("roles.{role}"),
                        family: key.clone(),
                    })
                }
            }
            // Нейтральный источник всегда разрешим (neutral.anchors — обязательный
            // вход конфига, провалидирован check_theme_anchors как тройка hex).
            LadderSource::Neutral(_) => Ok(()),
        }
    }

    /// Скомпилировать роли конфига в [`NamedRoleTable`], которую
    /// [`resolve_named_set`](crate::semantic::resolve_named_set) резолвит той же
    /// физикой, что и встроенную `crate::RoleTable`.
    ///
    /// Сначала структурная фаза (приватный `validate_syntactic` — потому не линк),
    /// затем деривационные ошибки по ходу компиляции (снаружи все фазы разом —
    /// [`validate`](Self::validate), который и есть эта компиляция с отброшенным
    /// результатом; НЕ вызывать её из фаз — рекурсия). [`Ladder`](RoleRecipe::Ladder)
    /// раскладывает источник в пер-темный тинт, [`AlphaAnalog`](RoleRecipe::AlphaAnalog)
    /// — солид-цель источника + альфа.
    ///
    /// # Errors
    ///
    /// [`ConfigError`] структурной/деривационной фазы,
    /// [`ConfigError::EmptyContract`] на голом контракте (без ролей и алиасов)
    /// либо [`ConfigError::EmptyThemes`] на пустом словаре тем.
    pub fn compile_named_role_table(&self) -> Result<NamedRoleTable, ConfigError> {
        self.validate_syntactic()?;

        // Пустой контракт — ошибка НА ЗАГРУЗКЕ, не тихий приём: конфиг без
        // ролей/алиасов не эмитил бы ни одной роли, и дефект уехал бы на
        // использование. После структурной фазы: конкретные ошибки полей
        // всплывают раньше.
        if self.roles.is_empty() && self.aliases.is_empty() {
            return Err(ConfigError::EmptyContract);
        }
        // Пустой словарь тем — тот же класс дефекта, что и пустой контракт
        // ролей: без темы resolve/recheck невозможны, отказ обязан быть на
        // загрузке, а не поздним unknown_theme на использовании.
        if self.themes.entries.is_empty() {
            return Err(ConfigError::EmptyThemes);
        }

        let mut entries: Vec<(String, RoleSpec)> = Vec::with_capacity(self.roles.len());
        for (name, recipe) in &self.roles {
            let spec = self.compile_recipe(name, recipe)?;
            entries.push((name.clone(), spec));
        }

        // Neutral policy comes only from client data. An override explicitly
        // supplies hue; otherwise exact emitted bytes decide whether a direction
        // exists. Equal channels stay neutral instead of receiving matrix-noise
        // hue. The retired schema-level flat-ratio handle is intentionally absent.
        let curve = |canonical_hue_deg| RoleChroma::Curve {
            canonical_hue_deg,
            target_mp: self.neutral.tint.target_mp,
            hue_stiffness: self.neutral.tint.hue_stiffness,
        };
        let chroma = match self.neutral.tint.hue_override_deg {
            Some(hue) => curve(hue),
            None => {
                let dark_bytes =
                    crate::srgb8::hex_bytes(&self.neutral.anchors.dark).map_err(|_| {
                        ConfigError::InvalidHex {
                            field: "neutral.anchors.dark".to_string(),
                            value: self.neutral.anchors.dark.clone(),
                        }
                    })?;
                match hue_of_srgb8(Srgb8::new(dark_bytes)) {
                    OklabHue::Achromatic => RoleChroma::Neutral,
                    OklabHue::Chromatic { degrees } => curve(degrees),
                }
            }
        };

        // Тот же меж-полевой закон, что у публичного конструктора
        // `NamedRoleTable::new`: preflight не вправе создавать таблицу, которая
        // отвергнется только при первом runtime-resolve.
        for (role, spec) in &entries {
            if !spec.is_chroma_compatible(chroma) {
                return Err(ConfigError::IncompatibleRolePolicy { role: role.clone() });
            }
        }

        NamedRoleTable::from_validated_parts(entries, self.aliases.clone(), chroma).map_err(
            |error| ConfigError::OutOfBounds {
                handle: format!("roles.{}.alpha", self.roles[error.declaration_ordinal()].0),
                value: error.value(),
                bound: "0 < alpha ≤ 1 (запрошенная непрозрачность альфа-аналога)",
            },
        )
    }

    /// Скомпилировать один рецепт в [`RoleSpec`]. Ladder/AlphaAnalog раскладывают
    /// источник в пер-темный тинт (`Copy`-payload [`LadderTint`]) на этапе
    /// компиляции — резолв остаётся bg-зависимым только через фон подложки.
    fn compile_recipe(&self, role: &str, recipe: &RoleRecipe) -> Result<RoleSpec, ConfigError> {
        match recipe {
            RoleRecipe::TextAnchor {
                fraction,
                floor,
                hue,
            } => {
                let anchor =
                    TextAnchor::new(*fraction, *floor).map_err(|_| ConfigError::OutOfBounds {
                        handle: format!("roles.{role}.fraction"),
                        value: *fraction,
                        bound: "0 < fraction ≤ 1 (доля максимального контраста фона)",
                    })?;
                // Source-slot раскладывается в пер-темный якорь тем же механизмом,
                // что тинт лестницы. Резолв классифицирует точные байты и держит
                // контракт уровня без изобретения hue для серого источника.
                let anchor = match hue {
                    Some(source) => anchor.with_hue(self.compile_ladder_tint(role, source)?),
                    None => anchor,
                };
                Ok(RoleSpec::Anchor(anchor))
            }
            RoleRecipe::DjAnchor { light, dark } => Ok(RoleSpec::DecorativeDj {
                magnitude_dj: DjMagnitude::new(*light, *dark),
            }),
            RoleRecipe::DecorativeLc { magnitude } => Ok(RoleSpec::Decorative {
                magnitude: *magnitude,
            }),
            RoleRecipe::Zero => Ok(RoleSpec::Zero),
            RoleRecipe::Glow {
                source,
                step,
                decision_profile,
            } => Ok(RoleSpec::Glow {
                tint: self.compile_ladder_tint(role, source)?,
                step: *step,
                // Wire-ключ пока компилируется в execution mode и тем самым
                // явно выбирает одну из существующих численных ветвей. Это
                // pre-cutover поведение, а не доказательство общей graph-модели.
                mode: decision_profile.execution_mode(),
            }),
            RoleRecipe::Ladder {
                source,
                position,
                floor,
            } => {
                let (alpha_light, alpha_dark) = position.alpha_pair();
                Ok(RoleSpec::Ladder {
                    tint: self.compile_ladder_tint(role, source)?,
                    alpha_light,
                    alpha_dark,
                    floor: *floor,
                })
            }
            RoleRecipe::AlphaAnalog { of, alpha } => Ok(RoleSpec::AlphaAnalog {
                of: self.compile_ladder_tint(role, of)?,
                alpha: *alpha,
            }),
            RoleRecipe::Material {
                source,
                tone_light,
                tone_dark,
                floor,
            } => {
                // Любой source, включая Neutral(pick), передаёт выбранные точные
                // байты. Резолв сам классифицирует их как achromatic/chromatic;
                // общая policy таблицы не подменяет client-owned выбор источника.
                let hue = Some(self.compile_ladder_tint(role, source)?);
                Ok(RoleSpec::Material {
                    hue,
                    tone: DjMagnitude::new(*tone_light, *tone_dark),
                    floor: *floor,
                })
            }
            RoleRecipe::PairFill { source } => Ok(RoleSpec::PairFill {
                tint: self.compile_ladder_tint(role, source)?,
            }),
            RoleRecipe::PairLabel {
                source,
                fraction,
                floor,
            } => {
                // P1 унифицирует PairFill/PairLabel на единственной поверхности,
                // которую публичный PairFill уже эмитил: opaque source Paint.
                // Representation не выводится из клиентского имени позиции.
                let (surface_alpha_light, surface_alpha_dark) = (1.0, 1.0);
                Ok(RoleSpec::PairLabel {
                    tint: self.compile_ladder_tint(role, source)?,
                    fraction: *fraction,
                    floor: *floor,
                    surface_alpha_light,
                    surface_alpha_dark,
                })
            }
        }
    }

    /// Разложить источник лестницы в пер-темный кодированный [`LadderTint`].
    ///
    /// - [`LadderSource::Brand`] / [`LadderSource::Family`]: сырая пер-темная
    ///   четвёрка якорей (эмитится напрямую как `rgba`).
    fn compile_ladder_tint(
        &self,
        role: &str,
        source: &LadderSource,
    ) -> Result<LadderTint, ConfigError> {
        let anchors = match source {
            LadderSource::Brand => self.brand.anchors.clone(),
            LadderSource::Family(key) => self.family_anchors(role, key)?.clone(),
            LadderSource::Neutral(pick) => self.neutral_anchors(role, *pick)?,
        };
        let quad = anchors
            .encoded_quad()
            .map_err(|_| ConfigError::InvalidHex {
                field: format!("roles.{role} (источник лестницы)"),
                value: "<пер-темный якорь>".to_string(),
            })?;
        LadderTint::new(quad).map_err(|mode| ConfigError::InvalidHex {
            field: format!("roles.{role} (тинт лестницы, режим {mode})"),
            value: "<вне кодированного домена>".to_string(),
        })
    }

    /// Нейтральный якорь из [`NeutralConfig::anchors`] по [`NeutralPick`],
    /// продублированный на четыре режима (нейтральная шкала конфига несёт один
    /// hex на край, без пер-темных IC-вариантов). Заземление — стаб labui:
    /// `Neutral/Derivable` тинтуется этими краями (`#787880`/`#FFFFFF`/`#101012`).
    fn neutral_anchors(&self, role: &str, pick: NeutralPick) -> Result<ThemeAnchors, ConfigError> {
        // Edge/Inverted — пер-темные четвёрки из конфига: дублирование одного
        // hex дало бы невидимые роли (контур #101012 на тёмной теме); без поля
        // pick честно падает ошибкой, не выдумкой.
        let single =
            match pick {
                NeutralPick::Edge => {
                    return self
                        .neutral
                        .edge
                        .clone()
                        .ok_or(ConfigError::MissingNeutralAnchors {
                            referenced_by: format!("roles.{role}"),
                            field: "neutral.edge",
                        });
                }
                NeutralPick::Inverted => {
                    return self.neutral.inverted.clone().ok_or(
                        ConfigError::MissingNeutralAnchors {
                            referenced_by: format!("roles.{role}"),
                            field: "neutral.inverted",
                        },
                    );
                }
                NeutralPick::Mid => &self.neutral.anchors.mid,
                NeutralPick::Light => &self.neutral.anchors.light,
                NeutralPick::Dark => &self.neutral.anchors.dark,
            };
        Ok(ThemeAnchors {
            light: single.clone(),
            dark: single.clone(),
            light_ic: single.clone(),
            dark_ic: single.clone(),
        })
    }

    /// Пер-темные якоря семейства палитры по ключу (валидатор уже проверил
    /// существование — здесь защита компиляции).
    fn family_anchors(&self, role: &str, key: &str) -> Result<&ThemeAnchors, ConfigError> {
        self.palette
            .iter()
            .find(|f| f.key == key)
            .map(|f| &f.anchors)
            .ok_or_else(|| ConfigError::UnknownFamily {
                referenced_by: format!("roles.{role}"),
                family: key.to_string(),
            })
    }
}

/// Словарь эталонного пресета labui — ТОЛЬКО семантика ролей/алиасов (ни одного
/// цветового значения). `#[cfg(test)]`-only (ADR-0001): labui-дерево ушло из
/// ОТГРУЖАЕМОГО кода ядра — прод-скан агностичности
/// (`tests/agnostic_production_surface.rs`) его не видит. Единственный потребитель —
/// тестовая фикстура `fixture`.
#[cfg(test)]
pub(crate) mod preset;

/// Каноническая референс-фикстура labui (дерево Даниила С ЦВЕТАМИ) — тестовый
/// оракул, `#[cfg(test)]`-only (CH-3: брендовые hex не покидают тестов). Словарь
/// тянет из [`preset`]; эмиссия заморожена байт-гейтом (`crate::agnostic_gates`).
#[cfg(test)]
pub(crate) mod fixture;

/// Общая строковая форма `Resolved` для in-crate characterization/golden-тестов.
#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;
