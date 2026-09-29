//! THE FISHERMAN'S FIRST HIT AFTER A DRAG LANDS ON THE TICK HIS VICTIM IS RELEASED -- calibration combat.SPECIAL_HOOK
//! (client_hook_drag), state.rs `step_hook_drags` and `prime_after_release`.
//!
//! THE LAW, which combat.SPECIAL_HOOK's provenance states ("his first melee hit lands on the release tick"): let S be the
//! stop tick, the first tick after the drag's last 510 step, on which the victim is still held. The victim is free from
//! S + 1, and the thrower's first ordinary hit lands on S + 1, with no fresh load: his attack progress stands at
//! HitSpeed less one tick (1,250 of 1,300) from the throw to the release. Measured on both clients, on all 8 drags that
//! end in his reach, 9 seat readings: the Fisherman scenes on client 15.535.29 (mech-fisherman-far, -near and -knight,
//! both seats, 6 drags) and the 16.402 corpus's 20260920-081819 (2 drags: the Musketeer's, read in both seats, and the
//! Giant's first, read in seat B). The engine, before the round 9 fix, left the special with a fresh cycle, whose entry
//! credit put the first hit on S + 2 in all 9.
//!
//! THE SCENE: a Blue Fisherman at (9000, 11000) and a Red Knight at (9000, 17500), out of every crown tower's reach at the
//! start (the scene tests/test_fisherman_hook.py pins the drag on).
//!
//! WHAT IS PINNED:
//!   1. the scene: one unbroken run of drag steps (505 to 515 a tick), and no hp lost by the Knight before S;
//!   2. the Knight's first hp drop after the drag is on S + 1, the release tick.
//!
//! PLANT (regression):
//!   * `hook_release_fresh_cycle` -- the special ends with a fresh cycle: (2) goes red (the first hit on S + 2), and so
//!     does tests/hook_buildings.rs's pull, which ends through the same priming.
//!     RUSTFLAGS='--cfg clash_plant="hook_release_fresh_cycle"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!     --test hook_first_strike
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, SpecialHook};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Per tick after the tick: (the Knight's step, native, its hp).
fn scene() -> Vec<(i64, i32)> {
    let mut cfg = config();
    assert_eq!(cfg.calib.special_hook, SpecialHook::ClientHookDrag, "the shipped combat.SPECIAL_HOOK");
    cfg.calib.special_hook = SpecialHook::ClientHookDrag;
    let mut s = BattleState::new(0, cfg);
    s.scenario_spawn_now(Team::Blue, "Fisherman", at((9000, 11000)), None).expect("the Fisherman");
    let k = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 17500)), None).expect("the Knight");
    let mut rows = Vec::new();
    for _ in 0..140 {
        let Some(a) = s.entity(k).map(|v| v.pos) else { break };
        s.tick();
        let Some(v) = s.entity(k) else { break };
        let (dx, dy) = ((v.pos.x / K - a.x / K) as i64, (v.pos.y / K - a.y / K) as i64);
        rows.push((isqrt(dx * dx + dy * dy), v.hp));
    }
    rows
}

/// S, the first row after the drag's last step.
fn stop_tick(rows: &[(i64, i32)]) -> usize {
    let steps: Vec<usize> = (0..rows.len()).filter(|&i| (505..=515).contains(&rows[i].0)).collect();
    assert!(!steps.is_empty(), "the scene drifted: no drag");
    assert_eq!(steps.last().unwrap() - steps[0] + 1, steps.len(), "the scene drifted: the drag is not one run of ticks");
    steps.last().unwrap() + 1
}

/// Plant: hook_release_fresh_cycle.
#[test]
fn the_first_hit_lands_on_the_release_tick() {
    let rows = scene();
    let s = stop_tick(&rows);
    let full = rows[0].1;
    assert!(rows[..=s].iter().all(|r| r.1 == full), "the scene drifted: the Knight lost hp by the stop tick S = {s}");
    let first = (s + 1..rows.len()).find(|&i| rows[i].1 < rows[i - 1].1).expect("the Fisherman never hit the Knight");
    assert_eq!(first - s, 1, "the first hit after the drag, as S + n (S = {s})");
}
