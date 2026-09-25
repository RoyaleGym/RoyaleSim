//! combat.POST_KILL_RETARGET_WAIT's unit list is matched by NAME against each unit's `unit_name` (the
//! game's unit name, so Goblin_Stab for a Goblins or Goblin Gang member), and a misspelt or wrong-
//! vocabulary entry is not an error anywhere else: that unit would simply never wait. This pins that
//! every listed name is a unit the card table knows, loadable or refused with a reason, and that the
//! multi-unit cards resolve to their unit (the case that first failed here: Goblin_Stab matched nothing).
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
        let loaded = (0..db.cards.len()).any(|i| db.get(i as u16).unit_name == *u);
        let refused = db.rejected.iter().any(|(n, _)| n == u);
        assert!(loaded || refused, "{u} is on combat.POST_KILL_RETARGET_WAIT's list but no unit of the card table has that name");
    }
}

#[test]
fn every_attack_finish_override_unit_is_one_the_card_table_knows() {
    let c = Calib::shipped();
    let db = cards();
    assert_eq!(c.post_kill_wait_override_units.len(), 4, "the four 15.535 OverrideAttackFinishTime cards");
    for u in &c.post_kill_wait_override_units {
        let loaded = (0..db.cards.len()).any(|i| db.get(i as u16).unit_name == *u);
        let refused = db.rejected.iter().any(|(n, _)| n == u);
        assert!(loaded || refused, "{u} is on value.attack_finish_override_units but no unit of the card table has that name");
    }
}

/// The doomed test counts HOMING shots only; the flag comes from the data, so pin that it arrives.
#[test]
fn projectile_homing_is_read_from_the_card_table() {
    let db = cards();
    let homing = |name: &str| (0..db.cards.len()).map(|i| db.get(i as u16)).find(|d| d.name == name).map(|d| (d.projectile.is_some(), d.projectile_homing));
    assert_eq!(homing("Musketeer"), Some((true, true)), "the Musketeer's shot homes");
    assert_eq!(homing("Knight"), Some((false, false)), "a direct hitter has no projectile");
    if let Some(b) = homing("Bomber") {
        assert_eq!(b, (true, false), "the Bomber's bomb does not home");
    }
}

#[test]
fn a_multi_unit_cards_members_carry_the_games_unit_name() {
    let db = cards();
    for (card, unit) in [("Goblins", "Goblin_Stab"), ("GoblinGang", "Goblin_Stab"), ("Skeletons", "Skeleton"), ("Bats", "Bat"), ("Knight", "Knight")] {
        let Some(i) = (0..db.cards.len()).find(|&i| db.get(i as u16).name == card) else {
            assert!(db.rejected.iter().any(|(n, _)| n == card), "{card} is neither loaded nor refused");
            continue;
        };
        assert_eq!(db.get(i as u16).unit_name, unit, "{card}'s members are the game's {unit}");
    }
}
