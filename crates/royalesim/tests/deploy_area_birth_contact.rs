//! A DEPLOY AREA'S CHARACTER ON ITS CREATION TICK (spells.DEPLOY_AREA_BIRTH_CONTACT; state.rs `phase_spawn`'s
//! `Scratch::area_born`, `phase_path16402_for`'s `own_scan_off`).
//!
//! THE LAW, client16402_unpushed_creation_tick (client 16.402, parity r75, live13): the character a played card's deploy
//! area effect makes (the Electro Wizard, the Ice Wizard) runs no separation scan of its own on its creation tick: the
//! Wizards stood unmoved on C in 28 of 29 plays overlapping an older body, every other card in 20 of 984; the engine's were
//! pushed on C in 4 of 4. It stays a body, and is pushed from the next tick.
//!
//! THE SCENE (tests/deploy_area_effect_deploy_time.rs's deck): a Blue Electro Wizard played on (3500, 10500), alone or with a
//! Blue Knight held 400 north of it: the Wizard's point on its first tick.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="deploy_area_birth_pushed"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//! --test deploy_area_birth_contact`): the new arm still pushes it on its creation tick: the test goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DeployAreaBirthContact};
use royalesim::Team;

const DECK: [&str; 8] = ["IceWizard", "ElectroWizard", "Knight", "Musketeer", "Giant", "Bomber", "Valkyrie", "Archers"];
const AT: (i32, i32) = (3500, 10500);
const KNIGHT: (i32, i32) = (3500, 10900);

fn cfg(arm: DeployAreaBirthContact) -> BattleConfig {
    let mut c = config();
    c.calib.deploy_area_birth_contact = arm;
    c.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    c.shuffle_decks = false;
    c
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Under `arm`: the `card`'s point on the first tick it stands on the board, `played` from the hand (else put down by a
/// scenario spawn), with or without the Blue Knight held on KNIGHT.
fn first_tick_pos(arm: DeployAreaBirthContact, card: &str, played: bool, knight: bool) -> Vec2 {
    let mut s = BattleState::new(7, cfg(arm));
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let kn = knight.then(|| s.scenario_spawn_now(Team::Blue, "Knight", at(KNIGHT), None).expect("the Knight"));
    if played {
        s.deploy(Team::Blue, card, at(AT)).unwrap_or_else(|e| panic!("scene: the {card} play was refused: {e:?}"));
    } else {
        s.scenario_spawn_now(Team::Blue, card, at(AT), None).unwrap_or_else(|e| panic!("scene: the {card} spawn: {e:?}"));
        if let Some(k) = kn {
            assert!(s.debug_set_pos(k, at(KNIGHT)));
        }
        s.tick();
        return s.entities().find(|e| e.card == card).map(|e| e.pos).expect("the spawned character");
    }
    for _ in 0..60 {
        if let Some(k) = kn {
            assert!(s.debug_set_pos(k, at(KNIGHT)));
        }
        s.tick();
        if let Some(e) = s.entities().find(|e| e.card == card) {
            return e.pos;
        }
    }
    panic!("scene: the {card} never appeared");
}

/// Plant: deploy_area_birth_pushed.
#[test]
fn under_client16402_unpushed_creation_tick_a_played_electro_wizard_is_not_pushed_on_its_first_tick() {
    let new = DeployAreaBirthContact::Client16402UnpushedCreationTick;
    let alone = first_tick_pos(new, "ElectroWizard", true, false);
    assert_eq!(first_tick_pos(new, "ElectroWizard", true, true), alone, "client16402_unpushed_creation_tick: the Wizard was pushed on its creation tick");
    assert_eq!(first_tick_pos(new, "IceWizard", true, true), first_tick_pos(new, "IceWizard", true, false), "client16402_unpushed_creation_tick: the Ice Wizard was pushed on its creation tick");
    // NOT VACUOUS: under pushed the Knight pushes it on that tick.
    let old = DeployAreaBirthContact::Pushed;
    assert_ne!(first_tick_pos(old, "ElectroWizard", true, true), first_tick_pos(old, "ElectroWizard", true, false), "pushed: the scene drifted (no push on the creation tick)");
}

/// A character put down by a scenario spawn is no play: its first tick's push is the same under both arms.
#[test]
fn a_scenario_spawned_electro_wizard_is_pushed_under_both_arms() {
    let new = first_tick_pos(DeployAreaBirthContact::Client16402UnpushedCreationTick, "ElectroWizard", false, true);
    assert_eq!(new, first_tick_pos(DeployAreaBirthContact::Pushed, "ElectroWizard", false, true), "the arms part on a scenario spawn");
    assert_ne!(new, first_tick_pos(DeployAreaBirthContact::Pushed, "ElectroWizard", false, false), "the scene drifted: no push on a scenario spawn");
}
