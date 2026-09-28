//! placement.TOWER_TAP_PUSH, read off `resolve_point`: where a troop tap on an own crown tower's box goes under
//! placement.TROOP_TOWER_TAPS = client16402_half_open_relocate.
//!
//! THE LAW, measured on client 15.535.29 (a Knight on every tile centre of all four own princess boxes, both seats, 36
//! taps, with 4 taps one tile outside and 2 on a downed princess's box: 42 of 42): push the unit out along the axis
//! where the RAW tap is farther from the tower's centre; on an exact tie take the first OUTWARD direction in the fixed
//! ARENA order -y, -x, +y, +x; land on the first tile centre beyond the box, on the tapped tile's row or column. The
//! 16.402 corpus's two ties (side 1 at (4500, 24500)) went -y. Today's ring search (ring_nearest) breaks ties
//! column-major in the placer's frame and lands 12 of the 36 box taps elsewhere. WHAT IS PINNED:
//!   1. client16402_axis_push: every box tap lands on the client's tile, and a tap one tile outside stays;
//!   2. both values: a tap on a downed princess's box stays (and, with the princess standing, it moves);
//!   3. client16402_axis_push: taps on one tile, either side of its diagonal, go the ways the client sent them, under
//!      both placement.TAP_SNAP values (the rule reads the tap before the snap);
//!   4. client16402_axis_push: the king's three ties, in arena coordinates;
//!   5. ring_nearest lands exactly the 12 box taps the client puts elsewhere on another tile (the table separates the
//!      values, so (1) can fail);
//!   6. the shipped value is client16402_axis_push (the 2026-09-28 placement batch, with placement.TAP_SNAP).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test tower_tap_push`):
//!   * `tower_tap_push_x_first` -- a tie takes x first (-x, -y, +x, +y): (1) and (4) go red.
//!   * `tower_tap_push_snapped_tile` -- the push reads the snapped tile centre, not the raw tap: (3) goes red.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, TapSnap, TowerTapPush, TroopTowerTaps};
use royalesim::Team;

const NEW: TowerTapPush = TowerTapPush::Client16402AxisPush;
const OLD: TowerTapPush = TowerTapPush::RingNearest;

/// A point, native units.
type P = (i32, i32);
type Row = (u8, P, P);

/// (side, tap, where the Knight stood on client 15.535.29, as its tile centre), native units, arena frame. The 36 box
/// taps, then the 4 taps one tile outside (they stay).
const BOX_TAPS_15535: [Row; 40] = [
    (0, (2500, 5500), (2500, 4500)),
    (0, (3500, 5500), (3500, 4500)),
    (0, (4500, 5500), (4500, 4500)),
    (0, (2500, 6500), (1500, 6500)),
    (0, (3500, 6500), (3500, 4500)),
    (0, (4500, 6500), (5500, 6500)),
    (0, (2500, 7500), (1500, 7500)),
    (0, (3500, 7500), (3500, 8500)),
    (0, (4500, 7500), (4500, 8500)),
    (0, (13500, 5500), (13500, 4500)),
    (0, (14500, 5500), (14500, 4500)),
    (0, (15500, 5500), (15500, 4500)),
    (0, (13500, 6500), (12500, 6500)),
    (0, (14500, 6500), (14500, 4500)),
    (0, (15500, 6500), (16500, 6500)),
    (0, (13500, 7500), (12500, 7500)),
    (0, (14500, 7500), (14500, 8500)),
    (0, (15500, 7500), (15500, 8500)),
    (1, (2500, 24500), (2500, 23500)),
    (1, (3500, 24500), (3500, 23500)),
    (1, (4500, 24500), (4500, 23500)),
    (1, (2500, 25500), (1500, 25500)),
    (1, (3500, 25500), (3500, 23500)),
    (1, (4500, 25500), (5500, 25500)),
    (1, (2500, 26500), (1500, 26500)),
    (1, (3500, 26500), (3500, 27500)),
    (1, (4500, 26500), (4500, 27500)),
    (1, (13500, 24500), (13500, 23500)),
    (1, (14500, 24500), (14500, 23500)),
    (1, (15500, 24500), (15500, 23500)),
    (1, (13500, 25500), (12500, 25500)),
    (1, (14500, 25500), (14500, 23500)),
    (1, (15500, 25500), (16500, 25500)),
    (1, (13500, 26500), (12500, 26500)),
    (1, (14500, 26500), (14500, 27500)),
    (1, (15500, 26500), (15500, 27500)),
    (0, (5500, 6500), (5500, 6500)),
    (0, (12500, 6500), (12500, 6500)),
    (1, (5500, 25500), (5500, 25500)),
    (1, (12500, 25500), (12500, 25500)),
];

/// The 12 box taps ring_nearest lands on another tile: every centre, and the corners and edges its order gets wrong.
const RING_MISSES: [(u8, P); 12] = [
    (0, (2500, 5500)),
    (0, (3500, 6500)),
    (0, (13500, 5500)),
    (0, (14500, 6500)),
    (1, (4500, 24500)),
    (1, (3500, 25500)),
    (1, (2500, 26500)),
    (1, (4500, 26500)),
    (1, (15500, 24500)),
    (1, (14500, 25500)),
    (1, (13500, 26500)),
    (1, (15500, 26500)),
];

/// Side 1's arena-right princess downed: its centre and its (-1, +1) corner were accepted where tapped.
const DOWNED_15535: [P; 2] = [(14500, 25500), (13500, 26500)];

/// Side 0 taps on one tile, either side of its diagonal, and the tile centre each went to.
const PAIRS_15535: [(P, P); 6] = [
    ((10501, 1001), (10500, 500)),
    ((10999, 1499), (11500, 1500)),
    ((2600, 5400), (2500, 4500)),
    ((2400, 5600), (1500, 5500)),
    ((4400, 7600), (4500, 8500)),
    ((4600, 7400), (5500, 7500)),
];

/// The king's three ties, arena frame, before formation.GROUND_Y_CLAMP (side 1's +y then stands on 31000).
const KING_TIES_15535: [Row; 3] = [(0, (10500, 1500), (10500, 500)), (1, (7500, 30500), (6500, 30500)), (1, (10500, 30500), (10500, 31500))];

fn native(p: P) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn team(side: u8) -> Team {
    if side == 0 {
        Team::Blue
    } else {
        Team::Red
    }
}

fn battle(push: TowerTapPush, snap: TapSnap) -> BattleState {
    let mut cfg = config();
    cfg.calib.placement_troop_tower_taps = TroopTowerTaps::HalfOpenRelocate;
    cfg.calib.placement_tower_tap_push = push;
    cfg.calib.placement_tap_snap = snap;
    BattleState::new(1, cfg)
}

/// Where a Knight tapped at `tap` is put down, native units.
fn landing(s: &BattleState, side: u8, tap: P) -> P {
    let idx = s.cards().index("Knight").expect("data: Knight loads");
    let p = s.resolve_point(team(side), idx, native(tap));
    (p.x / K, p.y / K)
}

fn misses(push: TowerTapPush) -> Vec<(u8, P, P, P)> {
    let s = battle(push, TapSnap::None);
    BOX_TAPS_15535.iter().map(|&(side, tap, want)| (side, tap, want, landing(&s, side, tap))).filter(|r| r.2 != r.3).collect()
}

#[test]
fn every_princess_box_tap_lands_as_on_client_15535() {
    let off = misses(NEW);
    assert!(off.is_empty(), "{} of {} taps land off the client's tile (side, tap, client, engine): {off:?}", off.len(), BOX_TAPS_15535.len());
}

#[test]
fn a_tap_on_a_downed_princess_box_stays() {
    for push in [NEW, OLD] {
        let mut s = battle(push, TapSnap::None);
        for tap in DOWNED_15535 {
            assert_ne!(landing(&s, 1, tap), tap, "{push:?}: the scene drifted: with the princess standing, {tap:?} is not moved");
        }
        let k = s
            .tower_ids(Team::Red)
            .into_iter()
            .position(|t| t.and_then(|t| s.entity(t)).is_some_and(|v| v.pos == native((14500, 25500))))
            .expect("side 1's arena-right princess stands at (14500, 25500)");
        s.scenario_set_tower_hp(Team::Red, k, 0).expect("the princess goes down");
        for tap in DOWNED_15535 {
            assert_eq!(landing(&s, 1, tap), tap, "{push:?}: {tap:?} on a downed princess's box was moved");
        }
    }
}

#[test]
fn the_raw_tap_not_its_tile_decides() {
    for snap in [TapSnap::None, TapSnap::TileCentre] {
        let s = battle(NEW, snap);
        let off: Vec<_> = PAIRS_15535.iter().map(|&(tap, want)| (tap, want, landing(&s, 0, tap))).filter(|r| r.1 != r.2).collect();
        assert!(off.is_empty(), "TAP_SNAP {snap:?}: (tap, client, engine) {off:?}");
    }
}

#[test]
fn the_kings_ties_go_the_arena_order() {
    let s = battle(NEW, TapSnap::None);
    let off: Vec<_> = KING_TIES_15535.iter().map(|&(side, tap, want)| (side, tap, want, landing(&s, side, tap))).filter(|r| r.2 != r.3).collect();
    assert!(off.is_empty(), "(side, tap, client, engine) {off:?}");
}

#[test]
fn the_old_value_is_the_ring_search() {
    let mut got: Vec<(u8, P)> = misses(OLD).into_iter().map(|r| (r.0, r.1)).collect();
    let mut want = RING_MISSES.to_vec();
    got.sort();
    want.sort();
    assert_eq!(got, want, "ring_nearest no longer misses exactly the 12 box taps");
}

#[test]
fn the_shipped_value_is_the_axis_push() {
    assert_eq!(Calib::shipped().placement_tower_tap_push, NEW);
}
