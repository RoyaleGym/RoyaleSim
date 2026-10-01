//! THE HERO GOBLINS' FLAG (tools/extract_cards.py `flag_button`; card.rs `FlagSpawnsDef`, `CardDb::flag_of`; state.rs
//! the flag's release in `phase_reap`, `note_flag_play`, `flag_tagged`, `FlagRun`, `flag_pass`, the button on the flag,
//! EARLY_TRIGGER_TICKS), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-Goblins-hero-s0; the press issued t366): the last goblin's last frame
//! t285, the flag's first t286 on its point; the dummies' first frames t376 and t380 at the flag's point + (1000, -500)
//! and (-1000, -500) (x toward the centre); the flag's last frame t384. Read off the table: the button's 5000 ms window
//! and the flag's end 1500 ms after it, and a play of the Goblins card tagging the goblins then standing.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_goblins`): flag_never, flag_spawns_never, flag_play_ignored, flag_never_dies.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeployError};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Goblins", "Knight", "Archers", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
/// On blue's side, left of the centre line (its dummies step right, toward the centre), out of every red tower's reach.
const AT: (i32, i32) = (5000, 9000);
const FLAG: &str = "GoblinHero_Flag_Building";

fn battle() -> BattleState {
    let mut cfg = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s
}

/// The hero Goblins played at AT and, once down, held on 1 hitpoint around a red Valkyrie until her spin kills them:
/// the battle and the death tick.
fn goblins_killed(s: &mut BattleState) -> u32 {
    s.deploy(Team::Blue, "Goblins", n(AT.0, AT.1)).expect("the play");
    for _ in 0..30 {
        s.tick();
    }
    let gobs: Vec<EntityId> = find_live(s, Team::Blue, "Goblins_hero").iter().map(|e| e.id).collect();
    assert_eq!(gobs.len(), 4, "four hero goblins");
    let valk = s.scenario_spawn_now(Team::Red, "Valkyrie", n(AT.0, AT.1 + 600), None).expect("the Valkyrie");
    for _ in 0..200 {
        for g in &gobs {
            if s.entity(*g).is_some() {
                assert!(s.debug_set_hp(*g, 1));
                assert!(s.debug_set_pos(*g, n(AT.0, AT.1)));
            }
        }
        assert!(s.debug_set_pos(valk, n(AT.0, AT.1 + 600)));
        s.tick();
        if gobs.iter().all(|g| s.entity(*g).is_none()) {
            return s.tick_count() - 1;
        }
    }
    panic!("the goblins outlived the Valkyrie");
}

fn flag(s: &BattleState) -> Option<(EntityId, Vec2)> {
    find_live(s, Team::Blue, FLAG).first().map(|e| (e.id, e.pos))
}

#[test]
fn the_last_goblins_death_leaves_the_flag_and_its_button_for_5000_ms() {
    let mut s = battle();
    let d = goblins_killed(&mut s);
    let (_, at) = flag(&s).expect("the flag on the death tick");
    let off = ((at.x - n(AT.0, AT.1).x) / K, (at.y - n(AT.0, AT.1).y) / K);
    assert!(off.0.abs() <= 200 && off.1.abs() <= 200, "the flag on the goblin's point (tick {d}): {off:?}");
    assert_eq!(s.check_ability_button(Team::Blue, 0), Ok(()), "its button, ready at once");
    let mut ready_until = None;
    let mut gone = None;
    for _ in 0..160 {
        s.tick();
        let now = s.tick_count() - 1;
        if ready_until.is_none() && s.check_ability_button(Team::Blue, 0) == Err(DeployError::NoHero) {
            ready_until = Some(now - d);
        }
        if gone.is_none() && flag(&s).is_none() {
            gone = Some(now - d);
        }
    }
    // A check after tick X runs as a press on X + 1: the last press taken is on the flag's tick + 99.
    assert_eq!(ready_until, Some(99), "the button gone its 5000 ms after the flag");
    assert_eq!(gone, Some(130), "the flag gone 1500 ms after that");
}

#[test]
fn its_press_puts_two_dummies_down_toward_the_centre_and_takes_the_flag() {
    let mut s = battle();
    goblins_killed(&mut s);
    let (_, at) = flag(&s).expect("the flag");
    for _ in 0..5 {
        s.tick();
    }
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut dummies: Vec<(u32, Vec2)> = Vec::new();
    let mut ids: Vec<EntityId> = Vec::new();
    let mut gone = None;
    for _ in 0..20 {
        s.tick();
        let now = s.tick_count() - 1;
        for e in find_live(&s, Team::Blue, "Goblin_dummy") {
            if !ids.contains(&e.id) && e.deploying {
                ids.push(e.id);
                dummies.push((now - p, e.pos));
            }
        }
        if gone.is_none() && flag(&s).is_none() {
            gone = Some(now - p);
        }
    }
    let firsts: Vec<(u32, Vec2)> = dummies.iter().fold(Vec::new(), |mut v, d| {
        if !v.iter().any(|(t, _): &(u32, Vec2)| *t == d.0) {
            v.push(*d);
        }
        v
    });
    assert_eq!(firsts.len(), 2, "two dummies: {dummies:?}");
    assert_eq!(firsts[0], (10, Vec2::new(at.x + 1000 * K, at.y - 500 * K)), "the first at the trigger (P + 10): {dummies:?}");
    assert_eq!(firsts[1], (14, Vec2::new(at.x - 1000 * K, at.y - 500 * K)), "the second 200 ms on: {dummies:?}");
    assert_eq!(gone, Some(19), "the flag gone 450 ms after the trigger");
}

#[test]
fn a_second_goblins_play_tags_the_first_ones_and_leaves_no_flag() {
    let mut s = battle();
    s.deploy(Team::Blue, "Goblins", n(AT.0, AT.1)).expect("the first play");
    for _ in 0..30 {
        s.tick();
    }
    // The second play of the Goblins card (its base: the hand's next copy), far off.
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.spawn_unit(Team::Blue, "Goblins", n(15000, 5000), None).expect("the second play");
    for _ in 0..3 {
        s.tick();
    }
    goblins_killed_again(&mut s);
    assert!(flag(&s).is_none(), "no flag: the goblins were tagged by the second play");
}

/// The first play's goblins (the hero ones standing at AT) killed as in `goblins_killed`.
fn goblins_killed_again(s: &mut BattleState) {
    let gobs: Vec<EntityId> = find_live(s, Team::Blue, "Goblins_hero").iter().map(|e| e.id).collect();
    assert_eq!(gobs.len(), 4, "four hero goblins");
    let valk = s.scenario_spawn_now(Team::Red, "Valkyrie", n(AT.0, AT.1 + 600), None).expect("the Valkyrie");
    for _ in 0..200 {
        for g in &gobs {
            if s.entity(*g).is_some() {
                assert!(s.debug_set_hp(*g, 1));
                assert!(s.debug_set_pos(*g, n(AT.0, AT.1)));
            }
        }
        assert!(s.debug_set_pos(valk, n(AT.0, AT.1 + 600)));
        s.tick();
        if gobs.iter().all(|g| s.entity(*g).is_none()) {
            return;
        }
    }
    panic!("the goblins outlived the Valkyrie");
}
