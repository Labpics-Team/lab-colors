//! Реестр действующих численных доказательств и его единственная проекция V2.
//!
//! Строка определяет область, закон, артефакт и допустимый класс свидетельства.
//! Манифест выводится из этих строк; отсутствующая возможность не появляется
//! из режима совместимости или названия прежнего алгоритма.

/// Whether a stable profile may silently choose another decision path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalFallbackStatusV1 {
    /// No fallback; compatibility requires an explicit mode.
    None,
}

impl NumericalFallbackStatusV1 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::None => "none",
        }
    }
}

/// Зарегистрированные выпуски совместимости. Сейчас множество пусто.
/// Поле манифеста остаётся явным пустым списком без скрытого запасного алгоритма.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalCompatibilityReleaseIdV1 {}

impl NumericalCompatibilityReleaseIdV1 {
    /// Ключ недостижим, пока ни один выпуск не зарегистрирован.
    pub fn key(self) -> &'static str {
        match self {}
    }
}

// Одна декларация порождает идентификаторы и записи, без второго реестра.
macro_rules! define_numerical_registry {
    ($(
        $(#[$meta:meta])*
        $variant:ident => {
            key: $key:literal,
            operations: $operations:literal,
            domain: $domain:literal,
            branch_effect: $branch:literal,
            stable_outcomes: [$($outcome:ident),+ $(,)?],
            compatibility_releases: [$($compatibility:path),* $(,)?],
            evidence_classes: [$($evidence:ident),+ $(,)?],
            artifact_ids: [$($artifact:path),+ $(,)?],
            bound_ids: [$($bound_id:path),+ $(,)?],
            proof_ids: [$($proof:path),+ $(,)?],
            bound_status: $bound:ident,
            boundary_corpus: $corpus:literal,
            runtime_matrix: $matrix:literal,
            fallback_status: $fallback:path $(,)?
        }
    ),+ $(,)?) => {
        /// Действующий владелец численного доказательства.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[non_exhaustive]
        pub enum NumericalSiteIdV2 { $($(#[$meta])* $variant),+ }
        impl NumericalSiteIdV2 {
            /// Стабильный ключ внешнего манифеста.
            pub fn key(self) -> &'static str { match self { $(Self::$variant => $key),+ } }
        }
        const NUMERICAL_REGISTRY_V2: &[NumericalSiteRecordV2] = &[
            $(NumericalSiteRecordV2 {
                site_id: NumericalSiteIdV2::$variant,
                operations: $operations,
                domain: $domain,
                branch_effect: $branch,
                stable_outcomes: &[$(StableNumericalOutcomeV2::$outcome),+],
                compatibility_releases: &[$($compatibility),*],
                evidence_classes: &[$(NumericalEvidenceClassV2::$evidence),+],
                artifact_ids: &[$($artifact),+],
                bound_ids: &[$($bound_id),+],
                proof_ids: &[$($proof),+],
                runtime_attestations: &[],
                bound_status: NumericalBoundStatusV2::$bound,
                boundary_corpus: $corpus,
                runtime_matrix: $matrix,
                fallback_status: $fallback,
            }),+
        ];
    };
}

define_numerical_registry! {
    /// WCAG 2.2 assessment of one final sRGB8 pair.
    Wcag22Srgb8ContrastV1 => {
        key: "wcag22-srgb8-contrast-v1",
        operations: "integer threshold laws over Q55 outward luminance bounds; both orientations",
        domain: "final foreground/background sRGB8 pair + explicit criterion -> atomic assessment",
        branch_effect: "proved Pass versus proved Fail; full-domain proof rejects unresolved artifact",
        stable_outcomes: [CanonicalFiniteBounded],
        compatibility_releases: [],
        evidence_classes: [CanonicalFiniteBounded],
        artifact_ids: [NumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55V1],
        bound_ids: [NumericalErrorBoundIdV2::Wcag22Srgb8OutwardQ55V1],
        proof_ids: [NumericalProofIdV2::Wcag22Srgb8FullDomainQ55V1],
        bound_status: Available,
        boundary_corpus: "anti-epsilon witnesses; exact 21:1; threshold-equality; full 16.7M domain scan",
        runtime_matrix: "native + wasm32 integer-only comparisons; adapters transport terminal results",
        fallback_status: NumericalFallbackStatusV1::None,
    },
    /// Retained point-support surplus over a separately declared anchor.
    PointSupportRetainedReferenceSurplusV1 => {
        key: "point-support-retained-reference-surplus-v1",
        operations: "Q55 conservative reference-distance witness; exact rational anchor surplus; basis-point retention; continued-fraction ordering",
        domain: "baseline/current final sRGB8 pairs + explicit 1/3/4.5 anchor + drop basis points -> retained/not-retained",
        branch_effect: "runtime reconciliation trigger, independent of WCAG criterion assessment",
        stable_outcomes: [CanonicalFiniteBounded],
        compatibility_releases: [],
        evidence_classes: [CanonicalFiniteBounded],
        artifact_ids: [NumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55V1],
        bound_ids: [NumericalErrorBoundIdV2::PointSupportReferenceSurplusQ55BpsV1],
        proof_ids: [NumericalProofIdV2::PointSupportReferenceSurplusIntegerV1],
        bound_status: Available,
        boundary_corpus: "identity/3/4.5 anchors; drop 0/10000; equality; polarity reversal; overlapping Q55 intervals; u128 extrema",
        runtime_matrix: "native Core evaluation; wasm32 Core compile; package/FFI transport not yet exposed",
        fallback_status: NumericalFallbackStatusV1::None,
    },
}

// ── Package capability manifest encoding (#289) ─────────────────────────────

/// Length-prefixed запись: u32 LE длина + байты. Единый примитив canonical
/// encoding manifest/plan (versioned контракт, не JSON).
pub(crate) fn push_len_prefixed(buffer: &mut Vec<u8>, bytes: &[u8]) {
    buffer.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    buffer.extend_from_slice(bytes);
}

/// Отсортированный по UTF-8 bytes список ключей: u32 LE count (явный и для
/// пустого списка) + length-prefixed элементы. Дубликаты запрещены by
/// construction registry (закреплено тестом уникальности).
fn push_sorted_key_list(buffer: &mut Vec<u8>, keys: &mut Vec<&'static str>) {
    keys.sort_unstable();
    buffer.extend_from_slice(&(keys.len() as u32).to_le_bytes());
    for key in keys.iter() {
        push_len_prefixed(buffer, key.as_bytes());
    }
}

// ── Proof-capable package capability manifest V2 (#284) ────────────────────

/// Версия единственной публичной proof-capable capability-схемы.
pub const NUMERICAL_CAPABILITY_SCHEMA_VERSION_V2: u32 = 2;

/// Домен-сепаратор canonical checksum preimage V2.
const CAPABILITY_CHECKSUM_DOMAIN_V2: &[u8] = b"labcolors.numerical-capability.v2";

/// Stable outcomes admitted by the proof-capable V2 registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StableNumericalOutcomeV2 {
    /// Determinate branch follows from exact finite/integer/rational evidence.
    BitExact,
    /// Determinate branch follows from a canonical finite outward-bound artifact.
    CanonicalFiniteBounded,
    /// No semantic branch is selected.
    Indeterminate,
}

impl StableNumericalOutcomeV2 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::BitExact => "bit-exact",
            Self::CanonicalFiniteBounded => "canonical-finite-bounded",
            Self::Indeterminate => "indeterminate",
        }
    }
}

/// Sound-bound availability in the V2 registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalBoundStatusV2 {
    /// Registered sound bound is shipped and independently verified.
    Available,
    /// No sound bound has been admitted.
    Unavailable,
}

impl NumericalBoundStatusV2 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Evidence classes the V2 package can mint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalEvidenceClassV2 {
    /// Exact decision over finite/integer state.
    BitExact,
    /// Decision from a registered canonical finite artifact and outward law.
    CanonicalFiniteBounded,
}

impl NumericalEvidenceClassV2 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::BitExact => "bit-exact",
            Self::CanonicalFiniteBounded => "canonical-finite-bounded",
        }
    }
}

/// Canonical finite artifact identities admitted by V2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalArtifactIdV2 {
    /// Q55 outward tables for WCAG 2.2 sRGB8 relative luminance.
    Wcag22Srgb8LuminanceQ55V1,
}

impl NumericalArtifactIdV2 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Wcag22Srgb8LuminanceQ55V1 => "wcag22-srgb8-luminance-q55-v1",
        }
    }
}

/// Registered error-bound identities admitted by V2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalErrorBoundIdV2 {
    /// Integer Q55 outward-bound and threshold laws for WCAG 3.0/4.5.
    Wcag22Srgb8OutwardQ55V1,
    /// Conservative Q55 reference-surplus witness and exact basis-point
    /// rational retention law for point support.
    PointSupportReferenceSurplusQ55BpsV1,
}

impl NumericalErrorBoundIdV2 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Wcag22Srgb8OutwardQ55V1 => "wcag22-srgb8-outward-q55-v1",
            Self::PointSupportReferenceSurplusQ55BpsV1 => {
                "point-support-reference-surplus-q55-bps-v1"
            }
        }
    }
}

/// Replayable proof identities admitted by V2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalProofIdV2 {
    /// Full sRGB8-domain proof with zero unresolved WCAG 3.0/4.5 decisions.
    Wcag22Srgb8FullDomainQ55V1,
    /// Replayable integer/rational proof for point-support retained surplus.
    PointSupportReferenceSurplusIntegerV1,
}

impl NumericalProofIdV2 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Wcag22Srgb8FullDomainQ55V1 => "wcag22-srgb8-full-domain-q55-v1",
            Self::PointSupportReferenceSurplusIntegerV1 => {
                "point-support-reference-surplus-integer-v1"
            }
        }
    }
}

/// Runtime attestation identities admitted by V2. Empty until #258.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalRuntimeAttestationIdV2 {}

impl NumericalRuntimeAttestationIdV2 {
    /// Stable manifest key (unreachable while the type is uninhabited).
    pub fn key(self) -> &'static str {
        match self {}
    }
}

/// Machine-readable proof-capable registry row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct NumericalSiteRecordV2 {
    /// Stable site identity.
    pub site_id: NumericalSiteIdV2,
    /// Branch-sensitive operations (research metadata).
    pub operations: &'static str,
    /// Input/output domain (research metadata).
    pub domain: &'static str,
    /// Semantic branch affected by the value (research metadata).
    pub branch_effect: &'static str,
    /// Lawful stable outcomes.
    pub stable_outcomes: &'static [StableNumericalOutcomeV2],
    /// Registered compatibility releases.
    pub compatibility_releases: &'static [NumericalCompatibilityReleaseIdV1],
    /// Evidence classes mintable for the site.
    pub evidence_classes: &'static [NumericalEvidenceClassV2],
    /// Canonical finite artifacts.
    pub artifact_ids: &'static [NumericalArtifactIdV2],
    /// Registered error bounds.
    pub bound_ids: &'static [NumericalErrorBoundIdV2],
    /// Replayable proof artifacts.
    pub proof_ids: &'static [NumericalProofIdV2],
    /// Runtime attestations (empty until #258).
    pub runtime_attestations: &'static [NumericalRuntimeAttestationIdV2],
    /// Sound-bound availability (research metadata).
    pub bound_status: NumericalBoundStatusV2,
    /// Executable boundary corpus identifiers (research metadata).
    pub boundary_corpus: &'static str,
    /// Required cross-runtime comparison scope (research metadata).
    pub runtime_matrix: &'static str,
    /// Fallback status.
    pub fallback_status: NumericalFallbackStatusV1,
}

/// Registry of proof-capable typed-decision sites.
pub fn numerical_registry_v2() -> &'static [NumericalSiteRecordV2] {
    NUMERICAL_REGISTRY_V2
}

/// V2 registry coverage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalRegistryCoverageV2 {
    /// Only migrated sites are listed; this is not a whole-core audit claim.
    MigratedSitesOnlyV1,
}

impl NumericalRegistryCoverageV2 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::MigratedSitesOnlyV1 => "migrated-sites-only-v1",
        }
    }
}

/// Proof-capable capability projection for one V2 registry site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericalSiteCapabilityV2 {
    /// Site identity.
    pub site_id: NumericalSiteIdV2,
    /// Lawful stable outcomes.
    pub stable_outcomes: &'static [StableNumericalOutcomeV2],
    /// Registered compatibility releases.
    pub compatibility_releases: &'static [NumericalCompatibilityReleaseIdV1],
    /// Mintable evidence classes.
    pub evidence_classes: &'static [NumericalEvidenceClassV2],
    /// Canonical finite artifacts.
    pub artifact_ids: &'static [NumericalArtifactIdV2],
    /// Registered error bounds.
    pub bound_ids: &'static [NumericalErrorBoundIdV2],
    /// Replayable proof artifacts.
    pub proof_ids: &'static [NumericalProofIdV2],
    /// Runtime attestations.
    pub runtime_attestations: &'static [NumericalRuntimeAttestationIdV2],
}

/// Drift checksum for the V2 canonical capability projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericalCapabilityChecksumV2(u32);

impl NumericalCapabilityChecksumV2 {
    /// FNV-1a-32 over the V2 canonical preimage.
    pub fn from_preimage(preimage: &[u8]) -> Self {
        Self(crate::hash::fnv1a_32(preimage))
    }

    /// Canonical lowercase eight-hex representation.
    pub fn hex(self) -> String {
        format!("{:08x}", self.0)
    }
}

/// Proof-capable package manifest generated only from the V2 registry SSOT.
#[derive(Debug, Clone, PartialEq)]
pub struct NumericalCapabilityManifestV2 {
    /// Capability schema version.
    pub schema_version: u32,
    /// Registry coverage.
    pub coverage: NumericalRegistryCoverageV2,
    /// Canonically sorted site capabilities.
    pub sites: Vec<NumericalSiteCapabilityV2>,
    /// Drift checksum of the canonical projection.
    pub checksum: NumericalCapabilityChecksumV2,
}

impl NumericalCapabilityManifestV2 {
    /// Versioned length-prefixed canonical checksum preimage. `proof_ids`
    /// кодируются после `bound_ids` и до runtime attestations.
    pub fn canonical_checksum_preimage(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        push_len_prefixed(&mut buffer, CAPABILITY_CHECKSUM_DOMAIN_V2);
        buffer.extend_from_slice(&self.schema_version.to_le_bytes());
        push_len_prefixed(&mut buffer, self.coverage.key().as_bytes());
        let mut sites: Vec<&NumericalSiteCapabilityV2> = self.sites.iter().collect();
        sites.sort_unstable_by_key(|site| site.site_id.key().as_bytes());
        buffer.extend_from_slice(&(sites.len() as u32).to_le_bytes());
        for site in sites {
            push_len_prefixed(&mut buffer, site.site_id.key().as_bytes());
            push_sorted_key_list(
                &mut buffer,
                &mut site.stable_outcomes.iter().map(|v| v.key()).collect(),
            );
            push_sorted_key_list(
                &mut buffer,
                &mut site
                    .compatibility_releases
                    .iter()
                    .map(|v| v.key())
                    .collect(),
            );
            push_sorted_key_list(
                &mut buffer,
                &mut site.evidence_classes.iter().map(|v| v.key()).collect(),
            );
            push_sorted_key_list(
                &mut buffer,
                &mut site.artifact_ids.iter().map(|v| v.key()).collect(),
            );
            push_sorted_key_list(
                &mut buffer,
                &mut site.bound_ids.iter().map(|v| v.key()).collect(),
            );
            push_sorted_key_list(
                &mut buffer,
                &mut site.proof_ids.iter().map(|v| v.key()).collect(),
            );
            push_sorted_key_list(
                &mut buffer,
                &mut site.runtime_attestations.iter().map(|v| v.key()).collect(),
            );
        }
        buffer
    }
}

/// Capability manifest for the proof-capable V2 registry.
pub fn numerical_capability_manifest_v2() -> NumericalCapabilityManifestV2 {
    let mut sites: Vec<NumericalSiteCapabilityV2> = NUMERICAL_REGISTRY_V2
        .iter()
        .map(|row| NumericalSiteCapabilityV2 {
            site_id: row.site_id,
            stable_outcomes: row.stable_outcomes,
            compatibility_releases: row.compatibility_releases,
            evidence_classes: row.evidence_classes,
            artifact_ids: row.artifact_ids,
            bound_ids: row.bound_ids,
            proof_ids: row.proof_ids,
            runtime_attestations: row.runtime_attestations,
        })
        .collect();
    sites.sort_unstable_by_key(|site| site.site_id.key().as_bytes());
    let mut manifest = NumericalCapabilityManifestV2 {
        schema_version: NUMERICAL_CAPABILITY_SCHEMA_VERSION_V2,
        coverage: NumericalRegistryCoverageV2::MigratedSitesOnlyV1,
        sites,
        checksum: NumericalCapabilityChecksumV2(0),
    };
    manifest.checksum =
        NumericalCapabilityChecksumV2::from_preimage(&manifest.canonical_checksum_preimage());
    manifest
}

/// Свидетельство проверенного конечного результата. Полезная нагрузка
/// создаётся только владельцем зарегистрированного доказательства.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NumericalDecisionEvidenceV1 {
    /// Конечный артефакт, внешние численные границы и целочисленный закон.
    CanonicalFiniteBounded(crate::wcag22_evidence::CanonicalFiniteBoundedEvidenceV1),
}

impl NumericalDecisionEvidenceV1 {
    /// Стабильный ключ класса свидетельства.
    pub fn class_key(&self) -> &'static str {
        match self {
            Self::CanonicalFiniteBounded(_) => "canonical-finite-bounded",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_contains_only_current_proof_owners() {
        let rows = numerical_registry_v2();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows.iter().map(|row| row.site_id.key()).collect::<Vec<_>>(),
            [
                "wcag22-srgb8-contrast-v1",
                "point-support-retained-reference-surplus-v1"
            ]
        );
        for row in rows {
            assert!(row.compatibility_releases.is_empty());
            assert!(!row.artifact_ids.is_empty() && !row.proof_ids.is_empty());
        }
        let mut site_keys = std::collections::HashSet::new();
        for row in rows {
            assert!(
                site_keys.insert(row.site_id.key()),
                "duplicate V2 numerical site wire key: {}",
                row.site_id.key()
            );
        }
        let wcag = rows
            .iter()
            .find(|row| row.site_id == NumericalSiteIdV2::Wcag22Srgb8ContrastV1)
            .expect("WCAG22 site обязан быть зарегистрирован в V2");
        assert_eq!(
            wcag.evidence_classes,
            [NumericalEvidenceClassV2::CanonicalFiniteBounded]
        );
        assert_eq!(
            wcag.artifact_ids,
            [NumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55V1]
        );
        assert_eq!(
            wcag.bound_ids,
            [NumericalErrorBoundIdV2::Wcag22Srgb8OutwardQ55V1]
        );
        assert_eq!(
            wcag.proof_ids,
            [NumericalProofIdV2::Wcag22Srgb8FullDomainQ55V1]
        );
        assert_eq!(wcag.bound_status, NumericalBoundStatusV2::Available);
        let stability = rows
            .iter()
            .find(|row| row.site_id == NumericalSiteIdV2::PointSupportRetainedReferenceSurplusV1)
            .expect("point-support stability site must be registered");
        assert_eq!(
            stability.artifact_ids,
            [NumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55V1]
        );
        assert_eq!(
            stability.bound_ids,
            [NumericalErrorBoundIdV2::PointSupportReferenceSurplusQ55BpsV1]
        );
        assert_eq!(
            stability.proof_ids,
            [NumericalProofIdV2::PointSupportReferenceSurplusIntegerV1]
        );
    }

    /// Checksum: детерминирован, чувствителен к содержимому canonical-полей и
    /// нечувствителен к порядку rows (сортировка внутри preimage).
    #[test]
    fn capability_checksum_is_canonical_and_tamper_sensitive() {
        let manifest = numerical_capability_manifest_v2();
        let recomputed =
            NumericalCapabilityChecksumV2::from_preimage(&manifest.canonical_checksum_preimage());
        assert_eq!(manifest.checksum, recomputed);
        assert_eq!(manifest.checksum.hex().len(), 8);

        // Tamper: смена schema version меняет preimage/checksum.
        let mut tampered = manifest.clone();
        tampered.schema_version += 1;
        assert_ne!(
            NumericalCapabilityChecksumV2::from_preimage(&tampered.canonical_checksum_preimage()),
            manifest.checksum
        );

        // Tamper: удаление row меняет checksum.
        let mut emptied = manifest.clone();
        emptied.sites.clear();
        assert_ne!(
            NumericalCapabilityChecksumV2::from_preimage(&emptied.canonical_checksum_preimage()),
            manifest.checksum
        );

        // Tamper: удаление proof identity меняет checksum независимо от
        // остальных capability-полей строки.
        let mut proof_tampered = manifest.clone();
        let wcag = proof_tampered
            .sites
            .iter_mut()
            .find(|site| site.site_id == NumericalSiteIdV2::Wcag22Srgb8ContrastV1)
            .expect("WCAG22 capability row");
        wcag.proof_ids = &[];
        assert_ne!(
            NumericalCapabilityChecksumV2::from_preimage(
                &proof_tampered.canonical_checksum_preimage()
            ),
            manifest.checksum
        );
    }

    /// Exact independent oracle for the `proof_ids` list position and bytes.
    /// This intentionally does not call either production encoding helper.
    #[test]
    fn proof_ids_have_independent_canonical_encoding_guard() {
        fn push_expected_len_prefixed(buffer: &mut Vec<u8>, value: &[u8]) {
            buffer.extend_from_slice(&(value.len() as u32).to_le_bytes());
            buffer.extend_from_slice(value);
        }

        let manifest = NumericalCapabilityManifestV2 {
            schema_version: 2,
            coverage: NumericalRegistryCoverageV2::MigratedSitesOnlyV1,
            sites: vec![NumericalSiteCapabilityV2 {
                site_id: NumericalSiteIdV2::Wcag22Srgb8ContrastV1,
                stable_outcomes: &[],
                compatibility_releases: &[],
                evidence_classes: &[],
                artifact_ids: &[],
                bound_ids: &[],
                proof_ids: &[NumericalProofIdV2::Wcag22Srgb8FullDomainQ55V1],
                runtime_attestations: &[],
            }],
            checksum: NumericalCapabilityChecksumV2(0),
        };

        let mut expected = Vec::new();
        push_expected_len_prefixed(&mut expected, b"labcolors.numerical-capability.v2");
        expected.extend_from_slice(&2_u32.to_le_bytes());
        push_expected_len_prefixed(&mut expected, b"migrated-sites-only-v1");
        expected.extend_from_slice(&1_u32.to_le_bytes());
        push_expected_len_prefixed(&mut expected, b"wcag22-srgb8-contrast-v1");
        // stable outcomes, releases, evidence, artifacts, then bounds.
        for _ in 0..5 {
            expected.extend_from_slice(&0_u32.to_le_bytes());
        }
        expected.extend_from_slice(&1_u32.to_le_bytes());
        push_expected_len_prefixed(&mut expected, b"wcag22-srgb8-full-domain-q55-v1");
        // runtime attestations follow proof IDs.
        expected.extend_from_slice(&0_u32.to_le_bytes());

        assert_eq!(manifest.canonical_checksum_preimage(), expected);
    }
}
