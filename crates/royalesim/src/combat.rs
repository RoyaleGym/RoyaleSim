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

use crate::card::CardDb;
use crate::entity::{AttackPhase, EntityKind, Entities, HideState, SpatialHash};
use crate::fixed::{in_range_edge, Vec2};
use crate::path::advance;
use crate::state::{AttackCycle, Calib, ChargeLevelScaling, ChargedHitTiming, HitSpeedBuff, ProjectileLaunch, TargetBuffScope};
use crate::spell::EffectBuffer;
use crate::status::{BuffApply, BuffHit, Sel};
use crate::target::in_attack_range;
use crate::{EntityId, Team};

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

/// An in-flight homing projectile.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
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
/// SpecialRange and per-target ranges, LoadFirstHit (Sparky), the dash's hit.
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
        // combat.PROJECTILE_LAUNCH: the projectile is born ProjectileStartRadius from
        // the attacker's centre toward the target and does not move on the fire tick
        // (the live tower arrows: 299-300 from the centre on the launch frame, 600 per
        // tick from the next); the old arm starts it at the centre and steps it at once.
        let (pos, fresh) = match calib.projectile_launch {
            ProjectileLaunch::StartRadiusNextTick => {
                let d = ents.pos[ti].sub(ents.pos[a]);
                let len = crate::fixed::isqrt(d.len2()) as i32;
                let r = card.projectile_start_radius.min(len.max(0));
                let pos = if len > 0 {
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
            speed: p.speed * calib.projectile_speed_to_subtiles_per_tick,
            damage: amount,
            crown_pct: pct,
            splash: splash_r,
            hits_air: card.attacks_air,
            hits_ground: card.attacks_ground,
            frac: Vec2::default(),
            fresh,
            buff: atk_buff,
            pulse: atk_pulse,
            firer_card: Some(ents.card[a]),
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

/// Advance every projectile; arrivals write into the damage buffer. Each
/// projectile depends only on its own state and the (unchanging during this
/// phase) entity positions, so processing order is irrelevant.
pub fn step_projectiles(
    ents: &Entities,
    hash: &SpatialHash,
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
        let alive = ents.is_alive(p.target) && ents.hp[p.target.index as usize] > 0;
        if alive {
            p.aim = ents.pos[p.target.index as usize];
        }
        let (np, _) = advance(p.pos, p.aim, p.speed, &mut p.frac);
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
/// takes toward its target as the target stands now (`step_projectiles`' own `advance`, arriving on
/// the step that reaches it), plus one for a shot whose first step is next tick's (`fresh`). A moving
/// target's count is re-read each tick; for a still target it is the countdown fixed at launch.
pub fn ticks_to_land(ents: &Entities, p: &Projectile) -> i32 {
    let aim = if ents.is_alive(p.target) { ents.pos[p.target.index as usize] } else { p.aim };
    let (mut pos, mut frac) = (p.pos, p.frac);
    let mut n = i32::from(p.fresh);
    // A shot always lands: `advance` moves it at least `speed` closer each step. The bound only
    // guards a zero speed, which no card ships.
    for _ in 0..100_000 {
        n += 1;
        let (np, _) = advance(pos, aim, p.speed, &mut frac);
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
pub fn doomed_by_shots_in_flight(ents: &Entities, projectiles: &[Projectile], rounding: CrownRounding, tick_ms: i32, limit_ms: i32) -> Vec<bool> {
    let cap = ents.capacity();
    let mut pending = vec![0i64; cap];
    let mut last_ms = vec![0i32; cap];
    for p in projectiles {
        if !ents.is_alive(p.target) {
            continue;
        }
        let t = p.target.index as usize;
        pending[t] += damage_against(ents.kind[t], p.damage, p.crown_pct, rounding) as i64;
        last_ms[t] = last_ms[t].max(ticks_to_land(ents, p) * tick_ms);
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
pub fn resolve(ents: &mut Entities, dmg: &mut DamageBuffer, sums: &mut Vec<i64>, hidden_immune: bool) -> ResolveOut {
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
