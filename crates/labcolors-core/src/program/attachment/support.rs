use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TestSinkOutputIdV1(u32);

impl TestSinkOutputIdV1 {
    pub(crate) const fn new(value: u32) -> Self {
        Self(value)
    }

    pub(crate) const fn value(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TestPublishedStampV1 {
    sequence: u64,
    epoch: u64,
}

// Stamp должен оставаться Copy, поэтому test sink получает неповторимую эпоху
// из монотонного issuer-а, а не владеет Rc и не выводит identity из адреса.
static NEXT_TEST_SINK_EPOCH: AtomicU64 = AtomicU64::new(1);

impl TestPublishedStampV1 {
    fn next(&self) -> Result<Self, InMemoryPointSinkErrorV1> {
        Ok(Self {
            sequence: self
                .sequence
                .checked_add(1)
                .ok_or(InMemoryPointSinkErrorV1::StampExhausted)?,
            epoch: self.epoch,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InMemoryPointSinkErrorV1 {
    Busy,
    StampMismatch,
    RejectedPrepare,
    RejectedInstall,
    RejectedInstallAfterSwap,
    StampExhausted,
    ResourceExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TestSnapshotEntryV1 {
    output: OutputSlotIdV1,
    sink_output: TestSinkOutputIdV1,
    paint: EncodedPointPaintV1,
}

impl TestSnapshotEntryV1 {
    pub(crate) const fn output(self) -> OutputSlotIdV1 {
        self.output
    }

    pub(crate) const fn sink_output(self) -> TestSinkOutputIdV1 {
        self.sink_output
    }

    pub(crate) const fn paint(self) -> EncodedPointPaintV1 {
        self.paint
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TestIntentCountsV1 {
    pub(crate) set_all: usize,
    pub(crate) revoke_all: usize,
    pub(crate) confirm_exact: usize,
}

struct TestSinkStateV1 {
    snapshot: Vec<TestSnapshotEntryV1>,
    revision: Option<u64>,
    stamp: TestPublishedStampV1,
    reject_next_install: bool,
    counts: TestIntentCountsV1,
    revoke_count: usize,
    sequence: u64,
    revoke_sequence: Option<u64>,
    lease_drop_sequence: Option<u64>,
    revoke_saw_exact_stamp: bool,
}

struct TestSinkSharedV1 {
    state: RefCell<TestSinkStateV1>,
    busy: Cell<bool>,
    reject_next_prepare: Cell<bool>,
    rejected_prepare_saw_busy: Cell<bool>,
    reject_next_install_after_swap: Cell<bool>,
    misreport_next_confirm_proposed_stamp: Cell<bool>,
    panic_on_retirement_drop: Cell<bool>,
    retirement_drop_count: Cell<usize>,
    measure_terminal_tail: Cell<bool>,
}

pub(crate) struct InMemoryPointSinkLeaseV1 {
    owned_scope: Vec<TestSinkOutputIdV1>,
    shared: Rc<TestSinkSharedV1>,
    retired: Option<TestSinkRetirementV1>,
}

#[derive(Clone)]
pub(crate) struct InMemoryPointSinkProbeV1 {
    shared: Rc<TestSinkSharedV1>,
}

impl InMemoryPointSinkProbeV1 {
    pub(crate) fn snapshot(&self) -> Vec<TestSnapshotEntryV1> {
        self.shared.state.borrow().snapshot.clone()
    }

    pub(crate) fn revision(&self) -> Option<u64> {
        self.shared.state.borrow().revision
    }

    pub(crate) fn stamp(&self) -> TestPublishedStampV1 {
        self.shared.state.borrow().stamp
    }

    pub(crate) fn intent_counts(&self) -> TestIntentCountsV1 {
        self.shared.state.borrow().counts
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.shared.busy.get()
    }

    pub(crate) fn reject_next_install(&self) {
        self.shared.state.borrow_mut().reject_next_install = true;
    }

    pub(crate) fn reject_next_prepare(&self) {
        self.shared.reject_next_prepare.set(true);
    }

    pub(crate) fn reject_next_install_after_swap(&self) {
        self.shared.reject_next_install_after_swap.set(true);
    }

    pub(crate) fn rejected_prepare_saw_busy(&self) -> bool {
        self.shared.rejected_prepare_saw_busy.get()
    }

    pub(crate) fn misreport_next_confirm_proposed_stamp(&self) {
        self.shared.misreport_next_confirm_proposed_stamp.set(true);
    }

    pub(crate) fn revoke_count(&self) -> usize {
        self.shared.state.borrow().revoke_count
    }

    pub(crate) fn revoked_before_lease_drop(&self) -> bool {
        let state = self.shared.state.borrow();
        matches!(
            (state.revoke_sequence, state.lease_drop_sequence),
            (Some(revoke), Some(release)) if revoke < release
        )
    }

    pub(crate) fn revoke_saw_exact_stamp(&self) -> bool {
        self.shared.state.borrow().revoke_saw_exact_stamp
    }

    pub(crate) fn panic_on_next_retirement_drop(&self) {
        self.shared.panic_on_retirement_drop.set(true);
    }

    pub(crate) fn retirement_drop_count(&self) -> usize {
        self.shared.retirement_drop_count.get()
    }

    pub(crate) fn checkpoint_next_terminal_tail(&self) {
        assert!(
            !self.shared.measure_terminal_tail.replace(true),
            "terminal-tail measurements cannot overlap"
        );
    }
}

pub(crate) fn in_memory_point_sink(
    owned_scope: &[u32],
) -> (InMemoryPointSinkLeaseV1, InMemoryPointSinkProbeV1) {
    let epoch = NEXT_TEST_SINK_EPOCH
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .unwrap_or_else(|_| panic!("test sink epoch space exhausted"));
    let shared = Rc::new(TestSinkSharedV1 {
        state: RefCell::new(TestSinkStateV1 {
            snapshot: Vec::new(),
            revision: None,
            stamp: TestPublishedStampV1 { sequence: 0, epoch },
            reject_next_install: false,
            counts: TestIntentCountsV1::default(),
            revoke_count: 0,
            sequence: 0,
            revoke_sequence: None,
            lease_drop_sequence: None,
            revoke_saw_exact_stamp: false,
        }),
        busy: Cell::new(false),
        reject_next_prepare: Cell::new(false),
        rejected_prepare_saw_busy: Cell::new(false),
        reject_next_install_after_swap: Cell::new(false),
        misreport_next_confirm_proposed_stamp: Cell::new(false),
        panic_on_retirement_drop: Cell::new(false),
        retirement_drop_count: Cell::new(0),
        measure_terminal_tail: Cell::new(false),
    });
    (
        InMemoryPointSinkLeaseV1 {
            owned_scope: owned_scope
                .iter()
                .copied()
                .map(TestSinkOutputIdV1::new)
                .collect(),
            shared: Rc::clone(&shared),
            retired: None,
        },
        InMemoryPointSinkProbeV1 { shared },
    )
}

pub(crate) const fn authored_emission(
    output: u32,
    sink_output: u32,
) -> AuthoredPointEmissionBindingV1<TestSinkOutputIdV1> {
    AuthoredPointEmissionBindingV1::new(
        OutputSlotIdV1::new(output),
        TestSinkOutputIdV1::new(sink_output),
    )
}

pub(crate) const fn authored_presentation(
    output: u32,
    root: u32,
    occurrence: u32,
) -> AuthoredPointPresentationBindingV1 {
    AuthoredPointPresentationBindingV1::new(
        OutputSlotIdV1::new(output),
        PresentationRootIdV1::new(root),
        OccurrenceIdV1::new(occurrence),
    )
}

impl sink_private::Sealed for InMemoryPointSinkLeaseV1 {}

impl LinearPointSinkLeaseV1 for InMemoryPointSinkLeaseV1 {
    type OutputId = TestSinkOutputIdV1;
    type Stamp = TestPublishedStampV1;
    type Error = InMemoryPointSinkErrorV1;
    type Prepared<'lease> = InMemoryPreparedPointSinkWriteV1<'lease>;

    fn owned_output_scope(&self) -> &[Self::OutputId] {
        &self.owned_scope
    }

    fn prepare<'lease>(
        &'lease mut self,
        intent: PointSinkIntentV1<'_, Self::OutputId, Self::Stamp>,
    ) -> Result<Self::Prepared<'lease>, Self::Error> {
        // Retirement предыдущего install завершается до Busy и до любых
        // изменений нового физического снимка.
        drop(self.retired.take());
        let (base_stamp, current_revision, busy) = {
            let state = self.shared.state.borrow();
            (state.stamp, state.revision, self.shared.busy.get())
        };
        if busy {
            return Err(InMemoryPointSinkErrorV1::Busy);
        }

        let (staging, proposed, intent_kind) = match intent {
            PointSinkIntentV1::SetAll { revision, patch } => {
                let mut snapshot = Vec::new();
                snapshot
                    .try_reserve_exact(patch.len())
                    .map_err(|_| InMemoryPointSinkErrorV1::ResourceExhausted)?;
                snapshot.extend(patch.iter().copied().map(|entry| TestSnapshotEntryV1 {
                    output: entry.output(),
                    sink_output: entry.sink_output(),
                    paint: entry.paint(),
                }));
                (
                    TestStagingV1::SetAll { revision, snapshot },
                    base_stamp.next()?,
                    TestIntentKindV1::SetAll,
                )
            }
            PointSinkIntentV1::RevokeAll { revision } => (
                TestStagingV1::RevokeAll {
                    revision,
                    retired: Vec::new(),
                },
                base_stamp.next()?,
                TestIntentKindV1::RevokeAll,
            ),
            PointSinkIntentV1::ConfirmExact {
                revision,
                published_stamp,
            } => {
                if published_stamp != &base_stamp || current_revision != Some(revision) {
                    return Err(InMemoryPointSinkErrorV1::StampMismatch);
                }
                let proposed = if self
                    .shared
                    .misreport_next_confirm_proposed_stamp
                    .replace(false)
                {
                    published_stamp.next()?
                } else {
                    *published_stamp
                };
                (
                    TestStagingV1::ConfirmExact { revision },
                    proposed,
                    TestIntentKindV1::ConfirmExact,
                )
            }
        };

        {
            let mut state = self.shared.state.borrow_mut();
            if self.shared.busy.get() || state.stamp != base_stamp {
                return Err(InMemoryPointSinkErrorV1::Busy);
            }
            self.shared.busy.set(true);
            // Счётчики фиксируют каждый intent, получивший linear Busy;
            // test-only fault injection после этого тоже считается попыткой.
            match intent_kind {
                TestIntentKindV1::SetAll => state.counts.set_all += 1,
                TestIntentKindV1::RevokeAll => state.counts.revoke_all += 1,
                TestIntentKindV1::ConfirmExact => state.counts.confirm_exact += 1,
            }
        }
        if self.shared.reject_next_prepare.replace(false) {
            self.shared
                .rejected_prepare_saw_busy
                .set(self.shared.busy.get());
            self.shared.busy.set(false);
            return Err(InMemoryPointSinkErrorV1::RejectedPrepare);
        }

        let retirement_probe = Some(Rc::clone(&self.shared));
        Ok(InMemoryPreparedPointSinkWriteV1 {
            lease: self,
            base_stamp: Some(base_stamp),
            proposed: Some(proposed),
            staging: Some(staging),
            retired_stamp: None,
            retirement_probe,
            finished: false,
        })
    }

    fn revoke_all_before_release(&mut self, published_stamp: Option<&Self::Stamp>) {
        let mut state = self.shared.state.borrow_mut();
        state.revoke_saw_exact_stamp = published_stamp.is_none_or(|stamp| stamp == &state.stamp);
        state.snapshot.clear();
        state.revision = None;
        state.revoke_count += 1;
        state.sequence = state.sequence.saturating_add(1);
        state.revoke_sequence = Some(state.sequence);
    }
}

impl Drop for InMemoryPointSinkLeaseV1 {
    fn drop(&mut self) {
        let mut state = self.shared.state.borrow_mut();
        state.sequence = state.sequence.saturating_add(1);
        state.lease_drop_sequence = Some(state.sequence);
    }
}

#[derive(Clone, Copy)]
enum TestIntentKindV1 {
    SetAll,
    RevokeAll,
    ConfirmExact,
}

enum TestStagingV1 {
    SetAll {
        revision: u64,
        snapshot: Vec<TestSnapshotEntryV1>,
    },
    RevokeAll {
        revision: u64,
        retired: Vec<TestSnapshotEntryV1>,
    },
    ConfirmExact {
        revision: u64,
    },
}

struct TestSinkRetirementV1 {
    _staging: Option<TestStagingV1>,
    probe: Option<Rc<TestSinkSharedV1>>,
}

impl Drop for TestSinkRetirementV1 {
    fn drop(&mut self) {
        if let Some(probe) = &self.probe {
            probe
                .retirement_drop_count
                .set(probe.retirement_drop_count.get() + 1);
            if probe.panic_on_retirement_drop.replace(false) {
                panic!("sink retirement crossed the physical install boundary");
            }
        }
    }
}

pub(crate) struct InMemoryPreparedPointSinkWriteV1<'lease> {
    lease: &'lease mut InMemoryPointSinkLeaseV1,
    base_stamp: Option<TestPublishedStampV1>,
    proposed: Option<TestPublishedStampV1>,
    staging: Option<TestStagingV1>,
    retired_stamp: Option<TestPublishedStampV1>,
    retirement_probe: Option<Rc<TestSinkSharedV1>>,
    finished: bool,
}

impl PreparedPointSinkWriteV1 for InMemoryPreparedPointSinkWriteV1<'_> {
    type Stamp = TestPublishedStampV1;
    type Error = InMemoryPointSinkErrorV1;

    fn proposed_stamp(&self) -> Self::Stamp {
        self.proposed
            .unwrap_or_else(|| unreachable!("proposed stamp читается до install"))
    }

    fn try_install(&mut self) -> Result<(), Self::Error> {
        let proposed = match self.proposed.take() {
            Some(proposed) => proposed,
            None => return Err(InMemoryPointSinkErrorV1::StampMismatch),
        };
        if self.retired_stamp.is_some() {
            return Err(InMemoryPointSinkErrorV1::StampMismatch);
        }
        let staging = match self.staging.as_mut() {
            Some(staging) => staging,
            None => return Err(InMemoryPointSinkErrorV1::StampMismatch),
        };
        let mut state = self.lease.shared.state.borrow_mut();
        if state.reject_next_install {
            state.reject_next_install = false;
            return Err(InMemoryPointSinkErrorV1::RejectedInstall);
        }
        if self.base_stamp.as_ref() != Some(&state.stamp) {
            return Err(InMemoryPointSinkErrorV1::StampMismatch);
        }

        let prior_revision = state.revision;
        let revision = match staging {
            TestStagingV1::SetAll { revision, .. } | TestStagingV1::RevokeAll { revision, .. } => {
                *revision
            }
            TestStagingV1::ConfirmExact { revision } => {
                if state.revision != Some(*revision) {
                    return Err(InMemoryPointSinkErrorV1::StampMismatch);
                }
                *revision
            }
        };
        match staging {
            TestStagingV1::SetAll { snapshot, .. } => {
                mem::swap(&mut state.snapshot, snapshot);
            }
            TestStagingV1::RevokeAll { retired, .. } => {
                mem::swap(&mut state.snapshot, retired);
            }
            TestStagingV1::ConfirmExact { .. } => {}
        }
        self.retired_stamp = Some(mem::replace(&mut state.stamp, proposed));
        state.revision = Some(revision);
        if self
            .lease
            .shared
            .reject_next_install_after_swap
            .replace(false)
        {
            match staging {
                TestStagingV1::SetAll { snapshot, .. } => {
                    mem::swap(&mut state.snapshot, snapshot);
                }
                TestStagingV1::RevokeAll { retired, .. } => {
                    mem::swap(&mut state.snapshot, retired);
                }
                TestStagingV1::ConfirmExact { .. } => {}
            }
            let retired_stamp = self
                .retired_stamp
                .take()
                .unwrap_or_else(|| unreachable!("install уже перенёс прежний stamp"));
            self.proposed = Some(mem::replace(&mut state.stamp, retired_stamp));
            state.revision = prior_revision;
            return Err(InMemoryPointSinkErrorV1::RejectedInstallAfterSwap);
        }
        drop(state);
        if self.lease.shared.measure_terminal_tail.replace(false) {
            crate::test_support::reset_allocator_events();
        }
        Ok(())
    }

    fn finish_after_session(mut self) {
        let retirement = TestSinkRetirementV1 {
            _staging: self.staging.take(),
            probe: self.retirement_probe.take(),
        };
        self.lease.retired = Some(retirement);
        self.lease.shared.busy.set(false);
        self.finished = true;
    }
}

impl Drop for InMemoryPreparedPointSinkWriteV1<'_> {
    fn drop(&mut self) {
        if !self.finished {
            drop(self.staging.take());
            drop(self.retirement_probe.take());
            self.lease.shared.busy.set(false);
        }
    }
}
