//! Типизированный анализ качества экранного цвета без универсального verdict.
//!
//! Модуль разделяет четыре разных вида знания:
//!
//! - координаты опубликованных моделей цветового восприятия;
//! - математически выведенную геометрию конечного sRGB;
//! - относительное сравнение с явно переданным семейным якорем;
//! - проверяемые гипотезы Labpics, которые не выдаются за ответы наблюдателя.
//!
//! Здесь намеренно нет оси `clean ↔ dirty`, общего числового score и изменения
//! цвета. Проекция является отдельной задачей #217 и может опираться на этот
//! отчёт только после screen-native валидации #232.

use crate::cleanliness::{DefectContext, Theme};
use crate::scale::max_chroma;
use crate::spaces::oklab::srgb_linear_to_oklab;
use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;

/// Версия агрегированного отчёта, который не изменяет исходный цвет.
pub const COLOR_QUALITY_AUDIT_MODEL_V1: &str = "lab-screen-color-quality-audit-v1";

/// Замороженная версия исследовательского взаимодействия
/// «положительная жёлтая компонента × относительное ослабление».
///
/// Формула сохранена для фальсификации и сравнения с будущими данными. Идентификатор
/// не содержит слова `dirt`: модель не является человеческой шкалой чистоты.
pub const WARM_DARK_INTERACTION_MODEL_V2: &str =
    "lab-warm-dark-interaction-cam16ucs-v2";

/// Версия относительного сравнения хроматичности с семейным якорем.
pub const MUTEDNESS_MODEL_V2: &str = "lab-relative-mutedness-cam16ucs-v2";

/// Происхождение численной величины или вывода.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ModelProvenance {
    /// Координаты или уравнение опубликованной модели/стандарта.
    PublishedModel,
    /// Точное математическое следствие объявленных координат и конечного домена.
    MathematicalDerivation,
    /// Фальсифицируемая формула Labpics без population-validation.
    LabpicsHypothesis,
}

/// Режим, в котором наблюдатель интерпретирует экранный стимул.
///
/// Значение никогда не выводится из RGB или имени роли: один и тот же пиксель
/// может быть самостоятельным светящимся сигналом или изображением поверхности.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AppearanceMode {
    EmissiveUi,
    SurfaceLike,
    MaterialLike,
    SpatialEffect,
    ImageContent,
}

/// Измеренные физические условия дисплея.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct DisplayConditionsV1 {
    pub white_luminance_cd_m2: f64,
    pub black_luminance_cd_m2: f64,
    pub ambient_luminance_cd_m2: f64,
}

impl DisplayConditionsV1 {
    fn validate(self) -> Result<(), String> {
        if !(self.white_luminance_cd_m2.is_finite() && self.white_luminance_cd_m2 > 0.0) {
            return Err("яркость белого дисплея должна быть конечной и положительной".into());
        }
        if !(self.black_luminance_cd_m2.is_finite() && self.black_luminance_cd_m2 >= 0.0) {
            return Err("яркость чёрного дисплея должна быть конечной и неотрицательной".into());
        }
        if self.black_luminance_cd_m2 >= self.white_luminance_cd_m2 {
            return Err("яркость чёрного дисплея должна быть ниже яркости белого".into());
        }
        if !(self.ambient_luminance_cd_m2.is_finite() && self.ambient_luminance_cd_m2 >= 0.0) {
            return Err("окружающая яркость должна быть конечной и неотрицательной".into());
        }
        Ok(())
    }
}

/// Известная пространственная часть экранного стимула.
///
/// `None` означает отсутствие измерения, а не нулевое значение. Поэтому неполный
/// контекст можно вернуть как `InsufficientContext`, не подменяя его пресетом.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[non_exhaustive]
pub struct SpatialContextV1 {
    pub adjacent_fraction: Option<f64>,
    pub angular_size_deg: Option<f64>,
    pub viewing_duration_ms: Option<f64>,
}

impl SpatialContextV1 {
    fn validate(self) -> Result<(), String> {
        if self
            .adjacent_fraction
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        {
            return Err("доля прилегающего окружения должна лежать внутри [0, 1]".into());
        }
        if self
            .angular_size_deg
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            return Err("угловой размер должен быть конечным и положительным".into());
        }
        if self
            .viewing_duration_ms
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
        {
            return Err("длительность просмотра должна быть конечной и положительной".into());
        }
        Ok(())
    }
}

/// Явный контекст appearance-модели.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct AppearanceContextV1 {
    pub mode: AppearanceMode,
    pub viewing_conditions: ViewingConditions,
    /// `None` означает номинальный, а не измеренный дисплей.
    pub display: Option<DisplayConditionsV1>,
    /// `None` означает, что пространственная геометрия неизвестна.
    pub spatial: Option<SpatialContextV1>,
}

impl AppearanceContextV1 {
    /// Номинальный контекст сохраняет mode и VC, но не выдаёт отсутствующие
    /// измерения дисплея и геометрии за известные значения.
    pub fn nominal(mode: AppearanceMode, viewing_conditions: ViewingConditions) -> Self {
        Self {
            mode,
            viewing_conditions,
            display: None,
            spatial: None,
        }
    }

    /// Измеренный контекст проверяет физические величины до анализа цвета.
    pub fn measured(
        mode: AppearanceMode,
        viewing_conditions: ViewingConditions,
        display: DisplayConditionsV1,
        spatial: SpatialContextV1,
    ) -> Result<Self, String> {
        let context = Self {
            mode,
            viewing_conditions,
            display: Some(display),
            spatial: Some(spatial),
        };
        context.validate()?;
        Ok(context)
    }

    fn validate(&self) -> Result<(), String> {
        self.viewing_conditions.validate()?;
        if let Some(display) = self.display {
            display.validate()?;
        }
        if let Some(spatial) = self.spatial {
            spatial.validate()?;
        }
        Ok(())
    }
}

/// Воспроизводимая геометрия одного цвета относительно конечного гамута sRGB.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct SrgbGamutGeometryV1 {
    pub l_ok: f64,
    pub c_ok: f64,
    pub h_ok: Option<f64>,
    pub max_srgb_chroma: Option<f64>,
    pub gamut_radius_fraction: Option<f64>,
    pub remaining_chroma_radius: Option<f64>,
    pub in_srgb_gamut: bool,
}

/// Геометрия гамута рядом с CAM16-коррелятами старой тематической обёртки.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct ContextualSrgbGamutAnalysisV1 {
    pub gamut: SrgbGamutGeometryV1,
    pub cam16_j: f64,
    pub cam16_m: f64,
    pub cam16_h: f64,
    pub background_y: f64,
    pub high_contrast: bool,
}

/// Опубликованные CAM16-UCS-координаты и выведенная геометрия sRGB.
///
/// Полей `cleanliness`, `preference`, `vividness` или `blackness` здесь нет:
/// соответствующие observer-модели добавляются только после воспроизведения
/// формул и проверки applicability matrix #230.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct AppearanceProfileV1 {
    pub coordinate_provenance: ModelProvenance,
    pub gamut_provenance: ModelProvenance,
    pub jp: f64,
    pub ap: f64,
    pub bp: f64,
    pub mp: f64,
    /// Евклидово расстояние CAM16-UCS от чёрного `hypot(J′, M′)`.
    pub radius_from_black: f64,
    /// `atan2(M′, J′)`, без превращения в абсолютный класс приглушённости.
    pub chroma_angle: f64,
    pub gamut: SrgbGamutGeometryV1,
}

/// Отношение кандидата к явно переданному семейному якорю.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum MutednessRelationV2 {
    Preserved,
    MoreMuted,
    LessMuted,
    CandidateAchromatic,
    ReferenceAchromatic,
    BothAchromatic,
}

/// Отчёт относительной потери/роста хроматичности без абсолютного порога.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct MutednessComparisonV2 {
    pub model: &'static str,
    pub reference_chroma_angle: f64,
    pub candidate_chroma_angle: f64,
    pub log_relative_chroma_loss: Option<f64>,
    pub relation: MutednessRelationV2,
}

/// Причина точного нуля исследовательского взаимодействия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WarmDarkZeroReasonV2 {
    ExactBlackForeground,
    AchromaticForeground,
    NonPositiveWarmComponent,
    NoRelativeDarkening,
}

/// Статус исследовательской формулы без человеческого verdict `Clean/Dirty`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WarmDarkInteractionStatusV2 {
    InteractionZero(WarmDarkZeroReasonV2),
    InteractionPositive,
    InsufficientContext,
    NotApplicable(AppearanceMode),
    NumericallyIndeterminate,
}

/// Замороженный отчёт гипотезы `−e·y·r·ln(r)`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct WarmDarkInteractionReportV2 {
    pub model: &'static str,
    pub provenance: ModelProvenance,
    pub appearance_mode: AppearanceMode,
    pub status: WarmDarkInteractionStatusV2,
    pub yellow_direction_cosine: f64,
    pub foreground_background_ratio: Option<f64>,
    pub interaction_potential: Option<f64>,
}

/// Немутабельный агрегат независимых сигналов.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ColorQualityAuditV1 {
    pub model: &'static str,
    pub context: AppearanceContextV1,
    pub foreground_hex: String,
    pub background_hex: String,
    pub appearance: AppearanceProfileV1,
    pub relative_mutedness: Option<MutednessComparisonV2>,
    pub warm_dark_interaction: WarmDarkInteractionReportV2,
}

#[derive(Debug, Clone, Copy)]
struct UcsAppearance {
    jp: f64,
    ap: f64,
    bp: f64,
    mp: f64,
    radius: f64,
}

fn ucs_appearance(rgb: [f64; 3], vc: &ViewingConditions) -> UcsAppearance {
    if rgb == [0.0, 0.0, 0.0] {
        return UcsAppearance {
            jp: 0.0,
            ap: 0.0,
            bp: 0.0,
            mp: 0.0,
            radius: 0.0,
        };
    }

    let (j, m, h) = crate::spaces::cam16::forward(srgb_to_xyz(rgb), vc);
    let jp = crate::spaces::cam16::ucs_j(j);
    let (mp, ap, bp) = if rgb[0] == rgb[1] && rgb[1] == rgb[2] {
        (0.0, 0.0, 0.0)
    } else {
        let mp = crate::spaces::cam16::ucs_m(m);
        let hue = h.to_radians();
        (mp, mp * hue.cos(), mp * hue.sin())
    };

    UcsAppearance {
        jp,
        ap,
        bp,
        mp,
        radius: jp.hypot(mp),
    }
}

fn compare_ucs_mutedness(
    candidate: UcsAppearance,
    reference: UcsAppearance,
) -> MutednessComparisonV2 {
    let reference_chroma_angle = reference.mp.atan2(reference.jp);
    let candidate_chroma_angle = candidate.mp.atan2(candidate.jp);

    let (log_relative_chroma_loss, relation) = match (reference.mp == 0.0, candidate.mp == 0.0) {
        (true, true) => (None, MutednessRelationV2::BothAchromatic),
        (true, false) => (None, MutednessRelationV2::ReferenceAchromatic),
        (false, true) => (None, MutednessRelationV2::CandidateAchromatic),
        (false, false) => {
            let reference_ratio = reference.mp / reference.jp;
            let candidate_ratio = candidate.mp / candidate.jp;
            let loss = (reference_ratio / candidate_ratio).ln();
            let relation = if loss > 0.0 {
                MutednessRelationV2::MoreMuted
            } else if loss < 0.0 {
                MutednessRelationV2::LessMuted
            } else {
                MutednessRelationV2::Preserved
            };
            (Some(loss), relation)
        }
    };

    MutednessComparisonV2 {
        model: MUTEDNESS_MODEL_V2,
        reference_chroma_angle,
        candidate_chroma_angle,
        log_relative_chroma_loss,
        relation,
    }
}

/// Сравнивает candidate с явно переданным family anchor.
pub fn compare_mutedness_v2(
    candidate_hex: &str,
    reference_hex: &str,
    vc: &ViewingConditions,
) -> Result<MutednessComparisonV2, String> {
    vc.validate()?;
    let candidate = ucs_appearance(srgb_from_hex(candidate_hex)?, vc);
    let reference = ucs_appearance(srgb_from_hex(reference_hex)?, vc);
    Ok(compare_ucs_mutedness(candidate, reference))
}

fn warm_dark_interaction(yellow_direction_cosine: f64, ratio: f64) -> f64 {
    if ratio == 0.0 || ratio == 1.0 || yellow_direction_cosine == 0.0 {
        0.0
    } else {
        (-std::f64::consts::E * yellow_direction_cosine * ratio * ratio.ln())
            .clamp(0.0, yellow_direction_cosine)
    }
}

fn warm_dark_report_from_rgb(
    foreground_rgb: [f64; 3],
    background_rgb: [f64; 3],
    context: &AppearanceContextV1,
) -> WarmDarkInteractionReportV2 {
    if matches!(
        context.mode,
        AppearanceMode::SpatialEffect | AppearanceMode::ImageContent
    ) {
        return WarmDarkInteractionReportV2 {
            model: WARM_DARK_INTERACTION_MODEL_V2,
            provenance: ModelProvenance::LabpicsHypothesis,
            appearance_mode: context.mode,
            status: WarmDarkInteractionStatusV2::NotApplicable(context.mode),
            yellow_direction_cosine: 0.0,
            foreground_background_ratio: None,
            interaction_potential: None,
        };
    }

    if context.mode == AppearanceMode::MaterialLike
        && (context.display.is_none() || context.spatial.is_none())
    {
        return WarmDarkInteractionReportV2 {
            model: WARM_DARK_INTERACTION_MODEL_V2,
            provenance: ModelProvenance::LabpicsHypothesis,
            appearance_mode: context.mode,
            status: WarmDarkInteractionStatusV2::InsufficientContext,
            yellow_direction_cosine: 0.0,
            foreground_background_ratio: None,
            interaction_potential: None,
        };
    }

    let foreground = ucs_appearance(foreground_rgb, &context.viewing_conditions);
    let background = ucs_appearance(background_rgb, &context.viewing_conditions);
    if ![
        foreground.jp,
        foreground.ap,
        foreground.bp,
        foreground.mp,
        foreground.radius,
        background.radius,
    ]
    .into_iter()
    .all(f64::is_finite)
    {
        return WarmDarkInteractionReportV2 {
            model: WARM_DARK_INTERACTION_MODEL_V2,
            provenance: ModelProvenance::LabpicsHypothesis,
            appearance_mode: context.mode,
            status: WarmDarkInteractionStatusV2::NumericallyIndeterminate,
            yellow_direction_cosine: 0.0,
            foreground_background_ratio: None,
            interaction_potential: None,
        };
    }

    let yellow_direction_cosine = if foreground.radius == 0.0 {
        0.0
    } else {
        (foreground.bp / foreground.radius).max(0.0)
    };

    let (status, ratio, potential) = if foreground.radius == 0.0 {
        (
            WarmDarkInteractionStatusV2::InteractionZero(
                WarmDarkZeroReasonV2::ExactBlackForeground,
            ),
            Some(0.0),
            Some(0.0),
        )
    } else if foreground.mp == 0.0 {
        (
            WarmDarkInteractionStatusV2::InteractionZero(
                WarmDarkZeroReasonV2::AchromaticForeground,
            ),
            (background.radius > 0.0)
                .then_some((foreground.radius / background.radius).min(1.0)),
            Some(0.0),
        )
    } else if yellow_direction_cosine == 0.0 {
        (
            WarmDarkInteractionStatusV2::InteractionZero(
                WarmDarkZeroReasonV2::NonPositiveWarmComponent,
            ),
            (background.radius > 0.0)
                .then_some((foreground.radius / background.radius).min(1.0)),
            Some(0.0),
        )
    } else if background.radius == 0.0 {
        (
            WarmDarkInteractionStatusV2::InsufficientContext,
            None,
            None,
        )
    } else if foreground.radius >= background.radius {
        (
            WarmDarkInteractionStatusV2::InteractionZero(
                WarmDarkZeroReasonV2::NoRelativeDarkening,
            ),
            Some(1.0),
            Some(0.0),
        )
    } else {
        let ratio = foreground.radius / background.radius;
        let potential = warm_dark_interaction(yellow_direction_cosine, ratio);
        (
            WarmDarkInteractionStatusV2::InteractionPositive,
            Some(ratio),
            Some(potential),
        )
    };

    WarmDarkInteractionReportV2 {
        model: WARM_DARK_INTERACTION_MODEL_V2,
        provenance: ModelProvenance::LabpicsHypothesis,
        appearance_mode: context.mode,
        status,
        yellow_direction_cosine,
        foreground_background_ratio: ratio,
        interaction_potential: potential,
    }
}

/// Вычисляет только замороженный research baseline; результат не является
/// человеческой классификацией или разрешением изменить цвет.
pub fn analyze_warm_dark_interaction_v2(
    foreground_hex: &str,
    background_hex: &str,
    context: &AppearanceContextV1,
) -> Result<WarmDarkInteractionReportV2, String> {
    context.validate()?;
    let foreground_rgb = srgb_from_hex(foreground_hex)?;
    let background_rgb = srgb_from_hex(background_hex)?;
    Ok(warm_dark_report_from_rgb(
        foreground_rgb,
        background_rgb,
        context,
    ))
}

fn appearance_profile_from_rgb(
    rgb: [f64; 3],
    context: &AppearanceContextV1,
) -> Result<AppearanceProfileV1, String> {
    let appearance = ucs_appearance(rgb, &context.viewing_conditions);
    if ![
        appearance.jp,
        appearance.ap,
        appearance.bp,
        appearance.mp,
        appearance.radius,
    ]
    .into_iter()
    .all(f64::is_finite)
    {
        return Err("CAM16-UCS не определён для переданного стимула".into());
    }

    Ok(AppearanceProfileV1 {
        coordinate_provenance: ModelProvenance::PublishedModel,
        gamut_provenance: ModelProvenance::MathematicalDerivation,
        jp: appearance.jp,
        ap: appearance.ap,
        bp: appearance.bp,
        mp: appearance.mp,
        radius_from_black: appearance.radius,
        chroma_angle: appearance.mp.atan2(appearance.jp),
        gamut: analyze_srgb_gamut_linear_v1(rgb)?,
    })
}

/// Собирает независимые screen-color signals и не изменяет ни один output byte.
pub fn audit_color_quality_v1(
    foreground_hex: &str,
    background_hex: &str,
    reference_hex: Option<&str>,
    context: &AppearanceContextV1,
) -> Result<ColorQualityAuditV1, String> {
    context.validate()?;
    let foreground_rgb = srgb_from_hex(foreground_hex)?;
    let background_rgb = srgb_from_hex(background_hex)?;
    let appearance = appearance_profile_from_rgb(foreground_rgb, context)?;
    let relative_mutedness = reference_hex
        .map(|reference| {
            let reference_rgb = srgb_from_hex(reference)?;
            Ok::<_, String>(compare_ucs_mutedness(
                ucs_appearance(foreground_rgb, &context.viewing_conditions),
                ucs_appearance(reference_rgb, &context.viewing_conditions),
            ))
        })
        .transpose()?;
    let warm_dark_interaction =
        warm_dark_report_from_rgb(foreground_rgb, background_rgb, context);

    Ok(ColorQualityAuditV1 {
        model: COLOR_QUALITY_AUDIT_MODEL_V1,
        context: *context,
        foreground_hex: foreground_hex.to_ascii_uppercase(),
        background_hex: background_hex.to_ascii_uppercase(),
        appearance,
        relative_mutedness,
        warm_dark_interaction,
    })
}

/// Анализирует Oklch относительно той же радиальной границы sRGB, что и генератор.
pub fn analyze_srgb_gamut_oklch_v1(
    l_ok: f64,
    c_ok: f64,
    h_ok: f64,
) -> Result<SrgbGamutGeometryV1, String> {
    if !(l_ok.is_finite() && (0.0..=1.0).contains(&l_ok)) {
        return Err(format!(
            "Oklab L должна быть конечной и лежать внутри [0, 1]: {l_ok}"
        ));
    }
    if !(c_ok.is_finite() && c_ok >= 0.0) {
        return Err(format!(
            "хрома Oklab должна быть конечной и неотрицательной: {c_ok}"
        ));
    }
    if !h_ok.is_finite() {
        return Err(format!("оттенок Oklab должен быть конечным: {h_ok}"));
    }

    if c_ok == 0.0 {
        return Ok(SrgbGamutGeometryV1 {
            l_ok,
            c_ok,
            h_ok: None,
            max_srgb_chroma: None,
            gamut_radius_fraction: None,
            remaining_chroma_radius: None,
            in_srgb_gamut: true,
        });
    }

    let h_ok = h_ok.rem_euclid(360.0);
    let max_srgb_chroma = max_chroma(l_ok, h_ok);
    Ok(SrgbGamutGeometryV1 {
        l_ok,
        c_ok,
        h_ok: Some(h_ok),
        max_srgb_chroma: Some(max_srgb_chroma),
        gamut_radius_fraction: (max_srgb_chroma > 0.0).then_some(c_ok / max_srgb_chroma),
        remaining_chroma_radius: Some((max_srgb_chroma - c_ok).max(0.0)),
        in_srgb_gamut: c_ok <= max_srgb_chroma,
    })
}

/// Анализирует linear sRGB без clipping.
pub fn analyze_srgb_gamut_linear_v1(rgb: [f64; 3]) -> Result<SrgbGamutGeometryV1, String> {
    if !rgb
        .into_iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    {
        return Err(format!(
            "каналы линейного sRGB должны быть конечными и лежать внутри [0, 1]: {rgb:?}"
        ));
    }
    if rgb[0] == rgb[1] && rgb[1] == rgb[2] {
        let l_ok = srgb_linear_to_oklab(rgb)[0];
        return analyze_srgb_gamut_oklch_v1(l_ok, 0.0, 0.0);
    }

    let lab = srgb_linear_to_oklab(rgb);
    let c_ok = lab[1].hypot(lab[2]);
    let h_ok = if c_ok == 0.0 {
        0.0
    } else {
        lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0)
    };
    analyze_srgb_gamut_oklch_v1(lab[0], c_ok, h_ok)
}

/// Анализирует кодированный цвет `#RRGGBB`.
pub fn analyze_srgb_gamut_hex_v1(hex: &str) -> Result<SrgbGamutGeometryV1, String> {
    analyze_srgb_gamut_linear_v1(srgb_from_hex(hex)?)
}

fn background_y(hex: &str) -> Result<f64, String> {
    Ok(srgb_to_xyz(srgb_from_hex(hex)?)[1])
}

fn contextual_vc(theme: Theme, y: f64) -> ViewingConditions {
    let (mut vc, high_contrast) = match theme {
        Theme::Light => (ViewingConditions::srgb_with_yb(y * 100.0), false),
        Theme::Dark => (ViewingConditions::dim_surround_with_yb(y * 100.0), false),
        Theme::LightIc => (ViewingConditions::srgb_with_yb(y * 100.0), true),
        Theme::DarkIc => (ViewingConditions::dim_surround_with_yb(y * 100.0), true),
    };
    vc.high_contrast = high_contrast;
    vc
}

/// Совместимый отчёт CAM16/gamut для исторического `DefectContext`.
pub fn analyze_srgb_gamut_hex_in_context_v1(
    hex: &str,
    ctx: DefectContext<'_>,
) -> Result<ContextualSrgbGamutAnalysisV1, String> {
    let rgb = srgb_from_hex(hex)?;
    let gamut = analyze_srgb_gamut_linear_v1(rgb)?;
    let requested_background_y = background_y(ctx.bg_hex)?;
    let vc = contextual_vc(ctx.theme, requested_background_y);
    let (cam16_j, cam16_m, cam16_h) = crate::spaces::cam16::forward(srgb_to_xyz(rgb), &vc);
    Ok(ContextualSrgbGamutAnalysisV1 {
        gamut,
        cam16_j,
        cam16_m,
        cam16_h,
        background_y: vc.n,
        high_contrast: vc.high_contrast,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_encoded_grays_have_no_warm_dark_interaction() {
        let context = AppearanceContextV1::nominal(
            AppearanceMode::SurfaceLike,
            ViewingConditions::srgb(),
        );
        for byte in 0_u8..=u8::MAX {
            let hex = format!("#{byte:02X}{byte:02X}{byte:02X}");
            let report = analyze_warm_dark_interaction_v2(&hex, "#FFFFFF", &context).unwrap();
            assert_eq!(report.yellow_direction_cosine, 0.0, "{hex}");
            assert_eq!(report.interaction_potential, Some(0.0), "{hex}");
            assert!(matches!(
                report.status,
                WarmDarkInteractionStatusV2::InteractionZero(
                    WarmDarkZeroReasonV2::AchromaticForeground
                        | WarmDarkZeroReasonV2::ExactBlackForeground
                )
            ));
        }
    }

    #[test]
    fn frozen_interaction_has_derived_endpoints_and_peak() {
        assert_eq!(warm_dark_interaction(1.0, 0.0), 0.0);
        assert_eq!(warm_dark_interaction(1.0, 1.0), 0.0);
        let peak_ratio = 1.0 / std::f64::consts::E;
        assert_eq!(warm_dark_interaction(1.0, peak_ratio), 1.0);
        assert!(warm_dark_interaction(1.0, peak_ratio * 0.5) < 1.0);
        assert!(warm_dark_interaction(1.0, peak_ratio * 2.0) < 1.0);
    }

    #[test]
    fn measured_context_rejects_nonphysical_display_values() {
        let error = AppearanceContextV1::measured(
            AppearanceMode::EmissiveUi,
            ViewingConditions::srgb(),
            DisplayConditionsV1 {
                white_luminance_cd_m2: 100.0,
                black_luminance_cd_m2: 100.0,
                ambient_luminance_cd_m2: 0.0,
            },
            SpatialContextV1::default(),
        )
        .unwrap_err();
        assert!(error.contains("чёрного дисплея"));
    }

    #[test]
    fn audit_keeps_reference_relative_and_optional() {
        let context = AppearanceContextV1::nominal(
            AppearanceMode::EmissiveUi,
            ViewingConditions::srgb(),
        );
        let without = audit_color_quality_v1("#6B6B2E", "#FFFFFF", None, &context).unwrap();
        let with = audit_color_quality_v1(
            "#6B6B2E",
            "#FFFFFF",
            Some("#FFD60A"),
            &context,
        )
        .unwrap();
        assert!(without.relative_mutedness.is_none());
        assert!(with.relative_mutedness.is_some());
    }
}
