//! A COPY OF A BATTLE UNDER WAY (state.rs `scenario_set_king_active`, `scenario_set_evo_plays`,
//! `scenario_spawn_batch_levels`, `scenario_spell_in_flight`; py.rs `Battle.reset`'s king_active, evo_counts,
//! unit_spawns and spells_in_flight): the pieces of state a fresh battle does not have, set at the start.
//!   1. a woken king targets from the first ticks, a king put to sleep after a princess fell does not;
//!   2. an evolved card's count makes its next play the one it says, and a count past its cycle, or for a card the deck
//!      does not carry evolved, is refused;
//!   3. a summoned unit goes down by its record's name at the level it is given;
//!   4. two spawns that differ in their level alone go down in one order whatever the list's;
//!   5. a Fireball put on its flight lands on the tick the same flight from its launch does, and a spell that does not
//!      fly is refused.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! scenario_copy`):
//!   - copy_king_active_unset -> 1 red;
//!   - copy_evo_plays_unset -> 2 red;
//!   - copy_spawn_level_ignored -> 3 red;
//!   - copy_spell_flight_from_launch -> 5 red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["BabyDragon".into(), "Knight".into(), "Fireball".into(), "Zap".into()], vec!["Knight".into(), "Golem".into()]];
    cfg.forms = [vec![1, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

/// The ticks, of `ticks`, on which Blue's king has a target, with a red Knight held in its reach (4500 from it) and
/// topped up so the princess towers' fire does not kill it.
fn king_targets(s: &mut BattleState, ticks: u32) -> Vec<u32> {
    let k = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 7500), None).expect("the Knight");
    let king = s.tower_ids(Team::Blue)[0].expect("the king");
    let mut on = Vec::new();
    for t in 1..=ticks {
        let top = s.entity(k).expect("the Knight").max_hp;
        assert!(s.debug_set_pos(k, n(9000, 7500)) && s.debug_set_hp(k, top));
        s.tick();
        if s.entity(king).expect("the king").target.is_some() {
            on.push(t);
        }
    }
    on
}

#[test]
fn a_woken_king_targets_at_once_and_one_put_to_sleep_does_not() {
    let mut s = battle();
    assert!(king_targets(&mut s.clone(), 40).is_empty(), "the control: a fresh king sleeps");
    s.scenario_set_king_active(Team::Blue, true);
    let on = king_targets(&mut s, 40);
    assert!(on.first().is_some_and(|t| *t <= 10), "the woken king targets the Knight within 10 ticks: {on:?}");
    // a princess down wakes the king; put back to sleep, it stays asleep
    let mut s = battle();
    s.scenario_set_tower_hp(Team::Blue, 1, 0).expect("a princess down");
    s.scenario_set_king_active(Team::Blue, false);
    assert!(king_targets(&mut s, 40).is_empty(), "the king put to sleep does not target");
}

#[test]
fn an_evo_count_makes_the_play_it_says() {
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "BabyDragon").expect("the dragon in hand");
        s.cards().get(s.resolve_play(Team::Blue, slot).expect("a play resolves").card).name.clone()
    };
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 2, "the form's cycle");
    assert_eq!(next(&s), "BabyDragon", "a fresh count: a basic play");
    s.scenario_set_evo_plays(Team::Blue, "BabyDragon", 2).expect("a count at its cycle");
    assert_eq!(next(&s), "BabyDragon_EV1", "two basic plays done: the next is the form");
    s.scenario_set_evo_plays(Team::Blue, "BabyDragon", 1).expect("a count below it");
    assert_eq!(next(&s), "BabyDragon", "one done: the next is basic");
    assert!(s.scenario_set_evo_plays(Team::Blue, "BabyDragon", 3).is_err(), "past its cycle");
    assert!(s.scenario_set_evo_plays(Team::Blue, "Knight", 1).is_err(), "a card the deck does not carry evolved");
    assert!(s.scenario_set_evo_plays(Team::Red, "BabyDragon", 1).is_err(), "the other side's deck");
}

#[test]
fn a_summoned_unit_goes_down_by_name_at_its_level() {
    let mut s = battle();
    let db = s.cards().clone();
    let g = db.index("Golemite").expect("the Golemite record");
    let hp_at = |level: i32| db.scaled(g, level, db.get(g).hitpoints).expect("the level");
    assert_ne!(hp_at(9), hp_at(11), "the levels differ in hitpoints");
    let ids = s
        .scenario_spawn_batch_levels(&[(Team::Red, "Golemite", n(5000, 20000), None, Some(9)), (Team::Red, "Golemite", n(13000, 20000), None, None)])
        .expect("two Golemites");
    let max = |s: &BattleState, id: EntityId| s.entity(id).expect("the Golemite").max_hp;
    assert_eq!((max(&s, ids[0]), max(&s, ids[1])), (hp_at(9), hp_at(11)), "level 9 as given; the other at the side's 11");
    assert_eq!(s.entity(ids[0]).expect("the Golemite").card, "Golemite");
    assert!(!s.entity(ids[0]).expect("the Golemite").deploying, "standing, deployed");
}

#[test]
fn spawns_that_differ_in_level_alone_go_down_in_one_order() {
    let at = n(9000, 20000);
    let hash_of = |order: [i32; 2]| {
        let mut s = battle();
        let specs: Vec<(Team, &str, Vec2, Option<i32>, Option<i32>)> = order.iter().map(|l| (Team::Red, "Golemite", at, Some(100), Some(*l))).collect();
        s.scenario_spawn_batch_levels(&specs).expect("two Golemites");
        s.state_hash()
    };
    assert_eq!(hash_of([9, 11]), hash_of([11, 9]), "the list's order does not reach the battle");
}

/// Blue's Fireball cast at `aim` and stepped until it is gone: the Flight point after each tick, and the tick it left.
fn flight(mut s: BattleState) -> (Vec<Vec2>, u32) {
    let mut at = Vec::new();
    for t in 1..=200u32 {
        s.tick();
        match s.spells().iter().find_map(|sp| match &sp.motion {
            SpellMotion::Flight { pos, .. } => Some(*pos),
            _ => None,
        }) {
            Some(p) => at.push(p),
            None => return (at, t),
        }
    }
    panic!("the Fireball never landed");
}

#[test]
fn a_fireball_on_its_flight_lands_when_the_same_flight_does() {
    let aim = n(9000, 22000);
    let mut fresh = battle();
    fresh.scenario_set_elixir_milli(Team::Blue, 10_000);
    fresh.deploy(Team::Blue, "Fireball", aim).expect("the cast");
    let (path, land) = flight(fresh);
    assert!(path.len() > 10, "the scene drifted: a flight of {} ticks", path.len());
    // put on its way at the point the fresh flight had reached after tick m: it lands land - m ticks later
    let m = path.len() / 2;
    let mut copy = battle();
    copy.scenario_spell_in_flight(Team::Blue, "Fireball", None, path[m - 1], aim).expect("the Fireball in flight");
    let (_, copy_land) = flight(copy);
    assert_eq!(copy_land, land - m as u32, "the copy from tick {m}'s point ({:?})", path[m - 1]);
    let mut zap = battle();
    let e = zap.scenario_spell_in_flight(Team::Blue, "Zap", None, aim, aim).expect_err("a Zap does not fly");
    assert!(e.contains("does not fly (at_once)"), "{e}");
    // asked first (spell.rs `in_flight_kind`), as a copy's builder does
    let db = zap.cards().clone();
    let kind = |n: &str| royalesim::spell::in_flight_kind(db.get(db.index(n).unwrap_or_else(|| panic!("{n}"))));
    assert_eq!(
        ["Fireball", "Rocket", "Arrows", "Log", "BarbLog", "Snowball", "Zap", "Knight"].map(kind),
        ["flies", "flies", "flies", "rolls", "rolls", "rolls", "at_once", "not_a_spell"]
    );
    let e = zap.scenario_spell_in_flight(Team::Blue, "Log", None, aim, aim).expect_err("the Log rolls");
    assert!(e.contains("does not fly (rolls)"), "{e}");
}
