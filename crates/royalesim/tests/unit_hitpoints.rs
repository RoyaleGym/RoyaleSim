//! EVERY UNIT A CARD PUTS ON THE BOARD, WITH ITS HITPOINTS (py.rs `unit_hitpoint_rows`, the body of
//! `Battle.unit_hitpoints(card_id, level)`): the card's own row first, then each unit down its chain at the level it
//! takes, with its role.
//!
//! WHAT IS PINNED (the figures are the 15.535.29 tables' through the shipped card data, `shipped_data`: the committed
//! table with cards.CLIENT16402_VALUES applied, as `Battle.unit_hitpoints` reads it; each test also runs a second
//! level, whose different figures show the rows follow the level asked for):
//!   1. for every catalogue card at 11 and at 13 (where its ladder has the level), the own row is ("own", its unit's
//!      row name, the catalogue's hitpoints column); a spell has no own row and the catalogue shows 0;
//!   2. a death spawn and a second summon are listed: the Golem [own Golem 5120, death_spawn Golemite 1039] (at 13:
//!      6180, 1254), the Rascals [own RascalBoy 1832, second_summon RascalGirl 261];
//!   3. the Tri Wizards list their three units at the hitpoints the client showed them at, level 11 (client 15.535.29,
//!      sweep-TriWizards): TriWizard 755, Electro Wizard 714, Ice Wizard 688;
//!   4. a unit a card puts down two ways is one row per way (the Tombstone's Skeleton: spawn and death_spawn), a
//!      spell lists its release (the Goblin Barrel's Goblin), a chain goes down (the Elixir Golem's ElixirGolem2 and
//!      ElixirGolem4), and the Mirror puts down nothing of its own;
//!   5. a level the card's ladder lacks is refused, naming it;
//!   6. the rows are the battle's card data, not the committed table's: over every catalogue card at 11 and 13 the
//!      overlay moves the rows of exactly six cards at each level (the Ice Spirits, the Fire Spirits, the Ice Golem,
//!      the Furnace's Fire Spirits, the Goblin Cage's Brawler, the Heal's spirit), the Brawler 1121 against the
//!      table's 1080 and the Ice Spirits 215 against 217 at 11. Tests 1 to 5 read `shipped_data`, so a figure the
//!      overlay moves is pinned at the value a battle plays; none of the figures 2 to 4 pin is one it moves.
mod common;

use common::*;
use royalesim::card::CardDb;
use royalesim::py::{catalogue_rows, unit_hitpoint_rows, UNIT_ROLES};
use royalesim::state::Calib;
use std::sync::Arc;

/// The card data a battle on the shipped ledger runs, as `Battle.unit_hitpoints` reads it: the committed table with
/// cards.CLIENT16402_VALUES applied (`Calib::card_data`).
fn shipped_data() -> CardDb {
    (*Calib::shipped().card_data(Arc::new(cards())).expect("the shipped card data")).clone()
}

/// Every default catalogue card (136, the Minion Giant last), by index.
fn catalogue(db: &CardDb) -> Vec<u16> {
    (0..db.cards.len() as u16)
        .filter(|i| {
            let c = db.get(*i);
            !c.summon_only && c.evo.is_none() && c.form_of.is_none() && c.name != royalesim::card::KING_TOWER && c.name != royalesim::card::PRINCESS_TOWER && db.index(&c.name) == Some(*i)
        })
        .collect()
}

fn rows(db: &CardDb, card: &str, level: i32) -> Vec<(&'static str, String, i32)> {
    let idx = db.index(card).unwrap_or_else(|| panic!("{card} loads"));
    unit_hitpoint_rows(db, &Calib::shipped(), idx, level).unwrap_or_else(|e| panic!("{card} at {level}: {e}"))
}

fn want(r: &[(&'static str, &str, i32)]) -> Vec<(&'static str, String, i32)> {
    r.iter().map(|(a, b, c)| (*a, b.to_string(), *c)).collect()
}

// ---------------------------------------------------------------------------
// 1. the own row is the catalogue's

#[test]
fn the_own_row_is_the_catalogues_hitpoints_for_every_card() {
    let db = shipped_data();
    let calib = Calib::shipped();
    let catalogue = catalogue(&db);
    assert_eq!(catalogue.len(), 136, "scene: the default catalogue's 136 cards");
    let mut checked = 0;
    for level in [11, 13] {
        for &i in &catalogue {
            if db.level_multiplier(i, level).is_err() {
                continue;
            }
            let row: serde_json::Value = serde_json::from_str(&catalogue_rows(&db, &calib, &[i], level).expect("a catalogue row")).expect("JSON");
            let catalogue_hp = row[0][6].as_i64().expect("the hitpoints column") as i32;
            let got = unit_hitpoint_rows(&db, &calib, i, level).unwrap_or_else(|e| panic!("{} at {level}: {e}", db.get(i).name));
            assert!(got.iter().all(|(r, _, _)| UNIT_ROLES.contains(r)), "{}: a role outside UNIT_ROLES: {got:?}", db.get(i).name);
            match got.first() {
                Some(("own", _, hp)) => assert_eq!(*hp, catalogue_hp, "{} at {level}: the own row against the catalogue", db.get(i).name),
                _ => assert_eq!(catalogue_hp, 0, "{} at {level}: no own row, and the catalogue shows {catalogue_hp}", db.get(i).name),
            }
            checked += 1;
        }
    }
    assert!(checked >= 2 * 100, "vacuous: only {checked} card-levels compared");
    let knight = |l| rows(&db, "Knight", l);
    assert_eq!((knight(11), knight(13)), (want(&[("own", "Knight", 1766)]), want(&[("own", "Knight", 2132)])), "the Knight's own row at 11 and 13");
}

// ---------------------------------------------------------------------------
// 2. a death spawn and a second summon

#[test]
fn a_death_spawn_and_a_second_summon_are_listed() {
    let db = shipped_data();
    assert_eq!(rows(&db, "Golem", 11), want(&[("own", "Golem", 5120), ("death_spawn", "Golemite", 1039)]));
    assert_eq!(rows(&db, "Golem", 13), want(&[("own", "Golem", 6180), ("death_spawn", "Golemite", 1254)]), "each at the level it takes");
    assert_eq!(rows(&db, "Rascals", 11), want(&[("own", "RascalBoy", 1832), ("second_summon", "RascalGirl", 261)]));
}

// ---------------------------------------------------------------------------
// 3. the Tri Wizards

#[test]
fn the_tri_wizards_list_their_three_units_at_the_clients_hitpoints() {
    let db = shipped_data();
    assert_eq!(rows(&db, "TriWizards", 11), want(&[("own", "TriWizard", 755), ("second_summon", "ElectroWizard", 714), ("second_summon", "IceWizard", 688)]));
    assert_eq!(rows(&db, "TriWizards", 13), want(&[("own", "TriWizard", 911), ("second_summon", "ElectroWizard", 862), ("second_summon", "IceWizard", 831)]));
}

// ---------------------------------------------------------------------------
// 4. two ways, a spell, a chain, nothing

#[test]
fn every_way_a_unit_comes_is_a_row() {
    let db = shipped_data();
    assert_eq!(rows(&db, "Tombstone", 11), want(&[("own", "Tombstone", 529), ("spawn", "Skeleton", 81), ("death_spawn", "Skeleton", 81)]));
    assert_eq!(rows(&db, "GoblinBarrel", 11), want(&[("release", "Goblin", 202)]), "a spell: its release, no own row");
    assert_eq!(rows(&db, "ElixirGolem", 11), want(&[("own", "ElixirGolem1", 1569), ("death_spawn", "ElixirGolem2", 762), ("death_spawn", "ElixirGolem4", 360)]));
    assert_eq!(rows(&db, "Mirror", 11), want(&[]), "the Mirror puts down nothing of its own");
}

// ---------------------------------------------------------------------------
// 5. a level the ladder lacks

#[test]
fn a_level_the_ladder_lacks_is_refused_by_name() {
    let db = shipped_data();
    let calib = Calib::shipped();
    let e = unit_hitpoint_rows(&db, &calib, db.index("Knight").unwrap(), 99).expect_err("level 99 is refused");
    assert!(e.contains("level 99"), "the refusal names the level: {e}");
    let e = unit_hitpoint_rows(&db, &calib, db.index("TriWizards").unwrap(), 8).expect_err("a Legendary card at 8 is refused");
    assert!(e.contains("level 8"), "the refusal names the level: {e}");
}

// ---------------------------------------------------------------------------
// 6. the battle's card data, not the table's

#[test]
fn the_rows_are_the_battles_card_data_not_the_tables() {
    let (shipped, table) = (shipped_data(), cards());
    let calib = Calib::shipped();
    let mut moved: Vec<(String, i32)> = Vec::new();
    for level in [11, 13] {
        for i in catalogue(&table) {
            if table.level_multiplier(i, level).is_err() {
                continue;
            }
            let on = |db: &CardDb| unit_hitpoint_rows(db, &calib, i, level).unwrap_or_else(|e| panic!("{} at {level}: {e}", table.get(i).name));
            if on(&shipped) != on(&table) {
                moved.push((table.get(i).name.clone(), level));
            }
        }
    }
    let six = ["IceSpirits", "FireSpirits", "IceGolemite", "FirespiritHut", "GoblinCage", "Heal"];
    let want_moved: Vec<(String, i32)> = [11, 13].into_iter().flat_map(|l| six.iter().map(move |n| (n.to_string(), l))).collect();
    assert_eq!(moved, want_moved, "the card-levels whose rows the card-value overlay moves");
    assert_eq!(rows(&shipped, "GoblinCage", 11), want(&[("own", "GoblinCage", 780), ("death_spawn", "GoblinBrawler", 1121)]), "the Brawler on the battle's card data");
    assert_eq!(rows(&table, "GoblinCage", 11), want(&[("own", "GoblinCage", 780), ("death_spawn", "GoblinBrawler", 1080)]), "the Brawler in the committed table");
    // The Evo Goblin Cage's Brawler, a row of its own, plays the plain Brawler's hitpoints.
    let ev1 = |db: &CardDb| {
        let i = db.index("GoblinCage_EV1_GoblinBrawler").expect("the evolved Brawler's row");
        db.scaled(i, 11, db.get(i).hitpoints).expect("level 11")
    };
    assert_eq!((ev1(&shipped), ev1(&table)), (1121, 1080), "the evolved Brawler: the battle's 1121, the table's 1080");
    assert_eq!((rows(&shipped, "IceSpirits", 11), rows(&table, "IceSpirits", 11)), (want(&[("own", "IceSpirits", 215)]), want(&[("own", "IceSpirits", 217)])), "an own row: the battle's 215, the table's 217");
}
