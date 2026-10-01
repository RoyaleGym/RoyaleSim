//! THE BOSS BANDIT'S BUTTON (tools/extract_cards.py `champion_warp_back`; card.rs `WarpBackDef`; state.rs `MagicRun`,
//! `magic_warps`, `HeroUnit::uses`, `charges_left`, EARLY_TRIGGER_TICKS), at level 11.
//!
//! THE MEASUREMENTS (client 15.535.29, Oracle's sp-champ-BossBandit-s0; the press issued t175 mid-dash, the cast
//! t178..t194): charges 2 -> 1 and the cooldown 3000 on the press; 6000 back on t195 (the trigger t181, + 700 ms), its
//! target dropped; the button back on t238, the cast's start + 60, with one charge left.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! boss_bandit`): warp_back_never, champion_charges_unread, early_trigger_late.
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
    let p = s.tick_count();
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

#[test]
fn it_has_two_charges_the_second_after_a_3000_ms_cooldown() {
    let (mut s, bb) = scene();
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let p = s.tick_count();
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
