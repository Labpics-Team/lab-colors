//! Public FV-01 facade over the existing private Program attachment transaction.
//!
//! This module owns no evaluator, Session state machine, or sink physics. It
//! translates public scalar IDs into the already compiled Program/attachment
//! contract and keeps the private typestate behind one provisional facade.

use crate::Srgb8;
use crate::family_artifact::FamilyArtifactBundleV2;
use crate::program::attachment::fv01::{
    AttachedPointSinkOutputIdV1, AttachedProgramAttachmentV1, attached_point_sink, owner_pin,
};
use crate::program::attachment::{
    AttachmentCreateErrorV1, AttachmentCreateFailureV2, AuthoredPointEmissionBindingV1,
    AuthoredPointPresentationBindingV1,
};
use crate::program::wire::{ProgramWireErrorV1, decode_program_wire_v1};
use crate::program::{
    CompileErrorV1, OccurrenceIdV1, OutputSlotIdV1, OwnerV1, PresentationRootIdV1,
};
use crate::program_session::{CoreProgramEvaluatorsV1, ProgramOwnerLeaseV1};

use super::{AttachedPointSinkAdmissionErrorV1, AttachedPointSinkHostV1};

/// Typed failure while compiling canonical Program wire for attached execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttachedProgramCompileErrorV1 {
    /// Bytes violate the canonical Program wire grammar.
    Wire,
    /// Canonical bytes describe an invalid Program graph.
    Compile,
    /// A declared presentation root is consumed downstream and is not terminal.
    RootConsumedDownstream,
    /// The Program requires family artifacts for which this first public seam
    /// intentionally has no trust parameter yet.
    FamilyArtifactsRequired,
}

/// Typed failure before an attached runtime becomes live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttachedProgramAttachErrorV1 {
    /// Allocation failed before host admission completed.
    ResourceExhausted,
    /// Session construction failed before the host could publish anything.
    Instantiate,
    /// Output/presentation bindings do not match the compiled Program exactly.
    InvalidBindings,
    /// Host-owned output scope differs from the compiler-minted scope.
    ScopeChanged,
    /// A fresh process-local host binding epoch could not be minted.
    EpochExhausted,
}

/// One public output-slot → host-sink binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachedProgramEmissionBindingV1 {
    output: u32,
    sink_output: u32,
}

impl AttachedProgramEmissionBindingV1 {
    /// Construct one opaque output-to-sink binding.
    #[must_use]
    pub const fn new(output: u32, sink_output: u32) -> Self {
        Self {
            output,
            sink_output,
        }
    }

    /// Authored Program output slot.
    #[must_use]
    pub const fn output(self) -> u32 {
        self.output
    }

    /// Host-owned sink identity.
    #[must_use]
    pub const fn sink_output(self) -> u32 {
        self.sink_output
    }
}

/// One public output → compiled presentation target binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachedProgramPresentationBindingV1 {
    output: u32,
    root: u32,
    occurrence: u32,
}

impl AttachedProgramPresentationBindingV1 {
    /// Construct one opaque presentation binding.
    #[must_use]
    pub const fn new(output: u32, root: u32, occurrence: u32) -> Self {
        Self {
            output,
            root,
            occurrence,
        }
    }

    /// Authored Program output slot.
    #[must_use]
    pub const fn output(self) -> u32 {
        self.output
    }

    /// Authored presentation root ID.
    #[must_use]
    pub const fn root(self) -> u32 {
        self.root
    }

    /// Authored presentation target occurrence ID.
    #[must_use]
    pub const fn occurrence(self) -> u32 {
        self.occurrence
    }
}

/// One observed physical scenario in compiled surface-input order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedProgramScenarioV1 {
    id: u32,
    surfaces: Vec<Srgb8>,
}

impl AttachedProgramScenarioV1 {
    /// Construct one owned observation row.
    #[must_use]
    pub fn new(id: u32, surfaces: Vec<Srgb8>) -> Self {
        Self { id, surfaces }
    }

    /// Opaque scenario provenance ID.
    #[must_use]
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Surface values in the Program's compiled schema order.
    #[must_use]
    pub fn surfaces(&self) -> &[Srgb8] {
        &self.surfaces
    }
}

/// One fully compiled Program generation capable of minting attached runtimes.
pub struct CompiledAttachedProgramV1 {
    owner: OwnerV1,
}

impl CompiledAttachedProgramV1 {
    /// Canonical 256-bit content identity of this Program generation.
    #[must_use]
    pub fn content_identity(&self) -> [u8; 32] {
        *self.owner.content_identity().as_bytes()
    }

    /// Number of surface inputs required in each observed scenario.
    #[must_use]
    pub fn surface_input_count(&self) -> usize {
        self.owner.surface_input_port_count()
    }

    /// Bind one host-owned output scope to this exact compiled generation.
    ///
    /// The compiled owner is borrowed, not consumed, so tests and hosts can
    /// create two independent sink epochs from the same generation and prove
    /// that foreign-epoch authorities are rejected independently of content
    /// identity.
    pub fn attach<H>(
        &self,
        stream_id: u32,
        emissions: &[AttachedProgramEmissionBindingV1],
        presentations: &[AttachedProgramPresentationBindingV1],
        host: H,
    ) -> Result<AttachedProgramV1<H>, AttachedProgramAttachErrorV1>
    where
        H: AttachedPointSinkHostV1,
    {
        let mut authored_emissions = Vec::new();
        authored_emissions
            .try_reserve_exact(emissions.len())
            .map_err(|_| AttachedProgramAttachErrorV1::ResourceExhausted)?;
        authored_emissions.extend(emissions.iter().copied().map(|binding| {
            AuthoredPointEmissionBindingV1::new(
                OutputSlotIdV1::new(binding.output),
                AttachedPointSinkOutputIdV1::new(binding.sink_output),
            )
        }));

        let mut authored_presentations = Vec::new();
        authored_presentations
            .try_reserve_exact(presentations.len())
            .map_err(|_| AttachedProgramAttachErrorV1::ResourceExhausted)?;
        authored_presentations.extend(presentations.iter().copied().map(|binding| {
            AuthoredPointPresentationBindingV1::new(
                OutputSlotIdV1::new(binding.output),
                PresentationRootIdV1::new(binding.root),
                OccurrenceIdV1::new(binding.occurrence),
            )
        }));

        // Admission compares canonical output order. Derive that order from the
        // public bindings before creating the unbound writer so caller order has
        // no hidden semantic meaning.
        let mut scope = Vec::new();
        scope
            .try_reserve_exact(emissions.len())
            .map_err(|_| AttachedProgramAttachErrorV1::ResourceExhausted)?;
        scope.extend(
            emissions
                .iter()
                .map(|binding| (binding.output, binding.sink_output)),
        );
        scope.sort_unstable_by_key(|(output, _)| *output);
        let owned_scope = scope
            .into_iter()
            .map(|(_, sink)| AttachedPointSinkOutputIdV1::new(sink))
            .collect();

        let pin = owner_pin(&self.owner);
        let content_identity = self.owner.content_identity();
        self.owner
            .attach_external(
                stream_id,
                &authored_emissions,
                &authored_presentations,
                FamilyArtifactBundleV2::empty(),
                attached_point_sink(owned_scope, host),
            )
            .map(|attachment| AttachedProgramV1 {
                attachment,
                owner_pin: pin,
                content_identity,
            })
            .map_err(map_attach_failure)
    }
}

/// Live attached Program runtime for one exact host binding epoch.
pub struct AttachedProgramV1<H>
where
    H: AttachedPointSinkHostV1,
{
    pub(crate) attachment: AttachedProgramAttachmentV1<H>,
    pub(crate) owner_pin: ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
    pub(crate) content_identity: crate::program::ContentIdentityV9,
}

/// Compile canonical Program wire into an owner that can create attached
/// runtimes without creating any detached authority.
pub fn compile_attached_program_wire_v1(
    bytes: &[u8],
) -> Result<CompiledAttachedProgramV1, AttachedProgramCompileErrorV1> {
    let draft = decode_program_wire_v1(bytes)
        .map_err(|_error: ProgramWireErrorV1| AttachedProgramCompileErrorV1::Wire)?;
    let owner = draft.compile().map_err(|error| match error {
        CompileErrorV1::PresentationRootConsumedDownstream { .. } => {
            AttachedProgramCompileErrorV1::RootConsumedDownstream
        }
        _ => AttachedProgramCompileErrorV1::Compile,
    })?;
    if owner.required_family_releases().next().is_some() {
        return Err(AttachedProgramCompileErrorV1::FamilyArtifactsRequired);
    }
    Ok(CompiledAttachedProgramV1 { owner })
}

fn map_attach_failure<H>(
    failure: AttachmentCreateFailureV2<
        crate::program::attachment::fv01::AttachedPointSinkWriterV1<H>,
    >,
) -> AttachedProgramAttachErrorV1
where
    H: AttachedPointSinkHostV1,
{
    match failure {
        AttachmentCreateFailureV2::Contract { cause, .. } => match cause {
            AttachmentCreateErrorV1::ResourceExhausted => {
                AttachedProgramAttachErrorV1::ResourceExhausted
            }
            AttachmentCreateErrorV1::Instantiate(_) => AttachedProgramAttachErrorV1::Instantiate,
            _ => AttachedProgramAttachErrorV1::InvalidBindings,
        },
        AttachmentCreateFailureV2::SinkAdmission { cause, .. } => match cause {
            AttachedPointSinkAdmissionErrorV1::ScopeChanged => {
                AttachedProgramAttachErrorV1::ScopeChanged
            }
            AttachedPointSinkAdmissionErrorV1::EpochExhausted => {
                AttachedProgramAttachErrorV1::EpochExhausted
            }
            AttachedPointSinkAdmissionErrorV1::ResourceExhausted => {
                AttachedProgramAttachErrorV1::ResourceExhausted
            }
        },
    }
}
