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
use crate::path::{advance, advance_client, native_in_frame};
use crate::state::{
    AttackCycle, Calib, ChargeLevelScaling, ChargedHitTiming, CustomFirstProjectile, HitBeyondCancelRange, HitSpeedBuff, MultipleProjectiles, ProjectileLaunch,
    ProjectileStep, RangeProjectile, SpawnPathfindBody, TargetBuffScope, VariableDamage,
};
use crate::spell::{forward_dy, push_from, EffectBuffer, SpellCtx};
use crate::status::{BuffApply, BuffHit, Sel};
use crate::target::{in_attack_range, walking_own_radius};
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
    /// THE THROWER OF A HOOK (calibration combat.SPECIAL_HOOK = client_hook_drag;
    /// `launch_hook`): Some only on a special projectile, which deals no damage and whose
    /// arrival hands (target, thrower) to Resolve to start the drag (`step_projectiles`).
    /// None on every other shot, and on every shot under the shipped not_read. `default` so a
    /// snapshot saved before it still loads; hashed only when Some.
    #[serde(default)]
    pub hook: Option<EntityId>,
    /// A SPARK CARRIER (calibration combat.SPAWN_PROJECTILE = client_spark_fan: the Firecracker's
    /// rocket): it flies to `aim` and never follows `target`, and where it lands it releases its
    /// card's sparks (`release_sparks`). None for every other shot, which is every shot under the
    /// shipped arm. Added after SNAPSHOT_FORMAT 20; absent in older snapshots = None, and hashed
    /// only when present.
    #[serde(default)]
    pub carrier: Option<Carrier>,
    /// The firer's (card, level) whose `projectile_area` this projectile leaves where it lands (the
    /// Heal Spirit's heal): an AreaRelease on the arrival tick. None for every other shot, and in a
    /// snapshot older than the field. In the state hash only when set.
    #[serde(default)]
    pub release: Option<(u16, i32)>,
    /// The firer's row sets ApplyBuffBeforeDamage (`CardDef::attack_buff_first`, the Mother Witch's): its
    /// buff lands before its damage (status.APPLY_BUFF_BEFORE_DAMAGE). In the state hash only on a shot
    /// whose buff has a death spawn, so every other shot hashes as before. `default` so a snapshot saved
    /// before it still loads.
    #[serde(default)]
    pub buff_first: bool,
    /// The firer's unified level, carried to the buff it lands (`BuffHit::src_level`). Hashed with
    /// `buff_first`, on the same shots only.
    #[serde(default)]
    pub src_level: i32,
    /// THE RUNE GIANT'S PROJECTILE (state.rs `launch_due_enchants`): Some only on the projectile that carries an
    /// enchant to a friend. It deals no damage: its landing on a live friend hands (friend, payload) to Resolve
    /// (`step_projectiles`), which puts the enchant on it. None on every other shot. `default` so a snapshot saved
    /// before it still loads; hashed only when Some.
    #[serde(default)]
    pub enchant: Option<EnchantPayload>,
    /// THE ENCHANT BONUS this shot was fired with (`enchant_bonus`): added to what it deals to each victim, `bonus`
    /// to a troop or a building and `bonus_crown` to a crown tower. 0 on every shot of an attacker that is not
    /// enchanted, or not on its bonus attack. `default`; hashed only when not 0.
    #[serde(default)]
    pub bonus: i32,
    #[serde(default)]
    pub bonus_crown: i32,
}

/// What the Rune Giant's projectile carries (`Projectile::enchant`): the unit that sent it, its card (whose
/// `EnchantDef` the enchant runs) and its level (the bonus is scaled at it).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct EnchantPayload {
    pub source: EntityId,
    pub card: u16,
    pub level: i32,
}

/// The state of a spark carrier (`Projectile::carrier`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Carrier {
    /// The shot's launch point: the sparks fan about the line from here to where it lands.
    pub from: Vec2,
    /// The firing card (a `CardDb` index), whose `spark` row the sparks are.
    pub card: u16,
    /// Each spark's damage: the row's Damage scaled at the attacker's level when it fired.
    pub damage: i32,
    /// Each spark's enchant bonus (`Projectile::bonus`), taken when the shot was fired. `default`; hashed only when
    /// not 0.
    #[serde(default)]
    pub bonus: i32,
    #[serde(default)]
    pub bonus_crown: i32,
}

/// AN ENCHANT BONUS (`enchant_bonus`): what one hit adds, to a troop or a building (`hit`) and to a crown tower
/// (`crown`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Bonus {
    pub hit: i32,
    pub crown: i32,
}

impl Bonus {
    /// The bonus against a victim of `kind`.
    #[inline]
    pub fn on(self, kind: EntityKind) -> i32 {
        if kind.is_crown_tower() {
            self.crown
        } else {
            self.hit
        }
    }
}

/// THE ENCHANT BONUS of attacker `a`'s current attack, as (its direct hits, its sparks), both zero unless `a` carries
/// an enchant and this attack is a bonus attack. The caller has already counted this attack (state.rs, the attack
/// pass). Measured on client 15.535.29:
/// - enchant.BONUS_ATTACKS = every_third_attack_from_enchant: the bonus lands on the 3rd, 6th, 9th ... attack after
///   the enchant (AttackAmount 3), for as long as it lasts;
/// - enchant.BONUS_LEVEL_SCALING = instigator_level: AddedDamage 86 scaled at the Rune Giant's level (+220 at 11,
///   +182 at 9), whatever the carrier's level;
/// - enchant.MULTIPLIER = per_mille_of_level1_then_scaled: a listed attacker takes that many thousandths of the
///   level-1 figure, truncated, then scaled (the Electro Wizard +110 a bolt, the Hunter +20 a pellet);
/// - enchant.CROWN_TOWER_BONUS = crown_column_no_percent: a crown tower takes AddedCrownTowerDamage scaled the same
///   way, and the attacker's crown percent is not applied to it (a hypothesis: the two columns are equal on the one
///   row, and the Knight's percent is 100).
pub fn enchant_bonus(ents: &Entities, cards: &CardDb, calib: &Calib, a: usize) -> (Bonus, Bonus) {
    use crate::state::{EnchantBonusAttacks, EnchantBonusLevel, EnchantCrownBonus, EnchantMultiplier};
    let none = (Bonus::default(), Bonus::default());
    let Some(s) = ents.enchant.get(a).copied().flatten() else { return none };
    let Some(def) = cards.get(s.card).enchant.as_ref() else { return none };
    let period = u32::from(def.period.max(1));
    #[cfg(not(any(clash_plant = "enchant_bonus_ignored", clash_plant = "enchant_bonus_first_three")))]
    let pays = match calib.enchant_bonus_attacks {
        EnchantBonusAttacks::EveryThirdFromEnchant => s.count > 0 && s.count % period == 0,
        EnchantBonusAttacks::FirstThree => s.count > 0 && s.count <= period,
    };
    #[cfg(clash_plant = "enchant_bonus_ignored")]
    let pays = {
        let _ = (calib.enchant_bonus_attacks, period);
        false // PLANT: an enchanted unit's attacks carry no bonus.
    };
    #[cfg(clash_plant = "enchant_bonus_first_three")]
    let pays = {
        let _ = calib.enchant_bonus_attacks;
        s.count > 0 && s.count <= period // PLANT: the bonus on the first AttackAmount attacks, then none.
    };
    if !pays {
        return none;
    }
    #[cfg(not(clash_plant = "enchant_scaled_by_carrier"))]
    let level = match calib.enchant_bonus_level {
        EnchantBonusLevel::Instigator => Some(s.level),
        EnchantBonusLevel::Carrier => Some(ents.level[a]),
        EnchantBonusLevel::Flat => None,
    };
    #[cfg(clash_plant = "enchant_scaled_by_carrier")]
    let level = {
        let _ = calib.enchant_bonus_level;
        Some(ents.level[a]) // PLANT: the bonus scaled at the carrier's level.
    };
    // A level-1 figure on the Rune Giant's ladder at `level`; a level the ladder does not hold (the carrier's, under
    // carrier_level) keeps the level-1 figure, as an attack buff's pulse does.
    let scale = |base: i32| match level {
        Some(l) => cards.scaled(s.card, l, base).unwrap_or(base),
        None => base,
    };
    let part = |added: i32, m: i32| -> i32 {
        #[cfg(clash_plant = "enchant_multiplier_after_scaling")]
        {
            let _ = calib.enchant_multiplier;
            return scale(added) * m / PER_MILLE_I32; // PLANT: the per mille taken of the scaled bonus.
        }
        #[cfg(clash_plant = "enchant_multiplier_percent")]
        {
            let _ = calib.enchant_multiplier;
            return scale(added * m / PERCENT as i32); // PLANT: the table's values read as percents.
        }
        #[allow(unreachable_code)]
        match calib.enchant_multiplier {
            EnchantMultiplier::Level1ThenScaled => scale(added * m / PER_MILLE_I32),
            EnchantMultiplier::OfScaled => scale(added) * m / PER_MILLE_I32,
        }
    };
    let card = cards.get(ents.card[a]);
    let bonus = |m: i32| -> Bonus {
        let hit = part(def.added, m);
        #[cfg(not(clash_plant = "crown_bonus_from_added_damage"))]
        let crown = match calib.enchant_crown_bonus {
            EnchantCrownBonus::CrownColumn => part(def.added_crown, m),
            EnchantCrownBonus::AddedWithAttackerPercent => damage_against(EntityKind::PrincessTower, hit, card.crown_tower_damage_percent, calib.crown_rounding),
        };
        #[cfg(clash_plant = "crown_bonus_from_added_damage")]
        let crown = {
            let _ = (calib.enchant_crown_bonus, card);
            hit // PLANT: a crown tower takes the AddedDamage bonus.
        };
        Bonus { hit, crown }
    };
    let (direct, spark) = def.per_mille(ents.card[a]);
    (bonus(direct), bonus(spark))
}

/// `crate::card::PER_MILLE`, as the divisor the bonus arithmetic uses.
const PER_MILLE_I32: i32 = crate::card::PER_MILLE;

/// Add bonus (`hit`, `crown`) to the hits a splash just wrote (`hits`), in place: on every victim, or under
/// enchant.SPLASH_BONUS = primary_target_only on `primary` alone. Measured on client 15.535.29 (every_victim): a
/// Valkyrie's bonus swing gave its bonus to both victims. One Hit per victim, never a second one.
fn add_splash_bonus(ents: &Entities, calib: &Calib, hits: &mut [Hit], b: Bonus, primary: EntityId) {
    if b == Bonus::default() {
        return;
    }
    #[cfg(not(clash_plant = "splash_bonus_primary_only"))]
    let every = calib.enchant_splash_bonus == crate::state::EnchantSplashBonus::EveryVictim;
    #[cfg(clash_plant = "splash_bonus_primary_only")]
    let every = {
        let _ = calib.enchant_splash_bonus;
        false // PLANT: only the target the attack was aimed at takes the bonus.
    };
    for h in hits.iter_mut() {
        if every || h.target == primary {
            h.amount += b.on(ents.kind[h.target.index as usize]);
        }
    }
}

/// The state of a straight shot (`Projectile::straight`). Distances SUBTILES. `Default` is a
/// one-way shot at the origin that holds nothing and has hit nothing (a spark is built from it).
#[derive(Clone, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
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
    /// THE SHOT IS GONE ON THE TICK IT HITS (calibration combat.PROJECTILE_COLLISIONS =
    /// client_columns on a CheckCollisions row: the Hunter's pellet). Every unit it reaches on
    /// that tick takes the hit; whether the client lets one pellet hit two units on one tick is
    /// unmeasured. False for every other shot, which flies on. Hashed only when set; absent in
    /// older snapshots = false.
    #[serde(default)]
    pub stop_on_hit: bool,
    /// THE THROWER OF A PINGPONG THROW (`period` > 0): while this shot is in flight its
    /// thrower's attack stands still, and it takes no new target (`throwers_out`). None on
    /// every one-way shot. Hashed only when set; absent in older snapshots = None.
    #[serde(default)]
    pub thrower: Option<EntityId>,
}

/// THE THROWERS WAITING FOR A PINGPONG THROW (combat.RANGE_PROJECTILE = straight_to_range on a
/// PingpongVisualTime row), by entity index: true for each live unit whose own throw is still
/// in `projectiles`. Empty when there is none, which is every tick of a battle without one.
///
/// Measured on client 15.535.29 (the Executioner's catalogue scenario, 3 throws) and in the
/// 16.402 corpus (the two Executioners of one battle, 11 throws): from the throw tick T through
/// T + 31 his attack progress stands at the value of the throw, and on T + 32 it takes its
/// first step again, so his throws are 49 ticks apart (247, 296, 345; 1403, 1452), not the 18
/// of HitSpeed 900. The throw is in flight for T .. T + 30 and is removed in the Projectile
/// phase of T + 31, which runs after that tick's Target and Attack phases, so "the throw is
/// still in the list" is the rule for all 32 ticks. While he waits he neither walks nor takes
/// a new target: a target killed on the way out reads none until T + 32 (359 -> 377 and
/// 1366 -> 1370), and he walks on T + 32 (377 and 1370).
pub fn throwers_out(ents: &Entities, projectiles: &[Projectile]) -> Vec<bool> {
    let mut out = Vec::new();
    #[cfg(clash_plant = "pingpong_thrower_unheld")]
    let projectiles: &[Projectile] = {
        let _ = projectiles;
        &[] // PLANT (regression): the thrower attacks on every HitSpeed while its throw is out, and walks off.
    };
    for p in projectiles {
        if let Some(t) = p.straight.as_ref().and_then(|s| s.thrower) {
            if ents.is_alive(t) {
                if out.is_empty() {
                    out = vec![false; ents.capacity()];
                }
                out[t.index as usize] = true;
            }
        }
    }
    out
}

/// combat.MULTIPLE_PROJECTILES = client_fan: the angle between neighbouring pellets of a fan,
/// degrees. Measured on client 15.535.29 on the Hunter (26 volleys: offsets 0, +7, -7, +14, -14,
/// +21, -21, +28, -28, +35 in creation order, to 0.06 degrees). The only row measured; another
/// MultipleProjectiles row may step otherwise.
pub const FAN_STEP_DEG: i64 = 7;

/// combat.MULTIPLE_PROJECTILES = client_fan: a pellet starts to move this many ticks after its
/// creation, drawn uniformly from the battle's Rng. Measured on client 15.535.29 on the Hunter:
/// 2 to 5 on all but one of about 250 pellets (a single 1). The client draws from its own
/// random stream, so the engine matches the range, not the order. The old arm of
/// combat.PROJECTILE_COLLISIONS; its new arm reads the row's RandomDelay instead (`release_hold`).
pub const FAN_RELEASE_TICKS: (i32, i32) = (2, 5);

/// combat.PROJECTILE_COLLISIONS = client_columns: the ticks a shot of a RandomDelay row stands
/// after its creation tick before its first step, ceil(U / TICK_MS) with U drawn uniformly from
/// 0..RandomDelay ms, one draw a shot, so its first step comes 1 + ceil(U / TICK_MS) ticks after
/// its creation. Measured on client 15.535.29 on the Hunter (RandomDelay 200, two battle seeds,
/// both sides): 136 pellet delays of 1-5 ticks, one of them 1 and 2-5 about equally often (36,
/// 28, 30 and 36 of the 130 in complete volleys), the spread of 1 + ceil(U / 50) for U uniform on
/// 0..200, whose 1 needs U = 0. The delays repeat by volley number on one seed, so they come from
/// the battle's random stream; that stream is the client's own, so the engine matches the law,
/// not the order. A row without RandomDelay stands 0 ticks and draws nothing.
fn release_hold(rng: &mut Rng, random_delay_ms: i32, tick_ms: i32) -> i32 {
    if random_delay_ms <= 0 {
        return 0;
    }
    let tick = tick_ms.max(1);
    #[cfg(not(clash_plant = "random_delay_unread"))]
    let u = rng.below(random_delay_ms as u32) as i32;
    #[cfg(clash_plant = "random_delay_unread")]
    let u = {
        let _ = rng;
        0 // PLANT (regression): the new arm reads no delay, so every shot steps on the tick after its creation.
    };
    (u + tick - 1) / tick
}

/// combat.DEPLOY_PROJECTILE = client_on_landing: the deploy projectile lands this many ticks
/// after the unit's first frame. Measured on client 15.535.29 on the Mega Knight (both sides:
/// first frame 259, the blow on 265). Whether it is a flight time (the row's Speed is 1000) or
/// a fixed delay is open; one card carries a deploy projectile.
pub const DEPLOY_PROJECTILE_DELAY_TICKS: i32 = 6;

/// combat.SPAWN_PROJECTILE = client_spark_fan: the angle between neighbouring sparks of one landing,
/// degrees; the fan is centred on the carrier's flight line (`release_sparks`). Measured on client
/// 15.535.29 on the Firecracker (2 landings, 10 sparks: -32, -16, 0, +16, +32 to 0.05 degrees).
/// FirecrackerExplosion's SpawnRadius 80 over its SpawnCount 5 is also 16; one row cannot tell a
/// constant from that reading, so the constant is what is measured.
pub const SPARK_STEP_DEG: i64 = 16;

/// ONE TICK OF A FLYING PROJECTILE, a troop's, a building's or a crown tower's shot or a
/// spell's flight, under calibration combat.PROJECTILE_STEP. Returns the new position, `aim`
/// itself on the tick it arrives. `speed` is SUBTILES per tick. `team` is the projectile's
/// owner: the native arm reads positions on the native grid in that team's frame.
#[inline]
pub fn projectile_advance(step: ProjectileStep, pos: Vec2, aim: Vec2, speed: i32, frac: &mut Vec2, team: Team) -> Vec2 {
    match step {
        ProjectileStep::FractionCarry => advance(pos, aim, speed, frac).0,
        // client_native_truncated: path.rs `advance_client`, in native units, nothing carried.
        #[cfg(not(clash_plant = "projectile_step_carries_fraction"))]
        ProjectileStep::ClientNativeTruncated => {
            *frac = Vec2::default();
            advance_client(pos, aim, speed / K, team)
        }
        // PLANT (regression): the new arm steps exactly Speed and carries the remainder.
        #[cfg(clash_plant = "projectile_step_carries_fraction")]
        ProjectileStep::ClientNativeTruncated => {
            let _ = team;
            advance(pos, aim, speed, frac).0
        }
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
/// isqrt(v.v))`. `src` and `tgt` are read on the native grid in `team`'s frame (path.rs
/// `native_in_frame`), so a Red shot starts at the rotation of a Blue one.
fn fan_aim(src: Vec2, tgt: Vec2, dist: i32, deg: i64, team: Team) -> Vec2 {
    let (sx, sy) = (native_in_frame(src.x, team), native_in_frame(src.y, team));
    let (mut vx, mut vy) = (native_in_frame(tgt.x, team) - sx, native_in_frame(tgt.y, team) - sy);
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
/// on the client at the value one tick before it; that is open and not modelled. `origin` and `apex` are read on the
/// native grid in the thrower's frame (path.rs `native_in_frame`), so a Red throw is the rotation of a Blue one.
fn pingpong_pos(origin: Vec2, apex: Vec2, start: i32, t: i32, period: i32, team: Team) -> Vec2 {
    let (ox, oy) = (native_in_frame(origin.x, team), native_in_frame(origin.y, team));
    let (ux, uy) = (native_in_frame(apex.x, team) - ox, native_in_frame(apex.y, team) - oy);
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
        if if ents.in_air(v) { !hits_air } else { !hits_ground } {
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
/// per-target ranges. The dash's hit is not this cycle's: state.rs `phase_path16402` lands it
/// (combat.DASH_ATTACK). LoadFirstHit (the Sparky) is combat.LOAD_FIRST_HIT's: under
/// load_time_from_deploy_end the load timer leaves the deploy at LoadTime (state.rs
/// `load_first_hit_on_deployed`) and this same entry formula times the first launch from the
/// deploy end. SpecialRange (the Fisherman's hook) is combat.SPECIAL_HOOK's, which runs in
/// place of this step while the special is under way (state.rs `special_step`).
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
    // targeting.VARIABLE_DAMAGE_WALK_REACH: a unit that walked on the previous tick (it still holds a walking goal,
    // entity.rs `route_goal`) is gated at the reach it walks to, which adds no own radius for an Inferno Dragon under
    // client16402_no_own_radius_walking (target.rs `walking_own_radius`); a standing one at Range + both radii.
    let own = if ents.route_goal[a].is_some() { walking_own_radius(calib, card, ents.radius[a]) } else { ents.radius[a] };
    let in_range = in_attack_range(calib, ents.pos[a], card.range, own, ents.pos[ti], ents.radius[ti]);
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

/// Does attacker `a`'s swing at entity `ti` land as its attack selector's MELEE entry (card.rs `AttackSelectDef`)?
/// The entry chosen at the swing's start (entities `attack_seq`, state.rs `select_attack`), unless the target is now
/// in the air and the melee branch takes ground targets only. The one test `fire` and state.rs `phase_attack_for`
/// share, so the hit and the bookkeeping around it never disagree about which entry fired.
#[inline]
pub fn melee_chosen(ents: &Entities, a: usize, ti: usize, sel: crate::card::AttackSelectDef) -> bool {
    ents.attack_seq[a] == 1 && melee_target_ok(ents, ti, sel)
}

/// THE SELECTOR'S GROUND CLAUSE (`target_is_ground`; card.rs `AttackSelectDef::ground_only`): an air target never
/// takes the melee entry. Measured on client 15.535.29: 24 of 24 swings at a Lava Hound and its Pups were shots,
/// one of them with the Hound overhead at 1248. Read by state.rs `select_attack` and by `melee_chosen`.
#[inline]
pub fn melee_target_ok(ents: &Entities, ti: usize, sel: crate::card::AttackSelectDef) -> bool {
    #[cfg(clash_plant = "bayonet_on_air")]
    {
        let _ = (ents, ti, sel);
        return true; // PLANT: the ground clause dropped.
    }
    #[allow(unreachable_code)]
    !(sel.ground_only && ents.in_air(ti))
}

/// Turn a completed windup into damage: a projectile, or an instant hit.
///
/// `rng` is drawn only for the release delays of a fan (combat.MULTIPLE_PROJECTILES =
/// client_fan), so a battle under the shipped arms consumes nothing from it here.
/// `bolts` are the extra bolts of this attack (combat.MULTIPLE_TARGETS =
/// client_bolts_per_target, state.rs `extra_bolts`), empty under the shipped arm.
/// `tick` is the tick being run: a CheckCollisions shot's creation-tick test reads which of the
/// hits already written this tick Resolve will land (`killed_this_tick`).
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
    tick: u32,
) {
    let card = cards.get(ents.card[a]);
    let ti = target.index as usize;
    // THE ENCHANT BONUS of this attack (`enchant_bonus`): zero for every attacker that carries no enchant and on every
    // attack of one that is not its bonus attack. It rides every hit the attack deals, one Hit per victim.
    let (direct, spark) = enchant_bonus(ents, cards, calib, a);
    // THE ATTACK'S BUFF (card.rs `CardDef::attack_buff`): a projectile carries it to
    // its arrival tick, an instant hit applies it here. The per-pulse amount is the
    // ATTACKER's level-scaled figure, computed once, because the victim does not know
    // the attacker's level.
    let atk_buff = card.attack_buff;
    let atk_pulse = match atk_buff {
        None => 0,
        // An unscalable level falls back to the level-1 figure, as before
        // status.BUFF_PULSE_AMOUNT had a second arm; the closure cannot fail.
        Some(b) => cards.buffs[b.buff as usize]
            .pulse_amount(calib.buff_pulse_amount, |m| {
                Ok::<i32, ()>(cards.scaled(ents.card[a], ents.level[a], m).unwrap_or(m))
            })
            .unwrap_or(0),
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
        _ => stage_damage(ents, cards, calib, a),
    };
    let pct = card.crown_tower_damage_percent;
    let splash_r = if card.area_damage_radius > 0 {
        card.area_damage_radius
    } else {
        card.projectile.map(|p| p.radius).unwrap_or(0)
    };
    // THE ATTACK SELECTOR'S MELEE ENTRY (card.rs `AttackSelectDef`; the Three Musketeers' bayonet), chosen for this
    // swing by state.rs `select_attack`: an instant hit of its own damage on the target and no projectile. Measured on
    // client 15.535.29: 314 = 123 x 256 % at level 11 on a Giant, landing on the frame a ranged member of the same
    // LoadTime would shoot. Its crown-tower share is the card's own (100); a bayonet on a crown tower is unmeasured. A
    // melee choice whose target is now flying (a retarget within the swing) fires the projectile: the filter's ground
    // clause cannot hold for it. An enchanted member's bonus attack carries the enchant bonus on this hit as on any
    // direct hit (`enchant_bonus`; a bayonet under an enchant is unmeasured).
    if let Some(sel) = card.attack_select {
        if melee_chosen(ents, a, ti, sel) {
            let amount = cards.scaled(ents.card[a], ents.level[a], sel.melee_damage).expect("level validated at spawn");
            dmg.hits.push(Hit { target, amount: damage_against(ents.kind[ti], amount, pct, calib.crown_rounding) + direct.on(ents.kind[ti]), ignores_hide: false });
            if let Some(b) = atk_buff {
                fx.buffs.push(BuffHit { src_level: ents.level[a], before_damage: card.attack_buff_first, ..BuffHit::plain(target, b.buff, b.time_ms, atk_pulse) });
            }
            return;
        }
    }
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
                // combat.PROJECTILE_COLLISIONS = client_columns: the row's CheckCollisions,
                // ProjectileStartExtraRadius and RandomDelay; under not_read none of them.
                let columns = calib.projectile_collisions == crate::state::ProjectileCollisions::ClientColumns;
                #[cfg(not(clash_plant = "check_collisions_fly_on"))]
                let stop_on_hit = columns && rs.check_collisions;
                #[cfg(clash_plant = "check_collisions_fly_on")]
                let stop_on_hit = {
                    let _ = rs.check_collisions;
                    false // PLANT (regression): the new arm lets a CheckCollisions shot fly on after its hit, as not_read.
                };
                #[cfg(not(clash_plant = "start_extra_radius_unread"))]
                let start_extra = if columns { rs.start_extra } else { 0 };
                #[cfg(clash_plant = "start_extra_radius_unread")]
                let start_extra = {
                    let _ = rs.start_extra;
                    0 // PLANT (regression): the new arm's creation-tick test reaches ProjectileRadius alone.
                };
                for k in 0..pellets {
                    // Each pellet of a fan is aimed at ProjectileRange on its own offset from
                    // the bearing to the target and released 2-5 ticks after its creation
                    // (`FAN_RELEASE_TICKS`); a single shot on the bearing, released at once.
                    // Under combat.PROJECTILE_COLLISIONS = client_columns a RandomDelay row's
                    // shot is released by its own draw instead (`release_hold`).
                    let aim = fan_aim(src, tgt, rs.range, fan_offset_deg(k), team);
                    #[cfg(not(clash_plant = "fan_pellets_unheld"))]
                    let hold = if columns {
                        release_hold(rng, rs.random_delay_ms, calib.tick_ms)
                    } else if pellets > 1 {
                        rng.range(FAN_RELEASE_TICKS.0, FAN_RELEASE_TICKS.1) - 1
                    } else {
                        0
                    };
                    #[cfg(clash_plant = "fan_pellets_unheld")]
                    let hold = {
                        let _ = (&rng, FAN_RELEASE_TICKS, release_hold);
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
                        carrier: None,
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
                            stop_on_hit,
                            // A pingpong throw holds its thrower until it is back (`throwers_out`).
                            thrower: if period > 0 { Some(ents.id_of(a)) } else { None },
                        }),
                        hook: None,
                        release: None,
                        buff_first: card.attack_buff_first,
                        src_level: ents.level[a],
                        enchant: None,
                        bonus: direct.hit,
                        bonus_crown: direct.crown,
                    };
                    // THE CREATION TICK'S TEST (a one-way shot born on its launch point): the
                    // launch point against the start-of-tick positions, measured on client
                    // 15.535.29 (a boulder hit 1428 from its launch point on the tick it was
                    // created). A pingpong throw first tests on the next tick, against this
                    // tick's position (`step_straight`).
                    // combat.PROJECTILE_COLLISIONS = client_columns: this test alone reaches
                    // ProjectileRadius + ProjectileStartExtraRadius (`start_extra`), measured on
                    // client 15.535.29 on the Hunter: whole volleys landed on their creation tick on
                    // a Knight 1280-1295 and a Giant 1555 from the launch point, and none on a Knight
                    // 2009 or a Giant 1834 out, so the reach less the victim's radius lies in
                    // 805-1084; 300 + 650 = 950. A CheckCollisions shot that hits here is gone
                    // before it is ever seen. The pellets are tested in creation order, each
                    // after the hits of the ones before it: a pellet whose victim those hits have
                    // killed passes it (`killed_this_tick`) and flies on.
                    if fresh && period == 0 {
                        if let Some(s) = shot.straight.as_mut() {
                            s.reach += start_extra;
                        }
                        let hit = straight_hits(ents, hash, cards, calib, &mut shot, pos, dmg, fx, scratch, tick);
                        if let Some(s) = shot.straight.as_mut() {
                            s.reach -= start_extra;
                        }
                        if hit && stop_on_hit {
                            continue;
                        }
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
        let (pos, fresh) = launch_point(ents, calib, card.projectile_start_radius, a, ti);
        // ProjectileYOffset (the hero rows alone, `CardDef::projectile_y_offset`): the shot starts that much further
        // along its owner's forward axis. Measured on client 16.402: the Hero Musketeer's shots first appear at 1800 x
        // the aim + 300 x her side's forward (100 launches over five captures), the base Musketeer's at 450 x the aim.
        let pos = if fresh && card.projectile_y_offset != 0 { Vec2::new(pos.x, pos.y + crate::spell::forward_dy(ents.team[a]) * card.projectile_y_offset) } else { pos };
        // combat.SPAWN_PROJECTILE = client_spark_fan: a card whose shot releases sparks
        // (`CardDef::spark`, the Firecracker's rocket) fires a CARRIER, aimed at the target's
        // start-of-tick centre and flown there whatever the target does (measured on client
        // 15.535.29: both rockets were aimed at the Knight's centre on the tick before the launch,
        // to the unit, and landed there); `release_sparks` puts the sparks down where it lands.
        #[cfg(not(clash_plant = "sparks_unspawned"))]
        let sparks_arm = calib.spawn_projectile == crate::state::SpawnProjectile::ClientSparkFan;
        #[cfg(clash_plant = "sparks_unspawned")]
        let sparks_arm = {
            let _ = calib.spawn_projectile;
            false // PLANT (regression): the new arm's shot lands and releases nothing, as not_read.
        };
        let carrier = match (sparks_arm, custom, card.spark) {
            (true, None, Some(sp)) => Some(Carrier {
                from: pos,
                card: ents.card[a],
                damage: cards.scaled(ents.card[a], ents.level[a], sp.damage).expect("level validated at spawn"),
                bonus: spark.hit,
                bonus_crown: spark.crown,
            }),
            _ => None,
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
            hook: None,
            // A carrier deals nothing itself: its sparks carry the bonus (`Carrier::bonus`).
            bonus: if carrier.is_some() { 0 } else { direct.hit },
            bonus_crown: if carrier.is_some() { 0 } else { direct.crown },
            carrier,
            release: card.projectile_area.as_ref().map(|_| (ents.card[a], ents.level[a])),
            buff_first: card.attack_buff_first,
            src_level: ents.level[a],
            enchant: None,
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
        let from = dmg.hits.len();
        splash(ents, hash, ents.team[a], centre, splash_r, card.attacks_air, card.attacks_ground, amount, pct, calib.crown_rounding, dmg, scratch);
        add_splash_bonus(ents, calib, &mut dmg.hits[from..], direct, target);
        apply_attack_buff(ents, calib, atk_buff, atk_pulse, (ents.level[a], card.attack_buff_first), target, scratch, fx);
    } else {
        // combat.HIT_BEYOND_CANCEL_RANGE = no_damage: a single-target direct hit whose target stands more than
        // targeting.LOGIC_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE past the attacker's reach (Range + both collision radii
        // under targeting.ATTACK_RANGE_RULE) deals no damage and applies no buff. The attack pass runs before the
        // move, so these are the start-of-tick positions. The cycle and the target are the caller's and are kept.
        // Measured on client 15.535.29: 80 hits up to 1149 past reach dealt their damage, the three from 1507 to
        // 2697 past it dealt none.
        #[cfg(not(clash_plant = "hit_beyond_cancel_deals_damage"))]
        let void = calib.hit_beyond_cancel_range == HitBeyondCancelRange::NoDamage
            && !in_attack_range(calib, ents.pos[a], card.range + calib.cancel_hit_from_long_distance_range, ents.radius[a], ents.pos[ti], ents.radius[ti]);
        #[cfg(clash_plant = "hit_beyond_cancel_deals_damage")]
        let void = false; // PLANT (regression): the far hit deals its full damage under the new arm too.
        if !void {
            dmg.hits.push(Hit { target, amount: damage_against(ents.kind[ti], amount, pct, calib.crown_rounding) + direct.on(ents.kind[ti]), ignores_hide: false });
            if let Some(b) = atk_buff {
                fx.buffs.push(BuffHit { src_level: ents.level[a], before_damage: card.attack_buff_first, ..BuffHit::plain(target, b.buff, b.time_ms, atk_pulse) });
            }
        }
        // combat.MULTIPLE_TARGETS = client_bolts_per_target: every other bolt of the attack is
        // the whole hit again, damage and buff, on its own victim (state.rs `extra_bolts`
        // chose them; under AllTargetsHit a bolt with no other enemy is `target` again).
        for &b in bolts {
            // A bolt padded onto the target itself is that far hit again, and is voided with it
            // (combat.HIT_BEYOND_CANCEL_RANGE); every other bolt's victim was chosen within reach.
            if void && b == target {
                continue;
            }
            let bi = b.index as usize;
            dmg.hits.push(Hit { target: b, amount: damage_against(ents.kind[bi], amount, pct, calib.crown_rounding) + direct.on(ents.kind[bi]), ignores_hide: false });
            if let Some(bf) = atk_buff {
                fx.buffs.push(BuffHit { src_level: ents.level[a], before_damage: card.attack_buff_first, ..BuffHit::plain(b, bf.buff, bf.time_ms, atk_pulse) });
            }
        }
    }
}

/// THE DAMAGE OF AN ORDINARY (uncharged) HIT: the entity's scaled Damage or, under
/// calibration combat.VARIABLE_DAMAGE = client16402_attack_progress_stages and on a card
/// that carries the ramp (card.rs `VariableDamageDef`), the stage its attack progress on the
/// current target has reached. Measured on the 16.402 corpus: an Inferno Tower at level 11
/// hits for 43 below 2000 of progress, 158 below 4000 and 847 from there on (Damage,
/// VariableDamage2 and VariableDamage3 at 256 per cent), an Inferno Dragon 35 / 120 / 422.
///
/// THE PROGRESS IS combat.ATTACK_CYCLE's COUNTER, `attack_ms`, which the caller
/// (state.rs `phase_attack`) has already set to this hit's value: a hit does not reset it,
/// so the first hit reads 400 and every later one 400 more. It restarts where that counter
/// restarts: a new target (the post-kill wait zeroes it, a switch resets a swing) and, under
/// the same key, a stun (state.rs `apply_effects`). The old arm is Damage on every hit.
fn stage_damage(ents: &Entities, cards: &CardDb, calib: &Calib, a: usize) -> i32 {
    #[cfg(not(clash_plant = "variable_damage_first_stage"))]
    let ramps = calib.variable_damage == VariableDamage::AttackProgressStages;
    #[cfg(clash_plant = "variable_damage_first_stage")]
    let ramps = {
        // PLANT (regression): the Inferno deals its first-stage Damage on every hit.
        let _ = calib.variable_damage == VariableDamage::AttackProgressStages;
        false
    };
    let Some(vd) = cards.get(ents.card[a]).variable_damage.filter(|_| ramps) else {
        return ents.damage[a];
    };
    let progress = ents.attack_ms[a];
    let base = if progress < vd.time1_ms {
        return ents.damage[a];
    } else if progress < vd.time1_ms + vd.time2_ms {
        vd.damage2
    } else {
        vd.damage3
    };
    // A level-1 stat like Damage; the level was validated at spawn.
    cards.scaled(ents.card[a], ents.level[a], base).expect("level validated at spawn")
}

/// Where attacker `a`'s shot at entity `ti` is born, and whether its first step is the
/// next tick's (combat.PROJECTILE_LAUNCH): ProjectileStartRadius (`start_radius`, subtiles)
/// from the attacker's centre toward the target, first step the next tick (the live tower
/// arrows: 299-300 from the centre on the launch frame, 600 per tick from the next); the old
/// arm starts it at the centre and steps it at once. `fire` and `launch_hook` both read it.
fn launch_point(ents: &Entities, calib: &Calib, start_radius: i32, a: usize, ti: usize) -> (Vec2, bool) {
    match calib.projectile_launch {
        ProjectileLaunch::StartRadiusNextTick => {
            let d = ents.pos[ti].sub(ents.pos[a]);
            let len = crate::fixed::isqrt(d.len2()) as i32;
            let r = start_radius.min(len.max(0));
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
    }
}

/// THE HOOK (calibration combat.SPECIAL_HOOK = client_hook_drag; card.rs `SpecialDef`;
/// state.rs `special_step`): thrower `a` launches its special projectile at `target`, born
/// like any shot (`launch_point`) and flying at the special projectile's Speed (`speed_raw`,
/// converted like every projectile Speed). It deals no damage: its arrival hands (target,
/// thrower) to Resolve (`step_projectiles`), which starts the drag. Measured on client
/// 15.535.29 (5 runs): the hook flies at about 800 a tick and the Knight it lands on loses
/// no hitpoints to it.
pub fn launch_hook(ents: &Entities, cards: &CardDb, calib: &Calib, a: usize, target: EntityId, speed_raw: i32, projectiles: &mut Vec<Projectile>) {
    let ti = target.index as usize;
    let (pos, fresh) = launch_point(ents, calib, cards.get(ents.card[a]).projectile_start_radius, a, ti);
    projectiles.push(Projectile {
        team: ents.team[a],
        pos,
        target,
        aim: ents.pos[ti],
        speed: speed_raw * calib.projectile_speed_to_subtiles_per_tick,
        damage: 0,
        crown_pct: PERCENT as i32,
        splash: 0,
        hits_air: false,
        hits_ground: true,
        frac: Vec2::default(),
        fresh,
        buff: None,
        pulse: 0,
        firer_card: Some(ents.card[a]),
        straight: None,
        hook: Some(ents.id_of(a)),
        carrier: None,
        release: None,
        buff_first: false,
        src_level: ents.level[a],
        enchant: None,
        bonus: 0,
        bonus_crown: 0,
    });
}

/// Buffer `buff` on everything the hit it rode landed on. `scratch` is the victim
/// list `splash` just filled, so the buff lands on exactly the units the damage did
/// (calibration status.TARGET_BUFF_ON_SPLASH = whole_splash); under
/// `primary_target_only` only `target` takes it. `firer` is the attacker's unified level and whether
/// its row lands the buff before the damage (`BuffHit::src_level`, `before_damage`).
#[allow(clippy::too_many_arguments)]
fn apply_attack_buff(
    ents: &Entities,
    calib: &Calib,
    buff: Option<BuffApply>,
    pulse: i32,
    firer: (i32, bool),
    target: EntityId,
    scratch: &[u32],
    fx: &mut EffectBuffer,
) {
    let Some(b) = buff else { return };
    let (src_level, before_damage) = firer;
    match calib.target_buff_on_splash {
        TargetBuffScope::WholeSplash => {
            for &v in scratch {
                fx.buffs.push(BuffHit { src_level, before_damage, ..BuffHit::plain(ents.id_of(v as usize), b.buff, b.time_ms, pulse) });
            }
        }
        TargetBuffScope::PrimaryTargetOnly => {
            fx.buffs.push(BuffHit { src_level, before_damage, ..BuffHit::plain(target, b.buff, b.time_ms, pulse) });
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
/// A shot with `stop_on_hit` also passes a unit the hits already written this tick have killed
/// (`killed_this_tick`, read at `tick`), so the pellets after a kill fly on.
/// Every shot passes a body no hit lands on (`untouchable_now`: a unit under ground, a hidden
/// building) and an attached rider, as a target scan does: none of them is hit, and none joins
/// the shot's hit set, so a shot that stops on its first hit flies on through them.
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
    tick: u32,
) -> bool {
    let (team, damage, crown_pct, hits_air, hits_ground, buff, pulse) = (p.team, p.damage, p.crown_pct, p.hits_air, p.hits_ground, p.buff, p.pulse);
    let (src_level, before_damage) = (p.src_level, p.buff_first);
    // The enchant bonus the shot was fired with (`enchant_bonus`), on each unit it hits.
    let bonus = Bonus { hit: p.bonus, crown: p.bonus_crown };
    let Some(s) = p.straight.as_mut() else { return false };
    let ctx = SpellCtx { ents, hash, cards, calib, steps: &[], tick };
    // What `resolve` drops a hit on (`untouchable_now`), read from the same keys.
    #[cfg(not(clash_plant = "straight_shot_meets_under_ground"))]
    let underground_immune = calib.spawn_pathfind_body == SpawnPathfindBody::Untouchable;
    #[cfg(clash_plant = "straight_shot_meets_under_ground")]
    let underground_immune = false; // PLANT (regression): a straight shot stops on a unit under ground.
    #[cfg(not(clash_plant = "straight_shot_meets_hidden"))]
    let hidden_immune = calib.hide_hidden_immune;
    #[cfg(clash_plant = "straight_shot_meets_hidden")]
    let hidden_immune = false; // PLANT (regression): a straight shot stops on a hidden building.
    hash.neighbours_within(ents, at, s.reach + hash.max_radius(), nb);
    let mut any = false;
    for &v in nb.iter() {
        let v = v as usize;
        if !ents.alive[v] || ents.hp[v] <= 0 || (s.only_enemies && ents.team[v] == team) {
            continue;
        }
        if if ents.in_air(v) { !hits_air } else { !hits_ground } {
            continue;
        }
        // rider.TARGETABLE_WHILE_ATTACHED = untargetable_immune: a straight shot passes an attached rider.
        if crate::target::rider_untouchable(calib, ents, v) {
            continue;
        }
        // movement.SPAWN_PATHFIND_BODY = untouchable and hide.HIDDEN_IMMUNE_TO_DAMAGE: it passes a unit
        // under ground and a hidden building too. A Hunter's pellet that met a tunnelling Miner or an idle
        // Tesla used to stop on it, and `resolve` then dropped the hit, so the pellet was spent on nothing.
        if untouchable_now(ents, v, hidden_immune, underground_immune, false) {
            continue;
        }
        if !in_range_edge(at, ents.pos[v], s.reach, ents.radius[v]) {
            continue;
        }
        // combat.PROJECTILE_COLLISIONS = client_columns: a CheckCollisions shot does not collide
        // with a unit this tick's earlier hits have already killed; it flies on past it.
        #[cfg(not(clash_plant = "kill_inside_tick_unread"))]
        if s.stop_on_hit && killed_this_tick(ents, dmg, calib.hide_hidden_immune, tick, v) {
            continue;
        }
        #[cfg(clash_plant = "kill_inside_tick_unread")]
        let _ = (tick, killed_this_tick); // PLANT (regression): every pellet in reach stops on a unit killed earlier this tick.
        let id = ents.id_of(v);
        match s.hit.binary_search(&id) {
            Ok(_) => continue,
            Err(k) => s.hit.insert(k, id),
        }
        any = true;
        dmg.hits.push(Hit { target: id, amount: damage_against(ents.kind[v], damage, crown_pct, calib.crown_rounding) + bonus.on(ents.kind[v]), ignores_hide: false });
        if let Some(b) = buff {
            fx.buffs.push(BuffHit { src_level, before_damage, ..BuffHit::plain(id, b.buff, b.time_ms, pulse) });
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

/// THE KILL INSIDE A TICK (calibration combat.PROJECTILE_COLLISIONS = client_columns, read by a
/// `stop_on_hit` shot in `straight_hits`): whether the hits already written against unit `v` this
/// tick take it to 0 hp when `resolve` lands them. The engine buffers a tick's damage for Resolve;
/// the client applies each hit as it happens, so a pellet tested after the ones that killed a unit
/// finds it dead and flies on. Measured on client 15.535.29 on the Hunter, the one volley recorded
/// meeting a victim that fewer than 10 pellets kill (one scene, both sides): a point-blank volley,
/// all 10 pellets within reach of a Knight with 347 hp. The first five in creation order (offsets
/// 0, +7, -7, +14, -14) hit it for 420 on the creation tick and were never seen; the other five
/// (+21, -21, +28, -28, +35) were seen at the launch point on that tick and flew on to their range.
///
/// The sum is `resolve`'s: a hit Resolve drops (on a hidden building, on a unit inside its dash
/// immunity at `tick`) counts nothing, and a unit with a shield is never killed by the tick's hits,
/// because a shield takes the whole sum. Every hit in the buffer counts, whatever wrote it; only
/// pellets killing for pellets is measured. Whether a pellet passes a victim killed this tick by
/// something else, and in which order the client runs the tick's other hits, are unmeasured.
fn killed_this_tick(ents: &Entities, dmg: &DamageBuffer, hidden_immune: bool, tick: u32, v: usize) -> bool {
    if ents.shield[v] > 0 || ents.dash_immune(v, tick) {
        return false;
    }
    let id = ents.id_of(v);
    let hidden = hidden_immune && ents.hide[v] == HideState::Hidden;
    let dealt: i64 = dmg.hits.iter().filter(|h| h.target == id && h.amount > 0 && (!hidden || h.ignores_hide)).map(|h| h.amount as i64).sum();
    dealt >= ents.hp[v] as i64
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
///   * A shot with `stop_on_hit` (combat.PROJECTILE_COLLISIONS = client_columns on a
///     CheckCollisions row) is gone after any of these tests that hits, and a test passes a
///     unit this tick's earlier hits have killed (`killed_this_tick`, at `tick`).
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
    tick: u32,
) -> bool {
    let Some(s) = p.straight.as_mut() else { return false };
    // combat.PROJECTILE_COLLISIONS = client_columns on a CheckCollisions row: the shot is gone on
    // the tick it hits (measured on client 15.535.29: each of 134 Hunter pellets that hit a Knight
    // or a Giant in 7 runs was last seen the tick before the hit, and every drop is 84 a pellet gone).
    let stop = s.stop_on_hit;
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
        let hit = straight_hits(ents, hash, cards, calib, p, prev, dmg, fx, nb, tick);
        p.pos = pingpong_pos(origin, p.aim, start, t, period, p.team);
        return !(hit && stop);
    }
    if s.hold > 0 {
        s.hold -= 1;
        let here = p.pos;
        let hit = straight_hits(ents, hash, cards, calib, p, here, dmg, fx, nb, tick);
        return !(hit && stop);
    }
    let np = projectile_advance(calib.projectile_step, p.pos, p.aim, p.speed, &mut p.frac, p.team);
    if np == p.aim {
        return false;
    }
    p.pos = np;
    #[cfg(not(clash_plant = "range_shot_ends_on_first_hit"))]
    let hit = straight_hits(ents, hash, cards, calib, p, np, dmg, fx, nb, tick);
    // PLANT (regression): a straight shot ends on the first unit it hits, as a homing shot does.
    #[cfg(clash_plant = "range_shot_ends_on_first_hit")]
    let hit = if straight_hits(ents, hash, cards, calib, p, np, dmg, fx, nb, tick) {
        return false;
    } else {
        false
    };
    !(hit && stop)
}

/// Advance every projectile; arrivals write into the damage buffer. Each
/// projectile depends only on its own state and the (unchanging during this
/// phase) entity positions, so processing order is irrelevant. A straight shot
/// (`step_straight`) writes its hits as it passes and keeps its own hit set, so no
/// projectile reads another's state. The one exception is a `stop_on_hit` shot
/// (combat.PROJECTILE_COLLISIONS = client_columns), which reads the hits written before
/// it this tick (`killed_this_tick`, at `tick`): the list is in creation order (shots are
/// appended as they are made and `retain_mut` keeps the order), so it reads those of the
/// shots created before it.
#[allow(clippy::too_many_arguments)]
pub fn step_projectiles(
    ents: &Entities,
    hash: &SpatialHash,
    cards: &CardDb,
    calib: &Calib,
    projectiles: &mut Vec<Projectile>,
    dmg: &mut DamageBuffer,
    fx: &mut EffectBuffer,
    areas: &mut Vec<crate::spell::AreaRelease>,
    scratch: &mut Vec<u32>,
    tick: u32,
) {
    let rounding = calib.crown_rounding;
    // combat.SPAWN_PROJECTILE = client_spark_fan: the sparks the carriers landing this tick release,
    // appended after every projectile has stepped, so each first steps next tick.
    let mut released: Vec<Projectile> = Vec::new();
    projectiles.retain_mut(|p| {
        if p.fresh {
            // born this tick: its first step is next tick's (combat.PROJECTILE_LAUNCH)
            p.fresh = false;
            return true;
        }
        if p.straight.is_some() {
            return step_straight(ents, hash, cards, calib, p, dmg, fx, scratch, tick);
        }
        let alive = ents.is_alive(p.target) && ents.hp[p.target.index as usize] > 0;
        // A spark carrier flies to the point it was aimed at and never follows its target.
        #[cfg(not(clash_plant = "carrier_follows_target"))]
        let follows = p.carrier.is_none();
        #[cfg(clash_plant = "carrier_follows_target")]
        let follows = true; // PLANT (regression): the new arm's carrier follows its target, as a homing shot does.
        if alive && follows {
            p.aim = ents.pos[p.target.index as usize];
        }
        // combat.PROJECTILE_STEP: the aim is the target's position after it moved this tick.
        let np = projectile_advance(calib.projectile_step, p.pos, p.aim, p.speed, &mut p.frac, p.team);
        p.pos = np;
        if np != p.aim {
            return true;
        }
        // THE RUNE GIANT'S PROJECTILE deals no damage either. Its landing on a live friend is handed to Resolve
        // (state.rs `apply_effects` puts the enchant on it); one whose friend died in flight lands on nothing (the
        // table's AllowResetTarget is false).
        if let Some(en) = p.enchant {
            #[cfg(not(clash_plant = "projectile_enchant_dropped"))]
            if alive {
                fx.enchants.push((p.target, en));
            }
            #[cfg(clash_plant = "projectile_enchant_dropped")]
            let _ = en; // PLANT: the projectile lands and enchants nobody.
            return false;
        }
        // combat.SPECIAL_HOOK = client_hook_drag: a hook deals no damage. Its landing on a
        // live target is handed to Resolve (state.rs `apply_effects` starts the drag); a hook
        // whose target died in flight lands on nothing.
        if let Some(by) = p.hook {
            if alive {
                fx.hooks.push((p.target, by));
            }
            return false;
        }
        if let Some(c) = p.carrier {
            // It lands: it deals nothing itself (the rocket's row has no Damage) and releases its sparks.
            release_sparks(ents, hash, cards, calib, p, c, dmg, fx, scratch, &mut released, tick);
            return false;
        }
        // The enchant bonus the shot was fired with (`enchant_bonus`): on each splash victim, or on the one target.
        let bonus = Bonus { hit: p.bonus, crown: p.bonus_crown };
        if p.splash > 0 {
            let from = dmg.hits.len();
            splash(ents, hash, p.team, p.aim, p.splash, p.hits_air, p.hits_ground, p.damage, p.crown_pct, rounding, dmg, scratch);
            add_splash_bonus(ents, calib, &mut dmg.hits[from..], bonus, p.target);
            apply_attack_buff(ents, calib, p.buff, p.pulse, (p.src_level, p.buff_first), p.target, scratch, fx);
        } else if alive {
            let ti = p.target.index as usize;
            dmg.hits.push(Hit { target: p.target, amount: damage_against(ents.kind[ti], p.damage, p.crown_pct, rounding) + bonus.on(ents.kind[ti]), ignores_hide: false });
            if let Some(b) = p.buff {
                // The shot's buff rides its arrival. A row that sets ApplyBuffBeforeDamage (the Mother Witch's) says so
                // on the application, and Resolve lands a death-spawning buff on a unit this same hit kills
                // (status.APPLY_BUFF_BEFORE_DAMAGE, state.rs `apply_effects`).
                fx.buffs.push(BuffHit { src_level: p.src_level, before_damage: p.buff_first, ..BuffHit::plain(p.target, b.buff, b.time_ms, p.pulse) });
            }
        }
        // THE AREA THE SHOT LEAVES (CardDef::projectile_area), at the point it landed on: cast in
        // this Projectile phase, so it first acts next tick (state.rs `phase_projectile`).
        #[cfg(not(clash_plant = "projectile_area_dropped"))]
        if let Some((card, level)) = p.release {
            areas.push(crate::spell::AreaRelease { team: p.team, card, level, pos: p.aim });
        }
        #[cfg(clash_plant = "projectile_area_dropped")]
        let _ = &areas; // PLANT: the shot's area is dropped.
        false
    });
    projectiles.append(&mut released);
}

/// combat.SPAWN_PROJECTILE = client_spark_fan: the sparks the carrier `p` releases on the tick it
/// lands (`CardDef::spark`). Measured on client 15.535.29 on the Firecracker (2 landings, 10
/// sparks): SpawnCount straight shots created at the landing point itself (0 off it: SpawnRadius
/// moves nothing), each aimed ProjectileRange (4999-5002) from it at its own offset from the
/// carrier's flight line, launch point to landing point: -32, -16, 0, +16, +32 degrees in creation
/// order (`SPARK_STEP_DEG`). Each steps its Speed (549.0-550.6) from the next tick and is gone on
/// the step that would reach its aim point (last seen 4941-4950 out). On both landings every spark
/// hit the Knight on the landing tick (5 x 64 at level 11, the Knight 209 and 493 from the landing
/// point) and flew on to its range, and none hit it again (5 sparks 786-815 from it on the next
/// tick). So each spark is a straight shot (`Straight`) with the row's ProjectileRadius, filters and
/// damage, tested here at the landing point against the post-move positions, that hits each enemy
/// once and flies on. Whether a spark hits a unit it reaches later on its path is unmeasured: no
/// second enemy came near one.
#[allow(clippy::too_many_arguments)]
fn release_sparks(
    ents: &Entities,
    hash: &SpatialHash,
    cards: &CardDb,
    calib: &Calib,
    p: &Projectile,
    c: Carrier,
    dmg: &mut DamageBuffer,
    fx: &mut EffectBuffer,
    nb: &mut Vec<u32>,
    out: &mut Vec<Projectile>,
    tick: u32,
) {
    let Some(sp) = cards.get(c.card).spark else { return };
    let at = p.aim;
    // A point beyond the landing point on the carrier's flight line: the bearing the fan turns from.
    let ahead = Vec2::new(2 * at.x - c.from.x, 2 * at.y - c.from.y);
    for k in 0..sp.count {
        let deg = (2 * k - (sp.count - 1)) as i64 * SPARK_STEP_DEG / 2;
        let mut spark = Projectile {
            team: p.team,
            pos: at,
            target: p.target,
            aim: fan_aim(at, ahead, sp.range, deg, p.team),
            speed: sp.speed * calib.projectile_speed_to_subtiles_per_tick,
            damage: c.damage,
            crown_pct: sp.crown_pct,
            splash: 0,
            hits_air: sp.hits_air,
            hits_ground: sp.hits_ground,
            frac: Vec2::default(),
            fresh: false,
            buff: None,
            pulse: 0,
            firer_card: p.firer_card,
            carrier: None,
            straight: Some(Straight { origin: at, reach: sp.reach, only_enemies: sp.only_enemies, ..Straight::default() }),
            hook: None,
            release: None,
            buff_first: false,
            src_level: p.src_level,
            enchant: None,
            bonus: c.bonus,
            bonus_crown: c.bonus_crown,
        };
        straight_hits(ents, hash, cards, calib, &mut spark, at, dmg, fx, nb, tick);
        out.push(spark);
    }
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
        let np = projectile_advance(step, pos, aim, p.speed, &mut frac, p.team);
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
/// target, and may miss it. Whether the client counts it is unmeasured. A HOOK (`Projectile::hook`,
/// only under combat.SPECIAL_HOOK = client_hook_drag) deals no damage and is skipped too, so its
/// flight time never stretches the ETA of the shots that do.
/// A SPARK CARRIER (`Projectile::carrier`, only under combat.SPAWN_PROJECTILE = client_spark_fan)
/// deals nothing itself and flies to a fixed point, and is skipped for the same reason; its sparks
/// are straight shots. THE RUNE GIANT'S PROJECTILE (`Projectile::enchant`) deals nothing and flies at a friend, and
/// is skipped too. A shot's enchant bonus (`Projectile::bonus`) is part of what it deals.
pub fn doomed_by_shots_in_flight(ents: &Entities, projectiles: &[Projectile], rounding: CrownRounding, tick_ms: i32, limit_ms: i32, step: ProjectileStep) -> Vec<bool> {
    let cap = ents.capacity();
    let mut pending = vec![0i64; cap];
    let mut last_ms = vec![0i32; cap];
    for p in projectiles {
        if p.straight.is_some() || p.hook.is_some() || p.carrier.is_some() || p.enchant.is_some() || !ents.is_alive(p.target) {
            continue;
        }
        let t = p.target.index as usize;
        pending[t] += (damage_against(ents.kind[t], p.damage, p.crown_pct, rounding) + Bonus { hit: p.bonus, crown: p.bonus_crown }.on(ents.kind[t])) as i64;
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
///
/// AND FOR AN ATTACHED RIDER (`riders_immune`: target.rs `riders_immune`, rider.TARGETABLE_WHILE_ATTACHED =
/// untargetable_immune; entity.rs `attached`): a hit written on one is dropped, whatever wrote it (a melee
/// splash, a death blow), so the rule does not rest on every writer asking first.
pub fn resolve(ents: &mut Entities, dmg: &mut DamageBuffer, sums: &mut Vec<i64>, hidden_immune: bool, underground_immune: bool, riders_immune: bool, tick: u32) -> ResolveOut {
    #[cfg(clash_plant = "hidden_takes_damage")]
    let hidden_immune = {
        // PLANT (regression): the choke point never consults the hide state.
        let _ = hidden_immune;
        false
    };
    let cap = ents.capacity();
    sums.clear();
    sums.resize(cap, 0);
    for h in dmg.hits.drain(..) {
        if !ents.is_alive(h.target) || h.amount <= 0 {
            continue;
        }
        // movement.SPAWN_PATHFIND_BODY = untouchable (the caller passes it): a unit under ground takes no
        // hit, whatever wrote it -- a splash, a death's damage, an area (entity.rs `underground`). And a
        // hidden building takes none but a hit that ignores the hide (`untouchable_now`).
        if untouchable_now(ents, h.target.index as usize, hidden_immune, underground_immune, h.ignores_hide) {
            continue;
        }
        #[cfg(not(clash_plant = "dash_not_immune"))]
        if ents.dash_immune(h.target.index as usize, tick) {
            continue;
        }
        #[cfg(clash_plant = "dash_not_immune")]
        let _ = tick; // PLANT: a dashing unit takes every hit.
        if riders_immune && ents.attached(h.target.index as usize) {
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

/// A BODY NO HIT LANDS ON, now: unit `v` is under ground under movement.SPAWN_PATHFIND_BODY =
/// untouchable (`underground_immune`; entity.rs `underground`), or it is a building hidden by its own
/// hide under hide.HIDDEN_IMMUNE_TO_DAMAGE (`hidden_immune`; `HideState::Hidden`) and the hit does not
/// ignore the hide (`ignores_hide`: the lifetime expiry). The one test `resolve` and `land_at_once`
/// drop a hit by, and the one a straight shot passes a unit by (`straight_hits`).
#[inline]
pub fn untouchable_now(ents: &Entities, v: usize, hidden_immune: bool, underground_immune: bool, ignores_hide: bool) -> bool {
    (underground_immune && ents.underground(v)) || (hidden_immune && !ignores_hide && ents.hide[v] == HideState::Hidden)
}

/// LAND ONE DIRECT STRIKE AT ONCE (calibration match.TICK_ORDER = client_sequential_strike;
/// state.rs `land_strike`): `hits` are the hits one `fire` of a unit with no projectile buffered,
/// applied as `resolve` applies a tick's hits -- a dead target and a non-positive amount skipped,
/// the under-ground and hide immunities (`untouchable_now`, `underground_immune` read as `resolve`'s
/// caller reads it) and a dash's immunity (combat.DASH_ATTACK, entity.rs `dash_immune`, at `tick`)
/// respected, a shield absorbing the hit with no overflow into hitpoints -- but now,
/// so the units after the striker in the pass read the result. Returns which teams' king tower
/// was struck, for its wake. A victim at 0 hp is not despawned here: `resolve` queues every live
/// entity at or below 0, so its death, its death spawn and its reaping are the tick's as before.
///
/// ONE STRIKE AT A TIME, where `resolve` sums a tick's hits on one target before the shield: a
/// shield broken by this strike lets a later hit of the same tick through. Unmeasured.
pub fn land_at_once(ents: &mut Entities, hits: &[Hit], hidden_immune: bool, underground_immune: bool, riders_immune: bool, tick: u32) -> [bool; 2] {
    let mut king_hit = [false; 2];
    for h in hits {
        if !ents.is_alive(h.target) || h.amount <= 0 {
            continue;
        }
        let t = h.target.index as usize;
        // `resolve`'s under-ground and hide immunities. The under-ground one was missing, so under this
        // order alone a melee strike or a spin took hp off a Miner still under ground.
        if untouchable_now(ents, t, hidden_immune, underground_immune, h.ignores_hide) {
            continue;
        }
        // `resolve`'s dash immunity (combat.DASH_ATTACK = client_dash), the same plant with it.
        #[cfg(not(clash_plant = "dash_not_immune"))]
        if ents.dash_immune(t, tick) {
            continue;
        }
        #[cfg(clash_plant = "dash_not_immune")]
        let _ = tick; // PLANT: a dashing unit takes every hit.
        // `resolve`'s rider immunity (rider.TARGETABLE_WHILE_ATTACHED).
        if riders_immune && ents.attached(t) {
            continue;
        }
        if ents.kind[t] == EntityKind::KingTower {
            king_hit[ents.team[t] as usize] = true;
        }
        if ents.shield[t] > 0 {
            ents.shield[t] = (ents.shield[t] as i64 - h.amount as i64).max(0) as i32;
        } else {
            ents.hp[t] = (ents.hp[t] as i64 - h.amount as i64).max(i32::MIN as i64) as i32;
        }
    }
    king_hit
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
