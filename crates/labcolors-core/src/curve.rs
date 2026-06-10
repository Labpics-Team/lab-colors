use crate::lcs::LcsColor;
use crate::spaces::vc::ViewingConditions;

/// A parametric colour curve sampled over `t ∈ [0, 1]`.
///
/// Implemented by [`NeutralCurve`](crate::neutral::NeutralCurve) and
/// [`AccentCurve`](crate::scale::AccentCurve) so that downstream consumers
/// (e.g. semantic resolution) can accept either generically.
pub trait ColorCurve {
    /// Colour at normalised position `t`, clamped to `[0, 1]`.
    fn at(&self, t: f64) -> LcsColor;

    /// The viewing conditions this curve was built with.
    ///
    /// Hex conversion MUST go through these conditions — converting a
    /// colour with mismatched VC silently drifts (see the
    /// `wrong_vc_roundtrip_drifts` test in `lcs`).
    fn vc(&self) -> &ViewingConditions;

    /// `n` evenly-spaced samples along the curve.
    ///
    /// Default implementation delegates to [`at`](ColorCurve::at).
    fn sample(&self, n: usize) -> Vec<LcsColor> {
        if n == 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![self.at(0.5)];
        }
        (0..n)
            .map(|i| self.at(i as f64 / (n - 1) as f64))
            .collect()
    }

    /// `n` hex strings sampled through this curve's own viewing conditions.
    fn sample_hex(&self, n: usize) -> Vec<String> {
        self.sample(n)
            .iter()
            .map(|c| c.to_hex_with_vc(self.vc()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::neutral::{CurveParams, NeutralCurve};

    #[test]
    fn dyn_curve_renders_through_own_vc() {
        let vc = ViewingConditions::dim_surround();
        let curve = NeutralCurve::with_vc(
            "#FFFFFF",
            "#787880",
            "#101012",
            &CurveParams::default(),
            &vc,
        )
        .expect("NeutralCurve::with_vc should succeed for valid dim-surround anchors");
        let curve: &dyn ColorCurve = &curve;

        let hexes = curve.sample_hex(13);
        assert_eq!(hexes[0].to_uppercase(), "#FFFFFF");
        assert_eq!(hexes[6].to_uppercase(), "#787880");
        assert_eq!(hexes[12].to_uppercase(), "#101012");
    }
}
