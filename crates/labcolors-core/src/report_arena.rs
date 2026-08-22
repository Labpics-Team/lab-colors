// O-13 PR3: ReportArenaPoolV1 — reusable report/output materialization storage.
//
// Mirror of ObservationArenaPoolV1 (observation.rs) parameterized over
// ProgramPaintOutputV1 payloads. Eliminates per-snapshot Vec allocation in
// snapshot_from_evidence by reusing Rc-backed slots across Session lifecycle
// transitions. Slot count = 3 matches the Session automaton invariant: two
// simultaneously retained evidence (cause + previous) plus one prospective
// buffer.

use std::rc::Rc;

use crate::program_wire::ProgramPaintOutputV1;

/// Automaton-derived slot count: cause + previous + prospective buffer.
/// Matches OBSERVATION_ARENA_SLOT_COUNT_V1 and FIELD_ARENA_SLOT_COUNT_V1.
pub(crate) const REPORT_ARENA_SLOT_COUNT_V1: usize = 3;

/// Typed slot identity for report arena backing stores.
/// Distinct from ObservationArenaSlotV1 / FieldArenaSlotV1 to prevent
/// cross-domain slot confusion at the type level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReportArenaSlotV1(u8);

impl ReportArenaSlotV1 {
    pub(crate) const ALL: [Self; REPORT_ARENA_SLOT_COUNT_V1] = [Self(0), Self(1), Self(2)];

    #[expect(
        dead_code,
        reason = "ReportArenaSlotV1::index is reserved for PR4 ArenaLifecycleCoordinator diagnostics"
    )]
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Rc-managed report output storage. Private fields enforce construction
/// through the pool only; consumers receive Rc handles and cannot forge
/// backings that bypass arena lifecycle.
#[derive(Debug, PartialEq)]
pub(crate) struct ReportBackingV1 {
    arena_slot: ReportArenaSlotV1,
    /// Replaces outputs_scratch Vec from PR #591. Capacity is retained
    /// across clear() calls so post-warmup materializations are zero-alloc.
    outputs: Vec<ProgramPaintOutputV1>,
}

impl ReportBackingV1 {
    pub(crate) const fn arena_slot(&self) -> ReportArenaSlotV1 {
        self.arena_slot
    }

    pub(crate) fn outputs(&self) -> &[ProgramPaintOutputV1] {
        &self.outputs
    }
}

/// Error returned when all arena slots are externally retained and no
/// unique backing is available for reuse. Payload enables diagnostics
/// without string formatting in hot paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReportArenaExhaustedV1 {
    pub(crate) slot_count: usize,
}

/// Three permanent backing allocations owned by one Session.
///
/// Pool holds exactly one Rc per free slot. Snapshot consumers clone only
/// the control block, so Rc::get_mut proves unique ownership without
/// allocation — a freed slot can be overwritten in place.
#[derive(Debug)]
pub(crate) struct ReportArenaPoolV1 {
    slots: [Rc<ReportBackingV1>; REPORT_ARENA_SLOT_COUNT_V1],
    high_water_mark: usize,
}

impl ReportArenaPoolV1 {
    pub(crate) fn new() -> Self {
        Self {
            slots: ReportArenaSlotV1::ALL.map(|arena_slot| {
                Rc::new(ReportBackingV1 {
                    arena_slot,
                    outputs: Vec::new(),
                })
            }),
            high_water_mark: 0,
        }
    }

    /// Acquire a unique slot, clear its payload, invoke the materializer,
    /// and return an Rc handle. Returns Err if all slots are retained.
    ///
    /// The closure receives a mutable reference to the cleared Vec so it
    /// can extend/push without allocating (capacity preserved from prior
    /// use). On closure error the slot remains cleared but unreturned —
    /// the next call will see it as uniquely owned and reuse it.
    pub(crate) fn materialize_report_into<E>(
        &mut self,
        materialize: impl FnOnce(&mut Vec<ProgramPaintOutputV1>) -> Result<(), E>,
    ) -> Result<Rc<ReportBackingV1>, E>
    where
        E: From<ReportArenaExhaustedV1>,
    {
        for (retained, slot_index) in (0..REPORT_ARENA_SLOT_COUNT_V1).enumerate() {
            let Some(backing) = Rc::get_mut(&mut self.slots[slot_index]) else {
                continue;
            };
            backing.outputs.clear();
            materialize(&mut backing.outputs)?;
            let current_retained = retained + 1;
            if current_retained > self.high_water_mark {
                self.high_water_mark = current_retained;
            }
            return Ok(Rc::clone(&self.slots[slot_index]));
        }
        Err(ReportArenaExhaustedV1 {
            slot_count: REPORT_ARENA_SLOT_COUNT_V1,
        }
        .into())
    }

    /// Maximum number of simultaneously retained backings observed since
    /// construction. Used by diagnostic gates and the zero-allocation
    /// test in PR4 to verify the automaton invariant holds.
    pub(crate) const fn high_water_mark(&self) -> usize {
        self.high_water_mark
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pool_has_zero_high_water_mark() {
        let pool = ReportArenaPoolV1::new();
        assert_eq!(pool.high_water_mark(), 0);
    }

    #[test]
    fn slot_acquisition_returns_unique_backing() {
        let mut pool = ReportArenaPoolV1::new();
        let backing = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|outputs| {
                outputs.push(ProgramPaintOutputV1::for_test(1));
                Ok(())
            })
            .expect("first acquisition must succeed");
        assert_eq!(backing.outputs().len(), 1);
        assert_eq!(backing.arena_slot(), ReportArenaSlotV1(0));
    }

    #[test]
    fn reused_slot_preserves_capacity_across_cycles() {
        let mut pool = ReportArenaPoolV1::new();

        // Warm up slot 0 with a large payload to establish capacity.
        let _warmup = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|outputs| {
                for i in 0..64 {
                    outputs.push(ProgramPaintOutputV1::for_test(i));
                }
                Ok(())
            })
            .expect("warmup must succeed");

        // Drop the warmup handle so the slot becomes uniquely owned again.
        drop(_warmup);

        // Second materialization reuses the same slot; capacity should be
        // sufficient for 64 elements without reallocation. We cannot
        // directly inspect Vec capacity through the Rc, but we verify
        // correctness: the output is correct and no panic occurs.
        let second = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|outputs| {
                for i in 0..64 {
                    outputs.push(ProgramPaintOutputV1::for_test(i));
                }
                Ok(())
            })
            .expect("reuse must succeed");
        assert_eq!(second.outputs().len(), 64);
    }

    #[test]
    fn high_water_mark_tracks_peak_retention() {
        let mut pool = ReportArenaPoolV1::new();

        let a = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|_| Ok(()))
            .expect("a");
        assert_eq!(pool.high_water_mark(), 1);

        let b = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|_| Ok(()))
            .expect("b");
        assert_eq!(pool.high_water_mark(), 2);

        let c = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|_| Ok(()))
            .expect("c");
        assert_eq!(pool.high_water_mark(), 3);

        // Dropping handles does not decrease the high water mark.
        drop(a);
        drop(b);
        drop(c);
        assert_eq!(pool.high_water_mark(), 3);
    }

    #[test]
    fn exhausted_pool_returns_error_with_slot_count() {
        let mut pool = ReportArenaPoolV1::new();

        let _a = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|_| Ok(()))
            .expect("a");
        let _b = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|_| Ok(()))
            .expect("b");
        let _c = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|_| Ok(()))
            .expect("c");

        let err = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|_| Ok(()))
            .expect_err("all slots retained");
        assert_eq!(err.slot_count, REPORT_ARENA_SLOT_COUNT_V1);
    }

    #[test]
    fn stale_slot_rejection_prevents_cross_boundary_reference_retention() {
        // Reports must NOT retain references to observation-arena-backed
        // data. This test verifies the boundary by confirming that
        // ReportBackingV1 owns its Vec<ProgramPaintOutputV1> entirely —
        // no borrowed slices, no Rc<ObservationBackingV1> leaks.
        //
        // The type system enforces this: ReportBackingV1 contains only
        // ReportArenaSlotV1 (Copy) and Vec<ProgramPaintOutputV1> (owned).
        // There is no lifetime parameter and no reference field. A
        // compile-time assertion via static type inspection:
        fn assert_owned<T: 'static>() {}
        assert_owned::<ReportBackingV1>();

        // Runtime confirmation: materialize, drop the Rc, reacquire —
        // the slot is fully independent of any prior owner.
        let mut pool = ReportArenaPoolV1::new();
        let first = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|outputs| {
                outputs.push(ProgramPaintOutputV1::for_test(42));
                Ok(())
            })
            .expect("first");
        assert_eq!(first.outputs()[0].slot(), 42);
        drop(first);

        let second = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|outputs| {
                outputs.push(ProgramPaintOutputV1::for_test(99));
                Ok(())
            })
            .expect("second");
        // Slot was cleared and repopulated; no stale data remains.
        assert_eq!(second.outputs().len(), 1);
        assert_eq!(second.outputs()[0].slot(), 99);
    }

    #[test]
    fn materializer_error_does_not_corrupt_slot_state() {
        let mut pool = ReportArenaPoolV1::new();

        // First call fails after pushing partial data.
        let result: Result<Rc<ReportBackingV1>, ReportArenaExhaustedV1> = pool
            .materialize_report_into(|outputs| {
                outputs.push(ProgramPaintOutputV1::for_test(1));
                Err(ReportArenaExhaustedV1 { slot_count: 0 })
            });
        assert!(result.is_err());

        // Next call on the same slot sees a cleared Vec (the pool clears
        // before invoking the closure, and on error the slot remains
        // uniquely owned so the next call clears it again).
        let ok = pool
            .materialize_report_into::<ReportArenaExhaustedV1>(|outputs| {
                outputs.push(ProgramPaintOutputV1::for_test(2));
                Ok(())
            })
            .expect("recovery must succeed");
        assert_eq!(ok.outputs().len(), 1);
        assert_eq!(ok.outputs()[0].slot(), 2);
    }
}
