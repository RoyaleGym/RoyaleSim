//! STATUS EFFECTS: the buff a unit carries, and the two arithmetics that read it.
//!
//! A buff is a row of `character_buffs` (or of a per-character TOML's `[BUFF.x]`
//! block) that an area effect, a projectile or an attack hangs on a unit for a
//! number of milliseconds. The row's columns are card data and live in `BuffDef`;
//! WHICH of them an entity carries right now is `Entities::buff_*`; what carrying
//! them does is here.
//!
//! THE COMPOSITION LAW (calibration movement.BUFF_SPEED_COMPOSITION =
//! strongest_up_and_down and combat.HIT_SPEED_BUFF = progress_scaled: one loop over
//! the unit's buff list, run once on the SpeedMultiplier column and once on the
//! HitSpeedMultiplier one):
//!
//! ```text
//!   maxpos = 100;  maxneg = 0
//!   for each buff on the unit:
//!       m = the column
//!       m > 0 -> maxpos = max(maxpos, m)
//!       m < 0 -> maxneg = max(maxneg, -m)
//!       m = 0 -> skipped
//!   neg = 100 - maxneg
//!   r   = tdiv(maxpos * value, 100)
//!   out = tdiv(max(0, min(100, neg)) * r, 100)
//! ```
//!
//! So the strongest speed-up and the strongest slow apply, nothing else does, and
//! two Rages are one Rage. Every division is TRUNCATING: the single-buff case is
//! measured on the live raged Ice Golem (52 -> 67, where round and ceil both give
//! 68; movement.BUFF_SPEED_RULE). `Freeze` ships -100 in all three multiplier
//! columns, which makes `neg` 0 and the whole product 0: a FROZEN UNIT IS A HELD
//! UNIT, and the engine runs it down the same path a Zap stun takes
//! (`Entities::held`). The Ronin's counter stun stops the walk (speed -100) and not the
//! clock (hit speed -95: 2 of every 50 ms), and under the shipped
//! status.FULL_STOP_BUFF_IS_STUN such a row is a plain buff, not a hold.
//!
//! WHERE THE TWO COMPOSITIONS ARE READ
//!   * SPEED, `Sel::Speed`: `state.rs effective_speed` (the walk, before the charge
//!     multiplier, which applies to the composed result) and the stomp clock's
//!     advance, `tdiv(compose(Speed, 100), 2)` ms per walking tick -- 65 under Rage
//!     (`movement.STOMP_PAUSE_SCHEDULE = ms_clock`, measured on the live raged Golem).
//!   * HIT SPEED, `Sel::HitSpeed`: the attack progress counter's advance,
//!     `compose(HitSpeed, TICK_MS)` per tick, and the whole progress step is SKIPPED
//!     when that is <= 0. The LOAD timer is NOT scaled -- a flat 50 comes off it
//!     every tick, held or not.
//!
#![allow(unexpected_cfgs)]
//!
//! NOTHING HERE TYPES A CARD NUMBER. Every multiplier, damage-per-second, hit
//! frequency and crown percent comes from `data/derived/cards.json`; every rule the
//! columns do not settle is a `calibration.json` `status.*` key.

use crate::fixed::Vec2;
use crate::state::PulseAmount;

/// A `character_buffs` row, as the loader read it. Copy, and printed by CardDef's
/// Debug through `BuffApply`, so the card fingerprint moves when a buff's numbers
/// move.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BuffDef {
    /// SpeedMultiplier, RAW (Rage 130, IceWizardSlowDown -30, Freeze -100). 0 = the
    /// column is blank and this buff does not touch the speed at all -- which is NOT
    /// the same as 100, because `compose` skips zeros.
    pub speed_pct: i32,
    /// HitSpeedMultiplier, raw, same convention.
    pub hit_speed_pct: i32,
    /// SpawnSpeedMultiplier, raw, same convention. Read by the spawner clocks: an
    /// action-made spawner's (spawner.ACTION_SPAWNER_SPAWN_SPEED) and, under
    /// spawner.SPAWN_SPAWNER_SPAWN_SPEED = buffed, a Spawn* spawner's (state.rs
    /// `spawner_pass`).
    pub spawn_speed_pct: i32,
    /// DamagePerSecond: a damage-over-time pulse of `dps * hit_frequency_ms / 1000`
    /// every `hit_frequency_ms`. Level-scaled at APPLICATION time, like every other
    /// spell number, and carried scaled on the slot.
    pub damage_per_second: i32,
    /// HealPerSecond, the same pulse with the sign flipped.
    pub heal_per_second: i32,
    /// HitFrequency, ms between pulses. 0 with a dps or a heal is a card the loader
    /// refuses (a pulse with no period).
    pub hit_frequency_ms: i32,
    /// CrownTowerDamagePercent, EFFECTIVE (cards.json's 100 + negative raw): Poison 23.
    pub crown_pct: i32,
    /// BuildingDamagePercent, EFFECTIVE (Earthquake 350; 100 when blank).
    pub building_pct: i32,
    /// NoEffectToCrownTowers: the pulse skips a crown tower entirely.
    pub no_effect_to_crown_towers: bool,
    /// EnableStacking: a second application of this same buff is a SECOND SLOT rather
    /// than a refresh of the first (calibration status.SAME_BUFF_REAPPLY governs the
    /// non-stacking case).
    pub enable_stacking: bool,
    /// AttractPercentage, RAW (Tornado 360). The victim takes an extra step of
    /// `tdiv(S * attract_pct, 100)` native straight at the area effect's centre every
    /// tick the effect lives, ADDED to whatever else moves it, where S is its own
    /// EFFECTIVE speed -- see `status.ATTRACT_LAW`. 0 = the column is blank.
    ///
    /// IT DOES NOT LIVE ON THE SLOT. The pull is read from the live area effect each
    /// tick (state.rs `phase_path16402`), because the buff outlives the effect by
    /// BuffTime and the corpus shows the pull stopping with the EFFECT: the Giant of
    /// 20260920-081819 has a displacement of exactly (0,0) on two ticks where its buff
    /// was still current and the area was gone.
    pub attract_pct: i32,
    /// HitTickFromSource (the Earthquake's): under status.AREA_BUFF_SOURCE_BINDING =
    /// client_source_bound the pulses fall on the clock of the area that hangs the buff, when its
    /// age crosses a multiple of `hit_frequency_ms` (spell.rs `area_bound`), not a period after
    /// the buff's first application.
    pub hit_tick_from_source: bool,
    /// ControlledByParent: under status.AREA_BUFF_SOURCE_BINDING = client_source_bound the buff is
    /// taken away when the ControlsBuff area that hung it ends (state.rs
    /// `release_orphaned_buffs`). cards.json does not carry the column yet, so this reads false.
    pub controlled_by_parent: bool,
    /// THE BUFF'S DEATH SPAWN (character_buffs DeathSpawn and its columns): a unit that dies while
    /// it carries this buff leaves a unit where it fell (state.rs `phase_reap`; the Mother Witch's
    /// VoodooCurse leaves a VoodooHog, the Goblin Curse's mark a GoblinCurseGoblin). None on every
    /// other row. `default` so a record written before the field still reads.
    #[serde(default)]
    pub death_spawn: Option<BuffDeathSpawn>,
    /// IgnoreBuildings: the buff never lands on a building or a crown tower (state.rs `land_buff`).
    #[serde(default)]
    pub ignore_buildings: bool,
    /// CrownTowerDamagePerHit, level 1 (0 = blank): what one pulse deals a crown tower instead of
    /// the crown-tower percent of the pulse (status.CROWN_TOWER_DAMAGE_PER_HIT_SCALING; the Goblin
    /// Curse's damage buff, 4). Read on a pulsing buff only; the loader refuses it on any other.
    #[serde(default)]
    pub crown_hit: i32,
    /// character_buffs Clone: the buff is the Clone spell's hold. Landing, it holds the unit (the stun timer) without
    /// the stun's resets: the unit keeps its target, its charge and its damage ramp (state.rs `apply_effects`;
    /// calibration spells.CLONE_HOLD_TARGETS). True on the Clone's row alone, so no other buff lands otherwise than
    /// before, and it keeps the row apart from an otherwise equal full stop (ZapFreeze) when the loader interns it.
    #[serde(default)]
    pub clone_hold: bool,
    /// character_buffs NotCloned: a copy the Clone makes does not take this buff from its original (state.rs
    /// `materialise_clones`, calibration spells.CLONE_COPY_BUFFS). True on a handful of rows, none of which a loaded
    /// card hangs.
    #[serde(default)]
    pub not_cloned: bool,
    /// character_buffs Invisible: no enemy may target the carrier while the buff lasts, a kept target included
    /// (target.rs `invisible_at`); area damage still lands. Measured on client 15.535.29 on the Archer Queen's cape
    /// (sp-champ-ArcherQueen-s0): every enemy targeting her drops her on the buff's first frame. `default` so a record
    /// written before the field still reads.
    #[serde(default)]
    pub invisible: bool,
    /// NO_PUSHED_BY_ALLY among the row's GameTagsToSet (the Hero Wizard's shot's HeroWizardNoMove): while it lasts the
    /// carrier's own side does not push it (move16402.rs `separation_scan_with`). `default` so a record written before
    /// the field still reads.
    #[serde(default)]
    pub no_pushed_by_ally: bool,
    /// NO_PUSHED_BY_ENEMY among the row's GameTagsToSet (the Evo Valkyrie's Valkyrie_NotPushed_BUF): while it lasts the
    /// other side does not push its carrier (move16402.rs `separation_scan_with`). `default` so a record written before
    /// the field still reads.
    #[serde(default)]
    pub no_pushed_by_enemy: bool,
    /// NO_DAMAGE among the row's GameTagsToSet (the Evo Minion Horde's ghost): while it lasts no hit lands on its carrier
    /// (combat.rs `resolve`). `default` so a record written before the field still reads.
    #[serde(default)]
    pub no_damage: bool,
    /// character_buffs DamageReduction, RAW, 1..=100 (0 = blank; the loader refuses any other value): every hit a
    /// carrier takes is scaled by (100 - DamageReduction) / 100 under status.DAMAGE_REDUCTION (combat.rs
    /// `reduce_hit`, `damage_reduction_of`). The Super Knight's shield area hangs 100, the Evo Knight's idle buff 60.
    /// `default` so a record written before the field still reads.
    #[serde(default)]
    pub damage_reduction: i32,
    /// character_buffs DamageMultiplier, RAW per cent (0 = blank): the CARRIER's own hits deal
    /// `damage x damage_pct / 100`, truncated, on its level-scaled damage (combat.rs `fire`, `own_damage`). Read on a
    /// buff a hero's button hangs on the hero itself alone (card.rs `RawBuff::convert_own`, `AbilityEffect::ActionGroup`);
    /// the loader refuses it on every other row. Measured on client 15.535.29 (sp-form-Berserker-hero-s0): the Hero
    /// Berserker's 102 at level 11 lands as 167 under BerserkerHero_buff's 164, 10 hits of 10. `default` so a record
    /// written before the field still reads.
    #[serde(default)]
    pub damage_pct: i32,
    /// character_buffs GameTagsToSet UNKILLABLE: the carrier keeps at least 1 hitpoint while the buff lasts, whatever
    /// a hit takes (combat.rs `unkillable_floor`, in `resolve` and `land_at_once`). Read where `damage_pct` is. Measured
    /// on client 15.535.29 (sp-form-Berserker-hero-s0): a Musketeer's 217 on the raged hero's 43 leaves it at 1 for the
    /// rest of the buff, and it dies after it.
    #[serde(default)]
    pub unkillable: bool,
    /// character_buffs CharacterCrownTowerDamagePercent, EFFECTIVE per cent (0 = blank): the carrier's own hit on a
    /// crown tower takes this percent in place of its card's CrownTowerDamagePercent while the buff lasts (combat.rs
    /// `fire`). Read where `damage_pct` is. UNMEASURED: no scene has the raged Hero Berserker (25; its stat screen
    /// reads -75 %) hit a tower; the percent applies after `damage_pct`, as the card's own does.
    #[serde(default)]
    pub char_crown_pct: i32,
}

/// A UNIT THAT DIES WITH THIS BUFF LIVE RELEASES `count` of `unit` (character_buffs DeathSpawn,
/// DeathSpawnCount, DeathSpawnIsEnemy, DeathSpawnDeployDelay, DeathSpawnSameLocation). Part of
/// `BuffDef`, so the card fingerprint moves when one moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BuffDeathSpawn {
    /// The released unit's CardDb index (a `summon_only` card), filled once the loader has loaded it.
    pub unit: u16,
    /// DeathSpawnCount (1 on every row the loader accepts).
    pub count: i32,
    /// DeathSpawnIsEnemy: the unit is released for the side opposite the one that died (the caster's).
    pub for_other_side: bool,
    /// DeathSpawnDeployDelay: the unit deploys (status.BUFF_DEATH_SPAWN_DEPLOY_TIME).
    pub deploy_delay: bool,
    /// DeathSpawnSameLocation: the unit stands on the dead unit's point (status.BUFF_DEATH_SPAWN_POINT).
    pub same_location: bool,
}

impl BuffDef {
    /// Does this buff do anything the engine implements? A row with no multiplier,
    /// no damage and no heal is a marker (an Invisible or a Clone flag) and the
    /// loader refuses the card that carries it rather than running it as a no-op.
    /// A buff that releases a unit when its carrier dies does something: that is its
    /// whole mechanic (the curses). So does one that reduces the damage its carrier
    /// takes (status.DAMAGE_REDUCTION), and one that multiplies its carrier's damage or
    /// keeps it alive (`damage_pct`, `unkillable`).
    pub fn is_inert(&self) -> bool {
        self.speed_pct == 0
            && self.hit_speed_pct == 0
            && self.spawn_speed_pct == 0
            && self.damage_per_second == 0
            && self.heal_per_second == 0
            && self.attract_pct == 0
            && self.death_spawn.is_none()
            && self.damage_reduction == 0
            && self.damage_pct == 0
            && !self.unkillable
            && !self.no_pushed_by_ally
            && !self.no_pushed_by_enemy
            && !self.no_damage
    }

    /// Does this buff pulse damage or healing?
    pub fn pulses(&self) -> bool {
        self.damage_per_second != 0 || self.heal_per_second != 0
    }

    /// THE LEVEL-1 AMOUNT OF ONE PULSE, positive for damage and negative for a heal
    /// (calibration status.BUFF_PULSE_AMOUNT = per_second_times_frequency): the
    /// column is per SECOND and the pulse falls every HitFrequency ms, so one pulse
    /// is `dps * HitFrequency / 1000` -- Poison 36 at 1000 ms = 36, the Heal Spirit's
    /// 157 at 250 ms = 39. 0 for a buff that does not pulse.
    pub fn pulse_base(&self) -> i32 {
        if !self.pulses() || self.hit_frequency_ms <= 0 {
            return 0;
        }
        let per_second = (self.damage_per_second - self.heal_per_second) as i64;
        (per_second * self.hit_frequency_ms as i64 / 1000) as i32
    }

    /// THE LEVEL-SCALED AMOUNT OF ONE PULSE, positive for damage and negative for a
    /// heal (calibration status.BUFF_PULSE_AMOUNT). `scale` is the caster's level
    /// scaling of a positive figure (`CardDb::scaled` for its card and level).
    ///
    /// per_second_times_frequency scales `pulse_base`: the share first, then the
    /// level. scaled_per_second_times_frequency scales the per-second figure first
    /// and takes its HitFrequency share, truncated. Measured on client 15.535.29:
    /// the Battle Healer's spawn heal (79 a second, a pulse every 250 ms) heals 50 a
    /// pulse at level 11, which is 202 / 4; the other order gives 19 scaled, 48. The
    /// two orders agree on every buff that pulses once a second. 0 for a buff that
    /// does not pulse.
    pub fn pulse_amount<E>(&self, arm: PulseAmount, scale: impl Fn(i32) -> Result<i32, E>) -> Result<i32, E> {
        match arm {
            PulseAmount::ScaledPerSecondTimesFrequency => {
                if !self.pulses() || self.hit_frequency_ms <= 0 {
                    return Ok(0);
                }
                let per_second = self.damage_per_second - self.heal_per_second;
                #[cfg(not(clash_plant = "pulse_share_scaled"))]
                let mag = (scale(per_second.abs())? as i64 * self.hit_frequency_ms as i64 / 1000) as i32;
                #[cfg(clash_plant = "pulse_share_scaled")]
                let mag = scale(self.pulse_base().abs())?; // PLANT: the share first, then the level (48, not 50).
                Ok(if per_second < 0 { -mag } else { mag })
            }
            PulseAmount::PerSecondTimesFrequency | PulseAmount::PerPulse => {
                let base = self.pulse_base();
                if base == 0 {
                    return Ok(0);
                }
                let mag = scale(base.abs())?;
                Ok(if base < 0 { -mag } else { mag })
            }
        }
    }
}

/// Which multiplier column `compose` reads. The two live passes are the same loop
/// over two columns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Sel {
    Speed,
    HitSpeed,
    SpawnSpeed,
}

impl Sel {
    #[inline]
    fn of(self, b: &BuffDef) -> i32 {
        match self {
            Sel::Speed => b.speed_pct,
            Sel::HitSpeed => b.hit_speed_pct,
            Sel::SpawnSpeed => b.spawn_speed_pct,
        }
    }
}

/// THE COMPOSITION LAW (module doc). `value` is the unbuffed
/// figure -- a speed in the engine's stored units, or a number of milliseconds --
/// and the result is what the buffed unit uses this tick. With no buff on the unit
/// the answer is `value` exactly, so composing buffs cannot move an unbuffed unit:
/// every unbuffed corpus tick scores the same with the law in as with it out.
#[inline]
pub fn compose<'a>(buffs: impl Iterator<Item = &'a BuffDef>, sel: Sel, value: i32) -> i32 {
    let mut maxpos: i32 = 100;
    let mut maxneg: i32 = 0;
    for b in buffs {
        let m = sel.of(b);
        if m > 0 {
            maxpos = maxpos.max(m);
        } else if m < 0 {
            maxneg = maxneg.max(-m);
        }
    }
    let neg = 100 - maxneg;
    // i64 because maxpos * value can overflow an i32 on a stored speed (a Hog Rider
    // at 1350 x 130 is small, but nothing in the data bounds either operand).
    #[cfg(not(clash_plant = "buff_speed_unfloored"))]
    let r = (maxpos as i64) * (value as i64) / 100;
    // PLANT (regression): the composition ROUNDS instead of truncating. The raged Ice
    // Golem's 52 x 130 / 100 = 67.6 becomes 68, which the live corpus refutes
    // (movement.BUFF_SPEED_RULE).
    #[cfg(clash_plant = "buff_speed_unfloored")]
    let r = ((maxpos as i64) * (value as i64) + 50) / 100;
    let neg = neg.clamp(0, 100) as i64;
    #[cfg(not(clash_plant = "buff_speed_unfloored"))]
    let out = (neg * r / 100) as i32;
    #[cfg(clash_plant = "buff_speed_unfloored")]
    let out = ((neg * r + 50) / 100) as i32;
    out
}

/// One buff a unit is carrying. Fixed-size and Copy: `Entities` keeps
/// `MAX_BUFFS_PER_ENTITY` of these per slot in one flat column.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct BuffSlot {
    /// Index into `CardDb::buffs`, plus one. 0 = the slot is empty, so a default
    /// `BuffSlot` is an empty one and a freshly grown column needs no fixup.
    pub id: u16,
    /// Milliseconds left. Decremented in the Status phase under
    /// status.BUFF_EXPIRY_TICK_ALIGNMENT, exactly where `stun_ms` is.
    pub ms: i32,
    /// Milliseconds until this buff's next damage / heal pulse (BuffDef::pulses
    /// only; 0 otherwise). Counts down with `ms` and reloads at `hit_frequency_ms`.
    pub pulse_ms: i32,
    /// The pulse amount for THIS application, already level-scaled by the caster:
    /// `dps * hit_frequency_ms / 1000`, positive for damage and negative for a heal.
    /// Stored rather than recomputed because the caster's level is not on the victim.
    pub pulse_amount: i32,
    /// THE AREA THIS BUFF IS BOUND TO, by its position (an area effect never moves, and a unit
    /// takes a buff row from one side's areas only): set when a ControlsBuff area hangs a
    /// ControlledByParent buff under status.AREA_BUFF_SOURCE_BINDING = client_source_bound, and the
    /// slot is emptied when no live area of this row stands there (state.rs
    /// `release_orphaned_buffs`). None on every other slot, and on every slot under not_read.
    /// `default` so a snapshot saved before it still loads.
    #[serde(default)]
    pub source: Option<Vec2>,
    /// The unified level of whatever hung this application (the caster, the attacker): the level a
    /// buff's death spawn takes under status.BUFF_DEATH_SPAWN_LEVEL = source_level. A refresh takes
    /// the latest application's. Hashed only on a slot whose buff has a death spawn, so every other
    /// slot hashes as before. `default` so a snapshot saved before it still loads.
    #[serde(default)]
    pub src_level: i32,
    /// What one pulse deals a CROWN TOWER, already scaled by the caster (BuffDef::crown_hit under
    /// status.CROWN_TOWER_DAMAGE_PER_HIT_SCALING); 0 takes the crown-tower percent of the pulse, as
    /// before the column. Hashed only when positive. `default` so a snapshot saved before it still
    /// loads.
    #[serde(default)]
    pub crown_amount: i32,
    /// The slot lands on and pulses on a building hidden under ground (`BuffHit::reach_hidden`: the Vines' snare on an
    /// idle Tesla). Hashed only when set. `default` so a snapshot saved before it still loads.
    #[serde(default)]
    pub reach_hidden: bool,
}

impl BuffSlot {
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.id == 0
    }
}

/// How many buffs one entity can carry at once. Four is what the shipped data can
/// actually stack on one unit (a slow, a damage over time, a rage and one spare), and
/// `apply` drops a fifth rather than growing.
pub const MAX_BUFFS_PER_ENTITY: usize = 4;

/// A buff an attack, a projectile or an area effect hangs on its victim: which row,
/// and for how long. `Copy`, and part of `CardDef`'s Debug, so the card fingerprint
/// moves when a card's buff moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct BuffApply {
    /// Index into `CardDb::buffs`.
    pub buff: u16,
    /// BuffTime, ms.
    pub time_ms: i32,
}

/// One buff application this tick, buffered like a stun and drained in Resolve.
/// `pulse_amount` is the caster's level-scaled pulse (see `BuffSlot`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct BuffHit {
    pub target: crate::EntityId,
    pub buff: u16,
    pub time_ms: i32,
    pub pulse_amount: i32,
    /// The pulse clock a NEW slot starts with, when the area that hangs the buff sets it
    /// (HitTickFromSource under status.AREA_BUFF_SOURCE_BINDING = client_source_bound; spell.rs
    /// `area_bound`). None: status.BUFF_PULSE_TIMING decides, as before the key.
    #[serde(default)]
    pub first_pulse_ms: Option<i32>,
    /// The area the buff is bound to (`BuffSlot::source`), or None.
    #[serde(default)]
    pub source: Option<Vec2>,
    /// The unified level of whatever hung the buff (`BuffSlot::src_level`); 0 where nothing reads it.
    #[serde(default)]
    pub src_level: i32,
    /// The hit that carried the buff sets ApplyBuffBeforeDamage (the Mother Witch's projectile): under
    /// status.APPLY_BUFF_BEFORE_DAMAGE = lands_on_a_unit_the_hit_kills a buff with a death spawn lands
    /// on a unit that same hit kills (state.rs `apply_effects`).
    #[serde(default)]
    pub before_damage: bool,
    /// The scaled crown-tower pulse (`BuffSlot::crown_amount`); 0 for the percent route.
    #[serde(default)]
    pub crown_amount: i32,
    /// THE BUFF REACHES A BUILDING HIDDEN UNDER GROUND (an action's selector whose filter does not leave one out: the
    /// Vines hold an idle Tesla, measured on client 15.535.29 -- caught, held, both pulses, and it stayed hidden). It
    /// lands on one (state.rs `apply_effects`), and its pulses hit it (`buff_pulse_pass`), where every other effect
    /// passes a hidden building by. False on every other application.
    #[serde(default)]
    pub reach_hidden: bool,
}

impl BuffHit {
    /// A plain application: `buff` for `time_ms` with `pulse_amount` a pulse, bound to nothing, carrying no
    /// source level, no crown-tower pulse and no before-damage flag. What every hit that is not an area's,
    /// a curse's or the Mother Witch's hangs.
    pub fn plain(target: crate::EntityId, buff: u16, time_ms: i32, pulse_amount: i32) -> BuffHit {
        BuffHit { target, buff, time_ms, pulse_amount, first_pulse_ms: None, source: None, src_level: 0, before_damage: false, crown_amount: 0, reach_hidden: false }
    }
}

/// A pulsing area effect standing on the ground (Poison, Earthquake). Its position
/// never moves; what it does is re-apply its buff to everything inside `radius`
/// every `hit_speed_ms` until `life_ms` runs out.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct Pulse {
    pub pos: Vec2,
    /// ms of life left.
    pub life_ms: i32,
    /// ms until the next application.
    pub next_ms: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(speed: i32) -> BuffDef {
        BuffDef { speed_pct: speed, ..BuffDef::default() }
    }

    #[test]
    fn no_buff_is_the_identity() {
        for v in [0, 1, 45, 60, 1350, 99999] {
            assert_eq!(compose([].iter(), Sel::Speed, v), v);
        }
    }

    #[test]
    fn rage_is_thirty_percent_truncated() {
        // The Ice Golem's stomped 52 -> 67, the number that settled the rounding on
        // the live 16.402 corpus (calibration movement.BUFF_SPEED_RULE).
        assert_eq!(compose([b(130)].iter(), Sel::Speed, 52), 67);
        assert_eq!(compose([b(130)].iter(), Sel::Speed, 60), 78);
        assert_eq!(compose([b(130)].iter(), Sel::Speed, 100), 130);
    }

    #[test]
    fn freeze_is_zero() {
        assert_eq!(compose([b(-100)].iter(), Sel::Speed, 1350), 0);
        // and it beats a Rage on the same unit, because the two clamps multiply.
        assert_eq!(compose([b(130), b(-100)].iter(), Sel::Speed, 1350), 0);
    }

    #[test]
    fn only_the_strongest_of_each_sign_counts() {
        // two Rages are one Rage
        assert_eq!(compose([b(130), b(130)].iter(), Sel::Speed, 60), compose([b(130)].iter(), Sel::Speed, 60));
        // the deeper slow wins
        assert_eq!(compose([b(-15), b(-30)].iter(), Sel::Speed, 100), 70);
        // a zero column is skipped, not read as 100
        assert_eq!(compose([b(0), b(130)].iter(), Sel::Speed, 60), 78);
    }

    #[test]
    fn positive_then_negative_in_that_order() {
        // tdiv(85 * tdiv(130 * 45, 100), 100) = tdiv(85 * 58, 100) = 49,
        // NOT tdiv(45 * 115, 100) = 51 and not tdiv(tdiv(45*85,100)*130,100) = 49.
        assert_eq!(compose([b(130), b(-15)].iter(), Sel::Speed, 45), 49);
    }

    #[test]
    fn the_selector_reads_its_own_column() {
        let only_hit = BuffDef { hit_speed_pct: -30, ..BuffDef::default() };
        assert_eq!(compose([only_hit].iter(), Sel::Speed, 100), 100);
        assert_eq!(compose([only_hit].iter(), Sel::HitSpeed, 100), 70);
    }
}
