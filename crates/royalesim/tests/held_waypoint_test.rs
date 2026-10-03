//! movement.HELD_WAYPOINT_TEST: whether a unit held by a freeze or a stun, aiming at its next waypoint at speed 0
//! (movement.HELD_FACING = client15535_toward_waypoint), runs that waypoint's reached test on the point the contact law
//! moved it to (state.rs `phase_path16402_for`, `bookkeeping`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, every troop that stood two ticks in behaviour state 1
//! popped its next waypoint exactly when the reached test passed (120 of 120) and kept it exactly when it failed (3,111
//! of 3,111); sp-il-208a t1561, an Ice Golem frozen by an Ice Spirit and pushed by a Musketeer overlapping it popped the
//! waypoint the push carried it past.
//!
//! THE SCENE: a Blue Knight walks up the left lane from (3500, 9000) for 40 ticks; a Red Freeze on its point holds it;
//! a Blue Giant is set down 400 behind it on its line of walk, so the contact law pushes the held Knight forward along
//! its route (150 a tick, the cap) for the twelve held ticks read. Its next waypoint stood at most 1,500 ahead, so it
//! crosses the test's 1,000.
//!
//! WHAT IS PINNED, and the plant that turns it red (held_waypoint_test_skipped):
//!   1. client15535_run: the held Knight's route loses its next waypoint while held; not_run (the engine's, the
//!      vacuity check): it keeps every waypoint through the same push.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, HeldFacing, HeldWaypointTest};
use royalesim::Team;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The held Knight's route length when the Giant is set down, on each of the twelve held ticks after, and how far it
/// moved (native, along y) over them.
fn scene(arm: HeldWaypointTest) -> (usize, Vec<usize>, i32) {
    let mut cfg = config();
    cfg.calib.held_waypoint_test = arm;
    cfg.calib.held_facing = HeldFacing::Client15535TowardWaypoint;
    let mut s = BattleState::new(5, cfg);
    let k = s.scenario_spawn_now(Team::Blue, "Knight", at(3500, 9000), None).expect("the Knight");
    for _ in 0..40 {
        s.tick();
    }
    let p = s.entity(k).expect("the Knight").pos;
    s.spawn_unit(Team::Red, "Freeze", p, None).expect("cast Freeze");
    let mut waited = 0;
    while s.entity(k).expect("the Knight").stun_ms == 0 {
        s.tick();
        waited += 1;
        assert!(waited < 10, "the scene drifted: the Freeze never held the Knight");
    }
    let held = s.entity(k).expect("the Knight");
    let (before, start) = (held.route.len(), held.pos);
    assert!(before > 0, "the scene drifted: the held Knight has no route");
    s.scenario_spawn_now(Team::Blue, "Giant", Vec2::new(start.x, start.y - 400 * K), None).expect("the Giant");
    let mut lens = Vec::new();
    for _ in 0..12 {
        s.tick();
        let kv = s.entity(k).expect("the Knight");
        assert!(kv.stun_ms > 0, "the scene drifted: the hold ended inside the window");
        lens.push(kv.route.len());
    }
    let moved = (s.entity(k).expect("the Knight").pos.y - start.y) / K;
    (before, lens, moved)
}

/// Plant: held_waypoint_test_skipped.
#[test]
fn a_held_unit_pushed_past_its_waypoint_pops_it_under_client15535_run() {
    let (before, lens, moved) = scene(HeldWaypointTest::Client15535Run);
    assert!(moved >= 500, "the scene drifted: the held Knight was pushed only {moved}");
    assert!(lens.iter().any(|&l| l < before), "client15535_run: the held Knight kept its waypoint ({before} -> {lens:?})");
    // NOT VACUOUS: the same push under the engine's arm leaves the route whole.
    let (before, lens, moved) = scene(HeldWaypointTest::NotRun);
    assert!(moved >= 500, "the scene drifted: the held Knight was pushed only {moved} (not_run)");
    assert!(lens.iter().all(|&l| l == before), "not_run: the held Knight popped a waypoint while held ({before} -> {lens:?})");
}
