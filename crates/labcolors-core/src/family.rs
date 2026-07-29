//! Точный конечный образ versioned family-generator-а.
//!
//! Family здесь — множество физических [`ColorSignal`], а не роль, оттенок или
//! обещание визуальной чистоты. Объявленное множество полно по определению;
//! независимо вычисленный образ допускается только после исчерпывающего
//! равенства канонических множеств. SHA-256 адресует весь проверенный объект.

#[cfg(test)]
use crate::Srgb8;
use crate::constraints::HardDecision;
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

const GENERATOR_DOMAIN_V1: &[u8] = b"labcolors.family-generator-content-identity.v1\0";
const GENERATOR_PARAMETERS_DOMAIN_V1: &[u8] = b"labcolors.family-generator-parameters.v1\0";
const IMAGE_DOMAIN_V1: &[u8] = b"labcolors.family-image-identity.v1\0";
const FAMILY_DOMAIN_V1: &[u8] = b"labcolors.family-certificate-content-identity.v1\0";
const GENERATOR_PARAMETERS_CODEC_V1: u8 = 1;
const IMAGE_CODEC_V1: u8 = 1;
const FAMILY_CERTIFICATE_CODEC_V1: u8 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum OutputProfileTagV1 {
    // Дискриминант принадлежит canonical codec family V1; новый output profile
    // требует нового tag, sensitivity vectors и явного решения, может ли один
    // family-set вообще содержать сигналы разных профилей.
    Iec61966Srgb8D65V1 = 1,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FamilyGeneratorContentIdentityV1([u8; 32]);

impl FamilyGeneratorContentIdentityV1 {
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FamilyImageContentIdentityV1([u8; 32]);

impl FamilyImageContentIdentityV1 {
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct FamilyContentIdentityV1([u8; 32]);

impl FamilyContentIdentityV1 {
    pub(crate) const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyGeneratorReleaseV1 {
    DeclaredFiniteImageV1,
    #[cfg(test)]
    EncodedSrgb8EqualChannelAxisV1,
    #[cfg(test)]
    EncodedSrgb8RedBlueDiagonalV1,
    #[cfg(test)]
    NonInjectiveUnorderedFixtureV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FamilyImageProofReleaseV1 {
    DeclaredImageIsDefinitionV1,
    #[cfg(test)]
    ExhaustiveCanonicalImageComparisonV1,
}

/// Закрытое определение генератора с конечным каноническим доменом.
///
/// Технические оси существуют только как proof fixtures. Production-вариант
/// определяет ровно объявленный конечный образ и не приписывает ему human meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CompleteFamilyGeneratorV1 {
    DeclaredFiniteImageV1 {
        definition: Vec<ColorSignal>,
    },
    #[cfg(test)]
    EncodedSrgb8EqualChannelAxisV1,
    #[cfg(test)]
    EncodedSrgb8RedBlueDiagonalV1,
    #[cfg(test)]
    NonInjectiveUnorderedFixtureV1 {
        permuted: bool,
        provenance: u8,
    },
}

/// Повторяемое определение генератора, сохранённое каждым сертификатом.
///
/// У declared-image каноническое множество само является определением. Будущие
/// параметрические providers обязаны хранить здесь все свои точные входы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FamilyGeneratorDescriptorV1 {
    DeclaredFiniteImageV1,
    #[cfg(test)]
    EncodedSrgb8EqualChannelAxisV1,
    #[cfg(test)]
    EncodedSrgb8RedBlueDiagonalV1,
    #[cfg(test)]
    NonInjectiveUnorderedFixtureV1 {
        permuted: bool,
        provenance: u8,
    },
}

impl CompleteFamilyGeneratorV1 {
    pub(crate) fn try_declared_finite_image_v1(
        mut definition: Vec<ColorSignal>,
    ) -> Result<Self, FamilyImageErrorV1> {
        canonicalize(&mut definition);
        if definition.is_empty() {
            return Err(FamilyImageErrorV1::EmptyGeneratorDomain);
        }
        Ok(Self::DeclaredFiniteImageV1 { definition })
    }

    #[cfg(test)]
    pub(crate) const fn encoded_srgb8_equal_channel_axis_v1() -> Self {
        Self::EncodedSrgb8EqualChannelAxisV1
    }

    #[cfg(test)]
    pub(crate) const fn encoded_srgb8_red_blue_diagonal_v1() -> Self {
        Self::EncodedSrgb8RedBlueDiagonalV1
    }

    #[cfg(test)]
    pub(crate) const fn noninjective_unordered_fixture_v1() -> Self {
        Self::NonInjectiveUnorderedFixtureV1 {
            permuted: false,
            provenance: 0,
        }
    }

    #[cfg(test)]
    pub(crate) const fn permuted_noninjective_unordered_fixture_v1() -> Self {
        Self::NonInjectiveUnorderedFixtureV1 {
            permuted: true,
            provenance: 0,
        }
    }

    #[cfg(test)]
    pub(crate) const fn noninjective_fixture_with_provenance_v1(provenance: u8) -> Self {
        Self::NonInjectiveUnorderedFixtureV1 {
            permuted: false,
            provenance,
        }
    }

    /// Перечисляет полный ordinal-образ генератора до превращения его в множество.
    ///
    /// У ordinal-generator-а порядок и повторы принадлежат identity генератора;
    /// следующий шаг отдельно канонизирует множество для membership.
    /// Declared-image уже определён как каноническое множество, поэтому порядок
    /// и повторы входной записи являются лишь представлением.
    pub(crate) fn into_complete_output(
        self,
    ) -> Result<UnverifiedFamilyImageV1, FamilyImageErrorV1> {
        let members = match self {
            Self::DeclaredFiniteImageV1 { definition } => definition,
            #[cfg(test)]
            Self::EncodedSrgb8EqualChannelAxisV1 => {
                let mut members = Vec::new();
                members
                    .try_reserve_exact(256)
                    .map_err(|_| FamilyImageErrorV1::ResourceExhausted)?;
                members.extend((0_u16..=255).map(|value| {
                    let value = value as u8;
                    ColorSignal::from_srgb8(Srgb8::new([value; 3]))
                }));
                members
            }
            #[cfg(test)]
            Self::EncodedSrgb8RedBlueDiagonalV1 => {
                let mut members = Vec::new();
                members
                    .try_reserve_exact(256)
                    .map_err(|_| FamilyImageErrorV1::ResourceExhausted)?;
                members.extend((0_u16..=255).map(|value| {
                    let value = value as u8;
                    ColorSignal::from_srgb8(Srgb8::new([value, 0, 255 - value]))
                }));
                members
            }
            #[cfg(test)]
            Self::NonInjectiveUnorderedFixtureV1 { permuted, .. } => {
                let mut members = Vec::new();
                members
                    .try_reserve_exact(4)
                    .map_err(|_| FamilyImageErrorV1::ResourceExhausted)?;
                let (first, second) = if permuted { (10, 20) } else { (20, 10) };
                members.extend([
                    ColorSignal::from_srgb8(Srgb8::new([first; 3])),
                    ColorSignal::from_srgb8(Srgb8::new([second; 3])),
                    ColorSignal::from_srgb8(Srgb8::new([first; 3])),
                    ColorSignal::from_srgb8(Srgb8::new([second; 3])),
                ]);
                members
            }
        };
        Ok(UnverifiedFamilyImageV1 { members })
    }

    const fn release(&self) -> FamilyGeneratorReleaseV1 {
        match self {
            Self::DeclaredFiniteImageV1 { .. } => FamilyGeneratorReleaseV1::DeclaredFiniteImageV1,
            #[cfg(test)]
            Self::EncodedSrgb8EqualChannelAxisV1 => {
                FamilyGeneratorReleaseV1::EncodedSrgb8EqualChannelAxisV1
            }
            #[cfg(test)]
            Self::EncodedSrgb8RedBlueDiagonalV1 => {
                FamilyGeneratorReleaseV1::EncodedSrgb8RedBlueDiagonalV1
            }
            #[cfg(test)]
            Self::NonInjectiveUnorderedFixtureV1 { .. } => {
                FamilyGeneratorReleaseV1::NonInjectiveUnorderedFixtureV1
            }
        }
    }

    const fn descriptor(&self) -> FamilyGeneratorDescriptorV1 {
        match self {
            Self::DeclaredFiniteImageV1 { .. } => {
                FamilyGeneratorDescriptorV1::DeclaredFiniteImageV1
            }
            #[cfg(test)]
            Self::EncodedSrgb8EqualChannelAxisV1 => {
                FamilyGeneratorDescriptorV1::EncodedSrgb8EqualChannelAxisV1
            }
            #[cfg(test)]
            Self::EncodedSrgb8RedBlueDiagonalV1 => {
                FamilyGeneratorDescriptorV1::EncodedSrgb8RedBlueDiagonalV1
            }
            #[cfg(test)]
            Self::NonInjectiveUnorderedFixtureV1 {
                permuted,
                provenance,
            } => FamilyGeneratorDescriptorV1::NonInjectiveUnorderedFixtureV1 {
                permuted: *permuted,
                provenance: *provenance,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnverifiedFamilyImageV1 {
    members: Vec<ColorSignal>,
}

impl UnverifiedFamilyImageV1 {
    #[cfg(test)]
    pub(crate) const fn new(members: Vec<ColorSignal>) -> Self {
        Self { members }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyImageCertificateV1 {
    generator_release: FamilyGeneratorReleaseV1,
    generator_content_identity: FamilyGeneratorContentIdentityV1,
    image_content_identity: FamilyImageContentIdentityV1,
    family_content_identity: FamilyContentIdentityV1,
    output_profile: OutputProfileId,
    proof_release: FamilyImageProofReleaseV1,
    preimage_count: u64,
    member_count: u64,
}

impl FamilyImageCertificateV1 {
    #[cfg(test)]
    pub(crate) const fn generator_release(self) -> FamilyGeneratorReleaseV1 {
        self.generator_release
    }

    #[cfg(test)]
    pub(crate) const fn generator_content_identity(self) -> FamilyGeneratorContentIdentityV1 {
        self.generator_content_identity
    }

    #[cfg(test)]
    pub(crate) const fn image_content_identity(self) -> FamilyImageContentIdentityV1 {
        self.image_content_identity
    }

    pub(crate) const fn family_content_identity(self) -> FamilyContentIdentityV1 {
        self.family_content_identity
    }

    #[cfg(test)]
    pub(crate) const fn proof_release(self) -> FamilyImageProofReleaseV1 {
        self.proof_release
    }

    #[cfg(test)]
    pub(crate) const fn preimage_count(self) -> u64 {
        self.preimage_count
    }

    #[cfg(test)]
    pub(crate) const fn member_count(self) -> u64 {
        self.member_count
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdmittedFamilySetV1 {
    generator: FamilyGeneratorDescriptorV1,
    certificate: FamilyImageCertificateV1,
    members: Vec<ColorSignal>,
}

impl AdmittedFamilySetV1 {
    pub(crate) const fn certificate(&self) -> FamilyImageCertificateV1 {
        self.certificate
    }

    pub(crate) fn assess(
        &self,
        signal: ColorSignal,
    ) -> (
        FamilyMembershipMeasurementV1,
        HardDecision<FamilyMembershipPassV1, FamilyMembershipViolationV1>,
    ) {
        #[cfg(test)]
        FAMILY_MEMBERSHIP_ASSESS_CALLS.with(|calls| calls.set(calls.get() + 1));
        let measurement = FamilyMembershipMeasurementV1 {
            family: self.certificate.family_content_identity,
            signal,
        };
        let decision = match self
            .members
            .binary_search_by_key(&canonical_signal_key(signal), |member| {
                canonical_signal_key(*member)
            }) {
            Ok(rank) => HardDecision::Pass(FamilyMembershipPassV1 { rank }),
            Err(insertion_rank) => HardDecision::Violation(FamilyMembershipViolationV1 {
                insertion_rank,
                lower: insertion_rank
                    .checked_sub(1)
                    .and_then(|rank| self.members.get(rank))
                    .copied(),
                upper: self.members.get(insertion_rank).copied(),
            }),
        };
        (measurement, decision)
    }

    /// Повторно связывает полный образ и все поля сертификата до компиляции.
    pub(crate) fn verify(&self) -> Result<(), FamilyImageErrorV1> {
        if self.members.is_empty() {
            return Err(FamilyImageErrorV1::EmptyGeneratorDomain);
        }
        if self
            .members
            .windows(2)
            .any(|pair| canonical_signal_key(pair[0]) >= canonical_signal_key(pair[1]))
        {
            return Err(FamilyImageErrorV1::NonCanonicalAdmittedImage);
        }
        let release = self.generator.release();
        if release != self.certificate.generator_release {
            return Err(FamilyImageErrorV1::CertificateMismatch);
        }
        let replay = self.generator.replay(&self.members)?;
        let (generator_content_identity, preimage_count) = match replay {
            GeneratorReplayV1::Declared(output) => (
                generator_content_identity_for(self.generator, output)?,
                u64::try_from(output.len()).map_err(|_| FamilyImageErrorV1::ResourceExhausted)?,
            ),
            #[cfg(test)]
            GeneratorReplayV1::Generated(mut output) => {
                let generator_content_identity =
                    generator_content_identity_for(self.generator, &output)?;
                let preimage_count = u64::try_from(output.len())
                    .map_err(|_| FamilyImageErrorV1::ResourceExhausted)?;
                canonicalize(&mut output);
                if first_set_mismatch(&output, &self.members) != (None, None) {
                    return Err(FamilyImageErrorV1::CertificateMismatch);
                }
                (generator_content_identity, preimage_count)
            }
        };
        let proof_is_admitted = match (
            self.certificate.generator_release,
            self.certificate.proof_release,
        ) {
            (
                FamilyGeneratorReleaseV1::DeclaredFiniteImageV1,
                FamilyImageProofReleaseV1::DeclaredImageIsDefinitionV1,
            ) => true,
            #[cfg(test)]
            (
                FamilyGeneratorReleaseV1::DeclaredFiniteImageV1
                | FamilyGeneratorReleaseV1::EncodedSrgb8EqualChannelAxisV1
                | FamilyGeneratorReleaseV1::EncodedSrgb8RedBlueDiagonalV1
                | FamilyGeneratorReleaseV1::NonInjectiveUnorderedFixtureV1,
                FamilyImageProofReleaseV1::ExhaustiveCanonicalImageComparisonV1,
            ) => true,
            #[cfg(test)]
            (
                FamilyGeneratorReleaseV1::EncodedSrgb8EqualChannelAxisV1
                | FamilyGeneratorReleaseV1::EncodedSrgb8RedBlueDiagonalV1
                | FamilyGeneratorReleaseV1::NonInjectiveUnorderedFixtureV1,
                FamilyImageProofReleaseV1::DeclaredImageIsDefinitionV1,
            ) => false,
        };
        if !proof_is_admitted {
            return Err(FamilyImageErrorV1::CertificateMismatch);
        }
        let expected = certificate_for(
            self.certificate.generator_release,
            generator_content_identity,
            &self.members,
            preimage_count,
            self.certificate.proof_release,
        )?;
        if expected != self.certificate {
            return Err(FamilyImageErrorV1::CertificateMismatch);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn corrupt_first_member_for_test(&mut self, replacement: ColorSignal) {
        if let Some(first) = self.members.first_mut() {
            *first = replacement;
        }
    }

    #[cfg(test)]
    pub(crate) fn recertify_proof_for_test(&mut self, proof: FamilyImageProofReleaseV1) {
        self.certificate = certificate_for(
            self.certificate.generator_release,
            self.certificate.generator_content_identity,
            &self.members,
            self.certificate.preimage_count,
            proof,
        )
        .expect("the admitted fixture already has one output profile");
    }

    #[cfg(test)]
    pub(crate) fn recertify_preimage_count_for_test(&mut self, preimage_count: u64) {
        self.certificate = certificate_for(
            self.certificate.generator_release,
            self.certificate.generator_content_identity,
            &self.members,
            preimage_count,
            self.certificate.proof_release,
        )
        .expect("the admitted fixture already has one output profile");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FamilyDeclarationV1 {
    id: FamilyId,
    set: AdmittedFamilySetV1,
}

impl FamilyDeclarationV1 {
    pub(crate) const fn new(id: FamilyId, set: AdmittedFamilySetV1) -> Self {
        Self { id, set }
    }

    pub(crate) const fn id(&self) -> FamilyId {
        self.id
    }

    pub(crate) const fn set(&self) -> &AdmittedFamilySetV1 {
        &self.set
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyMembershipMeasurementV1 {
    family: FamilyContentIdentityV1,
    signal: ColorSignal,
}

impl FamilyMembershipMeasurementV1 {
    pub(crate) const fn family(self) -> FamilyContentIdentityV1 {
        self.family
    }

    pub(crate) const fn signal(self) -> ColorSignal {
        self.signal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyMembershipPassV1 {
    rank: usize,
}

impl FamilyMembershipPassV1 {
    pub(crate) const fn rank(self) -> usize {
        self.rank
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FamilyMembershipViolationV1 {
    insertion_rank: usize,
    lower: Option<ColorSignal>,
    upper: Option<ColorSignal>,
}

impl FamilyMembershipViolationV1 {
    pub(crate) const fn insertion_rank(self) -> usize {
        self.insertion_rank
    }

    pub(crate) const fn lower(self) -> Option<ColorSignal> {
        self.lower
    }

    pub(crate) const fn upper(self) -> Option<ColorSignal> {
        self.upper
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FamilyImageErrorV1 {
    EmptyGeneratorDomain,
    #[cfg(test)]
    ImageMismatch {
        missing: Option<ColorSignal>,
        extraneous: Option<ColorSignal>,
    },
    NonCanonicalAdmittedImage,
    CertificateMismatch,
    ResourceExhausted,
}

#[cfg(test)]
pub(crate) fn verify_complete_family_image_v1(
    generator: CompleteFamilyGeneratorV1,
    mut proposed: UnverifiedFamilyImageV1,
) -> Result<AdmittedFamilySetV1, FamilyImageErrorV1> {
    let release = generator.release();
    let descriptor = generator.descriptor();
    let generated = generator.into_complete_output()?;
    if generated.members.is_empty() {
        return Err(FamilyImageErrorV1::EmptyGeneratorDomain);
    }
    let preimage_count = u64::try_from(generated.members.len())
        .map_err(|_| FamilyImageErrorV1::ResourceExhausted)?;
    let generator_content_identity =
        generator_content_identity_for(descriptor, &generated.members)?;
    let mut expected_members = generated.members;
    canonicalize(&mut expected_members);
    canonicalize(&mut proposed.members);
    let mismatch = first_set_mismatch(&expected_members, &proposed.members);
    if mismatch != (None, None) {
        return Err(FamilyImageErrorV1::ImageMismatch {
            missing: mismatch.0,
            extraneous: mismatch.1,
        });
    }
    let members = proposed.members;
    let certificate = certificate_for(
        release,
        generator_content_identity,
        &members,
        preimage_count,
        FamilyImageProofReleaseV1::ExhaustiveCanonicalImageComparisonV1,
    )?;
    let admitted = AdmittedFamilySetV1 {
        generator: descriptor,
        certificate,
        members,
    };
    admitted.verify()?;
    Ok(admitted)
}

pub(crate) fn admit_declared_family_image_v1(
    definition: Vec<ColorSignal>,
) -> Result<AdmittedFamilySetV1, FamilyImageErrorV1> {
    let generator = CompleteFamilyGeneratorV1::try_declared_finite_image_v1(definition)?;
    let release = generator.release();
    let descriptor = generator.descriptor();
    let generated = generator.into_complete_output()?.members;
    let preimage_count =
        u64::try_from(generated.len()).map_err(|_| FamilyImageErrorV1::ResourceExhausted)?;
    let generator_content_identity = generator_content_identity_for(descriptor, &generated)?;
    let certificate = certificate_for(
        release,
        generator_content_identity,
        &generated,
        preimage_count,
        FamilyImageProofReleaseV1::DeclaredImageIsDefinitionV1,
    )?;
    let admitted = AdmittedFamilySetV1 {
        generator: descriptor,
        certificate,
        members: generated,
    };
    admitted.verify()?;
    Ok(admitted)
}

fn canonicalize(members: &mut Vec<ColorSignal>) {
    members.sort_unstable_by_key(|member| canonical_signal_key(*member));
    members.dedup_by_key(|member| canonical_signal_key(*member));
}

/// Канонический codec V1 задан явно и не зависит от layout/derive порядка типа.
const fn canonical_signal_key(signal: ColorSignal) -> (u8, [u8; 3]) {
    (
        output_profile_tag(signal.output_profile()) as u8,
        signal.srgb8().bytes(),
    )
}

#[cfg(test)]
fn first_set_mismatch(
    expected: &[ColorSignal],
    actual: &[ColorSignal],
) -> (Option<ColorSignal>, Option<ColorSignal>) {
    let (mut expected_index, mut actual_index) = (0, 0);
    let (mut missing, mut extraneous) = (None, None);
    while expected_index < expected.len() || actual_index < actual.len() {
        match (expected.get(expected_index), actual.get(actual_index)) {
            (Some(expected), Some(actual))
                if canonical_signal_key(*expected) == canonical_signal_key(*actual) =>
            {
                expected_index += 1;
                actual_index += 1;
            }
            (Some(expected), Some(actual))
                if canonical_signal_key(*expected) < canonical_signal_key(*actual) =>
            {
                missing.get_or_insert(*expected);
                expected_index += 1;
            }
            (Some(_), Some(actual)) => {
                extraneous.get_or_insert(*actual);
                actual_index += 1;
            }
            (Some(expected), None) => {
                missing.get_or_insert(*expected);
                expected_index += 1;
            }
            (None, Some(actual)) => {
                extraneous.get_or_insert(*actual);
                actual_index += 1;
            }
            (None, None) => break,
        }
        if missing.is_some() && extraneous.is_some() {
            break;
        }
    }
    (missing, extraneous)
}

fn certificate_for(
    generator_release: FamilyGeneratorReleaseV1,
    generator_content_identity: FamilyGeneratorContentIdentityV1,
    members: &[ColorSignal],
    preimage_count: u64,
    proof_release: FamilyImageProofReleaseV1,
) -> Result<FamilyImageCertificateV1, FamilyImageErrorV1> {
    let first = members
        .first()
        .copied()
        .ok_or(FamilyImageErrorV1::EmptyGeneratorDomain)?;
    let output_profile = first.output_profile();
    let member_count =
        u64::try_from(members.len()).map_err(|_| FamilyImageErrorV1::ResourceExhausted)?;
    let image_content_identity = FamilyImageContentIdentityV1(image_digest(
        IMAGE_DOMAIN_V1,
        IMAGE_CODEC_V1,
        output_profile,
        member_count,
        members,
    ));
    let mut hasher = Hasher::new();
    hasher.update(FAMILY_DOMAIN_V1);
    hasher.update(&[
        FAMILY_CERTIFICATE_CODEC_V1,
        generator_release_tag(generator_release),
    ]);
    hasher.update(generator_content_identity.as_bytes());
    hasher.update(image_content_identity.as_bytes());
    hasher.update(&[
        output_profile_tag(output_profile) as u8,
        proof_release_tag(proof_release),
    ]);
    hasher.update(&preimage_count.to_be_bytes());
    hasher.update(&member_count.to_be_bytes());
    let family_content_identity = FamilyContentIdentityV1(*hasher.finalize().as_bytes());
    Ok(FamilyImageCertificateV1 {
        generator_release,
        generator_content_identity,
        image_content_identity,
        family_content_identity,
        output_profile,
        proof_release,
        preimage_count,
        member_count,
    })
}

fn image_digest(
    domain: &[u8],
    release: u8,
    profile: OutputProfileId,
    count: u64,
    members: &[ColorSignal],
) -> [u8; 32] {
    let mut hasher = Hasher::new();
    hasher.update(domain);
    hasher.update(&[release, output_profile_tag(profile) as u8]);
    hasher.update(&count.to_be_bytes());
    for member in members.iter().copied() {
        hasher.update(&[output_profile_tag(member.output_profile()) as u8]);
        hasher.update(&member.srgb8().bytes());
    }
    *hasher.finalize().as_bytes()
}

const fn output_profile_tag(profile: OutputProfileId) -> OutputProfileTagV1 {
    match profile {
        OutputProfileId::Iec61966Srgb8D65V1 => OutputProfileTagV1::Iec61966Srgb8D65V1,
    }
}

const fn generator_release_tag(release: FamilyGeneratorReleaseV1) -> u8 {
    match release {
        FamilyGeneratorReleaseV1::DeclaredFiniteImageV1 => 1,
        #[cfg(test)]
        FamilyGeneratorReleaseV1::EncodedSrgb8EqualChannelAxisV1 => 2,
        #[cfg(test)]
        FamilyGeneratorReleaseV1::EncodedSrgb8RedBlueDiagonalV1 => 3,
        #[cfg(test)]
        FamilyGeneratorReleaseV1::NonInjectiveUnorderedFixtureV1 => 4,
    }
}

const fn proof_release_tag(release: FamilyImageProofReleaseV1) -> u8 {
    match release {
        FamilyImageProofReleaseV1::DeclaredImageIsDefinitionV1 => 1,
        #[cfg(test)]
        FamilyImageProofReleaseV1::ExhaustiveCanonicalImageComparisonV1 => 2,
    }
}

fn generator_content_identity_for(
    descriptor: FamilyGeneratorDescriptorV1,
    output: &[ColorSignal],
) -> Result<FamilyGeneratorContentIdentityV1, FamilyImageErrorV1> {
    let first = output
        .first()
        .copied()
        .ok_or(FamilyImageErrorV1::EmptyGeneratorDomain)?;
    let count = u64::try_from(output.len()).map_err(|_| FamilyImageErrorV1::ResourceExhausted)?;
    let mut hasher = Hasher::new();
    hasher.update(GENERATOR_DOMAIN_V1);
    hasher.update(&[
        generator_release_tag(descriptor.release()),
        output_profile_tag(first.output_profile()) as u8,
    ]);
    descriptor.update_parameter_identity(&mut hasher);
    hasher.update(&count.to_be_bytes());
    for (ordinal, member) in (0_u64..).zip(output.iter().copied()) {
        hasher.update(&ordinal.to_be_bytes());
        hasher.update(&[output_profile_tag(member.output_profile()) as u8]);
        hasher.update(&member.srgb8().bytes());
    }
    Ok(FamilyGeneratorContentIdentityV1(
        *hasher.finalize().as_bytes(),
    ))
}

enum GeneratorReplayV1<'a> {
    Declared(&'a [ColorSignal]),
    #[cfg(test)]
    Generated(Vec<ColorSignal>),
}

impl FamilyGeneratorDescriptorV1 {
    const fn release(self) -> FamilyGeneratorReleaseV1 {
        match self {
            Self::DeclaredFiniteImageV1 => FamilyGeneratorReleaseV1::DeclaredFiniteImageV1,
            #[cfg(test)]
            Self::EncodedSrgb8EqualChannelAxisV1 => {
                FamilyGeneratorReleaseV1::EncodedSrgb8EqualChannelAxisV1
            }
            #[cfg(test)]
            Self::EncodedSrgb8RedBlueDiagonalV1 => {
                FamilyGeneratorReleaseV1::EncodedSrgb8RedBlueDiagonalV1
            }
            #[cfg(test)]
            Self::NonInjectiveUnorderedFixtureV1 { .. } => {
                FamilyGeneratorReleaseV1::NonInjectiveUnorderedFixtureV1
            }
        }
    }

    fn update_parameter_identity(self, hasher: &mut Hasher) {
        hasher.update(GENERATOR_PARAMETERS_DOMAIN_V1);
        match self {
            Self::DeclaredFiniteImageV1 => {
                hasher.update(&[GENERATOR_PARAMETERS_CODEC_V1, 0]);
            }
            #[cfg(test)]
            Self::EncodedSrgb8EqualChannelAxisV1 => {
                hasher.update(&[GENERATOR_PARAMETERS_CODEC_V1, 0]);
            }
            #[cfg(test)]
            Self::EncodedSrgb8RedBlueDiagonalV1 => {
                hasher.update(&[GENERATOR_PARAMETERS_CODEC_V1, 0]);
            }
            #[cfg(test)]
            Self::NonInjectiveUnorderedFixtureV1 { provenance, .. } => {
                // Перестановку связывает сам ordinal-output ниже. Здесь остаётся
                // независимый parameter provenance, чтобы тесты не могли
                // взаимно маскировать две части generator identity.
                hasher.update(&[GENERATOR_PARAMETERS_CODEC_V1, 1, provenance]);
            }
        }
    }

    fn replay(
        self,
        declared_members: &[ColorSignal],
    ) -> Result<GeneratorReplayV1<'_>, FamilyImageErrorV1> {
        match self {
            Self::DeclaredFiniteImageV1 => Ok(GeneratorReplayV1::Declared(declared_members)),
            #[cfg(test)]
            Self::EncodedSrgb8EqualChannelAxisV1 => Ok(GeneratorReplayV1::Generated(
                CompleteFamilyGeneratorV1::EncodedSrgb8EqualChannelAxisV1
                    .into_complete_output()?
                    .members,
            )),
            #[cfg(test)]
            Self::EncodedSrgb8RedBlueDiagonalV1 => Ok(GeneratorReplayV1::Generated(
                CompleteFamilyGeneratorV1::EncodedSrgb8RedBlueDiagonalV1
                    .into_complete_output()?
                    .members,
            )),
            #[cfg(test)]
            Self::NonInjectiveUnorderedFixtureV1 {
                permuted,
                provenance,
            } => Ok(GeneratorReplayV1::Generated(
                CompleteFamilyGeneratorV1::NonInjectiveUnorderedFixtureV1 {
                    permuted,
                    provenance,
                }
                .into_complete_output()?
                .members,
            )),
        }
    }
}
