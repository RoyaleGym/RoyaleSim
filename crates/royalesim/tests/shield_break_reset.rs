//! A SHIELD'S BREAK RESTARTS A RAMP -- calibration combat.SHIELD_BREAK_RESETS_ATTACK, state.rs `shield_break_resets`
//! (called from `shield_blasts`).
//!
//! THE READING (client 16.402, parity r88, the oracle's replays of 100 recorded battles): on the tick a unit's shield
//! breaks, every enemy targeting it whose row ramps (VariableDamage) or combos restarts its attack: progress 0, target and
//! load timer kept. An Inferno Dragon's progress went 350 -> 0 at the break (its hits then 7 ticks later than the engine's),
//! and its damage fell back to the first stage. Attackers targeting a Royal Recruit reset just before its first hp loss:
//! Inferno Dragon 12 of 16, Inferno Tower 3 of 3, Monk 10 of 17; a Knight 0 of 82.
//!
//! THE SCENE: a Blue Inferno Dragon at (9000, 12000) on a Red Dark Prince held at (9000, 14500); the Prince's shield is set
//! to 1 once the Dragon's attack has run a while, so its next hit breaks it. The Dragon's progress (attack_ms) on the tick
//! before the break and the tick after.
//!
//! PLANT (regression): shield_break_keeps_attack ->
//!   `under_client16402_variable_rows_restart_a_shields_break_restarts_an_infernos_attack` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, ShieldBreakResetsAttack};
use royalesim::Team;

/// Under `arm`: the Dragon's attack progress on the tick before the Prince's shield breaks and on the tick it breaks.
fn progress_at_break(arm: ShieldBreakResetsAttack) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.shield_break_resets_attack = arm;
    let mut s = BattleState::new(0, cfg);
    let at = |p: (i32, i32)| Vec2::new(p.0 * K, p.1 * K);
    let dragon = s.scenario_spawn_now(Team::Blue, "InfernoDragon", at((9000, 12000)), None).expect("the Inferno Dragon");
    let prince = s.scenario_spawn_now(Team::Red, "DarkPrince", at((9000, 14500)), None).expect("the Dark Prince");
    let mut armed = false;
    let mut before = 0;
    for _ in 0..400 {
        assert!(s.debug_set_pos(prince, at((9000, 14500))));
        let (target, progress) = s.entity(dragon).map(|d| (d.target, d.attack_ms)).expect("the scene drifted: the Dragon died");
        if !armed && target == Some(prince) && progress > 1000 {
            assert!(s.debug_set_shield(prince, 1));
            armed = true;
        }
        before = progress;
        s.tick();
        let p = s.entity(prince).expect("the scene drifted: the Prince died");
        if armed && p.shield == 0 {
            let after = s.entity(dragon).expect("the scene drifted: the Dragon died");
            assert_eq!(after.target, Some(prince), "the Dragon let the Prince go at the break");
            return (before, after.attack_ms);
        }
    }
    panic!("the scene drifted: the Prince's shield never broke under the Dragon (armed {armed})");
}

/// Plant: shield_break_keeps_attack.
#[test]
fn under_client16402_variable_rows_restart_a_shields_break_restarts_an_infernos_attack() {
    let (before, after) = progress_at_break(ShieldBreakResetsAttack::Client16402VariableRowsRestart);
    assert!(before > 1000, "the scene drifted: the Dragon's progress before the break was {before}");
    assert!(after < before && after <= 100, "client16402_variable_rows_restart: the Dragon's progress went {before} -> {after} at the break");
    let (b, a) = progress_at_break(ShieldBreakResetsAttack::Client16402InfernoRowsRestart);
    assert!(a < b && a <= 100, "client16402_inferno_rows_restart: the Dragon's progress went {b} -> {a} at the break");
    // NOT VACUOUS: the old arm keeps it running
    let (b, a) = progress_at_break(ShieldBreakResetsAttack::Kept);
    assert!(a > b, "kept: the Dragon's progress went {b} -> {a} at the break");
}

#[test]
fn the_shipped_shield_break_resets_attack_is_kept() {
    assert_eq!(Calib::shipped().shield_break_resets_attack, ShieldBreakResetsAttack::Kept);
}
