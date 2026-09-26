//! ONE ENUMERATION OF WHAT A CARD PUTS ON THE BOARD (card.rs `CardDb::unit_refs`).
//!
//! Five readers walk a card's unit-producing blocks: level validation
//! (`CardDb::check_levels`), the catalogue attribution (py.rs `ids_of_indices`, the
//! card id every exported entity row carries), the death-bomb path check and the
//! rejected-card cleanup (both in `CardDb::from_json_str`), and the replay harness's
//! rooting (examples/replay_parity/harness.rs `Roots::new`). They used to walk them by
//! hand, five lists that had to agree; all five read `unit_refs` now. So a block that
//! `unit_refs` misses is missed by all five at once, and no agreement between them can
//! show it. The checks here hold `unit_refs` against something else: the rows of a
//! synthetic file, the CardDef FIELDS (`field_refs` below, the one hand list left,
//! which a new unit-producing block must join), and the unit indices in each record's
//! Debug text, which no list has to name.
//!
//! THE CHECKS:
//!   1. `every_unit_block_is_enumerated`: on the synthetic file, each card's list is
//!      exactly the blocks its rows carry, by unit name, in order, with the spell
//!      release's level index; on every record of both shipped files, `unit_refs` is
//!      exactly `field_refs`; and on every record of all three files, `unit_refs` is
//!      as long as the record's Debug text has `unit: <n>` fields, so a block that
//!      holds a unit and that neither list names still shows;
//!   2. `every_registered_cards_units_pass_their_level_checks`: both shipped files,
//!      every registered card at every level it has: each unit resolves, exists at its
//!      level, and `check_levels` agrees;
//!   3. `every_unit_a_catalogue_card_puts_on_the_board_reports_its_card`: in the
//!      default catalogue (py.rs `Battle(card_names=None)`), every unit a catalogue
//!      card's FIELDS name gets a card id, never -1 -- both shipped files and the
//!      synthetic one, where the id is pinned to the first card that names the unit;
//!   4. `a_rejected_card_keeps_no_unit_block`: cards rejected after their push, one
//!      carrying a spawner, a death spawn, a second summon and a death area effect and
//!      one a spell release, end with every block dropped, read off the fields;
//!   5. `summon_only_numbering_is_breadth_first`: the summon-only records follow every
//!      card, numbered in first-need order. That is breadth-first over ONE level: a
//!      unit that itself puts units on the board is still refused as a spawn chain,
//!      and this test asserts the refusal, so the change that lifts it must replace
//!      that assertion with the second level's order. A death area's units are the
//!      dying card's needs, at its own level; no accepted area has any yet, and the
//!      change that gives one some pins their place here too;
//!   6. `a_unit_that_fails_a_check_is_caught_through_every_block`: the failing
//!      direction of the two checks that read `unit_refs`, on a synthetic file.
//!      (a) A Common card whose unit is a Legendary row, one per block (spawner,
//!      death spawn, second summon, spell release): `check_levels` at unified 1 is Err
//!      and names the unit, and passes at 9, where Legendary starts. (b) A death bomb
//!      named by a spawner, a spell release and a second summon: each card is refused
//!      with the exact text; the card that death-spawns it loads.
//!
//! PLANT: `RUSTFLAGS='--cfg clash_plant="unit_refs_skips_new_paths"'
//! CARGO_TARGET_DIR=target/plant cargo test --test unit_refs`: `unit_refs` drops the
//! second summon -> 1, 3, 4 and 6 red (6 by its second-summon cases: the level check
//! passes and the bomb card loads). 2 and 5 stay green: a block the enumeration skips
//! skips its level check silently (the defect itself, which 6 shows failing), and the
//! numbering does not read `unit_refs`.

mod common;

use common::*;
use royalesim::card::{CardDb, CardDef, CardSource, SpellDef, SpellShape, UnitRef, KING_TOWER, PRINCESS_TOWER};
use royalesim::py::ids_of_indices;

/// A file with every unit block this loader reads, each unit a distinct row:
///   Alpha   a spawner (Imp) and a death spawn (Ghoul);
///   Beta    a spawner (Imp, shared) and a second summon (Squire);
///   Gamma   a spell releasing Wisp, SpawnCharacterLevelIndex 2;
///   Delta   a death spawn (Nester) whose row death-spawns Imp itself: a chain;
///   Omega   a spawner, a death spawn, a death area effect and a second summon
///           (Nobody) with no `units` row;
///   Sigma   a spell releasing Nobody.
/// No towers: the fallback pair follows the units.
const SYNTH: &str = r#"{ "version": "test", "cards": [
 { "name":"Alpha", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":300, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500,
   "spawner":{"character":"Imp", "number":1, "pause_time_ms":5000}, "death_spawn":{"character":"Ghoul", "count":2} },
 { "name":"Beta", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":310, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500,
   "spawner":{"character":"Imp", "number":1, "pause_time_ms":5000}, "second_summon":{"character":"Squire", "count":2} },
 { "name":"Gamma", "kind":"spell", "elixir":3, "rarity":"Common",
   "projectile":{"name":"GammaBarrel", "speed":400, "spawn_character":"Wisp", "spawn_character_count":3,
   "spawn_character_level_index":2} },
 { "name":"Delta", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":320, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"Nester", "count":1} },
 { "name":"Omega", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":330, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500,
   "spawner":{"character":"Imp", "number":1, "pause_time_ms":5000}, "death_spawn":{"character":"Ghoul", "count":2},
   "death_area_effect":"OmegaArea", "second_summon":{"character":"Nobody", "count":1} },
 { "name":"Sigma", "kind":"spell", "elixir":2, "rarity":"Common",
   "projectile":{"name":"SigmaBarrel", "speed":400, "spawn_character":"Nobody", "spawn_character_count":1} }
 ],
 "units": {
  "Imp":    { "name":"Imp", "rarity":"Common", "hitpoints":80, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Ghoul":  { "name":"Ghoul", "rarity":"Common", "hitpoints":90, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Squire": { "name":"Squire", "rarity":"Common", "hitpoints":100, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Wisp":   { "name":"Wisp", "rarity":"Common", "hitpoints":110, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Nester": { "name":"Nester", "rarity":"Common", "hitpoints":120, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300,
              "death_spawn":{"character":"Imp", "count":1} }
 },
 "area_effect_objects": {
  "OmegaArea": { "name":"OmegaArea", "radius_milli":2000, "damage":50, "hits_ground":true, "only_enemies":true }
 }
}"#;

fn synth() -> CardDb {
    CardDb::from_json_str(SYNTH, CardSource::DerivedJson).unwrap()
}

/// Both shipped vintages: cards.json through the data gate (`common::cards`), and the
/// 2018 file beside it.
fn shipped() -> Vec<(&'static str, CardDb)> {
    let old = CardDb::load_repo_file("cards-2018.json").unwrap_or_else(|e| panic!("{e} (tools/extract_cards.py --vintage 2018)"));
    vec![("cards.json", cards()), ("cards-2018.json", old)]
}

/// The unit blocks a card's FIELDS carry, read field by field: what `unit_refs` must
/// equal. A new unit-producing block joins this list and `unit_refs` together.
fn field_refs(c: &CardDef) -> Vec<(UnitRef, u16, Option<i32>)> {
    let mut out = Vec::new();
    if let Some(SpellDef { shape: SpellShape::Projectile { spawn: Some(sp), .. }, .. }) = &c.spell {
        out.push((UnitRef::SpellRelease, sp.unit, sp.level_index));
    }
    if let Some(sp) = &c.spawner {
        out.push((UnitRef::Spawner, sp.unit, None));
    }
    if let Some(ds) = &c.death_spawn {
        out.push((UnitRef::DeathSpawn, ds.unit, None));
    }
    if let Some(ss) = &c.formation.second_summon {
        out.push((UnitRef::SecondSummon, ss.unit, None));
    }
    out
}

/// The unit indices a record carries, counted off its Debug text: every `unit: <n>`
/// field of any block, whether or not a list names the block (today SpawnDef,
/// SpawnerDef, DeathSpawnDef and SecondSummonDef). `unit_name:` does not match.
fn unit_fields_in_debug(c: &CardDef) -> usize {
    let text = format!("{c:?}");
    ["{ unit: ", ", unit: "]
        .into_iter()
        .map(|sep| text.match_indices(sep).filter(|&(at, _)| text[at + sep.len()..].starts_with(|ch: char| ch.is_ascii_digit())).count())
        .sum()
}

/// Every registered non-tower, non-summon card in CardDb order: the catalogue py.rs
/// builds when no names are given.
fn default_catalogue(db: &CardDb) -> Vec<u16> {
    (0..db.cards.len() as u16)
        .filter(|i| {
            let c = db.get(*i);
            c.name != KING_TOWER && c.name != PRINCESS_TOWER && !c.summon_only && db.index(&c.name) == Some(*i)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// (1)

#[test]
fn every_unit_block_is_enumerated() {
    let db = synth();
    let named = |card: &str| -> Vec<(UnitRef, String, Option<i32>)> {
        let i = db.index(card).unwrap_or_else(|| panic!("{card} refused: {:?}", db.rejected));
        db.unit_refs(i).into_iter().map(|(path, u, ix)| (path, db.get(u).name.clone(), ix)).collect()
    };
    let want = |v: &[(UnitRef, &str, Option<i32>)]| -> Vec<(UnitRef, String, Option<i32>)> { v.iter().map(|(p, n, ix)| (*p, n.to_string(), *ix)).collect() };
    assert_eq!(named("Alpha"), want(&[(UnitRef::Spawner, "Imp", None), (UnitRef::DeathSpawn, "Ghoul", None)]));
    assert_eq!(named("Beta"), want(&[(UnitRef::Spawner, "Imp", None), (UnitRef::SecondSummon, "Squire", None)]));
    assert_eq!(named("Gamma"), want(&[(UnitRef::SpellRelease, "Wisp", Some(2))]));
    // Against the Debug text, which needs no list: a block holding a unit that neither
    // `unit_refs` nor `field_refs` names is counted here all the same.
    let why_debug = "unit_refs does not name every unit index its Debug text carries";
    for i in 0..db.cards.len() as u16 {
        assert_eq!(unit_fields_in_debug(db.get(i)), db.unit_refs(i).len(), "the synthetic file {}: {why_debug}", db.get(i).name);
    }
    // Every record of the shipped files, against its fields and its Debug text; together
    // they carry every block (15.535: the Goblin Barrel, the Tombstone, the Golem, the
    // Goblin Gang).
    let mut seen: Vec<UnitRef> = Vec::new();
    for (file, db) in shipped() {
        let mut refs = 0;
        for i in 0..db.cards.len() as u16 {
            let got = db.unit_refs(i);
            assert_eq!(got, field_refs(db.get(i)), "{file} {}: unit_refs is not the blocks its fields carry", db.get(i).name);
            assert_eq!(unit_fields_in_debug(db.get(i)), got.len(), "{file} {}: {why_debug}", db.get(i).name);
            refs += got.len();
            for (path, _, _) in got {
                if !seen.contains(&path) {
                    seen.push(path);
                }
            }
        }
        assert!(refs > 0, "{file}: vacuous, no record puts a unit on the board");
    }
    for path in [UnitRef::SpellRelease, UnitRef::Spawner, UnitRef::DeathSpawn, UnitRef::SecondSummon] {
        assert!(seen.contains(&path), "vacuous: no shipped record carries {path:?}");
    }
}

// ---------------------------------------------------------------------------
// (2)

#[test]
fn every_registered_cards_units_pass_their_level_checks() {
    for (file, db) in shipped() {
        let mut checked = 0;
        for i in 0..db.cards.len() as u16 {
            let c = db.get(i);
            if db.index(&c.name) != Some(i) {
                continue; // rejected after its push: unregistered, its blocks dropped
            }
            for level in 1..=20 {
                if db.level_multiplier(i, level).is_err() {
                    continue; // not a level this card has
                }
                db.check_levels(i, level).unwrap_or_else(|e| panic!("{file} {} at {level}: {e}", c.name));
                for (path, u, ix) in db.unit_refs(i) {
                    assert!((u as usize) < db.cards.len(), "{file} {}: {path:?} unit unresolved", c.name);
                    let got = match path {
                        UnitRef::SpellRelease => db.spawn_level(i, level),
                        _ => db.unit_level(i, u, ix, level),
                    };
                    if let Err(e) = got {
                        panic!("{file} {} at {level}: {} {}: {e}", c.name, path.block_name(), db.get(u).name);
                    }
                    checked += 1;
                }
            }
        }
        assert!(checked > 0, "{file}: vacuous, no registered card puts a unit on the board");
    }
}

// ---------------------------------------------------------------------------
// (3)

#[test]
fn every_unit_a_catalogue_card_puts_on_the_board_reports_its_card() {
    let mut files = shipped();
    files.push(("the synthetic file", synth()));
    for (file, db) in &files {
        let catalogue = default_catalogue(db);
        let ids = ids_of_indices(db, &catalogue);
        let mut checked = 0;
        for &i in &catalogue {
            for (path, u, _) in field_refs(db.get(i)) {
                assert!(
                    ids.get(u as usize).is_some_and(|id| *id >= 0),
                    "{file} {}: its {} unit {} reports card id -1",
                    db.get(i).name,
                    path.block_name(),
                    db.get(u).name
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "{file}: vacuous, no catalogue card puts a unit on the board");
    }
    // The id is the FIRST catalogue card that names the unit: Imp is Alpha's and Beta's,
    // Squire only Beta's (its second summon), Wisp only Gamma's.
    let db = synth();
    let catalogue = default_catalogue(&db);
    let ids = ids_of_indices(&db, &catalogue);
    let id_of = |unit: &str| ids[db.cards.iter().position(|c| c.summon_only && c.name == unit).unwrap_or_else(|| panic!("no unit {unit}"))];
    let cid = |card: &str| catalogue.iter().position(|i| db.get(*i).name == card).unwrap_or_else(|| panic!("{card} not in the catalogue")) as i32;
    assert_eq!(
        [id_of("Imp"), id_of("Ghoul"), id_of("Squire"), id_of("Wisp")],
        [cid("Alpha"), cid("Alpha"), cid("Beta"), cid("Gamma")]
    );
}

// ---------------------------------------------------------------------------
// (4)

#[test]
fn a_rejected_card_keeps_no_unit_block() {
    let db = synth();
    let why = |n: &str| db.rejected.iter().find(|(r, _)| r == n).map(|(_, w)| w.clone()).unwrap_or_else(|| panic!("{n} not rejected: {:?}", db.rejected));
    for n in ["Omega", "Sigma"] {
        assert_eq!(why(n), "spawned unit Nobody has no units record", "{n}");
    }
    let card = |n: &str| db.cards.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("{n} was never pushed"));
    // What the cleanup had to drop: the same rows resolve on the registered cards, and
    // Sigma is a projectile spell (so its release would still be there).
    assert!(card("Alpha").spawner.is_some() && card("Alpha").death_spawn.is_some() && card("Beta").formation.second_summon.is_some());
    assert!(matches!(&card("Sigma").spell, Some(SpellDef { shape: SpellShape::Projectile { .. }, .. })), "Sigma is not a projectile spell");
    for n in ["Omega", "Sigma", "Delta"] {
        assert!(db.index(n).is_none(), "{n} is still registered");
        let c = card(n);
        assert!(c.spawner.is_none(), "{n} kept its spawner");
        assert!(c.death_spawn.is_none(), "{n} kept its death spawn");
        assert!(c.formation.second_summon.is_none(), "{n} kept its second summon");
        assert!(c.death_area_effect.is_none(), "{n} kept its death area effect");
        if let Some(SpellDef { shape: SpellShape::Projectile { spawn, .. }, .. }) = &c.spell {
            assert!(spawn.is_none(), "{n} kept its spell release");
        }
    }
    for (i, c) in db.cards.iter().enumerate() {
        for (path, u, _) in db.unit_refs(i as u16) {
            assert!((u as usize) < db.cards.len(), "{}: {path:?} unit unresolved", c.name);
        }
    }
}

// ---------------------------------------------------------------------------
// (5)

#[test]
fn summon_only_numbering_is_breadth_first() {
    let db = synth();
    // The six cards keep their file order and places (the three rejected after their
    // push included); the units follow in first-need order -- Alpha's Imp and Ghoul,
    // Beta's Squire (its Imp is loaded already), Gamma's Wisp -- then the fallback towers.
    let names: Vec<&str> = db.cards.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(&names[..6], &["Alpha", "Beta", "Gamma", "Delta", "Omega", "Sigma"]);
    let units: Vec<(usize, &str)> = db.cards.iter().enumerate().filter(|(_, c)| c.summon_only).map(|(i, c)| (i, c.name.as_str())).collect();
    assert_eq!(units, vec![(6, "Imp"), (7, "Ghoul"), (8, "Squire"), (9, "Wisp")]);
    assert_eq!(&names[10..], &[KING_TOWER, PRINCESS_TOWER]);
    // THERE IS NO SECOND LEVEL YET: Nester death-spawns Imp, so it is refused as a
    // chain and never numbered.
    let (_, why) = db.rejected.iter().find(|(n, _)| n == "Delta").expect("Delta is refused");
    assert_eq!(why, "units.Nester itself spawns units (Imp); a spawn chain is not simulated");
    assert!(names.iter().all(|n| *n != "Nester" && *n != "units.Nester"));
}

// ---------------------------------------------------------------------------
// (6)

/// Units that fail a check, one per block. No `rarities`, so the shipped 2018 table
/// applies: a Common card has unified level 1, a Legendary row's first level is 9.
///   LordSpawner   a spawner of Lord,          a Legendary row;
///   DukeTomb      a death spawn of Duke,      a Legendary row;
///   EarlPair      a second summon of Earl,    a Legendary row;
///   BaronBarrel   a spell releasing Baron,    a Legendary row;
///   BombSpawner, BombBarrel, BombPair: a spawner, a spell release and a second summon
///                 of Bomb, a death bomb (a building row with a fuse, a death damage
///                 and a radius, and nothing else);
///   BombDropper   a death spawn of Bomb, the one block that may release it.
const GUARDS: &str = r#"{ "version": "test", "cards": [
 { "name":"LordSpawner", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":300, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "spawner":{"character":"Lord", "number":1, "pause_time_ms":5000} },
 { "name":"DukeTomb", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":310, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"Duke", "count":1} },
 { "name":"EarlPair", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":320, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "second_summon":{"character":"Earl", "count":1} },
 { "name":"BaronBarrel", "kind":"spell", "elixir":3, "rarity":"Common",
   "projectile":{"name":"BaronBarrelProjectile", "speed":400, "spawn_character":"Baron", "spawn_character_count":1} },
 { "name":"BombSpawner", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":330, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "spawner":{"character":"Bomb", "number":1, "pause_time_ms":5000} },
 { "name":"BombBarrel", "kind":"spell", "elixir":3, "rarity":"Common",
   "projectile":{"name":"BombBarrelProjectile", "speed":400, "spawn_character":"Bomb", "spawn_character_count":1} },
 { "name":"BombPair", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":340, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "second_summon":{"character":"Bomb", "count":1} },
 { "name":"BombDropper", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":350, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"Bomb", "count":1} }
 ],
 "units": {
  "Lord":  { "name":"Lord", "rarity":"Legendary", "hitpoints":80, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Duke":  { "name":"Duke", "rarity":"Legendary", "hitpoints":90, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Earl":  { "name":"Earl", "rarity":"Legendary", "hitpoints":100, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Baron": { "name":"Baron", "rarity":"Legendary", "hitpoints":110, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Bomb":  { "name":"Bomb", "source_table":"buildings", "rarity":"Common", "deploy_time_ms":3000, "death_damage":200,
             "death_damage_radius_milli":2000 }
 }
}"#;

#[test]
fn a_unit_that_fails_a_check_is_caught_through_every_block() {
    let db = CardDb::from_json_str(GUARDS, CardSource::DerivedJson).unwrap();
    let idx = |card: &str| db.index(card).unwrap_or_else(|| panic!("{card} refused: {:?}", db.rejected));
    // (a) The unit's level is checked through every block: at unified 1 each Legendary
    // unit has none, at Legendary's first level (9, read off the table) each has one.
    let legendary_first = db.rarity("Legendary").expect("the shipped table has Legendary").relative_level + 1;
    assert!(legendary_first > 1, "vacuous: a Legendary row has a level 1 in this table");
    for (card, unit) in [("LordSpawner", "Lord"), ("DukeTomb", "Duke"), ("EarlPair", "Earl"), ("BaronBarrel", "Baron")] {
        let i = idx(card);
        match db.check_levels(i, 1) {
            Ok(()) => panic!("{card} at level 1: check_levels passed, but its unit {unit} has no level 1"),
            Err(e) => assert!(e.starts_with(&format!("{unit} (Legendary) has no level 1 ")), "{card}: {e}"),
        }
        db.check_levels(i, legendary_first).unwrap_or_else(|e| panic!("{card} at level {legendary_first}: {e}"));
    }
    // (b) A death bomb reaches the board through a death spawn and nothing else: every
    // other block that names one is refused, by name.
    for (card, block) in [("BombSpawner", "a periodic spawner"), ("BombBarrel", "a spell release"), ("BombPair", "a second summon")] {
        let want = format!("Bomb is a death bomb, which only a death spawn releases; {block} cannot");
        let why = db.rejected.iter().find(|(n, _)| n == card).map(|(_, w)| w.as_str());
        assert_eq!(why, Some(want.as_str()), "{card}: {:?}", db.rejected);
        assert!(db.index(card).is_none(), "{card} is still registered");
    }
    let bomb = db.get(idx("BombDropper")).death_spawn.expect("BombDropper's death spawn").unit;
    assert_eq!((db.get(bomb).name.as_str(), db.get(bomb).death_bomb_fuse_ms()), ("Bomb", Some(3000)), "the death spawn is not the bomb");
}
