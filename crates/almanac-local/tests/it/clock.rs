//! `WallClock` is the only test that reads the wall clock: it answers something after 2020.

use almanac_core::UnixSeconds;
use almanac_local::WallClock;
use almanac_service::Clock;

#[test]
fn the_wall_clock_answers_a_recent_instant() {
    assert!(WallClock.now() > UnixSeconds(1_577_836_800));
}
