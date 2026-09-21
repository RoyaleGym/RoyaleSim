//! Unit separation and static blocking.
//!
//! WHY THIS MATTERS
//!     An engine with no unit collision cannot model body-blocking, swarm spread
//!     or a tank leading its support -- three of the mechanics a Clash Royale
//!     policy has to learn. This module is what makes them exist.
//!
//! ORDER INDEPENDENCE
//!     Every displacement in a pass is computed from the positions at the START
//!     of that pass and written into a buffer; the buffer is applied once. A unit
//!     processed first gets no advantage because nothing it does is visible to
//!     anyone else until the pass ends. Same for buildings: the push-outs from
//!     all footprints are computed from one base position and summed, so a unit
//!     wedged between two towers is not resolved differently depending on which
//!     tower has the lower slot index.
//!
//! PUSH MODELS (calibration collision.PUSH_MODEL, status guess)
//!     The share of a pair's overlap that unit i absorbs:
//!       MassWeighted   m_j / (m_i + m_j)   -- heavier units yield less
//!       SpeedWeighted  s_j / (s_i + s_j)   -- faster units yield less. The
//!                      DIRECTION of that weighting is itself unverified; the
//!                      public reimplementation that weights by speed does not
//!                      document it.
//!       EqualSplit     1 / 2
//!     When a model's inputs are missing or both zero, the pair splits equally.
//!     calibration also lists "mass_times_speed"; lib.rs::PushModel has no such
//!     variant, so it is not implemented here.
//!
//! WATER
//!     A ground unit whose resolved position touches water keeps as much of the
//!     move as stays dry (x-only, then y-only), else stays put. It is never
//!     pushed onto water.
#![allow(unexpected_cfgs)]

use crate::arena::Arena;
use crate::entity::{EntityKind, Entities, SpatialHash};
use crate::fixed::{isqrt, Vec2};
use crate::path::Obstacle;
use crate::PushModel;

/// Fraction (num, den) of the pair overlap that unit i absorbs.
#[inline]
fn share(push: PushModel, ents: &Entities, i: usize, j: usize) -> (i64, i64) {
    let equal = (1, 2);
    match push {
        #[cfg(not(clash_plant = "equal_split_uses_mass"))]
        PushModel::EqualSplit => equal,
        #[cfg(clash_plant = "equal_split_uses_mass")]
        PushModel::EqualSplit => share(PushModel::MassWeighted, ents, i, j),
        #[cfg(not(clash_plant = "mass_share_inverted"))]
        PushModel::MassWeighted => match (ents.mass[i], ents.mass[j]) {
            (Some(mi), Some(mj)) if mi + mj > 0 => (mj as i64, (mi + mj) as i64),
            _ => equal,
        },
        #[cfg(clash_plant = "mass_share_inverted")]
        PushModel::MassWeighted => match (ents.mass[i], ents.mass[j]) {
            (Some(mi), Some(mj)) if mi + mj > 0 => (mi as i64, (mi + mj) as i64),
            _ => equal,
        },
        PushModel::SpeedWeighted => {
            let (si, sj) = (ents.speed[i] as i64, ents.speed[j] as i64);
            if si + sj > 0 {
                (sj, si + sj)
            } else {
                equal
            }
        }
    }
}

/// Is entity i a unit that takes part in unit-unit separation?
#[inline]
fn is_mobile_unit(ents: &Entities, i: usize) -> bool {
    ents.kind[i] == EntityKind::Troop
}

/// Keep a ground unit off water: prefer the full move, then each axis alone.
#[inline]
pub fn dry_position(arena: &Arena, old: Vec2, new: Vec2) -> Vec2 {
    #[cfg(clash_plant = "no_water_block")]
    {
        // PLANT: accept any position.
        let _ = (arena, old);
        return new;
    }
    #[allow(unreachable_code)]
    {
        if arena.is_passable_ground(new) {
            return new;
        }
        let xo = Vec2::new(new.x, old.y);
        if arena.is_passable_ground(xo) {
            return xo;
        }
        let yo = Vec2::new(old.x, new.y);
        if arena.is_passable_ground(yo) {
            return yo;
        }
        old
    }
}

#[inline]
fn clamp_to_arena(arena: &Arena, p: Vec2) -> Vec2 {
    Vec2::new(p.x.clamp(0, arena.width), p.y.clamp(0, arena.height))
}

/// Scratch buffers reused across ticks (not simulation state).
#[derive(Default, Clone, Debug)]
pub struct CollideScratch {
    disp: Vec<Vec2>,
    nb: Vec<u32>,
}

/// Resolve overlaps: `iterations` unit-unit passes, then one static pass
/// against building footprints and water. Rebuilds `hash` as it goes and
/// leaves it current.
pub fn separate(
    ents: &mut Entities,
    hash: &mut SpatialHash,
    arena: &Arena,
    obstacles: &[Obstacle],
    push: PushModel,
    iterations: i32,
    scratch: &mut CollideScratch,
) {
    let cap = ents.capacity();
    scratch.disp.clear();
    scratch.disp.resize(cap, Vec2::default());

    #[cfg(clash_plant = "no_collision")]
    let iterations = {
        let _ = iterations;
        0
    };

    for _ in 0..iterations.max(0) {
        hash.rebuild(ents);
        for d in scratch.disp.iter_mut() {
            *d = Vec2::default();
        }
        let max_r = hash.max_radius();
        for i in 0..cap {
            if !ents.alive[i] || !is_mobile_unit(ents, i) {
                continue;
            }
            let pi = ents.pos[i];
            let ri = ents.radius[i];
            hash.neighbours_within(ents, pi, ri + max_r, &mut scratch.nb);
            let mut acc = Vec2::default();
            for &j in scratch.nb.iter() {
                let j = j as usize;
                if j == i || !is_mobile_unit(ents, j) || ents.flying[i] != ents.flying[j] {
                    continue;
                }
                let need = (ri + ents.radius[j]) as i64;
                let d = pi.sub(ents.pos[j]);
                let d2 = d.len2();
                if d2 >= need * need {
                    continue;
                }
                let (num, den) = share(push, ents, i, j);
                if d2 == 0 {
                    // Coincident: no direction to push along. Decided in each
                    // unit's OWN frame, so every branch is antisymmetric in the
                    // pair and commutes with the 180-degree seat rotation:
                    //   different teams: each goes toward its own side (y);
                    //   same team: the lower team_seq goes to the team's own-left,
                    //   the higher to its own-right (team_seq is unique per team).
                    // Resolving it in ENGINE x for both seats instead (lower
                    // team_seq -x, higher +x, equal team_seq along y) is a seat
                    // bias: a Blue sibling pair splits one way and its rotated Red
                    // twin pair the other way round, and a cross-team pair with
                    // different team_seq is pushed the SAME engine direction after
                    // rotation.
                    let mv = (need * num / den) as i32;
                    let (si, sj) = (ents.team_seq[i], ents.team_seq[j]);
                    let (ti, tj) = (ents.team[i], ents.team[j]);
                    #[cfg(not(clash_plant = "reflection_push_tie"))]
                    {
                        if ti != tj {
                            acc.y += Arena::own_side_dy(ti) * mv;
                        } else {
                            let left = Arena::own_left_dx(ti);
                            acc.x += if si < sj { left * mv } else { -left * mv };
                        }
                    }
                    #[cfg(clash_plant = "reflection_push_tie")]
                    {
                        // PLANT (regression): the pre-ruling engine-x split.
                        let _ = (ti, tj);
                        if si < sj {
                            acc.x -= mv;
                        } else if si > sj {
                            acc.x += mv;
                        } else {
                            acc.y += Arena::own_side_dy(ti) * mv;
                        }
                    }
                    continue;
                }
                let len = isqrt(d2).max(1);
                let overlap = need - len;
                let mv = overlap * num / den;
                acc.x += ((d.x as i64) * mv / len) as i32;
                acc.y += ((d.y as i64) * mv / len) as i32;
            }
            scratch.disp[i] = acc;
            #[cfg(clash_plant = "sequential_separation")]
            {
                // PLANT: apply each unit's push immediately, so later units see
                // earlier units' new positions -- slot order decides.
                let old = ents.pos[i];
                ents.pos[i] = clamp_to_arena(arena, old.add(acc));
                scratch.disp[i] = Vec2::default();
            }
        }
        for i in 0..cap {
            if !ents.alive[i] || !is_mobile_unit(ents, i) {
                continue;
            }
            let old = ents.pos[i];
            let moved = clamp_to_arena(arena, old.add(scratch.disp[i]));
            ents.pos[i] = if ents.flying[i] { moved } else { dry_position(arena, old, moved) };
        }
    }

    // Static pass: buildings and towers block ground units.
    #[cfg(clash_plant = "no_building_block")]
    let cap = {
        // PLANT: skip the static pass, so footprints block nothing.
        let _ = cap;
        0
    };
    for i in 0..cap {
        if !ents.alive[i] || !is_mobile_unit(ents, i) || ents.flying[i] {
            continue;
        }
        let p = ents.pos[i];
        let r = ents.radius[i];
        let team = ents.team[i];
        let mut total = Vec2::default();
        for o in obstacles {
            if let Some(q) = o.shape.push_out(p, r, team) {
                total = total.add(q.sub(p));
            }
        }
        if total != Vec2::default() {
            let moved = clamp_to_arena(arena, p.add(total));
            ents.pos[i] = dry_position(arena, p, moved);
        }
    }
    hash.rebuild(ents);
}
