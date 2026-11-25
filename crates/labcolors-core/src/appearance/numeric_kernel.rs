//! Closed numeric interpretations of the one compiled appearance program.

use super::*;
use crate::composition::{AdmittedOpacityV1, source_over_channel_continuous_value};

mod sealed {
    pub trait Sealed {}
}

/// Only arithmetic and materialized values vary. Topological traversal,
/// input identity and the order of opacity multiplication stay in the program.
pub(crate) trait AppearanceNumericKernel: sealed::Sealed + std::fmt::Debug {
    type Color: Copy + std::fmt::Debug;
    type Occurrence: Copy + std::fmt::Debug;

    fn visible(occurrence: Self::Occurrence) -> Self::Color;
    fn compose(
        spec: &CompiledOccurrenceSpec,
        subject: PointPaintV1<Self::Color>,
        backdrop: Self::Color,
        backdrop_occurrence: Option<Self::Occurrence>,
    ) -> Self::Occurrence;
}

#[derive(Debug)]
pub(crate) struct ByteAppearanceKernel;
impl sealed::Sealed for ByteAppearanceKernel {}

impl AppearanceNumericKernel for ByteAppearanceKernel {
    type Color = Srgb8;
    type Occurrence = ResolvedOccurrence;

    fn visible(occurrence: Self::Occurrence) -> Self::Color {
        Srgb8::new(occurrence.visible())
    }

    fn compose(
        spec: &CompiledOccurrenceSpec,
        subject: EncodedPointPaintV1,
        backdrop: Srgb8,
        backdrop_occurrence: Option<ResolvedOccurrence>,
    ) -> ResolvedOccurrence {
        let certificate = SourceOverCertificateV1::compose(
            spec.profile,
            subject.source().bytes(),
            subject.opacity(),
            backdrop.bytes(),
        );
        let backdrop_radius = backdrop_occurrence.map_or(0, |value| value.emission.visible_radius);
        let emission = crate::emission::PointEmissionV1 {
            visible_radius: crate::emission::radius_after(
                spec.raster_precision,
                subject.source().bytes(),
                subject.opacity(),
                backdrop.bytes(),
                certificate.output_rgb(),
                backdrop_radius,
            ),
            backdrop_radius,
            same_as_backdrop: subject.opacity().bits() == 0,
        };
        ResolvedOccurrence {
            id: spec.id,
            subject: spec.subject_id,
            against: spec.against_id,
            certificate,
            emission,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContinuousColorAdmissionErrorV1 {
    NonFinite { channel: usize },
    OutsideEncodedSrgb { channel: usize },
}

/// Unquantized encoded-sRGB channels in byte units, each finite in [0,255].
/// This physical domain does not perform gamut mapping or accept extended RGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ContinuousSrgbV1([f64; 3]);

impl ContinuousSrgbV1 {
    pub(crate) fn new(channels: [f64; 3]) -> Result<Self, super::ContinuousColorAdmissionErrorV1> {
        for (channel, value) in channels.iter().copied().enumerate() {
            if !value.is_finite() {
                return Err(ContinuousColorAdmissionErrorV1::NonFinite { channel });
            }
            if !(0.0..=255.0).contains(&value) {
                return Err(ContinuousColorAdmissionErrorV1::OutsideEncodedSrgb { channel });
            }
        }
        Ok(Self(
            channels.map(|value| if value == 0.0 { 0.0 } else { value }),
        ))
    }

    pub(crate) fn from_srgb8(value: Srgb8) -> Self {
        Self(value.bytes().map(f64::from))
    }

    pub(crate) const fn channels(self) -> [f64; 3] {
        self.0
    }
}

pub(crate) type ContinuousPointPaintValueV1 = PointPaintValueV1<ContinuousSrgbV1>;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ContinuousResolvedOccurrenceV1 {
    pub(super) id: OccurrenceId,
    visible: ContinuousSrgbV1,
    backdrop: ContinuousSrgbV1,
}

impl ContinuousResolvedOccurrenceV1 {
    pub(crate) const fn visible(self) -> ContinuousSrgbV1 {
        self.visible
    }

    pub(crate) const fn backdrop(self) -> ContinuousSrgbV1 {
        self.backdrop
    }
}

#[derive(Debug)]
pub(crate) struct ContinuousAppearanceKernel;
impl sealed::Sealed for ContinuousAppearanceKernel {}

#[derive(Debug)]
pub(crate) struct CertifiedAppearanceKernel;
impl sealed::Sealed for CertifiedAppearanceKernel {}

pub(crate) type CertifiedColorResultV1 = Result<
    crate::certified_numeric::CertifiedRgbV1,
    crate::certified_numeric::NumericUncertaintyV1,
>;

#[derive(Debug, Clone, Copy)]
pub(crate) struct CertifiedResolvedOccurrenceV1 {
    pub(super) id: OccurrenceId,
    pub(crate) visible: CertifiedColorResultV1,
    pub(crate) backdrop: CertifiedColorResultV1,
}

impl AppearanceNumericKernel for CertifiedAppearanceKernel {
    type Color = CertifiedColorResultV1;
    type Occurrence = CertifiedResolvedOccurrenceV1;

    fn visible(occurrence: Self::Occurrence) -> Self::Color {
        occurrence.visible
    }

    fn compose(
        spec: &CompiledOccurrenceSpec,
        subject: PointPaintV1<Self::Color>,
        backdrop: Self::Color,
        _backdrop_occurrence: Option<Self::Occurrence>,
    ) -> Self::Occurrence {
        let opacity = subject.opacity();
        let visible = if opacity == AdmittedOpacityV1::OPAQUE {
            subject.source()
        } else if opacity.bits() == 0 {
            backdrop
        } else {
            subject.source().and_then(|source| {
                backdrop.and_then(|backdrop| source.source_over(opacity, backdrop))
            })
        };
        CertifiedResolvedOccurrenceV1 {
            id: spec.id,
            visible,
            backdrop,
        }
    }
}

impl AppearanceNumericKernel for ContinuousAppearanceKernel {
    type Color = ContinuousSrgbV1;
    type Occurrence = ContinuousResolvedOccurrenceV1;

    fn visible(occurrence: Self::Occurrence) -> Self::Color {
        occurrence.visible
    }

    fn compose(
        spec: &CompiledOccurrenceSpec,
        subject: PointPaintV1<ContinuousSrgbV1>,
        backdrop: ContinuousSrgbV1,
        _backdrop_occurrence: Option<ContinuousResolvedOccurrenceV1>,
    ) -> ContinuousResolvedOccurrenceV1 {
        let source = subject.source();
        let opacity = subject.opacity();
        let channels = match spec.profile {
            CompositionProfileV1::EncodedSrgb8SourceOverV1 => {
                // Exact physical endpoints avoid cancellation of a fractional
                // source at alpha=1; no intermediate quantization is introduced.
                if opacity == AdmittedOpacityV1::OPAQUE {
                    source.channels()
                } else if opacity.bits() == 0 {
                    backdrop.channels()
                } else {
                    std::array::from_fn(|channel| {
                        source_over_channel_continuous_value(
                            source.0[channel],
                            opacity.value(),
                            backdrop.0[channel],
                        )
                    })
                }
            }
        };
        // This evaluates points in the declared binary64 order. Rounding can
        // break monotonicity between adjacent fractional inputs; byte endpoint
        // bounds must not be reused as continuous interval certificates.
        // Convex interpolation of admitted channels remains finite in [0,255].
        // There is no clamp: every intermediate keeps the actual binary64 value.
        debug_assert!(
            channels
                .iter()
                .all(|value| value.is_finite() && (0.0..=255.0).contains(value))
        );
        ContinuousResolvedOccurrenceV1 {
            id: spec.id,
            visible: ContinuousSrgbV1(channels),
            backdrop,
        }
    }
}
