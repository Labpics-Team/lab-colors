//! Законы состояния доказываются на произвольных payload-идентичностях.
//! Сквозной evaluator/arena/drop проверяется отдельно реальными историями Session.
use super::*;

fn state() -> SessionState<u64, u64, u64> {
    match kani::any::<u8>() % 7 {
        0 => SessionState::Waiting,
        1 => SessionState::Ready {
            current: kani::any(),
        },
        2 => SessionState::Stale {
            previous: kani::any(),
        },
        3 => SessionState::Failed {
            cause: kani::any(),
            previous: None,
        },
        4 => SessionState::Failed {
            cause: kani::any(),
            previous: Some(kani::any()),
        },
        5 => SessionState::Incomplete {
            cause: kani::any(),
            previous: None,
        },
        _ => SessionState::Incomplete {
            cause: kani::any(),
            previous: Some(kani::any()),
        },
    }
}

#[kani::proof]
fn current_evidence_never_promotes_historical_state() {
    let value = state();
    let current = value.current_verified().copied();
    let historical = value.last_verified().copied();
    let (expected_current, expected_history) = match value {
        SessionState::Waiting => (None, None),
        SessionState::Ready { current } => (Some(current), Some(current)),
        SessionState::Stale { previous } => (None, Some(previous)),
        SessionState::Failed { previous, .. } | SessionState::Incomplete { previous, .. } => {
            (None, previous)
        }
    };
    assert!(
        current == expected_current,
        "render authority must never promote historical evidence to current"
    );
    assert!(
        historical == expected_history,
        "last good evidence must remain distinct from current authority"
    );
    kani::cover!(current.is_some(), "ready evidence authorizes");
    kani::cover!(
        current.is_none() && historical.is_some(),
        "historical evidence does not authorize"
    );
    kani::cover!(historical.is_none(), "no evidence remains absent");
    kani::cover!(
        matches!(
            value,
            SessionState::Incomplete {
                previous: Some(_),
                ..
            }
        ),
        "incomplete retains only historical evidence"
    );
    kani::cover!(
        matches!(value, SessionState::Incomplete { previous: None, .. }),
        "incomplete has no invented evidence"
    );
}

#[kani::proof]
fn state_displacement_conserves_every_owned_payload() {
    let original = state();
    let expected = match original {
        SessionState::Waiting => (None, None, None),
        SessionState::Ready { current } => (Some(current), None, None),
        SessionState::Stale { previous } => (Some(previous), None, None),
        SessionState::Failed { cause, previous } => (previous, Some(cause), None),
        SessionState::Incomplete { cause, previous } => (previous, None, Some(cause)),
    };
    let mut remaining = original.clone();
    let displaced = displace_session_state(&mut remaining);
    assert!(
        matches!(remaining, SessionState::Waiting),
        "displacement must empty the old state before publication"
    );
    assert!(
        (
            displaced.last_verified,
            displaced.discarded_violation,
            displaced.discarded_incomplete,
        ) == expected,
        "displacement must retain exactly the old good violation and incomplete payloads"
    );
    kani::cover!(
        expected.0.is_some() && expected.1.is_some(),
        "failed state keeps two owned payloads"
    );
    kani::cover!(
        expected.0.is_none() && expected.1.is_some(),
        "first violation has no invented predecessor"
    );
    kani::cover!(
        expected == (None, None, None),
        "empty state has nothing to retire"
    );
    kani::cover!(
        expected.0.is_some() && expected.2.is_some(),
        "incomplete state keeps two owned payloads"
    );
    kani::cover!(
        expected.0.is_none() && expected.2.is_some(),
        "first incomplete has no invented predecessor"
    );
}
