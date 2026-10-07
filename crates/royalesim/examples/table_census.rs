//! THE LOADER'S CENSUS OF A CARD TABLE: every `cards` row, evolution and hero form the engine's loader (card.rs
//! `CardDb::from_json_str`) refuses, with why, and the counts of what it takes. A diagnostic, not a gate: it answers
//! "what does the engine make of this table" for a table no battle runs yet (the 160402017 one, option B).
//!
//!     cargo run --release --example table_census -- data/derived/cards-160402017-20261006.json [more tables]
//!
//! Exit 1 when a table does not load at all (the loader's own Err), else 0, refusals or not.

use royalesim::card::{CardDb, CardSource};

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: table_census <cards json> [more]");
        std::process::exit(2);
    }
    let mut failed = false;
    for p in paths {
        let text = match std::fs::read_to_string(&p) {
            Ok(t) => t,
            Err(e) => {
                println!("{p}: unreadable: {e}");
                failed = true;
                continue;
            }
        };
        let db = match CardDb::from_json_str(&text, CardSource::DerivedJson) {
            Ok(db) => db,
            Err(e) => {
                println!("{p}: DOES NOT LOAD: {e}");
                failed = true;
                continue;
            }
        };
        let summon_only = db.cards.iter().filter(|c| c.summon_only).count();
        println!(
            "{p}: version {} | {} records ({} summon-only) | {} evolutions, {} hero forms | refused: {} rows, {} evolutions, {} hero forms",
            db.version,
            db.cards.len(),
            summon_only,
            db.forms.len(),
            db.hero_forms.len(),
            db.rejected.len(),
            db.rejected_evolutions.len(),
            db.rejected_forms.len()
        );
        for (what, list) in [("row", &db.rejected), ("evolution", &db.rejected_evolutions), ("hero form", &db.rejected_forms)] {
            for (name, why) in list {
                println!("  REFUSED {what} {name}: {why}");
            }
        }
    }
    std::process::exit(i32::from(failed));
}
