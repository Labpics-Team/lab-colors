use core::{
    mem,
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::appearance::EncodedPointPaintV1;
use crate::program::OutputSlotIdV1;

use super::{
    BoundPointSinkScopePermitV1, ExternallyManagedAttachmentV1, PointSinkAdmissionFailureV1,
    PointSinkBindingEpochV1, PointSinkIntentV1, PointSinkPresentationCompositeV1,
    PointSinkPresentationEvidenceV1, PointSinkStampV1, PointSinkWriterAdmissionV1,
    PointSinkWriterV1, PreparedPointSinkWriteV1, UnboundPointSinkWriterV1, sink_private,
};

static NEXT_HANDOFF_POINT_SINK_EPOCH_V1: AtomicU64 = AtomicU64::new(1);

/// Непрозрачный ID одного элемента полного принадлежащего host scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HandoffPointSinkOutputIdV1(u32);

impl HandoffPointSinkOutputIdV1 {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> u32 {
        self.0
    }
}

/// Один source Paint; композиции остаются отдельной проекцией Core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HandoffPublishedPointV1 {
    output: OutputSlotIdV1,
    sink_output: HandoffPointSinkOutputIdV1,
    paint: EncodedPointPaintV1,
}

impl HandoffPublishedPointV1 {
    pub(crate) const fn output(self) -> OutputSlotIdV1 {
        self.output
    }

    pub(crate) const fn sink_output(self) -> HandoffPointSinkOutputIdV1 {
        self.sink_output
    }

    pub(crate) const fn paint(self) -> EncodedPointPaintV1 {
        self.paint
    }
}

/// Полная синхронная команда; ссылки действуют только внутри host callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandoffPointSinkHostIntentV1<'a> {
    SetAll {
        revision: u64,
        expected_sequence: u64,
        desired_sequence: u64,
        binding_epoch: PointSinkBindingEpochV1,
        scope: &'a [HandoffPointSinkOutputIdV1],
        points: &'a [HandoffPublishedPointV1],
        composites: &'a [PointSinkPresentationCompositeV1],
    },
    RevokeAll {
        revision: u64,
        expected_sequence: u64,
        desired_sequence: u64,
        binding_epoch: PointSinkBindingEpochV1,
        scope: &'a [HandoffPointSinkOutputIdV1],
    },
    ConfirmExact {
        revision: u64,
        published_sequence: u64,
        binding_epoch: PointSinkBindingEpochV1,
        scope: &'a [HandoffPointSinkOutputIdV1],
        points: &'a [HandoffPublishedPointV1],
        composites: &'a [PointSinkPresentationCompositeV1],
    },
}

impl<'a> HandoffPointSinkHostIntentV1<'a> {
    pub(crate) const fn operation(self) -> u32 {
        match self {
            Self::SetAll { .. } => 1,
            Self::RevokeAll { .. } => 2,
            Self::ConfirmExact { .. } => 3,
        }
    }

    pub(crate) const fn revision(self) -> u64 {
        match self {
            Self::SetAll { revision, .. }
            | Self::RevokeAll { revision, .. }
            | Self::ConfirmExact { revision, .. } => revision,
        }
    }

    pub(crate) const fn expected_sequence(self) -> u64 {
        match self {
            Self::SetAll {
                expected_sequence, ..
            }
            | Self::RevokeAll {
                expected_sequence, ..
            } => expected_sequence,
            Self::ConfirmExact {
                published_sequence, ..
            } => published_sequence,
        }
    }

    pub(crate) const fn desired_sequence(self) -> u64 {
        match self {
            Self::SetAll {
                desired_sequence, ..
            }
            | Self::RevokeAll {
                desired_sequence, ..
            } => desired_sequence,
            Self::ConfirmExact {
                published_sequence, ..
            } => published_sequence,
        }
    }

    pub(crate) const fn binding_epoch(self) -> PointSinkBindingEpochV1 {
        match self {
            Self::SetAll { binding_epoch, .. }
            | Self::RevokeAll { binding_epoch, .. }
            | Self::ConfirmExact { binding_epoch, .. } => binding_epoch,
        }
    }

    pub(crate) const fn scope(self) -> &'a [HandoffPointSinkOutputIdV1] {
        match self {
            Self::SetAll { scope, .. }
            | Self::RevokeAll { scope, .. }
            | Self::ConfirmExact { scope, .. } => scope,
        }
    }

    pub(crate) const fn points(self) -> &'a [HandoffPublishedPointV1] {
        match self {
            Self::SetAll { points, .. } | Self::ConfirmExact { points, .. } => points,
            Self::RevokeAll { .. } => &[],
        }
    }

    pub(crate) const fn presentation_composites(self) -> &'a [PointSinkPresentationCompositeV1] {
        match self {
            Self::SetAll { composites, .. } | Self::ConfirmExact { composites, .. } => composites,
            Self::RevokeAll { .. } => &[],
        }
    }

    /// Узкая scalar-проекция не выбирает первый элемент полного scope.
    pub(crate) fn point(
        self,
    ) -> Option<(
        OutputSlotIdV1,
        HandoffPointSinkOutputIdV1,
        EncodedPointPaintV1,
    )> {
        let [point] = self.points() else {
            return None;
        };
        Some((point.output, point.sink_output, point.paint))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandoffPointSinkHostErrorV1 {
    Rejected,
    Protocol,
    ResourceExhausted,
}

/// Host применяет команду целиком либо сохраняет прежнее наблюдаемое состояние.
/// Отзыв при disposal подтверждает внешний владелец; Drop не имеет host-эффекта.
pub(crate) trait HandoffPointSinkHostV1 {
    fn try_install(
        &mut self,
        intent: HandoffPointSinkHostIntentV1<'_>,
    ) -> Result<(), HandoffPointSinkHostErrorV1>;
}

/// Принимает уже выделенный полный scope; допуск сверяет его с compiled bindings.
pub(crate) fn handoff_point_sink<H>(
    owned_scope: Vec<HandoffPointSinkOutputIdV1>,
    host: H,
) -> HandoffPointSinkWriterV1<H>
where
    H: HandoffPointSinkHostV1,
{
    HandoffPointSinkWriterV1 { owned_scope, host }
}

pub(crate) type HandoffAttachmentV1<H> =
    ExternallyManagedAttachmentV1<AdmittedHandoffPointSinkWriterV1<H>>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandoffCommittedPointStateV1 {
    stamp: PointSinkStampV1,
    revision: Option<u64>,
    points: Vec<HandoffPublishedPointV1>,
    composites: Vec<PointSinkPresentationCompositeV1>,
}

pub(crate) struct HandoffPointSinkWriterV1<H>
where
    H: HandoffPointSinkHostV1,
{
    owned_scope: Vec<HandoffPointSinkOutputIdV1>,
    host: H,
}

impl<H> HandoffPointSinkWriterV1<H>
where
    H: HandoffPointSinkHostV1,
{
    fn scope_matches(
        &self,
        actual_scope: impl Iterator<Item = HandoffPointSinkOutputIdV1>,
    ) -> bool {
        !self.owned_scope.is_empty()
            && self
                .owned_scope
                .iter()
                .enumerate()
                .all(|(index, owned)| !self.owned_scope[..index].contains(owned))
            && self.owned_scope.iter().copied().eq(actual_scope)
    }

    #[cfg(test)]
    fn try_admit_exact<I>(
        self,
        actual_scope: I,
    ) -> Result<
        PointSinkWriterAdmissionV1<AdmittedHandoffPointSinkWriterV1<H>>,
        PointSinkAdmissionFailureV1<Self>,
    >
    where
        I: Iterator<Item = HandoffPointSinkOutputIdV1>,
    {
        if !self.scope_matches(actual_scope) {
            return Err(PointSinkAdmissionFailureV1::new(
                HandoffPointSinkAdmissionErrorV1::ScopeChanged,
                self,
            ));
        }
        self.finish_admission()
    }

    fn finish_admission(
        self,
    ) -> Result<
        PointSinkWriterAdmissionV1<AdmittedHandoffPointSinkWriterV1<H>>,
        PointSinkAdmissionFailureV1<Self>,
    > {
        let Some(binding_epoch) = next_handoff_point_sink_epoch_v1() else {
            return Err(PointSinkAdmissionFailureV1::new(
                HandoffPointSinkAdmissionErrorV1::EpochExhausted,
                self,
            ));
        };
        let Self { owned_scope, host } = self;
        let stamp = PointSinkStampV1::new(0, binding_epoch);
        Ok(PointSinkWriterAdmissionV1::new(
            AdmittedHandoffPointSinkWriterV1 {
                owned_scope,
                binding_epoch,
                committed: HandoffCommittedPointStateV1 {
                    stamp,
                    revision: None,
                    points: Vec::new(),
                    composites: Vec::new(),
                },
                retired: None,
                host,
            },
        ))
    }
}

pub(crate) struct AdmittedHandoffPointSinkWriterV1<H>
where
    H: HandoffPointSinkHostV1,
{
    owned_scope: Vec<HandoffPointSinkOutputIdV1>,
    binding_epoch: PointSinkBindingEpochV1,
    committed: HandoffCommittedPointStateV1,
    // Предыдущее состояние не освобождается между host ACK и Session commit.
    retired: Option<HandoffCommittedPointStateV1>,
    host: H,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandoffPointSinkAdmissionErrorV1 {
    ScopeChanged,
    EpochExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandoffPointSinkErrorV1 {
    PatchScopeMismatch,
    PresentationEvidenceMismatch,
    StampMismatch,
    RevisionMismatch,
    AlreadyInstalled,
    ResourceExhausted,
    Host(HandoffPointSinkHostErrorV1),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HandoffStagedPointStateV1 {
    SetAll {
        revision: u64,
        expected_stamp: PointSinkStampV1,
        desired: HandoffCommittedPointStateV1,
    },
    RevokeAll {
        revision: u64,
        expected_stamp: PointSinkStampV1,
        desired: HandoffCommittedPointStateV1,
    },
    ConfirmExact {
        expected_stamp: PointSinkStampV1,
        revision: u64,
    },
}

impl HandoffStagedPointStateV1 {
    const fn expected_stamp(&self) -> PointSinkStampV1 {
        match self {
            Self::SetAll { expected_stamp, .. }
            | Self::RevokeAll { expected_stamp, .. }
            | Self::ConfirmExact { expected_stamp, .. } => *expected_stamp,
        }
    }

    #[cfg(any(test, kani))]
    fn desired<'a>(
        &'a self,
        current: &'a HandoffCommittedPointStateV1,
    ) -> &'a HandoffCommittedPointStateV1 {
        match self {
            Self::SetAll { desired, .. } | Self::RevokeAll { desired, .. } => desired,
            Self::ConfirmExact { .. } => current,
        }
    }

    fn host_intent<'a>(
        &'a self,
        scope: &'a [HandoffPointSinkOutputIdV1],
        current: &'a HandoffCommittedPointStateV1,
    ) -> HandoffPointSinkHostIntentV1<'a> {
        match self {
            Self::SetAll {
                revision,
                expected_stamp,
                desired,
            } => HandoffPointSinkHostIntentV1::SetAll {
                revision: *revision,
                expected_sequence: expected_stamp.sequence(),
                desired_sequence: desired.stamp.sequence(),
                binding_epoch: expected_stamp.binding_epoch(),
                scope,
                points: &desired.points,
                composites: &desired.composites,
            },
            Self::RevokeAll {
                revision,
                expected_stamp,
                desired,
            } => HandoffPointSinkHostIntentV1::RevokeAll {
                revision: *revision,
                expected_sequence: expected_stamp.sequence(),
                desired_sequence: desired.stamp.sequence(),
                binding_epoch: expected_stamp.binding_epoch(),
                scope,
            },
            Self::ConfirmExact {
                expected_stamp,
                revision,
            } => HandoffPointSinkHostIntentV1::ConfirmExact {
                revision: *revision,
                published_sequence: expected_stamp.sequence(),
                binding_epoch: expected_stamp.binding_epoch(),
                scope,
                points: &current.points,
                composites: &current.composites,
            },
        }
    }
}

pub(crate) struct HandoffPreparedPointSinkWriteV1<'lease, H>
where
    H: HandoffPointSinkHostV1,
{
    writer: &'lease mut AdmittedHandoffPointSinkWriterV1<H>,
    staged: Option<HandoffStagedPointStateV1>,
}

impl<H> sink_private::Sealed for HandoffPointSinkWriterV1<H> where H: HandoffPointSinkHostV1 {}
impl<H> sink_private::Sealed for AdmittedHandoffPointSinkWriterV1<H> where H: HandoffPointSinkHostV1 {}

impl<H> UnboundPointSinkWriterV1 for HandoffPointSinkWriterV1<H>
where
    H: HandoffPointSinkHostV1,
{
    type OutputId = HandoffPointSinkOutputIdV1;
    type Writer = AdmittedHandoffPointSinkWriterV1<H>;
    type AdmissionError = HandoffPointSinkAdmissionErrorV1;

    fn owned_output_scope(&self) -> &[Self::OutputId] {
        &self.owned_scope
    }

    fn try_admit_writer(
        self,
        scope: BoundPointSinkScopePermitV1<'_, Self::OutputId>,
    ) -> Result<PointSinkWriterAdmissionV1<Self::Writer>, PointSinkAdmissionFailureV1<Self>> {
        if !self.scope_matches(scope.output_scope()) {
            return Err(PointSinkAdmissionFailureV1::new(
                HandoffPointSinkAdmissionErrorV1::ScopeChanged,
                self,
            ));
        }
        self.finish_admission()
    }
}

impl<H> AdmittedHandoffPointSinkWriterV1<H>
where
    H: HandoffPointSinkHostV1,
{
    fn reusable_buffers(
        &mut self,
    ) -> (
        Vec<HandoffPublishedPointV1>,
        Vec<PointSinkPresentationCompositeV1>,
    ) {
        let (mut points, mut composites) = match self.retired.take() {
            Some(retired) => (retired.points, retired.composites),
            None => (Vec::new(), Vec::new()),
        };
        points.clear();
        composites.clear();
        (points, composites)
    }
}

impl<H> PointSinkWriterV1 for AdmittedHandoffPointSinkWriterV1<H>
where
    H: HandoffPointSinkHostV1,
{
    type OutputId = HandoffPointSinkOutputIdV1;
    type Error = HandoffPointSinkErrorV1;
    type Prepared<'lease>
        = HandoffPreparedPointSinkWriteV1<'lease, H>
    where
        Self: 'lease;

    fn binding_epoch(&self) -> PointSinkBindingEpochV1 {
        self.binding_epoch
    }

    fn prepare<'lease>(
        &'lease mut self,
        intent: PointSinkIntentV1<'_, Self::OutputId>,
    ) -> Result<Self::Prepared<'lease>, Self::Error> {
        let current_stamp = self.committed.stamp;
        let staged = match intent {
            PointSinkIntentV1::SetAll {
                revision,
                stamp,
                patch,
                presentation,
            } => {
                if stamp.expected() != current_stamp {
                    return Err(HandoffPointSinkErrorV1::StampMismatch);
                }
                if patch.len() != self.owned_scope.len()
                    || !patch
                        .iter()
                        .zip(&self.owned_scope)
                        .all(|(point, owned)| point.sink_output() == *owned)
                    // Компилятор упорядочивает полные bindings по output ID.
                    || patch.windows(2).any(|pair| pair[0].output() >= pair[1].output())
                {
                    return Err(HandoffPointSinkErrorV1::PatchScopeMismatch);
                }
                let (mut points, mut composites) = self.reusable_buffers();
                points
                    .try_reserve_exact(patch.len())
                    .map_err(|_| HandoffPointSinkErrorV1::ResourceExhausted)?;
                points.extend(patch.iter().map(|point| HandoffPublishedPointV1 {
                    output: point.output(),
                    sink_output: point.sink_output(),
                    paint: point.paint(),
                }));
                copy_presentation_composites(presentation, &mut composites)?;
                HandoffStagedPointStateV1::SetAll {
                    revision,
                    expected_stamp: current_stamp,
                    desired: HandoffCommittedPointStateV1 {
                        stamp: stamp.desired(),
                        revision: Some(revision),
                        points,
                        composites,
                    },
                }
            }
            PointSinkIntentV1::RevokeAll { revision, stamp } => {
                if stamp.expected() != current_stamp {
                    return Err(HandoffPointSinkErrorV1::StampMismatch);
                }
                let (points, composites) = self.reusable_buffers();
                HandoffStagedPointStateV1::RevokeAll {
                    revision,
                    expected_stamp: current_stamp,
                    desired: HandoffCommittedPointStateV1 {
                        stamp: stamp.desired(),
                        revision: Some(revision),
                        points,
                        composites,
                    },
                }
            }
            PointSinkIntentV1::ConfirmExact {
                revision,
                published_stamp,
            } => {
                if published_stamp != current_stamp {
                    return Err(HandoffPointSinkErrorV1::StampMismatch);
                }
                if self.committed.revision != Some(revision) {
                    return Err(HandoffPointSinkErrorV1::RevisionMismatch);
                }
                HandoffStagedPointStateV1::ConfirmExact {
                    expected_stamp: current_stamp,
                    revision,
                }
            }
        };
        Ok(HandoffPreparedPointSinkWriteV1 {
            writer: self,
            staged: Some(staged),
        })
    }
}

fn copy_presentation_composites(
    presentation: PointSinkPresentationEvidenceV1<'_>,
    output: &mut Vec<PointSinkPresentationCompositeV1>,
) -> Result<(), HandoffPointSinkErrorV1> {
    let count = match presentation {
        PointSinkPresentationEvidenceV1::Verified(verified) => {
            let mut count = 0usize;
            for composite in verified.presentation_composites() {
                let composite =
                    composite.ok_or(HandoffPointSinkErrorV1::PresentationEvidenceMismatch)?;
                count = count
                    .checked_add(composite.scenarios.len())
                    .ok_or(HandoffPointSinkErrorV1::ResourceExhausted)?;
            }
            count
        }
        #[cfg(any(test, kani))]
        PointSinkPresentationEvidenceV1::Fixture(composites) => composites.len(),
    };
    count
        .checked_mul(mem::size_of::<PointSinkPresentationCompositeV1>())
        .ok_or(HandoffPointSinkErrorV1::ResourceExhausted)?;
    output
        .try_reserve_exact(count)
        .map_err(|_| HandoffPointSinkErrorV1::ResourceExhausted)?;
    match presentation {
        PointSinkPresentationEvidenceV1::Verified(verified) => {
            for composite in verified.presentation_composites() {
                let composite =
                    composite.ok_or(HandoffPointSinkErrorV1::PresentationEvidenceMismatch)?;
                for scenario in composite.scenarios {
                    output.push(PointSinkPresentationCompositeV1::new(
                        composite.root,
                        scenario.value(),
                        crate::Srgb8::new(composite.rgb),
                    ));
                }
            }
        }
        #[cfg(any(test, kani))]
        PointSinkPresentationEvidenceV1::Fixture(composites) => {
            output.extend_from_slice(composites)
        }
    }
    crate::ordering::sort_by_key(output, |entry| (entry.root(), entry.scenario()));
    if output.windows(2).any(|pair| {
        pair[0].root() == pair[1].root()
            && pair[0].scenario() == pair[1].scenario()
            && pair[0].visible() != pair[1].visible()
    }) {
        return Err(HandoffPointSinkErrorV1::PresentationEvidenceMismatch);
    }
    output.dedup_by_key(|entry| (entry.root(), entry.scenario()));
    Ok(())
}

impl<H> PreparedPointSinkWriteV1 for HandoffPreparedPointSinkWriteV1<'_, H>
where
    H: HandoffPointSinkHostV1,
{
    type Error = HandoffPointSinkErrorV1;

    fn try_install(&mut self) -> Result<(), Self::Error> {
        let Some(staged) = self.staged.take() else {
            return Err(HandoffPointSinkErrorV1::AlreadyInstalled);
        };
        if self.writer.committed.stamp != staged.expected_stamp() {
            self.staged = Some(staged);
            return Err(HandoffPointSinkErrorV1::StampMismatch);
        }
        let intent = staged.host_intent(&self.writer.owned_scope, &self.writer.committed);
        if let Err(error) = self.writer.host.try_install(intent) {
            self.staged = Some(staged);
            return Err(HandoffPointSinkErrorV1::Host(error));
        }

        // После host ACK только moves: прежние Vec остаются в retirement-слоте.
        match staged {
            HandoffStagedPointStateV1::SetAll { desired, .. }
            | HandoffStagedPointStateV1::RevokeAll { desired, .. } => {
                let retired = mem::replace(&mut self.writer.committed, desired);
                self.writer.retired = Some(retired);
            }
            HandoffStagedPointStateV1::ConfirmExact { .. } => {}
        }
        Ok(())
    }

    fn finish_after_session(self) {}
}

/// Собственный snapshot callback для unit/formal проверки; не production-порт.
#[cfg(any(test, kani))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandoffRecordedHostIntentV1 {
    operation: u32,
    revision: u64,
    expected_sequence: u64,
    desired_sequence: u64,
    binding_epoch: PointSinkBindingEpochV1,
    scope: Vec<HandoffPointSinkOutputIdV1>,
    points: Vec<HandoffPublishedPointV1>,
    composites: Vec<PointSinkPresentationCompositeV1>,
}

#[cfg(any(test, kani))]
impl HandoffRecordedHostIntentV1 {
    pub(crate) fn from_intent(intent: HandoffPointSinkHostIntentV1<'_>) -> Self {
        Self {
            operation: intent.operation(),
            revision: intent.revision(),
            expected_sequence: intent.expected_sequence(),
            desired_sequence: intent.desired_sequence(),
            binding_epoch: intent.binding_epoch(),
            scope: intent.scope().to_vec(),
            points: intent.points().to_vec(),
            composites: intent.presentation_composites().to_vec(),
        }
    }

    pub(crate) const fn operation(&self) -> u32 {
        self.operation
    }
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }
    pub(crate) const fn expected_sequence(&self) -> u64 {
        self.expected_sequence
    }
    pub(crate) const fn desired_sequence(&self) -> u64 {
        self.desired_sequence
    }
    pub(crate) const fn binding_epoch(&self) -> PointSinkBindingEpochV1 {
        self.binding_epoch
    }
    pub(crate) fn point(
        &self,
    ) -> Option<(
        OutputSlotIdV1,
        HandoffPointSinkOutputIdV1,
        EncodedPointPaintV1,
    )> {
        let [point] = self.points.as_slice() else {
            return None;
        };
        Some((point.output, point.sink_output, point.paint))
    }
}

// Эпоха — уникальный токен, не synchronization edge с другими данными.
fn next_handoff_point_sink_epoch_v1() -> Option<PointSinkBindingEpochV1> {
    NEXT_HANDOFF_POINT_SINK_EPOCH_V1
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .ok()
        .and_then(NonZeroU64::new)
        .map(PointSinkBindingEpochV1::new)
}

#[cfg(test)]
mod tests {
    use core::cell::RefCell;
    use std::rc::Rc;

    use super::super::{
        AttachedPointEmissionV1, AuthoredPointEmissionBindingV1,
        AuthoredPointPresentationBindingV1, PointSinkMutationStampV1, PointSinkPatchEntryV1,
    };
    use super::*;
    use crate::Srgb8;
    use crate::appearance::{EncodedPointPaintValueV1, PaintId};
    use crate::family_artifact::FamilyArtifactBundleV2;
    use crate::program::{
        AppearanceContextV1, ConstraintIdV1, DraftV1, OccurrenceIdV1, PaintIdV1,
        PresentationRootIdV1, ScenarioV1, SourceIdV1, SurfaceIdV1, SurfaceInputPortIdV1,
        SurroundV1, TargetIdV1, UpdateV1,
    };

    const SOURCE: SourceIdV1 = SourceIdV1::new(1);
    const TARGET: TargetIdV1 = TargetIdV1::new(2);
    const PAINT: PaintIdV1 = PaintIdV1::new(3);
    const INPUT: SurfaceInputPortIdV1 = SurfaceInputPortIdV1::new(4);
    const INPUT_SURFACE: SurfaceIdV1 = SurfaceIdV1::new(5);
    const INNER_SURFACE: SurfaceIdV1 = SurfaceIdV1::new(6);
    const INNER: OccurrenceIdV1 = OccurrenceIdV1::new(7);
    const TERMINAL: OccurrenceIdV1 = OccurrenceIdV1::new(8);
    const ROOT: PresentationRootIdV1 = PresentationRootIdV1::new(9);
    const OUTPUT: OutputSlotIdV1 = OutputSlotIdV1::new(10);
    const VALUE: Srgb8 = Srgb8::new([12, 34, 56]);
    const HOST_GENERATION: u32 = 41;

    #[derive(Debug)]
    struct FakeHostStateV1 {
        generation: u32,
        owned_scope: Vec<HandoffPointSinkOutputIdV1>,
        active: bool,
        dispose_count: usize,
        installs: Vec<HandoffRecordedHostIntentV1>,
        published: Option<HandoffRecordedHostIntentV1>,
        next_error: Option<HandoffPointSinkHostErrorV1>,
        checkpoint_terminal_tail: bool,
        panic_before_install: bool,
    }

    impl FakeHostStateV1 {
        fn new(owned: HandoffPointSinkOutputIdV1) -> Self {
            Self::new_scope(vec![owned])
        }

        fn new_scope(owned_scope: Vec<HandoffPointSinkOutputIdV1>) -> Self {
            Self {
                generation: HOST_GENERATION,
                owned_scope,
                active: true,
                dispose_count: 0,
                installs: Vec::new(),
                published: None,
                next_error: None,
                checkpoint_terminal_tail: false,
                panic_before_install: false,
            }
        }

        fn dispose_exact(
            &mut self,
            generation: u32,
            owned_scope: [HandoffPointSinkOutputIdV1; 1],
        ) -> bool {
            if !self.active || generation != self.generation || self.owned_scope != owned_scope {
                return false;
            }
            self.active = false;
            self.published = None;
            self.dispose_count += 1;
            true
        }
    }

    struct FakeHostV1(Rc<RefCell<FakeHostStateV1>>);

    impl HandoffPointSinkHostV1 for FakeHostV1 {
        fn try_install(
            &mut self,
            intent: HandoffPointSinkHostIntentV1<'_>,
        ) -> Result<(), HandoffPointSinkHostErrorV1> {
            let mut state = self.0.borrow_mut();
            if core::mem::take(&mut state.panic_before_install) {
                panic!("test host panics before publication");
            }
            if !state.active || intent.scope() != state.owned_scope {
                return Err(HandoffPointSinkHostErrorV1::Protocol);
            }
            state
                .installs
                .push(HandoffRecordedHostIntentV1::from_intent(intent));
            if let Some(error) = state.next_error.take() {
                return Err(error);
            }
            state.published = Some(HandoffRecordedHostIntentV1::from_intent(intent));
            let checkpoint_terminal_tail = core::mem::take(&mut state.checkpoint_terminal_tail);
            drop(state);
            if checkpoint_terminal_tail {
                crate::test_support::reset_allocator_events();
            }
            Ok(())
        }
    }

    fn fake_host(owned: HandoffPointSinkOutputIdV1) -> (FakeHostV1, Rc<RefCell<FakeHostStateV1>>) {
        let state = Rc::new(RefCell::new(FakeHostStateV1::new(owned)));
        (FakeHostV1(Rc::clone(&state)), state)
    }

    fn fake_host_scope(
        scope: &[HandoffPointSinkOutputIdV1],
    ) -> (FakeHostV1, Rc<RefCell<FakeHostStateV1>>) {
        let state = Rc::new(RefCell::new(FakeHostStateV1::new_scope(scope.to_vec())));
        (FakeHostV1(Rc::clone(&state)), state)
    }

    #[test]
    fn admission_is_exact_and_unbound_drop_has_no_host_effect() {
        let owned = HandoffPointSinkOutputIdV1::new(900);
        let foreign = HandoffPointSinkOutputIdV1::new(901);
        let (host, state) = fake_host(owned);
        let factory_lease = handoff_point_sink(vec![owned], host);
        assert_eq!(factory_lease.owned_output_scope(), &[owned]);
        drop(factory_lease);
        assert!(state.borrow().installs.is_empty());

        let (host, state) = fake_host(owned);
        let failure = match handoff_point_sink(vec![owned], host)
            .try_admit_exact([owned, foreign].into_iter())
        {
            Ok(_) => panic!("an overbroad scope must not be admitted"),
            Err(failure) => failure,
        };
        let (cause, retry) = failure.into_parts();
        assert_eq!(cause, HandoffPointSinkAdmissionErrorV1::ScopeChanged);
        drop(retry);
        assert!(state.borrow().installs.is_empty());
    }

    #[test]
    fn failed_host_install_leaves_sink_and_session_old_and_retryable() {
        let owned = HandoffPointSinkOutputIdV1::new(900);
        let (host, state) = fake_host(owned);
        state.borrow_mut().next_error = Some(HandoffPointSinkHostErrorV1::Rejected);
        let mut attachment = attached(owned, host);
        let before_revision = attachment.committed_revision;
        let before_stamp = attachment.expected_sink_stamp;

        let surface = [VALUE];
        let scenarios = [ScenarioV1::new(1, &surface)];
        assert!(matches!(
            attachment.update(UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            }),
            Err(super::super::AttachmentUpdateErrorV1::SinkInstall(
                HandoffPointSinkErrorV1::Host(HandoffPointSinkHostErrorV1::Rejected)
            ))
        ));
        assert_eq!(attachment.committed_revision, before_revision);
        assert_eq!(attachment.expected_sink_stamp, before_stamp);
        assert_eq!(attachment.sink.committed.revision, None);
        assert_eq!(state.borrow().published, None);

        attachment
            .update(UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            })
            .unwrap_or_else(|_| panic!("the unchanged Session must permit an exact retry"));
        assert_eq!(attachment.committed_revision, Some(1));
        assert!(state.borrow().published.is_some());
        drop(attachment);
    }

    #[test]
    fn external_registry_retains_exact_cleanup_capability_after_local_core_drop() {
        let owned = HandoffPointSinkOutputIdV1::new(900);
        let (host, state) = fake_host(owned);
        let mut attachment = attached(owned, host);
        let surface = [VALUE];
        let scenarios = [ScenarioV1::new(1, &surface)];

        let rendered = {
            let commit = attachment
                .update(UpdateV1::Observed {
                    revision: 1,
                    scenarios: &scenarios,
                })
                .unwrap_or_else(|_| panic!("the host-backed update must commit"));
            let render = commit
                .render_outputs()
                .next()
                .unwrap_or_else(|| panic!("the committed output must be certified"));
            (render.output(), render.sink_output(), render.paint())
        };
        let state_after_publish = state.borrow();
        assert_eq!(state_after_publish.installs.len(), 1);
        let published = state_after_publish
            .published
            .as_ref()
            .expect("host intent is published");
        assert_eq!(published.operation, 1);
        assert_eq!(published.revision, 1);
        assert_eq!(published.expected_sequence, 0);
        assert_eq!(published.desired_sequence, 1);
        assert_ne!(published.binding_epoch.value().get(), 0);
        assert_eq!(published.scope, [owned]);
        assert_eq!(published.point(), Some(rendered));
        drop(state_after_publish);

        drop(attachment);
        assert!(state.borrow().published.is_some());
        assert!(state.borrow().active);
        assert!(
            !state
                .borrow_mut()
                .dispose_exact(HOST_GENERATION.wrapping_add(1), [owned])
        );
        assert!(!state.borrow_mut().dispose_exact(
            HOST_GENERATION,
            [HandoffPointSinkOutputIdV1::new(owned.value() + 1)],
        ));
        assert!(state.borrow_mut().dispose_exact(HOST_GENERATION, [owned]));
        let state = state.borrow();
        assert!(!state.active);
        assert_eq!(state.dispose_count, 1);
        assert_eq!(state.published, None);
    }

    #[test]
    fn set_revoke_confirm_empty_then_set_reuses_the_same_owned_lease() {
        let owned = HandoffPointSinkOutputIdV1::new(900);
        let (host, state) = fake_host(owned);
        let (mut sink, initial_stamp) = admitted_sink(owned, host);
        let paint = point_paint();
        let patch = [point_patch(owned, paint)];

        let first_mutation = PointSinkMutationStampV1::new(initial_stamp)
            .unwrap_or_else(|| panic!("the initial stamp has a successor"));
        let mut set = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 17,
                stamp: first_mutation,
                patch: &patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
            })
            .unwrap_or_else(|_| panic!("the exact initial patch must prepare"));
        set.try_install()
            .unwrap_or_else(|_| panic!("the exact initial patch must install"));
        set.finish_after_session();

        let second_mutation = PointSinkMutationStampV1::new(first_mutation.desired())
            .unwrap_or_else(|| panic!("the published stamp has a successor"));
        let mut revoke = sink
            .prepare(PointSinkIntentV1::RevokeAll {
                revision: 18,
                stamp: second_mutation,
            })
            .unwrap_or_else(|_| panic!("the empty publish must prepare"));
        revoke
            .try_install()
            .unwrap_or_else(|_| panic!("the empty publish must install"));
        revoke.finish_after_session();

        let mut confirm_empty = sink
            .prepare(PointSinkIntentV1::ConfirmExact {
                revision: 18,
                published_stamp: second_mutation.desired(),
            })
            .unwrap_or_else(|_| panic!("the empty committed patch must confirm"));
        confirm_empty
            .try_install()
            .unwrap_or_else(|_| panic!("the empty committed patch must republish"));
        confirm_empty.finish_after_session();

        let third_mutation = PointSinkMutationStampV1::new(second_mutation.desired())
            .unwrap_or_else(|| panic!("the empty patch stamp has a successor"));
        let mut reset = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 19,
                stamp: third_mutation,
                patch: &patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
            })
            .unwrap_or_else(|_| panic!("the same lease must accept a later set"));
        reset
            .try_install()
            .unwrap_or_else(|_| panic!("the same lease must publish a later set"));
        reset.finish_after_session();

        let installs = &state.borrow().installs;
        assert_eq!(installs.len(), 4);
        assert_eq!(
            installs
                .iter()
                .map(|intent| intent.operation)
                .collect::<Vec<_>>(),
            [1, 2, 3, 1]
        );
        assert!(installs[2].points.is_empty());
        assert_eq!(sink.owned_scope, [owned]);
        assert_eq!(sink.committed.revision, Some(19));
        assert!(!sink.committed.points.is_empty());
    }

    #[test]
    fn host_success_through_session_commit_has_no_allocator_events() {
        let owned = HandoffPointSinkOutputIdV1::new(900);
        let (host, state) = fake_host(owned);
        let mut attachment = attached(owned, host);
        let surface = [VALUE];
        let scenarios = [ScenarioV1::new(1, &surface)];
        state.borrow_mut().checkpoint_terminal_tail = true;

        let (result, events) = crate::test_support::measured_allocator_events(|| {
            attachment.update(UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            })
        });

        assert!(result.is_ok());
        assert_eq!(events, crate::test_support::AllocatorEvents::default());
    }

    #[test]
    fn wrong_stamp_revision_scope_and_stale_install_never_call_host() {
        let owned = HandoffPointSinkOutputIdV1::new(900);
        let foreign = HandoffPointSinkOutputIdV1::new(901);
        let (host, state) = fake_host(owned);
        let (mut sink, initial_stamp) = admitted_sink(owned, host);
        let (foreign_host, _) = fake_host(foreign);
        let (_, foreign_stamp) = admitted_sink(foreign, foreign_host);
        let paint = point_paint();
        let owned_patch = [point_patch(owned, paint)];
        let foreign_patch = [point_patch(foreign, paint)];
        let foreign_mutation = PointSinkMutationStampV1::new(foreign_stamp)
            .unwrap_or_else(|| panic!("a fresh stamp has a successor"));

        assert!(matches!(
            sink.prepare(PointSinkIntentV1::SetAll {
                revision: 17,
                stamp: foreign_mutation,
                patch: &owned_patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
            }),
            Err(HandoffPointSinkErrorV1::StampMismatch)
        ));
        let mutation = PointSinkMutationStampV1::new(initial_stamp)
            .unwrap_or_else(|| panic!("the initial stamp has a successor"));
        assert!(matches!(
            sink.prepare(PointSinkIntentV1::SetAll {
                revision: 17,
                stamp: mutation,
                patch: &foreign_patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
            }),
            Err(HandoffPointSinkErrorV1::PatchScopeMismatch)
        ));
        assert!(state.borrow().installs.is_empty());

        {
            let mut prepared = sink
                .prepare(PointSinkIntentV1::SetAll {
                    revision: 17,
                    stamp: mutation,
                    patch: &owned_patch,
                    presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
                })
                .unwrap_or_else(|_| panic!("the exact patch must prepare"));
            prepared.writer.committed.stamp = mutation.desired();
            assert_eq!(
                prepared.try_install(),
                Err(HandoffPointSinkErrorV1::StampMismatch)
            );
            assert!(state.borrow().installs.is_empty());
        }

        sink.committed.stamp = initial_stamp;
        let mut installed = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 17,
                stamp: mutation,
                patch: &owned_patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
            })
            .unwrap_or_else(|_| panic!("the exact patch must prepare"));
        installed
            .try_install()
            .unwrap_or_else(|_| panic!("the exact patch must install"));
        installed.finish_after_session();
        assert!(matches!(
            sink.prepare(PointSinkIntentV1::ConfirmExact {
                revision: 18,
                published_stamp: mutation.desired(),
            }),
            Err(HandoffPointSinkErrorV1::RevisionMismatch)
        ));
        assert_eq!(state.borrow().installs.len(), 1);
    }

    #[test]
    fn drop_before_install_does_not_publish_and_lease_can_retry() {
        let owned = HandoffPointSinkOutputIdV1::new(900);
        let (host, state) = fake_host(owned);
        let (mut sink, initial_stamp) = admitted_sink(owned, host);
        let mutation = PointSinkMutationStampV1::new(initial_stamp)
            .unwrap_or_else(|| panic!("the initial stamp has a successor"));
        let patch = [point_patch(owned, point_paint())];
        {
            let _prepared = sink
                .prepare(PointSinkIntentV1::SetAll {
                    revision: 17,
                    stamp: mutation,
                    patch: &patch,
                    presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
                })
                .unwrap_or_else(|_| panic!("the exact patch must prepare"));
        }
        assert!(state.borrow().installs.is_empty());
        let mut retry = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 17,
                stamp: mutation,
                patch: &patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
            })
            .unwrap_or_else(|_| panic!("the lease must remain retryable"));
        retry
            .try_install()
            .unwrap_or_else(|_| panic!("the retry must install"));
        retry.finish_after_session();
        assert_eq!(state.borrow().installs.len(), 1);
    }

    #[test]
    fn exhaustive_handoff_histories_keep_host_session_and_authority_in_sync() {
        // Семь событий, все истории длины три; два начальных состояния и
        // обе границы ревизии. Ошибка host затем повторяется тем же запросом.
        let mut histories = 0;
        for initial_revision in [0, u64::MAX - 3] {
            for initial_unknown in [false, true] {
                for history in 0..343_u32 {
                    let owned = HandoffPointSinkOutputIdV1::new(900);
                    let (host, probe) = fake_host(owned);
                    let mut attachment = attached(owned, host);
                    let values = [VALUE];
                    let scenarios = [ScenarioV1::new(1, &values)];
                    let input = |revision, unknown| {
                        if unknown {
                            UpdateV1::Unknown {
                                revision,
                                reason_id: 77,
                            }
                        } else {
                            UpdateV1::Observed {
                                revision,
                                scenarios: &scenarios,
                            }
                        }
                    };
                    attachment
                        .update(input(initial_revision, initial_unknown))
                        .expect("начальное наблюдаемое или неизвестное состояние истории должно зафиксироваться");
                    let mut revision = initial_revision;
                    let mut unknown = initial_unknown;
                    let mut sequence = attachment.expected_sink_stamp.sequence();
                    let epoch = attachment.expected_sink_stamp.binding_epoch();
                    let mut remaining = history;
                    for step in 0..3 {
                        let action = remaining % 7;
                        remaining /= 7;
                        let old_sink = attachment.sink.committed.clone();
                        let old_published = probe.borrow().published.clone();
                        let old_calls = probe.borrow().installs.len();
                        let old_kind = attachment.session.evidence().kind();
                        let repeated = action == 2 || action == 6;
                        let stale = action == 3;
                        let reject = action >= 4;
                        let next_revision = if repeated {
                            revision
                        } else if stale {
                            revision.saturating_sub(1)
                        } else {
                            revision + 1
                        };
                        let next_unknown = if repeated {
                            unknown
                        } else if stale {
                            !unknown
                        } else {
                            action == 1 || action == 5
                        };
                        let failure = if step % 2 == 0 {
                            HandoffPointSinkHostErrorV1::Rejected
                        } else {
                            HandoffPointSinkHostErrorV1::Protocol
                        };
                        if reject {
                            probe.borrow_mut().next_error = Some(failure);
                        }
                        let result = attachment.update(input(next_revision, next_unknown));
                        if stale || reject {
                            if stale {
                                assert!(matches!(
                                    result,
                                    Err(super::super::AttachmentUpdateErrorV1::Update(_))
                                ));
                            } else {
                                assert!(
                                    matches!(result, Err(super::super::AttachmentUpdateErrorV1::SinkInstall(
                                    HandoffPointSinkErrorV1::Host(error))) if error == failure)
                                );
                            }
                            assert_eq!(attachment.committed_revision, Some(revision));
                            assert_eq!(attachment.expected_sink_stamp, old_sink.stamp);
                            assert_eq!(attachment.sink.committed, old_sink);
                            assert_eq!(attachment.session.evidence().kind(), old_kind);
                            assert_eq!(probe.borrow().published, old_published);
                            assert_eq!(
                                probe.borrow().installs.len(),
                                old_calls + usize::from(reject)
                            );
                            if stale {
                                continue;
                            }
                            attachment
                                .update(input(next_revision, next_unknown))
                                .expect(
                                    "тот же запрос после отказа host должен успешно повториться",
                                );
                        } else {
                            assert!(result.is_ok());
                        }
                        if !repeated {
                            sequence += 1;
                        }
                        revision = next_revision;
                        unknown = next_unknown;
                        assert_eq!(attachment.expected_sink_stamp.sequence(), sequence);
                        assert_eq!(attachment.expected_sink_stamp.binding_epoch(), epoch);
                        assert_eq!(attachment.committed_revision, Some(revision));
                        assert_eq!(
                            attachment.sink.committed.stamp,
                            attachment.expected_sink_stamp
                        );
                        assert_eq!(attachment.sink.committed.revision, Some(revision));
                        assert_eq!(attachment.sink.committed.points.is_empty(), unknown);
                        let published = probe
                            .borrow()
                            .published
                            .clone()
                            .expect("успешный шаг истории должен опубликовать intent host");
                        assert_eq!(published.revision(), revision);
                        assert_eq!(published.desired_sequence(), sequence);
                        assert_eq!(published.binding_epoch(), epoch);
                        assert_eq!(
                            published.operation(),
                            if repeated {
                                3
                            } else if unknown {
                                2
                            } else {
                                1
                            }
                        );
                        assert_eq!(
                            published.point(),
                            if unknown {
                                None
                            } else {
                                Some((OUTPUT, owned, point_paint()))
                            }
                        );
                        assert_eq!(
                            probe.borrow().installs.len(),
                            old_calls + if reject { 2 } else { 1 }
                        );
                        // Повтор проходит весь Attachment и подтверждает живое evidence,
                        // не повышая исторические значения до текущего render output.
                        let confirmed = attachment
                            .update(input(revision, unknown))
                            .expect("повтор текущего запроса должен подтвердить живое evidence");
                        assert_eq!(confirmed.render_outputs().len(), usize::from(!unknown));
                        if let Some(output) = confirmed.render_outputs().next() {
                            assert_eq!(output.paint(), point_paint());
                            assert_eq!(output.sink_output(), owned);
                            assert_eq!(output.published_stamp().revision(), revision);
                            assert_eq!(output.published_stamp().sink_stamp().sequence(), sequence);
                        }
                    }
                    histories += 1;
                }
            }
        }
        assert_eq!(
            histories, 1_372,
            "all declared handoff histories must execute"
        );
    }

    #[test]
    fn host_unwind_before_publication_releases_prepared_state_for_retry() {
        let owned = HandoffPointSinkOutputIdV1::new(900);
        let (host, probe) = fake_host(owned);
        let mut attachment = attached(owned, host);
        let values = [VALUE];
        let scenarios = [ScenarioV1::new(1, &values)];
        attachment
            .update(UpdateV1::Observed {
                revision: 1,
                scenarios: &scenarios,
            })
            .expect("начальное наблюдение должно зафиксироваться до проверки panic host");
        for unknown in [true, false] {
            let next_revision = attachment
                .committed_revision
                .expect("успешное начальное обновление должно установить ревизию")
                + 1;
            let input = || {
                if unknown {
                    UpdateV1::Unknown {
                        revision: next_revision,
                        reason_id: 77,
                    }
                } else {
                    UpdateV1::Observed {
                        revision: next_revision,
                        scenarios: &scenarios,
                    }
                }
            };
            let old = attachment.sink.committed.clone();
            let published = probe.borrow().published.clone();
            for _ in 0..4 {
                probe.borrow_mut().panic_before_install = true;
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let _ = attachment.update(input());
                }));
                assert!(result.is_err());
                assert_eq!(attachment.sink.committed, old);
                assert_eq!(attachment.expected_sink_stamp, old.stamp);
                assert_eq!(attachment.committed_revision, old.revision);
                assert_eq!(probe.borrow().published, published);
            }
            let commit = attachment
                .update(input())
                .expect("после panic host тот же запрос должен зафиксироваться");
            assert_eq!(commit.render_outputs().len(), usize::from(!unknown));
            assert_eq!(attachment.committed_revision, Some(next_revision));
        }
    }

    fn owner() -> crate::program::OwnerV1 {
        let context = AppearanceContextV1::try_new(64.0, 0.2, SurroundV1::Dim)
            .unwrap_or_else(|_| panic!("the fixture context is valid"));
        let mut draft = DraftV1::new();
        draft.push_source(SOURCE, VALUE);
        draft.push_fixed_target(TARGET, SOURCE);
        draft.push_surface_input_port(INPUT);
        draft.push_solid_paint(PAINT, TARGET);
        draft.push_input_surface(INPUT_SURFACE, INPUT);
        draft.push_source_over_occurrence(INNER, PAINT, INPUT_SURFACE, context);
        draft.push_occurrence_surface(INNER_SURFACE, INNER);
        draft.push_source_over_occurrence(TERMINAL, PAINT, INNER_SURFACE, context);
        draft.push_point_presentation_root(ROOT, TERMINAL);
        draft.push_point_presentation_target(ROOT, INNER);
        draft.push_exact_visible_unary_hard(ConstraintIdV1::new(11), INNER, VALUE);
        draft.push_output(OUTPUT, PAINT);
        draft
            .compile()
            .unwrap_or_else(|_| panic!("the handoff fixture program must compile"))
    }

    fn attached(
        owned: HandoffPointSinkOutputIdV1,
        host: FakeHostV1,
    ) -> HandoffAttachmentV1<FakeHostV1> {
        let emissions = [AuthoredPointEmissionBindingV1::new(OUTPUT, owned)];
        let presentations = [AuthoredPointPresentationBindingV1::new(OUTPUT, ROOT, INNER)];
        owner()
            .attach_external(
                1,
                &emissions,
                &presentations,
                FamilyArtifactBundleV2::empty(),
                handoff_point_sink(vec![owned], host),
            )
            .unwrap_or_else(|_| panic!("the exact fixture attachment must be admitted"))
    }

    #[test]
    fn complete_scope_preserves_alpha_composites_replay_and_revoke() {
        let scope = [
            HandoffPointSinkOutputIdV1::new(900),
            HandoffPointSinkOutputIdV1::new(901),
        ];
        let (host, state) = fake_host_scope(&scope);
        let (mut sink, initial) = admitted_scope(&scope, host);
        let patch = two_point_patch(&scope);
        let composites = [
            PointSinkPresentationCompositeV1::new(10, 8, Srgb8::new([40, 50, 60])),
            PointSinkPresentationCompositeV1::new(9, 7, Srgb8::new([10, 20, 30])),
        ];
        let stamp = PointSinkMutationStampV1::new(initial)
            .expect("начальный stamp должен иметь допустимого преемника");
        let mut prepared = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 17,
                stamp,
                patch: &patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&composites),
            })
            .expect("полный patch с двумя выходами должен подготовиться");
        prepared
            .try_install()
            .expect("полный patch должен установиться атомарно");
        prepared.finish_after_session();
        let first = state
            .borrow()
            .published
            .clone()
            .expect("успешная установка должна опубликовать оба выхода");
        assert_eq!(first.scope, scope);
        assert_eq!(first.points.len(), 2);
        assert_eq!(first.points[0].paint.opacity_bits(), 0.25f64.to_bits());
        assert_eq!(first.points[1].paint.opacity_bits(), 0.75f64.to_bits());
        assert_eq!(first.points[0].paint.source(), Srgb8::new([12, 34, 56]));
        assert_eq!(first.points[1].paint.source(), Srgb8::new([22, 44, 66]));
        assert_eq!(first.composites, [composites[1], composites[0]]);
        let mut replay = sink
            .prepare(PointSinkIntentV1::ConfirmExact {
                revision: 17,
                published_stamp: stamp.desired(),
            })
            .expect("повтор полного patch должен подготовиться без новой ревизии");
        replay
            .try_install()
            .expect("точный повтор полного patch должен установиться");
        replay.finish_after_session();
        let replayed = state
            .borrow()
            .published
            .clone()
            .expect("точный повтор должен сохранить опубликованные значения и композиции");
        assert_eq!(replayed.operation, 3);
        assert_eq!(replayed.scope, first.scope);
        assert_eq!(replayed.points, first.points);
        assert_eq!(replayed.composites, first.composites);
        assert_eq!(replayed.desired_sequence, first.desired_sequence);
        let revoke_stamp = PointSinkMutationStampV1::new(stamp.desired())
            .expect("опубликованный stamp должен допускать отзыв");
        let mut revoke = sink
            .prepare(PointSinkIntentV1::RevokeAll {
                revision: 18,
                stamp: revoke_stamp,
            })
            .expect("отзыв полного patch должен подготовиться");
        revoke
            .try_install()
            .expect("отзыв должен атомарно очистить все выходы");
        revoke.finish_after_session();
        let revoked = state
            .borrow()
            .published
            .clone()
            .expect("успешный отзыв должен опубликовать пустое состояние");
        assert_eq!(revoked.operation, 2);
        assert_eq!(revoked.scope, scope);
        assert!(revoked.points.is_empty() && revoked.composites.is_empty());
        let mut replay = sink
            .prepare(PointSinkIntentV1::ConfirmExact {
                revision: 18,
                published_stamp: revoke_stamp.desired(),
            })
            .expect("подтверждение пустого состояния должно подготовиться");
        replay
            .try_install()
            .expect("подтверждение пустого состояния должно установиться");
        replay.finish_after_session();
        let confirmed = state
            .borrow()
            .published
            .clone()
            .expect("host должен подтвердить пустое состояние без смены stamp");
        assert_eq!(confirmed.operation, 3);
        assert_eq!(confirmed.scope, scope);
        assert!(confirmed.points.is_empty() && confirmed.composites.is_empty());
        assert_eq!(confirmed.desired_sequence, revoked.desired_sequence);
    }

    #[test]
    fn complete_scope_refusal_retains_every_old_value_and_retry_is_once_only() {
        let scope = [
            HandoffPointSinkOutputIdV1::new(900),
            HandoffPointSinkOutputIdV1::new(901),
        ];
        let (host, state) = fake_host_scope(&scope);
        let (mut sink, initial) = admitted_scope(&scope, host);
        let patch = two_point_patch(&scope);
        let first_stamp = PointSinkMutationStampV1::new(initial)
            .expect("начальный stamp должен иметь допустимого преемника");
        let mut first = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 1,
                stamp: first_stamp,
                patch: &patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
            })
            .expect("полный начальный patch должен подготовиться до проверки отказов");
        first
            .try_install()
            .expect("полный начальный patch должен установиться до проверки отказов");
        first.finish_after_session();
        let old = sink.committed.clone();
        let published = state.borrow().published.clone();
        let next_stamp = PointSinkMutationStampV1::new(first_stamp.desired())
            .expect("начальный опубликованный stamp должен допускать следующее обновление");
        let changed = [
            patch[0],
            point_patch_at(1, 11, scope[1], alpha_paint(4, [88, 99, 111], 0.75)),
        ];
        for error in [
            HandoffPointSinkHostErrorV1::Rejected,
            HandoffPointSinkHostErrorV1::Protocol,
            HandoffPointSinkHostErrorV1::ResourceExhausted,
        ] {
            state.borrow_mut().next_error = Some(error);
            let mut next = sink
                .prepare(PointSinkIntentV1::SetAll {
                    revision: 2,
                    stamp: next_stamp,
                    patch: &changed,
                    presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
                })
                .expect("полный новый patch должен подготовиться до проверки отказа host");
            assert_eq!(
                next.try_install(),
                Err(HandoffPointSinkErrorV1::Host(error))
            );
            assert_eq!(next.writer.committed, old);
            assert_eq!(state.borrow().published, published);
            assert!(next.staged.is_some());
        }
        let mut recovery = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 2,
                stamp: next_stamp,
                patch: &changed,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
            })
            .expect("тот же patch после отказов host должен повторно подготовиться");
        recovery
            .try_install()
            .expect("тот же patch после отказов host должен успешно установиться");
        let calls = state.borrow().installs.len();
        assert_eq!(
            recovery.try_install(),
            Err(HandoffPointSinkErrorV1::AlreadyInstalled)
        );
        assert_eq!(state.borrow().installs.len(), calls);
        recovery.finish_after_session();
        assert_eq!(sink.committed.stamp, next_stamp.desired());
        assert_eq!(sink.committed.revision, Some(2));
        assert_eq!(sink.committed.points[0].paint, old.points[0].paint);
        assert_eq!(sink.committed.points[1].paint, changed[1].paint());
    }

    #[test]
    fn complete_scope_admission_and_patch_reject_missing_aliased_foreign_entries() {
        let first = HandoffPointSinkOutputIdV1::new(900);
        let second = HandoffPointSinkOutputIdV1::new(901);
        let foreign = HandoffPointSinkOutputIdV1::new(902);
        for (owned, actual) in [
            (vec![first, second], vec![first]),
            (vec![first, second], vec![first, foreign]),
            (vec![first, first], vec![first, first]),
            (vec![], vec![]),
        ] {
            let (host, state) = fake_host_scope(&owned);
            let failure = match handoff_point_sink(owned, host).try_admit_exact(actual.into_iter())
            {
                Ok(_) => panic!("неполный, чужой или повторяющийся scope нельзя допустить"),
                Err(failure) => failure,
            };
            assert_eq!(
                failure.into_parts().0,
                HandoffPointSinkAdmissionErrorV1::ScopeChanged
            );
            assert!(state.borrow().installs.is_empty());
        }
        let scope = [first, second];
        let (host, state) = fake_host_scope(&scope);
        let (mut sink, initial) = admitted_scope(&scope, host);
        let good = two_point_patch(&scope);
        let bad = [
            vec![good[0]],
            vec![good[0], point_patch_at(1, 11, foreign, good[1].paint())],
            vec![good[0], point_patch_at(1, 10, second, good[1].paint())],
            vec![good[1], good[0]],
            vec![
                point_patch_at(0, 11, first, good[0].paint()),
                point_patch_at(1, 10, second, good[1].paint()),
            ],
        ];
        let stamp = PointSinkMutationStampV1::new(initial)
            .expect("начальный stamp должен иметь допустимого преемника");
        for patch in &bad {
            assert!(matches!(
                sink.prepare(PointSinkIntentV1::SetAll {
                    revision: 1,
                    stamp,
                    patch,
                    presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
                }),
                Err(HandoffPointSinkErrorV1::PatchScopeMismatch)
            ));
        }
        assert!(state.borrow().installs.is_empty());
        let mut legal = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 1,
                stamp,
                patch: &good,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&[]),
            })
            .expect("точный полный scope должен подготовить допустимый patch");
        legal
            .try_install()
            .expect("допустимый patch должен установиться после отказов неверным scope");
        legal.finish_after_session();
        assert_eq!(
            state
                .borrow()
                .published
                .as_ref()
                .expect("успешная установка полного scope должна опубликовать оба выхода")
                .points
                .len(),
            2
        );
    }

    #[test]
    fn complete_scope_install_retains_old_buffers_without_post_ack_allocator_events() {
        let scope = [
            HandoffPointSinkOutputIdV1::new(900),
            HandoffPointSinkOutputIdV1::new(901),
        ];
        let (host, state) = fake_host_scope(&scope);
        let (mut sink, initial) = admitted_scope(&scope, host);
        let patch = two_point_patch(&scope);
        let composites = [PointSinkPresentationCompositeV1::new(9, 7, VALUE)];
        let first_stamp = PointSinkMutationStampV1::new(initial)
            .expect("начальный stamp должен иметь допустимого преемника");
        let mut first = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 1,
                stamp: first_stamp,
                patch: &patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&composites),
            })
            .expect("начальный patch с композициями должен подготовиться");
        first
            .try_install()
            .expect("начальный patch с композициями должен установиться");
        first.finish_after_session();
        let second_stamp = PointSinkMutationStampV1::new(first_stamp.desired())
            .expect("первый опубликованный stamp должен допускать вторую установку");
        let mut next = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 2,
                stamp: second_stamp,
                patch: &patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&composites),
            })
            .expect("второй patch должен подготовиться до измерения выделений памяти");
        state.borrow_mut().checkpoint_terminal_tail = true;
        let (result, events) = crate::test_support::measured_allocator_events(|| {
            next.try_install()?;
            next.finish_after_session();
            Ok::<_, HandoffPointSinkErrorV1>(())
        });
        assert!(result.is_ok());
        assert_eq!(events, crate::test_support::AllocatorEvents::default());
        assert_eq!(
            sink.retired
                .as_ref()
                .expect("после ACK старые буферы должны оставаться в retired")
                .points
                .len(),
            2
        );
        let published = state.borrow().published.clone();
        {
            let _cancelled = sink
                .prepare(PointSinkIntentV1::SetAll {
                    revision: 3,
                    stamp: PointSinkMutationStampV1::new(second_stamp.desired())
                        .expect("второй опубликованный stamp должен допускать третье обновление"),
                    patch: &patch,
                    presentation: PointSinkPresentationEvidenceV1::Fixture(&composites),
                })
                .expect("допустимый третий patch должен подготовиться до отмены");
        }
        assert_eq!(sink.committed.stamp, second_stamp.desired());
        assert_eq!(state.borrow().published, published);
        assert!(sink.retired.is_none());
        let mut recovery = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 3,
                stamp: PointSinkMutationStampV1::new(second_stamp.desired()).expect(
                    "второй опубликованный stamp должен допускать повтор третьего обновления",
                ),
                patch: &patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&composites),
            })
            .expect("тот же patch после отмены должен повторно подготовиться");
        recovery
            .try_install()
            .expect("тот же patch после отмены должен успешно установиться");
        recovery.finish_after_session();
        assert_eq!(sink.committed.revision, Some(3));
    }

    #[test]
    fn presentation_fanout_keeps_scenarios_distinct_and_rejects_conflicting_evidence() {
        let scope = [
            HandoffPointSinkOutputIdV1::new(900),
            HandoffPointSinkOutputIdV1::new(901),
        ];
        let (host, state) = fake_host_scope(&scope);
        let (mut sink, initial) = admitted_scope(&scope, host);
        let patch = two_point_patch(&scope);
        let stamp = PointSinkMutationStampV1::new(initial)
            .expect("начальный stamp должен иметь допустимого преемника");
        let first = PointSinkPresentationCompositeV1::new(9, 7, Srgb8::new([10, 20, 30]));
        let second = PointSinkPresentationCompositeV1::new(9, 8, Srgb8::new([40, 50, 60]));
        let conflict = PointSinkPresentationCompositeV1::new(9, 7, Srgb8::new([40, 50, 60]));
        assert!(matches!(
            sink.prepare(PointSinkIntentV1::SetAll {
                revision: 1,
                stamp,
                patch: &patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&[first, conflict]),
            }),
            Err(HandoffPointSinkErrorV1::PresentationEvidenceMismatch)
        ));
        assert!(state.borrow().published.is_none());
        let mut valid = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 1,
                stamp,
                patch: &patch,
                presentation: PointSinkPresentationEvidenceV1::Fixture(&[second, first, first]),
            })
            .expect("согласованные композиции разных сценариев должны подготовиться");
        valid
            .try_install()
            .expect("согласованные композиции разных сценариев должны установиться");
        valid.finish_after_session();
        assert_eq!(
            state
                .borrow()
                .published
                .as_ref()
                .expect("успешная установка должна опубликовать обе композиции")
                .composites,
            [first, second]
        );
    }

    fn admitted_scope(
        scope: &[HandoffPointSinkOutputIdV1],
        host: FakeHostV1,
    ) -> (
        AdmittedHandoffPointSinkWriterV1<FakeHostV1>,
        PointSinkStampV1,
    ) {
        handoff_point_sink(scope.to_vec(), host)
            .try_admit_exact(scope.iter().copied())
            .unwrap_or_else(|_| panic!("полный scope должен пройти допуск"))
            .into_parts()
    }

    fn two_point_patch(
        scope: &[HandoffPointSinkOutputIdV1; 2],
    ) -> [PointSinkPatchEntryV1<HandoffPointSinkOutputIdV1>; 2] {
        [
            point_patch_at(0, 10, scope[0], alpha_paint(3, [12, 34, 56], 0.25)),
            point_patch_at(1, 11, scope[1], alpha_paint(4, [22, 44, 66], 0.75)),
        ]
    }

    fn alpha_paint(id: u32, source: [u8; 3], alpha: f64) -> EncodedPointPaintV1 {
        let opacity = crate::composition::AdmittedOpacityV1::new(alpha)
            .expect("opacity контрольного paint в диапазоне от нуля до единицы должна приниматься");
        EncodedPointPaintV1::from_value(
            PaintId::new(id),
            EncodedPointPaintValueV1::from_admitted(Srgb8::new(source), opacity),
        )
    }

    fn point_patch_at(
        ordinal: usize,
        output: u32,
        sink_output: HandoffPointSinkOutputIdV1,
        paint: EncodedPointPaintV1,
    ) -> PointSinkPatchEntryV1<HandoffPointSinkOutputIdV1> {
        PointSinkPatchEntryV1 {
            emission: AttachedPointEmissionV1 {
                output_ordinal: ordinal,
                output: OutputSlotIdV1::new(output),
                sink_output,
            },
            paint,
        }
    }

    fn admitted_sink(
        owned: HandoffPointSinkOutputIdV1,
        host: FakeHostV1,
    ) -> (
        AdmittedHandoffPointSinkWriterV1<FakeHostV1>,
        PointSinkStampV1,
    ) {
        handoff_point_sink(vec![owned], host)
            .try_admit_exact([owned].into_iter())
            .unwrap_or_else(|_| panic!("the exact scope must be admitted"))
            .into_parts()
    }

    fn point_paint() -> EncodedPointPaintV1 {
        EncodedPointPaintV1::from_value(PaintId::new(3), EncodedPointPaintValueV1::opaque(VALUE))
    }

    fn point_patch(
        sink_output: HandoffPointSinkOutputIdV1,
        paint: EncodedPointPaintV1,
    ) -> PointSinkPatchEntryV1<HandoffPointSinkOutputIdV1> {
        PointSinkPatchEntryV1 {
            emission: AttachedPointEmissionV1 {
                output_ordinal: 0,
                output: OUTPUT,
                sink_output,
            },
            paint,
        }
    }
}

#[cfg(kani)]
mod proofs;
