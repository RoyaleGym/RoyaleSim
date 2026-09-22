//! BUILDING PLACEMENT: the tile footprint, the snap and the relocation.
//!
//! The defect these gate is the one the owner reported: the engine judged a
//! building by its TAP POINT, so a Cannon could be put with 0.05 tiles of
//! clearance to the arena wall and stand there. It is a 3x3 building.
//!
//! WHAT IS PINNED, all measured over 114 recorded building placements
//! (calibration placement.*):
//!   1. a Cannon's footprint is 3x3 tiles and a Tesla's is 2x2;
//!   2. a tap whose box leaves the arena, crosses the river or overlaps a tower
//!      or another building does not produce a building there;
//!   3. such a tap is not REFUSED: the building is moved to a nearby legal tile;
//!   4. flush contact is legal, against a wall, a tower and another building;
//!   5. the two seats get the same answer for rotated taps;
//!   6. the box is a placement footprint and never a collision shape.
//!
//! WHAT EACH TEST WOULD CATCH is stated on the test. To see one fail, revert
//! `check_position`'s building branch in state.rs to the old tap-point check.

mod common;

use common::*;
use royalesim::arena::{placement_tiles, Arena};
use royalesim::fixed::{milli, tiles, Vec2};
use royalesim::state::{BattleState, PlacementIllegalTap};
use royalesim::Team;

/// A battle with nothing on the board but the six crown towers.
fn board() -> BattleState {
    BattleState::new(7, config())
}

fn tile_centre(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * tiles(1) + tiles(1) / 2, y * tiles(1) + tiles(1) / 2)
}

fn place(s: &BattleState, team: Team, card: &str, p: Vec2) -> Option<(Vec2, royalesim::arena::Rect)> {
    let idx = s.config().cards.index(card).expect("card in the catalogue");
    s.building_placement(team, idx, p)
}

/// Catches a size rule taken from the collision circle. A Cannon's CollisionRadius
/// is 1.2 tiles, so any reading that rounds 2R down calls it 2x2.
#[test]
fn a_cannon_is_three_tiles_and_a_tesla_is_two() {
    let s = board();
    assert_eq!(placement_tiles(card_stat(&s, "Cannon").collision_radius), 3);
    assert_eq!(placement_tiles(card_stat(&s, "Tesla").collision_radius), 2);
    assert_eq!(placement_tiles(card_stat(&s, "Tombstone").collision_radius), 3);
    assert_eq!(placement_tiles(milli(1400)), 4, "a king tower");

    let (_, b) = place(&s, Team::Blue, "Cannon", tile_centre(6, 8)).expect("open ground is legal");
    assert_eq!(b.max.x - b.min.x, 3 * tiles(1), "the Cannon's box is 3 tiles wide");
    assert_eq!(b.max.y - b.min.y, 3 * tiles(1), "and 3 tiles tall");
}

/// THE REPORTED DEFECT. Each tap is a legal POINT whose 3x3 footprint is not, and
/// under the old tap-point check every one of them built a Cannon where it was
/// tapped. None of them may now do that.
#[test]
fn a_cannon_never_stands_where_its_footprint_does_not_fit() {
    let s = board();
    let t = tiles(1);
    let taps = [
        (Vec2::new(9 * t + t / 2, t / 20), "0.05 tiles from the back wall"),
        (Vec2::new(t / 20, 8 * t + t / 2), "0.05 tiles from the side wall at mid-field"),
        (Vec2::new(17 * t + 19 * t / 20, 8 * t + t / 2), "0.05 tiles from the far side wall"),
        (tile_centre(9, 14), "on the river bank"),
        (tile_centre(3, 6), "on the left princess tower"),
        (tile_centre(9, 3), "on the king tower"),
    ];
    for (tap, what) in taps {
        let (centre, b) = place(&s, Team::Blue, "Cannon", tap).unwrap_or_else(|| panic!("{what}: refused outright"));
        assert_ne!(centre, tap, "{what}: the building stayed on the tap");
        assert!(b.min.x >= 0 && b.min.y >= 0, "{what}: box leaves the arena at {b:?}");
        assert!(b.max.x <= 18 * t && b.max.y <= 32 * t, "{what}: box leaves the arena at {b:?}");
        assert!(b.max.y <= 15 * t, "{what}: box crosses the river at {b:?}");
    }
}

/// Catches a relocation that refuses instead of moving. The recordings show 39 of
/// 64 known taps relocating and none refused, so a `None` here is a regression to
/// the arm the corpus refutes.
#[test]
fn an_unfittable_tap_moves_rather_than_being_refused() {
    let s = board();
    let tap = Vec2::new(9 * tiles(1) + tiles(1) / 2, tiles(1) / 20);
    let (centre, _) = place(&s, Team::Blue, "Cannon", tap).expect("the back wall tap must still build a Cannon");
    assert_eq!(centre.y, tiles(1) + tiles(1) / 2, "it moves exactly far enough to fit, one tile in");
    assert_eq!(centre.x, tap.x - tap.x % tiles(1) + tiles(1) / 2, "and does not drift sideways");
}

/// Catches an overlap test written with `<=`. The recordings hold 93 exactly
/// touching pairs and no overlapping one, so contact must stay legal: this Cannon
/// sits flush against the left princess tower's box.
#[test]
fn flush_against_a_tower_is_legal_and_one_tile_in_is_not() {
    let s = board();
    // The Blue left princess is centred on tile (3, 6), so its 3x3 box spans
    // tiles 2..5. A Cannon centred on tile (6, 6) spans 5..8 and shares an edge.
    let (flush, fb) = place(&s, Team::Blue, "Cannon", tile_centre(6, 6)).expect("flush is legal");
    assert_eq!(flush, tile_centre(6, 6), "a flush tap is kept where it was tapped");
    assert_eq!(fb.min.x, 5 * tiles(1), "its box starts exactly where the tower's ends");
    // One tile closer overlaps by a tile, so it must not stay.
    let (moved, _) = place(&s, Team::Blue, "Cannon", tile_centre(5, 6)).expect("relocated, not refused");
    assert_ne!(moved, tile_centre(5, 6), "a tap overlapping the tower by one tile stayed put");
}

/// Catches a placement decided in engine coordinates. The two seats must answer
/// rotations of each other, or a shared policy learns a seat-dependent offset.
#[test]
fn both_seats_place_a_rotated_tap_the_same_way() {
    let s = board();
    let arena = &s.config().arena;
    for card in ["Cannon", "Tesla", "Tombstone"] {
        for (x, y) in [(9, 0), (0, 8), (9, 14), (6, 8), (5, 6), (3, 6), (17, 1)] {
            let tap = tile_centre(x, y);
            let blue = place(&s, Team::Blue, card, tap);
            let red = place(&s, Team::Red, card, arena.rotate(tap));
            match (blue, red) {
                (Some((bc, _)), Some((rc, _))) => {
                    assert_eq!(arena.rotate(bc), rc, "{card} at tile ({x}, {y})");
                }
                (None, None) => {}
                (b, r) => panic!("{card} at tile ({x}, {y}): one seat placed and the other did not: {b:?} / {r:?}"),
            }
        }
    }
}

/// Catches a snap that keeps the tap, and one that treats every size the same.
/// An even-sized building takes a tile CORNER, which is half a tile from where an
/// odd-sized one of the same tap would sit.
#[test]
fn odd_sizes_take_a_tile_centre_and_even_sizes_a_corner() {
    let s = board();
    let t = tiles(1);
    let tap = Vec2::new(6 * t + 1234, 8 * t + 17_000);
    let (cannon, _) = place(&s, Team::Blue, "Cannon", tap).expect("legal");
    let (tesla, _) = place(&s, Team::Blue, "Tesla", tap).expect("legal");
    assert_eq!(cannon, tile_centre(6, 8), "a 3x3 goes to the tapped tile's centre");
    assert_eq!(tesla, Vec2::new(6 * t, 8 * t), "a 2x2 goes to the tapped tile's corner");
}

/// Catches a relocation arm swapped in silently. Under `refuse` the back-wall tap
/// has no answer at all, which is what the engine did before the footprint was
/// modelled, and what a snapshot from then restores into.
#[test]
fn the_refuse_arm_still_refuses() {
    let mut cfg = config();
    cfg.calib.placement_illegal_tap = PlacementIllegalTap::Refuse;
    let s = BattleState::new(7, cfg);
    let tap = Vec2::new(9 * tiles(1) + tiles(1) / 2, tiles(1) / 20);
    assert!(place(&s, Team::Blue, "Cannon", tap).is_none(), "the refuse arm placed a building anyway");
    assert!(place(&s, Team::Blue, "Cannon", tile_centre(6, 8)).is_some(), "and it must still allow open ground");
}

/// Catches a footprint used as a collision shape. The recordings show troop
/// centres inside building boxes constantly, so a placement box must not move,
/// block or refuse a troop.
#[test]
fn a_troop_may_stand_inside_a_building_box() {
    let mut s = board();
    let centre = tile_centre(6, 8);
    s.spawn_unit(Team::Blue, "Cannon", centre, None).expect("the Cannon goes down");
    s.tick();
    let b = Arena::placement_box(centre, 3);
    // A Knight placed inside the box is accepted, and lands inside it.
    s.spawn_unit(Team::Blue, "Knight", Vec2::new(centre.x + tiles(1), centre.y), None)
        .expect("a troop inside a building's box is not refused");
    s.tick();
    let knights = find_live(&s, Team::Blue, "Knight");
    assert_eq!(knights.len(), 1, "the Knight is on the board");
    let p = knights[0].pos;
    assert!(
        p.x > b.min.x && p.x < b.max.x && p.y > b.min.y && p.y < b.max.y,
        "the Knight at {p:?} was pushed out of the placement box {b:?}, which is not a collision shape"
    );
}
