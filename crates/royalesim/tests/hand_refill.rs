//! THE HAND REFILL TIMER (state.rs `refill_hands`, `PlayerState::refill_ms`, match.HAND_REFILL_MS_1X/_2X/_3X).
//!
//! Measured on client 15.535.29 (the oracle's full per-tick client state, 2026-10-02): one timer per player, counting down 50
//! a tick. At 0, the queue's front card fills the LOWEST empty slot and the timer restarts at 1000 ms (500 in 2x
//! elixir, 350 in 3x), reading 1000 on the refill tick itself. A play while the timer reads 0 is refilled on the same tick; a
//! play while it runs leaves its slot empty until it runs out; back-to-back plays queue one card per period.
//!
//! Pinned:
//!   1. a play with the timer at 0 refills its slot on the same tick;
//!   2. a second play at once leaves its slot empty for 19 more ticks and refills it on the 20th (1x);
//!   3. two waiting slots fill lowest first, one period apart;
//!   4. an empty slot cannot be played (EmptySlot);
//!   5. in 2x elixir the period is 10 ticks;
//!   6. the oracle's refill scene (sp-refill-s0) runs tick for tick: the period read at the restart across the 2x step,
//!      350 ms in 3x, and the refill before a play on the same tick.
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

/// THE ORACLE'S REFILL SCENE, tick for tick (sp-refill-s0, client 15.535.29, 2026-10-02): plays executed at 2386 and
/// 2387 (1x, the second slot waits; its refill at 2406 restarts the timer at 500, the 2x period, though the countdown
/// began in 1x); at 4804 and 4805 (3x: the first refills at once, the second at 4811, 7 ticks of 350 ms); and a play
/// executing on 4811 itself, which the tick's refill comes before: the lower slot fills, and the play's own slot waits
/// a full period, to 4818. Spells cast on Blue's own half keep the battle level into overtime.
#[test]
fn the_oracles_refill_scene_runs_tick_for_tick() {
    const SPELLS: [&str; 8] = ["Zap", "Arrows", "Fireball", "Rocket", "Poison", "Freeze", "Rage", "Tornado"];
    let mut cfg: BattleConfig = config();
    cfg.decks = [SPELLS.iter().map(|s| s.to_string()).collect(), SPELLS.iter().map(|s| s.to_string()).collect()];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    // A play "executing on T" is a deploy made before the tick that brings tick_count() to T.
    let run_to = |s: &mut BattleState, t: u32| {
        while s.tick_count() + 1 < t {
            s.tick();
        }
    };
    let cast = |s: &mut BattleState, slot: usize| {
        s.scenario_set_elixir_milli(Team::Blue, 10_000);
        s.deploy_slot(Team::Blue, slot, n((9000, 4000))).expect("a spell on Blue's own half");
    };
    let filled = |s: &BattleState, slot: usize| s.hand_card(Team::Blue, slot).is_ok();
    // (1x -> 2x) a1 at 2386 refills at once; a2 at 2387 waits; its refill at 2406.
    run_to(&mut s, 2386);
    cast(&mut s, 0);
    s.tick();
    assert!(filled(&s, 0), "a1's slot refilled on 2386");
    cast(&mut s, 1);
    for t in 2387..2406 {
        s.tick();
        assert!(!filled(&s, 1), "a2's slot waits through {t}");
    }
    s.tick();
    assert_eq!(s.tick_count(), 2406);
    assert!(filled(&s, 1), "a2's slot refilled on 2406");
    // (3x) b1 at 4804 refills at once; b2 at 4805 waits to 4811; c1 executes on 4811 and waits to 4818.
    run_to(&mut s, 4804);
    assert_eq!(s.elixir_multiplier(), 3, "4804 is in the 3x phase");
    cast(&mut s, 0);
    s.tick();
    assert!(filled(&s, 0), "b1's slot refilled on 4804");
    cast(&mut s, 0);
    run_to(&mut s, 4811);
    assert!(!filled(&s, 0), "b2's slot waits to 4810");
    cast(&mut s, 1);
    s.tick();
    assert_eq!(s.tick_count(), 4811);
    assert!(filled(&s, 0) && !filled(&s, 1), "on 4811 the refill fills the lower slot (b2's) and c1's slot waits");
    run_to(&mut s, 4818);
    assert!(!filled(&s, 1), "c1's slot waits to 4817");
    s.tick();
    assert!(filled(&s, 1), "c1's slot refilled on 4818, a full 350 ms on");
}
