//! A UNIT HELD BY A FREEZE OR A STUN STILL MEETS ITS NEIGHBOURS -- calibration collision.HELD_UNIT_CONTACT,
//! state.rs `phase_path16402_for` (the bodies' collidable flag and the held branch).
//!
//! THE LAW (client16402_speed_zero_update): a held troop takes its ordinary move update at speed 0 -- no path request,
//! no stomp clock, no step and no facing change of its own, but the avoidance scan while it is not attacking, the
//! offset's decay in both states, and the separation scan whose mean moves it -- and it stays in every neighbour's
//! scans as the troop it is. Measured on the 16.402 corpus: a held troop with a ground neighbour overlapping it moved
//! on 52 of 52 held unit-ticks (6 Goblins frozen by Ice Spirits, 3 battles); with none it stood still on 353 of 353.
//! Measured on client 15.535.29: a Knight stunned by an Electro Giant's reflect moved on 13 of 13 stunned ticks that
//! began with the Giant overlapping it and on 0 of 5 that began apart. How far a held unit is pushed per tick is not
//! pinned: these tests assert that it moves, not by how much.
//!
//! THE SCENES.
//!   * The freeze: two Blue Knights on the left lane, the rear one at (3500, 7500) created first and the front one at
//!     (3500, 13000); a Red Freeze on (3500, 13500) holds the front Knight for 80 ticks and misses the rear one (5,500
//!     away), which walks up the lane into the held Knight's back. Under out_of_the_pass the rear Knight walks into the
//!     held one's circle (the radii sum to 1000) and the held Knight does not move until the hold ends.
//!   * The stun (the client 15.535.29 Electro Giant scenario's spots): a red Knight at (13945, 15507) and a blue
//!     Electro Giant at (9500, 11500), set down together. The Giant walks up the Knight's line, the Knight attacks it and
//!     the reflect stuns it on each hit; from the second stun on, the Giant walks through it. Under out_of_the_pass the
//!     Knight stands while the Giant walks into it, and the two are thrown apart when a stun ends.
//!
//! WHAT IS PINNED (a "held" tick of the freeze scene is one the front Knight is held through; a "held" tick of the
//! stun scene is one whose Knight has its stun counter up after it or after the tick before):
//!   1. client16402_speed_zero_update, the freeze: the held Knight moves while the rear Knight is at it, on every held
//!      tick that begins with the two overlapping by 5 or more, and never with the rear Knight more than 200 past
//!      touching;
//!   2. out_of_the_pass, the freeze (the old arm): the held Knight never moves and the rear Knight gets within 800;
//!   3. both arms, the freeze: a held Knight with no neighbour stands still for the whole hold and walks on after it
//!      (tests/status.rs `a_freeze_is_a_whole_unit_hold_of_its_bufftime` is the same law for the old arm);
//!   4. client16402_speed_zero_update, the stun: the Knight moves on every held tick that begins with the Giant
//!      overlapping it by 5 or more (at least 5 such ticks);
//!   5. out_of_the_pass, the stun (the old arm): the Knight moves on no such tick, and a stun's end throws the Giant
//!      more than 100;
//!   6. both arms, the stun: on held ticks that begin with the two apart the Knight does not move (at least 5);
//!   7. the shipped value is client16402_speed_zero_update.
//!
//! PLANTS (regression):
//!   * `held_contact_invisible` leaves a held unit out of its neighbours' scans under the new arm: (1) and (4) go red.
//!   * `held_contact_walks` lets a held unit take its walking step, at its own speed, under the new arm: (3), and
//!     likely (1), go red.
//!     RUSTFLAGS='--cfg clash_plant="held_contact_invisible"' CARGO_TARGET_DIR=target/plant cargo test --test held_unit_contact

mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE};
use royalesim::state::{BattleConfig, BattleState, Calib, HeldUnitContact};
use royalesim::Team;

const NEW: HeldUnitContact = HeldUnitContact::Client16402SpeedZeroUpdate;
const OLD: HeldUnitContact = HeldUnitContact::OutOfThePass;
/// A tick "begins overlapping" when the start-of-tick centre distance is under both radii by at least this much.
const OVERLAP: i64 = 5;

fn with(arm: HeldUnitContact) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.held_unit_contact = arm;
    cfg
}

fn native(p: Vec2) -> (i32, i32) {
    (p.x / SUBTILE_PER_MILLITILE, p.y / SUBTILE_PER_MILLITILE)
}

/// Native centre distance, truncated.
fn dist(a: (i32, i32), b: (i32, i32)) -> i64 {
    let (dx, dy) = ((a.0 - b.0) as i64, (a.1 - b.1) as i64);
    isqrt(dx * dx + dy * dy)
}

// ---------------------------------------------------------------------------------------------------------------------
// The freeze scene.

/// Two Knights' collision radii, native units.
const KNIGHTS_RADII: i64 = 1000;

struct Row {
    front: (i32, i32),
    front_stun: i32,
    rear: Option<(i32, i32)>,
    rear_stun: i32,
}

/// Per tick after the tick: the front Knight's position and hold timer, the rear Knight's (if any).
fn freeze_scene(arm: HeldUnitContact, with_rear: bool) -> Vec<Row> {
    let mut s = BattleState::new(5, with(arm));
    // the rear Knight first: it is the earlier-created unit, so it moves before the front one in the pass
    let rear = if with_rear { Some(s.scenario_spawn_now(Team::Blue, "Knight", t(350, 750), None).expect("the rear Knight")) } else { None };
    let front = s.scenario_spawn_now(Team::Blue, "Knight", t(350, 1300), None).expect("the front Knight");
    let mut rows = Vec::new();
    for k in 0..95 {
        if k == 3 {
            s.spawn_unit(Team::Red, "Freeze", t(350, 1350), None).expect("cast Freeze");
        }
        s.tick();
        let f = s.entity(front).expect("the front Knight is gone");
        let (front_pos, front_stun) = (native(f.pos), f.stun_ms);
        let (rear_pos, rear_stun) = match rear {
            Some(id) => {
                let r = s.entity(id).expect("the rear Knight is gone");
                (Some(native(r.pos)), r.stun_ms)
            }
            None => (None, 0),
        };
        rows.push(Row { front: front_pos, front_stun, rear: rear_pos, rear_stun });
    }
    rows
}

/// The rows of the hold (the front Knight's timer > 0 after the tick), with the scene's preconditions.
fn hold(rows: &[Row], what: &str) -> Vec<usize> {
    let held: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].front_stun > 0).collect();
    assert!(!held.is_empty(), "{what}: the scene drifted: the Freeze never held the front Knight");
    assert_eq!(held.last().unwrap() - held[0] + 1, held.len(), "{what}: the scene drifted: the hold is not one run");
    assert!(rows.iter().all(|r| r.rear_stun == 0), "{what}: the scene drifted: the Freeze held the rear Knight too");
    held
}

/// The two Knights' centre distance at the START of row i's tick.
fn start_distance(rows: &[Row], i: usize) -> i64 {
    dist(rows[i - 1].front, rows[i - 1].rear.expect("no rear Knight"))
}

/// Plant: held_contact_invisible; likely held_contact_walks too.
#[test]
fn a_held_unit_is_pushed_by_the_unit_walking_into_it() {
    let rows = freeze_scene(NEW, true);
    let held = hold(&rows, "new arm");
    // the hold's first row applies the freeze; from the next one the front Knight is held through the whole tick
    let inside = &held[1..];
    assert!(
        inside.iter().any(|&i| start_distance(&rows, i) <= KNIGHTS_RADII + 60),
        "new arm: the scene drifted: the rear Knight never came near the held one"
    );
    let moved: Vec<usize> = inside.iter().copied().filter(|&i| rows[i].front != rows[i - 1].front).collect();
    let nearest = inside.iter().map(|&i| start_distance(&rows, i)).min().unwrap();
    assert!(!moved.is_empty(), "new arm: the rear Knight came within {nearest} of the held Knight, whose position never changed while it was held");
    let over_still: Vec<usize> =
        inside.iter().copied().filter(|&i| start_distance(&rows, i) <= KNIGHTS_RADII - OVERLAP && rows[i].front == rows[i - 1].front).collect();
    assert!(over_still.is_empty(), "new arm: the held Knight stood on held ticks {over_still:?} that began with the rear Knight overlapping it");
    let far: Vec<usize> = moved.iter().copied().filter(|&i| start_distance(&rows, i) > KNIGHTS_RADII + 200).collect();
    assert!(far.is_empty(), "new arm: the held Knight moved with nobody touching it on rows {far:?}");
}

#[test]
fn the_old_arm_walks_into_a_held_unit() {
    let rows = freeze_scene(OLD, true);
    let held = hold(&rows, "old arm");
    let inside = &held[1..];
    let moved: Vec<usize> = inside.iter().copied().filter(|&i| rows[i].front != rows[i - 1].front).collect();
    assert!(moved.is_empty(), "old arm: the held Knight moved on rows {moved:?}");
    let closest = inside.iter().map(|&i| dist(rows[i].front, rows[i].rear.unwrap())).min().unwrap();
    assert!(closest < 800, "old arm: the scene drifted: the rear Knight stayed {closest} away");
}

/// Plant: held_contact_walks.
#[test]
fn a_held_unit_with_nobody_near_stands_still() {
    for arm in [NEW, OLD] {
        let rows = freeze_scene(arm, false);
        let held = hold(&rows, &format!("{arm:?}"));
        let moved: Vec<usize> = held[1..].iter().copied().filter(|&i| rows[i].front != rows[i - 1].front).collect();
        assert!(moved.is_empty(), "{arm:?}: a held Knight with no neighbour moved on rows {moved:?}");
        let after = held.last().unwrap() + 2;
        assert!(after < rows.len() && rows[after].front != rows[*held.last().unwrap()].front, "{arm:?}: the Knight never walked again");
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// The stun scene.

const KNIGHT_AT: (i32, i32) = (13945, 15507);
const GIANT_AT: (i32, i32) = (9500, 11500);
/// Both collision radii (the Knight's 500, the Electro Giant's 750), native.
const KNIGHT_GIANT_RADII: i64 = 1250;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * SUBTILE_PER_MILLITILE, p.1 * SUBTILE_PER_MILLITILE)
}

/// Per tick: (held, the start-of-tick centre distance, the Knight's move, the Giant's move, the distance from the
/// Knight's start to the Giant's END of the tick), native. In this scene the Giant takes its step before the Knight's
/// contact update, so a tick's contact is judged against the moved Giant: a tick that begins 1,269 apart meets a Giant
/// 40 nearer. The overlapping and the apart ticks are classified on the last field (with the start distance for apart).
fn stun_scene(arm: HeldUnitContact, ticks: u32) -> Vec<(bool, i64, i64, i64, i64)> {
    let mut s = BattleState::new(0, with(arm));
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at(KNIGHT_AT), None), (Team::Blue, "ElectroGiant", at(GIANT_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (k, g) = (ids[0], ids[1]);
    let mut rows = Vec::new();
    let mut was_stunned = false;
    for _ in 0..ticks {
        let (Some(kv), Some(gv)) = (s.entity(k), s.entity(g)) else { break };
        let (k0, g0) = (native(kv.pos), native(gv.pos));
        s.tick();
        let (Some(kv), Some(gv)) = (s.entity(k), s.entity(g)) else { break };
        let stunned = kv.stun_ms > 0;
        rows.push((stunned || was_stunned, dist(k0, g0), dist(k0, native(kv.pos)), dist(g0, native(gv.pos)), dist(k0, native(gv.pos))));
        was_stunned = stunned;
    }
    rows
}

/// Plant: held_contact_invisible.
#[test]
fn a_stunned_knight_is_moved_by_the_giant_walking_through_it() {
    let rows = stun_scene(NEW, 170);
    let over: Vec<(usize, i64)> = rows.iter().enumerate().filter(|(_, r)| r.0 && r.4 <= KNIGHT_GIANT_RADII - OVERLAP).map(|(i, r)| (i, r.2)).collect();
    assert!(over.len() >= 5, "the scene drifted: only {} stunned ticks began with the Giant overlapping", over.len());
    let still: Vec<usize> = over.iter().filter(|o| o.1 == 0).map(|o| o.0).collect();
    assert!(still.is_empty(), "new arm: the stunned Knight stood on overlapping ticks {still:?} (of {over:?})");
}

#[test]
fn the_old_arm_holds_the_stunned_knight_and_throws_the_giant() {
    let rows = stun_scene(OLD, 170);
    let over: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r.0 && r.4 <= KNIGHT_GIANT_RADII - OVERLAP).map(|(i, _)| i).collect();
    assert!(over.len() >= 5, "the scene drifted: only {} stunned ticks began with the Giant overlapping", over.len());
    let moved: Vec<usize> = over.iter().copied().filter(|&i| rows[i].2 > 0).collect();
    assert!(moved.is_empty(), "old arm: the stunned Knight moved on {moved:?}");
    assert!(rows.iter().any(|r| r.3 > 100), "the scene drifted: the Giant was never thrown back when a stun ended");
}

#[test]
fn a_stunned_knight_with_nothing_overlapping_stands_under_both_arms() {
    for arm in [NEW, OLD] {
        let rows = stun_scene(arm, 170);
        let apart: Vec<usize> = rows.iter().enumerate().filter(|(_, r)| r.0 && r.1.min(r.4) >= KNIGHT_GIANT_RADII).map(|(i, _)| i).collect();
        assert!(apart.len() >= 5, "the scene drifted: only {} stunned ticks began apart", apart.len());
        let moved: Vec<usize> = apart.iter().copied().filter(|&i| rows[i].2 > 0).collect();
        assert!(moved.is_empty(), "{arm:?}: the stunned Knight moved with nothing overlapping it on {moved:?}");
    }
}

#[test]
fn the_shipped_value_is_the_new_arm() {
    assert_eq!(Calib::shipped().held_unit_contact, NEW);
}
