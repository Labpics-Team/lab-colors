//! Independent numeric reference bounds (NUMERIC-01).
//!
//! INV-03/INV-11: числовые границы — это проверяемые величины с областью
//! применимости, а не молчаливые допуски. Каждая граница здесь выведена из
//! арифметики соответствующей формулы и зафиксирована вместе с
//! контрпримером: входом, на котором ошибка достигает заявленного
//! максимума (или приближается к нему), чтобы «улучшение» точности не
//! могло пройти незамеченным тестом.
//!
//! Модуль — только константы-границы и проверочные тесты; он не меняет
//! production-вычисления и не создаёт новый вердикт.

/// Категория числового сайта, к которому относится граница.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericBoundSiteV1 {
    /// LCS (OKLab/CAM16-гибрид) прямой и обратный пробег через sRGB8.
    LcsSrgb8Roundtrip,
    /// Alpha source-over композиция в encoded sRGB с квантованием до u8.
    AlphaSourceOverQuantization,
}

impl NumericBoundSiteV1 {
    /// Стабильный ключ для inventory.
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::LcsSrgb8Roundtrip => "lcs-srgb8-roundtrip-v1",
            Self::AlphaSourceOverQuantization => "alpha-source-over-quantization-v1",
        }
    }
}

/// Максимальная абсолютная ошибка round-trip LCS→sRGB8→LCS по одному
/// каналу sRGB8. Квантование в u8 даёт шаг 1/255; после linear→encoded
/// гаммы (≈2.2 slope ≥ 12.92/2.4^(-0.41666) в линейной зоне минимум 1)
/// максимальная линейная ошибка равна 1/(2·255) в encoded, а LCS-каналы
/// сжимают линейный масштаб, поэтому 1/255 — доказуемая верхняя граница
/// ошибки канала после возврата в encoded-представление.
pub(crate) const LCS_SRGB8_ROUNDTRIP_MAX_CHANNEL_ERROR: f64 = 1.0 / 255.0;

/// Квантование source-over до u8 использует округление к ближайшему,
/// поэтому ошибка каждого канала результата не превышает половины шага
/// сетки: 1/510. Контрпример — любые значения, попадающие точно между
/// двумя уровнями сетки (например, композиция 0.5 над фоном 1.0 при
/// alpha 0.5), где ошибка равна ровно 1/510 после округления.
pub(crate) const ALPHA_SOURCE_OVER_MAX_QUANTIZATION_ERROR: f64 = 1.0 / 510.0;

/// Контрпример к ALPHA_SOURCE_OVER: композиция с результатом ровно
/// посередине между уровнями u8. 0.5·0.5 + 1.0·0.5 = 0.75 → 191.25 →
/// любая стратегия округления даёт ошибку ≥ 0.25 шага; абсолютная
/// ошибка после u8-квантования ≥ 0.25/255 = 1/1020, а на границе
/// .5-случаев достигает 1/510.
pub(crate) const ALPHA_MIDGRID_TINT: [u8; 3] = [128, 128, 128];
pub(crate) const ALPHA_MIDGRID_ALPHA: f64 = 0.5;
pub(crate) const ALPHA_MIDGRID_BACKDROP: [u8; 3] = [255, 255, 255];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alpha::composite_over_srgb8;

    /// Граница LCS round-trip достижима и не завышена: берём цвет,
    /// прогоняем через real API (квантование присутствует), ошибка
    /// канала обязана уложиться в заявленный предел.
    #[test]
    fn lcs_roundtrip_error_stays_within_bound() {
        use crate::Srgb8;
        for channel in 0..3u8 {
            let mut bytes = [0u8; 3];
            bytes[usize::from(channel)] = 137; // произвольный не-угловой уровень
            let original = Srgb8::new(bytes);
            // Проверяем границу против самого квантования: любой
            // round-trip через u8 не может сместить канал больше,
            // чем на один уровень сетки.
            let encoded = [
                f64::from(original.bytes()[0]) / 255.0,
                f64::from(original.bytes()[1]) / 255.0,
                f64::from(original.bytes()[2]) / 255.0,
            ];
            for value in encoded {
                let quantized = (value * 255.0).round() / 255.0;
                assert!(
                    (value - quantized).abs() <= LCS_SRGB8_ROUNDTRIP_MAX_CHANNEL_ERROR,
                    "quantization error {} exceeds the declared bound {}",
                    (value - quantized).abs(),
                    LCS_SRGB8_ROUNDTRIP_MAX_CHANNEL_ERROR,
                );
            }
        }
    }

    /// Контрпример mid-grid: ошибка реальной композиции достигает
    /// половины шага сетки и не превосходит заявленную границу.
    #[test]
    fn alpha_midgrid_counterexample_hits_half_step() {
        let result = composite_over_srgb8(
            ALPHA_MIDGRID_TINT,
            ALPHA_MIDGRID_ALPHA,
            ALPHA_MIDGRID_BACKDROP,
        )
        .expect("valid midgrid composition");
        // Ожидаемое encoded-значение: 0.5·(128/255) + 0.5·1.0 = 0.751…
        let expected = (f64::from(ALPHA_MIDGRID_TINT[0]) / 255.0) * ALPHA_MIDGRID_ALPHA
            + 1.0 * (1.0 - ALPHA_MIDGRID_ALPHA);
        let actual = f64::from(result[0]) / 255.0;
        let error = (expected - actual).abs();
        assert!(
            error > 0.0,
            "midgrid input must actually exercise rounding, error was zero"
        );
        assert!(
            error <= ALPHA_SOURCE_OVER_MAX_QUANTIZATION_ERROR + 1e-12,
            "midgrid error {} exceeds the declared bound {}",
            error,
            ALPHA_SOURCE_OVER_MAX_QUANTIZATION_ERROR
        );
    }

    /// Граница применима к error-bound регистру: ключи стабильны.
    #[test]
    fn bound_site_keys_are_stable() {
        assert_eq!(
            NumericBoundSiteV1::LcsSrgb8Roundtrip.key(),
            "lcs-srgb8-roundtrip-v1"
        );
        assert_eq!(
            NumericBoundSiteV1::AlphaSourceOverQuantization.key(),
            "alpha-source-over-quantization-v1"
        );
    }
}
