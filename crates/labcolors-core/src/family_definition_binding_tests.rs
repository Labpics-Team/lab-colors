//! Certificate допускается только к тому определению, которое спрошено.

use core::cell::Cell;

use crate::Srgb8;
use crate::contextual_region::{
    ContextualRegionFamilyProviderV1, ContextualRegionPipelineV1, PiecewiseLinearCartesianTubeV1,
};
use crate::contextual_region_tests::{
    ONE, POSITIVE_ZERO, TWO, context, pipeline, region_with_centers,
};
use crate::family_artifact::{
    EncodedFamilyArtifactV2, FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS, FamilyArtifactLoadErrorV1,
    FamilyArtifactLoaderV1, FamilyImageCertificateV2,
    encode_raw_bitmap24_family_artifact_v2_for_test,
};
use crate::family_definition_binding::{
    DefinitionBoundFamilyLoadErrorV1, DefinitionBoundFamilyLoaderV1,
};
use crate::lcs_occurrence::{ColorSignal, IEC_SRGB_D65_XYZ_FRAME_V1};

fn asked_pipeline() -> ContextualRegionPipelineV1 {
    pipeline(context(IEC_SRGB_D65_XYZ_FRAME_V1))
}

/// Регионы отличаются только центрами узлов: геометрия семейства другая, а
/// весь остальной pipeline тот же.
fn asked_region() -> PiecewiseLinearCartesianTubeV1 {
    region_with_centers([[POSITIVE_ZERO; 2]; 2])
}

fn other_region() -> PiecewiseLinearCartesianTubeV1 {
    region_with_centers([[ONE, TWO], [TWO, ONE]])
}

fn members() -> Vec<ColorSignal> {
    [[0, 0, 1], [0, 0, 2], [0, 0, 255]]
        .into_iter()
        .map(Srgb8::new)
        .map(ColorSignal::from_srgb8)
        .collect()
}

/// Внешний registry поставляет доверенный certificate образа именно этого
/// региона вместе с его bytes.
fn artifact_of(
    region: &PiecewiseLinearCartesianTubeV1,
) -> (FamilyImageCertificateV2, EncodedFamilyArtifactV2) {
    let definition = ContextualRegionFamilyProviderV1::definition_digest(asked_pipeline(), region);
    encode_raw_bitmap24_family_artifact_v2_for_test(definition, &members()).unwrap()
}

#[test]
fn the_artifact_of_the_asked_region_is_admitted() {
    let region = asked_region();
    let (certificate, encoded) = artifact_of(&region);

    let admitted =
        DefinitionBoundFamilyLoaderV1::load(asked_pipeline(), &region, certificate, encoded)
            .unwrap();

    assert_eq!(admitted.semantic_release(), certificate.semantic_release());
    assert_eq!(admitted.artifact_receipt(), certificate.artifact_receipt());
    for member in members() {
        assert!(admitted.contains(member), "missing member {member:?}");
    }
}

#[test]
fn an_intact_artifact_of_another_region_is_refused_by_type() {
    let asked = asked_region();
    let other = other_region();
    let (certificate, encoded) = artifact_of(&other);
    let allocation = encoded.allocation_ptr_for_test();

    let failure =
        DefinitionBoundFamilyLoaderV1::load(asked_pipeline(), &asked, certificate, encoded)
            .unwrap_err();

    assert_eq!(
        failure.cause(),
        DefinitionBoundFamilyLoadErrorV1::ForeignDefinition {
            asked: ContextualRegionFamilyProviderV1::definition_digest(asked_pipeline(), &asked),
            certified: ContextualRegionFamilyProviderV1::definition_digest(
                asked_pipeline(),
                &other,
            ),
        },
    );
    let (_, returned) = failure.into_parts();
    assert_eq!(returned.allocation_ptr_for_test(), allocation);
    // Отвергнут не повреждённый artifact: те же bytes и тот же certificate
    // безупречны для transport-границы, они лишь про другое определение.
    FamilyArtifactLoaderV1::load(certificate, returned).unwrap();
}

#[test]
fn a_foreign_definition_is_refused_before_any_payload_work() {
    let asked = asked_region();
    let (foreign_certificate, foreign_encoded) = artifact_of(&other_region());
    let (asked_certificate, asked_encoded) = artifact_of(&asked);

    FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS.with(|calls| calls.set(0));
    let failure = DefinitionBoundFamilyLoaderV1::load(
        asked_pipeline(),
        &asked,
        foreign_certificate,
        foreign_encoded,
    )
    .unwrap_err();
    let refused_cost = FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS.with(Cell::get);

    assert!(matches!(
        failure.cause(),
        DefinitionBoundFamilyLoadErrorV1::ForeignDefinition { .. },
    ));
    // Payload digest — первая дорогая работа над 2 MiB; decode образа идёт
    // строго после него, поэтому ноль здесь закрывает обе стадии.
    assert_eq!(
        refused_cost, 0,
        "foreign definition must be refused on the record, before payload work",
    );

    // Anti-vacuity: счётчик действительно наблюдает дорогую работу, когда
    // адрес совпал.
    DefinitionBoundFamilyLoaderV1::load(asked_pipeline(), &asked, asked_certificate, asked_encoded)
        .unwrap();
    assert_eq!(FAMILY_ARTIFACT_PAYLOAD_DIGEST_CALLS.with(Cell::get), 1);
}

#[test]
fn the_asked_definition_does_not_weaken_the_transport_contract() {
    let region = asked_region();
    let (certificate, mut encoded) = artifact_of(&region);
    encoded.flip_first_payload_bit_for_test();

    let failure =
        DefinitionBoundFamilyLoaderV1::load(asked_pipeline(), &region, certificate, encoded)
            .unwrap_err();

    assert_eq!(
        failure.cause(),
        DefinitionBoundFamilyLoadErrorV1::Artifact(
            FamilyArtifactLoadErrorV1::PayloadDigestMismatch,
        ),
    );
}
