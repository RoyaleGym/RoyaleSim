//! targeting.FALLEN_LANE_TOWER_PICK = client16635_last_lane: WHICH lane cells move a troop's walked lane
//! (arena.rs `circle_lane_bits`, state.rs `note_lanes`). Measured on client 16.635.1's own battle engine, run headless
//! on constructed scenes: the lane strips between the king areas and the bridges count; the lane marking that wraps
//! around each king tower does not (a Barbarian created on the right that fought among the left-lane cells beside its
//! own king walked back to its right lane's tower, in a long battle and in 18 constructed scenes; counting those cells
//! sends it to the king). The parity of the whole rule against the client's own tracks is
//! tests/test_walk_lane16635.py.
//!
//! PLANTS: `king_area_lane_counts` -- the king-area rows count like the strips:
//! the_king_area_rows_are_the_rows_around_each_king and the_lane_cells_around_a_king_count_for_nothing.

use royalesim::arena::Arena;
use royalesim::fixed::Vec2;

/// The king-area rows are those with a lane-marked cell in a king block's columns: 2..=9 and 54..=61 on the shipped map.
#[test]
fn the_king_area_rows_are_the_rows_around_each_king() {
    let a = Arena::shipped();
    let rows: Vec<i32> = (0..a.rows).filter(|&j| a.king_area_row(j)).collect();
    let want: Vec<i32> = (2..=9).chain(54..=61).collect();
    assert_eq!(rows, want);
}

fn at(a: &Arena, x_milli: i32, y_milli: i32, r_milli: i32) -> u8 {
    let s = royalesim::fixed::SUBTILE_PER_MILLITILE;
    a.circle_lane_bits(Vec2::new(x_milli * s, y_milli * s), r_milli * s)
}

/// A body counts the strip it overlaps, by its radius, not its centre's cell: in a long battle on client 16.635.1,
/// Skeletons whose centres stood at x 13462, one column short of the right strip (x 13500..15500), took its lane.
#[test]
fn a_body_overlapping_the_strip_takes_its_lane() {
    let a = Arena::shipped();
    assert_eq!(at(&a, 13462, 21234, 500), a.bit_lane_right);
    assert_eq!(at(&a, 13462, 21234, 1), 0, "the centre alone is off the strip");
    assert_eq!(at(&a, 12230, 19271, 500), 0, "a Knight that stopped at x 12230 is off the strip (it kept its lane)");
    assert_eq!(at(&a, 3500, 16000, 500), a.bit_lane_left, "the left bridge");
    assert_eq!(at(&a, 14500, 16000, 500), a.bit_lane_right, "the right bridge");
}

/// The lane cells around a king tower count for nothing (rows 54..=61 here): a Knight at (8321, 26789), radius 500,
/// overlaps the left-lane cells beside Red's king (row 54) and moves no lane.
#[test]
fn the_lane_cells_around_a_king_count_for_nothing() {
    let a = Arena::shipped();
    assert_eq!(at(&a, 8321, 26789, 500), 0);
    assert_eq!(at(&a, 7735, 27054, 500), 0, "the long battle's Barbarian beside its own king");
    // (radius 1: a cell counts when its nearest point is STRICTLY closer than the radius)
    // a row-53 point off every strip, and a row-47 point on the wide left strip beside the princess tower
    assert_eq!(at(&a, 9000, 26600, 1), 0);
    assert_eq!(at(&a, 3500, 23800, 1), a.bit_lane_left);
}

/// FNV-1a over `Arena`'s Debug text, as state.rs `fingerprint_debug` takes a saved battle's arena fingerprint.
fn arena_debug_fnv(a: &Arena) -> u64 {
    format!("{a:?}").bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// The king-area rows are read off the cells, not kept in `Arena`: its Debug text, a saved battle's arena
/// fingerprint, is 0.1.57's, so a battle saved by 0.1.57 still loads (a field there refused every such blob).
#[test]
fn the_arena_fingerprint_is_0157s() {
    assert_eq!(arena_debug_fnv(&Arena::shipped()), ARENA_0157);
}
/// `Arena::shipped()`'s fingerprint on royalesim 0.1.57.
const ARENA_0157: u64 = 15_660_221_742_453_292_092;
