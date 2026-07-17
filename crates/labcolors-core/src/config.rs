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
//! [`RoleRecipe::Ladder`] (акцентная/сентимент/бренд-лестница, поглощает GAP #59)
//! компилируется в [`RoleSpec::Ladder`]: источник раскладывается в пер-темный
//! тинт-якорь ([`crate::ladder::LadderTint`]), позиция несёт альфу Figma-рампы.
//! [`RoleRecipe::AlphaAnalog`] компилируется в [`RoleSpec::AlphaAnalog`] (солид-
//! цель источника + запрошенная альфа, композит-инверсия — [`crate::alpha`], #119).
//! Резолв обоих — [`crate::semantic::Resolved::Translucent`] (тинт×альфа напрямую + солид-
//! композит на фоне резолва для замера контраста). Меню позиций + провенанс —
//! приложение A к `docs/decisions/0001-config-boundary.md`.
//!
//! # Агностичность: конфиг несёт СВОЙ словарь ролей
//!
//! Ядро не знает ни одной роли дизайн-системы — [`ThemeConfig`] обязан нести
//! собственные `roles`/`aliases` (клиент вносит и значения, и семантику). Пустой
//! контракт (без `roles` и без `aliases`) отклоняется на загрузке —
//! [`ConfigError::EmptyContract`]. Фоновая лестница дельтами (§4 плана BL-007) —
//! отдельный заход; сейчас фоны едут якорями конфига как есть.

use crate::ladder::{LadderPosition, LadderTint, ThemeAnchors};
use crate::semantic::{
    DECORATIVE_FLOOR_MIN, DjMagnitude, NamedRoleTable, RoleChroma, RoleSpec, TextAnchor,
    validate_ladder_floor,
};
use crate::solve::Floor;

// ─────────────────────────────────────────────────────────────────────────────
// Пределы валидатора.
//
// Каждый предел — широкий и честный: он отсекает значения, которые молча ломают
// восприятие/WCAG или вырождают физику, но НЕ навязывает узость. Границы выведены
// из перцептивных инвариантов и стиля реестра `docs/empirical-inventory.md`
// (напр. `0.030 ≤ C0 ≤ 0.050`). ВАЖНО: эти константы — НЕ перцептивные величины
// (они пределы допустимости ручек, не сами ручки), и модуль `config.rs` не входит
// в аудит-поверхность `tests/empirical_inventory.rs` (только 6 перцептивных
// модулей), поэтому им не нужен SSOT-маркер — обоснование каждого несёт doc-строка.
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
// декоративного пола ядра. Единица — воспринимаемый контраст `Lc`; знак выбирает
// физика от фона, поэтому конфиг несёт величину (модуль). Значение ниже
// `DECORATIVE_FLOOR_MIN` попадает в квантованный low-contrast gap; ядро не
// переписывает такую декларацию в другой контракт, а отклоняет её на загрузке.

/// Коэффициент хромы подтона (`neutral.tint.ratio`) обязан лежать в `[0, 1]`.
///
/// Абсолютная хрома подтона = `ratio · max_chroma(L)`. `ratio = 0` — чистый серый
/// (допустимо: явный отказ от подтона), `ratio = 1` — максимум гамута. Значения вне
/// `[0, 1]` не имеют физического смысла (отрицательная хрома, либо запрос за стеной
/// гамута). Используется v1-путём ([`RoleChroma::Tinted`]); в дефолтном v2-пути
/// ([`RoleChroma::Curve`]) сила задаётся `target_mp`, но ручка всё равно
/// валидируется, т.к. экспонирована.
const TINT_RATIO_MIN_INCLUSIVE: f64 = 0.0;
/// Верхний предел коэффициента хромы подтона (включительно).
const TINT_RATIO_MAX_INCLUSIVE: f64 = 1.0;

/// Целевая красочность подтона (`neutral.tint.target_mp`, CAM16-UCS `M'`) обязана
/// быть строго положительной.
///
/// `M'` — перцептивная красочность; отрицательная бессмысленна, нулевая = серый (для
/// серого есть явный `ratio = 0`). Реестр держит дефолт `6.1` на плато измеренной
/// референс-рампы; предел лишь отсекает нефизичное `≤ 0`.
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

/// Нижняя граница валидации поля `sentiments.hardness` (`≥ 1`).
///
/// ⚠️ VESTIGIAL после Волны 1: поле `hardness` задавало p-норму brand-displacement
/// (Sticky Potential Well, #55), но закон категориальных зон снёс этот механизм —
/// поле больше НЕ влияет на выход (см. док `SentimentsConfig.hardness`). Граница
/// `≥ 1` сохранена как контракт схемы конфига: значение всё ещё валидируется
/// (мусор отвергается), но законом оттенка не потребляется. Историческая
/// семантика: `p < 1` выводило p-норму из корректной области.
const HARDNESS_MIN_INCLUSIVE: f64 = 1.0;

/// Доля хромы сентимент-цвета (`sentiments.chroma_fraction`) обязана лежать в
/// `(0, 1]`.
///
/// `≤ 0` — обесцвеченный (не сентимент), `> 1` — за стеной гамута (неон/недостижимо).
/// Дефолт реестра — `0.88` (держится `< 1`, чтобы сидеть внутри стены гамута, не
/// читаясь как неон).
///
/// СЕМАНТИКА (применена 2026-07-03; тем самым закрыт слайс «инертной ручки»
/// аудита): в продакшн-тинте (`resolve_config_sentiment_solid`) ручка —
/// АНТИ-НЕОНОВЫЙ ПОТОЛОК: `c = min(c_якоря, f · C_max(L, h_решённый))`.
/// Хрома якоря клиента — авторитет идентичности и сохраняется, пока не
/// упирается в долю гамутного максимума; усечение идёт по оси хромы
/// (оттенок сохранён — прежний канальный клип sRGB искажал оттенок).
const CHROMA_FRACTION_MIN_EXCLUSIVE: f64 = 0.0;
/// Верхний предел доли хромы сентимента (включительно).
const CHROMA_FRACTION_MAX_INCLUSIVE: f64 = 1.0;

/// Запрошенная альфа альфа-аналога (`roles.*.alpha`) обязана лежать в `(0, 1]`.
///
/// `≤ 0` — невидимая роль (вырождение), `> 1` — не альфа. Резолвер поднимает
/// фактическую α до `α_min`, если запрошенная ниже минимально-разрешимой в
/// гамуте ([`crate::alpha::resolve_alpha_analog`]) — но сам запрос должен быть
/// валидной альфой.
const ALPHA_MIN_EXCLUSIVE: f64 = 0.0;
/// Верхний предел запрошенной альфы (включительно; α = 1 = солид).
const ALPHA_MAX_INCLUSIVE: f64 = 1.0;

// Канонический домен оттенка [0, 360) — единый дом в crate::sentiment
// (два независимых литерала одного домена = класс тихого расхождения).
use crate::sentiment::{HUE_DOMAIN_MAX_EXCLUSIVE, HUE_DOMAIN_MIN_INCLUSIVE};

// ─────────────────────────────────────────────────────────────────────────────
// Ошибки валидации конфига.
// ─────────────────────────────────────────────────────────────────────────────

/// Ошибка компиляции или валидации [`ThemeConfig`].
///
/// Матчится по вариантам (это часть публичного API ядра): потребитель различает
/// «невалидный hex», «ручка вне предела» и «ссылка на несуществующее семейство».
/// Реализована вручную (без `thiserror`) — крейт
/// `labcolors-core` держит НОЛЬ runtime-зависимостей (issue #29); стиль `Display`
/// повторяет ручные ошибки ядра.
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
    /// Ссылка на категорию сентиментов, которой нет в `sentiments.categories`.
    UnknownSentiment {
        referenced_by: String,
        sentiment: String,
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
    /// Источник вывода оттенка ахроматичен (Oklab-хрома ≈ 0): hue математически
    /// не определён — требуется явный hue_override_deg.
    AchromaticHueSource { field: String },
    /// Значение ручки вне допустимого предела. `handle` — путь до ручки, `bound` —
    /// человеко-читаемое описание нарушенного предела с обоснованием.
    OutOfBounds {
        handle: String,
        value: f64,
        bound: &'static str,
    },
    /// Сентимент-солвер не смог развести оттенок (пустая легальная дуга,
    /// недоменные углы/пороги). Отдельный вариант, а не
    /// [`InvalidHex`](Self::InvalidHex): ошибка политики/геометрии,
    /// замаскированная под ошибку парсинга hex, ломала бы матчинг
    /// потребителя по вариантам.
    SentimentResolution {
        role: String,
        sentiment: String,
        reason: String,
    },
    /// Контракт пуст: ни ролей, ни алиасов. Резолв не эмитил бы ни одной роли —
    /// молчаливый приём увёл бы дефект на использование. Отказ обязан быть НА
    /// ЗАГРУЗКЕ (`#[serde(default)]` на `roles` на границе WASM разрешает ОПУСТИТЬ
    /// словарь синтаксически, но не остаться совсем без контракта).
    EmptyContract,
    /// `material`-рецепту передан `floor: zero` — у материала нет цели для вывода
    /// альфы без пола читаемости. Отказ на загрузке (а не молчаливая невидимая
    /// роль): материал обязан нести `aa-text` или `aa-ui`.
    MaterialFloorRequired { role: String },
    /// Readability-floor лестницы объявлен для полупрозрачной позиции либо как
    /// no-op `zero`. Публичный floor сейчас определён только для opaque
    /// occurrence; для translucent не объявлено, какую occurrence ограничивать
    /// и разрешено ли менять tint, alpha или оба параметра.
    InvalidLadderFloor { role: String, reason: &'static str },
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
            ConfigError::UnknownSentiment {
                referenced_by,
                sentiment,
            } => write!(
                f,
                "`{referenced_by}` ссылается на категорию сентиментов `{sentiment}`, которой нет в sentiments"
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
            ConfigError::AchromaticHueSource { field } => write!(
                f,
                "источник оттенка `{field}` ахроматичен — hue не определён, задай hue_override_deg"
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
            ConfigError::SentimentResolution {
                role,
                sentiment,
                reason,
            } => write!(f, "сентимент `{sentiment}` (роль `{role}`): {reason}"),
            ConfigError::EmptyContract => write!(f, "контракт пуст: передайте roles"),
            ConfigError::MaterialFloorRequired { role } => write!(
                f,
                "material-роль `{role}` требует пол читаемости (aa-text/aa-ui), получен zero-floor"
            ),
            ConfigError::InvalidLadderFloor { role, reason } => {
                write!(f, "ladder-роль `{role}` несёт невалидный floor: {reason}")
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
/// (`reference/labui-accent-primitives.md` §2: Brand light `#007AFF` /
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
    /// Коэффициент хромы подтона (v1 flat-путь): `[0, 1]`.
    pub ratio: f64,
    /// Целевая перцептивная красочность CAM16-UCS `M'` (v2-кривая, «сила»): `> 0`.
    pub target_mp: f64,
    /// Жёсткость прижатия оттенка к каноническому (v2-кривая): `≥ 0`.
    pub hue_stiffness: f64,
    /// Явный оттенок подтона (градусы `[0, 360)`), если у потребителя есть
    /// ИЗМЕРЕННАЯ величина (labui: SSOT 286.0°). `None` — движок выводит оттенок
    /// из тёмного якоря нейтрали (та же деривация, которой была получена
    /// labui-константа: `#101012` → 285.97°).
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
/// заземление `reference/labui-accent-primitives.md` §2 показывает, что тёмный и
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

/// Политика одной семантической категории потребителя: маппинг на семейство
/// палитры + категориальные ручки.
#[derive(Debug, Clone, PartialEq)]
pub struct SentimentCategory {
    /// Семантическое имя категории (`danger`, `warning`, …); `[a-z0-9-]+`.
    pub name: String,
    /// Ключ семейства палитры, на которое отображается категория.
    pub family: String,
    /// Минимальный угол оттенка категории (градусы, `[0, 360)`), если задан.
    pub hue_floor_deg: Option<f64>,
    /// Предпочтительная сторона смещения оттенка (`+1` / `-1`), если задана.
    ///
    /// # МЁРТВАЯ РУЧКА после Волны 1 (закон категориальных зон)
    ///
    /// Прежде применялась в вырожденном шве `brand == prototype` p-норм-резолвера.
    /// Волна 1 убрала brand-displacement ЦЕЛИКОМ, поэтому шва больше НЕ существует
    /// и ручка ничего не смещает. Поле СОХРАНЕНО ради стабильности схемы конфига
    /// (течёт в wasm-DTO, JSON-фикстуры, JS-golden) и по-прежнему ВАЛИДИРУЕТСЯ
    /// (`preferred_side ∈ {-1, +1}`) как контракт границы — но законом оттенка
    /// БОЛЬШЕ НЕ ПОТРЕБЛЯЕТСЯ (прецедент «задокументированной мёртвой ручки»
    /// сохранён). Вайринг/удаление — отдельное решение оркестратора.
    pub preferred_side: Option<i8>,
}

/// Конфиг сентиментов: категории + общие ручки различимости.
#[derive(Debug, Clone, PartialEq)]
pub struct SentimentsConfig {
    /// Категории потребителя.
    pub categories: Vec<SentimentCategory>,
    /// Жёсткость p-нормы Sticky Potential Well (`≥ 1`).
    ///
    /// # МЁРТВАЯ РУЧКА после Волны 1 (закон категориальных зон)
    ///
    /// p-норма настраивала brand-displacement — смещение сентимента ОТ бренда.
    /// Категориальный закон убрал brand-displacement ЦЕЛИКОМ (сентимент отдыхает на
    /// фокусе своей категории), поэтому ручка БОЛЬШЕ НЕ ПОТРЕБЛЯЕТСЯ законом
    /// оттенка. Поле СОХРАНЕНО ради стабильности схемы конфига (течёт в wasm-DTO,
    /// JSON-фикстуры, JS-golden) и по-прежнему ВАЛИДИРУЕТСЯ (`≥ 1`) как контракт
    /// границы — но эмитируемые байты больше не двигает.
    pub hardness: f64,
    /// Доля хромы сентимент-цвета от максимума гамута (`(0, 1]`).
    pub chroma_fraction: f64,
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

/// Рецепт роли из ФИЗИЧЕСКОГО меню (типология из [`crate::semantic`]).
///
/// Все рецепты компилируются в [`RoleSpec`]: текст/dJ'/Lc/zero — солвер-роли,
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
        /// Опциональный источник ОТТЕНКА семьи (ратификация ch5c, M1). `None` —
        /// нейтральный лейбл (подтон таблицы, прежний путь). `Some(source)` —
        /// ЦВЕТНОЙ лейбл: держит ТОТ ЖЕ контракт уровня (`fraction`/`floor`), что
        /// нейтральный (одноуровневость по построению), но решённый в чистом
        /// оттенке семьи-источника. Источник валидируется как у лестницы
        /// (существование семейства/сентимента); аддитивен в JSON/DTO —
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
    /// Декоративная роль в единице `Lc` (стек теней): величина, знак — от фона.
    DecorativeLc {
        /// Величина `Lc` (`> 0`).
        magnitude: f64,
    },
    /// Ступень лестницы акцента/сентимента/бренда/нейтрали: `rgba(якорь, α)`
    /// напрямую (поглощает акцентный GAP #59). `source` — откуда берётся тинт,
    /// `position` — позиция закрытого меню (несёт свою альфу; перечень —
    /// приложение A к ADR-0001). Компилируется в [`RoleSpec::Ladder`].
    Ladder {
        /// Источник тинта: бренд, семейство палитры, сентимент или нейтраль.
        source: LadderSource,
        /// Позиция меню (несёт пер-темную пару альф из стаба labui).
        position: LadderPosition,
        /// Опциональный юр. пол UI (ратификация ch5c, M2). `None` — прежний путь
        /// (тинт эмитится как есть). `Some(floor)` — только для СОЛИДНОЙ позиции
        /// (`α=1`, напр. `BorderStrong`): семейный солид обязан держать пол
        /// (3:1); если не держит — минимальный легальный сдвиг по кривой семьи с
        /// честным флагом. Аддитивен в JSON — `{..., floor?: "aa-ui"}`.
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
    /// Заливка пары «поверхность × лейбл» ([`crate::pair`]): якорь источника,
    /// минимально сдвинутый по светлоте до победы перцептивно правильной
    /// стороны лейбла в штатной полярности (Oklab: оттенок/хрома идентичности
    /// целы). Лейбл на такой заливке решается ОБЫЧНЫМ nested resolve — пара
    /// не второй текстовый закон, а подготовка поверхности. Эмиссия — солид
    /// (лестничная сантехника с α = 1).
    PairFill {
        /// Источник якоря: бренд, семейство, сентимент или нейтраль.
        source: LadderSource,
    },
    /// Лейбл ТИНТ-бейджа — лейбл-сторона пары «поверхность × лейбл»
    /// ([`crate::pair`]). Семейно-оттеночный лейбл, чей WCAG-пол энфорсится
    /// ПРОТИВ объявленной тинт-поверхности (композит семейного тинта при
    /// compatibility-альфе позиции `fill-*-primary` над фоном резолва), а НЕ
    /// против фона страницы и НЕ против эмитированного
    /// [`PairFill`](Self::PairFill) — тот решается отдельно и не является
    /// подложкой лейбла. Закрывает класс «контраст label↔tinted-fill
    /// эмерджентен, не гарантирован»: обычные `label-*`/`fill-*-tinted` роли
    /// решаются независимо против фона страницы, и их взаимный контраст на
    /// тинт-подложке бейджа никем не констрейнится (для warning/статусных семей
    /// на кривой оседает к ~3:1 и ниже). Здесь foreground решается ШТАТНЫМ
    /// законом НА тинт-поверхности, собранной appearance-графом (#307), поэтому
    /// пол гарантирован против той подложки, на которой лейбл реально стоит.
    /// Недостижимость пола на кривой семьи выражается флагом `compressed` рядом
    /// с фактическим результатом. Компилируется в
    /// [`RoleSpec::PairLabel`].
    PairLabel {
        /// Источник семьи оттенка/тинта: бренд, семейство, сентимент или нейтраль.
        source: LadderSource,
        /// Доля максимума контраста тинт-поверхности `(0, 1]` (как у
        /// [`TextAnchor`](Self::TextAnchor)): низкая доля = максимально «цветной»
        /// лейбл у пола, высокая = ближе к нейтральному пределу.
        fraction: f64,
        /// WCAG-пол, энфорсимый ПРОТИВ тинт-поверхности (а не фона страницы).
        floor: Floor,
    },
    /// Альфа-аналог солида источника через композит-инверсию ([`crate::alpha`],
    /// #119): `(tint, α)`, чей композит на фоне резолва равен солиду `of`. Даёт
    /// `-tinted`-роли labui. Компилируется в [`RoleSpec::AlphaAnalog`].
    AlphaAnalog {
        /// Источник солид-цели (бренд/семейство/сентимент), чей аналог берётся.
        of: LadderSource,
        /// Запрошенная альфа `(0, 1]` (поднимается до `α_min`, если ниже).
        alpha: f64,
    },
    /// Двухслойный материал (стекло/акрил; канон — `docs/whitepaper.md` §3.7): пара «тинт `01` (с выведенной α)
    /// и опаковая база `02`». База — семейно-оттеночная поверхность на целевом
    /// перцептивном шаге тира; тинт — тот же тон, а альфа выведена из
    /// композит-неравенства над коридором фонов. Компилируется в
    /// [`RoleSpec::Material`].
    ///
    /// Тир (`base/muted/soft/subtle`) — величина `tone` (|ΔJ'|): крупнее =
    /// заметнее/плотнее. Семья — `source`: [`Neutral`](LadderSource::Neutral) даёт
    /// нейтральный материал (подтон таблицы), остальные — семейно-оттеночный
    /// (акцент-стекло/сентимент).
    Material {
        /// Источник ОТТЕНКА семьи: бренд/семейство/сентимент/нейтраль. Нейтраль →
        /// нейтральный материал; иное → семейно-оттеночный.
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
/// и [`Sentiment`](Self::Sentiment) `key` — ссылка на семейство/категорию конфига
/// (валидатор проверяет существование). Сентимент-источник разводит оттенок с
/// брендом сентимент-солвером ([`crate::sentiment`]); при бренде labui резолв
/// сентимента совпадает с сырым якорем семейства (деривационная идентичность —
/// тестом).
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum LadderSource {
    /// Бренд-вход конфига (пер-темный якорь [`Brand`]).
    Brand,
    /// Семейство палитры по ключу (пер-темный якорь [`PaletteFamily`]).
    Family(String),
    /// Сентимент-категория по имени: оттенок семейства, разведённый с брендом
    /// сентимент-солвером (пер-темный солид на разрешённом оттенке).
    Sentiment(String),
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
/// `#[non_exhaustive]`: будущие поля (напр. фоновая лестница дельтами, §4 плана
/// BL-007) станут неломающими. Внешние крейты собирают конфиг через
/// [`ThemeConfig::new`](Self::new), не struct-литералом; поля остаются `pub` для
/// чтения и мутации.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ThemeConfig {
    /// Бренд-вход.
    pub brand: Brand,
    /// Нейтральная шкала + подтон.
    pub neutral: NeutralConfig,
    /// Семейства палитры.
    pub palette: Vec<PaletteFamily>,
    /// Сентимент-политика.
    pub sentiments: SentimentsConfig,
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

use crate::sentiment::ACHROMATIC_CHROMA_EPS;

/// Oklab-хрома hex-цвета (для гарда ахроматичности источников оттенка).
fn oklab_chroma_of_hex(hex: &str) -> f64 {
    match crate::spaces::srgb::srgb_from_hex(hex) {
        Ok(lin) => {
            let lab = crate::spaces::oklab::srgb_linear_to_oklab(lin);
            (lab[1] * lab[1] + lab[2] * lab[2]).sqrt()
        }
        Err(_) => 0.0, // невалидный hex ловится валидатором раньше
    }
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

/// Проверить `min ≤ value ≤ max` (замкнутый интервал).
fn check_in_incl_incl(
    handle: &str,
    value: f64,
    min_incl: f64,
    max_incl: f64,
    bound: &'static str,
) -> Result<(), ConfigError> {
    if value >= min_incl && value <= max_incl {
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
    /// Конструктор существует, потому что [`ThemeConfig`] помечен
    /// `#[non_exhaustive]` (будущие поля — неломающие): внешние крейты (граница
    /// WASM) собирают конфиг через него, а не struct-литералом.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        brand: Brand,
        neutral: NeutralConfig,
        palette: Vec<PaletteFamily>,
        sentiments: SentimentsConfig,
        themes: ThemesConfig,
        roles: Vec<(String, RoleRecipe)>,
        aliases: Vec<(String, String)>,
    ) -> Self {
        ThemeConfig {
            brand,
            neutral,
            palette,
            sentiments,
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
    /// деривационное (ахроматичный источник оттенка, отсутствующие
    /// edge/inverted-четвёрки, пустая легальная дуга сентимента). Первая
    /// найденная ошибка возвращается сразу — клиент чинит по одной.
    ///
    /// # Errors
    ///
    /// Та же [`ConfigError`], которую вернула бы компиляция.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.compile_named_role_table().map(drop)
    }

    /// Структурная фаза валидации: hex, имена, ссылки на семейства/источники
    /// лестницы, дубликаты словарей и пределы каждой экспонируемой ручки.
    /// НЕ полный preflight: деривационные ошибки (ахроматичность, пустая дуга
    /// сентимента) всплывают только в фазе компиляции — снаружи полноту даёт
    /// [`validate`](Self::validate).
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
        check_in_incl_incl(
            "neutral.tint.ratio",
            self.neutral.tint.ratio,
            TINT_RATIO_MIN_INCLUSIVE,
            TINT_RATIO_MAX_INCLUSIVE,
            "0 ≤ ratio ≤ 1 (доля хромы подтона; 0 = серый, 1 = максимум гамута)",
        )?;
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

        // Сентименты: ручки + категории (маппинг на существующее семейство).
        check_ge(
            "sentiments.hardness",
            self.sentiments.hardness,
            HARDNESS_MIN_INCLUSIVE,
            "hardness ≥ 1 (vestigial-контракт схемы; прежде p-норма brand-displacement, снесена Волной 1)",
        )?;
        check_in_excl_incl(
            "sentiments.chroma_fraction",
            self.sentiments.chroma_fraction,
            CHROMA_FRACTION_MIN_EXCLUSIVE,
            CHROMA_FRACTION_MAX_INCLUSIVE,
            "0 < chroma_fraction ≤ 1 (доля хромы сентимента; >1 = за стеной гамута)",
        )?;
        for cat in &self.sentiments.categories {
            let name_field = format!("sentiments.{}.name", cat.name);
            check_name(&name_field, &cat.name)?;
            if !self.palette.iter().any(|f| f.key == cat.family) {
                return Err(ConfigError::UnknownFamily {
                    referenced_by: format!("sentiments.{}", cat.name),
                    family: cat.family.clone(),
                });
            }
            if let Some(side) = cat.preferred_side {
                if side != 1 && side != -1 {
                    return Err(ConfigError::OutOfBounds {
                        handle: format!("sentiments.{}.preferred_side", cat.name),
                        value: f64::from(side),
                        bound: "preferred_side ∈ {-1, +1} (закрытое меню сторон смещения)",
                    });
                }
            }
            if let Some(hue) = cat.hue_floor_deg {
                let field = format!("sentiments.{}.hue_floor_deg", cat.name);
                // Полуинтервал `[0, 360)`: угол по модулю 360°, где 360° ≡ 0°.
                if !(HUE_DOMAIN_MIN_INCLUSIVE..HUE_DOMAIN_MAX_EXCLUSIVE).contains(&hue) {
                    return Err(ConfigError::OutOfBounds {
                        handle: field,
                        value: hue,
                        bound: "0 ≤ hue_floor_deg < 360 (угол оттенка по модулю 360°)",
                    });
                }
            }
        }

        if let Some(hue) = self.neutral.tint.hue_override_deg {
            if !(hue.is_finite()
                && (HUE_DOMAIN_MIN_INCLUSIVE..HUE_DOMAIN_MAX_EXCLUSIVE).contains(&hue))
            {
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
            "sentiments.categories",
            self.sentiments.categories.iter().map(|c| c.name.as_str()),
        )?;
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
                // Цветной лейбл (M1): источник оттенка обязан существовать —
                // та же проверка, что у источника лестницы.
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
                    "0 < fraction ≤ 1 (доля максимального контраста тинт-поверхности бейджа)",
                )
            }
            RoleRecipe::AlphaAnalog { of, alpha } => {
                self.check_ladder_source(role, of)?;
                check_in_excl_incl(
                    &format!("roles.{role}.alpha"),
                    *alpha,
                    ALPHA_MIN_EXCLUSIVE,
                    ALPHA_MAX_INCLUSIVE,
                    "0 < alpha ≤ 1 (запрошенная альфа альфа-аналога)",
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
    /// ссылается на существующее семейство `palette`, [`LadderSource::Sentiment`]
    /// — на существующую категорию `sentiments`; [`LadderSource::Brand`] всегда
    /// разрешим (бренд — обязательный вход конфига).
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
            LadderSource::Sentiment(name) => {
                if self.sentiments.categories.iter().any(|c| &c.name == name) {
                    Ok(())
                } else {
                    Err(ConfigError::UnknownSentiment {
                        referenced_by: format!("roles.{role}"),
                        sentiment: name.clone(),
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
    /// [`ConfigError`] структурной/деривационной фазы либо
    /// [`ConfigError::EmptyContract`] на голом контракте (без ролей и алиасов).
    pub fn compile_named_role_table(&self) -> Result<NamedRoleTable, ConfigError> {
        self.validate_syntactic()?;

        // Пустой контракт — ошибка НА ЗАГРУЗКЕ, не тихий приём: конфиг без
        // ролей/алиасов не эмитил бы ни одной роли, и дефект уехал бы на
        // использование. После структурной фазы: конкретные ошибки полей
        // всплывают раньше.
        if self.roles.is_empty() && self.aliases.is_empty() {
            return Err(ConfigError::EmptyContract);
        }

        let mut entries: Vec<(String, RoleSpec)> = Vec::with_capacity(self.roles.len());
        for (name, recipe) in &self.roles {
            let spec = self.compile_recipe(name, recipe)?;
            entries.push((name.clone(), spec));
        }

        // Нейтраль-подтон: v2-кривая. Оттенок подтона — ИЗ КОНФИГА: явная ручка
        // hue_override (labui несёт измеренную SSOT-величину 286.0°), иначе —
        // деривация из ТЁМНОГО якоря нейтрали клиента (NEUTRAL_HUE_DEG сам был
        // измерен по #101012 → 285.97°; labui-константа для чужой нейтрали была
        // бы чужим подтоном — дефект агностичности). `ratio` в v2-кривую не
        // входит (поле v1 flat-пути), но валидируется как ручка.
        let canonical_hue_deg = match self.neutral.tint.hue_override_deg {
            Some(hue) => hue,
            None => {
                // Ахроматичный якорь не несёт оттенка: atan2(0,0) дал бы
                // произвольный 0° — тихо чужой подтон. Порог технический
                // (числовая определённость), не перцептивный.
                if oklab_chroma_of_hex(&self.neutral.anchors.dark) < ACHROMATIC_CHROMA_EPS {
                    return Err(ConfigError::AchromaticHueSource {
                        field: "neutral.anchors.dark".to_string(),
                    });
                }
                crate::accent::oklab_hue_of(&self.neutral.anchors.dark)
            }
        };
        let chroma = RoleChroma::Curve {
            canonical_hue_deg,
            target_mp: self.neutral.tint.target_mp,
            hue_stiffness: self.neutral.tint.hue_stiffness,
        };

        Ok(NamedRoleTable::from_validated_parts(
            entries,
            self.aliases.clone(),
            chroma,
        ))
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
                // Цветной лейбл (M1): источник оттенка раскладывается в пер-темный
                // тинт-якорь тем же механизмом, что тинт лестницы (для сентимента
                // — солид, разведённый с брендом). Резолв держит контракт уровня в
                // этом оттенке.
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
                // Migration adapter (#292): прежний клиентский wire-ключ
                // (`stable-v1 | legacy-platform-dependent-v1`) лоуверится в
                // generic typed execution mode compiled invocation.
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
                // Нейтральный источник → нейтральный материал (`hue = None`, подтон
                // ТАБЛИЦЫ); семейный → оттенок якоря подставляется в резолве.
                let hue = match source {
                    LadderSource::Neutral(_) => None,
                    _ => Some(self.compile_ladder_tint(role, source)?),
                };
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
                // Поверхность бейджа = семейный тинт при альфе `fill-*-primary`
                // (@12) над фоном резолва. Альфа берётся из ЗАКРЫТОГО меню позиции
                // (не литерал), поэтому tinted-badge лейбл и `fill-*-tinted`
                // заливка всегда садятся на одну и ту же подложку по построению.
                // `FillPrimary` здесь — ТОЛЬКО источник legacy-alpha-данных на
                // этапе lowering (compatibility datum клиентской калибровки);
                // core-семантика и appearance-граф это имя/позицию не знают.
                let (surface_alpha_light, surface_alpha_dark) =
                    crate::ladder::LadderPosition::FillPrimary.alpha_pair();
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
    /// - [`LadderSource::Sentiment`]: пер-темный СОЛИД, чей оттенок разведён с
    ///   брендом сентимент-солвером (`crate::sentiment`); светлота/хрома —
    ///   исходного якоря семейства категории.
    fn compile_ladder_tint(
        &self,
        role: &str,
        source: &LadderSource,
    ) -> Result<LadderTint, ConfigError> {
        let anchors = match source {
            LadderSource::Brand => self.brand.anchors.clone(),
            LadderSource::Family(key) => self.family_anchors(role, key)?.clone(),
            LadderSource::Sentiment(name) => return self.compile_sentiment_tint(role, name),
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

    /// Пер-темный сентимент-солид: для каждой темы взять якорь семейства категории
    /// и разрешить его оттенок законом категориальных зон (покой на прототипе;
    /// пол/зоны — исключения), сохранив светлоту/хрому якоря. Бренд оттенок больше
    /// НЕ смещает (Волна 1). `s_perc_min` — пересчёт из хром якорей сентиментов
    /// конфига (питает попарные зоны фазы-2, не зависит от labui-констант).
    fn compile_sentiment_tint(&self, role: &str, name: &str) -> Result<LadderTint, ConfigError> {
        // Существование категории проверяется до пофазного резолва, чтобы
        // неизвестное имя дало свою ошибку, а не «семья не найдена».
        if !self.sentiments.categories.iter().any(|c| c.name == name) {
            return Err(ConfigError::UnknownSentiment {
                referenced_by: format!("roles.{role}"),
                sentiment: name.to_string(),
            });
        }

        LadderTint::new([
            self.sentiment_solid_for_mode(role, name, 0)?,
            self.sentiment_solid_for_mode(role, name, 1)?,
            self.sentiment_solid_for_mode(role, name, 2)?,
            self.sentiment_solid_for_mode(role, name, 3)?,
        ])
        .map_err(|mode| ConfigError::InvalidHex {
            field: format!("roles.{role} (сентимент-тинт, режим {mode})"),
            value: "<вне кодированного домена>".to_string(),
        })
    }

    /// Солид сентимента `name` в режиме `mode_idx` (0 light / 1 dark /
    /// 2 light-ic / 3 dark-ic) под законом категориальных зон (Волна 1).
    ///
    /// Оркестрация ДВУХФАЗНАЯ (сохранена), но БРЕНД-СВОБОДНАЯ — бренд оттенок
    /// больше не смещает; двигать его может только пол и попарные зоны соседей:
    ///
    /// 1. каждая категория решается на СВОЁМ прототипе с полом (без бренда,
    ///    `resolve_config_sentiment_solid_among` с пустыми зонами);
    ///    ПОКОЯЩИЕСЯ (решённый оттенок = оттенок прототипа, ≤ 0.5°) объявляются
    ///    неподвижными оккупантами — деривационная идентичность якорей клиента
    ///    не нарушается по построению (для labui-якорей ВСЕ покоятся);
    /// 2. СМЕЩЁННЫЕ (пол клампанул оттенок) перерешиваются по порядку конфига с
    ///    зонами оккупантов: угловой отступ от каждой зоны — инверсия хорды
    ///    `s_perc_min` при средней хроме пары (тот же закон различимости
    ///    сентиментов между собой; СОХРАНЁН). Решённый смещённый сам становится
    ///    оккупантом для следующих.
    ///
    /// Ахроматичные оккупанты (C < ε) зон не несут — у серого нет оттенка.
    /// Ахроматичный ЯКОРЬ обрабатывается внутри солвера (сырой якорь).
    fn sentiment_solid_for_mode(
        &self,
        role: &str,
        name: &str,
        mode_idx: usize,
    ) -> Result<[f64; 3], ConfigError> {
        // s_perc_min нужен ТОЛЬКО для попарных зон фазы-2 (различимость
        // сентиментов между собой) — brand-разведение убрано Волной 1.
        let s_perc_min = self.sentiment_s_perc_min()?;
        let pick = |a: &ThemeAnchors| -> String {
            match mode_idx {
                0 => a.light.clone(),
                1 => a.dark.clone(),
                2 => a.light_ic.clone(),
                _ => a.dark_ic.clone(),
            }
        };

        let anchor_of = |cat: &SentimentCategory| -> Result<String, ConfigError> {
            Ok(pick(self.family_anchors(role, &cat.family)?))
        };

        // Закон категориальных зон: оттенок семейства ОТДЫХАЕТ на своём прототипе,
        // бренд его не смещает (ахроматичный якорь обрабатывается внутри солвера).
        // Пол/зоны — единственные исключения.
        let solve = |cat: &SentimentCategory,
                     anchor_hex: &str,
                     zones: &[crate::sentiment::NeighborZone]|
         -> Result<String, ConfigError> {
            crate::sentiment::resolve_config_sentiment_solid_among(
                anchor_hex,
                self.sentiments.chroma_fraction,
                cat.hue_floor_deg,
                zones,
            )
            .map_err(|reason| ConfigError::SentimentResolution {
                role: role.to_string(),
                sentiment: cat.name.clone(),
                reason,
            })
        };

        // Фаза 1: резолв всех категорий на прототипе с полом; классификация покоя.
        // (hue, chroma) занятых зон меряются на РЕШЁННОМ солиде — честный
        // оккупант, не идеал.
        let mut occupied: Vec<(f64, f64)> = Vec::new();
        let mut displaced: Vec<(usize, String)> = Vec::new(); // (index, anchor)
        let mut target_phase1: Option<String> = None;
        for (i, cat) in self.sentiments.categories.iter().enumerate() {
            let anchor_hex = anchor_of(cat)?;
            let solid = solve(cat, &anchor_hex, &[])?;
            let proto_hue = crate::accent::oklab_hue_of(&anchor_hex);
            let solid_hue = crate::accent::oklab_hue_of(&solid);
            let solid_chroma = oklab_chroma_of_hex(&solid);
            let rested = crate::sentiment::angular_distance(solid_hue, proto_hue) <= 0.5
                || solid_chroma < ACHROMATIC_CHROMA_EPS;
            if rested {
                if solid_chroma >= ACHROMATIC_CHROMA_EPS {
                    occupied.push((solid_hue, solid_chroma));
                }
                if cat.name == name {
                    target_phase1 = Some(solid);
                }
            } else {
                displaced.push((i, anchor_hex));
            }
        }
        if let Some(solid) = target_phase1 {
            // Покоящаяся цель неподвижна по построению — байт-в-байт фаза 1.
            return crate::spaces::srgb::srgb_encoded_from_hex(&solid).map_err(|_| {
                ConfigError::InvalidHex {
                    field: format!("roles.{role} (сентимент-солид)"),
                    value: solid,
                }
            });
        }

        // Фаза 2: смещённые перерешиваются с зонами оккупантов, по порядку.
        for (i, anchor_hex) in displaced {
            let cat = &self.sentiments.categories[i];
            let c_self = oklab_chroma_of_hex(&anchor_hex);
            let mut zones = Vec::with_capacity(occupied.len());
            for &(hue, c_other) in &occupied {
                let pair_chroma = (c_self + c_other) / 2.0;
                let min_sep = crate::sentiment::s_min_deg_from_chord(s_perc_min, pair_chroma)
                    .map_err(|reason| ConfigError::SentimentResolution {
                        role: role.to_string(),
                        sentiment: cat.name.clone(),
                        reason,
                    })?;
                // Сатурация (маркер 180° из s_min_deg_from_chord): пара так
                // приглушена, что категориальная хорда недостижима при любом угле —
                // перцептивно НЕРАЗДЕЛИМА. Отступ бессмыслен, зону пропускаем: иначе
                // legalize искал бы точный антипод (мера нуль) и вернул ложный
                // «пустая дуга». Дормантно для labui (все сентименты хромны).
                if min_sep >= 180.0 {
                    continue;
                }
                zones.push(crate::sentiment::NeighborZone {
                    hue_deg: hue,
                    min_sep_deg: min_sep,
                });
            }
            let solid = solve(cat, &anchor_hex, &zones)?;
            if cat.name == name {
                return crate::spaces::srgb::srgb_encoded_from_hex(&solid).map_err(|_| {
                    ConfigError::InvalidHex {
                        field: format!("roles.{role} (сентимент-солид)"),
                        value: solid,
                    }
                });
            }
            let solid_hue = crate::accent::oklab_hue_of(&solid);
            let solid_chroma = oklab_chroma_of_hex(&solid);
            if solid_chroma >= ACHROMATIC_CHROMA_EPS {
                occupied.push((solid_hue, solid_chroma));
            }
        }
        unreachable!("категория `{name}` обязана быть покоящейся или смещённой")
    }

    /// `S_PERC_MIN`, пересчитанный из Oklab-хром светлых якорей 4 (или скольких
    /// есть) сентимент-категорий конфига — закон `2·C_rep·sin(20°/2)`.
    /// При labui-якорях == замороженная константа (тест-идентичность).
    /// # Errors
    ///
    /// `Err`, если категория ссылается на несуществующее семейство или якорь
    /// семейства — невалидный hex: порог разделения, посчитанный по НЕПОЛНОМУ
    /// набору категорий, был бы тихой математической ложью.
    pub fn sentiment_s_perc_min(&self) -> Result<f64, ConfigError> {
        let mut chromas = Vec::with_capacity(self.sentiments.categories.len());
        for c in &self.sentiments.categories {
            let fam = self
                .palette
                .iter()
                .find(|f| f.key == c.family)
                .ok_or_else(|| ConfigError::UnknownFamily {
                    referenced_by: format!("sentiments.{}", c.name),
                    family: c.family.clone(),
                })?;
            let lin = crate::spaces::srgb::srgb_from_hex(&fam.anchors.light).map_err(|_| {
                ConfigError::InvalidHex {
                    field: format!("palette.{}.anchors.light", fam.key),
                    value: fam.anchors.light.clone(),
                }
            })?;
            let lab = crate::spaces::oklab::srgb_linear_to_oklab(lin);
            chromas.push((lab[1] * lab[1] + lab[2] * lab[2]).sqrt());
        }
        Ok(crate::sentiment::s_perc_min_from_chromas(&chromas))
    }
}

/// Словарь эталонного пресета labui — ТОЛЬКО семантика ролей/алиасов (ни одного
/// цветового значения). `#[cfg(test)]`-only (ADR-0001 PR-c): labui-дерево ушло из
/// ОТГРУЖАЕМОГО кода ядра — прод-скан агностичности
/// (`tests/agnostic_cleanliness.rs`) его не видит. Единственный потребитель —
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
