//! spawner.FIRST_STEP_DYING_CONTACT: whether the units dying on a newborn's tick, which spawner.FIRST_STEP_DYING_BODIES =
//! client16402_seen puts in its creation-tick first update, push it (state.rs `phase_path16402_for`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, sp-hero2-Tombstone-nopress-s0 t222: a Tombstone Skeleton
//! emitted on the tick a Knight's hit kills another Skeleton 624 off is turned by it (offset -190, a dropped waypoint)
//! and not pushed off it.
//!
//! THE SCENES (tests/first_step_dying_bodies.rs's): a Tombstone's second Skeleton born on the tick a Red Knight's hit
//! kills a Blue Knight beside the emission point (the doomed in the pass); a dying Tombstone's four Skeletons born on the
//! tick its first Skeleton dies beside the emission point (the Reap's dead).
//!
//! WHAT IS PINNED, and the plant that turns it red (first_step_dying_pushes):
//!   1. the emission: under pushed (the engine's, the vacuity check) the Skeleton stands at least 80 further from the
//!      dying Knight than under FIRST_STEP_DYING_BODIES = hidden; under client15535_avoidance_only less than 60 further
//!      (a turned step, no push);
//!   2. the death spawn: under pushed at least 100 further from the dying Skeleton's last point than under hidden; under
//!      client15535_avoidance_only one step from the emission point (60..=100), less than 60 further than under hidden.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, FirstStepDying, FirstStepDyingContact};
use royalesim::{EntityId, Team};

const TOMB: (i32, i32) = (9000, 8000);
const EMISSION: (i32, i32) = (9000, 9500);

fn arms(mut cfg: BattleConfig, seen: FirstStepDying, contact: FirstStepDyingContact) -> BattleConfig {
    cfg.calib.first_step_dying = seen;
    cfg.calib.first_step_dying_contact = contact;
    cfg
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn native(v: Vec2) -> (i32, i32) {
    (v.x / K, v.y / K)
}

fn dist(a: (i32, i32), b: (i32, i32)) -> i64 {
    let (dx, dy) = ((a.0 - b.0) as i64, (a.1 - b.1) as i64);
    isqrt(dx * dx + dy * dy)
}

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

/// The emission scene: (the tick the Blue Knight is gone, the second Skeleton's birth tick, its first-frame point, the
/// Blue Knight's last point).
fn emission(seen: FirstStepDying, contact: FirstStepDyingContact) -> (Option<u32>, u32, (i32, i32), (i32, i32)) {
    let mut cfg = arms(config(), seen, contact);
    cfg.calib.dying_unit_visibility = royalesim::state::DyingUnitVisibility::CreationOrderBeforeVictim;
    let mut s = BattleState::new(7, cfg);
    let ids = s
        .scenario_spawn_batch(&[
            (Team::Blue, "Tombstone", at(TOMB), None),
            (Team::Blue, "Knight", at((9250, 9700)), Some(1)),
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

/// The death spawn scene: the four death Skeletons' points on their first frame and the first Skeleton's last point.
fn death(seen: FirstStepDying, contact: FirstStepDyingContact) -> (Vec<(i32, i32)>, (i32, i32)) {
    let mut s = BattleState::new(7, arms(config(), seen, contact));
    let tomb = s.scenario_spawn_now(Team::Blue, "Tombstone", at(TOMB), None).unwrap();
    let born = tick_until_born(&mut s, 40, |e| e.spawned_by.is_some());
    let sk = born[0];
    let seen_at = native(s.entity(sk).unwrap().pos);
    assert!(s.debug_set_hp(tomb, 0));
    assert!(s.debug_set_hp(sk, 0));
    let four = tick_until_born(&mut s, 3, |e| e.spawned_by.is_none());
    assert_eq!(four.len(), 4, "scene: the Tombstone's four death Skeletons");
    (four.iter().map(|id| native(s.entity(*id).unwrap().pos)).collect(), seen_at)
}

/// Plant: first_step_dying_pushes.
#[test]
fn an_emission_is_not_pushed_off_a_unit_dying_on_its_tick_under_client15535_avoidance_only() {
    let (died, born, hidden, knight) = emission(FirstStepDying::Hidden, FirstStepDyingContact::Pushed);
    assert_eq!(died, Some(born), "scene: the Blue Knight died on {died:?}, the second Skeleton came out on {born}");
    let (_, _, pushed, _) = emission(FirstStepDying::Seen, FirstStepDyingContact::Pushed);
    // NOT VACUOUS: the engine's arm pushes the newborn off the dying Knight.
    assert!(dist(pushed, knight) >= dist(hidden, knight) + 80, "pushed: {} from the Knight against hidden's {}", dist(pushed, knight), dist(hidden, knight));
    let (died_a, born_a, avoid, knight_a) = emission(FirstStepDying::Seen, FirstStepDyingContact::Client15535AvoidanceOnly);
    assert_eq!((died_a, born_a, knight_a), (died, born, knight), "scene: the arms parted before the birth");
    assert!(dist(avoid, knight) < dist(hidden, knight) + 60, "client15535_avoidance_only: {} from the Knight against hidden's {}: pushed off it", dist(avoid, knight), dist(hidden, knight));
}

/// Plant: first_step_dying_pushes.
#[test]
fn a_death_spawn_is_not_pushed_off_a_unit_dying_in_its_reap_under_client15535_avoidance_only() {
    let (hidden, seen_at) = death(FirstStepDying::Hidden, FirstStepDyingContact::Pushed);
    let (pushed, _) = death(FirstStepDying::Seen, FirstStepDyingContact::Pushed);
    // NOT VACUOUS: the engine's arm pushes the four off the dying Skeleton.
    assert!(dist(pushed[0], seen_at) >= dist(hidden[0], seen_at) + 100, "pushed: {} from the dying Skeleton against hidden's {}", dist(pushed[0], seen_at), dist(hidden[0], seen_at));
    let (avoid, seen_a) = death(FirstStepDying::Seen, FirstStepDyingContact::Client15535AvoidanceOnly);
    assert_eq!(seen_a, seen_at, "scene: the arms parted before the deaths");
    assert!(avoid.iter().all(|p| *p == avoid[0]), "client15535_avoidance_only: the four do not share one point: {avoid:?}");
    let moved = dist(avoid[0], EMISSION);
    assert!((60..=100).contains(&moved), "client15535_avoidance_only: {moved} from the emission point, not one step");
    assert!(dist(avoid[0], seen_at) < dist(hidden[0], seen_at) + 60, "client15535_avoidance_only: pushed off the dying Skeleton");
}

#[test]
fn the_shipped_value_is_pushed() {
    assert_eq!(Calib::shipped().first_step_dying_contact, FirstStepDyingContact::Pushed);
}
