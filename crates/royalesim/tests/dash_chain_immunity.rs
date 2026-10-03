//! combat.DASH_CHAIN_IMMUNITY: whether a dash chain (the Golden Knight's) makes its champion immune to damage (state.rs
//! `fire_ability`, `chain_pass`; entity.rs `dash_immune`, read by combat.rs `resolve`).
//!
//! THE READING (client 15.535.29, every shot landing on a Golden Knight in his chain, 7 of 7 dropped; sp-champ-GoldenKnight-s0
//! t208: a Musketeer's 217 due on him in a pause between his dashes came to nothing, where the engine took it).
//!
//! The scene: the Golden Knight and a red Musketeer 5,000 ahead of him; on the tick the Musketeer's shot at him is first
//! seen the button is pressed, so the shot lands while his chain runs.
//!
//! PLANT (regression): chain_not_immune -> `a_shot_landing_in_his_chain_does_nothing` red.
//!   RUSTFLAGS='--cfg clash_plant="chain_not_immune"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   dash_chain_immunity
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DashChainImmunity};
use royalesim::Team;

const DECK: [&str; 8] = ["GoldenKnight", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Golden Knight's hp lost on the tick the Musketeer's shot landed (pressed as it left).
fn scene(arm: DashChainImmunity) -> i32 {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dash_chain_immunity = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    let gk = s.scenario_spawn_now(Team::Blue, "GoldenKnight", n(3500, 9000), None).expect("the Golden Knight");
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", n(3500, 14000), None).expect("the Musketeer");
    let mut pressed = false;
    for _ in 0..200 {
        assert!(s.debug_set_pos(musk, n(3500, 14000)));
        s.tick();
        if s.projectiles().iter().any(|p| p.team == Team::Red && p.target == gk) {
            s.press_ability_button(Team::Blue, 0).expect("the press is taken");
            pressed = true;
            break;
        }
    }
    assert!(pressed, "the scene drifted: the Musketeer never shot at him");
    for _ in 0..40 {
        let before = s.entity(gk).expect("he lives").hp;
        s.tick();
        if !s.projectiles().iter().any(|p| p.team == Team::Red && p.target == gk) {
            return before - s.entity(gk).expect("he lives").hp;
        }
    }
    panic!("the scene drifted: the shot never landed");
}

/// Plant: chain_not_immune.
#[test]
fn a_shot_landing_in_his_chain_does_nothing() {
    assert_eq!(scene(DashChainImmunity::Client15535WholeChain), 0, "client15535_whole_chain: he lost hp to the shot");
    assert!(scene(DashChainImmunity::None) > 100, "none: the shot did not land on him (vacuous)");
}

#[test]
fn the_shipped_arm_reads_no_immunity() {
    assert_eq!(Calib::shipped().dash_chain_immunity, DashChainImmunity::None);
}
