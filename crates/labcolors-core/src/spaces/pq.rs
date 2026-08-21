//! SMPTE ST 2084 (PQ) transfer function primitives.
//!
//! Reference: SMPTE ST 2084:2014 Equations 4.1 (EOTF) and 5.1 (Inverse EOTF).
//! Domain: absolute luminance [0.0001, 10000.0] cd/m².

/// Absolute luminance in cd/m². Valid PQ domain: [0.0001, 10000.0].
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct AbsoluteLuminanceV1(f64);

impl AbsoluteLuminanceV1 {
    pub const PQ_MIN: f64 = 0.0001;
    pub const PQ_MAX: f64 = 10_000.0;

    pub fn try_new(cd_per_m2: f64) -> Result<Self, HdrNumericalErrorV1> {
        if !cd_per_m2.is_finite() {
            return Err(HdrNumericalErrorV1::NonFinite);
        }
        if cd_per_m2 < Self::PQ_MIN || cd_per_m2 > Self::PQ_MAX {
            return Err(HdrNumericalErrorV1::OutOfRange {
                value: cd_per_m2,
                min: Self::PQ_MIN,
                max: Self::PQ_MAX,
            });
        }
        Ok(Self(cd_per_m2))
    }

    /// Construct without validation. Caller guarantees invariant.
    /// SAFETY INVARIANT: value must be finite and within [PQ_MIN, PQ_MAX].
    pub(crate) fn new_unchecked(cd_per_m2: f64) -> Self {
        Self(cd_per_m2)
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

/// PQ code value in [0, 1]. Normalized per SMPTE ST 2084.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct PqCodeValueV1(f64);

impl PqCodeValueV1 {
    pub fn try_new(normalized: f64) -> Result<Self, HdrNumericalErrorV1> {
        if !normalized.is_finite() {
            return Err(HdrNumericalErrorV1::NonFinite);
        }
        if !(0.0..=1.0).contains(&normalized) {
            return Err(HdrNumericalErrorV1::OutOfRange {
                value: normalized,
                min: 0.0,
                max: 1.0,
            });
        }
        Ok(Self(normalized))
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

/// Numerical error specific to HDR/PQ operations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HdrNumericalErrorV1 {
    NonFinite,
    OutOfRange { value: f64, min: f64, max: f64 },
    RoundTripExceededTolerance { expected: f64, actual: f64, tolerance: f64 },
}

// PQ constants per SMPTE ST 2084
const M1: f64 = 0.1593017578125; // 2610 / 16384
const M2: f64 = 78.84375;        // 2523 / 32
const C1: f64 = 0.8359375;       // 3424 / 4096
const C2: f64 = 18.8515625;      // 2413 / 128
const C3: f64 = 18.6875;         // 2392 / 128

/// PQ EOTF: PQ code value → absolute luminance (cd/m²).
/// SMPTE ST 2084:2014 Equation 4.1.
pub fn pq_eotf(code: PqCodeValueV1) -> AbsoluteLuminanceV1 {
    let n = code.value();
    let nm2 = n.powf(1.0 / M2);
    let num = (nm2 - C1).max(0.0);
    let den = C2 - C3 * nm2;
    // Guard against division by zero at extreme codes
    let y = if den.abs() < 1e-15 { 0.0 } else { (num / den).powf(1.0 / M1) };
    // Scale: PQ reference peak is 10000 cd/m²
    AbsoluteLuminanceV1::new_unchecked(y * 10_000.0)
}

/// PQ Inverse EOTF: absolute luminance → PQ code value.
/// SMPTE ST 2084:2014 Equation 5.1.
pub fn pq_inverse_eotf(luminance: AbsoluteLuminanceV1) -> PqCodeValueV1 {
    let y = luminance.value() / 10_000.0;
    let ym1 = y.powf(M1);
    let num = C1 + C2 * ym1;
    let den = 1.0 + C3 * ym1;
    let n = (num / den).powf(M2);
    PqCodeValueV1(n.clamp(0.0, 1.0))
}

/// Verify round-trip fidelity: |pq_eotf(pq_inverse_eotf(L)) - L| ≤ tolerance.
pub fn verify_pq_roundtrip(luminance: AbsoluteLuminanceV1) -> Result<(), HdrNumericalErrorV1> {
    let code = pq_inverse_eotf(luminance);
    let reconstructed = pq_eotf(code);
    let delta = (reconstructed.value() - luminance.value()).abs();
    let tolerance = if luminance.value() < 1000.0 { 1e-4 } else { 1e-2 };
    if delta > tolerance {
        return Err(HdrNumericalErrorV1::RoundTripExceededTolerance {
            expected: luminance.value(),
            actual: reconstructed.value(),
            tolerance,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pq_roundtrip_log_spaced() {
        for i in 0..100 {
            let log_lum = AbsoluteLuminanceV1::PQ_MIN.ln()
                + (AbsoluteLuminanceV1::PQ_MAX.ln() - AbsoluteLuminanceV1::PQ_MIN.ln()) * (i as f64 / 99.0);
            let lum = AbsoluteLuminanceV1::try_new(log_lum.exp()).expect("valid luminance");
            verify_pq_roundtrip(lum).expect("round-trip within tolerance");
        }
    }

    #[test]
    fn pq_boundary_values() {
        let min = AbsoluteLuminanceV1::try_new(AbsoluteLuminanceV1::PQ_MIN).unwrap();
        let max = AbsoluteLuminanceV1::try_new(AbsoluteLuminanceV1::PQ_MAX).unwrap();
        verify_pq_roundtrip(min).unwrap();
        verify_pq_roundtrip(max).unwrap();
    }

    #[test]
    fn pq_code_domain_rejects_invalid() {
        assert!(PqCodeValueV1::try_new(-0.001).is_err());
        assert!(PqCodeValueV1::try_new(1.001).is_err());
        assert!(PqCodeValueV1::try_new(f64::NAN).is_err());
    }
}