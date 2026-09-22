//! The attack pipeline: windup (load_time) -> hit -> cooldown (rest of hit_speed).
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
use crate::state::{Calib, ChargeLevelScaling};
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
    for &v in scratch.iter() {
        let v = v as usize;
        if ents.team[v] == team || ents.hp[v] <= 0 {
            continue;
        }
        if if ents.flying[v] { !hits_air } else { !hits_ground } {
            continue;
        }
        if in_range_edge(center, ents.pos[v], radius, ents.radius[v]) {
            out.hits.push(Hit { target: ents.id_of(v), amount: damage_against(ents.kind[v], amount, crown_pct, rounding), ignores_hide: false });
        }
    }
}

/// What one entity's attack step decided. Applied by the caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttackStep {
    pub phase: AttackPhase,
    pub ms: i32,
    pub fired_at: Option<EntityId>,
}

/// Advance entity a's attack state by one tick, reading only.
pub fn attack_step(ents: &Entities, cards: &CardDb, calib: &Calib, a: usize, can_act: bool) -> AttackStep {
    let card = cards.get(ents.card[a]);
    let target = ents.target[a].filter(|t| ents.is_alive(*t));
    let mut phase = ents.attack_phase[a];
    let mut ms = ents.attack_ms[a];
    let load = card.load_time_ms.max(0);
    let cooldown = (card.hit_speed_ms - load).max(0);
    let in_range = |t: EntityId| {
        let ti = t.index as usize;
        in_attack_range(calib, ents.pos[a], card.range, ents.pos[ti], ents.radius[ti])
    };
    if !can_act {
        return AttackStep { phase, ms, fired_at: None };
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
                return AttackStep { phase: AttackPhase::Idle, ms: 0, fired_at: None };
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
                    _ => return AttackStep { phase: AttackPhase::Idle, ms: 0, fired_at: None },
                }
            }
        }
    }
    if phase == AttackPhase::Windup && ms >= load {
        if let Some(t) = target {
            return AttackStep { phase: AttackPhase::Cooldown, ms: ms - load, fired_at: Some(t) };
        }
    }
    AttackStep { phase, ms, fired_at: None }
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
    projectiles: &mut Vec<Projectile>,
    scratch: &mut Vec<u32>,
) {
    let card = cards.get(ents.card[a]);
    let ti = target.index as usize;
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
        projectiles.push(Projectile {
            team: ents.team[a],
            pos: ents.pos[a],
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
    } else {
        dmg.hits.push(Hit { target, amount: damage_against(ents.kind[ti], amount, pct, calib.crown_rounding), ignores_hide: false });
    }
}

/// Advance every projectile; arrivals write into the damage buffer. Each
/// projectile depends only on its own state and the (unchanging during this
/// phase) entity positions, so processing order is irrelevant.
pub fn step_projectiles(
    ents: &Entities,
    hash: &SpatialHash,
    rounding: CrownRounding,
    projectiles: &mut Vec<Projectile>,
    dmg: &mut DamageBuffer,
    scratch: &mut Vec<u32>,
) {
    projectiles.retain_mut(|p| {
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
        } else if alive {
            let ti = p.target.index as usize;
            dmg.hits.push(Hit { target: p.target, amount: damage_against(ents.kind[ti], p.damage, p.crown_pct, rounding), ignores_hide: false });
        }
        false
    });
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
