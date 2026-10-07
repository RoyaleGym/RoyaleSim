//! THE GLOBALCLONE EVENT CARD (item 294; card.rs `clone_shape_of`, spell.rs the Clone arm of `step_spells`): the Clone's
//! shape with Radius 30000 and its own Buff, the copies' hold, which also lands on the own units in reach it does not copy.
//!
//! THE MEASUREMENT (client 15.535.29, sp-event-GlobalClone-s0, one cast at (9500, 12500)): six copies on C, one of each of
//! the 6 Blue troops up to 6,597 off the tap, no tower; each on its original's point after its step on C, hp 1 of 1; the
//! pairs held C..C+9 and slid 125 a tick by the Clone's own law. One cast: a lead that agrees with the Clone's laws.
//!
//! WHAT IS PINNED: a lone Blue Knight 14,318 from the tap is copied by GlobalClone and not by the Clone (its 3000 reach);
//! a copy standing in the reach of a second GlobalClone is not copied again but takes the area's hold.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test global_clone`):
//!   * `global_clone_refused` -- the event is refused again: both tests go red;
//!   * `global_clone_buff_on_picked_only` -- the hold reaches the copied units alone: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The copies on the board after `spell` is cast at (16500, 3500) over a lone Blue Knight at (3500, 9500), 14,318 away.
fn copies_far(spell: &str) -> usize {
    let mut s = BattleState::new(0, config());
    s.scenario_spawn_now(Team::Blue, "Knight", at((3500, 9500)), None).expect("the Knight");
    for _ in 0..25 {
        s.tick();
    }
    s.spawn_unit(Team::Blue, spell, at((16500, 3500)), None).unwrap_or_else(|e| panic!("cast {spell}: {e:?}"));
    s.tick();
    s.tick();
    s.entities().filter(|e| e.cloned).count()
}

/// (1) Its reach is the whole 30,000. Plant: global_clone_refused.
#[test]
fn the_event_copies_an_own_troop_14318_off_the_tap() {
    assert_eq!(copies_far("GlobalClone"), 1, "GlobalClone: the Knight 14,318 off the tap is copied");
    // NOT VACUOUS: the Clone's 3000 reach does not reach it.
    assert_eq!(copies_far("Clone"), 0, "Clone: the Knight was copied from 14,318");
}

/// (2) A second GlobalClone over a pair copies the original alone, and holds the old copy too (the Clone hold in its
/// buff slots). Plant: global_clone_buff_on_picked_only.
#[test]
fn a_second_cast_holds_the_earlier_copy_it_does_not_copy() {
    let mut s = BattleState::new(0, config());
    s.scenario_spawn_now(Team::Blue, "Knight", at((9000, 9000)), None).expect("the Knight");
    for _ in 0..25 {
        s.tick();
    }
    s.spawn_unit(Team::Blue, "GlobalClone", at((9000, 9000)), None).expect("the first cast");
    for _ in 0..30 {
        s.tick();
    }
    let first: Vec<_> = s.entities().filter(|e| e.cloned).map(|e| e.id).collect();
    assert_eq!(first.len(), 1, "the scene drifted: the first cast made {} copies", first.len());
    let copy = first[0];
    s.spawn_unit(Team::Blue, "GlobalClone", at((9000, 9000)), None).expect("the second cast");
    s.tick();
    s.tick();
    assert_eq!(s.entities().filter(|e| e.cloned).count(), 2, "the second cast copies the original alone");
    let held = s.entity(copy).is_some_and(|e| e.buffs.iter().any(|sl| !sl.is_empty() && s.cards().buffs.get(sl.id as usize - 1).is_some_and(|d| d.clone_hold)));
    assert!(held, "the earlier copy, in reach and not copied, takes the area's hold");
}
