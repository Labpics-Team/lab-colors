//! Type foundation for a context-bound Labpics Colors Space occurrence.
//!
//! This module intentionally contains no colour transforms. It makes the
//! physical identity split representable before the existing kernels are moved:
//! encoded output, framed tristimulus evidence, appearance context and derived
//! hue state are different values. In particular, an occurrence has no inverse
//! operation accepting an arbitrary second context.

/// A registered encoded-output domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OutputProfileId {
    Iec61966Srgb8D65V1,
}

/// A registered render operation. This is deliberately not an output profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RenderProfileId {
    EncodedSrgb8PointV1,
}

/// Exact encoded channels plus the profile which gives those channels meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColorSignal {
    channels: [u8; 3],
    output_profile: OutputProfileId,
}

impl ColorSignal {
    pub(crate) const fn new(channels: [u8; 3], output_profile: OutputProfileId) -> Self {
        Self {
            channels,
            output_profile,
        }
    }

    pub(crate) const fn channels(self) -> [u8; 3] {
        self.channels
    }

    pub(crate) const fn output_profile(self) -> OutputProfileId {
        self.output_profile
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObserverProfileId {
    Cie1931TwoDegreeV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ReferenceWhiteId {
    Iec61966D65ChromaticityV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TristimulusScale {
    RelativeY1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColorimetricFrameReleaseId {
    XyzV1,
    #[cfg(test)]
    MutationSentinelV1,
}

/// Everything required to interpret one XYZ triple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColorimetricFrameId {
    observer: ObserverProfileId,
    reference_white: ReferenceWhiteId,
    scale: TristimulusScale,
    release: ColorimetricFrameReleaseId,
}

impl ColorimetricFrameId {
    pub(crate) const fn new(
        observer: ObserverProfileId,
        reference_white: ReferenceWhiteId,
        scale: TristimulusScale,
        release: ColorimetricFrameReleaseId,
    ) -> Self {
        Self {
            observer,
            reference_white,
            scale,
            release,
        }
    }

    pub(crate) const fn observer(self) -> ObserverProfileId {
        self.observer
    }

    pub(crate) const fn reference_white(self) -> ReferenceWhiteId {
        self.reference_white
    }

    pub(crate) const fn scale(self) -> TristimulusScale {
        self.scale
    }

    pub(crate) const fn release(self) -> ColorimetricFrameReleaseId {
        self.release
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericDomainError {
    NonFinite,
    Negative,
    NotPositive,
    AboveOne,
    HueOutOfRange,
}

/// Finite, non-negative binary64 value with canonical positive zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FiniteNonNegative(u64);

impl FiniteNonNegative {
    pub(crate) fn new(value: f64) -> Result<Self, NumericDomainError> {
        if !value.is_finite() {
            return Err(NumericDomainError::NonFinite);
        }
        if value < 0.0 {
            return Err(NumericDomainError::Negative);
        }
        Ok(Self(if value == 0.0 {
            0.0_f64.to_bits()
        } else {
            value.to_bits()
        }))
    }

    pub(crate) fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

/// One finite XYZ point plus the identity of its colorimetric frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TristimulusSample {
    xyz: [FiniteNonNegative; 3],
    frame: ColorimetricFrameId,
}

impl TristimulusSample {
    pub(crate) fn new(
        xyz: [f64; 3],
        frame: ColorimetricFrameId,
    ) -> Result<Self, NumericDomainError> {
        Ok(Self {
            xyz: [
                FiniteNonNegative::new(xyz[0])?,
                FiniteNonNegative::new(xyz[1])?,
                FiniteNonNegative::new(xyz[2])?,
            ],
            frame,
        })
    }

    pub(crate) fn xyz(self) -> [f64; 3] {
        self.xyz.map(FiniteNonNegative::get)
    }

    pub(crate) const fn frame(self) -> ColorimetricFrameId {
        self.frame
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppearanceContextReleaseId {
    Cam16V1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurroundProfileId {
    AverageV1,
    DimV1,
    DarkV1,
}

/// Content identity of immutable semantic viewing inputs.
///
/// Derived CAM constants are intentionally absent and must remain a private
/// cache of the observer implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppearanceContextId {
    release: AppearanceContextReleaseId,
    frame: ColorimetricFrameId,
    adapting_luminance: FiniteNonNegative,
    background_relative_luminance: FiniteNonNegative,
    surround: SurroundProfileId,
}

impl AppearanceContextId {
    pub(crate) fn new(
        release: AppearanceContextReleaseId,
        frame: ColorimetricFrameId,
        adapting_luminance: f64,
        background_relative_luminance: f64,
        surround: SurroundProfileId,
    ) -> Result<Self, NumericDomainError> {
        let adapting_luminance = FiniteNonNegative::new(adapting_luminance)?;
        if adapting_luminance.get() == 0.0 {
            return Err(NumericDomainError::NotPositive);
        }
        let background_relative_luminance =
            FiniteNonNegative::new(background_relative_luminance)?;
        if background_relative_luminance.get() == 0.0 {
            return Err(NumericDomainError::NotPositive);
        }
        if background_relative_luminance.get() > 1.0 {
            return Err(NumericDomainError::AboveOne);
        }
        Ok(Self {
            release,
            frame,
            adapting_luminance,
            background_relative_luminance,
            surround,
        })
    }

    pub(crate) const fn frame(self) -> ColorimetricFrameId {
        self.frame
    }
}

/// A finite angle; absence of hue is represented by [`HueState`], never `0°`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HueAngle(u64);

impl HueAngle {
    pub(crate) fn new(degrees: f64) -> Result<Self, NumericDomainError> {
        if !degrees.is_finite() {
            return Err(NumericDomainError::NonFinite);
        }
        if !(0.0..360.0).contains(&degrees) {
            return Err(NumericDomainError::HueOutOfRange);
        }
        Ok(Self(if degrees == 0.0 {
            0.0_f64.to_bits()
        } else {
            degrees.to_bits()
        }))
    }

    pub(crate) fn degrees(self) -> f64 {
        f64::from_bits(self.0)
    }
}

/// Opaque identity of an admitted powerless-hue rule.
///
/// It has no constructor until a concrete named-view release is registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HuePowerlessProfileId(u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HueState {
    Defined(HueAngle),
    UndefinedExact,
    PowerlessBy(HuePowerlessProfileId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObserveError {
    FrameMismatch {
        stimulus: ColorimetricFrameId,
        context: ColorimetricFrameId,
    },
}

/// LCS identity: one physical sample observed in one immutable context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LcsOccurrence {
    sample: TristimulusSample,
    context: AppearanceContextId,
}

impl LcsOccurrence {
    /// Observe one sample in one context with an exactly matching frame.
    ///
    /// Named appearance views are derived later from this pair. No view
    /// coordinate is accepted here, so contradictory cached views cannot become
    /// part of occurrence identity.
    pub(crate) fn observe(
        sample: TristimulusSample,
        context: AppearanceContextId,
    ) -> Result<Self, ObserveError> {
        if sample.frame() != context.frame() {
            return Err(ObserveError::FrameMismatch {
                stimulus: sample.frame(),
                context: context.frame(),
            });
        }
        Ok(Self { sample, context })
    }

    pub(crate) const fn sample(self) -> TristimulusSample {
        self.sample
    }

    pub(crate) const fn context(self) -> AppearanceContextId {
        self.context
    }
}
