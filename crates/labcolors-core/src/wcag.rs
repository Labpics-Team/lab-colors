//! WCAG 2.1 contrast ratio — the legal conformance floor.
//!
//! The European Accessibility Act (in force 2025-06-28) requires, through
//! EN 301 549, WCAG 2.1 level AA: a contrast ratio of at least 4.5:1 for normal
//! text (success criterion 1.4.3) and 3:1 for user-interface components and
//! graphical objects (1.4.11). This is the *standard relative-luminance*
//! contrast ratio — symmetric and chroma-blind — computed on the output sRGB
//! colour. It is the legal minimum beneath the perceptual LPC target the solver
//! aims for; it never replaces it (see [`crate::solve`]).
//!
//! The implementation below is a faithful, self-contained transcription of the
//! W3C definitions, deliberately independent of the CAM16/LPC pipeline so it can
//! be audited line-by-line against the spec:
//! <https://www.w3.org/TR/WCAG21/#dfn-relative-luminance> and
//! <https://www.w3.org/TR/WCAG21/#dfn-contrast-ratio>.

/// WCAG 2.1 AA minimum contrast ratio for normal text (success criterion 1.4.3).
pub(crate) const AA_TEXT_RATIO: f64 = 4.5;

/// WCAG 2.1 AA minimum contrast ratio for UI components and graphical objects
/// (success criterion 1.4.11).
pub(crate) const AA_UI_RATIO: f64 = 3.0;

/// Linearise one gamma-encoded sRGB channel in `[0, 1]`, per WCAG 2.1 §1.4.3.
///
/// The 0.03928 threshold is the value normatively fixed by WCAG; for 8-bit
/// inputs it selects the same piecewise branch as the IEC sRGB transfer
/// function, so quantised colours linearise identically either way.
fn linearise(channel: f64) -> f64 {
    if channel <= 0.039_28 {
        channel / 12.92
    } else {
        ((channel + 0.055) / 1.055).powf(2.4)
    }
}

/// WCAG 2.1 relative luminance of a gamma-encoded sRGB colour `[r, g, b]` in
/// `[0, 1]`: `0.2126·R + 0.7152·G + 0.0722·B` over the linearised channels.
pub(crate) fn relative_luminance(srgb: [f64; 3]) -> f64 {
    0.2126 * linearise(srgb[0]) + 0.7152 * linearise(srgb[1]) + 0.0722 * linearise(srgb[2])
}

/// WCAG 2.1 contrast ratio between two gamma-encoded sRGB colours, in `[1, 21]`.
///
/// `(L_lighter + 0.05) / (L_darker + 0.05)`. Symmetric and polarity-agnostic by
/// construction — unlike the signed perceptual LPC metric, which is why the two
/// numbers are reported separately and never folded into one.
pub(crate) fn contrast_ratio(a: [f64; 3], b: [f64; 3]) -> f64 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let (lighter, darker) = if la >= lb { (la, lb) } else { (lb, la) };
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// White is `[1,1,1]` (L = 1), black is `[0,0,0]` (L = 0): the canonical
    /// 21:1 extreme.
    #[test]
    fn black_on_white_is_twentyone_to_one() {
        let r = contrast_ratio([1.0, 1.0, 1.0], [0.0, 0.0, 0.0]);
        assert!(
            (r - 21.0).abs() < 1e-9,
            "black on white must be 21:1, got {r}"
        );
    }

    #[test]
    fn ratio_is_symmetric() {
        let white = [1.0, 1.0, 1.0];
        let grey = [0.5, 0.5, 0.5];
        assert!((contrast_ratio(white, grey) - contrast_ratio(grey, white)).abs() < 1e-12);
    }

    #[test]
    fn identical_colours_are_one_to_one() {
        let c = [0.42, 0.13, 0.77];
        assert!((contrast_ratio(c, c) - 1.0).abs() < 1e-12);
    }

    /// `#767676` on white is the textbook AA-text boundary (~4.54:1).
    #[test]
    fn grey_boundary_matches_published_value() {
        let g = 0x76 as f64 / 255.0;
        let r = contrast_ratio([1.0, 1.0, 1.0], [g, g, g]);
        assert!(
            (r - 4.54).abs() < 0.05,
            "#767676 on white should be ~4.54:1, got {r}"
        );
    }
}
