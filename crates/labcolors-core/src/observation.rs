//! Admission коррелированных point sRGB8 observations.
//!
//! Authored color inputs не могут удовлетворить runtime surface ports. Один
//! stream атомарно принимает полный коррелированный ScenarioSet и хранит один
//! revision-ordered head: отклонённый update сохраняет прежнее состояние, а
//! Unknown никогда не изобретает поверхность.

use std::collections::BTreeMap;

use crate::Srgb8;
use crate::appearance::SurfaceInputPortId;

/// Runtime instance/epoch одного атомарного потока наблюдений.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ObservationStreamId(u32);

impl ObservationStreamId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
    }
}

/// Монотонная revision внутри одного [`ObservationStreamId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Revision(u64);

impl Revision {
    pub(crate) const fn new(raw: u64) -> Self {
        Self(raw)
    }
}

/// Opaque provenance одной одновременно наблюдённой tuple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScenarioId(u32);

impl ScenarioId {
    pub(crate) const fn new(raw: u32) -> Self {
        Self(raw)
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
pub(crate) struct SurfaceInputBinding {
    pub(crate) port: SurfaceInputPortId,
    pub(crate) value: Srgb8,
}

/// Raw tuple: все bindings были наблюдены одновременно.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScenarioInput {
    pub(crate) id: ScenarioId,
    pub(crate) bindings: Vec<SurfaceInputBinding>,
}

/// Raw collection до проверки schema и канонизации.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedScenarioSetInput {
    pub(crate) scenarios: Vec<ScenarioInput>,
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
    cases: Box<[PhysicalScenario]>,
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

/// Последний admitted observation до явного `Unknown`.
///
/// Значение является только evidence для presentation hold; current set из
/// него не восстанавливается.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PriorObservation {
    revision: Revision,
    set: ObservedScenarioSet,
}

impl PriorObservation {
    pub(crate) fn revision(&self) -> Revision {
        self.revision
    }

    pub(crate) fn set(&self) -> &ObservedScenarioSet {
        &self.set
    }
}

/// Единственный SSOT watermark и последнего payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObservationHead {
    Empty,
    Unknown {
        revision: Revision,
        reason: UnknownReasonId,
        previous: Option<PriorObservation>,
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

    fn prior_observation(&self) -> Option<PriorObservation> {
        match self {
            Self::Empty => None,
            Self::Unknown { previous, .. } => previous.clone(),
            Self::Observed { revision, set } => Some(PriorObservation {
                revision: *revision,
                set: set.clone(),
            }),
        }
    }
}

/// Availability полностью выводится из [`ObservationHead`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Availability {
    Waiting,
    Ready,
    Stale,
}

/// Один атомарный borrow текущей availability и всего evidence, которое ей
/// соответствует. Consumer не может прочитать state двумя вызовами и случайно
/// связать revision с другим payload после будущего interior-mutable adapter-а.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObservationSnapshot<'a> {
    Waiting {
        stream: ObservationStreamId,
        schema: &'a [SurfaceInputPortId],
        revision: Option<Revision>,
    },
    Ready {
        stream: ObservationStreamId,
        schema: &'a [SurfaceInputPortId],
        revision: Revision,
        set: &'a ObservedScenarioSet,
    },
    Stale {
        stream: ObservationStreamId,
        schema: &'a [SurfaceInputPortId],
        revision: Revision,
        previous: &'a PriorObservation,
    },
}

impl ObservationSnapshot<'_> {
    pub(crate) fn schema(&self) -> &[SurfaceInputPortId] {
        match self {
            Self::Waiting { schema, .. }
            | Self::Ready { schema, .. }
            | Self::Stale { schema, .. } => schema,
        }
    }
}

/// Успешное обновление либо exact-idempotent replay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateDisposition {
    Applied,
    Idempotent,
}

/// Typed admission failures; при любом варианте state побайтно сохраняется.
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CanonicalPayload {
    Observed(ObservedScenarioSet),
    Unknown(UnknownReasonId),
}

/// Stream-affine admission-state с immutable canonical schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservationState {
    stream: ObservationStreamId,
    compiled_surface_input_schema: Box<[SurfaceInputPortId]>,
    head: ObservationHead,
}

impl ObservationState {
    pub(crate) fn new(
        stream: ObservationStreamId,
        mut compiled_surface_input_schema: Vec<SurfaceInputPortId>,
    ) -> Result<Self, ObservationError> {
        if compiled_surface_input_schema.is_empty() {
            return Err(ObservationError::EmptyCompiledSurfaceInputSchema);
        }
        compiled_surface_input_schema.sort_unstable();
        if let Some(duplicate) = compiled_surface_input_schema
            .windows(2)
            .find(|window| window[0] == window[1])
        {
            return Err(ObservationError::DuplicateCompiledSurfaceInputPort {
                input: duplicate[0],
            });
        }
        Ok(Self {
            stream,
            compiled_surface_input_schema: compiled_surface_input_schema.into_boxed_slice(),
            head: ObservationHead::Empty,
        })
    }

    pub(crate) fn stream(&self) -> ObservationStreamId {
        self.stream
    }

    pub(crate) fn compiled_surface_input_schema(&self) -> &[SurfaceInputPortId] {
        &self.compiled_surface_input_schema
    }

    pub(crate) fn head(&self) -> &ObservationHead {
        &self.head
    }

    pub(crate) fn availability(&self) -> Availability {
        match &self.head {
            ObservationHead::Empty | ObservationHead::Unknown { previous: None, .. } => {
                Availability::Waiting
            }
            ObservationHead::Observed { .. } => Availability::Ready,
            ObservationHead::Unknown {
                previous: Some(_), ..
            } => Availability::Stale,
        }
    }

    /// Возвращает согласованный stream/revision/payload snapshot одним
    /// чтением единственного `head`; `PriorObservation` остаётся только stale
    /// evidence и никогда не проецируется как `Ready`.
    pub(crate) fn snapshot(&self) -> ObservationSnapshot<'_> {
        match &self.head {
            ObservationHead::Empty => ObservationSnapshot::Waiting {
                stream: self.stream,
                schema: &self.compiled_surface_input_schema,
                revision: None,
            },
            ObservationHead::Unknown {
                revision,
                previous: None,
                ..
            } => ObservationSnapshot::Waiting {
                stream: self.stream,
                schema: &self.compiled_surface_input_schema,
                revision: Some(*revision),
            },
            ObservationHead::Observed { revision, set } => ObservationSnapshot::Ready {
                stream: self.stream,
                schema: &self.compiled_surface_input_schema,
                revision: *revision,
                set,
            },
            ObservationHead::Unknown {
                revision,
                previous: Some(previous),
                ..
            } => ObservationSnapshot::Stale {
                stream: self.stream,
                schema: &self.compiled_surface_input_schema,
                revision: *revision,
                previous,
            },
        }
    }

    /// Только текущий `Observed` предоставляет set. Prior evidence не является
    /// неявным fallback-ом.
    pub(crate) fn current_set(&self) -> Option<&ObservedScenarioSet> {
        match &self.head {
            ObservationHead::Observed { set, .. } => Some(set),
            ObservationHead::Empty | ObservationHead::Unknown { .. } => None,
        }
    }

    /// Полностью канонизирует raw payload до сравнения revision, затем меняет
    /// один `head`. Поэтому malformed update не потребляет watermark.
    pub(crate) fn apply(
        &mut self,
        update: ObservationUpdateInput,
    ) -> Result<UpdateDisposition, ObservationError> {
        if update.stream != self.stream {
            return Err(ObservationError::StreamMismatch {
                expected: self.stream,
                actual: update.stream,
            });
        }

        let payload = match update.payload {
            ObservationPayloadInput::Scenarios(raw) => CanonicalPayload::Observed(admit_scenarios(
                &self.compiled_surface_input_schema,
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
                    Ok(UpdateDisposition::Idempotent)
                } else {
                    Err(ObservationError::RevisionConflict { revision: current })
                };
            }
        }

        self.head = match payload {
            CanonicalPayload::Observed(set) => ObservationHead::Observed {
                revision: update.revision,
                set,
            },
            CanonicalPayload::Unknown(reason) => ObservationHead::Unknown {
                revision: update.revision,
                reason,
                previous: self.head.prior_observation(),
            },
        };
        Ok(UpdateDisposition::Applied)
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

    let cases = grouped
        .into_iter()
        .map(|(bindings, provenance)| PhysicalScenario {
            bindings: bindings.into_boxed_slice(),
            provenance: provenance.into_boxed_slice(),
        })
        .collect::<Vec<_>>()
        .into_boxed_slice();
    Ok(ObservedScenarioSet { cases })
}
