//! ONE LEVEL PAST A RARITY'S COUNT (card.rs `LEVELS_PAST_COUNT`, `RarityRow::tail`).
//!
//! The live 16.402 game's max-level cards stand at unified level 17, one past every rarity's LevelCount (rarities.csv
//! caps each at 16), on their ladder's next multiplier: Common's PowerLevelMultiplier entry past the count, 450 after
//! 409. Measured on live 2026-10-08 battles (the reader's own per-unit level): every level-17 unit's max_hp is exactly
//! its base x 450 % -- a Minion 90 -> 405, a P.E.K.K.A 1469 -> 6610, a Golden Knight 703 -> 3163. Pinned:
//!   1. a Common, an Epic and a Champion card each load at 17 and scale by 450 %, on both 160402017 tables and the
//!      15.535.29 one;
//!   2. two past the count is still refused as a PLAY (`check_levels`), though the ladder runs on for a level the
//!      battle reaches itself (a hero's level-up: 18 on 495 %); nothing below 17 moves;
//!   3. a battle whose deck is at 17 builds, and its unit stands at its level-17 hitpoints.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test level_past_count`):
//!   levels_past_count_refused   a level past the count is refused, as before: 1 and 3 go red.
mod common;

use royalesim::card::CardDb;
use royalesim::state::{BattleConfig, BattleState, CardTable};
use royalesim::Team;

fn hp_at(db: &CardDb, name: &str, level: i32) -> Result<i32, String> {
    let i = db.index(name).unwrap_or_else(|| panic!("{name} loads"));
    db.scaled(i, level, db.get(i).hitpoints)
}

#[test]
fn a_card_plays_one_level_past_its_count_on_its_ladders_next_multiplier() {
    for table in [CardTable::Client160402017, CardTable::Client160402017Install, CardTable::Client15535] {
        let db = CardDb::load_table(table).expect("the table loads");
        // (card, level-1 hitpoints): a Common, an Epic and a Champion
        for (name, base) in [("Minions", 90), ("Pekka", 1469), ("GoldenKnight", 703)] {
            let i = db.index(name).unwrap_or_else(|| panic!("{table:?}: {name} loads"));
            assert_eq!(db.get(i).hitpoints, base, "{table:?} {name}: the measured base");
            assert_eq!(hp_at(&db, name, 16), Ok(base * 409 / 100), "{table:?} {name}: level 16 unchanged");
            assert_eq!(hp_at(&db, name, 17), Ok(base * 450 / 100), "{table:?} {name}: level 17 on 450 %");
            assert!(db.check_levels(i, 18).is_err(), "{table:?} {name}: two past the count is no play");
            assert_eq!(hp_at(&db, name, 18), Ok(base * 495 / 100), "{table:?} {name}: a level-up's 18, the tail's next");
        }
    }
    let db = CardDb::load_table(CardTable::Client160402017).expect("the 10-06 table");
    assert_eq!(hp_at(&db, "Minions", 17), Ok(405), "the live Minion");
    assert_eq!(hp_at(&db, "Pekka", 17), Ok(6610), "the live P.E.K.K.A");
    assert_eq!(hp_at(&db, "GoldenKnight", 17), Ok(3163), "the live Golden Knight");
}

#[test]
fn a_deck_at_seventeen_builds_and_its_unit_stands_at_its_hitpoints() {
    let db = CardDb::load_table(CardTable::Client160402017).expect("the 10-06 table");
    let mut cfg = BattleConfig::with_cards(db);
    cfg.calib.card_table = CardTable::Client160402017;
    cfg.decks = [vec!["Knight".to_string()], vec!["Knight".to_string()]];
    cfg.deck_levels = [vec![17], vec![16]];
    let mut s = BattleState::try_new(1, cfg).expect("a level-17 deck builds");
    s.spawn_unit(Team::Blue, "Knight", common::t(850, 1050), Some(17)).expect("a level-17 Knight goes down");
    s.tick(); // a play's units exist from the next tick
    let knight = common::find_live(&s, Team::Blue, "Knight");
    let base = s.cards().get(s.cards().index("Knight").unwrap()).hitpoints;
    assert_eq!(knight.first().map(|k| k.max_hp), Some(base * 450 / 100), "the Knight at 17");
}
