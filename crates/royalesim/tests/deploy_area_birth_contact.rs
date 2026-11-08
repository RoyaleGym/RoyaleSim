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
use royalesim::state::{BattleConfig, BattleState, DeployAreaBirthContact, DeployAreaCharacterFirstFrame};
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


/// Under spells.DEPLOY_AREA_CHARACTER_FIRST_FRAME = `arm`: a Red Knight held 3800 north of AT (outside the Electro Wizard's
/// zap, inside its sight), then a Blue Electro Wizard played on AT: (the Wizard's creation tick C, the first tick after which
/// the Knight's target is the Wizard).
fn knight_takes_wizard(arm: DeployAreaCharacterFirstFrame) -> (u32, Option<u32>) {
    let mut c = cfg(DeployAreaBirthContact::Client16402UnpushedCreationTick);
    c.calib.deploy_area_character_first_frame = arm;
    let mut s = BattleState::new(7, c);
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let far = (AT.0, AT.1 + 3800);
    let kn = s.scenario_spawn_now(Team::Red, "Knight", at(far), Some(100_000)).expect("the Knight");
    for _ in 0..40 {
        assert!(s.debug_set_pos(kn, at(far)));
        s.tick();
    }
    s.deploy(Team::Blue, "ElectroWizard", at(AT)).expect("the Electro Wizard play");
    let (mut c_tick, mut wiz) = (None, None);
    let mut took = None;
    for _ in 0..80 {
        assert!(s.debug_set_pos(kn, at(far)));
        s.tick();
        let now = s.tick_count() - 1;
        if wiz.is_none() {
            wiz = s.entities().find(|e| e.card == "ElectroWizard").map(|e| e.id);
            if wiz.is_some() {
                c_tick = Some(now);
            }
        }
        if let (Some(w), None) = (wiz, took) {
            if s.entity(kn).expect("the Knight").target == Some(w) {
                took = Some(now);
            }
        }
    }
    (c_tick.expect("the scene drifted: the Electro Wizard never appeared"), took)
}

/// spells.DEPLOY_AREA_CHARACTER_FIRST_FRAME = client16635_unseen_on_creation_tick (client 16.635, parity r90, live15n: of 70
/// characters a deploy area made none was targeted on its creation tick): the Knight takes the Electro Wizard after C, where
/// under seen_on_creation_tick (the vacuity check) it takes him on C. Plant: area_character_seen_on_creation.
#[test]
fn under_client16635_unseen_on_creation_tick_no_unit_takes_a_played_electro_wizard_on_its_creation_tick() {
    let (c0, old) = knight_takes_wizard(DeployAreaCharacterFirstFrame::SeenOnCreationTick);
    assert_eq!(old, Some(c0), "seen_on_creation_tick (the vacuity check): the Knight did not take the Wizard on C");
    let (c1, new) = knight_takes_wizard(DeployAreaCharacterFirstFrame::Client16635UnseenOnCreationTick);
    assert_eq!(c1, c0, "the scene drifted between the arms");
    assert_eq!(new, Some(c0 + 1), "client16635_unseen_on_creation_tick: the Knight took the Wizard on {new:?}, C = {c0}");
}


#[test]
fn the_shipped_parity_r90_keys_are_their_old_arms() {
    use royalesim::state::{Calib, DashStandAttack, DeployAreaCharacterFirstFrame, GuardPushRearm, LoadFirstHitCorpseSwitch, OnBuffHoldAttract, SelectorReach};
    let c = Calib::shipped();
    assert_eq!(c.deploy_area_character_first_frame, DeployAreaCharacterFirstFrame::SeenOnCreationTick);
    assert_eq!(c.load_first_hit_corpse_switch, LoadFirstHitCorpseSwitch::KeptInReach);
    assert_eq!(c.guard_push_rearm, GuardPushRearm::KeepFaster);
    assert_eq!(c.dash_stand_attack, DashStandAttack::AttackEndsStand);
    assert_eq!(c.selector_reach, SelectorReach::RadiusPlusTargetRadius);
    assert_eq!(c.on_buff_hold_attract, OnBuffHoldAttract::Pulled);
}
