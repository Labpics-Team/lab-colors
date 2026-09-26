//! Полный u64-domain реального sink-stamp, включая границу исчерпания.
use super::*;

#[kani::proof]
fn mutation_stamp_preserves_epoch_and_cannot_wrap() {
    let sequence: u64 = kani::any();
    let epoch: u64 = kani::any();
    if let Some(epoch) = NonZeroU64::new(epoch) {
        let original = PointSinkStampV1::new(sequence, PointSinkBindingEpochV1::new(epoch));
        let mutation = PointSinkMutationStampV1::new(original);
        assert!(
            mutation.is_some() == (sequence != u64::MAX),
            "sink mutation must refuse exhausted sequence instead of wrapping"
        );
        if let Some(mutation) = mutation {
            assert!(
                mutation.expected() == original,
                "sink CAS must preserve the entire expected token"
            );
            assert!(
                mutation.desired().sequence() as u128 == sequence as u128 + 1
                    && mutation.desired().binding_epoch().value() == epoch,
                "sink successor must advance exactly once within the same epoch"
            );
        }
        kani::cover!(sequence == u64::MAX, "sink sequence exhaustion");
        kani::cover!(sequence == u64::MAX - 1, "last valid sink mutation");
        kani::cover!(
            epoch.get() == u64::MAX && sequence == 0,
            "full width binding epoch preserved"
        );
    }
}
