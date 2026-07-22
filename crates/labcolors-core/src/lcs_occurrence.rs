//! Type foundation for a context-bound Labpics Colors Space occurrence.
//!
//! Encoded output, a framed tristimulus, immutable appearance context and
//! separately named derived views are different values. Executable transforms
//! are sealed and versioned: encoded sRGB8 lowers through the existing IEC
//! transfer table and XYZ(D65) matrix, then an occurrence can derive independent
//! rectangular Oklab and contextual CAM16 views. These are deterministic model
//! derivations, not evidence that a host rendered or a person observed the
//! result. In particular, an occurrence has no inverse operation accepting an
//! arbitrary second context.

use crate::Srgb8;
use crate::spaces::cam16::forward_correlates_v1;
use crate::spaces::oklab::xyz_d65_to_oklab_v1;
use crate::spaces::srgb::xyz_d65_from_srgb8_v1;
use crate::spaces::vc::{Cam16SurroundV1, ViewingConditions};

/// A registered encoded-output domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OutputProfileId {
    Iec61966Srgb8D65V1,
}

/// Exact encoded channels plus the output profile which gives them meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ColorSignal {
    srgb8: Srgb8,
    output_profile: OutputProfileId,
}

impl ColorSignal {
    /// Form the only admitted encoded signal without accepting a free-form
    /// channel/profile pairing.
    pub(crate) const fn from_srgb8(srgb8: Srgb8) -> Self {
        Self {
            srgb8,
            output_profile: OutputProfileId::Iec61966Srgb8D65V1,
        }
    }

    pub(crate) const fn srgb8(self) -> Srgb8 {
        self.srgb8
    }

    pub(crate) const fn output_profile(self) -> OutputProfileId {
        self.output_profile
    }
}

/// Exact code release for one colorimetric signal-to-tristimulus transform.
///
/// This is not a composition profile, renderer capability or observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ColorimetricTransformReleaseId {
    Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1,
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
    const fn registered(
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

/// Canonical result frame of the registered encoded-sRGB8 transform.
pub(crate) const IEC_SRGB_D65_XYZ_FRAME_V1: ColorimetricFrameId = ColorimetricFrameId::registered(
    ObserverProfileId::Cie1931TwoDegreeV1,
    ReferenceWhiteId::Iec61966D65ChromaticityV1,
    TristimulusScale::RelativeY1,
    ColorimetricFrameReleaseId::XyzV1,
);

#[cfg(test)]
pub(crate) const MUTATION_SENTINEL_XYZ_FRAME_V1: ColorimetricFrameId =
    ColorimetricFrameId::registered(
        ObserverProfileId::Cie1931TwoDegreeV1,
        ReferenceWhiteId::Iec61966D65ChromaticityV1,
        TristimulusScale::RelativeY1,
        ColorimetricFrameReleaseId::MutationSentinelV1,
    );

/// The one closed, code-owned binding admitted by the current F0 slice.
///
/// A variant is the tuple: independent profile, transform and frame fields
/// cannot be authored or mixed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AdmittedSrgb8TristimulusBindingV1 {
    Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1,
}

impl AdmittedSrgb8TristimulusBindingV1 {
    pub(crate) const fn signal_output_profile(self) -> OutputProfileId {
        match self {
            Self::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 => {
                OutputProfileId::Iec61966Srgb8D65V1
            }
        }
    }

    pub(crate) const fn transform_release(self) -> ColorimetricTransformReleaseId {
        match self {
            Self::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 => {
                ColorimetricTransformReleaseId::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1
            }
        }
    }

    pub(crate) const fn result_frame(self) -> ColorimetricFrameId {
        match self {
            Self::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 => IEC_SRGB_D65_XYZ_FRAME_V1,
        }
    }
}

pub(crate) const ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1: AdmittedSrgb8TristimulusBindingV1 =
    AdmittedSrgb8TristimulusBindingV1::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1;

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
struct FiniteNonNegative(u64);

impl FiniteNonNegative {
    fn new(value: f64) -> Result<Self, NumericDomainError> {
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

    fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

/// One finite XYZ point plus the identity of its colorimetric frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TristimulusSample {
    xyz: [FiniteNonNegative; 3],
    frame: ColorimetricFrameId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TristimulusComponentV1 {
    X,
    Y,
    Z,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TristimulusDomainErrorV1 {
    component: TristimulusComponentV1,
    reason: NumericDomainError,
}

impl TristimulusDomainErrorV1 {
    pub(crate) const fn component(self) -> TristimulusComponentV1 {
        self.component
    }

    pub(crate) const fn reason(self) -> NumericDomainError {
        self.reason
    }
}

impl TristimulusSample {
    fn try_from_registered_xyz(
        xyz: [f64; 3],
        frame: ColorimetricFrameId,
    ) -> Result<Self, TristimulusDomainErrorV1> {
        let admit = |value, component| {
            FiniteNonNegative::new(value)
                .map_err(|reason| TristimulusDomainErrorV1 { component, reason })
        };
        Ok(Self {
            xyz: [
                admit(xyz[0], TristimulusComponentV1::X)?,
                admit(xyz[1], TristimulusComponentV1::Y)?,
                admit(xyz[2], TristimulusComponentV1::Z)?,
            ],
            frame,
        })
    }

    #[cfg(test)]
    pub(crate) fn try_from_xyz_for_test(
        xyz: [f64; 3],
        frame: ColorimetricFrameId,
    ) -> Result<Self, TristimulusDomainErrorV1> {
        Self::try_from_registered_xyz(xyz, frame)
    }

    pub(crate) fn xyz(self) -> [f64; 3] {
        self.xyz.map(FiniteNonNegative::get)
    }

    pub(crate) const fn frame(self) -> ColorimetricFrameId {
        self.frame
    }
}

/// Content-bound provenance of one deterministic modeled transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModeledTristimulusProvenanceV1 {
    source_signal: ColorSignal,
    binding: AdmittedSrgb8TristimulusBindingV1,
}

impl ModeledTristimulusProvenanceV1 {
    pub(crate) const fn source_signal(self) -> ColorSignal {
        self.source_signal
    }

    pub(crate) const fn binding(self) -> AdmittedSrgb8TristimulusBindingV1 {
        self.binding
    }
}

/// Replayable ideal colorimetric derivation under one admitted binding.
///
/// This is not a renderer capability, render observation, human observation or
/// certified field bound. It cannot by itself satisfy such predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModeledTristimulusDerivationV1 {
    sample: TristimulusSample,
    provenance: ModeledTristimulusProvenanceV1,
}

impl ModeledTristimulusDerivationV1 {
    pub(crate) const fn sample(self) -> TristimulusSample {
        self.sample
    }

    pub(crate) const fn provenance(self) -> ModeledTristimulusProvenanceV1 {
        self.provenance
    }

    pub(crate) fn replay(self) -> Result<TristimulusSample, TristimulusDomainErrorV1> {
        derive_sample_with_binding(self.provenance.source_signal, self.provenance.binding)
    }
}

fn admitted_binding(output_profile: OutputProfileId) -> AdmittedSrgb8TristimulusBindingV1 {
    match output_profile {
        OutputProfileId::Iec61966Srgb8D65V1 => ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1,
    }
}

fn derive_sample_with_binding(
    signal: ColorSignal,
    binding: AdmittedSrgb8TristimulusBindingV1,
) -> Result<TristimulusSample, TristimulusDomainErrorV1> {
    let xyz = match (
        signal.output_profile(),
        binding.signal_output_profile(),
        binding.transform_release(),
    ) {
        (
            OutputProfileId::Iec61966Srgb8D65V1,
            OutputProfileId::Iec61966Srgb8D65V1,
            ColorimetricTransformReleaseId::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1,
        ) => xyz_d65_from_srgb8_v1(signal.srgb8()),
    };
    TristimulusSample::try_from_registered_xyz(xyz, binding.result_frame())
}

#[cfg(test)]
thread_local! {
    /// Per-thread count of modeled signal-to-tristimulus derivations. Program
    /// regression tests use this deterministic metric to pin one derivation
    /// per unique target occurrence and physical case without timing noise.
    pub(crate) static MODELED_TRISTIMULUS_DERIVATION_CALLS: std::cell::Cell<u64> =
        const { std::cell::Cell::new(0) };
}

/// Derive one modeled tristimulus from an encoded sRGB8 point.
///
/// Composition provenance remains the responsibility of the upstream
/// render/composition layer that produced the signal; renderer capability and
/// actual observations are deliberately outside this deterministic transform.
pub(crate) fn derive_modeled_tristimulus_v1(
    signal: ColorSignal,
) -> Result<ModeledTristimulusDerivationV1, TristimulusDomainErrorV1> {
    #[cfg(test)]
    MODELED_TRISTIMULUS_DERIVATION_CALLS.with(|calls| calls.set(calls.get() + 1));
    let binding = admitted_binding(signal.output_profile());
    let sample = derive_sample_with_binding(signal, binding)?;
    Ok(ModeledTristimulusDerivationV1 {
        sample,
        provenance: ModeledTristimulusProvenanceV1 {
            source_signal: signal,
            binding,
        },
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppearanceContextSchemaReleaseId {
    Ciecam16ViewingInputsV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SurroundProfileId {
    AverageV1,
    DimV1,
    DarkV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppearanceContextFieldV1 {
    AdaptingLuminanceCdM2,
    BackgroundLuminanceRatio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppearanceContextDomainErrorV1 {
    field: AppearanceContextFieldV1,
    reason: NumericDomainError,
}

impl AppearanceContextDomainErrorV1 {
    pub(crate) const fn field(self) -> AppearanceContextFieldV1 {
        self.field
    }

    pub(crate) const fn reason(self) -> NumericDomainError {
        self.reason
    }
}

/// Finite, strictly positive CIECAM16 adapting luminance in cd/m².
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AdaptingLuminanceCdM2(FiniteNonNegative);

impl AdaptingLuminanceCdM2 {
    pub(crate) fn try_new(value: f64) -> Result<Self, AppearanceContextDomainErrorV1> {
        let field = AppearanceContextFieldV1::AdaptingLuminanceCdM2;
        let value = FiniteNonNegative::new(value)
            .map_err(|reason| AppearanceContextDomainErrorV1 { field, reason })?;
        if value.get() == 0.0 {
            return Err(AppearanceContextDomainErrorV1 {
                field,
                reason: NumericDomainError::NotPositive,
            });
        }
        Ok(Self(value))
    }

    pub(crate) fn get(self) -> f64 {
        self.0.get()
    }
}

/// Finite CIECAM16 background ratio `Y_b / Y_w` in `(0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BackgroundLuminanceRatio(FiniteNonNegative);

impl BackgroundLuminanceRatio {
    pub(crate) fn try_new(value: f64) -> Result<Self, AppearanceContextDomainErrorV1> {
        let field = AppearanceContextFieldV1::BackgroundLuminanceRatio;
        let value = FiniteNonNegative::new(value)
            .map_err(|reason| AppearanceContextDomainErrorV1 { field, reason })?;
        if value.get() == 0.0 {
            return Err(AppearanceContextDomainErrorV1 {
                field,
                reason: NumericDomainError::NotPositive,
            });
        }
        if value.get() > 1.0 {
            return Err(AppearanceContextDomainErrorV1 {
                field,
                reason: NumericDomainError::AboveOne,
            });
        }
        Ok(Self(value))
    }

    pub(crate) fn get(self) -> f64 {
        self.0.get()
    }
}

/// Content identity of immutable semantic viewing inputs.
///
/// Derived CAM constants are intentionally absent and must remain a private
/// cache of the appearance-view implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppearanceContextId {
    schema_release: AppearanceContextSchemaReleaseId,
    frame: ColorimetricFrameId,
    adapting_luminance_cd_m2: AdaptingLuminanceCdM2,
    background_luminance_ratio: BackgroundLuminanceRatio,
    surround: SurroundProfileId,
}

impl AppearanceContextId {
    pub(crate) const fn from_inputs(
        schema_release: AppearanceContextSchemaReleaseId,
        frame: ColorimetricFrameId,
        adapting_luminance_cd_m2: AdaptingLuminanceCdM2,
        background_luminance_ratio: BackgroundLuminanceRatio,
        surround: SurroundProfileId,
    ) -> Self {
        Self {
            schema_release,
            frame,
            adapting_luminance_cd_m2,
            background_luminance_ratio,
            surround,
        }
    }

    pub(crate) const fn schema_release(self) -> AppearanceContextSchemaReleaseId {
        self.schema_release
    }

    pub(crate) const fn frame(self) -> ColorimetricFrameId {
        self.frame
    }

    pub(crate) fn adapting_luminance_cd_m2(self) -> f64 {
        self.adapting_luminance_cd_m2.get()
    }

    pub(crate) fn background_luminance_ratio(self) -> f64 {
        self.background_luminance_ratio.get()
    }

    pub(crate) const fn surround_profile(self) -> SurroundProfileId {
        self.surround
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
pub enum OccurrenceFormationError {
    FrameMismatch {
        stimulus: ColorimetricFrameId,
        context: ColorimetricFrameId,
    },
}

/// LCS identity: one tristimulus sample bound to one immutable context.
///
/// Whether the sample is modeled, renderer-observed or measured belongs to its
/// external provenance; forming this identity does not upgrade that claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LcsOccurrence {
    sample: TristimulusSample,
    context: AppearanceContextId,
}

impl LcsOccurrence {
    /// Bind one sample to one context with an exactly matching frame.
    ///
    /// Named appearance views are derived later from this pair. No view
    /// coordinate is accepted here, so contradictory cached views cannot become
    /// part of occurrence identity.
    pub(crate) fn in_context(
        sample: TristimulusSample,
        context: AppearanceContextId,
    ) -> Result<Self, OccurrenceFormationError> {
        if sample.frame() != context.frame() {
            return Err(OccurrenceFormationError::FrameMismatch {
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

/// Failure while binding replayable modeled provenance to one context-bound
/// occurrence. Every mismatch is explicit; no convenient encoded signal may be
/// attached to unrelated XYZ/context identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeledLcsOccurrenceFormationErrorV1 {
    Tristimulus(TristimulusDomainErrorV1),
    Formation(OccurrenceFormationError),
    ProvenanceReplayFailed(TristimulusDomainErrorV1),
    RecordedSampleDoesNotReplay {
        recorded: TristimulusSample,
        replayed: TristimulusSample,
    },
    OccurrenceSampleMismatch {
        occurrence: TristimulusSample,
        modeled: TristimulusSample,
    },
}

/// One replayable modeled signal derivation bound to exactly one immutable
/// appearance context.
///
/// The provenance is retained beside the LCS identity: a bare XYZ triple or a
/// derived appearance coordinate can never impersonate the encoded signal
/// which produced this occurrence. This value is still a deterministic model,
/// not evidence that a renderer or observer produced the stimulus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeledLcsOccurrenceV1 {
    derivation: ModeledTristimulusDerivationV1,
    occurrence: LcsOccurrence,
}

impl ModeledLcsOccurrenceV1 {
    pub(crate) fn from_signal_in_context(
        signal: ColorSignal,
        context: AppearanceContextId,
    ) -> Result<Self, ModeledLcsOccurrenceFormationErrorV1> {
        let derivation = derive_modeled_tristimulus_v1(signal)
            .map_err(ModeledLcsOccurrenceFormationErrorV1::Tristimulus)?;
        let occurrence = LcsOccurrence::in_context(derivation.sample(), context)
            .map_err(ModeledLcsOccurrenceFormationErrorV1::Formation)?;
        // Both values are formed in this function from the same admitted
        // sample. Replaying the transform here would derive sRGB -> XYZ twice
        // on the Program hot path; replay remains mandatory for `bind`, whose
        // two pre-existing inputs may be unrelated.
        Ok(Self {
            derivation,
            occurrence,
        })
    }

    pub(crate) fn bind(
        occurrence: LcsOccurrence,
        derivation: ModeledTristimulusDerivationV1,
    ) -> Result<Self, ModeledLcsOccurrenceFormationErrorV1> {
        let modeled = Self {
            derivation,
            occurrence,
        };
        modeled.verify()?;
        Ok(modeled)
    }

    pub(crate) fn verify(self) -> Result<(), ModeledLcsOccurrenceFormationErrorV1> {
        let replayed = self
            .derivation
            .replay()
            .map_err(ModeledLcsOccurrenceFormationErrorV1::ProvenanceReplayFailed)?;
        let recorded = self.derivation.sample();
        if replayed != recorded {
            return Err(
                ModeledLcsOccurrenceFormationErrorV1::RecordedSampleDoesNotReplay {
                    recorded,
                    replayed,
                },
            );
        }
        let occurrence = self.occurrence.sample();
        if occurrence != recorded {
            return Err(
                ModeledLcsOccurrenceFormationErrorV1::OccurrenceSampleMismatch {
                    occurrence,
                    modeled: recorded,
                },
            );
        }
        Ok(())
    }

    pub(crate) const fn derivation(self) -> ModeledTristimulusDerivationV1 {
        self.derivation
    }

    pub(crate) const fn occurrence(self) -> LcsOccurrence {
        self.occurrence
    }

    pub(crate) const fn provenance(self) -> ModeledTristimulusProvenanceV1 {
        self.derivation.provenance()
    }

    pub(crate) const fn signal(self) -> ColorSignal {
        self.provenance().source_signal()
    }
}

/// Formula and operation-order release of the rectangular Oklab view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OklabViewReleaseId {
    Ottosson20210125XyzD65V1,
}

/// Formula and operation-order release of the context-dependent CAM16 view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Cam16ViewReleaseId {
    LiEtAl2017Cie248ForwardV1,
}

/// Typed release discriminator used only to qualify derivation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppearanceViewReleaseIdV1 {
    Oklab(OklabViewReleaseId),
    Cam16(Cam16ViewReleaseId),
}

pub(crate) const OKLAB_VIEW_RELEASE_V1: OklabViewReleaseId =
    OklabViewReleaseId::Ottosson20210125XyzD65V1;
pub(crate) const CAM16_VIEW_RELEASE_V1: Cam16ViewReleaseId =
    Cam16ViewReleaseId::LiEtAl2017Cie248ForwardV1;

/// Finite binary64 coordinate with canonical positive zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct FiniteCoordinate(u64);

impl FiniteCoordinate {
    fn new(value: f64) -> Result<Self, NumericDomainError> {
        if !value.is_finite() {
            return Err(NumericDomainError::NonFinite);
        }
        Ok(Self(if value == 0.0 {
            0.0_f64.to_bits()
        } else {
            value.to_bits()
        }))
    }

    fn get(self) -> f64 {
        f64::from_bits(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppearanceViewFieldV1 {
    OklabL,
    OklabA,
    OklabB,
    Cam16J,
    Cam16Q,
    Cam16C,
    Cam16M,
    Cam16S,
    Cam16Hue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppearanceStateDerivationErrorV1 {
    UnsupportedFrame {
        frame: ColorimetricFrameId,
    },
    NumericDomain {
        release: AppearanceViewReleaseIdV1,
        field: AppearanceViewFieldV1,
        reason: NumericDomainError,
    },
}

/// Rectangular Oklab geometry of one admitted XYZ(D65) stimulus.
///
/// It deliberately has no hue, context-dependent correlate, inverse or setter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OklabViewV1 {
    release: OklabViewReleaseId,
    l: FiniteCoordinate,
    a: FiniteCoordinate,
    b: FiniteCoordinate,
}

impl OklabViewV1 {
    pub(crate) const fn release(self) -> OklabViewReleaseId {
        self.release
    }

    pub(crate) fn l(self) -> f64 {
        self.l.get()
    }

    pub(crate) fn a(self) -> f64 {
        self.a.get()
    }

    pub(crate) fn b(self) -> f64 {
        self.b.get()
    }
}

/// CAM16 appearance correlates of one occurrence under its own context.
///
/// This view is not CAM16-UCS, a difference calibration or a rendering claim.
/// Its hue is the CAM16 angular correlate only; no Oklab direction enters it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cam16ViewV1 {
    release: Cam16ViewReleaseId,
    j: FiniteNonNegative,
    q: FiniteNonNegative,
    c: FiniteNonNegative,
    m: FiniteNonNegative,
    s: FiniteNonNegative,
    hue: HueState,
}

impl Cam16ViewV1 {
    pub(crate) const fn release(self) -> Cam16ViewReleaseId {
        self.release
    }

    pub(crate) fn j(self) -> f64 {
        self.j.get()
    }

    pub(crate) fn q(self) -> f64 {
        self.q.get()
    }

    pub(crate) fn c(self) -> f64 {
        self.c.get()
    }

    pub(crate) fn m(self) -> f64 {
        self.m.get()
    }

    pub(crate) fn s(self) -> f64 {
        self.s.get()
    }

    pub(crate) const fn hue(self) -> HueState {
        self.hue
    }
}

/// One-way, derived appearance snapshot of exactly one occurrence.
///
/// Canonical LCS identity remains [`LcsOccurrence`] (`sample × context`). This
/// type is only a deterministic cache of separately named views and cannot be
/// constructed from, edited through or inverted from view coordinates.
#[derive(Debug, Clone, Copy)]
pub struct AppearanceState {
    occurrence: LcsOccurrence,
    oklab: OklabViewV1,
    cam16: Cam16ViewV1,
}

impl AppearanceState {
    pub(crate) fn derive_v1(
        occurrence: LcsOccurrence,
    ) -> Result<Self, AppearanceStateDerivationErrorV1> {
        if occurrence.sample().frame() != IEC_SRGB_D65_XYZ_FRAME_V1 {
            return Err(AppearanceStateDerivationErrorV1::UnsupportedFrame {
                frame: occurrence.sample().frame(),
            });
        }

        let oklab = derive_oklab_view_v1(occurrence.sample().xyz())?;
        let cam16 = derive_cam16_view_v1(occurrence)?;
        Ok(Self {
            occurrence,
            oklab,
            cam16,
        })
    }

    pub(crate) const fn occurrence(self) -> LcsOccurrence {
        self.occurrence
    }

    pub(crate) const fn oklab(self) -> OklabViewV1 {
        self.oklab
    }

    pub(crate) const fn cam16(self) -> Cam16ViewV1 {
        self.cam16
    }
}

fn view_numeric_error(
    release: AppearanceViewReleaseIdV1,
    field: AppearanceViewFieldV1,
    reason: NumericDomainError,
) -> AppearanceStateDerivationErrorV1 {
    AppearanceStateDerivationErrorV1::NumericDomain {
        release,
        field,
        reason,
    }
}

pub(crate) fn derive_oklab_view_v1(
    xyz: [f64; 3],
) -> Result<OklabViewV1, AppearanceStateDerivationErrorV1> {
    let [l, a, b] = xyz_d65_to_oklab_v1(xyz);
    let release = AppearanceViewReleaseIdV1::Oklab(OKLAB_VIEW_RELEASE_V1);
    Ok(OklabViewV1 {
        release: OKLAB_VIEW_RELEASE_V1,
        l: FiniteCoordinate::new(l)
            .map_err(|reason| view_numeric_error(release, AppearanceViewFieldV1::OklabL, reason))?,
        a: FiniteCoordinate::new(a)
            .map_err(|reason| view_numeric_error(release, AppearanceViewFieldV1::OklabA, reason))?,
        b: FiniteCoordinate::new(b)
            .map_err(|reason| view_numeric_error(release, AppearanceViewFieldV1::OklabB, reason))?,
    })
}

fn derive_cam16_view_v1(
    occurrence: LcsOccurrence,
) -> Result<Cam16ViewV1, AppearanceStateDerivationErrorV1> {
    let context = occurrence.context();
    let surround = match context.surround_profile() {
        SurroundProfileId::AverageV1 => Cam16SurroundV1::Average,
        SurroundProfileId::DimV1 => Cam16SurroundV1::Dim,
        SurroundProfileId::DarkV1 => Cam16SurroundV1::Dark,
    };
    let vc = match context.schema_release() {
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1 => {
            ViewingConditions::from_semantic_inputs_v1(
                context.adapting_luminance_cd_m2(),
                context.background_luminance_ratio(),
                surround,
            )
        }
    };
    let coordinates = forward_correlates_v1(occurrence.sample().xyz(), &vc);
    let release = AppearanceViewReleaseIdV1::Cam16(CAM16_VIEW_RELEASE_V1);
    let admit = |value, field| {
        FiniteNonNegative::new(value).map_err(|reason| view_numeric_error(release, field, reason))
    };
    let j = admit(coordinates.j, AppearanceViewFieldV1::Cam16J)?;
    let q = admit(coordinates.q, AppearanceViewFieldV1::Cam16Q)?;
    let c = admit(coordinates.c, AppearanceViewFieldV1::Cam16C)?;
    let m = admit(coordinates.m, AppearanceViewFieldV1::Cam16M)?;
    let s = admit(coordinates.s, AppearanceViewFieldV1::Cam16S)?;
    let hue = if m.get() == 0.0 {
        HueState::UndefinedExact
    } else {
        HueState::Defined(HueAngle::new(coordinates.h).map_err(|reason| {
            view_numeric_error(release, AppearanceViewFieldV1::Cam16Hue, reason)
        })?)
    };
    Ok(Cam16ViewV1 {
        release: CAM16_VIEW_RELEASE_V1,
        j,
        q,
        c,
        m,
        s,
        hue,
    })
}
