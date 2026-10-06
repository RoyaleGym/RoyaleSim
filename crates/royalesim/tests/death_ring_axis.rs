//! spawner.DEATH_RING_AXIS: the direction a facing ring's angle is read off (state.rs, the death path's `axis`).
//!
//! THE LAW, measured on client 15.535.29: a dying unit's facing ring (spawner.DEATH_SPAWN_LAYOUT = facing_ring_rounded)
//! lies at the whole degree of its members' heading, the direction from the death point to the target normalized to
//! 256 native units per axis and truncated, not at the degree of the raw direction. The two round apart now and then: a
//! Ram killed 2165 from the right princess tower it was charging, the raw direction (-287, 2146) at 97.62 degrees, the
//! heading (-33, 253) at 97.43; the client laid the ring at 97 (7 of 7 such deaths, the seven sp-ram-v and sp-ram-w
//! scenes), the Barbarians at (-73, +595) and (+73, -595) from the death point, where the raw direction gives 98.
//!
//! WHAT IS PINNED, on a Blue Battle Ram heading for the red right princess tower from a row of start points, each killed
//! once it has taken the tower:
//!   1. the row holds deaths where the raw direction and the heading round to different degrees, and deaths where they
//!      agree (or the scene separates nothing);
//!   2. where they differ, client15535_unit_heading lays the two Barbarians at the heading's degree and raw_direction at
//!      the raw direction's, each worked here from the sine table; both arms give the members that heading;
//!   3. where they agree, both arms lay the same points.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test death_ring_axis`):
//!   * `death_ring_axis_raw` -- the ring's angle off the raw direction under the new arm too: (2) goes red.
//!   * `death_ring_step_unread` -- spawner.DEATH_RING_DIRECTION's new arm still lays a walker's ring toward its target:
//!     `a_ram_dying_on_its_walk_lays_its_ring_on_its_step_under_client15535_walk_step` goes red.
//!   * `death_ring_facing_unread` -- spawner.DEATH_RING_DIRECTION's client15535_facing lays a walker's ring along its step:
//!     `a_ram_dying_on_its_walk_lays_its_ring_on_its_heading_under_client15535_facing` goes red.
//!
//! spawner.DEATH_RING_DIRECTION = client15535_walk_step (client 15.535.29, 9 of 9 Evo Battle Rams dying on their walk):
//! a ring dying on the walk lies at the degree of the death tick's step; target_first lays it toward the target.
//! spawner.DEATH_RING_DIRECTION = client15535_facing (client 15.535.29, 13 of 13, one Evo Battle Ram sliding along the
//! bridge's edge at t1147 of sp-form-BattleRam-evo-s0): it lies at the degree of the walker's heading after its death
//! tick, which a walk pushed off its line steps away from.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::formation::{rounded_degree, sin1024};
use royalesim::move16402::normalize_to;
use royalesim::state::{BattleState, DeathRingAxis, DeathRingDirection, DeathSpawnLayout, SpawnedFirstStep};
use royalesim::{EntityId, Team};

/// Where the red right princess tower stands, native.
const TOWER: (i32, i32) = (14500, 25500);

/// A Blue Battle Ram started at `start` (native), killed on the tick after it has taken the red right princess tower:
/// the death point, the raw direction it died on (tower - death point, subtiles), and its Barbarians (position, facing)
/// on their first frame. The death point is read off the ring: two members 180 degrees apart stand on exactly opposite
/// offsets, truncation included, so their midpoint is the death point.
fn ram_death(axis: DeathRingAxis, start: (i32, i32)) -> (Vec2, Vec2, Vec<(Vec2, Vec2)>) {
    let mut cfg = config();
    cfg.calib.death_spawn_layout = DeathSpawnLayout::FacingRingRounded;
    cfg.calib.death_ring_axis = axis;
    // spawner.SPAWNED_FIRST_STEP at none: the scene reads where the Barbarians are CREATED.
    cfg.calib.spawned_first_step = SpawnedFirstStep::None;
    let mut s = BattleState::new(0, cfg);
    let at = |p: (i32, i32)| Vec2::new(p.0 * K, p.1 * K);
    let ram = s.scenario_spawn_now(Team::Blue, "BattleRam", at(start), None).unwrap();
    let tower = s.entities().find(|e| e.team == Team::Red && e.pos == at(TOWER)).expect("the red right princess tower").id;
    for _ in 0..20 {
        s.tick();
        if s.entity(ram).and_then(|e| e.target) == Some(tower) {
            break;
        }
    }
    let rv = s.entity(ram).expect("scene: the Ram died before it took the tower");
    assert_eq!(rv.target, Some(tower), "scene: the Ram never took the tower from {start:?}");
    let before: Vec<_> = s.entities().map(|e| e.id).collect();
    assert!(s.debug_set_hp(ram, 0));
    s.tick();
    assert!(s.entity(ram).is_none(), "scene: the Ram did not die");
    let kids: Vec<(Vec2, Vec2)> = s.entities().filter(|e| !before.contains(&e.id)).map(|e| (e.pos, e.facing)).collect();
    assert_eq!(kids.len(), 2, "scene: the Ram's death released {} units", kids.len());
    let death = Vec2::new((kids[0].0.x + kids[1].0.x) / 2, (kids[0].0.y + kids[1].0.y) / 2);
    (death, at(TOWER).sub(death), kids)
}

/// The two points of a two-member ring at `deg` around `death`, worked from the sine table (each axis of the radius in
/// whole native units x the table / 1024, truncated toward zero), sorted.
fn ring(death: Vec2, deg: i32, r: i64, shift: i32) -> Vec<Vec2> {
    let rn = r / K as i64;
    let mut v: Vec<Vec2> = (0..2)
        .map(|k| {
            let d = deg + shift + k * 180;
            let (x, y) = ((rn * sin1024(d + 90) as i64 / 1024) as i32, (rn * sin1024(d) as i64 / 1024) as i32);
            death.add(Vec2::new(x * K, y * K))
        })
        .collect();
    v.sort_by_key(|p| (p.x, p.y));
    v
}

fn sorted(kids: &[(Vec2, Vec2)]) -> Vec<Vec2> {
    let mut v: Vec<Vec2> = kids.iter().map(|k| k.0).collect();
    v.sort_by_key(|p| (p.x, p.y));
    v
}

#[test]
fn a_death_ring_lies_at_its_members_heading_degree() {
    let s = BattleState::new(0, config());
    let ram = card_stat(&s, "BattleRam");
    let ds = ram.death_spawn.as_ref().expect("data: the Battle Ram has a death spawn");
    let r = ds.radius.expect("data: the Battle Ram's DeathSpawnRadius") as i64;
    let shift = ram.formation.spawn_angle_shift_deg;
    assert_eq!(ds.count, 2, "data: the Battle Ram releases two");

    let (mut apart, mut agree) = (0, 0);
    for k in 0..41 {
        let start = (14200 + 20 * k, 22800);
        let (death, raw, kids) = ram_death(DeathRingAxis::Client15535UnitHeading, start);
        let mut h = (raw.x / K, raw.y / K);
        normalize_to(&mut h, 256);
        let heading = Vec2::new(h.0, h.1);
        let (a_raw, a_head) = (rounded_degree(raw), rounded_degree(heading));
        let (death_raw, raw_again, old) = ram_death(DeathRingAxis::RawDirection, start);
        assert_eq!((death_raw, raw_again), (death, raw), "scene: the arms moved the Ram before its death from {start:?}");
        for (_, f) in kids.iter().chain(old.iter()) {
            assert_eq!(*f, heading, "a Barbarian from {start:?} does not start with the heading {heading:?}");
        }
        if a_raw == a_head {
            agree += 1;
            assert_eq!(sorted(&kids), sorted(&old), "the arms lay different rings from {start:?} where both directions round to {a_raw}");
            continue;
        }
        apart += 1;
        assert_eq!(sorted(&kids), ring(death, a_head, r, shift), "client15535_unit_heading from {start:?}: the ring is not at the heading {heading:?}'s degree {a_head} (the raw direction {raw:?} rounds to {a_raw})");
        assert_eq!(sorted(&old), ring(death, a_raw, r, shift), "raw_direction from {start:?}: the ring is not at the raw direction {raw:?}'s degree {a_raw}");
    }
    assert!(apart > 0, "precondition: no start point puts the Ram's death where the raw direction and the heading round apart");
    assert!(agree > 0, "precondition: every start point rounds apart");
}

/// A Blue Battle Ram started at `start`, a Blue Knight held on `knight` in its way, killed `walk` ticks after it has
/// taken the red right princess tower, under spawner.DEATH_RING_DIRECTION = `dir`: the death point, its death tick's
/// step (the death point less its point the tick before), the direction to the tower and its two Barbarians.
fn walking_death(dir: DeathRingDirection, start: (i32, i32), knight: (i32, i32), walk: u32) -> (Vec2, Vec2, Vec2, Vec<(Vec2, Vec2)>) {
    let (mut s, ram) = walking_scene(dir, start, knight, walk);
    let at = |p: (i32, i32)| Vec2::new(p.0 * K, p.1 * K);
    let before: Vec<_> = s.entities().map(|e| e.id).collect();
    let p0 = s.entity(ram).expect("scene: the Ram died early").pos;
    assert!(s.debug_set_hp(ram, 0));
    s.tick();
    assert!(s.entity(ram).is_none(), "scene: the Ram did not die");
    let kids: Vec<(Vec2, Vec2)> = s.entities().filter(|e| !before.contains(&e.id)).map(|e| (e.pos, e.facing)).collect();
    assert_eq!(kids.len(), 2, "scene: the Ram's death released {} units", kids.len());
    let death = Vec2::new((kids[0].0.x + kids[1].0.x) / 2, (kids[0].0.y + kids[1].0.y) / 2);
    (death, death.sub(p0), at(TOWER).sub(death), kids)
}

/// The same scene with the Ram left alive on the tick it would die on: its point and its facing after that tick (raw, 256
/// native units per axis).
fn walking_heading(start: (i32, i32), knight: (i32, i32), walk: u32) -> (Vec2, Vec2) {
    let (mut s, ram) = walking_scene(DeathRingDirection::TargetFirst, start, knight, walk);
    s.tick();
    let e = s.entity(ram).expect("scene: the Ram died on its own");
    (e.pos, e.facing)
}

/// `walking_death`'s scene up to the tick its Ram dies on: the battle and the Ram.
fn walking_scene(dir: DeathRingDirection, start: (i32, i32), knight: (i32, i32), walk: u32) -> (BattleState, EntityId) {
    let mut cfg = config();
    cfg.calib.death_spawn_layout = DeathSpawnLayout::FacingRingRounded;
    cfg.calib.death_ring_axis = DeathRingAxis::Client15535UnitHeading;
    cfg.calib.death_ring_direction = dir;
    cfg.calib.spawned_first_step = SpawnedFirstStep::None;
    let mut s = BattleState::new(0, cfg);
    let at = |p: (i32, i32)| Vec2::new(p.0 * K, p.1 * K);
    let ram = s.scenario_spawn_now(Team::Blue, "BattleRam", at(start), None).unwrap();
    let kn = s.scenario_spawn_now(Team::Blue, "Knight", at(knight), None).unwrap();
    let tower = s.entities().find(|e| e.team == Team::Red && e.pos == at(TOWER)).expect("the red right princess tower").id;
    let mut taken = None;
    for k in 0..40 {
        if s.entity(kn).is_some() {
            assert!(s.debug_set_pos(kn, at(knight)));
        }
        s.tick();
        if taken.is_none() && s.entity(ram).and_then(|e| e.target) == Some(tower) {
            taken = Some(k);
        }
        if taken.is_some_and(|t| k >= t + walk) {
            break;
        }
    }
    assert!(taken.is_some(), "scene: the Ram never took the tower from {start:?}");
    (s, ram)
}

/// The whole degree of `v`'s heading (normalized to 256 native units per axis, truncated).
fn heading_degree(v: Vec2) -> i32 {
    let mut h = (v.x / K, v.y / K);
    normalize_to(&mut h, 256);
    rounded_degree(Vec2::new(h.0, h.1))
}

/// Plant: death_ring_step_unread.
#[test]
fn a_ram_dying_on_its_walk_lays_its_ring_on_its_step_under_client15535_walk_step() {
    let s = BattleState::new(0, config());
    let ram = card_stat(&s, "BattleRam");
    let ds = ram.death_spawn.as_ref().expect("data: the Battle Ram has a death spawn");
    let r = ds.radius.expect("data: the Battle Ram's DeathSpawnRadius") as i64;
    let shift = ram.formation.spawn_angle_shift_deg;
    let mut apart = 0;
    for k in 0..12 {
        let start = (13600 + 100 * k, 21800);
        let knight = (start.0 + 200, 23200);
        let (death, step, raw, kids) = walking_death(DeathRingDirection::Client15535WalkStep, start, knight, 6);
        let (death_old, step_old, raw_old, old) = walking_death(DeathRingDirection::TargetFirst, start, knight, 6);
        assert_eq!((death_old, step_old, raw_old), (death, step, raw), "scene: the arms moved the Ram before its death from {start:?}");
        if step == Vec2::default() {
            continue;
        }
        let (a_step, a_raw) = (heading_degree(step), heading_degree(raw));
        if a_step == a_raw {
            continue;
        }
        apart += 1;
        assert_eq!(sorted(&kids), ring(death, a_step, r, shift), "client15535_walk_step from {start:?}: the ring is not at its step {step:?}'s degree {a_step}");
        // NOT VACUOUS: target_first lays it toward the tower.
        assert_eq!(sorted(&old), ring(death, a_raw, r, shift), "target_first from {start:?}: the ring is not at the tower's degree {a_raw}");
    }
    assert!(apart > 0, "precondition: no start point kills the Ram on a step whose degree differs from the tower's");
}

/// Plant: death_ring_facing_unread.
#[test]
fn a_ram_dying_on_its_walk_lays_its_ring_on_its_heading_under_client15535_facing() {
    let s = BattleState::new(0, config());
    let ram = card_stat(&s, "BattleRam");
    let ds = ram.death_spawn.as_ref().expect("data: the Battle Ram has a death spawn");
    let r = ds.radius.expect("data: the Battle Ram's DeathSpawnRadius") as i64;
    let shift = ram.formation.spawn_angle_shift_deg;
    let (mut walked, mut apart, mut slid) = (0, 0, 0);
    for k in 0..24 {
        let start = (13600 + 50 * k, 21800);
        let knight = (start.0 + 200, 23200);
        let (death, step, raw, kids) = walking_death(DeathRingDirection::Client15535Facing, start, knight, 6);
        let (point, heading) = walking_heading(start, knight, 6);
        assert_eq!(point, death, "scene: the Ram left alive from {start:?} is not at the death point");
        if step == Vec2::default() {
            continue;
        }
        walked += 1;
        // The members' heading: the Ram's facing after the death tick, normalized to 256 per axis.
        let mut h = (heading.x, heading.y);
        normalize_to(&mut h, 256);
        let h = Vec2::new(h.0, h.1);
        let (a_head, a_step, a_raw) = (rounded_degree(h), heading_degree(step), heading_degree(raw));
        for (_, f) in &kids {
            assert_eq!(*f, h, "client15535_facing from {start:?}: a Barbarian does not start with the Ram's heading {h:?} (its step {step:?})");
        }
        assert_eq!(sorted(&kids), ring(death, a_head, r, shift), "client15535_facing from {start:?}: the ring is not at the heading {h:?}'s degree {a_head} (its step's {a_step}, the tower's {a_raw})");
        apart += usize::from(a_head != a_raw);
        // NOT VACUOUS: a walk pushed off its line by the Knight, where the step's degree is not the heading's.
        slid += usize::from(a_head != a_step);
    }
    assert!(walked > 0, "precondition: no start point kills the Ram on a step");
    assert!(apart > 0, "precondition: no start point kills the Ram on a heading whose degree differs from the tower's");
    assert!(slid > 0, "precondition: no start point kills the Ram on a step off its heading");
}
