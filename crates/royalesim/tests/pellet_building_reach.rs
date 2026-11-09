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
use royalesim::{EntityId, Team};

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

/// Both clients reach a building's square (client 15.535.29: 136 of 136 pellet ends; client 16.402: 35 of 35). Clients 16.402
/// and 16.635 ship client16635_sparks_too since parity r92 (client16402_every_straight_shot from r80), which squares the
/// pellets as client15535_rounded_square does (below) and a Firecracker's sparks too (tests/spark_building_square.rs); client
/// 15.535.29 keeps client15535_rounded_square (CLIENT15535_ARMS).
#[test]
fn the_shipped_arm_reaches_the_square() {
    assert_eq!(Calib::shipped().straight_shot_building_reach, StraightShotBuildingReach::Client16635SparksToo);
    assert_eq!(config().calib.straight_shot_building_reach, StraightShotBuildingReach::Client16635SparksToo);
}

/// Red's princess tower on (14500, 25500), its square of half-side 1000 (its radius) and the circle of 250 + 1000 a straight shot
/// tests it with. The Magic Archer's line x - y = MA_LINE passes its centre 1265 out (outside the circle's 1250) and its corner
/// (15500, 24500) 149 inside, at 45 degrees, so the arrow's equal x and y steps stay on it; MA_AT stands 8800 along before the
/// line's nearest point to the centre, its Red Knight 6000 along from it.
const MA_LINE: i32 = -9211;
const MA_AT: (i32, i32) = (9172, 18383);
const MA_KNIGHT: (i32, i32) = (9172 + 4243, 18383 + 4243);

/// The tower's hitpoints on each of the 200 ticks after a Blue Magic Archer is put down on MA_AT with a Red Knight held on
/// MA_KNIGHT, both held.
fn tower_hp_ma(arm: StraightShotBuildingReach) -> Vec<i32> {
    assert_eq!((MA_AT.0 - MA_AT.1, MA_KNIGHT.0 - MA_KNIGHT.1), (MA_LINE, MA_LINE), "the scene: both on the line");
    let mut cfg = config();
    cfg.calib.straight_shot_building_reach = arm;
    let mut s = BattleState::new(15, cfg);
    let tower = s.tower_ids(Team::Red).iter().flatten().copied().find(|t| s.entity(*t).is_some_and(|e| e.pos == n(14500, 25500))).expect("Red's princess tower on (14500, 25500)");
    let ma = s.scenario_spawn_now(Team::Blue, "EliteArcher", n(MA_AT.0, MA_AT.1), None).expect("the Magic Archer");
    let kn: EntityId = s.scenario_spawn_now(Team::Red, "Knight", n(MA_KNIGHT.0, MA_KNIGHT.1), None).expect("the Knight");
    let mut hps = Vec::new();
    let mut fired = false;
    for _ in 0..200 {
        assert!(s.debug_set_pos(ma, n(MA_AT.0, MA_AT.1)) && s.debug_set_hp(ma, 10_000), "the Magic Archer held");
        let full = s.entity(kn).expect("the Knight").max_hp;
        assert!(s.debug_set_pos(kn, n(MA_KNIGHT.0, MA_KNIGHT.1)) && s.debug_set_hp(kn, full), "the Knight held");
        s.tick();
        fired |= s.projectiles().iter().any(|q| q.firer == Some(ma));
        hps.push(s.entity(tower).map_or(0, |e| e.hp));
    }
    assert!(fired, "the scene drifted: the Magic Archer never fired");
    hps
}

/// combat.STRAIGHT_SHOT_BUILDING_REACH = client16402_every_straight_shot (parity r80, live13: 6 of 6 Magic Archer arrows that
/// passed a princess tower inside its square's reach and outside its circle hit it). Plant: straight_square_pellets_only.
#[test]
fn under_client16402_every_straight_shot_a_magic_archer_arrow_reaches_a_towers_square() {
    let new = tower_hp_ma(StraightShotBuildingReach::Client16402EveryStraightShot);
    assert!(new.iter().any(|h| *h < new[0]), "client16402_every_straight_shot: no arrow reached the tower: {new:?}");
    // NOT VACUOUS: the shipped arm tests the arrow against the circle, which the line passes outside
    let old = tower_hp_ma(StraightShotBuildingReach::Client15535RoundedSquare);
    assert!(old.iter().all(|h| *h == old[0]), "client15535_rounded_square: the circle reached it: {old:?}");
}

/// The new arm still squares the Hunter's pellets as client15535_rounded_square does.
#[test]
fn under_client16402_every_straight_shot_the_pellet_still_reaches_the_square() {
    assert_eq!(tower_hp(StraightShotBuildingReach::Client16402EveryStraightShot), tower_hp(StraightShotBuildingReach::Client15535RoundedSquare));
}
