//! THE HERO BARBARIAN BARREL'S RE-ROLL (tools/extract_cards.py `spell_hero_card`, `reroll_button`; card.rs `ReRollDef`,
//! REROLL_ROLL_SHORT_STEPS; state.rs `RerollRun`, `reroll_pass`, `reroll_logs`, EARLY_TRIGGER_TICKS), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-BarbLog-hero-s0; the press issued t316, the cast from t317): the
//! Barbarian 142 back a tick on t318..t323, healed half its missing hitpoints on t324 (607 -> 661), on the log's point
//! of the tick before from t325 (200 a tick), 2800 on from t339, deploying t339..t358.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_barbarian_barrel`): reroll_never, reroll_never_lands, reroll_log_never, early_trigger_late.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["BarbLog", "Knight", "Archers", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
const HERO: &str = "BarbLogBarbarianHero";

/// The hero Barbarian Barrel played at (3500, 9000) and its Barbarian stood up: the battle and the Barbarian.
fn barbarian() -> (BattleState, EntityId) {
    let mut cfg = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "BarbLog", n(3500, 9000)).expect("the play");
    for _ in 0..120 {
        s.tick();
        let found = find_live(&s, Team::Blue, HERO).first().map(|e| (e.id, e.deploying));
        if let Some((id, false)) = found {
            return (s, id);
        }
    }
    panic!("no Barbarian stood up");
}

#[test]
fn its_press_slides_it_back_heals_it_and_rides_it_2800_on() {
    let (mut s, b) = barbarian();
    let at = n(3500, 9500);
    for _ in 0..3 {
        assert!(s.debug_set_pos(b, at));
        s.tick();
    }
    let max = s.entity(b).expect("the Barbarian").max_hp;
    assert!(s.debug_set_hp(b, max - 200));
    assert!(s.debug_set_pos(b, at));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count();
    s.press_ability_button(Team::Blue, 0).expect("the press, on the Barbarian");
    let log = s.cards().index("BarbLog_hero_reroll").expect("the re-roll's log record");
    let mut rows: Vec<(u32, i32, i32, bool)> = Vec::new();
    let mut logs: Vec<u32> = Vec::new();
    for _ in 0..30 {
        s.tick();
        let e = s.entity(b).expect("the Barbarian");
        rows.push((s.tick_count() - 1 - p, e.pos.y / K, e.hp, e.deploying));
        if s.spells().iter().any(|x| x.card == log) {
            logs.push(s.tick_count() - 1 - p);
        }
    }
    assert_eq!(logs.first(), Some(&8), "the log put down on P + 8, rolling from P + 9: {logs:?}");
    let y = |k: u32| rows.iter().find(|r| r.0 == k).expect("a row").1;
    let y0 = at.y / K;
    // The trigger P + 1 (its cast's start, TriggerDelay 50 less a tick); the slide P + 2 .. P + 7.
    for (k, want) in (2..=7).zip([142, 284, 426, 568, 710, 852]) {
        assert_eq!(y(k), y0 - want, "the slide on P + {k}: {rows:?}");
    }
    let heal = rows.iter().find(|r| r.0 == 8).expect("row 8").2;
    assert_eq!(heal, max - 100, "healed half of its missing 200 on P + 8: {rows:?}");
    assert_eq!(y(9), y0 - 852, "still on the log's first point on P + 9: {rows:?}");
    assert_eq!(y(10), y0 - 652, "one step on on P + 10: {rows:?}");
    assert_eq!(y(23), y0 - 852 + 2800, "2800 on on P + 23: {rows:?}");
    assert!(rows.iter().find(|r| r.0 == 23).expect("row 23").3, "deploying on P + 23: {rows:?}");
}
