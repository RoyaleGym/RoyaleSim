//! status.DEFLECT_TRIGGER_TICK: the tick the Monk's Deflect lands (card.rs `AbilityEffect::Deflect`; state.rs
//! `press_ability_button`).
//!
//! THE LAW, measured on client 16.635 (parity r99, the oracle's il-mg-16635 set: his stand state began on P + 13 in 13 of
//! 13 presses; 2 of 2 hits on P + 14 cut to 35 %, 2 of 2 on P + 13 whole): the Deflect lands on the tick whose 50 ms step
//! completes its TriggerDelay, counted from the cast's first frame P: on P + ceil(TriggerDelay / 50) - 1. On 16.635's
//! table (700) that is P + 13, where the engine's arm, whole_ticks, lands it on P + floor(TriggerDelay / 50), P + 14. For
//! 933 (clients 15.535.29 and 16.402) both give P + 18.
//!
//! THE SCENE: blue's level-11 Monk alone, held on AT until his press, status.DEFLECT_BUFF_LANDING =
//! client_after_hits_tick_short (the trigger tick's hits in full, the next ones at 35 %); P is the first tick after the
//! press, the cast's first frame; a red Zap cast so it lands on P + k, against its full damage on an unpressed twin.
//!
//! WHAT IS PINNED: on 16.635's table under client16635_last_partial_tick a Zap on P + 13 lands in full and one on P + 14
//! at 35 %; under whole_ticks (the vacuity check) the one on P + 14 lands in full. On client 15.535.29's table (933),
//! under both arms, a Zap on P + 18 lands in full and one on P + 19 at 35 %.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! deflect_trigger`):
//!   * `deflect_trigger_whole_ticks` -- the new arm still counts whole ticks:
//!     `under_client16635_last_partial_tick_a_700_ms_deflect_lands_on_p_plus_13` red (the Zap on P + 14 lands in full).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::CardDb;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, CardTable, DeflectBuffLanding, DeflectTriggerTick};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Monk", "Knight", "Archer", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
/// The Monk's point, held until his press.
const AT: (i32, i32) = (3500, 12500);
/// The ticks from the Monk's spawn to his press: room to cast the Zap before it.
const LEAD: u32 = 30;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// A level-11 battle on `table` under status.DEFLECT_TRIGGER_TICK = `arm` (status.DEFLECT_BUFF_LANDING =
/// client_after_hits_tick_short), past the opening lockout, blue's Monk put down on AT: the battle and his id.
fn monk_battle(table: CardTable, arm: DeflectTriggerTick) -> (BattleState, EntityId) {
    let mut cfg = BattleConfig::with_cards(CardDb::load_table(table).expect("the table loads"));
    cfg.calib.card_table = table;
    cfg.calib.deflect_trigger_tick = arm;
    cfg.calib.deflect_buff_landing = DeflectBuffLanding::ClientAfterHitsTickShort;
    let deck: Vec<String> = DECK.iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let monk = s.scenario_spawn_now(Team::Blue, "Monk", n(AT), None).expect("the Monk");
    (s, monk)
}

/// Under `table` and `arm`: the hp the Monk loses to a red Zap landing on P + `k` (P the first tick after his press, LEAD
/// ticks after his spawn), and the Zap's full damage on him unpressed. The Zap's flight is read off an unpressed twin:
/// cast before tick c, it lands on c + d.
fn zap_on(table: CardTable, arm: DeflectTriggerTick, k: u32) -> (i32, i32) {
    let (mut s, monk) = monk_battle(table, arm);
    let p = s.tick_count() + LEAD;
    let mut twin = s.clone();
    let c = twin.tick_count();
    let hp0 = twin.entity(monk).expect("the Monk").hp;
    twin.spawn_unit(Team::Red, "Zap", n(AT), None).expect("the Zap");
    let mut d = None;
    for _ in 0..20 {
        let before = twin.entity(monk).expect("the Monk").hp;
        twin.tick();
        if twin.entity(monk).expect("the Monk").hp < before {
            d = Some(twin.tick_count() - 1 - c);
            break;
        }
    }
    let d = d.expect("the Zap never landed on the unpressed Monk");
    let full = hp0 - twin.entity(monk).expect("the Monk").hp;
    let land = p + k;
    assert!(land >= c + d, "the Zap's flight ({d} ticks) is longer than the scene's lead");
    let mut lost = None;
    while s.tick_count() <= land {
        let now = s.tick_count();
        if now < p {
            assert!(s.debug_set_pos(monk, n(AT)), "the Monk lives");
        }
        if now == p {
            s.scenario_set_elixir_milli(Team::Blue, 10_000);
            s.press_ability_button(Team::Blue, 0).expect("the press");
        }
        if now == land - d {
            s.spawn_unit(Team::Red, "Zap", n(AT), None).expect("the Zap");
        }
        let before = s.entity(monk).expect("the Monk").hp;
        s.tick();
        if now == land {
            lost = Some(before - s.entity(monk).expect("the Monk").hp);
        }
    }
    (lost.expect("the row's tick"), full)
}

/// status.DEFLECT_TRIGGER_TICK = client16635_last_partial_tick (client 16.635, parity r99, the oracle's il-mg-16635 set:
/// 249c9115 t535 and 0b5f1c68 t2785, hits on P + 14 cut to 35 %): on 16.635's table (TriggerDelay 700) the Deflect lands on
/// P + 13, a Zap there in full and one on P + 14 at 35 %; under whole_ticks (the vacuity check) it lands on P + 14, the Zap
/// there in full. Plant: deflect_trigger_whole_ticks.
#[test]
fn under_client16635_last_partial_tick_a_700_ms_deflect_lands_on_p_plus_13() {
    let table = CardTable::Client160635007Nov03;
    let (s, _) = monk_battle(table, DeflectTriggerTick::WholeTicks);
    assert_eq!(card_stat(&s, "Monk").ability.as_ref().expect("the Monk's button").trigger_ms, 700, "16.635's TriggerDelay");
    let (old_14, full) = zap_on(table, DeflectTriggerTick::WholeTicks, 14);
    assert!(full > 0, "the scene drifted: the Zap took nothing off the unpressed Monk");
    // NOT VACUOUS: whole_ticks lands the Deflect on P + 14, that tick's hit in full.
    assert_eq!(old_14, full, "whole_ticks: the Zap on P + 14 (its trigger tick) lands in full");
    let (on_13, _) = zap_on(table, DeflectTriggerTick::Client16635LastPartialTick, 13);
    assert_eq!(on_13, full, "client16635_last_partial_tick: the Zap on P + 13 (its trigger tick) lands in full");
    let (on_14, _) = zap_on(table, DeflectTriggerTick::Client16635LastPartialTick, 14);
    assert!(on_14 > 0 && on_14 < full, "client16635_last_partial_tick: the Zap on P + 14 lands at 35 % ({on_14} of {full})");
}

/// On client 15.535.29's table (TriggerDelay 933) both arms land the Deflect on P + 18: a Zap there in full, one on P + 19
/// at 35 %.
#[test]
fn a_933_ms_deflect_lands_on_p_plus_18_under_both_arms() {
    let table = CardTable::Client15535;
    let (s, _) = monk_battle(table, DeflectTriggerTick::WholeTicks);
    assert_eq!(card_stat(&s, "Monk").ability.as_ref().expect("the Monk's button").trigger_ms, 933, "15.535.29's TriggerDelay");
    for arm in [DeflectTriggerTick::WholeTicks, DeflectTriggerTick::Client16635LastPartialTick] {
        let (on_18, full) = zap_on(table, arm, 18);
        assert!(full > 0, "the scene drifted: the Zap took nothing off the unpressed Monk");
        assert_eq!(on_18, full, "{arm:?}: the Zap on P + 18 (its trigger tick) lands in full");
        let (on_19, _) = zap_on(table, arm, 19);
        assert!(on_19 > 0 && on_19 < full, "{arm:?}: the Zap on P + 19 lands at 35 % ({on_19} of {full})");
    }
}

#[test]
fn the_shipped_arm_is_whole_ticks() {
    assert_eq!(Calib::shipped().deflect_trigger_tick, DeflectTriggerTick::WholeTicks);
}
