//! Pathfinding: three candidate models behind one trait, and exact movement.
//!
//! WHY THREE
//!     calibration.json pathfinding.ALGORITHM is a guess, and the version every
//!     public source documents is the one Supercell replaced on 2025-03-31:
//!       "Troops now rely much less on lanes. Previously, they would move
//!        horizontally to reach their lane before advancing. Now, they move
//!        diagonally."
//!       "Troops can now see Buildings and adjust their movement in advance
//!        instead of walking straight into them and only navigating around
//!        after a collision."
//!     So: LaneSnap is the pre-2025 behaviour (horizontal to lane, then advance;
//!     buildings handled only on contact), DiagonalLookahead is the post-2025
//!     reading (diagonal to the bridge, buildings avoided as soon as they are in
//!     sight), and GridAStar is the community-engine alternative. A measurement
//!     picks one; no model is privileged in code.
//!
//! NO COST TABLE
//!     pathfinding.PATHFINDING_COSTS is disputed_existence (two sources disagree
//!     by 114x on water). Water is hard-blocked for ground and ignored by air;
//!     A* uses plain octile step costs (10 straight / 14 diagonal) and nothing else.
//!
//! PLANNED IN THE TEAM'S FRAME
//!     Every planner receives its inputs already transformed into the moving
//!     team's frame (Red is ROTATED 180 degrees, so every team "attacks upward"
//!     and has its own-left at low x) and its output is transformed back. The
//!     arena is rotation-symmetric (arena.rs `is_rotation_symmetric`), so a Red
//!     unit's planning problem is bit-identical to its rotated Blue twin's, and so
//!     is the answer -- including A*'s heap tie-breaks (keyed on frame cell index)
//!     and every "lower x" / "right-hand side" choice below, which therefore mean
//!     the unit's OWN left / right. Symmetry by construction, not by auditing
//!     every comparison. Modelling Red as a y-mirror instead (the arena is also
//!     y-symmetric) makes every frame "lower x" an ENGINE-lower x for both seats,
//!     which is a seat bias under the rotation.
//!
//! RE-PATH CADENCE is calibration REPATH_INTERVAL_TICKS (a guess), applied by
//! the caller in state.rs; nothing here inlines it.
#![allow(unexpected_cfgs)]

use crate::arena::{Arena, Shape};
use crate::fixed::{isqrt, Vec2};
use crate::{EntityId, PathModel, Team};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// A building footprint as seen by planners.
#[derive(Clone, Copy, Debug)]
pub struct Obstacle {
    pub id: EntityId,
    pub shape: Shape,
    /// The building's CollisionRadius, SUBTILES. Kept beside `shape` because the
    /// 2026 occlusion box is the CollisionRadius box whatever
    /// collision.BUILDING_FOOTPRINT_MODEL says the footprint is -- the two are
    /// different questions and the measurement only settles the first
    /// (calibration pathfinding.OCCLUSION_MODEL).
    pub radius: i32,
    /// The building's seat-invariant identity (spawn tick, card, level, hp,
    /// team_seq) -- the same tuple as a troop's `YieldKey`.
    pub key: YieldKey,
    /// Owned by the team whose frame this obstacle list is in. Rotation-invariant
    /// (a Blue building seen by Blue <-> its Red twin seen by Red).
    pub ally: bool,
}

/// The world as one team sees it: obstacles already in that team's frame.
pub struct FrameWorld<'a> {
    pub arena: &'a Arena,
    pub obstacles: &'a [Obstacle],
}

/// One unit's navigation problem, in frame coordinates.
#[derive(Clone, Copy, Debug)]
pub struct NavRequest {
    /// PLANT ONLY (`reflection_bridge_tie`): whether the mover is Red. A planner
    /// must never know the team -- that is what makes it seat-symmetric.
    #[cfg(clash_plant = "reflection_bridge_tie")]
    pub red: bool,
    /// The mover's team. READ BY ONE PLANNER ONLY: the measured 16.402 search
    /// (path16402.rs, selected by calibration pathfinding.PATH_SEARCH) plans in
    /// ABSOLUTE arena coordinates because the game's scan and neighbour orders are
    /// absolute, so it un-rotates a Red request first. The frame-planned models
    /// never look at it; their seat symmetry is by construction as before.
    pub team: Team,
    pub pos: Vec2,
    pub goal: Vec2,
    pub radius: i32,
    pub sight: i32,
    /// Subtiles per tick.
    pub step: i32,
    /// 2026 model only: Range + the MOVER's own CollisionRadius, subtiles. The
    /// path is truncated at the first cell whose centre is within this of the
    /// target's centre POINT (calibration pathfinding.PATH_GOAL_RULE). Not the sum
    /// of both collision radii, and not the footprint edge -- both were measured
    /// and both are wrong.
    pub reach: i32,
    pub flying: bool,
    /// The TARGET flies (FlyingHeight > 0). READ BY THE 16.402 SEARCH ONLY: the
    /// goal-cell choice ANDs KS_POS_TO_TARGET_GROUND_AVOID_BUILDINGS with the target
    /// not flying, so a ground unit chasing a flying target takes the nearest
    /// in-reach cell whether or not a building box covers it.
    pub target_flying: bool,
    /// The mover is JumpEnabled (card.rs `JumpDef`). READ BY THE 16.402 SEARCH
    /// ONLY: its cost field prices water at WATER_COST instead of BLOCKED; the hop
    /// itself is the walk's (state.rs `phase_path16402`, jump16402.rs).
    pub jumper: bool,
    /// Obstacle to ignore -- the building the unit is walking up to attack.
    pub ignore: Option<EntityId>,
}

pub trait Pathfinder {
    /// Plan waypoints from `req.pos` to `req.goal` (frame coordinates). The last
    /// waypoint is the goal.
    fn plan(&self, world: &FrameWorld, req: &NavRequest) -> Vec<Vec2>;

    /// Per-tick steering toward `waypoint`. Default: head straight for it.
    fn steer(&self, _world: &FrameWorld, _req: &NavRequest, waypoint: Vec2) -> Vec2 {
        waypoint
    }
}

pub struct LaneSnap;
pub struct GridAStar;
pub struct DiagonalLookahead;

pub fn pathfinder_for(model: PathModel) -> &'static dyn Pathfinder {
    #[cfg(clash_plant = "lanesnap_is_diagonal")]
    {
        // PLANT: collapse the pre-2025 model onto the post-2025 one.
        let _ = model;
        return &DiagonalLookahead;
    }
    #[allow(unreachable_code)]
    match model {
        PathModel::LaneSnap => &LaneSnap,
        PathModel::GridAStar => &GridAStar,
        PathModel::DiagonalLookahead => &DiagonalLookahead,
        // The 2026 model's route is GOAL-FIRST and its locomotion law is not this
        // trait's (no `steer`, no `advance`, no fractional carry). It has its own
        // per-tick loop in state.rs and never reaches the generic one; the arm
        // exists so the match is total.
        PathModel::Oracle2026 => &crate::path2026::Oracle2026,
    }
}

// ---------------------------------------------------------------------------
// exact movement

/// Fractional carry resolution: 1/65536 subtile.
const FRAC: i128 = 1 << 16;

/// Move from `from` toward `to` by `amount` subtiles.
///
/// Returns (new position, unused amount). Arriving lands EXACTLY on `to` and
/// returns the leftover so the caller can spend it on the next waypoint. Off
/// axis, each component is computed at 1/65536-subtile precision and the
/// sub-subtile remainder is carried in `frac` into the next tick, so a unit
/// walking a diagonal for a minute ends within a subtile of where exact
/// arithmetic puts it, instead of losing up to a subtile per axis per tick.
/// All divisions are on RELATIVE vectors and truncate toward zero, which is
/// odd-symmetric, so the mirror of a move is the move of the mirror.
pub fn advance(from: Vec2, to: Vec2, amount: i32, frac: &mut Vec2) -> (Vec2, i32) {
    let d = to.sub(from);
    let d2 = d.len2();
    if d2 == 0 {
        *frac = Vec2::default();
        return (to, amount);
    }
    // len scaled by 256 for precision: isqrt(d2 * 65536) = len * 256.
    let len256 = isqrt(d2 << 16).max(1) as i128;
    let amt256 = (amount as i128) << 8;
    if amt256 >= len256 {
        *frac = Vec2::default();
        let used = (len256 >> 8) as i32;
        return (to, (amount - used).max(0));
    }
    #[cfg(clash_plant = "truncate_drift")]
    {
        // PLANT: per-tick truncation, no carry.
        let _ = (amt256, len256);
        return (from.step_toward(to, amount), 0);
    }
    #[allow(unreachable_code)]
    {
        // component in 1/FRAC subtiles: d * amount / len
        let nx = (d.x as i128) * amt256 * FRAC / len256 + frac.x as i128;
        let ny = (d.y as i128) * amt256 * FRAC / len256 + frac.y as i128;
        frac.x = (nx % FRAC) as i32;
        frac.y = (ny % FRAC) as i32;
        (Vec2::new(from.x + (nx / FRAC) as i32, from.y + (ny / FRAC) as i32), 0)
    }
}

// ---------------------------------------------------------------------------
// shared geometry helpers (frame coordinates throughout)

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Home,
    River,
    Enemy,
}

fn side(arena: &Arena, y: i32) -> Side {
    if y < arena.water_y_min {
        Side::Home
    } else if y > arena.water_y_max {
        Side::Enemy
    } else {
        Side::River
    }
}

/// Clearance beyond touching that detours keep from a building: half the unit's
/// radius. A GEOMETRIC choice, not a game constant -- zero clearance makes the
/// detour point graze the footprint and the collision pass then fights it.
#[inline]
fn clearance(radius: i32) -> i32 {
    radius / 2
}

/// Does `p` (disc radius r) sit inside some obstacle other than `ignore`?
fn inside_obstacle(world: &FrameWorld, p: Vec2, r: i32, ignore: Option<EntityId>) -> bool {
    world.obstacles.iter().any(|o| Some(o.id) != ignore && o.shape.penetrates(p, r))
}

/// Does the straight walk a->b cross water? Sampled at a quarter half-tile.
fn crosses_water(arena: &Arena, a: Vec2, b: Vec2) -> bool {
    let len = a.dist(b);
    let step = (arena.cell / 4).max(1);
    let n = len / step + 1;
    (0..=n).any(|k| {
        let p = Vec2::new(a.x + crate::fixed::mul_div(b.x - a.x, k, n), a.y + crate::fixed::mul_div(b.y - a.y, k, n));
        arena.is_water(p)
    })
}

/// The ordered obstacle key used whenever several obstacles compete. It is a
/// TOTAL order on distinct obstacles and every component is rotation-invariant, so
/// the winner never depends on the order of `world.obstacles` (slot order, which
/// differs between seats once slots are reused):
///   distance from `from`, frame centre x, frame centre y (the geometry);
///   bound radius (what `detour_point` reads besides the centre);
///   the building's `YieldKey` (spawn tick, card, level, hp, team_seq);
///   ally before enemy.
/// Two distinct buildings cannot share all of it: equal `ally` means one team, and
/// team_seq is unique within a team.
/// Geometry alone (dist2, centre x, centre y) is NOT a total order: two buildings
/// stacked on one centre -- a `MatchSetup` can place them -- tie, and `min_by_key`
/// then returns the first in SLOT order, the same seat-dependent fallback as the
/// troop tie in `avoid_units`. Plant: slot_order_obstacle_tie.
fn obstacle_key(from: Vec2, o: &Obstacle) -> (i64, i32, i32, i32, YieldKey, bool) {
    let c = o.shape.center();
    #[cfg(clash_plant = "slot_order_obstacle_tie")]
    {
        // PLANT (regression): geometry only; a stacked tie falls to list order.
        return (from.dist2(c), c.x, c.y, 0, (0, 0, 0, 0, 0), false);
    }
    #[allow(unreachable_code)]
    (from.dist2(c), c.x, c.y, o.shape.bound_radius(), o.key, !o.ally)
}

/// Nearest obstacle (not `ignore`) the segment a->b passes within `inflate` of,
/// considering only obstacles whose centre is within `horizon` of a.
fn first_blocker<'w>(
    world: &'w FrameWorld,
    a: Vec2,
    b: Vec2,
    inflate: i32,
    horizon: i32,
    ignore: Option<EntityId>,
) -> Option<&'w Obstacle> {
    world
        .obstacles
        .iter()
        .filter(|o| Some(o.id) != ignore)
        .filter(|o| {
            let reach = (horizon as i64) + (o.shape.bound_radius() as i64);
            a.dist2(o.shape.center()) <= reach * reach
        })
        .filter(|o| o.shape.segment_hits(a, b, inflate))
        // min_by_key returns the FIRST minimum; obstacle_key is total, so there is
        // only ever one.
        .min_by_key(|o| obstacle_key(a, o))
}

/// A waypoint beside obstacle `o` that walks around it on the side the
/// obstacle is NOT on. Head-on (exactly collinear) goes to the side nearer the
/// arena's centre line, measured in the frame: distance to the centre line is
/// rotation-invariant and the frame's right-hand side is the unit's own right, so
/// this cannot favour a seat. ("x is mirror-invariant" is true of a y-reflection
/// only, and is not a reason that survives the rotation.)
fn detour_point(world: &FrameWorld, from: Vec2, to: Vec2, o: &Obstacle, radius: i32) -> Vec2 {
    let arena = world.arena;
    let c = o.shape.center();
    let reach = (o.shape.bound_radius() + radius + clearance(radius)) as i64;
    let dir = to.sub(from);
    let len = (dir.len() as i64).max(1);
    let rel = c.sub(from);
    let cross = (dir.x as i64) * (rel.y as i64) - (dir.y as i64) * (rel.x as i64);
    // right-hand normal (dir.y, -dir.x); left-hand (-dir.y, dir.x)
    let right = Vec2::new(c.x + ((dir.y as i64) * reach / len) as i32, c.y + ((-(dir.x as i64)) * reach / len) as i32);
    let left = Vec2::new(c.x + ((-(dir.y as i64)) * reach / len) as i32, c.y + ((dir.x as i64) * reach / len) as i32);
    let mid = arena.width / 2;
    let (first, second) = if cross > 0 {
        (right, left)
    } else if cross < 0 {
        (left, right)
    } else if (right.x - mid).abs() <= (left.x - mid).abs() {
        (right, left)
    } else {
        (left, right)
    };
    let ok = |p: Vec2| arena.is_passable_ground(p);
    if ok(first) {
        first
    } else if ok(second) {
        second
    } else {
        first
    }
}

/// Bridge gate points for crossing from `from` toward `goal` over bridge `bx`.
fn gates(arena: &Arena, bx: i32, from: Vec2, goal: Vec2) -> (Vec2, Vec2) {
    let lo = Vec2::new(bx, arena.water_y_min - arena.cell);
    let hi = Vec2::new(bx, arena.water_y_max + arena.cell);
    if goal.y >= from.y {
        (lo, hi)
    } else {
        (hi, lo)
    }
}

fn needs_crossing(arena: &Arena, from: Vec2, goal: Vec2) -> bool {
    let (a, b) = (side(arena, from.y), side(arena, goal.y));
    if a == b && a != Side::River {
        return false;
    }
    crosses_water(arena, from, goal)
}

/// Drop waypoints a unit could never stand on (inside a building).
fn prune(world: &FrameWorld, req: &NavRequest, mut wps: Vec<Vec2>) -> Vec<Vec2> {
    let last = wps.pop();
    wps.retain(|p| !inside_obstacle(world, *p, req.radius, req.ignore));
    if let Some(g) = last {
        wps.push(g);
    }
    wps
}

// ---------------------------------------------------------------------------
// LaneSnap: pre-2025

impl Pathfinder for LaneSnap {
    fn plan(&self, world: &FrameWorld, req: &NavRequest) -> Vec<Vec2> {
        let arena = world.arena;
        if req.flying || !needs_crossing(arena, req.pos, req.goal) {
            return vec![req.goal];
        }
        let bx = arena.bridge(arena.lane_by_x(req.pos.x)).center_x;
        let (near, far) = gates(arena, bx, req.pos, req.goal);
        let mut wps = Vec::with_capacity(4);
        if side(arena, req.pos.y) != Side::River {
            // "move horizontally to reach their lane before advancing"
            wps.push(Vec2::new(bx, req.pos.y));
            wps.push(near);
        }
        wps.push(far);
        wps.push(req.goal);
        prune(world, req, wps)
    }

    /// Buildings are handled only on contact: a detour is taken when the next
    /// two ticks of travel would run into a footprint.
    fn steer(&self, world: &FrameWorld, req: &NavRequest, waypoint: Vec2) -> Vec2 {
        if req.flying {
            return waypoint;
        }
        let dir = waypoint.sub(req.pos);
        let len = dir.len();
        if len == 0 {
            return waypoint;
        }
        let probe = (req.step * 2 + clearance(req.radius)).min(len);
        let tip = Vec2::new(
            req.pos.x + crate::fixed::mul_div(dir.x, probe, len),
            req.pos.y + crate::fixed::mul_div(dir.y, probe, len),
        );
        match first_blocker(world, req.pos, tip, req.radius, probe + req.radius, req.ignore) {
            Some(o) => detour_point(world, req.pos, waypoint, o, req.radius),
            None => waypoint,
        }
    }
}

// ---------------------------------------------------------------------------
// local unit avoidance (every model)

/// Seat-invariant ordering used ONLY when two units meet exactly colinearly:
/// (spawn tick, card index, level, hp, team_seq). Nothing in it depends on team or
/// slot index; team_seq (spawn ordinal within the unit's own team) is equal for a
/// unit and its rotated twin and differs between formation siblings.
/// (spawn tick, card, level, hp) alone is not enough: formation siblings from one
/// deploy share all four, which is why team_seq is in the key.
pub type YieldKey = (u32, u16, i32, i32, u32);

/// A troop another troop may have to walk around, in the MOVER's frame.
#[derive(Clone, Copy, Debug)]
pub struct UnitBlocker {
    pub pos: Vec2,
    pub radius: i32,
    pub key: YieldKey,
    /// Same team as the MOVER. Rotation-invariant; it is the last tie-break
    /// component (see `blocker_key`).
    pub ally: bool,
}

/// The order in which `avoid_units` picks the blocker to walk around. A TOTAL
/// order on distinct blockers, every component rotation-invariant:
///   distance from the mover, frame x, frame y (the geometry);
///   radius (what the detour reads besides position and key);
///   the blocker's `YieldKey`;
///   ally before enemy.
/// Two distinct blockers cannot share all of it: equal `YieldKey` needs two teams
/// (team_seq is unique within a team), and then `ally` differs. Equal keys on one
/// point also mean the same card and so the same radius, so for that last case the
/// detour is identical whichever is picked; `ally` is there so the order is total
/// by construction rather than by that argument.
///
/// Geometry alone (dist2, frame x, frame y) is NOT a total order: two blockers
/// stacked on one exact point tie, and the scan then keeps whichever came first in
/// the neighbour list, i.e. SLOT order, which differs between seats once slots are
/// reused. It is rare but reachable -- about one random game in a thousand, when a
/// freshly deployed Skeleton lands exactly on a Skeleton Army unit and the mover's
/// key falls between theirs, so "which blocker" decides "which side".
/// Plant: slot_order_blocker_tie. Tests: tests/stacked_tie.rs.
pub type BlockerKey = (i64, i32, i32, i32, YieldKey, bool);

#[inline]
fn blocker_key(mover: Vec2, b: &UnitBlocker) -> BlockerKey {
    #[cfg(clash_plant = "slot_order_blocker_tie")]
    {
        // PLANT (regression): geometry only; a stacked tie falls to slot order.
        return (mover.dist2(b.pos), b.pos.x, b.pos.y, 0, (0, 0, 0, 0, 0), false);
    }
    #[allow(unreachable_code)]
    (mover.dist2(b.pos), b.pos.x, b.pos.y, b.radius, b.key, !b.ally)
}

/// Local avoidance of other troops, applied after a model's `steer`.
///
/// WHY IT EXISTS: every model funnels lane traffic onto EXACTLY the bridge centre
/// x (LaneSnap's lane waypoint and all gate points are `bridge.center_x`), and the
/// separation pass pushes an exactly colinear pair purely along y. Without a
/// local-avoidance step two opposing Giants that meet on a bridge stand there for
/// the rest of the battle, and a Giant (mass 18) bulldozes a Hog Rider (mass 4)
/// nine tiles back across the river. calibration pathfinding.ALGORITHM is
/// "lane_flow_with_local_avoidance"; this is the second half of that name.
///
/// THE RULE (UNVERIFIED -- a scenario to measure on client 15.535.29): if the next
/// `2 * step + clearance` of travel would bring the mover's disc into a troop
/// that is AHEAD of it (not its target; the caller filters), aim for a point
/// beside that troop instead, on the side it is not on. Exactly colinear
/// (no side to prefer): the lower `YieldKey` goes toward the arena's centre line,
/// the higher away from it, so two different units pick opposite sides and pass.
/// Equal keys both go toward the centre (ties on the centre line itself go to the
/// frame's right-hand side, i.e. each unit's own right).
///
/// WHO CAN HAVE EQUAL KEYS. Not only exact mirror twins: same-team formation
/// siblings share (spawn tick, card, level, hp), which is why team_seq is part of
/// the key. With it, equal keys need two units of DIFFERENT teams with the same
/// spawn tick, card, level, hp and team_seq. Under the 180-degree seat rotation a
/// unit's twin is on the OTHER bridge, so a head-on meeting on one bridge is not a
/// symmetric configuration and no symmetry forbids breaking it; it is broken or not
/// by this rule's geometry (tests/mirror.rs `twin_giants_on_one_bridge`
/// measures which).
///
/// Everything is computed in the mover's frame, so rotated twins decide identically.
pub fn avoid_units(world: &FrameWorld, req: &NavRequest, my_key: YieldKey, aim: Vec2, blockers: &[UnitBlocker]) -> Vec2 {
    #[cfg(clash_plant = "no_unit_avoidance")]
    {
        // PLANT: the pre-fix engine.
        let _ = (world, req, my_key, blockers);
        return aim;
    }
    #[allow(unreachable_code)]
    {
        let dir = aim.sub(req.pos);
        let len = dir.len();
        if len == 0 || blockers.is_empty() {
            return aim;
        }
        let probe = (req.step * 2 + clearance(req.radius)).min(len);
        let tip = Vec2::new(
            req.pos.x + crate::fixed::mul_div(dir.x, probe, len),
            req.pos.y + crate::fixed::mul_div(dir.y, probe, len),
        );
        // Strict `<` keeps the first of equal keys, so the key must be total
        // (`blocker_key`): with it, the list order cannot reach the result.
        let mut best: Option<(BlockerKey, &UnitBlocker)> = None;
        for b in blockers {
            let rel = b.pos.sub(req.pos);
            let dot = (dir.x as i64) * (rel.x as i64) + (dir.y as i64) * (rel.y as i64);
            if dot <= 0 {
                continue;
            }
            if !(Shape::Circle { c: b.pos, r: b.radius }).segment_hits(req.pos, tip, req.radius) {
                continue;
            }
            let k = blocker_key(req.pos, b);
            if best.as_ref().map_or(true, |(bk, _)| k < *bk) {
                best = Some((k, b));
            }
        }
        let Some((_, b)) = best else { return aim };
        let reach = (b.radius + req.radius + clearance(req.radius)) as i64;
        let l = len as i64;
        let right = Vec2::new(b.pos.x + ((dir.y as i64) * reach / l) as i32, b.pos.y + ((-(dir.x as i64)) * reach / l) as i32);
        let left = Vec2::new(b.pos.x + ((-(dir.y as i64)) * reach / l) as i32, b.pos.y + ((dir.x as i64) * reach / l) as i32);
        let rel = b.pos.sub(req.pos);
        let cross = (dir.x as i64) * (rel.y as i64) - (dir.y as i64) * (rel.x as i64);
        let (first, second) = if cross > 0 {
            (right, left)
        } else if cross < 0 {
            (left, right)
        } else {
            let mid = world.arena.width / 2;
            let (central, outer) = if (right.x - mid).abs() <= (left.x - mid).abs() { (right, left) } else { (left, right) };
            if my_key > b.key {
                (outer, central)
            } else {
                (central, outer)
            }
        };
        if req.flying || world.arena.is_passable_ground(first) {
            first
        } else if world.arena.is_passable_ground(second) {
            second
        } else {
            aim
        }
    }
}

// ---------------------------------------------------------------------------
// DiagonalLookahead: post-2025

/// Look-ahead horizon for buildings: the unit's own sight range. "Troops can
/// now see Buildings" -- so the distance at which they react is tied to sight
/// rather than to a new invented constant. UNVERIFIED.
fn lookahead_horizon(req: &NavRequest) -> i32 {
    req.sight
}

const MAX_DETOURS_PER_LEG: usize = 3;

impl Pathfinder for DiagonalLookahead {
    fn plan(&self, world: &FrameWorld, req: &NavRequest) -> Vec<Vec2> {
        let arena = world.arena;
        if req.flying {
            return vec![req.goal];
        }
        let mut legs: Vec<Vec2> = Vec::with_capacity(3);
        if needs_crossing(arena, req.pos, req.goal) {
            // Diagonal to whichever bridge makes the whole trip shortest. Ties go
            // to the lower FRAME-x bridge: the unit's own-left (a unit standing
            // exactly on x = W/2 facing a goal on the centre line). Breaking the
            // tie on ENGINE x instead sends both seats to the engine-left bridge.
            let mut best: Option<(i64, usize)> = None;
            for (bi, b) in arena.bridges.iter().enumerate() {
                let (near, far) = gates(arena, b.center_x, req.pos, req.goal);
                let on_river = side(arena, req.pos.y) == Side::River;
                let cost = if on_river {
                    (req.pos.dist(far) + far.dist(req.goal)) as i64
                } else {
                    (req.pos.dist(near) + near.dist(far) + far.dist(req.goal)) as i64
                };
                #[cfg(not(clash_plant = "reflection_bridge_tie"))]
                let better = best.map_or(true, |(bc, _)| cost < bc);
                #[cfg(clash_plant = "reflection_bridge_tie")]
                let better = best.map_or(true, |(bc, _)| cost < bc || (cost == bc && req.red)); // PLANT: engine-left for Red too
                if better {
                    best = Some((cost, bi));
                }
            }
            let b = arena.bridges[best.map(|(_, i)| i).unwrap_or(0)];
            let (near, far) = gates(arena, b.center_x, req.pos, req.goal);
            if side(arena, req.pos.y) != Side::River {
                legs.push(near);
            }
            legs.push(far);
        }
        legs.push(req.goal);

        // Look ahead along each leg and bend around buildings before touching them.
        let horizon = lookahead_horizon(req);
        let inflate = req.radius + clearance(req.radius);
        let mut out = Vec::with_capacity(legs.len() + 2);
        let mut cur = req.pos;
        for wp in legs {
            let mut detours = 0;
            while detours < MAX_DETOURS_PER_LEG {
                let travelled = req.pos.dist(cur);
                if travelled >= horizon {
                    break;
                }
                match first_blocker(world, cur, wp, inflate, horizon - travelled, req.ignore) {
                    Some(o) => {
                        let d = detour_point(world, cur, wp, o, req.radius);
                        if d == cur {
                            break;
                        }
                        out.push(d);
                        cur = d;
                        detours += 1;
                    }
                    None => break,
                }
            }
            out.push(wp);
            cur = wp;
        }
        prune(world, req, out)
    }
}

// ---------------------------------------------------------------------------
// GridAStar

const COST_STRAIGHT: i32 = 10;
const COST_DIAGONAL: i32 = 14;

fn octile(dx: i32, dy: i32) -> i32 {
    let (a, b) = (dx.abs(), dy.abs());
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    COST_DIAGONAL * lo + COST_STRAIGHT * (hi - lo)
}

impl GridAStar {
    fn blocked_grid(world: &FrameWorld, req: &NavRequest) -> Vec<bool> {
        let a = world.arena;
        let mut g = vec![false; (a.cols * a.rows) as usize];
        for row in 0..a.rows {
            for col in 0..a.cols {
                if !a.is_passable_ground(a.half_to_subtile_center(col, row)) {
                    g[(row * a.cols + col) as usize] = true;
                }
            }
        }
        for o in world.obstacles.iter().filter(|o| Some(o.id) != req.ignore) {
            let c = o.shape.center();
            let reach = o.shape.bound_radius() + req.radius + a.cell;
            let (c0, r0) = a.subtile_to_half(Vec2::new(c.x - reach, c.y - reach));
            let (c1, r1) = a.subtile_to_half(Vec2::new(c.x + reach, c.y + reach));
            for row in r0.max(0)..=r1.min(a.rows - 1) {
                for col in c0.max(0)..=c1.min(a.cols - 1) {
                    if o.shape.penetrates(a.half_to_subtile_center(col, row), req.radius) {
                        g[(row * a.cols + col) as usize] = true;
                    }
                }
            }
        }
        g
    }

    fn line_clear(world: &FrameWorld, blocked: &[bool], a: Vec2, b: Vec2) -> bool {
        let ar = world.arena;
        let len = a.dist(b);
        let step = (ar.cell / 4).max(1);
        let n = len / step + 1;
        (0..=n).all(|k| {
            let p = Vec2::new(a.x + crate::fixed::mul_div(b.x - a.x, k, n), a.y + crate::fixed::mul_div(b.y - a.y, k, n));
            let (col, row) = ar.subtile_to_half(p);
            let cell_ok = col < 0
                || row < 0
                || col >= ar.cols
                || row >= ar.rows
                || !blocked[(row * ar.cols + col) as usize];
            cell_ok && ar.is_passable_ground(p)
        })
    }
}

impl Pathfinder for GridAStar {
    fn plan(&self, world: &FrameWorld, req: &NavRequest) -> Vec<Vec2> {
        let a = world.arena;
        if req.flying {
            return vec![req.goal];
        }
        let blocked = Self::blocked_grid(world, req);
        let clamp_cell = |p: Vec2| {
            let (c, r) = a.subtile_to_half(p);
            (c.clamp(0, a.cols - 1), r.clamp(0, a.rows - 1))
        };
        let (sc, sr) = clamp_cell(req.pos);
        let (gc, gr) = clamp_cell(req.goal);
        let idx = |c: i32, r: i32| (r * a.cols + c) as usize;
        let start = idx(sc, sr);
        let goal = idx(gc, gr);
        if start == goal || Self::line_clear(world, &blocked, req.pos, req.goal) {
            return vec![req.goal];
        }
        let n = (a.cols * a.rows) as usize;
        let mut gcost = vec![i32::MAX; n];
        let mut came = vec![u32::MAX; n];
        let mut closed = vec![false; n];
        let mut open = BinaryHeap::new();
        gcost[start] = 0;
        open.push(Reverse((octile(gc - sc, gr - sr), octile(gc - sc, gr - sr), start as u32)));
        let passable = |i: usize| i == start || i == goal || !blocked[i];
        let mut found = false;
        while let Some(Reverse((_, _, cur))) = open.pop() {
            let cur = cur as usize;
            if closed[cur] {
                continue;
            }
            if cur == goal {
                found = true;
                break;
            }
            closed[cur] = true;
            let (cc, cr) = ((cur as i32) % a.cols, (cur as i32) / a.cols);
            for dr in -1..=1 {
                for dc in -1..=1 {
                    if dr == 0 && dc == 0 {
                        continue;
                    }
                    let (nc, nr) = (cc + dc, cr + dr);
                    if nc < 0 || nr < 0 || nc >= a.cols || nr >= a.rows {
                        continue;
                    }
                    let ni = idx(nc, nr);
                    if closed[ni] || !passable(ni) {
                        continue;
                    }
                    let diag = dc != 0 && dr != 0;
                    if diag && (!passable(idx(cc + dc, cr)) || !passable(idx(cc, cr + dr))) {
                        continue; // no corner cutting past a blocked cell
                    }
                    let ng = gcost[cur] + if diag { COST_DIAGONAL } else { COST_STRAIGHT };
                    if ng < gcost[ni] {
                        gcost[ni] = ng;
                        came[ni] = cur as u32;
                        let h = octile(gc - nc, gr - nr);
                        open.push(Reverse((ng + h, h, ni as u32)));
                    }
                }
            }
        }
        if !found {
            return vec![req.goal];
        }
        let mut cells = Vec::new();
        let mut c = goal;
        while c != start {
            cells.push(c);
            c = came[c] as usize;
        }
        cells.reverse();
        let mut pts: Vec<Vec2> = cells
            .iter()
            .map(|&i| a.half_to_subtile_center((i as i32) % a.cols, (i as i32) / a.cols))
            .collect();
        if let Some(last) = pts.last_mut() {
            *last = req.goal;
        }
        // String-pull: from each anchor, jump to the farthest visible point.
        let mut out = Vec::new();
        let mut anchor = req.pos;
        let mut i = 0;
        while i < pts.len() {
            let mut j = pts.len() - 1;
            while j > i && !Self::line_clear(world, &blocked, anchor, pts[j]) {
                j -= 1;
            }
            out.push(pts[j]);
            anchor = pts[j];
            i = j + 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixed::tiles;

    #[test]
    fn advance_diagonal_has_no_truncation_drift() {
        // 3-4-5 direction with a step whose per-tick components are FRACTIONAL
        // (731 * 3/5 = 438.6, 731 * 4/5 = 584.8), so per-tick truncation loses
        // 0.6 / 0.8 subtile every tick. The step size matters: at amount 900
        // (Speed 60 at 15 subtiles/tick) the components 540 and 720 are exact
        // integers, nothing is ever truncated, and the `truncate_drift` plant does
        // not land -- the test would pass vacuously. Plant: truncate_drift.
        let from = Vec2::new(0, 0);
        let to = Vec2::new(tiles(12), tiles(16)); // 20 tiles away
        let amount = 731;
        let mut frac = Vec2::default();
        let mut p = from;
        let ticks = 333;
        for _ in 0..ticks {
            p = advance(p, to, amount, &mut frac).0;
        }
        let exact = Vec2::new(amount * ticks * 3 / 5, amount * ticks * 4 / 5);
        assert!((p.x - exact.x).abs() <= 2 && (p.y - exact.y).abs() <= 2, "p={p:?} exact={exact:?}");
    }

    #[test]
    fn advance_arrives_exactly_with_leftover() {
        let mut frac = Vec2::default();
        let (p, left) = advance(Vec2::new(0, 0), Vec2::new(300, 400), 900, &mut frac);
        assert_eq!(p, Vec2::new(300, 400));
        assert_eq!(left, 400);
    }

    #[test]
    fn advance_is_odd_symmetric() {
        let mut fa = Vec2::default();
        let mut fb = Vec2::default();
        let mut a = Vec2::new(1000, 1000);
        let mut b = Vec2::new(1000, -1000);
        for _ in 0..50 {
            a = advance(a, Vec2::new(77_777, 55_555), 731, &mut fa).0;
            b = advance(b, Vec2::new(77_777, -55_555), 731, &mut fb).0;
            assert_eq!(a.x, b.x);
            assert_eq!(a.y, -b.y);
        }
    }

    fn req_at(pos: Vec2, radius: i32, step: i32) -> NavRequest {
        NavRequest {
            #[cfg(clash_plant = "reflection_bridge_tie")]
            red: false,
            team: Team::Blue,
            pos,
            goal: Vec2::new(tiles(3) + tiles(1) / 2, pos.y),
            radius,
            sight: tiles(5),
            step,
            reach: 0,
            flying: false,
            target_flying: false,
            jumper: false,
            ignore: None,
        }
    }

    #[test]
    fn stacked_unit_blockers_are_chosen_independently_of_list_order() {
        // Two blockers on ONE exact point, colinear ahead of the mover, with the
        // mover's YieldKey between theirs: which one is picked decides the side.
        // The pick must not depend on the slice order (= slot order in the engine).
        // Plant: slot_order_blocker_tie. Engine-level twin: tests/stacked_tie.rs.
        let arena = Arena::shipped();
        let world = FrameWorld { arena: &arena, obstacles: &[] };
        let (r, step) = (9000, 900);
        let req = req_at(Vec2::new(tiles(8), tiles(12)), r, step);
        let p = Vec2::new(req.pos.x - (2 * r + step), req.pos.y);
        let my_key: YieldKey = (0, 5, 9, 500, 3);
        let lo = UnitBlocker { pos: p, radius: r, key: (0, 5, 9, 250, 4), ally: true };
        let hi = UnitBlocker { pos: p, radius: r, key: (0, 5, 9, 750, 5), ally: true };
        let aim = req.goal;
        let only_lo = avoid_units(&world, &req, my_key, aim, &[lo]);
        let only_hi = avoid_units(&world, &req, my_key, aim, &[hi]);
        assert_ne!(only_lo, only_hi, "vacuous: the two blockers send the mover the same way");
        assert_ne!(only_lo, aim, "vacuous: the avoidance did not fire");
        let ab = avoid_units(&world, &req, my_key, aim, &[lo, hi]);
        let ba = avoid_units(&world, &req, my_key, aim, &[hi, lo]);
        assert_eq!(ab, ba, "blocker list order decided the side-step");
        // Equal YieldKeys on one point (a unit and its rotated twin, say): distinct
        // only by `ally`, and the answer is the same whichever is taken.
        let mine = UnitBlocker { pos: p, radius: r, key: (0, 5, 9, 250, 4), ally: true };
        let theirs = UnitBlocker { ally: false, ..mine };
        assert_eq!(blocker_key(req.pos, &mine).cmp(&blocker_key(req.pos, &theirs)), std::cmp::Ordering::Less);
        assert_eq!(avoid_units(&world, &req, my_key, aim, &[mine, theirs]), avoid_units(&world, &req, my_key, aim, &[theirs, mine]));
    }

    #[test]
    fn stacked_obstacles_are_chosen_independently_of_list_order() {
        // Two building footprints on ONE centre with different radii (so a
        // different detour reach). first_blocker must return the same one for either
        // list order. Plant: slot_order_obstacle_tie.
        let arena = Arena::shipped();
        let (r, step) = (9000, 900);
        let req = req_at(Vec2::new(tiles(8), tiles(12)), r, step);
        let c = Vec2::new(req.pos.x - (10800 + r + step), req.pos.y);
        let big = Obstacle {
            id: EntityId { index: 7, generation: 0 },
            shape: Shape::Circle { c, r: 10800 },
            radius: 10800,
            key: (0, 1, 9, 350, 4),
            ally: true,
        };
        let small =
            Obstacle { id: EntityId { index: 2, generation: 0 }, shape: Shape::Circle { c, r: 9000 }, radius: 9000, key: (0, 2, 9, 450, 5), ally: true };
        let tip = Vec2::new(req.pos.x - (2 * step + r / 2), req.pos.y);
        let horizon = 2 * step + r / 2 + r;
        let pick = |obs: &[Obstacle]| {
            let world = FrameWorld { arena: &arena, obstacles: obs };
            let o = first_blocker(&world, req.pos, tip, r, horizon, None).map(|o| o.id);
            let d = first_blocker(&world, req.pos, tip, r, horizon, None).map(|o| detour_point(&world, req.pos, req.goal, o, r));
            (o, d)
        };
        let (only_big, only_small) = (pick(&[big]), pick(&[small]));
        assert!(only_big.0.is_some() && only_small.0.is_some(), "vacuous: the probe does not reach both footprints");
        assert_ne!(only_big.1, only_small.1, "vacuous: both footprints give the same detour");
        assert_eq!(pick(&[big, small]), pick(&[small, big]), "obstacle list order decided the detour");
    }

    #[test]
    fn octile_costs() {
        assert_eq!(octile(3, 0), 30);
        assert_eq!(octile(3, 3), 42);
        assert_eq!(octile(-2, 5), 28 + 30);
    }
}
