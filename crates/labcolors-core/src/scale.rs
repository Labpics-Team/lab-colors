use crate::lcs::LcsColor;
use crate::neutral::NeutralCurve;
use crate::spaces::cam16;
use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
use crate::spaces::srgb::{hex_from_srgb, srgb_from_hex, srgb_gamma, srgb_to_xyz};
use crate::spaces::vc::ViewingConditions;

/// Акцентная кривая: светлотный скелет — нейтральная кривая темы, оттенок и
/// насыщенность — от канонического цвета бренда.
///
/// Каждая ступень получает H-K-цель из фактически эмитированного состояния
/// [`NeutralCurve::at`], а затем выбирает ближайший допустимый sRGB8. Поэтому
/// закон сравнивает реальные токены, а не обещает недостижимое равенство двух
/// непрерывных float-точек после последующего округления.
#[derive(Debug, Clone)]
pub struct AccentCurve {
    neutral: NeutralCurve,
    /// Oklab-оттенок хранится только для публичного геометрического аксессора.
    h_canonical: f64,
    /// CAM16-оттенок, в котором строится изоуровень H-K.
    h_cam_canonical: f64,
    sat_ratio: f64,
    canonical_hex: String,
    vc: ViewingConditions,
}

impl AccentCurve {
    /// Кривая от канонического hex поверх H-K-светлотного скелета `neutral`.
    ///
    /// Канонический цвет задаёт CAM16-оттенок и долю его CAM16-хромы от первой
    /// физической границы на собственном изоуровне H-K. Та же безразмерная доля
    /// переносится на остальные уровни. Поэтому закон не смешивает Oklab и
    /// CAM16 и не содержит подобранного коэффициента насыщенности.
    pub fn new(canonical_hex: &str, neutral: &NeutralCurve) -> Result<Self, String> {
        let rgb = srgb_from_hex(canonical_hex)?;
        let identity = iso_hk_identity_from_anchor(canonical_hex, neutral.vc())?;

        Ok(Self {
            neutral: neutral.clone(),
            h_canonical: identity.h_ok,
            h_cam_canonical: identity.h_cam,
            sat_ratio: identity.chroma_ratio,
            canonical_hex: hex_from_srgb(rgb),
            vc: *neutral.vc(),
        })
    }

    /// Точка рампы при `t ∈ [0, 1]`.
    ///
    /// Сырой CAM16 J выводится прямо из равенства Hellwig 2022
    /// `J_HK = J + f(h)·C^0.587`; CAM16-оттенок, VC и относительная хрома в
    /// решении и выдаче одни и те же. После непрерывного решения выбирается
    /// детерминированный ближайший sRGB8, и наружу возвращается его повторно
    /// декодированный [`LcsColor`], а не скрыто клипнутая float-точка.
    pub fn try_at(&self, t: f64) -> Result<LcsColor, IsoHkError> {
        assert!(t.is_finite(), "параметр кривой t должен быть конечным");
        let t = t.clamp(0.0, 1.0);
        let neutral_color = self.neutral.at(t);
        quantized_iso_hk_for_neutral(
            &neutral_color,
            self.h_cam_canonical,
            self.sat_ratio,
            &self.vc,
        )
        .map(|resolved| resolved.color)
    }

    /// Совместимая инфаллибельная обёртка над [`Self::try_at`].
    ///
    /// Кривая, созданная [`Self::new`], обязана быть достижима на всём своём
    /// нейтральном скелете. Код с произвольными условиями просмотра должен
    /// вызывать вариант с `Result` и явно обрабатывать диагностическую ошибку.
    pub fn at(&self, t: f64) -> LcsColor {
        self.try_at(t)
            .expect("H-K-уровень акцента недостижим в физическом гамуте sRGB")
    }

    /// Условия просмотра, унаследованные от нейтральной кривой.
    pub fn vc(&self) -> &ViewingConditions {
        &self.vc
    }

    /// Oklab-оттенок канонического цвета (градусы) — идентичность семьи.
    pub fn canonical_hue(&self) -> f64 {
        self.h_canonical
    }

    /// Доля канонической CAM16-хромы от границы её изоуровня H-K, `[0, 1]`
    /// (см. [`AccentCurve::new`]).
    pub fn sat_ratio(&self) -> f64 {
        self.sat_ratio
    }

    /// CAM16-оттенок решателя изоуровня H-K, в градусах `[0, 360)`.
    pub fn canonical_cam16_hue(&self) -> f64 {
        self.h_cam_canonical
    }

    /// Исходный hex из [`AccentCurve::new`], нормализованный к верхнему регистру.
    pub fn canonical_hex(&self) -> &str {
        &self.canonical_hex
    }
}

/// H-K-светлота Hellwig 2022, представленная [`LcsColor`].
///
/// Значение восстанавливается из тех же CAM16-коррелятов, которые хранит LCS:
/// здесь нет лишнего обратного цикла через XYZ и сравнения разных координат светлоты.
pub(crate) fn perceived_lightness(color: &LcsColor, vc: &ViewingConditions) -> f64 {
    let j = cam16::ucs_j_inv(color.jp);
    let m = cam16::ucs_m_inv(color.mp());
    crate::lpc::j_hk_from_cam16(j, m, color.h_cam(), vc)
}

/// Единственная цветовая идентичность, выводимая из клиентского sRGB8-якоря.
///
/// Oklab hue остаётся описательным аксессором, а физическое построение использует
/// только сопряжённые CAM16 hue и C. `chroma_ratio` — доля C якоря от первой
/// связной границы его собственного iso-HK-уровня; значение никогда не клипуется.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IsoHkIdentity {
    pub h_ok: f64,
    pub h_cam: f64,
    pub chroma_ratio: f64,
}

pub(crate) fn iso_hk_identity_from_anchor(
    anchor_hex: &str,
    vc: &ViewingConditions,
) -> Result<IsoHkIdentity, String> {
    vc.validate()?;
    let rgb = srgb_from_hex(anchor_hex)?;
    let achromatic = rgb[0] == rgb[1] && rgb[1] == rgb[2];
    if achromatic {
        return Ok(IsoHkIdentity {
            h_ok: 0.0,
            h_cam: 0.0,
            chroma_ratio: 0.0,
        });
    }

    let lab = srgb_linear_to_oklab(rgb);
    let h_ok = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
    let anchor = LcsColor::from_hex_with_vc(anchor_hex, vc)?;
    let h_cam = anchor.h_cam();
    let anchor_c = cam16::ucs_m_inv(anchor.mp()) / vc.fl_pow_025;
    let target = perceived_lightness(&anchor, vc);
    let maximum = identity_radius_from_physical_anchor(target, h_cam, anchor_c, rgb, vc)?;
    if maximum <= 0.0 {
        return Err(format!(
            "хроматический якорь имеет C={anchor_c}, но физический радиус его H-K-уровня равен {maximum}"
        ));
    }
    let chroma_ratio = anchor_c / maximum;
    if !chroma_ratio.is_finite() || !(0.0..=1.0).contains(&chroma_ratio) {
        return Err(format!(
            "якорь лежит вне сертифицированного iso-HK-радиуса: C={anchor_c}, Cmax={maximum}, доля={chroma_ratio}"
        ));
    }

    Ok(IsoHkIdentity {
        h_ok,
        h_cam,
        chroma_ratio,
    })
}

/// Радиус iso-HK-компоненты с конструктивным физическим свидетелем.
///
/// Декодированный sRGB8 anchor сам доказывает `Cmax ≥ C_anchor`. На грани куба
/// обратный CAM16 может дать `1 + несколько ulp`, из-за чего строгий аналитический
/// solver возвращает соседний внутренний f64. Это не делает свидетель
/// внегамутным. Если такое расхождение возникло, поиск продолжается наружу от
/// `C_anchor`; последняя физическая binary64-точка и становится denominator.
fn identity_radius_from_physical_anchor(
    target: f64,
    h_cam: f64,
    anchor_c: f64,
    anchor_rgb: [f64; 3],
    vc: &ViewingConditions,
) -> Result<f64, String> {
    let solved = max_chroma_at_perceived_lightness(target, h_cam, vc)
        .map_err(|error| format!("якорь нельзя разместить на его H-K-уровне: {error}"))?;
    if solved >= anchor_c {
        return Ok(solved);
    }

    let on_cube_face = anchor_rgb
        .into_iter()
        .any(|channel| channel == 0.0 || channel == 1.0);
    if !on_cube_face {
        return Err(format!(
            "сертифицированный iso-HK-радиус меньше интерьерного физического anchor: C={anchor_c}, Cmax={solved}"
        ));
    }

    let first_outward = anchor_c.next_up();
    if !iso_hk_point_is_physical(target, h_cam, first_outward, vc) {
        return Ok(anchor_c);
    }

    let mut inside = first_outward;
    let mut span = first_outward - anchor_c;
    let outside = loop {
        span *= 2.0;
        let probe = anchor_c + span;
        if !probe.is_finite()
            || iso_hk_j(target, probe, h_cam, vc) <= 0.0
            || !iso_hk_point_is_physical(target, h_cam, probe, vc)
        {
            break probe;
        }
        inside = probe;
    };

    let mut outside = outside;
    while inside.next_up() < outside {
        let middle = inside + (outside - inside) * 0.5;
        if middle == inside || middle == outside {
            break;
        }
        if iso_hk_point_is_physical(target, h_cam, middle, vc) {
            inside = middle;
        } else {
            outside = middle;
        }
    }
    Ok(inside)
}

/// Измеряет H-K-светлоту именно эмитируемого `#RRGGBB`.
///
/// Равенство трёх декодированных каналов задаёт физический ахромат, поэтому для
/// него C принимается равной нулю точно. Это не позволяет матричному шуму CAT16
/// создавать ложный H-K-вклад у серого состояния конечной sRGB8-лестницы.
pub fn emitted_perceived_lightness(hex: &str, vc: &ViewingConditions) -> Result<f64, String> {
    vc.validate()?;
    emitted_perceived_lightness_unchecked(hex, vc)
}

fn emitted_perceived_lightness_unchecked(hex: &str, vc: &ViewingConditions) -> Result<f64, String> {
    let rgb = srgb_from_hex(hex)?;
    let (j, m, h_cam) = crate::lpc::cam16_jch_from_xyz(srgb_to_xyz(rgb), vc);
    if rgb[0] == rgb[1] && rgb[1] == rgb[2] {
        Ok(j)
    } else {
        Ok(crate::lpc::j_hk_from_cam16(j, m, h_cam, vc))
    }
}

/// Результат конечного выбора на sRGB8-решётке.
#[derive(Debug, Clone, Copy)]
pub(crate) struct QuantizedIsoHkColor {
    pub color: LcsColor,
    pub target_hk: f64,
    pub achieved_hk: f64,
    pub chroma_ratio: f64,
}

#[derive(Debug, Clone, Copy)]
struct QuantizedCandidate {
    color: LcsColor,
    bytes: [u8; 3],
    achieved_hk: f64,
    level_error: f64,
    ideal_delta_e: f64,
}

impl QuantizedCandidate {
    fn is_better_than(self, other: Self) -> bool {
        self.level_error
            .total_cmp(&other.level_error)
            .then_with(|| self.ideal_delta_e.total_cmp(&other.ideal_delta_e))
            .then_with(|| self.bytes.cmp(&other.bytes))
            .is_lt()
    }
}

/// Выбирает ближайшее представимое состояние для одного конечного уровня.
///
/// Непрерывная iso-HK-точка однозначно задаётся `(target, h_cam, ratio)`. Для
/// хроматического цвета рассматриваются все восемь вершин единственной
/// содержащей его sRGB8-ячейки. При нулевом радиусе исчерпываются все 256 серых:
/// это полный конечный кодомен ахромата, а не сеточная аппроксимация.
/// Лексикографическая цель: минимальная ошибка H-K-уровня, затем CAM16-UCS ΔE
/// до непрерывного идеала, затем меньшая тройка RGB-байтов как технический tie.
pub(crate) fn quantized_iso_hk_for_neutral(
    neutral: &LcsColor,
    h_cam: f64,
    chroma_ratio: f64,
    vc: &ViewingConditions,
) -> Result<QuantizedIsoHkColor, IsoHkError> {
    let neutral_hex = neutral.to_hex_with_vc(vc);
    let target_hk = emitted_perceived_lightness_unchecked(&neutral_hex, vc)
        .map_err(|_| IsoHkError::NumericalFailure)?;

    // В вершинах sRGB-куба хроматический радиус физически равен нулю. Возврат
    // исходного конечного состояния сохраняет его байты без обратного CAM-цикла.
    if neutral_hex == "#000000" || neutral_hex == "#FFFFFF" {
        return Ok(QuantizedIsoHkColor {
            color: *neutral,
            target_hk,
            achieved_hk: target_hk,
            chroma_ratio,
        });
    }

    let ideal = color_at_perceived_lightness(target_hk, h_cam, chroma_ratio, vc)?;
    let ideal_rgb = ideal.to_linear_srgb(vc);
    if !in_physical_srgb(ideal_rgb) {
        return Err(IsoHkError::NumericalFailure);
    }

    let mut best: Option<QuantizedCandidate> = None;
    let mut consider = |bytes: [u8; 3]| -> Result<(), IsoHkError> {
        let hex = format!("#{:02X}{:02X}{:02X}", bytes[0], bytes[1], bytes[2]);
        let color =
            LcsColor::from_hex_with_vc(&hex, vc).map_err(|_| IsoHkError::NumericalFailure)?;
        let achieved_hk = emitted_perceived_lightness_unchecked(&hex, vc)
            .map_err(|_| IsoHkError::NumericalFailure)?;
        let candidate = QuantizedCandidate {
            color,
            bytes,
            achieved_hk,
            level_error: (achieved_hk - target_hk).abs(),
            ideal_delta_e: color.delta_e_ucs(&ideal),
        };
        if best.is_none_or(|current| candidate.is_better_than(current)) {
            best = Some(candidate);
        }
        Ok(())
    };

    if chroma_ratio == 0.0 {
        // При нулевом радиусе hue не существует. Полный представимый кодомен —
        // ровно 256 серых, поэтому исчерпывающий поиск одновременно быстрее и
        // строже локальной RGB-ячейки: цветной байтовый шум невозможен.
        for byte in 0_u16..=255 {
            let byte = byte as u8;
            consider([byte, byte, byte])?;
        }
    } else {
        let mut bounds = [[0_u8; 2]; 3];
        for channel in 0..3 {
            let scaled = srgb_gamma(ideal_rgb[channel]) * 255.0;
            if !scaled.is_finite() || !(0.0..=255.0).contains(&scaled) {
                return Err(IsoHkError::NumericalFailure);
            }
            bounds[channel] = [scaled.floor() as u8, scaled.ceil() as u8];
        }
        for mask in 0_u8..8 {
            consider([
                bounds[0][usize::from(mask & 1)],
                bounds[1][usize::from((mask >> 1) & 1)],
                bounds[2][usize::from((mask >> 2) & 1)],
            ])?;
        }
    }

    let best = best.ok_or(IsoHkError::NumericalFailure)?;
    Ok(QuantizedIsoHkColor {
        color: best.color,
        target_hk,
        achieved_hk: best.achieved_hk,
        chroma_ratio,
    })
}

/// Ошибка построения физического sRGB-цвета на заданном изоуровне Hellwig 2022.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IsoHkError {
    /// Запрошенная H-K-светлота равна NaN или бесконечности.
    NonFiniteTarget,
    /// CAM16-оттенок не конечен.
    NonFiniteHue,
    /// Относительная хрома не конечна либо лежит вне `[0, 1]`.
    InvalidChromaRatio,
    /// Ахроматическое начало лежит ниже физического чёрного.
    BelowBlack { target: f64 },
    /// При данном CAM16-оттенке нет физического цвета с такой H-K-светлотой.
    NoPhysicalColor { target: f64, h_cam: f64 },
    /// Численный расчёт не смог сертифицировать связную физическую компоненту.
    NumericalFailure,
}

impl std::fmt::Display for IsoHkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::NonFiniteTarget => write!(f, "H-K-светлота должна быть конечной"),
            Self::NonFiniteHue => write!(f, "CAM16-оттенок должен быть конечным"),
            Self::InvalidChromaRatio => {
                write!(
                    f,
                    "относительная CAM16-хрома должна быть конечной и лежать в [0, 1]"
                )
            }
            Self::BelowBlack { target } => {
                write!(f, "H-K-светлота {target} лежит ниже физического чёрного")
            }
            Self::NoPhysicalColor { target, h_cam } => write!(
                f,
                "H-K-светлота {target} недостижима в sRGB при CAM16-оттенке {h_cam}"
            ),
            Self::NumericalFailure => write!(
                f,
                "не удалось сертифицировать связный физический интервал sRGB"
            ),
        }
    }
}

impl std::error::Error for IsoHkError {}

/// Максимальная CAM16-хрома первой связной физической части изоуровня H-K.
///
/// Вся кривая параметризуется в одной модели восприятия и при одних условиях
/// просмотра:
///
/// `J(C) = J_HK_target - f(h) * C^0.587`, `M(C) = C * F_L^0.25`.
///
/// `f(h)` и показатель вычисляет [`crate::lpc::j_hk_from_cam16`] — единственный
/// источник опубликованного уравнения Hellwig 2022. Здесь нет Oklab-светлоты,
/// Oklab-оттенка, сероосевого приближения и перцептивного допуска. Возвращается
/// последнее представимое внутригамутное `C` перед первым выходом из первой
/// физической компоненты; при `J_HK` выше ахроматического белого её начало может
/// быть больше нуля.
pub(crate) fn max_chroma_at_perceived_lightness(
    target: f64,
    h_cam: f64,
    vc: &ViewingConditions,
) -> Result<f64, IsoHkError> {
    validate_iso_hk_inputs(target, h_cam)?;
    if target < 0.0 {
        return Err(IsoHkError::BelowBlack { target });
    }
    let white = white_perceived_lightness(vc);

    // У чёрного и белого ахроматическое начало уже лежит на грани куба. Значит,
    // первая связная граница равна C=0 точно: конечная точка остаётся точкой, а не
    // искусственным цветным ореолом.
    if target == 0.0 || target == white {
        return Ok(0.0);
    }

    let h_cam = h_cam.rem_euclid(360.0);
    physical_chroma_interval(target, h_cam, vc).map(|(_, maximum)| maximum)
}

/// Построить цвет на заданной доле физического CAM16-радиуса изоуровня H-K.
pub(crate) fn color_at_perceived_lightness(
    target: f64,
    h_cam: f64,
    chroma_ratio: f64,
    vc: &ViewingConditions,
) -> Result<LcsColor, IsoHkError> {
    validate_iso_hk_inputs(target, h_cam)?;
    if !chroma_ratio.is_finite() || !(0.0..=1.0).contains(&chroma_ratio) {
        return Err(IsoHkError::InvalidChromaRatio);
    }

    let white = white_perceived_lightness(vc);
    if target < 0.0 {
        return Err(IsoHkError::BelowBlack { target });
    }
    if target == 0.0 || target == white {
        return if target == 0.0 {
            Ok(LcsColor::from_cam16(0.0, 0.0, 0.0, 0.0))
        } else {
            Ok(white_lcs(vc))
        };
    }

    let h_cam = h_cam.rem_euclid(360.0);
    let (minimum, maximum) = physical_chroma_interval(target, h_cam, vc)?;
    let c = chroma_ratio * maximum;
    if c < minimum {
        return Err(IsoHkError::NoPhysicalColor { target, h_cam });
    }
    let color = iso_hk_color_unchecked(target, h_cam, c, vc);
    if in_physical_srgb(color.to_linear_srgb(vc)) {
        Ok(color)
    } else {
        Err(IsoHkError::NumericalFailure)
    }
}

fn validate_iso_hk_inputs(target: f64, h_cam: f64) -> Result<(), IsoHkError> {
    if !target.is_finite() {
        return Err(IsoHkError::NonFiniteTarget);
    }
    if !h_cam.is_finite() {
        return Err(IsoHkError::NonFiniteHue);
    }
    Ok(())
}

fn white_lcs(vc: &ViewingConditions) -> LcsColor {
    LcsColor::from_xyz_with_hok(srgb_to_xyz([1.0, 1.0, 1.0]), 0.0, vc)
}

fn white_perceived_lightness(vc: &ViewingConditions) -> f64 {
    // Белый sRGB — ахроматический стимул, поэтому в уравнении H-K C=0 и
    // J_HK=J. Мелкий ненулевой M, возникающий из печатной точности матриц
    // CAT16, не должен превращаться в физический цветовой вклад.
    crate::lpc::cam16_jch_from_xyz(srgb_to_xyz([1.0, 1.0, 1.0]), vc).0
}

/// Сырой CAM16 J, однозначно следующий из равенства H-K при хроме `C`.
fn iso_hk_j(target: f64, c: f64, h_cam: f64, vc: &ViewingConditions) -> f64 {
    let m = c * vc.fl_pow_025;
    target - crate::lpc::j_hk_from_cam16(0.0, m, h_cam, vc)
}

fn iso_hk_color_unchecked(target: f64, h_cam: f64, c: f64, vc: &ViewingConditions) -> LcsColor {
    let j = iso_hk_j(target, c, h_cam, vc);
    assert!(
        j >= 0.0,
        "внутренний инвариант iso-HK нарушен: C вышла за границу J=0"
    );
    let m = c * vc.fl_pow_025;
    if c == 0.0 {
        // В ахроматическом начале оттенок не определён. Храним канонический ноль,
        // а не входной оттенок у точки с нулевым радиусом.
        LcsColor::from_cam16(j, 0.0, 0.0, 0.0)
    } else {
        LcsColor::from_ucs_polar(cam16::ucs_j(j), cam16::ucs_m(m), h_cam, vc)
    }
}

fn in_physical_srgb(rgb: [f64; 3]) -> bool {
    rgb.into_iter()
        .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
}

struct IsoHkContext<'a> {
    target: f64,
    h_cam: f64,
    vc: &'a ViewingConditions,
    cos_h: f64,
    sin_h: f64,
    p1: f64,
    cone_to_rgb: [[f64; 3]; 3],
}

impl<'a> IsoHkContext<'a> {
    fn new(target: f64, h_cam: f64, vc: &'a ViewingConditions) -> Self {
        let hr = h_cam.to_radians();
        let cos_h = hr.cos();
        let sin_h = hr.sin();
        let e_hue = 0.25 * ((hr + 2.0).cos() + 3.8);
        let p1 = e_hue * (50000.0 / 13.0) * vc.nc * vc.nbb;

        let mut cone_to_rgb = [[0.0_f64; 3]; 3];
        for basis in 0..3 {
            let mut lms = [0.0_f64; 3];
            lms[basis] = 1.0 / vc.rgb_d[basis];
            let xyz_100 = crate::spaces::cat16::cone_to_xyz(lms);
            let rgb = crate::spaces::srgb::xyz_to_srgb([
                xyz_100[0] / 100.0,
                xyz_100[1] / 100.0,
                xyz_100[2] / 100.0,
            ]);
            for channel in 0..3 {
                cone_to_rgb[channel][basis] = rgb[channel];
            }
        }

        Self {
            target,
            h_cam,
            vc,
            cos_h,
            sin_h,
            p1,
            cone_to_rgb,
        }
    }
}

/// Первый связный интервал физической CAM16-хромы на изоуровне H-K.
///
/// Если `J_HK` выше ахроматического белого, `C=0` находится вне sRGB, но цветная
/// точка всё ещё может быть физической: вклад H-K позволяет уменьшить сырой `J`.
/// Поэтому ищутся и первый вход, и первый выход; начало компоненты не обязано
/// совпадать с нулём.
fn physical_chroma_interval(
    target: f64,
    h_cam: f64,
    vc: &ViewingConditions,
) -> Result<(f64, f64), IsoHkError> {
    let context = IsoHkContext::new(target, h_cam, vc);
    // Ищем конечную верхнюю точку с J(C)<=0. Начальная единица имеет размерность
    // одной CAM16-C, а удвоение исчерпывающе проходит двоичные порядки и не
    // задаёт подогнанный потолок хромы. f(h) Hellwig положителен при любом
    // оттенке, поэтому для положительной цели поиск обязательно завершается.
    let mut upper = 1.0_f64;
    while iso_hk_j(target, upper, h_cam, vc) > 0.0 {
        upper *= 2.0;
        if !upper.is_finite() {
            return Err(IsoHkError::NumericalFailure);
        }
    }

    let mut pending = vec![(0.0_f64, upper)];
    let mut first = None;
    let mut last = None;
    let mut last_seen = None;

    while let Some((lo, hi)) = pending.pop() {
        match classify_iso_hk_interval(&context, lo, hi) {
            IntervalClass::Inside => {
                first.get_or_insert(lo);
                last = Some(hi);
                continue;
            }
            IntervalClass::Outside => {
                if let (Some(start), Some(end)) = (first, last) {
                    return Ok((start, end));
                }
                continue;
            }
            IntervalClass::Unresolved => {}
        }

        let mid = lo + (hi - lo) * 0.5;
        if mid == lo || mid == hi {
            for point in [lo, hi] {
                if last_seen == Some(point.to_bits()) {
                    continue;
                }
                last_seen = Some(point.to_bits());
                if iso_hk_point_is_physical(target, h_cam, point, vc) {
                    first.get_or_insert(point);
                    last = Some(point);
                } else if let (Some(start), Some(end)) = (first, last) {
                    return Ok((start, end));
                }
            }
            continue;
        }

        // Стек LIFO: правую половину кладём первой, чтобы полностью
        // сертифицировать левую до рассмотрения любой большей хромы.
        pending.push((mid, hi));
        pending.push((lo, mid));
    }

    if let (Some(start), Some(end)) = (first, last) {
        Ok((start, end))
    } else {
        Err(IsoHkError::NoPhysicalColor { target, h_cam })
    }
}

fn iso_hk_point_is_physical(target: f64, h_cam: f64, c: f64, vc: &ViewingConditions) -> bool {
    let j = iso_hk_j(target, c, h_cam, vc);
    if j <= 0.0 && c > 0.0 {
        return false;
    }
    in_physical_srgb(iso_hk_color_unchecked(target, h_cam, c, vc).to_linear_srgb(vc))
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    lo: f64,
    hi: f64,
}

impl Bounds {
    fn point(value: f64) -> Self {
        Self {
            lo: value,
            hi: value,
        }
    }

    fn outward(lo: f64, hi: f64) -> Self {
        Self {
            lo: if lo.is_finite() { lo.next_down() } else { lo },
            hi: if hi.is_finite() { hi.next_up() } else { hi },
        }
    }

    fn add(self, other: Self) -> Self {
        Self::outward(self.lo + other.lo, self.hi + other.hi)
    }

    fn sub(self, other: Self) -> Self {
        Self::outward(self.lo - other.hi, self.hi - other.lo)
    }

    fn mul(self, other: Self) -> Self {
        let products = [
            self.lo * other.lo,
            self.lo * other.hi,
            self.hi * other.lo,
            self.hi * other.hi,
        ];
        let lo = products.into_iter().fold(f64::INFINITY, f64::min);
        let hi = products.into_iter().fold(f64::NEG_INFINITY, f64::max);
        Self::outward(lo, hi)
    }

    fn div(self, other: Self) -> Option<Self> {
        if other.lo <= 0.0 && other.hi >= 0.0 {
            return None;
        }
        Some(self.mul(Self::outward(1.0 / other.hi, 1.0 / other.lo)))
    }

    fn powf(self, exponent: f64) -> Option<Self> {
        if self.lo < 0.0 || !(exponent.is_finite() && exponent > 0.0) {
            return None;
        }
        Some(Self::outward(
            self.lo.powf(exponent),
            self.hi.powf(exponent),
        ))
    }
}

/// Внешняя интервальная оболочка прямой CAM16-инверсии на интервале хромы.
///
/// Коэффициенты матриц получаются применением канонических линейных
/// CAT16/XYZ/sRGB-преобразований крейта к базисным векторам. Поэтому здесь нет
/// второй копии опубликованных матриц, способной разойтись с основным путём.
fn iso_hk_rgb_bounds(context: &IsoHkContext<'_>, c_lo: f64, c_hi: f64) -> Option<[Bounds; 3]> {
    let target = context.target;
    let h_cam = context.h_cam;
    let vc = context.vc;
    let c = Bounds { lo: c_lo, hi: c_hi };
    let j = Bounds::outward(
        iso_hk_j(target, c_hi, h_cam, vc),
        iso_hk_j(target, c_lo, h_cam, vc),
    );
    if j.lo <= 0.0 || !j.hi.is_finite() {
        return None;
    }

    let m = c.mul(Bounds::point(vc.fl_pow_025));
    let j_fraction = j.div(Bounds::point(100.0))?;
    let sqrt_j = j_fraction.powf(0.5)?;
    let t_denominator = sqrt_j
        .mul(Bounds::point(vc.t_inner))
        .mul(Bounds::point(vc.fl_pow_025));
    let t = m.div(t_denominator)?.powf(1.0 / 0.9)?;

    let cos_h = context.cos_h;
    let sin_h = context.sin_h;
    let p1 = context.p1;
    let p2 = Bounds::point(vc.aw)
        .mul(j_fraction.powf(1.0 / (vc.c * vc.z))?)
        .div(Bounds::point(vc.nbb))?;
    let gamma_numerator = Bounds::point(23.0).mul(p2.add(Bounds::point(0.305))).mul(t);
    let gamma_denominator =
        Bounds::point(23.0 * p1).add(t.mul(Bounds::point(11.0 * cos_h + 108.0 * sin_h)));
    let gamma = gamma_numerator.div(gamma_denominator)?;
    let a = gamma.mul(Bounds::point(cos_h));
    let b = gamma.mul(Bounds::point(sin_h));

    let r_a = Bounds::point(460.0)
        .mul(p2)
        .add(Bounds::point(451.0).mul(a))
        .add(Bounds::point(288.0).mul(b))
        .div(Bounds::point(1403.0))?;
    let g_a = Bounds::point(460.0)
        .mul(p2)
        .sub(Bounds::point(891.0).mul(a))
        .sub(Bounds::point(261.0).mul(b))
        .div(Bounds::point(1403.0))?;
    let b_a = Bounds::point(460.0)
        .mul(p2)
        .sub(Bounds::point(220.0).mul(a))
        .sub(Bounds::point(6300.0).mul(b))
        .div(Bounds::point(1403.0))?;

    let adapted = [r_a, g_a, b_a];
    let mut cone = [Bounds::point(0.0); 3];
    for (slot, value) in cone.iter_mut().zip(adapted) {
        if value.lo <= -400.0 || value.hi >= 400.0 {
            return None;
        }
        *slot = Bounds::outward(
            cam16::unadapt(value.lo, vc.fl),
            cam16::unadapt(value.hi, vc.fl),
        );
    }

    let mut rgb = [Bounds::point(0.0); 3];
    for (channel, output) in rgb.iter_mut().enumerate() {
        for (basis, component) in cone.iter().enumerate() {
            *output = output.add(component.mul(Bounds::point(context.cone_to_rgb[channel][basis])));
        }
    }
    Some(rgb)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntervalClass {
    Inside,
    Outside,
    Unresolved,
}

fn classify_iso_hk_interval(context: &IsoHkContext<'_>, c_lo: f64, c_hi: f64) -> IntervalClass {
    let target = context.target;
    let h_cam = context.h_cam;
    let vc = context.vc;
    // J строго убывает с C. После неположительной левой границы ни одна более
    // поздняя точка не может представлять цвет с положительной красочностью.
    if iso_hk_j(target, c_lo, h_cam, vc) <= 0.0 && c_lo > 0.0 {
        return IntervalClass::Outside;
    }

    let Some(rgb) = iso_hk_rgb_bounds(context, c_lo, c_hi) else {
        return IntervalClass::Unresolved;
    };
    if rgb
        .into_iter()
        .all(|channel| channel.lo >= 0.0 && channel.hi <= 1.0)
    {
        IntervalClass::Inside
    } else if rgb
        .into_iter()
        .any(|channel| channel.hi < 0.0 || channel.lo > 1.0)
    {
        IntervalClass::Outside
    } else {
        IntervalClass::Unresolved
    }
}

/// Argmax index over `0..=steps` found coarse-to-fine, reproducing a flat 1° scan
/// bit-for-bit.
///
/// `score(i)` MUST be the exact per-index score a flat scan would compute (same
/// arithmetic); the only thing that changes is WHICH indices are visited. A
/// coarse pass on the `coarse`-degree grid maps the ridge; the winner is then
/// chosen by a SINGLE ascending pass over the candidate indices with the same
/// strict-`>` first-maximum tie-break the flat scan uses — so the returned index
/// is identical to the flat scan's whenever the flat argmax is a candidate.
///
/// Candidates are every coarse sample PLUS every 1° index within `±bracket` of a
/// coarse LOCAL MAXIMUM (a coarse sample no lower than its coarse neighbours).
/// Refining around *all* coarse local maxima — not just the global coarse argmax
/// — is what makes the bimodal accent/tint score safe: its two peaks (the
/// canonical drift-hump and the gamut-cusp chroma-hump) can sit farther apart
/// than one coarse step, so a single bracket around the coarse argmax would miss
/// the other peak. Pinned bit-for-bit on the full 180k-point grid by diff test B
/// / the cusp diff test, and on the real (non-integer-hue) accent/tint inputs by
/// the golden and 240-cell byte-identity snapshots. Shared with the semantic tint
/// cusp sweep, hence `pub(crate)`.
///
/// # Preconditions (a caller that breaks any of these gets a silently wrong index)
///
/// 1. **`score(i)` is the EXACT arithmetic a flat 1° scan would compute** for
///    index `i` (same operations, same order). Only WHICH indices are visited may
///    change; the per-index value must not, or the tie-break diverges.
/// 2. **The coarse grid fits the fixed buffer:** `steps / coarse + 2 ≤ 64`
///    samples (`debug_assert`ed). A larger grid would truncate silently.
/// 3. **Every peak of `score` is reachable within `±bracket` of a coarse local
///    maximum.** If the flat argmax can sit farther than `bracket` from every
///    coarse local maximum it is not a candidate and the result diverges from the
///    flat scan. For this crate's accent (±30°) and tint (±40°) windows a
///    full-grid measurement fixed that distance at ≤ 13°, so `bracket = 15`
///    holds; a new caller must re-establish this bound for its own score.
pub(crate) fn coarse_to_fine_argmax(
    steps: i32,
    coarse: i32,
    bracket: i32,
    mut score: impl FnMut(i32) -> f64,
) -> i32 {
    // Phase 1 — coarse scan, cached (each coarse `max_chroma` solved once).
    // Fixed 64-slot buffers keep this allocation-free; 64 covers any window this
    // crate sweeps (≤ 80° at a 5° coarse step → 17 samples). The size is inlined
    // (no named const) so the frozen policy-const audit never sees it.
    let mut ci = [0i32; 64];
    let mut cs = [f64::NEG_INFINITY; 64];
    // Guard the fixed-buffer contract in debug builds (compiled out of release,
    // where callers pass known-good constants): a coarse step < 1 would not
    // progress the scan (an infinite/stuck loop or wrong grid), and a grid larger
    // than the 64-slot buffer would truncate silently — a wrong argmax with no
    // panic. Silently-wrong is forbidden; fail loud in debug. The `coarse >= 1`
    // check runs first so the capacity division below is never by zero.
    debug_assert!(coarse >= 1, "coarse step must be ≥ 1 (got {coarse})");
    debug_assert!(
        (steps.max(0) / coarse + 2) as usize <= ci.len(),
        "coarse grid ({} samples) overflows the {}-slot buffer (steps={steps}, coarse={coarse})",
        steps.max(0) / coarse + 2,
        ci.len(),
    );
    let mut nc = 0usize;
    let mut i = 0;
    while i <= steps && nc < ci.len() {
        ci[nc] = i;
        cs[nc] = score(i);
        nc += 1;
        i += coarse;
    }
    // Guarantee the top endpoint is a coarse sample even on an unaligned window.
    if nc > 0 && nc < ci.len() && ci[nc - 1] != steps {
        ci[nc] = steps;
        cs[nc] = score(steps);
        nc += 1;
    }

    // A coarse sample is a local maximum when it is no lower than its coarse
    // neighbours (endpoints compared to their single neighbour).
    let is_local_max = |p: usize| -> bool {
        let left = p == 0 || cs[p] >= cs[p - 1];
        let right = p + 1 >= nc || cs[p] >= cs[p + 1];
        left && right
    };

    // Phase 2 — single ascending pass with the flat scan's strict-`>` tie-break.
    let mut win_i = 0;
    let mut win_s = f64::NEG_INFINITY;
    let mut c = 0usize; // cursor: ci[c] is the greatest coarse index ≤ idx
    for idx in 0..=steps {
        while c + 1 < nc && ci[c + 1] <= idx {
            c += 1;
        }
        let s = if idx == ci[c] {
            cs[c] // a coarse sample — reuse its cached score, no re-solve
        } else if (0..nc).any(|p| is_local_max(p) && (idx - ci[p]).abs() <= bracket) {
            score(idx) // a 1° refinement near a coarse local maximum
        } else {
            continue; // provably off-ridge — skip
        };
        if s > win_s {
            win_s = s;
            win_i = idx;
        }
    }
    win_i
}

/// Oklab L of the grey whose CAM16-UCS lightness J' equals `jp`, in closed form.
///
/// # Derivation (mirror of `lpc::y_hk_analytic`)
///
/// `AccentCurve::at` calls this once per stretch point to anchor the accent's
/// lightness on the same grey axis the neutral curve defines. The forward map it
/// inverts is the achromatic chain
///
/// ```text
///   y  ──grey_j──▶  J  ──UCS──▶  J' = 1.7·J / (1 + 0.007·J)
/// ```
///
/// followed by `L_ok = srgb_linear_to_oklab([y, y, y])[0]`. Every link is a
/// strictly increasing bijection, so the whole map is invertible:
///
/// 1. **J' → J** — the CAM16-UCS lightness rescale (Li et al. 2017,
///    DOI 10.1002/col.22131) inverts in closed form:
///    `jp·(1 + 0.007·J) = 1.7·J  ⇒  J = jp / (1.7 − 0.007·jp)`. This is the same
///    inverse `lpc::y_hk_from_lcs` already uses for the LcsColor contrast path.
/// 2. **J → y** — on the achromatic D65 ray, chroma is zero, so the Hellwig H-K
///    term vanishes and `J_HK ≡ J`. Recovering the grey luminance from `J` is
///    therefore *exactly* `lpc::y_hk(J, vc)` — the analytic CAM16 grey-axis
///    inverse (closed-form seed + two Newton steps) that replaced an identical
///    64-step bisection in PR #51. Reused verbatim here, no second copy of the
///    cone-response algebra.
/// 3. **y → L_ok** — for a grey `[y, y, y]` linear-sRGB triple,
///    `srgb_linear_to_oklab` collapses to a single cube root scaled by the
///    near-unity matrix row sums (`SRGB_TO_LMS` rows ≈ 1, `LMS_TO_OKLAB` row 0 ≈
///    1 but **not exactly** — 0.9999999935). The closed form is still evaluated
///    through the very same `srgb_linear_to_oklab([y, y, y])` call the bisection
///    used, so the emitted L carries byte-identical rounding and the accent
///    golden snapshot does not drift.
///
/// Replacing the 64 forward CAM16 passes with one analytic `y_hk` is the only
/// behavioural change; everything downstream of `y` is unchanged.
///
/// `pub(crate)` so the semantic dJ' contract (decorative perceived-lightness
/// difference, `surface-jnd`) can map a target CAM16-UCS lightness `J'` onto the
/// Oklab `L` the solver's `build_color` consumes — the same grey-axis inverse the
/// accent curve uses, never a second copy of the rescale algebra.
pub(crate) fn jp_to_oklab_l(jp: f64, vc: &ViewingConditions) -> f64 {
    // Step 1: invert the CAM16-UCS lightness rescale J' → J through the shared
    // single-source helper (#19/#60) — never re-type the rescale constants here.
    // J' is bounded above by the rescale's horizontal asymptote (1.7/0.007 ≈
    // 242.86); at or past it `ucs_j_inv` has a non-positive denominator and
    // returns a non-finite or non-positive J, so the grey saturates at white,
    // exactly as the bisection's hi = 1.0 cap did.
    if jp <= 0.0 {
        return srgb_linear_to_oklab([0.0, 0.0, 0.0])[0];
    }
    let j = cam16::ucs_j_inv(jp);
    if !j.is_finite() || j <= 0.0 {
        return srgb_linear_to_oklab([1.0, 1.0, 1.0])[0];
    }

    // Step 2: invert the achromatic CAM16 chain J → y. On the grey axis chroma
    // is zero, so J_HK ≡ J and the H-K-corrected grey inverse is the plain one.
    let y = crate::lpc::y_hk(j, vc);

    // Step 3: grey Oklab L through the identical forward function the bisection
    // used — keeps the emitted lightness bit-for-bit, so the accent golden holds.
    srgb_linear_to_oklab([y, y, y])[0]
}

/// Render an Oklab `(L, C, hue)` point to an [`LcsColor`] under viewing
/// conditions `vc` — the SINGLE source of the `Oklab → linear sRGB → XYZ →
/// CAM16-UCS` assembly the accent-surface and accent-balance primitives share
/// (no second copy of the rescale chain).
///
/// The caller guarantees `(l_ok, c_ok)` is in gamut (e.g. `c_ok ≤`
/// [`max_chroma`]); the per-channel `clamp` is only machine-noise insurance at
/// the gamut wall and a no-op strictly inside it. VC enters only the CAM16
/// projection, exactly as the accent curve's emission does.
pub(crate) fn lcs_from_oklab_lch(
    l_ok: f64,
    c_ok: f64,
    hue_deg: f64,
    vc: &ViewingConditions,
) -> LcsColor {
    let h_rad = hue_deg.to_radians();
    let a_ok = c_ok * h_rad.cos();
    let b_ok = c_ok * h_rad.sin();

    let rgb = oklab_to_srgb_linear([l_ok, a_ok, b_ok]);
    let rgb_clamped = [
        rgb[0].clamp(0.0, 1.0),
        rgb[1].clamp(0.0, 1.0),
        rgb[2].clamp(0.0, 1.0),
    ];

    let xyz = srgb_to_xyz(rgb_clamped);
    let h_ok = b_ok.atan2(a_ok).to_degrees().rem_euclid(360.0);

    let (j, m, h_cam) = crate::lpc::cam16_jch_from_xyz(xyz, vc);
    // CAM16-UCS rescale через единый источник (#19/#60) — константы не дублируем.
    let jp_actual = cam16::ucs_j(j);
    let mp = cam16::ucs_m(m);
    let s = if jp_actual + 1.0 > 1e-9 {
        mp / (jp_actual + 1.0)
    } else {
        0.0
    };
    LcsColor::new(jp_actual, h_ok, s.max(0.0), h_cam)
}

/// The 64-step bisection that [`jp_to_oklab_l`] replaced, kept as the reference
/// oracle the analytic inverse is proven against on a dense J' grid (tests) and
/// timed against (the `jp_inv` Criterion bench). Reached only through
/// [`bench_support`] and the test module — never on the production path.
fn jp_to_oklab_l_bisect(jp: f64, vc: &ViewingConditions) -> f64 {
    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;

    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        let xyz = [
            mid * crate::spaces::srgb::D65_WHITE[0],
            mid,
            mid * crate::spaces::srgb::D65_WHITE[2],
        ];
        let (j, _, _) = crate::lpc::cam16_jch_from_xyz(xyz, vc);
        let jp_mid = 1.7 * j / (1.0 + 0.007 * j);
        if jp_mid < jp {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    let y = (lo + hi) * 0.5;
    let lab = srgb_linear_to_oklab([y, y, y]);
    lab[0]
}

/// Benchmark-only access to the two grey-axis J' → Oklab L implementations.
///
/// Wraps the module-private analytic `jp_to_oklab_l` and the bisection oracle
/// so the `benches/jp_inv.rs` Criterion harness can compare them head-to-head.
/// Hidden from the rendered docs and not part of the supported public surface —
/// production callers reach this only through [`AccentCurve::at`].
#[doc(hidden)]
pub mod bench_support {
    use super::ViewingConditions;

    /// Analytic closed-form + Newton inverse (the production path).
    pub fn jp_to_oklab_l_analytic(jp: f64, vc: &ViewingConditions) -> f64 {
        super::jp_to_oklab_l(jp, vc)
    }

    /// Bisection reference (64 iterations, full CAM16 pass per step).
    pub fn jp_to_oklab_l_bisect(jp: f64, vc: &ViewingConditions) -> f64 {
        super::jp_to_oklab_l_bisect(jp, vc)
    }
}

/// The largest in-gamut Oklab chroma along the ray of fixed lightness `l_ok` and
/// hue `h_ok_deg`, found in closed form.
///
/// Along a ray of fixed `(L, h)` in Oklab, the chroma `C` enters each
/// intermediate LMS channel **linearly** (`OKLAB_TO_LMS` is affine in `C`),
/// is then cubed, and recombined into
/// linear sRGB by `LMS_TO_SRGB` — so every sRGB channel is a **cubic polynomial
/// in `C`**. The sRGB gamut wall is the first `C > 0` at which any of the six
/// physical constraints (`channel = 0` or `channel = 1`) is hit. That smallest positive crossing
/// is the maximum chroma, found by solving the cubics in closed form instead of
/// 64 blind bisection steps.
///
/// VC-independent by construction: the only inputs are `(l_ok, h_ok_deg)` and
/// the fixed sRGB↔Oklab matrices — no viewing conditions enter, exactly as the
/// bisection it replaces.
pub(crate) fn max_chroma(l_ok: f64, h_ok_deg: f64) -> f64 {
    use crate::spaces::oklab::{LMS_TO_SRGB, OKLAB_TO_LMS};

    debug_assert!(l_ok.is_finite() && (0.0..=1.0).contains(&l_ok));
    debug_assert!(h_ok_deg.is_finite());
    if l_ok == 0.0 || l_ok == 1.0 {
        return 0.0;
    }

    let h_ok = h_ok_deg.to_radians();
    let cos_h = h_ok.cos();
    let sin_h = h_ok.sin();

    // Каждая LMS′-координата аффинна по C:
    // lms_[k] = row[0]·L + (row[1]·cos h + row[2]·sin h)·C.
    // Первый столбец математической обратной матрицы близок к единице, но у
    // выведенной binary64-матрицы не обязан быть побитово равен ей. Подмена
    // row[0]·L на L смещала коэффициенты кубика и физическую границу гамута.
    let mut p = [0.0_f64; 3];
    let mut q = [0.0_f64; 3];
    for (k, row) in OKLAB_TO_LMS.iter().enumerate() {
        p[k] = row[0] * l_ok;
        q[k] = row[1] * cos_h + row[2] * sin_h;
    }

    // Each sRGB channel rgb[ch](C) = Σ_k M[ch][k] * (p_k + q_k C)^3 is a cubic
    // in C. Build its coefficients [c0, c1, c2, c3] (ascending powers).
    let mut smallest = f64::INFINITY;
    for m in &LMS_TO_SRGB {
        let mut coeff = [0.0_f64; 4];
        for ((&mk, &pk), &qk) in m.iter().zip(p.iter()).zip(q.iter()) {
            // (pk + qk C)^3 = pk^3 + 3 pk^2 qk C + 3 pk qk^2 C^2 + qk^3 C^3
            coeff[0] += mk * pk * pk * pk;
            coeff[1] += mk * 3.0 * pk * pk * qk;
            coeff[2] += mk * 3.0 * pk * qk * qk;
            coeff[3] += mk * qk * qk * qk;
        }
        // C1 — prune the non-binding wall. A channel starts at C = 0 strictly
        // inside both walls (f(0) = l_ok^3 in [0, 1]). Where the channel's cubic
        // is monotone on [0, ∞) it can only ever reach the wall in its slope
        // direction; the opposite wall has NO C > 0 crossing, so the solver would
        // return None for it and skipping it is bit-identical. Only when the
        // channel may reverse on the positive axis are both walls solved — the
        // exact prior behaviour, including the near-black non-convex slivers.
        match binding_walls(coeff) {
            WallBinding::UpperOnly => {
                if let Some(c) = smallest_positive_crossing(coeff, 1.0) {
                    smallest = smallest.min(c);
                }
            }
            WallBinding::LowerOnly => {
                if let Some(c) = smallest_positive_crossing(coeff, 0.0) {
                    smallest = smallest.min(c);
                }
            }
            WallBinding::Both => {
                // First crossing of either physical cube face.
                if let Some(c) = smallest_positive_crossing(coeff, 1.0) {
                    smallest = smallest.min(c);
                }
                if let Some(c) = smallest_positive_crossing(coeff, 0.0) {
                    smallest = smallest.min(c);
                }
            }
        }
    }

    debug_assert!(smallest.is_finite() && smallest > 0.0);

    // A polynomial root lies on the mathematical cube face. Round toward the
    // interior until the actual forward transform satisfies strict device
    // bounds. This corrects binary64 evaluation; it does not widen the gamut.
    let in_gamut = |c: f64| {
        oklab_to_srgb_linear([l_ok, c * cos_h, c * sin_h])
            .into_iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
    };
    if in_gamut(smallest) {
        return smallest;
    }
    let adjacent_inside = smallest.next_down();
    if in_gamut(adjacent_inside) {
        return adjacent_inside;
    }

    // Newton polishing can land several ulps outside. Isolate the boundary from
    // the known achromatic interior until no representable midpoint remains.
    let (mut lo, mut hi) = (0.0_f64, smallest);
    loop {
        let mid = (lo + hi) * 0.5;
        if mid == lo || mid == hi {
            return lo;
        }
        if in_gamut(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
}

/// Which gamut wall(s) a channel's cubic can reach for `C > 0`.
enum WallBinding {
    /// Monotone rising: only the upper wall is reachable.
    UpperOnly,
    /// Monotone falling: only the lower wall is reachable.
    LowerOnly,
    /// May reverse on the positive axis (or a near-degenerate slope): solve both.
    Both,
}

/// Decide, from the SHAPE of the channel cubic `f(C) = coeff · [1, C, C², C³]`,
/// which gamut wall(s) it can cross for `C > 0` — soundly, never optimistically.
///
/// `f(0) = coeff[0] = l_ok³ ∈ [0, 1]` sits strictly inside both walls
/// (`-eps < 0 ≤ f(0) ≤ 1 < 1 + eps`). If `f` is monotone on `[0, ∞)` — and hence
/// on the capped search domain `[0, 1]` (`max_chroma` caps `smallest` at 1.0) —
/// it stays on one side of `f(0)`: rising ⇒ `f ≥ f(0) ≥ 0 > -eps`, so the LOWER
/// wall has no `C > 0` crossing; falling ⇒ `f ≤ f(0) ≤ 1 < 1 + eps`, so the UPPER
/// wall has none. Either way the away wall's crossing does not exist, the
/// two-wall solver returns `None` for it, and pruning it is bit-identical.
/// Soundness rests ONLY on the DROPPED wall having no crossing — never on the
/// kept wall being reached — so it holds whether `f` is a genuine cubic or the
/// `a < 1e-14` quadratic/linear degenerate branch (where `f` need not diverge).
///
/// Monotonicity is tested through the derivative `f'(C) = 3c₃·C² + 2c₂·C + c₁`:
/// if `f'` has no real root at or near the non-negative axis, `f` keeps one
/// slope sign on `[0, ∞)`. A comfortable negative margin (`R_MARGIN`) below zero
/// guarantees floating-point error near a root can never let a real positive
/// reversal (a non-convex gamut sliver) masquerade as monotone: any critical
/// point within the margin, or an ambiguous near-zero slope, falls back to
/// solving both walls — conservative and never wrong.
fn binding_walls(coeff: [f64; 4]) -> WallBinding {
    let c1 = coeff[1];
    let c2 = coeff[2];
    let c3 = coeff[3];

    // f'(C) = a·C² + b·C + cc.
    let a = 3.0 * c3;
    let b = 2.0 * c2;
    let cc = c1;

    // Largest real root of f'(C) = 0 (−∞ sentinel = no real root ⇒ one sign).
    let max_root = if a.abs() < 1e-14 {
        if b.abs() < 1e-14 {
            f64::NEG_INFINITY // constant slope
        } else {
            -cc / b // linear derivative
        }
    } else {
        let disc = b * b - 4.0 * a * cc;
        if disc < 0.0 {
            f64::NEG_INFINITY // no real root: slope keeps one sign
        } else {
            let s = disc.sqrt();
            ((-b + s) / (2.0 * a)).max((-b - s) / (2.0 * a))
        }
    };

    // Any critical point at or near the non-negative axis ⇒ possible reversal ⇒
    // solve both walls. The `1e-6` margin below zero is a floating-point safety
    // band: it comfortably exceeds the round-off in a root near the origin, so a
    // real positive reversal (a non-convex gamut sliver) can never be misread as
    // a negative root and mistakenly pruned.
    if max_root >= -1e-6 {
        return WallBinding::Both;
    }

    // Monotone on [0, ∞): the slope sign at the origin is the direction. The
    // `1e-12` band leaves an ambiguous near-zero slope to the safe both-walls
    // branch rather than committing to a direction it cannot confidently sign.
    if cc > 1e-12 {
        WallBinding::UpperOnly
    } else if cc < -1e-12 {
        WallBinding::LowerOnly
    } else {
        WallBinding::Both
    }
}

/// The smallest strictly-positive real root of the cubic `coeff` (ascending
/// powers) equal to `level`, i.e. of `f(C) - level = 0`, or `None` if the cubic
/// never reaches `level` for `C > 0`.
///
/// Roots are taken in closed form (Cardano / quadratic / linear by degree) and
/// each is polished with two Newton steps so the returned chroma matches the
/// 64-step bisection to full f64 precision.
fn smallest_positive_crossing(coeff: [f64; 4], level: f64) -> Option<f64> {
    let g = [coeff[0] - level, coeff[1], coeff[2], coeff[3]];
    let (roots, n) = cubic_roots(g);
    let mut best: Option<f64> = None;
    for &r in roots.iter().take(n) {
        // Discard non-positive and spurious roots; a real crossing is C > 0.
        if r > 0.0 {
            let polished = newton_polish(g, r);
            if polished > 0.0 {
                best = Some(match best {
                    Some(b) => b.min(polished),
                    None => polished,
                });
            }
        }
    }
    best
}

/// Two Newton iterations on the cubic `g` (ascending powers) from seed `x`,
/// refining a closed-form root to full f64 accuracy.
fn newton_polish(g: [f64; 4], mut x: f64) -> f64 {
    for _ in 0..2 {
        let f = g[0] + x * (g[1] + x * (g[2] + x * g[3]));
        let df = g[1] + x * (2.0 * g[2] + x * 3.0 * g[3]);
        if df.abs() < 1e-18 {
            break;
        }
        x -= f / df;
    }
    x
}

/// Real roots of `g[0] + g[1] x + g[2] x^2 + g[3] x^3 = 0`, handling degenerate
/// (quadratic / linear / constant) leading coefficients. Returns the roots in a
/// fixed buffer plus the count `n` (0–3), allocation-free for the hot path.
fn cubic_roots(g: [f64; 4]) -> ([f64; 3], usize) {
    let [d, c, b, a] = g;

    // Degenerate: not actually cubic.
    if a.abs() < 1e-14 {
        return quadratic_roots(d, c, b);
    }

    // Normalise to x^3 + p2 x^2 + p1 x + p0.
    let p2 = b / a;
    let p1 = c / a;
    let p0 = d / a;

    // Depressed cubic t^3 + p t + q via x = t - p2/3.
    let shift = p2 / 3.0;
    let p = p1 - p2 * p2 / 3.0;
    let q = 2.0 * p2 * p2 * p2 / 27.0 - p2 * p1 / 3.0 + p0;

    let disc = q * q / 4.0 + p * p * p / 27.0;
    let mut roots = [0.0_f64; 3];

    if disc > 1e-30 {
        // One real root.
        let sqrt_disc = disc.sqrt();
        let u = (-q / 2.0 + sqrt_disc).cbrt();
        let v = (-q / 2.0 - sqrt_disc).cbrt();
        roots[0] = u + v - shift;
        (roots, 1)
    } else if disc < -1e-30 {
        // Three distinct real roots (trigonometric form).
        let m = 2.0 * (-p / 3.0).sqrt();
        let theta = ((3.0 * q) / (p * m)).clamp(-1.0, 1.0).acos() / 3.0;
        for (k, slot) in roots.iter_mut().enumerate() {
            *slot = m * (theta - 2.0 * std::f64::consts::PI * k as f64 / 3.0).cos() - shift;
        }
        (roots, 3)
    } else {
        // Repeated roots (disc ~ 0).
        let t1 = if q.abs() < 1e-30 { 0.0 } else { 3.0 * q / p };
        let t2 = -t1 / 2.0;
        roots[0] = t1 - shift;
        roots[1] = t2 - shift;
        (roots, 2)
    }
}

/// Real roots of `b x^2 + c x + d = 0` (handles linear / constant degeneracy),
/// returned in the same fixed-buffer-plus-count form as [`cubic_roots`].
fn quadratic_roots(d: f64, c: f64, b: f64) -> ([f64; 3], usize) {
    let mut roots = [0.0_f64; 3];
    if b.abs() < 1e-14 {
        // Linear c x + d = 0.
        if c.abs() < 1e-14 {
            return (roots, 0);
        }
        roots[0] = -d / c;
        return (roots, 1);
    }
    let disc = c * c - 4.0 * b * d;
    if disc < 0.0 {
        return (roots, 0);
    }
    let sqrt_disc = disc.sqrt();
    roots[0] = (-c + sqrt_disc) / (2.0 * b);
    roots[1] = (-c - sqrt_disc) / (2.0 * b);
    (roots, 2)
}

/// Стена гамута **Display P3** при `(L, h)` — та же бисекция, что
/// [`max_chroma_bisect`], но валидность кандидата проверяется в ЛИНЕЙНОМ P3
/// (Oklab → линейный sRGB → XYZ → линейный P3; первые два шага — линейная
/// алгебра, корректная и за пределами sRGB-куба).
///
/// Этап 1 gamut-aware солвера (2026-07-03): геометрия стен и решётка эмиссии;
/// перевод `Solved`/эмиссии на P3-кандидаты — этап 2. Чистая гамут-геометрия
/// CSS Color 4 матриц — нуля подгонки (класс M-13 инвентаря).
// Прод-потребитель — этап 2 (P3-кандидаты солвера); до него читается тестами.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn max_chroma_p3_bisect(l_ok: f64, h_ok_deg: f64) -> f64 {
    if l_ok == 0.0 || l_ok == 1.0 {
        return 0.0;
    }
    let h_ok = h_ok_deg.to_radians();
    let cos_h = h_ok.cos();
    let sin_h = h_ok.sin();

    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;

    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        let a = mid * cos_h;
        let b = mid * sin_h;
        let rgb =
            crate::spaces::p3::xyz_to_p3_linear(srgb_to_xyz(oklab_to_srgb_linear([l_ok, a, b])));

        if rgb
            .into_iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
        {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    // `lo` по инварианту лежит внутри P3; середина могла бы оказаться снаружи.
    lo
}

/// Независимая строгая бисекция для тестового сравнения с [`max_chroma`].
#[cfg(test)]
pub(crate) fn max_chroma_bisect(l_ok: f64, h_ok_deg: f64) -> f64 {
    if l_ok == 0.0 || l_ok == 1.0 {
        return 0.0;
    }
    let h_ok = h_ok_deg.to_radians();
    let cos_h = h_ok.cos();
    let sin_h = h_ok.sin();

    let mut lo = 0.0_f64;
    let mut hi = 1.0_f64;

    for _ in 0..64 {
        let mid = (lo + hi) * 0.5;
        let a = mid * cos_h;
        let b = mid * sin_h;
        let rgb = oklab_to_srgb_linear([l_ok, a, b]);

        if rgb
            .into_iter()
            .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
        {
            lo = mid;
        } else {
            hi = mid;
        }
    }

    // Возвращаем доказанно внутреннюю сторону брекета, а не потенциально
    // внегамутную середину последнего интервала.
    lo
}

impl crate::curve::ColorCurve for AccentCurve {
    fn at(&self, t: f64) -> LcsColor {
        self.at(t)
    }

    fn vc(&self) -> &ViewingConditions {
        &self.vc
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curve::ColorCurve;

    fn default_neutral() -> NeutralCurve {
        NeutralCurve::new("#FFFFFF", "#787880", "#101012").unwrap()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // DIFFERENTIAL HARNESS для отсечения недостижимой грани куба.
    //
    // Независимый полный вариант решает обе грани каждого канала. Production
    // вправе пропускать одну грань лишь после доказательства монотонности;
    // побитовое совпадение на плотной сетке проверяет именно эту оптимизацию.
    //
    // Корневые функции намеренно продублированы, чтобы ошибка в production
    // Cardano/Newton не копировалась через общий вызов.
    // ─────────────────────────────────────────────────────────────────────────

    /// Полный reference: решает обе физические грани `[0, 1]` каждого канала,
    /// не используя эвристику [`binding_walls`].
    fn max_chroma_reference(l_ok: f64, h_ok_deg: f64) -> f64 {
        use crate::spaces::oklab::{LMS_TO_SRGB, OKLAB_TO_LMS};

        if l_ok == 0.0 || l_ok == 1.0 {
            return 0.0;
        }

        let h_ok = h_ok_deg.to_radians();
        let cos_h = h_ok.cos();
        let sin_h = h_ok.sin();

        let mut p = [0.0_f64; 3];
        let mut q = [0.0_f64; 3];
        for (k, row) in OKLAB_TO_LMS.iter().enumerate() {
            p[k] = row[0] * l_ok;
            q[k] = row[1] * cos_h + row[2] * sin_h;
        }

        let mut smallest = f64::INFINITY;
        for m in &LMS_TO_SRGB {
            let mut coeff = [0.0_f64; 4];
            for ((&mk, &pk), &qk) in m.iter().zip(p.iter()).zip(q.iter()) {
                coeff[0] += mk * pk * pk * pk;
                coeff[1] += mk * 3.0 * pk * pk * qk;
                coeff[2] += mk * 3.0 * pk * qk * qk;
                coeff[3] += mk * qk * qk * qk;
            }
            if let Some(c) = spc_ref(coeff, 1.0) {
                smallest = smallest.min(c);
            }
            if let Some(c) = spc_ref(coeff, 0.0) {
                smallest = smallest.min(c);
            }
        }

        assert!(smallest.is_finite() && smallest > 0.0);
        let in_gamut = |c: f64| {
            oklab_to_srgb_linear([l_ok, c * cos_h, c * sin_h])
                .into_iter()
                .all(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
        };
        if in_gamut(smallest) {
            return smallest;
        }
        let adjacent_inside = smallest.next_down();
        if in_gamut(adjacent_inside) {
            return adjacent_inside;
        }
        let (mut lo, mut hi) = (0.0_f64, smallest);
        loop {
            let mid = (lo + hi) * 0.5;
            if mid == lo || mid == hi {
                return lo;
            }
            if in_gamut(mid) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
    }

    /// Frozen mirror of [`smallest_positive_crossing`].
    fn spc_ref(coeff: [f64; 4], level: f64) -> Option<f64> {
        let g = [coeff[0] - level, coeff[1], coeff[2], coeff[3]];
        let (roots, n) = cubic_roots_ref(g);
        let mut best: Option<f64> = None;
        for &r in roots.iter().take(n) {
            if r > 0.0 {
                let polished = newton_polish_ref(g, r);
                if polished > 0.0 {
                    best = Some(match best {
                        Some(b) => b.min(polished),
                        None => polished,
                    });
                }
            }
        }
        best
    }

    /// Frozen mirror of [`newton_polish`].
    fn newton_polish_ref(g: [f64; 4], mut x: f64) -> f64 {
        for _ in 0..2 {
            let f = g[0] + x * (g[1] + x * (g[2] + x * g[3]));
            let df = g[1] + x * (2.0 * g[2] + x * 3.0 * g[3]);
            if df.abs() < 1e-18 {
                break;
            }
            x -= f / df;
        }
        x
    }

    /// Frozen mirror of [`cubic_roots`].
    fn cubic_roots_ref(g: [f64; 4]) -> ([f64; 3], usize) {
        let [d, c, b, a] = g;
        if a.abs() < 1e-14 {
            return quadratic_roots_ref(d, c, b);
        }
        let p2 = b / a;
        let p1 = c / a;
        let p0 = d / a;
        let shift = p2 / 3.0;
        let p = p1 - p2 * p2 / 3.0;
        let q = 2.0 * p2 * p2 * p2 / 27.0 - p2 * p1 / 3.0 + p0;
        let disc = q * q / 4.0 + p * p * p / 27.0;
        let mut roots = [0.0_f64; 3];
        if disc > 1e-30 {
            let sqrt_disc = disc.sqrt();
            let u = (-q / 2.0 + sqrt_disc).cbrt();
            let v = (-q / 2.0 - sqrt_disc).cbrt();
            roots[0] = u + v - shift;
            (roots, 1)
        } else if disc < -1e-30 {
            let m = 2.0 * (-p / 3.0).sqrt();
            let theta = ((3.0 * q) / (p * m)).clamp(-1.0, 1.0).acos() / 3.0;
            for (k, slot) in roots.iter_mut().enumerate() {
                *slot = m * (theta - 2.0 * std::f64::consts::PI * k as f64 / 3.0).cos() - shift;
            }
            (roots, 3)
        } else {
            let t1 = if q.abs() < 1e-30 { 0.0 } else { 3.0 * q / p };
            let t2 = -t1 / 2.0;
            roots[0] = t1 - shift;
            roots[1] = t2 - shift;
            (roots, 2)
        }
    }

    /// Frozen mirror of [`quadratic_roots`].
    fn quadratic_roots_ref(d: f64, c: f64, b: f64) -> ([f64; 3], usize) {
        let mut roots = [0.0_f64; 3];
        if b.abs() < 1e-14 {
            if c.abs() < 1e-14 {
                return (roots, 0);
            }
            roots[0] = -d / c;
            return (roots, 1);
        }
        let disc = c * c - 4.0 * b * d;
        if disc < 0.0 {
            return (roots, 0);
        }
        let sqrt_disc = disc.sqrt();
        roots[0] = (-c + sqrt_disc) / (2.0 * b);
        roots[1] = (-c - sqrt_disc) / (2.0 * b);
        (roots, 2)
    }

    /// Diff test A over a grid: production `max_chroma` must equal the frozen
    /// reference to full f64 bit identity. `l_steps`/`h_step_deg` size the grid.
    fn assert_max_chroma_matches_full_reference(l_steps: usize, h_step_deg: usize) -> usize {
        let mut points = 0usize;
        for li in 0..=l_steps {
            let l = li as f64 / l_steps as f64;
            let mut h = 0usize;
            while h < 360 {
                let hd = h as f64;
                let prod = max_chroma(l, hd);
                let refv = max_chroma_reference(l, hd);
                assert_eq!(
                    prod.to_bits(),
                    refv.to_bits(),
                    "max_chroma drift at (L={l}, h={hd}): prod={prod:e} ref={refv:e}"
                );
                points += 1;
                h += h_step_deg;
            }
        }
        points
    }

    #[test]
    fn max_chroma_matches_full_two_wall_reference_fast() {
        // Fast subset for the per-PR run: 101 L × 72 hue = 7 272 points.
        let n = assert_max_chroma_matches_full_reference(100, 5);
        assert_eq!(n, 101 * 72);
    }

    #[test]
    #[ignore = "full 180k-point grid — run with `--ignored`; slow at opt-level 0"]
    fn max_chroma_matches_full_two_wall_reference_full() {
        // Full grid: L step 0.002 (501) × hue step 1° (360) = 180 360 points.
        let n = assert_max_chroma_matches_full_reference(500, 1);
        assert_eq!(n, 501 * 360);
    }

    // Diversion tests: prove the coarse_to_fine_argmax debug_asserts BITE, so a
    // caller that violates the fixed-buffer contract fails loud instead of
    // returning a silently-wrong argmax. Debug-only (the asserts compile out of
    // release, so the panic would not fire there).
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "coarse step must be ≥ 1")]
    fn coarse_to_fine_rejects_nonpositive_coarse() {
        let _ = coarse_to_fine_argmax(60, 0, 15, |_| 0.0);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "overflows the")]
    fn coarse_to_fine_rejects_oversized_grid() {
        // 320 / 1 + 2 = 322 coarse samples ≫ the 64-slot buffer.
        let _ = coarse_to_fine_argmax(320, 1, 15, |_| 0.0);
    }

    #[test]
    fn accent_jp_monotonically_decreasing() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        let steps = curve.sample(50);
        for w in steps.windows(2) {
            assert!(
                w[0].jp >= w[1].jp - 0.5,
                "jp increased: {} -> {}",
                w[0].jp,
                w[1].jp
            );
        }
    }

    #[test]
    fn accent_s_non_negative() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        for i in 0..=50 {
            let c = curve.at(i as f64 / 50.0);
            assert!(c.s >= -1e-6, "negative s at t={}: {}", i as f64 / 50.0, c.s);
        }
    }

    #[test]
    fn accent_all_in_gamut() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        for i in 0..=50 {
            let color = curve.at(i as f64 / 50.0);
            let hex = color.to_hex();
            let rgb = srgb_from_hex(&hex).unwrap();
            assert!(
                rgb.iter().all(|&c| (-0.01..=1.01).contains(&c)),
                "out of gamut at t={}: {:?}",
                i as f64 / 50.0,
                rgb
            );
        }
    }

    #[test]
    fn iso_hk_has_exact_achromatic_endpoints() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            let white = white_perceived_lightness(&vc);
            for h in (0..360).step_by(30) {
                let h = f64::from(h);
                assert_eq!(max_chroma_at_perceived_lightness(0.0, h, &vc), Ok(0.0));
                assert_eq!(max_chroma_at_perceived_lightness(white, h, &vc), Ok(0.0));

                let black = color_at_perceived_lightness(0.0, h, 1.0, &vc).unwrap();
                let white_color = color_at_perceived_lightness(white, h, 1.0, &vc).unwrap();
                assert_eq!(black.to_hex_with_vc(&vc), "#000000");
                assert_eq!(white_color.to_hex_with_vc(&vc), "#FFFFFF");
            }
        }
    }

    #[test]
    fn iso_hk_preserves_equation_hue_and_physical_gamut() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for y in [0.02_f64, 0.25, 0.8] {
                let target = crate::lpc::grey_j(y, &vc);
                for h in (0..360).step_by(60) {
                    let h = f64::from(h);
                    for ratio in [0.0_f64, 0.5, 1.0] {
                        let color = color_at_perceived_lightness(target, h, ratio, &vc).unwrap();
                        assert!(in_physical_srgb(color.to_linear_srgb(&vc)));

                        let got = perceived_lightness(&color, &vc);
                        let arithmetic_bound = target.max(1.0) * f64::EPSILON.sqrt();
                        assert!(
                            (got - target).abs() <= arithmetic_bound,
                            "J_HK разошёлся: target={target}, got={got}, h={h}, ratio={ratio}"
                        );
                        if ratio > 0.0 {
                            assert_eq!(color.h_cam().to_bits(), h.to_bits());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn iso_hk_returns_first_connected_physical_boundary() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for y in [0.05_f64, 0.5, 0.95] {
                let target = crate::lpc::grey_j(y, &vc);
                for h in (0..360).step_by(60) {
                    let h = f64::from(h);
                    let (minimum, maximum) = physical_chroma_interval(target, h, &vc).unwrap();
                    assert_eq!(minimum, 0.0);
                    for step in 0..=32 {
                        let c = maximum * f64::from(step) / 32.0;
                        assert!(
                            iso_hk_point_is_physical(target, h, c, &vc),
                            "разрыв до первой границы: target={target}, h={h}, C={c}/{maximum}"
                        );
                    }
                    let outside = maximum.next_up();
                    assert!(
                        !iso_hk_point_is_physical(target, h, outside, &vc),
                        "следующее представимое C после границы осталось в гамуте: {maximum}"
                    );
                    assert!(iso_hk_j(target, maximum, h, &vc) > 0.0);
                }
            }
        }
    }

    #[test]
    fn iso_hk_anchor_identity_reconstructs_emitted_anchor_without_clipping() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            for hex in ["#007AFF", "#FF3B30", "#34C759", "#FFD700"] {
                let anchor = LcsColor::from_hex_with_vc(hex, &vc).unwrap();
                let identity = iso_hk_identity_from_anchor(hex, &vc).unwrap();
                assert!((0.0..=1.0).contains(&identity.chroma_ratio));
                let rebuilt = quantized_iso_hk_for_neutral(
                    &anchor,
                    identity.h_cam,
                    identity.chroma_ratio,
                    &vc,
                )
                .unwrap()
                .color;
                assert_eq!(
                    rebuilt.to_hex_with_vc(&vc),
                    hex,
                    "физический witness должен восстановиться без min/clamp"
                );
            }
        }
    }

    #[test]
    fn iso_hk_rejects_invalid_or_unreachable_requests() {
        let vc = ViewingConditions::srgb();
        assert_eq!(
            max_chroma_at_perceived_lightness(f64::NAN, 0.0, &vc),
            Err(IsoHkError::NonFiniteTarget)
        );
        assert_eq!(
            max_chroma_at_perceived_lightness(50.0, f64::INFINITY, &vc),
            Err(IsoHkError::NonFiniteHue)
        );
        assert_eq!(
            color_at_perceived_lightness(50.0, 0.0, -f64::EPSILON, &vc),
            Err(IsoHkError::InvalidChromaRatio)
        );
        assert_eq!(
            max_chroma_at_perceived_lightness(-f64::EPSILON, 0.0, &vc),
            Err(IsoHkError::BelowBlack {
                target: -f64::EPSILON
            })
        );
    }

    #[test]
    fn max_chroma_white_is_small() {
        let c = max_chroma(1.0, 0.0);
        assert!(c < 0.01, "max chroma at L=1 should be ~0: {}", c);
    }

    #[test]
    fn physical_gamut_has_point_endcaps_and_no_padded_black_halo() {
        for h in 0..360 {
            let h = f64::from(h);
            assert_eq!(max_chroma(0.0, h), 0.0);
            assert_eq!(max_chroma(1.0, h), 0.0);
        }

        // Regression for the former [-1e-6, 1+1e-6] padded cube: its first
        // disconnected sliver made the radius collapse 4x from L=.006 to .007.
        let lower = max_chroma(0.006, 203.0);
        let upper = max_chroma(0.007, 203.0);
        assert!(
            upper >= lower,
            "black-tip radius reversed: {lower} -> {upper}"
        );
    }

    #[test]
    fn max_chroma_mid_has_room() {
        let c = max_chroma(0.5, 30.0);
        assert!(c > 0.1, "max chroma at L=0.5, h=30 should be > 0.1: {}", c);
    }

    #[test]
    fn analytic_max_chroma_agrees_with_bisection_and_is_honest_at_the_wall() {
        // Аналитический решатель сравнивается со строгой 64-шаговой бисекцией.
        // На связном луче они должны совпасть до разрешения самой бисекции.
        //
        // На редких невыпуклых лучах бисекция может перескочить первый короткий
        // выход из куба и найти более дальнюю компоненту. Аналитический ответ
        // обязан оставаться первой строгой границей. Поэтому контракт таков:
        //   * analytic <= bisect + 1e-7   (never over-claims vs the oracle), and
        //   * |analytic − bisect| <= 1e-7 except on the non-convex sliver rays,
        //     which are bounded in count and magnitude and verified to be the
        //     more-correct (in-gamut) side by `analytic_max_chroma_never_exceeds_gamut`.
        let mut convex_worst = 0.0_f64;
        let mut convex_worst_at = (0.0, 0.0);
        let mut nonconvex_points = 0u32;
        let mut nonconvex_worst = 0.0_f64;
        // 201 lightness * 360 hue = 72_360 samples, the full ray space.
        for li in 0..=200 {
            let l = li as f64 / 200.0;
            for hi in 0..360 {
                let h = hi as f64;
                let analytic = max_chroma(l, h);
                let bisect = max_chroma_bisect(l, h);
                // The analytic value must never exceed the bisection's chroma by
                // more than rounding: it is the honest in-gamut bound.
                assert!(
                    analytic <= bisect + 1e-7,
                    "analytic {analytic} over-claims vs bisection {bisect} at (L,h)=({l},{h})"
                );
                let resid = (analytic - bisect).abs();
                if resid <= 1e-7 {
                    convex_worst = convex_worst.max(resid);
                    if resid >= convex_worst {
                        convex_worst_at = (l, h);
                    }
                } else {
                    // A non-convex sliver: analytic is the strictly-in-gamut side.
                    nonconvex_points += 1;
                    nonconvex_worst = nonconvex_worst.max(resid);
                }
            }
        }
        // The convex bulk agrees to bisection precision.
        assert!(
            convex_worst <= 1e-7,
            "convex-region residual {convex_worst:.2e} at {convex_worst_at:?}"
        );
        // Невыпуклые лучи остаются малой частью пространства, а не системным
        // расхождением формул.
        assert!(
            nonconvex_points <= 200,
            "too many non-convex disagreements ({nonconvex_points}) — likely a solver bug, \
             not the known near-black gamut sliver (worst {nonconvex_worst:.2e})"
        );
    }

    #[test]
    fn analytic_max_chroma_never_exceeds_gamut() {
        // Возвращаемая хрома обязана лежать в строгом физическом кубе. Допуск
        // здесь скрыл бы тот же выход за стену, который функция должна исключать.
        for li in 0..=100 {
            let l = li as f64 / 100.0;
            for hi in 0..72 {
                let h = hi as f64 * 5.0;
                let c = max_chroma(l, h);
                let hr = h.to_radians();
                let rgb = oklab_to_srgb_linear([l, c * hr.cos(), c * hr.sin()]);
                for (ch, &v) in rgb.iter().enumerate() {
                    assert!(
                        v.is_finite() && (0.0..=1.0).contains(&v),
                        "C*={c} при (L {l}, h {h}) вывела канал {ch} из гамута: {v}"
                    );
                }
            }
        }
    }

    #[test]
    fn jp_to_oklab_l_analytic_matches_bisection_on_grid() {
        // Equivalence gate: the analytic J' → Oklab L inverse must reproduce the
        // 64-step bisection oracle to better than the bisection's own resolution.
        // Both paths feed the identical `srgb_linear_to_oklab([y,y,y])`, so the
        // only divergence is in the recovered grey luminance `y`; that inherits
        // the < 1e-12 bound `lpc::y_hk` is held to (see y_hk_analytic tests), and
        // the cube root only contracts it. We assert max|dL| < 1e-12 and report
        // the measured worst case.
        //
        // Domain: J' > 0, the values an accent actually feeds here (the neutral
        // curve's J' is a lightness, never negative). The J' = 0 endpoint and the
        // negative / above-asymptote tails are *not* an equivalence region: there
        // the analytic path returns exact black / white by definition, while the
        // bisection only *converges toward* black — its `y` floor is 2^-65, and
        // the cube root blows that up to L ≈ 3e-7, never exact 0, so the analytic
        // answer is the more correct one. Those exact endpoints are pinned
        // separately in `jp_to_oklab_l_endpoints_and_saturation`. For any J' > 0
        // the true grey luminance sits far above the bisection floor and the two
        // agree to f64 round-off.
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            let mut max_dl = 0.0_f64;
            let mut worst_jp = 0.0_f64;
            // grey_j(1.0) ≈ 100; sweep (0, 104] to a hair past white, mirroring
            // the y_hk grid test's reachable-range coverage. Start at n = 1 so the
            // exact-black J' = 0 endpoint (pinned elsewhere) is excluded.
            for n in 1..=6000 {
                let jp = (n as f64 / 6000.0) * 104.0;
                let analytic = jp_to_oklab_l(jp, &vc);
                let bisect = jp_to_oklab_l_bisect(jp, &vc);
                let dl = (analytic - bisect).abs();
                if dl > max_dl {
                    max_dl = dl;
                    worst_jp = jp;
                }
            }
            assert!(
                max_dl < 1e-12,
                "analytic vs bisection max|dL| = {max_dl:e} at J'={worst_jp} exceeds 1e-12"
            );
            eprintln!("jp_to_oklab_l max|dL| = {max_dl:e} (worst J'={worst_jp})");
        }
    }

    #[test]
    fn jp_to_oklab_l_endpoints_and_saturation() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            // J' = 0 → black grey; matches srgb_linear_to_oklab([0,0,0])[0].
            assert_eq!(
                jp_to_oklab_l(0.0, &vc),
                srgb_linear_to_oklab([0.0, 0.0, 0.0])[0]
            );
            // Negative J' is below black and clamps to the black grey too.
            assert_eq!(
                jp_to_oklab_l(-3.0, &vc),
                srgb_linear_to_oklab([0.0, 0.0, 0.0])[0]
            );
            // At/above the UCS asymptote (1.7/0.007 ≈ 242.86) there is no finite
            // J: saturate at the white grey, as the bisection's hi = 1.0 did.
            let white_l = srgb_linear_to_oklab([1.0, 1.0, 1.0])[0];
            assert_eq!(jp_to_oklab_l(250.0, &vc), white_l);
            // Round-trip: the J' produced by a known grey luminance recovers an L
            // that equals the forward grey L for that same luminance.
            for &y in &[0.02_f64, 0.18, 0.5, 0.9, 1.0] {
                let j = crate::lpc::grey_j(y, &vc);
                // J' through the shared helper — the same J'-generation production
                // uses; the equivalence is still anchored by the independent
                // `srgb_linear_to_oklab` reference below, not by `ucs_j`.
                let jp = cam16::ucs_j(j);
                let l = jp_to_oklab_l(jp, &vc);
                let l_ref = srgb_linear_to_oklab([y, y, y])[0];
                assert!(
                    (l - l_ref).abs() < 1e-9,
                    "round-trip y={y}: L={l}, forward grey L={l_ref}, |d|={}",
                    (l - l_ref).abs()
                );
            }
        }
    }

    #[test]
    fn sat_ratio_for_saturated_color() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#FF0000", &neutral).unwrap();
        assert!(
            curve.sat_ratio() > 0.5,
            "red should have high sat_ratio: {}",
            curve.sat_ratio()
        );
    }

    #[test]
    fn sat_ratio_for_desaturated_color() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#CC8888", &neutral).unwrap();
        assert!(
            curve.sat_ratio() < 0.5,
            "desaturated should have low sat_ratio: {}",
            curve.sat_ratio()
        );
    }

    #[test]
    fn boundary_anchors_are_exact_physical_witnesses_without_ratio_clipping() {
        let neutral = default_neutral();
        for anchor_hex in ["#FF3B30", "#3E87FF"] {
            let curve = AccentCurve::new(anchor_hex, &neutral).unwrap();
            assert_eq!(
                curve.sat_ratio().to_bits(),
                1.0_f64.to_bits(),
                "граничный anchor {anchor_hex} обязан быть точным witness радиуса"
            );

            let anchor = LcsColor::from_hex_with_vc(anchor_hex, neutral.vc()).unwrap();
            let identity = iso_hk_identity_from_anchor(anchor_hex, neutral.vc()).unwrap();
            let rebuilt = quantized_iso_hk_for_neutral(
                &anchor,
                identity.h_cam,
                identity.chroma_ratio,
                neutral.vc(),
            )
            .unwrap();
            assert_eq!(rebuilt.color.to_hex(), anchor_hex);
            assert_eq!(rebuilt.target_hk.to_bits(), rebuilt.achieved_hk.to_bits());
        }

        let interior = AccentCurve::new("#B36A65", &neutral).unwrap();
        assert!(
            (0.0..1.0).contains(&interior.sat_ratio()),
            "интерьерный anchor не должен искусственно становиться стеной"
        );
    }

    #[test]
    fn achromatic_anchor_searches_the_complete_finite_gray_domain() {
        let neutral = default_neutral();
        for anchor in ["#000000", "#808080", "#FFFFFF"] {
            let curve = AccentCurve::new(anchor, &neutral).unwrap();
            assert_eq!(curve.sat_ratio(), 0.0);
            for hex in curve.sample_hex(17) {
                let rgb = srgb_from_hex(&hex).unwrap();
                assert_eq!(rgb[0].to_bits(), rgb[1].to_bits(), "{anchor} → {hex}");
                assert_eq!(rgb[1].to_bits(), rgb[2].to_bits(), "{anchor} → {hex}");
            }
        }
    }

    #[test]
    fn sample_hex_produces_valid_colors() {
        let neutral = default_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        let hexes = curve.sample_hex(13);
        assert_eq!(hexes.len(), 13);
        for hex in &hexes {
            assert!(LcsColor::from_hex(hex).is_ok(), "invalid hex: {}", hex);
        }
    }

    #[test]
    fn rejects_bad_hex() {
        let neutral = default_neutral();
        assert!(AccentCurve::new("#GGGGGG", &neutral).is_err());
    }

    // ── Dark-theme (dim-surround) accent tests ────────────────

    fn dim_neutral() -> NeutralCurve {
        use crate::neutral::CurveParams;
        use crate::spaces::vc::ViewingConditions;
        let vc = ViewingConditions::dim_surround();
        NeutralCurve::with_vc(
            "#FFFFFF",
            "#787880",
            "#101012",
            &CurveParams::default(),
            &vc,
        )
        .unwrap()
    }

    #[test]
    fn dim_accent_jp_monotonically_decreasing() {
        let neutral = dim_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        let steps = curve.sample(50);
        for w in steps.windows(2) {
            assert!(
                w[0].jp >= w[1].jp - 0.5,
                "dim accent jp increased: {} -> {}",
                w[0].jp,
                w[1].jp,
            );
        }
    }

    #[test]
    fn dim_accent_all_in_gamut() {
        let neutral = dim_neutral();
        let curve = AccentCurve::new("#007AFF", &neutral).unwrap();
        for i in 0..=50 {
            let color = curve.at(i as f64 / 50.0);
            let hex = color.to_hex_with_vc(&curve.vc);
            let rgb = srgb_from_hex(&hex).unwrap();
            assert!(
                rgb.iter().all(|&c| (-0.01..=1.01).contains(&c)),
                "dim accent out of gamut at t={}: {:?}",
                i as f64 / 50.0,
                rgb
            );
        }
    }

    #[test]
    fn dim_accent_inherits_vc_from_neutral() {
        let neutral = dim_neutral();
        let curve = AccentCurve::new("#FF0000", &neutral).unwrap();
        assert!(
            (curve.vc().c - 0.59).abs() < 1e-10,
            "accent vc.c should match dim neutral: {}",
            curve.vc().c,
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Научные локи + EXPOSURE (волна science/constants-objectivization) для окна поиска
// оптимального оттенка и наклона штрафа дрейфа. Реимплементируют argmax-предикат с
// ЯВНЫМ окном/наклоном (продакшн НЕ трогается) и мерят долю (l,hue)-сетки, где меняется
// выбранная категория оттенка.
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
#[cfg(any())]
mod exposure_locks {
    use super::{HUE_DRIFT_PENALTY_SLOPE, HUE_SEARCH_HALF_WINDOW, max_chroma};

    /// Flat-scan argmax оттенка с ЯВНЫМ окном и наклоном штрафа (bit-совместим с
    /// продакшн `find_optimal_hue_core` при window=HUE_SEARCH_HALF_WINDOW,
    /// penalty_scale=SLOPE/HALF_WINDOW).
    fn argmax_hue(l: f64, hc: f64, penalty_scale: f64, half_window: f64) -> f64 {
        let steps = (half_window * 2.0) as i32;
        let (mut best_h, mut best) = (hc, f64::NEG_INFINITY);
        for i in 0..=steps {
            let h = hc - half_window + i as f64;
            let s = max_chroma(l, h) - penalty_scale * (h - hc).abs();
            if s > best {
                best = s;
                best_h = h;
            }
        }
        best_h
    }

    fn grid_flip(sweep: &[(f64, f64)]) -> (f64, f64) {
        // sweep = list of (penalty_scale, half_window). base is first.
        let (base_ps, base_hw) = sweep[0];
        let (mut flips, mut total, mut max_shift) = (0usize, 0usize, 0.0f64);
        let mut l = 0.05;
        while l <= 0.95 {
            let mut hc = 0.0;
            while hc < 360.0 {
                let base = argmax_hue(l, hc, base_ps, base_hw);
                let mut flipped = false;
                for &(ps, hw) in &sweep[1..] {
                    let alt = argmax_hue(l, hc, ps, hw);
                    let shift = (alt - base).abs();
                    max_shift = max_shift.max(shift);
                    if shift > 0.5 {
                        flipped = true;
                    }
                }
                if flipped {
                    flips += 1;
                }
                total += 1;
                hc += 2.0;
            }
            l += 0.05;
        }
        (100.0 * flips as f64 / total as f64, max_shift)
    }

    /// EXPOSURE окна поиска: доля (l,hue)-сетки, где выбранный оттенок меняется при
    /// свипе окна в [25°,35°] (наклон штрафа держится продакшн). Малая доля ⇒ окно —
    /// нежёсткая нижняя граница, точное 30° нематериально; заметная ⇒ окно намеренно
    /// КАПИРУЕТ дрейф оттенка (как CUSP_HALF_WINDOW_DEG) — мишень с приоритетом.
    #[test]
    fn exposure_hue_search_window() {
        let ps = HUE_DRIFT_PENALTY_SLOPE / HUE_SEARCH_HALF_WINDOW;
        let sweep = [
            (ps, HUE_SEARCH_HALF_WINDOW),
            (ps, 25.0),
            (ps, 35.0),
            (ps, 45.0),
        ];
        let (pct, max_shift) = grid_flip(&sweep);
        eprintln!(
            "EXPOSURE HUE_SEARCH_HALF_WINDOW sweep=25..45deg grid_flip={pct:.2}% max_hue_shift={max_shift:.2}deg"
        );
    }

    /// EXPOSURE наклона штрафа: доля (l,hue)-сетки, где выбранный оттенок меняется при
    /// свипе наклона в [0.10,0.20] (окно фиксировано). Прямо измеряет чувствительность
    /// категории оттенка к калибровочному наклону (у которого есть кандидат-вывод —
    /// хорда Oklab — дающий ДРУГОЕ значение: см. реестр строка 37).
    #[test]
    fn exposure_hue_drift_penalty_slope() {
        let hw = HUE_SEARCH_HALF_WINDOW;
        let base = HUE_DRIFT_PENALTY_SLOPE / hw;
        let sweep = [(base, hw), (0.10 / hw, hw), (0.20 / hw, hw)];
        let (pct, max_shift) = grid_flip(&sweep);
        eprintln!(
            "EXPOSURE HUE_DRIFT_PENALTY_SLOPE sweep=0.10..0.20 grid_flip={pct:.2}% max_hue_shift={max_shift:.2}deg"
        );
    }
}

/// Замки отвергнутых дерайваций: кандидаты, которые ПРОВЕРЕНЫ и отклонены с
/// измеренной причиной. Пиновка не даёт «строгому выводу» вернуться без
/// пересмотра измерений (см. docs/empirical-residue.md, мишень №2).
#[cfg(test)]
#[cfg(any())]
mod derivation_rejection_locks {
    use super::{max_chroma, srgb_from_hex, srgb_linear_to_oklab};

    /// Flat-scan argmax (bit-совместим с find_optimal_hue_core, окно ±30°/1°).
    fn argmax(l: f64, hc: f64, penalty_scale: f64) -> f64 {
        let (mut best_h, mut best) = (hc, f64::NEG_INFINITY);
        for i in 0..=60 {
            let h = hc - 30.0 + i as f64;
            let s = max_chroma(l, h) - penalty_scale * (h - hc).abs();
            if s > best {
                best = s;
                best_h = h;
            }
        }
        best_h
    }

    /// ОТКЛОНЁННЫЙ кандидат для `HUE_DRIFT_PENALTY_SLOPE`: «строгий» хордовый
    /// штраф Oklab `penalty_scale = C·π/180` (перцептивная цена дрейфа = длина
    /// хорды). Замер на 49-якорном замороженном паспорте labui (2026-07-06),
    /// l-сетка 0.05..0.95 шаг 0.01:
    ///   * прод-наклон 0.15/30 = 0.005/°: оптимум ВНУТРИ окна на всех 43
    ///     хроматических якорях (0 прижатий к ребру);
    ///   * хордовый кандидат: прижатие к ребру ±30° на 12/43 якорях, флип
    ///     оптимума >0.5° на 27/43, сдвиги до полного окна (ΔE_ok до 0.077).
    ///
    /// Вывод: кандидат не объективизирует — он передаёт решение произвольной
    /// границе HUE_SEARCH_HALF_WINDOW (интерьерный оптимум → клип по окну),
    /// делая нечувствительную константу окна чувствительной. Отклонён;
    /// значение остаётся design-choice (e), MODEL-CONFLICT: ИЗМЕРЕН-И-ОТКЛОНЁН
    /// (терминальная таксономия; класс (d) упразднён).
    #[test]
    fn chord_derived_slope_rejected_degenerates_to_window_edge() {
        let (mut base_edge, mut chord_edge, mut chord_flip, mut n) =
            (0usize, 0usize, 0usize, 0usize);
        for &hex in crate::exposure_support::LABUI_ANCHORS {
            let rgb = srgb_from_hex(hex).unwrap();
            let lab = srgb_linear_to_oklab(rgb);
            let c0 = (lab[1] * lab[1] + lab[2] * lab[2]).sqrt();
            if c0 < 0.02 {
                continue; // нейтраль: поиск оттенка не участвует
            }
            n += 1;
            let hc = lab[2].atan2(lab[1]).to_degrees().rem_euclid(360.0);
            let (mut be, mut ce, mut cf) = (false, false, false);
            let mut l = 0.05;
            while l <= 0.951 {
                let base = argmax(l, hc, 0.005);
                let ps_chord = max_chroma(l, hc) * std::f64::consts::PI / 180.0;
                let chord = argmax(l, hc, ps_chord);
                if (base - hc).abs() >= 29.999 {
                    be = true;
                }
                if (chord - hc).abs() >= 29.999 {
                    ce = true;
                }
                if (chord - base).abs() > 0.5 {
                    cf = true;
                }
                l += 0.01;
            }
            if be {
                base_edge += 1;
            }
            if ce {
                chord_edge += 1;
            }
            if cf {
                chord_flip += 1;
            }
        }
        assert_eq!(n, 43, "хроматических якорей в паспорте");
        // Прод-наклон: интерьерный оптимум всюду — окно НЕ является решающим.
        assert_eq!(
            base_edge, 0,
            "прод-наклон не должен прижиматься к ребру окна"
        );
        // Хордовый кандидат вырождается (замерено 12 и 27; нижние границы с
        // запасом на будущие уточнения max_chroma).
        assert!(chord_edge >= 8, "прижатий к ребру: {chord_edge}");
        assert!(chord_flip >= 20, "флипов оптимума: {chord_flip}");
    }
}
