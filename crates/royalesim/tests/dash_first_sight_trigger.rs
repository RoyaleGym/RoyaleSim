//! combat.DASH_FIRST_SIGHT_TRIGGER, read off the engine: the trigger tick of a dasher (combat.DASH_ATTACK =
//! client_dash) that first sees its target already inside DashMaxRange + the target's radius (state.rs
//! `phase_path16402`, the dash block's first-sight branch).
//!
//! THE LAW, measured on client 15.535.29: take F, the first tick the dasher's target pointer names its target. Of 12
//! dashers whose target was inside the trigger distance on F (and not nearer than DashMinRange edge to edge), the 2
//! Mega Knights moved on F + 17 and the 10 Bandits on F + 16. The 16 dashers that walked into their trigger distance
//! moved on T + 17 and T + 16, T the first tick within it. So F is the trigger. Today's engine (next_tick) triggers
//! on F + 1: the Mega Knight moves on F + 18 and the Bandit on F + 17.
//!
//! A unit set down by the scenario has its target on tick 0 (F = 0 here). WHAT IS PINNED, each with its
//! precondition:
//!   1. first_sight_tick: a Mega Knight set down 4,805 from a Knight (inside 5,000 + 500, an edge of 3,555 over
//!      DashMinRange 3,500) targets the Knight on tick 0 and first moves on tick 17, and that move is the jump;
//!   2. first_sight_tick: a Bandit set down 6,231 from a Giant standing at its tower (inside 6,000 + 750, an edge of
//!      4,881) targets it on tick 0, stands, and first moves on tick 16, a dash half-step pair;
//!   3. next_tick: the same scenes first move on ticks 18 and 17 (today's engine);
//!   4. both values: a Mega Knight that walks into its trigger distance of that Giant (set down 6,061 from it) jumps
//!      on the first tick within it + 17: the key touches no walking trigger;
//!   5. the shipped value is next_tick.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test dash_first_sight_trigger`):
//!   * `dash_first_sight_next_tick` -- first_sight_tick still triggers on the tick after first sight: (1) and (2) go
//!     red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DashAttack, DashFirstSightTrigger};
use royalesim::Team;

fn with(arm: DashFirstSightTrigger) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.dash_attack = DashAttack::ClientDash;
    cfg.calib.dash_first_sight_trigger = arm;
    cfg
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Native distance between two subtile positions, truncated.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// One tick of a scene: (tick index, the dasher's move, the start-of-tick centre distance, whether the dasher
/// targets the other unit after the tick).
type Row = (u32, i64, i64, bool);

/// A red `target` card at `them` and a blue `dasher` at `me`, run for `ticks` ticks or until either dies.
fn scene(arm: DashFirstSightTrigger, dasher: &str, me: (i32, i32), target: &str, them: (i32, i32), ticks: u32) -> Vec<Row> {
    let mut s = BattleState::new(0, with(arm));
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, target, at(them), None), (Team::Blue, dasher, at(me), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (k, a) = (ids[0], ids[1]);
    let mut rows = Vec::new();
    for t in 0..ticks {
        let (Some(av), Some(kv)) = (s.entity(a), s.entity(k)) else { break };
        let (a0, k0) = (av.pos, kv.pos);
        s.tick();
        let Some(av) = s.entity(a) else { break };
        rows.push((t, dist(a0, av.pos), dist(a0, k0), av.target == Some(k)));
    }
    rows
}

/// The Mega Knight put down inside its trigger (the sweep's scene, as tests/dash_attack.rs sets it).
const MK_AT: (i32, i32) = (9500, 11500);
const KNIGHT_AT: (i32, i32) = (13126, 14653);
/// A Giant standing hitting the blue right princess tower, and a Bandit set down inside its trigger of it.
const GIANT_AT: (i32, i32) = (14731, 9439);
const BANDIT_AT: (i32, i32) = (8500, 9500);
/// A Mega Knight that walks into its trigger of that Giant.
const MK_WALK_AT: (i32, i32) = (9500, 12500);

fn first_move(rows: &[Row]) -> &Row {
    rows.iter().find(|r| r.1 > 0).expect("the dasher never moved")
}

fn preconditions(rows: &[Row], reach: i64, near: i64) {
    let r0 = rows[0];
    assert!(r0.3, "the scene drifted: the dasher has no target after tick 0");
    assert!(r0.2 <= reach, "the scene drifted: first sight at {} is outside the trigger {}", r0.2, reach);
    assert!(r0.2 > near, "the scene drifted: first sight at {} is inside DashMinRange's centre distance {}", r0.2, near);
}

/// Plant: dash_first_sight_next_tick.
#[test]
fn a_mega_knight_put_down_inside_its_trigger_jumps_on_the_first_sight_plus_17() {
    let rows = scene(DashFirstSightTrigger::FirstSightTick, "MegaKnight", MK_AT, "Knight", KNIGHT_AT, 40);
    preconditions(&rows, 5000 + 500, 3500 + 750 + 500);
    let m = first_move(&rows);
    assert_eq!(m.0, 17, "the Mega Knight first moved on tick {}, not on the first sight (0) + 17", m.0);
    assert!(m.1 > 200, "its first move is not the jump: {}", m.1);
}

/// Plant: dash_first_sight_next_tick.
#[test]
fn a_bandit_put_down_inside_its_trigger_dashes_on_the_first_sight_plus_16() {
    let rows = scene(DashFirstSightTrigger::FirstSightTick, "Assassin", BANDIT_AT, "Giant", GIANT_AT, 40);
    preconditions(&rows, 6000 + 750, 3500 + 600 + 750);
    let m = first_move(&rows);
    assert_eq!(m.0, 16, "the Bandit first moved on tick {}, not on the first sight (0) + 16", m.0);
    assert!(m.1 > 400, "its first move is not a dash: {}", m.1);
}

#[test]
fn next_tick_is_todays_engine() {
    let mk = scene(DashFirstSightTrigger::NextTick, "MegaKnight", MK_AT, "Knight", KNIGHT_AT, 40);
    preconditions(&mk, 5000 + 500, 3500 + 750 + 500);
    assert_eq!(first_move(&mk).0, 18, "next_tick: the Mega Knight's first move");
    let b = scene(DashFirstSightTrigger::NextTick, "Assassin", BANDIT_AT, "Giant", GIANT_AT, 40);
    preconditions(&b, 6000 + 750, 3500 + 600 + 750);
    assert_eq!(first_move(&b).0, 17, "next_tick: the Bandit's first move");
}

#[test]
fn a_walking_trigger_is_the_same_under_both_values() {
    for arm in [DashFirstSightTrigger::NextTick, DashFirstSightTrigger::FirstSightTick] {
        let rows = scene(arm, "MegaKnight", MK_WALK_AT, "Giant", GIANT_AT, 60);
        assert!(rows[0].2 > 5000 + 750, "the scene drifted: the Mega Knight started inside its trigger ({})", rows[0].2);
        let trig = rows.iter().find(|r| r.2 <= 5000 + 750).expect("the Mega Knight never came within its trigger");
        let jump = rows.iter().find(|r| r.1 > 200).expect("the Mega Knight never jumped");
        assert_eq!(jump.0, trig.0 + 17, "{arm:?}: the jump began on {}, not on the trigger {} + 17", jump.0, trig.0);
    }
}

#[test]
fn the_shipped_value_is_next_tick() {
    assert_eq!(config().calib.dash_first_sight_trigger, DashFirstSightTrigger::NextTick, "shipped combat.DASH_FIRST_SIGHT_TRIGGER");
}
