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
    AttackCombo, AttackCycle, Calib, ComboPushback, LoadFirstHitLeave, RandomDelayStream, DamageReductionLaw, IdleBuffLaw, ChargeLevelScaling, ChargedHitTiming, CustomFirstProjectile, HitBeyondCancelRange, HitSpeedBuff, LaunchBeyondCancelRange, MultipleProjectiles, ProjectileLaunch,
    ProjectileStep, ProjectileYOffset, RangeProjectile, SpawnPathfindBody, StraightShotBuildingReach, DirectHitBuffCountdown, TargetBuffScope, VariableDamage,
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
    /// THE UNIT'S OWN DRAIN OR DEATH: a building's lifetime drain and expiry, a delayed kamikaze's drain, a
    /// kamikaze's death on its fire (state.rs). No DamageReduction applies to it (status.DAMAGE_REDUCTION,
    /// `reduce_hit`): the unit is not being hit by anything. Every other writer leaves it false. `default` so a
    /// buffer saved before it still reads.
    #[serde(default)]
    pub own: bool,
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
    /// THE FIRER ITSELF: the unit a shot a deflecting champion sends back flies to (card.rs `AbilityEffect::Deflect`,
    /// `step_projectiles`). Set by the fire that made the shot and read only where the shot lands on a unit whose
    /// deflect is active, so it is NOT in the state hash (every battle without a deflect hashes as it did); a snapshot
    /// restores it, and one saved before the field (`default` None) sends nothing back.
    #[serde(default)]
    pub firer: Option<EntityId>,
    /// A shot a deflect sent back: it is not deflected again. False on every other shot; hashed only when set.
    #[serde(default)]
    pub deflected: bool,
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
    /// A SHOT THAT KEEPS ITS AIM (calibration combat.NON_HOMING_AIM = fixed_at_fire): a splash shot whose
    /// row is not Homing (projectiles.csv Homing false: the Princess's, the Bomber's, the Mortar's) flies to
    /// `aim`, its target's start-of-tick position on the fire tick, lands there and splashes there, whatever
    /// its target does after the shot. False on every other shot, and on every shot under the shipped
    /// follows_target. Added after SNAPSHOT_FORMAT 20; absent in older snapshots = false, and hashed only when
    /// set.
    #[serde(default)]
    pub fixed: bool,
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
    /// THE EVO ELITE BARBARIANS' SPEAR (card.rs `SpearDef`): Some only on a thrown spear, whose trail areas
    /// `step_projectiles` makes as it flies. None on every other shot. `default` so a snapshot saved before it still
    /// loads; hashed only when Some.
    #[serde(default)]
    pub trail: Option<SpearTrail>,
    /// A CHAINED SHOT (card.rs `ChainHitDef`: the Electro Dragon's, the Electro Spirit's): the hops it has left and the
    /// units it has hit; where it lands on its live target it goes on from there (`step_projectiles`, `chain_next`).
    /// None on every other shot. `default` so a snapshot saved before it still loads; hashed only when Some.
    #[serde(default)]
    pub chain: Option<ChainHop>,
    /// AN EVO BOMBER'S BOUNCE (card.rs `BounceDef`): the bounces it has left and their length; where it lands it goes on
    /// (`step_projectiles`). None on every other shot. `default` so a snapshot saved before it still loads; hashed only
    /// when Some.
    #[serde(default)]
    pub bounce: Option<BounceHop>,
}

/// A spear's trail (`Projectile::trail`): the thrower's card and level (the area is the card's `projectile_area`), the
/// spear's steps so far and the next trail area's number.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct SpearTrail {
    pub card: u16,
    pub level: i32,
    pub steps: u16,
    pub next: u16,
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

/// AN EVO BOMBER'S BOUNCES (`Projectile::bounce`): the bounces left, their length (subtiles), the point this flight
/// started from (the thrower's, then each landing's): the line a bounce goes on along, and every unit the throw's
/// landings have hit so far, sorted (a later landing spares them). `hit` is `default` so a battle saved before it loads.
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct BounceHop {
    pub left: u8,
    pub range: i32,
    pub from: Vec2,
    #[serde(default)]
    pub hit: Vec<EntityId>,
}

/// A CHAINED SHOT'S HOPS (`Projectile::chain`): the hops left, the chain's radius (subtiles), every unit it has hit, in
/// order, and the ticks a hop still waits before its first step (CHAIN_HOP_WAIT_TICKS; 0 on the fired shot).
#[derive(Clone, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct ChainHop {
    pub left: u8,
    pub radius: i32,
    pub hit: Vec<EntityId>,
    #[serde(default)]
    pub wait: u8,
    /// THE EVO ELECTRO DRAGON'S CHAIN (card.rs `EvoChainDef`): endless, `hit` its last hits only. `default` so a battle
    /// saved before it loads.
    #[serde(default)]
    pub evo: Option<EvoHop>,
}

/// AN EVO ELECTRO DRAGON'S CHAIN (`ChainHop::evo`): this projectile's place in it (0: his shot) and the tick of his shot.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct EvoHop {
    pub n: u16,
    pub shot: u32,
}

/// A CHAINED SHOT'S HOP WAITS this many ticks on the target it hit before its first step, then flies at the shot's speed.
/// Measured on client 15.535.29 (Oracle's sp-chain-*: an Electro Dragon's shot, 2000 a tick, and an Electro Spirit's,
/// 1000): each next hit came 3 + ceil(hop / speed) ticks after the last, 16 hops of 420 to 3000.
pub const CHAIN_HOP_WAIT_TICKS: u8 = 3;

/// AN EVO ELECTRO DRAGON'S HOP after the first waits this many ticks on the unit it hit before its first step, under
/// combat.EVO_CHAIN_HOP_WAIT = client15535_two_ticks (state.rs `EvoChainHopWait`). Measured on client 15.535.29: its track
/// starts 2 ticks after the last one's landing, 67 of 77.
pub const EVO_CHAIN_HOP_WAIT_TICKS: u8 = 2;

/// THE NEXT TARGET OF A CHAINED SHOT (card.rs `ChainHitDef`): the closest enemy of `team` to unit `from`, centre to
/// centre and within the chain's radius, that the shot has not hit: alive, visible, above the ground, in the air or on
/// it as the shot hits (a crown tower included, as the default targets are). Ties by creation order.
#[allow(clippy::too_many_arguments)]
pub fn chain_next(ents: &Entities, cards: &CardDb, calib: &Calib, tick: u32, team: Team, from: usize, hop: &ChainHop, hits_air: bool, hits_ground: bool) -> Option<EntityId> {
    let r2 = (hop.radius as i64) * (hop.radius as i64);
    (0..ents.capacity())
        .filter(|&j| ents.alive[j] && ents.hp[j] > 0 && ents.team[j] != team && !hop.hit.contains(&ents.id_of(j)))
        .filter(|&j| if ents.in_air(j) { hits_air } else { hits_ground })
        .filter(|&j| !ents.underground(j) && !crate::target::invisible_at(calib, cards, ents, tick, j))
        .map(|j| (ents.pos[j].dist2(ents.pos[from]), ents.creation_seq[j], j))
        .filter(|(d2, _, _)| *d2 <= r2)
        .min()
        .map(|(_, _, j)| ents.id_of(j))
}

/// THE NEXT TARGET OF AN EVO ELECTRO DRAGON'S CHAIN (card.rs `EvoChainDef`): the closest enemy of `team` to unit `from`,
/// centre to centre within the chain's radius, not `from` itself, that is not one of the chain's last `remember` hits;
/// if there is none, the closest of those (a repeat). Alive, above the ground, in the air or on it as the shot hits,
/// visible unless `invisible`, and a crown tower only while `towers`. Ties by creation order.
#[allow(clippy::too_many_arguments)]
pub fn chain_next_remember(ents: &Entities, cards: &CardDb, calib: &Calib, tick: u32, team: Team, from: usize, hop: &ChainHop, remember: usize, towers: bool, invisible: bool, hits_air: bool, hits_ground: bool) -> Option<EntityId> {
    let r2 = (hop.radius as i64) * (hop.radius as i64);
    let recent = &hop.hit[hop.hit.len().saturating_sub(remember)..];
    let near = |j: usize| (ents.pos[j].dist2(ents.pos[from]), ents.creation_seq[j], j);
    // combat.EVO_CHAIN_HOP_REACH = client15535_strict: a unit at exactly the radius is passed over (client 15.535.29,
    // sp-f4-ed-s0 t1165: an Ice Golem 4,000 off).
    // PLANT (regression) hop_reach_inclusive: the new arm still reaches a unit at exactly the radius.
    #[cfg(not(clash_plant = "hop_reach_inclusive"))]
    let strict = calib.evo_chain_hop_reach == crate::state::EvoChainHopReach::Client15535Strict;
    #[cfg(clash_plant = "hop_reach_inclusive")]
    let strict = false;
    let within = |d2: i64| if strict { d2 < r2 } else { d2 <= r2 };
    let cands: Vec<usize> = (0..ents.capacity())
        .filter(|&j| j != from && ents.alive[j] && ents.hp[j] > 0 && ents.team[j] != team)
        .filter(|&j| if ents.in_air(j) { hits_air } else { hits_ground })
        .filter(|&j| !ents.underground(j) && (invisible || !crate::target::invisible_at(calib, cards, ents, tick, j)))
        .filter(|&j| towers || !matches!(ents.kind[j], EntityKind::KingTower | EntityKind::PrincessTower))
        .filter(|&j| within(near(j).0))
        .collect();
    #[cfg(not(clash_plant = "evo_chain_no_repeat"))]
    let repeat = cands.iter().filter(|&&j| recent.contains(&ents.id_of(j))).map(|&j| near(j)).min();
    #[cfg(clash_plant = "evo_chain_no_repeat")]
    let repeat: Option<(i64, u32, usize)> = None; // PLANT (regression): the chain never repeats a target.
    cands.iter().filter(|&&j| !recent.contains(&ents.id_of(j))).map(|&j| near(j)).min().or(repeat).map(|(_, _, j)| ents.id_of(j))
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
/// combat.DEPLOY_PROJECTILE = client_on_landing_action_at_2: the deploy blow of a unit an ACTION makes (the Hero
/// Musketeer's turret, `fire_ability`) lands on the 2nd tick after its creation, measured on client 15.535.29.
pub const ACTION_DEPLOY_PROJECTILE_DELAY_TICKS: i32 = 2;

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

/// THE POWER SHOT'S SIDE COPIES (card.rs `PowerShotDef`; `fire`, right after the middle): `count` copies of `side`,
/// `distance` apart across the middle's line through its first point, each a straight shot of its own range from there
/// that stands a tick (`hold`) before its first step, as the client makes them on the tick after the middle. Measured on
/// client 15.535.29 (sp-form-EliteArcher-hero-s0): the middle's first frame t249, the copies' t250 at its first point 750
/// to each side, stepping 1,000 from t251.
fn power_copies(ents: &Entities, cards: &CardDb, calib: &Calib, a: usize, ti: usize, pw: crate::card::PowerShotDef, projectiles: &mut Vec<Projectile>) {
    let Some(middle) = projectiles.last().filter(|p| p.firer == Some(ents.id_of(a)) && p.straight.is_some()).cloned() else { return };
    let (team, src, tgt) = (ents.team[a], ents.pos[a], ents.pos[ti]);
    let base = fan_aim(src, tgt, 0, 0, team);
    let off = |d: i32, deg: i64| {
        let q = fan_aim(src, tgt, d, deg, team);
        Vec2::new(q.x - base.x, q.y - base.y)
    };
    let ahead = off(pw.side.shot.range, 0);
    let damage = own_damage(ents, cards, a, cards.scaled(ents.card[a], ents.level[a], pw.side.damage).expect("level validated at spawn"));
    let crown_pct = own_crown_pct(ents, cards, a).unwrap_or(pw.side.crown_pct);
    let (push, push_all) = pw.side.shot.knockback.map_or((0, false), |k| (k.distance, k.all));
    for k in 0..pw.count {
        // Across the line, from -distance / 2 to +distance / 2 in `count` steps.
        let across = pw.distance * (2 * k - (pw.count - 1)) / 2;
        let side = if across >= 0 { off(across, 90) } else { off(-across, -90) };
        let pos = Vec2::new(middle.pos.x + side.x, middle.pos.y + side.y);
        projectiles.push(Projectile {
            pos,
            aim: Vec2::new(pos.x + ahead.x, pos.y + ahead.y),
            speed: pw.side.speed * calib.projectile_speed_to_subtiles_per_tick,
            damage,
            crown_pct,
            hits_air: pw.side.shot.hits_air,
            hits_ground: pw.side.shot.hits_ground,
            frac: Vec2::default(),
            straight: Some(Straight { origin: pos, reach: pw.side.shot.reach, push, push_all, only_enemies: pw.side.shot.only_enemies, hold: 1, period: 0, start: 0, t: 0, hit: Vec::new(), stop_on_hit: false, thrower: None }),
            ..middle.clone()
        });
    }
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
/// `range` out on the launch line, in NATIVE units. `origin` and `apex` are read on the
/// native grid in the thrower's frame (path.rs `native_in_frame`), so a Red throw is the rotation of a Blue one.
///
/// THE APEX TICK (t = period / 2) STANDS WHERE THE TICK BEFORE STOOD: the throw never reaches `range`. Client
/// 15.535.29 records the apex tick at the value one tick before it, and on client 16.402 it decides a hit: the
/// Executioner's axe of capture 20260920-082459 t3068 (period 30) stands on (2943, 15232), 6965 out, on t = 14, 15 and
/// 16. A Skeleton on (3969, 16333), 1504 from that point, is not hit and lives to strike the Princess on t3086; from
/// the apex, 7000 out on the same line, it would be 1479 away, inside the 1500 reach.
fn pingpong_pos(origin: Vec2, apex: Vec2, start: i32, t: i32, period: i32, team: Team) -> Vec2 {
    let t = if period > 0 && 2 * t == period { t - 1 } else { t };
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
            out.hits.push(Hit { target: ents.id_of(v), amount: damage_against(ents.kind[v], amount, crown_pct, rounding), ignores_hide: false, own: false });
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
    // combat.LOAD_TIMER_TARGET_LOSS = client15535_stands_after_walk_loss: a unit holding after a walking loss keeps its
    // load timer (entity.rs `load_hold` 2; 2 is written under that arm alone).
    // PLANT (regression) load_hold_unread: the hold is not read; the timer runs on.
    #[cfg(not(clash_plant = "load_hold_unread"))]
    let held = ents.load_hold[a] == 2;
    #[cfg(clash_plant = "load_hold_unread")]
    let held = false;
    // combat.LOAD_TIMER_TARGET_LOSS = client15535_stands_while_held: a unit stunned, frozen or knocked back keeps its load
    // timer on every tick of the hold (client 15.535.29: 10-tick stuns 33 of 33, 22-tick freezes 39 of 39; every loss
    // with no hold ran on).
    // PLANT (regression) held_load_runs_on: the new arm's held unit still runs its timer down.
    // A knockback LADDER (`push_active`) is not a hold: the timer runs on through it (client 15.535.29,
    // ladder_load_census.py: 108 of 112 ladders over a running timer ran on; the 4 that stood were the Hero Giant's slap,
    // whose flight is a stun, `held`).
    // PLANT (regression) ladder_load_stands: the new arm still holds the timer through a ladder.
    #[cfg(not(clash_plant = "ladder_load_stands"))]
    let knock_holds = ents.knocked(a) && !ents.push_active[a];
    #[cfg(clash_plant = "ladder_load_stands")]
    let knock_holds = ents.knocked(a);
    #[cfg(not(clash_plant = "held_load_runs_on"))]
    let stands = calib.load_timer_target_loss == crate::state::LoadTimerTargetLoss::Client15535StandsWhileHeld
        && (ents.held(&cards.buffs, a, calib.full_stop_buff_is_stun) || knock_holds);
    #[cfg(clash_plant = "held_load_runs_on")]
    let stands = false;
    let held = held || stands;
    let mut load = if held { ents.attack_load_ms[a] } else { (ents.attack_load_ms[a] - calib.tick_ms).max(0) };
    let phase = ents.attack_phase[a];
    let mut progress = ents.attack_ms[a];
    // combat.LOAD_FIRST_HIT_LEAVE = client15535_windup_refunded: a LoadFirstHit unit leaving its attack before the attack
    // fires (its progress short of HitSpeed) gets the entry's windup back: its load timer reads LoadTime less its
    // progress, floored at 0, so a Sparky charged before the lock is charged again. Measured on client 15.535.29, every
    // Sparky leave before a fire: sp-il-8b9b t880 (progress 3450, load -450 then 0; relocked on t891 with progress 3050),
    // sp-il-04cb t1870 (progress 1500, load 1500). Every other unit's timer runs on.
    #[cfg(not(clash_plant = "load_first_hit_leave_runs_on"))]
    let refund = card.load_first_hit
        && matches!(calib.load_first_hit_leave, LoadFirstHitLeave::Client15535WindupRefunded | LoadFirstHitLeave::Client15535RefundAtDeath)
        && progress > 0
        && progress < card.hit_speed_ms;
    #[cfg(clash_plant = "load_first_hit_leave_runs_on")]
    let refund = false; // PLANT (regression): the leaving Sparky's timer runs on from its entry's reset.
    let refunded = if refund { Some((card.load_time_ms.max(0) - progress).max(0)) } else { None };
    let idle = |load| AttackStep { phase: AttackPhase::Idle, ms: 0, load_ms: refunded.unwrap_or(load), fired_at: None, charge_snapped: false };
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
    let in_range = in_attack_range(calib, ents.pos[a], attack_reach(ents, card, a), own, ents.pos[ti], ents.radius[ti]);
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
        in_attack_range(calib, ents.pos[a], attack_reach(ents, card, a), ents.radius[a], ents.pos[ti], ents.radius[ti])
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

/// THE ATTACK'S REACH for the range gate of the attack cycle: the card's Range, or while an Evo Musketeer aims a snipe
/// (entities `attack_seq` 1 on a card with a snipe; state.rs `snipe_pass`) the snipe entry's CustomRange.
#[inline]
pub fn attack_reach(ents: &Entities, card: &crate::card::CardDef, a: usize) -> i32 {
    match (card.evo.as_ref().and_then(|v| v.snipe), card.evo.as_ref().and_then(|v| v.spear)) {
        (Some(sn), _) if ents.attack_seq[a] == 1 => sn.reach,
        // An Evo Elite Barbarian holding its spear on a target out of melee reach (state.rs `spear_pass`).
        (_, Some(sp)) if ents.attack_seq[a] == 1 => sp.max,
        _ => card.range,
    }
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
    client_rng: &mut Option<u32>,
    bolts: &[EntityId],
    tick: u32,
) {
    let card = cards.get(ents.card[a]);
    let ti = target.index as usize;
    // combat.LAUNCH_BEYOND_CANCEL_RANGE = not_launched, the projectile half of combat.HIT_BEYOND_CANCEL_RANGE: a projectile
    // attacker's due launch whose target stands more than targeting.LOGIC_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE past its
    // reach (Range + both radii; start-of-tick positions, Attack runs before Move) is not made: no projectile, no extra
    // bolt, nothing it would carry. The cycle (the load timer reset as on a launch) and the target are the caller's and
    // are kept. Measured on client 16.402 (20260920-071744 t1983): a Lava Pup's swing at a Goblin 2,393 past reach ended
    // with no projectile, where 1,259 due launches (seats apart) stood within 633 past. Crown towers
    // (targeting.TOWER_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE), kamikazes, selector cards (their melee entry) and evolved
    // forms launch as before.
    // PLANT (regression) launch_beyond_cancel_launched: the new arm still launches past the cancel range.
    #[cfg(not(clash_plant = "launch_beyond_cancel_launched"))]
    let cancelled = calib.launch_beyond_cancel_range == LaunchBeyondCancelRange::NotLaunched
        && card.projectile.is_some()
        && !card.kamikaze
        && card.attack_select.is_none()
        && card.evo.is_none()
        && !ents.kind[a].is_crown_tower()
        && !in_attack_range(calib, ents.pos[a], card.range + calib.cancel_hit_from_long_distance_range, ents.radius[a], ents.pos[ti], ents.radius[ti]);
    #[cfg(clash_plant = "launch_beyond_cancel_launched")]
    let cancelled = false;
    if cancelled {
        return;
    }
    // THE ENCHANT BONUS of this attack (`enchant_bonus`): zero for every attacker that carries no enchant and on every
    // attack of one that is not its bonus attack. It rides every hit the attack deals, one Hit per victim.
    let (direct, spark) = enchant_bonus(ents, cards, calib, a);
    // THE ATTACK'S BUFF (card.rs `CardDef::attack_buff`): a projectile carries it to
    // its arrival tick, an instant hit applies it here. The per-pulse amount is the
    // ATTACKER's level-scaled figure, computed once, because the victim does not know
    // the attacker's level.
    let atk_buff = card.attack_buff;
    // combat.DIRECT_HIT_BUFF_COUNTDOWN = client15535_landing_tick: the buff an INSTANT hit lands (below: the melee entry,
    // the single-target hit, its extra bolts, the instant splash) is counted down on its landing tick, as one landing in
    // the attacker's turn is; a projectile carries `atk_buff` whole to its arrival.
    // PLANT (regression) direct_hit_buff_full_time: the new arm's instant hit still lands its whole time.
    #[cfg(not(clash_plant = "direct_hit_buff_full_time"))]
    let landing_tick = calib.direct_hit_buff_countdown == DirectHitBuffCountdown::Client15535LandingTick;
    #[cfg(clash_plant = "direct_hit_buff_full_time")]
    let landing_tick = false;
    let direct_buff = atk_buff.map(|b| if landing_tick { BuffApply { time_ms: (b.time_ms - calib.tick_ms).max(0), ..b } } else { b });
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
    // THE CARRIER'S OWN BUFF (status.rs `BuffDef::damage_pct`, `char_crown_pct`; the Hero Berserker's rage): the hit
    // is scaled by the strongest DamageMultiplier the attacker carries, and a crown tower takes the buff's own percent
    // of it in place of the card's. Nothing moves for a unit without such a buff.
    // THE EVO ARCHER'S POWER SHOT (card.rs `FarShotDef`): an attack of her far entry (`attack_seq` 1, state.rs
    // `select_attack`) deals her Projectile2's level-scaled Damage in place of her own.
    #[cfg(not(clash_plant = "far_shot_never"))]
    let amount = match card.evo.as_ref().and_then(|v| v.far_shot) {
        Some(fs) if ents.attack_seq[a] == 1 => cards.scaled(ents.card[a], ents.level[a], fs.damage).expect("level validated at spawn"),
        _ => amount,
    };
    let amount = own_damage(ents, cards, a, amount);
    let pct = own_crown_pct(ents, cards, a).unwrap_or(card.crown_tower_damage_percent);
    // THE EVO ELITE BARBARIANS' SPEAR (card.rs `SpearDef`; state.rs `spear_pass`): an attack of the thrown entry
    // (`attack_seq` 1) throws the spear at the target, a homing shot from the spear's start radius at its own speed and
    // level-scaled damage, releasing the card's area as it flies and where it lands (`step_projectiles`). Measured on
    // client 15.535.29 (sp-form-AngryBarbarians-evo-s0): the spear leaves 196-200 ahead of its thrower on the fire tick,
    // steps 600 a tick and takes 284 off a level-11 Knight (111 on the ladder) on its arrival.
    #[cfg(not(clash_plant = "spear_never_thrown"))]
    let throws = card.evo.as_ref().and_then(|v| v.spear).filter(|_| ents.attack_seq[a] == 1);
    #[cfg(clash_plant = "spear_never_thrown")]
    let throws: Option<crate::card::SpearDef> = None; // PLANT (regression): the member strikes in melee on every attack.
    if let Some(sp) = throws {
        let (pos, fresh) = launch_point(ents, calib, sp.start_radius, 0, false, false, a, ti);
        projectiles.push(Projectile {
            team: ents.team[a],
            pos,
            target,
            aim: ents.pos[ti],
            fixed: false,
            speed: sp.speed * calib.projectile_speed_to_subtiles_per_tick,
            damage: cards.scaled(ents.card[a], ents.level[a], sp.damage).expect("level validated at spawn"),
            crown_pct: sp.crown_pct,
            splash: 0,
            hits_air: card.attacks_air,
            hits_ground: card.attacks_ground,
            frac: Vec2::default(),
            fresh,
            buff: None,
            pulse: 0,
            firer_card: Some(ents.card[a]),
            firer: Some(ents.id_of(a)),
            deflected: false,
            straight: None,
            hook: None,
            bonus: 0,
            bonus_crown: 0,
            carrier: None,
            release: (card.projectile_area.is_some() || card.evo.as_ref().is_some_and(|v| v.shot_spawn.is_some())).then(|| (ents.card[a], ents.level[a])),
            buff_first: false,
            src_level: ents.level[a],
            enchant: None,
            trail: Some(SpearTrail { card: ents.card[a], level: ents.level[a], steps: 0, next: 0 }),
            chain: None,
            bounce: None,
        });
        return;
    }
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
            dmg.hits.push(Hit { target, amount: damage_against(ents.kind[ti], amount, pct, calib.crown_rounding) + direct.on(ents.kind[ti]), ignores_hide: false, own: false });
            if let Some(b) = direct_buff {
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
        // THE HERO MAGIC ARCHER'S POWER SHOT (card.rs `PowerShotDef`): an attack of its entry 1 (`attack_seq`, set by its
        // button: state.rs `fire_ability`) fires the middle arrow, its speed, damage, crown share and straight block in
        // place of the row's own, and its side copies beside it (`power_copies`).
        #[cfg(not(clash_plant = "power_shot_unread"))]
        let power = match card.ability.as_ref().map(|x| &x.effect) {
            Some(crate::card::AbilityEffect::DecoyWarp(d)) if ents.attack_seq[a] == 1 => Some(d.power),
            _ => None,
        };
        #[cfg(clash_plant = "power_shot_unread")]
        let power: Option<crate::card::PowerShotDef> = None; // PLANT: the hero's own arrow on every attack.
        if let (None, Some(rs)) = (custom, power.map(|pw| pw.middle.shot).or(card.range_shot)) {
            let (amount, pct, speed) = match power {
                Some(pw) => {
                    let base = cards.scaled(ents.card[a], ents.level[a], pw.middle.damage).expect("level validated at spawn");
                    (own_damage(ents, cards, a, base), own_crown_pct(ents, cards, a).unwrap_or(pw.middle.crown_pct), pw.middle.speed)
                }
                None => (amount, pct, p.speed),
            };
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
                // combat.RANDOM_DELAY_STREAM = client15535_battle_stream: a RandomDelay shot's delay is a draw of the client's
                // battle generator (state.rs `client_draw`, which a replay syncs to the capture), one draw a shot in the fan's
                // order and one more before the next shot. Measured on client 15.535.29: every complete Hunter volley, 56 of
                // 56, 19 draws a volley of 10, the even ones the shots' delays (sp-f4-hunterG0 t444: from 4014919791 the
                // first steps 5, 4, 2, 5, 5, 3, 4, 5, 2, 4 ticks after the volley's creation, and 3699668171 after).
                #[cfg(not(clash_plant = "random_delay_engine_stream"))]
                let stream = calib.random_delay_stream == RandomDelayStream::Client15535BattleStream;
                #[cfg(clash_plant = "random_delay_engine_stream")]
                let stream = false; // PLANT (regression): the delays still draw from the engine's own generator.
                for k in 0..pellets {
                    // Each pellet of a fan is aimed at ProjectileRange on its own offset from
                    // the bearing to the target and released 2-5 ticks after its creation
                    // (`FAN_RELEASE_TICKS`); a single shot on the bearing, released at once.
                    // Under combat.PROJECTILE_COLLISIONS = client_columns a RandomDelay row's
                    // shot is released by its own draw instead (`release_hold`).
                    let aim = fan_aim(src, tgt, rs.range, fan_offset_deg(k), team);
                    #[cfg(not(clash_plant = "fan_pellets_unheld"))]
                    let hold = if columns && stream && rs.random_delay_ms > 0 {
                        let tk = calib.tick_ms.max(1);
                        let u = crate::state::client_draw(client_rng, rng, rs.random_delay_ms as u32) as i32;
                        if k + 1 < pellets {
                            crate::state::client_draw(client_rng, rng, 0);
                        }
                        (u + tk - 1) / tk
                    } else if columns {
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
                        speed: speed * calib.projectile_speed_to_subtiles_per_tick,
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
                        firer: Some(ents.id_of(a)),
                        deflected: false,
                        carrier: None,
                        fixed: false,
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
                        trail: None,
                        chain: None,
                        bounce: None,
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
                if let Some(pw) = power {
                    power_copies(ents, cards, calib, a, ti, pw, projectiles);
                }
                return;
            }
        }
        let (speed, amount, splash_r, hits_air, hits_ground, pct) = match custom {
            Some(cf) => (cf.speed, cards.scaled(ents.card[a], ents.level[a], cf.damage).expect("level validated at spawn"), cf.radius, cf.hits_air, cf.hits_ground, cf.crown_pct),
            None => (p.speed, amount, splash_r, card.attacks_air, card.attacks_ground, pct),
        };
        // ProjectileYOffset (`CardDef::projectile_y_offset`): always on a record the hero pass loaded (measured on client
        // 16.402: the Hero Musketeer's shots first appear at 1800 x the aim + 300 x her side's forward, 100 launches over
        // five captures; the base Musketeer's at 450 x the aim), under combat.PROJECTILE_Y_OFFSET on every other.
        let always = cards.is_hero_record(ents.card[a]);
        // THE HERO BOWLER'S NEAR ENTRY (card.rs `SiegeDef::near_start`; state.rs, the swing's entry pick): its shot
        // leaves from its own CustomProjectileStartRadius.
        let start_radius = match cards.siege_of(ents.card[a]) {
            Some(sg) if ents.attack_seq[a] == 2 => sg.near_start,
            _ => card.projectile_start_radius,
        };
        let (pos, fresh) = launch_point(ents, calib, start_radius, card.projectile_y_offset, always, card.projectile_homing, a, ti);
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
        // combat.NON_HOMING_AIM = fixed_at_fire: a splash shot of a row that is not Homing keeps the aim it is
        // fired with, its target's start-of-tick position (Attack runs before Move), and `step_projectiles` flies
        // it there, lands it there and splashes there. Measured on client 16.402: every Princess, Bomber and
        // Mortar shot at a moving target kept its end point for the whole flight, and the Princess's area landed
        // on that point, not on her target. The Princess's CustomFirstProjectile row is not Homing either. A
        // direct hit (no splash) and a spark carrier are left as they were.
        #[cfg(not(clash_plant = "non_homing_shot_follows"))]
        let fixed = calib.non_homing_aim == crate::state::NonHomingAim::FixedAtFire && !card.projectile_homing && splash_r > 0 && carrier.is_none();
        #[cfg(clash_plant = "non_homing_shot_follows")]
        let fixed = {
            let _ = calib.non_homing_aim;
            false // PLANT (regression): the new arm's non-homing splash shot follows its target, as follows_target.
        };
        projectiles.push(Projectile {
            team: ents.team[a],
            pos,
            target,
            aim: ents.pos[ti],
            fixed,
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
            firer: Some(ents.id_of(a)),
            deflected: false,
            straight: None,
            hook: None,
            // A carrier deals nothing itself: its sparks carry the bonus (`Carrier::bonus`).
            bonus: if carrier.is_some() { 0 } else { direct.hit },
            bonus_crown: if carrier.is_some() { 0 } else { direct.crown },
            carrier,
            release: (card.projectile_area.is_some() || card.evo.as_ref().is_some_and(|v| v.shot_spawn.is_some())).then(|| (ents.card[a], ents.level[a])),
            buff_first: card.attack_buff_first,
            src_level: ents.level[a],
            enchant: None,
            trail: None,
            // THE CHAIN (card.rs `ChainHitDef`): the hops after this target.
            chain: card.chain_hit.map(|c| ChainHop { left: (c.count - 1).clamp(0, 255) as u8, radius: c.radius, hit: vec![target], wait: 0, evo: None }),
            // THE BOUNCE (card.rs `BounceDef`, the Evo Bomber's): the bounces after this landing.
            bounce: card.evo.as_ref().and_then(|v| v.bounce).map(|b| BounceHop { left: b.count, range: b.range, from: pos, hit: Vec::new() }),
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
        // combat.HIT_BEYOND_CANCEL_RANGE = no_damage, for a splash centred on the TARGET: when the target stands more
        // than targeting.LOGIC_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE past the attacker's reach (start-of-tick positions),
        // the whole splash is void, as the single-target hit below is. Measured on client 15.535.29
        // (sp-sk-souls-ignore-s0 t405): a Skeleton King's swing completes with a Battle Ram 2,391 past reach, and
        // the Ram keeps its hp. A splash centred on the attacker (the Valkyrie) is unmeasured and left as it was.
        #[cfg(not(clash_plant = "area_hit_beyond_cancel_deals_damage"))]
        let void = !card.self_as_aoe_center
            && calib.hit_beyond_cancel_range == HitBeyondCancelRange::NoDamage
            && !in_attack_range(calib, ents.pos[a], card.range + calib.cancel_hit_from_long_distance_range, ents.radius[a], ents.pos[ti], ents.radius[ti]);
        #[cfg(clash_plant = "area_hit_beyond_cancel_deals_damage")]
        let void = false; // PLANT (regression): the far area hit deals its damage, as before r21.
        if !void {
            let from = dmg.hits.len();
            splash(ents, hash, ents.team[a], centre, splash_r, card.attacks_air, card.attacks_ground, amount, pct, calib.crown_rounding, dmg, scratch);
            add_splash_bonus(ents, calib, &mut dmg.hits[from..], direct, target);
            apply_attack_buff(ents, calib, direct_buff, atk_pulse, (ents.level[a], card.attack_buff_first), target, scratch, fx);
        }
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
            dmg.hits.push(Hit { target, amount: damage_against(ents.kind[ti], amount, pct, calib.crown_rounding) + direct.on(ents.kind[ti]), ignores_hide: false, own: false });
            if let Some(b) = direct_buff {
                fx.buffs.push(BuffHit { src_level: ents.level[a], before_damage: card.attack_buff_first, ..BuffHit::plain(target, b.buff, b.time_ms, atk_pulse) });
            }
            // knockback.COMBO_PUSHBACK = client15535_ladder_from_attacker_hit_tick: a combo hit whose entry carries a
            // melee pushback (card.rs `ComboStage::pushback`; the Monk's third, 1800) pushes its target straight away
            // from the attacker's start-of-tick centre along the knockback ladder, the first step on this tick
            // (`push_from` with `now`). Measured on client 15.535.29: both of a level-11 Monk's third hits on a Knight
            // stepped 249 on the hit's tick. Only a troop is pushed (spell.rs `pushable`), and the entry's
            // IsMeleePushbackAll overrides the target's IgnorePushback. The single-target hit alone: no combo row
            // splashes.
            #[cfg(not(clash_plant = "combo_push_next_tick"))]
            let now = true;
            #[cfg(clash_plant = "combo_push_next_tick")]
            let now = false; // PLANT (regression): the push's first step is the next tick's, as a Fireball's.
            // client15535_ladder_armed_at_hit pushes as the measured arm does; Resolve (state.rs `apply_effects`) undoes the
            // victim's walk of the hit's tick.
            if let (ComboPushback::LadderFromAttackerHitTick | ComboPushback::Client15535LadderArmedAtHit, Some(c)) = (calib.combo_pushback, card.combo) {
                let stage = c.entry(ents.combo_ix[a]).1;
                if stage.pushback > 0 {
                    let ctx = SpellCtx { ents, hash, cards, calib, steps: &[], tick };
                    push_from(&ctx, ents.team[a], ti, ents.pos[a], &KnockbackDef { distance: stage.pushback, all: stage.pushback_all }, now, fx);
                }
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
            dmg.hits.push(Hit { target: b, amount: damage_against(ents.kind[bi], amount, pct, calib.crown_rounding) + direct.on(ents.kind[bi]), ignores_hide: false, own: false });
            if let Some(bf) = direct_buff {
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
///
/// THE COMBO comes first (combat.ATTACK_COMBO = client15535_sequence_across_targets; card.rs `ComboDef`): a combo
/// row's hit deals the damage of the entry its count has reached (entity.rs `combo_ix`, moved on by state.rs
/// `phase_attack` after the hit), stage 0 the entity's own Damage and the others VariableDamage2 / VariableDamage3
/// scaled like it. Measured on client 15.535.29: a level-11 Monk's hits run 140, 140, 422, 140, ..., and the count
/// runs across targets (his first hit killed a Skeleton, his second and third fell on a Knight as 140 and 422).
fn stage_damage(ents: &Entities, cards: &CardDb, calib: &Calib, a: usize) -> i32 {
    #[cfg(not(clash_plant = "combo_unread"))]
    let combo = calib.attack_combo == AttackCombo::SequenceAcrossTargets;
    #[cfg(clash_plant = "combo_unread")]
    let combo = {
        // PLANT (regression): every Monk hit deals Damage under the new arm too.
        let _ = calib.attack_combo;
        false
    };
    if let Some(c) = cards.get(ents.card[a]).combo.filter(|_| combo) {
        let (s, stage) = c.entry(ents.combo_ix[a]);
        if s == 0 {
            return ents.damage[a];
        }
        // A level-1 stat like Damage; the level was validated at spawn.
        return cards.scaled(ents.card[a], ents.level[a], stage.damage).expect("level validated at spawn");
    }
    // THE EVO INFERNO DRAGON (card.rs `StagesDef`): the entry its hit count has reached (entity.rs `combo_ix`, moved on
    // by state.rs `evo_after_fire` after the hit), in place of its row's VariableDamage ramp; the count runs across
    // targets. Measured on client 15.535.29 (sp-form-InfernoDragon-evo-s0, level 11): hits 400 ms apart read 35, 35,
    // (a third on the Skeleton it kills), 35 on a Knight, then 120 on each of the next four, the fifth hit onward.
    #[cfg(not(clash_plant = "stages_unread"))]
    if let Some(st) = cards.get(ents.card[a]).evo.as_ref().and_then(|v| v.stages) {
        // A level-1 stat like Damage; the level was validated at spawn.
        return cards.scaled(ents.card[a], ents.level[a], st.damages[st.entry(ents.combo_ix[a])]).expect("level validated at spawn");
    }
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

/// Does the combo count run (entity.rs `combo_ix`)? Under either combo key's new arm (combat.ATTACK_COMBO,
/// knockback.COMBO_PUSHBACK), so each can be switched on alone; under both old arms the count stays 0 and a battle
/// hashes as it did before the column.
#[inline]
pub fn combo_counts(calib: &Calib) -> bool {
    calib.attack_combo != AttackCombo::NotRead || calib.combo_pushback != ComboPushback::NotRead
}

/// Where attacker `a`'s shot at entity `ti` is born, and whether its first step is the
/// next tick's (combat.PROJECTILE_LAUNCH): ProjectileStartRadius (`start_radius`, subtiles)
/// from the attacker's centre toward the target, first step the next tick (the live tower
/// arrows: 299-300 from the centre on the launch frame, 600 per tick from the next); the old
/// arm starts it at the centre and steps it at once. `fire` and `launch_hook` both read it.
///
/// combat.PROJECTILE_Y_OFFSET = client_forward_y adds the row's ProjectileYOffset (`y_offset`,
/// subtiles; the King Tower's 400) along the attacker's OWN forward y to that point: Blue +y,
/// Red -y, whatever the bearing to the target. `always` adds it whatever the key says: a record
/// the hero pass loaded (card.rs `CardDb::is_hero_record`; the Hero Musketeer's 300 and her
/// turret's, measured on client 16.402), whose offset was never under the key. Measured on the 16.402 corpus on every king-tower
/// shot's first frame (413 of 413, one seat per battle): the shot sits exactly 400 past the plain
/// point toward the enemy side, and it flies from there. Only this point moves: the bearing is
/// still read from the attacker's centre, and the other launch paths (a straight shot's, a fan's)
/// do not read the column, which no loaded row that sets it fires.
#[allow(clippy::too_many_arguments)]
fn launch_point(ents: &Entities, calib: &Calib, start_radius: i32, y_offset: i32, always: bool, homing: bool, a: usize, ti: usize) -> (Vec2, bool) {
    match calib.projectile_launch {
        ProjectileLaunch::StartRadiusNextTick => {
            let d = ents.pos[ti].sub(ents.pos[a]);
            let len = crate::fixed::isqrt(d.len2()) as i32;
            // combat.LAUNCH_PAST_TARGET = client15535_homing_unclamped: a homing shot starts its whole radius out along the
            // aim, past a nearer target, and flies back to it (client 15.535.29: the Hero Musketeer's near shots 6 of 6 at
            // 1800 x the aim, none on the target; sp-il-04cb t1204, 952 from Skeleton 58, hers appears 1,070 past it and
            // lands on t1206). clamped: never past the target (the engine's).
            #[cfg(not(clash_plant = "launch_clamped_past_target"))]
            let unclamped = homing && calib.launch_past_target == crate::state::LaunchPastTarget::Client15535HomingUnclamped;
            #[cfg(clash_plant = "launch_clamped_past_target")]
            let unclamped = {
                let _ = homing;
                false // PLANT (regression): the new arm's shot still starts no farther than its target.
            };
            let r = if unclamped { start_radius.max(0) } else { start_radius.min(len.max(0)) };
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
            if (always || calib.projectile_y_offset == ProjectileYOffset::ClientForwardY) && y_offset != 0 {
                #[cfg(not(clash_plant = "projectile_y_offset_arena_frame"))]
                let forward = forward_dy(ents.team[a]);
                #[cfg(clash_plant = "projectile_y_offset_arena_frame")]
                let forward = 1; // PLANT (regression): the offset goes up the arena for both seats, so the Red king's shot is born behind it.
                return (Vec2::new(pos.x, pos.y + forward * y_offset), true);
            }
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
    let thrower = cards.get(ents.card[a]);
    let (pos, fresh) = launch_point(ents, calib, thrower.projectile_start_radius, thrower.projectile_y_offset, cards.is_hero_record(ents.card[a]), false, a, ti);
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
        firer: Some(ents.id_of(a)),
        deflected: false,
        straight: None,
        hook: Some(ents.id_of(a)),
        carrier: None,
        fixed: false,
        release: None,
        buff_first: false,
        src_level: ents.level[a],
        enchant: None,
        bonus: 0,
        bonus_crown: 0,
        trail: None,
        chain: None,
        bounce: None,
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
    // The Evo Executioner's axe (card.rs `AxeDef`), on a throw of his.
    let axe = p.firer_card.and_then(|c| cards.get(c).evo.as_ref().and_then(|e| e.axe).map(|a| (c, a)));
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
    // combat.STRAIGHT_SHOT_BUILDING_REACH = client15535_rounded_square: a CheckCollisions shot reaches a building's square,
    // whose corner stands its radius x 1.415 from the centre (the broad phase reaches it).
    // PLANT (regression) pellet_building_circle: the new arm still tests a building's circle.
    #[cfg(not(clash_plant = "pellet_building_circle"))]
    let square = s.stop_on_hit && calib.straight_shot_building_reach == StraightShotBuildingReach::Client15535RoundedSquare;
    #[cfg(clash_plant = "pellet_building_circle")]
    let square = false;
    let broad = if square { hash.max_radius() * 1415 / 1000 + 1 } else { hash.max_radius() };
    hash.neighbours_within(ents, at, s.reach + broad, nb);
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
        let reached = if square && ents.kind[v].is_building() {
            crate::spell::in_square(at, ents.pos[v], ents.radius[v], s.reach)
        } else {
            in_range_edge(at, ents.pos[v], s.reach, ents.radius[v])
        };
        if !reached {
            continue;
        }
        // combat.PROJECTILE_COLLISIONS = client_columns: a CheckCollisions shot does not collide
        // with a unit this tick's earlier hits have already killed; it flies on past it.
        #[cfg(not(clash_plant = "kill_inside_tick_unread"))]
        if s.stop_on_hit && killed_this_tick(ents, cards, calib, dmg, tick, v) {
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
        // THE EVO EXECUTIONER'S AXE (card.rs `AxeDef`): strong on a victim whose edge is within its range of his, from
        // where he stands (from where he threw, once he is gone); its strong hit on the way out pushes the victim away.
        let mut hit_damage = damage;
        if let Some((c, ax)) = axe {
            let (from, r) = match s.thrower.filter(|t| ents.is_alive(*t)) {
                Some(t) => (ents.pos[t.index as usize], ents.radius[t.index as usize]),
                None => (s.origin, cards.get(c).collision_radius),
            };
            #[cfg(not(clash_plant = "axe_strong_by_centre"))]
            let strong = in_range_edge(from, ents.pos[v], ax.strong_range + r, ents.radius[v]);
            #[cfg(clash_plant = "axe_strong_by_centre")]
            let strong = in_range_edge(from, ents.pos[v], ax.strong_range, 0); // PLANT: the range read centre to centre.
            #[cfg(clash_plant = "axe_strong_never")]
            let strong = !strong && false; // PLANT: every hit normal.
            if strong {
                hit_damage = cards.scaled(c, src_level, ax.strong_damage).unwrap_or(ax.strong_damage);
                #[cfg(not(clash_plant = "axe_push_never"))]
                if 2 * s.t <= s.period || cfg!(clash_plant = "axe_push_both_legs") {
                    push_from(&ctx, team, v, from, &KnockbackDef { distance: ax.push, all: false }, false, fx);
                }
            }
        }
        dmg.hits.push(Hit { target: id, amount: damage_against(ents.kind[v], hit_damage, crown_pct, calib.crown_rounding) + bonus.on(ents.kind[v]), ignores_hide: false, own: false });
        if let Some(b) = buff {
            fx.buffs.push(BuffHit { src_level, before_damage, ..BuffHit::plain(id, b.buff, b.time_ms, pulse) });
        }
        #[cfg(not(clash_plant = "range_shot_unpushed"))]
        if s.push > 0 {
            push_from(&ctx, team, v, at, &KnockbackDef { distance: s.push, all: s.push_all }, false, fx);
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
fn killed_this_tick(ents: &Entities, cards: &CardDb, calib: &Calib, dmg: &DamageBuffer, tick: u32, v: usize) -> bool {
    if ents.shield[v] > 0 || ents.dash_immune(v, tick) {
        return false;
    }
    let id = ents.id_of(v);
    let hidden = calib.hide_hidden_immune && ents.hide[v] == HideState::Hidden;
    // status.DAMAGE_REDUCTION: each hit as `resolve` lands it.
    let r = damage_reduction_of(ents, cards, calib, tick, v);
    let dealt: i64 = dmg
        .hits
        .iter()
        .filter(|h| h.target == id && h.amount > 0 && (!hidden || h.ignores_hide))
        .map(|h| landed(h, r, calib.damage_reduction) as i64)
        .sum();
    dealt >= ents.hp[v] as i64
}

/// status.IDLE_BUFF: is unit `v`'s idle buff (card.rs `IdleBuffDef`: the Super Knight's shield, the Evo Knight's
/// reduction) on at `tick`? Never under not_read, never on a card without one, never while the unit deploys. Else on
/// from `idle_back` (entity.rs): 0 before the unit's first hit, and after a hit the first tick past the unit's attack
/// plus its BuffWhenNotAttackingTime (state.rs `idle_buff_pass`). Read at the tick's START state: the pass that moves
/// `idle_back` runs after Resolve, so the tick of the unit's own hit still counts as idle.
pub fn idle_on(ents: &Entities, cards: &CardDb, calib: &Calib, tick: u32, v: usize) -> bool {
    if calib.idle_buff == IdleBuffLaw::NotRead || cards.get(ents.card[v]).idle_buff.is_none() {
        return false;
    }
    #[cfg(not(clash_plant = "idle_shield_while_attacking"))]
    let back = ents.idle_back[v];
    #[cfg(clash_plant = "idle_shield_while_attacking")]
    let back = 0; // PLANT (regression): the idle buff never goes off, the unit's attack included.
    ents.deploy_ms[v] <= 0 && tick >= back
}

/// status.DAMAGE_REDUCTION: the DamageReduction unit `v` takes a hit under at `tick`: the strongest of the buffs it
/// carries (its slots, and its own idle buff while that is on, `idle_on`), as every other multiplier column composes
/// (status.rs `compose`: the strongest of a sign applies). 0 under not_read and on a unit carrying none. The one
/// definition `resolve`, `land_at_once` and `killed_this_tick` read.
pub fn damage_reduction_of(ents: &Entities, cards: &CardDb, calib: &Calib, tick: u32, v: usize) -> i32 {
    if calib.damage_reduction == DamageReductionLaw::NotRead {
        return 0;
    }
    let mut r = ents.buff_slots(v).iter().filter(|s| !s.is_empty()).map(|s| cards.buffs[(s.id - 1) as usize].damage_reduction).max().unwrap_or(0);
    if let Some(own) = cards.get(ents.card[v]).idle_buff.as_ref().and_then(|b| b.own) {
        if idle_on(ents, cards, calib, tick, v) {
            r = r.max(cards.buffs[own as usize].damage_reduction);
        }
    }
    r
}

/// status.DAMAGE_REDUCTION: hit `amount` (> 0) on a unit whose DamageReduction is `r` (1..=100), under `law`. A
/// non-positive amount, and a unit with no reduction, pass unchanged. The shipped truncated_floor_one is
/// max(1, amount * (100 - r) / 100) with the division truncating, as every other percentage buff's is (status.rs
/// `compose`): measured at r = 100 on client 15.535.29 (sweep-SuperKnight, a Knight's 202 and a princess tower's hit
/// each take 1); the Evo Knight's 60, the Monk's 65 and the hero Valkyrie's 15 are datamined, and what the formula
/// gives at them is a hypothesis (202 at 60: 80).
pub fn reduce_hit(amount: i32, r: i32, law: DamageReductionLaw) -> i32 {
    if amount <= 0 || r <= 0 {
        return amount;
    }
    let (a, kept) = (amount as i64, (100 - r.min(100)) as i64);
    match law {
        DamageReductionLaw::NotRead => amount,
        #[cfg(not(clash_plant = "reduction_rounds_up"))]
        DamageReductionLaw::TruncatedFloorOne => (a * kept / 100).max(1) as i32,
        // PLANT (regression): the shipped arm rounds a partial reduction up (202 at 60: 81, not 80); 100 still gives 1.
        #[cfg(clash_plant = "reduction_rounds_up")]
        DamageReductionLaw::TruncatedFloorOne => ((a * kept + 99) / 100).max(1) as i32,
        DamageReductionLaw::TruncatedNoFloor => (a * kept / 100) as i32,
        DamageReductionLaw::CeilFloorOne => ((a * kept + 99) / 100).max(1) as i32,
    }
}

/// What hit `h` takes off a unit whose DamageReduction is `r`: `reduce_hit`, except on the unit's own drain or death
/// (`Hit::own`), which lands whole.
#[inline]
fn landed(h: &Hit, r: i32, law: DamageReductionLaw) -> i32 {
    if h.own {
        h.amount
    } else {
        reduce_hit(h.amount, r, law)
    }
}

/// One tick of a straight shot (`Projectile::straight`), after the move pass. Returns false
/// once it is gone.
///   * A PINGPONG throw: `t` advances; the shot tests the positions it hits against its
///     PREVIOUS frame's position (measured on client 15.535.29 on the Executioner's axe:
///     5 hits, the tightest miss 1519 against 1500), then moves to `pingpong_pos`. Its hit
///     set is cleared as the return leg starts, so it hits a unit once a leg, and it is gone
///     after `t = period`.
///   * A held pellet (`hold` > 0) stands at its launch point and tests there; under combat.HELD_SHOT_TEST =
///     client15535_untested a RandomDelay pellet (`stop_on_hit`) tests nothing while it stands.
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
        // combat.HELD_SHOT_TEST = client15535_untested: a RandomDelay pellet (a CheckCollisions row's, `stop_on_hit`)
        // stands untested; it is tested again from its first step. A power shot's side copies test as before.
        // PLANT (regression) held_shot_tested: the new arm still tests the standing pellet.
        #[cfg(not(clash_plant = "held_shot_tested"))]
        if stop && calib.held_shot_test == crate::state::HeldShotTest::Client15535Untested {
            return true;
        }
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
    units: &mut Vec<crate::spell::Release>,
    scratch: &mut Vec<u32>,
    tick: u32,
    deflecting: &[(EntityId, i32)],
) {
    let rounding = calib.crown_rounding;
    // combat.SPAWN_PROJECTILE = client_spark_fan: the sparks the carriers landing this tick release,
    // appended after every projectile has stepped, so each first steps next tick. In a cell so that the pass's step
    // can be run again on the tick's Evo hops (combat.EVO_CHAIN_HOP_FIRST_STEP, below).
    let released: std::cell::RefCell<Vec<Projectile>> = std::cell::RefCell::new(Vec::new());
    let mut step = |p: &mut Projectile| -> bool {
        if p.fresh {
            // born this tick: its first step is next tick's (combat.PROJECTILE_LAUNCH)
            p.fresh = false;
            return true;
        }
        if p.straight.is_some() {
            let more = step_straight(ents, hash, cards, calib, p, dmg, fx, scratch, tick);
            // THE EVO FIRECRACKER'S SPARK leaves its small fireworks where its flight ends (card.rs `FireworksDef`).
            #[cfg(not(clash_plant = "fireworks_never"))]
            if !more {
                if let Some(card) = p.firer_card.filter(|c| cards.get(*c).evo.as_ref().is_some_and(|v| v.fireworks.is_some())) {
                    areas.push(crate::spell::AreaRelease { team: p.team, card, level: p.src_level, pos: p.pos, target: None, part: Some(crate::card::EVO_SPARK_FIREWORKS) });
                }
            }
            return more;
        }
        // A CHAINED SHOT'S HOP waits on the target it hit (CHAIN_HOP_WAIT_TICKS).
        if let Some(c) = p.chain.as_mut().filter(|c| c.wait > 0) {
            c.wait -= 1;
            return true;
        }
        let alive = ents.is_alive(p.target) && ents.hp[p.target.index as usize] > 0;
        // hide.DRILL_UNDER_SHOT = client15535_dropped: a shot flying at an Evo Goblin Drill building that has gone under is
        // dropped: it no longer follows the building and lands on nothing (client 15.535.29, sp-f4-drill-s0 t1011: the
        // arrow lost its target on the hide tick, the pending 109 went to 0 and no hitpoint moved).
        // PLANT (regression) drill_under_shot_lands: the new arm still lands the shot.
        #[cfg(not(clash_plant = "drill_under_shot_lands"))]
        let alive = alive
            && !(calib.drill_under_shot == crate::state::DrillUnderShot::Client15535Dropped
                && ents.hide[p.target.index as usize] == HideState::Hidden
                && cards.get(ents.card[p.target.index as usize]).evo.as_ref().is_some_and(|v| v.drill.is_some()));
        // A spark carrier flies to the point it was aimed at and never follows its target, and so does a shot
        // that keeps its aim (`Projectile::fixed`, combat.NON_HOMING_AIM = fixed_at_fire).
        #[cfg(not(clash_plant = "carrier_follows_target"))]
        let follows = p.carrier.is_none() && !p.fixed;
        #[cfg(clash_plant = "carrier_follows_target")]
        let follows = !p.fixed; // PLANT (regression): the new arm's carrier follows its target, as a homing shot does.
        if alive && follows {
            p.aim = ents.pos[p.target.index as usize];
        }
        // combat.HOOK_LANDING: where a hook stood at the start of this tick, before its step.
        let start = p.pos;
        // A SPEAR'S TRAIL (`Projectile::trail`; card.rs `SpearDef`): on step n of its flight every trail area due by
        // then (area k on step floor((first + k x every) / TICK_MS)) is released on the point the spear held before
        // this step's move, cast in this Projectile phase as the landing's area is. Measured on client 15.535.29
        // (sp-rage-5000-s0 and sp-rage-4000-s0, own troops walking the spears' lines; the recordings list no area): the
        // first area stands 1800 along the flight (the point before step 4's move, 200 ms). The later areas' spacing
        // (every 160 ms, floor) is the reading that agrees best with the probes' raged ticks (825 of 930 clean ticks
        // against 790 for ceil on sp-rage-4000-s0), not a measurement of their points.
        if let Some(tr) = p.trail.as_mut() {
            tr.steps = tr.steps.saturating_add(1);
            #[cfg(not(clash_plant = "spear_trail_dropped"))]
            if let Some(sp) = cards.get(tr.card).evo.as_ref().and_then(|v| v.spear) {
                let dt = calib.tick_ms.max(1);
                while i32::from(tr.steps) >= (sp.trail_first_ms + sp.trail_every_ms * i32::from(tr.next)) / dt {
                    areas.push(crate::spell::AreaRelease { team: p.team, card: tr.card, level: tr.level, pos: start, target: None, part: None });
                    tr.next = tr.next.saturating_add(1);
                }
            }
        }
        // combat.PROJECTILE_STEP: the aim is the target's position after it moved this tick.
        let np = projectile_advance(calib.projectile_step, p.pos, p.aim, p.speed, &mut p.frac, p.team);
        p.pos = np;
        // A DEFLECT'S AREA (card.rs `AbilityEffect::Deflect::radius`, the Monk's 1500): an enemy single-target shot at a
        // champion whose deflect is active lands on the tick its step brings it within the area's radius of him, not at
        // his centre. Measured on client 15.535.29 (sp-champ-Monk-s0): a Musketeer 5996 away, three shots of three a tick
        // before a flight to his centre would land (t233, t253, t273), their returns a tick earlier too.
        #[cfg(not(clash_plant = "deflect_catches_at_body"))]
        let caught = alive && p.splash == 0 && !p.deflected && {
            let tp = ents.pos[p.target.index as usize];
            deflecting.iter().any(|(id, r)| *id == p.target && np.dist2(tp) <= i64::from(*r) * i64::from(*r))
        };
        #[cfg(clash_plant = "deflect_catches_at_body")]
        let caught = false; // PLANT (regression): the shot flies to his centre.
        if np != p.aim && !caught {
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
                fx.hooks.push((p.target, by, start));
            }
            return false;
        }
        if let Some(c) = p.carrier {
            // It lands: it deals nothing itself (the rocket's row has no Damage) and releases its sparks.
            release_sparks(ents, hash, cards, calib, p, c, dmg, fx, scratch, &mut released.borrow_mut(), tick);
            // THE EVO FIRECRACKER'S ROCKET leaves its big fireworks where it lands (card.rs `FireworksDef`).
            #[cfg(not(clash_plant = "fireworks_never"))]
            if cards.get(c.card).evo.as_ref().is_some_and(|v| v.fireworks.is_some()) {
                areas.push(crate::spell::AreaRelease { team: p.team, card: c.card, level: p.src_level, pos: p.aim, target: None, part: Some(crate::card::EVO_FIREWORKS) });
            }
            return false;
        }
        // The enchant bonus the shot was fired with (`enchant_bonus`): on each splash victim, or on the one target.
        let bonus = Bonus { hit: p.bonus, crown: p.bonus_crown };
        if p.splash > 0 {
            let from = dmg.hits.len();
            splash(ents, hash, p.team, p.aim, p.splash, p.hits_air, p.hits_ground, p.damage, p.crown_pct, rounding, dmg, scratch);
            // AN EVO BOMBER'S LATER LANDING spares every unit the throw's earlier landings hit (card.rs `BounceDef`).
            #[cfg(not(clash_plant = "bounce_rehits"))]
            if let Some(b) = p.bounce.as_ref().filter(|b| !b.hit.is_empty()) {
                let mut k = from;
                while k < dmg.hits.len() {
                    if b.hit.binary_search(&dmg.hits[k].target).is_ok() {
                        dmg.hits.remove(k);
                    } else {
                        k += 1;
                    }
                }
            }
            add_splash_bonus(ents, calib, &mut dmg.hits[from..], bonus, p.target);
            // hide.SHOT_AT_HIDING_BUILDING = client15535_lands: the splash's hit on the building the shot was fired at
            // passes its hide (`shot_passes_hide`).
            if shot_passes_hide(calib) {
                for h in dmg.hits[from..].iter_mut().filter(|h| h.target == p.target) {
                    h.ignores_hide = true;
                }
            }
            apply_attack_buff(ents, calib, p.buff, p.pulse, (p.src_level, p.buff_first), p.target, scratch, fx);
            // AN EVO BOMBER'S BOUNCE (`Projectile::bounce`, card.rs `BounceDef`): a new bomb on the landing point, aimed
            // `range` on along the line this flight came (from `BounceHop::from`), one bounce fewer; it first steps next
            // tick, as every released shot does.
            #[cfg(not(clash_plant = "bounce_never"))]
            if let Some(b) = p.bounce.as_ref().filter(|b| b.left > 0) {
                let (dx, dy) = (i64::from(p.aim.x - b.from.x), i64::from(p.aim.y - b.from.y));
                let len = isqrt(dx * dx + dy * dy);
                if len > 0 {
                    let r = i64::from(b.range);
                    let next = Vec2::new(p.aim.x + (dx * r / len) as i32, p.aim.y + (dy * r / len) as i32);
                    // The next landing spares this one's victims too.
                    let mut hit = b.hit.clone();
                    for h in &dmg.hits[from..] {
                        if let Err(k) = hit.binary_search(&h.target) {
                            hit.insert(k, h.target);
                        }
                    }
                    let hop = BounceHop { left: b.left - 1, range: b.range, from: p.aim, hit };
                    released.borrow_mut().push(Projectile { pos: p.aim, aim: next, fixed: true, frac: Vec2::default(), fresh: false, bounce: Some(hop), ..p.clone() });
                }
            }
        } else if alive {
            let ti = p.target.index as usize;
            dmg.hits.push(Hit { target: p.target, amount: damage_against(ents.kind[ti], p.damage, p.crown_pct, rounding) + bonus.on(ents.kind[ti]), ignores_hide: shot_passes_hide(calib), own: false });
            // A DEFLECT (card.rs `AbilityEffect::Deflect`, the Monk's): an enemy shot landing on a champion whose deflect
            // is active also goes back at its firer, for its full damage, from where he stands; the hit on him stands
            // (his buff's DamageReduction cuts it, combat.rs `reduce_hit`). Measured on client 15.535.29: a level-11
            // Musketeer's 217 took 75 off the deflecting Monk and 217 off the Musketeer six ticks later, three shots of
            // three. A splash shot, and a shot that was itself deflected, go nowhere; a returned shot carries its damage
            // only, not the shot's buff, area or trail (an Evo Elite Barbarian's spear comes back bare); all unmeasured.
            #[cfg(not(clash_plant = "deflect_returns_nothing"))]
            let returns = !p.deflected && deflecting.iter().any(|(id, _)| *id == p.target);
            #[cfg(clash_plant = "deflect_returns_nothing")]
            let returns = {
                let _ = deflecting;
                false // PLANT (regression): the deflect sends nothing back.
            };
            if let (true, Some(f)) = (returns, p.firer.filter(|f| ents.is_alive(*f))) {
                let fi = f.index as usize;
                released.borrow_mut().push(Projectile {
                    team: ents.team[ti],
                    pos: ents.pos[ti],
                    target: f,
                    aim: ents.pos[fi],
                    frac: Vec2::default(),
                    fresh: false,
                    buff: None,
                    pulse: 0,
                    firer_card: Some(ents.card[ti]),
                    firer: Some(p.target),
                    deflected: true,
                    release: None,
                    enchant: None,
                    trail: None,
                    chain: None,
                    bounce: None,
                    bonus: 0,
                    bonus_crown: 0,
                    ..p.clone()
                });
            }
            // THE EVO DART GOBLIN'S DART (card.rs `DartPoisonDef`): its landing starts or stacks the poison on its target.
            #[cfg(not(clash_plant = "dart_poison_never"))]
            if let Some(card) = p.firer_card.filter(|c| cards.get(*c).evo.as_ref().is_some_and(|v| v.dart_poison.is_some())) {
                areas.push(crate::spell::AreaRelease { team: p.team, card, level: p.src_level, pos: p.aim, target: Some(p.target), part: Some(crate::card::EVO_DART_POISON) });
            }
            if let Some(b) = p.buff {
                // The shot's buff rides its arrival. A row that sets ApplyBuffBeforeDamage (the Mother Witch's) says so
                // on the application, and Resolve lands a death-spawning buff on a unit this same hit kills
                // (status.APPLY_BUFF_BEFORE_DAMAGE, state.rs `apply_effects`).
                fx.buffs.push(BuffHit { src_level: p.src_level, before_damage: p.buff_first, ..BuffHit::plain(p.target, b.buff, b.time_ms, p.pulse) });
            }
            // A CHAINED SHOT (`Projectile::chain`, card.rs `ChainHitDef`): it goes on from the target it landed on to
            // the next (`chain_next`), a new shot from that target's point with the same speed, damage and buff, which
            // first steps next tick as every released shot does.
            // AN EVO ELECTRO DRAGON'S CHAIN (card.rs `EvoChainDef`) hops by its own rule (`chain_next_remember`), at once,
            // without end while he lives; its hops past the strong ones are the weak lightning.
            #[cfg(not(clash_plant = "chain_never"))]
            if let Some((c, e, d)) = p.chain.as_ref().filter(|c| c.left > 0).and_then(|c| {
                let d = p.firer_card.and_then(|f| cards.get(f).evo.as_ref().and_then(|v| v.chain))?;
                c.evo.map(|e| (c, e, d))
            }) {
                let towers = e.n.saturating_add(1) < u16::from(d.towers_until);
                let next = chain_next_remember(ents, cards, calib, tick, p.team, ti, c, usize::from(d.remember), towers, d.invisible, p.hits_air, p.hits_ground)
                    .filter(|_| p.firer.is_some_and(|f| ents.is_alive(f)));
                if let Some(next) = next {
                    let keep = c.hit.len().saturating_sub(usize::from(d.remember).saturating_sub(1));
                    let mut hit = c.hit[keep..].to_vec();
                    hit.push(next);
                    let n = e.n.saturating_add(1);
                    // combat.EVO_CHAIN_HOP_WAIT = client15535_two_ticks: every hop after the first waits on the unit hit.
                    #[cfg(not(clash_plant = "evo_hop_at_once"))]
                    let wait = if n >= 2 && calib.evo_chain_hop_wait == crate::state::EvoChainHopWait::Client15535TwoTicks { EVO_CHAIN_HOP_WAIT_TICKS } else { 0 };
                    #[cfg(clash_plant = "evo_hop_at_once")]
                    let wait = 0; // PLANT (regression): the new arm's hops fly at once, as the old one's do.
                    let hop = ChainHop { left: c.left, radius: c.radius, hit, wait, evo: Some(EvoHop { n, shot: e.shot }) };
                    let mut q = Projectile { pos: ents.pos[ti], target: next, aim: ents.pos[next.index as usize], frac: Vec2::default(), fresh: false, chain: Some(hop), ..p.clone() };
                    if n >= u16::from(d.strong) {
                        q.speed = d.weak_speed * calib.projectile_speed_to_subtiles_per_tick;
                        q.damage = p.firer_card.and_then(|f| cards.scaled(f, p.src_level, d.weak_damage).ok()).unwrap_or(d.weak_damage);
                        q.buff = None;
                        q.pulse = 0;
                    }
                    released.borrow_mut().push(q);
                }
            } else if let Some(c) = p.chain.as_ref().filter(|c| c.left > 0) {
                if let Some(next) = chain_next(ents, cards, calib, tick, p.team, ti, c, p.hits_air, p.hits_ground) {
                    let mut hit = c.hit.clone();
                    hit.push(next);
                    #[cfg(not(clash_plant = "chain_hop_no_wait"))]
                    let wait = CHAIN_HOP_WAIT_TICKS;
                    #[cfg(clash_plant = "chain_hop_no_wait")]
                    let wait = 0; // PLANT: the hop steps on the tick after the hit.
                    let hop = ChainHop { left: c.left - 1, radius: c.radius, hit, wait, evo: None };
                    released.borrow_mut().push(Projectile { pos: ents.pos[ti], target: next, aim: ents.pos[next.index as usize], frac: Vec2::default(), fresh: false, chain: Some(hop), ..p.clone() });
                }
            }
        }
        // THE AREA THE SHOT LEAVES (CardDef::projectile_area), at the point it landed on: cast in
        // this Projectile phase, so it first acts next tick (state.rs `phase_projectile`).
        #[cfg(not(clash_plant = "projectile_area_dropped"))]
        if let Some((card, level)) = p.release.filter(|(c, _)| cards.get(*c).projectile_area.is_some()) {
            // `CardDef::projectile_area_ahead` along the owner's forward (the Hero Wizard's air form's 1000; 0 on every
            // other card).
            let ahead = cards.get(card).projectile_area_ahead * crate::spell::forward_dy(p.team);
            areas.push(crate::spell::AreaRelease { team: p.team, card, level, pos: Vec2::new(p.aim.x, p.aim.y + ahead), target: Some(p.target), part: None });
        }
        // THE UNIT THE SHOT PUTS DOWN (card.rs `ShotSpawnDef`, the Evo Mortar's Goblin): one, where it landed, released
        // this tick at the firer's level on the unit's ladder (state.rs `phase_projectile`: on this frame, inert on it).
        #[cfg(not(clash_plant = "shot_spawn_dropped"))]
        if let Some((card, level)) = p.release {
            if let Some(ss) = cards.get(card).evo.as_ref().and_then(|v| v.shot_spawn) {
                let lvl = cards.unit_level(card, ss.unit.unit, None, level).expect("the shot's unit's level validated at deploy");
                units.push(crate::spell::Release { team: p.team, unit: ss.unit.unit, level: lvl, pos: p.aim, deploy_ms: Some(ss.deploy_ms), count: 1, source: card });
            }
        }
        // THE EVO PRINCESS'S FREEZING ARROW (card.rs `FreezeVolleyDef`): its area where it landed, standing there (a plain
        // arrow of hers carries no release).
        #[cfg(not(clash_plant = "freeze_area_dropped"))]
        if let Some((card, level)) = p.release.filter(|(c, _)| cards.get(*c).evo.as_ref().is_some_and(|v| v.freeze_volley.is_some())) {
            areas.push(crate::spell::AreaRelease { team: p.team, card, level, pos: p.aim, target: None, part: Some(crate::card::EVO_FREEZE_AREA) });
        }
        #[cfg(clash_plant = "projectile_area_dropped")]
        let _ = &areas; // PLANT: the shot's area is dropped.
        false
    };
    projectiles.retain_mut(&mut step);
    // combat.EVO_CHAIN_HOP_FIRST_STEP = client15535_creation_tick: an Evo Electro Dragon's hop released this tick takes
    // its first step in this same pass -- a first hop within a step of its target lands on its shot's tick, and a later
    // hop's wait counts this tick -- and so does every hop those steps release.
    // PLANT (regression) evo_hop_steps_next_tick: the new arm's hops still first step next tick.
    #[cfg(not(clash_plant = "evo_hop_steps_next_tick"))]
    let same_tick = matches!(
        calib.evo_chain_hop_first_step,
        crate::state::EvoChainHopFirstStep::Client15535CreationTick | crate::state::EvoChainHopFirstStep::Client15535ShotPlusTwo
    );
    #[cfg(clash_plant = "evo_hop_steps_next_tick")]
    let same_tick = false;
    // combat.EVO_CHAIN_HOP_FIRST_STEP = client15535_shot_plus_two: a FIRST hop released on its shot's first step (a
    // one-step shot, landing on fire + 2 under combat.EVO_CHAIN_SHOT_LAUNCH = client15535_next_tick, fire + 1 under the
    // shipped arm) takes its first step next tick, on the shot's appearance + 2 (client 15.535.29: 8 of 8 one-step shots'
    // hops landed a tick after them, sp-f4-ed3-s0 t1205); every other hop steps in this pass.
    // PLANT (regression) evo_first_hop_same_pass: the new arm's one-step first hop still steps in the landing's pass.
    #[cfg(not(clash_plant = "evo_first_hop_same_pass"))]
    let defer_first = calib.evo_chain_hop_first_step == crate::state::EvoChainHopFirstStep::Client15535ShotPlusTwo;
    #[cfg(clash_plant = "evo_first_hop_same_pass")]
    let defer_first = false;
    let first_step_after = 1 + u32::from(calib.evo_chain_shot_launch == crate::state::EvoChainShotLaunch::Client15535NextTick);
    if same_tick {
        loop {
            let mut hops: Vec<Projectile> = {
                let mut r = released.borrow_mut();
                let (hops, rest): (Vec<Projectile>, Vec<Projectile>) = r.drain(..).partition(|q| q.chain.as_ref().is_some_and(|c| c.evo.is_some()));
                *r = rest;
                hops
            };
            if defer_first {
                let (later, now): (Vec<Projectile>, Vec<Projectile>) =
                    hops.into_iter().partition(|q| q.chain.as_ref().and_then(|c| c.evo).is_some_and(|e| e.n == 1 && tick == e.shot + first_step_after));
                projectiles.extend(later);
                hops = now;
            }
            if hops.is_empty() {
                break;
            }
            hops.retain_mut(&mut step);
            projectiles.append(&mut hops);
        }
    }
    projectiles.append(&mut released.borrow_mut());
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
            firer: p.firer,
            deflected: false,
            carrier: None,
            fixed: false,
            straight: Some(Straight { origin: at, reach: sp.reach, only_enemies: sp.only_enemies, ..Straight::default() }),
            hook: None,
            release: None,
            buff_first: false,
            src_level: p.src_level,
            enchant: None,
            bonus: c.bonus,
            bonus_crown: c.bonus_crown,
            trail: None,
            chain: None,
            bounce: None,
        };
        straight_hits(ents, hash, cards, calib, &mut spark, at, dmg, fx, nb, tick);
        out.push(spark);
    }
}

/// The whole ticks until projectile `p`'s hit resolves, read at the end of a tick: the steps it still
/// takes toward its target as the target stands now (`step_projectiles`' own step, combat.PROJECTILE_STEP's
/// `step`, arriving on the step that reaches it), plus one for a shot whose first step is next tick's
/// (`fresh`). A moving target's count is re-read each tick; for a still target it is the countdown fixed
/// at launch. A shot that keeps its aim (`Projectile::fixed`) counts the steps to that aim.
pub fn ticks_to_land(ents: &Entities, p: &Projectile, step: ProjectileStep) -> i32 {
    let aim = if ents.is_alive(p.target) && !p.fixed { ents.pos[p.target.index as usize] } else { p.aim };
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
/// is skipped too. A shot's enchant bonus (`Projectile::bonus`) is part of what it deals. `homing_only`: see
/// `shots_in_flight_at`.
#[allow(clippy::too_many_arguments)]
pub fn doomed_by_shots_in_flight(ents: &Entities, projectiles: &[Projectile], rounding: CrownRounding, tick_ms: i32, limit_ms: i32, step: ProjectileStep, homing_only: Option<&CardDb>, spears: bool) -> Vec<bool> {
    let cap = ents.capacity();
    let (pending, last_ms) = shots_in_flight_at(ents, projectiles, rounding, tick_ms, step, homing_only, spears);
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

/// The two quantities `doomed_by_shots_in_flight` weighs, per slot: the summed damage of the shots in flight at the
/// unit (against its kind, crown-tower arrows and enchant bonuses included; straight shots, hooks, spark carriers and
/// the Rune Giant's projectile skipped, as there), and the ETA in ms of the one of them that lands last (0 when none
/// flies at it).
///
/// targeting.DOOMED_SET_SHOTS = client_homing_only (`homing_only` names the card table): a shot whose firer's card fires a
/// non-homing projectile (`CardDef::projectile_homing` false: a Bomber's bomb, a Princess's arrows) is left out too, as
/// the client's own pending damage leaves it out on both clients; a shot with no firer card counts as before.
///
/// targeting.DOOMED_DROP_SPEAR_MEMBERS = client15535_projectile (`spears`): an Evo Elite Barbarian's spear in flight
/// (`Projectile::trail`) counts as the homing shot it is, though its firer's card (its projectile taken off by the loader)
/// names none (client 15.535.29: its victim's pending damage carried the spear's 284 or 453).
pub fn shots_in_flight_at(ents: &Entities, projectiles: &[Projectile], rounding: CrownRounding, tick_ms: i32, step: ProjectileStep, homing_only: Option<&CardDb>, spears: bool) -> (Vec<i64>, Vec<i32>) {
    let cap = ents.capacity();
    let mut pending = vec![0i64; cap];
    let mut last_ms = vec![0i32; cap];
    for p in projectiles {
        if p.straight.is_some() || p.hook.is_some() || p.carrier.is_some() || p.enchant.is_some() || !ents.is_alive(p.target) {
            continue;
        }
        if homing_only.is_some_and(|cards| p.firer_card.is_some_and(|c| !cards.get(c).projectile_homing)) && !(spears && p.trail.is_some()) {
            continue;
        }
        let t = p.target.index as usize;
        pending[t] += (damage_against(ents.kind[t], p.damage, p.crown_pct, rounding) + Bonus { hit: p.bonus, crown: p.bonus_crown }.on(ents.kind[t])) as i64;
        last_ms[t] = last_ms[t].max(ticks_to_land(ents, p, step) * tick_ms);
    }
    (pending, last_ms)
}

/// Result of applying the buffer.
#[derive(Clone, Debug, Default)]
pub struct ResolveOut {
    /// Entities whose hp reached 0 this tick, ascending slot order.
    pub deaths: Vec<EntityId>,
    /// Teams whose king tower took damage this tick.
    pub king_hit: [bool; 2],
    /// Units of a card with a first-hit buff (card.rs `EvoDef::first_hit`) that took damage this tick and live on it,
    /// ascending slot order (state.rs `first_hit`).
    pub hurt: Vec<EntityId>,
    /// Units of a card with a shield blast (card.rs `EvoDef::shield_blast`) whose shield this tick's damage took to 0,
    /// ascending slot order (state.rs `shield_blasts`).
    pub shield_broke: Vec<EntityId>,
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
#[allow(clippy::too_many_arguments)]
pub fn resolve(
    ents: &mut Entities,
    cards: &CardDb,
    calib: &Calib,
    dmg: &mut DamageBuffer,
    sums: &mut Vec<i64>,
    hidden_immune: bool,
    underground_immune: bool,
    riders_immune: bool,
    tick: u32,
) -> ResolveOut {
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
        // NO_DAMAGE stops the hits of others, not a unit's own (`Hit::own`: its lifetime drain, its expiry, its
        // kamikaze): measured on client 15.535.29, the Hero Goblins' flag, NO_DAMAGE, drains its LifeTime 1.28 a tick
        // (sp-form-Goblins-hero-s0).
        #[cfg(not(clash_plant = "no_damage_blocks_drain"))]
        let others = !h.own;
        #[cfg(clash_plant = "no_damage_blocks_drain")]
        let others = true; // PLANT: NO_DAMAGE stops the unit's own drain too.
        // NO_DAMAGE (card.rs `CardDef::no_damage`, the Evo Skeleton Army's Spectral): no hit lands on it. Measured on client
        // 15.535.29 (sp-esa-spectrals-s0): two Spectrals 1025 and 1319 from a Zap's centre kept their 2 hp.
        #[cfg(not(clash_plant = "spectral_takes_damage"))]
        if others && cards.get(ents.card[h.target.index as usize]).no_damage {
            continue;
        }
        // NO_DAMAGE ON A BUFF (status.rs `BuffDef::no_damage`, the Evo Minion Horde's ghost): no hit lands on its carrier
        // while the buff lasts. The table's word; the scene shows a hit minion untouched for a while after (open).
        #[cfg(not(clash_plant = "ghost_takes_damage"))]
        if others && ents.buffs_of(&cards.buffs, h.target.index as usize).any(|b| b.no_damage) {
            continue;
        }
        // status.DAMAGE_REDUCTION: each hit is scaled on its own, before the tick's sum meets the shield.
        let r = damage_reduction_of(ents, cards, calib, tick, h.target.index as usize);
        sums[h.target.index as usize] += landed(&h, r, calib.damage_reduction) as i64;
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
            if ents.shield[i] == 0 && cards.get(ents.card[i]).evo.as_ref().is_some_and(|v| v.shield_blast.is_some()) {
                out.shield_broke.push(ents.id_of(i));
            }
        } else {
            ents.hp[i] = (ents.hp[i] as i64 - s).max(i32::MIN as i64) as i32;
            unkillable_floor(ents, cards, i);
        }
        if ents.hp[i] > 0 && cards.get(ents.card[i]).evo.as_ref().is_some_and(|v| v.first_hit.is_some()) {
            out.hurt.push(ents.id_of(i));
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

/// Hit `amount` of attacker `a` through the DamageMultiplier of the buffs it carries (status.rs `BuffDef::damage_pct`,
/// raw per cent, 0 = blank): amount x the strongest / 100, truncated as every percentage buff's is (status.rs
/// `compose`), on the level-scaled figure. Measured on client 15.535.29 (sp-form-Berserker-hero-s0): 102 x 164 / 100 =
/// 167, 10 hits of 10; on the level-1 figure it would be 166 or 168. Unchanged for a unit that carries none.
pub fn own_damage(ents: &Entities, cards: &CardDb, a: usize, amount: i32) -> i32 {
    #[cfg(not(clash_plant = "own_damage_multiplier_ignored"))]
    let m = ents.buff_slots(a).iter().filter(|s| !s.is_empty()).map(|s| cards.buffs[(s.id - 1) as usize].damage_pct).max().unwrap_or(0);
    #[cfg(clash_plant = "own_damage_multiplier_ignored")]
    let m = {
        let _ = (ents, cards, a);
        0 // PLANT (regression): the carrier's DamageMultiplier is not read.
    };
    // A NEGATIVE DamageMultiplier (card.rs `RawBuff::convert_opts`: -100 alone, the Hero Tombstone's stand-still hold)
    // leaves its carrier's hits nothing. Unmeasured: no scene has the held monster hit.
    #[cfg(not(clash_plant = "own_damage_multiplier_ignored"))]
    if ents.buff_slots(a).iter().any(|s| !s.is_empty() && cards.buffs[(s.id - 1) as usize].damage_pct < 0) {
        return 0;
    }
    if m <= 0 {
        return amount;
    }
    ((amount as i64) * (m as i64) / 100) as i32
}

/// The CharacterCrownTowerDamagePercent of a buff attacker `a` carries (status.rs `BuffDef::char_crown_pct`), the
/// lowest if several: its hit's share on a crown tower in place of its card's. None for a unit that carries none.
fn own_crown_pct(ents: &Entities, cards: &CardDb, a: usize) -> Option<i32> {
    ents.buff_slots(a).iter().filter(|s| !s.is_empty()).map(|s| cards.buffs[(s.id - 1) as usize].char_crown_pct).filter(|p| *p > 0).min()
}

/// GameTagsToSet UNKILLABLE (status.rs `BuffDef::unkillable`; the Hero Berserker's rage): unit `i`, carrying such a
/// buff, keeps at least 1 hitpoint whatever a hit took (`resolve`, `land_at_once`). Measured on client 15.535.29
/// (sp-form-Berserker-hero-s0): a Musketeer's 217 on the raged hero's 43 leaves it at 1 through the rest of the buff;
/// it dies after. Every other unit is untouched.
fn unkillable_floor(ents: &mut Entities, cards: &CardDb, i: usize) {
    #[cfg(not(clash_plant = "unkillable_not_read"))]
    if ents.hp[i] < 1 && ents.buff_slots(i).iter().any(|s| !s.is_empty() && cards.buffs[(s.id - 1) as usize].unkillable) {
        ents.hp[i] = 1;
    }
    #[cfg(clash_plant = "unkillable_not_read")]
    let _ = (ents, cards, i); // PLANT (regression): the tag is not read; the carrier dies as any unit.
}

/// hide.SHOT_AT_HIDING_BUILDING = client15535_lands: a shot's hit on the building it was fired at passes that building's
/// hide (it was up when the shot left: a hidden building is no one's target). Measured on client 15.535.29: 2 of 2 shots
/// fired at a Tesla while it was up landed after it went under (state.rs `ShotAtHidingBuilding`).
#[inline]
fn shot_passes_hide(calib: &Calib) -> bool {
    #[cfg(not(clash_plant = "hiding_shot_dropped"))]
    return calib.shot_at_hiding_building == crate::state::ShotAtHidingBuilding::Client15535Lands;
    #[cfg(clash_plant = "hiding_shot_dropped")]
    {
        let _ = calib;
        false // PLANT (regression): the new arm drops the shot on the hidden building, as the old one does.
    }
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
///
/// `resolve`'s NO_DAMAGE guards hold here too, and what `resolve` reports from its hits -- a first-hit card's unit hurt
/// and alive, an Evo shield taken to 0 -- is returned for the same tick's Resolve to fold into its own (state.rs
/// `phase_resolve`), so a strike's blast and buff land when a buffered hit's would.
#[allow(clippy::too_many_arguments)]
pub fn land_at_once(ents: &mut Entities, cards: &CardDb, calib: &Calib, hits: &[Hit], hidden_immune: bool, underground_immune: bool, riders_immune: bool, tick: u32) -> StrikeOut {
    let mut out = StrikeOut::default();
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
        // `resolve`'s NO_DAMAGE guards, its plants with them: the card's (the Evo Skeleton Army's Spectral) and a buff's
        // (the Evo Minion Horde's ghost), against the hits of others.
        #[cfg(not(clash_plant = "no_damage_blocks_drain"))]
        let others = !h.own;
        #[cfg(clash_plant = "no_damage_blocks_drain")]
        let others = true;
        #[cfg(not(clash_plant = "spectral_takes_damage"))]
        if others && cards.get(ents.card[t]).no_damage {
            continue;
        }
        #[cfg(not(clash_plant = "ghost_takes_damage"))]
        if others && ents.buffs_of(&cards.buffs, t).any(|b| b.no_damage) {
            continue;
        }
        let _ = others;
        if ents.kind[t] == EntityKind::KingTower {
            out.king_hit[ents.team[t] as usize] = true;
        }
        // status.DAMAGE_REDUCTION, as `resolve` scales each hit.
        let amount = landed(h, damage_reduction_of(ents, cards, calib, tick, t), calib.damage_reduction) as i64;
        if ents.shield[t] > 0 {
            ents.shield[t] = (ents.shield[t] as i64 - amount).max(0) as i32;
            #[cfg(not(clash_plant = "strike_skips_resolve_effects"))]
            if ents.shield[t] == 0 && cards.get(ents.card[t]).evo.as_ref().is_some_and(|v| v.shield_blast.is_some()) && !out.shield_broke.contains(&ents.id_of(t)) {
                out.shield_broke.push(ents.id_of(t));
            }
        } else {
            ents.hp[t] = (ents.hp[t] as i64 - amount).max(i32::MIN as i64) as i32;
            unkillable_floor(ents, cards, t);
        }
        #[cfg(not(clash_plant = "strike_skips_resolve_effects"))]
        if amount > 0 && ents.hp[t] > 0 && cards.get(ents.card[t]).evo.as_ref().is_some_and(|v| v.first_hit.is_some()) && !out.hurt.contains(&ents.id_of(t)) {
            out.hurt.push(ents.id_of(t));
        }
    }
    out
}

/// What a strike landed at once leaves for the tick's Resolve (`land_at_once`): the teams whose king tower it struck, and
/// the units `resolve` would report from its hits (`ResolveOut::hurt`, `ResolveOut::shield_broke`).
#[derive(Default, Debug)]
pub struct StrikeOut {
    pub king_hit: [bool; 2],
    pub hurt: Vec<EntityId>,
    pub shield_broke: Vec<EntityId>,
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

    #[test]
    fn a_pingpong_throw_stands_on_its_previous_point_on_the_apex_tick() {
        // Capture 20260920-082459 t3068: the Executioner's axe (period 30, start 600, range 7000) stands on the same
        // point on t = 14 and 15 and never reaches its apex; a Skeleton 1504 from that point is not hit.
        let k = crate::fixed::SUBTILE_PER_MILLITILE;
        let (o, apex) = (Vec2::new(4480 * k, 8442 * k), Vec2::new(2939 * k, 15271 * k));
        let at = |t| pingpong_pos(o, apex, 600 * k, t, 30, Team::Blue);
        assert_eq!(at(15), at(14), "the apex tick moved past the tick before");
        assert_ne!(at(14), at(13), "vacuous: the throw is not moving");
        assert_ne!(at(15), apex, "the throw reached its apex");
    }
}
