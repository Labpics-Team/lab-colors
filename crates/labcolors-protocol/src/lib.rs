//! Versioned bounded transport contracts shared by Lab Colors adapters.
//!
//! This crate is the only owner of feasibility wire parsing and Core-to-wire
//! projection. It performs no colour mathematics or canonicalization.

#![forbid(unsafe_code)]

use core::fmt;

use labcolors_core::Srgb8;
use labcolors_core::numerics::{
    NumericalArtifactIdV2 as CoreNumericalArtifactIdV2,
    NumericalErrorBoundIdV2 as CoreNumericalErrorBoundIdV2,
    NumericalProofIdV2 as CoreNumericalProofIdV2,
};
use labcolors_core::wcag22::{
    Wcag22ClientDeclaredNotApplicableV1 as CoreNotApplicableV1,
    Wcag22CriterionV1 as CoreWcag22CriterionV1,
    Wcag22EvaluationErrorV1 as CoreWcag22EvaluationErrorV1,
    Wcag22ProfileIdV1 as CoreWcag22ProfileIdV1,
};
use labcolors_core::wcag22_feasibility::{
    self as core_feasibility, CompilerInvariantV1 as CoreCompilerInvariantV1,
    DomainIdV1 as CoreDomainIdV1, ErrorV1 as CoreFeasibilityErrorV1,
    EvaluatedV1 as CoreEvaluatedV1, EvaluatorInvariantV1 as CoreEvaluatorInvariantV1,
    FeasibilityV1 as CoreFeasibilityV1, InvalidRequestV1 as CoreInvalidRequestSourceV1,
    NotEvaluatedV1 as CoreNotEvaluatedV1, OccurrenceId as CoreOccurrenceId,
    RelationId as CoreRelationId, RelationV1 as CoreRelationV1, RequestV1 as CoreRequestV1,
    ResourceDimensionV1 as CoreResourceDimensionV1, ResourceProfileIdV1 as CoreResourceProfileIdV1,
};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

#[cfg(feature = "wcag22-explicit-selection")]
pub mod explicit_selection;

/// Exact schema version of the request and outcome envelopes.
pub const SCHEMA_VERSION_V1: u32 = 1;

const FIXED_ENVELOPE_BYTES_V1: u64 = b"{\"schemaVersion\":1,\"domainId\":\"".len() as u64
    + CoreDomainIdV1::Srgb8NeutralAxis.key().len() as u64
    + b"\",\"resourceProfileId\":\"".len() as u64
    + CoreResourceProfileIdV1::Compile.key().len() as u64
    + b"\",\"relations\":[".len() as u64
    + b"]}".len() as u64;
pub(crate) const APPLICABLE_SKELETON_BYTES_V1: u64 =
    b"{\"relationId\":\"\",\"occurrenceId\":\"\",\"kind\":\"applicable\",\"criterion\":\"".len()
        as u64
        + CoreWcag22CriterionV1::Sc1411UiComponentOrState.key().len() as u64
        + b"\",\"adjacent\":[]}".len() as u64;
pub(crate) const NOT_APPLICABLE_SKELETON_BYTES_V1: u64 =
    b"{\"relationId\":\"\",\"occurrenceId\":\"\",\"kind\":\"notApplicable\",\"reasonId\":\"\"}"
        .len() as u64;
pub(crate) const MAX_RGB_TRIPLE_BYTES_V1: u64 = b"[255,255,255]".len() as u64;
pub(crate) const MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1: u64 = b"\\u0000".len() as u64;
pub(crate) const RAW_RELATION_LIMIT_V1: u64 =
    CoreResourceProfileIdV1::Compile.limit(CoreResourceDimensionV1::RawRelations);
pub(crate) const RAW_ADJACENT_LIMIT_V1: u64 =
    CoreResourceProfileIdV1::Compile.limit(CoreResourceDimensionV1::RawAdjacentEntries);
pub(crate) const OPAQUE_UTF8_LIMIT_V1: u64 =
    CoreResourceProfileIdV1::Compile.limit(CoreResourceDimensionV1::OpaqueUtf8Bytes);
const MAX_APPLICABLE_RELATIONS_V1: u64 = if RAW_RELATION_LIMIT_V1 < RAW_ADJACENT_LIMIT_V1 {
    RAW_RELATION_LIMIT_V1
} else {
    RAW_ADJACENT_LIMIT_V1
};
const MAX_NOT_APPLICABLE_RELATIONS_V1: u64 = RAW_RELATION_LIMIT_V1 - MAX_APPLICABLE_RELATIONS_V1;
const MAX_RELATION_SEPARATORS_V1: u64 = RAW_RELATION_LIMIT_V1.saturating_sub(1);
const MAX_ADJACENT_SEPARATORS_V1: u64 = RAW_ADJACENT_LIMIT_V1 - MAX_APPLICABLE_RELATIONS_V1;

/// Tight maximum byte length of a compact, Core-admissible V1 request.
///
/// The expression is derived from the literal JSON grammar above and the
/// public Core `Compile` profile. It has no discretionary headroom.
pub const MAX_ENVELOPE_BYTES_V1: u64 = FIXED_ENVELOPE_BYTES_V1
    + APPLICABLE_SKELETON_BYTES_V1 * MAX_APPLICABLE_RELATIONS_V1
    + NOT_APPLICABLE_SKELETON_BYTES_V1 * MAX_NOT_APPLICABLE_RELATIONS_V1
    + MAX_RELATION_SEPARATORS_V1
    + MAX_RGB_TRIPLE_BYTES_V1 * RAW_ADJACENT_LIMIT_V1
    + MAX_ADJACENT_SEPARATORS_V1
    + MAX_JSON_ESCAPE_BYTES_PER_OPAQUE_BYTE_V1 * OPAQUE_UTF8_LIMIT_V1;

/// Registered finite candidate domain exposed by protocol V1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainIdV1 {
    /// Exact ascending encoded-sRGB8 neutral axis.
    Srgb8NeutralAxis,
}

impl DomainIdV1 {
    /// Stable Core-owned wire key.
    pub const fn key(self) -> &'static str {
        CoreDomainIdV1::Srgb8NeutralAxis.key()
    }

    fn parse(key: &str) -> Option<Self> {
        (key == CoreDomainIdV1::Srgb8NeutralAxis.key()).then_some(Self::Srgb8NeutralAxis)
    }
}

impl Serialize for DomainIdV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.key())
    }
}

/// Bounded operational resource profile exposed by protocol V1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceProfileIdV1 {
    /// Offline compile profile.
    Compile,
}

impl ResourceProfileIdV1 {
    /// Stable Core-owned wire key.
    pub const fn key(self) -> &'static str {
        CoreResourceProfileIdV1::Compile.key()
    }

    fn parse(key: &str) -> Option<Self> {
        (key == CoreResourceProfileIdV1::Compile.key()).then_some(Self::Compile)
    }
}

impl Serialize for ResourceProfileIdV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.key())
    }
}

/// Exact WCAG occurrence criterion exposed by protocol V1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wcag22CriterionV1 {
    /// SC 1.4.3 ordinary text.
    Sc143TextDefault,
    /// SC 1.4.3 explicitly declared large-scale text.
    Sc143TextLargeScale,
    /// SC 1.4.11 UI component or state.
    Sc1411UiComponentOrState,
    /// SC 1.4.11 graphical object.
    Sc1411GraphicalObject,
}

impl Serialize for Wcag22CriterionV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.key())
    }
}

impl Wcag22CriterionV1 {
    /// Stable Core-owned wire key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Sc143TextDefault => CoreWcag22CriterionV1::Sc143TextDefault.key(),
            Self::Sc143TextLargeScale => CoreWcag22CriterionV1::Sc143TextLargeScale.key(),
            Self::Sc1411UiComponentOrState => CoreWcag22CriterionV1::Sc1411UiComponentOrState.key(),
            Self::Sc1411GraphicalObject => CoreWcag22CriterionV1::Sc1411GraphicalObject.key(),
        }
    }

    fn parse(key: &str) -> Option<Self> {
        match CoreWcag22CriterionV1::parse(key)? {
            CoreWcag22CriterionV1::Sc143TextDefault => Some(Self::Sc143TextDefault),
            CoreWcag22CriterionV1::Sc143TextLargeScale => Some(Self::Sc143TextLargeScale),
            CoreWcag22CriterionV1::Sc1411UiComponentOrState => Some(Self::Sc1411UiComponentOrState),
            CoreWcag22CriterionV1::Sc1411GraphicalObject => Some(Self::Sc1411GraphicalObject),
            _ => None,
        }
    }

    const fn into_core(self) -> CoreWcag22CriterionV1 {
        match self {
            Self::Sc143TextDefault => CoreWcag22CriterionV1::Sc143TextDefault,
            Self::Sc143TextLargeScale => CoreWcag22CriterionV1::Sc143TextLargeScale,
            Self::Sc1411UiComponentOrState => CoreWcag22CriterionV1::Sc1411UiComponentOrState,
            Self::Sc1411GraphicalObject => CoreWcag22CriterionV1::Sc1411GraphicalObject,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RelationKindV1 {
    Applicable {
        criterion: Wcag22CriterionV1,
        adjacent: Vec<[u8; 3]>,
    },
    NotApplicable {
        reason_id: String,
    },
}

/// One client-declared occurrence relation in the canonical V1 wire shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationV1 {
    relation_id: String,
    occurrence_id: String,
    kind: RelationKindV1,
}

impl RelationV1 {
    /// Construct a locally valid applicable relation.
    pub fn applicable(
        relation_id: impl Into<String>,
        occurrence_id: impl Into<String>,
        criterion: Wcag22CriterionV1,
        adjacent: Vec<[u8; 3]>,
    ) -> Result<Self, ProtocolErrorV1> {
        let relation_id = relation_id.into();
        let occurrence_id = occurrence_id.into();
        validate_relation_id(&relation_id)?;
        validate_occurrence_id(&occurrence_id)?;
        if adjacent.is_empty() {
            return Err(core_invalid(CoreInvalidRequestV1::EmptyAdjacentSet {
                relation_id,
            }));
        }
        Ok(Self {
            relation_id,
            occurrence_id,
            kind: RelationKindV1::Applicable {
                criterion,
                adjacent,
            },
        })
    }

    /// Construct a locally valid client-declared NotApplicable relation.
    pub fn not_applicable(
        relation_id: impl Into<String>,
        occurrence_id: impl Into<String>,
        reason_id: impl Into<String>,
    ) -> Result<Self, ProtocolErrorV1> {
        let relation_id = relation_id.into();
        let occurrence_id = occurrence_id.into();
        let reason_id = reason_id.into();
        validate_relation_id(&relation_id)?;
        validate_occurrence_id(&occurrence_id)?;
        if reason_id.is_empty() {
            return Err(ProtocolErrorV1::Transport(
                TransportErrorV1::EmptyNotApplicableReason,
            ));
        }
        Ok(Self {
            relation_id,
            occurrence_id,
            kind: RelationKindV1::NotApplicable { reason_id },
        })
    }

    /// Opaque relation identity.
    pub fn relation_id(&self) -> &str {
        &self.relation_id
    }

    /// Opaque occurrence identity.
    pub fn occurrence_id(&self) -> &str {
        &self.occurrence_id
    }

    /// Applicable criterion and exact adjacent byte triples, if applicable.
    pub fn as_applicable(&self) -> Option<(Wcag22CriterionV1, &[[u8; 3]])> {
        match &self.kind {
            RelationKindV1::Applicable {
                criterion,
                adjacent,
            } => Some((*criterion, adjacent)),
            RelationKindV1::NotApplicable { .. } => None,
        }
    }

    /// Opaque client reason, if declared NotApplicable.
    pub fn as_not_applicable(&self) -> Option<&str> {
        match &self.kind {
            RelationKindV1::Applicable { .. } => None,
            RelationKindV1::NotApplicable { reason_id } => Some(reason_id),
        }
    }

    fn into_core(self) -> Result<CoreRelationV1, ProtocolErrorV1> {
        let relation_id = CoreRelationId::try_new(self.relation_id).map_err(map_core_invalid)?;
        let occurrence_id =
            CoreOccurrenceId::try_new(self.occurrence_id).map_err(map_core_invalid)?;
        match self.kind {
            RelationKindV1::Applicable {
                criterion,
                adjacent,
            } => CoreRelationV1::applicable(
                relation_id,
                occurrence_id,
                criterion.into_core(),
                adjacent.into_iter().map(Srgb8::new).collect(),
            )
            .map_err(map_core_invalid),
            RelationKindV1::NotApplicable { reason_id } => {
                let declaration = CoreNotApplicableV1::try_new(reason_id).map_err(|error| {
                    if matches!(error, CoreWcag22EvaluationErrorV1::EmptyNotApplicableReason) {
                        ProtocolErrorV1::Transport(TransportErrorV1::EmptyNotApplicableReason)
                    } else {
                        ProtocolErrorV1::IncompatibleCoreContract
                    }
                })?;
                Ok(CoreRelationV1::not_applicable(
                    relation_id,
                    occurrence_id,
                    declaration,
                ))
            }
        }
    }
}

impl Serialize for RelationV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self.kind {
            RelationKindV1::Applicable {
                criterion,
                adjacent,
            } => {
                let mut state = serializer.serialize_struct("RelationV1", 5)?;
                state.serialize_field("relationId", &self.relation_id)?;
                state.serialize_field("occurrenceId", &self.occurrence_id)?;
                state.serialize_field("kind", "applicable")?;
                state.serialize_field("criterion", criterion)?;
                state.serialize_field("adjacent", adjacent)?;
                state.end()
            }
            RelationKindV1::NotApplicable { reason_id } => {
                let mut state = serializer.serialize_struct("RelationV1", 4)?;
                state.serialize_field("relationId", &self.relation_id)?;
                state.serialize_field("occurrenceId", &self.occurrence_id)?;
                state.serialize_field("kind", "notApplicable")?;
                state.serialize_field("reasonId", reason_id)?;
                state.end()
            }
        }
    }
}

/// Validated V1 request that the canonical encoder can serialize.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestV1 {
    schema_version: u32,
    domain_id: DomainIdV1,
    resource_profile_id: ResourceProfileIdV1,
    relations: Vec<RelationV1>,
}

impl RequestV1 {
    /// Construct a locally valid request. Aggregate resource and conflict laws
    /// remain Core-owned and are evaluated only after transport preflight.
    pub fn try_new(
        domain_id: DomainIdV1,
        relations: Vec<RelationV1>,
        resource_profile_id: ResourceProfileIdV1,
    ) -> Result<Self, ProtocolErrorV1> {
        if relations.is_empty() {
            return Err(core_invalid(CoreInvalidRequestV1::EmptyRelations));
        }
        Ok(Self {
            schema_version: SCHEMA_VERSION_V1,
            domain_id,
            resource_profile_id,
            relations,
        })
    }

    /// Registered domain.
    pub const fn domain_id(&self) -> DomainIdV1 {
        self.domain_id
    }

    /// Operational resource profile.
    pub const fn resource_profile_id(&self) -> ResourceProfileIdV1 {
        self.resource_profile_id
    }

    /// Declared relations in caller order.
    pub fn relations(&self) -> &[RelationV1] {
        &self.relations
    }

    fn into_core(self) -> Result<CoreRequestV1, ProtocolErrorV1> {
        let relations = self
            .relations
            .into_iter()
            .map(RelationV1::into_core)
            .collect::<Result<Vec<_>, _>>()?;
        CoreRequestV1::try_new(
            match self.domain_id {
                DomainIdV1::Srgb8NeutralAxis => CoreDomainIdV1::Srgb8NeutralAxis,
            },
            relations,
            match self.resource_profile_id {
                ResourceProfileIdV1::Compile => CoreResourceProfileIdV1::Compile,
            },
        )
        .map_err(map_core_invalid)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RawRequestV1 {
    schema_version: u32,
    domain_id: String,
    resource_profile_id: String,
    relations: Vec<RawRelationV1>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum RawRelationV1 {
    Applicable {
        #[serde(rename = "relationId")]
        relation_id: String,
        #[serde(rename = "occurrenceId")]
        occurrence_id: String,
        criterion: String,
        adjacent: Vec<[u8; 3]>,
    },
    NotApplicable {
        #[serde(rename = "relationId")]
        relation_id: String,
        #[serde(rename = "occurrenceId")]
        occurrence_id: String,
        #[serde(rename = "reasonId")]
        reason_id: String,
    },
}

/// Stable category for malformed JSON without exposing parser prose as ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MalformedEnvelopeClassV1 {
    /// JSON token/grammar error.
    Syntax,
    /// Struct shape, duplicate, unknown or value-domain error.
    Shape,
    /// Unexpected end of input.
    EndOfInput,
    /// Underlying reader failure; unreachable for in-memory bytes but total.
    Io,
}

/// Typed transport failures. No variant is a feasibility terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TransportErrorV1 {
    /// Raw bytes exceed the derived V1 ceiling.
    EnvelopeTooLarge {
        /// Supplied raw bytes.
        #[serde(serialize_with = "serialize_u64_decimal")]
        requested_bytes: u64,
        /// Exact V1 byte ceiling.
        #[serde(serialize_with = "serialize_u64_decimal")]
        limit_bytes: u64,
    },
    /// Raw bytes are not UTF-8 JSON text.
    InvalidUtf8,
    /// JSON syntax or strict schema shape was rejected.
    MalformedEnvelope {
        /// Stable parser category.
        class: MalformedEnvelopeClassV1,
    },
    /// Envelope schema version is unsupported.
    UnsupportedSchemaVersion {
        /// Received schema version.
        received: u32,
    },
    /// Domain key is not registered in V1.
    UnsupportedDomainId {
        /// Exact rejected key.
        received: String,
    },
    /// Resource-profile key is not registered in V1.
    UnsupportedResourceProfileId {
        /// Exact rejected key.
        received: String,
    },
    /// Criterion key is not admitted by Core.
    UnsupportedCriterion {
        /// Exact rejected key.
        received: String,
    },
    /// A NotApplicable declaration had no client reason identity.
    EmptyNotApplicableReason,
    /// Policy-kind key is not registered in V1.
    #[cfg(feature = "wcag22-explicit-selection")]
    UnsupportedPolicyKind {
        /// Exact rejected key.
        received: String,
    },
}

/// Canonical JSON encoding failure. This is not a colour or feasibility result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolEncodingErrorV1 {
    /// A sealed protocol value could not be serialized by the JSON backend.
    SerializationFailed,
}

impl fmt::Display for ProtocolEncodingErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SerializationFailed => formatter.write_str("protocol JSON serialization failed"),
        }
    }
}

impl std::error::Error for ProtocolEncodingErrorV1 {}

/// Exact Core resource dimension projected into protocol errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ResourceDimensionV1 {
    /// Relations before canonical duplicate removal.
    RawRelations,
    /// Adjacent entries before per-relation deduplication.
    RawAdjacentEntries,
    /// Exact projection of Core's operation-scoped opaque UTF-8 payload-byte sum.
    OpaqueUtf8Bytes,
    /// Relations after canonical duplicate removal.
    CanonicalRelations,
    /// Canonical applicable edges.
    ApplicableEdges,
    /// Candidate-edge assessment count.
    LogicalAssessments,
    /// Packed matrix plus partition bytes.
    PackedResultBytes,
}

/// Invalid or contradictory request projected exactly from Core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CoreInvalidRequestV1 {
    /// Empty relation identity.
    EmptyRelationId,
    /// Empty occurrence identity.
    EmptyOccurrenceId,
    /// No declared relations.
    EmptyRelations,
    /// Applicable relation had no adjacency.
    EmptyAdjacentSet {
        /// Opaque relation identity.
        relation_id: String,
    },
    /// One relation identity described contradictory declarations.
    ConflictingRelationId {
        /// Opaque relation identity.
        relation_id: String,
    },
    /// An explicit candidate ID was empty.
    #[cfg(feature = "wcag22-explicit-selection")]
    EmptyCandidateId,
    /// No explicit candidates were declared.
    #[cfg(feature = "wcag22-explicit-selection")]
    EmptyCandidates,
    /// The same explicit candidate ID occurred more than once.
    #[cfg(feature = "wcag22-explicit-selection")]
    DuplicateCandidateId {
        /// Opaque candidate identity.
        candidate_id: String,
    },
    /// Checked cardinality arithmetic overflowed.
    ArithmeticOverflow,
}

/// Atomic WCAG evaluator error projected without collapsing its cause.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Wcag22EvaluationErrorV1 {
    /// A direct atomic input was not exact sRGB8.
    InvalidSrgb8 {
        /// Input field identity.
        field: String,
        /// Core parser diagnostic.
        reason: String,
    },
    /// A NotApplicable declaration had no reason identity.
    EmptyNotApplicableReason,
    /// Finite bounds failed to separate a supported threshold.
    ArtifactInvariantViolation {
        /// Declared occurrence criterion.
        criterion: Wcag22CriterionV1,
        /// Exact foreground bytes.
        foreground: [u8; 3],
        /// Exact background bytes.
        background: [u8; 3],
    },
    /// Atomic evidence and registry identity drifted.
    EvidenceRegistryMismatch {
        /// Core diagnostic.
        message: String,
    },
}

/// Proof-bound evaluator invariant projected from Core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", content = "details", rename_all = "camelCase")]
pub enum EvaluatorInvariantV1 {
    /// Atomic source evaluator failed closed.
    Source(Wcag22EvaluationErrorV1),
    /// Applicable call returned NotEvaluated.
    UnexpectedNotEvaluated,
    /// Returned byte inputs differed from the call.
    InputMismatch,
    /// Returned criterion differed from the declaration.
    CriterionMismatch,
    /// Returned atomic evidence differed from the registry binding.
    EvidenceMismatch,
}

/// Compiler-owned completeness invariant projected from Core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CompilerInvariantV1 {
    /// Derived layout counts disagreed.
    LayoutMismatch,
    /// Observed assessment count differed from exact preflight work.
    AssessmentCardinalityMismatch {
        /// Exact expected count.
        #[serde(serialize_with = "serialize_u64_decimal")]
        expected: u64,
        /// Exact observed count.
        #[serde(serialize_with = "serialize_u64_decimal")]
        observed: u64,
    },
    /// Observed domain count differed from the registered domain.
    CandidateCardinalityMismatch {
        /// Exact expected count.
        #[serde(serialize_with = "serialize_u64_decimal")]
        expected: u64,
        /// Exact observed count.
        #[serde(serialize_with = "serialize_u64_decimal")]
        observed: u64,
    },
    /// Packed storage rejected an addressable cell.
    DecisionStorageRejectedCell,
    /// Packed storage rejected the fixed domain partition.
    DecisionStorageRejectedPartition,
    /// Completed matrix, partition or proof state disagreed.
    CompleteResultMismatch,
}

/// Complete Core failure algebra projected as data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "code",
    content = "details",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum CoreErrorV1 {
    /// Invalid or contradictory request.
    InvalidRequest(CoreInvalidRequestV1),
    /// One exact resource dimension exceeded its profile.
    ResourceLimitExceeded {
        /// Admitting profile.
        profile_id: ResourceProfileIdV1,
        /// Rejected dimension.
        dimension: ResourceDimensionV1,
        /// Requested exact count.
        #[serde(serialize_with = "serialize_u64_decimal")]
        requested: u64,
        /// Admitted exact count.
        #[serde(serialize_with = "serialize_u64_decimal")]
        limit: u64,
    },
    /// Exact packed allocation failed before evaluation.
    AllocationFailed {
        /// Admitting profile.
        profile_id: ResourceProfileIdV1,
        /// Requested allocation bytes.
        #[serde(serialize_with = "serialize_u64_decimal")]
        requested_bytes: u64,
    },
    /// Atomic evaluator violated its proof-bound contract.
    EvaluatorInvariantViolation {
        /// Registered candidate bytes.
        candidate: [u8; 3],
        /// Opaque relation identity.
        relation_id: String,
        /// Exact adjacent bytes.
        adjacent: [u8; 3],
        /// Exact invariant cause.
        violation: EvaluatorInvariantV1,
    },
    /// Compiler completeness state was inconsistent.
    CompilerInvariantViolation(CompilerInvariantV1),
}

/// Failure source at the public protocol boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "source", content = "error", rename_all = "camelCase")]
pub enum ProtocolErrorV1 {
    /// Raw envelope or schema failure.
    Transport(TransportErrorV1),
    /// Exact Core failure.
    Core(CoreErrorV1),
    /// A future non-exhaustive Core variant is not represented by this schema.
    IncompatibleCoreContract,
}

/// Typed canonical domain content identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct DomainDigestV1([u8; 32]);

impl DomainDigestV1 {
    /// Exact SHA-256 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Typed canonical relation-set content identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RelationSetDigestV1([u8; 32]);

impl RelationSetDigestV1 {
    /// Exact SHA-256 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Typed complete-evaluation identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct EvaluationIdV1([u8; 32]);

impl EvaluationIdV1 {
    /// Exact SHA-256 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Atomic WCAG evaluator profile identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wcag22ProfileIdV1 {
    /// Exact WCAG 2.2 final-sRGB8 contrast profile.
    Wcag22Srgb8Contrast,
}

impl Wcag22ProfileIdV1 {
    /// Stable Core-owned key.
    pub fn key(self) -> &'static str {
        CoreWcag22ProfileIdV1::Wcag22Srgb8ContrastV1.key()
    }
}

impl Serialize for Wcag22ProfileIdV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.key())
    }
}

/// Canonical finite numerical artifact identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericalArtifactIdV2 {
    /// WCAG sRGB8 relative-luminance Q55 table.
    Wcag22Srgb8LuminanceQ55,
}

impl NumericalArtifactIdV2 {
    /// Stable Core-owned key.
    pub fn key(self) -> &'static str {
        CoreNumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55V1.key()
    }
}

impl Serialize for NumericalArtifactIdV2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.key())
    }
}

/// Numerical bound-law identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericalErrorBoundIdV2 {
    /// WCAG sRGB8 outward-Q55 threshold law.
    Wcag22Srgb8OutwardQ55,
}

impl NumericalErrorBoundIdV2 {
    /// Stable Core-owned key.
    pub fn key(self) -> &'static str {
        CoreNumericalErrorBoundIdV2::Wcag22Srgb8OutwardQ55V1.key()
    }
}

impl Serialize for NumericalErrorBoundIdV2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.key())
    }
}

/// Replayable numerical proof identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericalProofIdV2 {
    /// Full finite sRGB8 WCAG Q55 proof.
    Wcag22Srgb8FullDomainQ55,
}

impl NumericalProofIdV2 {
    /// Stable Core-owned key.
    pub fn key(self) -> &'static str {
        CoreNumericalProofIdV2::Wcag22Srgb8FullDomainQ55V1.key()
    }
}

impl Serialize for NumericalProofIdV2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.key())
    }
}

/// Sealed, fully projected complete-enumeration proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluationProofV1 {
    evaluation_id: EvaluationIdV1,
    resource_profile_id: ResourceProfileIdV1,
    domain_id: DomainIdV1,
    domain_digest: DomainDigestV1,
    #[serde(serialize_with = "serialize_u64_decimal")]
    domain_count: u64,
    domain_first: [u8; 3],
    domain_last: [u8; 3],
    relation_set_digest: RelationSetDigestV1,
    #[serde(serialize_with = "serialize_u64_decimal")]
    canonical_relations: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    applicable_relations: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    not_applicable_relations: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    applicable_edges: u64,
    #[serde(serialize_with = "serialize_u64_decimal")]
    logical_assessments: u64,
    matrix_digest: [u8; 32],
    partition: [u8; 32],
    wcag22_profile_id: Wcag22ProfileIdV1,
    artifact_id: NumericalArtifactIdV2,
    bound_id: NumericalErrorBoundIdV2,
    proof_id: NumericalProofIdV2,
    proof_sha256: [u8; 32],
}

impl EvaluationProofV1 {
    /// Semantic evaluation identity.
    pub const fn evaluation_id(&self) -> EvaluationIdV1 {
        self.evaluation_id
    }

    /// Resource profile that admitted the computation.
    pub const fn resource_profile_id(&self) -> ResourceProfileIdV1 {
        self.resource_profile_id
    }

    /// Registered domain identity.
    pub const fn domain_id(&self) -> DomainIdV1 {
        self.domain_id
    }

    /// Canonical domain digest.
    pub const fn domain_digest(&self) -> DomainDigestV1 {
        self.domain_digest
    }

    /// Exact registered-domain cardinality.
    pub const fn domain_count(&self) -> u64 {
        self.domain_count
    }

    /// First domain byte triple.
    pub const fn domain_first(&self) -> [u8; 3] {
        self.domain_first
    }

    /// Last domain byte triple.
    pub const fn domain_last(&self) -> [u8; 3] {
        self.domain_last
    }

    /// Canonical relation-set digest.
    pub const fn relation_set_digest(&self) -> RelationSetDigestV1 {
        self.relation_set_digest
    }

    /// Canonical relation count.
    pub const fn canonical_relations(&self) -> u64 {
        self.canonical_relations
    }

    /// Canonical applicable-relation count.
    pub const fn applicable_relations(&self) -> u64 {
        self.applicable_relations
    }

    /// Canonical NotApplicable relation count.
    pub const fn not_applicable_relations(&self) -> u64 {
        self.not_applicable_relations
    }

    /// Flattened canonical edge count.
    pub const fn applicable_edges(&self) -> u64 {
        self.applicable_edges
    }

    /// Exact complete assessment count.
    pub const fn logical_assessments(&self) -> u64 {
        self.logical_assessments
    }

    /// SHA-256 of the packed failure matrix.
    pub const fn matrix_digest(&self) -> &[u8; 32] {
        &self.matrix_digest
    }

    /// Exact 256-bit feasible partition, candidate-index LSB0.
    pub const fn partition(&self) -> &[u8; 32] {
        &self.partition
    }

    /// Atomic WCAG profile.
    pub const fn wcag22_profile_id(&self) -> Wcag22ProfileIdV1 {
        self.wcag22_profile_id
    }

    /// Atomic finite artifact.
    pub const fn artifact_id(&self) -> NumericalArtifactIdV2 {
        self.artifact_id
    }

    /// Atomic numerical bound law.
    pub const fn bound_id(&self) -> NumericalErrorBoundIdV2 {
        self.bound_id
    }

    /// Atomic complete-domain proof identity.
    pub const fn proof_id(&self) -> NumericalProofIdV2 {
        self.proof_id
    }

    /// Exact proof-file SHA-256 bytes.
    pub const fn proof_sha256(&self) -> &[u8; 32] {
        &self.proof_sha256
    }
}

/// Packed evaluated payload shared by Feasible and Infeasible terminals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluatedV1 {
    domain: Vec<[u8; 3]>,
    relations: Vec<RelationV1>,
    failure_matrix: Vec<u8>,
    proof: EvaluationProofV1,
}

impl EvaluatedV1 {
    /// Exact registered domain in Core order, transported once.
    pub fn domain(&self) -> &[[u8; 3]] {
        &self.domain
    }

    /// Canonical declarations, transported once.
    pub fn relations(&self) -> &[RelationV1] {
        &self.relations
    }

    /// Candidate-major LSB0 packed failure matrix.
    pub fn failure_matrix(&self) -> &[u8] {
        &self.failure_matrix
    }

    /// Sealed complete-evaluation proof.
    pub const fn proof(&self) -> &EvaluationProofV1 {
        &self.proof
    }
}

/// Declaration-only terminal payload with no fabricated numerical proof.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotEvaluatedV1 {
    domain_id: DomainIdV1,
    domain_digest: DomainDigestV1,
    relation_set_digest: RelationSetDigestV1,
    resource_profile_id: ResourceProfileIdV1,
    relations: Vec<RelationV1>,
}

impl NotEvaluatedV1 {
    /// Registered domain identity.
    pub const fn domain_id(&self) -> DomainIdV1 {
        self.domain_id
    }

    /// Canonical domain digest.
    pub const fn domain_digest(&self) -> DomainDigestV1 {
        self.domain_digest
    }

    /// Canonical declaration-set digest.
    pub const fn relation_set_digest(&self) -> RelationSetDigestV1 {
        self.relation_set_digest
    }

    /// Admitting operational profile.
    pub const fn resource_profile_id(&self) -> ResourceProfileIdV1 {
        self.resource_profile_id
    }

    /// Canonical declarations, transported once.
    pub fn relations(&self) -> &[RelationV1] {
        &self.relations
    }
}

/// Successful feasibility terminal. Failure is represented only by
/// [`ProtocolOutcomeV1::Failure`], never by a fourth variant here.
///
/// Evaluated terminal payloads are variant-sealed: an external adapter may
/// inspect or clone the evidence, but cannot rewrap genuine Core evidence with
/// the opposite terminal.
///
/// ```compile_fail
/// use labcolors_protocol::{
///     EvaluatedV1, FeasibilityV1, NotEvaluatedV1, ProtocolOutcomeV1,
/// };
///
/// fn rewrap_feasible(value: EvaluatedV1) -> FeasibilityV1 {
///     FeasibilityV1::Feasible(value)
/// }
///
/// fn rewrap_infeasible(value: EvaluatedV1) -> FeasibilityV1 {
///     FeasibilityV1::Infeasible(value)
/// }
///
/// fn rewrap_not_evaluated(value: NotEvaluatedV1) -> FeasibilityV1 {
///     FeasibilityV1::NotEvaluated(value)
/// }
///
/// fn flip_terminal(outcome: ProtocolOutcomeV1) -> ProtocolOutcomeV1 {
///     let evaluated = outcome
///         .feasibility()
///         .and_then(FeasibilityV1::evaluated)
///         .expect("evaluated Core outcome")
///         .clone();
///     ProtocolOutcomeV1::Success {
///         feasibility: FeasibilityV1::Infeasible(evaluated),
///     }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", content = "result", rename_all = "camelCase")]
#[non_exhaustive]
pub enum FeasibilityV1 {
    /// Complete evaluation found a non-empty feasible partition.
    #[non_exhaustive]
    Feasible(EvaluatedV1),
    /// Complete evaluation proved the registered partition empty.
    #[non_exhaustive]
    Infeasible(EvaluatedV1),
    /// Every canonical relation was client-declared NotApplicable.
    #[non_exhaustive]
    NotEvaluated(NotEvaluatedV1),
}

impl FeasibilityV1 {
    /// Whether complete evaluation found at least one feasible candidate.
    pub const fn is_feasible(&self) -> bool {
        matches!(self, Self::Feasible(..))
    }

    /// Whether complete evaluation proved the feasible partition empty.
    pub const fn is_infeasible(&self) -> bool {
        matches!(self, Self::Infeasible(..))
    }

    /// Whether every canonical relation was declared NotApplicable.
    pub const fn is_not_evaluated(&self) -> bool {
        matches!(self, Self::NotEvaluated(..))
    }

    /// Borrow an evaluated payload from either evaluated terminal.
    pub const fn evaluated(&self) -> Option<&EvaluatedV1> {
        match self {
            Self::Feasible(value) | Self::Infeasible(value) => Some(value),
            Self::NotEvaluated(_) => None,
        }
    }

    /// Borrow the declaration-only payload.
    pub const fn not_evaluated(&self) -> Option<&NotEvaluatedV1> {
        match self {
            Self::NotEvaluated(value) => Some(value),
            Self::Feasible(..) | Self::Infeasible(..) => None,
        }
    }
}

/// Total public protocol result: success with one feasibility terminal, or
/// failure with typed transport/Core error data.
///
/// The type deliberately does not implement `Deserialize`: proof-bearing
/// terminals can be obtained only by evaluating Core, not by parsing forged
/// JSON.
///
/// ```compile_fail
/// let _: labcolors_protocol::ProtocolOutcomeV1 =
///     serde_json::from_str(r#"{"schemaVersion":1,"outcome":"success"}"#).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
// The successful terminal keeps bounded owned-buffer headers inline, avoiding
// one heap allocation, OOM edge and indirection on every compiler call. The
// layout test below admits exactly the largest payload plus its required tag
// slot, with no discretionary stack headroom; this is not wire ABI.
#[allow(clippy::large_enum_variant)]
pub enum ProtocolOutcomeV1 {
    /// Successful complete feasibility operation.
    Success {
        /// Exact feasibility terminal.
        feasibility: FeasibilityV1,
    },
    /// Failed operation with no partial feasibility terminal.
    Failure {
        /// Typed failure data.
        error: ProtocolErrorV1,
    },
}

impl ProtocolOutcomeV1 {
    /// Borrow the successful feasibility terminal.
    pub const fn feasibility(&self) -> Option<&FeasibilityV1> {
        match self {
            Self::Success { feasibility } => Some(feasibility),
            Self::Failure { .. } => None,
        }
    }

    /// Borrow typed failure data.
    pub const fn error(&self) -> Option<&ProtocolErrorV1> {
        match self {
            Self::Success { .. } => None,
            Self::Failure { error } => Some(error),
        }
    }

    fn failure(error: ProtocolErrorV1) -> Self {
        Self::Failure { error }
    }
}

impl Serialize for ProtocolOutcomeV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Success { feasibility } => {
                let mut state = serializer.serialize_struct("ProtocolOutcomeV1", 3)?;
                state.serialize_field("schemaVersion", &SCHEMA_VERSION_V1)?;
                state.serialize_field("outcome", "success")?;
                state.serialize_field("feasibility", feasibility)?;
                state.end()
            }
            Self::Failure { error } => {
                let mut state = serializer.serialize_struct("ProtocolOutcomeV1", 3)?;
                state.serialize_field("schemaVersion", &SCHEMA_VERSION_V1)?;
                state.serialize_field("outcome", "failure")?;
                state.serialize_field("error", error)?;
                state.end()
            }
        }
    }
}

pub(crate) fn serialize_u64_decimal<S>(value: &u64, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&value.to_string())
}

pub(crate) fn usize_as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

pub(crate) fn core_invalid(error: CoreInvalidRequestV1) -> ProtocolErrorV1 {
    ProtocolErrorV1::Core(CoreErrorV1::InvalidRequest(error))
}

fn validate_relation_id(value: &str) -> Result<(), ProtocolErrorV1> {
    if value.is_empty() {
        Err(core_invalid(CoreInvalidRequestV1::EmptyRelationId))
    } else {
        Ok(())
    }
}

fn validate_occurrence_id(value: &str) -> Result<(), ProtocolErrorV1> {
    if value.is_empty() {
        Err(core_invalid(CoreInvalidRequestV1::EmptyOccurrenceId))
    } else {
        Ok(())
    }
}

/// Encode one validated request as deterministic compact UTF-8 JSON bytes.
pub fn encode_request_v1(request: &RequestV1) -> Result<Vec<u8>, ProtocolEncodingErrorV1> {
    serde_json::to_vec(request).map_err(|_| ProtocolEncodingErrorV1::SerializationFailed)
}

/// Encode one sealed outcome as deterministic compact UTF-8 JSON bytes.
pub fn encode_outcome_v1(outcome: &ProtocolOutcomeV1) -> Result<Vec<u8>, ProtocolEncodingErrorV1> {
    serde_json::to_vec(outcome).map_err(|_| ProtocolEncodingErrorV1::SerializationFailed)
}

/// Construct the canonical typed failure used by host-side byte preflight.
///
/// Adapters call this scalar helper only after proving their raw byte length is
/// greater than [`MAX_ENVELOPE_BYTES_V1`]. The authoritative Rust byte path
/// uses the same constructor.
pub fn envelope_too_large_outcome_v1(requested_bytes: u64) -> ProtocolOutcomeV1 {
    ProtocolOutcomeV1::failure(ProtocolErrorV1::Transport(
        TransportErrorV1::EnvelopeTooLarge {
            requested_bytes,
            limit_bytes: MAX_ENVELOPE_BYTES_V1,
        },
    ))
}

/// Evaluate one exact raw UTF-8 JSON envelope through the sole Core compiler.
///
/// Raw length is rejected before UTF-8 validation, JSON parsing, nested
/// allocation or Core work. Every public-input and Core error becomes a typed
/// [`ProtocolOutcomeV1::Failure`]; no `Result`, panic or fallback escapes.
pub fn evaluate_wcag22_feasibility_v1(raw: &[u8]) -> ProtocolOutcomeV1 {
    evaluate_with_decoder(raw, decode_request_v1)
}

fn evaluate_with_decoder<F>(raw: &[u8], decoder: F) -> ProtocolOutcomeV1
where
    F: FnOnce(&str) -> Result<CoreRequestV1, ProtocolErrorV1>,
{
    let requested_bytes = usize_as_u64(raw.len());
    if requested_bytes > MAX_ENVELOPE_BYTES_V1 {
        return envelope_too_large_outcome_v1(requested_bytes);
    }
    let text = match core::str::from_utf8(raw) {
        Ok(text) => text,
        Err(_) => {
            return ProtocolOutcomeV1::failure(ProtocolErrorV1::Transport(
                TransportErrorV1::InvalidUtf8,
            ));
        }
    };
    let request = match decoder(text) {
        Ok(request) => request,
        Err(error) => return ProtocolOutcomeV1::failure(error),
    };
    match core_feasibility::evaluate(request) {
        Ok(feasibility) => match project_feasibility(&feasibility) {
            Ok(feasibility) => ProtocolOutcomeV1::Success { feasibility },
            Err(error) => ProtocolOutcomeV1::failure(error),
        },
        Err(error) => ProtocolOutcomeV1::failure(project_core_error(error)),
    }
}

fn decode_request_v1(text: &str) -> Result<CoreRequestV1, ProtocolErrorV1> {
    let raw: RawRequestV1 = serde_json::from_str(text).map_err(|error| {
        let class = match error.classify() {
            serde_json::error::Category::Io => MalformedEnvelopeClassV1::Io,
            serde_json::error::Category::Syntax => MalformedEnvelopeClassV1::Syntax,
            serde_json::error::Category::Data => MalformedEnvelopeClassV1::Shape,
            serde_json::error::Category::Eof => MalformedEnvelopeClassV1::EndOfInput,
        };
        ProtocolErrorV1::Transport(TransportErrorV1::MalformedEnvelope { class })
    })?;
    if raw.schema_version != SCHEMA_VERSION_V1 {
        return Err(ProtocolErrorV1::Transport(
            TransportErrorV1::UnsupportedSchemaVersion {
                received: raw.schema_version,
            },
        ));
    }
    let domain_id = match DomainIdV1::parse(&raw.domain_id) {
        Some(domain_id) => domain_id,
        None => {
            return Err(ProtocolErrorV1::Transport(
                TransportErrorV1::UnsupportedDomainId {
                    received: raw.domain_id,
                },
            ));
        }
    };
    let resource_profile_id = match ResourceProfileIdV1::parse(&raw.resource_profile_id) {
        Some(resource_profile_id) => resource_profile_id,
        None => {
            return Err(ProtocolErrorV1::Transport(
                TransportErrorV1::UnsupportedResourceProfileId {
                    received: raw.resource_profile_id,
                },
            ));
        }
    };
    let relations = raw
        .relations
        .into_iter()
        .map(|relation| match relation {
            RawRelationV1::Applicable {
                relation_id,
                occurrence_id,
                criterion,
                adjacent,
            } => {
                let criterion = Wcag22CriterionV1::parse(&criterion).ok_or({
                    ProtocolErrorV1::Transport(TransportErrorV1::UnsupportedCriterion {
                        received: criterion,
                    })
                })?;
                RelationV1::applicable(relation_id, occurrence_id, criterion, adjacent)
            }
            RawRelationV1::NotApplicable {
                relation_id,
                occurrence_id,
                reason_id,
            } => RelationV1::not_applicable(relation_id, occurrence_id, reason_id),
        })
        .collect::<Result<Vec<_>, _>>()?;
    RequestV1::try_new(domain_id, relations, resource_profile_id)?.into_core()
}

fn project_feasibility(value: &CoreFeasibilityV1) -> Result<FeasibilityV1, ProtocolErrorV1> {
    if let Some(evaluated) = value.evaluated() {
        let evaluated = project_evaluated(evaluated)?;
        if value.is_feasible() {
            return Ok(FeasibilityV1::Feasible(evaluated));
        }
        if value.is_infeasible() {
            return Ok(FeasibilityV1::Infeasible(evaluated));
        }
        return Err(ProtocolErrorV1::IncompatibleCoreContract);
    }
    if let Some(not_evaluated) = value.not_evaluated() {
        if value.is_not_evaluated() {
            return Ok(FeasibilityV1::NotEvaluated(project_not_evaluated(
                not_evaluated,
            )?));
        }
    }
    Err(ProtocolErrorV1::IncompatibleCoreContract)
}

fn project_evaluated(value: &CoreEvaluatedV1) -> Result<EvaluatedV1, ProtocolErrorV1> {
    let domain_id = project_domain_id(value.domain_id())?;
    let domain = value.domain_id().candidates().map(Srgb8::bytes).collect();
    let relations = project_relations(value.relations())?;
    Ok(EvaluatedV1 {
        domain,
        relations,
        failure_matrix: value.failure_matrix().to_vec(),
        proof: project_proof(value.proof(), domain_id)?,
    })
}

fn project_not_evaluated(value: &CoreNotEvaluatedV1) -> Result<NotEvaluatedV1, ProtocolErrorV1> {
    Ok(NotEvaluatedV1 {
        domain_id: project_domain_id(value.domain_id())?,
        domain_digest: DomainDigestV1(*value.domain_digest().as_bytes()),
        relation_set_digest: RelationSetDigestV1(*value.relation_set_digest().as_bytes()),
        resource_profile_id: project_resource_profile_id(value.resource_profile_id())?,
        relations: project_relations(value.relations())?,
    })
}

pub(crate) fn project_relations(
    values: &[CoreRelationV1],
) -> Result<Vec<RelationV1>, ProtocolErrorV1> {
    values.iter().map(project_relation).collect()
}

fn project_relation(value: &CoreRelationV1) -> Result<RelationV1, ProtocolErrorV1> {
    if let Some((criterion, adjacent)) = value.as_applicable() {
        return RelationV1::applicable(
            value.relation_id().as_str(),
            value.occurrence_id().as_str(),
            project_criterion(criterion)?,
            adjacent.iter().copied().map(Srgb8::bytes).collect(),
        );
    }
    if let Some(declaration) = value.as_not_applicable() {
        return RelationV1::not_applicable(
            value.relation_id().as_str(),
            value.occurrence_id().as_str(),
            declaration.reason_id(),
        );
    }
    Err(ProtocolErrorV1::IncompatibleCoreContract)
}

fn project_proof(
    value: &core_feasibility::EvaluationProofV1,
    domain_id: DomainIdV1,
) -> Result<EvaluationProofV1, ProtocolErrorV1> {
    Ok(EvaluationProofV1 {
        evaluation_id: EvaluationIdV1(*value.evaluation_id().as_bytes()),
        resource_profile_id: project_resource_profile_id(value.resource_profile_id())?,
        domain_id,
        domain_digest: DomainDigestV1(*value.domain_digest().as_bytes()),
        domain_count: value.domain_count(),
        domain_first: value.domain_first().bytes(),
        domain_last: value.domain_last().bytes(),
        relation_set_digest: RelationSetDigestV1(*value.relation_set_digest().as_bytes()),
        canonical_relations: value.canonical_relations(),
        applicable_relations: value.applicable_relations(),
        not_applicable_relations: value.not_applicable_relations(),
        applicable_edges: value.applicable_edges(),
        logical_assessments: value.logical_assessments(),
        matrix_digest: *value.matrix_digest(),
        partition: *value.partition(),
        wcag22_profile_id: project_wcag22_profile_id(value.profile_id())?,
        artifact_id: project_artifact_id(value.artifact_id())?,
        bound_id: project_bound_id(value.bound_id())?,
        proof_id: project_proof_id(value.proof_id())?,
        proof_sha256: *value.proof_sha256(),
    })
}

fn project_domain_id(value: CoreDomainIdV1) -> Result<DomainIdV1, ProtocolErrorV1> {
    match value {
        CoreDomainIdV1::Srgb8NeutralAxis => Ok(DomainIdV1::Srgb8NeutralAxis),
        _ => Err(ProtocolErrorV1::IncompatibleCoreContract),
    }
}

pub(crate) fn project_resource_profile_id(
    value: CoreResourceProfileIdV1,
) -> Result<ResourceProfileIdV1, ProtocolErrorV1> {
    match value {
        CoreResourceProfileIdV1::Compile => Ok(ResourceProfileIdV1::Compile),
        _ => Err(ProtocolErrorV1::IncompatibleCoreContract),
    }
}

pub(crate) fn project_criterion(
    value: CoreWcag22CriterionV1,
) -> Result<Wcag22CriterionV1, ProtocolErrorV1> {
    match value {
        CoreWcag22CriterionV1::Sc143TextDefault => Ok(Wcag22CriterionV1::Sc143TextDefault),
        CoreWcag22CriterionV1::Sc143TextLargeScale => Ok(Wcag22CriterionV1::Sc143TextLargeScale),
        CoreWcag22CriterionV1::Sc1411UiComponentOrState => {
            Ok(Wcag22CriterionV1::Sc1411UiComponentOrState)
        }
        CoreWcag22CriterionV1::Sc1411GraphicalObject => {
            Ok(Wcag22CriterionV1::Sc1411GraphicalObject)
        }
        _ => Err(ProtocolErrorV1::IncompatibleCoreContract),
    }
}

pub(crate) fn project_wcag22_profile_id(
    value: CoreWcag22ProfileIdV1,
) -> Result<Wcag22ProfileIdV1, ProtocolErrorV1> {
    match value {
        CoreWcag22ProfileIdV1::Wcag22Srgb8ContrastV1 => Ok(Wcag22ProfileIdV1::Wcag22Srgb8Contrast),
        _ => Err(ProtocolErrorV1::IncompatibleCoreContract),
    }
}

pub(crate) fn project_artifact_id(
    value: CoreNumericalArtifactIdV2,
) -> Result<NumericalArtifactIdV2, ProtocolErrorV1> {
    match value {
        CoreNumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55V1 => {
            Ok(NumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55)
        }
        _ => Err(ProtocolErrorV1::IncompatibleCoreContract),
    }
}

pub(crate) fn project_bound_id(
    value: CoreNumericalErrorBoundIdV2,
) -> Result<NumericalErrorBoundIdV2, ProtocolErrorV1> {
    match value {
        CoreNumericalErrorBoundIdV2::Wcag22Srgb8OutwardQ55V1 => {
            Ok(NumericalErrorBoundIdV2::Wcag22Srgb8OutwardQ55)
        }
        _ => Err(ProtocolErrorV1::IncompatibleCoreContract),
    }
}

pub(crate) fn project_proof_id(
    value: CoreNumericalProofIdV2,
) -> Result<NumericalProofIdV2, ProtocolErrorV1> {
    match value {
        CoreNumericalProofIdV2::Wcag22Srgb8FullDomainQ55V1 => {
            Ok(NumericalProofIdV2::Wcag22Srgb8FullDomainQ55)
        }
        _ => Err(ProtocolErrorV1::IncompatibleCoreContract),
    }
}

fn map_core_invalid(error: CoreInvalidRequestSourceV1) -> ProtocolErrorV1 {
    match project_core_invalid(error) {
        Ok(error) => core_invalid(error),
        Err(()) => ProtocolErrorV1::IncompatibleCoreContract,
    }
}

fn project_core_invalid(error: CoreInvalidRequestSourceV1) -> Result<CoreInvalidRequestV1, ()> {
    match error {
        CoreInvalidRequestSourceV1::EmptyRelationId => Ok(CoreInvalidRequestV1::EmptyRelationId),
        CoreInvalidRequestSourceV1::EmptyOccurrenceId => {
            Ok(CoreInvalidRequestV1::EmptyOccurrenceId)
        }
        CoreInvalidRequestSourceV1::EmptyRelations => Ok(CoreInvalidRequestV1::EmptyRelations),
        CoreInvalidRequestSourceV1::EmptyAdjacentSet { relation_id } => {
            Ok(CoreInvalidRequestV1::EmptyAdjacentSet {
                relation_id: relation_id.as_str().to_string(),
            })
        }
        CoreInvalidRequestSourceV1::ConflictingRelationId { relation_id } => {
            Ok(CoreInvalidRequestV1::ConflictingRelationId {
                relation_id: relation_id.as_str().to_string(),
            })
        }
        CoreInvalidRequestSourceV1::ArithmeticOverflow => {
            Ok(CoreInvalidRequestV1::ArithmeticOverflow)
        }
        #[cfg(feature = "wcag22-explicit-selection")]
        CoreInvalidRequestSourceV1::EmptyCandidateId => Ok(CoreInvalidRequestV1::EmptyCandidateId),
        #[cfg(feature = "wcag22-explicit-selection")]
        CoreInvalidRequestSourceV1::EmptyCandidates => Ok(CoreInvalidRequestV1::EmptyCandidates),
        #[cfg(feature = "wcag22-explicit-selection")]
        CoreInvalidRequestSourceV1::DuplicateCandidateId { candidate_id } => {
            Ok(CoreInvalidRequestV1::DuplicateCandidateId {
                candidate_id: candidate_id.as_str().to_string(),
            })
        }
        _ => Err(()),
    }
}

pub(crate) fn project_core_error(error: CoreFeasibilityErrorV1) -> ProtocolErrorV1 {
    let projected = match error {
        CoreFeasibilityErrorV1::InvalidRequest(error) => {
            project_core_invalid(error).map(CoreErrorV1::InvalidRequest)
        }
        CoreFeasibilityErrorV1::ResourceLimitExceeded {
            profile_id,
            dimension,
            requested,
            limit,
        } => Ok(CoreErrorV1::ResourceLimitExceeded {
            profile_id: match project_resource_profile_id(profile_id) {
                Ok(value) => value,
                Err(_) => return ProtocolErrorV1::IncompatibleCoreContract,
            },
            dimension: match project_resource_dimension(dimension) {
                Ok(value) => value,
                Err(()) => return ProtocolErrorV1::IncompatibleCoreContract,
            },
            requested,
            limit,
        }),
        CoreFeasibilityErrorV1::AllocationFailed {
            profile_id,
            requested_bytes,
        } => Ok(CoreErrorV1::AllocationFailed {
            profile_id: match project_resource_profile_id(profile_id) {
                Ok(value) => value,
                Err(_) => return ProtocolErrorV1::IncompatibleCoreContract,
            },
            requested_bytes,
        }),
        CoreFeasibilityErrorV1::EvaluatorInvariantViolation {
            candidate,
            relation_id,
            adjacent,
            violation,
        } => Ok(CoreErrorV1::EvaluatorInvariantViolation {
            candidate: candidate.bytes(),
            relation_id: relation_id.as_str().to_string(),
            adjacent: adjacent.bytes(),
            violation: match project_evaluator_invariant(violation) {
                Ok(value) => value,
                Err(()) => return ProtocolErrorV1::IncompatibleCoreContract,
            },
        }),
        CoreFeasibilityErrorV1::CompilerInvariantViolation(violation) => {
            project_compiler_invariant(violation).map(CoreErrorV1::CompilerInvariantViolation)
        }
        _ => Err(()),
    };
    match projected {
        Ok(error) => ProtocolErrorV1::Core(error),
        Err(()) => ProtocolErrorV1::IncompatibleCoreContract,
    }
}

fn project_resource_dimension(value: CoreResourceDimensionV1) -> Result<ResourceDimensionV1, ()> {
    match value {
        CoreResourceDimensionV1::RawRelations => Ok(ResourceDimensionV1::RawRelations),
        CoreResourceDimensionV1::RawAdjacentEntries => Ok(ResourceDimensionV1::RawAdjacentEntries),
        CoreResourceDimensionV1::OpaqueUtf8Bytes => Ok(ResourceDimensionV1::OpaqueUtf8Bytes),
        CoreResourceDimensionV1::CanonicalRelations => Ok(ResourceDimensionV1::CanonicalRelations),
        CoreResourceDimensionV1::ApplicableEdges => Ok(ResourceDimensionV1::ApplicableEdges),
        CoreResourceDimensionV1::LogicalAssessments => Ok(ResourceDimensionV1::LogicalAssessments),
        CoreResourceDimensionV1::PackedResultBytes => Ok(ResourceDimensionV1::PackedResultBytes),
        _ => Err(()),
    }
}

fn project_evaluator_invariant(
    value: CoreEvaluatorInvariantV1,
) -> Result<EvaluatorInvariantV1, ()> {
    match value {
        CoreEvaluatorInvariantV1::Source(error) => {
            project_wcag22_error(error).map(EvaluatorInvariantV1::Source)
        }
        CoreEvaluatorInvariantV1::UnexpectedNotEvaluated => {
            Ok(EvaluatorInvariantV1::UnexpectedNotEvaluated)
        }
        CoreEvaluatorInvariantV1::InputMismatch => Ok(EvaluatorInvariantV1::InputMismatch),
        CoreEvaluatorInvariantV1::CriterionMismatch => Ok(EvaluatorInvariantV1::CriterionMismatch),
        CoreEvaluatorInvariantV1::EvidenceMismatch => Ok(EvaluatorInvariantV1::EvidenceMismatch),
        _ => Err(()),
    }
}

fn project_wcag22_error(value: CoreWcag22EvaluationErrorV1) -> Result<Wcag22EvaluationErrorV1, ()> {
    match value {
        CoreWcag22EvaluationErrorV1::InvalidSrgb8 { field, reason } => {
            Ok(Wcag22EvaluationErrorV1::InvalidSrgb8 {
                field: field.to_string(),
                reason,
            })
        }
        CoreWcag22EvaluationErrorV1::EmptyNotApplicableReason => {
            Ok(Wcag22EvaluationErrorV1::EmptyNotApplicableReason)
        }
        CoreWcag22EvaluationErrorV1::ArtifactInvariantViolation {
            criterion,
            foreground,
            background,
        } => Ok(Wcag22EvaluationErrorV1::ArtifactInvariantViolation {
            criterion: match project_criterion(criterion) {
                Ok(value) => value,
                Err(_) => return Err(()),
            },
            foreground,
            background,
        }),
        CoreWcag22EvaluationErrorV1::EvidenceRegistryMismatch(message) => {
            Ok(Wcag22EvaluationErrorV1::EvidenceRegistryMismatch { message })
        }
        _ => Err(()),
    }
}

fn project_compiler_invariant(value: CoreCompilerInvariantV1) -> Result<CompilerInvariantV1, ()> {
    match value {
        CoreCompilerInvariantV1::LayoutMismatch => Ok(CompilerInvariantV1::LayoutMismatch),
        CoreCompilerInvariantV1::AssessmentCardinalityMismatch { expected, observed } => {
            Ok(CompilerInvariantV1::AssessmentCardinalityMismatch { expected, observed })
        }
        CoreCompilerInvariantV1::CandidateCardinalityMismatch { expected, observed } => {
            Ok(CompilerInvariantV1::CandidateCardinalityMismatch { expected, observed })
        }
        CoreCompilerInvariantV1::DecisionStorageRejectedCell => {
            Ok(CompilerInvariantV1::DecisionStorageRejectedCell)
        }
        CoreCompilerInvariantV1::DecisionStorageRejectedPartition => {
            Ok(CompilerInvariantV1::DecisionStorageRejectedPartition)
        }
        CoreCompilerInvariantV1::CompleteResultMismatch => {
            Ok(CompilerInvariantV1::CompleteResultMismatch)
        }
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn one_core_request() -> CoreRequestV1 {
        let relation = CoreRelationV1::applicable(
            CoreRelationId::try_new("relation").unwrap(),
            CoreOccurrenceId::try_new("occurrence").unwrap(),
            CoreWcag22CriterionV1::Sc143TextDefault,
            vec![Srgb8::new([0x76; 3])],
        )
        .unwrap();
        CoreRequestV1::try_new(
            CoreDomainIdV1::Srgb8NeutralAxis,
            vec![relation],
            CoreResourceProfileIdV1::Compile,
        )
        .unwrap()
    }

    #[test]
    fn raw_byte_preflight_calls_no_decoder_above_limit_and_one_at_limit() {
        let calls = Cell::new(0_u32);
        let over = vec![b' '; usize::try_from(MAX_ENVELOPE_BYTES_V1 + 1).unwrap()];
        let outcome = evaluate_with_decoder(&over, |_| {
            calls.set(calls.get() + 1);
            Ok(one_core_request())
        });
        assert_eq!(calls.get(), 0);
        assert_eq!(
            outcome,
            envelope_too_large_outcome_v1(MAX_ENVELOPE_BYTES_V1 + 1)
        );

        let at_limit = vec![b' '; usize::try_from(MAX_ENVELOPE_BYTES_V1).unwrap()];
        let outcome = evaluate_with_decoder(&at_limit, |_| {
            calls.set(calls.get() + 1);
            Ok(one_core_request())
        });
        assert_eq!(calls.get(), 1);
        assert!(matches!(
            outcome,
            ProtocolOutcomeV1::Success {
                feasibility: FeasibilityV1::Feasible(_)
            }
        ));
    }

    #[test]
    fn inline_outcome_header_has_no_discretionary_stack_headroom() {
        let largest_terminal_payload =
            core::mem::size_of::<EvaluatedV1>().max(core::mem::size_of::<NotEvaluatedV1>());
        let required_terminal_tag_slot = core::mem::align_of::<FeasibilityV1>();
        let exact_terminal_header = largest_terminal_payload + required_terminal_tag_slot;

        assert_eq!(
            core::mem::size_of::<FeasibilityV1>(),
            exact_terminal_header,
            "terminal layout gained bytes beyond its largest payload and aligned tag slot"
        );
        assert!(
            core::mem::size_of::<ProtocolErrorV1>() <= exact_terminal_header,
            "failure payload became the largest outcome payload"
        );
        assert_eq!(
            core::mem::size_of::<ProtocolOutcomeV1>(),
            exact_terminal_header,
            "outer outcome tag introduced an additional stack slot"
        );
    }

    #[test]
    fn every_current_core_error_branch_has_a_typed_projection() {
        let relation_id = CoreRelationId::try_new("relation").unwrap();
        let invalid = [
            CoreInvalidRequestSourceV1::EmptyRelationId,
            CoreInvalidRequestSourceV1::EmptyOccurrenceId,
            CoreInvalidRequestSourceV1::EmptyRelations,
            CoreInvalidRequestSourceV1::EmptyAdjacentSet {
                relation_id: relation_id.clone(),
            },
            CoreInvalidRequestSourceV1::ConflictingRelationId {
                relation_id: relation_id.clone(),
            },
            CoreInvalidRequestSourceV1::ArithmeticOverflow,
        ];
        for error in invalid {
            assert!(matches!(
                project_core_error(CoreFeasibilityErrorV1::InvalidRequest(error)),
                ProtocolErrorV1::Core(CoreErrorV1::InvalidRequest(_))
            ));
        }

        let dimensions = [
            (
                CoreResourceDimensionV1::RawRelations,
                ResourceDimensionV1::RawRelations,
            ),
            (
                CoreResourceDimensionV1::RawAdjacentEntries,
                ResourceDimensionV1::RawAdjacentEntries,
            ),
            (
                CoreResourceDimensionV1::OpaqueUtf8Bytes,
                ResourceDimensionV1::OpaqueUtf8Bytes,
            ),
            (
                CoreResourceDimensionV1::CanonicalRelations,
                ResourceDimensionV1::CanonicalRelations,
            ),
            (
                CoreResourceDimensionV1::ApplicableEdges,
                ResourceDimensionV1::ApplicableEdges,
            ),
            (
                CoreResourceDimensionV1::LogicalAssessments,
                ResourceDimensionV1::LogicalAssessments,
            ),
            (
                CoreResourceDimensionV1::PackedResultBytes,
                ResourceDimensionV1::PackedResultBytes,
            ),
        ];
        for (core_dimension, protocol_dimension) in dimensions {
            assert_eq!(
                project_core_error(CoreFeasibilityErrorV1::ResourceLimitExceeded {
                    profile_id: CoreResourceProfileIdV1::Compile,
                    dimension: core_dimension,
                    requested: 2,
                    limit: 1,
                }),
                ProtocolErrorV1::Core(CoreErrorV1::ResourceLimitExceeded {
                    profile_id: ResourceProfileIdV1::Compile,
                    dimension: protocol_dimension,
                    requested: 2,
                    limit: 1,
                })
            );
        }
        assert!(matches!(
            project_core_error(CoreFeasibilityErrorV1::AllocationFailed {
                profile_id: CoreResourceProfileIdV1::Compile,
                requested_bytes: 32,
            }),
            ProtocolErrorV1::Core(CoreErrorV1::AllocationFailed { .. })
        ));

        let source_errors = [
            CoreWcag22EvaluationErrorV1::InvalidSrgb8 {
                field: "foreground",
                reason: "invalid".to_string(),
            },
            CoreWcag22EvaluationErrorV1::EmptyNotApplicableReason,
            CoreWcag22EvaluationErrorV1::ArtifactInvariantViolation {
                criterion: CoreWcag22CriterionV1::Sc143TextDefault,
                foreground: [0; 3],
                background: [255; 3],
            },
            CoreWcag22EvaluationErrorV1::EvidenceRegistryMismatch("drift".to_string()),
        ];
        for source in source_errors {
            assert!(matches!(
                project_core_error(CoreFeasibilityErrorV1::EvaluatorInvariantViolation {
                    candidate: Srgb8::new([0; 3]),
                    relation_id: relation_id.clone(),
                    adjacent: Srgb8::new([255; 3]),
                    violation: CoreEvaluatorInvariantV1::Source(source),
                }),
                ProtocolErrorV1::Core(CoreErrorV1::EvaluatorInvariantViolation { .. })
            ));
        }
        let evaluator_invariants = [
            CoreEvaluatorInvariantV1::UnexpectedNotEvaluated,
            CoreEvaluatorInvariantV1::InputMismatch,
            CoreEvaluatorInvariantV1::CriterionMismatch,
            CoreEvaluatorInvariantV1::EvidenceMismatch,
        ];
        for violation in evaluator_invariants {
            assert!(matches!(
                project_core_error(CoreFeasibilityErrorV1::EvaluatorInvariantViolation {
                    candidate: Srgb8::new([0; 3]),
                    relation_id: relation_id.clone(),
                    adjacent: Srgb8::new([255; 3]),
                    violation,
                }),
                ProtocolErrorV1::Core(CoreErrorV1::EvaluatorInvariantViolation { .. })
            ));
        }

        let compiler_invariants = [
            CoreCompilerInvariantV1::LayoutMismatch,
            CoreCompilerInvariantV1::AssessmentCardinalityMismatch {
                expected: 2,
                observed: 1,
            },
            CoreCompilerInvariantV1::CandidateCardinalityMismatch {
                expected: 256,
                observed: 255,
            },
            CoreCompilerInvariantV1::DecisionStorageRejectedCell,
            CoreCompilerInvariantV1::DecisionStorageRejectedPartition,
            CoreCompilerInvariantV1::CompleteResultMismatch,
        ];
        for violation in compiler_invariants {
            assert!(matches!(
                project_core_error(CoreFeasibilityErrorV1::CompilerInvariantViolation(
                    violation
                )),
                ProtocolErrorV1::Core(CoreErrorV1::CompilerInvariantViolation(_))
            ));
        }
    }

    #[test]
    fn serialized_stable_ids_are_sourced_from_core_keys() {
        assert_eq!(
            serde_json::to_string(&DomainIdV1::Srgb8NeutralAxis).unwrap(),
            serde_json::to_string(CoreDomainIdV1::Srgb8NeutralAxis.key()).unwrap()
        );
        assert_eq!(
            serde_json::to_string(&ResourceProfileIdV1::Compile).unwrap(),
            serde_json::to_string(CoreResourceProfileIdV1::Compile.key()).unwrap()
        );
        let criteria = [
            Wcag22CriterionV1::Sc143TextDefault,
            Wcag22CriterionV1::Sc143TextLargeScale,
            Wcag22CriterionV1::Sc1411UiComponentOrState,
            Wcag22CriterionV1::Sc1411GraphicalObject,
        ];
        for (protocol, core) in criteria.into_iter().zip(CoreWcag22CriterionV1::ALL) {
            assert_eq!(
                serde_json::to_string(&protocol).unwrap(),
                serde_json::to_string(core.key()).unwrap()
            );
        }
        assert_eq!(
            serde_json::to_string(&Wcag22ProfileIdV1::Wcag22Srgb8Contrast).unwrap(),
            serde_json::to_string(CoreWcag22ProfileIdV1::Wcag22Srgb8ContrastV1.key()).unwrap()
        );
        assert_eq!(
            serde_json::to_string(&NumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55).unwrap(),
            serde_json::to_string(CoreNumericalArtifactIdV2::Wcag22Srgb8LuminanceQ55V1.key())
                .unwrap()
        );
        assert_eq!(
            serde_json::to_string(&NumericalErrorBoundIdV2::Wcag22Srgb8OutwardQ55).unwrap(),
            serde_json::to_string(CoreNumericalErrorBoundIdV2::Wcag22Srgb8OutwardQ55V1.key())
                .unwrap()
        );
        assert_eq!(
            serde_json::to_string(&NumericalProofIdV2::Wcag22Srgb8FullDomainQ55).unwrap(),
            serde_json::to_string(CoreNumericalProofIdV2::Wcag22Srgb8FullDomainQ55V1.key())
                .unwrap()
        );
    }

    #[test]
    fn every_public_u64_error_field_serializes_as_exact_decimal_text() {
        let resource = serde_json::to_value(CoreErrorV1::ResourceLimitExceeded {
            profile_id: ResourceProfileIdV1::Compile,
            dimension: ResourceDimensionV1::OpaqueUtf8Bytes,
            requested: u64::MAX,
            limit: 2_047,
        })
        .unwrap();
        assert_eq!(
            resource
                .pointer("/details/dimension")
                .and_then(|v| v.as_str()),
            Some("opaqueUtf8Bytes")
        );
        assert_eq!(
            resource
                .pointer("/details/requested")
                .and_then(|v| v.as_str()),
            Some("18446744073709551615")
        );
        assert_eq!(
            resource.pointer("/details/limit").and_then(|v| v.as_str()),
            Some("2047")
        );

        let allocation = serde_json::to_value(CoreErrorV1::AllocationFailed {
            profile_id: ResourceProfileIdV1::Compile,
            requested_bytes: u64::MAX,
        })
        .unwrap();
        assert_eq!(
            allocation
                .pointer("/details/requestedBytes")
                .and_then(|v| v.as_str()),
            Some("18446744073709551615")
        );

        let compiler = serde_json::to_value(CompilerInvariantV1::AssessmentCardinalityMismatch {
            expected: u64::MAX,
            observed: 1,
        })
        .unwrap();
        assert_eq!(
            compiler.get("expected").and_then(|v| v.as_str()),
            Some("18446744073709551615")
        );
        assert_eq!(compiler.get("observed").and_then(|v| v.as_str()), Some("1"));
    }
}
