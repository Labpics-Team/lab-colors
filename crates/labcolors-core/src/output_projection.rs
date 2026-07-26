//! Private, release-bound output projection from one modeled LCS occurrence.
//!
//! This module deliberately admits one narrow edge only:
//!
//! ```text
//! modeled IEC sRGB8 signal
//!     -> replayed tristimulus
//!     -> the same context-bound LCS occurrence
//!     -> solid CSS Color 4 `oklch(...)` + replayable certificate
//! ```
//!
//! An output projection is neither an appearance view nor a pairwise
//! difference calibration.  The nominal release identifiers below therefore
//! cannot be substituted for one another.  No alpha/composition, inverse,
//! output-gamut transform, P3 path or perceptual metric is admitted by this
//! slice.

use crate::lcs_occurrence::{
    AppearanceStateDerivationErrorV1, ColorSignal, HueAngle, HueState, LcsOccurrence,
    ModeledLcsOccurrenceFormationErrorV1, ModeledLcsOccurrenceV1, ModeledTristimulusProvenanceV1,
    NumericDomainError, OKLAB_VIEW_RELEASE_V1, OklabViewReleaseId, derive_oklab_view_v1,
};

/// Formula, policy and operation-order identity of an output projection.
///
/// The source qualifier is intentional: this release can re-express only an
/// occurrence whose exact modeled IEC sRGB8 provenance is supplied and
/// replayed.  It does not claim an inverse rendering solution for arbitrary
/// XYZ occurrences.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum OutputProjectionReleaseIdV1 {
    CssColor4OklchD65FromModeledIec61966Srgb8SolidV1,
}

impl OutputProjectionReleaseIdV1 {
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::CssColor4OklchD65FromModeledIec61966Srgb8SolidV1 => {
                "css-color-4-oklch-d65-from-modeled-iec61966-srgb8-solid-v1"
            }
        }
    }
}

pub(crate) const CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1: OutputProjectionReleaseIdV1 =
    OutputProjectionReleaseIdV1::CssColor4OklchD65FromModeledIec61966Srgb8SolidV1;

/// No pairwise difference calibration is admitted by #441-A.
///
/// Keeping this as a nominal, uninhabited type makes absence executable: code
/// cannot mint a distance result, alias an unrelated selector or pass
/// an output/appearance release where a calibration release is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum DifferenceCalibrationReleaseIdV1 {}

impl DifferenceCalibrationReleaseIdV1 {
    #[allow(dead_code)]
    pub(crate) const fn key(self) -> &'static str {
        match self {}
    }
}

/// Polar view derived from the registered rectangular Oklab appearance view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum OklchViewReleaseId {
    PolarFromOttosson20210125OklabV1,
}

impl OklchViewReleaseId {
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::PolarFromOttosson20210125OklabV1 => "polar-from-ottosson-2021-01-25-oklab-v1",
        }
    }
}

pub(crate) const OKLCH_VIEW_RELEASE_V1: OklchViewReleaseId =
    OklchViewReleaseId::PolarFromOttosson20210125OklabV1;

/// Finite binary64 appearance coordinate with canonical positive zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct FiniteOklchCoordinateV1(u64);

impl FiniteOklchCoordinateV1 {
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

/// Immutable polar Oklch view of the output projection's admitted occurrence.
///
/// `UndefinedExact` remains a distinct appearance state.  Mapping it to a CSS
/// numeric token happens later under
/// [`CssOklchHueSerializationReleaseIdV1`], never inside this view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct OklchViewV1 {
    release: OklchViewReleaseId,
    l: FiniteOklchCoordinateV1,
    c: FiniteOklchCoordinateV1,
    hue: HueState,
}

impl OklchViewV1 {
    pub(crate) const fn release(self) -> OklchViewReleaseId {
        self.release
    }

    pub(crate) fn l(self) -> f64 {
        self.l.get()
    }

    pub(crate) fn c(self) -> f64 {
        self.c.get()
    }

    pub(crate) const fn hue(self) -> HueState {
        self.hue
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum OklchViewFieldV1 {
    OklabL,
    OklabA,
    OklabB,
    OklchChroma,
    OklchHue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OklchViewDerivationErrorV1 {
    release: OklchViewReleaseId,
    field: OklchViewFieldV1,
    reason: NumericDomainError,
}

impl OklchViewDerivationErrorV1 {
    pub(crate) const fn release(self) -> OklchViewReleaseId {
        self.release
    }

    pub(crate) const fn field(self) -> OklchViewFieldV1 {
        self.field
    }

    pub(crate) const fn reason(self) -> NumericDomainError {
        self.reason
    }
}

fn oklch_view_error(
    field: OklchViewFieldV1,
    reason: NumericDomainError,
) -> OklchViewDerivationErrorV1 {
    OklchViewDerivationErrorV1 {
        release: OKLCH_VIEW_RELEASE_V1,
        field,
        reason,
    }
}

/// Derive the named polar view before any CSS serialization policy runs.
///
/// Hue is derived solely from the rectangular coordinates.  Encoded-source
/// facts belong to serialization policy and cannot change this view.
fn derive_oklch_view_v1(oklab: [f64; 3]) -> Result<OklchViewV1, OklchViewDerivationErrorV1> {
    let [l, a, b] = oklab;
    let l = FiniteOklchCoordinateV1::new(l)
        .map_err(|reason| oklch_view_error(OklchViewFieldV1::OklabL, reason))?;
    FiniteOklchCoordinateV1::new(a)
        .map_err(|reason| oklch_view_error(OklchViewFieldV1::OklabA, reason))?;
    FiniteOklchCoordinateV1::new(b)
        .map_err(|reason| oklch_view_error(OklchViewFieldV1::OklabB, reason))?;
    let c = FiniteOklchCoordinateV1::new(a.hypot(b))
        .map_err(|reason| oklch_view_error(OklchViewFieldV1::OklchChroma, reason))?;
    let hue = if a == 0.0 && b == 0.0 {
        HueState::UndefinedExact
    } else {
        let degrees = b.atan2(a).to_degrees();
        let canonical = if degrees < 0.0 {
            degrees + 360.0
        } else {
            degrees
        };
        HueState::Defined(
            HueAngle::new(canonical)
                .map_err(|reason| oklch_view_error(OklchViewFieldV1::OklchHue, reason))?,
        )
    };
    Ok(OklchViewV1 {
        release: OKLCH_VIEW_RELEASE_V1,
        l,
        c,
        hue,
    })
}

/// Exact decimal serialization policy carried by the projection certificate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum CssOklchNumberEncodingReleaseIdV1 {
    LPercent5C6Hue3V1,
}

/// Hue serialization is output syntax, not occurrence hue identity.
///
/// Exact encoded greys and an exact rectangular Oklab origin serialize as the
/// harmless numeric CSS convention `0`.  This never changes an appearance
/// view's [`crate::lcs_occurrence::HueState`] to `Defined(0°)` and introduces no
/// tolerance or perceptual-achromaticity threshold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum CssOklchHueSerializationReleaseIdV1 {
    ExactSourceGreyOrRectangularOriginToZeroV1,
}

/// This projection release performs no explicit output-gamut mapping step.
///
/// The name deliberately makes no identity or round-trip claim about a later
/// inverse conversion, quantizer or host renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum OutputGamutTreatmentV1 {
    NoExplicitProjectionGamutMapV1,
}

/// The release choice is made before any coordinates or output bytes exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OutputProjectionRequestV1 {
    source: ModeledLcsOccurrenceV1,
    release: OutputProjectionReleaseIdV1,
}

impl OutputProjectionRequestV1 {
    pub(crate) const fn new(
        source: ModeledLcsOccurrenceV1,
        release: OutputProjectionReleaseIdV1,
    ) -> Self {
        Self { source, release }
    }

    pub(crate) const fn source(self) -> ModeledLcsOccurrenceV1 {
        self.source
    }

    pub(crate) const fn release(self) -> OutputProjectionReleaseIdV1 {
        self.release
    }
}

/// A solid CSS Color 4 value.  There is intentionally no alpha field or
/// constructor accepting opacity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CssColor4OklchD65SolidV1(String);

impl CssColor4OklchD65SolidV1 {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum OutputProjectionFieldV1 {
    OklchLightness,
    OklchHueState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputProjectionNumericErrorV1 {
    LightnessOutsideSourceDomain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputProjectionErrorV1 {
    Source(ModeledLcsOccurrenceFormationErrorV1),
    OklabView(AppearanceStateDerivationErrorV1),
    OklchView(OklchViewDerivationErrorV1),
    Numeric {
        release: OutputProjectionReleaseIdV1,
        field: OutputProjectionFieldV1,
        reason: OutputProjectionNumericErrorV1,
    },
    UnsupportedHueState {
        release: OutputProjectionReleaseIdV1,
        field: OutputProjectionFieldV1,
        hue: HueState,
    },
}

/// All inputs and versioned policies needed to replay one projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OutputProjectionCertificateV1 {
    source: ModeledLcsOccurrenceV1,
    release: OutputProjectionReleaseIdV1,
    oklab_release: OklabViewReleaseId,
    oklch_release: OklchViewReleaseId,
    number_encoding: CssOklchNumberEncodingReleaseIdV1,
    hue_serialization: CssOklchHueSerializationReleaseIdV1,
    gamut_treatment: OutputGamutTreatmentV1,
}

impl OutputProjectionCertificateV1 {
    pub(crate) const fn source(self) -> ModeledLcsOccurrenceV1 {
        self.source
    }

    pub(crate) const fn source_occurrence(self) -> LcsOccurrence {
        self.source.occurrence()
    }

    pub(crate) const fn source_provenance(self) -> ModeledTristimulusProvenanceV1 {
        self.source.provenance()
    }

    pub(crate) const fn source_signal(self) -> ColorSignal {
        self.source.signal()
    }

    pub(crate) const fn release(self) -> OutputProjectionReleaseIdV1 {
        self.release
    }

    pub(crate) const fn oklab_release(self) -> OklabViewReleaseId {
        self.oklab_release
    }

    pub(crate) const fn oklch_release(self) -> OklchViewReleaseId {
        self.oklch_release
    }

    pub(crate) const fn number_encoding(self) -> CssOklchNumberEncodingReleaseIdV1 {
        self.number_encoding
    }

    pub(crate) const fn hue_serialization(self) -> CssOklchHueSerializationReleaseIdV1 {
        self.hue_serialization
    }

    pub(crate) const fn gamut_treatment(self) -> OutputGamutTreatmentV1 {
        self.gamut_treatment
    }

    /// Re-run source provenance, view math and serialization under the same
    /// release.  Equality with the certified value is the byte replay check.
    pub(crate) fn replay(self) -> Result<OutputProjectionV1, OutputProjectionErrorV1> {
        project_output_v1(OutputProjectionRequestV1::new(self.source, self.release))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutputProjectionV1 {
    oklch_view: OklchViewV1,
    value: CssColor4OklchD65SolidV1,
    certificate: OutputProjectionCertificateV1,
}

impl OutputProjectionV1 {
    pub(crate) const fn oklch_view(&self) -> OklchViewV1 {
        self.oklch_view
    }

    pub(crate) fn value(&self) -> &CssColor4OklchD65SolidV1 {
        &self.value
    }

    pub(crate) const fn certificate(&self) -> OutputProjectionCertificateV1 {
        self.certificate
    }
}

fn numeric_error(
    release: OutputProjectionReleaseIdV1,
    field: OutputProjectionFieldV1,
    reason: OutputProjectionNumericErrorV1,
) -> OutputProjectionErrorV1 {
    OutputProjectionErrorV1::Numeric {
        release,
        field,
        reason,
    }
}

fn project_css_color4_oklch_d65_from_modeled_srgb8_solid_v1(
    source: ModeledLcsOccurrenceV1,
    release: OutputProjectionReleaseIdV1,
) -> Result<OutputProjectionV1, OutputProjectionErrorV1> {
    source.verify().map_err(OutputProjectionErrorV1::Source)?;

    // The occurrence sample, not the encoded source channels, owns appearance
    // geometry.  Provenance is used only to prove that this narrow output
    // release may serialize the occurrence under its registered policies.
    let oklab = derive_oklab_view_v1(source.occurrence().sample().xyz())
        .map_err(OutputProjectionErrorV1::OklabView)?;
    let source_srgb8 = source.signal().srgb8();
    let oklch_view = derive_oklch_view_v1([oklab.l(), oklab.a(), oklab.b()])
        .map_err(OutputProjectionErrorV1::OklchView)?;
    let l = oklch_view.l();
    if !(0.0..=1.0).contains(&l) {
        return Err(numeric_error(
            release,
            OutputProjectionFieldV1::OklchLightness,
            OutputProjectionNumericErrorV1::LightnessOutsideSourceDomain,
        ));
    }

    let hue_degrees = if source_srgb8.is_achromatic() {
        0.0
    } else {
        match oklch_view.hue() {
            HueState::Defined(angle) => angle.degrees(),
            HueState::UndefinedExact => 0.0,
            hue @ HueState::PowerlessBy(_) => {
                return Err(OutputProjectionErrorV1::UnsupportedHueState {
                    release,
                    field: OutputProjectionFieldV1::OklchHueState,
                    hue,
                });
            }
        }
    };

    let value = CssColor4OklchD65SolidV1(format!(
        "oklch({:.5}% {:.6} {:.3})",
        l * 100.0,
        oklch_view.c(),
        hue_degrees,
    ));
    let certificate = OutputProjectionCertificateV1 {
        source,
        release,
        oklab_release: OKLAB_VIEW_RELEASE_V1,
        oklch_release: oklch_view.release(),
        number_encoding: CssOklchNumberEncodingReleaseIdV1::LPercent5C6Hue3V1,
        hue_serialization:
            CssOklchHueSerializationReleaseIdV1::ExactSourceGreyOrRectangularOriginToZeroV1,
        gamut_treatment: OutputGamutTreatmentV1::NoExplicitProjectionGamutMapV1,
    };
    Ok(OutputProjectionV1 {
        oklch_view,
        value,
        certificate,
    })
}

/// Execute exactly the release selected in the request.  There is no
/// result-dependent release selection or fallback.
pub(crate) fn project_output_v1(
    request: OutputProjectionRequestV1,
) -> Result<OutputProjectionV1, OutputProjectionErrorV1> {
    match request.release() {
        OutputProjectionReleaseIdV1::CssColor4OklchD65FromModeledIec61966Srgb8SolidV1 => {
            project_css_color4_oklch_d65_from_modeled_srgb8_solid_v1(
                request.source(),
                request.release(),
            )
        }
    }
}
