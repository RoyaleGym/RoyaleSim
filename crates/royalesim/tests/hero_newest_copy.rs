//! A HERO'S BUTTON FOLLOWS ITS NEWEST COPY (state.rs `HeroUnit::superseded`, `newest_hero`).
//!
//! Measured on client 15.535.29 (sp-hero-twin-death-s0 and sp-hero-twin-fresh-s0, two Hero Ice Golems, level 11): on
//! the tick a second copy appears, the first loses its ability fields and the second holds the button; when the second
//! dies the first, still alive, gets no button back (a press on it is refused); a new copy starts with its own charge
//! whatever the older copy spent. Read the same on client 16.402 in a ladder battle.
//!
//! Pinned:
//!   1. the second copy takes the button from the first on its arrival;
//!   2. the second copy's death leaves no button while the first lives (NoHero);
//!   3. a new copy brings a fresh charge after the first spent its own.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test hero_newest_copy`):
//!   hero_button_falls_back   no copy is superseded: (1) and (2) go red.
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeployError};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["IceGolemite", "Skeletons", "IceSpirits", "Knight", "Archer", "Goblins", "Zap", "Arrows"];

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn battle() -> BattleState {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s
}

/// Plays the Ice Golem (its hero form) at `at` and returns the new copy once it stands, deployed.
fn play_golem(s: &mut BattleState, at: (i32, i32)) -> EntityId {
    // Cycle the hand until the Ice Golem is back in it.
    for _ in 0..8 {
        if s.hand(Team::Blue).contains(&"IceGolemite") {
            break;
        }
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        let other = s.hand(Team::Blue).iter().find(|c| **c != "IceGolemite").map(|c| c.to_string()).expect("a card to cycle");
        s.deploy(Team::Blue, &other, n((3500, 9500))).expect("a cycling play");
        s.tick();
    }
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let before: Vec<EntityId> = find_live(s, Team::Blue, "IceGolemite_hero").iter().map(|e| e.id).collect();
    s.deploy(Team::Blue, "IceGolemite", n(at)).expect("the Ice Golem's play");
    for _ in 0..60 {
        s.tick();
        let now: Vec<EntityId> = find_live(s, Team::Blue, "IceGolemite_hero").iter().map(|e| e.id).filter(|id| !before.contains(id)).collect();
        if let Some(&id) = now.first() {
            if s.entity(id).is_some_and(|e| e.deploy_ms == 0) {
                return id;
            }
        }
    }
    panic!("the Ice Golem's hero form did not stand within 60 ticks");
}

#[test]
fn the_newest_copy_holds_the_button_and_its_death_leaves_none() {
    let mut s = battle();
    let first = play_golem(&mut s, (3500, 4500));
    assert_eq!(s.ability_buttons(Team::Blue)[0].hero, Some(first), "one copy holds the button");
    let second = play_golem(&mut s, (14500, 4500));
    // (1)
    assert!(s.entity(first).is_some(), "the first copy lives");
    assert_eq!(s.ability_buttons(Team::Blue)[0].hero, Some(second), "the newest copy holds the button");
    // (2) Red Rockets on the second copy, one every 10 ticks where it stands, until it dies.
    for k in 0..300 {
        let Some(e) = s.entity(second) else { break };
        if k % 10 == 0 {
            s.spawn_unit(Team::Red, "Rocket", e.pos, None).expect("a Rocket");
        }
        s.tick();
    }
    assert!(s.entity(second).is_none(), "the second copy died");
    assert!(s.entity(first).is_some(), "the first copy still lives");
    let b = s.ability_buttons(Team::Blue)[0];
    assert!(b.hero.is_none() && !b.available, "no button after the newest copy's death: {b:?}");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    assert_eq!(s.check_ability_button(Team::Blue, 0), Err(DeployError::NoHero), "a press on the older copy is refused");
}

#[test]
fn a_new_copy_brings_a_fresh_charge() {
    let mut s = battle();
    let first = play_golem(&mut s, (3500, 4500));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the first copy's press");
    for _ in 0..5 {
        s.tick();
    }
    assert!(s.ability_buttons(Team::Blue)[0].spent, "the first copy's charge is spent");
    let second = play_golem(&mut s, (14500, 4500));
    let b = s.ability_buttons(Team::Blue)[0];
    assert_eq!(b.hero, Some(second), "the newest copy holds the button");
    assert!(!b.spent && b.available, "with a fresh charge: {b:?}");
    let _ = first;
}
