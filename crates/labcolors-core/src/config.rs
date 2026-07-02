//! Граница конфига: типы, которыми потребитель движка (дизайн-система) задаёт
//! свою семантику, и компиляция этой семантики в физическую [`NamedRoleTable`].
//!
//! Разделение зон (ADR-0001, `docs/decisions/0001-config-boundary.md`): движок не
//! знает ни одного имени роли и ни одного брендового значения. Всё, что меняется
//! при смене клиента студии, живёт здесь, в [`ThemeConfig`]; всё, что является
//! законом восприятия / математикой / WCAG, остаётся физикой ядра
//! ([`crate::semantic`], [`crate::solve`]). Направление зависимостей — только
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
//!   встроенную [`crate::RoleTable`].
//!
//! # Рецепты лестницы и альфа-аналога
//!
//! [`RoleRecipe::Ladder`] (акцентная/сентимент/бренд-лестница, поглощает GAP #59)
//! компилируется в [`RoleSpec::Ladder`]: источник раскладывается в пер-темный
//! тинт-якорь ([`crate::ladder::LadderTint`]), позиция несёт альфу Figma-рампы.
//! [`RoleRecipe::AlphaAnalog`] компилируется в [`RoleSpec::AlphaAnalog`] (солид-
//! цель источника + запрошенная альфа, композит-инверсия — [`crate::alpha`], #119).
//! Резолв обоих — [`crate::semantic::Resolved::Rgba`] (rgba напрямую + солид-
//! композит на фоне резолва для замера контраста). Меню позиций + провенанс —
//! приложение A к `docs/decisions/0001-config-boundary.md`.

use crate::ladder::{LadderPosition, LadderTint, ThemeAnchors};
use crate::semantic::{self, DjMagnitude, NamedRoleTable, RoleChroma, RoleSpec, TextAnchor};
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
/// фона (всегда [`crate::Unreachable`]). Верхняя граница включает `1.0` — это
/// «почти максимум, который фон физически позволяет» (сам движок клампит долю чуть
/// ниже единицы, см. [`TextAnchor::new`]).
const FRACTION_MIN_EXCLUSIVE: f64 = 0.0;
/// Верхний предел доли текстового якоря (включительно).
const FRACTION_MAX_INCLUSIVE: f64 = 1.0;

/// dJ'-якорь декоративной роли обязан быть строго положительным.
///
/// dJ' — перцептивная разница светлоты (шаг `J'`), которую роль держит от своей
/// поверхности. `dj ≤ 0` означает «нет различимого шага» — вырожденная роль,
/// неотличимая от фона. Верхнего предела нет по величине как таковой: физика сама
/// вернёт [`crate::Unreachable`], если якорь недостижим на данном фоне, — но
/// нулевой/отрицательный якорь это ошибка КОНФИГА, а не недостижимость.
const DJ_MIN_EXCLUSIVE: f64 = 0.0;

/// Lc-величина декоративной роли (тени) обязана быть строго положительной.
///
/// Единица — воспринимаемый контраст `Lc`; знак выбирает физика от фона, поэтому
/// конфиг несёт величину (модуль). `magnitude ≤ 0` — невидимая тень (вырождение).
/// Движок дополнительно поднимает величину до порога квантования
/// ([`DECORATIVE_FLOOR_MIN`](crate::semantic)), так что валидатор проверяет лишь
/// положительность как контракт конфига.
const DECORATIVE_LC_MIN_EXCLUSIVE: f64 = 0.0;

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

/// Жёсткость сентимент-разделения (`sentiments.hardness`) обязана быть `≥ 1`.
///
/// Это p-норма модели Sticky Potential Well (#55): `p → ∞` восстанавливает жёсткую
/// стену 20°, `p → 1` — самый мягкий изгиб. `p < 1` выводит p-норму из
/// корректной области (перестаёт быть нормой). Дефолт реестра — `5.0`.
const HARDNESS_MIN_INCLUSIVE: f64 = 1.0;

/// Доля хромы сентимент-цвета (`sentiments.chroma_fraction`) обязана лежать в
/// `(0, 1]`.
///
/// Каждый сентимент-цвет несёт `chroma_fraction · max_chroma` на своей светлоте.
/// `≤ 0` — обесцвеченный (не сентимент), `> 1` — за стеной гамута (неон/недостижимо).
/// Дефолт реестра — `0.88` (держится `< 1`, чтобы сидеть внутри стены гамута, не
/// читаясь как неон).
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
/// «невалидный hex», «ручка вне предела», «ссылка на несуществующее семейство» и
/// «рецепт ещё не реализован». Реализована вручную (без `thiserror`) — крейт
/// `labcolors-core` держит НОЛЬ runtime-зависимостей (issue #29); стиль `Display`
/// повторяет ручные ошибки ядра.
///
/// NB: `PartialEq` наследует IEEE-семантику `f64`-полей —
/// `OutOfBounds { value: NaN, .. }` НЕ равен самому себе (как и сам `f64`);
/// для сравнения NaN-ошибок матчитесь по варианту, не по равенству.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ConfigError {
    /// Невалидная hex-строка цвета (`#RGB` / `#RRGGBB`). `field` — путь до поля в
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
    /// Дубликат ключа в словаре конфига: повтор имени сделал бы lookup и
    /// эмиссию неоднозначными (какая запись выиграла — вопрос порядка, тихо).
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
    /// Рецепт объявлен в меню, но его компиляция ещё не реализована — честная
    /// заглушка для БУДУЩИХ рецептов (все текущие компилируются; вариант
    /// сохранён как сеам для расширения меню без ломающего изменения).
    NotYetImplemented { recipe: &'static str, role: String },
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
            ConfigError::NotYetImplemented { recipe, role } => write!(
                f,
                "рецепт `{recipe}` (роль `{role}`) ещё не реализован ядром"
            ),
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
    pub preferred_side: Option<i8>,
}

/// Конфиг сентиментов: категории + общие ручки различимости.
#[derive(Debug, Clone, PartialEq)]
pub struct SentimentsConfig {
    /// Категории потребителя.
    pub categories: Vec<SentimentCategory>,
    /// Жёсткость p-нормы Sticky Potential Well (`≥ 1`).
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
/// [`Ladder`](Self::Ladder) / [`AlphaAnalog`](Self::AlphaAnalog) — rgba-эмиссия.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RoleRecipe {
    /// Текстовый/UI-якорь: доля от максимального контраста фона + WCAG-пол.
    TextAnchor {
        /// Доля максимума контраста, `(0, 1]`.
        fraction: f64,
        /// WCAG-пол читаемости.
        floor: Floor,
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
    /// Явный ноль: «нет цвета здесь» ([`RoleSpec::Zero`]).
    Zero,
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
#[derive(Debug, Clone, PartialEq)]
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

/// Проверить, что hex парсится ядром (`#RGB` / `#RRGGBB`).
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
            "hardness ≥ 1 (p-норма Sticky Potential Well; p < 1 не норма)",
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
            if let Some(side) = cat.preferred_side
                && side != 1
                && side != -1
            {
                return Err(ConfigError::OutOfBounds {
                    handle: format!("sentiments.{}.preferred_side", cat.name),
                    value: f64::from(side),
                    bound: "preferred_side ∈ {-1, +1} (закрытое меню сторон смещения)",
                });
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

        if let Some(hue) = self.neutral.tint.hue_override_deg
            && !(hue.is_finite()
                && (HUE_DOMAIN_MIN_INCLUSIVE..HUE_DOMAIN_MAX_EXCLUSIVE).contains(&hue))
        {
            return Err(ConfigError::OutOfBounds {
                handle: "neutral.tint.hue_override_deg".to_string(),
                value: hue,
                bound: "0 ≤ hue < 360 (явный оттенок подтона по модулю 360°)",
            });
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

        Ok(())
    }

    /// Провалидировать пределы ручек одного рецепта роли.
    fn validate_recipe(&self, role: &str, recipe: &RoleRecipe) -> Result<(), ConfigError> {
        match recipe {
            RoleRecipe::TextAnchor { fraction, .. } => check_in_excl_incl(
                &format!("roles.{role}.fraction"),
                *fraction,
                FRACTION_MIN_EXCLUSIVE,
                FRACTION_MAX_INCLUSIVE,
                "0 < fraction ≤ 1 (доля максимального контраста фона)",
            ),
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
            RoleRecipe::DecorativeLc { magnitude } => check_gt(
                &format!("roles.{role}.magnitude"),
                *magnitude,
                DECORATIVE_LC_MIN_EXCLUSIVE,
                "magnitude > 0 (Lc-величина тени; ≤ 0 = невидима)",
            ),
            RoleRecipe::Ladder { source, .. } => self.check_ladder_source(role, source),
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
    /// физикой, что и встроенную [`crate::RoleTable`].
    ///
    /// Структурная фаза ([`validate_syntactic`](Self::validate_syntactic))
    /// выполняется первой; деривационные ошибки возвращаются по ходу компиляции
    /// (снаружи обе фазы разом — [`validate`](Self::validate), который и есть
    /// эта компиляция с отброшенным результатом — НЕ вызывать её отсюда:
    /// рекурсия). [`Ladder`](RoleRecipe::Ladder) раскладывает источник в
    /// пер-темный тинт, [`AlphaAnalog`](RoleRecipe::AlphaAnalog) — солид-цель
    /// источника + альфа.
    pub fn compile_named_role_table(&self) -> Result<NamedRoleTable, ConfigError> {
        self.validate_syntactic()?;

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

        Ok(NamedRoleTable::new(entries, self.aliases.clone(), chroma))
    }

    /// Скомпилировать один рецепт в [`RoleSpec`]. Ladder/AlphaAnalog раскладывают
    /// источник в пер-темный тинт (`Copy`-payload [`LadderTint`]) на этапе
    /// компиляции — резолв остаётся bg-зависимым только через фон подложки.
    fn compile_recipe(&self, role: &str, recipe: &RoleRecipe) -> Result<RoleSpec, ConfigError> {
        match recipe {
            RoleRecipe::TextAnchor { fraction, floor } => {
                Ok(RoleSpec::Anchor(TextAnchor::new(*fraction, *floor)))
            }
            RoleRecipe::DjAnchor { light, dark } => Ok(RoleSpec::DecorativeDj {
                magnitude_dj: DjMagnitude::new(*light, *dark),
            }),
            RoleRecipe::DecorativeLc { magnitude } => Ok(RoleSpec::Decorative {
                magnitude: *magnitude,
            }),
            RoleRecipe::Zero => Ok(RoleSpec::Zero),
            RoleRecipe::Ladder { source, position } => {
                let (alpha_light, alpha_dark) = position.alpha_pair();
                Ok(RoleSpec::Ladder {
                    tint: self.compile_ladder_tint(role, source)?,
                    alpha_light,
                    alpha_dark,
                })
            }
            RoleRecipe::AlphaAnalog { of, alpha } => Ok(RoleSpec::AlphaAnalog {
                of: self.compile_ladder_tint(role, of)?,
                alpha: *alpha,
            }),
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

    /// Пер-темный сентимент-солид: для каждой темы взять якорь семейства
    /// категории, развести оттенок с пер-темным брендом сентимент-солвером,
    /// сохранив светлоту/хрому якоря. `S_PERC_MIN` — пересчёт из хром якорей
    /// сентиментов конфига (закон не зависит от labui-констант).
    fn compile_sentiment_tint(&self, role: &str, name: &str) -> Result<LadderTint, ConfigError> {
        let cat = self
            .sentiments
            .categories
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| ConfigError::UnknownSentiment {
                referenced_by: format!("roles.{role}"),
                sentiment: name.to_string(),
            })?;
        let fam = self.family_anchors(role, &cat.family)?.clone();
        let brand = self.brand.anchors.clone();
        let s_perc_min = self.sentiment_s_perc_min()?;

        let solid_of = |anchor_hex: &str, brand_hex: &str| -> Result<[f64; 3], ConfigError> {
            // Серый бренд не несёт оттенка — разведение по hue бессмысленно и
            // численно не определено: сентимент честно остаётся сырым якорем
            // семейства (ни с чем не сливается по оттенку).
            if oklab_chroma_of_hex(brand_hex) < ACHROMATIC_CHROMA_EPS {
                return crate::spaces::srgb::srgb_encoded_from_hex(anchor_hex).map_err(|_| {
                    ConfigError::InvalidHex {
                        field: format!("roles.{role} (якорь сентимента)"),
                        value: anchor_hex.to_string(),
                    }
                });
            }
            let brand_hue = crate::accent::oklab_hue_of(brand_hex);
            let solid = crate::sentiment::resolve_config_sentiment_solid(
                anchor_hex,
                brand_hue,
                self.sentiments.hardness,
                self.sentiments.chroma_fraction,
                cat.hue_floor_deg,
                cat.preferred_side.map_or(1.0, f64::from),
                s_perc_min,
            )
            .map_err(|reason| ConfigError::SentimentResolution {
                role: role.to_string(),
                sentiment: name.to_string(),
                reason,
            })?;
            crate::spaces::srgb::srgb_encoded_from_hex(&solid).map_err(|_| {
                ConfigError::InvalidHex {
                    field: format!("roles.{role} (сентимент-солид)"),
                    value: solid.clone(),
                }
            })
        };

        LadderTint::new([
            solid_of(&fam.light, &brand.light)?,
            solid_of(&fam.dark, &brand.dark)?,
            solid_of(&fam.light_ic, &brand.light_ic)?,
            solid_of(&fam.dark_ic, &brand.dark_ic)?,
        ])
        .map_err(|mode| ConfigError::InvalidHex {
            field: format!("roles.{role} (сентимент-тинт, режим {mode})"),
            value: "<вне кодированного домена>".to_string(),
        })
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

// ─────────────────────────────────────────────────────────────────────────────
// Эталонная фикстура labui.
// ─────────────────────────────────────────────────────────────────────────────

/// КАНОНИЧЕСКИЙ конфиг labui (не тестовая фикстура): значения = замеры
/// Figma/стаба; публичен намеренно — до переезда данными пакета
/// `@labpics/colors` это единственный носитель labui-семантики у движка.
///
/// Эталонный конфиг labui — 20 нейтральных ролей ядра (байт-в-байт
/// с [`RoleTable::default`](crate::RoleTable)) плюс акцент/сентимент/FX/альфа-роли
/// лестницы и алиасы — полное покрытие consumedRoles labui-контракта.
///
/// Имена ролей = `Role::key()` сегодняшнего ядра; рецепты сняты 1:1 из
/// [`RoleTable::default`](crate::RoleTable) (`semantic.rs`): текст-фракции с их
/// WCAG-полами, dJ'-якоря fill/border из тех же констант, Lc-тени, zero. Нейтраль —
/// тройка Даниила `#FFFFFF`/`#787880`/`#101012`; ручки подтона — из констант
/// `semantic.rs` (единый источник истины, фикстура не может тихо разойтись с ядром).
///
/// Байт-в-байт тест доказывает: [`resolve_named_set`](crate::semantic::resolve_named_set)
/// этой фикстуры эмитит идентично [`resolve_set`](crate::resolve_set) дефолтной
/// таблицы на всех 240 точках golden-грида (лестница/альфа — сверх этой таблицы).
pub fn labui_reference() -> ThemeConfig {
    // Фракции и полы — 1:1 из RoleTable::default (semantic.rs), включая border-strong
    // = контракт label-primary. Рецепты собраны так, чтобы имя роли совпадало с
    // Role::key(), а RoleSpec был идентичен дефолтному.
    let text = |fraction, floor| RoleRecipe::TextAnchor { fraction, floor };
    let lc = |magnitude| RoleRecipe::DecorativeLc { magnitude };
    // Конструктор нейтрального источника (стаб: `Neutral/Derivable` тинтуется
    // краями нейтральной шкалы, НЕ семейством палитры).
    let neutral_pos = |pick, position| RoleRecipe::Ladder {
        source: LadderSource::Neutral(pick),
        position,
    };

    // Конструкторы лестницы: источник × позиция → рецепт rgba-эмиссии.
    let brand_pos = |position| RoleRecipe::Ladder {
        source: LadderSource::Brand,
        position,
    };
    let sent_pos = |name: &str, position| RoleRecipe::Ladder {
        source: LadderSource::Sentiment(name.to_string()),
        position,
    };

    let mut roles = vec![
        // Labels.
        ("label-primary".to_string(), text(0.968, Floor::AaText)),
        ("label-secondary".to_string(), text(0.627, Floor::AaText)),
        ("label-tertiary".to_string(), text(0.461, Floor::AaUi)),
        ("label-quaternary".to_string(), text(0.276, Floor::None)),
        // Icon.
        ("icon".to_string(), text(0.461, Floor::AaUi)),
        // Separator — Lc decorative.
        ("separator".to_string(), lc(8.0)),
        // Border ladder. Strong = label-primary контракт; base/soft — лестница
        // от нейтрали: полупрозрачный mid-тинт ложится на ЛЮБУЮ поверхность
        // (композитит браузер), пер-темные пары альф — данные позиций.
        ("border-strong".to_string(), text(0.968, Floor::AaText)),
        (
            "border-base".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralBorderBase),
        ),
        (
            "border-soft".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralBorderSoft),
        ),
        ("border-ghost".to_string(), RoleRecipe::Zero),
        // Fill ladder — лестница от нейтрали (та же форма, что стаб labui:
        // rgba(mid, α) с пер-темной парой — заливка обязана красиво ложиться
        // на любой фон, солвер-солид терял полупрозрачность).
        (
            "fill-primary".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralFillPrimary),
        ),
        (
            "fill-secondary".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralFillSecondary),
        ),
        (
            "fill-tertiary".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralFillTertiary),
        ),
        (
            "fill-quaternary".to_string(),
            neutral_pos(NeutralPick::Mid, LadderPosition::NeutralFillQuaternary),
        ),
        ("fill-none".to_string(), RoleRecipe::Zero),
        // Shadow stack — Lc.
        ("shadow-minor".to_string(), lc(semantic::SHADOW_MINOR_JND)),
        (
            "shadow-ambient".to_string(),
            lc(semantic::SHADOW_AMBIENT_JND),
        ),
        (
            "shadow-penumbra".to_string(),
            lc(semantic::SHADOW_PENUMBRA_JND),
        ),
        ("shadow-major".to_string(), lc(semantic::SHADOW_MAJOR_JND)),
        // Универсальный ноль.
        ("none".to_string(), RoleRecipe::Zero),
    ];

    // ── Акцентная/сентимент/FX/альфа-лестница (поглощает GAP #59) ─────────────
    // Имена = consumedRoles labui (roles.json) без префикса `--lab-`, минус
    // удаляемые по коллапсу (static-*/inverted-*/on-*/material-*, роли-от-фона).
    // Каждая семья (brand + 4 сентимента) несёт label×4 · fill×4 · border(strong/
    // base/soft). FX focus-ring/glow — солид/@52. `-tinted` — альфа-аналог солида
    // соответствующего fill-*-primary. Все альфы — из меню LadderPosition (Figma).
    let ladder_family = |prefix: &str, mk: &dyn Fn(LadderPosition) -> RoleRecipe| {
        use LadderPosition::*;
        vec![
            (format!("label-{prefix}-primary"), mk(LabelPrimary)),
            (format!("label-{prefix}-secondary"), mk(LabelSecondary)),
            (format!("label-{prefix}-tertiary"), mk(LabelTertiary)),
            (format!("label-{prefix}-quaternary"), mk(LabelQuaternary)),
            (format!("fill-{prefix}-primary"), mk(FillPrimary)),
            (format!("fill-{prefix}-secondary"), mk(FillSecondary)),
            (format!("fill-{prefix}-tertiary"), mk(FillTertiary)),
            (format!("fill-{prefix}-quaternary"), mk(FillQuaternary)),
            (format!("border-{prefix}-strong"), mk(BorderStrong)),
            (format!("border-{prefix}-base"), mk(BorderBase)),
            (format!("border-{prefix}-soft"), mk(BorderSoft)),
        ]
    };

    // Brand-семья: источник = бренд.
    roles.extend(ladder_family("brand", &brand_pos));
    // Сентимент-семьи: источник = сентимент-категория (разводится с брендом).
    for (prefix, sname) in [
        ("danger", "danger"),
        ("warning", "warning"),
        ("success", "success"),
        ("info", "info"),
    ] {
        let mk = move |pos| sent_pos(sname, pos);
        roles.extend(ladder_family(prefix, &mk));
    }

    // FX focus-ring (солид) и glow (@52). Сентимент/бренд-источники — акцентные;
    // `*-neutral`/`inverted` — НЕЙТРАЛЬНЫЕ (стаб: rgb(255 255 255 / .522) и т.п.,
    // НЕ бренд).
    roles.push((
        "fx-focus-ring-brand".to_string(),
        brand_pos(LadderPosition::FocusRing),
    ));
    roles.push((
        "fx-focus-ring-danger".to_string(),
        sent_pos("danger", LadderPosition::FocusRing),
    ));
    roles.push((
        "fx-focus-ring-warning".to_string(),
        sent_pos("warning", LadderPosition::FocusRing),
    ));
    // Нейтральный фокус: тёмный край нейтрали, солид (стаб light rgb(16 16 18) =
    // Контур нейтрали ПЕР-ТЕМНЫЙ (стаб: light #101012 / dark #F6F8FA) — едет
    // на neutral.edge (дублирование одного края дало бы невидимое кольцо
    // фокуса на тёмной теме). В точном value-тесте — обе темы.
    roles.push((
        "fx-focus-ring-neutral".to_string(),
        neutral_pos(NeutralPick::Edge, LadderPosition::FocusRing),
    ));
    roles.push(("fx-glow-brand".to_string(), brand_pos(LadderPosition::Glow)));
    roles.push((
        "fx-glow-danger".to_string(),
        sent_pos("danger", LadderPosition::Glow),
    ));
    roles.push((
        "fx-glow-warning".to_string(),
        sent_pos("warning", LadderPosition::Glow),
    ));
    // Нейтральное свечение: светлый край нейтрали @52 (стаб rgb(255 255 255 / .522)).
    roles.push((
        "fx-glow-neutral".to_string(),
        neutral_pos(NeutralPick::Light, LadderPosition::Glow),
    ));
    // Инвертированное свечение — на neutral.inverted (пер-темная пара стаба
    // #B0B0B9 / #3C3C43 дословно). В точном value-тесте — обе темы.
    roles.push((
        "fx-glow-inverted".to_string(),
        neutral_pos(NeutralPick::Inverted, LadderPosition::Glow),
    ));
    // Skeleton — нейтральный тинт #787880 (стаб rgb(120 120 128 / …)), ПЕР-ТЕМНАЯ
    // альфа: base light @8 / dark @12, highlight @4. Источник = Neutral(Mid).
    roles.push((
        "fx-skeleton-base".to_string(),
        neutral_pos(NeutralPick::Mid, LadderPosition::SkeletonBase),
    ));
    roles.push((
        "fx-skeleton-highlight".to_string(),
        neutral_pos(NeutralPick::Mid, LadderPosition::SkeletonHighlight),
    ));

    // Компонентные роли. accent = бренд, danger = danger-сентимент, neutral —
    // НЕЙТРАЛЬНЫЙ (стаб: fill-neutral солид-литерал; fill-neutral-tinted и
    // border-neutral алиасят нейтральные core-роли fill-primary/border-base).
    //
    // Солид-роль (`fill-accent`) = лестница LabelPrimary (солид, α=1). `-tinted` —
    // ЗАЛИВКА при низкой альфе (rgba напрямую), то есть Ladder FillPrimary: тинт
    // = якорь источника, α = @12. (AlphaAnalog-рецепт — для инверсии УЖЕ
    // РЕШЁННОГО контраст-солида, отдельный случай #119; здесь тинт-якорь эмитится
    // напрямую, поэтому Ladder, а не инверсия — иначе солид над белым дал бы
    // α_min≈1 и «-tinted» перестал быть полупрозрачным.)
    roles.push((
        "fill-accent".to_string(),
        brand_pos(LadderPosition::LabelPrimary),
    ));
    // fill-neutral — солид-литерал стаба без engine-деривации; приближен
    // солидом Neutral(Mid) и потому исключён из точного value-теста.
    roles.push((
        "fill-neutral".to_string(),
        neutral_pos(NeutralPick::Mid, LadderPosition::LabelPrimary),
    ));
    roles.push((
        "fill-danger".to_string(),
        sent_pos("danger", LadderPosition::LabelPrimary),
    ));
    roles.push((
        "fill-accent-tinted".to_string(),
        brand_pos(LadderPosition::FillPrimary),
    ));
    // fill-neutral-tinted = var(fill-primary) → алиас на нейтральную core-заливку.
    // border-neutral = var(border-base) → алиас (см. aliases ниже).
    roles.push((
        "fill-danger-tinted".to_string(),
        sent_pos("danger", LadderPosition::FillPrimary),
    ));
    roles.push((
        "label-accent".to_string(),
        brand_pos(LadderPosition::LabelPrimary),
    ));
    roles.push((
        "label-danger".to_string(),
        sent_pos("danger", LadderPosition::LabelPrimary),
    ));
    roles.push((
        "border-accent".to_string(),
        brand_pos(LadderPosition::BorderBase),
    ));
    // border-neutral = var(border-base): алиас на нейтральную dJ' границу core.
    roles.push((
        "border-danger".to_string(),
        sent_pos("danger", LadderPosition::BorderBase),
    ));
    roles.push((
        "border-focus".to_string(),
        brand_pos(LadderPosition::FocusRing),
    ));

    ThemeConfig {
        brand: Brand {
            // Пер-темный бренд labui (reference/labui-accent-primitives.md §2,
            // Figma `Accent/Brand`): light/dark/light-ic/dark-ic — дословно.
            anchors: anchors("#007AFF", "#4A8FFF", "#0040DD", "#409CFF"),
        },
        neutral: NeutralConfig {
            anchors: NeutralAnchors {
                light: "#FFFFFF".to_string(),
                mid: "#787880".to_string(),
                dark: "#101012".to_string(),
            },
            tint: NeutralTint {
                // Ручки подтона — из констант semantic.rs (единый источник истины).
                ratio: semantic::NEUTRAL_TINT_RATIO,
                target_mp: semantic::TINT_TARGET_MP,
                hue_stiffness: semantic::TINT_HUE_STIFFNESS,
                // Явный измеренный оттенок (SSOT NEUTRAL_HUE_DEG): labui несёт
                // замер, деривация из тёмного якоря — путь клиентов без замера.
                hue_override_deg: Some(semantic::NEUTRAL_HUE_DEG),
            },
            // Пер-темные края (стаб labui дословно; IC = дубль базовых — стаб
            // без ic-скоупов, наследование как у альф):
            // контур — light #101012 / dark #F6F8FA; инверт — #B0B0B9 / #3C3C43.
            edge: Some(crate::ladder::ThemeAnchors {
                light: "#101012".to_string(),
                dark: "#F6F8FA".to_string(),
                light_ic: "#101012".to_string(),
                dark_ic: "#F6F8FA".to_string(),
            }),
            inverted: Some(crate::ladder::ThemeAnchors {
                light: "#B0B0B9".to_string(),
                dark: "#3C3C43".to_string(),
                light_ic: "#B0B0B9".to_string(),
                dark_ic: "#3C3C43".to_string(),
            }),
        },
        // Палитра labui — 10 замеренных семейств, ПЕР-ТЕМНО ДОСЛОВНО из
        // reference/labui-accent-primitives.md §2 (Figma `Accent/*`, все 4 режима,
        // замер 2026-07-02). Светлый якорь совпадает с accent.rs::anchor_hex.
        palette: vec![
            fam("red", "#FF3B30", "#FF3A3A", "#D70015", "#FF6161"),
            fam("orange", "#FFA100", "#FF9008", "#C93400", "#FFA940"),
            fam("yellow", "#FFD000", "#FFD60A", "#B25000", "#FFD426"),
            fam("green", "#34C759", "#30D158", "#248A3D", "#30DB5B"),
            fam("teal", "#5AC8FA", "#64D2FF", "#0071A4", "#70D7FF"),
            fam("mint", "#00C7BE", "#63E6E2", "#0C817B", "#6CEBE7"),
            fam("blue", "#3E87FF", "#5696FF", "#0050CF", "#95C0FF"),
            fam("indigo", "#5856D6", "#5E5CE6", "#3634A3", "#7D7AFF"),
            fam("purple", "#AF52DE", "#BF5AF2", "#8944AB", "#DA8FFF"),
            fam("pink", "#FF2D55", "#FF2D55", "#D30F45", "#FF6482"),
        ],
        sentiments: SentimentsConfig {
            categories: vec![
                sentiment("danger", "red", None, None),
                sentiment("warning", "orange", Some(45.0), Some(1)),
                sentiment("success", "green", None, None),
                sentiment("info", "blue", None, None),
            ],
            hardness: 5.0,
            chroma_fraction: 0.88,
        },
        themes: ThemesConfig {
            entries: vec![
                ("light".to_string(), VcPreset::Srgb),
                ("dark".to_string(), VcPreset::Dim),
                ("light-ic".to_string(), VcPreset::SrgbIc),
                ("dark-ic".to_string(), VcPreset::DimIc),
            ],
        },
        roles,
        // Компонентные нейтральные роли, которые стаб алиасит через var() на
        // нейтральные core-роли (одна истина, ноль дублирования значений):
        // fill-neutral-tinted = var(--lab-fill-primary); border-neutral = var(--lab-border-base).
        aliases: vec![
            (
                "fill-neutral-tinted".to_string(),
                "fill-primary".to_string(),
            ),
            ("border-neutral".to_string(), "border-base".to_string()),
        ],
    }
}

/// Краткий конструктор пер-темной четвёрки якорей.
fn anchors(light: &str, dark: &str, light_ic: &str, dark_ic: &str) -> ThemeAnchors {
    ThemeAnchors {
        light: light.to_string(),
        dark: dark.to_string(),
        light_ic: light_ic.to_string(),
        dark_ic: dark_ic.to_string(),
    }
}

/// Краткий конструктор семейства палитры для фикстуры (пер-темно).
fn fam(key: &str, light: &str, dark: &str, light_ic: &str, dark_ic: &str) -> PaletteFamily {
    PaletteFamily {
        key: key.to_string(),
        anchors: anchors(light, dark, light_ic, dark_ic),
    }
}

/// Краткий конструктор сентимент-категории для фикстуры.
fn sentiment(
    name: &str,
    family: &str,
    hue_floor_deg: Option<f64>,
    preferred_side: Option<i8>,
) -> SentimentCategory {
    SentimentCategory {
        name: name.to_string(),
        family: family.to_string(),
        hue_floor_deg,
        preferred_side,
    }
}

#[cfg(test)]
mod tests;
