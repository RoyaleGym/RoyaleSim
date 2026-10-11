//! ONE CLIENT'S MEASURED SETUP (src/client_arms.rs `client_overrides`; Python `royalesim.client_arms`).
//!
//! Plants (each a cfg in the engine source, aimed at the tests named):
//!   * `client_arms_keep_card_values` -- the 15.535 setup leaves the current client's card values on:
//!     the_15535_setup_runs_its_arms_and_the_tables_own_card_values.

use royalesim::client_arms::{client_overrides, CARD_VALUES_KEY, CLIENT15535_ARMS};
use royalesim::state::{Calib, CardValuesArm};
use std::collections::BTreeSet;

/// THE 15.535 SETUP: every measured arm of `CLIENT15535_ARMS` and cards.CLIENT16402_VALUES at arm none, applied as a
/// battle's overrides would be, the ledger loading with them. The ledger ships the current client's values; the setup
/// turns them off, so the tables' own run (what a replay of a 15.535.29 capture runs). Any 15.535 build is given it.
#[test]
fn the_15535_setup_runs_its_arms_and_the_tables_own_card_values() {
    let o = client_overrides("15.535.29").expect("15.535.29 is a known client");
    assert_eq!(o.len(), CLIENT15535_ARMS.len() + 1, "every arm and the card values key");
    for (k, v) in CLIENT15535_ARMS {
        assert_eq!(o.get(*k).map(String::as_str), Some(*v), "{k}");
    }
    assert_eq!(Calib::shipped().card_values, CardValuesArm::Client16402, "the ledger ships the current client's card values");
    let (c, applied) = Calib::shipped_with_overrides(&o).expect("the 15.535 setup loads");
    assert_eq!(applied.len(), o.len());
    assert!(applied.contains_key(CARD_VALUES_KEY));
    assert_eq!(c.card_values, CardValuesArm::None, "the 15.535 setup runs the tables' own card values");
    assert_eq!(client_overrides("15.535").expect("15.535"), o);
    assert_eq!(client_overrides("15.535.13").expect("15.535.13"), o);
}

/// THE CURRENT CLIENT IS THE LEDGER: "16.402" and any 16.402 build need no override. Any other name is refused, and the
/// error names the known clients.
#[test]
fn the_current_client_is_the_ledger_and_an_unknown_client_is_refused() {
    assert!(client_overrides("16.402").expect("16.402").is_empty());
    assert!(client_overrides("16.402.17").expect("16.402.17").is_empty());
    for bad in ["", "15", "15.5", "15.535.", "15.535x", "15.535.29a", "16.4", "17.0"] {
        let err = client_overrides(bad).expect_err(bad);
        assert!(err.contains("15.535.29") && err.contains("16.402"), "{bad}: {err}");
    }
}

/// EVERY ARM NAMES ITS OWN LEDGER KEY, once, as `section.KEY`, with a JSON value.
#[test]
fn every_arm_names_a_distinct_ledger_key_with_a_json_value() {
    let mut seen = BTreeSet::new();
    for (k, v) in CLIENT15535_ARMS {
        assert!(seen.insert(*k), "{k} listed twice");
        assert!(k.split_once('.').is_some(), "{k} is not section.KEY");
        assert!(serde_json::from_str::<serde_json::Value>(v).is_ok(), "{k}: {v} is not JSON");
    }
    assert!(!seen.contains(CARD_VALUES_KEY), "the card values key is the setup's own, not an arm");
}
