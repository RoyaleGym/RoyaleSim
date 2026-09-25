//! spawner.DEATH_SPAWN_AT_EMISSION_POINT's unit list is matched by NAME against the dying unit's
//! `unit_name`, so a misspelt entry is not an error anywhere else: that unit would simply keep the ring. This
//! pins that every listed name is a unit the card table knows, loadable or refused with a reason, and that
//! every LOADABLE listed unit has both halves the rule needs: a spawner and a death spawn.
//! The behaviour itself is pinned in tests/test_spawner_leftover_and_death_point.py (Python, through the
//! module's calibration_overrides, on the measured arm and the old one).
mod common;

use common::cards;
use royalesim::state::Calib;

#[test]
fn every_unit_on_the_death_at_emission_list_is_a_spawner_with_a_death_spawn() {
    let c = Calib::shipped();
    let db = cards();
    assert!(!c.death_spawn_at_emission_units.is_empty(), "the list is empty, so the rule could never apply");
    for u in &c.death_spawn_at_emission_units {
        let loaded = (0..db.cards.len()).map(|i| db.get(i as u16)).find(|d| d.unit_name == *u);
        let refused = db.rejected.iter().any(|(n, _)| n == u);
        match loaded {
            Some(d) => {
                assert!(d.spawner.is_some(), "{u} is on the list but has no spawner, so it has no emission point");
                assert!(d.death_spawn.is_some(), "{u} is on the list but has no death spawn");
            }
            None => assert!(refused, "{u} is on spawner.DEATH_SPAWN_AT_EMISSION_POINT's list but no unit of the card table has that name"),
        }
    }
}
