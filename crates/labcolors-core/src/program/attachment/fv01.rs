use core::{
    mem,
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::program::{AppearanceContextV1, ContentIdentityV9, OccurrenceIdV1};
use crate::program_session::{CoreProgramEvaluatorsV1, ProgramOwnerLeaseV1};
use crate::program_wire::{
    AttachedMaterializationAuthorityErrorV1, AttachedMaterializationAuthorityPartsV1,
    AttachedMaterializationAuthorityV1, AttachedMaterializationCaseProofV1,
    AttachedPointSinkAdmissionErrorV1, AttachedPointSinkErrorV1, AttachedPointSinkHostIntentV1,
    AttachedPointSinkHostPatchEntryV1, AttachedPointSinkHostV1,
};

use super::{
    AttachedRenderOutputV1, AttachedRenderOutputsV1, BoundPointSinkScopePermitV1,
    ExternallyManagedAttachmentV1, PointSinkAdmissionFailureV1, PointSinkBindingEpochV1,
    PointSinkIntentV1, PointSinkStampV1, PointSinkWriterAdmissionV1, PointSinkWriterV1,
    PreparedPointSinkWriteV1, UnboundPointSinkWriterV1, sink_private,
};

static NEXT_ATTACHED_POINT_SINK_EPOCH_V1: AtomicU64 = AtomicU64::new(1);

#[cfg(test)]
std::thread_local! {
    static AUTHORITY_RESERVATION_FAILURE_ARMED: core::cell::Cell<bool> = const {
        core::cell::Cell::new(false)
    };
}

#[cfg(test)]
pub(crate) struct AttachedAuthorityReservationFailureGuardV1;

#[cfg(test)]
impl Drop for AttachedAuthorityReservationFailureGuardV1 {
    fn drop(&mut self) {
        AUTHORITY_RESERVATION_FAILURE_ARMED.with(|armed| armed.set(false));
    }
}

#[cfg(test)]
pub(crate) fn fail_next_attached_authority_reservation_for_test(
) -> AttachedAuthorityReservationFailureGuardV1 {
    AUTHORITY_RESERVATION_FAILURE_ARMED.with(|armed| {
        assert!(!armed.replace(true), "an authority reservation failure is already armed");
    });
    AttachedAuthorityReservationFailureGuardV1
}

fn try_reserve_authority<T>(
    values: &mut Vec<T>,
    additional: usize,
) -> Result<(), AttachedMaterializationAuthorityErrorV1> {
    #[cfg(test)]
    if additional != 0 && AUTHORITY_RESERVATION_FAILURE_ARMED.with(|armed| armed.replace(false)) {
        return Err(AttachedMaterializationAuthorityErrorV1::ResourceExhausted);
    }
    values
        .try_reserve(additional)
        .map_err(|_| AttachedMaterializationAuthorityErrorV1::ResourceExhausted)
}

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
        if committed_patch
            .try_reserve_exact(self.owned_scope.len())
            .is_err()
        {
            return Err(PointSinkAdmissionFailureV1::new(
                AttachedPointSinkAdmissionErrorV1::ResourceExhausted,
                self,
            ));
        }
        let mut scratch_patch = Vec::new();
        if scratch_patch
            .try_reserve_exact(self.owned_scope.len())
            .is_err()
        {
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

/// Fallible proof material prepared before host installation. The contained
/// capability values remain sealed inside this private batch until the exact
/// attachment transition commits successfully.
pub(crate) struct PreparedAttachedAuthoritiesV1 {
    authorities: Vec<AttachedMaterializationAuthorityV1>,
}

impl PreparedAttachedAuthoritiesV1 {
    pub(crate) fn is_empty(&self) -> bool {
        self.authorities.is_empty()
    }

    /// Capability publication is intentionally an infallible move after commit.
    pub(crate) fn seal_after_commit(self) -> Vec<AttachedMaterializationAuthorityV1> {
        self.authorities
    }
}

pub(crate) fn prepare_authorities(
    render_outputs: AttachedRenderOutputsV1<'_, AttachedPointSinkOutputIdV1>,
    owner_pin: &ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
) -> Result<PreparedAttachedAuthoritiesV1, AttachedMaterializationAuthorityErrorV1> {
    let mut authorities = Vec::new();
    try_reserve_authority(&mut authorities, render_outputs.len())?;

    for render in render_outputs {
        authorities.push(prepare_render_output_authority(render, owner_pin)?);
    }
    Ok(PreparedAttachedAuthoritiesV1 { authorities })
}

fn prepare_render_output_authority(
    render: AttachedRenderOutputV1<'_, AttachedPointSinkOutputIdV1>,
    owner_pin: &ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
) -> Result<AttachedMaterializationAuthorityV1, AttachedMaterializationAuthorityErrorV1> {
    let certificate = render.certificate();
    let published = render.published_stamp();
    if certificate.observation().revision() != published.revision() {
        return Err(AttachedMaterializationAuthorityErrorV1::PublishedRevisionMismatch);
    }

    let content_identity = certificate.content_identity();
    let compiled = render.patch.presentation.compiled;
    let mut cases = Vec::new();
    for causal in certificate
        .inner
        .point_causal_certificates()
        .filter(|causal| {
            causal.presentation_root().value() == render.root().value()
                && causal.target().value() == render.occurrence().value()
        })
    {
        if causal.content_identity().as_bytes() != content_identity.as_bytes() {
            return Err(AttachedMaterializationAuthorityErrorV1::ProgramIdentityMismatch);
        }
        if causal.observation().revision().value() != published.revision() {
            return Err(AttachedMaterializationAuthorityErrorV1::PublishedRevisionMismatch);
        }
        if causal.modeled_terminal_occurrence() != compiled.terminal() {
            return Err(AttachedMaterializationAuthorityErrorV1::NonTerminalRoot);
        }
        cases
            .try_reserve(1)
            .map_err(|_| AttachedMaterializationAuthorityErrorV1::ResourceExhausted)?;
        cases.push(AttachedMaterializationCaseProofV1::from_domain(
            causal.case_index(),
            causal.domain(),
        )?);
    }
    if cases.is_empty() {
        return Err(AttachedMaterializationAuthorityErrorV1::MissingExactPointAbsenceProof);
    }

    let sink_stamp = published.sink_stamp();
    AttachedMaterializationAuthorityV1::from_attached_parts(
        AttachedMaterializationAuthorityPartsV1 {
            content_identity,
            published_revision: published.revision(),
            sink_stamp,
            sink_binding_epoch: sink_stamp.binding_epoch().0.get(),
            presentation_root: render.root(),
            presentation_occurrence: render.occurrence(),
            terminal_occurrence: OccurrenceIdV1::from_core(compiled.terminal()),
            appearance_context: AppearanceContextV1::from_core(compiled.terminal_context()),
            paint: render.paint(),
            cases: cases.into_boxed_slice(),
            owner_pin: owner_pin.clone(),
        },
    )
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
    if authority.stored_content_identity() != expected_content_identity {
        return Err(AttachedMaterializationAuthorityErrorV1::ProgramIdentityMismatch);
    }
    if !authority.same_owner_generation(owner_pin) {
        return Err(AttachedMaterializationAuthorityErrorV1::ForeignOwnerGeneration);
    }
    let current_stamp = attachment.state.expected_sink_stamp;
    if authority.stored_sink_stamp().binding_epoch() != current_stamp.binding_epoch() {
        return Err(AttachedMaterializationAuthorityErrorV1::ForeignBindingEpoch);
    }
    if attachment.state.committed_revision != Some(authority.published_revision()) {
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
