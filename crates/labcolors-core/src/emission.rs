//! Numeric allowance for encoded-sRGB8 material emission.
//!
//! Source-over still has one reference result. Raster pipelines may quantize
//! alpha and premultiplied channels before the final byte. Hard requirements
//! cover this allowance; authored colour character uses the reference result.
//! See docs/explanation/material-emission.md for the local error derivation.

use crate::composition::{AdmittedOpacityV1, source_over_channel_srgb8};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PointEmissionV1 {
    pub(crate) visible_radius: u8,
    pub(crate) backdrop_radius: u8,
    pub(crate) same_as_backdrop: bool,
}

impl PointEmissionV1 {
    pub(crate) const fn exact(self) -> bool {
        self.visible_radius == 0 && self.backdrop_radius == 0
    }
}

pub(crate) fn rgb_bounds(rgb: [u8; 3], radius: u8) -> ([u8; 3], [u8; 3]) {
    (
        rgb.map(|channel| channel.saturating_sub(radius)),
        rgb.map(|channel| channel.saturating_add(radius)),
    )
}

/// An integer bound relative to the reference *rounded byte* at each layer.
/// Opaque paint resets inherited uncertainty; transparent paint preserves it.
pub(crate) fn radius_after(
    raster: bool,
    source: [u8; 3],
    opacity: AdmittedOpacityV1,
    backdrop: [u8; 3],
    visible: [u8; 3],
    backdrop_radius: u8,
) -> u8 {
    if opacity == AdmittedOpacityV1::OPAQUE {
        return 0;
    }
    if opacity.bits() == 0 {
        return backdrop_radius;
    }
    if raster {
        // floor((1-a)*E+3) = E+3-ceil(a*E). Move the only floating
        // product down before ceil; the remaining operations are integers.
        // This is deliberately independent of source RGB and search choices.
        let product = opacity.value() * f64::from(backdrop_radius);
        let lower = if product == 0.0 {
            0.0
        } else {
            f64::from_bits(product.to_bits() - 1)
        };
        return (u16::from(backdrop_radius) + 3)
            .saturating_sub(lower.ceil() as u16)
            .min(255) as u8;
    }
    if backdrop_radius == 0 {
        return 0;
    }
    // A reference-only node can occur between two raster nodes in a low-level
    // declaration. It propagates inherited error instead of erasing it.
    let (lower, upper) = rgb_bounds(backdrop, backdrop_radius);
    let mut radius = 0;
    for channel in 0..3 {
        for endpoint in [lower[channel], upper[channel]] {
            radius = radius.max(
                source_over_channel_srgb8(source[channel], opacity.value(), endpoint)
                    .abs_diff(visible[channel]),
            );
        }
    }
    radius
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(value: f64) -> Result<AdmittedOpacityV1, String> {
        AdmittedOpacityV1::new(value).map_err(|error| format!("alpha: {error:?}"))
    }

    #[test]
    fn actual_browser_samples_fit_the_material_allowance() -> Result<(), String> {
        // Edge 158, solid interior pixels. This is the observed
        // counterexample to accepting the rounded reference without allowance.
        let (source, opacity, backdrop, actual) =
            ([246_u8, 107, 57], 0.72, [107, 127, 114], [207_u8, 112, 73]);
        let nominal =
            std::array::from_fn(|i| source_over_channel_srgb8(source[i], opacity, backdrop[i]));
        let radius = radius_after(true, source, alpha(opacity)?, backdrop, nominal, 0);
        assert_eq!(radius, 3);
        assert!((0..3).all(|i| actual[i].abs_diff(nominal[i]) <= radius));
        Ok(())
    }

    #[test]
    fn published_integer_raster_paths_stay_inside_the_radius() -> Result<(), String> {
        for alpha_byte in 1..255_u16 {
            let opacity = alpha(f64::from(alpha_byte) / 255.0)?;
            for backdrop in 0..=255_u16 {
                for source in [0_u16, 1, 17, 127, 254, 255] {
                    let premul = (source * alpha_byte + 127) / 255;
                    let residual = backdrop * (255 - alpha_byte);
                    let outputs = [
                        premul + (residual + 127) / 255,
                        premul + ((residual + 255) >> 8),
                        premul + ((backdrop * (256 - alpha_byte)) >> 8),
                    ];
                    let nominal =
                        source_over_channel_srgb8(source as u8, opacity.value(), backdrop as u8);
                    for output in outputs {
                        assert!(output.min(255).abs_diff(u16::from(nominal)) <= 3);
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn deep_layers_accumulate_error_and_endpoints_preserve_or_reset_it() -> Result<(), String> {
        let opacity = alpha(0.1)?;
        let mut nominal = 0_u8;
        let mut actual = 0_u8;
        let mut radius = 0_u8;
        for _ in 0..100 {
            let previous = nominal;
            nominal = source_over_channel_srgb8(254, opacity.value(), previous);
            // Published packed law: nearest premul, alpha byte 26, scale256.
            actual = (25_u16 + ((u16::from(actual) * 230) >> 8)).min(255) as u8;
            radius = radius_after(true, [254; 3], opacity, [previous; 3], [nominal; 3], radius);
            assert!(actual.abs_diff(nominal) <= radius);
        }
        assert_eq!((nominal, actual), (250, 237));
        assert!(radius > 3);
        assert_eq!(
            radius_after(
                true,
                [42; 3],
                alpha(0.0)?,
                [nominal; 3],
                [nominal; 3],
                radius
            ),
            radius
        );
        assert_eq!(
            radius_after(true, [42; 3], alpha(1.0)?, [nominal; 3], [42; 3], radius),
            0
        );
        // A reference-only node carries the uncertainty of its raster parent.
        let visible = source_over_channel_srgb8(42, 0.5, nominal);
        let mixed = radius_after(
            false,
            [42; 3],
            alpha(0.5)?,
            [nominal; 3],
            [visible; 3],
            radius,
        );
        assert!(mixed > 0);
        for backdrop in nominal.saturating_sub(radius)..=nominal.saturating_add(radius) {
            assert!(source_over_channel_srgb8(42, 0.5, backdrop).abs_diff(visible) <= mixed);
        }
        Ok(())
    }
}
