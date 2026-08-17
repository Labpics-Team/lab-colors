//! Private whole-field operators and renderer-bound evidence.
//!
//! The module is deliberately role-agnostic. It evaluates typed raster
//! operations, records the exact finite kernel and render context, and only
//! mints a whole-field certificate from a complete reference raster or a
//! prospective host observation bound to the same request and scene revision.

use crate::Srgb8;
use crate::observation::{ObservationStreamId, Revision};
use crate::session::SessionObservationBindingPermitV1;
use crate::sha256::Hasher;

const Q32_NORMALIZATION: u64 = 1_u64 << 32;
// A radius r uses row 2r. Exact integer Q32 probabilities require 2r <= 32;
// wider rows need a separately versioned quantisation law rather than rounding.
const MAX_EXACT_BINOMIAL_DEVICE_RADIUS_PX: u32 = 16;

macro_rules! opaque_u64_id {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub(crate) struct $name(u64);

        impl $name {
            pub(crate) const fn new(raw: u64) -> Self {
                Self(raw)
            }

            const fn value(self) -> u64 {
                self.0
            }
        }
    };
}

opaque_u64_id!(FieldRequestIdV1);
opaque_u64_id!(FieldOperatorInstanceIdV1);
opaque_u64_id!(FieldRasterIdentityV1);
opaque_u64_id!(FieldEvidenceIdentityV1);
opaque_u64_id!(FieldRendererIdV1);
opaque_u64_id!(FieldHostConformanceIdV1);
opaque_u64_id!(FieldUnsupportedReasonIdV1);

macro_rules! digest_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub(crate) struct $name([u8; 32]);
    };
}

digest_type!(FieldRequestDigestV1);
digest_type!(FieldRasterDigestV1);
digest_type!(FieldKernelDigestV1);
digest_type!(FieldCertificateDigestV1);
digest_type!(FieldEvaluationLayoutDigestV1);

impl FieldRequestDigestV1 {
    const fn as_bytes(self) -> [u8; 32] {
        self.0
    }

    #[cfg(test)]
    pub(crate) const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl FieldRasterDigestV1 {
    const fn as_bytes(self) -> [u8; 32] {
        self.0
    }

    #[cfg(test)]
    pub(crate) const fn from_bytes_for_test(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl FieldKernelDigestV1 {
    const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Typed failures at the field admission, evaluation, and proof boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldEvaluationErrorV1 {
    EmptyExtent,
    EmptyRect,
    GeometryOverflow,
    RectOutsideExtent,
    ExtentMismatch,
    RasterLengthMismatch {
        expected: usize,
        actual: usize,
    },
    InvalidPremultipliedPixel {
        channels: [u8; 4],
    },
    NonFiniteOpacity,
    OpacityOutsideUnitInterval,
    UnsupportedDevicePixelRatio {
        actual: u8,
    },
    InvalidKernelShape,
    KernelWeightsNotNormalized,
    KernelWeightsNotSymmetric,
    KernelWeightsDoNotMatchProfile,
    UnsupportedBinomialDeviceRadius {
        actual: u32,
        maximum: u32,
    },
    ZeroKernelWeight,
    KernelDevicePixelRatioMismatch,
    UnsupportedWorkingSpace,
    UnsupportedPrecision,
    UnsupportedQuantization,
    OutputCapabilityMismatch,
    ResourceExhausted,
    ArithmeticOverflow,
    InternalInvariant,
    WeakEvidenceCannotProveWholeField {
        class: FieldEvidenceClassV1,
    },
    UnknownRenderer {
        renderer: FieldRendererIdV1,
    },
    UnsupportedRenderer {
        renderer: FieldRendererIdV1,
        reason: FieldUnsupportedReasonIdV1,
    },
    ExactReferenceCannotProveHostRenderer,
    ProspectiveObservationRequiresHostConformantRenderer,
    EvidenceRequestDigestMismatch,
    EvidenceSceneRevisionMismatch {
        expected: FieldSceneRevisionV1,
        actual: FieldSceneRevisionV1,
    },
    EvidenceRenderCapabilityMismatch,
    ObservedRasterMismatch {
        pixel_index: usize,
    },
    CarrierAbsent,
    CarrierErased,
    CarrierVariationErased,
    IncrementalScratchUninitialised,
    IncrementalPreviousRequestMismatch,
    IncrementalChangeOutsideDirtyRegion {
        pixel_index: usize,
    },
    IncrementalLayoutMismatch,
}

/// Finite device-pixel extent of one raster field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FieldExtentV1 {
    width: u32,
    height: u32,
}

impl FieldExtentV1 {
    pub(crate) fn try_new(width: u32, height: u32) -> Result<Self, FieldEvaluationErrorV1> {
        if width == 0 || height == 0 {
            return Err(FieldEvaluationErrorV1::EmptyExtent);
        }
        let extent = Self { width, height };
        extent.pixel_count()?;
        Ok(extent)
    }

    pub(crate) const fn width(self) -> u32 {
        self.width
    }

    pub(crate) const fn height(self) -> u32 {
        self.height
    }

    fn pixel_count(self) -> Result<usize, FieldEvaluationErrorV1> {
        let count = u64::from(self.width)
            .checked_mul(u64::from(self.height))
            .ok_or(FieldEvaluationErrorV1::GeometryOverflow)?;
        usize::try_from(count).map_err(|_| FieldEvaluationErrorV1::GeometryOverflow)
    }
}

/// Non-empty rectangle already admitted inside one exact extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FieldRectV1 {
    extent: FieldExtentV1,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
}

impl FieldRectV1 {
    pub(crate) fn try_new(
        extent: FieldExtentV1,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<Self, FieldEvaluationErrorV1> {
        if width == 0 || height == 0 {
            return Err(FieldEvaluationErrorV1::EmptyRect);
        }
        let end_x = x
            .checked_add(width)
            .ok_or(FieldEvaluationErrorV1::GeometryOverflow)?;
        let end_y = y
            .checked_add(height)
            .ok_or(FieldEvaluationErrorV1::GeometryOverflow)?;
        if end_x > extent.width || end_y > extent.height {
            return Err(FieldEvaluationErrorV1::RectOutsideExtent);
        }
        Ok(Self {
            extent,
            x,
            y,
            width,
            height,
        })
    }

    pub(crate) const fn full(extent: FieldExtentV1) -> Self {
        Self {
            extent,
            x: 0,
            y: 0,
            width: extent.width,
            height: extent.height,
        }
    }

    pub(crate) fn expanded(
        self,
        radius: u32,
        extent: FieldExtentV1,
    ) -> Result<Self, FieldEvaluationErrorV1> {
        if self.extent != extent {
            return Err(FieldEvaluationErrorV1::ExtentMismatch);
        }
        let right = self
            .x
            .checked_add(self.width)
            .ok_or(FieldEvaluationErrorV1::GeometryOverflow)?;
        let bottom = self
            .y
            .checked_add(self.height)
            .ok_or(FieldEvaluationErrorV1::GeometryOverflow)?;
        // Overflow is an invalid geometry declaration even when clipping would
        // hide it. Silently replacing it with MAX would change the footprint.
        let expanded_right = right
            .checked_add(radius)
            .ok_or(FieldEvaluationErrorV1::GeometryOverflow)?;
        let expanded_bottom = bottom
            .checked_add(radius)
            .ok_or(FieldEvaluationErrorV1::GeometryOverflow)?;
        let left = self.x.saturating_sub(radius);
        let top = self.y.saturating_sub(radius);
        let clipped_right = expanded_right.min(extent.width);
        let clipped_bottom = expanded_bottom.min(extent.height);
        Self::try_new(
            extent,
            left,
            top,
            clipped_right - left,
            clipped_bottom - top,
        )
    }

    pub(crate) const fn extent(self) -> FieldExtentV1 {
        self.extent
    }

    pub(crate) const fn x(self) -> u32 {
        self.x
    }

    pub(crate) const fn y(self) -> u32 {
        self.y
    }

    pub(crate) const fn width(self) -> u32 {
        self.width
    }

    pub(crate) const fn height(self) -> u32 {
        self.height
    }

    fn end_x(self) -> Result<u32, FieldEvaluationErrorV1> {
        self.x
            .checked_add(self.width)
            .ok_or(FieldEvaluationErrorV1::GeometryOverflow)
    }

    fn end_y(self) -> Result<u32, FieldEvaluationErrorV1> {
        self.y
            .checked_add(self.height)
            .ok_or(FieldEvaluationErrorV1::GeometryOverflow)
    }
}

/// Exact output influence and an independently named conservative bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldInfluenceV1 {
    exact: FieldRectV1,
    conservative: FieldRectV1,
}

impl FieldInfluenceV1 {
    pub(crate) const fn new(exact: FieldRectV1, conservative: FieldRectV1) -> Self {
        Self {
            exact,
            conservative,
        }
    }

    pub(crate) const fn exact(self) -> FieldRectV1 {
        self.exact
    }

    pub(crate) const fn conservative(self) -> FieldRectV1 {
        self.conservative
    }
}

/// Input footprint required to compute one output rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldFootprintV1 {
    output: FieldRectV1,
    exact_input: FieldRectV1,
    conservative_input: FieldRectV1,
}

impl FieldFootprintV1 {
    pub(crate) const fn output(self) -> FieldRectV1 {
        self.output
    }

    pub(crate) const fn exact_input(self) -> FieldRectV1 {
        self.exact_input
    }

    pub(crate) const fn conservative_input(self) -> FieldRectV1 {
        self.conservative_input
    }
}

/// Exact encoded-sRGB8 premultiplied pixel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PremultipliedRgba8V1([u8; 4]);

impl PremultipliedRgba8V1 {
    pub(crate) const TRANSPARENT: Self = Self([0; 4]);

    pub(crate) fn try_new(channels: [u8; 4]) -> Result<Self, FieldEvaluationErrorV1> {
        let alpha = channels[3];
        if channels[..3].iter().any(|channel| *channel > alpha) {
            return Err(FieldEvaluationErrorV1::InvalidPremultipliedPixel { channels });
        }
        Ok(Self(channels))
    }

    pub(crate) const fn channels(self) -> [u8; 4] {
        self.0
    }

    pub(crate) const fn alpha(self) -> u8 {
        self.0[3]
    }
}

/// Canonical binary64 straight alpha retained without premultiplication.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FieldOpacityV1(u64);

impl FieldOpacityV1 {
    pub(crate) fn try_new(alpha: f64) -> Result<Self, FieldEvaluationErrorV1> {
        if !alpha.is_finite() {
            return Err(FieldEvaluationErrorV1::NonFiniteOpacity);
        }
        if !(0.0..=1.0).contains(&alpha) {
            return Err(FieldEvaluationErrorV1::OpacityOutsideUnitInterval);
        }
        let canonical = if alpha == 0.0 { 0.0 } else { alpha };
        Ok(Self(canonical.to_bits()))
    }

    pub(crate) const fn bits(self) -> u64 {
        self.0
    }

    pub(crate) const fn value(self) -> f64 {
        f64::from_bits(self.0)
    }
}

/// Straight encoded-sRGB8 tint and exact per-pixel alpha for screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct EncodedSrgb8AlphaV1 {
    tint: Srgb8,
    alpha: FieldOpacityV1,
}

impl EncodedSrgb8AlphaV1 {
    pub(crate) const fn new(tint: Srgb8, alpha: FieldOpacityV1) -> Self {
        Self { tint, alpha }
    }

    pub(crate) const fn tint(self) -> Srgb8 {
        self.tint
    }

    pub(crate) const fn alpha(self) -> FieldOpacityV1 {
        self.alpha
    }
}

/// Borrowed premultiplied raster with an opaque caller-owned identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldRasterViewV1<'a> {
    identity: FieldRasterIdentityV1,
    extent: FieldExtentV1,
    pixels: &'a [PremultipliedRgba8V1],
}

impl<'a> FieldRasterViewV1<'a> {
    pub(crate) fn try_new(
        identity: FieldRasterIdentityV1,
        extent: FieldExtentV1,
        pixels: &'a [PremultipliedRgba8V1],
    ) -> Result<Self, FieldEvaluationErrorV1> {
        let expected = extent.pixel_count()?;
        if pixels.len() != expected {
            return Err(FieldEvaluationErrorV1::RasterLengthMismatch {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            identity,
            extent,
            pixels,
        })
    }

    pub(crate) const fn identity(self) -> FieldRasterIdentityV1 {
        self.identity
    }

    pub(crate) const fn extent(self) -> FieldExtentV1 {
        self.extent
    }

    pub(crate) const fn pixels(self) -> &'a [PremultipliedRgba8V1] {
        self.pixels
    }
}

/// Borrowed opaque encoded-sRGB8 raster.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OpaqueSrgb8RasterViewV1<'a> {
    identity: FieldRasterIdentityV1,
    extent: FieldExtentV1,
    pixels: &'a [Srgb8],
}

impl<'a> OpaqueSrgb8RasterViewV1<'a> {
    pub(crate) fn try_new(
        identity: FieldRasterIdentityV1,
        extent: FieldExtentV1,
        pixels: &'a [Srgb8],
    ) -> Result<Self, FieldEvaluationErrorV1> {
        let expected = extent.pixel_count()?;
        if pixels.len() != expected {
            return Err(FieldEvaluationErrorV1::RasterLengthMismatch {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            identity,
            extent,
            pixels,
        })
    }

    pub(crate) const fn identity(self) -> FieldRasterIdentityV1 {
        self.identity
    }

    pub(crate) const fn extent(self) -> FieldExtentV1 {
        self.extent
    }

    pub(crate) const fn pixels(self) -> &'a [Srgb8] {
        self.pixels
    }
}

/// Borrowed straight-tint/alpha raster used only by encoded-sRGB8 screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct EncodedSrgb8AlphaRasterViewV1<'a> {
    identity: FieldRasterIdentityV1,
    extent: FieldExtentV1,
    pixels: &'a [EncodedSrgb8AlphaV1],
}

impl<'a> EncodedSrgb8AlphaRasterViewV1<'a> {
    pub(crate) fn try_new(
        identity: FieldRasterIdentityV1,
        extent: FieldExtentV1,
        pixels: &'a [EncodedSrgb8AlphaV1],
    ) -> Result<Self, FieldEvaluationErrorV1> {
        let expected = extent.pixel_count()?;
        if pixels.len() != expected {
            return Err(FieldEvaluationErrorV1::RasterLengthMismatch {
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            identity,
            extent,
            pixels,
        })
    }

    pub(crate) const fn identity(self) -> FieldRasterIdentityV1 {
        self.identity
    }

    pub(crate) const fn extent(self) -> FieldExtentV1 {
        self.extent
    }

    pub(crate) const fn pixels(self) -> &'a [EncodedSrgb8AlphaV1] {
        self.pixels
    }
}

/// Supported integer device-pixel ratios for the v1 deterministic kernel set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum DevicePixelRatioV1 {
    One,
    Two,
    Three,
    Four,
}

impl DevicePixelRatioV1 {
    pub(crate) fn try_new(actual: u8) -> Result<Self, FieldEvaluationErrorV1> {
        match actual {
            1 => Ok(Self::One),
            2 => Ok(Self::Two),
            3 => Ok(Self::Three),
            4 => Ok(Self::Four),
            _ => Err(FieldEvaluationErrorV1::UnsupportedDevicePixelRatio { actual }),
        }
    }

    pub(crate) const fn value(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
            Self::Four => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GaussianKernelProfileV1 {
    /// Symmetric binomial discretisation with exact Q32 normalization.
    BinomialGaussianQ32V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum GaussianEdgeModeV1 {
    ClampToEdgeV1,
}

/// Validated finite separable Gaussian kernel owned by the compiled plan.
/// `css_radius_px` is the finite support radius, not a Gaussian sigma. The
/// versioned binomial law uses Pascal row `2 * css_radius_px * DPR` exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GaussianKernelV1 {
    profile: GaussianKernelProfileV1,
    css_radius_px: u32,
    device_pixel_ratio: DevicePixelRatioV1,
    device_radius_px: u32,
    weights_q32: Vec<u32>,
}

impl GaussianKernelV1 {
    pub(crate) fn try_new(
        profile: GaussianKernelProfileV1,
        css_radius_px: u32,
        device_pixel_ratio: DevicePixelRatioV1,
        weights_q32: Vec<u32>,
    ) -> Result<Self, FieldEvaluationErrorV1> {
        if css_radius_px == 0 {
            return Err(FieldEvaluationErrorV1::InvalidKernelShape);
        }
        let device_radius_px = css_radius_px
            .checked_mul(u32::from(device_pixel_ratio.value()))
            .ok_or(FieldEvaluationErrorV1::GeometryOverflow)?;
        if profile == GaussianKernelProfileV1::BinomialGaussianQ32V1
            && device_radius_px > MAX_EXACT_BINOMIAL_DEVICE_RADIUS_PX
        {
            return Err(FieldEvaluationErrorV1::UnsupportedBinomialDeviceRadius {
                actual: device_radius_px,
                maximum: MAX_EXACT_BINOMIAL_DEVICE_RADIUS_PX,
            });
        }
        let expected_u32 = device_radius_px
            .checked_mul(2)
            .and_then(|diameter| diameter.checked_add(1))
            .ok_or(FieldEvaluationErrorV1::GeometryOverflow)?;
        let expected =
            usize::try_from(expected_u32).map_err(|_| FieldEvaluationErrorV1::GeometryOverflow)?;
        if weights_q32.len() != expected {
            return Err(FieldEvaluationErrorV1::InvalidKernelShape);
        }
        if weights_q32.contains(&0) {
            return Err(FieldEvaluationErrorV1::ZeroKernelWeight);
        }
        if !weights_q32
            .iter()
            .zip(weights_q32.iter().rev())
            .all(|(left, right)| left == right)
        {
            return Err(FieldEvaluationErrorV1::KernelWeightsNotSymmetric);
        }
        let sum = weights_q32.iter().try_fold(0_u64, |sum, weight| {
            sum.checked_add(u64::from(*weight))
                .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)
        })?;
        if sum != Q32_NORMALIZATION {
            return Err(FieldEvaluationErrorV1::KernelWeightsNotNormalized);
        }
        match profile {
            GaussianKernelProfileV1::BinomialGaussianQ32V1 => {
                validate_exact_binomial_q32(device_radius_px, &weights_q32)?;
            }
        }
        Ok(Self {
            profile,
            css_radius_px,
            device_pixel_ratio,
            device_radius_px,
            weights_q32,
        })
    }

    /// Canonical one-CSS-pixel kernel. Coefficients are exact rows of the
    /// binomial distribution and therefore sum to exactly 2^32.
    pub(crate) fn canonical_one_css_pixel(device_pixel_ratio: DevicePixelRatioV1) -> Self {
        let weights_q32: &[u32] = match device_pixel_ratio {
            DevicePixelRatioV1::One => &[1_073_741_824, 2_147_483_648, 1_073_741_824],
            DevicePixelRatioV1::Two => &[
                268_435_456,
                1_073_741_824,
                1_610_612_736,
                1_073_741_824,
                268_435_456,
            ],
            DevicePixelRatioV1::Three => &[
                67_108_864,
                402_653_184,
                1_006_632_960,
                1_342_177_280,
                1_006_632_960,
                402_653_184,
                67_108_864,
            ],
            DevicePixelRatioV1::Four => &[
                16_777_216,
                134_217_728,
                469_762_048,
                939_524_096,
                1_174_405_120,
                939_524_096,
                469_762_048,
                134_217_728,
                16_777_216,
            ],
        };
        Self {
            profile: GaussianKernelProfileV1::BinomialGaussianQ32V1,
            css_radius_px: 1,
            device_pixel_ratio,
            device_radius_px: u32::from(device_pixel_ratio.value()),
            weights_q32: weights_q32.to_vec(),
        }
    }

    pub(crate) const fn profile(&self) -> GaussianKernelProfileV1 {
        self.profile
    }

    pub(crate) const fn css_radius_px(&self) -> u32 {
        self.css_radius_px
    }

    pub(crate) const fn device_pixel_ratio(&self) -> DevicePixelRatioV1 {
        self.device_pixel_ratio
    }

    pub(crate) const fn device_radius_px(&self) -> u32 {
        self.device_radius_px
    }

    pub(crate) fn weights_q32(&self) -> &[u32] {
        &self.weights_q32
    }
}

fn validate_exact_binomial_q32(
    device_radius_px: u32,
    weights_q32: &[u32],
) -> Result<(), FieldEvaluationErrorV1> {
    let order = device_radius_px
        .checked_mul(2)
        .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)?;
    let scale = 1_u64
        .checked_shl(32 - order)
        .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)?;
    let mut coefficient = 1_u64;
    for (index, actual) in weights_q32.iter().copied().enumerate() {
        let expected = coefficient
            .checked_mul(scale)
            .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)?;
        if u64::from(actual) != expected {
            return Err(FieldEvaluationErrorV1::KernelWeightsDoNotMatchProfile);
        }
        let index = u32::try_from(index).map_err(|_| FieldEvaluationErrorV1::ArithmeticOverflow)?;
        if index < order {
            coefficient = coefficient
                .checked_mul(u64::from(order - index))
                .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)?
                / u64::from(index + 1);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FieldWorkingSpaceV1 {
    EncodedSrgb8PremultipliedV1,
    LinearSrgbQ31PremultipliedV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FieldPrecisionV1 {
    FixedQ32V1,
    Binary32V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FieldQuantizationV1 {
    RoundHalfUpSrgb8V1,
    RoundTiesToEvenSrgb8V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FieldOutputCapabilityV1 {
    PremultipliedRgba8V1,
    OpaqueSrgb8V1,
}

/// Opaque proof that a renderer/conformance pair crossed host admission.
/// Production minting is intentionally absent until the attachment-owned
/// admission token is wired into this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FieldHostConformancePermitV1 {
    renderer: FieldRendererIdV1,
    conformance: FieldHostConformanceIdV1,
}

impl FieldHostConformancePermitV1 {
    #[cfg(test)]
    pub(crate) const fn mint_for_test(
        renderer: FieldRendererIdV1,
        conformance: FieldHostConformanceIdV1,
    ) -> Self {
        Self {
            renderer,
            conformance,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FieldRendererCapabilityKindV1 {
    ExactReference {
        renderer: FieldRendererIdV1,
    },
    HostConformant {
        renderer: FieldRendererIdV1,
        conformance: FieldHostConformanceIdV1,
    },
    Unknown {
        renderer: FieldRendererIdV1,
    },
    Unsupported {
        renderer: FieldRendererIdV1,
        reason: FieldUnsupportedReasonIdV1,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FieldRendererCapabilityV1 {
    kind: FieldRendererCapabilityKindV1,
}

impl FieldRendererCapabilityV1 {
    pub(crate) const fn exact_reference(renderer: FieldRendererIdV1) -> Self {
        Self {
            kind: FieldRendererCapabilityKindV1::ExactReference { renderer },
        }
    }

    pub(crate) const fn host_conformant(permit: FieldHostConformancePermitV1) -> Self {
        Self {
            kind: FieldRendererCapabilityKindV1::HostConformant {
                renderer: permit.renderer,
                conformance: permit.conformance,
            },
        }
    }

    pub(crate) const fn unknown(renderer: FieldRendererIdV1) -> Self {
        Self {
            kind: FieldRendererCapabilityKindV1::Unknown { renderer },
        }
    }

    pub(crate) const fn unsupported(
        renderer: FieldRendererIdV1,
        reason: FieldUnsupportedReasonIdV1,
    ) -> Self {
        Self {
            kind: FieldRendererCapabilityKindV1::Unsupported { renderer, reason },
        }
    }

    const fn kind(self) -> FieldRendererCapabilityKindV1 {
        self.kind
    }

    const fn is_exact_reference(self) -> bool {
        matches!(
            self.kind,
            FieldRendererCapabilityKindV1::ExactReference { .. }
        )
    }

    const fn is_host_conformant(self) -> bool {
        matches!(
            self.kind,
            FieldRendererCapabilityKindV1::HostConformant { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FieldRenderCapabilityV1 {
    renderer: FieldRendererCapabilityV1,
    output: FieldOutputCapabilityV1,
}

impl FieldRenderCapabilityV1 {
    pub(crate) const fn new(
        renderer: FieldRendererCapabilityV1,
        output: FieldOutputCapabilityV1,
    ) -> Self {
        Self { renderer, output }
    }

    pub(crate) const fn renderer(self) -> FieldRendererCapabilityV1 {
        self.renderer
    }

    pub(crate) const fn output(self) -> FieldOutputCapabilityV1 {
        self.output
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldSceneRevisionV1 {
    stream: ObservationStreamId,
    revision: Revision,
}

impl FieldSceneRevisionV1 {
    /// Derives the field scene only from the exact observation authority minted
    /// by Session for the currently admitted head.
    pub(crate) const fn from_session_permit(permit: &SessionObservationBindingPermitV1) -> Self {
        Self {
            stream: permit.stream(),
            revision: permit.revision(),
        }
    }

    #[cfg(test)]
    pub(crate) const fn mint_for_test(stream: ObservationStreamId, revision: Revision) -> Self {
        Self { stream, revision }
    }

    pub(crate) const fn stream(self) -> ObservationStreamId {
        self.stream
    }

    pub(crate) const fn revision(self) -> Revision {
        self.revision
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct FieldGeometryV1 {
    extent: FieldExtentV1,
}

impl FieldGeometryV1 {
    pub(crate) const fn new(extent: FieldExtentV1) -> Self {
        Self { extent }
    }

    pub(crate) const fn extent(self) -> FieldExtentV1 {
        self.extent
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CarrierIntentV1 {
    Present,
    Contributes,
    SpatialVariation,
}

/// Separate stable identities for the four V7 field laws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum FieldOperatorKindV1 {
    GaussianBlurSeparableQ32V1,
    PremultipliedSourceOverV1,
    EncodedSrgb8ScreenOpaqueBackdropV1,
    PorterDuffLighterV1,
}

/// One typed operator invocation over complete borrowed input rasters.
#[derive(Debug)]
pub(crate) enum FieldOperationV1<'a> {
    GaussianBlur {
        source: FieldRasterViewV1<'a>,
        kernel: GaussianKernelV1,
        edge_mode: GaussianEdgeModeV1,
    },
    PremultipliedSourceOver {
        source: FieldRasterViewV1<'a>,
        destination: FieldRasterViewV1<'a>,
    },
    EncodedSrgb8ScreenOpaqueBackdrop {
        source: EncodedSrgb8AlphaRasterViewV1<'a>,
        backdrop: OpaqueSrgb8RasterViewV1<'a>,
    },
    PorterDuffLighter {
        source: FieldRasterViewV1<'a>,
        destination: FieldRasterViewV1<'a>,
    },
}

impl FieldOperationV1<'_> {
    pub(crate) const fn kind(&self) -> FieldOperatorKindV1 {
        match self {
            Self::GaussianBlur { .. } => FieldOperatorKindV1::GaussianBlurSeparableQ32V1,
            Self::PremultipliedSourceOver { .. } => FieldOperatorKindV1::PremultipliedSourceOverV1,
            Self::EncodedSrgb8ScreenOpaqueBackdrop { .. } => {
                FieldOperatorKindV1::EncodedSrgb8ScreenOpaqueBackdropV1
            }
            Self::PorterDuffLighter { .. } => FieldOperatorKindV1::PorterDuffLighterV1,
        }
    }

    const fn extent(&self) -> FieldExtentV1 {
        match self {
            Self::GaussianBlur { source, .. }
            | Self::PremultipliedSourceOver { source, .. }
            | Self::PorterDuffLighter { source, .. } => source.extent(),
            Self::EncodedSrgb8ScreenOpaqueBackdrop { source, .. } => source.extent(),
        }
    }

    const fn output_capability(&self) -> FieldOutputCapabilityV1 {
        match self {
            Self::EncodedSrgb8ScreenOpaqueBackdrop { .. } => FieldOutputCapabilityV1::OpaqueSrgb8V1,
            Self::GaussianBlur { .. }
            | Self::PremultipliedSourceOver { .. }
            | Self::PorterDuffLighter { .. } => FieldOutputCapabilityV1::PremultipliedRgba8V1,
        }
    }

    const fn radius(&self) -> u32 {
        match self {
            Self::GaussianBlur { kernel, .. } => kernel.device_radius_px(),
            Self::PremultipliedSourceOver { .. }
            | Self::EncodedSrgb8ScreenOpaqueBackdrop { .. }
            | Self::PorterDuffLighter { .. } => 0,
        }
    }
}

/// Fully admitted reference request. Input raster bytes remain caller-owned.
#[derive(Debug)]
pub(crate) struct FieldEvaluationRequestV1<'a> {
    request_id: FieldRequestIdV1,
    operator_instance: FieldOperatorInstanceIdV1,
    geometry: FieldGeometryV1,
    device_pixel_ratio: DevicePixelRatioV1,
    working_space: FieldWorkingSpaceV1,
    precision: FieldPrecisionV1,
    quantization: FieldQuantizationV1,
    render_capability: FieldRenderCapabilityV1,
    scene_revision: FieldSceneRevisionV1,
    carrier_intent: CarrierIntentV1,
    operation: FieldOperationV1<'a>,
}

impl<'a> FieldEvaluationRequestV1<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn try_new(
        request_id: FieldRequestIdV1,
        operator_instance: FieldOperatorInstanceIdV1,
        geometry: FieldGeometryV1,
        device_pixel_ratio: DevicePixelRatioV1,
        working_space: FieldWorkingSpaceV1,
        precision: FieldPrecisionV1,
        quantization: FieldQuantizationV1,
        render_capability: FieldRenderCapabilityV1,
        scene_revision: FieldSceneRevisionV1,
        carrier_intent: CarrierIntentV1,
        operation: FieldOperationV1<'a>,
    ) -> Result<Self, FieldEvaluationErrorV1> {
        if operation.extent() != geometry.extent() {
            return Err(FieldEvaluationErrorV1::ExtentMismatch);
        }
        match &operation {
            FieldOperationV1::GaussianBlur { kernel, .. } => {
                if kernel.device_pixel_ratio() != device_pixel_ratio {
                    return Err(FieldEvaluationErrorV1::KernelDevicePixelRatioMismatch);
                }
            }
            FieldOperationV1::PremultipliedSourceOver { destination, .. }
            | FieldOperationV1::PorterDuffLighter { destination, .. } => {
                if destination.extent() != geometry.extent() {
                    return Err(FieldEvaluationErrorV1::ExtentMismatch);
                }
            }
            FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop { backdrop, .. } => {
                if backdrop.extent() != geometry.extent() {
                    return Err(FieldEvaluationErrorV1::ExtentMismatch);
                }
            }
        }
        if working_space != FieldWorkingSpaceV1::EncodedSrgb8PremultipliedV1 {
            return Err(FieldEvaluationErrorV1::UnsupportedWorkingSpace);
        }
        if precision != FieldPrecisionV1::FixedQ32V1 {
            return Err(FieldEvaluationErrorV1::UnsupportedPrecision);
        }
        if quantization != FieldQuantizationV1::RoundHalfUpSrgb8V1 {
            return Err(FieldEvaluationErrorV1::UnsupportedQuantization);
        }
        if operation.output_capability() != render_capability.output() {
            return Err(FieldEvaluationErrorV1::OutputCapabilityMismatch);
        }
        Ok(Self {
            request_id,
            operator_instance,
            geometry,
            device_pixel_ratio,
            working_space,
            precision,
            quantization,
            render_capability,
            scene_revision,
            carrier_intent,
            operation,
        })
    }

    pub(crate) const fn operator_kind(&self) -> FieldOperatorKindV1 {
        self.operation.kind()
    }

    pub(crate) const fn geometry(&self) -> FieldGeometryV1 {
        self.geometry
    }

    pub(crate) const fn render_capability(&self) -> FieldRenderCapabilityV1 {
        self.render_capability
    }

    pub(crate) const fn scene_revision(&self) -> FieldSceneRevisionV1 {
        self.scene_revision
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldEvidenceClassV1 {
    PointSample,
    FieldSamples,
    FieldAverage,
    GradientStops,
    CommittedRaster,
    ExactReferenceWholeRaster,
    ProspectiveObservedWholeRaster,
}

/// Complete prospective raster observed for the exact pending scene revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProspectiveObservedRasterV1<'a> {
    identity: FieldEvidenceIdentityV1,
    request_digest: FieldRequestDigestV1,
    scene_revision: FieldSceneRevisionV1,
    render_capability: FieldRenderCapabilityV1,
    raster: FieldRasterViewV1<'a>,
}

impl<'a> ProspectiveObservedRasterV1<'a> {
    pub(crate) const fn from_host_observation(
        identity: FieldEvidenceIdentityV1,
        request_digest: FieldRequestDigestV1,
        scene_revision: FieldSceneRevisionV1,
        output: FieldOutputCapabilityV1,
        permit: FieldHostConformancePermitV1,
        raster: FieldRasterViewV1<'a>,
    ) -> Self {
        Self {
            identity,
            request_digest,
            scene_revision,
            render_capability: FieldRenderCapabilityV1::new(
                FieldRendererCapabilityV1::host_conformant(permit),
                output,
            ),
            raster,
        }
    }
}

/// Raw evidence classes. Only the two whole-raster variants can mint proof.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldEvidenceV1<'a> {
    PointSample {
        identity: FieldEvidenceIdentityV1,
    },
    FieldSamples {
        identity: FieldEvidenceIdentityV1,
        sample_count: usize,
    },
    FieldAverage {
        identity: FieldEvidenceIdentityV1,
    },
    GradientStops {
        identity: FieldEvidenceIdentityV1,
        stop_count: usize,
    },
    CommittedRaster {
        identity: FieldEvidenceIdentityV1,
        raster: FieldRasterViewV1<'a>,
    },
    ExactReferenceWholeRaster {
        identity: FieldEvidenceIdentityV1,
    },
    ProspectiveObservedWholeRaster(ProspectiveObservedRasterV1<'a>),
}

impl FieldEvidenceV1<'_> {
    pub(crate) const fn class(self) -> FieldEvidenceClassV1 {
        match self {
            Self::PointSample { .. } => FieldEvidenceClassV1::PointSample,
            Self::FieldSamples { .. } => FieldEvidenceClassV1::FieldSamples,
            Self::FieldAverage { .. } => FieldEvidenceClassV1::FieldAverage,
            Self::GradientStops { .. } => FieldEvidenceClassV1::GradientStops,
            Self::CommittedRaster { .. } => FieldEvidenceClassV1::CommittedRaster,
            Self::ExactReferenceWholeRaster { .. } => {
                FieldEvidenceClassV1::ExactReferenceWholeRaster
            }
            Self::ProspectiveObservedWholeRaster(_) => {
                FieldEvidenceClassV1::ProspectiveObservedWholeRaster
            }
        }
    }

    const fn identity(self) -> FieldEvidenceIdentityV1 {
        match self {
            Self::PointSample { identity }
            | Self::FieldSamples { identity, .. }
            | Self::FieldAverage { identity }
            | Self::GradientStops { identity, .. }
            | Self::CommittedRaster { identity, .. }
            | Self::ExactReferenceWholeRaster { identity } => identity,
            Self::ProspectiveObservedWholeRaster(observed) => observed.identity,
        }
    }
}

/// Reusable buffers owned by the caller/compiled plan, never by a certificate.
#[derive(Debug, Default)]
pub(crate) struct FieldEvaluationScratchV1 {
    output: Vec<PremultipliedRgba8V1>,
    horizontal_q32: Vec<[u64; 4]>,
    extent: Option<FieldExtentV1>,
    layout_digest: Option<FieldEvaluationLayoutDigestV1>,
    last_request_digest: Option<FieldRequestDigestV1>,
}

impl FieldEvaluationScratchV1 {
    pub(crate) const fn new() -> Self {
        Self {
            output: Vec::new(),
            horizontal_q32: Vec::new(),
            extent: None,
            layout_digest: None,
            last_request_digest: None,
        }
    }

    pub(crate) fn output(&self) -> &[PremultipliedRgba8V1] {
        &self.output
    }

    #[cfg(test)]
    pub(crate) fn capacity_snapshot_for_test(&self) -> (usize, usize) {
        (self.output.capacity(), self.horizontal_q32.capacity())
    }

    #[cfg(test)]
    pub(crate) fn pointer_snapshot_for_test(
        &self,
    ) -> (*const PremultipliedRgba8V1, *const [u64; 4]) {
        (self.output.as_ptr(), self.horizontal_q32.as_ptr())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldWholeRasterCertificateV1 {
    request_digest: FieldRequestDigestV1,
    output_digest: FieldRasterDigestV1,
    kernel_digest: FieldKernelDigestV1,
    evidence_identity: FieldEvidenceIdentityV1,
    evidence_class: FieldEvidenceClassV1,
    operator_kind: FieldOperatorKindV1,
    scene_revision: FieldSceneRevisionV1,
    render_capability: FieldRenderCapabilityV1,
    digest: FieldCertificateDigestV1,
}

impl FieldWholeRasterCertificateV1 {
    pub(crate) const fn request_digest(self) -> FieldRequestDigestV1 {
        self.request_digest
    }

    pub(crate) const fn output_digest(self) -> FieldRasterDigestV1 {
        self.output_digest
    }

    pub(crate) const fn kernel_digest(self) -> FieldKernelDigestV1 {
        self.kernel_digest
    }

    pub(crate) const fn evidence_class(self) -> FieldEvidenceClassV1 {
        self.evidence_class
    }

    pub(crate) const fn operator_kind(self) -> FieldOperatorKindV1 {
        self.operator_kind
    }

    pub(crate) const fn scene_revision(self) -> FieldSceneRevisionV1 {
        self.scene_revision
    }

    pub(crate) const fn render_capability(self) -> FieldRenderCapabilityV1 {
        self.render_capability
    }

    pub(crate) const fn digest(self) -> FieldCertificateDigestV1 {
        self.digest
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FieldCertificateReplayErrorV1 {
    SceneRevision {
        expected: FieldSceneRevisionV1,
        actual: FieldSceneRevisionV1,
    },
    RenderCapability,
    Request,
}

pub(crate) fn footprint_for_output(
    request: &FieldEvaluationRequestV1<'_>,
    output: FieldRectV1,
) -> Result<FieldFootprintV1, FieldEvaluationErrorV1> {
    if output.extent() != request.geometry.extent() {
        return Err(FieldEvaluationErrorV1::ExtentMismatch);
    }
    let exact_input = output.expanded(request.operation.radius(), request.geometry.extent())?;
    Ok(FieldFootprintV1 {
        output,
        exact_input,
        conservative_input: exact_input,
    })
}

pub(crate) fn influence_for_input(
    request: &FieldEvaluationRequestV1<'_>,
    dirty_input: FieldRectV1,
) -> Result<FieldInfluenceV1, FieldEvaluationErrorV1> {
    if dirty_input.extent() != request.geometry.extent() {
        return Err(FieldEvaluationErrorV1::ExtentMismatch);
    }
    let exact = dirty_input.expanded(request.operation.radius(), request.geometry.extent())?;
    Ok(FieldInfluenceV1::new(exact, exact))
}

pub(crate) fn evaluate_reference_full<'scratch>(
    request: &FieldEvaluationRequestV1<'_>,
    scratch: &'scratch mut FieldEvaluationScratchV1,
) -> Result<&'scratch [PremultipliedRgba8V1], FieldEvaluationErrorV1> {
    let extent = request.geometry.extent();
    prepare_scratch(scratch, extent, evaluation_layout_digest(request))?;
    scratch.last_request_digest = None;
    scratch.output.fill(PremultipliedRgba8V1::TRANSPARENT);
    evaluate_region(request, FieldRectV1::full(extent), scratch)?;
    scratch.last_request_digest = Some(request_digest(request));
    Ok(&scratch.output)
}

pub(crate) fn evaluate_reference_incremental(
    previous_request: &FieldEvaluationRequestV1<'_>,
    request: &FieldEvaluationRequestV1<'_>,
    dirty_input: FieldRectV1,
    scratch: &mut FieldEvaluationScratchV1,
) -> Result<FieldInfluenceV1, FieldEvaluationErrorV1> {
    let extent = request.geometry.extent();
    if scratch.extent.is_none()
        || scratch.layout_digest.is_none()
        || scratch.last_request_digest.is_none()
    {
        return Err(FieldEvaluationErrorV1::IncrementalScratchUninitialised);
    }
    if scratch.last_request_digest != Some(request_digest(previous_request)) {
        return Err(FieldEvaluationErrorV1::IncrementalPreviousRequestMismatch);
    }
    let previous_layout = evaluation_layout_digest(previous_request);
    let next_layout = evaluation_layout_digest(request);
    if scratch.extent != Some(extent)
        || previous_request.geometry.extent() != extent
        || scratch.layout_digest != Some(previous_layout)
        || previous_layout != next_layout
        || scratch.output.len() != extent.pixel_count()?
    {
        return Err(FieldEvaluationErrorV1::IncrementalLayoutMismatch);
    }
    verify_incremental_change_scope(previous_request, request, dirty_input)?;
    let influence = influence_for_input(request, dirty_input)?;
    scratch.last_request_digest = None;
    evaluate_region(request, influence.exact(), scratch)?;
    scratch.layout_digest = Some(next_layout);
    scratch.last_request_digest = Some(request_digest(request));
    Ok(influence)
}

fn verify_incremental_change_scope(
    previous: &FieldEvaluationRequestV1<'_>,
    current: &FieldEvaluationRequestV1<'_>,
    dirty: FieldRectV1,
) -> Result<(), FieldEvaluationErrorV1> {
    let extent = current.geometry.extent();
    if dirty.extent() != extent || previous.geometry.extent() != extent {
        return Err(FieldEvaluationErrorV1::ExtentMismatch);
    }
    let end_x = dirty.end_x()?;
    let end_y = dirty.end_y()?;
    for y in 0..extent.height() {
        for x in 0..extent.width() {
            if x >= dirty.x() && x < end_x && y >= dirty.y() && y < end_y {
                continue;
            }
            let pixel_index = field_index(extent, x, y)?;
            if operation_input_changed_outside_dirty(
                &previous.operation,
                &current.operation,
                pixel_index,
            )? {
                return Err(
                    FieldEvaluationErrorV1::IncrementalChangeOutsideDirtyRegion { pixel_index },
                );
            }
        }
    }
    Ok(())
}

fn operation_input_changed_outside_dirty(
    previous: &FieldOperationV1<'_>,
    current: &FieldOperationV1<'_>,
    pixel_index: usize,
) -> Result<bool, FieldEvaluationErrorV1> {
    let changed = match (previous, current) {
        (
            FieldOperationV1::GaussianBlur {
                source: previous_source,
                ..
            },
            FieldOperationV1::GaussianBlur {
                source: current_source,
                ..
            },
        ) => previous_source.pixels().get(pixel_index) != current_source.pixels().get(pixel_index),
        (
            FieldOperationV1::PremultipliedSourceOver {
                source: previous_source,
                destination: previous_destination,
            },
            FieldOperationV1::PremultipliedSourceOver {
                source: current_source,
                destination: current_destination,
            },
        )
        | (
            FieldOperationV1::PorterDuffLighter {
                source: previous_source,
                destination: previous_destination,
            },
            FieldOperationV1::PorterDuffLighter {
                source: current_source,
                destination: current_destination,
            },
        ) => {
            previous_source.pixels().get(pixel_index) != current_source.pixels().get(pixel_index)
                || previous_destination.pixels().get(pixel_index)
                    != current_destination.pixels().get(pixel_index)
        }
        (
            FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop {
                source: previous_source,
                backdrop: previous_backdrop,
            },
            FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop {
                source: current_source,
                backdrop: current_backdrop,
            },
        ) => {
            previous_source.pixels().get(pixel_index) != current_source.pixels().get(pixel_index)
                || previous_backdrop.pixels().get(pixel_index)
                    != current_backdrop.pixels().get(pixel_index)
        }
        _ => return Err(FieldEvaluationErrorV1::IncrementalLayoutMismatch),
    };
    Ok(changed)
}

pub(crate) fn evaluate_whole_field(
    request: &FieldEvaluationRequestV1<'_>,
    evidence: FieldEvidenceV1<'_>,
    scratch: &mut FieldEvaluationScratchV1,
) -> Result<FieldWholeRasterCertificateV1, FieldEvaluationErrorV1> {
    admit_proof_evidence(request, evidence)?;
    evaluate_reference_full(request, scratch)?;
    enforce_carrier_intent(request, &scratch.output)?;

    if let FieldEvidenceV1::ProspectiveObservedWholeRaster(observed) = evidence {
        let mismatch = scratch
            .output
            .iter()
            .zip(observed.raster.pixels())
            .position(|(expected, actual)| expected != actual);
        if let Some(pixel_index) = mismatch {
            return Err(FieldEvaluationErrorV1::ObservedRasterMismatch { pixel_index });
        }
    }

    let request_digest = request_digest(request);
    let output_digest = raster_digest(request.geometry.extent(), &scratch.output);
    let kernel_digest = kernel_digest(&request.operation);
    let evidence_identity = evidence.identity();
    let evidence_class = evidence.class();
    let digest = certificate_digest(
        request_digest,
        output_digest,
        kernel_digest,
        evidence_identity,
        evidence_class,
        request,
        evidence,
    );
    Ok(FieldWholeRasterCertificateV1 {
        request_digest,
        output_digest,
        kernel_digest,
        evidence_identity,
        evidence_class,
        operator_kind: request.operator_kind(),
        scene_revision: request.scene_revision,
        render_capability: request.render_capability,
        digest,
    })
}

pub(crate) fn verify_certificate_replay(
    certificate: &FieldWholeRasterCertificateV1,
    request: &FieldEvaluationRequestV1<'_>,
) -> Result<(), FieldCertificateReplayErrorV1> {
    if certificate.scene_revision != request.scene_revision {
        return Err(FieldCertificateReplayErrorV1::SceneRevision {
            expected: certificate.scene_revision,
            actual: request.scene_revision,
        });
    }
    if certificate.render_capability != request.render_capability {
        return Err(FieldCertificateReplayErrorV1::RenderCapability);
    }
    if certificate.request_digest != request_digest(request) {
        return Err(FieldCertificateReplayErrorV1::Request);
    }
    Ok(())
}

pub(crate) fn request_digest(request: &FieldEvaluationRequestV1<'_>) -> FieldRequestDigestV1 {
    let mut hasher = Hasher::new();
    hasher.update(b"labcolors-field-request-v1\0");
    hash_u64(&mut hasher, request.request_id.value());
    hash_u64(&mut hasher, request.operator_instance.value());
    hash_geometry(&mut hasher, request.geometry);
    hash_u8(&mut hasher, request.device_pixel_ratio.value());
    hash_working_space(&mut hasher, request.working_space);
    hash_precision(&mut hasher, request.precision);
    hash_quantization(&mut hasher, request.quantization);
    hash_render_capability(&mut hasher, request.render_capability);
    hash_scene_revision(&mut hasher, request.scene_revision);
    hash_carrier_intent(&mut hasher, request.carrier_intent);
    hash_operation(&mut hasher, &request.operation);
    FieldRequestDigestV1(finalize(hasher))
}

fn admit_proof_evidence(
    request: &FieldEvaluationRequestV1<'_>,
    evidence: FieldEvidenceV1<'_>,
) -> Result<(), FieldEvaluationErrorV1> {
    match evidence {
        FieldEvidenceV1::PointSample { .. }
        | FieldEvidenceV1::FieldSamples { .. }
        | FieldEvidenceV1::FieldAverage { .. }
        | FieldEvidenceV1::GradientStops { .. }
        | FieldEvidenceV1::CommittedRaster { .. } => {
            return Err(FieldEvaluationErrorV1::WeakEvidenceCannotProveWholeField {
                class: evidence.class(),
            });
        }
        FieldEvidenceV1::ExactReferenceWholeRaster { .. } => {
            admit_renderer(request.render_capability.renderer())?;
            if !request.render_capability.renderer().is_exact_reference() {
                return Err(FieldEvaluationErrorV1::ExactReferenceCannotProveHostRenderer);
            }
        }
        FieldEvidenceV1::ProspectiveObservedWholeRaster(observed) => {
            admit_renderer(request.render_capability.renderer())?;
            if !request.render_capability.renderer().is_host_conformant() {
                return Err(
                    FieldEvaluationErrorV1::ProspectiveObservationRequiresHostConformantRenderer,
                );
            }
            if observed.request_digest != request_digest(request) {
                return Err(FieldEvaluationErrorV1::EvidenceRequestDigestMismatch);
            }
            if observed.scene_revision != request.scene_revision {
                return Err(FieldEvaluationErrorV1::EvidenceSceneRevisionMismatch {
                    expected: request.scene_revision,
                    actual: observed.scene_revision,
                });
            }
            if observed.render_capability != request.render_capability {
                return Err(FieldEvaluationErrorV1::EvidenceRenderCapabilityMismatch);
            }
            if observed.raster.extent() != request.geometry.extent() {
                return Err(FieldEvaluationErrorV1::ExtentMismatch);
            }
        }
    }
    Ok(())
}

fn admit_renderer(renderer: FieldRendererCapabilityV1) -> Result<(), FieldEvaluationErrorV1> {
    match renderer.kind() {
        FieldRendererCapabilityKindV1::ExactReference { .. }
        | FieldRendererCapabilityKindV1::HostConformant { .. } => Ok(()),
        FieldRendererCapabilityKindV1::Unknown { renderer } => {
            Err(FieldEvaluationErrorV1::UnknownRenderer { renderer })
        }
        FieldRendererCapabilityKindV1::Unsupported { renderer, reason } => {
            Err(FieldEvaluationErrorV1::UnsupportedRenderer { renderer, reason })
        }
    }
}

fn prepare_scratch(
    scratch: &mut FieldEvaluationScratchV1,
    extent: FieldExtentV1,
    layout_digest: FieldEvaluationLayoutDigestV1,
) -> Result<(), FieldEvaluationErrorV1> {
    let required = extent.pixel_count()?;
    try_reserve_total(&mut scratch.output, required)?;
    try_reserve_total(&mut scratch.horizontal_q32, required)?;
    scratch
        .output
        .resize(required, PremultipliedRgba8V1::TRANSPARENT);
    scratch.horizontal_q32.resize(required, [0; 4]);
    scratch.extent = Some(extent);
    scratch.layout_digest = Some(layout_digest);
    Ok(())
}

fn try_reserve_total<T>(
    storage: &mut Vec<T>,
    required: usize,
) -> Result<(), FieldEvaluationErrorV1> {
    if storage.capacity() < required {
        storage
            .try_reserve_exact(required - storage.len())
            .map_err(|_| FieldEvaluationErrorV1::ResourceExhausted)?;
    }
    Ok(())
}

fn evaluate_region(
    request: &FieldEvaluationRequestV1<'_>,
    output_region: FieldRectV1,
    scratch: &mut FieldEvaluationScratchV1,
) -> Result<(), FieldEvaluationErrorV1> {
    match &request.operation {
        FieldOperationV1::GaussianBlur {
            source,
            kernel,
            edge_mode,
        } => evaluate_gaussian_region(*source, kernel, *edge_mode, output_region, scratch),
        FieldOperationV1::PremultipliedSourceOver {
            source,
            destination,
        } => evaluate_pointwise_region(output_region, scratch, |pixel_index| {
            premultiplied_source_over(
                source.pixels()[pixel_index],
                destination.pixels()[pixel_index],
            )
        }),
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop { source, backdrop } => {
            evaluate_pointwise_region(output_region, scratch, |pixel_index| {
                screen_opaque_backdrop(source.pixels()[pixel_index], backdrop.pixels()[pixel_index])
            })
        }
        FieldOperationV1::PorterDuffLighter {
            source,
            destination,
        } => evaluate_pointwise_region(output_region, scratch, |pixel_index| {
            porter_duff_lighter(
                source.pixels()[pixel_index],
                destination.pixels()[pixel_index],
            )
        }),
    }
}

fn evaluate_pointwise_region(
    region: FieldRectV1,
    scratch: &mut FieldEvaluationScratchV1,
    mut evaluate: impl FnMut(usize) -> Result<PremultipliedRgba8V1, FieldEvaluationErrorV1>,
) -> Result<(), FieldEvaluationErrorV1> {
    let end_y = region.end_y()?;
    let end_x = region.end_x()?;
    for y in region.y()..end_y {
        for x in region.x()..end_x {
            let pixel_index = field_index(region.extent(), x, y)?;
            scratch.output[pixel_index] = evaluate(pixel_index)?;
        }
    }
    Ok(())
}

fn evaluate_gaussian_region(
    source: FieldRasterViewV1<'_>,
    kernel: &GaussianKernelV1,
    edge_mode: GaussianEdgeModeV1,
    output_region: FieldRectV1,
    scratch: &mut FieldEvaluationScratchV1,
) -> Result<(), FieldEvaluationErrorV1> {
    let extent = source.extent();
    let input_rows = output_region.expanded(kernel.device_radius_px(), extent)?;
    let output_end_x = output_region.end_x()?;
    let input_end_y = input_rows.end_y()?;

    for y in input_rows.y()..input_end_y {
        for x in output_region.x()..output_end_x {
            let mut sums = [0_u64; 4];
            for (weight_index, weight) in kernel.weights_q32().iter().copied().enumerate() {
                let sample_x = gaussian_sample_coordinate(
                    x,
                    weight_index,
                    kernel.device_radius_px(),
                    extent.width(),
                    edge_mode,
                )?;
                let sample = source.pixels()[field_index(extent, sample_x, y)?].channels();
                for channel in 0..4 {
                    let contribution = u64::from(sample[channel])
                        .checked_mul(u64::from(weight))
                        .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)?;
                    sums[channel] = sums[channel]
                        .checked_add(contribution)
                        .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)?;
                }
            }
            let index = field_index(extent, x, y)?;
            scratch.horizontal_q32[index] = sums;
        }
    }

    let output_end_y = output_region.end_y()?;
    for y in output_region.y()..output_end_y {
        for x in output_region.x()..output_end_x {
            let mut sums = [0_u128; 4];
            for (weight_index, weight) in kernel.weights_q32().iter().copied().enumerate() {
                let sample_y = gaussian_sample_coordinate(
                    y,
                    weight_index,
                    kernel.device_radius_px(),
                    extent.height(),
                    edge_mode,
                )?;
                let horizontal = scratch.horizontal_q32[field_index(extent, x, sample_y)?];
                for channel in 0..4 {
                    let contribution = u128::from(horizontal[channel])
                        .checked_mul(u128::from(weight))
                        .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)?;
                    sums[channel] = sums[channel]
                        .checked_add(contribution)
                        .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)?;
                }
            }
            let mut channels = [0_u8; 4];
            for channel in 0..4 {
                let rounded = sums[channel]
                    .checked_add(1_u128 << 63)
                    .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)?
                    >> 64;
                channels[channel] =
                    u8::try_from(rounded).map_err(|_| FieldEvaluationErrorV1::InternalInvariant)?;
            }
            let pixel = PremultipliedRgba8V1::try_new(channels)
                .map_err(|_| FieldEvaluationErrorV1::InternalInvariant)?;
            let output_index = field_index(extent, x, y)?;
            scratch.output[output_index] = pixel;
        }
    }
    Ok(())
}

fn gaussian_sample_coordinate(
    coordinate: u32,
    weight_index: usize,
    radius: u32,
    limit: u32,
    edge_mode: GaussianEdgeModeV1,
) -> Result<u32, FieldEvaluationErrorV1> {
    let weight_index =
        i64::try_from(weight_index).map_err(|_| FieldEvaluationErrorV1::GeometryOverflow)?;
    let offset = weight_index - i64::from(radius);
    let candidate = i64::from(coordinate)
        .checked_add(offset)
        .ok_or(FieldEvaluationErrorV1::GeometryOverflow)?;
    match edge_mode {
        GaussianEdgeModeV1::ClampToEdgeV1 => {
            if candidate < 0 {
                Ok(0)
            } else if candidate >= i64::from(limit) {
                Ok(limit - 1)
            } else {
                u32::try_from(candidate).map_err(|_| FieldEvaluationErrorV1::GeometryOverflow)
            }
        }
    }
}

fn premultiplied_source_over(
    source: PremultipliedRgba8V1,
    destination: PremultipliedRgba8V1,
) -> Result<PremultipliedRgba8V1, FieldEvaluationErrorV1> {
    let source = source.channels();
    let destination = destination.channels();
    let inverse_alpha = u16::from(u8::MAX - source[3]);
    let mut output = [0_u8; 4];
    for channel in 0..4 {
        let attenuated = (u16::from(destination[channel]) * inverse_alpha + 127) / 255;
        let value = u16::from(source[channel])
            .checked_add(attenuated)
            .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)?;
        output[channel] =
            u8::try_from(value).map_err(|_| FieldEvaluationErrorV1::InternalInvariant)?;
    }
    PremultipliedRgba8V1::try_new(output).map_err(|_| FieldEvaluationErrorV1::InternalInvariant)
}

fn screen_opaque_backdrop(
    source: EncodedSrgb8AlphaV1,
    backdrop: Srgb8,
) -> Result<PremultipliedRgba8V1, FieldEvaluationErrorV1> {
    let tint = source.tint().bytes();
    let alpha = source.alpha().value();
    let backdrop = backdrop.bytes();
    let mut output = [0_u8; 4];
    for channel in 0..3 {
        // This order is the legacy-compatible encoded-sRGB8 reference law:
        // one binary64 alpha, straight tint, and one final byte rounding.
        let value = (f64::from(backdrop[channel])
            + alpha * f64::from(tint[channel]) * f64::from(u8::MAX - backdrop[channel])
                / f64::from(u8::MAX))
        .round();
        if !(0.0..=f64::from(u8::MAX)).contains(&value) {
            return Err(FieldEvaluationErrorV1::InternalInvariant);
        }
        output[channel] = value as u8;
    }
    output[3] = u8::MAX;
    PremultipliedRgba8V1::try_new(output).map_err(|_| FieldEvaluationErrorV1::InternalInvariant)
}

fn porter_duff_lighter(
    source: PremultipliedRgba8V1,
    destination: PremultipliedRgba8V1,
) -> Result<PremultipliedRgba8V1, FieldEvaluationErrorV1> {
    let source = source.channels();
    let destination = destination.channels();
    let mut output = [0_u8; 4];
    for channel in 0..4 {
        // Capping at one is the Porter-Duff lighter law, not error recovery.
        output[channel] = u16::from(source[channel])
            .checked_add(u16::from(destination[channel]))
            .ok_or(FieldEvaluationErrorV1::ArithmeticOverflow)?
            .min(u16::from(u8::MAX)) as u8;
    }
    PremultipliedRgba8V1::try_new(output).map_err(|_| FieldEvaluationErrorV1::InternalInvariant)
}

fn enforce_carrier_intent(
    request: &FieldEvaluationRequestV1<'_>,
    output: &[PremultipliedRgba8V1],
) -> Result<(), FieldEvaluationErrorV1> {
    let mut carrier_present = false;
    for pixel_index in 0..output.len() {
        if carrier_sample(&request.operation, pixel_index)?.is_present() {
            carrier_present = true;
            break;
        }
    }
    if !carrier_present {
        return Err(FieldEvaluationErrorV1::CarrierAbsent);
    }
    if request.carrier_intent == CarrierIntentV1::Present {
        return Ok(());
    }

    let mut first_contribution: Option<(
        CarrierSampleV1,
        PremultipliedRgba8V1,
        PremultipliedRgba8V1,
    )> = None;
    let mut contribution_varies = false;
    for (pixel_index, actual) in output.iter().copied().enumerate() {
        let counterfactual = counterfactual_pixel(&request.operation, pixel_index)?;
        if actual == counterfactual {
            continue;
        }
        let signature = (
            carrier_sample(&request.operation, pixel_index)?,
            counterfactual,
            actual,
        );
        if let Some(first) = first_contribution {
            if signature != first {
                contribution_varies = true;
            }
        } else {
            first_contribution = Some(signature);
        }
    }
    if first_contribution.is_none() {
        return Err(FieldEvaluationErrorV1::CarrierErased);
    }
    if request.carrier_intent == CarrierIntentV1::SpatialVariation && !contribution_varies {
        return Err(FieldEvaluationErrorV1::CarrierVariationErased);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CarrierSampleV1 {
    Premultiplied(PremultipliedRgba8V1),
    StraightScreen(EncodedSrgb8AlphaV1),
}

impl CarrierSampleV1 {
    const fn is_present(self) -> bool {
        match self {
            Self::Premultiplied(pixel) => pixel.alpha() != 0,
            Self::StraightScreen(pixel) => pixel.alpha().bits() != 0,
        }
    }
}

fn carrier_sample(
    operation: &FieldOperationV1<'_>,
    pixel_index: usize,
) -> Result<CarrierSampleV1, FieldEvaluationErrorV1> {
    match operation {
        FieldOperationV1::GaussianBlur { source, .. }
        | FieldOperationV1::PremultipliedSourceOver { source, .. }
        | FieldOperationV1::PorterDuffLighter { source, .. } => source
            .pixels()
            .get(pixel_index)
            .copied()
            .map(CarrierSampleV1::Premultiplied)
            .ok_or(FieldEvaluationErrorV1::InternalInvariant),
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop { source, .. } => source
            .pixels()
            .get(pixel_index)
            .copied()
            .map(CarrierSampleV1::StraightScreen)
            .ok_or(FieldEvaluationErrorV1::InternalInvariant),
    }
}

fn counterfactual_pixel(
    operation: &FieldOperationV1<'_>,
    pixel_index: usize,
) -> Result<PremultipliedRgba8V1, FieldEvaluationErrorV1> {
    match operation {
        FieldOperationV1::GaussianBlur { .. } => Ok(PremultipliedRgba8V1::TRANSPARENT),
        FieldOperationV1::PremultipliedSourceOver { destination, .. }
        | FieldOperationV1::PorterDuffLighter { destination, .. } => destination
            .pixels()
            .get(pixel_index)
            .copied()
            .ok_or(FieldEvaluationErrorV1::InternalInvariant),
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop { backdrop, .. } => {
            let bytes = backdrop
                .pixels()
                .get(pixel_index)
                .copied()
                .ok_or(FieldEvaluationErrorV1::InternalInvariant)?
                .bytes();
            PremultipliedRgba8V1::try_new([bytes[0], bytes[1], bytes[2], u8::MAX])
                .map_err(|_| FieldEvaluationErrorV1::InternalInvariant)
        }
    }
}

fn field_index(extent: FieldExtentV1, x: u32, y: u32) -> Result<usize, FieldEvaluationErrorV1> {
    if x >= extent.width() || y >= extent.height() {
        return Err(FieldEvaluationErrorV1::RectOutsideExtent);
    }
    let index = u64::from(y)
        .checked_mul(u64::from(extent.width()))
        .and_then(|row| row.checked_add(u64::from(x)))
        .ok_or(FieldEvaluationErrorV1::GeometryOverflow)?;
    usize::try_from(index).map_err(|_| FieldEvaluationErrorV1::GeometryOverflow)
}

fn evaluation_layout_digest(
    request: &FieldEvaluationRequestV1<'_>,
) -> FieldEvaluationLayoutDigestV1 {
    let mut hasher = Hasher::new();
    hasher.update(b"labcolors-field-evaluation-layout-v1\0");
    hash_geometry(&mut hasher, request.geometry);
    hash_u8(&mut hasher, request.device_pixel_ratio.value());
    hash_working_space(&mut hasher, request.working_space);
    hash_precision(&mut hasher, request.precision);
    hash_quantization(&mut hasher, request.quantization);
    hash_output_capability(&mut hasher, request.render_capability.output());
    hash_operator_kind(&mut hasher, request.operator_kind());
    hash_kernel_metadata(&mut hasher, &request.operation);
    FieldEvaluationLayoutDigestV1(finalize(hasher))
}

fn raster_digest(extent: FieldExtentV1, pixels: &[PremultipliedRgba8V1]) -> FieldRasterDigestV1 {
    let mut hasher = Hasher::new();
    hasher.update(b"labcolors-field-raster-v1\0");
    hash_extent(&mut hasher, extent);
    hash_usize(&mut hasher, pixels.len());
    for pixel in pixels {
        hasher.update(&pixel.channels());
    }
    FieldRasterDigestV1(finalize(hasher))
}

fn kernel_digest(operation: &FieldOperationV1<'_>) -> FieldKernelDigestV1 {
    let mut hasher = Hasher::new();
    hasher.update(b"labcolors-field-kernel-v1\0");
    hash_operator_kind(&mut hasher, operation.kind());
    hash_kernel_metadata(&mut hasher, operation);
    FieldKernelDigestV1(finalize(hasher))
}

#[allow(clippy::too_many_arguments)]
fn certificate_digest(
    request_digest: FieldRequestDigestV1,
    output_digest: FieldRasterDigestV1,
    kernel_digest: FieldKernelDigestV1,
    evidence_identity: FieldEvidenceIdentityV1,
    evidence_class: FieldEvidenceClassV1,
    request: &FieldEvaluationRequestV1<'_>,
    evidence: FieldEvidenceV1<'_>,
) -> FieldCertificateDigestV1 {
    let mut hasher = Hasher::new();
    hasher.update(b"labcolors-field-certificate-v1\0");
    hasher.update(&request_digest.as_bytes());
    hasher.update(&output_digest.as_bytes());
    hasher.update(&kernel_digest.as_bytes());
    hash_u64(&mut hasher, evidence_identity.value());
    hash_evidence_class(&mut hasher, evidence_class);
    hash_scene_revision(&mut hasher, request.scene_revision);
    hash_render_capability(&mut hasher, request.render_capability);
    match evidence {
        FieldEvidenceV1::ProspectiveObservedWholeRaster(observed) => {
            hash_u64(&mut hasher, observed.raster.identity().value());
            hasher.update(&observed.request_digest.as_bytes());
            hash_scene_revision(&mut hasher, observed.scene_revision);
            hash_render_capability(&mut hasher, observed.render_capability);
            hasher.update(
                &raster_digest(observed.raster.extent(), observed.raster.pixels()).as_bytes(),
            );
        }
        FieldEvidenceV1::ExactReferenceWholeRaster { .. } => {
            hasher.update(b"exact-reference-whole-raster\0");
        }
        FieldEvidenceV1::PointSample { .. }
        | FieldEvidenceV1::FieldSamples { .. }
        | FieldEvidenceV1::FieldAverage { .. }
        | FieldEvidenceV1::GradientStops { .. }
        | FieldEvidenceV1::CommittedRaster { .. } => {
            // The admission boundary rejects these variants before this point.
            hasher.update(b"unreachable-weak-evidence\0");
        }
    }
    FieldCertificateDigestV1(finalize(hasher))
}

fn hash_operation(hasher: &mut Hasher, operation: &FieldOperationV1<'_>) {
    hash_operator_kind(hasher, operation.kind());
    hash_kernel_metadata(hasher, operation);
    match operation {
        FieldOperationV1::GaussianBlur { source, .. } => {
            hash_premultiplied_raster(hasher, *source);
        }
        FieldOperationV1::PremultipliedSourceOver {
            source,
            destination,
        }
        | FieldOperationV1::PorterDuffLighter {
            source,
            destination,
        } => {
            hash_premultiplied_raster(hasher, *source);
            hash_premultiplied_raster(hasher, *destination);
        }
        FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop { source, backdrop } => {
            hash_screen_raster(hasher, *source);
            hash_opaque_raster(hasher, *backdrop);
        }
    }
}

fn hash_kernel_metadata(hasher: &mut Hasher, operation: &FieldOperationV1<'_>) {
    match operation {
        FieldOperationV1::GaussianBlur {
            kernel, edge_mode, ..
        } => {
            hash_u8(
                hasher,
                match kernel.profile() {
                    GaussianKernelProfileV1::BinomialGaussianQ32V1 => 1,
                },
            );
            hash_u32(hasher, kernel.css_radius_px());
            hash_u8(hasher, kernel.device_pixel_ratio().value());
            hash_u32(hasher, kernel.device_radius_px());
            hash_u8(
                hasher,
                match edge_mode {
                    GaussianEdgeModeV1::ClampToEdgeV1 => 1,
                },
            );
            hash_usize(hasher, kernel.weights_q32().len());
            for weight in kernel.weights_q32() {
                hash_u32(hasher, *weight);
            }
        }
        FieldOperationV1::PremultipliedSourceOver { .. }
        | FieldOperationV1::EncodedSrgb8ScreenOpaqueBackdrop { .. }
        | FieldOperationV1::PorterDuffLighter { .. } => {
            hasher.update(b"no-spatial-kernel\0");
        }
    }
}

fn hash_premultiplied_raster(hasher: &mut Hasher, raster: FieldRasterViewV1<'_>) {
    hash_u64(hasher, raster.identity().value());
    hash_extent(hasher, raster.extent());
    hash_usize(hasher, raster.pixels().len());
    for pixel in raster.pixels() {
        hasher.update(&pixel.channels());
    }
}

fn hash_opaque_raster(hasher: &mut Hasher, raster: OpaqueSrgb8RasterViewV1<'_>) {
    hash_u64(hasher, raster.identity().value());
    hash_extent(hasher, raster.extent());
    hash_usize(hasher, raster.pixels().len());
    for pixel in raster.pixels() {
        hasher.update(&pixel.bytes());
    }
}

fn hash_screen_raster(hasher: &mut Hasher, raster: EncodedSrgb8AlphaRasterViewV1<'_>) {
    hash_u64(hasher, raster.identity().value());
    hash_extent(hasher, raster.extent());
    hash_usize(hasher, raster.pixels().len());
    for pixel in raster.pixels() {
        hasher.update(&pixel.tint().bytes());
        hash_u64(hasher, pixel.alpha().bits());
    }
}

fn hash_geometry(hasher: &mut Hasher, geometry: FieldGeometryV1) {
    hash_extent(hasher, geometry.extent());
    hasher.update(b"device-pixels-top-left-v1\0");
}

fn hash_extent(hasher: &mut Hasher, extent: FieldExtentV1) {
    hash_u32(hasher, extent.width());
    hash_u32(hasher, extent.height());
}

fn hash_working_space(hasher: &mut Hasher, working_space: FieldWorkingSpaceV1) {
    hash_u8(
        hasher,
        match working_space {
            FieldWorkingSpaceV1::EncodedSrgb8PremultipliedV1 => 1,
            FieldWorkingSpaceV1::LinearSrgbQ31PremultipliedV1 => 2,
        },
    );
}

fn hash_precision(hasher: &mut Hasher, precision: FieldPrecisionV1) {
    hash_u8(
        hasher,
        match precision {
            FieldPrecisionV1::FixedQ32V1 => 1,
            FieldPrecisionV1::Binary32V1 => 2,
        },
    );
}

fn hash_quantization(hasher: &mut Hasher, quantization: FieldQuantizationV1) {
    hash_u8(
        hasher,
        match quantization {
            FieldQuantizationV1::RoundHalfUpSrgb8V1 => 1,
            FieldQuantizationV1::RoundTiesToEvenSrgb8V1 => 2,
        },
    );
}

fn hash_render_capability(hasher: &mut Hasher, capability: FieldRenderCapabilityV1) {
    match capability.renderer().kind() {
        FieldRendererCapabilityKindV1::ExactReference { renderer } => {
            hash_u8(hasher, 1);
            hash_u64(hasher, renderer.value());
        }
        FieldRendererCapabilityKindV1::HostConformant {
            renderer,
            conformance,
        } => {
            hash_u8(hasher, 2);
            hash_u64(hasher, renderer.value());
            hash_u64(hasher, conformance.value());
        }
        FieldRendererCapabilityKindV1::Unknown { renderer } => {
            hash_u8(hasher, 3);
            hash_u64(hasher, renderer.value());
        }
        FieldRendererCapabilityKindV1::Unsupported { renderer, reason } => {
            hash_u8(hasher, 4);
            hash_u64(hasher, renderer.value());
            hash_u64(hasher, reason.value());
        }
    }
    hash_output_capability(hasher, capability.output());
}

fn hash_output_capability(hasher: &mut Hasher, output: FieldOutputCapabilityV1) {
    hash_u8(
        hasher,
        match output {
            FieldOutputCapabilityV1::PremultipliedRgba8V1 => 1,
            FieldOutputCapabilityV1::OpaqueSrgb8V1 => 2,
        },
    );
}

fn hash_scene_revision(hasher: &mut Hasher, scene_revision: FieldSceneRevisionV1) {
    hash_u32(hasher, scene_revision.stream().value());
    hash_u64(hasher, scene_revision.revision().value());
}

fn hash_carrier_intent(hasher: &mut Hasher, intent: CarrierIntentV1) {
    hash_u8(
        hasher,
        match intent {
            CarrierIntentV1::Present => 1,
            CarrierIntentV1::Contributes => 2,
            CarrierIntentV1::SpatialVariation => 3,
        },
    );
}

fn hash_operator_kind(hasher: &mut Hasher, kind: FieldOperatorKindV1) {
    hash_u8(
        hasher,
        match kind {
            FieldOperatorKindV1::GaussianBlurSeparableQ32V1 => 1,
            FieldOperatorKindV1::PremultipliedSourceOverV1 => 2,
            FieldOperatorKindV1::EncodedSrgb8ScreenOpaqueBackdropV1 => 3,
            FieldOperatorKindV1::PorterDuffLighterV1 => 4,
        },
    );
}

fn hash_evidence_class(hasher: &mut Hasher, class: FieldEvidenceClassV1) {
    hash_u8(
        hasher,
        match class {
            FieldEvidenceClassV1::PointSample => 1,
            FieldEvidenceClassV1::FieldSamples => 2,
            FieldEvidenceClassV1::FieldAverage => 3,
            FieldEvidenceClassV1::GradientStops => 4,
            FieldEvidenceClassV1::CommittedRaster => 5,
            FieldEvidenceClassV1::ExactReferenceWholeRaster => 6,
            FieldEvidenceClassV1::ProspectiveObservedWholeRaster => 7,
        },
    );
}

fn hash_u8(hasher: &mut Hasher, value: u8) {
    hasher.update(&[value]);
}

fn hash_u32(hasher: &mut Hasher, value: u32) {
    hasher.update(&value.to_be_bytes());
}

fn hash_u64(hasher: &mut Hasher, value: u64) {
    hasher.update(&value.to_be_bytes());
}

fn hash_usize(hasher: &mut Hasher, value: usize) {
    hash_u64(hasher, value as u64);
}

fn finalize(hasher: Hasher) -> [u8; 32] {
    let digest = hasher.finalize();
    *digest.as_bytes()
}
