//! cards.CLIENT16402_VALUES, the HitSpeed and LoadTime columns: a unit's attack clock (card.rs `CardDb::with_values`,
//! `CardColumn`), on the table the overlay names, the 160402017 one included (cards.CARD_TABLE).
//!
//! WHY. Paired replays of post-2026-10-06 ladder battles score the Minion Giant's new row (HitSpeed 1700, LoadTime 900,
//! client files) against the old (1500 / 700); these columns let one card's clock move on the current table without a
//! table edit. Pinned:
//!   1. on the 160402017 table a blue Minion Giant hits a held red Cannon every 34 ticks (1700 ms); listed at HitSpeed
//!      1500 and LoadTime 700, every 30, and the battle's row reads 1500 and 700;
//!   2. the columns are refused on a row that does not attack (a spell) and for a value that is not a time.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="attack_clock_values_unread"' CARGO_TARGET_DIR=target/plant cargo test --test
//! attack_clock_values`): the columns are accepted and the row keeps its clock: (1) goes red, (2) stays green.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardColumn, CardDb, CardValue};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, CardTable, CardValuesArm};
use royalesim::{EntityId, Team};

const V160402017: &str = "cards-160402017-20261006.1";

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The 160402017 table's battle config, the overlay listing exactly `listed` (card, column, value) on that table.
fn config_160402017(listed: &[(&str, CardColumn, i32)]) -> BattleConfig {
    let db = CardDb::load_table(CardTable::Client160402017).expect("the 160402017 table");
    let mut cfg = BattleConfig::with_cards(db);
    cfg.calib.card_table = CardTable::Client160402017;
    cfg.calib.card_values = CardValuesArm::Client16402;
    cfg.calib.card_values_table = V160402017.to_string();
    cfg.calib.card_value_overrides = listed.iter().map(|(c, col, v)| CardValue { card: c.to_string(), column: *col, value: *v }).collect();
    cfg.decks = [vec!["MinionGiant".into()], vec!["Cannon".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

/// The ticks a held red Cannon loses a blue Minion Giant's hit on (a loss above its decay), over 140 ticks.
fn hit_ticks(cfg: BattleConfig) -> (Vec<u32>, i32, i32) {
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let mg = s.scenario_spawn_now(Team::Blue, "MinionGiant", n(9000, 20000), None).expect("the Minion Giant");
    let cannon: EntityId = s.scenario_spawn_now(Team::Red, "Cannon", n(9000, 23500), None).expect("the Cannon");
    let c = card_stat(&s, "MinionGiant");
    let clock = (c.hit_speed_ms, c.load_time_ms);
    let mut hits = Vec::new();
    for t in 0..140u32 {
        let top = s.entity(cannon).expect("the Cannon").max_hp;
        assert!(s.debug_set_pos(mg, n(9000, 20000)) && s.debug_set_hp(cannon, top));
        s.tick();
        if top - s.entity(cannon).expect("the Cannon").hp > 20 {
            hits.push(t);
        }
    }
    (hits, clock.0, clock.1)
}

#[test]
fn the_columns_move_one_units_attack_clock_on_the_160402017_table() {
    let gaps = |h: &[u32]| h.windows(2).map(|w| w[1] - w[0]).collect::<Vec<_>>();
    let (plain, hs, lt) = hit_ticks(config_160402017(&[]));
    assert_eq!((hs, lt), (1700, 900), "the 160402017 row");
    assert!(plain.len() >= 3 && gaps(&plain).iter().all(|g| *g == 34), "1700 ms: {plain:?}");
    let (listed, hs, lt) = hit_ticks(config_160402017(&[("MinionGiant", CardColumn::HitSpeed, 1500), ("MinionGiant", CardColumn::LoadTime, 700)]));
    assert_eq!((hs, lt), (1500, 700), "the listed clock reaches the battle's row");
    assert!(listed.len() >= 3 && gaps(&listed).iter().all(|g| *g == 30), "1500 ms: {listed:?}");
}

#[test]
fn the_columns_are_refused_on_a_row_that_does_not_attack_and_for_a_non_time() {
    let db = CardDb::load_table(CardTable::Client160402017).expect("the 160402017 table");
    let one = |card: &str, column: CardColumn, value: i32| db.with_values(&[CardValue { card: card.into(), column, value }]);
    assert!(one("Fireball", CardColumn::HitSpeed, 1500).is_err(), "a spell has no attack clock");
    assert!(one("MinionGiant", CardColumn::HitSpeed, 0).is_err(), "0 is not a HitSpeed");
    assert!(one("MinionGiant", CardColumn::LoadTime, -1).is_err(), "-1 is not a LoadTime");
    assert!(one("MinionGiant", CardColumn::LoadTime, 0).is_ok(), "0 is a LoadTime");
}
