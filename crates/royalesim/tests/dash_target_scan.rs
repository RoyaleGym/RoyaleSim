//! targeting.DASH_TARGET_SCAN (target.rs `decide`): whether a unit in its dash (Entities::dash_state Dashing) scans for a target.
//!
//! THE LAW, measured on client 16.402 (parity r83, live13): over 8,275 truth dash ticks (the Bandit 797, the Boss Bandit 654, the
//! Mega Knight's jumps 6,824) no dasher changed its target to a live unit; its only changes were to none, on the target's death.
//! The engine's `scans` switched on 17 ticks in 12 dashes, 6 of them to a deploying unit's first frame.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test dash_target_scan`):
//!   * `dash_scans_mid_dash` -- the new arm still scans mid-dash: the client16402_held_mid_dash test red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DashTargetScan};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// tests/dash_attack.rs's Bandit geometry: a red Knight walking to the blue right princess tower and a blue Bandit after it.
const KNIGHT_AT: (i32, i32) = (14231, 9500);
const BANDIT_AT: (i32, i32) = (8200, 6200);

/// Under targeting.DASH_TARGET_SCAN = `arm`: the Bandit walks after the Knight, stands and dashes at it; on its first dash tick a
/// red Skeleton is put 1,500 ahead of it on its line (nearer than the Knight, in its sight) and held there. The Knight, the
/// Skeleton, and the Bandit's target after each tick of the dash from then (its move above 400).
fn dash_targets(arm: DashTargetScan) -> (EntityId, EntityId, Vec<Option<EntityId>>) {
    let mut cfg = config();
    cfg.calib.dash_target_scan = arm;
    let mut s = BattleState::new(0, cfg);
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at(KNIGHT_AT), None), (Team::Blue, "Assassin", at(BANDIT_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (knight, bandit) = (ids[0], ids[1]);
    let step = |a: Vec2, b: Vec2| {
        let (dx, dy) = (i64::from((b.x - a.x) / K), i64::from((b.y - a.y) / K));
        isqrt(dx * dx + dy * dy)
    };
    for _ in 0..120 {
        let before = s.entity(bandit).expect("the Bandit").pos;
        s.tick();
        let v = s.entity(bandit).expect("the Bandit");
        if step(before, v.pos) > 400 {
            assert_eq!(v.target, Some(knight), "the scene drifted: the Bandit dashes at something else");
            let k = s.entity(knight).expect("the Knight").pos;
            let (dx, dy) = (i64::from((k.x - v.pos.x) / K), i64::from((k.y - v.pos.y) / K));
            let len = isqrt(dx * dx + dy * dy).max(1);
            let p = Vec2::new(v.pos.x + (dx * 1500 / len) as i32 * K, v.pos.y + (dy * 1500 / len) as i32 * K);
            let skel = s.scenario_spawn_now(Team::Red, "Skeleton", p, None).expect("a red Skeleton");
            let mut rows = Vec::new();
            for _ in 0..30 {
                assert!(s.debug_set_pos(skel, p));
                let before = s.entity(bandit).expect("the Bandit").pos;
                s.tick();
                let Some(v) = s.entity(bandit) else { break };
                if step(before, v.pos) <= 400 {
                    break;
                }
                rows.push(v.target);
            }
            assert!(rows.len() >= 2, "the scene drifted: the dash ended {} ticks after the Skeleton came", rows.len());
            return (knight, skel, rows);
        }
    }
    panic!("{arm:?}: the scene drifted: the Bandit never dashed");
}

/// targeting.DASH_TARGET_SCAN = client16402_held_mid_dash (client 16.402, parity r83, live13: 0 of 8,275 dash ticks changed to a
/// live target). Plant: dash_scans_mid_dash.
#[test]
fn under_client16402_held_mid_dash_a_dashing_bandit_keeps_its_target() {
    let (knight, _, rows) = dash_targets(DashTargetScan::Client16402HeldMidDash);
    assert!(rows.iter().all(|t| *t == Some(knight)), "client16402_held_mid_dash: the Bandit let its Knight go mid-dash: {rows:?}");
    // NOT VACUOUS: scans takes the nearer Skeleton mid-dash
    let (_, skel, old) = dash_targets(DashTargetScan::Scans);
    assert!(old.contains(&Some(skel)), "scans: the Bandit never took the Skeleton mid-dash: {old:?}");
}
