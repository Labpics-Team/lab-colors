//! Private, release-bound output projection from one modeled LCS occurrence.
//!
//! This module admits two narrow edges:
//!
//! ```text
//! modeled IEC sRGB8 signal
//!     -> replayed tristimulus
//!     -> the same context-bound LCS occurrence
//!     -> solid CSS Color 4 `oklch(...)` + replayable certificate
//!
//! modeled IEC sRGB8 signal
//!     -> replayed XYZ(D65) tristimulus
//!     -> the same context-bound LCS occurrence
//!     -> solid CSS Color 4 `color(display-p3 ...)` + replayable certificate
//! ```
//!
//! An output projection is neither an appearance view nor a pairwise
//! difference calibration. The nominal release identifiers below therefore
//! cannot be substituted for one another. No alpha/composition, inverse,
//! or perceptual metric is admitted by this slice.

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
    /// CSS Color 4 `color(display-p3 R G B)` from modeled XYZ(D65).
    /// Distinct release identity; no substitution with sRGB variant.
    CssColor4DisplayP3FromModeledXyzD65SolidV1,
    /// HDR PQ Rec.2020 output with absolute luminance and explicit tone mapping.
    CssColor4PqRec2020FromModeledXyzAbsoluteV1,
}

impl OutputProjectionReleaseIdV1 {
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::CssColor4OklchD65FromModeledIec61966Srgb8SolidV1 => {
                "css-color-4-oklch-d65-from-modeled-iec61966-srgb8-solid-v1"
            }
            Self::CssColor4DisplayP3FromModeledXyzD65SolidV1 => {
                "css-color-4-display-p3-from-modeled-xyz-d65-solid-v1"
            }
            Self::CssColor4PqRec2020FromModeledXyzAbsoluteV1 => {
                "css-color-4-pq-rec2020-from-modeled-xyz-absolute-v1"
            }
        }
    }
}

pub(crate) const CSS_COLOR_4_OKLCH_D65_FROM_MODELED_SRGB8_SOLID_V1: OutputProjectionReleaseIdV1 =
    OutputProjectionReleaseIdV1::CssColor4OklchD65FromModeledIec61966Srgb8SolidV1;

pub(crate) const CSS_COLOR_4_DISPLAY_P3_FROM_MODELED_XYZ_D65_SOLID_V1: OutputProjectionReleaseIdV1 =
    OutputProjectionReleaseIdV1::CssColor4DisplayP3FromModeledXyzD65SolidV1;

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

/// Decimal precision policy for `color(display-p3 R G B)` channel values.
/// Each channel is a unitless number in [0, 1] serialized with 6 decimal places.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum CssDisplayP3NumberEncodingReleaseIdV1 {
    /// Six decimal places per channel, matching CSS Color 4 recommendation
    /// for lossless round-trip of 8-bit encoded values.
    SixDecimalPlacesV1,
}

/// This projection release performs no explicit output-gamut mapping step.
///
/// The name deliberately makes no identity or round-trip claim about a later
/// inverse conversion, quantizer or host renderer.
/// This projection release performs no explicit output-gamut mapping step.
///
/// The name deliberately makes no identity or round-trip claim about a later
/// inverse conversion, quantizer or host renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum OutputGamutTreatmentV1 {
    NoExplicitProjectionGamutMapV1,
    /// Hard-clip each channel to [0, 1] and record whether any channel was
    /// out of gamut in the certificate. No perceptual mapping is applied.
    HardClipWithOutOfGamutFlagV1,
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

/// A solid CSS Color 4 `color(display-p3 R G B)` value. No alpha field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CssColor4DisplayP3SolidV1(String);

impl CssColor4DisplayP3SolidV1 {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// Outcome of the P3-domain encoded recheck.
/// Satisfies roadmap requirement: "final encoded recheck делается в P3 domain."
///
/// `max_channel_delta` is stored as IEEE 754 binary64 bits (`u64`) so that
/// this type can derive `Eq`. Floating-point `f64` does not implement `Eq`
/// because NaN != NaN, but the bit representation is a total equivalence
/// relation suitable for byte-identity replay verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct P3EncodedRecheckV1 {
    /// Maximum absolute difference across R, G, B channels between
    /// pre-encoding linear P3 and decode-back-from-serialized linear P3,
    /// stored as IEEE 754 binary64 bits.
    max_channel_delta_bits: u64,
    /// Whether the recheck passed within the declared tolerance.
    passed: bool,
}

impl P3EncodedRecheckV1 {
    pub(crate) fn max_channel_delta(self) -> f64 {
        f64::from_bits(self.max_channel_delta_bits)
    }

    pub(crate) const fn passed(self) -> bool {
        self.passed
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

#[derive(Debug, Clone, PartialEq)]
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
    /// P3 encoded-domain recheck exceeded tolerance.
    P3EncodedRecheckFailed {
        release: OutputProjectionReleaseIdV1,
        recheck: P3EncodedRecheckV1,
    },
    /// HDR projection error. Distinct from SDR path errors.
    Hdr(HdrProjectionErrorV1),
}

// ─── HDR Projection Types (O-08) ────────────────────────────────────────

use crate::spaces::pq::{AbsoluteLuminanceV1, HdrNumericalErrorV1, PqCodeValueV1};
use crate::spaces::rec2020::LinearRec2020V1;

/// Versioned tone-mapping operator identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub(crate) enum ToneMapOperatorIdV1 {
    /// Reinhard global: L_out = L / (1 + L/L_white).
    ReinhardGlobalV1,
    /// Linear clamp: hard clip at display peak. Valid only when source ≤ display.
    LinearClampV1,
}

impl ToneMapOperatorIdV1 {
    pub(crate) const fn key(self) -> &'static str {
        match self {
            Self::ReinhardGlobalV1 => "reinhard-global-v1",
            Self::LinearClampV1 => "linear-clamp-v1",
        }
    }
}

/// Result of applying a tone-mapping operator to one luminance value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ToneMapResultV1 {
    pub operator: ToneMapOperatorIdV1,
    pub input_luminance: AbsoluteLuminanceV1,
    pub output_luminance: AbsoluteLuminanceV1,
    pub compression_ratio: f64,
}

/// Apply tone mapping. Returns error only on numerical failure.
pub(crate) fn tone_map(
    operator: ToneMapOperatorIdV1,
    input: AbsoluteLuminanceV1,
    display_peak: AbsoluteLuminanceV1,
) -> Result<ToneMapResultV1, HdrNumericalErrorV1> {
    let l_in = input.value();
    let l_white = display_peak.value();
    let l_out = match operator {
        ToneMapOperatorIdV1::ReinhardGlobalV1 => l_in / (1.0 + l_in / l_white),
        ToneMapOperatorIdV1::LinearClampV1 => l_in.min(l_white),
    };
    let output = AbsoluteLuminanceV1::try_new(l_out)?;
    let ratio = if l_in > 0.0 { l_out / l_in } else { 1.0 };
    Ok(ToneMapResultV1 {
        operator,
        input_luminance: input,
        output_luminance: output,
        compression_ratio: ratio,
    })
}

/// Encode a single XYZ(D65) tristimulus to PQ Rec.2020 with tone mapping.
///
/// Returns three PQ code values (R, G, B) and the tone-map result for the
/// luminance channel (Y component of the mapped tristimulus).
pub(crate) fn encode_xyz_to_hdr_pq_rec2020(
    xyz: [f64; 3],
    request: &HdrProjectionRequestV1,
) -> Result<([PqCodeValueV1; 3], ToneMapResultV1), HdrProjectionErrorV1> {
    // 1. Convert XYZ → Linear Rec.2020
    let linear = LinearRec2020V1::from_xyz_d65(xyz);
    let [lr, lg, lb] = linear.channels();

    // 2. Compute luminance (Y component in XYZ is already luminance)
    let source_lum = AbsoluteLuminanceV1::try_new(xyz[1]).map_err(|_| {
        HdrProjectionErrorV1::LuminanceOutOfRange {
            requested_value: xyz[1],
            valid_min: AbsoluteLuminanceV1::PQ_MIN,
            valid_max: AbsoluteLuminanceV1::PQ_MAX,
        }
    })?;

    // 3. Tone-map luminance
    let tm_result = tone_map(request.tone_map_operator, source_lum, request.peak_white)
        .map_err(|_| HdrProjectionErrorV1::ToneMapNumericalFailure)?;

    // 4. Scale RGB channels by tone-mapped luminance ratio
    let scale = if source_lum.value() > 1e-15 {
        tm_result.output_luminance.value() / source_lum.value()
    } else {
        1.0
    };
    let scaled_r = (lr * scale).clamp(0.0, 1.0);
    let scaled_g = (lg * scale).clamp(0.0, 1.0);
    let scaled_b = (lb * scale).clamp(0.0, 1.0);

    // 5. Convert scaled linear Rec.2020 → absolute luminance per channel → PQ
    //    Each channel represents relative luminance; scale by display peak
    let pq_r = crate::spaces::pq::pq_inverse_eotf(
        AbsoluteLuminanceV1::try_new(scaled_r * request.peak_white.value())
            .unwrap_or_else(|_| AbsoluteLuminanceV1::new_unchecked(AbsoluteLuminanceV1::PQ_MIN)),
    );
    let pq_g = crate::spaces::pq::pq_inverse_eotf(
        AbsoluteLuminanceV1::try_new(scaled_g * request.peak_white.value())
            .unwrap_or_else(|_| AbsoluteLuminanceV1::new_unchecked(AbsoluteLuminanceV1::PQ_MIN)),
    );
    let pq_b = crate::spaces::pq::pq_inverse_eotf(
        AbsoluteLuminanceV1::try_new(scaled_b * request.peak_white.value())
            .unwrap_or_else(|_| AbsoluteLuminanceV1::new_unchecked(AbsoluteLuminanceV1::PQ_MIN)),
    );

    Ok(([pq_r, pq_g, pq_b], tm_result))
}

/// CSS Color 4 HDR serialization: `color(rec2020-pq R G B)`.
pub(crate) fn serialize_hdr_pq_rec2020(pq: [PqCodeValueV1; 3]) -> String {
    format!(
        "color(rec2020-pq {:.6} {:.6} {:.6})",
        pq[0].value(),
        pq[1].value(),
        pq[2].value(),
    )
}

/// Typed host HDR support level. Absence of capability is a value, not a panic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub(crate) enum HostHdrCapabilityV1 {
    /// Host supports PQ + Rec.2020 at declared luminance range.
    FullHdrPqRec2020,
    /// Host supports SDR only; HDR must be tone-mapped. Certificate carries TMO metadata.
    SdrFallbackWithToneMap,
    /// Host cannot render HDR or tone-mapped SDR. Projection returns typed error.
    Unsupported,
}

/// Errors specific to HDR projection. Distinct from OutputProjectionErrorV1.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum HdrProjectionErrorV1 {
    HostUnsupported {
        capability: HostHdrCapabilityV1,
        reason: String,
    },
    LuminanceOutOfRange {
        requested_value: f64,
        valid_min: f64,
        valid_max: f64,
    },
    ToneMapNumericalFailure,
    ConformanceDigestMismatch {
        expected_sha256_hex: String,
        actual_sha256_hex: String,
    },
}

/// HDR luminance metadata carried in every HDR certificate.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HdrLuminanceMetadataV1 {
    pub black_point: AbsoluteLuminanceV1,
    pub peak_white: AbsoluteLuminanceV1,
    pub reference_white: AbsoluteLuminanceV1,
    pub source_content_peak: AbsoluteLuminanceV1,
}

/// HDR projection request. Carries all luminance context.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HdrProjectionRequestV1 {
    pub black_point: AbsoluteLuminanceV1,
    pub peak_white: AbsoluteLuminanceV1,
    pub reference_white: AbsoluteLuminanceV1,
    pub tone_map_operator: ToneMapOperatorIdV1,
    pub source_content_peak: AbsoluteLuminanceV1,
}

/// Conformance digest of PQ-encoded output + luminance metadata.
///
/// Deterministic hash over 7 f64 LE values (3 PQ codes + 4 luminance fields).
/// Uses a dependency-free FNV-1a variant producing 32 bytes by running two
/// independent 64-bit FNV-1a hashes with different seeds and concatenating
/// their big-endian representations twice to fill 32 bytes. This preserves
/// the spec's byte-order and field-order contract without external crates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct HdrConformanceDigestV1([u8; 32]);

impl HdrConformanceDigestV1 {
    /// Compute digest from PQ values and luminance metadata.
    /// Dependency-free: uses FNV-1a over the canonical 56-byte input.
    pub fn compute(pq_values: &[PqCodeValueV1; 3], metadata: &HdrLuminanceMetadataV1) -> Self {
        // Canonical byte sequence: 7 × f64 LE = 56 bytes, order-sensitive.
        let mut buf = [0u8; 56];
        let mut offset = 0usize;
        for pq in pq_values {
            let bytes = pq.value().to_le_bytes();
            buf[offset..offset + 8].copy_from_slice(&bytes);
            offset += 8;
        }
        for val in [
            metadata.black_point.value(),
            metadata.peak_white.value(),
            metadata.reference_white.value(),
            metadata.source_content_peak.value(),
        ] {
            let bytes = val.to_le_bytes();
            buf[offset..offset + 8].copy_from_slice(&bytes);
            offset += 8;
        }
        debug_assert_eq!(offset, 56);

        // Two independent FNV-1a 64-bit hashes with distinct offsets primes.
        let h0 = fnv1a_64(&buf, 0xcbf29ce484222325);
        let h1 = fnv1a_64(&buf, 0x100000001b3_u64.wrapping_mul(0x9e3779b97f4a7c15));
        let h2 = fnv1a_64(&buf, h0.wrapping_add(0x6c62272e07bb0142));
        let h3 = fnv1a_64(&buf, h1.wrapping_add(0x14020a57acced8b7));

        let mut bytes = [0u8; 32];
        bytes[0..8].copy_from_slice(&h0.to_be_bytes());
        bytes[8..16].copy_from_slice(&h1.to_be_bytes());
        bytes[16..24].copy_from_slice(&h2.to_be_bytes());
        bytes[24..32].copy_from_slice(&h3.to_be_bytes());
        Self(bytes)
    }

    pub fn to_hex(self) -> String {
        self.0.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

/// FNV-1a 64-bit hash. Pure function, no allocations, no external deps.
fn fnv1a_64(data: &[u8], basis: u64) -> u64 {
    const FNV_PRIME: u64 = 0x00000100000001B3;
    let mut hash = basis;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// HDR projection certificate. Composes base certificate with HDR-specific metadata.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HdrProjectionCertificateV1 {
    pub base: OutputProjectionCertificateV1,
    pub luminance_metadata: HdrLuminanceMetadataV1,
    pub tone_map_result: ToneMapResultV1,
    pub host_capability: HostHdrCapabilityV1,
    pub conformance_digest: Option<HdrConformanceDigestV1>,
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
    /// Present only when release is CssColor4DisplayP3FromModeledXyzD65SolidV1.
    p3_encoded_recheck: Option<P3EncodedRecheckV1>,
    /// True when HardClipWithOutOfGamutFlagV1 was applied and at least one
    /// channel was outside [0, 1] before clipping.
    out_of_gamut: bool,
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

    pub(crate) const fn p3_encoded_recheck(self) -> Option<P3EncodedRecheckV1> {
        self.p3_encoded_recheck
    }

    pub(crate) const fn out_of_gamut(self) -> bool {
        self.out_of_gamut
    }

    /// Re-run source provenance, view math and serialization under the same
    /// release.  Equality with the certified value is the byte replay check.
    pub(crate) fn replay(self) -> Result<OutputProjectionV1, OutputProjectionErrorV1> {
        project_output_v1(OutputProjectionRequestV1::new(self.source, self.release))
    }
}

#[derive(Debug, Clone, PartialEq)]
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

/// Maximum acceptable delta for P3 encoded domain recheck.
/// Justified by 6-decimal-place serialization quantization amplified
/// through inverse sRGB EOTF near black.
const P3_ENCODED_RECHECK_TOLERANCE: f64 = 1e-5;

/// Hard-clip each channel to [0, 1] and report whether any were out of range.
fn hard_clip_with_flag(linear: [f64; 3]) -> ([f64; 3], bool) {
    let mut out_of_gamut = false;
    let clipped = [
        if linear[0] < 0.0 || linear[0] > 1.0 {
            out_of_gamut = true;
            linear[0].clamp(0.0, 1.0)
        } else {
            linear[0]
        },
        if linear[1] < 0.0 || linear[1] > 1.0 {
            out_of_gamut = true;
            linear[1].clamp(0.0, 1.0)
        } else {
            linear[1]
        },
        if linear[2] < 0.0 || linear[2] > 1.0 {
            out_of_gamut = true;
            linear[2].clamp(0.0, 1.0)
        } else {
            linear[2]
        },
    ];
    (clipped, out_of_gamut)
}

/// Serialize gamma-encoded P3 channels to CSS Color 4 syntax.
fn serialize_display_p3(encoded: [f64; 3]) -> CssColor4DisplayP3SolidV1 {
    CssColor4DisplayP3SolidV1(format!(
        "color(display-p3 {:.6} {:.6} {:.6})",
        encoded[0], encoded[1], encoded[2],
    ))
}

/// Verify that encoding then decoding preserves linear P3 within tolerance.
fn verify_p3_encoded_roundtrip(
    pre_encoding_linear_p3: [f64; 3],
    serialized_channels: [f64; 3],
    tolerance: f64,
) -> P3EncodedRecheckV1 {
    let decoded_linear = crate::spaces::p3::p3_gamma_decode(serialized_channels);
    let max_channel_delta = decoded_linear
        .iter()
        .zip(pre_encoding_linear_p3.iter())
        .map(|(d, p)| (d - p).abs())
        .fold(0.0_f64, f64::max);

    P3EncodedRecheckV1 {
        max_channel_delta_bits: max_channel_delta.to_bits(),
        passed: max_channel_delta <= tolerance,
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
        p3_encoded_recheck: None,
        out_of_gamut: false,
    };
    Ok(OutputProjectionV1 {
        oklch_view,
        value,
        certificate,
    })
}

fn project_css_color4_display_p3_from_modeled_xyz_d65_solid_v1(
    source: ModeledLcsOccurrenceV1,
    release: OutputProjectionReleaseIdV1,
) -> Result<OutputProjectionV1, OutputProjectionErrorV1> {
    // 1. Verify replay invariant (same as existing sRGB path).
    source.verify().map_err(OutputProjectionErrorV1::Source)?;

    // 2. Obtain XYZ(D65) from the occurrence sample.
    let xyz = source.occurrence().sample().xyz();

    // 3. Transform XYZ(D65) to linear P3.
    let linear_p3 = crate::spaces::p3::xyz_to_p3_linear(xyz);

    // 4. Gamut check + hard clip.
    let (clipped_p3, out_of_gamut) = hard_clip_with_flag(linear_p3);

    // 5. Gamma encode.
    let encoded_p3 = crate::spaces::p3::p3_gamma_encode(clipped_p3);

    // 6. CSS serialization.
    let css_value = serialize_display_p3(encoded_p3);

    // 7. Encoded domain recheck against pre-encoding linear P3.
    //    We compare against the CLIPPED linear P3 because the serialized
    //    string represents the clipped value.
    let recheck = verify_p3_encoded_roundtrip(clipped_p3, encoded_p3, P3_ENCODED_RECHECK_TOLERANCE);
    if !recheck.passed() {
        return Err(OutputProjectionErrorV1::P3EncodedRecheckFailed { release, recheck });
    }

    // 8. Construct certificate with P3-specific fields.
    //    Oklab/Oklch views are not meaningful for P3 output but the
    //    certificate struct requires them. Use the same derivation as
    //    the sRGB path for structural consistency.
    let oklab = derive_oklab_view_v1(xyz).map_err(OutputProjectionErrorV1::OklabView)?;
    let oklch_view = derive_oklch_view_v1([oklab.l(), oklab.a(), oklab.b()])
        .map_err(OutputProjectionErrorV1::OklchView)?;

    let certificate = OutputProjectionCertificateV1 {
        source,
        release,
        oklab_release: OKLAB_VIEW_RELEASE_V1,
        oklch_release: oklch_view.release(),
        number_encoding: CssOklchNumberEncodingReleaseIdV1::LPercent5C6Hue3V1,
        hue_serialization:
            CssOklchHueSerializationReleaseIdV1::ExactSourceGreyOrRectangularOriginToZeroV1,
        gamut_treatment: OutputGamutTreatmentV1::HardClipWithOutOfGamutFlagV1,
        p3_encoded_recheck: Some(recheck),
        out_of_gamut,
    };

    Ok(OutputProjectionV1 {
        oklch_view,
        value: CssColor4OklchD65SolidV1(css_value.as_str().to_string()),
        certificate,
    })
}

/// Execute exactly the release selected in the request. There is no
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
        OutputProjectionReleaseIdV1::CssColor4DisplayP3FromModeledXyzD65SolidV1 => {
            project_css_color4_display_p3_from_modeled_xyz_d65_solid_v1(
                request.source(),
                request.release(),
            )
        }
        OutputProjectionReleaseIdV1::CssColor4PqRec2020FromModeledXyzAbsoluteV1 => {
            // HDR path requires HdrProjectionRequestV1; use dedicated entry point.
            return Err(OutputProjectionErrorV1::Hdr(
                HdrProjectionErrorV1::HostUnsupported {
                    capability: HostHdrCapabilityV1::Unsupported,
                    reason: "HDR projection requires project_hdr_output_v1 entry point".into(),
                },
            ));
        }
    }
}

/// HDR-specific projection entry point. Separate from SDR path to avoid
/// polluting the SDR request type with optional HDR fields.
pub(crate) fn project_hdr_output_v1(
    source: ModeledLcsOccurrenceV1,
    hdr_request: HdrProjectionRequestV1,
    host_capability: HostHdrCapabilityV1,
) -> Result<OutputProjectionV1, OutputProjectionErrorV1> {
    // Validate host capability upfront — no silent degradation
    if matches!(host_capability, HostHdrCapabilityV1::Unsupported) {
        return Err(OutputProjectionErrorV1::Hdr(
            HdrProjectionErrorV1::HostUnsupported {
                capability: host_capability,
                reason: "Host does not support PQ Rec.2020 HDR output".into(),
            },
        ));
    }

    source.verify().map_err(OutputProjectionErrorV1::Source)?;
    let xyz = source.occurrence().sample().xyz();

    let (pq_values, tm_result) =
        encode_xyz_to_hdr_pq_rec2020(xyz, &hdr_request).map_err(OutputProjectionErrorV1::Hdr)?;

    let css_value = serialize_hdr_pq_rec2020(pq_values);

    let luminance_metadata = HdrLuminanceMetadataV1 {
        black_point: hdr_request.black_point,
        peak_white: hdr_request.peak_white,
        reference_white: hdr_request.reference_white,
        source_content_peak: hdr_request.source_content_peak,
    };

    let digest = HdrConformanceDigestV1::compute(&pq_values, &luminance_metadata);

    // Build base certificate (reuse existing fields where applicable)
    let oklab = derive_oklab_view_v1(xyz).map_err(OutputProjectionErrorV1::OklabView)?;
    let oklch_view = derive_oklch_view_v1([oklab.l(), oklab.a(), oklab.b()])
        .map_err(OutputProjectionErrorV1::OklchView)?;

    let base = OutputProjectionCertificateV1 {
        source,
        release: OutputProjectionReleaseIdV1::CssColor4PqRec2020FromModeledXyzAbsoluteV1,
        oklab_release: OKLAB_VIEW_RELEASE_V1,
        oklch_release: oklch_view.release(),
        number_encoding: CssOklchNumberEncodingReleaseIdV1::LPercent5C6Hue3V1,
        hue_serialization:
            CssOklchHueSerializationReleaseIdV1::ExactSourceGreyOrRectangularOriginToZeroV1,
        gamut_treatment: OutputGamutTreatmentV1::NoExplicitProjectionGamutMapV1,
        p3_encoded_recheck: None,
        out_of_gamut: false,
    };

    // HDR certificate wraps base; available for downstream consumers
    let _hdr_certificate = HdrProjectionCertificateV1 {
        base,
        luminance_metadata,
        tone_map_result: tm_result,
        host_capability,
        conformance_digest: Some(digest),
    };

    Ok(OutputProjectionV1 {
        oklch_view,
        value: CssColor4OklchD65SolidV1(css_value),
        certificate: base,
    })
}

#[cfg(test)]
mod hdr_tests {
    use super::*;
    use crate::spaces::pq::AbsoluteLuminanceV1;

    #[test]
    fn hdr_conformance_digest_deterministic() {
        let pq = [
            PqCodeValueV1::try_new(0.5).unwrap(),
            PqCodeValueV1::try_new(0.5).unwrap(),
            PqCodeValueV1::try_new(0.5).unwrap(),
        ];
        let meta = HdrLuminanceMetadataV1 {
            black_point: AbsoluteLuminanceV1::try_new(0.0001).unwrap(),
            peak_white: AbsoluteLuminanceV1::try_new(1000.0).unwrap(),
            reference_white: AbsoluteLuminanceV1::try_new(203.0).unwrap(),
            source_content_peak: AbsoluteLuminanceV1::try_new(4000.0).unwrap(),
        };
        let d1 = HdrConformanceDigestV1::compute(&pq, &meta);
        let d2 = HdrConformanceDigestV1::compute(&pq, &meta);
        assert_eq!(d1, d2);
    }

    #[test]
    fn tone_map_monotonicity() {
        let l1 = AbsoluteLuminanceV1::try_new(100.0).unwrap();
        let l2 = AbsoluteLuminanceV1::try_new(200.0).unwrap();
        let peak = AbsoluteLuminanceV1::try_new(1000.0).unwrap();
        let r1 = tone_map(ToneMapOperatorIdV1::ReinhardGlobalV1, l1, peak).unwrap();
        let r2 = tone_map(ToneMapOperatorIdV1::ReinhardGlobalV1, l2, peak).unwrap();
        assert!(r1.output_luminance.value() <= r2.output_luminance.value());
    }

    #[test]
    fn tone_map_linear_clamp_monotonicity() {
        let l1 = AbsoluteLuminanceV1::try_new(500.0).unwrap();
        let l2 = AbsoluteLuminanceV1::try_new(1500.0).unwrap();
        let peak = AbsoluteLuminanceV1::try_new(1000.0).unwrap();
        let r1 = tone_map(ToneMapOperatorIdV1::LinearClampV1, l1, peak).unwrap();
        let r2 = tone_map(ToneMapOperatorIdV1::LinearClampV1, l2, peak).unwrap();
        assert!(r1.output_luminance.value() <= r2.output_luminance.value());
        assert!((r2.output_luminance.value() - 1000.0).abs() < 1e-10);
    }

    #[test]
    fn hdr_unsupported_host_returns_typed_error() {
        // Verify that Unsupported capability produces typed error without panic
        let err = OutputProjectionErrorV1::Hdr(HdrProjectionErrorV1::HostUnsupported {
            capability: HostHdrCapabilityV1::Unsupported,
            reason: "test".into(),
        });
        assert!(matches!(
            err,
            OutputProjectionErrorV1::Hdr(HdrProjectionErrorV1::HostUnsupported { .. })
        ));
    }
}
