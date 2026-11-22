//! Геометрия авторского цветового характера и контекстных опор.
//!
//! Имя роли не участвует в вычислении. Один источник задаёт направление оттенка,
//! а место применения — текущую опору светлоты. Все ограничения проверяются на
//! конечных sRGB8 значениях, в том числе после поиска и квантования.

use crate::Srgb8;
use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
use crate::spaces::srgb::{srgb_linear_from_srgb8, srgb8_from_linear};

/// Настройки геометрии в Oklch: градусы, абсолютная chroma и светлота [0, 1].
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct IdentityOptionsV1 {
    pub(crate) hue_deviation: Option<f64>,
    pub(crate) chroma: Option<[f64; 2]>,
    pub(crate) lightness: Option<[f64; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorIntentErrorV1 {
    NonFinite,
    InvalidRange,
    InvalidHueDeviation,
    NeutralChroma,
    InvalidCurve,
}

/// Версия авторской политики, а не порог именования цветов человеком.
///
/// При отсутствии overrides источник сохраняет геометрическое направление
/// оттенка с окном ±18° и от 10% до 100% своей chroma. Округлённый RGB может
/// превысить верхнюю границу, только если её соблюдение до кодирования даёт
/// ровно те же bytes. Явный chroma override задаёт строгие encoded границы.
/// Точная серая ось остаётся серой; почти нейтральный цвет сохраняет оттенок.
const IDENTITY_POLICY_VERSION: u8 = 2;
const DEFAULT_HUE_DEVIATION_DEGREES: f64 = 18.0;
const DEFAULT_MINIMUM_CHROMA_FRACTION: f64 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ColorIdentityDomainV1 {
    source: Srgb8,
    neutral: bool,
    options: IdentityOptionsV1,
    hue: f64,
    hue_deviation: f64,
    chroma: [f64; 2],
    lightness: [f64; 2],
}

// Все float-поля проходят admission и канонизацию; NaN непредставим.
impl Eq for ColorIdentityDomainV1 {}

fn checked_range(value: [f64; 2], maximum: f64) -> Result<[f64; 2], ColorIntentErrorV1> {
    if !value.iter().all(|v| v.is_finite()) {
        return Err(ColorIntentErrorV1::NonFinite);
    }
    if value[0] < 0.0 || value[0] > value[1] || value[1] > maximum {
        return Err(ColorIntentErrorV1::InvalidRange);
    }
    Ok(value.map(|v| if v == 0.0 { 0.0 } else { v }))
}

pub(crate) fn oklab(rgb: Srgb8) -> [f64; 3] {
    srgb_linear_to_oklab(srgb_linear_from_srgb8(rgb))
}

fn chroma(lab: [f64; 3]) -> f64 {
    lab[1].hypot(lab[2])
}

fn hue(lab: [f64; 3]) -> f64 {
    lab[2].atan2(lab[1]).to_degrees()
}

fn hue_distance(a: f64, b: f64) -> f64 {
    let d = (a - b).abs();
    d.min(360.0 - d)
}

impl ColorIdentityDomainV1 {
    pub(crate) fn new(
        source: Srgb8,
        force_neutral: bool,
        options: IdentityOptionsV1,
    ) -> Result<Self, ColorIntentErrorV1> {
        let hue_deviation = options
            .hue_deviation
            .unwrap_or(DEFAULT_HUE_DEVIATION_DEGREES);
        if !hue_deviation.is_finite() {
            return Err(ColorIntentErrorV1::NonFinite);
        }
        if !(0.0..=180.0).contains(&hue_deviation) {
            return Err(ColorIntentErrorV1::InvalidHueDeviation);
        }
        let lab = oklab(source);
        let neutral = force_neutral || source.is_achromatic();
        let chroma = checked_range(
            options.chroma.unwrap_or(if neutral {
                [0.0, 0.0]
            } else {
                [chroma(lab) * DEFAULT_MINIMUM_CHROMA_FRACTION, chroma(lab)]
            }),
            1.0,
        )?;
        if neutral && chroma[0] > 0.0 {
            return Err(ColorIntentErrorV1::NeutralChroma);
        }
        let lightness = checked_range(options.lightness.unwrap_or([0.0, 1.0]), 1.0)?;
        let options = IdentityOptionsV1 {
            hue_deviation: options
                .hue_deviation
                .map(|v| if v == 0.0 { 0.0 } else { v }),
            chroma: options
                .chroma
                .map(|v| v.map(|x| if x == 0.0 { 0.0 } else { x })),
            lightness: options
                .lightness
                .map(|v| v.map(|x| if x == 0.0 { 0.0 } else { x })),
        };
        Ok(Self {
            source,
            neutral,
            options,
            hue: if neutral { 0.0 } else { hue(lab) },
            hue_deviation,
            chroma,
            lightness,
        })
    }

    pub(crate) const fn neutral(&self) -> bool {
        self.neutral
    }

    pub(crate) const fn source(&self) -> Srgb8 {
        self.source
    }

    pub(crate) fn contains(&self, rgb: Srgb8) -> bool {
        if self.neutral && !rgb.is_achromatic() {
            return false;
        }
        let lab = oklab(rgb);
        if lab[0] < self.lightness[0] || lab[0] > self.lightness[1] {
            return false;
        }
        // Номинальный hue серого от round-off матриц не становится характером.
        if rgb.is_achromatic() {
            return self.chroma[0] == 0.0;
        }
        let c = chroma(lab);
        if c < self.chroma[0] || hue_distance(hue(lab), self.hue) > self.hue_deviation {
            return false;
        }
        if c <= self.chroma[1] {
            return true;
        }
        if self.options.chroma.is_some() {
            return false;
        }
        // A weak authored tint must not become a stronger tint to pay for a
        // smaller lightness error. Preserve representable rounding: this same
        // RGB has a concrete, in-gamut source with no added authored chroma.
        // This witness is sufficient, not a full inverse-quantization search.
        let scale = self.chroma[1] / c;
        let bounded = oklab_to_srgb_linear([lab[0], lab[1] * scale, lab[2] * scale]);
        in_gamut(bounded) && srgb8_from_linear(bounded) == rgb
    }

    /// Консервативный ранний отказ; точный predicate остаётся у point-проверки.
    pub(crate) fn may_intersect_rgb_box(&self, lower: [u8; 3], upper: [u8; 3]) -> bool {
        if lower == upper {
            return self.contains(Srgb8::new(lower));
        }
        if self.neutral {
            let low = lower[0].max(lower[1]).max(lower[2]);
            let high = upper[0].min(upper[1]).min(upper[2]);
            if low > high {
                return false;
            }
            // На точной серой оси Oklab L монотонна.
            return oklab(Srgb8::new([low; 3]))[0] <= self.lightness[1]
                && oklab(Srgb8::new([high; 3]))[0] >= self.lightness[0];
        }
        true
    }

    /// Только авторские данные и версия политики; derived floats не задают ID.
    pub(crate) fn identity_bytes(&self) -> [u8; 46] {
        let mut bytes = [0; 46];
        bytes[0] = IDENTITY_POLICY_VERSION;
        bytes[1] = u8::from(self.neutral);
        bytes[2..5].copy_from_slice(&self.source.bytes());
        bytes[5] = u8::from(self.options.hue_deviation.is_some())
            | (u8::from(self.options.chroma.is_some()) << 1)
            | (u8::from(self.options.lightness.is_some()) << 2);
        let values = [
            self.options.hue_deviation.unwrap_or(0.0),
            self.options.chroma.unwrap_or([0.0; 2])[0],
            self.options.chroma.unwrap_or([0.0; 2])[1],
            self.options.lightness.unwrap_or([0.0; 2])[0],
            self.options.lightness.unwrap_or([0.0; 2])[1],
        ];
        for (slot, value) in bytes[6..].chunks_exact_mut(8).zip(values) {
            slot.copy_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes
    }
}

/// Клиентская функция preferred Oklab L от Oklab L наблюдаемого input-порта.
/// Она не задаёт структуру интерфейса, число уровней или названия режимов.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ToneCurveV1 {
    input: u32,
    stops: Vec<[f64; 2]>,
}

impl Eq for ToneCurveV1 {}

impl ToneCurveV1 {
    pub(crate) fn new(input: u32, mut stops: Vec<[f64; 2]>) -> Result<Self, ColorIntentErrorV1> {
        if stops.len() < 2 {
            return Err(ColorIntentErrorV1::InvalidCurve);
        }
        for stop in &mut stops {
            if !stop.iter().all(|v| v.is_finite()) {
                return Err(ColorIntentErrorV1::NonFinite);
            }
            if !stop.iter().all(|v| (0.0..=1.0).contains(v)) {
                return Err(ColorIntentErrorV1::InvalidCurve);
            }
            for value in stop {
                if *value == 0.0 {
                    *value = 0.0;
                }
            }
        }
        if stops[0][0] != 0.0
            || stops[stops.len() - 1][0] != 1.0
            || stops.windows(2).any(|pair| pair[0][0] >= pair[1][0])
        {
            return Err(ColorIntentErrorV1::InvalidCurve);
        }
        Ok(Self { input, stops })
    }

    pub(crate) const fn input(&self) -> u32 {
        self.input
    }

    pub(crate) fn stops(&self) -> &[[f64; 2]] {
        &self.stops
    }

    pub(crate) fn preferred_lightness(&self, backdrop: Srgb8) -> f64 {
        let lightness = oklab(backdrop)[0].clamp(0.0, 1.0);
        let upper = self.stops.partition_point(|stop| stop[0] < lightness);
        if upper == 0 {
            return self.stops[0][1];
        }
        if upper == self.stops.len() {
            return self.stops[upper - 1][1];
        }
        let [x0, y0] = self.stops[upper - 1];
        let [x1, y1] = self.stops[upper];
        let t = (lightness - x0) / (x1 - x0);
        // Convex interpolation prevents overshoot even for opposite-sign slope.
        (1.0 - t) * y0 + t * y1
    }

    pub(crate) fn preferred_color(&self, source: Srgb8, backdrop: Srgb8) -> Srgb8 {
        color_at_lightness(source, self.preferred_lightness(backdrop))
    }
}

fn in_gamut(rgb: [f64; 3]) -> bool {
    rgb.iter().all(|v| (0.0..=1.0).contains(v))
}

/// Численная опора поиска: переносит hue/chroma источника на заданную светлоту.
/// Gamut compression уменьшает chroma по одному лучу, не вращая hue.
///
/// Эта опора не сужает пространство решения и не является прошедшим result.
/// Solver проверяет полученные bytes теми же constraints, что любой RGB-кандидат.
pub(crate) fn color_at_lightness(source: Srgb8, lightness: f64) -> Srgb8 {
    debug_assert!(lightness.is_finite() && (0.0..=1.0).contains(&lightness));
    if lightness == 0.0 {
        return Srgb8::new([0; 3]);
    }
    if lightness == 1.0 {
        return Srgb8::new([255; 3]);
    }
    if source.is_achromatic() {
        return srgb8_from_linear([lightness * lightness * lightness; 3]);
    }
    let [_, a, b] = oklab(source);
    let project = |scale| oklab_to_srgb_linear([lightness, a * scale, b * scale]);
    let uncompressed = project(1.0);
    if in_gamut(uncompressed) {
        return srgb8_from_linear(uncompressed);
    }
    let mut lower = 0.0;
    let mut upper = 1.0;
    let mut rgb = project(lower);
    // 64 bisections exceed the 53-bit significand on this bounded interval.
    // Stopping when adjacent endpoints share a midpoint avoids useless work.
    for _ in 0..64 {
        let middle = (lower + upper) * 0.5;
        if middle == lower || middle == upper {
            break;
        }
        let candidate = project(middle);
        if in_gamut(candidate) {
            lower = middle;
            rgb = candidate;
        } else {
            upper = middle;
        }
    }
    srgb8_from_linear(rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_preserves_source_direction_without_using_the_role_name() -> Result<(), String> {
        let blue = ColorIdentityDomainV1::new(
            Srgb8::new([0, 103, 244]),
            false,
            IdentityOptionsV1::default(),
        )
        .map_err(|error| format!("blue identity: {error:?}"))?;
        assert!(blue.contains(blue.source()));
        assert!(blue.contains(Srgb8::new([90, 155, 255])));
        assert!(!blue.contains(Srgb8::new([244, 103, 0])));
        assert!(!blue.contains(Srgb8::new([128; 3])));
        assert!(!blue.contains(Srgb8::new([0; 3])));
        Ok(())
    }

    #[test]
    fn identity_does_not_amplify_a_tint_without_an_authored_override() -> Result<(), String> {
        let source = Srgb8::new([239, 230, 216]);
        let expanded = Srgb8::new([222, 216, 189]);
        let identity = ColorIdentityDomainV1::new(source, false, Default::default())
            .map_err(|error| format!("authored tint: {error:?}"))?;
        assert!(identity.contains(source));
        assert!(identity.contains(Srgb8::new([222, 215, 202])));
        assert!(!identity.contains(expanded));
        let explicit = ColorIdentityDomainV1::new(
            source,
            false,
            IdentityOptionsV1 {
                chroma: Some([0.0, 1.0]),
                ..Default::default()
            },
        )
        .map_err(|error| format!("expanded authored domain: {error:?}"))?;
        assert!(explicit.contains(expanded));
        Ok(())
    }

    #[test]
    fn non_expansion_keeps_representable_dark_tints_without_a_numeric_slack() -> Result<(), String>
    {
        let source = Srgb8::new([250, 250, 251]);
        let identity = ColorIdentityDomainV1::new(source, false, Default::default())
            .map_err(|error| format!("weak tint: {error:?}"))?;
        let strict = ColorIdentityDomainV1::new(
            source,
            false,
            IdentityOptionsV1 {
                chroma: Some([0.0, chroma(oklab(source))]),
                ..Default::default()
            },
        )
        .map_err(|error| format!("strict encoded chroma: {error:?}"))?;
        for rgb in [[6, 6, 7], [10, 10, 11], [63, 63, 64], [128, 128, 129]] {
            let candidate = Srgb8::new(rgb);
            assert!(identity.contains(candidate), "{rgb:?}");
            assert!(!strict.contains(candidate), "{rgb:?}");
        }
        assert!(!identity.contains(Srgb8::new([63; 3])));
        Ok(())
    }

    #[test]
    fn neutral_is_an_exact_gray_locus_not_a_noisy_angle() -> Result<(), String> {
        let neutral =
            ColorIdentityDomainV1::new(Srgb8::new([128; 3]), false, IdentityOptionsV1::default())
                .map_err(|error| format!("neutral identity: {error:?}"))?;
        for channel in 0..=255 {
            assert!(neutral.contains(Srgb8::new([channel; 3])));
        }
        assert!(!neutral.contains(Srgb8::new([128, 128, 129])));
        assert!(!neutral.may_intersect_rgb_box([0, 100, 200], [20, 120, 220]));
        assert!(neutral.may_intersect_rgb_box([100, 90, 80], [120, 130, 140]));
        Ok(())
    }

    #[test]
    fn hue_wrap_has_the_short_distance() {
        assert_eq!(hue_distance(-179.0, 179.0), 2.0);
        assert_eq!(hue_distance(179.0, -179.0), 2.0);
        assert_eq!(hue_distance(0.0, 180.0), 180.0);
    }

    #[test]
    fn invalid_identity_settings_do_not_become_a_default() {
        let source = Srgb8::new([0, 103, 244]);
        for value in [f64::NAN, f64::INFINITY, -f64::INFINITY] {
            assert_eq!(
                ColorIdentityDomainV1::new(
                    source,
                    false,
                    IdentityOptionsV1 {
                        hue_deviation: Some(value),
                        ..Default::default()
                    }
                ),
                Err(ColorIntentErrorV1::NonFinite),
            );
        }
        for range in [[0.8, 0.2], [-0.1, 0.5], [0.0, 1.1]] {
            assert_eq!(
                ColorIdentityDomainV1::new(
                    source,
                    false,
                    IdentityOptionsV1 {
                        lightness: Some(range),
                        ..Default::default()
                    }
                ),
                Err(ColorIntentErrorV1::InvalidRange),
            );
        }
        assert_eq!(
            ColorIdentityDomainV1::new(
                source,
                true,
                IdentityOptionsV1 {
                    chroma: Some([0.1, 0.2]),
                    ..Default::default()
                }
            ),
            Err(ColorIntentErrorV1::NeutralChroma),
        );
    }

    #[test]
    fn policy_identity_canonicalizes_zero_and_keeps_explicit_choices() -> Result<(), String> {
        let source = Srgb8::new([0, 103, 244]);
        let make = |value| -> Result<[u8; 46], String> {
            Ok(ColorIdentityDomainV1::new(
                source,
                false,
                IdentityOptionsV1 {
                    hue_deviation: Some(value),
                    chroma: Some([value, 1.0]),
                    lightness: Some([value, 1.0]),
                },
            )
            .map_err(|error| format!("explicit identity: {error:?}"))?
            .identity_bytes())
        };
        assert_eq!(make(0.0)?, make(-0.0)?);
        assert_ne!(
            make(0.0)?,
            ColorIdentityDomainV1::new(source, false, IdentityOptionsV1::default())
                .map_err(|error| format!("default identity: {error:?}"))?
                .identity_bytes()
        );
        Ok(())
    }

    #[test]
    fn client_curve_allows_different_surface_orders() -> Result<(), String> {
        let first = ToneCurveV1::new(7, vec![[0.0, 0.12], [1.0, 0.96]])
            .map_err(|error| format!("first curve: {error:?}"))?;
        let second = ToneCurveV1::new(7, vec![[0.0, 0.20], [1.0, 1.0]])
            .map_err(|error| format!("second curve: {error:?}"))?;
        let dark = Srgb8::new([0; 3]);
        let light = Srgb8::new([255; 3]);
        let neutral = Srgb8::new([128; 3]);
        assert!(
            first.preferred_color(neutral, dark).bytes()[0]
                < second.preferred_color(neutral, dark).bytes()[0]
        );
        assert!(
            first.preferred_color(neutral, light).bytes()[0]
                < second.preferred_color(neutral, light).bytes()[0]
        );
        assert_eq!(first.input(), 7);
        assert_eq!(first.stops().len(), 2);
        Ok(())
    }

    #[test]
    fn curve_rejects_holes_duplicate_knots_and_nonfinite_values() {
        for stops in [
            vec![],
            vec![[0.0, 0.3]],
            vec![[0.1, 0.3], [1.0, 0.8]],
            vec![[0.0, 0.3], [0.5, 0.8]],
            vec![[0.0, 0.3], [0.0, 0.4], [1.0, 0.8]],
            vec![[0.0, 0.3], [1.0, 1.1]],
        ] {
            assert_eq!(
                ToneCurveV1::new(0, stops),
                Err(ColorIntentErrorV1::InvalidCurve)
            );
        }
        assert_eq!(
            ToneCurveV1::new(0, vec![[0.0, 0.3], [1.0, f64::NAN]]),
            Err(ColorIntentErrorV1::NonFinite)
        );
    }

    #[test]
    fn tonal_seeds_are_opaque_bytes_and_keep_a_useful_blue_path() -> Result<(), String> {
        let source = Srgb8::new([0, 103, 244]);
        let default_identity = ColorIdentityDomainV1::new(source, false, Default::default())
            .map_err(|error| format!("default blue identity: {error:?}"))?;
        assert!(!default_identity.contains(Srgb8::new([16, 111, 253])));
        assert!(default_identity.contains(Srgb8::new([16, 111, 252])));
        // A tonal hint is not an admitted result. Its encoded chroma can
        // exceed the source after quantization; the selector checks the
        // declared domain and can choose an adjacent, admissible RGB instead.
        let identity = ColorIdentityDomainV1::new(
            source,
            false,
            IdentityOptionsV1 {
                chroma: Some([0.0, 1.0]),
                ..Default::default()
            },
        )
        .map_err(|error| format!("blue identity: {error:?}"))?;
        let mut previous = 0.0;
        for step in 1..255 {
            let candidate = color_at_lightness(source, f64::from(step) / 255.0);
            let l = oklab(candidate)[0];
            // Quantization can merge adjacent lightness steps but cannot cause
            // an appreciable reversal on this fixed-hue path.
            assert!(l + 0.004 >= previous);
            previous = l;
            if (64..=230).contains(&step) {
                assert!(identity.contains(candidate), "step {step}: {candidate:?}");
            }
        }
        assert_eq!(color_at_lightness(source, 0.0), Srgb8::new([0; 3]));
        assert_eq!(color_at_lightness(source, 1.0), Srgb8::new([255; 3]));
        Ok(())
    }
}
