//! Semantic contract точного конечного family image.
//!
//! Opaque [`FamilyId`] маршрутизирует клиентский граф, representation-independent
//! semantic release связывает definition и канонический image, а membership
//! measurement называет проверенный [`ColorSignal`]. Transport, proof и verifier
//! admission принадлежат только `family_artifact` и не входят в Program identity.

use crate::lcs_occurrence::{ColorSignal, OutputProfileId};
use crate::sha256::Hasher;

#[cfg(test)]
pub(crate) use assess_counter::FAMILY_MEMBERSHIP_ASSESS_CALLS;

#[cfg(test)]
mod assess_counter {
    std::thread_local! {
        pub(crate) static FAMILY_MEMBERSHIP_ASSESS_CALLS: core::cell::Cell<usize> =
            const { core::cell::Cell::new(0) };
    }
}

#[cfg(test)]
const FAMILY_DEFINITION_DOMAIN_V2: &[u8] = b"labcolors.family-definition.v2\0";
const FAMILY_IMAGE_DOMAIN_V2: &[u8] = b"labcolors.family-canonical-image.v2\0";
const FAMILY_SEMANTIC_RELEASE_DOMAIN_V2: &[u8] = b"labcolors.family-semantic-release.v2\0";
// Image и semantic preimage развиваются независимо; общий literal мог бы
// незаметно переиспользовать старый content address при изменении только одного.
const FAMILY_IMAGE_ENCODING_RELEASE_V2: u8 = 2;
const FAMILY_SEMANTIC_ENCODING_RELEASE_V2: u8 = 2;
// Канонические кодеки пишут длины как u64. Этот закон платформы делает каждое
// usize → u64 преобразование точным вместо ложной runtime-ветки ошибки.
const _: () = assert!(usize::BITS <= u64::BITS);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum OutputProfileTag {
    // Дискриминант принадлежит canonical codec family V2; новый output profile
    // требует нового tag, sensitivity vectors и явного решения, может ли один
    // family-set вообще содержать сигналы разных профилей.
    Iec61966Srgb8D65V1 = 1,
}

const fn output_profile_tag(profile: OutputProfileId) -> OutputProfileTag {
    match profile {
        OutputProfileId::Iec61966Srgb8D65V1 => OutputProfileTag::Iec61966Srgb8D65V1,
    }
}

/// Непрозрачный клиентский ключ одного объявленного family-set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FamilyId(u32);

impl FamilyId {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> u32 {
        self.0
    }
}

/// Контентный адрес определения provider-а до его точного конечного образа.
///
/// Context, transform и параметры будущего provider-а входят в этот digest;
/// storage codec и opaque [`FamilyId`] не входят.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FamilyDefinitionDigestV2([u8; 32]);

impl FamilyDefinitionDigestV2 {
    pub(crate) const fn from_digest(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    #[cfg(test)]
    pub(crate) fn from_fixture_bytes_v2(bytes: &[u8]) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(FAMILY_DEFINITION_DOMAIN_V2);
        hasher.update(bytes);
        Self(*hasher.finalize().as_bytes())
    }
}

/// Representation-independent адрес канонического точного множества сигналов.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct CanonicalFamilyImageDigestV2([u8; 32]);

impl CanonicalFamilyImageDigestV2 {
    pub(crate) const fn from_digest(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Семантический release family: provider definition + его точный образ.
///
/// Два lossless artifact codec-а одного release имеют этот же ID, но разные
/// artifact receipts. Opaque client ID также не участвует в адресе.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SemanticFamilyReleaseIdV2([u8; 32]);

impl SemanticFamilyReleaseIdV2 {
    pub(crate) const fn from_digest(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Program-декларация связывает opaque ID только с semantic release, не с bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyDeclarationV2 {
    id: FamilyId,
    semantic: SemanticFamilyReleaseIdV2,
}

impl FamilyDeclarationV2 {
    pub(crate) const fn new(id: FamilyId, semantic: SemanticFamilyReleaseIdV2) -> Self {
        Self { id, semantic }
    }

    pub(crate) const fn id(self) -> FamilyId {
        self.id
    }

    pub(crate) const fn semantic(self) -> SemanticFamilyReleaseIdV2 {
        self.semantic
    }
}

/// Membership evidence связывает semantic release и проверенный сигнал.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyMembershipMeasurementV2 {
    semantic: SemanticFamilyReleaseIdV2,
    signal: ColorSignal,
}

impl FamilyMembershipMeasurementV2 {
    pub(crate) const fn new(semantic: SemanticFamilyReleaseIdV2, signal: ColorSignal) -> Self {
        Self { semantic, signal }
    }

    pub(crate) const fn semantic(self) -> SemanticFamilyReleaseIdV2 {
        self.semantic
    }

    pub(crate) const fn signal(self) -> ColorSignal {
        self.signal
    }
}

#[cfg(test)]
pub(crate) fn canonical_family_image_digest_v2(
    output_profile: OutputProfileId,
    members: &[ColorSignal],
) -> Result<CanonicalFamilyImageDigestV2, CanonicalFamilyImageErrorV2> {
    canonical_family_image_digest_from_ordered_members_v2(
        output_profile,
        members.len() as u64,
        members.iter().copied(),
    )
    .map_err(|_| CanonicalFamilyImageErrorV2::NonCanonicalAdmittedImage)
}

/// Один canonical preimage для slice-oracle и streaming artifact codec-ов.
///
/// `expected_count` входит в preimage до самих members, поэтому
/// расхождение с фактическим потоком является отдельным
/// certificate violation, а не другим образом того же множества.
pub(crate) fn canonical_family_image_digest_from_ordered_members_v2(
    output_profile: OutputProfileId,
    expected_count: u64,
    members: impl IntoIterator<Item = ColorSignal>,
) -> Result<CanonicalFamilyImageDigestV2, CanonicalFamilyImageStreamErrorV2> {
    let mut hasher = Hasher::new();
    hasher.update(FAMILY_IMAGE_DOMAIN_V2);
    hasher.update(&[
        FAMILY_IMAGE_ENCODING_RELEASE_V2,
        output_profile_tag(output_profile) as u8,
    ]);
    hasher.update(&expected_count.to_be_bytes());
    let mut previous = None;
    let mut actual_count = 0_u64;
    for member in members {
        let key = family_image_member_key(member);
        if member.output_profile() != output_profile
            || previous.is_some_and(|previous| previous >= key)
        {
            return Err(CanonicalFamilyImageStreamErrorV2::NonCanonicalAdmittedImage);
        }
        actual_count = actual_count
            .checked_add(1)
            .ok_or(CanonicalFamilyImageStreamErrorV2::NonCanonicalAdmittedImage)?;
        hasher.update(&[output_profile_tag(member.output_profile()) as u8]);
        hasher.update(&member.srgb8().bytes());
        previous = Some(key);
    }
    if actual_count != expected_count {
        return Err(CanonicalFamilyImageStreamErrorV2::MemberCountMismatch {
            expected: expected_count,
            actual: actual_count,
        });
    }
    Ok(CanonicalFamilyImageDigestV2(*hasher.finalize().as_bytes()))
}

fn family_image_member_key(member: ColorSignal) -> (u8, [u8; 3]) {
    (
        output_profile_tag(member.output_profile()) as u8,
        member.srgb8().bytes(),
    )
}

pub(crate) fn semantic_family_release_id_v2(
    definition: FamilyDefinitionDigestV2,
    image: CanonicalFamilyImageDigestV2,
    member_count: u64,
) -> SemanticFamilyReleaseIdV2 {
    let mut hasher = Hasher::new();
    hasher.update(FAMILY_SEMANTIC_RELEASE_DOMAIN_V2);
    hasher.update(&[FAMILY_SEMANTIC_ENCODING_RELEASE_V2]);
    hasher.update(definition.as_bytes());
    hasher.update(image.as_bytes());
    hasher.update(&member_count.to_be_bytes());
    SemanticFamilyReleaseIdV2(*hasher.finalize().as_bytes())
}

/// Membership proof is intentionally empty: semantic release and queried
/// signal live in measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyMembershipPassV1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyMembershipViolationV1;

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CanonicalFamilyImageErrorV2 {
    NonCanonicalAdmittedImage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CanonicalFamilyImageStreamErrorV2 {
    NonCanonicalAdmittedImage,
    MemberCountMismatch { expected: u64, actual: u64 },
}
