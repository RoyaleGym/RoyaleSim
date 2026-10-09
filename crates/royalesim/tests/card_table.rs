//! THE CARD TABLE A BATTLE LOADS (calibration cards.CARD_TABLE; card.rs `CardDb::load_table`, state.rs `CardTable`).
//!
//! Option B moves the engine's card tables to the current client. Until the 160402017 table is scored and the ledger
//! flips, a battle picks it by the key. Pinned:
//!   1. the ledger ships the 15.535.29 table, and that arm loads it;
//!   2. the 160402017 arm loads that client's table, with its own values (the Royal Ghost's 450 hitpoints, 473 on
//!      15.535.29);
//!   3. its compiled-in copy is the committed file, byte for byte, and loads the same cards;
//!   4. a battle saved under the 160402017 arm restores on that table (`BattleState::load`), and one saved under the
//!      shipped arm on the 15.535.29 table;
//!   5. the install-time arm (client160402017, content 16.402.2) loads that table, with the values from before the
//!      2026-10-06 update (the Royal Ghost's 473, the Skeletons' 700 ring), the same cards as the updated table, and
//!      a battle saved under it restores on it.
//!
//! Plant card_table_unread: every arm loads the 15.535.29 table and every blob restores on it, so 2, 4 and 5 go red.
use royalesim::card::{
    CardDb, CardSource, CARDS_160402017_FILE, CARDS_160402017_INSTALL_VERSION, EMBEDDED_CARDS_160402017_JSON,
};
use royalesim::fixed::milli;
use royalesim::state::{BattleConfig, BattleState, Calib, CardTable};
use std::collections::BTreeMap;

const V15535: &str = "cards-15535.1";
const V160402017: &str = "cards-160402017-20261006.1";

/// The shipped ledger with cards.CARD_TABLE = `name`.
fn arm(name: &str) -> Calib {
    let mut o = BTreeMap::new();
    o.insert("cards.CARD_TABLE".to_string(), format!("{name:?}"));
    Calib::shipped_with_overrides(&o).expect("the override applies").0
}

fn ghost_hitpoints(db: &CardDb) -> i32 {
    db.get(db.index("Ghost").expect("the Royal Ghost loads")).hitpoints
}

#[test]
fn the_ledger_ships_the_15535_table() {
    assert_eq!(Calib::shipped().card_table, CardTable::Client15535);
    let db = CardDb::load_table(Calib::shipped().card_table).expect("the shipped table loads");
    assert_eq!(db.version, V15535);
    assert_eq!(ghost_hitpoints(&db), 473);
    assert_eq!(arm("client15535").card_table, CardTable::Client15535);
}

#[test]
fn the_160402017_arm_loads_that_clients_table() {
    let c = arm("client160402017_20261006");
    assert_eq!(c.card_table, CardTable::Client160402017);
    let db = CardDb::load_table(c.card_table).expect("the 160402017 table loads");
    assert_eq!(db.version, V160402017, "the arm loads its own table");
    assert_eq!(ghost_hitpoints(&db), 450, "the 160402017 table's own Royal Ghost");
    assert!(db.cards.len() > 100, "the full table: {}", db.cards.len());
}

#[test]
fn the_compiled_in_160402017_table_is_the_committed_file() {
    let on_disk = std::fs::read_to_string(CardDb::repo_file_path(CARDS_160402017_FILE)).expect("the committed table");
    // A Windows checkout may hand include_str! the file with \r\n; the engine's JSON reader does not care.
    assert_eq!(EMBEDDED_CARDS_160402017_JSON.replace("\r\n", "\n"), on_disk.replace("\r\n", "\n"));
    let embedded = CardDb::from_json_str(EMBEDDED_CARDS_160402017_JSON, CardSource::Embedded).expect("the copy parses");
    let file = CardDb::load_repo_file(CARDS_160402017_FILE).expect("the file loads");
    let names = |db: &CardDb| db.cards.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    assert_eq!(embedded.version, V160402017);
    assert_eq!(names(&embedded), names(&file));
}

#[test]
fn a_saved_battle_restores_on_the_table_it_ran() {
    for (name, version) in [
        ("client15535", V15535),
        ("client160402017_20261006", V160402017),
        ("client160402017", CARDS_160402017_INSTALL_VERSION),
    ] {
        let c = arm(name);
        let mut cfg = BattleConfig::with_cards(CardDb::load_table(c.card_table).expect("the table loads"));
        cfg.calib = c;
        let mut s = BattleState::new(7, cfg);
        for _ in 0..20 {
            s.tick();
        }
        let back = BattleState::load(&s.save()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.config().cards.version, version, "{name}");
        assert_eq!(back.state_hash(), s.state_hash(), "{name}");
    }
}

#[test]
fn the_install_arm_loads_the_table_from_before_the_update() {
    let c = arm("client160402017");
    assert_eq!(c.card_table, CardTable::Client160402017Install);
    let db = CardDb::load_table(c.card_table).expect("the install-time table loads");
    assert_eq!(db.version, CARDS_160402017_INSTALL_VERSION, "the arm loads its own table");
    assert_eq!(ghost_hitpoints(&db), 473, "the Royal Ghost before the 2026-10-06 update (450 after it)");
    let radius = |db: &CardDb| db.get(db.index("Skeletons").expect("Skeletons load")).formation.summon_radius;
    let updated = CardDb::load_table(CardTable::Client160402017).expect("the updated table");
    assert_eq!((radius(&db), radius(&updated)), (milli(700), milli(400)), "the update's Skeletons ring");
    let names = |db: &CardDb| db.cards.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&db), names(&updated), "the update changes values, not the roster");
}
