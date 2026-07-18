//! Двухслойный материал: опаковая тон-база + полупрозрачный тинт с ВЫВЕДЕННОЙ
//! альфой (композит-гарантия над коридором фонов).
//!
//! # Модель
//!
//! Материальная поверхность (стекло/акрил) — полупрозрачный тинт `01` над
//! непрозрачной базой `02`. В СОЛИД-режиме видно `01`-над-`02`; в GLASS-режиме
//! база отброшена и `01` лежит над ЖИВЫМ, авторски неизвестным фоном
//! (`backdrop-filter`). База и тинт — ОДИН тон `T` (семейно-оттеночный опаковый
//! цвет на целевой светлоте тира): база = `T` непрозрачна, тинт = `T` при альфе
//! `α`. Тогда солид-канон `01`-над-`02` = `α·T + (1−α)·T = T` — байт-точно при
//! любой `α` (композит `T` над `T` есть `T`). Единственная РЕШАЕМАЯ величина — `α`.
//!
//! # Выведенная альфа (не рукописная)
//!
//! `α` — проходящая плотность, выбранная бисекцией с фиксированным числом шагов
//! в объявленном охарактеризованном для платформы runtime: тинт над ХУДШИМ
//! разрешённым фоном остаётся в
//! контракте базы. Коммит-лейбл поверхности (ахроматический полюс
//! максимального контраста на тоне `T` — белый на тёмном `T`, чёрный на светлом)
//! держит пол читаемости ПО ВСЕМУ коридору достижимых фонов.
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
//! На поддерживаемом направленном домене material-солвер применяет детерминированную
//! бисекцию с фиксированным числом шагов и повторно проверяет обе стороны
//! найденного fail/pass bracket. Legacy EOTF seam опровергает глобальную
//! монотонность, поэтому результат не называется первым или минимальным. На
//! `α = 1` полоса вырождается в `L(T)`, а
//! полюс максимального контраста на ЛЮБОМ тоне даёт ≥ 4.58:1 (кроссовер чёрного и
//! белого полюса при `L ≈ 0.179`), поэтому для пола ≤ AA годная `α ∈ (0, 1]`
//! существует всегда; при более высоком поле (напр. AAA на среднем тоне)
//! честно возвращается `α = 1` с флагом [`MaterialAlpha::degraded`] — не молчание.
//!
//! # Пространство
//!
//! Композит — гамма-кодированный sRGB reference-профиля [`crate::alpha`],
//! заземлённого 12 Figma-парами, но не выдаваемого за любой браузерный pipeline. WCAG-
//! светлота меряется на кодированном тоне-тинте (квантованном до 8-битного hex —
//! эмитируемый цвет `01`), композит над фоном берётся без промежуточного
//! переквантования как `(B_byte + α·(T_byte−B_byte))/255`: тем же byte-scale
//! affine порядком binary64-операций, что официальный потребитель. Потребитель
//! может независимо воспроизвести point-композиты; all-backdrop verdict ядра
//! дополнительно использует описанный conservative channel/EOTF envelope и не
//! равен одному raw point-замеру. Применимость к рендереру и управлению цветом
//! принадлежит отдельному conformance-гейту.

use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex};
use crate::wcag::{ratio_from_luminances, relative_luminance, relative_luminance_range};
use std::fmt;

const MATERIAL_ALPHA_BISECTION_ITERATIONS: u32 = 60;

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
const MATERIAL_COMPOSITE_SINGLE_EVALUATION_ERROR_BOUND: f64 =
    5.0 * (64.0 * f64::EPSILON) / 255.0 + 0.5 * f64::EPSILON;
#[cfg(test)]
const MATERIAL_COMPOSITE_PAIRWISE_ERROR_BOUND: f64 =
    2.0 * MATERIAL_COMPOSITE_SINGLE_EVALUATION_ERROR_BOUND;
#[cfg(test)]
const MATERIAL_COMPOSITE_OUTWARD_ROUNDING_BOUND: f64 = 0.5 * f64::EPSILON;
const MATERIAL_COMPOSITE_RANGE_MARGIN: f64 = 8.0 * f64::EPSILON;

/// Ахроматический полюс коммит-лейбла поверхности: цвет максимального контраста
/// на тоне (белый на тёмном тоне, чёрный на светлом). Механизм полярности
/// внутреннего `pair`/`commitLabel` в чистом, параметр-свободном виде.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pole {
    /// Чёрный лейбл (`L = 0`) — полюс максимального контраста на СВЕТЛОМ тоне.
    Black,
    /// Белый лейбл (`L = 1`) — полюс максимального контраста на ТЁМНОМ тоне.
    White,
}

impl Pole {
    /// WCAG-светлота полюса: `0.0` (чёрный) / `1.0` (белый).
    #[cfg(test)]
    fn luminance(self) -> f64 {
        match self {
            Pole::Black => 0.0,
            Pole::White => 1.0,
        }
    }
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

/// Класс численного профиля, в котором охарактеризована граница material-alpha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MaterialNumericalProfileV1 {
    /// Byte-scale affine source-over и frozen legacy WCAG 2.1 (2018)
    /// `0.03928`/`powf` в Rust IEEE-754 binary64; `powf` остаётся зависимым от
    /// платформы и toolchain. Canonical WCAG 2.2 принадлежит issue #284.
    EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
}

impl MaterialNumericalProfileV1 {
    /// Стабильный wire-ключ.
    pub fn key(self) -> &'static str {
        match self {
            Self::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1 => {
                "encoded-srgb-byte-scale-affine-platform-binary64-powf-v1"
            }
        }
    }
}

/// Честный класс численной гарантии выбранной material alpha.
// Суффикс — часть публичного словаря гарантий: каждый текущий вариант является
// явно охарактеризованным исходом, но не доказательством точности или минимальности.
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum MaterialAlphaGuaranteeV1 {
    /// Детерминированная бинарная partition с фиксированным числом шагов;
    /// выбранный верхний кандидат повторно прошёл floor, а нижний повторно не
    /// прошёл. Глобальная монотонность, первый passing state и sound-
    /// доказательство межсредовой минимальности не заявляются.
    BisectionBracketCharacterizedV1 {
        /// Число исполненных шагов бисекции.
        iterations: u32,
        /// Последний кандидат, измеренный ниже floor в этом профиле.
        lower_alpha: f64,
        /// Верхняя граница интервала кандидатов, повторно прошедшая floor.
        upper_alpha: f64,
        /// Численный профиль чувствительного к ветвлению WCAG-пути `powf`.
        numerical_profile: MaterialNumericalProfileV1,
    },
    /// Уже прозрачный endpoint держит floor; бисекция не нужна.
    TransparentEndpointCharacterizedV1 {
        /// Численный профиль повторной проверки возвращённого худшего контраста.
        numerical_profile: MaterialNumericalProfileV1,
    },
    /// Даже непрозрачный endpoint не держит floor; endpoint повторно измерен
    /// в указанном профиле и возвращён с типизированным статусом `Degraded`.
    /// Оптимальность вне объявленного endpoint-контракта не заявляется.
    OpaqueEndpointCharacterizedV1 {
        /// Численный профиль чувствительного к ветвлению WCAG-пути `powf`.
        numerical_profile: MaterialNumericalProfileV1,
    },
}

impl MaterialAlphaGuaranteeV1 {
    /// Стабильный wire-ключ.
    pub fn key(self) -> &'static str {
        match self {
            Self::BisectionBracketCharacterizedV1 { .. } => "bisection-bracket-characterized-v1",
            Self::TransparentEndpointCharacterizedV1 { .. } => {
                "transparent-endpoint-characterized-v1"
            }
            Self::OpaqueEndpointCharacterizedV1 { .. } => "opaque-endpoint-characterized-v1",
        }
    }

    /// Runtime-класс, неразделимо связанный с гарантией.
    pub fn numerical_profile(self) -> MaterialNumericalProfileV1 {
        match self {
            Self::BisectionBracketCharacterizedV1 {
                numerical_profile, ..
            }
            | Self::TransparentEndpointCharacterizedV1 { numerical_profile }
            | Self::OpaqueEndpointCharacterizedV1 { numerical_profile } => numerical_profile,
        }
    }

    /// Фиксированное число шагов бинарной partition для bracket-исхода.
    pub fn iterations(self) -> Option<u32> {
        match self {
            Self::BisectionBracketCharacterizedV1 { iterations, .. } => Some(iterations),
            Self::TransparentEndpointCharacterizedV1 { .. }
            | Self::OpaqueEndpointCharacterizedV1 { .. } => None,
        }
    }

    /// Охарактеризованные нижний и верхний кандидаты только для настоящего
    /// порогового интервала.
    pub fn bracket(self) -> Option<(f64, f64)> {
        match self {
            Self::BisectionBracketCharacterizedV1 {
                lower_alpha,
                upper_alpha,
                ..
            } => Some((lower_alpha, upper_alpha)),
            Self::TransparentEndpointCharacterizedV1 { .. }
            | Self::OpaqueEndpointCharacterizedV1 { .. } => None,
        }
    }
}

/// Доменный исход решения material-alpha; недостижимость не является ошибкой.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MaterialAlphaStatusV1 {
    /// Выбранная alpha проходит запрошенный floor в объявленном runtime.
    Satisfied,
    /// Floor недостижим даже при alpha 1; возвращается непрозрачный endpoint.
    Degraded,
}

impl MaterialAlphaStatusV1 {
    /// Стабильный wire-ключ.
    pub fn key(self) -> &'static str {
        match self {
            Self::Satisfied => "satisfied",
            Self::Degraded => "degraded",
        }
    }
}

/// Типизированная публичная ошибка material-вычислений; без clamp, swap и fallback.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MaterialSolveErrorV1 {
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
    /// WCAG floor не конечен или лежит вне `[1,21]`.
    InvalidFloorRatio {
        /// Отклонённое отношение.
        value: f64,
    },
    /// Текущий закон бисекции неприменим к этому валидному отношению тона и фона.
    UnsupportedDirectedSearchRelation {
        /// Коммит-полюс, направленное предусловие которого нарушено.
        pole: Pole,
    },
    /// Предполагавшийся конечным внутренний численный результат вышел из
    /// объявленного домена.
    NumericallyIndeterminate,
}

impl fmt::Display for MaterialSolveErrorV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidTone(error) => write!(f, "invalid material tone: {error}"),
            Self::InvalidToneHex { reason } => write!(f, "invalid material tone hex: {reason}"),
            Self::InvalidAlpha { value } => {
                write!(f, "material alpha is outside finite [0,1]: {value}")
            }
            Self::InvalidFloorRatio { value } => {
                write!(f, "material floor ratio is outside finite [1,21]: {value}")
            }
            Self::UnsupportedDirectedSearchRelation { pole } => write!(
                f,
                "material tone/backdrop relation does not support directed search for {pole:?} pole"
            ),
            Self::NumericallyIndeterminate => {
                f.write_str("material numerical result left its declared finite domain")
            }
        }
    }
}

impl std::error::Error for MaterialSolveErrorV1 {}

/// Осепараллельный короб достижимых фонов: поканальный минимум и максимум.
///
/// `min`/`max` — кодированные углы `[0,1]³`. Материальный дефолт (стекло над
/// неизвестным живым фоном) — [`FULL`](Self::FULL) = `[чёрный, белый]`, худший
/// возможный коридор. Известная область (изображение/градиент под лейблом) — её
/// поканальные экстремумы (обобщение коридора, labui ADR-0004 Решение 2).
/// Прямые литералы структуры намеренно недоступны; все публичные экземпляры
/// проходят через [`try_new`](Self::try_new).
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
    /// Полный коридор `[чёрный, белый]` — материальный случай (неизвестный живой
    /// фон): худший возможный диапазон, самая консервативная гарантия.
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

/// Результат вывода альфы материала: плотность + вердикт гарантии.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialAlpha {
    alpha: f64,
    worst_contrast: f64,
    pole: Pole,
    status: MaterialAlphaStatusV1,
    guarantee: MaterialAlphaGuaranteeV1,
}

impl MaterialAlpha {
    /// Выбранная alpha тинта `01`, `[0,1]`.
    pub fn alpha(self) -> f64 {
        self.alpha
    }

    /// Повторно вычисленный худший контраст выбранного состояния.
    pub fn worst_contrast(self) -> f64 {
        self.worst_contrast
    }

    /// Коммит-полюс поверхности.
    pub fn pole(self) -> Pole {
        self.pole
    }

    /// Доменный исход satisfied/degraded.
    pub fn status(self) -> MaterialAlphaStatusV1 {
        self.status
    }

    /// Класс численного свидетельства; не смешивается с утверждением точности
    /// композитора.
    pub fn guarantee(self) -> MaterialAlphaGuaranteeV1 {
        self.guarantee
    }

    /// Предикат совместимости поверх типизированного статуса.
    pub fn degraded(self) -> bool {
        self.status == MaterialAlphaStatusV1::Degraded
    }
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

/// Квантовать кодированный цвет до 8-битной сетки дисплея (round-trip через hex —
/// то же представление, в котором браузер отдаёт пиксели и эмитируется `01`).
fn quantise(v: [f64; 3]) -> [f64; 3] {
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
/// [`MaterialSolveErrorV1::InvalidTone`] для неконечного канала или канала вне диапазона.
pub fn committed_pole_encoded(tone: [f64; 3]) -> Result<Pole, MaterialSolveErrorV1> {
    validate_encoded_rgb(tone).map_err(MaterialSolveErrorV1::InvalidTone)?;
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

/// Один канал официального material consumer в зафиксированном byte-scale
/// affine operation order.
fn material_channel_over_byte_scale(tint_byte: f64, alpha: f64, background: f64) -> f64 {
    let background_byte_scale = background * 255.0;
    (background_byte_scale + alpha * (tint_byte - background_byte_scale)) / 255.0
}

/// Conservative encoded-channel range over an ordered background interval.
///
/// The exact affine reference is monotone in the byte-scale background, but
/// its binary64 evaluation is not. Actual endpoint values are therefore sorted
/// and expanded by [`MATERIAL_COMPOSITE_RANGE_MARGIN`]. At the opaque endpoint,
/// source-over is the tint identity and is represented exactly without a
/// fictitious uncertainty band.
fn material_channel_range(
    tint_byte: f64,
    alpha: f64,
    background_lo: f64,
    background_hi: f64,
) -> (f64, f64) {
    if alpha == 1.0 {
        let tint = tint_byte / 255.0;
        return (tint, tint);
    }
    let at_lo = material_channel_over_byte_scale(tint_byte, alpha, background_lo);
    let at_hi = material_channel_over_byte_scale(tint_byte, alpha, background_hi);
    let raw_lo = at_lo.min(at_hi);
    let raw_hi = at_lo.max(at_hi);
    (
        (raw_lo - MATERIAL_COMPOSITE_RANGE_MARGIN).max(0.0),
        (raw_hi + MATERIAL_COMPOSITE_RANGE_MARGIN).min(1.0),
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
    // an uncertainty band that would falsely degrade #000/#FFF at floor 21.
    if alpha == 1.0 {
        let luminance = relative_luminance(tint_q);
        return (luminance, luminance);
    }
    let ranges: [(f64, f64); 3] = core::array::from_fn(|channel| {
        material_channel_range(
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
fn worst_contrast_of_band(pole: Pole, lo: f64, hi: f64) -> f64 {
    match pole {
        Pole::Black => ratio_from_luminances(0.0, lo),
        Pole::White => ratio_from_luminances(1.0, hi),
    }
}

/// Худший WCAG-контраст коммит-полюса тинта `tone` при `alpha` над коридором.
///
/// Тон квантуется (эмитируемый `01`); композитный диапазон использует объявленный
/// порядок binary64-операций без промежуточного квантования, затем conservative
/// channel/EOTF enclosure.
/// # Errors
///
/// Типизированная [`MaterialSolveErrorV1`] для невалидных tone/alpha либо
/// внутреннего нарушения численного домена. [`BackdropBox`] уже валиден по типу.
pub fn worst_contrast_encoded(
    tone: [f64; 3],
    alpha: f64,
    backdrop: &BackdropBox,
    pole: Pole,
) -> Result<f64, MaterialSolveErrorV1> {
    validate_encoded_rgb(tone).map_err(MaterialSolveErrorV1::InvalidTone)?;
    if !(0.0..=1.0).contains(&alpha) {
        return Err(MaterialSolveErrorV1::InvalidAlpha { value: alpha });
    }
    let (lo, hi) = band_luminance(quantise(tone), alpha, backdrop);
    let worst = worst_contrast_of_band(pole, lo, hi);
    if !(1.0..=21.0).contains(&worst) {
        return Err(MaterialSolveErrorV1::NumericallyIndeterminate);
    }
    Ok(worst)
}

fn validate_rechecked_bracket(
    lower_contrast: f64,
    upper_contrast: f64,
    floor_ratio: f64,
) -> Result<(), MaterialSolveErrorV1> {
    if !matches!(
        lower_contrast.partial_cmp(&floor_ratio),
        Some(core::cmp::Ordering::Less)
    ) {
        return Err(MaterialSolveErrorV1::NumericallyIndeterminate);
    }
    if !matches!(
        upper_contrast.partial_cmp(&floor_ratio),
        Some(core::cmp::Ordering::Equal | core::cmp::Ordering::Greater)
    ) {
        return Err(MaterialSolveErrorV1::NumericallyIndeterminate);
    }
    Ok(())
}

/// Выбрать проходящую альфу тинта `01` бинарной partition с фиксированным числом
/// шагов и повторно проверить найденные fail/pass endpoints.
///
/// Проверка endpoint, затем детерминированная бинарная partition по
/// `α ∈ [0, 1]` и замер на квантованном тоне-тинте. Каждый шаг сохраняет
/// rechecked fail/pass endpoints; глобальная монотонность или первый passing
/// state не заявляются. Возвращённое состояние
/// несёт [`MaterialAlphaGuaranteeV1`]: зависящая от платформы и toolchain
/// функция `powf` не выдаётся за sound-доказательство точной межсредовой
/// минимальной границы. При недостижимости пола даже на `α = 1` возвращается
/// типизированный degraded-исход, а не ошибка или fallback.
///
/// # Errors
///
/// Типизированный невалидный tone/floor или
/// [`MaterialSolveErrorV1::UnsupportedDirectedSearchRelation`].
/// Материальный путь с [`BackdropBox::FULL`] всегда лежит в направленном домене.
pub fn solve_material_alpha_encoded(
    tone: [f64; 3],
    backdrop: &BackdropBox,
    floor_ratio: f64,
) -> Result<MaterialAlpha, MaterialSolveErrorV1> {
    validate_encoded_rgb(tone).map_err(MaterialSolveErrorV1::InvalidTone)?;
    if !(1.0..=21.0).contains(&floor_ratio) {
        return Err(MaterialSolveErrorV1::InvalidFloorRatio { value: floor_ratio });
    }
    let tone_q = quantise(tone);
    let pole = committed_pole_for_valid_tone(tone_q);
    let worst_at = |alpha: f64| -> Result<f64, MaterialSolveErrorV1> {
        let (lo, hi) = band_luminance(tone_q, alpha, backdrop);
        let worst = worst_contrast_of_band(pole, lo, hi);
        if !(1.0..=21.0).contains(&worst) {
            return Err(MaterialSolveErrorV1::NumericallyIndeterminate);
        }
        Ok(worst)
    };

    // Общий закон endpoint: если полностью прозрачный tint уже держит floor,
    // порогового поиска не существует и выдумывать неуспешную нижнюю границу нельзя.
    let transparent = worst_at(0.0)?;
    if transparent >= floor_ratio {
        return Ok(MaterialAlpha {
            alpha: 0.0,
            worst_contrast: transparent,
            pole,
            status: MaterialAlphaStatusV1::Satisfied,
            guarantee: MaterialAlphaGuaranteeV1::TransparentEndpointCharacterizedV1 {
                numerical_profile:
                    MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
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
        return Err(MaterialSolveErrorV1::UnsupportedDirectedSearchRelation { pole });
    }

    // α = 1: полоса вырождается в L(tone) → худший контраст = контраст полюса на
    // тоне (солид-канон). Если и он ниже пола, возвращаем этот повторно
    // измеренный endpoint с типизированным статусом `Degraded`: гарантия не
    // выполнена.
    let opaque = worst_at(1.0)?;
    if opaque < floor_ratio {
        return Ok(MaterialAlpha {
            alpha: 1.0,
            worst_contrast: opaque,
            pole,
            status: MaterialAlphaStatusV1::Degraded,
            guarantee: MaterialAlphaGuaranteeV1::OpaqueEndpointCharacterizedV1 {
                numerical_profile:
                    MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
            },
        });
    }

    // Binary partition with a fixed number of steps: lo is rechecked failing and
    // hi rechecked passing after every update. No global-first/minimum claim is
    // made for the discontinuous legacy predicate.
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..MATERIAL_ALPHA_BISECTION_ITERATIONS {
        let mid = 0.5 * (lo + hi);
        if worst_at(mid)? >= floor_ratio {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    let worst_contrast = worst_at(hi)?;
    let lower_contrast = worst_at(lo)?;
    validate_rechecked_bracket(lower_contrast, worst_contrast, floor_ratio)?;
    Ok(MaterialAlpha {
        alpha: hi,
        worst_contrast,
        pole,
        status: MaterialAlphaStatusV1::Satisfied,
        guarantee: MaterialAlphaGuaranteeV1::BisectionBracketCharacterizedV1 {
            iterations: MATERIAL_ALPHA_BISECTION_ITERATIONS,
            lower_alpha: lo,
            upper_alpha: hi,
            numerical_profile:
                MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
        },
    })
}

/// Hex-обёртка [`solve_material_alpha_encoded`] над полным коридором
/// `[чёрный, белый]` (материальный случай — неизвестный живой фон).
///
/// # Errors
///
/// Типизированная [`MaterialSolveErrorV1`] при невалидном hex/floor.
pub fn solve_material_alpha_hex(
    tone_hex: &str,
    floor_ratio: f64,
) -> Result<MaterialAlpha, MaterialSolveErrorV1> {
    let tone = srgb_encoded_from_hex(tone_hex)
        .map_err(|reason| MaterialSolveErrorV1::InvalidToneHex { reason })?;
    solve_material_alpha_encoded(tone, &BackdropBox::FULL, floor_ratio)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alpha::composite_over_encoded;
    use crate::wcag::contrast_ratio;
    use proptest::prelude::*;

    const AA_TEXT: f64 = 4.5;

    fn enc(hex: &str) -> [f64; 3] {
        srgb_encoded_from_hex(hex).unwrap()
    }

    /// Солид-канон `01`-над-`02` байт-точно равен тону при ЛЮБОЙ α (композит `T`
    /// над `T` есть `T`) — фундамент дизайна: единственная решаемая величина α.
    #[test]
    fn solid_canon_is_tone_byte_exact_for_any_alpha() {
        for hex in ["#FFFFFF", "#787880", "#101012", "#3E87FF", "#B0B0B8"] {
            let t = enc(hex);
            let tq = quantise(t);
            for alpha in [0.01, 0.1, 0.5, 0.837, 1.0] {
                let solid = composite_over_encoded(tq, alpha, tq)
                    .expect("тестовые sRGB-каналы и alpha лежат в домене");
                assert_eq!(
                    hex_from_srgb_encoded(solid),
                    hex_from_srgb_encoded(tq),
                    "{hex}@{alpha}: солид-канон 01-над-02 разошёлся с тоном"
                );
            }
        }
    }

    /// Выбранный верхний кандидат повторно держит floor в объявленном runtime.
    /// Отдельная проба на 2% ниже делает fixture невакуумным, но не выдаётся за
    /// доказательство predecessor или минимальности без sound bound для `powf`.
    #[test]
    fn selected_alpha_rechecks_and_fixture_has_a_non_vacuous_threshold() {
        for hex in ["#E4E4E6", "#B0B0B8", "#35353A", "#2A2A30", "#5C5C5C"] {
            let m = solve_material_alpha_hex(hex, AA_TEXT).unwrap();
            assert!(!m.degraded(), "{hex}: неожиданная деградация на AA");
            assert!(m.alpha() > 0.0 && m.alpha() <= 1.0, "{hex}: α вне (0,1]");
            let pole = m.pole();
            let at =
                |a: f64| worst_contrast_encoded(enc(hex), a, &BackdropBox::FULL, pole).unwrap();
            // Финальная повторная runtime-проверка обязана держать floor.
            assert!(
                at(m.alpha()) >= AA_TEXT - 1e-9,
                "{hex}: на α={} худший контраст {} < пола",
                m.alpha(),
                at(m.alpha())
            );
            // Грубая anti-vacuum характеризация, а не сертификат минимальности.
            if m.alpha() > 1e-3 {
                assert!(
                    at(m.alpha() * 0.98) < AA_TEXT,
                    "{hex}: 2%-lower anti-vacuum probe unexpectedly passes for α={}",
                    m.alpha()
                );
            }
        }
    }

    /// Frozen legacy EOTF имеет downward seam, поэтому даже направленный
    /// material path нельзя объявлять глобально монотонным по alpha.
    #[test]
    fn legacy_eotf_seam_rejects_global_alpha_monotonicity() {
        let alpha = 0.039_28_f64;
        let next_alpha = f64::from_bits(alpha.to_bits() + 1);
        let at = material_channel_over_byte_scale(255.0, alpha, 0.0);
        let after = material_channel_over_byte_scale(255.0, next_alpha, 0.0);
        assert_eq!(at.to_bits(), alpha.to_bits());
        assert_eq!(after.to_bits(), next_alpha.to_bits());

        let before_luminance = relative_luminance([at; 3]);
        let after_luminance = relative_luminance([after; 3]);
        assert!(
            after_luminance < before_luminance,
            "legacy seam witness stopped rejecting global monotonicity"
        );
    }

    /// Гарантия проверяема из эмитированных значений: dense independent backdrop
    /// probes используют официальный scalar compositor, а conservative verdict
    /// ядра не превышает ни один измеренный probe.
    #[test]
    fn guarantee_recomputable_from_emitted_tint() {
        for hex in ["#E9E9EB", "#A0A0A8", "#313135"] {
            let m = solve_material_alpha_hex(hex, AA_TEXT).unwrap();
            let tint_q = quantise(enc(hex));
            let pole_lum = m.pole().luminance();
            let probes = [0.0, 0.039_28, 0.039_280_000_000_000_01, 0.5, 1.0];
            let mut measured_min = f64::INFINITY;
            for red in probes {
                for green in probes {
                    for blue in probes {
                        let background = [red, green, blue];
                        let composite = core::array::from_fn(|channel| {
                            material_channel_over_byte_scale(
                                (tint_q[channel] * 255.0).round(),
                                m.alpha(),
                                background[channel],
                            )
                        });
                        let measured =
                            ratio_from_luminances(pole_lum, relative_luminance(composite));
                        measured_min = measured_min.min(measured);
                    }
                }
            }
            assert!(
                m.worst_contrast() <= measured_min,
                "{hex}: conservative verdict {} exceeded measured probe {measured_min}",
                m.worst_contrast()
            );
            assert!(m.worst_contrast() >= AA_TEXT, "{hex}: verdict ниже пола");
        }
    }

    /// Коммит-полюс = полюс максимального контраста: светлый тон → чёрный лейбл,
    /// тёмный → белый. Сверка против прямого WCAG-максимума.
    #[test]
    fn committed_pole_maximises_contrast() {
        for hex in [
            "#FFFFFF", "#EDEDEF", "#C0C0C0", "#808080", "#5C5C5C", "#303030", "#101012", "#000000",
            // Насыщенные хроматические тоны обеих полярностей (полюс — свойство
            // светлоты, но проверяем и на цвете).
            "#FFCC00", "#34C759", "#3E87FF", "#FF3B30", "#AF52DE", "#0A3A6B",
        ] {
            let tone = quantise(enc(hex));
            let pole = committed_pole_encoded(tone).unwrap();
            let c_black = contrast_ratio([0.0; 3], tone);
            let c_white = contrast_ratio([1.0; 3], tone);
            let want = if c_black >= c_white {
                Pole::Black
            } else {
                Pole::White
            };
            assert_eq!(pole, want, "{hex}: полюс не максимизирует контраст");
        }
    }

    /// AA-пол разрешим на ЛЮБОМ тоне (полюс максимального контраста даёт ≥ 4.58 на
    /// α=1) — теорема существования годной α ∈ (0,1]. Свип по всей серой оси И по
    /// насыщенным ХРОМАТИЧЕСКИМ тонам (теорема хрома-независима: max-контраст
    /// полюса — функция только светлоты, минимум 4.58 в кроссовере).
    #[test]
    fn aa_floor_always_solvable_no_degradation() {
        for i in 0..=255 {
            let g = f64::from(i) / 255.0;
            let m = solve_material_alpha_encoded([g, g, g], &BackdropBox::FULL, AA_TEXT).unwrap();
            assert!(
                !m.degraded(),
                "серый {i}: AA обязан быть разрешим без деградации"
            );
            assert!(m.alpha() > 0.0 && m.alpha() <= 1.0);
        }
        // Насыщенные тоны разных светлот/оттенков — не только серые.
        for hex in [
            "#3E87FF", "#FF3B30", "#34C759", "#FFCC00", "#AF52DE", "#007AFF", "#B03030", "#0A3A6B",
        ] {
            let m = solve_material_alpha_hex(hex, AA_TEXT).unwrap();
            assert!(
                !m.degraded(),
                "{hex}: AA обязан быть разрешим без деградации"
            );
            assert!(m.alpha() > 0.0 && m.alpha() <= 1.0);
        }
    }

    /// Directed-search guard: узкий коридор с тоном ВНЕ короба со стороны полюса
    /// честно отвергается, а не получает произвольную alpha/degradation.
    /// Контрпример из независимой верификации:
    /// тон `#8A8A8A` (Lum≈0.25, чёрный полюс) над коридором `[#B3B3B3, белый]`
    /// (все фоны СВЕТЛЕЕ тона) — `tone < min`, немонотонно.
    #[test]
    fn guard_rejects_non_monotone_narrow_corridor() {
        let tone = enc("#8A8A8A");
        assert_eq!(committed_pole_encoded(tone), Ok(Pole::Black));
        let bad = BackdropBox::try_new(enc("#B3B3B3"), [1.0; 3]).unwrap();
        // Неподдерживаемое направление → typed error, не ложная деградация.
        assert!(matches!(
            solve_material_alpha_encoded(tone, &bad, 15.0),
            Err(MaterialSolveErrorV1::UnsupportedDirectedSearchRelation { pole: Pole::Black })
        ));
        let transparent = solve_material_alpha_encoded(tone, &bad, 1.0)
            .expect("transparent endpoint does not require monotone threshold search");
        assert_eq!(transparent.alpha().to_bits(), 0.0_f64.to_bits());
        assert_eq!(transparent.status(), MaterialAlphaStatusV1::Satisfied);
        assert!(matches!(
            transparent.guarantee(),
            MaterialAlphaGuaranteeV1::TransparentEndpointCharacterizedV1 { .. }
        ));
        // worst_contrast_encoded сам не применяет selection guard.
        assert!(worst_contrast_encoded(tone, 0.5, &bad, Pole::Black).is_ok());
        // Поддерживаемое направление (тон ВНУТРИ короба, tone ≥ min) — решается.
        let good = BackdropBox::try_new(enc("#202020"), [1.0; 3]).unwrap();
        assert!(
            solve_material_alpha_encoded(tone, &good, AA_TEXT).is_ok(),
            "тон в коробе — поддерживаемое направление, обязан решиться"
        );
    }

    /// Тон дальше от фона (прим. более серый на светлой теме) требует ПЛОТНЕЕ α,
    /// чем тон ближе к фону — порядок тиров (base плотнее subtle) выводится
    /// физикой, не подбором. Светлая тема: белый фон, тон тем темнее, чем дальше.
    #[test]
    fn denser_tone_needs_higher_alpha_light_theme() {
        // Светлые тоны, убывающая светлота (subtle→base): база плотнее.
        let subtle = solve_material_alpha_hex("#E8E8EA", AA_TEXT).unwrap();
        let soft = solve_material_alpha_hex("#D8D8DC", AA_TEXT).unwrap();
        let base = solve_material_alpha_hex("#B4B4BC", AA_TEXT).unwrap();
        assert!(
            subtle.alpha < soft.alpha && soft.alpha < base.alpha,
            "порядок α нарушен: subtle {} soft {} base {}",
            subtle.alpha,
            soft.alpha,
            base.alpha
        );
    }

    /// Более высокий пол (AAA 7:1) может быть недостижим даже на α=1 (средний тон)
    /// — тогда честная деградация, а не ложное обещание.
    #[test]
    fn high_floor_degrades_honestly_on_mid_tone() {
        // Средний тон: полюс максимального контраста ≈ 4.6, ниже 7:1.
        let m = solve_material_alpha_encoded([0.42, 0.42, 0.42], &BackdropBox::FULL, 7.0).unwrap();
        assert!(
            m.degraded(),
            "средний тон обязан деградировать на AAA-поле 7:1"
        );
        assert_eq!(m.alpha(), 1.0, "degraded-endpoint обязан вернуть α=1");
        assert!(m.worst_contrast() < 7.0);
        assert_eq!(m.status(), MaterialAlphaStatusV1::Degraded);
        assert!(matches!(
            m.guarantee(),
            MaterialAlphaGuaranteeV1::OpaqueEndpointCharacterizedV1 {
                numerical_profile:
                    MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
            }
        ));
    }

    /// Узкий коридор требует МЕНЬШЕ плотности, чем полный [чёрный, белый]:
    /// известная область фона (ADR-0004) — более щадящая гарантия.
    #[test]
    fn narrow_corridor_needs_less_alpha_than_full() {
        let tone = enc("#C8C8CE");
        let full = solve_material_alpha_encoded(tone, &BackdropBox::FULL, AA_TEXT).unwrap();
        // Узкий светлый коридор: фон только в [#C0.., #FF..].
        let narrow = BackdropBox::try_new(enc("#C0C0C0"), [1.0; 3]).unwrap();
        let m = solve_material_alpha_encoded(tone, &narrow, AA_TEXT).unwrap();
        assert!(
            m.alpha() <= full.alpha() + 1e-12,
            "узкий коридор потребовал не меньше полного: {} vs {}",
            m.alpha(),
            full.alpha()
        );
    }

    /// Домен закреплён: мусор-входы отвергаются (молчаливый ответ был бы ложным
    /// обещанием разрешимости).
    #[test]
    fn out_of_domain_is_rejected() {
        let point = BackdropBox::try_new([0.25, 0.5, 0.75], [0.25, 0.5, 0.75])
            .expect("degenerate box remains valid");
        assert!(worst_contrast_encoded([1.0; 3], 0.5, &point, Pole::Black).is_ok());

        assert!(matches!(
            solve_material_alpha_encoded([1.5, 0.0, 0.0], &BackdropBox::FULL, AA_TEXT),
            Err(MaterialSolveErrorV1::InvalidTone(
                EncodedRgbErrorV1::OutOfRangeChannel {
                    channel: RgbChannelV1::Red,
                    value: 1.5,
                }
            ))
        ));
        assert!(matches!(
            solve_material_alpha_encoded([f64::NAN, 0.5, 0.5], &BackdropBox::FULL, AA_TEXT),
            Err(MaterialSolveErrorV1::InvalidTone(
                EncodedRgbErrorV1::NonFiniteChannel {
                    channel: RgbChannelV1::Red,
                }
            ))
        ));
        for floor in [0.5, 25.0, f64::NAN] {
            assert!(matches!(
                solve_material_alpha_encoded([0.5; 3], &BackdropBox::FULL, floor),
                Err(MaterialSolveErrorV1::InvalidFloorRatio { value })
                    if value.to_bits() == floor.to_bits()
            ));
        }
        assert!(matches!(
            committed_pole_encoded([2.0, 0.0, 0.0]),
            Err(MaterialSolveErrorV1::InvalidTone(_))
        ));
        assert!(matches!(
            worst_contrast_encoded([0.5; 3], 1.5, &BackdropBox::FULL, Pole::Black),
            Err(MaterialSolveErrorV1::InvalidAlpha { value: 1.5 })
        ));
        assert!(matches!(
            solve_material_alpha_hex("нет", AA_TEXT),
            Err(MaterialSolveErrorV1::InvalidToneHex { .. })
        ));
    }

    #[test]
    fn backdrop_box_constructor_reports_distinct_invariant_failures() {
        assert!(matches!(
            BackdropBox::try_new([1.0, 0.0, 0.0], [0.0, 1.0, 1.0]),
            Err(BackdropBoxErrorV1::ReversedChannel {
                channel: RgbChannelV1::Red,
                min: 1.0,
                max: 0.0,
            })
        ));
        assert!(matches!(
            BackdropBox::try_new([0.0; 3], [f64::NAN, 1.0, 1.0]),
            Err(BackdropBoxErrorV1::NonFiniteChannel {
                bound: BackdropBoundV1::Max,
                channel: RgbChannelV1::Red,
            })
        ));
        assert!(matches!(
            BackdropBox::try_new([-0.01, 0.0, 0.0], [1.0; 3]),
            Err(BackdropBoxErrorV1::OutOfRangeChannel {
                bound: BackdropBoundV1::Min,
                channel: RgbChannelV1::Red,
                value: -0.01,
            })
        ));

        let point = BackdropBox::try_new([0.25, 0.5, 0.75], [0.25, 0.5, 0.75])
            .expect("degenerate ordered box is a valid reachable set");
        assert_eq!(point.min(), [0.25, 0.5, 0.75]);
        assert_eq!(point.max(), [0.25, 0.5, 0.75]);
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
                Err(MaterialSolveErrorV1::NumericallyIndeterminate),
                "lower={lower}, upper={upper}",
            );
        }
    }

    #[test]
    fn material_public_api_has_typed_failures_and_explicit_numerical_guarantee() {
        assert!(matches!(
            worst_contrast_encoded([0.5; 3], f64::NAN, &BackdropBox::FULL, Pole::Black),
            Err(MaterialSolveErrorV1::InvalidAlpha { .. })
        ));
        assert!(matches!(
            solve_material_alpha_encoded([f64::NAN, 0.5, 0.5], &BackdropBox::FULL, AA_TEXT),
            Err(MaterialSolveErrorV1::InvalidTone(
                EncodedRgbErrorV1::NonFiniteChannel {
                    channel: RgbChannelV1::Red,
                }
            ))
        ));
        assert!(matches!(
            solve_material_alpha_encoded([0.5; 3], &BackdropBox::FULL, 25.0),
            Err(MaterialSolveErrorV1::InvalidFloorRatio { value: 25.0 })
        ));

        let non_monotone = BackdropBox::try_new(enc("#B3B3B3"), [1.0; 3]).unwrap();
        assert!(matches!(
            solve_material_alpha_encoded(enc("#8A8A8A"), &non_monotone, 15.0),
            Err(MaterialSolveErrorV1::UnsupportedDirectedSearchRelation { pole: Pole::Black })
        ));

        let solved = solve_material_alpha_hex("#E4E4E6", AA_TEXT).unwrap();
        assert_eq!(solved.status(), MaterialAlphaStatusV1::Satisfied);
        let (lower_alpha, upper_alpha) = match solved.guarantee() {
            MaterialAlphaGuaranteeV1::BisectionBracketCharacterizedV1 {
                iterations: 60,
                lower_alpha,
                upper_alpha,
                numerical_profile:
                    MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
            } => (lower_alpha, upper_alpha),
            other => panic!("expected characterized threshold bracket, got {other:?}"),
        };
        assert_eq!(solved.alpha().to_bits(), upper_alpha.to_bits());
        assert!(
            worst_contrast_encoded(
                enc("#E4E4E6"),
                lower_alpha,
                &BackdropBox::FULL,
                solved.pole(),
            )
            .unwrap()
                < AA_TEXT
        );
        let recomputed = worst_contrast_encoded(
            enc("#E4E4E6"),
            solved.alpha(),
            &BackdropBox::FULL,
            solved.pole(),
        )
        .unwrap();
        assert_eq!(recomputed.to_bits(), solved.worst_contrast().to_bits());
        assert!(recomputed >= AA_TEXT);

        let transparent = solve_material_alpha_hex("#E4E4E6", 1.0).unwrap();
        assert_eq!(transparent.alpha().to_bits(), 0.0_f64.to_bits());
        assert_eq!(transparent.status(), MaterialAlphaStatusV1::Satisfied);
        assert!(matches!(
            transparent.guarantee(),
            MaterialAlphaGuaranteeV1::TransparentEndpointCharacterizedV1 {
                numerical_profile:
                    MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1,
            }
        ));
        assert!(transparent.worst_contrast() >= 1.0);
    }

    #[test]
    fn selected_alpha_rechecks_in_the_official_byte_scale_affine_order() {
        for (tone_hex, floor, alpha_bits) in [
            ("#020202", 3.0, 0x3fda_d867_f596_0b7c),
            ("#000000", 7.0, 0x3fe4_d36f_0c15_b3eb),
            ("#000000", 19.7963, 0x3fee_be38_c41e_493b),
        ] {
            let solved = solve_material_alpha_hex(tone_hex, floor).unwrap();
            assert_eq!(solved.alpha().to_bits(), alpha_bits);
            let tint = quantise(enc(tone_hex));
            let composite = |background: [f64; 3]| {
                core::array::from_fn(|channel| {
                    let tint_byte = (tint[channel] * 255.0).round();
                    let background_byte_scale = background[channel] * 255.0;
                    (background_byte_scale + solved.alpha() * (tint_byte - background_byte_scale))
                        / 255.0
                })
            };
            let lo = relative_luminance(composite([0.0; 3]));
            let hi = relative_luminance(composite([1.0; 3]));
            let consumer_worst = worst_contrast_of_band(solved.pole(), lo, hi);
            assert!(
                consumer_worst >= solved.worst_contrast(),
                "{tone_hex}: conservative core bound {} exceeded endpoint consumer {consumer_worst}",
                solved.worst_contrast()
            );
            assert!(
                consumer_worst >= floor,
                "{tone_hex}: reported Satisfied state misses requested floor in the official consumer"
            );
        }
    }

    #[test]
    fn valid_material_fixture_bits_are_characterized_across_the_typed_migration() {
        let fixtures = [
            (
                "#E4E4E6",
                0x3fe0_4922_26de_cb0a,
                0x4012_0000_0000_0000,
                Pole::Black,
            ),
            (
                "#B0B0B8",
                0x3fe5_0a37_d1ac_0292,
                0x4012_0000_0000_0000,
                Pole::Black,
            ),
            (
                "#35353A",
                0x3fe5_a367_f8ad_9e98,
                0x4012_0000_0000_0001,
                Pole::White,
            ),
            (
                "#2A2A30",
                0x3fe4_86ad_a430_0884,
                0x4012_0000_0000_0000,
                Pole::White,
            ),
            (
                "#5C5C5C",
                0x3fea_c450_4ad4_a737,
                0x4012_0000_0000_0000,
                Pole::White,
            ),
        ];
        for (hex, alpha_bits, worst_bits, pole) in fixtures {
            let solved = solve_material_alpha_hex(hex, AA_TEXT).unwrap();
            assert_eq!(solved.alpha().to_bits(), alpha_bits, "{hex}: alpha drift");
            assert_eq!(
                solved.worst_contrast().to_bits(),
                worst_bits,
                "{hex}: worst contrast drift"
            );
            assert_eq!(solved.pole(), pole, "{hex}: pole drift");
            assert_eq!(solved.status(), MaterialAlphaStatusV1::Satisfied);
        }
    }

    #[test]
    fn interior_wcag_eotf_seam_cannot_undercut_a_satisfied_full_box_result() {
        let floor = 19.7963;
        let solved = solve_material_alpha_hex("#000000", floor).unwrap();
        assert_eq!(solved.status(), MaterialAlphaStatusV1::Satisfied);
        assert!(solved.worst_contrast() >= floor);

        // Допустимый interior backdrop переводит byte-scale composite ровно к
        // discontinuity frozen legacy WCAG 2.1 (2018) split 0.03928. Старый
        // corner-only band пропускал эту точку и завышал all-backdrops verdict.
        let gray = 0.999_762_480_394_283_1;
        let point = BackdropBox::try_new([gray; 3], [gray; 3]).unwrap();
        let actual =
            worst_contrast_encoded(enc("#000000"), solved.alpha(), &point, solved.pole()).unwrap();
        assert!(
            actual >= floor,
            "full-box Satisfied undercut at interior EOTF seam: {actual} < {floor}"
        );
    }

    #[test]
    fn channel_range_encloses_known_binary64_interior_jitter() {
        let alpha = f64::from_bits(1.0_f64.to_bits() - 1);
        let tint_byte = 2.0;
        let interior_byte_scale = 0.137_643_568_434_543_95;
        let interior_background = interior_byte_scale / 255.0;
        let actual = material_channel_over_byte_scale(tint_byte, alpha, interior_background);
        let endpoint_min = material_channel_over_byte_scale(tint_byte, alpha, 0.0)
            .min(material_channel_over_byte_scale(tint_byte, alpha, 1.0));
        assert!(
            actual < endpoint_min,
            "fixture must reject endpoint-only affine monotonicity"
        );

        let (lo, hi) = material_channel_range(tint_byte, alpha, 0.0, 1.0);
        assert!(
            lo <= actual && actual <= hi,
            "{actual} outside [{lo}, {hi}]"
        );
    }

    #[test]
    fn composite_range_margin_dominates_pairwise_error_and_outward_rounding() {
        let single = std::hint::black_box(MATERIAL_COMPOSITE_SINGLE_EVALUATION_ERROR_BOUND);
        let pairwise = std::hint::black_box(MATERIAL_COMPOSITE_PAIRWISE_ERROR_BOUND);
        let outward = std::hint::black_box(MATERIAL_COMPOSITE_OUTWARD_ROUNDING_BOUND);
        let margin = std::hint::black_box(MATERIAL_COMPOSITE_RANGE_MARGIN);
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
        let at = material_channel_over_byte_scale(tint_byte, alpha, background);
        let next = material_channel_over_byte_scale(tint_byte, alpha, next_background);
        assert!(
            next < at,
            "fixture must reject binary64 background monotonicity"
        );

        let (lo, hi) = material_channel_range(tint_byte, alpha, background, next_background);
        assert!(lo <= at && at <= hi);
        assert!(lo <= next && next <= hi);
    }

    #[test]
    fn opaque_endpoint_is_exact_tint_identity() {
        for tint_byte in [0.0, 1.0, 2.0, 127.0, 228.0, 254.0, 255.0] {
            let expected = tint_byte / 255.0;
            let (lo, hi) = material_channel_range(tint_byte, 1.0, 0.0, 1.0);
            assert_eq!(lo.to_bits(), expected.to_bits());
            assert_eq!(hi.to_bits(), expected.to_bits());
            for background in [0.0, f64::EPSILON, 0.137_643_568_434_543_95, 0.5, 1.0] {
                assert_eq!(
                    material_channel_over_byte_scale(tint_byte, 1.0, background).to_bits(),
                    expected.to_bits(),
                    "tint={tint_byte}, background={background}"
                );
            }
        }
    }

    #[test]
    fn extreme_opaque_endpoints_hold_exact_twentyone_to_one() {
        for hex in ["#000000", "#FFFFFF"] {
            let solved = solve_material_alpha_hex(hex, 21.0).unwrap();
            assert_eq!(solved.status(), MaterialAlphaStatusV1::Satisfied, "{hex}");
            assert_eq!(solved.alpha().to_bits(), 1.0_f64.to_bits(), "{hex}");
            assert_eq!(
                solved.worst_contrast().to_bits(),
                21.0_f64.to_bits(),
                "{hex}"
            );
        }
    }

    #[test]
    fn point_and_ordered_boxes_preserve_characterized_material_result_bits() {
        let tone = enc("#C8C8CE");
        let fixtures = [
            (
                BackdropBox::try_new(enc("#202020"), enc("#202020")).unwrap(),
                0x3fdf_f53f_9eff_0967,
                0x4012_0000_0000_0000,
            ),
            (
                BackdropBox::try_new(enc("#303030"), enc("#A0A0A0")).unwrap(),
                0x3fdc_9853_0c20_50b1,
                0x4012_0000_0000_0000,
            ),
        ];

        for (backdrop, alpha_bits, worst_bits) in fixtures {
            let solved = solve_material_alpha_encoded(tone, &backdrop, AA_TEXT).unwrap();
            assert_eq!(solved.alpha().to_bits(), alpha_bits);
            assert_eq!(solved.worst_contrast().to_bits(), worst_bits);
            assert_eq!(solved.pole(), Pole::Black);
            assert_eq!(solved.status(), MaterialAlphaStatusV1::Satisfied);
        }
    }

    proptest! {
        #[test]
        fn backdrop_constructor_accepts_exactly_finite_ordered_encoded_boxes(
            min in proptest::array::uniform3(any::<f64>()),
            max in proptest::array::uniform3(any::<f64>()),
        ) {
            let expected = min.into_iter().chain(max).all(|value| {
                value.is_finite() && (0.0..=1.0).contains(&value)
            }) && (0..3).all(|channel| min[channel] <= max[channel]);
            prop_assert_eq!(BackdropBox::try_new(min, max).is_ok(), expected);
        }

        #[test]
        fn material_channel_range_encloses_official_scalar_compositor(
            tint_byte in any::<u8>(),
            alpha in 0.0_f64..1.0_f64,
            background_a in 0.0_f64..1.0_f64,
            background_b in 0.0_f64..1.0_f64,
            position in 0.0_f64..1.0_f64,
        ) {
            let background_lo = background_a.min(background_b);
            let background_hi = background_a.max(background_b);
            let background = background_lo + position * (background_hi - background_lo);
            let tint_byte = f64::from(tint_byte);
            let actual = material_channel_over_byte_scale(tint_byte, alpha, background);
            let (lo, hi) = material_channel_range(
                tint_byte,
                alpha,
                background_lo,
                background_hi,
            );
            prop_assert!(lo.is_finite() && hi.is_finite());
            prop_assert!((0.0..=1.0).contains(&lo));
            prop_assert!((0.0..=1.0).contains(&hi));
            prop_assert!(lo <= actual, "{actual} below {lo}");
            prop_assert!(actual <= hi, "{actual} above {hi}");
        }
    }
}
