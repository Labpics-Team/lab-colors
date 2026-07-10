use std::sync::Arc;

use crate::lcs::LcsColor;
use crate::spaces::srgb::{quantise_srgb, srgb_encoded_from_hex, srgb_from_hex, srgb_gamma_inv};
use crate::spaces::vc::ViewingConditions;

/// Маркер бескоэффициентного построения нейтральной кривой.
///
/// Нулевой размер сохраняет совместимость вызовов `with_params`/`with_vc`, но не
/// оставляет скрытых gamma/chroma/hue-ручек. Геометрию полностью задают три
/// якоря, стандартная sRGB-квантизация и выбранные условия просмотра.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CurveParams;

/// Одно фактически представимое состояние пути и пройденная до него длина.
#[derive(Debug, Clone, Copy)]
struct QuantizedState {
    color: LcsColor,
    cumulative_ucs: f64,
    #[cfg(test)]
    rgb8: [u8; 3],
}

/// Конечная цветовая полилиния через светлый, базовый и тёмный якоря.
///
/// Между соседними якорями строится выпуклый отрезок в **линейном свете sRGB**.
/// Затем перечисляются все состояния, которые фактически выдаёт общий
/// `encode → clamp → round → decode`-квантизатор sRGB8. Для каждого следующего
/// кода канала ищется первое представимое `f64 t`, на котором переключается
/// production-квантование; аналитическая half-byte граница не считается
/// эквивалентной живому пути из-за округления и `libm`.
///
/// Длина пути — сумма CAM16-UCS расстояний между реальными `#RRGGBB`, а не длина
/// непрерывной кривой до квантования. Побитовая межплатформенная идентичность
/// трансцендентного encode-пути отдельно сертифицируется задачей #223.
///
/// Поэтому [`Self::at`] — ступенчатая функция и близкие значения `t` закономерно
/// могут вернуть один цвет. «Равномерность» здесь означает только ближайшее по
/// накопленной CAM16-UCS длине состояние данной конечной sRGB8-полилинии. Это не
/// утверждение об универсальной психофизической равномерности CAM16-UCS и не
/// аппроксимация непрерывной float-кривой.
#[derive(Debug, Clone)]
pub struct NeutralCurve {
    a_light: LcsColor,
    a_base: LcsColor,
    a_dark: LcsColor,
    states: Arc<[QuantizedState]>,
    total_ucs: f64,
    base_t: f64,
    vc: ViewingConditions,
}

impl NeutralCurve {
    /// Строит кривую для стандартного среднего окружения sRGB.
    pub fn new(light: &str, base: &str, dark: &str) -> Result<Self, String> {
        Self::with_vc(light, base, dark, &CurveParams, &ViewingConditions::srgb())
    }

    /// Совместимая форма вызова для бескоэффициентного построения.
    pub fn with_params(
        light: &str,
        base: &str,
        dark: &str,
        params: CurveParams,
    ) -> Result<Self, String> {
        Self::with_vc(light, base, dark, &params, &ViewingConditions::srgb())
    }

    /// Строит кривую при явно заданных условиях просмотра.
    ///
    /// Все состояния декодируются из конечных sRGB8-байтов в одних условиях.
    /// Это гарантирует допустимость гаммы по построению: метод `at` никогда не
    /// создаёт CAM16-точку, которую затем пришлось бы клиппить или радиально
    /// отображать обратно в sRGB.
    pub fn with_vc(
        light: &str,
        base: &str,
        dark: &str,
        _params: &CurveParams,
        vc: &ViewingConditions,
    ) -> Result<Self, String> {
        vc.validate()?;
        let light_bytes = bytes_from_hex(light)?;
        let base_bytes = bytes_from_hex(base)?;
        let dark_bytes = bytes_from_hex(dark)?;

        let a_light = LcsColor::from_hex_with_vc(light, vc)?;
        let a_base = LcsColor::from_hex_with_vc(base, vc)?;
        let a_dark = LcsColor::from_hex_with_vc(dark, vc)?;

        if a_light.jp <= a_base.jp {
            return Err("светлый якорь должен иметь J′ строго выше базового".into());
        }
        if a_base.jp <= a_dark.jp {
            return Err("базовый якорь должен иметь J′ строго выше тёмного".into());
        }

        let mut path = enumerate_segment(light_bytes, base_bytes)?;
        let base_index = path.len() - 1;
        path.extend(
            enumerate_segment(base_bytes, dark_bytes)?
                .into_iter()
                .skip(1),
        );

        let mut states: Vec<QuantizedState> = Vec::with_capacity(path.len());
        let mut cumulative = 0.0_f64;
        let mut compensation = 0.0_f64;

        for (index, &bytes) in path.iter().enumerate() {
            let color = match index {
                0 => a_light,
                i if i == base_index => a_base,
                i if i + 1 == path.len() => a_dark,
                _ => LcsColor::from_hex_with_vc(&hex_from_bytes(bytes), vc)?,
            };
            if !color.jp.is_finite() || !color.ucs_cartesian().into_iter().all(f64::is_finite) {
                return Err(format!(
                    "CAM16-UCS не определён для состояния {}",
                    hex_from_bytes(bytes)
                ));
            }

            if let Some(previous) = states.last() {
                if previous.color.jp <= color.jp {
                    return Err(format!(
                        "конечный sRGB8-путь не строго убывает по J′: {} → {}",
                        hex_from_bytes(path[index - 1]),
                        hex_from_bytes(bytes)
                    ));
                }

                let step = previous.color.delta_e_ucs(&color);
                if !step.is_finite() || step <= 0.0 {
                    return Err(format!(
                        "нулевая или неопределённая длина CAM16-UCS: {} → {}",
                        hex_from_bytes(path[index - 1]),
                        hex_from_bytes(bytes)
                    ));
                }

                // Компенсированное суммирование не меняет метрику: оно лишь не
                // даёт сотням положительных отрезков терять младшие биты суммы.
                let corrected = step - compensation;
                let next = cumulative + corrected;
                compensation = (next - cumulative) - corrected;
                cumulative = next;
            }

            states.push(QuantizedState {
                color,
                cumulative_ucs: cumulative,
                #[cfg(test)]
                rgb8: bytes,
            });
        }

        if !cumulative.is_finite() || cumulative <= 0.0 {
            return Err("суммарная длина sRGB8-пути должна быть конечной и положительной".into());
        }
        let base_length = states[base_index].cumulative_ucs;
        let base_t = base_length / cumulative;
        if !(0.0 < base_t && base_t < 1.0) {
            return Err("базовый якорь должен лежать внутри конечного пути".into());
        }

        Ok(Self {
            a_light,
            a_base,
            a_dark,
            states: states.into(),
            total_ucs: cumulative,
            base_t,
            vc: *vc,
        })
    }

    /// Ближайшее представимое состояние на нормированной накопленной длине
    /// CAM16-UCS `t ∈ [0, 1]`.
    ///
    /// При точном равенстве расстояний выбирается более светлое, то есть более
    /// раннее состояние пути. Tie-break нужен только для полной определённости
    /// дискретной функции и не вводит цветовой коэффициент.
    pub fn at(&self, t: f64) -> LcsColor {
        assert!(t.is_finite(), "параметр кривой t должен быть конечным");
        let t = t.clamp(0.0, 1.0);

        // Явные контрактные точки не зависят от округления `base_t * total` и
        // всегда возвращают ровно декодированные пользовательские якоря.
        if t == 0.0 {
            return self.a_light;
        }
        if t == self.base_t {
            return self.a_base;
        }
        if t == 1.0 {
            return self.a_dark;
        }

        let target = t * self.total_ucs;
        let upper = self
            .states
            .partition_point(|state| state.cumulative_ucs <= target);
        if upper == 0 {
            return self.states[0].color;
        }
        if upper == self.states.len() {
            return self.states[upper - 1].color;
        }

        let before = self.states[upper - 1];
        let after = self.states[upper];
        let distance_before = target - before.cumulative_ucs;
        let distance_after = after.cumulative_ucs - target;
        if distance_after < distance_before {
            after.color
        } else {
            before.color
        }
    }

    pub fn vc(&self) -> &ViewingConditions {
        &self.vc
    }

    pub fn light_anchor(&self) -> &LcsColor {
        &self.a_light
    }

    pub fn base_anchor(&self) -> &LcsColor {
        &self.a_base
    }

    pub fn dark_anchor(&self) -> &LcsColor {
        &self.a_dark
    }

    /// Положение базы, выведенное из длин фактически эмитируемых sRGB8-рёбер.
    pub fn base_position(&self) -> f64 {
        self.base_t
    }
}

#[derive(Debug, Clone, Copy)]
struct Boundary {
    t: f64,
    channel: usize,
    next_byte: u8,
}

/// Первое представимое `t ∈ (0, 1]`, на котором production-квантование канала
/// достигает следующего кода.
///
/// Для неотрицательных finite `f64` порядок битовых представлений совпадает с
/// числовым, поэтому бинарный поиск охватывает весь кодомен `t`, а не выборочную
/// сетку. Проверяется именно общий [`quantise_srgb`], чтобы перечисление и
/// финальная эмиссия не имели двух версий правила округления.
fn first_transition_t(
    start: f64,
    end: f64,
    target_byte: u8,
    increasing: bool,
) -> Result<f64, String> {
    let target = srgb_gamma_inv(f64::from(target_byte) / 255.0);
    let crossed = |t: f64| {
        let linear = start + (end - start) * t;
        let quantized = quantise_srgb([linear, 0.0, 0.0])[0];
        if increasing {
            quantized >= target
        } else {
            quantized <= target
        }
    };

    if crossed(0.0) || !crossed(1.0) {
        return Err(format!(
            "квантователь не образует переход к коду {target_byte} внутри сегмента"
        ));
    }

    let mut lo = 0.0_f64.to_bits();
    let mut hi = 1.0_f64.to_bits();
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if crossed(f64::from_bits(mid)) {
            hi = mid;
        } else {
            lo = mid;
        }
    }

    let t = f64::from_bits(hi);
    if t.is_finite() && 0.0 < t && t <= 1.0 {
        Ok(t)
    } else {
        Err(format!(
            "переход к коду {target_byte} дал недопустимый параметр t={t}"
        ))
    }
}

/// Перечисляет все состояния сегмента, реально достижимые общим sRGB8-квантователем.
fn enumerate_segment(from: [u8; 3], to: [u8; 3]) -> Result<Vec<[u8; 3]>, String> {
    let from_rgb = srgb_from_hex(&hex_from_bytes(from))?;
    let to_rgb = srgb_from_hex(&hex_from_bytes(to))?;
    let mut boundaries = Vec::new();

    for channel in 0..3 {
        let from_byte = from[channel];
        let to_byte = to[channel];
        match to_byte.cmp(&from_byte) {
            std::cmp::Ordering::Greater => {
                for next_byte in (from_byte + 1)..=to_byte {
                    boundaries.push(Boundary {
                        t: first_transition_t(from_rgb[channel], to_rgb[channel], next_byte, true)?,
                        channel,
                        next_byte,
                    });
                }
            }
            std::cmp::Ordering::Less => {
                for next_byte in to_byte..from_byte {
                    boundaries.push(Boundary {
                        t: first_transition_t(
                            from_rgb[channel],
                            to_rgb[channel],
                            next_byte,
                            false,
                        )?,
                        channel,
                        next_byte,
                    });
                }
            }
            std::cmp::Ordering::Equal => {}
        }
    }

    boundaries.sort_by(|a, b| a.t.total_cmp(&b.t).then_with(|| a.channel.cmp(&b.channel)));

    let mut bytes = from;
    let mut states = Vec::with_capacity(boundaries.len() + 1);
    states.push(bytes);
    let mut index = 0;
    while index < boundaries.len() {
        let event_t = boundaries[index].t;
        while index < boundaries.len() && boundaries[index].t == event_t {
            let event = boundaries[index];
            bytes[event.channel] = event.next_byte;
            index += 1;
        }
        states.push(bytes);
    }

    if bytes != to {
        return Err("перечисление живого sRGB8-квантователя не достигло конечного якоря".into());
    }
    Ok(states)
}

fn bytes_from_hex(hex: &str) -> Result<[u8; 3], String> {
    let encoded = srgb_encoded_from_hex(hex)?;
    Ok(encoded.map(|value| (value * 255.0).round() as u8))
}

fn hex_from_bytes([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

impl crate::curve::ColorCurve for NeutralCurve {
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

    fn curve(vc: &ViewingConditions) -> NeutralCurve {
        NeutralCurve::with_vc("#FFFFFF", "#787880", "#101012", &CurveParams, vc).unwrap()
    }

    fn assert_all_outputs_are_real_path_states(curve: &NeutralCurve) {
        let states: std::collections::HashSet<_> = curve
            .states
            .iter()
            .map(|state| {
                let expected = hex_from_bytes(state.rgb8);
                assert_eq!(
                    state.color.to_hex_with_vc(curve.vc()),
                    expected,
                    "CAM16 round-trip изменил перечисленное sRGB8-состояние"
                );
                expected
            })
            .collect();
        for i in 0..=4096 {
            let t = f64::from(i) / 4096.0;
            let hex = curve.at(t).to_hex_with_vc(curve.vc());
            assert!(
                states.contains(&hex),
                "t={t} вернул состояние вне пути: {hex}"
            );
            assert_eq!(
                LcsColor::from_hex_with_vc(&hex, curve.vc())
                    .unwrap()
                    .to_hex_with_vc(curve.vc()),
                hex,
                "эмитируемое состояние обязано быть реальным sRGB8"
            );
        }
    }

    #[test]
    fn anchors_are_byte_exact() {
        for c in [
            curve(&ViewingConditions::srgb()),
            curve(&ViewingConditions::dim_surround()),
        ] {
            assert_eq!(c.at(0.0).to_hex_with_vc(c.vc()), "#FFFFFF");
            assert_eq!(c.at(c.base_position()).to_hex_with_vc(c.vc()), "#787880");
            assert_eq!(c.at(1.0).to_hex_with_vc(c.vc()), "#101012");
        }
    }

    #[test]
    fn finite_path_is_strictly_monotone_in_jp() {
        for c in [
            curve(&ViewingConditions::srgb()),
            curve(&ViewingConditions::dim_surround()),
        ] {
            for pair in c.states.windows(2) {
                assert!(pair[0].color.jp > pair[1].color.jp);
            }
        }
    }

    #[test]
    fn base_position_uses_the_quantized_polyline_length() {
        let c = curve(&ViewingConditions::srgb());
        let base = c
            .states
            .iter()
            .position(|state| state.color == c.a_base)
            .unwrap();
        assert_eq!(
            c.base_position().to_bits(),
            (c.states[base].cumulative_ucs / c.total_ucs).to_bits()
        );
    }

    #[test]
    fn at_selects_a_nearest_state_with_lightward_ties() {
        let c = curve(&ViewingConditions::srgb());
        for i in 0..=2048 {
            let t = f64::from(i) / 2048.0;
            let target = t * c.total_ucs;
            let expected = c
                .states
                .iter()
                .enumerate()
                .min_by(|(ia, a), (ib, b)| {
                    (a.cumulative_ucs - target)
                        .abs()
                        .total_cmp(&(b.cumulative_ucs - target).abs())
                        .then_with(|| ia.cmp(ib))
                })
                .unwrap()
                .1
                .color;
            assert_eq!(c.at(t), expected, "неверное ближайшее состояние при t={t}");
        }
    }

    #[test]
    fn close_anchors_honestly_repeat_states_when_sampled() {
        let c = NeutralCurve::new("#FFFFFF", "#FEFEFE", "#FDFDFD").unwrap();
        let samples = c.sample_hex(257);
        let unique: std::collections::HashSet<_> = samples.iter().collect();
        assert_eq!(unique.len(), 3);
        assert!(samples.windows(2).any(|pair| pair[0] == pair[1]));
        assert_eq!(c.at(c.base_position()).to_hex(), "#FEFEFE");
    }

    #[test]
    fn every_output_is_a_gamut_state_from_the_finite_path() {
        for vc in [ViewingConditions::srgb(), ViewingConditions::dim_surround()] {
            assert_all_outputs_are_real_path_states(&curve(&vc));
        }
    }

    #[test]
    fn regression_near_white_base_never_leaves_srgb8_path() {
        let c = NeutralCurve::new("#FFFFFF", "#FFF9FC", "#000000").unwrap();
        assert_eq!(c.at(0.0).to_hex(), "#FFFFFF");
        assert_eq!(c.at(c.base_position()).to_hex(), "#FFF9FC");
        assert_eq!(c.at(1.0).to_hex(), "#000000");
        assert_all_outputs_are_real_path_states(&c);
    }

    #[test]
    fn regression_chromatic_anchors_never_return_an_out_of_gamut_candidate() {
        let error = NeutralCurve::new("#FAFF16", "#BDFFFF", "#724523").unwrap_err();
        assert_eq!(
            error,
            "конечный sRGB8-путь не строго убывает по J′: #FAFF16 → #FAFF17"
        );
    }

    #[test]
    fn rejects_invalid_misordered_or_nonmonotone_anchors() {
        assert!(NeutralCurve::new("#GGGGGG", "#787880", "#101012").is_err());
        assert!(NeutralCurve::new("#787880", "#FFFFFF", "#101012").is_err());
        assert!(NeutralCurve::new("#FFFFFF", "#101012", "#787880").is_err());

        // Якоря могут быть упорядочены, но отдельный байтовый переход внутри
        // сегмента всё равно повысить J′; такой путь нельзя молча переставлять.
        let error = NeutralCurve::new("#FAFF16", "#BDFFFF", "#724523").unwrap_err();
        assert!(error.contains("не строго убывает по J′"));
    }

    #[test]
    fn rejects_nonfinite_or_internally_inconsistent_viewing_conditions() {
        let build = |vc: &ViewingConditions| {
            NeutralCurve::with_vc("#FFFFFF", "#787880", "#101012", &CurveParams, vc)
        };

        let mut nonfinite = ViewingConditions::srgb();
        nonfinite.c = f64::NAN;
        assert!(
            build(&nonfinite)
                .unwrap_err()
                .contains("c должен быть конечным")
        );

        let mut bad_rgb_d = ViewingConditions::srgb();
        bad_rgb_d.rgb_d[1] = f64::INFINITY;
        assert!(
            build(&bad_rgb_d)
                .unwrap_err()
                .contains("rgb_d[1] должен быть конечным")
        );

        let mut stale_fl_power = ViewingConditions::srgb();
        stale_fl_power.fl_pow_025 = f64::from_bits(stale_fl_power.fl_pow_025.to_bits() + 1);
        assert!(
            build(&stale_fl_power)
                .unwrap_err()
                .contains("fl_pow_025 не согласован с fl")
        );

        let mut stale_aw = ViewingConditions::srgb();
        stale_aw.aw = f64::from_bits(stale_aw.aw.to_bits() + 1);
        assert!(
            build(&stale_aw)
                .unwrap_err()
                .contains("aw не согласован с fl, nbb и rgb_d")
        );

        let mut stale_ncb = ViewingConditions::srgb();
        stale_ncb.ncb = f64::from_bits(stale_ncb.ncb.to_bits() + 1);
        assert!(
            build(&stale_ncb)
                .unwrap_err()
                .contains("ncb должен побитово совпадать с nbb")
        );
    }

    #[test]
    fn arbitrary_anchors_are_monotone_or_rejected_explicitly() {
        let mut state = 0x6A09_E667_F3BC_C909_u64;
        let mut next_byte = || {
            // Фиксированный LCG нужен только для воспроизводимого покрытия разных
            // направлений каналов; порогов или решений production-кривой в нём нет.
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 56) as u8
        };

        for _ in 0..32 {
            let mut anchors = [[0_u8; 3]; 3];
            for anchor in &mut anchors {
                *anchor = [next_byte(), next_byte(), next_byte()];
            }
            anchors.sort_by(|a, b| {
                let ja = LcsColor::from_hex(&hex_from_bytes(*a)).unwrap().jp;
                let jb = LcsColor::from_hex(&hex_from_bytes(*b)).unwrap().jp;
                jb.total_cmp(&ja)
            });

            let hex = anchors.map(hex_from_bytes);
            let jp = hex
                .each_ref()
                .map(|value| LcsColor::from_hex(value).unwrap().jp);
            if !(jp[0] > jp[1] && jp[1] > jp[2]) {
                continue;
            }

            match NeutralCurve::new(&hex[0], &hex[1], &hex[2]) {
                Ok(curve) => {
                    assert_eq!(curve.at(0.0).to_hex(), hex[0]);
                    assert_eq!(curve.at(curve.base_position()).to_hex(), hex[1]);
                    assert_eq!(curve.at(1.0).to_hex(), hex[2]);
                    assert!(
                        curve
                            .states
                            .windows(2)
                            .all(|pair| pair[0].color.jp > pair[1].color.jp)
                    );
                    for state in curve.states.iter() {
                        assert_eq!(state.color.to_hex(), hex_from_bytes(state.rgb8));
                    }
                }
                Err(error) => assert!(
                    error.contains("не строго убывает по J′"),
                    "упорядоченные валидные якоря дали неожиданный отказ: {error}"
                ),
            }
        }
    }

    #[test]
    #[should_panic(expected = "параметр кривой t должен быть конечным")]
    fn rejects_nan_parameter() {
        let _ = curve(&ViewingConditions::srgb()).at(f64::NAN);
    }
}
