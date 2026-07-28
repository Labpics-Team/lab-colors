use std::{
    cell::{Cell, RefCell},
    num::NonZeroU64,
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

// Stamp должен оставаться Copy, поэтому test sink получает неповторимую эпоху
// из монотонного issuer-а, а не владеет Rc и не выводит identity из адреса.
static NEXT_TEST_SINK_EPOCH: AtomicU64 = AtomicU64::new(1);

impl PointSinkStampV1 {
    const fn rebound(self, epoch: PointSinkBindingEpochV1) -> Self {
        Self::new(self.sequence(), epoch)
    }
}

fn next_test_sink_epoch() -> Option<PointSinkBindingEpochV1> {
    NEXT_TEST_SINK_EPOCH
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .ok()
        .and_then(NonZeroU64::new)
        .map(PointSinkBindingEpochV1::new)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InMemoryPointSinkErrorV1 {
    Busy,
    BindingDrift,
    /// Test sink observed either a stale stamp or a patch whose shape/output
    /// does not match its single admitted scope; production semantics do not
    /// require those oracle diagnostics to be distinguished.
    StampMismatch,
    RejectedPrepare,
    RejectedInstall,
    RejectedInstallAfterSwap,
    ResourceExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TestHostBindingV1 {
    generation: u64,
    realm: u64,
    root: u64,
    scope: u64,
    codec: u64,
    capabilities: u64,
    tombstone: u64,
}

impl TestHostBindingV1 {
    const INITIAL: Self = Self {
        generation: 1,
        realm: 1,
        root: 2,
        scope: 3,
        codec: 4,
        capabilities: 5,
        tombstone: 6,
    };

    fn drifted(mut self, axis: TestHostBindingAxisV1) -> Self {
        let value = match axis {
            TestHostBindingAxisV1::Realm => &mut self.realm,
            TestHostBindingAxisV1::Root => &mut self.root,
            TestHostBindingAxisV1::Scope => &mut self.scope,
            TestHostBindingAxisV1::Codec => &mut self.codec,
            TestHostBindingAxisV1::Capabilities => &mut self.capabilities,
            TestHostBindingAxisV1::Tombstone => &mut self.tombstone,
        };
        *value = value.wrapping_add(1);
        self.generation = self
            .generation
            .checked_add(1)
            .unwrap_or_else(|| unreachable!("test host generation exhausted"));
        self
    }

    fn restored_facts(self) -> Self {
        Self {
            generation: self
                .generation
                .checked_add(1)
                .unwrap_or_else(|| unreachable!("test host generation exhausted")),
            ..Self::INITIAL
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TestHostBindingAxisV1 {
    Realm,
    Root,
    Scope,
    Codec,
    Capabilities,
    Tombstone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InMemoryPointSinkAdmissionErrorV1 {
    RejectedBeforeInstall,
    RejectedAfterInstall,
    ScopeChanged,
    HostStateChanged,
    EpochExhausted,
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

enum TestHostLayerV1 {
    AmbientExposed,
    Closed,
    Published(Vec<TestSnapshotEntryV1>),
}

struct TestSinkStateV1 {
    // `layer` остаётся admission-bound resource; host drift создаёт отдельный
    // foreign scope, которым старый closed lease никогда не владеет.
    layer: TestHostLayerV1,
    foreign_layer: Option<TestHostLayerV1>,
    revision: Option<u64>,
    stamp: Option<PointSinkStampV1>,
    reject_next_install: bool,
    counts: TestIntentCountsV1,
    revoke_count: usize,
    sequence: u64,
    revoke_sequence: Option<u64>,
    lease_drop_sequence: Option<u64>,
}

struct TestSinkSharedV1 {
    state: RefCell<TestSinkStateV1>,
    host_binding: Cell<TestHostBindingV1>,
    reject_next_admission: Cell<bool>,
    reject_next_admission_after_install: Cell<bool>,
    busy: Cell<bool>,
    reject_next_prepare: Cell<bool>,
    rejected_prepare_saw_busy: Cell<bool>,
    reject_next_install_after_swap: Cell<bool>,
    panic_on_retirement_drop: Cell<bool>,
    retirement_drop_count: Cell<usize>,
    measure_terminal_tail: Cell<bool>,
}

pub(crate) struct InMemoryPointSinkLeaseV1 {
    owned_scope: Vec<TestSinkOutputIdV1>,
    shared: Rc<TestSinkSharedV1>,
}

pub(crate) struct ClosedInMemoryPointSinkLeaseV1 {
    _owned_scope: Vec<TestSinkOutputIdV1>,
    shared: Rc<TestSinkSharedV1>,
    bound_host: TestHostBindingV1,
    binding_epoch: PointSinkBindingEpochV1,
    retired: Option<TestSinkRetirementV1>,
}

#[derive(Clone)]
pub(crate) struct InMemoryPointSinkProbeV1 {
    shared: Rc<TestSinkSharedV1>,
}

impl InMemoryPointSinkProbeV1 {
    pub(crate) fn snapshot(&self) -> Vec<TestSnapshotEntryV1> {
        match &self.shared.state.borrow().layer {
            TestHostLayerV1::Published(snapshot) => snapshot.clone(),
            TestHostLayerV1::AmbientExposed | TestHostLayerV1::Closed => Vec::new(),
        }
    }

    pub(crate) fn revision(&self) -> Option<u64> {
        self.shared.state.borrow().revision
    }

    pub(crate) fn stamp(&self) -> PointSinkStampV1 {
        self.shared
            .state
            .borrow()
            .stamp
            .unwrap_or_else(|| unreachable!("stamp существует только после admission"))
    }

    pub(crate) fn admitted_stamp(&self) -> Option<PointSinkStampV1> {
        self.shared.state.borrow().stamp
    }

    pub(crate) fn force_stamp_sequence(&self, sequence: u64) {
        let mut state = self.shared.state.borrow_mut();
        let stamp = state
            .stamp
            .unwrap_or_else(|| unreachable!("stamp существует только после admission"));
        state.stamp = Some(PointSinkStampV1::new(sequence, stamp.binding_epoch()));
    }

    pub(crate) fn drift_host_binding(&self, axis: TestHostBindingAxisV1) {
        self.replace_host_binding(self.shared.host_binding.get().drifted(axis));
    }

    pub(crate) fn restore_host_binding(&self) {
        self.replace_host_binding(self.shared.host_binding.get().restored_facts());
    }

    fn replace_host_binding(&self, binding: TestHostBindingV1) {
        self.shared.host_binding.set(binding);
        let epoch = next_test_sink_epoch()
            .unwrap_or_else(|| unreachable!("test sink epoch exhausted during host mutation"));
        let mut state = self.shared.state.borrow_mut();
        if let Some(stamp) = state.stamp.as_mut() {
            *stamp = stamp.rebound(epoch);
        }
        state
            .foreign_layer
            .get_or_insert(TestHostLayerV1::AmbientExposed);
    }

    pub(crate) fn ambient_fallback_is_exposed(&self) -> bool {
        matches!(
            &self.shared.state.borrow().layer,
            TestHostLayerV1::AmbientExposed
        )
    }

    pub(crate) fn is_closed(&self) -> bool {
        matches!(&self.shared.state.borrow().layer, TestHostLayerV1::Closed)
    }

    pub(crate) fn foreign_scope_is_untouched(&self) -> bool {
        matches!(
            &self.shared.state.borrow().foreign_layer,
            Some(TestHostLayerV1::AmbientExposed)
        )
    }

    pub(crate) fn lease_was_dropped(&self) -> bool {
        self.shared.state.borrow().lease_drop_sequence.is_some()
    }

    pub(crate) fn reject_next_admission(&self) {
        self.shared.reject_next_admission.set(true);
    }

    pub(crate) fn reject_next_admission_after_install(&self) {
        self.shared.reject_next_admission_after_install.set(true);
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
    let shared = Rc::new(TestSinkSharedV1 {
        state: RefCell::new(TestSinkStateV1 {
            layer: TestHostLayerV1::AmbientExposed,
            foreign_layer: None,
            revision: None,
            stamp: None,
            reject_next_install: false,
            counts: TestIntentCountsV1::default(),
            revoke_count: 0,
            sequence: 0,
            revoke_sequence: None,
            lease_drop_sequence: None,
        }),
        host_binding: Cell::new(TestHostBindingV1::INITIAL),
        reject_next_admission: Cell::new(false),
        reject_next_admission_after_install: Cell::new(false),
        busy: Cell::new(false),
        reject_next_prepare: Cell::new(false),
        rejected_prepare_saw_busy: Cell::new(false),
        reject_next_install_after_swap: Cell::new(false),
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
        },
        InMemoryPointSinkProbeV1 { shared },
    )
}

/// Однослотовый sink для whole-update allocator oracle.
///
/// Его snapshot и staging имеют фиксированный размер, поэтому после cold
/// construction тест приписывает каждый allocator event самому Core path, а
/// не test adapter-у. Полный поведенческий sink выше остаётся независимым
/// oracle транзакционной семантики.
struct AllocatorPointSinkStateV1 {
    entry: Cell<Option<TestSnapshotEntryV1>>,
    revision: Cell<Option<u64>>,
    stamp: Cell<Option<PointSinkStampV1>>,
    busy: Cell<bool>,
    reject_next_prepare: Cell<bool>,
    reject_next_install: Cell<bool>,
    reject_next_install_after_swap: Cell<bool>,
}

pub(crate) struct AllocatorPointSinkLeaseV1 {
    owned_scope: [TestSinkOutputIdV1; 1],
    shared: Rc<AllocatorPointSinkStateV1>,
}

pub(crate) struct ClosedAllocatorPointSinkLeaseV1 {
    owned_scope: [TestSinkOutputIdV1; 1],
    binding_epoch: PointSinkBindingEpochV1,
    shared: Rc<AllocatorPointSinkStateV1>,
}

#[derive(Clone)]
pub(crate) struct AllocatorPointSinkProbeV1 {
    shared: Rc<AllocatorPointSinkStateV1>,
}

impl AllocatorPointSinkProbeV1 {
    pub(crate) fn clear_stamp_for_test(&self) {
        self.shared.stamp.set(None);
    }

    pub(crate) fn reject_next_prepare(&self) {
        self.shared.reject_next_prepare.set(true);
    }

    pub(crate) fn reject_next_install(&self) {
        self.shared.reject_next_install.set(true);
    }

    pub(crate) fn reject_next_install_after_swap(&self) {
        self.shared.reject_next_install_after_swap.set(true);
    }

    pub(crate) fn revision(&self) -> Option<u64> {
        self.shared.revision.get()
    }

    pub(crate) fn entry(&self) -> Option<TestSnapshotEntryV1> {
        self.shared.entry.get()
    }

    pub(crate) fn stamp(&self) -> PointSinkStampV1 {
        self.shared
            .stamp
            .get()
            .unwrap_or_else(|| unreachable!("allocator sink is admitted before updates"))
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.shared.busy.get()
    }
}

pub(crate) fn allocator_point_sink(
    owned_output: u32,
) -> (AllocatorPointSinkLeaseV1, AllocatorPointSinkProbeV1) {
    let shared = Rc::new(AllocatorPointSinkStateV1 {
        entry: Cell::new(None),
        revision: Cell::new(None),
        stamp: Cell::new(None),
        busy: Cell::new(false),
        reject_next_prepare: Cell::new(false),
        reject_next_install: Cell::new(false),
        reject_next_install_after_swap: Cell::new(false),
    });
    (
        AllocatorPointSinkLeaseV1 {
            owned_scope: [TestSinkOutputIdV1::new(owned_output)],
            shared: Rc::clone(&shared),
        },
        AllocatorPointSinkProbeV1 { shared },
    )
}

impl sink_private::Sealed for AllocatorPointSinkLeaseV1 {}
impl sink_private::Sealed for ClosedAllocatorPointSinkLeaseV1 {}

impl UnboundPointSinkLeaseV1 for AllocatorPointSinkLeaseV1 {
    type OutputId = TestSinkOutputIdV1;
    type Closed = ClosedAllocatorPointSinkLeaseV1;
    type AdmissionError = InMemoryPointSinkAdmissionErrorV1;

    fn owned_output_scope(&self) -> &[Self::OutputId] {
        &self.owned_scope
    }

    fn try_admit_closed(
        self,
        scope: BoundPointSinkScopePermitV1<'_, Self::OutputId>,
    ) -> Result<ClosedPointSinkAdmissionV1<Self::Closed>, PointSinkAdmissionFailureV1<Self>> {
        let mut actual = scope.output_scope();
        if actual.next() != self.owned_scope.first().copied() || actual.next().is_some() {
            return Err(PointSinkAdmissionFailureV1::new(
                InMemoryPointSinkAdmissionErrorV1::ScopeChanged,
                self,
            ));
        }
        let Some(binding_epoch) = next_test_sink_epoch() else {
            return Err(PointSinkAdmissionFailureV1::new(
                InMemoryPointSinkAdmissionErrorV1::EpochExhausted,
                self,
            ));
        };
        let shared = Rc::clone(&self.shared);
        let admission = ClosedPointSinkAdmissionV1::new(ClosedAllocatorPointSinkLeaseV1 {
            owned_scope: self.owned_scope,
            binding_epoch,
            shared: self.shared,
        });
        shared.stamp.set(Some(admission.initial_stamp()));
        Ok(admission)
    }
}

#[derive(Clone, Copy)]
enum AllocatorPointSinkStagingV1 {
    SetAll {
        revision: u64,
        entry: TestSnapshotEntryV1,
    },
    RevokeAll {
        revision: u64,
    },
    ConfirmExact {
        revision: u64,
    },
}

pub(crate) struct AllocatorPreparedPointSinkWriteV1<'lease> {
    lease: &'lease mut ClosedAllocatorPointSinkLeaseV1,
    base_stamp: PointSinkStampV1,
    desired_stamp: Option<PointSinkStampV1>,
    staging: AllocatorPointSinkStagingV1,
    finished: bool,
}

impl ClosedPointSinkLeaseV1 for ClosedAllocatorPointSinkLeaseV1 {
    type OutputId = TestSinkOutputIdV1;
    type Error = InMemoryPointSinkErrorV1;
    type Prepared<'lease> = AllocatorPreparedPointSinkWriteV1<'lease>;

    fn binding_epoch(&self) -> PointSinkBindingEpochV1 {
        self.binding_epoch
    }

    fn prepare<'lease>(
        &'lease mut self,
        intent: PointSinkIntentV1<'_, Self::OutputId>,
    ) -> Result<Self::Prepared<'lease>, Self::Error> {
        if self.shared.busy.replace(true) {
            return Err(InMemoryPointSinkErrorV1::Busy);
        }
        if self.shared.reject_next_prepare.replace(false) {
            self.shared.busy.set(false);
            return Err(InMemoryPointSinkErrorV1::RejectedPrepare);
        }

        let Some(base_stamp) = self.shared.stamp.get() else {
            self.shared.busy.set(false);
            return Err(InMemoryPointSinkErrorV1::StampMismatch);
        };
        let (staging, desired_stamp) = match intent {
            PointSinkIntentV1::SetAll {
                revision,
                stamp,
                patch,
            } => {
                if stamp.expected() != base_stamp {
                    self.shared.busy.set(false);
                    return Err(InMemoryPointSinkErrorV1::StampMismatch);
                }
                let [patch] = patch else {
                    self.shared.busy.set(false);
                    return Err(InMemoryPointSinkErrorV1::StampMismatch);
                };
                if patch.sink_output() != self.owned_scope[0] {
                    self.shared.busy.set(false);
                    return Err(InMemoryPointSinkErrorV1::StampMismatch);
                }
                (
                    AllocatorPointSinkStagingV1::SetAll {
                        revision,
                        entry: TestSnapshotEntryV1 {
                            output: patch.output(),
                            sink_output: patch.sink_output(),
                            paint: patch.paint(),
                        },
                    },
                    stamp.desired(),
                )
            }
            PointSinkIntentV1::RevokeAll { revision, stamp } => {
                if stamp.expected() != base_stamp {
                    self.shared.busy.set(false);
                    return Err(InMemoryPointSinkErrorV1::StampMismatch);
                }
                (
                    AllocatorPointSinkStagingV1::RevokeAll { revision },
                    stamp.desired(),
                )
            }
            PointSinkIntentV1::ConfirmExact {
                revision,
                published_stamp,
            } => {
                if published_stamp != base_stamp || self.shared.revision.get() != Some(revision) {
                    self.shared.busy.set(false);
                    return Err(InMemoryPointSinkErrorV1::StampMismatch);
                }
                (
                    AllocatorPointSinkStagingV1::ConfirmExact { revision },
                    published_stamp,
                )
            }
        };
        Ok(AllocatorPreparedPointSinkWriteV1 {
            lease: self,
            base_stamp,
            desired_stamp: Some(desired_stamp),
            staging,
            finished: false,
        })
    }

    fn close_before_release(&mut self) {
        self.shared.entry.set(None);
        self.shared.revision.set(None);
    }
}

impl PreparedPointSinkWriteV1 for AllocatorPreparedPointSinkWriteV1<'_> {
    type Error = InMemoryPointSinkErrorV1;

    fn try_install(&mut self) -> Result<(), Self::Error> {
        if self.lease.shared.reject_next_install.replace(false) {
            return Err(InMemoryPointSinkErrorV1::RejectedInstall);
        }
        if self.lease.shared.stamp.get() != Some(self.base_stamp) {
            return Err(InMemoryPointSinkErrorV1::StampMismatch);
        }
        let Some(desired_stamp) = self.desired_stamp.take() else {
            return Err(InMemoryPointSinkErrorV1::StampMismatch);
        };
        let previous_entry = self.lease.shared.entry.get();
        let previous_revision = self.lease.shared.revision.get();
        let revision = match self.staging {
            AllocatorPointSinkStagingV1::SetAll { revision, entry } => {
                self.lease.shared.entry.set(Some(entry));
                revision
            }
            AllocatorPointSinkStagingV1::RevokeAll { revision } => {
                self.lease.shared.entry.set(None);
                revision
            }
            AllocatorPointSinkStagingV1::ConfirmExact { revision } => revision,
        };
        self.lease.shared.revision.set(Some(revision));
        self.lease.shared.stamp.set(Some(desired_stamp));
        if self
            .lease
            .shared
            .reject_next_install_after_swap
            .replace(false)
        {
            self.lease.shared.entry.set(previous_entry);
            self.lease.shared.revision.set(previous_revision);
            self.lease.shared.stamp.set(Some(self.base_stamp));
            self.desired_stamp = Some(desired_stamp);
            return Err(InMemoryPointSinkErrorV1::RejectedInstallAfterSwap);
        }
        Ok(())
    }

    fn finish_after_session(mut self) {
        self.lease.shared.busy.set(false);
        self.finished = true;
    }
}

impl Drop for AllocatorPreparedPointSinkWriteV1<'_> {
    fn drop(&mut self) {
        if !self.finished {
            self.lease.shared.busy.set(false);
        }
    }
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
impl sink_private::Sealed for ClosedInMemoryPointSinkLeaseV1 {}

impl UnboundPointSinkLeaseV1 for InMemoryPointSinkLeaseV1 {
    type OutputId = TestSinkOutputIdV1;
    type Closed = ClosedInMemoryPointSinkLeaseV1;
    type AdmissionError = InMemoryPointSinkAdmissionErrorV1;

    fn owned_output_scope(&self) -> &[Self::OutputId] {
        &self.owned_scope
    }

    fn try_admit_closed(
        self,
        scope: BoundPointSinkScopePermitV1<'_, Self::OutputId>,
    ) -> Result<ClosedPointSinkAdmissionV1<Self::Closed>, PointSinkAdmissionFailureV1<Self>> {
        let mut output_scope = scope.output_scope();
        if output_scope.len() != self.owned_scope.len()
            || output_scope.any(|output| !self.owned_scope.contains(&output))
        {
            return Err(PointSinkAdmissionFailureV1::new(
                InMemoryPointSinkAdmissionErrorV1::ScopeChanged,
                self,
            ));
        }
        if self.shared.reject_next_admission.replace(false) {
            return Err(PointSinkAdmissionFailureV1::new(
                InMemoryPointSinkAdmissionErrorV1::RejectedBeforeInstall,
                self,
            ));
        }
        {
            let state = self.shared.state.borrow();
            if !matches!(&state.layer, TestHostLayerV1::AmbientExposed)
                || state.stamp.is_some()
                || state.revision.is_some()
            {
                drop(state);
                return Err(PointSinkAdmissionFailureV1::new(
                    InMemoryPointSinkAdmissionErrorV1::HostStateChanged,
                    self,
                ));
            }
        }

        // Test adapter моделирует один atomic host install: любой отказ после
        // swap восстанавливает побитово прежний unbound state.
        self.shared.state.borrow_mut().layer = TestHostLayerV1::Closed;
        if self
            .shared
            .reject_next_admission_after_install
            .replace(false)
        {
            self.shared.state.borrow_mut().layer = TestHostLayerV1::AmbientExposed;
            return Err(PointSinkAdmissionFailureV1::new(
                InMemoryPointSinkAdmissionErrorV1::RejectedAfterInstall,
                self,
            ));
        }
        let epoch = match next_test_sink_epoch() {
            Some(epoch) => epoch,
            None => {
                self.shared.state.borrow_mut().layer = TestHostLayerV1::AmbientExposed;
                return Err(PointSinkAdmissionFailureV1::new(
                    InMemoryPointSinkAdmissionErrorV1::EpochExhausted,
                    self,
                ));
            }
        };
        let shared = Rc::clone(&self.shared);
        let closed = ClosedInMemoryPointSinkLeaseV1 {
            _owned_scope: self.owned_scope,
            bound_host: self.shared.host_binding.get(),
            binding_epoch: epoch,
            shared: self.shared,
            retired: None,
        };
        let admission = ClosedPointSinkAdmissionV1::new(closed);
        shared.state.borrow_mut().stamp = Some(admission.initial_stamp());
        Ok(admission)
    }
}

impl ClosedPointSinkLeaseV1 for ClosedInMemoryPointSinkLeaseV1 {
    type OutputId = TestSinkOutputIdV1;
    type Error = InMemoryPointSinkErrorV1;
    type Prepared<'lease> = InMemoryPreparedPointSinkWriteV1<'lease>;

    fn binding_epoch(&self) -> PointSinkBindingEpochV1 {
        self.binding_epoch
    }

    fn prepare<'lease>(
        &'lease mut self,
        intent: PointSinkIntentV1<'_, Self::OutputId>,
    ) -> Result<Self::Prepared<'lease>, Self::Error> {
        if self.shared.host_binding.get() != self.bound_host {
            return Err(InMemoryPointSinkErrorV1::BindingDrift);
        }
        // Retirement предыдущего install завершается до Busy и до любых
        // изменений нового физического снимка.
        drop(self.retired.take());
        let (base_stamp, current_revision, busy) = {
            let state = self.shared.state.borrow();
            let stamp = state
                .stamp
                .unwrap_or_else(|| unreachable!("closed lease всегда имеет stamp"));
            (stamp, state.revision, self.shared.busy.get())
        };
        if busy {
            return Err(InMemoryPointSinkErrorV1::Busy);
        }

        let (staging, desired, intent_kind) = match intent {
            PointSinkIntentV1::SetAll {
                revision,
                stamp,
                patch,
            } => {
                if stamp.expected() != base_stamp {
                    return Err(InMemoryPointSinkErrorV1::StampMismatch);
                }
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
                    TestStagingV1::SetAll {
                        revision,
                        layer: TestHostLayerV1::Published(snapshot),
                    },
                    stamp.desired(),
                    TestIntentKindV1::SetAll,
                )
            }
            PointSinkIntentV1::RevokeAll { revision, stamp } => {
                if stamp.expected() != base_stamp {
                    return Err(InMemoryPointSinkErrorV1::StampMismatch);
                }
                (
                    TestStagingV1::RevokeAll {
                        revision,
                        layer: TestHostLayerV1::Closed,
                    },
                    stamp.desired(),
                    TestIntentKindV1::RevokeAll,
                )
            }
            PointSinkIntentV1::ConfirmExact {
                revision,
                published_stamp,
            } => {
                if published_stamp != base_stamp || current_revision != Some(revision) {
                    return Err(InMemoryPointSinkErrorV1::StampMismatch);
                }
                (
                    TestStagingV1::ConfirmExact { revision },
                    published_stamp,
                    TestIntentKindV1::ConfirmExact,
                )
            }
        };

        {
            let mut state = self.shared.state.borrow_mut();
            if self.shared.busy.get() || state.stamp != Some(base_stamp) {
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
            desired: Some(desired),
            staging: Some(staging),
            retired_stamp: None,
            retirement_probe,
            finished: false,
        })
    }

    fn close_before_release(&mut self) {
        let mut state = self.shared.state.borrow_mut();
        state.layer = TestHostLayerV1::Closed;
        state.revision = None;
        state.revoke_count += 1;
        state.sequence = state.sequence.saturating_add(1);
        state.revoke_sequence = Some(state.sequence);
    }
}

impl Drop for ClosedInMemoryPointSinkLeaseV1 {
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
        layer: TestHostLayerV1,
    },
    RevokeAll {
        revision: u64,
        layer: TestHostLayerV1,
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
    lease: &'lease mut ClosedInMemoryPointSinkLeaseV1,
    base_stamp: Option<PointSinkStampV1>,
    desired: Option<PointSinkStampV1>,
    staging: Option<TestStagingV1>,
    retired_stamp: Option<PointSinkStampV1>,
    retirement_probe: Option<Rc<TestSinkSharedV1>>,
    finished: bool,
}

impl PreparedPointSinkWriteV1 for InMemoryPreparedPointSinkWriteV1<'_> {
    type Error = InMemoryPointSinkErrorV1;

    fn try_install(&mut self) -> Result<(), Self::Error> {
        let desired = match self.desired.take() {
            Some(desired) => desired,
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
        if self.base_stamp.as_ref() != state.stamp.as_ref() {
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
            TestStagingV1::SetAll { layer, .. } | TestStagingV1::RevokeAll { layer, .. } => {
                mem::swap(&mut state.layer, layer);
            }
            TestStagingV1::ConfirmExact { .. } => {}
        }
        self.retired_stamp = Some(
            state
                .stamp
                .replace(desired)
                .unwrap_or_else(|| unreachable!("closed state всегда имеет stamp")),
        );
        state.revision = Some(revision);
        if self
            .lease
            .shared
            .reject_next_install_after_swap
            .replace(false)
        {
            match staging {
                TestStagingV1::SetAll { layer, .. } | TestStagingV1::RevokeAll { layer, .. } => {
                    mem::swap(&mut state.layer, layer);
                }
                TestStagingV1::ConfirmExact { .. } => {}
            }
            let retired_stamp = self
                .retired_stamp
                .take()
                .unwrap_or_else(|| unreachable!("install уже перенёс прежний stamp"));
            self.desired = Some(
                state
                    .stamp
                    .replace(retired_stamp)
                    .unwrap_or_else(|| unreachable!("install уже записал desired stamp")),
            );
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
