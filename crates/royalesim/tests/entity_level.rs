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
    assert_eq!(
        ENTITY_FIELDS[ENTITY_FIELDS.len() - 6..],
        ["level", "mount_uid", "charge", "dest_x", "dest_y", "ability_ticks"],
        "the level and the mount, then what a player watches, end the row, in the Gym's EntityState's order"
    );
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

// ---------------------------------------------------------------------------
// 5. a level per deck card (BattleConfig::deck_levels)

fn deck() -> Vec<String> {
    ["Knight", "Archer", "Giant", "Musketeer", "Mirror", "Valkyrie", "HogRider", "Minions"].iter().map(|s| s.to_string()).collect()
}

/// A deck whose cards each have their own level, on both sides, unequal between them: each side's Knight plays at its
/// own entry's level and has that level's hitpoints, whatever the side's `card_level`.
#[test]
fn each_deck_card_plays_at_its_own_level_on_each_side() {
    let mut cfg = config();
    cfg.card_level = [11, 11];
    cfg.decks = [deck(), deck()];
    cfg.deck_levels = [vec![9, 11, 11, 11, 11, 11, 11, 11], vec![13, 11, 11, 11, 11, 11, 11, 11]];
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    for team in [Team::Blue, Team::Red] {
        s.scenario_set_elixir_milli(team, 10000);
    }
    s.deploy_slot(Team::Blue, 0, at((9500, 9500))).expect("Blue's Knight");
    s.deploy_slot(Team::Red, 0, at((8500, 22500))).expect("Red's Knight");
    run_until(&mut s, 10, |s| !find_live(s, Team::Blue, "Knight").is_empty() && !find_live(s, Team::Red, "Knight").is_empty());
    let (b, r) = (first_of(&s, Team::Blue, "Knight"), first_of(&s, Team::Red, "Knight"));
    assert_eq!((level_of(&s, b), level_of(&s, r)), (9, 13), "each Knight at its own deck entry's level");
    let hp = |id| s.entity(id).expect("the Knight").max_hp;
    // The same card's hitpoints at the two levels, from a battle that spawns each at it.
    let mut lone = BattleState::new(0, config());
    lone.spawn_unit(Team::Blue, "Knight", at((9500, 9500)), Some(9)).unwrap();
    lone.spawn_unit(Team::Red, "Knight", at((8500, 22500)), Some(13)).unwrap();
    lone.tick();
    let lone_hp = |t| lone.entity(first_of(&lone, t, "Knight")).unwrap().max_hp;
    assert_eq!((hp(b), hp(r)), (lone_hp(Team::Blue), lone_hp(Team::Red)), "each at its level's hitpoints");
    assert!(hp(b) < hp(r), "scene: the levels differ in hitpoints");
}

/// The measured Mirror (client 15.535.29): a Knight 11 copied by a Mirror 9 stands at level 10, the MIRROR's level plus
/// MIRROR_LEVEL_OFFSET, not the copied card's.
#[test]
fn a_mirror_raises_its_own_level_not_the_copied_cards() {
    let mut cfg = config();
    cfg.card_level = [11, 11];
    cfg.decks = [deck(), deck()];
    cfg.deck_levels = [vec![11, 11, 11, 11, 9, 11, 11, 11], Vec::new()];
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
    assert_eq!((level_of(&s, knights[0].id), level_of(&s, knights[1].id)), (11, 10), "the Knight at 11, the Mirror 9's copy at 10");
}

#[test]
fn a_deck_level_list_of_the_wrong_length_is_refused_and_one_kept_through_a_snapshot() {
    let mut cfg = config();
    cfg.decks = [deck(), deck()];
    cfg.deck_levels = [vec![9, 10], Vec::new()];
    let Err(e) = BattleState::try_new(0, cfg.clone()) else { panic!("a two-entry level list for an eight-card deck was accepted") };
    assert!(e.contains("deck_levels[0]"), "{e}");
    cfg.deck_levels = [vec![9; 8], vec![13; 8]];
    let s = BattleState::try_new(0, cfg).expect("one level per card");
    // A setup spawn of a deck card takes its entry's level.
    let mut s2 = s.clone();
    s2.scenario_spawn_now(Team::Red, "Giant", at((8500, 22500)), None).unwrap();
    s2.tick();
    assert_eq!(level_of(&s2, first_of(&s2, Team::Red, "Giant")), 13, "a setup spawn of a deck card at its entry's level");
    let back = BattleState::load_with(&s.save(), s.config().cards.clone(), s.config().arena.clone()).expect("round trip");
    assert_eq!(back.config().deck_levels, [vec![9; 8], vec![13; 8]], "the levels survive a snapshot");
}
