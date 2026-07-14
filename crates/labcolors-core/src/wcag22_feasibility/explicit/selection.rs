//! Explicit client-owned selection over one sealed feasible result (#296-B).
//!
//! Core never ranks opaque candidate IDs. The client supplies the complete
//! order; Core validates it, chooses the first member already proved feasible,
//! and rechecks only that selected row through the same proof-bound evaluator
//! used by feasibility compilation.

use core::fmt;
use std::sync::Arc;

use crate::Srgb8;
use crate::numerics::{NumericalArtifactIdV2, NumericalErrorBoundIdV2, NumericalProofIdV2};
use crate::sha256::Hasher;
use crate::wcag22::{Wcag22ApplicableDecisionV1, Wcag22ProfileIdV1};

use super::super::{
    AtomicPairEvaluator, EvaluationIdV1, EvaluatorInvariantV1, PairEvaluator, RelationId,
    RelationSetDigestV1, ResourceDimensionV1, ResourceProfileIdV1, evaluate_bound_pair,
    hash_len_prefixed, hash_u64, packed_bit,
};
use super::{CandidateId, CandidateV1, EvaluatedV1, FeasibilityV1};

const POLICY_SEPARATOR: &[u8] =
    b"labcolors/wcag22-feasibility/selection/policy/first-feasible-in-declared-order/v1\0";
const RECEIPT_SEPARATOR: &[u8] =
    b"labcolors/wcag22-feasibility/selection/receipt/selected-final-verification/v1\0";
const POLICY_KIND_KEY: &str = "first-feasible-in-declared-order-v1";
const RECEIPT_KIND_KEY: &str = "selected-final-verification-v1";

/// Opaque client-owned identity of one declared selection policy.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyId(Arc<str>);

impl PolicyId {
    /// Construct a non-empty opaque ID without normalization or interpretation.
    pub fn try_new(value: impl Into<String>) -> Result<Self, InvalidSelectionRequestV1> {
        let value = value.into();
        if value.is_empty() {
            return Err(InvalidSelectionRequestV1::EmptyPolicyId);
        }
        Ok(Self(value.into()))
    }

    /// Exact client bytes.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for PolicyId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// The sole V1 selection law: first feasible opaque ID in declared order.
#[derive(Debug)]
pub struct FirstFeasibleInDeclaredOrderV1 {
    policy_id: PolicyId,
    ordered_candidate_ids: Vec<CandidateId>,
}

impl FirstFeasibleInDeclaredOrderV1 {
    /// Construct a non-empty declared order. Aggregate validity is checked by
    /// [`select`] against the exact sealed domain and resource profile.
    pub fn try_new(
        policy_id: PolicyId,
        ordered_candidate_ids: Vec<CandidateId>,
    ) -> Result<Self, InvalidSelectionRequestV1> {
        if ordered_candidate_ids.is_empty() {
            return Err(InvalidSelectionRequestV1::EmptyCandidateOrder);
        }
        Ok(Self {
            policy_id,
            ordered_candidate_ids,
        })
    }

    /// Versioned Core-owned policy kind.
    pub const fn kind_key(&self) -> &'static str {
        POLICY_KIND_KEY
    }

    /// Opaque client policy identity.
    pub const fn policy_id(&self) -> &PolicyId {
        &self.policy_id
    }

    /// Exact declared candidate order.
    pub fn ordered_candidate_ids(&self) -> &[CandidateId] {
        &self.ordered_candidate_ids
    }
}

/// A sealed capability minted only by a [`FeasibilityV1::Feasible`] terminal.
#[derive(Debug, Clone, Copy)]
pub struct FeasibleSelectionSourceV1<'a> {
    record: &'a EvaluatedV1,
}

impl FeasibilityV1 {
    /// Mint selection authority only for a complete non-empty feasible partition.
    pub const fn selection_source(&self) -> Option<FeasibleSelectionSourceV1<'_>> {
        match self {
            Self::Feasible(record) => Some(FeasibleSelectionSourceV1 { record }),
            Self::Infeasible(..) | Self::NotEvaluated(..) => None,
        }
    }
}

/// Canonical identity of the exact policy ID and declared candidate order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PolicyDigestV1([u8; 32]);

impl PolicyDigestV1 {
    /// Exact SHA-256 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Canonical identity of one fully rechecked successful selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SelectionReceiptDigestV1([u8; 32]);

impl SelectionReceiptDigestV1 {
    /// Exact SHA-256 bytes.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Locally invalid or contradictory selection input.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InvalidSelectionRequestV1 {
    /// The opaque client policy ID was empty.
    EmptyPolicyId,
    /// No candidate order was declared.
    EmptyCandidateOrder,
    /// Checked size arithmetic overflowed.
    ArithmeticOverflow,
    /// The declared order cannot be a set subset of this finite domain.
    PolicyCardinalityExceedsDomain { requested: u64, domain: u64 },
    /// A declared opaque ID does not belong to the sealed domain.
    ForeignCandidateId { candidate_id: CandidateId },
    /// The same opaque ID occurred more than once in the declared order.
    DuplicateCandidateId { candidate_id: CandidateId },
}

impl fmt::Display for InvalidSelectionRequestV1 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyPolicyId => formatter.write_str("policy ID must be non-empty"),
            Self::EmptyCandidateOrder => {
                formatter.write_str("at least one candidate ID must be declared")
            }
            Self::ArithmeticOverflow => {
                formatter.write_str("checked selection arithmetic overflowed")
            }
            Self::PolicyCardinalityExceedsDomain { requested, domain } => write!(
                formatter,
                "selection order cardinality {requested} exceeds finite domain {domain}"
            ),
            Self::ForeignCandidateId { candidate_id } => {
                write!(
                    formatter,
                    "candidate ID {candidate_id} is outside the sealed domain"
                )
            }
            Self::DuplicateCandidateId { candidate_id } => {
                write!(formatter, "candidate ID {candidate_id} is duplicated")
            }
        }
    }
}

impl std::error::Error for InvalidSelectionRequestV1 {}

/// A selected-row recheck disagreed with sealed feasibility or its evaluator contract.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelectionIntegrityViolationV1 {
    /// The shared proof-bound atomic evaluator violated its adapter contract.
    EvaluatorContract {
        candidate_id: CandidateId,
        relation_id: RelationId,
        adjacent: Srgb8,
        violation: EvaluatorInvariantV1,
    },
    /// The final atomic verdict disagreed with the corresponding sealed cell.
    SealedDecisionMismatch {
        candidate_id: CandidateId,
        relation_id: RelationId,
        adjacent: Srgb8,
        sealed: Wcag22ApplicableDecisionV1,
        rechecked: Wcag22ApplicableDecisionV1,
    },
    /// A supposedly feasible selected row contained a non-passing sealed cell.
    SelectedRowNotPassing {
        candidate_id: CandidateId,
        relation_id: RelationId,
        adjacent: Srgb8,
    },
    /// The retained canonical graph no longer matches its sealed edge count.
    ApplicableEdgeCountMismatch { expected: u64, observed: u64 },
    /// Sealed traversal state exceeded its already-admitted integer envelope.
    SealedTraversalArithmeticOverflow,
}

/// A selection request failed closed without minting a partial receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelectionErrorV1 {
    /// Invalid or contradictory declared input.
    InvalidRequest(InvalidSelectionRequestV1),
    /// One exact preflight dimension exceeded the source profile.
    ResourceLimitExceeded {
        profile_id: ResourceProfileIdV1,
        dimension: ResourceDimensionV1,
        requested: u64,
        limit: u64,
    },
    /// The final recheck disagreed with sealed evidence.
    IntegrityViolation(SelectionIntegrityViolationV1),
}

impl From<InvalidSelectionRequestV1> for SelectionErrorV1 {
    fn from(value: InvalidSelectionRequestV1) -> Self {
        Self::InvalidRequest(value)
    }
}

impl fmt::Display for SelectionErrorV1 {
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
                "{profile_id:?} selection limit exceeded for {dimension:?}: {requested} > {limit}"
            ),
            Self::IntegrityViolation(violation) => {
                write!(formatter, "selection integrity violation: {violation:?}")
            }
        }
    }
}

impl std::error::Error for SelectionErrorV1 {}

/// Why a valid declared order produced no candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NoSelectionReasonV1 {
    /// None of the declared domain members belongs to the feasible partition.
    NoDeclaredCandidateFeasible,
}

/// Exact proof-bound final recheck identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinalRelationVerificationV1 {
    relation_set_digest: RelationSetDigestV1,
    verified_applicable_edges: u64,
    profile_id: Wcag22ProfileIdV1,
    artifact_id: NumericalArtifactIdV2,
    bound_id: NumericalErrorBoundIdV2,
    proof_id: NumericalProofIdV2,
    proof_sha256: [u8; 32],
    receipt_digest: SelectionReceiptDigestV1,
}

impl FinalRelationVerificationV1 {
    /// Canonical declared relation-set identity.
    pub const fn relation_set_digest(&self) -> RelationSetDigestV1 {
        self.relation_set_digest
    }

    /// Exact number of canonical applicable edges actually rechecked.
    pub const fn verified_applicable_edges(&self) -> u64 {
        self.verified_applicable_edges
    }

    /// Exact WCAG evaluator profile bound into every rechecked edge.
    pub const fn profile_id(&self) -> Wcag22ProfileIdV1 {
        self.profile_id
    }

    /// Exact finite numerical artifact used by every rechecked edge.
    pub const fn artifact_id(&self) -> NumericalArtifactIdV2 {
        self.artifact_id
    }

    /// Exact numerical error-bound law used by every rechecked edge.
    pub const fn bound_id(&self) -> NumericalErrorBoundIdV2 {
        self.bound_id
    }

    /// Exact complete-domain proof used by every rechecked edge.
    pub const fn proof_id(&self) -> NumericalProofIdV2 {
        self.proof_id
    }

    /// SHA-256 of the exact registered numerical proof-file bytes.
    pub const fn proof_sha256(&self) -> &[u8; 32] {
        &self.proof_sha256
    }

    /// Content identity shared with the selection-proof projection.
    pub const fn receipt_digest(&self) -> SelectionReceiptDigestV1 {
        self.receipt_digest
    }
}

/// A selected candidate and its sealed final receipt. Fields are private so a
/// downstream caller cannot wrap an arbitrary payload as a Core outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedV1 {
    candidate: CandidateV1,
    policy_id: PolicyId,
    policy_digest: PolicyDigestV1,
    evaluation_id: EvaluationIdV1,
    selected_policy_ordinal: u64,
    final_verification: FinalRelationVerificationV1,
}

impl SelectedV1 {
    /// Exact selected opaque ID and final physical sRGB8 bytes.
    pub const fn candidate(&self) -> &CandidateV1 {
        &self.candidate
    }

    /// Opaque client policy identity.
    pub const fn policy_id(&self) -> &PolicyId {
        &self.policy_id
    }

    /// Canonical declared policy identity.
    pub const fn policy_digest(&self) -> PolicyDigestV1 {
        self.policy_digest
    }

    /// Sealed feasibility result identity used as source.
    pub const fn evaluation_id(&self) -> EvaluationIdV1 {
        self.evaluation_id
    }

    /// Borrow the sealed selection proof projection.
    pub const fn proof(&self) -> SelectionProofV1<'_> {
        SelectionProofV1 { selected: self }
    }

    /// Borrow the exact final relation recheck projection.
    pub const fn final_verification(&self) -> &FinalRelationVerificationV1 {
        &self.final_verification
    }

    /// One content identity shared by both public evidence projections.
    pub const fn receipt_digest(&self) -> SelectionReceiptDigestV1 {
        self.final_verification.receipt_digest
    }
}

/// Borrowed sealed proof projection for one selected result.
#[derive(Debug, Clone, Copy)]
pub struct SelectionProofV1<'a> {
    selected: &'a SelectedV1,
}

impl SelectionProofV1<'_> {
    /// Zero-based position in the exact client-declared order.
    pub const fn selected_policy_ordinal(&self) -> u64 {
        self.selected.selected_policy_ordinal
    }

    /// Canonical declared policy identity.
    pub const fn policy_digest(&self) -> PolicyDigestV1 {
        self.selected.policy_digest
    }

    /// Sealed feasibility result identity used as source.
    pub const fn evaluation_id(&self) -> EvaluationIdV1 {
        self.selected.evaluation_id
    }

    /// Same receipt identity exposed by final verification.
    pub const fn receipt_digest(&self) -> SelectionReceiptDigestV1 {
        self.selected.final_verification.receipt_digest
    }
}

/// A valid policy whose declared domain members were all infeasible.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoSelectionV1 {
    reason: NoSelectionReasonV1,
    policy_id: PolicyId,
    policy_digest: PolicyDigestV1,
    evaluation_id: EvaluationIdV1,
}

impl NoSelectionV1 {
    /// Exact exhaustive reason.
    pub const fn reason(&self) -> NoSelectionReasonV1 {
        self.reason
    }

    /// Opaque client policy identity.
    pub const fn policy_id(&self) -> &PolicyId {
        &self.policy_id
    }

    /// Canonical declared policy identity.
    pub const fn policy_digest(&self) -> PolicyDigestV1 {
        self.policy_digest
    }

    /// Sealed feasibility result identity used as source.
    pub const fn evaluation_id(&self) -> EvaluationIdV1 {
        self.evaluation_id
    }
}

/// Exhaustive V1 outcome. Variant construction is reserved to Core so an
/// arbitrary payload cannot be promoted into a certified result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionOutcomeV1 {
    /// The first feasible member in declared order, with final evidence.
    #[non_exhaustive]
    Selected { selected: SelectedV1 },
    /// The complete declared order contained no feasible member.
    #[non_exhaustive]
    NoSelection { no_selection: NoSelectionV1 },
}

fn policy_digest(policy: &FirstFeasibleInDeclaredOrderV1, entries: u64) -> PolicyDigestV1 {
    let mut hasher = Hasher::new();
    hasher.update(POLICY_SEPARATOR);
    hash_len_prefixed(&mut hasher, POLICY_KIND_KEY.as_bytes());
    hash_len_prefixed(&mut hasher, policy.policy_id.as_str().as_bytes());
    hash_u64(&mut hasher, entries);
    for candidate_id in &policy.ordered_candidate_ids {
        hash_len_prefixed(&mut hasher, candidate_id.as_str().as_bytes());
    }
    PolicyDigestV1(*hasher.finalize().as_bytes())
}

fn receipt_hasher(
    record: &EvaluatedV1,
    policy_digest: PolicyDigestV1,
    selected_policy_ordinal: u64,
    selected: &CandidateV1,
) -> Hasher {
    let binding = &record.proof.atomic_evidence;
    let mut hasher = Hasher::new();
    hasher.update(RECEIPT_SEPARATOR);
    hash_len_prefixed(&mut hasher, RECEIPT_KIND_KEY.as_bytes());
    hasher.update(record.evaluation_id.as_bytes());
    hasher.update(record.relation_set_digest.as_bytes());
    hasher.update(policy_digest.as_bytes());
    hash_u64(&mut hasher, selected_policy_ordinal);
    hash_len_prefixed(&mut hasher, selected.candidate_id.as_str().as_bytes());
    hasher.update(&selected.emitted.bytes());
    hash_len_prefixed(&mut hasher, binding.profile_id.key().as_bytes());
    hash_len_prefixed(&mut hasher, binding.artifact_id.key().as_bytes());
    hash_len_prefixed(&mut hasher, binding.bound_id.key().as_bytes());
    hash_len_prefixed(&mut hasher, binding.proof_id.key().as_bytes());
    hasher.update(&binding.proof_sha256);
    hash_u64(&mut hasher, record.layout.applicable_edges);
    hasher
}

trait ReceiptSink {
    fn write(&mut self, bytes: &[u8]);
}

impl ReceiptSink for Hasher {
    fn write(&mut self, bytes: &[u8]) {
        self.update(bytes);
    }
}

fn receipt_sink_u64(sink: &mut impl ReceiptSink, value: u64) {
    sink.write(&value.to_le_bytes());
}

fn receipt_sink_len_prefixed(sink: &mut impl ReceiptSink, bytes: &[u8]) {
    receipt_sink_u64(sink, bytes.len() as u64);
    sink.write(bytes);
}

fn stream_receipt_edge(
    sink: &mut impl ReceiptSink,
    edge_ordinal: u64,
    relation_id: &[u8],
    criterion_key: &[u8],
    foreground: Srgb8,
    background: Srgb8,
) {
    receipt_sink_u64(sink, edge_ordinal);
    receipt_sink_len_prefixed(sink, relation_id);
    receipt_sink_len_prefixed(sink, criterion_key);
    sink.write(&foreground.bytes());
    sink.write(&background.bytes());
    sink.write(&[1]);
}

fn preflight(
    record: &EvaluatedV1,
    policy: &FirstFeasibleInDeclaredOrderV1,
) -> Result<u64, SelectionErrorV1> {
    let mut bytes = u64::try_from(policy.policy_id.as_str().len())
        .map_err(|_| InvalidSelectionRequestV1::ArithmeticOverflow)?;
    for candidate_id in &policy.ordered_candidate_ids {
        let length = u64::try_from(candidate_id.as_str().len())
            .map_err(|_| InvalidSelectionRequestV1::ArithmeticOverflow)?;
        bytes = bytes
            .checked_add(length)
            .ok_or(InvalidSelectionRequestV1::ArithmeticOverflow)?;
    }
    let profile_id = record.proof.resource_profile_id;
    let dimension = ResourceDimensionV1::OpaqueUtf8Bytes;
    let limit = profile_id.limit(dimension);
    if bytes > limit {
        return Err(SelectionErrorV1::ResourceLimitExceeded {
            profile_id,
            dimension,
            requested: bytes,
            limit,
        });
    }

    let entries = u64::try_from(policy.ordered_candidate_ids.len())
        .map_err(|_| InvalidSelectionRequestV1::ArithmeticOverflow)?;
    let domain = record.domain.candidate_count;
    if entries > domain {
        return Err(InvalidSelectionRequestV1::PolicyCardinalityExceedsDomain {
            requested: entries,
            domain,
        }
        .into());
    }
    Ok(entries)
}

fn find_canonical_candidate(record: &EvaluatedV1, id: &CandidateId) -> Option<usize> {
    record
        .candidates
        .binary_search_by(|candidate| {
            candidate
                .candidate_id
                .as_str()
                .as_bytes()
                .cmp(id.as_str().as_bytes())
        })
        .ok()
}

fn select_with<E: PairEvaluator>(
    source: FeasibleSelectionSourceV1<'_>,
    mut policy: FirstFeasibleInDeclaredOrderV1,
    evaluator: &mut E,
) -> Result<SelectionOutcomeV1, SelectionErrorV1> {
    let record = source.record;
    let entries = preflight(record, &policy)?;
    let digest = policy_digest(&policy, entries);

    let mut selected = None;
    for (ordinal, id) in policy.ordered_candidate_ids.iter().enumerate() {
        let Some(candidate_index) = find_canonical_candidate(record, id) else {
            return Err(InvalidSelectionRequestV1::ForeignCandidateId {
                candidate_id: id.clone(),
            }
            .into());
        };
        let canonical_ordinal = u64::try_from(candidate_index).map_err(|_| {
            SelectionErrorV1::IntegrityViolation(
                SelectionIntegrityViolationV1::SealedTraversalArithmeticOverflow,
            )
        })?;
        if selected.is_none() && packed_bit(record.partition(), canonical_ordinal) {
            let policy_ordinal = u64::try_from(ordinal).map_err(|_| {
                SelectionErrorV1::IntegrityViolation(
                    SelectionIntegrityViolationV1::SealedTraversalArithmeticOverflow,
                )
            })?;
            selected = Some((policy_ordinal, candidate_index));
        }
    }

    policy
        .ordered_candidate_ids
        .sort_unstable_by(|left, right| left.as_str().as_bytes().cmp(right.as_str().as_bytes()));
    if let Some(pair) = policy
        .ordered_candidate_ids
        .windows(2)
        .find(|pair| pair[0] == pair[1])
    {
        return Err(InvalidSelectionRequestV1::DuplicateCandidateId {
            candidate_id: pair[0].clone(),
        }
        .into());
    }

    let Some((selected_policy_ordinal, candidate_index)) = selected else {
        return Ok(SelectionOutcomeV1::NoSelection {
            no_selection: NoSelectionV1 {
                reason: NoSelectionReasonV1::NoDeclaredCandidateFeasible,
                policy_id: policy.policy_id,
                policy_digest: digest,
                evaluation_id: record.evaluation_id,
            },
        });
    };

    let candidate = &record.candidates[candidate_index];
    let mut receipt = receipt_hasher(record, digest, selected_policy_ordinal, candidate);
    let mut edge_ordinal = 0_u64;
    for relation in &record.relations {
        let Some((criterion, adjacent)) = relation.as_applicable() else {
            continue;
        };
        for adjacent in adjacent.iter().copied() {
            let decision = evaluate_bound_pair(
                evaluator,
                &record.proof.atomic_evidence,
                candidate.emitted,
                adjacent,
                criterion,
            )
            .map_err(|violation| {
                SelectionErrorV1::IntegrityViolation(
                    SelectionIntegrityViolationV1::EvaluatorContract {
                        candidate_id: candidate.candidate_id.clone(),
                        relation_id: relation.relation_id().clone(),
                        adjacent,
                        violation,
                    },
                )
            })?;
            let canonical_ordinal = u64::try_from(candidate_index).map_err(|_| {
                SelectionErrorV1::IntegrityViolation(
                    SelectionIntegrityViolationV1::SealedTraversalArithmeticOverflow,
                )
            })?;
            let logical_index = canonical_ordinal
                .checked_mul(record.layout.applicable_edges)
                .and_then(|base| base.checked_add(edge_ordinal))
                .ok_or({
                    SelectionErrorV1::IntegrityViolation(
                        SelectionIntegrityViolationV1::SealedTraversalArithmeticOverflow,
                    )
                })?;
            let sealed = if packed_bit(record.failure_matrix(), logical_index) {
                Wcag22ApplicableDecisionV1::Fail
            } else {
                Wcag22ApplicableDecisionV1::Pass
            };
            if decision != sealed {
                return Err(SelectionErrorV1::IntegrityViolation(
                    SelectionIntegrityViolationV1::SealedDecisionMismatch {
                        candidate_id: candidate.candidate_id.clone(),
                        relation_id: relation.relation_id().clone(),
                        adjacent,
                        sealed,
                        rechecked: decision,
                    },
                ));
            }
            if sealed != Wcag22ApplicableDecisionV1::Pass {
                return Err(SelectionErrorV1::IntegrityViolation(
                    SelectionIntegrityViolationV1::SelectedRowNotPassing {
                        candidate_id: candidate.candidate_id.clone(),
                        relation_id: relation.relation_id().clone(),
                        adjacent,
                    },
                ));
            }

            stream_receipt_edge(
                &mut receipt,
                edge_ordinal,
                relation.relation_id().as_str().as_bytes(),
                criterion.key().as_bytes(),
                candidate.emitted,
                adjacent,
            );
            edge_ordinal = edge_ordinal.checked_add(1).ok_or({
                SelectionErrorV1::IntegrityViolation(
                    SelectionIntegrityViolationV1::SealedTraversalArithmeticOverflow,
                )
            })?;
        }
    }
    if edge_ordinal != record.layout.applicable_edges {
        return Err(SelectionErrorV1::IntegrityViolation(
            SelectionIntegrityViolationV1::ApplicableEdgeCountMismatch {
                expected: record.layout.applicable_edges,
                observed: edge_ordinal,
            },
        ));
    }
    let receipt_digest = SelectionReceiptDigestV1(*receipt.finalize().as_bytes());
    let binding = &record.proof.atomic_evidence;
    let final_verification = FinalRelationVerificationV1 {
        relation_set_digest: record.relation_set_digest,
        verified_applicable_edges: edge_ordinal,
        profile_id: binding.profile_id,
        artifact_id: binding.artifact_id,
        bound_id: binding.bound_id,
        proof_id: binding.proof_id,
        proof_sha256: binding.proof_sha256,
        receipt_digest,
    };
    Ok(SelectionOutcomeV1::Selected {
        selected: SelectedV1 {
            candidate: candidate.clone(),
            policy_id: policy.policy_id,
            policy_digest: digest,
            evaluation_id: record.evaluation_id,
            selected_policy_ordinal,
            final_verification,
        },
    })
}

/// Select the first feasible declared opaque ID and recheck its complete row.
pub fn select(
    source: FeasibleSelectionSourceV1<'_>,
    policy: FirstFeasibleInDeclaredOrderV1,
) -> Result<SelectionOutcomeV1, SelectionErrorV1> {
    let mut evaluator = AtomicPairEvaluator::new();
    select_with(source, policy, &mut evaluator)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wcag22::{Wcag22CriterionV1, Wcag22EvaluationErrorV1};
    use crate::wcag22_feasibility::explicit::{
        DomainRequestV1, RequestV1, evaluate as evaluate_explicit,
    };
    use crate::wcag22_feasibility::{
        AtomicEvidenceBindingV1, OccurrenceId, PairEvaluationV1, RelationV1,
        expected_atomic_evidence_binding_v1,
    };

    fn candidate(id: &str, value: u8) -> CandidateV1 {
        CandidateV1::new(
            CandidateId::try_new(id).expect("test candidate ID is non-empty"),
            Srgb8::new([value; 3]),
        )
    }

    fn feasible(candidates: Vec<CandidateV1>, adjacent: Vec<Srgb8>) -> FeasibilityV1 {
        let relation = RelationV1::applicable(
            RelationId::try_new("relation").unwrap(),
            OccurrenceId::try_new("occurrence").unwrap(),
            Wcag22CriterionV1::Sc143TextDefault,
            adjacent,
        )
        .unwrap();
        evaluate_explicit(
            RequestV1::try_new(
                DomainRequestV1::try_new(candidates).unwrap(),
                vec![relation],
                ResourceProfileIdV1::Compile,
            )
            .unwrap(),
        )
        .expect("test fixture compiles")
    }

    fn policy(id: &str, order: &[&str]) -> FirstFeasibleInDeclaredOrderV1 {
        FirstFeasibleInDeclaredOrderV1::try_new(
            PolicyId::try_new(id).unwrap(),
            order
                .iter()
                .map(|id| CandidateId::try_new(*id).unwrap())
                .collect(),
        )
        .unwrap()
    }

    #[derive(Debug, Clone, Copy)]
    enum ProbeMode {
        Pass,
        FailAt(u64),
        InputMismatchAt(u64),
    }

    struct ProbeEvaluator {
        calls: u64,
        mode: ProbeMode,
        evidence: AtomicEvidenceBindingV1,
    }

    impl ProbeEvaluator {
        fn new(mode: ProbeMode) -> Self {
            Self {
                calls: 0,
                mode,
                evidence: expected_atomic_evidence_binding_v1().unwrap(),
            }
        }
    }

    impl PairEvaluator for ProbeEvaluator {
        fn evaluate_pair(
            &mut self,
            candidate: Srgb8,
            adjacent: Srgb8,
            criterion: Wcag22CriterionV1,
        ) -> Result<PairEvaluationV1, Wcag22EvaluationErrorV1> {
            self.calls += 1;
            let decision = if matches!(self.mode, ProbeMode::FailAt(call) if call == self.calls) {
                Wcag22ApplicableDecisionV1::Fail
            } else {
                Wcag22ApplicableDecisionV1::Pass
            };
            let mut foreground = candidate.bytes();
            if matches!(self.mode, ProbeMode::InputMismatchAt(call) if call == self.calls) {
                foreground[0] ^= 1;
            }
            Ok(PairEvaluationV1::Evaluated {
                foreground,
                background: adjacent.bytes(),
                criterion,
                decision,
                evidence: self.evidence.clone(),
            })
        }
    }

    #[derive(Default)]
    struct CountingSink {
        bytes: u64,
    }

    impl ReceiptSink for CountingSink {
        fn write(&mut self, bytes: &[u8]) {
            self.bytes += u64::try_from(bytes.len()).unwrap();
        }
    }

    fn relation_receipt_bytes(relation_id: &[u8], edges: u64) -> u64 {
        let mut sink = CountingSink::default();
        for edge_ordinal in 0..edges {
            stream_receipt_edge(
                &mut sink,
                edge_ordinal,
                relation_id,
                Wcag22CriterionV1::Sc143TextDefault.key().as_bytes(),
                Srgb8::new([255; 3]),
                Srgb8::new([0; 3]),
            );
        }
        sink.bytes
    }

    #[test]
    fn relation_identity_is_streamed_once_not_once_per_edge() {
        let edges = ResourceProfileIdV1::Compile.limit(ResourceDimensionV1::ApplicableEdges);
        let opaque_bytes = ResourceProfileIdV1::Compile.limit(ResourceDimensionV1::OpaqueUtf8Bytes);
        let long_len = usize::try_from(opaque_bytes - 2).unwrap();
        let short_id = [b'r'];
        let long_id = vec![b'r'; long_len];

        let short = relation_receipt_bytes(&short_id, edges);
        let long = relation_receipt_bytes(&long_id, edges);
        assert_eq!(
            long - short,
            u64::try_from(long_len - short_id.len()).unwrap()
        );
    }

    #[test]
    fn complete_request_validation_and_no_selection_make_zero_final_evaluator_calls() {
        let source = feasible(
            vec![
                candidate("first", 255),
                candidate("second", 254),
                candidate("third", 253),
            ],
            vec![Srgb8::new([0; 3])],
        );
        let source = source.selection_source().unwrap();
        let mut evaluator = ProbeEvaluator::new(ProbeMode::Pass);

        for invalid in [
            policy("invalid", &["first", "foreign"]),
            policy("invalid", &["first", "second", "first"]),
        ] {
            select_with(source, invalid, &mut evaluator)
                .expect_err("an invalid tail cannot mint a partial selection");
        }
        select_with(
            source,
            policy(&"p".repeat(65_530), &["first", "foreign"]),
            &mut evaluator,
        )
        .expect_err("resource preflight precedes semantic lookup");
        assert_eq!(evaluator.calls, 0);

        let mixed = feasible(
            vec![candidate("fail", 0), candidate("pass", 255)],
            vec![Srgb8::new([0; 3])],
        );
        let outcome = select_with(
            mixed.selection_source().unwrap(),
            policy("no-selection", &["fail"]),
            &mut evaluator,
        )
        .unwrap();
        assert!(matches!(outcome, SelectionOutcomeV1::NoSelection { .. }));
        assert_eq!(evaluator.calls, 0);
    }

    #[test]
    fn successful_final_recheck_makes_exactly_one_call_per_applicable_edge() {
        let source = feasible(
            vec![candidate("selected", 255)],
            vec![Srgb8::new([0; 3]), Srgb8::new([1; 3]), Srgb8::new([2; 3])],
        );
        let mut evaluator = ProbeEvaluator::new(ProbeMode::Pass);
        let outcome = select_with(
            source.selection_source().unwrap(),
            policy("exact-e", &["selected"]),
            &mut evaluator,
        )
        .unwrap();

        assert_eq!(evaluator.calls, 3);
        let SelectionOutcomeV1::Selected { selected, .. } = outcome else {
            panic!("feasible singleton must be selected");
        };
        assert_eq!(selected.final_verification().verified_applicable_edges(), 3);
    }

    #[test]
    fn a_fault_on_each_edge_fails_closed_at_that_exact_edge() {
        let source = feasible(
            vec![candidate("selected", 255)],
            vec![Srgb8::new([0; 3]), Srgb8::new([1; 3]), Srgb8::new([2; 3])],
        );

        for fault_at in 1..=3 {
            let mut verdict_fault = ProbeEvaluator::new(ProbeMode::FailAt(fault_at));
            let error = select_with(
                source.selection_source().unwrap(),
                policy("verdict-fault", &["selected"]),
                &mut verdict_fault,
            )
            .expect_err("a verdict mismatch on any edge cannot mint a receipt");
            assert!(matches!(
                error,
                SelectionErrorV1::IntegrityViolation(
                    SelectionIntegrityViolationV1::SealedDecisionMismatch { .. }
                )
            ));
            assert_eq!(verdict_fault.calls, fault_at);

            let mut input_fault = ProbeEvaluator::new(ProbeMode::InputMismatchAt(fault_at));
            let error = select_with(
                source.selection_source().unwrap(),
                policy("input-fault", &["selected"]),
                &mut input_fault,
            )
            .expect_err("an adapter mismatch on any edge cannot mint a receipt");
            assert!(matches!(
                error,
                SelectionErrorV1::IntegrityViolation(
                    SelectionIntegrityViolationV1::EvaluatorContract {
                        violation: EvaluatorInvariantV1::InputMismatch,
                        ..
                    }
                )
            ));
            assert_eq!(input_fault.calls, fault_at);
        }
    }

    #[test]
    fn selected_row_recheck_reads_the_exact_last_lsb0_cell_across_a_byte_boundary() {
        let mut source = feasible(
            vec![
                candidate("a", 255),
                candidate("b", 255),
                candidate("z", 255),
            ],
            vec![Srgb8::new([0; 3]), Srgb8::new([1; 3]), Srgb8::new([2; 3])],
        );
        let record = match &mut source {
            FeasibilityV1::Feasible(record) => record,
            _ => panic!("the fixture must mint a feasible terminal"),
        };
        // Candidate `z` is canonical row 2 and E=3, so its last cell is
        // logical bit 8: the first bit of the second matrix byte. Corrupting
        // only that sealed cell makes a constant-Pass or wrong-row reader
        // survive neither the verdict nor the selected-row postcondition.
        record.packed[1] |= 1;

        let mut pass = ProbeEvaluator::new(ProbeMode::Pass);
        let error = select_with(
            source.selection_source().unwrap(),
            policy("sealed-last-cell", &["z"]),
            &mut pass,
        )
        .expect_err("the recheck must read the corrupted last sealed cell");
        assert!(matches!(
            error,
            SelectionErrorV1::IntegrityViolation(
                SelectionIntegrityViolationV1::SealedDecisionMismatch {
                    sealed: Wcag22ApplicableDecisionV1::Fail,
                    rechecked: Wcag22ApplicableDecisionV1::Pass,
                    ..
                }
            )
        ));
        assert_eq!(pass.calls, 3);

        let mut matching_fail = ProbeEvaluator::new(ProbeMode::FailAt(3));
        let error = select_with(
            source.selection_source().unwrap(),
            policy("sealed-last-cell", &["z"]),
            &mut matching_fail,
        )
        .expect_err("a matching Fail still contradicts a feasible partition");
        assert!(matches!(
            error,
            SelectionErrorV1::IntegrityViolation(
                SelectionIntegrityViolationV1::SelectedRowNotPassing { .. }
            )
        ));
        assert_eq!(matching_fail.calls, 3);
    }
}
