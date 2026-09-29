//! A HELD UNIT IS MASKED OUT OF THE AVOIDANCE SCAN -- calibration collision.HELD_UNIT_AVOIDANCE, state.rs
//! `phase_path16402_for` (the avoidance scan's guard, `held_masked`).
//!
//! THE LAW (masked): a unit held by a freeze or a stun, in the update it takes at speed 0 under
//! collision.HELD_UNIT_CONTACT = client16402_speed_zero_update, runs no avoidance scan, as an attacking unit runs none:
//! its offset neither starts nor is refreshed, and it still decays 10 a tick. Read off the 16.402 corpus: in 2,024 held
//! frames not one offset starts, where the walking rate predicts about 14, and a running offset decays 10 a frame while
//! held. The event it settles, 20260918-124946 t2298-t2302: an enemy Knight frozen by an Ice Spirit keeps offset 0 in
//! the client, and a walking Ice Golem meeting it reads 0 and turns by its own side test (-190); the engine's frozen
//! Knight (scanned, the shipped arm) starts +190 on t2298, and the Golem reads that sign and turns +190.
//!
//! THE SCENE, that event in miniature: a Blue Knight at (3500, 10000) and a Red Hog Rider at (3750, 14000), set down
//! together; a Red Freeze on the Knight's point three ticks later holds the Knight (not the Hog Rider, its caster's
//! own) facing (47, 251). The Hog Rider, which targets buildings, runs down the lane past it.
//!
//! WHAT IS PINNED (the rows are the ticks after the Freeze is cast, the first being 0):
//!   1. masked: the held Knight's offset is 0 on every row of the hold, and on row 19 the Hog Rider, which meets the
//!      Knight first on that row, starts -190 and stands on (3863, 11321);
//!   2. scanned (the old arm, non-vacuity): the held Knight starts +190 on row 18, the tick the Hog Rider enters its
//!      look circle, then 180 on row 19, and on row 19 the Hog Rider reads that sign and starts +190 on (3637, 11321);
//!   3. masked: a unit held with a running offset while a static lies in its look circle still decays 10 a tick and is
//!      not refreshed: a Knight walking past a Blue Cannon at (3600, 11500), its offset running since the Cannon first
//!      turned it, frozen after its 18th tick, goes -70, -60, ... 0 on the 14 held rows; scanned (the old arm,
//!      non-vacuity) refreshes it from the Cannon, -70, -80, ... -190;
//!   4. the shipped value is scanned.
//!
//! PLANT (regression): `held_avoidance_scanned` runs the scan for a held unit under masked: (1) and (3) go red.
//!     RUSTFLAGS='--cfg clash_plant="held_avoidance_scanned"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test held_unit_avoidance

mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE};
use royalesim::state::{BattleState, Calib, HeldUnitAvoidance};
use royalesim::Team;

const NEW: HeldUnitAvoidance = HeldUnitAvoidance::Masked;
const OLD: HeldUnitAvoidance = HeldUnitAvoidance::Scanned;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * SUBTILE_PER_MILLITILE, y * SUBTILE_PER_MILLITILE)
}

fn native(p: Vec2) -> (i32, i32) {
    (p.x / SUBTILE_PER_MILLITILE, p.y / SUBTILE_PER_MILLITILE)
}

/// One row per tick after the Freeze is cast: the held unit's offset and hold timer, the walker's offset and position.
struct Row {
    held_offset: i32,
    held_stun: i32,
    walker_offset: i32,
    walker: (i32, i32),
}

/// The Knight at `knight`, the Red `walker` card at `walker_at` (None: no walker), a Blue Cannon at `cannon` (None:
/// none); the Freeze cast on the Knight's point after `cast_after` ticks; `rows` rows from the cast.
fn scene(arm: HeldUnitAvoidance, knight: (i32, i32), walker: Option<(&str, (i32, i32))>, cannon: Option<(i32, i32)>, cast_after: u32, rows: usize) -> Vec<Row> {
    let mut cfg = config();
    cfg.calib.held_unit_avoidance = arm;
    let mut s = BattleState::new(5, cfg);
    let k = s.scenario_spawn_now(Team::Blue, "Knight", at(knight.0, knight.1), None).expect("the Knight");
    let w = walker.map(|(card, p)| s.scenario_spawn_now(Team::Red, card, at(p.0, p.1), None).expect("the walker"));
    if let Some(p) = cannon {
        s.scenario_spawn_now(Team::Blue, "Cannon", at(p.0, p.1), None).expect("the Cannon");
    }
    for _ in 0..cast_after {
        s.tick();
    }
    s.spawn_unit(Team::Red, "Freeze", at(knight.0, knight.1), None).expect("cast Freeze");
    let mut out = Vec::new();
    for _ in 0..rows {
        s.tick();
        let kv = s.entity(k).expect("the Knight is gone");
        let (walker_offset, walker_pos) = match w.and_then(|w| s.entity(w)) {
            Some(wv) => (wv.avoid_offset, native(wv.pos)),
            None => (0, (0, 0)),
        };
        out.push(Row { held_offset: kv.avoid_offset, held_stun: kv.stun_ms, walker_offset, walker: walker_pos });
    }
    out
}

/// The Hog Rider scene (the module doc's), 30 rows.
fn hog(arm: HeldUnitAvoidance) -> Vec<Row> {
    scene(arm, (3500, 10000), Some(("HogRider", (3750, 14000))), None, 3, 30)
}

/// The rows the Knight is held through (its timer up after the row and after the row before).
fn held_rows(rows: &[Row]) -> Vec<usize> {
    (1..rows.len()).filter(|&i| rows[i].held_stun > 0 && rows[i - 1].held_stun > 0).collect()
}

/// Plant: held_avoidance_scanned.
#[test]
fn a_held_unit_starts_no_offset_and_the_walker_turns_by_its_own_side() {
    let rows = hog(NEW);
    let held = held_rows(&rows);
    assert!(held.len() >= 25, "the scene drifted: the Knight is held on only {} rows", held.len());
    let started: Vec<(usize, i32)> = held.iter().map(|&i| (i, rows[i].held_offset)).filter(|r| r.1 != 0).collect();
    assert!(started.is_empty(), "masked: the held Knight's offset moved off 0 on rows {started:?}");
    let first = rows.iter().position(|r| r.walker_offset != 0).expect("the scene drifted: the Hog Rider never met the Knight");
    assert_eq!(
        (first, rows[first].walker_offset, rows[first].walker),
        (19, -190, (3863, 11321)),
        "masked: the Hog Rider's first avoidance row, its offset and its position"
    );
}

#[test]
fn the_old_arm_starts_the_held_units_offset_and_the_walker_reads_its_sign() {
    let rows = hog(OLD);
    let first_held = rows.iter().position(|r| r.held_offset != 0).expect("the scene drifted: the held Knight never started an offset");
    assert_eq!(
        (first_held, rows[first_held].held_offset, rows[first_held + 1].held_offset),
        (18, 190, 180),
        "scanned: the held Knight's first offset row and its values"
    );
    assert!(rows[first_held].held_stun > 0, "the scene drifted: the Knight was not held when it started its offset");
    let first = rows.iter().position(|r| r.walker_offset != 0).expect("the scene drifted: the Hog Rider never met the Knight");
    assert_eq!(
        (first, rows[first].walker_offset, rows[first].walker),
        (19, 190, (3637, 11321)),
        "scanned: the Hog Rider's first avoidance row, its offset and its position"
    );
}

/// Plant: held_avoidance_scanned.
#[test]
fn a_held_units_running_offset_decays_and_the_static_in_its_look_circle_does_not_refresh_it() {
    // A Knight walking past a Blue Cannon at (3600, 11500) starts -190 on its 7th tick and lets it decay; the Cannon is
    // back in its look circle from its 20th. The Freeze cast after its 18th tick holds it with the offset running, the
    // Cannon in its look circle from the second held row on.
    let held = |arm| {
        let rows = scene(arm, (3500, 10000), None, Some((3600, 11500)), 18, 14);
        assert!(rows.iter().all(|r| r.held_stun > 0), "the scene drifted: the Knight is not held on every row");
        rows.iter().map(|r| r.held_offset).collect::<Vec<i32>>()
    };
    assert_eq!(held(NEW), [-70, -60, -50, -40, -30, -20, -10, 0, 0, 0, 0, 0, 0, 0], "masked: the held Knight's offset, row by row");
    assert_eq!(
        held(OLD),
        [-70, -80, -90, -100, -110, -120, -130, -140, -150, -160, -170, -180, -190, -190],
        "scanned (non-vacuity): the Cannon refreshes the held Knight's offset, row by row"
    );
}

#[test]
fn the_shipped_value_is_the_old_arm() {
    assert_eq!(Calib::shipped().held_unit_avoidance, OLD);
}
