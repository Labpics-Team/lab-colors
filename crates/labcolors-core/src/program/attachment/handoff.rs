use core::{
    num::NonZeroU64,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::appearance::EncodedPointPaintV1;
use crate::program::OutputSlotIdV1;

use super::{
    BoundPointSinkScopePermitV1, ExternallyManagedAttachmentV1, PointSinkAdmissionFailureV1,
    PointSinkBindingEpochV1, PointSinkIntentV1, PointSinkStampV1, PointSinkWriterAdmissionV1,
    PointSinkWriterV1, PreparedPointSinkWriteV1, UnboundPointSinkWriterV1, sink_private,
};

static NEXT_HANDOFF_POINT_SINK_EPOCH_V1: AtomicU64 = AtomicU64::new(1);

/// Opaque identifier for the private fixture's single terminal output.
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

/// One complete, typed command for the host-owned atomic output lease.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandoffPointSinkHostIntentV1 {
    SetAll {
        revision: u64,
        expected_sequence: u64,
        desired_sequence: u64,
        output: OutputSlotIdV1,
        sink_output: HandoffPointSinkOutputIdV1,
        paint: EncodedPointPaintV1,
    },
    RevokeAll {
        revision: u64,
        expected_sequence: u64,
        desired_sequence: u64,
    },
    ConfirmExact {
        revision: u64,
        published_sequence: u64,
        point: Option<(
            OutputSlotIdV1,
            HandoffPointSinkOutputIdV1,
            EncodedPointPaintV1,
        )>,
    },
}

impl HandoffPointSinkHostIntentV1 {
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

    pub(crate) const fn point(
        self,
    ) -> Option<(
        OutputSlotIdV1,
        HandoffPointSinkOutputIdV1,
        EncodedPointPaintV1,
    )> {
        match self {
            Self::SetAll {
                output,
                sink_output,
                paint,
                ..
            } => Some((output, sink_output, paint)),
            Self::ConfirmExact { point, .. } => point,
            Self::RevokeAll { .. } => None,
        }
    }
}

/// Typed status returned by the synchronous host install boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HandoffPointSinkHostErrorV1 {
    Rejected,
    Protocol,
}

/// The only effectful boundary of the private fixture's terminal sink.
///
/// `try_install` must publish the whole command atomically or leave the host
/// lease unchanged. Lifecycle disposal is confirmed by the enclosing private
/// ABI before it drops the Attachment; Handoff Drop itself has no host effect.
pub(crate) trait HandoffPointSinkHostV1 {
    fn try_install(
        &mut self,
        intent: HandoffPointSinkHostIntentV1,
    ) -> Result<(), HandoffPointSinkHostErrorV1>;
}

/// Creates the single-slot host-backed sink used only by the private fixture.
pub(crate) fn handoff_point_sink<H>(
    output: HandoffPointSinkOutputIdV1,
    host: H,
) -> HandoffPointSinkWriterV1<H>
where
    H: HandoffPointSinkHostV1,
{
    HandoffPointSinkWriterV1::new(output, host)
}

pub(crate) type HandoffAttachmentV1<H> =
    ExternallyManagedAttachmentV1<AdmittedHandoffPointSinkWriterV1<H>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HandoffPublishedPointV1 {
    output: OutputSlotIdV1,
    sink_output: HandoffPointSinkOutputIdV1,
    paint: EncodedPointPaintV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HandoffCommittedPointStateV1 {
    stamp: PointSinkStampV1,
    revision: Option<u64>,
    point: Option<HandoffPublishedPointV1>,
}

pub(crate) struct HandoffPointSinkWriterV1<H>
where
    H: HandoffPointSinkHostV1,
{
    owned_scope: [HandoffPointSinkOutputIdV1; 1],
    host: H,
}

impl<H> HandoffPointSinkWriterV1<H>
where
    H: HandoffPointSinkHostV1,
{
    const fn new(output: HandoffPointSinkOutputIdV1, host: H) -> Self {
        Self {
            owned_scope: [output],
            host,
        }
    }

    #[cfg(test)]
    fn try_admit_exact<I>(
        self,
        mut actual_scope: I,
    ) -> Result<
        PointSinkWriterAdmissionV1<AdmittedHandoffPointSinkWriterV1<H>>,
        PointSinkAdmissionFailureV1<Self>,
    >
    where
        I: Iterator<Item = HandoffPointSinkOutputIdV1>,
    {
        if actual_scope.next() != self.owned_scope.first().copied() || actual_scope.next().is_some()
        {
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
                    point: None,
                },
                host,
            },
        ))
    }
}

pub(crate) struct AdmittedHandoffPointSinkWriterV1<H>
where
    H: HandoffPointSinkHostV1,
{
    owned_scope: [HandoffPointSinkOutputIdV1; 1],
    binding_epoch: PointSinkBindingEpochV1,
    committed: HandoffCommittedPointStateV1,
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
    StampMismatch,
    RevisionMismatch,
    AlreadyInstalled,
    Host(HandoffPointSinkHostErrorV1),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HandoffStagedPointStateV1 {
    expected_stamp: PointSinkStampV1,
    desired: HandoffCommittedPointStateV1,
    host_intent: HandoffPointSinkHostIntentV1,
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
        let mut actual_scope = scope.output_scope();
        if actual_scope.next() != self.owned_scope.first().copied() || actual_scope.next().is_some()
        {
            return Err(PointSinkAdmissionFailureV1::new(
                HandoffPointSinkAdmissionErrorV1::ScopeChanged,
                self,
            ));
        }
        self.finish_admission()
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
        let current = self.committed;
        let (desired, host_intent) = match intent {
            PointSinkIntentV1::SetAll {
                revision,
                stamp,
                patch,
            } => {
                if stamp.expected() != current.stamp {
                    return Err(HandoffPointSinkErrorV1::StampMismatch);
                }
                let [point] = patch else {
                    return Err(HandoffPointSinkErrorV1::PatchScopeMismatch);
                };
                if point.sink_output() != self.owned_scope[0] {
                    return Err(HandoffPointSinkErrorV1::PatchScopeMismatch);
                }
                let published = HandoffPublishedPointV1 {
                    output: point.output(),
                    sink_output: point.sink_output(),
                    paint: point.paint(),
                };
                (
                    HandoffCommittedPointStateV1 {
                        stamp: stamp.desired(),
                        revision: Some(revision),
                        point: Some(published),
                    },
                    HandoffPointSinkHostIntentV1::SetAll {
                        revision,
                        expected_sequence: stamp.expected().sequence(),
                        desired_sequence: stamp.desired().sequence(),
                        output: published.output,
                        sink_output: published.sink_output,
                        paint: published.paint,
                    },
                )
            }
            PointSinkIntentV1::RevokeAll { revision, stamp } => {
                if stamp.expected() != current.stamp {
                    return Err(HandoffPointSinkErrorV1::StampMismatch);
                }
                (
                    HandoffCommittedPointStateV1 {
                        stamp: stamp.desired(),
                        revision: Some(revision),
                        point: None,
                    },
                    HandoffPointSinkHostIntentV1::RevokeAll {
                        revision,
                        expected_sequence: stamp.expected().sequence(),
                        desired_sequence: stamp.desired().sequence(),
                    },
                )
            }
            PointSinkIntentV1::ConfirmExact {
                revision,
                published_stamp,
            } => {
                if published_stamp != current.stamp {
                    return Err(HandoffPointSinkErrorV1::StampMismatch);
                }
                if current.revision != Some(revision) {
                    return Err(HandoffPointSinkErrorV1::RevisionMismatch);
                }
                (
                    current,
                    HandoffPointSinkHostIntentV1::ConfirmExact {
                        revision,
                        published_sequence: published_stamp.sequence(),
                        point: current
                            .point
                            .map(|point| (point.output, point.sink_output, point.paint)),
                    },
                )
            }
        };

        Ok(HandoffPreparedPointSinkWriteV1 {
            writer: self,
            staged: Some(HandoffStagedPointStateV1 {
                expected_stamp: current.stamp,
                desired,
                host_intent,
            }),
        })
    }
}

impl<H> PreparedPointSinkWriteV1 for HandoffPreparedPointSinkWriteV1<'_, H>
where
    H: HandoffPointSinkHostV1,
{
    type Error = HandoffPointSinkErrorV1;

    fn try_install(&mut self) -> Result<(), Self::Error> {
        let Some(staged) = self.staged else {
            return Err(HandoffPointSinkErrorV1::AlreadyInstalled);
        };
        if self.writer.committed.stamp != staged.expected_stamp {
            return Err(HandoffPointSinkErrorV1::StampMismatch);
        }
        self.writer
            .host
            .try_install(staged.host_intent)
            .map_err(HandoffPointSinkErrorV1::Host)?;

        // The synchronous host publication above is the only fallible effect.
        // From here through Session commit, only Copy writes and moves occur.
        self.writer.committed = staged.desired;
        self.staged = None;
        Ok(())
    }

    fn finish_after_session(self) {}
}

// `Relaxed` load/store suffices: the epoch is used only as a unique token that
// is handed to the caller by value, so no synchronization edge with any other
// datum is required — uniqueness of the returned value is the only invariant,
// and `fetch_update` makes the increment atomic regardless of ordering.
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
        owned_scope: [HandoffPointSinkOutputIdV1; 1],
        active: bool,
        dispose_count: usize,
        installs: Vec<HandoffPointSinkHostIntentV1>,
        published: Option<HandoffPointSinkHostIntentV1>,
        next_error: Option<HandoffPointSinkHostErrorV1>,
        checkpoint_terminal_tail: bool,
    }

    impl FakeHostStateV1 {
        fn new(owned: HandoffPointSinkOutputIdV1) -> Self {
            Self {
                generation: HOST_GENERATION,
                owned_scope: [owned],
                active: true,
                dispose_count: 0,
                installs: Vec::new(),
                published: None,
                next_error: None,
                checkpoint_terminal_tail: false,
            }
        }

        fn dispose_exact(
            &mut self,
            generation: u32,
            owned_scope: [HandoffPointSinkOutputIdV1; 1],
        ) -> bool {
            if !self.active || generation != self.generation || owned_scope != self.owned_scope {
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
            intent: HandoffPointSinkHostIntentV1,
        ) -> Result<(), HandoffPointSinkHostErrorV1> {
            let mut state = self.0.borrow_mut();
            state.installs.push(intent);
            if let Some(error) = state.next_error.take() {
                return Err(error);
            }
            state.published = Some(intent);
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

    #[test]
    fn admission_is_exact_and_unbound_drop_has_no_host_effect() {
        let owned = HandoffPointSinkOutputIdV1::new(900);
        let foreign = HandoffPointSinkOutputIdV1::new(901);
        let (host, state) = fake_host(owned);
        let factory_lease = handoff_point_sink(owned, host);
        assert_eq!(factory_lease.owned_output_scope(), &[owned]);
        drop(factory_lease);
        assert!(state.borrow().installs.is_empty());

        let (host, state) = fake_host(owned);
        let failure =
            match handoff_point_sink(owned, host).try_admit_exact([owned, foreign].into_iter()) {
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
        assert_eq!(
            state_after_publish.published,
            Some(HandoffPointSinkHostIntentV1::SetAll {
                revision: 1,
                expected_sequence: 0,
                desired_sequence: 1,
                output: rendered.0,
                sink_output: rendered.1,
                paint: rendered.2,
            })
        );
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
            })
            .unwrap_or_else(|_| panic!("the same lease must accept a later set"));
        reset
            .try_install()
            .unwrap_or_else(|_| panic!("the same lease must publish a later set"));
        reset.finish_after_session();

        let installs = &state.borrow().installs;
        assert_eq!(installs.len(), 4);
        assert!(matches!(
            installs[0],
            HandoffPointSinkHostIntentV1::SetAll { .. }
        ));
        assert!(matches!(
            installs[1],
            HandoffPointSinkHostIntentV1::RevokeAll { .. }
        ));
        assert!(matches!(
            installs[2],
            HandoffPointSinkHostIntentV1::ConfirmExact { point: None, .. }
        ));
        assert!(matches!(
            installs[3],
            HandoffPointSinkHostIntentV1::SetAll { .. }
        ));
        assert_eq!(sink.owned_scope, [owned]);
        assert_eq!(sink.committed.revision, Some(19));
        assert!(sink.committed.point.is_some());
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
                })
                .unwrap_or_else(|_| panic!("the exact patch must prepare"));
        }
        assert!(state.borrow().installs.is_empty());
        let mut retry = sink
            .prepare(PointSinkIntentV1::SetAll {
                revision: 17,
                stamp: mutation,
                patch: &patch,
            })
            .unwrap_or_else(|_| panic!("the lease must remain retryable"));
        retry
            .try_install()
            .unwrap_or_else(|_| panic!("the retry must install"));
        retry.finish_after_session();
        assert_eq!(state.borrow().installs.len(), 1);
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
                handoff_point_sink(owned, host),
            )
            .unwrap_or_else(|_| panic!("the exact fixture attachment must be admitted"))
    }

    fn admitted_sink(
        owned: HandoffPointSinkOutputIdV1,
        host: FakeHostV1,
    ) -> (
        AdmittedHandoffPointSinkWriterV1<FakeHostV1>,
        PointSinkStampV1,
    ) {
        handoff_point_sink(owned, host)
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
