//! `spawn_unit_resolved`: a troop at an OBSERVED creation point (a live capture's, which the client already resolved)
//! goes down on that point under placement.TAP_SNAP = client16402_tile_centre, where `spawn_unit` snaps the tap to its
//! tile centre and lays a single ground unit on the deploy point beside it. A spell goes down as through `spawn_unit`.
//! Under the old arm, none, the two are the same. The replay harness plays a corpus troop row through it
//! (examples/replay_parity/harness.rs), so the snap does not move a point the capture recorded.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, TapSnap, TroopTowerTaps};
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
    // Not vacuous under the placement arms of the capture's reading, by name (placement.TOWER_TAP_PUSH = ring_nearest,
    // SNAP_EVEN_CORNER = placer_frame, whose frame decides the tile of a tap on the tile edge y 31000): the tap on that
    // point is moved there. (Under the arms shipped since the 2026-09-28 placement batch the tap on that point lands
    // where the client created the Giant, so the two paths agree on it.)
    let mut cfg = config();
    cfg.calib.placement_tap_snap = TapSnap::None;
    cfg.calib.placement_tower_tap_push = royalesim::state::TowerTapPush::RingNearest;
    cfg.calib.placement_snap_even = royalesim::state::PlacementSnapEven::PlacerFrame;
    let mut s = BattleState::new(1, cfg);
    s.spawn_unit(Team::Red, "Giant", at, None).expect("the tap is taken");
    let tapped = s.pending_spawns().into_iter().find(|(t, c, _)| *t == Team::Red && s.cards().get(*c).name == "Giant").map(|(_, _, p)| p);
    assert_eq!(tapped, Some(native(8500, 31000)), "vacuous: the tap on that point is not relocated");
}

#[test]
fn an_observed_group_still_resolves() {
    // A group's point is a tile or a centroid, not a creation point: the relocation still runs on it, as on a tap.
    let at = native(8499, 31000);
    assert_eq!(laid_by(Team::Red, TapSnap::None, "Skeletons", at, true), laid_by(Team::Red, TapSnap::None, "Skeletons", at, false));
}

/// A LONE BUILDING KEEPS ITS OWN POINT under placement.TAP_SNAP = tile_centre and formation.GROUND_DEPLOY_POINT =
/// client16402_one_unit: the one-native offset (x - 1 on the left half, y - 1 for side 1) is a ground troop's. The
/// 16.402 corpus's Tombstones and Goblin Huts stand on their tile centres; 20260920-071056-A's Tombstone on (3500,
/// 10500) took (3499, 10500) under the offset and its Skeletons walked out on the other side of the column. A Knight on
/// the same tiles takes the offset, so the check is not vacuous. Plant: tap_snap_offsets_buildings.
#[test]
fn a_lone_building_keeps_its_own_point_under_the_tap_snap() {
    use royalesim::state::GroundDeployPoint;
    let laid_as = |team: Team, card: &str, at: Vec2| {
        let mut cfg = config();
        cfg.calib.placement_tap_snap = TapSnap::TileCentre;
        cfg.calib.formation_ground_deploy_point = GroundDeployPoint::Client16402OneUnit;
        let mut s = BattleState::new(1, cfg);
        s.spawn_unit(team, card, at, None).expect("the tap is taken");
        s.pending_spawns().into_iter().find(|(t, c, _)| *t == team && s.cards().get(*c).name == card).map(|(_, _, p)| p).expect("nothing was laid")
    };
    let (left, red) = (native(3500, 10500), native(14500, 19500));
    assert_eq!(laid_as(Team::Blue, "Tombstone", left), left, "a left-half building took the ground x offset");
    assert_eq!(laid_as(Team::Red, "Tombstone", red), red, "a side-1 building took the ground y offset");
    assert_eq!(laid_as(Team::Blue, "Knight", left), native(3499, 10500), "vacuous: the troop on that tile took no offset");
    assert_eq!(laid_as(Team::Red, "Knight", red), native(14500, 19499), "vacuous: the side-1 troop took no offset");
}

/// Where `card` played by `team` at `at` is laid under `taps` (the shipped TAP_SNAP), through `spawn_unit_resolved`
/// when `observed`, else `spawn_unit`.
fn laid_under_taps(team: Team, taps: TroopTowerTaps, card: &str, at: Vec2, observed: bool) -> Vec2 {
    let mut cfg = config();
    cfg.calib.placement_troop_tower_taps = taps;
    let mut s = BattleState::new(1, cfg);
    if observed {
        s.spawn_unit_resolved(team, card, at, None).expect("the observed point is taken");
    } else {
        s.spawn_unit(team, card, at, None).expect("the tap is taken");
    }
    s.pending_spawns().into_iter().find(|(t, c, _)| *t == team && s.cards().get(*c).name == card).map(|(_, _, p)| p).expect("nothing was laid")
}

#[test]
fn an_observed_single_troop_behind_its_column_back_bound_is_not_clamped() {
    // Capture 20260920-005517-A t2084: side 1's Bomber created on (9474, 31536), own (8526, 464), behind its column's
    // back bound (own 1000), where the client's tower-tap relocation put it. The single-unit clamp of
    // placement.TROOP_TOWER_TAPS = client16402_half_open_relocate raised it to own 1000, absolute (9474, 31000).
    let at = native(9474, 31536);
    for card in ["Bomber", "Knight"] {
        assert_eq!(laid_under_taps(Team::Red, TroopTowerTaps::HalfOpenRelocate, card, at, true), at, "{card}: the observed point moved");
        assert_eq!(laid_under_taps(Team::Red, TroopTowerTaps::ClosedBlock, card, at, true), at, "{card}: closed_block moved the observed point");
    }
    // Not vacuous: the same point played as a tap under the same arm is resolved off it.
    assert_ne!(laid_under_taps(Team::Red, TroopTowerTaps::HalfOpenRelocate, "Bomber", at, false), at, "vacuous: the tap on that point is not moved");
}

/// AN OBSERVED SINGLE TROOP'S POINT IS NOT RESOLVED ON THE BOARD EITHER (`resolve_observed_point`, which the replay
/// harness's `resolve_on_board` takes for a corpus row): side 1's Bomber of the 16.402 capture 20260920-005517-A,
/// created on (9500, 31000) behind its King, keeps that point, where the same point played as a tap is relocated off
/// the King's box. A group's observed point still resolves as before. Plant: observed_single_resolved_on_board.
#[test]
fn an_observed_single_troops_point_is_not_resolved_on_the_board() {
    let s = BattleState::new(1, config());
    let at = native(9500, 31000);
    for card in ["Bomber", "Knight", "Musketeer"] {
        let idx = s.cards().index(card).unwrap_or_else(|| panic!("{card} does not load"));
        assert_eq!(s.resolve_observed_point(Team::Red, idx, at), at, "{card}: the observed point was resolved again");
        // Not vacuous: the same point as a tap is moved.
        assert_ne!(s.resolve_point(Team::Red, idx, at), at, "vacuous: {card}'s tap on that point is not relocated");
    }
    // A group's observed point is a centroid, not a creation point, and still resolves as a tap does (bar the snap):
    // on side 1's own princess box, (14500, 25500), a Skeletons centroid is moved off the box where a single Bomber's
    // creation point stays. (At (9500, 31000) the shipped axis push leaves a group where it is, so that point cannot
    // show it.)
    let skel = s.cards().index("Skeletons").expect("Skeletons load");
    let bomber = s.cards().index("Bomber").expect("Bomber loads");
    let on_box = native(14500, 25500);
    assert_ne!(s.resolve_observed_point(Team::Red, skel, on_box), on_box, "a group's observed point went unresolved");
    assert_eq!(s.resolve_observed_point(Team::Red, bomber, on_box), on_box, "a single's observed point was resolved there");
}
