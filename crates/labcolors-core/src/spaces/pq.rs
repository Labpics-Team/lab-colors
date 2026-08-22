//! Perceptual Quantizer (PQ / SMPTE ST 2084) transfer functions and typed luminance.
//!
//! Provides the inverse EOTF (linear absolute luminance → PQ code value),
//! a validated absolute-luminance newtype bounded to the PQ representable range,
//! and the PQ code-value newtype used by HDR output projection.

/// Minimum absolute luminance representable in PQ (cd/m²). ~1e-7 nits.
const PQ_MIN_LUMINANCE: f64 = 1e-7;
/// Maximum absolute luminance representable in PQ (cd/m²). 10 000 nits.
const PQ_MAX_LUMINANCE: f64 = 10_000.0;

// PQ constants from SMPTE ST 2084.
const M1: f64 = 0.159_301_757_812_5; // 2610 / 16384
const M2: f64 = 78.84375; // 2523 / 32
const C1: f64 = 0.835_937_5; // 3424 / 4096
const C2: f64 = 18.851_562_5; // 2413 / 128
const C3: f64 = 18.6875; // 2392 / 128

/// Error returned when a luminance or PQ value falls outside the valid range.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HdrNumericalErrorV1 {
    pub message: String,
}

impl core::fmt::Display for HdrNumericalErrorV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "HDR numerical error: {}", self.message)
    }
}

/// Validated absolute luminance in cd/m², bounded to the PQ representable range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AbsoluteLuminanceV1(f64);

impl AbsoluteLuminanceV1 {
    /// Minimum PQ-representable luminance.
    pub const PQ_MIN: f64 = PQ_MIN_LUMINANCE;
    /// Maximum PQ-representable luminance.
    pub const PQ_MAX: f64 = PQ_MAX_LUMINANCE;

    /// Construct a validated luminance. Returns error if out of PQ range or NaN.
    pub fn try_new(value: f64) -> Result<Self, HdrNumericalErrorV1> {
        if value.is_nan() {
            return Err(HdrNumericalErrorV1 {
                message: "luminance is NaN".into(),
            });
        }
        if !(PQ_MIN_LUMINANCE..=PQ_MAX_LUMINANCE).contains(&value) {
            return Err(HdrNumericalErrorV1 {
                message: format!(
                    "luminance {value} outside PQ range [{PQ_MIN_LUMINANCE}, {PQ_MAX_LUMINANCE}]"
                ),
            });
        }
        Ok(Self(value))
    }

    /// Construct without validation. Caller guarantees the value is in range.
    pub fn new_unchecked(value: f64) -> Self {
        debug_assert!(
            !value.is_nan() && (PQ_MIN_LUMINANCE..=PQ_MAX_LUMINANCE).contains(&value),
            "new_unchecked called with out-of-range value {value}"
        );
        Self(value)
    }

    /// Raw f64 value in cd/m².
    pub fn value(self) -> f64 {
        self.0
    }
}

/// PQ code value in [0, 1]. Produced by `pq_inverse_eotf`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PqCodeValueV1(f64);

impl PqCodeValueV1 {
    /// Construct a validated PQ code value. Must be in [0, 1] and finite.
    #[allow(dead_code)]
    pub fn try_new(value: f64) -> Result<Self, HdrNumericalErrorV1> {
        if value.is_nan() || !(0.0..=1.0).contains(&value) {
            return Err(HdrNumericalErrorV1 {
                message: format!("PQ code value {value} outside [0, 1]"),
            });
        }
        Ok(Self(value))
    }

    /// Raw f64 value.
    pub fn value(self) -> f64 {
        self.0
    }
}

/// SMPTE ST 2084 inverse EOTF: absolute luminance (cd/m²) → PQ code value.
///
/// Input must be non-negative. Values above 10 000 nits are clamped.
pub(crate) fn pq_inverse_eotf(luminance: AbsoluteLuminanceV1) -> PqCodeValueV1 {
    let y = luminance.value().max(0.0) / PQ_MAX_LUMINANCE;
    let ym1 = y.powf(M1);
    let num = C1 + C2 * ym1;
    let den = 1.0 + C3 * ym1;
    let pq = (num / den).powf(M2);
    PqCodeValueV1(pq.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pq_black_is_zero() {
        let lum = AbsoluteLuminanceV1::try_new(PQ_MIN_LUMINANCE).unwrap();
        let pq = pq_inverse_eotf(lum);
        assert!(pq.value() < 1e-4, "black should map near zero PQ");
    }

    #[test]
    fn pq_peak_white_is_one() {
        let lum = AbsoluteLuminanceV1::try_new(PQ_MAX_LUMINANCE).unwrap();
        let pq = pq_inverse_eotf(lum);
        assert!(
            (pq.value() - 1.0).abs() < 1e-9,
            "peak white should map to PQ=1, got {}",
            pq.value()
        );
    }

    #[test]
    fn luminance_out_of_range_rejected() {
        assert!(AbsoluteLuminanceV1::try_new(-1.0).is_err());
        assert!(AbsoluteLuminanceV1::try_new(20_000.0).is_err());
        assert!(AbsoluteLuminanceV1::try_new(f64::NAN).is_err());
    }
}
