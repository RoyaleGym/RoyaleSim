//! combat.DASH_STAND_CLOCK: how a dasher's stand counts its DashCooldown (state.rs `phase_path16402_for`'s dash stand,
//! Entities::dash_stand_ms).
//!
//! THE LAW, measured on client 16.635 (parity r96, the oracle's il-mg-16635 set: 39 Mega Knight dashes in 20 battles): the
//! stand counts DashCooldown (900 ms) in ms, each stand tick adding the Mega Knight's composed hit-speed tick (50 unbuffed,
//! 35 under a -30 % slow); the jump comes on the first tick the count reaches 900: unbuffed on the trigger + 17 (the
//! engine's mark), slowed throughout on + 25. Exact on 24 of 39 against the tick count's 15.
//!
//! WHAT IS PINNED, on tests/dash_first_sight_trigger.rs's scene (a Blue Mega Knight put down inside its trigger of a Red
//! Knight, first sight on tick 0):
//!   1. with no buff both arms jump on the first sight + 17;
//!   2. inside a Blue Rage (its hit speed up) the new arm jumps sooner, the tick count (the vacuity check) on + 17.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! dash_stand_clock`):
//!   * `dash_stand_clock_unscaled` -- the new arm counts TICK_MS a tick whatever the buffs: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DashAttack, DashFirstSightTrigger, DashStandClock};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const MK_AT: (i32, i32) = (9500, 11500);
const KNIGHT_AT: (i32, i32) = (13126, 14653);

/// The scene under `arm`, a Blue Rage cast on the Mega Knight's point 10 ticks before he is put down when `rage`: the tick
/// (counted from his put-down) of his first move, the jump.
fn jump_tick(arm: DashStandClock, rage: bool) -> u32 {
    let mut cfg = config();
    cfg.calib.dash_attack = DashAttack::ClientDash;
    cfg.calib.dash_first_sight_trigger = DashFirstSightTrigger::FirstSightTick;
    cfg.calib.dash_stand_clock = arm;
    let mut s = BattleState::new(0, cfg);
    if rage {
        s.spawn_unit(Team::Blue, "Rage", at(MK_AT), None).expect("the Rage");
        for _ in 0..10 {
            s.tick();
        }
    }
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at(KNIGHT_AT), None), (Team::Blue, "MegaKnight", at(MK_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let mk = ids[1];
    for t in 0..40u32 {
        let p0 = s.entity(mk).expect("the Mega Knight").pos;
        s.tick();
        let p1 = s.entity(mk).expect("the Mega Knight").pos;
        if p1 != p0 && p0.dist2(p1) > (200 * K as i64).pow(2) {
            return t;
        }
    }
    panic!("the scene drifted: the Mega Knight never jumped in 40 ticks");
}

/// combat.DASH_STAND_CLOCK = client16635_hit_speed_scaled (client 16.635, parity r96, the oracle's il-mg-16635 set: 24 of 39
/// dashes exact, 9 slowed stands moved wrong to right): unbuffed both arms jump on + 17; raged, the new arm jumps sooner
/// than + 17 and ticks (the vacuity check) on + 17. Plant: dash_stand_clock_unscaled.
#[test]
fn under_client16635_hit_speed_scaled_a_raged_mega_knights_stand_is_shorter() {
    assert_eq!(jump_tick(DashStandClock::Ticks, false), 17, "ticks: the unbuffed jump");
    assert_eq!(jump_tick(DashStandClock::Client16635HitSpeedScaled, false), 17, "client16635_hit_speed_scaled: the unbuffed jump");
    assert_eq!(jump_tick(DashStandClock::Ticks, true), 17, "ticks (the vacuity check): the raged jump");
    let new = jump_tick(DashStandClock::Client16635HitSpeedScaled, true);
    assert!((13..17).contains(&new), "client16635_hit_speed_scaled: the raged jump on + {new}, not 13 to 16");
}

/// Since parity r96 for clients 16.402 and 16.635 (ticks before it; client 15.535.29 keeps ticks).
#[test]
fn the_shipped_arm_is_client16635_hit_speed_scaled() {
    assert_eq!(
        royalesim::state::Calib::shipped().dash_stand_clock,
        royalesim::state::DashStandClock::Client16635HitSpeedScaled
    );
    assert_eq!(config().calib.dash_stand_clock, royalesim::state::DashStandClock::Client16635HitSpeedScaled);
}
