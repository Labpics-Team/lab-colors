//! Independent numeric reference bounds for the NUMERIC-01 exit envelope.
//!
//! INV-03/INV-11: числовые границы — это проверяемые величины с областью
//! применимости, а не молчаливые допуски. Источник формулы остаётся у
//! owning-модуля (IEC 61966-2-1 для sRGB, Ottosson 2021 для Oklab, Li et al.
//! 2017/CIE 248:2022 для CAM16); здесь живут только проверяемые error bounds,
//! counterexamples и finite-domain evidence для фактически исполняемых путей.
//!
//! Перечень закрывает все численные категории, названные r13 для NUMERIC-01,
//! не вводя искусственный счётчик «доменов»: LCS round-trip проверяется на всех
//! 16 777 216 encoded-sRGB8 стимулах при фиксированных sRGB viewing conditions;
//! alpha/backdrop имеет half-step границу относительно binary64-композиции;
//! sRGB-квантование отдельно учитывает округление масштаба на 255; transforms
//! покрыты полным 256-level sRGB transfer grid; gamut проверяет clamp + emitted
//! quantization; output projection имеет отдельные Oklch chroma/hue bounds и
//! near-neutral wrap counterexample; Oklab precision проверяется на полном
//! encoded-sRGB8 домене. Mismatched viewing conditions остаются отрицательным
//! applicability-control, а не частью round-trip guarantee.
//!
//! Удалённый ролевой Glow не является потребителем этих границ.
//! Действующий численный реестр задаётся в `numerics.rs`; этот модуль
//! проверяет геометрию и погрешности названных преобразований, не создаёт
//! дополнительного оценщика или человеческого вердикта.

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
    /// Полярный Oklch-вид output projection (hypot/atan2/degrees).
    OklchPolarView,
    /// Точность Oklab-матричного round-trip (linear sRGB → Oklab → linear).
    OklabMatmulPrecision,
}

impl NumericBoundSiteV1 {
    /// Стабильный ключ для inventory.
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::LcsSrgb8Roundtrip => "lcs-srgb8-roundtrip-v1",
            Self::AlphaSourceOverQuantization => "alpha-source-over-quantization-v1",
            Self::SrgbGammaRoundtrip => "srgb-gamma-roundtrip-v1",
            Self::SrgbGamutClamp => "srgb-gamut-clamp-v1",
            Self::OklchPolarView => "oklch-polar-view-v1",
            Self::OklabMatmulPrecision => "oklab-matmul-precision-v1",
        }
    }
}

/// Максимальная ошибка round-trip sRGB8→LCS→sRGB8 по одному каналу,
/// измеренная в шагах сетки sRGB8: после прямого пробега значение
/// возвращается в тот же encoded sRGB8 байт на всём конечном домене,
/// поэтому максимальный сдвиг канала равен 0. Метод: round-trip через
/// `LcsColor::from_srgb8_with_vc`/`to_srgb8_with_vc` (тот же production-путь,
/// но без строковых parse/format-аллокаций тестового harness). Тест ниже
/// исчерпывающе перебирает весь конечный домен encoded sRGB8: 16 777 216
/// стимулов при одной и той же `ViewingConditions::srgb()`. Любой ненулевой
/// сдвиг канала является falsifier; mismatched viewing conditions
/// не входят в applicability envelope и отдельно дают отрицательный control в
/// `lcs::tests::wrong_vc_roundtrip_drifts`.
pub(crate) const LCS_SRGB8_ROUNDTRIP_MAX_CHANNEL_STEPS: u8 = 0;

/// Квантование source-over до u8 использует округление к ближайшему,
/// поэтому ошибка каждого канала результата не превышает половины шага
/// сетки: 1/510 в [0,1]-масштабе относительно вычисленной binary64-композиции
/// в байтах. Ближайший binary64 к этой рациональной границе лежит ниже неё;
/// следующий представимый уровень даёт минимальную консервативную границу.
/// Точный целочисленный oracle ниже проверяет оба соседних представления.
pub(crate) const ALPHA_SOURCE_OVER_MAX_QUANTIZATION_ERROR: f64 =
    f64::from_bits((1.0_f64 / 510.0).to_bits() + 1);

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

/// Граница между вычисленным `encoded = srgb_gamma(linear).clamp(0, 1)`
/// и точным рациональным уровнем выданного байта `byte/255`.
/// Ошибка gamma относительно вещественной формулы и расстояние до значения
/// до clamp сюда не входят. Runtime сохраняет `round(encoded * 255)`.
///
/// На [0,255] ошибка binary64-умножения не больше 2^-46: это половина
/// наибольшего ulp произведения. Последующий round добавляет не больше 0.5
/// байта, поэтому нормированная ошибка <= (0.5 + 2^-46)/255. Одного
/// half-step недостаточно: умножение может округлиться ровно к полубайту.
/// Деление этой константы округлено вверх; точное целочисленное сравнение
/// ниже проверяет enclosure, не округляя измеренную ошибку в binary64.
pub(crate) const SRGB_GAMUT_CLAMP_MAX_CHANNEL_ERROR: f64 = (0.5 + 64.0 * f64::EPSILON) / 255.0;

/// Граница chroma полярного Oklch-вида (output projection), в единицах
/// Oklab chroma. На полной сетке 360 целых углов тест строит rectangular
/// `a`/`b` при nominal chroma 0.2 и проверяет production `hypot` через
/// `derive_oklch_view_v1`. 1e-15 сохраняет прежний допустимый предел,
/// но делает единицы и владельца допуска явными вместо смешивания с hue.
pub(crate) const OKLCH_CHROMA_MAX_ABS_ERROR_OKLAB: f64 = 1.0e-15;

/// Граница hue-вывода полярного Oklch-вида (output projection):
/// production-путь `b.atan2(a).to_degrees()` с канонизацией в [0, 360).
/// Тест рядом с production `derive_oklch_view_v1` вызывает именно эту
/// функцию на полной сетке 360 целых градусов (не случайная выборка),
/// проверяет chroma/hue и отдельный near-neutral контрпример округления
/// к 360°. Абсолютная ошибка hue ограничена 1e-12 градуса.
pub(crate) const OKLCH_HUE_MAX_ABS_ERROR_DEGREES: f64 = 1.0e-12;

/// Максимальная абсолютная ошибка одного linear-sRGB канала после
/// production round-trip `encoded sRGB8 → linear sRGB → Oklab → linear sRGB`.
///
/// Относительная метрика предыдущей staged-волны была выборочной и оказалась
/// непригодна как full-domain закон: production-контрпример `[255, 255, 12]`
/// превышает прежний ceiling `1e-6`. Поэтому здесь остаётся более простая
/// физическая величина в единицах linear-sRGB, без деления на произвольный
/// scale floor. Production-тест ниже перебирает все 16 777 216 encoded-sRGB8
/// стимулов после настоящего decode и требует максимум не выше `3e-7`.
/// Опубликованные матрицы Ottosson — десятичные приближения, поэтому
/// round-trip не заявляется bit-exact.
pub(crate) const OKLAB_MATMUL_ROUNDTRIP_MAX_ABS_ERROR_LINEAR: f64 = 3.0e-7;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alpha::composite_over_srgb8;
    #[allow(
        deprecated,
        reason = "the bound targets the frozen V1 round-trip itself"
    )]
    use crate::lcs::LcsColor;

    /// Реальный round-trip через production API на полном конечном домене
    /// encoded sRGB8. Строковая public-обёртка намеренно обходится: тест живёт
    /// внутри crate и вызывает те же typed production-границы до/после LCS,
    /// убирая 33,5 млн нерелевантных String-аллокаций из proof workload.
    #[test]
    #[allow(
        deprecated,
        reason = "the bound targets the frozen V1 round-trip itself"
    )]
    fn lcs_roundtrip_error_stays_within_bound() {
        use crate::Srgb8;
        use crate::spaces::vc::ViewingConditions;

        let vc = ViewingConditions::srgb();
        let mut visited = 0u32;

        assert_eq!(
            LCS_SRGB8_ROUNDTRIP_MAX_CHANNEL_STEPS, 0,
            "this proof claims byte-exact LCS roundtrip on its declared envelope",
        );
        for packed in 0u32..=0x00ff_ffff {
            let source = [
                ((packed >> 16) & 0xff) as u8,
                ((packed >> 8) & 0xff) as u8,
                (packed & 0xff) as u8,
            ];
            let roundtrip = LcsColor::from_srgb8_with_vc(Srgb8::new(source), &vc)
                .to_srgb8_with_vc(&vc)
                .bytes();
            assert_eq!(
                roundtrip, source,
                "LCS roundtrip must preserve encoded sRGB8 exactly: {source:?} -> {roundtrip:?}",
            );
            visited += 1;
        }

        assert_eq!(visited, 16_777_216, "full sRGB8 domain must be visited");
    }

    /// Ошибка mid-grid достигает точной рациональной половины шага.
    /// Сравнение в целых числах не повторяет округление самой границы;
    /// соседняя четверть шага сохраняет полезный положительный контроль.
    #[test]
    fn alpha_midgrid_counterexample_hits_half_step() {
        let (bound_numerator, bound_denominator) =
            bounded_dyadic(ALPHA_SOURCE_OVER_MAX_QUANTIZATION_ERROR);
        assert!(
            510 * bound_numerator >= bound_denominator,
            "binary64 bound must enclose the exact rational half-step 1/510"
        );
        let (previous_numerator, previous_denominator) = bounded_dyadic(f64::from_bits(
            ALPHA_SOURCE_OVER_MAX_QUANTIZATION_ERROR.to_bits() - 1,
        ));
        assert!(510 * previous_numerator < previous_denominator);
        let result = composite_over_srgb8(
            ALPHA_MIDGRID_TINT,
            ALPHA_MIDGRID_ALPHA,
            ALPHA_MIDGRID_BACKDROP,
        )
        .expect("valid midgrid composition");
        // 255 + (128-255)/2 = 383/2: byte 192 даёт exact error 1/510.
        assert_eq!(result, [192; 3]);
        let half_step_error_numerator = (2 * u128::from(result[0])).abs_diff(383);
        assert_eq!(half_step_error_numerator, 1);
        assert!(half_step_error_numerator * bound_denominator <= 510 * bound_numerator);

        // 255 + (128-255)/4 = 893/4: byte 223 даёт exact error 1/1020.
        let neighbor = composite_over_srgb8(ALPHA_MIDGRID_TINT, 0.25, ALPHA_MIDGRID_BACKDROP)
            .expect("valid neighboring composition");
        assert_eq!(neighbor, [223; 3]);
        let quarter_step_error_numerator = (4 * u128::from(neighbor[0])).abs_diff(893);
        assert_eq!(quarter_step_error_numerator, 1);
        assert!(quarter_step_error_numerator * bound_denominator <= 1020 * bound_numerator);
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
        assert_eq!(
            NumericBoundSiteV1::OklchPolarView.key(),
            "oklch-polar-view-v1"
        );
        assert_eq!(
            NumericBoundSiteV1::OklabMatmulPrecision.key(),
            "oklab-matmul-precision-v1"
        );
    }

    /// Oklab матричный round-trip через production-функции на полном конечном
    /// домене encoded sRGB8. Каждый stimulus сначала проходит настоящий sRGB8
    /// decode; затем измеряется абсолютный linear-sRGB drift каждого канала.
    #[test]
    fn oklab_matmul_roundtrip_full_srgb8_domain_stays_within_bound() {
        use crate::Srgb8;
        use crate::spaces::oklab::{oklab_to_srgb_linear, srgb_linear_to_oklab};
        use crate::spaces::srgb::srgb_linear_from_srgb8;

        // RED/falsifier предыдущей выборочной relative-метрики: этот допустимый
        // encoded stimulus не входил в старый корпус и пробивает ceiling 1e-6.
        let old_metric_source = [255, 255, 12];
        let old_rgb = srgb_linear_from_srgb8(Srgb8::new(old_metric_source));
        let old_roundtrip = oklab_to_srgb_linear(srgb_linear_to_oklab(old_rgb));
        let old_relative =
            (old_roundtrip[2] - old_rgb[2]).abs() / old_rgb[2].abs().max(1.0 / 255.0);
        assert!(
            old_relative > 1.0e-6,
            "full-domain falsifier must exceed the retired sampled relative ceiling; got {old_relative:e}",
        );

        let mut worst_abs = 0.0f64;
        let mut worst_source = [0u8; 3];
        let mut worst_channel = 0usize;
        let mut visited = 0u32;

        for packed in 0u32..=0x00ff_ffff {
            let source = [
                ((packed >> 16) & 0xff) as u8,
                ((packed >> 8) & 0xff) as u8,
                (packed & 0xff) as u8,
            ];
            let rgb = srgb_linear_from_srgb8(Srgb8::new(source));
            let roundtrip = oklab_to_srgb_linear(srgb_linear_to_oklab(rgb));
            for channel in 0..3 {
                let error = (roundtrip[channel] - rgb[channel]).abs();
                assert!(
                    error.is_finite(),
                    "non-finite Oklab round-trip error at {source:?} channel {channel}"
                );
                if error > worst_abs {
                    worst_abs = error;
                    worst_source = source;
                    worst_channel = channel;
                }
            }
            visited += 1;
        }

        assert_eq!(visited, 16_777_216, "full sRGB8 domain must be visited");
        assert!(
            worst_abs <= OKLAB_MATMUL_ROUNDTRIP_MAX_ABS_ERROR_LINEAR,
            "Oklab round-trip worst abs error {worst_abs:e} at {worst_source:?} channel {worst_channel} exceeds bound {bound:e}",
            bound = OKLAB_MATMUL_ROUNDTRIP_MAX_ABS_ERROR_LINEAR,
        );
        assert!(
            worst_abs > 2.0e-7,
            "anti-vacuity: expected published-matrix approximation drift, max was {worst_abs:e} at {worst_source:?} channel {worst_channel}",
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

    /// Независимый oracle для точного binary64-значения в области теста.
    /// Нули представлены отдельно; нормальные значения дают дробь без
    /// floating-point умножения, деления или вычитания.
    fn bounded_dyadic(value: f64) -> (u128, u128) {
        assert!(value.is_finite() && (0.0..=1.0).contains(&value));
        if value == 0.0 {
            return (0, 1);
        }
        let bits = value.to_bits();
        let exponent = ((bits >> 52) & 0x7ff) as i32 - 1023;
        // В тесте минимальное ненулевое encoded около 1/510. Ограничение
        // гарантирует отсутствие overflow и в перекрёстных произведениях.
        assert!((-10..=0).contains(&exponent));
        let numerator = u128::from((bits & ((1_u64 << 52) - 1)) | (1_u64 << 52));
        (numerator, 1_u128 << (52 - exponent))
    }

    /// Полушаговые окрестности обнаруживают double rounding; универсальная
    /// граница следует из оценки двух операций, а не из плотности выборки.
    #[test]
    fn srgb_gamut_clamp_error_stays_within_bound() {
        use crate::spaces::srgb::{
            hex_from_srgb_encoded, srgb_gamma, srgb_gamma_inv, srgb8_from_linear,
        };
        let (bound_numerator, bound_denominator) =
            bounded_dyadic(SRGB_GAMUT_CLAMP_MAX_CHANNEL_ERROR);
        // Выводим (0.5 + 2^-46)/255 независимо от floating-point формулы.
        let exact_numerator = (1_u128 << 45) + 1;
        let exact_denominator = 255_u128 << 46;
        assert!(bound_numerator * exact_denominator >= exact_numerator * bound_denominator);
        // Соседний меньший f64 уже не является верхней границей.
        let (previous_numerator, previous_denominator) = bounded_dyadic(f64::from_bits(
            SRGB_GAMUT_CLAMP_MAX_CHANNEL_ERROR.to_bits() - 1,
        ));
        assert!(previous_numerator * exact_denominator < exact_numerator * previous_denominator);

        // Фиксируем шаг квантования независимо от зависящего от платформы powf:
        // gamma исходного линейного witness даёт это e на проверенной среде.
        // Тот же живой encoded-formatter обязан сохранить прежний байт 33.
        let witness = f64::from_bits(0x3fc0_5050_5050_5050);
        assert_eq!(hex_from_srgb_encoded([witness; 3]), "#212121");
        let (witness_numerator, witness_denominator) = bounded_dyadic(witness);
        let witness_error = 33 * witness_denominator - 255 * witness_numerator;
        assert_eq!(
            (2 * witness_error - witness_denominator) * 114_841_790_497_947_648_u128,
            510 * witness_denominator,
        );

        let mut linear_inputs = vec![
            f64::from_bits(0x3f8e_5bae_05f5_ea9a),
            f64::from_bits(0x3f8e_5bae_05f5_ea8a),
            f64::from_bits(0x3f8e_5bae_05f5_eaaa),
            1.5,
            -0.25,
            0.5,
            1.0 - 0.25 / 255.0,
        ];
        for byte in 0..255 {
            let middle = srgb_gamma_inv((f64::from(byte) + 0.5) / 255.0).to_bits();
            for bits in middle - 4..=middle + 4 {
                linear_inputs.push(f64::from_bits(bits));
            }
        }
        let mut ideal_halfstep_violations = 0;
        for v in linear_inputs {
            let quantized = srgb8_from_linear([v, v, v]);
            let encoded = srgb_gamma(v).clamp(0.0, 1.0);
            let (numerator, denominator) = bounded_dyadic(encoded);
            let byte = u128::from(quantized.bytes()[0]);
            let error_numerator = (byte * denominator).abs_diff(255 * numerator);
            // Оба знаменателя — степени двойки. Их НОК вместо произведения
            // сохраняет точность и запас u128 даже для ошибочного байта 255.
            let common_denominator = denominator.max(bound_denominator);
            assert!(
                error_numerator * (common_denominator / denominator)
                    <= bound_numerator * 255 * (common_denominator / bound_denominator),
                "linear {:016x}, encoded {:016x}, byte {byte}: exact error exceeds bound",
                v.to_bits(),
                encoded.to_bits(),
            );
            ideal_halfstep_violations += usize::from(2 * error_numerator > denominator);
        }
        assert!(
            ideal_halfstep_violations > 0,
            "старый half-step bound должен быть опровергнут"
        );
        assert_eq!(srgb8_from_linear([1.5, 1.5, 1.5]).bytes()[0], 255);
        assert_eq!(srgb8_from_linear([-0.25, -0.25, -0.25]).bytes()[0], 0);
    }
}
