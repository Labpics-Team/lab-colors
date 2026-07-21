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

    /// Композиция opacity-конструкторов замкнута в admitted `[0,1]`: два
    /// конечных неотрицательных множителя не могут создать новый invalid state.
    pub(crate) fn multiply(self, rhs: Self) -> Self {
        Self((self.value() * rhs.value()).to_bits())
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

#[cfg(test)]
pub(crate) fn validate_alpha(alpha: f64) -> Result<(), String> {
    AdmittedOpacityV1::new(alpha)
        .map(|_| ())
        .map_err(|_| format!("alpha вне конечного [0,1]: {alpha}"))
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
    use super::AdmittedOpacityV1;

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
}
