//! Private contract for the bounded complete-enumeration kernel (#295).
//!
//! This file is intentionally a child of the `wcag22_feasibility`
//! module (`use super::*`).  It specifies two small, non-public seams:
//!
//! - `checked_layout_v1(raw, canonical, limits, addressable_byte_limit)`,
//!   `checked_logical_assessments_v1(E)` and
//!   `checked_packed_result_bytes_v1(A, E)` are pure checked arithmetic.  They
//!   do not inspect or allocate proportional input/result storage. `W = 256E`
//!   is the exact evaluator-call and assessment cardinality.  `B = 0`
//!   when `A = 0`; only an evaluated result (`A > 0`) owns the 32-byte
//!   candidate partition and therefore has `B = 32(E + 1)`.
//! - `evaluate_with(request, evaluator, storage)` owns canonicalization and the
//!   exact `256 * E` traversal while `PairEvaluator` and `DecisionStorage` are
//!   replaceable private dependencies.  The production wrapper supplies the
//!   sealed #284 adapter and packed storage.
//!
//! The expected private records used below are deliberately plain structs.
//! `RawInputCountsV1`, `CanonicalCountsV1`, `ResourceLimitsV1` and
//! `WorkLayoutV1` use the field spellings exercised by the assertions.  The
//! evaluator returns `PairEvaluationV1`; its copied inputs, criterion and
//! `AtomicEvidenceBindingV1` let the compiler reject a dishonest adapter
//! before recording a bit.  `KernelResultV1` keeps `Evaluated` and
//! `NotEvaluated` structurally distinct; `finish` is the evaluated
//! terminal-mint boundary only.

use std::cell::Cell;
use std::rc::Rc;

use crate::Srgb8;
use crate::wcag22::{
    Wcag22ApplicableDecisionV1, Wcag22ClientDeclaredNotApplicableV1, Wcag22CriterionV1,
    Wcag22EvaluationErrorV1,
};

use super::*;

const DOMAIN: DomainIdV1 = DomainIdV1::Srgb8NeutralAxis;
const PROFILE: ResourceProfileIdV1 = ResourceProfileIdV1::Compile;
const CANDIDATES: u64 = 256;

fn grey(value: u8) -> Srgb8 {
    Srgb8::new([value; 3])
}

fn relation_id(value: &str) -> RelationId {
    RelationId::try_new(value).expect("the test uses non-empty opaque relation IDs")
}

fn occurrence_id(value: &str) -> OccurrenceId {
    OccurrenceId::try_new(value).expect("the test uses non-empty opaque occurrence IDs")
}

fn applicable_relation(
    relation: &str,
    adjacent: Vec<Srgb8>,
    criterion: Wcag22CriterionV1,
) -> RelationV1 {
    RelationV1::applicable(
        relation_id(relation),
        occurrence_id(&format!("occurrence-{relation}")),
        criterion,
        adjacent,
    )
    .expect("an applicable relation has at least one adjacent colour")
}

fn not_applicable_relation(relation: &str) -> RelationV1 {
    RelationV1::not_applicable(
        relation_id(relation),
        occurrence_id(&format!("occurrence-{relation}")),
        Wcag22ClientDeclaredNotApplicableV1::try_new(format!("reason-{relation}"))
            .expect("the test uses a non-empty client declaration"),
    )
}

fn request(relations: Vec<RelationV1>) -> RequestV1 {
    RequestV1::try_new(DOMAIN, relations, PROFILE).expect("the test request is locally well-formed")
}

fn raw_counts() -> RawInputCountsV1 {
    RawInputCountsV1 {
        raw_relations: 5,
        raw_adjacent_entries: 9,
        opaque_utf8_bytes: 17,
    }
}

fn canonical_counts() -> CanonicalCountsV1 {
    CanonicalCountsV1 {
        canonical_relations: 3,
        applicable_relations: 2,
        not_evaluated_relations: 1,
        applicable_edges: 7,
    }
}

fn unlimited() -> ResourceLimitsV1 {
    ResourceLimitsV1 {
        raw_relations: u64::MAX,
        raw_adjacent_entries: u64::MAX,
        opaque_utf8_bytes: u64::MAX,
        canonical_relations: u64::MAX,
        applicable_edges: u64::MAX,
        logical_assessments: u64::MAX,
        packed_result_bytes: u64::MAX,
    }
}

fn requested(layout: &WorkLayoutV1, dimension: ResourceDimensionV1) -> u64 {
    match dimension {
        ResourceDimensionV1::RawRelations => layout.raw_relations,
        ResourceDimensionV1::RawAdjacentEntries => layout.raw_adjacent_entries,
        ResourceDimensionV1::OpaqueUtf8Bytes => layout.opaque_utf8_bytes,
        ResourceDimensionV1::CanonicalRelations => layout.canonical_relations,
        ResourceDimensionV1::ApplicableEdges => layout.applicable_edges,
        ResourceDimensionV1::LogicalAssessments => layout.logical_assessments,
        ResourceDimensionV1::PackedResultBytes => layout.packed_result_bytes,
    }
}

fn with_limit(
    mut limits: ResourceLimitsV1,
    dimension: ResourceDimensionV1,
    limit: u64,
) -> ResourceLimitsV1 {
    match dimension {
        ResourceDimensionV1::RawRelations => limits.raw_relations = limit,
        ResourceDimensionV1::RawAdjacentEntries => limits.raw_adjacent_entries = limit,
        ResourceDimensionV1::OpaqueUtf8Bytes => limits.opaque_utf8_bytes = limit,
        ResourceDimensionV1::CanonicalRelations => limits.canonical_relations = limit,
        ResourceDimensionV1::ApplicableEdges => limits.applicable_edges = limit,
        ResourceDimensionV1::LogicalAssessments => limits.logical_assessments = limit,
        ResourceDimensionV1::PackedResultBytes => limits.packed_result_bytes = limit,
    }
    limits
}

#[test]
fn raw_and_canonical_counts_map_to_one_exact_checked_layout() {
    let layout = checked_layout_v1(
        PROFILE,
        raw_counts(),
        canonical_counts(),
        unlimited(),
        u64::MAX,
    )
    .expect("small counts fit every checked quantity");

    assert_eq!(layout.raw_relations, 5);
    assert_eq!(layout.raw_adjacent_entries, 9);
    assert_eq!(layout.opaque_utf8_bytes, 17);
    assert_eq!(layout.canonical_relations, 3);
    assert_eq!(layout.applicable_relations, 2);
    assert_eq!(layout.not_evaluated_relations, 1);
    assert_eq!(layout.applicable_edges, 7);
    assert_eq!(layout.candidate_count, CANDIDATES);
    assert_eq!(layout.logical_assessments, CANDIDATES * 7);
    assert_eq!(layout.failure_matrix_bytes, 32 * 7);
    assert_eq!(layout.partition_bytes, 32);
    assert_eq!(layout.packed_result_bytes, 32 * (7 + 1));
}

#[test]
fn each_raw_to_canonical_lower_bound_is_an_independent_layout_invariant() {
    let relation_underflow = checked_layout_v1(
        PROFILE,
        RawInputCountsV1 {
            raw_relations: canonical_counts().canonical_relations - 1,
            ..raw_counts()
        },
        canonical_counts(),
        unlimited(),
        u64::MAX,
    )
    .expect_err("canonical relations cannot outnumber raw relations");
    assert!(matches!(
        relation_underflow,
        ErrorV1::CompilerInvariantViolation(CompilerInvariantV1::LayoutMismatch)
    ));

    let adjacent_underflow = checked_layout_v1(
        PROFILE,
        RawInputCountsV1 {
            raw_adjacent_entries: canonical_counts().applicable_edges - 1,
            ..raw_counts()
        },
        canonical_counts(),
        unlimited(),
        u64::MAX,
    )
    .expect_err("canonical edges cannot outnumber raw adjacent entries");
    assert!(matches!(
        adjacent_underflow,
        ErrorV1::CompilerInvariantViolation(CompilerInvariantV1::LayoutMismatch)
    ));
}

#[test]
fn raw_relation_limit_precedes_conflict_detection_and_canonical_work() {
    let limit = PROFILE.limit(ResourceDimensionV1::RawRelations);
    let duplicate = not_applicable_relation("same");
    let mut relations = vec![duplicate; limit as usize];
    relations.push(RelationV1::not_applicable(
        relation_id("same"),
        occurrence_id("conflicting-occurrence"),
        Wcag22ClientDeclaredNotApplicableV1::try_new("conflicting-reason").unwrap(),
    ));

    let error = evaluate(request(relations))
        .expect_err("raw cardinality must reject before conflict detection");
    assert!(matches!(
        error,
        ErrorV1::ResourceLimitExceeded {
            profile_id: PROFILE,
            dimension: ResourceDimensionV1::RawRelations,
            requested,
            limit: actual_limit,
        } if requested == limit + 1 && actual_limit == limit
    ));
}

#[test]
fn every_resource_dimension_accepts_limit_and_rejects_limit_plus_one() {
    let baseline = checked_layout_v1(
        PROFILE,
        raw_counts(),
        canonical_counts(),
        unlimited(),
        u64::MAX,
    )
    .unwrap();

    for dimension in [
        ResourceDimensionV1::RawRelations,
        ResourceDimensionV1::RawAdjacentEntries,
        ResourceDimensionV1::OpaqueUtf8Bytes,
        ResourceDimensionV1::CanonicalRelations,
        ResourceDimensionV1::ApplicableEdges,
        ResourceDimensionV1::LogicalAssessments,
        ResourceDimensionV1::PackedResultBytes,
    ] {
        let value = requested(&baseline, dimension);
        checked_layout_v1(
            PROFILE,
            raw_counts(),
            canonical_counts(),
            with_limit(unlimited(), dimension, value),
            u64::MAX,
        )
        .unwrap_or_else(|error| panic!("{dimension:?} rejected its exact limit: {error:?}"));

        let limit = value - 1;
        let error = checked_layout_v1(
            PROFILE,
            raw_counts(),
            canonical_counts(),
            with_limit(unlimited(), dimension, limit),
            u64::MAX,
        )
        .expect_err("limit + 1 must reject before allocation");
        assert!(matches!(
            error,
            ErrorV1::ResourceLimitExceeded {
                profile_id: PROFILE,
                dimension: actual_dimension,
                requested,
                limit: actual_limit,
            } if actual_dimension == dimension && requested == value && actual_limit == limit
        ));
    }
}

#[test]
fn products_and_packed_result_sum_are_checked_without_allocating() {
    assert_eq!(checked_logical_assessments_v1(7).unwrap(), CANDIDATES * 7);
    assert!(matches!(
        checked_logical_assessments_v1(u64::MAX / CANDIDATES + 1),
        Err(InvalidRequestV1::ArithmeticOverflow)
    ));

    assert!(matches!(
        checked_packed_result_bytes_v1(1, u64::MAX / 32 + 1),
        Err(InvalidRequestV1::ArithmeticOverflow)
    ));
    assert!(matches!(
        checked_packed_result_bytes_v1(1, u64::MAX),
        Err(InvalidRequestV1::ArithmeticOverflow)
    ));
}

#[test]
fn sha256_hex_decoder_rejects_every_wrong_ascii_length_without_panicking() {
    for length in [0, 1, 63, 65, 127] {
        let value = "0".repeat(length);
        assert_eq!(decode_sha256_hex(&value), None, "accepted length {length}");
    }
    assert_eq!(decode_sha256_hex(&"g".repeat(64)), None);
    assert_eq!(decode_sha256_hex(&"é".repeat(32)), None);
}

#[test]
fn packed_storage_is_exactly_32_times_e_plus_one() {
    for edges in [1, 2, 7, 65_537] {
        assert_eq!(
            checked_packed_result_bytes_v1(1, edges).unwrap(),
            32 * (edges + 1),
            "one bit per candidate/edge plus one 256-bit partition"
        );
    }
}

#[test]
fn all_not_applicable_has_zero_work_and_zero_packed_result_bytes() {
    let layout = checked_layout_v1(
        PROFILE,
        RawInputCountsV1 {
            raw_relations: 2,
            raw_adjacent_entries: 0,
            opaque_utf8_bytes: 12,
        },
        CanonicalCountsV1 {
            canonical_relations: 2,
            applicable_relations: 0,
            not_evaluated_relations: 2,
            applicable_edges: 0,
        },
        unlimited(),
        u64::MAX,
    )
    .expect("declarations alone are a valid NotEvaluated compilation");

    assert_eq!(checked_packed_result_bytes_v1(0, 0).unwrap(), 0);
    assert_eq!(layout.logical_assessments, 0);
    assert_eq!(layout.failure_matrix_bytes, 0);
    assert_eq!(layout.partition_bytes, 0);
    assert_eq!(layout.packed_result_bytes, 0);
}

#[test]
fn u32_addressability_rejects_a_4_gib_result_without_building_a_vec() {
    let edges = u64::from(u32::MAX).div_ceil(32) - 1;
    let counts = CanonicalCountsV1 {
        canonical_relations: 1,
        applicable_relations: 1,
        not_evaluated_relations: 0,
        applicable_edges: edges,
    };
    let requested_bytes = 32 * (edges + 1);
    assert_eq!(requested_bytes, u64::from(u32::MAX) + 1);

    let error = checked_layout_v1(
        PROFILE,
        RawInputCountsV1 {
            raw_relations: 1,
            raw_adjacent_entries: edges,
            opaque_utf8_bytes: 2,
        },
        counts,
        unlimited(),
        u64::from(u32::MAX),
    )
    .expect_err("a 4 GiB result is not u32-addressable");
    assert!(matches!(
        error,
        ErrorV1::ResourceLimitExceeded {
            profile_id: PROFILE,
            dimension: ResourceDimensionV1::PackedResultBytes,
            requested,
            limit,
        } if requested == requested_bytes && limit == u64::from(u32::MAX)
    ));
}

#[derive(Clone)]
enum EvaluatorMode {
    AllPass,
    AllFail,
    FailFirstOfEachCandidate { edges_per_candidate: u64 },
    SourceError(Wcag22EvaluationErrorV1),
    UnexpectedNotEvaluated,
    InvalidEvidence,
    ForegroundMismatch,
    BackgroundMismatch,
    CriterionMismatch,
    EvidenceMismatch,
}

struct ProbeEvaluator {
    mode: EvaluatorMode,
    calls: Rc<Cell<u64>>,
}

impl ProbeEvaluator {
    fn new(mode: EvaluatorMode) -> (Self, Rc<Cell<u64>>) {
        let calls = Rc::new(Cell::new(0));
        (
            Self {
                mode,
                calls: Rc::clone(&calls),
            },
            calls,
        )
    }
}

impl PairEvaluator for ProbeEvaluator {
    fn evaluate_pair(
        &mut self,
        candidate: Srgb8,
        adjacent: Srgb8,
        criterion: Wcag22CriterionV1,
    ) -> Result<PairEvaluationV1, Wcag22EvaluationErrorV1> {
        let call = self.calls.get();
        self.calls.set(call + 1);

        if let EvaluatorMode::SourceError(error) = &self.mode {
            return Err(error.clone());
        }
        if matches!(&self.mode, EvaluatorMode::UnexpectedNotEvaluated) {
            return Ok(PairEvaluationV1::NotEvaluated);
        }
        if matches!(&self.mode, EvaluatorMode::InvalidEvidence) {
            return Ok(PairEvaluationV1::InvalidEvidence);
        }

        let mut foreground = candidate.bytes();
        let mut background = adjacent.bytes();
        let mut actual_criterion = criterion;
        let mut evidence = expected_atomic_evidence_binding_v1()
            .expect("the committed #284 evidence binding is valid");
        match &self.mode {
            EvaluatorMode::AllPass | EvaluatorMode::SourceError(_) => {}
            EvaluatorMode::AllFail => {}
            EvaluatorMode::FailFirstOfEachCandidate { .. } => {}
            EvaluatorMode::UnexpectedNotEvaluated => unreachable!(),
            EvaluatorMode::InvalidEvidence => unreachable!(),
            EvaluatorMode::ForegroundMismatch => foreground[0] ^= 1,
            EvaluatorMode::BackgroundMismatch => background[0] ^= 1,
            EvaluatorMode::CriterionMismatch => {
                actual_criterion = Wcag22CriterionV1::Sc1411GraphicalObject;
                if actual_criterion == criterion {
                    actual_criterion = Wcag22CriterionV1::Sc143TextDefault;
                }
            }
            EvaluatorMode::EvidenceMismatch => evidence.proof_sha256[0] ^= 1,
        }
        let decision = match &self.mode {
            EvaluatorMode::AllFail => Wcag22ApplicableDecisionV1::Fail,
            EvaluatorMode::FailFirstOfEachCandidate {
                edges_per_candidate,
            } if call % *edges_per_candidate == 0 => Wcag22ApplicableDecisionV1::Fail,
            _ => Wcag22ApplicableDecisionV1::Pass,
        };
        Ok(PairEvaluationV1::Evaluated {
            foreground,
            background,
            criterion: actual_criterion,
            decision,
            evidence,
        })
    }
}

#[derive(Debug, Default)]
struct ProbeStorage {
    fail_reserve: bool,
    fail_write: bool,
    fail_finish: bool,
    reserve_calls: u64,
    reserved_bytes: Option<u64>,
    writes: u64,
    finish_calls: u64,
}

impl ProbeStorage {
    fn allocation_failure() -> Self {
        Self {
            fail_reserve: true,
            ..Self::default()
        }
    }

    fn write_failure() -> Self {
        Self {
            fail_write: true,
            ..Self::default()
        }
    }

    fn finish_failure() -> Self {
        Self {
            fail_finish: true,
            ..Self::default()
        }
    }
}

impl DecisionStorage for ProbeStorage {
    fn try_reserve_exact(&mut self, requested_bytes: usize) -> Result<(), ()> {
        self.reserve_calls += 1;
        self.reserved_bytes =
            Some(u64::try_from(requested_bytes).expect("usize is representable in u64 on targets"));
        if self.fail_reserve { Err(()) } else { Ok(()) }
    }

    fn write_decision(
        &mut self,
        _logical_index: u64,
        _decision: Wcag22ApplicableDecisionV1,
    ) -> Result<(), ()> {
        self.writes += 1;
        if self.fail_write { Err(()) } else { Ok(()) }
    }

    fn finish(&mut self, _passing_partition: [u8; 32]) -> Result<(), ()> {
        self.finish_calls += 1;
        if self.fail_finish { Err(()) } else { Ok(()) }
    }
}

fn one_by_e_request() -> RequestV1 {
    request(vec![applicable_relation(
        "one-by-e",
        vec![grey(0), grey(118), grey(255)],
        Wcag22CriterionV1::Sc143TextDefault,
    )])
}

fn e_by_one_request() -> RequestV1 {
    request(vec![
        applicable_relation("edge-a", vec![grey(0)], Wcag22CriterionV1::Sc143TextDefault),
        applicable_relation(
            "edge-b",
            vec![grey(118)],
            Wcag22CriterionV1::Sc143TextLargeScale,
        ),
        applicable_relation(
            "edge-c",
            vec![grey(255)],
            Wcag22CriterionV1::Sc1411GraphicalObject,
        ),
    ])
}

fn mixed_request() -> RequestV1 {
    request(vec![
        not_applicable_relation("declared-a"),
        applicable_relation(
            "applicable",
            vec![grey(0), grey(255)],
            Wcag22CriterionV1::Sc143TextDefault,
        ),
        not_applicable_relation("declared-b"),
    ])
}

fn assert_complete_shape(request: RequestV1, edges: u64, mode: EvaluatorMode, passing: u64) {
    let (mut evaluator, calls) = ProbeEvaluator::new(mode);
    let mut storage = ProbeStorage::default();
    let result = evaluate_with(request, &mut evaluator, &mut storage)
        .expect("a truthful evaluator and available storage must finish");
    let result = match result {
        KernelResultV1::Evaluated(result) => result,
        KernelResultV1::NotEvaluated(_) => panic!("an applicable edge must be evaluated"),
    };
    let work = CANDIDATES * edges;

    assert_eq!(result.layout.applicable_edges, edges);
    assert_eq!(result.layout.logical_assessments, work);
    assert_eq!(result.observed_candidates, CANDIDATES);
    assert_eq!(result.observed_assessments, work);
    assert_eq!(result.passing_candidates, passing);
    assert_eq!(calls.get(), work, "the compiler must not short-circuit");
    assert_eq!(storage.reserve_calls, 1);
    assert_eq!(storage.reserved_bytes, Some(32 * (edges + 1)));
    assert_eq!(storage.writes, work);
    assert_eq!(storage.finish_calls, 1);
}

#[test]
fn all_not_applicable_never_calls_the_evaluator_or_mints_an_evaluated_partition() {
    let (mut evaluator, calls) = ProbeEvaluator::new(EvaluatorMode::AllPass);
    let mut storage = ProbeStorage::default();
    let result = evaluate_with(
        request(vec![
            not_applicable_relation("only-declaration-a"),
            not_applicable_relation("only-declaration-b"),
        ]),
        &mut evaluator,
        &mut storage,
    )
    .expect("explicit declarations compile to NotEvaluated");
    let result = match result {
        KernelResultV1::NotEvaluated(result) => result,
        KernelResultV1::Evaluated(_) => panic!("A = 0 cannot fabricate an evaluated terminal"),
    };

    assert_eq!(result.layout.applicable_relations, 0);
    assert_eq!(result.layout.applicable_edges, 0);
    assert_eq!(result.layout.logical_assessments, 0);
    assert_eq!(result.layout.partition_bytes, 0);
    assert_eq!(result.layout.packed_result_bytes, 0);
    assert_eq!(calls.get(), 0);
    assert_eq!(storage.reserve_calls, 1);
    assert_eq!(storage.reserved_bytes, Some(0));
    assert_eq!(storage.writes, 0);
    assert_eq!(storage.finish_calls, 0);
}

#[test]
fn one_by_e_e_by_one_and_mixed_declarations_all_execute_exact_w() {
    for make_request in [
        one_by_e_request as fn() -> RequestV1,
        e_by_one_request as fn() -> RequestV1,
    ] {
        assert_complete_shape(make_request(), 3, EvaluatorMode::AllPass, CANDIDATES);
        assert_complete_shape(make_request(), 3, EvaluatorMode::AllFail, 0);
        assert_complete_shape(
            make_request(),
            3,
            EvaluatorMode::FailFirstOfEachCandidate {
                edges_per_candidate: 3,
            },
            0,
        );
    }

    assert_complete_shape(mixed_request(), 2, EvaluatorMode::AllPass, CANDIDATES);
    assert_complete_shape(mixed_request(), 2, EvaluatorMode::AllFail, 0);
    assert_complete_shape(
        mixed_request(),
        2,
        EvaluatorMode::FailFirstOfEachCandidate {
            edges_per_candidate: 2,
        },
        0,
    );
}

#[test]
fn allocation_failure_precedes_the_first_evaluator_call_and_terminal() {
    let (mut evaluator, calls) = ProbeEvaluator::new(EvaluatorMode::AllPass);
    let mut storage = ProbeStorage::allocation_failure();
    let error = evaluate_with(
        request(vec![applicable_relation(
            "allocation",
            vec![grey(0)],
            Wcag22CriterionV1::Sc143TextDefault,
        )]),
        &mut evaluator,
        &mut storage,
    )
    .expect_err("the injected exact reservation fails");

    assert!(matches!(
        error,
        ErrorV1::AllocationFailed {
            profile_id: PROFILE,
            requested_bytes: 64,
        }
    ));
    assert_eq!(storage.reserve_calls, 1);
    assert_eq!(storage.reserved_bytes, Some(64));
    assert_eq!(calls.get(), 0);
    assert_eq!(storage.writes, 0);
    assert_eq!(storage.finish_calls, 0);
}

#[test]
fn storage_write_and_finish_failures_are_compiler_invariants_not_fake_pair_errors() {
    for (mut storage, expected, expected_calls) in [
        (
            ProbeStorage::write_failure(),
            CompilerInvariantV1::DecisionStorageRejectedCell,
            1,
        ),
        (
            ProbeStorage::finish_failure(),
            CompilerInvariantV1::DecisionStorageRejectedPartition,
            CANDIDATES,
        ),
    ] {
        let (mut evaluator, calls) = ProbeEvaluator::new(EvaluatorMode::AllPass);
        let error = evaluate_with(
            request(vec![applicable_relation(
                "storage",
                vec![grey(255)],
                Wcag22CriterionV1::Sc143TextDefault,
            )]),
            &mut evaluator,
            &mut storage,
        )
        .expect_err("storage invariants fail closed before a terminal is minted");

        assert!(matches!(
            error,
            ErrorV1::CompilerInvariantViolation(actual) if actual == expected
        ));
        assert_eq!(calls.get(), expected_calls);
        assert_eq!(
            storage.finish_calls,
            u64::from(expected_calls == CANDIDATES)
        );
    }
}

fn first_cell_failure(mode: EvaluatorMode) -> (ErrorV1, u64, ProbeStorage) {
    let (mut evaluator, calls) = ProbeEvaluator::new(mode);
    let mut storage = ProbeStorage::default();
    let error = evaluate_with(
        request(vec![applicable_relation(
            "invariant",
            vec![grey(255)],
            Wcag22CriterionV1::Sc143TextDefault,
        )]),
        &mut evaluator,
        &mut storage,
    )
    .expect_err("the first dishonest/source result must fail closed");
    (error, calls.get(), storage)
}

fn assert_no_partial_terminal(calls: u64, storage: &ProbeStorage) {
    assert_eq!(calls, 1);
    assert_eq!(storage.reserve_calls, 1);
    assert_eq!(storage.writes, 0);
    assert_eq!(storage.finish_calls, 0);
}

#[test]
fn source_error_is_typed_and_cannot_mint_a_partial_terminal() {
    let source = Wcag22EvaluationErrorV1::EvidenceRegistryMismatch("injected-source".into());
    let (error, calls, storage) = first_cell_failure(EvaluatorMode::SourceError(source.clone()));
    assert!(matches!(
        error,
        ErrorV1::EvaluatorInvariantViolation {
            candidate,
            adjacent,
            violation: EvaluatorInvariantV1::Source(actual),
            ..
        } if candidate == grey(0) && adjacent == grey(255) && actual == source
    ));
    assert_no_partial_terminal(calls, &storage);
}

#[test]
fn unexpected_not_evaluated_is_typed_and_cannot_mint_a_partial_terminal() {
    let (error, calls, storage) = first_cell_failure(EvaluatorMode::UnexpectedNotEvaluated);
    assert!(matches!(
        error,
        ErrorV1::EvaluatorInvariantViolation {
            violation: EvaluatorInvariantV1::UnexpectedNotEvaluated,
            ..
        }
    ));
    assert_no_partial_terminal(calls, &storage);
}

#[test]
fn invalid_evidence_shape_is_rejected_directly_without_a_synthetic_binding() {
    let (error, calls, storage) = first_cell_failure(EvaluatorMode::InvalidEvidence);
    assert!(matches!(
        error,
        ErrorV1::EvaluatorInvariantViolation {
            violation: EvaluatorInvariantV1::EvidenceMismatch,
            ..
        }
    ));
    assert_no_partial_terminal(calls, &storage);
}

#[test]
fn either_atomic_input_mismatch_is_typed_before_recording_a_bit() {
    for mode in [
        EvaluatorMode::ForegroundMismatch,
        EvaluatorMode::BackgroundMismatch,
    ] {
        let (error, calls, storage) = first_cell_failure(mode);
        assert!(matches!(
            error,
            ErrorV1::EvaluatorInvariantViolation {
                violation: EvaluatorInvariantV1::InputMismatch,
                ..
            }
        ));
        assert_no_partial_terminal(calls, &storage);
    }
}

#[test]
fn criterion_and_evidence_mismatches_are_typed_before_recording_a_bit() {
    let (criterion_error, criterion_calls, criterion_storage) =
        first_cell_failure(EvaluatorMode::CriterionMismatch);
    assert!(matches!(
        criterion_error,
        ErrorV1::EvaluatorInvariantViolation {
            violation: EvaluatorInvariantV1::CriterionMismatch,
            ..
        }
    ));
    assert_no_partial_terminal(criterion_calls, &criterion_storage);

    let (evidence_error, evidence_calls, evidence_storage) =
        first_cell_failure(EvaluatorMode::EvidenceMismatch);
    assert!(matches!(
        evidence_error,
        ErrorV1::EvaluatorInvariantViolation {
            violation: EvaluatorInvariantV1::EvidenceMismatch,
            ..
        }
    ));
    assert_no_partial_terminal(evidence_calls, &evidence_storage);
}

#[test]
fn matrix_partition_and_proof_invariants_reject_single_field_mutations() {
    let layout = checked_layout_v1(
        PROFILE,
        RawInputCountsV1 {
            raw_relations: 1,
            raw_adjacent_entries: 1,
            opaque_utf8_bytes: 2,
        },
        CanonicalCountsV1 {
            canonical_relations: 1,
            applicable_relations: 1,
            not_evaluated_relations: 0,
            applicable_edges: 1,
        },
        unlimited(),
        u64::MAX,
    )
    .unwrap();
    let all_pass_matrix = [0_u8; 32];
    let all_pass_partition = [u8::MAX; 32];
    let proof = EvaluationProofCountersV1 {
        logical_assessments: CANDIDATES,
        passing_candidates: CANDIDATES,
        failing_candidates: 0,
    };
    validate_complete_result_v1(layout, &all_pass_matrix, &all_pass_partition, proof)
        .expect("the exact all-pass matrix, partition and counts agree");

    assert!(
        validate_complete_result_v1(layout, &all_pass_matrix[..31], &all_pass_partition, proof,)
            .is_err(),
        "matrix length is part of the proof"
    );

    let mut one_failed_cell = all_pass_matrix;
    one_failed_cell[0] ^= 1;
    assert!(
        validate_complete_result_v1(layout, &one_failed_cell, &all_pass_partition, proof).is_err(),
        "the candidate partition must be derived from every matrix cell"
    );

    let mut one_missing_candidate = all_pass_partition;
    one_missing_candidate[0] ^= 1;
    assert!(
        validate_complete_result_v1(layout, &all_pass_matrix, &one_missing_candidate, proof)
            .is_err(),
        "partition bytes and proof counts cannot diverge"
    );

    assert!(
        validate_complete_result_v1(
            layout,
            &all_pass_matrix,
            &all_pass_partition,
            EvaluationProofCountersV1 {
                logical_assessments: CANDIDATES - 1,
                ..proof
            },
        )
        .is_err(),
        "the proof must attest the exact W cells"
    );
    assert!(
        validate_complete_result_v1(
            layout,
            &all_pass_matrix,
            &all_pass_partition,
            EvaluationProofCountersV1 {
                passing_candidates: CANDIDATES - 1,
                failing_candidates: 0,
                ..proof
            },
        )
        .is_err(),
        "passing + failing must equal the 256-candidate domain"
    );
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

#[test]
fn exact_identity_preimages_match_the_independent_cross_language_fixture() {
    let applicable = RelationV1::applicable(
        relation_id("alpha"),
        occurrence_id("hover/🎨"),
        Wcag22CriterionV1::Sc143TextDefault,
        vec![grey(255), grey(0), grey(118)],
    )
    .unwrap();
    let not_applicable = RelationV1::not_applicable(
        relation_id("zeta"),
        occurrence_id("ornament"),
        Wcag22ClientDeclaredNotApplicableV1::try_new("client/не-применимо").unwrap(),
    );
    let mut request = request(vec![not_applicable, applicable]);
    let raw = super::raw_counts(&request).unwrap();
    let canonical = canonicalize(&mut request).unwrap();
    let layout = checked_layout_v1(
        PROFILE,
        raw,
        canonical,
        ResourceLimitsV1::for_profile(PROFILE),
        u64::from(u32::MAX),
    )
    .unwrap();
    let matrix: Vec<u8> = (0..96).map(|index| (index * 37 + 11) as u8).collect();
    let mut partition = [0_u8; 32];
    for (index, byte) in partition.iter_mut().enumerate() {
        *byte = 255_u8.wrapping_sub((index * 3) as u8);
    }
    let domain = domain_digest(DOMAIN);
    let relations = relation_set_digest(&request.relations);
    let matrix_digest = *sha256::digest(&matrix).as_bytes();
    let evaluation = evaluation_id(
        domain,
        relations,
        &expected_atomic_evidence_binding_v1().unwrap(),
        layout,
        &matrix_digest,
        &partition,
    );

    assert_eq!(
        hex(domain.as_bytes()),
        "9634ac326979b23c2103ffcd92a2b890427ea8914a97b264b0c73409640f8466"
    );
    assert_eq!(
        hex(relations.as_bytes()),
        "990dbc58252dc518ccf63b2f4b63ef5ae227a2bed48dda9e5e5959f3e2477132"
    );
    assert_eq!(
        hex(evaluation.as_bytes()),
        "b1c69024ecded3a20269f69701192ec8c1803c40a384e6390bd94272c339c953"
    );
}
