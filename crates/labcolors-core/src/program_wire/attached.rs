//! Public FV-01 facade over the existing private Program attachment transaction.
//!
//! This module owns no evaluator, Session state machine, or sink physics. It
//! translates public scalar IDs into the already compiled Program/attachment
//! contract and keeps the private typestate behind one provisional facade.

use crate::family_artifact::FamilyArtifactBundleV2;
use crate::program::attachment::fv01::{
    AttachedPointSinkOutputIdV1, AttachedProgramAttachmentV1, attached_point_sink,
    mint_authorities, owner_pin, validate_authority as validate_attached_authority,
};
use crate::program::attachment::{
    AttachmentCreateErrorV1, AttachmentCreateFailureV2, AttachmentUpdateErrorV1,
    AuthoredPointEmissionBindingV1, AuthoredPointPresentationBindingV1,
};
use crate::program::wire::{ProgramWireErrorV1, decode_program_wire_v1};
use crate::program::{
    CompileErrorV1, OccurrenceIdV1, OutputSlotIdV1, OwnerV1, PresentationRootIdV1, ScenarioV1,
    StateKindV1, UpdateV1,
};
use crate::program_session::{CoreProgramEvaluatorsV1, ProgramOwnerLeaseV1};

use super::{
    AttachedMaterializationAuthorityErrorV1, AttachedMaterializationAuthorityV1,
    AttachedPointSinkAdmissionErrorV1, AttachedPointSinkErrorV1, AttachedPointSinkHostV1,
    ProgramScenarioV1,
};

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
    /// The compiled owner is borrowed, not consumed, so two attachments can
    /// share content identity and owner generation while retaining independent
    /// host binding epochs.
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

/// Lifecycle class after one atomically published attached update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttachedProgramUpdateStateV1 {
    /// No certified observation exists yet.
    Waiting,
    /// Current revision is certified and carries materialization authority.
    Ready,
    /// Current observation is unavailable; the previous certificate is diagnostic only.
    Stale,
    /// Current revision is exhaustively infeasible and the host scope is revoked.
    Failed,
}

/// Owned result of one attached update.
pub struct AttachedProgramUpdateV1 {
    state: AttachedProgramUpdateStateV1,
    authorities: Vec<AttachedMaterializationAuthorityV1>,
}

impl AttachedProgramUpdateV1 {
    /// Lifecycle class committed by the same transaction as the host effect.
    #[must_use]
    pub const fn state(&self) -> AttachedProgramUpdateStateV1 {
        self.state
    }

    /// Authorities minted from this exact post-commit `Ready` state.
    /// Non-`Ready` outcomes always expose an empty slice.
    #[must_use]
    pub fn authorities(&self) -> &[AttachedMaterializationAuthorityV1] {
        &self.authorities
    }

    /// Consume the outcome and take ownership of all minted authorities.
    #[must_use]
    pub fn into_authorities(self) -> Vec<AttachedMaterializationAuthorityV1> {
        self.authorities
    }
}

/// Typed refusal of an attached update.
#[non_exhaustive]
pub enum AttachedProgramUpdateErrorV1<HostError> {
    /// Temporary descriptor storage for borrowed scenarios could not be reserved.
    ResourceExhausted,
    /// Core rejected the observation update before any host effect was committed.
    Update,
    /// The admitted sink rejected preparation before installation.
    SinkPrepare(AttachedPointSinkErrorV1<HostError>),
    /// The host rejected the atomic installation; previous materialization remains intact.
    SinkInstall(AttachedPointSinkErrorV1<HostError>),
    /// Post-commit authority proof could not be minted or revalidated.
    Authority(AttachedMaterializationAuthorityErrorV1),
    /// A sealed Core invariant was violated.
    InternalInvariant,
}

/// Live attached Program runtime for one exact host binding epoch.
pub struct AttachedProgramV1<H>
where
    H: AttachedPointSinkHostV1,
{
    attachment: AttachedProgramAttachmentV1<H>,
    owner_pin: ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
    content_identity: crate::program::ContentIdentityV9,
}

impl<H> AttachedProgramV1<H>
where
    H: AttachedPointSinkHostV1,
{
    /// Atomically evaluate and publish one observed revision.
    ///
    /// Scenario values are borrowed. Authority is minted exclusively from the
    /// successful post-commit attachment view, never from a detached snapshot.
    pub fn update_observed(
        &mut self,
        revision: u64,
        scenarios: &[ProgramScenarioV1],
    ) -> Result<AttachedProgramUpdateV1, AttachedProgramUpdateErrorV1<H::Error>> {
        let mut projected = Vec::new();
        projected
            .try_reserve_exact(scenarios.len())
            .map_err(|_| AttachedProgramUpdateErrorV1::ResourceExhausted)?;
        projected.extend(
            scenarios
                .iter()
                .map(|scenario| ScenarioV1::new(scenario.id(), scenario.surfaces())),
        );
        self.apply_update(UpdateV1::Observed {
            revision,
            scenarios: &projected,
        })
    }

    /// Atomically publish that the observation is unavailable.
    ///
    /// A previously installed materialization is revoked by the same attachment
    /// transaction before the returned lifecycle can become `Stale`.
    pub fn update_unknown(
        &mut self,
        revision: u64,
        reason_id: u32,
    ) -> Result<AttachedProgramUpdateV1, AttachedProgramUpdateErrorV1<H::Error>> {
        self.apply_update(UpdateV1::Unknown {
            revision,
            reason_id,
        })
    }

    /// Revalidate a previously minted authority against this exact live
    /// attachment, Program generation, host binding epoch and committed revision.
    ///
    /// This is the only public freshness check for owned authority values. The
    /// validation rules remain owned by the private attachment implementation.
    pub fn validate_authority(
        &self,
        authority: &AttachedMaterializationAuthorityV1,
    ) -> Result<(), AttachedMaterializationAuthorityErrorV1> {
        validate_attached_authority(
            &self.attachment,
            &self.owner_pin,
            self.content_identity,
            authority,
        )
    }

    fn apply_update(
        &mut self,
        update: UpdateV1<'_>,
    ) -> Result<AttachedProgramUpdateV1, AttachedProgramUpdateErrorV1<H::Error>> {
        let (state, authorities) = {
            let commit = self.attachment.update(update).map_err(map_update_failure)?;
            let state = match commit.evidence().kind() {
                StateKindV1::Waiting => AttachedProgramUpdateStateV1::Waiting,
                StateKindV1::Ready => AttachedProgramUpdateStateV1::Ready,
                StateKindV1::Stale => AttachedProgramUpdateStateV1::Stale,
                StateKindV1::Failed => AttachedProgramUpdateStateV1::Failed,
            };
            let authorities = if state == AttachedProgramUpdateStateV1::Ready {
                mint_authorities(commit, &self.owner_pin)
                    .map_err(AttachedProgramUpdateErrorV1::Authority)?
            } else {
                Vec::new()
            };
            (state, authorities)
        };

        if state == AttachedProgramUpdateStateV1::Ready {
            if authorities.is_empty() {
                return Err(AttachedProgramUpdateErrorV1::InternalInvariant);
            }
            for authority in &authorities {
                validate_attached_authority(
                    &self.attachment,
                    &self.owner_pin,
                    self.content_identity,
                    authority,
                )
                .map_err(AttachedProgramUpdateErrorV1::Authority)?;
            }
        } else if !authorities.is_empty() {
            return Err(AttachedProgramUpdateErrorV1::InternalInvariant);
        }

        Ok(AttachedProgramUpdateV1 { state, authorities })
    }
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

fn map_update_failure<HostError>(
    failure: AttachmentUpdateErrorV1<AttachedPointSinkErrorV1<HostError>>,
) -> AttachedProgramUpdateErrorV1<HostError> {
    match failure {
        AttachmentUpdateErrorV1::Update(_) => AttachedProgramUpdateErrorV1::Update,
        AttachmentUpdateErrorV1::SinkPrepare(error) => {
            AttachedProgramUpdateErrorV1::SinkPrepare(error)
        }
        AttachmentUpdateErrorV1::SinkInstall(error) => {
            AttachedProgramUpdateErrorV1::SinkInstall(error)
        }
        AttachmentUpdateErrorV1::InternalInvariant(_) => {
            AttachedProgramUpdateErrorV1::InternalInvariant
        }
    }
}
