//! V-17b incremental runtime type foundation (staged).
//!
//! This module introduces the core types for the incremental transition
//! contract: `StampTransitionV1`, its error type, a joint feasible path
//! identifier placeholder, and the atomic switch fallback disposition.
//!
//! All types are `pub(crate)` and staged under `#![expect(dead_code)]` until
//! the first runtime consumer lands in a subsequent PR. No unsafe, no
//! Arc<Mutex>, no `.unwrap()` on production paths.

#![expect(
    dead_code,
    reason = "V-17b type foundation is staged before the first runtime consumer"
)]

use core::num::NonZeroU64;

/// Binding epoch for stamp transitions. Mirrors `PointSinkBindingEpochV1`
/// semantics: an opaque non-zero epoch tag that resets the sequence space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StampBindingEpochV1(NonZeroU64);

impl StampBindingEpochV1 {
    pub(crate) const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }
}

/// A fixed-size Copy stamp identifying a committed state within a binding epoch.
/// Modelled after `PointSinkStampV1`: two-word CAS token with monotonic sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StampV1 {
    sequence: u64,
    binding_epoch: StampBindingEpochV1,
}

impl StampV1 {
    pub(crate) const fn new(sequence: u64, binding_epoch: StampBindingEpochV1) -> Self {
        Self {
            sequence,
            binding_epoch,
        }
    }

    pub(crate) const fn sequence(self) -> u64 {
        self.sequence
    }

    pub(crate) const fn binding_epoch(self) -> StampBindingEpochV1 {
        self.binding_epoch
    }

    /// Returns the next stamp in the same epoch, or `None` on overflow.
    const fn checked_successor(self) -> Option<Self> {
        match self.sequence.checked_add(1) {
            Some(sequence) => Some(Self::new(sequence, self.binding_epoch)),
            None => None,
        }
    }
}

/// Typed error for invalid stamp transitions. Closed enum for exhaustive matching.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StampTransitionErrorV1 {
    /// Destination sequence is not exactly source + 1.
    SequenceRegression {
        source_sequence: u64,
        destination_sequence: u64,
    },
    /// Source and destination belong to different binding epochs.
    EpochMismatch {
        source_epoch: NonZeroU64,
        destination_epoch: NonZeroU64,
    },
    /// Source sequence is at u64::MAX; no successor exists.
    NonAdjacentTransition {
        source_sequence: u64,
    },
}

impl core::fmt::Display for StampTransitionErrorV1 {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::SequenceRegression {
                source_sequence,
                destination_sequence,
            } => write!(
                f,
                "stamp transition sequence regression: source={source_sequence}, destination={destination_sequence}"
            ),
            Self::EpochMismatch {
                source_epoch,
                destination_epoch,
            } => write!(
                f,
                "stamp transition epoch mismatch: source={source_epoch}, destination={destination_epoch}"
            ),
            Self::NonAdjacentTransition { source_sequence } => write!(
                f,
                "stamp transition non-adjacent: source sequence {source_sequence} has no successor"
            ),
        }
    }
}

/// A Copy typestate token describing an incremental visual transition between
/// two committed stamp states within the same binding epoch. Enforces monotonic
/// sequence increment via the checked constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct StampTransitionV1 {
    source: StampV1,
    destination: StampV1,
}

impl StampTransitionV1 {
    /// Constructs a valid transition from `source` to `destination`.
    ///
    /// Returns `Err` if:
    /// - The epochs differ.
    /// - The destination is not exactly `source + 1`.
    /// - The source sequence overflows (no successor exists).
    pub(crate) const fn try_new(
        source: StampV1,
        destination: StampV1,
    ) -> Result<Self, StampTransitionErrorV1> {
        if source.binding_epoch.0.get() != destination.binding_epoch.0.get() {
            return Err(StampTransitionErrorV1::EpochMismatch {
                source_epoch: source.binding_epoch.0,
                destination_epoch: destination.binding_epoch.0,
            });
        }

        match source.checked_successor() {
            Some(expected) => {
                if expected.sequence == destination.sequence {
                    Ok(Self {
                        source,
                        destination,
                    })
                } else {
                    Err(StampTransitionErrorV1::SequenceRegression {
                        source_sequence: source.sequence,
                        destination_sequence: destination.sequence,
                    })
                }
            }
            None => Err(StampTransitionErrorV1::NonAdjacentTransition {
                source_sequence: source.sequence,
            }),
        }
    }

    pub(crate) const fn source(self) -> StampV1 {
        self.source
    }

    pub(crate) const fn destination(self) -> StampV1 {
        self.destination
    }
}

/// Opaque identifier for a joint feasible path computation result.
/// Placeholder type for criterion #6 — no solver logic in this PR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct JointFeasiblePathIdV1(NonZeroU64);

impl JointFeasiblePathIdV1 {
    pub(crate) const fn new(value: NonZeroU64) -> Self {
        Self(value)
    }
}

/// Disposition of an atomic switch when no certified feasible path exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum AtomicSwitchFallbackV1 {
    /// Transition was committed successfully.
    Committed,
    /// No certified path; fell back to full re-resolve.
    FallbackToFull,
    /// Transition rejected with a reason.
    Rejected(StampTransitionErrorV1),
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::num::NonZeroU64;

    fn epoch(n: u64) -> StampBindingEpochV1 {
        StampBindingEpochV1::new(NonZeroU64::new(n).expect("test epoch must be non-zero"))
    }

    #[test]
    fn valid_transition_round_trips_source_and_destination() {
        let e = epoch(1);
        let source = StampV1::new(10, e);
        let destination = StampV1::new(11, e);

        let transition = StampTransitionV1::try_new(source, destination)
            .expect("valid adjacent transition must succeed");

        assert_eq!(transition.source(), source);
        assert_eq!(transition.destination(), destination);
    }

    #[test]
    fn sequence_regression_rejected_with_typed_error() {
        let e = epoch(1);
        let source = StampV1::new(10, e);
        let destination = StampV1::new(9, e); // regression

        let err = StampTransitionV1::try_new(source, destination)
            .expect_err("regression must fail");

        assert_eq!(
            err,
            StampTransitionErrorV1::SequenceRegression {
                source_sequence: 10,
                destination_sequence: 9,
            }
        );
    }

    #[test]
    fn sequence_skip_rejected_with_typed_error() {
        let e = epoch(1);
        let source = StampV1::new(10, e);
        let destination = StampV1::new(12, e); // skip

        let err = StampTransitionV1::try_new(source, destination)
            .expect_err("non-adjacent forward jump must fail");

        assert_eq!(
            err,
            StampTransitionErrorV1::SequenceRegression {
                source_sequence: 10,
                destination_sequence: 12,
            }
        );
    }

    #[test]
    fn epoch_mismatch_rejected_with_typed_error() {
        let source = StampV1::new(10, epoch(1));
        let destination = StampV1::new(11, epoch(2));

        let err = StampTransitionV1::try_new(source, destination)
            .expect_err("epoch mismatch must fail");

        assert_eq!(
            err,
            StampTransitionErrorV1::EpochMismatch {
                source_epoch: NonZeroU64::new(1).expect("non-zero"),
                destination_epoch: NonZeroU64::new(2).expect("non-zero"),
            }
        );
    }

    #[test]
    fn overflow_source_rejected_as_non_adjacent() {
        let e = epoch(1);
        let source = StampV1::new(u64::MAX, e);
        let destination = StampV1::new(0, e);

        let err = StampTransitionV1::try_new(source, destination)
            .expect_err("overflow must fail");

        assert_eq!(
            err,
            StampTransitionErrorV1::NonAdjacentTransition {
                source_sequence: u64::MAX,
            }
        );
    }

    #[test]
    fn atomic_switch_fallback_exhaustive_match() {
        let e = epoch(1);
        let source = StampV1::new(5, e);
        let dest_bad = StampV1::new(3, e);

        let committed = AtomicSwitchFallbackV1::Committed;
        let fallback = AtomicSwitchFallbackV1::FallbackToFull;
        let rejected = AtomicSwitchFallbackV1::Rejected(
            StampTransitionV1::try_new(source, dest_bad).expect_err("must fail"),
        );

        // Exhaustive match proves all variants are constructible and matchable.
        let label = match committed {
            AtomicSwitchFallbackV1::Committed => "c",
            AtomicSwitchFallbackV1::FallbackToFull => "f",
            AtomicSwitchFallbackV1::Rejected(_) => "r",
        };
        assert_eq!(label, "c");

        let label = match fallback {
            AtomicSwitchFallbackV1::Committed => "c",
            AtomicSwitchFallbackV1::FallbackToFull => "f",
            AtomicSwitchFallbackV1::Rejected(_) => "r",
        };
        assert_eq!(label, "f");

        let label = match rejected {
            AtomicSwitchFallbackV1::Committed => "c",
            AtomicSwitchFallbackV1::FallbackToFull => "f",
            AtomicSwitchFallbackV1::Rejected(_) => "r",
        };
        assert_eq!(label, "r");
    }

    #[test]
    fn joint_feasible_path_id_is_opaque_newtype() {
        let id = JointFeasiblePathIdV1::new(NonZeroU64::new(42).expect("non-zero"));
        let id2 = JointFeasiblePathIdV1::new(NonZeroU64::new(42).expect("non-zero"));
        let id3 = JointFeasiblePathIdV1::new(NonZeroU64::new(43).expect("non-zero"));

        assert_eq!(id, id2);
        assert_ne!(id, id3);
    }

    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn checked_successor_always_produces_valid_transition(seq in 0u64..u64::MAX) {
                let e = epoch(7);
                let source = StampV1::new(seq, e);
                let destination = source.checked_successor()
                    .expect("seq < u64::MAX guarantees successor");

                let transition = StampTransitionV1::try_new(source, destination)
                    .expect("checked successor must always produce valid transition");

                prop_assert_eq!(transition.source().sequence(), seq);
                prop_assert_eq!(transition.destination().sequence(), seq + 1);
            }

            #[test]
            fn non_adjacent_destination_rejected(
                seq in 0u64..u64::MAX,
                offset in 2u64..=100u64,
            ) {
                let e = epoch(3);
                let source = StampV1::new(seq, e);
                let dest_seq = seq.wrapping_add(offset);
                let destination = StampV1::new(dest_seq, e);

                // Only test when wrapping didn't accidentally produce seq+1
                if dest_seq != seq.wrapping_add(1) || offset != 1 {
                    let result = StampTransitionV1::try_new(source, destination);
                    prop_assert!(result.is_err(), "non-adjacent must be rejected");
                }
            }
        }
    }
}