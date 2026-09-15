//! FV-01 public attached-materialization contract.
//!
//! The public port lives here so no staged `program` source becomes part of the
//! resolved Rust API. The physical transaction and authority mint remain inside
//! the private attachment implementation.

use core::iter::FusedIterator;

use crate::Srgb8;
use crate::appearance::{EncodedPointPaintV1, ExactFinalOwnedPointDomainV1};
use crate::program::attachment::PointSinkStampV1;
use crate::program::{
    AppearanceContextV1, ContentIdentityV9, OccurrenceIdV1, PresentationRootIdV1, SurroundV1,
};
use crate::program_session::{CoreProgramEvaluatorsV1, ProgramOwnerLeaseV1};

/// One complete host patch entry after compiler admission.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttachedPointSinkHostPatchEntryV1 {
    pub(crate) output: u32,
    pub(crate) sink_output: u32,
    pub(crate) source: Srgb8,
    pub(crate) opacity: f64,
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
    fn try_install(&mut self, intent: AttachedPointSinkHostIntentV1<'_>)
    -> Result<(), Self::Error>;
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

/// Renderer provenance carried by the first attached-materialization slice.
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
    /// Authority storage could not be allocated before a usable value is returned.
    ResourceExhausted,
}

/// One exact physical-case projection carried by an attached authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachedMaterializationCaseV1 {
    case_index: usize,
    composite: Srgb8,
}

impl AttachedMaterializationCaseV1 {
    /// Canonical physical-case index inside the certified observation.
    #[must_use]
    pub const fn case_index(self) -> usize {
        self.case_index
    }

    /// Exact final-owned composite proven by counterfactual replay.
    #[must_use]
    pub const fn composite(self) -> Srgb8 {
        self.composite
    }
}

pub(crate) struct AttachedMaterializationCaseProofV1 {
    case_index: usize,
    _domain: ExactFinalOwnedPointDomainV1,
    composite: Srgb8,
}

impl AttachedMaterializationCaseProofV1 {
    pub(crate) fn from_domain(
        case_index: usize,
        domain: ExactFinalOwnedPointDomainV1,
    ) -> Result<Self, AttachedMaterializationAuthorityErrorV1> {
        let ExactFinalOwnedPointDomainV1::Singleton { visible } = domain else {
            return Err(AttachedMaterializationAuthorityErrorV1::EmptyFinalOwnedDomain);
        };
        Ok(Self {
            case_index,
            _domain: domain,
            composite: Srgb8::new(visible),
        })
    }

    const fn public(&self) -> AttachedMaterializationCaseV1 {
        AttachedMaterializationCaseV1 {
            case_index: self.case_index,
            composite: self.composite,
        }
    }
}

/// Provisional pre-1.0 capability for one installed point materialization.
///
/// There is intentionally no public constructor. A value represents one
/// `(Program content identity × presentation root × sink stamp)` materialization
/// and carries every exact physical-case contribution proof from the matching
/// `Ready` attachment commit. Detached snapshots, Paint values, sRGB8 values and
/// CSS strings cannot construct this type.
pub struct AttachedMaterializationAuthorityV1 {
    content_identity: ContentIdentityV9,
    published_revision: u64,
    sink_stamp: PointSinkStampV1,
    sink_binding_epoch: u64,
    presentation_root: PresentationRootIdV1,
    presentation_occurrence: OccurrenceIdV1,
    terminal_occurrence: OccurrenceIdV1,
    appearance_context: AppearanceContextV1,
    paint: EncodedPointPaintV1,
    cases: Box<[AttachedMaterializationCaseProofV1]>,
    renderer_provenance: RendererProvenanceV1,
    owner_pin: ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
}

pub(crate) struct AttachedMaterializationAuthorityPartsV1 {
    pub(crate) content_identity: ContentIdentityV9,
    pub(crate) published_revision: u64,
    pub(crate) sink_stamp: PointSinkStampV1,
    pub(crate) sink_binding_epoch: u64,
    pub(crate) presentation_root: PresentationRootIdV1,
    pub(crate) presentation_occurrence: OccurrenceIdV1,
    pub(crate) terminal_occurrence: OccurrenceIdV1,
    pub(crate) appearance_context: AppearanceContextV1,
    pub(crate) paint: EncodedPointPaintV1,
    pub(crate) cases: Box<[AttachedMaterializationCaseProofV1]>,
    pub(crate) owner_pin: ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
}

impl AttachedMaterializationAuthorityV1 {
    pub(crate) fn from_attached_parts(
        parts: AttachedMaterializationAuthorityPartsV1,
    ) -> Result<Self, AttachedMaterializationAuthorityErrorV1> {
        let AttachedMaterializationAuthorityPartsV1 {
            content_identity,
            published_revision,
            sink_stamp,
            sink_binding_epoch,
            presentation_root,
            presentation_occurrence,
            terminal_occurrence,
            appearance_context,
            paint,
            cases,
            owner_pin,
        } = parts;
        if cases.is_empty() {
            return Err(AttachedMaterializationAuthorityErrorV1::MissingExactPointAbsenceProof);
        }
        Ok(Self {
            content_identity,
            published_revision,
            sink_stamp,
            sink_binding_epoch,
            presentation_root,
            presentation_occurrence,
            terminal_occurrence,
            appearance_context,
            paint,
            cases,
            renderer_provenance: RendererProvenanceV1::Unverified,
            owner_pin,
        })
    }

    pub(crate) fn same_owner_generation(
        &self,
        owner_pin: &ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
    ) -> bool {
        self.owner_pin.same_generation(owner_pin)
    }

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
    pub const fn sink_binding_epoch(&self) -> u64 {
        self.sink_binding_epoch
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

    /// Number of exact physical-case contribution proofs in canonical order.
    #[must_use]
    pub fn case_count(&self) -> usize {
        self.cases.len()
    }

    /// Exact physical-case composites in canonical observation order.
    pub fn cases(
        &self,
    ) -> impl ExactSizeIterator<Item = AttachedMaterializationCaseV1> + FusedIterator + '_ {
        self.cases
            .iter()
            .map(AttachedMaterializationCaseProofV1::public)
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

    /// Renderer provenance of this first slice.
    #[must_use]
    pub const fn renderer_provenance(&self) -> RendererProvenanceV1 {
        self.renderer_provenance
    }

    pub(crate) const fn stored_content_identity(&self) -> ContentIdentityV9 {
        self.content_identity
    }

    pub(crate) const fn stored_sink_stamp(&self) -> PointSinkStampV1 {
        self.sink_stamp
    }
}
