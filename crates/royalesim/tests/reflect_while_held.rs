//! A HELD REFLECTOR -- calibration combat.REFLECT_WHILE_HELD, state.rs `reflect_melee_hit` and `reflect_landed_shot` through
//! `reflect_held`.
//!
//! THE READING (client 16.402, parity r88, the oracle's replays of 100 recorded battles): 37 of 40 hits on a stunned
//! Electro Giant within its reflect reach drew no reflect; the 3 that did landed on the stun's last held tick
//! (client16402_held_no_reflect_but_last). The engine answered every one: a Royal Recruit zapped on a stunned Giant's hit
//! and killed early, a Goblin Brawler stunned by an answer the client never gave.
//!
//! THE SCENE: a Blue Electro Giant held on (9000, 13500), a Red Knight from (9000, 14700) on it; once the Knight has been
//! answered once, Red Zaps the Giant every 9 ticks (a Zap's stun holds 10), so the Giant stays stunned; the Knight's hp
//! losses (only the reflect hits it) over the 80 ticks after.
//!
//! PLANT (regression): held_unit_reflects -> `under_client16402_held_no_reflect_a_stunned_electro_giant_does_not_answer` red.
//!   RUSTFLAGS='--cfg clash_plant="held_unit_reflects"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   reflect_while_held
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, ReflectWhileHeld};
use royalesim::Team;

const GIANT_AT: (i32, i32) = (9000, 13500);

/// Under `arm`: the Knight's hp losses over the 80 ticks the Giant is kept stunned, and whether the Giant was stunned on every
/// one of those ticks.
fn answers_while_stunned(arm: ReflectWhileHeld) -> (Vec<i32>, bool) {
    let mut cfg = config();
    cfg.calib.reflect_while_held = arm;
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tick(200);
    let at = |p: (i32, i32)| Vec2::new(p.0 * K, p.1 * K);
    let giant = s.scenario_spawn_now(Team::Blue, "ElectroGiant", at(GIANT_AT), None).expect("the Electro Giant");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 14700)), None).expect("the Knight");
    let mut last = s.entity(knight).expect("the Knight").hp;
    let answered = (0..200).any(|_| {
        assert!(s.debug_set_pos(giant, at(GIANT_AT)));
        s.tick();
        let hp = s.entity(knight).expect("the scene drifted: the Knight died").hp;
        let hit = hp < last;
        last = hp;
        hit
    });
    assert!(answered, "the scene drifted: the Knight's hits were never answered");
    let mut losses = Vec::new();
    let mut held = true;
    for k in 0..80 {
        if k % 9 == 0 {
            s.spawn_unit(Team::Red, "Zap", at(GIANT_AT), None).expect("Zap the Giant");
        }
        assert!(s.debug_set_pos(giant, at(GIANT_AT)));
        s.tick();
        held &= s.entity(giant).expect("the scene drifted: the Giant died").stun_ms > 0;
        let hp = s.entity(knight).expect("the scene drifted: the Knight died").hp;
        if hp < last {
            losses.push(last - hp);
        }
        last = hp;
    }
    (losses, held)
}

/// Plant: held_unit_reflects.
#[test]
fn under_client16402_held_no_reflect_a_stunned_electro_giant_does_not_answer() {
    let (new, held) = answers_while_stunned(ReflectWhileHeld::Client16402HeldNoReflect);
    assert!(held, "the scene drifted: the Giant was not stunned on every tick of the window");
    assert!(new.is_empty(), "client16402_held_no_reflect: the stunned Giant answered: {new:?}");
    // NOT VACUOUS: the old arm answers the same hits
    let (old, _) = answers_while_stunned(ReflectWhileHeld::Answers);
    assert!(old.len() >= 2, "the scene drifted: answers gave under 2 answers in the window: {old:?}");
}

#[test]
fn the_shipped_reflect_while_held_is_answers() {
    assert_eq!(Calib::shipped().reflect_while_held, ReflectWhileHeld::Answers);
}
