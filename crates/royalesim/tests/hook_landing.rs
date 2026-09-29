//! A LANDING HOOK SETS ITS VICTIM ONTO ITS OWN POINT -- calibration combat.HOOK_LANDING, state.rs `apply_effects` (a
//! landed hook) and combat.rs `step_projectiles` (the hook's start-of-tick point).
//!
//! THE READING (client_hook_point, shipped at its old arm on_victim): on the tick the Fisherman's hook arrives, the
//! client leaves the hook where it stood at the start of that tick and sets the victim onto that point, over its walk;
//! the 510 drag steps start on the next tick, from there. Read off client 15.535.29's Fisherman scenes and the 16.402
//! corpus's 20260920-081819: the recorded hook stands still on the landing tick and the victim stands on it (far-s0: the
//! hook on (11572, 17340) on t161, 98 short of the Knight; on t162 it has not moved and the Knight stands on it). The
//! victim's landing move is 42 to 759 toward the thrower over the 8 drags (9 seat readings). Today's engine lands the hook on the victim
//! and leaves the victim where it walked, so its drag starts farther out: one and two extra steps in near-s1 and
//! near-s0.
//!
//! THE SCENE: a Blue Fisherman at (9000, 11000) and a Red Knight at (9000, 17500) (tests/hook_first_strike.rs's).
//!
//! WHAT IS PINNED (X: the landing tick, the tick before the drag's first 510 step):
//!   1. on_victim: the Knight's move on X is a walk (55 to 61), and the drag takes 10 steps;
//!   2. client_hook_point: the Knight's move on X is the hook's shortfall, 572 toward the Fisherman, and the drag takes
//!      9 steps, one fewer, as the client's near-s1 drag does against the engine's;
//!   3. the shipped value is client_hook_point (since the 2026-09-28 round 9 lanes flip).
//!
//! PLANT (regression):
//!   * `hook_lands_on_victim` -- the new arm leaves the victim where it walked: (2) goes red.
//!     RUSTFLAGS='--cfg clash_plant="hook_lands_on_victim"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!     --test hook_landing
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, HookLanding};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Per tick after the tick: the Knight's step, native.
fn steps(arm: HookLanding) -> Vec<i64> {
    let mut cfg = config();
    cfg.calib.hook_landing = arm;
    let mut s = BattleState::new(0, cfg);
    s.scenario_spawn_now(Team::Blue, "Fisherman", at((9000, 11000)), None).expect("the Fisherman");
    let k = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 17500)), None).expect("the Knight");
    let mut rows = Vec::new();
    for _ in 0..140 {
        let Some(a) = s.entity(k).map(|v| v.pos) else { break };
        s.tick();
        let Some(v) = s.entity(k) else { break };
        let (dx, dy) = ((v.pos.x / K - a.x / K) as i64, (v.pos.y / K - a.y / K) as i64);
        rows.push(isqrt(dx * dx + dy * dy));
    }
    rows
}

/// (the landing tick X's move, the number of drag steps).
fn landing(arm: HookLanding) -> (i64, usize) {
    let rows = steps(arm);
    let drag: Vec<usize> = (0..rows.len()).filter(|&i| (505..=515).contains(&rows[i])).collect();
    assert!(!drag.is_empty(), "{arm:?}: the scene drifted: no drag");
    assert_eq!(drag.last().unwrap() - drag[0] + 1, drag.len(), "{arm:?}: the scene drifted: the drag is not one run of ticks");
    (rows[drag[0] - 1], drag.len())
}

#[test]
fn the_old_arm_leaves_the_victim_where_it_walked() {
    let (x, n) = landing(HookLanding::OnVictim);
    assert!((55..=61).contains(&x), "on_victim: the landing tick's move {x} is not a walk");
    assert_eq!(n, 10, "on_victim: the drag steps");
}

/// Plant: hook_lands_on_victim.
#[test]
fn the_new_arm_sets_the_victim_onto_the_hook() {
    assert_eq!(landing(HookLanding::ClientHookPoint), (572, 9), "client_hook_point: (the landing move, the drag steps)");
}

#[test]
fn the_shipped_value_is_client_hook_point() {
    assert_eq!(Calib::shipped().hook_landing, HookLanding::ClientHookPoint);
}
