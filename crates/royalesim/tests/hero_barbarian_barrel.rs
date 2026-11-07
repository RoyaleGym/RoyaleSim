//! THE HERO BARBARIAN BARREL'S RE-ROLL (tools/extract_cards.py `spell_hero_card`, `reroll_button`; card.rs `ReRollDef`,
//! REROLL_ROLL_SHORT_STEPS; state.rs `RerollRun`, `reroll_pass`, `reroll_logs`, EARLY_TRIGGER_TICKS), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-BarbLog-hero-s0; the press issued t316, the cast from t317): the
//! Barbarian 142 back a tick on t318..t323, healed half its missing hitpoints on t324 (607 -> 661), on the log's point
//! of the tick before from t325 (200 a tick), 2800 on from t339, deploying t339..t358.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_barbarian_barrel`): reroll_never, reroll_never_lands, reroll_log_never, early_trigger_late,
//! reroll_hidden_at_trigger (parity r88).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, RerollHideWindow};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["BarbLog", "Knight", "Archers", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
const HERO: &str = "BarbLogBarbarianHero";

/// The hero Barbarian Barrel played at (3500, 9000) and its Barbarian stood up: the battle and the Barbarian.
fn barbarian() -> (BattleState, EntityId) {
    barbarian_under(RerollHideWindow::AtTrigger)
}

/// As `barbarian`, under spells.REROLL_HIDE_WINDOW = `arm`.
fn barbarian_under(arm: RerollHideWindow) -> (BattleState, EntityId) {
    let mut cfg = config();
    cfg.calib.reroll_hide_window = arm;
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
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
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

/// Under spells.REROLL_HIDE_WINDOW = `arm`, the press as in the scene above: per tick from P + 1, (P + k, invisible to enemies
/// (its re-roll's buff; status bit 1), deploying, whether the tick just run's targeting could take it).
fn hide_rows(arm: RerollHideWindow) -> Vec<(u32, bool, bool, bool)> {
    let (mut s, b) = barbarian_under(arm);
    let at = n(3500, 9500);
    for _ in 0..3 {
        assert!(s.debug_set_pos(b, at));
        s.tick();
    }
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press, on the Barbarian");
    let mut rows = Vec::new();
    for _ in 0..30 {
        s.tick();
        let now = s.tick_count();
        let e = s.entity(b).expect("the Barbarian");
        rows.push((now - 1 - p, e.status_flags & 2 != 0, e.deploying, e.acquirable_from < now));
    }
    rows
}

/// spells.REROLL_HIDE_WINDOW = client16402_log_tick_to_land_plus_2 (parity r88, the oracle's replays of 100 battles on client
/// 16.402, 43 re-rolls: hittable through the back-slide in 22, the engine's 0; hidden from the log's tick; nobody's target
/// before the landing + 2): the Barbarian is not hidden on its slide (P + 2 .. P + 7), is hidden from the heal's tick (P + 8)
/// to its landing, and is not acquirable on the landing's tick nor the next; under at_trigger it is hidden on P + 2. Plant:
/// reroll_hidden_at_trigger.
#[test]
fn under_client16402_log_tick_to_land_plus_2_the_barbarian_is_hittable_on_its_slide_and_hidden_from_its_log() {
    let rows = hide_rows(RerollHideWindow::Client16402LogTickToLandPlus2);
    let row = |k: u32| *rows.iter().find(|r| r.0 == k).unwrap_or_else(|| panic!("no row P + {k}: {rows:?}"));
    for k in 2..=7 {
        assert!(!row(k).1, "client16402_log_tick_to_land_plus_2: hidden on its slide, P + {k}: {rows:?}");
    }
    assert!(row(9).1, "client16402_log_tick_to_land_plus_2: not hidden the tick after the heal's, P + 9: {rows:?}");
    let land = rows.iter().find(|r| r.2).expect("the scene drifted: it never stood up").0;
    assert!(!row(land).3 && !row(land + 1).3 && row(land + 2).3, "client16402_log_tick_to_land_plus_2: acquirable from the landing + 2 (landing P + {land}): {rows:?}");
    // NOT VACUOUS: the old arm hides it on its slide
    let old = hide_rows(RerollHideWindow::AtTrigger);
    assert!(old.iter().find(|r| r.0 == 2).is_some_and(|r| r.1), "at_trigger: not hidden on P + 2: {old:?}");
}

/// Since parity r88 (at_trigger before it; client 15.535.29 keeps at_trigger).
#[test]
fn the_shipped_reroll_hide_window_is_client16402_log_tick_to_land_plus_2() {
    assert_eq!(Calib::shipped().reroll_hide_window, RerollHideWindow::Client16402LogTickToLandPlus2);
}
