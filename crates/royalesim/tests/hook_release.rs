//! combat.HOOK_RELEASE: the tick a hook's victim is free after its drag stops (state.rs `step_hook_drags`, entity.rs
//! `drag_idle`).
//!
//! THE READING (both clients, drag_end_census.py): let S be the stop tick, the first tick after the drag's last 510 step,
//! on which the victim is still held. Every drag in the captures, 12 of 12, idles on S + 1 (state 0, no step) and acts
//! from S + 2: client 15.535.29's Fisherman scenes (Knights attacking him from S + 2) and the 16.402 corpus's
//! 20260920-081819 (a Musketeer attacking, a Giant walking from S + 2). The engine freed the victim on S + 1.
//!
//! THE SCENE (tests/hook_first_strike.rs's): a Blue Fisherman at (9000, 11000) and a Red `victim` at (9000, 17500). Pinned:
//! the first tick after S on which the victim acts, as S + n: a Knight's attack under way, a Giant's first step.
//!
//! PLANT (regression): hook_release_next_tick -> `the_victim_idles_the_tick_after_the_stop` red.
//!   RUSTFLAGS='--cfg clash_plant="hook_release_next_tick"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   hook_release
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, HookRelease, SpecialHook};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The first tick after the stop tick S on which the victim acts (a Knight: its attack under way; a Giant: a step), as
/// S + n.
fn acts_after_stop(arm: HookRelease, victim: &str) -> usize {
    let mut cfg = config();
    cfg.calib.special_hook = SpecialHook::ClientHookDrag;
    cfg.calib.hook_release = arm;
    let mut s = BattleState::new(0, cfg);
    s.scenario_spawn_now(Team::Blue, "Fisherman", at((9000, 11000)), None).expect("the Fisherman");
    let v = s.scenario_spawn_now(Team::Red, victim, at((9000, 17500)), None).expect("the victim");
    // Per tick after the tick: (its step, native; whether its attack is under way).
    let mut rows: Vec<(i64, bool)> = Vec::new();
    for _ in 0..160 {
        let Some(a) = s.entity(v).map(|e| e.pos) else { break };
        s.tick();
        let Some(e) = s.entity(v) else { break };
        let (dx, dy) = ((e.pos.x / K - a.x / K) as i64, (e.pos.y / K - a.y / K) as i64);
        rows.push((isqrt(dx * dx + dy * dy), e.attack_phase != AttackPhase::Idle));
    }
    let steps: Vec<usize> = (0..rows.len()).filter(|&i| (505..=515).contains(&rows[i].0)).collect();
    assert!(!steps.is_empty(), "the scene drifted: no drag of the {victim}");
    assert_eq!(steps.last().unwrap() - steps[0] + 1, steps.len(), "the scene drifted: the drag is not one run of ticks");
    let stop = steps.last().unwrap() + 1;
    assert_eq!(rows[stop], (0, false), "the scene drifted: the {victim} acted on the stop tick");
    let acts = |r: &(i64, bool)| if victim == "Giant" { r.0 > 0 } else { r.1 };
    let first = (stop + 1..rows.len()).find(|&i| acts(&rows[i])).unwrap_or_else(|| panic!("the scene drifted: the {victim} never acted"));
    first - stop
}

/// Plant: hook_release_next_tick.
#[test]
fn the_victim_idles_the_tick_after_the_stop() {
    assert_eq!(acts_after_stop(HookRelease::ClientIdleTick, "Knight"), 2, "client_idle_tick: the Knight's attack, as S + n");
    assert_eq!(acts_after_stop(HookRelease::ClientIdleTick, "Giant"), 2, "client_idle_tick: the Giant's first step, as S + n");
}

#[test]
fn the_old_arm_frees_it_the_tick_after_the_stop() {
    assert_eq!(acts_after_stop(HookRelease::FreeNextTick, "Knight"), 1, "free_next_tick: the Knight's attack, as S + n");
    assert_eq!(acts_after_stop(HookRelease::FreeNextTick, "Giant"), 1, "free_next_tick: the Giant's first step, as S + n");
}

#[test]
fn the_shipped_arm_frees_it_the_next_tick() {
    assert_eq!(Calib::shipped().hook_release, HookRelease::FreeNextTick);
}
