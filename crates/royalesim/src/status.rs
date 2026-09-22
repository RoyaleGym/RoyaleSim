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
//! (`Entities::held`).
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
    /// SpawnSpeedMultiplier, raw, same convention. Read by nothing yet (the spawner
    /// pass is `spawner.EMISSION_TIMING`'s, and no corpus card buffs a spawner).
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
}

impl BuffDef {
    /// Does this buff do anything the engine implements? A row with no multiplier,
    /// no damage and no heal is a marker (an Invisible or a Clone flag) and the
    /// loader refuses the card that carries it rather than running it as a no-op.
    pub fn is_inert(&self) -> bool {
        self.speed_pct == 0
            && self.hit_speed_pct == 0
            && self.spawn_speed_pct == 0
            && self.damage_per_second == 0
            && self.heal_per_second == 0
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
/// the answer is `value` exactly, which is why every unbuffed corpus tick is
/// unchanged by this pass.
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
