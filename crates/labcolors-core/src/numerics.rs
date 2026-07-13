//! Типизированные решения для branch-sensitive численных sites (#281, #289, #292).
//!
//! Diagnostic `f64` не становится semantic verdict сам по себе. Три уровня
//! контракта разделены типами и не подменяют друг друга:
//!
//! ```text
//! package capability       — registry-строка/manifest: что данная сборка
//!                            ВООБЩЕ умеет для site (outcomes, releases,
//!                            evidence classes; без выбранного mode)
//! compiled invocation plan — typed execution mode каждой compiled invocation
//!                            и его derived-проекция ([`crate::numerical_plan`])
//! result evidence          — атомарный терминальный результат: запечатанное
//!                            [`NumericalDecisionEvidenceV1`] у Determinate,
//!                            registered release у Compatibility, типизированная
//!                            причина у Indeterminate
//! ```
//!
//! Законы versioned capability-срезов (#292/#284):
//!
//! * `Determinate` несёт только реально минтимое core-ом evidence: опубликованный
//!   V1 registry допускает `BitExact`, proof-capable V2 также допускает
//!   `CanonicalFiniteBounded`; оба конструктора запечатаны и registry-owned;
//! * текущий нехарактеризованный legacy-результат — отдельный атомарный вариант
//!   `Compatibility` с зарегистрированным release ID и provenance-классом
//!   `LegacyPlatformDependentV1`; он НЕ является determinate evidence и не
//!   конвертируется в него;
//! * caller-created интервал не изготовляет никакого evidence: интервал живёт
//!   только как диагностический payload `Indeterminate::IntervalOverlap`;
//! * незаконная комбинация (stable + legacy provenance и т. п.) непредставима
//!   типами, а не запрещена соглашением.
//!
//! # Гарантии, закреплённые компилятором (#292)
//!
//! Прежний числовой классификатор `classify_at_least_v1` (вместе с
//! `AtLeastDecisionV1`/`DecisionGuaranteeV1`) УДАЛЁН, а не deprecated: «сырое
//! сравнение с порогом» больше не публичный закон, и его невозможно
//! импортировать — регрессия ловится компилятором, не code review:
//!
//! ```compile_fail
//! use labcolors_core::classify_at_least_v1;
//! ```
//!
//! BitExact-evidence запечатан registry-owned минтером: печать
//! [`EvidenceSeal`] несёт приватное поле, поэтому внешний struct-литерал
//! варианта [`NumericalDecisionEvidenceV1::BitExact`] не компилируется —
//! внешний код может лишь матчить вариант через `..`:
//!
//! ```compile_fail
//! let forged = labcolors_core::NumericalDecisionEvidenceV1::BitExact {
//!     reference_profile_id: labcolors_core::ReferenceProfileIdV1::EncodedSrgb8ScreenV1,
//!     // Печать не изготовить снаружи: поле `_private` приватно (E0451).
//!     _seal: labcolors_core::numerics::EvidenceSeal { _private: () },
//! };
//! ```
//!
//! Подлинное evidence также нельзя переиспользовать для другого site/result:
//! каждый terminal-вариант запечатан целиком, а не только его evidence payload.
//!
//! ```compile_fail,E0639
//! use labcolors_core::{NumericalDecisionV1, NumericalSiteIdV1};
//! use labcolors_core::wcag22::{
//!     Wcag22AssessmentV1, Wcag22CriterionV1, evaluate_wcag22_srgb8,
//! };
//!
//! let genuine = evaluate_wcag22_srgb8(
//!     [0, 0, 0],
//!     [255, 255, 255],
//!     Wcag22CriterionV1::Sc143TextDefault,
//! ).unwrap();
//! let Wcag22AssessmentV1::Evaluated { evidence, .. } = genuine else {
//!     unreachable!()
//! };
//! let _forged: NumericalDecisionV1<&str> = NumericalDecisionV1::Determinate {
//!     site_id: NumericalSiteIdV1::GlowTargetOrMaximumV1,
//!     value: "forged cross-site result",
//!     evidence,
//! };
//! ```

/// Stable outcomes admitted for a migrated branch-sensitive site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum StableNumericalOutcomeV1 {
    /// Determinate branch follows from exact finite/integer/rational evidence.
    BitExact,
    /// No semantic branch is selected.
    Indeterminate,
}

impl StableNumericalOutcomeV1 {
    /// Stable manifest key.
    #[cfg(test)]
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::BitExact => "bit-exact",
            Self::Indeterminate => "indeterminate",
        }
    }
}

/// Sound-bound availability for a migrated numerical site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum NumericalBoundStatusV1 {
    /// No sound bound has been admitted.
    Unavailable,
}

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

/// Зарегистрированный generic compatibility release: конкретный прежний
/// алгоритм, сохранённый явно. Release идентифицирует АЛГОРИТМ; provenance-класс
/// результата ([`LegacyPlatformDependentV1`]) его не заменяет.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalCompatibilityReleaseIdV1 {
    /// Текущий CAM16-UCS J′ target/max селектор точечного Glow.
    GlowCam16UcsJPrimeTargetOrMaxV1,
}

impl NumericalCompatibilityReleaseIdV1 {
    /// Стабильный registry/wire key.
    pub fn key(self) -> &'static str {
        match self {
            Self::GlowCam16UcsJPrimeTargetOrMaxV1 => "glow-cam16-ucs-jprime-target-or-max-v1",
        }
    }
}

/// Класс evidence, который package способен минтить для site (manifest-уровень).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum NumericalEvidenceClassV1 {
    /// Точное решение из конечного integer/байтового состояния.
    BitExact,
}

impl NumericalEvidenceClassV1 {
    /// Стабильный manifest key.
    #[cfg(test)]
    pub(crate) fn key(self) -> &'static str {
        match self {
            Self::BitExact => "bit-exact",
        }
    }
}

/// Идентификатор reference-профиля, в чьём точном конечном домене доказан
/// BitExact-результат.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReferenceProfileIdV1 {
    /// Точечный screen-композит в encoded sRGB8 (профиль Glow-композитора).
    EncodedSrgb8ScreenV1,
}

impl ReferenceProfileIdV1 {
    /// Стабильный wire key (совпадает с ключом профиля композитора).
    pub fn key(self) -> &'static str {
        match self {
            Self::EncodedSrgb8ScreenV1 => "encoded-srgb8-screen-v1",
        }
    }
}

/// Internal V1 artifact identity. Тип намеренно ненаселён: adaptive Glow
/// registry не может приписать себе canonical finite artifact.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericalArtifactIdV1 {}

impl NumericalArtifactIdV1 {
    /// Стабильный manifest key (недостижимо: тип ненаселён).
    #[cfg(test)]
    pub(crate) fn key(self) -> &'static str {
        match self {}
    }
}

/// Идентификатор зарегистрированного error bound. Не допущен в V1 (ненаселён).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericalErrorBoundIdV1 {}

impl NumericalErrorBoundIdV1 {
    /// Стабильный manifest key (недостижимо: тип ненаселён).
    #[cfg(test)]
    pub(crate) fn key(self) -> &'static str {
        match self {}
    }
}

/// Идентификатор runtime attestation. Не допущен до immutable attestation
/// registry (#258): тип ненаселён, `PlatformCharacterized` непредставим.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericalRuntimeAttestationIdV1 {}

impl NumericalRuntimeAttestationIdV1 {
    /// Стабильный manifest key (недостижимо: тип ненаселён).
    #[cfg(test)]
    pub(crate) fn key(self) -> &'static str {
        match self {}
    }
}

/// Internal adaptive-runtime registry row required by research lock #281.
///
/// Текстовые поля (`operations`/`domain`/`branch_effect`/`boundary_corpus`/
/// `runtime_matrix`) — human-readable research metadata, не public capability
/// projection и не input runtime-решения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct NumericalSiteRecordV1 {
    /// Stable site identity.
    pub site_id: NumericalSiteIdV1,
    /// Branch-sensitive operations (research metadata).
    pub operations: &'static str,
    /// Input/output domain (research metadata).
    pub domain: &'static str,
    /// Semantic branch affected by the value (research metadata).
    pub branch_effect: &'static str,
    /// Lawful stable outcomes.
    pub stable_outcomes: &'static [StableNumericalOutcomeV1],
    /// Registered generic compatibility releases данного site.
    pub compatibility_releases: &'static [NumericalCompatibilityReleaseIdV1],
    /// Классы evidence, минтимые package-ом для site.
    pub evidence_classes: &'static [NumericalEvidenceClassV1],
    /// Canonical finite artifacts (пусто = evidence отсутствует, не implicit support).
    pub artifact_ids: &'static [NumericalArtifactIdV1],
    /// Registered error bounds (пусто = отсутствуют).
    pub bound_ids: &'static [NumericalErrorBoundIdV1],
    /// Runtime attestations (пусто до #258).
    pub runtime_attestations: &'static [NumericalRuntimeAttestationIdV1],
    /// Sound-bound availability (research metadata).
    pub bound_status: NumericalBoundStatusV1,
    /// Executable boundary corpus identifiers (research metadata).
    pub boundary_corpus: &'static str,
    /// Required cross-runtime comparison scope (research metadata).
    pub runtime_matrix: &'static str,
    /// Fallback status.
    pub fallback_status: NumericalFallbackStatusV1,
}

// One declaration emits both the internal adaptive-runtime V1 projection and
// the only public proof-capable V2 registry. Shared sites therefore cannot
// drift between runtime/plan validation and the package capability manifest.
macro_rules! define_numerical_registries {
    (
        legacy {
            $(
                $(#[$legacy_meta:meta])*
                $legacy_variant:ident => {
                    key: $legacy_key:literal,
                    operations: $legacy_operations:literal,
                    domain: $legacy_domain:literal,
                    branch_effect: $legacy_branch_effect:literal,
                    stable_outcomes: [$($legacy_stable:ident),+ $(,)?],
                    compatibility_releases: [$($legacy_release:path),* $(,)?],
                    evidence_classes: [$($legacy_evidence:ident),* $(,)?],
                    bound_status: $legacy_bound:ident,
                    boundary_corpus: $legacy_corpus:literal,
                    runtime_matrix: $legacy_matrix:literal,
                    fallback_status: $legacy_fallback:path $(,)?
                }
            ),+ $(,)?
        }
        proof {
            $(
                $(#[$proof_meta:meta])*
                $proof_variant:ident => {
                    key: $proof_key:literal,
                    operations: $proof_operations:literal,
                    domain: $proof_domain:literal,
                    branch_effect: $proof_branch_effect:literal,
                    stable_outcomes: [$($proof_stable:ident),+ $(,)?],
                    compatibility_releases: [$($proof_release:path),* $(,)?],
                    evidence_classes: [$($proof_evidence:ident),+ $(,)?],
                    artifact_ids: [$($proof_artifact:path),+ $(,)?],
                    bound_ids: [$($proof_bound_id:path),+ $(,)?],
                    proof_ids: [$($proof_id:path),+ $(,)?],
                    bound_status: $proof_bound:ident,
                    boundary_corpus: $proof_corpus:literal,
                    runtime_matrix: $proof_matrix:literal,
                    fallback_status: $proof_fallback:path $(,)?
                }
            ),+ $(,)?
        }
    ) => {
        /// Зарегистрированный migrated site, где число влияет на semantic branch.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[non_exhaustive]
        pub enum NumericalSiteIdV1 {
            $($(#[$legacy_meta])* $legacy_variant),+
        }

        impl NumericalSiteIdV1 {
            /// Стабильный wire/registry key.
            pub fn key(self) -> &'static str {
                match self {
                    $(Self::$legacy_variant => $legacy_key),+
                }
            }
        }

        /// Registered site in the single public proof-capable registry.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[non_exhaustive]
        pub enum NumericalSiteIdV2 {
            $($(#[$legacy_meta])* $legacy_variant),+,
            $($(#[$proof_meta])* $proof_variant),+
        }

        impl NumericalSiteIdV2 {
            /// Stable wire/registry key.
            pub fn key(self) -> &'static str {
                match self {
                    $(Self::$legacy_variant => $legacy_key),+,
                    $(Self::$proof_variant => $proof_key),+
                }
            }
        }

        const NUMERICAL_REGISTRY_V1: &[NumericalSiteRecordV1] = &[
            $(NumericalSiteRecordV1 {
                site_id: NumericalSiteIdV1::$legacy_variant,
                operations: $legacy_operations,
                domain: $legacy_domain,
                branch_effect: $legacy_branch_effect,
                stable_outcomes: &[$(StableNumericalOutcomeV1::$legacy_stable),+],
                compatibility_releases: &[$($legacy_release),*],
                evidence_classes: &[$(NumericalEvidenceClassV1::$legacy_evidence),*],
                artifact_ids: &[],
                bound_ids: &[],
                runtime_attestations: &[],
                bound_status: NumericalBoundStatusV1::$legacy_bound,
                boundary_corpus: $legacy_corpus,
                runtime_matrix: $legacy_matrix,
                fallback_status: $legacy_fallback,
            }),+
        ];

        const NUMERICAL_REGISTRY_V2: &[NumericalSiteRecordV2] = &[
            $(NumericalSiteRecordV2 {
                site_id: NumericalSiteIdV2::$legacy_variant,
                operations: $legacy_operations,
                domain: $legacy_domain,
                branch_effect: $legacy_branch_effect,
                stable_outcomes: &[$(StableNumericalOutcomeV2::$legacy_stable),+],
                compatibility_releases: &[$($legacy_release),*],
                evidence_classes: &[$(NumericalEvidenceClassV2::$legacy_evidence),*],
                artifact_ids: &[],
                bound_ids: &[],
                proof_ids: &[],
                runtime_attestations: &[],
                bound_status: NumericalBoundStatusV2::$legacy_bound,
                boundary_corpus: $legacy_corpus,
                runtime_matrix: $legacy_matrix,
                fallback_status: $legacy_fallback,
            }),+,
            $(NumericalSiteRecordV2 {
                site_id: NumericalSiteIdV2::$proof_variant,
                operations: $proof_operations,
                domain: $proof_domain,
                branch_effect: $proof_branch_effect,
                stable_outcomes: &[$(StableNumericalOutcomeV2::$proof_stable),+],
                compatibility_releases: &[$($proof_release),*],
                evidence_classes: &[$(NumericalEvidenceClassV2::$proof_evidence),+],
                artifact_ids: &[$($proof_artifact),+],
                bound_ids: &[$($proof_bound_id),+],
                proof_ids: &[$($proof_id),+],
                runtime_attestations: &[],
                bound_status: NumericalBoundStatusV2::$proof_bound,
                boundary_corpus: $proof_corpus,
                runtime_matrix: $proof_matrix,
                fallback_status: $proof_fallback,
            }),+
        ];
    };
}

define_numerical_registries! {
    legacy {
        /// Glow: первый state, достигший target, либо глобальный максимум.
        GlowTargetOrMaximumV1 => {
            key: "glow-target-or-maximum-v1",
            operations: "CAM16 forward powf; CAM16-UCS J-prime; abs; target >=; maximum ordering",
            domain: "encoded sRGB8 point screen states -> diagnostic CAM16-UCS delta J-prime",
            branch_effect: "first reached state versus global maximum and reached/unreachable status",
            stable_outcomes: [BitExact, Indeterminate],
            compatibility_releases: [
                NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,
            ],
            evidence_classes: [BitExact],
            bound_status: Unavailable,
            boundary_corpus: "glow stable-indeterminate; exact no-op; finite-state compositor; half-tie alpha",
            runtime_matrix: "active: native x86_64 + wasm32; native arm64 required before any cross-runtime CAM16 decision claim; exact bytes only for compositor",
            fallback_status: NumericalFallbackStatusV1::None,
        },
    }
    proof {
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
    }
}

/// Registry уже мигрированных typed-decision sites V1.
///
/// Это не заявление о завершённом аудите старых `f64` branches: его владелец —
/// #291. Новый site, переводимый на stable typed decision, обязан получить
/// строку до изменения runtime behavior.
#[cfg(test)]
pub(crate) fn numerical_registry_v1() -> &'static [NumericalSiteRecordV1] {
    NUMERICAL_REGISTRY_V1
}

/// Строка registry для site, если он зарегистрирован.
pub(crate) fn registry_row(site_id: NumericalSiteIdV1) -> Option<&'static NumericalSiteRecordV1> {
    NUMERICAL_REGISTRY_V1
        .iter()
        .find(|row| row.site_id == site_id)
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
}

impl NumericalErrorBoundIdV2 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Wcag22Srgb8OutwardQ55V1 => "wcag22-srgb8-outward-q55-v1",
        }
    }
}

/// Replayable proof identities admitted by V2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalProofIdV2 {
    /// Full sRGB8-domain proof with zero unresolved WCAG 3.0/4.5 decisions.
    Wcag22Srgb8FullDomainQ55V1,
}

impl NumericalProofIdV2 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Wcag22Srgb8FullDomainQ55V1 => "wcag22-srgb8-full-domain-q55-v1",
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

// ── Result evidence и атомарные terminal outcomes ───────────────────────────

/// Конечный упорядоченный интервал `[lower, upper]` — диагностический payload.
///
/// Конструктор проверяет только форму (конечность, порядок) и НЕ доказывает,
/// что истинное значение лежит внутри. Determinate evidence из интервала не
/// изготовляется: он живёт только внутри
/// [`NumericalIndeterminacyV1::IntervalOverlap`]. Bounded determinate evidence
/// минтится отдельно только для proof-capable registry V2 и никогда не выводится
/// из caller-created диагностического интервала.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutwardIntervalV1 {
    lower: f64,
    upper: f64,
}

impl OutwardIntervalV1 {
    /// Создаёт конечный непустой интервал; перепутанные/NaN bounds — ошибка.
    pub fn try_new(lower: f64, upper: f64) -> Result<Self, String> {
        if !lower.is_finite() || !upper.is_finite() || lower > upper {
            return Err(format!(
                "outward interval вне домена: lower={lower}, upper={upper}"
            ));
        }
        Ok(Self { lower, upper })
    }

    /// Нижняя заявленная граница.
    pub fn lower(self) -> f64 {
        self.lower
    }

    /// Верхняя заявленная граница.
    pub fn upper(self) -> f64 {
        self.upper
    }
}

/// Неразделимое evidence, почему stable semantic decision нельзя принять.
///
/// Вариант владеет своим доказательством: невозможно сконструировать
/// `IntervalOverlap` без interval или приписать outward bound отсутствующему
/// sound model.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NumericalIndeterminacyV1 {
    /// Для используемого transcendental backend нет sound error bound.
    SoundBoundUnavailable,
    /// Заявленный interval пересекает semantic boundary (диагностика).
    IntervalOverlap(OutwardIntervalV1),
}

impl NumericalIndeterminacyV1 {
    /// Стабильный wire key причины.
    pub fn reason_key(self) -> &'static str {
        match self {
            Self::SoundBoundUnavailable => "sound-bound-unavailable",
            Self::IntervalOverlap(_) => "interval-overlap",
        }
    }
}

/// Provenance-класс текущего нехарактеризованного legacy-пути. Тип-уровневый
/// маркер: `Compatibility` физически не может нести stable/exact provenance.
/// `PlatformCharacterized` не существует до immutable attestation registry (#258).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegacyPlatformDependentV1;

impl LegacyPlatformDependentV1 {
    /// Стабильный wire key класса.
    pub fn key(self) -> &'static str {
        "legacy-platform-dependent-v1"
    }
}

/// Печать evidence: тип публичен (входит в публичный enum-вариант), но его
/// приватное поле делает конструирование возможным только внутри модуля —
/// внешний код может лишь матчить вариант через `..`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidenceSeal {
    _private: (),
}

/// Запечатанное evidence determinate-решения. Опубликованный BitExact-путь
/// остаётся привязан к registry V1; новый bounded-вариант может ссылаться
/// только на proof-capable identity из registry V2.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub enum NumericalDecisionEvidenceV1 {
    /// Решение следует из точного конечного состояния объявленного
    /// reference-профиля.
    BitExact {
        /// Профиль, в чьём точном домене доказан результат.
        reference_profile_id: ReferenceProfileIdV1,
        /// Печать: внешняя конструкция невозможна (тип поля приватен).
        _seal: EvidenceSeal,
    },
    /// Решение следует из зарегистрированного canonical finite artifact:
    /// outward-границы + целочисленные пороговые законы (#284).
    CanonicalFiniteBounded(crate::wcag22_evidence::CanonicalFiniteBoundedEvidenceV1),
}

impl NumericalDecisionEvidenceV1 {
    /// Стабильный wire key класса evidence.
    pub fn class_key(&self) -> &'static str {
        match self {
            Self::BitExact { .. } => "bit-exact",
            Self::CanonicalFiniteBounded(_) => "canonical-finite-bounded",
        }
    }
}

/// Registry-owned минт BitExact-evidence: допустим только для site, чья
/// capability-строка объявляет класс BitExact.
///
/// # Errors
///
/// Site не зарегистрирован либо не объявляет BitExact.
pub(crate) fn mint_bit_exact_evidence(
    site_id: NumericalSiteIdV1,
    reference_profile_id: ReferenceProfileIdV1,
) -> Result<NumericalDecisionEvidenceV1, String> {
    let row = registry_row(site_id)
        .ok_or_else(|| format!("site {} отсутствует в registry V1", site_id.key()))?;
    mint_bit_exact_for_row(row, reference_profile_id)
}

/// Row-уровневый минт — отделён от registry-lookup, чтобы отказная ветвь
/// (row без объявленного BitExact) была проверяема на синтетической строке.
fn mint_bit_exact_for_row(
    row: &NumericalSiteRecordV1,
    reference_profile_id: ReferenceProfileIdV1,
) -> Result<NumericalDecisionEvidenceV1, String> {
    if !row
        .evidence_classes
        .contains(&NumericalEvidenceClassV1::BitExact)
    {
        return Err(format!(
            "site {} не объявляет evidence class bit-exact",
            row.site_id.key()
        ));
    }
    Ok(NumericalDecisionEvidenceV1::BitExact {
        reference_profile_id,
        _seal: EvidenceSeal { _private: () },
    })
}

/// Атомарный терминальный результат численного решения.
///
/// Три законных класса; их смешение непредставимо типами:
///
/// * `Determinate` — доказанное решение с запечатанным evidence;
/// * `Compatibility` — явный прежний алгоритм (registered release) c
///   provenance-классом `LegacyPlatformDependentV1`; не determinate evidence
///   и не конвертируется в BitExact/Bounded/Proven*;
/// * `Indeterminate` — stable branch честно не выбран.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum NumericalDecisionV1<T> {
    /// Решение принято под запечатанным evidence.
    #[non_exhaustive]
    Determinate {
        /// Зарегистрированный site.
        site_id: NumericalSiteIdV1,
        /// Предметный результат.
        value: T,
        /// Запечатанное registry-owned evidence.
        evidence: NumericalDecisionEvidenceV1,
    },
    /// Явно выбранный зарегистрированный прежний алгоритм.
    #[non_exhaustive]
    Compatibility {
        /// Зарегистрированный site.
        site_id: NumericalSiteIdV1,
        /// Registered release, реально исполнивший invocation.
        release_id: NumericalCompatibilityReleaseIdV1,
        /// Предметный результат.
        value: T,
        /// Класс происхождения (не заменяет release identity).
        provenance: LegacyPlatformDependentV1,
    },
    /// Stable branch не выбран.
    #[non_exhaustive]
    Indeterminate {
        /// Зарегистрированный site.
        site_id: NumericalSiteIdV1,
        /// Причина и её неразделимое numerical evidence.
        evidence: NumericalIndeterminacyV1,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrated_registry_is_non_vacuous_unique_and_covers_glow_site() {
        let rows = numerical_registry_v1();
        assert!(!rows.is_empty());
        assert!(rows.iter().any(|row| {
            row.site_id == NumericalSiteIdV1::GlowTargetOrMaximumV1
                && row.stable_outcomes
                    == [
                        StableNumericalOutcomeV1::BitExact,
                        StableNumericalOutcomeV1::Indeterminate,
                    ]
                && row.compatibility_releases
                    == [NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1]
                && row.evidence_classes == [NumericalEvidenceClassV1::BitExact]
                && row.artifact_ids.is_empty()
                && row.bound_ids.is_empty()
                && row.runtime_attestations.is_empty()
                && row.bound_status == NumericalBoundStatusV1::Unavailable
                && row.fallback_status == NumericalFallbackStatusV1::None
        }));
        for (index, row) in rows.iter().enumerate() {
            assert!(!row.operations.is_empty());
            assert!(!row.domain.is_empty());
            assert!(!row.branch_effect.is_empty());
            assert!(!row.stable_outcomes.is_empty());
            assert!(!row.boundary_corpus.is_empty());
            assert!(!row.runtime_matrix.is_empty());
            assert!(
                rows[..index]
                    .iter()
                    .all(|previous| previous.site_id != row.site_id),
                "duplicate numerical site: {}",
                row.site_id.key()
            );
            // ВСЕ set-поля checksum-preimage не имеют дубликатов.
            let unique = |keys: Vec<&str>| {
                let mut sorted = keys.clone();
                sorted.sort_unstable();
                sorted.dedup();
                assert_eq!(sorted.len(), keys.len(), "дубликат в set-поле");
            };
            unique(row.stable_outcomes.iter().map(|v| v.key()).collect());
            unique(row.compatibility_releases.iter().map(|v| v.key()).collect());
            unique(row.evidence_classes.iter().map(|v| v.key()).collect());
            unique(row.artifact_ids.iter().map(|v| v.key()).collect());
            unique(row.bound_ids.iter().map(|v| v.key()).collect());
            unique(row.runtime_attestations.iter().map(|v| v.key()).collect());
        }
    }

    #[test]
    fn unified_registry_projects_runtime_glow_and_proof_bound_wcag() {
        assert_eq!(numerical_registry_v1().len(), 1);
        let rows = numerical_registry_v2();
        assert_eq!(rows.len(), 2);
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
    }

    /// Диагностический интервал проверяет только форму.
    #[test]
    fn diagnostic_interval_validates_shape_only() {
        assert!(OutwardIntervalV1::try_new(2.0, 1.0).is_err());
        assert!(OutwardIntervalV1::try_new(f64::NAN, 1.0).is_err());
        let interval = OutwardIntervalV1::try_new(0.5, 1.5).unwrap();
        assert_eq!(
            NumericalIndeterminacyV1::IntervalOverlap(interval).reason_key(),
            "interval-overlap"
        );
    }

    /// Минт отклоняет строку без объявленного BitExact (registry-owned
    /// закон): отказная ветвь исполняется на синтетической строке, успешная —
    /// на настоящем registry.
    #[test]
    fn bit_exact_mint_is_refused_without_declared_capability() {
        let minted = mint_bit_exact_evidence(
            NumericalSiteIdV1::GlowTargetOrMaximumV1,
            ReferenceProfileIdV1::EncodedSrgb8ScreenV1,
        );
        assert!(minted.is_ok(), "Glow объявляет bit-exact");

        let mut orphan = *registry_row(NumericalSiteIdV1::GlowTargetOrMaximumV1).unwrap();
        orphan.evidence_classes = &[];
        let refused = mint_bit_exact_for_row(&orphan, ReferenceProfileIdV1::EncodedSrgb8ScreenV1);
        assert!(
            refused.is_err(),
            "строка без BitExact обязана отклонять минт"
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

#[cfg(test)]
mod red_292_tests {
    use super::*;

    /// RED #292: legacy-результат — атомарный `Compatibility` с registered
    /// release ID, НЕ determinate evidence; изготовить его как
    /// `Determinate/BitExact` невозможно типами.
    #[test]
    fn legacy_result_is_compatibility_not_determinate_evidence() {
        let vc = crate::spaces::vc::ViewingConditions::srgb();
        let decision = crate::glow::solve_screen_alpha_for_dj(
            "#FF6633",
            "#101012",
            1.0,
            NumericalExecutionModeV1::ExplicitCompatibility {
                release_id: NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,
            },
            &vc,
        )
        .unwrap();
        assert!(matches!(
            decision,
            NumericalDecisionV1::Compatibility {
                site_id: NumericalSiteIdV1::GlowTargetOrMaximumV1,
                release_id: NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,
                provenance: LegacyPlatformDependentV1,
                ..
            }
        ));
    }

    /// RED #292: BitExact-evidence запечатан и registry-owned — минтится
    /// только для site, чья capability-строка объявляет класс BitExact.
    #[test]
    fn bit_exact_evidence_is_registry_owned_and_sealed() {
        let minted = mint_bit_exact_evidence(
            NumericalSiteIdV1::GlowTargetOrMaximumV1,
            ReferenceProfileIdV1::EncodedSrgb8ScreenV1,
        )
        .expect("Glow site объявляет BitExact");
        assert!(matches!(
            minted,
            NumericalDecisionEvidenceV1::BitExact {
                reference_profile_id: ReferenceProfileIdV1::EncodedSrgb8ScreenV1,
                ..
            }
        ));
    }

    /// RED #292/#289/#284: единственный capability manifest — proof-capable
    /// V2 projection с каноническим checksum; выбранный mode отсутствует.
    #[test]
    fn capability_manifest_is_canonical_registry_projection() {
        let manifest = numerical_capability_manifest_v2();
        assert!(matches!(
            manifest.coverage,
            NumericalRegistryCoverageV2::MigratedSitesOnlyV1
        ));
        assert_eq!(manifest.sites.len(), 2);
        let site = manifest
            .sites
            .iter()
            .find(|site| site.site_id == NumericalSiteIdV2::GlowTargetOrMaximumV1)
            .expect("Glow capability row");
        assert_eq!(site.site_id, NumericalSiteIdV2::GlowTargetOrMaximumV1);
        assert_eq!(
            site.stable_outcomes,
            [
                StableNumericalOutcomeV2::BitExact,
                StableNumericalOutcomeV2::Indeterminate,
            ]
        );
        assert_eq!(
            site.compatibility_releases,
            [NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,]
        );
        assert_eq!(site.evidence_classes, [NumericalEvidenceClassV2::BitExact]);
        assert!(site.artifact_ids.is_empty());
        assert!(site.bound_ids.is_empty());
        assert!(site.proof_ids.is_empty());
        assert!(site.runtime_attestations.is_empty());
        // Checksum детерминирован и воспроизводим из canonical preimage.
        assert!(manifest.sites.iter().any(|site| {
            site.site_id == NumericalSiteIdV2::Wcag22Srgb8ContrastV1
                && site.proof_ids == [NumericalProofIdV2::Wcag22Srgb8FullDomainQ55V1]
        }));
        assert_eq!(
            manifest.checksum,
            NumericalCapabilityChecksumV2::from_preimage(&manifest.canonical_checksum_preimage())
        );
    }

    use crate::numerical_plan::NumericalExecutionModeV1;
}
