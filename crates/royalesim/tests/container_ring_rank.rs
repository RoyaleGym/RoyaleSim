//! targeting.CONTAINER_RING_RANK: a container's death-spawn ring carries a fixed target priority (target.rs `key`,
//! Entities::ring_rank, written by state.rs `release_fuse_end` through PendingSpawn::ring_rank).
//!
//! THE LAW, measured on client 16.635 (parity r98, the oracle's il-mg-16635 set: 13 of 13 picks where the client took a
//! farther Skeleton Barrel ring member than the engine took the higher-index one; 0 of 856 agreed picks against): under
//! client16635_ring_index_80 member i of a container's ring (0..6 in its ring's creation order) ranks in every target scan
//! as if its squared centre distance were (80 i)^2 native^2 smaller, floored at 0; member 0 ranks as before. Under none
//! (the old arm) the members rank by their plain centre distance. The sight test and the equal-distance tie are unchanged.
//!
//! WHAT IS PINNED. A Skeleton Barrel is killed on its enemy's half (tests/skeleton_barrel.rs's scene); its seven Skeletons
//! slide out on T + 13 .. T + 16, and on T + 17, while they still deploy, they are put down around the enemy's princess
//! tower W: member 0 5,000 from W, member 6 5,006.4 (25,064,064 native^2 against 25,000,000, inside member 6's cut of
//! 230,400) and the other five past 6,800. W's first pick, on the members' 8th frame or later:
//!   1. member 0 under none (the vacuity check), member 6 under client16635_ring_index_80, on the same tick, every member
//!      on the same point in both runs on every tick up to it (the law moves no position);
//!   2. the same for the other seat: a Blue barrel on Red's half against Red's princess tower.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! container_ring_rank`):
//!   * `ring_rank_unread` -- the new arm still ranks the members by their plain distance: (1) and (2) go red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, ContainerRingRank, DeathSpawnPushback, SpawnedUnitAcquireDelay, TargetRankDistance};
use royalesim::target::CONTAINER_RING_RANK_STEP;
use royalesim::{EntityId, Team};

const BARREL: &str = "SkeletonBalloon";

/// Every measurement is at level 11.
const LEVEL: i32 = 11;

/// Where the Red barrel is killed (Blue's half), and Blue's princess tower the scene is laid around (the nearest to it).
const P0: (i32, i32) = (5000, 10500);
const W0: (i32, i32) = (3500, 6500);

/// Member k's point from W, native, in Blue's frame (+y toward the river): member 0 at 5,000, member 6 at 5,006.4, the
/// other five past 6,800, every two at least 1,800 apart (no contact).
const SPOTS: [(i32, i32); 7] = [(0, 5000), (-2500, 6800), (-1000, 6800), (500, 6800), (2000, 6800), (3500, 6800), (3000, 4008)];

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn native(v: Vec2) -> (i32, i32) {
    (v.x / K, v.y / K)
}

/// The arena point turned 180 degrees about the centre, native.
fn turn(s: &BattleState, p: (i32, i32)) -> (i32, i32) {
    (s.arena().width / K - p.0, s.arena().height / K - p.1)
}

/// The shipped config at level 11 with targeting.CONTAINER_RING_RANK = `arm`; the ring's own arms asserted.
fn config_with(arm: ContainerRingRank) -> BattleConfig {
    let mut cfg = config();
    cfg.card_level = [LEVEL, LEVEL];
    assert_eq!(cfg.calib.death_spawn_pushback, DeathSpawnPushback::ClientRingSlide, "the shipped spawner.DEATH_SPAWN_PUSHBACK");
    assert_ne!(cfg.calib.spawned_unit_acquire_delay, SpawnedUnitAcquireDelay::None, "the shipped targeting.SPAWNED_UNIT_ACQUIRE_DELAY");
    // The rank's cut is read in the centre-distance rank (target.rs `key`).
    assert_eq!(cfg.calib.target_rank_distance, TargetRankDistance::Client16402Centre, "the shipped targeting.TARGET_RANK_DISTANCE");
    cfg.calib.container_ring_rank = arm;
    cfg
}

/// A `team` Skeleton Barrel put at `p` (native) and killed on the next tick. Returns T, the tick it died on.
fn kill_barrel(s: &mut BattleState, team: Team, p: (i32, i32)) -> u32 {
    let id = s.scenario_spawn_now(team, BARREL, at(p), None).expect("the barrel is placed");
    assert!(s.debug_set_hp(id, 0));
    let t = s.tick_count();
    s.tick();
    assert!(s.entity(id).is_none(), "the barrel died on its tick");
    t
}

/// Tick until the battle stands after tick `tick`.
fn after(s: &mut BattleState, tick: u32) {
    while s.tick_count() <= tick {
        s.tick();
    }
}

/// The container's Skeletons of `team`, in creation order.
fn members(s: &BattleState, team: Team) -> Vec<EntityId> {
    let mut m: Vec<(u32, EntityId)> = s.entities().filter(|v| v.team == team && v.card == "Skeleton" && v.spawned_by.is_none()).map(|v| (v.team_seq, v.id)).collect();
    m.sort();
    m.into_iter().map(|(_, id)| id).collect()
}

/// `team`'s princess tower nearest `near` (native): its id and its point, native.
fn princess_tower(s: &BattleState, team: Team, near: (i32, i32)) -> (EntityId, (i32, i32)) {
    let v = s
        .entities()
        .filter(|v| v.team == team && v.kind == EntityKind::PrincessTower)
        .min_by_key(|v| v.pos.dist2(at(near)))
        .expect("a princess tower");
    (v.id, native(v.pos))
}

/// The scene under `arm`: a `team` barrel killed on `p`; on T + 17 its member k is put on `spots[k]` (native); ticked until
/// the enemy princess tower on `w` holds one of them. (the tick from T, the member's index, every member's point after
/// each tick from T + 12 to then)
fn scene(arm: ContainerRingRank, team: Team, p: (i32, i32), w: (i32, i32), spots: &[(i32, i32); 7]) -> (u32, usize, Vec<Vec<Vec2>>) {
    let mut s = BattleState::new(9, config_with(arm));
    let (tower, wp) = princess_tower(&s, team.other(), w);
    assert_eq!(wp, w, "scene: the tower stands on W");
    let t = kill_barrel(&mut s, team, p);
    after(&mut s, t + 12);
    let m = members(&s, team);
    assert_eq!(m.len(), 7, "{team:?} {arm:?}: the seven Skeletons on T + 12");
    let mut track: Vec<Vec<Vec2>> = Vec::new();
    for k in 12..=40u32 {
        after(&mut s, t + k);
        track.push(m.iter().map(|id| s.entity(*id).expect("a Skeleton").pos).collect());
        if k == 16 {
            for (id, q) in m.iter().zip(spots.iter()) {
                assert!(s.debug_set_pos(*id, at(*q)), "the member is alive on T + 16");
            }
        } else if k > 16 {
            let held = s.entity(tower).expect("the tower stands").target;
            if let Some(j) = held.and_then(|tg| m.iter().position(|id| *id == tg)) {
                return (k, j, track);
            }
        }
    }
    panic!("{team:?} {arm:?}: the tower took no Skeleton by T + 40");
}

/// The scene for `team`'s barrel on `p` against the enemy princess tower nearest `w_near`, its spots turned by `sign` (1 in
/// Blue's frame, -1 in Red's): member 0 under none, member 6 under client16635_ring_index_80, on one tick, on one track.
fn check(team: Team, p: (i32, i32), w_near: (i32, i32), sign: i32) {
    let s = BattleState::new(9, config_with(ContainerRingRank::None));
    let (_, w) = princess_tower(&s, team.other(), w_near);
    let spots = SPOTS.map(|(dx, dy)| (w.0 + sign * dx, w.1 + sign * dy));
    // The scene's arithmetic: member 0 the nearest by plain distance, member 6 the nearest once each member's cut is taken.
    let d2 = |k: usize| {
        let (dx, dy) = (i64::from(spots[k].0 - w.0), i64::from(spots[k].1 - w.1));
        dx * dx + dy * dy
    };
    let cut = |k: usize| (i64::from(CONTAINER_RING_RANK_STEP) * k as i64).pow(2);
    assert!((1..7).all(|k| d2(0) < d2(k)), "scene: member 0 is the nearest by plain distance");
    assert!((0..6).all(|k| d2(6) - cut(6) < d2(k) - cut(k)), "scene: member 6 is the nearest once each member's cut is taken");
    let (t_old, j_old, track_old) = scene(ContainerRingRank::None, team, p, w, &spots);
    assert_eq!(j_old, 0, "{team:?} none (the vacuity check): the tower takes member 0, the nearest");
    let (t_new, j_new, track_new) = scene(ContainerRingRank::Client16635RingIndex80, team, p, w, &spots);
    assert_eq!(j_new, 6, "{team:?} client16635_ring_index_80: the tower takes member 6, its (80 x 6)^2 cut the nearer");
    assert_eq!(t_new, t_old, "{team:?}: the first pick's tick is not the arm's");
    assert!(t_old >= 19, "{team:?}: a pick before the members' 8th frame (T + 19): T + {t_old}");
    assert_eq!(track_new, track_old, "{team:?}: the law moves no position");
}

#[test]
fn a_princess_tower_takes_the_ring_member_its_index_brings_nearer() {
    check(Team::Red, P0, W0, 1);
}

#[test]
fn the_other_seat_ranks_its_ring_alike() {
    let s = BattleState::new(9, config_with(ContainerRingRank::None));
    check(Team::Blue, turn(&s, P0), turn(&s, W0), -1);
}

#[test]
fn the_shipped_arm_is_none() {
    assert_eq!(royalesim::state::Calib::shipped().container_ring_rank, ContainerRingRank::None);
}
