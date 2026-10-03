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
//! client15535_stands_while_held (client 15.535.29: 10-tick stuns 33 of 33, 22-tick freezes 39 of 39 held the timer;
//! every unheld loss ran on): the scene's Valkyrie, walking to the Red Knight, is Zapped; her timer stands through the
//! stun under the new arm and falls 50 a tick under runs_on; the Red Knight carried off alive (no hold) runs it on.
//!
//! PLANT (regression): load_hold_unread -> `the_timer_stands_after_a_walking_loss` red.
//!   RUSTFLAGS='--cfg clash_plant="load_hold_unread"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   load_timer_target_loss
//! PLANT (regression): held_load_runs_on -> `a_stunned_unit_keeps_its_load_timer` red.
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
    lose(arm, false)
}

/// `scene` with the Red Knight carried 12,000 off alive instead of killed: (the timer on the loss tick, five ticks later).
fn live_scene(arm: LoadTimerTargetLoss) -> (i32, i32) {
    lose(arm, true)
}

fn lose(arm: LoadTimerTargetLoss, alive: bool) -> (i32, i32) {
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
    if alive {
        assert!(s.debug_set_pos(red, n(16500, 25000)), "could not carry the Red Knight off");
    } else {
        assert!(s.debug_set_hp(red, 0), "could not kill the Red Knight");
    }
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

/// Under `arm`: the Valkyrie walking to the Red Knight with her timer above 0 is Zapped by Red; her load timer on the
/// tick the stun is first seen on her (L) and on each of the 5 ticks after, with the stun still on her on each.
fn zapped(arm: LoadTimerTargetLoss) -> Vec<i32> {
    let mut cfg = config();
    cfg.calib.load_timer_target_loss = arm;
    cfg.decks = [vec!["Valkyrie".into()], vec!["Zap".into(), "Knight".into()]];
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
    assert!(s.entity(blue).unwrap().attack_load_ms >= 600, "the scene drifted: the timer is down to {}", s.entity(blue).unwrap().attack_load_ms);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    let at = s.entity(blue).unwrap().pos;
    s.deploy(Team::Red, "Zap", at).expect("the Zap");
    while s.entity(blue).unwrap().stun_ms == 0 {
        assert!(waited < 140, "the scene drifted: the Zap never stunned the Valkyrie");
        s.tick();
        waited += 1;
    }
    let mut out = vec![s.entity(blue).unwrap().attack_load_ms];
    for _ in 0..5 {
        s.tick();
        let b = s.entity(blue).unwrap();
        assert!(b.stun_ms > 0, "the scene drifted: the stun ended within 5 ticks");
        out.push(b.attack_load_ms);
    }
    out
}

/// Plant: held_load_runs_on.
#[test]
fn a_stunned_unit_keeps_its_load_timer() {
    let l = zapped(LoadTimerTargetLoss::Client15535StandsWhileHeld);
    assert!(l[0] >= 300, "the scene drifted: the timer read {} on the stun's first tick", l[0]);
    assert!(l.iter().all(|x| *x == l[0]), "client15535_stands_while_held: the timer moved through the stun: {l:?}");
}

#[test]
fn the_old_arm_runs_a_stunned_units_timer_down() {
    let l = zapped(LoadTimerTargetLoss::RunsOn);
    assert_eq!(l[5], l[0] - 250, "runs_on: the timer stood through the stun (vacuous otherwise): {l:?}");
}

#[test]
fn an_unheld_live_loss_runs_it_on_under_the_held_arm() {
    let (lost, after) = live_scene(LoadTimerTargetLoss::Client15535StandsWhileHeld);
    assert!(lost >= 300, "the scene drifted: the timer read {lost} on the loss tick");
    assert_eq!(after, lost - 250, "client15535_stands_while_held: the timer stood after a loss with no hold");
}

#[test]
fn a_dead_loss_runs_it_on_under_the_held_arm() {
    let (lost, after) = scene(LoadTimerTargetLoss::Client15535StandsWhileHeld);
    assert!(lost >= 300, "the scene drifted: the timer read {lost} on the loss tick");
    assert_eq!(after, lost - 250, "client15535_stands_while_held: the timer stood after the target died");
}

#[test]
fn the_shipped_arm_runs_on() {
    assert_eq!(Calib::shipped().load_timer_target_loss, LoadTimerTargetLoss::RunsOn);
}

/// The blue Knight's load timer on each tick of the ladder a red Fireball knocks it on, cast `d` ticks after it has fought
/// a red Knight 60 ticks (its last swing's timer running), under client15535_stands_while_held: (the timer before the
/// ladder, its values through the ladder).
fn ladder_timer(d: u32) -> Option<(i32, Vec<i32>)> {
    use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
    let mut cfg = config();
    cfg.calib.load_timer_target_loss = royalesim::state::LoadTimerTargetLoss::Client15535StandsWhileHeld;
    cfg.decks = [vec!["Knight".into(), "Zap".into()], vec!["Knight".into(), "Fireball".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = royalesim::state::BattleState::new(3, cfg);
    past_deploy_lockout(&mut s);
    let n = |x: i32, y: i32| Vec2::new(x * K, y * K);
    let ids = s
        .scenario_spawn_batch(&[(royalesim::Team::Blue, "Knight", n(9000, 12000), None), (royalesim::Team::Red, "Knight", n(9000, 13300), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let blue = ids[0];
    for _ in 0..(60 + d) {
        s.tick();
    }
    let at = s.entity(blue)?.pos;
    s.spawn_unit(royalesim::Team::Red, "Fireball", at, None).expect("the Fireball");
    let mut rows: Vec<(Vec2, i32)> = vec![(at, s.entity(blue)?.attack_load_ms)];
    for _ in 0..60 {
        s.tick();
        let k = s.entity(blue)?;
        rows.push((k.pos, k.attack_load_ms));
    }
    let step = |j: usize| {
        let (a, b) = (rows[j - 1].0, rows[j].0);
        let (dx, dy) = (i64::from((b.x - a.x) / K), i64::from((b.y - a.y) / K));
        isqrt(dx * dx + dy * dy)
    };
    let j = (1..rows.len()).find(|&j| step(j) >= 100)?;
    let mut k = j;
    while k + 1 < rows.len() && (15..=35).contains(&(step(k) - step(k + 1))) {
        k += 1;
    }
    Some((rows[j - 1].1, (j..=k).map(|m| rows[m].1).collect()))
}

/// Plant: ladder_load_stands. A knockback ladder is no hold: the timer runs down through it (client 15.535.29: 108 of 112
/// ladders over a running timer).
#[test]
fn a_knockback_ladder_runs_the_load_timer_under_client15535_stands_while_held() {
    let (before, during) = (0..30)
        .filter_map(ladder_timer)
        .find(|(b, l)| *b > 100 && l.len() >= 4)
        .expect("the scene drifted: no Fireball cast in 30 ticks knocked the Knight with its timer running");
    assert!(during[0] < before && during[1] < during[0], "new: the timer stood through the ladder ({before} then {during:?})");
}

