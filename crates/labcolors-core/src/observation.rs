//! Admission коррелированных point sRGB8 observations.
//!
//! Raw store владеет только immutable schema, stream watermark и последним
//! payload `Empty | Unknown | Observed`. Он не хранит previous verified evidence
//! и не выводит lifecycle-состояния: `Waiting | Ready | Stale | Failed` принадлежат
//! единственному Session owner.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::Srgb8;
use crate::appearance::SurfaceInputPortId;

/// Runtime instance/epoch одного атомарного потока наблюдений.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObservationStreamId(u32);

impl ObservationStreamId {
    /// Construct one client-owned opaque observation-stream identity.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Exact transport value. It has identity semantics only.
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Монотонная revision внутри одного [`ObservationStreamId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Revision(u64);

impl Revision {
    /// Construct one monotonic revision inside an observation stream.
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    /// Exact transport value.
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Opaque provenance одной одновременно наблюдённой tuple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ScenarioId(u32);

impl ScenarioId {
    /// Construct one client-owned opaque scenario identity.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Exact transport value. It has identity semantics only.
    pub const fn value(self) -> u32 {
        self.0
    }
}

/// Opaque причина утраты текущего observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct UnknownReasonId(u32);

impl UnknownReasonId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Raw binding одного surface-input внутри коррелированного scenario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceInputBinding {
    pub(crate) port: SurfaceInputPortId,
    pub(crate) value: Srgb8,
}

impl SurfaceInputBinding {
    pub const fn new(port: SurfaceInputPortId, value: Srgb8) -> Self {
        Self { port, value }
    }

    pub const fn port(self) -> SurfaceInputPortId {
        self.port
    }

    pub const fn value(self) -> Srgb8 {
        self.value
    }
}

/// Raw tuple: все bindings были наблюдены одновременно.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioInput {
    pub(crate) id: ScenarioId,
    pub(crate) bindings: Vec<SurfaceInputBinding>,
}

impl ScenarioInput {
    pub fn new(id: ScenarioId, bindings: Vec<SurfaceInputBinding>) -> Self {
        Self { id, bindings }
    }

    pub const fn id(&self) -> ScenarioId {
        self.id
    }

    pub fn bindings(&self) -> &[SurfaceInputBinding] {
        &self.bindings
    }
}

/// Raw collection до проверки schema и канонизации.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedScenarioSetInput {
    pub(crate) scenarios: Vec<ScenarioInput>,
}

impl ObservedScenarioSetInput {
    pub fn new(scenarios: Vec<ScenarioInput>) -> Self {
        Self { scenarios }
    }

    pub fn scenarios(&self) -> &[ScenarioInput] {
        &self.scenarios
    }
}

/// Canonical immutable surface-input schema shared by admission and bound
/// evaluators. Construction sorts opaque identities and rejects duplicates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservationSchemaV1 {
    ports: Arc<[SurfaceInputPortId]>,
}

impl ObservationSchemaV1 {
    pub fn try_new(
        mut ports: Vec<SurfaceInputPortId>,
    ) -> Result<Self, ObservationError> {
        if ports.is_empty() {
            return Err(ObservationError::EmptyCompiledSurfaceInputSchema);
        }
        ports.sort_unstable();
        if let Some(duplicate) = ports.windows(2).find(|window| window[0] == window[1]) {
            return Err(ObservationError::DuplicateCompiledSurfaceInputPort {
                input: duplicate[0],
            });
        }
        Ok(Self {
            ports: Arc::from(ports.into_boxed_slice()),
        })
    }

    pub fn ports(&self) -> &[SurfaceInputPortId] {
        &self.ports
    }
}

/// Raw payload одной revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObservationPayloadInput {
    Scenarios(ObservedScenarioSetInput),
    Unknown(UnknownReasonId),
}

/// Атомарное обновление одного stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservationUpdateInput {
    pub(crate) stream: ObservationStreamId,
    pub(crate) revision: Revision,
    pub(crate) payload: ObservationPayloadInput,
}

/// Одна уникальная физическая tuple в порядке compiled schema.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PhysicalScenario {
    bindings: Box<[Srgb8]>,
    provenance: Box<[ScenarioId]>,
}

impl PhysicalScenario {
    pub(crate) fn bindings(&self) -> &[Srgb8] {
        &self.bindings
    }

    pub(crate) fn provenance(&self) -> &[ScenarioId] {
        &self.provenance
    }
}

/// Sealed canonical value: nonempty unique physical cases без скрытой редукции.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedScenarioSet {
    cases: Arc<[PhysicalScenario]>,
}

impl ObservedScenarioSet {
    pub(crate) fn cases(&self) -> &[PhysicalScenario] {
        &self.cases
    }

    pub(crate) fn physical_bindings(&self) -> Vec<Vec<Srgb8>> {
        self.cases
            .iter()
            .map(|case| case.bindings.to_vec())
            .collect()
    }
}

/// Sealed revision-bound evidence только для admitted `Observed` payload.
/// Schema и canonical set используют immutable shared backing: clone не копирует
/// tuples/provenance, но equality остаётся content-based.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RevisionBoundObservationV1 {
    stream: ObservationStreamId,
    schema: ObservationSchemaV1,
    revision: Revision,
    set: ObservedScenarioSet,
}

impl RevisionBoundObservationV1 {
    pub const fn stream(&self) -> ObservationStreamId {
        self.stream
    }

    pub const fn admitted_schema(&self) -> &ObservationSchemaV1 {
        &self.schema
    }

    pub fn schema(&self) -> &[SurfaceInputPortId] {
        self.schema.ports()
    }

    pub const fn revision(&self) -> Revision {
        self.revision
    }

    pub(crate) const fn set(&self) -> &ObservedScenarioSet {
        &self.set
    }

    pub fn physical_case_count(&self) -> usize {
        self.set.cases().len()
    }

    pub fn physical_bindings(&self, case_index: usize) -> Option<&[Srgb8]> {
        self.set.cases().get(case_index).map(PhysicalScenario::bindings)
    }

    pub fn provenance(&self, case_index: usize) -> Option<&[ScenarioId]> {
        self.set
            .cases()
            .get(case_index)
            .map(PhysicalScenario::provenance)
    }
}

/// Admit one declared immutable snapshot through the same canonicalizer used
/// by the stateful observation store. This binds values to a stream/revision;
/// it does not claim that the revision is still current.
pub fn admit_observation_snapshot_v1(
    schema: ObservationSchemaV1,
    stream: ObservationStreamId,
    revision: Revision,
    scenarios: ObservedScenarioSetInput,
) -> Result<RevisionBoundObservationV1, ObservationError> {
    let set = admit_scenarios(schema.ports(), scenarios)?;
    Ok(RevisionBoundObservationV1 {
        stream,
        schema,
        revision,
        set,
    })
}

/// Revision-bound факт отсутствия текущего observation. Он не содержит previous
/// payload или verified evidence и сам по себе не является lifecycle state.
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

/// Единственный SSOT watermark и текущего raw payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObservationHead {
    Empty,
    Unknown {
        revision: Revision,
        reason: UnknownReasonId,
    },
    Observed {
        revision: Revision,
        set: ObservedScenarioSet,
    },
}

impl ObservationHead {
    fn revision(&self) -> Option<Revision> {
        match self {
            Self::Empty => None,
            Self::Unknown { revision, .. } | Self::Observed { revision, .. } => Some(*revision),
        }
    }

    fn matches(&self, payload: &CanonicalPayload) -> bool {
        match (self, payload) {
            (Self::Unknown { reason, .. }, CanonicalPayload::Unknown(candidate)) => {
                reason == candidate
            }
            (Self::Observed { set, .. }, CanonicalPayload::Observed(candidate)) => set == candidate,
            _ => false,
        }
    }
}

/// Успешный commit либо exact-idempotent replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateDisposition {
    Applied,
    Idempotent,
}

/// Typed admission failures; prepare не меняет raw head.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservationError {
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CanonicalPayload {
    Observed(ObservedScenarioSet),
    Unknown(UnknownReasonId),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PreparedObservationPayloadV1 {
    Idempotent,
    AppliedUnknown(RevisionBoundUnknownV1),
    AppliedObserved(RevisionBoundObservationV1),
}

/// Read-only projection prepared transaction. Она не содержит authority и не
/// может быть передана commit-у.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PreparedObservationViewV1 {
    Idempotent,
    AppliedUnknown(RevisionBoundUnknownV1),
    AppliedObserved(RevisionBoundObservationV1),
}

/// Linear transaction, привязанная mutable-borrow к конкретному raw store.
/// Её нельзя сконструировать, клонировать или commit-нуть в другой stream/state.
#[derive(Debug)]
pub(crate) struct PreparedObservationUpdateV1<'a> {
    state: &'a mut ObservationState,
    payload: PreparedObservationPayloadV1,
}

impl PreparedObservationUpdateV1<'_> {
    pub(crate) fn view(&self) -> PreparedObservationViewV1 {
        match &self.payload {
            PreparedObservationPayloadV1::Idempotent => PreparedObservationViewV1::Idempotent,
            PreparedObservationPayloadV1::AppliedUnknown(unknown) => {
                PreparedObservationViewV1::AppliedUnknown(*unknown)
            }
            PreparedObservationPayloadV1::AppliedObserved(observation) => {
                PreparedObservationViewV1::AppliedObserved(observation.clone())
            }
        }
    }

    pub(crate) const fn current_head(&self) -> &ObservationHead {
        &self.state.head
    }

    /// Consumes the only authority and commits into the exact state borrowed by
    /// `prepare`; cross-state commit is structurally unrepresentable.
    pub(crate) fn commit(self) -> UpdateDisposition {
        let Self { state, payload } = self;
        match payload {
            PreparedObservationPayloadV1::Idempotent => UpdateDisposition::Idempotent,
            PreparedObservationPayloadV1::AppliedUnknown(unknown) => {
                state.head = ObservationHead::Unknown {
                    revision: unknown.revision,
                    reason: unknown.reason,
                };
                UpdateDisposition::Applied
            }
            PreparedObservationPayloadV1::AppliedObserved(observation) => {
                state.head = ObservationHead::Observed {
                    revision: observation.revision,
                    set: observation.set,
                };
                UpdateDisposition::Applied
            }
        }
    }
}

/// Stream-affine admission-state с immutable canonical schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservationState {
    stream: ObservationStreamId,
    compiled_surface_input_schema: ObservationSchemaV1,
    head: ObservationHead,
}

impl ObservationState {
    pub(crate) fn new(
        stream: ObservationStreamId,
        compiled_surface_input_schema: Vec<SurfaceInputPortId>,
    ) -> Result<Self, ObservationError> {
        let compiled_surface_input_schema =
            ObservationSchemaV1::try_new(compiled_surface_input_schema)?;
        Ok(Self {
            stream,
            compiled_surface_input_schema,
            head: ObservationHead::Empty,
        })
    }

    pub(crate) fn compiled_surface_input_schema(&self) -> &[SurfaceInputPortId] {
        self.compiled_surface_input_schema.ports()
    }

    pub(crate) const fn head(&self) -> &ObservationHead {
        &self.head
    }

    pub(crate) fn current_observation(&self) -> Option<RevisionBoundObservationV1> {
        match &self.head {
            ObservationHead::Observed { revision, set } => Some(RevisionBoundObservationV1 {
                stream: self.stream,
                schema: self.compiled_surface_input_schema.clone(),
                revision: *revision,
                set: set.clone(),
            }),
            ObservationHead::Empty | ObservationHead::Unknown { .. } => None,
        }
    }

    pub(crate) fn current_unknown(&self) -> Option<RevisionBoundUnknownV1> {
        match self.head {
            ObservationHead::Unknown { revision, reason } => Some(RevisionBoundUnknownV1 {
                stream: self.stream,
                revision,
                reason,
            }),
            ObservationHead::Empty | ObservationHead::Observed { .. } => None,
        }
    }

    /// Полностью канонизирует raw payload до сравнения revision и ничего не
    /// мутирует. Успешный результат линейно занимает store до commit/drop.
    pub(crate) fn prepare(
        &mut self,
        update: ObservationUpdateInput,
    ) -> Result<PreparedObservationUpdateV1<'_>, ObservationError> {
        if update.stream != self.stream {
            return Err(ObservationError::StreamMismatch {
                expected: self.stream,
                actual: update.stream,
            });
        }

        let payload = match update.payload {
            ObservationPayloadInput::Scenarios(raw) => CanonicalPayload::Observed(admit_scenarios(
                self.compiled_surface_input_schema.ports(),
                raw,
            )?),
            ObservationPayloadInput::Unknown(reason) => CanonicalPayload::Unknown(reason),
        };

        if let Some(current) = self.head.revision() {
            if update.revision < current {
                return Err(ObservationError::RevisionOutOfOrder {
                    current,
                    incoming: update.revision,
                });
            }
            if update.revision == current {
                return if self.head.matches(&payload) {
                    Ok(PreparedObservationUpdateV1 {
                        state: self,
                        payload: PreparedObservationPayloadV1::Idempotent,
                    })
                } else {
                    Err(ObservationError::RevisionConflict { revision: current })
                };
            }
        }

        let payload = match payload {
            CanonicalPayload::Observed(set) => {
                PreparedObservationPayloadV1::AppliedObserved(RevisionBoundObservationV1 {
                    stream: self.stream,
                    schema: self.compiled_surface_input_schema.clone(),
                    revision: update.revision,
                    set,
                })
            }
            CanonicalPayload::Unknown(reason) => {
                PreparedObservationPayloadV1::AppliedUnknown(RevisionBoundUnknownV1 {
                    stream: self.stream,
                    revision: update.revision,
                    reason,
                })
            }
        };
        Ok(PreparedObservationUpdateV1 {
            state: self,
            payload,
        })
    }

    #[cfg(test)]
    pub(crate) fn apply(
        &mut self,
        update: ObservationUpdateInput,
    ) -> Result<UpdateDisposition, ObservationError> {
        Ok(self.prepare(update)?.commit())
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

    let mut grouped: BTreeMap<Vec<Srgb8>, Vec<ScenarioId>> = BTreeMap::new();
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

        let tuple: Vec<Srgb8> = schema
            .iter()
            .map(|required| {
                let index = scenario
                    .bindings
                    .binary_search_by_key(required, |binding| binding.port)
                    .unwrap_or_else(|_| unreachable!("required bindings were checked"));
                scenario.bindings[index].value
            })
            .collect();
        grouped.entry(tuple).or_default().push(scenario.id);
    }

    let cases: Arc<[PhysicalScenario]> = grouped
        .into_iter()
        .map(|(bindings, provenance)| PhysicalScenario {
            bindings: bindings.into_boxed_slice(),
            provenance: provenance.into_boxed_slice(),
        })
        .collect::<Vec<_>>()
        .into();
    Ok(ObservedScenarioSet { cases })
}
