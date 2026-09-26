//! The attack pipeline. The shipped cycle (calibration combat.ATTACK_CYCLE =
//! progress_credit, measured on the live 16.402 corpus: 1690 fresh target entries,
//! 911 first projectile launches): a progress counter that a fresh cycle enters at
//! LoadTime + 50 and that fires on every multiple of HitSpeed, so the FIRST hit
//! lands HitSpeed - LoadTime after the target came into range and every later one
//! HitSpeed apart; a load timer that runs LoadTime -> 0 from every hit (and every
//! entry) and whose remainder is taken off the credit of a re-entry; a swing
//! already under way finishes on a target that stepped out of range; a charged
//! unit's progress snaps to the next multiple, so its hit lands on the entry tick
//! (charge.CHARGED_HIT_TIMING). The earlier arm -- windup (load_time) -> hit ->
//! cooldown (rest of hit_speed) -- is runnable as windup_load_time
//! (`attack_step_windup`).
//!
//! THE DAMAGE BUFFER
//!     No attack, projectile or death effect ever subtracts hp. They append `Hit`s
//!     to a `DamageBuffer`; `resolve` applies the whole buffer in one pass. Two
//!     Knights that swing on the same tick therefore both land, whatever their slot
//!     order. Applying damage inline instead makes the earlier slot win every
//!     mirror trade, which is a systematic asymmetry rather than a rounding detail.
//!
//! TIMING
//!     Timers are kept in MILLISECONDS and advanced by TICK_MS, with the overshoot
//!     carried into the next phase. So hit_speed 1100 at 50 ms/tick fires on ticks
//!     22 apart on average rather than rounding every period to 22 or 23, and
//!     changing TICK_MS in calibration.json needs no change here. The tick on which
//!     a windup starts counts toward it (UNVERIFIED off-by-one vs the real game).
//!
//! ARITHMETIC (calibration combat.DAMAGE_ARITHMETIC: guess)
//!     Integer hp and damage. Crown-tower reduction rounding is calibration
//!     combat.CROWN_TOWER_DAMAGE_ROUNDING (`CrownRounding`), default ceil_kept_share:
//!     ceil(damage * pct / 100). The evidence for ceil over a plain truncating
//!     `damage * pct / 100` is 8/8 Supercell-published level-11 crown values
//!     against 3/8 for floor (docs/spell-spec.md). It matters only for percents
//!     below 100: spells, and the non-thin-slice Miner.
//!     Simultaneous hits on one target within a tick are summed BEFORE shields are
//!     considered, and the sum is one hit: a shield absorbs that whole tick's damage
//!     and breaks with no overflow into hitpoints. Applying hits one by one would
//!     make the result depend on the order hits are processed (shield 100, hits 150
//!     and 30: 150-first leaks 30 into hp, 30-first leaks nothing). Whether the real
//!     game carries overflow past a breaking shield is UNVERIFIED.
#![allow(unexpected_cfgs)]

use crate::card::{CardDb, CardDef, CustomShotDef, KnockbackDef};
use crate::entity::{AttackPhase, EntityKind, Entities, HideState, SpatialHash};
use crate::fixed::{cos_pi_frac, in_range_edge, isqrt, sin_pi_frac, Vec2, SUBTILE_PER_MILLITILE as K, TRIG_ONE};
use crate::path::{advance, advance_client};
use crate::state::{
    AttackCycle, Calib, ChargeLevelScaling, ChargedHitTiming, CustomFirstProjectile, HitSpeedBuff, MultipleProjectiles, ProjectileLaunch, ProjectileStep, RangeProjectile,
    TargetBuffScope,
};
use crate::spell::{forward_dy, push_from, EffectBuffer, SpellCtx};
use crate::status::{BuffApply, BuffHit, Sel};
use crate::target::in_attack_range;
use crate::{EntityId, Rng, Team};

const PERCENT: i64 = 100;

/// calibration.json combat.CROWN_TOWER_DAMAGE_ROUNDING.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum CrownRounding {
    /// ceil(damage * pct / 100) == damage + trunc0(damage * (pct - 100) / 100).
    CeilKeptShare,
    Floor,
    RoundHalfUp,
}

impl CrownRounding {
    pub fn from_calibration_name(s: &str) -> Option<Self> {
        match s {
            "ceil_kept_share" => Some(Self::CeilKeptShare),
            "floor" => Some(Self::Floor),
            "round_half_up" => Some(Self::RoundHalfUp),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Hit {
    pub target: EntityId,
    pub amount: i32,
    /// Lands on a HIDDEN building regardless of hide.HIDDEN_IMMUNE_TO_DAMAGE. Only
    /// the building-lifetime expiry hit (state.rs phase_status) sets it: a Tesla
    /// whose 40 s are up dies under ground. Every attack, projectile, spell and
    /// death-damage hit leaves it false and is dropped by `resolve` while the
    /// victim is Hidden. Snapshot format 6; absent in older snapshots = false.
    #[serde(default)]
    pub ignores_hide: bool,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct DamageBuffer {
    pub hits: Vec<Hit>,
}

/// An in-flight projectile: a homing shot, or a straight one (`straight`).
///
/// NOT `Copy`: a straight shot's hit set (`Straight::hit`), a Vec, rides on it.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Projectile {
    pub team: Team,
    pub pos: Vec2,
    pub target: EntityId,
    /// Last known target position; flown to if the target dies.
    pub aim: Vec2,
    /// Subtiles per tick.
    pub speed: i32,
    pub damage: i32,
    pub crown_pct: i32,
    pub splash: i32,
    pub hits_air: bool,
    pub hits_ground: bool,
    pub frac: Vec2,
    /// Born this tick (combat.PROJECTILE_LAUNCH = start_radius_next_tick): the
    /// Projectile phase of the fire tick skips it, its first step is the next
    /// tick's. Snapshot format 16; absent in older snapshots = false.
    #[serde(default)]
    pub fresh: bool,
    /// THE ATTACKER'S `attack_buff`, riding to the arrival tick (the Ice Spirit's
    /// Freeze, the Ice Wizard's slow): the buff lands where the damage lands, on the
    /// splash or on the one target (status.TARGET_BUFF_ON_SPLASH). None for every
    /// card without one. Snapshot format 18; absent in older snapshots = None.
    #[serde(default)]
    pub buff: Option<BuffApply>,
    /// The attacker's level-scaled per-pulse amount for that buff, 0 when it does not
    /// pulse. Snapshot format 18.
    #[serde(default)]
    pub pulse: i32,
    /// THE FIRER'S CARD (a `CardDb` index), for DISPLAY: which card's shot this is, so a
    /// viewer can tell a tower's bolt from a Musketeer's. It changes nothing the
    /// simulation does, so it is deliberately NOT in the state hash. None for a
    /// projectile restored from a snapshot older than this field -- which is why the
    /// export writes -2 for it rather than folding it into the -1 of a crown tower.
    #[serde(default)]
    pub firer_card: Option<u16>,
    /// A STRAIGHT SHOT (calibration combat.RANGE_PROJECTILE = straight_to_range, or a
    /// pellet of combat.MULTIPLE_PROJECTILES = client_fan): it flies to `aim`, the point
    /// ProjectileRange from its attacker, and never follows `target`, hitting what it
    /// passes (`step_straight`). None for every homing shot, which is every shot under the
    /// shipped arms. Added after SNAPSHOT_FORMAT 20; absent in older snapshots = None, and
    /// hashed only when present, so a battle without one hashes as it did.
    #[serde(default)]
    pub straight: Option<Straight>,
}

/// The state of a straight shot (`Projectile::straight`). Distances SUBTILES.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Straight {
    /// The attacker's centre at launch: the range, and a pingpong throw's distance, are
    /// measured from it.
    pub origin: Vec2,
    /// ProjectileRadius: an enemy is hit when its centre comes within this plus its radius.
    pub reach: i32,
    /// Pushback (0: none) and PushbackAll: a victim is pushed radially from the shot's centre.
    pub push: i32,
    pub push_all: bool,
    /// OnlyEnemies.
    pub only_enemies: bool,
    /// Ticks the shot still stands at its launch point after its creation tick before its
    /// first step (a fan pellet's release delay; 0 for a single shot).
    pub hold: i32,
    /// A PINGPONG throw's period in ticks (PingpongVisualTime / TICK_MS), 0 for a one-way
    /// shot. Nothing sets it while cards.json does not carry PingpongVisualTime.
    pub period: i32,
    /// The pingpong throw's start distance (ProjectileStartRadius).
    pub start: i32,
    /// Ticks since a pingpong throw (0 on its creation tick).
    pub t: i32,
    /// Every unit hit so far (on this leg, for a pingpong throw), kept sorted so the set --
    /// and the state hash -- is a function of who was hit, not of neighbour order.
    pub hit: Vec<EntityId>,
}

/// combat.MULTIPLE_PROJECTILES = client_fan: the angle between neighbouring pellets of a fan,
/// degrees. Measured on client 15.535.29 on the Hunter (26 volleys: offsets 0, +7, -7, +14, -14,
/// +21, -21, +28, -28, +35 in creation order, to 0.06 degrees). The only row measured; another
/// MultipleProjectiles row may step otherwise.
pub const FAN_STEP_DEG: i64 = 7;

/// combat.MULTIPLE_PROJECTILES = client_fan: a pellet starts to move this many ticks after its
/// creation, drawn uniformly from the battle's Rng. Measured on client 15.535.29 on the Hunter:
/// 2 to 5 on all but one of about 250 pellets (a single 1). The client draws from its own
/// random stream, so the engine matches the range, not the order.
pub const FAN_RELEASE_TICKS: (i32, i32) = (2, 5);

/// combat.DEPLOY_PROJECTILE = client_on_landing: the deploy projectile lands this many ticks
/// after the unit's first frame. Measured on client 15.535.29 on the Mega Knight (both sides:
/// first frame 259, the blow on 265). Whether it is a flight time (the row's Speed is 1000) or
/// a fixed delay is open; one card carries a deploy projectile.
pub const DEPLOY_PROJECTILE_DELAY_TICKS: i32 = 6;

/// ONE TICK OF A FLYING PROJECTILE, a troop's, a building's or a crown tower's shot or a
/// spell's flight, under calibration combat.PROJECTILE_STEP. Returns the new position, `aim`
/// itself on the tick it arrives. `speed` is SUBTILES per tick.
#[inline]
pub fn projectile_advance(step: ProjectileStep, pos: Vec2, aim: Vec2, speed: i32, frac: &mut Vec2) -> Vec2 {
    match step {
        ProjectileStep::FractionCarry => advance(pos, aim, speed, frac).0,
        // client_native_truncated: path.rs `advance_client`, in native units, nothing carried.
        #[cfg(not(clash_plant = "projectile_step_carries_fraction"))]
        ProjectileStep::ClientNativeTruncated => {
            *frac = Vec2::default();
            advance_client(pos, aim, speed / K)
        }
        // PLANT (regression): the new arm steps exactly Speed and carries the remainder.
        #[cfg(clash_plant = "projectile_step_carries_fraction")]
        ProjectileStep::ClientNativeTruncated => advance(pos, aim, speed, frac).0,
    }
}

/// `src` moved `amount` (SUBTILES) toward `tgt` by the client's arithmetic, in NATIVE units:
/// `src + trunc0(v * amount / isqrt(v.v))` with `v = tgt - src`, the result a whole native
/// position. A zero `v` takes the caster's forward axis. The start point of a shot under
/// combat.PROJECTILE_STEP = client_native_truncated and of every straight shot.
fn toward_native(src: Vec2, tgt: Vec2, amount: i32, team: Team) -> Vec2 {
    fan_aim(src, tgt, amount, 0, team)
}

/// The point `dist` (SUBTILES) from `src` on the bearing to `tgt` turned `deg` degrees
/// (positive = counter-clockwise in arena coordinates, the same for both seats: the client's
/// order is not mirrored), in NATIVE units: the aim point of a straight shot and of each
/// pellet of a fan. The turn is the integer rotation of `v = tgt - src` by fixed.rs
/// `cos_pi_frac` / `sin_pi_frac`; at 0 degrees it is exactly `src + trunc0(v * dist /
/// isqrt(v.v))`.
fn fan_aim(src: Vec2, tgt: Vec2, dist: i32, deg: i64, team: Team) -> Vec2 {
    let (sx, sy) = ((src.x / K) as i64, (src.y / K) as i64);
    let (mut vx, mut vy) = ((tgt.x / K) as i64 - sx, (tgt.y / K) as i64 - sy);
    let mut n = isqrt(vx * vx + vy * vy);
    if n == 0 {
        (vx, vy, n) = (0, forward_dy(team) as i64, 1);
    }
    let (c, s) = (cos_pi_frac(deg, 180) as i128, sin_pi_frac(deg, 180) as i128);
    let (vx, vy) = (vx as i128, vy as i128);
    let (rx, ry) = (vx * c - vy * s, vx * s + vy * c);
    let den = (n as i128) * (TRIG_ONE as i128);
    let d = (dist / K) as i128;
    Vec2::new(((sx as i128 + rx * d / den) as i32) * K, ((sy as i128 + ry * d / den) as i32) * K)
}

/// The offset of pellet `k` of a fan, degrees: 0, +7, -7, +14, -14, ... (`FAN_STEP_DEG`).
fn fan_offset_deg(k: i32) -> i64 {
    let m = ((k + 1) / 2) as i64 * FAN_STEP_DEG;
    if k % 2 == 1 {
        m
    } else {
        -m
    }
}

/// The projectiles one shot of `card` fires under combat.MULTIPLE_PROJECTILES.
fn fan_count(calib: &Calib, card: &CardDef) -> i32 {
    #[cfg(not(clash_plant = "fan_single_pellet"))]
    let n = card.multiple_projectiles.max(1);
    #[cfg(clash_plant = "fan_single_pellet")]
    let n = {
        let _ = card.multiple_projectiles;
        1 // PLANT (regression): the new arm fires one projectile a shot, as `one` does.
    };
    if calib.multiple_projectiles == MultipleProjectiles::ClientFan {
        n
    } else {
        1
    }
}

/// A pingpong throw's position `t` ticks after it (combat.RANGE_PROJECTILE = straight_to_range
/// on a PingpongVisualTime row), measured on client 15.535.29 on the Executioner's axe (within
/// 5): `start + (range - start) x sin(pi t / period)` from `origin` toward `apex`, the point
/// `range` out on the launch line, in NATIVE units. The apex tick (t = period / 2) is recorded
/// on the client at the value one tick before it; that is open and not modelled.
fn pingpong_pos(origin: Vec2, apex: Vec2, start: i32, t: i32, period: i32) -> Vec2 {
    let (ox, oy) = ((origin.x / K) as i64, (origin.y / K) as i64);
    let (ux, uy) = ((apex.x / K) as i64 - ox, (apex.y / K) as i64 - oy);
    let range = isqrt(ux * ux + uy * uy).max(1);
    let start = (start / K) as i64;
    let d = start + (range - start) * sin_pi_frac(t as i64, period.max(1) as i64) / TRIG_ONE;
    Vec2::new(((ox + ux * d / range) as i32) * K, ((oy + uy * d / range) as i32) * K)
}

/// Damage after the crown-tower reduction, if the victim is a crown tower. Damage
/// and percent are non-negative, so every mode is a plain integer division.
#[inline]
pub fn damage_against(kind: EntityKind, amount: i32, crown_pct: i32, rounding: CrownRounding) -> i32 {
    if !kind.is_crown_tower() {
        return amount;
    }
    let n = (amount as i64) * (crown_pct as i64);
    #[cfg(clash_plant = "ct_floor")]
    let rounding = {
        // PLANT (regression): a truncating crown-tower reduction.
        let _ = rounding;
        CrownRounding::Floor
    };
    (match rounding {
        CrownRounding::CeilKeptShare => (n + PERCENT - 1) / PERCENT,
        CrownRounding::Floor => n / PERCENT,
        CrownRounding::RoundHalfUp => (n + PERCENT / 2) / PERCENT,
    }) as i32
}

/// Append a hit on every enemy of `team` whose hitbox edge is within `radius`
/// of `center`. Victim order does not matter: the buffer is summed.
///
/// ON RETURN `scratch` HOLDS EXACTLY THE VICTIMS THAT WERE HIT, compacted in
/// place out of the neighbour list the spatial hash filled. It is not a
/// convenience: `apply_attack_buff` rides this list, and an earlier version read
/// the RAW neighbour query -- `radius + hash.max_radius()` around the centre,
/// unfiltered -- so an Ice Spirit's freeze landed on the attacker's own team and on
/// units outside the splash disc
/// (`tests/status.rs::an_attack_buff_lands_on_the_splash_victims_only`).
#[allow(clippy::too_many_arguments)]
pub fn splash(
    ents: &Entities,
    hash: &SpatialHash,
    team: Team,
    center: Vec2,
    radius: i32,
    hits_air: bool,
    hits_ground: bool,
    amount: i32,
    crown_pct: i32,
    rounding: CrownRounding,
    out: &mut DamageBuffer,
    scratch: &mut Vec<u32>,
) {
    hash.neighbours_within(ents, center, radius + hash.max_radius(), scratch);
    let mut kept = 0usize;
    for i in 0..scratch.len() {
        let v = scratch[i] as usize;
        if ents.team[v] == team || ents.hp[v] <= 0 {
            continue;
        }
        #[cfg(clash_plant = "acquire_delay_blocks_splash")]
        if ents.acquirable_from[v] > 0 {
            continue; // PLANT: a unit under targeting.SPAWNED_UNIT_ACQUIRE_DELAY is spared by a splash.
        }
        if if ents.flying[v] { !hits_air } else { !hits_ground } {
            continue;
        }
        if in_range_edge(center, ents.pos[v], radius, ents.radius[v]) {
            out.hits.push(Hit { target: ents.id_of(v), amount: damage_against(ents.kind[v], amount, crown_pct, rounding), ignores_hide: false });
            scratch[kept] = v as u32;
            kept += 1;
        }
    }
    // PLANT (regression): the earlier scratch, the RAW neighbour query, so an
    // attack buff rides it onto the attacker's own team and past the splash disc.
    #[cfg(not(clash_plant = "buff_on_raw_neighbours"))]
    scratch.truncate(kept);
    #[cfg(clash_plant = "buff_on_raw_neighbours")]
    let _ = kept;
}

/// What one entity's attack step decided. Applied by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttackStep {
    pub phase: AttackPhase,
    /// The progress counter (progress_credit) or the ms elapsed in `phase`
    /// (windup_load_time).
    pub ms: i32,
    /// The load timer (progress_credit only; 0 under windup_load_time).
    pub load_ms: i32,
    pub fired_at: Option<EntityId>,
    /// The charged snap fired this hit (charge.CHARGED_HIT_TIMING): the charge is
    /// consumed at the snap itself, whatever charge.RESET_ON_ATTACK says about an
    /// ordinary hit.
    pub charge_snapped: bool,
}

/// Advance entity a's attack state by one tick, reading only. Dispatches on
/// calibration combat.ATTACK_CYCLE.
pub fn attack_step(ents: &Entities, cards: &CardDb, calib: &Calib, a: usize, can_act: bool) -> AttackStep {
    match calib.attack_cycle {
        AttackCycle::ProgressCredit => attack_step_progress(ents, cards, calib, a, can_act),
        AttackCycle::WindupLoadTime => attack_step_windup(ents, cards, calib, a, can_act),
    }
}

/// THE SHIPPED CYCLE (combat.ATTACK_CYCLE = progress_credit; module doc), the
/// attack pass of one tick for entity `a`:
///
///   load -= 50, clamped at 0
///   no target -> progress 0, not attacking
///   in_range = ATTACK_RANGE_RULE(target)
///   hit_started = progress % HitSpeed > 50
///   attack this tick iff in_range || hit_started
///     else: progress = 0, stop attacking
///   attacking:
///     progress == 0 && !charged (a fresh cycle):
///       LoadTime <= HitSpeed: progress = LoadTime - load; load = LoadTime
///       LoadTime >  HitSpeed: wait while load > HitSpeed (progress stays 0),
///                             else progress = 0, load = 0
///       then progress += 50
///     charged: progress += HitSpeed - progress % HitSpeed, the charge consumed
///     else: progress += 50
///   fire iff progress / HitSpeed grew
///     the fire sets load = LoadTime
///
/// The corpus shows exactly this: on the frame a unit starts attacking a fresh
/// target its progress reads LoadTime + 50 and its load timer LoadTime (1690 of
/// 2024 entries; the rest are spawned units filed under their spawner's card),
/// the first projectile leaves (HitSpeed - LoadTime) / 50 - 1 ticks later (911 of
/// 1004: Musketeer 13, Archers 9, Bomber 3, the princess tower 15, the king 9),
/// and a re-entry within LoadTime of the last hit reads LoadTime + 50 - (load - 50)
/// (113 of 172). `AttackPhase::Windup` = attacking with the swing under way (the
/// target lock, the Path phase's hold), `Cooldown` = attacking on the hit tick
/// (the unit stands; next tick the range gates the next cycle), `Idle` = not
/// attacking. Not modelled: a hit-speed buff (Rage's HitSpeedMultiplier),
/// SpecialRange and per-target ranges, LoadFirstHit (Sparky). The dash's hit is not
/// this cycle's: state.rs `phase_path16402` lands it (combat.DASH_ATTACK).
fn attack_step_progress(ents: &Entities, cards: &CardDb, calib: &Calib, a: usize, can_act: bool) -> AttackStep {
    let card = cards.get(ents.card[a]);
    let target = ents.target[a].filter(|t| ents.is_alive(*t));
    // THE PROGRESS ADVANCE (combat.HIT_SPEED_BUFF = progress_scaled): the counter
    // gains the HitSpeedMultiplier composition of TICK_MS, not TICK_MS -- 65 under
    // Rage, 35 under an Ice Wizard's slow -- and when that is <= 0 the whole step is
    // skipped, so a -100 Freeze holds the cycle exactly where it stands. The LOAD
    // timer is NOT scaled: a flat 50 comes off it every tick.
    let tick = match calib.hit_speed_buff {
        HitSpeedBuff::ProgressScaled => ents.buffed(&cards.buffs, a, Sel::HitSpeed, calib.tick_ms),
        HitSpeedBuff::None => calib.tick_ms,
    };
    let hs = card.hit_speed_ms;
    let lt = card.load_time_ms.max(0);
    let mut load = (ents.attack_load_ms[a] - calib.tick_ms).max(0);
    let phase = ents.attack_phase[a];
    let mut progress = ents.attack_ms[a];
    let idle = |load| AttackStep { phase: AttackPhase::Idle, ms: 0, load_ms: load, fired_at: None, charge_snapped: false };
    if tick <= 0 {
        // The composed advance is 0: a -100 HitSpeedMultiplier (a freeze). The
        // PROGRESS holds, but the LOAD TIMER does NOT: its decrement, max(load - 50,
        // 0), runs every tick before any of this, held or not (combat.ATTACK_CYCLE).
        // An earlier version held the load timer too, which made a unit frozen
        // mid-cooldown keep its whole remaining reload, so its first hit after the
        // hold came LoadTime / 50 ticks late.
        return AttackStep { phase, ms: progress, load_ms: load, fired_at: None, charge_snapped: false };
    }
    if !can_act {
        // Deploying, stunned, mid-ladder, leaping, under ground, an inactive king:
        // the counters hold (status.STUN_ATTACK_TIMER_MODEL = pause; a landed push
        // already reset the cycle in `apply_effects` under knockback.ATTACK_RESET).
        return AttackStep { phase, ms: progress, load_ms: load, fired_at: None, charge_snapped: false };
    }
    let Some(t) = target else { return idle(load) };
    if hs <= 0 {
        return idle(load);
    }
    let ti = t.index as usize;
    let in_range = in_attack_range(calib, ents.pos[a], card.range, ents.radius[a], ents.pos[ti], ents.radius[ti]);
    // THE HIT-STARTED TEST, `progress % HitSpeed > TICK_MS` -- a FLAT constant and
    // not the buffed advance, so a slowed unit's swing is still under way. `>=
    // TICK_MS` would be off by one and reachable on EVERY cycle of EVERY card: a
    // fire leaves the progress on an exact multiple of HitSpeed, so the tick after
    // the cycle restarts has remainder exactly 50 (Knight 1250 % 1200, Prince
    // 1450 % 1400, Musketeer 1050 % 1000), which is "not started" and lets the
    // range gate cancel (combat.ATTACK_CYCLE's boundary note).
    let hit_started = progress % hs > calib.tick_ms;
    if !(in_range || hit_started) {
        return idle(load);
    }
    let cycle_before = progress / hs;
    let snap = ents.charged[a] && card.charge.is_some() && calib.charged_hit_timing == ChargedHitTiming::FirstAttackPassNoWindup;
    let mut charge_snapped = false;
    if progress == 0 && !snap {
        if lt <= hs {
            progress = lt - load;
            load = lt;
        } else if load > hs {
            // LoadTime longer than a cycle: the unit stands attacking until its
            // reload is within one cycle.
            return AttackStep { phase: AttackPhase::Windup, ms: 0, load_ms: load, fired_at: None, charge_snapped: false };
        } else {
            load = 0;
        }
        progress += tick;
    } else if snap {
        progress += hs - progress % hs;
        charge_snapped = true;
    } else {
        progress += tick;
    }
    let fired = progress / hs > cycle_before;
    if fired {
        load = lt;
    }
    AttackStep {
        phase: if fired { AttackPhase::Cooldown } else { AttackPhase::Windup },
        ms: progress,
        load_ms: load,
        fired_at: if fired { Some(t) } else { None },
        charge_snapped,
    }
}

/// THE EARLIER CYCLE (combat.ATTACK_CYCLE = windup_load_time): a LoadTime
/// windup on entering range, the hit, then HitSpeed - LoadTime of cooldown; a
/// charged unit winds up like any other (charge.CHARGED_HIT_TIMING's
/// after_load_time_windup, whatever the key says under this arm). Refuted by the
/// live corpus on every first hit (the key's provenance); kept runnable.
fn attack_step_windup(ents: &Entities, cards: &CardDb, calib: &Calib, a: usize, can_act: bool) -> AttackStep {
    let card = cards.get(ents.card[a]);
    let target = ents.target[a].filter(|t| ents.is_alive(*t));
    let mut phase = ents.attack_phase[a];
    let mut ms = ents.attack_ms[a];
    let load = card.load_time_ms.max(0);
    let cooldown = (card.hit_speed_ms - load).max(0);
    let in_range = |t: EntityId| {
        let ti = t.index as usize;
        in_attack_range(calib, ents.pos[a], card.range, ents.radius[a], ents.pos[ti], ents.radius[ti])
    };
    let out = |phase, ms, fired_at| AttackStep { phase, ms, load_ms: 0, fired_at, charge_snapped: false };
    if !can_act {
        return out(phase, ms, None);
    }
    let tick = calib.tick_ms;
    match phase {
        AttackPhase::Idle => {
            if let Some(t) = target {
                if in_range(t) {
                    phase = AttackPhase::Windup;
                    ms = tick;
                }
            }
        }
        AttackPhase::Windup => {
            if target.is_none() {
                // Target died mid-windup: the swing is lost.
                return out(AttackPhase::Idle, 0, None);
            }
            ms += tick;
        }
        AttackPhase::Cooldown => {
            ms += tick;
            if ms >= cooldown {
                match target {
                    Some(t) if in_range(t) => {
                        phase = AttackPhase::Windup;
                        ms -= cooldown;
                    }
                    _ => return out(AttackPhase::Idle, 0, None),
                }
            }
        }
    }
    if phase == AttackPhase::Windup && ms >= load {
        if let Some(t) = target {
            return out(AttackPhase::Cooldown, ms - load, Some(t));
        }
    }
    out(phase, ms, None)
}

/// Turn a completed windup into damage: a projectile, or an instant hit.
///
/// `rng` is drawn only for the release delays of a fan (combat.MULTIPLE_PROJECTILES =
/// client_fan), so a battle under the shipped arms consumes nothing from it here.
/// `bolts` are the extra bolts of this attack (combat.MULTIPLE_TARGETS =
/// client_bolts_per_target, state.rs `extra_bolts`), empty under the shipped arm.
#[allow(clippy::too_many_arguments)]
pub fn fire(
    ents: &Entities,
    hash: &SpatialHash,
    cards: &CardDb,
    calib: &Calib,
    a: usize,
    target: EntityId,
    dmg: &mut DamageBuffer,
    fx: &mut EffectBuffer,
    projectiles: &mut Vec<Projectile>,
    scratch: &mut Vec<u32>,
    rng: &mut Rng,
    bolts: &[EntityId],
) {
    let card = cards.get(ents.card[a]);
    let ti = target.index as usize;
    // THE ATTACK'S BUFF (card.rs `CardDef::attack_buff`): a projectile carries it to
    // its arrival tick, an instant hit applies it here. The per-pulse amount is the
    // ATTACKER's level-scaled figure, computed once, because the victim does not know
    // the attacker's level.
    let atk_buff = card.attack_buff;
    let atk_pulse = match atk_buff {
        None => 0,
        Some(b) => {
            let base = cards.buffs[b.buff as usize].pulse_base();
            if base == 0 {
                0
            } else {
                let mag = cards.scaled(ents.card[a], ents.level[a], base.abs()).unwrap_or(base.abs());
                if base < 0 {
                    -mag
                } else {
                    mag
                }
            }
        }
    };
    // THE CHARGED HIT (card.rs `ChargeDef`; calibration charge.SPECIAL_LEVEL_SCALING):
    // a charged unit's hit is DamageSpecial INSTEAD of Damage -- a replacement, not
    // an addition (DamageSpecial is exactly 2 x Damage on every 2018 charge row, the
    // relation DashDamage has where replacement is unambiguous). Everything else
    // about the hit is the ordinary one: the card's range, its crown-tower percent
    // (100 on all three), no knockback (AttackPushBack is blank on them). The
    // caller (state.rs phase_attack) consumes the charge right after this call.
    let amount = match (ents.charged[a], card.charge) {
        (true, Some(ch)) => {
            #[cfg(clash_plant = "charge_damage_ignores_level")]
            {
                let _ = calib.charge_special_level_scaling;
                ch.damage_special // PLANT: the level-1 DamageSpecial at every level.
            }
            #[cfg(not(clash_plant = "charge_damage_ignores_level"))]
            match calib.charge_special_level_scaling {
                // A level-1 stat like every other; the level was validated at spawn
                // (state.rs spawn_now scaled hitpoints through the same table).
                ChargeLevelScaling::ScaleSpecialBase => cards.scaled(ents.card[a], ents.level[a], ch.damage_special).expect("level validated at spawn"),
                ChargeLevelScaling::TwiceScaledDamage => ((ents.damage[a] as i64) * (ch.damage_special as i64) / (card.damage.max(1) as i64)) as i32,
            }
        }
        _ => ents.damage[a],
    };
    let pct = card.crown_tower_damage_percent;
    let splash_r = if card.area_damage_radius > 0 {
        card.area_damage_radius
    } else {
        card.projectile.map(|p| p.radius).unwrap_or(0)
    };
    if let Some(p) = card.projectile {
        // combat.CUSTOM_FIRST_PROJECTILE = client_first_of_volley: the attack's projectile is
        // the card's CustomFirstProjectile row when that is a row of its own (the Princess's
        // PrincessProjectile: 66 at level 1, Radius 2000, Speed 600, air and ground), with
        // ITS damage, splash radius, speed, filters and crown percent. Measured on client
        // 15.535.29 (both sides): her first volley takes 168 off two Goblins about 1,200 apart
        // on one frame. The volley's other MultipleProjectiles - 1 are her Projectile column,
        // the damage-less PrincessProjectileDeco, and are not fired: they would change
        // nothing the simulation computes, and their layout (GroupProjectiles) is unmeasured.
        #[cfg(not(clash_plant = "custom_first_projectile_unread"))]
        let custom: Option<CustomShotDef> = match calib.custom_first_projectile {
            CustomFirstProjectile::ClientFirstOfVolley => card.custom_first_projectile,
            CustomFirstProjectile::NotRead => None,
        };
        #[cfg(clash_plant = "custom_first_projectile_unread")]
        let custom: Option<CustomShotDef> = {
            let _ = calib.custom_first_projectile;
            None // PLANT (regression): the new arm fires the Projectile column, the Princess's decoration.
        };
        // combat.RANGE_PROJECTILE = straight_to_range, and combat.MULTIPLE_PROJECTILES =
        // client_fan: a row with a ProjectileRange fires straight shots (`Straight`).
        if let (None, Some(rs)) = (custom, card.range_shot) {
            let pellets = fan_count(calib, card);
            #[cfg(not(clash_plant = "range_projectile_unread"))]
            let straight_arm = calib.range_projectile == RangeProjectile::StraightToRange;
            #[cfg(clash_plant = "range_projectile_unread")]
            let straight_arm = {
                let _ = calib.range_projectile;
                false // PLANT (regression): the new arm aims a range row's shot at its target, as to_target.
            };
            if straight_arm || pellets > 1 {
                let team = ents.team[a];
                // Start-of-tick positions: Attack runs before Move.
                let (src, tgt) = (ents.pos[a], ents.pos[ti]);
                // The start point and its timing are combat.PROJECTILE_LAUNCH's; a straight
                // shot's arithmetic is always the client's native one.
                let (pos, fresh) = match calib.projectile_launch {
                    ProjectileLaunch::StartRadiusNextTick => (toward_native(src, tgt, card.projectile_start_radius, team), true),
                    ProjectileLaunch::AttackerCentreSameTick => (src, false),
                };
                // A pingpong row (PingpongVisualTime) throws one shot out and back.
                let period = match rs.pingpong_ms {
                    Some(ms) if pellets == 1 => (ms / calib.tick_ms.max(1)).max(1),
                    _ => 0,
                };
                let (push, push_all) = rs.knockback.map_or((0, false), |k| (k.distance, k.all));
                for k in 0..pellets {
                    // Each pellet of a fan is aimed at ProjectileRange on its own offset from
                    // the bearing to the target and released 2-5 ticks after its creation
                    // (`FAN_RELEASE_TICKS`); a single shot on the bearing, released at once.
                    let aim = fan_aim(src, tgt, rs.range, fan_offset_deg(k), team);
                    #[cfg(not(clash_plant = "fan_pellets_unheld"))]
                    let hold = if pellets > 1 { rng.range(FAN_RELEASE_TICKS.0, FAN_RELEASE_TICKS.1) - 1 } else { 0 };
                    #[cfg(clash_plant = "fan_pellets_unheld")]
                    let hold = {
                        let _ = (&rng, FAN_RELEASE_TICKS);
                        0 // PLANT (regression): every pellet moves on the tick after its creation.
                    };
                    let mut shot = Projectile {
                        team,
                        pos,
                        target,
                        aim,
                        speed: p.speed * calib.projectile_speed_to_subtiles_per_tick,
                        damage: amount,
                        crown_pct: pct,
                        splash: 0,
                        hits_air: rs.hits_air,
                        hits_ground: rs.hits_ground,
                        frac: Vec2::default(),
                        fresh,
                        buff: atk_buff,
                        pulse: atk_pulse,
                        firer_card: Some(ents.card[a]),
                        straight: Some(Straight {
                            origin: src,
                            reach: rs.reach,
                            push,
                            push_all,
                            only_enemies: rs.only_enemies,
                            hold,
                            period,
                            start: card.projectile_start_radius,
                            t: 0,
                            hit: Vec::new(),
                        }),
                    };
                    // THE CREATION TICK'S TEST (a one-way shot born on its launch point): the
                    // launch point against the start-of-tick positions, measured on client
                    // 15.535.29 (a boulder hit 1428 from its launch point on the tick it was
                    // created). A pingpong throw first tests on the next tick, against this
                    // tick's position (`step_straight`).
                    if fresh && period == 0 {
                        straight_hits(ents, hash, cards, calib, &mut shot, pos, dmg, fx, scratch);
                    }
                    projectiles.push(shot);
                }
                return;
            }
        }
        let (speed, amount, splash_r, hits_air, hits_ground, pct) = match custom {
            Some(cf) => (cf.speed, cards.scaled(ents.card[a], ents.level[a], cf.damage).expect("level validated at spawn"), cf.radius, cf.hits_air, cf.hits_ground, cf.crown_pct),
            None => (p.speed, amount, splash_r, card.attacks_air, card.attacks_ground, pct),
        };
        // combat.PROJECTILE_LAUNCH: the projectile is born ProjectileStartRadius from
        // the attacker's centre toward the target and does not move on the fire tick
        // (the live tower arrows: 299-300 from the centre on the launch frame, 600 per
        // tick from the next); the old arm starts it at the centre and steps it at once.
        let (pos, fresh) = match calib.projectile_launch {
            ProjectileLaunch::StartRadiusNextTick => {
                let d = ents.pos[ti].sub(ents.pos[a]);
                let len = crate::fixed::isqrt(d.len2()) as i32;
                let r = card.projectile_start_radius.min(len.max(0));
                // combat.PROJECTILE_STEP = client_native_truncated: the same point by the
                // client's arithmetic, src + trunc0(v * R / isqrt(v.v)) in NATIVE units
                // (measured on client 15.535.29: 6,695 of 6,813 launches), so the shot sits
                // on the native grid; the old arm truncates in subtiles.
                #[cfg(not(clash_plant = "projectile_start_in_subtiles"))]
                let native = calib.projectile_step == ProjectileStep::ClientNativeTruncated;
                #[cfg(clash_plant = "projectile_start_in_subtiles")]
                let native = false; // PLANT (regression): the new arm's start point is truncated in subtiles.
                let pos = if native {
                    toward_native(ents.pos[a], ents.pos[ti], r, ents.team[a])
                } else if len > 0 {
                    Vec2::new(ents.pos[a].x + ((d.x as i64) * (r as i64) / (len as i64)) as i32, ents.pos[a].y + ((d.y as i64) * (r as i64) / (len as i64)) as i32)
                } else {
                    ents.pos[a]
                };
                (pos, true)
            }
            ProjectileLaunch::AttackerCentreSameTick => (ents.pos[a], false),
        };
        projectiles.push(Projectile {
            team: ents.team[a],
            pos,
            target,
            aim: ents.pos[ti],
            // Every projectile reads time.PROJECTILE_SPEED_TO_SUBTILES_PER_TICK, its
            // own key, NOT the troop key `calib.speed_to_subtiles_per_tick` (the two
            // hold the same value today; see that key's provenance for why they are
            // separate).
            speed: speed * calib.projectile_speed_to_subtiles_per_tick,
            damage: amount,
            crown_pct: pct,
            splash: splash_r,
            hits_air,
            hits_ground,
            frac: Vec2::default(),
            fresh,
            buff: atk_buff,
            pulse: atk_pulse,
            firer_card: Some(ents.card[a]),
            straight: None,
        });
        return;
    }
    if splash_r > 0 {
        // SelfAsAoeCenter (Valkyrie): the splash is centred on the ATTACKER.
        // Centring the splash on the TARGET instead is wrong for this family, and
        // silently so: the flag loads into CardDef whether or not it is read.
        // Pinned by mechanics::valkyrie_splash_is_centred_on_the_valkyrie_not_her_target.
        #[cfg(not(clash_plant = "aoe_centre_on_target"))]
        let centre = if card.self_as_aoe_center { ents.pos[a] } else { ents.pos[ti] };
        #[cfg(clash_plant = "aoe_centre_on_target")]
        let centre = ents.pos[ti];
        splash(ents, hash, ents.team[a], centre, splash_r, card.attacks_air, card.attacks_ground, amount, pct, calib.crown_rounding, dmg, scratch);
        apply_attack_buff(ents, calib, atk_buff, atk_pulse, target, scratch, fx);
    } else {
        dmg.hits.push(Hit { target, amount: damage_against(ents.kind[ti], amount, pct, calib.crown_rounding), ignores_hide: false });
        if let Some(b) = atk_buff {
            fx.buffs.push(BuffHit { target, buff: b.buff, time_ms: b.time_ms, pulse_amount: atk_pulse });
        }
        // combat.MULTIPLE_TARGETS = client_bolts_per_target: every other bolt of the attack is
        // the whole hit again, damage and buff, on its own victim (state.rs `extra_bolts`
        // chose them; under AllTargetsHit a bolt with no other enemy is `target` again).
        for &b in bolts {
            let bi = b.index as usize;
            dmg.hits.push(Hit { target: b, amount: damage_against(ents.kind[bi], amount, pct, calib.crown_rounding), ignores_hide: false });
            if let Some(bf) = atk_buff {
                fx.buffs.push(BuffHit { target: b, buff: bf.buff, time_ms: bf.time_ms, pulse_amount: atk_pulse });
            }
        }
    }
}

/// Buffer `buff` on everything the hit it rode landed on. `scratch` is the victim
/// list `splash` just filled, so the buff lands on exactly the units the damage did
/// (calibration status.TARGET_BUFF_ON_SPLASH = whole_splash); under
/// `primary_target_only` only `target` takes it.
fn apply_attack_buff(
    ents: &Entities,
    calib: &Calib,
    buff: Option<BuffApply>,
    pulse: i32,
    target: EntityId,
    scratch: &[u32],
    fx: &mut EffectBuffer,
) {
    let Some(b) = buff else { return };
    match calib.target_buff_on_splash {
        TargetBuffScope::WholeSplash => {
            for &v in scratch {
                fx.buffs.push(BuffHit { target: ents.id_of(v as usize), buff: b.buff, time_ms: b.time_ms, pulse_amount: pulse });
            }
        }
        TargetBuffScope::PrimaryTargetOnly => {
            fx.buffs.push(BuffHit { target, buff: b.buff, time_ms: b.time_ms, pulse_amount: pulse });
        }
    }
}

/// Every enemy a straight shot `p` at `at` hits now: alive, of the shot's air/ground
/// filters and OnlyEnemies, not hit before (on this leg), with its centre within the
/// shot's ProjectileRadius plus its own radius (measured on client 15.535.29: the Bowler's
/// boulder, the Elite Archer's arrow). Each takes the shot's damage and buff, and a
/// Pushback pushes it radially from `at` (spell.rs `push_from`, knockback.DIRECTION_ROLLING's
/// radial law). Hits and pushes go to the buffers in neighbour order; neither depends on it
/// (the hit set is sorted, the damage summed, and each unit is pushed once by one shot).
/// Returns whether anything was hit.
#[allow(clippy::too_many_arguments)]
fn straight_hits(
    ents: &Entities,
    hash: &SpatialHash,
    cards: &CardDb,
    calib: &Calib,
    p: &mut Projectile,
    at: Vec2,
    dmg: &mut DamageBuffer,
    fx: &mut EffectBuffer,
    nb: &mut Vec<u32>,
) -> bool {
    let (team, damage, crown_pct, hits_air, hits_ground, buff, pulse) = (p.team, p.damage, p.crown_pct, p.hits_air, p.hits_ground, p.buff, p.pulse);
    let Some(s) = p.straight.as_mut() else { return false };
    let ctx = SpellCtx { ents, hash, cards, calib };
    hash.neighbours_within(ents, at, s.reach + hash.max_radius(), nb);
    let mut any = false;
    for &v in nb.iter() {
        let v = v as usize;
        if !ents.alive[v] || ents.hp[v] <= 0 || (s.only_enemies && ents.team[v] == team) {
            continue;
        }
        if if ents.flying[v] { !hits_air } else { !hits_ground } {
            continue;
        }
        if !in_range_edge(at, ents.pos[v], s.reach, ents.radius[v]) {
            continue;
        }
        let id = ents.id_of(v);
        match s.hit.binary_search(&id) {
            Ok(_) => continue,
            Err(k) => s.hit.insert(k, id),
        }
        any = true;
        dmg.hits.push(Hit { target: id, amount: damage_against(ents.kind[v], damage, crown_pct, calib.crown_rounding), ignores_hide: false });
        if let Some(b) = buff {
            fx.buffs.push(BuffHit { target: id, buff: b.buff, time_ms: b.time_ms, pulse_amount: pulse });
        }
        #[cfg(not(clash_plant = "range_shot_unpushed"))]
        if s.push > 0 {
            push_from(&ctx, team, v, at, &KnockbackDef { distance: s.push, all: s.push_all }, fx);
        }
        #[cfg(clash_plant = "range_shot_unpushed")]
        let _ = (&ctx, s.push_all); // PLANT (regression): a straight shot's Pushback moves nothing.
    }
    any
}

/// One tick of a straight shot (`Projectile::straight`), after the move pass. Returns false
/// once it is gone.
///   * A PINGPONG throw: `t` advances; the shot tests the positions it hits against its
///     PREVIOUS frame's position (measured on client 15.535.29 on the Executioner's axe:
///     5 hits, the tightest miss 1519 against 1500), then moves to `pingpong_pos`. Its hit
///     set is cleared as the return leg starts, so it hits a unit once a leg, and it is gone
///     after `t = period`.
///   * A held pellet (`hold` > 0) stands at its launch point and tests there.
///   * A one-way shot steps toward `aim`, the point ProjectileRange out, by
///     combat.PROJECTILE_STEP's law, and is GONE instead of taking the step that would reach
///     it: it is last seen at the last point within ProjectileRange (measured on client
///     15.535.29: 6933-6939 of 7000 for the boulder, 10789-10796 of 11000 for the arrow).
///     Otherwise it moves and tests its new position against the post-move positions.
#[allow(clippy::too_many_arguments)]
fn step_straight(
    ents: &Entities,
    hash: &SpatialHash,
    cards: &CardDb,
    calib: &Calib,
    p: &mut Projectile,
    dmg: &mut DamageBuffer,
    fx: &mut EffectBuffer,
    nb: &mut Vec<u32>,
) -> bool {
    let Some(s) = p.straight.as_mut() else { return false };
    if s.period > 0 {
        s.t += 1;
        if s.t > s.period {
            return false;
        }
        if 2 * s.t > s.period && 2 * (s.t - 1) <= s.period {
            s.hit.clear();
        }
        let (origin, start, t, period) = (s.origin, s.start, s.t, s.period);
        let prev = p.pos;
        straight_hits(ents, hash, cards, calib, p, prev, dmg, fx, nb);
        p.pos = pingpong_pos(origin, p.aim, start, t, period);
        return true;
    }
    if s.hold > 0 {
        s.hold -= 1;
        let here = p.pos;
        straight_hits(ents, hash, cards, calib, p, here, dmg, fx, nb);
        return true;
    }
    let np = projectile_advance(calib.projectile_step, p.pos, p.aim, p.speed, &mut p.frac);
    if np == p.aim {
        return false;
    }
    p.pos = np;
    #[cfg(not(clash_plant = "range_shot_ends_on_first_hit"))]
    straight_hits(ents, hash, cards, calib, p, np, dmg, fx, nb);
    // PLANT (regression): a straight shot ends on the first unit it hits, as a homing shot does.
    #[cfg(clash_plant = "range_shot_ends_on_first_hit")]
    if straight_hits(ents, hash, cards, calib, p, np, dmg, fx, nb) {
        return false;
    }
    true
}

/// Advance every projectile; arrivals write into the damage buffer. Each
/// projectile depends only on its own state and the (unchanging during this
/// phase) entity positions, so processing order is irrelevant. A straight shot
/// (`step_straight`) writes its hits as it passes and keeps its own hit set, so no
/// projectile reads another's state.
#[allow(clippy::too_many_arguments)]
pub fn step_projectiles(
    ents: &Entities,
    hash: &SpatialHash,
    cards: &CardDb,
    calib: &Calib,
    projectiles: &mut Vec<Projectile>,
    dmg: &mut DamageBuffer,
    fx: &mut EffectBuffer,
    scratch: &mut Vec<u32>,
) {
    let rounding = calib.crown_rounding;
    projectiles.retain_mut(|p| {
        if p.fresh {
            // born this tick: its first step is next tick's (combat.PROJECTILE_LAUNCH)
            p.fresh = false;
            return true;
        }
        if p.straight.is_some() {
            return step_straight(ents, hash, cards, calib, p, dmg, fx, scratch);
        }
        let alive = ents.is_alive(p.target) && ents.hp[p.target.index as usize] > 0;
        if alive {
            p.aim = ents.pos[p.target.index as usize];
        }
        // combat.PROJECTILE_STEP: the aim is the target's position after it moved this tick.
        let np = projectile_advance(calib.projectile_step, p.pos, p.aim, p.speed, &mut p.frac);
        p.pos = np;
        if np != p.aim {
            return true;
        }
        if p.splash > 0 {
            splash(ents, hash, p.team, p.aim, p.splash, p.hits_air, p.hits_ground, p.damage, p.crown_pct, rounding, dmg, scratch);
            apply_attack_buff(ents, calib, p.buff, p.pulse, p.target, scratch, fx);
        } else if alive {
            let ti = p.target.index as usize;
            dmg.hits.push(Hit { target: p.target, amount: damage_against(ents.kind[ti], p.damage, p.crown_pct, rounding), ignores_hide: false });
            if let Some(b) = p.buff {
                fx.buffs.push(BuffHit { target: p.target, buff: b.buff, time_ms: b.time_ms, pulse_amount: p.pulse });
            }
        }
        false
    });
}

/// The whole ticks until projectile `p`'s hit resolves, read at the end of a tick: the steps it still
/// takes toward its target as the target stands now (`step_projectiles`' own step, combat.PROJECTILE_STEP's
/// `step`, arriving on the step that reaches it), plus one for a shot whose first step is next tick's
/// (`fresh`). A moving target's count is re-read each tick; for a still target it is the countdown fixed
/// at launch.
pub fn ticks_to_land(ents: &Entities, p: &Projectile, step: ProjectileStep) -> i32 {
    let aim = if ents.is_alive(p.target) { ents.pos[p.target.index as usize] } else { p.aim };
    let (mut pos, mut frac) = (p.pos, p.frac);
    let mut n = i32::from(p.fresh);
    // A shot always lands: either step moves it closer each tick and lands it within one step. The
    // bound only guards a zero speed, which no card ships.
    for _ in 0..100_000 {
        n += 1;
        let np = projectile_advance(step, pos, aim, p.speed, &mut frac);
        if np == aim {
            break;
        }
        pos = np;
    }
    n
}

/// THE DOOMED SET of targeting.DOOMED_TARGET_DROP = projectile_attackers, read from the projectiles in
/// flight at the tick's start (the state the previous tick ended in): a unit is doomed when the summed
/// damage of the shots flying at it, crown-tower arrows included, covers its hitpoints and shield, and
/// the shot of those that lands LAST does so within `limit_ms` (measured on client 15.535.29: 62 of 62
/// drops, 44 of 44 keeps while the lethal damage was 650-750 ms away).
///
/// A STRAIGHT SHOT (`Projectile::straight`, only under combat.RANGE_PROJECTILE = straight_to_range or
/// combat.MULTIPLE_PROJECTILES = client_fan) dooms nothing: it flies to its range point, not to its
/// target, and may miss it. Whether the client counts it is unmeasured.
pub fn doomed_by_shots_in_flight(ents: &Entities, projectiles: &[Projectile], rounding: CrownRounding, tick_ms: i32, limit_ms: i32, step: ProjectileStep) -> Vec<bool> {
    let cap = ents.capacity();
    let mut pending = vec![0i64; cap];
    let mut last_ms = vec![0i32; cap];
    for p in projectiles {
        if p.straight.is_some() || !ents.is_alive(p.target) {
            continue;
        }
        let t = p.target.index as usize;
        pending[t] += damage_against(ents.kind[t], p.damage, p.crown_pct, rounding) as i64;
        last_ms[t] = last_ms[t].max(ticks_to_land(ents, p, step) * tick_ms);
    }
    #[cfg(not(clash_plant = "doomed_eta_ignored"))]
    let within = |t: usize| last_ms[t] <= limit_ms;
    #[cfg(clash_plant = "doomed_eta_ignored")]
    let within = |_t: usize| {
        let _ = (limit_ms, &last_ms);
        true // PLANT (regression): damage lands whenever it lands.
    };
    (0..cap)
        .map(|t| pending[t] > 0 && pending[t] >= (ents.hp[t].max(0) + ents.shield[t].max(0)) as i64 && within(t))
        .collect()
}

/// Result of applying the buffer.
#[derive(Clone, Debug, Default)]
pub struct ResolveOut {
    /// Entities whose hp reached 0 this tick, ascending slot order.
    pub deaths: Vec<EntityId>,
    /// Teams whose king tower took damage this tick.
    pub king_hit: [bool; 2],
}

/// Apply every buffered hit in one pass. `sums` is scratch.
///
/// THE ONE CHOKE POINT FOR HIDE IMMUNITY (calibration hide.HIDDEN_IMMUNE_TO_DAMAGE,
/// `hidden_immune`): a hit on a building that is `HideState::Hidden` at resolve
/// time is dropped here, whatever wrote it -- an attack, a projectile, a spell, a
/// rolling Log, death damage -- unless the hit says `ignores_hide` (the lifetime
/// expiry). Dropping at resolve rather than at push time means every writer stays
/// ignorant of hide, and the state that decides is the one Target phase set this
/// tick (the hide pass runs before any damage is written).
///
/// THE SAME CHOKE POINT FOR THE DASH (calibration combat.DASH_ATTACK = client_dash;
/// entity.rs `dash_immune`): a hit on a unit that is dashing, or whose dash ended within
/// its DashImmuneToDamageTime, is dropped on the Resolve phase of `tick`, not deferred.
/// Measured on client 15.535.29 with tower arrows only; every writer is dropped alike.
pub fn resolve(ents: &mut Entities, dmg: &mut DamageBuffer, sums: &mut Vec<i64>, hidden_immune: bool, tick: u32) -> ResolveOut {
    let cap = ents.capacity();
    sums.clear();
    sums.resize(cap, 0);
    for h in dmg.hits.drain(..) {
        if !ents.is_alive(h.target) || h.amount <= 0 {
            continue;
        }
        #[cfg(not(clash_plant = "hidden_takes_damage"))]
        let immune = hidden_immune && !h.ignores_hide && ents.hide[h.target.index as usize] == HideState::Hidden;
        #[cfg(clash_plant = "hidden_takes_damage")]
        let immune = {
            // PLANT (regression): the choke point never consults the hide state.
            let _ = hidden_immune;
            false
        };
        if immune {
            continue;
        }
        #[cfg(not(clash_plant = "dash_not_immune"))]
        if ents.dash_immune(h.target.index as usize, tick) {
            continue;
        }
        #[cfg(clash_plant = "dash_not_immune")]
        let _ = tick; // PLANT: a dashing unit takes every hit.
        sums[h.target.index as usize] += h.amount as i64;
    }
    let mut out = ResolveOut::default();
    for (i, &s) in sums.iter().enumerate() {
        if s <= 0 || !ents.alive[i] {
            continue;
        }
        if ents.kind[i] == EntityKind::KingTower {
            out.king_hit[ents.team[i] as usize] = true;
        }
        if ents.shield[i] > 0 {
            ents.shield[i] = (ents.shield[i] as i64 - s).max(0) as i32;
        } else {
            ents.hp[i] = (ents.hp[i] as i64 - s).max(i32::MIN as i64) as i32;
        }
    }
    // Every live entity at or below zero dies, however it got there.
    for i in 0..cap {
        if ents.alive[i] && ents.hp[i] <= 0 {
            out.deaths.push(ents.id_of(i));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crown_tower_reduction_follows_the_registry_rounding() {
        // Floor (159 at 35% -> 55) is one candidate of calibration
        // combat.CROWN_TOWER_DAMAGE_ROUNDING, not the engine's default; the test
        // pins all three so the key is what decides.
        use CrownRounding::*;
        assert_eq!(damage_against(EntityKind::PrincessTower, 159, 35, Floor), 55); // 55.65
        assert_eq!(damage_against(EntityKind::PrincessTower, 159, 35, RoundHalfUp), 56);
        assert_eq!(damage_against(EntityKind::PrincessTower, 159, 35, CeilKeptShare), 56);
        assert_eq!(damage_against(EntityKind::PrincessTower, 151, 33, RoundHalfUp), 50); // 49.83
        assert_eq!(damage_against(EntityKind::PrincessTower, 151, 33, Floor), 49);
        assert_eq!(damage_against(EntityKind::KingTower, 140, 35, Floor), 49); // exact: all agree
        assert_eq!(damage_against(EntityKind::KingTower, 140, 35, CeilKeptShare), 49);
        assert_eq!(damage_against(EntityKind::Troop, 159, 35, CeilKeptShare), 159);
        assert_eq!(damage_against(EntityKind::Building, 159, 35, CeilKeptShare), 159, "Cannon and Tesla take full damage");
        // The spec's Supercell points under ceil_kept_share: Fireball 688 at 30% -> 207
        // (floor 206); Zap 192 at 30% -> 58 (floor 57).
        assert_eq!(damage_against(EntityKind::PrincessTower, 688, 30, CeilKeptShare), 207);
        assert_eq!(damage_against(EntityKind::PrincessTower, 688, 30, Floor), 206);
        assert_eq!(damage_against(EntityKind::PrincessTower, 192, 30, CeilKeptShare), 58);
    }
}
