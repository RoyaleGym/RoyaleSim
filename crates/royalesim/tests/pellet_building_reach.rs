//! combat.STRAIGHT_SHOT_BUILDING_REACH: the reach test a CheckCollisions shot (the Hunter's pellet) runs on a building
//! (combat.rs `straight_hits`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, every Hunter pellet that ended on a building reached the
//! building's square (half-side its radius) within its ProjectileRadius on the tick it was gone, 136 of 136; 97 of them
//! were outside the circle (sp-f4-hunter-s0 t689: (1001, 1277) from Red's princess tower).
//!
//! THE SCENE: a Blue Hunter held on (14725, 21019), the measured point, shooting at Red's princess tower (14500, 25500)
//! 4,486 off, its fan's side pellets passing the tower's square near its corners. The tower's hitpoints are read each tick
//! for 200 ticks, 0 once it falls, and the two arms compared tick by tick.
//!
//! WHAT IS PINNED, and the plant that turns it red (pellet_building_circle):
//!   1. client15535_rounded_square: on some tick the tower has lost more than under circle on the same tick (the engine's,
//!      where the side pellets fly on and land later, or miss).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, StraightShotBuildingReach};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The tower's hitpoints on each of the 200 ticks after the Hunter is put down, 0 once it falls (the volleys bring it
/// down inside the window).
fn tower_hp(arm: StraightShotBuildingReach) -> Vec<i32> {
    let mut cfg = config();
    cfg.calib.straight_shot_building_reach = arm;
    let mut s = BattleState::new(15, cfg);
    let tower = s.tower_ids(Team::Red).iter().flatten().copied().find(|t| s.entity(*t).is_some_and(|e| e.pos == n(14500, 25500))).expect("Red's princess tower on (14500, 25500)");
    let hunter = s.scenario_spawn_now(Team::Blue, "Hunter", n(14725, 21019), None).expect("the Hunter");
    let full = s.entity(tower).expect("the tower").hp;
    let mut hps = Vec::new();
    for _ in 0..200 {
        assert!(s.debug_set_pos(hunter, n(14725, 21019)) && s.debug_set_hp(hunter, 10_000), "the Hunter held");
        s.tick();
        hps.push(s.entity(tower).map_or(0, |e| e.hp));
    }
    assert!(hps.iter().any(|h| *h < full), "the scene drifted: no pellet reached the tower");
    hps
}

/// Plant: pellet_building_circle.
#[test]
fn a_pellet_reaches_a_buildings_square_under_client15535_rounded_square() {
    let square = tower_hp(StraightShotBuildingReach::Client15535RoundedSquare);
    let circle = tower_hp(StraightShotBuildingReach::Circle);
    assert!(
        square.iter().zip(&circle).any(|(a, b)| a < b),
        "client15535_rounded_square: the tower never stood lower than under circle\nsquare {square:?}\ncircle {circle:?}"
    );
}

/// Both clients reach a building's square (client 15.535.29: 136 of 136 pellet ends; client 16.402: 35 of 35).
#[test]
fn the_shipped_arm_reaches_the_square() {
    assert_eq!(Calib::shipped().straight_shot_building_reach, StraightShotBuildingReach::Client15535RoundedSquare);
}
