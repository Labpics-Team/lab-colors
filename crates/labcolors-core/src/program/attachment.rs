//! Терминальный point-attachment с одной revision-bound Session и одним writer.
//!
//! Конструктор минтит presentation-корреляции и strong Program pin из одного
//! compiled owner. Поэтому runtime-update не принимает независимые owner,
//! Session, stamp или sink handle, которые клиент мог бы перепутать.

use core::{iter::FusedIterator, mem};

use crate::appearance::EncodedPointPaintV1;
use crate::program_session::{
    CompiledPointOutputPresentationV1, CoreProgramEvaluatorsV1, PointOutputPresentationBindErrorV1,
    ProgramOwnerLeaseV1, ProgramPaintOutputV1,
};
use crate::session::PreparedSessionDispositionV1;

use super::{
    CoreDeferredSessionRetirementV1, CorePreparedSessionTransitionV1, EvidenceViewV1,
    InstantiateErrorV1, OccurrenceIdV1, OutputSlotIdV1, OwnerV1, PresentationRootIdV1,
    SessionState, SessionV1, UpdateErrorV1, UpdateV1, VerifiedCertificateV1,
};

/// Sealing оставляет реализации физического writer внутри пакета.
pub(crate) mod sink_private {
    pub(crate) trait Sealed {}
}

/// Одна authored и ещё не доверенная корреляция output→sink для emission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthoredPointEmissionBindingV1<SinkOutputId> {
    output: OutputSlotIdV1,
    sink_output: SinkOutputId,
}

impl<SinkOutputId: Copy> AuthoredPointEmissionBindingV1<SinkOutputId> {
    pub(crate) const fn new(output: OutputSlotIdV1, sink_output: SinkOutputId) -> Self {
        Self {
            output,
            sink_output,
        }
    }
}

/// Одна authored relation output→presentation; fan-out разрешён явно.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthoredPointPresentationBindingV1 {
    output: OutputSlotIdV1,
    root: PresentationRootIdV1,
    occurrence: OccurrenceIdV1,
}

impl AuthoredPointPresentationBindingV1 {
    pub(crate) const fn new(
        output: OutputSlotIdV1,
        root: PresentationRootIdV1,
        occurrence: OccurrenceIdV1,
    ) -> Self {
        Self {
            output,
            root,
            occurrence,
        }
    }
}

/// Сминченная компилятором unique emission-корреляция в output order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttachedPointEmissionV1<SinkOutputId> {
    output_ordinal: usize,
    output: OutputSlotIdV1,
    sink_output: SinkOutputId,
}

impl<SinkOutputId: Copy> AttachedPointEmissionV1<SinkOutputId> {
    pub(crate) const fn output(self) -> OutputSlotIdV1 {
        self.output
    }

    pub(crate) const fn sink_output(self) -> SinkOutputId {
        self.sink_output
    }
}

/// Сминченная compiler relation; output и sink могут законно повторяться.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttachedPointPresentationV1<SinkOutputId> {
    compiled: CompiledPointOutputPresentationV1,
    sink_output: SinkOutputId,
}

impl<SinkOutputId: Copy> AttachedPointPresentationV1<SinkOutputId> {
    pub(crate) const fn output(self) -> OutputSlotIdV1 {
        OutputSlotIdV1::from_core(self.compiled.output())
    }

    pub(crate) const fn root(self) -> PresentationRootIdV1 {
        PresentationRootIdV1::from_core(self.compiled.root())
    }

    pub(crate) const fn occurrence(self) -> OccurrenceIdV1 {
        OccurrenceIdV1::from_core(self.compiled.occurrence())
    }

    pub(crate) const fn sink_output(self) -> SinkOutputId {
        self.sink_output
    }
}

/// Один элемент полного сертифицированного point-снимка для sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSinkPatchEntryV1<SinkOutputId> {
    emission: AttachedPointEmissionV1<SinkOutputId>,
    paint: EncodedPointPaintV1,
}

impl<SinkOutputId: Copy> PointSinkPatchEntryV1<SinkOutputId> {
    pub(crate) const fn output(self) -> OutputSlotIdV1 {
        self.emission.output()
    }

    pub(crate) const fn sink_output(self) -> SinkOutputId {
        self.emission.sink_output()
    }

    pub(crate) const fn paint(self) -> EncodedPointPaintV1 {
        self.paint
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AttachedRenderPatchEntryV1<SinkOutputId> {
    presentation: AttachedPointPresentationV1<SinkOutputId>,
    paint: EncodedPointPaintV1,
}

/// Единственные три intent, принимаемые терминальным point sink.
pub(crate) enum PointSinkIntentV1<'a, SinkOutputId, Stamp> {
    SetAll {
        revision: u64,
        patch: &'a [PointSinkPatchEntryV1<SinkOutputId>],
    },
    RevokeAll {
        revision: u64,
    },
    ConfirmExact {
        revision: u64,
        published_stamp: &'a Stamp,
    },
}

/// Подготовленная sink-local транзакция целого снимка, удерживающая Busy lease.
///
/// Все реализации подчиняются hard-law all-or-nothing: внешний наблюдатель
/// никогда не видит часть нового scope. Успех заменяет весь принадлежащий lease
/// scope одной атомарной публикацией. Любой отказ сохраняет прежние наблюдаемые
/// scope→value snapshot, revision и равный по [`Eq`] Stamp.
pub(crate) trait PreparedPointSinkWriteV1 {
    type Stamp: Clone + Eq;
    type Error;

    /// Stamp точного снимка, который опубликует успешный install.
    fn proposed_stamp(&self) -> &Self::Stamp;

    /// Единственная fallible-операция после parsing, allocations и CAS setup.
    ///
    /// `Ok` означает, что весь scope уже опубликован атомарно. `Err` означает,
    /// что прежние snapshot, revision и Stamp наблюдаемо не изменились. Пока
    /// Prepared жив, Busy может оставаться занятым; его Drop обязан освободить
    /// Busy, поэтому [`Attachment::update`] возвращает ошибку уже после release.
    fn try_install(&mut self) -> Result<(), Self::Error>;

    /// Освобождает Busy после переноса Session и состояния Attachment.
    ///
    /// Реализация обязана быть infallible и allocation-free. Она только
    /// переносит все owning-значения в заранее очищенный lease retirement-slot
    /// и снимает Busy; Drop/deallocation в этой фазе запрещены.
    fn finish_after_session(self);
}

/// Линейное владение одним точным физическим point-sink scope.
pub(crate) trait LinearPointSinkLeaseV1: sink_private::Sealed {
    type OutputId: Copy + Eq;
    type Stamp: Clone + Eq;
    type Error;
    type Prepared<'lease>: PreparedPointSinkWriteV1<Stamp = Self::Stamp, Error = Self::Error>
    where
        Self: 'lease;

    /// Точный scope, которым эксклюзивно владеет lease, в каноническом порядке
    /// выходов скомпилированной Program.
    fn owned_output_scope(&self) -> &[Self::OutputId];

    /// Готовит полный снимок, не сохраняя borrowed-данные patch.
    ///
    /// `Err` сохраняет прежние наблюдаемые snapshot, revision и Stamp и
    /// возвращает lease с уже свободным Busy. Подготовка не публикует даже
    /// допустимую часть нового снимка.
    fn prepare<'lease>(
        &'lease mut self,
        intent: PointSinkIntentV1<'_, Self::OutputId, Self::Stamp>,
    ) -> Result<Self::Prepared<'lease>, Self::Error>;

    /// Атомарно отзывает полный scope lease перед его освобождением.
    ///
    /// Реализация обязана быть infallible и allocation-free, даже если ни один
    /// снимок ещё не публиковался.
    fn revoke_all_before_release(&mut self, published_stamp: Option<&Self::Stamp>);
}

/// Cold-ошибка создания Attachment; sink ещё ничего не опубликовал.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachmentCreateErrorV1<SinkOutputId> {
    ResourceExhausted,
    Instantiate(InstantiateErrorV1),
    EmissionBindingCount {
        expected: usize,
        actual: usize,
    },
    DuplicateEmissionOutput {
        output: OutputSlotIdV1,
    },
    EmissionOutputMismatch {
        ordinal: usize,
        expected: OutputSlotIdV1,
        authored: OutputSlotIdV1,
    },
    SinkOutputAliased {
        sink_output: SinkOutputId,
        first_output: OutputSlotIdV1,
        second_output: OutputSlotIdV1,
    },
    EmptyPresentations,
    PresentationCount {
        expected: usize,
        actual: usize,
    },
    MissingOutputPresentation {
        output: OutputSlotIdV1,
    },
    DuplicatePresentation {
        root: PresentationRootIdV1,
        occurrence: OccurrenceIdV1,
        first_output: OutputSlotIdV1,
        second_output: OutputSlotIdV1,
    },
    MissingCompiledPresentation {
        presentation_ordinal: usize,
    },
    SinkScopeCount {
        expected: usize,
        actual: usize,
    },
    DuplicateSinkScopeOutput {
        sink_output: SinkOutputId,
    },
    SinkScopeMismatch {
        ordinal: usize,
        binding: SinkOutputId,
        owned: SinkOutputId,
    },
    InvalidPointBinding {
        authored_index: usize,
        cause: PointOutputPresentationBindErrorV1,
    },
    InternalInvariant,
}

/// Закрытый отказ уже скомпилированной терминальной транзакции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachmentInvariantV1 {
    EmptyIdempotentHead,
    MissingPublishedStamp,
    PublishedRevisionMismatch,
    OutputCountMismatch,
    OutputIdentityMismatch,
    PaintIdentityMismatch,
    ScratchCapacityLost,
    ConfirmStampMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttachmentUpdateErrorV1<SinkError> {
    Update(UpdateErrorV1),
    SinkPrepare(SinkError),
    SinkInstall(SinkError),
    InternalInvariant(AttachmentInvariantV1),
}

type AttachmentUpdateResultV1<'a, L> = Result<
    AttachmentCommitV1<
        'a,
        <L as LinearPointSinkLeaseV1>::OutputId,
        <L as LinearPointSinkLeaseV1>::Stamp,
    >,
    AttachmentUpdateErrorV1<<L as LinearPointSinkLeaseV1>::Error>,
>;

/// Prospective sink-смысл одного полностью вычисленного перехода Session.
pub(super) enum PreparedDispositionV1<'a> {
    ConfirmExact {
        revision: u64,
    },
    RevokeAll {
        revision: u64,
    },
    SetAll {
        revision: u64,
        outputs: &'a [ProgramPaintOutputV1],
    },
}

fn prepared_disposition<'prepared>(
    transition: &'prepared CorePreparedSessionTransitionV1<'_>,
) -> Result<PreparedDispositionV1<'prepared>, AttachmentInvariantV1> {
    match transition.disposition() {
        PreparedSessionDispositionV1::Idempotent { raw_head, .. } => raw_head
            .revision()
            .map(|revision| PreparedDispositionV1::ConfirmExact {
                revision: revision.value(),
            })
            .ok_or(AttachmentInvariantV1::EmptyIdempotentHead),
        PreparedSessionDispositionV1::Unknown(unknown) => Ok(PreparedDispositionV1::RevokeAll {
            revision: unknown.revision().value(),
        }),
        PreparedSessionDispositionV1::Verified(verified) => Ok(PreparedDispositionV1::SetAll {
            revision: verified.report().observation().revision().value(),
            outputs: verified.outputs(),
        }),
        PreparedSessionDispositionV1::Violation(violation) => {
            Ok(PreparedDispositionV1::RevokeAll {
                revision: violation.report().observation().revision().value(),
            })
        }
    }
}

struct PublishedAttachmentStampV1<Stamp> {
    revision: u64,
    sink: Stamp,
}

enum PreparedPatchActionV1 {
    SetAll { revision: u64 },
    RevokeAll { revision: u64 },
    ConfirmExact { revision: u64 },
}

impl PreparedPatchActionV1 {
    const fn revision(&self) -> u64 {
        match self {
            Self::SetAll { revision }
            | Self::RevokeAll { revision }
            | Self::ConfirmExact { revision, .. } => *revision,
        }
    }
}

/// Borrowed exact stamp снимка, принадлежащего одному Attachment.
pub(crate) struct AttachedPublishedStampV1<'a, Stamp> {
    inner: &'a PublishedAttachmentStampV1<Stamp>,
}

impl<Stamp> Copy for AttachedPublishedStampV1<'_, Stamp> {}

impl<Stamp> Clone for AttachedPublishedStampV1<'_, Stamp> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<Stamp> AttachedPublishedStampV1<'_, Stamp> {
    pub(crate) const fn revision(self) -> u64 {
        self.inner.revision
    }
}

/// Один элемент final render-authority после commit sink и Session.
pub(crate) struct AttachedRenderOutputV1<'a, SinkOutputId, Stamp> {
    certificate: VerifiedCertificateV1<'a>,
    patch: AttachedRenderPatchEntryV1<SinkOutputId>,
    published_stamp: AttachedPublishedStampV1<'a, Stamp>,
}

impl<SinkOutputId: Copy, Stamp> Copy for AttachedRenderOutputV1<'_, SinkOutputId, Stamp> {}

impl<SinkOutputId: Copy, Stamp> Clone for AttachedRenderOutputV1<'_, SinkOutputId, Stamp> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, SinkOutputId: Copy, Stamp> AttachedRenderOutputV1<'a, SinkOutputId, Stamp> {
    pub(crate) const fn certificate(self) -> VerifiedCertificateV1<'a> {
        self.certificate
    }

    pub(crate) const fn output(self) -> OutputSlotIdV1 {
        self.patch.presentation.output()
    }

    pub(crate) const fn paint(self) -> EncodedPointPaintV1 {
        self.patch.paint
    }

    pub(crate) const fn root(self) -> PresentationRootIdV1 {
        self.patch.presentation.root()
    }

    pub(crate) const fn occurrence(self) -> OccurrenceIdV1 {
        self.patch.presentation.occurrence()
    }

    pub(crate) const fn sink_output(self) -> SinkOutputId {
        self.patch.presentation.sink_output()
    }

    pub(crate) const fn published_stamp(self) -> AttachedPublishedStampV1<'a, Stamp> {
        self.published_stamp
    }
}

/// Точный post-commit view; historical evidence и render authority не смешаны.
pub(crate) struct AttachmentCommitV1<'a, SinkOutputId, Stamp> {
    evidence: EvidenceViewV1<'a>,
    committed_render_patch: &'a [AttachedRenderPatchEntryV1<SinkOutputId>],
    published_stamp: &'a PublishedAttachmentStampV1<Stamp>,
}

impl<SinkOutputId, Stamp> Copy for AttachmentCommitV1<'_, SinkOutputId, Stamp> {}

impl<SinkOutputId, Stamp> Clone for AttachmentCommitV1<'_, SinkOutputId, Stamp> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, SinkOutputId: Copy, Stamp> AttachmentCommitV1<'a, SinkOutputId, Stamp> {
    pub(crate) const fn evidence(self) -> EvidenceViewV1<'a> {
        self.evidence
    }

    pub(crate) fn render_outputs(self) -> AttachedRenderOutputsV1<'a, SinkOutputId, Stamp> {
        let certificate = match self.evidence.state() {
            SessionState::Ready { current } => Some(VerifiedCertificateV1 { inner: current }),
            SessionState::Waiting | SessionState::Stale { .. } | SessionState::Failed { .. } => {
                None
            }
        };
        AttachedRenderOutputsV1 {
            certificate,
            committed_render_patch: self.committed_render_patch,
            published_stamp: AttachedPublishedStampV1 {
                inner: self.published_stamp,
            },
            index: 0,
        }
    }
}

pub(crate) struct AttachedRenderOutputsV1<'a, SinkOutputId, Stamp> {
    certificate: Option<VerifiedCertificateV1<'a>>,
    committed_render_patch: &'a [AttachedRenderPatchEntryV1<SinkOutputId>],
    published_stamp: AttachedPublishedStampV1<'a, Stamp>,
    index: usize,
}

impl<'a, SinkOutputId: Copy, Stamp> Iterator for AttachedRenderOutputsV1<'a, SinkOutputId, Stamp> {
    type Item = AttachedRenderOutputV1<'a, SinkOutputId, Stamp>;

    fn next(&mut self) -> Option<Self::Item> {
        let certificate = self.certificate?;
        let patch = *self.committed_render_patch.get(self.index)?;
        self.index += 1;
        Some(AttachedRenderOutputV1 {
            certificate,
            patch,
            published_stamp: self.published_stamp,
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = if self.certificate.is_some() {
            self.committed_render_patch.len().saturating_sub(self.index)
        } else {
            0
        };
        (remaining, Some(remaining))
    }
}

impl<SinkOutputId: Copy, Stamp> ExactSizeIterator
    for AttachedRenderOutputsV1<'_, SinkOutputId, Stamp>
{
}
impl<SinkOutputId: Copy, Stamp> FusedIterator for AttachedRenderOutputsV1<'_, SinkOutputId, Stamp> {}

/// Владеет одной Session, одним exact Program pin и одним linear writer.
pub(crate) struct Attachment<L>
where
    L: LinearPointSinkLeaseV1,
{
    // Порядок полей задаёт освобождение после `Drop::drop`: writer, Session,
    // инертные снимки и последней — точная Program generation.
    sink: L,
    session: SessionV1,
    emissions: Vec<AttachedPointEmissionV1<L::OutputId>>,
    presentations: Vec<AttachedPointPresentationV1<L::OutputId>>,
    committed_sink_patch: Vec<PointSinkPatchEntryV1<L::OutputId>>,
    scratch_sink_patch: Vec<PointSinkPatchEntryV1<L::OutputId>>,
    committed_render_patch: Vec<AttachedRenderPatchEntryV1<L::OutputId>>,
    scratch_render_patch: Vec<AttachedRenderPatchEntryV1<L::OutputId>>,
    published_stamp: Option<PublishedAttachmentStampV1<L::Stamp>>,
    // Прошлый control block переносится сюда после install и освобождается до
    // следующей транзакции, но не в infallible commit tail.
    retired_stamp: Option<PublishedAttachmentStampV1<L::Stamp>>,
    // Вытеснённые Session evidence и transaction owner после install только
    // переносятся сюда и освобождаются до следующего prepare.
    retired_session: Option<CoreDeferredSessionRetirementV1>,
    _owner_pin: ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
}

struct UnpublishedSinkGuardV1<L: LinearPointSinkLeaseV1> {
    sink: Option<L>,
}

impl<L: LinearPointSinkLeaseV1> UnpublishedSinkGuardV1<L> {
    const fn new(sink: L) -> Self {
        Self { sink: Some(sink) }
    }

    fn sink(&self) -> &L {
        match &self.sink {
            Some(sink) => sink,
            None => unreachable!("guard владеет sink до успешного принятия"),
        }
    }

    fn accept(mut self) -> L {
        match self.sink.take() {
            Some(sink) => sink,
            None => unreachable!("guard принимает sink ровно один раз"),
        }
    }
}

impl<L: LinearPointSinkLeaseV1> Drop for UnpublishedSinkGuardV1<L> {
    fn drop(&mut self) {
        if let Some(sink) = &mut self.sink {
            sink.revoke_all_before_release(None);
        }
    }
}

impl OwnerV1 {
    /// Создаёт один terminal attachment этой exact compiled generation.
    pub(crate) fn attach<L>(
        &self,
        stream_id: u32,
        authored_emissions: &[AuthoredPointEmissionBindingV1<L::OutputId>],
        authored_presentations: &[AuthoredPointPresentationBindingV1],
        sink: L,
    ) -> Result<Attachment<L>, AttachmentCreateErrorV1<L::OutputId>>
    where
        L: LinearPointSinkLeaseV1,
    {
        Attachment::try_new(
            self,
            stream_id,
            authored_emissions,
            authored_presentations,
            sink,
        )
    }
}

impl<L> Attachment<L>
where
    L: LinearPointSinkLeaseV1,
{
    /// Атомарно связывает authored IDs и pin той же exact compiled generation.
    fn try_new(
        owner: &OwnerV1,
        stream_id: u32,
        authored_emissions: &[AuthoredPointEmissionBindingV1<L::OutputId>],
        authored_presentations: &[AuthoredPointPresentationBindingV1],
        sink: L,
    ) -> Result<Self, AttachmentCreateErrorV1<L::OutputId>> {
        let sink = UnpublishedSinkGuardV1::new(sink);
        let expected_outputs = owner.compiled.output_count();
        if authored_emissions.len() != expected_outputs {
            return Err(AttachmentCreateErrorV1::EmissionBindingCount {
                expected: expected_outputs,
                actual: authored_emissions.len(),
            });
        }

        let mut emissions: Vec<AttachedPointEmissionV1<L::OutputId>> = Vec::new();
        emissions
            .try_reserve_exact(expected_outputs)
            .map_err(|_| AttachmentCreateErrorV1::ResourceExhausted)?;
        emissions.extend(authored_emissions.iter().copied().map(|binding| {
            AttachedPointEmissionV1 {
                output_ordinal: 0,
                output: binding.output,
                sink_output: binding.sink_output,
            }
        }));
        emissions.sort_unstable_by_key(|binding| binding.output);
        for adjacent in emissions.windows(2) {
            if adjacent[0].output == adjacent[1].output {
                return Err(AttachmentCreateErrorV1::DuplicateEmissionOutput {
                    output: adjacent[0].output,
                });
            }
        }
        for (output_ordinal, (binding, expected)) in
            emissions.iter_mut().zip(owner.output_slots()).enumerate()
        {
            if binding.output != expected {
                return Err(AttachmentCreateErrorV1::EmissionOutputMismatch {
                    ordinal: output_ordinal,
                    expected,
                    authored: binding.output,
                });
            }
            binding.output_ordinal = output_ordinal;
        }
        for emission_index in 0..emissions.len() {
            let emission = emissions[emission_index];
            if let Some(previous) = emissions[..emission_index]
                .iter()
                .find(|previous| previous.sink_output == emission.sink_output)
            {
                return Err(AttachmentCreateErrorV1::SinkOutputAliased {
                    sink_output: emission.sink_output,
                    first_output: previous.output,
                    second_output: emission.output,
                });
            }
        }

        let owned_scope = sink.sink().owned_output_scope();
        if owned_scope.len() != emissions.len() {
            return Err(AttachmentCreateErrorV1::SinkScopeCount {
                expected: emissions.len(),
                actual: owned_scope.len(),
            });
        }
        for (ordinal, owned) in owned_scope.iter().copied().enumerate() {
            if owned_scope[..ordinal].contains(&owned) {
                return Err(AttachmentCreateErrorV1::DuplicateSinkScopeOutput {
                    sink_output: owned,
                });
            }
        }

        if authored_presentations.is_empty() {
            return Err(AttachmentCreateErrorV1::EmptyPresentations);
        }
        let expected_presentations = owner.compiled.point_presentation_count();
        if authored_presentations.len() != expected_presentations {
            return Err(AttachmentCreateErrorV1::PresentationCount {
                expected: expected_presentations,
                actual: authored_presentations.len(),
            });
        }

        let mut presentations = Vec::new();
        presentations
            .try_reserve_exact(authored_presentations.len())
            .map_err(|_| AttachmentCreateErrorV1::ResourceExhausted)?;
        for (authored_index, binding) in authored_presentations.iter().copied().enumerate() {
            let compiled = owner
                .compiled
                .bind_point_output_presentation(
                    binding.output.into_core(),
                    binding.root.into_core(),
                    binding.occurrence.into_core(),
                )
                .map_err(|cause| AttachmentCreateErrorV1::InvalidPointBinding {
                    authored_index,
                    cause,
                })?;
            let emission = emissions
                .get(compiled.output_ordinal())
                .copied()
                .ok_or(AttachmentCreateErrorV1::InternalInvariant)?;
            if emission.output.into_core() != compiled.output() {
                return Err(AttachmentCreateErrorV1::InternalInvariant);
            }
            presentations.push(AttachedPointPresentationV1 {
                compiled,
                sink_output: emission.sink_output,
            });
        }
        presentations.sort_unstable_by_key(|binding| binding.compiled.presentation_ordinal());
        for adjacent in presentations.windows(2) {
            if adjacent[0].compiled.presentation_ordinal()
                == adjacent[1].compiled.presentation_ordinal()
            {
                let previous = adjacent[0];
                let relation = adjacent[1];
                return Err(AttachmentCreateErrorV1::DuplicatePresentation {
                    root: relation.root(),
                    occurrence: relation.occurrence(),
                    first_output: previous.output(),
                    second_output: relation.output(),
                });
            }
        }
        for (presentation_ordinal, presentation) in presentations.iter().enumerate() {
            if presentation.compiled.presentation_ordinal() != presentation_ordinal {
                return Err(AttachmentCreateErrorV1::MissingCompiledPresentation {
                    presentation_ordinal,
                });
            }
        }
        presentations.sort_unstable_by_key(|binding| {
            (
                binding.compiled.output_ordinal(),
                binding.compiled.presentation_ordinal(),
            )
        });
        let mut presentation_index = 0;
        for emission in &emissions {
            if presentations
                .get(presentation_index)
                .is_none_or(|presentation| {
                    presentation.compiled.output_ordinal() != emission.output_ordinal
                })
            {
                return Err(AttachmentCreateErrorV1::MissingOutputPresentation {
                    output: emission.output,
                });
            }
            while presentations
                .get(presentation_index)
                .is_some_and(|presentation| {
                    presentation.compiled.output_ordinal() == emission.output_ordinal
                })
            {
                presentation_index += 1;
            }
        }
        if presentation_index != presentations.len() {
            return Err(AttachmentCreateErrorV1::InternalInvariant);
        }
        for (ordinal, (binding, owned)) in emissions
            .iter()
            .zip(owned_scope.iter().copied())
            .enumerate()
        {
            if binding.sink_output != owned {
                return Err(AttachmentCreateErrorV1::SinkScopeMismatch {
                    ordinal,
                    binding: binding.sink_output,
                    owned,
                });
            }
        }

        let mut committed_sink_patch = Vec::new();
        committed_sink_patch
            .try_reserve_exact(expected_outputs)
            .map_err(|_| AttachmentCreateErrorV1::ResourceExhausted)?;
        let mut scratch_sink_patch = Vec::new();
        scratch_sink_patch
            .try_reserve_exact(expected_outputs)
            .map_err(|_| AttachmentCreateErrorV1::ResourceExhausted)?;
        let mut committed_render_patch = Vec::new();
        committed_render_patch
            .try_reserve_exact(presentations.len())
            .map_err(|_| AttachmentCreateErrorV1::ResourceExhausted)?;
        let mut scratch_render_patch = Vec::new();
        scratch_render_patch
            .try_reserve_exact(presentations.len())
            .map_err(|_| AttachmentCreateErrorV1::ResourceExhausted)?;

        let session = owner
            .instantiate(stream_id)
            .map_err(AttachmentCreateErrorV1::Instantiate)?;
        let owner_pin = owner.compiled.pin_owner();
        let sink = sink.accept();
        Ok(Self {
            sink,
            session,
            emissions,
            presentations,
            committed_sink_patch,
            scratch_sink_patch,
            committed_render_patch,
            scratch_render_patch,
            published_stamp: None,
            retired_stamp: None,
            retired_session: None,
            _owner_pin: owner_pin,
        })
    }

    /// Готовит, атомарно устанавливает и infallibly публикует целый update.
    pub(crate) fn update(&mut self, update: UpdateV1<'_>) -> AttachmentUpdateResultV1<'_, L> {
        drop(self.retired_session.take());
        drop(self.retired_stamp.take());
        let transition = self
            .session
            .prepare_update(update)
            .map_err(AttachmentUpdateErrorV1::Update)?;
        let disposition = prepared_disposition(&transition)
            .map_err(AttachmentUpdateErrorV1::InternalInvariant)?;

        let action = match disposition {
            PreparedDispositionV1::SetAll { revision, outputs } => {
                stage_complete_patches(
                    &self.emissions,
                    &self.presentations,
                    outputs,
                    &mut self.scratch_sink_patch,
                    &mut self.scratch_render_patch,
                )
                .map_err(AttachmentUpdateErrorV1::InternalInvariant)?;
                PreparedPatchActionV1::SetAll { revision }
            }
            PreparedDispositionV1::RevokeAll { revision } => {
                self.scratch_sink_patch.clear();
                self.scratch_render_patch.clear();
                PreparedPatchActionV1::RevokeAll { revision }
            }
            PreparedDispositionV1::ConfirmExact { revision } => {
                let published = self.published_stamp.as_ref().ok_or(
                    AttachmentUpdateErrorV1::InternalInvariant(
                        AttachmentInvariantV1::MissingPublishedStamp,
                    ),
                )?;
                if published.revision != revision {
                    return Err(AttachmentUpdateErrorV1::InternalInvariant(
                        AttachmentInvariantV1::PublishedRevisionMismatch,
                    ));
                }
                PreparedPatchActionV1::ConfirmExact { revision }
            }
        };

        let intent = match &action {
            PreparedPatchActionV1::SetAll { revision } => PointSinkIntentV1::SetAll {
                revision: *revision,
                patch: &self.scratch_sink_patch,
            },
            PreparedPatchActionV1::RevokeAll { revision } => PointSinkIntentV1::RevokeAll {
                revision: *revision,
            },
            PreparedPatchActionV1::ConfirmExact { revision } => {
                let expected = &self
                    .published_stamp
                    .as_ref()
                    .ok_or(AttachmentUpdateErrorV1::InternalInvariant(
                        AttachmentInvariantV1::MissingPublishedStamp,
                    ))?
                    .sink;
                PointSinkIntentV1::ConfirmExact {
                    revision: *revision,
                    published_stamp: expected,
                }
            }
        };
        let sink_prepared = self
            .sink
            .prepare(intent)
            .map_err(AttachmentUpdateErrorV1::SinkPrepare)?;
        let mut prepared: PreparedAttachmentUpdateV1<'_, '_, L> =
            PreparedAttachmentUpdateV1::new(transition, sink_prepared);
        let next_stamp = PublishedAttachmentStampV1 {
            revision: action.revision(),
            sink: prepared.proposed_stamp().clone(),
        };
        if matches!(&action, PreparedPatchActionV1::ConfirmExact { .. }) {
            let expected = &self
                .published_stamp
                .as_ref()
                .ok_or(AttachmentUpdateErrorV1::InternalInvariant(
                    AttachmentInvariantV1::MissingPublishedStamp,
                ))?
                .sink;
            if &next_stamp.sink != expected {
                return Err(AttachmentUpdateErrorV1::InternalInvariant(
                    AttachmentInvariantV1::ConfirmStampMismatch,
                ));
            }
        }

        prepared
            .try_install()
            .map_err(AttachmentUpdateErrorV1::SinkInstall)?;

        // Ниже только moves/swaps/writes в заранее пустые retirement-слоты.
        let (installed_sink, retired_session) = prepared.commit_session();
        self.retired_session = Some(retired_session);
        match &action {
            PreparedPatchActionV1::SetAll { .. } | PreparedPatchActionV1::RevokeAll { .. } => {
                mem::swap(&mut self.committed_sink_patch, &mut self.scratch_sink_patch);
                mem::swap(
                    &mut self.committed_render_patch,
                    &mut self.scratch_render_patch,
                );
            }
            PreparedPatchActionV1::ConfirmExact { .. } => {}
        }
        let previous_stamp = self.published_stamp.take();
        self.retired_stamp = previous_stamp;
        let published_stamp = self.published_stamp.insert(next_stamp);
        installed_sink.finish_after_session();

        Ok(AttachmentCommitV1 {
            evidence: self.session.evidence(),
            committed_render_patch: &self.committed_render_patch,
            published_stamp,
        })
    }

    /// Детерминированное consuming-освобождение; revoke выполняет `Drop`.
    pub(crate) fn dispose(self) {
        drop(self);
    }
}

impl<L> Drop for Attachment<L>
where
    L: LinearPointSinkLeaseV1,
{
    fn drop(&mut self) {
        self.sink
            .revoke_all_before_release(self.published_stamp.as_ref().map(|stamp| &stamp.sink));
        self.published_stamp = None;
        self.committed_sink_patch.clear();
        self.committed_render_patch.clear();
    }
}

fn stage_complete_patches<SinkOutputId: Copy + Eq>(
    emissions: &[AttachedPointEmissionV1<SinkOutputId>],
    presentations: &[AttachedPointPresentationV1<SinkOutputId>],
    outputs: &[ProgramPaintOutputV1],
    sink_scratch: &mut Vec<PointSinkPatchEntryV1<SinkOutputId>>,
    render_scratch: &mut Vec<AttachedRenderPatchEntryV1<SinkOutputId>>,
) -> Result<(), AttachmentInvariantV1> {
    if outputs.len() != emissions.len() {
        return Err(AttachmentInvariantV1::OutputCountMismatch);
    }
    if sink_scratch.capacity() < emissions.len() || render_scratch.capacity() < presentations.len()
    {
        return Err(AttachmentInvariantV1::ScratchCapacityLost);
    }

    sink_scratch.clear();
    render_scratch.clear();
    for (output_ordinal, (emission, output)) in emissions
        .iter()
        .copied()
        .zip(outputs.iter().copied())
        .enumerate()
    {
        if emission.output_ordinal != output_ordinal
            || emission.output.into_core() != output.output()
        {
            return Err(AttachmentInvariantV1::OutputIdentityMismatch);
        }
        sink_scratch.push(PointSinkPatchEntryV1 {
            emission,
            paint: output.paint(),
        });
    }
    for presentation in presentations.iter().copied() {
        let output = outputs
            .get(presentation.compiled.output_ordinal())
            .copied()
            .ok_or(AttachmentInvariantV1::OutputCountMismatch)?;
        if presentation.compiled.output() != output.output() {
            return Err(AttachmentInvariantV1::OutputIdentityMismatch);
        }
        if presentation.compiled.paint() != output.paint().id() {
            return Err(AttachmentInvariantV1::PaintIdentityMismatch);
        }
        render_scratch.push(AttachedRenderPatchEntryV1 {
            presentation,
            paint: output.paint(),
        });
    }
    Ok(())
}

/// Общий token: abort всегда уничтожает evidence до освобождения Busy.
struct PreparedAttachmentUpdateV1<'session, 'sink, L>
where
    L: LinearPointSinkLeaseV1 + 'sink,
{
    // Порядок объявления и есть abort-протокол: prospective evidence
    // уничтожается, пока sink ещё удерживает Busy.
    transition: CorePreparedSessionTransitionV1<'session>,
    sink: L::Prepared<'sink>,
}

impl<'session, 'sink, L> PreparedAttachmentUpdateV1<'session, 'sink, L>
where
    L: LinearPointSinkLeaseV1 + 'sink,
{
    fn new(
        transition: CorePreparedSessionTransitionV1<'session>,
        sink: L::Prepared<'sink>,
    ) -> Self {
        Self { transition, sink }
    }

    fn proposed_stamp(&self) -> &L::Stamp {
        self.sink.proposed_stamp()
    }

    fn try_install(&mut self) -> Result<(), L::Error> {
        self.sink.try_install()
    }

    fn commit_session(self) -> (L::Prepared<'sink>, CoreDeferredSessionRetirementV1) {
        let Self { transition, sink } = self;
        let (_view, retirement) = transition.commit_deferred();
        (sink, retirement)
    }
}

#[cfg(test)]
pub(crate) mod support;
#[cfg(test)]
mod tests;
