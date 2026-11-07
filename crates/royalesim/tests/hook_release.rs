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
//! PLANT (regression, parity r85): hook_drag_subtile_step ->
//!   `under_client16402_native_axes_a_diagonal_drag_steps_on_the_native_centres_truncated` red.
//!   RUSTFLAGS='--cfg clash_plant="hook_release_next_tick"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   hook_release
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, HookDragRounding, HookDragSpeed, HookRelease, HookWaterLanding, SpecialHook};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The first tick after the stop tick S on which the victim acts (a Knight: its attack under way; a Giant: a step), as
/// S + n.
fn acts_after_stop(arm: HookRelease, victim: &str) -> usize {
    let mut cfg = config();
    // combat.HOOK_DRAG_SPEED = constant_510: this scene was read on the 510 drag (parity r88 ships client16402_victim_speed,
    // under which a victim's step is its Speed x 850 / 100).
    cfg.calib.hook_drag_speed = HookDragSpeed::Constant510;
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
fn the_shipped_arm_idles_it_a_tick() {
    assert_eq!(Calib::shipped().hook_release, HookRelease::ClientIdleTick);
}

type WaterStop = (usize, (i32, i32), (i32, i32), (bool, bool));

/// A Blue Fisherman at (9000, 13600) hooks a Red Giant walking in from (9000, 20500), under combat.HOOK_WATER_LANDING = `arm`
/// (HOOK_RELEASE = client_idle_tick): (S, the stop tick; the Giant's point on S and on S + 1, native; whether each is off water).
fn water_stop(arm: HookWaterLanding) -> WaterStop {
    let mut cfg = config();
    // combat.HOOK_DRAG_SPEED = constant_510: this scene was read on the 510 drag (parity r88 ships client16402_victim_speed,
    // under which a victim's step is its Speed x 850 / 100).
    cfg.calib.hook_drag_speed = HookDragSpeed::Constant510;
    cfg.calib.special_hook = SpecialHook::ClientHookDrag;
    cfg.calib.hook_release = HookRelease::ClientIdleTick;
    cfg.calib.hook_water_landing = arm;
    let mut s = BattleState::new(0, cfg);
    s.scenario_spawn_now(Team::Blue, "Fisherman", at((9000, 13600)), None).expect("the Fisherman");
    let v = s.scenario_spawn_now(Team::Red, "Giant", at((9000, 20500)), None).expect("the Giant");
    let mut rows: Vec<(i64, (i32, i32))> = Vec::new();
    for _ in 0..200 {
        let Some(a) = s.entity(v).map(|e| e.pos) else { break };
        s.tick();
        let Some(e) = s.entity(v) else { break };
        let (dx, dy) = ((e.pos.x / K - a.x / K) as i64, (e.pos.y / K - a.y / K) as i64);
        rows.push((isqrt(dx * dx + dy * dy), (e.pos.x / K, e.pos.y / K)));
    }
    let steps: Vec<usize> = (0..rows.len()).filter(|&i| (505..=515).contains(&rows[i].0)).collect();
    assert!(!steps.is_empty(), "{arm:?}: the scene drifted: no drag of the Giant");
    let stop = steps.last().unwrap() + 1;
    let valid = |p: (i32, i32)| s.arena().is_valid_position(at(p));
    (stop, rows[stop].1, rows[stop + 1].1, (valid(rows[stop].1), valid(rows[stop + 1].1)))
}

/// combat.HOOK_WATER_LANDING = client16402_nearest_land_lattice (client 16.402, parity r84, live14: 3 of 3 drags ending on the river
/// set on the guard's land lattice on the stop + 1). Plant: hook_victim_left_in_water.
#[test]
fn under_client16402_nearest_land_lattice_a_victim_dragged_onto_the_river_stands_on_land_on_its_idle_tick() {
    let (s_old, stop_old, idle_old, (on_land, _)) = water_stop(HookWaterLanding::InWater);
    assert!(!on_land, "the scene drifted: the drag stopped on land at {stop_old:?}");
    let (s_new, stop_new, idle_new, (_, idle_on_land)) = water_stop(HookWaterLanding::Client16402NearestLandLattice);
    assert_eq!((s_old, stop_old), (s_new, stop_new), "the same drag under both arms");
    // NOT VACUOUS: in_water leaves the Giant on the river on S + 1
    assert_eq!(idle_old, stop_old, "in_water: the Giant moved on its idle tick");
    let d = (idle_new.0 - stop_new.0, idle_new.1 - stop_new.1);
    assert!(d.0.abs() % 500 == 250 && d.1.abs() % 500 == 250, "client16402_nearest_land_lattice: an odd multiple of 250 on each axis: {d:?}");
    assert!(idle_on_land, "client16402_nearest_land_lattice: still on water at {idle_new:?}");
}

/// Since parity r84 (in_water before it; client 15.535.29 keeps in_water).
#[test]
fn the_shipped_hook_water_landing_is_client16402_nearest_land_lattice() {
    assert_eq!(Calib::shipped().hook_water_landing, HookWaterLanding::Client16402NearestLandLattice);
}

/// One full drag tick of `drag_steps`: (the Giant's native point before it, the Fisherman's before it, the Giant's after it).
type DragStep = ((i64, i64), (i64, i64), (i64, i64));

/// A Blue Fisherman at (9000, 11000) hooks a Red Giant walking in from (11500, 17500) (a diagonal drag), under
/// combat.HOOK_DRAG_ROUNDING = `arm` (HOOK_RELEASE = client_idle_tick): every full drag tick's (the Giant's native point
/// before it, the Fisherman's before it, the Giant's after it).
fn drag_steps(arm: HookDragRounding) -> Vec<DragStep> {
    let mut cfg = config();
    // combat.HOOK_DRAG_SPEED = constant_510: this scene was read on the 510 drag (parity r88 ships client16402_victim_speed,
    // under which a victim's step is its Speed x 850 / 100).
    cfg.calib.hook_drag_speed = HookDragSpeed::Constant510;
    cfg.calib.special_hook = SpecialHook::ClientHookDrag;
    cfg.calib.hook_release = HookRelease::ClientIdleTick;
    cfg.calib.hook_drag_rounding = arm;
    let mut s = BattleState::new(0, cfg);
    let f = s.scenario_spawn_now(Team::Blue, "Fisherman", at((9000, 11000)), None).expect("the Fisherman");
    let v = s.scenario_spawn_now(Team::Red, "Giant", at((11500, 17500)), None).expect("the Giant");
    let nat = |p: Vec2| ((p.x / K) as i64, (p.y / K) as i64);
    let mut rows = Vec::new();
    for _ in 0..200 {
        let (Some(a), Some(b)) = (s.entity(v).map(|e| e.pos), s.entity(f).map(|e| e.pos)) else { break };
        s.tick();
        let Some(e) = s.entity(v) else { break };
        let (p, q, n) = (nat(a), nat(b), nat(e.pos));
        let (dx, dy) = (n.0 - p.0, n.1 - p.1);
        if (505..=515).contains(&isqrt(dx * dx + dy * dy)) {
            rows.push((p, q, n));
        }
    }
    rows
}

/// combat.HOOK_DRAG_ROUNDING = client16402_native_axes (client 16.402, parity r85, live14: 47 of 47 drags' step counts and 38 of
/// 47 ends within 2 native re-run under the native-axes step, the subtile step two short). Plant: hook_drag_subtile_step.
#[test]
fn under_client16402_native_axes_a_diagonal_drag_steps_on_the_native_centres_truncated() {
    let model = |p: (i64, i64), f: (i64, i64)| {
        let (dx, dy) = (f.0 - p.0, f.1 - p.1);
        let l = isqrt(dx * dx + dy * dy);
        (p.0 + dx * 510 / l, p.1 + dy * 510 / l)
    };
    let new = drag_steps(HookDragRounding::Client16402NativeAxes);
    assert!(new.len() >= 5, "the scene drifted: under 5 drag steps: {new:?}");
    let (p0, f0, _) = new[0];
    assert!((f0.0 - p0.0).abs() >= 1000 && (f0.1 - p0.1).abs() >= 1000, "the scene drifted: the drag is not diagonal: {new:?}");
    assert!(new.iter().all(|(p, f, a)| model(*p, *f) == *a), "client16402_native_axes: each step the truncated native one: {new:?}");
    // NOT VACUOUS: the subtile step's native shadow is not the truncated native step on a diagonal
    let old = drag_steps(HookDragRounding::Subtile);
    assert!(old.iter().any(|(p, f, a)| model(*p, *f) != *a), "subtile: every step was the truncated native one: {old:?}");
}

/// A Blue Fisherman at (9000, 11000) hooks a Red `victim` put down at (9000, 17500) under combat.HOOK_DRAG_SPEED = `arm`: the
/// victim's step lengths, native, over 200 ticks.
fn victim_steps(arm: HookDragSpeed, victim: &str) -> Vec<i64> {
    let mut cfg = config();
    cfg.calib.special_hook = SpecialHook::ClientHookDrag;
    cfg.calib.hook_release = HookRelease::ClientIdleTick;
    cfg.calib.hook_drag_speed = arm;
    let mut s = BattleState::new(0, cfg);
    s.scenario_spawn_now(Team::Blue, "Fisherman", at((9000, 11000)), None).expect("the Fisherman");
    let v = s.scenario_spawn_now(Team::Red, victim, at((9000, 17500)), None).expect("the victim");
    let mut out = Vec::new();
    for _ in 0..200 {
        let Some(a) = s.entity(v).map(|e| e.pos) else { break };
        s.tick();
        let Some(e) = s.entity(v) else { break };
        let (dx, dy) = ((e.pos.x / K - a.x / K) as i64, (e.pos.y / K - a.y / K) as i64);
        out.push(isqrt(dx * dx + dy * dy));
    }
    out
}

/// combat.HOOK_DRAG_SPEED = client16402_victim_speed (parity r88, the oracle's replays of 100 battles on client 16.402: the drag
/// step is the victim's Speed x 850 / 100, a Royal Hog's (120) 1010 to 1020 in 17 of 21 steps, a Recruit's (60) 510): a Hog
/// Rider (Speed 120) is dragged 1020 a tick, where constant_510 drags it 510; a Knight (60) is dragged 510 under both. Plant:
/// hook_drag_constant.
#[test]
fn under_client16402_victim_speed_the_drag_step_is_the_victims_speed_times_850_over_100() {
    let fast = victim_steps(HookDragSpeed::Client16402VictimSpeed, "HogRider");
    assert!(fast.iter().filter(|&&d| (1015..=1025).contains(&d)).count() >= 2, "client16402_victim_speed: a Hog Rider's drag is not 1020 a step: {fast:?}");
    // NOT VACUOUS: the old arm drags the same Hog Rider 510 a step
    let old = victim_steps(HookDragSpeed::Constant510, "HogRider");
    assert!(old.iter().filter(|&&d| (505..=515).contains(&d)).count() >= 2, "the scene drifted: constant_510 drags no Hog Rider 510: {old:?}");
    assert!(!old.iter().any(|&d| (1015..=1025).contains(&d)), "constant_510: a 1020 step: {old:?}");
    let knight = victim_steps(HookDragSpeed::Client16402VictimSpeed, "Knight");
    assert!(knight.iter().filter(|&&d| (505..=515).contains(&d)).count() >= 2, "client16402_victim_speed: a Knight's drag is not 510 a step: {knight:?}");
}

/// combat.HOOK_DRAG_SPEED = client16402_victim_speed_floor_60 (parity r88: the 16.402 corpus's 20260920-081819 drags a Giant,
/// Speed 45, 510 a step, where Speed x 850 / 100 is 382): a Giant is dragged 510 a tick, a Hog Rider 1020; under
/// client16402_victim_speed the Giant is dragged at its own Speed (382). Plant: hook_drag_floor_ignored.
#[test]
fn under_client16402_victim_speed_floor_60_a_slow_victim_is_dragged_510() {
    let giant = victim_steps(HookDragSpeed::Client16402VictimSpeedFloor60, "Giant");
    assert!(giant.iter().filter(|&&d| (505..=515).contains(&d)).count() >= 2, "client16402_victim_speed_floor_60: a Giant's drag is not 510 a step: {giant:?}");
    let fast = victim_steps(HookDragSpeed::Client16402VictimSpeedFloor60, "HogRider");
    assert!(fast.iter().filter(|&&d| (1015..=1025).contains(&d)).count() >= 2, "client16402_victim_speed_floor_60: a Hog Rider's drag is not 1020 a step: {fast:?}");
    // NOT VACUOUS: the unfloored arm drags the Giant at its own Speed
    let raw = victim_steps(HookDragSpeed::Client16402VictimSpeed, "Giant");
    assert!(raw.iter().filter(|&&d| (375..=390).contains(&d)).count() >= 2, "the scene drifted: client16402_victim_speed drags no Giant at 382: {raw:?}");
}

/// Since parity r88 (constant_510 before it; client 15.535.29 keeps constant_510).
#[test]
fn the_shipped_hook_drag_speed_is_client16402_victim_speed_floor_60() {
    assert_eq!(Calib::shipped().hook_drag_speed, HookDragSpeed::Client16402VictimSpeedFloor60);
}

#[test]
fn the_shipped_drag_step_is_subtile() {
    assert_eq!(Calib::shipped().hook_drag_rounding, HookDragRounding::Subtile);
}
