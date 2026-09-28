//! spawner.FIRST_STEP_DYING_BODIES, read off the engine: whether a unit's creation-tick first update (state.rs
//! `first_update`, under spawner.SPAWNED_FIRST_STEP = client16402_same_tick) meets the units dying on that tick.
//!
//! THE LAW, measured on client 16.402: on the two corpus births beside a unit that died on the same tick and was not
//! their parent, the dying unit stood in the first update's scans as a body at its current position. A Tombstone's
//! four death Skeletons were pushed 150 off one of its Skeletons that the same bomb killed; a Tombstone's periodic
//! Skeleton was turned and pushed off another that a Knight's hit killed on its creation tick. Under the old value,
//! hidden (the engine before this key), Reap despawns the tick's dead before a death spawn steps, and the doomed mask
//! (movement.DYING_UNIT_VISIBILITY) hides a unit an Attack-phase hit kills from an emission's step. client16402_seen
//! ships since parity scored its flip; the tests name both values through `with_arm`.
//!
//! WHAT IS PINNED, each with the precondition that makes it bite:
//!   1. a dying Tombstone's four Skeletons, born on the tick its first Skeleton dies beside the emission point: under
//!      hidden one step past the emission point, where that Skeleton stood a tick before; under client16402_seen
//!      pushed at least 100 further off it;
//!   2. a Tombstone's second Skeleton, born on the tick a Red Knight's hit kills a Blue Knight standing beside the
//!      emission point: under client16402_seen at least 80 further from that Knight than under hidden;
//!   3. with nothing dying beside it, the second Skeleton's first frame is the same under both values (the control);
//!   4. a snapshot taken under client16402_seen resumes hash for hash (the dying bodies are scratch, not state);
//!   5. the shipped value is client16402_seen.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test first_step_dying_bodies`):
//!   * `first_step_dying_hidden` -- client16402_seen still hides the dying units from a first update: (1) and (2) go
//!     red.
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, FirstStepDying};
use royalesim::{EntityId, Team};

const NEW: FirstStepDying = FirstStepDying::Seen;
const OLD: FirstStepDying = FirstStepDying::Hidden;

fn with_arm(mut cfg: BattleConfig, arm: FirstStepDying) -> BattleConfig {
    cfg.calib.first_step_dying = arm;
    cfg
}

/// A Blue Tombstone at (9000, 8000) native emits at (9000, 9500): its 1000 plus a Skeleton's 500, forward for Blue.
const TOMB: (i32, i32) = (9000, 8000);
const EMISSION: (i32, i32) = (9000, 9500);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn native(v: Vec2) -> (i32, i32) {
    (v.x / K, v.y / K)
}

/// The distance between two native points, the integer square root (the engine has no floating point).
fn dist(a: (i32, i32), b: (i32, i32)) -> i64 {
    let (dx, dy) = ((a.0 - b.0) as i64, (a.1 - b.1) as i64);
    isqrt(dx * dx + dy * dy)
}

/// Tick `s` until a Blue unit that was not on the board appears and matches `pick`; returns the newborns (by creation
/// order) on that first frame. Panics after `max` ticks.
fn tick_until_born(s: &mut BattleState, max: u32, pick: impl Fn(&royalesim::state::EntityView) -> bool) -> Vec<EntityId> {
    let before: Vec<EntityId> = s.entities().map(|e| e.id).collect();
    for _ in 0..max {
        s.tick();
        let mut born: Vec<(u32, EntityId)> =
            s.entities().filter(|e| e.team == Team::Blue && !before.contains(&e.id) && pick(e)).map(|e| (e.team_seq, e.id)).collect();
        if !born.is_empty() {
            born.sort();
            return born.into_iter().map(|(_, id)| id).collect();
        }
    }
    panic!("scene: nothing was born within {max} ticks");
}

/// A Blue Tombstone and its first Skeleton, set to 0 hp together on the Skeleton's first frame: on the next tick the
/// Skeleton takes its step and both die in one Reap. Returns the four death Skeletons' points on their first frame and
/// the first Skeleton's point on its first frame (the last on which it is seen).
fn death_beside_a_dying_skeleton(arm: FirstStepDying) -> (Vec<(i32, i32)>, (i32, i32)) {
    let mut s = BattleState::new(7, with_arm(config(), arm));
    let tomb = s.scenario_spawn_now(Team::Blue, "Tombstone", at(TOMB), None).unwrap();
    let born = tick_until_born(&mut s, 40, |e| e.spawned_by.is_some());
    assert_eq!(born.len(), 1, "scene: one Skeleton per emission");
    let sk = born[0];
    let seen = native(s.entity(sk).unwrap().pos);
    assert!(dist(seen, EMISSION) < 200, "scene: the first Skeleton stands {} from the emission point", dist(seen, EMISSION));
    assert!(s.debug_set_hp(tomb, 0));
    assert!(s.debug_set_hp(sk, 0));
    let four = tick_until_born(&mut s, 3, |e| e.spawned_by.is_none());
    assert!(s.entity(tomb).is_none() && s.entity(sk).is_none(), "scene: the Tombstone and its Skeleton did not die together");
    assert_eq!(four.len(), 4, "scene: the Tombstone's four death Skeletons");
    (four.iter().map(|id| native(s.entity(*id).unwrap().pos)).collect(), seen)
}

#[test]
fn a_death_spawn_is_pushed_off_a_unit_dying_with_its_parent() {
    let (old, seen) = death_beside_a_dying_skeleton(OLD);
    assert!(old.iter().all(|p| *p == old[0]), "hidden: the four do not share one point: {old:?}");
    let moved = dist(old[0], EMISSION);
    assert!((60..=100).contains(&moved), "hidden: the death spawn stands {moved} from the emission point, not one step");
    let (new, seen_new) = death_beside_a_dying_skeleton(NEW);
    assert_eq!(seen_new, seen, "scene: the arms parted before the deaths");
    assert!(new.iter().all(|p| *p == new[0]), "client16402_seen: the four do not share one point: {new:?}");
    assert!(dist(new[0], old[0]) >= 100, "client16402_seen: the death spawn stands {} from hidden's point", dist(new[0], old[0]));
    assert!(
        dist(new[0], seen) >= dist(old[0], seen) + 100,
        "client16402_seen: the death spawn stands {} from the dying Skeleton's last point ({} under hidden): not pushed off it",
        dist(new[0], seen),
        dist(old[0], seen)
    );
}

/// A Blue Tombstone, a Blue Knight at 1 hp beside its emission point and a Red Knight that fights it (the Python
/// test's scene). Returns (the tick the Blue Knight is gone or None, the tick the Tombstone's second Skeleton appears,
/// that Skeleton's first-frame point, the Blue Knight's last point).
fn emission_beside(arm: FirstStepDying, blue_knight: (i32, i32)) -> (Option<u32>, u32, (i32, i32), (i32, i32)) {
    // Under movement.DYING_UNIT_VISIBILITY = creation_order_before_victim BY NAME: the two FIRST_STEP arms part only
    // where the dying unit is hidden from a later mover, which the shipped whole_tick (the 2026-09-28 round 7 flip)
    // never does.
    let mut cfg = with_arm(config(), arm);
    cfg.calib.dying_unit_visibility = royalesim::state::DyingUnitVisibility::CreationOrderBeforeVictim;
    let mut s = BattleState::new(7, cfg);
    let ids = s
        .scenario_spawn_batch(&[
            (Team::Blue, "Tombstone", at(TOMB), None),
            (Team::Blue, "Knight", at(blue_knight), Some(1)),
            (Team::Red, "Knight", at((9250, 10650)), None),
        ])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let knight = ids[1];
    let mut last = native(s.entity(knight).unwrap().pos);
    let mut died = None;
    let mut births: Vec<(u32, (i32, i32))> = Vec::new();
    let mut known: Vec<EntityId> = s.entities().map(|e| e.id).collect();
    for _ in 0..30 {
        s.tick();
        let t = s.tick_count();
        match s.entity(knight) {
            Some(e) => last = native(e.pos),
            None if died.is_none() => died = Some(t),
            None => {}
        }
        for e in s.entities() {
            if !known.contains(&e.id) {
                known.push(e.id);
                if e.team == Team::Blue && e.spawned_by.is_some() {
                    births.push((t, native(e.pos)));
                }
            }
        }
        if births.len() >= 2 {
            return (died, births[1].0, births[1].1, last);
        }
    }
    panic!("scene: the Tombstone did not emit two Skeletons within 30 ticks");
}

#[test]
fn an_emission_steps_off_a_unit_doomed_on_its_creation_tick() {
    let (died, born, old, knight) = emission_beside(OLD, (9250, 9700));
    assert_eq!(died, Some(born), "scene: the Blue Knight died on {died:?}, the second Skeleton came out on {born}");
    let (died_n, born_n, new, knight_n) = emission_beside(NEW, (9250, 9700));
    assert_eq!((died_n, born_n, knight_n), (died, born, knight), "scene: the arms parted before the birth");
    assert!(dist(new, old) >= 80, "client16402_seen: the Skeleton stands {} from hidden's point {old:?}", dist(new, old));
    assert!(
        dist(new, knight) >= dist(old, knight) + 80,
        "client16402_seen: the Skeleton stands {} from the dying Knight ({} under hidden): the Knight did not turn or push it",
        dist(new, knight),
        dist(old, knight)
    );
}

#[test]
fn with_nothing_dying_beside_it_the_first_step_is_the_same_under_both_values() {
    let (died, born, old, _) = emission_beside(OLD, (3500, 9700));
    assert_ne!(died, Some(born), "scene: the far Knight died on the birth tick");
    let (_, born_n, new, _) = emission_beside(NEW, (3500, 9700));
    assert_eq!((born_n, new), (born, old), "the values parted with nothing dying beside the birth");
}

#[test]
fn a_snapshot_under_the_new_value_resumes_hash_for_hash() {
    let mut s = BattleState::new(7, with_arm(config(), NEW));
    let tomb = s.scenario_spawn_now(Team::Blue, "Tombstone", at(TOMB), None).unwrap();
    let sk = tick_until_born(&mut s, 40, |e| e.spawned_by.is_some())[0];
    assert!(s.debug_set_hp(tomb, 0));
    assert!(s.debug_set_hp(sk, 0));
    let blob = s.save();
    let mut l = BattleState::load(&blob).unwrap_or_else(|e| panic!("the snapshot does not load: {e}"));
    assert_eq!(l.state_hash(), s.state_hash());
    for k in 0..100 {
        s.tick();
        l.tick();
        assert_eq!(l.state_hash(), s.state_hash(), "diverged {k} ticks after the load");
    }
}

#[test]
fn the_shipped_value_is_seen() {
    assert_eq!(Calib::shipped().first_step_dying, FirstStepDying::Seen);
}
