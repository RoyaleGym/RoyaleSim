//! A UNIT A HOOK DRAGGED PLANS A FRESH ROUTE FROM WHERE THE DRAG LEFT IT -- calibration combat.HOOK_DRAG_ROUTE,
//! state.rs `step_hook_drags` (the drag's end on its margin).
//!
//! THE LAW (client16402_dropped): the Fisherman's hook (combat.SPECIAL_HOOK) drags its victim straight at him, 510 a
//! tick, until the next step would bring the centres within DragMargin plus both radii. When the drag ends there the
//! victim drops its route, and the next path request plans a fresh one from where it stands. Measured on the 16.402
//! corpus: both drags of the red Giant of 20260920-081819 (both seats) hold the route unchanged through the stop, show
//! no route on the frame after it and a fresh route from the end point on the frame after that. The old arm (`kept`,
//! the engine before this key) keeps the old route, whose next waypoint the drag left behind the victim, and walks back
//! to it. client16402_dropped ships since parity scored its flip; the tests name both arms through their config.
//!
//! THE SCENE: a Blue Fisherman at (3500, 12000) and a Red Giant at (3500, 22000) on the left lane. The Giant walks
//! south toward the Blue princess tower; the Fisherman stops, hooks it and drags it about 4,600 south across the
//! river. A Giant targets buildings only, so it walks on after the drag instead of attacking the Fisherman.
//!
//! WHAT IS PINNED:
//!   1. both arms: the scene holds -- one run of drag steps, and the route the Giant held when the hook landed has its
//!      next node more than 1,000 north of the drag's end point;
//!   2. client16402_dropped: the Giant's first step after the drag goes south, and its next node is not north of it;
//!   3. kept (the old arm): the Giant's first step after the drag goes north, back toward the held node;
//!   4. the null: a Red Knight in the Giant's place attacks the Fisherman after the drag, which drops its route on that
//!      transition under either arm, so it walks the same track under both arms;
//!   5. the shipped value is client16402_dropped.
//!
//! PLANTS (regression):
//!   * `hook_drag_route_kept` keeps the pre-drag route under the new arm: (2) goes red.
//!     RUSTFLAGS='--cfg clash_plant="hook_drag_route_kept"' CARGO_TARGET_DIR=target/plant cargo test --test hook_drag_route

mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE};
use royalesim::state::{BattleConfig, BattleState, Calib, HookDragRoute};
use royalesim::Team;

const NEW: HookDragRoute = HookDragRoute::Client16402Dropped;
const OLD: HookDragRoute = HookDragRoute::Kept;
/// A drag step is 510 native a tick; a walk is far below this.
const DRAG_STEP_FLOOR: i32 = 500;
const TICKS: usize = 140;

fn with(arm: HookDragRoute) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.hook_drag_route = arm;
    cfg
}

fn native(p: Vec2) -> (i32, i32) {
    (p.x / SUBTILE_PER_MILLITILE, p.y / SUBTILE_PER_MILLITILE)
}

/// Per tick after the tick: the victim's position and its route's next node (native), None once it is gone.
type Row = Option<((i32, i32), Option<(i32, i32)>)>;

fn scene(arm: HookDragRoute, victim: &str) -> Vec<Row> {
    let mut s = BattleState::new(0, with(arm));
    s.scenario_spawn_now(Team::Blue, "Fisherman", t(350, 1200), None).expect("the Fisherman");
    let v = s.scenario_spawn_now(Team::Red, victim, t(350, 2200), None).expect("the victim");
    let mut rows = Vec::new();
    for _ in 0..TICKS {
        s.tick();
        rows.push(s.entity(v).map(|e| (native(e.pos), e.route.last().map(|p| native(*p)))));
    }
    rows
}

/// (first, last) row of the drag: the rows whose step is a drag step, one unbroken run.
fn drag(rows: &[Row], what: &str) -> (usize, usize) {
    let steps: Vec<usize> = (1..rows.len())
        .filter(|&i| match (rows[i], rows[i - 1]) {
            (Some((a, _)), Some((b, _))) => (a.1 - b.1).abs() >= DRAG_STEP_FLOOR,
            _ => false,
        })
        .collect();
    assert!(!steps.is_empty(), "{what}: the scene drifted: the hook never dragged the victim");
    assert_eq!(steps.last().unwrap() - steps[0] + 1, steps.len(), "{what}: the scene drifted: the drag is not one run of ticks");
    (steps[0], *steps.last().unwrap())
}

/// The first row after the drag on which the victim moves.
fn first_walk_after(rows: &[Row], last: usize, what: &str) -> usize {
    (last + 1..(last + 15).min(rows.len()))
        .find(|&i| matches!((rows[i], rows[i - 1]), (Some((a, _)), Some((b, _))) if a != b))
        .unwrap_or_else(|| panic!("{what}: the scene drifted: the victim never walked within 14 ticks of the drag's end"))
}

fn pos(rows: &[Row], i: usize) -> (i32, i32) {
    rows[i].expect("the victim is gone").0
}

#[test]
fn the_scene_leaves_the_held_route_behind_the_giant() {
    for arm in [NEW, OLD] {
        let rows = scene(arm, "Giant");
        let (first, last) = drag(&rows, "Giant");
        let held = rows[first - 1].expect("the Giant is gone").1.expect("the scene drifted: no route when the hook landed");
        let end = pos(&rows, last);
        assert!(held.1 > end.1 + 1000, "{arm:?}: the scene drifted: the held node {held:?} is not well north of the drag's end {end:?}");
    }
}

/// Plant: hook_drag_route_kept.
#[test]
fn the_dragged_giant_walks_on_from_where_the_drag_left_it() {
    let rows = scene(NEW, "Giant");
    let (first, last) = drag(&rows, "new arm");
    let k = first_walk_after(&rows, last, "new arm");
    let (p0, p1) = (pos(&rows, k - 1), pos(&rows, k));
    let held = rows[first - 1].unwrap().1;
    assert!(p1.1 < p0.1, "new arm: after the drag ended at {p0:?} the Giant stepped to {p1:?}, back toward the node it held ({held:?})");
    let next = rows[k].unwrap().1.expect("new arm: the Giant walks with no route");
    assert!(next.1 <= p0.1, "new arm: the Giant's next node {next:?} is north of it ({p0:?})");
}

#[test]
fn the_old_arm_walks_back_to_the_held_route() {
    let rows = scene(OLD, "Giant");
    let (_, last) = drag(&rows, "old arm");
    let k = first_walk_after(&rows, last, "old arm");
    let (p0, p1) = (pos(&rows, k - 1), pos(&rows, k));
    assert!(p1.1 > p0.1, "old arm: after the drag the Giant stepped from {p0:?} to {p1:?}, not back north");
}

/// The positions are compared, not the routes: on the stop tick itself the new arm has already dropped the route that
/// the old arm drops on the next tick, when the Knight goes into its attack.
#[test]
fn a_knight_that_attacks_after_the_drag_runs_the_same_under_both_arms() {
    let (new, old) = (scene(NEW, "Knight"), scene(OLD, "Knight"));
    drag(&new, "Knight, new arm");
    let track = |rows: &[Row]| rows.iter().map(|r| r.map(|(p, _)| p)).collect::<Vec<_>>();
    assert_eq!(track(&new), track(&old), "a Knight dragged and then attacking walked differently under the two arms");
}

#[test]
fn the_shipped_value_is_dropped() {
    assert_eq!(Calib::shipped().hook_drag_route, NEW);
}
