//! THE ENTITY ROW'S LEVEL COLUMN (py.rs ENTITY_FIELDS `level`, `state_json_text`; state.rs `EntityView::level`): the
//! unified level each entity plays at, as the engine gave it.
//!
//! THE RULES, each the engine's own (the column reports them; it decides none):
//!   - a played unit stands at its card level, a crown tower at its tower level;
//!   - a Mirror's copy at the Mirror's level plus the table's MIRROR_LEVEL_OFFSET (1): its card level plus one;
//!   - a Clone's copy at the level spells.CLONE_LEVEL gives it: the Clone's (spell_level, shipped), the original's
//!     under original_level;
//!   - a unit another puts down (a spawner's, a death spawn) at its parent's level (`CardDb::unit_level`).
//!
//! WHAT IS PINNED, each at a level no other rule in the scene gives, so a column that read the side's card level (or
//! a constant) goes red:
//!   1. the column is ENTITY_FIELDS' last but the mount, and a Knight played at 12 reads 12 while every crown tower of a battle
//!      whose towers are at 13 reads 13;
//!   2. a Mirror after a level-11 Knight: the Knight reads 11, its copy 12;
//!   3. a level-11 Clone on a level-12 Knight in a battle whose cards are at 13 and whose towers are at 14: the copy
//!      reads 11 (spell_level), and 12 under original_level;
//!   4. a level-13 Tombstone in a battle at 11: its Skeletons read 13; a level-13 Golem: its Golemites read 13.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::py::{ids_of_indices, state_json_text, ENTITY_FIELDS};
use royalesim::state::{BattleConfig, BattleState, CloneLevel, EntityView};
use royalesim::{EntityId, Team};
use std::collections::BTreeMap;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn uid(e: &EntityView) -> i64 {
    (e.team_seq as i64) * 2 + e.team as i64
}

/// The level column of every entity row `state_json_text` writes, by uid, the catalogue every card that loads.
fn level_column(s: &BattleState) -> BTreeMap<i64, i64> {
    let db = s.cards();
    let catalogue: Vec<u16> = (0..db.cards.len() as u16).filter(|i| !db.get(*i).summon_only && db.index(&db.get(*i).name) == Some(*i)).collect();
    let ids = ids_of_indices(db, &catalogue);
    let text = state_json_text(s, db, &ids, &[[0, 1, 2], [0, 1, 2]], &BTreeMap::new()).expect("state_json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("state_json is JSON");
    let col = ENTITY_FIELDS.iter().position(|f| *f == "level").expect("ENTITY_FIELDS names the level");
    v["entities"].as_array().expect("entities").iter().map(|r| (r[0].as_i64().unwrap(), r[col].as_i64().expect("a level is an int"))).collect()
}

fn level_of(s: &BattleState, id: EntityId) -> i64 {
    let e = s.entity(id).expect("the entity lives");
    level_column(s)[&uid(&e)]
}

fn first_of(s: &BattleState, team: Team, card: &str) -> EntityId {
    let mut v = find_live(s, team, card);
    v.sort_by_key(|e| e.team_seq);
    v.first().unwrap_or_else(|| panic!("scene: no {card}")).id
}

// ---------------------------------------------------------------------------
// 1. a played unit, the towers

#[test]
fn a_played_unit_reads_its_card_level_and_a_tower_its_tower_level() {
    assert_eq!(ENTITY_FIELDS[ENTITY_FIELDS.len() - 2..], ["level", "mount_uid"], "the level, then the mount, end the row (the Gym's EntityState ends in them)");
    let mut cfg = config();
    cfg.card_level = [12, 12];
    cfg.tower_level = [13, 13];
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "Knight", at((9500, 9500)), None).expect("play a Knight");
    s.tick();
    assert_eq!(level_of(&s, first_of(&s, Team::Blue, "Knight")), 12, "the Knight at its card level");
    let towers: Vec<i64> = [Team::Blue, Team::Red].into_iter().flat_map(|t| s.tower_ids(t)).flatten().map(|id| level_of(&s, id)).collect();
    assert_eq!(towers, vec![13; 6], "every crown tower at its tower level");
}

// ---------------------------------------------------------------------------
// 2. a Mirror's copy

#[test]
fn a_mirrors_copy_reads_its_card_level_plus_one() {
    let deck: Vec<String> = ["Knight", "Archer", "Giant", "Musketeer", "Mirror", "Valkyrie", "HogRider", "Minions"].iter().map(|s| s.to_string()).collect();
    let mut cfg = config();
    cfg.card_level = [11, 11];
    cfg.decks = [deck.clone(), deck];
    cfg.shuffle_decks = false;
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10000);
    s.deploy_slot(Team::Blue, 0, at((9500, 9500))).expect("play the Knight");
    s.scenario_set_elixir_milli(Team::Blue, 10000);
    assert_eq!(s.hand(Team::Blue)[0], "Mirror", "scene: the Mirror came up into slot 0");
    s.deploy_slot(Team::Blue, 0, at((5500, 10500))).expect("the Mirror of the Knight");
    run_until(&mut s, 10, |s| find_live(s, Team::Blue, "Knight").len() == 2);
    let mut knights = find_live(&s, Team::Blue, "Knight");
    knights.sort_by_key(|e| e.team_seq);
    let (original, copy) = (knights[0].id, knights[1].id);
    assert_eq!((level_of(&s, original), level_of(&s, copy)), (11, 12), "the Knight at 11, the Mirror's copy at 12");
}

// ---------------------------------------------------------------------------
// 3. a Clone's copy

/// A level-12 Knight, then a level-11 Clone on it, in a battle whose cards are at 13 and whose towers are at 14, so
/// that no level but the Clone's and the Knight's is 11 or 12: the copy's level column.
fn copy_level(mut cfg: BattleConfig) -> i64 {
    cfg.card_level = [13, 13];
    cfg.tower_level = [14, 14];
    let mut s = BattleState::new(0, cfg);
    let towers: Vec<i64> = [Team::Blue, Team::Red].into_iter().flat_map(|t| s.tower_ids(t)).flatten().map(|id| level_of(&s, id)).collect();
    assert_eq!((s.config().card_level, towers), ([13, 13], vec![14; 6]), "scene: the side's card level and every tower's level");
    s.spawn_unit(Team::Blue, "Knight", at((9500, 9500)), Some(12)).expect("a level-12 Knight");
    for _ in 0..25 {
        s.tick();
    }
    let k = first_of(&s, Team::Blue, "Knight");
    assert_eq!(level_of(&s, k), 12, "scene: the Knight reads 12");
    let p = s.entity(k).expect("the Knight").pos;
    s.spawn_unit(Team::Blue, "Clone", p, Some(11)).expect("a level-11 Clone");
    s.tick();
    let copy = s.entities().find(|e| e.cloned).map(|e| e.id).expect("one copy");
    level_of(&s, copy)
}

#[test]
fn a_clones_copy_reads_the_level_the_clone_gives_it() {
    let shipped = config();
    assert_eq!(shipped.calib.clone_level, CloneLevel::SpellLevel, "the shipped spells.CLONE_LEVEL this test names");
    assert_eq!(copy_level(shipped), 11, "spell_level: the copy at the Clone's 11");
    let mut other = config();
    other.calib.clone_level = CloneLevel::OriginalLevel;
    assert_eq!(copy_level(other), 12, "original_level: the copy at the Knight's 12");
}

// ---------------------------------------------------------------------------
// 4. a spawner's unit, a death spawn

#[test]
fn a_spawned_and_a_death_spawned_unit_read_their_parents_level() {
    let mut cfg = config();
    cfg.card_level = [11, 11];
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "Tombstone", at((9500, 9500)), Some(13)).expect("a level-13 Tombstone");
    s.spawn_unit(Team::Blue, "Golem", at((3500, 10500)), Some(13)).expect("a level-13 Golem");
    run_until(&mut s, 400, |s| !find_live(s, Team::Blue, "Skeleton").is_empty());
    let skel = first_of(&s, Team::Blue, "Skeleton");
    assert_eq!(level_of(&s, first_of(&s, Team::Blue, "Tombstone")), 13, "scene: the Tombstone reads 13");
    assert_eq!(level_of(&s, skel), 13, "the Tombstone's Skeleton at its parent's 13, not the side's 11");
    let golem = first_of(&s, Team::Blue, "Golem");
    assert!(s.debug_set_hp(golem, 0), "scene: the Golem stands");
    run_until(&mut s, 20, |s| !find_live(s, Team::Blue, "Golemite").is_empty());
    let mites: Vec<i64> = find_live(&s, Team::Blue, "Golemite").iter().map(|e| e.id).collect::<Vec<_>>().into_iter().map(|id| level_of(&s, id)).collect();
    assert_eq!(mites, vec![13, 13], "the Golem's two Golemites at its 13");
}

// ---------------------------------------------------------------------------
// the mount column

/// The mount_uid column of every entity row `state_json_text` writes, by uid.
fn mount_column(s: &BattleState) -> BTreeMap<i64, i64> {
    let db = s.cards();
    let catalogue: Vec<u16> = (0..db.cards.len() as u16).filter(|i| !db.get(*i).summon_only && db.index(&db.get(*i).name) == Some(*i)).collect();
    let ids = ids_of_indices(db, &catalogue);
    let text = state_json_text(s, db, &ids, &[[0, 1, 2], [0, 1, 2]], &BTreeMap::new()).expect("state_json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("state_json is JSON");
    let col = ENTITY_FIELDS.iter().position(|f| *f == "mount_uid").expect("ENTITY_FIELDS names the mount");
    v["entities"].as_array().expect("entities").iter().map(|r| (r[0].as_i64().unwrap(), r[col].as_i64().expect("a mount uid is an int"))).collect()
}

#[test]
fn a_rider_reads_its_mounts_uid_and_every_other_entity_minus_one() {
    let mut s = BattleState::new(0, config());
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "RamRider", at((9500, 9500)), None).expect("play a Ram Rider");
    s.tick();
    let col = mount_column(&s);
    let riders: Vec<(i64, i64)> = s.entities().filter_map(|e| e.attached_to.and_then(|m| s.entity(m)).map(|m| (uid(&e), uid(&m)))).collect();
    assert_eq!(riders.len(), 1, "the scene drifted: the Ram Rider put down {} riders", riders.len());
    for (rider, mount) in &riders {
        assert_eq!(col[rider], *mount, "the rider's row names its mount");
    }
    let others: Vec<i64> = col.iter().filter(|(u, _)| !riders.iter().any(|(r, _)| r == *u)).map(|(_, m)| *m).collect();
    assert!(!others.is_empty() && others.iter().all(|m| *m == -1), "an entity that rides nothing reads -1: {others:?}");
}
