//! Admission of correlated point-sRGB8 observations.
//!
//! This module owns canonicalization and a linear prepare transaction only.
//! The F2 Session is the sole production owner of the current payload and the
//! only code allowed to bind an admitted observation to evaluator evidence.

use core::ops::Range;
use std::rc::Rc;

use crate::appearance::SurfaceInputPortId;
use crate::lcs_occurrence::ColorSignal;

/// Stable compile-time identity of one atomic observation boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ObservationGroupId(u32);

impl ObservationGroupId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Runtime instance/epoch of one atomic observation stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ObservationStreamId(u32);

impl ObservationStreamId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Monotonic revision inside one [`ObservationStreamId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Revision(u64);

impl Revision {
    pub(crate) const fn new(raw: u64) -> Self {
        Self(raw)
    }
}

/// Opaque provenance of one simultaneously observed tuple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScenarioId(u32);

impl ScenarioId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Opaque reason why the current observation is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct UnknownReasonId(u32);

impl UnknownReasonId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// One raw surface-input binding inside a correlated scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SurfaceInputBinding {
    pub(crate) port: SurfaceInputPortId,
    pub(crate) value: ColorSignal,
}

impl SurfaceInputBinding {
    pub(crate) const fn new(port: SurfaceInputPortId, value: ColorSignal) -> Self {
        Self { port, value }
    }
}

/// Raw tuple: every binding was observed simultaneously.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScenarioInput {
    pub(crate) id: ScenarioId,
    pub(crate) bindings: Vec<SurfaceInputBinding>,
}

/// Raw scenario collection before schema validation and canonicalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedScenarioSetInput {
    pub(crate) scenarios: Vec<ScenarioInput>,
}

/// Raw payload of one revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObservationPayloadInput {
    Scenarios(ObservedScenarioSetInput),
    Unknown(UnknownReasonId),
}

/// Atomic update of one stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservationUpdateInput {
    pub(crate) stream: ObservationStreamId,
    pub(crate) revision: Revision,
    pub(crate) payload: ObservationPayloadInput,
}

/// One unique physical tuple inside the shared canonical backing.
///
/// Values are stored once in schema order. The schema remains attached to the
/// backing itself instead of being repeated beside every value.
#[derive(Debug, PartialEq, Eq)]
struct PhysicalScenario {
    values: Range<usize>,
    provenance: Range<usize>,
}

/// First canonical ordinal where an observation's intrinsic keyed schema and a
/// compiled consumer schema differ. `None` represents an exhausted side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ObservationSchemaMismatchV1 {
    case_index: usize,
    binding_index: usize,
    expected: Option<SurfaceInputPortId>,
    actual: Option<SurfaceInputPortId>,
}

impl ObservationSchemaMismatchV1 {
    pub(crate) const fn new(
        case_index: usize,
        binding_index: usize,
        expected: Option<SurfaceInputPortId>,
        actual: Option<SurfaceInputPortId>,
    ) -> Self {
        Self {
            case_index,
            binding_index,
            expected,
            actual,
        }
    }
}

/// Canonical nonempty correlated set. Values and provenance each use one flat
/// allocation; a physical case owns only ranges into those arrays.
#[derive(Debug, PartialEq, Eq)]
struct ObservedScenarioSet {
    cases: Box<[PhysicalScenario]>,
    values: Box<[ColorSignal]>,
    provenance: Box<[ScenarioId]>,
}

impl ObservedScenarioSet {
    fn values(&self, case_index: usize) -> Option<&[ColorSignal]> {
        let values = &self.cases.get(case_index)?.values;
        self.values.get(values.start..values.end)
    }

    fn provenance(&self, case_index: usize) -> Option<&[ScenarioId]> {
        let provenance = &self.cases.get(case_index)?.provenance;
        self.provenance.get(provenance.start..provenance.end)
    }
}

/// Canonical immutable schema shared by the compiled recheck and every
/// admitted observation backing created for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CanonicalObservationSchemaV1(Rc<[SurfaceInputPortId]>);

impl CanonicalObservationSchemaV1 {
    pub(crate) fn as_slice(&self) -> &[SurfaceInputPortId] {
        &self.0
    }

    fn shares_backing_with(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }

    #[cfg(test)]
    pub(crate) fn backing_ptr_for_test(&self) -> *const SurfaceInputPortId {
        self.0.as_ptr()
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ObservationBackingV1 {
    schema: CanonicalObservationSchemaV1,
    set: ObservedScenarioSet,
}

/// Sealed observation admitted against the Session-owned compiled schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RevisionBoundObservationV1 {
    stream: ObservationStreamId,
    revision: Revision,
    backing: Rc<ObservationBackingV1>,
}

impl RevisionBoundObservationV1 {
    pub(crate) const fn stream(&self) -> ObservationStreamId {
        self.stream
    }

    pub(crate) const fn revision(&self) -> Revision {
        self.revision
    }

    pub(crate) fn schema(&self) -> &[SurfaceInputPortId] {
        self.backing.schema.as_slice()
    }

    pub(crate) fn physical_case_count(&self) -> usize {
        self.backing.set.cases.len()
    }

    pub(crate) fn physical_values(&self, case_index: usize) -> Option<&[ColorSignal]> {
        self.backing.set.values(case_index)
    }

    pub(crate) fn provenance(&self, case_index: usize) -> Option<&[ScenarioId]> {
        self.backing.set.provenance(case_index)
    }

    pub(crate) fn validate_surface_schema(
        &self,
        expected: &[SurfaceInputPortId],
    ) -> Result<(), ObservationSchemaMismatchV1> {
        let actual = self.schema();
        let schema_len = expected.len().max(actual.len());
        for binding_index in 0..schema_len {
            let expected_input = expected.get(binding_index).copied();
            let actual_input = actual.get(binding_index).copied();
            if expected_input != actual_input {
                return Err(ObservationSchemaMismatchV1::new(
                    0,
                    binding_index,
                    expected_input,
                    actual_input,
                ));
            }
        }
        Ok(())
    }

    pub(crate) fn shares_schema_backing_with(
        &self,
        expected: &CanonicalObservationSchemaV1,
    ) -> bool {
        self.backing.schema.shares_backing_with(expected)
    }

    pub(crate) fn is_same_binding_as(&self, other: &Self) -> bool {
        self.stream == other.stream
            && self.revision == other.revision
            && Rc::ptr_eq(&self.backing, &other.backing)
    }

    fn has_canonical_input(
        &self,
        schema: &CanonicalObservationSchemaV1,
        scenarios: &[ScenarioInput],
    ) -> bool {
        &self.backing.schema == schema && canonical_input_matches_set(&self.backing.set, scenarios)
    }

    #[cfg(test)]
    pub(crate) fn backing_ptr_for_test(&self) -> *const () {
        Rc::as_ptr(&self.backing).cast()
    }

    #[cfg(test)]
    pub(crate) fn schema_ptr_for_test(&self) -> *const SurfaceInputPortId {
        self.backing.schema.backing_ptr_for_test()
    }
}

/// Revision-bound current `Unknown` payload. It contains no previous evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RevisionBoundUnknownV1 {
    stream: ObservationStreamId,
    revision: Revision,
    reason: UnknownReasonId,
}

impl RevisionBoundUnknownV1 {
    pub(crate) const fn stream(self) -> ObservationStreamId {
        self.stream
    }

    pub(crate) const fn revision(self) -> Revision {
        self.revision
    }

    pub(crate) const fn reason(self) -> UnknownReasonId {
        self.reason
    }
}

/// Allocation-free read projection of the one current payload owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObservationHeadViewV1<'owner> {
    Empty,
    Unknown(&'owner RevisionBoundUnknownV1),
    Observed(&'owner RevisionBoundObservationV1),
}

impl ObservationHeadViewV1<'_> {
    pub(crate) const fn revision(self) -> Option<Revision> {
        match self {
            Self::Empty => None,
            Self::Unknown(unknown) => Some(unknown.revision),
            Self::Observed(observation) => Some(observation.revision),
        }
    }
}

/// Implemented only by the closed Session state and test-only admission owner.
/// The transaction borrows this owner linearly through evaluation and commit.
pub(crate) trait ObservationOwnerV1 {
    fn observation_head(&self) -> ObservationHeadViewV1<'_>;
}

/// Typed admission failures. Preparing an update never mutates its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObservationError {
    EmptyCompiledSurfaceInputSchema,
    DuplicateCompiledSurfaceInputPort {
        input: SurfaceInputPortId,
    },
    StreamMismatch {
        expected: ObservationStreamId,
        actual: ObservationStreamId,
    },
    EmptyScenarioSet,
    DuplicateScenarioId {
        scenario: ScenarioId,
    },
    DuplicateSurfaceInputBinding {
        scenario: ScenarioId,
        input: SurfaceInputPortId,
    },
    MissingSurfaceInputBinding {
        scenario: ScenarioId,
        input: SurfaceInputPortId,
    },
    UnexpectedSurfaceInputBinding {
        scenario: ScenarioId,
        input: SurfaceInputPortId,
    },
    RevisionOutOfOrder {
        current: Revision,
        incoming: Revision,
    },
    RevisionConflict {
        revision: Revision,
    },
    ResourceExhausted,
}

/// Exact replay performed no state transition.
pub(crate) struct PreparedIdempotentV1<'owner, Owner> {
    owner: &'owner mut Owner,
}

impl<'owner, Owner> PreparedIdempotentV1<'owner, Owner> {
    pub(crate) fn into_owner(self) -> &'owner mut Owner {
        self.owner
    }
}

/// Higher-revision admitted `Unknown`, still uncommitted.
pub(crate) struct PreparedUnknownV1<'owner, Owner> {
    owner: &'owner mut Owner,
    unknown: RevisionBoundUnknownV1,
}

impl<'owner, Owner> PreparedUnknownV1<'owner, Owner> {
    #[cfg(test)]
    pub(crate) const fn unknown(&self) -> RevisionBoundUnknownV1 {
        self.unknown
    }

    pub(crate) fn into_parts(self) -> (&'owner mut Owner, RevisionBoundUnknownV1) {
        (self.owner, self.unknown)
    }
}

/// Higher-revision admitted observation, still uncommitted and unbound to any
/// evaluator result.
pub(crate) struct PreparedObservedV1<'owner, Owner> {
    owner: &'owner mut Owner,
    observation: RevisionBoundObservationV1,
}

impl<'owner, Owner> PreparedObservedV1<'owner, Owner> {
    #[cfg(test)]
    pub(crate) const fn observation(&self) -> &RevisionBoundObservationV1 {
        &self.observation
    }

    pub(crate) fn into_parts(self) -> (&'owner mut Owner, RevisionBoundObservationV1) {
        (self.owner, self.observation)
    }
}

/// Linear prepare result. Variant-specific owners make an invalid commit
/// structurally unrepresentable and no read projection clones canonical data.
pub(crate) enum PreparedObservationUpdateV1<'owner, Owner> {
    Idempotent(PreparedIdempotentV1<'owner, Owner>),
    Unknown(PreparedUnknownV1<'owner, Owner>),
    Observed(PreparedObservedV1<'owner, Owner>),
}

/// Canonicalize and validate a compiled surface-input schema.
///
/// Sorting and duplicate detection reuse the supplied `Vec`. Converting its
/// storage into the immutable `Rc`-backed schema can still allocate through the
/// global allocator; this function does not claim allocator-wide recoverable
/// OOM semantics.
pub(crate) fn canonicalize_observation_schema(
    mut ports: Vec<SurfaceInputPortId>,
) -> Result<CanonicalObservationSchemaV1, ObservationError> {
    if ports.is_empty() {
        return Err(ObservationError::EmptyCompiledSurfaceInputSchema);
    }
    ports.sort_unstable();
    if let Some(duplicate) = ports.windows(2).find(|window| window[0] == window[1]) {
        return Err(ObservationError::DuplicateCompiledSurfaceInputPort {
            input: duplicate[0],
        });
    }
    Ok(CanonicalObservationSchemaV1(Rc::from(
        ports.into_boxed_slice(),
    )))
}

/// Prepare without mutation. Stream identity always precedes payload handling.
/// `Scenarios` are fully admitted before revision comparison so malformed
/// correlated input is never masked by an otherwise valid revision error.
/// `Unknown` has no payload structure to admit and takes the cheap revision
/// path directly.
pub(crate) fn prepare_observation<'owner, Owner: ObservationOwnerV1>(
    owner: &'owner mut Owner,
    stream: ObservationStreamId,
    schema: &CanonicalObservationSchemaV1,
    update: ObservationUpdateInput,
) -> Result<PreparedObservationUpdateV1<'owner, Owner>, ObservationError> {
    if update.stream != stream {
        return Err(ObservationError::StreamMismatch {
            expected: stream,
            actual: update.stream,
        });
    }

    match update.payload {
        ObservationPayloadInput::Unknown(reason) => {
            let current_revision = owner.observation_head().revision();
            if let Some(current) = current_revision {
                if update.revision < current {
                    return Err(ObservationError::RevisionOutOfOrder {
                        current,
                        incoming: update.revision,
                    });
                }
            }
            if current_revision == Some(update.revision) {
                let exact = matches!(
                    owner.observation_head(),
                    ObservationHeadViewV1::Unknown(current) if current.reason == reason
                );
                return if exact {
                    Ok(PreparedObservationUpdateV1::Idempotent(
                        PreparedIdempotentV1 { owner },
                    ))
                } else {
                    Err(ObservationError::RevisionConflict {
                        revision: update.revision,
                    })
                };
            }
            Ok(PreparedObservationUpdateV1::Unknown(PreparedUnknownV1 {
                owner,
                unknown: RevisionBoundUnknownV1 {
                    stream,
                    revision: update.revision,
                    reason,
                },
            }))
        }
        ObservationPayloadInput::Scenarios(raw) => {
            let scenarios = canonicalize_scenarios_input(schema.as_slice(), raw)?;
            let current_revision = owner.observation_head().revision();
            if let Some(current) = current_revision {
                if update.revision < current {
                    return Err(ObservationError::RevisionOutOfOrder {
                        current,
                        incoming: update.revision,
                    });
                }
            }
            if current_revision == Some(update.revision)
                && !matches!(owner.observation_head(), ObservationHeadViewV1::Observed(_))
            {
                return Err(ObservationError::RevisionConflict {
                    revision: update.revision,
                });
            }

            if current_revision == Some(update.revision) {
                let exact = matches!(
                    owner.observation_head(), ObservationHeadViewV1::Observed(current)
                        if current.has_canonical_input(schema, &scenarios)
                );
                return if exact {
                    Ok(PreparedObservationUpdateV1::Idempotent(
                        PreparedIdempotentV1 { owner },
                    ))
                } else {
                    Err(ObservationError::RevisionConflict {
                        revision: update.revision,
                    })
                };
            }

            let set = materialize_scenarios(schema.as_slice(), scenarios)?;
            Ok(PreparedObservationUpdateV1::Observed(PreparedObservedV1 {
                owner,
                observation: RevisionBoundObservationV1 {
                    stream,
                    revision: update.revision,
                    backing: Rc::new(ObservationBackingV1 {
                        schema: schema.clone(),
                        set,
                    }),
                },
            }))
        }
    }
}

fn canonicalize_scenarios_input(
    schema: &[SurfaceInputPortId],
    raw: ObservedScenarioSetInput,
) -> Result<Vec<ScenarioInput>, ObservationError> {
    if raw.scenarios.is_empty() {
        return Err(ObservationError::EmptyScenarioSet);
    }

    let mut scenarios = raw.scenarios;
    scenarios.sort_unstable_by_key(|scenario| scenario.id);
    if let Some(duplicate) = scenarios
        .windows(2)
        .find(|window| window[0].id == window[1].id)
    {
        return Err(ObservationError::DuplicateScenarioId {
            scenario: duplicate[0].id,
        });
    }

    for scenario in &mut scenarios {
        scenario
            .bindings
            .sort_unstable_by_key(|binding| binding.port);
        if let Some(duplicate) = scenario
            .bindings
            .windows(2)
            .find(|window| window[0].port == window[1].port)
        {
            return Err(ObservationError::DuplicateSurfaceInputBinding {
                scenario: scenario.id,
                input: duplicate[0].port,
            });
        }

        if let Some(missing) = schema.iter().find(|required| {
            scenario
                .bindings
                .binary_search_by_key(*required, |binding| binding.port)
                .is_err()
        }) {
            return Err(ObservationError::MissingSurfaceInputBinding {
                scenario: scenario.id,
                input: *missing,
            });
        }
        if let Some(unexpected) = scenario
            .bindings
            .iter()
            .find(|binding| schema.binary_search(&binding.port).is_err())
        {
            return Err(ObservationError::UnexpectedSurfaceInputBinding {
                scenario: scenario.id,
                input: unexpected.port,
            });
        }
    }

    // Reuse the caller-owned outer Vec and every bindings Vec. This complete
    // canonical input is enough for lower/same-revision decisions without any
    // new allocation; only an applied higher revision materializes backing.
    scenarios.sort_unstable_by(|left, right| {
        left.bindings
            .cmp(&right.bindings)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(scenarios)
}

fn canonical_input_matches_set(set: &ObservedScenarioSet, scenarios: &[ScenarioInput]) -> bool {
    let mut case_index = 0;
    let mut scenario_index = 0;
    while scenario_index < scenarios.len() {
        let bindings = &scenarios[scenario_index].bindings;
        let Some(values) = set.values(case_index) else {
            return false;
        };
        if bindings.len() != values.len()
            || bindings
                .iter()
                .zip(values)
                .any(|(binding, value)| binding.value != *value)
        {
            return false;
        }

        let first = scenario_index;
        scenario_index += 1;
        while scenario_index < scenarios.len()
            && scenarios[scenario_index].bindings.as_slice() == bindings.as_slice()
        {
            scenario_index += 1;
        }
        let Some(provenance) = set.provenance(case_index) else {
            return false;
        };
        if provenance.len() != scenario_index - first
            || scenarios[first..scenario_index]
                .iter()
                .zip(provenance)
                .any(|(scenario, provenance)| scenario.id != *provenance)
        {
            return false;
        }
        case_index += 1;
    }
    case_index == set.cases.len()
}

fn materialize_scenarios(
    schema: &[SurfaceInputPortId],
    scenarios: Vec<ScenarioInput>,
) -> Result<ObservedScenarioSet, ObservationError> {
    debug_assert!(!scenarios.is_empty());

    let unique_case_count = 1 + scenarios
        .windows(2)
        .filter(|window| window[0].bindings.as_slice() != window[1].bindings.as_slice())
        .count();
    let value_count = unique_case_count
        .checked_mul(schema.len())
        .ok_or(ObservationError::ResourceExhausted)?;

    // Capacity for every variable-sized Vec used by grouping is fallibly
    // reserved before the first push/extend, so grouping cannot grow one of
    // those Vecs. The final boxed representation and its later Rc owner still
    // follow the global allocator's OOM behavior.
    let mut cases = Vec::new();
    cases
        .try_reserve_exact(unique_case_count)
        .map_err(|_| ObservationError::ResourceExhausted)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(value_count)
        .map_err(|_| ObservationError::ResourceExhausted)?;
    let mut provenance = Vec::new();
    provenance
        .try_reserve_exact(scenarios.len())
        .map_err(|_| ObservationError::ResourceExhausted)?;

    let mut scenarios = scenarios.into_iter().peekable();
    while let Some(ScenarioInput {
        id: first_id,
        bindings,
    }) = scenarios.next()
    {
        let values_start = values.len();
        values.extend(bindings.iter().map(|binding| binding.value));
        let values_end = values.len();
        let provenance_start = provenance.len();
        provenance.push(first_id);
        while matches!(scenarios.peek(), Some(candidate) if candidate.bindings.as_slice() == bindings.as_slice())
        {
            let ScenarioInput { id, .. } = scenarios
                .next()
                .unwrap_or_else(|| unreachable!("peek observed the next scenario"));
            provenance.push(id);
        }
        let provenance_end = provenance.len();
        cases.push(PhysicalScenario {
            values: values_start..values_end,
            provenance: provenance_start..provenance_end,
        });
    }

    debug_assert_eq!(cases.len(), unique_case_count);
    debug_assert_eq!(values.len(), value_count);
    Ok(ObservedScenarioSet {
        cases: cases.into_boxed_slice(),
        values: values.into_boxed_slice(),
        provenance: provenance.into_boxed_slice(),
    })
}
