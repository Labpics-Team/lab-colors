//! Pre-allocated arena pools for field raster storage.
//!
//! Specializes the O-13 three-slot Rc-backed pattern for raster-scale
//! allocations. Raster arenas use buffer reuse via `Rc::get_mut` to avoid
//! per-frame heap churn at megabyte scale.
//!
//! Certificate arena (`FieldCertificateArenaPoolV1`) is defined in
//! `field_presentation` (PR 2) because it depends on
//! `FieldPresentationCertificateV1` which does not exist until that PR.

use std::rc::Rc;

use crate::field_effect::{FieldEvaluationErrorV1, FieldExtentV1, PremultipliedRgba8V1};

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
        // 4K UHD: 3840 x 2160 = 8,294,400 pixels × 4 bytes ≈ 31.6 MiB per slot
        let extent = FieldExtentV1::try_new(3840, 2160).expect("valid test extent");
        let pool = FieldRasterArenaPoolV1::new(extent);
        assert_eq!(pool.slots[0].buffer.len(), 3840 * 2160);
    }
}