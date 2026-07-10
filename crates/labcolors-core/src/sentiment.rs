//! Сентимент-цвета как геометрия клиентских якорей.
//!
//! Имя категории (`danger`, `warning`, …) не определяет цветовую область.
//! Физические данные приходят только из связанного с категорией якоря. Закон
//! V2 сохраняет его оттенок и выводит попарный контракт из фактических Oklab
//! `a/b`-дистанций. Поэтому в модуле нет универсального угла, «предпочтительной
//! стороны» или порядка, в котором категории занимают окружность.

#[cfg(test)]
use crate::accent::Accent;
#[cfg(test)]
use crate::accent::oklab_hue_of;
use crate::lcs::LcsColor;
use crate::neutral::NeutralCurve;
use crate::scale::{IsoHkIdentity, iso_hk_identity_from_anchor, quantized_iso_hk_for_neutral};
use crate::spaces::oklab::srgb_linear_to_oklab;
use crate::spaces::srgb::{hex_from_srgb_encoded, srgb_encoded_from_hex, srgb_from_hex};
use crate::spaces::vc::ViewingConditions;

/// Стабильный идентификатор коэффициент-свободного закона сентиментов.
pub const SENTIMENT_GEOMETRY_V2: &str = "anchor-distance-v2";

/// Встроенные категории существуют только как тестовый свидетель. Продакшн
/// получает произвольные имена и связи `category → family` из конфига.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg(test)]
pub enum Sentiment {
    Danger,
    Warning,
    Success,
    Info,
}

#[cfg(test)]
impl Sentiment {
    fn prototype_hue(self) -> f64 {
        oklab_hue_of(self.anchor_hex())
    }

    pub fn accent(self) -> Accent {
        match self {
            Self::Danger => Accent::Red,
            Self::Warning => Accent::Orange,
            Self::Success => Accent::Green,
            Self::Info => Accent::Blue,
        }
    }

    fn anchor_hex(self) -> &'static str {
        self.accent().anchor_hex()
    }

    pub(crate) const ALL: [Self; 4] = [Self::Danger, Self::Warning, Self::Success, Self::Info];
}

/// Сентиментная кривая на общей H-K-лестнице нейтрали.
///
/// Хроматическая сила выводится из клиентского якоря как его доля физического
/// CAM16-радиуса на собственном H-K-уровне. Та же доля переносится на остальные
/// уровни; отдельной ручки «насыщенности сентимента» нет.
#[derive(Debug, Clone)]
pub struct SentimentCurve {
    /// Oklab hue клиентского якоря; V2 его не смещает.
    pub resolved_hue: f64,
    /// Поле совместимости отчёта. В V2 всегда `false`.
    pub was_displaced: bool,
    /// Поле совместимости отчёта. В V2 всегда `0`.
    pub displacement: f64,
    neutral: NeutralCurve,
    resolved_cam_hue: f64,
    chroma_ratio: f64,
}

impl SentimentCurve {
    /// Построить кривую из единственного клиентского якоря.
    ///
    /// Oklab hue для отчёта, CAM16 hue для iso-HK и доля физического радиуса
    /// выводятся одним проходом из тех же байтов. Поэтому вызывающий не может
    /// случайно связать имя категории с одним оттенком, а физику — с другим.
    pub fn from_anchor(chroma_hex: &str, neutral: &NeutralCurve) -> Result<Self, String> {
        let identity = iso_hk_identity_from_anchor(chroma_hex, neutral.vc())?;
        Ok(Self::from_identity(identity, neutral))
    }

    /// Совместимый вход со старым отдельным `prototype_hue`.
    ///
    /// Значение больше не управляет цветом: оно обязано побитово совпасть с hue,
    /// заново выведенным из `chroma_hex`, иначе возвращается ошибка. Новый код
    /// должен вызывать [`Self::from_anchor`].
    #[deprecated(note = "используйте SentimentCurve::from_anchor; hue выводится из anchor")]
    pub fn new(
        prototype_hue: f64,
        chroma_hex: &str,
        neutral: &NeutralCurve,
    ) -> Result<Self, String> {
        if !prototype_hue.is_finite() {
            return Err(format!(
                "prototype_hue должен быть конечным: {prototype_hue}"
            ));
        }
        let identity = iso_hk_identity_from_anchor(chroma_hex, neutral.vc())?;
        let supplied = normalize_hue(prototype_hue);
        if supplied.to_bits() != identity.h_ok.to_bits() {
            return Err(format!(
                "prototype_hue={supplied} противоречит hue={}; источник истины — anchor {chroma_hex}",
                identity.h_ok
            ));
        }
        Ok(Self::from_identity(identity, neutral))
    }

    fn from_identity(identity: IsoHkIdentity, neutral: &NeutralCurve) -> Self {
        Self {
            resolved_hue: identity.h_ok,
            was_displaced: false,
            displacement: 0.0,
            neutral: neutral.clone(),
            resolved_cam_hue: identity.h_cam,
            chroma_ratio: identity.chroma_ratio,
        }
    }

    /// Цвет на том же H-K-уровне, что и нейтраль в позиции `t`.
    pub fn at(&self, t: f64) -> LcsColor {
        assert!(t.is_finite(), "параметр кривой должен быть конечным");
        let vc = self.neutral.vc();
        let t = t.clamp(0.0, 1.0);
        let neutral = self.neutral.at(t);
        quantized_iso_hk_for_neutral(&neutral, self.resolved_cam_hue, self.chroma_ratio, vc)
            .expect("сентиментный H-K-уровень обязан быть физически достижим")
            .color
    }

    pub fn sample_hex(&self, n: usize) -> Vec<String> {
        match n {
            0 => Vec::new(),
            1 => vec![self.hex_at(0.5)],
            _ => (0..n)
                .map(|i| self.hex_at(i as f64 / (n - 1) as f64))
                .collect(),
        }
    }

    fn hex_at(&self, t: f64) -> String {
        self.at(t).to_hex_with_vc(self.neutral.vc())
    }

    /// Доля физической CAM16-хромы, выведенная из якоря.
    pub fn chroma_ratio(&self) -> f64 {
        self.chroma_ratio
    }
}

impl crate::curve::ColorCurve for SentimentCurve {
    fn at(&self, t: f64) -> LcsColor {
        self.at(t)
    }

    fn vc(&self) -> &ViewingConditions {
        self.neutral.vc()
    }
}

#[cfg(test)]
impl SentimentCurve {
    pub(crate) fn from_sentiment(
        sentiment: Sentiment,
        _legacy_brand_hue: f64,
        chroma_hex: &str,
        neutral: &NeutralCurve,
    ) -> Result<Self, String> {
        #[allow(deprecated)]
        Self::new(sentiment.prototype_hue(), chroma_hex, neutral)
    }
}

fn normalize_hue(h: f64) -> f64 {
    h.rem_euclid(360.0)
}

/// Техническая граница числовой определённости Oklab hue.
///
/// Она не участвует в попарном законе V2: расстояния считаются непосредственно
/// в `a/b`. Константа остаётся общей защитой для других API, где нужен `atan2`.
// SSOT-TRACKED (#38): числовой epsilon, не порог восприятия.
pub(crate) const ACHROMATIC_CHROMA_EPS: f64 = 1e-7;

/// Канонический кодомен нормализации hue.
// SSOT-TRACKED (#39): начало математического домена угла.
pub(crate) const HUE_DOMAIN_MIN_INCLUSIVE: f64 = 0.0;
// SSOT-TRACKED (#40): полный оборот, верхняя граница не включена.
pub(crate) const HUE_DOMAIN_MAX_EXCLUSIVE: f64 = 360.0;

/// Совместимый вход старой сигнатуры.
///
/// V2 не применяет `chroma_fraction` и `hue_floor`. Чтобы старый вызов не
/// выглядел успешно применённой политикой, допускаются только их инертные
/// значения. Новый код вызывает [`resolve_config_sentiment_solid_v2`].
#[deprecated(note = "используйте resolve_config_sentiment_solid_v2; ручки V1 удалены")]
pub fn resolve_config_sentiment_solid(
    family_anchor_hex: &str,
    chroma_fraction: f64,
    hue_floor: Option<f64>,
) -> Result<String, String> {
    if chroma_fraction != 1.0 || hue_floor.is_some() {
        return Err(format!(
            "chroma_fraction/hue_floor принадлежат устаревшему закону; \
             модель {SENTIMENT_GEOMETRY_V2} принимает только клиентский anchor"
        ));
    }
    resolve_config_sentiment_solid_v2(family_anchor_hex)
}

/// Нормализовать представимый sRGB-якорь, не меняя его цвет.
pub fn resolve_config_sentiment_solid_v2(family_anchor_hex: &str) -> Result<String, String> {
    Ok(hex_from_srgb_encoded(srgb_encoded_from_hex(
        family_anchor_hex,
    )?))
}

/// Минимальный угловой отступ для двух произвольных Oklab-радиусов.
///
/// Формула следует непосредственно из закона косинусов:
///
/// `D² = C₁² + C₂² − 2 C₁ C₂ cos(Δh)`.
///
/// `D` задаётся фактической `a/b`-дистанцией клиентской пары. Если
/// `D ≤ |C₁−C₂|`, радиальная разница уже выполняет контракт. Если
/// `D > C₁+C₂`, ни один угол не может выполнить контракт, поэтому результат —
/// ошибка, а не пропуск пары.
pub fn minimum_hue_separation_deg(
    distance: f64,
    chroma_1: f64,
    chroma_2: f64,
) -> Result<f64, String> {
    if !distance.is_finite()
        || !chroma_1.is_finite()
        || !chroma_2.is_finite()
        || distance < 0.0
        || chroma_1 < 0.0
        || chroma_2 < 0.0
    {
        return Err(format!(
            "попарная геометрия вне домена: D={distance}, C1={chroma_1}, C2={chroma_2}"
        ));
    }

    let radial_distance = (chroma_1 - chroma_2).abs();
    let diameter = chroma_1 + chroma_2;
    if distance <= radial_distance {
        return Ok(0.0);
    }
    if distance > diameter {
        return Err(format!(
            "попарная дистанция недостижима: D={distance} > C1+C2={diameter}"
        ));
    }
    if chroma_1 == 0.0 || chroma_2 == 0.0 {
        return Err(format!(
            "попарная дистанция недостижима при ахроматическом радиусе: \
             D={distance}, C1={chroma_1}, C2={chroma_2}"
        ));
    }

    let cosine = (chroma_1 * chroma_1 + chroma_2 * chroma_2 - distance * distance)
        / (2.0 * chroma_1 * chroma_2);
    Ok(cosine.clamp(-1.0, 1.0).acos().to_degrees())
}

#[derive(Debug, Clone)]
struct AnchorGeometry {
    name: String,
    hex: String,
    a: f64,
    b: f64,
}

impl AnchorGeometry {
    fn from_hex(name: &str, hex: &str) -> Result<Self, String> {
        let normalized = resolve_config_sentiment_solid_v2(hex)?;
        let lab = srgb_linear_to_oklab(srgb_from_hex(&normalized)?);
        Ok(Self {
            name: name.to_string(),
            hex: normalized,
            a: lab[1],
            b: lab[2],
        })
    }

    fn chroma(&self) -> f64 {
        self.a.hypot(self.b)
    }

    fn distance_squared(&self, other: &Self) -> f64 {
        (self.a - other.a).powi(2) + (self.b - other.b).powi(2)
    }
}

/// Финальная перепроверка всех неупорядоченных пар.
///
/// Для каждой пары `reference` несёт собственный `Dᵢⱼ`. Наборы индексируются
/// по имени, поэтому результат не зависит от порядка категорий. Функция не
/// возвращает частичный успех: одна недостижимая или столкнувшаяся пара делает
/// недействительным весь набор.
fn verify_pairwise_anchor_contract(
    reference: &[AnchorGeometry],
    resolved: &[AnchorGeometry],
) -> Result<(), String> {
    let reference_by_name: std::collections::BTreeMap<&str, &AnchorGeometry> = reference
        .iter()
        .map(|point| (point.name.as_str(), point))
        .collect();
    let resolved_by_name: std::collections::BTreeMap<&str, &AnchorGeometry> = resolved
        .iter()
        .map(|point| (point.name.as_str(), point))
        .collect();
    if reference_by_name.len() != reference.len()
        || resolved_by_name.len() != resolved.len()
        || reference_by_name.keys().ne(resolved_by_name.keys())
    {
        return Err("набор имён сентиментов неоднозначен или изменился при резолве".into());
    }

    let names: Vec<&str> = reference_by_name.keys().copied().collect();
    for i in 0..names.len() {
        for j in (i + 1)..names.len() {
            let left = names[i];
            let right = names[j];
            let source_left = reference_by_name[left];
            let source_right = reference_by_name[right];
            let output_left = resolved_by_name[left];
            let output_right = resolved_by_name[right];
            let required_sq = source_left.distance_squared(source_right);
            let required = required_sq.sqrt();

            minimum_hue_separation_deg(required, output_left.chroma(), output_right.chroma())
                .map_err(|reason| format!("пара `{left}`/`{right}`: {reason}"))?;

            let achieved_sq = output_left.distance_squared(output_right);
            if achieved_sq < required_sq {
                return Err(format!(
                    "пара `{left}`/`{right}` уменьшила различимость: \
                     D²={achieved_sq} < исходного D²={required_sq}"
                ));
            }
        }
    }
    Ok(())
}

/// Разрешить весь набор якорей одной темы.
///
/// V2 сохраняет входные цвета, но всё равно выполняет наборную финальную
/// проверку. Это важный шов для H-K-проекции: её нельзя будет подключить к
/// отдельной категории и забыть перепроверить уже «отдохнувшие» пары.
pub(crate) fn resolve_anchor_palette_v2(
    anchors: &[(String, String)],
) -> Result<Vec<(String, String)>, String> {
    let mut reference = Vec::with_capacity(anchors.len());
    for (name, hex) in anchors {
        reference.push(AnchorGeometry::from_hex(name, hex)?);
    }
    let resolved = reference.clone();
    verify_pairwise_anchor_contract(&reference, &resolved)?;

    let mut output: Vec<(String, String)> = resolved
        .into_iter()
        .map(|point| (point.name, point.hex))
        .collect();
    output.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn neutral() -> NeutralCurve {
        NeutralCurve::new("#FFFFFF", "#787880", "#101012").unwrap()
    }

    #[test]
    fn curve_keeps_anchor_hue_and_derives_chroma() {
        let neutral = neutral();
        let saturated = SentimentCurve::from_anchor("#FF3B30", &neutral).unwrap();
        let muted = SentimentCurve::from_anchor("#B36A65", &neutral).unwrap();
        assert!(!saturated.was_displaced && saturated.displacement == 0.0);
        assert!(!muted.was_displaced && muted.displacement == 0.0);
        assert!(saturated.chroma_ratio() > muted.chroma_ratio());
    }

    #[test]
    fn curve_uses_the_nearest_srgb8_state_for_each_neutral_level() {
        let neutral = neutral();
        for sentiment in Sentiment::ALL {
            let curve =
                SentimentCurve::from_sentiment(sentiment, 0.0, sentiment.anchor_hex(), &neutral)
                    .unwrap();
            for step in 0..=32 {
                let t = f64::from(step) / 32.0;
                let neutral_level = neutral.at(t);
                let identity =
                    iso_hk_identity_from_anchor(sentiment.anchor_hex(), neutral.vc()).unwrap();
                let expected = quantized_iso_hk_for_neutral(
                    &neutral_level,
                    identity.h_cam,
                    identity.chroma_ratio,
                    neutral.vc(),
                )
                .unwrap();
                assert_eq!(
                    curve.at(t).to_hex_with_vc(neutral.vc()),
                    expected.color.to_hex_with_vc(neutral.vc()),
                    "{sentiment:?} t={t}: выбран не оптимум конечной sRGB8-ячейки"
                );
            }
        }
    }

    #[test]
    #[allow(deprecated)]
    fn legacy_two_source_constructor_rejects_a_contradictory_hue() {
        let neutral = neutral();
        let actual = oklab_hue_of("#3E87FF");
        assert!(SentimentCurve::new(actual, "#3E87FF", &neutral).is_ok());
        let error = SentimentCurve::new(actual.next_up(), "#3E87FF", &neutral).unwrap_err();
        assert!(error.contains("противоречит"), "{error}");
    }

    #[test]
    fn unequal_radius_formula_reconstructs_the_law_of_cosines() {
        for c1_i in 1..=20 {
            for c2_i in 1..=20 {
                let c1 = f64::from(c1_i) / 20.0;
                let c2 = f64::from(c2_i) / 20.0;
                // 180° проверяется отдельно точным входом `C1+C2`: вычисление
                // D через sqrt перед публичной границей могло бы округлить его
                // на один ulp за математический диаметр и тем самым проверить
                // арифметику вызывающего, а не закон косинусов.
                for angle in 0..180 {
                    let radians = f64::from(angle).to_radians();
                    let distance = (c1 * c1 + c2 * c2 - 2.0 * c1 * c2 * radians.cos()).sqrt();
                    let recovered = minimum_hue_separation_deg(distance, c1, c2).unwrap();
                    let reconstructed_sq =
                        c1 * c1 + c2 * c2 - 2.0 * c1 * c2 * recovered.to_radians().cos();
                    let expected_sq = distance * distance;
                    let arithmetic_bound =
                        32.0 * f64::EPSILON * (1.0 + c1 * c1 + c2 * c2 + expected_sq);
                    assert!(
                        (reconstructed_sq - expected_sq).abs() <= arithmetic_bound,
                        "C1={c1}, C2={c2}, angle={angle}, recovered={recovered}, \
                         D²={reconstructed_sq}, expected={expected_sq}"
                    );
                }
            }
        }
    }

    #[test]
    fn radial_difference_needs_no_angle_and_diameter_overflow_is_error() {
        assert_eq!(minimum_hue_separation_deg(0.3, 0.8, 0.5).unwrap(), 0.0);
        assert_eq!(minimum_hue_separation_deg(1.3, 0.8, 0.5).unwrap(), 180.0);
        assert!(minimum_hue_separation_deg(1.300_000_000_1, 0.8, 0.5).is_err());
        assert!(minimum_hue_separation_deg(0.2, 0.0, 0.1).is_err());
    }

    #[test]
    fn palette_resolution_is_independent_of_category_order() {
        let base = vec![
            ("danger".to_string(), "#FF3B30".to_string()),
            ("warning".to_string(), "#FFA100".to_string()),
            ("success".to_string(), "#34C759".to_string()),
            ("info".to_string(), "#3E87FF".to_string()),
        ];
        let expected = resolve_anchor_palette_v2(&base).unwrap();
        let permutations = [
            vec![0, 1, 2, 3],
            vec![3, 2, 1, 0],
            vec![1, 3, 0, 2],
            vec![2, 0, 3, 1],
        ];
        for order in permutations {
            let input: Vec<_> = order.into_iter().map(|index| base[index].clone()).collect();
            assert_eq!(resolve_anchor_palette_v2(&input).unwrap(), expected);
        }
    }

    #[test]
    fn final_pairwise_recheck_rejects_every_collision_not_only_last_neighbor() {
        let reference = vec![
            AnchorGeometry::from_hex("a", "#FF0000").unwrap(),
            AnchorGeometry::from_hex("b", "#00FF00").unwrap(),
            AnchorGeometry::from_hex("c", "#0000FF").unwrap(),
        ];
        let resolved = vec![
            AnchorGeometry::from_hex("a", "#FF0000").unwrap(),
            AnchorGeometry::from_hex("b", "#FF0000").unwrap(),
            AnchorGeometry::from_hex("c", "#0000FF").unwrap(),
        ];
        let error = verify_pairwise_anchor_contract(&reference, &resolved).unwrap_err();
        assert!(error.contains("`a`/`b`"), "{error}");
    }

    #[test]
    fn duplicate_names_are_rejected_before_pairwise_work() {
        let input = vec![
            ("danger".to_string(), "#FF0000".to_string()),
            ("danger".to_string(), "#00FF00".to_string()),
        ];
        assert!(resolve_anchor_palette_v2(&input).is_err());
    }

    #[test]
    #[allow(deprecated)]
    fn legacy_adapter_accepts_only_inert_fields() {
        assert_eq!(
            resolve_config_sentiment_solid("#ff3b30", 1.0, None).unwrap(),
            "#FF3B30"
        );
        assert!(resolve_config_sentiment_solid("#FF3B30", 0.88, None).is_err());
        assert!(resolve_config_sentiment_solid("#FF3B30", 1.0, Some(0.0)).is_err());
    }

    #[test]
    fn sample_hex_has_requested_length_and_valid_values() {
        let neutral = neutral();
        let curve = SentimentCurve::from_anchor("#3E87FF", &neutral).unwrap();
        for count in [0, 1, 2, 13] {
            let samples = curve.sample_hex(count);
            assert_eq!(samples.len(), count);
            assert!(samples.iter().all(|hex| srgb_from_hex(hex).is_ok()));
        }
    }
}
