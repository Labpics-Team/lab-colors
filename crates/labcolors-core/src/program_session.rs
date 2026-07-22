//! Generation-bound execution owner for one compiled point-render graph.
//!
//! This is deliberately below the future public `Program` boundary.  It owns
//! no client vocabulary and accepts no recipe-shaped input.  The sole strong
//! epoch lives in [`PointRenderOwnerV1`]; attached sessions retain only
//! a [`Weak`] reference, so successful replacement or disposal makes the old
//! graph physically unreachable from every old session.
//!
//! The first executable transport is intentionally narrow: one correlated set
//! of encoded Surface input signals per revision. It is transport-only state,
//! not an observed stimulus, physical evidence or certificate. F0
//! observer/output/render identities remain a terminal prerequisite before any
//! such claim can be minted. Expanding the private transport to a ScenarioSet
//! does not require exposing the legacy multi-background metric matrix.
//! In particular, the wire magic is not an `lcs` or physical identity.

use std::mem;
use std::rc::{Rc, Weak};

use crate::Srgb8;
use crate::appearance::{
    AdmittedAppearanceBindings, AppearanceBindings, AppearanceGraphSpec, AppearanceWorkspace,
    BindingError, CompileError, CompiledAppearanceGraph, SurfaceInputPortId,
};

/// ASCII `LCR1`: code-owned Lab Colors Render transport version 1.
///
/// This is a wire discriminator, not an LCS, context or physical identity.
pub(crate) const PACKED_ENCODED_SURFACE_UPDATE_MAGIC_V1: u32 = 0x4c43_5231;
pub(crate) const PACKED_ENCODED_SURFACE_UNAVAILABLE_TAG_V1: u32 = 0;
pub(crate) const PACKED_ENCODED_SURFACE_PRESENT_TAG_V1: u32 = 1;
const PACKED_ENCODED_SURFACE_HEADER_WORDS_V1: usize = 4;
const PACKED_SURFACE_UNAVAILABLE_WORDS_V1: usize = 5;

#[derive(Debug)]
struct ProgramEpochV1 {
    graph: CompiledAppearanceGraph,
    binding_template: AdmittedAppearanceBindings,
    surface_ports: Box<[SurfaceInputPortId]>,
    occurrence_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PointRenderEpochBuildErrorV1 {
    Compile(CompileError),
    Bindings(BindingError),
    EmptySurfaceSchema,
    EmptyOccurrenceSet,
    ResourceExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PointRenderAttachErrorV1 {
    Disposed,
    Bindings(BindingError),
    Workspace(BindingError),
    ResourceExhausted,
}

/// The only strong owner of the current non-reusable program epoch.
///
/// Replacement is prepare-then-swap: a failed build leaves the current `Rc`
/// untouched and therefore leaves all of its sessions live.  No epoch number,
/// fingerprint or wrapping generation participates in this ownership proof.
#[derive(Debug, Default)]
pub(crate) struct PointRenderOwnerV1 {
    current: Option<Rc<ProgramEpochV1>>,
}

impl PointRenderOwnerV1 {
    pub(crate) fn new(
        spec: AppearanceGraphSpec,
        bindings: AppearanceBindings,
    ) -> Result<Self, PointRenderEpochBuildErrorV1> {
        Ok(Self {
            current: Some(Rc::new(prepare_epoch(spec, bindings)?)),
        })
    }

    /// Compile/admit the complete replacement before revoking the old epoch.
    pub(crate) fn replace(
        &mut self,
        spec: AppearanceGraphSpec,
        bindings: AppearanceBindings,
    ) -> Result<(), PointRenderEpochBuildErrorV1> {
        let replacement = Rc::new(prepare_epoch(spec, bindings)?);
        self.current = Some(replacement);
        Ok(())
    }

    /// Revoke the current epoch. Existing sessions fail on their next call.
    pub(crate) fn dispose(&mut self) {
        self.current = None;
    }

    pub(crate) fn attach(&self) -> Result<PointRenderSessionV1, PointRenderAttachErrorV1> {
        let epoch = self
            .current
            .as_ref()
            .ok_or(PointRenderAttachErrorV1::Disposed)?;
        let workspace = epoch
            .graph
            .new_workspace()
            .map_err(PointRenderAttachErrorV1::Workspace)?;
        let bindings = epoch
            .binding_template
            .try_clone_v1()
            .map_err(PointRenderAttachErrorV1::Bindings)?;
        let initial_signal_buffers = CompositedSignalBuffersV1::try_new(
            epoch.surface_ports.len(),
            epoch.occurrence_count,
        )?;
        Ok(PointRenderSessionV1 {
            epoch: Rc::downgrade(epoch),
            bindings,
            workspace,
            initial_signal_buffers: Some(initial_signal_buffers),
            state: PointRenderSessionStateV1::Waiting {
                current_unavailable: None,
            },
        })
    }
}

fn try_zeroed_signal_words(len: usize) -> Result<Vec<u32>, PointRenderAttachErrorV1> {
    let mut words = Vec::new();
    words
        .try_reserve_exact(len)
        .map_err(|_| PointRenderAttachErrorV1::ResourceExhausted)?;
    words.resize(len, 0);
    Ok(words)
}

fn prepare_epoch(
    spec: AppearanceGraphSpec,
    bindings: AppearanceBindings,
) -> Result<ProgramEpochV1, PointRenderEpochBuildErrorV1> {
    let graph = spec
        .compile()
        .map_err(PointRenderEpochBuildErrorV1::Compile)?;
    let surface_ports: Box<[_]> = {
        let inputs = graph.surface_input_ports();
        let mut values = Vec::new();
        values
            .try_reserve_exact(inputs.len())
            .map_err(|_| PointRenderEpochBuildErrorV1::ResourceExhausted)?;
        values.extend(inputs);
        values.into_boxed_slice()
    };
    if surface_ports.is_empty() {
        return Err(PointRenderEpochBuildErrorV1::EmptySurfaceSchema);
    }
    let occurrence_count = graph.occurrence_ids().len();
    if occurrence_count == 0 {
        return Err(PointRenderEpochBuildErrorV1::EmptyOccurrenceSet);
    }
    let binding_template = graph
        .admit_bindings(&bindings)
        .map_err(PointRenderEpochBuildErrorV1::Bindings)?;
    Ok(ProgramEpochV1 {
        graph,
        binding_template,
        surface_ports,
        occurrence_count,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RevisionBoundSurfaceUnavailableV1 {
    revision: u64,
    reason: u32,
}

impl RevisionBoundSurfaceUnavailableV1 {
    pub(crate) const fn revision(self) -> u64 {
        self.revision
    }

    pub(crate) const fn reason(self) -> u32 {
        self.reason
    }
}

/// Compact committed value: one word per compiled occurrence, in the graph's
/// canonical occurrence order. No metric, threshold or JS-derived verdict is
/// present on this boundary.
#[derive(Debug, PartialEq, Eq)]
struct CompositedSignalBuffersV1 {
    input_surface_signals_rgb24: Vec<u32>,
    composited_occurrence_signals_rgb24: Vec<u32>,
}

impl CompositedSignalBuffersV1 {
    fn try_new(
        surface_count: usize,
        occurrence_count: usize,
    ) -> Result<Self, PointRenderAttachErrorV1> {
        Ok(Self {
            input_surface_signals_rgb24: try_zeroed_signal_words(surface_count)?,
            composited_occurrence_signals_rgb24: try_zeroed_signal_words(occurrence_count)?,
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CompositedSignalSnapshotV1 {
    revision: u64,
    buffers: CompositedSignalBuffersV1,
}

impl CompositedSignalSnapshotV1 {
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    pub(crate) fn input_surface_signals_rgb24(&self) -> &[u32] {
        &self.buffers.input_surface_signals_rgb24
    }

    pub(crate) fn composited_occurrence_signals_rgb24(&self) -> &[u32] {
        &self.buffers.composited_occurrence_signals_rgb24
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PointRenderSessionStateV1 {
    Waiting {
        current_unavailable: Option<RevisionBoundSurfaceUnavailableV1>,
    },
    Ready {
        current: CompositedSignalSnapshotV1,
    },
    Stale {
        previous: CompositedSignalSnapshotV1,
        current_unavailable: RevisionBoundSurfaceUnavailableV1,
    },
}

impl PointRenderSessionStateV1 {
    const fn head_revision(&self) -> Option<u64> {
        match self {
            Self::Waiting {
                current_unavailable: None,
            } => None,
            Self::Waiting {
                current_unavailable: Some(unavailable),
            }
            | Self::Stale {
                current_unavailable: unavailable,
                ..
            } => Some(unavailable.revision),
            Self::Ready { current } => Some(current.revision),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PackedEncodedSurfaceUpdateErrorV1 {
    HeaderTooShort,
    MagicMismatch { actual: u32 },
    UnsupportedTag { actual: u32 },
    LengthMismatch { expected: usize, actual: usize },
    ReservedSignalByteNonZero { surface_index: usize, value: u32 },
    RevisionOutOfOrder { current: u64, incoming: u64 },
    RevisionConflict { revision: u64 },
    ResourceExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PointRenderSessionUpdateErrorV1 {
    ProgramExpired,
    EncodedSurfaceUpdate(PackedEncodedSurfaceUpdateErrorV1),
    Evaluation(BindingError),
}

enum PreparedEncodedSurfaceUpdateV1<'input> {
    Unavailable(RevisionBoundSurfaceUnavailableV1),
    Present {
        revision: u64,
        surfaces_rgb24: &'input [u32],
    },
}

/// Generation-bound mutable runtime. It owns reusable values/scratch, never a
/// strong reference or a copy of the compiled graph. All fixed-cardinality
/// signal buffers are allocated fallibly by `attach`; `update_packed` only
/// moves and overwrites their ownership after evaluation succeeds.
#[derive(Debug)]
pub(crate) struct PointRenderSessionV1 {
    epoch: Weak<ProgramEpochV1>,
    bindings: AdmittedAppearanceBindings,
    workspace: AppearanceWorkspace,
    initial_signal_buffers: Option<CompositedSignalBuffersV1>,
    state: PointRenderSessionStateV1,
}

impl PointRenderSessionV1 {
    pub(crate) const fn state(&self) -> &PointRenderSessionStateV1 {
        &self.state
    }

    /// Admit, evaluate and commit one encoded Surface-input update transaction.
    pub(crate) fn update_packed(
        &mut self,
        words: &[u32],
    ) -> Result<&PointRenderSessionStateV1, PointRenderSessionUpdateErrorV1> {
        let epoch = self
            .epoch
            .upgrade()
            .ok_or(PointRenderSessionUpdateErrorV1::ProgramExpired)?;
        let prepared = decode_encoded_surface_update(words, epoch.surface_ports.len())
            .map_err(PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate)?;
        let incoming_revision = match &prepared {
            PreparedEncodedSurfaceUpdateV1::Unavailable(unavailable) => unavailable.revision,
            PreparedEncodedSurfaceUpdateV1::Present { revision, .. } => *revision,
        };

        if let Some(current) = self.state.head_revision() {
            if incoming_revision < current {
                return Err(PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate(
                    PackedEncodedSurfaceUpdateErrorV1::RevisionOutOfOrder {
                        current,
                        incoming: incoming_revision,
                    },
                ));
            }
            if incoming_revision == current {
                return self.admit_same_revision(prepared);
            }
        }

        match prepared {
            PreparedEncodedSurfaceUpdateV1::Unavailable(unavailable) => {
                let previous = take_last_ready(&mut self.state);
                self.state = match previous {
                    Some(previous) => PointRenderSessionStateV1::Stale {
                        previous,
                        current_unavailable: unavailable,
                    },
                    None => PointRenderSessionStateV1::Waiting {
                        current_unavailable: Some(unavailable),
                    },
                };
            }
            PreparedEncodedSurfaceUpdateV1::Present {
                revision,
                surfaces_rgb24,
            } => {
                let retained_shape_matches = match &self.state {
                    PointRenderSessionStateV1::Waiting { .. } => self
                        .initial_signal_buffers
                        .as_ref()
                        .is_some_and(|buffers| {
                            buffers.input_surface_signals_rgb24.len()
                                == epoch.surface_ports.len()
                                && buffers.composited_occurrence_signals_rgb24.len()
                                    == epoch.occurrence_count
                        }),
                    PointRenderSessionStateV1::Ready { current } => {
                        current.buffers.input_surface_signals_rgb24.len()
                            == epoch.surface_ports.len()
                            && current.buffers.composited_occurrence_signals_rgb24.len()
                                == epoch.occurrence_count
                    }
                    PointRenderSessionStateV1::Stale { previous, .. } => {
                        previous.buffers.input_surface_signals_rgb24.len()
                            == epoch.surface_ports.len()
                            && previous.buffers.composited_occurrence_signals_rgb24.len()
                                == epoch.occurrence_count
                    }
                };
                if !retained_shape_matches {
                    return Err(PointRenderSessionUpdateErrorV1::Evaluation(
                        BindingError::IncompatibleWorkspace,
                    ));
                }

                // Decode admitted the exact epoch-owned cardinality and every
                // word before this loop. Each typed port therefore exists in
                // the cloned admitted schema; setters cannot partially reject
                // a later element. These mutable values are scratch only and
                // are not published until the final state replacement below.
                for (&port, &rgb24) in epoch.surface_ports.iter().zip(surfaces_rgb24.iter()) {
                    self.bindings
                        .set_surface_input(port, Srgb8::new(unpack_rgb24(rgb24)))
                        .map_err(PointRenderSessionUpdateErrorV1::Evaluation)?;
                }
                let evaluation = epoch
                    .graph
                    .evaluate_admitted_into(&self.bindings, &mut self.workspace)
                    .map_err(PointRenderSessionUpdateErrorV1::Evaluation)?;
                if evaluation.occurrences().len() != epoch.occurrence_count {
                    return Err(PointRenderSessionUpdateErrorV1::Evaluation(
                        BindingError::IncompatibleWorkspace,
                    ));
                }

                // No fallible work follows. Preserve the committed snapshot
                // through decode, binding mutation and evaluation; only now
                // reclaim the one fixed buffer pair and overwrite it.
                let mut buffers = match take_last_ready(&mut self.state) {
                    Some(previous) => previous.buffers,
                    None => self.initial_signal_buffers.take().unwrap_or_else(|| {
                        unreachable!("a Session without prior Ready must retain initial buffers")
                    }),
                };
                debug_assert_eq!(
                    buffers.input_surface_signals_rgb24.len(),
                    surfaces_rgb24.len()
                );
                debug_assert_eq!(
                    buffers.composited_occurrence_signals_rgb24.len(),
                    epoch.occurrence_count
                );
                buffers
                    .input_surface_signals_rgb24
                    .copy_from_slice(surfaces_rgb24);
                for (resolved, output) in evaluation
                    .occurrences()
                    .zip(buffers.composited_occurrence_signals_rgb24.iter_mut())
                {
                    // Packing an already resolved encoded point is infallible.
                    // Any future fallible verifier must finish before buffer
                    // reclamation above (or introduce its own staging value).
                    *output = pack_rgb24(resolved.visible());
                }
                self.state = PointRenderSessionStateV1::Ready {
                    current: CompositedSignalSnapshotV1 {
                        revision,
                        buffers,
                    },
                };
            }
        }
        Ok(&self.state)
    }

    fn admit_same_revision(
        &self,
        prepared: PreparedEncodedSurfaceUpdateV1<'_>,
    ) -> Result<&PointRenderSessionStateV1, PointRenderSessionUpdateErrorV1> {
        let exact = match (prepared, &self.state) {
            (
                PreparedEncodedSurfaceUpdateV1::Unavailable(incoming),
                PointRenderSessionStateV1::Waiting {
                    current_unavailable: Some(current),
                }
                | PointRenderSessionStateV1::Stale {
                    current_unavailable: current,
                    ..
                },
            ) => incoming == *current,
            (
                PreparedEncodedSurfaceUpdateV1::Present {
                    revision,
                    surfaces_rgb24,
                },
                PointRenderSessionStateV1::Ready { current },
            ) => {
                revision == current.revision
                    && surfaces_rgb24
                        == current.buffers.input_surface_signals_rgb24.as_slice()
            }
            _ => false,
        };
        if exact {
            Ok(&self.state)
        } else {
            Err(PointRenderSessionUpdateErrorV1::EncodedSurfaceUpdate(
                PackedEncodedSurfaceUpdateErrorV1::RevisionConflict {
                    revision: self
                        .state
                        .head_revision()
                        .unwrap_or_else(|| unreachable!("same-revision branch has a head")),
                },
            ))
        }
    }
}

fn decode_encoded_surface_update(
    words: &[u32],
    surface_count: usize,
) -> Result<PreparedEncodedSurfaceUpdateV1<'_>, PackedEncodedSurfaceUpdateErrorV1> {
    if words.len() < PACKED_ENCODED_SURFACE_HEADER_WORDS_V1 {
        return Err(PackedEncodedSurfaceUpdateErrorV1::HeaderTooShort);
    }
    if words[0] != PACKED_ENCODED_SURFACE_UPDATE_MAGIC_V1 {
        return Err(PackedEncodedSurfaceUpdateErrorV1::MagicMismatch { actual: words[0] });
    }
    let revision = u64::from(words[2]) | (u64::from(words[3]) << 32);
    match words[1] {
        PACKED_ENCODED_SURFACE_UNAVAILABLE_TAG_V1 => {
            if words.len() != PACKED_SURFACE_UNAVAILABLE_WORDS_V1 {
                return Err(PackedEncodedSurfaceUpdateErrorV1::LengthMismatch {
                    expected: PACKED_SURFACE_UNAVAILABLE_WORDS_V1,
                    actual: words.len(),
                });
            }
            Ok(PreparedEncodedSurfaceUpdateV1::Unavailable(
                RevisionBoundSurfaceUnavailableV1 {
                    revision,
                    reason: words[4],
                },
            ))
        }
        PACKED_ENCODED_SURFACE_PRESENT_TAG_V1 => {
            let expected = PACKED_ENCODED_SURFACE_HEADER_WORDS_V1
                .checked_add(surface_count)
                .ok_or(PackedEncodedSurfaceUpdateErrorV1::ResourceExhausted)?;
            if words.len() != expected {
                return Err(PackedEncodedSurfaceUpdateErrorV1::LengthMismatch {
                    expected,
                    actual: words.len(),
                });
            }
            let surfaces = &words[PACKED_ENCODED_SURFACE_HEADER_WORDS_V1..];
            for (surface_index, &value) in surfaces.iter().enumerate() {
                if value & 0xff00_0000 != 0 {
                    return Err(
                        PackedEncodedSurfaceUpdateErrorV1::ReservedSignalByteNonZero {
                            surface_index,
                            value,
                        },
                    );
                }
            }
            Ok(PreparedEncodedSurfaceUpdateV1::Present {
                revision,
                surfaces_rgb24: surfaces,
            })
        }
        actual => Err(PackedEncodedSurfaceUpdateErrorV1::UnsupportedTag { actual }),
    }
}

fn take_last_ready(state: &mut PointRenderSessionStateV1) -> Option<CompositedSignalSnapshotV1> {
    match mem::replace(
        state,
        PointRenderSessionStateV1::Waiting {
            current_unavailable: None,
        },
    ) {
        PointRenderSessionStateV1::Waiting { .. } => None,
        PointRenderSessionStateV1::Ready { current } => Some(current),
        PointRenderSessionStateV1::Stale { previous, .. } => Some(previous),
    }
}

const fn unpack_rgb24(word: u32) -> [u8; 3] {
    [
        ((word >> 16) & 0xff) as u8,
        ((word >> 8) & 0xff) as u8,
        (word & 0xff) as u8,
    ]
}

const fn pack_rgb24(bytes: [u8; 3]) -> u32 {
    ((bytes[0] as u32) << 16) | ((bytes[1] as u32) << 8) | bytes[2] as u32
}
