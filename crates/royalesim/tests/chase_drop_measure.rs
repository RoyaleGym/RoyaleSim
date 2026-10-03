//! targeting.CHASE_DROP_MEASURE: what the chase-drop limit of targeting.CHASE_DROP_RANGE = client_sight_minus_1000 is
//! measured on (target.rs `chase_measure`, `past_chase_limit`).
//!
//! THE READING (client 15.535.29, every scenario and IL truth): of the walking troop targets that crossed max(|dx|, |dy|)
//! of the limit while their holder walked, 41 of 42 whose |dy| crossed it were let go on that tick, and 19 of 19 whose
//! |dx| alone crossed it were kept (retarget-knight t153: a Skeleton kept a Knight 5,523 across, 3,013 up, against its
//! 5,500).
//!
//! The scene: a Blue Knight holding a Red Knight 1,000 across and 3,000 up, both put back each tick; on the sixth tick
//! the Red one stands 5,700 across (500 up), or 5,700 up (500 across), past the Blue Knight's limit of 5,500 and inside
//! its round sight. Pinned: whether the Blue Knight still holds it, under each arm (targeting.CHASE_DROP_WALKING_AWAY at
//! the shipped any_target, so the edge lets go of any held troop past the limit).
//!
//! PLANT (regression): chase_measure_max_abs -> `a_target_past_the_limit_across_alone_is_kept` red.
//!   RUSTFLAGS='--cfg clash_plant="chase_measure_max_abs"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   chase_drop_measure
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, ChaseDropMeasure, ChaseDropRange, ChaseDropWalkingAway};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Whether the Blue Knight at (6000, 8000) still holds the Red Knight after the tick that finds it `last` (native, from
/// the Blue one) having held it at (1000, 3000) for five ticks.
fn holds(measure: ChaseDropMeasure, last: (i32, i32)) -> bool {
    let mut cfg = config();
    cfg.calib.chase_drop_range = ChaseDropRange::ClientSightMinus1000;
    cfg.calib.chase_drop_walking_away = ChaseDropWalkingAway::AnyTarget;
    cfg.calib.chase_drop_measure = measure;
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    let at = n(6000, 8000);
    let blue = s.scenario_spawn_now(Team::Blue, "Knight", at, None).expect("the Blue Knight");
    let red = s.scenario_spawn_now(Team::Red, "Knight", n(7000, 11000), None).expect("the Red Knight");
    for _ in 0..5 {
        assert!(s.debug_set_pos(blue, at) && s.debug_set_pos(red, n(7000, 11000)), "the scene drifted: a Knight is gone");
        s.tick();
    }
    assert_eq!(s.entity(blue).unwrap().target, Some(red), "the scene drifted: the Blue Knight does not hold the Red one");
    assert!(s.debug_set_pos(blue, at) && s.debug_set_pos(red, n(6000 + last.0, 8000 + last.1)), "the scene drifted: a Knight is gone");
    s.tick();
    let b = s.entity(blue).expect("the Blue Knight stands");
    assert_eq!(b.attack_phase, royalesim::entity::AttackPhase::Idle, "the scene drifted: the Blue Knight is not walking");
    b.target == Some(red)
}

/// Plant: chase_measure_max_abs.
#[test]
fn a_target_past_the_limit_across_alone_is_kept() {
    assert!(holds(ChaseDropMeasure::Client15535LaneDy, (5700, 500)), "client15535_lane_dy: let go 5,700 across, 500 up");
}

#[test]
fn the_old_arm_lets_it_go() {
    assert!(!holds(ChaseDropMeasure::MaxAbs, (5700, 500)), "max_abs: kept 5,700 across (the control is vacuous)");
}

#[test]
fn both_arms_let_a_target_past_the_limit_up_the_lane_go() {
    for m in [ChaseDropMeasure::MaxAbs, ChaseDropMeasure::Client15535LaneDy] {
        assert!(!holds(m, (500, 5700)), "{m:?}: kept 5,700 up the lane");
    }
}

#[test]
fn the_shipped_arm_is_max_abs() {
    assert_eq!(Calib::shipped().chase_drop_measure, ChaseDropMeasure::MaxAbs);
}
