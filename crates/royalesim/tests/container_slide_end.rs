//! spawner.CONTAINER_SLIDE_END: where a container ring member's slide ends beside an arena edge, and how many steps it
//! makes (state.rs `container_end_inset`, `slide_end_points`, `slide_birth_toward_end`, `release_fuse_end`'s per-member
//! cap).
//!
//! THE LAW, measured on client 16.635 (parity r98, the oracle's il-mg-16635 set: 6 of 6 step counts and 4 of 4 birth
//! directions of the Skeleton Barrel members whose ring point lay past 250 from a side edge): under
//! client16635_arena_inset_250 a container ring member's ring point is kept 250 native inside every arena edge; the member
//! is born one step toward that point and slides min(4, floor(|end - birth| / 250)) steps toward it, none if 0. Under
//! ring_point (the old arm) the end is the bare ring point at DeathSpawnRadius and every member slides 4 ticks.
//!
//! WHAT IS PINNED, on a Blue Skeleton Barrel killed on (1017, 11000), its burst point C about 1,017 from the left edge, and
//! on its mirror, a Red one beside the right edge:
//!   1. the on-axis member nearest the edge, born 250 from C: under the new arm its slide ends 250 inside the edge and runs
//!      2 ticks (floor((C.x - 250 - 250) / 250)), and it rests inside the edge; under ring_point (the vacuity check) its
//!      end lies past the edge, it slides 4 ticks and ends on the arena's own clamp;
//!   2. the members whose ring points lie inside the inset keep their ends and their 4 slide ticks under both arms;
//!   3. the diagonal members whose ring points lie past the inset are born one step toward their inset ends, not toward
//!      their bare ring points.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! container_slide_end`):
//!   * `container_slide_end_uninset` -- the new arm keeps the bare ring point and the 4-tick cap: (1) and (3) go red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::move16402;
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, ContainerSlideEnd, DeathSlideAim, DeathSlideBirth, DeathSpawnPushback, CONTAINER_END_INSET};
use royalesim::{EntityId, Team};

const BARREL: &str = "SkeletonBalloon";

/// Every measurement is at level 11.
const LEVEL: i32 = 11;

/// Where the Blue barrel is killed: 1,017 from the left edge, on land, no enemy within reach.
const P: (i32, i32) = (1017, 11000);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn native(v: Vec2) -> (i32, i32) {
    (v.x / K, v.y / K)
}

/// The shipped config at level 11 with spawner.CONTAINER_SLIDE_END = `arm`; the slide's own arms asserted.
fn config_with(arm: ContainerSlideEnd) -> BattleConfig {
    let mut cfg = config();
    cfg.card_level = [LEVEL, LEVEL];
    let c = &cfg.calib;
    assert_eq!(c.death_slide_aim, DeathSlideAim::FixedEndPoint, "the shipped spawner.DEATH_SLIDE_AIM");
    assert_eq!(c.death_slide_birth, DeathSlideBirth::StepTowardEnd, "the shipped spawner.DEATH_SLIDE_BIRTH");
    assert_eq!(c.death_spawn_pushback, DeathSpawnPushback::ClientRingSlide, "the shipped spawner.DEATH_SPAWN_PUSHBACK");
    cfg.calib.container_slide_end = arm;
    cfg
}

/// The container record the barrel's death leaves.
fn container_idx(s: &BattleState) -> u16 {
    let db = s.cards();
    db.get(db.index(BARREL).expect("the Skeleton Barrel loads")).death_spawn.expect("its death spawn").unit
}

/// The point the container stands on, while it waits out its fuse.
fn container_point(s: &BattleState) -> Option<Vec2> {
    let c = container_idx(s);
    s.spells().iter().find(|sp| sp.card == c).map(|sp| match &sp.motion {
        SpellMotion::Flight { aim, .. } => *aim,
        m => panic!("the container is not a Flight: {m:?}"),
    })
}

/// A `team` Skeleton Barrel put at `p` (native) and killed on the next tick. Returns T, the tick it died on, and C, the
/// point its container stands on.
fn kill_barrel(s: &mut BattleState, team: Team, p: (i32, i32)) -> (u32, Vec2) {
    let id = s.scenario_spawn_now(team, BARREL, at(p), None).expect("the barrel is placed");
    assert!(s.debug_set_hp(id, 0));
    let t = s.tick_count();
    s.tick();
    assert!(s.entity(id).is_none(), "the barrel died on its tick");
    let c = container_point(s).expect("the barrel's death left its container");
    (t, c)
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

/// One ring under one arm: C, the arena's width (subtiles), and member j's (point, slide end, still sliding) after each
/// tick from T + 12 to T + 20.
struct Ring {
    c: Vec2,
    width: i32,
    track: Vec<Vec<(Vec2, Vec2, bool)>>,
}

impl Ring {
    /// Member j's point after T + k.
    fn pos(&self, j: usize, k: u32) -> Vec2 {
        self.track[j][(k - 12) as usize].0
    }

    /// Member j's slide end (entity.rs `death_slide_end`), fixed at its birth.
    fn end(&self, j: usize) -> Vec2 {
        self.track[j][0].1
    }

    /// Is member j still sliding after T + k?
    fn sliding(&self, j: usize, k: u32) -> bool {
        self.track[j][(k - 12) as usize].2
    }
}

fn ring(team: Team, p: (i32, i32), arm: ContainerSlideEnd) -> Ring {
    let mut s = BattleState::new(9, config_with(arm));
    let (t, c) = kill_barrel(&mut s, team, p);
    after(&mut s, t + 12);
    let m = members(&s, team);
    assert_eq!(m.len(), 7, "{team:?} {arm:?}: the seven Skeletons on T + 12");
    let mut track: Vec<Vec<(Vec2, Vec2, bool)>> = vec![Vec::new(); m.len()];
    for k in 12..=20 {
        after(&mut s, t + k);
        for (tr, id) in track.iter_mut().zip(m.iter()) {
            let v = s.entity(*id).expect("a Skeleton");
            tr.push((v.pos, v.death_slide_end, v.death_slide_radius > 0));
        }
    }
    Ring { c, width: s.arena().width, track }
}

/// The edge scene for `team`'s barrel killed on `p`: `sign` -1 beside the left edge, 1 beside the right.
fn check(team: Team, p: (i32, i32), sign: i32) {
    let old = ring(team, p, ContainerSlideEnd::RingPoint);
    let new = ring(team, p, ContainerSlideEnd::Client16635ArenaInset250);
    assert_eq!(old.c, new.c, "scene: the two arms' barrels burst on one point");
    let (c, width) = (new.c, new.width);
    let inset = CONTAINER_END_INSET * K;
    // A point's distance from the edge, subtiles.
    let gap = |v: Vec2| if sign < 0 { v.x } else { width - v.x };
    let toward = |r: &Ring, j: usize| sign * (r.pos(j, 12).x - c.x);
    let e = (0..7).max_by_key(|&j| toward(&new, j)).expect("seven members");
    assert_eq!(e, (0..7).max_by_key(|&j| toward(&old, j)).expect("seven members"), "scene: one edge member under both arms");

    // (1) the on-axis member beside the edge
    for r in [&old, &new] {
        let o = native(r.pos(e, 12).sub(c));
        assert!((o.0 - sign * move16402::DEATH_SLIDE_START).abs() <= 1 && o.1.abs() <= 1, "{team:?}: the edge member born on {o:?} from C");
    }
    let birth = gap(new.pos(e, 12)) / K;
    assert_eq!((birth - CONTAINER_END_INSET) / move16402::DEATH_SLIDE_STEP, 2, "scene: the edge member is born {birth} from the edge");
    assert!(gap(old.end(e)) < 0, "scene: under ring_point the edge member's ring point lies past the edge");
    assert_eq!(gap(new.end(e)), inset, "{team:?} client16635_arena_inset_250: the edge member's slide ends 250 inside the edge");
    assert_eq!(new.end(e).y, old.end(e).y, "{team:?}: the inset keeps an end's coordinate that lies inside");
    assert!(new.sliding(e, 13) && !new.sliding(e, 14), "{team:?} client16635_arena_inset_250: the edge member slides 2 ticks");
    assert!(old.sliding(e, 15) && !old.sliding(e, 16), "{team:?} ring_point (the vacuity check): the edge member slides 4 ticks");
    for k in 13..=14 {
        assert!(gap(new.pos(e, k)) < gap(new.pos(e, k - 1)) - 100 * K, "{team:?} client16635_arena_inset_250: no slide step on T + {k}");
    }
    // A contact push after the slide may still move it a little (client 16.635, b32f9aae t789: 41 toward the edge).
    let floor = CONTAINER_END_INSET - 150;
    for k in 15..=20 {
        let g = gap(new.pos(e, k)) / K;
        assert!(g > floor, "{team:?} client16635_arena_inset_250: the edge member {g} from the edge on T + {k}");
    }
    let g = gap(old.pos(e, 16)) / K;
    assert!(g < floor, "{team:?} ring_point: the edge member {g} from the edge on T + 16, short of the arena's own clamp");

    // (2) the members whose ring points lie inside the inset
    let inside: Vec<usize> = (0..7).filter(|&j| gap(old.end(j)) >= inset).collect();
    assert!(inside.len() >= 3, "scene: the members with their ring points inside the inset: {inside:?}");
    for &j in &inside {
        assert_eq!(new.end(j), old.end(j), "{team:?} member {j}: a ring point inside the inset is moved");
        for r in [&old, &new] {
            assert!(r.sliding(j, 15) && !r.sliding(j, 16), "{team:?} member {j}: a ring point inside the inset slides other than 4 ticks");
        }
    }

    // (3) the diagonal members whose ring points lie past the inset
    let past: Vec<usize> = (0..7).filter(|&j| j != e && gap(old.end(j)) < inset).collect();
    assert!(!past.is_empty(), "scene: no diagonal member with its ring point past the inset");
    for &j in &past {
        let end = new.end(j);
        assert_eq!((gap(end), end.y), (inset, old.end(j).y), "{team:?} member {j}: its ring point is not kept 250 inside the edge");
        let ((x, y), _) = move16402::death_slide_toward((0, 0), ((end.x - c.x) / K, (end.y - c.y) / K), move16402::DEATH_SLIDE_START);
        let want = c.add(Vec2::new(x * K, y * K));
        let got = new.pos(j, 12);
        assert!(
            (got.x - want.x).abs() <= K && (got.y - want.y).abs() <= K,
            "{team:?} member {j}: born on {:?} from C, one step toward its inset end is {:?}",
            native(got.sub(c)),
            native(want.sub(c))
        );
        let o = old.pos(j, 12);
        assert!((o.x - got.x).abs() > 2 * K || (o.y - got.y).abs() > 2 * K, "{team:?} ring_point (the vacuity check): member {j} born on the same point");
    }
}

#[test]
fn a_ring_member_beside_the_side_edge_stops_inside_the_inset() {
    check(Team::Blue, P, -1);
}

#[test]
fn the_right_edge_mirrors() {
    let s = BattleState::new(9, config_with(ContainerSlideEnd::RingPoint));
    let (w, h) = (s.arena().width / K, s.arena().height / K);
    check(Team::Red, (w - P.0, h - P.1), 1);
}

/// Since parity r98 for clients 16.402 and 16.635 (ring_point before it; client 15.535.29 keeps ring_point).
#[test]
fn the_shipped_arm_is_client16635_arena_inset_250() {
    assert_eq!(
        royalesim::state::Calib::shipped().container_slide_end,
        royalesim::state::ContainerSlideEnd::Client16635ArenaInset250
    );
    assert_eq!(config().calib.container_slide_end, royalesim::state::ContainerSlideEnd::Client16635ArenaInset250);
}
