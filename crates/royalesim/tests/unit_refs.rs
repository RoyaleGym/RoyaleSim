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
//!      default catalogue (py.rs `Battle(card_names=None)`, the Mirror and the cards
//!      that travel under ground included), every unit a catalogue card's FIELDS name gets a
//!      card id, never -1 -- both shipped files and the synthetic one, where the id is
//!      pinned to the first card that names the unit;
//!   4. `a_rejected_card_keeps_no_unit_block`: cards rejected after their push, one
//!      carrying a spawner, a death spawn, a second summon and a death area effect, one
//!      a spell release and one a transformation, end with every block dropped, read
//!      off the fields;
//!   5. `summon_only_numbering_is_breadth_first`: the summon-only records follow every
//!      card, numbered in first-need order, one level at a time. A unit that itself puts
//!      units on the board loads (a spawn chain): Delta's Nester death-spawns Imp, which is
//!      loaded already, so it adds no record. The second level's own order, and the chain's
//!      limit, are pinned in tests/spawn_chain.rs. A death area's units are the dying
//!      card's needs, at its own level; no accepted area has any yet, and the change that
//!      gives one some pins their place here too;
//!   6. `a_unit_that_fails_a_check_is_caught_through_every_block`: the failing
//!      direction of the two checks that read `unit_refs`, on a synthetic file.
//!      (a) A Common card whose unit is a Legendary row, one per block (spawner,
//!      death spawn, second summon, spell release, transformation): `check_levels` at
//!      unified 1 is Err and names the unit, and passes at 9, where Legendary starts.
//!      (b) A death bomb named by a spawner, a spell release, a second summon and a
//!      transformation: each card is refused with the exact text; the card that
//!      death-spawns it loads;
//!   7. `a_containers_units_report_their_card_and_pass_level_checks`: a CONTAINER (a death
//!      bomb that carries a death spawn: card.rs `Hitpointless::BombWithDeathSpawn`, the
//!      Skeleton Barrel's) puts its units one level down the chain. In the catalogue
//!      [SkeletonBalloon, Knight] the container's Skeleton reports the barrel's id; on a
//!      synthetic file, `check_levels` errs through a container whose unit is a Legendary
//!      row and passes at Legendary's first level, and that unit loads after the
//!      container. PLANTS `ids_one_level` (the Skeleton reports -1), `check_levels_one_deep`
//!      (the check passes at 1) and `container_not_a_bomb` (the container is refused) turn
//!      it red.
//!
//! PLANT: `RUSTFLAGS='--cfg clash_plant="unit_refs_skips_new_paths"'
//! CARGO_TARGET_DIR=target/plant cargo test --test unit_refs`: `unit_refs` drops the
//! second summon, a centre-aimed strike's delivery, a buff's death spawn and the
//! transformation -> 1, 3, 4 and 6 red (6 by its second-summon and transformation cases:
//! the level check passes and the bomb card loads; 1 and 3 also by the Royal Delivery's
//! Recruit and the curses' units). 2 and 5 stay green: a block the enumeration skips skips
//! its level check silently (the defect itself, which 6 shows failing), and the numbering
//! does not read `unit_refs`.
//!
//! PLANT: `unit_refs_skips_attach`: `unit_refs` drops the attached rider (card.rs
//! `AttachDef`) -> 1 red (Theta, and the shipped Ram Rider against its fields) and 3 red
//! (Jockey and the Ram Rider's rider report card id -1).
//!
//! PLANT: `unit_refs_skips_scheduled`: `unit_refs` drops a scheduled area's entries (card.rs
//! `SpellShape::ScheduledArea`) -> 1 red (the shipped Graveyard and Suspicious Bush against their
//! fields and their Debug text) and 3 red (their Skeleton and goblin report card id -1).

mod common;

use common::*;
use royalesim::card::{CardDb, CardDef, CardSource, SpellDef, SpellShape, UnitRef, KING_TOWER, PRINCESS_TOWER};
use std::collections::BTreeSet;
use royalesim::py::ids_of_indices;

/// A file with every unit block this loader reads, each unit a distinct row:
///   Alpha   a spawner (Imp) and a death spawn (Ghoul);
///   Beta    a spawner (Imp, shared) and a second summon (Squire);
///   Gamma   a spell releasing Wisp, SpawnCharacterLevelIndex 2;
///   Delta   a death spawn (Nester) whose row death-spawns Imp itself: a chain, which loads;
///   Omega   a spawner, a death spawn, a death area effect and a second summon
///           (Nobody) with no `units` row;
///   Sigma   a spell releasing Nobody;
///   Theta   an attached rider (Jockey);
///   Tau     a transformation into Husk;
///   Upsilon a transformation into Nobody.
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
   "projectile":{"name":"SigmaBarrel", "speed":400, "spawn_character":"Nobody", "spawn_character_count":1} },
 { "name":"Theta", "kind":"troop", "elixir":5, "rarity":"Common", "hitpoints":340, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "spawner":{"character":"Jockey", "number":1, "attach":true} },
 { "name":"Tau", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":340, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500,
   "action_graph":{"roots":{"OnStartingAction":"AtHealth"},"class_types":["ActionChangeGameObjectData","ActionRunActionAtHealth"],"spawns":[],"mechanic":true},
   "transform_at_hp":{"into":"Husk","reset_target":false,"group_delays_ms":[],"at":0,"pct":50,"noop_spawns":[]} },
 { "name":"Upsilon", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":350, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500,
   "action_graph":{"roots":{"OnStartingAction":"AtHealth"},"class_types":["ActionChangeGameObjectData","ActionRunActionAtHealth"],"spawns":[],"mechanic":true},
   "transform_at_hp":{"into":"Nobody","reset_target":false,"group_delays_ms":[],"at":0,"pct":50,"noop_spawns":[]} }
 ],
 "units": {
  "Imp":    { "name":"Imp", "rarity":"Common", "hitpoints":80, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Ghoul":  { "name":"Ghoul", "rarity":"Common", "hitpoints":90, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Squire": { "name":"Squire", "rarity":"Common", "hitpoints":100, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Wisp":   { "name":"Wisp", "rarity":"Common", "hitpoints":110, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Nester": { "name":"Nester", "rarity":"Common", "hitpoints":120, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300,
              "death_spawn":{"character":"Imp", "count":1} },
  "Jockey": { "name":"Jockey", "rarity":"Common", "hitpoints":130, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Husk":   { "name":"Husk", "rarity":"Common", "hitpoints":130, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 }
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

/// Every buff a shape's chain hangs, field by field (`buff`, then `buff2`, then the next object's), each once.
fn shape_buffs(shape: &SpellShape, out: &mut Vec<u16>) {
    let (hit, next) = match shape {
        SpellShape::Projectile { hit, .. } => (hit.as_ref(), None),
        SpellShape::AreaEffect { hit } => (Some(hit), None),
        SpellShape::PulsingAreaEffect { hit, child, .. } => (Some(hit), child.as_deref()),
        SpellShape::Rolling { hit, .. } => (Some(hit), None),
        SpellShape::Strikes(d) => (Some(&d.hit), d.delivery.as_deref()),
        SpellShape::Clone { hit, .. } => (Some(hit), None),
        SpellShape::Fuse { then, .. } => (None, Some(then.as_ref())),
        SpellShape::Echo { hit, then } => (Some(hit), Some(then.as_ref())),
        SpellShape::Summon { .. } | SpellShape::Mirror | SpellShape::Variant { .. } | SpellShape::ScheduledArea { .. } => (None, None),
    };
    if let Some(h) = hit {
        for b in [h.buff, h.buff2].into_iter().flatten() {
            if !out.contains(&b.buff) {
                out.push(b.buff);
            }
        }
    }
    // A selector's buffs (the Vines' snare, the Void's tiers) and the Clone's hold, after the hit's.
    let extra: Vec<u16> = match shape {
        SpellShape::Strikes(d) => d.selector.as_ref().map_or(Vec::new(), |s| s.buffs.iter().map(|b| b.buff).collect()),
        SpellShape::Clone { hold, .. } => vec![hold.buff],
        _ => Vec::new(),
    };
    for b in extra {
        if !out.contains(&b) {
            out.push(b);
        }
    }
    if let Some(n) = next {
        shape_buffs(n, out);
    }
}

/// The unit blocks a card's FIELDS carry, read field by field: what `unit_refs` must
/// equal. A new unit-producing block joins this list and `unit_refs` together.
fn field_refs(db: &CardDb, c: &CardDef) -> Vec<(UnitRef, u16, Option<i32>)> {
    let mut out = Vec::new();
    // a projectile's release and a roll's, read field by field
    if let Some(SpellDef { shape: SpellShape::Projectile { spawn: Some(sp), .. } | SpellShape::Rolling { spawn: Some(sp), .. }, .. }) = &c.spell {
        out.push((UnitRef::SpellRelease, sp.unit, sp.level_index));
    }
    // a centre-aimed strike's delivery (the Royal Delivery's crate)
    if let Some(SpellDef { shape: SpellShape::Strikes(d), .. }) = &c.spell {
        if let Some(SpellShape::Projectile { spawn: Some(sp), .. }) = d.delivery.as_deref() {
            out.push((UnitRef::SpellRelease, sp.unit, sp.level_index));
        }
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
    if let Some(SpellDef { shape: SpellShape::Projectile { spawn: Some(sp), .. }, .. }) = &c.death_projectile {
        out.push((UnitRef::DeathProjectile, sp.unit, sp.level_index));
    }
    if let Some(SpellDef { shape: SpellShape::Summon { unit, .. }, .. }) = &c.spell {
        out.push((UnitRef::SpellSummon, *unit, None));
    }
    if let Some(ls) = &c.life_state {
        out.push((UnitRef::LifeState, ls.unit, None));
    }
    if let Some(m) = c.spawn_pathfind.and_then(|p| p.morph) {
        out.push((UnitRef::Morph, m, None));
    }
    if let Some(at) = &c.attach {
        out.push((UnitRef::Attach, at.unit, None));
    }
    // a deploy at explicit offsets: every member, member 0 the card itself (the Three Musketeers)
    if let Some(ms) = &c.summon_members {
        for (k, m) in ms.iter().enumerate() {
            out.push((UnitRef::SummonMember(k as u8), m.unit, None));
        }
    }
    // a variant card's forms, each a card of its own (the Spirit Empress)
    if let Some(opts) = c.variant() {
        for (k, o) in opts.iter().enumerate() {
            out.push((UnitRef::VariantForm(k as u8), o.card, None));
        }
    }
    // the death spawn of each buff the card hangs: its attack's, its reflect's, its counter's stun, then along each
    // spell object's chain
    let mut buffs: Vec<u16> = Vec::new();
    for b in [c.attack_buff, c.reflect.and_then(|r| r.buff), c.parry.map(|p| p.stun)].into_iter().flatten() {
        if !buffs.contains(&b.buff) {
            buffs.push(b.buff);
        }
    }
    for d in [&c.spell, &c.death_area_effect, &c.deploy_projectile, &c.death_projectile, &c.deploy_area_effect, &c.spawn_area_effect, &c.projectile_area].into_iter().flatten() {
        shape_buffs(&d.shape, &mut buffs);
    }
    for b in buffs {
        if let Some(ds) = db.buffs[b as usize].death_spawn {
            out.push((UnitRef::BuffDeathSpawn, ds.unit, None));
        }
    }
    if let Some(t) = &c.transform_at_hp {
        out.push((UnitRef::Transform, t.unit, None));
    }
    // every entry of a scheduled area, in the spell, the death area or the projectile area, read field by field down
    // each chain (the Graveyard's Skeletons, the Suspicious Bush's goblins)
    for d in [&c.spell, &c.death_area_effect, &c.projectile_area].into_iter().flatten() {
        let mut shape = Some(&d.shape);
        while let Some(sh) = shape {
            if let SpellShape::ScheduledArea { schedule, .. } = sh {
                for (k, e) in schedule.iter().enumerate() {
                    out.push((UnitRef::Scheduled(k as u8), e.unit, None));
                }
            }
            shape = match sh {
                SpellShape::Fuse { then, .. } => Some(then.as_ref()),
                SpellShape::PulsingAreaEffect { child, .. } => child.as_deref(),
                SpellShape::Strikes(s) => s.delivery.as_deref(),
                _ => None,
            };
        }
    }
    // the unit a hero's button puts down (the Hero Musketeer's turret)
    if let Some(royalesim::card::AbilityDef { effect: royalesim::card::AbilityEffect::SpawnAhead { unit, .. }, .. }) = &c.ability {
        out.push((UnitRef::AbilityUnit, *unit, None));
    }
    // every entry of the card's deploy spawn area (the Tri Wizards' TriWizardSpawn): entry 0 the card itself, then the
    // cards whose deploy areas its actions make
    if let Some(SpellDef { shape: SpellShape::ScheduledArea { schedule, .. }, .. }) = &c.deploy_spawn_area {
        for (k, e) in schedule.iter().enumerate() {
            out.push((UnitRef::DeploySpawn(k as u8), e.unit, None));
        }
    }
    out
}

/// Every `unit: <n>` field in `text`, the underground walk's `morph: Some(<n>)` (SpawnPathfindDef, the Goblin Drill's
/// building), and every `card: <n>` (a VariantOption's form, a card index). `unit_name:` does not match.
fn unit_fields_in(text: &str) -> usize {
    ["{ unit: ", ", unit: ", "morph: Some(", ", card: "]
        .into_iter()
        .map(|sep| text.match_indices(sep).filter(|&(at, _)| text[at + sep.len()..].starts_with(|ch: char| ch.is_ascii_digit())).count())
        .sum()
}

/// The unit indices a record carries, counted off Debug text: every `unit: <n>` field of
/// any block of the record, whether or not a list names the block (today SpawnDef,
/// SpawnerDef, DeathSpawnDef, SecondSummonDef, LifeStateDef, SummonMemberDef and AttachDef, with
/// SpawnPathfindDef's morph and VariantOption's card, `unit_fields_in`), and of every buff row the
/// record names (`BuffApply { buff: <n>`, each row once: a buff's death spawn holds its unit
/// on the shared row, BuffDeathSpawn).
fn unit_fields_in_debug(db: &CardDb, c: &CardDef) -> usize {
    let text = format!("{c:?}");
    let sep = "BuffApply { buff: ";
    let rows: BTreeSet<usize> = text
        .match_indices(sep)
        .filter_map(|(at, _)| text[at + sep.len()..].split(|ch: char| !ch.is_ascii_digit()).next().and_then(|d| d.parse().ok()))
        .collect();
    unit_fields_in(&text) + rows.into_iter().map(|b| unit_fields_in(&format!("{:?}", db.buffs[b]))).sum::<usize>()
}

/// Every registered non-tower, non-summon card in CardDb order: the catalogue py.rs
/// builds when no names are given, the Mirror (code 6) and the cards that travel under
/// ground (the Miner, the Goblin Drill, code 5) included.
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
    assert_eq!(named("Theta"), want(&[(UnitRef::Attach, "Jockey", None)]));
    assert_eq!(named("Tau"), want(&[(UnitRef::Transform, "Husk", None)]));
    // Against the Debug text, which needs no list: a block holding a unit that neither
    // `unit_refs` nor `field_refs` names is counted here all the same.
    let why_debug = "unit_refs does not name every unit index its Debug text carries";
    for i in 0..db.cards.len() as u16 {
        assert_eq!(unit_fields_in_debug(&db, db.get(i)), db.unit_refs(i).len(), "the synthetic file {}: {why_debug}", db.get(i).name);
    }
    // Every record of the shipped files, against its fields and its Debug text; together
    // they carry every block (15.535: the Goblin Barrel, the Tombstone, the Golem, the
    // Goblin Gang, the Ram Rider, and the Mother Witch's and the Goblin Curse's buff death spawns).
    let mut seen: Vec<UnitRef> = Vec::new();
    for (file, db) in shipped() {
        let mut refs = 0;
        for i in 0..db.cards.len() as u16 {
            let got = db.unit_refs(i);
            assert_eq!(got, field_refs(&db, db.get(i)), "{file} {}: unit_refs is not the blocks its fields carry", db.get(i).name);
            assert_eq!(unit_fields_in_debug(&db, db.get(i)), got.len(), "{file} {}: {why_debug}", db.get(i).name);
            refs += got.len();
            for (path, _, _) in got {
                if !seen.contains(&path) {
                    seen.push(path);
                }
            }
        }
        assert!(refs > 0, "{file}: vacuous, no record puts a unit on the board");
    }
    for path in [UnitRef::SpellRelease, UnitRef::Spawner, UnitRef::DeathSpawn, UnitRef::SecondSummon, UnitRef::Attach, UnitRef::BuffDeathSpawn, UnitRef::Transform, UnitRef::Scheduled(0), UnitRef::DeploySpawn(0)] {
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
            for (path, u, _) in field_refs(db, db.get(i)) {
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
    // Squire only Beta's (its second summon), Wisp only Gamma's, Jockey only Theta's (its
    // attached rider), Husk only Tau's (its transformation).
    let db = synth();
    let catalogue = default_catalogue(&db);
    let ids = ids_of_indices(&db, &catalogue);
    let id_of = |unit: &str| ids[db.cards.iter().position(|c| c.summon_only && c.name == unit).unwrap_or_else(|| panic!("no unit {unit}"))];
    let cid = |card: &str| catalogue.iter().position(|i| db.get(*i).name == card).unwrap_or_else(|| panic!("{card} not in the catalogue")) as i32;
    assert_eq!(
        [id_of("Imp"), id_of("Ghoul"), id_of("Squire"), id_of("Wisp"), id_of("Jockey"), id_of("Husk")],
        [cid("Alpha"), cid("Alpha"), cid("Beta"), cid("Gamma"), cid("Theta"), cid("Tau")]
    );
}

// ---------------------------------------------------------------------------
// (4)

#[test]
fn a_rejected_card_keeps_no_unit_block() {
    let db = synth();
    let why = |n: &str| db.rejected.iter().find(|(r, _)| r == n).map(|(_, w)| w.clone()).unwrap_or_else(|| panic!("{n} not rejected: {:?}", db.rejected));
    for n in ["Omega", "Sigma", "Upsilon"] {
        assert_eq!(why(n), "spawned unit Nobody has no units record", "{n}");
    }
    let card = |n: &str| db.cards.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("{n} was never pushed"));
    // What the cleanup had to drop: the same rows resolve on the registered cards, and
    // Sigma is a projectile spell (so its release would still be there).
    assert!(card("Alpha").spawner.is_some() && card("Alpha").death_spawn.is_some() && card("Beta").formation.second_summon.is_some());
    assert!(matches!(&card("Sigma").spell, Some(SpellDef { shape: SpellShape::Projectile { .. }, .. })), "Sigma is not a projectile spell");
    assert!(card("Tau").transform_at_hp.is_some(), "Tau's transformation does not resolve");
    for n in ["Omega", "Sigma", "Upsilon"] {
        assert!(db.index(n).is_none(), "{n} is still registered");
        let c = card(n);
        assert!(c.spawner.is_none(), "{n} kept its spawner");
        assert!(c.death_spawn.is_none(), "{n} kept its death spawn");
        assert!(c.formation.second_summon.is_none(), "{n} kept its second summon");
        assert!(c.death_area_effect.is_none(), "{n} kept its death area effect");
        assert!(c.transform_at_hp.is_none(), "{n} kept its transformation");
        if let Some(SpellDef { shape: SpellShape::Projectile { spawn, .. } | SpellShape::Rolling { spawn, .. }, .. }) = &c.spell {
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
    // The nine cards keep their file order and places (the three rejected after their
    // push included); the units follow in first-need order -- Alpha's Imp and Ghoul,
    // Beta's Squire (its Imp is loaded already), Gamma's Wisp, Delta's Nester, Theta's
    // Jockey, Tau's Husk -- then the fallback towers.
    let names: Vec<&str> = db.cards.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(&names[..9], &["Alpha", "Beta", "Gamma", "Delta", "Omega", "Sigma", "Theta", "Tau", "Upsilon"]);
    let units: Vec<(usize, &str)> = db.cards.iter().enumerate().filter(|(_, c)| c.summon_only).map(|(i, c)| (i, c.name.as_str())).collect();
    assert_eq!(units, vec![(9, "Imp"), (10, "Ghoul"), (11, "Squire"), (12, "Wisp"), (13, "Nester"), (14, "Jockey"), (15, "Husk")]);
    assert_eq!(&names[16..], &[KING_TOWER, PRINCESS_TOWER]);
    // A CHAIN LOADS: Nester, Delta's first-level need, death-spawns Imp itself; its own
    // need is the Imp record already loaded, so the second level adds nothing here.
    let delta = db.index("Delta").unwrap_or_else(|| panic!("Delta refused: {:?}", db.rejected));
    let nester = db.get(delta).death_spawn.expect("Delta's death spawn").unit;
    assert_eq!(db.get(nester).name, "Nester");
    let imp = db.get(nester).death_spawn.expect("Nester's own death spawn").unit;
    assert_eq!((imp, db.get(imp).name.as_str()), (9, "Imp"), "Nester's Imp is the Imp record");
}

// ---------------------------------------------------------------------------
// (6)

/// Units that fail a check, one per block. No `rarities`, so the shipped 2018 table
/// applies: a Common card has unified level 1, a Legendary row's first level is 9.
///   LordSpawner   a spawner of Lord,          a Legendary row;
///   DukeTomb      a death spawn of Duke,      a Legendary row;
///   EarlPair      a second summon of Earl,    a Legendary row;
///   BaronBarrel   a spell releasing Baron,    a Legendary row;
///   CountShift    a transformation into Count, a Legendary row;
///   BombSpawner, BombBarrel, BombPair, BombShift: a spawner, a spell release, a second
///                 summon and a transformation of Bomb, a death bomb (a building row with a
///                 fuse, a death damage and a radius, and nothing else);
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
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"Bomb", "count":1} },
 { "name":"CountShift", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":360, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500,
   "action_graph":{"roots":{"OnStartingAction":"AtHealth"},"class_types":["ActionChangeGameObjectData","ActionRunActionAtHealth"],"spawns":[],"mechanic":true},
   "transform_at_hp":{"into":"Count","reset_target":false,"group_delays_ms":[],"at":0,"pct":50,"noop_spawns":[]} },
 { "name":"BombShift", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":370, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500,
   "action_graph":{"roots":{"OnStartingAction":"AtHealth"},"class_types":["ActionChangeGameObjectData","ActionRunActionAtHealth"],"spawns":[],"mechanic":true},
   "transform_at_hp":{"into":"Bomb","reset_target":false,"group_delays_ms":[],"at":0,"pct":50,"noop_spawns":[]} }
 ],
 "units": {
  "Lord":  { "name":"Lord", "rarity":"Legendary", "hitpoints":80, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Duke":  { "name":"Duke", "rarity":"Legendary", "hitpoints":90, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Earl":  { "name":"Earl", "rarity":"Legendary", "hitpoints":100, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Baron": { "name":"Baron", "rarity":"Legendary", "hitpoints":110, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Count": { "name":"Count", "rarity":"Legendary", "hitpoints":120, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
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
    for (card, unit) in [("LordSpawner", "Lord"), ("DukeTomb", "Duke"), ("EarlPair", "Earl"), ("BaronBarrel", "Baron"), ("CountShift", "Count")] {
        let i = idx(card);
        match db.check_levels(i, 1) {
            Ok(()) => panic!("{card} at level 1: check_levels passed, but its unit {unit} has no level 1"),
            Err(e) => assert!(e.starts_with(&format!("{unit} (Legendary) has no level 1 ")), "{card}: {e}"),
        }
        db.check_levels(i, legendary_first).unwrap_or_else(|e| panic!("{card} at level {legendary_first}: {e}"));
    }
    // (b) A death bomb reaches the board through a death spawn and nothing else: every
    // other block that names one is refused, by name.
    for (card, block) in [("BombSpawner", "a periodic spawner"), ("BombBarrel", "a spell release"), ("BombPair", "a second summon"), ("BombShift", "a transformation")] {
        let want = format!("Bomb is a death bomb, which only a death spawn releases; {block} cannot");
        let why = db.rejected.iter().find(|(n, _)| n == card).map(|(_, w)| w.as_str());
        assert_eq!(why, Some(want.as_str()), "{card}: {:?}", db.rejected);
        assert!(db.index(card).is_none(), "{card} is still registered");
    }
    let bomb = db.get(idx("BombDropper")).death_spawn.expect("BombDropper's death spawn").unit;
    assert_eq!((db.get(bomb).name.as_str(), db.get(bomb).death_bomb_fuse_ms()), ("Bomb", Some(3000)), "the death spawn is not the bomb");
}

// ---------------------------------------------------------------------------
// (7) a container's units

/// A card whose death leaves a CONTAINER (Crate: DeployTime, DeathDamage, DeathDamageRadius and a death spawn),
/// whose units are a Legendary row.
const CONTAINER: &str = r#"{ "version": "test", "cards": [
 { "name":"CrateDropper", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":300, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"Crate", "count":1} }
 ],
 "units": {
  "Crate":   { "name":"Crate", "source_table":"buildings", "rarity":"Common", "deploy_time_ms":600, "death_damage":57,
               "death_damage_radius_milli":2000,
               "death_spawn":{"character":"Marquis", "count":3, "radius_milli":1480, "deploy_time_ms":500} },
  "Marquis": { "name":"Marquis", "rarity":"Legendary", "hitpoints":80, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 }
 }
}"#;

#[test]
fn a_containers_units_report_their_card_and_pass_level_checks() {
    // The shipped table: the Skeleton Barrel's container, and its Skeletons one level down.
    let db = cards();
    let barrel = db.index("SkeletonBalloon").unwrap_or_else(|| panic!("the Skeleton Barrel refused: {:?}", db.rejected.iter().find(|(n, _)| n == "SkeletonBalloon")));
    let knight = db.index("Knight").expect("the Knight loads");
    let crate_idx = db.get(barrel).death_spawn.expect("the barrel's death spawn").unit;
    assert!(db.get(crate_idx).death_bomb_fuse_ms().is_some(), "the barrel's death spawn is not a death bomb");
    let skeleton = db.get(crate_idx).death_spawn.expect("the container's own death spawn").unit;
    let ids = ids_of_indices(&db, &[barrel, knight]);
    assert_eq!(ids[crate_idx as usize], 0, "the container reports the barrel's id");
    assert_eq!(ids[skeleton as usize], 0, "the container's Skeleton reports the barrel's id, one level down");
    // A synthetic container whose unit has no level 1: the check goes through the container to it.
    let db = CardDb::from_json_str(CONTAINER, CardSource::DerivedJson).unwrap();
    let dropper = db.index("CrateDropper").unwrap_or_else(|| panic!("CrateDropper refused: {:?}", db.rejected));
    let crate_idx = db.get(dropper).death_spawn.expect("CrateDropper's death spawn").unit;
    let c = db.get(crate_idx);
    assert_eq!((c.name.as_str(), c.death_bomb_fuse_ms()), ("Crate", Some(600)), "the death spawn is not the container");
    let marquis = c.death_spawn.expect("the container's own death spawn").unit;
    assert_eq!(db.get(marquis).name, "Marquis");
    assert!(marquis > crate_idx, "the container's unit loads after the container, one level down");
    match db.check_levels(dropper, 1) {
        Ok(()) => panic!("CrateDropper at level 1: check_levels passed, but the container's Marquis has no level 1"),
        Err(e) => assert!(e.starts_with("Marquis (Legendary) has no level 1 "), "CrateDropper: {e}"),
    }
    let legendary_first = db.rarity("Legendary").expect("the table has Legendary").relative_level + 1;
    db.check_levels(dropper, legendary_first).unwrap_or_else(|e| panic!("CrateDropper at level {legendary_first}: {e}"));
}
