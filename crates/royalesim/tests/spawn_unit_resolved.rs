//! `spawn_unit_resolved`: a troop at an OBSERVED creation point (a live capture's, which the client already resolved)
//! goes down on that point under placement.TAP_SNAP = client16402_tile_centre, where `spawn_unit` snaps the tap to its
//! tile centre and lays a single ground unit on the deploy point beside it. A spell goes down as through `spawn_unit`.
//! Under the old arm, none, the two are the same. The replay harness plays a corpus troop row through it
//! (examples/replay_parity/harness.rs), so the snap does not move a point the capture recorded.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, TapSnap};
use royalesim::Team;

fn native(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Where `card` played by Blue at `at` is laid (its first pending member), through `spawn_unit_resolved` when
/// `observed`, else `spawn_unit`, under `snap`.
fn laid(snap: TapSnap, card: &str, at: Vec2, observed: bool) -> Vec2 {
    let mut cfg = config();
    cfg.calib.placement_tap_snap = snap;
    let mut s = BattleState::new(1, cfg);
    if observed {
        s.spawn_unit_resolved(Team::Blue, card, at, None).expect("the observed point is taken");
    } else {
        s.spawn_unit(Team::Blue, card, at, None).expect("the tap is taken");
    }
    s.pending_spawns().into_iter().find(|(t, c, _)| *t == Team::Blue && s.cards().get(*c).name == card).map(|(_, _, p)| p).expect("nothing was laid")
}

#[test]
fn an_observed_troop_point_is_not_snapped_again() {
    let at = native(6321, 10789);
    assert_eq!(laid(TapSnap::TileCentre, "Knight", at, true), at, "the observed point moved");
    assert_ne!(laid(TapSnap::TileCentre, "Knight", at, false), at, "vacuous: spawn_unit did not snap this tap either");
}

#[test]
fn under_the_old_arm_the_two_lay_one_point() {
    let at = native(6321, 10789);
    assert_eq!(laid(TapSnap::None, "Knight", at, true), laid(TapSnap::None, "Knight", at, false));
}

#[test]
fn a_spell_goes_down_as_through_spawn_unit() {
    let at = native(4321, 16789);
    assert_eq!(laid(TapSnap::TileCentre, "Fireball", at, true), laid(TapSnap::TileCentre, "Fireball", at, false));
}


/// Where `card` played by `team` at `at` is laid under `snap` (its first pending member), through
/// `spawn_unit_resolved` when `observed`, else `spawn_unit`.
fn laid_by(team: Team, snap: TapSnap, card: &str, at: Vec2, observed: bool) -> Vec2 {
    let mut cfg = config();
    cfg.calib.placement_tap_snap = snap;
    let mut s = BattleState::new(1, cfg);
    if observed {
        s.spawn_unit_resolved(team, card, at, None).expect("the observed point is taken");
    } else {
        s.spawn_unit(team, card, at, None).expect("the tap is taken");
    }
    s.pending_spawns().into_iter().find(|(t, c, _)| *t == team && s.cards().get(*c).name == card).map(|(_, _, p)| p).expect("nothing was laid")
}

#[test]
fn an_observed_single_troop_behind_its_own_king_keeps_its_point() {
    // Capture 20260918-112751 t235: side 1's Giant created on (8499, 31000), behind its King. Resolved again, its
    // one-tile box is on the King's, the relocation puts it on the tile (8500, 31500) and the single-unit clamp brings
    // it back to (8500, 31000), a unit right of where the client created it.
    let at = native(8499, 31000);
    for snap in [TapSnap::None, TapSnap::TileCentre] {
        assert_eq!(laid_by(Team::Red, snap, "Giant", at, true), at, "the observed point moved");
    }
    assert_eq!(laid_by(Team::Red, TapSnap::None, "Giant", at, false), native(8500, 31000), "vacuous: the tap on that point is not relocated");
}

#[test]
fn an_observed_group_still_resolves() {
    // A group's point is a tile or a centroid, not a creation point: the relocation still runs on it, as on a tap.
    let at = native(8499, 31000);
    assert_eq!(laid_by(Team::Red, TapSnap::None, "Skeletons", at, true), laid_by(Team::Red, TapSnap::None, "Skeletons", at, false));
}
