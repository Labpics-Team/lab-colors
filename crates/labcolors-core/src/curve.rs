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
        (0..n).map(|i| self.at(i as f64 / (n - 1) as f64)).collect()
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
    use crate::scale::AccentCurve;
    use crate::sentiment::{Sentiment, SentimentCurve};

    /// Dedup guard (issue #6): all three curves must reach `sample()` through the
    /// ONE trait default, not through per-struct inherent copies. A `&dyn
    /// ColorCurve` can only call the trait method, so if any struct stops
    /// implementing `ColorCurve` — the exact regression that left this issue
    /// half-done for `SentimentCurve` — this test fails to compile. The value
    /// assertions pin that the shared default reproduces each curve's own ramp,
    /// so removing the inherent bodies changed no output.
    #[test]
    fn every_curve_samples_through_the_shared_trait_default() {
        let neutral = NeutralCurve::new("#FFFFFF", "#787880", "#101012")
            .expect("canonical anchors are valid");
        let accent = AccentCurve::new("#007AFF", &neutral).expect("#007AFF is a valid accent seed");
        let sentiment = SentimentCurve::from_sentiment(Sentiment::Info, 33.5, "#3E87FF", &neutral)
            .expect("Info sentiment resolves on the canonical neutral");

        for curve in [
            &neutral as &dyn ColorCurve,
            &accent as &dyn ColorCurve,
            &sentiment as &dyn ColorCurve,
        ] {
            // n == 0 / n == 1 / n > 1 branches of the single default body.
            assert!(curve.sample(0).is_empty(), "n=0 must be empty");
            assert_eq!(curve.sample(1), vec![curve.at(0.5)], "n=1 is the midpoint");
            let five = curve.sample(5);
            assert_eq!(five.len(), 5, "n>1 returns n samples");
            assert_eq!(five[0], curve.at(0.0), "first sample is t=0");
            assert_eq!(five[4], curve.at(1.0), "last sample is t=1");
        }
    }

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
