use crate::Srgb8;
use crate::lcs::LcsColor;
use crate::spaces::vc::ViewingConditions;

/// A finite normalised position on a colour curve.
///
/// Construction is the public input boundary: once this value exists, curve
/// evaluation cannot receive `NaN`, infinity, or an out-of-domain scalar and
/// therefore never needs a clamp or plausible-colour fallback.
///
/// ```compile_fail
/// use labcolors_core::neutral::NeutralCurve;
/// let curve = NeutralCurve::new("#FFFFFF", "#808080", "#000000").unwrap();
/// curve.at(f64::NAN);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct CurvePosition(f64);

/// Why a raw scalar cannot become a [`CurvePosition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurvePositionError {
    NonFinite,
    OutsideUnitInterval,
}

impl CurvePosition {
    pub const START: Self = Self(0.0);
    pub const MIDPOINT: Self = Self(0.5);
    pub const END: Self = Self(1.0);

    pub fn new(value: f64) -> Result<Self, CurvePositionError> {
        if !value.is_finite() {
            return Err(CurvePositionError::NonFinite);
        }
        if !(0.0..=1.0).contains(&value) {
            return Err(CurvePositionError::OutsideUnitInterval);
        }
        // One canonical representation makes `-0.0` and `0.0` the same domain
        // value instead of leaking an irrelevant IEEE sign bit into identity.
        Ok(Self(if value == 0.0 { 0.0 } else { value }))
    }

    pub const fn get(self) -> f64 {
        self.0
    }

    fn from_sample(index: usize, last: usize) -> Self {
        debug_assert!(last > 0 && index <= last);
        Self(index as f64 / last as f64)
    }
}

impl TryFrom<f64> for CurvePosition {
    type Error = CurvePositionError;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl std::fmt::Display for CurvePositionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinite => f.write_str("curve position must be finite"),
            Self::OutsideUnitInterval => {
                f.write_str("curve position must be within the closed interval [0, 1]")
            }
        }
    }
}

impl std::error::Error for CurvePositionError {}

/// A parametric colour curve sampled over `t ∈ [0, 1]`.
///
/// Implemented by [`NeutralCurve`](crate::neutral::NeutralCurve) and
/// [`AccentCurve`](crate::scale::AccentCurve) so that downstream consumers
/// (e.g. semantic resolution) can accept either generically.
pub trait ColorCurve {
    /// Colour at a validated normalised position.
    fn at(&self, position: CurvePosition) -> LcsColor;

    /// The viewing conditions this curve was built with.
    ///
    /// Hex conversion MUST go through these conditions — converting a
    /// colour with mismatched VC silently drifts (see the
    /// `wrong_vc_roundtrip_drifts` test in `lcs`).
    fn vc(&self) -> &ViewingConditions;

    /// Quantise one continuous point at the typed final-output boundary.
    ///
    /// Implementations may preserve an exact representation constraint here
    /// (for example, an authored neutral axis), but [`Self::at`] itself remains
    /// continuous and never snaps to the output lattice.
    fn render_srgb8(&self, position: CurvePosition) -> Srgb8 {
        self.at(position).to_srgb8_with_vc(self.vc())
    }

    /// `n` evenly-spaced samples along the curve.
    ///
    /// Default implementation delegates to [`at`](ColorCurve::at).
    fn sample(&self, n: usize) -> Vec<LcsColor> {
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![self.at(CurvePosition::MIDPOINT)];
        }
        (0..n)
            .map(|i| self.at(CurvePosition::from_sample(i, n - 1)))
            .collect()
    }

    /// `n` hex strings sampled through this curve's own viewing conditions.
    fn sample_hex(&self, n: usize) -> Vec<String> {
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![self.render_srgb8(CurvePosition::MIDPOINT).to_hex()];
        }
        (0..n)
            .map(|i| {
                self.render_srgb8(CurvePosition::from_sample(i, n - 1))
                    .to_hex()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::neutral::NeutralCurve;
    use crate::scale::AccentCurve;

    #[test]
    fn curve_position_rejects_nonfinite_and_out_of_domain_values() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(
                CurvePosition::try_from(value),
                Err(CurvePositionError::NonFinite)
            );
        }
        for value in [
            -f64::from_bits(1),
            f64::from_bits(1.0_f64.to_bits() + 1),
            -f64::MAX,
            f64::MAX,
        ] {
            assert_eq!(
                CurvePosition::try_from(value),
                Err(CurvePositionError::OutsideUnitInterval)
            );
        }
        assert_eq!(CurvePosition::try_from(-0.0).unwrap().get().to_bits(), 0);
        assert_eq!(CurvePosition::try_from(1.0).unwrap().get(), 1.0);
    }

    /// Dedup guard (issue #6): both curves must reach `sample()` through the
    /// ONE trait default, not through per-struct inherent copies. A `&dyn
    /// ColorCurve` can only call the trait method, so if any struct stops
    /// implementing `ColorCurve` — the exact regression that left this issue
    /// half-done for either curve — this test fails to compile. The value
    /// assertions pin that the shared default reproduces each curve's own ramp,
    /// so removing the inherent bodies changed no output.
    #[test]
    fn every_curve_samples_through_the_shared_trait_default() {
        let neutral = NeutralCurve::new("#FFFFFF", "#787880", "#101012")
            .expect("canonical anchors are valid");
        let accent = AccentCurve::new("#007AFF", &neutral).expect("#007AFF is a valid accent seed");
        for curve in [&neutral as &dyn ColorCurve, &accent as &dyn ColorCurve] {
            // n == 0 / n == 1 / n > 1 branches of the single default body.
            assert!(curve.sample(0).is_empty(), "n=0 must be empty");
            assert_eq!(
                curve.sample(1),
                vec![curve.at(CurvePosition::MIDPOINT)],
                "n=1 is the midpoint"
            );
            let five = curve.sample(5);
            assert_eq!(five.len(), 5, "n>1 returns n samples");
            assert_eq!(
                five[0],
                curve.at(CurvePosition::START),
                "first sample is t=0"
            );
            assert_eq!(five[4], curve.at(CurvePosition::END), "last sample is t=1");
        }
    }

    #[test]
    fn dyn_curve_renders_through_own_vc() {
        let vc = ViewingConditions::dim_surround();
        let curve = NeutralCurve::with_vc("#FFFFFF", "#787880", "#101012", &vc)
            .expect("NeutralCurve::with_vc should succeed for valid dim-surround anchors");
        let curve: &dyn ColorCurve = &curve;

        let hexes = curve.sample_hex(13);
        assert_eq!(hexes[0].to_uppercase(), "#FFFFFF");
        assert_eq!(hexes[6].to_uppercase(), "#787880");
        assert_eq!(hexes[12].to_uppercase(), "#101012");
    }
}
