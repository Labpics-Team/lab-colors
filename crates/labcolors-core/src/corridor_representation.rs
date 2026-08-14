//! Точная corridor-композиция: выведенная плотность полупрозрачного слоя над
//! осепараллельным коробом достижимых подложек.
//!
//! # Закон
//!
//! Один тон `T` эмитится как полупрозрачный слой при плотности `α` над
//! неизвестной подложкой из объявленного корридора `[min, max]³`. Единственная
//! решаемая величина — `α`: она выбирается детерминированной бисекцией с
//! фиксированным числом шагов так, чтобы слой над ХУДШЕЙ разрешённой подложкой
//! оставался в контракте читаемости коммит-полюса (ахроматический полюс
//! максимального контраста на `T`).
//!
//! Композит исполняется в официальном byte-scale affine order
//! `(B_byte + α·(T_byte−B_byte))/255`. Алгебраическая монотонность этой формулы
//! недостаточна для binary64: округление может опустить interior background на
//! один ULP ниже обоих endpoint-значений. Кроме того, frozen legacy WCAG 2.1
//! (2018) split `0.03928` имеет малый downward seam. Поэтому ядро строит
//! поканальный conservative envelope фактического композитора, включает обе
//! стороны пересечённого EOTF seam и только затем складывает положительно
//! взвешенные channel ranges. Это all-backdrop characterization выбранного
//! state, а не ложная теорема о двух углах.
//!
//! На поддерживаемом направленном домене солвер применяет детерминированную
//! бисекцию с фиксированным числом шагов и повторно проверяет обе стороны
//! найденного fail/pass bracket. Legacy EOTF seam опровергает глобальную
//! монотонность, поэтому результат не называется первым или минимальным. На
//! `α = 1` полоса вырождается в `L(T)`, а полюс максимального контраста на
//! ЛЮБОМ тоне даёт ≥ 4.58:1 (кроссовер чёрного и белого полюса при `L ≈ 0.179`),
//! поэтому для порога ≤ 4.5 годная `α ∈ (0, 1]` существует всегда; при более
//! высоком пороге честно возвращается `α = 1` с типизированным статусом
//! деградации — не молчание.
//!
//! # Пространство
//!
//! Композит — гамма-кодированный sRGB reference-профиля [`crate::alpha`],
//! заземлённого 12 Figma-парами, но не выдаваемого за любой браузерный
//! pipeline. WCAG-светлота меряется на кодированном тоне (квантованном до
//! 8-битного hex — эмитируемое значение), композит над подложкой берётся без
//! промежуточного переквантования тем же byte-scale affine порядком
//! binary64-операций, что официальный потребитель. Применимость к рендереру и
//! управлению цветом принадлежит отдельному conformance-гейту.

use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};
use crate::wcag::{ratio_from_luminances, relative_luminance, relative_luminance_range};
use std::fmt;

pub(crate) const CORRIDOR_ALPHA_BISECTION_ITERATIONS: u32 = 60;

/// Absolute encoded-channel padding around the two actual endpoint evaluations
/// of the byte-scale affine compositor.
///
/// With all numerator-scale operands bounded by 255, a binary64 forward-error
/// accounting bounds one evaluation by
/// `E = 5·2^-46/255 + 2^-53 < 3.90e-16`. Comparing an interior evaluation with
/// an evaluated endpoint therefore costs `2E`; the outward padding operation
/// can lose at most another `2^-53` to rounding. The combined requirement is
/// `< 8.91e-16`. `8·EPSILON = 2^-49` is exactly `816/409 ≈ 1.995` times that
/// requirement while remaining an absolute bound near zero. This covers
/// compositor rounding only; WCAG `powf` остаётся legacy-platform-dependent
/// (immutable attestation отсутствует до #258).
#[cfg(test)]
pub(crate) const CORRIDOR_COMPOSITE_SINGLE_EVALUATION_ERROR_BOUND: f64 =
    5.0 * (64.0 * f64::EPSILON) / 255.0 + 0.5 * f64::EPSILON;
#[cfg(test)]
pub(crate) const CORRIDOR_COMPOSITE_PAIRWISE_ERROR_BOUND: f64 =
    2.0 * CORRIDOR_COMPOSITE_SINGLE_EVALUATION_ERROR_BOUND;
#[cfg(test)]
pub(crate) const CORRIDOR_COMPOSITE_OUTWARD_ROUNDING_BOUND: f64 = 0.5 * f64::EPSILON;
const CORRIDOR_COMPOSITE_RANGE_MARGIN: f64 = 8.0 * f64::EPSILON;

/// Ахроматический полюс коммит-лейбла поверхности: цвет максимального контраста
/// на тоне (белый на тёмном тоне, чёрный на светлом). Параметр-свободный
/// механизм полярности в чистом виде.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pole {
    /// Чёрный лейбл (`L = 0`) — полюс максимального контраста на СВЕТЛОМ тоне.
    Black,
    /// Белый лейбл (`L = 1`) — полюс максимального контраста на ТЁМНОМ тоне.
    White,
}

/// Канал encoded-sRGB в типизированных ошибках валидации.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RgbChannelV1 {
    /// Красный канал.
    Red,
    /// Зелёный канал.
    Green,
    /// Синий канал.
    Blue,
}

impl RgbChannelV1 {
    fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Red,
            1 => Self::Green,
            2 => Self::Blue,
            _ => unreachable!("RGB channel index is internal and bounded by 0..3"),
        }
    }

    /// Стабильный машинный ключ.
    pub fn key(self) -> &'static str {
        match self {
            Self::Red => "red",
            Self::Green => "green",
            Self::Blue => "blue",
        }
    }
}

/// Какая граница [`BackdropBox`] нарушила инвариант encoded-sRGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BackdropBoundV1 {
    /// Поканальный минимум.
    Min,
    /// Поканальный максимум.
    Max,
}

impl BackdropBoundV1 {
    /// Стабильный машинный ключ.
    pub fn key(self) -> &'static str {
        match self {
            Self::Min => "min",
            Self::Max => "max",
        }
    }
}

/// Нарушение домена encoded-sRGB `[0,1]^3`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum EncodedRgbErrorV1 {
    /// Канал равен NaN или бесконечности.
    NonFiniteChannel {
        /// Канал, нарушивший конечный домен.
        channel: RgbChannelV1,
    },
    /// Конечный канал лежит вне `[0,1]`.
    OutOfRangeChannel {
        /// Канал, нарушивший диапазон.
        channel: RgbChannelV1,
        /// Фактическое значение.
        value: f64,
    },
}

impl fmt::Display for EncodedRgbErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteChannel { channel } => {
                write!(f, "encoded-sRGB {} channel is not finite", channel.key())
            }
            Self::OutOfRangeChannel { channel, value } => write!(
                f,
                "encoded-sRGB {} channel is outside [0,1]: {value}",
                channel.key()
            ),
        }
    }
}

impl std::error::Error for EncodedRgbErrorV1 {}

/// Типизированное нарушение инварианта при создании [`BackdropBox`].
// Имена публичных вариантов явно несут нарушенный инвариант канала; удаление
// общего суффикса сделало бы сопоставление с образцом менее самодокументируемым.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum BackdropBoxErrorV1 {
    /// Канал одной из границ равен NaN или бесконечности.
    NonFiniteChannel {
        /// Нарушившая граница.
        bound: BackdropBoundV1,
        /// Нарушивший канал.
        channel: RgbChannelV1,
    },
    /// Конечный канал одной из границ лежит вне `[0,1]`.
    OutOfRangeChannel {
        /// Нарушившая граница.
        bound: BackdropBoundV1,
        /// Нарушивший канал.
        channel: RgbChannelV1,
        /// Фактическое значение.
        value: f64,
    },
    /// `min[channel] > max[channel]`; конструктор не переставляет границы молча.
    ReversedChannel {
        /// Перепутанный канал.
        channel: RgbChannelV1,
        /// Переданный минимум.
        min: f64,
        /// Переданный максимум.
        max: f64,
    },
}

impl fmt::Display for BackdropBoxErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteChannel { bound, channel } => write!(
                f,
                "backdrop {} {} channel is not finite",
                bound.key(),
                channel.key()
            ),
            Self::OutOfRangeChannel {
                bound,
                channel,
                value,
            } => write!(
                f,
                "backdrop {} {} channel is outside [0,1]: {value}",
                bound.key(),
                channel.key()
            ),
            Self::ReversedChannel { channel, min, max } => write!(
                f,
                "backdrop {} channel is reversed: min={min}, max={max}",
                channel.key()
            ),
        }
    }
}

impl std::error::Error for BackdropBoxErrorV1 {}

/// Класс численного профиля, в котором охарактеризована corridor-alpha граница.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum CorridorNumericalProfileV1 {
    /// Byte-scale affine source-over и frozen legacy WCAG 2.1 (2018)
    /// `0.03928`/`powf` в Rust IEEE-754 binary64; `powf` остаётся зависимым от
    /// платформы и toolchain. Canonical WCAG 2.2 принадлежит issue #284.
    EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
}

/// Честный класс численной гарантии выбранной corridor-alpha.
// Суффикс — часть словаря гарантий: каждый текущий вариант является явно
// охарактеризованным исходом, но не доказательством точности или минимальности.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub(crate) enum CorridorAlphaGuaranteeV1 {
    /// Детерминированная бинарная partition с фиксированным числом шагов;
    /// выбранный верхний кандидат повторно прошёл порог, а нижний повторно не
    /// прошёл. Глобальная монотонность, первый passing state и sound-
    /// доказательство межсредовой минимальности не заявляются.
    BisectionBracketCharacterizedV1 {
        /// Число исполненных шагов бисекции.
        iterations: u32,
        /// Последний кандидат, измеренный ниже порога в этом профиле.
        lower_alpha: f64,
        /// Верхняя граница интервала кандидатов, повторно прошедшая порог.
        upper_alpha: f64,
        /// Численный профиль чувствительного к ветвлению WCAG-пути `powf`.
        numerical_profile: CorridorNumericalProfileV1,
    },
    /// Уже прозрачный endpoint держит порог; бисекция не нужна.
    TransparentEndpointCharacterizedV1 {
        /// Численный профиль повторной проверки возвращённого худшего контраста.
        numerical_profile: CorridorNumericalProfileV1,
    },
    /// Даже непрозрачный endpoint не держит порог; endpoint повторно измерен
    /// в указанном профиле и возвращён с типизированным статусом деградации.
    /// Оптимальность вне объявленного endpoint-контракта не заявляется.
    OpaqueEndpointCharacterizedV1 {
        /// Численный профиль чувствительного к ветвлению WCAG-пути `powf`.
        numerical_profile: CorridorNumericalProfileV1,
    },
}

/// Доменный исход решения corridor-alpha; недостижимость не является ошибкой.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum CorridorAlphaStatusV1 {
    /// Выбранная alpha проходит запрошенный порог в объявленном runtime.
    Satisfied,
    /// Порог недостижим даже при alpha 1; возвращается непрозрачный endpoint.
    Degraded,
}

/// Типизированная ошибка corridor-вычислений; без clamp, swap и fallback.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub(crate) enum CorridorSolveErrorV1 {
    /// Тон нарушает домен encoded-sRGB `[0,1]^3`.
    InvalidTone(EncodedRgbErrorV1),
    /// Hex-обёртка получила неканоническую или невалидную строку sRGB.
    InvalidToneHex {
        /// Свидетельство парсера.
        reason: String,
    },
    /// Alpha не конечна или лежит вне `[0,1]`.
    InvalidAlpha {
        /// Отклонённая alpha.
        value: f64,
    },
    /// WCAG-порог не конечен или лежит вне `[1,21]`.
    InvalidThresholdRatio {
        /// Отклонённое отношение.
        value: f64,
    },
    /// Текущий закон бисекции неприменим к этому валидному отношению тона и
    /// короба подложек.
    UnsupportedDirectedSearchRelation {
        /// Коммит-полюс, направленное предусловие которого нарушено.
        pole: Pole,
    },
    /// Предполагавшийся конечным внутренний численный результат вышел из
    /// объявленного домена.
    NumericallyIndeterminate,
}

impl fmt::Display for CorridorSolveErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTone(error) => write!(f, "invalid corridor tone: {error}"),
            Self::InvalidToneHex { reason } => write!(f, "invalid corridor tone hex: {reason}"),
            Self::InvalidAlpha { value } => {
                write!(f, "corridor alpha is outside finite [0,1]: {value}")
            }
            Self::InvalidThresholdRatio { value } => {
                write!(
                    f,
                    "corridor threshold ratio is outside finite [1,21]: {value}"
                )
            }
            Self::UnsupportedDirectedSearchRelation { pole } => write!(
                f,
                "corridor tone/backdrop relation does not support directed search for {pole:?} pole"
            ),
            Self::NumericallyIndeterminate => {
                f.write_str("corridor numerical result left its declared finite domain")
            }
        }
    }
}

impl std::error::Error for CorridorSolveErrorV1 {}

/// Осепараллельный короб достижимых подложек: поканальный минимум и максимум.
///
/// `min`/`max` — кодированные углы `[0,1]³`. Дефолт glass-случая (слой над
/// неизвестной живой подложкой) — [`FULL`](Self::FULL) = `[чёрный, белый]`,
/// худший возможный коридор. Известная область (изображение/градиент под
/// лейблом) — её поканальные экстремумы (обобщение коридора, labui ADR-0004
/// Решение 2). Прямые литералы структуры намеренно недоступны; все публичные
/// экземпляры проходят через [`try_new`](Self::try_new).
///
/// ```compile_fail
/// use labcolors_core::BackdropBox;
/// let _ = BackdropBox { min: [0.0; 3], max: [1.0; 3] };
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BackdropBox {
    min: [f64; 3],
    max: [f64; 3],
}

impl BackdropBox {
    /// Полный коридор `[чёрный, белый]` — слой над неизвестной живой подложкой:
    /// худший возможный диапазон, самая консервативная гарантия.
    pub const FULL: BackdropBox = BackdropBox {
        min: [0.0, 0.0, 0.0],
        max: [1.0, 1.0, 1.0],
    };

    /// Создаёт короб только из конечных границ encoded-sRGB с `min <= max`.
    /// Перепутанные границы отвергаются, а не нормализуются молча.
    pub fn try_new(min: [f64; 3], max: [f64; 3]) -> Result<Self, BackdropBoxErrorV1> {
        validate_backdrop_bound(BackdropBoundV1::Min, min)?;
        validate_backdrop_bound(BackdropBoundV1::Max, max)?;
        for channel_index in 0..3 {
            if min[channel_index] > max[channel_index] {
                return Err(BackdropBoxErrorV1::ReversedChannel {
                    channel: RgbChannelV1::from_index(channel_index),
                    min: min[channel_index],
                    max: max[channel_index],
                });
            }
        }
        Ok(Self { min, max })
    }

    /// Поканально-минимальный (темнейший) угол.
    pub fn min(self) -> [f64; 3] {
        self.min
    }

    /// Поканально-максимальный (светлейший) угол.
    pub fn max(self) -> [f64; 3] {
        self.max
    }
}

/// Результат вывода corridor-alpha: плотность + вердикт гарантии.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CorridorAlphaV1 {
    pub(crate) alpha: f64,
    pub(crate) worst_contrast: f64,
    pub(crate) pole: Pole,
    pub(crate) status: CorridorAlphaStatusV1,
    pub(crate) guarantee: CorridorAlphaGuaranteeV1,
}

/// Единственный валидатор массива encoded-sRGB: конечность и `[0,1]^3`.
fn validate_encoded_rgb(v: [f64; 3]) -> Result<(), EncodedRgbErrorV1> {
    for (channel_index, value) in v.into_iter().enumerate() {
        let channel = RgbChannelV1::from_index(channel_index);
        if !value.is_finite() {
            return Err(EncodedRgbErrorV1::NonFiniteChannel { channel });
        }
        if !(0.0..=1.0).contains(&value) {
            return Err(EncodedRgbErrorV1::OutOfRangeChannel { channel, value });
        }
    }
    Ok(())
}

fn validate_backdrop_bound(
    bound: BackdropBoundV1,
    value: [f64; 3],
) -> Result<(), BackdropBoxErrorV1> {
    validate_encoded_rgb(value).map_err(|error| match error {
        EncodedRgbErrorV1::NonFiniteChannel { channel } => {
            BackdropBoxErrorV1::NonFiniteChannel { bound, channel }
        }
        EncodedRgbErrorV1::OutOfRangeChannel { channel, value } => {
            BackdropBoxErrorV1::OutOfRangeChannel {
                bound,
                channel,
                value,
            }
        }
    })
}

/// Квантовать кодированный цвет до 8-битной сетки дисплея (round-trip через
/// hex — то же представление, в котором браузер отдаёт пиксели и эмитируется
/// слой).
pub(crate) fn quantise(v: [f64; 3]) -> [f64; 3] {
    srgb_encoded_from_hex(&hex_from_srgb_encoded(v))
        .expect("hex собственного форматтера всегда валиден")
}

/// Коммит-полюс поверхности тона: полюс максимального WCAG-контраста на `L(tone)`.
///
/// Белый полюс на тёмном тоне, чёрный на светлом; граница — кроссовер
/// `L ≈ 0.1791` (там оба полюса дают ≈ 4.58:1). Тон квантуется (полюс — свойство
/// ЭМИТИРУЕМОГО тона).
///
/// # Errors
///
/// [`CorridorSolveErrorV1::InvalidTone`] для неконечного канала или канала вне
/// диапазона.
pub(crate) fn committed_pole_encoded(tone: [f64; 3]) -> Result<Pole, CorridorSolveErrorV1> {
    validate_encoded_rgb(tone).map_err(CorridorSolveErrorV1::InvalidTone)?;
    Ok(committed_pole_for_valid_tone(quantise(tone)))
}

fn committed_pole_for_valid_tone(tone_q: [f64; 3]) -> Pole {
    let l = relative_luminance(tone_q);
    // contrast(белый, L) > contrast(чёрный, L) ⇔ 1.05/(L+0.05) > (L+0.05)/0.05
    //                                          ⇔ (L+0.05)² < 0.0525.
    let s = l + 0.05;
    if s * s < 1.05 * 0.05 {
        Pole::White
    } else {
        Pole::Black
    }
}

/// Один канал официального corridor consumer в зафиксированном byte-scale
/// affine operation order.
pub(crate) fn corridor_channel_over_byte_scale(tint_byte: f64, alpha: f64, background: f64) -> f64 {
    let background_byte_scale = background * 255.0;
    (background_byte_scale + alpha * (tint_byte - background_byte_scale)) / 255.0
}

/// Conservative encoded-channel range over an ordered background interval.
///
/// The exact affine reference is monotone in the byte-scale background, but
/// its binary64 evaluation is not. Actual endpoint values are therefore sorted
/// and expanded by [`CORRIDOR_COMPOSITE_RANGE_MARGIN`]. At the opaque endpoint,
/// source-over is the tint identity and is represented exactly without a
/// fictitious uncertainty band.
pub(crate) fn corridor_channel_range(
    tint_byte: f64,
    alpha: f64,
    background_lo: f64,
    background_hi: f64,
) -> (f64, f64) {
    if alpha == 1.0 {
        let tint = tint_byte / 255.0;
        return (tint, tint);
    }
    let at_lo = corridor_channel_over_byte_scale(tint_byte, alpha, background_lo);
    let at_hi = corridor_channel_over_byte_scale(tint_byte, alpha, background_hi);
    let raw_lo = at_lo.min(at_hi);
    let raw_hi = at_lo.max(at_hi);
    (
        (raw_lo - CORRIDOR_COMPOSITE_RANGE_MARGIN).max(0.0),
        (raw_hi + CORRIDOR_COMPOSITE_RANGE_MARGIN).min(1.0),
    )
}

/// Conservative characterized luminance enclosure `[effLow, effHigh]` for the
/// tint over every background in the axis-aligned box.
///
/// Each actual byte-scale channel is enclosed independently; the legacy WCAG
/// profile then includes both sides of a crossed EOTF seam before combining the
/// positive luminance weights. No RGB-corner or global monotonicity theorem is
/// claimed.
fn band_luminance(tint_q: [f64; 3], alpha: f64, backdrop: &BackdropBox) -> (f64, f64) {
    // Source-over at alpha=1 is exactly the emitted tint for every backdrop in
    // this byte-scale profile. Preserve that proven endpoint instead of adding
    // an uncertainty band that would falsely degrade #000/#FFF at threshold 21.
    if alpha == 1.0 {
        let luminance = relative_luminance(tint_q);
        return (luminance, luminance);
    }
    let ranges: [(f64, f64); 3] = core::array::from_fn(|channel| {
        corridor_channel_range(
            (tint_q[channel] * 255.0).round(),
            alpha,
            backdrop.min[channel],
            backdrop.max[channel],
        )
    });
    relative_luminance_range(
        core::array::from_fn(|channel| ranges[channel].0),
        core::array::from_fn(|channel| ranges[channel].1),
    )
}

/// Худший WCAG-контраст ахроматического полюса по достижимой полосе `[lo, hi]`.
/// Для чёрного полюса ближайший endpoint — `lo`, для белого — `hi`.
pub(crate) fn worst_contrast_of_band(pole: Pole, lo: f64, hi: f64) -> f64 {
    match pole {
        Pole::Black => ratio_from_luminances(0.0, lo),
        Pole::White => ratio_from_luminances(1.0, hi),
    }
}

/// Худший WCAG-контраст коммит-полюса слоя `tone` при `alpha` над коридором.
///
/// Тон квантуется (эмитируемое значение); композитный диапазон использует
/// объявленный порядок binary64-операций без промежуточного квантования, затем
/// conservative channel/EOTF enclosure.
///
/// # Errors
///
/// Типизированная [`CorridorSolveErrorV1`] для невалидных tone/alpha либо
/// внутреннего нарушения численного домена. [`BackdropBox`] уже валиден по типу.
pub(crate) fn worst_contrast_encoded(
    tone: [f64; 3],
    alpha: f64,
    backdrop: &BackdropBox,
    pole: Pole,
) -> Result<f64, CorridorSolveErrorV1> {
    validate_encoded_rgb(tone).map_err(CorridorSolveErrorV1::InvalidTone)?;
    if !(0.0..=1.0).contains(&alpha) {
        return Err(CorridorSolveErrorV1::InvalidAlpha { value: alpha });
    }
    let (lo, hi) = band_luminance(quantise(tone), alpha, backdrop);
    let worst = worst_contrast_of_band(pole, lo, hi);
    if !(1.0..=21.0).contains(&worst) {
        return Err(CorridorSolveErrorV1::NumericallyIndeterminate);
    }
    Ok(worst)
}

pub(crate) fn validate_rechecked_bracket(
    lower_contrast: f64,
    upper_contrast: f64,
    threshold_ratio: f64,
) -> Result<(), CorridorSolveErrorV1> {
    if !matches!(
        lower_contrast.partial_cmp(&threshold_ratio),
        Some(core::cmp::Ordering::Less)
    ) {
        return Err(CorridorSolveErrorV1::NumericallyIndeterminate);
    }
    if !matches!(
        upper_contrast.partial_cmp(&threshold_ratio),
        Some(core::cmp::Ordering::Equal | core::cmp::Ordering::Greater)
    ) {
        return Err(CorridorSolveErrorV1::NumericallyIndeterminate);
    }
    Ok(())
}

/// Выбрать проходящую альфу слоя бинарной partition с фиксированным числом
/// шагов и повторно проверить найденные fail/pass endpoints.
///
/// Проверка endpoint, затем детерминированная бинарная partition по
/// `α ∈ [0, 1]` и замер на квантованном тоне. Каждый шаг сохраняет rechecked
/// fail/pass endpoints; глобальная монотонность или первый passing state не
/// заявляются. Возвращённое состояние несёт [`CorridorAlphaGuaranteeV1`]:
/// зависящая от платформы и toolchain функция `powf` не выдаётся за
/// sound-доказательство точной межсредовой минимальной границы. При
/// недостижимости порога даже на `α = 1` возвращается типизированный
/// degraded-исход, а не ошибка или fallback.
///
/// # Errors
///
/// Типизированный невалидный tone/threshold или
/// [`CorridorSolveErrorV1::UnsupportedDirectedSearchRelation`].
/// Путь с [`BackdropBox::FULL`] всегда лежит в направленном домене.
pub(crate) fn solve_corridor_alpha_encoded(
    tone: [f64; 3],
    backdrop: &BackdropBox,
    threshold_ratio: f64,
) -> Result<CorridorAlphaV1, CorridorSolveErrorV1> {
    validate_encoded_rgb(tone).map_err(CorridorSolveErrorV1::InvalidTone)?;
    if !(1.0..=21.0).contains(&threshold_ratio) {
        return Err(CorridorSolveErrorV1::InvalidThresholdRatio {
            value: threshold_ratio,
        });
    }
    let tone_q = quantise(tone);
    let pole = committed_pole_for_valid_tone(tone_q);
    let worst_at = |alpha: f64| -> Result<f64, CorridorSolveErrorV1> {
        let (lo, hi) = band_luminance(tone_q, alpha, backdrop);
        let worst = worst_contrast_of_band(pole, lo, hi);
        if !(1.0..=21.0).contains(&worst) {
            return Err(CorridorSolveErrorV1::NumericallyIndeterminate);
        }
        Ok(worst)
    };

    // Общий закон endpoint: если полностью прозрачный слой уже держит порог,
    // порогового поиска не существует и выдумывать неуспешную нижнюю границу нельзя.
    let transparent = worst_at(0.0)?;
    if transparent >= threshold_ratio {
        return Ok(CorridorAlphaV1 {
            alpha: 0.0,
            worst_contrast: transparent,
            pole,
            status: CorridorAlphaStatusV1::Satisfied,
            guarantee: CorridorAlphaGuaranteeV1::TransparentEndpointCharacterizedV1 {
                numerical_profile:
                    CorridorNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
            },
        });
    }

    // Density search is supported only in the direction that contracts the
    // backdrop interval toward the committed tone: black requires tone >= min,
    // white requires tone <= max. This scopes the selection policy; it is not a
    // claim that the branch-sensitive binary64 predicate is globally monotone.
    // The transparent endpoint above does not require this precondition.
    let directed = match pole {
        Pole::Black => (0..3).all(|c| tone_q[c] >= backdrop.min[c]),
        Pole::White => (0..3).all(|c| tone_q[c] <= backdrop.max[c]),
    };
    if !directed {
        return Err(CorridorSolveErrorV1::UnsupportedDirectedSearchRelation { pole });
    }

    // α = 1: полоса вырождается в L(tone) → худший контраст = контраст полюса
    // на тоне (солид-канон). Если и он ниже порога, возвращаем этот повторно
    // измеренный endpoint с типизированным статусом деградации: гарантия не
    // выполнена.
    let opaque = worst_at(1.0)?;
    if opaque < threshold_ratio {
        return Ok(CorridorAlphaV1 {
            alpha: 1.0,
            worst_contrast: opaque,
            pole,
            status: CorridorAlphaStatusV1::Degraded,
            guarantee: CorridorAlphaGuaranteeV1::OpaqueEndpointCharacterizedV1 {
                numerical_profile:
                    CorridorNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
            },
        });
    }

    // Binary partition with a fixed number of steps: lo is rechecked failing and
    // hi rechecked passing after every update. No global-first/minimum claim is
    // made for the discontinuous legacy predicate.
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..CORRIDOR_ALPHA_BISECTION_ITERATIONS {
        let mid = 0.5 * (lo + hi);
        if worst_at(mid)? >= threshold_ratio {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let worst_contrast = worst_at(hi)?;
    let lower_contrast = worst_at(lo)?;
    validate_rechecked_bracket(lower_contrast, worst_contrast, threshold_ratio)?;
    Ok(CorridorAlphaV1 {
        alpha: hi,
        worst_contrast,
        pole,
        status: CorridorAlphaStatusV1::Satisfied,
        guarantee: CorridorAlphaGuaranteeV1::BisectionBracketCharacterizedV1 {
            iterations: CORRIDOR_ALPHA_BISECTION_ITERATIONS,
            lower_alpha: lo,
            upper_alpha: hi,
            numerical_profile:
                CorridorNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
        },
    })
}

/// Hex-обёртка [`solve_corridor_alpha_encoded`] над полным коридором
/// `[чёрный, белый]` (слой над неизвестной живой подложкой).
///
/// # Errors
///
/// Типизированная [`CorridorSolveErrorV1`] при невалидном hex/threshold.
pub(crate) fn solve_corridor_alpha_hex(
    tone_hex: &str,
    threshold_ratio: f64,
) -> Result<CorridorAlphaV1, CorridorSolveErrorV1> {
    let tone = srgb_encoded_from_hex(tone_hex)
        .map_err(|reason| CorridorSolveErrorV1::InvalidToneHex { reason })?;
    solve_corridor_alpha_encoded(tone, &BackdropBox::FULL, threshold_ratio)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wcag::relative_luminance;

    /// Frozen legacy EOTF имеет downward seam, поэтому даже направленный
    /// corridor path нельзя объявлять глобально монотонным по alpha.
    #[test]
    fn legacy_eotf_seam_rejects_global_alpha_monotonicity() {
        let alpha = 0.039_28_f64;
        let next_alpha = f64::from_bits(alpha.to_bits() + 1);
        let at = corridor_channel_over_byte_scale(255.0, alpha, 0.0);
        let after = corridor_channel_over_byte_scale(255.0, next_alpha, 0.0);
        assert_eq!(at.to_bits(), alpha.to_bits());
        assert_eq!(after.to_bits(), next_alpha.to_bits());

        let before_luminance = relative_luminance([at; 3]);
        let after_luminance = relative_luminance([after; 3]);
        assert!(
            after_luminance < before_luminance,
            "legacy seam witness stopped rejecting global monotonicity"
        );
    }

    #[test]
    fn rechecked_bracket_postcondition_rejects_each_invalid_side_independently() {
        assert!(validate_rechecked_bracket(2.99, 3.0, 3.0).is_ok());
        for (lower, upper) in [
            (3.0, 3.0),
            (3.01, 3.0),
            (f64::NAN, 3.0),
            (2.99, 2.99),
            (2.99, f64::NAN),
        ] {
            assert_eq!(
                validate_rechecked_bracket(lower, upper, 3.0),
                Err(CorridorSolveErrorV1::NumericallyIndeterminate),
                "lower={lower}, upper={upper}",
            );
        }
    }

    #[test]
    fn channel_range_encloses_known_binary64_interior_jitter() {
        let alpha = f64::from_bits(1.0_f64.to_bits() - 1);
        let tint_byte = 2.0;
        let interior_byte_scale = 0.137_643_568_434_543_95;
        let interior_background = interior_byte_scale / 255.0;
        let actual = corridor_channel_over_byte_scale(tint_byte, alpha, interior_background);
        let endpoint_min = corridor_channel_over_byte_scale(tint_byte, alpha, 0.0)
            .min(corridor_channel_over_byte_scale(tint_byte, alpha, 1.0));
        assert!(
            actual < endpoint_min,
            "fixture must reject endpoint-only affine monotonicity"
        );

        let (lo, hi) = corridor_channel_range(tint_byte, alpha, 0.0, 1.0);
        assert!(
            lo <= actual && actual <= hi,
            "{actual} outside [{lo}, {hi}]"
        );
    }

    #[test]
    fn composite_range_margin_dominates_pairwise_error_and_outward_rounding() {
        let single = std::hint::black_box(CORRIDOR_COMPOSITE_SINGLE_EVALUATION_ERROR_BOUND);
        let pairwise = std::hint::black_box(CORRIDOR_COMPOSITE_PAIRWISE_ERROR_BOUND);
        let outward = std::hint::black_box(CORRIDOR_COMPOSITE_OUTWARD_ROUNDING_BOUND);
        let margin = std::hint::black_box(CORRIDOR_COMPOSITE_RANGE_MARGIN);
        assert!(single < 3.90e-16);
        assert_eq!(pairwise, 2.0 * single);
        let required = pairwise + outward;
        assert!(margin > required);
        assert!(margin > 1.9 * required);
    }

    #[test]
    fn scalar_compositor_can_decrease_across_adjacent_backgrounds_but_range_encloses_both() {
        let tint_byte = 228.0;
        let alpha = 0.618_654_751_765_414_3;
        let byte_scale = 73.103_401_561_508_89_f64;
        let next_byte_scale = f64::from_bits(byte_scale.to_bits() + 1);
        let background = byte_scale / 255.0;
        let next_background = next_byte_scale / 255.0;
        let at = corridor_channel_over_byte_scale(tint_byte, alpha, background);
        let next = corridor_channel_over_byte_scale(tint_byte, alpha, next_background);
        assert!(
            next < at,
            "fixture must reject binary64 background monotonicity"
        );

        let (lo, hi) = corridor_channel_range(tint_byte, alpha, background, next_background);
        assert!(lo <= at && at <= hi);
        assert!(lo <= next && next <= hi);
    }

    #[test]
    fn opaque_endpoint_is_exact_tint_identity() {
        for tint_byte in [0.0, 1.0, 2.0, 127.0, 228.0, 254.0, 255.0] {
            let expected = tint_byte / 255.0;
            let (lo, hi) = corridor_channel_range(tint_byte, 1.0, 0.0, 1.0);
            assert_eq!(lo.to_bits(), expected.to_bits());
            assert_eq!(hi.to_bits(), expected.to_bits());
            for background in [0.0, f64::EPSILON, 0.137_643_568_434_543_95, 0.5, 1.0] {
                assert_eq!(
                    corridor_channel_over_byte_scale(tint_byte, 1.0, background).to_bits(),
                    expected.to_bits(),
                    "tint={tint_byte}, background={background}"
                );
            }
        }
    }
}
