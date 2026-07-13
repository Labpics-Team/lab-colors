//! Raw native-process measurements for the bounded WCAG 2.2 feasibility profile.
//!
//! This is an admission-data harness, not a timing correctness gate. It records
//! every native wall-time and global-allocator sample without reducing them to
//! a pass/fail percentile. The companion checker proves only scenario identity
//! and the exact `W = 256E`, `B = 0 | 32(E + 1)` page-slot arithmetic.
//!
//! The harness deliberately does not claim WebAssembly runtime memory, wire
//! serialization size, or client latency. Those require their own target- and
//! adapter-specific measurements.
//!
//! Run after registering this target with `harness = false`:
//!
//! ```text
//! LABCOLORS_WCAG22_BENCH_SAMPLES=5 \
//! LABCOLORS_WCAG22_BENCH_OUTPUT=/private/tmp/labcolors-wcag22-feasibility-admission-raw-v1.json \
//! cargo bench -p labcolors-core --bench wcag22_feasibility_admission
//! ```

use std::alloc::{GlobalAlloc, Layout, System};
use std::collections::{BTreeMap, btree_map::Entry};
use std::error::Error;
use std::fmt::Write as _;
use std::fs;
use std::hint::black_box;
use std::num::NonZeroU8;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use labcolors_core::Srgb8;
use labcolors_core::wcag22::{Wcag22ClientDeclaredNotApplicableV1, Wcag22CriterionV1};
use labcolors_core::wcag22_feasibility::{
    DomainIdV1, FeasibilityV1, OccurrenceId, RelationId, RelationV1, RequestV1,
    ResourceDimensionV1, ResourceProfileIdV1, evaluate,
};

#[path = "../src/sha256.rs"]
mod subject_sha256;

const ARTIFACT_ID: &str = "wcag22-feasibility-admission-raw-v1";
const DEFAULT_OUTPUT_FILENAME: &str = "labcolors-wcag22-feasibility-admission-raw-v1.json";
const CANDIDATE_COUNT: u64 = 256;
const PAGE_BYTES: u64 = 65_536;
const DECISION_SLOT_BYTES: u64 = 32;
const PARTITION_BYTES: u64 = 32;
const MAX_APPLICABLE_EDGES: u64 = PAGE_BYTES / DECISION_SLOT_BYTES - 1;
// The committed result artifact lives under `contracts`, so binding that whole
// tree would make durable verification self-referential. Bind the two contracts
// the compiler actually consumes as exact blobs instead.
const SOURCE_OBJECTS: [(&str, &str); 8] = [
    ("workspaceCargo", "Cargo.toml"),
    ("workspaceLock", "Cargo.lock"),
    ("coreCargo", "crates/labcolors-core/Cargo.toml"),
    ("coreSourceTree", "crates/labcolors-core/src"),
    (
        "wcag22Srgb8Contract",
        "crates/labcolors-core/contracts/wcag22-srgb8-v1.json",
    ),
    (
        "wcag22Q55ProofContract",
        "crates/labcolors-core/contracts/wcag22-srgb8-q55-proof-v1.json",
    ),
    (
        "benchmarkHarness",
        "crates/labcolors-core/benches/wcag22_feasibility_admission.rs",
    ),
    (
        "benchmarkChecker",
        "scripts/check_wcag22_feasibility_benchmark.py",
    ),
];
const SUBJECT_PATHS: [&str; 16] = [
    "Cargo.toml",
    "crates/labcolors-core/src/lib.rs",
    "crates/labcolors-core/src/wcag22_feasibility.rs",
    "crates/labcolors-core/src/srgb8.rs",
    "crates/labcolors-core/src/sha256.rs",
    "crates/labcolors-core/src/wcag22.rs",
    "crates/labcolors-core/src/wcag22/kernel.rs",
    "crates/labcolors-core/src/wcag22/q55_data.rs",
    "crates/labcolors-core/src/wcag22_evidence.rs",
    "crates/labcolors-core/src/numerics.rs",
    "crates/labcolors-core/contracts/wcag22-srgb8-v1.json",
    "crates/labcolors-core/contracts/wcag22-srgb8-q55-proof-v1.json",
    "crates/labcolors-core/Cargo.toml",
    "Cargo.lock",
    "crates/labcolors-core/benches/wcag22_feasibility_admission.rs",
    "scripts/check_wcag22_feasibility_benchmark.py",
];

struct CountingAllocator;

static ALLOCATION_CALLS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static DEALLOCATION_CALLS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);
static LIVE_BYTES: AtomicU64 = AtomicU64::new(0);
static PEAK_LIVE_BYTES: AtomicU64 = AtomicU64::new(0);

fn update_peak(candidate: u64) {
    let mut peak = PEAK_LIVE_BYTES.load(Ordering::Relaxed);
    while candidate > peak {
        match PEAK_LIVE_BYTES.compare_exchange_weak(
            peak,
            candidate,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(actual) => peak = actual,
        }
    }
}

fn record_allocation(bytes: usize) {
    let bytes = bytes as u64;
    ALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
    ALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    let live = LIVE_BYTES.fetch_add(bytes, Ordering::Relaxed) + bytes;
    update_peak(live);
}

fn record_deallocation(bytes: usize) {
    let bytes = bytes as u64;
    DEALLOCATION_CALLS.fetch_add(1, Ordering::Relaxed);
    DEALLOCATED_BYTES.fetch_add(bytes, Ordering::Relaxed);
    let previous = LIVE_BYTES.fetch_sub(bytes, Ordering::Relaxed);
    debug_assert!(
        previous >= bytes,
        "allocator live-byte accounting underflow"
    );
}

// SAFETY: every operation delegates to `System` with its original allocation
// contract. The atomics observe successful operations but never alter pointers,
// layouts, alignment, allocation failure, or deallocation order.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_allocation(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        record_deallocation(layout.size());
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let replacement = unsafe { System.realloc(pointer, layout, new_size) };
        if !replacement.is_null() {
            record_deallocation(layout.size());
            record_allocation(new_size);
        }
        replacement
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: CountingAllocator = CountingAllocator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shape {
    raw_relations: u64,
    raw_adjacent_entries: u64,
    opaque_utf8_bytes: u64,
    canonical_relations: u64,
    applicable_relations: u64,
    applicable_edges: u64,
}

#[derive(Debug)]
enum HarnessError {
    SampleCountNotUtf8,
    InvalidSampleCount {
        maximum: u8,
        source: std::num::ParseIntError,
    },
    SampleReservation {
        scenario: &'static str,
        sample_count: usize,
        source: std::collections::TryReserveError,
    },
    ShapeArithmeticOverflow,
    ConflictingScenarioRelation {
        relation_id: String,
    },
    MissingScenarioRelationKind {
        relation_id: String,
    },
    ShapeMismatch {
        scenario: &'static str,
        declared: Shape,
        actual: Shape,
    },
    MissingIdentity {
        scenario: &'static str,
    },
}

impl std::fmt::Display for HarnessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SampleCountNotUtf8 => {
                formatter.write_str("LABCOLORS_WCAG22_BENCH_SAMPLES is not UTF-8")
            }
            Self::InvalidSampleCount { maximum, .. } => write!(
                formatter,
                "LABCOLORS_WCAG22_BENCH_SAMPLES must be an integer in 1..={maximum}"
            ),
            Self::SampleReservation {
                scenario,
                sample_count,
                ..
            } => write!(
                formatter,
                "cannot reserve {sample_count} raw samples for scenario {scenario}"
            ),
            Self::ShapeArithmeticOverflow => {
                formatter.write_str("benchmark scenario shape arithmetic overflowed")
            }
            Self::ConflictingScenarioRelation { relation_id } => write!(
                formatter,
                "benchmark scenario relation {relation_id} has conflicting declarations"
            ),
            Self::MissingScenarioRelationKind { relation_id } => write!(
                formatter,
                "benchmark scenario relation {relation_id} has no public relation kind"
            ),
            Self::ShapeMismatch {
                scenario,
                declared,
                actual,
            } => write!(
                formatter,
                "scenario {scenario} declared shape {declared:?}, but built request has {actual:?}"
            ),
            Self::MissingIdentity { scenario } => {
                write!(
                    formatter,
                    "scenario {scenario} produced no measured identity"
                )
            }
        }
    }
}

impl Error for HarnessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidSampleCount { source, .. } => Some(source),
            Self::SampleReservation { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct SampleCount(NonZeroU8);

impl SampleCount {
    // Это граница представления протокола, а не статистический порог: harness
    // сохраняет каждое сырое наблюдение одновременно в памяти и JSON.
    const LOCAL_SMOKE: Self = Self(NonZeroU8::MIN);

    fn get(self) -> usize {
        usize::from(self.0.get())
    }
}

impl Shape {
    fn logical_assessments(self) -> u64 {
        CANDIDATE_COUNT * self.applicable_edges
    }

    fn packed_result_bytes(self) -> u64 {
        if self.applicable_relations == 0 {
            0
        } else {
            DECISION_SLOT_BYTES * (self.applicable_edges + 1)
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Terminal {
    Feasible,
    Infeasible,
    NotEvaluated,
}

impl Terminal {
    const fn key(self) -> &'static str {
        match self {
            Self::Feasible => "feasible",
            Self::Infeasible => "infeasible",
            Self::NotEvaluated => "not-evaluated",
        }
    }
}

struct Scenario {
    name: &'static str,
    shape: Shape,
    terminal: Terminal,
    feasible_candidates: Option<u64>,
    build: fn() -> Result<PreparedRequest, Box<dyn Error>>,
}

struct PreparedRequest {
    value: RequestV1,
    shape: Shape,
}

#[derive(Clone, PartialEq, Eq)]
struct Identity {
    terminal: Terminal,
    domain_digest: [u8; 32],
    relation_set_digest: [u8; 32],
    evaluation_id: Option<[u8; 32]>,
    logical_assessments: u64,
    assessment_iterator_len: u64,
    feasible_candidates: Option<u64>,
}

struct AllocatorSnapshot {
    allocation_calls: u64,
    allocated_bytes: u64,
    deallocation_calls: u64,
    deallocated_bytes: u64,
    baseline_live_bytes: u64,
    end_live_bytes: u64,
    peak_live_bytes: u64,
}

struct RawSample {
    index: usize,
    elapsed_ns: u128,
    allocator: AllocatorSnapshot,
}

struct ScenarioRun {
    scenario: &'static Scenario,
    identity: Identity,
    samples: Vec<RawSample>,
}

struct Protocol {
    sample_count: SampleCount,
    sample_count_explicit: bool,
}

struct GitMetadata {
    revision: String,
    tree: String,
    clean: bool,
    source_objects: Vec<(&'static str, &'static str, String)>,
}

fn relation_id(value: impl Into<String>) -> RelationId {
    RelationId::try_new(value).expect("benchmark relation IDs are non-empty")
}

fn occurrence_id(value: impl Into<String>) -> OccurrenceId {
    OccurrenceId::try_new(value).expect("benchmark occurrence IDs are non-empty")
}

fn applicable(
    relation: impl Into<String>,
    occurrence: impl Into<String>,
    adjacent: Vec<Srgb8>,
) -> RelationV1 {
    RelationV1::applicable(
        relation_id(relation),
        occurrence_id(occurrence),
        Wcag22CriterionV1::Sc143TextDefault,
        adjacent,
    )
    .expect("benchmark applicable relation has non-empty adjacency")
}

fn not_applicable(
    relation: impl Into<String>,
    occurrence: impl Into<String>,
    reason: String,
) -> RelationV1 {
    let declaration = Wcag22ClientDeclaredNotApplicableV1::try_new(reason)
        .expect("benchmark NotApplicable reason is non-empty");
    RelationV1::not_applicable(
        relation_id(relation),
        occurrence_id(occurrence),
        declaration,
    )
}

#[derive(PartialEq, Eq)]
enum CanonicalDeclaration {
    Applicable {
        occurrence_id: String,
        criterion: Wcag22CriterionV1,
        adjacent: Vec<Srgb8>,
    },
    NotApplicable {
        occurrence_id: String,
        reason_id: String,
    },
}

fn shape_count(value: usize) -> Result<u64, HarnessError> {
    u64::try_from(value).map_err(|_| HarnessError::ShapeArithmeticOverflow)
}

fn add_shape_count(target: &mut u64, value: u64) -> Result<(), HarnessError> {
    *target = target
        .checked_add(value)
        .ok_or(HarnessError::ShapeArithmeticOverflow)?;
    Ok(())
}

fn derive_shape(relations: &[RelationV1]) -> Result<Shape, HarnessError> {
    // Считаем только через публичные value-object API, не переиспользуя
    // внутренний preflight Core: так артефакт независимо связывает декларацию
    // Shape с тем же Vec, который затем без изменения переходит в RequestV1.
    let raw_relations = shape_count(relations.len())?;
    let mut raw_adjacent_entries = 0_u64;
    let mut opaque_utf8_bytes = 0_u64;
    let mut canonical_by_id = BTreeMap::new();

    for relation in relations {
        let relation_id = relation.relation_id().as_str();
        let occurrence_id = relation.occurrence_id().as_str();
        add_shape_count(&mut opaque_utf8_bytes, shape_count(relation_id.len())?)?;
        add_shape_count(&mut opaque_utf8_bytes, shape_count(occurrence_id.len())?)?;

        let declaration = if let Some((criterion, adjacent)) = relation.as_applicable() {
            add_shape_count(&mut raw_adjacent_entries, shape_count(adjacent.len())?)?;
            let mut adjacent = adjacent.to_vec();
            adjacent.sort_unstable();
            adjacent.dedup();
            CanonicalDeclaration::Applicable {
                occurrence_id: occurrence_id.to_owned(),
                criterion,
                adjacent,
            }
        } else if let Some(declaration) = relation.as_not_applicable() {
            add_shape_count(
                &mut opaque_utf8_bytes,
                shape_count(declaration.reason_id().len())?,
            )?;
            CanonicalDeclaration::NotApplicable {
                occurrence_id: occurrence_id.to_owned(),
                reason_id: declaration.reason_id().to_owned(),
            }
        } else {
            return Err(HarnessError::MissingScenarioRelationKind {
                relation_id: relation_id.to_owned(),
            });
        };

        match canonical_by_id.entry(relation_id.to_owned()) {
            Entry::Vacant(entry) => {
                entry.insert(declaration);
            }
            Entry::Occupied(entry) if entry.get() == &declaration => {}
            Entry::Occupied(_) => {
                return Err(HarnessError::ConflictingScenarioRelation {
                    relation_id: relation_id.to_owned(),
                });
            }
        }
    }

    let canonical_relations = shape_count(canonical_by_id.len())?;
    let mut applicable_relations = 0_u64;
    let mut applicable_edges = 0_u64;
    for declaration in canonical_by_id.values() {
        if let CanonicalDeclaration::Applicable { adjacent, .. } = declaration {
            add_shape_count(&mut applicable_relations, 1)?;
            add_shape_count(&mut applicable_edges, shape_count(adjacent.len())?)?;
        }
    }

    Ok(Shape {
        raw_relations,
        raw_adjacent_entries,
        opaque_utf8_bytes,
        canonical_relations,
        applicable_relations,
        applicable_edges,
    })
}

fn request(relations: Vec<RelationV1>) -> Result<PreparedRequest, Box<dyn Error>> {
    let shape = derive_shape(&relations)?;
    let value = RequestV1::try_new(
        DomainIdV1::Srgb8NeutralAxis,
        relations,
        ResourceProfileIdV1::Compile,
    )?;
    Ok(PreparedRequest { value, shape })
}

fn build_minimum_evaluated() -> Result<PreparedRequest, Box<dyn Error>> {
    request(vec![applicable("r", "o", vec![Srgb8::new([0x76; 3])])])
}

fn maximum_distinct_adjacent() -> Vec<Srgb8> {
    let mandatory = [
        Srgb8::new([0x00; 3]),
        Srgb8::new([0x76; 3]),
        Srgb8::new([0xFF; 3]),
    ];
    let mut adjacent = Vec::with_capacity(MAX_APPLICABLE_EDGES as usize);
    adjacent.extend(mandatory);
    for code in 0_u32..=0xFF_FFFF {
        if adjacent.len() == MAX_APPLICABLE_EDGES as usize {
            break;
        }
        let value = Srgb8::new([
            ((code >> 16) & 0xFF) as u8,
            ((code >> 8) & 0xFF) as u8,
            (code & 0xFF) as u8,
        ]);
        if !mandatory.contains(&value) {
            adjacent.push(value);
        }
    }
    assert_eq!(adjacent.len(), MAX_APPLICABLE_EDGES as usize);
    adjacent
}

fn build_maximum_applicable_edges() -> Result<PreparedRequest, Box<dyn Error>> {
    request(vec![applicable(
        "max-edges",
        "occurrence",
        maximum_distinct_adjacent(),
    )])
}

fn build_maximum_raw_duplicate_relations() -> Result<PreparedRequest, Box<dyn Error>> {
    let duplicate = applicable("duplicate", "same", vec![Srgb8::new([0x76; 3])]);
    request(vec![duplicate; MAX_APPLICABLE_EDGES as usize])
}

fn build_maximum_raw_adjacent_duplicates() -> Result<PreparedRequest, Box<dyn Error>> {
    request(vec![applicable(
        "r",
        "o",
        vec![Srgb8::new([0x76; 3]); MAX_APPLICABLE_EDGES as usize],
    )])
}

fn build_maximum_canonical_applicable_relations() -> Result<PreparedRequest, Box<dyn Error>> {
    let relations = (0..MAX_APPLICABLE_EDGES)
        .map(|index| {
            applicable(
                format!("r{index:04}"),
                format!("o{index:04}"),
                vec![Srgb8::new([0x76; 3])],
            )
        })
        .collect();
    request(relations)
}

fn build_maximum_combined_applicable_envelope() -> Result<PreparedRequest, Box<dyn Error>> {
    let relations = (0..MAX_APPLICABLE_EDGES)
        .map(|index| {
            let extra_bytes = if index < 32 { 23 } else { 22 };
            applicable(
                format!("r{index:04}"),
                format!("o{index:04}{}", "x".repeat(extra_bytes)),
                vec![Srgb8::new([0x76; 3])],
            )
        })
        .collect();
    request(relations)
}

fn build_maximum_canonical_not_applicable_relations() -> Result<PreparedRequest, Box<dyn Error>> {
    let relations = (0..MAX_APPLICABLE_EDGES)
        .map(|index| {
            not_applicable(
                format!("r{index:04}"),
                format!("o{index:04}"),
                "not-wcag".to_owned(),
            )
        })
        .collect();
    request(relations)
}

fn build_maximum_combined_not_applicable_envelope() -> Result<PreparedRequest, Box<dyn Error>> {
    let relations = (0..MAX_APPLICABLE_EDGES)
        .map(|index| {
            let extra_bytes = if index < 32 { 15 } else { 14 };
            not_applicable(
                format!("r{index:04}"),
                format!("o{index:04}"),
                format!("not-wcag{}", "x".repeat(extra_bytes)),
            )
        })
        .collect();
    request(relations)
}

fn build_maximum_mixed_relations() -> Result<PreparedRequest, Box<dyn Error>> {
    let applicable_count = MAX_APPLICABLE_EDGES / 2;
    let mut relations = Vec::with_capacity(MAX_APPLICABLE_EDGES as usize);
    relations.extend((0..applicable_count).map(|index| {
        applicable(
            format!("a{index:04}"),
            format!("o{index:04}"),
            vec![Srgb8::new([0x76; 3])],
        )
    }));
    relations.extend((0..(MAX_APPLICABLE_EDGES - applicable_count)).map(|index| {
        not_applicable(
            format!("n{index:04}"),
            format!("o{index:04}"),
            "not-wcag".to_owned(),
        )
    }));
    request(relations)
}

fn build_maximum_opaque_utf8_bytes() -> Result<PreparedRequest, Box<dyn Error>> {
    let reason = "x".repeat(PAGE_BYTES as usize - 2);
    request(vec![not_applicable("r", "o", reason)])
}

static SCENARIOS: [Scenario; 10] = [
    Scenario {
        name: "minimum-evaluated",
        shape: Shape {
            raw_relations: 1,
            raw_adjacent_entries: 1,
            opaque_utf8_bytes: 2,
            canonical_relations: 1,
            applicable_relations: 1,
            applicable_edges: 1,
        },
        terminal: Terminal::Feasible,
        feasible_candidates: Some(7),
        build: build_minimum_evaluated,
    },
    Scenario {
        name: "maximum-applicable-edges",
        shape: Shape {
            raw_relations: 1,
            raw_adjacent_entries: MAX_APPLICABLE_EDGES,
            opaque_utf8_bytes: 19,
            canonical_relations: 1,
            applicable_relations: 1,
            applicable_edges: MAX_APPLICABLE_EDGES,
        },
        terminal: Terminal::Infeasible,
        feasible_candidates: Some(0),
        build: build_maximum_applicable_edges,
    },
    Scenario {
        name: "maximum-raw-duplicate-relations",
        shape: Shape {
            raw_relations: MAX_APPLICABLE_EDGES,
            raw_adjacent_entries: MAX_APPLICABLE_EDGES,
            opaque_utf8_bytes: 13 * MAX_APPLICABLE_EDGES,
            canonical_relations: 1,
            applicable_relations: 1,
            applicable_edges: 1,
        },
        terminal: Terminal::Feasible,
        feasible_candidates: Some(7),
        build: build_maximum_raw_duplicate_relations,
    },
    Scenario {
        name: "maximum-raw-adjacent-duplicates",
        shape: Shape {
            raw_relations: 1,
            raw_adjacent_entries: MAX_APPLICABLE_EDGES,
            opaque_utf8_bytes: 2,
            canonical_relations: 1,
            applicable_relations: 1,
            applicable_edges: 1,
        },
        terminal: Terminal::Feasible,
        feasible_candidates: Some(7),
        build: build_maximum_raw_adjacent_duplicates,
    },
    Scenario {
        name: "maximum-canonical-applicable-relations",
        shape: Shape {
            raw_relations: MAX_APPLICABLE_EDGES,
            raw_adjacent_entries: MAX_APPLICABLE_EDGES,
            opaque_utf8_bytes: 10 * MAX_APPLICABLE_EDGES,
            canonical_relations: MAX_APPLICABLE_EDGES,
            applicable_relations: MAX_APPLICABLE_EDGES,
            applicable_edges: MAX_APPLICABLE_EDGES,
        },
        terminal: Terminal::Feasible,
        feasible_candidates: Some(7),
        build: build_maximum_canonical_applicable_relations,
    },
    Scenario {
        name: "maximum-combined-applicable-envelope",
        shape: Shape {
            raw_relations: MAX_APPLICABLE_EDGES,
            raw_adjacent_entries: MAX_APPLICABLE_EDGES,
            opaque_utf8_bytes: PAGE_BYTES,
            canonical_relations: MAX_APPLICABLE_EDGES,
            applicable_relations: MAX_APPLICABLE_EDGES,
            applicable_edges: MAX_APPLICABLE_EDGES,
        },
        terminal: Terminal::Feasible,
        feasible_candidates: Some(7),
        build: build_maximum_combined_applicable_envelope,
    },
    Scenario {
        name: "maximum-canonical-not-applicable-relations",
        shape: Shape {
            raw_relations: MAX_APPLICABLE_EDGES,
            raw_adjacent_entries: 0,
            opaque_utf8_bytes: 18 * MAX_APPLICABLE_EDGES,
            canonical_relations: MAX_APPLICABLE_EDGES,
            applicable_relations: 0,
            applicable_edges: 0,
        },
        terminal: Terminal::NotEvaluated,
        feasible_candidates: None,
        build: build_maximum_canonical_not_applicable_relations,
    },
    Scenario {
        name: "maximum-combined-not-applicable-envelope",
        shape: Shape {
            raw_relations: MAX_APPLICABLE_EDGES,
            raw_adjacent_entries: 0,
            opaque_utf8_bytes: PAGE_BYTES,
            canonical_relations: MAX_APPLICABLE_EDGES,
            applicable_relations: 0,
            applicable_edges: 0,
        },
        terminal: Terminal::NotEvaluated,
        feasible_candidates: None,
        build: build_maximum_combined_not_applicable_envelope,
    },
    Scenario {
        name: "maximum-mixed-relations",
        shape: Shape {
            raw_relations: MAX_APPLICABLE_EDGES,
            raw_adjacent_entries: MAX_APPLICABLE_EDGES / 2,
            opaque_utf8_bytes: 28_662,
            canonical_relations: MAX_APPLICABLE_EDGES,
            applicable_relations: MAX_APPLICABLE_EDGES / 2,
            applicable_edges: MAX_APPLICABLE_EDGES / 2,
        },
        terminal: Terminal::Feasible,
        feasible_candidates: Some(7),
        build: build_maximum_mixed_relations,
    },
    Scenario {
        name: "maximum-opaque-utf8-bytes",
        shape: Shape {
            raw_relations: 1,
            raw_adjacent_entries: 0,
            opaque_utf8_bytes: PAGE_BYTES,
            canonical_relations: 1,
            applicable_relations: 0,
            applicable_edges: 0,
        },
        terminal: Terminal::NotEvaluated,
        feasible_candidates: None,
        build: build_maximum_opaque_utf8_bytes,
    },
];

fn begin_allocator_sample() -> u64 {
    ALLOCATION_CALLS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    DEALLOCATION_CALLS.store(0, Ordering::Relaxed);
    DEALLOCATED_BYTES.store(0, Ordering::Relaxed);
    let baseline = LIVE_BYTES.load(Ordering::Relaxed);
    PEAK_LIVE_BYTES.store(baseline, Ordering::Relaxed);
    baseline
}

fn end_allocator_sample(baseline_live_bytes: u64) -> AllocatorSnapshot {
    AllocatorSnapshot {
        allocation_calls: ALLOCATION_CALLS.load(Ordering::Relaxed),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
        deallocation_calls: DEALLOCATION_CALLS.load(Ordering::Relaxed),
        deallocated_bytes: DEALLOCATED_BYTES.load(Ordering::Relaxed),
        baseline_live_bytes,
        end_live_bytes: LIVE_BYTES.load(Ordering::Relaxed),
        peak_live_bytes: PEAK_LIVE_BYTES.load(Ordering::Relaxed),
    }
}

fn observe(result: &FeasibilityV1) -> Identity {
    if let Some(evaluated) = result.evaluated() {
        let terminal = if result.is_feasible() {
            Terminal::Feasible
        } else {
            Terminal::Infeasible
        };
        Identity {
            terminal,
            domain_digest: *evaluated.domain_digest().as_bytes(),
            relation_set_digest: *evaluated.relation_set_digest().as_bytes(),
            evaluation_id: Some(*evaluated.evaluation_id().as_bytes()),
            logical_assessments: evaluated.proof().logical_assessments(),
            assessment_iterator_len: evaluated.assessments().len() as u64,
            feasible_candidates: Some(evaluated.feasible_candidates().count() as u64),
        }
    } else {
        let not_evaluated = result
            .not_evaluated()
            .expect("terminal must be evaluated or NotEvaluated");
        Identity {
            terminal: Terminal::NotEvaluated,
            domain_digest: *not_evaluated.domain_digest().as_bytes(),
            relation_set_digest: *not_evaluated.relation_set_digest().as_bytes(),
            evaluation_id: None,
            logical_assessments: 0,
            assessment_iterator_len: 0,
            feasible_candidates: None,
        }
    }
}

fn run_scenario(
    scenario: &'static Scenario,
    sample_count: SampleCount,
) -> Result<ScenarioRun, Box<dyn Error>> {
    let mut identity: Option<Identity> = None;
    let sample_count = sample_count.get();
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(sample_count)
        .map_err(|source| HarnessError::SampleReservation {
            scenario: scenario.name,
            sample_count,
            source,
        })?;
    for index in 0..sample_count {
        // Request construction is intentionally outside the sample: callers
        // already own this input. Evaluation still includes raw preflight,
        // canonicalization, exact allocation, all pair calls and proof hashing.
        let prepared = (scenario.build)()?;
        if prepared.shape != scenario.shape {
            return Err(HarnessError::ShapeMismatch {
                scenario: scenario.name,
                declared: scenario.shape,
                actual: prepared.shape,
            }
            .into());
        }
        let request = black_box(prepared.value);
        let baseline_live_bytes = begin_allocator_sample();
        let started = Instant::now();
        let result = evaluate(request)?;
        let elapsed_ns = started.elapsed().as_nanos();
        let allocator = end_allocator_sample(baseline_live_bytes);
        let observed = observe(black_box(&result));

        if observed.terminal != scenario.terminal
            || observed.logical_assessments != scenario.shape.logical_assessments()
            || observed.assessment_iterator_len != scenario.shape.logical_assessments()
            || observed.feasible_candidates != scenario.feasible_candidates
        {
            return Err(format!("scenario {} violated its bound identity", scenario.name).into());
        }
        if let Some(previous) = &identity {
            if previous != &observed {
                return Err(format!(
                    "scenario {} identity drifted between samples",
                    scenario.name
                )
                .into());
            }
        } else {
            identity = Some(observed);
        }
        samples.push(RawSample {
            index,
            elapsed_ns,
            allocator,
        });
    }
    Ok(ScenarioRun {
        scenario,
        identity: identity.ok_or(HarnessError::MissingIdentity {
            scenario: scenario.name,
        })?,
        samples,
    })
}

fn parse_protocol() -> Result<Protocol, Box<dyn Error>> {
    let Some(value) = std::env::var_os("LABCOLORS_WCAG22_BENCH_SAMPLES") else {
        // One raw observation is a convenient local smoke default, not an
        // admission threshold. A committed protocol must set the count.
        return Ok(Protocol {
            sample_count: SampleCount::LOCAL_SMOKE,
            sample_count_explicit: false,
        });
    };
    let value = value
        .into_string()
        .map_err(|_| HarnessError::SampleCountNotUtf8)?;
    let parsed = value
        .parse::<NonZeroU8>()
        .map(SampleCount)
        .map_err(|source| HarnessError::InvalidSampleCount {
            maximum: u8::MAX,
            source,
        })?;
    Ok(Protocol {
        sample_count: parsed,
        sample_count_explicit: true,
    })
}

fn command_output(mut command: Command) -> String {
    match command.output() {
        Ok(output) if output.status.success() => {
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        _ => "unavailable".to_owned(),
    }
}

fn repository_root() -> PathBuf {
    let manifest = Path::new(option_env!("CARGO_MANIFEST_DIR").unwrap_or("."));
    manifest
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn git_metadata() -> GitMetadata {
    let root = repository_root();
    let mut revision = Command::new("git");
    revision.current_dir(&root).args(["rev-parse", "HEAD"]);
    let revision = command_output(revision);

    let mut tree = Command::new("git");
    tree.current_dir(&root).args(["rev-parse", "HEAD^{tree}"]);
    let tree = command_output(tree);

    let mut status = Command::new("git");
    status
        .current_dir(&root)
        .args(["status", "--porcelain", "--untracked-files=normal"]);
    let clean = match status.output() {
        Ok(output) if output.status.success() => output.stdout.is_empty(),
        _ => false,
    };
    let source_objects: Vec<(&'static str, &'static str, String)> = SOURCE_OBJECTS
        .iter()
        .map(|&(name, path)| {
            let mut object = Command::new("git");
            object
                .current_dir(&root)
                .args(["rev-parse", &format!("HEAD:{path}")]);
            (name, path, command_output(object))
        })
        .collect();
    let complete_revision = clean
        && source_objects
            .iter()
            .all(|(_, _, object)| object != "unavailable");
    GitMetadata {
        revision: if complete_revision {
            revision
        } else {
            "unavailable".to_owned()
        },
        tree: if complete_revision {
            tree
        } else {
            "unavailable".to_owned()
        },
        clean,
        source_objects,
    }
}

fn rustc_verbose() -> String {
    let executable = std::env::var_os("RUSTC")
        .or_else(|| option_env!("RUSTC").map(Into::into))
        .unwrap_or_else(|| "rustc".into());
    let mut command = Command::new(executable);
    command.arg("-Vv");
    command_output(command)
}

fn subject_manifest() -> Result<Vec<(&'static str, [u8; 32])>, std::io::Error> {
    let root = repository_root();
    SUBJECT_PATHS
        .iter()
        .map(|&path| {
            let bytes = fs::read(root.join(path))?;
            Ok((path, *subject_sha256::digest(&bytes).as_bytes()))
        })
        .collect()
}

fn push_json_string(output: &mut String, value: &str) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value if value <= '\u{1F}' => {
                write!(output, "\\u{:04x}", value as u32).expect("String writes cannot fail");
            }
            value => output.push(value),
        }
    }
    output.push('"');
}

fn hex(bytes: &[u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("String writes cannot fail");
    }
    output
}

fn render_json(runs: &[ScenarioRun], protocol: &Protocol) -> Result<String, std::io::Error> {
    let git = git_metadata();
    let subjects = subject_manifest()?;
    let rustc = rustc_verbose();
    let profile = ResourceProfileIdV1::Compile;
    let mut output = String::new();
    output.push_str("{\n  \"schemaVersion\": 1,\n  \"artifactId\": ");
    push_json_string(&mut output, ARTIFACT_ID);
    output.push_str(
        ",\n  \"claimBoundary\": \"native-process-observations-and-page-slot-arithmetic-only\",\n",
    );
    output.push_str("  \"notMeasured\": [\"webassembly-runtime-memory\", \"serialized-output-size\", \"client-latency\"],\n");
    output.push_str(
        "  \"environment\": {\n    \"execution\": \"native-process\",\n    \"targetArch\": ",
    );
    push_json_string(&mut output, std::env::consts::ARCH);
    output.push_str(",\n    \"targetOs\": ");
    push_json_string(&mut output, std::env::consts::OS);
    write!(
        output,
        ",\n    \"pointerWidthBits\": {},\n    \"debugAssertions\": {},\n    \"packageVersion\": ",
        usize::BITS,
        cfg!(debug_assertions),
    )
    .expect("String writes cannot fail");
    push_json_string(
        &mut output,
        option_env!("CARGO_PKG_VERSION").unwrap_or("unknown"),
    );
    output.push_str(",\n    \"allocator\": \"std::alloc::System\",\n    \"allocatorInstrumentationIncludedInElapsedTime\": true,\n    \"timer\": \"std::time::Instant\",\n    \"measurementThreads\": 1,\n    \"requestConstructionMeasured\": false,\n    \"rustcVerbose\": ");
    push_json_string(&mut output, &rustc);
    output.push_str(",\n    \"gitRevision\": ");
    push_json_string(&mut output, &git.revision);
    output.push_str(",\n    \"gitTree\": ");
    push_json_string(&mut output, &git.tree);
    write!(
        output,
        ",\n    \"sourceTreeClean\": {},\n    \"sampleCountExplicit\": {},\n    \"sourceObjects\": {{\n",
        git.clean, protocol.sample_count_explicit,
    )
    .expect("String writes cannot fail");
    for (index, (name, path, object)) in git.source_objects.iter().enumerate() {
        if index != 0 {
            output.push_str(",\n");
        }
        output.push_str("      ");
        push_json_string(&mut output, name);
        output.push_str(": {\"path\": ");
        push_json_string(&mut output, path);
        output.push_str(", \"gitObject\": ");
        push_json_string(&mut output, object);
        output.push('}');
    }
    output.push_str("\n    }\n  },\n");
    output.push_str("  \"warmupSamples\": 0,\n  \"scenarioOrder\": \"as-emitted\",\n");
    write!(
        output,
        "  \"sampleCount\": {},\n  \"admissionStatus\": \"measurement-only-unless-admission-check-passes\",\n  \"hardSlo\": {{\n    \"class\": \"deterministic-work-and-storage\",\n    \"logicalAssessmentLaw\": \"W=256E\",\n    \"packedStorageLaw\": \"B=0 if A=0, otherwise B=32(E+1)\",\n    \"partialTerminalAllowed\": false,\n    \"timingThresholdNs\": null,\n    \"allRequiredShapesMustComplete\": true\n  }},\n  \"boundedEnvelopeModel\": {{\n    \"scope\": \"product-policy-capacity-arithmetic-not-total-memory\",\n    \"referenceBoundedBytes\": {PAGE_BYTES},\n    \"candidateCount\": {CANDIDATE_COUNT},\n    \"decisionSlotBytes\": {DECISION_SLOT_BYTES},\n    \"partitionBytes\": {PARTITION_BYTES},\n    \"reservedPartitionSlots\": 1,\n    \"maximumCardinality\": {MAX_APPLICABLE_EDGES},\n    \"maximumLogicalAssessments\": {},\n    \"maximumPackedResultBytes\": {}\n  }},\n",
        protocol.sample_count.get(),
        CANDIDATE_COUNT * MAX_APPLICABLE_EDGES,
        DECISION_SLOT_BYTES * (MAX_APPLICABLE_EDGES + 1),
    )
    .expect("String writes cannot fail");
    output.push_str("  \"subjectManifest\": [\n");
    for (index, (path, sha256)) in subjects.iter().enumerate() {
        if index != 0 {
            output.push_str(",\n");
        }
        output.push_str("    {\"path\": ");
        push_json_string(&mut output, path);
        output.push_str(", \"sha256\": ");
        push_json_string(&mut output, &hex(sha256));
        output.push('}');
    }
    output.push_str("\n  ],\n");
    write!(
        output,
        "  \"profileLimits\": {{\n    \"profileId\": \"{}\",\n    \"rawRelations\": {},\n    \"rawAdjacentEntries\": {},\n    \"opaqueUtf8Bytes\": {},\n    \"canonicalRelations\": {},\n    \"applicableEdges\": {},\n    \"logicalAssessments\": {},\n    \"packedResultBytes\": {}\n  }},\n",
        profile.key(),
        profile.limit(ResourceDimensionV1::RawRelations),
        profile.limit(ResourceDimensionV1::RawAdjacentEntries),
        profile.limit(ResourceDimensionV1::OpaqueUtf8Bytes),
        profile.limit(ResourceDimensionV1::CanonicalRelations),
        profile.limit(ResourceDimensionV1::ApplicableEdges),
        profile.limit(ResourceDimensionV1::LogicalAssessments),
        profile.limit(ResourceDimensionV1::PackedResultBytes),
    )
    .expect("String writes cannot fail");
    output.push_str("  \"scenarios\": [\n");
    for (run_index, run) in runs.iter().enumerate() {
        if run_index != 0 {
            output.push_str(",\n");
        }
        let scenario = run.scenario;
        let shape = scenario.shape;
        output.push_str("    {\n      \"name\": ");
        push_json_string(&mut output, scenario.name);
        write!(
            output,
            ",\n      \"shape\": {{\n        \"rawRelations\": {},\n        \"rawAdjacentEntries\": {},\n        \"opaqueUtf8Bytes\": {},\n        \"canonicalRelations\": {},\n        \"applicableRelations\": {},\n        \"applicableEdges\": {}\n      }},\n      \"expected\": {{\n        \"terminal\": \"{}\",\n        \"logicalAssessments\": {},\n        \"packedResultBytes\": {},\n        \"feasibleCandidates\": ",
            shape.raw_relations,
            shape.raw_adjacent_entries,
            shape.opaque_utf8_bytes,
            shape.canonical_relations,
            shape.applicable_relations,
            shape.applicable_edges,
            scenario.terminal.key(),
            shape.logical_assessments(),
            shape.packed_result_bytes(),
        )
        .expect("String writes cannot fail");
        match scenario.feasible_candidates {
            Some(value) => write!(output, "{value}").expect("String writes cannot fail"),
            None => output.push_str("null"),
        }
        output.push_str("\n      },\n      \"observedIdentity\": {\n        \"terminal\": ");
        push_json_string(&mut output, run.identity.terminal.key());
        output.push_str(",\n        \"domainDigestSha256\": ");
        push_json_string(&mut output, &hex(&run.identity.domain_digest));
        output.push_str(",\n        \"relationSetDigestSha256\": ");
        push_json_string(&mut output, &hex(&run.identity.relation_set_digest));
        output.push_str(",\n        \"evaluationIdSha256\": ");
        match run.identity.evaluation_id {
            Some(value) => push_json_string(&mut output, &hex(&value)),
            None => output.push_str("null"),
        }
        write!(
            output,
            ",\n        \"logicalAssessments\": {},\n        \"assessmentIteratorLen\": {},\n        \"derivedPackedResultBytes\": {},\n        \"feasibleCandidates\": ",
            run.identity.logical_assessments,
            run.identity.assessment_iterator_len,
            shape.packed_result_bytes(),
        )
        .expect("String writes cannot fail");
        match run.identity.feasible_candidates {
            Some(value) => write!(output, "{value}").expect("String writes cannot fail"),
            None => output.push_str("null"),
        }
        output.push_str("\n      },\n      \"samples\": [\n");
        for (sample_index, sample) in run.samples.iter().enumerate() {
            if sample_index != 0 {
                output.push_str(",\n");
            }
            let allocator = &sample.allocator;
            write!(
                output,
                "        {{\"index\": {}, \"elapsedNs\": {}, \"allocationCalls\": {}, \"allocatedBytes\": {}, \"deallocationCalls\": {}, \"deallocatedBytes\": {}, \"baselineLiveBytes\": {}, \"endLiveBytes\": {}, \"peakLiveBytes\": {}, \"peakAdditionalLiveBytes\": {}}}",
                sample.index,
                sample.elapsed_ns,
                allocator.allocation_calls,
                allocator.allocated_bytes,
                allocator.deallocation_calls,
                allocator.deallocated_bytes,
                allocator.baseline_live_bytes,
                allocator.end_live_bytes,
                allocator.peak_live_bytes,
                allocator
                    .peak_live_bytes
                    .saturating_sub(allocator.baseline_live_bytes),
            )
            .expect("String writes cannot fail");
        }
        output.push_str("\n      ]\n    }");
    }
    output.push_str("\n  ]\n}\n");
    Ok(output)
}

fn main() -> Result<(), Box<dyn Error>> {
    assert_eq!(MAX_APPLICABLE_EDGES, 2_047);
    assert_eq!(
        ResourceProfileIdV1::Compile.limit(ResourceDimensionV1::ApplicableEdges),
        MAX_APPLICABLE_EDGES
    );
    let protocol = parse_protocol()?;
    let mut runs = Vec::with_capacity(SCENARIOS.len());
    for scenario in &SCENARIOS {
        runs.push(run_scenario(scenario, protocol.sample_count)?);
    }

    let output_path = std::env::var_os("LABCOLORS_WCAG22_BENCH_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(DEFAULT_OUTPUT_FILENAME));
    let payload = render_json(&runs, &protocol)?;
    fs::write(&output_path, payload)?;
    println!(
        "wrote {} raw scenarios to {}",
        runs.len(),
        output_path.display()
    );
    Ok(())
}
