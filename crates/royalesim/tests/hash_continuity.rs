//! A CHANGE'S BATTLES AGAINST ITS PARENT'S, HASH BY HASH.
//!
//! WHY. `state_hash` hashes each entity's CardDb INDEX (state.rs `hash_state`). When a
//! card the loader used to refuse while converting its row starts to load, it takes a
//! slot among the cards, and every later card, both crown towers and every summon-only
//! unit move up one. Every battle's hash VALUE then moves although no battle plays any
//! differently, and a battle that DOES play differently hides in the same movement.
//! The determinism tests compare two runs of one build; no other test here compares a
//! battle's hashes across a change. This file does.
//!
//! WHAT IT RUNS. Five short scripted battles on each shipped table, every play at a
//! fixed tick after the opening deploy lockout, every point given in a player's own
//! frame or by a crown tower:
//!   troop_duel       a Knight each at one bridge, then an Archer behind the blue one
//!                    and a Valkyrie for red;
//!   spawner          a blue Tombstone and a red Hog Rider that goes for it, then a red
//!                    Knight down the other lane;
//!   spell_cast       a blue Giant under a red Fireball and a red Zap, then blue Arrows
//!                    on the red princess tower of the Giant's lane;
//!   building         a red Giant and a blue Cannon, then a Knight each;
//!   king_activation  a blue Knight placed in front of the red king before the first
//!                    tick, which wakes it by hitting it; red Arrows on a blue princess
//!                    tower set to 1 hp, whose fall wakes the blue king.
//! Each battle's `state_hash` before the first tick and after every tick, and the
//! CardDb's slot names in index order, are held against
//! tests/data/hash_continuity.json, recorded at the PARENT of the
//! change under test. A change that plays every battle as its parent did passes. Any
//! other movement fails, naming the battle and the first tick that moved, and the first
//! slot that moved if one did.
//!
//! LOADED_SINCE_PARENT. A change that makes a card load which its parent refused while
//! converting the row lists that row there. This file builds each CardDb from the table
//! WITHOUT those rows, so every other slot is where the parent had it, and the battles
//! (which never play those cards) must hash as before. A row the parent refused AFTER
//! pushing it (a refusal found while loading the units it needs, such as the 15.535.29
//! ElixirGolem's unit chain) kept its slot at the parent, so it is never listed: listing
//! it takes a slot the parent had, and the SLOTS line of the failure names it. When such
//! a row loads, its slot stays, but the units it needs are new summon-only slots and can
//! renumber later ones: that shows as movement too, and is named in the change that
//! loads it. Each change names its own list; the next change starts from an empty one.
//!
//! RECORDING. `ROYALESIM_RECORD_HASH_CONTINUITY=1 cargo test --release --test
//! hash_continuity`, run AT THE PARENT (this file copied onto it if the parent does not
//! have it), never at the change itself: hashes the change records would only restate
//! it. A recording run takes the WHOLE table, since it records what that commit itself
//! hashes for the next change to be held to, writes this table's entry and then FAILS,
//! saying so: a run that wrote its own expectation checked nothing. It needs both tables
//! and refuses under CI. Re-record after a rebase onto new commits (they can move the
//! hashes too) and give the reason in the commit text. A change that moves a loaded
//! card's behaviour on purpose records at itself once the movement is named, so the
//! next change is held to the new battles.
//!
//! THE TABLES. data/derived/cards.json (the 15.535.29 table the engine loads) and
//! data/derived/cards-2018.json (tools/extract_cards.py --vintage 2018). The record
//! holds one entry per table `version`. This file does not go through
//! `common::cards()`, which panics without cards.json: a table that is absent, or that
//! holds another version, is SKIPPED LOUDLY, on one line written past the harness's
//! output capture so that it shows in a passing run too. Under CI a skip is a failure.
//! A table the record holds NO entry for fails, table or no table, until it is recorded:
//! an empty record must never read as a pass, or as a skip. A battle a table cannot set
//! up (a deck card it does not load) runs nowhere on that table, so a refusal is never
//! quiet. When EXPECTED_REFUSALS does not list it, a recording run writes nothing and
//! fails, and a check fails. A listed refusal is recorded and compared as that refusal,
//! with a line past the output capture on every run, since that battle's hashes are not
//! compared. The list is empty, so every battle must run on both tables.
//!
//! WHAT IT CANNOT CATCH. A change that moves only what these five battles never reach.
//! It is a continuity check, not a fidelity check: a battle that was wrong at the parent
//! passes if it is wrong the same way now.
//!
//! THE CHECKS:
//!   1. `the_15535_table_hashes_as_its_parent_did`;
//!   2. `the_2018_table_hashes_as_its_parent_did`;
//!   3. `removing_the_rows_loaded_since_the_parent_realigns_every_index`: on a small
//!      synthetic table, a row that starts to load moves every later slot and the
//!      hash, and removing it gives back the parent's slots and hashes;
//!   4. `the_comparison_names_the_first_tick_that_moved`.
//!
//! 3 and 4 need no table.
//!
//! PLANT: `RUSTFLAGS='--cfg clash_plant="hash_line_unconditional"' CARGO_TARGET_DIR=target/plant
//! cargo test --test hash_continuity`: `hash_state` hashes one more u32 for every alive
//! entity, the shape of a new column hashed at its neutral value -> 1 and 2 red at tick
//! 0 of every battle (once recorded; while the record is empty they are red anyway);
//! 3 and 4 stay green, since both sides of each comparison run under the plant.

use royalesim::arena::Lane;
use royalesim::card::{CardDb, CardSource};
use royalesim::fixed::{Vec2, SUBTILE};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;
use serde_json::{json, Value};
use std::sync::Mutex;

/// data/derived beside the crate, where `CardDb::load_repo_file` reads.
const DERIVED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived");

/// The record: each table's slots and per-tick hashes, as the PARENT commit ran them.
const RECORD: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/hash_continuity.json");

/// Set to 1 to record instead of check (RECORDING in the header).
const RECORD_VAR: &str = "ROYALESIM_RECORD_HASH_CONTINUITY";

/// The record's format. A file of any other format is refused, never read as empty.
const FORMAT: &str = "hash_continuity.1";

/// What the record file says about itself.
const HOW: &str = "Written by crates/royalesim/tests/hash_continuity.rs with ROYALESIM_RECORD_HASH_CONTINUITY=1 at the \
                   PARENT of the change it checks. Per table version: the CardDb slot names in index order, and each \
                   battle's state_hash before the first tick and after every tick, in hex, 8 to a row.";

/// Hashes per row of the record.
const ROW: usize = 8;

/// The rows this change makes loadable that its parent refused while converting them,
/// as (table file, card name). Empty in a change that loads no card. The rule is in the
/// header (LOADED_SINCE_PARENT): never a row the parent refused after pushing it.
const LOADED_SINCE_PARENT: &[(&str, &str)] = &[];

/// The battles a table may refuse to set up, as (table version, battle name, why that is
/// accepted). Any other refusal fails the run that meets it, recording or checking: a
/// refused battle is compared as its refusal from then on, so it would leave the check for
/// good without a word. Changing the battle's decks so the table sets it up keeps the
/// coverage; listing it here gives the coverage up, and says why.
const EXPECTED_REFUSALS: &[(&str, &str, &str)] = &[];

/// Why EXPECTED_REFUSALS accepts that `version` refuses `battle`, or None if it does not.
fn expected_refusal(version: &str, battle: &str) -> Option<&'static str> {
    EXPECTED_REFUSALS.iter().find(|(v, b, _)| *v == version && *b == battle).map(|(_, _, why)| *why)
}

/// One shipped table.
struct Table {
    /// The test that reads it, for the skip line.
    test: &'static str,
    /// Its file under data/derived.
    file: &'static str,
    /// The cards.json `version` its record entry belongs to.
    version: &'static str,
    /// How a checkout gets the file.
    make: &'static str,
}

const TABLE_15535: Table = Table {
    test: "the_15535_table_hashes_as_its_parent_did",
    file: "cards.json",
    version: "cards-15535.1",
    make: "the README's stage 3 copies data/derived/cards-15.535.json to data/derived/cards.json",
};

const TABLE_2018: Table = Table {
    test: "the_2018_table_hashes_as_its_parent_did",
    file: "cards-2018.json",
    version: "cards-2018.1",
    make: "tools/extract_cards.py --vintage 2018 writes it",
};

// ---------------------------------------------------------------------------
// the battles

/// Where a play or a placed unit goes. Hundredths of a tile.
#[derive(Clone, Copy)]
enum At {
    /// A point in the acting player's OWN frame (it defends low y; its own-left is low x).
    Own(i32, i32),
    /// A point in the OPPONENT's own frame: a spell on the other side's units.
    Theirs(i32, i32),
    /// The centre of a team's princess tower on an ENGINE lane.
    Princess(Team, Lane),
    /// This far in front of a team's king tower, toward the river.
    KingFront(Team, i32),
}

/// A card played from hand, `after` ticks after the deploy lockout opens.
struct Play {
    after: u32,
    team: Team,
    card: &'static str,
    at: At,
}

/// What is set up before the first tick.
enum Setup {
    /// A crown tower's hp (`k`: 0 king, 1 engine-Left princess, 2 engine-Right).
    TowerHp { team: Team, k: usize, hp: i32 },
    /// One unit of a card, placed already deployed.
    Unit { team: Team, card: &'static str, at: At },
}

/// One scripted battle. The decks are unshuffled, so the first four cards start in hand;
/// every play is one of them, and no team plays one twice.
struct Battle {
    name: &'static str,
    seed: u64,
    decks: [[&'static str; 8]; 2],
    setup: &'static [Setup],
    plays: &'static [Play],
    /// Ticks run after the lockout opens.
    ticks: u32,
    /// Both kings must be awake at the end (checked when recording).
    kings: bool,
}

const BLUE: Team = Team::Blue;
const RED: Team = Team::Red;

/// The five battles. Elixir: every play is paid from the 6 each side starts with plus one
/// per 2.8 s after the lockout opens, so no play depends on elixir gained before it.
const BATTLES: &[Battle] = &[
    Battle {
        name: "troop_duel",
        seed: 0x4843_0001,
        decks: [
            ["Knight", "Archer", "Giant", "Musketeer", "MiniPekka", "Valkyrie", "HogRider", "Fireball"],
            ["Knight", "Valkyrie", "Giant", "Musketeer", "MiniPekka", "Archer", "HogRider", "Fireball"],
        ],
        setup: &[],
        plays: &[
            // Blue's own-left and Red's own-right are one engine lane.
            Play { after: 0, team: BLUE, card: "Knight", at: At::Own(350, 1200) },
            Play { after: 0, team: RED, card: "Knight", at: At::Own(1450, 1200) },
            Play { after: 20, team: BLUE, card: "Archer", at: At::Own(350, 900) },
            Play { after: 80, team: RED, card: "Valkyrie", at: At::Own(1450, 1200) },
        ],
        ticks: 400,
        kings: false,
    },
    Battle {
        name: "spawner",
        seed: 0x4843_0002,
        decks: [
            ["Tombstone", "Knight", "Archer", "Giant", "Musketeer", "MiniPekka", "Valkyrie", "Fireball"],
            ["HogRider", "Knight", "Archer", "Giant", "Musketeer", "MiniPekka", "Valkyrie", "Fireball"],
        ],
        setup: &[],
        plays: &[
            Play { after: 0, team: BLUE, card: "Tombstone", at: At::Own(900, 1000) },
            Play { after: 30, team: RED, card: "HogRider", at: At::Own(1450, 1400) },
            Play { after: 120, team: RED, card: "Knight", at: At::Own(350, 1200) },
        ],
        ticks: 500,
        kings: false,
    },
    Battle {
        name: "spell_cast",
        seed: 0x4843_0003,
        decks: [
            ["Giant", "Arrows", "Knight", "Archer", "Musketeer", "MiniPekka", "Valkyrie", "HogRider"],
            ["Fireball", "Zap", "Knight", "Archer", "Giant", "Musketeer", "MiniPekka", "Valkyrie"],
        ],
        setup: &[],
        plays: &[
            Play { after: 0, team: BLUE, card: "Giant", at: At::Own(1450, 1400) },
            Play { after: 40, team: RED, card: "Fireball", at: At::Theirs(1450, 1450) },
            Play { after: 70, team: RED, card: "Zap", at: At::Theirs(1450, 1500) },
            Play { after: 150, team: BLUE, card: "Arrows", at: At::Princess(RED, Lane::Right) },
        ],
        ticks: 300,
        kings: false,
    },
    Battle {
        name: "building",
        seed: 0x4843_0004,
        decks: [
            ["Cannon", "Knight", "Archer", "Giant", "Musketeer", "MiniPekka", "Valkyrie", "Fireball"],
            ["Giant", "Knight", "Archer", "Musketeer", "MiniPekka", "Valkyrie", "HogRider", "Fireball"],
        ],
        setup: &[],
        plays: &[
            Play { after: 0, team: RED, card: "Giant", at: At::Own(350, 1400) },
            Play { after: 40, team: BLUE, card: "Cannon", at: At::Own(900, 1000) },
            Play { after: 100, team: BLUE, card: "Knight", at: At::Own(1450, 1200) },
            Play { after: 140, team: RED, card: "Knight", at: At::Own(350, 1200) },
        ],
        ticks: 400,
        kings: false,
    },
    Battle {
        name: "king_activation",
        seed: 0x4843_0005,
        decks: [
            ["Knight", "Archer", "Giant", "Musketeer", "MiniPekka", "Valkyrie", "HogRider", "Fireball"],
            ["Arrows", "Knight", "Archer", "Giant", "Musketeer", "MiniPekka", "Valkyrie", "Fireball"],
        ],
        setup: &[
            Setup::TowerHp { team: BLUE, k: 1, hp: 1 },
            Setup::Unit { team: BLUE, card: "Knight", at: At::KingFront(RED, 300) },
        ],
        plays: &[Play { after: 0, team: RED, card: "Arrows", at: At::Princess(BLUE, Lane::Left) }],
        ticks: 300,
        kings: true,
    },
];

/// The engine point `at` names for `team`.
fn point(s: &BattleState, team: Team, at: At) -> Vec2 {
    let a = s.arena();
    match at {
        At::Own(x, y) => a.from_frame(team, Vec2::from_tiles_100(x, y)),
        At::Theirs(x, y) => a.from_frame(team.other(), Vec2::from_tiles_100(x, y)),
        At::Princess(owner, lane) => a.princess_tower_pos(owner, lane),
        At::KingFront(owner, dy) => {
            let k = a.to_frame(owner, a.king_tower_pos(owner));
            a.from_frame(owner, Vec2::new(k.x, k.y + dy * (SUBTILE / 100)))
        }
    }
}

/// What a battle gave on one table.
#[derive(Debug, PartialEq)]
enum Ran {
    /// `state_hash` before the first tick and after every tick.
    Hashes(Vec<u64>),
    /// The table cannot set the battle up (`BattleState::try_new`'s refusal).
    Refused(String),
}

/// Run `b` on `db`: what it gave, and whether each king (blue, red) is awake at the end.
/// A refused setup step or play PANICS: the script no longer runs as written, which is
/// not a hash movement and must be fixed (at the parent too) before anything is compared.
fn run(db: &CardDb, b: &Battle) -> (Ran, [bool; 2]) {
    let mut cfg = BattleConfig::with_cards(db.clone());
    cfg.decks = [b.decks[0].iter().map(|c| c.to_string()).collect(), b.decks[1].iter().map(|c| c.to_string()).collect()];
    let mut s = match BattleState::try_new(b.seed, cfg) {
        Ok(s) => s,
        Err(why) => return (Ran::Refused(why), [false, false]),
    };
    for step in b.setup {
        match *step {
            Setup::TowerHp { team, k, hp } => {
                if let Err(e) = s.scenario_set_tower_hp(team, k, hp) {
                    panic!("{}: the setup of {team:?}'s tower {k} at {hp} hp was refused: {e}", b.name);
                }
            }
            Setup::Unit { team, card, at } => {
                let p = point(&s, team, at);
                if let Err((_, e)) = s.scenario_spawn_batch(&[(team, card, p, None)]) {
                    panic!("{}: the setup {team:?} {card} at ({}, {}) was refused: {e:?}", b.name, p.x, p.y);
                }
            }
        }
    }
    let open = s.config().calib.deploy_lockout_ticks.max(0) as u32;
    let end = open + b.ticks;
    let mut played = 0;
    let mut hashes = vec![s.state_hash()];
    while s.tick_count() < end && !s.is_done() {
        let now = s.tick_count();
        for p in b.plays.iter().filter(|p| open + p.after == now) {
            let pos = point(&s, p.team, p.at);
            if let Err(e) = s.deploy(p.team, p.card, pos) {
                panic!(
                    "{}: the script's {:?} {} at ({}, {}) on tick {now} was refused: {e:?}. The battle no longer runs as \
                     written, which is not a hash movement: fix the script, at the parent too, before comparing",
                    b.name, p.team, p.card, pos.x, pos.y
                );
            }
            played += 1;
        }
        s.tick();
        hashes.push(s.state_hash());
    }
    assert_eq!(played, b.plays.len(), "{}: the battle ended on tick {} before its every play was made", b.name, s.tick_count());
    (Ran::Hashes(hashes), [s.king_active(Team::Blue), s.king_active(Team::Red)])
}

// ---------------------------------------------------------------------------
// the table, the record and the comparison

/// The CardDb of the cards.json document `doc` without the `cards` rows named in `removed`.
/// The document goes through serde_json whether or not a row is removed, so a table with
/// nothing removed loads exactly as one with rows removed. The loader keeps the
/// file's arrays in order and reads its maps sorted by key (card.rs `RawCardsFile`), and
/// a round trip through serde_json keeps both.
fn db_without(doc: &Value, removed: &[&str], what: &str) -> CardDb {
    let mut d = doc.clone();
    let rows = d["cards"].as_array_mut().unwrap_or_else(|| panic!("{what}: no `cards` array"));
    for name in removed {
        let before = rows.len();
        rows.retain(|r| r["name"].as_str() != Some(*name));
        assert_eq!(before - rows.len(), 1, "{what}: LOADED_SINCE_PARENT names {name}, which is not exactly one row of `cards`");
    }
    CardDb::from_json_str(&d.to_string(), CardSource::DerivedJson).unwrap_or_else(|e| panic!("{what}: the loader refused the whole file: {e}"))
}

/// The CardDb's slot names in index order: cards, crown towers, summon-only units, and
/// the rows refused after their push, which keep their slot.
fn slots(db: &CardDb) -> Vec<String> {
    db.cards.iter().map(|c| c.name.clone()).collect()
}

/// One shipped table, read.
struct Loaded {
    text: String,
    doc: Value,
    /// A line naming the bytes it came from.
    what: String,
}

/// `table` from data/derived. Err(why to skip) when the file is absent or holds another
/// version. A table that is there and does not parse is a failure, not a skip.
fn load(table: &Table) -> Result<Loaded, String> {
    let path = format!("{DERIVED}/{}", table.file);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(format!("{path} is absent ({})", table.make)),
        Err(e) => panic!("{path}: {e}"),
    };
    let doc: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
    let found = doc["version"].as_str().unwrap_or("(none)").to_string();
    let what = format!("{path} (version {found}, {} bytes, FNV-1a 64 {:016x})", text.len(), fnv1a64(text.as_bytes()));
    if found != table.version {
        return Err(format!("{what} is not the {} table its record entry belongs to ({})", table.version, table.make));
    }
    Ok(Loaded { text, doc, what })
}

/// FNV-1a 64 of a table's bytes, so a report says which bytes it is about.
fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

/// Hashes as record rows: 16 hex digits each, `ROW` to a row.
fn hex_rows(hashes: &[u64]) -> Vec<Value> {
    hashes.chunks(ROW).map(|c| Value::String(c.iter().map(|h| format!("{h:016x}")).collect::<Vec<_>>().join(" "))).collect()
}

/// The hashes of record rows.
fn parse_rows(rows: &Value, what: &str) -> Vec<u64> {
    rows.as_array()
        .unwrap_or_else(|| panic!("{what}: `hashes` is not an array of rows"))
        .iter()
        .flat_map(|row| row.as_str().unwrap_or_else(|| panic!("{what}: a row is not a string: {row}")).split_whitespace())
        .map(|h| u64::from_str_radix(h, 16).unwrap_or_else(|e| panic!("{what}: {h:?} is not a hash: {e}")))
        .collect()
}

/// A string array of the record.
fn strings(v: &Value, what: &str) -> Vec<String> {
    v.as_array()
        .unwrap_or_else(|| panic!("{what}: not an array"))
        .iter()
        .map(|s| s.as_str().unwrap_or_else(|| panic!("{what}: {s} is not a string")).to_string())
        .collect()
}

/// None when the slots are the recorded ones; else where they first part.
fn slot_diff(recorded: &[String], now: &[String]) -> Option<String> {
    if recorded == now {
        return None;
    }
    let k = recorded.iter().zip(now).position(|(a, b)| a != b).unwrap_or(recorded.len().min(now.len()));
    let then_had = recorded.get(k).map_or("nothing", String::as_str);
    let now_has = now.get(k).map_or("nothing", String::as_str);
    Some(format!(
        "the CardDb slots first differ at index {k}: the parent had {then_had}, this build has {now_has} ({} slots then, {} now)",
        recorded.len(),
        now.len()
    ))
}

/// None when `now` is `recorded`; else where they first part. Index k is the hash after
/// k ticks (0: before the first tick).
fn first_moved(recorded: &[u64], now: &[u64]) -> Option<String> {
    if recorded == now {
        return None;
    }
    Some(match recorded.iter().zip(now).position(|(a, b)| a != b) {
        Some(k) => format!("the hash first differs at tick {k}: recorded {:016x}, now {:016x}", recorded[k], now[k]),
        None => format!(
            "the hashes agree for as long as both run, and the battle ran {} ticks where it ran {}",
            now.len().saturating_sub(1),
            recorded.len().saturating_sub(1)
        ),
    })
}

/// Both table tests may record at once, and one may read while the other writes: the
/// file is only read or written under this.
static RECORD_FILE: Mutex<()> = Mutex::new(());

/// The whole record file.
fn read_record() -> Value {
    let _held = RECORD_FILE.lock().unwrap_or_else(|e| e.into_inner());
    read_record_file()
}

/// The whole record file, `RECORD_FILE` held. An absent or blank file, or `{}`, is a
/// record with no tables.
fn read_record_file() -> Value {
    let empty = || json!({ "format": FORMAT, "how": HOW, "tables": {} });
    let text = match std::fs::read_to_string(RECORD) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return empty(),
        Err(e) => panic!("{RECORD}: {e}"),
    };
    if text.trim().is_empty() {
        return empty();
    }
    let v: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{RECORD}: {e}"));
    if v.as_object().is_some_and(|o| o.is_empty()) {
        return empty();
    }
    assert_eq!(v["format"].as_str(), Some(FORMAT), "{RECORD} is not a {FORMAT} record, and is never read as an empty one");
    assert!(v["tables"].is_object(), "{RECORD}: `tables` is not an object");
    v
}

/// Put one table's entry into the record file, read, changed and written in one hold.
fn write_entry(version: &str, entry: Value) {
    let _held = RECORD_FILE.lock().unwrap_or_else(|e| e.into_inner());
    let mut rec = read_record_file();
    rec["format"] = json!(FORMAT);
    rec["how"] = json!(HOW);
    rec["tables"][version] = entry;
    let dir = std::path::Path::new(RECORD).parent().expect("the record has a directory");
    std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    let text = serde_json::to_string_pretty(&rec).expect("the record serializes") + "\n";
    std::fs::write(RECORD, text).unwrap_or_else(|e| panic!("{RECORD}: {e}"));
}

/// Is this a recording run? The variable is 1 to record, unset (or empty, or 0) to check.
fn recording() -> bool {
    match std::env::var(RECORD_VAR) {
        Ok(v) if v == "1" => true,
        Ok(v) if v.is_empty() || v == "0" => false,
        Ok(v) => panic!("{RECORD_VAR}={v:?}: set it to 1 to record, or unset it to check"),
        Err(_) => false,
    }
}

/// SKIP LOUDLY. The line goes straight to the process's stderr, which the harness's
/// capture does not intercept, so it shows in a passing run too; the caller then
/// returns. Under CI it is a failure instead: the workflow writes both tables.
fn skip(test: &str, why: &str) {
    if std::env::var_os("CI").is_some() {
        panic!("{why} -- and CI is set, where a skip would read as a pass");
    }
    let line = format!("\nSKIP hash_continuity::{test}: {why}. The hashes were NOT compared: this is not a pass.\n");
    let _ = std::io::Write::write_all(&mut std::io::stderr(), line.as_bytes());
}

/// Record `table` from its whole table, then fail: a recording run checked nothing.
fn record(table: &Table, t: &Loaded) -> ! {
    assert!(std::env::var_os("CI").is_none(), "{RECORD_VAR} is set under CI, where a run must never write its own expectation");
    let db = db_without(&t.doc, &[], &t.what);
    assert!(!db.towers_from_fallback, "{}: the file has no crown towers, and the fallback pair is not its rows", t.what);
    let mut battles = serde_json::Map::new();
    let mut refused = Vec::new();
    let mut unlisted = Vec::new();
    for b in BATTLES {
        let (ran, kings) = run(&db, b);
        let entry = match ran {
            Ran::Hashes(h) => {
                if let Some(why) = expected_refusal(table.version, b.name) {
                    panic!(
                        "{}: EXPECTED_REFUSALS lists {} on the {} table ({why}), and this table sets it up. Nothing was \
                         recorded: take it off the list, which must name only the refusals the table makes",
                        t.what, b.name, table.version
                    );
                }
                assert!(
                    !b.kings || kings == [true, true],
                    "{}: {} ends with the kings awake {kings:?} (blue, red), not both: it does not do what it is named for. Fix the \
                     script before recording it",
                    t.what,
                    b.name
                );
                json!({ "ticks": h.len() - 1, "hashes": hex_rows(&h) })
            }
            Ran::Refused(why) => {
                if expected_refusal(table.version, b.name).is_none() {
                    unlisted.push(format!("{}: {why}", b.name));
                }
                refused.push(b.name);
                json!({ "refused": why })
            }
        };
        battles.insert(b.name.to_string(), entry);
    }
    assert!(
        unlisted.is_empty(),
        "{}: the {} table cannot set up {unlisted:?}, and EXPECTED_REFUSALS does not list it. Nothing was recorded: a \
         recorded refusal is compared as a refusal from then on, so that battle would leave the check for good. Change its \
         decks so this table sets it up, or list it there with the reason",
        t.what,
        table.version
    );
    assert!(refused.len() < BATTLES.len(), "{}: the table sets up none of the battles, so a record of it would pin nothing", t.what);
    write_entry(
        table.version,
        json!({
            "file": table.file,
            "bytes": t.text.len(),
            "fnv1a64": format!("{:016x}", fnv1a64(t.text.as_bytes())),
            "slots": slots(&db),
            "battles": battles
        }),
    );
    panic!(
        "RECORDED the {} table into {RECORD} from {} ({} battles, refused by this table: {refused:?}). This run CHECKED \
         NOTHING: it wrote the expectation the next change is held to. Run again without {RECORD_VAR} to check",
        table.version,
        t.what,
        BATTLES.len()
    );
}

/// Hold `table` against its record entry.
fn compare(table: &Table, t: &Loaded, entry: &Value) {
    let what = &t.what;
    let removed: Vec<&str> = LOADED_SINCE_PARENT.iter().filter(|(f, _)| *f == table.file).map(|(_, n)| *n).collect();
    if !removed.is_empty() {
        let full = db_without(&t.doc, &[], what);
        for name in &removed {
            let loads = full.index(name).is_some_and(|i| !full.get(i).summon_only);
            assert!(loads, "{what}: LOADED_SINCE_PARENT names {name}, which this build does not load: a row that does not load here did not start to load since the parent");
        }
    }
    let db = db_without(&t.doc, &removed, what);
    assert!(!db.towers_from_fallback, "{what}: the file has no crown towers, and the fallback pair is not its rows");
    let mut moved: Vec<String> = Vec::new();
    if let Some(d) = slot_diff(&strings(&entry["slots"], "the record's slots"), &slots(&db)) {
        moved.push(format!("  SLOTS: {d}"));
    }
    let recorded = entry["battles"].as_object().unwrap_or_else(|| panic!("{RECORD}: the {} entry has no `battles`", table.version));
    for b in BATTLES {
        let Some(rec) = recorded.get(b.name) else {
            moved.push(format!("  {}: not in the record, which was made before this battle was added: record again at the parent", b.name));
            continue;
        };
        let (ran, _) = run(&db, b);
        match (ran, rec.get("refused").and_then(Value::as_str)) {
            (Ran::Refused(now), Some(then)) if now == then => match expected_refusal(table.version, b.name) {
                Some(accepted) => {
                    let line = format!(
                        "\nNOT RUN hash_continuity::{}: the {} table refuses {} as it did at the parent ({now}), so its \
                         hashes were NOT compared. EXPECTED_REFUSALS accepts it: {accepted}\n",
                        table.test, table.version, b.name
                    );
                    let _ = std::io::Write::write_all(&mut std::io::stderr(), line.as_bytes());
                }
                None => moved.push(format!(
                    "  {}: refused at the parent and now alike ({now}), and EXPECTED_REFUSALS does not list it: its hashes \
                     were not compared",
                    b.name
                )),
            },
            (Ran::Refused(now), Some(then)) => moved.push(format!("  {}: refused at the parent with {then:?}, now with {now:?}", b.name)),
            (Ran::Refused(now), None) => moved.push(format!("  {}: ran at the parent, and this table now refuses it: {now}", b.name)),
            (Ran::Hashes(_), Some(then)) => moved.push(format!("  {}: refused at the parent ({then}), and runs now", b.name)),
            (Ran::Hashes(now), None) => {
                let then = parse_rows(&rec["hashes"], b.name);
                assert_eq!(rec["ticks"].as_u64(), Some(then.len().saturating_sub(1) as u64), "{RECORD}: {} holds a `ticks` its hashes do not have", b.name);
                if let Some(d) = first_moved(&then, &now) {
                    moved.push(format!("  {}: {d}", b.name));
                }
            }
        }
    }
    for name in recorded.keys().filter(|n| !BATTLES.iter().any(|b| b.name == n.as_str())) {
        moved.push(format!("  {name}: in the record, and this file no longer runs it"));
    }
    if moved.is_empty() {
        return;
    }
    panic!(
        "{what}: THE BATTLES MOVED against the record of this table at the parent (rows removed first, LOADED_SINCE_PARENT: \
         {removed:?}).\n{}\n\nThe record was made from {} bytes, FNV-1a 64 {}. A movement is one of: a change that moves a \
         loaded card's behaviour on purpose (name the card and the reason in the commit text, then record at this commit \
         with {RECORD_VAR}=1 so the next change is held to it); a card that started to load since the parent and is not in \
         LOADED_SINCE_PARENT, or a row listed there that kept its slot (the SLOTS line names the index); or a finding, a \
         change that moves battles it did not mean to.\n",
        moved.join("\n"),
        entry["bytes"],
        entry["fnv1a64"].as_str().unwrap_or("(none)")
    );
}

/// The check for one table: record, or compare, or skip, or fail as unrecorded.
fn check_table(table: &Table) {
    for (file, name) in LOADED_SINCE_PARENT {
        assert!(
            [TABLE_15535.file, TABLE_2018.file].contains(file),
            "LOADED_SINCE_PARENT names {name} in {file}, which is neither table this file reads"
        );
    }
    for (version, battle, _) in EXPECTED_REFUSALS {
        assert!(
            [TABLE_15535.version, TABLE_2018.version].contains(version) && BATTLES.iter().any(|b| b.name == *battle),
            "EXPECTED_REFUSALS names {battle} on {version}, which is not a battle of this file on a table it reads"
        );
    }
    let recording = recording();
    let entry = read_record()["tables"].get(table.version).cloned();
    if !recording && entry.is_none() {
        let here = match load(table) {
            Ok(t) => t.what,
            Err(why) => why,
        };
        panic!(
            "NOT RECORDED: {RECORD} holds no entry for the {} table, so this test fails until it does. Record it at the \
             PARENT of the change under test, never at the change itself (hashes the change records only restate it): \
             {RECORD_VAR}=1 cargo test --release --test hash_continuity. The table here: {here}",
            table.version
        );
    }
    let t = match load(table) {
        Ok(t) => t,
        Err(why) if recording => panic!("cannot record the {} table: {why}", table.version),
        Err(why) => {
            skip(table.test, &why);
            return;
        }
    };
    if recording {
        record(table, &t);
    }
    compare(table, &t, &entry.expect("checked above"));
}

// ---------------------------------------------------------------------------
// (1), (2)

/// Plant: hash_line_unconditional.
#[test]
fn the_15535_table_hashes_as_its_parent_did() {
    check_table(&TABLE_15535);
}

/// Plant: hash_line_unconditional.
#[test]
fn the_2018_table_hashes_as_its_parent_did() {
    check_table(&TABLE_2018);
}

// ---------------------------------------------------------------------------
// (3)

/// A small table: Alpha, Beta and Epsilon, three plain troops with the fields the
/// fallback Knight carries, no crown towers (the fallback pair follows every card).
/// Beta has no hit_speed_ms unless `beta_loads`, which the loader refuses while
/// converting the row, before it takes a slot.
fn small(beta_loads: bool) -> Value {
    let row = |name: &str, loads: bool| {
        let mut r = json!({
            "name": name, "kind": "troop", "elixir": 3, "rarity": "Common",
            "hitpoints": 660, "damage": 75, "load_time_ms": 700, "speed": 60,
            "range_milli": 1000, "sight_range_milli": 5500, "collision_radius_milli": 500, "mass": 6,
            "deploy_time_ms": 1000, "attacks_air": false, "attacks_ground": true,
            "target_only_buildings": false, "flying_height": 0, "count": 1
        });
        if loads {
            r["hit_speed_ms"] = json!(1100);
        }
        r
    };
    json!({ "version": "test", "cards": [row("Alpha", true), row("Beta", beta_loads), row("Epsilon", true)] })
}

/// `state_hash` before the first tick and after each of three, on a battle with no
/// decks: the crown towers alone, whose card index is hashed.
fn tower_hashes(db: &CardDb) -> Vec<u64> {
    let mut s = BattleState::new(7, BattleConfig::with_cards(db.clone()));
    let mut out = vec![s.state_hash()];
    for _ in 0..3 {
        s.tick();
        out.push(s.state_hash());
    }
    out
}

#[test]
fn removing_the_rows_loaded_since_the_parent_realigns_every_index() {
    let parent = db_without(&small(false), &[], "the parent's table");
    let change = db_without(&small(true), &[], "the change's table");
    let aligned = db_without(&small(true), &["Beta"], "the change's table without Beta");
    // The premise: Beta loads in the change and was refused at the parent before it
    // took a slot.
    assert!(parent.index("Beta").is_none(), "{:?}", parent.rejected);
    assert!(change.index("Beta").is_some(), "{:?}", change.rejected);
    assert!(!slots(&parent).iter().any(|n| n == "Beta"), "the parent kept a slot for Beta: {:?}", slots(&parent));
    // Without the removal every later slot moves up one, the towers included, and the
    // slot comparison says where.
    let moved = slot_diff(&slots(&parent), &slots(&change)).expect("Beta's slot moves every later one");
    assert!(moved.contains("index 1: the parent had Epsilon, this build has Beta"), "{moved}");
    assert_ne!(tower_hashes(&change)[0], tower_hashes(&parent)[0], "a battle without Beta in it hashes as before once Beta takes a slot: state_hash no longer hashes the index, and the removal list is no longer needed");
    // With it, every slot and every hash is the parent's.
    assert_eq!(slot_diff(&slots(&parent), &slots(&aligned)), None);
    assert_eq!(tower_hashes(&aligned), tower_hashes(&parent));
}

// ---------------------------------------------------------------------------
// (4)

#[test]
fn the_comparison_names_the_first_tick_that_moved() {
    let then = [1_u64, 2, 3, 4];
    assert_eq!(first_moved(&then, &then), None);
    let report = first_moved(&then, &[1, 2, 9, 4]).expect("a moved hash is reported");
    assert!(report.contains("at tick 2: recorded 0000000000000003, now 0000000000000009"), "{report}");
    let report = first_moved(&then, &then[..3]).expect("a shorter battle is reported");
    assert!(report.contains("ran 2 ticks where it ran 3"), "{report}");
    let report = first_moved(&then[..3], &then).expect("a longer battle is reported");
    assert!(report.contains("ran 3 ticks where it ran 2"), "{report}");
    // The slot comparison, on a slot taken and on a slot lost at the end.
    fn names(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }
    assert_eq!(slot_diff(&names(&["A", "B"]), &names(&["A", "B"])), None);
    let report = slot_diff(&names(&["A", "C", "K"]), &names(&["A", "B", "C", "K"])).expect("a new slot is reported");
    assert!(report.contains("index 1: the parent had C, this build has B (3 slots then, 4 now)"), "{report}");
    let report = slot_diff(&names(&["A", "B"]), &names(&["A"])).expect("a lost slot is reported");
    assert!(report.contains("index 1: the parent had B, this build has nothing"), "{report}");
    // The record's rows read back what they were written from, across a row boundary.
    let hashes: Vec<u64> = (0..=ROW as u64 * 2).map(|k| k.wrapping_mul(0x9e37_79b9_7f4a_7c15)).chain([0, u64::MAX]).collect();
    let rows = Value::Array(hex_rows(&hashes));
    assert_eq!(rows.as_array().map(Vec::len), Some(hashes.len().div_ceil(ROW)));
    assert_eq!(parse_rows(&rows, "the round trip"), hashes);
}
