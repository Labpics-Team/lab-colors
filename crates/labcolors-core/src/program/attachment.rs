//! Терминальный point-attachment с одной revision-bound Session и одним writer.
//!
//! Конструктор минтит presentation-корреляции и strong Program pin из одного
//! compiled owner. Поэтому runtime-update не принимает независимые owner,
//! Session, stamp или sink handle, которые клиент мог бы перепутать.

use core::{fmt, iter::FusedIterator, mem, num::NonZeroU64};

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

/// Непередаваемый compiler-side permit полного terminal scope.
///
/// Значение создаётся только после точной output→sink и
/// output→presentation bijection, удерживает exact owner generation и
/// поглощается единственным host admission. Content identity сама по себе не
/// является этим полномочием.
pub(crate) struct BoundPointSinkScopePermitV1<'a, SinkOutputId> {
    _owner: &'a ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
    emissions: &'a [AttachedPointEmissionV1<SinkOutputId>],
    _presentations: &'a [AttachedPointPresentationV1<SinkOutputId>],
}

impl<SinkOutputId: Copy> BoundPointSinkScopePermitV1<'_, SinkOutputId> {
    pub(crate) fn output_scope(
        &self,
    ) -> impl ExactSizeIterator<Item = SinkOutputId> + FusedIterator + '_ {
        self.emissions.iter().map(|emission| emission.sink_output())
    }
}

/// Один элемент полного сертифицированного point-снимка для sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSinkPatchEntryV1<SinkOutputId> {
    emission: AttachedPointEmissionV1<SinkOutputId>,
    paint: EncodedPointPaintV1,
}

/// Номинальная локальная для процесса эпоха одной неизменяемой host-привязки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSinkBindingEpochV1(NonZeroU64);

impl PointSinkBindingEpochV1 {
    pub(crate) const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }
}

/// Двухсловный CAS-token одной допущенной инкарнации sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSinkStampV1 {
    sequence: u64,
    binding_epoch: PointSinkBindingEpochV1,
}

impl PointSinkStampV1 {
    pub(crate) const fn new(sequence: u64, binding_epoch: PointSinkBindingEpochV1) -> Self {
        Self {
            sequence,
            binding_epoch,
        }
    }

    pub(crate) const fn sequence(self) -> u64 {
        self.sequence
    }

    pub(crate) const fn binding_epoch(self) -> PointSinkBindingEpochV1 {
        self.binding_epoch
    }

    const fn checked_successor(self) -> Option<Self> {
        match self.sequence.checked_add(1) {
            Some(sequence) => Some(Self::new(sequence, self.binding_epoch)),
            None => None,
        }
    }
}

/// Единственный конструируемый Core переход stamp для меняющего снимок intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PointSinkMutationStampV1 {
    expected: PointSinkStampV1,
    desired: PointSinkStampV1,
}

impl PointSinkMutationStampV1 {
    const fn new(expected: PointSinkStampV1) -> Option<Self> {
        match expected.checked_successor() {
            Some(desired) => Some(Self { expected, desired }),
            None => None,
        }
    }

    pub(crate) const fn expected(self) -> PointSinkStampV1 {
        self.expected
    }

    pub(crate) const fn desired(self) -> PointSinkStampV1 {
        self.desired
    }
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
pub(crate) enum PointSinkIntentV1<'a, SinkOutputId> {
    SetAll {
        revision: u64,
        stamp: PointSinkMutationStampV1,
        patch: &'a [PointSinkPatchEntryV1<SinkOutputId>],
    },
    RevokeAll {
        revision: u64,
        stamp: PointSinkMutationStampV1,
    },
    ConfirmExact {
        revision: u64,
        published_stamp: PointSinkStampV1,
    },
}

/// Подготовленная sink-local транзакция целого снимка, удерживающая Busy lease.
///
/// Все реализации подчиняются hard-law all-or-nothing: внешний наблюдатель
/// никогда не видит часть нового scope. Успех заменяет весь принадлежащий lease
/// scope одной атомарной публикацией. Любой отказ сохраняет прежние наблюдаемые
/// scope→value snapshot, revision и равный по [`Eq`] Stamp.
pub(crate) trait PreparedPointSinkWriteV1 {
    type Error;

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

/// Сырой linear lease, который ещё не создавал Lab-output в host scope.
///
/// Только успешный admission атомарно устанавливает persistent closed state и
/// превращает lease в [`ClosedPointSinkLeaseV1`]. Ошибка сохраняет прежний host
/// state и возвращает тот же lease: fallible cleanup никогда не пересекает
/// границу [`Attachment`].
pub(crate) trait UnboundPointSinkLeaseV1: sink_private::Sealed + Sized {
    type OutputId: Copy + Eq;
    type Closed: ClosedPointSinkLeaseV1<OutputId = Self::OutputId>;
    type AdmissionError;

    /// Scope зарезервирован lease, но ещё не является Lab-output authority.
    fn owned_output_scope(&self) -> &[Self::OutputId];

    /// Последняя fallible-операция создания Attachment.
    ///
    /// Permit минтится только после полной compiler-backed bijection и всех
    /// allocations. Успех обязан атомарно установить closed state, связать
    /// новый process-local epoch со всем immutable host binding и вернуть
    /// lease, чей Drop можно закрыть без ошибки. Ошибка не меняет host state.
    fn try_admit_closed(
        self,
        scope: BoundPointSinkScopePermitV1<'_, Self::OutputId>,
    ) -> Result<ClosedPointSinkAdmissionV1<Self::Closed>, PointSinkAdmissionFailureV1<Self>>;
}

/// Атомарный результат допуска: закрытый lease и первый CAS-token одной эпохи.
pub(crate) struct ClosedPointSinkAdmissionV1<L>
where
    L: ClosedPointSinkLeaseV1,
{
    sink: L,
    initial_stamp: PointSinkStampV1,
}

impl<L> ClosedPointSinkAdmissionV1<L>
where
    L: ClosedPointSinkLeaseV1,
{
    fn new(sink: L) -> Self {
        let initial_stamp = PointSinkStampV1::new(0, sink.binding_epoch());
        Self {
            sink,
            initial_stamp,
        }
    }

    const fn initial_stamp(&self) -> PointSinkStampV1 {
        self.initial_stamp
    }

    fn into_parts(self) -> (L, PointSinkStampV1) {
        (self.sink, self.initial_stamp)
    }
}

/// Owning-отказ host admission; исходный unbound lease остаётся retryable.
pub(crate) struct PointSinkAdmissionFailureV1<L>
where
    L: UnboundPointSinkLeaseV1,
{
    cause: L::AdmissionError,
    sink: L,
}

impl<L> PointSinkAdmissionFailureV1<L>
where
    L: UnboundPointSinkLeaseV1,
{
    pub(crate) const fn new(cause: L::AdmissionError, sink: L) -> Self {
        Self { cause, sink }
    }

    pub(crate) fn into_parts(self) -> (L::AdmissionError, L) {
        (self.cause, self.sink)
    }
}

/// Линейное владение admission-bound физическим point-sink scope.
///
/// Сам typestate является единственным closed-absence + infallible-release
/// capability. Его Stamp обязан включать process-local binding epoch, который
/// меняется при любом изменении realm, host root, owned scope, codec release,
/// capability set или atomic primitive. Эти host-факты не становятся Core DTO.
pub(crate) trait ClosedPointSinkLeaseV1: sink_private::Sealed {
    type OutputId: Copy + Eq;
    type Error;
    type Prepared<'lease>: PreparedPointSinkWriteV1<Error = Self::Error>
    where
        Self: 'lease;

    /// Неизменяемая эпоха полномочия tombstone, захваченная атомарным допуском.
    fn binding_epoch(&self) -> PointSinkBindingEpochV1;

    /// Готовит полный снимок, не сохраняя borrowed-данные patch.
    ///
    /// `Err` сохраняет прежние наблюдаемые snapshot, revision и Stamp и
    /// возвращает lease с уже свободным Busy. Подготовка не публикует даже
    /// допустимую часть нового снимка.
    fn prepare<'lease>(
        &'lease mut self,
        intent: PointSinkIntentV1<'_, Self::OutputId>,
    ) -> Result<Self::Prepared<'lease>, Self::Error>;

    /// Атомарно отзывает полный scope lease перед его освобождением.
    ///
    /// Реализация обязана быть infallible и allocation-free, даже если ни один
    /// снимок ещё не публиковался.
    fn close_before_release(&mut self);
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
    UnownedSinkOutput {
        output: OutputSlotIdV1,
        sink_output: SinkOutputId,
    },
    InvalidPointBinding {
        authored_index: usize,
        cause: PointOutputPresentationBindErrorV1,
    },
    InternalInvariant,
}

/// Точная причина cold attach failure; lease хранится один раз во внешнем
/// owning-контейнере и не дублируется по вариантам.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttachmentCreateCauseV1<SinkOutputId, SinkAdmissionError> {
    Contract(AttachmentCreateErrorV1<SinkOutputId>),
    SinkAdmission(SinkAdmissionError),
}

/// Cold failure сохраняет тот же unbound lease для исправления и retry.
pub(crate) struct AttachmentCreateFailureV1<L>
where
    L: UnboundPointSinkLeaseV1,
{
    cause: AttachmentCreateCauseV1<L::OutputId, L::AdmissionError>,
    sink: L,
}

impl<L> fmt::Debug for AttachmentCreateFailureV1<L>
where
    L: UnboundPointSinkLeaseV1,
    L::OutputId: fmt::Debug,
    L::AdmissionError: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AttachmentCreateFailureV1")
            .field("cause", &self.cause)
            .finish_non_exhaustive()
    }
}

impl<L> AttachmentCreateFailureV1<L>
where
    L: UnboundPointSinkLeaseV1,
{
    const fn contract(cause: AttachmentCreateErrorV1<L::OutputId>, sink: L) -> Self {
        Self {
            cause: AttachmentCreateCauseV1::Contract(cause),
            sink,
        }
    }

    const fn sink_admission(cause: L::AdmissionError, sink: L) -> Self {
        Self {
            cause: AttachmentCreateCauseV1::SinkAdmission(cause),
            sink,
        }
    }

    pub(crate) const fn cause(&self) -> &AttachmentCreateCauseV1<L::OutputId, L::AdmissionError> {
        &self.cause
    }

    pub(crate) fn into_sink(self) -> L {
        self.sink
    }

    pub(crate) fn into_parts(self) -> (AttachmentCreateCauseV1<L::OutputId, L::AdmissionError>, L) {
        (self.cause, self.sink)
    }
}

/// Закрытый отказ уже скомпилированной терминальной транзакции.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachmentInvariantV1 {
    EmptyIdempotentHead,
    MissingCommittedRevision,
    PublishedRevisionMismatch,
    OutputCountMismatch,
    OutputIdentityMismatch,
    PaintIdentityMismatch,
    ScratchCapacityLost,
    SinkStampExhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttachmentUpdateErrorV1<SinkError> {
    Update(UpdateErrorV1),
    SinkPrepare(SinkError),
    SinkInstall(SinkError),
    InternalInvariant(AttachmentInvariantV1),
}

type AttachmentUpdateResultV1<'a, L> = Result<
    AttachmentCommitV1<'a, <L as ClosedPointSinkLeaseV1>::OutputId>,
    AttachmentUpdateErrorV1<<L as ClosedPointSinkLeaseV1>::Error>,
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

#[derive(Clone, Copy)]
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

/// Компактное заимствованное представление exact stamp одного Attachment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttachedPublishedStampV1<'a> {
    revision: u64,
    sink: &'a PointSinkStampV1,
}

impl<'a> AttachedPublishedStampV1<'a> {
    pub(crate) const fn revision(self) -> u64 {
        self.revision
    }

    pub(crate) const fn sink_stamp(self) -> PointSinkStampV1 {
        *self.sink
    }
}

/// Один элемент final render-authority после commit sink и Session.
pub(crate) struct AttachedRenderOutputV1<'a, SinkOutputId> {
    certificate: VerifiedCertificateV1<'a>,
    patch: AttachedRenderPatchEntryV1<SinkOutputId>,
    published_stamp: AttachedPublishedStampV1<'a>,
}

impl<SinkOutputId: Copy> Copy for AttachedRenderOutputV1<'_, SinkOutputId> {}

impl<SinkOutputId: Copy> Clone for AttachedRenderOutputV1<'_, SinkOutputId> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, SinkOutputId: Copy> AttachedRenderOutputV1<'a, SinkOutputId> {
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

    pub(crate) const fn published_stamp(self) -> AttachedPublishedStampV1<'a> {
        self.published_stamp
    }
}

/// Точный post-commit view; historical evidence и render authority не смешаны.
pub(crate) struct AttachmentCommitV1<'a, SinkOutputId> {
    evidence: EvidenceViewV1<'a>,
    committed_render_patch: &'a [AttachedRenderPatchEntryV1<SinkOutputId>],
    committed_revision: u64,
    committed_sink_stamp: &'a PointSinkStampV1,
}

impl<SinkOutputId> Copy for AttachmentCommitV1<'_, SinkOutputId> {}

impl<SinkOutputId> Clone for AttachmentCommitV1<'_, SinkOutputId> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, SinkOutputId: Copy> AttachmentCommitV1<'a, SinkOutputId> {
    pub(crate) const fn evidence(self) -> EvidenceViewV1<'a> {
        self.evidence
    }

    pub(crate) fn render_outputs(self) -> AttachedRenderOutputsV1<'a, SinkOutputId> {
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
                revision: self.committed_revision,
                sink: self.committed_sink_stamp,
            },
            index: 0,
        }
    }
}

pub(crate) struct AttachedRenderOutputsV1<'a, SinkOutputId> {
    certificate: Option<VerifiedCertificateV1<'a>>,
    committed_render_patch: &'a [AttachedRenderPatchEntryV1<SinkOutputId>],
    published_stamp: AttachedPublishedStampV1<'a>,
    index: usize,
}

impl<'a, SinkOutputId: Copy> Iterator for AttachedRenderOutputsV1<'a, SinkOutputId> {
    type Item = AttachedRenderOutputV1<'a, SinkOutputId>;

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

impl<SinkOutputId: Copy> ExactSizeIterator for AttachedRenderOutputsV1<'_, SinkOutputId> {}
impl<SinkOutputId: Copy> FusedIterator for AttachedRenderOutputsV1<'_, SinkOutputId> {}

/// Владеет одной Session, одним exact Program pin и одним linear writer.
pub(crate) struct Attachment<L>
where
    L: ClosedPointSinkLeaseV1,
{
    // Порядок полей задаёт освобождение после `Drop::drop`: writer, Session,
    // инертные снимки и последней — точная Program generation.
    sink: L,
    session: SessionV1,
    emissions: Vec<AttachedPointEmissionV1<L::OutputId>>,
    presentations: Vec<AttachedPointPresentationV1<L::OutputId>>,
    // Published patch остаётся reusable backing: swap/clear меняют длину,
    // но сохраняют заранее зарезервированную capacity для следующей ревизии.
    committed_sink_patch: Vec<PointSinkPatchEntryV1<L::OutputId>>,
    scratch_sink_patch: Vec<PointSinkPatchEntryV1<L::OutputId>>,
    committed_render_patch: Vec<AttachedRenderPatchEntryV1<L::OutputId>>,
    scratch_render_patch: Vec<AttachedRenderPatchEntryV1<L::OutputId>>,
    expected_sink_stamp: PointSinkStampV1,
    committed_revision: Option<u64>,
    // Вытеснённые Session evidence и transaction owner после install только
    // переносятся сюда и освобождаются до следующего prepare.
    retired_session: Option<CoreDeferredSessionRetirementV1>,
    _owner_pin: ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
}

/// Все fallible Core-части cold attach, завершённые до host admission.
struct PreparedAttachmentColdV1<SinkOutputId> {
    session: SessionV1,
    emissions: Vec<AttachedPointEmissionV1<SinkOutputId>>,
    presentations: Vec<AttachedPointPresentationV1<SinkOutputId>>,
    committed_sink_patch: Vec<PointSinkPatchEntryV1<SinkOutputId>>,
    scratch_sink_patch: Vec<PointSinkPatchEntryV1<SinkOutputId>>,
    committed_render_patch: Vec<AttachedRenderPatchEntryV1<SinkOutputId>>,
    scratch_render_patch: Vec<AttachedRenderPatchEntryV1<SinkOutputId>>,
    owner_pin: ProgramOwnerLeaseV1<CoreProgramEvaluatorsV1>,
}

impl OwnerV1 {
    /// Создаёт один terminal attachment этой exact compiled generation.
    pub(crate) fn attach<L>(
        &self,
        stream_id: u32,
        authored_emissions: &[AuthoredPointEmissionBindingV1<L::OutputId>],
        authored_presentations: &[AuthoredPointPresentationBindingV1],
        sink: L,
    ) -> Result<Attachment<L::Closed>, AttachmentCreateFailureV1<L>>
    where
        L: UnboundPointSinkLeaseV1,
    {
        let prepared = match PreparedAttachmentColdV1::try_new(
            self,
            stream_id,
            authored_emissions,
            authored_presentations,
            &sink,
        ) {
            Ok(prepared) => prepared,
            Err(cause) => return Err(AttachmentCreateFailureV1::contract(cause, sink)),
        };
        let permit = BoundPointSinkScopePermitV1 {
            _owner: &prepared.owner_pin,
            emissions: &prepared.emissions,
            _presentations: &prepared.presentations,
        };
        let admission = match sink.try_admit_closed(permit) {
            Ok(admission) => admission,
            Err(failure) => {
                let (cause, sink) = failure.into_parts();
                return Err(AttachmentCreateFailureV1::sink_admission(cause, sink));
            }
        };
        // Возвращаемый `Self`, а не `Result`, типом закрывает fallible-границу.
        Ok(Attachment::from_closed_admission(prepared, admission))
    }
}

impl<L> Attachment<L>
where
    L: ClosedPointSinkLeaseV1,
{
    fn from_closed_admission(
        prepared: PreparedAttachmentColdV1<L::OutputId>,
        admission: ClosedPointSinkAdmissionV1<L>,
    ) -> Self {
        // POST_ADMISSION_TAIL_START_V1
        let (sink, initial_sink_stamp) = admission.into_parts();
        let attachment = Self {
            sink,
            session: prepared.session,
            emissions: prepared.emissions,
            presentations: prepared.presentations,
            committed_sink_patch: prepared.committed_sink_patch,
            scratch_sink_patch: prepared.scratch_sink_patch,
            committed_render_patch: prepared.committed_render_patch,
            scratch_render_patch: prepared.scratch_render_patch,
            expected_sink_stamp: initial_sink_stamp,
            committed_revision: None,
            retired_session: None,
            _owner_pin: prepared.owner_pin,
        };
        // POST_ADMISSION_TAIL_END_V1
        attachment
    }
}

impl<SinkOutputId> PreparedAttachmentColdV1<SinkOutputId>
where
    SinkOutputId: Copy + Eq,
{
    /// Связывает authored IDs и pin той же exact compiled generation до host admission.
    fn try_new<L>(
        owner: &OwnerV1,
        stream_id: u32,
        authored_emissions: &[AuthoredPointEmissionBindingV1<SinkOutputId>],
        authored_presentations: &[AuthoredPointPresentationBindingV1],
        sink: &L,
    ) -> Result<Self, AttachmentCreateErrorV1<SinkOutputId>>
    where
        L: UnboundPointSinkLeaseV1<OutputId = SinkOutputId>,
    {
        let expected_outputs = owner.compiled.output_count();
        if authored_emissions.len() != expected_outputs {
            return Err(AttachmentCreateErrorV1::EmissionBindingCount {
                expected: expected_outputs,
                actual: authored_emissions.len(),
            });
        }

        let mut emissions: Vec<AttachedPointEmissionV1<SinkOutputId>> = Vec::new();
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

        let owned_scope = sink.owned_output_scope();
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
        for binding in &emissions {
            if !owned_scope.contains(&binding.sink_output) {
                return Err(AttachmentCreateErrorV1::UnownedSinkOutput {
                    output: binding.output,
                    sink_output: binding.sink_output,
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
        Ok(Self {
            session,
            emissions,
            presentations,
            committed_sink_patch,
            scratch_sink_patch,
            committed_render_patch,
            scratch_render_patch,
            owner_pin,
        })
    }
}

impl<L> Attachment<L>
where
    L: ClosedPointSinkLeaseV1,
{
    /// Готовит, атомарно устанавливает и infallibly публикует целый update.
    pub(crate) fn update(&mut self, update: UpdateV1<'_>) -> AttachmentUpdateResultV1<'_, L> {
        drop(self.retired_session.take());
        let transition = self
            .session
            .prepare_update(update)
            .map_err(AttachmentUpdateErrorV1::Update)?;
        let disposition = prepared_disposition(&transition)
            .map_err(AttachmentUpdateErrorV1::InternalInvariant)?;

        match &disposition {
            PreparedDispositionV1::ConfirmExact { revision } => {
                let published_revision =
                    self.committed_revision
                        .ok_or(AttachmentUpdateErrorV1::InternalInvariant(
                            AttachmentInvariantV1::MissingCommittedRevision,
                        ))?;
                if published_revision != *revision {
                    return Err(AttachmentUpdateErrorV1::InternalInvariant(
                        AttachmentInvariantV1::PublishedRevisionMismatch,
                    ));
                }
            }
            PreparedDispositionV1::SetAll { .. } | PreparedDispositionV1::RevokeAll { .. } => {}
        }

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
                PreparedPatchActionV1::ConfirmExact { revision }
            }
        };

        let (intent, desired_sink_stamp) = match action {
            PreparedPatchActionV1::SetAll { revision } => {
                let stamp = PointSinkMutationStampV1::new(self.expected_sink_stamp).ok_or(
                    AttachmentUpdateErrorV1::InternalInvariant(
                        AttachmentInvariantV1::SinkStampExhausted,
                    ),
                )?;
                (
                    PointSinkIntentV1::SetAll {
                        revision,
                        stamp,
                        patch: &self.scratch_sink_patch,
                    },
                    stamp.desired(),
                )
            }
            PreparedPatchActionV1::RevokeAll { revision } => {
                let stamp = PointSinkMutationStampV1::new(self.expected_sink_stamp).ok_or(
                    AttachmentUpdateErrorV1::InternalInvariant(
                        AttachmentInvariantV1::SinkStampExhausted,
                    ),
                )?;
                (
                    PointSinkIntentV1::RevokeAll { revision, stamp },
                    stamp.desired(),
                )
            }
            PreparedPatchActionV1::ConfirmExact { revision } => (
                PointSinkIntentV1::ConfirmExact {
                    revision,
                    published_stamp: self.expected_sink_stamp,
                },
                self.expected_sink_stamp,
            ),
        };
        let sink_prepared = self
            .sink
            .prepare(intent)
            .map_err(AttachmentUpdateErrorV1::SinkPrepare)?;
        let mut prepared: PreparedAttachmentUpdateV1<'_, '_, L> =
            PreparedAttachmentUpdateV1::new(transition, sink_prepared);

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
        self.expected_sink_stamp = desired_sink_stamp;
        self.committed_revision = Some(action.revision());
        installed_sink.finish_after_session();

        Ok(AttachmentCommitV1 {
            evidence: self.session.evidence(),
            committed_render_patch: &self.committed_render_patch,
            committed_revision: action.revision(),
            committed_sink_stamp: &self.expected_sink_stamp,
        })
    }

    /// Детерминированное consuming-освобождение; revoke выполняет `Drop`.
    pub(crate) fn dispose(self) {
        drop(self);
    }
}

impl<L> Drop for Attachment<L>
where
    L: ClosedPointSinkLeaseV1,
{
    fn drop(&mut self) {
        self.sink.close_before_release();
        self.committed_revision = None;
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
    L: ClosedPointSinkLeaseV1 + 'sink,
{
    // Порядок объявления и есть abort-протокол: prospective evidence
    // уничтожается, пока sink ещё удерживает Busy.
    transition: CorePreparedSessionTransitionV1<'session>,
    sink: L::Prepared<'sink>,
}

impl<'session, 'sink, L> PreparedAttachmentUpdateV1<'session, 'sink, L>
where
    L: ClosedPointSinkLeaseV1 + 'sink,
{
    fn new(
        transition: CorePreparedSessionTransitionV1<'session>,
        sink: L::Prepared<'sink>,
    ) -> Self {
        Self { transition, sink }
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
