//! combat.DASH_PUSHBACK, read off the engine: what the Mega Knight's jump blow (combat.DASH_ATTACK = client_dash, a
//! dash with a DashRadius) does to the enemies it hits besides DashDamage (state.rs `land_dash_blows`).
//!
//! THE LAW, measured on client 15.535.29 (the Mega Knight's single-card scenario and its two jump scenarios) and the
//! 16.402 corpus (20260920-081819, the B seat): from the tick after the blow each Knight it hit slid 199 or 200, 174
//! or 175, 149 or 150, 124 or 125, 99 or 100, 74 or 75, 49 or 50, 24 or 25 (4 of 4, within 2), away from the Mega
//! Knight's position on the blow tick along the line to the Knight's centre (within 0.1 degree): the knockback ladder
//! of Pushback 1000 that spells and the Mega Knight's deploy blow already run. Two Giants (IgnorePushback) did not
//! move. Today's engine (not_read) leaves every victim where the blow found it.
//!
//! The scenes: a red Knight standing hitting the blue right princess tower at (14231, 9182), the Knight's spot in the
//! client scenario, and a blue Mega Knight set down at (9500, 6500) that walks, jumps and lands its blow on it; and a
//! red Giant at (14731, 9439) with the Mega Knight set down at (9500, 12500). WHAT IS PINNED, each with its
//! precondition (the Knight loses DashDamage at the scene's level on one tick, the blow tick B):
//!   1. client_ladder_from_landing: the Knight's moves on B + 1 .. B + 8 are the ladder above, within 2, and the first
//!      points away from the Mega Knight's position after B (within 1 degree);
//!   2. not_read: the Knight does not move on B + 1 (today's engine);
//!   3. both values: the Giant (IgnorePushback) does not move on B + 1 .. B + 8;
//!   4. the shipped value is not_read.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test dash_pushback`):
//!   * `dash_pushback_unapplied` -- client_ladder_from_landing pushes nobody: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DashAttack, DashPushback};
use royalesim::Team;

fn with(arm: DashPushback) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.dash_attack = DashAttack::ClientDash;
    cfg.calib.dash_pushback = arm;
    cfg
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Native position of a subtile vector.
fn native(p: Vec2) -> (i64, i64) {
    ((p.x / K) as i64, (p.y / K) as i64)
}

/// One tick: (the victim's hp loss, the victim's move, native; the Mega Knight's and the victim's positions after it).
type Row = (i32, i64, (i64, i64), (i64, i64));

fn scene(arm: DashPushback, victim: &str, them: (i32, i32), mk: (i32, i32), ticks: u32) -> (Vec<Row>, i32) {
    let mut s = BattleState::new(0, with(arm));
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, victim, at(them), None), (Team::Blue, "MegaKnight", at(mk), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (v, m) = (ids[0], ids[1]);
    let idx = s.cards().index("MegaKnight").expect("the Mega Knight is simulable");
    let dash = s.cards().get(idx).dash.expect("the Mega Knight carries a dash block");
    let blow = s.cards().scaled(idx, s.config().card_level[0], dash.damage).expect("a valid level");
    let mut rows = Vec::new();
    for _ in 0..ticks {
        let (Some(vv), Some(_)) = (s.entity(v), s.entity(m)) else { break };
        let (v0, hp0) = (native(vv.pos), vv.hp);
        s.tick();
        let (Some(vv), Some(mv)) = (s.entity(v), s.entity(m)) else { break };
        let p = native(vv.pos);
        let (dx, dy) = (p.0 - v0.0, p.1 - v0.1);
        rows.push((hp0 - vv.hp, isqrt(dx * dx + dy * dy), native(mv.pos), p));
    }
    (rows, blow)
}

fn blow_tick(rows: &[Row], blow: i32) -> usize {
    let hits: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r.0 == blow).map(|(i, _)| i).collect();
    assert_eq!(hits.len(), 1, "the scene drifted: the victim lost DashDamage {blow} on ticks {hits:?}");
    assert!(hits[0] + 9 < rows.len(), "the scene drifted: the run ended {} ticks after the blow", rows.len() - hits[0]);
    hits[0]
}

const KNIGHT_AT: (i32, i32) = (14231, 9182);
const MK_FOR_KNIGHT: (i32, i32) = (9500, 6500);
const GIANT_AT: (i32, i32) = (14731, 9439);
const MK_FOR_GIANT: (i32, i32) = (9500, 12500);
/// The knockback ladder of Pushback 1000, native, from the tick after the push lands.
const LADDER: [i64; 8] = [200, 175, 150, 125, 100, 75, 50, 25];

/// Plant: dash_pushback_unapplied.
#[test]
fn the_jump_blow_slides_the_knight_down_the_ladder_away_from_the_landing_point() {
    let (rows, blow) = scene(DashPushback::ClientLadderFromLanding, "Knight", KNIGHT_AT, MK_FOR_KNIGHT, 90);
    let b = blow_tick(&rows, blow);
    let steps: Vec<i64> = rows[b + 1..=b + 8].iter().map(|r| r.1).collect();
    for (s, l) in steps.iter().zip(LADDER) {
        assert!((s - l).abs() <= 2, "the Knight's moves after the blow on {b}: {steps:?}, not the ladder {LADDER:?}");
    }
    // the first move points away from the Mega Knight's position after the blow tick
    let (m, k0, k1) = (rows[b].2, rows[b].3, rows[b + 1].3);
    let (ax, ay) = (k0.0 - m.0, k0.1 - m.1);
    let (sx, sy) = (k1.0 - k0.0, k1.1 - k0.1);
    // within 1 degree of the line away from the Mega Knight: a move away (dot > 0) with |cross| <= tan(1 degree) *
    // dot, and tan(1 degree) = 0.01746 is under 18 / 1000
    let cross = (ax * sy - ay * sx).abs();
    let dot = ax * sx + ay * sy;
    assert!(
        dot > 0 && cross * 1000 <= dot * 18,
        "the Knight's first move ({sx}, {sy}) is not along the line away from the Mega Knight at {m:?}: cross {cross}, dot {dot}"
    );
}

#[test]
fn not_read_is_todays_engine() {
    let (rows, blow) = scene(DashPushback::NotRead, "Knight", KNIGHT_AT, MK_FOR_KNIGHT, 90);
    let b = blow_tick(&rows, blow);
    assert_eq!(rows[b + 1].1, 0, "not_read: the Knight moved {} on the tick after the blow", rows[b + 1].1);
}

#[test]
fn a_victim_that_ignores_pushback_is_not_moved_under_either_value() {
    for arm in [DashPushback::NotRead, DashPushback::ClientLadderFromLanding] {
        let (rows, blow) = scene(arm, "Giant", GIANT_AT, MK_FOR_GIANT, 90);
        let b = blow_tick(&rows, blow);
        let steps: Vec<i64> = rows[b + 1..=b + 8].iter().map(|r| r.1).collect();
        assert!(steps.iter().all(|&s| s == 0), "{arm:?}: the Giant moved after the blow on {b}: {steps:?}");
    }
}

#[test]
fn the_shipped_value_is_not_read() {
    assert_eq!(config().calib.dash_pushback, DashPushback::NotRead, "shipped combat.DASH_PUSHBACK");
}
