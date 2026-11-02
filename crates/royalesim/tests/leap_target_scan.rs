//! targeting.LEAP_TARGET_SCAN (target.rs `decide`): whether a troop in its river leap scans for a target.
//!
//! THE LAW, measured on client 16.402 (parity r80, the r76 census re-checked on 0.1.42, live13): over 343 river leaps (Hog
//! Rider 210, Royal Hog 89, Dark Prince 20, Prince 15, Ram 9), no Target phase whose start-of-tick state was the leap (J + 1
//! through the landing tick L) took, switched or dropped a live target; the first scan came on L + 1. The engine's `scans`
//! took one mid-leap on 85 of them.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test leap_target_scan`):
//!   * `leap_scans_mid_leap` -- the new arm still scans in the air: the client16402_held_mid_leap test red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, LeapTargetScan};
use royalesim::{EntityId, Team};

/// The Hog's spot: the river's centre column, so it leaps (tests/jump_landing_contact.rs).
const HOG_AT: (i32, i32) = (9500, 12500);
/// Where the Red Cannon deploys, out of the Hog's sight, and where it is held once the Hog is in the air: past the river, in
/// the Hog's sight from the leap. Red's princess towers are taken down first: a Hog sees a crown tower 2000 farther than its
/// sight (EXTRA_SIGHT_RANGE_TO_CROWN_TOWERS), which from the river would give it one before its leap.
const CANNON_PARK: (i32, i32) = (16500, 30500);
const CANNON_AT: (i32, i32) = (9500, 19500);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Under targeting.LEAP_TARGET_SCAN = `arm`: Red's princess towers down, a Red Cannon deployed out of sight, a Blue Hog Rider put
/// down at HOG_AT; from the tick after its leap began the Cannon held at CANNON_AT. The Cannon and, after each tick from then,
/// (jumping, target).
fn hog_rows(arm: LeapTargetScan) -> (EntityId, Vec<(bool, Option<EntityId>)>) {
    let mut cfg = config();
    cfg.calib.leap_target_scan = arm;
    let mut s = BattleState::new(3, cfg);
    for t in s.tower_ids(Team::Red).iter().skip(1).flatten().copied().collect::<Vec<_>>() {
        assert!(s.debug_set_hp(t, 0), "a Red princess tower down");
    }
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", at(CANNON_PARK), None).expect("the Cannon");
    for _ in 0..30 {
        s.tick();
    }
    let hog = s.scenario_spawn_now(Team::Blue, "HogRider", at(HOG_AT), None).expect("the Hog");
    let mut n = 0;
    while !s.entity(hog).expect("the Hog").jumping {
        assert_eq!(s.entity(hog).unwrap().target, None, "the scene drifted: the Hog took a target before its leap");
        s.tick();
        n += 1;
        assert!(n < 80, "the scene drifted: the Hog never leapt");
    }
    let mut rows = Vec::new();
    for _ in 0..60 {
        let max = s.entity(cannon).expect("the Cannon").max_hp;
        assert!(s.debug_set_pos(cannon, at(CANNON_AT)) && s.debug_set_hp(cannon, max));
        s.tick();
        let h = s.entity(hog).expect("the Hog");
        rows.push((h.jumping, h.target));
    }
    (cannon, rows)
}

/// targeting.LEAP_TARGET_SCAN = client16402_held_mid_leap (parity r80, live13: 0 of 343 leaps took a target mid-leap; the first
/// scan on L + 1). Plant: leap_scans_mid_leap.
#[test]
fn under_client16402_held_mid_leap_a_leaping_hog_takes_its_building_on_the_tick_after_landing() {
    let (cannon, rows) = hog_rows(LeapTargetScan::Client16402HeldMidLeap);
    let l = rows.iter().position(|r| !r.0).expect("the Hog never landed");
    assert!(l >= 2, "the scene drifted: the leap ended {l} ticks after the Cannon came");
    assert!(rows[..=l].iter().all(|r| r.1 != Some(cannon)), "a target in the air or on the landing tick: {:?}", &rows[..=l]);
    assert_eq!(rows[l + 1].1, Some(cannon), "its first scan on L + 1: {:?}", &rows[..l + 3]);
    // NOT VACUOUS: scans takes the Cannon while it is in the air
    let (c0, old) = hog_rows(LeapTargetScan::Scans);
    assert!(old.iter().any(|r| r.0 && r.1 == Some(c0)), "scans: no target in the air: {old:?}");
}
