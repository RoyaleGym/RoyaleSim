//! LEVEL SCALING AGAINST THE LIVE CLIENT (calibration.json combat.STAT_BASE_LEVEL).
//!
//! WHY IT EXISTS: the 15.535 card data (data/derived/cards.json) keeps its stats at
//! the UNIFIED level 1 on the object's own Common ladder, where the 2018 data kept
//! them at the card rarity's local level 1. The two readings agree on every
//! Common card and part on every other one: a Rare Hog Rider at unified level 11 is
//! 663 x 256 % = 1697 under the object ladder and 663 x 212 % = 1405 under the card's.
//! The live 16.402 captures publish every entity's `level` and `max_hp`, so the
//! reading is a measurement, not a choice: tests/fixtures/live_levels.json
//! (tools/make_live_levels_fixture.py) holds every distinct (card, level, max_hp) the
//! captures show, resolved to the object of that card whose 15.535 base reproduces it.
//!
//! THE CHECKS:
//!   1. `every_live_max_hp_is_the_engines_scaled_hitpoints`: for every resolved row
//!      whose card the loader simulates, `CardDb::scaled(unit, level, hitpoints)` ==
//!      max_hp -- through the same `level_multiplier` a battle uses. Vacuity: enough
//!      rows, and at least one NON-Common card at a level where the readings part.
//!   2. `the_card_rarity_reading_is_refuted_by_the_live_rows`: the other ledger
//!      candidate, computed here from the file's own rarities, misses those rows.
//!   3. `the_loader_refuses_a_reading_it_does_not_implement` and takes its rarities
//!      from the file (a Champion loads; a Rare has the file's level count);
//!   4. `the_fixture_measures_95_of_109_rows_and_names_every_miss`:
//!      the MEASUREMENT itself -- `rows_matched` 95 of 109, a named list of cards
//!      with a resolved row, and every unresolved row a known 16.402 balance delta,
//!      an action-graph object or a crown tower; the generator assigns `unit` with
//!      the engine's own formula, so without this the count could silently fall;
//!   5. THE CROWN TOWERS, calibration
//!      combat.TOWER_HITPOINT_LADDER: the seven tower rows of the fixture (king 3312 /
//!      4824 and princess 2030 / 3052 at tower levels 6 / 11) through BattleState::new
//!      at those tower levels, the princess tower's 109 damage at 11 (a live Prince's
//!      hp drops), the known king table 1..15, and the Common card ladder the engine
//!      ran before (6144 / 3584 at 11) refuted.
//!
//! PLANTS: `RUSTFLAGS='--cfg clash_plant="level_card_rarity_local"' CARGO_TARGET_DIR=target/plant
//! cargo test --test levels`: card.rs `level_multiplier` back to the card ladder from
//! its local level 1 -> check 1 red on every Rare / Epic / Legendary row;
//! `tower_card_ladder`: state.rs spawn_now scales the towers on the card ladder again
//! -> check 5 red.
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource};
use royalesim::entity::EntityKind;
use royalesim::state::{tower_multiplier_percent, BattleState, Calib, TowerLadder};
use royalesim::Team;
use serde_json::Value;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/live_levels.json");
const CARDS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json");

fn fixture() -> Value {
    let text = std::fs::read_to_string(FIXTURE).unwrap_or_else(|e| panic!("{FIXTURE}: {e} (tools/make_live_levels_fixture.py)"));
    serde_json::from_str(&text).unwrap()
}

/// (card, unit, level, max_hp) of every row the generator resolved to an object.
fn resolved_rows(doc: &Value) -> Vec<(String, String, i32, i32)> {
    doc["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["unit"].is_string())
        .map(|r| (r["card"].as_str().unwrap().to_string(), r["unit"].as_str().unwrap().to_string(), r["level"].as_i64().unwrap() as i32, r["max_hp"].as_i64().unwrap() as i32))
        .collect()
}

/// The CardDb index that carries `unit`'s stats for `card`: the card itself when the
/// unit is the card's own, else the summon-only unit the card loaded.
fn unit_index(db: &CardDb, card: &str, unit: &str) -> Option<u16> {
    let ci = db.index(card)?;
    let c = db.get(ci);
    if c.spell.is_none() && (unit == card || db.get(ci).name == unit) {
        return Some(ci);
    }
    // The card's own unit under another internal name (Goblins -> Goblin_Stab): the
    // card record IS that unit's row.
    let doc: Value = serde_json::from_str(&std::fs::read_to_string(CARDS).unwrap()).unwrap();
    let rec = doc["cards"].as_array().unwrap().iter().find(|x| x["name"] == card)?;
    if rec["summon_character"].as_str() == Some(unit) {
        return Some(ci);
    }
    db.index(unit).filter(|&u| db.get(u).summon_only || db.get(u).spell.is_none())
}

#[test]
fn every_live_max_hp_is_the_engines_scaled_hitpoints() {
    let db = cards();
    let doc = fixture();
    assert_eq!(doc["level_base_reading"].as_str(), Some("object_rarity_local_1"), "the fixture was resolved under another reading; regenerate it");
    let mut checked = 0;
    let mut parted = Vec::new();
    let mut misses = Vec::new();
    for (card, unit, level, max_hp) in resolved_rows(&doc) {
        let Some(idx) = unit_index(&db, &card, &unit) else { continue };
        let c = db.get(idx);
        let got = db.scaled(idx, level, c.hitpoints).unwrap_or_else(|e| panic!("{card}/{unit} at {level}: {e}"));
        if got != max_hp {
            misses.push(format!("{card}/{unit} L{level}: engine {got}, live {max_hp}"));
        }
        checked += 1;
        // Where the two readings part: a non-Common card whose card-local ladder
        // entry differs from the unified one.
        let r = db.rarity(&c.rarity).unwrap();
        if r.relative_level > 0 && level > r.relative_level + 1 {
            parted.push((card.clone(), level));
        }
    }
    println!("live levels: {checked} rows through the engine, {} at a level where the readings part", parted.len());
    assert!(misses.is_empty(), "{} of {checked} live rows disagree with the engine's level arithmetic:\n{}", misses.len(), misses.join("\n"));
    assert!(checked >= 40, "vacuous: only {checked} live rows reached the engine");
    assert!(parted.len() >= 5, "vacuous: no non-Common card at a level where the readings part: {parted:?}");
    assert!(parted.iter().any(|(c, _)| c == "HogRider"), "the Rare Hog Rider must be among the rows ({parted:?})");
}

#[test]
fn the_card_rarity_reading_is_refuted_by_the_live_rows() {
    // The other combat.STAT_BASE_LEVEL candidate, computed from the file's own
    // rarities: the CARD rarity's ladder entered at its local level 1.
    let db = cards();
    let doc = fixture();
    let mut refuted = 0;
    let mut agreed = 0;
    for (card, unit, level, max_hp) in resolved_rows(&doc) {
        let Some(idx) = unit_index(&db, &card, &unit) else { continue };
        let c = db.get(idx);
        let r = db.rarity(&c.rarity).unwrap();
        let local = level - r.relative_level;
        let pct = if local == 1 { 100 } else { r.multipliers[(local - 2) as usize] };
        let card_ladder = CardDb::scale(c.hitpoints, pct);
        if r.relative_level > 0 && local >= 2 {
            if card_ladder == max_hp {
                agreed += 1;
            } else {
                refuted += 1;
            }
        }
    }
    assert!(refuted >= 5 && agreed == 0, "card_rarity_local_1: {agreed} live rows agree, {refuted} refute it");
}

#[test]
fn the_loader_takes_its_rarities_from_the_file_and_refuses_an_unknown_reading() {
    let text = std::fs::read_to_string(CARDS).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    let db = CardDb::from_json_str(&text, CardSource::DerivedJson).unwrap();
    // Every rarity of the file, with its level count and relative level.
    for (name, r) in doc["rarities"].as_object().unwrap() {
        let row = db.rarity(name).unwrap_or_else(|| panic!("rarity {name} not loaded"));
        assert_eq!(row.level_count as i64, r["level_count"].as_i64().unwrap(), "{name}");
        assert_eq!(row.relative_level as i64, r["relative_level"].as_i64().unwrap(), "{name}");
        assert_eq!(row.multipliers.len() + 1, r["multiplier_percent_by_level"].as_array().unwrap().len(), "{name}");
    }
    // A card at the top of its rarity's range loads a multiplier; one past it is refused.
    let hog = db.index("HogRider").unwrap();
    let rare = db.rarity(&db.get(hog).rarity).unwrap();
    let top = rare.relative_level + rare.level_count;
    assert!(db.level_multiplier(hog, top).is_ok(), "HogRider at {top}");
    assert!(db.level_multiplier(hog, top + 1).is_err());
    assert!(db.level_multiplier(hog, rare.relative_level).is_err(), "below the card's range");
    // A Champion card is a card of the file's table (rejected for its mechanic, never
    // for its rarity): none is rejected with "rarity ... not in".
    assert!(db.rarity("Champion").is_some());
    assert!(db.rejected.iter().all(|(_, why)| !why.contains("rarity Champion not in")), "{:?}", db.rejected);
    // An unimplemented reading refuses the card, out loud.
    let mut d = doc.clone();
    let knight = d["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "Knight").unwrap();
    knight["level_scaling"]["reading"] = Value::from("king_level_ladder");
    let db2 = CardDb::from_json_str(&d.to_string(), CardSource::DerivedJson).unwrap();
    assert!(db2.index("Knight").is_none());
    let (_, why) = db2.rejected.iter().find(|(n, _)| n == "Knight").unwrap();
    assert!(why.contains("king_level_ladder") && why.contains("not implemented"), "{why}");
    // The object reading without its base level is a data error, not a default.
    let mut d = doc.clone();
    let knight = d["cards"].as_array_mut().unwrap().iter_mut().find(|c| c["name"] == "Knight").unwrap();
    knight["level_scaling"]["base_level"] = Value::Null;
    let db3 = CardDb::from_json_str(&d.to_string(), CardSource::DerivedJson).unwrap();
    assert!(db3.index("Knight").is_none());
    // A file without a rarities block falls back to the shipped 2018 table.
    let mut d = doc.clone();
    d.as_object_mut().unwrap().remove("rarities");
    let db4 = CardDb::from_json_str(&d.to_string(), CardSource::DerivedJson).unwrap();
    assert!(db4.rarity("Champion").is_none());
    assert_eq!(db4.rarity("Rare").unwrap().level_count, royalesim::card::shipped_rarities().iter().find(|r| r.name == "Rare").unwrap().level_count);
}

#[test]
fn the_fixture_measures_95_of_109_rows_and_names_every_miss() {
    // The 95 / 109 the ledger's HIGH confidence rests on (combat.STAT_BASE_LEVEL),
    // pinned here rather than trusted from the generator's own count.
    let doc = fixture();
    let rows = doc["rows"].as_array().unwrap();
    assert_eq!((doc["rows_matched"].as_i64(), doc["rows_total"].as_i64()), (Some(95), Some(109)), "the measurement moved: regenerate and re-read");
    let resolved = resolved_rows(&doc);
    assert_eq!(resolved.len(), 95, "the generator's count and its rows disagree");
    assert_eq!(rows.len(), 109);
    for card in ["Knight", "Giant", "Musketeer", "HogRider", "Prince", "DarkPrince", "BattleRam", "Witch", "Tombstone", "Tesla", "Skeletons", "Goblins", "MiniPekka", "Golem", "LavaHound"] {
        assert!(resolved.iter().any(|(c, _, _, _)| c == card), "no resolved live row for {card}");
    }
    // Every unresolved row is one of the known kinds: a crown tower (7 rows, the
    // tower ladder's own measurement below), a 16.402 balance delta on a 15.535
    // object, or an object an action graph spawns.
    let known_misses = ["IceSpirits", "IceGolemite", "FirespiritHut", "GoblinCage", "GoblinDrill", "Heal"];
    let mut towers = 0;
    for r in rows.iter().filter(|r| !r["unit"].is_string()) {
        if r["tower"].as_bool() == Some(true) {
            towers += 1;
            continue;
        }
        let card = r["card"].as_str().unwrap_or("?");
        assert!(known_misses.contains(&card), "an unresolved row that is not a known miss: {r}");
        assert!(r["note"].as_str().is_some_and(|n| n.contains("16.402") || n.contains("action graph")), "an unresolved row without its reason: {r}");
    }
    assert_eq!(towers, 7, "the seven crown-tower rows");
}

#[test]
fn the_crown_towers_scale_on_the_globals_percent_ladder_the_live_captures_measured() {
    // Plant tower_card_ladder. combat.TOWER_HITPOINT_LADDER = globals_percent_per_level_compound_floor.
    let calib = Calib::shipped();
    assert_eq!(calib.tower_ladder, TowerLadder::GlobalsPercentPerLevelCompoundFloor, "the shipped arm this test pins");
    let doc = fixture();
    // The live rows: per tower level, the set of max_hp the captures show on crown
    // towers (the captures' kind does not tell king from princess; the pair does).
    let mut live: std::collections::BTreeMap<i32, std::collections::BTreeSet<i32>> = Default::default();
    for r in doc["rows"].as_array().unwrap().iter().filter(|r| r["tower"].as_bool() == Some(true)) {
        live.entry(r["level"].as_i64().unwrap() as i32).or_default().insert(r["max_hp"].as_i64().unwrap() as i32);
    }
    assert_eq!(live.keys().copied().collect::<Vec<_>>(), vec![6, 11], "vacuous: the fixture's tower levels");
    for (level, hps) in &live {
        let mut cfg = config();
        cfg.tower_level = [*level, *level];
        let s = BattleState::new(1, cfg);
        let mine: std::collections::BTreeSet<i32> = s.entities().filter(|e| matches!(e.kind, EntityKind::KingTower | EntityKind::PrincessTower)).map(|e| e.max_hp).collect();
        assert_eq!(&mine, hps, "tower level {level}: the engine's king / princess hitpoints against the live rows");
        // and the king is the larger of the pair
        let king = s.entities().find(|e| e.kind == EntityKind::KingTower && e.team == Team::Blue).unwrap().max_hp;
        assert_eq!(king, *hps.iter().max().unwrap(), "tower level {level}: the king");
    }
    // The princess tower's damage at 11: the Prince of capture 20260920-003751-A
    // lost 109 per tower hit (1920 -> 1811 -> 1702 -> 1593) = 50 x 218 %.
    let pct = tower_multiplier_percent(&calib, EntityKind::PrincessTower, 11, calib.tower_dmg_pct).unwrap();
    assert_eq!(pct, 218);
    let princess = card_stat(&BattleState::new(1, config()), "PrincessTower").damage;
    assert_eq!(CardDb::scale(princess, pct), 109, "the princess tower's damage at tower level 11");
    // The known king table, every level 1..15 (the formula's own check beyond the
    // two measured levels).
    let table = [2400, 2568, 2736, 2904, 3096, 3312, 3528, 3768, 4008, 4392, 4824, 5304, 5832, 6408, 7032];
    let king_base = card_stat(&BattleState::new(1, config()), "KingTower").hitpoints;
    assert_eq!(king_base, 2400, "data: the king tower row");
    for (i, want) in table.iter().enumerate() {
        let level = i as i32 + 1;
        let pct = tower_multiplier_percent(&calib, EntityKind::KingTower, level, calib.tower_hp_pct).unwrap();
        assert_eq!(CardDb::scale(king_base, pct), *want, "king tower at level {level}");
    }
    // The Common card ladder the engine ran before is refuted at both measured levels.
    let mut cfg = config();
    cfg.calib.tower_ladder = TowerLadder::CommonCardLadder;
    cfg.tower_level = [11, 11];
    let s = BattleState::new(1, cfg);
    let old: std::collections::BTreeSet<i32> = s.entities().filter(|e| matches!(e.kind, EntityKind::KingTower | EntityKind::PrincessTower)).map(|e| e.max_hp).collect();
    assert_eq!(old, [3584, 6144].into_iter().collect(), "the card ladder's towers at 11 (x 256 %)");
    assert_ne!(old, live[&11], "the card ladder is refuted by the live rows");
}
