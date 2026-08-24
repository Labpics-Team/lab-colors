//! Pre-allocated arena pools for field evaluation and raster storage.
//!
//! Two arena families live here:
//!
//! - `FieldArenaPoolV1` (O-13 PR2): general-purpose reusable backing for field
//!   evaluation results. Payload is a placeholder `Vec<u8>` until O-06 PR1
//!   lands the concrete type; pool mechanics are type-agnostic.
//! - `FieldRasterArenaPoolV1`: specialized raster-scale buffer pool reusing
//!   megabyte-scale allocations via `Rc::get_mut`.
//!
//! Both follow the O-13 three-slot Rc-backed pattern established by
//! `ObservationArenaPoolV1`.
//!
//! Certificate arena (`FieldCertificateArenaPoolV1`) is defined in
//! `field_presentation` (PR 2) because it depends on
//! `FieldPresentationCertificateV1` which does not exist until that PR.

#![allow(
    dead_code,
    reason = "O-13 PR2: field arena types are staged before their ProgramSession consumer in PR4"
)]

use std::rc::Rc;

use crate::field_effect::{FieldEvaluationErrorV1, FieldExtentV1, PremultipliedRgba8V1};

// ---------------------------------------------------------------------------
// O-13 PR2: FieldArenaPoolV1 — general-purpose field evaluation backing
// ---------------------------------------------------------------------------

/// Automaton-invariant slot count: two simultaneously retained evidence
/// (cause + previous) plus one prospective buffer. Matches
/// `OBSERVATION_ARENA_SLOT_COUNT_V1`; typed separately to prevent
/// cross-domain slot confusion.
pub(crate) const FIELD_ARENA_SLOT_COUNT_V1: usize = 3;

/// Closed identity for a field arena slot. Identifies only the storage
/// location; Session remains the lifecycle authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldArenaSlotV1(u8);

impl FieldArenaSlotV1 {
    pub(crate) const ALL: [Self; FIELD_ARENA_SLOT_COUNT_V1] = [Self(0), Self(1), Self(2)];

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Rc-managed field evaluation backing. Private fields ensure callers receive
/// only pool-owned handles; construction outside the pool is impossible.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FieldEvaluationBackingV1 {
    arena_slot: FieldArenaSlotV1,
    /// Placeholder payload. O-06 PR1 will replace `Vec<u8>` with the concrete
    /// field result storage. The pool's reuse semantics depend only on
    /// `clear()` + capacity retention, which holds for any Vec-like payload.
    payload: Vec<u8>,
}

impl FieldEvaluationBackingV1 {
    pub(crate) const fn arena_slot(&self) -> FieldArenaSlotV1 {
        self.arena_slot
    }

    pub(crate) fn payload(&self) -> &[u8] {
        &self.payload
    }

    #[cfg(test)]
    pub(crate) fn payload_mut_for_test(&mut self) -> &mut Vec<u8> {
        &mut self.payload
    }
}

/// Three permanent backing allocations owned by a single Session.
///
/// The pool holds exactly one `Rc` per free slot. Consumers clone only the
/// control block, so `Rc::get_mut` proves unique ownership without allocation
/// and the freed slot can be overwritten in place.
#[derive(Debug)]
pub(crate) struct FieldArenaPoolV1 {
    slots: [Rc<FieldEvaluationBackingV1>; FIELD_ARENA_SLOT_COUNT_V1],
    high_water_mark: usize,
    live_count: usize,
}

impl FieldArenaPoolV1 {
    pub(crate) fn new() -> Self {
        Self {
            slots: FieldArenaSlotV1::ALL.map(|arena_slot| {
                Rc::new(FieldEvaluationBackingV1 {
                    arena_slot,
                    payload: Vec::new(),
                })
            }),
            high_water_mark: 0,
            live_count: 0,
        }
    }

    /// Acquire a uniquely-owned slot, run `evaluate` against its payload, and
    /// return a cloned Rc handle. Returns `ArenaExhausted` if every slot is
    /// still retained by a consumer (internal invariant violation).
    pub(crate) fn evaluate_into(
        &mut self,
        evaluate: impl FnOnce(&mut Vec<u8>) -> Result<(), FieldEvaluationErrorV1>,
    ) -> Result<Rc<FieldEvaluationBackingV1>, FieldEvaluationErrorV1> {
        for slot_index in 0..FIELD_ARENA_SLOT_COUNT_V1 {
            let Some(backing) = Rc::get_mut(&mut self.slots[slot_index]) else {
                continue;
            };
            backing.payload.clear();
            evaluate(&mut backing.payload)?;
            self.live_count += 1;
            if self.live_count > self.high_water_mark {
                self.high_water_mark = self.live_count;
            }
            return Ok(Rc::clone(&self.slots[slot_index]));
        }
        Err(FieldEvaluationErrorV1::ArenaExhausted {
            slot_count: FIELD_ARENA_SLOT_COUNT_V1,
        })
    }

    /// Maximum number of simultaneously retained backings observed since
    /// construction. Used by diagnostic gates; must stay <= slot count.
    pub(crate) const fn high_water_mark(&self) -> usize {
        self.high_water_mark
    }

    /// Called when a backing is retired (dropped by its consumer). Tracks
    /// live count for high-water accounting. In production this is invoked
    /// by the retirement guard; tests call it directly.
    #[cfg(test)]
    pub(crate) fn retire_for_test(&mut self) {
        self.live_count = self.live_count.saturating_sub(1);
    }
}

// ---------------------------------------------------------------------------
// FieldRasterArenaPoolV1 — raster-scale buffer reuse
// ---------------------------------------------------------------------------

/// Number of raster arena slots. Matches OBSERVATION_ARENA_SLOT_COUNT_V1
/// for cross-subsystem consistency.
pub(crate) const FIELD_RASTER_ARENA_SLOT_COUNT_V1: usize = 3;

/// Opaque slot index for raster arena backing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FieldRasterArenaSlotV1(u8);

impl FieldRasterArenaSlotV1 {
    const ALL: [Self; FIELD_RASTER_ARENA_SLOT_COUNT_V1] = [Self(0), Self(1), Self(2)];

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

/// Single raster backing buffer owned by the arena pool.
///
/// Private fields prevent construction outside the pool. The parent
/// admission receives only a ready pool-owned Rc handle.
#[derive(Debug)]
pub(crate) struct FieldRasterBackingV1 {
    arena_slot: FieldRasterArenaSlotV1,
    extent: FieldExtentV1,
    buffer: Vec<PremultipliedRgba8V1>,
}

impl FieldRasterBackingV1 {
    pub(crate) const fn arena_slot(&self) -> FieldRasterArenaSlotV1 {
        self.arena_slot
    }

    pub(crate) const fn extent(&self) -> FieldExtentV1 {
        self.extent
    }

    pub(crate) fn buffer(&self) -> &[PremultipliedRgba8V1] {
        &self.buffer
    }
}

/// Pre-allocated raster buffer pool for field evaluation.
///
/// Three slots matching OBSERVATION_ARENA_SLOT_COUNT_V1 for consistency.
/// Buffers are reused when Rc::get_mut succeeds (unique ownership).
#[derive(Debug)]
pub(crate) struct FieldRasterArenaPoolV1 {
    slots: [Rc<FieldRasterBackingV1>; FIELD_RASTER_ARENA_SLOT_COUNT_V1],
}

impl FieldRasterArenaPoolV1 {
    pub(crate) fn new(extent: FieldExtentV1) -> Self {
        let pixel_count = (extent.width() as usize) * (extent.height() as usize);
        Self {
            slots: FieldRasterArenaSlotV1::ALL.map(|arena_slot| {
                Rc::new(FieldRasterBackingV1 {
                    arena_slot,
                    extent,
                    buffer: vec![PremultipliedRgba8V1::TRANSPARENT; pixel_count],
                })
            }),
        }
    }

    /// Find a uniquely-owned slot and populate it via closure.
    /// Returns Rc handle to the populated backing.
    ///
    /// # Errors
    ///
    /// Returns `FieldEvaluationErrorV1::ArenaExhausted` if all three slots
    /// are currently borrowed (Rc strong count > 1).
    pub(crate) fn materialize_into(
        &mut self,
        materialize: impl FnOnce(&mut [PremultipliedRgba8V1]) -> Result<(), FieldEvaluationErrorV1>,
    ) -> Result<Rc<FieldRasterBackingV1>, FieldEvaluationErrorV1> {
        for slot_index in 0..FIELD_RASTER_ARENA_SLOT_COUNT_V1 {
            let Some(backing) = Rc::get_mut(&mut self.slots[slot_index]) else {
                continue;
            };
            materialize(&mut backing.buffer)?;
            return Ok(Rc::clone(&self.slots[slot_index]));
        }
        Err(FieldEvaluationErrorV1::ArenaExhausted {
            slot_count: FIELD_RASTER_ARENA_SLOT_COUNT_V1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_raster_arena_materialize_reuses_slot() {
        let extent = FieldExtentV1::try_new(4, 4).expect("valid test extent");
        let mut pool = FieldRasterArenaPoolV1::new(extent);

        let first = pool
            .materialize_into(|buf| {
                buf[0] = PremultipliedRgba8V1::TRANSPARENT;
                Ok(())
            })
            .expect("first materialize must succeed");
        assert_eq!(first.arena_slot().index(), 0);

        // Drop first handle so slot 0 becomes uniquely owned again.
        drop(first);

        let second = pool
            .materialize_into(|buf| {
                buf[0] = PremultipliedRgba8V1::TRANSPARENT;
                Ok(())
            })
            .expect("second materialize must reuse slot 0");
        assert_eq!(second.arena_slot().index(), 0);
    }

    #[test]
    fn test_raster_arena_exhaustion() {
        let extent = FieldExtentV1::try_new(2, 2).expect("valid test extent");
        let mut pool = FieldRasterArenaPoolV1::new(extent);

        let _a = pool.materialize_into(|_| Ok(())).expect("slot 0");
        let _b = pool.materialize_into(|_| Ok(())).expect("slot 1");
        let _c = pool.materialize_into(|_| Ok(())).expect("slot 2");

        let err = pool
            .materialize_into(|_| Ok(()))
            .expect_err("all slots held");
        assert!(matches!(
            err,
            FieldEvaluationErrorV1::ArenaExhausted { slot_count: 3 }
        ));
    }

    #[test]
    fn test_raster_arena_buffer_size_matches_extent() {
        let extent = FieldExtentV1::try_new(16, 8).expect("valid test extent");
        let pool = FieldRasterArenaPoolV1::new(extent);
        assert_eq!(pool.slots[0].buffer.len(), 16 * 8);
    }

    #[test]
    fn test_raster_arena_large_allocation() {
        // 4K UHD: 3840 x 2160 = 8,294,400 pixels x 4 bytes ~ 31.6 MiB per slot
        let extent = FieldExtentV1::try_new(3840, 2160).expect("valid test extent");
        let pool = FieldRasterArenaPoolV1::new(extent);
        assert_eq!(pool.slots[0].buffer.len(), 3840 * 2160);
    }

    // -----------------------------------------------------------------------
    // O-13 PR2: FieldArenaPoolV1 tests
    // -----------------------------------------------------------------------

    #[test]
    fn field_arena_evaluate_into_returns_unique_backing_per_call() {
        let mut pool = FieldArenaPoolV1::new();
        let b1 = pool
            .evaluate_into(|buf| {
                buf.extend_from_slice(b"first");
                Ok(())
            })
            .expect("slot 0 available");
        let b2 = pool
            .evaluate_into(|buf| {
                buf.extend_from_slice(b"second");
                Ok(())
            })
            .expect("slot 1 available");
        assert_ne!(b1.payload(), b2.payload());
        assert_eq!(b1.payload(), b"first");
        assert_eq!(b2.payload(), b"second");
    }

    #[test]
    fn field_arena_evaluate_into_reuses_capacity_across_cycles() {
        let mut pool = FieldArenaPoolV1::new();

        // Warm up: fill all three slots with growing payloads.
        let mut handles: Vec<Rc<FieldEvaluationBackingV1>> = Vec::new();
        for i in 0..FIELD_ARENA_SLOT_COUNT_V1 {
            let h = pool
                .evaluate_into(|buf| {
                    buf.resize(i * 64 + 128, 0xAA);
                    Ok(())
                })
                .expect("warm-up slot available");
            handles.push(h);
        }
        assert_eq!(pool.high_water_mark(), FIELD_ARENA_SLOT_COUNT_V1);

        // Retire all handles so slots become uniquely owned again.
        handles.clear();
        for _ in 0..FIELD_ARENA_SLOT_COUNT_V1 {
            pool.retire_for_test();
        }

        // Second cycle: smaller payloads must reuse existing capacity.
        for _ in 0..100 {
            let h = pool
                .evaluate_into(|buf| {
                    buf.extend_from_slice(b"reuse");
                    Ok(())
                })
                .expect("slot reused after retire");
            assert_eq!(h.payload(), b"reuse");
            drop(h);
            pool.retire_for_test();
        }
        assert_eq!(pool.high_water_mark(), FIELD_ARENA_SLOT_COUNT_V1);
    }

    #[test]
    fn field_arena_exhaustion_returns_structured_error() {
        let mut pool = FieldArenaPoolV1::new();
        let mut held = Vec::new();
        for _ in 0..FIELD_ARENA_SLOT_COUNT_V1 {
            held.push(
                pool.evaluate_into(|buf| {
                    buf.push(1);
                    Ok(())
                })
                .expect("fill slot"),
            );
        }
        let err = pool
            .evaluate_into(|_| Ok(()))
            .expect_err("all slots retained");
        assert_eq!(
            err,
            FieldEvaluationErrorV1::ArenaExhausted {
                slot_count: FIELD_ARENA_SLOT_COUNT_V1
            }
        );
    }

    #[test]
    fn field_arena_propagates_closure_error_without_consuming_slot() {
        let mut pool = FieldArenaPoolV1::new();
        let err = pool
            .evaluate_into(|_| {
                Err(FieldEvaluationErrorV1::ArenaExhausted {
                    slot_count: FIELD_ARENA_SLOT_COUNT_V1,
                })
            })
            .expect_err("closure error propagated");
        assert_eq!(
            err,
            FieldEvaluationErrorV1::ArenaExhausted {
                slot_count: FIELD_ARENA_SLOT_COUNT_V1
            }
        );
        let h = pool
            .evaluate_into(|buf| {
                buf.push(42);
                Ok(())
            })
            .expect("slot still available after error");
        assert_eq!(h.payload(), &[42]);
    }

    #[test]
    fn field_arena_high_water_mark_tracks_peak_retention() {
        let mut pool = FieldArenaPoolV1::new();
        assert_eq!(pool.high_water_mark(), 0);

        let h1 = pool
            .evaluate_into(|b| {
                b.push(1);
                Ok(())
            })
            .expect("slot 0");
        assert_eq!(pool.high_water_mark(), 1);

        let h2 = pool
            .evaluate_into(|b| {
                b.push(2);
                Ok(())
            })
            .expect("slot 1");
        assert_eq!(pool.high_water_mark(), 2);

        drop(h1);
        pool.retire_for_test();
        assert_eq!(pool.high_water_mark(), 2);

        let h3 = pool
            .evaluate_into(|b| {
                b.push(3);
                Ok(())
            })
            .expect("slot 2");
        assert_eq!(pool.high_water_mark(), 2);

        drop(h2);
        drop(h3);
        pool.retire_for_test();
        pool.retire_for_test();
        assert_eq!(pool.high_water_mark(), 2);
    }

    #[test]
    fn field_arena_slot_identity_is_stable_across_reuse() {
        let mut pool = FieldArenaPoolV1::new();
        let first_slot = pool
            .evaluate_into(|b| {
                b.push(1);
                Ok(())
            })
            .expect("first acquire")
            .arena_slot();
        pool.retire_for_test();
        let second_slot = pool
            .evaluate_into(|b| {
                b.push(2);
                Ok(())
            })
            .expect("second acquire")
            .arena_slot();
        assert_eq!(first_slot, second_slot);
    }
}
