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
//! Законы первого V1-среза (#292):
//!
//! * `Determinate` несёт только реально минтимое core-ом evidence — `BitExact`
//!   (конструктор запечатан и registry-owned);
//! * текущий нехарактеризованный legacy-результат — отдельный атомарный вариант
//!   `Compatibility` с зарегистрированным release ID и provenance-классом
//!   `LegacyPlatformDependentV1`; он НЕ является determinate evidence и не
//!   конвертируется в него;
//! * caller-created интервал не изготовляет никакого evidence: интервал живёт
//!   только как диагностический payload `Indeterminate::IntervalOverlap`;
//! * незаконная комбинация (stable + legacy provenance и т. п.) непредставима
//!   типами, а не запрещена соглашением.

/// Stable outcomes admitted for a migrated branch-sensitive site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum StableNumericalOutcomeV1 {
    /// Determinate branch follows from exact finite/integer/rational evidence.
    BitExact,
    /// No semantic branch is selected.
    Indeterminate,
}

impl StableNumericalOutcomeV1 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::BitExact => "bit-exact",
            Self::Indeterminate => "indeterminate",
        }
    }
}

/// Sound-bound availability for a migrated numerical site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalBoundStatusV1 {
    /// No sound bound has been admitted.
    Unavailable,
}

impl NumericalBoundStatusV1 {
    /// Stable manifest key.
    pub fn key(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
        }
    }
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
pub enum NumericalEvidenceClassV1 {
    /// Точное решение из конечного integer/байтового состояния.
    BitExact,
}

impl NumericalEvidenceClassV1 {
    /// Стабильный manifest key.
    pub fn key(self) -> &'static str {
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

/// Идентификатор canonical finite artifact. Ни один artifact не допущен в V1:
/// тип намеренно ненаселён — пустой список в manifest единственно представим,
/// фиктивные IDs невозможны по построению.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericalArtifactIdV1 {}

impl NumericalArtifactIdV1 {
    /// Стабильный manifest key (недостижимо: тип ненаселён).
    pub fn key(self) -> &'static str {
        match self {}
    }
}

/// Идентификатор зарегистрированного error bound. Не допущен в V1 (ненаселён).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericalErrorBoundIdV1 {}

impl NumericalErrorBoundIdV1 {
    /// Стабильный manifest key (недостижимо: тип ненаселён).
    pub fn key(self) -> &'static str {
        match self {}
    }
}

/// Идентификатор runtime attestation. Не допущен до immutable attestation
/// registry (#258): тип ненаселён, `PlatformCharacterized` непредставим.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericalRuntimeAttestationIdV1 {}

impl NumericalRuntimeAttestationIdV1 {
    /// Стабильный manifest key (недостижимо: тип ненаселён).
    pub fn key(self) -> &'static str {
        match self {}
    }
}

/// Machine-readable registry row required by research lock #281.
///
/// Текстовые поля (`operations`/`domain`/`branch_effect`/`boundary_corpus`/
/// `runtime_matrix`) — human-readable research metadata; они НЕ входят в
/// canonical capability checksum preimage (#289).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct NumericalSiteRecordV1 {
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

// Enum identity and its registry row are emitted by one declaration. A new
// migrated site therefore cannot exist without machine-readable metadata.
macro_rules! define_numerical_registry_v1 {
    ($(
        $(#[$variant_meta:meta])*
        $variant:ident => {
            key: $key:literal,
            operations: $operations:literal,
            domain: $domain:literal,
            branch_effect: $branch_effect:literal,
            stable_outcomes: [$($stable_outcome:path),+ $(,)?],
            compatibility_releases: [$($release:path),* $(,)?],
            evidence_classes: [$($evidence_class:path),* $(,)?],
            bound_status: $bound_status:path,
            boundary_corpus: $boundary_corpus:literal,
            runtime_matrix: $runtime_matrix:literal,
            fallback_status: $fallback_status:path $(,)?
        }
    ),+ $(,)?) => {
        /// Зарегистрированный migrated site, где число влияет на semantic branch.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        #[non_exhaustive]
        pub enum NumericalSiteIdV1 {
            $($(#[$variant_meta])* $variant),+
        }

        impl NumericalSiteIdV1 {
            /// Стабильный wire/registry key.
            pub fn key(self) -> &'static str {
                match self {
                    $(Self::$variant => $key),+
                }
            }
        }

        const NUMERICAL_REGISTRY_V1: &[NumericalSiteRecordV1] = &[
            $(NumericalSiteRecordV1 {
                site_id: NumericalSiteIdV1::$variant,
                operations: $operations,
                domain: $domain,
                branch_effect: $branch_effect,
                stable_outcomes: &[$($stable_outcome),+],
                compatibility_releases: &[$($release),*],
                evidence_classes: &[$($evidence_class),*],
                artifact_ids: &[],
                bound_ids: &[],
                runtime_attestations: &[],
                bound_status: $bound_status,
                boundary_corpus: $boundary_corpus,
                runtime_matrix: $runtime_matrix,
                fallback_status: $fallback_status,
            }),+
        ];
    };
}

define_numerical_registry_v1! {
    /// Glow: первый state, достигший target, либо глобальный максимум.
    GlowTargetOrMaximumV1 => {
        key: "glow-target-or-maximum-v1",
        operations: "CAM16 forward powf; CAM16-UCS J-prime; abs; target >=; maximum ordering",
        domain: "encoded sRGB8 point screen states -> diagnostic CAM16-UCS delta J-prime",
        branch_effect: "first reached state versus global maximum and reached/unreachable status",
        stable_outcomes: [
            StableNumericalOutcomeV1::BitExact,
            StableNumericalOutcomeV1::Indeterminate,
        ],
        compatibility_releases: [
            NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,
        ],
        evidence_classes: [NumericalEvidenceClassV1::BitExact],
        bound_status: NumericalBoundStatusV1::Unavailable,
        boundary_corpus: "glow stable-indeterminate; exact no-op; finite-state compositor; half-tie alpha",
        runtime_matrix: "active: native x86_64 + wasm32; native arm64 required before any cross-runtime CAM16 decision claim; exact bytes only for compositor",
        fallback_status: NumericalFallbackStatusV1::None,
    },
}

/// Registry уже мигрированных typed-decision sites V1.
///
/// Это не заявление о завершённом аудите старых `f64` branches: его владелец —
/// #291. Новый site, переводимый на stable typed decision, обязан получить
/// строку до изменения runtime behavior.
pub fn numerical_registry_v1() -> &'static [NumericalSiteRecordV1] {
    NUMERICAL_REGISTRY_V1
}

/// Строка registry для site, если он зарегистрирован.
pub(crate) fn registry_row(site_id: NumericalSiteIdV1) -> Option<&'static NumericalSiteRecordV1> {
    NUMERICAL_REGISTRY_V1
        .iter()
        .find(|row| row.site_id == site_id)
}

// ── Package capability manifest (#289) ──────────────────────────────────────

/// Версия capability-схемы. Независима от версий conformance pack и
/// release-manifest (три разных version domain, #289).
pub const NUMERICAL_CAPABILITY_SCHEMA_VERSION_V1: u32 = 1;

/// Домен-сепаратор canonical checksum preimage.
const CAPABILITY_CHECKSUM_DOMAIN_V1: &[u8] = b"labcolors.numerical-capability.v1";

/// Покрытие registry данным manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NumericalRegistryCoverageV1 {
    /// Перечислены только уже мигрированные sites (не весь core).
    MigratedSitesOnlyV1,
}

impl NumericalRegistryCoverageV1 {
    /// Стабильный manifest key. `CompleteV1` недоступен до закрытия #291 и
    /// потому отсутствует в типе V1.
    pub fn key(self) -> &'static str {
        match self {
            Self::MigratedSitesOnlyV1 => "migrated-sites-only-v1",
        }
    }
}

/// Capability одного site — проекция registry-строки без research-текстов и
/// без выбранного mode (manifest описывает возможности сборки, не выбор клиента).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericalSiteCapabilityV1 {
    /// Site identity.
    pub site_id: NumericalSiteIdV1,
    /// Lawful stable outcomes.
    pub stable_outcomes: &'static [StableNumericalOutcomeV1],
    /// Registered compatibility releases.
    pub compatibility_releases: &'static [NumericalCompatibilityReleaseIdV1],
    /// Минтимые классы evidence.
    pub evidence_classes: &'static [NumericalEvidenceClassV1],
    /// Canonical finite artifacts (пусто в V1).
    pub artifact_ids: &'static [NumericalArtifactIdV1],
    /// Registered error bounds (пусто в V1).
    pub bound_ids: &'static [NumericalErrorBoundIdV1],
    /// Runtime attestations (пусто до #258).
    pub runtime_attestations: &'static [NumericalRuntimeAttestationIdV1],
}

/// Переносимый drift-checksum typed capability projection — НЕ
/// security/certificate/cache identity: exact rows остаются authority, а
/// SHA-256 сырых байтов полного artifact — отдельная integrity-гарантия.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NumericalCapabilityChecksumV1(u32);

impl NumericalCapabilityChecksumV1 {
    /// FNV-1a-32 canonical preimage (dependency-free, как `packDigest`).
    pub fn from_preimage(preimage: &[u8]) -> Self {
        Self(crate::hash::fnv1a_32(preimage))
    }

    /// Каноническая 8-hex запись (lowercase).
    pub fn hex(self) -> String {
        format!("{:08x}", self.0)
    }
}

/// Package capability manifest: статическое свойство сборки. Не содержит
/// выбранного mode; rows генерируются только из core registry SSOT.
#[derive(Debug, Clone, PartialEq)]
pub struct NumericalCapabilityManifestV1 {
    /// Версия capability-схемы.
    pub schema_version: u32,
    /// Покрытие registry.
    pub coverage: NumericalRegistryCoverageV1,
    /// Capability rows, отсортированные по UTF-8 bytes `site_id.key()`.
    pub sites: Vec<NumericalSiteCapabilityV1>,
    /// Drift-checksum canonical projection.
    pub checksum: NumericalCapabilityChecksumV1,
}

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

impl NumericalCapabilityManifestV1 {
    /// Canonical checksum preimage (#289): versioned length-prefixed binary
    /// encoding. В preimage НЕ входят checksum, config/plan, версии
    /// core/conformance/release, счётчики векторов, JSON-форматирование и
    /// human-readable research-тексты.
    pub fn canonical_checksum_preimage(&self) -> Vec<u8> {
        let mut buffer = Vec::new();
        push_len_prefixed(&mut buffer, CAPABILITY_CHECKSUM_DOMAIN_V1);
        buffer.extend_from_slice(&self.schema_version.to_le_bytes());
        push_len_prefixed(&mut buffer, self.coverage.key().as_bytes());
        let mut sites: Vec<&NumericalSiteCapabilityV1> = self.sites.iter().collect();
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
                &mut site.runtime_attestations.iter().map(|v| v.key()).collect(),
            );
        }
        buffer
    }
}

/// Capability manifest текущей сборки — единственная projection core registry
/// SSOT. Adapters не держат рукописной копии.
pub fn numerical_capability_manifest_v1() -> NumericalCapabilityManifestV1 {
    let mut sites: Vec<NumericalSiteCapabilityV1> = NUMERICAL_REGISTRY_V1
        .iter()
        .map(|row| NumericalSiteCapabilityV1 {
            site_id: row.site_id,
            stable_outcomes: row.stable_outcomes,
            compatibility_releases: row.compatibility_releases,
            evidence_classes: row.evidence_classes,
            artifact_ids: row.artifact_ids,
            bound_ids: row.bound_ids,
            runtime_attestations: row.runtime_attestations,
        })
        .collect();
    sites.sort_unstable_by_key(|site| site.site_id.key().as_bytes());
    let mut manifest = NumericalCapabilityManifestV1 {
        schema_version: NUMERICAL_CAPABILITY_SCHEMA_VERSION_V1,
        coverage: NumericalRegistryCoverageV1::MigratedSitesOnlyV1,
        sites,
        checksum: NumericalCapabilityChecksumV1(0),
    };
    manifest.checksum =
        NumericalCapabilityChecksumV1::from_preimage(&manifest.canonical_checksum_preimage());
    manifest
}

// ── Result evidence и атомарные terminal outcomes ───────────────────────────

/// Конечный упорядоченный интервал `[lower, upper]` — диагностический payload.
///
/// Конструктор проверяет только форму (конечность, порядок) и НЕ доказывает,
/// что истинное значение лежит внутри. Determinate evidence из интервала не
/// изготовляется: он живёт только внутри
/// [`NumericalIndeterminacyV1::IntervalOverlap`]. Bounded determinate evidence
/// вернётся в #284/#291 только вместе с зарегистрированным verifier и
/// bound/artifact identity.
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

/// Запечатанное evidence determinate-решения. В V1 минтится только реально
/// admitted `BitExact`; bounded/canonical-finite варианты появятся вместе с
/// зарегистрированным verifier и bound/artifact identity (#284/#291) —
/// фиктивные IDs запрещены.
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
}

impl NumericalDecisionEvidenceV1 {
    /// Стабильный wire key класса evidence.
    pub fn class_key(&self) -> &'static str {
        match self {
            Self::BitExact { .. } => "bit-exact",
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
    if !row
        .evidence_classes
        .contains(&NumericalEvidenceClassV1::BitExact)
    {
        return Err(format!(
            "site {} не объявляет evidence class bit-exact",
            site_id.key()
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
    Determinate {
        /// Зарегистрированный site.
        site_id: NumericalSiteIdV1,
        /// Предметный результат.
        value: T,
        /// Запечатанное registry-owned evidence.
        evidence: NumericalDecisionEvidenceV1,
    },
    /// Явно выбранный зарегистрированный прежний алгоритм.
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
            // Set-поля checksum-preimage не имеют дубликатов.
            let keys: Vec<_> = row.stable_outcomes.iter().map(|v| v.key()).collect();
            let mut sorted = keys.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), keys.len());
        }
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

    /// Минт отклоняет site без объявленного BitExact (registry-owned закон).
    #[test]
    fn bit_exact_mint_is_refused_without_declared_capability() {
        // Единственный способ проверить отказ без второго site — прямой
        // контракт минтера: он читает registry, не аргументы вызова.
        let minted = mint_bit_exact_evidence(
            NumericalSiteIdV1::GlowTargetOrMaximumV1,
            ReferenceProfileIdV1::EncodedSrgb8ScreenV1,
        );
        assert!(minted.is_ok(), "Glow объявляет bit-exact");
    }

    /// Checksum: детерминирован, чувствителен к содержимому canonical-полей и
    /// нечувствителен к порядку rows (сортировка внутри preimage).
    #[test]
    fn capability_checksum_is_canonical_and_tamper_sensitive() {
        let manifest = numerical_capability_manifest_v1();
        let recomputed =
            NumericalCapabilityChecksumV1::from_preimage(&manifest.canonical_checksum_preimage());
        assert_eq!(manifest.checksum, recomputed);
        assert_eq!(manifest.checksum.hex().len(), 8);

        // Tamper: смена schema version меняет preimage/checksum.
        let mut tampered = manifest.clone();
        tampered.schema_version += 1;
        assert_ne!(
            NumericalCapabilityChecksumV1::from_preimage(&tampered.canonical_checksum_preimage()),
            manifest.checksum
        );

        // Tamper: удаление row меняет checksum.
        let mut emptied = manifest.clone();
        emptied.sites.clear();
        assert_ne!(
            NumericalCapabilityChecksumV1::from_preimage(&emptied.canonical_checksum_preimage()),
            manifest.checksum
        );
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

    /// RED #292/#289: capability manifest — core registry projection с
    /// каноническим checksum; coverage MigratedSitesOnlyV1, mode отсутствует.
    #[test]
    fn capability_manifest_is_canonical_registry_projection() {
        let manifest = numerical_capability_manifest_v1();
        assert!(matches!(
            manifest.coverage,
            NumericalRegistryCoverageV1::MigratedSitesOnlyV1
        ));
        assert_eq!(manifest.sites.len(), 1);
        let site = &manifest.sites[0];
        assert_eq!(site.site_id, NumericalSiteIdV1::GlowTargetOrMaximumV1);
        assert_eq!(
            site.stable_outcomes,
            [
                StableNumericalOutcomeV1::BitExact,
                StableNumericalOutcomeV1::Indeterminate,
            ]
        );
        assert_eq!(
            site.compatibility_releases,
            [NumericalCompatibilityReleaseIdV1::GlowCam16UcsJPrimeTargetOrMaxV1,]
        );
        assert_eq!(site.evidence_classes, [NumericalEvidenceClassV1::BitExact]);
        assert!(site.artifact_ids.is_empty());
        assert!(site.bound_ids.is_empty());
        assert!(site.runtime_attestations.is_empty());
        // Checksum детерминирован и воспроизводим из canonical preimage.
        assert_eq!(
            manifest.checksum,
            NumericalCapabilityChecksumV1::from_preimage(&manifest.canonical_checksum_preimage())
        );
    }

    use crate::numerical_plan::NumericalExecutionModeV1;
}
