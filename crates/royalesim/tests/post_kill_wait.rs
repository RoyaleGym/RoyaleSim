//! combat.POST_KILL_RETARGET_WAIT names units in two lists, both matched by NAME against each unit's
//! `unit_name` (the game's unit name, so Goblin_Stab for a Goblins or Goblin Gang member):
//! value.attack_finish_override_units, clause (a) of the shipped arm client16402_attack_finish, and
//! value.units, read only by the client16402_measured_list arm, which shipped before it and stays a
//! candidate. A misspelt or wrong-vocabulary entry is not an error anywhere else: that unit would
//! simply never match. This pins which arm ships, that every listed name is a unit the card table
//! knows, loadable or refused with a reason, and that the multi-unit cards resolve to their unit (the
//! case that first failed here: Goblin_Stab matched nothing).
//! The behaviour itself is pinned in Python, through the module's calibration_overrides:
//! tests/test_post_kill_retarget_condition.py for the shipped arm (and the shipped build with no
//! override), tests/test_post_kill_retarget_wait.py for the list arm and for the Knight's wait under
//! both arms.
mod common;

use common::cards;
use royalesim::state::{Calib, PostKillWait};

/// The shipped arm, asserted, so a ledger change re-points this file and the two Python files
/// rather than leaving them describing an arm that no longer ships.
#[test]
fn the_shipped_arm_is_the_attack_finish_condition() {
    let c = Calib::shipped();
    assert_eq!(c.post_kill_wait, PostKillWait::AttackFinish, "combat.POST_KILL_RETARGET_WAIT's shipped value.arm");
    assert_eq!(
        PostKillWait::from_calibration_name("client16402_measured_list"),
        Some(PostKillWait::MeasuredList),
        "the list arm stays a loadable candidate"
    );
}

#[test]
fn every_unit_on_the_post_kill_wait_list_is_one_the_card_table_knows() {
    let c = Calib::shipped();
    let db = cards();
    assert!(!c.post_kill_wait_units.is_empty(), "value.units is empty, so under the list arm nothing could ever wait");
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
    assert_eq!(c.post_kill_wait_override_units.len(), 4, "the four names clause (a) reads");
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
