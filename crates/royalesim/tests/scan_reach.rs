//! targeting.SCAN_REACH: how far a sight scan's broad phase reaches (target.rs `scan_with`, `query`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, 120 takes of an enemy crown tower at a start-of-tick centre
//! distance past the engine's query (SightRange + 2000 + the largest live radius, the King's 1400) and within the
//! crown-tower sight (SightRange + 2000 + both radii); sp-form-RoyalHogs-evo-s0 t1190, a Royal Hog took Red's left
//! princess tower at 13,007 (query 12,900, sight 13,100).
//!
//! THE SCENE: a Blue Knight (SightRange 5500, radius 500) walks up the left lane from (3500, 14000) toward Red's left
//! princess tower (3500, 25500), radius 1000, with nothing else on the board: its crown-tower sight is 5500 + 2000 + 500 +
//! 1000 = 9000, the engine's query 5500 + 2000 + 1400 = 8900. The first tick it holds a target, and its centre distance
//! to the tower at that tick's start (the scan reads the board before the move).
//!
//! WHAT IS PINNED, and the plant that turns it red (scan_reach_without_own_radius):
//!   1. client15535_plus_own_radius: the Knight takes the tower at a start-of-tick distance in (8900, 9000]; under
//!      sight_extra_max_radius (the engine's, the vacuity check) it takes it only at or inside 8900.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, ScanReach};
use royalesim::Team;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Knight's first target, and the start-of-tick centre distance (native) at which it took it.
fn first_take(arm: ScanReach) -> (Vec2, i32) {
    let mut cfg = config();
    cfg.calib.scan_reach = arm;
    let mut s = BattleState::new(5, cfg);
    let k = s.scenario_spawn_now(Team::Blue, "Knight", at(3500, 14000), None).expect("the Knight");
    let tower = at(3500, 25500);
    for _ in 0..400 {
        let before = s.entity(k).expect("the Knight").pos;
        s.tick();
        let kv = s.entity(k).expect("the Knight");
        if let Some(t) = kv.target {
            let tp = s.entity(t).expect("its target").pos;
            return (tp, before.dist(tower) / K);
        }
    }
    panic!("the scene drifted: the Knight took no target");
}

/// Plant: scan_reach_without_own_radius.
#[test]
fn a_scan_sees_a_crown_tower_from_its_crown_sight_under_client15535_plus_own_radius() {
    let (tp, d) = first_take(ScanReach::Client15535PlusOwnRadius);
    assert_eq!(tp, at(3500, 25500), "the scene drifted: the Knight's first target is not Red's left princess tower");
    assert!(d > 8900 && d <= 9000, "client15535_plus_own_radius: the Knight took the tower at {d}, not in (8900, 9000]");
    // NOT VACUOUS: the engine's query took it only inside 8900.
    let (tp, d) = first_take(ScanReach::SightExtraMaxRadius);
    assert_eq!(tp, at(3500, 25500), "the scene drifted (sight_extra_max_radius)");
    assert!(d <= 8900, "sight_extra_max_radius: the Knight took the tower at {d}, past the engine's query");
}

/// The shipped arm since 2026-10-03 (Sim's ruling on form patch 180): the own-radius reach on both clients.
#[test]
fn the_shipped_arm_reaches_its_own_radius() {
    assert_eq!(Calib::shipped().scan_reach, ScanReach::Client15535PlusOwnRadius);
}
