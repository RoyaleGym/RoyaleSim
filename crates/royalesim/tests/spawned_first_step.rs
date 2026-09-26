//! spawner.SPAWNED_FIRST_STEP, read off the engine: a unit created after the tick's passes takes its first update
//! on the tick it is created (state.rs `first_update`).
//!
//! THE LAW, measured on client 15.535.29: on its first frame an emitted or death-spawned unit has already taken its
//! whole first update. With an enemy in range it is in its attack; otherwise it stands one move step from where it
//! was created. The first Skeleton of 8 of 8 Tombstone waves stands one Skeleton step from the emission point, and a
//! dying Tombstone's four Skeletons stand together on one point one step past it. Under the old value, none, the unit
//! stands on its creation point until the next tick.
//!
//! WHAT IS PINNED, each with the precondition that makes it bite:
//!   1. a lone Tombstone's first Skeleton: under none it stands on the emission point on its first frame; under
//!      client16402_same_tick one step from it, and until the wave's second Skeleton comes out it walks the old arm's
//!      path a tick early;
//!   2. a dying Tombstone's four Skeletons share one point on their first frame, one step past the emission point
//!      (on it under none): the members do not push each other on that step;
//!   3. a Golem's Golemites, whose row carries DeathSpawnPushback, stand on their first frame where they stand under
//!      none, under either value of spawner.DEATH_SPAWN_PUSHBACK: they are inert on the death frame;
//!   4. a Skeleton emitted beside an enemy Knight has the Knight as its target and is in its attack on its first frame,
//!      with the attack state the old arm reaches a tick later;
//!   5. a battle with no spawner and no death spawn runs the same under both values (the control);
//!   6. a snapshot taken under client16402_same_tick resumes hash for hash;
//!   7. the shipped value is none.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test spawned_first_step`):
//!   * `first_step_unread` -- the new value stands on the creation point: (1), (2) and (4) go red.
//!   * `first_step_siblings_push` -- the members of one death push each other on the first step: (2) goes red.
//!   * `first_step_moves_pushback_spawns` -- a DeathSpawnPushback row's members step on the death frame: (3) goes red.
//!   * `first_step_walks_only` -- the first update is the move step alone, no target and no attack: (4) goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, DeathSpawnPushback, SpawnedFirstStep};
use royalesim::{EntityId, Team};

const NEW: SpawnedFirstStep = SpawnedFirstStep::SameTick;
const OLD: SpawnedFirstStep = SpawnedFirstStep::None;

fn with_arm(mut cfg: BattleConfig, arm: SpawnedFirstStep) -> BattleConfig {
    cfg.calib.spawned_first_step = arm;
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

/// Tick `s` until a Blue troop that was not on the board appears and matches `pick`; returns the newborns (by
/// creation order) on that first frame. Panics after `max` ticks.
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

/// A lone Blue Tombstone's first periodic Skeleton, native, on its first 11 frames, and the first of those frames on
/// which the wave's second Skeleton is on the board.
fn lone_wave(arm: SpawnedFirstStep) -> (Vec<(i32, i32)>, usize) {
    let mut s = BattleState::new(7, with_arm(config(), arm));
    s.scenario_spawn_now(Team::Blue, "Tombstone", at(TOMB), None).unwrap();
    let born = tick_until_born(&mut s, 40, |e| e.spawned_by.is_some());
    assert_eq!(born.len(), 1, "scene: one Skeleton per emission");
    let sk = born[0];
    let mut path = vec![native(s.entity(sk).unwrap().pos)];
    let mut second = None;
    for k in 1..11 {
        s.tick();
        path.push(native(s.entity(sk).expect("scene: the Skeleton died in its first 11 frames").pos));
        if second.is_none() && s.entities().any(|e| e.spawned_by.is_some() && e.id != sk) {
            second = Some(k);
        }
    }
    (path, second.expect("scene: the wave's second Skeleton did not come out within 11 frames"))
}

#[test]
fn an_emitted_skeleton_takes_its_first_step_on_its_creation_tick() {
    let (old, n) = lone_wave(OLD);
    assert_eq!(old[0], EMISSION, "none: the first Skeleton is not on the emission point on its first frame");
    assert!(old[1] != old[0], "scene: the old arm's Skeleton does not walk on its second frame");
    let (new, _) = lone_wave(NEW);
    let moved = dist(new[0], EMISSION);
    assert!((60..=100).contains(&moved), "the first Skeleton stands {moved} from the emission point on its first frame, not one step");
    // The wave's second Skeleton comes out on frame n, within contact of the first (two radii of 500, 801 apart),
    // and pushes it from the next tick on, at a point of its path that differs between the arms: the paths are
    // compared up to frame n of the old arm, which the push has not reached.
    assert!(n >= 5, "scene: the second Skeleton came out on frame {n}, too soon to compare the paths");
    assert_eq!(&new[..n], &old[1..=n], "the new arm's first {n} frames are not the old arm's path a tick early");
}

/// A Blue Tombstone killed once its first wave has walked off (its two Skeletons 10 ticks apart, the next wave 70
/// ticks later), so nothing stands near the emission point; its four death Skeletons, native, on their first frame.
fn death_stack(arm: SpawnedFirstStep) -> Vec<(i32, i32)> {
    let mut s = BattleState::new(7, with_arm(config(), arm));
    let tomb = s.scenario_spawn_now(Team::Blue, "Tombstone", at(TOMB), None).unwrap();
    tick_until_born(&mut s, 40, |e| e.spawned_by.is_some());
    tick_until_born(&mut s, 40, |e| e.spawned_by.is_some());
    for _ in 0..12 {
        s.tick();
    }
    let near: Vec<(i32, i32)> = s.entities().filter(|e| e.card == "Skeleton").map(|e| native(e.pos)).filter(|p| dist(*p, EMISSION) < 1000).collect();
    assert!(near.is_empty(), "scene: a periodic Skeleton still stands near the emission point: {near:?}");
    assert!(s.debug_set_hp(tomb, 0));
    let born = tick_until_born(&mut s, 3, |e| e.spawned_by.is_none());
    assert!(s.entity(tomb).is_none(), "scene: the Tombstone did not die");
    assert_eq!(born.len(), 4, "scene: the Tombstone's four death Skeletons");
    born.iter().map(|id| native(s.entity(*id).unwrap().pos)).collect()
}

#[test]
fn a_dying_tombstones_four_skeletons_take_one_step_together() {
    let old = death_stack(OLD);
    assert!(old.iter().all(|p| *p == EMISSION), "none: the death Skeletons are not all on the emission point: {old:?}");
    let new = death_stack(NEW);
    assert!(new.iter().all(|p| *p == new[0]), "the four death Skeletons do not share one point on their first frame: {new:?}");
    let moved = dist(new[0], EMISSION);
    assert!((60..=100).contains(&moved), "the death stack stands {moved} from the emission point, not one step");
}

/// A Blue Golem at 0 hp at (9000, 13000) dies on the first tick; its Golemites, native, on their first frame.
fn golemites(arm: SpawnedFirstStep, pushback: DeathSpawnPushback) -> Vec<(i32, i32)> {
    let mut cfg = with_arm(config(), arm);
    cfg.calib.death_spawn_pushback = pushback;
    let mut s = BattleState::new(7, cfg);
    let golem = s.scenario_spawn_now(Team::Blue, "Golem", at((9000, 13000)), None).unwrap();
    assert!(s.debug_set_hp(golem, 0));
    let born = tick_until_born(&mut s, 3, |e| e.spawned_by.is_none());
    assert_eq!(born.len(), 2, "scene: the Golem's two Golemites");
    born.iter().map(|id| native(s.entity(*id).unwrap().pos)).collect()
}

#[test]
fn a_death_spawn_pushback_rows_members_are_inert_on_the_death_frame() {
    assert!(card_stat(&BattleState::new(7, config()), "Golem").death_spawn_pushback, "data: the Golem row carries DeathSpawnPushback");
    for pushback in [DeathSpawnPushback::NotRead, DeathSpawnPushback::ClientRingSlide] {
        let old = golemites(OLD, pushback);
        assert_eq!(golemites(NEW, pushback), old, "{pushback:?}: the Golemites moved on the death frame");
    }
}

/// A Blue Tombstone's first Skeleton with a Red Knight in reach of it: (target, attack phase, attack ms) on its first
/// two frames.
fn beside_an_enemy(arm: SpawnedFirstStep) -> (EntityId, Vec<(Option<EntityId>, AttackPhase, i32)>) {
    let mut s = BattleState::new(7, with_arm(config(), arm));
    s.scenario_spawn_now(Team::Blue, "Tombstone", at(TOMB), None).unwrap();
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 10300)), None).unwrap();
    let born = tick_until_born(&mut s, 40, |e| e.spawned_by.is_some());
    let sk = born[0];
    let row = |s: &BattleState| {
        let e = s.entity(sk).expect("scene: the Skeleton died on its first frames");
        (e.target, e.attack_phase, e.attack_ms)
    };
    let first = row(&s);
    s.tick();
    (knight, vec![first, row(&s)])
}

#[test]
fn a_skeleton_emitted_beside_an_enemy_is_in_its_attack_on_its_first_frame() {
    let (knight, old) = beside_an_enemy(OLD);
    assert_eq!((old[0].0, old[0].1), (None, AttackPhase::Idle), "none: the Skeleton is not idle with no target on its first frame");
    assert_eq!(old[1].0, Some(knight), "scene: under none the Skeleton does not take the Knight on its second frame");
    assert!(old[1].1 != AttackPhase::Idle, "scene: under none the Skeleton is not in its attack on its second frame");
    let (knight, new) = beside_an_enemy(NEW);
    assert_eq!(new[0].0, Some(knight), "the Skeleton's first-frame target is not the Knight");
    assert_eq!((new[0].1, new[0].2), (old[1].1, old[1].2), "the first-frame attack state is not the one the old arm reaches a tick later");
}

#[test]
fn a_battle_with_no_spawner_and_no_death_spawn_runs_the_same_under_both_values() {
    let run = |arm| {
        let mut s = BattleState::new(7, with_arm(config(), arm));
        s.scenario_spawn_batch(&[(Team::Blue, "Knight", at((9000, 12000)), None), (Team::Red, "Knight", at((9000, 20000)), None), (Team::Red, "Musketeer", at((14500, 22000)), None)])
            .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
        let mut rows = Vec::new();
        for _ in 0..300 {
            s.tick();
            rows.push(s.entities().map(|e| (e.id, e.pos, e.hp, e.target, e.attack_phase)).collect::<Vec<_>>());
        }
        rows
    };
    let old = run(OLD);
    assert!(old.last().unwrap().len() < old[0].len(), "vacuous: nothing died in 300 ticks");
    assert!(old == run(NEW), "the two values diverged on a battle neither reaches");
}

#[test]
fn a_snapshot_under_the_new_value_resumes_hash_for_hash() {
    let mut s = BattleState::new(7, with_arm(config(), NEW));
    s.scenario_spawn_now(Team::Blue, "Tombstone", at(TOMB), None).unwrap();
    s.scenario_spawn_now(Team::Red, "Knight", at((9000, 12000)), None).unwrap();
    tick_until_born(&mut s, 40, |e| e.spawned_by.is_some());
    let blob = s.save();
    let mut l = BattleState::load(&blob).unwrap_or_else(|e| panic!("the snapshot does not load: {e}"));
    assert_eq!(l.state_hash(), s.state_hash());
    for k in 0..200 {
        s.tick();
        l.tick();
        assert_eq!(l.state_hash(), s.state_hash(), "diverged {k} ticks after the load");
    }
}

#[test]
fn the_shipped_value_is_none() {
    assert_eq!(Calib::shipped().spawned_first_step, SpawnedFirstStep::None);
}
