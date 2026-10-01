//! THE BOSS BANDIT'S BUTTON (tools/extract_cards.py `champion_warp_back`; card.rs `WarpBackDef`; state.rs `MagicRun`,
//! `magic_warps`, `HeroUnit::uses`, `charges_left`, EARLY_TRIGGER_TICKS), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, Oracle's sp-champ-BossBandit-s0; the press issued t175 mid-dash, the cast
//! t178..t194): charges 2 -> 1 and the cooldown 3000 on the press; 6000 back on t195 (the trigger t181, + 700 ms), its
//! target dropped; the button back on t238, the cast's start + 60, with one charge left.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! boss_bandit`): warp_back_never, champion_charges_unread, early_trigger_late, press_during_dash.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeployError};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["BossBandit", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const AT: (i32, i32) = (3500, 12500);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Boss Bandit put at AT, alone, and held there 40 ticks: the battle and his id.
fn scene() -> (BattleState, EntityId) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let bb = s.scenario_spawn_now(Team::Blue, "BossBandit", n(AT), None).expect("the Boss Bandit");
    for _ in 0..40 {
        assert!(s.debug_set_pos(bb, n(AT)));
        s.tick();
    }
    (s, bb)
}

#[test]
fn its_press_warps_it_6000_back_700_ms_after_its_trigger() {
    // A free press on P: the cast from P + 1, the trigger P + 4 (TriggerDelay 200 less a tick), the warp P + 18.
    let (mut s, bb) = scene();
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut rows = Vec::new();
    for _ in 0..24 {
        s.tick();
        let e = s.entity(bb).expect("the Boss Bandit");
        rows.push((s.tick_count() - 1 - p, e.pos.y / K, e.target));
    }
    let moved = rows.iter().find(|r| r.1 < AT.1 - 3000).map(|r| r.0);
    assert_eq!(moved, Some(18), "the warp's tick from the press: {rows:?}");
    let r = rows.iter().find(|r| r.0 == 18).expect("row 18");
    assert!((AT.1 - 6000 - r.1).abs() <= 150 && r.2.is_none(), "6000 back, its target dropped: {rows:?}");
}

/// A point's distance from another, native.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = (((a.x - b.x) / K) as i64, ((a.y - b.y) / K) as i64);
    royalesim::fixed::isqrt(dx * dx + dy * dy)
}

/// A PRESS MID-DASH waits for the dash's end: measured on client 15.535.29 (sp-champ-BossBandit-s0: the press issued t175
/// mid-dash, the dash's last frame t177, the cast from t178, the warp on t195, the cast's start + 17).
#[test]
fn a_press_mid_dash_waits_for_the_dash_to_end() {
    // tests/dash_attack.rs's geometry: a red Knight walking to the blue right princess tower, the Boss Bandit after it from
    // beyond its trigger distance.
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let bb = s.scenario_spawn_now(Team::Blue, "BossBandit", n((8200, 6200)), None).expect("the Boss Bandit");
    s.scenario_spawn_now(Team::Red, "Knight", n((14231, 9500)), None).expect("a red Knight");
    // Its dash under way: a tick it steps more than 300 (its walk is under 100 a tick).
    let mut at = s.entity(bb).expect("the Boss Bandit").pos;
    let mut k = 0;
    loop {
        s.tick();
        let p = s.entity(bb).expect("the Boss Bandit").pos;
        if dist(p, at) > 300 {
            break;
        }
        at = p;
        k += 1;
        assert!(k < 200, "the Boss Bandit never dashed");
    }
    // The dash's end with no press, on a twin: the first tick it steps 300 or less.
    let mut twin = s.clone();
    let mut end = None;
    let mut at = twin.entity(bb).expect("the Boss Bandit").pos;
    for _ in 0..40 {
        twin.tick();
        let p = twin.entity(bb).expect("the Boss Bandit").pos;
        if dist(p, at) <= 300 {
            end = Some(twin.tick_count() - 1);
            break;
        }
        at = p;
    }
    let end = end.expect("the dash's end");
    let p0 = s.tick_count() - 1;
    assert!(end > p0 + 1, "a press mid-dash: pressed on {p0}, the dash's end on {end}");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press, mid-dash");
    // The warp: the tick it moves 3000 or more.
    let mut warp = None;
    let mut at = s.entity(bb).expect("the Boss Bandit").pos;
    for _ in 0..60 {
        s.tick();
        let Some(e) = s.entity(bb) else { break };
        if warp.is_none() && dist(e.pos, at) >= 3000 {
            warp = Some(s.tick_count() - 1);
        }
        at = e.pos;
    }
    assert_eq!(warp, Some(end + 17), "the warp the cast's start + 17, the cast from the dash's end {end} (pressed on {p0})");
}

#[test]
fn it_has_two_charges_the_second_after_a_3000_ms_cooldown() {
    let (mut s, bb) = scene();
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    // P: the tick the press is issued on, the last one run (its first frame, the cast's start, is P + 1).
    let p = s.tick_count() - 1;
    s.press_ability_button(Team::Blue, 0).expect("the first press");
    assert_eq!(s.check_ability_button(Team::Blue, 0), Err(DeployError::AbilityNotReady), "a charge left, behind its cooldown");
    // The cast starts on P + 1; its cooldown gives the button back 60 ticks on.
    let mut back = None;
    for _ in 0..80 {
        assert!(s.debug_set_pos(bb, n(AT)));
        s.tick();
        if back.is_none() && s.check_ability_button(Team::Blue, 0).is_ok() {
            back = Some(s.tick_count() - p);
        }
    }
    assert_eq!(back, Some(62), "the button back after the tick P + 61 ran (the cast's start P + 1, + 60)");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the second press");
    for _ in 0..80 {
        s.tick();
    }
    assert_eq!(s.check_ability_button(Team::Blue, 0), Err(DeployError::AbilitySpent), "two charges, both used");
}
