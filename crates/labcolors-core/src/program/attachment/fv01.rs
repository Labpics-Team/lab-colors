use core::{
    mem,
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::Srgb8;
use crate::appearance::{EncodedPointPaintV1, ExactFinalOwnedPointDomainV1};
use crate::program::{
    AppearanceContextV1, ContentIdentityV9, CoreVerifiedV1, OccurrenceIdV1, OutputSlotIdV1,
    PresentationRootIdV1, SurroundV1,
};
use crate::program_session::{CoreProgramEvaluatorsV1, ProgramOwnerLeaseV1};

use super::{
    AttachedRenderOutputV1, AttachmentCommitV1, BoundPointSinkScopePermitV1,
    ExternallyManagedAttachmentV1, PointSinkAdmissionFailureV1, PointSinkBindingEpochV1,
    PointSinkIntentV1, PointSinkStampV1, PointSinkWriterAdmissionV1, PointSinkWriterV1,
    PreparedPointSinkWriteV1, UnboundPointSinkWriterV1, sink_private,
};

static NEXT_ATTACHED_POINT_SINK_EPOCH_V1: AtomicU64 = AtomicU64::new(1);

/// Opaque host-owned output identity used by the FV-01 attachment sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttachedPointSinkOutputIdV1(u32);

impl AttachedPointSinkOutputIdV1 {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> u32 {
        self.0
    }
}

/// One complete host patch entry after compiler admission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttachedPointSinkHostPatchEntryV1 {
    output: u32,
    sink_output: u32,
    source: Srgb8,
    opacity: f64,
}

impl AttachedPointSinkHostPatchEntryV1 {
    /// Authored Program output slot.
    #[must_use]
    pub const fn output(self) -> u32 {
        self.output
    }

    /// Host-owned sink output identity.
    #[must_use]
    pub const fn sink_output(self) -> u32 {
        self.sink_output
    }

    /// Encoded-sRGB8 source written by the host.
    #[must_use]
    pub const fn source(self) -> Srgb8 {
        self.source
    }

    /// Straight alpha written by the host.
    #[must_use]
    pub const fn opacity(self) -> f64 {
        self.opacity
    }
}

/// One synchronous atomic host command.
///
/// `try_install` must publish the complete command or leave the previously
/// published scope, revision and sequence unchanged.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AttachedPointSinkHostIntentV1<'a> {
    /// Replace the complete admitted scope.
    SetAll {
        /// Observation revision being published.
        revision: u64,
        /// Process-local incarnation of this exact host binding.
        binding_epoch: u64,
        /// Sequence that must still be published.
        expected_sequence: u64,
        /// Sequence that becomes visible on success.
        desired_sequence: u64,
        /// Complete canonical output patch.
        patch: &'a [AttachedPointSinkHostPatchEntryV1],
    },
    /// Atomically remove the complete admitted scope.
    RevokeAll {
        /// Observation revision causing revocation.
        revision: u64,
        /// Process-local incarnation of this exact host binding.
        binding_epoch: u64,
        /// Sequence that must still be published.
        expected_sequence: u64,
        /// Sequence that becomes visible on success.
        desired_sequence: u64,
    },
    /// Confirm that the host still exposes the exact published snapshot.
    ConfirmExact {
        /// Revision that must already be published.
        revision: u64,
        /// Process-local incarnation of this exact host binding.
        binding_epoch: u64,
        /// Sequence that must already be published.
        published_sequence: u64,
        /// Exact complete patch expected to remain visible.
        patch: &'a [AttachedPointSinkHostPatchEntryV1],
    },
}

/// Host-owned synchronous effect boundary for attached Program materialization.
pub trait AttachedPointSinkHostV1 {
    /// Host-specific typed failure.
    type Error;

    /// Atomically apply or confirm one complete command.
    fn try_install(
        &mut self,
        intent: AttachedPointSinkHostIntentV1<'_>,
    ) -> Result<(), Self::Error>;
}

/// Failure before a host writer becomes bound to the compiled output scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttachedPointSinkAdmissionErrorV1 {
    /// Host-owned scope differs from the compiler-minted attachment scope.
    ScopeChanged,
    /// A fresh process-local binding epoch could not be minted.
    EpochExhausted,
    /// Pre-install reusable storage could not be reserved.
    ResourceExhausted,
}

/// Failure of the post-admission host stamp protocol.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AttachedPointSinkErrorV1<HostError> {
    /// Patch cardinality or sink identities differ from the admitted scope.
    PatchScopeMismatch,
    /// Expected sequence or binding epoch differs from the committed stamp.
    StampMismatch,
    /// Confirmed revision differs from the committed revision.
    RevisionMismatch,
    /// One prepared command was installed more than once.
    AlreadyInstalled,
    /// The synchronous host rejected the atomic command.
    Host(HostError),
}

/// Renderer provenance carried by the first FV authority slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RendererProvenanceV1 {
    /// Host materialization succeeded, but no renderer observation is claimed.
    Unverified,
}

/// Public projection of the registered appearance surround.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AppearanceSurroundV1 {
    /// Average surround.
    Average,
    /// Dim surround.
    Dim,
    /// Dark surround.
    Dark,
}

/// Typed refusal to issue or reuse an attached materialization authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AttachedMaterializationAuthorityErrorV1 {
    /// Requested presentation metadata is not compiler-minted terminal data.
    NonTerminalRoot,
    /// A candidate root is consumed downstream and cannot be terminal.
    RootConsumedDownstream,
    /// Published sink revision differs from the certificate/current attachment.
    PublishedRevisionMismatch,
    /// Program content identity differs, including occurrence appearance context.
    ProgramIdentityMismatch,
    /// Capability belongs to a different live compiled owner generation.
    ForeignOwnerGeneration,
    /// Capability belongs to another host binding incarnation.
    ForeignBindingEpoch,
    /// No exact point-causal replay exists for the attached presentation.
    MissingExactPointAbsenceProof,
    /// Exact replay proves that the target owns no final terminal contribution.
    EmptyFinalOwnedDomain,
    /// Authority storage could not be allocated after a successful commit.
    ResourceExhausted,
}

/// Provisional pre-1.0 capability for one installed point materialization and
/// one certified physical case.
///
/// There is intentionally no public constructor. Values are minted only from
/// `Ready` [`AttachmentCommitV1::render_outputs`] plus the matching
/// point-causal replay owned by that exact certificate.
pub struct AttachedMaterializationAuthorityV1 {
    content_identity: ContentIdentityV9,
    published_revision: u64,
    sink_stamp: PointSinkStampV1,
    presentation_root: PresentationRootIdV1,
    presentation_occurrence: OccurrenceIdV1,
    terminal_occurrence: OccurrenceIdV1,
    appearance_context: AppearanceContextV1,
    _point_domain_proof: ExactFinalOwnedPointDomainV1,
    composite: Srgb8,
    paint: EncodedPointPaintV1,
    case_index: usize,
    renderer_provenance: RendererProvenanceV1,
    owner_pin: ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
}

impl AttachedMaterializationAuthorityV1 {
    /// Canonical Program content identity.
    #[must_use]
    pub const fn content_identity(&self) -> [u8; 32] {
        *self.content_identity.as_bytes()
    }

    /// Revision atomically installed by the attachment.
    #[must_use]
    pub const fn published_revision(&self) -> u64 {
        self.published_revision
    }

    /// Published sink sequence.
    #[must_use]
    pub const fn sink_sequence(&self) -> u64 {
        self.sink_stamp.sequence()
    }

    /// Process-local host binding incarnation.
    #[must_use]
    pub fn sink_binding_epoch(&self) -> u64 {
        self.sink_stamp.binding_epoch().0.get()
    }

    /// Compiler-minted presentation root.
    #[must_use]
    pub const fn presentation_root(&self) -> u32 {
        self.presentation_root.value()
    }

    /// Presentation target occurrence whose contribution was replayed.
    #[must_use]
    pub const fn presentation_occurrence(&self) -> u32 {
        self.presentation_occurrence.value()
    }

    /// Compiler-minted terminal occurrence of the presentation root.
    #[must_use]
    pub const fn terminal_occurrence(&self) -> u32 {
        self.terminal_occurrence.value()
    }

    /// Canonical physical-case index inside the certified observation.
    #[must_use]
    pub const fn case_index(&self) -> usize {
        self.case_index
    }

    /// Admitted CIECAM16 adapting luminance in cd/m².
    #[must_use]
    pub fn adapting_luminance_cd_m2(&self) -> f64 {
        self.appearance_context.adapting_luminance_cd_m2()
    }

    /// Admitted background luminance ratio `Y_b/Y_w`.
    #[must_use]
    pub fn background_luminance_ratio_yb_yw(&self) -> f64 {
        self.appearance_context.background_luminance_ratio_yb_yw()
    }

    /// Registered surround of the exact terminal occurrence.
    #[must_use]
    pub const fn appearance_surround(&self) -> AppearanceSurroundV1 {
        match self.appearance_context.surround() {
            SurroundV1::Average => AppearanceSurroundV1::Average,
            SurroundV1::Dim => AppearanceSurroundV1::Dim,
            SurroundV1::Dark => AppearanceSurroundV1::Dark,
        }
    }

    /// Encoded-sRGB8 source installed in the host before backdrop composition.
    #[must_use]
    pub const fn source(&self) -> Srgb8 {
        self.paint.value().source()
    }

    /// Installed straight alpha.
    #[must_use]
    pub fn opacity(&self) -> f64 {
        self.paint.value().opacity().value()
    }

    /// Exact final-owned composite proven by counterfactual replay.
    #[must_use]
    pub const fn composite(&self) -> Srgb8 {
        self.composite
    }

    /// Renderer provenance of this first slice.
    #[must_use]
    pub const fn renderer_provenance(&self) -> RendererProvenanceV1 {
        self.renderer_provenance
    }
}

pub(crate) fn same_authority_owner_generation(
    left: &AttachedMaterializationAuthorityV1,
    right: &ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
) -> bool {
    left.owner_pin.same_generation(right)
}

pub(crate) type AttachedProgramAttachmentV1<H> =
    ExternallyManagedAttachmentV1<AdmittedAttachedPointSinkWriterV1<H>>;

pub(crate) struct AttachedPointSinkWriterV1<H>
where
    H: AttachedPointSinkHostV1,
{
    owned_scope: Vec<AttachedPointSinkOutputIdV1>,
    host: H,
}

pub(crate) fn attached_point_sink<H>(
    owned_scope: Vec<AttachedPointSinkOutputIdV1>,
    host: H,
) -> AttachedPointSinkWriterV1<H>
where
    H: AttachedPointSinkHostV1,
{
    AttachedPointSinkWriterV1 { owned_scope, host }
}

pub(crate) struct AdmittedAttachedPointSinkWriterV1<H>
where
    H: AttachedPointSinkHostV1,
{
    owned_scope: Vec<AttachedPointSinkOutputIdV1>,
    binding_epoch: PointSinkBindingEpochV1,
    committed_stamp: PointSinkStampV1,
    committed_revision: Option<u64>,
    committed_patch: Vec<AttachedPointSinkHostPatchEntryV1>,
    scratch_patch: Vec<AttachedPointSinkHostPatchEntryV1>,
    host: H,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreparedAttachedPointSinkActionV1 {
    SetAll {
        revision: u64,
        expected: PointSinkStampV1,
        desired: PointSinkStampV1,
    },
    RevokeAll {
        revision: u64,
        expected: PointSinkStampV1,
        desired: PointSinkStampV1,
    },
    ConfirmExact {
        revision: u64,
        published: PointSinkStampV1,
    },
}

pub(crate) struct PreparedAttachedPointSinkWriteV1<'writer, H>
where
    H: AttachedPointSinkHostV1,
{
    writer: &'writer mut AdmittedAttachedPointSinkWriterV1<H>,
    action: Option<PreparedAttachedPointSinkActionV1>,
}

impl<H> sink_private::Sealed for AttachedPointSinkWriterV1<H> where H: AttachedPointSinkHostV1 {}
impl<H> sink_private::Sealed for AdmittedAttachedPointSinkWriterV1<H> where
    H: AttachedPointSinkHostV1
{
}

impl<H> AttachedPointSinkWriterV1<H>
where
    H: AttachedPointSinkHostV1,
{
    fn finish_admission(
        self,
    ) -> Result<
        PointSinkWriterAdmissionV1<AdmittedAttachedPointSinkWriterV1<H>>,
        PointSinkAdmissionFailureV1<Self>,
    > {
        let Some(binding_epoch) = next_attached_point_sink_epoch_v1() else {
            return Err(PointSinkAdmissionFailureV1::new(
                AttachedPointSinkAdmissionErrorV1::EpochExhausted,
                self,
            ));
        };
        let mut committed_patch = Vec::new();
        if committed_patch.try_reserve_exact(self.owned_scope.len()).is_err() {
            return Err(PointSinkAdmissionFailureV1::new(
                AttachedPointSinkAdmissionErrorV1::ResourceExhausted,
                self,
            ));
        }
        let mut scratch_patch = Vec::new();
        if scratch_patch.try_reserve_exact(self.owned_scope.len()).is_err() {
            return Err(PointSinkAdmissionFailureV1::new(
                AttachedPointSinkAdmissionErrorV1::ResourceExhausted,
                self,
            ));
        }
        let initial_stamp = PointSinkStampV1::new(0, binding_epoch);
        let Self { owned_scope, host } = self;
        Ok(PointSinkWriterAdmissionV1::new(
            AdmittedAttachedPointSinkWriterV1 {
                owned_scope,
                binding_epoch,
                committed_stamp: initial_stamp,
                committed_revision: None,
                committed_patch,
                scratch_patch,
                host,
            },
        ))
    }
}

impl<H> UnboundPointSinkWriterV1 for AttachedPointSinkWriterV1<H>
where
    H: AttachedPointSinkHostV1,
{
    type OutputId = AttachedPointSinkOutputIdV1;
    type Writer = AdmittedAttachedPointSinkWriterV1<H>;
    type AdmissionError = AttachedPointSinkAdmissionErrorV1;

    fn owned_output_scope(&self) -> &[Self::OutputId] {
        &self.owned_scope
    }

    fn try_admit_writer(
        self,
        scope: BoundPointSinkScopePermitV1<'_, Self::OutputId>,
    ) -> Result<PointSinkWriterAdmissionV1<Self::Writer>, PointSinkAdmissionFailureV1<Self>> {
        if !scope.output_scope().eq(self.owned_scope.iter().copied()) {
            return Err(PointSinkAdmissionFailureV1::new(
                AttachedPointSinkAdmissionErrorV1::ScopeChanged,
                self,
            ));
        }
        self.finish_admission()
    }
}

impl<H> PointSinkWriterV1 for AdmittedAttachedPointSinkWriterV1<H>
where
    H: AttachedPointSinkHostV1,
{
    type OutputId = AttachedPointSinkOutputIdV1;
    type Error = AttachedPointSinkErrorV1<H::Error>;
    type Prepared<'writer>
        = PreparedAttachedPointSinkWriteV1<'writer, H>
    where
        Self: 'writer;

    fn binding_epoch(&self) -> PointSinkBindingEpochV1 {
        self.binding_epoch
    }

    fn prepare<'writer>(
        &'writer mut self,
        intent: PointSinkIntentV1<'_, Self::OutputId>,
    ) -> Result<Self::Prepared<'writer>, Self::Error> {
        let action = match intent {
            PointSinkIntentV1::SetAll {
                revision,
                stamp,
                patch,
            } => {
                if stamp.expected() != self.committed_stamp {
                    return Err(AttachedPointSinkErrorV1::StampMismatch);
                }
                if patch.len() != self.owned_scope.len()
                    || self.scratch_patch.capacity() < patch.len()
                {
                    return Err(AttachedPointSinkErrorV1::PatchScopeMismatch);
                }
                self.scratch_patch.clear();
                for (entry, expected_sink) in patch.iter().copied().zip(&self.owned_scope) {
                    if entry.sink_output() != *expected_sink {
                        self.scratch_patch.clear();
                        return Err(AttachedPointSinkErrorV1::PatchScopeMismatch);
                    }
                    let paint = entry.paint().value();
                    self.scratch_patch.push(AttachedPointSinkHostPatchEntryV1 {
                        output: entry.output().value(),
                        sink_output: entry.sink_output().value(),
                        source: paint.source(),
                        opacity: paint.opacity().value(),
                    });
                }
                PreparedAttachedPointSinkActionV1::SetAll {
                    revision,
                    expected: stamp.expected(),
                    desired: stamp.desired(),
                }
            }
            PointSinkIntentV1::RevokeAll { revision, stamp } => {
                if stamp.expected() != self.committed_stamp {
                    return Err(AttachedPointSinkErrorV1::StampMismatch);
                }
                self.scratch_patch.clear();
                PreparedAttachedPointSinkActionV1::RevokeAll {
                    revision,
                    expected: stamp.expected(),
                    desired: stamp.desired(),
                }
            }
            PointSinkIntentV1::ConfirmExact {
                revision,
                published_stamp,
            } => {
                if published_stamp != self.committed_stamp {
                    return Err(AttachedPointSinkErrorV1::StampMismatch);
                }
                if self.committed_revision != Some(revision) {
                    return Err(AttachedPointSinkErrorV1::RevisionMismatch);
                }
                PreparedAttachedPointSinkActionV1::ConfirmExact {
                    revision,
                    published: published_stamp,
                }
            }
        };
        Ok(PreparedAttachedPointSinkWriteV1 {
            writer: self,
            action: Some(action),
        })
    }
}

impl<H> PreparedPointSinkWriteV1 for PreparedAttachedPointSinkWriteV1<'_, H>
where
    H: AttachedPointSinkHostV1,
{
    type Error = AttachedPointSinkErrorV1<H::Error>;

    fn try_install(&mut self) -> Result<(), Self::Error> {
        let Some(action) = self.action else {
            return Err(AttachedPointSinkErrorV1::AlreadyInstalled);
        };
        let binding_epoch = self.writer.binding_epoch.0.get();
        match action {
            PreparedAttachedPointSinkActionV1::SetAll {
                revision,
                expected,
                desired,
            } => {
                if self.writer.committed_stamp != expected {
                    return Err(AttachedPointSinkErrorV1::StampMismatch);
                }
                self.writer
                    .host
                    .try_install(AttachedPointSinkHostIntentV1::SetAll {
                        revision,
                        binding_epoch,
                        expected_sequence: expected.sequence(),
                        desired_sequence: desired.sequence(),
                        patch: &self.writer.scratch_patch,
                    })
                    .map_err(AttachedPointSinkErrorV1::Host)?;
                mem::swap(
                    &mut self.writer.committed_patch,
                    &mut self.writer.scratch_patch,
                );
                self.writer.committed_stamp = desired;
                self.writer.committed_revision = Some(revision);
            }
            PreparedAttachedPointSinkActionV1::RevokeAll {
                revision,
                expected,
                desired,
            } => {
                if self.writer.committed_stamp != expected {
                    return Err(AttachedPointSinkErrorV1::StampMismatch);
                }
                self.writer
                    .host
                    .try_install(AttachedPointSinkHostIntentV1::RevokeAll {
                        revision,
                        binding_epoch,
                        expected_sequence: expected.sequence(),
                        desired_sequence: desired.sequence(),
                    })
                    .map_err(AttachedPointSinkErrorV1::Host)?;
                mem::swap(
                    &mut self.writer.committed_patch,
                    &mut self.writer.scratch_patch,
                );
                self.writer.committed_stamp = desired;
                self.writer.committed_revision = Some(revision);
            }
            PreparedAttachedPointSinkActionV1::ConfirmExact {
                revision,
                published,
            } => {
                if self.writer.committed_stamp != published
                    || self.writer.committed_revision != Some(revision)
                {
                    return Err(AttachedPointSinkErrorV1::RevisionMismatch);
                }
                self.writer
                    .host
                    .try_install(AttachedPointSinkHostIntentV1::ConfirmExact {
                        revision,
                        binding_epoch,
                        published_sequence: published.sequence(),
                        patch: &self.writer.committed_patch,
                    })
                    .map_err(AttachedPointSinkErrorV1::Host)?;
            }
        }
        self.action = None;
        Ok(())
    }

    fn finish_after_session(self) {}
}

pub(crate) fn mint_authorities(
    commit: AttachmentCommitV1<'_, AttachedPointSinkOutputIdV1>,
    owner_pin: &ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
) -> Result<Vec<AttachedMaterializationAuthorityV1>, AttachedMaterializationAuthorityErrorV1> {
    let render_outputs = commit.render_outputs();
    let mut authorities = Vec::new();
    let estimated = render_outputs.len();
    authorities
        .try_reserve(estimated)
        .map_err(|_| AttachedMaterializationAuthorityErrorV1::ResourceExhausted)?;

    for render in render_outputs {
        mint_render_output_authorities(render, owner_pin, &mut authorities)?;
    }
    Ok(authorities)
}

fn mint_render_output_authorities(
    render: AttachedRenderOutputV1<'_, AttachedPointSinkOutputIdV1>,
    owner_pin: &ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
    authorities: &mut Vec<AttachedMaterializationAuthorityV1>,
) -> Result<(), AttachedMaterializationAuthorityErrorV1> {
    let certificate = render.certificate();
    let published = render.published_stamp();
    if certificate.observation().revision() != published.revision() {
        return Err(AttachedMaterializationAuthorityErrorV1::PublishedRevisionMismatch);
    }
    let content_identity = certificate.content_identity();
    let compiled = render.patch.presentation.compiled;
    let mut matched = 0_usize;
    for causal in certificate
        .inner
        .point_causal_certificates()
        .filter(|causal| {
            causal.presentation_root().value() == render.root().value()
                && causal.target().value() == render.occurrence().value()
        })
    {
        matched += 1;
        if causal.content_identity().as_bytes() != content_identity.as_bytes() {
            return Err(AttachedMaterializationAuthorityErrorV1::ProgramIdentityMismatch);
        }
        if causal.observation().revision().value() != published.revision() {
            return Err(AttachedMaterializationAuthorityErrorV1::PublishedRevisionMismatch);
        }
        if causal.modeled_terminal_occurrence() != compiled.terminal() {
            return Err(AttachedMaterializationAuthorityErrorV1::NonTerminalRoot);
        }
        let domain = causal.domain();
        let visible = match domain {
            ExactFinalOwnedPointDomainV1::Singleton { visible } => visible,
            ExactFinalOwnedPointDomainV1::Empty => {
                return Err(AttachedMaterializationAuthorityErrorV1::EmptyFinalOwnedDomain);
            }
        };
        authorities
            .try_reserve(1)
            .map_err(|_| AttachedMaterializationAuthorityErrorV1::ResourceExhausted)?;
        authorities.push(AttachedMaterializationAuthorityV1 {
            content_identity,
            published_revision: published.revision(),
            sink_stamp: published.sink_stamp(),
            presentation_root: render.root(),
            presentation_occurrence: render.occurrence(),
            terminal_occurrence: OccurrenceIdV1::from_core(compiled.terminal()),
            appearance_context: AppearanceContextV1::from_core(compiled.terminal_context()),
            _point_domain_proof: domain,
            composite: Srgb8::new(visible),
            paint: render.paint(),
            case_index: causal.case_index(),
            renderer_provenance: RendererProvenanceV1::Unverified,
            owner_pin: owner_pin.clone(),
        });
    }
    if matched == 0 {
        return Err(AttachedMaterializationAuthorityErrorV1::MissingExactPointAbsenceProof);
    }
    Ok(())
}

pub(crate) fn validate_authority<W>(
    attachment: &ExternallyManagedAttachmentV1<W>,
    owner_pin: &ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
    expected_content_identity: ContentIdentityV9,
    authority: &AttachedMaterializationAuthorityV1,
) -> Result<(), AttachedMaterializationAuthorityErrorV1>
where
    W: PointSinkWriterV1<OutputId = AttachedPointSinkOutputIdV1>,
{
    if authority.content_identity != expected_content_identity {
        return Err(AttachedMaterializationAuthorityErrorV1::ProgramIdentityMismatch);
    }
    if !authority.owner_pin.same_generation(owner_pin) {
        return Err(AttachedMaterializationAuthorityErrorV1::ForeignOwnerGeneration);
    }
    let current_stamp = attachment.state.expected_sink_stamp;
    if authority.sink_stamp.binding_epoch() != current_stamp.binding_epoch() {
        return Err(AttachedMaterializationAuthorityErrorV1::ForeignBindingEpoch);
    }
    if attachment.state.committed_revision != Some(authority.published_revision) {
        return Err(AttachedMaterializationAuthorityErrorV1::PublishedRevisionMismatch);
    }
    Ok(())
}

pub(crate) fn owner_pin(
    owner: &super::super::OwnerV1,
) -> ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1> {
    owner.compiled.pin_owner()
}

fn next_attached_point_sink_epoch_v1() -> Option<PointSinkBindingEpochV1> {
    NEXT_ATTACHED_POINT_SINK_EPOCH_V1
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .ok()
        .and_then(NonZeroU64::new)
        .map(PointSinkBindingEpochV1::new)
}

#[allow(dead_code)]
fn _type_anchor(_: &CoreVerifiedV1) -> (OutputSlotIdV1, AppearanceContextV1) {
    unreachable!()
}
