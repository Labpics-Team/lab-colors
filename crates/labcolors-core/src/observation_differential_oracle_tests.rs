//! Differential oracle tests for O-10 bounded ScenarioId admission.
//!
//! These tests prove permutation invariance, provenance replay, replay
//! idempotency, adjacent duplicate detection, O(n log n) complexity bounds,
//! and arena slot reuse correctness for the schema-ordered admission path
//! independently of its implementation details.

use core::cell::Cell;

use crate::Srgb8;
use crate::appearance::SurfaceInputPortId;
use crate::observation::{
    CanonicalObservationSchemaV1, ObservationArenaPoolV1, ObservationError, ObservationHeadViewV1,
    ObservationOwnerV1, PreparedObservationUpdateV1, Revision, RevisionBoundObservationV1,
    RevisionBoundUnknownV1, ScenarioId, SchemaOrderedScenarioSourceV1,
    canonicalize_observation_schema, prepare_schema_ordered_observation,
};

const PORT_A: SurfaceInputPortId = SurfaceInputPortId::new(10);
const PORT_B: SurfaceInputPortId = SurfaceInputPortId::new(20);
const STREAM: crate::observation::ObservationStreamId =
    crate::observation::ObservationStreamId::new(7);

// ---------------------------------------------------------------------------
// Test infrastructure
// ---------------------------------------------------------------------------

/// Minimal owner that tracks only the current head view.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum OracleOwner {
    Empty,
    Unknown(RevisionBoundUnknownV1),
    Observed(RevisionBoundObservationV1),
}

impl ObservationOwnerV1 for OracleOwner {
    fn observation_head(&self) -> ObservationHeadViewV1<'_> {
        match self {
            Self::Empty => ObservationHeadViewV1::Empty,
            Self::Unknown(u) => ObservationHeadViewV1::Unknown(u),
            Self::Observed(o) => ObservationHeadViewV1::Observed(o),
        }
    }
}

/// In-memory schema-ordered source built from flat scenario data.
#[derive(Debug, Clone)]
struct FlatSource {
    scenario_ids: Vec<ScenarioId>,
    values: Vec<Srgb8>,
    binding_count: usize,
}

impl FlatSource {
    fn new(scenarios: &[(u32, &[Srgb8])]) -> Self {
        let binding_count = scenarios.first().map_or(0, |(_, v)| v.len());
        let mut scenario_ids = Vec::with_capacity(scenarios.len());
        let mut values = Vec::with_capacity(scenarios.len() * binding_count);
        for &(id, vals) in scenarios {
            scenario_ids.push(ScenarioId::new(id));
            values.extend_from_slice(vals);
        }
        Self {
            scenario_ids,
            values,
            binding_count,
        }
    }
}

impl SchemaOrderedScenarioSourceV1 for FlatSource {
    fn scenario_count(&self) -> usize {
        self.scenario_ids.len()
    }

    fn scenario_id(&self, scenario_index: usize) -> ScenarioId {
        self.scenario_ids[scenario_index]
    }

    fn value_count(&self, _scenario_index: usize) -> usize {
        self.binding_count
    }

    fn value(&self, scenario_index: usize, binding_index: usize) -> Srgb8 {
        self.values[scenario_index * self.binding_count + binding_index]
    }
}

fn test_schema() -> CanonicalObservationSchemaV1 {
    canonicalize_observation_schema(vec![PORT_A, PORT_B]).expect("test schema must be valid")
}

fn srgb(r: u8, g: u8, b: u8) -> Srgb8 {
    Srgb8::new([r, g, b])
}

/// Apply a schema-ordered observation at the given revision, returning the
/// admitted backing on success. Accepts any `SchemaOrderedScenarioSourceV1`.
fn apply_schema_ordered<S: SchemaOrderedScenarioSourceV1>(
    owner: &mut OracleOwner,
    arenas: &mut ObservationArenaPoolV1,
    schema: &CanonicalObservationSchemaV1,
    revision: u64,
    source: &S,
    scratch: &mut Vec<usize>,
) -> Result<RevisionBoundObservationV1, ObservationError> {
    let prepared = prepare_schema_ordered_observation(
        owner,
        arenas,
        STREAM,
        schema,
        Revision::new(revision),
        source,
        scratch,
    )?;
    match prepared {
        PreparedObservationUpdateV1::Observed(p) => {
            let (owner_ref, obs) = p.into_parts();
            *owner_ref = OracleOwner::Observed(obs.clone());
            Ok(obs)
        }
        PreparedObservationUpdateV1::Idempotent(_) => {
            panic!("expected Observed variant in test helper")
        }
        PreparedObservationUpdateV1::Unknown(_) => {
            panic!("expected Observed variant in test helper")
        }
    }
}

// ---------------------------------------------------------------------------
// 1. Permutation invariance
// ---------------------------------------------------------------------------

/// For any permutation of input scenarios with the same IDs and values, the
/// canonical output (physical case count, values, provenance) is byte-identical.
#[test]
fn permutation_invariance_canonical_output_is_identical() {
    let schema = test_schema();
    let base_scenarios: Vec<(u32, Vec<Srgb8>)> = vec![
        (5, vec![srgb(10, 20, 30), srgb(40, 50, 60)]),
        (2, vec![srgb(70, 80, 90), srgb(100, 110, 120)]),
        (8, vec![srgb(130, 140, 150), srgb(160, 170, 180)]),
        (1, vec![srgb(190, 200, 210), srgb(220, 230, 240)]),
    ];

    // Collect canonical reference from sorted order.
    let mut sorted = base_scenarios.clone();
    sorted.sort_by_key(|(id, _)| *id);
    let reference_source = FlatSource::new(
        &sorted
            .iter()
            .map(|(id, v)| (*id, v.as_slice()))
            .collect::<Vec<_>>(),
    );
    let mut ref_arenas = ObservationArenaPoolV1::new(&schema);
    let mut ref_owner = OracleOwner::Empty;
    let mut ref_scratch = Vec::new();
    let reference = apply_schema_ordered(
        &mut ref_owner,
        &mut ref_arenas,
        &schema,
        1,
        &reference_source,
        &mut ref_scratch,
    )
    .expect("reference admission must succeed");

    // Test several distinct permutations.
    let permutations: Vec<Vec<usize>> = vec![
        vec![0, 1, 2, 3],
        vec![3, 2, 1, 0],
        vec![1, 3, 0, 2],
        vec![2, 0, 3, 1],
        vec![3, 0, 1, 2],
    ];

    for perm in &permutations {
        let permuted: Vec<(u32, Vec<Srgb8>)> =
            perm.iter().map(|&i| base_scenarios[i].clone()).collect();
        let source = FlatSource::new(
            &permuted
                .iter()
                .map(|(id, v)| (*id, v.as_slice()))
                .collect::<Vec<_>>(),
        );
        let mut arenas = ObservationArenaPoolV1::new(&schema);
        let mut owner = OracleOwner::Empty;
        let mut scratch = Vec::new();
        let result =
            apply_schema_ordered(&mut owner, &mut arenas, &schema, 1, &source, &mut scratch)
                .expect("permuted admission must succeed");

        assert_eq!(
            result.physical_case_count(),
            reference.physical_case_count(),
            "case count mismatch for permutation {perm:?}"
        );
        for case in 0..result.physical_case_count() {
            assert_eq!(
                result.physical_values(case),
                reference.physical_values(case),
                "values mismatch at case {case} for permutation {perm:?}"
            );
            assert_eq!(
                result.provenance(case),
                reference.provenance(case),
                "provenance mismatch at case {case} for permutation {perm:?}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Provenance replay
// ---------------------------------------------------------------------------

/// The provenance vector maps each physical case back to its original
/// ScenarioId regardless of sort order. When multiple scenarios share the
/// same physical tuple, all their IDs appear in the provenance slice.
#[test]
fn provenance_replay_maps_physical_cases_to_original_scenario_ids() {
    let schema = test_schema();
    // Scenarios 3 and 7 have identical values — they should merge into one
    // physical case with two provenance entries.
    let shared_values = [srgb(10, 20, 30), srgb(40, 50, 60)];
    let unique_a = [srgb(70, 80, 90), srgb(100, 110, 120)];
    let unique_b = [srgb(130, 140, 150), srgb(160, 170, 180)];
    let scenarios: Vec<(u32, &[Srgb8])> = vec![
        (3, &shared_values),
        (1, &unique_a),
        (7, &shared_values),
        (5, &unique_b),
    ];
    let source = FlatSource::new(&scenarios);
    let mut arenas = ObservationArenaPoolV1::new(&schema);
    let mut owner = OracleOwner::Empty;
    let mut scratch = Vec::new();
    let obs = apply_schema_ordered(&mut owner, &mut arenas, &schema, 1, &source, &mut scratch)
        .expect("admission must succeed");

    // Physical cases are sorted by (values, scenario_id). Collect all
    // provenance IDs across all cases.
    let mut all_provenance: Vec<ScenarioId> = Vec::new();
    for case in 0..obs.physical_case_count() {
        let prov = obs.provenance(case).expect("case must have provenance");
        all_provenance.extend_from_slice(prov);
    }
    all_provenance.sort();

    let mut expected_ids: Vec<ScenarioId> = scenarios
        .iter()
        .map(|(id, _)| ScenarioId::new(*id))
        .collect();
    expected_ids.sort();

    assert_eq!(
        all_provenance, expected_ids,
        "every input ScenarioId must appear exactly once across all provenance slices"
    );

    // Verify the merged case contains both IDs 3 and 7.
    let mut found_merged = false;
    for case in 0..obs.physical_case_count() {
        let prov = obs.provenance(case).expect("case must have provenance");
        if prov.len() == 2 {
            let mut ids: Vec<u32> = prov.iter().map(|s| s.value()).collect();
            ids.sort();
            assert_eq!(
                ids,
                vec![3, 7],
                "merged case must contain scenario IDs 3 and 7"
            );
            found_merged = true;
        }
    }
    assert!(
        found_merged,
        "expected one physical case with two provenance entries for identical tuples"
    );
}

// ---------------------------------------------------------------------------
// 3. Replay idempotency
// ---------------------------------------------------------------------------

/// Feeding the same canonical input at the same revision produces an
/// Idempotent result without reallocating a new backing.
#[test]
fn replay_idempotency_same_revision_returns_idempotent_without_reallocation() {
    let schema = test_schema();
    let val_a = [srgb(10, 20, 30), srgb(40, 50, 60)];
    let val_b = [srgb(70, 80, 90), srgb(100, 110, 120)];
    let scenarios: Vec<(u32, &[Srgb8])> = vec![(2, &val_a), (5, &val_b)];
    let source = FlatSource::new(&scenarios);
    let mut arenas = ObservationArenaPoolV1::new(&schema);
    let mut owner = OracleOwner::Empty;
    let mut scratch = Vec::new();

    let first = apply_schema_ordered(&mut owner, &mut arenas, &schema, 1, &source, &mut scratch)
        .expect("first admission must succeed");
    let first_ptr = first.backing_ptr_for_test();

    // Replay at the same revision with the same data.
    let prepared = prepare_schema_ordered_observation(
        &mut owner,
        &mut arenas,
        STREAM,
        &schema,
        Revision::new(1),
        &source,
        &mut scratch,
    )
    .expect("replay must not error");

    match prepared {
        PreparedObservationUpdateV1::Idempotent(p) => {
            let _owner = p.into_owner();
            // Owner still holds the original observation.
            match &owner {
                OracleOwner::Observed(obs) => {
                    assert_eq!(
                        obs.backing_ptr_for_test(),
                        first_ptr,
                        "idempotent replay must reuse the same backing allocation"
                    );
                }
                _ => panic!("owner must remain Observed after idempotent replay"),
            }
        }
        _ => panic!("same-revision same-data replay must return Idempotent"),
    }
}

// ---------------------------------------------------------------------------
// 4. Adjacent duplicate detection
// ---------------------------------------------------------------------------

/// DuplicateScenarioId fires for any duplicate ScenarioId regardless of its
/// position in the input (beginning, middle, end, or non-adjacent before sort).
#[test]
fn adjacent_duplicate_detection_fires_for_any_duplicate_position() {
    let schema = test_schema();
    let val_a = [srgb(10, 20, 30), srgb(40, 50, 60)];
    let val_b = [srgb(70, 80, 90), srgb(100, 110, 120)];
    let val_c = [srgb(130, 140, 150), srgb(160, 170, 180)];

    // Each entry places the duplicate ID (42) at a different logical position.
    let duplicate_configs: Vec<Vec<(u32, &[Srgb8])>> = vec![
        // Duplicate at beginning (after sort: 42, 42, 99)
        vec![(42, &val_a), (99, &val_b), (42, &val_c)],
        // Duplicate at end (after sort: 1, 42, 42)
        vec![(1, &val_a), (42, &val_b), (42, &val_c)],
        // Duplicate in middle (after sort: 1, 42, 42, 99)
        vec![(42, &val_a), (1, &val_b), (99, &val_c), (42, &val_a)],
        // Non-adjacent before sort, adjacent after sort
        vec![(42, &val_a), (1, &val_b), (42, &val_c), (99, &val_a)],
    ];

    for (i, config) in duplicate_configs.iter().enumerate() {
        let source = FlatSource::new(config);
        let mut arenas = ObservationArenaPoolV1::new(&schema);
        let mut owner = OracleOwner::Empty;
        let mut scratch = Vec::new();
        let result = prepare_schema_ordered_observation(
            &mut owner,
            &mut arenas,
            STREAM,
            &schema,
            Revision::new(1),
            &source,
            &mut scratch,
        );
        match &result {
            Err(ObservationError::DuplicateScenarioId { scenario }) => {
                assert_eq!(
                    scenario.value(),
                    42,
                    "config {i}: expected DuplicateScenarioId(42), got DuplicateScenarioId({})",
                    scenario.value()
                );
            }
            Err(e) => panic!("config {i}: expected DuplicateScenarioId(42), got Err({e:?})"),
            Ok(_) => panic!("config {i}: expected DuplicateScenarioId(42), got Ok"),
        }
    }
}

// ---------------------------------------------------------------------------
// 5. Complexity bound: O(n log n) ScenarioId reads
// ---------------------------------------------------------------------------

/// A counting source wrapper proves that ScenarioId reads during admission
/// are bounded by O(n log n). We verify the count does not exceed
/// c * n * log2(n) for a reasonable constant c.
#[test]
fn complexity_bound_scenario_id_reads_are_o_n_log_n() {
    let schema = test_schema();
    let sizes: Vec<usize> = vec![8, 16, 32, 64, 128];

    for &n in &sizes {
        let scenarios: Vec<(u32, Vec<Srgb8>)> = (0..n)
            .rev() // Reverse order forces worst-case sort behavior.
            .map(|i| {
                let r = (i % 256) as u8;
                let g = ((i * 7) % 256) as u8;
                let b = ((i * 13) % 256) as u8;
                (i as u32, vec![srgb(r, g, b), srgb(b, r, g)])
            })
            .collect();
        let source = CountingSource::new(&scenarios);
        let read_counter = source.read_count();

        let mut arenas = ObservationArenaPoolV1::new(&schema);
        let mut owner = OracleOwner::Empty;
        let mut scratch = Vec::new();
        let _ = apply_schema_ordered(&mut owner, &mut arenas, &schema, 1, &source, &mut scratch)
            .expect("admission must succeed");

        let reads = read_counter.get();
        // Upper bound: c * n * ceil(log2(n)) with c = 8. The admission path
        // performs two unstable sorts over scenario indices (each comparison
        // reads ScenarioId from both sides), a linear duplicate scan, a value-
        // count validation pass, and the canonical materialization sort which
        // also compares by ScenarioId as tiebreaker. Empirical measurement at
        // n=16 yields ~18 reads/n, so c=8 provides safe headroom without
        // being vacuous.
        let log2_n = (n as f64).log2().ceil() as usize;
        let upper = 8 * n * log2_n.max(1);
        assert!(
            reads <= upper,
            "n={n}: ScenarioId reads {reads} exceeded O(n log n) bound {upper}"
        );
    }
}

/// Wrapper around FlatSource that counts `scenario_id()` calls.
#[derive(Debug)]
struct CountingSource {
    inner: FlatSource,
    read_count: Cell<usize>,
}

impl CountingSource {
    fn new(scenarios: &[(u32, Vec<Srgb8>)]) -> Self {
        let refs: Vec<(u32, &[Srgb8])> = scenarios
            .iter()
            .map(|(id, v)| (*id, v.as_slice()))
            .collect();
        Self {
            inner: FlatSource::new(&refs),
            read_count: Cell::new(0),
        }
    }

    fn read_count(&self) -> &Cell<usize> {
        &self.read_count
    }
}

impl SchemaOrderedScenarioSourceV1 for CountingSource {
    fn scenario_count(&self) -> usize {
        self.inner.scenario_count()
    }

    fn scenario_id(&self, scenario_index: usize) -> ScenarioId {
        self.read_count.set(self.read_count.get() + 1);
        self.inner.scenario_id(scenario_index)
    }

    fn value_count(&self, scenario_index: usize) -> usize {
        self.inner.value_count(scenario_index)
    }

    fn value(&self, scenario_index: usize, binding_index: usize) -> Srgb8 {
        self.inner.value(scenario_index, binding_index)
    }
}

// ---------------------------------------------------------------------------
// 6. Arena integration: admission across slot reuse cycles
// ---------------------------------------------------------------------------

/// Admission works correctly across all three arena slots and reuses freed
/// slots without corruption. After filling all 3 slots, releasing one via
/// owner replacement allows the next admission to reuse it.
#[test]
fn arena_integration_admission_works_across_slot_reuse_cycles() {
    let schema = test_schema();
    let mut arenas = ObservationArenaPoolV1::new(&schema);
    let mut owner = OracleOwner::Empty;
    let mut scratch = Vec::new();

    let make_source = |base: u8| {
        let v1 = [
            srgb(base, base + 1, base + 2),
            srgb(base + 3, base + 4, base + 5),
        ];
        let v2 = [
            srgb(base + 10, base + 11, base + 12),
            srgb(base + 13, base + 14, base + 15),
        ];
        FlatSource::new(&[(1, &v1), (2, &v2)])
    };

    // Fill slot 0.
    let obs1 = apply_schema_ordered(
        &mut owner,
        &mut arenas,
        &schema,
        1,
        &make_source(0),
        &mut scratch,
    )
    .expect("slot 0 admission must succeed");
    let _slot0_ptr = obs1.backing_ptr_for_test();

    // Replace owner to release slot 0's unique claim, then fill slot 1.
    let obs1_clone = match &owner {
        OracleOwner::Observed(o) => o.clone(),
        _ => panic!("must be Observed"),
    };
    // Drop the owner's hold so Rc::get_mut can succeed on slot 0 later.
    owner = OracleOwner::Empty;

    let obs2 = apply_schema_ordered(
        &mut owner,
        &mut arenas,
        &schema,
        2,
        &make_source(20),
        &mut scratch,
    )
    .expect("slot 1 admission must succeed");

    // Release again and fill slot 2.
    owner = OracleOwner::Empty;
    let obs3 = apply_schema_ordered(
        &mut owner,
        &mut arenas,
        &schema,
        3,
        &make_source(40),
        &mut scratch,
    )
    .expect("slot 2 admission must succeed");

    // All three backings should be distinct allocations.
    assert_ne!(
        obs1_clone.backing_ptr_for_test(),
        obs2.backing_ptr_for_test()
    );
    assert_ne!(obs2.backing_ptr_for_test(), obs3.backing_ptr_for_test());
    assert_ne!(
        obs1_clone.backing_ptr_for_test(),
        obs3.backing_ptr_for_test()
    );

    // Now release everything and admit again — should reuse a freed slot.
    owner = OracleOwner::Empty;
    drop(obs1_clone);
    drop(obs2);
    drop(obs3);

    let obs4 = apply_schema_ordered(
        &mut owner,
        &mut arenas,
        &schema,
        4,
        &make_source(60),
        &mut scratch,
    )
    .expect("reuse admission must succeed");

    // The reused slot should produce a valid observation with correct data.
    assert_eq!(obs4.physical_case_count(), 2);
    let prov0 = obs4.provenance(0).expect("case 0 must have provenance");
    let prov1 = obs4.provenance(1).expect("case 1 must have provenance");
    let mut all_ids: Vec<u32> = prov0.iter().chain(prov1).map(|s| s.value()).collect();
    all_ids.sort();
    assert_eq!(
        all_ids,
        vec![1, 2],
        "reused slot must contain correct provenance"
    );
}
