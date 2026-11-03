//! status.STUN_CLEARS_TARGET: what a stun or a freeze landing on a unit does to its target (state.rs `land_stun`).
//!
//! The client reads no target on the tick a Zap lands on a unit holding a live one (parity, round 9 item 30: 23 of 24
//! Zap hits on client 15.535.29), and a frozen princess tower holds none through its freeze. Pinned here, each with its
//! precondition:
//!   1. client_cleared: a Knight attacking a Giant holds no target from the tick a Zap lands on it to its resume,
//!      and takes the Giant back on the resume rescan;
//!   2. kept, the old arm: it holds the Giant through the stun;
//!   3. the shipped arm is client_cleared (since the 2026-09-28 round 9 arms flip).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test stun_clears_target`):
//!   stun_keeps_target  a landing hold keeps the target whatever the arm: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, StunClearsTarget};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// A Blue Knight attacking a Red Giant, Zapped by Red once it is attacking: per tick from the Zap's cast, the Knight's
/// target and whether it is held. Also the Giant's id.
fn zapped_knight(arm: StunClearsTarget) -> (EntityId, Vec<(Option<EntityId>, bool)>) {
    let mut cfg = config();
    cfg.calib.stun_clears_target = arm;
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", n(9000, 11000), None).expect("the Knight");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", n(9000, 12300), None).expect("the Giant");
    run_until(&mut s, 60, |s| s.entity(knight).is_some_and(|k| k.target == Some(giant) && k.attack_phase != royalesim::entity::AttackPhase::Idle));
    let k = s.entity(knight).unwrap();
    assert_eq!(k.target, Some(giant), "the scene drifted: the Knight is not attacking the Giant");
    s.spawn_unit(Team::Red, "Zap", k.pos, None).expect("the Zap");
    let rows = (0..80)
        .map(|_| {
            s.tick();
            let k = s.entity(knight).expect("the Knight lives");
            (k.target, k.stun_ms > 0)
        })
        .collect();
    (giant, rows)
}

#[test]
fn a_landing_stun_clears_the_target_under_client_cleared() {
    let (giant, rows) = zapped_knight(StunClearsTarget::ClientCleared);
    let landed = rows.iter().position(|r| r.1).expect("the scene drifted: the Zap never held the Knight");
    let resumed = (landed..rows.len()).find(|&k| !rows[k].1).expect("the scene drifted: the stun never ended");
    assert!(rows[landed..resumed].iter().all(|r| r.0.is_none()), "client_cleared: the Knight held a target through its stun: {:?}", &rows[landed..resumed]);
    assert!(rows[resumed..].iter().any(|r| r.0 == Some(giant)), "client_cleared: the resume rescan did not take the Giant back");
}

#[test]
fn the_old_arm_keeps_the_target_through_the_stun() {
    let (giant, rows) = zapped_knight(StunClearsTarget::Kept);
    let landed = rows.iter().position(|r| r.1).expect("the scene drifted: the Zap never held the Knight");
    let resumed = (landed..rows.len()).find(|&k| !rows[k].1).expect("the scene drifted: the stun never ended");
    assert!(rows[landed..resumed].iter().all(|r| r.0 == Some(giant)), "kept: the Knight lost the Giant during its stun");
}

/// Since parity r82 (client_cleared before it; client 15.535.29 keeps client_cleared).
#[test]
fn the_shipped_arm_is_client16402_hit_hold_next_tick() {
    assert_eq!(Calib::shipped().stun_clears_target, StunClearsTarget::Client16402HitHoldNextTick);
}

/// Under status.STUN_CLEARS_TARGET = `arm`: a Blue Valkyrie swinging at a Red Golem held beside her is frozen by a Red Ice
/// Spirit's hit (a hit-delivered hold); per tick from the spirit's spawn, the Valkyrie's target and whether she is held. Also
/// the Golem's id.
fn hit_frozen(arm: StunClearsTarget) -> (EntityId, Vec<(Option<EntityId>, bool)>) {
    let mut cfg = config();
    cfg.calib.stun_clears_target = arm;
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    let blue = s.scenario_spawn_now(Team::Blue, "Valkyrie", n(9000, 12000), None).expect("the Valkyrie");
    let golem = s.scenario_spawn_now(Team::Red, "Golem", n(9000, 13200), None).expect("the Golem");
    let mut waited = 0;
    while s.entity(blue).unwrap().target != Some(golem) || s.entity(blue).unwrap().attack_phase == royalesim::entity::AttackPhase::Idle {
        assert!(waited < 120, "the scene drifted: the Valkyrie never swung at the Golem");
        assert!(s.debug_set_pos(golem, n(9000, 13200)));
        s.tick();
        waited += 1;
    }
    s.scenario_spawn_now(Team::Red, "IceSpirits", n(11500, 12000), None).expect("the Ice Spirit");
    let rows = (0..140)
        .map(|_| {
            assert!(s.debug_set_pos(golem, n(9000, 13200)));
            s.tick();
            let b = s.entity(blue).expect("the Valkyrie lives");
            (b.target, b.stun_ms > 0)
        })
        .collect();
    (golem, rows)
}

/// status.STUN_CLEARS_TARGET = client16402_hit_hold_next_tick (client 16.402, parity r82, live13: 142 of 155 hit holds cleared
/// the target on H + 1). Plant: hit_hold_clears_at_landing.
#[test]
fn under_client16402_hit_hold_next_tick_a_hit_freeze_clears_the_target_a_tick_later() {
    let (golem, rows) = hit_frozen(StunClearsTarget::Client16402HitHoldNextTick);
    let h = rows.iter().position(|r| r.1).expect("the scene drifted: the Ice Spirit never froze the Valkyrie");
    assert_eq!(rows[h].0, Some(golem), "client16402_hit_hold_next_tick: she holds the Golem on the freeze's first tick");
    assert_eq!(rows[h + 1].0, None, "client16402_hit_hold_next_tick: cleared on H + 1");
    // NOT VACUOUS: client_cleared drops it on H
    let (_, old) = hit_frozen(StunClearsTarget::ClientCleared);
    let h0 = old.iter().position(|r| r.1).expect("the scene drifted: no freeze");
    assert_eq!(old[h0].0, None, "client_cleared: cleared on landing");
}

/// status.STUN_CLEARS_TARGET = client16402_hit_hold_next_tick: an area's hold (the Zap's) still clears on landing (live13: 40 of
/// 40 area holds on H).
#[test]
fn under_client16402_hit_hold_next_tick_a_zap_still_clears_on_landing() {
    let (giant, rows) = zapped_knight(StunClearsTarget::Client16402HitHoldNextTick);
    let landed = rows.iter().position(|r| r.1).expect("the scene drifted: the Zap never held the Knight");
    let resumed = (landed..rows.len()).find(|&k| !rows[k].1).expect("the scene drifted: the stun never ended");
    assert!(rows[landed..resumed].iter().all(|r| r.0.is_none()), "client16402_hit_hold_next_tick: the Zapped Knight held a target: {:?}", &rows[landed..resumed]);
    assert!(rows[resumed..].iter().any(|r| r.0 == Some(giant)), "the resume rescan did not take the Giant back");
}
