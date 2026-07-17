//! Semantic role table: a named contrast contract resolved from any background
//! in one [`solve`] call.
//!
//! Where [`solve`](crate::solve()) answers "what colour meets *this* signed
//! contrast against *this* background", this module answers the product-level
//! question one layer up: "give me the whole set of named colours a UI needs
//! against this background". A `Role` is a stable string key plus a recipe for
//! a [`Contract`]; `RoleTable` is the default recipe set, overridable per role;
//! `resolve` solves one role and `resolve_set` solves the whole table in a
//! single sweep. Serialising the result to CSS custom properties is the
//! runtime-engine chapter's job — this module returns a structured
//! `role → Solved` map and nothing else.
//!
//! # Polarity is read from the background, never from the role
//!
//! [`solve`] takes a *signed* `Lc` (positive = dark-on-light, negative =
//! light-on-dark). A role stores only the *magnitude* of the contrast it wants;
//! this module picks the sign from the background, so the same role table
//! resolves correctly on a light or a dark background without the caller
//! choosing a theme. That is what "resolved from any background" means.
//!
//! The sign is chosen in two stages, and — crucially — from the *WCAG* gate the
//! text roles actually have to clear, not from the perceptual maximum:
//!
//! 1. **WCAG reachability first.** A text role floors at the legal AA ratio
//!    (4.5:1 for text). Which polarity can reach that floor is a property of the
//!    background alone — `contrast_ratio(black, bg)` vs `contrast_ratio(white,
//!    bg)` — and is independent of the viewing conditions, because the WCAG
//!    formula is. So the polarity that clears the strict 4.5:1 floor wins. This
//!    is what stops a light-grey background (`#808080`, `#999999`) from reporting
//!    every text role unreachable while *black* text on it passes AA with room to
//!    spare: the old "pick the larger LPC maximum" rule flipped polarity near
//!    `#999999`, far from the WCAG flip near `#747474`, and chose the side the
//!    legal floor could not reach.
//! 2. **Tie-break to the perceptual winner (white).** When both polarities clear
//!    the strict floor they do so only on a narrow band: black clears 4.5:1 at
//!    `Y ≥ 0.175`, white at `Y ≤ 0.1833`, so both are legal only on
//!    `Y ∈ [0.175, 0.1833]` (`#757575`, `#767676` and same-luminance chromatics
//!    such as `#0078D4`). Across that whole band the perceptual layer prefers
//!    *light-on-dark* with a wide margin — the luminance-domain LPC core
//!    (`crate::lpc::contrast_core`) has its black-overtakes-white crossover far
//!    higher, near `Y ≈ 0.342` (measured, locked by
//!    `pair::exposure_locks::pair_crossover_equals_measured_core_polarity_flip`)
//!    — so the tie resolves to white (`break_tie`). This
//!    replaces the former "larger WCAG margin wins" rule, whose symmetric margin
//!    crossed over *inside* the band (`Y ≈ 0.1791`) and chose dark-on-light on the
//!    upper half — the perceptually weaker side there, and the one that made
//!    `#0078D4` emit black against the Fluent convention of white. When *neither*
//!    polarity can clear the floor (a true mid-grey with no readable side), the
//!    side that comes *closest* is chosen, so the [`SolveFailure`] a role surfaces
//!    carries the honest best-case `max_ratio`, not a worse one.
//!
//! Because the criterion is VC-independent, a role's polarity never flips between
//! the light and dim viewing conditions for the same background — no per-theme
//! coin-flip on a near-tie like `#3478F6`.
//!
//! # Sanity over arithmetic: the anchor principle
//!
//! Text contrast magnitudes are **not fixed deltas**. A fixed delta is how
//! `label-primary` once came out grey: a mid contrast number satisfies the
//! contract arithmetically but violates the design intent that primary text on
//! white reads as *black*. Instead, a text role anchors its target to a
//! **fraction of the maximum contrast the background can supply**
//! ([`TextAnchor`]). Primary asks for ~97 % of that maximum — almost the
//! strongest the background allows — so on white it lands near-black and on
//! black near-white, by construction, on *any* background. The fractions are
//! calibrated against Daniel's Figma anchors (see `RoleTable::default`) and
//! stay marked "calibrates" until his eye signs off.
//!
//! Because every text role is a fraction of the *same* per-background maximum,
//! the hierarchy primary > secondary > muted > disabled is **strict wherever the
//! background physically allows it** — symmetric by construction across both
//! polarities. This is the deliberate fix for the asymmetry baked into the
//! literal Figma tokens, where equal opacity steps produced a dark-theme
//! hierarchy ~40 % weaker than the light one (see the module tests).
//!
//! # Hierarchy compression is flagged, never silent
//!
//! On a background whose readable window is *narrower than the hierarchy's own
//! steps* — a near-AA mid-grey such as `#747474`, where the only readable
//! polarity has barely any room above 4.5:1 — two adjacent text roles can be
//! forced by the legal floor onto the same point. The old code let primary and
//! secondary collapse to an identical hex silently, falsifying the "strict
//! hierarchy by construction" claim. This module instead degrades *honestly*:
//! the order is kept non-strict (primary ≥ secondary ≥ muted ≥ disabled), a
//! subordinate role is nudged to the smallest distinguishable quantisation step
//! below its senior **only while it still clears its own floor**, and any role
//! whose target was lifted by the floor into this squeeze is marked
//! [`Resolved::compressed`]. A consumer can read the flag and know the hierarchy
//! is compressed here, rather than discovering two roles share a colour.
//!
//! # The zero token
//!
//! "Empty" is a value, not a missing entry. A role that means "no colour here"
//! (`Role::None`) is part of the table and resolves to an explicit
//! [`Resolved::None`] — an honest zero (transparent / no contrast), never a
//! skipped key. Swapping a literal for a token later is then a change of value,
//! not the insertion of a token where a hole used to be.
//!
//! # Out of scope for v1 (extension seams, not implementations)
//!
//! - **Decorative contracts split by physics.** The `border-base` / `border-soft`
//!   ladder and the `fill-primary..quaternary` ladder are *dJ'* roles
//!   ([`RoleSpec::DecorativeDj`]): each holds the owner's literal perceived-
//!   lightness step (a `J'` offset) against its surface, per theme, solved
//!   analytically with no readability floor. The `shadow-minor..major` stack and
//!   `separator` stay legacy Lc [`RoleSpec::Decorative`] placeholders (the shadow
//!   owner anchors are alpha opacities, not dJ' steps); only their relative order
//!   is a contract, defined and covered in the `surface-jnd` chapter.
//!   `border-strong` carries the `label-primary` FRACTION but a 3:1 non-text
//!   floor (WCAG 1.4.11): a border must be distinguishable, not readable.
//! - **Brand / sentiment roles are not here.** v1 carries one *neutral*
//!   undertone for the whole table (the cool tint of Daniel's neutral ladder,
//!   see [`RoleChroma`]); per-role brand/accent hues are a later chapter. The
//!   chroma seam (`RoleTable::with_chroma`) is left open so that chapter can
//!   swap the policy over the existing sentiment machinery without reshaping
//!   this table.
//!
//! # The neutral undertone: identity, not sterile grey
//!
//! Daniel's neutral is tinted — `#101012` carries a cool blue-violet undertone,
//! not a pure grey. A role table resolved with zero chroma threw that identity
//! away: `label-primary` on white came out the sterile `#141414`. So every
//! resolved role carries the neutral's undertone and lands as a *relative* of the
//! neutral family — `label-primary` on white as a cool near-black in the `#101012`
//! family. The undertone is small enough that the WCAG floors, the strict
//! hierarchy, and the near-black/near-white primary all hold exactly as before
//! (the solver re-solves lightness to the same target with the tint applied).
//!
//! The default undertone policy is [`RoleChroma::Curve`] (v2), derived from three
//! computable mechanisms rather than a flat ratio of the gamut:
//!
//! 1. **Constant CAM16-UCS coordinate** — the chroma at each role's resolved
//!    lightness is solved to a constant `M'` (`TINT_TARGET_MP`), not a fixed
//!    fraction of the gamut maximum. This is a characterized design policy that
//!    holds chroma in the lights and moderates it in the middle; it does not turn
//!    `M'` into a universal perceptual-colorfulness scale.
//! 2. **Cusp-attracted hue** — the hue at each lightness is pulled toward the
//!    local chroma cusp of the sRGB gamut, penalised for leaving the canonical
//!    286° (`cusp_attracted_hue`). The drift emerges from geometry; it is *not*
//!    a set of hard-coded hue nodes. (Honest limit: the gamut's cusp near 286°
//!    does not drift to the reference's light-end azure — see that function.)
//! 3. **Perceptibility floor** — where the gamut cannot host the target
//!    colorfulness, the curve takes the gamut maximum and is allowed to fall
//!    toward `TINT_PERCEPTIBLE_MP_FLOOR` rather than fake chroma it cannot reach.
//!
//! A caller who wants the v1 flat-ratio undertone opts back into it with
//! [`RoleChroma::flat_neutral_tint`]; pure grey with [`RoleChroma::Neutral`];
//! either via `RoleTable::with_chroma`.

use crate::ladder::LadderTint;
use crate::scale;
use crate::solve::{self, BgInput, ChromaPolicy, Contract, Floor, Hue, SolveFailure, Solved};
use crate::spaces::srgb::srgb_gamma;
use crate::spaces::vc::ViewingConditions;
use crate::wcag;

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
const DECORATIVE_FLOOR_MIN: f64 = 7.5;

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
// Rust 1.85 не считает использованием обращение только из const-assert и тестов;
// это разложение provenance, а не отдельная production-политика.
#[allow(dead_code)]
// SSOT-TRACKED — квант-guard декоративного пола (issue #44), терминал (c) interval-insensitive, см. docs/empirical-inventory.md.
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
// reference/labui-figma-structure.md). Это правильная единица измерения (шаг J',
// а не контраст Lc) и правильный тип ([`RoleSpec::DecorativeDj`], решается без
// порога читаемости и без обрезки низкого контраста — это различимость, а не
// разборчивость текста).
//
// Числа, единица измерения и источник — реальные измерения из Figma.
//
// По темам: якоря измерены отдельно под каждой темой, потому что восприятие
// светлоты зависит от окружения (surround). Тёмные якоря примерно в 2.2 раза
// больше светлых — измеренная компенсация для тёмного окружения — поэтому
// они НЕ выведены из светлого набора, а являются отдельными измерениями под
// тёмную тему, которые выбираются при резолве по теме viewing conditions.

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
/// перцептивная калибровка теневого стека — за владельцем.
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
// калибровка долей — за владельцем.

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
/// (7.5); финальная JND-калибровка — за владельцем.
#[cfg(test)]
// SSOT-TRACKED — провизорная декоративная величина Separator (Lc), см. docs/empirical-inventory.md.
const SEPARATOR_DECORATIVE_LC: f64 = 8.0;

/// The strict WCAG 2.1 AA *text* ratio (4.5:1) — the tightest legal gate any
/// role in the table imposes, and therefore the one polarity is chosen against.
/// Selecting against the strictest floor keeps a single polarity for the whole
/// set: a side that clears 4.5:1 trivially clears the laxer 3:1 UI floor too.
const POLARITY_FLOOR_RATIO: f64 = wcag::AA_TEXT_RATIO;

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

/// How a text/UI role expresses its target contrast against a background.
///
/// A fraction of the background's maximum achievable contrast — *not* a fixed
/// `Lc` delta. See the module docs on the anchor principle for why. `fraction`
/// is in `(0, 1]`: `1.0` names the physical contrast endpoint exactly.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextAnchor {
    fraction: f64,
    conformance: Floor,
    /// Опциональный источник ОТТЕНКА семьи (ратификация ch5c, M1). `None` —
    /// нейтральный лейбл (подтон из [`RoleChroma`] таблицы, прежний путь
    /// байт-в-байт). `Some(tint)` — ЦВЕТНОЙ лейбл: держит ТОТ ЖЕ Lc-контракт
    /// уровня, что нейтральный (доля·max, тот же WCAG-пол — одноуровневость по
    /// построению), но решённый в чистом оттенке семьи. Тинт — пер-темный
    /// `Copy`-якорь идентичности (Figma-оттенок сохранён); светлота выводится
    /// контрактом на LCS-кривой семьи, хрома = стена гамута на решённой
    /// светлоте. Резолв — [`resolve_hued_anchor`].
    hue: Option<crate::ladder::LadderTint>,
}

impl TextAnchor {
    /// A text anchor at `fraction` of the background's maximum contrast, with the
    /// given WCAG conformance floor. `fraction` must be finite and inside
    /// `(0, 1]`; invalid input is rejected rather than silently rewritten.
    /// Neutral undertone (no family hue); attach one with
    /// [`with_hue`](Self::with_hue).
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

    /// Тот же якорь, но решаемый в чистом оттенке `hue`-семьи (M1 ch5c). Контракт
    /// уровня (`fraction`/`conformance`) НЕ меняется — меняется лишь физика цвета:
    /// одноуровневость держится, оттенок = идентичность семьи.
    pub fn with_hue(mut self, hue: crate::ladder::LadderTint) -> Self {
        self.hue = Some(hue);
        self
    }

    /// The fraction of maximum contrast this anchor targets, in `(0, 1]`.
    pub fn fraction(self) -> f64 {
        self.fraction
    }

    /// The WCAG conformance floor applied after the perceptual target.
    pub fn conformance(self) -> Floor {
        self.conformance
    }

    /// Источник оттенка семьи, если это цветной лейбл (M1). `None` — нейтральный.
    pub fn hue(self) -> Option<crate::ladder::LadderTint> {
        self.hue
    }
}

/// A decorative perceived-lightness-difference (dJ') magnitude, with the owner's
/// per-theme calibration.
///
/// The owner measured the perceived-lightness step a decorative element should
/// hold against its surface separately for each theme — perception of a lightness
/// difference is not theme-invariant, and the owner's dark anchors run ~2.2× the
/// light ones (a measured over-compensation for dark surrounds). So the magnitude
/// is a `(light, dark)` pair, and the solver picks the side that matches the
/// viewing conditions it resolves under via [`for_vc`](DjMagnitude::for_vc). This
/// keeps the role table a single VC-agnostic instance (it serves both themes) and
/// localises the per-theme choice to the one place that knows the VC — the resolve
/// step — rather than forcing two tables or a theme parameter through every caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DjMagnitude {
    light: f64,
    dark: f64,
}

impl DjMagnitude {
    /// A dJ' magnitude from its per-theme anchors (light surround, dark surround).
    pub const fn new(light: f64, dark: f64) -> Self {
        Self { light, dark }
    }

    /// The anchor for these viewing conditions: the dark value under a dimmed
    /// surround (dark theme), the light value otherwise.
    pub fn for_vc(self, vc: &ViewingConditions) -> f64 {
        if vc.is_dark_theme() {
            self.dark
        } else {
            self.light
        }
    }

    /// The light-surround anchor.
    pub fn light(self) -> f64 {
        self.light
    }

    /// The dark-surround anchor.
    pub fn dark(self) -> f64 {
        self.dark
    }
}

/// The contrast recipe behind a role — the shape this module solves.
///
/// Text/UI roles ([`Anchor`](RoleSpec::Anchor)) target a fraction of the
/// background's maximum; dJ' decorative roles
/// ([`DecorativeDj`](RoleSpec::DecorativeDj)) target a perceived-lightness step on
/// the CAM16-UCS J' axis with no readability floor; legacy Lc decorative roles
/// ([`Decorative`](RoleSpec::Decorative)) target an `Lc` magnitude held only for
/// the stack's relative ordering (the shadow anchors are alpha opacities, not
/// dJ' steps — see the shadow-stack note above); the
/// zero token ([`Zero`](RoleSpec::Zero)) resolves to nothing. Construct these
/// through `RoleTable`; they are exposed so a caller can read or override a recipe.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RoleSpec {
    /// Anchored text/UI contrast: a fraction of the background's maximum.
    Anchor(TextAnchor),
    /// Decorative perceived-lightness difference (dJ'): the solved colour sits
    /// `magnitude_dj` away from the background on the CAM16-UCS lightness (`J'`)
    /// axis, toward the larger headroom (the set polarity). No readability floor
    /// and no low-contrast clip — this is distinguishability of a decorative
    /// element (a fill tint, a hairline border), a different physics from the
    /// legibility the [`Anchor`](RoleSpec::Anchor) / [`Decorative`](RoleSpec::Decorative) roles solve.
    ///
    /// The magnitude carries the owner's literal Figma-computed anchors per theme
    /// (see [`DjMagnitude`]); the solve is analytic (J' offset → grey-axis Oklab L
    /// → undertone build → quantise → honest dJ' measurement on the emitted hex).
    /// The unit, type, and source of the anchor are the owner's — not a
    /// substitute.
    DecorativeDj { magnitude_dj: DjMagnitude },
    /// Decorative just-noticeable-difference contrast: an `Lc` magnitude, held
    /// above `DECORATIVE_FLOOR_MIN`, with [`Floor::None`].
    ///
    /// Retained for the shadow stack, whose owner anchors are alpha opacities,
    /// not dJ' steps — converting them to dJ' would invent numbers with no owner
    /// source. The relative order between the shadow steps is the contract this
    /// variant carries; `surface-jnd` derives shadow contracts from the alphas.
    Decorative { magnitude: f64 },
    /// Ступень лестницы акцента/сентимента/бренда/нейтрали: тинт-якорь источника
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
        /// Typed execution mode compiled invocation (#292). Прежние
        /// config/wire-ключи `stable-v1 | legacy-platform-dependent-v1` —
        /// migration adapter на границе, не core-семантика.
        mode: crate::numerical_plan::NumericalExecutionModeV1,
    },
    /// Заливка пары ([`crate::pair`]): якорь источника, сдвинутый до победы
    /// перцептивной стороны лейбла в штатной полярности; солид-эмиссия.
    PairFill {
        /// Пер-темный кодированный якорь источника.
        tint: LadderTint,
    },
    /// Лейбл ТИНТ-бейджа ([`crate::pair`], лейбл-сторона). Семейно-оттеночный
    /// лейбл, чей WCAG-пол энфорсится ПРОТИВ объявленной тинт-поверхности
    /// (exact source-over композит declared `tint` при compatibility-альфе
    /// позиции `fill-*-primary` над фоном резолва), а НЕ против фона страницы
    /// и НЕ против эмитированного [`PairFill`](Self::PairFill) — у того своя,
    /// отдельно сдвинутая солид-эмиссия; ребра `PairFill → PairLabel` не
    /// существует. Резолв использует один generic-компонент appearance-графа:
    /// скомпилированный граф собирает поверхность и возвращает физические факты
    /// foreground occurrence против неё. Доказательный статус последующего
    /// резолвера граф не назначает. Дифференциальный тест закрепляет
    /// эквивалентность миграционного подключения и результатов на проверяемом
    /// домене, но не является независимым эталоном математики самого резолвера.
    /// Тон клампится (флаг `compressed`) при недостижимости на кривой семьи.
    PairLabel {
        /// Пер-темный кодированный тинт-якорь семьи (как у лестницы).
        tint: LadderTint,
        /// Доля максимума контраста тинт-поверхности `(0, 1]`.
        fraction: f64,
        /// WCAG-пол против тинт-поверхности.
        floor: Floor,
        /// Альфа поверхности (светлая тема) — из позиции `fill-*-primary`.
        surface_alpha_light: f64,
        /// Альфа поверхности (тёмная тема) — из позиции `fill-*-primary`.
        surface_alpha_dark: f64,
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
        /// `None` — прежний путь (тинт эмитится как есть). `Some(floor)` —
        /// если композит чист (≥ пола), эмитится точный семейный солид (Figma
        /// цел); иначе МИНИМАЛЬНЫЙ ЛЕГАЛЬНЫЙ СДВИГ по кривой семьи до легальности,
        /// объявленный флагом [`TranslucentResolved::floor_coerced`]. Применим
        /// только к солиду (α=1); у полупрозрачных позиций игнорируется (контраст
        /// полупрозрачной роли — свойство композита, не тинта).
        floor: Option<Floor>,
    },
    /// Альфа-аналог солида источника через композит-инверсию ([`crate::alpha`],
    /// #119): для солид-цвета `of` (по теме) на фоне резолва подбирается
    /// `(tint, α)`, чей композит равен солиду. Отличается от [`Ladder`](Self::Ladder)
    /// тем, что здесь солид-цель ФИКСИРОВАНА (тинт выводится инверсией), а не
    /// тинт-якорь эмитится напрямую. Даёт `-tinted`-роли labui (fill-*-tinted):
    /// заливка, чей композит на подложке = соответствующий солид.
    ///
    /// Фактическая α возвращается явно ([`crate::alpha::AlphaAnalog::alpha`]): при
    /// неразрешимой запрошенной α поднимается до `α_min` (композит остаётся точно
    /// равным солиду — двигается прозрачность, не цвет; кламп тинта запрещён).
    AlphaAnalog {
        /// Пер-темный кодированный солид-источник, чей альфа-аналог берётся.
        of: LadderTint,
        /// Запрошенная альфа: `(0, 1]` — контракт РОЛИ (тот же предел, что у
        /// конфиг-валидатора: α = 0 — невидимая роль, отказ честнее выдумки).
        /// Уже: библиотечный [`crate::alpha::resolve_alpha_analog`] принимает
        /// и `0.0` (вырожденный ответ tint=фон) — то его домен, не ролевой.
        /// Поднимается до `α_min`, если запрошенная ниже разрешимой.
        alpha: f64,
    },
    /// Двухслойный материал (стекло/акрил): опаковая тон-база `02` на целевом
    /// |ΔJ'| тира + полупрозрачный тинт `01` (тот же тон) с ВЫВЕДЕННОЙ альфой.
    /// Резолв — [`Resolved::Material`] ([`crate::material`]; канон — `docs/whitepaper.md` §3.7).
    ///
    /// Тон строится dj-anchor-солвером на светлоте `tone` от фона резолва в
    /// оттенке семьи; α выбирается охарактеризованным для платформы поиском
    /// с фиксированным числом шагов и повторно проверяется как проходящее
    /// состояние для `floor`.
    Material {
        /// Оттенок семьи тона. `None` — нейтральный материал (подтон таблицы, то
        /// же 286°, что и остальные нейтральные эмиссии). `Some(tint)` —
        /// семейно-оттеночный (акцент/сентимент): оттенок подставляется в кривую
        /// подтона таблицы, красочность (`target_mp`/`hue_stiffness`) — от неё же.
        hue: Option<LadderTint>,
        /// Целевой |ΔJ'| тона-базы от фона резолва (пер-темная пара): тир
        /// материала (base = крупный/заметный, subtle = малый/тонкий).
        tone: DjMagnitude,
        /// WCAG-пол читаемости, который держит выведенная α (`AaText`/`AaUi`).
        /// `Floor::None` невалиден — материал обязан нести пол (валидатор ловит).
        floor: Floor,
    },
    /// The zero token: resolves to [`Resolved::None`].
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
// SSOT-TRACKED — измеренный Oklab-оттенок нейтральной шкалы, терминал (e) design-choice (измеренный якорь, байт-инвариант по разбросу [285.78,286.01]), см. docs/empirical-inventory.md.
pub(crate) const NEUTRAL_HUE_DEG: f64 = 286.0;

/// Доля от максимальной хромы в гамуте, которую несёт тонированная роль.
///
/// Намеренно небольшая: подтон должен *ощущаться*, но никогда не *считываться*
/// как цвет. Решатель применяет абсолютную хрому `ratio · max_chroma(L)`
/// ([`build_color`](crate::solve)), а `max_chroma` достигает пика на средней
/// светлоте и падает почти до нуля на обоих краях (тёмном и светлом). Поэтому
/// один плоский коэффициент бесплатно воспроизводит дух огибающей нейтральной
/// кривой: самый сильный подтон приходится на роли средней силы, самый слабый —
/// на почти-чёрный/почти-белый края текстовой шкалы — "меньше у тёмных/светлых
/// краёв, больше к середине".
/// `0.10`: на белом `label-primary` резолвится в холодный почти-чёрный
/// семейства `#101012`, а не в чистый серый.
///
/// Терминал **(e) DESIGN-CHOICE** — генуинная свободная ручка «силы подтона».
/// Blast radius: используется ТОЛЬКО опциональной v1-политикой
/// [`RoleChroma::flat_neutral_tint`] (не дефолтная — дефолт `Curve` держит хрому
/// через `TINT_TARGET_MP`), т.е. в проде «спит», пока потребитель не вернётся к
/// плоскому тинту. Легальный диапазон конфига **[0, 1]** (валидатор `TINT_RATIO`
/// в `config.rs`). Sensitivity (Волна 2, лок
/// `neutral_tint_ratio_sensitivity_is_bounded`): свип легальной полосы
/// [0, 0.20] по светлотам шкалы даёт max ΔE_ok ≈ **0.0288** (>1 JND) —
/// НЕПРЕРЫВНЫЙ материальный дрейф (ratio прямо масштабирует хрому
/// `ratio · max_chroma(L)`), значит честный (e), не (c). Протокол калибровки:
/// поднимать ratio, пока подтон не начнёт «считываться как цвет» (перцептивный
/// потолок, замер по [`TINT_PERCEPTIBLE_MP_FLOOR`]) — тогда 0.10 = чуть ниже
/// этого потолка на серединных ролях.
// SSOT-TRACKED — коэффициент хромы нейтрального подтона, терминал (e) design-choice (opt-in v1-политика; max ΔE_ok 0.0288 по [0,0.2]), см. docs/empirical-inventory.md.
pub(crate) const NEUTRAL_TINT_RATIO: f64 = 0.10;

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
// `#[cfg(test)]` снова: с уходом фикстуры в `config::fixture` (тест-оракул)
// прод-потребителей не осталось — словарь пресета (BL-007) несёт только
// семантику ролей, а прод-репро `tint_target_sweep_repro` принимает цель
// параметром.
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
// SSOT-TRACKED — жёсткость прижатия оттенка к каспу, терминал (c) interval-insensitive (выход байт-инвариантен выше порога пиннинга ≈0.36, дефолт 9.0 = 25× порога), см. docs/empirical-inventory.md.
pub(crate) const TINT_HUE_STIFFNESS: f64 = 9.0;

/// Порог воспринимаемости (механизм 3) в единицах CAM16-UCS `M'`. Ниже
/// примерно этой красочности подтон попадает в "мёртвую серую зону" —
/// заметно неразличимую как цвет. Там, где гамут не может обеспечить
/// `TINT_TARGET_MP`, кривая не гонится за ним через стену гамута: она
/// берёт максимум, который даёт гамут, и честно допускает падение к этому
/// порогу на самых краях (почти-чёрный / почти-белый), где даже собственный
/// `M'` референса падает до ~2.3–3.0.
///
/// Терминал **(c) INTERVAL-INSENSITIVE**: порог сидит вплотную ПОД измеренным
/// потолком ахроматического `M'`-шума серых (максимум ≈1.53 у белого,
/// `tint_floor_tracks_achromatic_mp_noise_ceiling`), а доля sRGB-гаммы, где
/// точное значение решает «ощущаемый тон / мёртвая серость», — **0.07%**
/// (`exposure_tint_perceptible_mp_floor`). Ниже порога классификация выхода
/// провизорно неизменна по всему практическому интервалу — сильнее «выбор
/// дизайна», ре-аудит `science/reclassify-e-buckets` 2026-07-07, реестр
/// docs/empirical-inventory.md.
// SSOT-TRACKED — порог воспринимаемости в CAM16-UCS M', терминал (c) interval-insensitive (exposure 0.07%), см. docs/empirical-inventory.md.
pub(crate) const TINT_PERCEPTIBLE_MP_FLOOR: f64 = 1.5;

/// Half-width (degrees) of the hue window the cusp search explores around the
/// canonical hue. The undertone may drift inside a blue-violet band; it may not
/// wander into unrelated quadrants (red, cyan), so the search is bounded.
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

/// The chroma policy a role table carries.
///
/// The v1 default was [`Tinted`](RoleChroma::Tinted) (a flat ratio of the gamut
/// maximum); the v2 default is [`Curve`](RoleChroma::Curve), the science-derived
/// undertone (constant perceptual colorfulness + cusp-attracted hue + a
/// perceptibility floor). [`Neutral`](RoleChroma::Neutral) is the achromatic
/// override. A caller replaces the table's chroma wholesale via
/// `RoleTable::with_chroma`; the enum is the seam later chapters extend for
/// brand/sentiment-tinted roles without reshaping this type.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum RoleChroma {
    /// Achromatic (grey): zero chroma, hue ignored. The explicit override that
    /// reproduces the pre-tint behaviour.
    Neutral,
    /// A small undertone at a fixed Oklab `hue_deg`, carried as `ratio` of the
    /// in-gamut maximum chroma at each role's resolved lightness. The flat-ratio
    /// v1 policy: kept as an opt-in because its envelope follows `max_chroma(L)`,
    /// which over-saturates the middle and starves the light end relative to the
    /// reference. Prefer [`Curve`](RoleChroma::Curve).
    Tinted { hue_deg: f64, ratio: f64 },
    /// v2-подтон, выведенный из трёх вычислимых механизмов, а не из
    /// захардкоженных узлов рампы:
    ///
    /// 1. **Постоянная перцептивная красочность** — хрома на резолвленной
    ///    светлоте каждой роли решается так, чтобы цвет нёс `target_mp`
    ///    CAM16-UCS `M'` (а не фиксированную долю гамута). Именно
    ///    равномерность UCS позволяет одной константе держать хрому в светлых
    ///    и умерять её в середине. См. `TINT_TARGET_MP`.
    /// 2. **Оттенок, притянутый к каспу** — оттенок на каждой светлоте
    ///    притягивается к локальному каспу хромы гамута sRGB (вычисляется из
    ///    `max_chroma(L, h)`), со штрафом `hue_stiffness` за отклонение от
    ///    `canonical_hue_deg`. См. `cusp_attracted_hue`.
    /// 3. **Порог воспринимаемости** — там, где гамут не может обеспечить
    ///    `target_mp`, кривая берёт максимум гамута и на краях честно
    ///    допускает падение к `TINT_PERCEPTIBLE_MP_FLOOR`, а не подделывает
    ///    хрому.
    ///
    /// `target_mp` ("сила") и `hue_stiffness` ("удержание оттенка") — два
    /// **выбранных** скаляра, единственные свободные ручки политики.
    /// Остальное в кривой опирается на три **измеренные / геометрические**
    /// константы, а не на свободные параметры: оттенок тёмного якоря
    /// `canonical_hue_deg` (286°, измерен по нейтральной шкале), порог
    /// воспринимаемости (`TINT_PERCEPTIBLE_MP_FLOOR`, 1.5 `M'`) и окно
    /// поиска каспа по оттенку (`CUSP_HALF_WINDOW_DEG`, ±40°). То есть
    /// политика — это "два выбранных скаляра + три измеренные/геометрические
    /// константы", а не "два скаляра" — всё за пределами двух выбранных ручек
    /// является фиксированной геометрией.
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
                if !hue_deg.is_finite() {
                    return Err(SolveFailure::InvalidInput(format!(
                        "undertone hue must be finite, got {hue_deg}"
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
                if !canonical_hue_deg.is_finite() {
                    return Err(SolveFailure::InvalidInput(format!(
                        "curve canonical hue must be finite, got {canonical_hue_deg}"
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

    /// The v1 flat-ratio neutral undertone, kept as an explicit opt-in.
    ///
    /// The default table moved to [`Curve`](RoleChroma::Curve); a caller who
    /// prefers the older flat-ratio behaviour (the neutral's cool hue at a fixed
    /// fraction of the gamut maximum, `NEUTRAL_TINT_RATIO`) opts back into it
    /// with `RoleTable::default().with_chroma(RoleChroma::flat_neutral_tint())`.
    /// This is the additive seam the task requires: the v1 policy stays a
    /// first-class, named choice even though it is no longer the default.
    pub fn flat_neutral_tint() -> Self {
        RoleChroma::Tinted {
            hue_deg: NEUTRAL_HUE_DEG,
            ratio: NEUTRAL_TINT_RATIO,
        }
    }

    /// The v2 default: the science-derived undertone curve at its calibrated
    /// scalars.
    #[cfg(test)]
    fn neutral_curve() -> Self {
        RoleChroma::Curve {
            canonical_hue_deg: NEUTRAL_HUE_DEG,
            target_mp: TINT_TARGET_MP,
            hue_stiffness: TINT_HUE_STIFFNESS,
        }
    }

    /// Plan the solver's `(hue, chroma)` inputs for a role whose contrast-solved
    /// Oklab lightness is `l_ok`.
    ///
    /// For the lightness-independent policies ([`Neutral`](RoleChroma::Neutral),
    /// [`Tinted`](RoleChroma::Tinted)) the plan ignores `l_ok` and reproduces the
    /// v1 behaviour exactly. For [`Curve`](RoleChroma::Curve) the hue is the
    /// cusp-attracted hue at `l_ok` and the chroma ratio is the one that lands the
    /// colour on the target perceptual colorfulness at that lightness and hue —
    /// the per-lightness derivation that makes the undertone a curve, not a
    /// constant.
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
                // The Curve plan is a pure function of `(l_ok, policy scalars, vc)`:
                // an 81-step cusp-hue scan plus a CAM16 ratio bisection. Within one
                // resolve sweep the same lightness recurs across roles and across a
                // role's fixed-point refinements, so a sweep-scoped exact-key memo
                // returns the byte-identical `(hue, ratio)` without redoing either
                // scan. See [`curve_plan_cached`].
                curve_plan_cached(l_ok, canonical_hue_deg, target_mp, hue_stiffness, vc)
            }
        }
    }

    /// A lightness-independent plan for the achromatic probe pass (pass A), used
    /// only to discover a role's contrast-solved lightness before the real
    /// per-lightness plan is built. Always achromatic so the probe is fast and
    /// the discovered lightness is the role's true contrast lightness.
    fn probe_plan() -> (Hue, ChromaPolicy) {
        (Hue::deg(0.0), ChromaPolicy::Neutral)
    }
}

thread_local! {
    /// Process-lived memo for the [`RoleChroma::Curve`] plan, keyed on the bit
    /// patterns of `(l_ok, canonical, target_mp, stiffness, vc)` so a hit returns
    /// the byte-identical `(hue, ratio)` the 81-step cusp scan + CAM16 ratio
    /// bisection would. The plan is a deterministic function of the key, so the
    /// cache is always correct — a repeat of any of these means re-resolving the
    /// same theme (the common case: a tool re-resolving as a background is tweaked,
    /// or the same neutral resolved against many surfaces), where the cusp scan is
    /// pure recomputation. Bounded by [`CURVE_PLAN_CACHE_CAP`]: the bisected `l_ok`
    /// is effectively arbitrary across unrelated backgrounds, so without a cap the
    /// map could grow without bound — at the cap it is cleared wholesale (a cold
    /// rebuild, never incorrectness).
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
/// (CAM16-UCS `M'`) — mechanism 1, with the mechanism-3 floor at the gamut wall.
///
/// `M'` rises monotonically with chroma at fixed lightness and hue, so the ratio
/// is found by bisection: build the colour at a trial ratio, measure its `M'`
/// through the same CAM16-UCS path the engine uses ([`LcsColor::mp`]), and
/// narrow. If even `ratio = 1` (the gamut maximum) cannot reach `target_mp`, the
/// gamut is the limit — return `1.0` and let the colourfulness sit at the most
/// the gamut allows (honestly below target, toward
/// `TINT_PERCEPTIBLE_MP_FLOOR` at the pinched extremes) rather than fake it.
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

/// Plateau reference node for the tint sweep: `(oklab_l, reference_mp)`.
type PlateauNode = (f64, f64);
/// One tint-sweep row: `(candidate_target, rms_residual, max_residual)`.
type SweepRow = (f64, f64, f64);

/// Reproduction hook for `examples/tint_target_sweep.rs` — **not** stable public
/// API (`#[doc(hidden)]`). Exposes the real-engine tint identity-curve `M'` sweep
/// behind `TINT_TARGET_MP` so its provenance is reproducible from outside
/// `#[cfg(test)]` without duplicating (and drifting from) the engine. The realised
/// curve `M'` is computed by the exact path the `#[cfg(test)]`
/// `curve_fits_reference_plateau_colorfulness` metric uses (`cusp_attracted_hue` →
/// `ratio_for_target_mp` → gamut-clamped build → CAM16-UCS `M'`), so a caller's
/// printed numbers cannot drift from the test.
///
/// Returns `(plateau_nodes, sweep)`:
/// * `plateau_nodes`: `(oklab_l, reference_mp)` for the reference-ramp nodes whose
///   Oklab lightness lies in `[l_min, l_max]` (the colourfulness plateau).
/// * `sweep`: `(candidate_target, rms_residual, max_residual)` — over the plateau
///   of `|realised_curve_mp(l, target) − reference_mp|` (the `M'` of the
///   **gamut-clamped** curve built to `target`, not the raw target): `rms` is the
///   RMS (the metric `TINT_TARGET_MP` minimises), `max` is the largest per-node
///   residual (the quality figure the in-code test bounds at ≤ 1.0).
///
/// Value-preserving: reads the engine, changes nothing.
#[doc(hidden)]
pub fn tint_target_sweep_repro(
    targets: &[f64],
    l_min: f64,
    l_max: f64,
) -> (Vec<PlateauNode>, Vec<SweepRow>) {
    use crate::spaces::oklab::srgb_linear_to_oklab;
    use crate::spaces::srgb::{hex_from_srgb, srgb_from_hex};
    // Mirrors the `#[cfg(test)]` REFERENCE_NODES (owner reference ramp; pure
    // #FFFFFF dropped as achromatic). Bound by the shared metric, not by name.
    let nodes: [&str; 12] = [
        "#101012", "#151518", "#212125", "#303136", "#44444B", "#5B5C64", "#787881", "#9698A2",
        "#B3B5BF", "#CDD0D9", "#E4E7ED", "#F6F8FA",
    ];
    let vc = ViewingConditions::srgb();
    let mut plateau: Vec<PlateauNode> = Vec::new();
    for hex in nodes {
        let Ok(rgb) = srgb_from_hex(hex) else {
            continue;
        };
        let l = srgb_linear_to_oklab(rgb)[0];
        if l < l_min || l > l_max {
            continue;
        }
        let Ok(node) = crate::lcs::LcsColor::from_hex_with_vc(hex, &vc) else {
            continue;
        };
        plateau.push((l, node.mp()));
    }
    let curve_mp = |l: f64, target: f64| -> Option<f64> {
        let h = cusp_attracted_hue(l, NEUTRAL_HUE_DEG, TINT_HUE_STIFFNESS);
        let r = ratio_for_target_mp(l, h, target, &vc);
        let rgb = build_curve_color_with_cmax(l, h, r, crate::scale::max_chroma(l, h));
        crate::lcs::LcsColor::from_hex_with_vc(&hex_from_srgb(rgb), &vc)
            .ok()
            .map(|c| c.mp())
    };
    let mut sweep: Vec<SweepRow> = Vec::with_capacity(targets.len());
    for &t in targets {
        let mut sumsq = 0.0_f64;
        let mut maxabs = 0.0_f64;
        let mut n = 0_usize;
        for &(l, ref_mp) in &plateau {
            if let Some(cm) = curve_mp(l, t) {
                let d = (cm - ref_mp).abs();
                sumsq += d * d;
                if d > maxabs {
                    maxabs = d;
                }
                n += 1;
            }
        }
        let rms = if n > 0 {
            (sumsq / n as f64).sqrt()
        } else {
            f64::NAN
        };
        sweep.push((t, rms, maxabs));
    }
    (plateau, sweep)
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
    /// This is the *legal floor* the solver can never drop below for `role` —
    /// independent of the perceptual target and of the background. Anchored
    /// (text / UI) roles carry their [`TextAnchor`]'s WCAG conformance
    /// ([`Floor::AaText`] → 4.5, [`Floor::AaUi`] → 3.0); every decorative /
    /// JND / zero role has no legal floor and returns `None`.
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
    /// [`RoleChroma::flat_neutral_tint`] for the v1 flat-ratio undertone, or a
    /// custom [`RoleChroma::Tinted`] / [`RoleChroma::Curve`] for another policy.
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

/// The outcome of resolving one role: a solved colour, an honest zero, a typed
/// numerical indeterminacy, or a failure reason.
///
/// Physical unreachability is surfaced per role, never masked — a role on an extreme
/// background (e.g. muted text on a mid-grey that cannot supply enough contrast)
/// returns [`SolveFailure`], it is not silently clipped to a wrong colour.
/// [`SolveFailure::InternalInvariant`] has different provenance: bindings must
/// turn it into a whole-call internal/incompatible-contract error.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum Resolved {
    /// A solved colour for a text/UI or decorative role. `compressed` is `true`
    /// when the legal floor squeezed this role's target against its senior's so
    /// the strict hierarchy could not hold and the role was demoted to the
    /// smallest distinguishable step below — an honest, flagged degradation
    /// rather than a silent two-roles-one-colour collapse. See the module docs.
    ///
    /// `achieved_dj` — честный замер |ΔJ'| на отданном hex для dJ'-ролей
    /// (симметрия честности с [`GlowResolved::achieved_dj`]); `None` у
    /// контраст-ролей (их метрика — Lc, он в [`Solved::lc`]).
    Color {
        solved: Solved,
        compressed: bool,
        achieved_dj: Option<f64>,
        /// Цветной лейбл (M1 ch5c) фактически ПОТЕРЯЛ цвет: на решённой
        /// уровнем-контрактом светлоте красочность `M'` цвета упала ниже порога
        /// воспринимаемости тинта (`TINT_PERCEPTIBLE_MP_FLOOR`) — у краёв
        /// LCS-кривой семьи (почти-белый / почти-чёрный) хрома физически → 0.
        /// Честный флаг, НЕ молчаливая деградация к серому/белому: потребитель
        /// читает флаг и знает, что оттенок семьи здесь неразличим. `false` у
        /// нейтральных лейблов и у цветных, сохранивших различимый цвет.
        hue_vanished: bool,
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
    /// A typed solve failure. Its core-owned category distinguishes a proved
    /// unreachable contract, an unresolved bounded search, a rejected request,
    /// and an unsupported capability. Internal invariants fail the whole call
    /// closed at every binding.
    Failure(SolveFailure),
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
    /// Знаковый перцептивный контраст `Lc` композита против фона резолва.
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
    /// сдвига — оттенок/насыщенность семьи сохранены, изменилась лишь светлота.
    /// `false` у прямой лестницы и когда семейный солид уже легален (Figma-тинт
    /// эмитирован без сдвига).
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

    /// Знаковый `Lc` композита против фона резолва (метрика фазы 1 AA).
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
    /// Алиас совместимости: прежнее поле измеряло именно halo.
    pub fn achieved_dj(&self) -> f64 {
        self.halo_achieved_dj
    }
    /// Алиас совместимости: `true` для точного и legacy-исходов недостижимости.
    pub fn degraded(&self) -> bool {
        matches!(
            self.target_status,
            crate::glow::GlowTargetStatus::ExactNoopUnreachable
                | crate::glow::GlowTargetStatus::LegacyUnreachable
        )
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

/// Резолв двухслойного материала: тон `T` (семейно-оттеночный опаковый цвет на
/// целевой светлоте тира) + ВЫВЕДЕННАЯ альфа тинта.
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
/// и [`guaranteed`](Self::guaranteed) пересчитываемы потребителем из эмитированных
/// `01`/`02`: ядро и официальный `packages/colors/effective-bg.js::compositeOver`
/// используют один byte-scale affine order `B + α·(T−B)`.
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
    hue_vanished: bool,
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

    /// Предикат совместимости поверх [`Self::alpha_status`].
    pub fn guaranteed(&self) -> bool {
        self.alpha_status == crate::material::MaterialAlphaStatusV1::Satisfied
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

    /// Оттенок семьи физически выродился у края гамута (near-white/near-black):
    /// красочность тона ниже порога воспринимаемости. Честный флаг — не
    /// молчаливая деградация к серому. `false` у нейтрали и различимых оттенков.
    pub fn hue_vanished(&self) -> bool {
        self.hue_vanished
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
            hue_vanished: false,
        }
    }

    /// The solved colour, if this role resolved to one.
    pub fn solved(&self) -> Option<&Solved> {
        match self {
            Resolved::Color { solved, .. } => Some(solved),
            _ => None,
        }
    }

    /// Цветной лейбл потерял различимый цвет на решённой светлоте (M1 ch5c):
    /// `M'` цвета ниже `TINT_PERCEPTIBLE_MP_FLOOR`. `false` для нейтральных и
    /// сохранивших цвет ролей, для zero и unreachable. Честный сигнал вырождения
    /// оттенка — не молчаливая деградация.
    pub fn hue_vanished(&self) -> bool {
        matches!(
            self,
            Resolved::Color {
                hue_vanished: true,
                ..
            }
        )
    }

    /// Whether this role produced an explicitly non-exact outcome:
    ///
    /// - contrast roles: the legal floor forced the colour onto (or just below)
    ///   its senior, so its place in the hierarchy order is non-strict;
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

    /// The signed perceptual contrast `Lc` of a resolved colour, if any. The
    /// zero token reports `0.0`; an unreachable role reports `None`; a
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
    /// The maximum contrast magnitude the background supplies in `polarity`, or
    /// `None` if the background has no headroom in it at all (a pathological
    /// extreme). Anchored roles need this to take their fraction of it.
    max_contrast: Option<f64>,
    /// The background's H-K luminance interval, computed once for the whole set.
    /// Every role's solve reuses it instead of re-deriving the background's
    /// CIECAM16 forward per call. `Err` if the background cannot be reduced —
    /// then every colour role surfaces that reason.
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
            .ok()
            .and_then(|iv| max_contrast(bg, polarity, vc, *iv).ok());
        Self {
            polarity,
            max_contrast,
            interval,
            high_contrast: vc.high_contrast,
        }
    }

    /// The signed `Lc` target for an anchored text/UI role: the chosen polarity's
    /// sign times `fraction` of the background's maximum contrast. `Err` when the
    /// background has no headroom in the chosen polarity (the honest max-ratio is
    /// reported by the role's solve).
    fn anchored_contract(&self, anchor: TextAnchor) -> Result<Contract, SolveFailure> {
        let max = self.max_contrast.ok_or(SolveFailure::FloorUnreachable {
            floor: POLARITY_FLOOR_RATIO,
            max_ratio: 0.0,
        })?;
        let target = self.polarity.sign() * anchor.fraction() * max;
        Ok(Contract::text(target).with_conformance(anchor.conformance()))
    }

    /// The signed range contract for a decorative JND role: the chosen polarity's
    /// sign times a magnitude held above [`DECORATIVE_FLOOR_MIN`], no readability
    /// floor.
    ///
    /// Under high contrast the floor delta `IC_DECORATIVE_FLOOR_MIN −
    /// DECORATIVE_FLOOR_MIN` is applied as an ORDER-PRESERVING uniform shift on
    /// top of the regular floored magnitude — not as a `max` with the IC floor.
    /// A plain `max(|m|, 15.0)` collapsed every decorative magnitude below 15
    /// (the whole shadow stack 8/9.5/11.5/14 and the separator) onto one
    /// identical target, so under `-ic` all four shadows resolved to the same
    /// byte-identical colour, violating the stack's strictly-ascending contract
    /// and silently mutating the owner-measured Lc deltas. The shift keeps every
    /// pairwise gap exactly as measured while guaranteeing the result is at
    /// least `IC_DECORATIVE_FLOOR_MIN` (any input already sits at or above
    /// `DECORATIVE_FLOOR_MIN` after the regular floor).
    fn decorative_contract(&self, magnitude: f64) -> Contract {
        let floored = magnitude.abs().max(DECORATIVE_FLOOR_MIN);
        let effective = if self.high_contrast {
            floored + (IC_DECORATIVE_FLOOR_MIN - DECORATIVE_FLOOR_MIN)
        } else {
            floored
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
pub fn resolve(bg: &BgInput, role: Role, table: &RoleTable, vc: &ViewingConditions) -> Resolved {
    let ctx = ResolveContext::new(bg, vc);
    resolve_in(bg, role, table, vc, &ctx)
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
) -> Resolved {
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
) -> Resolved {
    let contract = match *spec {
        RoleSpec::Zero => return Resolved::None,
        RoleSpec::Anchor(anchor) => {
            // Цветной лейбл (M1 ch5c): тот же контракт уровня, решённый в чистом
            // оттенке семьи. Нейтральный (`hue == None`) идёт прежним путём
            // байт-в-байт.
            if let Some(hue_tint) = anchor.hue() {
                return resolve_hued_anchor(bg, anchor, hue_tint, vc, ctx);
            }
            match ctx.anchored_contract(anchor) {
                Ok(c) => c,
                Err(reason) => return Resolved::Failure(reason),
            }
        }
        RoleSpec::DecorativeDj { magnitude_dj } => {
            // dJ' has its own analytic solver (J' offset, not an Lc contract); it
            // builds the undertone itself, so it does not route through
            // `solve_with_chroma`. Если цель не попала в бюджет локального
            // ограниченного обхода, кандидат с минимальной ошибкой среди
            // просмотренных возвращается с `compressed`; флаг не утверждает
            // оптимум по всему гамуту.
            return match resolve_dj(bg, magnitude_dj.for_vc(vc), ctx.polarity, chroma, vc) {
                Ok(d) => Resolved::Color {
                    solved: d.solved,
                    compressed: d.degraded,
                    achieved_dj: Some(d.achieved_dj),
                    hue_vanished: false,
                },
                Err(reason) => Resolved::Failure(reason),
            };
        }
        RoleSpec::Decorative { magnitude } => ctx.decorative_contract(magnitude),
        RoleSpec::PairFill { tint } => {
            // Сторона пары — идентичность СЕМЬИ: решается по каноническому
            // светлому якорю и не флипается между темами/IC (тёмные якоря
            // labui осветлены и перелезали бы кроссовер). Пер-режимный якорь
            // затем двигается ПОД эту сторону; солид — лестничной сантехникой
            // (α = 1; композит на фоне резолва замеряется честно).
            let side =
                crate::pair::pair_side(tint.for_vc(&crate::spaces::vc::ViewingConditions::srgb()));
            let fill = crate::pair::pair_fill(tint.for_vc(vc), side);
            return resolve_rgba_direct(fill, 1.0, bg, vc);
        }
        RoleSpec::PairLabel {
            tint,
            fraction,
            floor,
            surface_alpha_light,
            surface_alpha_dark,
        } => {
            return resolve_pair_label(
                bg,
                tint,
                fraction,
                floor,
                surface_alpha_light,
                surface_alpha_dark,
                vc,
            );
        }
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
            // Свечение: halo = якорь источника по теме; core — пересвет;
            // интенсивность решается под контрактную ступень на фоне резолва.
            // Typed execution mode исполняется ПРЯМО из compiled spec (#292):
            // никакого plan lookup или string policy selection в hot path.
            let halo_hex = crate::spaces::srgb::hex_from_srgb_encoded(tint.for_vc(vc));
            let bg_hex =
                crate::spaces::srgb::hex_from_srgb_encoded(quantise_encoded(bg.encoded_display()));
            // Общая сборка полного Glow-результата из решённого состояния —
            // одна для обоих атомарных законных исходов.
            let assemble = |g: &crate::glow::GlowSolve,
                            outcome: crate::glow::GlowDecisionOutcomeV1|
             -> Resolved {
                let (core_hex, halo_hex) = match crate::glow::glow_layers_from_source(&halo_hex, vc)
                {
                    Ok(pair) => pair,
                    Err(e) => {
                        return Resolved::Failure(SolveFailure::InternalInvariant(format!(
                            "generated Glow layer recipe was rejected: {e}"
                        )));
                    }
                };
                let core_measurement = match crate::glow::measure_screen_layer_at_alpha(
                    &core_hex,
                    &bg_hex,
                    g.alpha(),
                    vc,
                ) {
                    Ok(measurement) => measurement,
                    Err(e) => {
                        return Resolved::Failure(SolveFailure::InternalInvariant(format!(
                            "generated Glow core measurement was rejected: {e}"
                        )));
                    }
                };
                Resolved::Glow(GlowResolved {
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
                    layer_recipe_profile:
                        crate::glow::GlowLayerRecipeProfileV1::Cam16JPrimeOklabCuspV1,
                    appearance_diagnostic_profile:
                        crate::glow::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1,
                    selection_diagnostic_profile: g.selection_diagnostic_profile(),
                    decision_outcome: outcome,
                    halo_composite_certificate: g.composite_certificate().clone(),
                    core_composite_certificate: core_measurement.certificate,
                })
            };
            return match crate::glow::solve_screen_alpha_for_dj(
                &halo_hex,
                &bg_hex,
                step.target_dj(),
                mode,
                vc,
            ) {
                Ok(crate::numerics::NumericalDecisionV1::Indeterminate { site_id, evidence }) => {
                    Resolved::GlowIndeterminate(GlowIndeterminateResolved {
                        source_hex: halo_hex,
                        target_dj: step.target_dj(),
                        decision_profile: crate::glow::GlowDecisionProfileV1::from_execution_mode(
                            mode,
                        ),
                        site_id,
                        evidence,
                    })
                }
                Ok(crate::numerics::NumericalDecisionV1::Determinate {
                    value: g,
                    evidence,
                    ..
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
                Err(e) => Resolved::Failure(SolveFailure::InternalInvariant(format!(
                    "generated Glow solve request was rejected: {e}"
                ))),
            };
        }
        RoleSpec::AlphaAnalog { of, alpha } => {
            // Альфа-аналог: солид-цель фиксирована (тинт источника по теме),
            // тинт выводится композит-инверсией (`crate::alpha`, #119). Фактическая
            // α поднимается до α_min, если запрошенная неразрешима в гамуте.
            return resolve_rgba_inverted(of.for_vc(vc), alpha, bg, vc);
        }
        RoleSpec::Material { hue, tone, floor } => {
            // Материал (whitepaper §3.7): тон-база — семейно-оттеночная опаковая поверхность на
            // целевом |ΔJ'| тира (dj-anchor-солвером), тинт — тот же тон с
            // ВЫВЕДЕННОЙ альфой. Нейтральный материал (`hue == None`) держит подтон
            // ТАБЛИЦЫ (тот же 286°, что остальные нейтральные эмиссии); семейный
            // подставляет оттенок якоря в кривую подтона.
            let tone_chroma = match hue {
                None => chroma,
                Some(hue_tint) => {
                    let hue_deg = crate::accent::oklab_hue_of(
                        &crate::spaces::srgb::hex_from_srgb_encoded(hue_tint.for_vc(vc)),
                    );
                    // Оттенок семьи подставляется в НЕСУЩИЙ ОТТЕНОК подтон таблицы,
                    // красочность (`target_mp`/`hue_stiffness` / `ratio`) — от неё же.
                    match chroma {
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
                        // Ахроматичный (или будущий) подтон не несёт оттенка —
                        // семейный материал на нём невыразим. Честный отказ, НЕ тихая
                        // подмена нейтральным тоном при флаге `family_hued=true`.
                        RoleChroma::Neutral => {
                            return Resolved::Failure(SolveFailure::InvalidInput(
                                "семейный материал требует хроматического подтона \
                                 таблицы (curve/tinted), у таблицы — ахроматический"
                                    .to_string(),
                            ));
                        }
                    }
                }
            };
            return resolve_material(
                bg,
                tone.for_vc(vc),
                floor,
                ctx.polarity,
                tone_chroma,
                hue.is_some(),
                vc,
            );
        }
    };

    let interval = match &ctx.interval {
        Ok(iv) => *iv,
        Err(reason) => return Resolved::Failure(reason.clone()),
    };
    match solve_with_chroma(bg, contract, chroma, vc, interval) {
        Ok(solved) => Resolved::color(solved),
        Err(reason) => Resolved::Failure(reason),
    }
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

fn role_alpha_valid(alpha: f64) -> bool {
    alpha.is_finite() && alpha > 0.0 && alpha <= 1.0
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
) -> Resolved {
    if !encoded_rgb_valid(tint_encoded) {
        return Resolved::Failure(SolveFailure::InternalInvariant(
            "validated/generated rgba tint left encoded sRGB domain".into(),
        ));
    }
    if !role_alpha_valid(alpha) {
        return Resolved::Failure(SolveFailure::InvalidInput(
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

/// Резолв ЦВЕТНОГО текст/UI-лейбла (ратификация ch5c, M1).
///
/// Цветной лейбл держит ТОТ ЖЕ Lc-контракт уровня, что нейтральный
/// ([`ResolveContext::anchored_contract`] — доля·max ахроматической полярности +
/// тот же WCAG-пол): одноуровневость поперёк характеров ПО ПОСТРОЕНИЮ, потому что
/// цель — абсолютный Lc нейтральной ступени, а НЕ доля от максимума-в-оттенке
/// (последнее снова оказалось бы слабее нейтрали). Отличие от нейтрального пути —
/// физика цвета:
///
/// * оттенок = ИДЕНТИЧНОСТЬ семьи (Oklab-угол пер-темного тинта-якоря; Figma-
///   оттенок сохранён, светлота выводится);
/// * лексикографический порядок «красоты» — (1) контракт читаемости фиксирует
///   светлоту на LCS-кривой семьи, (2) на ней берётся МАКСИМУМ чистого цвета
///   (стена гамута, [`ChromaPolicy::Relative`]`(1.0)`): каждая точка кривой
///   красива, хрома выведена, не оптимизируется отдельной метрикой.
///
/// Честные исходы (не молчаливая деградация):
/// * `compressed` — юр. пол уровня перекрыл перцептивную цель
///   ([`Solved::floor_override`]): контракт занят ближайшим легальным, не точным;
/// * `hue_vanished` — на решённой светлоте красочность `M'` цвета упала ниже
///   `TINT_PERCEPTIBLE_MP_FLOOR`: у краёв кривой (почти-белый/чёрный) хрома
///   физически → 0, лейбл фактически потерял цвет — объявлено флагом.
fn resolve_hued_anchor(
    bg: &BgInput,
    anchor: TextAnchor,
    hue_tint: crate::ladder::LadderTint,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) -> Resolved {
    resolve_hued_anchor_from_encoded_source(bg, anchor, hue_tint.for_vc(vc), vc, ctx)
}

/// Тот же цветной резолв, но источник оттенка — уже выбранный (по теме)
/// кодированный стимул, а не [`LadderTint`]-пейлоад.
///
/// Отдельный вход нужен appearance-графу (#307): foreground occurrence несёт
/// identity-ребро «что наблюдается», и потребитель обязан решать foreground из
/// ВОЗВРАЩЁННОГО occurrence-источника (байты → byte/255 точно), а не повторно
/// читать исходный пейлоад — иначе ребро идентичности было бы декоративным.
/// Для квантованного источника оба пути дают один hex по построению
/// ([`crate::spaces::srgb::hex_from_srgb_encoded`] округляет так же, как
/// квантизация эмиссии), что закреплено differential-тестами миграции.
fn resolve_hued_anchor_from_encoded_source(
    bg: &BgInput,
    anchor: TextAnchor,
    source_encoded: [f64; 3],
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) -> Resolved {
    let contract = match ctx.anchored_contract(anchor) {
        Ok(c) => c,
        Err(reason) => return Resolved::Failure(reason),
    };
    let interval = match &ctx.interval {
        Ok(iv) => *iv,
        Err(reason) => return Resolved::Failure(reason.clone()),
    };
    let hue_deg =
        crate::accent::oklab_hue_of(&crate::spaces::srgb::hex_from_srgb_encoded(source_encoded));
    match solve::solve_in(
        bg,
        contract,
        Hue::deg(hue_deg),
        ChromaPolicy::Relative(1.0),
        vc,
        interval,
    ) {
        Ok(solved) => {
            let hue_vanished = solved.color().mp() < TINT_PERCEPTIBLE_MP_FLOOR;
            // Тот же смысл, что у нейтрали: пол перекрыл перцептивную цель.
            let compressed = solved.floor_override();
            Resolved::Color {
                solved,
                compressed,
                achieved_dj: Option::None,
                hue_vanished,
            }
        }
        Err(reason) => Resolved::Failure(reason),
    }
}

/// Резолв СОЛИДНОЙ семейной границы `border-<family>-strong` с юр. полом UI
/// (ратификация ch5c, M2).
///
/// Солид семьи (α=1) обязан держать 3:1 (WCAG 1.4.11 для границ контролов). Если
/// композит тинта уже чист (≥ пола) — эмитится ТОЧНЫЙ семейный солид (Figma-
/// идентичность цела, диффа эмиссии нет). Иначе — МИНИМАЛЬНЫЙ ЛЕГАЛЬНЫЙ СДВИГ по
/// кривой семьи: контракт целит естественный Lc тинта, а юр. пол притемняет цвет
/// РОВНО до легальности; оттенок и насыщенность семьи сохранены (доля хромы
/// якоря на решённой светлоте). Сдвиг объявлен флагом
/// [`TranslucentResolved::floor_coerced`] — не молчаливая деградация. Эмиссия
/// остаётся полупрозрачной формой (`rgba`, α=1), как у любой семейной границы,
/// чтобы форма роли не расходилась между легальными и притемнёнными характерами.
fn resolve_solid_with_ui_floor(
    tint_encoded: [f64; 3],
    floor: Floor,
    bg: &BgInput,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) -> Resolved {
    if !encoded_rgb_valid(tint_encoded) {
        return Resolved::Failure(SolveFailure::InternalInvariant(
            "validated/generated solid tint left encoded sRGB domain".into(),
        ));
    }
    let bg_encoded = bg.encoded_display();
    let tint_q = quantise_encoded(tint_encoded);
    let min_ratio = match floor.min_ratio() {
        Some(r) => r,
        // Пол None (декоратив) — притемнять не нужно; прямой солид.
        None => return resolve_rgba_direct(tint_encoded, 1.0, bg, vc),
    };
    let current_wcag = crate::wcag::contrast_ratio(tint_q, bg_encoded);
    if current_wcag >= min_ratio {
        // Уже легально — точный семейный солид, без сдвига (floor_coerced = false).
        // Сравнение прямое: у семейных якорей нет цвета РОВНО на границе 3:1
        // (легальные — с запасом, нелегальные — заметно ниже), поэтому f64-шум
        // отношения не может переклассифицировать роль; порог-допуск не нужен.
        return resolve_rgba_direct(tint_encoded, 1.0, bg, vc);
    }
    // Нелегально: минимальный сдвиг по кривой семьи до пола.
    let interval = match &ctx.interval {
        Ok(iv) => *iv,
        Err(reason) => return Resolved::Failure(reason.clone()),
    };
    let tint_hex = crate::spaces::srgb::hex_from_srgb_encoded(tint_q);
    let hue_deg = crate::accent::oklab_hue_of(&tint_hex);
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
    let lab = crate::spaces::oklab::srgb_linear_to_oklab(tint_linear);
    let anchor_chroma = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
    let c_max = scale::max_chroma(lab[0], hue_deg);
    let chroma_ratio = if c_max > f64::EPSILON {
        (anchor_chroma / c_max).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Контракт целит естественный Lc; пол уровня (`floor`) притемняет ровно до
    // легальности — минимальный сдвиг по построению solve.
    let contract = solve::Contract::text(anchor_lc).with_conformance(floor);
    match solve::solve_in(
        bg,
        contract,
        Hue::deg(hue_deg),
        ChromaPolicy::Relative(chroma_ratio),
        vc,
        interval,
    ) {
        Ok(solved) => match crate::spaces::srgb::srgb_encoded_from_hex(solved.hex()) {
            Ok(shifted) => finish_rgba(shifted, 1.0, bg_encoded, vc, false, true),
            Err(reason) => Resolved::Failure(SolveFailure::InternalInvariant(format!(
                "solver emitted an invalid sRGB hex for solid-floor role: {reason}"
            ))),
        },
        Err(reason) => Resolved::Failure(reason),
    }
}

/// Непрозрачные структурные handles компонента «derived source-over
/// поверхность и foreground occurrence против неё» ([`crate::appearance`]).
/// Значения произвольны и не участвуют в физике (инвариант закреплён
/// graph-тестами); граф не знает ни одного клиентского имени — привязку к
/// рецепту делает только этот модуль.
const NESTED_SOURCE: crate::appearance::ColorInputId = crate::appearance::ColorInputId::new(0);
const NESTED_CONTEXT: crate::appearance::ColorInputId = crate::appearance::ColorInputId::new(1);
const NESTED_OPACITY: crate::appearance::OpacityInputId = crate::appearance::OpacityInputId::new(0);
const NESTED_CONTEXT_SURFACE: crate::appearance::SurfaceId = crate::appearance::SurfaceId::new(0);
const NESTED_DERIVED_SURFACE: crate::appearance::SurfaceId = crate::appearance::SurfaceId::new(1);
const NESTED_FOREGROUND: crate::appearance::OccurrenceId = crate::appearance::OccurrenceId::new(0);

/// Один статически скомпилированный generic-компонент вложенного foreground:
///
/// ```text
/// context input → context surface
/// source + opacity + context surface → exact source-over derived surface
/// foreground occurrence(identity = source) против derived surface
/// ```
///
/// Компилируется один раз ([`OnceLock`](std::sync::OnceLock)); спека статична,
/// поэтому ошибка компиляции недостижима по построению, но путь остаётся
/// типизированным (RoleSpec публичен, паника на публичном входе запрещена).
fn nested_foreground_component() -> Result<
    &'static crate::appearance::CompiledAppearanceGraph,
    &'static crate::appearance::GraphError,
> {
    use crate::appearance::{
        AppearanceGraphSpec, CompiledAppearanceGraph, CompositionProfileV1,
        ForegroundOccurrenceSpec, GraphError, SurfaceSpec,
    };
    static COMPONENT: std::sync::OnceLock<Result<CompiledAppearanceGraph, GraphError>> =
        std::sync::OnceLock::new();
    COMPONENT
        .get_or_init(|| {
            AppearanceGraphSpec::new(
                vec![NESTED_SOURCE, NESTED_CONTEXT],
                vec![NESTED_OPACITY],
                vec![
                    SurfaceSpec::Input {
                        id: NESTED_CONTEXT_SURFACE,
                        color: NESTED_CONTEXT,
                    },
                    SurfaceSpec::SourceOver {
                        id: NESTED_DERIVED_SURFACE,
                        source: NESTED_SOURCE,
                        opacity: NESTED_OPACITY,
                        backdrop: NESTED_CONTEXT_SURFACE,
                        profile: CompositionProfileV1::EncodedSrgb8SourceOverV1,
                    },
                ],
                vec![ForegroundOccurrenceSpec {
                    id: NESTED_FOREGROUND,
                    identity_source: NESTED_SOURCE,
                    against: NESTED_DERIVED_SURFACE,
                }],
            )
            .compile()
        })
        .as_ref()
}

/// Public PairLabel opacity rejected by the graph's SSOT validator.
fn pair_label_opacity_input_error(error: &str) -> Resolved {
    Resolved::Failure(SolveFailure::InvalidInput(format!(
        "тинт-поверхность бейджа вне encoded-sRGB8 reference-домена: {error}"
    )))
}

/// Резолв лейбла ТИНТ-бейджа — жёсткий контраст `label ↔ tinted-surface`
/// ([`crate::pair`], лейбл-сторона; родственен [`resolve_solid_with_ui_floor`],
/// но пол энфорсится против ВЫВОДИМОЙ подложки, а не против фона страницы).
///
/// С миграции #307 это compatibility-адаптер над одним generic-компонентом
/// appearance-графа ([`nested_foreground_component`]): скомпилированный граф
/// точно собирает derived-поверхность (объявленный тинт при compatibility-альфе
/// позиции `fill-*-primary` над локальным фоном резолва — exact source-over в
/// encoded-sRGB8 профиле) и возвращает foreground occurrence именно против неё.
/// Поверхность НЕ является эмитированным [`RoleSpec::PairFill`] — у того своя,
/// отдельно сдвинутая солид-эмиссия; никакого ребра `PairFill → PairLabel` нет.
///
/// Оттеночный foreground решается текущим
/// [`resolve_hued_anchor_from_encoded_source`] НА ЭТОЙ ПОВЕРХНОСТИ. Appearance-
/// граф не присваивает этому последующему решению доказательный статус: он
/// возвращает только source/against/backdrop. Дифференциальный тест закрепляет
/// подключение и результаты миграции на проверяемом домене, но оба пути
/// используют один резолвер и потому не образуют независимый эталон его
/// математики. Собственный [`ResolveContext`] поверхности задаёт
/// полярность/макс-контраст, поэтому
/// WCAG-пол лейбла
/// гарантирован против той подложки, на которой foreground реально стоит
/// (обычные `label-*` роли решаются против страницы, и на тинт-подложке их
/// контраст проседает — класс, который закрывает эта роль). Недостижимость пола
/// на кривой семьи клампит тон (`floor_override` → `compressed`), как у любой
/// контраст-роли; флаг не позволяет выдать нестрогий исход за точное выполнение.
/// Занимаемые графом typed handles структурны; клиентские
/// имена в граф не передаются.
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_pair_label(
    bg: &BgInput,
    tint: crate::ladder::LadderTint,
    fraction: f64,
    floor: Floor,
    surface_alpha_light: f64,
    surface_alpha_dark: f64,
    vc: &ViewingConditions,
) -> Resolved {
    let alpha = if vc.is_dark_theme() {
        surface_alpha_dark
    } else {
        surface_alpha_light
    };
    // Тинт квантуется ДО композита: подложка обязана считаться из отдаваемого
    // значения в едином encoded-sRGB8 reference-домене (контракт не изменён
    // миграцией). Байты источника и локального фона готовятся ТЕМ ЖЕ
    // квантизационным контрактом alpha-SSOT, что и внутри старого пути, —
    // порядок доменных проверок (tint → bg → α) сохранён дословно.
    let tint_q = quantise_encoded(tint.for_vc(vc));
    let source_rgb = match crate::alpha::encoded_to_srgb8(tint_q, "tint") {
        Ok(bytes) => bytes,
        Err(error) => {
            return Resolved::Failure(SolveFailure::InternalInvariant(format!(
                "validated PairLabel tint left encoded sRGB domain: {error}"
            )));
        }
    };
    let context_rgb = match crate::alpha::encoded_to_srgb8(bg.encoded_display(), "bg") {
        Ok(bytes) => bytes,
        Err(error) => {
            return Resolved::Failure(SolveFailure::InternalInvariant(format!(
                "validated background left encoded sRGB domain: {error}"
            )));
        }
    };
    let graph = match nested_foreground_component() {
        Ok(graph) => graph,
        // Статическая спека не компилируется только при внутреннем дефекте —
        // типизированный отказ честнее паники (RoleSpec публичен).
        Err(defect) => {
            return Resolved::Failure(SolveFailure::InternalInvariant(format!(
                "внутренний дефект компиляции компонента тинт-поверхности: {defect:?}"
            )));
        }
    };
    let bindings = crate::appearance::AppearanceBindings::new(
        vec![(NESTED_SOURCE, source_rgb), (NESTED_CONTEXT, context_rgb)],
        vec![(NESTED_OPACITY, alpha)],
    );
    let evaluation = match graph.evaluate(&bindings) {
        Ok(evaluation) => evaluation,
        // Доменный отказ по α несёт сообщение SSOT-валидатора дословно —
        // публичный текст отказа совпадает со старым путём байт-в-байт.
        Err(crate::appearance::GraphError::OpacityOutOfDomain { message, .. }) => {
            return pair_label_opacity_input_error(&message);
        }
        // Прочие ошибки исполнения статического компонента структурно
        // недостижимы (bindings собраны из объявленных handles); отказ
        // остаётся типизированным вместо паники.
        Err(defect) => {
            return Resolved::Failure(SolveFailure::InternalInvariant(format!(
                "внутренний дефект исполнения компонента тинт-поверхности: {defect:?}"
            )));
        }
    };
    let Some(occurrence) = evaluation.occurrence(NESTED_FOREGROUND) else {
        return Resolved::Failure(SolveFailure::InternalInvariant(
            "внутренний дефект компонента тинт-поверхности: occurrence отсутствует".into(),
        ));
    };
    // Финальные байты РЕАЛЬНО собранной поверхности → прежний контекст резолва.
    let surface_hex = crate::alpha::hex_from_srgb8(occurrence.backdrop);
    let Ok(surface_bg) = BgInput::solid(&surface_hex) else {
        // Композит 8-битных каналов всегда в кубе — недостижимо, но честнее
        // отказ, чем правдоподобный мусор (RoleSpec публичен).
        return Resolved::Failure(SolveFailure::InternalInvariant(
            "тинт-поверхность бейджа вне кодированного домена sRGB".into(),
        ));
    };
    // Свежий контекст ПОВЕРХНОСТИ: полярность/интервал/макс-контраст берутся от
    // тинт-подложки, не от фона страницы — потому пол энфорсится против неё.
    let surface_ctx = ResolveContext::new(&surface_bg, vc);
    let anchor = match TextAnchor::new(fraction, floor) {
        Ok(anchor) => anchor,
        Err(reason) => return Resolved::Failure(reason),
    };
    // Identity-ребро occurrence: foreground решается из ВОЗВРАЩЁННОГО
    // источника (byte → byte/255 точно), а не повторного чтения `tint` —
    // иначе объявленное ребро идентичности было бы декоративным.
    let source_encoded = occurrence.source.map(|channel| f64::from(channel) / 255.0);
    resolve_hued_anchor_from_encoded_source(&surface_bg, anchor, source_encoded, vc, &surface_ctx)
}

/// Замороженная ручная реализация `resolve_pair_label` ДО миграции #307 —
/// независимый differential-oracle графового пути, НЕ production-дубликат.
/// Композиция здесь идёт прежним `composite_hex_from_encoded`-маршрутом, а
/// foreground — через [`resolve_hued_anchor`] по исходному пейлоаду тинта.
/// Любой байтовый/статусный дрейф production-пути от этого оракула — дефект
/// миграции (см. differential-матрицу в тестах PairLabel).
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_pair_label_legacy_oracle(
    bg: &BgInput,
    tint: crate::ladder::LadderTint,
    fraction: f64,
    floor: Floor,
    surface_alpha_light: f64,
    surface_alpha_dark: f64,
    vc: &ViewingConditions,
) -> Resolved {
    let alpha = if vc.is_dark_theme() {
        surface_alpha_dark
    } else {
        surface_alpha_light
    };
    let tint_q = quantise_encoded(tint.for_vc(vc));
    let surface_hex =
        match crate::alpha::composite_hex_from_encoded(tint_q, alpha, bg.encoded_display()) {
            Ok(hex) => hex,
            Err(error) => {
                return Resolved::Failure(SolveFailure::InvalidInput(format!(
                    "тинт-поверхность бейджа вне encoded-sRGB8 reference-домена: {error}"
                )));
            }
        };
    let Ok(surface_bg) = BgInput::solid(&surface_hex) else {
        return Resolved::Failure(SolveFailure::InternalInvariant(
            "тинт-поверхность бейджа вне кодированного домена sRGB".into(),
        ));
    };
    let surface_ctx = ResolveContext::new(&surface_bg, vc);
    let anchor = match TextAnchor::new(fraction, floor) {
        Ok(anchor) => anchor,
        Err(reason) => return Resolved::Failure(reason),
    };
    resolve_hued_anchor(&surface_bg, anchor, tint, vc, &surface_ctx)
}

/// Альфа-аналог: солид-цель `solid` (кодированный, по теме) на фоне резолва
/// инвертируется в `(tint, фактическая α)`. Перед инверсией цель квантуется до
/// эмитируемой sRGB8-сетки; production-композитор обязан побайтно вернуть её.
fn resolve_rgba_inverted(
    solid_encoded: [f64; 3],
    requested_alpha: f64,
    bg: &BgInput,
    vc: &ViewingConditions,
) -> Resolved {
    // Тот же домен-гард, что у прямого rgba-пути: RoleSpec публичен. Без него
    // недоменная спека, собранная в обход валидатора конфига, дошла бы до
    // численного пути вместо честного типизированного исхода.
    if !encoded_rgb_valid(solid_encoded) {
        return Resolved::Failure(SolveFailure::InternalInvariant(
            "validated alpha-analog solid left encoded sRGB domain".into(),
        ));
    }
    if !role_alpha_valid(requested_alpha) {
        return Resolved::Failure(SolveFailure::InvalidInput(
            "alpha-analog alpha must be finite and inside (0, 1]".into(),
        ));
    }
    let bg_encoded = bg.encoded_display();
    let solid_q = quantise_encoded(solid_encoded);
    let analog =
        match crate::alpha::resolve_alpha_analog_srgb8(solid_q, requested_alpha, bg_encoded) {
            Ok(analog) => analog,
            Err(error) => {
                return Resolved::Failure(SolveFailure::InternalInvariant(format!(
                    "validated alpha-analog resolver violated its total-domain contract: {error}"
                )));
            }
        };
    let (tint_srgb8, actual_alpha) = analog;
    let tint_q = tint_srgb8.map(|channel| f64::from(channel) / 255.0);
    // Резолвер возвращает тот же binary64 либо строго больший точный пол.
    let alpha_coerced = actual_alpha > requested_alpha;
    finish_rgba(tint_q, actual_alpha, bg_encoded, vc, alpha_coerced, false)
}

/// Собрать [`Resolved::Translucent`] из эмитируемых тинта и альфы: вывести их
/// encoded-sRGB8 reference-композит и замерить его против фона резолва.
///
/// Контраст меряется в тех же метриках, что и у солид-роли: перцептивный `Lc`
/// на линейном свете ([`measure_contrast`]) и WCAG на кодированном дисплее — так
/// полупрозрачная роль сопоставима с solved-ролью на фазе 1 AA.
fn finish_rgba(
    tint_encoded: [f64; 3],
    alpha: f64,
    bg_encoded: [f64; 3],
    vc: &ViewingConditions,
    alpha_coerced: bool,
    floor_coerced: bool,
) -> Resolved {
    use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex, srgb_gamma_inv};
    // Единый байтовый домен SSOT нужен и для hex, и для обеих метрик:
    // нормализация `(byte/255)·255` способна изменить граничное значение
    // половинного округления на один LSB.
    let composite_hex =
        match crate::alpha::composite_hex_from_encoded(tint_encoded, alpha, bg_encoded) {
            Ok(hex) => hex,
            Err(error) => {
                return Resolved::Failure(SolveFailure::InternalInvariant(format!(
                    "rgba-композит вне encoded-sRGB8 reference-домена: {error}"
                )));
            }
        };
    let composite_q = match srgb_encoded_from_hex(&composite_hex) {
        Ok(value) => value,
        Err(reason) => {
            return Resolved::Failure(SolveFailure::InternalInvariant(format!(
                "rgba formatter emitted an invalid sRGB hex: {reason}"
            )));
        }
    };
    // Линейный свет из кодированного (per-channel gamma-декод) для перцептивного Lc.
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
    let composite_wcag = crate::wcag::contrast_ratio(composite_q, bg_encoded);
    // Отличимость в encoded-sRGB8 reference: сравнение по тем же
    // 8-битным hex, из которых строится сертификат. Фон квантуется тем же
    // форматтером; применимость к рендереру проверяется отдельно (#241).
    let composite_distinct = composite_hex != hex_from_srgb_encoded(bg_encoded);
    Resolved::Translucent(TranslucentResolved {
        tint_hex: hex_from_srgb_encoded(tint_encoded),
        alpha,
        composite_hex,
        composite_lc,
        composite_wcag,
        composite_distinct,
        alpha_coerced,
        floor_coerced,
    })
}

/// Резолв двухслойного материала (whitepaper §3.7): тон-база `02` на целевом |ΔJ'| в оттенке
/// семьи + тинт `01` (тот же тон) с ВЫВЕДЕННОЙ альфой.
///
/// Тон строится тем же dj-anchor-солвером, что декоративные |ΔJ'|-роли
/// ([`resolve_dj`]), поэтому различимость поверхности от фона наследуется его
/// физикой. Альфа тинта выбирается [`crate::material::solve_material_alpha_hex`]
/// как проходящий верхний кандидат, при котором композит тона над худшим фоном
/// коридора `[чёрный, белый]` держит пол. `family_hued` — оттенок
/// семьи присутствует (флаг вырождения оттенка применим); у нейтрали `false`.
fn resolve_material(
    bg: &BgInput,
    tone_dj: f64,
    floor: Floor,
    polarity: Polarity,
    chroma: RoleChroma,
    family_hued: bool,
    vc: &ViewingConditions,
) -> Resolved {
    use crate::spaces::srgb::hex_from_srgb_encoded;
    // Пол читаемости обязателен: у материала без пола нет цели для вывода α.
    let floor_ratio = match floor.min_ratio() {
        Some(r) => r,
        None => {
            return Resolved::Failure(SolveFailure::InvalidInput(
                "material-роль требует пол читаемости (aa-text/aa-ui), получен zero-floor"
                    .to_string(),
            ));
        }
    };
    // Тон-база 02: семейно-оттеночная опаковая поверхность на целевом |ΔJ'|.
    let dj = match resolve_dj(bg, tone_dj, polarity, chroma, vc) {
        Ok(d) => d,
        Err(reason) => return Resolved::Failure(reason),
    };
    let tone_hex = dj.solved.hex().to_string();
    // Вырождение оттенка семьи у края гамута — только у семейных материалов;
    // нейтраль ахроматична намеренно, не «выродилась».
    let hue_vanished = family_hued && dj.solved.color().mp() < TINT_PERCEPTIBLE_MP_FLOOR;
    // α: повторно проверенный проходящий верхний кандидат над коридором
    // [чёрный, белый].
    let m = match crate::material::solve_material_alpha_hex(&tone_hex, floor_ratio) {
        Ok(m) => m,
        Err(e) => {
            return Resolved::Failure(SolveFailure::InternalInvariant(format!(
                "generated Material solve request was rejected: {e}"
            )));
        }
    };
    // Различимость солид-канона (= тона) от фона резолва на 8-битной сетке (тот же
    // замер, что у полупрозрачных ролей; off-grid фон честно квантуется).
    let bg_hex = hex_from_srgb_encoded(quantise_encoded(bg.encoded_display()));
    let distinct = tone_hex != bg_hex;
    Resolved::Material(MaterialResolved {
        tone_hex,
        alpha: m.alpha(),
        worst_contrast: m.worst_contrast(),
        alpha_guarantee: m.guarantee(),
        alpha_status: m.status(),
        floor: floor_ratio,
        pole: m.pole(),
        achieved_dj: dj.achieved_dj,
        tone_compressed: dj.degraded,
        hue_vanished,
        distinct,
    })
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
) -> Result<Solved, SolveFailure> {
    if let RoleChroma::Curve { .. } = chroma {
        // Probe — discover the contrast-solved lightness achromatically.
        let (probe_hue, probe_chroma) = RoleChroma::probe_plan();
        let probe = solve::solve_in(bg, contract, probe_hue, probe_chroma, vc, interval)?;
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
            solved = solve::solve_in(bg, contract, hue, policy, vc, interval)?;
            let l_new = solved_oklab_lightness(&solved)?;
            if (l_new - l_plan).abs() <= LIGHTNESS_SETTLE {
                break;
            }
            l_plan = l_new;
        }
        Ok(solved)
    } else {
        let (hue, policy) = chroma.plan_for_lightness(0.0, vc);
        solve::solve_in(bg, contract, hue, policy, vc, interval)
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
/// step below if one still clears its floor, flagging it [`Resolved::compressed`]
/// — an honest, visible degradation rather than a silent identical-colour
/// collapse.
#[cfg(test)]
pub fn resolve_set(
    bg: &BgInput,
    table: &RoleTable,
    vc: &ViewingConditions,
) -> Vec<(Role, Resolved)> {
    // The former O(1) grey (`greyfast`) and chromatic-memo (`chromafast`) fast
    // paths were deleted with ADR-0001 PR-c: they only ever accelerated this
    // built-in `resolve_set`, which is no longer on any production path (the
    // agnostic engine ships only the string-keyed `resolve_named_set`). A cold
    // named grey resolve was measured at ~1.7 ms (resolve-only) / ~3.1 ms
    // (compile+resolve) in release — a one-time, sub-frame cost — so the ~468 KB
    // precomputed grey table earned no keep. This built-in path survives solely as
    // the `#[cfg(test)]` byte-identity oracle for the named path, so the live
    // solve is all it needs.
    resolve_set_live(bg, table, vc)
}

/// The full solver sweep behind `resolve_set` — the built-in byte-identity
/// oracle for the named path. Always recomputes.
#[cfg(test)]
pub(crate) fn resolve_set_live(
    bg: &BgInput,
    table: &RoleTable,
    vc: &ViewingConditions,
) -> Vec<(Role, Resolved)> {
    // Memoize the CIECAM16 forward for the span of this set: viewing conditions
    // are fixed here, so the refine fixed-point and the hierarchy pass that
    // re-measure the same candidate colours hit the cache instead of recomputing
    // (25–33 % of the forwards are exact repeats). Cleared on drop.
    let _forward_cache = crate::spaces::cam16::ForwardCacheGuard::activate();
    let ctx = ResolveContext::new(bg, vc);
    let mut set: Vec<(Role, Resolved)> = Role::ALL
        .iter()
        .map(|&role| (role, resolve_in(bg, role, table, vc, &ctx)))
        .collect();
    enforce_text_hierarchy(&mut set, bg, table, vc, &ctx);
    set
}

/// A recipe table keyed by **arbitrary string names**, the config-layer analogue
/// of `RoleTable`.
///
/// Where `RoleTable` carries the fixed v1 `Role` enum, `NamedRoleTable` carries
/// whatever role *names* a consumer's [`ThemeConfig`](crate::config::ThemeConfig)
/// declares — the engine knows none of them. It is built from a config via
/// [`from_config`](crate::config::ThemeConfig::compile_named_role_table) and
/// resolved by [`resolve_named_set`]. The physics is identical to `RoleTable`'s:
/// each entry is the same [`RoleSpec`] the built-in path solves, and the same
/// [`RoleChroma`] undertone applies to the whole table.
///
/// Text ladders are compressed honestly by `enforce_named_text_hierarchy`, the
/// string-keyed analogue of `enforce_text_hierarchy`: a ladder is read off the
/// config (a declaration-order run of strictly-descending [`Anchor`](RoleSpec::Anchor)
/// roles), not off role names, so an arbitrary consumer table degrades a squeezed
/// mid-grey exactly as the built-in table does instead of silently collapsing two
/// labels onto one colour. The pass is a no-op wherever every rung is individually
/// reachable — which is why the labui fixture stays byte-identical on the golden
/// grid (see the byte-identity test).
#[derive(Debug, Clone, PartialEq)]
pub struct NamedRoleTable {
    entries: Vec<(String, RoleSpec)>,
    aliases: Vec<(String, String)>,
    chroma: RoleChroma,
}

impl RoleSpec {
    /// WCAG-пол этой спеки — свойство контракта, не резолва: текст/UI-якорь
    /// несёт пол своего [`TextAnchor`] (AaText → 4.5, AaUi → 3.0), все
    /// остальные формы (декоративные, dJ', лестница, альфа-аналог, zero) —
    /// без легального пола. Одна семантика для обеих таблиц
    /// (`RoleTable::legal_floor` и string-keyed границы).
    pub fn legal_floor(&self) -> Option<f64> {
        match self {
            RoleSpec::Anchor(anchor) => anchor.conformance().min_ratio(),
            // Лейбл тинт-бейджа несёт свой пол против тинт-поверхности — семантика
            // контракта, как у текст/UI-якоря (иерархия-пасс его не трогает: он
            // singleton, не ступень лестницы).
            RoleSpec::PairLabel { floor, .. } => floor.min_ratio(),
            _ => None,
        }
    }
}

impl NamedRoleTable {
    /// Build a named table from its `(name, recipe)` entries and an undertone
    /// policy. Names are the CSS contract downstream (`--lab-{name}`); this
    /// constructor does not validate names — the config validator
    /// ([`ThemeConfig::validate`](crate::config::ThemeConfig::validate)) owns that.
    /// Численный домен `chroma` проверяется здесь, потому что таблицу можно
    /// собрать без конфига. Поэтому невалидная глобальная policy не существует
    /// даже у пустой таблицы, где per-role ошибка была бы некуда записать.
    ///
    /// # Errors
    ///
    /// [`SolveFailure::InvalidInput`], если параметры `chroma` не конечны или
    /// выходят из физического домена своей политики.
    pub fn new(
        entries: Vec<(String, RoleSpec)>,
        aliases: Vec<(String, String)>,
        chroma: RoleChroma,
    ) -> Result<Self, SolveFailure> {
        chroma.validate()?;
        Ok(Self::from_validated_parts(entries, aliases, chroma))
    }

    /// Собирает таблицу из частей, уже проверенных [`ThemeConfig::validate`].
    /// Отдельный crate-private путь не заставляет конфиг переводить одну и ту же
    /// ошибку между двумя error-типами, но не позволяет внешнему коду представить
    /// невалидную глобальную policy.
    pub(crate) fn from_validated_parts(
        entries: Vec<(String, RoleSpec)>,
        aliases: Vec<(String, String)>,
        chroma: RoleChroma,
    ) -> Self {
        Self {
            entries,
            aliases,
            chroma,
        }
    }

    /// Алиасы `(имя, цель)` сохраняются в скомпилированном контракте, чтобы
    /// delivery boundary спроецировала resolved outcome цели под client-owned
    /// именем алиаса. Алиас не запускает отдельный solve и не меняет физическое
    /// значение; без переноса сюда алиасные роли терялись бы при компиляции.
    pub fn aliases(&self) -> &[(String, String)] {
        &self.aliases
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

/// Resolve every named role in `table` against `bg` under `vc`, in declaration
/// order — the string-keyed sibling of `resolve_set`.
///
/// Each `(name, recipe)` pair resolves through the very same `resolve_spec_in`
/// physics core the built-in `resolve_set` uses, so a config whose recipes match
/// the built-in table emits byte-for-byte identical colours (the byte-identity
/// guarantee ADR-0001 requires of the labui fixture). The returned pairs preserve
/// declaration order so a serialiser emits stable output.
///
/// Unlike `resolve_set`, this takes no O(1) grey/chromatic fast path (those are
/// keyed on the built-in default table); it is the honest live sweep for an
/// arbitrary table, followed by the same honest hierarchy-compression pass
/// (`enforce_named_text_hierarchy`) applied to every declared text ladder.
pub fn resolve_named_set(
    bg: &BgInput,
    table: &NamedRoleTable,
    vc: &ViewingConditions,
) -> Vec<(String, Resolved)> {
    if let Err(reason) = table.chroma.validate() {
        // Политика общая для всей таблицы. Частичный правдоподобный результат
        // скрыл бы ошибку вызывающего кода, поэтому каждый объявленный outcome
        // получает одну и ту же структурированную причину.
        return table
            .entries
            .iter()
            .map(|(name, _)| (name.clone(), Resolved::Failure(reason.clone())))
            .collect();
    }
    // One CIECAM16 forward-cache for the span of this sweep, mirroring
    // `resolve_set_live`: the curve refine fixed-point and repeated lightnesses
    // across roles hit the cache instead of recomputing.
    let _forward_cache = crate::spaces::cam16::ForwardCacheGuard::activate();
    let ctx = ResolveContext::new(bg, vc);
    let mut set: Vec<(String, Resolved)> = table
        .entries
        .iter()
        .map(|(name, spec)| {
            (
                name.clone(),
                resolve_spec_in(bg, spec, table.chroma, vc, &ctx),
            )
        })
        .collect();
    // Keep every declared text ladder non-strict-but-honest, the string-keyed
    // analogue of the built-in path's hierarchy pass (see
    // [`enforce_named_text_hierarchy`]). A no-op wherever each ladder is already
    // individually reachable (the labui fixture on the golden grid).
    enforce_named_text_hierarchy(&mut set, table, bg, vc, &ctx);
    set
}

/// Measure the perceptual contrast (`Lc`) and WCAG 2.1 ratio a foreground colour
/// achieves against a background — the cheap **recheck** primitive.
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
/// for the same pair — so a colour the solver resolved against a background
/// re-measures here to its own reported `lc`/`wcag_ratio`. That identity is the
/// guarantee that "still passes" means the same thing as the original solve.
pub fn measure_contrast(
    bg_linear: [f64; 3],
    fg_linear: [f64; 3],
    _vc: &ViewingConditions,
) -> (f64, f64) {
    // Обе метрики — перцептивный `Lc` и легальный WCAG — читают ОДНУ люминансу
    // квантованного display-цвета (ось читаемости в `Ys`, ADR-0003), exactly as
    // the solver measures it (`finish` → `quantised_display`), so the recheck
    // reproduces the solver's reported `lc`/`wcag_ratio` bit-for-bit.
    let fg_disp = crate::solve::quantised_display(fg_linear);
    let bg_disp = crate::solve::quantised_display(bg_linear);
    let lc = crate::lpc::contrast_core(
        crate::wcag::relative_luminance(fg_disp),
        crate::wcag::relative_luminance(bg_disp),
    );
    let wcag = crate::wcag::contrast_ratio(fg_disp, bg_disp);
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
/// `rl` of its display bytes — с активации ADR-0003 перцептивный `Lc` и
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
    Ok(crate::wcag::relative_luminance(disp))
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
            let wcag = crate::wcag::ratio_from_luminances(rl_fg, rl_bg);
            Ok((lc, wcag))
        })
        .collect()
}

/// Multi-background recheck: the `(lc, wcag_ratio)` each foreground achieves
/// against EACH of several background samples, sharing every foreground's
/// forward across all samples. The reactive controller's worst-case loop
/// rechecks the SAME foreground set against N backdrop samples (a gradient /
/// image); each foreground's `rl_fg` is computed ONCE and reused for every
/// sample — с активации ADR-0003 форвард подешевел до одной
/// `relative_luminance` display-байтов (CAM16 ушёл с оси читаемости), но
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
    // Precompute each foreground's background-independent forward exactly once,
    // through the SAME `hex_forward` `recheck_against` uses — so the shared-forward
    // path guarantees byte-identity between the two entry points by construction.
    let fg_pre: Vec<f64> = fg_hexes
        .iter()
        .map(|fg_hex| hex_forward(fg_hex))
        .collect::<Result<_, String>>()?;

    let mut out = Vec::with_capacity(bg_hexes.len() * fg_hexes.len() * 2);
    for bg_hex in bg_hexes {
        let rl_bg = hex_forward(bg_hex)?;
        for &rl_fg in &fg_pre {
            out.push(crate::lpc::contrast_core(rl_fg, rl_bg));
            out.push(crate::wcag::ratio_from_luminances(rl_fg, rl_bg));
        }
    }
    Ok(out)
}

/// Walk the text roles strongest-first and keep the order non-strict but honest.
///
/// The anchor principle already orders the *targets* strictly, but the legal
/// floor can lift two adjacent roles onto the same colour where the readable
/// window is narrower than the hierarchy steps (a near-AA mid-grey). For each
/// junior text role that did not come out strictly weaker than the senior above
/// it, try to demote it by the smallest number of quantisation steps that makes
/// it strictly weaker *while it still clears its own WCAG floor*; if none does,
/// the junior becomes a copy of the senior (equality — never stronger). Either
/// way, flag it [`Resolved::compressed`] so the squeeze is visible, not silent.
#[cfg(test)]
fn enforce_text_hierarchy(
    set: &mut [(Role, Resolved)],
    bg: &BgInput,
    table: &RoleTable,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) {
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
        // The senior's colour is the legal ceiling for the junior: when no
        // distinguishable step below exists, the junior becomes a *copy* of the
        // senior — never a stronger colour. (The floor can lift the junior onto
        // a grid point above the senior; copying restores `senior ≥ junior`.)
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
            (Some(solved), _, _) => Resolved::Color {
                solved,
                compressed: true,
                achieved_dj: Option::None,
                hue_vanished: false,
            },
            // No room to separate: equal to the senior by copy, flagged.
            (None, Some(solved), Resolved::Color { .. }) => Resolved::Color {
                solved,
                compressed: true,
                achieved_dj: Option::None,
                hue_vanished: false,
            },
            (None, _, other) => other.clone(),
        };
    }
}

/// The string-keyed analogue of [`enforce_text_hierarchy`]: keep every declared
/// text ladder in an arbitrary [`NamedRoleTable`] non-strict-but-honest, so the
/// agnostic path degrades a squeezed hierarchy exactly as the built-in one does
/// (V1 found the named path had *no* such pass — a general config on a near-AA
/// mid-grey could silently collapse two labels onto one colour).
///
/// **Which roles form a ladder is read off the config, not off role names.** A
/// ladder is a maximal run of *consecutive* [`Anchor`](RoleSpec::Anchor) roles, in
/// declaration order, whose fractions strictly descend — the shape a text
/// hierarchy has by construction (`primary > secondary > …`). A non-anchor role or
/// a fraction that does not descend ends the run, so `icon` (a lone anchor whose
/// fraction sits above the label below it) and `border-strong` are singleton runs
/// the pass never touches — matching the built-in `TEXT_HIERARCHY` exactly for the
/// labui fixture. Coloured (hued) ladders demote in their family hue
/// ([`demote_below_hued`]); neutral ladders in the undertone ([`demote_below`]).
fn enforce_named_text_hierarchy(
    set: &mut [(String, Resolved)],
    table: &NamedRoleTable,
    bg: &BgInput,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) {
    let entries = table.entries();
    let chroma = table.chroma();

    // Group declaration-order anchors into strictly-descending runs (the hierarchy
    // shape). Runs of length < 2 have no senior/junior pair and are dropped.
    let mut runs: Vec<Vec<usize>> = Vec::new();
    let mut cur: Vec<usize> = Vec::new();
    let mut prev_fraction = f64::INFINITY;
    let flush = |cur: &mut Vec<usize>, runs: &mut Vec<Vec<usize>>| {
        if cur.len() >= 2 {
            runs.push(std::mem::take(cur));
        } else {
            cur.clear();
        }
    };
    for (i, (_, spec)) in entries.iter().enumerate() {
        match spec {
            RoleSpec::Anchor(a) if a.fraction() < prev_fraction => {
                cur.push(i);
                prev_fraction = a.fraction();
            }
            RoleSpec::Anchor(a) => {
                // An anchor that does not descend starts a fresh ladder at itself.
                flush(&mut cur, &mut runs);
                cur.push(i);
                prev_fraction = a.fraction();
            }
            _ => {
                flush(&mut cur, &mut runs);
                prev_fraction = f64::INFINITY;
            }
        }
    }
    flush(&mut cur, &mut runs);

    for run in &runs {
        for pair in run.windows(2) {
            let (senior_idx, junior_idx) = (pair[0], pair[1]);
            let Some(senior_mag) = set[senior_idx].1.solved().map(|s| s.lc().abs()) else {
                continue; // senior unreachable — nothing to compress against
            };
            let Some(junior_mag) = set[junior_idx].1.solved().map(|s| s.lc().abs()) else {
                continue; // junior unreachable — surfaced honestly already
            };
            if junior_mag + STRICT_STEP <= senior_mag {
                continue; // strictly weaker already — hierarchy holds here
            }

            // The junior's own conformance governs how far down it may still be legal.
            let RoleSpec::Anchor(anchor) = entries[junior_idx].1 else {
                continue;
            };
            let floor = anchor.conformance();
            let demoted = match anchor.hue() {
                Some(hue_tint) => demote_below_hued(senior_mag, hue_tint, floor, bg, vc, ctx),
                None => demote_below(senior_mag, ctx, chroma, floor, bg, vc),
            };
            // A hued junior that keeps colour reports `hue_vanished` the same way
            // `resolve_hued_anchor` does; a neutral junior never vanishes a hue.
            let vanished = |solved: &Solved| {
                anchor.hue().is_some() && solved.color().mp() < TINT_PERCEPTIBLE_MP_FLOOR
            };
            let senior_solved = set[senior_idx].1.solved().cloned();
            set[junior_idx].1 = match (demoted, senior_solved, &set[junior_idx].1) {
                // A distinguishable, still-legal step below the senior.
                (Some(solved), _, _) => {
                    let hue_vanished = vanished(&solved);
                    Resolved::Color {
                        solved,
                        compressed: true,
                        achieved_dj: Option::None,
                        hue_vanished,
                    }
                }
                // No room to separate: equal to the senior by copy, flagged.
                (None, Some(solved), Resolved::Color { .. }) => {
                    let hue_vanished = vanished(&solved);
                    Resolved::Color {
                        solved,
                        compressed: true,
                        achieved_dj: Option::None,
                        hue_vanished,
                    }
                }
                (None, _, other) => other.clone(),
            };
        }
    }
}

/// The smallest separation in `|Lc|` that counts as "strictly weaker". Note:
/// near the extremes a single quantisation step can be worth only ~0.2–0.3 Lc,
/// so a demotion may need several grid steps to clear it — and when even the
/// laxest legal target cannot, the junior is set equal to its senior instead.
/// The 0.5 threshold separates real visual distinction from float noise.
///
/// Терминал **(e) DESIGN-CHOICE** (НЕ (c)): `STRICT_STEP` — прямое слагаемое
/// цели демоции (`target = senior_mag − STRICT_STEP`), поэтому его точное
/// значение НЕПРЕРЫВНО и напрямую сдвигает эмитируемый цвет junior-роли —
/// доказательства интервал-нечувствительности нет. Лок
/// `strict_step_sits_just_above_typical_grid_step` лишь характеризует, что
/// 0.5 сидит чуть выше типичного (медианного ≈0.44) Lc-шага 8-бит серой
/// сетки — обоснование ГРАНИЦЫ квантования, не доказательство immaterial-сти.
///
/// Легальный диапазон (Волна 2): `[~0.44, ~7.85)` — снизу типичный (медианный)
/// Lc-шаг кванта серых (ниже него «строго слабее» = float-шум, лок), сверху
/// обрыв loClip мягкого клампа APCA (жёсткий разрыв — единственный шаг такого
/// размера). 0.5 сидит у нижней границы: минимальный шаг, надёжно превышающий
/// шум сетки. Протокол «объективизации»: замерить перцептивный порог
/// различимости соседних Lc-ступеней (JND по контрасту на серой рампе) — тогда
/// `STRICT_STEP = max(этот JND, медианный шаг сетки)`; измерение стало бы
/// кандидатом-выводом (замер → сравнение → решение), не обязательным экспериментом.
// SSOT-TRACKED — minimum Lc separation for visual distinction vs float noise, терминал (e) design-choice (не interval-insensitive — прямо параметризует выход; диапазон [~0.44,~7.85)), см. docs/empirical-inventory.md.
const STRICT_STEP: f64 = 0.5;

/// Try to solve a junior text role at the strongest target that is still
/// *strictly weaker* than its senior (`senior_mag − STRICT_STEP`) and still
/// clears `floor`. Returns the demoted colour, or `None` if even the laxest
/// distinguishable target cannot stay legal — in which case the caller keeps the
/// floored colour and only flags the compression.
fn demote_below(
    senior_mag: f64,
    ctx: &ResolveContext,
    chroma: RoleChroma,
    floor: Floor,
    bg: &BgInput,
    vc: &ViewingConditions,
) -> Option<Solved> {
    // Target just under the senior. The solve still applies the junior's own legal
    // floor, so if that floor lifts the colour right back onto the senior there is
    // no room to distinguish — detected by re-measuring the result below.
    let target = ctx.polarity.sign() * (senior_mag - STRICT_STEP).max(0.0);
    let contract = Contract::text(target).with_conformance(floor);
    // Reuse the set's one background interval; an unreducible background has no
    // demotion to offer, so propagate that as "no distinguishable step".
    let interval = ctx.interval.as_ref().ok().copied()?;
    let solved = solve_with_chroma(bg, contract, chroma, vc, interval).ok()?;
    if solved.lc().abs() + STRICT_STEP <= senior_mag {
        Some(solved)
    } else {
        None
    }
}

/// Hue-preserving sibling of [`demote_below`] for a **coloured** label (M1): the
/// junior is re-solved just under its senior *in the family hue*, not in the
/// neutral undertone — a neutral demote would strip the family colour the whole
/// point of a hued label is to carry. Mirrors [`resolve_hued_anchor`]'s solve
/// (`Hue::deg(family)`, `ChromaPolicy::Relative(1.0)`) but at the reduced target.
fn demote_below_hued(
    senior_mag: f64,
    hue_tint: crate::ladder::LadderTint,
    floor: Floor,
    bg: &BgInput,
    vc: &ViewingConditions,
    ctx: &ResolveContext,
) -> Option<Solved> {
    let target = ctx.polarity.sign() * (senior_mag - STRICT_STEP).max(0.0);
    let contract = Contract::text(target).with_conformance(floor);
    let interval = ctx.interval.as_ref().ok().copied()?;
    let hue_deg = crate::accent::oklab_hue_of(&crate::spaces::srgb::hex_from_srgb_encoded(
        hue_tint.for_vc(vc),
    ));
    let solved = solve::solve_in(
        bg,
        contract,
        Hue::deg(hue_deg),
        ChromaPolicy::Relative(1.0),
        vc,
        interval,
    )
    .ok()?;
    if solved.lc().abs() + STRICT_STEP <= senior_mag {
        Some(solved)
    } else {
        None
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
/// genuinely zero headroom in this polarity returns its reason.
fn max_contrast(
    bg: &BgInput,
    polarity: Polarity,
    vc: &ViewingConditions,
    interval: solve::LumaInterval,
) -> Result<f64, SolveFailure> {
    let sign = polarity.sign();
    // 300 Lc is comfortably past the ~106 ceiling of any sRGB background.
    let probe = Contract::text(sign * 300.0).with_conformance(Floor::None);
    match solve::solve_in(
        bg,
        probe,
        Hue::deg(0.0),
        ChromaPolicy::Neutral,
        vc,
        interval,
    ) {
        // The probe is unreachable by design; ExceedsRange carries the ceiling.
        Err(SolveFailure::ExceedsRange { max_achievable, .. }) => Ok(max_achievable.abs()),
        // A reachable 300 Lc is physically impossible; treat anything else as the
        // background having no usable headroom in this polarity.
        Ok(_) => Err(SolveFailure::InternalInvariant(
            "300 Lc ceiling probe unexpectedly resolved".to_string(),
        )),
        Err(other) => Err(other),
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
    let ratio_dark_on_light = wcag::contrast_ratio([0.0, 0.0, 0.0], bg_disp);
    let ratio_light_on_dark = wcag::contrast_ratio([1.0, 1.0, 1.0], bg_disp);

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
        );
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

        let legacy = resolve_named_set(
            &BgInput::solid("#101012").unwrap(),
            &one_glow_table(
                "#4A8FFF",
                crate::glow::GlowDecisionProfileV1::LegacyPlatformDependentV1,
            ),
            &vc,
        );
        let Resolved::Glow(legacy) = &legacy[0].1 else {
            panic!("explicit legacy selection must resolve as a full Glow result");
        };
        assert_eq!(
            legacy.selection_diagnostic_profile(),
            Some(crate::glow::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1)
        );
        assert_eq!(
            legacy.appearance_diagnostic_profile(),
            crate::glow::GlowDiagnosticProfileV1::Cam16UcsJPrimeLi2017V1
        );
        assert!(matches!(
            legacy.target_status(),
            crate::glow::GlowTargetStatus::LegacyReached
                | crate::glow::GlowTargetStatus::LegacyUnreachable
        ));
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
    fn measure_contrast_reproduces_the_solvers_own_lc_and_wcag() {
        // The recheck primitive must agree with the solver's own `finish`
        // measurement: a colour the solver resolved against a background
        // re-measures here to EXACTLY its reported lc/wcag. This identity is what
        // makes the runtime's "do these colours still pass?" mean the same thing
        // as the original solve — the foundation of the lazy/hysteresis controller.
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
                    assert!(
                        (wcag - solved.wcag_ratio()).abs() < 1e-9,
                        "{role:?} on {bg_hex}: recheck wcag {wcag} != solver {}",
                        solved.wcag_ratio()
                    );
                }
            }
        }
    }

    #[test]
    fn legal_floor_is_held_across_a_full_background_sweep() {
        // Defence-in-depth for the engine's core legal guarantee: every anchored
        // role's resolved colour clears its WCAG legal floor against EVERY
        // background across the full 256-step grey axis plus a chromatic palette,
        // under both calibrated viewing conditions. WCAG AA conformance is the
        // engine's reason for existence, so this is the one invariant worth a
        // brute sweep — and the per-role `legal_floor` accessor is only honest if
        // the solver actually meets it everywhere. Doubles as a no-panic sweep:
        // `resolve_set` must return cleanly across this whole input space.
        //
        // The floor is held essentially EXACTLY: the solver lands the quantised
        // hex just above the line (measured worst margin ≈ +1.5e-4 at #949494),
        // never below, so a tight `1e-6` epsilon — not a loose cushion — is the
        // honest assertion. A regression that dropped a role below its legal floor
        // (an accessibility-law violation) would fail here.
        const FLOOR_EPS: f64 = 1e-6;
        let table = RoleTable::default();
        let mut backgrounds: Vec<String> = (0u32..=255)
            .map(|c| format!("#{c:02X}{c:02X}{c:02X}"))
            .collect();
        for hex in [
            "#3478F6", "#FF3B30", "#34C759", "#FF9500", "#AF52DE", "#5AC8FA", "#A2845E", "#0A3D62",
            "#7B2D8E", "#C0FFEE", "#FFD60A", "#BF5AF2", "#30D158", "#FF453A", "#102A44", "#1C1C1E",
        ] {
            backgrounds.push(hex.to_string());
        }
        for (vi, vc) in [ViewingConditions::srgb(), ViewingConditions::dim_surround()]
            .iter()
            .enumerate()
        {
            for bg_hex in &backgrounds {
                let bg = BgInput::solid(bg_hex).unwrap();
                for (role, resolved) in resolve_set(&bg, &table, vc) {
                    let (Some(floor), Some(solved)) = (table.legal_floor(role), resolved.solved())
                    else {
                        continue;
                    };
                    assert!(
                        solved.wcag_ratio() >= floor - FLOOR_EPS,
                        "{role:?} on {bg_hex} (vc{vi}): wcag {:.5} below legal floor {floor}",
                        solved.wcag_ratio()
                    );
                }
            }
        }
    }

    #[test]
    fn legal_floor_reports_each_roles_wcag_clamp_and_holds_under_resolve() {
        // `legal_floor` is the floor the solver can never drop below for a role,
        // independent of background. Anchored roles carry their conformance
        // (AaText → 4.5, AaUi → 3.0); decorative / JND / zero roles have none.
        let table = RoleTable::default();
        assert_eq!(
            table.legal_floor(Role::LabelPrimary),
            Some(crate::wcag::AA_TEXT_RATIO)
        );
        assert_eq!(
            table.legal_floor(Role::LabelTertiary),
            Some(crate::wcag::AA_UI_RATIO)
        );
        // border-strong: различимость (non-text 3:1), не текстовый пол —
        // API-контракт для даунстримов, фиксируем значением.
        assert_eq!(
            table.legal_floor(Role::BorderStrong),
            Some(crate::wcag::AA_UI_RATIO)
        );
        // No legal floor for the decorative / JND / zero contracts.
        assert_eq!(table.legal_floor(Role::LabelQuaternary), None);
        assert_eq!(table.legal_floor(Role::Separator), None);
        assert_eq!(table.legal_floor(Role::BorderNone), None);

        // The contract holds: every resolved anchored role clears its own legal
        // floor against the live background (modulo the solver's own quantised
        // tie-tolerance), so the value a runtime clamps to is real, not aspirational.
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for bg_hex in ["#FFFFFF", "#1C1C1E", "#3478F6", "#7F7F7F"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                for (role, resolved) in resolve_set(&bg, &table, &vc) {
                    let (Some(floor), Some(solved)) = (table.legal_floor(role), resolved.solved())
                    else {
                        continue;
                    };
                    assert!(
                        solved.wcag_ratio() >= floor - 0.05,
                        "{role:?} on {bg_hex}: wcag {} below legal floor {floor}",
                        solved.wcag_ratio()
                    );
                }
            }
        }
    }

    #[test]
    fn recheck_against_batch_matches_per_pair_and_the_solver() {
        // The batch recheck (shared bg) must give exactly the same (lc, wcag) as
        // the single-pair `measure_contrast`, and both equal the solver's own
        // reported values for a freshly-resolved set.
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
                    assert!(
                        (batch[i].1 - s.wcag_ratio()).abs() < 1e-9,
                        "{bg_hex}: wcag != solver"
                    );
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

            // Defense in depth для crate-internal validated-пути: если его
            // инвариант когда-либо будет нарушен, резолв не вернёт частичный
            // правдоподобный набор.
            let bypassed = NamedRoleTable::from_validated_parts(entries(), Vec::new(), chroma);
            let outcomes = resolve_named_set(&bg, &bypassed, &vc);
            assert_eq!(
                outcomes
                    .iter()
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>(),
                ["first", "second"]
            );
            assert!(
                outcomes.iter().all(|(_, outcome)| matches!(
                    outcome,
                    Resolved::Failure(SolveFailure::InvalidInput(_))
                )),
                "глобальная ошибка не должна оставлять частично решённые роли: {outcomes:?}"
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
            RoleChroma::Curve {
                canonical_hue_deg: 0.0,
                target_mp: f64::MIN_POSITIVE,
                hue_stiffness: 0.0,
            },
        ] {
            assert!(
                resolve_valid(chroma)
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
        // The reproducible sweep (`examples/tint_target_sweep.rs`, real engine) measures
        // RMS-argmin t* = 6.01 and, at 6.1, RMS 0.358 / max per-node 0.649 M' on the
        // current engine — a broad, flat minimum across 6.0-6.2. (An earlier note read
        // "residual ≈ 0.90 M'"; that figure is superseded by this measurement.)
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
        crate::wcag::relative_luminance(bg_display(&bg))
    }

    /// The FROZEN pre-CH-4a tie-break — larger symmetric WCAG margin wins, exact
    /// tie to dark-on-light. Kept as the emission-diff ORACLE so the sweep can
    /// enumerate exactly which backgrounds the derivation moved. Do not "improve":
    /// it is a fixed historical reference (what `main` emits), not live policy.
    fn choose_polarity_margin_oracle(bg: &BgInput) -> Polarity {
        let disp = bg_display(bg);
        let dol = wcag::contrast_ratio([0.0, 0.0, 0.0], disp);
        let lod = wcag::contrast_ratio([1.0, 1.0, 1.0], disp);
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
            .ok()
            .and_then(|iv| max_contrast(bg, polarity, vc, *iv).ok());
        let ctx = ResolveContext {
            polarity,
            max_contrast: max,
            interval,
            high_contrast: vc.high_contrast,
        };
        let table = RoleTable::default();
        Role::ALL
            .iter()
            .map(|&r| (r, resolve_in(bg, r, &table, vc, &ctx)))
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
        let d = wcag::contrast_ratio([0.0, 0.0, 0.0], disp);
        let w = wcag::contrast_ratio([1.0, 1.0, 1.0], disp);
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
            let d = wcag::contrast_ratio([0.0, 0.0, 0.0], disp);
            let w = wcag::contrast_ratio([1.0, 1.0, 1.0], disp);
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
            let d = wcag::contrast_ratio([0.0, 0.0, 0.0], disp);
            let w = wcag::contrast_ratio([1.0, 1.0, 1.0], disp);
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
            Resolved::Color { solved, .. } => solved.lc(),
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
        // never compensated. Symmetry holds on the underlying targets; where the
        // measured light/dark values diverge, it is the WCAG floor lifting the
        // light side (flagged by `floor_override`), never silent drift.
        let vc = ViewingConditions::srgb();
        let white = BgInput::solid("#FFFFFF").unwrap();
        let black = BgInput::solid("#101012").unwrap();
        let table = RoleTable::default();
        // Figma's asymmetric dark anchors — what we deliberately do NOT reproduce.
        let figma_dark_asymmetric: [f64; 4] = [-105.4, -40.9, -26.2, -13.1];

        for (i, role) in TEXT_ORDER.iter().enumerate() {
            let light = match resolve(&white, *role, &table, &vc) {
                Resolved::Color { solved, .. } => solved,
                other => panic!("{}: {other:?}", role.key()),
            };
            let dark = match resolve(&black, *role, &table, &vc) {
                Resolved::Color { solved, .. } => solved,
                other => panic!("{}: {other:?}", role.key()),
            };
            let (light_lc, dark_lc) = (light.lc().abs(), dark.lc().abs());
            // Either the two polarities agree (true symmetry), or the gap is
            // accounted for by the WCAG floor overriding one side.
            let symmetric = (light_lc - dark_lc).abs() <= 1.5;
            let floor_explains = light.floor_override() || dark.floor_override();
            assert!(
                symmetric || floor_explains,
                "{}: light |Lc| {light_lc} vs dark {dark_lc} diverge without a floor override",
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
        let resolved = resolve(&bg, Role::None, &table, &vc);
        assert_eq!(resolved, Resolved::None);
        assert_eq!(resolved.lc(), Some(0.0));
        assert!(resolved.solved().is_none());
    }

    #[test]
    fn text_roles_meet_their_wcag_floor() {
        // Each text/UI role's solved colour clears its conformance floor.
        for (vc, vc_name) in vcs() {
            for bg_hex in ["#FFFFFF", "#101012"] {
                let bg = BgInput::solid(bg_hex).unwrap();
                let table = RoleTable::default();
                for (role, min_ratio) in [
                    (Role::LabelPrimary, 4.5),
                    (Role::LabelSecondary, 4.5),
                    (Role::LabelTertiary, 3.0),
                ] {
                    let solved = match resolve(&bg, role, &table, &vc) {
                        Resolved::Color { solved, .. } => solved,
                        other => panic!("{} {bg_hex}: {other:?}", role.key()),
                    };
                    assert!(
                        solved.wcag_ratio() >= min_ratio - 1e-9,
                        "{vc_name} {bg_hex} {}: ratio {} < {min_ratio}",
                        role.key(),
                        solved.wcag_ratio()
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
            let solved = match resolve(&bg, role, &table, &vc) {
                Resolved::Color { solved, .. } => solved,
                other => panic!("{} expected colour, got {other:?}", role.key()),
            };
            assert!(
                !solved.floor_override(),
                "{}: decorative role must not trip the WCAG floor",
                role.key()
            );
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
                Resolved::Color { solved, .. } => solved,
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

        let base = resolve(&bg, Role::Separator, &default_table, &vc);
        let bumped = resolve(&bg, Role::Separator, &stronger, &vc);
        let (b, s) = (base.lc().unwrap().abs(), bumped.lc().unwrap().abs());
        assert!(s > b, "bumped magnitude must raise |Lc|: {b} -> {s}");
    }

    /// Achieved `|dJ'|` of a single resolved role against `bg_hex` under `vc`.
    fn resolved_dj(bg_hex: &str, role: Role, table: &RoleTable, vc: &ViewingConditions) -> f64 {
        let bg = BgInput::solid(bg_hex).unwrap();
        let solved = match resolve(&bg, role, table, vc) {
            Resolved::Color { solved, .. } => solved,
            other => panic!("{} expected a colour, got {other:?}", role.key()),
        };
        let jp_fg = crate::lcs::LcsColor::from_hex_with_vc(solved.hex(), vc)
            .unwrap()
            .jp;
        let jp_bg = crate::lcs::LcsColor::from_hex_with_vc(bg_hex, vc)
            .unwrap()
            .jp;
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
        // The same role under the dark-theme VC must hold a larger perceived
        // separation than under the light VC — the per-VC anchor selection is real,
        // not a constant. (The owner's dark anchors run ~2.2x the light ones.)
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
            Resolved::Color {
                solved, compressed, ..
            } => {
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
            Resolved::Color { compressed, .. } => {
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
            Resolved::Color { solved, .. } => solved.lc(),
            other => panic!("{other:?}"),
        };
        assert!(
            (p_default - p_custom).abs() > 10.0,
            "override should move primary: {p_default} vs {p_custom}"
        );
        // Secondary unchanged.
        let s_default = solved_lc(&bg, Role::LabelSecondary, &vc);
        let s_custom = match resolve(&bg, Role::LabelSecondary, &custom, &vc) {
            Resolved::Color { solved, .. } => solved.lc(),
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
        // The core invariant of the two-stage polarity: on the whole band, no
        // text/UI role is FloorUnreachable, because the polarity is chosen to be
        // the one that clears the floor. (On solid sRGB the AA floor is always
        // reachable in *some* polarity — there is no background where both black
        // and white text fall below 4.5:1 — so a FloorUnreachable here would be a
        // false negative by construction.)
        for (vc, vc_name) in vcs() {
            for bg_hex in band_hexes() {
                let bg = BgInput::solid(&bg_hex).unwrap();
                let set = resolve_set(&bg, &table_default(), &vc);
                for (role, r) in &set {
                    if let Resolved::Failure(SolveFailure::FloorUnreachable { floor, max_ratio }) =
                        r
                    {
                        panic!(
                            "{vc_name} {bg_hex} {}: false FloorUnreachable (floor {floor}, max {max_ratio})",
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
                    .jp;
                let set = resolve_set(&bg, &table, &vc);
                let no_silent_clip = set.iter().all(|(role, r)| match r {
                    // Свечение не участвует в dJ'-клип-инварианте (не контраст-роль).
                    Resolved::Glow(_) | Resolved::GlowIndeterminate(_) => true,
                    Resolved::Color { solved, .. } => {
                        if matches!(table.spec(*role), RoleSpec::DecorativeDj { .. }) {
                            let jp_fg = crate::lcs::LcsColor::from_hex_with_vc(solved.hex(), &vc)
                                .unwrap()
                                .jp;
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
                    Resolved::Failure(_) => true,
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
            let resolved = resolve(&bg, role, &table, &vc);
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
            .jp;
        let jp_bg = crate::lcs::LcsColor::from_hex_with_vc(bg_hex, vc)
            .unwrap()
            .jp;
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
            let res = resolve_rgba_direct(tint, alpha, &bg, &vc);
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
        let res = resolve_rgba_direct(dark, 0.12, &bg_white, &vc);
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
            matches!(
                internal,
                Resolved::Failure(SolveFailure::InternalInvariant(_))
            ),
            "generated tint drift must fail the enclosing boundary: {internal:?}"
        );

        let rejected = resolve_rgba_direct([0.0, 0.0, 0.0], f64::NAN, &bg, &vc);
        match rejected {
            Resolved::Failure(failure) => {
                let boundary = failure.boundary().expect("public alpha has a wire failure");
                assert_eq!(
                    boundary.category(),
                    crate::solve::SolveFailureCategory::Rejected
                );
                assert_eq!(boundary.code(), "invalid_input");
            }
            other => panic!("public alpha must be rejected, got {other:?}"),
        }
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
        let coerced = resolve_rgba_inverted(dark_solid, 0.05, &white, &vc);
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
        let ok = resolve_rgba_inverted(light_solid, 0.5, &white, &vc);
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
        let quantised = resolve_rgba_inverted(quantised_solid, 0.12, &black, &vc);
        let t_quantised = quantised.translucent().unwrap();
        assert!(!t_quantised.alpha_coerced());
        assert_eq!(t_quantised.alpha().to_bits(), 0.12_f64.to_bits());
        assert_eq!(t_quantised.tint_hex(), "#FF0000");

        // Граница: α=1.0 всегда разрешима (тинт=солид) → флаг false даже для
        // насыщенного солида, который иначе коэрсил бы.
        let full = resolve_rgba_inverted(dark_solid, 1.0, &white, &vc);
        let t_full = full.translucent().expect("α=1.0 тривиально разрешима");
        assert!(
            !t_full.alpha_coerced(),
            "α=1.0 разрешима по построению — коэрсии нет"
        );

        // Прямая лестница (не альфа-аналог) НИКОГДА не коэрсит α.
        let direct = resolve_rgba_direct(dark_solid, 0.12, &white, &vc);
        assert!(
            !direct.translucent().unwrap().alpha_coerced(),
            "прямая rgba-лестница эмитит α как есть — флаг всегда false"
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
            Resolved::Color { solved, .. } => solved,
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
                Resolved::Color { solved, .. } => solved,
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
                        Resolved::Color { solved, .. } => solved,
                        other => panic!("{other:?}"),
                    };
                    let rgb = crate::spaces::srgb::srgb_from_hex(solved.hex()).unwrap();
                    let l = crate::spaces::oklab::srgb_linear_to_oklab(rgb)[0];
                    let mp = crate::lcs::LcsColor::from_hex_with_vc(solved.hex(), &vc)
                        .unwrap()
                        .mp();
                    (r, l, mp)
                })
                .collect();

            // The tint is genuinely present — never a flat zero.
            for (role, _l, mp) in &mps {
                assert!(
                    *mp > TINT_PERCEPTIBLE_MP_FLOOR - 1e-6,
                    "{bg_hex} {}: M' {mp} fell below the perceptibility floor",
                    role.key()
                );
            }

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
            Resolved::Color { solved, .. } => solved.hex().to_uppercase(),
            other => panic!("{other:?}"),
        };
        assert_eq!(grey, "#141414", "neutral override must restore pure grey");
    }

    #[test]
    fn v1_flat_tint_remains_a_valid_opt_in_policy() {
        // The v1 flat-ratio undertone is a decision the owner can still opt into:
        // `RoleChroma::Tinted { hue, ratio }` must keep resolving roles around its
        // fixed hue at a flat fraction of the gamut maximum — lightness-independent,
        // unchanged by the v2 curve default. This pins the additive-API promise: the
        // existing variant stays valid even though the default moved to `Curve`.
        let vc = ViewingConditions::srgb();
        let flat = RoleTable::default().with_chroma(RoleChroma::flat_neutral_tint());
        let bg = BgInput::solid("#FFFFFF").unwrap();
        // Secondary under the flat policy lands cool, around the canonical hue, and
        // carries a flat ratio of the gamut max — its chroma is `RATIO * max_chroma`.
        let [_a, b, chroma, _hue] = resolved_oklab(&bg, Role::LabelSecondary, &flat, &vc);
        assert!(b < 0.0, "flat tint must stay cool (b={b})");
        let solved = match resolve(&bg, Role::LabelSecondary, &flat, &vc) {
            Resolved::Color { solved, .. } => solved,
            other => panic!("{other:?}"),
        };
        let l = crate::spaces::oklab::srgb_linear_to_oklab(
            crate::spaces::srgb::srgb_from_hex(solved.hex()).unwrap(),
        )[0];
        let expected = NEUTRAL_TINT_RATIO * scale::max_chroma(l, NEUTRAL_HUE_DEG);
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
                        Resolved::Failure(_) => "UNREACHABLE".to_string(),
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
                    Resolved::Color { solved, .. } => solved,
                    other => panic!("{other:?}"),
                };
                let g = match resolve(&bg, role, &grey, &vc) {
                    Resolved::Color { solved, .. } => solved,
                    other => panic!("{other:?}"),
                };
                // Where perception governs, the tinted and grey roles target the
                // same Lc and must land within the 1-Lc quantisation budget. Where
                // the WCAG floor drives the result (an AA-floored role), the legal
                // gate — not the perceptual target — sets the colour, and the tint
                // can land on a neighbouring on-grid point that still clears the
                // floor; there the only honest invariant is that both clear it.
                let floor_driven = t.floor_override() || g.floor_override();
                if !floor_driven {
                    assert!(
                        (t.lc().abs() - g.lc().abs()).abs() <= 1.0,
                        "{bg_hex} {}: tint moved a perceptual target (tinted {} vs grey {})",
                        role.key(),
                        t.lc(),
                        g.lc()
                    );
                }
                assert!(
                    t.wcag_ratio() >= min_ratio - 1e-9,
                    "{bg_hex} {}: tinted role fails WCAG floor {min_ratio} ({})",
                    role.key(),
                    t.wcag_ratio()
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
    use super::{CUSP_HALF_WINDOW_DEG, LIGHTNESS_SETTLE, STRICT_STEP, TINT_PERCEPTIBLE_MP_FLOOR};
    use crate::lcs::LcsColor;

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
        let mut prev = crate::lpc::lpc(&grey(0), "#FFFFFF");
        for i in 1u8..=255 {
            let lc = crate::lpc::lpc(&grey(i), "#FFFFFF");
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

    /// Потолок ахроматического M'-шума CAM16 (серые #000..#FFF, дефолтный VC)
    /// отслеживается порогом TINT_PERCEPTIBLE_MP_FLOOR (1.5). ЗАМЕР: максимум —
    /// у белого, M'≈1.53; порог 1.5 стоит вплотную ПОД ним. Полоса характеризации
    /// широкая, брекетит замер, без подгонки под 1.5.
    #[test]
    fn tint_floor_tracks_achromatic_mp_noise_ceiling() {
        let mut max_mp = 0.0f64;
        for i in 0u8..=255 {
            let mp = LcsColor::from_hex(&grey(i)).expect("valid grey hex").mp();
            max_mp = max_mp.max(mp);
        }
        assert!(
            (1.4..1.7).contains(&max_mp),
            "потолок M'-шума серых {max_mp:.4} вне замеренного диапазона [1.4, 1.7)"
        );
        // Направленный ассерт (не |Δ|): порог стоит вплотную ПОД потолком шума —
        // floor < max_mp И зазор < 0.15. Инверсия направления (floor над потолком)
        // ломает тест.
        assert!(
            TINT_PERCEPTIBLE_MP_FLOOR < max_mp && max_mp - TINT_PERCEPTIBLE_MP_FLOOR < 0.15,
            "TINT_PERCEPTIBLE_MP_FLOOR={TINT_PERCEPTIBLE_MP_FLOOR} должен стоять чуть ПОД потолком \
             M'-шума {max_mp:.4} (floor < max_mp и max_mp − floor < 0.15)"
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
    use super::{
        DECORATIVE_FLOOR_MIN, IC_DECORATIVE_FLOOR_MIN, QUANT_GUARD, STRICT_STEP,
        TINT_PERCEPTIBLE_MP_FLOOR,
    };
    use crate::exposure_support::{band_exposure, mp_srgb};

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

    /// EXPOSURE TINT_PERCEPTIBLE_MP_FLOOR: доля гаммы с M' в +-50% полосе вокруг
    /// порога перцептируемости тинта — цвета, чья классификация «ощущаемый тон vs
    /// мёртвая серость» зависит от точного значения.
    #[test]
    fn exposure_tint_perceptible_mp_floor() {
        let (lo, hi) = (
            0.5 * TINT_PERCEPTIBLE_MP_FLOOR,
            1.5 * TINT_PERCEPTIBLE_MP_FLOOR,
        );
        let (grid_pct, labui) = band_exposure(|c| mp_srgb(c, false), lo, hi);
        eprintln!(
            "EXPOSURE TINT_PERCEPTIBLE_MP_FLOOR band=[{lo:.2},{hi:.2}] grid_flip={grid_pct:.2}% labui_in_zone={} {:?}",
            labui.len(),
            labui
        );
    }

    /// EXPOSURE STRICT_STEP: доля соседних Lc-шагов 8-бит серой сетки в +-20% полосе
    /// вокруг границы квантования — где точный STRICT_STEP решает «на сетке / нет».
    #[test]
    fn exposure_strict_step() {
        let greys: Vec<f64> = (0u16..=255)
            .map(|i| crate::lpc::lpc(&format!("#{i:02X}{i:02X}{i:02X}", i = i as u8), "#FFFFFF"))
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
    use super::{
        NEUTRAL_HUE_DEG, NEUTRAL_TINT_RATIO, TINT_HUE_STIFFNESS, build_curve_color,
        cusp_attracted_hue,
    };
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
        let ls = [0.15_f64, 0.31, 0.48, 0.58, 0.68, 0.78, 0.86];
        let mut max_spread = 0.0_f64;
        let mut max_wide = 0.0_f64;
        for &l in &ls {
            let base = build_curve_color(l, NEUTRAL_HUE_DEG, NEUTRAL_TINT_RATIO);
            for h in [285.78_f64, 285.90, 286.01] {
                max_spread =
                    max_spread.max(de_ok(base, build_curve_color(l, h, NEUTRAL_TINT_RATIO)));
            }
            for h in [266.0_f64, 276.0, 296.0, 306.0] {
                max_wide = max_wide.max(de_ok(base, build_curve_color(l, h, NEUTRAL_TINT_RATIO)));
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

    /// (e) DESIGN-CHOICE sensitivity-лок для `NEUTRAL_TINT_RATIO`. Свип легальной
    /// полосы [0, 0.20] по светлотам шкалы: непрерывный МАТЕРИАЛЬНЫЙ дрейф (ratio
    /// прямо масштабирует хрому), значит (e), не (c). КУСАЕТСЯ: value-пин `== 0.10`
    /// падает на любой мутации.
    #[test]
    fn neutral_tint_ratio_sensitivity_is_bounded() {
        assert_eq!(NEUTRAL_TINT_RATIO, 0.10, "коэффициент подтона");
        let ls = [0.15_f64, 0.31, 0.48, 0.58, 0.68, 0.78, 0.86];
        let mut max_de = 0.0_f64;
        for &l in &ls {
            let base = build_curve_color(l, NEUTRAL_HUE_DEG, NEUTRAL_TINT_RATIO);
            for r in [0.0_f64, 0.05, 0.08, 0.12, 0.15, 0.20] {
                max_de = max_de.max(de_ok(base, build_curve_color(l, NEUTRAL_HUE_DEG, r)));
            }
        }
        assert!(
            (0.015..0.05).contains(&max_de),
            "max ΔE_ok по [0,0.2] {max_de:.4} вне замеренного [0.015, 0.05) — ручка материальна (e)"
        );
        eprintln!(
            "WAVE2 NEUTRAL_TINT_RATIO (e): max ΔE_ok[0,0.2]={max_de:.4} (материальна → (e), не (c))"
        );
    }
}
