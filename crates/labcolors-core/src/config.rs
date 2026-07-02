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
//! # Что этот модуль делает (CH-02 t1)
//!
//! - Несёт типы конфига без сериализации ([`ThemeConfig`] и вложенные) — JSON-парсинг
//!   это отдельная задача границы WASM (t3), не ядро.
//! - [`ThemeConfig::validate`] проверяет пределы КАЖДОЙ экспонируемой ручки: значение
//!   вне предела возвращает [`ConfigError`], а не тихо принимается. Клиент не может
//!   молча сломать различимость или WCAG-полы.
//! - [`ThemeConfig::compile_named_role_table`] компилирует роли в [`NamedRoleTable`],
//!   которую [`crate::semantic::resolve_named_set`] резолвит той же физикой, что и
//!   встроенную [`crate::RoleTable`].
//!
//! # Честные заглушки (границы CH-02)
//!
//! Рецепты [`RoleRecipe::Ladder`] (акцентная/сентимент/нейтраль-лестница) и
//! [`RoleRecipe::AlphaAnalog`] (альфа-аналог через композит-инверсию) объявлены в
//! меню рецептов с ПРАВИЛЬНЫМ типом, но их компиляция возвращает
//! [`ConfigError::NotYetImplemented`] — реализация в t2 (ladder поглощает акцентный
//! GAP #59; alpha_analog опирается на [`crate::alpha`]). Это честная заглушка с
//! верным типом, а не выдумка значений.

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

/// Нижний предел `hue_floor` сентимент-политики (градусы): `[0, 360)`.
///
/// `hue_floor_deg` — минимальный угол оттенка категории (напр. Warning ≥ 45°).
/// Оттенок — величина по модулю 360°; значение вне `[0, 360)` не является
/// каноническим углом.
const HUE_FLOOR_MIN_INCLUSIVE: f64 = 0.0;
/// Верхний предел `hue_floor` (исключительно; 360° ≡ 0°).
const HUE_FLOOR_MAX_EXCLUSIVE: f64 = 360.0;

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
    /// Значение ручки вне допустимого предела. `handle` — путь до ручки, `bound` —
    /// человеко-читаемое описание нарушенного предела с обоснованием.
    OutOfBounds {
        handle: String,
        value: f64,
        bound: &'static str,
    },
    /// Рецепт объявлен в меню, но его компиляция ещё не реализована в этой главе
    /// (`Ladder` / `AlphaAnalog` — задача t2). Честная заглушка с верным типом.
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
            ConfigError::NotYetImplemented { recipe, role } => write!(
                f,
                "рецепт `{recipe}` (роль `{role}`) ещё не реализован — задача t2"
            ),
        }
    }
}

impl std::error::Error for ConfigError {}

// ─────────────────────────────────────────────────────────────────────────────
// Типы конфига (без serde — JSON-парсинг это t3).
// ─────────────────────────────────────────────────────────────────────────────

/// Бренд — вход, не роль: якорный hex; оттенок движок выводит физикой.
#[derive(Debug, Clone, PartialEq)]
pub struct Brand {
    /// Якорный цвет бренда в hex (`#RRGGBB`). Дефолта в ядре нет.
    pub anchor_hex: String,
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
}

/// Нейтраль: тройка якорей + ручки подтона.
#[derive(Debug, Clone, PartialEq)]
pub struct NeutralConfig {
    /// Тройка hex-якорей нейтральной шкалы.
    pub anchors: NeutralAnchors,
    /// Ручки подтона.
    pub tint: NeutralTint,
}

/// Именованное семейство палитры: ключ + якорный hex.
#[derive(Debug, Clone, PartialEq)]
pub struct PaletteFamily {
    /// Стабильный ключ семейства (`[a-z0-9-]+`), напр. `red`.
    pub key: String,
    /// Якорный цвет семейства в hex (`#RRGGBB`).
    pub anchor_hex: String,
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
/// Реализованные в t1 рецепты компилируются в [`RoleSpec`]; [`Ladder`](Self::Ladder)
/// и [`AlphaAnalog`](Self::AlphaAnalog) объявлены с верным типом, но их компиляция
/// возвращает [`ConfigError::NotYetImplemented`] — задача t2 (честная заглушка).
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
    /// Ступень рампы акцента/семейства/нейтрали. Меню t2 (акцентный GAP #59) —
    /// компиляция возвращает [`ConfigError::NotYetImplemented`].
    Ladder,
    /// Альфа-аналог через композит-инверсию ([`crate::alpha`]). Меню t2 —
    /// компиляция возвращает [`ConfigError::NotYetImplemented`].
    AlphaAnalog,
    /// Явный ноль: «нет цвета здесь» ([`RoleSpec::Zero`]).
    Zero,
}

/// Полный конфиг темы потребителя (без сериализации — t3).
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

/// Проверить, что hex парсится ядром (`#RGB` / `#RRGGBB`).
fn check_hex(field: &str, value: &str) -> Result<(), ConfigError> {
    crate::spaces::srgb::srgb_from_hex(value)
        .map(|_| ())
        .map_err(|_| ConfigError::InvalidHex {
            field: field.to_string(),
            value: value.to_string(),
        })
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

/// Проверить `value > min` (строго положительно).
fn check_gt(
    handle: &str,
    value: f64,
    min_excl: f64,
    bound: &'static str,
) -> Result<(), ConfigError> {
    if value > min_excl {
        Ok(())
    } else {
        Err(ConfigError::OutOfBounds {
            handle: handle.to_string(),
            value,
            bound,
        })
    }
}

/// Проверить `value ≥ min`.
fn check_ge(
    handle: &str,
    value: f64,
    min_incl: f64,
    bound: &'static str,
) -> Result<(), ConfigError> {
    if value >= min_incl {
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
    /// Провалидировать конфиг: hex, имена, ссылки на семейства и пределы каждой
    /// экспонируемой ручки. Первая найденная ошибка возвращается сразу — клиент
    /// чинит по одной. Успех означает: [`compile_named_role_table`](Self::compile_named_role_table)
    /// упадёт только на честной заглушке ([`ConfigError::NotYetImplemented`]),
    /// никогда на неверном hex/имени/пределе.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Бренд-hex.
        check_hex("brand.anchor_hex", &self.brand.anchor_hex)?;

        // Нейтраль: тройка hex.
        check_hex("neutral.anchors.light", &self.neutral.anchors.light)?;
        check_hex("neutral.anchors.mid", &self.neutral.anchors.mid)?;
        check_hex("neutral.anchors.dark", &self.neutral.anchors.dark)?;

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

        // Палитра: имена + hex каждого семейства.
        for fam in &self.palette {
            let field = format!("palette[{}].key", fam.key);
            check_name(&field, &fam.key)?;
            let hex_field = format!("palette[{}].anchor_hex", fam.key);
            check_hex(&hex_field, &fam.anchor_hex)?;
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
            if let Some(hue) = cat.hue_floor_deg {
                let field = format!("sentiments.{}.hue_floor_deg", cat.name);
                // Полуинтервал `[0, 360)`: угол по модулю 360°, где 360° ≡ 0°.
                if !(HUE_FLOOR_MIN_INCLUSIVE..HUE_FLOOR_MAX_EXCLUSIVE).contains(&hue) {
                    return Err(ConfigError::OutOfBounds {
                        handle: field,
                        value: hue,
                        bound: "0 ≤ hue_floor_deg < 360 (угол оттенка по модулю 360°)",
                    });
                }
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
                return Err(ConfigError::UnknownFamily {
                    referenced_by: format!("aliases.{alias}"),
                    family: target.clone(),
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
            // Заглушки t2: тип верный, значений нет — пределов тоже нет.
            RoleRecipe::Ladder | RoleRecipe::AlphaAnalog | RoleRecipe::Zero => Ok(()),
        }
    }

    /// Скомпилировать роли конфига в [`NamedRoleTable`], которую
    /// [`resolve_named_set`](crate::semantic::resolve_named_set) резолвит той же
    /// физикой, что и встроенную [`crate::RoleTable`].
    ///
    /// Валидирует конфиг ([`validate`](Self::validate)) перед компиляцией.
    /// [`Ladder`](RoleRecipe::Ladder) и [`AlphaAnalog`](RoleRecipe::AlphaAnalog)
    /// возвращают [`ConfigError::NotYetImplemented`] (задача t2).
    pub fn compile_named_role_table(&self) -> Result<NamedRoleTable, ConfigError> {
        self.validate()?;

        let mut entries: Vec<(String, RoleSpec)> = Vec::with_capacity(self.roles.len());
        for (name, recipe) in &self.roles {
            let spec = compile_recipe(name, recipe)?;
            entries.push((name.clone(), spec));
        }

        // Нейтраль-подтон: v2-кривая. Форма строго сверена с
        // `semantic::RoleChroma::Curve` дефолтной таблицы (`neutral_curve()`):
        // canonical_hue_deg = измеренный NEUTRAL_HUE_DEG (движок выводит оттенок из
        // нейтральной тройки; для t1 берём измеренную SSOT-величину — вывод из hex
        // это t2), target_mp / hue_stiffness — из конфиг-ручек. `ratio` в v2-кривую
        // не входит (это поле v1 flat-пути), но валидируется как экспонированная ручка.
        let chroma = RoleChroma::Curve {
            canonical_hue_deg: semantic::NEUTRAL_HUE_DEG,
            target_mp: self.neutral.tint.target_mp,
            hue_stiffness: self.neutral.tint.hue_stiffness,
        };

        Ok(NamedRoleTable::new(entries, chroma))
    }
}

/// Скомпилировать один рецепт в [`RoleSpec`]. Заглушки t2 возвращают
/// [`ConfigError::NotYetImplemented`] с верным именем рецепта.
fn compile_recipe(role: &str, recipe: &RoleRecipe) -> Result<RoleSpec, ConfigError> {
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
        RoleRecipe::Ladder => Err(ConfigError::NotYetImplemented {
            recipe: "ladder",
            role: role.to_string(),
        }),
        RoleRecipe::AlphaAnalog => Err(ConfigError::NotYetImplemented {
            recipe: "alpha_analog",
            role: role.to_string(),
        }),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Эталонная фикстура labui.
// ─────────────────────────────────────────────────────────────────────────────

/// Эталонный конфиг labui для CH-02 t1 — покрывает сегодняшние 20 эмитируемых ролей.
///
/// Имена ролей = `Role::key()` сегодняшнего ядра; рецепты сняты 1:1 из
/// [`RoleTable::default`](crate::RoleTable) (`semantic.rs`): текст-фракции с их
/// WCAG-полами, dJ'-якоря fill/border из тех же констант, Lc-тени, zero. Нейтраль —
/// тройка Даниила `#FFFFFF`/`#787880`/`#101012`; ручки подтона — из констант
/// `semantic.rs` (единый источник истины, фикстура не может тихо разойтись с ядром).
///
/// Байт-в-байт тест доказывает: [`resolve_named_set`](crate::semantic::resolve_named_set)
/// этой фикстуры эмитит идентично [`resolve_set`](crate::resolve_set) дефолтной
/// таблицы на всех 240 точках golden-грида. Акценты/сентименты/альфа/ladder — t2.
pub fn labui_reference() -> ThemeConfig {
    // Фракции и полы — 1:1 из RoleTable::default (semantic.rs), включая border-strong
    // = контракт label-primary. Рецепты собраны так, чтобы имя роли совпадало с
    // Role::key(), а RoleSpec был идентичен дефолтному.
    let text = |fraction, floor| RoleRecipe::TextAnchor { fraction, floor };
    let dj = |m: DjMagnitude| RoleRecipe::DjAnchor {
        light: m.light(),
        dark: m.dark(),
    };
    let lc = |magnitude| RoleRecipe::DecorativeLc { magnitude };

    let roles = vec![
        // Labels.
        ("label-primary".to_string(), text(0.968, Floor::AaText)),
        ("label-secondary".to_string(), text(0.627, Floor::AaText)),
        ("label-tertiary".to_string(), text(0.461, Floor::AaUi)),
        ("label-quaternary".to_string(), text(0.276, Floor::None)),
        // Icon.
        ("icon".to_string(), text(0.461, Floor::AaUi)),
        // Separator — Lc decorative.
        ("separator".to_string(), lc(8.0)),
        // Border ladder. Strong = label-primary контракт; base/soft — dJ'.
        ("border-strong".to_string(), text(0.968, Floor::AaText)),
        ("border-base".to_string(), dj(semantic::BORDER_BASE_DJ)),
        ("border-soft".to_string(), dj(semantic::BORDER_SOFT_DJ)),
        ("border-ghost".to_string(), RoleRecipe::Zero),
        // Fill ladder — dJ'.
        ("fill-primary".to_string(), dj(semantic::FILL_PRIMARY_DJ)),
        (
            "fill-secondary".to_string(),
            dj(semantic::FILL_SECONDARY_DJ),
        ),
        ("fill-tertiary".to_string(), dj(semantic::FILL_TERTIARY_DJ)),
        (
            "fill-quaternary".to_string(),
            dj(semantic::FILL_QUATERNARY_DJ),
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

    ThemeConfig {
        brand: Brand {
            // Дефолт бренда labui (accent.rs:54-56).
            anchor_hex: "#007AFF".to_string(),
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
            },
        },
        // Палитра labui — 10 замеренных семейств (Figma 2026-07-02, accent.rs:113-126).
        // В t1 не потребляется (акценты — t2), но несёт корректный конфиг-снимок.
        palette: vec![
            fam("red", "#FF3B30"),
            fam("orange", "#FF9500"),
            fam("yellow", "#FFCC00"),
            fam("green", "#34C759"),
            fam("mint", "#00C7BE"),
            fam("teal", "#30B0C7"),
            fam("cyan", "#32ADE6"),
            fam("blue", "#007AFF"),
            fam("indigo", "#5856D6"),
            fam("pink", "#FF2D55"),
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
        aliases: Vec::new(),
    }
}

/// Краткий конструктор семейства палитры для фикстуры.
fn fam(key: &str, anchor_hex: &str) -> PaletteFamily {
    PaletteFamily {
        key: key.to_string(),
        anchor_hex: anchor_hex.to_string(),
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
