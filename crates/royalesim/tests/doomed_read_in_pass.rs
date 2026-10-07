//! THE DOOM AN ATTACKER READS IN THE SEQUENTIAL PASS -- calibration combat.DOOMED_READ_IN_PASS, state.rs
//! `phase_target_attack_sequential` (`Scratch::turn_hp`) and `phase_target`'s doomed test.
//!
//! THE READING (client 15.535.29, under match.TICK_ORDER = client_sequential_strike): combat.POST_KILL_RETARGET_WAIT's
//! clause (c) frees a projectile attacker whose target died doomed (the homing shots in flight at it at least its
//! hitpoints). An attacker in its attack decides on the pass's starting hitpoints; its doomed test reads the hitpoints the
//! pass has left at its turn, after the strikes of units created before it. Of 89 projectile attackers whose target died
//! not doomed at the tick's start, that reading frees 4 and all 4 went at once (sp-champ-Goblinstein-nopress-s0 t297:
//! a Knight's and a Skeleton's strikes left the monster at -49 before the Musketeer's turn, a tower arrow in flight); of
//! the 85 it holds, 82 waited.
//!
//! The scene: a Blue Mini P.E.K.K.A (created first) and a Blue Musketeer (second) attack a Red Giant held in place, its
//! hitpoints put back each tick. A probe finds a tick on which the P.E.K.K.A's strike lands with a Musketeer shot in flight
//! at the Giant (and none landing); the scene is run again with the Giant's hitpoints set before that tick to the
//! P.E.K.K.A's damage, more than the shot's: the strike takes it to 0 before the Musketeer's turn.
//!   client15535_at_turn: doomed at the Musketeer's turn (the shot in flight against 0): it is freed, no wait;
//!   start_of_pass: not doomed (the shot against the starting hitpoints): it waits five ticks with no target, standing.
//!
//! PLANT (regression): doomed_reads_pass_start -> `an_attacker_whose_target_an_earlier_strike_felled_with_a_shot_in_flight_is_freed_under_client15535_at_turn` red.
//!   RUSTFLAGS='--cfg clash_plant="doomed_reads_pass_start"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test doomed_read_in_pass
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DoomedReadInPass, TickOrder};
use royalesim::{EntityId, Team};

const GIANT_AT: (i32, i32) = (9000, 14000);
const PEKKA_AT: (i32, i32) = (9000, 12900);
const MUSKETEER_AT: (i32, i32) = (9000, 9500);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Per tick k after the tick: (the Giant's hitpoints drop on k, the Musketeer's shots at the Giant in flight at k's start,
/// the Musketeer's target and point after k). `kill`: (the tick before which the Giant's hitpoints are set, to what).
fn scene(arm: DoomedReadInPass, kill: Option<(usize, i32)>) -> Vec<(i32, usize, Option<EntityId>, Vec2)> {
    let mut cfg = config();
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    cfg.calib.doomed_read_in_pass = arm;
    let mut s = BattleState::new(0, cfg);
    let _pekka = s.scenario_spawn_now(Team::Blue, "MiniPekka", at(PEKKA_AT), None).expect("the Mini P.E.K.K.A");
    let musketeer = s.scenario_spawn_now(Team::Blue, "Musketeer", at(MUSKETEER_AT), None).expect("the Musketeer");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", at(GIANT_AT), None).expect("the Giant");
    let full = s.entity(giant).expect("the Giant").max_hp;
    let mut rows = Vec::new();
    for k in 0..600 {
        let mut hp = 0;
        let mut flying = 0;
        if s.entity(giant).is_some() {
            assert!(s.debug_set_pos(giant, at(GIANT_AT)));
            hp = match kill {
                Some((t, h)) if t == k => h,
                _ => full,
            };
            assert!(s.debug_set_hp(giant, hp));
            flying = s.projectiles().iter().filter(|p| p.firer == Some(musketeer) && p.target == giant).count();
        }
        s.tick();
        let after = s.entity(giant).map_or(0, |g| g.hp.max(0));
        let m = s.entity(musketeer).expect("the Musketeer lives");
        rows.push((if hp > 0 { hp - after } else { 0 }, flying, m.target, m.pos));
        if kill.is_some_and(|(t, _)| k > t + 8) {
            break;
        }
    }
    rows
}

/// The probe: the P.E.K.K.A's damage and the first tick (past 40) its strike lands alone with a Musketeer shot in flight.
fn probe() -> (usize, i32) {
    let rows = scene(DoomedReadInPass::StartOfPass, None);
    let mut drops: Vec<i32> = rows.iter().map(|r| r.0).filter(|&d| d > 0).collect();
    drops.sort_unstable();
    drops.dedup();
    assert!(drops.len() >= 2 && (drops.len() == 2 || drops[2] == drops[0] + drops[1]), "the scene drifted: the Giant's drops {drops:?}");
    let (shot, strike) = (drops[0], drops[1]);
    assert!(strike > shot, "the scene drifted: the P.E.K.K.A's strike {strike} is not more than the Musketeer's shot {shot}");
    let l = (40..rows.len()).find(|&k| rows[k].0 == strike && rows[k].1 > 0).expect("the scene drifted: no strike with a shot in flight");
    (l, strike)
}

/// The ticks L+1..L+5 after the kill: (the Musketeer held no target, it stood where it stood on L).
fn after_kill(arm: DoomedReadInPass) -> Vec<(bool, bool)> {
    let (l, strike) = probe();
    let rows = scene(arm, Some((l, strike)));
    assert_eq!(rows[l].0, strike, "{arm:?}: the scene drifted: the strike on L = {l} is not the P.E.K.K.A's alone");
    assert!(rows[l].1 > 0, "{arm:?}: the scene drifted: no shot in flight on L");
    assert_eq!(rows[l + 1].0, 0, "{arm:?}: the scene drifted: the Giant outlived the strike");
    rows[l + 1..=l + 5].iter().map(|r| (r.2.is_none(), r.3 == rows[l].3)).collect()
}

/// Plant: doomed_reads_pass_start.
#[test]
fn an_attacker_whose_target_an_earlier_strike_felled_with_a_shot_in_flight_is_freed_under_client15535_at_turn() {
    let new = after_kill(DoomedReadInPass::Client15535AtTurn);
    assert!(!new.iter().all(|&(none, stood)| none && stood), "client15535_at_turn: the Musketeer served the wait: {new:?}");
    // NOT VACUOUS: start_of_pass serves it, five ticks with no target, standing.
    let old = after_kill(DoomedReadInPass::StartOfPass);
    assert!(old.iter().all(|&(none, stood)| none && stood), "start_of_pass: the Musketeer did not wait five ticks: {old:?}");
}

#[test]
fn the_shipped_arm_reads_the_pass_start() {
    assert_eq!(Calib::shipped().doomed_read_in_pass, DoomedReadInPass::StartOfPass);
}
