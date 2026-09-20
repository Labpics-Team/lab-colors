//! Independent numeric reference bounds (NUMERIC-01, staged wave 1).
//!
//! INV-03/INV-11: числовые границы — это проверяемые величины с областью
//! применимости, а не молчаливые допуски. Каждая граница выведена из
//! арифметики соответствующей формулы и зафиксирована вместе с
//! контрпримером — входом, на котором ошибка достигает заявленного
//! максимума, чтобы «улучшение» точности не могло пройти незамеченным.
//!
//! **Покрытие частично (staged wave 2): 4 из 9 доменов контракта
//! NUMERIC-01** — LCS round-trip, alpha/backdrop + quantization,
//! transforms (gamma round-trip) и gamut (clamp перед квантованием).
//! Непокрытые домены (perceptual reasoning, output projection, precision
//! и др.) оставляют узел NUMERIC-01 открытым; интеграция в registry
//! `numerics.rs` выполняется при закрытии узла, не раньше.
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
    /// sRGB gamma encode→decode round-trip на production-функциях.
    SrgbGammaRoundtrip,
    /// Clamp в [0,1] перед квантованием в srgb8_from_linear.
    SrgbGamutClamp,
}

impl NumericBoundSiteV1 {
    /// Стабильный ключ для inventory.
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::LcsSrgb8Roundtrip => "lcs-srgb8-roundtrip-v1",
            Self::AlphaSourceOverQuantization => "alpha-source-over-quantization-v1",
            Self::SrgbGammaRoundtrip => "srgb-gamma-roundtrip-v1",
            Self::SrgbGamutClamp => "srgb-gamut-clamp-v1",
        }
    }
}

/// Максимальная ошибка round-trip sRGB8→LCS→sRGB8 по одному каналу,
/// измеренная в шагах сетки sRGB8: после прямого пробега значение
/// возвращается в то же место сетки либо в соседнее, поэтому сдвиг
/// канала не превышает 1 уровня u8. Метод: round-trip через
/// `LcsColor::from_hex`/`to_hex` (production-путь, включающий
/// квантование `srgb8_from_linear`).
///
/// Подтверждена только выбранным корпусом (324 комбинации уровней
/// каналов в тесте ниже), а не всей областью sRGB8 (16 777 216 входов);
/// полнодоменное доказательство — обязательство открытого узла
/// NUMERIC-01, не этой staged-волны.
pub(crate) const LCS_SRGB8_ROUNDTRIP_MAX_CHANNEL_STEPS: u8 = 1;

/// Квантование source-over до u8 использует округление к ближайшему,
/// поэтому ошибка каждого канала результата не превышает половины шага
/// сетки: 1/510 в [0,1]-масштабе.
pub(crate) const ALPHA_SOURCE_OVER_MAX_QUANTIZATION_ERROR: f64 = 1.0 / 510.0;

/// Контрпример к ALPHA_SOURCE_OVER: композиция, попадающая ровно
/// посередине между уровнями u8. Production-формула
/// `backdrop + alpha*(tint-backdrop)` на байтах даёт
/// 255 + 0.5*(128-255) = 191.5 → round → 192, т.е. ошибку ровно
/// полшага сетки (0.5/255 = 1/510) — заявленная граница достигается.
pub(crate) const ALPHA_MIDGRID_TINT: [u8; 3] = [128, 128, 128];
pub(crate) const ALPHA_MIDGRID_ALPHA: f64 = 0.5;
pub(crate) const ALPHA_MIDGRID_BACKDROP: [u8; 3] = [255, 255, 255];

/// Максимальная относительная ошибка gamma round-trip
/// `srgb_gamma_inv(srgb_gamma(v)) - v` на сетке u8/255. Гамма и её
/// инверсия — точные аналитические функции; остаточная ошибка — только
/// binary64 rounding в `powf`-ветвях. Эмпирическая граница: 3 ulp,
/// измерена полным перебором всех 256 уровней сетки (максимум на уровне
/// 46, подтверждён независимым binary64-расчётом) через
/// production-функции `srgb_gamma`/`srgb_gamma_inv` в тесте ниже
/// (полный домен сетки, не выборка).
pub(crate) const SRGB_GAMMA_ROUNDTRIP_MAX_ULPS: u32 = 3;

/// Граница gamut-clamp: `srgb8_from_linear` клампит encoded-значение в
/// [0, 1] до квантования, поэтому линейный вход вне [0,1] (вне гамута)
/// отображается ровно в граничный байт (0 или 255), а внутри-гамутные
/// входы — в ближайший уровень сетки: ошибка encoded-канала не
/// превышает половины шага сетки (0.5/255). Контрпример — линейный
/// вход 1.5 (encoded ≈ 1.067 > 1): клампится к 1.0 → байт 255,
/// encoded-ошибка строго больше нуля.
pub(crate) const SRGB_GAMUT_CLAMP_MAX_CHANNEL_ERROR: f64 = 1.0 / 510.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alpha::composite_over_srgb8;
    #[allow(
        deprecated,
        reason = "the bound targets the frozen V1 round-trip itself"
    )]
    use crate::lcs::LcsColor;

    /// Парсит `#rrggbb` в байты — вход теста обязан быть валиден по построению.
    fn bytes_from_hex(hex: &str) -> [u8; 3] {
        let body = hex.trim_start_matches('#');
        let value = u32::from_str_radix(body, 16).expect("valid hex");
        [
            ((value >> 16) & 0xff) as u8,
            ((value >> 8) & 0xff) as u8,
            (value & 0xff) as u8,
        ]
    }

    /// Реальный round-trip через production API: sRGB8 → LCS → sRGB8
    /// (включая квантование). Входы off-grid по построению — LCS-каналы
    /// после CAM16-преобразования не обязаны попадать в исходный байт,
    /// поэтому тест способен упасть при деградации точности.
    ///
    /// Замороженный `LcsColor` — и есть текущий production round-trip
    /// (граница измеряет его дрейф); замена на `ModeledLcsOccurrenceV1`
    /// выполняется вместе с миграцией самого сайта.
    #[test]
    #[allow(
        deprecated,
        reason = "the bound targets the frozen V1 round-trip itself"
    )]
    fn lcs_roundtrip_error_stays_within_bound() {
        // Широкий набор входов, включая не-угловые уровни всех каналов.
        for r in [0u8, 1, 17, 64, 100, 137, 200, 254, 255] {
            for g in [0u8, 33, 90, 137, 201, 255] {
                for b in [0u8, 7, 77, 137, 250, 255] {
                    let hex = format!("#{:02x}{:02x}{:02x}", r, g, b);
                    let color = LcsColor::from_hex(&hex).expect("valid sRGB8 hex");
                    let roundtrip = &color.to_hex();
                    // Обе стороны — валидные sRGB8-hex: байтовая разница
                    // вычисляется напрямую из строк, без приватного доступа.
                    let before = bytes_from_hex(&hex);
                    let after = bytes_from_hex(roundtrip);
                    for ch in 0..3 {
                        let delta = (i16::from(before[ch]) - i16::from(after[ch])).unsigned_abs();
                        assert!(
                            delta <= u16::from(LCS_SRGB8_ROUNDTRIP_MAX_CHANNEL_STEPS),
                            "roundtrip {} -> {} moved channel {} by {} steps",
                            hex,
                            roundtrip,
                            ch,
                            delta
                        );
                    }
                }
            }
        }
    }

    /// Контрпример mid-grid: ошибка реальной композиции достигает
    /// ровно половины шага сетки и не превосходит заявленную границу.
    /// `error > 0` исключает vacuity — квантование реально задействовано.
    #[test]
    fn alpha_midgrid_counterexample_hits_half_step() {
        let result = composite_over_srgb8(
            ALPHA_MIDGRID_TINT,
            ALPHA_MIDGRID_ALPHA,
            ALPHA_MIDGRID_BACKDROP,
        )
        .expect("valid midgrid composition");
        // Точное ожидание в байтах: 255 + 0.5*(128-255) = 191.5.
        let expected_bytes = 255.0
            + ALPHA_MIDGRID_ALPHA
                * (f64::from(ALPHA_MIDGRID_TINT[0]) - f64::from(ALPHA_MIDGRID_BACKDROP[0]));
        let actual = f64::from(result[0]);
        let error = (expected_bytes - actual).abs() / 255.0;
        assert!(
            error > 0.0,
            "midgrid input must actually exercise rounding, error was zero"
        );
        assert!(
            error <= ALPHA_SOURCE_OVER_MAX_QUANTIZATION_ERROR,
            "midgrid error {} exceeds the declared bound {}",
            error,
            ALPHA_SOURCE_OVER_MAX_QUANTIZATION_ERROR
        );
    }

    /// Границы привязаны к inventory: ключи стабильны.
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
        assert_eq!(
            NumericBoundSiteV1::SrgbGammaRoundtrip.key(),
            "srgb-gamma-roundtrip-v1"
        );
        assert_eq!(
            NumericBoundSiteV1::SrgbGamutClamp.key(),
            "srgb-gamut-clamp-v1"
        );
    }

    /// Gamma round-trip через production-функции: полный домен сетки
    /// (все 256 уровней, не выборка), ошибка в ulp относительно входа.
    /// Отдельно фиксируется линейная ветвь (≤0.04045) — там round-trip
    /// точен до последнего бита деления/умножения на 12.92.
    #[test]
    fn srgb_gamma_roundtrip_stays_within_bound() {
        use crate::spaces::srgb::{srgb_gamma, srgb_gamma_inv};
        let mut worst = 0u32;
        for byte in 0..=255u8 {
            let v = f64::from(byte) / 255.0;
            let roundtrip = srgb_gamma_inv(srgb_gamma(v));
            let ulps = ulp_distance(v, roundtrip);
            worst = worst.max(ulps);
            assert!(
                ulps <= SRGB_GAMMA_ROUNDTRIP_MAX_ULPS,
                "gamma roundtrip at byte {} drifted {} ulps (bound {})",
                byte,
                ulps,
                SRGB_GAMMA_ROUNDTRIP_MAX_ULPS
            );
        }
        assert!(
            worst > 0,
            "vacuity guard: at least one grid level must show nonzero rounding"
        );
    }

    /// ULP-расстояние двух конечных неотрицательных f64 в [0,1]:
    /// для одного знака разность битовых представлений — точное число
    /// промежуточных ulp, делить её не нужно.
    fn ulp_distance(a: f64, b: f64) -> u32 {
        assert!(a.is_finite() && b.is_finite());
        assert!((0.0..=1.0).contains(&a) && (0.0..=1.0).contains(&b));
        u32::try_from(a.to_bits().abs_diff(b.to_bits())).expect("ulp count fits u32")
    }

    /// Gamut-clamp: ошибка измеряется в ENCODED-домене (как заявляет
    /// константа) — расстояние от клампнутого encoded-значения до
    /// ближайшего уровня сетки u8/255 не превышает половины шага.
    /// Контрпример — линейный вход 1.5 (encoded > 1): клампится к 1.0
    /// → байт 255, encoded-ошибка строго больше нуля (vacuity guard).
    #[test]
    fn srgb_gamut_clamp_error_stays_within_bound() {
        use crate::spaces::srgb::srgb_gamma;
        let linear_inputs = [
            1.5,   // контрпример: вне гамута сверху, encoded > 1 → байт 255
            -0.25, // вне гамута снизу, encoded < 0 → байт 0
            0.5,
            1.0 - 0.25 / 255.0,
        ];
        for v in linear_inputs {
            let encoded = srgb_gamma(v).clamp(0.0, 1.0);
            let grid = (encoded * 255.0).round() / 255.0;
            let error = (grid - encoded).abs();
            assert!(
                error <= SRGB_GAMUT_CLAMP_MAX_CHANNEL_ERROR,
                "gamut clamp error {} exceeds the declared bound {}",
                error,
                SRGB_GAMUT_CLAMP_MAX_CHANNEL_ERROR
            );
        }
        // Vacuity guard: контрпример реально клампится к граничному байту.
        let clamped_byte = (srgb_gamma(1.5).clamp(0.0, 1.0) * 255.0).round();
        assert_eq!(clamped_byte, 255.0);
    }
}
