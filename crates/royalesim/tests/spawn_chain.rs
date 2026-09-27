//! A SPAWNED UNIT'S OWN UNITS (card.rs `CardDb::from_json_str`, the worklist): a spawn chain loads.
//!
//! The loader used to refuse every unit whose own row put units on the board ("a spawn chain is not simulated").
//! Two cards of client 15.535.29 need one: the Goblin Drill (its building spawns a Goblin and death-spawns two) and the
//! Elixir Golem (ElixirGolem1 -> 2 x ElixirGolem2 -> 2 x ElixirGolem4). A chain now loads level by level, down to
//! MAX_CHAIN_DEPTH, and is as loadable as its weakest link.
//!
//! WHAT IS PINNED:
//!   1. `a_chain_loads_and_numbers_breadth_first`: on a synthetic file, a three-generation chain loads; the
//!      summon-only records are numbered one level at a time (every first-level need, then every second-level one);
//!   2. `the_level_check_walks_the_whole_chain`: a grandchild with no level at the card's level fails
//!      `check_levels`, naming the grandchild;
//!   3. `a_failing_link_refuses_every_card_that_reaches_it`: two cards sharing a unit whose own unit has no row are
//!      both refused, with the same reason, and the broken record is not listed among the rejected cards;
//!   4. `the_chain_stops_at_max_chain_depth`: a chain one record longer than the limit is refused with the depth in
//!      its reason; the longest allowed loads;
//!   5. `a_card_refused_by_a_table_row_leaves_no_unit_behind`: a card whose death projectile does not load leaves no
//!      summon-only record of its death spawn (synthetic, and the 15.535.29 SuperLavaHound);
//!   6. `every_link_reports_the_card_at_the_top`: py.rs `ids_of_indices` gives a grandchild its card's id;
//!   7. `the_elixir_golem_chain_loads_and_plays`: on cards.json the Elixir Golem loads all three generations (its
//!      elixir on death runs too, pinned by tests/mana_on_death.rs), and in play each death lays its pair on the x
//!      axis at +-750 (spawner.DEATH_SPAWN_RING, client 15.535.29).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test spawn_chain`):
//!   * `chain_refused` -- every chain refused as before: 1, 6 and 7 go red.
//!   * `check_levels_one_deep` -- `check_levels` stops one level down: 2 goes red.
//!   * `chain_failure_first_root_only` -- only the first card reaching a broken link is refused: 3 and 4 go red.
//!   * `unit_needs_first` -- a record's unit needs before its table needs: 1 and 5 go red.
//!   * `ids_one_level` -- the catalogue ids stop at the first level: 6 goes red.
//!   * `death_ring_facing` -- the listed units keep the facing ring: 7 goes red.
//!
//! OPEN, not pinned as a law: which member of a pair takes -x. 7 pins the lower `team_seq` on -x, which is what the
//! client 15.535.29 records show for the Elixir Golem (23 of 23 pairs), in key order.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, KING_TOWER, MAX_CHAIN_DEPTH, PRINCESS_TOWER};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::py::ids_of_indices;
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

/// A file of chains, every unit a distinct row. No `rarities`: the shipped 2018 table applies (a Legendary row's
/// first unified level is 9). No towers: the fallback pair follows the units.
///   Root     death-spawns Mid, which death-spawns Leaf (a Legendary row): three generations;
///   Other    a spawner of Imp, a first-level need numbered between Mid and Leaf;
///   Twin1/2  both death-spawn Shared, whose own death spawn Missing has no row;
///   Deep     D1 -> D2 -> D3 -> D4 -> D5 -> D6: D5, first loaded MAX_CHAIN_DEPTH levels below the card, needs a unit;
///   Shallow  S1 -> S2 -> S3 -> S4 -> S5: the longest chain the loader follows;
///   Hound    death-spawns Pup and carries a death projectile the file has no row for.
const CHAINS: &str = r#"{ "version": "test", "cards": [
 { "name":"Root", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":300, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"Mid", "count":2} },
 { "name":"Other", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":310, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "spawner":{"character":"Imp", "number":1, "pause_time_ms":5000} },
 { "name":"Twin1", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":320, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"Shared", "count":1} },
 { "name":"Twin2", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":330, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"Shared", "count":1} },
 { "name":"Deep", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":340, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"D1", "count":1} },
 { "name":"Shallow", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":350, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"S1", "count":1} },
 { "name":"Hound", "kind":"troop", "elixir":3, "rarity":"Common", "hitpoints":360, "hit_speed_ms":1000,
   "range_milli":1000, "collision_radius_milli":500, "death_spawn":{"character":"Pup", "count":2},
   "death_spawn_projectile":"NoSuchShot" }
 ],
 "units": {
  "Mid":    { "name":"Mid", "rarity":"Common", "hitpoints":100, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":400,
              "death_spawn":{"character":"Leaf", "count":2} },
  "Leaf":   { "name":"Leaf", "rarity":"Legendary", "hitpoints":50, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Imp":    { "name":"Imp", "rarity":"Common", "hitpoints":80, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Shared": { "name":"Shared", "rarity":"Common", "hitpoints":90, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300,
              "death_spawn":{"character":"Missing", "count":1} },
  "D1": { "name":"D1", "rarity":"Common", "hitpoints":61, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300, "death_spawn":{"character":"D2", "count":1} },
  "D2": { "name":"D2", "rarity":"Common", "hitpoints":62, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300, "death_spawn":{"character":"D3", "count":1} },
  "D3": { "name":"D3", "rarity":"Common", "hitpoints":63, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300, "death_spawn":{"character":"D4", "count":1} },
  "D4": { "name":"D4", "rarity":"Common", "hitpoints":64, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300, "death_spawn":{"character":"D5", "count":1} },
  "D5": { "name":"D5", "rarity":"Common", "hitpoints":65, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300, "death_spawn":{"character":"D6", "count":1} },
  "D6": { "name":"D6", "rarity":"Common", "hitpoints":66, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "S1": { "name":"S1", "rarity":"Common", "hitpoints":71, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300, "death_spawn":{"character":"S2", "count":1} },
  "S2": { "name":"S2", "rarity":"Common", "hitpoints":72, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300, "death_spawn":{"character":"S3", "count":1} },
  "S3": { "name":"S3", "rarity":"Common", "hitpoints":73, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300, "death_spawn":{"character":"S4", "count":1} },
  "S4": { "name":"S4", "rarity":"Common", "hitpoints":74, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300, "death_spawn":{"character":"S5", "count":1} },
  "S5": { "name":"S5", "rarity":"Common", "hitpoints":75, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 },
  "Pup":    { "name":"Pup", "rarity":"Common", "hitpoints":40, "hit_speed_ms":1000, "range_milli":500, "collision_radius_milli":300 }
 }
}"#;

fn chains() -> CardDb {
    CardDb::from_json_str(CHAINS, CardSource::DerivedJson).unwrap()
}

fn rejected<'a>(db: &'a CardDb, card: &str) -> Option<&'a str> {
    db.rejected.iter().find(|(n, _)| n == card).map(|(_, w)| w.as_str())
}

/// Every registered non-tower, non-summon card in CardDb order: the catalogue py.rs builds when no names are given,
/// BEFORE it leaves out the Mirror (code 6) and the cards that travel under ground (the Miner, the Goblin Drill),
/// which a decoder of codes 0 to 4 cannot place yet. A `card_names` list that names one gets it, so the units those
/// cards put on the board are kept in view here.
fn default_catalogue(db: &CardDb) -> Vec<u16> {
    (0..db.cards.len() as u16)
        .filter(|i| {
            let c = db.get(*i);
            c.name != KING_TOWER && c.name != PRINCESS_TOWER && !c.summon_only && db.index(&c.name) == Some(*i)
        })
        .collect()
}

/// The chain of death spawns below card `name`, by record name.
fn death_chain(db: &CardDb, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut at = db.index(name).unwrap_or_else(|| panic!("{name} refused: {:?}", rejected(db, name)));
    while let Some(ds) = db.get(at).death_spawn {
        at = ds.unit;
        out.push(db.get(at).name.clone());
        assert!(out.len() <= 16, "{name}: the chain comes back on itself");
    }
    out
}

// ---------------------------------------------------------------------------
// (1)

/// Plants: chain_refused, unit_needs_first.
#[test]
fn a_chain_loads_and_numbers_breadth_first() {
    let db = chains();
    assert_eq!(death_chain(&db, "Root"), ["Mid", "Leaf"], "Root's three generations");
    assert_eq!(death_chain(&db, "Shallow"), ["S1", "S2", "S3", "S4", "S5"]);
    let names: Vec<&str> = db.cards.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(&names[..7], &["Root", "Other", "Twin1", "Twin2", "Deep", "Shallow", "Hound"], "the cards keep their file order");
    // Level 0 in card order (Root's Mid, Other's Imp, the twins' Shared, Deep's D1, Shallow's S1; Hound is refused by
    // its table row before its Pup loads), then level 1 (Mid's Leaf, D2, S2), then 2, 3 and 4. Shared, D4 and the
    // records reaching D4 stay numbered though they are refused (see 3 and 4).
    let units: Vec<&str> = db.cards.iter().filter(|c| c.summon_only).map(|c| c.name.as_str()).collect();
    assert_eq!(units, ["Mid", "Imp", "Shared", "D1", "S1", "Leaf", "D2", "S2", "D3", "S3", "D4", "S4", "S5"], "the summon-only records, in load order");
    assert!(names[7..].iter().take(units.len()).all(|n| units.contains(n)), "a card among the summon-only records: {names:?}");
    assert_eq!(&names[7 + units.len()..], &[KING_TOWER, PRINCESS_TOWER], "the fallback towers follow the units");
}

// ---------------------------------------------------------------------------
// (2)

/// Plant: check_levels_one_deep.
#[test]
fn the_level_check_walks_the_whole_chain() {
    let db = chains();
    let root = db.index("Root").expect("Root loads");
    let legendary_first = db.rarity("Legendary").expect("the shipped table has Legendary").relative_level + 1;
    assert!(legendary_first > 1, "vacuous: a Legendary row has a level 1 in this table");
    match db.check_levels(root, 1) {
        Ok(()) => panic!("Root at level 1: check_levels passed, but its grandchild Leaf has no level 1"),
        Err(e) => assert!(e.starts_with("Leaf (Legendary) has no level 1 "), "Root at level 1: {e}"),
    }
    db.check_levels(root, legendary_first).unwrap_or_else(|e| panic!("Root at level {legendary_first}: {e}"));
    let shallow = db.index("Shallow").expect("Shallow loads");
    db.check_levels(shallow, 1).unwrap_or_else(|e| panic!("Shallow at level 1: {e}"));
}

// ---------------------------------------------------------------------------
// (3)

/// Plant: chain_failure_first_root_only.
#[test]
fn a_failing_link_refuses_every_card_that_reaches_it() {
    let db = chains();
    for card in ["Twin1", "Twin2"] {
        assert!(db.index(card).is_none(), "{card} is registered, though its unit's own unit has no row");
        assert_eq!(rejected(&db, card), Some("units.Shared: spawned unit Missing has no units record"), "{card}");
    }
    // the broken record is a unit, not a card of the file: unregistered, its blocks dropped, not listed
    assert!(db.index("Shared").is_none(), "the broken unit is still registered");
    assert!(rejected(&db, "Shared").is_none(), "the broken unit is listed among the rejected cards");
    let shared = db.cards.iter().find(|c| c.name == "Shared").expect("Shared was loaded");
    assert!(shared.death_spawn.is_none(), "the broken unit kept its death spawn");
    // every other card loads
    for card in ["Root", "Other", "Shallow"] {
        assert!(db.index(card).is_some(), "{card} refused: {:?}", rejected(&db, card));
    }
}

// ---------------------------------------------------------------------------
// (4)

/// Plant: chain_failure_first_root_only.
#[test]
fn the_chain_stops_at_max_chain_depth() {
    assert_eq!(MAX_CHAIN_DEPTH, 4, "the chains of this file are written for a limit of 4");
    let db = chains();
    let why = rejected(&db, "Deep").unwrap_or_else(|| panic!("Deep loads: {:?}", death_chain(&db, "Deep")));
    assert_eq!(why, "units.D1: units.D2: units.D3: units.D4: units.D5 itself spawns units (D6) 5 levels below a card, deeper than the 4 this loader follows");
    assert!(db.cards.iter().all(|c| c.name != "D5" && c.name != "D6"), "a record past the limit was loaded");
    assert_eq!(death_chain(&db, "Shallow").len(), 5, "the longest chain the loader follows: five units below the card");
}

// ---------------------------------------------------------------------------
// (5)

/// Plant: unit_needs_first.
#[test]
fn a_card_refused_by_a_table_row_leaves_no_unit_behind() {
    let db = chains();
    let why = rejected(&db, "Hound").expect("Hound loads without its death projectile's row");
    assert!(why.starts_with("death projectile NoSuchShot: "), "{why}");
    assert!(db.cards.iter().all(|c| c.name != "Pup" && c.name != "units.Pup"), "the refused Hound left its Pup behind");
    // The 15.535.29 SuperLavaHound: its death projectile chains a second projectile, which is not simulated, and its
    // death spawn SuperLavaHound2 (whose own death spawn is the Lava Pups) is never loaded, so no later record moves.
    let db = cards();
    let why = rejected(&db, "SuperLavaHound").expect("SuperLavaHound is refused");
    assert!(why.starts_with("death projectile FireWallProjectile: "), "{why}");
    assert!(db.cards.iter().all(|c| c.name != "SuperLavaHound2" && c.name != "units.SuperLavaHound2"), "the refused SuperLavaHound left SuperLavaHound2 behind");
}

// ---------------------------------------------------------------------------
// (6)

/// Plant: ids_one_level.
#[test]
fn every_link_reports_the_card_at_the_top() {
    let db = chains();
    let catalogue = default_catalogue(&db);
    let ids = ids_of_indices(&db, &catalogue);
    let cid = |card: &str| catalogue.iter().position(|i| db.get(*i).name == card).unwrap_or_else(|| panic!("{card} not in the catalogue")) as i32;
    let id_of = |unit: &str| ids[db.cards.iter().position(|c| c.summon_only && c.name == unit).unwrap_or_else(|| panic!("no unit {unit}"))];
    assert_eq!([id_of("Mid"), id_of("Leaf")], [cid("Root"), cid("Root")], "Root's child and grandchild");
    assert_eq!([id_of("S1"), id_of("S3"), id_of("S5")], [cid("Shallow"); 3], "Shallow's chain, down to its fifth unit");
    assert_eq!(id_of("Imp"), cid("Other"));
}

// ---------------------------------------------------------------------------
// (7)

/// The new `team` units of `name` on this frame, relative to `from`, native, in team_seq order.
fn new_units(s: &BattleState, team: Team, name: &str, before: &[EntityId], from: Vec2) -> Vec<(EntityId, i32, i32)> {
    let mut v: Vec<(u32, EntityId, i32, i32)> =
        find_live(s, team, name).iter().filter(|e| !before.contains(&e.id)).map(|e| (e.team_seq, e.id, (e.pos.x - from.x) / K, (e.pos.y - from.y) / K)).collect();
    v.sort();
    v.into_iter().map(|(_, id, dx, dy)| (id, dx, dy)).collect()
}

/// Kill `id` and tick once: (the new units of `child` on its death frame, relative to where it stood).
fn kill(s: &mut BattleState, id: EntityId, child: &str) -> Vec<(EntityId, i32, i32)> {
    let at = s.entity(id).expect("alive").pos;
    let before: Vec<EntityId> = find_live(s, Team::Blue, child).iter().map(|e| e.id).collect();
    assert!(s.debug_set_hp(id, 0));
    s.tick();
    assert!(s.entity(id).is_none(), "it did not die");
    new_units(s, Team::Blue, child, &before, at)
}

/// Plants: chain_refused, death_ring_facing.
#[test]
fn the_elixir_golem_chain_loads_and_plays() {
    let db = cards();
    let golem = db.index("ElixirGolem").unwrap_or_else(|| panic!("the Elixir Golem is refused: {:?}", rejected(&db, "ElixirGolem")));
    assert_eq!(death_chain(&db, "ElixirGolem"), ["ElixirGolem2", "ElixirGolem4"], "three generations");
    db.check_levels(golem, 11).unwrap_or_else(|e| panic!("the Elixir Golem at level 11: {e}"));
    let catalogue = default_catalogue(&db);
    let ids = ids_of_indices(&db, &catalogue);
    let blob = db.cards.iter().position(|c| c.summon_only && c.name == "ElixirGolem4").expect("ElixirGolem4 loads");
    let gid = catalogue.iter().position(|i| *i == golem).expect("the Elixir Golem is a catalogue card") as i32;
    assert_eq!(ids[blob], gid, "the grandchild reports the Elixir Golem's id");

    let mut cfg = BattleConfig::with_cards(db);
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(3, cfg);
    // walking up its lane, so its heading is on the y axis and a facing ring would lay the pair along y
    let g = s.scenario_spawn_now(Team::Blue, "ElixirGolem", Vec2::new(3500 * K, 10000 * K), None).expect("put the Elixir Golem down");
    for _ in 0..6 {
        s.tick();
    }
    let f = s.entity(g).expect("the Elixir Golem walks").facing;
    assert!(f.y > 4 * f.x.abs(), "the scene drifted: the Elixir Golem faces {f:?}, off the y axis");
    // each death lays its pair at +-750 on the arena's x axis, the lower team_seq on -x (open: key order). Allowed:
    // the dying unit's own step on its death tick and the pair's first step (each under 100 native); a facing ring
    // would put the pair at +-750 on y
    let on_x_axis = |pair: &[(EntityId, i32, i32)], what: &str| {
        assert_eq!(pair.len(), 2, "{what}: not a pair: {pair:?}");
        assert!(pair.iter().all(|p| p.2.abs() <= 250), "{what}: the pair is off the x axis: {pair:?}");
        assert!((-900..=-600).contains(&pair[0].1) && (600..=900).contains(&pair[1].1), "{what}: the pair is not at -750 then +750: {pair:?}");
    };
    let halves = kill(&mut s, g, "ElixirGolem2");
    on_x_axis(&halves, "ElixirGolem1's death");
    for _ in 0..4 {
        s.tick();
    }
    let blobs = kill(&mut s, halves[0].0, "ElixirGolem4");
    on_x_axis(&blobs, "ElixirGolem2's death");
}
