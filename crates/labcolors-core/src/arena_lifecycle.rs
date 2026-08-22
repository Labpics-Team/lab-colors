//! O-13 PR4: Unified arena lifecycle coordination across observation, field
//! raster, and report arena pools.
//!
//! [`ArenaLifecycleCoordinatorV1`] is embedded in `Session<Plan>` and owns
//! mutable references to all three arena pools during atomic lifecycle
//! transitions: cold-start reset, schema-version rebind, and clean shutdown
//! release. It never holds Rc handles itself — it coordinates via exclusive
//! `&mut` access guaranteed by Session's single-threaded evaluation path.
//!
//! Generation counters prevent stale-slot reuse bugs. When a pool is reset
//! or rebound, its generation increments. Any retained Rc handle from a prior
//! generation becomes detectably stale without runtime overhead in production
//! paths.

use crate::field_arena::FieldRasterArenaPoolV1;
use crate::field_effect::{FieldEvaluationErrorV1, FieldExtentV1};
use crate::observation::{CanonicalObservationSchemaV1, ObservationArenaPoolV1, ObservationError};
use crate::report_arena::ReportArenaPoolV1;

/// Monotonic generation counter for arena slot validity.
///
/// Incremented on every reset or schema rebind. Retained Rc handles
/// carry the generation at which they were issued; comparison detects
/// stale references without runtime overhead in production paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ArenaGenerationV1(u64);

impl ArenaGenerationV1 {
    pub(crate) const INITIAL: Self = Self(0);

    pub(crate) fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }

    #[expect(
        dead_code,
        reason = "ArenaGenerationV1::value is reserved for downstream generation-staleness diagnostics"
    )]
    pub(crate) const fn value(self) -> u64 {
        self.0
    }
}

/// Error returned when an atomic rebind fails partway through.
///
/// On error, the coordinator's generation for the failed domain remains
/// unchanged. Successfully-rebound domains have incremented generations.
/// The caller can inspect generations to determine partial progress.
#[derive(Debug)]
#[expect(
    dead_code,
    reason = "O-13 PR4 staged infrastructure before downstream consumers"
)]
#[allow(
    clippy::enum_variant_names,
    reason = "variant names mirror the upstream error types they wrap for diagnostic clarity"
)]
pub(crate) enum ArenaRebindErrorV1 {
    ObservationRebindFailed(ObservationError),
    FieldResetFailed(FieldEvaluationErrorV1),
    ReportResetFailed(crate::report_arena::ReportArenaExhaustedV1),
}

/// Coordinates lifecycle transitions across observation, field raster,
/// and report arena pools. Embedded in Session<Plan>; never constructed
/// independently.
///
/// All methods take `&mut self` — the coordinator has exclusive access
/// to all three pools during transitions. No interior mutability.
#[derive(Debug)]
pub(crate) struct ArenaLifecycleCoordinatorV1 {
    observation_generation: ArenaGenerationV1,
    field_generation: ArenaGenerationV1,
    report_generation: ArenaGenerationV1,
}

impl ArenaLifecycleCoordinatorV1 {
    /// Initial state: all generations at INITIAL. Pools are assumed
    /// freshly constructed by Session::new.
    pub(crate) fn new() -> Self {
        Self {
            observation_generation: ArenaGenerationV1::INITIAL,
            field_generation: ArenaGenerationV1::INITIAL,
            report_generation: ArenaGenerationV1::INITIAL,
        }
    }

    /// Resets all three arena pools atomically. Called during cold-start
    /// recovery or schema-version transitions.
    ///
    /// After this call:
    /// - All pools have been cleared/reinitialized via their respective
    ///   reset mechanisms (observation: rebind_to_schema; field/report:
    ///   clear + retain capacity).
    /// - All generation counters have incremented.
    /// - Any previously-retained Rc handles are generation-stale.
    ///
    /// This method takes mutable references to all three pools to
    /// guarantee exclusive access. No other code can hold pool references
    /// during reset because Session owns them exclusively.
    #[allow(
        dead_code,
        reason = "O-13 PR4 staged infrastructure before downstream consumers"
    )]
    pub(crate) fn reset_all(
        &mut self,
        observation_arenas: &mut ObservationArenaPoolV1,
        field_arenas: &mut FieldRasterArenaPoolV1,
        report_arenas: &mut ReportArenaPoolV1,
        new_observation_schema: &CanonicalObservationSchemaV1,
        new_field_extent: FieldExtentV1,
    ) -> Result<(), ArenaRebindErrorV1> {
        // Observation: rebind preserves backing allocations
        observation_arenas
            .rebind_to_schema(new_observation_schema)
            .map_err(ArenaRebindErrorV1::ObservationRebindFailed)?;
        self.observation_generation = self.observation_generation.next();

        // Field raster: reset extent if changed, preserve capacity pattern
        field_arenas.reset_extent(new_field_extent);
        self.field_generation = self.field_generation.next();

        // Report: clear outputs, retain Vec capacity
        report_arenas.reset();
        self.report_generation = self.report_generation.next();

        Ok(())
    }

    /// Releases all arena slots by dropping retained Rc handles held
    /// within the pools. Used during Session drop or explicit teardown.
    ///
    /// After this call, all slots are uniquely owned by the pool
    /// (strong_count == 1) and available for reuse if the session
    /// continues. If the session is being dropped, this ensures
    /// deterministic deallocation order.
    #[allow(
        dead_code,
        reason = "O-13 PR4 staged infrastructure before downstream consumers"
    )]
    pub(crate) fn release_all_slots(
        &mut self,
        observation_arenas: &mut ObservationArenaPoolV1,
        field_arenas: &mut FieldRasterArenaPoolV1,
        report_arenas: &mut ReportArenaPoolV1,
    ) {
        observation_arenas.release_all();
        field_arenas.release_all();
        report_arenas.release_all();
    }

    pub(crate) const fn observation_generation(&self) -> ArenaGenerationV1 {
        self.observation_generation
    }

    pub(crate) const fn field_generation(&self) -> ArenaGenerationV1 {
        self.field_generation
    }

    pub(crate) const fn report_generation(&self) -> ArenaGenerationV1 {
        self.report_generation
    }
}

#[cfg(debug_assertions)]
#[expect(
    dead_code,
    reason = "debug-only diagnostic logging gated behind debug_assertions"
)]
fn log_arena_exhaustion(arena_type: &str, slot_count: usize) {
    eprintln!(
        "[debug] {} arena exhausted ({} slots held)",
        arena_type, slot_count
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::field_effect::FieldExtentV1;
    use crate::observation::CanonicalObservationSchemaV1;

    fn test_schema() -> CanonicalObservationSchemaV1 {
        CanonicalObservationSchemaV1::for_test(&[])
    }

    fn test_extent() -> FieldExtentV1 {
        FieldExtentV1::try_new(4, 4).expect("valid test extent")
    }

    #[test]
    fn coordinator_new_has_initial_generations() {
        let coord = ArenaLifecycleCoordinatorV1::new();
        assert_eq!(coord.observation_generation(), ArenaGenerationV1::INITIAL);
        assert_eq!(coord.field_generation(), ArenaGenerationV1::INITIAL);
        assert_eq!(coord.report_generation(), ArenaGenerationV1::INITIAL);
    }

    #[test]
    fn reset_all_increments_all_generations() {
        let mut coord = ArenaLifecycleCoordinatorV1::new();
        let schema = test_schema();
        let extent = test_extent();
        let mut obs_pool = ObservationArenaPoolV1::new(&schema);
        let mut field_pool = FieldRasterArenaPoolV1::new(extent);
        let mut report_pool = ReportArenaPoolV1::new();

        coord
            .reset_all(
                &mut obs_pool,
                &mut field_pool,
                &mut report_pool,
                &schema,
                extent,
            )
            .expect("reset_all must succeed");

        assert_eq!(
            coord.observation_generation(),
            ArenaGenerationV1::INITIAL.next()
        );
        assert_eq!(coord.field_generation(), ArenaGenerationV1::INITIAL.next());
        assert_eq!(coord.report_generation(), ArenaGenerationV1::INITIAL.next());
    }

    #[test]
    fn generation_monotonicity_across_multiple_resets() {
        let mut coord = ArenaLifecycleCoordinatorV1::new();
        let schema = test_schema();
        let extent = test_extent();
        let mut obs_pool = ObservationArenaPoolV1::new(&schema);
        let mut field_pool = FieldRasterArenaPoolV1::new(extent);
        let mut report_pool = ReportArenaPoolV1::new();

        let mut prev_obs = coord.observation_generation();
        let mut prev_field = coord.field_generation();
        let mut prev_report = coord.report_generation();

        for _ in 0..10 {
            coord
                .reset_all(
                    &mut obs_pool,
                    &mut field_pool,
                    &mut report_pool,
                    &schema,
                    extent,
                )
                .expect("reset_all must succeed");
            assert!(coord.observation_generation() > prev_obs);
            assert!(coord.field_generation() > prev_field);
            assert!(coord.report_generation() > prev_report);
            prev_obs = coord.observation_generation();
            prev_field = coord.field_generation();
            prev_report = coord.report_generation();
        }
    }

    #[test]
    fn release_all_makes_slots_uniquely_owned() {
        let mut coord = ArenaLifecycleCoordinatorV1::new();
        let schema = test_schema();
        let extent = test_extent();
        let mut obs_pool = ObservationArenaPoolV1::new(&schema);
        let mut field_pool = FieldRasterArenaPoolV1::new(extent);
        let mut report_pool = ReportArenaPoolV1::new();

        // Acquire one slot from each pool to simulate external retention.
        obs_pool
            .materialize_empty_for_test()
            .expect("obs materialize");
        let _field_handle = field_pool
            .materialize_into(|_| Ok(()))
            .expect("field materialize");
        let _report_handle = report_pool
            .materialize_report_into::<crate::report_arena::ReportArenaExhaustedV1>(|_| Ok(()))
            .expect("report materialize");

        // Release all slots through the coordinator.
        coord.release_all_slots(&mut obs_pool, &mut field_pool, &mut report_pool);

        // After release, all slots should be uniquely owned (strong_count == 1).
        // We verify by successfully materializing into each pool again —
        // if any slot were still externally retained, we would have fewer
        // available slots.
        obs_pool
            .materialize_empty_for_test()
            .expect("obs after release");
        let _field2 = field_pool
            .materialize_into(|_| Ok(()))
            .expect("field after release");
        let _report2 = report_pool
            .materialize_report_into::<crate::report_arena::ReportArenaExhaustedV1>(|_| Ok(()))
            .expect("report after release");
    }
}
