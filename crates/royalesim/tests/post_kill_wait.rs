//! combat.POST_KILL_RETARGET_WAIT's unit list is matched by NAME against each unit's own name, so a
//! misspelt entry is not an error anywhere else: that unit would simply never wait. This pins that
//! every listed name is a unit the card table knows, loadable or refused with a reason.
//! The behaviour itself is pinned in tests/test_post_kill_retarget_wait.py (Python, through the
//! module's calibration_overrides, on the measured arm and the old one).
mod common;

use common::cards;
use royalesim::state::Calib;

#[test]
fn every_unit_on_the_post_kill_wait_list_is_one_the_card_table_knows() {
    let c = Calib::shipped();
    let db = cards();
    assert!(!c.post_kill_wait_units.is_empty(), "the list is empty, so nothing could ever wait");
    assert_eq!(c.post_kill_wait_ticks, 6, "the measured loss-to-next-target interval");
    for u in &c.post_kill_wait_units {
        let loaded = (0..db.cards.len()).any(|i| db.get(i as u16).name == *u);
        let refused = db.rejected.iter().any(|(n, _)| n == u);
        assert!(loaded || refused, "{u} is on combat.POST_KILL_RETARGET_WAIT's list but no unit of the card table has that name");
    }
}
