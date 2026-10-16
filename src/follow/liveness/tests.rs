use std::time::Duration;

use proptest::prelude::*;

use super::*;

const NANO: Duration = Duration::from_nanos(1);
const TEN_SECONDS: Duration = Duration::from_secs(10);
const FIVE_MINUTES: Duration = Duration::from_secs(300);

#[test]
fn live_until_10_seconds() {
    assert_eq!(Liveness::since(Duration::ZERO), Liveness::Live);
    assert_eq!(Liveness::since(TEN_SECONDS - NANO), Liveness::Live);
}

#[test]
fn quiet_from_10_seconds_until_5_minutes() {
    assert_eq!(Liveness::since(TEN_SECONDS), Liveness::Quiet);
    assert_eq!(Liveness::since(FIVE_MINUTES - NANO), Liveness::Quiet);
}

#[test]
fn idle_from_5_minutes_on() {
    assert_eq!(Liveness::since(FIVE_MINUTES), Liveness::Idle);
    assert_eq!(Liveness::since(Duration::MAX), Liveness::Idle);
}

proptest! {
    /// The liveness holds for exactly as long as `changes_in` says, then changes.
    #[test]
    fn changes_in_is_the_time_to_the_next_change(nanos in 0u64..400_000_000_000) {
        let elapsed = Duration::from_nanos(nanos);
        let now = Liveness::since(elapsed);
        match Liveness::changes_in(elapsed) {
            Some(left) => {
                prop_assert!(left > Duration::ZERO);
                prop_assert_eq!(Liveness::since(elapsed + left - NANO), now);
                prop_assert_ne!(Liveness::since(elapsed + left), now);
            }
            None => prop_assert_eq!(now, Liveness::Idle),
        }
    }
}
