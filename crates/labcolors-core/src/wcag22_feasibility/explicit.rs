//! Client-declared finite sRGB8 feasibility (#296-A).

use core::fmt;
use std::sync::Arc;

use crate::Srgb8;
use crate::numerics::{NumericalArtifactIdV2, NumericalErrorBoundIdV2, NumericalProofIdV2};
use crate::sha256::Hasher;
use crate::wcag22::{Wcag22ApplicableDecisionV1, Wcag22ProfileIdV1};

use super::{
    AssessmentCellV1, AssessmentCursorV1, AtomicEvidenceBindingV1, AtomicPairEvaluator,
    DomainDigestV1, ErrorV1, EvaluationIdV1, FiniteSrgb8DomainV1, InvalidRequestV1,
    KernelRequestV1, KernelResultV1, PackedDecisionStorage, RelationId, RelationSetDigestV1,
    RelationV1, ResourceProfileIdV1, VariablePackingV1, WorkLayoutV1, evaluate_domain_with,
    hash_len_prefixed, hash_u64, packed_bit, relation_set_digest, seal_evaluated_v1,
};

const DOMAIN_SEPARATOR: &[u8] = b"labcolors/wcag22-feasibility/domain/explicit-srgb8-set/v1\0";
const EVALUATION_SEPARATOR: &[u8] =
    b"labcolors/wcag22-feasibility/evaluation/explicit-srgb8-set/v1\0";

/// Opaque client-owned candidate identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CandidateId(Arc<str>);

impl CandidateId {
    /// Construct a non-empty opaque ID. Core uses the exact UTF-8 bytes and
    /// never normalizes or interprets the text.
    pub fn try_new(value: impl Into<String>) -> Result<Self, InvalidRequestV1> {
        let value = value.into();
        if value.is_empty() {
            return Err(InvalidRequestV1::EmptyCandidateId);
        }
        Ok(Self(value.into()))
    }

    /// Exact client bytes used for canonical ordering and identity.
    pub fn as_str(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for CandidateId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// One explicit physical candidate. The ID remains opaque to Core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateV1 {
    candidate_id: CandidateId,
    emitted: Srgb8,
}

impl CandidateV1 {
    /// Bind an opaque client ID to one exact final encoded-sRGB8 value.
    pub const fn new(candidate_id: CandidateId, emitted: Srgb8) -> Self {
        Self {
            candidate_id,
            emitted,
        }
    }

    /// Opaque client-owned identity.
    pub const fn candidate_id(&self) -> &CandidateId {
        &self.candidate_id
    }

    /// Exact final physical bytes evaluated by Core.
    pub const fn emitted(&self) -> Srgb8 {
        self.emitted
    }
}

/// Owned client-declared finite domain before Core canonicalization.
#[derive(Debug)]
pub struct DomainRequestV1 {
    candidates: Vec<CandidateV1>,
}

impl DomainRequestV1 {
    /// Construct a non-empty declared set. Duplicate IDs are rejected at the
    /// compiler boundary after raw resource preflight and before evaluation.
    pub fn try_new(candidates: Vec<CandidateV1>) -> Result<Self, InvalidRequestV1> {
        if candidates.is_empty() {
            return Err(InvalidRequestV1::EmptyCandidates);
        }
        Ok(Self { candidates })
    }

    fn into_candidates(self) -> Vec<CandidateV1> {
        self.candidates
    }
}

impl FiniteSrgb8DomainV1 for DomainRequestV1 {
    type Packing = VariablePackingV1;

    fn raw_opaque_utf8_bytes(&self) -> Result<u64, InvalidRequestV1> {
        let mut bytes = 0_u64;
        for candidate in &self.candidates {
            let length = u64::try_from(candidate.candidate_id.as_str().len())
                .map_err(|_| InvalidRequestV1::ArithmeticOverflow)?;
            bytes = bytes
                .checked_add(length)
                .ok_or(InvalidRequestV1::ArithmeticOverflow)?;
        }
        Ok(bytes)
    }

    fn canonicalize(&mut self) -> Result<(), InvalidRequestV1> {
        self.candidates.sort_unstable_by(|left, right| {
            left.candidate_id
                .as_str()
                .as_bytes()
                .cmp(right.candidate_id.as_str().as_bytes())
        });
        if let Some(pair) = self
            .candidates
            .windows(2)
            .find(|pair| pair[0].candidate_id == pair[1].candidate_id)
        {
            return Err(InvalidRequestV1::DuplicateCandidateId {
                candidate_id: pair[0].candidate_id.clone(),
            });
        }
        Ok(())
    }

    fn candidates(&self) -> impl ExactSizeIterator<Item = Srgb8> + '_ {
        self.candidates.iter().map(CandidateV1::emitted)
    }
}

/// Core-owned versioned kind of declared finite domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DomainKindV1 {
    /// Canonical set of opaque IDs bound to exact final sRGB8 values.
    ExplicitSrgb8Set,
}

impl DomainKindV1 {
    /// Stable semantic key.
    pub const fn key(self) -> &'static str {
        match self {
            Self::ExplicitSrgb8Set => "explicit-srgb8-set-v1",
        }
    }
}

/// Owned bounded-compilation request for one explicit finite domain.
#[derive(Debug)]
pub struct RequestV1 {
    domain: DomainRequestV1,
    relations: Vec<RelationV1>,
    resource_profile_id: ResourceProfileIdV1,
}

impl RequestV1 {
    /// Construct a locally well-formed request. Aggregate resource bounds and
    /// duplicate candidate IDs are checked by [`evaluate`] before any pair is
    /// evaluated.
    pub fn try_new(
        domain: DomainRequestV1,
        relations: Vec<RelationV1>,
        resource_profile_id: ResourceProfileIdV1,
    ) -> Result<Self, InvalidRequestV1> {
        if relations.is_empty() {
            return Err(InvalidRequestV1::EmptyRelations);
        }
        Ok(Self {
            domain,
            relations,
            resource_profile_id,
        })
    }
}

/// Domain-neutral evidence descriptor: kind, canonical content and exact
/// finite cardinality. It deliberately has no neutral-only first/last fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DomainDescriptorV1 {
    kind: DomainKindV1,
    digest: DomainDigestV1,
    candidate_count: u64,
}

impl DomainDescriptorV1 {
    /// Versioned domain kind.
    pub const fn kind(&self) -> DomainKindV1 {
        self.kind
    }

    /// Canonical `(candidate ID, emitted sRGB8)` content digest.
    pub const fn digest(&self) -> DomainDigestV1 {
        self.digest
    }

    /// Exact number of exhaustively evaluated declared candidates.
    pub const fn candidate_count(&self) -> u64 {
        self.candidate_count
    }
}

fn domain_digest(domain: &DomainRequestV1, candidate_count: u64) -> DomainDigestV1 {
    let mut hasher = Hasher::new();
    hasher.update(DOMAIN_SEPARATOR);
    hash_len_prefixed(&mut hasher, DomainKindV1::ExplicitSrgb8Set.key().as_bytes());
    hash_u64(&mut hasher, candidate_count);
    for candidate in &domain.candidates {
        hash_len_prefixed(&mut hasher, candidate.candidate_id.as_str().as_bytes());
        hasher.update(&candidate.emitted.bytes());
    }
    DomainDigestV1(*hasher.finalize().as_bytes())
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
        layout.candidate_count,
        layout.logical_assessments,
        layout.failure_matrix_bytes,
        layout.partition_bytes,
        layout.packed_result_bytes,
    ] {
        hash_u64(&mut hasher, value);
    }
    hasher.update(matrix_digest);
    hash_len_prefixed(&mut hasher, partition);
    EvaluationIdV1(*hasher.finalize().as_bytes())
}

#[derive(Debug)]
struct EvaluationProofDataV1 {
    resource_profile_id: ResourceProfileIdV1,
    canonical_relations: u64,
    applicable_relations: u64,
    not_applicable_relations: u64,
    applicable_edges: u64,
    logical_assessments: u64,
    matrix_digest: [u8; 32],
    atomic_evidence: AtomicEvidenceBindingV1,
}

/// Borrowed sealed view of one complete explicit-domain proof.
#[derive(Debug, Clone, Copy)]
pub struct EvaluationProofV1<'a> {
    record: &'a EvaluatedV1,
}

impl EvaluationProofV1<'_> {
    /// Semantic result identity; resource policy is excluded from it.
    pub const fn evaluation_id(&self) -> EvaluationIdV1 {
        self.record.evaluation_id
    }

    /// Operational policy that admitted this computation.
    pub const fn resource_profile_id(&self) -> ResourceProfileIdV1 {
        self.record.proof.resource_profile_id
    }

    /// Domain-neutral explicit-set descriptor.
    pub const fn domain(&self) -> &DomainDescriptorV1 {
        &self.record.domain
    }

    /// Canonical declared-relation digest.
    pub const fn relation_set_digest(&self) -> RelationSetDigestV1 {
        self.record.relation_set_digest
    }

    /// Exact canonical relation count.
    pub const fn canonical_relations(&self) -> u64 {
        self.record.proof.canonical_relations
    }

    /// Exact applicable relation count.
    pub const fn applicable_relations(&self) -> u64 {
        self.record.proof.applicable_relations
    }

    /// Exact client-declared NotApplicable relation count.
    pub const fn not_applicable_relations(&self) -> u64 {
        self.record.proof.not_applicable_relations
    }

    /// Exact flattened canonical edge count.
    pub const fn applicable_edges(&self) -> u64 {
        self.record.proof.applicable_edges
    }

    /// Exact number of evaluated candidate-edge cells.
    pub const fn logical_assessments(&self) -> u64 {
        self.record.proof.logical_assessments
    }

    /// SHA-256 of the exact packed candidate-major failure matrix.
    pub const fn matrix_digest(&self) -> &[u8; 32] {
        &self.record.proof.matrix_digest
    }

    /// Exact variable-width feasible-candidate partition, LSB0 by canonical
    /// candidate index.
    pub fn partition(&self) -> &[u8] {
        self.record.partition()
    }

    /// Exact WCAG evaluator profile bound into every atomic assessment.
    pub const fn profile_id(&self) -> Wcag22ProfileIdV1 {
        self.record.proof.atomic_evidence.profile_id
    }

    /// Exact finite numerical artifact used by the atomic evaluator.
    pub const fn artifact_id(&self) -> NumericalArtifactIdV2 {
        self.record.proof.atomic_evidence.artifact_id
    }

    /// Exact numerical error-bound law used by the atomic evaluator.
    pub const fn bound_id(&self) -> NumericalErrorBoundIdV2 {
        self.record.proof.atomic_evidence.bound_id
    }

    /// Exact complete-domain numerical proof used by the atomic evaluator.
    pub const fn proof_id(&self) -> NumericalProofIdV2 {
        self.record.proof.atomic_evidence.proof_id
    }

    /// SHA-256 of the exact #284 proof-file bytes.
    pub const fn proof_sha256(&self) -> &[u8; 32] {
        &self.record.proof.atomic_evidence.proof_sha256
    }
}

/// Zero-allocation view of one explicit candidate-major cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssessmentV1<'a> {
    candidate: &'a CandidateV1,
    relation_id: &'a RelationId,
    adjacent: Srgb8,
    decision: Wcag22ApplicableDecisionV1,
}

impl<'a> AssessmentV1<'a> {
    /// Opaque canonical candidate identity.
    pub const fn candidate_id(self) -> &'a CandidateId {
        &self.candidate.candidate_id
    }

    /// Exact final physical bytes.
    pub const fn emitted(self) -> Srgb8 {
        self.candidate.emitted
    }

    /// Opaque canonical relation identity.
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

struct AssessmentIter<'a> {
    candidates: &'a [CandidateV1],
    cursor: AssessmentCursorV1<'a>,
}

impl<'a> Iterator for AssessmentIter<'a> {
    type Item = AssessmentV1<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        let AssessmentCellV1 {
            candidate_index,
            relation,
            adjacent,
            decision,
        } = self.cursor.next()?;
        let candidate = usize::try_from(candidate_index)
            .ok()
            .and_then(|index| self.candidates.get(index))
            .expect("sealed explicit candidate cardinality must match the assessment cursor");
        Some(AssessmentV1 {
            candidate,
            relation_id: &relation.relation_id,
            adjacent,
            decision,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.cursor.size_hint()
    }
}

impl ExactSizeIterator for AssessmentIter<'_> {}

/// Complete evaluated explicit-domain terminal. It has no public constructor.
#[derive(Debug)]
pub struct EvaluatedV1 {
    candidates: Vec<CandidateV1>,
    relations: Vec<RelationV1>,
    layout: WorkLayoutV1,
    packed: Vec<u8>,
    domain: DomainDescriptorV1,
    relation_set_digest: RelationSetDigestV1,
    evaluation_id: EvaluationIdV1,
    proof: EvaluationProofDataV1,
}

impl EvaluatedV1 {
    /// Exact candidate-major packed failure matrix. For candidate `c` and edge
    /// `e`, bit index is `cE + e`; one means Fail.
    pub fn failure_matrix(&self) -> &[u8] {
        let length = self.layout.failure_matrix_bytes as usize;
        &self.packed[..length]
    }

    fn partition(&self) -> &[u8] {
        let length = self.layout.failure_matrix_bytes as usize;
        &self.packed[length..]
    }

    /// Canonical explicit domain descriptor.
    pub const fn domain(&self) -> &DomainDescriptorV1 {
        &self.domain
    }

    /// Canonical explicit domain digest.
    pub const fn domain_digest(&self) -> DomainDigestV1 {
        self.domain.digest
    }

    /// Canonical relation-set digest.
    pub const fn relation_set_digest(&self) -> RelationSetDigestV1 {
        self.relation_set_digest
    }

    /// Semantic evaluated-result identity.
    pub const fn evaluation_id(&self) -> EvaluationIdV1 {
        self.evaluation_id
    }

    /// Borrow the sealed complete-enumeration proof without copying its
    /// variable partition.
    pub const fn proof(&self) -> EvaluationProofV1<'_> {
        EvaluationProofV1 { record: self }
    }

    /// Canonical candidates in exact candidate-ID byte order.
    pub fn candidates(&self) -> &[CandidateV1] {
        &self.candidates
    }

    /// Canonical declared graph retained exactly once by the result.
    pub fn relations(&self) -> &[RelationV1] {
        &self.relations
    }

    /// Every feasible candidate in canonical candidate-ID byte order.
    pub fn feasible_candidates(&self) -> impl Iterator<Item = &CandidateV1> {
        (0_u64..)
            .zip(self.candidates.iter())
            .filter(move |(index, _)| packed_bit(self.partition(), *index))
            .map(|(_, candidate)| candidate)
    }

    /// Every infeasible candidate in canonical candidate-ID byte order.
    pub fn infeasible_candidates(&self) -> impl Iterator<Item = &CandidateV1> {
        (0_u64..)
            .zip(self.candidates.iter())
            .filter(move |(index, _)| !packed_bit(self.partition(), *index))
            .map(|(_, candidate)| candidate)
    }

    /// Full candidate-major `C×E` matrix without per-cell allocation.
    pub fn assessments(&self) -> impl ExactSizeIterator<Item = AssessmentV1<'_>> + '_ {
        AssessmentIter {
            candidates: &self.candidates,
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

/// Canonical declaration-only explicit-domain terminal.
#[derive(Debug)]
pub struct NotEvaluatedV1 {
    candidates: Vec<CandidateV1>,
    relations: Vec<RelationV1>,
    resource_profile_id: ResourceProfileIdV1,
    domain: DomainDescriptorV1,
    relation_set_digest: RelationSetDigestV1,
}

impl NotEvaluatedV1 {
    /// Canonical explicit domain descriptor.
    pub const fn domain(&self) -> &DomainDescriptorV1 {
        &self.domain
    }

    /// Canonical explicit candidates.
    pub fn candidates(&self) -> &[CandidateV1] {
        &self.candidates
    }

    /// Canonical relation-set digest.
    pub const fn relation_set_digest(&self) -> RelationSetDigestV1 {
        self.relation_set_digest
    }

    /// Operational policy that admitted canonicalization.
    pub const fn resource_profile_id(&self) -> ResourceProfileIdV1 {
        self.resource_profile_id
    }

    /// Canonical declaration-only graph.
    pub fn relations(&self) -> &[RelationV1] {
        &self.relations
    }
}

/// Exhaustive explicit-domain terminal algebra.
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
    /// Whether complete evaluation found at least one feasible candidate.
    pub const fn is_feasible(&self) -> bool {
        matches!(self, Self::Feasible(..))
    }

    /// Whether complete evaluation found no feasible candidate.
    pub const fn is_infeasible(&self) -> bool {
        matches!(self, Self::Infeasible(..))
    }

    /// Whether no relation was applicable and therefore no pair was evaluated.
    pub const fn is_not_evaluated(&self) -> bool {
        matches!(self, Self::NotEvaluated(..))
    }

    /// Borrow the complete evaluated record from either evaluated terminal.
    pub const fn evaluated(&self) -> Option<&EvaluatedV1> {
        match self {
            Self::Feasible(value) | Self::Infeasible(value) => Some(value),
            Self::NotEvaluated(..) => None,
        }
    }

    /// Borrow the declaration-only record when no relation was applicable.
    pub const fn not_evaluated(&self) -> Option<&NotEvaluatedV1> {
        match self {
            Self::NotEvaluated(value) => Some(value),
            Self::Feasible(..) | Self::Infeasible(..) => None,
        }
    }
}

/// Canonicalize and exhaustively evaluate one client-declared finite set.
pub fn evaluate(request: RequestV1) -> Result<FeasibilityV1, ErrorV1> {
    let mut evaluator = AtomicPairEvaluator::new();
    let mut storage = PackedDecisionStorage::default();
    let request = KernelRequestV1 {
        domain: request.domain,
        relations: request.relations,
        resource_profile_id: request.resource_profile_id,
    };
    match evaluate_domain_with(request, &mut evaluator, &mut storage)? {
        KernelResultV1::NotEvaluated(result) => {
            let candidate_count = result.layout.candidate_count;
            let digest = domain_digest(&result.domain, candidate_count);
            let domain = DomainDescriptorV1 {
                kind: DomainKindV1::ExplicitSrgb8Set,
                digest,
                candidate_count,
            };
            let relation_set_digest = relation_set_digest(&result.relations);
            Ok(FeasibilityV1::NotEvaluated(NotEvaluatedV1 {
                candidates: result.domain.into_candidates(),
                relations: result.relations,
                resource_profile_id: result.resource_profile_id,
                domain,
                relation_set_digest,
            }))
        }
        KernelResultV1::Evaluated(result) => {
            let sealed = seal_evaluated_v1(&result, storage.into_bytes())?;
            let digest = domain_digest(&result.domain, result.observed_candidates);
            let domain = DomainDescriptorV1 {
                kind: DomainKindV1::ExplicitSrgb8Set,
                digest,
                candidate_count: result.observed_candidates,
            };
            let relation_set_digest = relation_set_digest(&result.relations);
            let evaluation_id = evaluation_id(
                digest,
                relation_set_digest,
                &result.atomic_evidence,
                result.layout,
                &sealed.matrix_digest,
                sealed.partition(),
            );
            let proof = EvaluationProofDataV1 {
                resource_profile_id: result.resource_profile_id,
                canonical_relations: result.layout.canonical_relations,
                applicable_relations: result.layout.applicable_relations,
                not_applicable_relations: result.layout.not_evaluated_relations,
                applicable_edges: result.layout.applicable_edges,
                logical_assessments: result.observed_assessments,
                matrix_digest: sealed.matrix_digest,
                atomic_evidence: result.atomic_evidence,
            };
            let passing_candidates = result.passing_candidates;
            let evaluated = EvaluatedV1 {
                candidates: result.domain.into_candidates(),
                relations: result.relations,
                layout: result.layout,
                packed: sealed.packed,
                domain,
                relation_set_digest,
                evaluation_id,
                proof,
            };
            if passing_candidates == 0 {
                Ok(FeasibilityV1::Infeasible(evaluated))
            } else {
                Ok(FeasibilityV1::Feasible(evaluated))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wcag22::Wcag22CriterionV1;
    use crate::wcag22_feasibility::OccurrenceId;

    #[test]
    #[should_panic(expected = "sealed explicit candidate cardinality")]
    fn assessment_iterator_cannot_hide_a_broken_sealed_cardinality() {
        let candidates = [CandidateV1::new(
            CandidateId::try_new("only-candidate").unwrap(),
            Srgb8::new([0; 3]),
        )];
        let relations = [RelationV1::applicable(
            RelationId::try_new("relation").unwrap(),
            OccurrenceId::try_new("occurrence").unwrap(),
            Wcag22CriterionV1::Sc143TextDefault,
            vec![Srgb8::new([255; 3])],
        )
        .unwrap()];
        let matrix = [0_u8];
        let mut iterator = AssessmentIter {
            candidates: &candidates,
            cursor: AssessmentCursorV1 {
                relations: &relations,
                matrix: &matrix,
                candidate_index: 0,
                candidate_count: 2,
                relation_index: 0,
                adjacent_index: 0,
                logical_index: 0,
                remaining: 2,
            },
        };

        assert!(iterator.next().is_some());
        let _ = iterator.next();
    }
}
