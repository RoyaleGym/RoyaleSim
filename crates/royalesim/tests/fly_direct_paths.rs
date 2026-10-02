//! A FLYDIRECTPATHS FLYER AIMS AT ITS GOAL'S POSITION (calibration movement.FLY_DIRECT_PATHS; state.rs
//! `phase_path16402_for`, the step's aim; CardDef::fly_direct_paths). The Skeleton Barrel's row sets FlyDirectPaths.
//! Measured on client 15.535.29: the barrels of sp-form-SkeletonBalloon-evo-s0 took their target's centre on 482 of
//! 484 ticks where it parts from their route's next cell; every other flyer took the cell.
//!
//! The scene, the measured one: Blue's Skeleton Barrel on (15500, 3500) heads for the red right princess tower
//! (14500, 25500); its route's cell centre is (14750, 24750). Under read its facing on each flying tick is the direction
//! to the tower's centre from its start-of-tick point, truncated to 1/256; under not_read it is the cell's, x two to
//! three units smaller on the first ticks.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! fly_direct_paths`): fly_direct_paths_ignored -> `a_skeleton_barrel_faces_the_towers_centre_on_every_tick_it_flies` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, FlyDirectPaths};
use royalesim::Team;

const DECK: [&str; 8] = ["SkeletonBalloon", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const TOWER: (i64, i64) = (14500, 25500);

/// The barrel's facing on each tick it flies, with the direction from its start-of-tick point to the tower's centre.
fn flight(arm: FlyDirectPaths) -> Vec<((i32, i32), (i32, i32))> {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), DECK.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.fly_direct_paths = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let b = s.scenario_spawn_now(Team::Blue, "SkeletonBalloon", Vec2::new(15500 * K, 3500 * K), None).expect("the barrel");
    let mut out = Vec::new();
    for _ in 0..120 {
        let before = s.entity(b).expect("the barrel").pos;
        s.tick();
        let e = s.entity(b).expect("the barrel flies");
        if e.pos == before {
            continue;
        }
        let (dx, dy) = (TOWER.0 - i64::from(before.x / K), TOWER.1 - i64::from(before.y / K));
        let len = isqrt(dx * dx + dy * dy).max(1);
        out.push(((e.facing.x, e.facing.y), ((256 * dx / len) as i32, (256 * dy / len) as i32)));
    }
    out
}

#[test]
fn a_skeleton_barrel_faces_the_towers_centre_on_every_tick_it_flies() {
    let f = flight(FlyDirectPaths::Read);
    assert!(f.len() >= 40, "the scene drifted: the barrel flew {} ticks", f.len());
    for (k, (got, want)) in f.iter().enumerate() {
        assert!((got.0 - want.0).abs() <= 1 && (got.1 - want.1).abs() <= 1, "flying tick {k}: facing {got:?}, the tower's centre gives {want:?}");
    }
}

#[test]
fn not_read_flies_the_barrel_at_its_cell() {
    // the control: the old arm's barrel faces its route's cell, two or more units off the tower's x on the first ticks
    let f = flight(FlyDirectPaths::NotRead);
    assert!(f.len() >= 40, "the scene drifted: the barrel flew {} ticks", f.len());
    assert!(f.iter().take(10).all(|(got, want)| (got.0 - want.0).abs() >= 2), "the cell and the tower's centre do not part here: {:?}", &f[..10]);
}
