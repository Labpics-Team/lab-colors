//! Первый семейный артефакт: контракт-фикстура штатного пути допуска.
//!
//! Образ вырожден намеренно — в нём ровно одна точка. Артефакт доказывает
//! конвейер «доверенная запись -> загрузчик -> членство», а не описывает
//! продуктовое семейство: перечня семейств ядро не содержит и не должно.
//!
//! В гит не кладутся ни 2 МиБ образа, ни 254 байта записи. И то и другое
//! детерминированно строится здесь из списка ординалов, а контрактом остаются
//! инварианты: адрес определения, semantic release и два дайджеста над
//! построенными байтами. Мегабайты не являются контрактом — числа являются.
//!
//! Границы доказательства. Ядро адресует определение, но не минтит его образ
//! (`ContextualRegionFamilyProviderV1` — «definition address без mint image»),
//! поэтому здесь **не** доказывается, что образ региона `0a8d1c3d…` на всём
//! sRGB8 равен именно `{RGB(1,1,1)}`; это измерение внешнего трека
//! доказательств. Доказывается ровно то, что заявленный образ проходит
//! штатный путь предъявления и допуска без искажения.

use crate::Srgb8;
use crate::contextual_region::{
    CONTEXTUAL_REGION_FORMULA_RELEASE_V1, ContextualRegionFamilyProviderV1,
    ContextualRegionPipelineV1, PiecewiseLinearCartesianTubeV1, Shape2BitsV1, TubeKnotBitsV1,
};
use crate::family::FamilyDefinitionDigestV2;
use crate::family_artifact::{
    EncodedFamilyArtifactV2, FAMILY_CERTIFICATE_RECORD_LEN_V2, FamilyArtifactLoaderV1,
    FamilyImageCertificateV2, RAW_BITMAP24_PAYLOAD_LEN_V1,
    encode_raw_bitmap24_family_artifact_v2_for_test,
};
use crate::lcs_occurrence::{
    ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1, AdaptingLuminanceCdM2, AppearanceContextId,
    AppearanceContextSchemaReleaseId, BackgroundLuminanceRatio, CAM16_UCS_VIEW_RELEASE_V1,
    CAM16_VIEW_RELEASE_V1, ColorSignal, IEC_SRGB_D65_XYZ_FRAME_V1,
    MODELED_LCS_OCCURRENCE_RELEASE_V1, OutputProfileId, SurroundProfileId,
};

/// Сырые binary64-биты параметров региона: точное значение, а не литерал `f64`.
const POSITIVE_ZERO: u64 = 0x0000_0000_0000_0000;
const ONE: u64 = 0x3ff0_0000_0000_0000;
const TWO: u64 = 0x4000_0000_0000_0000;
const FOUR: u64 = 0x4010_0000_0000_0000;

/// Единственный член образа, заданный ординалом — он же SSOT содержимого.
///
/// RGB выводится из ординала арифметикой теста, а не берётся из кодека:
/// тест обязан иметь собственный оракул раскладки, иначе он согласится с
/// любым сдвигом, который выберет кодек.
const FIRST_IMAGE_MEMBER_ORDINALS_V1: [u32; 1] = [65_793];

/// Адрес определения, образом которого объявлен артефакт.
///
/// Те же 32 байта хеширует внешний трек доказательств из канонической строки
/// `proof/region/v1/fixtures/v5b2b-definition-0a8d1c3d.bin`: значение здесь
/// выводится из построенного региона, а не переписывается из файла.
const FIRST_IMAGE_DEFINITION_DIGEST_V1: &str =
    "0a8d1c3d2f0052be84b5783071699861aad0ac83dae62de3275267754681cdc9";

/// Semantic release: определение + его точный образ + мощность образа.
const FIRST_IMAGE_SEMANTIC_RELEASE_V1: &str =
    "d53186591060e7141523b72c88a358e07f1d9572af3432ca9d5ed09776518145";

/// SHA-256 доверенной 254-байтной записи сертификата.
const FIRST_IMAGE_RECORD_SHA256_V1: &str =
    "78290bbb2caf78471279b3f02422e5e187a14bf09d50d3f2f694240934fbb328";

/// SHA-256 всего артефакта: запись плюс 2 МиБ payload.
const FIRST_IMAGE_ARTIFACT_SHA256_V1: &str =
    "0e628aadfc90e1c6130627b0c97e2e82eb2e78c4d3c52bffd931617f946ba8a9";

/// Точки, которые обязаны остаться вне образа.
///
/// Правило выборки: точка попадает сюда, только если один правдоподобный
/// дефект индексации отобразил бы её на единственного члена. Это соседи по
/// ординалу (маска бита внутри байта), соседи через границу байта (индекс
/// байта), шесть соседей по одному каналу (константы сдвига каналов) и два
/// угла домена (сплошное членство). Полный обход 16 777 216 точек не
/// различил бы ничего сверх этого: утверждение «во всём домене ровно один
/// член» несёт `member_count`, который загрузчик пересчитывает по всему
/// payload прежде, чем допустить артефакт.
const FIRST_IMAGE_NON_MEMBER_SAMPLE_V1: [(u32, &str); 10] = [
    (65_792, "ординал -1: маска бита внутри того же байта"),
    (65_794, "ординал +1: маска бита внутри того же байта"),
    (65_791, "предыдущий байт payload"),
    (65_800, "следующий байт payload"),
    (257, "красный -1: сдвиг канала"),
    (131_329, "красный +1: сдвиг канала"),
    (65_537, "зелёный -1: сдвиг канала"),
    (66_049, "зелёный +1: сдвиг канала"),
    (0, "нижний угол домена"),
    (16_777_215, "верхний угол домена"),
];

/// Регион, чей образ несёт артефакт: тот же, что адресует внешний трек.
fn first_image_region() -> PiecewiseLinearCartesianTubeV1 {
    PiecewiseLinearCartesianTubeV1::try_from_bits(
        Shape2BitsV1::new(ONE, POSITIVE_ZERO, ONE),
        &[
            TubeKnotBitsV1::new(ONE, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
            TubeKnotBitsV1::new(TWO, POSITIVE_ZERO, POSITIVE_ZERO, FOUR),
        ],
    )
    .expect("регион фикстуры обязан быть допустимым")
}

fn first_image_pipeline() -> ContextualRegionPipelineV1 {
    let context = AppearanceContextId::from_inputs(
        AppearanceContextSchemaReleaseId::Ciecam16ViewingInputsV1,
        IEC_SRGB_D65_XYZ_FRAME_V1,
        AdaptingLuminanceCdM2::try_new(64.0).expect("adapting luminance фикстуры допустима"),
        BackgroundLuminanceRatio::try_new(0.2).expect("background ratio фикстуры допустим"),
        SurroundProfileId::AverageV1,
    );
    ContextualRegionPipelineV1::try_new(
        OutputProfileId::Iec61966Srgb8D65V1,
        ADMITTED_SRGB8_TRISTIMULUS_BINDING_V1,
        MODELED_LCS_OCCURRENCE_RELEASE_V1,
        context,
        CAM16_VIEW_RELEASE_V1,
        CAM16_UCS_VIEW_RELEASE_V1,
        CONTEXTUAL_REGION_FORMULA_RELEASE_V1,
    )
    .expect("pipeline фикстуры обязан быть согласованным")
}

fn first_image_definition() -> FamilyDefinitionDigestV2 {
    ContextualRegionFamilyProviderV1::definition_digest(
        first_image_pipeline(),
        &first_image_region(),
    )
}

/// Ординал -> сигнал: собственный оракул раскладки RGB big-endian.
fn signal_from_ordinal(ordinal: u32) -> ColorSignal {
    assert!(ordinal < 1 << 24, "ординал вне sRGB8: {ordinal}");
    ColorSignal::from_srgb8(Srgb8::new([
        (ordinal >> 16) as u8,
        (ordinal >> 8) as u8,
        ordinal as u8,
    ]))
}

/// Строит артефакт из списка ординалов — источника истины о содержимом.
fn build_image(ordinals: &[u32]) -> (FamilyImageCertificateV2, EncodedFamilyArtifactV2) {
    let members: Vec<ColorSignal> = ordinals.iter().copied().map(signal_from_ordinal).collect();
    encode_raw_bitmap24_family_artifact_v2_for_test(first_image_definition(), &members)
        .expect("образ фикстуры кодируется")
}

fn build_first_image() -> (FamilyImageCertificateV2, EncodedFamilyArtifactV2) {
    build_image(&FIRST_IMAGE_MEMBER_ORDINALS_V1)
}

/// Отделяет доверенную запись от артефакта, не копируя payload дважды.
fn split_record(encoded: EncodedFamilyArtifactV2) -> (Vec<u8>, EncodedFamilyArtifactV2) {
    let bytes = encoded.into_bytes();
    let record = bytes[..FAMILY_CERTIFICATE_RECORD_LEN_V2].to_vec();
    (record, EncodedFamilyArtifactV2::from_owned_bytes(bytes))
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

#[test]
fn first_family_image_admits_its_only_member_through_the_trusted_record() {
    let (built, encoded) = build_first_image();
    let (record, encoded) = split_record(encoded);

    // Штатный путь: потребитель приносит доверенную запись, ядро разбирает
    // её отдельно от артефакта и только потом допускает байты образа.
    let presented =
        FamilyImageCertificateV2::parse_trusted(&record).expect("доверенная запись разбирается");
    assert_eq!(presented, built, "запись обязана нести тот же сертификат");

    let admitted = FamilyArtifactLoaderV1::load(presented, encoded).expect("артефакт допускается");

    assert!(
        admitted.contains(signal_from_ordinal(FIRST_IMAGE_MEMBER_ORDINALS_V1[0])),
        "единственный член образа обязан быть членом",
    );
    // Загрузчик пересчитал popcount по всему payload и потребовал равенства:
    // после успешного допуска это утверждение о всём домене, а не о выборке.
    assert_eq!(
        presented.member_count(),
        FIRST_IMAGE_MEMBER_ORDINALS_V1.len() as u64,
        "во всём sRGB8 обязан быть ровно один член",
    );
}

#[test]
fn first_family_image_refuses_every_point_a_single_indexing_defect_could_alias() {
    let (built, encoded) = build_first_image();
    let (record, encoded) = split_record(encoded);
    let presented =
        FamilyImageCertificateV2::parse_trusted(&record).expect("доверенная запись разбирается");
    assert_eq!(presented, built);
    let admitted = FamilyArtifactLoaderV1::load(presented, encoded).expect("артефакт допускается");

    for (ordinal, reason) in FIRST_IMAGE_NON_MEMBER_SAMPLE_V1 {
        assert_ne!(
            ordinal, FIRST_IMAGE_MEMBER_ORDINALS_V1[0],
            "выборка не должна содержать самого члена",
        );
        assert!(
            !admitted.contains(signal_from_ordinal(ordinal)),
            "ординал {ordinal} обязан остаться вне образа ({reason})",
        );
    }
}

#[test]
fn the_only_member_bit_sits_at_the_declared_ordinal() {
    let (_, encoded) = build_first_image();
    let bytes = encoded.into_bytes();
    let payload = &bytes[FAMILY_CERTIFICATE_RECORD_LEN_V2..];
    assert_eq!(payload.len(), RAW_BITMAP24_PAYLOAD_LEN_V1);

    let ordinal = FIRST_IMAGE_MEMBER_ORDINALS_V1[0] as usize;
    let member_byte = ordinal >> 3;
    let member_mask = 0x80_u8 >> (ordinal & 7);
    for (index, byte) in payload.iter().copied().enumerate() {
        let expected = if index == member_byte { member_mask } else { 0 };
        assert_eq!(byte, expected, "байт payload {index}");
    }
}

#[test]
fn the_first_family_image_keeps_its_published_invariants() {
    let (built, encoded) = build_first_image();
    let (record, encoded) = split_record(encoded);
    let bytes = encoded.into_bytes();

    assert_eq!(record.len(), FAMILY_CERTIFICATE_RECORD_LEN_V2);
    assert_eq!(
        bytes.len(),
        FAMILY_CERTIFICATE_RECORD_LEN_V2 + RAW_BITMAP24_PAYLOAD_LEN_V1,
    );
    assert_eq!(
        hex(built.definition_digest().as_bytes()),
        FIRST_IMAGE_DEFINITION_DIGEST_V1,
    );
    assert_eq!(
        hex(built.semantic_release().as_bytes()),
        FIRST_IMAGE_SEMANTIC_RELEASE_V1,
    );
    assert_eq!(
        hex(crate::sha256::digest(&record).as_bytes()),
        FIRST_IMAGE_RECORD_SHA256_V1,
    );
    assert_eq!(
        hex(crate::sha256::digest(&bytes).as_bytes()),
        FIRST_IMAGE_ARTIFACT_SHA256_V1,
    );
}

#[test]
fn every_single_byte_corruption_of_the_trusted_record_is_refused() {
    let (_, encoded) = build_first_image();
    let (record, _) = split_record(encoded);

    for index in 0..record.len() {
        let mut corrupted = record.clone();
        corrupted[index] ^= 0x01;
        assert!(
            FamilyImageCertificateV2::parse_trusted(&corrupted).is_err(),
            "испорченный байт записи {index} обязан быть отвергнут",
        );
    }
}

#[test]
fn another_ordinal_moves_every_published_invariant_but_not_the_definition() {
    let (first, first_encoded) = build_first_image();
    let (first_record, first_encoded) = split_record(first_encoded);
    let first_bytes = first_encoded.into_bytes();

    let other_ordinal = FIRST_IMAGE_NON_MEMBER_SAMPLE_V1[0].0;
    let (other, other_encoded) = build_image(&[other_ordinal]);
    let (other_record, other_encoded) = split_record(other_encoded);
    let other_bytes = other_encoded.into_bytes();

    // Артефакт остаётся образом того же определения: двигается содержимое.
    assert_eq!(other.definition_digest(), first.definition_digest());
    assert_ne!(other.semantic_release(), first.semantic_release());
    assert_ne!(other.image_digest(), first.image_digest());
    assert_ne!(other_record, first_record);
    assert_ne!(other_bytes, first_bytes);

    // Именно эти четыре величины опубликованы как контракт: каждая обязана
    // покраснеть при подмене ординала, иначе golden ничего не удерживает.
    assert_ne!(
        hex(other.semantic_release().as_bytes()),
        FIRST_IMAGE_SEMANTIC_RELEASE_V1,
    );
    assert_ne!(
        hex(crate::sha256::digest(&other_record).as_bytes()),
        FIRST_IMAGE_RECORD_SHA256_V1,
    );
    assert_ne!(
        hex(crate::sha256::digest(&other_bytes).as_bytes()),
        FIRST_IMAGE_ARTIFACT_SHA256_V1,
    );
    assert_eq!(
        hex(other.definition_digest().as_bytes()),
        FIRST_IMAGE_DEFINITION_DIGEST_V1,
    );
}
