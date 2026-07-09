//! Версионированный анализ чистоты цвета и объективная геометрия гамута.
//!
//! В колориметрии нет стандартной наблюдаемой величины «грязность» и нет
//! независимого от наблюдателя универсального уравнения красоты, грязи или
//! приглушённости. Историческая модель перемножала порог хромы Oklab, окно
//! уникального жёлтого и член светлоты относительно cusp. Эти части происходят
//! из разных экспериментов и координатных систем, поэтому их произведение не
//! имело опубликованной психофизической модели.
//!
//! Поэтому объективная часть возвращает только точно воспроизводимые величины:
//! Oklab `(L, C, h)`, максимальную sRGB-хрому при тех же `(L, h)` и их отношение.
//! Оно отвечает лишь на вопрос «какая доля доступного радиуса sRGB занята?» и не
//! подменяет собой предпочтение или чистоту. Исходный Закон V1 остаётся под
//! прежним API как замороженная гипотеза и characterization-база; его смысл
//! никогда не переопределяется молча.

#[path = "cleanliness_legacy.rs"]
mod legacy_v1;

pub use legacy_v1::{
    B0, BW, C0, DefectContext, H_Y_DEG, JND, Theme, b_of, cusp_l_of, depth_mod, depth_term, drab,
    drab_in_context, hue_weight, muddiness_from_hex, muddiness_from_linear_srgb,
    muddiness_in_context, muddiness_oklch, n_pure, neutral_gate, raw_chromatic,
};

use crate::scale::max_chroma;
use crate::spaces::oklab::srgb_linear_to_oklab;
use crate::spaces::srgb::{srgb_from_hex, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;

/// Воспроизводимая геометрия одного цвета относительно гамута sRGB в Oklab.
///
/// Отдельный тип нужен, чтобы геометрию устройства нельзя было принять за
/// CAM16-colorfulness или оценку чистоты. У ахромата hue-зависимые величины
/// отсутствуют: произвольное направление 0° создало бы ложные данные.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct SrgbGamutGeometryV1 {
    /// Светлота Oklab.
    pub l_ok: f64,
    /// Хрома Oklab `hypot(a, b)`.
    pub c_ok: f64,
    /// Оттенок Oklab в градусах `[0, 360)`; отсутствует на ахроматической оси.
    pub h_ok: Option<f64>,
    /// Максимальная хрома Oklab внутри sRGB при тех же `(L, h)`; отсутствует,
    /// когда само направление оттенка не определено.
    pub max_srgb_chroma: Option<f64>,
    /// `C / C_max(L, h)`; отсутствует у неопределённого направления и в точке с
    /// нулевым радиусом. Значение выше единицы явно обозначает внегамутный вход.
    pub gamut_radius_fraction: Option<f64>,
    /// Неиспользованный радиус хромы `max(C_max - C, 0)`; у ахромата отсутствует.
    pub remaining_chroma_radius: Option<f64>,
    /// Принадлежность физическому кубу sRGB по той же радиальной границе, которой
    /// пользуется генератор; это исключает расхождение анализа и эмиссии.
    pub in_srgb_gamut: bool,
}

/// Геометрия гамута вместе с коррелятами CAM16 для заданного окружения и фона.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct ContextualSrgbGamutAnalysisV1 {
    pub gamut: SrgbGamutGeometryV1,
    pub cam16_j: f64,
    pub cam16_m: f64,
    pub cam16_h: f64,
    /// Фактическая относительная яркость фона `Y_b / Y_w` после проверки
    /// физического домена CAM16.
    pub background_y: f64,
    /// Метаданные IC-темы возвращаются отдельно, потому что у CAM16 нет входа
    /// «повышенный контраст» и подмешивать его в формулу было бы выдумкой.
    pub high_contrast: bool,
}

/// Стабильный идентификатор бескоэффициентной гипотезы грязи в CAM16-UCS.
pub const DIRT_MODEL_V2: &str = "lab-dirt-cam16ucs-v2";

/// Стабильный идентификатор относительной модели приглушённости.
pub const MUTEDNESS_MODEL_V2: &str = "lab-mutedness-cam16ucs-v2";

/// Отношение кандидата к хроматическому семейному якорю без абсолютного порога.
///
/// Абсолютный класс «мутный» потребовал бы эмпирически выбранной границы. Здесь
/// класс определяется только знаком точного изменения относительно переданного
/// якоря, поэтому он переносим между клиентами и не скрывает калибровку.
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

/// Отчёт о потере или росте относительной хроматичности против якоря.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct MutednessComparisonV2 {
    pub model: &'static str,
    pub reference_chroma_angle: f64,
    pub candidate_chroma_angle: f64,
    /// `ln(tan(theta_ref) / tan(theta_candidate))`: положительное значение
    /// означает потерю хроматичности. У ахроматических концов возвращается
    /// `None`, а их смысл несёт [`MutednessRelationV2`], без epsilon и infinity.
    pub log_relative_chroma_loss: Option<f64>,
    pub relation: MutednessRelationV2,
}

/// Причина активности или неактивности механизма «жёлтый × относительное почернение».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DirtApplicability {
    Active,
    AchromaticForeground,
    ExactBlackForeground,
    NonYellowOpponent,
    NoRelativeBlackening,
    /// Зарезервировано для чтения отчётов раннего кандидата V2. Финальная V2
    /// определяет относительное ослабление через радиусы двух фактических
    /// CAM16-UCS-стимулов и потому не делает разрыва между серым и почти-серым
    /// фоном. Новые вычисления этот вариант не возвращают.
    #[deprecated(note = "финальная V2 поддерживает хроматический локальный фон")]
    ChromaticBackground,
}

/// Дискретный результат операционального Закона Грязи V2 без численного порога.
///
/// Класс выводится из аналитических ветвей модели: `Dirty` означает строго
/// положительное взаимодействие «жёлтый × относительное почернение», `Clean` —
/// его точный ноль. Цветной фон не объявляется чистым или грязным, потому что
/// опубликованное основание модели не покрывает такой стимул.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DirtClassV2 {
    Clean,
    Dirty,
    OutsideModelDomain,
}

/// Две независимые координаты чистоты палитры.
///
/// `mutedness` — дополнение угла хромы CAM16-UCS; `dirtiness` описывает отдельный
/// механизм «жёлтый × индуцированная чернота». Разделение не даёт одной оси
/// скрыто объяснять другую. Ни одна величина не является вероятностью,
/// предпочтением или утверждением о красоте.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct DirtAnalysisV2 {
    pub model: &'static str,
    pub class: DirtClassV2,
    pub jp: f64,
    pub ap: f64,
    pub bp: f64,
    pub mp: f64,
    /// Евклидово расстояние CAM16-UCS от чёрного: `hypot(J′, M′)`.
    pub vividness_radius: f64,
    /// `atan2(M′, J′)` в радианах, диапазон `[0, π/2]`.
    pub chroma_angle: f64,
    /// `1 − 2·chroma_angle/π`; единица на ахроматической оси.
    pub mutedness: f64,
    /// Положительная проекция единичного вектора появления на жёлтую ось:
    /// `max(0, b′/hypot(J′, M′))`.
    ///
    /// В отличие от `b′/M′`, эта координата учитывает не только направление
    /// хроматического вектора, но и его угол к ахроматической оси. Поэтому она
    /// непрерывно стремится к нулю при исчезновении хромы и не объявляет
    /// однокодовый почти-серый столь же жёлтым, как насыщенный стимул.
    pub yellow_fraction: f64,
    /// `min(1, V_fg/V_bg)`, если радиус фактического локального фона ненулевой.
    pub foreground_background_ratio: Option<f64>,
    /// `−e·yellow_fraction·r·ln(r)` внутри домена модели.
    pub dirtiness_potential: Option<f64>,
    pub applicability: DirtApplicability,
}

#[derive(Debug, Clone, Copy)]
struct UcsAppearance {
    jp: f64,
    ap: f64,
    bp: f64,
    mp: f64,
    v: f64,
}

fn ucs_appearance(rgb: [f64; 3], vc: &ViewingConditions) -> UcsAppearance {
    if rgb == [0.0, 0.0, 0.0] {
        return UcsAppearance {
            jp: 0.0,
            ap: 0.0,
            bp: 0.0,
            mp: 0.0,
            v: 0.0,
        };
    }
    let (j, m, h) = crate::spaces::cam16::forward(srgb_to_xyz(rgb), vc);
    let jp = crate::spaces::cam16::ucs_j(j);
    // Равенство каналов определяет ахромат до матричного преобразования. Эта
    // ветвь не даёт округлению создать фиктивный opponent-вектор и случайный hue.
    let (mp, ap, bp) = if rgb[0] == rgb[1] && rgb[1] == rgb[2] {
        (0.0, 0.0, 0.0)
    } else {
        let mp = crate::spaces::cam16::ucs_m(m);
        let hr = h.to_radians();
        (mp, mp * hr.cos(), mp * hr.sin())
    };
    UcsAppearance {
        jp,
        ap,
        bp,
        mp,
        v: jp.hypot(mp),
    }
}

fn mutedness_from_ucs(jp: f64, mp: f64) -> (f64, f64) {
    let angle = mp.atan2(jp);
    (angle, 1.0 - 2.0 * angle / std::f64::consts::PI)
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

/// Сравнивает приглушённость двух sRGB-цветов при одинаковых условиях просмотра.
///
/// Якорь задаёт исходную идентичность семейства. Это делает «мутнее/чище»
/// относимым и проверяемым утверждением; функция не придумывает абсолютный
/// порог, которого нет в опубликованной модели.
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

fn dirt_interaction(yellow_fraction: f64, ratio: f64) -> f64 {
    if ratio == 0.0 || ratio == 1.0 || yellow_fraction == 0.0 {
        0.0
    } else {
        // Математический диапазон равен [0, yellow_fraction]. Ограничение лишь
        // возвращает результат в этот доказанный диапазон после округления f64.
        (-std::f64::consts::E * yellow_fraction * ratio * ratio.ln()).clamp(0.0, yellow_fraction)
    }
}

/// Анализирует Oklch-координату относительно той же границы sRGB, что и генератор.
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

/// Анализирует линейный sRGB; внегамутный вход отвергается, потому что clipping
/// превратил бы ошибку входа в правдоподобный, но другой цвет.
pub fn analyze_srgb_gamut_linear_v1(rgb: [f64; 3]) -> Result<SrgbGamutGeometryV1, String> {
    if !rgb
        .into_iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    {
        return Err(format!(
            "каналы линейного sRGB должны быть конечными и лежать внутри [0, 1]: {rgb:?}"
        ));
    }
    // Ахроматическая ось sRGB известна до матричного round-trip. Иначе конечная
    // точность напечатанных матриц создаёт у r=g=b малые a/b и произвольный hue.
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

/// Анализирует кодированный цвет sRGB `#RRGGBB`.
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

/// Возвращает CAM16 и геометрию Oklab рядом, но не смешивает их в одном уравнении:
/// это сохраняет физический смысл каждой координатной системы.
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

/// Анализирует гипотезу Закона Грязи Labpics V2 для цвета на локальном фоне.
///
/// Это новая фальсифицируемая операциональная дефиниция, а не вероятность,
/// подогнанная по наблюдателям. Компоненты опираются на литературу о
/// vividness/depth в CAM16-UCS `J′/M′` и на механизм «жёлтый + индуцированная
/// чернота» для коричневого. Их бескоэффициентное взаимодействие и нормировка —
/// новая, явно обозначенная гипотеза Labpics.
pub fn analyze_dirt_v2(
    foreground_hex: &str,
    background_hex: &str,
    vc: &ViewingConditions,
) -> Result<DirtAnalysisV2, String> {
    vc.validate()?;
    let foreground_rgb = srgb_from_hex(foreground_hex)?;
    let background_rgb = srgb_from_hex(background_hex)?;
    analyze_dirt_rgb_v2(foreground_rgb, background_rgb, vc)
}

/// Совместимая тематическая обёртка над [`analyze_dirt_v2`].
///
/// Первичный V2 API разделяет локальный фон-стимул и условия адаптации. Эта
/// обёртка намеренно связывает их для старого `DefectContext`, поэтому новый
/// клиентский код должен передавать реальные [`ViewingConditions`] явно.
pub fn analyze_dirt_for_theme_v2(
    foreground_hex: &str,
    ctx: DefectContext<'_>,
) -> Result<DirtAnalysisV2, String> {
    let background_rgb = srgb_from_hex(ctx.bg_hex)?;
    let requested_background_y = srgb_to_xyz(background_rgb)[1];
    let vc = contextual_vc(ctx.theme, requested_background_y);
    analyze_dirt_v2(foreground_hex, ctx.bg_hex, &vc)
}

fn analyze_dirt_rgb_v2(
    foreground_rgb: [f64; 3],
    background_rgb: [f64; 3],
    vc: &ViewingConditions,
) -> Result<DirtAnalysisV2, String> {
    vc.validate()?;
    if !foreground_rgb
        .into_iter()
        .chain(background_rgb)
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    {
        return Err("Dirt V2 требует конечные linear-sRGB каналы внутри [0, 1]".into());
    }

    let foreground = ucs_appearance(foreground_rgb, vc);
    let background = ucs_appearance(background_rgb, vc);

    let (chroma_angle, mutedness) = mutedness_from_ucs(foreground.jp, foreground.mp);
    // Это direction cosine полного вектора (J′, a′, b′), а не только
    // нормализация хроматической плоскости. Деление на V устраняет разрыв
    // прежнего кандидата b′/M′ при M′→0; в origin направление отсутствует и
    // проекция по определению равна нулю.
    let yellow_fraction = if foreground.v == 0.0 {
        0.0
    } else {
        (foreground.bp / foreground.v).max(0.0)
    };

    let (ratio, dirtiness_potential, applicability) = if foreground.v == 0.0 {
        (
            Some(0.0),
            Some(0.0),
            DirtApplicability::ExactBlackForeground,
        )
    } else if foreground.mp == 0.0 {
        (
            if background.v > 0.0 {
                Some((foreground.v / background.v).min(1.0))
            } else {
                None
            },
            Some(0.0),
            DirtApplicability::AchromaticForeground,
        )
    } else if yellow_fraction == 0.0 {
        (
            if background.v > 0.0 {
                Some((foreground.v / background.v).min(1.0))
            } else {
                None
            },
            Some(0.0),
            DirtApplicability::NonYellowOpponent,
        )
    } else if background.v == 0.0 {
        // Деление на нулевой радиус фона не определено. Нулевой потенциал здесь
        // следует не из выдуманного r=1, а из невозможности стать темнее чёрного.
        (None, Some(0.0), DirtApplicability::NoRelativeBlackening)
    } else if foreground.v >= background.v {
        (
            Some(1.0),
            Some(0.0),
            DirtApplicability::NoRelativeBlackening,
        )
    } else {
        let r = foreground.v / background.v;
        let dirt = dirt_interaction(yellow_fraction, r);
        (Some(r), Some(dirt), DirtApplicability::Active)
    };

    Ok(DirtAnalysisV2 {
        model: DIRT_MODEL_V2,
        class: match applicability {
            DirtApplicability::Active => DirtClassV2::Dirty,
            #[allow(deprecated)]
            DirtApplicability::ChromaticBackground => DirtClassV2::OutsideModelDomain,
            _ => DirtClassV2::Clean,
        },
        jp: foreground.jp,
        ap: foreground.ap,
        bp: foreground.bp,
        mp: foreground.mp,
        vividness_radius: foreground.v,
        chroma_angle,
        mutedness,
        yellow_fraction,
        foreground_background_ratio: ratio,
        dirtiness_potential,
        applicability,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_achromat_has_no_invented_radial_direction() {
        let a = analyze_srgb_gamut_oklch_v1(0.5, 0.0, 123.0).unwrap();
        assert_eq!(a.h_ok, None);
        assert_eq!(a.max_srgb_chroma, None);
        assert_eq!(a.gamut_radius_fraction, None);
        assert_eq!(a.remaining_chroma_radius, None);
        assert!(a.in_srgb_gamut);
    }

    #[test]
    fn gamut_wall_has_zero_relative_deficit_for_every_hue() {
        for li in 1..10 {
            let l = f64::from(li) / 10.0;
            for hi in 0..360 {
                let h = f64::from(hi);
                let c = max_chroma(l, h);
                let a = analyze_srgb_gamut_oklch_v1(l, c, h).unwrap();
                assert_eq!(a.gamut_radius_fraction, Some(1.0));
                assert_eq!(a.remaining_chroma_radius, Some(0.0));
                assert!(a.in_srgb_gamut);
            }
        }
    }

    #[test]
    fn equal_gamut_fractions_are_equal_without_hue_weighting() {
        let fraction = 0.5;
        for l in [0.2, 0.5, 0.8] {
            for h in (0..360).step_by(5).map(f64::from) {
                let a = analyze_srgb_gamut_oklch_v1(l, fraction * max_chroma(l, h), h).unwrap();
                assert_eq!(a.gamut_radius_fraction, Some(fraction));
            }
        }
    }

    #[test]
    fn invalid_continuous_inputs_are_rejected() {
        assert!(analyze_srgb_gamut_oklch_v1(f64::NAN, 0.0, 0.0).is_err());
        assert!(analyze_srgb_gamut_oklch_v1(0.5, -0.1, 0.0).is_err());
        assert!(analyze_srgb_gamut_linear_v1([0.0, 1.1, 0.0]).is_err());
        assert!(analyze_srgb_gamut_hex_v1("#GGGGGG").is_err());
    }

    #[test]
    fn all_encoded_grays_are_achromatic_and_never_activate_dirt_v2() {
        for byte in 0_u8..=u8::MAX {
            let hex = format!("#{byte:02X}{byte:02X}{byte:02X}");
            let report = analyze_dirt_for_theme_v2(
                &hex,
                DefectContext {
                    bg_hex: "#FFFFFF",
                    theme: Theme::Light,
                },
            )
            .unwrap();
            assert_eq!(report.mp, 0.0, "{hex}");
            assert_eq!(report.yellow_fraction, 0.0, "{hex}");
            assert_eq!(report.mutedness, 1.0, "{hex}");
            assert_eq!(report.dirtiness_potential, Some(0.0), "{hex}");
            assert_eq!(report.class, DirtClassV2::Clean, "{hex}");
            assert!(matches!(
                report.applicability,
                DirtApplicability::AchromaticForeground | DirtApplicability::ExactBlackForeground
            ));
        }
    }

    #[test]
    fn dirt_interaction_has_derived_endpoints_and_unique_peak() {
        assert_eq!(dirt_interaction(1.0, 0.0), 0.0);
        assert_eq!(dirt_interaction(1.0, 1.0), 0.0);
        let peak_ratio = 1.0 / std::f64::consts::E;
        assert_eq!(dirt_interaction(1.0, peak_ratio), 1.0);
        assert!(dirt_interaction(1.0, peak_ratio * 0.5) < 1.0);
        assert!(dirt_interaction(1.0, peak_ratio * 2.0) < 1.0);
        assert_eq!(dirt_interaction(0.0, peak_ratio), 0.0);
    }

    #[test]
    fn mutedness_is_hue_free_and_monotone_in_ucs_colorfulness() {
        let jp = 50.0;
        let (_, grey) = mutedness_from_ucs(jp, 0.0);
        let (_, medium) = mutedness_from_ucs(jp, 25.0);
        let (_, strong) = mutedness_from_ucs(jp, 50.0);
        assert_eq!(grey, 1.0);
        assert!(grey > medium && medium > strong);
    }

    #[test]
    fn relative_mutedness_loss_is_additive_without_a_threshold() {
        let appearance = |jp: f64, mp: f64| UcsAppearance {
            jp,
            ap: mp,
            bp: 0.0,
            mp,
            v: jp.hypot(mp),
        };
        let vivid = appearance(50.0, 40.0);
        let middle = appearance(50.0, 20.0);
        let muted = appearance(50.0, 10.0);

        let first = compare_ucs_mutedness(middle, vivid)
            .log_relative_chroma_loss
            .unwrap();
        let second = compare_ucs_mutedness(muted, middle)
            .log_relative_chroma_loss
            .unwrap();
        let direct = compare_ucs_mutedness(muted, vivid)
            .log_relative_chroma_loss
            .unwrap();
        let arithmetic_bound = 8.0 * f64::EPSILON;
        assert!((first - std::f64::consts::LN_2).abs() <= arithmetic_bound);
        assert!((second - std::f64::consts::LN_2).abs() <= arithmetic_bound);
        assert!((direct - (first + second)).abs() <= arithmetic_bound);
    }

    #[test]
    fn achromatic_mutedness_endpoints_are_states_not_infinities() {
        let vc = ViewingConditions::srgb();
        let same = compare_mutedness_v2("#808080", "#808080", &vc).unwrap();
        assert_eq!(same.relation, MutednessRelationV2::BothAchromatic);
        assert_eq!(same.log_relative_chroma_loss, None);

        let lost = compare_mutedness_v2("#808080", "#007AFF", &vc).unwrap();
        assert_eq!(lost.relation, MutednessRelationV2::CandidateAchromatic);
        assert_eq!(lost.log_relative_chroma_loss, None);
    }

    #[test]
    fn warm_mid_dark_stimulus_activates_but_cool_stimulus_does_not() {
        let ctx = DefectContext {
            bg_hex: "#FFFFFF",
            theme: Theme::Light,
        };
        let olive = analyze_dirt_for_theme_v2("#6B6B2E", ctx).unwrap();
        let teal = analyze_dirt_for_theme_v2("#008080", ctx).unwrap();
        assert_eq!(olive.applicability, DirtApplicability::Active);
        assert_eq!(olive.class, DirtClassV2::Dirty);
        assert!(olive.dirtiness_potential.unwrap() > 0.0);
        assert_eq!(teal.yellow_fraction, 0.0);
        assert_eq!(teal.dirtiness_potential, Some(0.0));
        assert_eq!(teal.applicability, DirtApplicability::NonYellowOpponent);
    }

    #[test]
    fn chromatic_background_uses_its_actual_ucs_radius_without_grey_substitution() {
        let report = analyze_dirt_for_theme_v2(
            "#6B6B2E",
            DefectContext {
                bg_hex: "#007AFF",
                theme: Theme::Light,
            },
        )
        .unwrap();
        assert_ne!(report.class, DirtClassV2::OutsideModelDomain);
        assert!(report.foreground_background_ratio.is_some());
        assert!(report.dirtiness_potential.is_some());
    }

    #[test]
    fn yellow_coordinate_is_continuous_toward_the_achromatic_axis() {
        let vc = ViewingConditions::srgb();
        let near_grey = analyze_dirt_v2("#80807F", "#FFFFFF", &vc).unwrap();
        assert!(near_grey.mp > 0.0);
        assert_eq!(
            near_grey.yellow_fraction.to_bits(),
            (near_grey.bp / near_grey.vividness_radius)
                .max(0.0)
                .to_bits()
        );
        assert!(near_grey.yellow_fraction <= near_grey.mp / near_grey.vividness_radius);
        assert!(near_grey.dirtiness_potential.unwrap() <= near_grey.yellow_fraction);
    }

    #[test]
    fn invalid_viewing_conditions_are_rejected_at_public_boundaries() {
        let mut invalid = ViewingConditions::srgb();
        invalid.c = f64::NAN;
        assert!(analyze_dirt_v2("#6B6B2E", "#FFFFFF", &invalid).is_err());
        assert!(compare_mutedness_v2("#6B6B2E", "#FFFFFF", &invalid).is_err());
    }

    #[test]
    fn black_background_has_no_invented_radius_ratio() {
        let report =
            analyze_dirt_v2("#6B6B2E", "#000000", &ViewingConditions::dim_surround()).unwrap();
        assert_eq!(report.foreground_background_ratio, None);
        assert_eq!(report.dirtiness_potential, Some(0.0));
        assert_eq!(
            report.applicability,
            DirtApplicability::NoRelativeBlackening
        );
    }

    #[test]
    fn primary_dirt_api_does_not_infer_adaptation_from_local_background() {
        let vc = ViewingConditions::srgb();
        let explicit = analyze_dirt_v2("#6B6B2E", "#FFFFFF", &vc).unwrap();
        let repeated = analyze_dirt_v2("#6B6B2E", "#FFFFFF", &vc).unwrap();
        assert_eq!(explicit, repeated);

        let contextual = analyze_dirt_for_theme_v2(
            "#6B6B2E",
            DefectContext {
                bg_hex: "#FFFFFF",
                theme: Theme::Light,
            },
        )
        .unwrap();
        assert_ne!(explicit.jp.to_bits(), contextual.jp.to_bits());
    }

    #[test]
    fn context_changes_cam16_but_not_device_gamut_geometry() {
        let light = analyze_srgb_gamut_hex_in_context_v1(
            "#6B6B2E",
            DefectContext {
                bg_hex: "#FFFFFF",
                theme: Theme::Light,
            },
        )
        .unwrap();
        let dark = analyze_srgb_gamut_hex_in_context_v1(
            "#6B6B2E",
            DefectContext {
                bg_hex: "#000000",
                theme: Theme::Dark,
            },
        )
        .unwrap();
        assert_eq!(light.gamut, dark.gamut);
        assert_ne!(light.cam16_j.to_bits(), dark.cam16_j.to_bits());
        assert_ne!(light.cam16_m.to_bits(), dark.cam16_m.to_bits());
    }

    #[test]
    fn increased_contrast_is_metadata_not_a_fake_cam16_parameter() {
        let plain = analyze_srgb_gamut_hex_in_context_v1(
            "#3E87FF",
            DefectContext {
                bg_hex: "#FFFFFF",
                theme: Theme::Light,
            },
        )
        .unwrap();
        let ic = analyze_srgb_gamut_hex_in_context_v1(
            "#3E87FF",
            DefectContext {
                bg_hex: "#FFFFFF",
                theme: Theme::LightIc,
            },
        )
        .unwrap();
        assert!(!plain.high_contrast && ic.high_contrast);
        assert_eq!(plain.cam16_j.to_bits(), ic.cam16_j.to_bits());
        assert_eq!(plain.cam16_m.to_bits(), ic.cam16_m.to_bits());
        assert_eq!(plain.cam16_h.to_bits(), ic.cam16_h.to_bits());
    }

    #[test]
    fn theme_keys_round_trip() {
        for theme in [Theme::Light, Theme::Dark, Theme::LightIc, Theme::DarkIc] {
            assert_eq!(Theme::parse(theme.key()).unwrap(), theme);
        }
        assert!(Theme::parse("unknown").is_err());
    }
}
