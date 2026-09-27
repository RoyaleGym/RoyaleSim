//! ELIXIR FOR THE OPPONENT ON A DEATH (card.rs `ManaDef::on_death_for_opponent`; state.rs `phase_reap`,
//! `mana_for_opponent`; economy.MANA_ON_DEATH_FOR_OPPONENT_UNIT, MANA_ON_DEATH_TRIGGER).
//!
//! THE READING, from the 15.535.29 tables and the community reading of the card, not measured (the opponent sat at
//! 10 elixir at both Elixir Golem deaths measured on client 16.402): ManaOnDeathForOpponent is in thousandths of an
//! elixir, so the Elixir Golem (1000) pays the other side one elixir and each golemite and blob (500) half. The engine
//! pays on the death tick, summed per side over the tick's deaths and capped once.
//!
//! HOW A PAYMENT IS READ: two battles, the same but for the dying unit, Blue's elixir set to the same value before
//! every tick; the payment is the difference after the tick.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. a unit whose row carries ManaOnDeathForOpponent 500 pays the OTHER side half an elixir on its death tick, and
//!      its own side nothing (a Knight given the column: the mechanism, whatever card carries it);
//!   2. under the `elixir` arm the same row pays 500 elixir, capped at 10;
//!   3. two such deaths on one tick pay the sum, capped once;
//!   4. the Elixir Golem pays the opponent one elixir, each golemite half and each blob half (needs the Elixir Golem's
//!      unit chain to load: ElixirGolem2 death-spawns ElixirGolem4);
//!   5. a payment that would pass 10 fills to 10.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="mana_on_death_to_owner"' CARGO_TARGET_DIR=target/plant cargo test --test
//! mana_on_death`): the payment goes to the dying unit's own side -> 1, 3 and 4 red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, ForOpponentUnit};
use royalesim::{EntityId, Team};
use std::sync::Arc;

/// A red spot on its own half, native units, out of every blue tower's reach.
const AT: (i32, i32) = (9000, 21000);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The shipped table with ManaOnDeathForOpponent 500 given to the Knight's row.
fn knight_pays() -> BattleConfig {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("data/derived/cards.json");
    let mut doc: serde_json::Value = serde_json::from_str(&text).unwrap();
    let knight = doc["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "Knight").expect("the Knight row");
    knight["mana"] = serde_json::json!({"collect_amount": null, "generate_time_ms": null, "on_death": null, "on_death_for_opponent": 500});
    let db = CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).expect("the edited table loads");
    let mut c = config();
    c.cards = Arc::new(db);
    c.card_level = [11, 11];
    c.tower_level = [11, 11];
    c
}

fn level11() -> BattleConfig {
    let mut c = config();
    c.card_level = [11, 11];
    c.tower_level = [11, 11];
    c
}

/// What `zapped` saw.
struct Zapped {
    /// Per tick: (Blue's raw elixir with the dying units minus without, the same for Red, Blue's raw elixir with).
    d: Vec<(i64, i64, i64)>,
    /// The dying units.
    ids: Vec<EntityId>,
    /// One elixir, raw.
    unit: i64,
    /// The battle with the dying units, at the end.
    s: BattleState,
}

/// Two battles from `c`: `with` gets the red units of `dying` (card, native point) at 1 hp; both get a blue Zap on
/// each point. Blue's elixir is set to `blue_milli` before every tick.
fn zapped(c: BattleConfig, dying: &[(&str, (i32, i32))], blue_milli: i64, ticks: u32) -> Zapped {
    let mut with = BattleState::new(0, c.clone());
    let mut without = BattleState::new(0, c);
    let ids: Vec<EntityId> = dying.iter().map(|(card, p)| with.scenario_spawn_now(Team::Red, card, at(*p), Some(1)).expect("the red unit")).collect();
    for (_, p) in dying {
        with.spawn_unit(Team::Blue, "Zap", at(*p), None).expect("the Zap");
        without.spawn_unit(Team::Blue, "Zap", at(*p), None).expect("the Zap");
    }
    let unit = with.elixir_raw(Team::Blue).1;
    let mut out = Vec::new();
    for _ in 0..ticks {
        with.scenario_set_elixir_milli(Team::Blue, blue_milli);
        without.scenario_set_elixir_milli(Team::Blue, blue_milli);
        with.tick();
        without.tick();
        let blue = with.elixir_raw(Team::Blue).0;
        out.push((blue - without.elixir_raw(Team::Blue).0, with.elixir_raw(Team::Red).0 - without.elixir_raw(Team::Red).0, blue));
    }
    Zapped { d: out, ids, unit, s: with }
}

/// 1. Plant: mana_on_death_to_owner.
#[test]
fn a_death_pays_the_other_side_in_thousandths_of_an_elixir() {
    let Zapped { d, ids, unit, s } = zapped(knight_pays(), &[("Knight", AT)], 2000, 20);
    assert!(s.entity(ids[0]).is_none(), "the Knight survived the Zap: nothing was tested");
    let paid: Vec<(i64, i64)> = d.iter().map(|x| (x.0, x.1)).filter(|x| *x != (0, 0)).collect();
    assert_eq!(paid, vec![(unit / 2, 0)], "one payment of half an elixir to Blue, nothing to Red");
}

/// 2. economy.MANA_ON_DEATH_FOR_OPPONENT_UNIT = elixir.
#[test]
fn under_the_elixir_arm_the_same_row_fills_the_opponent_to_ten() {
    let mut c = knight_pays();
    c.calib.mana_for_opponent_unit = ForOpponentUnit::Elixir;
    let Zapped { d, ids, unit, s } = zapped(c, &[("Knight", AT)], 2000, 20);
    assert!(s.entity(ids[0]).is_none(), "the Knight survived the Zap");
    let cap = s.config().calib.max_mana as i64 * unit;
    let paid: Vec<i64> = d.iter().filter(|x| x.0 != 0).map(|x| x.2).collect();
    assert_eq!(paid, vec![cap], "500 elixir on the death tick, capped at 10");
}

/// 3. Two deaths on one tick. Plant: mana_on_death_to_owner.
#[test]
fn two_deaths_on_one_tick_pay_the_sum() {
    let Zapped { d, ids, unit, s } = zapped(knight_pays(), &[("Knight", AT), ("Knight", (AT.0 + 800, AT.1))], 2000, 20);
    assert!(ids.iter().all(|id| s.entity(*id).is_none()), "a Knight survived");
    let paid: Vec<(i64, i64)> = d.iter().map(|x| (x.0, x.1)).filter(|x| *x != (0, 0)).collect();
    assert_eq!(paid, vec![(unit, 0)], "two halves on the one tick both die on");
}

/// 4. The Elixir Golem, generation by generation. Plant: mana_on_death_to_owner.
#[test]
fn each_elixir_golem_generation_pays_the_opponent() {
    let c = level11();
    let db = c.cards.clone();
    let golem = db.index("ElixirGolem").unwrap_or_else(|| {
        panic!("ElixirGolem does not load ({:?}); its unit chain (ElixirGolem2 death-spawns ElixirGolem4) must load first", db.rejected.iter().find(|(n, _)| n == "ElixirGolem"))
    });
    assert_eq!(db.get(golem).mana.map(|m| m.on_death_for_opponent), Some(1000), "the 15.535.29 ElixirGolem1 row");
    let mut with = BattleState::new(0, c.clone());
    let mut without = BattleState::new(0, c);
    let id = with.scenario_spawn_now(Team::Red, "ElixirGolem", at(AT), Some(1)).expect("the golem");
    let unit = with.elixir_raw(Team::Blue).1;
    // Kill everything of `card` on Red's side (1 hp, a Zap on each) and return what Blue gained over the ticks it took.
    let generation = |with: &mut BattleState, without: &mut BattleState, card: &str| -> (usize, i64, i64) {
        let targets: Vec<(EntityId, Vec2)> = with.entities().filter(|v| v.card == card && v.team == Team::Red).map(|v| (v.id, v.pos)).collect();
        for (t, p) in &targets {
            with.debug_set_hp(*t, 1);
            with.spawn_unit(Team::Blue, "Zap", *p, None).expect("the Zap");
            without.spawn_unit(Team::Blue, "Zap", *p, None).expect("the Zap");
        }
        let (mut blue, mut red) = (0, 0);
        for _ in 0..10 {
            with.scenario_set_elixir_milli(Team::Blue, 1000);
            without.scenario_set_elixir_milli(Team::Blue, 1000);
            with.tick();
            without.tick();
            blue += with.elixir_raw(Team::Blue).0 - without.elixir_raw(Team::Blue).0;
            red += with.elixir_raw(Team::Red).0 - without.elixir_raw(Team::Red).0;
            if targets.iter().all(|(t, _)| with.entity(*t).is_none()) {
                break;
            }
        }
        assert!(targets.iter().all(|(t, _)| with.entity(*t).is_none()), "{card}: a unit survived its Zap");
        (targets.len(), blue, red)
    };
    assert!(with.entity(id).is_some());
    assert_eq!(generation(&mut with, &mut without, "ElixirGolem"), (1, unit, 0), "the golem: one elixir to Blue");
    assert_eq!(generation(&mut with, &mut without, "ElixirGolem2"), (2, unit, 0), "two golemites: half an elixir each");
    assert_eq!(generation(&mut with, &mut without, "ElixirGolem4"), (4, 2 * unit, 0), "four blobs: half an elixir each");
}

/// 5. The payment is capped.
#[test]
fn the_payment_is_capped_at_ten() {
    let Zapped { d, ids, unit, s } = zapped(knight_pays(), &[("Knight", AT)], 9_800, 20);
    assert!(s.entity(ids[0]).is_none(), "the Knight survived the Zap");
    let cap = s.config().calib.max_mana as i64 * unit;
    let paid: Vec<i64> = d.iter().filter(|x| x.0 != 0).map(|x| x.2).collect();
    assert_eq!(paid, vec![cap], "9.8 elixir, the tick's regen and half an elixir: exactly 10");
}
