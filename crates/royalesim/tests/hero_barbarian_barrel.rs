//! THE HERO BARBARIAN BARREL'S RE-ROLL (tools/extract_cards.py `spell_hero_card`, `reroll_button`; card.rs `ReRollDef`,
//! REROLL_ROLL_SHORT_STEPS; state.rs `RerollRun`, `reroll_pass`, `reroll_logs`, EARLY_TRIGGER_TICKS), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, sp-form-BarbLog-hero-s0; the press issued t316, the cast from t317): the
//! Barbarian 142 back a tick on t318..t323, healed half its missing hitpoints on t324 (607 -> 661), on the log's point
//! of the tick before from t325 (200 a tick), 2800 on from t339, deploying t339..t358.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! hero_barbarian_barrel`): reroll_never, reroll_never_lands, reroll_log_never, early_trigger_late,
//! reroll_hidden_at_trigger (parity r88); parity r98:
//!   - reroll_holders_drop_at_hide ->
//!     `under_client16635_holders_let_go_next_tick_its_holder_keeps_it_through_the_logs_tick` red;
//!   - reroll_log_hits_next_tick -> `under_client16635_log_tick_the_reroll_log_hits_on_its_own_tick` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, RerollHideWindow, RerollLogFirstHit};
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
    barbarian_with(|c| c.reroll_hide_window = arm)
}

/// As `barbarian`, with `set` applied to the calibration first.
fn barbarian_with(set: impl FnOnce(&mut Calib)) -> (BattleState, EntityId) {
    let mut cfg = config();
    set(&mut cfg.calib);
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

/// `holder_rows`' rows: whether the Musketeer held the Barbarian at the end of P + `k`.
fn held_on(rows: &[(u32, bool)], k: u32) -> bool {
    rows.iter().find(|r| r.0 == k).unwrap_or_else(|| panic!("no row P + {k}: {rows:?}")).1
}

/// Under spells.REROLL_HIDE_WINDOW = `arm`: a red Musketeer held on (3500, 14000), 4,500 ahead of the Barbarian (held on
/// (3500, 9500) and topped up), takes it; then the press. Per tick from P + 1: (P + k, whether the Musketeer holds the
/// Barbarian at the end of P + k). The slide's 852 keeps the Barbarian inside the Musketeer's reach.
fn holder_rows(arm: RerollHideWindow) -> Vec<(u32, bool)> {
    let (mut s, b) = barbarian_under(arm);
    let (at, m_at) = (n(3500, 9500), n(3500, 14000));
    let m = s.scenario_spawn_now(Team::Red, "Musketeer", m_at, None).expect("the Musketeer");
    let max = s.entity(b).expect("the Barbarian").max_hp;
    let holds = |s: &BattleState| s.entity(m).is_some_and(|e| e.target == Some(b));
    let mut took = false;
    for _ in 0..40 {
        assert!(s.debug_set_pos(b, at) && s.debug_set_hp(b, max) && s.debug_set_pos(m, m_at));
        s.tick();
        if s.entity(m).is_some_and(|e| !e.deploying && e.target == Some(b)) {
            took = true;
            break;
        }
    }
    assert!(took, "the scene drifted: the Musketeer never took the Barbarian");
    assert!(s.debug_set_pos(b, at) && s.debug_set_pos(m, m_at));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press, on the Barbarian");
    let mut rows = Vec::new();
    for _ in 0..12 {
        assert!(s.debug_set_pos(m, m_at));
        s.tick();
        rows.push((s.tick_count() - 1 - p, holds(&s)));
    }
    rows
}

/// spells.REROLL_HIDE_WINDOW = client16635_holders_let_go_next_tick (client 16.635, parity r98, the oracle's il-mg-16635 set:
/// 18 of 18 holders in 8 re-rolls held the Barbarian on its log's tick and let it go on the next): the Musketeer holds it
/// through the slide and the log's tick (P + 2 .. P + 8) and lets it go on P + 9; under client16402_log_tick_to_land_plus_2
/// (the vacuity check) it holds it on P + 7 and lets it go on P + 8. Plant: reroll_holders_drop_at_hide.
#[test]
fn under_client16635_holders_let_go_next_tick_its_holder_keeps_it_through_the_logs_tick() {
    let rows = holder_rows(RerollHideWindow::Client16635HoldersLetGoNextTick);
    for k in 2..=8 {
        assert!(held_on(&rows, k), "client16635_holders_let_go_next_tick: let go on P + {k}: {rows:?}");
    }
    assert!(!held_on(&rows, 9), "client16635_holders_let_go_next_tick: still held on P + 9: {rows:?}");
    // NOT VACUOUS: the shipped arm lets it go on the log's tick
    let old = holder_rows(RerollHideWindow::Client16402LogTickToLandPlus2);
    assert!(held_on(&old, 7) && !held_on(&old, 8), "client16402_log_tick_to_land_plus_2 (the vacuity check): held on P + 7, let go on P + 8: {old:?}");
}

/// Under spells.REROLL_LOG_FIRST_HIT = `arm` (and spells.REROLL_HIDE_WINDOW = client16402_log_tick_to_land_plus_2): the press
/// as in the slide scene; at the end of P + 7 a red Knight with 1 hitpoint is put down on (4000, 8648), 500 beside the log's
/// first point (the Barbarian's, 852 back) and inside its 1,300 half-width: (the Knight stands at the end of P + 8, at the
/// end of P + 9). The log stands in the spells at the end of P + 8 under both arms.
fn log_start_victim(arm: RerollLogFirstHit) -> (bool, bool) {
    let (mut s, b) = barbarian_with(|c| {
        c.reroll_hide_window = RerollHideWindow::Client16402LogTickToLandPlus2;
        c.reroll_log_first_hit = arm;
    });
    let at = n(3500, 9500);
    for _ in 0..3 {
        assert!(s.debug_set_pos(b, at));
        s.tick();
    }
    assert!(s.debug_set_pos(b, at));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press, on the Barbarian");
    for _ in 0..7 {
        s.tick();
    }
    assert_eq!(s.tick_count() - 1, p + 7);
    let k = s.scenario_spawn_now(Team::Red, "Knight", n(4000, 8648), None).expect("the Knight");
    assert!(s.debug_set_hp(k, 1));
    let log = s.cards().index("BarbLog_hero_reroll").expect("the re-roll's log record");
    let stands = |s: &BattleState| s.entity(k).is_some_and(|e| e.hp > 0);
    s.tick();
    assert!(s.spells().iter().any(|x| x.card == log), "the scene drifted: no re-roll log at the end of P + 8 ({arm:?})");
    let on_log_tick = stands(&s);
    s.tick();
    (on_log_tick, stands(&s))
}

/// spells.REROLL_LOG_FIRST_HIT = client16635_log_tick (client 16.635, parity r98, the oracle's il-mg-16635 set: 4 hits in 2
/// re-rolls on the log's tick, the engine's a tick later): the log, put down before the log tick's spell pass, sweeps its first
/// point there, so the Knight beside it falls on P + 8; under next_tick (the vacuity check) it stands through P + 8 and falls on
/// P + 9. Plant: reroll_log_hits_next_tick.
#[test]
fn under_client16635_log_tick_the_reroll_log_hits_on_its_own_tick() {
    assert_eq!(log_start_victim(RerollLogFirstHit::NextTick), (true, false), "next_tick (the vacuity check): it falls on P + 9");
    assert_eq!(log_start_victim(RerollLogFirstHit::Client16635LogTick), (false, false), "client16635_log_tick: it falls on the log's tick, P + 8");
}

/// spells.REROLL_LOG_FIRST_HIT ships at next_tick.
#[test]
fn the_shipped_arm_is_next_tick() {
    assert_eq!(Calib::shipped().reroll_log_first_hit, RerollLogFirstHit::NextTick);
}
