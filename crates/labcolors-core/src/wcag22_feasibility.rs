//! Bounded complete WCAG 2.2 enumeration over finite colour domains.
//!
//! Client identifiers and applicability remain opaque declarations. Core owns
//! canonicalization, exhaustive pair evaluation, packed decisions and sealed
//! content identities. This module neither selects nor ranks a colour.

use core::fmt;

use crate::Srgb8;
use crate::numerics::{
    NumericalArtifactIdV2, NumericalDecisionEvidenceV1, NumericalErrorBoundIdV2, NumericalProofIdV2,
};
use crate::sha256::{self, Hasher};
use crate::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22AssessmentV1, Wcag22ClientDeclaredNotApplicableV1,
    Wcag22CriterionV1, Wcag22EvaluationErrorV1, Wcag22ProfileIdV1, evaluate_wcag22_srgb8,
    wcag22_profile_v1,
};

#[path = "wcag22_feasibility/explicit.rs"]
#[cfg(feature = "wcag22-explicit-feasibility")]
pub mod explicit;

const CANDIDATE_COUNT: u64 = 256;
const PARTITION_BYTES: u64 = CANDIDATE_COUNT / 8;

const DOMAIN_SEPARATOR: &[u8] = b"labcolors/wcag22-feasibility/domain/v1\0";
const RELATION_SEPARATOR: &[u8] = b"labcolors/wcag22-feasibility/relations/v1\0";
const EVALUATION_SEPARATOR: &[u8] = b"labcolors/wcag22-feasibility/evaluation/v1\0";

/// Opaque client-owned relation identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelationId(std::sync::Arc<str>);

impl RelationId {
    /// Construct a non-empty opaque ID. Core never interprets its text.
    pub fn try_new(value: impl Into<String>) -> Result<Self, InvalidRequestV1> {
        let value = value.into();
        if value.is_empty() {
            return Err(InvalidRequestV1::EmptyRelationId);
        }
        Ok(Self(value.into()))
    }

    /// Exact client bytes used by canonical identity.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for RelationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Opaque client-owned occurrence identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OccurrenceId(String);

impl OccurrenceId {
    /// Construct a non-empty opaque ID. Core never interprets its text.
    pub fn try_new(value: impl Into<String>) -> Result<Self, InvalidRequestV1> {
        let value = value.into();
        if value.is_empty() {
            return Err(InvalidRequestV1::EmptyOccurrenceId);
        }
        Ok(Self(value))
    }

    /// Exact client bytes used by canonical identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OccurrenceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Registered finite candidate domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DomainIdV1 {
    /// Exact ascending `#000000 … #FFFFFF` encoded-sRGB8 neutral axis.
    Srgb8NeutralAxis,
}

impl DomainIdV1 {
    /// Stable semantic key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Srgb8NeutralAxis => "srgb8-neutral-axis-v1",
        }
    }

    /// Every candidate in canonical domain order.
    pub fn candidates(self) -> impl ExactSizeIterator<Item = Srgb8> {
        match self {
            Self::Srgb8NeutralAxis => (0_u16..256).map(|value| {
                let value = value as u8;
                Srgb8::new([value; 3])
            }),
        }
    }
}

/// Bounded compile-time resource policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResourceProfileIdV1 {
    /// Bounded offline compilation profile.
    Compile,
}

/// Individually preflighted resource dimensions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ResourceDimensionV1 {
    /// Relations present before canonical duplicate removal.
    RawRelations,
    /// Applicable adjacent entries before per-relation deduplication.
    RawAdjacentEntries,
    /// Raw UTF-8 payload bytes of opaque client IDs declared by one operation.
    ///
    /// Counting happens before canonicalization, deduplication or lookup and
    /// excludes Core-owned keys, framing, escaped transport bytes and total
    /// memory. Feasibility counts every raw explicit-candidate, relation and
    /// occurrence ID plus every `NotApplicable` reason ID; a registered domain
    /// contributes no candidate bytes. Selection counts its policy ID and every
    /// raw ordered candidate ID without recounting IDs retained by the completed
    /// feasibility source. Values from separate operations are never accumulated.
    OpaqueUtf8Bytes,
    /// Relations after canonical duplicate removal.
    CanonicalRelations,
    /// Canonical applicable relation-adjacent edges.
    ApplicableEdges,
    /// Exact candidate-edge evaluator calls and assessment cells.
    LogicalAssessments,
    /// Packed matrix plus partition bytes.
    PackedResultBytes,
}

// Offline compilation capacity: one standard WebAssembly 64-KiB page is the
// deterministic packed-result envelope, not a claim about allocator capacity,
// memory.grow or total operation memory. The committed benchmark artifact is
// the executable admission record for these product-policy limits.
const COMPILE_PAGE_SLOT_BYTES: u64 = 65_536;
const COMPILE_CONTENT_SLOTS: u64 = COMPILE_PAGE_SLOT_BYTES / PARTITION_BYTES - 1;
const COMPILE_LOGICAL_ASSESSMENTS: u64 = CANDIDATE_COUNT * COMPILE_CONTENT_SLOTS;

impl ResourceProfileIdV1 {
    /// Stable operational-policy key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::Compile => "compile-v1",
        }
    }

    /// Exact admitted bound for one dimension.
    pub const fn limit(self, dimension: ResourceDimensionV1) -> u64 {
        match (self, dimension) {
            (Self::Compile, ResourceDimensionV1::RawRelations)
            | (Self::Compile, ResourceDimensionV1::RawAdjacentEntries)
            | (Self::Compile, ResourceDimensionV1::CanonicalRelations)
            | (Self::Compile, ResourceDimensionV1::ApplicableEdges) => COMPILE_CONTENT_SLOTS,
            (Self::Compile, ResourceDimensionV1::OpaqueUtf8Bytes) => COMPILE_PAGE_SLOT_BYTES,
            (Self::Compile, ResourceDimensionV1::LogicalAssessments) => COMPILE_LOGICAL_ASSESSMENTS,
            (Self::Compile, ResourceDimensionV1::PackedResultBytes) => COMPILE_PAGE_SLOT_BYTES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RelationKindV1 {
    Applicable {
        criterion: Wcag22CriterionV1,
        adjacent: Vec<Srgb8>,
    },
    NotApplicable {
        declaration: Wcag22ClientDeclaredNotApplicableV1,
    },
}

/// One client-declared occurrence relation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationV1 {
    relation_id: RelationId,
    occurrence_id: OccurrenceId,
    kind: RelationKindV1,
}

impl RelationV1 {
    /// Declare an applicable occurrence with one or more exact adjacent colours.
    pub fn applicable(
        relation_id: RelationId,
        occurrence_id: OccurrenceId,
        criterion: Wcag22CriterionV1,
        adjacent: Vec<Srgb8>,
    ) -> Result<Self, InvalidRequestV1> {
        if adjacent.is_empty() {
            return Err(InvalidRequestV1::EmptyAdjacentSet { relation_id });
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

    /// Declare this relation NotApplicable for the identified occurrence.
    pub fn not_applicable(
        relation_id: RelationId,
        occurrence_id: OccurrenceId,
        declaration: Wcag22ClientDeclaredNotApplicableV1,
    ) -> Self {
        Self {
            relation_id,
            occurrence_id,
            kind: RelationKindV1::NotApplicable { declaration },
        }
    }

    /// Opaque relation identity.
    pub fn relation_id(&self) -> &RelationId {
        &self.relation_id
    }

    /// Opaque occurrence identity.
    pub fn occurrence_id(&self) -> &OccurrenceId {
        &self.occurrence_id
    }

    /// Borrow the applicable criterion and stored adjacent colours.
    /// Relations borrowed from a terminal are already canonicalized.
    pub fn as_applicable(&self) -> Option<(Wcag22CriterionV1, &[Srgb8])> {
        match &self.kind {
            RelationKindV1::Applicable {
                criterion,
                adjacent,
            } => Some((*criterion, adjacent)),
            RelationKindV1::NotApplicable { .. } => None,
        }
    }

    /// Borrow the client declaration when this relation is NotApplicable.
    pub fn as_not_applicable(&self) -> Option<&Wcag22ClientDeclaredNotApplicableV1> {
        match &self.kind {
            RelationKindV1::Applicable { .. } => None,
            RelationKindV1::NotApplicable { declaration } => Some(declaration),
        }
    }
}

/// Owned bounded-compilation request.
#[derive(Debug)]
pub struct RequestV1 {
    domain_id: DomainIdV1,
    relations: Vec<RelationV1>,
    resource_profile_id: ResourceProfileIdV1,
}

impl RequestV1 {
    /// Construct a locally well-formed request. Aggregate bounds are checked by
    /// [`evaluate`] before canonicalization or evaluation.
    pub fn try_new(
        domain_id: DomainIdV1,
        relations: Vec<RelationV1>,
        resource_profile_id: ResourceProfileIdV1,
    ) -> Result<Self, InvalidRequestV1> {
        if relations.is_empty() {
            return Err(InvalidRequestV1::EmptyRelations);
        }
        Ok(Self {
            domain_id,
            relations,
            resource_profile_id,
        })
    }
}

/// Canonical domain content identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DomainDigestV1([u8; 32]);

impl DomainDigestV1 {
    /// Exact SHA-256 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Canonical declared-relation-set content identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelationSetDigestV1([u8; 32]);

impl RelationSetDigestV1 {
    /// Exact SHA-256 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Semantic identity of one complete evaluated result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvaluationIdV1([u8; 32]);

impl EvaluationIdV1 {
    /// Exact SHA-256 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Locally invalid or contradictory input.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidRequestV1 {
    /// A relation ID was empty.
    EmptyRelationId,
    /// An occurrence ID was empty.
    EmptyOccurrenceId,
    /// No relations were declared.
    EmptyRelations,
    /// An explicit candidate ID was empty.
    #[cfg(feature = "wcag22-explicit-feasibility")]
    EmptyCandidateId,
    /// No explicit candidates were declared.
    #[cfg(feature = "wcag22-explicit-feasibility")]
    EmptyCandidates,
    /// The same explicit candidate ID occurred more than once.
    #[cfg(feature = "wcag22-explicit-feasibility")]
    DuplicateCandidateId { candidate_id: explicit::CandidateId },
    /// An applicable relation had no adjacent colour.
    EmptyAdjacentSet { relation_id: RelationId },
    /// The same relation ID described different canonical declarations.
    ConflictingRelationId { relation_id: RelationId },
    /// Checked cardinality arithmetic overflowed.
    ArithmeticOverflow,
}

impl fmt::Display for InvalidRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyRelationId => formatter.write_str("relation ID must be non-empty"),
            Self::EmptyOccurrenceId => formatter.write_str("occurrence ID must be non-empty"),
            Self::EmptyRelations => formatter.write_str("at least one relation is required"),
            #[cfg(feature = "wcag22-explicit-feasibility")]
            Self::EmptyCandidateId => formatter.write_str("candidate ID must be non-empty"),
            #[cfg(feature = "wcag22-explicit-feasibility")]
            Self::EmptyCandidates => formatter.write_str("at least one candidate is required"),
            #[cfg(feature = "wcag22-explicit-feasibility")]
            Self::DuplicateCandidateId { candidate_id } => {
                write!(formatter, "candidate ID {candidate_id} is duplicated")
            }
            Self::EmptyAdjacentSet { relation_id } => {
                write!(
                    formatter,
                    "applicable relation {relation_id} has no adjacency"
                )
            }
            Self::ConflictingRelationId { relation_id } => {
                write!(
                    formatter,
                    "relation ID {relation_id} has conflicting declarations"
                )
            }
            Self::ArithmeticOverflow => {
                formatter.write_str("checked feasibility arithmetic overflowed")
            }
        }
    }
}

impl std::error::Error for InvalidRequestV1 {}

/// A proof-bound atomic evaluator violated its adapter contract.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EvaluatorInvariantV1 {
    /// The atomic evaluator failed closed.
    Source(Wcag22EvaluationErrorV1),
    /// An applicable call unexpectedly returned `NotEvaluated`.
    UnexpectedNotEvaluated,
    /// Returned foreground or background bytes differed from the call.
    InputMismatch,
    /// Returned criterion differed from the declared occurrence criterion.
    CriterionMismatch,
    /// Returned proof-bearing evidence differed from the registered binding.
    EvidenceMismatch,
}

/// A compiler-owned completeness invariant failed before terminal minting.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompilerInvariantV1 {
    /// Derived raw/canonical layout counts violated their construction laws.
    LayoutMismatch,
    /// Observed evaluator cells differed from the preflighted `C×E` work.
    AssessmentCardinalityMismatch { expected: u64, observed: u64 },
    /// Observed candidates differed from the declared finite-domain count.
    CandidateCardinalityMismatch { expected: u64, observed: u64 },
    /// Packed storage rejected a cell proved addressable by preflight.
    DecisionStorageRejectedCell,
    /// Packed storage rejected a partition bit proved addressable by preflight.
    DecisionStorageRejectedPartition,
    /// Completed matrix, partition or proof counters disagreed.
    CompleteResultMismatch,
}

/// Complete-enumeration failure. No variant is a colour decision.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorV1 {
    /// Invalid or contradictory declared input.
    InvalidRequest(InvalidRequestV1),
    /// One exact preflight dimension exceeded the selected profile.
    ResourceLimitExceeded {
        profile_id: ResourceProfileIdV1,
        dimension: ResourceDimensionV1,
        requested: u64,
        limit: u64,
    },
    /// The exact packed allocation failed before the first evaluator call.
    AllocationFailed {
        profile_id: ResourceProfileIdV1,
        requested_bytes: u64,
    },
    /// The atomic result did not match the exact requested cell and evidence.
    EvaluatorInvariantViolation {
        candidate: Srgb8,
        relation_id: RelationId,
        adjacent: Srgb8,
        violation: EvaluatorInvariantV1,
    },
    /// Compiler-owned traversal, storage or proof state was inconsistent.
    CompilerInvariantViolation(CompilerInvariantV1),
}

impl From<InvalidRequestV1> for ErrorV1 {
    fn from(value: InvalidRequestV1) -> Self {
        Self::InvalidRequest(value)
    }
}

impl fmt::Display for ErrorV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(error) => error.fmt(formatter),
            Self::ResourceLimitExceeded {
                profile_id,
                dimension,
                requested,
                limit,
            } => write!(
                formatter,
                "{profile_id:?} feasibility limit exceeded for {dimension:?}: {requested} > {limit}"
            ),
            Self::AllocationFailed {
                profile_id,
                requested_bytes,
            } => write!(
                formatter,
                "{profile_id:?} feasibility allocation failed for {requested_bytes} bytes"
            ),
            Self::EvaluatorInvariantViolation {
                candidate,
                relation_id,
                adjacent,
                violation,
            } => write!(
                formatter,
                "atomic WCAG invariant failed for {candidate:?}/{relation_id}/{adjacent:?}: {violation:?}"
            ),
            Self::CompilerInvariantViolation(violation) => {
                write!(
                    formatter,
                    "WCAG feasibility compiler invariant failed: {violation:?}"
                )
            }
        }
    }
}

impl std::error::Error for ErrorV1 {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RawInputCountsV1 {
    raw_relations: u64,
    raw_adjacent_entries: u64,
    opaque_utf8_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CanonicalCountsV1 {
    canonical_relations: u64,
    applicable_relations: u64,
    not_evaluated_relations: u64,
    applicable_edges: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ResourceLimitsV1 {
    raw_relations: u64,
    raw_adjacent_entries: u64,
    opaque_utf8_bytes: u64,
    canonical_relations: u64,
    applicable_edges: u64,
    logical_assessments: u64,
    packed_result_bytes: u64,
}

impl ResourceLimitsV1 {
    const fn for_profile(profile: ResourceProfileIdV1) -> Self {
        Self {
            raw_relations: profile.limit(ResourceDimensionV1::RawRelations),
            raw_adjacent_entries: profile.limit(ResourceDimensionV1::RawAdjacentEntries),
            opaque_utf8_bytes: profile.limit(ResourceDimensionV1::OpaqueUtf8Bytes),
            canonical_relations: profile.limit(ResourceDimensionV1::CanonicalRelations),
            applicable_edges: profile.limit(ResourceDimensionV1::ApplicableEdges),
            logical_assessments: profile.limit(ResourceDimensionV1::LogicalAssessments),
            packed_result_bytes: profile.limit(ResourceDimensionV1::PackedResultBytes),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WorkLayoutV1 {
    raw_relations: u64,
    raw_adjacent_entries: u64,
    opaque_utf8_bytes: u64,
    canonical_relations: u64,
    applicable_relations: u64,
    not_evaluated_relations: u64,
    applicable_edges: u64,
    candidate_count: u64,
    logical_assessments: u64,
    failure_matrix_bytes: u64,
    partition_bytes: u64,
    packed_result_bytes: u64,
}

/// Private boundary implemented by every finite sRGB8 enumeration. Domain
/// implementations own only candidate identity/order; the compiler below owns
/// relation canonicalization, exhaustive evaluation and packed evidence.
trait FiniteSrgb8DomainV1: Sized {
    type Packing: PackedDomainV1 + fmt::Debug;

    fn raw_opaque_utf8_bytes(&self) -> Result<u64, InvalidRequestV1>;
    fn canonicalize(&mut self) -> Result<(), InvalidRequestV1>;
    fn candidates(&self) -> impl ExactSizeIterator<Item = Srgb8> + '_;

    fn candidate_count(&self) -> Result<u64, InvalidRequestV1> {
        u64::try_from(self.candidates().len()).map_err(|_| InvalidRequestV1::ArithmeticOverflow)
    }
}

impl FiniteSrgb8DomainV1 for DomainIdV1 {
    type Packing = NeutralAxisPackingV1;

    fn raw_opaque_utf8_bytes(&self) -> Result<u64, InvalidRequestV1> {
        Ok(0)
    }

    fn canonicalize(&mut self) -> Result<(), InvalidRequestV1> {
        Ok(())
    }

    fn candidates(&self) -> impl ExactSizeIterator<Item = Srgb8> + '_ {
        DomainIdV1::candidates(*self)
    }
}

fn overflow() -> InvalidRequestV1 {
    InvalidRequestV1::ArithmeticOverflow
}

fn checked_logical_assessments_for_domain_v1(
    candidate_count: u64,
    applicable_edges: u64,
) -> Result<u64, InvalidRequestV1> {
    candidate_count
        .checked_mul(applicable_edges)
        .ok_or_else(overflow)
}

#[cfg(test)]
fn checked_logical_assessments_v1(applicable_edges: u64) -> Result<u64, InvalidRequestV1> {
    checked_logical_assessments_for_domain_v1(CANDIDATE_COUNT, applicable_edges)
}

fn checked_bit_bytes_v1(bits: u64) -> Result<u64, InvalidRequestV1> {
    (bits / 8)
        .checked_add(u64::from(bits % 8 != 0))
        .ok_or_else(overflow)
}

fn checked_packed_result_bytes_for_domain_v1(
    candidate_count: u64,
    applicable_relations: u64,
    applicable_edges: u64,
) -> Result<u64, InvalidRequestV1> {
    if applicable_relations == 0 {
        return Ok(0);
    }
    let logical_assessments =
        checked_logical_assessments_for_domain_v1(candidate_count, applicable_edges)?;
    let matrix_bytes = checked_bit_bytes_v1(logical_assessments)?;
    let partition_bytes = checked_bit_bytes_v1(candidate_count)?;
    matrix_bytes
        .checked_add(partition_bytes)
        .ok_or_else(overflow)
}

#[cfg(test)]
fn checked_packed_result_bytes_v1(
    applicable_relations: u64,
    applicable_edges: u64,
) -> Result<u64, InvalidRequestV1> {
    checked_packed_result_bytes_for_domain_v1(
        CANDIDATE_COUNT,
        applicable_relations,
        applicable_edges,
    )
}

fn resource_value(
    raw: RawInputCountsV1,
    canonical: CanonicalCountsV1,
    logical_assessments: u64,
    packed_result_bytes: u64,
    dimension: ResourceDimensionV1,
) -> u64 {
    match dimension {
        ResourceDimensionV1::RawRelations => raw.raw_relations,
        ResourceDimensionV1::RawAdjacentEntries => raw.raw_adjacent_entries,
        ResourceDimensionV1::OpaqueUtf8Bytes => raw.opaque_utf8_bytes,
        ResourceDimensionV1::CanonicalRelations => canonical.canonical_relations,
        ResourceDimensionV1::ApplicableEdges => canonical.applicable_edges,
        ResourceDimensionV1::LogicalAssessments => logical_assessments,
        ResourceDimensionV1::PackedResultBytes => packed_result_bytes,
    }
}

fn resource_limit(limits: ResourceLimitsV1, dimension: ResourceDimensionV1) -> u64 {
    match dimension {
        ResourceDimensionV1::RawRelations => limits.raw_relations,
        ResourceDimensionV1::RawAdjacentEntries => limits.raw_adjacent_entries,
        ResourceDimensionV1::OpaqueUtf8Bytes => limits.opaque_utf8_bytes,
        ResourceDimensionV1::CanonicalRelations => limits.canonical_relations,
        ResourceDimensionV1::ApplicableEdges => limits.applicable_edges,
        ResourceDimensionV1::LogicalAssessments => limits.logical_assessments,
        ResourceDimensionV1::PackedResultBytes => limits.packed_result_bytes,
    }
}

#[cfg(test)]
fn checked_layout_v1(
    profile_id: ResourceProfileIdV1,
    raw: RawInputCountsV1,
    canonical: CanonicalCountsV1,
    limits: ResourceLimitsV1,
    addressable_byte_limit: u64,
) -> Result<WorkLayoutV1, ErrorV1> {
    checked_layout_for_domain_v1(
        profile_id,
        CANDIDATE_COUNT,
        raw,
        canonical,
        limits,
        addressable_byte_limit,
    )
}

fn checked_layout_for_domain_v1(
    profile_id: ResourceProfileIdV1,
    candidate_count: u64,
    raw: RawInputCountsV1,
    canonical: CanonicalCountsV1,
    limits: ResourceLimitsV1,
    addressable_byte_limit: u64,
) -> Result<WorkLayoutV1, ErrorV1> {
    let total_relations = canonical
        .applicable_relations
        .checked_add(canonical.not_evaluated_relations)
        .ok_or_else(overflow)?;
    if total_relations != canonical.canonical_relations {
        return Err(ErrorV1::CompilerInvariantViolation(
            CompilerInvariantV1::LayoutMismatch,
        ));
    }
    if (canonical.applicable_relations == 0) != (canonical.applicable_edges == 0) {
        return Err(ErrorV1::CompilerInvariantViolation(
            CompilerInvariantV1::LayoutMismatch,
        ));
    }
    if canonical.applicable_edges < canonical.applicable_relations {
        return Err(ErrorV1::CompilerInvariantViolation(
            CompilerInvariantV1::LayoutMismatch,
        ));
    }
    if raw.raw_relations < canonical.canonical_relations
        || raw.raw_adjacent_entries < canonical.applicable_edges
    {
        return Err(ErrorV1::CompilerInvariantViolation(
            CompilerInvariantV1::LayoutMismatch,
        ));
    }

    if candidate_count == 0 {
        return Err(ErrorV1::CompilerInvariantViolation(
            CompilerInvariantV1::LayoutMismatch,
        ));
    }
    let logical_assessments =
        checked_logical_assessments_for_domain_v1(candidate_count, canonical.applicable_edges)?;
    let failure_matrix_bytes = if canonical.applicable_relations == 0 {
        0
    } else {
        checked_bit_bytes_v1(logical_assessments)?
    };
    let partition_bytes = if canonical.applicable_relations == 0 {
        0
    } else {
        checked_bit_bytes_v1(candidate_count)?
    };
    let packed_result_bytes = checked_packed_result_bytes_for_domain_v1(
        candidate_count,
        canonical.applicable_relations,
        canonical.applicable_edges,
    )?;

    for dimension in [
        ResourceDimensionV1::RawRelations,
        ResourceDimensionV1::RawAdjacentEntries,
        ResourceDimensionV1::OpaqueUtf8Bytes,
        ResourceDimensionV1::CanonicalRelations,
        ResourceDimensionV1::ApplicableEdges,
        ResourceDimensionV1::LogicalAssessments,
        ResourceDimensionV1::PackedResultBytes,
    ] {
        let requested = resource_value(
            raw,
            canonical,
            logical_assessments,
            packed_result_bytes,
            dimension,
        );
        let limit = resource_limit(limits, dimension);
        if requested > limit {
            return Err(ErrorV1::ResourceLimitExceeded {
                profile_id,
                dimension,
                requested,
                limit,
            });
        }
    }
    if packed_result_bytes > addressable_byte_limit {
        return Err(ErrorV1::ResourceLimitExceeded {
            profile_id,
            dimension: ResourceDimensionV1::PackedResultBytes,
            requested: packed_result_bytes,
            limit: addressable_byte_limit,
        });
    }

    Ok(WorkLayoutV1 {
        raw_relations: raw.raw_relations,
        raw_adjacent_entries: raw.raw_adjacent_entries,
        opaque_utf8_bytes: raw.opaque_utf8_bytes,
        canonical_relations: canonical.canonical_relations,
        applicable_relations: canonical.applicable_relations,
        not_evaluated_relations: canonical.not_evaluated_relations,
        applicable_edges: canonical.applicable_edges,
        candidate_count,
        logical_assessments,
        failure_matrix_bytes,
        partition_bytes,
        packed_result_bytes,
    })
}

fn checked_layout_for_finite_domain_v1<D: FiniteSrgb8DomainV1>(
    profile_id: ResourceProfileIdV1,
    domain: &D,
    raw: RawInputCountsV1,
    canonical: CanonicalCountsV1,
    limits: ResourceLimitsV1,
    addressable_byte_limit: u64,
) -> Result<WorkLayoutV1, ErrorV1> {
    checked_layout_for_domain_v1(
        profile_id,
        domain.candidate_count()?,
        raw,
        canonical,
        limits,
        addressable_byte_limit,
    )
}

fn add_checked(target: &mut u64, value: u64) -> Result<(), ErrorV1> {
    *target = target.checked_add(value).ok_or_else(overflow)?;
    Ok(())
}

fn usize_as_u64(value: usize) -> Result<u64, ErrorV1> {
    u64::try_from(value).map_err(|_| overflow().into())
}

fn raw_relation_counts(relations: &[RelationV1]) -> Result<RawInputCountsV1, ErrorV1> {
    let raw_relations = usize_as_u64(relations.len())?;
    let mut raw_adjacent_entries = 0_u64;
    let mut opaque_utf8_bytes = 0_u64;
    for relation in relations {
        add_checked(
            &mut opaque_utf8_bytes,
            usize_as_u64(relation.relation_id.as_str().len())?,
        )?;
        add_checked(
            &mut opaque_utf8_bytes,
            usize_as_u64(relation.occurrence_id.as_str().len())?,
        )?;
        match &relation.kind {
            RelationKindV1::Applicable { adjacent, .. } => {
                add_checked(&mut raw_adjacent_entries, usize_as_u64(adjacent.len())?)?
            }
            RelationKindV1::NotApplicable { declaration } => add_checked(
                &mut opaque_utf8_bytes,
                usize_as_u64(declaration.reason_id().len())?,
            )?,
        }
    }
    Ok(RawInputCountsV1 {
        raw_relations,
        raw_adjacent_entries,
        opaque_utf8_bytes,
    })
}

#[cfg(test)]
fn raw_counts(request: &RequestV1) -> Result<RawInputCountsV1, ErrorV1> {
    raw_counts_for_domain(&request.domain_id, &request.relations)
}

fn raw_counts_for_domain<D: FiniteSrgb8DomainV1>(
    domain: &D,
    relations: &[RelationV1],
) -> Result<RawInputCountsV1, ErrorV1> {
    let mut raw = raw_relation_counts(relations)?;
    add_checked(&mut raw.opaque_utf8_bytes, domain.raw_opaque_utf8_bytes()?)?;
    Ok(raw)
}

fn check_raw_limits(profile_id: ResourceProfileIdV1, raw: RawInputCountsV1) -> Result<(), ErrorV1> {
    for (dimension, requested) in [
        (ResourceDimensionV1::RawRelations, raw.raw_relations),
        (
            ResourceDimensionV1::RawAdjacentEntries,
            raw.raw_adjacent_entries,
        ),
        (ResourceDimensionV1::OpaqueUtf8Bytes, raw.opaque_utf8_bytes),
    ] {
        let limit = profile_id.limit(dimension);
        if requested > limit {
            return Err(ErrorV1::ResourceLimitExceeded {
                profile_id,
                dimension,
                requested,
                limit,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
fn canonicalize(request: &mut RequestV1) -> Result<CanonicalCountsV1, ErrorV1> {
    canonicalize_relations(&mut request.relations)
}

fn canonicalize_relations(relations: &mut Vec<RelationV1>) -> Result<CanonicalCountsV1, ErrorV1> {
    for relation in relations.iter_mut() {
        if let RelationKindV1::Applicable { adjacent, .. } = &mut relation.kind {
            adjacent.sort_unstable();
            adjacent.dedup();
        }
    }
    relations.sort_unstable_by(|left, right| left.relation_id.cmp(&right.relation_id));

    let conflict_index = relations
        .windows(2)
        .position(|pair| pair[0].relation_id == pair[1].relation_id && pair[0] != pair[1]);
    if let Some(conflict_index) = conflict_index {
        // `RelationId` uses Arc<str>: this is a fixed-size ownership clone,
        // never a proportional copy of the opaque client bytes.
        let relation_id = relations[conflict_index].relation_id.clone();
        return Err(InvalidRequestV1::ConflictingRelationId { relation_id }.into());
    }
    relations.dedup();

    let mut applicable_relations = 0_u64;
    let mut not_evaluated_relations = 0_u64;
    let mut applicable_edges = 0_u64;
    for relation in relations.iter() {
        match &relation.kind {
            RelationKindV1::Applicable { adjacent, .. } => {
                add_checked(&mut applicable_relations, 1)?;
                add_checked(&mut applicable_edges, usize_as_u64(adjacent.len())?)?;
            }
            RelationKindV1::NotApplicable { .. } => add_checked(&mut not_evaluated_relations, 1)?,
        }
    }
    Ok(CanonicalCountsV1 {
        canonical_relations: usize_as_u64(relations.len())?,
        applicable_relations,
        not_evaluated_relations,
        applicable_edges,
    })
}

fn first_applicable_edge(relations: &[RelationV1]) -> Option<(&RelationV1, Srgb8)> {
    relations.iter().find_map(|relation| match &relation.kind {
        RelationKindV1::Applicable { adjacent, .. } => {
            adjacent.first().copied().map(|value| (relation, value))
        }
        RelationKindV1::NotApplicable { .. } => None,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AtomicEvidenceBindingV1 {
    profile_id: Wcag22ProfileIdV1,
    artifact_id: NumericalArtifactIdV2,
    bound_id: NumericalErrorBoundIdV2,
    proof_id: NumericalProofIdV2,
    proof_sha256: [u8; 32],
}

fn decode_sha256_hex(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 || !value.is_ascii() {
        return None;
    }
    fn nibble(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        }
    }
    let bytes = value.as_bytes();
    let mut output = [0_u8; 32];
    for index in 0..32 {
        output[index] = (nibble(bytes[index * 2])? << 4) | nibble(bytes[index * 2 + 1])?;
    }
    Some(output)
}

fn expected_atomic_evidence_binding_v1() -> Result<AtomicEvidenceBindingV1, Wcag22EvaluationErrorV1>
{
    let profile = wcag22_profile_v1();
    let proof_sha256 = decode_sha256_hex(profile.proof_sha256).ok_or_else(|| {
        Wcag22EvaluationErrorV1::EvidenceRegistryMismatch(
            "WCAG22 proof SHA-256 is not canonical lowercase hex".to_string(),
        )
    })?;
    Ok(AtomicEvidenceBindingV1 {
        profile_id: profile.profile_id,
        artifact_id: profile.artifact_id,
        bound_id: profile.bound_id,
        proof_id: profile.proof_id,
        proof_sha256,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PairEvaluationV1 {
    Evaluated {
        foreground: [u8; 3],
        background: [u8; 3],
        criterion: Wcag22CriterionV1,
        decision: Wcag22ApplicableDecisionV1,
        evidence: AtomicEvidenceBindingV1,
    },
    NotEvaluated,
    InvalidEvidence,
}

trait PairEvaluator {
    fn evaluate_pair(
        &mut self,
        candidate: Srgb8,
        adjacent: Srgb8,
        criterion: Wcag22CriterionV1,
    ) -> Result<PairEvaluationV1, Wcag22EvaluationErrorV1>;
}

fn evaluate_bound_pair<E: PairEvaluator>(
    evaluator: &mut E,
    expected_evidence: &AtomicEvidenceBindingV1,
    candidate: Srgb8,
    adjacent: Srgb8,
    criterion: Wcag22CriterionV1,
) -> Result<Wcag22ApplicableDecisionV1, EvaluatorInvariantV1> {
    let result = evaluator
        .evaluate_pair(candidate, adjacent, criterion)
        .map_err(EvaluatorInvariantV1::Source)?;
    let (foreground, background, actual_criterion, decision, evidence) = match result {
        PairEvaluationV1::Evaluated {
            foreground,
            background,
            criterion,
            decision,
            evidence,
        } => (foreground, background, criterion, decision, evidence),
        PairEvaluationV1::NotEvaluated => {
            return Err(EvaluatorInvariantV1::UnexpectedNotEvaluated);
        }
        PairEvaluationV1::InvalidEvidence => {
            return Err(EvaluatorInvariantV1::EvidenceMismatch);
        }
    };
    if foreground != candidate.bytes() || background != adjacent.bytes() {
        return Err(EvaluatorInvariantV1::InputMismatch);
    }
    if actual_criterion != criterion {
        return Err(EvaluatorInvariantV1::CriterionMismatch);
    }
    if evidence != *expected_evidence {
        return Err(EvaluatorInvariantV1::EvidenceMismatch);
    }
    Ok(decision)
}

struct AtomicPairEvaluator {
    expected_evidence: Result<AtomicEvidenceBindingV1, Wcag22EvaluationErrorV1>,
}

impl AtomicPairEvaluator {
    fn new() -> Self {
        Self {
            expected_evidence: expected_atomic_evidence_binding_v1(),
        }
    }
}

impl PairEvaluator for AtomicPairEvaluator {
    fn evaluate_pair(
        &mut self,
        candidate: Srgb8,
        adjacent: Srgb8,
        criterion: Wcag22CriterionV1,
    ) -> Result<PairEvaluationV1, Wcag22EvaluationErrorV1> {
        let proof_sha256 = match &self.expected_evidence {
            Ok(binding) => binding.proof_sha256,
            Err(error) => return Err(error.clone()),
        };
        match evaluate_wcag22_srgb8(candidate.bytes(), adjacent.bytes(), criterion)? {
            Wcag22AssessmentV1::Evaluated {
                profile_id,
                criterion,
                measurement,
                decision,
                evidence,
            } => {
                let (artifact_id, bound_id, proof_id) = match evidence {
                    NumericalDecisionEvidenceV1::CanonicalFiniteBounded(payload) => (
                        payload.artifact_id(),
                        payload.bound_id(),
                        payload.proof_id(),
                    ),
                    _ => return Ok(PairEvaluationV1::InvalidEvidence),
                };
                Ok(PairEvaluationV1::Evaluated {
                    foreground: measurement.foreground,
                    background: measurement.background,
                    criterion,
                    decision,
                    evidence: AtomicEvidenceBindingV1 {
                        profile_id,
                        artifact_id,
                        bound_id,
                        proof_id,
                        proof_sha256,
                    },
                })
            }
            Wcag22AssessmentV1::NotEvaluated { .. } => Ok(PairEvaluationV1::NotEvaluated),
        }
    }
}

trait DecisionStorage {
    fn try_reserve_exact(&mut self, requested_bytes: usize) -> Result<(), ()>;
    fn write_decision(
        &mut self,
        logical_index: u64,
        decision: Wcag22ApplicableDecisionV1,
    ) -> Result<(), ()>;
    #[cfg(feature = "wcag22-explicit-feasibility")]
    fn write_feasible_candidate(
        &mut self,
        matrix_bytes: u64,
        candidate_index: u64,
    ) -> Result<(), ()>;
    fn finish(&mut self, partition: &[u8]) -> Result<(), ()>;
}

#[derive(Debug, Default)]
struct PackedDecisionStorage {
    bytes: Vec<u8>,
}

impl PackedDecisionStorage {
    #[cfg(feature = "wcag22-explicit-feasibility")]
    fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

impl DecisionStorage for PackedDecisionStorage {
    fn try_reserve_exact(&mut self, requested_bytes: usize) -> Result<(), ()> {
        self.bytes
            .try_reserve_exact(requested_bytes)
            .map_err(|_| ())?;
        self.bytes.resize(requested_bytes, 0);
        Ok(())
    }

    fn write_decision(
        &mut self,
        logical_index: u64,
        decision: Wcag22ApplicableDecisionV1,
    ) -> Result<(), ()> {
        if decision == Wcag22ApplicableDecisionV1::Fail {
            let byte_index = usize::try_from(logical_index / 8).map_err(|_| ())?;
            let bit = (logical_index % 8) as u8;
            let byte = self.bytes.get_mut(byte_index).ok_or(())?;
            *byte |= 1_u8 << bit;
        }
        Ok(())
    }

    #[cfg(feature = "wcag22-explicit-feasibility")]
    fn write_feasible_candidate(
        &mut self,
        matrix_bytes: u64,
        candidate_index: u64,
    ) -> Result<(), ()> {
        let byte_index = matrix_bytes
            .checked_add(candidate_index / 8)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or(())?;
        let bit = (candidate_index % 8) as u8;
        let byte = self.bytes.get_mut(byte_index).ok_or(())?;
        *byte |= 1_u8 << bit;
        Ok(())
    }

    fn finish(&mut self, partition: &[u8]) -> Result<(), ()> {
        let start = self.bytes.len().checked_sub(partition.len()).ok_or(())?;
        let destination = self.bytes.get_mut(start..).ok_or(())?;
        if destination.len() != partition.len() {
            return Err(());
        }
        destination.copy_from_slice(partition);
        Ok(())
    }
}

trait PackedDomainV1 {
    type Partition: Default + fmt::Debug;

    fn record_feasible<S: DecisionStorage>(
        partition: &mut Self::Partition,
        storage: &mut S,
        matrix_bytes: u64,
        candidate_index: usize,
    ) -> Result<(), ()>;

    fn finish<S: DecisionStorage>(partition: &Self::Partition, storage: &mut S) -> Result<(), ()>;
}

#[derive(Debug)]
struct NeutralAxisPackingV1;

impl PackedDomainV1 for NeutralAxisPackingV1 {
    type Partition = [u8; PARTITION_BYTES as usize];

    fn record_feasible<S: DecisionStorage>(
        partition: &mut Self::Partition,
        _storage: &mut S,
        _matrix_bytes: u64,
        candidate_index: usize,
    ) -> Result<(), ()> {
        // The associated type seals this packing to DomainIdV1, whose exact
        // iterator yields only 0..256; bounds safety therefore cannot depend on
        // client data. The post-loop check only validates that private iterator
        // contract after this write, without a branch in every hot cell.
        partition[candidate_index / 8] |= 1_u8 << (candidate_index % 8);
        Ok(())
    }

    fn finish<S: DecisionStorage>(partition: &Self::Partition, storage: &mut S) -> Result<(), ()> {
        storage.finish(partition)
    }
}

#[derive(Debug)]
#[cfg(feature = "wcag22-explicit-feasibility")]
struct VariablePackingV1;

#[cfg(feature = "wcag22-explicit-feasibility")]
impl PackedDomainV1 for VariablePackingV1 {
    type Partition = ();

    fn record_feasible<S: DecisionStorage>(
        _partition: &mut Self::Partition,
        storage: &mut S,
        matrix_bytes: u64,
        candidate_index: usize,
    ) -> Result<(), ()> {
        let candidate_index = u64::try_from(candidate_index).map_err(|_| ())?;
        storage.write_feasible_candidate(matrix_bytes, candidate_index)
    }

    fn finish<S: DecisionStorage>(
        _partition: &Self::Partition,
        _storage: &mut S,
    ) -> Result<(), ()> {
        Ok(())
    }
}

#[derive(Debug)]
struct KernelEvaluatedV1<D: FiniteSrgb8DomainV1> {
    domain: D,
    resource_profile_id: ResourceProfileIdV1,
    relations: Vec<RelationV1>,
    layout: WorkLayoutV1,
    observed_candidates: u64,
    observed_assessments: u64,
    passing_candidates: u64,
    partition: <D::Packing as PackedDomainV1>::Partition,
    atomic_evidence: AtomicEvidenceBindingV1,
}

#[derive(Debug)]
struct KernelNotEvaluatedV1<D: FiniteSrgb8DomainV1> {
    domain: D,
    resource_profile_id: ResourceProfileIdV1,
    relations: Vec<RelationV1>,
    layout: WorkLayoutV1,
}

#[derive(Debug)]
enum KernelResultV1<D: FiniteSrgb8DomainV1> {
    Evaluated(KernelEvaluatedV1<D>),
    NotEvaluated(KernelNotEvaluatedV1<D>),
}

struct KernelRequestV1<D> {
    domain: D,
    relations: Vec<RelationV1>,
    resource_profile_id: ResourceProfileIdV1,
}

fn evaluator_error(
    candidate: Srgb8,
    relation: &RelationV1,
    adjacent: Srgb8,
    violation: EvaluatorInvariantV1,
) -> ErrorV1 {
    ErrorV1::EvaluatorInvariantViolation {
        candidate,
        relation_id: relation.relation_id.clone(),
        adjacent,
        violation,
    }
}

fn compiler_result_error() -> ErrorV1 {
    ErrorV1::CompilerInvariantViolation(CompilerInvariantV1::CompleteResultMismatch)
}

fn evaluate_with<E: PairEvaluator, S: DecisionStorage>(
    request: RequestV1,
    evaluator: &mut E,
    storage: &mut S,
) -> Result<KernelResultV1<DomainIdV1>, ErrorV1> {
    evaluate_domain_with(
        KernelRequestV1 {
            domain: request.domain_id,
            relations: request.relations,
            resource_profile_id: request.resource_profile_id,
        },
        evaluator,
        storage,
    )
}

fn evaluate_domain_with<D, E, S>(
    mut request: KernelRequestV1<D>,
    evaluator: &mut E,
    storage: &mut S,
) -> Result<KernelResultV1<D>, ErrorV1>
where
    D: FiniteSrgb8DomainV1,
    E: PairEvaluator,
    S: DecisionStorage,
{
    let raw = raw_counts_for_domain(&request.domain, &request.relations)?;
    check_raw_limits(request.resource_profile_id, raw)?;
    request.domain.canonicalize()?;
    let canonical = canonicalize_relations(&mut request.relations)?;
    let layout = checked_layout_for_finite_domain_v1(
        request.resource_profile_id,
        &request.domain,
        raw,
        canonical,
        ResourceLimitsV1::for_profile(request.resource_profile_id),
        u64::from(u32::MAX),
    )?;
    let requested_bytes = usize::try_from(layout.packed_result_bytes).map_err(|_| {
        ErrorV1::ResourceLimitExceeded {
            profile_id: request.resource_profile_id,
            dimension: ResourceDimensionV1::PackedResultBytes,
            requested: layout.packed_result_bytes,
            limit: usize::MAX as u64,
        }
    })?;
    storage
        .try_reserve_exact(requested_bytes)
        .map_err(|()| ErrorV1::AllocationFailed {
            profile_id: request.resource_profile_id,
            requested_bytes: layout.packed_result_bytes,
        })?;

    if layout.applicable_relations == 0 {
        return Ok(KernelResultV1::NotEvaluated(KernelNotEvaluatedV1 {
            domain: request.domain,
            resource_profile_id: request.resource_profile_id,
            relations: request.relations,
            layout,
        }));
    }

    let first_applicable = first_applicable_edge(&request.relations);
    let Some(first_applicable) = first_applicable else {
        return Err(ErrorV1::CompilerInvariantViolation(
            CompilerInvariantV1::LayoutMismatch,
        ));
    };
    let first_candidate = request.domain.candidates().next().ok_or({
        ErrorV1::CompilerInvariantViolation(CompilerInvariantV1::CandidateCardinalityMismatch {
            expected: layout.candidate_count,
            observed: 0,
        })
    })?;
    let expected_evidence = expected_atomic_evidence_binding_v1().map_err(|source| {
        evaluator_error(
            first_candidate,
            first_applicable.0,
            first_applicable.1,
            EvaluatorInvariantV1::Source(source),
        )
    })?;
    let mut logical_index = 0_u64;
    let mut observed_candidates = 0_u64;
    let mut passing_candidates = 0_u64;
    let mut partition = <D::Packing as PackedDomainV1>::Partition::default();
    for (candidate_index, candidate) in request.domain.candidates().enumerate() {
        let mut candidate_passes = true;
        for relation in &request.relations {
            let RelationKindV1::Applicable {
                criterion,
                adjacent,
            } = &relation.kind
            else {
                continue;
            };
            for adjacent in adjacent.iter().copied() {
                let decision = evaluate_bound_pair(
                    evaluator,
                    &expected_evidence,
                    candidate,
                    adjacent,
                    *criterion,
                )
                .map_err(|violation| evaluator_error(candidate, relation, adjacent, violation))?;
                storage
                    .write_decision(logical_index, decision)
                    .map_err(|()| {
                        ErrorV1::CompilerInvariantViolation(
                            CompilerInvariantV1::DecisionStorageRejectedCell,
                        )
                    })?;
                logical_index += 1;
                candidate_passes &= decision == Wcag22ApplicableDecisionV1::Pass;
            }
        }
        if candidate_passes {
            D::Packing::record_feasible(
                &mut partition,
                storage,
                layout.failure_matrix_bytes,
                candidate_index,
            )
            .map_err(|()| {
                ErrorV1::CompilerInvariantViolation(
                    CompilerInvariantV1::DecisionStorageRejectedPartition,
                )
            })?;
            passing_candidates += 1;
        }
        observed_candidates += 1;
    }
    if logical_index != layout.logical_assessments {
        return Err(ErrorV1::CompilerInvariantViolation(
            CompilerInvariantV1::AssessmentCardinalityMismatch {
                expected: layout.logical_assessments,
                observed: logical_index,
            },
        ));
    }
    if observed_candidates != layout.candidate_count {
        return Err(ErrorV1::CompilerInvariantViolation(
            CompilerInvariantV1::CandidateCardinalityMismatch {
                expected: layout.candidate_count,
                observed: observed_candidates,
            },
        ));
    }
    D::Packing::finish(&partition, storage).map_err(|()| {
        ErrorV1::CompilerInvariantViolation(CompilerInvariantV1::DecisionStorageRejectedPartition)
    })?;

    Ok(KernelResultV1::Evaluated(KernelEvaluatedV1 {
        domain: request.domain,
        resource_profile_id: request.resource_profile_id,
        relations: request.relations,
        layout,
        observed_candidates,
        observed_assessments: logical_index,
        passing_candidates,
        partition,
        atomic_evidence: expected_evidence,
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EvaluationProofCountersV1 {
    logical_assessments: u64,
    passing_candidates: u64,
    failing_candidates: u64,
}

fn packed_bit(bytes: &[u8], logical_index: u64) -> bool {
    // Every caller either validated the exact packed length immediately above
    // or borrows a sealed record that passed that validation before minting.
    let byte_index = (logical_index / 8) as usize;
    let bit = (logical_index % 8) as u8;
    bytes[byte_index] & (1_u8 << bit) != 0
}

#[cfg(feature = "wcag22-explicit-feasibility")]
fn unused_tail_bits_are_zero(bytes: &[u8], used_bits: u64) -> bool {
    let remainder = (used_bits % 8) as u8;
    if remainder == 0 {
        return true;
    }
    let Some(last) = bytes.last() else {
        return false;
    };
    let used_mask = ((1_u16 << remainder) - 1) as u8;
    last & !used_mask == 0
}

fn validate_neutral_complete_result_v1(
    layout: WorkLayoutV1,
    matrix: &[u8],
    partition: &[u8; PARTITION_BYTES as usize],
    counters: EvaluationProofCountersV1,
) -> Result<(), CompilerInvariantV1> {
    let expected_matrix_bytes = usize::try_from(layout.failure_matrix_bytes)
        .map_err(|_| CompilerInvariantV1::CompleteResultMismatch)?;
    if matrix.len() != expected_matrix_bytes {
        return Err(CompilerInvariantV1::CompleteResultMismatch);
    }
    if counters.logical_assessments != layout.logical_assessments {
        return Err(CompilerInvariantV1::CompleteResultMismatch);
    }
    let mut derived_partition = [0_u8; PARTITION_BYTES as usize];
    let mut passing = 0_u64;
    for candidate in 0_u64..CANDIDATE_COUNT {
        let row_start = candidate
            .checked_mul(layout.applicable_edges)
            .ok_or(CompilerInvariantV1::CompleteResultMismatch)?;
        let mut row_passes = true;
        for edge in 0..layout.applicable_edges {
            let failed = packed_bit(matrix, row_start + edge);
            row_passes &= !failed;
        }
        if row_passes {
            let candidate = usize::try_from(candidate)
                .map_err(|_| CompilerInvariantV1::CompleteResultMismatch)?;
            derived_partition[candidate / 8] |= 1_u8 << (candidate % 8);
            passing += 1;
        }
    }
    if &derived_partition != partition || counters.passing_candidates != passing {
        return Err(CompilerInvariantV1::CompleteResultMismatch);
    }
    if counters
        .passing_candidates
        .checked_add(counters.failing_candidates)
        != Some(CANDIDATE_COUNT)
    {
        return Err(CompilerInvariantV1::CompleteResultMismatch);
    }
    Ok(())
}

#[cfg(feature = "wcag22-explicit-feasibility")]
fn validate_variable_complete_result_v1(
    layout: WorkLayoutV1,
    matrix: &[u8],
    partition: &[u8],
    counters: EvaluationProofCountersV1,
) -> Result<(), CompilerInvariantV1> {
    let expected_matrix_bytes = usize::try_from(layout.failure_matrix_bytes)
        .map_err(|_| CompilerInvariantV1::CompleteResultMismatch)?;
    let expected_partition_bytes = usize::try_from(layout.partition_bytes)
        .map_err(|_| CompilerInvariantV1::CompleteResultMismatch)?;
    if matrix.len() != expected_matrix_bytes
        || partition.len() != expected_partition_bytes
        || !unused_tail_bits_are_zero(matrix, layout.logical_assessments)
        || !unused_tail_bits_are_zero(partition, layout.candidate_count)
    {
        return Err(CompilerInvariantV1::CompleteResultMismatch);
    }
    if counters.logical_assessments != layout.logical_assessments {
        return Err(CompilerInvariantV1::CompleteResultMismatch);
    }
    let mut passing = 0_u64;
    for candidate in 0_u64..layout.candidate_count {
        let row_start = candidate
            .checked_mul(layout.applicable_edges)
            .ok_or(CompilerInvariantV1::CompleteResultMismatch)?;
        let mut row_passes = true;
        for edge in 0..layout.applicable_edges {
            let failed = packed_bit(matrix, row_start + edge);
            row_passes &= !failed;
        }
        if row_passes != packed_bit(partition, candidate) {
            return Err(CompilerInvariantV1::CompleteResultMismatch);
        }
        if row_passes {
            passing += 1;
        }
    }
    if counters.passing_candidates != passing {
        return Err(CompilerInvariantV1::CompleteResultMismatch);
    }
    if counters
        .passing_candidates
        .checked_add(counters.failing_candidates)
        != Some(layout.candidate_count)
    {
        return Err(CompilerInvariantV1::CompleteResultMismatch);
    }
    Ok(())
}

// Identity encoders target this minimal sink so production SHA-256 and bounded
// byte-work probes execute the same byte grammar rather than parallel copies.
trait CanonicalByteSink {
    fn write(&mut self, bytes: &[u8]);
}

impl CanonicalByteSink for Hasher {
    fn write(&mut self, bytes: &[u8]) {
        self.update(bytes);
    }
}

fn hash_len_prefixed(sink: &mut impl CanonicalByteSink, bytes: &[u8]) {
    hash_u64(sink, bytes.len() as u64);
    sink.write(bytes);
}

fn hash_u64(sink: &mut impl CanonicalByteSink, value: u64) {
    sink.write(&value.to_be_bytes());
}

fn domain_digest(domain_id: DomainIdV1) -> DomainDigestV1 {
    let mut hasher = Hasher::new();
    hasher.update(DOMAIN_SEPARATOR);
    hash_len_prefixed(&mut hasher, domain_id.key().as_bytes());
    hash_u64(&mut hasher, CANDIDATE_COUNT);
    for candidate in domain_id.candidates() {
        hasher.update(&candidate.bytes());
    }
    DomainDigestV1(*hasher.finalize().as_bytes())
}

fn relation_set_digest(relations: &[RelationV1]) -> RelationSetDigestV1 {
    let mut hasher = Hasher::new();
    hasher.update(RELATION_SEPARATOR);
    hash_u64(&mut hasher, relations.len() as u64);
    for relation in relations {
        match &relation.kind {
            RelationKindV1::Applicable {
                criterion,
                adjacent,
            } => {
                hasher.update(&[1]);
                hash_len_prefixed(&mut hasher, relation.relation_id.as_str().as_bytes());
                hash_len_prefixed(&mut hasher, relation.occurrence_id.as_str().as_bytes());
                hash_len_prefixed(&mut hasher, criterion.key().as_bytes());
                hash_u64(&mut hasher, adjacent.len() as u64);
                for value in adjacent {
                    hasher.update(&value.bytes());
                }
            }
            RelationKindV1::NotApplicable { declaration } => {
                hasher.update(&[2]);
                hash_len_prefixed(&mut hasher, relation.relation_id.as_str().as_bytes());
                hash_len_prefixed(&mut hasher, relation.occurrence_id.as_str().as_bytes());
                hash_len_prefixed(&mut hasher, declaration.reason_id().as_bytes());
            }
        }
    }
    RelationSetDigestV1(*hasher.finalize().as_bytes())
}

fn evaluation_id(
    domain_digest: DomainDigestV1,
    relation_digest: RelationSetDigestV1,
    binding: &AtomicEvidenceBindingV1,
    layout: WorkLayoutV1,
    matrix_digest: &[u8; 32],
    partition: &[u8],
) -> EvaluationIdV1 {
    let mut hasher = Hasher::new();
    hasher.update(EVALUATION_SEPARATOR);
    hasher.update(domain_digest.as_bytes());
    hasher.update(relation_digest.as_bytes());
    hash_len_prefixed(&mut hasher, binding.profile_id.key().as_bytes());
    hash_len_prefixed(&mut hasher, binding.artifact_id.key().as_bytes());
    hash_len_prefixed(&mut hasher, binding.bound_id.key().as_bytes());
    hash_len_prefixed(&mut hasher, binding.proof_id.key().as_bytes());
    hasher.update(&binding.proof_sha256);
    for value in [
        layout.canonical_relations,
        layout.applicable_relations,
        layout.not_evaluated_relations,
        layout.applicable_edges,
        layout.logical_assessments,
        layout.packed_result_bytes,
    ] {
        hash_u64(&mut hasher, value);
    }
    hasher.update(matrix_digest);
    hasher.update(partition);
    EvaluationIdV1(*hasher.finalize().as_bytes())
}

#[cfg(feature = "wcag22-explicit-feasibility")]
struct SealedPackedV1 {
    packed: Vec<u8>,
    matrix_digest: [u8; 32],
    matrix_bytes: usize,
}

#[cfg(feature = "wcag22-explicit-feasibility")]
impl SealedPackedV1 {
    fn partition(&self) -> &[u8] {
        &self.packed[self.matrix_bytes..]
    }
}

#[cfg(feature = "wcag22-explicit-feasibility")]
fn seal_evaluated_v1<D: FiniteSrgb8DomainV1>(
    result: &KernelEvaluatedV1<D>,
    packed: Vec<u8>,
) -> Result<SealedPackedV1, ErrorV1> {
    let matrix_bytes = usize::try_from(result.layout.failure_matrix_bytes).map_err(|_| {
        ErrorV1::ResourceLimitExceeded {
            profile_id: result.resource_profile_id,
            dimension: ResourceDimensionV1::PackedResultBytes,
            requested: result.layout.failure_matrix_bytes,
            limit: usize::MAX as u64,
        }
    })?;
    let packed_bytes = usize::try_from(result.layout.packed_result_bytes).map_err(|_| {
        ErrorV1::ResourceLimitExceeded {
            profile_id: result.resource_profile_id,
            dimension: ResourceDimensionV1::PackedResultBytes,
            requested: result.layout.packed_result_bytes,
            limit: usize::MAX as u64,
        }
    })?;
    if packed.len() != packed_bytes || matrix_bytes > packed_bytes {
        return Err(compiler_result_error());
    }
    let (matrix, partition) = packed.split_at(matrix_bytes);
    let Some(failing_candidates) = result
        .layout
        .candidate_count
        .checked_sub(result.passing_candidates)
    else {
        return Err(compiler_result_error());
    };
    validate_variable_complete_result_v1(
        result.layout,
        matrix,
        partition,
        EvaluationProofCountersV1 {
            logical_assessments: result.observed_assessments,
            passing_candidates: result.passing_candidates,
            failing_candidates,
        },
    )
    .map_err(ErrorV1::CompilerInvariantViolation)?;
    Ok(SealedPackedV1 {
        matrix_digest: *sha256::digest(matrix).as_bytes(),
        packed,
        matrix_bytes,
    })
}

/// Sealed evidence for one complete evaluated terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationProofV1 {
    evaluation_id: EvaluationIdV1,
    resource_profile_id: ResourceProfileIdV1,
    domain_id: DomainIdV1,
    domain_digest: DomainDigestV1,
    domain_count: u64,
    domain_first: Srgb8,
    domain_last: Srgb8,
    relation_set_digest: RelationSetDigestV1,
    canonical_relations: u64,
    applicable_relations: u64,
    not_applicable_relations: u64,
    applicable_edges: u64,
    logical_assessments: u64,
    matrix_digest: [u8; 32],
    partition: [u8; 32],
    atomic_evidence: AtomicEvidenceBindingV1,
}

impl EvaluationProofV1 {
    /// Semantic result identity; resource policy is excluded from it.
    pub const fn evaluation_id(&self) -> EvaluationIdV1 {
        self.evaluation_id
    }

    /// Operational policy that admitted this computation.
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

    /// Exact number of candidates in the registered domain.
    pub const fn domain_count(&self) -> u64 {
        self.domain_count
    }

    /// First candidate in canonical domain order.
    pub const fn domain_first(&self) -> Srgb8 {
        self.domain_first
    }

    /// Last candidate in canonical domain order.
    pub const fn domain_last(&self) -> Srgb8 {
        self.domain_last
    }

    /// Canonical declared-relation digest.
    pub const fn relation_set_digest(&self) -> RelationSetDigestV1 {
        self.relation_set_digest
    }

    /// Exact canonical relation count.
    pub const fn canonical_relations(&self) -> u64 {
        self.canonical_relations
    }

    /// Exact canonical applicable-relation count.
    pub const fn applicable_relations(&self) -> u64 {
        self.applicable_relations
    }

    /// Exact canonical client-declared NotApplicable relation count.
    pub const fn not_applicable_relations(&self) -> u64 {
        self.not_applicable_relations
    }

    /// Exact flattened canonical applicable-edge count.
    pub const fn applicable_edges(&self) -> u64 {
        self.applicable_edges
    }

    /// Exact number of evaluated cells.
    pub const fn logical_assessments(&self) -> u64 {
        self.logical_assessments
    }

    /// SHA-256 of the exact packed candidate-major failure matrix.
    pub const fn matrix_digest(&self) -> &[u8; 32] {
        &self.matrix_digest
    }

    /// Exact packed candidate partition. Candidate `c` uses byte `c / 8`,
    /// mask `1 << (c % 8)`; one means feasible.
    pub const fn partition(&self) -> &[u8; 32] {
        &self.partition
    }

    /// Exact WCAG evaluator profile bound into every atomic assessment.
    pub const fn profile_id(&self) -> Wcag22ProfileIdV1 {
        self.atomic_evidence.profile_id
    }

    /// Exact finite numerical artifact used by the atomic evaluator.
    pub const fn artifact_id(&self) -> NumericalArtifactIdV2 {
        self.atomic_evidence.artifact_id
    }

    /// Exact numerical error-bound law used by the atomic evaluator.
    pub const fn bound_id(&self) -> NumericalErrorBoundIdV2 {
        self.atomic_evidence.bound_id
    }

    /// Exact complete-domain numerical proof used by the atomic evaluator.
    pub const fn proof_id(&self) -> NumericalProofIdV2 {
        self.atomic_evidence.proof_id
    }

    /// SHA-256 of the exact #284 proof-file bytes.
    pub const fn proof_sha256(&self) -> &[u8; 32] {
        &self.atomic_evidence.proof_sha256
    }
}

/// Zero-allocation view of one canonical evaluated cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssessmentV1<'a> {
    candidate: Srgb8,
    relation_id: &'a RelationId,
    adjacent: Srgb8,
    decision: Wcag22ApplicableDecisionV1,
}

impl<'a> AssessmentV1<'a> {
    /// Registered-domain candidate.
    pub const fn candidate(self) -> Srgb8 {
        self.candidate
    }

    /// Opaque canonical relation ID.
    pub const fn relation_id(self) -> &'a RelationId {
        self.relation_id
    }

    /// Exact declared adjacent colour.
    pub const fn adjacent(self) -> Srgb8 {
        self.adjacent
    }

    /// Atomic #284 decision.
    pub const fn decision(self) -> Wcag22ApplicableDecisionV1 {
        self.decision
    }
}

struct AssessmentCellV1<'a> {
    candidate_index: u64,
    relation: &'a RelationV1,
    adjacent: Srgb8,
    decision: Wcag22ApplicableDecisionV1,
}

struct AssessmentCursorV1<'a> {
    relations: &'a [RelationV1],
    matrix: &'a [u8],
    candidate_index: u64,
    candidate_count: u64,
    relation_index: usize,
    adjacent_index: usize,
    logical_index: u64,
    remaining: usize,
}

impl<'a> Iterator for AssessmentCursorV1<'a> {
    type Item = AssessmentCellV1<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        while self.candidate_index < self.candidate_count {
            if self.relation_index >= self.relations.len() {
                self.candidate_index += 1;
                self.relation_index = 0;
                self.adjacent_index = 0;
                continue;
            }
            let relation = &self.relations[self.relation_index];
            let RelationKindV1::Applicable { adjacent, .. } = &relation.kind else {
                self.relation_index += 1;
                self.adjacent_index = 0;
                continue;
            };
            if self.adjacent_index >= adjacent.len() {
                self.relation_index += 1;
                self.adjacent_index = 0;
                continue;
            }
            let adjacent_value = adjacent[self.adjacent_index];
            let decision = if packed_bit(self.matrix, self.logical_index) {
                Wcag22ApplicableDecisionV1::Fail
            } else {
                Wcag22ApplicableDecisionV1::Pass
            };
            self.adjacent_index += 1;
            self.logical_index += 1;
            self.remaining -= 1;
            return Some(AssessmentCellV1 {
                candidate_index: self.candidate_index,
                relation,
                adjacent: adjacent_value,
                decision,
            });
        }
        None
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for AssessmentCursorV1<'_> {}

struct AssessmentIter<'a> {
    cursor: AssessmentCursorV1<'a>,
}

impl<'a> Iterator for AssessmentIter<'a> {
    type Item = AssessmentV1<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let cell = self.cursor.next()?;
        let candidate = cell.candidate_index as u8;
        Some(AssessmentV1 {
            candidate: Srgb8::new([candidate; 3]),
            relation_id: &cell.relation.relation_id,
            adjacent: cell.adjacent,
            decision: cell.decision,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.cursor.size_hint()
    }
}

impl ExactSizeIterator for AssessmentIter<'_> {}

/// Complete evaluated terminal record. It has no public constructor.
#[derive(Debug)]
pub struct EvaluatedV1 {
    domain_id: DomainIdV1,
    relations: Vec<RelationV1>,
    layout: WorkLayoutV1,
    packed: Vec<u8>,
    domain_digest: DomainDigestV1,
    relation_set_digest: RelationSetDigestV1,
    evaluation_id: EvaluationIdV1,
    proof: EvaluationProofV1,
}

impl EvaluatedV1 {
    /// Exact candidate-major packed failure matrix owned by Core. For candidate
    /// `c` and flattened edge `e`, `i = cE + e`; byte `i / 8`, mask
    /// `1 << (i % 8)`; one means Fail.
    pub fn failure_matrix(&self) -> &[u8] {
        let length = self.layout.failure_matrix_bytes as usize;
        &self.packed[..length]
    }

    fn partition(&self) -> &[u8] {
        let length = self.layout.failure_matrix_bytes as usize;
        &self.packed[length..]
    }

    /// Registered domain.
    pub const fn domain_id(&self) -> DomainIdV1 {
        self.domain_id
    }

    /// Canonical domain digest.
    pub const fn domain_digest(&self) -> DomainDigestV1 {
        self.domain_digest
    }

    /// Canonical declared-relation digest.
    pub const fn relation_set_digest(&self) -> RelationSetDigestV1 {
        self.relation_set_digest
    }

    /// Semantic evaluated-result identity.
    pub const fn evaluation_id(&self) -> EvaluationIdV1 {
        self.evaluation_id
    }

    /// Sealed complete-enumeration proof.
    pub const fn proof(&self) -> &EvaluationProofV1 {
        &self.proof
    }

    /// Canonical declared graph retained exactly once by the result.
    pub fn relations(&self) -> &[RelationV1] {
        &self.relations
    }

    /// Every feasible candidate in ascending registered-domain order.
    pub fn feasible_candidates(&self) -> impl Iterator<Item = Srgb8> + '_ {
        self.domain_id.candidates().filter(move |candidate| {
            let index = u64::from(candidate.bytes()[0]);
            packed_bit(self.partition(), index)
        })
    }

    /// Every infeasible candidate in ascending registered-domain order.
    pub fn infeasible_candidates(&self) -> impl Iterator<Item = Srgb8> + '_ {
        self.domain_id.candidates().filter(move |candidate| {
            let index = u64::from(candidate.bytes()[0]);
            !packed_bit(self.partition(), index)
        })
    }

    /// Full candidate-major `256E` matrix without per-cell allocation.
    pub fn assessments(&self) -> impl ExactSizeIterator<Item = AssessmentV1<'_>> + '_ {
        AssessmentIter {
            cursor: AssessmentCursorV1 {
                relations: &self.relations,
                matrix: self.failure_matrix(),
                candidate_index: 0,
                candidate_count: self.layout.candidate_count,
                relation_index: 0,
                adjacent_index: 0,
                logical_index: 0,
                remaining: self.layout.logical_assessments as usize,
            },
        }
    }
}

/// Canonical declaration-only terminal. It carries no numerical proof.
#[derive(Debug)]
pub struct NotEvaluatedV1 {
    domain_id: DomainIdV1,
    relations: Vec<RelationV1>,
    resource_profile_id: ResourceProfileIdV1,
    domain_digest: DomainDigestV1,
    relation_set_digest: RelationSetDigestV1,
}

impl NotEvaluatedV1 {
    /// Registered domain whose request was canonicalized.
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

    /// Operational policy that admitted canonicalization.
    pub const fn resource_profile_id(&self) -> ResourceProfileIdV1 {
        self.resource_profile_id
    }

    /// Canonical declaration-only graph retained exactly once by the result.
    pub fn relations(&self) -> &[RelationV1] {
        &self.relations
    }
}

/// Exhaustive terminal algebra.
#[derive(Debug)]
#[non_exhaustive]
pub enum FeasibilityV1 {
    /// Complete evaluation with a non-empty feasible partition.
    #[non_exhaustive]
    Feasible(EvaluatedV1),
    /// Complete evaluation with an empty feasible partition.
    #[non_exhaustive]
    Infeasible(EvaluatedV1),
    /// Canonical request contained no applicable relation.
    #[non_exhaustive]
    NotEvaluated(NotEvaluatedV1),
}

impl FeasibilityV1 {
    /// Whether complete enumeration found at least one feasible candidate.
    pub const fn is_feasible(&self) -> bool {
        matches!(self, Self::Feasible(..))
    }

    /// Whether complete enumeration proved the feasible partition empty.
    pub const fn is_infeasible(&self) -> bool {
        matches!(self, Self::Infeasible(..))
    }

    /// Whether the canonical request contained no applicable relation.
    pub const fn is_not_evaluated(&self) -> bool {
        matches!(self, Self::NotEvaluated(..))
    }

    /// Borrow an evaluated payload from either evaluated terminal.
    pub const fn evaluated(&self) -> Option<&EvaluatedV1> {
        match self {
            Self::Feasible(value) | Self::Infeasible(value) => Some(value),
            Self::NotEvaluated(..) => None,
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

/// Canonicalize and exhaustively evaluate one bounded request.
pub fn evaluate(request: RequestV1) -> Result<FeasibilityV1, ErrorV1> {
    let mut evaluator = AtomicPairEvaluator::new();
    let mut storage = PackedDecisionStorage::default();
    match evaluate_with(request, &mut evaluator, &mut storage)? {
        KernelResultV1::NotEvaluated(result) => {
            debug_assert_eq!(result.layout.applicable_relations, 0);
            let domain_digest = domain_digest(result.domain);
            let relation_set_digest = relation_set_digest(&result.relations);
            Ok(FeasibilityV1::NotEvaluated(NotEvaluatedV1 {
                domain_id: result.domain,
                relations: result.relations,
                resource_profile_id: result.resource_profile_id,
                domain_digest,
                relation_set_digest,
            }))
        }
        KernelResultV1::Evaluated(result) => {
            let matrix_length =
                usize::try_from(result.layout.failure_matrix_bytes).map_err(|_| {
                    ErrorV1::ResourceLimitExceeded {
                        profile_id: result.resource_profile_id,
                        dimension: ResourceDimensionV1::PackedResultBytes,
                        requested: result.layout.failure_matrix_bytes,
                        limit: usize::MAX as u64,
                    }
                })?;
            let packed_length =
                usize::try_from(result.layout.packed_result_bytes).map_err(|_| {
                    ErrorV1::ResourceLimitExceeded {
                        profile_id: result.resource_profile_id,
                        dimension: ResourceDimensionV1::PackedResultBytes,
                        requested: result.layout.packed_result_bytes,
                        limit: usize::MAX as u64,
                    }
                })?;
            if storage.bytes.len() != packed_length {
                return Err(compiler_result_error());
            }
            let (matrix, packed_partition) = storage.bytes.split_at(matrix_length);
            let partition = result.partition;
            if packed_partition != partition {
                return Err(compiler_result_error());
            }
            let Some(failing_candidates) = CANDIDATE_COUNT.checked_sub(result.passing_candidates)
            else {
                return Err(compiler_result_error());
            };
            validate_neutral_complete_result_v1(
                result.layout,
                matrix,
                &partition,
                EvaluationProofCountersV1 {
                    logical_assessments: result.observed_assessments,
                    passing_candidates: result.passing_candidates,
                    failing_candidates,
                },
            )
            .map_err(ErrorV1::CompilerInvariantViolation)?;

            let domain_digest = domain_digest(result.domain);
            let relation_set_digest = relation_set_digest(&result.relations);
            let binding = result.atomic_evidence;
            let matrix_digest = *sha256::digest(matrix).as_bytes();
            let evaluation_id = evaluation_id(
                domain_digest,
                relation_set_digest,
                &binding,
                result.layout,
                &matrix_digest,
                &partition,
            );
            let proof = EvaluationProofV1 {
                evaluation_id,
                resource_profile_id: result.resource_profile_id,
                domain_id: result.domain,
                domain_digest,
                domain_count: result.observed_candidates,
                domain_first: Srgb8::new([0; 3]),
                domain_last: Srgb8::new([255; 3]),
                relation_set_digest,
                canonical_relations: result.layout.canonical_relations,
                applicable_relations: result.layout.applicable_relations,
                not_applicable_relations: result.layout.not_evaluated_relations,
                applicable_edges: result.layout.applicable_edges,
                logical_assessments: result.observed_assessments,
                matrix_digest,
                partition,
                atomic_evidence: binding,
            };
            let evaluated = EvaluatedV1 {
                domain_id: result.domain,
                relations: result.relations,
                layout: result.layout,
                packed: storage.bytes,
                domain_digest,
                relation_set_digest,
                evaluation_id,
                proof,
            };
            if result.passing_candidates == 0 {
                Ok(FeasibilityV1::Infeasible(evaluated))
            } else {
                Ok(FeasibilityV1::Feasible(evaluated))
            }
        }
    }
}

#[cfg(test)]
#[path = "wcag22_feasibility_tests.rs"]
mod tests;
