//! combat.LOAD_TIMER_TARGET_LOSS: what a unit's load timer does after the unit loses, while walking, the target it
//! walked to (combat.rs `attack_step_progress`, entity.rs `load_hold`).
//!
//! THE READING (client 15.535.29): every walking loss followed by ticks with no target, 4 of 4, kept the timer from the
//! tick after the loss until the next target (sp-f2-ice-s0 t275: a Valkyrie's stood at 1,150 for 11 ticks, so its next
//! swing landed 10 ticks after the engine's); losses out of an attack and units never holding a target ran on.
//!
//! The scene: a Blue Valkyrie kills a Red Skeleton beside it (its fire sets the timer to LoadTime, 1,400) and takes a
//! Red Knight 5,000 off; while she walks to it the Red Knight is killed, with nothing else in sight. Pinned: her load
//! timer over the five ticks after the loss, standing under client15535_stands_after_walk_loss, falling 250 under
//! runs_on.
//!
//! PLANT (regression): load_hold_unread -> `the_timer_stands_after_a_walking_loss` red.
//!   RUSTFLAGS='--cfg clash_plant="load_hold_unread"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   load_timer_target_loss
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, LoadTimerTargetLoss};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// (the Valkyrie's load timer on the loss tick, the same five ticks later).
fn scene(arm: LoadTimerTargetLoss) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.load_timer_target_loss = arm;
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    let blue = s.scenario_spawn_now(Team::Blue, "Valkyrie", n(9000, 8000), None).expect("the Valkyrie");
    let skel = s.scenario_spawn_now(Team::Red, "Skeleton", n(9000, 9200), None).expect("the Skeleton");
    let red = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 13000), None).expect("the Red Knight");
    let mut waited = 0;
    while s.entity(skel).is_some() || s.entity(blue).unwrap().target != Some(red) {
        assert!(waited < 80, "the scene drifted: the Valkyrie did not kill the Skeleton and take the Red Knight");
        s.tick();
        waited += 1;
    }
    let b = s.entity(blue).unwrap();
    assert_eq!(b.attack_phase, AttackPhase::Idle, "the scene drifted: the Valkyrie is not walking to the Red one");
    assert!(b.attack_load_ms >= 400, "the scene drifted: the Valkyrie's timer is down to {}", b.attack_load_ms);
    assert!(s.debug_set_hp(red, 0), "could not kill the Red Knight");
    while s.entity(blue).unwrap().target.is_some() {
        assert!(waited < 100, "the scene drifted: the Valkyrie kept a target");
        s.tick();
        waited += 1;
    }
    let lost = s.entity(blue).unwrap().attack_load_ms;
    for _ in 0..5 {
        s.tick();
        let b = s.entity(blue).unwrap();
        assert!(b.target.is_none() && b.attack_phase == AttackPhase::Idle, "the scene drifted: the Valkyrie took a target");
    }
    (lost, s.entity(blue).unwrap().attack_load_ms)
}

/// Plant: load_hold_unread.
#[test]
fn the_timer_stands_after_a_walking_loss() {
    let (lost, after) = scene(LoadTimerTargetLoss::Client15535StandsAfterWalkLoss);
    assert!(lost >= 300, "the scene drifted: the timer read {lost} on the loss tick");
    assert_eq!(after, lost, "client15535_stands_after_walk_loss: the timer ran on");
}

#[test]
fn the_old_arm_runs_it_on() {
    let (lost, after) = scene(LoadTimerTargetLoss::RunsOn);
    assert_eq!(after, lost - 250, "runs_on: the timer stood (vacuous otherwise)");
}

#[test]
fn the_shipped_arm_runs_on() {
    assert_eq!(Calib::shipped().load_timer_target_loss, LoadTimerTargetLoss::RunsOn);
}
