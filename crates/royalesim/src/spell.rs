//! Spells in flight and on the ground: fixed-target projectiles (Fireball, Arrows,
//! Goblin Barrel), one-shot area effects (Zap), rolling projectiles (The Log), and the
//! two effects they share -- KNOCKBACK and STUN.
//!
//! SPEC: docs/spell-spec.md (PARTIALLY VERIFIED; its header explains the vintage
//! trap). Every card number comes from data/derived/cards.json through card.rs
//! `SpellDef`; every mechanic no column settles is a calibration.json key (spells.*,
//! knockback.*, status.*), read into `Calib`. Nothing here types a card stat.
//!
//! LIFECYCLE (lib.rs TICK_PHASES)
//!     deploy      validated like a troop (state.rs check_position), elixir paid,
//!                 card cycled, a PendingSpawn queued -- so a cast has exactly a
//!                 troop's latency and both seats' same-tick casts materialise in
//!                 the same Spawn phase.
//!     Spawn       `cast` turns it into `Spell` objects (one per Arrows wave).
//!     Projectile  `step_spells`, after troop projectiles: flights advance, rolls
//!                 roll, area effects apply. Every hit test reads entity positions
//!                 AS THEY ARE ON THE ARRIVAL TICK (post-Move), never at cast. Hits go
//!                 to the DamageBuffer; knockbacks and stuns to their own buffers;
//!                 released units to the deferred spawn queue.
//!     Resolve     damage first; then (state.rs) stun timers tick, new stuns merge by
//!                 max, knockbacks sum per unit and move the survivors.
//!     The one-shot area effect runs INSIDE the Projectile phase rather than in a new
//!     Phase::AreaEffect (the spec's aoe.PHASE_POSITION "after projectile, before
//!     resolve"): the observable order is identical, because nothing in a phase reads
//!     another writer's buffer before Resolve, and TICK_PHASES stays unchanged.
//!
//! ORDER INDEPENDENCE AND THE SEAT ROTATION, BY CONSTRUCTION
//!     * A spell reads only entity state (which does not change during the phase) and
//!       its own record, and writes only to buffers. So `spells` order cannot matter.
//!     * Knockbacks are SUMMED per unit and stuns MERGED BY MAX before anything moves
//!       (calibration knockback.STACKING, status.SAME_BUFF_REAPPLY); both commute.
//!     * Directions are world-frame RELATIVE vectors scaled by integer division that
//!       truncates toward zero -- odd-symmetric, so the rotation of a push is the push
//!       of the rotation. Every fallback direction is the CASTER's forward axis (+y
//!       for Blue, -y for Red), never a fixed engine axis.
//!     * Flights advance with path::advance in world coordinates, whose sub-subtile
//!       carry flips sign under the rotation (tests/common canon_team knows).
//!     * Water ejection breaks ties in the pushed unit's own frame
//!       (Arena::nearest_passable_ground).
#![allow(unexpected_cfgs)]

use crate::arena::{Arena, Rect, Shape};
use crate::card::{CardDb, KnockbackDef, SpellHit, SpellShape};
use crate::combat::{damage_against, DamageBuffer, Hit};
use crate::entity::{EntityKind, Entities, SpatialHash};
use crate::fixed::{in_range_edge, isqrt, Vec2};
use crate::path::{advance, Obstacle};
use crate::state::{AoeHitTest, Calib, KnockZeroVector, LaunchModel, RollDirection, RollHitShape};
use crate::{EntityId, Team};

/// Where a spell is in its life.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SpellMotion {
    /// A fixed-target projectile. `delay_ms` > 0: a later wave not yet released.
    Flight { pos: Vec2, aim: Vec2, frac: Vec2, delay_ms: i32 },
    /// The Log's airborne phase: no hitbox. On arrival it becomes `Rolling` at
    /// `roll_start` with `roll_len` to go.
    Airborne { pos: Vec2, aim: Vec2, frac: Vec2, roll_start: Vec2, roll_len: i32 },
    /// A rolling projectile moving along its caster's forward axis. `hit` holds every
    /// entity it has already struck (each at most once), sorted.
    Rolling { pos: Vec2, travelled: i32, len: i32, hit: Vec<EntityId> },
    /// A one-shot area effect at `pos`, applied on its first update.
    Area { pos: Vec2 },
}

/// One live spell object.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Spell {
    pub team: Team,
    /// The spell card's CardDb index.
    pub card: u16,
    /// Unified level it was cast at.
    pub level: i32,
    /// Level-scaled damage per victim (0 when it deals none).
    pub damage: i32,
    pub motion: SpellMotion,
}

/// A unit a landing spell releases, for the deferred spawn queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Release {
    pub team: Team,
    pub unit: u16,
    pub level: i32,
    pub pos: Vec2,
    pub deploy_ms: Option<i32>,
    pub count: i32,
}

/// Everything a tick of spells produced, applied later by state.rs.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EffectBuffer {
    /// Knockback displacements, WORLD subtiles; summed per unit in Resolve.
    pub knocks: Vec<(EntityId, Vec2)>,
    /// Stun durations, ms; merged by max per unit in Resolve.
    pub stuns: Vec<(EntityId, i32)>,
}

/// The caster's forward axis: +1 for Blue (toward high y), -1 for Red.
#[inline]
pub fn forward_dy(team: Team) -> i32 {
    -Arena::own_side_dy(team)
}

/// Turn one accepted cast into its spell objects. Pure; `level` already validated.
pub fn cast(cards: &CardDb, calib: &Calib, arena: &Arena, team: Team, card: u16, level: i32, tap: Vec2) -> Result<Vec<Spell>, String> {
    let def = cards.get(card);
    let spell = def.spell.as_ref().ok_or_else(|| format!("{} is not a spell", def.name))?;
    #[cfg(not(clash_plant = "spell_damage_unscaled"))]
    let scaled = |h: &SpellHit| cards.scaled(card, level, h.damage);
    #[cfg(clash_plant = "spell_damage_unscaled")]
    let scaled = |h: &SpellHit| cards.scaled(card, level, h.damage).map(|_| h.damage); // PLANT: level-1 damage at every level.
    let mut out = Vec::new();
    match &spell.shape {
        SpellShape::Projectile { hit, waves, wave_interval_ms, .. } => {
            let damage = match hit {
                Some(h) => scaled(h)?,
                None => 0,
            };
            // calibration spells.LAUNCH_POINT = caster_king_tower_centre (the only
            // implemented candidate; Calib refuses the others).
            #[cfg(not(clash_plant = "spell_launch_from_tap"))]
            let launch = arena.king_tower_pos(team);
            #[cfg(clash_plant = "spell_launch_from_tap")]
            let launch = tap; // PLANT: no flight -- the impact lands on the cast tick.
            for w in 0..*waves {
                #[cfg(not(clash_plant = "waves_simultaneous"))]
                let delay_ms = w * wave_interval_ms;
                #[cfg(clash_plant = "waves_simultaneous")]
                let delay_ms = {
                    let _ = (w, wave_interval_ms); // PLANT: every wave released at once.
                    0
                };
                out.push(Spell { team, card, level, damage, motion: SpellMotion::Flight { pos: launch, aim: tap, frac: Vec2::default(), delay_ms } });
            }
        }
        SpellShape::AreaEffect { hit } => {
            out.push(Spell { team, card, level, damage: scaled(hit)?, motion: SpellMotion::Area { pos: tap } });
        }
        SpellShape::Rolling { airborne_min_distance, range, hit, .. } => {
            let damage = scaled(hit)?;
            let fwd = forward_dy(team);
            let ahead = |d: i32| Vec2::new(tap.x, tap.y + fwd * d);
            #[cfg(not(clash_plant = "airborne_skipped"))]
            let launch = calib.spell_as_deploy_launch;
            #[cfg(clash_plant = "airborne_skipped")]
            let launch = LaunchModel::InstantRollAtTap; // PLANT: no airborne phase whatever the registry says.
            #[cfg(not(clash_plant = "roll_range_from_airborne_start"))]
            let behind_len = *range;
            #[cfg(clash_plant = "roll_range_from_airborne_start")]
            let behind_len = range - airborne_min_distance; // PLANT: the roll ends ProjectileRange past the airborne START.
            let motion = match launch {
                LaunchModel::AirborneFromBehind => SpellMotion::Airborne {
                    pos: ahead(-airborne_min_distance),
                    aim: tap,
                    frac: Vec2::default(),
                    roll_start: tap,
                    roll_len: behind_len,
                },
                LaunchModel::AirborneFromTapForward => SpellMotion::Airborne {
                    pos: tap,
                    aim: ahead(*airborne_min_distance),
                    frac: Vec2::default(),
                    roll_start: ahead(*airborne_min_distance),
                    roll_len: (range - airborne_min_distance).max(0),
                },
                LaunchModel::InstantRollAtTap => SpellMotion::Rolling { pos: tap, travelled: 0, len: *range, hit: Vec::new() },
            };
            out.push(Spell { team, card, level, damage, motion });
        }
    }
    Ok(out)
}

/// Read-only world a spell step needs.
pub struct SpellCtx<'a> {
    pub ents: &'a Entities,
    pub hash: &'a SpatialHash,
    pub cards: &'a CardDb,
    pub calib: &'a Calib,
}

/// Is victim `v` a legal target of `hit` cast by `team`? Alive, hp > 0, team and
/// air/ground filters, building filters. Deploying units ARE victims (spec: every
/// family).
#[inline]
fn eligible(ents: &Entities, v: usize, team: Team, hit: &SpellHit) -> bool {
    if !ents.alive[v] || ents.hp[v] <= 0 {
        return false;
    }
    #[cfg(not(clash_plant = "spell_friendly_fire"))]
    if hit.only_enemies && ents.team[v] == team {
        return false;
    }
    #[cfg(clash_plant = "spell_friendly_fire")]
    let _ = (team, hit.only_enemies); // PLANT: OnlyEnemies ignored.
    #[cfg(clash_plant = "spells_never_hit_air")]
    if ents.flying[v] {
        return false; // PLANT: AoeToAir / HitsAir ignored.
    }
    let kind = ents.kind[v];
    if hit.ignore_buildings && kind != EntityKind::Troop {
        return false;
    }
    if hit.no_effect_to_crown_towers && kind.is_crown_tower() {
        return false;
    }
    if ents.flying[v] {
        hit.hits_air
    } else {
        hit.hits_ground
    }
}

/// A knockback vector of length `k.distance` along `d` (world, relative), or along the
/// fallback when `d` is zero. Exact to well under a subtile: the length is taken at
/// 1/256-subtile precision, like path::advance, so a short `d` cannot inflate the push.
#[inline]
fn push_along(d: Vec2, distance: i32, fallback: Option<Vec2>) -> Option<Vec2> {
    let d2 = d.len2();
    if d2 == 0 {
        return fallback;
    }
    let len256 = isqrt(d2 << 16).max(1) as i128;
    let s = (distance as i128) << 8;
    Some(Vec2::new(((d.x as i128) * s / len256) as i32, ((d.y as i128) * s / len256) as i32))
}

/// Should victim `v` be displaced by knockback `k`? Troops only (buildings and crown
/// towers have no Speed column and are never moved); IgnorePushback unless PushbackAll;
/// deploying units per knockback.AFFECTS_DEPLOYING_UNITS.
#[inline]
fn pushable(ctx: &SpellCtx, v: usize, k: &KnockbackDef) -> bool {
    let e = ctx.ents;
    if e.kind[v] != EntityKind::Troop {
        #[cfg(not(clash_plant = "push_buildings"))]
        return false;
    }
    #[cfg(not(clash_plant = "ignore_flag_not_read"))]
    if !k.all && ctx.cards.get(e.card[v]).ignore_pushback {
        return false;
    }
    #[cfg(clash_plant = "ignore_flag_not_read")]
    let _ = k.all; // PLANT: IgnorePushback never consulted.
    #[cfg(clash_plant = "pushback_respects_ignore")]
    if ctx.cards.get(e.card[v]).ignore_pushback {
        return false; // PLANT: PushbackAll no longer overrides IgnorePushback.
    }
    !(e.deploy_ms[v] > 0 && !ctx.calib.knock_affects_deploying)
}

/// Apply one circular impact of `hit` at `centre` for `team`. `damage` is level-scaled.
#[allow(clippy::too_many_arguments)]
fn impact(ctx: &SpellCtx, team: Team, centre: Vec2, hit: &SpellHit, damage: i32, dmg: &mut DamageBuffer, fx: &mut EffectBuffer, nb: &mut Vec<u32>) {
    let e = ctx.ents;
    ctx.hash.neighbours_within(e, centre, hit.radius + ctx.hash.max_radius(), nb);
    let fallback = match ctx.calib.knock_zero_vector {
        #[cfg(not(clash_plant = "zero_vector_plus_y"))]
        KnockZeroVector::CasterForward => Some(Vec2::new(0, forward_dy(team))),
        #[cfg(clash_plant = "zero_vector_plus_y")]
        KnockZeroVector::CasterForward => Some(Vec2::new(0, 1)), // PLANT: crforge's fixed +y for both seats.
        KnockZeroVector::NoPush => None,
    };
    for &v in nb.iter() {
        let v = v as usize;
        if !eligible(e, v, team, hit) {
            continue;
        }
        let edge = match ctx.calib.aoe_hit_test {
            AoeHitTest::EdgeInclusive => e.radius[v],
            AoeHitTest::CentreInRadius => 0,
        };
        #[cfg(clash_plant = "aoe_centre_to_centre")]
        let edge = {
            let _ = edge; // PLANT: centre-in-radius whatever the registry says.
            0
        };
        if !in_range_edge(centre, e.pos[v], hit.radius, edge) {
            continue;
        }
        let id = e.id_of(v);
        if damage > 0 {
            #[cfg(not(clash_plant = "crown_pct_ignored"))]
            let pct = hit.crown_pct;
            #[cfg(clash_plant = "crown_pct_ignored")]
            let pct = 100; // PLANT: crown towers take full spell damage.
            dmg.hits.push(Hit { target: id, amount: damage_against(e.kind[v], damage, pct, ctx.calib.crown_rounding), ignores_hide: false });
        }
        if hit.stun_ms > 0 {
            fx.stuns.push((id, hit.stun_ms));
        }
        if let Some(k) = hit.knockback {
            #[cfg(clash_plant = "no_knockback")]
            let _ = k; // PLANT: nothing is ever pushed.
            #[cfg(not(clash_plant = "no_knockback"))]
            if pushable(ctx, v, &k) {
                // Unit direction scaled up first: the fallback is a unit axis.
                let dir = push_along(e.pos[v].sub(centre), k.distance, fallback.map(|f| Vec2::new(f.x * k.distance, f.y * k.distance)));
                if let Some(d) = dir {
                    fx.knocks.push((id, d));
                }
            }
        }
    }
}

/// One tick of a rolling projectile: move, sweep, hit each new victim once.
/// Returns true while it still has distance to roll.
#[allow(clippy::too_many_arguments)]
fn roll(ctx: &SpellCtx, team: Team, card: u16, damage: i32, pos: &mut Vec2, travelled: &mut i32, len: i32, hit_set: &mut Vec<EntityId>, dmg: &mut DamageBuffer, fx: &mut EffectBuffer, nb: &mut Vec<u32>) -> bool {
    let Some(crate::card::SpellDef { shape: SpellShape::Rolling { speed, half_width, half_depth, hit, .. }, .. }) = &ctx.cards.get(card).spell else {
        return false;
    };
    let (speed, half_width, half_depth) = (*speed, *half_width, *half_depth);
    let e = ctx.ents;
    let fwd = forward_dy(team);
    let step = (speed * ctx.calib.projectile_speed_to_subtiles_per_tick).min(len - *travelled).max(0);
    let prev = *pos;
    #[cfg(not(clash_plant = "rolling_forward_plus_y_for_both"))]
    let cur = Vec2::new(prev.x, prev.y + fwd * step);
    #[cfg(clash_plant = "rolling_forward_plus_y_for_both")]
    let cur = Vec2::new(prev.x, prev.y + step); // PLANT: rolls toward +y for both seats.
    *pos = cur;
    *travelled += step;
    // The swept rectangle: the roll's cross-section from its previous to its current
    // centre, extended by the half-depth fore and aft. A closed Rect, so a victim
    // touching it is hit (spells.ROLLING_HIT_SHAPE) -- and swept, so the result does
    // not depend on TICK_MS.
    let rect = Rect {
        min: Vec2::new(cur.x - half_width, prev.y.min(cur.y) - half_depth),
        max: Vec2::new(cur.x + half_width, prev.y.max(cur.y) + half_depth),
    };
    let shape = Shape::Box(rect);
    let reach = half_width.max(half_depth + step) + ctx.hash.max_radius();
    let mid = Vec2::new(cur.x, (prev.y + cur.y) / 2);
    ctx.hash.neighbours_within(e, mid, reach + half_width + half_depth, nb);
    for &v in nb.iter() {
        let v = v as usize;
        #[cfg(not(clash_plant = "rolling_hits_air"))]
        let ok = eligible(e, v, team, hit);
        #[cfg(clash_plant = "rolling_hits_air")]
        let ok = eligible(e, v, team, &SpellHit { hits_air: true, ..*hit }); // PLANT: AoeToAir blank read as TRUE.
        if !ok {
            continue;
        }
        let id = e.id_of(v);
        #[cfg(not(clash_plant = "rolling_rehit_every_tick"))]
        if hit_set.binary_search(&id).is_ok() {
            continue;
        }
        #[cfg(not(clash_plant = "rolling_centre_in_rect"))]
        let hit_shape = ctx.calib.rolling_hit_shape;
        #[cfg(clash_plant = "rolling_centre_in_rect")]
        let hit_shape = RollHitShape::RectContainsCentre; // PLANT: the victim's centre must be inside.
        let touched = match hit_shape {
            RollHitShape::RectVsCircleEdge => shape.covers_disc(e.pos[v], e.radius[v]),
            RollHitShape::RectContainsCentre => rect.contains_closed(e.pos[v]),
        };
        if !touched {
            continue;
        }
        // Kept sorted by (index, generation) so the set -- and so state_hash -- is a
        // function of which entities were hit, not of neighbour order.
        if let Err(at) = hit_set.binary_search(&id) {
            hit_set.insert(at, id);
        }
        #[cfg(not(clash_plant = "crown_pct_ignored"))]
        let pct = hit.crown_pct;
        #[cfg(clash_plant = "crown_pct_ignored")]
        let pct = 100; // PLANT: crown towers take full spell damage.
        dmg.hits.push(Hit { target: id, amount: damage_against(e.kind[v], damage, pct, ctx.calib.crown_rounding), ignores_hide: false });
        if let Some(k) = hit.knockback {
            if pushable(ctx, v, &k) {
                let along = Vec2::new(0, fwd * k.distance);
                // RADIAL FROM THE LOG'S CENTRE WHEN IT FIRST TOUCHED THIS VICTIM, not from
                // where the centre is at the end of the tick. The sweep exists so a hit
                // does not depend on TICK_MS; a direction taken from the tick-end centre
                // did: a unit standing on the tap when the log lands was pushed BACKWARD
                // (toward the caster), because the centre had already rolled one step past
                // it. The contact point along the roll is where the front face (centre +
                // half-depth) meets the victim's near edge, clamped to this tick's sweep.
                // Taking the direction from the tick-end centre instead
                // (`push_along(e.pos[v].sub(cur), ..)`) is what produces the
                // backward push; pinned by
                // log_pushes_forward_and_moves_ignore_pushback_units.
                let edge = match hit_shape {
                    RollHitShape::RectVsCircleEdge => e.radius[v],
                    RollHitShape::RectContainsCentre => 0,
                };
                let (a_prev, a_cur, a_v) = (prev.y * fwd, cur.y * fwd, e.pos[v].y * fwd);
                #[cfg(not(clash_plant = "rolling_push_from_tick_end"))]
                let contact = Vec2::new(cur.x, (a_v - half_depth - edge).clamp(a_prev.min(a_cur), a_prev.max(a_cur)) * fwd);
                #[cfg(clash_plant = "rolling_push_from_tick_end")]
                let contact = {
                    let _ = (a_prev, a_cur, a_v, edge); // PLANT (regression): the tick-end centre.
                    cur
                };
                let d = match ctx.calib.knock_direction_rolling {
                    // NOT SELECTED. Kept implemented because it is a listed candidate
                    // of knockback.DIRECTION_ROLLING and state.rs::pick refuses a
                    // candidate string with no engine implementation at load -- deleting
                    // this arm makes the registry unloadable, it does not tidy anything.
                    // It is also observably wrong: a radial push throws a victim at own
                    // along-offsets -1 to -(half_depth + victim radius) a full tile BACK
                    // toward the caster, in both seats.
                    RollDirection::RadialFromCentre => push_along(e.pos[v].sub(contact), k.distance, Some(along)),
                    // SELECTED. In the live game the Log's push is never backward, always
                    // forward: every victim it touches goes exactly Pushback along the
                    // caster's forward axis, with no sideways component, including one
                    // caught behind the tap by the landing log's back edge.
                    #[cfg(not(clash_plant = "rolling_push_radial_from_centre"))]
                    RollDirection::TravelDirection => Some(along),
                    // PLANT (regression): the radial push, put back.
                    #[cfg(clash_plant = "rolling_push_radial_from_centre")]
                    RollDirection::TravelDirection => push_along(e.pos[v].sub(contact), k.distance, Some(along)),
                };
                if let Some(d) = d {
                    fx.knocks.push((id, d));
                }
            }
        }
    }
    *travelled < len
}

/// Advance every spell by one tick. Units released by landing spells are returned in
/// `released`, in `spells` order (deterministic; team_seq is per team, and a team's
/// spells keep their cast order).
pub fn step_spells(ctx: &SpellCtx, spells: &mut Vec<Spell>, dmg: &mut DamageBuffer, fx: &mut EffectBuffer, released: &mut Vec<Release>, nb: &mut Vec<u32>) {
    let tick = ctx.calib.tick_ms;
    let mult = ctx.calib.projectile_speed_to_subtiles_per_tick;
    spells.retain_mut(|s| {
        let def = ctx.cards.get(s.card);
        let Some(sdef) = def.spell.as_ref() else { return false };
        match (&mut s.motion, &sdef.shape) {
            (SpellMotion::Flight { pos, aim, frac, delay_ms }, SpellShape::Projectile { speed, hit, spawn, .. }) => {
                if *delay_ms > 0 {
                    *delay_ms -= tick;
                    return true;
                }
                let (np, _) = advance(*pos, *aim, speed * mult, frac);
                *pos = np;
                if np != *aim {
                    return true;
                }
                if let Some(h) = hit {
                    impact(ctx, s.team, *aim, h, s.damage, dmg, fx, nb);
                }
                if let Some(sp) = spawn {
                    // Level validated when the cast was accepted (state.rs).
                    if let Ok(level) = ctx.cards.spawn_level(s.card, s.level) {
                        #[cfg(not(clash_plant = "spawn_uses_character_deploy_time"))]
                        let deploy_ms = sp.deploy_time_ms;
                        #[cfg(clash_plant = "spawn_uses_character_deploy_time")]
                        let deploy_ms = None; // PLANT: the unit's own DeployTime.
                        #[cfg(not(clash_plant = "spawn_count_one"))]
                        let count = sp.count;
                        #[cfg(clash_plant = "spawn_count_one")]
                        let count = 1; // PLANT: SpawnCharacterCount ignored.
                        released.push(Release { team: s.team, unit: sp.unit, level, pos: *aim, deploy_ms, count });
                    }
                }
                false
            }
            (SpellMotion::Area { pos }, SpellShape::AreaEffect { hit }) => {
                impact(ctx, s.team, *pos, hit, s.damage, dmg, fx, nb);
                false
            }
            (SpellMotion::Airborne { pos, aim, frac, roll_start, roll_len }, SpellShape::Rolling { airborne_speed, .. }) => {
                let (np, _) = advance(*pos, *aim, airborne_speed * mult, frac);
                *pos = np;
                if np != *aim {
                    return true;
                }
                // Landed: the roll starts here and runs its first step this tick.
                let (mut p, mut travelled, len, mut hit) = (*roll_start, 0, *roll_len, Vec::new());
                let more = roll(ctx, s.team, s.card, s.damage, &mut p, &mut travelled, len, &mut hit, dmg, fx, nb);
                s.motion = SpellMotion::Rolling { pos: p, travelled, len, hit };
                more
            }
            (SpellMotion::Rolling { pos, travelled, len, hit }, SpellShape::Rolling { .. }) => {
                roll(ctx, s.team, s.card, s.damage, pos, travelled, *len, hit, dmg, fx, nb)
            }
            _ => false,
        }
    });
}

/// Where a ground unit pushed from `old` toward `desired` comes to rest: clamped to
/// the arena, pushed out of building footprints as collision does, ejected from water
/// (knockback.WATER_RESOLUTION = eject_to_nearest_land), repeated until both hold. If
/// they cannot be satisfied together in a few rounds the unit stays at `old` -- which
/// the every-tick invariants already hold to be legal ground.
pub fn settle(arena: &Arena, obstacles: &[Obstacle], team: Team, radius: i32, flying: bool, old: Vec2, desired: Vec2) -> Vec2 {
    #[cfg(clash_plant = "knock_unclamped")]
    if !arena.in_bounds(desired) {
        // PLANT: a push may leave the arena. (Removing only the clamp below does NOT
        // land: the water ejection also returns in-bounds points, so the clamp is
        // defence in depth, not the only guard.)
        return desired;
    }
    let clamp = |p: Vec2| Vec2::new(p.x.clamp(0, arena.width), p.y.clamp(0, arena.height));
    let mut p = clamp(desired);
    if flying {
        return p;
    }
    #[cfg(clash_plant = "no_water_resolution")]
    {
        let _ = (obstacles, team, radius, old); // PLANT: the push ends wherever it ends.
        return p;
    }
    #[allow(unreachable_code)]
    for _ in 0..4 {
        let mut total = Vec2::default();
        #[cfg(clash_plant = "knock_ignores_footprints")]
        let obstacles: &[Obstacle] = &[]; // PLANT: pushes pass through buildings.
        for o in obstacles {
            if let Some(q) = o.shape.push_out(p, radius, team) {
                total = total.add(q.sub(p));
            }
        }
        if total != Vec2::default() {
            p = clamp(p.add(total));
        }
        if !arena.is_passable_ground(p) {
            match arena.nearest_passable_ground(p, team) {
                Some(q) => p = q,
                None => return old,
            }
        }
        if arena.is_passable_ground(p) && !obstacles.iter().any(|o| o.shape.penetrates(p, 0)) {
            return p;
        }
    }
    old
}
