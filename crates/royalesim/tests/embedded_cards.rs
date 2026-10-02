//! THE CARD TABLE AN INSTALLED WHEEL RUNS (card.rs `EMBEDDED_CARDS_JSON`, `CardDb::load_repo`).
//!
//! Away from the checkout it was built in, the engine loads the compiled-in copy of data/derived/cards-15.535.json.
//! Pinned:
//!   1. the compiled-in copy is the committed file, byte for byte;
//!   2. it loads as `CardSource::Embedded` with the same cards, in the same order, as the file read from disk.
use royalesim::card::{CardDb, CardSource, EMBEDDED_CARDS_JSON};

#[test]
fn the_compiled_in_table_is_the_committed_file() {
    let on_disk = std::fs::read_to_string(CardDb::repo_file_path("cards-15.535.json")).expect("cards-15.535.json");
    // A Windows checkout may hand include_str! the file with \r\n; the engine's JSON reader does not care.
    assert_eq!(EMBEDDED_CARDS_JSON.replace("\r\n", "\n"), on_disk.replace("\r\n", "\n"));
}

#[test]
fn the_compiled_in_table_loads_the_same_cards() {
    let embedded = CardDb::from_json_str(EMBEDDED_CARDS_JSON, CardSource::Embedded).expect("the copy parses");
    let file = CardDb::load_repo_file("cards-15.535.json").expect("the file loads");
    assert_eq!(embedded.source, CardSource::Embedded);
    let names = |db: &CardDb| db.cards.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&embedded), names(&file));
    assert!(embedded.cards.len() > 100, "the full table, not the four-card fallback: {}", embedded.cards.len());
}
