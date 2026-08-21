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
//! # C7d: физика перенесена в общий модуль
//!
//! Исполняемая физика (60-шаговая бисекция над [`BackdropBox`], conservative
//! channel/EOTF envelope, полюсная полярность) живёт в ОДНОМ месте — приватном
//! generic-модуле `corridor_representation` с corridor-словарём. Этот
//! модуль — замороженная публичная проекция pre-C7c: публичные типы и функции
//! остаются нетронутым синтаксисом, а их тела делегируют corridor-физике без
//! второго источника математики. Resolver исполняет Material через
//! скомпилированную invocation (см. `crate::semantic`), а не через этот
//! публичный фасад.
//!
//! # Выведенная альфа (не рукописная)
//!
//! `α` — проходящая плотность, выбранная бисекцией с фиксированным числом шагов
//! в объявленном охарактеризованном для платформы runtime: тинт над ХУДШИМ
//! разрешённым фоном остаётся в контракте базы. Коммит-лейбл поверхности
//! (ахроматический полюс максимального контраста на тоне `T` — белый на тёмном
//! `T`, чёрный на светлом) держит пол читаемости ПО ВСЕМУ коридору достижимых
//! фонов. На `α = 1` полоса вырождается в `L(T)`, а полюс максимального
//! контраста на ЛЮБОМ тоне даёт ≥ 4.58:1, поэтому для пола ≤ AA годная
//! `α ∈ (0, 1]` существует всегда; при более высоком поле честно возвращается
//! `α = 1` с флагом [`MaterialAlpha::degraded`] — не молчание.
//!
//! # Пространство
//!
//! Композит — гамма-кодированный sRGB reference-профиля [`crate::alpha`],
//! заземлённого 12 Figma-парами, но не выдаваемого за любой браузерный pipeline.
//! WCAG-светлота меряется на кодированном тоне-тинте (квантованном до 8-битного
//! hex — эмитируемый цвет `01`), композит над фоном берётся без промежуточного
//! переквантования в byte-scale affine порядке binary64-операций, что и
//! официальный потребитель. Применимость к рендереру и управлению цветом
//! принадлежит отдельному conformance-гейту.

use crate::corridor_representation::{
    CorridorAlphaGuaranteeV1, CorridorAlphaStatusV1, CorridorAlphaV1, CorridorNumericalProfileV1,
    CorridorSolveErrorV1,
};
use std::fmt;

// Общие геометрические типы (короб фонов, полюс, каналы) определяются в
// единственном источнике физики — `crate::corridor_representation` — и
// переиспользуются здесь как публичный синтаксис pre-C7c без второго определения.
#[cfg(test)]
use crate::corridor_representation::{BackdropBoundV1, BackdropBoxErrorV1, RgbChannelV1};
pub use crate::corridor_representation::{BackdropBox, EncodedRgbErrorV1, Pole};

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

/// Публичная проекция corridor-результата в замороженный material-словарь.
///
/// Байт-идентичность выходов: поля копируются побитно из corridor-результата;
/// численный профиль имеет ровно один вариант в обоих словарях, поэтому
/// отображение не меняет ни одной границы.
impl MaterialAlpha {
    pub(crate) fn from_corridor(result: CorridorAlphaV1) -> Self {
        Self {
            alpha: result.alpha,
            worst_contrast: result.worst_contrast,
            pole: result.pole,
            status: MaterialAlphaStatusV1::from_corridor(result.status),
            guarantee: MaterialAlphaGuaranteeV1::from_corridor(result.guarantee),
        }
    }
}

impl MaterialAlphaStatusV1 {
    /// Байт-эквивалентная проекция corridor-статуса: единственный способ создать
    /// публичный статус из corridor-исхода без второй физики.
    pub(crate) fn from_corridor(status: CorridorAlphaStatusV1) -> Self {
        match status {
            CorridorAlphaStatusV1::Satisfied => Self::Satisfied,
            CorridorAlphaStatusV1::Degraded => Self::Degraded,
        }
    }
}

impl MaterialAlphaGuaranteeV1 {
    /// Байт-эквивалентная проекция corridor-гарантии в публичный словарь.
    pub(crate) fn from_corridor(guarantee: CorridorAlphaGuaranteeV1) -> Self {
        match guarantee {
            CorridorAlphaGuaranteeV1::BisectionBracketCharacterizedV1 {
                iterations,
                lower_alpha,
                upper_alpha,
                numerical_profile,
            } => Self::BisectionBracketCharacterizedV1 {
                iterations,
                lower_alpha,
                upper_alpha,
                numerical_profile: match numerical_profile {
                    CorridorNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1 => {
                        MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1
                    }
                },
            },
            CorridorAlphaGuaranteeV1::TransparentEndpointCharacterizedV1 { numerical_profile } => {
                Self::TransparentEndpointCharacterizedV1 {
                    numerical_profile: match numerical_profile {
                        CorridorNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1 => {
                            MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1
                        }
                    },
                }
            }
            CorridorAlphaGuaranteeV1::OpaqueEndpointCharacterizedV1 { numerical_profile } => {
                Self::OpaqueEndpointCharacterizedV1 {
                    numerical_profile: match numerical_profile {
                        CorridorNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1 => {
                            MaterialNumericalProfileV1::EncodedSrgbByteScaleAffinePlatformBinary64PowfV1
                        }
                    },
                }
            }
        }
    }
}

/// Типизированная проекция corridor-ошибки в публичный material-словарь.
///
/// Варианты соответствуют один-в-один; имена `InvalidFloorRatio` (материал) и
/// `InvalidThresholdRatio` (corridor) различаются только словарём, не доменом.
pub(crate) fn from_corridor_error(error: CorridorSolveErrorV1) -> MaterialSolveErrorV1 {
    match error {
        CorridorSolveErrorV1::InvalidTone(tone) => MaterialSolveErrorV1::InvalidTone(tone),
        CorridorSolveErrorV1::InvalidToneHex { reason } => {
            MaterialSolveErrorV1::InvalidToneHex { reason }
        }
        CorridorSolveErrorV1::InvalidAlpha { value } => {
            MaterialSolveErrorV1::InvalidAlpha { value }
        }
        CorridorSolveErrorV1::InvalidThresholdRatio { value } => {
            MaterialSolveErrorV1::InvalidFloorRatio { value }
        }
        CorridorSolveErrorV1::UnsupportedDirectedSearchRelation { pole } => {
            MaterialSolveErrorV1::UnsupportedDirectedSearchRelation { pole }
        }
        CorridorSolveErrorV1::NumericallyIndeterminate => {
            MaterialSolveErrorV1::NumericallyIndeterminate
        }
    }
}

/// Коммит-полюс поверхности тона: полюс максимального WCAG-контраста на `L(tone)`.
///
/// # Errors
///
/// [`MaterialSolveErrorV1::InvalidTone`] для неконечного канала или канала вне диапазона.
pub fn committed_pole_encoded(tone: [f64; 3]) -> Result<Pole, MaterialSolveErrorV1> {
    crate::corridor_representation::committed_pole_encoded(tone).map_err(from_corridor_error)
}

/// Худший WCAG-контраст коммит-полюса тинта `tone` при `alpha` над коридором.
///
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
    crate::corridor_representation::worst_contrast_encoded(tone, alpha, backdrop, pole)
        .map_err(from_corridor_error)
}

/// Выбрать проходящую альфу тинта `01` бинарной partition с фиксированным числом
/// шагов и повторно проверить найденные fail/pass endpoints.
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
    crate::corridor_representation::solve_corridor_alpha_encoded(tone, backdrop, floor_ratio)
        .map(MaterialAlpha::from_corridor)
        .map_err(from_corridor_error)
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
    crate::corridor_representation::solve_corridor_alpha_hex(tone_hex, floor_ratio)
        .map(MaterialAlpha::from_corridor)
        .map_err(from_corridor_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spaces::srgb::srgb_encoded_from_hex;

    const AA_TEXT: f64 = 4.5;

    fn enc(hex: &str) -> [f64; 3] {
        srgb_encoded_from_hex(hex).unwrap()
    }

    /// Публичный фасад обязан быть бит-в-бит эквивалентен corridor-физике:
    /// одна и та же математика даёт одинаковые биты через обе границы.
    #[test]
    fn public_facade_is_bit_identical_to_corridor_physics() {
        for hex in [
            "#E4E4E6", "#B0B0B8", "#35353A", "#2A2A30", "#5C5C5C", "#000000",
        ] {
            for floor in [AA_TEXT, 7.0, 21.0] {
                let facade = solve_material_alpha_hex(hex, floor).unwrap();
                let corridor =
                    crate::corridor_representation::solve_corridor_alpha_hex(hex, floor).unwrap();
                assert_eq!(
                    facade.alpha().to_bits(),
                    corridor.alpha.to_bits(),
                    "{hex}@{floor}: alpha drift"
                );
                assert_eq!(
                    facade.worst_contrast().to_bits(),
                    corridor.worst_contrast.to_bits(),
                    "{hex}@{floor}: worst drift"
                );
                assert_eq!(facade.pole(), corridor.pole, "{hex}@{floor}: pole drift");
            }
        }
    }

    /// Выбранный верхний кандидат повторно держит floor в объявленном runtime.
    #[test]
    fn selected_alpha_rechecks_and_fixture_has_a_non_vacuous_threshold() {
        for hex in ["#E4E4E6", "#B0B0B8", "#35353A", "#2A2A30", "#5C5C5C"] {
            let m = solve_material_alpha_hex(hex, AA_TEXT).unwrap();
            assert!(!m.degraded(), "{hex}: неожиданная деградация на AA");
            assert!(m.alpha() > 0.0 && m.alpha() <= 1.0, "{hex}: α вне (0,1]");
            let pole = m.pole();
            let at =
                |a: f64| worst_contrast_encoded(enc(hex), a, &BackdropBox::FULL, pole).unwrap();
            assert!(
                at(m.alpha()) >= AA_TEXT - 1e-9,
                "{hex}: на α={} худший контраст {} < пола",
                m.alpha(),
                at(m.alpha())
            );
            if m.alpha() > 1e-3 {
                assert!(
                    at(m.alpha() * 0.98) < AA_TEXT,
                    "{hex}: 2%-lower anti-vacuum probe unexpectedly passes for α={}",
                    m.alpha()
                );
            }
        }
    }

    /// Гарантия проверяема из эмитированных значений: dense independent backdrop
    /// probes используют официальный scalar compositor, а conservative verdict
    /// ядра не превышает ни один измеренный probe.
    #[test]
    fn guarantee_recomputable_from_emitted_tint() {
        for hex in ["#E9E9EB", "#A0A0A8", "#313135"] {
            let m = solve_material_alpha_hex(hex, AA_TEXT).unwrap();
            let tint_q = crate::corridor_representation::quantise(enc(hex));
            let pole_lum = match m.pole() {
                Pole::White => 1.0,
                Pole::Black => 0.0,
            };
            let probes = [0.0, 0.039_28, 0.039_280_000_000_000_01, 0.5, 1.0];
            let mut measured_min = f64::INFINITY;
            for red in probes {
                for green in probes {
                    for blue in probes {
                        let background = [red, green, blue];
                        let composite = core::array::from_fn(|channel| {
                            crate::corridor_representation::corridor_channel_over_byte_scale(
                                (tint_q[channel] * 255.0).round(),
                                m.alpha(),
                                background[channel],
                            )
                        });
                        let measured = crate::spaces::srgb::relative_luminance_ratio(
                            pole_lum,
                            crate::spaces::srgb::encoded_srgb_relative_luminance(composite),
                        );
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

    /// AA-пол разрешим на ЛЮБОМ тоне (полюс максимального контраста даёт ≥ 4.58 на
    /// α=1) — теорема существования годной α ∈ (0,1]. Свип по всей серой оси И по
    /// насыщенным ХРОМАТИЧЕСКИМ тонам.
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
    #[test]
    fn guard_rejects_non_monotone_narrow_corridor() {
        let tone = enc("#8A8A8A");
        assert_eq!(committed_pole_encoded(tone), Ok(Pole::Black));
        let bad = BackdropBox::try_new(enc("#B3B3B3"), [1.0; 3]).unwrap();
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
        assert!(worst_contrast_encoded(tone, 0.5, &bad, Pole::Black).is_ok());
        let good = BackdropBox::try_new(enc("#202020"), [1.0; 3]).unwrap();
        assert!(
            solve_material_alpha_encoded(tone, &good, AA_TEXT).is_ok(),
            "тон в коробе — поддерживаемое направление, обязан решиться"
        );
    }

    /// Более высокий пол (AAA 7:1) может быть недостижим даже на α=1 (средний тон)
    /// — тогда честная деградация, а не ложное обещание.
    #[test]
    fn high_floor_degrades_honestly_on_mid_tone() {
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
}
