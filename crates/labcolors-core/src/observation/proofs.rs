//! Полный машинный домен порядка ревизий у всех входов observation.
use super::*;

#[kani::proof]
fn revision_order_is_total_without_wraparound() {
    let initialized: bool = kani::any();
    let previous: u64 = kani::any();
    let incoming: u64 = kani::any();
    let current = initialized.then_some(Revision::new(previous));
    let actual = revision_is_replay(current, Revision::new(incoming));
    let expected = if !initialized || incoming > previous {
        Ok(false)
    } else if incoming == previous {
        Ok(true)
    } else {
        Err(ObservationError::RevisionOutOfOrder {
            current: Revision::new(previous),
            incoming: Revision::new(incoming),
        })
    };
    assert!(
        actual == expected,
        "all observation paths must preserve revision order without wraparound"
    );
    kani::cover!(!initialized && incoming == 0, "first zero revision");
    kani::cover!(initialized && incoming > previous, "new revision");
    kani::cover!(initialized && incoming == previous, "replay revision");
    kani::cover!(
        initialized && previous == u64::MAX && incoming == 0,
        "wrapped revision rejected"
    );
}
