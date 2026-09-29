//! EVERY UNIT A CARD PUTS ON THE BOARD, WITH ITS HITPOINTS (py.rs `unit_hitpoint_rows`, the body of
//! `Battle.unit_hitpoints(card_id, level)`): the card's own row first, then each unit down its chain at the level it
//! takes, with its role.
//!
//! WHAT IS PINNED (the level-11 figures are the 15.535.29 tables' through the shipped card data; each test also runs a
//! second level, whose different figures show the rows follow the level asked for):
//!   1. for every catalogue card at 11 and at 13 (where its ladder has the level), the own row is ("own", its unit's
//!      row name, the catalogue's hitpoints column); a spell has no own row and the catalogue shows 0;
//!   2. a death spawn and a second summon are listed: the Golem [own Golem 5120, death_spawn Golemite 1039] (at 13:
//!      6180, 1254), the Rascals [own RascalBoy 1832, second_summon RascalGirl 261];
//!   3. the Tri Wizards list their three units at the hitpoints the client showed them at, level 11 (client 15.535.29,
//!      sweep-TriWizards): TriWizard 755, Electro Wizard 714, Ice Wizard 688;
//!   4. a unit a card puts down two ways is one row per way (the Tombstone's Skeleton: spawn and death_spawn), a
//!      spell lists its release (the Goblin Barrel's Goblin), a chain goes down (the Elixir Golem's ElixirGolem2 and
//!      ElixirGolem4), and the Mirror puts down nothing of its own;
//!   5. a level the card's ladder lacks is refused, naming it.
mod common;

use common::*;
use royalesim::card::CardDb;
use royalesim::py::{catalogue_rows, unit_hitpoint_rows, UNIT_ROLES};
use royalesim::state::Calib;

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
    let db = cards();
    let calib = Calib::shipped();
    let catalogue: Vec<u16> = (0..db.cards.len() as u16)
        .filter(|i| {
            let c = db.get(*i);
            !c.summon_only && c.evo.is_none() && c.form_of.is_none() && c.name != royalesim::card::KING_TOWER && c.name != royalesim::card::PRINCESS_TOWER && db.index(&c.name) == Some(*i)
        })
        .collect();
    assert_eq!(catalogue.len(), 132, "scene: the default catalogue's 132 cards");
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
    let db = cards();
    assert_eq!(rows(&db, "Golem", 11), want(&[("own", "Golem", 5120), ("death_spawn", "Golemite", 1039)]));
    assert_eq!(rows(&db, "Golem", 13), want(&[("own", "Golem", 6180), ("death_spawn", "Golemite", 1254)]), "each at the level it takes");
    assert_eq!(rows(&db, "Rascals", 11), want(&[("own", "RascalBoy", 1832), ("second_summon", "RascalGirl", 261)]));
}

// ---------------------------------------------------------------------------
// 3. the Tri Wizards

#[test]
fn the_tri_wizards_list_their_three_units_at_the_clients_hitpoints() {
    let db = cards();
    assert_eq!(rows(&db, "TriWizards", 11), want(&[("own", "TriWizard", 755), ("second_summon", "ElectroWizard", 714), ("second_summon", "IceWizard", 688)]));
    assert_eq!(rows(&db, "TriWizards", 13), want(&[("own", "TriWizard", 911), ("second_summon", "ElectroWizard", 862), ("second_summon", "IceWizard", 831)]));
}

// ---------------------------------------------------------------------------
// 4. two ways, a spell, a chain, nothing

#[test]
fn every_way_a_unit_comes_is_a_row() {
    let db = cards();
    assert_eq!(rows(&db, "Tombstone", 11), want(&[("own", "Tombstone", 529), ("spawn", "Skeleton", 81), ("death_spawn", "Skeleton", 81)]));
    assert_eq!(rows(&db, "GoblinBarrel", 11), want(&[("release", "Goblin", 202)]), "a spell: its release, no own row");
    assert_eq!(rows(&db, "ElixirGolem", 11), want(&[("own", "ElixirGolem1", 1569), ("death_spawn", "ElixirGolem2", 762), ("death_spawn", "ElixirGolem4", 360)]));
    assert_eq!(rows(&db, "Mirror", 11), want(&[]), "the Mirror puts down nothing of its own");
}

// ---------------------------------------------------------------------------
// 5. a level the ladder lacks

#[test]
fn a_level_the_ladder_lacks_is_refused_by_name() {
    let db = cards();
    let calib = Calib::shipped();
    let e = unit_hitpoint_rows(&db, &calib, db.index("Knight").unwrap(), 99).expect_err("level 99 is refused");
    assert!(e.contains("level 99"), "the refusal names the level: {e}");
    let e = unit_hitpoint_rows(&db, &calib, db.index("TriWizards").unwrap(), 8).expect_err("a Legendary card at 8 is refused");
    assert!(e.contains("level 8"), "the refusal names the level: {e}");
}
