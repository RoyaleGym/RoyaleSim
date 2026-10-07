//! THE GLOBALLIGHTNING EVENT CARD (item 297; card.rs `late_area_spawn`, tools/extract_cards.py `schedule_entry`'s
//! ActionDelay): a hand cast makes an area that lives out its 5000 ms and makes nothing. Its charge's one spawn of the next
//! charge is due at ActionDelay 5000, its own LifeDuration, and is lost with it.
//!
//! THE MEASUREMENT (client 15.535.29, sp-event-GlobalLightning-s0 and sp-event-GlobalLightning2-s0, two hand casts, level
//! 11): no unit or tower lost hp and no unit was held for 450 ticks after the cast, where the table's chain would have
//! struck the Knight and the Musketeer from about the cast + 219; 1 elixir paid, the hand cycled.
//!
//! WHAT IS PINNED: the card loads as a 5000 ms area with no entries; a cast beside two Red troops makes one object that is
//! gone 100 ticks later, and neither troop loses hp or is held in 200 ticks.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test global_lightning`):
//!   * `late_area_spawn_refused` -- the card is refused again: both tests go red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::SpellShape;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// (1) Its shape. Plant: late_area_spawn_refused.
#[test]
fn the_card_loads_as_an_area_that_makes_nothing_for_5000_ms() {
    let db = cards();
    let idx = db.index("GlobalLightning").unwrap_or_else(|| panic!("GlobalLightning not simulable: {:?}", db.rejected));
    let spell = db.get(idx).spell.clone().expect("a spell");
    match spell.shape {
        SpellShape::ScheduledArea { life_ms, schedule } => assert_eq!((life_ms, schedule.len()), (5000, 0)),
        other => panic!("GlobalLightning: {other:?}"),
    }
}

/// (2) A cast: one object for 100 ticks, nothing hit, nothing held. Plant: late_area_spawn_refused.
#[test]
fn a_cast_hits_and_holds_nothing() {
    let mut s = BattleState::new(15, config());
    past_deploy_lockout(&mut s);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 19000)), None).expect("the Knight");
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", at((7000, 20000)), None).expect("the Musketeer");
    for _ in 0..25 {
        s.tick();
    }
    let hp = |s: &BattleState| [knight, musk].map(|id| s.entity(id).map_or(-1, |e| e.hp));
    let before = hp(&s);
    s.spawn_unit(Team::Blue, "GlobalLightning", at((9000, 19000)), None).expect("the cast is accepted");
    let mut lived = 0;
    for _ in 0..200 {
        s.debug_set_pos(knight, at((9000, 19000)));
        s.debug_set_pos(musk, at((7000, 20000)));
        s.tick();
        if !s.spells().is_empty() {
            lived += 1;
        }
        assert_eq!(hp(&s), before, "nothing is hit (tick {})", s.tick_count() - 1);
        assert!([knight, musk].iter().all(|&id| s.entity(id).is_some_and(|e| e.stun_ms == 0)), "nothing is held");
    }
    assert!((99..=101).contains(&lived), "the area lives its 5000 ms (100 ticks) and goes: {lived}");
}
