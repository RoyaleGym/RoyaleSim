//! placement.TROOP_TOWER_TAPS through `spawn_unit`, the replay harness's deploy path.
//!
//! `spawn_unit` skips legality on purpose (the game already enforced it), so the king-area relocation has to live in the
//! placement resolution it shares with the play path, or the corpus never sees it. Side 0's recorded king taps at own
//! (8500, 1500) stand, in the game, relocated to row 0: a Goblins group laid around own (8500, 500), not a tile forward
//! over the king. Under the old arm the tap stands where it was given.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, TapSnap, TroopTowerTaps};
use royalesim::Team;

fn native(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn goblins_at_the_king(arm: TroopTowerTaps) -> (Vec2, Vec<Vec2>) {
    let mut cfg = config();
    cfg.calib.placement_troop_tower_taps = arm;
    cfg.calib.placement_tap_snap = TapSnap::TileCentre;
    let mut s = BattleState::new(1, cfg);
    let idx = s.cards().index("Goblins").expect("data: Goblins loads");
    let tap = native(8500, 1500);
    let resolved = s.resolve_point(Team::Blue, idx, tap);
    s.spawn_unit(Team::Blue, "Goblins", tap, None).expect("spawn_unit takes the recorded tap");
    let laid = s.pending_spawns().into_iter().filter(|(t, c, _)| *t == Team::Blue && s.cards().get(*c).name == "Goblins").map(|(_, _, p)| p).collect();
    (resolved, laid)
}

#[test]
fn spawn_unit_relocates_a_goblins_tap_on_the_own_king_to_row_0() {
    let (resolved, laid) = goblins_at_the_king(TroopTowerTaps::HalfOpenRelocate);
    assert_eq!(resolved, native(8500, 500), "the tap on the king's tile relocates to the tile below it");
    assert!(!laid.is_empty(), "nothing was laid");
    for p in &laid {
        assert!(p.y < 1400 * K, "a member stands over the king's tile row: {p:?} (native y {})", p.y / K);
    }
}

#[test]
fn the_old_arm_lays_the_tap_where_it_was_given() {
    let (resolved, laid) = goblins_at_the_king(TroopTowerTaps::ClosedBlock);
    assert_eq!(resolved, native(8500, 1500), "the old arm must not relocate");
    assert!(laid.iter().any(|p| p.y >= 1400 * K), "the old arm's ring should reach the king's tile row: {laid:?}");
}
