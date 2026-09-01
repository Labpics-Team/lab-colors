// Solver curve uses deprecated LcsColor per F-01 design
#![allow(deprecated)]

//! Контекстный резолвер скомпилированных клиентских цветовых контрактов.
//!
//! # Текущий контракт
//!
//! [`NamedRoleTable`] — переходное скомпилированное представление набора.
//! Клиентские идентификаторы непрозрачны: Core не выводит из строк семантику,
//! иерархию или физику. [`resolve_named_set`] решает весь набор относительно
//! текущего фона и viewing conditions; сериализация принадлежит биндингу.
//!
//! Граница набора атомарна. Доказанная недостижимость и незавершённый bounded
//! search остаются типизированными исходами отдельных ролей; rejected и
//! internal закрывают вызов через [`ResolveSetError`] без
//! частичного успешного вектора. Нулевое значение представлено явно через
//! [`Resolved::None`].
//!
//! Клиентский источник задаётся неизменяемыми sRGB8-байтами. Равные каналы
//! точно ахроматичны; неравные доказывают лишь хроматичность представления.
//! Финальные байты сами по себе не доказывают сохранение или различимость
//! authored hue — такой вывод требует явного relation-constraint и сертификата.
//!
//! # Целевая граница
//!
//! Публичная модель сходится к общему графу `Color`, `Paint`, `Surface`,
//! `Occurrence` и `Constraint`: значение или представление наблюдается в
//! конкретном контексте и удовлетворяет объявленным физическим ограничениям.
//! Клиент владеет именами и смыслом; Core — композитингом, контрастом,
//! адаптацией, конечной эмиссией и сертификатами.
//!
//! [`RoleSpec`], [`RoleChroma`] и остальные recipe-варианты — переходный
//! compiled transport существующего пути, а не публичная точка расширения.
//! Новые возможности должны выражаться общими узлами, рёбрами и constraints,
//! без добавления семантических recipe-кейсов в Core.

use crate::Srgb8;
use crate::appearance::PointOpacityOverSurfaceV1;
use crate::ladder::LadderTint;
use crate::output_bindings::{OutputBindingCompileError, OutputBindingSet, OutputBindingShape};
use crate::scale;
use crate::solve::{
    self, BgInput, ChromaPolicy, Contract, Hue, SolveFailure, SolveFailureBoundary,
    SolveFailureCategory, Solved,
};
use crate::spaces::oklab::{HUE_DEG_MAX_EXCLUSIVE, HUE_DEG_MIN_INCLUSIVE};
use crate::spaces::srgb::srgb_gamma;
use crate::spaces::vc::ViewingConditions;
use crate::wcag22::Wcag22CriterionV1;

/// Frozen pre-cutover projection of an explicit readability criterion.
///
/// W5 removes this value from the numerical solver. It survives only on the
/// recipe facade and lowers one-way to the canonical WCAG 2.2 criterion used by
/// the generic Program evaluator; `None` means that no such criterion is
/// authored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Floor {
    /// WCAG 2.2 success criterion 1.4.3 for default-size text.
    AaText,
    /// WCAG 2.2 success criterion 1.4.11 for UI components or graphical objects.
    AaUi,
    /// No authored WCAG criterion.
    None,
}

impl Floor {
    /// One-way projection to the canonical WCAG 2.2 criterion used by the
    /// generic Program evaluator.
    pub(crate) const fn criterion(self) -> Option<Wcag22CriterionV1> {
        match self {
            Self::AaText => Some(Wcag22CriterionV1::Sc143TextDefault),
            Self::AaUi => Some(Wcag22CriterionV1::Sc1411UiComponentOrState),
            Self::None => None,
        }
    }

    /// Frozen report projection for pre-cutover DTOs; never solver authority.
    pub(crate) const fn min_ratio(self) -> Option<f64> {
        match self {
            Self::AaText => Some(4.5),
            Self::AaUi => Some(3.0),
            Self::None => None,
        }
    }
}

/// DERIVED — надёжная нижняя граница контрастной величины декоративной роли.
///
/// Раскладывается как [`MODEL_LC_FLOOR`](crate::lpc::MODEL_LC_FLOOR) (`7.3`) +
/// [`QUANT_GUARD`] (`0.2`). За клипом решатель эмитит |Lc| ≥ 7.3 — это алгебраически
/// следует из GROUNDED APCA `0.0.98G-4g` набора (issue #44); guard держит пол
/// строго выше 7.3, чтобы `Contract::range` у самого клипа не садился на порог
/// квантования и не возвращал [`SolveFailure::BelowContrastFloor`]. Каждый
/// декоративный порог держится строго выше этой границы до появления полной
/// JND-калибровки.
///
/// Значение оставлено ЛИТЕРАЛОМ `7.5` ради байт-идентичности выходов (сумма
/// `7.3 + 0.2` в f64 отличается от `7.5` на ~1e-15); идентичность
/// `7.5 == MODEL_LC_FLOOR + QUANT_GUARD` закреплена компайл-тайм-проверкой ниже и
/// тестом `decorative_floor_is_model_floor_plus_guard`.
// SSOT-TRACKED — DERIVED: MODEL_LC_FLOOR (7.3) + QUANT_GUARD (0.2), issue #44, см. docs/empirical-inventory.md.
pub(crate) const DECORATIVE_FLOOR_MIN: f64 = 7.5;

/// Квант-guard над модельным полом [`MODEL_LC_FLOOR`](crate::lpc::MODEL_LC_FLOOR):
/// зазор `DECORATIVE_FLOOR_MIN − MODEL_LC_FLOOR`, держащий декоративный пол строго
/// выше 7.3, чтобы решатель не садился на порог клипа и не возвращал нулевой
/// контраст (issue #44).
///
/// Терминал **(c) INTERVAL-INSENSITIVE**: `QUANT_GUARD` НЕ используется в
/// продакшене независимо — единственная величина, которую видит решатель, это
/// байт-идентичный литерал [`DECORATIVE_FLOOR_MIN`] (7.5); guard существует
/// только как провенанс-разложение этого литерала на DERIVED-член (7.3) и
/// задекларированный запас, пиннится компайл-тайм-проверкой и локом
/// `decorative_floor_is_model_floor_plus_guard`. Скан-тест
/// `lpc::no_pair_emits_contrast_below_model_floor` показывает: фактический
/// минимальный ненулевой |Lc| = 7.3005 — сидит практически НА модельном полу
/// 7.3, а не у номинальной цели 7.5, то есть точная магнитуда guard'а (пока он
/// положителен и цель остаётся достижимой в пределах `QUANT_BUDGET` = 1.0) не
/// меняет ни одного реального эмитируемого контраста. Ре-аудит
/// `science/reclassify-e-buckets` 2026-07-07 — реестр
/// docs/empirical-inventory.md.
// SSOT-TRACKED — квант-guard декоративного пола (issue #44), терминал (c) interval-insensitive, см. docs/empirical-inventory.md.
// SSOT-TRACKED — DERIVED component of DECORATIVE_FLOOR_MIN (issue #44)
// Rust 1.85 не считает использованием обращение только из const-assert и тестов;
// это разложение provenance, а не отдельная production-политика.
#[allow(
    dead_code,
    reason = "SSOT provenance decomposition for decorative floor; used only in const-asserts and tests"
)]
// SSOT-TRACKED — DERIVED component of DECORATIVE_FLOOR_MIN (issue #44)
const QUANT_GUARD: f64 = 0.2;

// Компайл-тайм пиннинг деривации: DECORATIVE_FLOOR_MIN == MODEL_LC_FLOOR +
// QUANT_GUARD в пределах f64-шума суммирования. Это compile-time-фиксация
// provenance; сам поставляемый литерал 7.5 при этом НЕ меняется
// (байт-идентичность).
const _: () = {
    let derived = crate::lpc::MODEL_LC_FLOOR + QUANT_GUARD;
    let d = DECORATIVE_FLOOR_MIN - derived;
    let ad = if d < 0.0 { -d } else { d };
    assert!(
        ad < 1e-9,
        "DECORATIVE_FLOOR_MIN must equal MODEL_LC_FLOOR + QUANT_GUARD"
    );
};

/// GROUNDED — порог декоративного контраста для тем повышенной контрастности (`-ic`).
///
/// Опубликованный APCA-уровень `Lc 15` — «absolute minimum for any non-text that
/// needs to be discernible», ниже которого элемент считается невидимым (APCA
/// project docs, Somers/Myndex; **DRAFT/beta, single-origin — НЕ норматив WCAG 3**).
/// Декоративный `-ic`-пол — ровно этот случай: минимальный различимый
/// не-текстовый контраст. Применяется порядкосохраняющим сдвигом `+7.5`
/// (= 15 − 7.5), НЕ как `max` — см. [`RoleTable::decorative_contract`].
// GROUNDED — APCA `Lc 15` non-text discernibility level, draft (docs/empirical-inventory.md).
const IC_DECORATIVE_FLOOR_MIN: f64 = 15.0;

// ── dJ'-якоря декоративных ролей (буквальные значения из Figma) ─────────────────
//
// Лестницы fill и border несут буквальные dJ'-якоря — перцептивную разницу
// светлоты, которую каждый декоративный шаг держит относительно своей
// поверхности, вычисленные из растяжек LabUI Figma ("Вычисленные якоря",
// reference/labui-figma-structure.md). Это смещения координаты J', а не Lc;
// они не являются измеренным JND или моделью разборчивости.
//
// Числа — авторская per-theme дизайн-калибровка, вычисленная из reference-
// эмиссий под объявленными VC. Dark-значения не выводятся из light множителем:
// обе стороны выбираются непосредственно по VC.

/// dJ'-якоря лестницы fill (`fill-primary` … `fill-quaternary`), строго убывающие
/// по видимости. Буквальные измеренные значения; отдельно light/dark по теме.
#[cfg(test)]
// SSOT-TRACKED — dJ'-якоря из Figma-структуры LabUI (light, dark).
pub(crate) const FILL_PRIMARY_DJ: DjMagnitude = DjMagnitude::new(7.93, 17.67);
#[cfg(test)]
// SSOT-TRACKED — dJ'-якоря из Figma-структуры LabUI (light, dark).
pub(crate) const FILL_SECONDARY_DJ: DjMagnitude = DjMagnitude::new(6.41, 15.78);
#[cfg(test)]
// SSOT-TRACKED — dJ'-якоря из Figma-структуры LabUI (light, dark).
pub(crate) const FILL_TERTIARY_DJ: DjMagnitude = DjMagnitude::new(4.63, 12.01);
#[cfg(test)]
// SSOT-TRACKED — dJ'-якоря из Figma-структуры LabUI (light, dark).
pub(crate) const FILL_QUATERNARY_DJ: DjMagnitude = DjMagnitude::new(3.15, 8.22);

/// dJ'-якоря border base/soft. Буквальные измеренные значения; base сильнее soft.
/// (`border-strong` — заякоренная роль различимости (доля label-primary, пол
/// non-text 3:1), не dJ'-шаг — её здесь нет.)
#[cfg(test)]
// SSOT-TRACKED — dJ'-якоря из Figma-структуры LabUI (light, dark).
pub(crate) const BORDER_BASE_DJ: DjMagnitude = DjMagnitude::new(6.41, 10.12);
#[cfg(test)]
// SSOT-TRACKED — dJ'-якоря из Figma-структуры LabUI (light, dark).
pub(crate) const BORDER_SOFT_DJ: DjMagnitude = DjMagnitude::new(3.15, 5.83);

// ── Величины теней (Lc decorative — см. пояснение) ─────────────────────────────
//
// Стек теней НЕ перенесён на dJ'. Якоря теней — это значения *альфа*
// (прозрачности @1/@2/@4/@12 прогрессивного стека теней), не dJ'-шаги — иная
// величина (воспринимаемость полупрозрачного градиента поверх переменного
// контента, мост к составным фонам). dJ'-числа для них были бы значением из
// другой физической величины, поэтому тени держат единицу Lc
// [`RoleSpec::Decorative`]: величины выше [`DECORATIVE_FLOOR_MIN`], строго по
// возрастанию — единственный контракт стека.

/// Стек теней, строго ВОЗРАСТАЮЩИЙ по видимости (minor самая тонкая → major
/// самая сильная) — прогрессивная рампа FX/Shadow. Единица Lc, держится выше
/// [`DECORATIVE_FLOOR_MIN`] с шагом между уровнями ≥1.5 Lc.
///
/// Суффикс `_LC`, а НЕ `_JND`: это лестница Lc-величин, не JND-замер — прямого
/// измерения различимости (JND) для этих ступеней НЕТ, прежнее имя `_JND` было
/// терминологическим подлогом. Значения провизорны; единственный контракт —
/// строгий возрастающий порядок ступеней (тест
/// `shadow_constant_stack_is_strictly_ascending_with_gaps`). Финальная
/// Перцептивное повышение статуса требует опубликованных данных либо явной
/// клиентской policy; собственная owner-сессия не является evidence.
#[cfg(test)]
// SSOT-TRACKED — величина Lc стека теней (минимальная ступень).
pub(crate) const SHADOW_MINOR_LC: f64 = 8.0;
#[cfg(test)]
// SSOT-TRACKED — величина Lc стека теней.
pub(crate) const SHADOW_AMBIENT_LC: f64 = 9.5;
#[cfg(test)]
// SSOT-TRACKED — величина Lc стека теней.
pub(crate) const SHADOW_PENUMBRA_LC: f64 = 11.5;
#[cfg(test)]
// SSOT-TRACKED — величина Lc стека теней (максимальная ступень).
pub(crate) const SHADOW_MAJOR_LC: f64 = 14.0;

// ── Доли текстовой иерархии (Labels) ────────────────────────────────────────────
//
// Каждая доля = Ys-якорь Lc роли на белом ÷ максимально достижимый Ys-Lc
// 106.0407 (чёрный на белом). Якоря перенесены из генезис-домена Y_hk
// (Figma-замеры 102.6/66.5/48.9/29.3) при миграции мерила читаемости на Ys:
// инвариант переноса — ЦВЕТ, не Lc-число. Primary/secondary/quaternary — это
// Ys-замер принятых владельцем hex'ов лестницы (#141414/#767676/#C2C2C2), что
// гарантирует байт-идентичность эмиссии; tertiary эмиссией защищён полом 3:1
// (#949494), его якорь восстановлен побайтовой инверсией генезис-числа 48.9 →
// #9C9C9C → Ys 50.446. Вывод задокументирован в rustdoc `Default for
// RoleTable` ниже. Это «якорный принцип»: роль держит почти максимум, что
// позволяет фон, а не фиксированную дельту; финальная перцептивная
// Их изменение требует опубликованного evidence либо явной client policy.

/// Доля максимального Lc для `LabelPrimary` (и `BorderStrong`):
/// Ys(#141414)/Ys(max) = 103.2157/106.0407.
#[cfg(test)]
// SSOT-TRACKED — доля Ys-якоря Lc / max Ys-Lc 106.0407, см. docs/empirical-inventory.md.
const LABEL_PRIMARY_FRACTION: f64 = 0.97335917;
/// Доля максимального Lc для `LabelSecondary`: Ys(#767676)/Ys(max) = 68.2467/106.0407.
#[cfg(test)]
// SSOT-TRACKED — доля Ys-якоря Lc / max Ys-Lc 106.0407, см. docs/empirical-inventory.md.
const LABEL_SECONDARY_FRACTION: f64 = 0.64359014;
/// Доля максимального Lc для `LabelTertiary`:
/// Ys(#9C9C9C)/Ys(max) = 50.4459/106.0407 (инверсия генезис-якоря 48.9). Иконки —
/// глифы: красятся `label-tertiary`; отдельной роли `icon` в словаре нет.
#[cfg(test)]
// SSOT-TRACKED — доля Ys-якоря Lc / max Ys-Lc 106.0407, см. docs/empirical-inventory.md.
const LABEL_TERTIARY_FRACTION: f64 = 0.47572199;
/// Доля максимального Lc для `LabelQuaternary` (disabled):
/// Ys(#C2C2C2)/Ys(max) = 31.1081/106.0407.
#[cfg(test)]
// SSOT-TRACKED — доля Ys-якоря Lc / max Ys-Lc 106.0407, см. docs/empirical-inventory.md.
const LABEL_QUATERNARY_FRACTION: f64 = 0.29335999;

/// Lc-величина декоративного разделителя (`Separator`). Единственная оставшаяся
/// провизорная декоративная величина: держится выше [`DECORATIVE_FLOOR_MIN`]
/// (7.5). Перцептивная калибровка требует опубликованного evidence либо явной
/// client policy; литерал не является JND-утверждением.
#[cfg(test)]
// SSOT-TRACKED — провизорная декоративная величина Separator (Lc), см. docs/empirical-inventory.md.
const SEPARATOR_DECORATIVE_LC: f64 = 8.0;

/// The strict WCAG 2.1 AA *text* ratio (4.5:1) — the tightest legal gate any
/// role in the table imposes, and therefore the one polarity is chosen against.
/// Selecting against the strictest floor keeps a single polarity for the whole
/// set: a side that clears 4.5:1 trivially clears the laxer 3:1 UI floor too.
const POLARITY_FLOOR_RATIO: f64 = 4.5_f64;

/// The contrast polarity a background hosts: dark foreground on a light
/// background, or light foreground on a dark one.
///
/// Replaces the old bare `f64` sign (`+1.0` / `-1.0`): the two valid states are
/// named, illegal ones (a zero or non-unit sign) are unrepresentable, and the
/// `sign()` accessor is the single place the enum becomes the signed `Lc` the
/// solver consumes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Polarity {
    /// Dark foreground on a light background — positive signed `Lc`.
    DarkOnLight,
    /// Light foreground on a dark background — negative signed `Lc`.
    LightOnDark,
}

impl Polarity {
    /// The signed multiplier this polarity applies to a contrast magnitude:
    /// `+1` for dark-on-light, `-1` for light-on-dark.
    fn sign(self) -> f64 {
        match self {
            Polarity::DarkOnLight => 1.0,
            Polarity::LightOnDark => -1.0,
        }
    }
}

/// One semantic colour slot: a stable key plus the recipe for its contract.
///
/// The key is the public contract with downstream consumers (CSS custom
/// properties in the runtime-engine chapter); the variants are the v1 role set.
/// [`None`](Role::None) is a first-class member, not the absence of a role — see
/// the module docs on the zero token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[cfg(test)]
pub enum Role {
    /// Body / primary label text — anchored near the strongest contrast the
    /// background allows, so it reads black-on-light or white-on-dark. HIG
    /// "Labels/Primary" (`N12`-strength). Was `text-primary` before the HIG
    /// taxonomy rename; the contract (0.9734 of max, AA-text floor) is unchanged.
    LabelPrimary,
    /// Secondary label text — clearly subordinate to primary, still comfortably
    /// readable. HIG "Labels/Secondary". Was `text-secondary`.
    LabelSecondary,
    /// Tertiary label text — the weakest label still meant to be read. HIG
    /// "Labels/Tertiary". Was `text-muted`; same 0.4757 / AA-UI contract.
    LabelTertiary,
    /// Quaternary label text — deliberately low contrast; not for reading, so it
    /// carries no readability floor (WCAG excludes inactive controls). HIG
    /// "Labels/Quaternary". Was `text-disabled`.
    LabelQuaternary,
    /// Meaningful icons and graphical UI objects — non-text 3:1 floor.
    ///
    /// Hairline separator between content — a decorative JND contract. Kept as a
    /// first-class role (HIG carries it under separators / hairlines).
    Separator,
    /// Strong container outline — the strongest border. HIG "Border/Strong" =
    /// `N12`, the same strength as [`LabelPrimary`](Role::LabelPrimary), so it is
    /// an *anchored* role (not a JND placeholder): it carries the label-primary
    /// contract (0.9734 of max, AA-text floor), giving a crisp `N12`-weight edge.
    BorderStrong,
    /// Base container outline — the default border weight. HIG "Border/Base". A
    /// dJ' step at the owner's literal anchor (light 6.41 / dark 10.12).
    BorderBase,
    /// Soft container outline — the faintest visible border. HIG "Border/Soft". A
    /// dJ' step at the owner's literal anchor (light 3.15 / dark 5.83).
    BorderSoft,
    /// The explicit-zero border: "no edge here". HIG "Border/None" (`@0`).
    /// Resolves to [`Resolved::None`], the honest zero of the border ramp.
    BorderNone,
    /// Strongest fill tint over the background. HIG "Fills/Primary". A dJ' step at
    /// the owner's literal anchor (light 7.93 / dark 17.67); top of the
    /// strictly-descending fill ladder.
    FillPrimary,
    /// Secondary fill tint. HIG "Fills/Secondary". dJ' anchor (light 6.41 / dark 15.78).
    FillSecondary,
    /// Tertiary fill tint. HIG "Fills/Tertiary". dJ' anchor (light 4.63 / dark 12.01).
    FillTertiary,
    /// Quaternary fill tint — the faintest visible fill. HIG "Fills/Quaternary".
    /// dJ' anchor (light 3.15 / dark 8.22).
    FillQuaternary,
    /// The explicit-zero fill: "no fill here". HIG "Fills/None" (`@0`). Resolves
    /// to [`Resolved::None`], the honest zero of the fill ladder — the mirror of
    /// `Role::None` for the fills family.
    FillNone,
    /// The subtlest shadow step. HIG "FX/Shadow/Minor". A Lc magnitude; bottom
    /// of the progressive shadow stack (minor < ambient < penumbra < major in
    /// visibility).
    ShadowMinor,
    /// Ambient shadow step. HIG "FX/Shadow/Ambient". Lc magnitude.
    ShadowAmbient,
    /// Penumbra shadow step. HIG "FX/Shadow/Penumbra". Lc magnitude.
    ShadowPenumbra,
    /// The strongest shadow step. HIG "FX/Shadow/Major". Lc magnitude; top of
    /// the progressive shadow stack.
    ShadowMajor,
    /// The explicit zero token: "no colour here". Resolves to
    /// [`Resolved::None`], an honest zero, never a skipped key.
    None,
}

#[cfg(test)]
impl Role {
    /// Every role, grouped by family and ordered within each family by visual
    /// weight (strongest first, except the progressive shadow stack which runs
    /// subtlest→strongest), so a resolved set iterates deterministically and the
    /// ladder ordering invariants read off the sequence directly.
    pub const ALL: [Role; 19] = [
        // Labels — strongest text first.
        Role::LabelPrimary,
        Role::LabelSecondary,
        Role::LabelTertiary,
        Role::LabelQuaternary,
        // Separator.
        Role::Separator,
        // Border ladder — strong → soft, then the explicit zero.
        Role::BorderStrong,
        Role::BorderBase,
        Role::BorderSoft,
        Role::BorderNone,
        // Fill ladder — primary (most visible) → quaternary, then the zero.
        Role::FillPrimary,
        Role::FillSecondary,
        Role::FillTertiary,
        Role::FillQuaternary,
        Role::FillNone,
        // Shadow stack — minor (subtlest) → major (strongest), progressive.
        Role::ShadowMinor,
        Role::ShadowAmbient,
        Role::ShadowPenumbra,
        Role::ShadowMajor,
        // The universal zero token.
        Role::None,
    ];

    /// The stable string key for this role — the contract with CSS custom
    /// properties downstream (kebab-case, matching the owner's HIG token names,
    /// e.g. `--lab-label-primary`). These never change without a versioned
    /// migration.
    pub fn key(self) -> &'static str {
        match self {
            Role::LabelPrimary => "label-primary",
            Role::LabelSecondary => "label-secondary",
            Role::LabelTertiary => "label-tertiary",
            Role::LabelQuaternary => "label-quaternary",
            Role::Separator => "separator",
            Role::BorderStrong => "border-strong",
            Role::BorderBase => "border-base",
            Role::BorderSoft => "border-soft",
            Role::BorderNone => "border-none",
            Role::FillPrimary => "fill-primary",
            Role::FillSecondary => "fill-secondary",
            Role::FillTertiary => "fill-tertiary",
            Role::FillQuaternary => "fill-quaternary",
            Role::FillNone => "fill-none",
            Role::ShadowMinor => "shadow-minor",
            Role::ShadowAmbient => "shadow-ambient",
            Role::ShadowPenumbra => "shadow-penumbra",
            Role::ShadowMajor => "shadow-major",
            Role::None => "none",
        }
    }
}

/// Переходное представление цели text/UI-роли по Ys candidate-score.
///
/// Доля максимальной величины candidate-score данного фона, а не фиксированная
/// дельта `Lc`. `fraction` лежит в `(0, 1]`; `1.0` означает конец замороженной
/// SAPC-shaped кривой. Координата не является LPC/readability evidence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextAnchor {
    fraction: f64,
    conformance: Floor,
    /// Опциональный источник физической цветовой идентичности. `None` берёт
    /// [`RoleChroma`] таблицы. `Some(tint)` держит тот же candidate-score/floor-контракт, а
    /// точные эмитируемые sRGB8-байты выбирают план: равные каналы остаются
    /// нейтральными; неравные задают Oklab-направление и максимальную доступную
    /// хрому на решённой светлоте. Резолв — [`resolve_hued_anchor`].
    hue: Option<crate::ladder::LadderTint>,
}

impl TextAnchor {
    /// Якорь на доле `fraction` от максимального Ys candidate-score фона с
    /// заданным WCAG-полом. `fraction` обязан быть конечным и лежать в `(0, 1]`;
    /// невалидный ввод отклоняется без тихой коррекции. По умолчанию семейный
    /// оттенок отсутствует; его добавляет [`with_hue`](Self::with_hue).
    pub fn new(fraction: f64, conformance: Floor) -> Result<Self, SolveFailure> {
        if !fraction.is_finite() || fraction <= 0.0 || fraction > 1.0 {
            return Err(SolveFailure::InvalidInput(format!(
                "text anchor fraction must be finite and inside (0, 1], got {fraction}"
            )));
        }
        Ok(Self {
            fraction,
            conformance,
            hue: None,
        })
    }

    /// Тот же якорь с явным источником цветовой идентичности. Контракт уровня
    /// (`fraction`/`conformance`) не меняется. Равные sRGB8-каналы источника
    /// остаются нейтральными; неравные несут направление hue.
    pub fn with_hue(mut self, hue: crate::ladder::LadderTint) -> Self {
        self.hue = Some(hue);
        self
    }

    /// Доля максимальной величины Ys candidate-score в `(0, 1]`.
    pub fn fraction(self) -> f64 {
        self.fraction
    }

    /// WCAG-пол, применяемый после candidate-score цели.
    pub fn conformance(self) -> Floor {
        self.conformance
    }

    /// Явный источник физической цветовой идентичности; `None` берёт policy таблицы.
    pub fn hue(self) -> Option<crate::ladder::LadderTint> {
        self.hue
    }
}

/// Пара авторских per-theme смещений координаты CAM16-UCS `J'`.
///
/// Значения выведены из указанных Figma-эмиссий под объявленными viewing
/// conditions. Это дизайн-калибровка, а не измеренный JND или универсальный
/// закон компенсации окружения. Нужную сторону выбирает
/// [`for_vc`](DjMagnitude::for_vc).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DjMagnitude {
    light: f64,
    dark: f64,
}

impl DjMagnitude {
    /// Авторские `J'`-смещения для светлого и тёмного профилей VC.
    pub const fn new(light: f64, dark: f64) -> Self {
        Self { light, dark }
    }

    /// Якорь для данных VC: dark-значение при dimmed surround, иначе light.
    pub fn for_vc(self, vc: &ViewingConditions) -> f64 {
        if vc.is_dark_theme() {
            self.dark
        } else {
            self.light
        }
    }

    /// Якорь светлого окружения.
    pub fn light(self) -> f64 {
        self.light
    }

    /// Якорь тёмного окружения.
    pub fn dark(self) -> f64 {
        self.dark
    }
}

/// Переходный численный рецепт роли — форма, исполняемая этим модулем.
///
/// Text/UI-якоря задают долю максимума замороженной Ys-кривой; декоративные dJ'
/// роли — шаг координаты CAM16-UCS `J'` без WCAG-пола; legacy-декоративные Lc
/// роли — величину Ys candidate-score только для относительного порядка стека.
/// Нулевой токен разрешается в отсутствие значения. Эти варианты описывают
/// текущий transport и не являются точкой расширения Core.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RoleSpec {
    /// Ys candidate-score text/UI-якоря: доля максимума данного фона.
    Anchor(TextAnchor),
    /// Декоративное смещение координаты CAM16-UCS `J'`: решённый цвет находится
    /// на `magnitude_dj` от фона по оси `J'` в сторону большего доступного
    /// диапазона. Нет WCAG-пола и low-contrast clip; координатный контракт сам
    /// по себе не заявляет JND или читаемость глифов.
    ///
    /// Величина несёт авторские Figma-якоря по темам (см. [`DjMagnitude`]); solve
    /// аналитически переводит J'-смещение в Oklab L, строит подтон, квантует и
    /// повторно измеряет dJ' на эмитированном hex.
    DecorativeDj { magnitude_dj: DjMagnitude },
    /// Decorative Ys candidate-score magnitude `Lc`, held above
    /// `DECORATIVE_FLOOR_MIN`, with [`Floor::None`]. It is not a JND claim.
    ///
    /// Сохранено для стека теней: его исходные якоря — alpha opacity, не dJ'.
    /// Перевод в dJ' выдумал бы числа без источника; этот вариант несёт только
    /// относительный порядок ступеней.
    Decorative { magnitude: f64 },
    /// Ступень лестницы семейства/бренда/нейтрали: тинт-якорь источника
    /// (по теме) при альфе позиции. Эмитит пару (тинт, α) НАПРЯМУЮ (закон
    /// лестницы labui — композитит браузер, а не солид-эквивалент, см.
    /// [`crate::ladder`]). Резолв — [`Resolved::Translucent`]: несёт тинт (то, что
    /// красит `--lab-*`), альфу и солид-композит `α·tint + (1−α)·bg` на фоне
    /// резолва для честного замера контраста (фаза 1 AA меряет композит).
    ///
    /// bg-независимость тинта (это якорь источника) — почему он `Copy`-payload
    /// [`LadderTint`], разложенный по темам на этапе компиляции. Альфа — ПЕР-ТЕМНАЯ
    /// пара `(light, dark)` данных позиции меню
    /// ([`LadderPosition::alpha_pair`](crate::ladder::LadderPosition::alpha_pair)):
    /// у акцентов пара равна, но скелетон-база пер-темна (стаб light @8 / dark @12),
    /// поэтому альфа выбирается по теме резолва, как и тинт.
    /// Свечение — добавление света: screen-слой цвета
    /// источника, интенсивность решается солвером под контрактную ступень
    /// [`crate::glow::GlowStep`] на фактическом фоне резолва. Эмиссия — пара
    /// слоёв (core = пересвет, halo = источник) + α; оператор потребителя —
    /// `mix-blend-mode: screen` (контрактный, не темнит по построению).
    Glow {
        /// Пер-темный кодированный якорь источника (как у лестницы).
        tint: crate::ladder::LadderTint,
        /// Контрактная ступень стека.
        step: crate::glow::GlowStep,
        /// Typed execution mode реально выбирает зарегистрированный numerical
        /// path. Wire-ключ — его boundary-проекция, а не декоративный metadata.
        mode: crate::numerical_plan::NumericalExecutionModeV1,
    },
    Ladder {
        /// Пер-темный кодированный тинт (якорь источника).
        tint: LadderTint,
        /// Альфа для светлой темы (`(0, 1]`; солид = 1.0).
        alpha_light: f64,
        /// Альфа для тёмной темы (`(0, 1]`; у акцентов = `alpha_light`).
        alpha_dark: f64,
        /// Опциональный юр. пол UI (M2 ch5c): солидная семейная граница
        /// (`border-<family>-strong`, α=1) ОБЯЗАНА держать 3:1 (WCAG 1.4.11).
        /// `None` — тинт эмитится как есть. `Some(floor)` допустим только для
        /// solid-позиции (`α=1`): если она уже проходит пол, сохраняются точные
        /// байты; иначе выполняется минимальный легальный сдвиг с флагом
        /// [`TranslucentResolved::floor_coerced`]. Для полупрозрачной позиции
        /// поле должно отсутствовать; `Some(Floor::None)` тоже невалиден.
        floor: Option<Floor>,
    },
    /// Альфа-аналог солида источника через композит-инверсию ([`crate::alpha`]):
    /// для солид-цвета `of` (по теме) на фоне резолва подбирается
    /// `(tint, α)`, чей композит равен солиду. Отличается от [`Ladder`](Self::Ladder)
    /// тем, что здесь солид-цель ФИКСИРОВАНА (тинт выводится инверсией), а не
    /// тинт-якорь эмитится напрямую. Даёт `-tinted`-роли labui (fill-*-tinted):
    /// заливка, чей композит на подложке = соответствующий солид.
    ///
    /// Фактическая α возвращается явно ([`TranslucentResolved::alpha`]): при
    /// неразрешимой запрошенной α поднимается до `α_min` (композит остаётся точно
    /// равным солиду — двигается прозрачность, не цвет; кламп тинта запрещён).
    AlphaAnalog {
        /// Пер-темный кодированный солид-источник, чей альфа-аналог берётся.
        of: LadderTint,
        /// Запрошенная альфа: `(0, 1]` — контракт РОЛИ (тот же предел, что у
        /// конфиг-валидатора: α = 0 — невидимая роль, отказ честнее выдумки).
        /// Generic point-закон допускает `0.0` как физическую opacity, но этот
        /// временный recipe-frontend исключает невидимую роль. Поднимается до
        /// первого exact sRGB8-представления, если запрос ниже разрешимого.
        alpha: f64,
    },
    /// Двухслойный материал (стекло/акрил): опаковая тон-база `02` на целевом
    /// |ΔJ'| тира + полупрозрачный тинт `01` (тот же тон) с ВЫВЕДЕННОЙ альфой.
    /// Резолв — [`Resolved::Material`] ([`crate::material`]; см. `docs/whitepaper.md`, «Точечные композиции»).
    ///
    /// Тон строится dj-anchor-солвером на светлоте `tone` от фона резолва в
    /// оттенке семьи; α выбирается охарактеризованным для платформы поиском
    /// с фиксированным числом шагов и повторно проверяется как проходящее
    /// состояние для `floor`.
    Material {
        /// Источник цветовой идентичности тона. `None` берёт neutral policy
        /// таблицы. Для `Some(tint)` равные sRGB8-каналы остаются нейтральными,
        /// неравные подставляют направление источника в chroma-policy таблицы.
        hue: Option<LadderTint>,
        /// Целевой |ΔJ'| тона-базы от фона резолва (пер-темная пара): тир
        /// материала (base = крупный/заметный, subtle = малый/тонкий).
        tone: DjMagnitude,
        /// WCAG-пол читаемости, который держит выведенная α (`AaText`/`AaUi`).
        /// `Floor::None` невалиден — материал обязан нести пол (валидатор ловит).
        floor: Floor,
    },
    /// Нулевой токен: разрешается в [`Resolved::None`].
    Zero,
}

/// Oklab-оттенок (в градусах), в который тонирован системный нейтральный цвет.
///
/// Нейтральная шкала не чисто серая: несёт холодный сине-фиолетовый подтон.
/// Измеренный в Oklab по опорным точкам оттенок стабилен по всей шкале —
/// `#101012` → 285.97°, `#3C3C43` (вторичный Figma) → 285.78°, `#787880`
/// (средний) → 286.01° — поэтому его захватывает одна константа. Резолвленные
/// роли наследуют этот оттенок, из-за чего `label-primary` на белом ложится как
/// родственник `#101012` (холодный почти-чёрный), а не стерильно-серый
/// `#141414`.
///
/// Терминал **(e) DESIGN-CHOICE** — ИЗМЕРЕННЫЙ якорь дизайна (оттенок семейства
/// нейтралей labui), не первопринципная величина: канонического «правильного»
/// оттенка нейтрали не существует, это выбор холодного почти-чёрного семейства.
/// Легальный диапазон = ИЗМЕРЕННЫЙ разброс шкалы **[285.78°, 286.01°]** (0.23°).
/// Sensitivity (Волна 2, лок `neutral_hue_emits_byte_invariant_across_measured_family_spread`):
/// эмитируемый тинт-цвет **байт-инвариантен (ΔE_ok = 0)** по всему измеренному
/// разбросу — при малой хроме тинта (ratio 0.10) сдвиг 0.23° ниже кванта
/// 8-бит сетки; даже грубая ошибка ±20° даёт лишь ΔE_ok ≈ 0.0114 (≈1 JND в
/// Oklab). Оттенок ВСЁ ЖЕ материален (питает `cusp_attracted_hue` как канонический
/// и в режиме пиннинга переносится в выход ~1:1), поэтому не (c), а честный (e):
/// измеренный якорь с доказанной локальной робастностью. Протокол калибровки:
/// пере-замерить Oklab-оттенок собственного семейства нейтралей потребителя
/// (`atan2(b,a)` средней ступени) — измерение, а не эксперимент с наблюдателями.
// Test-only characterization anchor; not a production policy constant.
#[cfg(test)]
pub(crate) const NEUTRAL_HUE_DEG: f64 = 286.0;

/// Целевая перцептивная красочность (CAM16-UCS `M'`) по умолчанию, которую
/// v2-кривая подтона держит по всей шкале светлоты — параметр "сила".
///
/// Это ядро механизма 1 (*постоянная перцептивная хрома*). Референсная рампа,
/// измеренная в CAM16-UCS, держит **не** постоянную долю от максимума гамута,
/// а примерно постоянную *красочность* `M'` по всему телу шкалы, спадающую
/// только на самых краях, где гамут сужается:
///
/// | референс hex | L_ok | M' |
/// |---------|------|----|
/// | #303136 | 0.31 | 4.6 |
/// | #5B5C64 | 0.48 | 6.0 |
/// | #787881 | 0.58 | 6.2 |
/// | #9698A2 | 0.68 | 6.8 |
/// | #B3B5BF | 0.78 | 6.6 |
/// | #CDD0D9 | 0.86 | 6.2 |
///
/// `M'` лежит на плато ~6.3 от L≈0.48 до L≈0.86 — в диапазоне, где живёт
/// большинство ролей UI — и спадает только на самых краях (почти-чёрный /
/// почти-белый). Плоский `ratio · max_chroma` (v1) вместо этого следует
/// огибающей гамута: перенасыщает середину (M' роли secondary достигает 10.3,
/// ~60% выше референса) и обедняет светлый край (M' primary-на-тёмном 1.8,
/// ~40% от референса). Постоянный `M'` воспроизводит огибающую референса
/// "держит хрому в светлых, умеренную в середине" именно потому, что UCS
/// перцептивно равномерна — равный `M'` считывается как равная красочность
/// независимо от светлоты.
///
/// `6.1` — единственный скаляр силы, применённый одинаково по всей шкале
/// (см. тест `curve_fits_reference_plateau_colorfulness` для количественного
/// сравнения с референсом).
// `#[cfg(test)]`: значение принадлежит только characterization fixture и не
// задаёт политику agnostic Core.
// SSOT-TRACKED — целевой M' в CAM16-UCS.
#[cfg(test)]
pub(crate) const TINT_TARGET_MP: f64 = 6.1;

/// Жёсткость притяжения оттенка к канонической точке по умолчанию для
/// v2-кривой. Чем выше значение, тем сильнее оттенок прижат к каноническому
/// [`NEUTRAL_HUE_DEG`]; чем ниже — тем свободнее он смещается к локальному
/// каспу хромы гамута sRGB.
///
/// Терминал **(c) INTERVAL-INSENSITIVE** (ре-классификация Волны 2
/// «объективизация», 2026-07-08; ранее (e) DESIGN-CHOICE). Замер
/// `cusp_attracted_hue` на светлотной сетке [0.05, 0.95]: локальный касп около
/// 286° даёт лишь небольшой выигрыш хромы относительно канонического оттенка,
/// поэтому выше **измеренного порога пиннинга ≈0.36 (шаг 0.01)** штраф `stiffness/100 · drift`
/// подавляет весь выигрыш, и argmax встаёт РОВНО на канонический оттенок
/// (drift = 0). Отклонение эмитируемого оттенка от канонического по всей полосе
/// жёсткости `[1.0, 100.0]` — **0.000000°** (байт-инвариант выхода), тогда как
/// при `stiffness → 0` оттенок уходит до края окна ±40° (полностью материален
/// НИЖЕ режима). `9.0` сидит в **25×** порога пиннинга — значение доказуемо
/// нематериально для выхода в своём режиме: это ИЗМЕРЕННОЕ утверждение об
/// инвариантности выхода, сильнее «выбора дизайна». Лок
/// `stiffness_pins_hue_to_canonical_above_threshold` (порог + инвариантность +
/// маржа), реестр docs/empirical-inventory.md.
///
/// Легальный диапазон конфига: `hue_stiffness ≥ 0` (валидатор `config.rs`);
/// режим пиннинга — `[≈0.36, ∞)`. Ниже ≈0.36 оттенок клиента материально
/// смещается к каспу — там `hue_stiffness` становится настоящей (e)-ручкой
/// потребителя, но ДЕФОЛТ 9.0 к этому режиму не относится. Протокол «выхода из
/// (c)»: если потребитель осознанно ставит стиффнес < 0.36 (хочет касп-дрейф),
/// он ре-открывает материальность и обязан объявить своё значение (e)-ручкой.
// Test-only characterization scalar; not a production policy constant.
#[cfg(test)]
pub(crate) const TINT_HUE_STIFFNESS: f64 = 9.0;

/// Полуширина окна оттенка в градусах для cusp-поиска вокруг канонического hue.
/// Подтон может смещаться внутри ограниченного диапазона, но не уходить в
/// несвязанные квадранты.
///
/// Терминал **(e) DESIGN-CHOICE** (НЕ (c), несмотря на внешнее сходство с
/// [`crate::scale::HUE_SEARCH_HALF_WINDOW`]): замер
/// `cusp_window_is_near_measured_gamut_drift` показывает, что полный
/// геометрический дрейф каспа гамута (≈42.5°) СТРОГО ПРЕВЫШАЕТ окно (40°) —
/// окно намеренно клипует чуть ВНУТРИ полного дрейфа (движок держит оттенок
/// у канонического, не гонится за магента-каспом). Это доказанно СВЯЗЫВАЮЩИЙ
/// кап (в отличие от `HUE_SEARCH_HALF_WINDOW`, где интерьерный оптимум НИКОГДА
/// не касается ребра окна ни на одном из 43 хроматических якорей) — точное
/// значение окна МЕНЯЕТ, на сколько градусов оттенку позволено сместиться на
/// экстремумах светлоты, поэтому доказательства интервал-нечувствительности
/// нет: честный терминал — задекларированный дизайн-кап, не (c).
///
/// Легальный диапазон (Волна 2): `(0°, ~42.5°]` — измеренный полный дрейф
/// каспа. Значение обязано быть ≤ полного дрейфа (иначе кап перестаёт быть
/// связывающим и вырождается в `HUE_SEARCH`-случай) и > 0. Sensitivity: у
/// края этого интервала точное значение решает величину допустимого сдвига
/// оттенка на крайних светлотах (лок `cusp_window_is_near_measured_gamut_drift`
/// пинит `дрейф > окно`). Протокол «объективизации»: замерить перцептивный
/// порог приемлемого дрейфа undertone (2AFC на рампе near-white ролей) — тогда
/// окно = min(этот порог, полный дрейф); измерение стало бы кандидатом-выводом
/// (замер → сравнение → решение), а не обязательным экспериментом.
// SSOT-TRACKED — hue search half-window (degrees), терминал (e) design-choice (намеренный кап (0,~42.5°], не interval-insensitive), см. docs/empirical-inventory.md.
const CUSP_HALF_WINDOW_DEG: f64 = 40.0;

/// Chroma-policy, которую несёт таблица ролей.
///
/// [`Tinted`](RoleChroma::Tinted) — общий fixed-ratio примитив с явными
/// клиентскими hue/ratio; [`Curve`](RoleChroma::Curve) — параметризованное
/// построение подтона; [`Neutral`](RoleChroma::Neutral) — ахроматическая policy.
/// [`NamedRoleTable`] хранит policy без вывода из client ID.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RoleChroma {
    /// Ахроматический режим: нулевая chroma, hue не влияет.
    Neutral,
    /// Цвет в Oklab-направлении `hue_deg` при `ratio` от максимальной in-gamut
    /// chroma на каждой решённой светлоте. Оба значения — явные входы policy;
    /// Core не приписывает им клиентский смысл.
    Tinted { hue_deg: f64, ratio: f64 },
    /// Параметризованное построение цветности на решённой Oklab-светлоте.
    ///
    /// 1. На каждой кандидатной светлоте решатель ищет gamut-valid цвет с
    ///    заданной координатой CAM16-UCS `target_mp`. Это численная цель
    ///    конкретной модели, а не утверждение об универсальной перцептивной
    ///    равномерности.
    /// 2. Поиск оттенка стартует от переданного `canonical_hue_deg` и применяет
    ///    `hue_stiffness` при сравнении с локальной геометрией максимальной
    ///    sRGB-хромы. Оттенок принадлежит caller/compiled policy; Core не
    ///    приписывает ему роль, происхождение или фиксированное значение.
    /// 3. Если `target_mp` недостижим, построение ограничивается доступной
    ///    границей гамута. Любая последующая интерпретация результата остаётся
    ///    model-scoped.
    ///
    /// Все три поля — входы политики с проверяемым численным доменом.
    /// `CUSP_HALF_WINDOW_DEG` ограничивает внутренний поиск как versioned
    /// design choice с документированной чувствительностью; это не следствие
    /// одной лишь геометрии и не научный default для клиентов.
    Curve {
        canonical_hue_deg: f64,
        target_mp: f64,
        hue_stiffness: f64,
    },
}

impl RoleChroma {
    /// Проверяет численный домен политики до дорогостоящего построения кривой.
    /// `NamedRoleTable` можно собрать напрямую, в обход конфиг-валидатора, поэтому
    /// граница резолва обязана отвергать такой ввод сама, а не получать из NaN
    /// правдоподобный серый цвет через особенности сравнений `f64`.
    fn validate(self) -> Result<(), SolveFailure> {
        match self {
            RoleChroma::Neutral => Ok(()),
            RoleChroma::Tinted { hue_deg, ratio } => {
                if !hue_deg.is_finite()
                    || !(HUE_DEG_MIN_INCLUSIVE..HUE_DEG_MAX_EXCLUSIVE).contains(&hue_deg)
                {
                    return Err(SolveFailure::InvalidInput(format!(
                        "undertone hue must be finite and inside [0, 360), got {hue_deg}"
                    )));
                }
                if !ratio.is_finite() || !(0.0..=1.0).contains(&ratio) {
                    return Err(SolveFailure::InvalidInput(format!(
                        "undertone chroma ratio must be finite and inside [0, 1], got {ratio}"
                    )));
                }
                Ok(())
            }
            RoleChroma::Curve {
                canonical_hue_deg,
                target_mp,
                hue_stiffness,
            } => {
                if !canonical_hue_deg.is_finite()
                    || !(HUE_DEG_MIN_INCLUSIVE..HUE_DEG_MAX_EXCLUSIVE).contains(&canonical_hue_deg)
                {
                    return Err(SolveFailure::InvalidInput(format!(
                        "curve canonical hue must be finite and inside [0, 360), got {canonical_hue_deg}"
                    )));
                }
                if !target_mp.is_finite() || target_mp <= 0.0 {
                    return Err(SolveFailure::InvalidInput(format!(
                        "curve target M' must be finite and greater than zero, got {target_mp}"
                    )));
                }
                if !hue_stiffness.is_finite() || hue_stiffness < 0.0 {
                    return Err(SolveFailure::InvalidInput(format!(
                        "curve hue stiffness must be finite and non-negative, got {hue_stiffness}"
                    )));
                }
                Ok(())
            }
        }
    }

    /// Замороженная кривая test-only фикстуры: oracle миграционного паритета,
    /// не научный default для клиентских данных.
    #[cfg(test)]
    fn neutral_curve() -> Self {
        RoleChroma::Curve {
            canonical_hue_deg: NEUTRAL_HUE_DEG,
            target_mp: TINT_TARGET_MP,
            hue_stiffness: TINT_HUE_STIFFNESS,
        }
    }

    /// Строит `(hue, chroma)` для роли с уже решённой Oklab-светлотой `l_ok`.
    ///
    /// [`Neutral`](RoleChroma::Neutral) и [`Tinted`](RoleChroma::Tinted) не
    /// зависят от `l_ok`. Для [`Curve`](RoleChroma::Curve) hue притягивается к
    /// cusp при `l_ok`, а ratio решается к объявленной численной цели `target_mp`
    /// на этой светлоте; это model-scoped построение кривой, не общий закон
    /// восприятия.
    fn plan_for_lightness(self, l_ok: f64, vc: &ViewingConditions) -> (Hue, ChromaPolicy) {
        match self {
            RoleChroma::Neutral => (Hue::deg(0.0), ChromaPolicy::Neutral),
            RoleChroma::Tinted { hue_deg, ratio } => {
                (Hue::deg(hue_deg), ChromaPolicy::Relative(ratio))
            }
            RoleChroma::Curve {
                canonical_hue_deg,
                target_mp,
                hue_stiffness,
            } => {
                // Curve-план — чистая функция `(l_ok, policy scalars, vc)`:
                // 81-точечный hue-поиск и CAM16-бисекция ratio. Exact-key memo
                // возвращает те же биты при повторе светлоты внутри sweep, не
                // повторяя оба поиска; см. [`curve_plan_cached`].
                curve_plan_cached(l_ok, canonical_hue_deg, target_mp, hue_stiffness, vc)
            }
        }
    }

    /// Независимый от светлоты ахроматический probe-план: узнаёт contrast-solved
    /// светлоту роли до построения основного per-lightness плана.
    fn probe_plan() -> (Hue, ChromaPolicy) {
        (Hue::deg(0.0), ChromaPolicy::Neutral)
    }
}

thread_local! {
    /// Process-lived memo Curve-плана по битам
    /// `(l_ok, canonical, target_mp, stiffness, vc)`. Попадание возвращает тот же
    /// `(hue, ratio)`, что 81-точечный cusp-поиск и CAM16-бисекция. Размер ограничен
    /// [`CURVE_PLAN_CACHE_CAP`]; при достижении cap карта очищается, что вызывает
    /// только cold rebuild, но не меняет результат.
    static CURVE_PLAN_CACHE: std::cell::RefCell<
        std::collections::HashMap<[u64; 5], (f64, f64)>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Empty the per-thread curve-plan cache. Test-only: lets the forwards regression
/// guard pin the COLD path (cache empty) separately from the WARM path (cache
/// primed) deterministically, instead of depending on iteration order.
#[cfg(test)]
pub(crate) fn reset_curve_plan_cache() {
    CURVE_PLAN_CACHE.with(|c| c.borrow_mut().clear());
}

/// Upper bound on live curve-plan cache entries. One resolve sweep visits at most
/// a few dozen distinct lightnesses; this holds thousands of sweeps' worth of
/// distinct themes (~56 bytes/entry → well under 1 MB at the cap) before a
/// wholesale clear. The cap turns an otherwise unbounded thread-local into a
/// fixed-footprint cache — bounded memory is a correctness property here, not a
/// nicety (ZERO SURPRISES under sustained load).
const CURVE_PLAN_CACHE_CAP: usize = 16_384;

/// The [`RoleChroma::Curve`] `(hue, ratio)` plan at `l_ok`, memoised per sweep.
///
/// The plan is a deterministic function of its inputs, so the cache holds the
/// exact value the uncached scans produce — no tolerance, no hex drift. The key
/// includes the VC so the light and dim sweeps never alias.
fn curve_plan_cached(
    l_ok: f64,
    canonical_hue_deg: f64,
    target_mp: f64,
    hue_stiffness: f64,
    vc: &ViewingConditions,
) -> (Hue, ChromaPolicy) {
    let key = [
        l_ok.to_bits(),
        canonical_hue_deg.to_bits(),
        target_mp.to_bits(),
        hue_stiffness.to_bits(),
        vc.fingerprint(),
    ];
    if let Some((hue_deg, ratio)) = CURVE_PLAN_CACHE.with(|c| c.borrow().get(&key).copied()) {
        return (Hue::deg(hue_deg), ChromaPolicy::Relative(ratio));
    }
    let hue_deg = cusp_attracted_hue(l_ok, canonical_hue_deg, hue_stiffness);
    let ratio = ratio_for_target_mp(l_ok, hue_deg, target_mp, vc);
    CURVE_PLAN_CACHE.with(|c| {
        let mut m = c.borrow_mut();
        if m.len() >= CURVE_PLAN_CACHE_CAP {
            m.clear(); // bounded footprint: wholesale cold rebuild, never wrong
        }
        m.insert(key, (hue_deg, ratio));
    });
    (Hue::deg(hue_deg), ChromaPolicy::Relative(ratio))
}

/// The hue (degrees) the undertone takes at Oklab lightness `l_ok` — mechanism 2.
///
/// Cusp attraction: scan a bounded blue-violet window around `canonical_deg` and
/// pick the hue maximising achievable purity minus a stiffness penalty for
/// leaving the canonical hue:
///
/// ```text
/// score(h) = max_chroma(l_ok, h) − (stiffness / 100) · |h − canonical|
/// ```
///
/// The chroma term rewards hues where the sRGB gamut reaches further at this
/// lightness; the penalty, scaled by the "hue hold" stiffness, keeps the drift
/// anchored to the canonical 286°. The drift therefore *emerges from gamut
/// geometry* — no hue nodes are hard-coded.
///
/// HONEST LIMIT (measured 2026-06-12). The sRGB gamut's local chroma cusp near
/// 286° drifts toward **azure (~264°) at LOW lightness** and toward
/// **magenta (~326°) at HIGH lightness**. The owner's reference ramp does the
/// opposite — it holds ~286° in the dark and drifts to azure (~248–271°) in the
/// *lights*. So geometry-driven cusp attraction reproduces the dark-end hue but
/// **cannot** produce the reference's light-end azure drift; left unchecked it
/// would pull the lights toward magenta. The stiffness is therefore calibrated
/// high enough to keep the hue close to canonical (a faithful, undramatic
/// blue-violet across the ladder) rather than chase a magenta cusp the reference
/// never visits. The azure light-end drift is a property of the owner's hand
/// calibration, not of the sRGB gamut, and is flagged as out of reach here.
fn cusp_attracted_hue(l_ok: f64, canonical_deg: f64, stiffness: f64) -> f64 {
    let penalty_scale = stiffness / 100.0;
    // 1° window steps — finer than the cusp moves between roles.
    let steps = (CUSP_HALF_WINDOW_DEG * 2.0) as i32;

    // Bit-identical per-index score (the exact arithmetic of the flat sweep).
    let score_at = |i: i32| -> f64 {
        let h = canonical_deg - CUSP_HALF_WINDOW_DEG + i as f64;
        let chroma = scale::max_chroma(l_ok, h);
        let drift = (h - canonical_deg).abs();
        chroma - penalty_scale * drift
    };

    // C2 — coarse-to-fine hue sweep (shared with the accent ramp, see
    // `scale::coarse_to_fine_argmax`). 5° coarse grid, ±15° refinement bracket
    // around every coarse local maximum, then a single ascending pass with the
    // flat scan's strict-`>` first-maximum tie-break. Bit-identical to the flat
    // 81-point sweep — pinned on the full (l_ok × canonical) grid by the cusp
    // diff test and on real tints by the 240-cell resolve_set byte-identity
    // snapshot. (`let`, not `const`, keeps the frozen policy-const audit clean.)
    let coarse = 5;
    let bracket = 15;
    let best_i = scale::coarse_to_fine_argmax(steps, coarse, bracket, score_at);
    canonical_deg - CUSP_HALF_WINDOW_DEG + best_i as f64
}

/// The chroma ratio (for [`ChromaPolicy::Relative`]) that lands a colour of Oklab
/// lightness `l_ok` and hue `hue_deg` on perceptual colorfulness `target_mp`
/// (CAM16-UCS `M'`), bounded by the gamut wall.
///
/// `M'` rises monotonically with chroma at fixed lightness and hue, so the ratio
/// is found by bisection: build the colour at a trial ratio, measure its `M'`
/// through the same CAM16-UCS path the engine uses ([`LcsColor::mp`]), and
/// narrow. If even `ratio = 1` (the gamut maximum) cannot reach `target_mp`, the
/// gamut is the limit — return `1.0` and let the colourfulness sit at the most
/// the gamut allows (honestly below target at pinched extremes) rather than fake
/// it. This solver does not classify human perceptibility.
fn ratio_for_target_mp(l_ok: f64, hue_deg: f64, target_mp: f64, vc: &ViewingConditions) -> f64 {
    let target = target_mp;
    // The in-gamut max chroma depends only on `(l_ok, hue_deg)`, both fixed across
    // the ratio bisection — solve it once here instead of re-solving it on every
    // `mp_at` iteration (the bisection ran it ~30× per call). Bit-identical: the
    // value fed to every `build_curve_color_with_cmax` is the same `max_chroma`
    // the per-iteration call produced.
    let c_max = scale::max_chroma(l_ok, hue_deg);
    let mp_at = |ratio: f64| -> f64 {
        // `build_curve_color_with_cmax` returns clamped linear sRGB; quantise it to
        // the display grid (the byte-for-byte identity of the old hex round-trip)
        // and measure M' directly — no `format!`/parse on the bisection's hot path.
        let rgb = crate::spaces::srgb::quantise_srgb(build_curve_color_with_cmax(
            l_ok, hue_deg, ratio, c_max,
        ));
        crate::lcs::LcsColor::mp_of_linear_srgb(rgb, vc)
    };

    // The gamut maximum cannot reach the target — take all the gamut offers.
    if mp_at(1.0) <= target {
        return 1.0;
    }
    // Bisect ratio in [0, 1] for the one that hits target_mp. The ratio scales a
    // chroma that is then quantised to the 8-bit hex grid, so once the bracket is
    // narrower than `RATIO_BISECT_EPS` the emitted colour can no longer move and
    // the remaining halvings are wasted CAM16 evaluations. The early exit is
    // exact — the same value the full 48-step loop converged to.
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;
    for _ in 0..48 {
        if hi - lo < RATIO_BISECT_EPS {
            break;
        }
        let mid = (lo + hi) * 0.5;
        if mp_at(mid) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) * 0.5
}

/// The chroma-ratio bracket width below which the ratio bisection has pinned the
/// undertone chroma finely enough that the emitted 8-bit hex cannot change. At
/// ~1e-9 it is far tighter than the chroma step one hex byte spans, so the early
/// exit is provably hex-preserving while cutting the bisection from 48 steps to
/// ~30.
const RATIO_BISECT_EPS: f64 = 1e-9;

/// Build the in-gamut linear-sRGB colour at Oklab lightness `l_ok`, hue
/// `hue_deg`, carrying `ratio` of the in-gamut maximum chroma — the same
/// construction [`solve::solve`] applies internally, mirrored here so the curve
/// can measure the `M'` a candidate ratio would yield before committing to it.
#[cfg(test)]
fn build_curve_color(l_ok: f64, hue_deg: f64, ratio: f64) -> [f64; 3] {
    build_curve_color_with_cmax(l_ok, hue_deg, ratio, scale::max_chroma(l_ok, hue_deg))
}

/// [`build_curve_color`] with the in-gamut max chroma supplied by the caller, so
/// a loop over many `ratio`s at a fixed `(l_ok, hue_deg)` solves `max_chroma`
/// once instead of per iteration. Bit-identical to `build_curve_color` when
/// `c_max == max_chroma(l_ok, hue_deg)`.
fn build_curve_color_with_cmax(l_ok: f64, hue_deg: f64, ratio: f64, c_max: f64) -> [f64; 3] {
    use crate::spaces::oklab::oklab_to_srgb_linear;
    let hr = hue_deg.to_radians();
    let chroma = ratio.clamp(0.0, 1.0) * c_max;
    let lab = [l_ok, chroma * hr.cos(), chroma * hr.sin()];
    let rgb = oklab_to_srgb_linear(lab);
    [
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
    ]
}

/// The default, overridable recipe set mapping every `Role` to a [`RoleSpec`].
///
/// [`default`](RoleTable::default) is the calibrated v1 table; override any
/// single role with [`with`](RoleTable::with) and the rest stay at their
/// defaults. A custom table is how a caller tunes one role's target without
/// touching the others.
#[derive(Debug, Clone, PartialEq)]
#[cfg(test)]
pub struct RoleTable {
    specs: [(Role, RoleSpec); 19],
    chroma: RoleChroma,
}

#[cfg(test)]
impl RoleTable {
    /// The recipe for `role` in this table.
    pub fn spec(&self, role: Role) -> RoleSpec {
        self.specs
            .iter()
            .find(|(r, _)| *r == role)
            .map(|(_, s)| *s)
            // Every variant of the (closed-construction) v1 enum is present in
            // `specs`; the table is built from `Role::ALL`.
            .unwrap_or(RoleSpec::Zero)
    }

    /// The chroma policy this table applies to every role (v1: always neutral).
    pub fn chroma(&self) -> RoleChroma {
        self.chroma
    }

    /// The minimum WCAG 2.1 contrast ratio this role is legally clamped to, if
    /// any.
    ///
    /// This is the explicit final-emission criterion for `role`, independent of
    /// the Ys candidate-score target and of the background. Anchored (text / UI)
    /// roles carry their [`TextAnchor`]'s WCAG conformance
    /// ([`Floor::AaText`] → 4.5, [`Floor::AaUi`] → 3.0); every decorative /
    /// decorative / zero role has no legal floor and returns `None`.
    ///
    /// A runtime that eases between resolved themes uses this to *hold the
    /// floor every frame* during the transition: an intermediate (interpolated)
    /// colour is only allowed to be served while it still clears this ratio
    /// against the live background. The value is a property of the contract,
    /// not of any one solve, so it is exposed alongside each resolved role.
    pub fn legal_floor(&self, role: Role) -> Option<f64> {
        self.spec(role).legal_floor()
    }

    /// Return a copy with `role`'s recipe replaced — every other role keeps its
    /// default. This is the role-table override seam.
    pub fn with(mut self, role: Role, spec: RoleSpec) -> Self {
        if let Some(entry) = self.specs.iter_mut().find(|(r, _)| *r == role) {
            entry.1 = spec;
        }
        self
    }

    /// Return a copy with the chroma policy replaced wholesale.
    ///
    /// The default table carries the v2 undertone curve ([`RoleChroma::Curve`]);
    /// this is the seam that overrides it completely — pass
    /// [`RoleChroma::Neutral`] for the achromatic pure-grey behaviour,
    /// an explicit [`RoleChroma::Tinted`] or [`RoleChroma::Curve`] for another
    /// test policy.
    /// The override is total: it replaces the policy for *every* role, including
    /// dropping the tint to zero.
    pub fn with_chroma(mut self, chroma: RoleChroma) -> Self {
        self.chroma = chroma;
        self
    }
}

#[cfg(test)]
impl Default for RoleTable {
    /// The v1 role table.
    ///
    /// Text fractions are calibrated against Daniel's Figma "Labels/Neutral"
    /// anchors on white, transferred into the Ys readability metric (the
    /// genesis anchors were measured in the legacy Y_hk metric; the transfer
    /// invariant is the COLOUR, not the Lc number). Maximum achievable
    /// contrast on white is 106.0407 Ys-Lc (black on white):
    ///
    /// | Role | genesis Lc (Y_hk) | anchor colour | Ys Lc | fraction of max |
    /// |------|-------------------|---------------|-------|-----------------|
    /// | primary | 102.6 | `#141414` (accepted) | 103.2157 | 0.97335917 |
    /// | secondary | 66.5 | `#767676` (accepted) | 68.2467 | 0.64359014 |
    /// | muted (tertiary) | 48.9 | `#9C9C9C` (inverted) | 50.4459 | 0.47572199 |
    /// | disabled (quaternary) | 29.3 | `#C2C2C2` (accepted) | 31.1081 | 0.29335999 |
    ///
    /// "Accepted" anchor colours are the ladder hexes Daniel signed off
    /// (byte-identity is the review acceptance criterion), so solver
    /// quantisation lands exactly back on them; tertiary's emission is
    /// protected by the 3:1 floor (`#949494`), so its anchor is the byte-level
    /// inversion of the genesis 48.9 instead.
    ///
    /// Primary's 0.973 makes it "almost the maximum the background allows" — the
    /// anchor principle, not a fixed delta — so it reads black/white on the
    /// extremes rather than grey. The fractions are equal across polarities by
    /// design, which is the deliberate correction of the asymmetry in the
    /// literal Figma tokens (dark anchors were −105.4/−40.9/−26.2/−13.1: a
    /// dark hierarchy ~40 % weaker than light).
    ///
    /// Conformance: primary/secondary carry the AA text floor (4.5:1), muted and
    /// icon the AA UI floor (3:1), disabled carries none (WCAG excludes inactive
    /// controls). Decorative roles carry Lc magnitudes with no floor.
    fn default() -> Self {
        let anchor = |fraction, conformance| {
            RoleSpec::Anchor(TextAnchor {
                fraction,
                conformance,
                hue: None,
            })
        };
        // Lc decorative magnitudes — the shadow stack only (its owner anchors
        // are alpha opacities, not dJ' steps). See `surface-jnd` for context.
        let decorative = |magnitude| RoleSpec::Decorative { magnitude };
        // dJ' decorative steps carry the owner's LITERAL per-theme anchors.
        let dj = |magnitude_dj| RoleSpec::DecorativeDj { magnitude_dj };
        Self {
            specs: [
                // Labels — the text ladder, renamed from text-* to the owner's HIG
                // names. The contracts are carried over 1:1 (0.97335917 /
                // 0.64359014 / 0.47572199 / 0.29335999 with the same
                // AaText/AaText/AaUi/None floors), so the
                // emitted colours are byte-identical to the old text-* roles.
                (
                    Role::LabelPrimary,
                    anchor(LABEL_PRIMARY_FRACTION, Floor::AaText),
                ),
                (
                    Role::LabelSecondary,
                    anchor(LABEL_SECONDARY_FRACTION, Floor::AaText),
                ),
                (
                    Role::LabelTertiary,
                    anchor(LABEL_TERTIARY_FRACTION, Floor::AaUi),
                ),
                (
                    Role::LabelQuaternary,
                    anchor(LABEL_QUATERNARY_FRACTION, Floor::None),
                ),
                // Separator — Lc decorative (no owner dJ' anchor for it).
                (Role::Separator, decorative(SEPARATOR_DECORATIVE_LC)),
                // Border ladder. Strong is an ANCHOR (HIG Border/Strong = N12 =
                // Labels/Primary strength): the label-primary FRACTION with a
                // non-text 3:1 floor (WCAG 1.4.11) — a border must be
                // distinguishable, not readable. Base/Soft are dJ' steps carrying
                // the owner's LITERAL anchors (light/dark per theme); base
                // stronger than soft is the order contract.
                (
                    Role::BorderStrong,
                    anchor(LABEL_PRIMARY_FRACTION, Floor::AaUi),
                ),
                (Role::BorderBase, dj(BORDER_BASE_DJ)),
                (Role::BorderSoft, dj(BORDER_SOFT_DJ)),
                (Role::BorderNone, RoleSpec::Zero),
                // Fill ladder — dJ' steps with the owner's LITERAL Figma-computed
                // anchors (light 7.93/6.41/4.63/3.15, dark 17.67/15.78/12.01/8.22),
                // strictly descending in visibility (primary most visible →
                // quaternary faintest). The anchors are the contract.
                (Role::FillPrimary, dj(FILL_PRIMARY_DJ)),
                (Role::FillSecondary, dj(FILL_SECONDARY_DJ)),
                (Role::FillTertiary, dj(FILL_TERTIARY_DJ)),
                (Role::FillQuaternary, dj(FILL_QUATERNARY_DJ)),
                (Role::FillNone, RoleSpec::Zero),
                // Shadow stack — strictly ascending in visibility (minor
                // subtlest → major strongest), the progressive stack the
                // owner's FX/Shadow ramp describes (Minor < Ambient < Penumbra <
                // Major). Magnitudes stay above the reliable floor.
                (Role::ShadowMinor, decorative(SHADOW_MINOR_LC)),
                (Role::ShadowAmbient, decorative(SHADOW_AMBIENT_LC)),
                (Role::ShadowPenumbra, decorative(SHADOW_PENUMBRA_LC)),
                (Role::ShadowMajor, decorative(SHADOW_MAJOR_LC)),
                // The universal zero token.
                (Role::None, RoleSpec::Zero),
            ],
            chroma: RoleChroma::neutral_curve(),
        }
    }
}

/// Единственные категории отказа, которые может нести допущенная роль.
///
/// Rejected-запросы, неподдержанные capability и внутренний дрейф отсутствуют
/// намеренно: [`resolve_named_set`] возвращает их как [`ResolveSetError`] на
/// весь набор, чтобы потребитель не принял их за «один недостающий цвет».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleFailureCategory {
    /// Объявленный физический/выходной домен доказывает: решения нет.
    Unreachable,
    /// Ограниченный алгоритм завершился, не доказав достижимость ни в одну сторону.
    Unresolved,
}

impl RoleFailureCategory {
    /// Стабильное wire-написание для биндингов.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unreachable => SolveFailureCategory::Unreachable.as_str(),
            Self::Unresolved => SolveFailureCategory::Unresolved.as_str(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct BoundaryFailure {
    reason: SolveFailure,
    boundary: SolveFailureBoundary,
}

#[derive(Debug, Clone, PartialEq)]
enum RoleFailureState {
    Unreachable(BoundaryFailure),
    Unresolved(BoundaryFailure),
}

/// Допущенный пер-ролевой отказ из единственной классификации ядра
/// [`SolveFailure::boundary`].
///
/// Поля и конструкторы приватны намеренно: создать это значение может только
/// финальный проход допуска набора, и он принимает ровно `unreachable` или
/// `unresolved`.
#[derive(Debug, Clone, PartialEq)]
pub struct RoleFailure {
    state: RoleFailureState,
}

impl RoleFailure {
    const fn evidence(&self) -> &BoundaryFailure {
        match &self.state {
            RoleFailureState::Unreachable(evidence) | RoleFailureState::Unresolved(evidence) => {
                evidence
            }
        }
    }

    /// Суженная семантическая категория этого локального отказа роли.
    pub const fn category(&self) -> RoleFailureCategory {
        match &self.state {
            RoleFailureState::Unreachable(_) => RoleFailureCategory::Unreachable,
            RoleFailureState::Unresolved(_) => RoleFailureCategory::Unresolved,
        }
    }

    /// Стабильный машинный код, принадлежащий ядру.
    pub const fn code(&self) -> &'static str {
        self.evidence().boundary.code()
    }

    /// Структурированное свидетельство солвера внутри допущенного отказа.
    pub const fn reason(&self) -> &SolveFailure {
        &self.evidence().reason
    }
}

impl core::fmt::Display for RoleFailure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.reason().fmt(f)
    }
}

impl std::error::Error for RoleFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.reason())
    }
}

/// Почему резолв уже скомпилированного набора отказал атомарно.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveSetErrorKind {
    /// Значение запроса вышло за объявленный домен.
    Rejected,
    /// Состояние, произведённое ядром, нарушило внутренний постинвариант.
    Internal,
}

impl ResolveSetErrorKind {
    /// Стабильное диагностическое написание для whole-call-отказов в биндингах.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rejected => SolveFailureCategory::Rejected.as_str(),
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ResolveSetErrorState {
    Rejected(BoundaryFailure),
    Internal(SolveFailure),
}

/// Отказ всего набора из [`resolve_named_set`].
///
/// Rejected-запросы и внутренний дрейф закрывают весь вызов. Конструкторы
/// приватны; допуск делит [`SolveFailure::boundary`] с [`RoleFailure`], а
/// [`Self::kind`] и [`Self::code`] остаются авторитетной whole-call-
/// классификацией.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolveSetError {
    state: ResolveSetErrorState,
}

impl ResolveSetError {
    /// Суженная whole-call-категория.
    pub const fn kind(&self) -> ResolveSetErrorKind {
        match &self.state {
            ResolveSetErrorState::Rejected(_) => ResolveSetErrorKind::Rejected,
            ResolveSetErrorState::Internal(_) => ResolveSetErrorKind::Internal,
        }
    }

    /// Стабильный машинный код ядра для rejected-отказов.
    /// У внутреннего дрейфа намеренно нет публичного solver-кода.
    pub const fn code(&self) -> Option<&'static str> {
        match &self.state {
            ResolveSetErrorState::Rejected(evidence) => Some(evidence.boundary.code()),
            ResolveSetErrorState::Internal(_) => None,
        }
    }

    /// Структурированный исходный отказ — диагностика и точные evidence-поля.
    pub const fn reason(&self) -> &SolveFailure {
        match &self.state {
            ResolveSetErrorState::Rejected(evidence) => &evidence.reason,
            ResolveSetErrorState::Internal(reason) => reason,
        }
    }
}

impl core::fmt::Display for ResolveSetError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.reason().fmt(f)
    }
}

impl std::error::Error for ResolveSetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.reason())
    }
}

/// Внутреннее состояние роли до того, как весь набор прошёл допуск отказов.
type PendingResolution = Result<Resolved, SolveFailure>;

/// Исход резолва одной допущенной роли: решённый цвет, честный ноль,
/// типизированная численная неопределённость или локальный отказ роли.
///
/// Доказанная недостижимость и незавершённый bounded search отдаются пер-ролью
/// и не маскируются. Rejected/internal провенанс в этом типе жить не может: он
/// закрывает [`resolve_named_set`] через [`ResolveSetError`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Resolved {
    /// A solved colour for a text/UI or decorative role. `compressed` is `true`
    /// when the legal floor squeezed this role's target against its senior's so
    /// the exact hierarchy target could not hold. The role is either demoted to
    /// the smallest still-legal step, copied from a still-legal senior, or kept at
    /// its own legal colour when hierarchy and floor conflict. No normative
    /// WCAG floor is traded for ordering. See the module docs.
    ///
    /// `achieved_dj` — честный замер |ΔJ'| на отданном hex для dJ'-ролей
    /// (как явно названные [`GlowResolved::halo_achieved_dj`] и
    /// [`GlowResolved::core_achieved_dj`] для изолированных Glow-слоёв); `None` у
    /// score-ролей (их переходная Ys candidate-координата — [`Solved::lc`]).
    Color {
        solved: Solved,
        compressed: bool,
        achieved_dj: Option<f64>,
    },
    /// Полупрозрачная роль лестницы/альфа-аналога: `rgba(tint, α)`, которую
    /// потребитель красит НАПРЯМУЮ (закон лестницы labui — композитит браузер).
    /// Несёт солид-композит на фоне резолва для честного замера контраста.
    Translucent(TranslucentResolved),
    /// Свечение: screen-слои (core, halo) + решённая интенсивность. Потребитель
    /// красит слои с `mix-blend-mode: screen`.
    Glow(GlowResolved),
    /// Стабильный запрос Glow, для которого отсутствует sound численная граница:
    /// семантический победитель не выбран, CSS-эмиссия отсутствует.
    GlowIndeterminate(GlowIndeterminateResolved),
    /// Двухслойный материал (стекло/акрил): полупрозрачный тинт `01` + опаковая
    /// база `02`, обе — один тон, с ВЫВЕДЕННОЙ альфой (композит-гарантия над
    /// коридором фонов). См. [`MaterialResolved`] и [`crate::material`].
    Material(MaterialResolved),
    /// The honest zero of the `Role::None` token: no colour, no contrast.
    None,
    /// Доказанно недостижимый контракт либо незавершённый bounded search.
    /// Иной провенанс отказа — [`ResolveSetError`] на весь набор.
    Failure(RoleFailure),
}

/// Допустить один сырой результат роли в терминальную поверхность набора.
///
/// Единственное место, где широкие отказы солвера делятся на локальные данные
/// роли и whole-call-ошибки. Читает существующий boundary-дескриптор; второй
/// таблицы «вариант → категория» здесь нет.
fn classify_role_failure(reason: SolveFailure) -> Result<RoleFailure, ResolveSetError> {
    match reason.boundary() {
        Some(boundary) => match boundary.category() {
            SolveFailureCategory::Unreachable => Ok(RoleFailure {
                state: RoleFailureState::Unreachable(BoundaryFailure { reason, boundary }),
            }),
            SolveFailureCategory::Unresolved => Ok(RoleFailure {
                state: RoleFailureState::Unresolved(BoundaryFailure { reason, boundary }),
            }),
            SolveFailureCategory::Rejected => Err(ResolveSetError {
                state: ResolveSetErrorState::Rejected(BoundaryFailure { reason, boundary }),
            }),
        },
        None => Err(ResolveSetError {
            state: ResolveSetErrorState::Internal(reason),
        }),
    }
}

fn admit_resolution(pending: PendingResolution) -> Result<Resolved, ResolveSetError> {
    match pending {
        Ok(resolved) => Ok(resolved),
        Err(reason) => classify_role_failure(reason).map(Resolved::Failure),
    }
}

/// Резолв полупрозрачной роли: пара `(tint, α)` для прямой эмиссии `rgba(...)`
/// плюс её солид-композит на фоне резолва для замера контраста.
///
/// Потребитель красит `--lab-{role}: rgba(tint, α)` — браузер композитит на
/// фактической подложке. `composite` — то, во что этот rgba складывается на
/// ФОНЕ РЕЗОЛВА (`α·tint + (1−α)·bg`) в объявленном encoded-sRGB8 reference-
/// профиле, заземлённом Figma-якорями, но не выдаваемом за универсальный
/// браузерный pipeline; его контраст ([`TranslucentResolved::composite_lc`],
/// [`composite_wcag`](TranslucentResolved::composite_wcag)) — то, что фаза 1 AA меряет
/// (контраст полупрозрачной роли определён её композитом, не тинтом). На ином
/// фоне композит другой — это и есть смысл альфы; гарантия сформулирована для
/// фона резолва.
#[derive(Debug, Clone, PartialEq)]
pub struct TranslucentResolved {
    /// Тинт `#RRGGBB` — цвет, эмитируемый как `rgba(tint, α)` (без учёта α).
    tint_hex: String,
    /// Фактическая α `(0, 1]` — запрошенная, если существует точный байтовый тинт,
    /// иначе первый проходящий `binary64` (у прямой лестницы = альфа позиции).
    alpha: f64,
    /// Солид-композит `rgba(tint, α)` над фоном резолва, `#RRGGBB`.
    composite_hex: String,
    /// Знаковая кандидатная оценка `Lc` по `Ys` для композита и фона резолва.
    composite_lc: f64,
    /// WCAG 2.1 контраст-отношение композита против фона резолва (1–21).
    composite_wcag: f64,
    /// Композит отличим от фона на 8-битной сетке дисплея.
    ///
    /// `false` — вырожденный случай «тинт ≈ фон»: квантованный композит
    /// побайтно равен квантованному фону, эмиссия роли — пиксельный no-op
    /// (класс, признанный в `ladder.rs`: «тинт ≈ фон ⇒ dJ = 0 при любой α»;
    /// до этого флага такие тени/свечения проходили как валидный резолв
    /// молча). Параметр-свободный замер: сетка дисплея, не политика.
    composite_distinct: bool,
    /// Запрошенная α была поднята до первого разрешимого sRGB8-значения, потому
    /// что ни один байтовый тинт не воспроизводит цель — честный флаг деградации
    /// КОНТРАКТА
    /// РОЛИ (симметрия с `compressed`/`degraded`). Ставится только на пути
    /// альфа-аналога ([`resolve_rgba_inverted`]), где солид-цель фиксирована, а
    /// α выводится; у прямой лестницы ([`resolve_rgba_direct`]) всегда `false`.
    ///
    /// Цвет при этом НЕ врёт: композит фактической пары остаётся ПОБАЙТНО равен
    /// солиду (двигается только прозрачность — см. [`crate::alpha`]). Флаг лишь
    /// объявляет, что эмитированная α — не запрошенная, а первая разрешимая
    /// ([`alpha`](Self::alpha) несёт фактическое значение).
    alpha_coerced: bool,
    /// Солидная семейная граница (`border-<family>-strong`, M2 ch5c) была
    /// притемнена по кривой семьи до юр. пола UI (3:1), потому что тинт-якорь
    /// семьи не держал 3:1 на этом фоне. Честный флаг минимального легального
    /// сдвига по объявленной кривой семьи. Флаг не утверждает перцептивную
    /// сохранность hue/chroma: истиной представления остаются финальные байты.
    /// `false` у прямой лестницы и когда семейный солид уже легален.
    floor_coerced: bool,
}

impl TranslucentResolved {
    /// Тинт `#RRGGBB` — красится как `rgba(tint, α)`.
    pub fn tint_hex(&self) -> &str {
        &self.tint_hex
    }

    /// Фактическая альфа `(0, 1]`.
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    /// Солид-композит `#RRGGBB` над фоном резолва.
    pub fn composite_hex(&self) -> &str {
        &self.composite_hex
    }

    /// Знаковая кандидатная оценка `Lc` по `Ys` композита против фона резолва.
    pub fn composite_lc(&self) -> f64 {
        self.composite_lc
    }

    /// WCAG 2.1 контраст-отношение композита против фона резолва.
    pub fn composite_wcag(&self) -> f64 {
        self.composite_wcag
    }

    /// Композит отличим от фона резолва на 8-битной сетке дисплея.
    ///
    /// `false` — эмиссия роли является пиксельным no-op на этом фоне
    /// (вырожденный тинт ≈ фон); потребитель обязан считать такую
    /// тень/свечение побайтовым no-op в объявленном reference-профиле.
    pub fn composite_distinct(&self) -> bool {
        self.composite_distinct
    }

    /// Запрошенная α была поднята до первого проходящего sRGB8-значения
    /// (альфа-аналог с неразрешимой
    /// запрошенной прозрачностью). `false` у прямой лестницы и когда
    /// запрошенная α разрешима как есть. Цвет композита при этом равен солиду
    /// побайтно — коэрсится только прозрачность (см. поле-документацию).
    pub fn alpha_coerced(&self) -> bool {
        self.alpha_coerced
    }

    /// Солидная семейная граница притемнена до юр. пола UI (M2 ch5c). `false` у
    /// прямой лестницы и легальных семейных солидов. См. поле-документацию.
    pub fn floor_coerced(&self) -> bool {
        self.floor_coerced
    }
}

/// Резолв свечения: двухслойная анатомия + решённая интенсивность.
///
/// Слои — [`crate::glow::glow_layers_from_source`] (halo = источник, core =
/// пересвет); α — [`crate::glow::solve_screen_alpha_for_dj`] под контрактную
/// ступень на фактическом фоне; `target_status` сообщает исход объявленного
/// профиля исполнения. Legacy-профиль перечисляет все sRGB8-состояния точечного
/// композита этого screen-потока и при промахе цели выбирает максимум внутри
/// конечного набора; stable-профиль без sound bound возвращает типизированный
/// `Indeterminate`. На белом screen является точечным no-op только в объявленном
/// reference-профиле — это не утверждение о физическом свечении. Recipe,
/// appearance-диагностика, диагностика выбора и точный сертификат композита
/// возвращаются раздельно: ни одно из них не повышает силу другого.
#[derive(Debug, Clone, PartialEq)]
pub struct GlowResolved {
    core_hex: String,
    halo_hex: String,
    alpha: f64,
    alpha_css: String,
    target_dj: f64,
    halo_composite_hex: String,
    halo_achieved_dj: f64,
    core_composite_hex: String,
    core_achieved_dj: f64,
    target_status: crate::glow::GlowTargetStatus,
    layer_recipe_profile: crate::glow::GlowLayerRecipeProfileV1,
    appearance_diagnostic_profile: crate::glow::GlowDiagnosticProfileV1,
    selection_diagnostic_profile: Option<crate::glow::GlowDiagnosticProfileV1>,
    decision_outcome: crate::glow::GlowDecisionOutcomeV1,
    halo_composite_certificate: crate::glow::GlowCompositeCertificateV1,
    core_composite_certificate: crate::glow::GlowCompositeCertificateV1,
}

impl GlowResolved {
    /// Слой пересвета (малый радиус), `#RRGGBB`.
    pub fn core_hex(&self) -> &str {
        &self.core_hex
    }
    /// Слой ореола (большой радиус) — источник, `#RRGGBB`.
    pub fn halo_hex(&self) -> &str {
        &self.halo_hex
    }
    /// Решённая интенсивность screen-слоя `(0, 1]`.
    pub fn alpha(&self) -> f64 {
        self.alpha
    }
    /// Каноническая CSS-запись alpha с точным обратным чтением в тот же `f64`.
    pub fn alpha_css(&self) -> &str {
        &self.alpha_css
    }
    /// Целевой |ΔJ′| изолированного halo-композита.
    pub fn target_dj(&self) -> f64 {
        self.target_dj
    }
    /// Точный профиль композита, отдельный от гарантии диагностики или решения.
    pub fn composite_profile(&self) -> crate::glow::GlowCompositeProfileV1 {
        self.halo_composite_certificate.profile()
    }
    /// Версионированный профиль, построивший анатомию core/halo.
    pub fn layer_recipe_profile(&self) -> crate::glow::GlowLayerRecipeProfileV1 {
        self.layer_recipe_profile
    }
    /// Appearance-профиль изолированных point-слоёв Glow (core/halo меряются
    /// по отдельности; двухслойный пространственный стек, blur и геометрия не
    /// моделируются). Он всегда присутствует: даже
    /// выбор точного no-op вычисляет `core_achieved_dj` через CAM16-UCS J′.
    pub fn appearance_diagnostic_profile(&self) -> crate::glow::GlowDiagnosticProfileV1 {
        self.appearance_diagnostic_profile
    }
    /// Диагностический профиль, участвовавший именно в выборе target/max.
    /// У стабильного точного no-op его нет.
    pub fn selection_diagnostic_profile(&self) -> Option<crate::glow::GlowDiagnosticProfileV1> {
        self.selection_diagnostic_profile
    }
    /// Boundary-проекция прежнего клиентского профиля (migration adapter).
    pub fn decision_profile(&self) -> crate::glow::GlowDecisionProfileV1 {
        self.decision_outcome.decision_profile()
    }
    /// Атомарный исход решения: доказанный stable exact no-op либо явный
    /// registered compatibility-алгоритм (#292). Незаконные комбинации
    /// profile × guarantee непредставимы.
    pub fn decision_outcome(&self) -> crate::glow::GlowDecisionOutcomeV1 {
        self.decision_outcome
    }
    /// Слой, по которому решалась целевая ступень.
    pub fn constraint_layer(&self) -> crate::glow::GlowConstraintLayer {
        crate::glow::GlowConstraintLayer::Halo
    }
    /// Типизированный исход проверки цели.
    pub fn target_status(&self) -> crate::glow::GlowTargetStatus {
        self.target_status
    }
    /// Reference-композит изолированного halo на фоне резолва.
    pub fn halo_composite_hex(&self) -> &str {
        &self.halo_composite_hex
    }
    /// Фактический |ΔJ′| изолированного halo-композита.
    pub fn halo_achieved_dj(&self) -> f64 {
        self.halo_achieved_dj
    }
    /// Reference-композит изолированного core на том же фоне и alpha.
    pub fn core_composite_hex(&self) -> &str {
        &self.core_composite_hex
    }
    /// Фактический |ΔJ′| изолированного core-композита.
    pub fn core_achieved_dj(&self) -> f64 {
        self.core_achieved_dj
    }
    /// Точное свидетельство halo-композита.
    pub fn halo_composite_certificate(&self) -> &crate::glow::GlowCompositeCertificateV1 {
        &self.halo_composite_certificate
    }
    /// Точное свидетельство core-композита.
    pub fn core_composite_certificate(&self) -> &crate::glow::GlowCompositeCertificateV1 {
        &self.core_composite_certificate
    }
}

/// Стабильный терминальный исход Glow при отсутствии sound-границы target/max.
#[derive(Debug, Clone, PartialEq)]
pub struct GlowIndeterminateResolved {
    source_hex: String,
    target_dj: f64,
    decision_profile: crate::glow::GlowDecisionProfileV1,
    site_id: crate::numerics::NumericalSiteIdV1,
    evidence: crate::numerics::NumericalIndeterminacyV1,
}

impl GlowIndeterminateResolved {
    /// Канонический source/halo-якорь; он не эмитится без решения.
    pub fn source_hex(&self) -> &str {
        &self.source_hex
    }
    /// Запрошенный модуль диагностической цели.
    pub fn target_dj(&self) -> f64 {
        self.target_dj
    }
    /// Явный стабильный профиль из клиентского контракта.
    pub fn decision_profile(&self) -> crate::glow::GlowDecisionProfileV1 {
        self.decision_profile
    }
    /// Зарегистрированный чувствительный к ветвлению участок.
    pub fn site_id(&self) -> crate::numerics::NumericalSiteIdV1 {
        self.site_id
    }
    /// Неразделимая причина вместе с её sound-интервалом, если он существует.
    pub fn evidence(&self) -> crate::numerics::NumericalIndeterminacyV1 {
        self.evidence
    }
    /// Цель относится к точечному слою halo.
    pub fn constraint_layer(&self) -> crate::glow::GlowConstraintLayer {
        crate::glow::GlowConstraintLayer::Halo
    }
}

/// Резолв двухслойного материала: тон `T` на целевой светлоте тира +
/// ВЫВЕДЕННАЯ альфа тинта.
///
/// Потребитель красит `--lab-bg-material-<tier>-01: oklch(<tone> / α)`
/// (полупрозрачный слой стекла) и `-02: <tone>` (опаковая база под солид-каноном).
/// Солид-канон `01`-над-`02` = `α·T + (1−α)·T = T` — БАЙТ-ТОЧНО равен тону при
/// любой α (композит `T` над `T` есть `T`), поэтому [`tint_hex`](Self::tint_hex),
/// [`base_hex`](Self::base_hex) и [`solid_hex`](Self::solid_hex) равны по
/// построению (единственная решаемая величина — α).
///
/// Гарантия читаемости — свойство GLASS-режима (тинт над живым фоном): α —
/// повторно проверенный верхний кандидат, при котором коммит-полюс поверхности
/// ([`pole`](Self::pole)) держит [`floor`](Self::floor) по всему коридору
/// `[чёрный, белый]` ([`crate::material`]). [`worst_contrast`](Self::worst_contrast)
/// и [`alpha_status`](Self::alpha_status) пересчитываемы потребителем из эмитированных
/// `01`/`02` только по зафиксированному continuous interval profile
/// [`crate::material`]: byte-scale affine order `B + α·(T−B)` исполняется без
/// промежуточного округления в sRGB8, затем расширяется conservative envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialResolved {
    tone_hex: String,
    alpha: f64,
    worst_contrast: f64,
    alpha_guarantee: crate::material::MaterialAlphaGuaranteeV1,
    alpha_status: crate::material::MaterialAlphaStatusV1,
    floor: f64,
    pole: crate::material::Pole,
    achieved_dj: f64,
    tone_compressed: bool,
    distinct: bool,
}

impl MaterialResolved {
    /// Тинт слоя `01` `#RRGGBB` — красится как `oklch(<tone> / α)`. Равен базе и
    /// солид-канону по построению (композит `T` над `T` есть `T`).
    pub fn tint_hex(&self) -> &str {
        &self.tone_hex
    }

    /// База слоя `02` `#RRGGBB` — опаковая подложка под солид-каноном. Равна тинту
    /// (тот же тон, лишь без прозрачности).
    pub fn base_hex(&self) -> &str {
        &self.tone_hex
    }

    /// Солид-канон `01`-над-`02` `#RRGGBB` — то, что видно в SOLID-режиме и в
    /// деградациях. Равен тону байт-точно.
    pub fn solid_hex(&self) -> &str {
        &self.tone_hex
    }

    /// Выбранная альфа тинта `01`, `[0, 1]`.
    pub fn alpha(&self) -> f64 {
        self.alpha
    }

    /// Худший WCAG-контраст коммит-полюса по коридору `[чёрный, белый]` при
    /// выведенной α (`[1, 21]`).
    pub fn worst_contrast(&self) -> f64 {
        self.worst_contrast
    }

    /// Численный класс выбора alpha-границы; точные утверждения композитора
    /// намеренно отделены от этого охарактеризованного для платформы свидетельства.
    pub fn alpha_guarantee(&self) -> crate::material::MaterialAlphaGuaranteeV1 {
        self.alpha_guarantee
    }

    /// Типизированный исход floor; недостижимость отделена от невалидного ввода.
    pub fn alpha_status(&self) -> crate::material::MaterialAlphaStatusV1 {
        self.alpha_status
    }

    /// Запрошенный WCAG-пол (например 4.5 / 3.0). Он выполнен только при
    /// [`MaterialAlphaStatusV1::Satisfied`](crate::material::MaterialAlphaStatusV1::Satisfied).
    pub fn floor(&self) -> f64 {
        self.floor
    }

    /// Коммит-полюс поверхности: полюс максимального контраста на тоне (белый на
    /// тёмном, чёрный на светлом).
    pub fn pole(&self) -> crate::material::Pole {
        self.pole
    }

    /// Фактический |ΔJ'| тона-базы от фона резолва — различимость поверхности
    /// (замер на эмитируемом hex).
    pub fn achieved_dj(&self) -> f64 {
        self.achieved_dj
    }

    /// Целевой |ΔJ'| тона не попал в бюджет локального ограниченного поиска;
    /// возвращён кандидат с минимальной ошибкой среди просмотренных. `false` в
    /// норме. Флаг не заявляет оптимум по всему гамуту.
    pub fn tone_compressed(&self) -> bool {
        self.tone_compressed
    }

    /// Солид-канон (тон) отличим от фона резолва на 8-битной сетке дисплея. `false`
    /// — тон ≈ фон, поверхность является пиксельным no-op на этом фоне.
    pub fn distinct(&self) -> bool {
        self.distinct
    }
}

impl Resolved {
    /// A non-compressed solved colour — the common case where the hierarchy holds
    /// strictly and no floor squeeze was needed.
    fn color(solved: Solved) -> Self {
        Resolved::Color {
            solved,
            compressed: false,
            achieved_dj: Option::None,
        }
    }

    /// The solved colour, if this role resolved to one.
    pub fn solved(&self) -> Option<&Solved> {
        match self {
            Resolved::Color { solved, .. } => Some(solved),
            _ => None,
        }
    }

    /// Whether this role produced an explicitly non-exact outcome:
    ///
    /// - contrast roles: the legal floor prevented the exact hierarchy target;
    ///   the colour was placed at a still-legal compressed step, or retained at
    ///   its own legal value when the floor and hierarchy order conflict;
    /// - decorative dJ' roles: the requested |ΔJ'| missed the budget of the
    ///   bounded candidate walk, so the lowest-error examined candidate was
    ///   emitted. This is not a whole-gamut optimality claim.
    ///
    /// `false` for the zero token and unreachable roles.
    pub fn compressed(&self) -> bool {
        matches!(
            self,
            Resolved::Color {
                compressed: true,
                ..
            }
        )
    }

    /// The signed Ys candidate score `Lc` of a resolved colour, if any. This is not
    /// an LPC/readability verdict. The zero token reports `0.0`; an unreachable role reports `None`; a
    /// [`Translucent`](Resolved::Translucent) role reports its **composite's** `Lc` (a
    /// semi-transparent role's contrast is that of its composite, not its tint).
    pub fn lc(&self) -> Option<f64> {
        match self {
            Resolved::Color { solved, .. } => Some(solved.lc()),
            Resolved::Translucent(r) => Some(r.composite_lc),
            // Свечение — не контраст-роль: его контракт — |ΔJ'| ступени, не Lc.
            Resolved::Glow(_) | Resolved::GlowIndeterminate(_) => Option::None,
            // Материал — поверхность, не контраст-роль: его контракт — WCAG
            // α-гарантия читаемости + |ΔJ'| различимость тона, не единый Lc.
            Resolved::Material(_) => Option::None,
            Resolved::None => Some(0.0),
            Resolved::Failure(_) => Option::None,
        }
    }

    /// The `(tint, α)` of a semi-transparent [`Translucent`](Resolved::Translucent) role, if this
    /// resolved to one. `None` for solved-colour / zero / unreachable roles.
    pub fn translucent(&self) -> Option<&TranslucentResolved> {
        match self {
            Resolved::Translucent(r) => Some(r),
            _ => Option::None,
        }
    }

    /// Слои свечения [`Glow`](Resolved::Glow)-роли, если роль решилась в
    /// свечение. `None` для остальных исходов (паритет с [`Self::translucent`]).
    pub fn glow(&self) -> Option<&GlowResolved> {
        match self {
            Resolved::Glow(g) => Some(g),
            _ => Option::None,
        }
    }

    /// Двухслойный материал [`Material`](Resolved::Material)-роли, если роль
    /// решилась в материал. `None` для остальных исходов (паритет с
    /// [`Self::translucent`]/[`Self::glow`]).
    pub fn material(&self) -> Option<&MaterialResolved> {
        match self {
            Resolved::Material(m) => Some(m),
            _ => Option::None,
        }
    }
}

/// Everything about a `(background, viewing-conditions)` pair that every role in
/// a set shares: the one polarity the whole table resolves in, and the maximum
/// contrast magnitude that polarity can supply.
///
/// Computing this once is what makes `resolve_set` solve the table in a single
/// sweep instead of re-deriving polarity (two probe solves) and the maximum (one
/// more) per role — 32 `solve` calls collapse to 12. It also *guarantees* a
/// uniform polarity across the set: every role reads its sign from the same
/// `polarity` field, so they cannot disagree.
#[derive(Debug, Clone)]
struct ResolveContext {
    /// The single polarity the whole set resolves in (chosen WCAG-first).
    polarity: Polarity,
    /// The maximum contrast magnitude the background supplies in `polarity`.
    /// Zero is a valid physical ceiling; derivation failures retain their typed
    /// provenance for every anchored role instead of becoming a sentinel.
    max_contrast: Result<f64, SolveFailure>,
    /// The background's quantised display-luminance (`Ys`) interval, computed
    /// once for the whole set. Every contrast role reuses it. `Err` means the
    /// background cannot be reduced, so every colour role surfaces that reason.
    interval: Result<solve::LumaInterval, SolveFailure>,
    /// Whether these conditions enforce increased contrast (IC).
    high_contrast: bool,
}

impl ResolveContext {
    /// Derive the shared context for `bg` under `vc`: pick the polarity, take the
    /// background's luminance interval once, then read the maximum contrast in
    /// that polarity back from the solver — reusing the interval, not re-forwarding.
    fn new(bg: &BgInput, vc: &ViewingConditions) -> Self {
        let polarity = choose_polarity(bg);
        // One background forward for the whole set; every role's solve reuses it.
        let interval = bg.luma_interval(vc);
        let max_contrast = interval
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|iv| max_contrast(bg, polarity, vc, *iv));
        Self {
            polarity,
            max_contrast,
            interval,
            high_contrast: vc.high_contrast,
        }
    }

    /// The signed `Lc` target for an anchored text/UI role: the chosen polarity's
    /// sign times `fraction` of the background's maximum contrast. Context
    /// derivation failures retain their original type.
    fn anchored_contract(&self, anchor: TextAnchor) -> Result<Contract, SolveFailure> {
        let max = *self.max_contrast.as_ref().map_err(Clone::clone)?;
        let target = self.polarity.sign() * anchor.fraction() * max;
        Ok(Contract::text(target))
    }

    /// The signed range contract for a decorative JND role: the chosen polarity's
    /// sign times a magnitude held above [`DECORATIVE_FLOOR_MIN`], no readability
    /// floor.
    ///
    /// Under high contrast the floor delta `IC_DECORATIVE_FLOOR_MIN −
    /// DECORATIVE_FLOOR_MIN` is applied as an ORDER-PRESERVING uniform shift on
    /// top of the validated magnitude — not as a `max` with the IC floor.
    /// A plain `max(|m|, 15.0)` collapsed every decorative magnitude below 15
    /// (the whole shadow stack 8/9.5/11.5/14 and the separator) onto one
    /// identical target, so under `-ic` all four shadows resolved to the same
    /// byte-identical colour, violating the stack's strictly-ascending contract
    /// and silently mutating the owner-measured Lc deltas. The shift keeps every
    /// pairwise gap exactly as measured while guaranteeing the result is at
    /// least `IC_DECORATIVE_FLOOR_MIN` (any input already sits at or above
    /// `DECORATIVE_FLOOR_MIN` by construction).
    fn decorative_contract(&self, magnitude: f64) -> Contract {
        debug_assert!(magnitude.is_finite() && magnitude >= DECORATIVE_FLOOR_MIN);
        let effective = if self.high_contrast {
            magnitude + (IC_DECORATIVE_FLOOR_MIN - DECORATIVE_FLOOR_MIN)
        } else {
            magnitude
        };
        let target = self.polarity.sign() * effective;
        // `range` already carries `Floor::None`; the degenerate band [t, t] targets t.
        Contract::range(target, target)
    }
}

/// Resolve one `Role` against `bg` under `vc`, using `table`'s recipe.
///
/// Polarity is chosen from the background (WCAG-first, see the module docs), so
/// the same role resolves on light or dark backgrounds. Returns:
///
/// * [`Resolved::Color`] — the solved colour for a text/UI or decorative role;
/// * [`Resolved::None`] — for the `Role::None` zero token;
/// * [`Resolved::Failure`] — when no colour can meet the role's contract on
///   this background (an extreme background, never a silent clip).
///
/// This solves the single role in isolation. The `compressed` flag has two
/// independent sources, and only one is suppressed here:
/// * **Hierarchy compression** is a *set* property — a role squeezed against its
///   senior's target — and is raised only by `resolve_set`, which sees a
///   role's seniors. In isolation it is therefore never set.
/// * **dJ'-path degradation** is a *single-role* property: a decorative dJ' role
///   ([`RoleSpec::DecorativeDj`]) whose magnitude misses the bounded local
///   selection budget reports `compressed == true` on its own (see `resolve_dj`),
///   even resolved here in isolation.
///
/// So a contract (Lc) role resolved here always reports `compressed == false`,
/// but a decorative dJ' role can report `compressed == true`.
///
/// * `bg` — the background to resolve against.
/// * `role` — which semantic slot to solve.
/// * `table` — the recipe set; pass [`RoleTable::default`] for the v1 table.
/// * `vc` — viewing conditions (light vs dim/dark); pass the same VC the theme
///   resolves under.
#[cfg(test)]
pub fn resolve(
    bg: &BgInput,
    role: Role,
    table: &RoleTable,
    vc: &ViewingConditions,
) -> Result<Resolved, ResolveSetError> {
    let ctx = ResolveContext::new(bg, vc);
    admit_resolution(resolve_in(bg, role, table, vc, &ctx))
}

/// Resolve one role through an already-derived [`ResolveContext`], so a whole set
/// shares one polarity and one maximum-contrast computation.
///
/// Thin wrapper over [`resolve_spec_in`]: it looks the role's recipe up in
/// `table` and resolves that recipe. Keeping the recipe-driven physics in
/// [`resolve_spec_in`] is the dependency-inversion seam — the config layer
/// ([`NamedRoleTable`]) resolves the *same* recipe against the *same* physics
/// without knowing about the `Role` enum, and the golden `Role` path stays a
/// byte-for-byte-equivalent wrapper.
#[cfg(test)]
fn resolve_in(
    bg: &BgInput,
    role: Role,
    table: &RoleTable,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) -> PendingResolution {
    resolve_spec_in(bg, &table.spec(role), table.chroma(), vc, ctx)
}

/// Resolve one [`RoleSpec`] against `bg` under an already-derived
/// [`ResolveContext`], applying `chroma` as the undertone policy.
///
/// This is the physics core the two front doors share: the `Role`-keyed
/// [`resolve_in`] and the string-keyed [`resolve_named_set`]. It takes a `&RoleSpec`
/// directly — not a `Role` — so a caller that names roles with arbitrary strings
/// (the consumer config) resolves them through the identical code path as the
/// built-in table. Nothing about the physics changes; only *where the recipe comes
/// from* differs, which is exactly the seam ADR-0001 opens.
fn resolve_spec_in(
    bg: &BgInput,
    spec: &RoleSpec,
    chroma: RoleChroma,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) -> PendingResolution {
    let contract = match *spec {
        RoleSpec::Zero => return Ok(Resolved::None),
        RoleSpec::Anchor(anchor) => {
            // Явный source-slot держит тот же контракт уровня; exact-gray остаётся
            // нейтральным, остальные байты несут направление. `None` берёт policy
            // таблицы.
            if let Some(hue_tint) = anchor.hue() {
                return resolve_hued_anchor(bg, anchor, hue_tint, vc, ctx);
            }
            ctx.anchored_contract(anchor)?
        }
        RoleSpec::DecorativeDj { magnitude_dj } => {
            // dJ' has its own analytic solver (J' offset, not an Lc contract); it
            // builds the undertone itself, so it does not route through
            // `solve_with_chroma`. Если цель не попала в бюджет локального
            // ограниченного обхода, кандидат с минимальной ошибкой среди
            // просмотренных возвращается с `compressed`; флаг не утверждает
            // оптимум по всему гамуту.
            return resolve_dj(bg, magnitude_dj.for_vc(vc), ctx.polarity, chroma, vc).map(|d| {
                Resolved::Color {
                    solved: d.solved,
                    compressed: d.degraded,
                    achieved_dj: Some(d.achieved_dj),
                }
            });
        }
        RoleSpec::Decorative { magnitude } => ctx.decorative_contract(magnitude),
        RoleSpec::Ladder {
            tint,
            alpha_light,
            alpha_dark,
            floor,
        } => {
            // Лестница эмитит rgba(tint, α) НАПРЯМУЮ: тинт — якорь источника по
            // теме (bg-независим), α — пер-темные данные позиции (light/dark).
            // Композит на фоне резолва — для честного замера контраста
            // (закон лестницы, `crate::ladder`).
            let alpha = if vc.is_dark_theme() {
                alpha_dark
            } else {
                alpha_light
            };
            // M2 ch5c: солидная семейная граница (`floor = Some`, α=1) обязана
            // держать юр. пол UI (3:1). Пол применим лишь к солиду — у
            // полупрозрачной позиции контраст определяется композитом, а не
            // тинтом, и притемнять тинт бессмысленно.
            if let Some(floor) = floor {
                if (alpha - 1.0).abs() < f64::EPSILON {
                    return resolve_solid_with_ui_floor(tint.for_vc(vc), floor, bg, vc, ctx);
                }
            }
            return resolve_rgba_direct(tint.for_vc(vc), alpha, bg, vc);
        }
        RoleSpec::Glow { tint, step, mode } => {
            let _ = (tint, step, mode);
            // Named runtime обязан перехватить этот ordinal скомпилированной
            // invocation до recipe-dispatch (C7e, как Material в C7d).
            // Исполнение raw variant здесь создало бы второй источник физики.
            return Err(SolveFailure::InternalInvariant(
                "glow recipe bypassed its compiled invocation".into(),
            ));
        }
        RoleSpec::AlphaAnalog { of, alpha } => {
            let _ = (of, alpha);
            // Named runtime обязан перехватить этот ordinal скомпилированной
            // invocation до recipe-dispatch. Исполнение raw variant здесь
            // создало бы второй источник физики.
            return Err(SolveFailure::InternalInvariant(
                "alpha-analog recipe bypassed its compiled invocation".into(),
            ));
        }
        RoleSpec::Material { .. } => {
            // Named runtime обязан перехватить этот ordinal скомпилированной
            // invocation до recipe-dispatch. Исполнение raw variant здесь
            // создало бы второй источник физики (см. C7d).
            return Err(SolveFailure::InternalInvariant(
                "material recipe bypassed its compiled invocation".into(),
            ));
        }
    };

    let interval = *ctx.interval.as_ref().map_err(Clone::clone)?;
    let criterion = match spec {
        RoleSpec::Anchor(anchor) => anchor.conformance().criterion(),
        _ => None,
    };
    let solved = solve_with_chroma(bg, contract, chroma, vc, interval, criterion)?;
    Ok(Resolved::color(solved))
}

/// Лестница: rgba(`tint`, `alpha`) эмитится напрямую; его композит на фоне
/// резолва замеряется для контраста. `tint` — кодированный (byte/255) sRGB.
///
/// Композитинг straight-alpha живёт в версионированном encoded-sRGB8 reference-
/// профиле, который воспроизводит измеренные Figma-якоря ([`crate::alpha`]), но
/// не сертифицирует любой pipeline браузера и управления цветом. Контраст меряется
/// на КОМПОЗИТЕ (солид-эквивалент), не на тинте: контраст полупрозрачной роли
/// определён тем, во что она складывается на подложке.
/// Квантовать кодированный цвет до 8-битной сетки (hex-roundtrip без строки):
/// эмиссия и замер обязаны считаться из одного — отдаваемого — значения.
fn quantise_encoded(e: [f64; 3]) -> [f64; 3] {
    let q = |c: f64| (c.clamp(0.0, 1.0) * 255.0).round() / 255.0;
    [q(e[0]), q(e[1]), q(e[2])]
}

fn encoded_rgb_valid(encoded: [f64; 3]) -> bool {
    encoded
        .iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(channel))
}

#[derive(Debug, Clone, Copy)]
struct SourceHuePlan {
    hue: Hue,
    chroma: ChromaPolicy,
}

/// Build the maximal-chroma plan only after exact sRGB8 hue classification.
/// Equal channel bytes remain neutral; Oklab matrix noise is never amplified.
fn source_hue_plan(source: Srgb8) -> SourceHuePlan {
    match crate::spaces::oklab::hue_of_srgb8(source) {
        crate::spaces::oklab::OklabHue::Achromatic => SourceHuePlan {
            hue: Hue::deg(0.0),
            chroma: ChromaPolicy::Neutral,
        },
        crate::spaces::oklab::OklabHue::Chromatic { degrees } => SourceHuePlan {
            hue: Hue::deg(degrees),
            chroma: ChromaPolicy::Relative(1.0),
        },
    }
}

fn role_alpha_valid(alpha: f64) -> bool {
    alpha.is_finite() && alpha > 0.0 && alpha <= 1.0
}

/// The public ladder-floor contract currently applies only to an opaque emitted
/// occurrence. For a translucent ladder the contract has not declared which
/// occurrence is constrained or whether tint, alpha, or both may move; accepting
/// it would invent semantics. `Some(Floor::None)` is a no-op disguised as a
/// constraint.
pub(crate) fn validate_ladder_floor(
    alpha_light: f64,
    alpha_dark: f64,
    floor: Option<Floor>,
) -> Result<(), &'static str> {
    match floor {
        None => Ok(()),
        Some(Floor::None) => Err("omit the ladder floor when no floor is required"),
        Some(_) if alpha_light == 1.0 && alpha_dark == 1.0 => Ok(()),
        Some(_) => Err("a ladder floor requires alpha = 1 in every theme"),
    }
}

/// Прямая rgba-лестница: спека зафиксировала точный тинт и α — движок НЕ решает
/// контракт читаемости, его работа здесь только честный замер. Светлота не
/// подбирается, α не коэрсится (флаги `finish_rgba(.., false, false)`);
/// composite/Lc/WCAG считаются из фактически эмитируемого 8-битного значения.
/// Альфа остаётся публичным полем `RoleSpec` и потому отвергается как вход.
/// Тинт к этому шву приходит только из валидированного `LadderTint` либо из
/// core-математики; его выход из домена был бы внутренним дефектом.
fn resolve_rgba_direct(
    tint_encoded: [f64; 3],
    alpha: f64,
    bg: &BgInput,
    vc: &ViewingConditions,
) -> PendingResolution {
    if !encoded_rgb_valid(tint_encoded) {
        return Err(SolveFailure::InternalInvariant(
            "validated/generated rgba tint left encoded sRGB domain".into(),
        ));
    }
    if !role_alpha_valid(alpha) {
        return Err(SolveFailure::InvalidInput(
            "rgba alpha must be finite and inside (0, 1]".into(),
        ));
    }
    let bg_encoded = bg.encoded_display();
    // Тинт квантуется ДО композита: сертификат и эмиссия обязаны ссылаться на
    // один и тот же encoded-sRGB8 reference-пиксель.
    let tint_q = quantise_encoded(tint_encoded);
    // Прямая лестница эмитит запрошенную α как есть — коэрсии нет по построению.
    finish_rgba(tint_q, alpha, bg_encoded, vc, false, false)
}

/// Резолв text/UI-якоря с явным источником цветовой идентичности.
///
/// Якорь держит тот же Lc-контракт уровня, что путь без явного source
/// ([`ResolveContext::anchored_contract`] — доля·max ахроматической полярности):
/// одноуровневость поперёк характеров ПО ПОСТРОЕНИЮ, потому что
/// цель — абсолютный Lc нейтральной ступени, а НЕ доля от максимума-в-оттенке
/// (последнее снова оказалось бы слабее нейтрали). Отличие от нейтрального пути —
/// физика источника:
///
/// * равные эмитируемые sRGB8-каналы не несут hue и используют neutral plan;
/// * неравные каналы задают Oklab-угол, а [`ChromaPolicy::Relative`]`(1.0)`
///   выбирает физический максимум доступной хромы на решённой светлоте.
///
/// W5: солвер больше не несёт юр. пола, потому `compressed` на этом пути всегда
/// `false`; нормативная читаемость финальной пары проверяется каноническим
/// WCAG22 evaluator-ом на emitted state, не скалярным поджатием контракта.
/// Сохранность воспринимаемой hue-идентичности из одних выходных байтов не
/// выводится; финальный hex остаётся единственной истиной представления.
fn resolve_hued_anchor(
    bg: &BgInput,
    anchor: TextAnchor,
    hue_tint: crate::ladder::LadderTint,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) -> PendingResolution {
    resolve_hued_anchor_from_srgb8(bg, anchor, hue_tint.srgb8_for_vc(vc), vc, ctx)
}

/// Тот же резолв, но выбранный по теме источник уже представлен точными
/// sRGB8-байтами. Для квантованного источника этот путь даёт тот же hex, что и
/// [`resolve_hued_anchor`], потому emission использует тот же byte-grid.
fn resolve_hued_anchor_from_srgb8(
    bg: &BgInput,
    anchor: TextAnchor,
    source: Srgb8,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) -> PendingResolution {
    let contract = ctx.anchored_contract(anchor)?;
    let interval = *ctx.interval.as_ref().map_err(Clone::clone)?;
    let plan = source_hue_plan(source);
    match solve_in_with_criterion(
        bg,
        contract,
        plan.hue,
        plan.chroma,
        vc,
        interval,
        anchor.conformance().criterion(),
    ) {
        Ok(solved) => Ok(Resolved::Color {
            solved,
            compressed: false,
            achieved_dj: Option::None,
        }),
        Err(reason) => Err(reason),
    }
}

/// Резолв СОЛИДНОЙ семейной границы `border-<family>-strong` с юр. полом UI
/// (ратификация ch5c, M2).
///
/// Солид семьи (α=1) обязан держать 3:1 (WCAG 1.4.11 для границ контролов). Если
/// композит тинта уже держит пол — эмитится точный исходный солид. Иначе —
/// минимальный легальный сдвиг по объявленной кривой семьи: контракт целит
/// естественный Lc тинта, а пол меняет lightness-кандидат ровно до легальности.
/// Гамут и квантование относятся к финальным байтам; перцептивная идентичность
/// отсюда не выводится. Сдвиг объявлен флагом
/// [`TranslucentResolved::floor_coerced`] — не молчаливая деградация. Эмиссия
/// остаётся полупрозрачной формой (`rgba`, α=1), как у любой семейной границы,
/// чтобы форма роли не расходилась между легальными и притемнёнными характерами.
fn resolve_solid_with_ui_floor(
    tint_encoded: [f64; 3],
    floor: Floor,
    bg: &BgInput,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) -> PendingResolution {
    if !encoded_rgb_valid(tint_encoded) {
        return Err(SolveFailure::InternalInvariant(
            "validated/generated solid tint left encoded sRGB domain".into(),
        ));
    }
    let bg_encoded = bg.encoded_display();
    let tint_q = quantise_encoded(tint_encoded);
    let Some(criterion) = floor.criterion() else {
        // Пол None (декоратив) — притемнять не нужно; прямой солид.
        return resolve_rgba_direct(tint_encoded, 1.0, bg, vc);
    };
    let tint_srgb8 = match crate::alpha::encoded_to_srgb8(tint_q, "solid tint") {
        Ok(s) => s,
        Err(reason) => {
            return Err(SolveFailure::InternalInvariant(format!(
                "validated solid tint could not be quantised: {reason}"
            )));
        }
    };
    let bg_srgb8 = match crate::alpha::encoded_to_srgb8(bg_encoded, "bg") {
        Ok(s) => s,
        Err(reason) => {
            return Err(SolveFailure::InternalInvariant(format!(
                "validated bg could not be quantised: {reason}"
            )));
        }
    };
    match crate::wcag22::evaluate_wcag22_srgb8(tint_srgb8, bg_srgb8, criterion)
        .map_err(|e| SolveFailure::InternalInvariant(format!("WCAG22 evaluation failed: {e:?}")))?
    {
        crate::wcag22::Wcag22AssessmentV1::Evaluated {
            decision: crate::wcag22::Wcag22ApplicableDecisionV1::Pass,
            ..
        } => {
            // Уже легально — точный семейный солид, без сдвига (floor_coerced = false).
            return resolve_rgba_direct(tint_encoded, 1.0, bg, vc);
        }
        crate::wcag22::Wcag22AssessmentV1::Evaluated {
            decision: crate::wcag22::Wcag22ApplicableDecisionV1::Fail,
            ..
        } => {}
        crate::wcag22::Wcag22AssessmentV1::NotEvaluated { .. } => {
            return Err(SolveFailure::InternalInvariant(
                "explicit solid criterion returned NotEvaluated".into(),
            ));
        }
    }
    // Нелегально: минимальный сдвиг по кривой семьи до пола.
    let interval = *ctx.interval.as_ref().map_err(Clone::clone)?;
    // Естественный знаковый Lc якоря и доля его хромы на собственной светлоте —
    // чтобы сдвиг сохранил насыщенность семьи, а не выехал на стену гамута.
    let tint_linear = [
        crate::spaces::srgb::srgb_gamma_inv(tint_q[0]),
        crate::spaces::srgb::srgb_gamma_inv(tint_q[1]),
        crate::spaces::srgb::srgb_gamma_inv(tint_q[2]),
    ];
    let bg_linear = [
        crate::spaces::srgb::srgb_gamma_inv(bg_encoded[0]),
        crate::spaces::srgb::srgb_gamma_inv(bg_encoded[1]),
        crate::spaces::srgb::srgb_gamma_inv(bg_encoded[2]),
    ];
    let (anchor_lc, _) = measure_contrast(bg_linear, tint_linear, vc);
    let source = Srgb8::new(tint_srgb8);
    let (hue, chroma_policy) = match crate::spaces::oklab::hue_of_srgb8(source) {
        crate::spaces::oklab::OklabHue::Achromatic => (Hue::deg(0.0), ChromaPolicy::Neutral),
        crate::spaces::oklab::OklabHue::Chromatic { degrees } => {
            let lab = crate::spaces::oklab::srgb_linear_to_oklab(tint_linear);
            let anchor_chroma = lab[1].hypot(lab[2]);
            let c_max = scale::max_chroma(lab[0], degrees);
            let chroma_ratio = if c_max > f64::EPSILON {
                (anchor_chroma / c_max).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (Hue::deg(degrees), ChromaPolicy::Relative(chroma_ratio))
        }
    };
    // Контракт целит естественный Lc. WCAG 1.4.11 остаётся caller-owned
    // final-emission predicate канонического WCAG22 evaluator-а.
    let contract = solve::Contract::text(anchor_lc);
    match solve_in_with_criterion(
        bg,
        contract,
        hue,
        chroma_policy,
        vc,
        interval,
        Some(criterion),
    ) {
        Ok(solved) => match crate::spaces::srgb::srgb_encoded_from_hex(solved.hex()) {
            Ok(shifted) => finish_rgba(shifted, 1.0, bg_encoded, vc, false, true),
            Err(reason) => Err(SolveFailure::InternalInvariant(format!(
                "solver emitted an invalid sRGB hex for solid-floor role: {reason}"
            ))),
        },
        Err(reason) => Err(reason),
    }
}

/// Замороженный recipe-frontend понижает solid-цель и локальный фон в общий
/// exact point-закон. Client routing заканчивается здесь и структурно не может
/// попасть в физический coordinator.
fn resolve_rgba_inverted_admitted(
    solid_encoded: [f64; 3],
    opacity_domain: crate::composition::OpacityDomainV1,
    bg: &BgInput,
    vc: &ViewingConditions,
) -> PendingResolution {
    // Тот же домен-гард, что у прямого rgba-пути: RoleSpec публичен. Без него
    // недоменная спека, собранная в обход валидатора конфига, дошла бы до
    // численного пути вместо честного типизированного исхода.
    if !encoded_rgb_valid(solid_encoded) {
        return Err(SolveFailure::InternalInvariant(
            "validated alpha-analog solid left encoded sRGB domain".into(),
        ));
    }
    let target = match crate::alpha::encoded_to_srgb8(solid_encoded, "solid") {
        Ok(target) => Srgb8::new(target),
        Err(error) => {
            return Err(SolveFailure::InternalInvariant(format!(
                "validated alpha-analog target left encoded sRGB domain: {error}"
            )));
        }
    };
    let backdrop = match crate::alpha::encoded_to_srgb8(bg.encoded_display(), "bg") {
        Ok(backdrop) => Srgb8::new(backdrop),
        Err(error) => {
            return Err(SolveFailure::InternalInvariant(format!(
                "validated alpha-analog backdrop left encoded sRGB domain: {error}"
            )));
        }
    };
    let verified = match crate::point_representation::resolve_exact_point_representation_v1(
        target,
        opacity_domain,
        backdrop,
    ) {
        Ok(verified) => verified,
        Err(_error) => {
            // Typed admission и полный byte-grid proof делают ветку
            // недостижимой, пока не изменился сам физический закон.
            return Err(SolveFailure::InternalInvariant(
                "validated point representation violated its total-domain contract".into(),
            ));
        }
    };
    let actual_opacity = verified.opacity();
    let actual_alpha = actual_opacity.value();
    // Резолвер возвращает тот же binary64 либо строго больший точный пол.
    let alpha_coerced = actual_alpha > opacity_domain.lower().value();
    finish_rgba_from_certificate(
        verified.source(),
        actual_alpha,
        verified.certificate(),
        vc,
        alpha_coerced,
        false,
    )
}

#[cfg(test)]
fn resolve_rgba_inverted(
    solid_encoded: [f64; 3],
    requested_alpha: f64,
    bg: &BgInput,
    vc: &ViewingConditions,
) -> PendingResolution {
    let opacity = crate::composition::AdmittedOpacityV1::new(requested_alpha)
        .ok()
        .filter(|opacity| opacity.value() > 0.0)
        .ok_or_else(|| {
            SolveFailure::InvalidInput("alpha-analog alpha must be finite and inside (0, 1]".into())
        })?;
    let domain = crate::composition::OpacityDomainV1::try_new(opacity.value(), 1.0)
        .map_err(|_| SolveFailure::InternalInvariant("invalid test opacity domain".into()))?;
    resolve_rgba_inverted_admitted(solid_encoded, domain, bg, vc)
}

/// Собрать [`Resolved::Translucent`] из эмитируемых тинта и альфы: вывести их
/// encoded-sRGB8 reference-композит и замерить его против фона резолва.
///
/// Числа вычисляются в тех же координатах, что и у solid-роли: Ys candidate
/// score ([`measure_contrast`]) и WCAG на кодированном дисплее — так
/// полупрозрачная роль сопоставима с solved-ролью на фазе 1 AA.
fn finish_rgba(
    tint_encoded: [f64; 3],
    alpha: f64,
    bg_encoded: [f64; 3],
    vc: &ViewingConditions,
    alpha_coerced: bool,
    floor_coerced: bool,
) -> PendingResolution {
    let tint = crate::alpha::encoded_to_srgb8(tint_encoded, "tint")
        .map(Srgb8::new)
        .map_err(|error| {
            SolveFailure::InternalInvariant(format!(
                "rgba tint вне encoded-sRGB8 reference-домена: {error}"
            ))
        })?;
    let backdrop = crate::alpha::encoded_to_srgb8(bg_encoded, "bg")
        .map(Srgb8::new)
        .map_err(|error| {
            SolveFailure::InternalInvariant(format!(
                "rgba backdrop вне encoded-sRGB8 reference-домена: {error}"
            ))
        })?;
    let occurrence = PointOpacityOverSurfaceV1::evaluate(tint.bytes(), alpha, backdrop.bytes())
        .map_err(|error| {
            SolveFailure::InternalInvariant(format!(
                "rgba-композит вне encoded-sRGB8 reference-домена: {}",
                error.message()
            ))
        })?;
    finish_rgba_from_certificate(
        tint,
        alpha,
        occurrence.certificate(),
        vc,
        alpha_coerced,
        floor_coerced,
    )
}

/// Финальная эмиссия читает байты уже исполненного occurrence. Ветка не умеет
/// композитить повторно, поэтому exact gate и публичный compositeHex физически
/// ссылаются на один результат.
fn finish_rgba_from_certificate(
    tint: Srgb8,
    alpha: f64,
    occurrence: &crate::appearance::SourceOverCertificateV1,
    vc: &ViewingConditions,
    alpha_coerced: bool,
    floor_coerced: bool,
) -> PendingResolution {
    use crate::spaces::srgb::srgb_gamma_inv;
    let composite = Srgb8::new(occurrence.output_rgb());
    let backdrop = Srgb8::new(occurrence.backdrop_rgb());
    let composite_q = composite.encoded();
    let bg_encoded = backdrop.encoded();
    // Линейный свет из кодированного (per-channel gamma-декод) для Ys candidate score.
    let decode = |e: [f64; 3]| {
        [
            srgb_gamma_inv(e[0]),
            srgb_gamma_inv(e[1]),
            srgb_gamma_inv(e[2]),
        ]
    };
    let composite_linear = decode(composite_q);
    let bg_linear = decode(bg_encoded);
    let (composite_lc, _) = measure_contrast(bg_linear, composite_linear, vc);
    let composite_wcag = crate::spaces::srgb::encoded_srgb_contrast_ratio(composite_q, bg_encoded);
    // Отличимость в encoded-sRGB8 reference: сравнение по тем же
    // 8-битным hex, из которых строится сертификат. Фон квантуется тем же
    // форматтером; этот вердикт не распространяется на иной renderer pipeline.
    let composite_distinct = composite != backdrop;
    Ok(Resolved::Translucent(TranslucentResolved {
        tint_hex: tint.to_hex(),
        alpha,
        composite_hex: composite.to_hex(),
        composite_lc,
        composite_wcag,
        composite_distinct,
        alpha_coerced,
        floor_coerced,
    }))
}

fn wcag22_final_emission_passes(
    foreground_hex: &str,
    background_hex: &str,
    criterion: Wcag22CriterionV1,
) -> Result<bool, SolveFailure> {
    let assessment = crate::wcag22::evaluate_wcag22_hex(foreground_hex, background_hex, criterion)
        .map_err(|error| {
            SolveFailure::InternalInvariant(format!(
                "canonical WCAG22 evaluator rejected generated sRGB8: {error:?}"
            ))
        })?;
    match assessment {
        crate::wcag22::Wcag22AssessmentV1::Evaluated {
            decision: crate::wcag22::Wcag22ApplicableDecisionV1::Pass,
            ..
        } => Ok(true),
        crate::wcag22::Wcag22AssessmentV1::Evaluated {
            decision: crate::wcag22::Wcag22ApplicableDecisionV1::Fail,
            ..
        } => Ok(false),
        crate::wcag22::Wcag22AssessmentV1::NotEvaluated { .. } => {
            Err(SolveFailure::InternalInvariant(
                "explicit WCAG22 evaluation returned NotEvaluated".into(),
            ))
        }
    }
}

fn solve_in_with_criterion(
    bg: &BgInput,
    contract: Contract,
    hue: Hue,
    chroma_policy: ChromaPolicy,
    vc: &ViewingConditions,
    interval: solve::LumaInterval,
    criterion: Option<Wcag22CriterionV1>,
) -> Result<Solved, SolveFailure> {
    let Some(criterion) = criterion else {
        return solve::solve_in(bg, contract, hue, chroma_policy, vc, interval);
    };
    let background_hex = crate::spaces::srgb::hex_from_srgb_encoded(bg.encoded_display());
    let accepts = |foreground_hex: &str| {
        wcag22_final_emission_passes(foreground_hex, &background_hex, criterion)
    };
    let constraint =
        solve::MonotoneFinalEmissionConstraint::toward_contrast_extreme(criterion.key(), &accepts);
    solve::solve_in_with_monotone_final_emission_constraint(
        bg,
        contract,
        hue,
        chroma_policy,
        vc,
        interval,
        constraint,
    )
}

/// Solve `contract` against `bg` under `chroma`, building the undertone the
/// policy prescribes.
///
/// For a lightness-independent policy ([`RoleChroma::Neutral`] /
/// [`RoleChroma::Tinted`]) this is a single solve at the fixed plan — the v1
/// path, unchanged. For the lightness-dependent [`RoleChroma::Curve`] it is a
/// short fixed-point: a probe solve discovers the role's contrast lightness, the
/// curve is planned *at that lightness* (cusp-attracted hue + the ratio that hits
/// the target colorfulness there), and that plan is re-solved. Because the curve
/// applies the ratio to `max_chroma` at the solve's *own* resolved lightness —
/// which can differ slightly from the lightness the plan was built for, and near
/// the white/black wall a small lightness shift moves `M'` sharply — the plan is
/// re-derived from the new lightness and re-solved until the lightness settles
/// (or a small iteration cap is hit). Every iteration is a real `solve`, so the
/// contrast contract is always honoured on the returned colour; the loop only
/// refines *which* lightness the colorfulness target is planned against.
fn solve_with_chroma(
    bg: &BgInput,
    contract: Contract,
    chroma: RoleChroma,
    vc: &ViewingConditions,
    interval: solve::LumaInterval,
    criterion: Option<Wcag22CriterionV1>,
) -> Result<Solved, SolveFailure> {
    if let RoleChroma::Curve { .. } = chroma {
        // Probe — discover the contrast-solved lightness achromatically.
        let (probe_hue, probe_chroma) = RoleChroma::probe_plan();
        let probe = solve_in_with_criterion(
            bg,
            contract,
            probe_hue,
            probe_chroma,
            vc,
            interval,
            criterion,
        )?;
        let mut l_plan = solved_oklab_lightness(&probe)?;
        let mut solved = probe;
        // Legacy-уточнение сохранено побайтно: этот узкий срез исправляет
        // валидацию входа, а не меняет алгоритм построения палитры.
        // `LIGHTNESS_SETTLE` — только ограниченная численная эвристика остановки:
        // она не доказывает неподвижную точку в sRGB8, и здесь возможен цикл из
        // двух состояний. Точная конечная замена принадлежит контракту solver-а
        // (issues #218 и #253).
        for _ in 0..CURVE_REFINE_STEPS {
            let (hue, policy) = chroma.plan_for_lightness(l_plan, vc);
            solved = solve_in_with_criterion(bg, contract, hue, policy, vc, interval, criterion)?;
            let l_new = solved_oklab_lightness(&solved)?;
            if (l_new - l_plan).abs() <= LIGHTNESS_SETTLE {
                break;
            }
            l_plan = l_new;
        }
        Ok(solved)
    } else {
        let (hue, policy) = chroma.plan_for_lightness(0.0, vc);
        solve_in_with_criterion(bg, contract, hue, policy, vc, interval, criterion)
    }
}

/// Solve a dJ' decorative role under `chroma`, applying the same undertone policy
/// every other role uses (the identity-curve v2 by default).
///
/// Mirrors [`solve_with_chroma`] but drives [`solve::solve_dj`] instead of the
/// contrast solver: the target is a perceived-lightness offset on the J' axis, not
/// an Lc contract. For the lightness-dependent [`RoleChroma::Curve`] the same short
/// fixed-point runs — an achromatic probe discovers the dJ'-solved lightness, the
/// curve is planned at that lightness, and the plan is re-solved until the
/// lightness settles. Every iteration is a real `solve_dj`, so the dJ' contract is
/// always honoured on the returned colour; the loop only refines which lightness
/// the undertone is planned against.
fn resolve_dj(
    bg: &BgInput,
    magnitude_dj: f64,
    polarity: Polarity,
    chroma: RoleChroma,
    vc: &ViewingConditions,
) -> Result<solve::DjSolved, SolveFailure> {
    let sign = polarity.sign();
    if let RoleChroma::Curve { .. } = chroma {
        let (probe_hue, probe_chroma) = RoleChroma::probe_plan();
        let probe = solve::solve_dj(bg, magnitude_dj, sign, probe_hue, probe_chroma, vc)?;
        let mut l_plan = solved_oklab_lightness(&probe.solved)?;
        let mut solved = probe;
        for _ in 0..CURVE_REFINE_STEPS {
            let (hue, policy) = chroma.plan_for_lightness(l_plan, vc);
            solved = solve::solve_dj(bg, magnitude_dj, sign, hue, policy, vc)?;
            let l_new = solved_oklab_lightness(&solved.solved)?;
            if (l_new - l_plan).abs() <= LIGHTNESS_SETTLE {
                break;
            }
            l_plan = l_new;
        }
        Ok(solved)
    } else {
        let (hue, policy) = chroma.plan_for_lightness(0.0, vc);
        solve::solve_dj(bg, magnitude_dj, sign, hue, policy, vc)
    }
}

/// Legacy-предел числа уточнений после ахроматической пробы. Он ограничивает
/// работу, но не доказывает сходимость конечного отображения эмитируемых
/// состояний (см. issues #218 и #253).
const CURVE_REFINE_STEPS: u32 = 3;

/// Legacy-эвристика остановки во float-пространстве, сохранённая ради
/// совместимости выхода. Малый сдвиг Oklab L всё ещё может пересечь байтовую
/// границу sRGB8, поэтому это значение нельзя называть сертификатом сходимости.
const LIGHTNESS_SETTLE: f64 = 0.002;

/// The Oklab lightness of a solved colour, read back from its emitted hex.
fn solved_oklab_lightness(solved: &Solved) -> Result<f64, SolveFailure> {
    use crate::spaces::oklab::srgb_linear_to_oklab;
    use crate::spaces::srgb::srgb_from_hex;
    srgb_from_hex(solved.hex())
        .map(|rgb| srgb_linear_to_oklab(rgb)[0])
        .map_err(|reason| {
            SolveFailure::InternalInvariant(format!(
                "engine emitted an invalid sRGB hex during curve refinement: {reason}"
            ))
        })
}

/// Resolve every `Role` in [`Role::ALL`] against `bg` in one sweep, in strict
/// visual-weight order (strongest text first, then decorative, then the zero
/// token). The returned pairs preserve that order, so a consumer can read the
/// hierarchy off the sequence and a serialiser emits stable output.
///
/// Polarity and maximum contrast are computed once for the whole set (see
/// [`ResolveContext`]); every role shares them. After the per-role solve a
/// hierarchy pass walks the text roles strongest-first and, where the legal floor
/// squeezed a role onto its senior, demotes it to the smallest distinguishable
/// still-legal step. If no ordered step clears the junior floor, the legal junior
/// is retained and flagged [`Resolved::compressed`]; readability remains a hard
/// constraint while hierarchy is an explicit soft outcome.
#[cfg(test)]
pub fn resolve_set(
    bg: &BgInput,
    table: &RoleTable,
    vc: &ViewingConditions,
) -> Vec<(Role, Resolved)> {
    // The former O(1) grey (`greyfast`) and chromatic-memo (`chromafast`) fast
    // paths were deleted under ADR-0001: they only ever accelerated this
    // built-in `resolve_set`, which is no longer on any production path (the
    // agnostic engine ships only the string-keyed `resolve_named_set`). A cold
    // named grey resolve was measured at ~1.7 ms (resolve-only) / ~3.1 ms
    // (compile+resolve) in release — a one-time, sub-frame cost — so the ~468 KB
    // precomputed grey table earned no keep. This built-in path survives solely as
    // the `#[cfg(test)]` byte-identity oracle for the named path, so the live
    // solve is all it needs.
    resolve_set_live(bg, table, vc)
        .expect("the cfg(test) built-in oracle must remain an admissible sRGB set")
}

/// The full solver sweep behind `resolve_set` — the built-in byte-identity
/// oracle for the named path. Always recomputes.
#[cfg(test)]
pub(crate) fn resolve_set_live(
    bg: &BgInput,
    table: &RoleTable,
    vc: &ViewingConditions,
) -> Result<Vec<(Role, Resolved)>, ResolveSetError> {
    // Memoize the CIECAM16 forward for the span of this set: viewing conditions
    // are fixed here, so the refine fixed-point and the hierarchy pass that
    // re-measure the same candidate colours hit the cache instead of recomputing
    // (25–33 % of the forwards are exact repeats). Cleared on drop.
    let _forward_cache = crate::spaces::cam16::ForwardCacheGuard::activate();
    let ctx = ResolveContext::new(bg, vc);
    let mut set = Vec::with_capacity(Role::ALL.len());
    for &role in &Role::ALL {
        let resolved = admit_resolution(resolve_in(bg, role, table, vc, &ctx))?;
        set.push((role, resolved));
    }
    enforce_text_hierarchy(&mut set, bg, table, vc, &ctx)?;
    Ok(set)
}

/// A recipe table keyed by client-authored string names, the config-layer
/// analogue of `RoleTable`. Names are opaque to colour semantics but must obey
/// the stable output identifier grammar.
///
/// Where `RoleTable` carries the fixed v1 `Role` enum, `NamedRoleTable` carries
/// whatever role *names* a consumer's [`ThemeConfig`](crate::config::ThemeConfig)
/// declares — the engine knows none of them. It is built from a config via
/// [`from_config`](crate::config::ThemeConfig::compile_named_role_table) and
/// resolved by [`resolve_named_set`]. The physics is identical to `RoleTable`'s:
/// each entry is the same [`RoleSpec`] the built-in path solves, and the same
/// [`RoleChroma`] undertone applies to the whole table.
///
/// Entries are independent opaque nodes. Declaration order never implies a
/// hierarchy or dependency; relations must arrive as explicit typed graph edges.
#[derive(Clone)]
pub struct NamedRoleTable {
    entries: Vec<(String, RoleSpec)>,
    aliases: Vec<(String, String)>,
    chroma: RoleChroma,
    output_bindings: OutputBindingSet,
    point_representation_invocations: Box<[CompiledPointRepresentationInvocationV1]>,
    material_invocations: Box<[CompiledMaterialInvocationV1]>,
    glow_invocations: Box<[CompiledGlowInvocationV1]>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CompiledPointRepresentationInvocationV1 {
    declaration_ordinal: usize,
    target: LadderTint,
    opacity_domain: crate::composition::OpacityDomainV1,
}

/// Скомпилированная invocation Material-роли: hue/tone/floor, проверенные при
/// создании таблицы. Resolver исполняет Material ТОЛЬКО через этот скомпилированный
/// путь — raw `RoleSpec::Material`-арм является typed guard (`InternalInvariant`),
/// как у AlphaAnalog (#518). Физика corridor-alpha живёт в общем модуле
/// [`crate::corridor_representation`], не в recipe-коде.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CompiledMaterialInvocationV1 {
    declaration_ordinal: usize,
    hue: Option<LadderTint>,
    tone: DjMagnitude,
    floor: Floor,
}

/// Скомпилированная invocation Glow-роли (C7e): tint/step/mode, проверенные при
/// создании таблицы. Resolver исполняет Glow ТОЛЬКО через этот скомпилированный
/// путь — raw `RoleSpec::Glow`-арм является typed guard (`InternalInvariant`),
/// как у Material (C7d) и AlphaAnalog (#518). Screen-физика живёт в
/// [`crate::field_effect`] (единственный SSOT закона), численный отбор —
/// в reference-слое [`crate::glow`] поверх него.
#[derive(Debug, Clone, Copy, PartialEq)]
struct CompiledGlowInvocationV1 {
    declaration_ordinal: usize,
    tint: LadderTint,
    step: crate::glow::GlowStep,
    mode: crate::numerical_plan::NumericalExecutionModeV1,
}

#[cfg(test)]
thread_local! {
    static POINT_REPRESENTATION_PLAN_COMPILATIONS: std::cell::Cell<usize> = const {
        std::cell::Cell::new(0)
    };
}

#[cfg(test)]
fn reset_point_representation_plan_compilation_count() {
    POINT_REPRESENTATION_PLAN_COMPILATIONS.with(|count| count.set(0));
}

#[cfg(test)]
fn point_representation_plan_compilation_count() -> usize {
    POINT_REPRESENTATION_PLAN_COMPILATIONS.with(std::cell::Cell::get)
}

impl CompiledPointRepresentationInvocationV1 {
    fn resolve(self, bg: &BgInput, vc: &ViewingConditions) -> PendingResolution {
        resolve_rgba_inverted_admitted(self.target.for_vc(vc), self.opacity_domain, bg, vc)
    }
}

impl CompiledGlowInvocationV1 {
    /// Резолв скомпилированной Glow-роли — ЕДИНСТВЕННЫЙ исполняемый путь (C7e,
    /// как Material в C7d). Raw `RoleSpec::Glow`-арм возвращает
    /// [`SolveFailure::InternalInvariant`], поэтому второй источник физики
    /// отсутствует. Screen-закон живёт в [`crate::field_effect`]; численный
    /// отбор интенсивности — reference-слой [`crate::glow`] поверх него.
    fn resolve(self, bg: &BgInput, vc: &ViewingConditions) -> PendingResolution {
        // Свечение: halo = якорь источника по теме; core — пересвет;
        // интенсивность решается под контрактную ступень на фоне резолва.
        // Typed execution mode исполняется ПРЯМО из compiled invocation:
        // никакого plan lookup или string policy selection в hot path.
        let halo_hex = crate::spaces::srgb::hex_from_srgb_encoded(self.tint.for_vc(vc));
        let bg_hex =
            crate::spaces::srgb::hex_from_srgb_encoded(quantise_encoded(bg.encoded_display()));
        // Общая сборка полного Glow-результата из решённого состояния —
        // одна для обоих атомарных законных исходов.
        let assemble = |g: &crate::glow::GlowSolve,
                        outcome: crate::glow::GlowDecisionOutcomeV1|
         -> PendingResolution {
            let (core_hex, halo_hex) = match crate::glow::glow_layers_from_source(&halo_hex, vc) {
                Ok(pair) => pair,
                Err(e) => {
                    return Err(SolveFailure::InternalInvariant(format!(
                        "generated Glow layer recipe was rejected: {e}"
                    )));
                }
            };
            let core_measurement =
                match crate::glow::measure_screen_layer_at_alpha(&core_hex, &bg_hex, g.alpha(), vc)
                {
                    Ok(measurement) => measurement,
                    Err(e) => {
                        return Err(SolveFailure::InternalInvariant(format!(
                            "generated Glow core measurement was rejected: {e}"
                        )));
                    }
                };
            Ok(Resolved::Glow(GlowResolved {
                core_hex,
                halo_hex,
                alpha: g.alpha(),
                alpha_css: g.alpha_css().to_string(),
                target_dj: g.target_dj(),
                halo_composite_hex: g.composite_hex().to_string(),
                halo_achieved_dj: g.achieved_dj(),
                core_composite_hex: core_measurement.composite_hex,
                core_achieved_dj: core_measurement.achieved_dj,
                target_status: g.status(),
                layer_recipe_profile: crate::glow::GlowLayerRecipeProfileV1::Cam16JPrimeOklabCuspV1,
                appearance_diagnostic_profile:
                    crate::glow::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1,
                selection_diagnostic_profile: g.selection_diagnostic_profile(),
                decision_outcome: outcome,
                halo_composite_certificate: g.composite_certificate().clone(),
                core_composite_certificate: core_measurement.certificate,
            }))
        };
        match crate::glow::solve_screen_alpha_for_dj(
            &halo_hex,
            &bg_hex,
            self.step.target_dj(),
            self.mode,
            vc,
        ) {
            Ok(crate::numerics::NumericalDecisionV1::Indeterminate { site_id, evidence }) => {
                Ok(Resolved::GlowIndeterminate(GlowIndeterminateResolved {
                    source_hex: halo_hex,
                    target_dj: self.step.target_dj(),
                    decision_profile: crate::glow::GlowDecisionProfileV1::from_execution_mode(
                        self.mode,
                    ),
                    site_id,
                    evidence,
                }))
            }
            Ok(crate::numerics::NumericalDecisionV1::Determinate {
                value: g, evidence, ..
            }) => assemble(
                &g,
                crate::glow::GlowDecisionOutcomeV1::StableExactNoop { evidence },
            ),
            Ok(crate::numerics::NumericalDecisionV1::Compatibility {
                value: g,
                release_id,
                provenance,
                ..
            }) => assemble(
                &g,
                crate::glow::GlowDecisionOutcomeV1::Compatibility {
                    release_id,
                    provenance,
                },
            ),
            Err(e) => Err(SolveFailure::InternalInvariant(format!(
                "generated Glow solve request was rejected: {e}"
            ))),
        }
    }
}

impl CompiledMaterialInvocationV1 {
    /// Резолв скомпилированной Material-роли — ЕДИНСТВЕННЫЙ исполняемый путь
    /// (как у AlphaAnalog). Raw `RoleSpec::Material`-арм возвращает
    /// [`SolveFailure::InternalInvariant`], поэтому второй источник физики
    /// отсутствует. Тон строится тем же dj-anchor-солвером ([`resolve_dj`]),
    /// альфа — corridor-физикой [`crate::corridor_representation`] над полным
    /// коридором `[чёрный, белый]`.
    fn resolve(
        self,
        bg: &BgInput,
        vc: &ViewingConditions,
        chroma: RoleChroma,
        polarity: Polarity,
    ) -> PendingResolution {
        use crate::spaces::srgb::hex_from_srgb_encoded;
        // Пол читаемости обязателен: у материала без пола нет цели для вывода α.
        // Гарантируется `validate_domain` при компиляции (floor != None); здесь —
        // типизированная повторная проверка без паники.
        let floor_ratio = match self.floor.min_ratio() {
            Some(ratio) => ratio,
            None => {
                return Err(SolveFailure::InternalInvariant(
                    "compiled Material invocation lost its readability floor".into(),
                ));
            }
        };
        // Тот же вывод tone_chroma из hue + policy таблицы, что был в raw-арме.
        let tone_chroma = match self.hue {
            None => chroma,
            Some(hue_tint) => match crate::spaces::oklab::hue_of_srgb8(hue_tint.srgb8_for_vc(vc)) {
                crate::spaces::oklab::OklabHue::Achromatic => RoleChroma::Neutral,
                crate::spaces::oklab::OklabHue::Chromatic { degrees: hue_deg } => match chroma {
                    RoleChroma::Curve {
                        target_mp,
                        hue_stiffness,
                        ..
                    } => RoleChroma::Curve {
                        canonical_hue_deg: hue_deg,
                        target_mp,
                        hue_stiffness,
                    },
                    RoleChroma::Tinted { ratio, .. } => RoleChroma::Tinted { hue_deg, ratio },
                    RoleChroma::Neutral => {
                        return Err(SolveFailure::InvalidInput(
                            RoleSpec::INCOMPATIBLE_CHROMA_REASON.to_owned(),
                        ));
                    }
                },
            },
        };
        // Тон-база 02: опаковая поверхность на целевом |ΔJ'|.
        let dj = resolve_dj(bg, self.tone.for_vc(vc), polarity, tone_chroma, vc)?;
        let tone_hex = dj.solved.hex().to_string();
        // α: corridor-физика над полным коридором [чёрный, белый].
        let corridor = match crate::corridor_representation::solve_corridor_alpha_hex(
            &tone_hex,
            floor_ratio,
        ) {
            Ok(result) => result,
            Err(error) => {
                return Err(SolveFailure::InternalInvariant(format!(
                    "generated Material solve request was rejected: {error}"
                )));
            }
        };
        // Различимость солид-канона (= тона) от фона резолва на 8-битной сетке.
        let bg_hex = hex_from_srgb_encoded(quantise_encoded(bg.encoded_display()));
        let distinct = tone_hex != bg_hex;
        Ok(Resolved::Material(MaterialResolved {
            tone_hex,
            alpha: corridor.alpha,
            worst_contrast: corridor.worst_contrast,
            alpha_guarantee: crate::material::MaterialAlphaGuaranteeV1::from_corridor(
                corridor.guarantee,
            ),
            alpha_status: crate::material::MaterialAlphaStatusV1::from_corridor(corridor.status),
            floor: floor_ratio,
            pole: corridor.pole,
            achieved_dj: dj.achieved_dj,
            tone_compressed: dj.degraded,
            distinct,
        }))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AlphaAnalogCompileErrorV1 {
    declaration_ordinal: usize,
    value: f64,
}

impl AlphaAnalogCompileErrorV1 {
    pub(crate) const fn declaration_ordinal(self) -> usize {
        self.declaration_ordinal
    }

    pub(crate) const fn value(self) -> f64 {
        self.value
    }
}

/// Структурная ошибка компиляции Material-invocation: роли без читаемостного
/// пола нельзя превратить в скомпилированный путь (нет цели для вывода α).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct MaterialCompileErrorV1 {
    declaration_ordinal: usize,
}

impl MaterialCompileErrorV1 {
    pub(crate) const fn declaration_ordinal(self) -> usize {
        self.declaration_ordinal
    }
}

/// Structural failure while materialising a validated named table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum NamedRoleTableCompileError {
    /// Static namespace is structurally invalid.
    OutputBindings(OutputBindingCompileError),
    /// A compiled alpha-analog invocation is outside its numeric domain.
    AlphaAnalog(AlphaAnalogCompileErrorV1),
    /// A compiled Material invocation lost its readability floor.
    Material(MaterialCompileErrorV1),
}

impl core::fmt::Debug for NamedRoleTable {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("NamedRoleTable")
            .field("entries", &self.entries)
            .field("aliases", &self.aliases)
            .field("chroma", &self.chroma)
            .field("output_bindings", &self.output_bindings)
            .finish()
    }
}

impl PartialEq for NamedRoleTable {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
            && self.aliases == other.aliases
            && self.chroma == other.chroma
            && self.output_bindings == other.output_bindings
    }
}

impl RoleSpec {
    /// Static output shape reserved by this executable recipe.
    pub(crate) const fn output_binding_shape(&self) -> OutputBindingShape {
        match self {
            Self::Glow { .. } => OutputBindingShape::Glow,
            Self::Material { .. } => OutputBindingShape::Material,
            Self::Anchor(_)
            | Self::DecorativeDj { .. }
            | Self::Decorative { .. }
            | Self::Ladder { .. }
            | Self::AlphaAnalog { .. }
            | Self::Zero => OutputBindingShape::Primary,
        }
    }

    /// Validate every raw numeric field before a spec can enter an executable
    /// named table. Typed payloads (`TextAnchor`, `LadderTint`, closed enums)
    /// already enforce their own domains; this guard owns the remaining public
    /// scalar seams in one place.
    pub(crate) fn validate_domain(self) -> Result<(), String> {
        let positive = |field: &str, value: f64| {
            if value.is_finite() && value > 0.0 {
                Ok(())
            } else {
                Err(format!(
                    "{field} must be finite and greater than zero, got {value}"
                ))
            }
        };
        let alpha = |field: &str, value: f64| {
            if role_alpha_valid(value) {
                Ok(())
            } else {
                Err(format!(
                    "{field} must be finite and inside (0, 1], got {value}"
                ))
            }
        };

        match self {
            RoleSpec::Anchor(_) | RoleSpec::Glow { .. } | RoleSpec::Zero => Ok(()),
            RoleSpec::DecorativeDj { magnitude_dj } => {
                positive("decorative dJ light magnitude", magnitude_dj.light())?;
                positive("decorative dJ dark magnitude", magnitude_dj.dark())
            }
            RoleSpec::Decorative { magnitude } => {
                if magnitude.is_finite() && magnitude >= DECORATIVE_FLOOR_MIN {
                    Ok(())
                } else {
                    Err(format!(
                        "decorative Lc magnitude must be finite and at least {DECORATIVE_FLOOR_MIN}, got {magnitude}"
                    ))
                }
            }
            RoleSpec::Ladder {
                alpha_light,
                alpha_dark,
                floor,
                ..
            } => {
                alpha("ladder light alpha", alpha_light)?;
                alpha("ladder dark alpha", alpha_dark)?;
                validate_ladder_floor(alpha_light, alpha_dark, floor).map_err(str::to_owned)
            }
            RoleSpec::AlphaAnalog { alpha: value, .. } => alpha("alpha-analog alpha", value),
            RoleSpec::Material { tone, floor, .. } => {
                positive("material light tone", tone.light())?;
                positive("material dark tone", tone.dark())?;
                if matches!(floor, Floor::None) {
                    return Err("material requires a readability floor".into());
                }
                Ok(())
            }
        }
    }

    /// Каноническая причина единственного конфликта рецепта с chroma-policy.
    pub(crate) const INCOMPATIBLE_CHROMA_REASON: &'static str =
        "chromatic source needs curve/tinted policy";

    /// Проверить и собственный домен рецепта, и его связь с chroma-policy.
    ///
    /// Публичный конструктор таблицы принимает сырые `RoleSpec`, поэтому обязан
    /// выполнить оба слоя проверки до появления исполняемого состояния.
    pub(crate) fn validate_with_chroma(self, chroma: RoleChroma) -> Result<(), String> {
        self.validate_domain()?;
        if !self.is_chroma_compatible(chroma) {
            return Err(Self::INCOMPATIBLE_CHROMA_REASON.to_owned());
        }
        Ok(())
    }

    /// Проверить только меж-полевой контракт после отдельной domain-validation.
    ///
    /// Хроматический material-источник требует политики, способной сохранить
    /// направление; ахроматический источник допустим и при neutral-policy.
    /// Две validation-границы используют один предикат. Явный inline удерживает
    /// его без отдельного тела в канонической WASM-сборке; backend-дрифт ловит
    /// исполняемый size-ратчет, а не это пояснение.
    #[inline(always)]
    pub(crate) fn is_chroma_compatible(&self, chroma: RoleChroma) -> bool {
        let material_has_chromatic_source = match self {
            RoleSpec::Material {
                hue: Some(source), ..
            } => !source.all_modes_achromatic(),
            _ => false,
        };
        !matches!(chroma, RoleChroma::Neutral) || !material_has_chromatic_source
    }

    /// WCAG-пол этой спеки — свойство контракта, не резолва: текст/UI-якорь
    /// несёт пол своего [`TextAnchor`] (AaText → 4.5, AaUi → 3.0), все
    /// остальные формы (декоративные, dJ', лестница, альфа-аналог, zero) —
    /// без легального пола. Одна семантика для обеих таблиц
    /// (`RoleTable::legal_floor` и string-keyed границы).
    pub fn legal_floor(&self) -> Option<f64> {
        match self {
            RoleSpec::Anchor(anchor) => anchor.conformance().min_ratio(),
            _ => None,
        }
    }
}

impl NamedRoleTable {
    /// Build a named table from its `(name, recipe)` entries and an undertone
    /// policy. Names are the CSS contract downstream (`--lab-{name}`), so this
    /// boundary validates the same non-empty `[a-z0-9-]+` grammar as config and
    /// rejects the complete role/alias satellite namespace on any collision.
    /// Все численные домены `RoleSpec` и `chroma` проверяются здесь, потому что
    /// таблицу можно собрать без конфига. Поэтому executable-таблица не может
    /// содержать отложенное невалидное состояние, зависящее от темы или ветки.
    ///
    /// # Errors
    ///
    /// [`SolveFailure::InvalidInput`], если любой численный параметр роли или
    /// `chroma` не конечен либо выходит из своего физического домена, а также
    /// при структурной несовместимости спеки с политикой хромы (например,
    /// `Material { hue: Some(_) }` при `RoleChroma::Neutral`), невалидном имени,
    /// неизвестной alias-цели или коллизии exact output binding.
    pub fn new(
        entries: Vec<(String, RoleSpec)>,
        aliases: Vec<(String, String)>,
        chroma: RoleChroma,
    ) -> Result<Self, SolveFailure> {
        chroma.validate()?;
        for (name, spec) in &entries {
            spec.validate_with_chroma(chroma)
                .map_err(|message| SolveFailure::InvalidInput(format!("role {name}: {message}")))?;
        }
        let point_representation_invocations =
            Self::compile_point_representation_invocations(&entries).map_err(|error| {
                SolveFailure::InvalidInput(format!(
                    "role {}: alpha-analog alpha must be finite and inside (0, 1], got {}",
                    entries[error.declaration_ordinal].0, error.value
                ))
            })?;
        let material_invocations =
            Self::compile_material_invocations(&entries).map_err(|error| {
                SolveFailure::InvalidInput(format!(
                    "role {}: material requires a readability floor",
                    entries[error.declaration_ordinal].0
                ))
            })?;
        let glow_invocations = Self::compile_glow_invocations(&entries);
        let output_bindings = Self::compile_output_bindings(&entries, &aliases)
            .map_err(|error| SolveFailure::InvalidInput(error.to_string()))?;
        Ok(Self::from_compiled_parts(
            entries,
            aliases,
            chroma,
            output_bindings,
            point_representation_invocations,
            material_invocations,
            glow_invocations,
        ))
    }

    /// Собирает таблицу из частей, уже проверенных конфигом. Output manifest
    /// всегда компилируется здесь из тех же owned entries/aliases, поэтому даже
    /// crate-private caller не может передать несогласованный manifest отдельно.
    pub(crate) fn from_validated_parts(
        entries: Vec<(String, RoleSpec)>,
        aliases: Vec<(String, String)>,
        chroma: RoleChroma,
    ) -> Result<Self, NamedRoleTableCompileError> {
        let output_bindings = Self::compile_output_bindings(&entries, &aliases)
            .map_err(NamedRoleTableCompileError::OutputBindings)?;
        let point_representation_invocations =
            Self::compile_point_representation_invocations(&entries)
                .map_err(NamedRoleTableCompileError::AlphaAnalog)?;
        let material_invocations = Self::compile_material_invocations(&entries)
            .map_err(NamedRoleTableCompileError::Material)?;
        let glow_invocations = Self::compile_glow_invocations(&entries);
        Ok(Self::from_compiled_parts(
            entries,
            aliases,
            chroma,
            output_bindings,
            point_representation_invocations,
            material_invocations,
            glow_invocations,
        ))
    }

    fn compile_output_bindings(
        entries: &[(String, RoleSpec)],
        aliases: &[(String, String)],
    ) -> Result<OutputBindingSet, OutputBindingCompileError> {
        OutputBindingSet::compile(
            entries
                .iter()
                .map(|(name, spec)| (name.as_str(), spec.output_binding_shape())),
            aliases
                .iter()
                .map(|(alias, target)| (alias.as_str(), target.as_str())),
        )
    }

    fn from_compiled_parts(
        entries: Vec<(String, RoleSpec)>,
        aliases: Vec<(String, String)>,
        chroma: RoleChroma,
        output_bindings: OutputBindingSet,
        point_representation_invocations: Box<[CompiledPointRepresentationInvocationV1]>,
        material_invocations: Box<[CompiledMaterialInvocationV1]>,
        glow_invocations: Box<[CompiledGlowInvocationV1]>,
    ) -> Self {
        Self {
            entries,
            aliases,
            chroma,
            output_bindings,
            point_representation_invocations,
            material_invocations,
            glow_invocations,
        }
    }

    /// Скомпилировать каждую Glow-роль в приватный compiled invocation (C7e).
    ///
    /// Исполнение Glow идёт ТОЛЬКО через этот slice (см. [`resolve_named_set`]);
    /// raw `RoleSpec::Glow`-арм возвращает [`SolveFailure::InternalInvariant`].
    /// Все поля уже typed и проверены на конфиг-границе, поэтому компиляция не
    /// имеет отказного домена, а порядок ordinals возрастает по построению.
    fn compile_glow_invocations(entries: &[(String, RoleSpec)]) -> Box<[CompiledGlowInvocationV1]> {
        let mut invocations = Vec::new();
        for (declaration_ordinal, (_, spec)) in entries.iter().enumerate() {
            let RoleSpec::Glow { tint, step, mode } = *spec else {
                continue;
            };
            invocations.push(CompiledGlowInvocationV1 {
                declaration_ordinal,
                tint,
                step,
                mode,
            });
        }
        debug_assert!(
            invocations
                .windows(2)
                .all(|pair| pair[0].declaration_ordinal < pair[1].declaration_ordinal)
        );
        debug_assert!(
            invocations
                .iter()
                .all(|invocation| invocation.declaration_ordinal < entries.len())
        );
        invocations.into_boxed_slice()
    }

    /// Скомпилировать каждую Material-роль в приватный compiled invocation.
    ///
    /// Исполнение Material идёт ТОЛЬКО через этот slice (см.
    /// [`resolve_named_set`]); raw `RoleSpec::Material`-арм возвращает
    /// [`SolveFailure::InternalInvariant`]. Как и у AlphaAnalog, отсутствие
    /// invocation не даёт resolver-у права вернуться к recipe-исполнению.
    fn compile_material_invocations(
        entries: &[(String, RoleSpec)],
    ) -> Result<Box<[CompiledMaterialInvocationV1]>, MaterialCompileErrorV1> {
        let mut invocations = Vec::new();
        for (declaration_ordinal, (_, spec)) in entries.iter().enumerate() {
            let RoleSpec::Material { hue, tone, floor } = *spec else {
                continue;
            };
            // Fail-closed: Material без читаемостного пола не имеет цели для
            // вывода α. `validate_domain` отвергает `Floor::None` на публичной
            // границе; эта typed-проверка — второй гейт для `from_validated_parts`,
            // который принимает части без domain-валидации.
            if floor.min_ratio().is_none() {
                return Err(MaterialCompileErrorV1 {
                    declaration_ordinal,
                });
            }
            invocations.push(CompiledMaterialInvocationV1 {
                declaration_ordinal,
                hue,
                tone,
                floor,
            });
        }
        debug_assert!(
            invocations
                .windows(2)
                .all(|pair| pair[0].declaration_ordinal < pair[1].declaration_ordinal)
        );
        debug_assert!(
            invocations
                .iter()
                .all(|invocation| invocation.declaration_ordinal < entries.len())
        );
        Ok(invocations.into_boxed_slice())
    }

    fn compile_point_representation_invocations(
        entries: &[(String, RoleSpec)],
    ) -> Result<Box<[CompiledPointRepresentationInvocationV1]>, AlphaAnalogCompileErrorV1> {
        #[cfg(test)]
        POINT_REPRESENTATION_PLAN_COMPILATIONS.with(|count| count.set(count.get() + 1));
        let mut invocations = Vec::new();
        for (declaration_ordinal, (_, spec)) in entries.iter().enumerate() {
            let RoleSpec::AlphaAnalog { of, alpha } = *spec else {
                continue;
            };
            let opacity_domain = crate::composition::OpacityDomainV1::try_new(alpha, 1.0)
                .ok()
                .filter(|domain| domain.lower().value() > 0.0)
                .ok_or(AlphaAnalogCompileErrorV1 {
                    declaration_ordinal,
                    value: alpha,
                })?;
            invocations.push(CompiledPointRepresentationInvocationV1 {
                declaration_ordinal,
                target: of,
                opacity_domain,
            });
        }
        debug_assert!(
            invocations
                .windows(2)
                .all(|pair| pair[0].declaration_ordinal < pair[1].declaration_ordinal)
        );
        debug_assert!(
            invocations
                .iter()
                .all(|invocation| invocation.declaration_ordinal < entries.len())
        );
        Ok(invocations.into_boxed_slice())
    }

    /// Алиасы `(имя, цель)` сохраняются в скомпилированном контракте, чтобы
    /// delivery boundary спроецировала resolved outcome цели под client-owned
    /// именем алиаса. Алиас не запускает отдельный solve и не меняет физическое
    /// значение; без переноса сюда алиасные роли терялись бы при компиляции.
    pub fn aliases(&self) -> &[(String, String)] {
        &self.aliases
    }

    /// Статический exact output contract всей таблицы.
    ///
    /// Содержит primary keys, recipe satellites и полный shape aliases в
    /// declaration order. Он скомпилирован и проверен на коллизии до появления
    /// executable table; resolve не выводит ownership из динамического outcome.
    pub fn output_bindings(&self) -> &OutputBindingSet {
        &self.output_bindings
    }

    /// The `(name, recipe)` entries, in declaration order.
    pub fn entries(&self) -> &[(String, RoleSpec)] {
        &self.entries
    }

    /// The undertone policy applied to every role in this table.
    pub fn chroma(&self) -> RoleChroma {
        self.chroma
    }

    /// Canonical numerical execution plan таблицы (#292) — DERIVED-проекция
    /// тех же деклараций `entries()`, а НЕ второй mutable map: единственный
    /// источник mode — сама спека [`RoleSpec::Glow`], план лишь перечисляет её
    /// compiled invocations. Проекция не участвует в resolve/frame path —
    /// resolver исполняет typed mode, хранящийся в спеке, и не делает plan
    /// lookup в hot path; план существует для boundary/manifest диагностики.
    ///
    /// Occurrences подаются в порядке деклараций (имя роли — opaque node
    /// bytes, site — [`GlowTargetOrMaximumV1`](crate::numerics::NumericalSiteIdV1)),
    /// поэтому локальные ordinals внутри пары `(node, site)` детерминированы;
    /// canonical-сортировка — внутренний закон самого плана и порядок
    /// `entries()` не переупорядочивает.
    ///
    /// # Errors
    ///
    /// Типизированная [`NumericalPlanErrorV1`](crate::numerical_plan::NumericalPlanErrorV1)
    /// компиляции плана (незарегистрированный site/release) — fail closed, не
    /// runtime fallback.
    pub fn numerical_plan_v1(
        &self,
    ) -> Result<
        crate::numerical_plan::CompiledNumericalPlanV1,
        crate::numerical_plan::NumericalPlanErrorV1,
    > {
        crate::numerical_plan::compile_numerical_plan_v1(self.entries.iter().filter_map(
            |(name, spec)| match spec {
                RoleSpec::Glow { mode, .. } => Some((
                    name.as_bytes(),
                    crate::numerics::NumericalSiteIdV1::GlowTargetOrMaximumV1,
                    *mode,
                )),
                _ => None,
            },
        ))
    }
}

/// Решить каждую клиентски именованную роль `table` против `bg` при `vc`.
///
/// `Ok` сохраняет порядок объявления и может нести только допущенные
/// локальные `unreachable | unresolved`. Rejected-вход, неподдержанная capability
/// or internal drift returns [`ResolveSetError`] for the entire call; no partial
/// вектор не наблюдаем. В отсутствие объявленных graph edges роли не влияют на
/// физический результат соседних деклараций.
pub fn resolve_named_set(
    bg: &BgInput,
    table: &NamedRoleTable,
    vc: &ViewingConditions,
) -> Result<Vec<(String, Resolved)>, ResolveSetError> {
    // One CIECAM16 forward-cache for the span of this sweep, mirroring
    // `resolve_set_live`: the curve refine fixed-point and repeated lightnesses
    // across roles hit the cache instead of recomputing.
    let _forward_cache = crate::spaces::cam16::ForwardCacheGuard::activate();
    let ctx = ResolveContext::new(bg, vc);
    let mut set = Vec::with_capacity(table.entries.len());
    let mut point_invocations = table
        .point_representation_invocations
        .iter()
        .copied()
        .peekable();
    let mut material_invocations = table.material_invocations.iter().copied().peekable();
    let mut glow_invocations = table.glow_invocations.iter().copied().peekable();
    for (declaration_ordinal, (name, spec)) in table.entries.iter().enumerate() {
        // Material intercept происходит ДО recipe-dispatch: скомпилированная
        // invocation исполняется напрямую, raw `RoleSpec::Material`-арм для
        // этого ordinal недостижим (как у AlphaAnalog). Дрейф порядка в любую
        // сторону — целостный InternalInvariant, без fallback.
        let pending = match material_invocations.peek().copied() {
            Some(invocation) if invocation.declaration_ordinal < declaration_ordinal => {
                return Err(ResolveSetError {
                    state: ResolveSetErrorState::Internal(SolveFailure::InternalInvariant(
                        "compiled material invocation order drifted behind declarations".into(),
                    )),
                });
            }
            Some(invocation) if invocation.declaration_ordinal == declaration_ordinal => {
                material_invocations.next();
                invocation.resolve(bg, vc, table.chroma, ctx.polarity)
            }
            _ => match point_invocations.peek().copied() {
                Some(invocation) if invocation.declaration_ordinal < declaration_ordinal => {
                    return Err(ResolveSetError {
                        state: ResolveSetErrorState::Internal(SolveFailure::InternalInvariant(
                            "compiled point-representation invocation order drifted behind declarations"
                                .into(),
                        )),
                    });
                }
                Some(invocation) if invocation.declaration_ordinal == declaration_ordinal => {
                    point_invocations.next();
                    invocation.resolve(bg, vc)
                }
                _ => match glow_invocations.peek().copied() {
                    Some(invocation) if invocation.declaration_ordinal < declaration_ordinal => {
                        return Err(ResolveSetError {
                            state: ResolveSetErrorState::Internal(SolveFailure::InternalInvariant(
                                "compiled glow invocation order drifted behind declarations".into(),
                            )),
                        });
                    }
                    Some(invocation) if invocation.declaration_ordinal == declaration_ordinal => {
                        glow_invocations.next();
                        invocation.resolve(bg, vc)
                    }
                    _ => resolve_spec_in(bg, spec, table.chroma, vc, &ctx),
                },
            },
        };
        let resolved = admit_resolution(pending)?;
        set.push((name.clone(), resolved));
    }
    if point_invocations.next().is_some()
        || material_invocations.next().is_some()
        || glow_invocations.next().is_some()
    {
        return Err(ResolveSetError {
            state: ResolveSetErrorState::Internal(SolveFailure::InternalInvariant(
                "compiled invocation points outside declarations".into(),
            )),
        });
    }
    Ok(set)
}

/// Measure the frozen candidate `Lc` score and WCAG 2.1 ratio a foreground
/// colour achieves against a background — the cheap **recheck** primitive.
///
/// Both colours are **linear** sRGB; the result is `(lc, wcag_ratio)`. С
/// активации ADR-0003 (глава #64) замер полностью display-доменный — ни
/// одного CAM16-форварда, только WCAG-арифметика — **no solve**. The reactive
/// runtime calls this per frame to decide whether already-resolved colours still
/// pass their contract against a *changed* background, re-solving (and easing)
/// only when they stably do not, instead of re-solving every frame.
///
/// The returned `lc` is **signed** (its sign is the achieved polarity, matching
/// [`Resolved::lc`]), and it is exactly what the solver's `finish` stage measures
/// for the same pair. The ratio is the frozen boundary report projection from
/// the same final bytes; it is not stored in or consumed by [`Solved`].
pub fn measure_contrast(
    bg_linear: [f64; 3],
    fg_linear: [f64; 3],
    _vc: &ViewingConditions,
) -> (f64, f64) {
    // Candidate `Lc` и легальный WCAG читают ОДНУ люминансу
    // квантованного display-цвета (candidate score в `Ys`, ADR-0003), exactly as
    // the solver measures it (`finish` → `quantised_display`), so the recheck
    // reproduces the solver's reported `lc` and the boundary ratio projection
    // bit-for-bit from one emitted state.
    let fg_disp = crate::solve::quantised_display(fg_linear);
    let bg_disp = crate::solve::quantised_display(bg_linear);
    let lc = crate::lpc::contrast_core(
        crate::spaces::srgb::encoded_srgb_relative_luminance(fg_disp),
        crate::spaces::srgb::encoded_srgb_relative_luminance(bg_disp),
    );
    let wcag = crate::spaces::srgb::encoded_srgb_contrast_ratio(fg_disp, bg_disp);
    (lc, wcag)
}

/// Batch recheck: the `(lc, wcag_ratio)` each foreground hex achieves against one
/// **shared** background hex, under `vc`. The per-frame primitive the reactive
/// runtime calls.
///
/// The background's luminance is computed **once** for the whole batch. С
/// активации ADR-0003 форвард цвета — это ОДНА `relative_luminance` его
/// display-байтов (ни одного CAM16), so "recheck every role each frame" is
/// cheaper still than "re-solve every role each frame": the controller keeps
/// the current colours while they still pass and only re-solves the rare role
/// that stably fails.
///
/// Each result equals what the solver's `finish` measured for that fg/bg pair, so
/// a freshly-resolved set re-checks to its own reported contrasts. Returns `Err`
/// if any hex is invalid (only `#RRGGBB` or bare `RRGGBB` is accepted).
/// One colour's recheck ingredient from its hex: the WCAG relative luminance
/// `rl` of its display bytes — с активации ADR-0003 candidate `Lc` и
/// легальный WCAG читают ОДНУ и ту же люминансу, бывшая пара `(y_hk, rl)`
/// схлопнулась в один скаляр, а recheck стал VC-независимым (display-домен).
///
/// SINGLE SOURCE OF TRUTH for the forward, shared by [`recheck_against`] and
/// [`recheck_against_multi`] so they cannot drift — the byte-identity both
/// functions promise now holds *by construction*, not by two copies staying in
/// sync. The hot-path economy lives here: the WCAG display value is taken
/// straight from the byte (`byte/255`) by `srgb_encoded_from_hex`, so the
/// per-channel `quantised_display` encode `powf` is gone —
/// `byte/255 == quantised_display(decode(byte))` exactly (pinned in
/// `spaces::srgb::display_equals_quantised_display_on_every_byte`).
fn hex_forward(hex: &str) -> Result<f64, String> {
    let disp = crate::spaces::srgb::srgb_encoded_from_hex(hex)?;
    Ok(crate::spaces::srgb::encoded_srgb_relative_luminance(disp))
}

pub fn recheck_against(
    bg_hex: &str,
    fg_hexes: &[&str],
    _vc: &ViewingConditions,
) -> Result<Vec<(f64, f64)>, String> {
    // The background's forward is loop-invariant — computed once. Один скаляр
    // на цвет: та же люминанса кормит и `contrast_core`, и WCAG-ратио.
    let rl_bg = hex_forward(bg_hex)?;
    fg_hexes
        .iter()
        .map(|fg_hex| {
            let rl_fg = hex_forward(fg_hex)?;
            let lc = crate::lpc::contrast_core(rl_fg, rl_bg);
            let wcag = crate::spaces::srgb::relative_luminance_ratio(rl_fg, rl_bg);
            Ok((lc, wcag))
        })
        .collect()
}

/// Compute the complete flat result cardinality before the first forward or
/// metric call. This is the arithmetic/allocator safety floor; the lower product
/// limit admitted by the versioned public resource profile remains owned by
/// #429 and must not be invented here.
fn checked_recheck_output_len(backgrounds: usize, foregrounds: usize) -> Result<usize, String> {
    backgrounds
        .checked_mul(foregrounds)
        .and_then(|cells| cells.checked_mul(2))
        .ok_or_else(|| "recheck batch cardinality exceeds platform capacity".to_owned())
}

fn reserve_recheck_entries<T>(
    values: &mut Vec<T>,
    entries: usize,
    buffer: &'static str,
) -> Result<(), String> {
    values.try_reserve_exact(entries).map_err(|_| {
        format!("recheck batch resource exhausted while reserving {buffer} ({entries} entries)")
    })
}

/// Multi-background recheck: the `(lc, wcag_ratio)` each foreground achieves
/// against EACH of several background samples, sharing every foreground's
/// forward across all samples. The reactive controller's worst-case loop
/// rechecks the SAME foreground set against N backdrop samples (a gradient /
/// image); each foreground's `rl_fg` is computed ONCE and reused for every
/// sample — с активации ADR-0003 форвард подешевел до одной
/// `relative_luminance` display-байтов (CAM16 не входит в score), но
/// хойстинг сохранён: он несёт контракт byte-identity двух входов, не только
/// экономию.
///
/// The result is **byte-identical**, pair for pair, to calling [`recheck_against`]
/// once per background: the same float operations run in the same order, only the
/// loop nesting is inverted so the foreground forward is hoisted. Layout is flat
/// and background-major: entry `bg s`, foreground `i` is at
/// `out[(s*fg_hexes.len() + i) * 2 + {0:lc, 1:wcag}]`. Returns `Err` on any
/// invalid hex.
pub fn recheck_against_multi(
    bg_hexes: &[&str],
    fg_hexes: &[&str],
    _vc: &ViewingConditions,
) -> Result<Vec<f64>, String> {
    // Preflight the complete matrix and both allocations before the first
    // display-forward. Overflow/allocator refusal is atomic: no partial evidence.
    let output_len = checked_recheck_output_len(bg_hexes.len(), fg_hexes.len())?;
    let mut fg_pre = Vec::new();
    reserve_recheck_entries(&mut fg_pre, fg_hexes.len(), "foreground forwards")?;
    let mut out = Vec::new();
    reserve_recheck_entries(&mut out, output_len, "result lanes")?;

    // Precompute each foreground's background-independent forward exactly once,
    // through the SAME `hex_forward` `recheck_against` uses — so the shared-forward
    // path guarantees byte-identity between the two entry points by construction.
    for fg_hex in fg_hexes {
        fg_pre.push(hex_forward(fg_hex)?);
    }

    for bg_hex in bg_hexes {
        let rl_bg = hex_forward(bg_hex)?;
        for &rl_fg in &fg_pre {
            out.push(crate::lpc::contrast_core(rl_fg, rl_bg));
            out.push(crate::spaces::srgb::relative_luminance_ratio(rl_fg, rl_bg));
        }
    }
    Ok(out)
}

/// Decode a packed `0x00RRGGBB` colour to its three exact encoded-sRGB8 bytes
/// `[R, G, B]` via pure shifts — the same octets `hex_bytes("#RRGGBB")` yields.
///
/// The high byte is **reserved and required-zero**: `0x00RRGGBB` is the only
/// legal shape, so `0xAARRGGBB` (an RGBA/ARGB word leaking in) is rejected up
/// front by a single mask instead of being silently truncated. This is the
/// cheap validation the packed boundary performs once, with no allocation.
fn bytes_from_u32(packed: u32) -> Result<[u8; 3], String> {
    if packed >> 24 != 0 {
        return Err(format!(
            "expected packed 0x00RRGGBB with a zero high byte, got {packed:#010X}"
        ));
    }
    Ok([
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    ])
}

/// One colour's recheck ingredient from its packed `0x00RRGGBB` word: the WCAG
/// relative luminance of its display bytes — the packed sibling of
/// [`hex_forward`].
///
/// Byte-identity to the hex path holds **by construction, not by a second
/// copy**: `bytes_from_u32(0x00RRGGBB)` returns exactly the `[R, G, B]` octets
/// `hex_bytes("#RRGGBB")` returns, both are lifted through the SAME
/// [`Srgb8::encoded`] projection, and the SAME [`crate::spaces::srgb::encoded_srgb_relative_luminance`]
/// SSOT reads them. The metric is never forked — a packed input and its hex
/// spelling cannot drift.
fn u32_forward(packed: u32) -> Result<f64, String> {
    let disp = Srgb8::new(bytes_from_u32(packed)?).encoded();
    Ok(crate::spaces::srgb::encoded_srgb_relative_luminance(disp))
}

/// Packed sibling of [`recheck_against`]: the `(lc, wcag_ratio)` each foreground
/// `0x00RRGGBB` word achieves against one shared background word, under `vc`.
///
/// This is byte-identical, pair for pair, to spelling the same colours as hex
/// and calling [`recheck_against`]: it hoists the background forward once and
/// feeds the SAME `crate::lpc::contrast_core` / `crate::spaces::srgb::relative_luminance_ratio`
/// in the SAME order — only the transport (a `u32` shift-decode instead of a
/// hex parse) differs. Returns `Err` if any word carries a non-zero high byte.
pub fn recheck_against_u32(
    bg: u32,
    fgs: &[u32],
    _vc: &ViewingConditions,
) -> Result<Vec<(f64, f64)>, String> {
    let rl_bg = u32_forward(bg)?;
    fgs.iter()
        .map(|&fg| {
            let rl_fg = u32_forward(fg)?;
            let lc = crate::lpc::contrast_core(rl_fg, rl_bg);
            let wcag = crate::spaces::srgb::relative_luminance_ratio(rl_fg, rl_bg);
            Ok((lc, wcag))
        })
        .collect()
}

/// Packed sibling of [`recheck_against_multi`]: the `(lc, wcag_ratio)` each
/// foreground `0x00RRGGBB` word achieves against EACH of several background
/// words, sharing every foreground's forward across all samples.
///
/// The result is **byte-identical**, entry for entry, to calling
/// [`recheck_against_multi`] with the same colours spelled as hex — same float
/// operations, same order, same background-major flat layout: entry `bg s`,
/// foreground `i` at `out[(s*fgs.len() + i) * 2 + {0:lc, 1:wcag}]`. Returns
/// `Err` on any word with a non-zero high byte.
pub fn recheck_against_multi_u32(
    bgs: &[u32],
    fgs: &[u32],
    _vc: &ViewingConditions,
) -> Result<Vec<f64>, String> {
    // Same atomic preflight as the string sibling. Keeping both transports on
    // this helper makes the overflow/resource law one SSOT.
    let output_len = checked_recheck_output_len(bgs.len(), fgs.len())?;
    let mut fg_pre = Vec::new();
    reserve_recheck_entries(&mut fg_pre, fgs.len(), "foreground forwards")?;
    let mut out = Vec::new();
    reserve_recheck_entries(&mut out, output_len, "result lanes")?;
    for &fg in fgs {
        fg_pre.push(u32_forward(fg)?);
    }

    for &bg in bgs {
        let rl_bg = u32_forward(bg)?;
        for &rl_fg in &fg_pre {
            out.push(crate::lpc::contrast_core(rl_fg, rl_bg));
            out.push(crate::spaces::srgb::relative_luminance_ratio(rl_fg, rl_bg));
        }
    }
    Ok(out)
}

/// Resolve the terminal hierarchy conflict without weakening a hard floor.
///
/// Copying the senior is valid only when that exact emitted colour also clears
/// the junior's floor. Otherwise the junior's already-legal solve is retained and
/// merely marked non-exact: hierarchy is a soft relation, readability is not.
#[cfg(test)]
fn hierarchy_fallback(
    senior: Option<Solved>,
    junior: &Resolved,
    bg: &BgInput,
    junior_floor: Floor,
) -> Result<Resolved, SolveFailure> {
    let retain_junior = || match junior {
        Resolved::Color {
            solved,
            achieved_dj,
            ..
        } => Resolved::Color {
            solved: solved.clone(),
            compressed: true,
            achieved_dj: *achieved_dj,
        },
        other => other.clone(),
    };
    let Some(senior_solved) = senior else {
        return Ok(retain_junior());
    };

    let senior_clears_junior_floor = match junior_floor.criterion() {
        Some(criterion) => {
            let background_hex = crate::spaces::srgb::hex_from_srgb_encoded(bg.encoded_display());
            wcag22_final_emission_passes(senior_solved.hex(), &background_hex, criterion)?
        }
        None => true,
    };
    if !senior_clears_junior_floor {
        return Ok(retain_junior());
    }

    Ok(match junior {
        Resolved::Color { .. } => Resolved::Color {
            solved: senior_solved,
            compressed: true,
            achieved_dj: Option::None,
        },
        other => other.clone(),
    })
}

/// Walk the text roles strongest-first and keep the order non-strict but honest.
///
/// The anchor principle already orders the *targets* strictly, but the legal
/// floor can lift two adjacent roles onto the same colour where the readable
/// window is narrower than the hierarchy steps (a near-AA mid-grey). For each
/// junior text role that did not come out strictly weaker than the senior above
/// it, try to demote it by the smallest number of quantisation steps that makes
/// it strictly weaker *while it still clears its own WCAG floor*. If none does,
/// copy the senior only when that colour also clears the junior floor; otherwise
/// retain the legal junior. Either non-exact outcome is flagged
/// [`Resolved::compressed`] so the conflict is visible, not silent.
#[cfg(test)]
fn enforce_text_hierarchy(
    set: &mut [(Role, Resolved)],
    bg: &BgInput,
    table: &RoleTable,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) -> Result<(), ResolveSetError> {
    let chroma = table.chroma();

    // Strongest-first text order; each junior is compared against its senior.
    for window in TEXT_HIERARCHY.windows(2) {
        let [senior_role, junior_role] = [window[0], window[1]];
        let Some(senior_mag) = solved_magnitude(set, senior_role) else {
            continue; // senior unreachable — nothing to compress against
        };
        let Some(junior_mag) = solved_magnitude(set, junior_role) else {
            continue; // junior unreachable — surfaced honestly already
        };
        if junior_mag + STRICT_STEP <= senior_mag {
            continue; // strictly weaker already — hierarchy holds here
        }

        // The floor squeezed this junior onto (or above) its senior. The junior's
        // own conformance governs how far down it may move and still be legal.
        let floor = match table.spec(junior_role) {
            RoleSpec::Anchor(a) => a.conformance(),
            _ => Floor::None,
        };
        let demoted = demote_below(senior_mag, ctx, chroma, floor, bg, vc);
        // Copying the senior can restore `senior ≥ junior`, but only if that
        // exact colour also clears the junior's floor. This test-only built-in
        // oracle owns that precedence law; named roles infer no hierarchy.
        let senior_solved = set.iter().find_map(|(r, res)| match res {
            Resolved::Color { solved, .. } if *r == senior_role => Some(solved.clone()),
            _ => None,
        });
        let Some(entry) = set.iter_mut().find(|(r, _)| *r == junior_role) else {
            continue;
        };
        entry.1 = match (demoted, senior_solved, &entry.1) {
            // A distinguishable, still-legal step below the senior.
            // achieved_dj сбрасывается: цвет заменён сжатием, прежний замер
            // ему не принадлежит (честнее None, чем чужое число).
            (Ok(Some(solved)), _, _) => Resolved::Color {
                solved,
                compressed: true,
                achieved_dj: Option::None,
            },
            // No ordered step: copy a legal senior or preserve the legal junior.
            (Ok(None), senior, junior) => match hierarchy_fallback(senior, junior, bg, floor) {
                Ok(resolved) => resolved,
                Err(reason) => admit_resolution(Err(reason))?,
            },
            (Err(reason), _, _) => admit_resolution(Err(reason))?,
        };
    }
    Ok(())
}

/// Frozen threshold of the test-only built-in `RoleTable` oracle. Production
/// named roles do not infer hierarchy from declaration order; this value carries
/// no product or perceptual claim and disappears with that legacy oracle.
#[cfg(test)]
const STRICT_STEP: f64 = 0.5;

/// Try to solve a junior text role at the strongest target that is still
/// *strictly weaker* than its senior (`senior_mag − STRICT_STEP`) and still
/// clears `floor`. Returns the demoted colour, or `None` if even the laxest
/// distinguishable target cannot stay legal — in which case the caller keeps the
/// floored colour and only flags the compression.
#[cfg(test)]
fn demote_below(
    senior_mag: f64,
    ctx: &ResolveContext,
    chroma: RoleChroma,
    floor: Floor,
    bg: &BgInput,
    vc: &ViewingConditions,
) -> Result<Option<Solved>, SolveFailure> {
    // Target just under the senior. W5: numerical solver не владеет юр. полом;
    // semantic boundary передаёт caller-owned final-emission criterion, который
    // проверяется каноническим WCAG22 evaluator-ом на фактических байтах.
    let target = ctx.polarity.sign() * (senior_mag - STRICT_STEP).max(0.0);
    let contract = Contract::text(target);
    // Reuse the set's one background interval without erasing its failure
    // provenance. Only a proven inability to find a weaker legal colour may
    // collapse the hierarchy to equality; unresolved/internal outcomes remain
    // typed failures.
    let interval = *ctx.interval.as_ref().map_err(Clone::clone)?;
    demotion_outcome(
        solve_with_chroma(bg, contract, chroma, vc, interval, floor.criterion()),
        senior_mag,
    )
}

/// Interpret one hierarchy-demotion solve without conflating proof, bounded
/// uncertainty, invalid generated state, and internal drift.
#[cfg(test)]
fn demotion_outcome(
    outcome: Result<Solved, SolveFailure>,
    senior_mag: f64,
) -> Result<Option<Solved>, SolveFailure> {
    match outcome {
        Ok(solved) if solved.lc().abs() + STRICT_STEP <= senior_mag => Ok(Some(solved)),
        Ok(_) => Ok(None),
        Err(failure) => match failure.boundary().map(|boundary| boundary.category()) {
            Some(SolveFailureCategory::Unreachable) => Ok(None),
            Some(SolveFailureCategory::Unresolved) | None => Err(failure),
            Some(SolveFailureCategory::Rejected) => Err(SolveFailure::InternalInvariant(format!(
                "validated sRGB hierarchy demotion produced {failure}"
            ))),
        },
    }
}

/// The `|Lc|` of a role's solved colour in `set`, if it resolved to one.
#[cfg(test)]
fn solved_magnitude(set: &[(Role, Resolved)], role: Role) -> Option<f64> {
    set.iter()
        .find(|(r, _)| *r == role)
        .and_then(|(_, res)| res.solved())
        .map(|s| s.lc().abs())
}

/// The text roles in strict visual-weight order — the sequence the hierarchy
/// invariant and the compression pass walk. Disabled is included: it is still
/// part of the order even though it carries no floor.
#[cfg(test)]
const TEXT_HIERARCHY: [Role; 4] = [
    Role::LabelPrimary,
    Role::LabelSecondary,
    Role::LabelTertiary,
    Role::LabelQuaternary,
];

/// The maximum contrast magnitude the background can supply in `polarity`, read
/// back from the solver's own [`SolveFailure::ExceedsRange`].
///
/// Probing a deliberately unreachable target makes `solve` report the true
/// forward-curve maximum, so the anchor fraction is taken against the same number
/// the solver would clip at — no duplicated contrast constants. A background with
/// genuinely zero headroom in this polarity returns a zero ceiling.
fn max_contrast(
    bg: &BgInput,
    polarity: Polarity,
    vc: &ViewingConditions,
    interval: solve::LumaInterval,
) -> Result<f64, SolveFailure> {
    let sign = polarity.sign();
    // 300 Lc is comfortably past the ~106 ceiling of any sRGB background.
    let probe = Contract::text(sign * 300.0);
    ceiling_from_probe(solve::solve_in(
        bg,
        probe,
        Hue::deg(0.0),
        ChromaPolicy::Neutral,
        vc,
        interval,
    ))
}

fn ceiling_from_probe(result: Result<Solved, SolveFailure>) -> Result<f64, SolveFailure> {
    match result {
        // The probe is unreachable by design; ExceedsRange carries the ceiling.
        Err(SolveFailure::ExceedsRange { max_achievable, .. }) => Ok(max_achievable.abs()),
        Ok(_) => Err(SolveFailure::InternalInvariant(
            "300 Lc ceiling probe unexpectedly resolved".to_string(),
        )),
        // Preserve an already-correct internal failure, but never project another
        // category from this controlled core-generated probe as client/physics.
        Err(failure @ SolveFailure::InternalInvariant(_)) => Err(failure),
        Err(other) => Err(SolveFailure::InternalInvariant(format!(
            "300 Lc ceiling probe returned an unexpected outcome: {other}"
        ))),
    }
}

/// Choose the polarity the whole set resolves in, WCAG-first and VC-independent.
///
/// Stage 1 — *legal reachability*: a text role floors at [`POLARITY_FLOOR_RATIO`]
/// (4.5:1), so the polarity that clears that floor wins. The reachability of each
/// polarity is `contrast_ratio(extreme_fg, bg)` — black for dark-on-light, white
/// for light-on-dark — which is a property of the background alone and does not
/// depend on `vc`, because the WCAG formula does not. This is the fix for the
/// false-unreachable stripe: the old "larger LPC maximum" rule flipped near
/// `#999999`, but the legal floor flips near `#747474`, and on the band between
/// them the LPC rule chose the side that could not reach 4.5:1.
///
/// Stage 2 — *tie-break*: when both sides clear the floor (the narrow
/// double-legal band `Y ∈ [0.175, 0.1833]`), the tie resolves to light-on-dark —
/// the side the perceptual layer prefers across the whole band ([`break_tie`]).
/// The winner is a fixed derived polarity, so the decision stays VC-independent.
/// When neither clears it, the side that comes *closest* wins, so the role's
/// [`SolveFailure`] reports the honest best-case `max_ratio`.
fn choose_polarity(bg: &BgInput) -> Polarity {
    let bg_disp = bg_display(bg);
    // Dark-on-light is hosted by a black foreground; light-on-dark by white.
    let ratio_dark_on_light =
        crate::spaces::srgb::encoded_srgb_contrast_ratio([0.0, 0.0, 0.0], bg_disp);
    let ratio_light_on_dark =
        crate::spaces::srgb::encoded_srgb_contrast_ratio([1.0, 1.0, 1.0], bg_disp);

    let dol_clears = ratio_dark_on_light + 1e-9 >= POLARITY_FLOOR_RATIO;
    let lod_clears = ratio_light_on_dark + 1e-9 >= POLARITY_FLOOR_RATIO;

    match (dol_clears, lod_clears) {
        // Exactly one side is legal — take it.
        (true, false) => Polarity::DarkOnLight,
        (false, true) => Polarity::LightOnDark,
        // Both legal — the derived perceptual winner across the band (white).
        (true, true) => break_tie(),
        // Neither legal — the closest side, so the diagnostic is the honest best.
        (false, false) => {
            if ratio_dark_on_light >= ratio_light_on_dark {
                Polarity::DarkOnLight
            } else {
                Polarity::LightOnDark
            }
        }
    }
}

/// Break a polarity tie when both sides clear the legal floor: **light-on-dark
/// (white)** — a value derived from the band's geometry and the perceptual
/// metric, not a tuned convention.
///
/// # Derivation
///
/// The double-legal band is narrow by construction. Solving the WCAG ratio for a
/// background luminance `Y`, black text clears the AA 4.5:1 floor when
/// `(Y + 0.05) / 0.05 ≥ 4.5` (i.e. `Y ≥ 0.175`) and white when
/// `1.05 / (Y + 0.05) ≥ 4.5` (i.e. `Y ≤ 0.1833`). Both are legal only on
/// `Y ∈ [0.175, 0.1833]`, a band ≈0.008 wide.
///
/// Legality does not decide the tie there — both sides are legal by definition —
/// so the perceptual layer does. In the luminance domain the LPC core
/// ([`crate::lpc::contrast_core`]) is asymmetric: its light-on-dark exponents
/// make a light foreground read *stronger* than a dark one against a
/// mid-luminance background, and the crossover where black would overtake white
/// sits far above the band, near `Y ≈ 0.342` — measured by bisection of the
/// luminance core and locked by
/// `pair::exposure_locks::pair_crossover_equals_measured_core_polarity_flip`
/// (on `Y = 0.211` white scores ≈69.8 Lc against black's ≈39.7; the earlier V3
/// estimate `≈0.36` is superseded by this measurement). So across the *entire*
/// double-legal band the readable-and-perceptually-stronger side is white.
///
/// This replaces the former "larger WCAG margin wins" rule. The WCAG ratio is
/// symmetric and non-perceptual; its margin crossover lies *inside* the band, at
/// `Y ≈ 0.1791` (where `(Y+0.05)/0.05 = 1.05/(Y+0.05)`), so on the upper half
/// `Y ∈ (0.1791, 0.1833]` the margin rule picked dark-on-light — the
/// perceptually weaker side, and the one that made Fluent `#0078D4` (white legal
/// at 4.529:1) emit black on its 4.637:1 margin, against the platform convention
/// of white text on that blue.
///
/// The decision is still a pure function of the background bytes (band
/// membership is WCAG; the winner is a fixed derived polarity), so it stays
/// VC-independent by construction — no LPC *fallback* that would read the
/// viewing conditions and re-open the per-theme-flip seam this module promises
/// away. The perceptual asymmetry motivates the constant; it is not evaluated
/// per background.
fn break_tie() -> Polarity {
    Polarity::LightOnDark
}

/// The quantised 8-bit *display* sRGB the WCAG formula is measured against — the
/// exact bytes of the background's hex.
///
/// [`BgInput`] stores *linear*-light sRGB (from `srgb_from_hex`), so it is
/// gamma-encoded back to display space and rounded to the 8-bit grid, matching
/// the quantisation `solve` uses internally so both sides of the WCAG comparison
/// are on the same grid.
fn bg_display(bg: &BgInput) -> [f64; 3] {
    let rgb_linear = bg.linear_srgb();
    let q = |c: f64| (srgb_gamma(c).clamp(0.0, 1.0) * 255.0).round() / 255.0;
    [q(rgb_linear[0]), q(rgb_linear[1]), q(rgb_linear[2])]
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn admission_preserves_every_failure_category_code_and_reason() {
        let local = [
            (
                SolveFailure::BelowContrastFloor { target: 3.0 },
                RoleFailureCategory::Unreachable,
                "below_contrast_floor",
            ),
            (
                SolveFailure::ExceedsRange {
                    target: 300.0,
                    max_achievable: 106.0,
                },
                RoleFailureCategory::Unreachable,
                "exceeds_range",
            ),
            (
                SolveFailure::BoundedSearchExhausted {
                    target: 10.0,
                    closest_examined: 9.0,
                },
                RoleFailureCategory::Unresolved,
                "bounded_search_exhausted",
            ),
        ];
        for (reason, category, code) in local {
            let admitted =
                admit_resolution(Err(reason.clone())).expect("physical failure remains role-local");
            let Resolved::Failure(failure) = admitted else {
                panic!("physical failure admitted as non-failure: {admitted:?}");
            };
            assert_eq!(failure.category(), category);
            assert_eq!(failure.code(), code);
            assert_eq!(failure.reason(), &reason);
        }

        let outer = [
            (
                SolveFailure::InvalidInput("invalid fixture".into()),
                ResolveSetErrorKind::Rejected,
                Some("invalid_input"),
            ),
            (
                SolveFailure::InternalInvariant("injected drift".into()),
                ResolveSetErrorKind::Internal,
                None,
            ),
        ];
        for (reason, kind, code) in outer {
            let error = admit_resolution(Err(reason.clone()))
                .expect_err("non-local failure must close the whole set");
            assert_eq!(error.kind(), kind);
            assert_eq!(error.code(), code);
            assert_eq!(error.reason(), &reason);
        }
    }

    #[test]
    fn admission_passes_success_through_unchanged() {
        assert_eq!(admit_resolution(Ok(Resolved::None)), Ok(Resolved::None));
    }

    #[test]
    fn invalid_alpha_cannot_enter_a_compiled_named_table() {
        let source = crate::spaces::srgb::srgb_encoded_from_hex("#3478F6").unwrap();
        let entries = vec![
            ("first".into(), RoleSpec::Zero),
            (
                "fatal-last".into(),
                RoleSpec::AlphaAnalog {
                    of: LadderTint::new([source; 4]).unwrap(),
                    alpha: f64::NAN,
                },
            ),
        ];
        let error = NamedRoleTable::from_validated_parts(entries, Vec::new(), RoleChroma::Neutral)
            .expect_err("invalid requested alpha must fail before a table exists");
        let NamedRoleTableCompileError::AlphaAnalog(error) = error else {
            panic!("valid names must reach the alpha-domain gate");
        };
        assert_eq!(error.declaration_ordinal(), 1);
        assert!(error.value().is_nan());
    }

    #[test]
    fn named_alpha_analog_uses_only_its_compiled_invocation_and_guards_plan_drift() {
        let target = crate::spaces::srgb::srgb_encoded_from_hex("#787880").unwrap();
        let table = NamedRoleTable::new(
            vec![
                ("plain".into(), RoleSpec::Zero),
                (
                    "analog".into(),
                    RoleSpec::AlphaAnalog {
                        of: LadderTint::new([target; 4]).unwrap(),
                        alpha: 0.5,
                    },
                ),
            ],
            Vec::new(),
            RoleChroma::Neutral,
        )
        .unwrap();
        assert_eq!(table.point_representation_invocations.len(), 1);
        assert_eq!(
            table.point_representation_invocations[0].declaration_ordinal,
            1
        );
        assert_eq!(
            table.point_representation_invocations[0]
                .opacity_domain
                .lower()
                .bits(),
            0.5_f64.to_bits(),
        );
        assert_eq!(
            table.point_representation_invocations[0]
                .opacity_domain
                .upper(),
            crate::composition::AdmittedOpacityV1::OPAQUE,
        );

        crate::composition::reset_source_over_evaluation_count();
        let set = resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &table,
            &ViewingConditions::srgb(),
        )
        .expect("compiled invocation must intercept the raw recipe arm");
        assert_eq!(set[1].1.translucent().unwrap().composite_hex(), "#787880");
        assert_eq!(crate::composition::source_over_evaluation_count(), 1);

        let mut missing = table.clone();
        missing.point_representation_invocations = Box::default();
        assert_eq!(
            missing, table,
            "derived execution state is outside public equality"
        );
        assert_eq!(format!("{missing:?}"), format!("{table:?}"));
        let error = resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &missing,
            &ViewingConditions::srgb(),
        )
        .expect_err("missing compiled invocation must not fall back to recipe execution");
        assert_eq!(error.kind(), ResolveSetErrorKind::Internal);
        assert!(matches!(error.reason(), SolveFailure::InternalInvariant(_)));
    }

    #[test]
    fn named_alpha_dispatch_does_not_read_recipe_kind_after_lowering() {
        let target = crate::spaces::srgb::srgb_encoded_from_hex("#787880").unwrap();
        let mut table = NamedRoleTable::new(
            vec![(
                "opaque-client-id".into(),
                RoleSpec::AlphaAnalog {
                    of: LadderTint::new([target; 4]).unwrap(),
                    alpha: 0.5,
                },
            )],
            Vec::new(),
            RoleChroma::Neutral,
        )
        .unwrap();
        table.entries[0].1 = RoleSpec::Zero;

        let set = resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &table,
            &ViewingConditions::srgb(),
        )
        .expect("compiled ordinal dispatch must not reopen the authored recipe");
        assert_eq!(set[0].1.translucent().unwrap().composite_hex(), "#787880");
    }

    #[test]
    fn named_material_uses_only_its_compiled_invocation_and_guards_plan_drift() {
        let table = NamedRoleTable::new(
            vec![
                ("plain".into(), RoleSpec::Zero),
                (
                    "material".into(),
                    RoleSpec::Material {
                        hue: None,
                        tone: DjMagnitude::new(14.0, 14.0),
                        floor: Floor::AaText,
                    },
                ),
            ],
            Vec::new(),
            RoleChroma::Neutral,
        )
        .unwrap();
        assert_eq!(table.material_invocations.len(), 1);
        assert_eq!(table.material_invocations[0].declaration_ordinal, 1);

        let set = resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &table,
            &ViewingConditions::srgb(),
        )
        .expect("compiled invocation must intercept the raw recipe arm");
        assert!(
            matches!(&set[1].1, Resolved::Material(_)),
            "compiled Material invocation must resolve to a Material outcome"
        );

        let mut missing = table.clone();
        missing.material_invocations = Box::default();
        assert_eq!(
            missing, table,
            "derived execution state is outside public equality"
        );
        assert_eq!(format!("{missing:?}"), format!("{table:?}"));
        let error = resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &missing,
            &ViewingConditions::srgb(),
        )
        .expect_err("missing compiled invocation must not fall back to recipe execution");
        assert_eq!(error.kind(), ResolveSetErrorKind::Internal);
        assert!(matches!(error.reason(), SolveFailure::InternalInvariant(_)));
    }

    #[test]
    fn named_glow_uses_only_its_compiled_invocation_and_guards_plan_drift() {
        let tint = crate::spaces::srgb::srgb_encoded_from_hex("#8AB4F8").unwrap();
        let table = NamedRoleTable::new(
            vec![
                ("plain".into(), RoleSpec::Zero),
                (
                    "glow".into(),
                    RoleSpec::Glow {
                        tint: LadderTint::new([tint; 4]).unwrap(),
                        step: crate::glow::GlowStep::Base,
                        mode: crate::numerical_plan::NumericalExecutionModeV1::StableOnly,
                    },
                ),
            ],
            Vec::new(),
            RoleChroma::Neutral,
        )
        .unwrap();
        assert_eq!(table.glow_invocations.len(), 1);
        assert_eq!(table.glow_invocations[0].declaration_ordinal, 1);

        let set = resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &table,
            &ViewingConditions::srgb(),
        )
        .expect("compiled invocation must intercept the raw recipe arm");
        assert!(
            matches!(
                &set[1].1,
                Resolved::Glow(_) | Resolved::GlowIndeterminate(_)
            ),
            "compiled Glow invocation must resolve to a Glow outcome"
        );

        let mut missing = table.clone();
        missing.glow_invocations = Box::default();
        let error = resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &missing,
            &ViewingConditions::srgb(),
        )
        .expect_err("missing compiled invocation must not fall back to recipe execution");
        assert_eq!(error.kind(), ResolveSetErrorKind::Internal);
        assert!(matches!(error.reason(), SolveFailure::InternalInvariant(_)));
    }

    #[test]
    fn named_material_dispatch_does_not_read_recipe_kind_after_lowering() {
        let mut table = NamedRoleTable::new(
            vec![(
                "opaque-client-id".into(),
                RoleSpec::Material {
                    hue: None,
                    tone: DjMagnitude::new(14.0, 14.0),
                    floor: Floor::AaText,
                },
            )],
            Vec::new(),
            RoleChroma::Neutral,
        )
        .unwrap();
        table.entries[0].1 = RoleSpec::Zero;

        let set = resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &table,
            &ViewingConditions::srgb(),
        )
        .expect("compiled ordinal dispatch must not reopen the authored recipe");
        assert!(
            matches!(&set[0].1, Resolved::Material(_)),
            "dispatch must keep executing the compiled Material invocation"
        );
    }

    #[test]
    fn material_compile_boundary_rejects_zero_floor_even_from_validated_parts() {
        let entries = vec![(
            "material-without-floor".into(),
            RoleSpec::Material {
                hue: None,
                tone: DjMagnitude::new(14.0, 14.0),
                floor: Floor::None,
            },
        )];
        let error = NamedRoleTable::from_validated_parts(entries, Vec::new(), RoleChroma::Neutral)
            .expect_err("zero-floor Material must fail before a table exists");
        let NamedRoleTableCompileError::Material(error) = error else {
            panic!("valid names must reach the material floor gate");
        };
        assert_eq!(error.declaration_ordinal(), 0);
    }

    #[test]
    fn material_invocations_are_sparse_compiled_once_and_reused() {
        let table = NamedRoleTable::new(
            vec![
                (
                    "first".into(),
                    RoleSpec::Material {
                        hue: None,
                        tone: DjMagnitude::new(12.0, 12.0),
                        floor: Floor::AaText,
                    },
                ),
                ("unrelated".into(), RoleSpec::Zero),
                (
                    "second".into(),
                    RoleSpec::Material {
                        hue: None,
                        tone: DjMagnitude::new(18.0, 18.0),
                        floor: Floor::AaUi,
                    },
                ),
            ],
            Vec::new(),
            RoleChroma::Neutral,
        )
        .unwrap();
        assert_eq!(table.material_invocations.len(), 2);
        assert_eq!(table.material_invocations[0].declaration_ordinal, 0);
        assert_eq!(table.material_invocations[1].declaration_ordinal, 2);
        assert_eq!(table.material_invocations[0].floor, Floor::AaText);
        assert_eq!(table.material_invocations[1].floor, Floor::AaUi);
    }

    #[test]
    fn point_representation_plan_is_sparse_compiled_once_and_reused() {
        let first_target = crate::spaces::srgb::srgb_encoded_from_hex("#787880").unwrap();
        let second_target = crate::spaces::srgb::srgb_encoded_from_hex("#406080").unwrap();
        reset_point_representation_plan_compilation_count();
        let table = NamedRoleTable::new(
            vec![
                (
                    "first".into(),
                    RoleSpec::AlphaAnalog {
                        of: LadderTint::new([first_target; 4]).unwrap(),
                        alpha: 0.5,
                    },
                ),
                ("unrelated".into(), RoleSpec::Zero),
                (
                    "second".into(),
                    RoleSpec::AlphaAnalog {
                        of: LadderTint::new([second_target; 4]).unwrap(),
                        alpha: 0.75,
                    },
                ),
            ],
            Vec::new(),
            RoleChroma::Neutral,
        )
        .unwrap();
        assert_eq!(point_representation_plan_compilation_count(), 1);
        assert_eq!(
            table
                .point_representation_invocations
                .iter()
                .map(|invocation| invocation.declaration_ordinal)
                .collect::<Vec<_>>(),
            vec![0, 2]
        );

        let cloned = table.clone();
        let backdrop = BgInput::solid("#FFFFFF").unwrap();
        for current in [&table, &table, &cloned] {
            let set = resolve_named_set(&backdrop, current, &ViewingConditions::srgb()).unwrap();
            assert_eq!(set.len(), 3);
        }
        assert_eq!(point_representation_plan_compilation_count(), 1);

        let without_analogs = NamedRoleTable::new(
            vec![("unrelated".into(), RoleSpec::Zero)],
            Vec::new(),
            RoleChroma::Neutral,
        )
        .unwrap();
        assert!(without_analogs.point_representation_invocations.is_empty());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn compiled_frontend_and_generic_point_law_share_exact_bytes(
            target_bytes in any::<[u8; 3]>(),
            backdrop_bytes in any::<[u8; 3]>(),
            alpha_step in 1_u16..=1024,
        ) {
            let target = Srgb8::new(target_bytes);
            let backdrop = Srgb8::new(backdrop_bytes);
            let requested_alpha = f64::from(alpha_step) / 1024.0;
            let table = NamedRoleTable::new(
                vec![(
                    "opaque-client-id".into(),
                    RoleSpec::AlphaAnalog {
                        of: LadderTint::new([target.encoded(); 4]).unwrap(),
                        alpha: requested_alpha,
                    },
                )],
                Vec::new(),
                RoleChroma::Neutral,
            )
            .unwrap();
            let named = resolve_named_set(
                &BgInput::solid(&backdrop.to_hex()).unwrap(),
                &table,
                &ViewingConditions::srgb(),
            )
            .unwrap();
            let named = named[0].1.translucent().unwrap();
            let physical = crate::point_representation::resolve_exact_point_representation_v1(
                target,
                crate::composition::OpacityDomainV1::try_new(requested_alpha, 1.0).unwrap(),
                backdrop,
            )
            .unwrap();

            prop_assert_eq!(named.tint_hex(), physical.source().to_hex());
            prop_assert_eq!(named.alpha().to_bits(), physical.opacity().value().to_bits());
            prop_assert_eq!(named.composite_hex(), target.to_hex());
            prop_assert_eq!(
                crate::alpha::composite_hex(
                    &physical.source().to_hex(),
                    physical.opacity().value(),
                    &backdrop.to_hex(),
                ).unwrap(),
                target.to_hex()
            );
        }
    }

    #[test]
    fn physical_unreachability_remains_local_after_real_resolution() {
        let table = NamedRoleTable::new(
            vec![(
                "physically-unreachable".into(),
                RoleSpec::Decorative { magnitude: 300.0 },
            )],
            Vec::new(),
            RoleChroma::Neutral,
        )
        .unwrap();
        let set = resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &table,
            &ViewingConditions::srgb(),
        )
        .expect("physical failure is role-local, not a whole-set error");
        let Resolved::Failure(failure) = &set[0].1 else {
            panic!("real unreachable fixture must stay a typed role failure: {set:?}");
        };
        assert_eq!(failure.category(), RoleFailureCategory::Unreachable);
        assert_eq!(failure.code(), "exceeds_range");
        let SolveFailure::ExceedsRange {
            target,
            max_achievable,
        } = failure.reason()
        else {
            panic!("unexpected physical reason: {failure:?}");
        };
        assert_eq!(target.to_bits(), 300.0_f64.to_bits());
        assert!(max_achievable.is_finite() && *max_achievable > 0.0);
    }

    #[test]
    fn controlled_ceiling_probe_never_leaks_a_client_or_physical_failure() {
        for unexpected in [
            SolveFailure::BelowContrastFloor { target: 3.0 },
            SolveFailure::BoundedSearchExhausted {
                target: 300.0,
                closest_examined: 100.0,
            },
            SolveFailure::InvalidInput("generated probe".into()),
        ] {
            assert!(
                matches!(
                    ceiling_from_probe(Err(unexpected)),
                    Err(SolveFailure::InternalInvariant(_))
                ),
                "controlled probe drift must fail the enclosing call"
            );
        }
        let internal = SolveFailure::InternalInvariant("original provenance".into());
        assert_eq!(ceiling_from_probe(Err(internal.clone())), Err(internal));
    }

    #[test]
    fn anchored_contract_preserves_context_failure_and_accepts_zero_ceiling() {
        let anchor = TextAnchor::new(0.5, Floor::AaText).unwrap();
        let failure = SolveFailure::InternalInvariant("controlled probe drift".into());
        let failed = ResolveContext {
            polarity: Polarity::DarkOnLight,
            max_contrast: Err(failure.clone()),
            interval: Err(SolveFailure::InternalInvariant("unused interval".into())),
            high_contrast: false,
        };
        assert_eq!(failed.anchored_contract(anchor), Err(failure));

        let zero = ResolveContext {
            polarity: Polarity::DarkOnLight,
            max_contrast: Ok(0.0),
            interval: Err(SolveFailure::InternalInvariant("unused interval".into())),
            high_contrast: false,
        };
        let contract = zero.anchored_contract(anchor).unwrap();
        assert_eq!(contract.floor(), 0.0, "zero is a valid physical ceiling");
    }

    #[test]
    fn hierarchy_demotion_distinguishes_proof_from_uncertainty_and_core_drift() {
        for failure in [
            SolveFailure::BoundedSearchExhausted {
                target: 10.0,
                closest_examined: 9.0,
            },
            SolveFailure::InternalInvariant("injected drift".into()),
        ] {
            assert_eq!(demotion_outcome(Err(failure.clone()), 20.0), Err(failure));
        }
        for failure in [
            SolveFailure::BelowContrastFloor { target: 3.0 },
            SolveFailure::ExceedsRange {
                target: 20.0,
                max_achievable: 10.0,
            },
        ] {
            assert_eq!(demotion_outcome(Err(failure), 20.0), Ok(None));
        }
        let failure = SolveFailure::InvalidInput("generated request".into());
        assert!(matches!(
            demotion_outcome(Err(failure), 20.0),
            Err(SolveFailure::InternalInvariant(_))
        ));
    }

    #[test]
    fn hierarchy_demotion_preserves_the_junior_final_emission_criterion() {
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let ctx = ResolveContext::new(&bg, &vc);
        let interval = *ctx.interval.as_ref().unwrap();
        let criterion = Wcag22CriterionV1::Sc143TextDefault;
        let senior_mag = 1.0;
        let target = ctx.polarity.sign() * (senior_mag - STRICT_STEP).max(0.0);
        let unconstrained = solve_with_chroma(
            &bg,
            Contract::text(target),
            RoleChroma::Neutral,
            &vc,
            interval,
            None,
        )
        .expect("the unconstrained hierarchy probe must be solvable");
        assert!(
            !wcag22_final_emission_passes(unconstrained.hex(), "#FFFFFF", criterion).unwrap(),
            "fixture must be RED before the semantic criterion is carried into demotion"
        );

        let constrained = demote_below(
            senior_mag,
            &ctx,
            RoleChroma::Neutral,
            Floor::AaText,
            &bg,
            &vc,
        )
        .expect("criterion enforcement must not produce an internal failure");
        assert_eq!(
            constrained, None,
            "no legal distinguishable demotion exists at this boundary; the semantic floor must not be dropped"
        );
    }

    #[test]
    fn hierarchy_fallback_never_replaces_a_legal_junior_with_an_illegal_senior() {
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let interval = bg.luma_interval(&vc).unwrap();
        let criterion = Wcag22CriterionV1::Sc143TextDefault;
        let senior = solve::solve_in(
            &bg,
            Contract::text(20.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            interval,
        )
        .unwrap();
        assert!(!wcag22_final_emission_passes(senior.hex(), "#FFFFFF", criterion).unwrap());
        let junior = solve_in_with_criterion(
            &bg,
            Contract::text(60.0),
            Hue::deg(0.0),
            ChromaPolicy::Neutral,
            &vc,
            interval,
            Some(criterion),
        )
        .unwrap();
        assert!(wcag22_final_emission_passes(junior.hex(), "#FFFFFF", criterion).unwrap());
        let junior_outcome = Resolved::Color {
            solved: junior.clone(),
            compressed: false,
            achieved_dj: None,
        };

        let fallback = hierarchy_fallback(Some(senior), &junior_outcome, &bg, Floor::AaText)
            .expect("canonical final-byte evaluation must be total for generated sRGB8");
        let Resolved::Color {
            solved,
            compressed,
            achieved_dj,
        } = fallback
        else {
            panic!("legal junior must remain a colour outcome");
        };
        assert_eq!(solved.hex(), junior.hex());
        assert!(compressed);
        assert_eq!(achieved_dj, None);
    }

    #[test]
    fn named_table_rejects_non_physical_decorative_magnitudes_at_construction() {
        for magnitude in [
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            0.0,
            -1.0,
            f64::from_bits(DECORATIVE_FLOOR_MIN.to_bits() - 1),
        ] {
            assert!(matches!(
                NamedRoleTable::new(
                    vec![("decorative".into(), RoleSpec::Decorative { magnitude })],
                    Vec::new(),
                    RoleChroma::Neutral,
                ),
                Err(SolveFailure::InvalidInput(_))
            ));
        }
        assert!(
            NamedRoleTable::new(
                vec![(
                    "decorative".into(),
                    RoleSpec::Decorative {
                        magnitude: DECORATIVE_FLOOR_MIN,
                    },
                )],
                Vec::new(),
                RoleChroma::Neutral,
            )
            .is_ok()
        );
    }

    #[test]
    fn named_table_validates_every_raw_role_field_before_resolution() {
        let tint = LadderTint::new([[0.25, 0.5, 0.75]; 4]).unwrap();
        let ladder = |light, dark| RoleSpec::Ladder {
            tint,
            alpha_light: light,
            alpha_dark: dark,
            floor: None,
        };
        let ladder_with_floor = |light, dark, floor| RoleSpec::Ladder {
            tint,
            alpha_light: light,
            alpha_dark: dark,
            floor,
        };
        let material = |light, dark, floor| RoleSpec::Material {
            hue: Some(tint),
            tone: DjMagnitude::new(light, dark),
            floor,
        };
        let invalid = [
            RoleSpec::DecorativeDj {
                magnitude_dj: DjMagnitude::new(f64::NAN, 1.0),
            },
            RoleSpec::DecorativeDj {
                magnitude_dj: DjMagnitude::new(1.0, 0.0),
            },
            ladder(0.0, 0.5),
            ladder(0.5, f64::NAN),
            ladder_with_floor(1.0, 1.0, Some(Floor::None)),
            ladder_with_floor(1.0, 0.5, Some(Floor::AaUi)),
            RoleSpec::AlphaAnalog {
                of: tint,
                alpha: 0.0,
            },
            material(f64::NAN, 1.0, Floor::AaText),
            material(1.0, 0.0, Floor::AaText),
            material(1.0, 1.0, Floor::None),
        ];
        for spec in invalid {
            assert!(matches!(
                NamedRoleTable::new(
                    vec![("invalid".into(), spec)],
                    Vec::new(),
                    RoleChroma::Neutral,
                ),
                Err(SolveFailure::InvalidInput(_))
            ));
        }

        for spec in [
            RoleSpec::DecorativeDj {
                magnitude_dj: DjMagnitude::new(1.0, 2.0),
            },
            ladder(0.25, 1.0),
            ladder_with_floor(1.0, 1.0, Some(Floor::AaUi)),
            RoleSpec::AlphaAnalog {
                of: tint,
                alpha: 0.25,
            },
        ] {
            assert!(
                NamedRoleTable::new(
                    vec![("valid".into(), spec)],
                    Vec::new(),
                    RoleChroma::Neutral,
                )
                .is_ok()
            );
        }

        assert!(
            matches!(
                NamedRoleTable::new(
                    vec![("family-material".into(), material(1.0, 2.0, Floor::AaText),)],
                    Vec::new(),
                    RoleChroma::Neutral,
                ),
                Err(SolveFailure::InvalidInput(_))
            ),
            "a family-hued material cannot be deferred into an achromatic executable table"
        );
        assert!(
            NamedRoleTable::new(
                vec![(
                    "neutral-material".into(),
                    RoleSpec::Material {
                        hue: None,
                        tone: DjMagnitude::new(1.0, 2.0),
                        floor: Floor::AaText,
                    },
                )],
                Vec::new(),
                RoleChroma::Neutral,
            )
            .is_ok(),
            "an explicitly neutral material is valid with an achromatic policy"
        );
        assert!(
            NamedRoleTable::new(
                vec![("family-material".into(), material(1.0, 2.0, Floor::AaText),)],
                Vec::new(),
                RoleChroma::Tinted {
                    hue_deg: 240.0,
                    ratio: 0.5,
                },
            )
            .is_ok(),
            "a family-hued material is valid when the table carries chroma"
        );
    }

    fn one_glow_table(
        source_hex: &str,
        decision_profile: crate::glow::GlowDecisionProfileV1,
    ) -> NamedRoleTable {
        let source = crate::spaces::srgb::srgb_encoded_from_hex(source_hex).unwrap();
        NamedRoleTable::new(
            vec![(
                "opaque-client-id".to_string(),
                RoleSpec::Glow {
                    tint: LadderTint::new([source; 4]).unwrap(),
                    step: crate::glow::GlowStep::Base,
                    mode: decision_profile.execution_mode(),
                },
            )],
            Vec::new(),
            RoleChroma::Neutral,
        )
        .unwrap()
    }

    #[test]
    fn full_glow_separates_recipe_appearance_and_selection_diagnostics() {
        let vc = ViewingConditions::srgb();

        let exact = resolve_named_set(
            &BgInput::solid("#FFFFFF").unwrap(),
            &one_glow_table("#4A8FFF", crate::glow::GlowDecisionProfileV1::StableV1),
            &vc,
        )
        .expect("valid stable Glow fixture resolves atomically");
        let Resolved::Glow(exact) = &exact[0].1 else {
            panic!("stable point no-op must resolve as a full Glow result");
        };
        assert_eq!(
            exact.target_status(),
            crate::glow::GlowTargetStatus::ExactNoopUnreachable
        );
        assert_eq!(
            exact.layer_recipe_profile().key(),
            "cam16-jprime-oklab-cusp-v1"
        );
        assert_eq!(
            exact.appearance_diagnostic_profile(),
            crate::glow::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1
        );
        assert!(exact.selection_diagnostic_profile().is_none());

        // C7e hard-cut: LegacyPlatformDependentV1 no longer executes CAM16 at
        // runtime. Non-noop inputs now resolve as GlowIndeterminate regardless
        // of profile, preserving wire schema compatibility without retaining
        // the platform-dependent solver path.
        let legacy = resolve_named_set(
            &BgInput::solid("#101012").unwrap(),
            &one_glow_table(
                "#4A8FFF",
                crate::glow::GlowDecisionProfileV1::LegacyPlatformDependentV1,
            ),
            &vc,
        )
        .expect("valid legacy Glow fixture resolves atomically");
        let Resolved::GlowIndeterminate(legacy) = &legacy[0].1 else {
            panic!(
                "explicit legacy selection must resolve as GlowIndeterminate after C7e hard-cut: got {:?}",
                legacy[0].1
            );
        };
        assert_eq!(
            legacy.decision_profile,
            crate::glow::GlowDecisionProfileV1::LegacyPlatformDependentV1
        );
        assert_eq!(
            legacy.evidence,
            crate::numerics::NumericalIndeterminacyV1::SoundBoundUnavailable
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // RED-группа 1 Issue #292 — numerical_plan_v1: derived-проекция таблицы.
    //
    // Таблицы собираются напрямую через `NamedRoleTable::new` (как
    // `one_glow_table`): план — свойство скомпилированной таблицы, не
    // конфиг-валидатора, поэтому тесты не зависят от словаря клиента.
    // ─────────────────────────────────────────────────────────────────────────

    /// Glow-спека с непрозрачным для core именем-носителем задаётся снаружи;
    /// здесь только рецепт: тинт + ступень + typed mode.
    fn plan_glow_spec(
        source_hex: &str,
        mode: crate::numerical_plan::NumericalExecutionModeV1,
    ) -> RoleSpec {
        let source = crate::spaces::srgb::srgb_encoded_from_hex(source_hex).unwrap();
        RoleSpec::Glow {
            tint: LadderTint::new([source; 4]).unwrap(),
            step: crate::glow::GlowStep::Base,
            mode,
        }
    }

    fn plan_stable_mode() -> crate::numerical_plan::NumericalExecutionModeV1 {
        crate::numerical_plan::NumericalExecutionModeV1::StableOnly
    }

    fn plan_compat_mode() -> crate::numerical_plan::NumericalExecutionModeV1 {
        crate::numerical_plan::NumericalExecutionModeV1::ExplicitCompatibility {
            release_id:
                crate::numerics::NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,
        }
    }

    fn plan_table(entries: Vec<(String, RoleSpec)>) -> NamedRoleTable {
        NamedRoleTable::new(entries, Vec::new(), RoleChroma::Neutral).unwrap()
    }

    /// (a)+(e) #292: биекция glow-ролей и invocations; каждый invocation
    /// соответствует manifest-supported site (mode/release объявлены
    /// capability-строкой); mixed modes сосуществуют в одном плане без
    /// глобального профиля; не-glow роли и алиасы не порождают invocations
    /// и остаются в `entries()` нетронутыми.
    #[test]
    fn red292_plan_is_bijective_with_glow_roles_and_manifest_supported() {
        use crate::numerical_plan::NumericalExecutionModeV1;
        let table = NamedRoleTable::new(
            vec![
                (
                    "klient-uzel-alpha".to_string(),
                    plan_glow_spec("#4A8FFF", plan_stable_mode()),
                ),
                // Не-glow роль между glow-декларациями: план обязан её пропустить.
                ("plain-zero".to_string(), RoleSpec::Zero),
                (
                    "klient-uzel-beta".to_string(),
                    plan_glow_spec("#FF6633", plan_compat_mode()),
                ),
            ],
            vec![("ring".to_string(), "plain-zero".to_string())],
            RoleChroma::Neutral,
        )
        .unwrap();

        let plan = table.numerical_plan_v1().unwrap();

        // Биекция: ровно по одному invocation на каждую glow-роль, и ни одного
        // на прочие роли/алиасы.
        assert_eq!(plan.invocations().len(), 2);
        for glow_name in ["klient-uzel-alpha", "klient-uzel-beta"] {
            assert_eq!(
                plan.invocations()
                    .iter()
                    .filter(|inv| inv.invocation_id.node_bytes() == glow_name.as_bytes())
                    .count(),
                1,
                "glow-роль `{glow_name}` обязана дать ровно один invocation"
            );
        }
        assert!(
            plan.invocations()
                .iter()
                .all(|inv| inv.invocation_id.node_bytes() != b"plain-zero"
                    && inv.invocation_id.node_bytes() != b"ring"),
            "не-glow роль/алиас не порождают numerical invocations"
        );
        // Таблица (resolve-словарь) нетронута планом.
        assert_eq!(
            table
                .entries()
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>(),
            ["klient-uzel-alpha", "plain-zero", "klient-uzel-beta"]
        );

        // Каждый invocation соответствует manifest-supported site: mode/release
        // объявлены capability-строкой сборки (registry SSOT).
        let manifest = crate::numerics::numerical_capability_manifest_v2();
        for inv in plan.invocations() {
            let site = manifest
                .sites
                .iter()
                .find(|site| site.site_id.key() == inv.site_id.key())
                .expect("site каждого invocation присутствует в capability manifest");
            match inv.mode {
                NumericalExecutionModeV1::StableOnly => assert!(
                    !site.stable_outcomes.is_empty(),
                    "StableOnly требует объявленных stable outcomes"
                ),
                NumericalExecutionModeV1::ExplicitCompatibility { release_id } => assert!(
                    site.compatibility_releases.contains(&release_id),
                    "release {} обязан быть зарегистрирован для site {}",
                    release_id.key(),
                    inv.site_id.key()
                ),
            }
        }

        // (e) mixed modes сосуществуют в ОДНОМ плане: у каждой invocation свой
        // typed mode, глобального профиля не существует.
        assert!(
            plan.invocations()
                .iter()
                .any(|inv| inv.mode == NumericalExecutionModeV1::StableOnly)
                && plan.invocations().iter().any(|inv| matches!(
                    inv.mode,
                    NumericalExecutionModeV1::ExplicitCompatibility { .. }
                ))
        );
    }

    /// (b)+(c) #292: A=[z,a] и B=[a,z] дают одинаковые invocation
    /// canonical_bytes и plan checksum (canonical-сортировка — закон плана),
    /// а `entries()` каждой таблицы сохраняет СВОЙ порядок деклараций;
    /// вставка третьего узла не меняет прежние canonical_bytes.
    #[test]
    fn red292_permutation_preserves_canonical_projection_and_declared_order() {
        let z = || {
            (
                "z-glow".to_string(),
                plan_glow_spec("#4A8FFF", plan_stable_mode()),
            )
        };
        let a = || {
            (
                "a-glow".to_string(),
                plan_glow_spec("#FF6633", plan_compat_mode()),
            )
        };
        let m = || {
            (
                "m-glow".to_string(),
                plan_glow_spec("#33CC88", plan_stable_mode()),
            )
        };

        let table_a = plan_table(vec![z(), a()]);
        let table_b = plan_table(vec![a(), z()]);

        // resolve-порядок = порядок деклараций каждой стороны (план его не трогает).
        let names = |t: &NamedRoleTable| {
            t.entries()
                .iter()
                .map(|(n, _)| n.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&table_a), ["z-glow", "a-glow"]);
        assert_eq!(names(&table_b), ["a-glow", "z-glow"]);

        let plan_a = table_a.numerical_plan_v1().unwrap();
        let plan_b = table_b.numerical_plan_v1().unwrap();
        let bytes = |plan: &crate::numerical_plan::CompiledNumericalPlanV1| {
            plan.invocations()
                .iter()
                .map(|inv| inv.invocation_id.canonical_bytes())
                .collect::<Vec<_>>()
        };
        assert_eq!(bytes(&plan_a), bytes(&plan_b));
        assert_eq!(plan_a.checksum, plan_b.checksum);

        // (c) Вставка третьего узла: прежние identity bytes неподвижны
        // (ordinal локален внутри (node, site), глобальных индексов нет).
        let extended = plan_table(vec![z(), m(), a()]).numerical_plan_v1().unwrap();
        let id_of = |plan: &crate::numerical_plan::CompiledNumericalPlanV1, node: &[u8]| {
            plan.invocations()
                .iter()
                .find(|inv| inv.invocation_id.node_bytes() == node)
                .map(|inv| inv.invocation_id.canonical_bytes())
                .unwrap()
        };
        assert_eq!(id_of(&plan_a, b"z-glow"), id_of(&extended, b"z-glow"));
        assert_eq!(id_of(&plan_a, b"a-glow"), id_of(&extended, b"a-glow"));
        assert_ne!(plan_a.checksum, extended.checksum);
    }

    /// (d) #292: rename роли меняет invocation identity (имя — opaque node
    /// bytes), но typed mode остаётся тем же — семантика исполнения не
    /// привязана к клиентскому имени.
    #[test]
    fn red292_rename_changes_identity_but_not_mode() {
        let old = plan_table(vec![(
            "staroe-imya".to_string(),
            plan_glow_spec("#4A8FFF", plan_compat_mode()),
        )])
        .numerical_plan_v1()
        .unwrap();
        let renamed = plan_table(vec![(
            "novoe-imya".to_string(),
            plan_glow_spec("#4A8FFF", plan_compat_mode()),
        )])
        .numerical_plan_v1()
        .unwrap();
        assert_ne!(
            old.invocations()[0].invocation_id,
            renamed.invocations()[0].invocation_id
        );
        assert_eq!(old.invocations()[0].mode, renamed.invocations()[0].mode);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // DIFFERENTIAL HARNESS (perf/max-chroma-hotpath) — cusp-attracted hue.
    //
    // A frozen copy of the tint undertone's hue sweep as it stood before any perf
    // optimisation, plus the bit-identity test that gates the C2 coarse-to-fine
    // rework of `cusp_attracted_hue`. Any change to the emitted hue would move a
    // tint hex value — forbidden on this branch — so the test pins the selected
    // hue to full f64 `to_bits()` identity over a dense (l_ok, canonical) grid.
    // The reference calls the PRODUCTION `scale::max_chroma`, isolating the
    // *selection* logic from the solver internals.
    // ─────────────────────────────────────────────────────────────────────────

    /// FROZEN reference: the flat 81-point cusp sweep exactly as it selected the
    /// undertone hue at the base of this branch.
    fn cusp_attracted_hue_reference(l_ok: f64, canonical_deg: f64, stiffness: f64) -> f64 {
        let penalty_scale = stiffness / 100.0;
        let mut best_h = canonical_deg;
        let mut best_score = f64::NEG_INFINITY;
        let steps = (CUSP_HALF_WINDOW_DEG * 2.0) as i32;
        for i in 0..=steps {
            let h = canonical_deg - CUSP_HALF_WINDOW_DEG + i as f64;
            let chroma = scale::max_chroma(l_ok, h);
            let drift = (h - canonical_deg).abs();
            let score = chroma - penalty_scale * drift;
            if score > best_score {
                best_score = score;
                best_h = h;
            }
        }
        best_h
    }

    /// Diff test over a grid: production `cusp_attracted_hue` must select the
    /// bit-identical undertone hue the frozen flat scan does, at the production
    /// tint stiffness, across `l_ok` and canonical hue.
    fn assert_cusp_hue_matches_reference(l_steps: usize, h_step_deg: usize) -> usize {
        let stiffness = TINT_HUE_STIFFNESS;
        let mut points = 0usize;
        for li in 0..=l_steps {
            let l = li as f64 / l_steps as f64;
            let mut hc = 0usize;
            while hc < 360 {
                // Integer canonical PLUS fractional offsets: the production tint
                // canonical (286°) is integer, but a consumer brand hue is not, so
                // testing hcd + {0, 0.25, 0.5} closes the aliasing-shift class the
                // integer grid alone cannot.
                for frac in [0.0, 0.25, 0.5] {
                    let hcd = hc as f64 + frac;
                    let prod = cusp_attracted_hue(l, hcd, stiffness);
                    let refv = cusp_attracted_hue_reference(l, hcd, stiffness);
                    assert_eq!(
                        prod.to_bits(),
                        refv.to_bits(),
                        "cusp_attracted_hue drift at (L={l}, canon={hcd}): prod={prod} ref={refv}"
                    );
                    points += 1;
                }
                hc += h_step_deg;
            }
        }
        points
    }

    #[test]
    fn diff_cusp_hue_matches_frozen_reference_fast() {
        // 101 L × 72 canonical-hue × 3 fractional offsets = 21 816 points.
        let n = assert_cusp_hue_matches_reference(100, 5);
        assert_eq!(n, 101 * 72 * 3);
    }

    #[test]
    #[ignore = "full grid × 3 offsets — run with `--ignored`; slow at opt-level 0"]
    fn diff_cusp_hue_matches_frozen_reference_full() {
        // 501 L × 360 canonical-hue × 3 fractional offsets = 541 080 points.
        let n = assert_cusp_hue_matches_reference(500, 1);
        assert_eq!(n, 501 * 360 * 3);
    }

    #[test]
    fn measure_contrast_reproduces_solved_lc_and_boundary_ratio() {
        // The recheck primitive must agree with the solver's `finish` measurement
        // for Lc and derive the frozen ratio report from the exact same final
        // bytes. This identity is the foundation of the lazy/hysteresis controller.
        use crate::spaces::srgb::srgb_from_hex;
        let table = RoleTable::default();
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for bg_hex in [
                "#FFFFFF", "#3478F6", "#1C1C1E", "#7F7F7F", "#101012", "#B5482E",
            ] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let bg_lin = srgb_from_hex(bg_hex).unwrap();
                for (role, resolved) in resolve_set(&bg, &table, &vc) {
                    let Some(solved) = resolved.solved() else {
                        continue;
                    };
                    let fg_lin = srgb_from_hex(solved.hex()).unwrap();
                    let (lc, wcag) = measure_contrast(bg_lin, fg_lin, &vc);
                    assert!(
                        (lc - solved.lc()).abs() < 1e-9,
                        "{role:?} on {bg_hex}: recheck lc {lc} != solver {}",
                        solved.lc()
                    );
                    // W5: солвер не отчитывает wcag_ratio; сверяем recheck-ратио
                    // с независимым continuous-пересчётом на тех же байтах.
                    let independent = crate::spaces::srgb::encoded_srgb_contrast_ratio(
                        crate::solve::quantised_display(fg_lin),
                        crate::solve::quantised_display(bg_lin),
                    );
                    assert!(
                        (wcag - independent).abs() < 1e-9,
                        "{role:?} on {bg_hex}: recheck wcag {wcag} != independent {independent}"
                    );
                }
            }
        }
    }

    #[test]
    fn recheck_against_batch_matches_per_pair_and_the_solver() {
        // The batch recheck (shared bg) must give exactly the same (lc, wcag) as
        // the single-pair `measure_contrast`; Lc also equals the solver report,
        // while WCAG remains a boundary projection from final bytes.
        use crate::spaces::srgb::srgb_from_hex;
        let table = RoleTable::default();
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for bg_hex in ["#FFFFFF", "#3478F6", "#1C1C1E", "#7F7F7F", "#A23E8C"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let bg_lin = srgb_from_hex(bg_hex).unwrap();
                let set = resolve_set(&bg, &table, &vc);
                let fg_hexes: Vec<&str> = set
                    .iter()
                    .filter_map(|(_, r)| r.solved().map(|s| s.hex()))
                    .collect();
                let batch = recheck_against(bg_hex, &fg_hexes, &vc).unwrap();
                let solved: Vec<_> = set.iter().filter_map(|(_, r)| r.solved()).collect();
                assert_eq!(batch.len(), solved.len());
                for (i, s) in solved.iter().enumerate() {
                    let fg_lin = srgb_from_hex(s.hex()).unwrap();
                    let single = measure_contrast(bg_lin, fg_lin, &vc);
                    assert_eq!(batch[i], single, "{bg_hex}: batch != single-pair");
                    assert!((batch[i].0 - s.lc()).abs() < 1e-9, "{bg_hex}: lc != solver");
                }
            }
        }
        // Invalid hex surfaces an Err, not a panic.
        assert!(recheck_against("#FFFFFF", &["nothex"], &ViewingConditions::srgb()).is_err());
    }

    #[test]
    fn recheck_against_multi_is_byte_identical_to_per_bg_recheck() {
        // The multi-background recheck (fg forwards shared across samples) must be
        // byte-identical, pair for pair, to calling recheck_against once per
        // background — only the loop nesting differs, never the arithmetic.
        let table = RoleTable::default();
        let bgs = ["#38383A", "#404042", "#2E2E30", "#FFFFFF", "#000000"];
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            // A representative foreground set: one solved theme's colours.
            let seed = BgInput::solid("#3A3A3C").unwrap();
            let set = resolve_set(&seed, &table, &vc);
            let fg_hexes: Vec<&str> = set
                .iter()
                .filter_map(|(_, r)| r.solved().map(|s| s.hex()))
                .collect();
            let multi = recheck_against_multi(&bgs, &fg_hexes, &vc).unwrap();
            assert_eq!(multi.len(), bgs.len() * fg_hexes.len() * 2);
            for (s, bg) in bgs.iter().enumerate() {
                let per = recheck_against(bg, &fg_hexes, &vc).unwrap();
                for (i, (lc, wcag)) in per.iter().enumerate() {
                    let base = (s * fg_hexes.len() + i) * 2;
                    assert_eq!(multi[base].to_bits(), lc.to_bits(), "{bg}: fg {i} lc drift");
                    assert_eq!(
                        multi[base + 1].to_bits(),
                        wcag.to_bits(),
                        "{bg}: fg {i} wcag drift"
                    );
                }
            }
        }
        // Invalid hex in either position surfaces an Err, not a panic.
        assert!(
            recheck_against_multi(&["#FFFFFF"], &["nothex"], &ViewingConditions::srgb()).is_err()
        );
        assert!(recheck_against_multi(&["bad"], &["#FFFFFF"], &ViewingConditions::srgb()).is_err());
    }

    #[test]
    fn recheck_against_ignores_alpha() {
        // CHARACTERIZATION: the colour-only recheck has no occurrence model.
        // Given a translucent tint's hex it forwards the tint's OWN display
        // luminance — never the alpha-composite over the backdrop. This pins the
        // exact gap the full-support occurrence descriptor (C8d step 2) closes:
        // the readability recheck must composite the tint at its alpha before
        // measuring, which this path structurally cannot.
        let vc = ViewingConditions::srgb();
        let bg = "#000000";
        let tint = "#FFFFFF";
        let alpha = 0.6;

        let seen = recheck_against(bg, &[tint], &vc).unwrap()[0];

        // What it reports is exactly the tint-vs-backdrop contrast.
        let rl_tint = crate::spaces::srgb::encoded_srgb_relative_luminance(
            crate::spaces::srgb::srgb_encoded_from_hex(tint).unwrap(),
        );
        let rl_bg = crate::spaces::srgb::encoded_srgb_relative_luminance(
            crate::spaces::srgb::srgb_encoded_from_hex(bg).unwrap(),
        );
        assert_eq!(
            seen.0.to_bits(),
            crate::lpc::contrast_core(rl_tint, rl_bg).to_bits()
        );
        assert_eq!(
            seen.1.to_bits(),
            crate::spaces::srgb::relative_luminance_ratio(rl_tint, rl_bg).to_bits()
        );

        // The real composite (#FFFFFF @0.6 over black) is a materially different
        // colour with a different contrast the colour-only path never observes.
        let composite =
            crate::composition::source_over_srgb8([255, 255, 255], alpha, [0, 0, 0]).unwrap();
        let rl_comp = crate::spaces::srgb::encoded_srgb_relative_luminance(
            crate::Srgb8::new(composite).encoded(),
        );
        assert_ne!(
            seen.1.to_bits(),
            crate::spaces::srgb::relative_luminance_ratio(rl_comp, rl_bg).to_bits()
        );
    }

    /// The packed `0x00RRGGBB` corpus: canonical 6-hex spelling paired with the
    /// word it encodes. `#ABC` shorthand is a boundary affordance the core does
    /// not accept, so its *expanded* form `#AABBCC` stands in for it here — the
    /// byte-identity claim is over the octets, and `#ABC`→`#AABBCC`→`0xAABBCC`.
    /// `#0057BB` is drawn from the wasm-boundary golden foregrounds.
    const PACKED_CORPUS: [(&str, u32); 8] = [
        ("#000000", 0x000000),
        ("#FFFFFF", 0xFFFFFF),
        ("#AABBCC", 0xAABBCC),
        ("#0057BB", 0x0057BB),
        ("#3478F6", 0x3478F6),
        ("#1C1C1E", 0x1C1C1E),
        ("#7F7F7F", 0x7F7F7F),
        ("#A23E8C", 0xA23E8C),
    ];

    #[test]
    fn packed_u32_recheck_is_byte_identical_to_hex() {
        // The packed entry point must return bit-identical (lc, wcag) to the hex
        // path over the corpus — byte-identity by construction, checked on the
        // raw bits so no rounding can hide a drift. Every colour serves as both
        // the shared background and a foreground.
        let vc = ViewingConditions::srgb();
        let hexes: Vec<&str> = PACKED_CORPUS.iter().map(|(h, _)| *h).collect();
        let words: Vec<u32> = PACKED_CORPUS.iter().map(|(_, u)| *u).collect();
        for (bg_hex, bg_u32) in PACKED_CORPUS {
            let hex_out = recheck_against(bg_hex, &hexes, &vc).unwrap();
            let u32_out = recheck_against_u32(bg_u32, &words, &vc).unwrap();
            assert_eq!(hex_out.len(), u32_out.len());
            for (i, ((lc_h, wc_h), (lc_u, wc_u))) in hex_out.iter().zip(u32_out.iter()).enumerate()
            {
                assert_eq!(
                    lc_h.to_bits(),
                    lc_u.to_bits(),
                    "bg {bg_hex}: fg {i} lc drift"
                );
                assert_eq!(
                    wc_h.to_bits(),
                    wc_u.to_bits(),
                    "bg {bg_hex}: fg {i} wcag drift"
                );
            }
        }
    }

    #[test]
    fn recheck_batch_cardinality_is_checked_before_allocation() {
        assert_eq!(checked_recheck_output_len(0, usize::MAX).unwrap(), 0);
        assert_eq!(checked_recheck_output_len(2, 3).unwrap(), 12);
        assert!(checked_recheck_output_len(usize::MAX, 2).is_err());
        assert!(checked_recheck_output_len(usize::MAX / 2 + 1, 1).is_err());
    }

    #[test]
    fn packed_u32_multi_recheck_is_byte_identical_to_hex() {
        // The packed multi-background entry point mirrors recheck_against_multi
        // bit-for-bit, including the background-major flat layout.
        let vc = ViewingConditions::srgb();
        let hexes: Vec<&str> = PACKED_CORPUS.iter().map(|(h, _)| *h).collect();
        let words: Vec<u32> = PACKED_CORPUS.iter().map(|(_, u)| *u).collect();
        let hex_flat = recheck_against_multi(&hexes, &hexes, &vc).unwrap();
        let u32_flat = recheck_against_multi_u32(&words, &words, &vc).unwrap();
        assert_eq!(hex_flat.len(), u32_flat.len());
        for (i, (h, u)) in hex_flat.iter().zip(u32_flat.iter()).enumerate() {
            assert_eq!(h.to_bits(), u.to_bits(), "flat entry {i} drift");
        }
    }

    #[test]
    fn packed_u32_decodes_rrggbb_channel_order() {
        // N2: pack(0x00RRGGBB) must decode to [RR, GG, BB] — a BBGGRR or RGBA
        // regression would make #0057BB read as #BB5700 (or shift a channel), so
        // it must match its own hex spelling and NOT the reversed one.
        let vc = ViewingConditions::srgb();
        let bg = 0xFFFFFF;
        let forward = recheck_against_u32(bg, &[0x0057BB], &vc).unwrap();
        let same = recheck_against("#FFFFFF", &["#0057BB"], &vc).unwrap();
        assert_eq!(
            forward[0].0.to_bits(),
            same[0].0.to_bits(),
            "lc channel order"
        );
        assert_eq!(
            forward[0].1.to_bits(),
            same[0].1.to_bits(),
            "wcag channel order"
        );
        // The byte-reversed spelling is a genuinely different colour: guards a
        // silent channel swap that would otherwise pass a self-consistent metric.
        let reversed = recheck_against("#FFFFFF", &["#BB5700"], &vc).unwrap();
        assert_ne!(
            forward[0].1.to_bits(),
            reversed[0].1.to_bits(),
            "0x0057BB must not decode to #BB5700"
        );
    }

    #[test]
    fn packed_u32_high_byte_required_zero() {
        // The high byte is reserved: a non-zero one (an RGBA/ARGB word leaking
        // in) is rejected cheaply, never silently truncated to the low 24 bits.
        let vc = ViewingConditions::srgb();
        assert!(recheck_against_u32(0x0100_0000, &[0x000000], &vc).is_err());
        assert!(recheck_against_u32(0xFFFF_FFFF, &[0x000000], &vc).is_err());
        assert!(recheck_against_u32(0x000000, &[0xFF00_0000], &vc).is_err());
        assert!(recheck_against_multi_u32(&[0x0000_0000], &[0x0100_0000], &vc).is_err());
        assert!(recheck_against_multi_u32(&[0xAB00_0000], &[0x000000], &vc).is_err());
    }

    /// The 12 mid-to-light nodes of the owner's reference neutral ramp (pure
    /// #FFFFFF dropped — it is achromatic). The VALIDATION set, never an input.
    const REFERENCE_NODES: [&str; 12] = [
        "#101012", "#151518", "#212125", "#303136", "#44444B", "#5B5C64", "#787881", "#9698A2",
        "#B3B5BF", "#CDD0D9", "#E4E7ED", "#F6F8FA",
    ];

    /// One reference node measured in the engine's spaces: Oklab lightness and
    /// hue, plus CAM16-UCS colourfulness `M'`.
    fn node_measure(hex: &str, vc: &ViewingConditions) -> (f64, f64, f64) {
        use crate::spaces::oklab::{oklab_hue, srgb_linear_to_oklab};
        use crate::spaces::srgb::srgb_from_hex;
        let rgb = srgb_from_hex(hex).unwrap();
        let l = srgb_linear_to_oklab(rgb)[0];
        let hue = oklab_hue(rgb);
        let mp = crate::lcs::LcsColor::from_hex_with_vc(hex, vc)
            .unwrap()
            .mp();
        (l, hue, mp)
    }

    /// What the v2 curve produces at a given lightness: hue and `M'`.
    fn curve_measure(l: f64, vc: &ViewingConditions) -> (f64, f64) {
        use crate::spaces::oklab::oklab_hue;
        use crate::spaces::srgb::hex_from_srgb;
        let h = cusp_attracted_hue(l, NEUTRAL_HUE_DEG, TINT_HUE_STIFFNESS);
        let r = ratio_for_target_mp(l, h, TINT_TARGET_MP, vc);
        let rgb = build_curve_color(l, h, r);
        let curve_hue = oklab_hue(rgb);
        let curve_mp = crate::lcs::LcsColor::from_hex_with_vc(&hex_from_srgb(rgb), vc)
            .unwrap()
            .mp();
        (curve_hue, curve_mp)
    }

    #[test]
    fn target_mp_is_not_artificially_raised_to_the_perceptibility_floor() {
        let vc = ViewingConditions::srgb();
        let l = 0.62;
        let h = NEUTRAL_HUE_DEG;
        let low = ratio_for_target_mp(l, h, 0.5, &vc);
        let higher = ratio_for_target_mp(l, h, 1.5, &vc);

        assert!(
            low < higher,
            "представимые target_mp=0.5 и target_mp=1.5 не должны молча превращаться в одну policy: {low} vs {higher}"
        );
    }

    #[test]
    fn named_table_rejects_invalid_chroma_policies_without_partial_output() {
        let bg = BgInput::solid("#FFFFFF").expect("контрольный фон валиден");
        let vc = ViewingConditions::srgb();
        let entries = || {
            vec![
                ("first".to_owned(), RoleSpec::Zero),
                ("second".to_owned(), RoleSpec::Zero),
            ]
        };
        let resolve_valid = |chroma| {
            let table = NamedRoleTable::new(entries(), Vec::new(), chroma)
                .expect("контрольная policy валидна");
            resolve_named_set(&bg, &table, &vc)
        };

        let invalid = [
            RoleChroma::Tinted {
                hue_deg: f64::NAN,
                ratio: 0.5,
            },
            RoleChroma::Tinted {
                hue_deg: -f64::EPSILON,
                ratio: 0.5,
            },
            RoleChroma::Tinted {
                hue_deg: 360.0,
                ratio: 0.5,
            },
            RoleChroma::Tinted {
                hue_deg: 0.0,
                ratio: -f64::EPSILON,
            },
            RoleChroma::Tinted {
                hue_deg: 0.0,
                ratio: 1.0 + f64::EPSILON,
            },
            RoleChroma::Curve {
                canonical_hue_deg: f64::INFINITY,
                target_mp: 1.0,
                hue_stiffness: 0.0,
            },
            RoleChroma::Curve {
                canonical_hue_deg: -f64::EPSILON,
                target_mp: 1.0,
                hue_stiffness: 0.0,
            },
            RoleChroma::Curve {
                canonical_hue_deg: 360.0,
                target_mp: 1.0,
                hue_stiffness: 0.0,
            },
            RoleChroma::Curve {
                canonical_hue_deg: 1.0e308,
                target_mp: 1.0,
                hue_stiffness: 0.0,
            },
            RoleChroma::Curve {
                canonical_hue_deg: 0.0,
                target_mp: 0.0,
                hue_stiffness: 0.0,
            },
            RoleChroma::Curve {
                canonical_hue_deg: 0.0,
                target_mp: f64::NAN,
                hue_stiffness: 0.0,
            },
            RoleChroma::Curve {
                canonical_hue_deg: 0.0,
                target_mp: 1.0,
                hue_stiffness: -f64::EPSILON,
            },
            RoleChroma::Curve {
                canonical_hue_deg: 0.0,
                target_mp: 1.0,
                hue_stiffness: f64::INFINITY,
            },
        ];

        for chroma in invalid {
            assert!(
                matches!(
                    NamedRoleTable::new(Vec::new(), Vec::new(), chroma),
                    Err(SolveFailure::InvalidInput(_))
                ),
                "даже пустая таблица не должна скрывать некорректную policy: {chroma:?}"
            );
        }

        for chroma in [
            RoleChroma::Tinted {
                hue_deg: 0.0,
                ratio: 0.0,
            },
            RoleChroma::Tinted {
                hue_deg: 359.999,
                ratio: 1.0,
            },
            RoleChroma::Tinted {
                hue_deg: f64::from_bits(360.0_f64.to_bits() - 1),
                ratio: 1.0,
            },
            RoleChroma::Curve {
                canonical_hue_deg: 0.0,
                target_mp: f64::MIN_POSITIVE,
                hue_stiffness: 0.0,
            },
            RoleChroma::Curve {
                canonical_hue_deg: f64::from_bits(360.0_f64.to_bits() - 1),
                target_mp: 1.0,
                hue_stiffness: 0.0,
            },
        ] {
            assert!(
                resolve_valid(chroma)
                    .expect("valid chroma boundary resolves atomically")
                    .iter()
                    .all(|(_, outcome)| *outcome == Resolved::None),
                "валидная граница отвергнута"
            );
        }
    }

    #[test]
    fn curve_fits_reference_plateau_colorfulness() {
        // VALIDATION (owner reference, tint-identity-curve). On the reference's
        // colourfulness PLATEAU (L in [0.45, 0.90], where the ramp holds ~constant
        // M' and the gamut has room) the curve's constant-M' policy must track it
        // tightly. This is the quality metric the PR body reports — the reference is
        // never an input, only the yardstick. The two ENDS are deliberately not
        // asserted to match: the dark end (L < 0.45) and the near-white end
        // (L > 0.90) both release colourfulness by hand in the reference, while the
        // UCS-constant policy holds it — an honest, documented divergence (the
        // mechanism-3 release happens only where the gamut wall forces it).
        //
        // TINT_TARGET_MP = 6.1 sits (essentially) at the constant that minimises the
        // root-mean-square residual of curve M' against the reference's plateau nodes.
        // The in-crate test deliberately owns both fixture and engine path: no
        // public reproduction hook may leak these client nodes into Core.
        let vc = ViewingConditions::srgb();
        let mut max_resid = 0.0_f64;
        for hex in REFERENCE_NODES {
            let (l, _ref_hue, ref_mp) = node_measure(hex, &vc);
            if !(0.45..=0.90).contains(&l) {
                continue;
            }
            let (_curve_hue, curve_mp) = curve_measure(l, &vc);
            let resid = (curve_mp - ref_mp).abs();
            max_resid = max_resid.max(resid);
            assert!(
                resid <= 1.0,
                "{hex} (L {l:.3}): curve M' {curve_mp:.2} strays from reference {ref_mp:.2}"
            );
        }
        // The plateau fit is tight — well inside one M' unit of colourfulness.
        assert!(
            max_resid <= 1.0,
            "plateau colourfulness residual {max_resid:.2} too large"
        );
    }

    #[test]
    fn curve_holds_canonical_hue_where_geometry_allows() {
        // VALIDATION (hue path). The reference holds ~286 on its dark and mid nodes
        // and only drifts to azure (264->248) at the two lightest nodes — a drift
        // the sRGB gamut geometry does NOT offer (the local chroma cusp moves the
        // OTHER way, toward magenta, at high L; measured 2026-06-12). So the honest,
        // geometry-derived result is a hue pinned near canonical 286 across the
        // ladder: it matches the reference everywhere the reference itself stays
        // canonical, and is explicitly allowed to diverge at the two azure-drifting
        // light nodes. This test asserts the match on the canonical-hue nodes and
        // documents the divergence at the azure nodes rather than faking the drift.
        let vc = ViewingConditions::srgb();
        for hex in REFERENCE_NODES {
            let (l, ref_hue, ref_mp) = node_measure(hex, &vc);
            if ref_mp <= 3.0 {
                continue; // faint node — hue is float-fragile, skip
            }
            let (curve_hue, _curve_mp) = curve_measure(l, &vc);
            let ref_drift = ((ref_hue - NEUTRAL_HUE_DEG + 180.0).rem_euclid(360.0)) - 180.0;
            let to_ref = ((curve_hue - ref_hue + 180.0).rem_euclid(360.0)) - 180.0;
            if ref_drift.abs() <= 12.0 {
                // The reference is near-canonical here — the curve must match it.
                assert!(
                    to_ref.abs() <= 12.0,
                    "{hex} (L {l:.3}): curve hue {curve_hue:.1} off reference {ref_hue:.1}"
                );
            }
            // Where the reference drifts to azure (|ref_drift| > 12), the curve
            // honestly stays near canonical — no assertion forces a drift the
            // geometry cannot produce.
            assert!(
                ((curve_hue - NEUTRAL_HUE_DEG + 180.0).rem_euclid(360.0) - 180.0).abs() <= 12.0,
                "{hex}: curve hue {curve_hue:.1} left the canonical blue-violet band"
            );
        }
    }

    fn vcs() -> [(ViewingConditions, &'static str); 2] {
        [
            (ViewingConditions::srgb(), "srgb"),
            (ViewingConditions::dim_surround(), "dim"),
        ]
    }

    /// Backgrounds with enough headroom in both VCs that every text role is
    /// reachable — the grid the ordering and polarity invariants run on.
    const REACHABLE_BGS: [&str; 6] = [
        "#FFFFFF", "#F7F8FA", "#EBEBF5", // light end of the neutral ladder
        "#101012", "#1C1C1E", "#242426", // dark end
    ];

    /// The four text roles, strongest first — the visual-weight order the
    /// hierarchy invariant asserts on.
    const TEXT_ORDER: [Role; 4] = [
        Role::LabelPrimary,
        Role::LabelSecondary,
        Role::LabelTertiary,
        Role::LabelQuaternary,
    ];

    /// The neutral band where the WCAG flip lives (~#747474) and where the old
    /// LPC-flip rule (~#999999) chose an unreachable polarity — the stripe
    /// BLOCKER 1 was about. Stepped one 8-bit quantum at a time, plus the two
    /// off-neutral cases (#93939C, #3478F6) from the diagnosis.
    #[test]
    fn hierarchy_never_inverts_on_found_counterexamples() {
        // Verification counterexamples: the floor used to lift the junior onto a
        // grid point ABOVE its senior (#727272/srgb, #0066FF/dim). The senior-copy
        // rule must keep `primary >= secondary` (equality allowed, flagged).
        for (bg_hex, vc) in [
            ("#727272", ViewingConditions::srgb()),
            ("#0066FF", ViewingConditions::dim_surround()),
            ("#6666CC", ViewingConditions::dim_surround()),
        ] {
            let bg = BgInput::solid(bg_hex).unwrap();
            let set = resolve_set(&bg, &RoleTable::default(), &vc);
            let mag = |role: Role| -> Option<f64> {
                set.iter().find_map(|(r, res)| match res {
                    Resolved::Color { solved, .. } if *r == role => Some(solved.lc().abs()),
                    _ => None,
                })
            };
            if let (Some(p), Some(sec)) = (mag(Role::LabelPrimary), mag(Role::LabelSecondary)) {
                assert!(
                    p + 1e-9 >= sec,
                    "{bg_hex}: primary {p} must not be weaker than secondary {sec}"
                );
            }
        }
    }

    #[test]
    fn declaration_order_does_not_override_an_independent_floor() {
        // Adjacent opaque IDs carry no hierarchy relation. Each declared floor is
        // solved independently until an explicit graph edge says otherwise.
        let table = NamedRoleTable::new(
            vec![
                (
                    "senior".into(),
                    RoleSpec::Anchor(TextAnchor::new(0.4, Floor::AaUi).unwrap()),
                ),
                (
                    "junior".into(),
                    RoleSpec::Anchor(TextAnchor::new(0.3, Floor::AaText).unwrap()),
                ),
            ],
            Vec::new(),
            RoleChroma::Neutral,
        )
        .unwrap();
        let bg = BgInput::solid("#2E2E2E").unwrap();
        let set = resolve_named_set(&bg, &table, &ViewingConditions::srgb())
            .expect("valid hierarchy fixture resolves atomically");
        let junior = set
            .iter()
            .find_map(|(name, resolved)| (name == "junior").then_some(resolved))
            .unwrap();
        let Resolved::Color {
            solved, compressed, ..
        } = junior
        else {
            panic!("junior must remain a typed colour outcome, got {junior:?}");
        };

        assert!(
            !compressed,
            "declaration order must not invent a hierarchy or compression outcome"
        );
        // W5: солвер не поднимает цвет юр. полом; нормативный вердикт финальной
        // пары принадлежит каноническому WCAG22 evaluator-у. Здесь фиксируем
        // независимость от порядка объявления, а не solver-владение полом.
        assert!(
            solved.lc().is_finite(),
            "junior must stay a measured colour: {}",
            solved.hex()
        );
    }

    #[test]
    fn polarity_tie_break_is_vc_independent_at_the_seam() {
        // #757575/#767676 straddle the equal-ratio crossover; the chosen
        // polarity must be identical under both viewing conditions.
        for bg_hex in ["#757575", "#767676", "#747474"] {
            let bg = BgInput::solid(bg_hex).unwrap();
            let srgb = ResolveContext::new(&bg, &ViewingConditions::srgb()).polarity;
            let dim = ResolveContext::new(&bg, &ViewingConditions::dim_surround()).polarity;
            assert_eq!(srgb, dim, "{bg_hex}: polarity must not depend on VC");
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // CH-4a — polarity tie-break: both legal ⇒ light-on-dark (derived, not tuned).
    //
    // Black clears AA 4.5:1 at Y ≥ 0.175, white at Y ≤ 0.1833, so both are legal
    // only on the narrow band Y ∈ [0.175, 0.1833]. The former rule broke the tie
    // on the larger symmetric WCAG margin, which crosses over INSIDE the band at
    // Y ≈ 0.1791 (solve (Y+0.05)/0.05 = 1.05/(Y+0.05)); above it black's margin
    // wins, so the old engine emitted DARK on Y ∈ (0.1791, 0.1833] — the
    // perceptually weaker side there (contrast_core's black-overtakes-white
    // crossover sits near Y ≈ 0.342, measured) and the one that made Fluent #0078D4 resolve
    // black. The derived rule takes light-on-dark across the whole band.
    // ─────────────────────────────────────────────────────────────────────────

    /// WCAG relative luminance of a solid background's quantised display bytes —
    /// the exact number the polarity gate reads (mirrors `bg_display`).
    fn bg_luminance(bg_hex: &str) -> f64 {
        let bg = BgInput::solid(bg_hex).unwrap();
        crate::spaces::srgb::encoded_srgb_relative_luminance(bg_display(&bg))
    }

    /// The FROZEN pre-CH-4a tie-break — larger symmetric WCAG margin wins, exact
    /// tie to dark-on-light. Kept as the emission-diff ORACLE so the sweep can
    /// enumerate exactly which backgrounds the derivation moved. Do not "improve":
    /// it is a fixed historical reference (what `main` emits), not live policy.
    fn choose_polarity_margin_oracle(bg: &BgInput) -> Polarity {
        let disp = bg_display(bg);
        let dol = crate::spaces::srgb::encoded_srgb_contrast_ratio([0.0, 0.0, 0.0], disp);
        let lod = crate::spaces::srgb::encoded_srgb_contrast_ratio([1.0, 1.0, 1.0], disp);
        let dol_clears = dol + 1e-9 >= POLARITY_FLOOR_RATIO;
        let lod_clears = lod + 1e-9 >= POLARITY_FLOOR_RATIO;
        match (dol_clears, lod_clears) {
            (true, false) => Polarity::DarkOnLight,
            (false, true) => Polarity::LightOnDark,
            (true, true) => {
                // The old margin rule: larger WCAG margin, exact tie to dark.
                if (lod - dol) > 1e-6 {
                    Polarity::LightOnDark
                } else {
                    Polarity::DarkOnLight
                }
            }
            (false, false) => {
                if dol >= lod {
                    Polarity::DarkOnLight
                } else {
                    Polarity::LightOnDark
                }
            }
        }
    }

    /// Resolve every role under a FORCED polarity, mirroring `ResolveContext::new`
    /// but substituting `polarity`. Because the tie-break is the *only* code the
    /// derivation changed, forcing the margin oracle's polarity reproduces exactly
    /// what `main` emits (the "before" of the emission diff) and forcing the live
    /// polarity reproduces `resolve` — so the sweep can compare the two bit-for-bit.
    fn resolve_all_with_polarity(
        bg: &BgInput,
        vc: &ViewingConditions,
        polarity: Polarity,
    ) -> Vec<(Role, Resolved)> {
        let interval = bg.luma_interval(vc);
        let max = interval
            .as_ref()
            .map_err(Clone::clone)
            .and_then(|iv| max_contrast(bg, polarity, vc, *iv));
        let ctx = ResolveContext {
            polarity,
            max_contrast: max,
            interval,
            high_contrast: vc.high_contrast,
        };
        let table = RoleTable::default();
        Role::ALL
            .iter()
            .map(|&r| {
                let resolved = admit_resolution(resolve_in(bg, r, &table, vc, &ctx))
                    .expect("forced valid fixture has no whole-set failure");
                (r, resolved)
            })
            .collect()
    }

    /// One role under a forced polarity — prints the emission diff's before/after
    /// primary hex.
    fn resolve_forced(
        bg: &BgInput,
        role: Role,
        vc: &ViewingConditions,
        polarity: Polarity,
    ) -> Resolved {
        resolve_all_with_polarity(bg, vc, polarity)
            .into_iter()
            .find(|(r, _)| *r == role)
            .map(|(_, res)| res)
            .unwrap()
    }

    /// 256 neutrals + a dense chromatic byte-cube + named design tokens near the
    /// band. The polarity gate is a pure function of the display bytes, so a
    /// byte-space cube probes the whole chromatic input space directly.
    fn emission_diff_grid() -> Vec<String> {
        let mut g: Vec<String> = (0u32..=255)
            .map(|c| format!("#{c:02X}{c:02X}{c:02X}"))
            .collect();
        for r in (0u32..=255).step_by(16) {
            for gg in (0u32..=255).step_by(16) {
                for b in (0u32..=255).step_by(16) {
                    if r == gg && gg == b {
                        continue; // neutrals already present
                    }
                    g.push(format!("#{r:02X}{gg:02X}{b:02X}"));
                }
            }
        }
        for t in [
            "#0078D4", "#3478F6", "#007AFF", "#0066FF", "#2F80ED", "#1E88E5", "#0A84FF",
        ] {
            g.push(t.to_string());
        }
        g.sort();
        g.dedup();
        g
    }

    #[test]
    fn fluent_blue_0078d4_resolves_light_on_dark() {
        // #0078D4 (Y ≈ 0.1818) is double-legal: white 4.529:1, black 4.637:1 —
        // both clear AA. The old margin rule took black (larger margin); the
        // derived rule takes white, matching the Fluent convention.
        let bg = BgInput::solid("#0078D4").unwrap();
        let disp = bg_display(&bg);
        let d = crate::spaces::srgb::encoded_srgb_contrast_ratio([0.0, 0.0, 0.0], disp);
        let w = crate::spaces::srgb::encoded_srgb_contrast_ratio([1.0, 1.0, 1.0], disp);
        assert!(
            d >= 4.5 && w >= 4.5,
            "premise: #0078D4 must be double-legal (dark {d:.3}, white {w:.3})"
        );
        assert_eq!(
            choose_polarity(&bg),
            Polarity::LightOnDark,
            "#0078D4 is double-legal; the derived tie-break must pick white"
        );
        // The whole resolved set is light-on-dark (primary lc < 0), both VCs.
        for (vc, name) in vcs() {
            let lc = solved_lc(&bg, Role::LabelPrimary, &vc);
            assert!(
                lc < 0.0,
                "{name} #0078D4: primary must be light-on-dark, got lc {lc}"
            );
        }
    }

    #[test]
    fn choose_polarity_covers_all_four_legality_branches() {
        // (true,false) — only dark-on-light legal (white-on-white is 1:1).
        assert_eq!(
            choose_polarity(&BgInput::solid("#FFFFFF").unwrap()),
            Polarity::DarkOnLight
        );
        // (false,true) — only light-on-dark legal (black-on-near-black < 4.5:1).
        assert_eq!(
            choose_polarity(&BgInput::solid("#101012").unwrap()),
            Polarity::LightOnDark
        );
        // (true,true) — both legal on the band → white by derivation.
        assert_eq!(
            choose_polarity(&BgInput::solid("#767676").unwrap()),
            Polarity::LightOnDark
        );
        assert_eq!(
            choose_polarity(&BgInput::solid("#0078D4").unwrap()),
            Polarity::LightOnDark
        );
        // (false,false) is unreachable on solid sRGB: for every Y at least one of
        // black (Y≥0.175) / white (Y≤0.1833) clears 4.5:1, and the two half-lines
        // cover the whole axis. Prove the dead arm stays dead over the full grid.
        for hex in emission_diff_grid() {
            let bg = BgInput::solid(&hex).unwrap();
            let disp = bg_display(&bg);
            let d = crate::spaces::srgb::encoded_srgb_contrast_ratio([0.0, 0.0, 0.0], disp);
            let w = crate::spaces::srgb::encoded_srgb_contrast_ratio([1.0, 1.0, 1.0], disp);
            assert!(
                d + 1e-9 >= 4.5 || w + 1e-9 >= 4.5,
                "{hex}: neither polarity clears 4.5:1 — (false,false) must be unreachable"
            );
        }
    }

    #[test]
    fn emission_diff_is_exactly_the_upper_double_legal_band_and_prints_the_table() {
        let mut flips: Vec<(String, Polarity, Polarity, f64)> = Vec::new();
        for hex in emission_diff_grid() {
            let bg = BgInput::solid(&hex).unwrap();
            let new = choose_polarity(&bg);
            let old = choose_polarity_margin_oracle(&bg);
            if new != old {
                flips.push((hex.clone(), old, new, bg_luminance(&hex)));
            }
        }
        // Every moved background is a dark→light flip on the upper double-legal
        // band Y ∈ (0.1791, 0.1833], both sides genuinely legal.
        for (hex, old, new, y) in &flips {
            assert_eq!(
                *old,
                Polarity::DarkOnLight,
                "{hex}: old polarity not dark-on-light"
            );
            assert_eq!(
                *new,
                Polarity::LightOnDark,
                "{hex}: new polarity not light-on-dark"
            );
            assert!(
                *y > 0.1791 && *y <= 0.1834,
                "{hex}: Y {y:.6} outside the moved band (0.1791, 0.1833]"
            );
            let bg = BgInput::solid(hex).unwrap();
            let disp = bg_display(&bg);
            let d = crate::spaces::srgb::encoded_srgb_contrast_ratio([0.0, 0.0, 0.0], disp);
            let w = crate::spaces::srgb::encoded_srgb_contrast_ratio([1.0, 1.0, 1.0], disp);
            assert!(
                d + 1e-9 >= 4.5 && w + 1e-9 >= 4.5,
                "{hex}: tie premise broken (dark {d:.4}, white {w:.4})"
            );
        }
        // The move is real and includes the canonical neutral and chromatic cases.
        assert!(
            flips.iter().any(|f| f.0.as_str() == "#767676"),
            "the neutral #767676 (Y≈0.1813) must move dark→light"
        );
        assert!(
            flips.iter().any(|f| f.0.as_str() == "#0078D4"),
            "Fluent #0078D4 (Y≈0.1818) must move dark→light"
        );
        // Print the before/after table (run with --nocapture to lift into the PR).
        let vc = ViewingConditions::srgb();
        println!(
            "\n=== CH-4a EMISSION DIFF: {} backgrounds moved dark->light ===",
            flips.len()
        );
        for (hex, old, new, y) in &flips {
            let bg = BgInput::solid(hex).unwrap();
            let before = resolve_forced(&bg, Role::LabelPrimary, &vc, *old)
                .solved()
                .map(|s| s.hex().to_string())
                .unwrap_or_else(|| "-".to_string());
            let after = resolve_forced(&bg, Role::LabelPrimary, &vc, *new)
                .solved()
                .map(|s| s.hex().to_string())
                .unwrap_or_else(|| "-".to_string());
            assert_ne!(
                before, after,
                "{hex}: emission must actually move at resolve level, both {before}"
            );
            println!("{hex}  Y={y:.6}  {old:?} -> {new:?}  primary {before} -> {after}");
        }
    }

    #[test]
    fn every_polarity_move_is_the_derived_white_tie_break_nothing_else_moves() {
        // Byte-identity vs main at the decision level: the tie-break is the ONLY
        // changed code, so wherever the derived choose_polarity equals the frozen
        // margin oracle the whole resolved set is byte-identical to main. Assert
        // that every DIVERGENCE across 256 neutrals + the chromatic cube is the
        // one sanctioned move (dark→light on the band); nothing else moves.
        for hex in emission_diff_grid() {
            let bg = BgInput::solid(&hex).unwrap();
            let new = choose_polarity(&bg);
            let old = choose_polarity_margin_oracle(&bg);
            if new == old {
                continue; // unchanged — identical to main
            }
            assert_eq!(
                old,
                Polarity::DarkOnLight,
                "{hex}: unexpected move FROM {old:?}"
            );
            assert_eq!(
                new,
                Polarity::LightOnDark,
                "{hex}: unexpected move TO {new:?}"
            );
            let y = bg_luminance(&hex);
            assert!(
                y > 0.1791 && y <= 0.1834,
                "{hex}: move at Y {y:.6} outside band"
            );
        }
    }

    #[test]
    fn neutrals_outside_the_band_resolve_byte_identical_to_the_margin_rule() {
        // Concrete to_bits identity THROUGH `resolve`, over all 256 8-bit neutrals
        // and both VCs: wherever the derived polarity equals the margin rule, every
        // role's emitted colour is bit-for-bit what main produced (signed lc bits +
        // hex). Inside the moved band the two intentionally differ (the emission
        // diff), so those neutrals are skipped here and covered by the diff test.
        for c in 0u32..=255 {
            let hex = format!("#{c:02X}{c:02X}{c:02X}");
            let bg = BgInput::solid(&hex).unwrap();
            let live = choose_polarity(&bg);
            let oracle = choose_polarity_margin_oracle(&bg);
            if live != oracle {
                continue; // the intended diff
            }
            for (vc, name) in vcs() {
                let a = resolve_all_with_polarity(&bg, &vc, live); // == resolve()
                let b = resolve_all_with_polarity(&bg, &vc, oracle); // == main
                for ((role, ra), (_, rb)) in a.iter().zip(b.iter()) {
                    match (ra.solved(), rb.solved()) {
                        (Some(sa), Some(sb)) => {
                            assert_eq!(
                                sa.lc().to_bits(),
                                sb.lc().to_bits(),
                                "{name} {hex} {}: lc bits moved outside the band",
                                role.key()
                            );
                            assert_eq!(
                                sa.hex(),
                                sb.hex(),
                                "{name} {hex} {}: hex moved outside the band",
                                role.key()
                            );
                        }
                        (None, None) => {}
                        _ => panic!("{name} {hex} {}: resolution shape differs", role.key()),
                    }
                }
            }
        }
    }

    fn band_hexes() -> Vec<String> {
        let mut v: Vec<String> = (0x70u32..=0x9F)
            .map(|g| format!("#{g:02X}{g:02X}{g:02X}"))
            .collect();
        v.push("#93939C".to_string());
        v.push("#3478F6".to_string());
        v
    }

    fn solved_lc(bg: &BgInput, role: Role, vc: &ViewingConditions) -> f64 {
        let table = RoleTable::default();
        match resolve(bg, role, &table, vc) {
            Ok(Resolved::Color { solved, .. }) => solved.lc(),
            other => panic!("{} expected a colour, got {other:?}", role.key()),
        }
    }

    fn table_default() -> RoleTable {
        RoleTable::default()
    }

    /// The signed `lc` of `role` in a set, if it resolved to a colour.
    fn set_lc_opt(set: &[(Role, Resolved)], role: Role) -> Option<f64> {
        set.iter()
            .find(|(r, _)| *r == role)
            .and_then(|(_, res)| res.solved())
            .map(|s| s.lc())
    }

    /// The emitted hex and the compression flag of `role` in a set, if it
    /// resolved to a colour.
    fn set_hex_and_flag(set: &[(Role, Resolved)], role: Role) -> Option<(String, bool)> {
        set.iter()
            .find(|(r, _)| *r == role)
            .and_then(|(_, res)| match res {
                Resolved::Color {
                    solved, compressed, ..
                } => Some((solved.hex().to_string(), *compressed)),
                _ => None,
            })
    }

    #[test]
    fn strict_text_hierarchy_holds_on_every_reachable_background() {
        // primary > secondary > muted > disabled in |Lc|, on every background,
        // both VCs — the anchor principle makes this hold by construction.
        for (vc, vc_name) in vcs() {
            for bg_hex in REACHABLE_BGS {
                let bg = BgInput::solid(bg_hex).unwrap();
                let mags: Vec<f64> = TEXT_ORDER
                    .iter()
                    .map(|&r| solved_lc(&bg, r, &vc).abs())
                    .collect();
                for pair in mags.windows(2) {
                    assert!(
                        pair[0] > pair[1],
                        "{vc_name} {bg_hex}: hierarchy broken, |Lc| {:?}",
                        mags
                    );
                }
            }
        }
    }

    #[test]
    fn primary_is_near_extreme_on_white_and_black() {
        // The sanity precedent: primary on white/black must read black/white,
        // not grey — |Lc| >= 95 on both extremes, both VCs.
        for (vc, vc_name) in vcs() {
            for bg_hex in ["#FFFFFF", "#101012"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let lc = solved_lc(&bg, Role::LabelPrimary, &vc).abs();
                assert!(
                    lc >= 95.0,
                    "{vc_name} {bg_hex}: primary |Lc| {lc} < 95 — reads grey, not black/white"
                );
            }
        }
    }

    #[test]
    fn polarity_is_uniform_across_a_background_and_read_from_it() {
        // Every text role on a light background is dark-on-light (lc > 0); on a
        // dark background light-on-dark (lc < 0). The whole set shares one
        // polarity, chosen from the background, not the role.
        for (vc, _) in vcs() {
            for (bg_hex, expect_positive) in [("#FFFFFF", true), ("#101012", false)] {
                let bg = BgInput::solid(bg_hex).unwrap();
                for &role in &TEXT_ORDER {
                    let lc = solved_lc(&bg, role, &vc);
                    assert_eq!(
                        lc > 0.0,
                        expect_positive,
                        "{bg_hex} {}: polarity not read from background, lc {lc}",
                        role.key()
                    );
                }
            }
        }
    }

    #[test]
    fn primary_matches_figma_light_anchor_within_tolerance() {
        // Snapshot: primary on white should land near the transferred anchor
        // 103.22 Ys-Lc (the 0.97335917 fraction of 106.0407; genesis Figma
        // anchor 102.6 in the legacy Y_hk metric). A few Lc of tolerance
        // absorbs quantisation and the max-probe.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let lc = solved_lc(&bg, Role::LabelPrimary, &vc);
        assert!(
            (lc - 103.22).abs() <= 2.5,
            "primary on white {lc} should match transferred Figma anchor 103.22 within 2.5"
        );
    }

    #[test]
    fn light_ladder_matches_figma_anchors() {
        // Snapshot: the light text ladder lands near Daniel's Figma "Labels"
        // anchors, transferred into Ys (anchor = Ys of the accepted ladder hex;
        // tertiary = byte-inversion of the genesis 48.9, see the fraction
        // consts). Primary/secondary/disabled match closely (targets sit
        // exactly on the accepted hexes); muted sits a few Lc *above* its
        // anchor because the WCAG 3:1 floor legitimately lifts it on white
        // (see `dark_ladder_is_symmetric_…`) — an explained shift, not silent
        // drift.
        let vc = ViewingConditions::srgb();
        let white = BgInput::solid("#FFFFFF").unwrap();
        let anchors = [
            (Role::LabelPrimary, 103.22, 2.5),
            (Role::LabelSecondary, 68.25, 1.0),
            (Role::LabelTertiary, 50.45, 4.5), // floored up to ~54.3 to clear 3:1
            (Role::LabelQuaternary, 31.11, 1.0),
        ];
        for (role, anchor, tol) in anchors {
            let lc = solved_lc(&white, role, &vc);
            assert!(
                (lc - anchor).abs() <= tol,
                "{}: light {lc} vs Figma anchor {anchor} (tol {tol})",
                role.key()
            );
        }
    }

    #[test]
    fn dark_ladder_is_symmetric_not_figma_asymmetric() {
        // The crux fix: contracts make the dark ladder the *mirror* of the light
        // one, NOT the literal Figma dark anchors (−105.4/−40.9/−26.2/−13.1),
        // which were ~40 % weaker than light because equal opacity steps were
        // never compensated. W5 keeps the authored targets symmetric, while an
        // explicit final-emission criterion may move one polarity to the nearest
        // admissible byte. That movement is generic report provenance, not a
        // criterion branch inside the numerical solver.
        let vc = ViewingConditions::srgb();
        let white = BgInput::solid("#FFFFFF").unwrap();
        let black = BgInput::solid("#101012").unwrap();
        let table = RoleTable::default();
        // Figma's asymmetric dark anchors — what we deliberately do NOT reproduce.
        let figma_dark_asymmetric: [f64; 4] = [-105.4, -40.9, -26.2, -13.1];

        for (i, role) in TEXT_ORDER.iter().enumerate() {
            let light = match resolve(&white, *role, &table, &vc) {
                Ok(Resolved::Color { solved, .. }) => solved,
                other => panic!("{}: {other:?}", role.key()),
            };
            let dark = match resolve(&black, *role, &table, &vc) {
                Ok(Resolved::Color { solved, .. }) => solved,
                other => panic!("{}: {other:?}", role.key()),
            };
            let (light_lc, dark_lc) = (light.lc().abs(), dark.lc().abs());
            // Оба таргета — одна и та же доля от максимума своей полярности;
            // максимумы белого и почти-чёрного близки, потому допуск узкий.
            // Если exact final criterion binds only one side, its explicit
            // adjustment report must account for the measured divergence.
            let criterion_explains =
                light.final_emission_adjusted() || dark.final_emission_adjusted();
            assert!(
                (light_lc - dark_lc).abs() <= 1.5 || criterion_explains,
                "{}: light |Lc| {light_lc} vs dark {dark_lc} diverge without a final-criterion adjustment",
                role.key()
            );
            if i >= 1 {
                // Secondary and weaker: the symmetric dark result is meaningfully
                // stronger than Figma's weak asymmetric dark anchor.
                assert!(
                    dark_lc > figma_dark_asymmetric[i].abs() + 5.0,
                    "{}: symmetric dark {dark_lc} should beat Figma's weak {}",
                    role.key(),
                    figma_dark_asymmetric[i].abs()
                );
            }
        }
    }

    #[test]
    fn none_role_resolves_to_an_honest_zero() {
        // The zero token is a value, not a missing key: it resolves explicitly
        // and reports zero contrast.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let table = RoleTable::default();
        let resolved =
            resolve(&bg, Role::None, &table, &vc).expect("zero role resolves atomically");
        assert_eq!(resolved, Resolved::None);
        assert_eq!(resolved.lc(), Some(0.0));
        assert!(resolved.solved().is_none());
    }

    #[test]
    fn text_roles_meet_their_wcag_criterion_on_final_bytes() {
        // W5: нормативный вердикт — только канонический WCAG22 evaluator на
        // финальных sRGB8-байтах. Anchored-цели дефолтной таблицы достаточно
        // сильны, чтобы держать критерий без solver-пола; регресс критерия на
        // финальной эмиссии падает именно здесь.
        use crate::wcag22::{Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, evaluate_wcag22_hex};
        for (vc, vc_name) in vcs() {
            for bg_hex in ["#FFFFFF", "#101012"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let table = RoleTable::default();
                for (role, criterion) in [
                    (Role::LabelPrimary, Wcag22CriterionV1::Sc143TextDefault),
                    (Role::LabelSecondary, Wcag22CriterionV1::Sc143TextDefault),
                    (
                        Role::LabelTertiary,
                        Wcag22CriterionV1::Sc1411UiComponentOrState,
                    ),
                ] {
                    let solved = match resolve(&bg, role, &table, &vc) {
                        Ok(Resolved::Color { solved, .. }) => solved,
                        other => panic!("{} {bg_hex}: {other:?}", role.key()),
                    };
                    let assessment = evaluate_wcag22_hex(solved.hex(), bg_hex, criterion)
                        .expect("emitted hex is admitted sRGB8");
                    let Wcag22AssessmentV1::Evaluated { decision, .. } = assessment else {
                        panic!("explicit criterion must evaluate");
                    };
                    assert_eq!(
                        decision,
                        Wcag22ApplicableDecisionV1::Pass,
                        "{vc_name} {bg_hex} {}: final bytes {} fail {criterion:?}",
                        role.key(),
                        solved.hex()
                    );
                }
            }
        }
    }

    #[test]
    fn decorative_roles_carry_no_wcag_override() {
        // No decorative role (dJ' or legacy Lc) trips the WCAG legal floor — they
        // are distinguishability contracts, not readability ones.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let table = RoleTable::default();
        for role in [
            Role::Separator,
            Role::BorderBase,
            Role::BorderSoft,
            Role::FillPrimary,
            Role::FillSecondary,
            Role::FillTertiary,
            Role::FillQuaternary,
            Role::ShadowMinor,
            Role::ShadowAmbient,
            Role::ShadowPenumbra,
            Role::ShadowMajor,
        ] {
            // W5: солвер не несёт юр. пола вовсе; сам факт типизированного
            // Color-исхода без criterion-ветви — инвариант декоративности.
            match resolve(&bg, role, &table, &vc) {
                Ok(Resolved::Color { .. }) => {}
                other => panic!("{} expected colour, got {other:?}", role.key()),
            }
        }

        // The legacy Lc decorative roles (shadow stack + separator) still sit above
        // the solver's reliable Lc floor; the dJ' roles deliberately do NOT (their
        // J' separation can be smaller than the Lc clip threshold — that is the
        // whole point of the different unit).
        for role in [
            Role::Separator,
            Role::ShadowMinor,
            Role::ShadowAmbient,
            Role::ShadowPenumbra,
            Role::ShadowMajor,
        ] {
            let solved = match resolve(&bg, role, &table, &vc) {
                Ok(Resolved::Color { solved, .. }) => solved,
                other => panic!("{other:?}"),
            };
            assert!(
                solved.lc().abs() >= DECORATIVE_FLOOR_MIN - 1.0,
                "{}: Lc decorative |Lc| {} below reliable floor",
                role.key(),
                solved.lc().abs()
            );
        }
    }

    #[test]
    fn decorative_magnitudes_drive_result() {
        // The decorative result is driven by the table's Lc magnitude,
        // not a hardcoded final value: change the magnitude, the result follows.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let default_table = RoleTable::default();
        let stronger = default_table
            .clone()
            .with(Role::Separator, RoleSpec::Decorative { magnitude: 20.0 });

        let base = resolve(&bg, Role::Separator, &default_table, &vc)
            .expect("valid default decorative role resolves atomically");
        let bumped = resolve(&bg, Role::Separator, &stronger, &vc)
            .expect("valid stronger decorative role resolves atomically");
        let (b, s) = (base.lc().unwrap().abs(), bumped.lc().unwrap().abs());
        assert!(s > b, "bumped magnitude must raise |Lc|: {b} -> {s}");
    }

    /// Achieved `|dJ'|` of a single resolved role against `bg_hex` under `vc`.
    fn resolved_dj(bg_hex: &str, role: Role, table: &RoleTable, vc: &ViewingConditions) -> f64 {
        let bg = BgInput::solid(bg_hex).unwrap();
        let solved = match resolve(&bg, role, table, vc) {
            Ok(Resolved::Color { solved, .. }) => solved,
            other => panic!("{} expected a colour, got {other:?}", role.key()),
        };
        let jp_fg = crate::lcs::LcsColor::from_hex_with_vc(solved.hex(), vc)
            .unwrap()
            .jp();
        let jp_bg = crate::lcs::LcsColor::from_hex_with_vc(bg_hex, vc)
            .unwrap()
            .jp();
        (jp_fg - jp_bg).abs()
    }

    #[test]
    fn dj_roles_land_on_the_owner_anchor_within_a_grid_step() {
        // The dJ' contract is honest: the emitted colour's measured |dJ'| against
        // the surface lands within one 8-bit grid step (~0.6 J') of the owner's
        // literal anchor — the right unit reproduced, not a substitute. Checked on
        // light and dark backgrounds, under the matching theme VC.
        let table = RoleTable::default();
        let light = ViewingConditions::srgb();
        let dark = ViewingConditions::dim_surround();
        // (role, light anchor, dark anchor)
        let cases = [
            (Role::FillPrimary, 7.93, 17.67),
            (Role::FillSecondary, 6.41, 15.78),
            (Role::FillTertiary, 4.63, 12.01),
            (Role::FillQuaternary, 3.15, 8.22),
            (Role::BorderBase, 6.41, 10.12),
            (Role::BorderSoft, 3.15, 5.83),
        ];
        for bg_hex in ["#FFFFFF", "#7F7F7F", "#101012"] {
            for (role, light_anchor, dark_anchor) in cases {
                let got_light = resolved_dj(bg_hex, role, &table, &light);
                assert!(
                    (got_light - light_anchor).abs() <= 0.7,
                    "{bg_hex} {} light: |dJ'| {got_light:.3} off anchor {light_anchor}",
                    role.key()
                );
                let got_dark = resolved_dj(bg_hex, role, &table, &dark);
                assert!(
                    (got_dark - dark_anchor).abs() <= 0.7,
                    "{bg_hex} {} dark: |dJ'| {got_dark:.3} off anchor {dark_anchor}",
                    role.key()
                );
            }
        }
    }

    #[test]
    fn dj_magnitude_selects_per_theme() {
        // The same role selects its independently authored dark offset under the
        // dark VC, proving this is a per-VC pair rather than one shared constant.
        let table = RoleTable::default();
        let light = ViewingConditions::srgb();
        let dark = ViewingConditions::dim_surround();
        for bg_hex in ["#FFFFFF", "#7F7F7F", "#101012"] {
            for role in [Role::FillPrimary, Role::BorderBase] {
                let l = resolved_dj(bg_hex, role, &table, &light);
                let d = resolved_dj(bg_hex, role, &table, &dark);
                assert!(
                    d > l + 1.0,
                    "{bg_hex} {}: dark dJ' {d:.3} must exceed light {l:.3}",
                    role.key()
                );
            }
        }
    }

    #[test]
    fn dj_magnitude_drives_the_result() {
        // The dJ' result follows the table's magnitude: a larger anchor yields a
        // larger achieved separation. Proves the value is wired through, not hard-coded.
        let vc = ViewingConditions::srgb();
        let table = RoleTable::default();
        let stronger = table.clone().with(
            Role::FillTertiary,
            RoleSpec::DecorativeDj {
                magnitude_dj: DjMagnitude::new(20.0, 20.0),
            },
        );
        let base = resolved_dj("#FFFFFF", Role::FillTertiary, &table, &vc);
        let bumped = resolved_dj("#FFFFFF", Role::FillTertiary, &stronger, &vc);
        assert!(
            bumped > base + 5.0,
            "bumped dJ' must rise: {base:.3} -> {bumped:.3}"
        );
    }

    #[test]
    fn dj_off_axis_target_reports_bounded_degradation() {
        // A dJ' larger than the axis can supply (300 J' on near-black — the
        // foreground would need J' ≈ −290) не попадает в ограниченный обход:
        // кандидат с минимальной ошибкой среди просмотренных (стена оси — почти
        // белый) несёт compressed. Тест проверяет типизированный статус и
        // фактическую сторону стены, но не заявляет оптимум по всему гамуту.
        let vc = ViewingConditions::srgb();
        let table = RoleTable::default().with(
            Role::FillPrimary,
            RoleSpec::DecorativeDj {
                magnitude_dj: DjMagnitude::new(300.0, 300.0),
            },
        );
        let bg = BgInput::solid("#101012").unwrap();
        match resolve(&bg, Role::FillPrimary, &table, &vc) {
            Ok(Resolved::Color {
                solved, compressed, ..
            }) => {
                assert!(compressed, "off-axis dJ' must carry the degradation flag");
                // Стена светлой стороны: решённый цвет заметно светлее фона
                // (light-on-dark полярность на #101012) — не тихий возврат фона.
                let rgb = crate::spaces::srgb::srgb_from_hex(solved.hex()).unwrap();
                let l = crate::spaces::oklab::srgb_linear_to_oklab(rgb)[0];
                assert!(
                    l > 0.9,
                    "degraded colour must sit at the light wall, got L={l:.3} ({})",
                    solved.hex()
                );
            }
            other => panic!("expected degraded Color for an off-axis dJ', got {other:?}"),
        }
    }

    #[test]
    fn dj_in_budget_target_is_not_flagged() {
        // Парный контроль: достижимая ступень (дефолтная таблица на белом)
        // решается точно и БЕЗ флага — деградация не размазывается на всех.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        match resolve(&bg, Role::FillPrimary, &RoleTable::default(), &vc) {
            Ok(Resolved::Color { compressed, .. }) => {
                assert!(!compressed, "in-budget dJ' must not be flagged")
            }
            other => panic!("expected Color, got {other:?}"),
        }
    }

    #[test]
    fn overriding_one_role_leaves_the_others_untouched() {
        // Custom target for one role changes only its output; the rest stay at
        // their defaults, and default() restores everything.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let default_table = RoleTable::default();
        let custom = default_table.clone().with(
            Role::LabelPrimary,
            RoleSpec::Anchor(TextAnchor::new(0.5, Floor::AaText).unwrap()),
        );

        // Primary changed.
        let p_default = solved_lc(&bg, Role::LabelPrimary, &vc);
        let p_custom = match resolve(&bg, Role::LabelPrimary, &custom, &vc) {
            Ok(Resolved::Color { solved, .. }) => solved.lc(),
            other => panic!("{other:?}"),
        };
        assert!(
            (p_default - p_custom).abs() > 10.0,
            "override should move primary: {p_default} vs {p_custom}"
        );
        // Secondary unchanged.
        let s_default = solved_lc(&bg, Role::LabelSecondary, &vc);
        let s_custom = match resolve(&bg, Role::LabelSecondary, &custom, &vc) {
            Ok(Resolved::Color { solved, .. }) => solved.lc(),
            other => panic!("{other:?}"),
        };
        assert!(
            (s_default - s_custom).abs() < 1e-9,
            "override of primary must not touch secondary"
        );
    }

    #[test]
    fn text_anchor_rejects_instead_of_rewriting_invalid_fraction() {
        for fraction in [f64::NAN, f64::NEG_INFINITY, -0.1, 0.0, 1.1, f64::INFINITY] {
            assert!(
                matches!(
                    TextAnchor::new(fraction, Floor::AaText),
                    Err(SolveFailure::InvalidInput(_))
                ),
                "fraction {fraction:?} must be rejected"
            );
        }
        assert_eq!(
            TextAnchor::new(1.0, Floor::AaText).unwrap().fraction(),
            1.0,
            "the physical endpoint must remain exact"
        );
    }

    #[test]
    fn resolve_set_is_complete_and_ordered() {
        // The full sweep returns every role exactly once, in Role::ALL order,
        // with no key skipped (the zero token included as Resolved::None).
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let table = RoleTable::default();
        let set = resolve_set(&bg, &table, &vc);
        let roles: Vec<Role> = set.iter().map(|(r, _)| *r).collect();
        assert_eq!(
            roles,
            Role::ALL.to_vec(),
            "set must cover Role::ALL in order"
        );
        let none = set.iter().find(|(r, _)| *r == Role::None).unwrap();
        assert_eq!(none.1, Resolved::None, "zero token present as honest zero");
    }

    #[test]
    fn light_grey_band_has_a_readable_text_polarity_not_a_false_unreachable() {
        // BLOCKER 1 regression: the light-grey band (#777777..#999999, incl.
        // #93939C and #3478F6) must NOT report text roles unreachable. Black text
        // on these backgrounds clears AA with room (#999999: 7.37:1; #808080:
        // 5.32:1; #3478F6: 5.16:1) — the old "larger LPC maximum" polarity rule
        // chose the white side, which cannot reach 4.5:1, and floored every text
        // role. With the WCAG-first polarity the readable side is chosen, so
        // primary/secondary/muted all resolve on the whole band, both VCs.
        for (vc, vc_name) in vcs() {
            for bg_hex in band_hexes() {
                let bg = BgInput::solid(&bg_hex).unwrap();
                let set = resolve_set(&bg, &table_default(), &vc);
                for role in [
                    Role::LabelPrimary,
                    Role::LabelSecondary,
                    Role::LabelTertiary,
                ] {
                    let r = &set.iter().find(|(rr, _)| *rr == role).unwrap().1;
                    assert!(
                        matches!(r, Resolved::Color { .. }),
                        "{vc_name} {bg_hex} {}: must resolve, got {r:?}",
                        role.key()
                    );
                }
            }
        }
    }

    #[test]
    fn no_false_unreachable_when_the_opposite_polarity_is_reachable() {
        // The core invariant of the two-stage polarity: on the whole band, every
        // text/UI role resolves to a colour, because the polarity is chosen to be
        // the reachable side. (On solid sRGB there is no background where both
        // black and white text fall below 4.5:1 — so any typed failure here
        // would be a false negative by construction.)
        for (vc, vc_name) in vcs() {
            for bg_hex in band_hexes() {
                let bg = BgInput::solid(&bg_hex).unwrap();
                let set = resolve_set(&bg, &table_default(), &vc);
                for (role, r) in &set {
                    if matches!(role, Role::LabelPrimary | Role::LabelSecondary) {
                        assert!(
                            matches!(r, Resolved::Color { .. }),
                            "{vc_name} {bg_hex} {}: false unreachable — got {r:?}",
                            role.key()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn polarity_is_vc_independent_across_the_band() {
        // The WCAG-first criterion is the VC-independent relative-luminance
        // formula, so a role's polarity (sign of lc) must be identical under the
        // light and dim viewing conditions for the same background — no per-theme
        // coin-flip on a near-tie like #3478F6.
        let srgb = ViewingConditions::srgb();
        let dim = ViewingConditions::dim_surround();
        for bg_hex in band_hexes() {
            let bg = BgInput::solid(&bg_hex).unwrap();
            let s = resolve_set(&bg, &table_default(), &srgb);
            let d = resolve_set(&bg, &table_default(), &dim);
            for role in TEXT_ORDER {
                let (Some(ls), Some(ld)) = (set_lc_opt(&s, role), set_lc_opt(&d, role)) else {
                    continue;
                };
                assert_eq!(
                    ls > 0.0,
                    ld > 0.0,
                    "{bg_hex} {}: polarity flipped between VCs (srgb {ls}, dim {ld})",
                    role.key()
                );
            }
        }
    }

    #[test]
    fn hierarchy_is_non_strict_and_compression_is_flagged_on_the_band() {
        // BLOCKER 2: where the readable window is narrower than the hierarchy
        // steps (#747474: the only readable polarity barely clears 4.5:1),
        // primary and secondary used to collapse to an identical hex silently.
        // Now: the order stays non-strict (|Lc| primary >= secondary >= muted >=
        // disabled) everywhere on the band, and any role squeezed onto its senior
        // is flagged compressed — never a silent two-roles-one-colour identity.
        for (vc, vc_name) in vcs() {
            for bg_hex in band_hexes() {
                let bg = BgInput::solid(&bg_hex).unwrap();
                let set = resolve_set(&bg, &table_default(), &vc);
                let mags: Vec<f64> = TEXT_ORDER
                    .iter()
                    .filter_map(|&r| set_lc_opt(&set, r).map(f64::abs))
                    .collect();
                for pair in mags.windows(2) {
                    assert!(
                        pair[0] + 1e-9 >= pair[1],
                        "{vc_name} {bg_hex}: order broken (junior stronger), |Lc| {mags:?}"
                    );
                }
                // No two adjacent *distinct* roles may share an identical hex
                // without the junior being flagged compressed.
                for window in TEXT_ORDER.windows(2) {
                    let [senior, junior] = [window[0], window[1]];
                    let (Some((sh, _)), Some((jh, jc))) = (
                        set_hex_and_flag(&set, senior),
                        set_hex_and_flag(&set, junior),
                    ) else {
                        continue;
                    };
                    if sh == jh {
                        assert!(
                            jc,
                            "{vc_name} {bg_hex}: {} == {} ({sh}) but not flagged compressed",
                            senior.key(),
                            junior.key()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn hierarchy_holds_strictly_on_white_with_no_compression_flag() {
        // On a background with full headroom the hierarchy is strict and nothing
        // is compressed — the flag is reserved for genuine squeezes.
        for (vc, _) in vcs() {
            let bg = BgInput::solid("#FFFFFF").unwrap();
            let set = resolve_set(&bg, &table_default(), &vc);
            for role in TEXT_ORDER {
                let r = &set.iter().find(|(rr, _)| *rr == role).unwrap().1;
                assert!(
                    !r.compressed(),
                    "{}: must not be compressed on white",
                    role.key()
                );
            }
        }
    }

    #[test]
    fn no_silent_clip_anywhere_on_the_band() {
        // Every resolved colour carries a real separation from its background; the
        // only legitimate zeros are the explicit zero-token roles (Role::None and
        // the family zeros border-none / fill-none, all spec'd RoleSpec::Zero); an
        // unreachable role surfaces a reason. Nothing clips silently.
        //
        // The honest "carries separation" metric depends on the contract's PHYSICS:
        // an Lc role (labels, separator, shadows) must have |Lc| ≥ 1; a dJ'
        // role (fills, base/soft borders) must have a real perceived-lightness
        // difference |dJ'| ≥ 1 — its |Lc| can sit at zero inside the low-contrast
        // clip while its J' separation is genuine, which is exactly why it uses a
        // different unit. Checking |Lc| on a dJ' role would be the wrong ruler.
        let table = table_default();
        for (vc, _) in vcs() {
            for bg_hex in band_hexes() {
                let bg = BgInput::solid(&bg_hex).unwrap();
                let jp_bg = crate::lcs::LcsColor::from_hex_with_vc(&bg_hex, &vc)
                    .unwrap()
                    .jp();
                let set = resolve_set(&bg, &table, &vc);
                let no_silent_clip = set.iter().all(|(role, r)| match r {
                    // Свечение не участвует в dJ'-клип-инварианте (не контраст-роль).
                    Resolved::Glow(_) | Resolved::GlowIndeterminate(_) => true,
                    Resolved::Color { solved, .. } => {
                        if matches!(table.spec(*role), RoleSpec::DecorativeDj { .. }) {
                            let jp_fg = crate::lcs::LcsColor::from_hex_with_vc(solved.hex(), &vc)
                                .unwrap()
                                .jp();
                            (jp_fg - jp_bg).abs() >= 1.0
                        } else {
                            solved.lc().abs() >= 1.0
                        }
                    }
                    Resolved::None => matches!(table.spec(*role), RoleSpec::Zero),
                    // Дефолтная таблица не несёт Ladder/AlphaAnalog — появление
                    // Translucent здесь означало бы дрейф default() и обязано падать
                    // шумно, а не маскироваться под «не клип».
                    Resolved::Translucent(_) => panic!(
                        "{bg_hex}: RoleTable::default() отдал Translucent для {:?} — дрейф дефолт-таблицы",
                        role
                    ),
                    // Дефолтная таблица не несёт Material — появление означало бы дрейф.
                    Resolved::Material(_) => panic!(
                        "{bg_hex}: RoleTable::default() отдал Material для {:?} — дрейф дефолт-таблицы",
                        role
                    ),
                    Resolved::Failure(failure) => {
                        crate::test_support::role_failure_repr(failure);
                        true
                    }
                });
                assert!(
                    no_silent_clip,
                    "{bg_hex}: a role resolved to a zero-separation clip"
                );
            }
        }
    }

    #[test]
    fn role_keys_are_stable_and_unique() {
        // The string keys are the downstream contract; they must be unique.
        let mut seen = std::collections::HashSet::new();
        for role in Role::ALL {
            assert!(seen.insert(role.key()), "duplicate key {}", role.key());
        }
    }

    #[test]
    fn role_keys_follow_the_hig_kebab_taxonomy() {
        // The CSS keys are the stable contract with labui (`--lab-label-primary`,
        // …). Pin the exact HIG-pattern kebab-case spelling so a rename never slips
        // through silently. labui already consumes these names.
        use std::collections::HashSet;
        let keys: HashSet<&str> = Role::ALL.iter().map(|r| r.key()).collect();
        for expected in [
            "label-primary",
            "label-secondary",
            "label-tertiary",
            "label-quaternary",
            "separator",
            "border-strong",
            "border-base",
            "border-soft",
            "border-none",
            "fill-primary",
            "fill-secondary",
            "fill-tertiary",
            "fill-quaternary",
            "fill-none",
            "shadow-minor",
            "shadow-ambient",
            "shadow-penumbra",
            "shadow-major",
            "none",
        ] {
            assert!(keys.contains(expected), "missing HIG role key {expected}");
        }
        assert_eq!(
            keys.len(),
            19,
            "the HIG taxonomy is exactly 19 roles, found {}",
            keys.len()
        );
        // No legacy text-* key may survive the rename.
        for legacy in [
            "text-primary",
            "text-secondary",
            "text-muted",
            "text-disabled",
            "surface",
        ] {
            assert!(!keys.contains(legacy), "legacy key {legacy} still present");
        }
    }

    #[test]
    fn border_strong_mirrors_label_primary_exactly() {
        // border-strong is HIG Border/Strong = N12 = Labels/Primary strength: same
        // FRACTION as label-primary, but the floor is non-text 3:1 (a border must
        // be distinguishable, not readable). On these backgrounds neither floor
        // binds, so the emitted colour must still equal label-primary's on every
        // background, both VCs — a crisp N12-weight edge.
        for (vc, vc_name) in vcs() {
            for bg_hex in ["#FFFFFF", "#101012", "#7F7F7F", "#3478F6"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let prim = solved_lc(&bg, Role::LabelPrimary, &vc);
                let strong = solved_lc(&bg, Role::BorderStrong, &vc);
                assert!(
                    (prim - strong).abs() < 1e-9,
                    "{vc_name} {bg_hex}: border-strong {strong} != label-primary {prim}"
                );
            }
        }
    }

    #[test]
    fn explicit_zero_roles_resolve_to_honest_zero() {
        // Both family zeros — border-none and fill-none — are values, not missing
        // keys: they resolve to Resolved::None with zero contrast, like Role::None.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let table = RoleTable::default();
        for role in [Role::BorderNone, Role::FillNone, Role::None] {
            let resolved =
                resolve(&bg, role, &table, &vc).expect("valid zero role resolves atomically");
            assert_eq!(
                resolved,
                Resolved::None,
                "{} must be an honest zero",
                role.key()
            );
            assert_eq!(resolved.lc(), Some(0.0), "{}: zero contrast", role.key());
        }
    }

    /// The signed |Lc| of a role in a set, panicking if it did not resolve to a
    /// colour — used by the ladder-ordering invariants.
    fn ladder_mag(set: &[(Role, Resolved)], role: Role) -> f64 {
        set.iter()
            .find(|(r, _)| *r == role)
            .and_then(|(_, res)| res.solved())
            .map(|s| s.lc().abs())
            .unwrap_or_else(|| panic!("{} did not resolve to a colour", role.key()))
    }

    /// The achieved perceived-lightness difference `|dJ'|` of a role's resolved
    /// hex against `bg_hex`, measured on the emitted colour under `vc` — the
    /// honest metric for the dJ' ladders (the fill/border steps the low-contrast
    /// LPC clip can report as zero `Lc` while their J' separation is real).
    fn ladder_dj(
        set: &[(Role, Resolved)],
        role: Role,
        bg_hex: &str,
        vc: &ViewingConditions,
    ) -> f64 {
        let solved = set
            .iter()
            .find(|(r, _)| *r == role)
            .and_then(|(_, res)| res.solved())
            .unwrap_or_else(|| panic!("{} did not resolve to a colour", role.key()));
        let jp_fg = crate::lcs::LcsColor::from_hex_with_vc(solved.hex(), vc)
            .unwrap()
            .jp();
        let jp_bg = crate::lcs::LcsColor::from_hex_with_vc(bg_hex, vc)
            .unwrap()
            .jp();
        (jp_fg - jp_bg).abs()
    }

    #[test]
    fn fill_ladder_is_strictly_descending_in_visibility() {
        // The fill ladder is a strict order contract: primary is the most visible
        // fill, quaternary the faintest. The fills are dJ' roles, so visibility is
        // the achieved perceived-lightness difference |dJ'| — NOT |Lc|, which the
        // low-contrast LPC clip can flatten to zero on the faint steps even though
        // their J' separation is real. Assert the strict order in J' on every
        // background, both VCs, with a real gap (no two steps collapsing).
        let table = RoleTable::default();
        for (vc, vc_name) in vcs() {
            for bg_hex in [
                "#FFFFFF", "#F2F2F7", "#7F7F7F", "#1C1C1E", "#101012", "#3478F6",
            ] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let set = resolve_set(&bg, &table, &vc);
                let ladder = [
                    Role::FillPrimary,
                    Role::FillSecondary,
                    Role::FillTertiary,
                    Role::FillQuaternary,
                ];
                let djs: Vec<f64> = ladder
                    .iter()
                    .map(|&r| ladder_dj(&set, r, bg_hex, &vc))
                    .collect();
                for pair in djs.windows(2) {
                    assert!(
                        pair[0] > pair[1],
                        "{vc_name} {bg_hex}: fill ladder not strictly descending, |dJ'| {djs:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn shadow_stack_is_strictly_ascending_in_visibility() {
        // The shadow stack is progressive: minor (subtlest) < ambient < penumbra <
        // major (strongest). Strict order is the contract carried by the Lc
        // magnitudes.
        let table = RoleTable::default();
        for (vc, vc_name) in vcs() {
            for bg_hex in [
                "#FFFFFF", "#F2F2F7", "#7F7F7F", "#1C1C1E", "#101012", "#3478F6",
            ] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let set = resolve_set(&bg, &table, &vc);
                let stack = [
                    Role::ShadowMinor,
                    Role::ShadowAmbient,
                    Role::ShadowPenumbra,
                    Role::ShadowMajor,
                ];
                let mags: Vec<f64> = stack.iter().map(|&r| ladder_mag(&set, r)).collect();
                for pair in mags.windows(2) {
                    assert!(
                        pair[0] < pair[1],
                        "{vc_name} {bg_hex}: shadow stack not strictly ascending, |Lc| {mags:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn shadow_stack_is_strictly_ascending_under_increased_contrast() {
        // Класс-замок корнер-кейса IC (2026-07-03): пол повышенной контрастности
        // применялся как `max(|m|, 15.0)` и схлопывал ВЕСЬ стек теней
        // (8/9.5/11.5/14 → 15/15/15/15) плюс сепаратор в один побайтно
        // одинаковый цвет — строгая лестница нарушалась ровно в `-ic` темах,
        // которые не гонял ни один тест (`vcs()` не содержит IC-пресетов).
        // RED до порядкосохраняющего сдвига в `decorative_contract`, GREEN после:
        // (1) строгий порядок minor < ambient < penumbra < major сохранён;
        // (2) каждая ступень держит IC-пол 15.0 Lc (с допуском квантования);
        // (3) измеренные владельцем зазоры стека не мутируют: целевые величины
        //     сдвинуты равномерно, поэтому попарные разрывы ≥ 1 Lc остаются.
        let table = RoleTable::default();
        for (vc, vc_name) in [
            (ViewingConditions::srgb_high_contrast(), "srgb-ic"),
            (ViewingConditions::dim_surround_high_contrast(), "dim-ic"),
        ] {
            for bg_hex in ["#FFFFFF", "#F2F2F7", "#1C1C1E", "#101012"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let set = resolve_set(&bg, &table, &vc);
                let stack = [
                    Role::ShadowMinor,
                    Role::ShadowAmbient,
                    Role::ShadowPenumbra,
                    Role::ShadowMajor,
                ];
                let mags: Vec<f64> = stack.iter().map(|&r| ladder_mag(&set, r)).collect();
                for pair in mags.windows(2) {
                    assert!(
                        pair[0] < pair[1],
                        "{vc_name} {bg_hex}: IC shadow stack not strictly ascending, |Lc| {mags:?}"
                    );
                    assert!(
                        pair[1] - pair[0] >= 1.0,
                        "{vc_name} {bg_hex}: IC shadow gap collapsed below 1 Lc: {mags:?}"
                    );
                }
                for (role, mag) in stack.iter().zip(&mags) {
                    assert!(
                        *mag >= IC_DECORATIVE_FLOOR_MIN - 1.0,
                        "{vc_name} {bg_hex}: {} |Lc| {mag:.2} below the IC floor \
                         {IC_DECORATIVE_FLOOR_MIN} (quantisation tolerance 1.0)",
                        role.key()
                    );
                }
            }
        }
    }

    #[test]
    fn composite_distinct_flags_degenerate_tint_over_own_background() {
        // Класс «тинт ≈ фон ⇒ пиксельный no-op»: до флага
        // вырожденная тень/свечение (тёмный тинт на тёмном фоне, светлый на
        // светлом) проходила как валидный Translucent молча — composite_lc≈0
        // вычислялся, но нигде не гейтился. Теперь вырождение ИЗМЕРИМО.
        use crate::spaces::srgb::srgb_encoded_from_hex;
        let vc = ViewingConditions::srgb();

        // Вырождение: тинт побайтно равен фону — композит не может отличаться.
        for (hex, alpha) in [("#101012", 0.22), ("#FFFFFF", 0.08), ("#787880", 0.5)] {
            let tint = srgb_encoded_from_hex(hex).unwrap();
            let bg = BgInput::solid(hex).unwrap();
            let res = resolve_rgba_direct(tint, alpha, &bg, &vc)
                .expect("valid direct rgba fixture resolves");
            let t = res
                .translucent()
                .unwrap_or_else(|| panic!("{hex} rgba должен резолвиться"));
            assert!(
                !t.composite_distinct(),
                "{hex} @ {alpha}: тинт == фон, композит обязан быть неотличим \
                 (composite={}, флаг должен быть false)",
                t.composite_hex()
            );
        }

        // Контроль: контрастный тинт на том же фоне отличим уже при малой α.
        let dark = srgb_encoded_from_hex("#101012").unwrap();
        let bg_white = BgInput::solid("#FFFFFF").unwrap();
        let res = resolve_rgba_direct(dark, 0.12, &bg_white, &vc)
            .expect("valid direct rgba fixture resolves");
        let t = res.translucent().expect("контрастный rgba резолвится");
        assert!(
            t.composite_distinct(),
            "тёмный тинт @ 0.12 на белом обязан быть отличим (composite={})",
            t.composite_hex()
        );
    }

    #[test]
    fn rgba_failure_provenance_does_not_blame_clients_for_core_output() {
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();

        let internal = resolve_rgba_direct([f64::NAN, 0.0, 0.0], 0.5, &bg, &vc);
        assert!(
            matches!(internal, Err(SolveFailure::InternalInvariant(_))),
            "generated tint drift must fail the enclosing boundary: {internal:?}"
        );

        let rejected = resolve_rgba_direct([0.0, 0.0, 0.0], f64::NAN, &bg, &vc);
        assert!(
            matches!(rejected, Err(SolveFailure::InvalidInput(_))),
            "public alpha must be rejected, got {rejected:?}"
        );
    }

    #[test]
    fn alpha_coerced_flags_only_when_requested_alpha_raised_to_floor() {
        // H1 (аудит 2026-07-03): поднятие α до byte-grid пола на пути альфа-аналога —
        // деградация КОНТРАКТА роли (эмитируется не запрошенная α). До флага она
        // проходила молча: композит побайтно равен солиду, но обещанная
        // прозрачность подменялась без объявления. Флаг делает подмену видимой.
        use crate::spaces::srgb::srgb_encoded_from_hex;
        let vc = ViewingConditions::srgb();
        let white = BgInput::solid("#FFFFFF").unwrap();

        // Коэрсия: почти-чёрный солид над белым требует α_min ≈ 0.94 (см.
        // alpha.rs::min_alpha_hex("#101012","#FFFFFF") > 0.9); запрос 0.05
        // неразрешим → α поднимается → флаг true.
        let dark_solid = srgb_encoded_from_hex("#101012").unwrap();
        let coerced = resolve_rgba_inverted(dark_solid, 0.05, &white, &vc)
            .expect("valid inverted rgba fixture resolves");
        let t = coerced
            .translucent()
            .expect("альфа-аналог резолвится (α поднимается до разрешимой)");
        assert!(
            t.alpha_coerced(),
            "запрос α=0.05 неразрешим для #101012 над белым — флаг обязан быть true \
             (фактическая α={}, тинт={})",
            t.alpha(),
            t.tint_hex()
        );
        assert!(
            t.alpha() > 0.05,
            "коэрсия обязана поднять α выше запрошенной 0.05, получено {}",
            t.alpha()
        );

        // Без коэрсии (α разрешима как есть): светлый солид над белым (низкий пол)
        // при α=0.5 → флаг false.
        let light_solid = srgb_encoded_from_hex("#E4E4E6").unwrap();
        let ok = resolve_rgba_inverted(light_solid, 0.5, &white, &vc)
            .expect("valid inverted rgba fixture resolves");
        let t_ok = ok
            .translucent()
            .expect("разрешимый альфа-аналог резолвится");
        assert!(
            !t_ok.alpha_coerced(),
            "α=0.5 разрешима для #E4E4E6 над белым — флаг обязан быть false (α={})",
            t_ok.alpha()
        );
        assert!(
            (t_ok.alpha() - 0.5).abs() < 1e-12,
            "разрешимая α эмитится как запрошенная, получено {}",
            t_ok.alpha()
        );

        // Разрешимость байтовой сетки сильнее непрерывной инверсии: белый красный
        // тинт @ 0.12 уже округляется в #1F0000, поэтому коэрсии быть не должно.
        let black = BgInput::solid("#000000").unwrap();
        let quantised_solid = srgb_encoded_from_hex("#1F0000").unwrap();
        let quantised = resolve_rgba_inverted(quantised_solid, 0.12, &black, &vc)
            .expect("valid quantised rgba fixture resolves");
        let t_quantised = quantised.translucent().unwrap();
        assert!(!t_quantised.alpha_coerced());
        assert_eq!(t_quantised.alpha().to_bits(), 0.12_f64.to_bits());
        assert_eq!(t_quantised.tint_hex(), "#FF0000");

        // Граница: α=1.0 всегда разрешима (тинт=солид) → флаг false даже для
        // насыщенного солида, который иначе коэрсил бы.
        let full = resolve_rgba_inverted(dark_solid, 1.0, &white, &vc)
            .expect("valid opaque rgba fixture resolves");
        let t_full = full.translucent().expect("α=1.0 тривиально разрешима");
        assert!(
            !t_full.alpha_coerced(),
            "α=1.0 разрешима по построению — коэрсии нет"
        );

        // Прямая лестница (не альфа-аналог) НИКОГДА не коэрсит α.
        let direct = resolve_rgba_direct(dark_solid, 0.12, &white, &vc)
            .expect("valid direct rgba fixture resolves");
        assert!(
            !direct.translucent().unwrap().alpha_coerced(),
            "прямая rgba-лестница эмитит α как есть — флаг всегда false"
        );
    }

    #[test]
    fn alpha_analog_semantic_path_evaluates_final_source_over_once() {
        let vc = ViewingConditions::srgb();
        let background = BgInput::solid("#FFFFFF").unwrap();
        let target = crate::spaces::srgb::srgb_encoded_from_hex("#787880").unwrap();

        crate::alpha::reset_source_over_evaluation_count();
        let resolved = resolve_rgba_inverted(target, 0.5, &background, &vc)
            .expect("valid alpha analog must resolve");

        assert_eq!(resolved.translucent().unwrap().composite_hex(), "#787880");
        assert_eq!(
            crate::alpha::source_over_evaluation_count(),
            1,
            "proposal, exact gate and semantic emission must share one final occurrence"
        );
    }

    #[test]
    fn fill_constant_anchors_are_strictly_descending() {
        assert!(
            FILL_PRIMARY_DJ.light() > FILL_SECONDARY_DJ.light(),
            "FILL_PRIMARY_DJ.light {} must exceed FILL_SECONDARY_DJ.light {}",
            FILL_PRIMARY_DJ.light(),
            FILL_SECONDARY_DJ.light()
        );
        assert!(
            FILL_SECONDARY_DJ.light() > FILL_TERTIARY_DJ.light(),
            "FILL_SECONDARY_DJ.light {} must exceed FILL_TERTIARY_DJ.light {}",
            FILL_SECONDARY_DJ.light(),
            FILL_TERTIARY_DJ.light()
        );
        assert!(
            FILL_TERTIARY_DJ.light() > FILL_QUATERNARY_DJ.light(),
            "FILL_TERTIARY_DJ.light {} must exceed FILL_QUATERNARY_DJ.light {}",
            FILL_TERTIARY_DJ.light(),
            FILL_QUATERNARY_DJ.light()
        );
        assert!(
            FILL_PRIMARY_DJ.dark() > FILL_SECONDARY_DJ.dark(),
            "FILL_PRIMARY_DJ.dark {} must exceed FILL_SECONDARY_DJ.dark {}",
            FILL_PRIMARY_DJ.dark(),
            FILL_SECONDARY_DJ.dark()
        );
        assert!(
            FILL_SECONDARY_DJ.dark() > FILL_TERTIARY_DJ.dark(),
            "FILL_SECONDARY_DJ.dark {} must exceed FILL_TERTIARY_DJ.dark {}",
            FILL_SECONDARY_DJ.dark(),
            FILL_TERTIARY_DJ.dark()
        );
        assert!(
            FILL_TERTIARY_DJ.dark() > FILL_QUATERNARY_DJ.dark(),
            "FILL_TERTIARY_DJ.dark {} must exceed FILL_QUATERNARY_DJ.dark {}",
            FILL_TERTIARY_DJ.dark(),
            FILL_QUATERNARY_DJ.dark()
        );
    }

    #[test]
    fn shadow_constant_stack_is_strictly_ascending_with_gaps() {
        const {
            assert!(
                SHADOW_MINOR_LC > DECORATIVE_FLOOR_MIN,
                "shadow-minor must exceed floor"
            );
            assert!(
                SHADOW_MINOR_LC < SHADOW_AMBIENT_LC,
                "shadow-minor must be less than ambient"
            );
            assert!(
                SHADOW_AMBIENT_LC < SHADOW_PENUMBRA_LC,
                "shadow-ambient must be less than penumbra"
            );
            assert!(
                SHADOW_PENUMBRA_LC < SHADOW_MAJOR_LC,
                "shadow-penumbra must be less than major"
            );
            assert!(
                SHADOW_AMBIENT_LC - SHADOW_MINOR_LC >= 1.5,
                "gap ambient-minor must be >= 1.5"
            );
            assert!(
                SHADOW_PENUMBRA_LC - SHADOW_AMBIENT_LC >= 1.5,
                "gap penumbra-ambient must be >= 1.5"
            );
            assert!(
                SHADOW_MAJOR_LC - SHADOW_PENUMBRA_LC >= 1.5,
                "gap major-penumbra must be >= 1.5"
            );
        }
    }

    #[test]
    fn border_base_is_stronger_than_border_soft() {
        // The border dJ' ladder (base > soft); strong is anchored and tested
        // separately. Strict order in J' is the contract (measured as |dJ'|, not
        // |Lc|, for the same reason as the fills).
        let table = RoleTable::default();
        for (vc, vc_name) in vcs() {
            for bg_hex in ["#FFFFFF", "#7F7F7F", "#101012", "#3478F6"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let set = resolve_set(&bg, &table, &vc);
                let base = ladder_dj(&set, Role::BorderBase, bg_hex, &vc);
                let soft = ladder_dj(&set, Role::BorderSoft, bg_hex, &vc);
                assert!(
                    base > soft,
                    "{vc_name} {bg_hex}: border-base {base} must exceed border-soft {soft}"
                );
            }
        }
    }

    #[test]
    fn resolve_set_returns_all_nineteen_roles() {
        // The full sweep returns 19 roles (the HIG taxonomy) including both family
        // zeros and the universal zero, in Role::ALL order.
        let vc = ViewingConditions::srgb();
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let set = resolve_set(&bg, &RoleTable::default(), &vc);
        assert_eq!(set.len(), 19, "resolve_set must return all 19 roles");
        let roles: Vec<Role> = set.iter().map(|(r, _)| *r).collect();
        assert_eq!(
            roles,
            Role::ALL.to_vec(),
            "set must cover Role::ALL in order"
        );
    }

    #[test]
    fn label_hierarchy_non_strict_across_broad_grid() {
        // The label hierarchy must hold (primary >= secondary >= tertiary >=
        // quaternary in absolute |Lc|) across a 6-background x 2-VC grid
        // including non-reachable mid-greys. quaternary is the only role allowed
        // to equal its senior (it carries no readability floor); primary/
        // secondary/tertiary are strict by the anchor principle. When the
        // hierarchy is compressed (readable window narrower than steps) the
        // `compressed` flag must be set, never a silent equality.
        const BROAD_BGS: [&str; 6] = [
            "#FFFFFF", "#000000", "#808080", "#3478F6", "#93939C", "#6D6C7E",
        ];
        for (vc, vc_name) in vcs() {
            for bg_hex in BROAD_BGS {
                let bg = BgInput::solid(bg_hex).unwrap();
                let set = resolve_set(&bg, &table_default(), &vc);
                let mags: Vec<Option<f64>> = TEXT_ORDER
                    .iter()
                    .map(|&r| set_lc_opt(&set, r).map(f64::abs))
                    .collect();
                // primary >= secondary
                if let (Some(p), Some(s)) = (mags[0], mags[1]) {
                    assert!(
                        p + 1e-9 >= s,
                        "{vc_name} {bg_hex}: primary {p} must not be weaker than secondary {s}"
                    );
                }
                // secondary >= tertiary
                if let (Some(s), Some(t)) = (mags[1], mags[2]) {
                    assert!(
                        s + 1e-9 >= t,
                        "{vc_name} {bg_hex}: secondary {s} must not be weaker than tertiary {t}"
                    );
                }
                // tertiary >= quaternary (quaternary may equal)
                if let (Some(t), Some(q)) = (mags[2], mags[3]) {
                    assert!(
                        t + 1e-9 >= q,
                        "{vc_name} {bg_hex}: tertiary {t} must not be weaker than quaternary {q}"
                    );
                }
                // Where reachable, primary/secondary/tertiary must be strict
                // (quaternary is the only non-strict step in the anchor ladder).
                for w in mags.windows(3) {
                    if let (Some(a), Some(b), Some(c)) = (w[0], w[1], w[2]) {
                        assert!(
                            a > b + 1e-9 && b > c + 1e-9,
                            "{vc_name} {bg_hex}: strict-order violated in primary/secondary/tertiary window: {a} > {b} > {c}"
                        );
                    }
                }
                // No two adjacent roles share an identical colour without being
                // flagged compressed.
                for window in TEXT_ORDER.windows(2) {
                    let (Some((sh, sc)), Some((jh, jc))) = (
                        set_hex_and_flag(&set, window[0]),
                        set_hex_and_flag(&set, window[1]),
                    ) else {
                        continue;
                    };
                    if sh == jh {
                        assert!(
                            sc || jc,
                            "{vc_name} {bg_hex}: {} and {} share hex {sh} without compression flag",
                            window[0].key(),
                            window[1].key()
                        );
                    }
                }
            }
        }
    }

    // ── Neutral undertone: identity, not sterile grey ─────────────────────────

    /// The Oklab `(a, b, chroma, hue°)` of a role's resolved hex — measured on
    /// the emitted colour, the value the caller actually gets.
    fn resolved_oklab(
        bg: &BgInput,
        role: Role,
        table: &RoleTable,
        vc: &ViewingConditions,
    ) -> [f64; 4] {
        use crate::spaces::oklab::{oklab_hue, srgb_linear_to_oklab};
        use crate::spaces::srgb::srgb_from_hex;
        let solved = match resolve(bg, role, table, vc) {
            Ok(Resolved::Color { solved, .. }) => solved,
            other => panic!("{} expected a colour, got {other:?}", role.key()),
        };
        let rgb = srgb_from_hex(solved.hex()).unwrap();
        let lab = srgb_linear_to_oklab(rgb);
        let chroma = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
        [lab[1], lab[2], chroma, oklab_hue(rgb)]
    }

    #[test]
    fn primary_on_white_is_a_relative_of_the_neutral_not_pure_grey() {
        // The headline sanity: text-primary on white must be a cool near-black in
        // the #101012 family — undertone preserved — NOT the sterile grey #141414
        // a zero-chroma policy produced. Measured on the emitted hex: it carries
        // real chroma, and the tint direction is cool (Oklab b < 0, like #101012),
        // i.e. the blue channel exceeds the red.
        use crate::spaces::srgb::srgb_from_hex;
        let table = RoleTable::default();
        for (vc, vc_name) in vcs() {
            let bg = BgInput::solid("#FFFFFF").unwrap();
            let solved = match resolve(&bg, Role::LabelPrimary, &table, &vc) {
                Ok(Resolved::Color { solved, .. }) => solved,
                other => panic!("{other:?}"),
            };
            assert_ne!(
                solved.hex().to_uppercase(),
                "#141414",
                "{vc_name}: primary on white is the sterile grey — undertone lost"
            );
            let [_a, b, chroma, _hue] = resolved_oklab(&bg, Role::LabelPrimary, &table, &vc);
            assert!(
                chroma > 1e-3,
                "{vc_name}: primary on white carries no chroma ({chroma}) — pure grey"
            );
            // Cool undertone: Oklab b is negative for the #101012 family, and the
            // emitted blue byte sits above the red byte.
            assert!(b < 0.0, "{vc_name}: primary undertone is not cool (b={b})");
            let rgb_q = srgb_from_hex(solved.hex()).unwrap();
            assert!(
                rgb_q[2] >= rgb_q[0],
                "{vc_name}: primary blue channel must lead red for a cool tint, hex {}",
                solved.hex()
            );
        }
    }

    #[test]
    fn resolved_text_roles_share_the_neutral_hue() {
        // Every text role with enough chroma to carry a reliable hue resolves
        // near the neutral's Oklab hue (~286°) on white and black — the undertone
        // is the neutral's, not an arbitrary tint. Near-black / near-white roles
        // whose chroma is below the quantisation-reliable threshold are checked
        // for cool *direction* (b < 0) instead, since their hue is float-fragile.
        let table = RoleTable::default();
        for (vc, vc_name) in vcs() {
            for bg_hex in ["#FFFFFF", "#101012"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                for &role in &TEXT_ORDER {
                    let [_a, b, chroma, hue] = resolved_oklab(&bg, role, &table, &vc);
                    if chroma > 4e-3 {
                        let dh = (hue - NEUTRAL_HUE_DEG + 180.0).rem_euclid(360.0) - 180.0;
                        assert!(
                            dh.abs() <= 12.0,
                            "{vc_name} {bg_hex} {}: hue {hue}° off neutral {NEUTRAL_HUE_DEG}° by {dh}°",
                            role.key()
                        );
                    } else {
                        assert!(
                            b <= 1e-6,
                            "{vc_name} {bg_hex} {}: faint tint must still be cool (b={b})",
                            role.key()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn role_chroma_holds_constant_perceptual_colorfulness() {
        // CONTRACT CHANGE (tint-identity-curve, owner decision 2026-06-12). The v1
        // envelope tied chroma to `ratio · max_chroma(L)`, so colourfulness tracked
        // the gamut shape: over-saturated in the middle (secondary M' ~10) and
        // starved at the light end (primary-on-dark M' ~1.8) — the "inverted
        // envelope" the owner objected to. The v2 curve instead holds a *constant
        // perceptual colourfulness* (CAM16-UCS M') across the ladder. The faithful
        // invariant is therefore on M', not on a gamut envelope: every reachable
        // text role carries M' within a tight band of the target, and the middle
        // is no longer richer than the reference's plateau.
        let vc = ViewingConditions::srgb();
        for bg_hex in ["#FFFFFF", "#101012", "#1C1C1E"] {
            let bg = BgInput::solid(bg_hex).unwrap();
            let mps: Vec<(Role, f64, f64)> = TEXT_ORDER
                .iter()
                .map(|&r| {
                    let solved = match resolve(&bg, r, &RoleTable::default(), &vc) {
                        Ok(Resolved::Color { solved, .. }) => solved,
                        other => panic!("{other:?}"),
                    };
                    let rgb = crate::spaces::srgb::srgb_from_hex(solved.hex()).unwrap();
                    let l = crate::spaces::oklab::srgb_linear_to_oklab(rgb)[0];
                    let mp = crate::lcs::LcsColor::from_hex_with_vc(solved.hex(), &vc)
                        .unwrap()
                        .mp();
                    let emitted = Srgb8::new(
                        crate::srgb8::hex_bytes(solved.hex())
                            .expect("solver output is canonical sRGB8"),
                    );
                    assert!(
                        !emitted.is_achromatic(),
                        "{bg_hex} {}: curve emission collapsed to exact grey",
                        r.key()
                    );
                    (r, l, mp)
                })
                .collect();

            // Constant colourfulness: every role whose lightness leaves the gamut
            // room to host the target sits within a tight band of it. Roles pinned
            // against the white/black wall (L very near 0 or 1) are allowed to fall
            // *below* target — the honest mechanism-3 release, never above it.
            for (role, l, mp) in &mps {
                let near_wall = *l < 0.18 || *l > 0.95;
                if near_wall {
                    assert!(
                        *mp <= TINT_TARGET_MP + 1.5,
                        "{bg_hex} {}: wall role over target (M' {mp}, L {l})",
                        role.key()
                    );
                } else {
                    assert!(
                        (*mp - TINT_TARGET_MP).abs() <= 1.5,
                        "{bg_hex} {}: M' {mp} strays from target {TINT_TARGET_MP} (L {l})",
                        role.key()
                    );
                }
            }

            // The middle is no longer over-saturated past the reference plateau:
            // no role exceeds the reference's own peak colourfulness (M' ~6.8 at
            // #9698A2) by more than the quantisation slack.
            let max_mp = mps.iter().map(|&(_, _, m)| m).fold(0.0_f64, f64::max);
            assert!(
                max_mp <= 8.5,
                "{bg_hex}: a role exceeds the reference colourfulness ceiling: {mps:?}"
            );
        }
    }

    #[test]
    fn custom_neutral_chroma_overrides_the_tint_to_pure_grey() {
        // The override seam: a table whose chroma policy is set to Neutral resolves
        // every role as a pure grey again — the default tint is replaced wholesale,
        // including dropping the undertone to exactly zero. This is the configurable
        // policy the task requires: a custom RoleChroma beats the default fully.
        let table = RoleTable::default().with_chroma(RoleChroma::Neutral);
        for (vc, vc_name) in vcs() {
            for bg_hex in ["#FFFFFF", "#101012"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                for &role in &TEXT_ORDER {
                    let [a, b, chroma, _hue] = resolved_oklab(&bg, role, &table, &vc);
                    assert!(
                        chroma < 1e-3,
                        "{vc_name} {bg_hex} {}: neutral override still tinted (a={a}, b={b})",
                        role.key()
                    );
                }
            }
        }
        // And the achromatic override reproduces the historic sterile grey exactly.
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let grey = match resolve(&bg, Role::LabelPrimary, &table, &ViewingConditions::srgb()) {
            Ok(Resolved::Color { solved, .. }) => solved.hex().to_uppercase(),
            other => panic!("{other:?}"),
        };
        assert_eq!(grey, "#141414", "neutral override must restore pure grey");
    }

    #[test]
    fn explicit_client_tinted_policy_remains_supported() {
        // These literals are a test client's declared policy, not a Core default.
        // The generic variant must keep the supplied fixed hue and gamut ratio.
        let vc = ViewingConditions::srgb();
        let flat = RoleTable::default().with_chroma(RoleChroma::Tinted {
            hue_deg: 286.0,
            ratio: 0.10,
        });
        let bg = BgInput::solid("#FFFFFF").unwrap();
        // Secondary under the flat policy lands cool, around the canonical hue, and
        // carries a flat ratio of the gamut max — its chroma is `RATIO * max_chroma`.
        let [_a, b, chroma, _hue] = resolved_oklab(&bg, Role::LabelSecondary, &flat, &vc);
        assert!(b < 0.0, "flat tint must stay cool (b={b})");
        let solved = match resolve(&bg, Role::LabelSecondary, &flat, &vc) {
            Ok(Resolved::Color { solved, .. }) => solved,
            other => panic!("{other:?}"),
        };
        let l = crate::spaces::oklab::srgb_linear_to_oklab(
            crate::spaces::srgb::srgb_from_hex(solved.hex()).unwrap(),
        )[0];
        let expected = 0.10 * scale::max_chroma(l, 286.0);
        assert!(
            (chroma - expected).abs() <= 2e-3,
            "flat tint chroma {chroma:.4} should be ratio*max_chroma {expected:.4}"
        );
    }

    #[test]
    fn curve_text_roles_share_the_canonical_hue_on_white_and_dark() {
        // The v2 curve's hue path: every text role with enough chroma to carry a
        // reliable hue resolves near the canonical 286 on white and on a dark bg —
        // the geometry-pinned blue-violet undertone, not a magenta or azure wander.
        let vc = ViewingConditions::srgb();
        let table = RoleTable::default();
        for bg_hex in ["#FFFFFF", "#1C1C1E"] {
            let bg = BgInput::solid(bg_hex).unwrap();
            for &role in &TEXT_ORDER {
                let [_a, _b, chroma, hue] = resolved_oklab(&bg, role, &table, &vc);
                if chroma > 4e-3 {
                    let dh = (hue - NEUTRAL_HUE_DEG + 180.0).rem_euclid(360.0) - 180.0;
                    assert!(
                        dh.abs() <= 14.0,
                        "{bg_hex} {}: curve hue {hue:.1} off canonical {NEUTRAL_HUE_DEG} by {dh:.1}",
                        role.key()
                    );
                }
            }
        }
    }

    #[test]
    fn resolve_set_golden_hex_is_byte_for_byte_stable() {
        // GOLDEN LOCK (tint-identity-curve perf fix, 2026-06-12). The owner has
        // signed off the v2 undertone's *visual* result; the perf work (analytic
        // max_chroma, allocation-free cubic solver, early-exit bisections, and the
        // y_hk / curve-plan memos) must not move a single emitted byte. This is the
        // verifier's 6-background × 2-VC grid, captured BEFORE the perf fix and
        // frozen here: every role's hex must match exactly. A change to any value
        // means the approved visual output drifted — re-approval by the owner is
        // required, never a silent edit of this table.
        //
        // EXPANDED for the HIG role taxonomy (role-taxonomy-hig). Two distinct
        // kinds of row live here, on purpose:
        //
        //   * The `label-*` rows (and `icon` / `separator` / `none`) are the OLD,
        //     owner-approved colours under their new HIG keys. Renaming text-* to
        //     label-* moved keys, never colours: each `label-primary/secondary/
        //     tertiary/quaternary` hex is byte-identical to the prior `text-primary/
        //     secondary/muted/disabled` hex on the same (vc, bg). `border-strong`
        //     reuses the label-primary contract, so it too matches label-primary.
        //     These rows are the control that proves the rename was pure.
        //
        //   * The `border-base/soft` and `fill-*` rows are dJ' decorative roles
        //     carrying the owner's LITERAL Figma-computed perceived-lightness
        //     anchors (per theme). They changed type from the earlier Lc block-lift
        //     placeholder to dJ' (role-taxonomy-hig doraботка, owner's iron rule:
        //     no substitute units) — so their colours moved here ON PURPOSE, with
        //     this comment, the change of unit being the reason. The unit, type,
        //     and source of each anchor are the owner's. The `shadow-*` rows stay
        //     Lc `Decorative` (the owner's shadow anchors are alpha opacities,
        //     not dJ' steps); frozen so a refactor cannot move them.
        const GOLDEN: [(&str, &str, &str, &str); 228] = [
            ("srgb", "#FFFFFF", "label-primary", "#14131A"),
            ("srgb", "#FFFFFF", "label-secondary", "#75757E"),
            ("srgb", "#FFFFFF", "label-tertiary", "#94949E"),
            ("srgb", "#FFFFFF", "label-quaternary", "#C1C1CB"),
            ("srgb", "#FFFFFF", "separator", "#EBECF6"),
            ("srgb", "#FFFFFF", "border-strong", "#14131A"),
            ("srgb", "#FFFFFF", "border-base", "#E8E8F3"),
            ("srgb", "#FFFFFF", "border-soft", "#F3F3FE"),
            ("srgb", "#FFFFFF", "border-none", "none"),
            ("srgb", "#FFFFFF", "fill-primary", "#E3E3ED"),
            ("srgb", "#FFFFFF", "fill-secondary", "#E8E8F3"),
            ("srgb", "#FFFFFF", "fill-tertiary", "#EEEEF9"),
            ("srgb", "#FFFFFF", "fill-quaternary", "#F3F3FE"),
            ("srgb", "#FFFFFF", "fill-none", "none"),
            ("srgb", "#FFFFFF", "shadow-minor", "#EBECF6"),
            ("srgb", "#FFFFFF", "shadow-ambient", "#E9E9F3"),
            ("srgb", "#FFFFFF", "shadow-penumbra", "#E5E5EF"),
            ("srgb", "#FFFFFF", "shadow-major", "#E1E1EA"),
            ("srgb", "#FFFFFF", "none", "none"),
            ("srgb", "#F2F2F7", "label-primary", "#131219"),
            ("srgb", "#F2F2F7", "label-secondary", "#6F6E77"),
            ("srgb", "#F2F2F7", "label-tertiary", "#8B8B95"),
            ("srgb", "#F2F2F7", "label-quaternary", "#B8B8C1"),
            ("srgb", "#F2F2F7", "separator", "#DFDFE8"),
            ("srgb", "#F2F2F7", "border-strong", "#131219"),
            ("srgb", "#F2F2F7", "border-base", "#DCDDE6"),
            ("srgb", "#F2F2F7", "border-soft", "#E7E7F0"),
            ("srgb", "#F2F2F7", "border-none", "none"),
            ("srgb", "#F2F2F7", "fill-primary", "#D8D8E1"),
            ("srgb", "#F2F2F7", "fill-secondary", "#DCDDE6"),
            ("srgb", "#F2F2F7", "fill-tertiary", "#E2E2EC"),
            ("srgb", "#F2F2F7", "fill-quaternary", "#E7E7F0"),
            ("srgb", "#F2F2F7", "fill-none", "none"),
            ("srgb", "#F2F2F7", "shadow-minor", "#DFDFE8"),
            ("srgb", "#F2F2F7", "shadow-ambient", "#DCDCE6"),
            ("srgb", "#F2F2F7", "shadow-penumbra", "#D8D9E2"),
            ("srgb", "#F2F2F7", "shadow-major", "#D4D4DD"),
            ("srgb", "#F2F2F7", "none", "none"),
            ("srgb", "#7F7F7F", "label-primary", "#08070E"),
            ("srgb", "#7F7F7F", "label-secondary", "#16151C"),
            ("srgb", "#7F7F7F", "label-tertiary", "#36353D"),
            ("srgb", "#7F7F7F", "label-quaternary", "#5F5F67"),
            ("srgb", "#7F7F7F", "separator", "#686870"),
            ("srgb", "#7F7F7F", "border-strong", "#08070E"),
            ("srgb", "#7F7F7F", "border-base", "#6F6F77"),
            ("srgb", "#7F7F7F", "border-soft", "#76767F"),
            ("srgb", "#7F7F7F", "border-none", "none"),
            ("srgb", "#7F7F7F", "fill-primary", "#6B6B73"),
            ("srgb", "#7F7F7F", "fill-secondary", "#6F6F77"),
            ("srgb", "#7F7F7F", "fill-tertiary", "#73737B"),
            ("srgb", "#7F7F7F", "fill-quaternary", "#76767F"),
            ("srgb", "#7F7F7F", "fill-none", "none"),
            ("srgb", "#7F7F7F", "shadow-minor", "#686870"),
            ("srgb", "#7F7F7F", "shadow-ambient", "#64646D"),
            ("srgb", "#7F7F7F", "shadow-penumbra", "#606068"),
            ("srgb", "#7F7F7F", "shadow-major", "#5A5A62"),
            ("srgb", "#7F7F7F", "none", "none"),
            ("srgb", "#1C1C1E", "label-primary", "#FAFAFF"),
            ("srgb", "#1C1C1E", "label-secondary", "#C0C0C9"),
            ("srgb", "#1C1C1E", "label-tertiary", "#9F9FA8"),
            ("srgb", "#1C1C1E", "label-quaternary", "#77777F"),
            ("srgb", "#1C1C1E", "separator", "#3E3E45"),
            ("srgb", "#1C1C1E", "border-strong", "#FAFAFF"),
            ("srgb", "#1C1C1E", "border-base", "#2B2B32"),
            ("srgb", "#1C1C1E", "border-soft", "#23232A"),
            ("srgb", "#1C1C1E", "border-none", "none"),
            ("srgb", "#1C1C1E", "fill-primary", "#2E2E35"),
            ("srgb", "#1C1C1E", "fill-secondary", "#2B2B32"),
            ("srgb", "#1C1C1E", "fill-tertiary", "#26262E"),
            ("srgb", "#1C1C1E", "fill-quaternary", "#23232A"),
            ("srgb", "#1C1C1E", "fill-none", "none"),
            ("srgb", "#1C1C1E", "shadow-minor", "#3E3E45"),
            ("srgb", "#1C1C1E", "shadow-ambient", "#42424A"),
            ("srgb", "#1C1C1E", "shadow-penumbra", "#48484F"),
            ("srgb", "#1C1C1E", "shadow-major", "#4F4F57"),
            ("srgb", "#1C1C1E", "none", "none"),
            ("srgb", "#101012", "label-primary", "#FAFAFF"),
            ("srgb", "#101012", "label-secondary", "#BEBEC8"),
            ("srgb", "#101012", "label-tertiary", "#9D9DA6"),
            ("srgb", "#101012", "label-quaternary", "#74747C"),
            ("srgb", "#101012", "separator", "#393940"),
            ("srgb", "#101012", "border-strong", "#FAFAFF"),
            ("srgb", "#101012", "border-base", "#1F1F27"),
            ("srgb", "#101012", "border-soft", "#18171E"),
            ("srgb", "#101012", "border-none", "none"),
            ("srgb", "#101012", "fill-primary", "#23232A"),
            ("srgb", "#101012", "fill-secondary", "#1F1F27"),
            ("srgb", "#101012", "fill-tertiary", "#1B1B21"),
            ("srgb", "#101012", "fill-quaternary", "#18171E"),
            ("srgb", "#101012", "fill-none", "none"),
            ("srgb", "#101012", "shadow-minor", "#393940"),
            ("srgb", "#101012", "shadow-ambient", "#3D3D44"),
            ("srgb", "#101012", "shadow-penumbra", "#43434A"),
            ("srgb", "#101012", "shadow-major", "#4A4A52"),
            ("srgb", "#101012", "none", "none"),
            ("srgb", "#3478F6", "label-primary", "#08070D"),
            ("srgb", "#3478F6", "label-secondary", "#14141B"),
            ("srgb", "#3478F6", "label-tertiary", "#35343C"),
            ("srgb", "#3478F6", "label-quaternary", "#5E5E67"),
            ("srgb", "#3478F6", "separator", "#67676F"),
            ("srgb", "#3478F6", "border-strong", "#08070D"),
            ("srgb", "#3478F6", "border-base", "#6E6E76"),
            ("srgb", "#3478F6", "border-soft", "#76767E"),
            ("srgb", "#3478F6", "border-none", "none"),
            ("srgb", "#3478F6", "fill-primary", "#6A6A73"),
            ("srgb", "#3478F6", "fill-secondary", "#6E6E76"),
            ("srgb", "#3478F6", "fill-tertiary", "#72727B"),
            ("srgb", "#3478F6", "fill-quaternary", "#76767E"),
            ("srgb", "#3478F6", "fill-none", "none"),
            ("srgb", "#3478F6", "shadow-minor", "#67676F"),
            ("srgb", "#3478F6", "shadow-ambient", "#63636B"),
            ("srgb", "#3478F6", "shadow-penumbra", "#5E5E67"),
            ("srgb", "#3478F6", "shadow-major", "#585861"),
            ("srgb", "#3478F6", "none", "none"),
            ("dim", "#FFFFFF", "label-primary", "#141419"),
            ("dim", "#FFFFFF", "label-secondary", "#75757E"),
            ("dim", "#FFFFFF", "label-tertiary", "#94949D"),
            ("dim", "#FFFFFF", "label-quaternary", "#C1C1CB"),
            ("dim", "#FFFFFF", "separator", "#ECECF5"),
            ("dim", "#FFFFFF", "border-strong", "#141419"),
            ("dim", "#FFFFFF", "border-base", "#D7D7E0"),
            ("dim", "#FFFFFF", "border-soft", "#E7E7F0"),
            ("dim", "#FFFFFF", "border-none", "none"),
            ("dim", "#FFFFFF", "fill-primary", "#BDBDC6"),
            ("dim", "#FFFFFF", "fill-secondary", "#C3C3CC"),
            ("dim", "#FFFFFF", "fill-tertiary", "#D0D0DA"),
            ("dim", "#FFFFFF", "fill-quaternary", "#DEDEE7"),
            ("dim", "#FFFFFF", "fill-none", "none"),
            ("dim", "#FFFFFF", "shadow-minor", "#ECECF5"),
            ("dim", "#FFFFFF", "shadow-ambient", "#E9E9F2"),
            ("dim", "#FFFFFF", "shadow-penumbra", "#E5E5EF"),
            ("dim", "#FFFFFF", "shadow-major", "#E1E1EA"),
            ("dim", "#FFFFFF", "none", "none"),
            ("dim", "#F2F2F7", "label-primary", "#131218"),
            ("dim", "#F2F2F7", "label-secondary", "#6F6E77"),
            ("dim", "#F2F2F7", "label-tertiary", "#8B8B94"),
            ("dim", "#F2F2F7", "label-quaternary", "#B8B8C1"),
            ("dim", "#F2F2F7", "separator", "#DFDFE8"),
            ("dim", "#F2F2F7", "border-strong", "#131218"),
            ("dim", "#F2F2F7", "border-base", "#CCCCD5"),
            ("dim", "#F2F2F7", "border-soft", "#DBDBE5"),
            ("dim", "#F2F2F7", "border-none", "none"),
            ("dim", "#F2F2F7", "fill-primary", "#B2B2BC"),
            ("dim", "#F2F2F7", "fill-secondary", "#B9B9C2"),
            ("dim", "#F2F2F7", "fill-tertiary", "#C5C5CF"),
            ("dim", "#F2F2F7", "fill-quaternary", "#D3D3DC"),
            ("dim", "#F2F2F7", "fill-none", "none"),
            ("dim", "#F2F2F7", "shadow-minor", "#DFDFE8"),
            ("dim", "#F2F2F7", "shadow-ambient", "#DCDCE6"),
            ("dim", "#F2F2F7", "shadow-penumbra", "#D8D9E2"),
            ("dim", "#F2F2F7", "shadow-major", "#D4D4DD"),
            ("dim", "#F2F2F7", "none", "none"),
            ("dim", "#7F7F7F", "label-primary", "#08080C"),
            ("dim", "#7F7F7F", "label-secondary", "#16161B"),
            ("dim", "#7F7F7F", "label-tertiary", "#36353D"),
            ("dim", "#7F7F7F", "label-quaternary", "#5F5F67"),
            ("dim", "#7F7F7F", "separator", "#686870"),
            ("dim", "#7F7F7F", "border-strong", "#08080C"),
            ("dim", "#7F7F7F", "border-base", "#64646C"),
            ("dim", "#7F7F7F", "border-soft", "#6F6F77"),
            ("dim", "#7F7F7F", "border-none", "none"),
            ("dim", "#7F7F7F", "fill-primary", "#525259"),
            ("dim", "#7F7F7F", "fill-secondary", "#56565D"),
            ("dim", "#7F7F7F", "fill-tertiary", "#5F5F67"),
            ("dim", "#7F7F7F", "fill-quaternary", "#696971"),
            ("dim", "#7F7F7F", "fill-none", "none"),
            ("dim", "#7F7F7F", "shadow-minor", "#686870"),
            ("dim", "#7F7F7F", "shadow-ambient", "#64646D"),
            ("dim", "#7F7F7F", "shadow-penumbra", "#606068"),
            ("dim", "#7F7F7F", "shadow-major", "#5A5A61"),
            ("dim", "#7F7F7F", "none", "none"),
            ("dim", "#1C1C1E", "label-primary", "#FAFAFF"),
            ("dim", "#1C1C1E", "label-secondary", "#C0C0C9"),
            ("dim", "#1C1C1E", "label-tertiary", "#9F9FA8"),
            ("dim", "#1C1C1E", "label-quaternary", "#77777F"),
            ("dim", "#1C1C1E", "separator", "#3E3E45"),
            ("dim", "#1C1C1E", "border-strong", "#FAFAFF"),
            ("dim", "#1C1C1E", "border-base", "#313137"),
            ("dim", "#1C1C1E", "border-soft", "#28282E"),
            ("dim", "#1C1C1E", "border-none", "none"),
            ("dim", "#1C1C1E", "fill-primary", "#424249"),
            ("dim", "#1C1C1E", "fill-secondary", "#3D3D45"),
            ("dim", "#1C1C1E", "fill-tertiary", "#35353C"),
            ("dim", "#1C1C1E", "fill-quaternary", "#2D2D33"),
            ("dim", "#1C1C1E", "fill-none", "none"),
            ("dim", "#1C1C1E", "shadow-minor", "#3E3E45"),
            ("dim", "#1C1C1E", "shadow-ambient", "#42424A"),
            ("dim", "#1C1C1E", "shadow-penumbra", "#48484F"),
            ("dim", "#1C1C1E", "shadow-major", "#4F4F56"),
            ("dim", "#1C1C1E", "none", "none"),
            ("dim", "#101012", "label-primary", "#FAFAFF"),
            ("dim", "#101012", "label-secondary", "#BEBEC8"),
            ("dim", "#101012", "label-tertiary", "#9D9DA6"),
            ("dim", "#101012", "label-quaternary", "#74747C"),
            ("dim", "#101012", "separator", "#393940"),
            ("dim", "#101012", "border-strong", "#FAFAFF"),
            ("dim", "#101012", "border-base", "#25252B"),
            ("dim", "#101012", "border-soft", "#1C1C22"),
            ("dim", "#101012", "border-none", "none"),
            ("dim", "#101012", "fill-primary", "#35353C"),
            ("dim", "#101012", "fill-secondary", "#313137"),
            ("dim", "#101012", "fill-tertiary", "#29292F"),
            ("dim", "#101012", "fill-quaternary", "#212127"),
            ("dim", "#101012", "fill-none", "none"),
            ("dim", "#101012", "shadow-minor", "#393940"),
            ("dim", "#101012", "shadow-ambient", "#3D3D44"),
            ("dim", "#101012", "shadow-penumbra", "#43434A"),
            ("dim", "#101012", "shadow-major", "#4A4A51"),
            ("dim", "#101012", "none", "none"),
            ("dim", "#3478F6", "label-primary", "#08070C"),
            ("dim", "#3478F6", "label-secondary", "#15141A"),
            ("dim", "#3478F6", "label-tertiary", "#35343B"),
            ("dim", "#3478F6", "label-quaternary", "#5E5E67"),
            ("dim", "#3478F6", "separator", "#67676F"),
            ("dim", "#3478F6", "border-strong", "#08070C"),
            ("dim", "#3478F6", "border-base", "#63636C"),
            ("dim", "#3478F6", "border-soft", "#6E6E76"),
            ("dim", "#3478F6", "border-none", "none"),
            ("dim", "#3478F6", "fill-primary", "#515158"),
            ("dim", "#3478F6", "fill-secondary", "#56565D"),
            ("dim", "#3478F6", "fill-tertiary", "#5F5F67"),
            ("dim", "#3478F6", "fill-quaternary", "#686870"),
            ("dim", "#3478F6", "fill-none", "none"),
            ("dim", "#3478F6", "shadow-minor", "#67676F"),
            ("dim", "#3478F6", "shadow-ambient", "#63636B"),
            ("dim", "#3478F6", "shadow-penumbra", "#5E5E67"),
            ("dim", "#3478F6", "shadow-major", "#585860"),
            ("dim", "#3478F6", "none", "none"),
        ];

        let table = RoleTable::default();
        for (vc, vc_name) in vcs() {
            for bg_hex in [
                "#FFFFFF", "#F2F2F7", "#7F7F7F", "#1C1C1E", "#101012", "#3478F6",
            ] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let set = resolve_set(&bg, &table, &vc);
                for (role, res) in &set {
                    let got = match res {
                        Resolved::Color { solved, .. } => solved.hex().to_string(),
                        // Дефолтная таблица не несёт Ladder/AlphaAnalog/Glow — недостижимо.
                        Resolved::Translucent(r) => format!("rgba({},{})", r.tint_hex(), r.alpha()),
                        Resolved::Glow(g) => format!("glow({},{})", g.halo_hex(), g.alpha()),
                        Resolved::GlowIndeterminate(_) => "GLOW_INDETERMINATE".to_string(),
                        Resolved::Material(m) => {
                            format!("material({},{:.4})", m.tint_hex(), m.alpha())
                        }
                        Resolved::None => "none".to_string(),
                        Resolved::Failure(failure) => {
                            crate::test_support::role_failure_repr(failure)
                        }
                    };
                    let want = GOLDEN
                        .iter()
                        .find(|(v, b, r, _)| *v == vc_name && *b == bg_hex && *r == role.key())
                        .map(|(_, _, _, hex)| *hex)
                        .unwrap_or_else(|| {
                            panic!("no golden row for {vc_name} {bg_hex} {}", role.key())
                        });
                    assert_eq!(
                        got,
                        want,
                        "GOLDEN DRIFT {vc_name} {bg_hex} {}: got {got}, approved {want}",
                        role.key()
                    );
                }
            }
        }
    }

    #[test]
    fn custom_tint_overrides_hue_and_ratio() {
        // The override is not limited to dropping the tint: a different Tinted
        // policy resolves roles around its own hue. A warm 30° undertone must land
        // the roles warm (Oklab b > 0), not at the cool default — proving the whole
        // policy, hue and ratio, is configurable.
        let vc = ViewingConditions::srgb();
        let warm = RoleTable::default().with_chroma(RoleChroma::Tinted {
            hue_deg: 30.0,
            ratio: 0.10,
        });
        let bg = BgInput::solid("#FFFFFF").unwrap();
        let [_a, b, chroma, _hue] = resolved_oklab(&bg, Role::LabelSecondary, &warm, &vc);
        assert!(chroma > 1e-3, "warm tint carries chroma");
        assert!(b > 0.0, "warm 30° undertone must be warm (b={b}), not cool");
    }

    #[test]
    fn tint_preserves_the_contrast_target_and_floor() {
        // The undertone must not move the contrast: the tinted default and the
        // achromatic override land within the 1-Lc quantisation budget of each
        // other on the same role, and both clear the WCAG floor. Identity is added
        // without surprising the contrast contract.
        let vc = ViewingConditions::srgb();
        let tinted = RoleTable::default();
        let grey = RoleTable::default().with_chroma(RoleChroma::Neutral);
        for bg_hex in ["#FFFFFF", "#101012"] {
            let bg = BgInput::solid(bg_hex).unwrap();
            for (role, min_ratio) in [
                (Role::LabelPrimary, 4.5),
                (Role::LabelSecondary, 4.5),
                (Role::LabelTertiary, 3.0),
            ] {
                let t = match resolve(&bg, role, &tinted, &vc) {
                    Ok(Resolved::Color { solved, .. }) => solved,
                    other => panic!("{other:?}"),
                };
                let g = match resolve(&bg, role, &grey, &vc) {
                    Ok(Resolved::Color { solved, .. }) => solved,
                    other => panic!("{other:?}"),
                };
                // W5: солвер не поднимает цели полом, потому обе стороны
                // обязаны попасть в один Lc-бюджет без floor-исключений.
                assert!(
                    (t.lc().abs() - g.lc().abs()).abs() <= 1.0,
                    "{bg_hex} {}: tint moved a candidate-score target (tinted {} vs grey {})",
                    role.key(),
                    t.lc(),
                    g.lc()
                );
                // Нормативный критерий финальной пары — канонический evaluator.
                let criterion = if min_ratio >= 4.5 {
                    Wcag22CriterionV1::Sc143TextDefault
                } else {
                    Wcag22CriterionV1::Sc1411UiComponentOrState
                };
                let assessment = crate::wcag22::evaluate_wcag22_hex(t.hex(), bg_hex, criterion)
                    .expect("emitted hex is admitted sRGB8");
                assert!(
                    matches!(
                        assessment,
                        crate::wcag22::Wcag22AssessmentV1::Evaluated {
                            decision: crate::wcag22::Wcag22ApplicableDecisionV1::Pass,
                            ..
                        }
                    ),
                    "{bg_hex} {}: tinted role fails {criterion:?} ({})",
                    role.key(),
                    t.hex()
                );
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Дериватор-локи (Fowler class Б: characterization). Эти тесты НЕ выводят
// константы из первых принципов — они ИЗМЕРЯЮТ перцептивную величину, к которой
// привязана каждая калибровочная константа, и фиксируют измеренное отношение как
// регрессионный якорь. Где строгая деривация НЕ держится (замер это показал),
// граница честно широкая и помечена — БЕЗ подгонки под значение (North).
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod derivator_locks {
    use super::{CUSP_HALF_WINDOW_DEG, LIGHTNESS_SETTLE, STRICT_STEP};

    fn grey(i: u8) -> String {
        format!("#{i:02X}{i:02X}{i:02X}")
    }

    /// Контрпример закрывает весь класс ложных float-доказательств: даже очень
    /// малая разница Oklab L может пересекать границу конечного кодируемого
    /// состояния. Текущая legacy-эвристика поэтому остаётся незаверенной;
    /// будущая конечная замена #218 обязана сравнивать эмитированные состояния.
    #[test]
    fn oklab_lightness_distance_cannot_certify_equal_srgb8_output() {
        use crate::spaces::oklab::srgb_linear_to_oklab;
        use crate::spaces::srgb::srgb_from_hex;

        let almost_yellow_rgb = srgb_from_hex("#FFFF01").unwrap();
        let yellow_rgb = srgb_from_hex("#FFFF00").unwrap();
        let almost_yellow = srgb_linear_to_oklab(almost_yellow_rgb)[0];
        let yellow = srgb_linear_to_oklab(yellow_rgb)[0];
        let delta = (almost_yellow - yellow).abs();

        assert!(
            delta < LIGHTNESS_SETTLE,
            "контрпример обязан лежать ниже legacy-порога: {delta}"
        );
        assert_ne!(
            almost_yellow_rgb, yellow_rgb,
            "разные байты нельзя объявлять одним состоянием"
        );
    }

    /// Типичный (медианный) Lc-шаг одного 8-бит кванта серых < STRICT_STEP (0.5).
    /// ⚠️ ЗАМЕР: медианный шаг ≈0.44 < 0.5, но МАКСИМАЛЬНЫЙ шаг ≈7.85 — это обрыв
    /// мягкого клампа APCA (разрыв, не шаг сетки), а не «шаг кванта». Поэтому
    /// характеризуем МЕДИАННЫЙ шаг: STRICT_STEP=0.5 сидит чуть выше типичного шага
    /// выходной сетки — это граница квантования сетки, не JND-клейм.
    #[test]
    fn strict_step_sits_just_above_typical_grid_step() {
        let mut steps: Vec<f64> = Vec::new();
        let mut prev = crate::lpc::apparent_contrast_candidate_hex_for_test(&grey(0), "#FFFFFF")
            .expect("generated grey is valid");
        for i in 1u8..=255 {
            let lc = crate::lpc::apparent_contrast_candidate_hex_for_test(&grey(i), "#FFFFFF")
                .expect("generated grey is valid");
            steps.push((lc - prev).abs());
            prev = lc;
        }
        steps.sort_by(|a, b| a.partial_cmp(b).expect("Lc steps are finite"));
        let median = steps[steps.len() / 2];
        assert!(
            median < STRICT_STEP,
            "медианный Lc-шаг кванта {median:.4} должен быть ниже STRICT_STEP={STRICT_STEP}"
        );
        assert!(
            (0.35..0.50).contains(&median),
            "медианный Lc-шаг {median:.4} вне замеренного диапазона [0.35, 0.50)"
        );
        // Максимальный шаг ≈7.85 — это обрыв loClip мягкого клампа APCA (разрыв у
        // порога различимости, НЕ шаг сетки); лочим его отдельной полосой, чтобы он
        // не путался со STRICT_STEP и был зафиксирован как отдельный класс величины.
        let max_step = *steps.last().expect("непустой набор шагов");
        assert!(
            (7.0..8.7).contains(&max_step) && (max_step - 7.85).abs() < 0.5,
            "max Lc-шаг {max_step:.4} (обрыв loClip, не шаг сетки) вне замеренной полосы ~7.85"
        );
    }

    /// Дрейф каспа гамута sRGB вблизи канонического 286° по L-шкале задаёт
    /// CUSP_HALF_WINDOW_DEG (40°). ⚠️ ЗАМЕР: полный дрейф достигает ≈42.5°, т.е.
    /// окно поиска (40°) клипует чуть ВНУТРИ полного дрейфа — намеренно (движок
    /// держит оттенок близко к каноническому, см. `cusp_attracted_hue`). Поэтому
    /// НЕ утверждаем «окно покрывает дрейф»; характеризуем, что окно ≈ замеренный
    /// дрейф (в широкой полосе), без подгонки.
    #[test]
    fn cusp_window_is_near_measured_gamut_drift() {
        let canonical = 286.0_f64;
        let mut max_drift = 0.0f64;
        let mut l = 0.05;
        while l <= 0.95 {
            let mut best_h = canonical;
            let mut best_c = f64::NEG_INFINITY;
            let mut h = canonical - 70.0;
            while h <= canonical + 70.0 {
                let c = crate::scale::max_chroma(l, h);
                if c > best_c {
                    best_c = c;
                    best_h = h;
                }
                h += 0.5;
            }
            max_drift = max_drift.max((best_h - canonical).abs());
            l += 0.02;
        }
        assert!(
            (35.0..46.0).contains(&max_drift),
            "замеренный дрейф каспа {max_drift:.2} вне диапазона [35, 46)"
        );
        // Направленный ассерт (не |Δ|): окно (40°) клипует чуть ВНУТРИ полного дрейфа —
        // дрейф СТРОГО больше окна (max_drift > 40°) и < 46°. Инверсия направления
        // («окно покрывает дрейф») ломает тест.
        assert!(
            max_drift > CUSP_HALF_WINDOW_DEG && max_drift < 46.0,
            "замеренный дрейф каспа {max_drift:.2} должен СТРОГО превышать окно \
             CUSP_HALF_WINDOW_DEG={CUSP_HALF_WINDOW_DEG} (клип внутри — по дизайну) и быть < 46"
        );
    }
}

// EXPOSURE-анализ (волна science/constants-objectivization) для чувствительных
// констант semantic.rs: доля входа, где точное значение меняет классификационное
// решение. Продакшн НЕ трогается.
#[cfg(test)]
mod exposure_locks {
    use super::{DECORATIVE_FLOOR_MIN, IC_DECORATIVE_FLOOR_MIN, QUANT_GUARD, STRICT_STEP};

    /// DERIVED-лок (issue #44): `DECORATIVE_FLOOR_MIN == MODEL_LC_FLOOR + QUANT_GUARD`,
    /// и модельный пол строго ниже декоративного (guard положителен). Рантайм-зеркало
    /// компайл-тайм `const _`-проверки в semantic.rs; печатает разложение 7.5 = 7.3 + 0.2.
    // Намеренные constant-relationship пины (регресс-локи): clippy-lint
    // assertions_on_constants здесь ожидаем и точечно подавлен.
    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn decorative_floor_is_model_floor_plus_guard() {
        use crate::lpc::MODEL_LC_FLOOR;
        assert!(
            (DECORATIVE_FLOOR_MIN - (MODEL_LC_FLOOR + QUANT_GUARD)).abs() < 1e-9,
            "7.5 must decompose as MODEL_LC_FLOOR {MODEL_LC_FLOOR} + QUANT_GUARD {QUANT_GUARD}"
        );
        assert!(
            DECORATIVE_FLOOR_MIN > MODEL_LC_FLOOR,
            "decorative floor {DECORATIVE_FLOOR_MIN} must sit strictly above the model floor {MODEL_LC_FLOOR}"
        );
        assert!(
            QUANT_GUARD > 0.0,
            "guard must be positive, got {QUANT_GUARD}"
        );
        eprintln!(
            "DECORATIVE_FLOOR_MIN {DECORATIVE_FLOOR_MIN} = MODEL_LC_FLOOR {MODEL_LC_FLOOR} + QUANT_GUARD {QUANT_GUARD}"
        );
    }

    /// GROUNDED-лок: IC-пол равен опубликованному APCA-уровню Lc 15 (минимум
    /// различимости не-текста, draft), а порядкосохраняющий IC-сдвиг над обычным
    /// полом равен `15 − 7.5 = 7.5` — тот, что применяет `decorative_contract`.
    // Намеренные constant-relationship пины (регресс-локи): clippy-lint
    // assertions_on_constants здесь ожидаем и точечно подавлен.
    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn ic_floor_is_apca_lc15_with_order_preserving_shift() {
        assert!(
            (IC_DECORATIVE_FLOOR_MIN - 15.0).abs() < 1e-12,
            "IC floor must be the published APCA Lc 15, got {IC_DECORATIVE_FLOOR_MIN}"
        );
        assert!(IC_DECORATIVE_FLOOR_MIN > DECORATIVE_FLOOR_MIN);
        assert!(
            ((IC_DECORATIVE_FLOOR_MIN - DECORATIVE_FLOOR_MIN) - 7.5).abs() < 1e-9,
            "the -ic uniform shift must be 15 − 7.5 = 7.5"
        );
    }

    /// EXPOSURE STRICT_STEP: доля соседних Lc-шагов 8-бит серой сетки в +-20% полосе
    /// вокруг границы квантования — где точный STRICT_STEP решает «на сетке / нет».
    #[test]
    fn exposure_strict_step() {
        let greys: Vec<f64> = (0u16..=255)
            .map(|i| {
                crate::lpc::apparent_contrast_candidate_hex_for_test(
                    &format!("#{i:02X}{i:02X}{i:02X}", i = i as u8),
                    "#FFFFFF",
                )
                .expect("generated grey is valid")
            })
            .collect();
        let (lo, hi) = (0.8 * STRICT_STEP, 1.2 * STRICT_STEP);
        let (mut hits, mut tot) = (0usize, 0usize);
        for w in greys.windows(2) {
            let s = (w[1] - w[0]).abs();
            if s >= lo && s < hi {
                hits += 1;
            }
            tot += 1;
        }
        eprintln!(
            "EXPOSURE STRICT_STEP band=[{lo:.2},{hi:.2}] step_flip={:.2}%",
            100.0 * hits as f64 / tot as f64
        );
    }

    /// EXPOSURE декоративных полов: доля декоративного Lc-диапазона [0,30], чьё
    /// значение флипает «зажат полом / нет», пока пол ходит в +-25% полосе. Раздельно
    /// для обычного (7.5) и -ic (15.0) порогов.
    #[test]
    fn exposure_decorative_floors() {
        let frac = |floor: f64| {
            let (lo, hi) = (0.75 * floor, 1.25 * floor);
            let (mut hits, mut tot) = (0usize, 0usize);
            let mut t = 0.0;
            while t <= 30.0 {
                if t >= lo && t < hi {
                    hits += 1;
                }
                tot += 1;
                t += 0.05;
            }
            100.0 * hits as f64 / tot as f64
        };
        eprintln!(
            "EXPOSURE DECORATIVE_FLOOR_MIN(7.5) range_flip={:.2}% | IC_DECORATIVE_FLOOR_MIN(15.0) range_flip={:.2}%",
            frac(DECORATIVE_FLOOR_MIN),
            frac(IC_DECORATIVE_FLOOR_MIN)
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Волна 2 «объективизация» — терминалы (e)-констант semantic.rs.
// Каждый лок КУСАЕТСЯ: value-пин генуинной (e)-ручки падает на любой мутации;
// (c)-инвариантность TINT_HUE_STIFFNESS падает при уходе значения из режима
// пиннинга. Продакшн-константы НЕ тронуты — это #[cfg(test)] измерители.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod wave2_e_locks {
    use super::{NEUTRAL_HUE_DEG, TINT_HUE_STIFFNESS, build_curve_color, cusp_attracted_hue};
    use crate::spaces::oklab::srgb_linear_to_oklab;
    use crate::spaces::srgb::quantise_srgb;

    /// ΔE в Oklab между эмитируемыми (квантованными в 8-бит) цветами.
    fn de_ok(a: [f64; 3], b: [f64; 3]) -> f64 {
        let la = srgb_linear_to_oklab(quantise_srgb(a));
        let lb = srgb_linear_to_oklab(quantise_srgb(b));
        ((la[0] - lb[0]).powi(2) + (la[1] - lb[1]).powi(2) + (la[2] - lb[2]).powi(2)).sqrt()
    }

    /// Светлотная сетка [0.05, 0.95] шаг 0.01 (как в `cusp_window_is_near_measured_gamut_drift`).
    fn lgrid() -> Vec<f64> {
        (5..=95).map(|i| i as f64 / 100.0).collect()
    }

    /// (c) INTERVAL-INSENSITIVE лок для `TINT_HUE_STIFFNESS`. Выше измеренного
    /// порога пиннинга (≈0.36) argmax `cusp_attracted_hue` встаёт РОВНО на
    /// канонический оттенок, и выход байт-инвариантен по всей практической полосе
    /// жёсткости [1, 100]. Дефолт 9.0 сидит глубоко в режиме — точное значение
    /// нематериально. КУСАЕТСЯ: если стиффнес уйдёт ниже режима пиннинга,
    /// `TINT_HUE_STIFFNESS >= 1.0` падает; если механизм перестанет пиннить,
    /// `max_dev == 0` падает.
    #[test]
    fn stiffness_pins_hue_to_canonical_above_threshold() {
        let ls = lgrid();
        // Измеренный порог пиннинга: минимальная жёсткость, при которой ВСЕ
        // светлоты дают оттенок == канонический (drift 0).
        let mut threshold = f64::INFINITY;
        let mut s = 0.0_f64;
        while s <= 5.0 {
            let all_pinned = ls.iter().all(|&l| {
                (cusp_attracted_hue(l, NEUTRAL_HUE_DEG, s) - NEUTRAL_HUE_DEG).abs() < 1e-9
            });
            if all_pinned {
                threshold = s;
                break;
            }
            s += 0.01;
        }
        assert!(
            (0.2..0.6).contains(&threshold),
            "порог пиннинга {threshold:.3} вне замеренного диапазона [0.2, 0.6)"
        );
        // Инвариантность выхода: по всей полосе [1, 100] оттенок отклоняется от
        // канонического на 0.0° — байт-инвариант (это свойство механизма, от
        // значения константы не зависит).
        let mut max_dev = 0.0_f64;
        for &st in &[1.0_f64, 3.0, 5.0, 9.0, 15.0, 30.0, 50.0, 100.0] {
            for &l in &ls {
                max_dev = max_dev
                    .max((cusp_attracted_hue(l, NEUTRAL_HUE_DEG, st) - NEUTRAL_HUE_DEG).abs());
            }
        }
        assert!(
            max_dev < 1e-9,
            "оттенок должен быть байт-инвариантен по [1,100] (max_dev={max_dev:.6}°)"
        );
        // Дефолт сидит в режиме пиннинга с комфортной маржой (устойчивость к float).
        assert!(
            TINT_HUE_STIFFNESS >= 1.0 && TINT_HUE_STIFFNESS > threshold * 2.0,
            "TINT_HUE_STIFFNESS={TINT_HUE_STIFFNESS} должен лежать в режиме пиннинга (> порог {threshold:.3} с маржой)"
        );
        eprintln!(
            "WAVE2 TINT_HUE_STIFFNESS (c): pinning_threshold={threshold:.3} value={TINT_HUE_STIFFNESS} (={:.1}x порога) max_dev[1..100]={max_dev:.6}deg",
            TINT_HUE_STIFFNESS / threshold
        );
    }

    /// (e) DESIGN-CHOICE robustness-лок для `NEUTRAL_HUE_DEG` (измеренный якорь).
    /// Эмитируемый тинт-цвет БАЙТ-ИНВАРИАНТЕН (ΔE_ok = 0) по всему измеренному
    /// разбросу семейства нейтралей [285.78°, 286.01°]; даже грубая ошибка ±20°
    /// даёт лишь ~1 JND (оттенок материален, но при малой хроме тинта эффект мал).
    /// КУСАЕТСЯ: value-пин `== 286.0` падает на любой мутации.
    #[test]
    fn neutral_hue_emits_byte_invariant_across_measured_family_spread() {
        assert_eq!(NEUTRAL_HUE_DEG, 286.0, "измеренный якорь нейтрали");
        // Explicit test-client policy used only to make hue sensitivity visible.
        let ratio = 0.10;
        let ls = [0.15_f64, 0.31, 0.48, 0.58, 0.68, 0.78, 0.86];
        let mut max_spread = 0.0_f64;
        let mut max_wide = 0.0_f64;
        for &l in &ls {
            let base = build_curve_color(l, NEUTRAL_HUE_DEG, ratio);
            for h in [285.78_f64, 285.90, 286.01] {
                max_spread = max_spread.max(de_ok(base, build_curve_color(l, h, ratio)));
            }
            for h in [266.0_f64, 276.0, 296.0, 306.0] {
                max_wide = max_wide.max(de_ok(base, build_curve_color(l, h, ratio)));
            }
        }
        assert!(
            max_spread < 1e-9,
            "тинт-цвет обязан быть байт-инвариантен по измеренному разбросу (ΔE={max_spread:.6})"
        );
        assert!(
            (0.005..0.03).contains(&max_wide),
            "широкополосная (±20°) чувствительность {max_wide:.4} вне замеренного [0.005, 0.03)"
        );
        eprintln!(
            "WAVE2 NEUTRAL_HUE_DEG (e): ΔE_ok[measured spread]={max_spread:.6} ΔE_ok[±20°]={max_wide:.4}"
        );
    }
}
