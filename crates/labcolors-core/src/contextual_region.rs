//! Точное определение контекстной декартовой области family.
//!
//! Авторские binary64 bit-patterns здесь являются конечными dyadic-числами, а
//! не результатами вычислений платформы. Модуль только парсит геометрию и
//! связывает полный versioned pipeline в [`FamilyDefinitionDigestV2`]. Точный
//! образ конечного output domain строится отдельным offline proof-срезом.

use core::cmp::Ordering;

use crate::family::FamilyDefinitionDigestV2;
use crate::lcs_occurrence::{
    AdmittedSrgb8TristimulusBindingV1, AppearanceContextId, AppearanceContextSchemaReleaseId,
    Cam16UcsViewReleaseId, Cam16ViewReleaseId, ColorimetricFrameId, ColorimetricFrameReleaseId,
    ColorimetricTransformReleaseId, ModeledLcsOccurrenceReleaseId, ObserverProfileId,
    OutputProfileId, ReferenceWhiteId, SurroundProfileId, TristimulusScale,
};
use crate::sha256::Hasher;

const SIGN_MASK: u64 = 1_u64 << 63;
const EXPONENT_MASK: u64 = 0x7ff0_0000_0000_0000;
const FRACTION_MASK: u64 = 0x000f_ffff_ffff_ffff;
const NORMAL_HIDDEN_BIT: u64 = 1_u64 << 52;
const POSITIVE_ZERO_BITS: u64 = 0;

// Канонический codec пишет длины как u64. На поддерживаемых Rust targets это
// доказывает точность usize -> u64 вместо недостижимой runtime-ошибки.
const _: () = assert!(usize::BITS <= u64::BITS);

/// Ошибка допуска одного raw IEEE 754 binary64 bit-pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExactDyadic64ErrorV1 {
    NonFinite,
    NegativeZero,
}

/// Конечное exact-dyadic число, канонически заданное raw binary64 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ExactDyadic64V1(u64);

impl ExactDyadic64V1 {
    pub(crate) const fn try_from_bits(bits: u64) -> Result<Self, ExactDyadic64ErrorV1> {
        if bits & EXPONENT_MASK == EXPONENT_MASK {
            return Err(ExactDyadic64ErrorV1::NonFinite);
        }
        if bits == SIGN_MASK {
            return Err(ExactDyadic64ErrorV1::NegativeZero);
        }
        Ok(Self(bits))
    }

    pub(crate) const fn bits(self) -> u64 {
        self.0
    }

    const fn is_negative(self) -> bool {
        self.0 & SIGN_MASK != 0
    }

    const fn magnitude_parts(self) -> (u64, i32) {
        let exponent = ((self.0 & EXPONENT_MASK) >> 52) as i32;
        let fraction = self.0 & FRACTION_MASK;
        if exponent == 0 {
            (fraction, -1074)
        } else {
            // Поля непересекаются по построению, поэтому сложение точно и
            // выражает значение significand, а не политику установки битов.
            (NORMAL_HIDDEN_BIT + fraction, exponent - 1075)
        }
    }
}

impl Ord for ExactDyadic64V1 {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.is_negative(), other.is_negative()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.0.cmp(&other.0),
            (true, true) => other.0.cmp(&self.0),
        }
    }
}

impl PartialOrd for ExactDyadic64V1 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExactDyadicProductV1 {
    negative: bool,
    significand: u128,
    exponent: i32,
}

impl ExactDyadicProductV1 {
    fn of(left: ExactDyadic64V1, right: ExactDyadic64V1) -> Self {
        let (left_significand, left_exponent) = left.magnitude_parts();
        let (right_significand, right_exponent) = right.magnitude_parts();
        let significand = u128::from(left_significand) * u128::from(right_significand);
        Self {
            negative: significand != 0 && left.is_negative() != right.is_negative(),
            significand,
            exponent: left_exponent + right_exponent,
        }
    }

    fn magnitude_cmp(self, other: Self) -> Ordering {
        match (self.significand, other.significand) {
            (0, 0) => return Ordering::Equal,
            (0, _) => return Ordering::Less,
            (_, 0) => return Ordering::Greater,
            _ => {}
        }

        // Индекс бита u128 лежит в 0..=127, поэтому оба преобразования точны.
        let self_top = self.exponent + (u128::BITS - 1 - self.significand.leading_zeros()) as i32;
        let other_top =
            other.exponent + (u128::BITS - 1 - other.significand.leading_zeros()) as i32;
        match self_top.cmp(&other_top) {
            Ordering::Equal => {
                let common_exponent = self.exponent.min(other.exponent);
                let self_shift = (self.exponent - common_exponent) as u32;
                let other_shift = (other.exponent - common_exponent) as u32;
                (self.significand << self_shift).cmp(&(other.significand << other_shift))
            }
            order => order,
        }
    }
}

impl Ord for ExactDyadicProductV1 {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self.magnitude_cmp(*other),
            (true, true) => other.magnitude_cmp(*self),
        }
    }
}

impl PartialOrd for ExactDyadicProductV1 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Сырое binary64-представление одной симметричной `2 x 2` shape matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shape2BitsV1 {
    g00: u64,
    g01: u64,
    g11: u64,
}

impl Shape2BitsV1 {
    pub(crate) const fn new(g00: u64, g01: u64, g11: u64) -> Self {
        Self { g00, g01, g11 }
    }
}

/// Точная ошибка positive-definite допуска shape matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape2ErrorV1 {
    NonPositiveLeadingMinor,
    NonPositiveDeterminant,
}

/// Симметричная exact-dyadic positive-definite `2 x 2` shape matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Shape2V1 {
    g00: ExactDyadic64V1,
    g01: ExactDyadic64V1,
    g11: ExactDyadic64V1,
}

impl Shape2V1 {
    fn try_new(
        g00: ExactDyadic64V1,
        g01: ExactDyadic64V1,
        g11: ExactDyadic64V1,
    ) -> Result<Self, Shape2ErrorV1> {
        let zero = ExactDyadic64V1(POSITIVE_ZERO_BITS);
        if g00 <= zero {
            return Err(Shape2ErrorV1::NonPositiveLeadingMinor);
        }

        // Sylvester для symmetric 2 x 2: g00 > 0 и
        // g00*g11 - g01*g01 > 0. Сравнение products сохраняет точную dyadic-
        // семантику даже когда platform binary64 получил бы 0 или infinity.
        let diagonal = ExactDyadicProductV1::of(g00, g11);
        let off_diagonal = ExactDyadicProductV1::of(g01, g01);
        if diagonal <= off_diagonal {
            return Err(Shape2ErrorV1::NonPositiveDeterminant);
        }
        Ok(Self { g00, g01, g11 })
    }

    pub(crate) const fn g00(self) -> ExactDyadic64V1 {
        self.g00
    }

    pub(crate) const fn g01(self) -> ExactDyadic64V1 {
        self.g01
    }

    pub(crate) const fn g11(self) -> ExactDyadic64V1 {
        self.g11
    }
}

/// Сырое binary64-представление одного piecewise-linear tone knot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TubeKnotBitsV1 {
    tone: u64,
    center_a: u64,
    center_b: u64,
    radius_squared: u64,
}

impl TubeKnotBitsV1 {
    pub(crate) const fn new(tone: u64, center_a: u64, center_b: u64, radius_squared: u64) -> Self {
        Self {
            tone,
            center_a,
            center_b,
            radius_squared,
        }
    }
}

/// Поле definition, в котором raw binary64 не был допущен.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TubeCoordinateV1 {
    ShapeG00,
    ShapeG01,
    ShapeG11,
    Tone,
    CenterA,
    CenterB,
    RadiusSquared,
}

/// Один допущенный knot непрерывной декартовой области.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TubeKnotV1 {
    tone: ExactDyadic64V1,
    center: [ExactDyadic64V1; 2],
    radius_squared: ExactDyadic64V1,
}

impl TubeKnotV1 {
    pub(crate) const fn tone(self) -> ExactDyadic64V1 {
        self.tone
    }

    pub(crate) const fn center(self) -> [ExactDyadic64V1; 2] {
        self.center
    }

    pub(crate) const fn radius_squared(self) -> ExactDyadic64V1 {
        self.radius_squared
    }
}

/// Ошибка parse целого `PiecewiseLinearCartesianTubeV1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PiecewiseLinearCartesianTubeErrorV1 {
    Coordinate {
        index: Option<usize>,
        coordinate: TubeCoordinateV1,
        reason: ExactDyadic64ErrorV1,
    },
    Shape(Shape2ErrorV1),
    EmptyToneDomain,
    ToneNotStrictlyIncreasing {
        index: usize,
    },
    NegativeRadiusSquared {
        index: usize,
    },
    ResourceExhausted,
}

/// Замкнутая область: между строго упорядоченными tone knots линейно
/// интерполируются центр и `radius_squared`; точка `z` принадлежит области,
/// когда `(z - center(t))ᵀ G (z - center(t)) <= radius_squared(t)`.
/// Здесь `t = J′`, `z = [a′, b′]` в identity-bound rectangular CAM16-UCS view;
/// tone-домен замкнут от первого до последнего knot, а один knot задаёт singleton.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PiecewiseLinearCartesianTubeV1 {
    shape: Shape2V1,
    knots: Vec<TubeKnotV1>,
}

impl PiecewiseLinearCartesianTubeV1 {
    pub(crate) fn try_from_bits(
        shape: Shape2BitsV1,
        knots: &[TubeKnotBitsV1],
    ) -> Result<Self, PiecewiseLinearCartesianTubeErrorV1> {
        fn coordinate(
            bits: u64,
            index: Option<usize>,
            coordinate: TubeCoordinateV1,
        ) -> Result<ExactDyadic64V1, PiecewiseLinearCartesianTubeErrorV1> {
            ExactDyadic64V1::try_from_bits(bits).map_err(|reason| {
                PiecewiseLinearCartesianTubeErrorV1::Coordinate {
                    index,
                    coordinate,
                    reason,
                }
            })
        }

        let shape = Shape2V1::try_new(
            coordinate(shape.g00, None, TubeCoordinateV1::ShapeG00)?,
            coordinate(shape.g01, None, TubeCoordinateV1::ShapeG01)?,
            coordinate(shape.g11, None, TubeCoordinateV1::ShapeG11)?,
        )
        .map_err(PiecewiseLinearCartesianTubeErrorV1::Shape)?;
        if knots.is_empty() {
            return Err(PiecewiseLinearCartesianTubeErrorV1::EmptyToneDomain);
        }

        let mut parsed = Vec::new();
        parsed
            .try_reserve_exact(knots.len())
            .map_err(|_| PiecewiseLinearCartesianTubeErrorV1::ResourceExhausted)?;
        let zero = ExactDyadic64V1(POSITIVE_ZERO_BITS);
        for (index, knot) in knots.iter().copied().enumerate() {
            let tone = coordinate(knot.tone, Some(index), TubeCoordinateV1::Tone)?;
            if parsed
                .last()
                .is_some_and(|previous: &TubeKnotV1| previous.tone >= tone)
            {
                return Err(
                    PiecewiseLinearCartesianTubeErrorV1::ToneNotStrictlyIncreasing { index },
                );
            }
            let center_a = coordinate(knot.center_a, Some(index), TubeCoordinateV1::CenterA)?;
            let center_b = coordinate(knot.center_b, Some(index), TubeCoordinateV1::CenterB)?;
            let radius_squared = coordinate(
                knot.radius_squared,
                Some(index),
                TubeCoordinateV1::RadiusSquared,
            )?;
            if radius_squared < zero {
                return Err(PiecewiseLinearCartesianTubeErrorV1::NegativeRadiusSquared { index });
            }
            parsed.push(TubeKnotV1 {
                tone,
                center: [center_a, center_b],
                radius_squared,
            });
        }

        Ok(Self {
            shape,
            knots: parsed,
        })
    }

    pub(crate) const fn shape(&self) -> Shape2V1 {
        self.shape
    }

    pub(crate) fn knots(&self) -> &[TubeKnotV1] {
        &self.knots
    }
}

/// Ошибка связывания закрытого contextual pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextualRegionPipelineErrorV1 {
    OutputProfileMismatch {
        domain: OutputProfileId,
        lowering: OutputProfileId,
    },
    FrameMismatch {
        lowering: ColorimetricFrameId,
        context: ColorimetricFrameId,
    },
}

/// Полный versioned pipeline, относительно которого definition имеет смысл.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContextualRegionPipelineV1 {
    output_profile: OutputProfileId,
    lowering: AdmittedSrgb8TristimulusBindingV1,
    modeled_occurrence: ModeledLcsOccurrenceReleaseId,
    context: AppearanceContextId,
    cam16_view: Cam16ViewReleaseId,
    rectangular_view: Cam16UcsViewReleaseId,
}

impl ContextualRegionPipelineV1 {
    pub(crate) fn try_new(
        output_profile: OutputProfileId,
        lowering: AdmittedSrgb8TristimulusBindingV1,
        modeled_occurrence: ModeledLcsOccurrenceReleaseId,
        context: AppearanceContextId,
        cam16_view: Cam16ViewReleaseId,
        rectangular_view: Cam16UcsViewReleaseId,
    ) -> Result<Self, ContextualRegionPipelineErrorV1> {
        if output_profile != lowering.signal_output_profile() {
            return Err(ContextualRegionPipelineErrorV1::OutputProfileMismatch {
                domain: output_profile,
                lowering: lowering.signal_output_profile(),
            });
        }
        if lowering.result_frame() != context.frame() {
            return Err(ContextualRegionPipelineErrorV1::FrameMismatch {
                lowering: lowering.result_frame(),
                context: context.frame(),
            });
        }
        Ok(Self {
            output_profile,
            lowering,
            modeled_occurrence,
            context,
            cam16_view,
            rectangular_view,
        })
    }
}

trait CanonicalSinkV1 {
    fn write(&mut self, bytes: &[u8]);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextualRegionIdentityEncodingReleaseIdV1 {
    LengthPrefixedBigEndianV1,
    #[cfg(test)]
    MutationSentinelV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContextualRegionFamilyProviderReleaseIdV1 {
    V1,
    #[cfg(test)]
    MutationSentinelV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PiecewiseLinearCartesianTubeReleaseIdV1 {
    V1,
    #[cfg(test)]
    MutationSentinelV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ContextualRegionDefinitionReleasesV1 {
    identity_encoding: ContextualRegionIdentityEncodingReleaseIdV1,
    provider: ContextualRegionFamilyProviderReleaseIdV1,
    region: PiecewiseLinearCartesianTubeReleaseIdV1,
}

impl ContextualRegionDefinitionReleasesV1 {
    #[cfg(test)]
    pub(crate) const fn new(
        identity_encoding: ContextualRegionIdentityEncodingReleaseIdV1,
        provider: ContextualRegionFamilyProviderReleaseIdV1,
        region: PiecewiseLinearCartesianTubeReleaseIdV1,
    ) -> Self {
        Self {
            identity_encoding,
            provider,
            region,
        }
    }
}

const CONTEXTUAL_REGION_DEFINITION_RELEASES_V1: ContextualRegionDefinitionReleasesV1 =
    ContextualRegionDefinitionReleasesV1 {
        identity_encoding: ContextualRegionIdentityEncodingReleaseIdV1::LengthPrefixedBigEndianV1,
        provider: ContextualRegionFamilyProviderReleaseIdV1::V1,
        region: PiecewiseLinearCartesianTubeReleaseIdV1::V1,
    };

impl CanonicalSinkV1 for Hasher {
    fn write(&mut self, bytes: &[u8]) {
        self.update(bytes);
    }
}

#[cfg(test)]
impl CanonicalSinkV1 for Vec<u8> {
    fn write(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

fn write_field(sink: &mut impl CanonicalSinkV1, bytes: &[u8]) {
    sink.write(&(bytes.len() as u64).to_be_bytes());
    sink.write(bytes);
}

fn output_profile_tag(value: OutputProfileId) -> u8 {
    match value {
        OutputProfileId::Iec61966Srgb8D65V1 => 1,
    }
}

fn lowering_tag(value: AdmittedSrgb8TristimulusBindingV1) -> u8 {
    match value {
        AdmittedSrgb8TristimulusBindingV1::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 => 1,
    }
}

fn transform_tag(value: ColorimetricTransformReleaseId) -> u8 {
    match value {
        ColorimetricTransformReleaseId::Iec61966Srgb8ToCie1931TwoDegreeXyzD65RelativeY1V1 => 1,
    }
}

fn observer_tag(value: ObserverProfileId) -> u8 {
    match value {
        ObserverProfileId::Cie1931TwoDegreeV1 => 1,
    }
}

fn reference_white_tag(value: ReferenceWhiteId) -> u8 {
    match value {
        ReferenceWhiteId::Iec61966D65ChromaticityV1 => 1,
    }
}

fn scale_tag(value: TristimulusScale) -> u8 {
    match value {
        TristimulusScale::RelativeY1 => 1,
    }
}

fn frame_release_tag(value: ColorimetricFrameReleaseId) -> u8 {
    match value {
        ColorimetricFrameReleaseId::XyzV1 => 1,
        #[cfg(test)]
        ColorimetricFrameReleaseId::MutationSentinelV1 => 2,
    }
}

fn frame_bytes(value: ColorimetricFrameId) -> [u8; 4] {
    [
        observer_tag(value.observer()),
        reference_white_tag(value.reference_white()),
        scale_tag(value.scale()),
        frame_release_tag(value.release()),
    ]
}

fn modeled_occurrence_tag(value: ModeledLcsOccurrenceReleaseId) -> u8 {
    match value {
        ModeledLcsOccurrenceReleaseId::V1 => 1,
        #[cfg(test)]
        ModeledLcsOccurrenceReleaseId::MutationSentinelV1 => 2,
    }
}

fn context_schema_tag(value: AppearanceContextSchemaReleaseId) -> u8 {
    match value {
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1 => 1,
    }
}

fn surround_tag(value: SurroundProfileId) -> u8 {
    match value {
        SurroundProfileId::AverageV1 => 1,
        SurroundProfileId::DimV1 => 2,
        SurroundProfileId::DarkV1 => 3,
    }
}

fn cam16_view_tag(value: Cam16ViewReleaseId) -> u8 {
    match value {
        Cam16ViewReleaseId::LiEtAl2017Cie248ForwardV1 => 1,
    }
}

fn rectangular_view_tag(value: Cam16UcsViewReleaseId) -> u8 {
    match value {
        Cam16UcsViewReleaseId::LiEtAl2017Cam16UcsV1 => 1,
    }
}

fn identity_encoding_tag(value: ContextualRegionIdentityEncodingReleaseIdV1) -> u8 {
    match value {
        ContextualRegionIdentityEncodingReleaseIdV1::LengthPrefixedBigEndianV1 => 1,
        #[cfg(test)]
        ContextualRegionIdentityEncodingReleaseIdV1::MutationSentinelV1 => 2,
    }
}

fn provider_release_tag(value: ContextualRegionFamilyProviderReleaseIdV1) -> u8 {
    match value {
        ContextualRegionFamilyProviderReleaseIdV1::V1 => 1,
        #[cfg(test)]
        ContextualRegionFamilyProviderReleaseIdV1::MutationSentinelV1 => 2,
    }
}

fn region_release_tag(value: PiecewiseLinearCartesianTubeReleaseIdV1) -> u8 {
    match value {
        PiecewiseLinearCartesianTubeReleaseIdV1::V1 => 1,
        #[cfg(test)]
        PiecewiseLinearCartesianTubeReleaseIdV1::MutationSentinelV1 => 2,
    }
}

fn write_identity(
    sink: &mut impl CanonicalSinkV1,
    releases: ContextualRegionDefinitionReleasesV1,
    pipeline: ContextualRegionPipelineV1,
    region: &PiecewiseLinearCartesianTubeV1,
) {
    const DOMAIN: &[u8] = b"labcolors.contextual-region-family-provider.v1\0";

    write_field(sink, DOMAIN);
    write_field(sink, &[identity_encoding_tag(releases.identity_encoding)]);
    write_field(sink, &[provider_release_tag(releases.provider)]);
    write_field(sink, &[output_profile_tag(pipeline.output_profile)]);
    write_field(sink, &[lowering_tag(pipeline.lowering)]);
    write_field(
        sink,
        &[output_profile_tag(
            pipeline.lowering.signal_output_profile(),
        )],
    );
    write_field(
        sink,
        &[transform_tag(pipeline.lowering.transform_release())],
    );
    write_field(sink, &frame_bytes(pipeline.lowering.result_frame()));
    write_field(sink, &[modeled_occurrence_tag(pipeline.modeled_occurrence)]);
    write_field(
        sink,
        &[context_schema_tag(pipeline.context.schema_release())],
    );
    write_field(sink, &frame_bytes(pipeline.context.frame()));
    write_field(
        sink,
        &pipeline
            .context
            .adapting_luminance_cd_m2()
            .to_bits()
            .to_be_bytes(),
    );
    write_field(
        sink,
        &pipeline
            .context
            .background_luminance_ratio()
            .to_bits()
            .to_be_bytes(),
    );
    write_field(sink, &[surround_tag(pipeline.context.surround_profile())]);
    write_field(sink, &[cam16_view_tag(pipeline.cam16_view)]);
    write_field(sink, &[rectangular_view_tag(pipeline.rectangular_view)]);
    write_field(sink, &[region_release_tag(releases.region)]);
    write_field(sink, &region.shape.g00.bits().to_be_bytes());
    write_field(sink, &region.shape.g01.bits().to_be_bytes());
    write_field(sink, &region.shape.g11.bits().to_be_bytes());
    write_field(sink, &(region.knots.len() as u64).to_be_bytes());
    for knot in &region.knots {
        write_field(sink, &knot.tone.bits().to_be_bytes());
        write_field(sink, &knot.center[0].bits().to_be_bytes());
        write_field(sink, &knot.center[1].bits().to_be_bytes());
        write_field(sink, &knot.radius_squared.bits().to_be_bytes());
    }
}

/// Единственная V5b2b provider-поверхность: definition address без mint image.
pub(crate) struct ContextualRegionFamilyProviderV1;

impl ContextualRegionFamilyProviderV1 {
    pub(crate) fn definition_digest(
        pipeline: ContextualRegionPipelineV1,
        region: &PiecewiseLinearCartesianTubeV1,
    ) -> FamilyDefinitionDigestV2 {
        let mut hasher = Hasher::new();
        write_identity(
            &mut hasher,
            CONTEXTUAL_REGION_DEFINITION_RELEASES_V1,
            pipeline,
            region,
        );
        FamilyDefinitionDigestV2::from_digest(*hasher.finalize().as_bytes())
    }

    #[cfg(test)]
    pub(crate) fn definition_digest_with_releases_for_test(
        releases: ContextualRegionDefinitionReleasesV1,
        pipeline: ContextualRegionPipelineV1,
        region: &PiecewiseLinearCartesianTubeV1,
    ) -> FamilyDefinitionDigestV2 {
        let mut hasher = Hasher::new();
        write_identity(&mut hasher, releases, pipeline, region);
        FamilyDefinitionDigestV2::from_digest(*hasher.finalize().as_bytes())
    }

    #[cfg(test)]
    pub(crate) fn canonical_identity_bytes_for_test(
        pipeline: ContextualRegionPipelineV1,
        region: &PiecewiseLinearCartesianTubeV1,
    ) -> Vec<u8> {
        let mut bytes = Vec::new();
        write_identity(
            &mut bytes,
            CONTEXTUAL_REGION_DEFINITION_RELEASES_V1,
            pipeline,
            region,
        );
        bytes
    }
}
