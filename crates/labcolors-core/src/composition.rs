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

/// Канонический straight alpha внутри конечного `[0,1]`.
///
/// Значение хранится битами: `-0.0` понижается в единственный физический `+0.0`
/// state, а все остальные binary64 значения сохраняются точно.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdmittedOpacityV1(u64);

impl AdmittedOpacityV1 {
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

    #[cfg(test)]
    pub(crate) const fn bits(self) -> u64 {
        self.0
    }

    pub(crate) const fn value(self) -> f64 {
        f64::from_bits(self.0)
    }
}

/// Порядок binary64-операций совпадает с официальным JS-потребителем на
/// непрозрачной подложке. Expanded-форма запрещена: два округления нарушают
/// монотонность на отдельных ULP-швах.
pub(crate) fn source_over_channel_value(tint: u8, alpha: f64, backdrop: u8) -> f64 {
    f64::from(backdrop) + alpha * (f64::from(tint) - f64::from(backdrop))
}

pub(crate) fn source_over_channel_srgb8(tint: u8, alpha: f64, backdrop: u8) -> u8 {
    source_over_channel_value(tint, alpha, backdrop).round() as u8
}

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
    validate_alpha(alpha)?;
    Ok(source_over_srgb8_validated(tint, alpha, backdrop))
}

pub(crate) fn source_over_srgb8_validated(tint: [u8; 3], alpha: f64, backdrop: [u8; 3]) -> [u8; 3] {
    debug_assert!(validate_alpha(alpha).is_ok());
    #[cfg(test)]
    SOURCE_OVER_EVALUATIONS.with(|count| count.set(count.get() + 1));
    [
        source_over_channel_srgb8(tint[0], alpha, backdrop[0]),
        source_over_channel_srgb8(tint[1], alpha, backdrop[1]),
        source_over_channel_srgb8(tint[2], alpha, backdrop[2]),
    ]
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
