//! Нижний exact-слой point-композиции.
//!
//! Модуль не знает solver, recipe, constraint или client ID. Он фиксирует
//! единственную физическую операцию encoded-sRGB8 source-over, чтобы proposal,
//! appearance runtime и final-emission gate не могли разойтись по арифметике.

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
    if !alpha.is_finite() || !(0.0..=1.0).contains(&alpha) {
        return Err(format!("alpha вне конечного [0,1]: {alpha}"));
    }
    Ok(())
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
