//! Нижний exact-слой point-композиции.
//!
//! Модуль не знает solver, recipe, constraint или client ID. Он фиксирует
//! единственную физическую операцию encoded-sRGB8 source-over, чтобы proposal,
//! appearance runtime и revision-bound recheck не могли разойтись по арифметике.

/// Typed отказ admission straight alpha. Диагностический transport-текст
/// строится только legacy façade-ом; нижняя физика не хранит stringly error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpacityAdmissionErrorV1 {
    NonFinite,
    OutsideUnitInterval,
}

/// Версионированная identity единственной point-операции композиции.
///
/// Профиль и исполняющий его закон живут вместе: graph executor, alpha-аналог
/// и certificate replay не вправе независимо выбирать арифметику по тому же
/// discriminant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum CompositionProfileV1 {
    /// Straight-alpha source-over в encoded-sRGB8 с одним округлением каждого
    /// финального канала occurrence. Это не модель произвольного renderer/HDR.
    EncodedSrgb8SourceOverV1,
}

/// Канонический straight alpha внутри конечного `[0,1]`.
///
/// Значение хранится битами: `-0.0` понижается в единственный физический `+0.0`
/// state, а все остальные binary64 значения сохраняются точно.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdmittedOpacityV1(u64);

impl AdmittedOpacityV1 {
    pub(crate) const TRANSPARENT: Self = Self(0.0f64.to_bits());
    pub(crate) const OPAQUE: Self = Self(1.0f64.to_bits());

    pub(crate) fn new(alpha: f64) -> Result<Self, OpacityAdmissionErrorV1> {
        if !alpha.is_finite() {
            return Err(OpacityAdmissionErrorV1::NonFinite);
        }
        if !(0.0..=1.0).contains(&alpha) {
            return Err(OpacityAdmissionErrorV1::OutsideUnitInterval);
        }
        let canonical = if alpha == 0.0 { 0.0 } else { alpha };
        Ok(Self(canonical.to_bits()))
    }

    pub(crate) const fn bits(self) -> u64 {
        self.0
    }

    pub(crate) const fn value(self) -> f64 {
        f64::from_bits(self.0)
    }

    /// Непосредственный binary64 predecessor внутри канонического `[0,1]`.
    pub(crate) const fn predecessor(self) -> Option<Self> {
        if self.0 == Self::TRANSPARENT.0 {
            None
        } else {
            Some(Self(self.0 - 1))
        }
    }

    /// Композиция opacity-конструкторов замкнута в admitted `[0,1]`: два
    /// конечных неотрицательных множителя не могут создать новый invalid state.
    pub(crate) fn multiply(self, rhs: Self) -> Self {
        Self((self.value() * rhs.value()).to_bits())
    }
}

/// Typed отказ admission замкнутого opacity-domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpacityDomainAdmissionErrorV1 {
    /// Нижняя граница сама не является допустимой opacity.
    InvalidLower(OpacityAdmissionErrorV1),
    /// Верхняя граница сама не является допустимой opacity.
    InvalidUpper(OpacityAdmissionErrorV1),
    /// Замкнутый интервал был объявлен в обратном порядке.
    Reversed,
}

/// Непустой замкнутый interval допустимых straight-alpha значений.
///
/// Fixed opacity представляется тем же типом с равными границами: физический
/// слой не получает второй variant и не может случайно применить recourse к
/// числу, которое клиент объявил фиксированным.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpacityDomainV1 {
    lower: AdmittedOpacityV1,
    upper: AdmittedOpacityV1,
}

impl OpacityDomainV1 {
    pub(crate) fn try_new(lower: f64, upper: f64) -> Result<Self, OpacityDomainAdmissionErrorV1> {
        let lower =
            AdmittedOpacityV1::new(lower).map_err(OpacityDomainAdmissionErrorV1::InvalidLower)?;
        let upper =
            AdmittedOpacityV1::new(upper).map_err(OpacityDomainAdmissionErrorV1::InvalidUpper)?;
        Self::try_from_admitted(lower, upper)
    }

    fn try_from_admitted(
        lower: AdmittedOpacityV1,
        upper: AdmittedOpacityV1,
    ) -> Result<Self, OpacityDomainAdmissionErrorV1> {
        // Для канонических неотрицательных finite binary64 в `[0,1]` unsigned
        // bit order совпадает с числовым; сравнение не создаёт float-policy.
        if lower.bits() > upper.bits() {
            return Err(OpacityDomainAdmissionErrorV1::Reversed);
        }
        Ok(Self { lower, upper })
    }

    pub(crate) const fn lower(self) -> AdmittedOpacityV1 {
        self.lower
    }

    pub(crate) const fn upper(self) -> AdmittedOpacityV1 {
        self.upper
    }

    pub(crate) const fn contains(self, opacity: AdmittedOpacityV1) -> bool {
        self.lower.bits() <= opacity.bits() && opacity.bits() <= self.upper.bits()
    }
}

impl CompositionProfileV1 {
    /// Stable identity of this executable composition profile.
    #[cfg(test)]
    pub const fn key(self) -> &'static str {
        match self {
            Self::EncodedSrgb8SourceOverV1 => "encoded-srgb8-source-over-v1",
        }
    }

    /// Исполняет ровно тот закон, identity которого несёт профиль.
    pub(crate) fn composite(
        self,
        tint: [u8; 3],
        alpha: AdmittedOpacityV1,
        backdrop: [u8; 3],
    ) -> [u8; 3] {
        match self {
            Self::EncodedSrgb8SourceOverV1 => {
                #[cfg(test)]
                SOURCE_OVER_EVALUATIONS.with(|count| count.set(count.get() + 1));
                let alpha = alpha.value();
                [
                    source_over_channel_srgb8(tint[0], alpha, backdrop[0]),
                    source_over_channel_srgb8(tint[1], alpha, backdrop[1]),
                    source_over_channel_srgb8(tint[2], alpha, backdrop[2]),
                ]
            }
        }
    }
}

/// Это declared binary64 operation order официального runtime: JS вызывает
/// Core, отдельной формулы у него нет. Expanded-форма запрещена: два округления
/// нарушают монотонность на отдельных ULP-швах.
pub(crate) fn source_over_channel_value(tint: u8, alpha: f64, backdrop: u8) -> f64 {
    f64::from(backdrop) + alpha * (f64::from(tint) - f64::from(backdrop))
}

pub(crate) fn source_over_channel_srgb8(tint: u8, alpha: f64, backdrop: u8) -> u8 {
    source_over_channel_value(tint, alpha, backdrop).round() as u8
}

pub(crate) fn source_over_srgb8(
    tint: [u8; 3],
    alpha: f64,
    backdrop: [u8; 3],
) -> Result<[u8; 3], String> {
    let alpha =
        AdmittedOpacityV1::new(alpha).map_err(|_| format!("alpha вне конечного [0,1]: {alpha}"))?;
    Ok(CompositionProfileV1::EncodedSrgb8SourceOverV1.composite(tint, alpha, backdrop))
}

#[cfg(test)]
std::thread_local! {
    static SOURCE_OVER_EVALUATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_source_over_evaluation_count() {
    SOURCE_OVER_EVALUATIONS.with(|count| count.set(0));
}

#[cfg(test)]
pub(crate) fn source_over_evaluation_count() -> usize {
    SOURCE_OVER_EVALUATIONS.with(std::cell::Cell::get)
}

#[cfg(test)]
mod tests {
    use super::{
        AdmittedOpacityV1, OpacityAdmissionErrorV1, OpacityDomainAdmissionErrorV1, OpacityDomainV1,
    };

    #[test]
    fn opacity_domain_rejects_invalid_or_reversed_boundaries_before_execution() {
        assert_eq!(
            OpacityDomainV1::try_new(f64::NAN, 1.0),
            Err(OpacityDomainAdmissionErrorV1::InvalidLower(
                OpacityAdmissionErrorV1::NonFinite,
            )),
        );
        assert_eq!(
            OpacityDomainV1::try_new(0.0, 1.0 + f64::EPSILON),
            Err(OpacityDomainAdmissionErrorV1::InvalidUpper(
                OpacityAdmissionErrorV1::OutsideUnitInterval,
            )),
        );
        assert_eq!(
            OpacityDomainV1::try_new(0.75, 0.25),
            Err(OpacityDomainAdmissionErrorV1::Reversed),
        );
    }

    #[test]
    fn opacity_domain_canonicalises_zero_and_represents_fixed_without_a_second_type() {
        let domain = OpacityDomainV1::try_new(-0.0, 1.0).unwrap();
        assert_eq!(domain.lower().bits(), 0.0_f64.to_bits());
        assert_eq!(domain.upper(), AdmittedOpacityV1::OPAQUE);

        let fixed = OpacityDomainV1::try_new(0.375, 0.375).unwrap();
        assert_eq!(fixed.lower(), fixed.upper());
        assert_eq!(fixed.lower().value().to_bits(), 0.375_f64.to_bits());

        let bounded = OpacityDomainV1::try_new(0.25, 0.75).unwrap();
        assert!(bounded.contains(AdmittedOpacityV1::new(0.5).unwrap()));
        assert!(!bounded.contains(AdmittedOpacityV1::TRANSPARENT));
        assert!(!bounded.contains(AdmittedOpacityV1::OPAQUE));
    }

    #[test]
    fn admitted_opacity_multiplication_is_closed_at_boundaries_and_underflow() {
        let zero = AdmittedOpacityV1::new(0.0).unwrap();
        let one = AdmittedOpacityV1::new(1.0).unwrap();
        assert_eq!(zero.multiply(zero).value(), 0.0);
        assert_eq!(one.multiply(one).value(), 1.0);

        let smallest_subnormal = AdmittedOpacityV1::new(f64::from_bits(1)).unwrap();
        let half = AdmittedOpacityV1::new(0.5).unwrap();
        let underflow = smallest_subnormal.multiply(half);
        assert_eq!(underflow.value(), 0.0);
        assert_eq!(underflow.bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn admitted_opacity_predecessor_is_exact_and_stays_inside_the_domain() {
        assert_eq!(AdmittedOpacityV1::TRANSPARENT.predecessor(), None);
        let predecessor = AdmittedOpacityV1::OPAQUE.predecessor().unwrap();
        assert_eq!(predecessor.bits(), 1.0_f64.to_bits() - 1);
        assert!(predecessor.value() < 1.0);
    }
}
