//! THE MIRROR (card.rs `SpellShape::Mirror`, `CardGlobals`; state.rs `resolve_play`, `PlayerState::last_played`).
//!
//! THE LAW, measured on client 15.535.29 and on client 16.402 (capture 20260920-090204):
//!   - playing the Mirror plays its side's last card again, one level above the Mirror's own (MIRROR_LEVEL_OFFSET 1:
//!     a Knight 11 copied by a Mirror 9 stood at level 10), for the copied card's cost plus the Mirror's 1 (a Knight
//!     4, a Fireball 5, the Three Musketeers 10);
//!   - the tap is judged and resolved by the copied card's placement: a troop copy on the enemy half stands where the
//!     troop itself would, a river tap is refused;
//!   - a Mirror's own play is not recorded, a refused play is not recorded, and before any play there is nothing to
//!     copy;
//!   - a copy past the rarity's last level: the client plays it (16 + 16 stood at 17), and so does this engine, one
//!     level past the count (card.rs `LEVELS_PAST_COUNT`); two past is refused
//!     (match.MIRROR_LEVEL_BEYOND_MAX = refuse_play, marked refuted in the ledger). Pinned here as shipped.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the Mirror as a shape of its own and the table's MIRROR_LEVEL_OFFSET; a file without the
//!      global, or with an offset below 1, refuses the card (synthetic file);
//!   2. the Mirror after a Knight, a Fireball and the Three Musketeers costs 4, 5 and 10, and plays the copied card at
//!      the team's level plus 1; its own play cycles the Mirror and leaves the record alone;
//!   3. the copy stands at the level above: its hitpoints are the ladder's one step up;
//!   4. with nothing to copy the Mirror is refused NothingToMirror, and a refused play is not recorded;
//!   5. the copy's tap is judged and resolved exactly as the copied Knight's own play would be;
//!   6. a Fireball copy is the Fireball cast one level up;
//!   7. a copy one past the last level plays at it (a Knight 16's copy stands at 17, 3105 hp); two past is refused
//!      and debits nothing;
//!   8. the record survives a save and load, and is state where a Mirror is in the deck (and only there).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test mirror_card`):
//!   * `mirror_flat_cost` -- the Mirror's own cost alone: (2) goes red.
//!   * `mirror_same_level` -- the copy at the Mirror's own level: (2), (3) and (6) go red.
//!   * `variant_placement_of_hand_card` -- the Mirror's own row judges the tap: (5) goes red.
//!   * `save_drops_last_play` -- the record is lost across a save: (8) goes red.
//!   * `hash_skips_last_play` -- the record is not hashed: (8) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, SpellShape};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeployError, Play};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// A clear point of Blue's own half.
const OWN: (i32, i32) = (9500, 9500);
/// Another, where the copies go down.
const OWN2: (i32, i32) = (5500, 10500);

/// A Blue deck whose first card is `first` and whose fifth is the Mirror: playing slot 0 brings the Mirror into slot
/// 0. The Mirror is never dealt into the starting hand here, whatever rule deals it.
fn deck(first: &str) -> Vec<String> {
    let rest = ["Archer", "Giant", "Musketeer", "Mirror", "Valkyrie", "HogRider", "Minions"];
    std::iter::once(first).chain(rest).map(String::from).collect()
}

/// A battle at `level`, past the lockout, Blue at 10 elixir, with `first` played at `tap` from slot 0 and the Mirror in
/// slot 0 after it.
fn after_playing(first: &str, tap: (i32, i32), level: i32) -> BattleState {
    let mut cfg = config();
    cfg.card_level = [level, level];
    cfg.decks = [deck(first), deck(first)];
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10000);
    s.deploy_slot(Team::Blue, 0, at(tap)).unwrap_or_else(|e| panic!("play the {first}: {e:?}"));
    s.scenario_set_elixir_milli(Team::Blue, 10000);
    assert_eq!(s.hand(Team::Blue)[0], "Mirror", "the Mirror came up from the queue");
    s
}

fn idx(s: &BattleState, name: &str) -> u16 {
    s.cards().index(name).unwrap_or_else(|| panic!("{name} does not load"))
}

// ---------------------------------------------------------------------------
// 1. the loader

#[test]
fn the_loader_reads_the_mirror_and_its_level_offset() {
    let s = BattleState::new(0, config());
    let m = card_stat(&s, "Mirror");
    assert!(m.is_mirror());
    assert_eq!(m.spell.as_ref().map(|d| &d.shape), Some(&SpellShape::Mirror));
    assert_eq!(s.cards().globals.mirror_level_offset, Some(1), "globals.csv MIRROR_LEVEL_OFFSET");
    // Synthetic: the Mirror loads with the global and is refused without it or below 1.
    let file = |globals: &str| {
        format!(
            r#"{{ "version": "test", "cards": [
             {{ "name":"Glass", "kind":"spell", "elixir":1, "rarity":"Epic", "spell": {{ "mirror": true }} }},
             {{ "name":"Glass2", "kind":"spell", "elixir":1, "rarity":"Epic",
                "projectile": {{ "speed": 600, "damage": 41 }}, "spell": {{ "mirror": true }} }}
            ]{globals} }}"#
        )
    };
    let load = |globals: &str| CardDb::from_json_str(&file(globals), CardSource::DerivedJson).expect("the synthetic file parses");
    let why = |db: &CardDb, n: &str| db.rejected.iter().find(|(r, _)| r == n).map(|(_, w)| w.clone());
    let db = load(r#", "globals": { "MIRROR_LEVEL_OFFSET": 1 }"#);
    assert!(db.index("Glass").is_some_and(|i| db.get(i).is_mirror()), "{:?}", db.rejected);
    assert_eq!(db.globals.mirror_level_offset, Some(1));
    assert!(why(&db, "Glass2").is_some_and(|w| w.contains("mechanic of its own")), "a Mirror with a projectile of its own: {:?}", db.rejected);
    let db = load("");
    assert!(why(&db, "Glass").is_some_and(|w| w.contains("MIRROR_LEVEL_OFFSET")), "{:?}", db.rejected);
    let db = load(r#", "globals": { "MIRROR_LEVEL_OFFSET": 0 }"#);
    assert!(why(&db, "Glass").is_some_and(|w| w.contains("MIRROR_LEVEL_OFFSET")), "{:?}", db.rejected);
    let bad = CardDb::from_json_str(&file(r#", "globals": { "MIRROR_LEVEL_OFFSET": "one" }"#), CardSource::DerivedJson);
    assert!(bad.is_err(), "a global of the wrong type refuses the file");
}

// ---------------------------------------------------------------------------
// 2 and 3. the play

/// Plants: mirror_flat_cost, mirror_same_level.
#[test]
fn a_mirror_replays_the_last_card_one_level_up_for_its_cost_plus_its_own() {
    for (first, tap, cost) in [("Knight", OWN, 4), ("Fireball", (9500, 20500), 5), ("ThreeMusketeers", OWN, 10)] {
        let mut s = after_playing(first, tap, 11);
        let (mirror, card) = (idx(&s, "Mirror"), idx(&s, first));
        assert_eq!(s.mirror_target(Team::Blue), Some(card), "{first} is the side's last play");
        let want = Play { in_slot: mirror, card, level: 12, cost };
        assert_eq!(s.resolve_play(Team::Blue, 0), Ok(want), "a Mirror of the {first}");
        assert_eq!(s.hand_costs(Team::Blue)[0], cost, "the hand prices the Mirror of the {first}");
        let (before, unit) = s.elixir_raw(Team::Blue);
        s.deploy_slot(Team::Blue, 0, at(if first == "Fireball" { tap } else { OWN2 })).unwrap_or_else(|e| panic!("the Mirror of the {first}: {e:?}"));
        assert_eq!(before - s.elixir_raw(Team::Blue).0, cost as i64 * unit, "the Mirror of the {first} debits {cost}");
        assert_eq!(s.queue_cards(Team::Blue).last(), Some(&mirror), "the Mirror cycles, not the copy");
        assert_eq!(s.mirror_target(Team::Blue), Some(card), "a Mirror's own play is not recorded");
        if first != "Fireball" {
            assert!(s.pending_spawns().iter().any(|p| p.0 == Team::Blue && p.1 == card), "the copy is queued: {:?}", s.pending_spawns());
        }
    }
}

/// Plant: mirror_same_level.
#[test]
fn the_copy_stands_one_level_up() {
    let mut s = after_playing("Knight", OWN, 11);
    s.deploy_slot(Team::Blue, 0, at(OWN2)).expect("the Mirror of the Knight");
    run_until(&mut s, 10, |s| find_live(s, Team::Blue, "Knight").len() == 2);
    let mut knights = find_live(&s, Team::Blue, "Knight");
    assert_eq!(knights.len(), 2, "the Knight and its copy");
    knights.sort_by_key(|v| v.team_seq);
    let k = idx(&s, "Knight");
    let hp = card_stat(&s, "Knight").hitpoints;
    let (at11, at12) = (s.cards().scaled(k, 11, hp).unwrap(), s.cards().scaled(k, 12, hp).unwrap());
    assert!(at12 > at11);
    assert_eq!((knights[0].max_hp, knights[1].max_hp), (at11, at12), "the original at 11, the copy at 12");
}

// ---------------------------------------------------------------------------
// 4. nothing to copy

#[test]
fn with_nothing_to_copy_the_mirror_is_refused_and_a_refused_play_is_not_recorded() {
    // A four-card deck: the Mirror is in the starting hand with nothing behind the hand to swap it for.
    let mut cfg = config();
    let d: Vec<String> = ["Mirror", "Knight", "Archer", "Giant"].iter().map(|n| n.to_string()).collect();
    cfg.decks = [d.clone(), d];
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10000);
    let slot = s.hand(Team::Blue).iter().position(|c| *c == "Mirror").expect("the Mirror is in hand");
    assert_eq!(s.mirror_target(Team::Blue), None);
    assert_eq!(s.resolve_play(Team::Blue, slot), Err(DeployError::NothingToMirror));
    assert_eq!(s.check_deploy_slot(Team::Blue, slot, at(OWN)), Err(DeployError::NothingToMirror));
    assert_eq!(s.hand_costs(Team::Blue)[slot], -1, "no play resolves from the slot");
    let before = s.elixir_raw(Team::Blue);
    assert_eq!(s.deploy_slot(Team::Blue, slot, at(OWN)), Err(DeployError::NothingToMirror));
    assert_eq!(s.elixir_raw(Team::Blue), before, "a refused Mirror debits nothing");
    // A refused Knight play (the river) records nothing; an accepted one records the Knight.
    let knight = s.hand(Team::Blue).iter().position(|c| *c == "Knight").expect("the Knight is in hand");
    assert!(s.deploy_slot(Team::Blue, knight, at((9500, 16000))).is_err(), "a Knight on the river is refused");
    assert_eq!(s.mirror_target(Team::Blue), None, "a refused play is not recorded");
    s.deploy_slot(Team::Blue, knight, at(OWN)).expect("play the Knight");
    assert_eq!(s.mirror_target(Team::Blue), Some(idx(&s, "Knight")));
    assert_eq!(s.mirror_target(Team::Red), None, "one side's play is not the other's");
}

// ---------------------------------------------------------------------------
// 5. placement

/// Plant: variant_placement_of_hand_card.
#[test]
fn the_copy_is_placed_by_the_copied_cards_rule() {
    // Own half, the enemy half, the river, the own princess tower's footprint.
    for tap in [OWN2, (9500, 20500), (9500, 16000), (3500, 7500)] {
        let mut mirrored = after_playing("Knight", OWN, 11);
        let mut twin = BattleState::new(0, {
            let mut cfg = config();
            cfg.decks = [deck("Knight"), deck("Knight")];
            cfg
        });
        past_deploy_lockout(&mut twin);
        twin.scenario_set_elixir_milli(Team::Blue, 10000);
        assert_eq!(mirrored.check_deploy_slot(Team::Blue, 0, at(tap)), twin.check_deploy_slot(Team::Blue, 0, at(tap)), "the verdict at {tap:?}");
        assert_eq!(mirrored.deploy_slot(Team::Blue, 0, at(tap)), twin.deploy_slot(Team::Blue, 0, at(tap)), "where the copy goes down for a tap at {tap:?}");
    }
}

// ---------------------------------------------------------------------------
// 6. a spell copy

/// Plant: mirror_same_level.
#[test]
fn a_fireball_copy_is_the_fireball_cast_one_level_up() {
    let tap = (9500, 20500);
    let mut s = after_playing("Fireball", tap, 11);
    run_until(&mut s, 200, |s| s.spells().is_empty() && s.pending_spawns().is_empty());
    assert!(s.spells().is_empty(), "the first Fireball is still in flight");
    s.scenario_set_elixir_milli(Team::Blue, 10000);
    s.deploy_slot(Team::Blue, 0, at(tap)).expect("the Mirror of the Fireball");
    run_until(&mut s, 10, |s| !s.spells().is_empty());
    let copy = s.spells().first().cloned().expect("the copy is cast");
    // The Fireball itself played at level 12.
    let mut plain = after_playing("Knight", OWN, 12);
    plain.spawn_unit(Team::Blue, "Fireball", at(tap), None).expect("a Fireball at 12");
    run_until(&mut plain, 10, |s| !s.spells().is_empty());
    let own = plain.spells().first().cloned().expect("the Fireball is cast");
    assert_eq!((copy.card, copy.level, copy.damage), (idx(&s, "Fireball"), 12, own.damage), "the copy: {copy:?}\nthe Fireball at 12: {own:?}");
}

// ---------------------------------------------------------------------------
// 7. past the last level

#[test]
fn a_copy_one_past_the_last_level_plays_there_and_two_past_is_refused() {
    // THE CLIENT'S: client 15.535.29 put a Knight 16 copied by a Mirror 16 down at level 17 (3105 hp = 690 x 450 %, the
    // multiplier list running past the rarity's LevelCount), and the engine plays one level past the count
    // (card.rs `LEVELS_PAST_COUNT`, the live max-level cards' 17).
    let mut s = after_playing("Knight", OWN, 16);
    let play = s.resolve_play(Team::Blue, 0).expect("the copy one past the count");
    assert_eq!(play.level, 17, "{play:?}");
    s.deploy_slot(Team::Blue, 0, at(OWN2)).expect("the copy goes down");
    run_until(&mut s, 10, |s| find_live(s, Team::Blue, "Knight").len() == 2);
    let hps: Vec<i32> = find_live(&s, Team::Blue, "Knight").iter().map(|k| k.max_hp).collect();
    assert!(hps.contains(&3105), "the copy at 17, 690 x 450 %: {hps:?}");
    // Two past the count (a Knight 17's copy at 18) is refused, debits nothing and leaves the Mirror in hand.
    let mut s = after_playing("Knight", OWN, 17);
    assert!(matches!(s.resolve_play(Team::Blue, 0), Err(DeployError::InvalidLevel(_))), "{:?}", s.resolve_play(Team::Blue, 0));
    assert_eq!(s.hand_costs(Team::Blue)[0], -1);
    let before = s.elixir_raw(Team::Blue);
    assert!(matches!(s.deploy_slot(Team::Blue, 0, at(OWN2)), Err(DeployError::InvalidLevel(_))));
    assert_eq!(s.elixir_raw(Team::Blue), before, "a refused copy debits nothing");
    assert_eq!(s.hand(Team::Blue)[0], "Mirror", "a refused Mirror stays in hand");
}

// ---------------------------------------------------------------------------
// 8. the record is state

/// Plant: save_drops_last_play.
#[test]
fn the_last_play_survives_a_save() {
    let mut s = after_playing("Knight", OWN, 11);
    let mut b = BattleState::load(&s.save()).expect("the save loads");
    assert_eq!(b.mirror_target(Team::Blue), s.mirror_target(Team::Blue), "the record crosses the save");
    assert_eq!(b.resolve_play(Team::Blue, 0), s.resolve_play(Team::Blue, 0));
    s.deploy_slot(Team::Blue, 0, at(OWN2)).expect("the Mirror, saved battle");
    b.deploy_slot(Team::Blue, 0, at(OWN2)).expect("the Mirror, loaded battle");
    for _ in 0..40 {
        s.tick();
        b.tick();
        assert_eq!(b.state_hash(), s.state_hash(), "tick {}: the loaded battle parts from the one it was saved from", s.tick_count());
    }
}

/// Plant: hash_skips_last_play.
#[test]
fn the_last_play_is_state_where_a_mirror_is_in_the_deck_and_only_there() {
    // An edit of the record alone: refused by the snapshot's self-check when the deck holds a Mirror.
    let edit = |s: &BattleState, to: u16| -> Vec<u8> {
        let mut v: serde_json::Value = serde_json::from_slice(&s.save()).expect("a snapshot is JSON");
        v["players"][0]["last_played"] = serde_json::Value::from(to);
        serde_json::to_vec(&v).unwrap()
    };
    let s = after_playing("Knight", OWN, 11);
    let archer = idx(&s, "Archer");
    assert!(BattleState::load(&s.save()).is_ok());
    assert!(BattleState::load(&edit(&s, archer)).is_err(), "two states differing only in the side's last play hash alike");
    // A deck with no Mirror: the record is never read, so it is not hashed, and a battle of cards that loaded before
    // the Mirror hashes as it always did.
    let mut cfg = config();
    let d: Vec<String> = ["Knight", "Archer", "Giant", "Musketeer", "Valkyrie", "HogRider", "Minions", "Fireball"].iter().map(|n| n.to_string()).collect();
    cfg.decks = [d.clone(), d];
    let mut plain = BattleState::new(0, cfg);
    past_deploy_lockout(&mut plain);
    plain.deploy_slot(Team::Blue, 0, at(OWN)).expect("play the Knight");
    let edited = BattleState::load(&edit(&plain, archer)).expect("without a Mirror the record is not in the hash");
    assert_eq!(edited.state_hash(), plain.state_hash());
}
