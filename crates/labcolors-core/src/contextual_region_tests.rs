use core::cmp::Ordering;

use proptest::prelude::*;

use crate::contextual_region::{
    CONTEXTUAL_REGION_FORMULA_RELEASE_V1, ContextualRegionDefinitionReleasesV1,
    ContextualRegionFamilyProviderReleaseIdV1, ContextualRegionFamilyProviderV1,
    ContextualRegionFormulaReleaseIdV1, ContextualRegionIdentityEncodingReleaseIdV1,
    ContextualRegionPipelineErrorV1, ContextualRegionPipelineV1, ExactDyadic64ErrorV1,
    ExactDyadic64V1, PiecewiseLinearCartesianTubeErrorV1, PiecewiseLinearCartesianTubeReleaseIdV1,
    PiecewiseLinearCartesianTubeV1, Shape2BitsV1, Shape2ErrorV1, TubeCoordinateV1, TubeKnotBitsV1,
};
use crate::family::FamilyDefinitionDigestV2;
use crate::lcs_occurrence::{
    ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1, AdaptingLuminanceCdM2, AppearanceContextId,
    AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio, CAM16_UCS_VIEW_RELEASE_V1,
    CAM16_VIEW_RELEASE_V1, IEC_SRGB_D65_XYZ_FRAME_V1, MODELED_LCS_OCCURRENCE_RELEASE_V1,
    MUTATION_SENTINEL_XYZ_FRAME_V1, ModeledLcsOccurrenceReleaseId, OutputProfileId,
    SurroundProfileId,
};

const POSITIVE_ZERO: u64 = 0x0000_0000_0000_0000;
const NEGATIVE_ZERO: u64 = 0x8000_0000_0000_0000;
const MIN_SUBNORMAL: u64 = 0x0000_0000_0000_0001;
const MIN_NORMAL: u64 = 0x0010_0000_0000_0000;
const NEGATIVE_MIN_SUBNORMAL: u64 = 0x8000_0000_0000_0001;
const ONE: u64 = 0x3ff0_0000_0000_0000;
const ONE_AND_HALF: u64 = 0x3ff8_0000_0000_0000;
const TWO: u64 = 0x4000_0000_0000_0000;
const TWO_AND_HALF: u64 = 0x4004_0000_0000_0000;
const THREE: u64 = 0x4008_0000_0000_0000;
const FOUR: u64 = 0x4010_0000_0000_0000;
const HALF: u64 = 0x3fe0_0000_0000_0000;
const NEGATIVE_ONE: u64 = 0xbff0_0000_0000_0000;
const MAX_FINITE: u64 = 0x7fef_ffff_ffff_ffff;
const NEGATIVE_MAX_FINITE: u64 = 0xffef_ffff_ffff_ffff;

fn context(frame: crate::lcs_occurrence::ColorimetricFrameId) -> AppearanceContextId {
    context_with(frame, 64.0, 0.2, SurroundProfileId::AverageV1)
}

fn context_with(
    frame: crate::lcs_occurrence::ColorimetricFrameId,
    adapting_luminance: f64,
    background_ratio: f64,
    surround: SurroundProfileId,
) -> AppearanceContextId {
    AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        frame,
        AdaptingLuminanceCdM2::try_new(adapting_luminance).unwrap(),
        BackgroundLuminanceRatio::try_new(background_ratio).unwrap(),
        surround,
    )
}

fn knot(tone: u64, center_a: u64, center_b: u64, radius_squared: u64) -> TubeKnotBitsV1 {
    TubeKnotBitsV1::new(tone, center_a, center_b, radius_squared)
}

fn region_with_centers(centers: [[u64; 2]; 2]) -> PiecewiseLinearCartesianTubeV1 {
    PiecewiseLinearCartesianTubeV1::try_from_bits(
        Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
        &[
            knot(ONE, centers[0][0], centers[0][1], FOUR),
            knot(TWO, centers[1][0], centers[1][1], FOUR),
        ],
    )
    .unwrap()
}

fn pipeline(context: AppearanceContextId) -> ContextualRegionPipelineV1 {
    pipeline_with_release(context, MODELED_LCS_OCCURRENCE_RELEASE_V1)
}

fn pipeline_with_release(
    context: AppearanceContextId,
    modeled: ModeledLcsOccurrenceReleaseId,
) -> ContextualRegionPipelineV1 {
    ContextualRegionPipelineV1::try_new(
        OutputProfileId::Iec61966Srgb8D65V1,
        ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1,
        modeled,
        context,
        CAM16_VIEW_RELEASE_V1,
        CAM16_UCS_VIEW_RELEASE_V1,
        CONTEXTUAL_REGION_FORMULA_RELEASE_V1,
    )
    .unwrap()
}

fn pipeline_with_formula_release(
    context: AppearanceContextId,
    formula: ContextualRegionFormulaReleaseIdV1,
) -> ContextualRegionPipelineV1 {
    ContextualRegionPipelineV1::try_new(
        OutputProfileId::Iec61966Srgb8D65V1,
        ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1,
        MODELED_LCS_OCCURRENCE_RELEASE_V1,
        context,
        CAM16_VIEW_RELEASE_V1,
        CAM16_UCS_VIEW_RELEASE_V1,
        formula,
    )
    .unwrap()
}

proptest! {
    #[test]
    fn exact_spd_matches_an_independent_integer_oracle(
        g00 in -1_000_i16..=1_000,
        g01 in -1_000_i16..=1_000,
        g11 in -1_000_i16..=1_000,
    ) {
        // Каждое сгенерированное целое и произведение точно представимо в
        // безопасном binary64-диапазоне. Oracle использует только integer math.
        let expected = g00 > 0
            && i64::from(g00) * i64::from(g11) > i64::from(g01) * i64::from(g01);
        let actual = PiecewiseLinearCartesianTubeV1::try_from_bits(
            Shape2BitsV1::new(
                f64::from(g00).to_bits(),
                f64::from(g01).to_bits(),
                f64::from(g11).to_bits(),
            ),
            &[knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE)],
        )
        .is_ok();
        prop_assert_eq!(actual, expected);
    }
}

#[test]
fn exact_dyadic_parser_preserves_every_finite_binary64_class() {
    for bits in [
        NEGATIVE_MAX_FINITE,
        NEGATIVE_ONE,
        NEGATIVE_MIN_SUBNORMAL,
        POSITIVE_ZERO,
        MIN_SUBNORMAL,
        ONE,
        MAX_FINITE,
    ] {
        assert_eq!(ExactDyadic64V1::try_from_bits(bits).unwrap().bits(), bits);
    }

    let ordered = [
        NEGATIVE_MAX_FINITE,
        NEGATIVE_ONE,
        NEGATIVE_MIN_SUBNORMAL,
        POSITIVE_ZERO,
        MIN_SUBNORMAL,
        ONE,
        MAX_FINITE,
    ]
    .map(|bits| ExactDyadic64V1::try_from_bits(bits).unwrap());
    assert!(ordered.windows(2).all(|pair| pair[0] < pair[1]));
    assert_eq!(ordered[3].cmp(&ordered[3]), Ordering::Equal);
}

#[test]
fn exact_dyadic_parser_rejects_only_nonfinite_and_negative_zero() {
    assert_eq!(
        ExactDyadic64V1::try_from_bits(NEGATIVE_ZERO),
        Err(ExactDyadic64ErrorV1::NegativeZero),
    );
    for bits in [
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        f64::NAN.to_bits(),
        0x7ff0_0000_0000_0001,
        0xfff8_0000_0000_0001,
    ] {
        assert_eq!(
            ExactDyadic64V1::try_from_bits(bits),
            Err(ExactDyadic64ErrorV1::NonFinite),
        );
    }
}

#[test]
fn shape_spd_is_exact_when_platform_products_overflow() {
    let admitted = PiecewiseLinearCartesianTubeV1::try_from_bits(
        Shape2BitsV1::new(MAX_FINITE, POSITIVE_ZERO, MAX_FINITE),
        &[knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE)],
    )
    .unwrap();
    assert_eq!(admitted.shape().g00().bits(), MAX_FINITE);
    assert_eq!(admitted.shape().g01().bits(), POSITIVE_ZERO);
    assert_eq!(admitted.shape().g11().bits(), MAX_FINITE);

    let singular = PiecewiseLinearCartesianTubeV1::try_from_bits(
        Shape2BitsV1::new(MAX_FINITE, MAX_FINITE, MAX_FINITE),
        &[knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE)],
    );
    assert_eq!(
        singular,
        Err(PiecewiseLinearCartesianTubeErrorV1::Shape(
            Shape2ErrorV1::NonPositiveDeterminant,
        )),
    );
}

#[test]
fn shape_spd_is_exact_when_platform_products_underflow() {
    PiecewiseLinearCartesianTubeV1::try_from_bits(
        Shape2BitsV1::new(MIN_SUBNORMAL, POSITIVE_ZERO, MIN_SUBNORMAL),
        &[knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE)],
    )
    .unwrap();

    let singular = PiecewiseLinearCartesianTubeV1::try_from_bits(
        Shape2BitsV1::new(MIN_SUBNORMAL, MIN_SUBNORMAL, MIN_SUBNORMAL),
        &[knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE)],
    );
    assert_eq!(
        singular,
        Err(PiecewiseLinearCartesianTubeErrorV1::Shape(
            Shape2ErrorV1::NonPositiveDeterminant,
        )),
    );
}

#[test]
fn shape_spd_compares_subnormal_and_normal_products_on_one_exact_scale() {
    PiecewiseLinearCartesianTubeV1::try_from_bits(
        Shape2BitsV1::new(MIN_SUBNORMAL, MIN_NORMAL, MAX_FINITE),
        &[knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE)],
    )
    .unwrap();

    let outside = PiecewiseLinearCartesianTubeV1::try_from_bits(
        Shape2BitsV1::new(MIN_SUBNORMAL, ONE, MAX_FINITE),
        &[knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE)],
    );
    assert_eq!(
        outside,
        Err(PiecewiseLinearCartesianTubeErrorV1::Shape(
            Shape2ErrorV1::NonPositiveDeterminant,
        )),
    );
}

#[test]
fn shape_spd_exactly_aligns_products_from_adjacent_binades() {
    PiecewiseLinearCartesianTubeV1::try_from_bits(
        Shape2BitsV1::new(ONE, ONE_AND_HALF, TWO_AND_HALF),
        &[knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE)],
    )
    .unwrap();
}

#[test]
fn region_parser_rejects_every_invalid_structural_state() {
    assert_eq!(
        PiecewiseLinearCartesianTubeV1::try_from_bits(
            Shape2BitsV1::new(POSITIVE_ZERO, POSITIVE_ZERO, ONE),
            &[knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE)],
        ),
        Err(PiecewiseLinearCartesianTubeErrorV1::Shape(
            Shape2ErrorV1::NonPositiveLeadingMinor,
        )),
    );
    assert_eq!(
        PiecewiseLinearCartesianTubeV1::try_from_bits(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[],
        ),
        Err(PiecewiseLinearCartesianTubeErrorV1::EmptyToneDomain),
    );
    assert_eq!(
        PiecewiseLinearCartesianTubeV1::try_from_bits(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE),
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE),
            ],
        ),
        Err(PiecewiseLinearCartesianTubeErrorV1::ToneNotStrictlyIncreasing { index: 1 },),
    );
    assert_eq!(
        PiecewiseLinearCartesianTubeV1::try_from_bits(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[
                knot(TWO, POSITIVE_ZERO, POSITIVE_ZERO, ONE),
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, ONE),
            ],
        ),
        Err(PiecewiseLinearCartesianTubeErrorV1::ToneNotStrictlyIncreasing { index: 1 },),
    );
    assert_eq!(
        PiecewiseLinearCartesianTubeV1::try_from_bits(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, NEGATIVE_ONE)],
        ),
        Err(PiecewiseLinearCartesianTubeErrorV1::NegativeRadiusSquared { index: 0 }),
    );
    assert_eq!(
        PiecewiseLinearCartesianTubeV1::try_from_bits(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[knot(ONE, f64::NAN.to_bits(), POSITIVE_ZERO, ONE)],
        ),
        Err(PiecewiseLinearCartesianTubeErrorV1::Coordinate {
            index: Some(0),
            coordinate: TubeCoordinateV1::CenterA,
            reason: ExactDyadic64ErrorV1::NonFinite,
        }),
    );
}

#[test]
fn all_center_strengths_are_data_of_one_region_law() {
    let zero = region_with_centers([
        [POSITIVE_ZERO, POSITIVE_ZERO],
        [POSITIVE_ZERO, POSITIVE_ZERO],
    ]);
    let weak = region_with_centers([[MIN_SUBNORMAL, HALF], [ONE, HALF]]);
    let chromatic = region_with_centers([[TWO, THREE], [THREE, FOUR]]);

    assert_eq!(zero.knots().len(), weak.knots().len());
    assert_eq!(weak.knots().len(), chromatic.knots().len());
    assert_eq!(zero.shape(), weak.shape());
    assert_eq!(weak.shape(), chromatic.shape());
    assert_eq!(zero.knots()[0].tone().bits(), ONE);
    assert_eq!(zero.knots()[0].center().map(ExactDyadic64V1::bits), [0; 2]);
    assert_eq!(zero.knots()[0].radius_squared().bits(), FOUR);
}

#[test]
fn contextual_pipeline_rejects_a_foreign_context_frame() {
    let error = ContextualRegionPipelineV1::try_new(
        OutputProfileId::Iec61966Srgb8D65V1,
        ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1,
        MODELED_LCS_OCCURRENCE_RELEASE_V1,
        context(MUTATION_SENTINEL_XYZ_FRAME_V1),
        CAM16_VIEW_RELEASE_V1,
        CAM16_UCS_VIEW_RELEASE_V1,
        CONTEXTUAL_REGION_FORMULA_RELEASE_V1,
    );
    assert_eq!(
        error,
        Err(ContextualRegionPipelineErrorV1::FrameMismatch {
            lowering: IEC_SRGB_D65_XYZ_FRAME_V1,
            context: MUTATION_SENTINEL_XYZ_FRAME_V1,
        }),
    );
}

#[test]
fn provider_returns_only_the_definition_digest() {
    let digest: FamilyDefinitionDigestV2 = ContextualRegionFamilyProviderV1::definition_digest(
        pipeline(context(IEC_SRGB_D65_XYZ_FRAME_V1)),
        &region_with_centers([[POSITIVE_ZERO; 2]; 2]),
    );
    assert_ne!(digest.as_bytes(), &[0; 32]);
}

#[test]
fn typed_pipeline_and_region_mutations_change_definition_identity() {
    let base_context = context(IEC_SRGB_D65_XYZ_FRAME_V1);
    let base_region = region_with_centers([[POSITIVE_ZERO; 2]; 2]);
    let baseline =
        ContextualRegionFamilyProviderV1::definition_digest(pipeline(base_context), &base_region);
    let assert_changed = |pipeline, region: &PiecewiseLinearCartesianTubeV1| {
        assert_ne!(
            ContextualRegionFamilyProviderV1::definition_digest(pipeline, region),
            baseline,
        );
    };

    assert_changed(
        pipeline_with_release(
            base_context,
            ModeledLcsOccurrenceReleaseId::MutationSentinelV1,
        ),
        &base_region,
    );
    assert_changed(
        pipeline_with_formula_release(
            base_context,
            ContextualRegionFormulaReleaseIdV1::MutationSentinelV1,
        ),
        &base_region,
    );
    for changed_context in [
        context_with(
            IEC_SRGB_D65_XYZ_FRAME_V1,
            32.0,
            0.2,
            SurroundProfileId::AverageV1,
        ),
        context_with(
            IEC_SRGB_D65_XYZ_FRAME_V1,
            64.0,
            0.3,
            SurroundProfileId::AverageV1,
        ),
        context_with(
            IEC_SRGB_D65_XYZ_FRAME_V1,
            64.0,
            0.2,
            SurroundProfileId::DimV1,
        ),
    ] {
        assert_changed(pipeline(changed_context), &base_region);
    }

    let region = |shape, knots: &[TubeKnotBitsV1]| {
        PiecewiseLinearCartesianTubeV1::try_from_bits(shape, knots).unwrap()
    };
    for changed_region in [
        region(
            Shape2BitsV1::new(TWO, POSITIVE_ZERO, ONE),
            &[
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
                knot(TWO, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
            ],
        ),
        region(
            Shape2BitsV1::new(ONE, HALF, ONE),
            &[
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
                knot(TWO, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
            ],
        ),
        region(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, TWO),
            &[
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
                knot(TWO, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
            ],
        ),
        region(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[
                knot(HALF, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
                knot(TWO, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
            ],
        ),
        region(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[
                knot(ONE, MIN_SUBNORMAL, POSITIVE_ZERO, FOUR),
                knot(TWO, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
            ],
        ),
        region(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[
                knot(ONE, POSITIVE_ZERO, NEGATIVE_MIN_SUBNORMAL, FOUR),
                knot(TWO, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
            ],
        ),
        region(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, THREE),
                knot(TWO, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
            ],
        ),
        region(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
                knot(THREE, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
            ],
        ),
        region(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
                knot(TWO, MIN_SUBNORMAL, POSITIVE_ZERO, FOUR),
            ],
        ),
        region(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
                knot(TWO, POSITIVE_ZERO, NEGATIVE_MIN_SUBNORMAL, FOUR),
            ],
        ),
        region(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
                knot(TWO, POSITIVE_ZERO, POSITIVE_ZERO, THREE),
            ],
        ),
        region(
            Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
            &[
                knot(ONE, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
                knot(TWO, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
                knot(THREE, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
            ],
        ),
    ] {
        assert_changed(pipeline(base_context), &changed_region);
    }
}

#[test]
fn typed_local_release_mutations_change_definition_identity() {
    let pipeline = pipeline(context(IEC_SRGB_D65_XYZ_FRAME_V1));
    let region = region_with_centers([[POSITIVE_ZERO; 2]; 2]);
    let baseline = ContextualRegionFamilyProviderV1::definition_digest(pipeline, &region);
    let stable_encoding = ContextualRegionIdentityEncodingReleaseIdV1::LengthPrefixedBigEndianV1;
    let stable_provider = ContextualRegionFamilyProviderReleaseIdV1::V1;
    let stable_region = PiecewiseLinearCartesianTubeReleaseIdV1::V1;

    for releases in [
        ContextualRegionDefinitionReleasesV1::new(
            ContextualRegionIdentityEncodingReleaseIdV1::MutationSentinelV1,
            stable_provider,
            stable_region,
        ),
        ContextualRegionDefinitionReleasesV1::new(
            stable_encoding,
            ContextualRegionFamilyProviderReleaseIdV1::MutationSentinelV1,
            stable_region,
        ),
        ContextualRegionDefinitionReleasesV1::new(
            stable_encoding,
            stable_provider,
            PiecewiseLinearCartesianTubeReleaseIdV1::MutationSentinelV1,
        ),
    ] {
        assert_ne!(
            ContextualRegionFamilyProviderV1::definition_digest_with_releases_for_test(
                releases, pipeline, &region,
            ),
            baseline,
        );
    }
}

#[test]
fn canonical_identity_is_length_prefixed_big_endian_and_golden() {
    let pipeline = pipeline(context(IEC_SRGB_D65_XYZ_FRAME_V1));
    let region = region_with_centers([[POSITIVE_ZERO; 2]; 2]);
    let bytes =
        ContextualRegionFamilyProviderV1::canonical_identity_bytes_for_test(pipeline, &region);

    let fields = decode_fields(&bytes);
    assert_eq!(fields.len(), 30);
    assert_eq!(
        fields[0],
        b"labcolors.contextual-region-family-provider.v1\0"
    );
    assert_eq!(fields[1], &[1]);
    assert_eq!(fields[2], &[1]);
    assert_eq!(fields[3], &[1]);
    assert_eq!(fields[4], &[1]);
    assert_eq!(fields[5], &[1]);
    assert_eq!(fields[6], &[1]);
    assert_eq!(fields[7], &[1, 1, 1, 1]);
    assert_eq!(fields[8], &[1]);
    assert_eq!(fields[9], &[1]);
    assert_eq!(fields[10], &[1, 1, 1, 1]);
    assert_eq!(fields[11], &64.0_f64.to_bits().to_be_bytes());
    assert_eq!(fields[12], &0.2_f64.to_bits().to_be_bytes());
    assert_eq!(fields[13], &[1]);
    assert_eq!(fields[14], &[1]);
    assert_eq!(fields[15], &[1]);
    assert_eq!(fields[16], &[1]);
    assert_eq!(fields[17], &[1]);
    assert_eq!(fields[18], &ONE.to_be_bytes());
    assert_eq!(fields[19], &POSITIVE_ZERO.to_be_bytes());
    assert_eq!(fields[20], &ONE.to_be_bytes());
    assert_eq!(fields[21], &2_u64.to_be_bytes());
    assert_eq!(fields[22], &ONE.to_be_bytes());
    assert_eq!(fields[23], &POSITIVE_ZERO.to_be_bytes());
    assert_eq!(fields[24], &POSITIVE_ZERO.to_be_bytes());
    assert_eq!(fields[25], &FOUR.to_be_bytes());
    assert_eq!(fields[26], &TWO.to_be_bytes());
    assert_eq!(fields[27], &POSITIVE_ZERO.to_be_bytes());
    assert_eq!(fields[28], &POSITIVE_ZERO.to_be_bytes());
    assert_eq!(fields[29], &FOUR.to_be_bytes());

    assert_eq!(
        hex(ContextualRegionFamilyProviderV1::definition_digest(pipeline, &region,).as_bytes()),
        "72d4329c98db9942c5992d718079cb94745615d78943e9323381ffc917bf6b91",
    );
}

#[test]
fn every_canonical_identity_field_changes_the_digest() {
    let pipeline = pipeline(context(IEC_SRGB_D65_XYZ_FRAME_V1));
    let region = region_with_centers([[POSITIVE_ZERO; 2]; 2]);
    let bytes =
        ContextualRegionFamilyProviderV1::canonical_identity_bytes_for_test(pipeline, &region);
    let baseline = crate::sha256::digest(&bytes);
    let ranges = field_payload_ranges(&bytes);
    assert_eq!(ranges.len(), 30);

    for (field, range) in ranges.into_iter().enumerate() {
        let mut mutated = bytes.clone();
        mutated[range.start] ^= 0x01;
        assert_ne!(
            crate::sha256::digest(&mutated),
            baseline,
            "canonical field {field} is not content-bound",
        );
    }
}

fn decode_fields(bytes: &[u8]) -> Vec<&[u8]> {
    field_payload_ranges(bytes)
        .into_iter()
        .map(|range| &bytes[range])
        .collect()
}

fn field_payload_ranges(bytes: &[u8]) -> Vec<core::ops::Range<usize>> {
    let mut fields = Vec::new();
    let mut cursor = 0;
    while cursor < bytes.len() {
        let length_end = cursor + 8;
        let length = u64::from_be_bytes(bytes[cursor..length_end].try_into().unwrap()) as usize;
        let start = length_end;
        let end = start + length;
        assert!(end <= bytes.len());
        fields.push(start..end);
        cursor = end;
    }
    assert_eq!(cursor, bytes.len());
    fields
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}
