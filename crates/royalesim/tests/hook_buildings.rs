//! THE FISHERMAN HOOKS A CROWN TOWER AND PULLS HIMSELF TO IT -- calibration combat.HOOK_BUILDINGS, state.rs `hookable`,
//! `pulls_self`, `special_step`, `apply_effects` (a landed hook) and `step_hook_drags` (the pull).
//!
//! THE READING (client_pull_self, shipped at its old arm troops_only), off the 16.402 corpus's 20260920-081051 (both
//! seats), a Fisherman walking up the left lane at the enemy left princess tower (3500, 25500):
//!   - he stops on the first tick his centre distance is 8,000 or less (t823 8,049, t824 7,989): SpecialRange 7,000
//!     plus the TARGET's radius, the tower's 1,000 (his own 500 would have stopped him about 8 ticks earlier);
//!   - he loads 26 ticks (SpecialLoadTime 1,300) and throws on t851; the hook lands on t861;
//!   - he is pulled from t862, 449 or 450 a tick straight at the tower (DragSelfSpeed 450), and stands on t875
//!     1,699 from its centre (1,000 + 500 + DragMargin 200);
//!   - on t876 he stands with no target, and his first hit lands on the tower on t877 (every 26 ticks after).
//!
//! The old arm is today's engine: the special takes troops only, and he walks on to melee range.
//!
//! THE SCENE: a Blue Fisherman at (3500, 12000) on the left lane, no other troop; the Red left princess tower at
//! (3500, 25500) is his target.
//!
//! WHAT IS PINNED:
//!   1. client_pull_self: he stops walking with his centre 8,000 or less from the tower's and more than 7,940 (a walk
//!      step, 60, short of it), stands 26 ticks, is then pulled in steps of 449 to 450 (the last one shorter or equal)
//!      and stops 1,700 (within 1) from the tower's centre; the tower loses his first hit on the tick after the stop tick;
//!   2. troops_only (the old arm): nothing moves him faster than a walk, and he walks within melee range of the tower;
//!   3. the shipped value is troops_only.
//!
//! PLANTS (regression):
//!   * `hook_buildings_unread` -- the new arm hooks troops only: (1) goes red.
//!   * `hook_release_fresh_cycle` (tests/hook_first_strike.rs's) -- the pull ends with a fresh cycle: (1) goes red too,
//!     the tower's first hit landing on S + 2.
//!     RUSTFLAGS='--cfg clash_plant="hook_buildings_unread"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!     --test hook_buildings
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, HookBuildings};
use royalesim::Team;

const TOWER: (i32, i32) = (3500, 25500);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn dist(p: Vec2, q: (i32, i32)) -> i64 {
    let (dx, dy) = ((p.x / K - q.0) as i64, (p.y / K - q.1) as i64);
    isqrt(dx * dx + dy * dy)
}

/// Per tick after the tick: (the Fisherman's step, his centre distance from the tower's, the tower's hp).
fn scene(arm: HookBuildings) -> Vec<(i64, i64, i32)> {
    let mut cfg = config();
    cfg.calib.hook_buildings = arm;
    let mut s = BattleState::new(0, cfg);
    let f = s.scenario_spawn_now(Team::Blue, "Fisherman", at((3500, 12000)), None).expect("the Fisherman");
    let tower = s.entities().find(|v| v.team == Team::Red && v.pos == at(TOWER)).map(|v| v.id).expect("the Red left princess tower");
    let mut rows = Vec::new();
    for _ in 0..400 {
        let a = s.entity(f).expect("the scene drifted: the Fisherman died").pos;
        s.tick();
        let Some(v) = s.entity(f) else { break };
        let (dx, dy) = ((v.pos.x / K - a.x / K) as i64, (v.pos.y / K - a.y / K) as i64);
        rows.push((isqrt(dx * dx + dy * dy), dist(v.pos, TOWER), s.entity(tower).map_or(0, |t| t.hp)));
    }
    rows
}

/// Plant: hook_buildings_unread.
#[test]
fn the_new_arm_hooks_the_tower_and_pulls_him_to_it() {
    let rows = scene(HookBuildings::ClientPullSelf);
    let stop = rows.iter().position(|r| r.0 == 0).expect("client_pull_self: he never stopped walking");
    let d = rows[stop - 1].1;
    assert!(d <= 8000 && d > 7940, "client_pull_self: he stopped {d} from the tower's centre");
    let pull: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].0 > 300).collect();
    assert!(!pull.is_empty(), "client_pull_self: he was never pulled");
    // 20260920-081051: he stands from t825 and is first pulled on t862 (throw on t851, the hook landing on t861).
    assert_eq!(pull[0] - stop, 37, "client_pull_self: the pull's first step, ticks after the first standing tick");
    assert_eq!(pull.last().unwrap() - pull[0] + 1, pull.len(), "client_pull_self: the pull is not one run of ticks");
    assert!(pull[..pull.len() - 1].iter().all(|&i| (449..=450).contains(&rows[i].0)), "client_pull_self: the pull's steps {:?}", pull.iter().map(|&i| rows[i].0).collect::<Vec<_>>());
    let end = *pull.last().unwrap();
    assert!((rows[end].1 - 1700).abs() <= 1, "client_pull_self: he stopped {} from the tower's centre", rows[end].1);
    let s = end + 1;
    let first = (s + 1..rows.len()).find(|&i| rows[i].2 < rows[i - 1].2).expect("client_pull_self: he never hit the tower");
    assert_eq!(first - s, 1, "client_pull_self: his first hit, as S + n (S = {s}, the stop tick)");
}

#[test]
fn the_old_arm_walks_to_melee_range() {
    let rows = scene(HookBuildings::TroopsOnly);
    assert!(rows.iter().all(|r| r.0 <= 70), "troops_only: something moved him faster than a walk");
    let closest = rows.iter().map(|r| r.1).min().unwrap();
    assert!(closest <= 1000 + 500 + 1200, "troops_only: he never walked within melee range ({closest})");
}

#[test]
fn the_shipped_value_is_troops_only() {
    assert_eq!(Calib::shipped().hook_buildings, HookBuildings::TroopsOnly);
}
