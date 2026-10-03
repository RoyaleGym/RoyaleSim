//! movement.CAST_HOLD_HEADING: whether a unit held by its own ability's cast counts its heading in its neighbours'
//! avoidance scans (state.rs `phase_path16402_for`, `cast_heading_off`; move16402.rs `avoidance_scan`).
//!
//! THE READING (client 15.535.29): sp-sk-souls-own-s0 t311, a Skeleton walking past the Skeleton King turned the full 190
//! on his cast's first frame (behaviour state 10), the offset decaying 10 a tick through the cast, as a dynamic
//! blocker's does; the Dark Prince the Hero Dark Prince's cast puts down beside him the same on its first frame (5 of 5).
//!
//! The scene: a Blue Archer Queen at (3500, 11200) and a Blue Knight 1,200 behind her, both walking north up the left
//! lane; her button is pressed. Pinned: the Knight's avoidance offset over the cast's first ticks; under
//! client15535_not_counted it turns, under counted she is a walker heading its way and it does not.
//!
//! PLANT (regression): cast_heading_counted -> `a_walker_turns_at_a_cast_beside_it` red.
//!   RUSTFLAGS='--cfg clash_plant="cast_heading_counted"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   cast_hold_heading
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, CastHoldHeading};
use royalesim::Team;

const DECK: [&str; 8] = ["ArcherQueen", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// (the Knight's largest |avoidance offset| over the 3 ticks before the press, the same over the 6 after it).
fn scene(arm: CastHoldHeading) -> (i32, i32) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.cast_hold_heading = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let aq = s.scenario_spawn_now(Team::Blue, "ArcherQueen", n(3500, 11200), None).expect("the Archer Queen");
    let k = s.scenario_spawn_now(Team::Blue, "Knight", n(3500, 10000), None).expect("the Knight");
    let off = |s: &BattleState| s.entity(k).expect("the Knight").avoid_offset.abs();
    let mut before = 0;
    for _ in 0..3 {
        s.tick();
        before = before.max(off(&s));
    }
    assert!(s.entity(aq).is_some(), "the scene drifted: the Archer Queen is gone");
    s.press_ability_button(Team::Blue, 0).expect("the press is taken");
    let mut after = 0;
    for _ in 0..6 {
        s.tick();
        after = after.max(off(&s));
    }
    (before, after)
}

/// Plant: cast_heading_counted.
#[test]
fn a_walker_turns_at_a_cast_beside_it() {
    let (before, after) = scene(CastHoldHeading::Client15535NotCounted);
    assert_eq!(before, 0, "the scene drifted: the Knight turned before the press");
    assert!(after >= 150, "client15535_not_counted: the Knight did not turn at her cast ({after})");
}

#[test]
fn the_old_arm_passes_her_by() {
    assert_eq!(scene(CastHoldHeading::Counted), (0, 0), "counted: the Knight turned (vacuous otherwise)");
}

#[test]
fn the_shipped_arm_counts_the_heading() {
    assert_eq!(Calib::shipped().cast_hold_heading, CastHoldHeading::Counted);
}
