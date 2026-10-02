//! THE HAND REFILL TIMER (state.rs `refill_hands`, `PlayerState::refill_ms`, match.HAND_REFILL_MS_1X/_2X/_3X).
//!
//! Measured on client 15.535.29 (the oracle's full kernel frames, 2026-10-02): one timer per player, counting down 50
//! a tick. At 0, the queue's front card fills the LOWEST empty slot and the timer restarts at 1000 ms (500 in 2x
//! elixir, 350 in 3x), reading 1000 on the refill tick itself. A play while the timer reads 0 is refilled on the same tick; a
//! play while it runs leaves its slot empty until it runs out; back-to-back plays queue one card per period.
//!
//! Pinned:
//!   1. a play with the timer at 0 refills its slot on the same tick;
//!   2. a second play at once leaves its slot empty for 19 more ticks and refills it on the 20th (1x);
//!   3. two waiting slots fill lowest first, one period apart;
//!   4. an empty slot cannot be played (EmptySlot);
//!   5. in 2x elixir the period is 10 ticks.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test hand_refill`):
//!   hand_refill_instant   the played slot refills at once: (2), (3), (4) and (5) go red.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DeployError};
use royalesim::Team;

const DECK: [&str; 8] = ["Knight", "Archer", "Goblins", "Giant", "Musketeer", "Fireball", "Arrows", "Skeletons"];

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s
}

/// Plays `slot` for Blue with a full bar, on Blue's own side.
fn play(s: &mut BattleState, slot: usize) {
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy_slot(Team::Blue, slot, n((9500, 8500))).expect("the play");
}

fn empty(s: &BattleState, slot: usize) -> bool {
    s.hand_card(Team::Blue, slot) == Err(DeployError::EmptySlot)
}

/// Ticks until `slot` holds a card again, up to `limit`.
fn ticks_to_refill(s: &mut BattleState, slot: usize, limit: u32) -> Option<u32> {
    for k in 1..=limit {
        s.tick();
        if !empty(s, slot) {
            return Some(k);
        }
    }
    None
}

#[test]
fn a_play_with_the_timer_idle_refills_on_the_same_tick() {
    let mut s = battle();
    // Let any opening timer run out.
    for _ in 0..40 {
        s.tick();
    }
    play(&mut s, 0);
    assert!(!empty(&s, 0), "(1) the timer was idle, so the play's own tick refills the slot");
}

#[test]
fn a_second_play_waits_for_the_timer_and_slots_fill_lowest_first() {
    let mut s = battle();
    for _ in 0..40 {
        s.tick();
    }
    play(&mut s, 0);
    s.tick(); // slot 0 refilled; the timer restarts at 1000 ms.
    play(&mut s, 2);
    // (2) slot 2 waits 20 ticks from slot 0's refill.
    assert_eq!(ticks_to_refill(&mut s, 2, 40), Some(20), "(2) one card per 1000 ms in 1x elixir");
    // (3) two slots waiting: the lower one fills first, the other a period later.
    play(&mut s, 3);
    play(&mut s, 1);
    // (4) a waiting slot cannot be played.
    assert_eq!(s.check_deploy_slot(Team::Blue, 1, n((9500, 8500))), Err(DeployError::EmptySlot), "(4)");
    assert_eq!(ticks_to_refill(&mut s, 1, 40), Some(20), "(3) slot 1, the lower, fills first");
    assert!(empty(&s, 3), "(3) slot 3 still waits");
    assert_eq!(ticks_to_refill(&mut s, 3, 40), Some(20), "(3) slot 3 a period later");
}

#[test]
fn the_period_is_ten_ticks_in_double_elixir() {
    let mut s = battle();
    while s.elixir_multiplier() < 2 {
        s.tick();
    }
    for _ in 0..20 {
        s.tick();
    }
    play(&mut s, 0);
    s.tick();
    play(&mut s, 1);
    assert_eq!(ticks_to_refill(&mut s, 1, 40), Some(10), "(5) one card per 500 ms in 2x elixir");
}
