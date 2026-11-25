//! Temporary verdicts for one component search. Its compiled constraint
//! ordinals belong to one immutable epoch; no entry outlives that search.

use crate::constraints::ProgramPointTargetV1;

#[derive(Clone, Copy)]
pub(super) struct SearchCell {
    pub(super) case: usize,
    pub(super) violation: bool,
}

#[derive(Clone, Copy)]
struct PointVerdict {
    constraint: usize,
    target: ProgramPointTargetV1,
    violation: bool,
}

pub(super) struct SearchProbe {
    pub(super) cells: Vec<SearchCell>,
    cache_field_points: bool,
    points: Option<Vec<Option<PointVerdict>>>,
}

impl SearchProbe {
    pub(super) fn new(physical_cases: usize) -> Self {
        Self {
            cells: Vec::new(),
            cache_field_points: physical_cases > 1,
            points: None,
        }
    }

    pub(super) fn begin(&mut self, cases: usize, constraints: usize) -> Result<(), ()> {
        self.cells.clear();
        let len = cases.checked_mul(constraints).ok_or(())?;
        self.cells.try_reserve(len).map_err(|_| ())
    }

    // Only the explicitly admitted numeric evaluators enter this method.
    // A point target exposes the two encoded colours and the emission bounds.
    // The ordinal fixes the invocation in this component's compiled program.
    pub(super) fn point_verdict<E>(
        &mut self,
        constraint: usize,
        target: ProgramPointTargetV1,
        assess: impl FnOnce() -> Result<bool, E>,
    ) -> Result<bool, E> {
        // A solid local backdrop needs no field cache, even when another
        // component in the same program spans many physical observations.
        if !self.cache_field_points {
            return assess();
        }
        // Power-of-two capacity bounds working memory independently of the
        // number of cases and source tuples examined. With no memory for the
        // optional cache, the original evaluator remains available.
        const SLOTS: usize = 1024;
        let slots = self.points.get_or_insert_with(|| {
            let mut slots = Vec::new();
            if slots.try_reserve_exact(SLOTS).is_ok() {
                slots.resize(SLOTS, None);
            }
            slots
        });
        // XXH32 primes mix each coordinate; collisions only replace a slot.
        let rgb = |[r, g, b]: [u8; 3]| u32::from_be_bytes([0, r, g, b]);
        let hash = rgb(target.encoded().visible()).wrapping_mul(0x9e37_79b1)
            ^ rgb(target.encoded().backdrop()).wrapping_mul(0x85eb_ca77)
            ^ (constraint as u32).wrapping_mul(0xc2b2_ae3d);
        let index = (hash >> (u32::BITS - SLOTS.ilog2())) as usize;
        let Some(slot) = slots.get_mut(index) else {
            return assess();
        };
        if let Some(entry) = slot {
            if entry.constraint == constraint && entry.target == target {
                return Ok(entry.violation);
            }
        }
        let violation = assess()?;
        *slot = Some(PointVerdict {
            constraint,
            target,
            violation,
        });
        Ok(violation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::PointOpacityOverSurfaceV1;
    use crate::composition::AdmittedOpacityV1;
    use crate::emission::PointEmissionV1;

    #[test]
    fn collisions_recheck_the_whole_physical_target_and_errors_leave_no_verdict() {
        let occurrence = PointOpacityOverSurfaceV1::evaluate_admitted(
            [130; 3],
            AdmittedOpacityV1::OPAQUE,
            [32; 3],
        );
        let encoded = occurrence.modeled_srgb8_point();
        let target = |emission| ProgramPointTargetV1::new(encoded, emission);
        let exact = target(PointEmissionV1::default());
        let mut solid = SearchProbe::new(1);
        assert_eq!(
            solid.point_verdict(0, exact, || Ok::<_, ()>(false)),
            Ok(false)
        );
        assert_eq!(solid.point_verdict(0, exact, || Err(())), Err(()));
        assert!(solid.points.is_none());

        let mut probe = SearchProbe::new(2);
        assert_eq!(
            probe.point_verdict(0, exact, || Ok::<_, ()>(false)),
            Ok(false)
        );
        assert_eq!(probe.point_verdict(0, exact, || Err(())), Ok(false));
        // The same encoded pair reaches the same slot for every emission.
        for different in [
            PointEmissionV1 {
                visible_radius: 1,
                ..Default::default()
            },
            PointEmissionV1 {
                backdrop_radius: 1,
                ..Default::default()
            },
            PointEmissionV1 {
                same_as_backdrop: true,
                ..Default::default()
            },
        ] {
            assert_eq!(
                probe.point_verdict(0, target(different), || Err(())),
                Err(())
            );
            assert_eq!(
                probe.point_verdict(0, target(different), || Ok::<_, ()>(true)),
                Ok(true)
            );
            assert_eq!(
                probe.point_verdict(0, exact, || Ok::<_, ()>(false)),
                Ok(false)
            );
        }
        assert_eq!(
            probe.point_verdict(1, exact, || Ok::<_, ()>(true)),
            Ok(true)
        );
        assert_eq!(
            probe.point_verdict(0, exact, || Ok::<_, ()>(false)),
            Ok(false)
        );
    }
}
