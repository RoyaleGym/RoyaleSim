//! A KNOCKED UNIT'S TARGET -- calibration targeting.KNOCKED_TARGET_HOLD, target.rs `decide`, `knocked_lets_go`.
//!
//! THE READING (client 15.535.29): a unit in a knock ladder (its own attack's recoil, or a push) holding a target that is
//! not a crown tower keeps it on every tick its centre is within SightRange + both radii + 25 (1,450 ticks, the largest
//! +24 past round sight) and lets it go on the first tick past that, 9 of 9 (+70 to +170: a Sparky in its own recoil
//! four times, an AxeMan and Musketeers pushed). sp-il-8b9b t509-512: a Sparky fired at a Hog Rider from 377 inside its
//! round sight, its recoil carried it to 54 inside (held) and then 70 past (let go, the princess tower taken).
//!
//! The scene: a blue Sparky fires at a red Knight held still 6,000 away (its round sight 5,000 + 1,000 + 500 = 6,500); its
//! recoil ladder carries it back past 6,525.
//!
//! PLANT (regression): knocked_target_held -> `a_recoiling_sparky_lets_a_target_past_its_sight_go` red.
//! PLANT (regression): knocked_lost_target_held -> `a_recoiling_sparky_whose_target_dies_takes_the_next_under_client15535_rescans` red.
//!   RUSTFLAGS='--cfg clash_plant="knocked_target_held"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test knocked_target_hold
//!
//! targeting.KNOCKED_LOST_TARGET = client15535_rescans (client 15.535.29: 28 of 28 ladders whose unit's target was gone
//! took a new one before the ladder ended): a laddered unit with no live target decides as one off the ladder would.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, KnockedLostTarget, KnockedTargetHold};
use royalesim::Team;

const SPARKY_AT: (i32, i32) = (9000, 6500);
const KNIGHT_AT: (i32, i32) = (9000, 12500);
/// targeting.LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET, native.
const KEEP_EXTENSION: i64 = 25;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Sparky's ladder ticks after its first shot: (the start-of-tick centre distance less its round sight + both radii +
/// 25, native; whether it still held the Knight after the tick).
fn ladder(arm: KnockedTargetHold) -> Vec<(i64, bool)> {
    let mut cfg = config();
    cfg.calib.knocked_target_hold = arm;
    let mut s = BattleState::new(3, cfg);
    let sparky = s.scenario_spawn_now(Team::Blue, "ZapMachine", at(SPARKY_AT), None).expect("the Sparky");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT_AT), Some(100_000)).expect("the Knight");
    let mut out = Vec::new();
    for _ in 0..400 {
        assert!(s.debug_set_pos(knight, at(KNIGHT_AT)));
        let (sp, kn) = (s.entity(sparky).expect("the Sparky"), s.entity(knight).expect("the Knight"));
        let knocked = sp.push_active;
        let sight = s.config().cards.get(sp.card_idx).sight_range as i64;
        let (dx, dy) = ((kn.pos.x - sp.pos.x) as i64, (kn.pos.y - sp.pos.y) as i64);
        let past = (isqrt(dx * dx + dy * dy) - sight - sp.radius as i64 - kn.radius as i64) / K as i64 - KEEP_EXTENSION;
        s.tick();
        if knocked {
            out.push((past, s.entity(sparky).expect("the Sparky").target == Some(knight)));
        } else if !out.is_empty() {
            break;
        }
    }
    assert!(!out.is_empty(), "the scene drifted: the Sparky never recoiled");
    assert!(out.iter().any(|&(p, _)| p > 0), "vacuous: the recoil never carried the Sparky past its keep reach: {out:?}");
    out
}

/// Plant: knocked_target_held.
#[test]
fn a_recoiling_sparky_lets_a_target_past_its_sight_go() {
    let out = ladder(KnockedTargetHold::Client15535SightKeep);
    let first = out.iter().position(|&(p, _)| p > 0).expect("a tick past the keep reach");
    assert!(out[..first].iter().all(|&(_, held)| held), "let go within its keep reach: {out:?}");
    assert!(!out[first].1, "kept the Knight on the first tick past its keep reach: {out:?}");
}

#[test]
fn the_shipped_arm_keeps_a_knocked_units_target() {
    assert_eq!(Calib::shipped().knocked_target_hold, KnockedTargetHold::HeldWhileKnocked);
    let out = ladder(KnockedTargetHold::HeldWhileKnocked);
    assert!(out.iter().all(|&(_, held)| held), "the shipped arm let the Knight go: {out:?}");
}

/// Where the second red Knight stands: 3,000 beside the Sparky's start, inside its reach.
const SECOND_AT: (i32, i32) = (SPARKY_AT.0 + 3000, SPARKY_AT.1);

/// The Sparky's ladder ticks after its first shot's recoil: on the ladder's first tick its Knight is killed and a second
/// red Knight is put down at SECOND_AT; whether it held the second Knight after each later ladder tick, under
/// targeting.KNOCKED_LOST_TARGET = `arm` (KNOCKED_TARGET_HOLD at the measured arm).
fn ladder_after_kill(arm: KnockedLostTarget) -> Vec<bool> {
    let mut cfg = config();
    cfg.calib.knocked_target_hold = KnockedTargetHold::Client15535SightKeep;
    cfg.calib.knocked_lost_target = arm;
    let mut s = BattleState::new(3, cfg);
    let sparky = s.scenario_spawn_now(Team::Blue, "ZapMachine", at(SPARKY_AT), None).expect("the Sparky");
    let first = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT_AT), Some(100_000)).expect("the first Knight");
    let mut second = None;
    let mut out = Vec::new();
    for _ in 0..400 {
        if s.entity(first).is_some() {
            assert!(s.debug_set_pos(first, at(KNIGHT_AT)));
        }
        if let Some(k) = second.filter(|k| s.entity(*k).is_some()) {
            assert!(s.debug_set_pos(k, at(SECOND_AT)));
        }
        s.tick();
        let (pushed, target) = {
            let sp = s.entity(sparky).expect("the Sparky");
            (sp.push_active, sp.target)
        };
        if pushed {
            if second.is_none() {
                assert!(s.debug_set_hp(first, 0));
                second = Some(s.scenario_spawn_now(Team::Red, "Knight", at(SECOND_AT), Some(100_000)).expect("the second Knight"));
                continue;
            }
            out.push(target.is_some() && target == second);
        } else if !out.is_empty() {
            break;
        }
    }
    assert!(out.len() >= 3, "the scene drifted: fewer than 3 ladder ticks after the first Knight's death: {out:?}");
    out
}

/// Plant: knocked_lost_target_held.
#[test]
fn a_recoiling_sparky_whose_target_dies_takes_the_next_under_client15535_rescans() {
    let new = ladder_after_kill(KnockedLostTarget::Client15535Rescans);
    assert!(new.iter().any(|&held| held), "client15535_rescans: the Sparky took no target on its ladder: {new:?}");
    // NOT VACUOUS: held_none holds none to the ladder's end.
    let old = ladder_after_kill(KnockedLostTarget::HeldNone);
    assert!(old.iter().all(|&held| !held), "held_none: the Sparky took the second Knight on its ladder: {old:?}");
}
