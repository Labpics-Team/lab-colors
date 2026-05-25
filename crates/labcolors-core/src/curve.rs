use crate::lcs::LcsColor;

/// A parametric colour curve sampled over `t ∈ [0, 1]`.
///
/// Implemented by [`NeutralCurve`](crate::neutral::NeutralCurve) and
/// [`AccentCurve`](crate::scale::AccentCurve) so that downstream consumers
/// (e.g. semantic resolution) can accept either generically.
pub trait ColorCurve {
    /// Colour at normalised position `t`, clamped to `[0, 1]`.
    fn at(&self, t: f64) -> LcsColor;

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
}
