//! THE AVOIDANCE SCAN'S WAYPOINT DROP KEEPS THE FROZEN SEGMENT -- calibration pathfinding.AVOIDANCE_DROP_SEGMENT,
//! state.rs `phase_path16402_for` (the drop after move16402.rs `avoidance_scan`).
//!
//! THE LAW (client16402_kept, measured on client 16.402 and client 15.535.29): when a static neighbour's circle holds
//! the next waypoint's centre, the scan drops it and the segment direction stays as it was, the direction toward the
//! node just dropped (11 of 11 drops, 8 moments, on 16.402; 2 of 2 on 15.535.29). Only a reached pop refreezes it, from the
//! end-of-tick position (42314 of 42314 and 21282 of 21282). The reached test measures the new waypoint along that
//! direction, so it decides the tick the unit takes its next node. refrozen, the old arm (the engine before this key),
//! clears the segment on the drop, and the step refreezes it from the start of the tick toward the new waypoint.
//! client16402_kept ships since parity scored its flip; the old arm's test selects refrozen by name.
//!
//! THE SCENE: a Blue Knight put down at (5250, 9250) walks up and to the left toward the left lane; Blue Goblins
//! played at (3750, 11250) six ticks later lay their members in front of it, and they wait out the deploy stagger as
//! static obstacles (movement.WAITING_HEADING = static_obstacle). The Knight's scan drops the waypoints whose centres
//! they hold (on the build that shipped the 19-key flip: (8, 20) and (8, 21) on the first two ticks after the play).
//!
//! WHAT IS PINNED, on every tick the Knight's route loses its next waypoint and nothing else, by the segment direction
//! after the tick ('end': refrozen from the end-of-tick position, a reached pop; 'kept': the one of the tick before;
//! 'start': refrozen from the start-of-tick position):
//!   1. under client16402_kept a drop keeps the segment, and no pop is 'start';
//!   2. under refrozen (the old arm) a drop is 'start', and no pop is 'kept';
//!   3. under both arms, without the Goblins, every pop is 'end' and the two arms walk the same track;
//!   4. the shipped value is client16402_kept.
//!
//! PLANT (regression):
//!   * `avoidance_drop_refreezes` refreezes the segment on a drop under either arm: (1) goes red.
//!     RUSTFLAGS='--cfg clash_plant="avoidance_drop_refreezes"' CARGO_TARGET_DIR=target/plant cargo test --test avoidance_drop_segment

mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE};
use royalesim::move16402::segment_dir;
use royalesim::state::{AvoidanceDropSegment, BattleState, Calib};
use royalesim::Team;

/// The tick index (after the Knight is put down) on which the Goblins are played.
const PLAY_AT: usize = 6;
const TICKS: usize = 60;

#[derive(Clone, PartialEq, Debug)]
struct Row {
    pos: (i32, i32),
    /// half-tile cells, goal first: the last one is the next waypoint
    route: Vec<(i32, i32)>,
    seg: (i32, i32),
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Class {
    End,
    Kept,
    Start,
    Ambiguous,
}

fn native(p: Vec2) -> (i32, i32) {
    (p.x / SUBTILE_PER_MILLITILE, p.y / SUBTILE_PER_MILLITILE)
}

fn centre(cell: (i32, i32)) -> (i32, i32) {
    (cell.0 * 500 + 250, cell.1 * 500 + 250)
}

/// One row per tick, after the tick.
fn scene(arm: AvoidanceDropSegment, goblins: bool) -> Vec<Row> {
    let mut cfg = config();
    cfg.calib.avoidance_drop_segment = arm;
    let mut s = BattleState::new(2, cfg);
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", t(525, 925), None).expect("the Knight");
    let mut rows = Vec::new();
    for k in 0..TICKS {
        if goblins && k == PLAY_AT {
            s.spawn_unit(Team::Blue, "Goblins", t(375, 1125), None).expect("play the Goblins");
        }
        s.tick();
        let e = s.entity(knight).expect("the scene drifted: the Knight is gone");
        let a = s.arena();
        rows.push(Row {
            pos: native(e.pos),
            route: e.route.iter().map(|p| a.subtile_to_half(*p)).collect(),
            seg: (e.seg_dir.x, e.seg_dir.y),
        });
    }
    rows
}

/// One lost waypoint, as `pops` returns it (its doc names the fields).
type Pop = (usize, (i32, i32), Class, (i32, i32), (i32, i32));

/// (row index, the waypoint lost, the class, segment before, segment after) for every tick whose route lost its next
/// waypoint and nothing else.
fn pops(rows: &[Row]) -> Vec<Pop> {
    let mut out = Vec::new();
    for (k, pair) in rows.windows(2).enumerate() {
        let (i, before, after) = (k + 1, &pair[0], &pair[1]);
        if before.route.len() < 2 || after.route[..] != before.route[..before.route.len() - 1] {
            continue;
        }
        let next = centre(*after.route.last().unwrap());
        let start = segment_dir(before.pos.0, before.pos.1, next);
        let end = segment_dir(after.pos.0, after.pos.1, next);
        let class = if after.seg == end {
            Class::End
        } else if after.seg == before.seg && after.seg != start {
            Class::Kept
        } else if after.seg == start && after.seg != before.seg {
            Class::Start
        } else {
            Class::Ambiguous
        };
        out.push((i, *before.route.last().unwrap(), class, before.seg, after.seg));
    }
    out
}

#[test]
fn a_drop_keeps_the_segment() {
    let got = pops(&scene(AvoidanceDropSegment::Client16402Kept, true));
    assert!(got.iter().any(|p| p.2 != Class::End), "new arm: the scene drifted: the Knight's scan dropped nothing: {got:?}");
    let refrozen: Vec<_> = got.iter().filter(|p| p.2 == Class::Start).collect();
    assert!(refrozen.is_empty(), "new arm: a drop refroze the segment from the start of the tick: {refrozen:?}");
    assert!(got.iter().any(|p| p.2 == Class::Kept), "new arm: no drop kept the segment: {got:?}");
}

#[test]
fn the_old_arm_refreezes_from_the_start_of_the_tick() {
    let got = pops(&scene(AvoidanceDropSegment::Refrozen, true));
    let kept: Vec<_> = got.iter().filter(|p| p.2 == Class::Kept).collect();
    assert!(kept.is_empty(), "old arm: a drop kept the segment: {kept:?}");
    assert!(got.iter().any(|p| p.2 == Class::Start), "old arm: the scene drifted: no drop refroze the segment: {got:?}");
}

#[test]
fn a_reached_pop_refreezes_from_the_end_of_the_tick() {
    for arm in [AvoidanceDropSegment::Client16402Kept, AvoidanceDropSegment::Refrozen] {
        let got = pops(&scene(arm, false));
        assert!(got.len() >= 4, "{arm:?}: the scene drifted: the Knight popped {} waypoints", got.len());
        let other: Vec<_> = got.iter().filter(|p| p.2 != Class::End).collect();
        assert!(other.is_empty(), "{arm:?}: with nothing to drop a waypoint, a pop did not refreeze from the end of the tick: {other:?}");
    }
}

#[test]
fn without_a_drop_the_two_arms_walk_the_same_track() {
    let new = scene(AvoidanceDropSegment::Client16402Kept, false);
    let old = scene(AvoidanceDropSegment::Refrozen, false);
    let parted: Vec<usize> = (0..new.len()).filter(|&i| new[i] != old[i]).collect();
    assert!(parted.is_empty(), "the arms part with nothing dropped, on rows {parted:?}");
}

#[test]
fn the_shipped_value_is_the_kept_segment() {
    assert_eq!(Calib::shipped().avoidance_drop_segment, AvoidanceDropSegment::Client16402Kept);
}
