//! A SAMEPATH REPLAN RESTARTS THE FROZEN SEGMENT -- calibration pathfinding.SAMEPATH_SEGMENT, state.rs
//! `phase_path16402_for` (the SAMEPATH branch of the occluder-change replan, path16402.rs
//! `path_touches_changed_occlusion`).
//!
//! THE LAW (client_restarted, measured on client 15.535.29, the recorded corpus): when one of the walker's own side's
//! buildings appears or leaves, the replan it forces returns a list the change did not touch, and the unit keeps its old
//! list, the segment restarts: it is refrozen from the start-of-tick position toward the kept next waypoint, and the
//! reached test measures the waypoint along the new direction. 498 of 501 pops with such a change in their window take
//! the client's tick with the restart, 478 without it. kept, the old arm (the engine before this key), leaves the
//! segment frozen where the node was taken.
//!
//! THE SCENE: Blue Barbarians put down at (3500, 9000) walk up the left lane, pushing each other off their segment
//! lines. Blue plays a Cannon at (14500, 10000), far from their route, while they walk.
//!
//! WHAT IS PINNED, per Barbarian, on every tick its route stays the same (a "hold"):
//!   1. under client_restarted, some holds change the segment, all of them on one tick at or after the play, each to the
//!      direction from the start-of-tick position toward the next waypoint;
//!   2. under kept (the old arm) no hold changes the segment;
//!   3. without the Cannon the two arms walk the same track;
//!   4. the shipped value is client_restarted, since the round-10 flip (scored +5,717 within 250, 9 reports up).
//!
//! PLANT (regression):
//!   * `samepath_keeps_segment` keeps the segment under either arm: (1) goes red.
//!     RUSTFLAGS='--cfg clash_plant="samepath_keeps_segment"' CARGO_TARGET_DIR=target/plant cargo test --test samepath_segment

mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE};
use royalesim::move16402::segment_dir;
use royalesim::entity::EntityKind;
use royalesim::state::{BattleState, Calib, SamepathSegment};
use royalesim::Team;

/// The tick index on which the Cannon is played: the Barbarians have finished deploying and walk.
const PLAY_AT: usize = 40;
const TICKS: usize = 70;

#[derive(Clone, PartialEq, Debug)]
struct Row {
    pos: (i32, i32),
    /// half-tile cells, goal first: the last one is the next waypoint
    route: Vec<(i32, i32)>,
    seg: (i32, i32),
}

fn native(p: Vec2) -> (i32, i32) {
    (p.x / SUBTILE_PER_MILLITILE, p.y / SUBTILE_PER_MILLITILE)
}

fn centre(cell: (i32, i32)) -> (i32, i32) {
    (cell.0 * 500 + 250, cell.1 * 500 + 250)
}

/// Per Barbarian, one row per tick, after the tick.
fn scene(arm: SamepathSegment, cannon: bool) -> Vec<Vec<Row>> {
    let mut cfg = config();
    cfg.calib.samepath_segment = arm;
    let mut s = BattleState::new(2, cfg);
    s.spawn_unit(Team::Blue, "Barbarians", t(350, 900), None).expect("play the Barbarians");
    let mut ids = Vec::new();
    let mut rows: Vec<Vec<Row>> = Vec::new();
    for k in 0..TICKS {
        if cannon && k == PLAY_AT {
            s.spawn_unit(Team::Blue, "Cannon", t(1450, 1000), None).expect("play the Cannon");
        }
        s.tick();
        if ids.is_empty() {
            ids = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::Troop).map(|e| e.id).collect();
            assert!(ids.len() >= 2, "the scene drifted: {} Barbarians", ids.len());
            rows = vec![Vec::new(); ids.len()];
        }
        let a = s.arena();
        for (n, id) in ids.iter().enumerate() {
            let e = s.entity(*id).expect("the scene drifted: a Barbarian is gone");
            rows[n].push(Row { pos: native(e.pos), route: e.route.iter().map(|p| a.subtile_to_half(*p)).collect(), seg: (e.seg_dir.x, e.seg_dir.y) });
        }
    }
    rows
}

/// (Barbarian, row index, segment before, segment after, the start-of-tick refreeze) for every hold that changed the
/// segment: the route the same after the tick as before it, the segment not.
/// One restart `restarts` reads (Barbarian, row index, segment before, segment after, the start-of-tick refreeze).
type Restart = (usize, usize, (i32, i32), (i32, i32), (i32, i32));

fn restarts(rows: &[Vec<Row>]) -> Vec<Restart> {
    let mut out = Vec::new();
    for (n, one) in rows.iter().enumerate() {
        for (k, pair) in one.windows(2).enumerate() {
            let (before, after) = (&pair[0], &pair[1]);
            if before.route.is_empty() || after.route != before.route || after.seg == before.seg {
                continue;
            }
            let start = segment_dir(before.pos.0, before.pos.1, centre(*after.route.last().unwrap()));
            out.push((n, k + 1, before.seg, after.seg, start));
        }
    }
    out
}

#[test]
fn a_samepath_replan_restarts_the_segment_from_the_start_of_the_tick() {
    let got = restarts(&scene(SamepathSegment::ClientRestarted, true));
    assert!(!got.is_empty(), "new arm: the scene drifted: no Barbarian's segment restarted when the Cannon came");
    let off: Vec<_> = got.iter().filter(|r| r.3 != r.4).collect();
    assert!(off.is_empty(), "new arm: a restart did not refreeze from the start-of-tick position: {off:?}");
    let first = got[0].1;
    assert!(first >= PLAY_AT, "new arm: a segment restarted before the Cannon was played: {got:?}");
    assert!(got.iter().all(|r| r.1 == first), "new arm: the restarts are not on one tick: {got:?}");
}

#[test]
fn the_old_arm_keeps_the_segment() {
    let got = restarts(&scene(SamepathSegment::Kept, true));
    assert!(got.is_empty(), "old arm: a hold changed the segment: {got:?}");
}

#[test]
fn without_a_building_change_the_two_arms_walk_the_same_track() {
    let new = scene(SamepathSegment::ClientRestarted, false);
    let old = scene(SamepathSegment::Kept, false);
    assert_eq!(new, old, "the arms part with no building played");
}

#[test]
fn the_shipped_value_is_client_restarted_since_the_round_10_flip() {
    assert_eq!(Calib::shipped().samepath_segment, SamepathSegment::ClientRestarted);
}
