//! spells.DEPLOY_AREA_EFFECT_DEPLOY_TIME, read off the engine: the character a card's area effect spawns (card.rs
//! `deploy_area_effect`: the Ice Wizard, the Electro Wizard) deploys one tick longer than a troop played by hand
//! (state.rs `phase_spawn`).
//!
//! THE LAW, measured on client 16.402 and client 15.535.29: a Wizard recorded from its first frame F leaves its deploy
//! on F + 20 and first acts on F + 21; every other single-unit troop played by hand leaves on F + 19 (153 of 153 on the
//! 16.402 corpus but the Princess and the Golem, which have rules of their own; 40 cards and 95 Knights in the
//! 15.535.29 sweep). Under the old value, unit_deploy_time (today's engine), the Wizards deploy as any troop.
//!
//! WHAT IS PINNED, each with the precondition that makes it bite:
//!   1. a played Ice Wizard and a played Electro Wizard first move one tick later under client_one_tick_longer than
//!      under unit_deploy_time, and a played Knight moves on the same tick under both (so the Wizard's first step is
//!      one tick after a Knight's played the same way);
//!   2. a scenario spawn of the Ice Wizard (the character put down, not the card) is the same under both values;
//!   3. the shipped value is unit_deploy_time.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test deploy_area_effect_deploy_time`):
//!   * `area_character_deploy_unread` -- client_one_tick_longer deploys the character in its DeployTime: (1) goes red.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, DeployAreaEffectDeployTime};
use royalesim::Team;

const NEW: DeployAreaEffectDeployTime = DeployAreaEffectDeployTime::OneTickLonger;
const OLD: DeployAreaEffectDeployTime = DeployAreaEffectDeployTime::UnitDeployTime;
const DECK: [&str; 8] = ["IceWizard", "ElectroWizard", "Knight", "Musketeer", "Giant", "Bomber", "Valkyrie", "Archers"];
const AT: (i32, i32) = (3500, 10500);

fn cfg(arm: DeployAreaEffectDeployTime) -> BattleConfig {
    let mut c = config();
    c.calib.deploy_area_effect_deploy_time = arm;
    c.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    c.shuffle_decks = false;
    c
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// (the tick the unit first appears, the first tick on which it stands elsewhere). `played` deploys the card from the
/// hand; otherwise the character is put down by a scenario spawn.
fn first_move(card: &str, arm: DeployAreaEffectDeployTime, played: bool) -> (u32, u32) {
    let mut s = BattleState::new(7, cfg(arm));
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    if played {
        s.deploy(Team::Blue, card, at(AT)).unwrap_or_else(|e| panic!("scene: the {card} play was refused: {e:?}"));
    } else {
        s.scenario_spawn_now(Team::Blue, card, at(AT), None).unwrap_or_else(|e| panic!("scene: the {card} spawn: {e:?}"));
    }
    let mut first: Option<(u32, Vec2)> = None;
    for _ in 0..60 {
        if first.is_none() {
            // a scenario spawn is on the board before any tick
            if let Some(e) = s.entities().find(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::Troop) {
                first = Some((s.tick_count(), e.pos));
            }
        }
        s.tick();
        let now: Vec<Vec2> = s.entities().filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::Troop).map(|e| e.pos).collect();
        assert!(now.len() <= 1, "scene: more than one Blue troop");
        match (first, now.first()) {
            (None, Some(p)) => first = Some((s.tick_count(), *p)),
            (Some((f, p0)), Some(p)) if *p != p0 => return (f, s.tick_count()),
            _ => {}
        }
    }
    panic!("scene: the {card} did not move within 60 ticks");
}

#[test]
fn a_played_wizard_first_moves_a_tick_later_than_under_the_old_value() {
    let knight_old = first_move("Knight", OLD, true);
    let knight_new = first_move("Knight", NEW, true);
    assert_eq!(knight_new, knight_old, "a played Knight moved on another tick under client_one_tick_longer");
    for wizard in ["IceWizard", "ElectroWizard"] {
        let (f_old, m_old) = first_move(wizard, OLD, true);
        let (f_new, m_new) = first_move(wizard, NEW, true);
        assert_eq!(f_new, f_old, "scene: the {wizard} appeared on another tick");
        assert_eq!(m_old - f_old, knight_old.1 - knight_old.0, "unit_deploy_time: the {wizard} does not deploy as the Knight does");
        assert_eq!(m_new, m_old + 1, "client_one_tick_longer: the {wizard} first moved on {m_new}, want {}", m_old + 1);
    }
}

#[test]
fn a_scenario_wizard_is_the_same_under_both_values() {
    assert_eq!(first_move("IceWizard", NEW, false), first_move("IceWizard", OLD, false), "the values part on a character put down by a scenario");
}

#[test]
fn the_shipped_value_is_unit_deploy_time() {
    assert_eq!(Calib::shipped().deploy_area_effect_deploy_time, DeployAreaEffectDeployTime::UnitDeployTime);
}
