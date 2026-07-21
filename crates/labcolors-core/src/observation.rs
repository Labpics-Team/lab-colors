//! Admission of correlated point-sRGB8 observations.
//!
//! This module owns canonicalization and a linear prepare transaction only.
//! The F2 Session is the sole production owner of the current payload and the
//! only code allowed to bind an admitted observation to evaluator evidence.

use core::ops::Range;

use crate::Srgb8;
use crate::appearance::SurfaceInputPortId;

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
    pub(crate) value: Srgb8,
}

impl SurfaceInputBinding {
    pub(crate) const fn new(port: SurfaceInputPortId, value: Srgb8) -> Self {
        Self { port, value }
    }

    pub(crate) const fn port(self) -> SurfaceInputPortId {
        self.port
    }

    pub(crate) const fn value(self) -> Srgb8 {
        self.value
    }
}

/// Raw tuple: every binding was observed simultaneously.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScenarioInput {
    pub(crate) id: ScenarioId,
    pub(crate) bindings: Vec<SurfaceInputBinding>,
}

impl ScenarioInput {
    pub(crate) fn new(id: ScenarioId, bindings: Vec<SurfaceInputBinding>) -> Self {
        Self { id, bindings }
    }

    pub(crate) const fn id(&self) -> ScenarioId {
        self.id
    }

    pub(crate) fn bindings(&self) -> &[SurfaceInputBinding] {
        &self.bindings
    }
}

/// Raw scenario collection before schema validation and canonicalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedScenarioSetInput {
    pub(crate) scenarios: Vec<ScenarioInput>,
}

impl ObservedScenarioSetInput {
    pub(crate) fn new(scenarios: Vec<ScenarioInput>) -> Self {
        Self { scenarios }
    }

    pub(crate) fn scenarios(&self) -> &[ScenarioInput] {
        &self.scenarios
    }
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

/// One unique physical tuple as canonical keyed bindings. Port identity travels
/// with every value, so an observation cannot be reinterpreted through a
/// different positional schema.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct PhysicalScenario {
    bindings: Vec<SurfaceInputBinding>,
    provenance: Range<usize>,
}

impl PhysicalScenario {
    pub(crate) fn bindings(&self) -> &[SurfaceInputBinding] {
        &self.bindings
    }
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

/// Canonical nonempty set. Provenance is one flat allocation shared by ranges,
/// not one allocation per physical case.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct ObservedScenarioSet {
    cases: Vec<PhysicalScenario>,
    provenance: Vec<ScenarioId>,
}

impl ObservedScenarioSet {
    pub(crate) fn cases(&self) -> &[PhysicalScenario] {
        &self.cases
    }

    pub(crate) fn provenance(&self, case_index: usize) -> Option<&[ScenarioId]> {
        let provenance = &self.cases.get(case_index)?.provenance;
        let range = provenance.start..provenance.end;
        self.provenance.get(range)
    }

    fn validate_surface_schema(
        &self,
        expected: &[SurfaceInputPortId],
    ) -> Result<(), ObservationSchemaMismatchV1> {
        for (case_index, case) in self.cases.iter().enumerate() {
            let schema_len = expected.len().max(case.bindings.len());
            for binding_index in 0..schema_len {
                let expected_input = expected.get(binding_index).copied();
                let actual_input = case.bindings.get(binding_index).map(|binding| binding.port);
                if expected_input != actual_input {
                    return Err(ObservationSchemaMismatchV1::new(
                        case_index,
                        binding_index,
                        expected_input,
                        actual_input,
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Sealed observation admitted against the Session-owned compiled schema.
#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(test, derive(Clone))]
pub(crate) struct RevisionBoundObservationV1 {
    stream: ObservationStreamId,
    revision: Revision,
    set: ObservedScenarioSet,
}

impl RevisionBoundObservationV1 {
    pub(crate) const fn stream(&self) -> ObservationStreamId {
        self.stream
    }

    pub(crate) const fn revision(&self) -> Revision {
        self.revision
    }

    pub(crate) const fn set(&self) -> &ObservedScenarioSet {
        &self.set
    }

    pub(crate) fn physical_case_count(&self) -> usize {
        self.set.cases().len()
    }

    pub(crate) fn physical_bindings(&self, case_index: usize) -> Option<&[SurfaceInputBinding]> {
        self.set
            .cases()
            .get(case_index)
            .map(PhysicalScenario::bindings)
    }

    pub(crate) fn provenance(&self, case_index: usize) -> Option<&[ScenarioId]> {
        self.set.provenance(case_index)
    }

    pub(crate) fn validate_surface_schema(
        &self,
        expected: &[SurfaceInputPortId],
    ) -> Result<(), ObservationSchemaMismatchV1> {
        self.set.validate_surface_schema(expected)
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

/// Canonicalize and validate a compiled surface-input schema without any new
/// allocation. The supplied Vec remains the unique schema owner.
pub(crate) fn canonicalize_observation_schema(
    mut ports: Vec<SurfaceInputPortId>,
) -> Result<Vec<SurfaceInputPortId>, ObservationError> {
    if ports.is_empty() {
        return Err(ObservationError::EmptyCompiledSurfaceInputSchema);
    }
    ports.sort_unstable();
    if let Some(duplicate) = ports.windows(2).find(|window| window[0] == window[1]) {
        return Err(ObservationError::DuplicateCompiledSurfaceInputPort {
            input: duplicate[0],
        });
    }
    Ok(ports)
}

/// Prepare without mutation. Cheap stream/lower-revision/payload-kind failures
/// precede scenario canonicalization; exact same-revision observed replay still
/// canonicalizes so equality is content-complete.
pub(crate) fn prepare_observation<'owner, Owner: ObservationOwnerV1>(
    owner: &'owner mut Owner,
    stream: ObservationStreamId,
    schema: &[SurfaceInputPortId],
    update: ObservationUpdateInput,
) -> Result<PreparedObservationUpdateV1<'owner, Owner>, ObservationError> {
    if update.stream != stream {
        return Err(ObservationError::StreamMismatch {
            expected: stream,
            actual: update.stream,
        });
    }

    let current_revision = owner.observation_head().revision();
    if let Some(current) = current_revision {
        if update.revision < current {
            return Err(ObservationError::RevisionOutOfOrder {
                current,
                incoming: update.revision,
            });
        }
    }

    match update.payload {
        ObservationPayloadInput::Unknown(reason) => {
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
            if current_revision == Some(update.revision)
                && !matches!(owner.observation_head(), ObservationHeadViewV1::Observed(_))
            {
                return Err(ObservationError::RevisionConflict {
                    revision: update.revision,
                });
            }

            let set = admit_scenarios(schema, raw)?;
            if current_revision == Some(update.revision) {
                let exact = matches!(
                    owner.observation_head(),
                    ObservationHeadViewV1::Observed(current) if current.set == set
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

            Ok(PreparedObservationUpdateV1::Observed(PreparedObservedV1 {
                owner,
                observation: RevisionBoundObservationV1 {
                    stream,
                    revision: update.revision,
                    set,
                },
            }))
        }
    }
}

fn admit_scenarios(
    schema: &[SurfaceInputPortId],
    raw: ObservedScenarioSetInput,
) -> Result<ObservedScenarioSet, ObservationError> {
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

    let mut tuples = Vec::new();
    tuples
        .try_reserve_exact(scenarios.len())
        .map_err(|_| ObservationError::ResourceExhausted)?;
    for mut scenario in scenarios {
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

        // Move the already allocated, sorted keyed tuple directly. Keeping the
        // port beside the value makes schema identity part of observation
        // equality and removes any later positional reinterpretation seam.
        tuples.push((scenario.bindings, scenario.id));
    }

    tuples.sort_unstable_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

    // Both output allocations are reserved before grouping. Moving tuples and
    // pushing into these capacities cannot allocate afterward.
    let mut cases = Vec::new();
    cases
        .try_reserve_exact(tuples.len())
        .map_err(|_| ObservationError::ResourceExhausted)?;
    let mut provenance = Vec::new();
    provenance
        .try_reserve_exact(tuples.len())
        .map_err(|_| ObservationError::ResourceExhausted)?;

    let mut tuples = tuples.into_iter().peekable();
    while let Some((bindings, first_id)) = tuples.next() {
        let start = provenance.len();
        provenance.push(first_id);
        while matches!(tuples.peek(), Some((candidate, _)) if candidate == &bindings) {
            let (_, id) = tuples
                .next()
                .unwrap_or_else(|| unreachable!("peek observed the next tuple"));
            provenance.push(id);
        }
        let end = provenance.len();
        cases.push(PhysicalScenario {
            bindings,
            provenance: start..end,
        });
    }

    Ok(ObservedScenarioSet { cases, provenance })
}
