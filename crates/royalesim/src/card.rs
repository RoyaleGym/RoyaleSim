//! Card definitions the engine runs on.
//!
//! SOURCE OF TRUTH
//!     data/derived/cards.json, produced by the data partition in the schema
//!     documented at the bottom of this file. Stats in it are the card's
//!     rarity-local LEVEL 1 values (the convention of Supercell's own CSVs).
//!
//! LEVEL SCALING
//!     Per-rarity PowerLevelMultiplier table from rarities.csv (Supercell data,
//!     loaded here, not typed in), in percent: stat(L) = base * table[L-2] / 100
//!     for rarity-local level L >= 2. This reproduces the published Hog Rider
//!     ladder 800 -> 880, 968, 1064, 1168, 1280, 1408, 1544 exactly. The old
//!     engine's 1.1^(level-1) drifts from level 4 on (1065, 1171, ...).
//!     ROUNDING: truncation toward zero (all inputs are non-negative, so this is
//!     floor). MEASURED for hitpoints on the 16.402 live captures (tests/levels.rs:
//!     Musketeer 282 x 256 % = 721.92 -> 721, Hog Rider 663 x 256 % = 1697.28 ->
//!     1697); the composition with crown-tower percent and shields is still
//!     calibration combat.DAMAGE_ARITHMETIC.
//! WHICH RARITY'S TABLE, AND FROM WHICH LEVEL (calibration.json
//!     combat.STAT_BASE_LEVEL = object_rarity_local_1, measured live): the table
//!     is the one of the OBJECT that carries the stat -- the character / projectile
//!     row's own Rarity column -- entered at unified `level - RelativeLevel(that
//!     rarity)`; the CARD's rarity only bounds the levels the card can be played at.
//!     In the 2018 files the character row carried the card's rarity, so the two
//!     coincide (Hog Rider Rare, 800 at unified 3). In the 15.535 files every base
//!     object says Common with the unified level-1 value (Hog Rider 663 at 1, 1697
//!     at 11 on the Common table; the Rare table at local 9 would give 1405, which
//!     no live Hog has). cards.json carries the decision as
//!     `level_scaling.base_level` (unified) with the ladder rarity's table and
//!     `level_scaling.reading` naming the ledger candidate; absent (the 2018 file,
//!     the fallback set, a hand-written record) means the card's own rarity from
//!     its local level 1, and a reading this loader does not implement refuses the
//!     card. The rarities themselves (level counts, relative levels, ladders) come
//!     from cards.json `rarities` when the file carries the block (15.535: five
//!     rarities plus Champion), else from the shipped 2018 rarities.csv.
//!
//! LEVEL NUMBERING
//!     `level` arguments are the unified "king-level" scale (Common 1..13 on the
//!     2018 table, 1..16 on the 15.535 one).
//!     A Rare at unified level 11 is rarity-local level 11 - RelativeLevel(2) = 9.
//!
//! FALLBACK
//!     `CardDb::fallback()` is a tiny hardcoded set for when cards.json does not
//!     exist. Its numbers are FALLBACK values copied from the 2018 CSV data and
//!     must never be mistaken for the live game. `CardDb::source` says which you
//!     got, so no caller can end up on the fallback silently.
#![allow(unexpected_cfgs)]

use crate::fixed::{milli, tiles, Vec2};
use crate::status::{BuffApply, BuffDeathSpawn, BuffDef};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::OnceLock;

const RARITIES_CSV: &str = include_str!("../../../data/raw/retroroyale-2018/csv_logic/rarities.csv");

/// Multipliers in rarities.csv are percentages.
const PERCENT: i64 = 100;
const PERCENT_I32: i32 = 100;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CardKind {
    Troop,
    Building,
    Spell,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CardSource {
    DerivedJson,
    Fallback,
}

/// A projectile an attack launches instead of hitting instantly.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ProjectileDef {
    /// Raw Speed column. Converted with SPEED_TO_SUBTILES_PER_TICK, the same
    /// multiplier as troops -- UNVERIFIED that projectile Speed shares units.
    pub speed: i32,
    /// Splash radius on arrival, subtiles (0 = single target).
    pub radius: i32,
}

/// A TROOP PROJECTILE THAT FLIES TO A RANGE instead of ending on its target
/// (projectiles.csv ProjectileRange with ProjectileRadius: the Bowler's boulder, the
/// Hunter's pellets, the Elite Archer's arrow, the Executioner's axe). Read by combat.rs
/// `fire` under calibration combat.RANGE_PROJECTILE = straight_to_range and under
/// combat.MULTIPLE_PROJECTILES = client_fan; inert under the shipped arms. On the card,
/// not on `ProjectileDef`, whose Debug is inside the format-3 card fingerprint. A row
/// with a ProjectileRange and no ProjectileRadius (the Wall Breakers' 1) is not one: it
/// could hit nothing on the way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RangeShotDef {
    /// ProjectileRange, SUBTILES: the shot ends at the last point within this of the
    /// attacker's centre at launch.
    pub range: i32,
    /// ProjectileRadius, SUBTILES: an enemy is hit when its centre comes within this plus
    /// its own radius.
    pub reach: i32,
    /// AoeToAir / AoeToGround: which victims the shot may hit on its way.
    pub hits_air: bool,
    pub hits_ground: bool,
    /// OnlyEnemies (a blank reads false, as for a spell's hit).
    pub only_enemies: bool,
    /// Pushback / PushbackAll: a victim is pushed radially from the projectile's centre.
    pub knockback: Option<KnockbackDef>,
    /// PingpongVisualTime, ms: the shot flies out and back over this long (the
    /// Executioner's axe). NOT CARRIED BY cards.json TODAY: tools/extract_cards.py does
    /// not write the column into a projectile object, so this reads None on every row
    /// and a pingpong row flies one way until the extractor carries it.
    pub pingpong_ms: Option<i32>,
    /// CheckCollisions (cards.json `check_collisions`; absent reads false): under calibration
    /// combat.PROJECTILE_COLLISIONS = client_columns the shot is gone on the tick it first
    /// hits. Set by the Hunter's pellet alone among the 15.535.29 projectile rows (its
    /// evolution's pellet extends that row).
    pub check_collisions: bool,
    /// ProjectileStartExtraRadius, SUBTILES (cards.json `projectile_start_extra_radius_milli`;
    /// absent reads 0): under combat.PROJECTILE_COLLISIONS = client_columns the shot's
    /// creation-tick test reaches `reach` plus this plus the enemy's radius.
    pub start_extra: i32,
    /// RandomDelay, ms (cards.json `random_delay_ms`; absent reads 0): under
    /// combat.PROJECTILE_COLLISIONS = client_columns the shot stands ceil(U / TICK_MS) ticks
    /// after its creation tick before its first step, U drawn uniformly from 0..RandomDelay.
    pub random_delay_ms: i32,
}

/// A CARD'S CustomFirstProjectile when it is a different row from its Projectile: the
/// Princess's PrincessProjectile, which carries her damage, against the damage-less
/// PrincessProjectileDeco of her Projectile column. (The Hunter names his own Projectile
/// row there, so his is None.) Resolved by `convert` from cards.json
/// `units.<unit>.raw.CustomFirstProjectile` against the file's `projectiles` table; read
/// by combat.rs `fire` under calibration combat.CUSTOM_FIRST_PROJECTILE =
/// client_first_of_volley.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CustomShotDef {
    /// Raw Speed column.
    pub speed: i32,
    /// Level-1 Damage (0 when blank), scaled by the attacker's level at the shot.
    pub damage: i32,
    /// Radius, SUBTILES: the splash on arrival (0 = single target).
    pub radius: i32,
    /// AoeToAir / AoeToGround: the splash's filters.
    pub hits_air: bool,
    pub hits_ground: bool,
    /// Effective crown-tower percent.
    pub crown_pct: i32,
}

/// THE SPARKS A TROOP'S SHOT RELEASES WHERE IT LANDS: its projectile row's SpawnProjectile when
/// that row is the measured shape (`spark_of`: the Firecracker's FirecrackerExplosion, SpawnCount
/// 5, Scatter "Line"). Read by combat.rs `fire` and `release_sparks` under calibration
/// combat.SPAWN_PROJECTILE = client_spark_fan; inert under the shipped not_read. On the card, not
/// on `ProjectileDef`, whose Debug is inside the format-3 card fingerprint.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SparkDef {
    /// SpawnCount: the sparks one landing releases.
    pub count: i32,
    /// Raw Speed column.
    pub speed: i32,
    /// Level-1 Damage, scaled by the attacker's level when it fires.
    pub damage: i32,
    /// Effective crown-tower percent.
    pub crown_pct: i32,
    /// ProjectileRange, SUBTILES: a spark is aimed this far from the landing point.
    pub range: i32,
    /// ProjectileRadius, SUBTILES: an enemy is hit when its centre comes within this plus its
    /// own radius.
    pub reach: i32,
    /// AoeToAir / AoeToGround.
    pub hits_air: bool,
    pub hits_ground: bool,
    /// OnlyEnemies (a blank reads false).
    pub only_enemies: bool,
}

/// A knockback a spell hit applies (card data: projectiles.csv Pushback / PushbackAll).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KnockbackDef {
    /// Displacement distance, SUBTILES (Pushback millitiles x 18). What the distance
    /// does over time is calibration (knockback.*), not card data.
    pub distance: i32,
    /// PushbackAll: overrides the victim's IgnorePushback.
    pub all: bool,
}

/// What one spell impact does to each victim. Level-1 BASE damage; the caster's
/// level scales it at cast time (`CardDb::scaled` on the spell's own card).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SpellHit {
    pub damage: i32,
    /// Effective crown-tower percent (cards.json convention: 100 + negative raw).
    pub crown_pct: i32,
    /// Area radius, SUBTILES. 0 for a rolling projectile (it uses its rectangle).
    pub radius: i32,
    pub hits_air: bool,
    pub hits_ground: bool,
    /// OnlyEnemies. False would hit both teams; no thin-slice spell ships that.
    pub only_enemies: bool,
    /// OnlyOwnTroops (area effects): the releaser's own side alone (the Battle Healer's spawn heal,
    /// Rage, the Heal Spirit's heal). A team filter, the mirror of `only_enemies`; which kinds of
    /// the own side it covers is spells.OWN_SIDE_AREA_SCOPE. IgnoreBuildings still applies on top
    /// (the Heal Spirit's heal sets both). False on every hit that is not an area's.
    pub only_own_troops: bool,
    /// IgnoreBuildings (area effects): troops only.
    pub ignore_buildings: bool,
    /// NoEffectToCrownTowers (area effects).
    pub no_effect_to_crown_towers: bool,
    pub knockback: Option<KnockbackDef>,
    /// THE BUFF THIS IMPACT HANGS ON EACH VICTIM (it replaced `stun_ms`, a
    /// stun-class special case). An area effect's Buff + BuffTime (Zap's
    /// ZapFreeze 500 ms, Freeze's Freeze 4000 ms, Poison's Poison 1000 ms) or a
    /// projectile's TargetBuff + BuffTime (the Snowball's IceWizardSlowDown 3000 ms).
    /// A buff whose composed speed is 0 -- all three -100 columns -- IS the engine's
    /// stun, and `apply_effects` drives `stun_ms` from it under
    /// status.FULL_STOP_BUFF_IS_STUN, so Zap and Freeze keep the one hold path they
    /// always had and every other buff joins it.
    pub buff: Option<BuffApply>,
    /// A SECOND BUFF THE SAME IMPACT HANGS (an area whose OnHitAction spawns two buffs: the Goblin Curse's
    /// circle hangs its damage and slow in `buff` and its mark, the buff whose carrier leaves a goblin when it
    /// dies, here). Never a pulsing buff: the loader puts the one that pulses in `buff`, so the spell's
    /// `pulse` stays that one's. None on every other hit.
    pub buff2: Option<BuffApply>,
    /// area_effect_objects CapBuffTimeToAreaEffectTime: an application of `buff` lasts no longer
    /// than the area it came from has left to live (status.AREA_BUFF_SOURCE_BINDING =
    /// client_source_bound; spell.rs `area_bound`). False on every hit that is not an area's.
    pub caps_buff_time: bool,
    /// area_effect_objects ControlsBuff: the area takes a ControlledByParent `buff` away when it
    /// ends (status.AREA_BUFF_SOURCE_BINDING = client_source_bound; state.rs
    /// `release_orphaned_buffs`). cards.json does not carry the column yet, so this reads false.
    pub controls_buff: bool,
}

/// Units a spell releases where it lands (Goblin Barrel).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SpawnDef {
    /// The spawned unit's CardDb index (a `summon_only` card).
    pub unit: u16,
    pub count: i32,
    /// SpawnCharacterDeployTime: overrides the unit's own DeployTime when present.
    pub deploy_time_ms: Option<i32>,
    /// SpawnCharacterLevelIndex: see `CardDb::spawn_level`.
    pub level_index: Option<i32>,
}

/// Where a spell may be cast. Derived from card data (docs/spell-spec.md, TERRITORY
/// BY SPELL FLAGS), never typed per card.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpellPlacement {
    /// Anywhere strictly inside the arena: river, enemy side, buildings. The 2018
    /// can_deploy_on_enemy_side / can_place_on_buildings on these rows are BLANK
    /// cells, not facts (the 2023 rows are TRUE), so they are not consumed.
    Anywhere,
    /// A spell whose projectile releases units (Goblin Barrel): anywhere except water,
    /// under calibration spells.SPAWNING_SPELL_WATER_RULE.
    AnywhereButWater,
    /// SpellAsDeploy without CanDeployOnEnemySide (The Log): the TROOP territory rule.
    /// `on_buildings` is CanPlaceOnBuildings (no footprint refusal).
    TroopTerritory { on_buildings: bool },
}

/// The mechanic a spell card runs. Every variant is fed from cards.json alone.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SpellShape {
    /// A non-homing projectile launched from calibration spells.LAUNCH_POINT to the
    /// tap, hitting (and/or releasing units) on its arrival tick. `waves` copies,
    /// wave i released i * `wave_interval_ms` later (spells_other ProjectileWaves /
    /// ProjectileWaveInterval; absent in the 2018 data, read as 1 and 0).
    Projectile { speed: i32, hit: Option<SpellHit>, waves: i32, wave_interval_ms: i32, spawn: Option<SpawnDef> },
    /// A one-shot area effect at the tap (HitSpeed blank), applied on its first
    /// update (calibration spells.ONE_SHOT_AREA_EFFECT_APPLICATION).
    AreaEffect { hit: SpellHit },
    /// A PULSING area effect at the tap (HitSpeed set): it stands on the ground for
    /// LifeDuration and applies `hit` to everything inside it every `hit_speed_ms`,
    /// starting with the tick it lands (Poison 8000 / 250, Earthquake 3000 / 100).
    /// Its damage is the BUFF's DamagePerSecond, not the area's own Damage column,
    /// which every pulsing row of the corpus leaves blank; calibration
    /// spells.PULSING_AREA_EFFECT says when the pulses fall.
    ///
    /// `child` is SpawnAreaEffectObject: a ONE-SHOT area born on this area's first update
    /// (spells.CHILD_AREA_BIRTH; Rage's RageDamage). None on Poison, Earthquake and Tornado.
    PulsingAreaEffect { hit: SpellHit, life_ms: i32, hit_speed_ms: i32, child: Option<Box<SpellShape>> },
    /// SpellAsDeploy airborne projectile that releases a rolling projectile (The Log).
    /// All distances SUBTILES; speeds raw.
    /// `spawn`: the units the roll releases where it stops (the Barbarian Barrel's Barbarian; `SpellShape::release`).
    Rolling { airborne_speed: i32, airborne_min_distance: i32, speed: i32, range: i32, half_width: i32, half_depth: i32, hit: SpellHit, spawn: Option<SpawnDef> },
    /// A HITPOINT-LESS SUMMON (Rage's bottle): a building row with a DeployTime, no Hitpoints and a
    /// DeathAreaEffect, nothing else. It is not an entity -- nothing targets it, nothing collides
    /// with it -- so it is a spell object that counts `fuse_ms` down and releases `then` where it
    /// stands (spells.SUMMON_FUSE_START).
    Fuse { fuse_ms: i32, then: Box<SpellShape> },
    /// A STRIKING AREA (Lightning: HitBiggestTargets over a Projectile row). It is not an area on the
    /// ground: it strikes one enemy at a time on its own schedule (`StrikeDef`; spell.rs `strike`).
    Strikes(Box<StrikeDef>),
    /// A SPELL THAT DEPLOYS A UNIT (the Heal Spirit card: SpellAsDeploy + SummonCharacter). It puts
    /// its unit down the way a troop card does -- the troop formation, the troop deploy timer, one
    /// tick of latency -- so state.rs `enqueue` expands it and it is never cast.
    Summon { unit: u16, count: i32 },
    /// THE MIRROR (spells_other Mirror; the Mirror card alone). It has no object of its own: playing it
    /// puts its side's last play down again, one level up, for that card's cost plus its own (state.rs
    /// `resolve_play`; calibration match.MIRROR_*). Never cast: spell.rs `cast` refuses it.
    Mirror,
    /// A CARD WITH FORMS (the 15.535.29 tables' LogicBattleSpellVariantData row: the Spirit Empress). It
    /// has no object of its own either: playing it plays the first option whose trigger the owner's
    /// elixir meets (the last when none is), as that FORM, a card of its own (state.rs `resolve_play`;
    /// calibration match.VARIANT_*). Never cast: spell.rs `cast` refuses it.
    Variant { options: Vec<VariantOption> },
    /// AN AREA WHOSE MECHANIC IS ITS SPAWN SCHEDULE (the Graveyard; the Suspicious Bush's death area). It hits
    /// nothing. Entry k puts its unit down on the area's creation tick plus the entry's delay (spell.rs `step_spells`;
    /// calibration actions.SUB_ACTIONS_DELAY and actions.SUB_TICK_DELAY_ROUNDING), at the point its offset gives
    /// (state.rs `scheduled_point`). The area lasts `life_ms`; an entry due after that never acts.
    ScheduledArea { life_ms: i32, schedule: Vec<ScheduledSpawn> },
    /// THE CLONE (area_effect_objects Clone with its ActionClone; the Clone card alone): a one-shot area at the tap
    /// that, on its first update, copies each own troop `hit` takes (spell.rs `step_spells`) and hangs `hold` on it;
    /// the copies appear in the Reap phase of that tick (state.rs `materialise_clones`) under `rules` (the table's
    /// CLONE_* globals) and calibration spells.CLONE_*. It deals nothing itself.
    Clone { hit: SpellHit, hold: BuffApply, rules: CloneRules },
}

/// ONE ENTRY OF A SCHEDULED AREA (`SpellShape::ScheduledArea`): an ActionSpawnToLocation of a unit, in the order the
/// area's action group lists it (repeats kept).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ScheduledSpawn {
    /// The entry's SubActionsDelay, ms, as the table writes it (read under actions.SUB_ACTIONS_DELAY).
    pub delay_ms: i32,
    /// The unit's CardDb index (a `summon_only` card, loaded through `UnitUse::Scheduled`); u16::MAX until resolved.
    pub unit: u16,
    /// The action's DeployTime, ms: the unit deploys this long instead of its own DeployTime. None: its own.
    pub deploy_time_ms: Option<i32>,
    /// Where the unit stands, from the area's centre.
    pub offset: SpawnOffset,
}

/// WHERE ONE SCHEDULED SPAWN STANDS, from its area's centre (`ScheduledSpawn::offset`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpawnOffset {
    /// The Graveyard's two expressions, SUBTILES: X = x + dx s, where s is -1 when the centre is past the arena's
    /// centre line (x > map_width / 2) and +1 otherwise; Y = y - dy t, where t is team_y_direction
    /// (calibration actions.TEAM_Y_DIRECTION).
    MirroredToWall { dx: i32, dy: i32 },
    /// RelativeX / RelativeY as the table writes them (the Suspicious Bush's goblins: -1 and +1). Their unit and frame
    /// are calibration spawner.RELATIVE_SPAWN_OFFSET's.
    Relative { x: i32, y: i32 },
}

/// THE CLONE'S RULES, the 15.535.29 globals.csv CLONE_* rows (cards.json `globals`; `CardGlobals`), as the loader
/// accepts them (`clone_shape` refuses the values the engine does not run).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CloneRules {
    /// CLONE_DISTANCE_Y, native: how far apart the pair slides along its owner's y axis, shared between the two over
    /// the hold (250 a tick, each 125; calibration spells.CLONE_OFFSET).
    pub distance_y: i32,
    /// CLONE_PRESERVE_SHIELD: a copy of a unit with a shield has one (spells.CLONE_HITPOINTS's shield).
    pub preserve_shield: bool,
    /// CLONE_RESET_TARGET / CLONE_RESET_CHARGE: the hold releases the original's target lock / clears its charge.
    /// Both FALSE in the 15.535.29 table, and measured so: the original keeps its target and its charge.
    pub reset_target: bool,
    pub reset_charge: bool,
    /// CLONE_DEATH_SPAWN_UNITS (and _BUILDINGS, which the loader holds equal to it): a copy's death spawns are copies
    /// (spells.CLONE_DEATH_SPAWNS).
    pub death_spawns: bool,
}

/// ONE FORM OF A VARIANT CARD (`SpellShape::Variant`): cards.json `spell.variant.options[k]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct VariantOption {
    /// AvailableManaTrigger, thousandths of an elixir: the form is played when the owner holds at least this
    /// (calibration match.VARIANT_TRIGGER_COMPARE).
    pub trigger_milli: i32,
    /// PrecastPendingTime, ms. Carried; the shipped arm of match.VARIANT_ELIXIR_MOMENT reads the elixir at
    /// the play and never this.
    pub precast_pending_ms: i32,
    /// The form: a registered troop or building CARD, resolved by its internal name (never a `units`
    /// record); u16::MAX until resolved.
    pub card: u16,
}

/// ONE MEMBER OF A DEPLOY LAID AT EXPLICIT OFFSETS (spells_characters SummonCharactersList with
/// SummonCharactersOffsetsX / Y; the Three Musketeers): cards.json `summon_members[k]`. Member 0 is the
/// card's own row (`unit` = the card's index); every other member is its own record (a `summon_only`
/// card, `UnitRef::SummonMember`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SummonMemberDef {
    /// The member's CardDb index; u16::MAX until resolved.
    pub unit: u16,
    /// SummonCharactersOffsetsX / Y, native units (millitiles), as the table gives them: laid in the
    /// owner's frame by state.rs `formation_members` under calibration formation.EXPLICIT_OFFSETS_FRAME.
    pub offset_x: i32,
    pub offset_y: i32,
}

/// THE ATTACK SELECTOR (the 15.535.29 tables' OnStartingAttackAction ActionFilter "target_in_range(V) &&
/// target_is_ground" over a two-entry AttackSequenceList; the Three Musketeers): at an attack's start
/// the unit picks its melee entry -- an instant hit on its target, no projectile -- when the target is
/// on the ground and within `melee_range`, else its ordinary projectile (state.rs `select_attack`,
/// combat.rs `fire`; calibration combat.ATTACK_SELECT_MOMENT, combat.ATTACK_SELECT_RANGE).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AttackSelectDef {
    /// The VARIABLE the condition names (ThreeMusketeer_Rework_melee_range 1600), SUBTILES.
    pub melee_range: i32,
    /// `target_is_ground`: an air target always takes the projectile.
    pub ground_only: bool,
    /// ActionDealDamage BaseDamageAmount: the melee hit's level-1 damage, scaled like Damage (123; 314 at
    /// level 11).
    pub melee_damage: i32,
}

/// A STRIKING AREA (`SpellShape::Strikes`; Lightning). Every number is the area row's or its Projectile row's.
/// Measured on client 15.535.29 (28 casts, 40 strikes): strike k falls on the cast tick + floor(k x HitSpeed / TICK_MS)
/// (spells.STRIKE_TIMER_LEFTOVER), on the eligible enemy with the highest hp (spells.STRIKE_HP_RANK) that this cast has
/// not struck, ties to the earliest created, within spells.STRIKE_REACH of the area's centre; the strike's damage and
/// TargetBuff land on the next tick, as a projectile born on the victim (spell.rs `strike`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct StrikeDef {
    /// One strike's hit on its one victim: `radius` the AREA's Radius (the selection radius, SUBTILES), `damage` and
    /// `crown_pct` the PROJECTILE row's (Lightning 413 and 25; the area's own crown percent, 100, is never read), the
    /// air / ground / enemy / building filters the area's, `buff` the projectile's TargetBuff (ZapFreeze 500 ms).
    pub hit: SpellHit,
    /// LifeDuration, ms (Lightning 1500).
    pub life_ms: i32,
    /// ms from each strike to the next, the first from the cast: HitSpeed repeated while the running sum is at most
    /// LifeDuration (`strike_gaps`; Lightning [460, 460, 460] from the 15.535.29 tables, which the shipped
    /// cards.CLIENT16402_VALUES = client16402 keeps; [500, 500, 500] when that key's value.values list the
    /// Lightning's AreaHitSpeed 500 (client 16.402), which the shipped value.values do not yet).
    pub gaps_ms: Vec<i32>,
    /// The projectile row's Speed, raw. The strike lands the next tick whatever it is: the projectile is born on its
    /// victim.
    pub speed: i32,
    /// WHAT A STRIKE AIMS AT: the highest-hp enemy (Lightning) or the area's own centre (the Royal Delivery).
    pub pick: StrikePick,
    /// THE OBJECT A CENTRE-AIMED STRIKE MAKES (`StrikePick::AreaCentre`): a `SpellShape::Projectile` of the
    /// projectile row, its hit and its SpawnCharacter, made on the centre on the strike tick and landing on the
    /// next (spell.rs `step_spells`). It is the chain's next object (`SpellShape::child`) and the shape's release
    /// (`SpellShape::release`). None on a strike that picks a victim.
    pub delivery: Option<Box<SpellShape>>,
    /// WHAT A STRIKE OF AN ACTION'S SELECTOR HANGS (`StrikePick::RankedCatches`, `StrikePick::CountTiers`; the Vines,
    /// the Void): its filter and its buffs (`SelectorDef`). None on the Lightning and the Royal Delivery.
    pub selector: Option<Box<SelectorDef>>,
}

/// What a striking area's strike aims at (`StrikeDef::pick`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StrikePick {
    /// The eligible enemy with the highest hp (spell.rs `strike`; Lightning, HitBiggestTargets).
    HighestHp,
    /// The area's own centre, whoever stands there: the strike makes its `delivery` there (the Royal Delivery's
    /// crate, a row with a Projectile and no HitBiggestTargets).
    AreaCentre,
    /// ONE CATCH A STRIKE (the Vines' ranked catches; spell.rs `catch`): the enemy of the selector's filter within
    /// its reach with the highest current hp plus shield that this cast has not caught, picked again at every catch
    /// (calibration spells.MULTI_CATCH_RANKING). It takes the selector's one buff, and a flier is held to the ground
    /// for the air-to-ground window (spells.AIR_TO_GROUND_WINDOW).
    RankedCatches,
    /// EVERY ENEMY OF THE FILTER WITHIN REACH A STRIKE (the Void's laser ball; spell.rs `laser`): each takes the
    /// buff of the tier the COUNT at that strike falls in (spells.COUNT_TIER_RULE).
    CountTiers,
}

/// THE TARGET FILTER OF AN ACTION'S SELECTOR (game_object_filters.toml; cards.json `strike_area.filter`): what a
/// Vines catch or a Void strike may take besides an ordinary spell's eligibility (spell.rs `eligible`, which keeps
/// out a unit under ground and an attached rider). A flag the filter leaves out filters nothing
/// (spells.TARGET_FILTER_ABSENT_FLAG).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SelectorFilter {
    /// FilterHidden: a building hidden under ground (a Tesla) is left out. The Void's filter sets it, the Vines' not.
    pub skip_hidden: bool,
    /// FilterUnderground: a unit under ground (entity.rs `underground`) is left out. Under the shipped
    /// movement.SPAWN_PATHFIND_BODY = untouchable no spell reaches one anyway (spell.rs `eligible`).
    pub skip_underground: bool,
    /// FilterDashImmune: a unit whose dash makes it immune to damage is left out (entity.rs `dash_immune`).
    pub skip_dash_immune: bool,
    /// FilterTags UNTARGETABLE (with NO_CHECKCOLLISIONS, NO_CHECKAVOIDANCE): a formation member still waiting out its
    /// deploy stagger is left out, the predicate target.rs `can_target` reads for it.
    pub skip_untargetable: bool,
    /// FilterBuildings false (or blank): buildings are taken.
    pub buildings: bool,
    /// FilterPrincessTowers false (or blank): the princess towers are taken.
    pub princess_towers: bool,
    /// FilterSummoner false (or blank): the king tower is taken.
    pub king_tower: bool,
}

/// THE AIR-TO-GROUND WINDOW OF A CATCH (the Vines' ActionAirToGround): a caught flier is a ground unit for its
/// targeting and for every hit's air filter until the window ends (entity.rs `grounded_ms`; spells.AIR_TO_GROUND_WINDOW
/// says how long it is from these two).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct GroundDef {
    /// TransitionDuration, ms (50).
    pub transition_ms: i32,
    /// TotalDuration, ms (2000).
    pub total_ms: i32,
}

/// A STRIKE OF AN ACTION'S SELECTOR (`StrikeDef::selector`): the filter, the buffs it hangs and, for the Void, the
/// count limits of its tiers. Every buff is delivered as a `BuffHit` on the strike tick, so it lands in that tick's
/// Resolve (spell.rs `deliver`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SelectorDef {
    pub filter: SelectorFilter,
    /// RankedCatches: the one buff a catch hangs (every size option of the Vines' select, which intern to one row).
    /// CountTiers: the tier buffs in list order, each with its SpawnTime (the Void's lv3, lv2, lv1).
    pub buffs: Vec<BuffApply>,
    /// CountTiers: MaxUnitPerActionList, one fewer than `buffs` and ascending: tier i is taken when the count is at
    /// most limits[i], the last when it is above them all (the Void's [1, 4]). Empty for RankedCatches.
    pub limits: Vec<i32>,
    /// RankedCatches: the catch's air-to-ground window. None for CountTiers.
    pub ground: Option<GroundDef>,
}

/// A striking area's gaps, ms (`StrikeDef::gaps_ms`): HitSpeed repeated while the running sum is at most LifeDuration.
/// Empty when HitSpeed outlasts the life. A strike due at exactly the LifeDuration is scheduled (500 of 1500: three);
/// spells.STRIKE_AREA_END decides whether a highest-hp strike due then falls, and a centre-aimed one due then (the
/// Royal Delivery's 2000 of 2000) falls on the update the life reaches 0 under spells.STRIKE_DUE =
/// clock_at_or_below_zero (spell.rs `step_spells`). The one rule for the table's value (`strike_shape`) and for a
/// ledger's replacement (`CardDb::with_values`, CardColumn::AreaHitSpeed).
fn strike_gaps(hit_speed_ms: i32, life_ms: i32) -> Vec<i32> {
    let mut gaps_ms = Vec::new();
    let mut sum = hit_speed_ms;
    while sum <= life_ms {
        gaps_ms.push(hit_speed_ms);
        sum += hit_speed_ms;
    }
    gaps_ms
}

/// THE STRIKING AREA a HitBiggestTargets row is (`StrikeDef`), or the reason it is refused. Exactly one shape is
/// accepted, Lightning's: HitSpeed and LifeDuration set, a Projectile row, and no Damage, Buff, MaximumTargets,
/// SpawnCharacter, child area or own-side filter of the area's own; the projectile deals Damage to one target (no
/// Radius, no Pushback) and releases nothing.
fn strike_shape(aeo: &RawAreaEffect, buffs: &mut BuffTable) -> Result<StrikeDef, String> {
    let what = aeo.name.clone().unwrap_or_default();
    let refuse = |why: &str| Err(format!("striking area effect {what}: {why}; not simulated"));
    let hit_speed_ms = match aeo.hit_speed_ms {
        Some(h) if h > 0 => h,
        _ => return refuse("no HitSpeed"),
    };
    let life_ms = match aeo.life_duration_ms {
        Some(l) if l > 0 => l,
        _ => return refuse("no LifeDuration"),
    };
    if aeo.damage.is_some() || aeo.buff.is_some() || aeo.maximum_targets.is_some() || aeo.spawn_character.is_some() {
        return refuse("the area carries its own Damage, Buff, MaximumTargets or SpawnCharacter");
    }
    if aeo.spawn_area_effect_object.is_some() || aeo.only_own_troops.unwrap_or(false) || aeo.pushback_milli.is_some() {
        return refuse("the area makes a child area, reaches its own side or pushes");
    }
    let p: RawSpellProjectile = match &aeo.projectile {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone()).map_err(|e| format!("striking area effect {what}: its projectile: {e}"))?,
        _ => return refuse("no Projectile"),
    };
    let pname = p.name.clone().unwrap_or_default();
    if p.radius_milli.is_some_and(|r| r > 0) || p.pushback_milli.is_some() || p.maximum_targets.is_some() {
        return refuse(&format!("its projectile {pname} splashes, pushes or caps its targets"));
    }
    if p.spawn_character.is_some() || p.spawn_area_effect_object.is_some() || p.spawn_projectile.is_some() || p.action_graph.is_some() {
        return refuse(&format!("its projectile {pname} releases something"));
    }
    let damage = p.damage.ok_or_else(|| format!("striking area effect {what}: its projectile {pname} deals no Damage"))?;
    let speed = p.speed.ok_or_else(|| format!("striking area effect {what}: its projectile {pname} has no Speed"))?;
    let buff = match &p.target_buff {
        None => None,
        Some(v) if v.is_null() => None,
        Some(v) => {
            let b: RawBuff = serde_json::from_value(v.clone()).map_err(|e| format!("striking area effect {what}'s projectile TargetBuff: {e}"))?;
            Some(buffs.apply(&b, p.buff_time_ms, &format!("striking area effect {what}'s projectile"))?)
        }
    };
    let gaps_ms = strike_gaps(hit_speed_ms, life_ms);
    if gaps_ms.is_empty() {
        return refuse("its HitSpeed outlasts its LifeDuration: it never strikes");
    }
    let hit = SpellHit {
        damage,
        // THE PROJECTILE's share (Lightning 25): measured on client 15.535.29, a princess tower loses 265 of a 1057
        // strike, ceil(1057 x 25 / 100); the area's own 100 is never read.
        #[cfg(not(clash_plant = "strike_crown_from_area"))]
        crown_pct: crown(p.crown_tower_damage_percent),
        #[cfg(clash_plant = "strike_crown_from_area")]
        crown_pct: crown(aeo.crown_tower_damage_percent), // PLANT: the area's 100.
        radius: milli(aeo.radius_milli.ok_or_else(|| format!("striking area effect {what} without radius"))?),
        hits_air: aeo.hits_air.unwrap_or(false),
        hits_ground: aeo.hits_ground.unwrap_or(false),
        only_enemies: aeo.only_enemies.unwrap_or(false),
        only_own_troops: false,
        ignore_buildings: aeo.ignore_buildings.unwrap_or(false),
        no_effect_to_crown_towers: aeo.no_effect_to_crown_towers.unwrap_or(false),
        knockback: None,
        buff,
        buff2: None,
        caps_buff_time: false,
        controls_buff: false,
    };
    if !hit.only_enemies {
        return refuse("it strikes both sides");
    }
    if !hit.hits_air && !hit.hits_ground {
        return refuse("it hits neither ground nor air");
    }
    Ok(StrikeDef { hit, life_ms, gaps_ms, speed, pick: StrikePick::HighestHp, delivery: None, selector: None })
}

/// THE CENTRE-AIMED STRIKE an area with a Projectile row and no HitBiggestTargets is (`StrikePick::AreaCentre`; the
/// Royal Delivery), with the unit its delivery releases as the need, or the reason it is refused. Exactly one shape is
/// accepted, the Royal Delivery's:
///   - the area: HitSpeed and LifeDuration set, and no Damage, Buff, MaximumTargets, SpawnCharacter, child area,
///     own-side filter or Pushback of its own;
///   - its projectile: Damage, Speed and a positive Radius, a SpawnCharacter with a count of at least one (a blank
///     count is refused, as on a spell projectile), and no Pushback, MaximumTargets, TargetBuff, SpawnProjectile,
///     SpawnAreaEffectObject or scripted action;
///   - the area's SpawnTime, when set, equal to the projectile's SpawnCharacterDeployTime: the two say how long the
///     released unit deploys, and a pair that disagreed would need a reading of which one wins.
///
/// The strike's `hit` is the AREA's filters (its Radius, air and ground, OnlyEnemies, IgnoreBuildings) with the
/// PROJECTILE's damage and crown share, so the spell object scales the damage the delivery deals. The strikes fall
/// every HitSpeed while the running sum is at most LifeDuration, as a striking area's do.
fn centre_strike_shape(aeo: &RawAreaEffect) -> Result<(StrikeDef, UnitNeeds), String> {
    let what = aeo.name.clone().unwrap_or_default();
    let refuse = |why: &str| Err(format!("area effect {what} with a projectile: {why}; not simulated"));
    let hit_speed_ms = match aeo.hit_speed_ms {
        Some(h) if h > 0 => h,
        _ => return refuse("no HitSpeed"),
    };
    let life_ms = match aeo.life_duration_ms {
        Some(l) if l > 0 => l,
        _ => return refuse("no LifeDuration"),
    };
    if aeo.damage.is_some() || aeo.buff.is_some() || aeo.maximum_targets.is_some() || aeo.spawn_character.is_some() {
        return refuse("the area carries its own Damage, Buff, MaximumTargets or SpawnCharacter");
    }
    if aeo.spawn_area_effect_object.is_some() || aeo.only_own_troops.unwrap_or(false) || aeo.pushback_milli.is_some() {
        return refuse("the area makes a child area, reaches its own side or pushes");
    }
    let p: RawSpellProjectile = match &aeo.projectile {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone()).map_err(|e| format!("area effect {what}: its projectile: {e}"))?,
        _ => return refuse("no Projectile"),
    };
    let pname = p.name.clone().unwrap_or_default();
    if p.pushback_milli.is_some() || p.maximum_targets.is_some() || p.target_buff.as_ref().is_some_and(|b| !b.is_null()) {
        return refuse(&format!("its projectile {pname} pushes, caps its targets or hangs a buff"));
    }
    if p.spawn_projectile.is_some() || p.spawn_area_effect_object.is_some() || p.action_graph.is_some() {
        return refuse(&format!("its projectile {pname} releases a projectile, an area or an action"));
    }
    let damage = p.damage.ok_or_else(|| format!("area effect {what}: its projectile {pname} deals no Damage"))?;
    let speed = p.speed.filter(|s| *s > 0).ok_or_else(|| format!("area effect {what}: its projectile {pname} has no Speed"))?;
    let p_radius = p.radius_milli.filter(|r| *r > 0).ok_or_else(|| format!("area effect {what}: its projectile {pname} has no Radius"))?;
    let unit = p.spawn_character.clone().ok_or_else(|| format!("area effect {what}: its projectile {pname} releases no unit; not simulated"))?;
    let count = p.spawn_character_count.filter(|c| *c > 0).ok_or_else(|| format!("area effect {what}: its projectile {pname} spawns {unit} with no count"))?;
    #[cfg(not(clash_plant = "delivery_spawn_time_unchecked"))]
    if let (Some(a), Some(b)) = (aeo.spawn_time_ms, p.spawn_character_deploy_time_ms) {
        if a != b {
            return refuse(&format!("its SpawnTime {a} and its projectile's SpawnCharacterDeployTime {b} disagree"));
        }
    }
    let mut gaps_ms = Vec::new();
    let mut sum = hit_speed_ms;
    while sum <= life_ms {
        gaps_ms.push(hit_speed_ms);
        sum += hit_speed_ms;
    }
    if gaps_ms.is_empty() {
        return refuse("its HitSpeed outlasts its LifeDuration: it never strikes");
    }
    let crown_pct = crown(p.crown_tower_damage_percent);
    let hit = SpellHit {
        damage,
        crown_pct,
        radius: milli(aeo.radius_milli.ok_or_else(|| format!("area effect {what} without radius"))?),
        hits_air: aeo.hits_air.unwrap_or(false),
        hits_ground: aeo.hits_ground.unwrap_or(false),
        only_enemies: aeo.only_enemies.unwrap_or(false),
        only_own_troops: false,
        ignore_buildings: aeo.ignore_buildings.unwrap_or(false),
        no_effect_to_crown_towers: aeo.no_effect_to_crown_towers.unwrap_or(false),
        knockback: None,
        buff: None,
        buff2: None,
        caps_buff_time: false,
        controls_buff: false,
    };
    if !hit.only_enemies {
        return refuse("it strikes both sides");
    }
    // THE DELIVERY: the projectile row's own hit (its Radius, its AoeToAir / AoeToGround and OnlyEnemies; the row
    // carries no IgnoreBuildings, spells.AREA_PROJECTILE_IGNORE_BUILDINGS) and its release.
    let delivery_hit = SpellHit {
        damage,
        crown_pct,
        radius: milli(p_radius),
        hits_air: p.aoe_to_air.unwrap_or(false),
        hits_ground: p.aoe_to_ground.unwrap_or(false),
        only_enemies: p.only_enemies.unwrap_or(false),
        only_own_troops: false,
        ignore_buildings: false,
        no_effect_to_crown_towers: false,
        knockback: None,
        buff: None,
        buff2: None,
        caps_buff_time: false,
        controls_buff: false,
    };
    if !delivery_hit.hits_air && !delivery_hit.hits_ground {
        return refuse(&format!("its projectile {pname} hits neither ground nor air"));
    }
    let spawn = SpawnDef {
        unit: u16::MAX, // resolved by from_json_str
        count,
        deploy_time_ms: p.spawn_character_deploy_time_ms,
        level_index: p.spawn_character_level_index,
    };
    let delivery = SpellShape::Projectile { speed, hit: Some(delivery_hit), waves: 1, wave_interval_ms: 0, spawn: Some(spawn) };
    Ok((StrikeDef { hit, life_ms, gaps_ms, speed, pick: StrikePick::AreaCentre, delivery: Some(Box::new(delivery)), selector: None }, vec![(UnitUse::Spell, unit)]))
}

/// THE TARGET FILTER a selector reads (`SelectorFilter`), or the reason it is refused. Accepted: enemies, characters,
/// and only the flags and tags the engine runs (FilterInvisible, FilterFlying and FilterCloning set are refused; a
/// blank flag filters nothing, spells.TARGET_FILTER_ABSENT_FLAG).
fn selector_filter(f: &RawTargetFilter) -> Result<SelectorFilter, String> {
    let name = f.name.clone().unwrap_or_default();
    if f.match_team_enemy != Some(true) || f.match_team_own == Some(true) {
        return Err(format!("target filter {name} does not take enemies alone"));
    }
    if f.match_type_characters != Some(true) {
        return Err(format!("target filter {name} does not take characters"));
    }
    for (flag, set) in [("FilterInvisible", f.filter_invisible), ("FilterFlying", f.filter_flying), ("FilterCloning", f.filter_cloning)] {
        if set == Some(true) {
            return Err(format!("target filter {name}: {flag} is not simulated"));
        }
    }
    let tags = f.tags.clone().unwrap_or_default();
    if let Some(t) = tags.iter().find(|t| !matches!(t.as_str(), "NO_CHECKAVOIDANCE" | "NO_CHECKCOLLISIONS" | "UNTARGETABLE")) {
        return Err(format!("target filter {name}: the tag {t} is not simulated"));
    }
    #[cfg(not(clash_plant = "vines_skips_hidden"))]
    let skip_hidden = f.filter_hidden == Some(true);
    #[cfg(clash_plant = "vines_skips_hidden")]
    let skip_hidden = true; // PLANT: a blank FilterHidden read as set, so the Vines leave a hidden Tesla alone.
    Ok(SelectorFilter {
        skip_hidden,
        skip_underground: f.filter_underground == Some(true),
        skip_dash_immune: f.filter_dash_immune == Some(true),
        skip_untargetable: tags.iter().any(|t| t == "UNTARGETABLE"),
        buildings: f.filter_buildings != Some(true),
        princess_towers: f.filter_princess_towers != Some(true),
        king_tower: f.filter_summoner != Some(true),
    })
}

/// THE STRIKING AREA WHOSE STRIKES ARE AN ACTION (cards.json `strike_area`: the Vines' ranked catches, the Void's
/// laser ball), or the reason it is refused. Both kinds: the area carries no Buff, Projectile, SpawnCharacter,
/// MaximumTargets, Pushback, child area, own-side filter, HitBiggestTargets or positive HitSpeed of its own, has a
/// LifeDuration, and its own Damage is not read when it hits neither ground nor air (the Void's 100; calibration
/// spells.AREA_DAMAGE_WITHOUT_HIT_FLAGS = inert), refused when it hits either. Then:
///   - ranked_catches: once per target, ranked by current hp plus shield, catch offsets ascending, every size option
///     ONE buff row by value with one time (the Vines' seven snares), an air-to-ground action that is a singleton, is
///     not aborted by its caster's death and lets the ground tag stand while idle. The gaps are the start delay plus
///     the first offset, then the offsets' differences (the Vines: [900, 50, 100]), each running sum within the life;
///   - laser_ball: tiers one more than the count limits, the limits ascending and positive, each tier a buff that
///     deals damage over time and nothing else, lasting one pulse (its time is its HitFrequency) with a
///     CrownTowerDamagePerHit. The gaps are the start delay plus FirstHitDelay, then HitFrequency while the running
///     sum is within the life (the Void: [1500, 1200, 1200]).
///
/// The strike's `hit` is a filter only (damage 0, both air and ground, enemies): the selector's filter decides.
fn strike_area_shape(aeo: &RawAreaEffect, sa: &RawStrikeArea, buffs: &mut BuffTable) -> Result<StrikeDef, String> {
    let what = aeo.name.clone().unwrap_or_default();
    let refuse = |why: String| -> Result<StrikeDef, String> { Err(format!("striking area effect {what}: {why}; not simulated")) };
    if aeo.buff.is_some()
        || aeo.projectile.as_ref().is_some_and(|p| !p.is_null())
        || aeo.spawn_character.is_some()
        || aeo.maximum_targets.is_some()
        || aeo.pushback_milli.is_some()
        || aeo.spawn_area_effect_object.is_some()
        || aeo.only_own_troops == Some(true)
        || aeo.hit_biggest_targets == Some(true)
    {
        return refuse("the area carries its own Buff, Projectile, SpawnCharacter, MaximumTargets, Pushback, child area, own-side filter or HitBiggestTargets beside its action".into());
    }
    if aeo.damage.is_some() && (aeo.hits_ground == Some(true) || aeo.hits_air == Some(true)) {
        return refuse("it carries its own Damage and its strikes".into());
    }
    if aeo.hit_speed_ms.is_some_and(|h| h > 0) {
        return refuse("a HitSpeed beside its action".into());
    }
    let Some(life_ms) = aeo.life_duration_ms.filter(|l| *l > 0) else { return refuse("no LifeDuration".into()) };
    let Some(raw_filter) = sa.filter.as_ref() else { return refuse("its action names no target filter".into()) };
    let filter = selector_filter(raw_filter).map_err(|e| format!("striking area effect {what}: {e}; not simulated"))?;
    let Some(start) = sa.start_delay_ms.filter(|d| *d >= 0) else { return refuse("its action has no start delay".into()) };
    let hit_of = |radius: i32| SpellHit {
        damage: 0,
        crown_pct: crown(aeo.crown_tower_damage_percent),
        radius: milli(radius),
        hits_air: true,
        hits_ground: true,
        only_enemies: true,
        only_own_troops: false,
        ignore_buildings: false,
        no_effect_to_crown_towers: false,
        knockback: None,
        buff: None,
        buff2: None,
        caps_buff_time: false,
        controls_buff: false,
    };
    // PLANT (void_area_damage_loaded): the area's own Damage is read onto the strike, whose victims then take it.
    #[cfg(clash_plant = "void_area_damage_loaded")]
    let hit_of = |radius: i32| SpellHit { damage: aeo.damage.unwrap_or(0), ..hit_of(radius) };
    match sa.kind.as_deref() {
        Some("ranked_catches") => {
            if sa.once_per_target != Some(true) {
                return refuse("catches that may take one target twice".into());
            }
            if sa.selection_mode.as_deref() != Some("HighestCurrentHpIncludeShields") {
                return refuse(format!("catches ranked by {:?}", sa.selection_mode));
            }
            let Some(radius) = sa.radius_milli.filter(|r| *r > 0) else { return refuse("catches with no shape radius".into()) };
            let offsets = sa.catch_offsets_ms.clone().unwrap_or_default();
            if offsets.is_empty() || offsets[0] < 0 || offsets.windows(2).any(|w| w[1] <= w[0]) {
                return refuse(format!("catch offsets {offsets:?} that are not ascending"));
            }
            // The Delays are offsets from the selector's start (measured on client 15.535.29: catches on C + 18, 19 and
            // 21 for 0, 50 and 150), so a gap is the difference of two.
            let mut gaps_ms = vec![start + offsets[0]];
            #[cfg(not(clash_plant = "vines_delays_cumulative"))]
            gaps_ms.extend(offsets.windows(2).map(|w| w[1] - w[0]));
            #[cfg(clash_plant = "vines_delays_cumulative")]
            gaps_ms.extend(offsets.iter().skip(1).copied()); // PLANT: each Delay read as the gap after the catch before it.
            let mut sum = 0;
            for g in &gaps_ms {
                sum += g;
                if sum > life_ms {
                    return refuse(format!("a catch {sum} ms after the cast, past its LifeDuration {life_ms}"));
                }
            }
            let Some(ag) = sa.air_to_ground.as_ref() else { return refuse("a catch without its air-to-ground action".into()) };
            if ag.singleton != Some(true) || ag.abort_if_instigator_dies != Some(false) || ag.allow_is_ground_tag_on_idle != Some(true) {
                return refuse("an air-to-ground action that is not a singleton, is aborted by its caster's death or drops the ground tag while idle".into());
            }
            let (Some(transition_ms), Some(total_ms)) = (ag.transition_ms.filter(|t| *t >= 0), ag.total_ms.filter(|t| *t > 0)) else {
                return refuse("an air-to-ground action with no durations".into());
            };
            let options = sa.options.as_deref().unwrap_or_default();
            let times = sa.option_time_ms.clone().unwrap_or_default();
            if options.is_empty() || times.len() != options.len() {
                return refuse("a catch whose size select has no buffs".into());
            }
            let mut buff: Option<BuffApply> = None;
            for (b, t) in options.iter().zip(times) {
                let got = buffs.apply(b, Some(t), &format!("striking area effect {what}'s catch"))?;
                match buff {
                    None => buff = Some(got),
                    Some(one) if one == got => {}
                    Some(_) => return refuse("a size select whose buffs differ in a mechanic column".into()),
                }
            }
            let buff = buff.expect("options is not empty");
            Ok(StrikeDef {
                hit: hit_of(radius),
                life_ms,
                gaps_ms,
                speed: 0,
                pick: StrikePick::RankedCatches,
                delivery: None,
                selector: Some(Box::new(SelectorDef { filter, buffs: vec![buff], limits: Vec::new(), ground: Some(GroundDef { transition_ms, total_ms }) })),
            })
        }
        Some("laser_ball") => {
            let Some(radius) = sa.detection_radius_milli.filter(|r| *r > 0) else { return refuse("a laser ball with no DetectionRadius".into()) };
            let (Some(first_ms), Some(every_ms)) = (sa.first_hit_delay_ms.filter(|d| *d >= 0), sa.hit_frequency_ms.filter(|h| *h > 0)) else {
                return refuse("a laser ball with no FirstHitDelay or HitFrequency".into());
            };
            let limits = sa.max_units_per_list.clone().unwrap_or_default();
            let tiers = sa.tiers.as_deref().unwrap_or_default();
            if limits.is_empty() || limits[0] <= 0 || limits.windows(2).any(|w| w[1] <= w[0]) || tiers.len() != limits.len() + 1 {
                return refuse(format!("count limits {limits:?} for {} tiers", tiers.len()));
            }
            let mut tier_buffs = Vec::new();
            for tier in tiers {
                let Some(b) = tier.buff.as_ref() else { return refuse("a tier with no buff".into()) };
                let name = b.name.clone().unwrap_or_default();
                let def = b.convert(&format!("striking area effect {what}'s tier"))?;
                let one_pulse = def.damage_per_second > 0
                    && def.heal_per_second == 0
                    && def.speed_pct == 0
                    && def.hit_speed_pct == 0
                    && def.spawn_speed_pct == 0
                    && def.attract_pct == 0
                    && def.death_spawn.is_none()
                    && def.crown_hit > 0
                    && tier.time_ms == Some(def.hit_frequency_ms);
                if !one_pulse {
                    return refuse(format!("the tier buff {name} is not one pulse of damage with its own crown-tower figure"));
                }
                // AddAsIndividualBuff is read either way: a tier lives one pulse, so a second application before it
                // ends would need two strikes 100 ms apart (status.BUFF_STACKING names the gap).
                let _ = tier.add_as_individual_buff;
                tier_buffs.push(buffs.apply(b, tier.time_ms, &format!("striking area effect {what}'s tier"))?);
            }
            let mut gaps_ms = vec![start + first_ms];
            let mut sum = start + first_ms;
            if sum > life_ms {
                return refuse("its first strike falls past its LifeDuration".into());
            }
            while sum + every_ms <= life_ms {
                gaps_ms.push(every_ms);
                sum += every_ms;
            }
            Ok(StrikeDef {
                hit: hit_of(radius),
                life_ms,
                gaps_ms,
                speed: 0,
                pick: StrikePick::CountTiers,
                delivery: None,
                selector: Some(Box::new(SelectorDef { filter, buffs: tier_buffs, limits, ground: None })),
            })
        }
        other => refuse(format!("a strike_area of kind {other:?}")),
    }
}

/// THE CLONE (area_effect_objects Clone: an own-side one-shot area whose OnHitAction is an ActionClone; cards.json
/// `clone` and `clone_action`), or None when the area is not one, and the caller's refusal stands. None also for an
/// area that carries its own Buff (the GlobalClone event's), which is refused by its action graph as before.
///
/// Refused, with the reason: an OnClonedAction that is not one BuffType spawn of a buff whose row sets Clone; a
/// HitSpeed; the area's own Damage, Pushback, Projectile, SpawnCharacter, MaximumTargets, child area or strikes; an
/// area that does not reach its own side alone; a CLONE_* global the file does not carry; and the rules the engine
/// does not run: CLONE_LEVEL_OFFSET or CLONE_DISTANCE_X not 0, CLONE_DISTANCE_Y not a positive even number,
/// CLONE_MOVE_PARENT false, CLONE_CLONED_UNITS or CLONE_INHERIT_CHARGE true, CLONE_DEATH_SPAWN_UNITS and _BUILDINGS
/// apart.
fn clone_shape(aeo: &RawAreaEffect, buffs: &mut BuffTable, ctx: &LoadCtx) -> Option<Result<(SpellShape, UnitNeeds), String>> {
    if aeo.clone != Some(true) || aeo.buff.is_some() {
        return None;
    }
    let action = aeo.clone_action.as_ref()?;
    Some(clone_shape_of(aeo, action, buffs, ctx))
}

fn clone_shape_of(aeo: &RawAreaEffect, action: &RawCloneAction, buffs: &mut BuffTable, ctx: &LoadCtx) -> Result<(SpellShape, UnitNeeds), String> {
    let what = aeo.name.clone().unwrap_or_default();
    let refuse = |why: String| -> Result<(SpellShape, UnitNeeds), String> { Err(format!("clone area effect {what}: {why}; not simulated")) };
    let Some(on) = action.on_cloned.as_ref() else { return refuse("its ActionClone runs nothing on a copy".into()) };
    if on.spawn_type.as_deref() != Some("BuffType") {
        return refuse(format!("its OnClonedAction spawns {:?} {:?}, not a buff", on.spawn_type, on.spawn));
    }
    let Some(raw_hold) = on.buff.as_ref() else { return refuse(format!("its OnClonedAction's buff {:?} has no row", on.spawn)) };
    if raw_hold.clone != Some(true) {
        return refuse(format!("its OnClonedAction's buff {} is not the Clone's hold", raw_hold.name.clone().unwrap_or_default()));
    }
    if aeo.hit_speed_ms.is_some_and(|h| h > 0) {
        return refuse("a HitSpeed".into());
    }
    if aeo.damage.is_some()
        || aeo.pushback_milli.is_some()
        || aeo.projectile.as_ref().is_some_and(|p| !p.is_null())
        || aeo.spawn_character.is_some()
        || aeo.maximum_targets.is_some()
        || aeo.spawn_area_effect_object.is_some()
        || aeo.hit_biggest_targets == Some(true)
        || aeo.strike_area.is_some()
    {
        return refuse("its own Damage, Pushback, Projectile, SpawnCharacter, MaximumTargets, child area or strikes".into());
    }
    if aeo.only_own_troops != Some(true) || aeo.only_enemies == Some(true) {
        return refuse("it does not reach its own side alone".into());
    }
    let g = ctx.globals;
    for name in CLONE_GLOBALS {
        if g.clone_number(name).is_none() && g.clone_flag(name).is_none() {
            return Err(format!("clone area effect {what}: Clone rules missing from cards.json: {name}"));
        }
    }
    let num = |n: &str| g.clone_number(n).unwrap_or(0);
    let flag = |n: &str| g.clone_flag(n).unwrap_or(false);
    let bad = |n: &str, v: String| -> Result<(SpellShape, UnitNeeds), String> { Err(format!("clone area effect {what}: Clone rules: {n} {v} is not simulated")) };
    #[cfg(not(clash_plant = "clone_rules_unchecked"))]
    if num("CLONE_LEVEL_OFFSET") != 0 {
        return bad("CLONE_LEVEL_OFFSET", num("CLONE_LEVEL_OFFSET").to_string());
    }
    if num("CLONE_DISTANCE_X") != 0 {
        return bad("CLONE_DISTANCE_X", num("CLONE_DISTANCE_X").to_string());
    }
    let distance_y = num("CLONE_DISTANCE_Y");
    if distance_y <= 0 || distance_y % 2 != 0 {
        return bad("CLONE_DISTANCE_Y", distance_y.to_string());
    }
    if !flag("CLONE_MOVE_PARENT") {
        return bad("CLONE_MOVE_PARENT", "FALSE".into());
    }
    for n in ["CLONE_CLONED_UNITS", "CLONE_INHERIT_CHARGE"] {
        if flag(n) {
            return bad(n, "TRUE".into());
        }
    }
    if flag("CLONE_DEATH_SPAWN_UNITS") != flag("CLONE_DEATH_SPAWN_BUILDINGS") {
        return bad("CLONE_DEATH_SPAWN_BUILDINGS", format!("{} beside CLONE_DEATH_SPAWN_UNITS {}", flag("CLONE_DEATH_SPAWN_BUILDINGS"), flag("CLONE_DEATH_SPAWN_UNITS")));
    }
    let rules = CloneRules {
        distance_y,
        preserve_shield: flag("CLONE_PRESERVE_SHIELD"),
        reset_target: flag("CLONE_RESET_TARGET"),
        reset_charge: flag("CLONE_RESET_CHARGE"),
        death_spawns: flag("CLONE_DEATH_SPAWN_UNITS"),
    };
    let hold = buffs.apply(raw_hold, on.spawn_time_ms, &format!("clone area effect {what}'s hold"))?;
    let hit = SpellHit {
        damage: 0,
        crown_pct: crown(aeo.crown_tower_damage_percent),
        radius: milli(aeo.radius_milli.ok_or_else(|| format!("clone area effect {what} without radius"))?),
        hits_air: aeo.hits_air.unwrap_or(false),
        hits_ground: aeo.hits_ground.unwrap_or(false),
        only_enemies: false,
        only_own_troops: true,
        ignore_buildings: aeo.ignore_buildings.unwrap_or(false),
        no_effect_to_crown_towers: aeo.no_effect_to_crown_towers.unwrap_or(false),
        knockback: None,
        buff: None,
        buff2: None,
        caps_buff_time: false,
        controls_buff: false,
    };
    if !hit.hits_air && !hit.hits_ground {
        return refuse("it hits neither ground nor air".into());
    }
    Ok((SpellShape::Clone { hit, hold, rules }, Vec::new()))
}

impl SpellShape {
    /// THE UNITS THIS SHAPE RELEASES: a projectile's where it lands (the Goblin Barrel), a roll's where it stops
    /// (the Barbarian Barrel). None for every other shape. Every reader of a spell's released units goes through
    /// this (`CardDb::spawn_level`, `unit_refs`, the loader's resolution), so a new releasing shape joins them all.
    pub fn release(&self) -> Option<&SpawnDef> {
        match self {
            SpellShape::Projectile { spawn, .. } | SpellShape::Rolling { spawn, .. } => spawn.as_ref(),
            // A centre-aimed strike releases what its delivery releases (the Royal Delivery's Recruit).
            SpellShape::Strikes(d) => d.delivery.as_deref().and_then(SpellShape::release),
            _ => None,
        }
    }

    /// The slot `release` reads, to fill or to clear.
    pub fn release_slot(&mut self) -> Option<&mut Option<SpawnDef>> {
        match self {
            SpellShape::Projectile { spawn, .. } | SpellShape::Rolling { spawn, .. } => Some(spawn),
            SpellShape::Strikes(d) => d.delivery.as_deref_mut().and_then(SpellShape::release_slot),
            _ => None,
        }
    }

    /// The next object of this shape's chain: what a `Fuse` releases, a pulsing area's child, or the object a
    /// centre-aimed strike makes (`StrikeDef::delivery`).
    pub fn child(&self) -> Option<&SpellShape> {
        match self {
            SpellShape::Fuse { then, .. } => Some(then),
            SpellShape::PulsingAreaEffect { child, .. } => child.as_deref(),
            SpellShape::Strikes(d) => d.delivery.as_deref(),
            _ => None,
        }
    }

    /// THE ENTRIES OF THE SCHEDULED AREA down this shape's chain (`SpellShape::ScheduledArea`), or None. Every reader of
    /// a scheduled area's units goes through this (`CardDb::unit_refs`, the loader's resolution).
    pub fn schedule(&self) -> Option<&[ScheduledSpawn]> {
        match self {
            SpellShape::ScheduledArea { schedule, .. } => Some(schedule),
            _ => self.child().and_then(SpellShape::schedule),
        }
    }

    /// `schedule`, to fill.
    fn schedule_mut(&mut self) -> Option<&mut Vec<ScheduledSpawn>> {
        match self {
            SpellShape::ScheduledArea { schedule, .. } => Some(schedule),
            SpellShape::Fuse { then, .. } => then.schedule_mut(),
            SpellShape::PulsingAreaEffect { child: Some(c), .. } => c.schedule_mut(),
            SpellShape::Strikes(d) => d.delivery.as_deref_mut().and_then(SpellShape::schedule_mut),
            _ => None,
        }
    }

    /// EVERY BUFF THIS SHAPE'S CHAIN CAN HANG, into `out` (each index once, in chain order: `buff`, then `buff2`,
    /// then the next object's). Read by `card_buffs`.
    fn buffs_into(&self, out: &mut Vec<u16>) {
        let hit = match self {
            SpellShape::Projectile { hit, .. } => hit.as_ref(),
            SpellShape::AreaEffect { hit } | SpellShape::PulsingAreaEffect { hit, .. } | SpellShape::Rolling { hit, .. } => Some(hit),
            SpellShape::Strikes(d) => Some(&d.hit),
            SpellShape::Clone { hit, .. } => Some(hit),
            SpellShape::Fuse { .. } | SpellShape::Summon { .. } | SpellShape::Mirror | SpellShape::Variant { .. } | SpellShape::ScheduledArea { .. } => None,
        };
        if let Some(h) = hit {
            for b in [h.buff, h.buff2].into_iter().flatten() {
                if !out.contains(&b.buff) {
                    out.push(b.buff);
                }
            }
        }
        // A selector's buffs (the Vines' snare, the Void's tiers) and the Clone's hold, after the hit's.
        let extra: Vec<u16> = match self {
            SpellShape::Strikes(d) => d.selector.as_ref().map_or(Vec::new(), |s| s.buffs.iter().map(|b| b.buff).collect()),
            SpellShape::Clone { hold, .. } => vec![hold.buff],
            _ => Vec::new(),
        };
        for b in extra {
            if !out.contains(&b) {
                out.push(b);
            }
        }
        if let Some(c) = self.child() {
            c.buffs_into(out);
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SpellDef {
    pub shape: SpellShape,
    pub placement: SpellPlacement,
}

/// A building that hides when it is not attacking (buildings.csv
/// HidesWhenNotAttacking / HideTimeMs / UpTimeMs; 2018 and 15.535 both: Tesla
/// 800 / 800). The state machine is entity.rs `HideState`, driven by state.rs
/// `hide_pass`; every rule the columns do not settle is a calibration.json `hide.*`
/// key. `HideBeforeFirstHit` (blank on every 2018 row) is not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HideDef {
    /// ms the building stays up without attacking before it hides again.
    pub hide_time_ms: i32,
    /// ms from the wake trigger to being up (able to target and attack).
    pub up_time_ms: i32,
}

/// A PERIODIC SPAWNER (characters.csv / buildings.csv SpawnCharacter, SpawnNumber,
/// SpawnInterval, SpawnPauseTime, SpawnStartTime, SpawnLimit, SpawnRadius; 2018:
/// Tombstone, GoblinHut, BarbarianHut, FirespiritHut, Witch, DarkWitch). The
/// column semantics were checked against the 15.535 csv_logic (same names, same
/// meanings) and the community reading; every rule the columns do not settle is
/// a calibration.json `spawner.*` key. Driven by state.rs `spawner_pass` (Spawn
/// phase); the per-entity timers are entity.rs `spawn_ms` / `spawn_wave_left`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnerDef {
    /// The spawned unit's CardDb index (a `summon_only` card; resolved by
    /// `CardDb::from_json_str`, which loads it).
    pub unit: u16,
    /// SpawnNumber: units per wave (>= 1).
    pub number: i32,
    /// SpawnInterval: ms between the units of ONE wave (Witch 300). Blank = all at
    /// once, read as 0 -- a column semantic, not a guess (the community reading and
    /// the only one under which a blank cell on a multi-unit wave means anything).
    pub interval_ms: i32,
    /// SpawnStartTime: ms to the first wave -- from ACTIVATION (deploy time over)
    /// or from PLACEMENT, calibration spawner.START_TIME_ORIGIN (every 15.535
    /// spawning troop ships SpawnStartTime == DeployTime, which reads like a
    /// placement-relative timer; the 2018 DarkWitch 1500 / 1000 does not settle it).
    /// Blank on every hut: calibration spawner.FIRST_WAVE decides.
    pub start_time_ms: Option<i32>,
    /// SpawnPauseTime: ms between waves (> 0).
    pub pause_time_ms: i32,
    /// SpawnLimit: the most units from THIS spawner alive (or queued) at once.
    /// Blank = unlimited (no 2018 row sets it).
    pub limit: Option<i32>,
    /// SpawnRadius, SUBTILES: how far from the spawner the units appear. Blank on
    /// every hut and the Witch: calibration spawner.SPAWN_POINT decides.
    pub radius: Option<i32>,
    /// WHERE THE BLOCK CAME FROM: the Spawn* columns, or an ActionInterval running an
    /// ActionSpawnToLocation (the Furnace; cards.json `interval_spawner`, `interval_spawner_of`).
    /// An interval block's first unit is timed by spawner.INTERVAL_START_ORIGIN from
    /// StartCounterAt (`start_time_ms`), its waves by Interval (`pause_time_ms`), and its clock
    /// by spawner.ACTION_SPAWNER_SPAWN_SPEED (state.rs `spawner_pass`).
    pub source: SpawnerSource,
    /// ActionSpawnToLocation MirroredX / MirroredY, raw: half tiles in the owner's frame under
    /// spawner.SPAWN_TO_LOCATION_OFFSET. None on a Spawn* block (spawner.SPAWN_POINT decides).
    pub to_location: Option<(i32, i32)>,
    /// The ActionSpawnToLocation's own DeployTime, ms: the emitted unit deploys for this long
    /// instead of its own DeployTime (the Furnace's Fire Spirit 500 against its own 1000). None on
    /// a Spawn* block (spawner.SPAWNED_DEPLOY_TIME decides) and on an action that sets none.
    pub emit_deploy_ms: Option<i32>,
}

/// Which columns a `SpawnerDef` came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpawnerSource {
    /// The Spawn* columns (Tombstone, the huts, the Witch).
    Columns,
    /// An ActionInterval running an ActionSpawnToLocation (the Furnace).
    ActionInterval,
}

/// AN ATTACHED RIDER (the 15.535.29 tables: SpawnAttach on a SpawnCharacter block with a
/// SpawnNumber and a blank SpawnPauseTime; the Ram Rider's Ram carries its rider so, the Goblin
/// Giant its two Spear Goblins). Not a periodic spawner: the riders exist from the mount's first
/// tick, stand where the mount stood a tick before and fight on their own (state.rs
/// `spawn_riders`, `carry_riders`; calibration rider.*). Taken on a troop only (`convert_attach`):
/// one rider with a blank SpawnRadius stands on its mount's centre (the Ram Rider's), and riders
/// with a SpawnRadius stand on an arc behind the mount (the Goblin Giant's; rider.OFFSET_LAW).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttachDef {
    /// The rider's CardDb index (a `summon_only` card, loaded through `UnitUse::Attach`).
    pub unit: u16,
    /// SpawnNumber: riders per mount. More than 1 only with a SpawnRadius (`convert_attach`).
    pub number: i32,
    /// SpawnRadius, SUBTILES: how far behind the mount its riders stand (calibration rider.OFFSET_LAW; the Goblin
    /// Giant's 900). None on the Ram Rider's block: its rider stands on the mount's centre.
    pub radius: Option<i32>,
}

/// A DEATH SPAWN (DeathSpawnCharacter / DeathSpawnCount / DeathSpawnRadius /
/// DeathSpawnDeployTime; 2018: Tombstone 4 Skeletons, Golem 2 Golemites, LavaHound
/// 6 LavaPups, BattleRam 2 Barbarians, DarkWitch 3 Bats). Fired by state.rs
/// `phase_reap` for every death that goes through the death queue (hp <= 0 from
/// any hit, the lifetime expiry hit included); never for a scenario removal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathSpawnDef {
    /// The spawned unit's CardDb index (a `summon_only` card).
    pub unit: u16,
    pub count: i32,
    /// DeathSpawnRadius, SUBTILES. None: calibration spawner.DEATH_SPAWN_RADIUS_DEFAULT.
    pub radius: Option<i32>,
    /// DeathSpawnDeployTime: overrides the unit's own DeployTime. None: calibration
    /// spawner.DEATH_SPAWN_DEPLOY_TIME_DEFAULT.
    pub deploy_time_ms: Option<i32>,
}

/// A CHARGE (characters.csv ChargeRange / DamageSpecial / ChargeSpeedMultiplier;
/// 2018: Prince 250 / 490 / 200, DarkPrince 250 / 290 / 200, BattleRam 300 / 280 /
/// 200). The unit walks a run-up, then moves at the multiplied speed and its next
/// landed hit deals DamageSpecial INSTEAD of Damage (DamageSpecial is exactly
/// 2 x Damage on all three rows, the same relation DashDamage has on Assassin and
/// MegaKnight where replacement is unambiguous). Every rule the columns do not
/// settle is a calibration.json `charge.*` key; the state is entity.rs
/// `charge_progress` / `charged`, driven by state.rs `charge_pass` (Move phase),
/// `effective_speed` and `phase_attack`, and combat.rs `fire`.
///
/// ALL THREE NUMBERS ARE RAW. `range_raw`'s UNIT is not established by the data
/// alone (250 is neither millitiles nor ms under this file's own conventions), so
/// it is NOT converted here: calibration charge.CHARGE_RANGE_UNIT and
/// charge.ACCUMULATOR decide in state.rs, where Calib is in scope.
/// `damage_special` is the rarity-local LEVEL-1 value like every other stat.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChargeDef {
    /// ChargeRange, raw.
    pub range_raw: i32,
    /// DamageSpecial, level 1, raw.
    pub damage_special: i32,
    /// ChargeSpeedMultiplier, percent (200 = twice the speed).
    pub speed_multiplier_percent: i32,
}

/// THE RIVER JUMP (characters.csv JumpEnabled / JumpHeight / JumpSpeed; 2018: HogRider
/// TRUE / 4000 / 160; the 15.535 card data gives Prince, DarkPrince, the Battle Ram's
/// Ram and RoyalHog the identical block). Only a JumpEnabled row leaps the water:
/// its search prices water at WATER_COST instead of BLOCKED (path16402.rs
/// `cell_cost_for`), and when on a walk tick a popped waypoint leaves a WATER node
/// next, the remaining water nodes are replaced by one landing node and the unit
/// moves at JumpSpeed until it is within two JumpSpeeds of that node's centre
/// (jump16402.rs; calibration.json movement.JUMP_WATER_HOP, measured on the five
/// live hops). MegaKnight and Assassin carry JumpHeight / JumpSpeed WITHOUT
/// JumpEnabled (their attack jump, a different state) and get no block. The state is
/// entity.rs `jumping`, driven by state.rs `phase_path16402`.
///
/// `speed` is JumpSpeed RAW: native millitiles per tick, the same unit as the Speed
/// column (the Hog's 160 is the 113-per-axis diagonal leap the live client shows).
/// `height_raw` is JumpHeight, kept for the record only: it draws the visual arc,
/// which touches no position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JumpDef {
    /// JumpSpeed, native units per tick.
    pub speed: i32,
    /// JumpHeight, raw (the arc's height; not modelled).
    pub height_raw: i32,
}

/// THE UNDERGROUND SPAWN WALK (characters.csv SpawnPathfindSpeed / SpawnPathfindMorph; the
/// Miner 650, the Goblin Drill's dig 300). Measured on client 16.402 (capture 20260920-083112,
/// both seats) and on client 15.535.29: the unit is born at its owner's King, moves under ground
/// at SpawnPathfindSpeed native units per tick toward the cell that holds its destination, and
/// comes up within one step of it -- as itself (the Miner), or by leaving its MORPH target in its
/// place (the Drill's dig becomes the 1313-hp building). state.rs `phase_tunnel`, calibration
/// movement.SPAWN_PATHFIND_STATES and its sibling keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnPathfindDef {
    /// SpawnPathfindSpeed, NATIVE units per tick (no buff reads it, as the river leap's speed).
    pub speed: i32,
    /// SpawnPathfindMorph resolved to a CardDb index (a summon-only building); None = the
    /// tunneller itself comes up. u16::MAX until `CardDb::from_json_str` resolves it.
    pub morph: Option<u16>,
}

/// THE DASH (characters.csv DashDamage / DashMinRange / DashMaxRange / DashCooldown /
/// DashRadius / DashPushBack / DashImmuneToDamageTime, with JumpSpeed, DashConstantTime and
/// DashLandingTime; 15.535: the Bandit's Assassin row 152 / 3500 / 6000 / 800 / - / - / 100
/// with 500 / - / -, the Mega Knight 210 / 3500 / 5000 / 900 / 2200 / 1000 / - with 250 / 800
/// / 300). A unit walking after its target stands, then dashes at it and lands DashDamage.
/// Run under calibration combat.DASH_ATTACK = client_dash (state.rs `phase_path16402`,
/// entity.rs `dash_state`); inert under the shipped `none`.
///
/// Only a dash that starts on its own is this block: one with DashMaxRange. The Golden
/// Knight's chain and the event Hog Rider's dash (no DashMaxRange) start from an Ability or a
/// scripted action; the extractor carries those columns as `triggered_dash`, which nothing
/// reads. Distances are in SUBTILES like every other range here; `speed` is JumpSpeed RAW,
/// native units per tick, the unit of the Speed column; the times are ms.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DashDef {
    /// DashDamage, level 1, raw.
    pub damage: i32,
    /// DashMinRange: a target whose edge gap is under this when first seen is walked into.
    pub min_range: i32,
    /// DashMaxRange: the dash starts when the centre distance is within this + the target's radius.
    pub max_range: i32,
    /// DashCooldown, ms: the stand before the dash.
    pub cooldown_ms: i32,
    /// DashRadius: the landing blow's radius. None: the blow is on the target alone.
    pub radius: Option<i32>,
    /// DashPushBack, raw (not modelled: calibration combat.DASH_ATTACK's open list).
    pub pushback_raw: Option<i32>,
    /// DashImmuneToDamageTime, ms. None: the dash gives no immunity.
    pub immune_ms: Option<i32>,
    /// JumpSpeed, native units per tick.
    pub speed: i32,
    /// DashConstantTime, ms: the blow lands this long after the dash starts. None (the
    /// Bandit): the blow lands on the step that reaches its Range.
    pub constant_time_ms: Option<i32>,
    /// DashLandingTime, ms, raw (not modelled: the ledger's open list).
    pub landing_time_ms: Option<i32>,
}

/// THE REFLECT (characters.csv ReflectedAttackDamage / ReflectAttackCrownTowerDamage /
/// ReflectedAttackRadius / ReflectedAttackBuff / ReflectedAttackBuffDuration; cards.json
/// `reflected_attack`, which the extractor writes only on a row that sets them: in the 15.535
/// table the Electro Giant's and no other). A melee hit landed on the unit by an attacker whose
/// EDGE is within `radius` of the unit's centre is answered in the same tick by `damage` on the
/// attacker and `buff` on it (state.rs `reflect_melee_hit`, calibration combat.REFLECT_ATTACK =
/// client_reflect_stun, measured on client 15.535.29). Loaded under either arm; run under that
/// one only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReflectDef {
    /// ReflectedAttackDamage, the LEVEL-1 value like every other stat: scaled by the REFLECTING
    /// unit's level at the hit (75 is 192 at level 11).
    pub damage: i32,
    /// ReflectAttackCrownTowerDamage, level 1, as loaded. READ BY NOTHING: the engine answers a
    /// MELEE hit only, and a crown tower's attack is a projectile. Whether the client answers a
    /// tower's shot from inside the reach with this figure is the ledger key's open item.
    pub crown_tower_damage: Option<i32>,
    /// ReflectedAttackRadius, SUBTILES: the attacker's edge, not its centre, must be this close
    /// (the measured reflect fired on a Knight 2,008 centre to centre).
    pub radius: i32,
    /// ReflectedAttackBuff for ReflectedAttackBuffDuration ms (the Electro Giant's ZapFreeze for
    /// 500): the stun the answer carries. None on a row that sets neither column.
    pub buff: Option<BuffApply>,
}

/// THE COUNTER (the 15.535.29 tables' ActionCounter: the Ronin's; cards.json `parry`, calibration parry.*). While
/// the counter is ready, the first hit parry.COUNTERED_HITS admits that lands on the unit is taken at `taken_pct`
/// percent. The attacker gets `stun` from the next tick and takes `reflect_pct` percent of the countered hit later,
/// each at its place in the instigator group (`delays_ms`, read by calibration actions.SUB_ACTIONS_DELAY). The
/// counter is then spent for `cooldown_ms` (state.rs `parry_pass`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParryDef {
    /// Cooldown, ms. The self group's cooldown tag (ActionWithDuration) must say the same, or the row is refused.
    pub cooldown_ms: i32,
    /// DeployActive: the counter works while the unit deploys (read under parry.READY_AT = spawn).
    pub ready_at_deploy: bool,
    /// DefenseScalar, 0 to 100: the percent of the countered hit the unit still takes (0 on the Ronin).
    pub taken_pct: i32,
    /// DamageScalar: the reflect is this percent of the countered hit (200 on the Ronin). Not scaled by level: the
    /// damage type sets EnableLevelScaling false, and a row that scales it is refused.
    pub reflect_pct: i32,
    /// The self group's forced animation, ForcedDuration ms (500). Read only under parry.COOLDOWN_START =
    /// lock_end, which counts the cooldown from its end.
    pub self_lock_ms: i32,
    /// The instigator group's BuffType spawn, for its SpawnTime (the Ronin's speed -100, hit speed -95 and spawn
    /// speed -100 row for 500 ms).
    pub stun: BuffApply,
    /// The instigator group's SubActionsDelay, raw, in SubActions order (the Ronin's [50, 300, 150]); entries past
    /// `group_len` are 0.
    pub delays_ms: [i32; 4],
    /// How many sub-actions the instigator group has.
    pub group_len: u8,
    /// The stun's place and the reflect's place in that group (0 and 1 on the Ronin).
    pub stun_at: u8,
    pub reflect_at: u8,
}

/// SummonCharacterSecond: a second kind of unit the same card summons on the same
/// ring (Goblin Gang: 3 Goblins + 3 Spear Goblins; Rascals: the Boy + 2 Girls).
/// Loaded as a `summon_only` card like a spawner's unit; `unit` is its CardDb index.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct SecondSummonDef {
    pub unit: u16,
    /// SummonCharacterSecondCount (>= 1).
    pub count: i32,
}

/// The summon layout and stagger columns (formation.rs; calibration.json
/// formation.*). SUBTILES and ms; a 0 is the column's blank, which the layout
/// treats as 0 (an int column with no default).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct FormationDef {
    /// SummonRadius, subtiles (0 = blank: the unit's SpawnRadius, else its
    /// CollisionRadius -- state.rs `formation_members`).
    pub summon_radius: i32,
    /// SummonWidth, subtiles: nonzero selects the LINE layout (RoyalHogs 3500).
    pub summon_width: i32,
    /// SummonDeployDelay, ms: member k >= 1 of the primaries waits k x this before
    /// its own DeployTime starts (formation.rs `stagger_ms`).
    pub summon_deploy_delay_ms: i32,
    /// SummonDeployDelaySecond, ms: the j-th second-summon member waits (j + 1) x this
    /// when SummonDeployDelay is blank.
    pub summon_deploy_delay_second_ms: i32,
    /// The unit's own SpawnRadius, subtiles (0 = blank).
    pub spawn_radius: i32,
    /// The unit's SpawnAngleShift, degrees (0 = blank).
    pub spawn_angle_shift_deg: i32,
    /// SummonCharacterSecond + count; None on every card without one (and on a card
    /// whose unit list came through the overlay: `RawCard::summon_resolution`).
    pub second_summon: Option<SecondSummonDef>,
    /// The unit's SpawnMaxAngle, degrees (0 = blank): the arc its mount's riders spread over (the Goblin Giant's Spear
    /// Goblins, 90; calibration rider.OFFSET_LAW, formation.rs `rider_arc_offset`). Read on a rider row alone.
    #[serde(default)]
    pub spawn_max_angle_deg: i32,
}

/// THE DAMAGE RAMP (characters / buildings VariableDamage2, VariableDamage3,
/// VariableDamageTime1, VariableDamageTime2; cards.json `variable_damage`). A hit deals
/// Damage while the attack progress on the current target is below `time1_ms`,
/// `damage2` below `time1_ms + time2_ms`, and `damage3` from there on (calibration
/// combat.VARIABLE_DAMAGE, combat.rs `fire`). Measured on the 16.402 corpus on the Inferno
/// Tower (17 / 62 / 331) and the Inferno Dragon (14 / 47 / 165), switched at 2000 and 4000.
/// The damages are level-1 values scaled like Damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VariableDamageDef {
    pub damage2: i32,
    pub damage3: i32,
    pub time1_ms: i32,
    pub time2_ms: i32,
}

/// THE HOOK SPECIAL (characters.csv SpecialRange, SpecialMinRange, SpecialLoadTime and the
/// ProjectileSpecial row's Speed and DragMargin; cards.json `special`). The Fisherman:
/// 7000, 3500, 1300 ms, FishermanProjectile at 800 with DragMargin 200. Run under
/// calibration combat.SPECIAL_HOOK = client_hook_drag (state.rs `special_step`,
/// `step_hook_drags`). DragBackSpeed and DragSelfSpeed are not read: the drag's measured
/// step is not derived from them yet, and a building target is not hooked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpecialDef {
    /// SpecialRange, SUBTILES.
    pub range: i32,
    /// SpecialMinRange, SUBTILES (0 = blank: no minimum).
    pub min_range: i32,
    /// SpecialLoadTime, ms.
    pub load_time_ms: i32,
    /// The special projectile's Speed, raw (the unit of every projectile Speed column).
    pub projectile_speed: i32,
    /// The special projectile's DragMargin, SUBTILES.
    pub drag_margin: i32,
}

/// THE ELIXIR COLUMNS (the 15.535.29 tables: ManaCollectAmount, ManaGenerateTimeMs, ManaOnDeath,
/// ManaOnDeathForOpponent; cards.json `mana`), raw. What a unit of each is worth is calibration's:
/// a payout of ManaCollectAmount 1 is one elixir (measured on client 16.402), ManaOnDeath is whole
/// elixir, and ManaOnDeathForOpponent is read under economy.MANA_ON_DEATH_FOR_OPPONENT_UNIT (the
/// Elixir Golem's 1000 / 500 / 500 beside the Collector's 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManaDef {
    /// (ManaCollectAmount, ManaGenerateTimeMs): the payout and its period, both positive, on a
    /// building only. None when the row produces nothing.
    pub collect: Option<(i32, i32)>,
    /// ManaOnDeath, to the owner (0 = blank).
    pub on_death: i32,
    /// ManaOnDeathForOpponent, to the other side (0 = blank).
    pub on_death_for_opponent: i32,
}

/// One card, in engine units. Distances are SUBTILES; times are ms.
#[derive(Clone, Debug)]
pub struct CardDef {
    pub name: String,
    /// The name of the UNIT this card puts on the board: its SummonCharacter (cards.json
    /// `summon_character`), else its own name. A multi-unit card's members carry the card, so
    /// `name` is the card's (Goblins, Skeletons, Bats) where the game names the unit
    /// (Goblin_Stab, Skeleton, Bat); a summon-only unit is its own record and the two agree.
    /// What a MEASURED list keyed by unit matches (combat.POST_KILL_RETARGET_WAIT).
    pub unit_name: String,
    pub kind: CardKind,
    pub elixir: i32,
    pub rarity: String,
    pub hitpoints: i32,
    pub damage: i32,
    pub hit_speed_ms: i32,
    pub load_time_ms: i32,
    /// Raw Speed column (movement = speed * SPEED_TO_SUBTILES_PER_TICK).
    pub speed: i32,
    pub range: i32,
    pub sight_range: i32,
    pub collision_radius: i32,
    /// Absent for buildings; the push model treats absence explicitly.
    pub mass: Option<i32>,
    pub deploy_time_ms: i32,
    pub attacks_air: bool,
    pub attacks_ground: bool,
    pub target_only_buildings: bool,
    pub flying_height: i32,
    pub area_damage_radius: i32,
    pub projectile: Option<ProjectileDef>,
    pub count: i32,
    pub shield_hitpoints: i32,
    pub crown_tower_damage_percent: i32,
    pub death_damage: i32,
    pub death_damage_radius: i32,
    /// Splash centred on the attacker rather than the target (Valkyrie).
    pub self_as_aoe_center: bool,
    /// LifeTime, ms: the entity bleeds its hitpoints away over this long (calibration lifetime.HP_DECAY;
    /// state.rs `lifetime_of`, `phase_status`). A building's row carries it; so does a troop row that only a
    /// transformation reaches (the Goblin Demolisher's kamikaze form, lifetime.TROOP_LIFETIME). The loader
    /// refuses a troop with a LifeTime reached any other way.
    pub lifetime_ms: Option<i32>,
    /// cards.json level_scaling.multiplier_percent_by_level: entry L-1 is the
    /// percent of the level-1 stat at rarity-local level L. When absent the
    /// rarities.csv table is used.
    pub level_table: Option<Vec<i32>>,
    /// Crown towers only: the full (width, height) of the tower's no-deploy
    /// rectangle, SUBTILES, from buildings.csv NoDeploySizeW/H read as TILES (the
    /// reading under which all four arena landmarks are exact; tools/check_data.py
    /// gates them). Enemy troops may not be placed inside the closed rect while the
    /// tower is alive (arena.rs TROOP TERRITORY). None for every other card.
    pub no_deploy_size: Option<Vec2>,
    /// characters.csv IgnorePushback (2018 vintage: Giant, Prince, BabyDragon TRUE --
    /// BabyDragon lost it in 2018-08, docs/spell-spec.md). Overridden by PushbackAll.
    pub ignore_pushback: bool,
    /// Some for a spell card; every stat field above is then 0.
    pub spell: Option<SpellDef>,
    /// A unit that is not itself a card (the Goblin a Goblin Barrel releases), loaded
    /// from cards.json `units` because a simulable spell spawns it. Never playable,
    /// never in a hand; bindings report it under the card that released it.
    pub summon_only: bool,
    /// characters.csv StopMovementAfterMS / WaitMS: the "stomp" cards (Giant,
    /// Royal Giant, Golem, Ice Golem) walk for `stop_movement_after_ms` and then
    /// stand still for `wait_ms`, forever. 0 on everything else.
    ///
    /// THE SPEED COLUMN IS NOT THE SPEED FOR THESE CARDS. The measured 15.535.29
    /// per-tick displacement is the FASTER figure -- Giant 52 and Golem 54 while
    /// both ship Speed 45 -- so the engine reads Speed through
    /// calibration.json movement.STOMP_SPEED_RULE, never raw. `move_speed()` is
    /// the only correct way to turn these three numbers into a speed.
    pub stop_movement_after_ms: i32,
    pub wait_ms: i32,
    /// buildings.csv HidesWhenNotAttacking with its two timers (Tesla). None on
    /// every card that does not hide; a hiding card with either timer missing is
    /// refused at load, never defaulted.
    pub hide: Option<HideDef>,
    /// The Spawn* columns (periodic spawner). None on every card without a
    /// SpawnCharacter; a spawner block missing SpawnNumber or SpawnPauseTime is
    /// refused at load, never defaulted.
    pub spawner: Option<SpawnerDef>,
    /// The DeathSpawn* columns. None without a DeathSpawnCharacter; a block missing
    /// its count is refused at load.
    pub death_spawn: Option<DeathSpawnDef>,
    /// The charge block (ChargeRange / DamageSpecial / ChargeSpeedMultiplier). None
    /// on every card without one; a block missing any of the three is refused at
    /// load, never defaulted (a card that half-charges runs as a different card).
    pub charge: Option<ChargeDef>,
    /// The jump block (JumpEnabled with JumpHeight / JumpSpeed). None on every card
    /// without JumpEnabled; a JumpEnabled card missing either number is refused at
    /// load, never defaulted.
    pub jump: Option<JumpDef>,
    /// cards.json `level_scaling.base_level`: the UNIFIED level the record's base
    /// stats hold at, from which `level_table` is entered (module doc, LEVEL
    /// SCALING). None: the card's own rarity's local level 1 (RelativeLevel + 1),
    /// the 2018 convention.
    pub level_base: Option<i32>,
    /// THE SUMMON LAYOUT AND STAGGER INPUTS: the spells_characters Summon* columns
    /// and the unit's SpawnRadius / SpawnAngleShift, read by state.rs
    /// `formation_members` under calibration formation.LAYOUT / DEPLOY_STAGGER.
    /// Always present (zeros are the columns' blanks); `count` above stays the
    /// primaries' SummonNumber.
    pub formation: FormationDef,
    /// ProjectileStartRadius, SUBTILES (0 = blank): a projectile is born this far
    /// from the attacker's centre toward the target and takes its first step the
    /// next tick (calibration combat.PROJECTILE_LAUNCH; the tower arrows 300, the
    /// Musketeer's 450). Read by combat.rs `fire`.
    pub projectile_start_radius: i32,
    /// Kamikaze: the unit dies on its own hit -- gone on the tick a melee hit lands
    /// or its projectile launches (calibration combat.KAMIKAZE_DEATH). Battle Ram,
    /// the Spirits, Wall Breakers. FALSE when KamikazeTime > 0 (the Skeleton Barrel's
    /// delayed death): that one is `kamikaze_time_ms`, run under combat.KAMIKAZE_TIME.
    pub kamikaze: bool,
    /// THE BUFF THIS CARD'S ATTACK APPLIES:
    /// projectiles.TargetBuff + BuffTime for a unit that shoots (the Ice Spirit's
    /// Freeze 1100 ms, the Ice Wizard's IceWizardSlowDown 2500 ms), or
    /// characters.BuffOnDamage + BuffTime for one that does not (the Electro
    /// Wizard's ZapFreeze 500 ms). `buff` indexes `CardDb::buffs`. Applied by
    /// combat.rs `fire` to everything the hit lands on, under calibration
    /// status.TARGET_BUFF_ON_SPLASH.
    pub attack_buff: Option<BuffApply>,
    /// The card's projectile is HOMING (projectiles.csv Homing; every 16.402 troop and tower
    /// row sets it: 32 true, 15 false -- Bomber, Princess, Bowler, Hunter, ...). The engine flies
    /// every projectile to its target, so this is read only where the game's difference shows:
    /// a victim's pending damage (combat.POST_KILL_RETARGET_WAIT = client16402_attack_finish).
    /// On the card, not on `ProjectileDef`, whose Debug is inside the format-3 card fingerprint.
    /// A blank reads as homing; false on a card with no projectile.
    pub projectile_homing: bool,
    /// THE AREA EFFECT THIS CARD'S DEATH LEAVES ON THE GROUND (characters /
    /// buildings DeathAreaEffect, resolved against cards.json `area_effect_objects`
    /// by `CardDb::from_json_str`). The Ice Golem's FreezeIceGolemite: a 2000
    /// millitile disc that hangs IceWizardSlowDown -- 30 % off speed, hit speed and
    /// spawn speed -- on every enemy inside it for 2000 ms, and carries no damage of
    /// its own.
    ///
    /// THE ROW'S NAME CONTRADICTS ITS BEHAVIOUR, so read the row and not the name.
    /// `FreezeIceGolemite` is a SLOW: its buff is IceWizardSlowDown at -30, which
    /// leaves the victim walking at 70 % of its speed. A freeze is -100, which is
    /// what `SuperIceGolemite`'s `SuperFreezeIceGolemite` actually ships. The
    /// recordings agree with the row rather than the name: after an Ice Golem death
    /// a Goblin that walked a straight line stepped 83-84 native units per tick
    /// against its Speed column of 120 (client 16.402). Anything reading this field
    /// by name, in this repo or in a consumer, will get the wrong mechanic.
    ///
    /// INDEPENDENT OF `death_damage` / `death_damage_radius`, which the same death
    /// also fires. The data settles that they are two effects of one death, not one:
    /// the Ice Golem's area carries a blank Damage beside a 33-damage disc, and the
    /// Super Ice Golem ships a 2000 disc of 40 next to an area of radius 40000
    /// dealing 72 at 30 % to crown towers -- two radii, two damage columns, two crown
    /// percents, one death. state.rs `phase_reap` releases both.
    ///
    /// A whole `SpellDef` so the release runs the engine's own area-effect path
    /// (spell.rs `cast`, then `step_spells`) rather than a second one. Its
    /// `placement` is never read: a death is not a cast and has no tap to validate.
    /// None on every card without the column; a card whose area the loader refuses
    /// is rejected WHOLE, never run as a card whose death does nothing.
    pub death_area_effect: Option<SpellDef>,
    /// characters / buildings DeathSpawnPushback (cards.json `death_spawn_pushback`, 15.535
    /// only; absent, as in the 2018 file, reads false): this card's DEATH SPAWN is born on a
    /// small fixed ring and slides out to DeathSpawnRadius, instead of being laid at the
    /// radius by spawner.DEATH_SPAWN_LAYOUT. True on the Golem and the Lava Hound among the
    /// loaded cards; blank on the Battle Ram, whose Barbarians the facing ring places.
    /// Acted on only under spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide (state.rs
    /// `death_spawn_points` / `fixed_slide_ring` and the slide in the Path phase); inert without a
    /// `death_spawn` and under the shipped `not_read`.
    pub death_spawn_pushback: bool,
    /// The dash block (card.rs `DashDef`): a unit that stands, then dashes into its target.
    /// None on every card without one, and on the 2018 file's Bandit and Mega Knight, whose
    /// block carries no JumpSpeed (the extractor writes the dash's motion on the 15.535 rows
    /// only). Acted on only under combat.DASH_ATTACK = client_dash.
    pub dash: Option<DashDef>,
    /// THE REFLECT this card's unit answers a melee hit with (`ReflectDef`; the Electro Giant's).
    /// None on every card whose row sets none of the ReflectedAttack columns. Declared in the
    /// post-format-3 tail for the reason below: migrate_v3 strips it with the rest of the tail.
    pub reflect: Option<ReflectDef>,
    /// The row's straight-to-range projectile (`RangeShotDef`): its projectile carries a
    /// ProjectileRange and a ProjectileRadius. None on every other card.
    pub range_shot: Option<RangeShotDef>,
    /// MultipleProjectiles (cards.json `units.<unit>.raw`): the projectiles one shot fires
    /// under calibration combat.MULTIPLE_PROJECTILES = client_fan (the Hunter 10, the
    /// Princess 5). 1 when blank.
    pub multiple_projectiles: i32,
    /// CustomFirstProjectile when it is a different row from the Projectile column
    /// (`CustomShotDef`: the Princess). None on every other card.
    pub custom_first_projectile: Option<CustomShotDef>,
    /// MultipleTargets (cards.json `units.<unit>.raw`): the bolts one attack delivers under
    /// calibration combat.MULTIPLE_TARGETS = client_bolts_per_target (the Electro Wizard 2).
    /// 1 when blank.
    pub multiple_targets: i32,
    /// AllTargetsHit (cards.json `units.<unit>.raw`): a bolt with no other enemy to go to
    /// lands on the target.
    pub all_targets_hit: bool,
    /// THE DEPLOY PROJECTILE (cards.json `deploy_projectile`, a troop card's
    /// spells_characters Projectile: the Mega Knight's MegaKnightAppear), as the impact it
    /// lands as: a `SpellDef` whose `SpellShape::Projectile` carries the hit, so the blow is
    /// an ordinary `Spell` running spell.rs `impact` (its damage scaled on this card's
    /// ladder, its radius, its air/ground filter, its knockback) through spell.rs
    /// `shape_of`, as a death bomb is. Fired by state.rs `spawn_now` under calibration
    /// combat.DEPLOY_PROJECTILE = client_on_landing (shipped); inert under the old not_read. A
    /// card with both this and a death area effect is refused (`shape_of` could name only
    /// one of them).
    pub deploy_projectile: Option<SpellDef>,
    /// characters.csv LoadFirstHit (cards.json `load_first_hit`; the Sparky): the unit leaves
    /// its deploy with its load timer at LoadTime, so its first attack is timed from the
    /// deploy end rather than from the lock. Acted on only under combat.LOAD_FIRST_HIT =
    /// load_time_from_deploy_end (state.rs `load_first_hit_on_deployed`); false on a blank.
    pub load_first_hit: bool,
    /// The damage ramp (`VariableDamageDef`); None on every card without VariableDamage2.
    /// Acted on only under combat.VARIABLE_DAMAGE = client16402_attack_progress_stages.
    pub variable_damage: Option<VariableDamageDef>,
    /// characters.csv AttackPushBack (cards.json `attack_pushback_milli`), SUBTILES; 0 on a
    /// blank. The recoil of each launch (the Sparky 750, the Firecracker 1000), acted on only
    /// under knockback.ATTACK_PUSHBACK = ladder_away_from_target (state.rs `attack_recoil`).
    pub attack_pushback: i32,
    /// The hook special (`SpecialDef`); None on every card without SpecialRange. Acted on
    /// only under combat.SPECIAL_HOOK = client_hook_drag.
    pub special: Option<SpecialDef>,
    /// THE PROJECTILE THIS CARD'S DEATH RELEASES (characters DeathSpawnProjectile, cards.json
    /// `death_spawn_projectile`, resolved against the file's `projectiles` table by
    /// `CardDb::from_json_str`): the Phoenix's PhoenixFireball, Damage 64 in a Radius of 2500 and
    /// a SpawnCharacter, the PhoenixEgg. A `SpellShape::Projectile` so its arrival is a cast
    /// projectile's (spell.rs `step_spells`: the impact, then the release); state.rs `phase_reap`
    /// stands it on the death point under spawner.DEATH_SPAWN_PROJECTILE = client_projectile and
    /// leaves it unread under `none`. Its release is `UnitRef::DeathProjectile`. None on every card
    /// without the column; a card whose projectile the loader refuses is rejected WHOLE.
    pub death_projectile: Option<SpellDef>,
    /// THE AREA EFFECT THIS CARD IS (spells_characters AreaEffectObject, when that area's one
    /// action spawns the card's own character; cards.json `deploy_area_effect`, resolved against
    /// `area_effect_objects`): the Electro Wizard's ElectroWizardZap, the Ice Wizard's
    /// IceWizardCold. Cast where the character appears on a deploy under spells.DEPLOY_AREA_EFFECT
    /// = client_area_effect (state.rs `phase_spawn`), under the card's own index and level. Taken
    /// on a one-member card alone. None on every card without the column; a card whose area the
    /// loader refuses is rejected WHOLE.
    pub deploy_area_effect: Option<SpellDef>,
    /// THE AREA EFFECT THIS UNIT PUTS DOWN WHERE IT APPEARS (characters SpawnAreaObject, cards.json
    /// `spawn_area_object`, resolved against `area_effect_objects`): the Battle Healer's
    /// BattleHealerSpawnHeal, a one-shot 3000 area hanging a heal buff on her own troops. Cast at
    /// creation under spawner.SPAWN_AREA_OBJECT_SCOPE = every_row (state.rs `spawn_now`). The one
    /// place an own-troop area is read. None on every card without the column; a card whose area
    /// the loader refuses is rejected WHOLE.
    ///
    /// ONE SPELL OBJECT NAMES ONE SHAPE (spell.rs `shape_of`): a card carries at most one of
    /// `spell`, `death_area_effect`, `deploy_projectile`, `death_projectile`, `deploy_area_effect`
    /// and this, and one that would carry two is refused (`CardDb::from_json_str`).
    pub spawn_area_effect: Option<SpellDef>,
    /// characters.csv Hovering (cards.json `hovering`): the troop hovers over the ground. True on
    /// two troop rows that load in the client, the Battle Healer and the Royal Ghost. Acted on only
    /// under pathfinding.HOVERING_WATER_RULE = priced_water_no_hop (state.rs `prices_water`), where
    /// the troop prices water as a JumpEnabled mover does and walks it; a blank, or a file that
    /// does not carry the column, reads false.
    pub hovering: bool,
    /// characters / buildings MinimumRange, SUBTILES (cards.json `minimum_range_milli`; 0 = blank): an attacker
    /// neither keeps nor takes a target whose edge distance is below it (the Mortar's 3500). Acted on only under
    /// calibration targeting.MINIMUM_RANGE = client16402_edge_distance (target.rs `inside_minimum_range`); inert
    /// under the shipped `not_read`.
    pub minimum_range: i32,
    /// THE SPARKS this card's shot releases where it lands (`SparkDef`: the Firecracker's
    /// FirecrackerExplosion), read under calibration combat.SPAWN_PROJECTILE =
    /// client_spark_fan. None on every other card, and on the Firecracker too until cards.json
    /// carries the row's SpawnCount and Scatter.
    pub spark: Option<SparkDef>,
    /// THE AREA THIS CARD'S PROJECTILE LEAVES WHERE IT LANDS (projectiles SpawnAreaEffectObject: the
    /// Heal Spirit's heal). A whole SpellDef, cast at the impact point under this card's index and the
    /// firer's level; it first acts on the next tick like every object born in the Projectile phase
    /// (spell.rs `SpellOut::areas`). None on every card whose projectile leaves none.
    pub projectile_area: Option<SpellDef>,
    /// THE LIFE-STATE CONTROLLER (the Goblin Hut; `LifeStateDef`, state.rs `life_state_pass`). None on every other
    /// card.
    pub life_state: Option<LifeStateDef>,
    /// INVISIBLE WHEN IDLE (the Royal Ghost, the Suspicious Bush; targeting.INVISIBILITY): `Some(idle ms)` when the
    /// row's BuffWhenNotAttacking is an invisibility, None on every other card. Read by target.rs `can_target` through
    /// the entity's `reveal_from`. `Some(0)` on a kamikaze whose row leaves the time blank (the Bush): its first hit is
    /// its death, so it is invisible for its whole life.
    pub invisible_when_idle: Option<i32>,
    /// THE UNDERGROUND SPAWN WALK (`SpawnPathfindDef`; state.rs `phase_tunnel`): a played card
    /// whose unit is born at its owner's King and travels under ground to the destination the
    /// play resolved (the Miner; the Goblin Drill, whose dig leaves its building there). None on
    /// every other card, and on every spawned unit: a units row that tunnels is refused.
    pub spawn_pathfind: Option<SpawnPathfindDef>,
    /// spells_characters / spells_buildings CanDeployOnEnemySide on a card that tunnels (cards.json
    /// `can_deploy_on_enemy_side`, the card row's flag): its play goes anywhere on land
    /// (placement.SPAWN_PATHFIND_TERRITORY, state.rs `deploy_rule`). The loader refuses a
    /// tunnelling card without it, so this is true on every card with `spawn_pathfind` and false
    /// on every other (a spell's own flag stays in its `SpellPlacement`).
    pub can_deploy_on_enemy_side: bool,
    /// THE ELIXIR COLUMNS (`ManaDef`: the Elixir Collector's payout and its elixir on death, the
    /// Elixir Golem's elixir for the opponent; calibration economy.*, state.rs `mana_pass` and
    /// `phase_reap`). None on every card whose row sets none of them, and on every 2018 row (the
    /// extractor writes the block on the 15.535 rows alone).
    pub mana: Option<ManaDef>,
    /// OmitFromStartingHand (the Elixir Collector, Mirror): the deal keeps the card out of the
    /// starting hand under economy.OMIT_FROM_STARTING_HAND (state.rs `try_new`). False on a blank.
    pub omit_from_starting_hand: bool,
    /// THE RIDERS this card's unit carries (`AttachDef`; the Ram Rider's Ram). None on every
    /// other card. The Spawn* block that sets SpawnAttach is read as this and never as a
    /// periodic spawner, so `spawner` is None beside it.
    pub attach: Option<AttachDef>,
    /// characters.csv TargetOnlyTroops (cards.json `target_only_troops`; the Ram Rider's rider):
    /// the unit never targets a building or a crown tower (target.rs `can_target`). False on a
    /// blank.
    pub target_only_troops: bool,
    /// characters.csv IgnoreTargetsWithBuff read with DeprioritizeTargetsWithBuff (the Ram
    /// Rider's rider: BolaSnare): the buff, a `CardDb::buffs` index, whose carriers this unit
    /// ranks after every other candidate and whose landing on its target clears that target
    /// (calibration targeting.DEPRIORITIZED_TARGET_BUFF; target.rs `scan`, state.rs
    /// `apply_effects`). None on every other card.
    pub deprioritize_buff: Option<u16>,
    /// THE DEPLOY'S MEMBERS AT EXPLICIT OFFSETS (`SummonMemberDef`; the Three Musketeers), one per member in list
    /// order, `count` of them. state.rs `formation_members` lays them at their offsets, each with its own unit, in
    /// place of the ring. None on every other card.
    pub summon_members: Option<Vec<SummonMemberDef>>,
    /// spells_characters CharactersOffsetsXMirrored, beside `summon_members`: read under
    /// formation.EXPLICIT_OFFSETS_FRAME's arm that mirrors x on a lane. False on every other card.
    pub summon_offsets_x_mirrored: bool,
    /// THE ATTACK SELECTOR (`AttackSelectDef`; the Three Musketeers). None on every other card.
    pub attack_select: Option<AttackSelectDef>,
    /// THE BUFFS THAT NEVER LAND ON THIS UNIT (characters / buildings IgnoreBuff): `CardDb::buffs` indices, sorted.
    /// The VoodooHog lists VoodooCurse and GoblinCurse, so a hog is never cursed into a second hog; the Golem, the
    /// Lava Hound and the Battle Ram list them too. A listed name no loaded buff carries is dropped. Read by state.rs
    /// `land_buff`. Empty on every other card.
    pub ignore_buffs: Vec<u16>,
    /// projectiles ApplyBuffBeforeDamage (the Mother Witch's VoodooProjectile): this card's attack buff lands before
    /// its damage, so a unit the hit kills can still carry it (status.APPLY_BUFF_BEFORE_DAMAGE, state.rs
    /// `apply_effects`). False on every other card.
    pub attack_buff_first: bool,
    /// THE ENCHANT (the Rune Giant; `EnchantDef`, state.rs `enchant_pass`, combat.rs `enchant_bonus`). None on every
    /// other card.
    pub enchant: Option<EnchantDef>,
    /// THE HEALTH-THRESHOLD TRANSFORMATION (the Cannon Cart, the Goblin Demolisher; `TransformDef`, state.rs
    /// `health_triggers` and `rebind_unit`): at its threshold the unit becomes another row in place, the same
    /// entity. None on every other card.
    pub transform_at_hp: Option<TransformDef>,
    /// THE COUNTER (the Ronin; `ParryDef`, state.rs `note_parry` and `parry_pass`): the first hit parry.COUNTERED_HITS
    /// admits while it is ready is taken at its DefenseScalar and answered with a stun and a reflect. None on every
    /// other card.
    pub parry: Option<ParryDef>,
    /// characters.csv KamikazeTime on a Kamikaze row (the Skeleton Barrel's 500), ms: the unit's
    /// death is DELAYED, drained from its first fire (state.rs `kamikaze_drain`) under
    /// combat.KAMIKAZE_TIME; `kamikaze` is false beside it. 0 on every other card, and on a row
    /// whose Kamikaze is not set.
    pub kamikaze_time_ms: i32,
    /// characters / buildings DeathPushBack (cards.json `death_pushback_milli`, 15.535 rows only),
    /// SUBTILES; 0 on a blank. The push a DEATH BOMB's hit gives the units it lands on, radially
    /// from the bomb, under knockback.DEATH_PUSHBACK (spell.rs `step_spells`: the Skeleton Barrel's
    /// container 1000). Loaded on every row: the Golem's 1800 and the Giant Skeleton's bomb's 1800
    /// stay unread under the shipped arm.
    pub death_pushback: i32,
    /// characters / buildings IgnoreClone (cards.json `ignore_clone`; the Goblin Drill's dig, the chess Recruits): the
    /// Clone spell never copies this unit (spell.rs `step_spells`). False on a blank.
    pub ignore_clone: bool,
    // ^ THE POST-FORMAT-3 TAIL IS DECLARED LAST ON PURPOSE (in declared order; new fields
    // append here in landing order). state.rs `migrate_v3` rebuilds the FORMAT-3 card
    // fingerprint by stripping the fields added after format 3 off the END of this
    // struct's Debug text, so a new field anywhere but after the last one, or a changed
    // value in a field format 3 also printed, puts that rebuild permanently out of reach
    // of a format-3 snapshot's saved hash. A new
    // field goes HERE, after `ignore_clone`, and onto the end of that tail
    // string. The in-repo fixture that used to prove the rebuild was retired on
    // 2026-09-21 for exactly that (tests/stacked_tie.rs says what went with it); the
    // discipline is kept for any format-3 snapshot a caller still holds, and nothing in
    // the suite would now catch breaking it.
}

impl CardDef {
    #[inline]
    pub fn is_flying(&self) -> bool {
        self.flying_height > 0
    }

    /// The forms of a VARIANT card (`SpellShape::Variant`, the Spirit Empress), None on every other card.
    pub fn variant(&self) -> Option<&[VariantOption]> {
        match self.spell.as_ref().map(|s| &s.shape) {
            Some(SpellShape::Variant { options }) => Some(options),
            _ => None,
        }
    }

    /// Is this the MIRROR (`SpellShape::Mirror`)?
    #[inline]
    pub fn is_mirror(&self) -> bool {
        matches!(self.spell.as_ref().map(|s| &s.shape), Some(SpellShape::Mirror))
    }

    /// EVERY BUFF THIS CARD CAN HANG, as `CardDb::buffs` indices, each once: its attack buff, its reflect's, its
    /// counter's stun, then each buff (`buff`, then `buff2`) along the chain of every spell object it carries -- its
    /// spell, death area, deploy blow, death projectile, deploy area, spawn area and projectile area, in that order.
    /// The one enumeration the loader's buff death-spawn needs and `CardDb::unit_refs` read.
    pub fn hung_buffs(&self) -> Vec<u16> {
        let mut out: Vec<u16> = Vec::new();
        for b in [self.attack_buff, self.reflect.and_then(|r| r.buff), self.parry.map(|p| p.stun)].into_iter().flatten() {
            if !out.contains(&b.buff) {
                out.push(b.buff);
            }
        }
        let blocks = [
            &self.spell,
            &self.death_area_effect,
            &self.deploy_projectile,
            &self.death_projectile,
            &self.deploy_area_effect,
            &self.spawn_area_effect,
            &self.projectile_area,
        ];
        for d in blocks.into_iter().flatten() {
            d.shape.buffs_into(&mut out);
        }
        out
    }

    /// THE FUSE OF A DEATH BOMB, ms, or None on every other card.
    ///
    /// `convert_death_bomb` is the only thing in this file that builds a card with
    /// BOTH `summon_only` and a `spell`, and the pair cannot arise any other way: a
    /// playable spell card is never `summon_only` (the loader refuses a spawned unit
    /// whose name is a card's), and a spawned unit that is a real unit never carries
    /// a spell (`convert` writes `spell: None` on every non-spell row). So the pair
    /// IS the bomb, and this is its whole interface: state.rs `phase_reap` asks the
    /// death-spawn unit for a fuse and, when it gets one, leaves a timed impact
    /// where the parent died instead of trying to spawn an entity with no
    /// hitpoints.
    #[inline]
    pub fn death_bomb_fuse_ms(&self) -> Option<i32> {
        if self.summon_only && self.spell.is_some() {
            Some(self.deploy_time_ms)
        } else {
            None
        }
    }

    /// The Speed the locomotion law uses, in raw Speed units
    /// (calibration.json movement.STOMP_SPEED_RULE, measured):
    ///
    /// ```text
    /// S = Speed                                          no stomp
    /// S = floor(Speed * (Stop + Wait) / Stop)            stomp cards
    /// ```
    ///
    /// A stomp card covers the SAME ground per (Stop + Wait) ms as a unit walking
    /// at this S would, because it stands still for Wait of them -- the pause
    /// schedule is calibration movement.STOMP_PAUSE_SCHEDULE and lives in the
    /// per-tick loop, not here.
    ///
    /// SO THE TWO MUST BE USED TOGETHER. Only `PathModel::Oracle2026` runs the pause
    /// schedule, so only it may spawn a unit at this speed; state.rs `spawn_now`
    /// gates on the path model for exactly that reason. Handing the raised speed to
    /// a model that never pauses is a silent 15 % speed-up of every stomp card.
    ///
    /// THE ROUNDING IS FLOOR, and it is measured (LIVE 16.402, calibration
    /// movement.STOMP_SPEED_RULE). The Giant and Golem could not separate it --
    /// 45 * 740/640 and 45 * 1200/1000 are whole numbers, so floor, round and
    /// truncate all give 52 and 54 -- but the ICE GOLEM does: Speed 45, Stop 470,
    /// Wait 80 gives 45 * 550/470 = 52.6596, and over 2373 isolated free-walk moving
    /// ticks S = 52 scores 2231 against 29 for S = 53 (51 scores 104, 54 scores 0).
    /// Floor. Every operand here is non-negative, so Rust's truncating `/` IS floor.
    ///
    /// WalkingSpeedTweakPercentage is NOT applied -- it is animation only (the Golem
    /// carries both a 15 % tweak and a stop/wait, and 45 * 1.15 = 51.75, not 54).
    ///
    /// A BUFF MULTIPLIES THIS S, not the raw Speed column, and also floors
    /// (calibration movement.BUFF_SPEED_RULE): a raged Ice Golem walks at exactly
    /// floor(52 * 130/100) = 67, where round and ceil give 68. Not implemented here
    /// -- nothing in royalesim applies a SpeedMultiplier yet -- and the ORDER is
    /// still open, because floor(floor(45 * 1.3) * 550/470) is also 67.
    #[inline]
    pub fn move_speed(&self) -> i32 {
        if self.stop_movement_after_ms > 0 {
            let period = self.stop_movement_after_ms as i64 + self.wait_ms as i64;
            ((self.speed as i64) * period / (self.stop_movement_after_ms as i64)) as i32
        } else {
            self.speed
        }
    }
}

#[derive(Deserialize)]
struct RawProjectileObj {
    speed: Option<i32>,
    damage: Option<i32>,
    #[serde(alias = "radius")]
    radius_milli: Option<i32>,
    /// TargetBuff + BuffTime: the buff this troop's projectile hangs on what it hits
    /// (IceSpiritsProjectile -> Freeze 1100 ms, ice_wizardProjectile ->
    /// IceWizardSlowDown 2500 ms). Loaded onto `CardDef::attack_buff`.
    target_buff: Option<RawBuff>,
    buff_time_ms: Option<i32>,
    /// projectiles.csv Homing (`CardDef::projectile_homing`).
    homing: Option<bool>,
    /// The row's name: compared with a CustomFirstProjectile (`CardDef::custom_first_projectile`).
    name: Option<String>,
    /// projectiles.csv SpawnProjectile, the whole row: what the shot releases where it lands
    /// (`CardDef::spark`, `spark_of`). Kept as a value and read as a `RawSpellProjectile`.
    spawn_projectile: Option<serde_json::Value>,
    // --- the straight-to-range columns (`RangeShotDef`).
    projectile_range_milli: Option<i32>,
    projectile_radius_milli: Option<i32>,
    aoe_to_air: Option<bool>,
    aoe_to_ground: Option<bool>,
    only_enemies: Option<bool>,
    pushback_milli: Option<i32>,
    pushback_all: Option<bool>,
    /// projectiles.csv PingpongVisualTime, ms. Not written by tools/extract_cards.py
    /// today, so absent on every row (`RangeShotDef::pingpong_ms`).
    pingpong_visual_time_ms: Option<i32>,
    /// projectiles.csv CheckCollisions, ProjectileStartExtraRadius and RandomDelay
    /// (`RangeShotDef::check_collisions`, `start_extra`, `random_delay_ms`). Written by
    /// tools/extract_cards.py on the 15.535 rows; absent reads false / 0.
    check_collisions: Option<bool>,
    projectile_start_extra_radius_milli: Option<i32>,
    random_delay_ms: Option<i32>,
    /// projectiles.csv SpawnAreaEffectObject: the NAME of the area the projectile leaves where it lands
    /// (`CardDef::projectile_area`; the Heal Spirit's heal, the SuperArcher's charge pull).
    spawn_area_effect_object: Option<String>,
    /// projectiles ApplyBuffBeforeDamage (`CardDef::attack_buff_first`; the Mother Witch's VoodooProjectile). Written
    /// on the 15.535 rows only; absent reads false.
    apply_buff_before_damage: Option<bool>,
}

#[derive(Deserialize)]
struct RawCard {
    name: String,
    display_name: Option<String>,
    kind: CardKind,
    elixir: Option<i32>,
    rarity: Option<String>,
    hitpoints: Option<i32>,
    damage: Option<i32>,
    hit_speed_ms: Option<i32>,
    load_time_ms: Option<i32>,
    speed: Option<i32>,
    stop_movement_after_ms: Option<i32>,
    wait_ms: Option<i32>,
    range_milli: Option<i32>,
    sight_range_milli: Option<i32>,
    collision_radius_milli: Option<i32>,
    mass: Option<i32>,
    deploy_time_ms: Option<i32>,
    attacks_air: Option<bool>,
    attacks_ground: Option<bool>,
    target_only_buildings: Option<bool>,
    flying_height: Option<i32>,
    area_damage_radius_milli: Option<i32>,
    projectile: Option<serde_json::Value>,
    count: Option<i32>,
    shield_hitpoints: Option<i32>,
    crown_tower_damage_percent: Option<i32>,
    death_damage: Option<i32>,
    death_damage_radius_milli: Option<i32>,
    self_as_aoe_center: Option<bool>,
    lifetime_ms: Option<i32>,
    level_scaling: Option<serde_json::Value>,
    /// [W, H] in tiles (towers array only).
    no_deploy_size_tiles: Option<[i32; 2]>,
    ignore_pushback: Option<bool>,
    /// Spells only: cards.json `spell` block.
    spell: Option<RawSpell>,
    /// buildings.csv HidesWhenNotAttacking / HideTimeMs / UpTimeMs (Tesla).
    hides_when_not_attacking: Option<bool>,
    hide_time_ms: Option<i32>,
    up_time_ms: Option<i32>,
    /// cards.json `spawner` block (Spawn* columns); null on most cards.
    spawner: Option<RawSpawner>,
    /// cards.json `death_spawn` block (DeathSpawn* columns); null on most cards.
    death_spawn: Option<RawDeathSpawn>,
    /// cards.json `charge` block (ChargeRange / DamageSpecial / ChargeSpeedMultiplier);
    /// null on every card but Prince, DarkPrince and BattleRam in the 2018 data.
    charge: Option<RawCharge>,
    /// cards.json `jump` block (JumpEnabled rows: JumpHeight / JumpSpeed); null on
    /// every card but HogRider in the 2018 data.
    jump: Option<RawJump>,
    /// cards.json `dash` block (the Dash* columns with JumpSpeed; a row with DashMaxRange
    /// only); null on every card but the Bandit and the Mega Knight among the loaded cards.
    dash: Option<RawDash>,
    /// cards.json `action_graph` (15.535: the scripted actions the row's *Action
    /// columns reach); absent in the 2018 file, null on a row that names none.
    action_graph: Option<RawActionGraph>,
    /// The Goblin Hut's controller (`LifeStateDef`), 15.535 only.
    life_state_spawner: Option<RawLifeState>,
    /// The health-threshold transformation (`TransformDef`), 15.535 only.
    transform_at_hp: Option<RawTransform>,
    /// cards.json `parry`: the counter (`ParryDef`), 15.535 only.
    parry: Option<RawParry>,
    /// cards.json `ignore_clone` (IgnoreClone; 15.535 only, written where set): `CardDef::ignore_clone`.
    ignore_clone: Option<bool>,
    /// cards.json `idle_invisibility` (15.535 only): the row's BuffWhenNotAttacking is an invisibility.
    idle_invisibility: Option<RawIdleInvisibility>,
    /// cards.json `interval_spawner` (15.535 only): the Furnace's ActionInterval -> ActionSpawnToLocation
    /// (`interval_spawner_of`).
    interval_spawner: Option<RawIntervalSpawner>,
    /// cards.json `mana` (15.535 only; the four Mana columns): `CardDef::mana` (`convert_mana`).
    mana: Option<RawMana>,
    /// cards.json `omit_from_starting_hand` (15.535 only, written where set): `CardDef::omit_from_starting_hand`.
    omit_from_starting_hand: Option<bool>,
    /// cards.json `card_table_kind` (15.535 only): the table a card is listed in, written beside `kind` where the
    /// row it puts on the board is of the other kind (the Furnace: a spells_buildings card whose unit is a
    /// troop). `kind` is what the engine runs; this one is checked, never run (`convert`).
    card_table_kind: Option<CardKind>,
    /// cards.json `target_only_troops` (TargetOnlyTroops; 15.535 only, written where set):
    /// `CardDef::target_only_troops`.
    target_only_troops: Option<bool>,
    /// cards.json `ignore_targets_with_buff` (the IgnoreTargetsWithBuff row) and
    /// `deprioritize_targets_with_buff` (15.535 only, each written where set):
    /// `CardDef::deprioritize_buff`.
    ignore_targets_with_buff: Option<RawBuff>,
    deprioritize_targets_with_buff: Option<bool>,
    /// The Rune Giant's enchant (`EnchantDef`), 15.535 only.
    enchant_friends: Option<RawEnchantFriends>,
    /// cards.json `death_area_effect`: the NAME of the area_effect_objects row the
    /// death leaves on the ground (the Ice Golem's FreezeIceGolemite, the Rage
    /// Barbarian's bottle dummy). A NAME, and nothing more: FreezeIceGolemite is a
    /// 30 % slow, not a freeze (see `CardDef::death_area_effect`). `from_json_str` looks it up in the file's
    /// `area_effect_objects` table and fills `CardDef::death_area_effect`; a name the
    /// table does not carry, or an area whose mechanic the loader does not read,
    /// refuses the card AFTER its push, which keeps the format-3 card list intact.
    death_area_effect: Option<String>,
    /// cards.json `death_spawn_pushback` (characters / buildings DeathSpawnPushback; written
    /// on the 15.535 rows only, so absent in the 2018 file): `CardDef::death_spawn_pushback`.
    death_spawn_pushback: Option<bool>,
    /// cards.json `hovering` (characters.csv Hovering): `CardDef::hovering`. Not written by the
    /// extractor yet, so absent everywhere today.
    hovering: Option<bool>,
    /// cards.json `minimum_range_milli` (characters / buildings MinimumRange, millitiles; null on most rows):
    /// `CardDef::minimum_range`.
    minimum_range_milli: Option<i32>,
    /// spells_characters SummonCharacter: the unit the card deploys (`CardDef::unit_name`).
    summon_character: Option<String>,
    // --- the summon layout and stagger (`FormationDef`). Every one null in the
    // 2018 file and on most 15.535 rows; a blank stays a blank.
    /// spells_characters SummonRadius, millitiles: the ring's radius input.
    summon_radius_milli: Option<i32>,
    /// spells_characters SummonWidth, millitiles: a LINE layout when set (RoyalHogs).
    summon_width_milli: Option<i32>,
    /// spells_characters SummonDeployDelay, ms: the per-member stagger.
    summon_deploy_delay_ms: Option<i32>,
    /// spells_characters SummonDeployDelaySecond, ms: the second summon's stagger.
    summon_deploy_delay_second_ms: Option<i32>,
    /// cards.json `second_summon` (SummonCharacterSecond / SummonCharacterSecondCount).
    second_summon: Option<RawSecondSummon>,
    /// cards.json `summon_resolution`: present when the card's unit came through the
    /// SummonCharactersList overlay or a spawn graph, not SummonCharacter. Such a
    /// card's `second_summon` is the extractor's carrier for the list's other
    /// entries (already inside `count`), so `convert` leaves it unread.
    summon_resolution: Option<serde_json::Value>,
    /// characters.csv SpawnRadius, millitiles, on the unit itself: the ring radius
    /// when SummonRadius is blank (SkeletonWarrior 800).
    spawn_radius_milli: Option<i32>,
    /// characters.csv SpawnAngleShift, degrees (Bat 45).
    spawn_angle_shift_deg: Option<i32>,
    /// characters.csv SpawnMaxAngle, degrees (15.535 only, written where set: the Goblin Giant's Spear Goblins, 90):
    /// `FormationDef::spawn_max_angle_deg`.
    spawn_max_angle_deg: Option<i32>,
    /// characters.csv ProjectileStartRadius, millitiles (the tower arrows 300).
    projectile_start_radius_milli: Option<i32>,
    /// characters.csv Kamikaze / KamikazeTime.
    kamikaze: Option<bool>,
    kamikaze_time_ms: Option<i32>,
    /// cards.json `spawn_pathfind` (characters.csv SpawnPathfindSpeed /
    /// SpawnPathfindMorph). Present = the row is born at its owner's King and travels
    /// underground to the tap (`spawn_pathfind_of`, `CardDef::spawn_pathfind`).
    spawn_pathfind: Option<RawSpawnPathfind>,
    /// cards.json `can_deploy_on_enemy_side` on a troop or building CARD row (spells_characters /
    /// spells_buildings CanDeployOnEnemySide; written on the 15.535 rows only): read with
    /// `spawn_pathfind` alone (`CardDef::can_deploy_on_enemy_side`). Absent reads false.
    can_deploy_on_enemy_side: Option<bool>,
    /// cards.json `buff_on_damage` (characters.csv BuffOnDamage / BuffTime): the buff
    /// this unit's own hit hangs on its victim, for a unit whose attack is not a
    /// projectile (the Electro Wizard's ZapFreeze, 500 ms). Loaded onto
    /// `CardDef::attack_buff`, the same field the projectile's TargetBuff uses --
    /// a card ships one or the other, never both.
    buff_on_damage: Option<RawBuffOnDamage>,
    /// cards.json `reflected_attack` (characters.csv ReflectedAttack* and
    /// ReflectAttackCrownTowerDamage): present only on a row that sets one of them, so absent
    /// on every 2018 row. Loaded onto `CardDef::reflect` by `convert_reflect`.
    reflected_attack: Option<RawReflectedAttack>,
    /// cards.json `deploy_projectile` (a troop card's spells_characters Projectile, the
    /// Mega Knight's MegaKnightAppear): `CardDef::deploy_projectile`.
    deploy_projectile: Option<serde_json::Value>,
    /// cards.json `load_first_hit` (characters.csv LoadFirstHit; the Sparky):
    /// `CardDef::load_first_hit`.
    load_first_hit: Option<bool>,
    /// cards.json `variable_damage` block (VariableDamage2 / VariableDamage3 /
    /// VariableDamageTime1 / VariableDamageTime2): `CardDef::variable_damage`. Not yet
    /// written by tools/extract_cards.py; absent reads as no ramp.
    variable_damage: Option<RawVariableDamage>,
    /// cards.json `attack_pushback_milli` (characters.csv AttackPushBack):
    /// `CardDef::attack_pushback`. Not yet written by the extractor; absent reads as 0.
    attack_pushback_milli: Option<i32>,
    /// cards.json `death_pushback_milli` (characters / buildings DeathPushBack, millitiles; 15.535
    /// rows only): `CardDef::death_pushback`. Absent reads as 0.
    death_pushback_milli: Option<i32>,
    /// cards.json `special` block (SpecialRange / SpecialMinRange / SpecialLoadTime and the
    /// ProjectileSpecial row): `CardDef::special`. Not yet written by the extractor; absent
    /// reads as no special.
    special: Option<RawSpecial>,
    /// cards.json `death_spawn_projectile`: the NAME of the `projectiles` row the unit's death
    /// releases (characters DeathSpawnProjectile; the Phoenix's PhoenixFireball).
    /// `from_json_str` resolves it into `CardDef::death_projectile`. Absent in a file whose
    /// extractor does not write the column, which reads as none.
    death_spawn_projectile: Option<String>,
    /// cards.json `deploy_area_effect`: the NAME of the `area_effect_objects` row the card IS,
    /// when that area's one action spawns the card's own character (spells_characters
    /// AreaEffectObject; the Electro Wizard's ElectroWizardZap). `from_json_str` resolves it into
    /// `CardDef::deploy_area_effect`. Absent reads as none.
    deploy_area_effect: Option<String>,
    /// cards.json `spawn_area_object`: the NAME of the `area_effect_objects` row the unit puts
    /// down where it appears (characters SpawnAreaObject; the Battle Healer's
    /// BattleHealerSpawnHeal). `from_json_str` resolves it into `CardDef::spawn_area_effect`.
    /// Absent reads as none.
    spawn_area_object: Option<String>,
    /// cards.json `summon_members` (15.535 only; the Three Musketeers): `CardDef::summon_members`.
    summon_members: Option<Vec<RawSummonMember>>,
    /// cards.json `summon_offsets_x_mirrored` (CharactersOffsetsXMirrored, beside `summon_members`).
    summon_offsets_x_mirrored: Option<bool>,
    /// cards.json `attack_select` (15.535 only; the Three Musketeers' selector): `CardDef::attack_select`.
    attack_select: Option<RawAttackSelect>,
    /// cards.json `ignore_buffs` (characters / buildings IgnoreBuff, a list; 15.535 rows only): the buff rows that
    /// never land on this unit. `from_json_str` resolves the names into `CardDef::ignore_buffs` once every buff is
    /// interned.
    ignore_buffs: Option<Vec<String>>,
}

/// cards.json `summon_members[k]`, every field nullable.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSummonMember {
    character: Option<String>,
    offset_x_milli: Option<i32>,
    offset_y_milli: Option<i32>,
}

/// cards.json `attack_select`, every field nullable (tools/extract_cards.py `attack_select`).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawAttackSelect {
    melee_range_milli: Option<i32>,
    melee_ground_only: Option<bool>,
    melee_damage: Option<i32>,
    melee_index: Option<i32>,
    ranged_index: Option<i32>,
}

/// THE ACTION CLASSES the attack selector's graph may reach (tools/extract_cards.py `attack_select`): the four it
/// runs and its one cosmetic hook. Its graph must reach the four and nothing outside the five.
const ATTACK_SELECT_CLASSES: [&str; 5] = ["ActionFilter", "ActionSetAttackSequenceIndex", "ActionRunOnInstigator", "ActionDealDamage", "ActionPlayEffect"];

/// The selector an `attack_select` block names, or the reason it is refused. The block must be the one shape the
/// extractor writes -- melee entry 1, the row's own projectile entry 0, a ground-only condition -- and the row's
/// graph exactly the selector: every class in ATTACK_SELECT_CLASSES but the effect, none outside them, no spawn.
fn attack_select_of(raw: &RawAttackSelect, graph: &Option<RawActionGraph>) -> Result<AttackSelectDef, String> {
    let refuse = |why: &str| Err(format!("the unit's attack selector: {why}; not simulated"));
    let Some(g) = graph else { return refuse("no action graph carries it") };
    if !g.spawns.is_empty() || g.class_types.iter().any(|c| !ATTACK_SELECT_CLASSES.contains(&c.as_str())) {
        return refuse(&format!("the graph runs more than the selector ({})", g.class_types.join(", ")));
    }
    if let Some(missing) = ATTACK_SELECT_CLASSES[..4].iter().find(|c| !g.class_types.iter().any(|x| x == *c)) {
        return refuse(&format!("the graph does not reach {missing}"));
    }
    if (raw.melee_index, raw.ranged_index) != (Some(1), Some(0)) {
        return refuse(&format!("sequence entries melee {:?} / ranged {:?}", raw.melee_index, raw.ranged_index));
    }
    if raw.melee_ground_only != Some(true) {
        return refuse("a melee branch that also takes air targets");
    }
    let melee_range = raw.melee_range_milli.filter(|v| *v > 0).ok_or("the unit's attack selector has no melee range")?;
    let melee_damage = raw.melee_damage.filter(|v| *v > 0).ok_or("the unit's attack selector has no melee damage")?;
    Ok(AttackSelectDef { melee_range: milli(melee_range), ground_only: true, melee_damage })
}

/// cards.json `buff_on_damage`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawBuffOnDamage {
    buff: Option<RawBuff>,
    time_ms: Option<i32>,
}

/// cards.json `reflected_attack`, every field nullable (the extractor writes the whole block
/// whenever one of its five columns is set; field names exactly as it writes them).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawReflectedAttack {
    damage: Option<i32>,
    crown_tower_damage: Option<i32>,
    radius_milli: Option<i32>,
    buff: Option<RawBuff>,
    buff_duration_ms: Option<i32>,
}

/// The REFLECT half of the loader: all-or-nothing like the other blocks. A row that sets any of
/// the columns must set ReflectedAttackDamage and a positive ReflectedAttackRadius (a reflect
/// with no damage or no reach is a shape no 15.535 row ships: refused, never guessed), and the
/// buff and its duration come as a pair. A buff that pulses is refused too: the reflect lands
/// its buff with no pulse amount, which such a buff would need from the reflecting unit's level.
fn convert_reflect(raw: Option<RawReflectedAttack>, buffs: &mut BuffTable) -> Result<Option<ReflectDef>, String> {
    let Some(b) = raw else { return Ok(None) };
    let damage = match b.damage {
        Some(x) if x >= 0 => x,
        Some(x) => return Err(format!("reflected_attack: ReflectedAttackDamage {x} is negative")),
        None => return Err("reflected_attack: no ReflectedAttackDamage".into()),
    };
    let radius = match b.radius_milli {
        Some(x) if x > 0 => milli(x),
        Some(x) => return Err(format!("reflected_attack: ReflectedAttackRadius {x} is not positive")),
        None => return Err("reflected_attack: no ReflectedAttackRadius".into()),
    };
    let buff = match (&b.buff, b.buff_duration_ms) {
        (None, None) => None,
        (Some(rb), time) => {
            if rb.convert("the unit's ReflectedAttackBuff")?.pulses() {
                return Err("reflected_attack: ReflectedAttackBuff pulses damage or healing; not simulated".into());
            }
            Some(buffs.apply(rb, time, "the unit's ReflectedAttackBuff")?)
        }
        (None, Some(t)) => return Err(format!("reflected_attack: ReflectedAttackBuffDuration {t} without a ReflectedAttackBuff")),
    };
    Ok(Some(ReflectDef { damage, crown_tower_damage: b.crown_tower_damage, radius, buff }))
}

/// cards.json `variable_damage` block, every field nullable (the extractor is to write the
/// whole block whenever VariableDamage2 is set).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawVariableDamage {
    damage2: Option<i32>,
    damage3: Option<i32>,
    time1_ms: Option<i32>,
    time2_ms: Option<i32>,
}

/// cards.json `special` block, every field nullable (the extractor is to write the whole
/// block whenever SpecialRange is set; `projectile` is the ProjectileSpecial row in the
/// shape of every other projectile object, and `drag_margin_milli` that row's DragMargin).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSpecial {
    range_milli: Option<i32>,
    min_range_milli: Option<i32>,
    load_time_ms: Option<i32>,
    projectile: Option<RawProjectileObj>,
    drag_margin_milli: Option<i32>,
}

/// cards.json `spawn_pathfind`: the underground spawn walk.
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct RawSpawnPathfind {
    speed: Option<i32>,
    morph: Option<String>,
}

/// THE UNDERGROUND SPAWN WALK OF A ROW (characters.csv SpawnPathfindSpeed / SpawnPathfindMorph),
/// or None for a row born where it is played. With the morph's NAME, for the unit loop to load.
///
/// A row with SpawnPathfindSpeed is NOT born at the tap: the recordings show it appearing at its
/// owner's king tower and travelling underground at that speed to the tap. With SpawnPathfindMorph
/// it then MORPHS into the named row on arrival, which is a different card entirely -- the 2560-hp
/// GoblinDrillDig troop the tap answers becomes a 1313-hp GoblinDrill BUILDING with a LifeTime and
/// a Goblin spawner the dig row does not carry (state.rs `phase_tunnel`, `surface`). Before the
/// walk was run the card was refused rather than played as its first row alone: the replay harness
/// had scored the Goblin Drill at 0.0 % within 250 native, 99.6 % of its unit-ticks an alive
/// mismatch, because everything the game put on the board was a row the engine never loaded.
///
/// FAIL CLOSED on every shape the measurement does not cover: a morph without a speed, a speed
/// that is not positive, and a row WITHOUT CanDeployOnEnemySide, whose territory nothing measured
/// (both 15.535.29 rows set it; the 2018 Miner row does not carry the flag in cards.json).
fn spawn_pathfind_of(sp: &Option<RawSpawnPathfind>, enemy_side: bool, what: &str) -> Result<Option<(SpawnPathfindDef, Option<String>)>, String> {
    let Some(p) = sp.as_ref().filter(|p| p.morph.is_some() || p.speed.is_some()) else { return Ok(None) };
    let speed = match p.speed {
        Some(v) if v > 0 => v,
        other => return Err(format!("{what} travels underground with SpawnPathfindSpeed {other:?}, not a positive speed")),
    };
    if !enemy_side {
        return Err(format!(
            "{what} travels underground to the tap (SpawnPathfindSpeed {speed}) without CanDeployOnEnemySide, a territory not simulated"
        ));
    }
    Ok(Some((SpawnPathfindDef { speed, morph: p.morph.as_ref().map(|_| u16::MAX) }, p.morph.clone())))
}

/// cards.json `second_summon` block.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSecondSummon {
    character: Option<String>,
    count: Option<i32>,
}

/// cards.json `action_graph` (tools/extract_cards.py `action_graph`): the
/// ClassTypes a row's actions reach and whether any is a MECHANIC (not an effect,
/// sound, animation or health-bar decoration). A reworked card keeps its rule
/// there -- the 15.535 GoblinHut_Rework clears SpawnNumber and spawns its Spear
/// Goblins from an ActionGoblinHutLifeState, the Furnace from an ActionInterval --
/// so a mechanic graph REFUSES the card: run as its columns alone it would be a
/// different, plainer card.
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct RawActionGraph {
    /// The row's *Action columns and the action each names (OnStartingAction, OnHitAction, ...).
    roots: BTreeMap<String, String>,
    class_types: Vec<String>,
    spawns: Vec<String>,
    mechanic: Option<bool>,
}

/// THE GOBLIN HUT'S LIFE-STATE CONTROLLER (the 15.535.29 tables' ActionGoblinHutLifeState; state.rs `life_state_pass`).
/// All times ms, the offset SUBTILES.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LifeStateDef {
    /// The released unit's CardDb index (a `summon_only` card: SpearGoblin_Dummy, a Spear Goblin with DeployTime 500).
    pub unit: u16,
    /// SpawnNumber: units a wave releases (1, the only count measured).
    pub number: i32,
    /// ActionDelay: from the hut's creation to its first look for an enemy.
    pub action_delay_ms: i32,
    /// SpawnInterval: from one wave to the next while an enemy stays in reach on the due tick.
    pub interval_ms: i32,
    /// SpawnOffset: how far from the hut's centre a wave stands.
    pub offset: i32,
    /// SingleDeployOffsetAngle: a wave stands this many degrees to one side of the line to its aim.
    pub offset_angle_deg: i32,
}

/// cards.json `idle_invisibility`, every field nullable.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawIdleInvisibility {
    time_ms: Option<i32>,
    area_damage_when_invisible: Option<bool>,
}

/// cards.json `life_state_spawner`, every field nullable.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawLifeState {
    action_delay_ms: Option<i32>,
    spawn_interval_ms: Option<i32>,
    character: Option<String>,
    number: Option<i32>,
    offset_milli: Option<i32>,
    offset_angle_deg: Option<i32>,
    object_filter: Option<String>,
}

/// The controller a `life_state_spawner` block names, and its unit's name; or the reason it is refused. The graph must
/// be exactly the controller and its cosmetic hooks (ActionGroup, ActionPlayEffect) and spawn nothing else; the object
/// filter must be DefaultCharacterTargets (the wake set is measured, not read from the filter: state.rs
/// `life_wakers`); a wave of one unit is the only one measured.
fn life_state_of(raw: &RawLifeState, graph: &Option<RawActionGraph>) -> Result<(LifeStateDef, String), String> {
    let refuse = |why: &str| Err(format!("the unit's life-state controller: {why}; not simulated"));
    let Some(g) = graph else { return refuse("no action graph carries it") };
    if !g.spawns.is_empty() || g.class_types.iter().any(|c| !matches!(c.as_str(), "ActionGoblinHutLifeState" | "ActionGroup" | "ActionPlayEffect")) {
        return refuse(&format!("the graph runs more than the controller ({})", g.class_types.join(", ")));
    }
    if raw.object_filter.as_deref() != Some("DefaultCharacterTargets") {
        return refuse(&format!("object filter {:?}", raw.object_filter));
    }
    let number = raw.number.unwrap_or(0);
    if number != 1 {
        return refuse(&format!("{number} units a wave"));
    }
    let interval_ms = raw.spawn_interval_ms.filter(|v| *v > 0).ok_or("the unit's life-state controller has no SpawnInterval")?;
    let action_delay_ms = raw.action_delay_ms.filter(|v| *v >= 0).ok_or("the unit's life-state controller has no ActionDelay")?;
    let offset = milli(raw.offset_milli.filter(|v| *v > 0).ok_or("the unit's life-state controller has no SpawnOffset")?);
    let offset_angle_deg = raw.offset_angle_deg.ok_or("the unit's life-state controller has no SingleDeployOffsetAngle")?;
    let unit = raw.character.clone().filter(|c| !c.is_empty()).ok_or("the unit's life-state controller spawns no character")?;
    Ok((LifeStateDef { unit: u16::MAX, number, action_delay_ms, interval_ms, offset, offset_angle_deg }, unit))
}

/// cards.json `interval_spawner` (tools/extract_cards.py `interval_spawner`), every field nullable.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawIntervalSpawner {
    start_counter_at_ms: Option<i32>,
    interval_ms: Option<i32>,
    affected_by_spawn_speed: Option<bool>,
    pause_tags: Vec<String>,
    character: Option<String>,
    deploy_time_ms: Option<i32>,
    mirrored_x: Option<i32>,
    mirrored_y: Option<i32>,
}

/// The pause tags an interval spawner may name. Nothing a basic card runs sets either on a unit
/// (the 15.535.29 game_tags table: NO_SUMMON is for evolution cards, UNIT_CUSTOM_TAG_1 for one
/// evolution and one champion), so a tag from this list never pauses anything here; any other tag
/// refuses the card.
const INERT_PAUSE_TAGS: &[&str] = &["NO_SUMMON", "UNIT_CUSTOM_TAG_1"];

/// The periodic spawner an `interval_spawner` block names (a `SpawnerDef` of source ActionInterval), and its
/// unit's name; or the reason it is refused. The graph must be exactly the interval, its spawn and a cosmetic
/// effect, and spawn exactly the block's character; the interval must run at the spawner's SpawnSpeed
/// (AffectedBySpawnSpeed: the only reading spawner.ACTION_SPAWNER_SPAWN_SPEED has); every number the timer and
/// the point need must be there, and the point's x half (MirroredX) must be 0, the one value measured. One unit per
/// emission: the action has no count column.
fn interval_spawner_of(raw: &RawIntervalSpawner, graph: &Option<RawActionGraph>) -> Result<(SpawnerDef, String), String> {
    let refuse = |why: &str| Err(format!("the unit's interval spawner: {why}; not simulated"));
    let Some(g) = graph else { return refuse("no action graph carries it") };
    let unit = match raw.character.as_deref() {
        Some(u) if !u.is_empty() => u.to_string(),
        _ => return refuse("it spawns no character"),
    };
    if g.class_types.iter().any(|c| !matches!(c.as_str(), "ActionInterval" | "ActionSpawnToLocation" | "ActionPlayEffect")) {
        return refuse(&format!("the graph runs more than the interval and its spawn ({})", g.class_types.join(", ")));
    }
    let want = format!("CharacterType:{unit}");
    if g.spawns.len() != 1 || g.spawns[0] != want {
        return refuse(&format!("the graph spawns {} rather than the block's one character", g.spawns.join(", ")));
    }
    if raw.affected_by_spawn_speed != Some(true) {
        return refuse("an interval that does not run at the spawner's SpawnSpeed");
    }
    if let Some(tag) = raw.pause_tags.iter().find(|t| !INERT_PAUSE_TAGS.contains(&t.as_str())) {
        return refuse(&format!("pause tag {tag}"));
    }
    let start = raw.start_counter_at_ms.filter(|v| *v >= 0).ok_or("the unit's interval spawner has no StartCounterAt")?;
    let interval = raw.interval_ms.filter(|v| *v > 0).ok_or("the unit's interval spawner has no Interval")?;
    let at = match (raw.mirrored_x, raw.mirrored_y) {
        (Some(x), Some(y)) => (x, y),
        _ => return refuse("its spawn has no MirroredX / MirroredY"),
    };
    // spawner.SPAWN_TO_LOCATION_OFFSET is measured on the y half alone: the one row that loads, the Furnace's, ships
    // MirroredX 0. Where an x offset stands, and which way it turns for each seat, nothing measured.
    if at.0 != 0 {
        return refuse(&format!("MirroredX {}, an x offset nothing measured", at.0));
    }
    let deploy = match raw.deploy_time_ms {
        Some(d) if d < 0 => return refuse(&format!("DeployTime {d}")),
        d => d,
    };
    Ok((
        SpawnerDef {
            unit: u16::MAX,
            number: 1,
            interval_ms: 0,
            start_time_ms: Some(start),
            pause_time_ms: interval,
            limit: None,
            radius: None,
            source: SpawnerSource::ActionInterval,
            to_location: Some(at),
            emit_deploy_ms: deploy,
        },
        unit,
    ))
}

/// cards.json `mana` (tools/extract_cards.py `norm_unit`), every field nullable.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawMana {
    collect_amount: Option<i32>,
    generate_time_ms: Option<i32>,
    on_death: Option<i32>,
    on_death_for_opponent: Option<i32>,
}

/// The `mana` block as a `ManaDef`, or the reason the card is refused: a payout is both columns, positive, on a
/// building (the engine's payout timer runs on the Elixir Collector's shape and nothing else); the two death
/// columns are non-negative. None when the row carries no block.
fn convert_mana(raw: Option<RawMana>, kind: CardKind) -> Result<Option<ManaDef>, String> {
    let Some(m) = raw else { return Ok(None) };
    let collect = match (m.collect_amount, m.generate_time_ms) {
        (None, None) => None,
        (Some(a), Some(t)) if a > 0 && t > 0 && kind == CardKind::Building => Some((a, t)),
        (a, t) => return Err(format!("elixir production ManaCollectAmount {a:?} / ManaGenerateTimeMs {t:?} on a {kind:?} is not simulated")),
    };
    let nonneg = |v: Option<i32>, what: &str| match v {
        Some(x) if x < 0 => Err(format!("{what} {x} < 0")),
        Some(x) => Ok(x),
        None => Ok(0),
    };
    Ok(Some(ManaDef { collect, on_death: nonneg(m.on_death, "ManaOnDeath")?, on_death_for_opponent: nonneg(m.on_death_for_opponent, "ManaOnDeathForOpponent")? }))
}

/// THE RUNE GIANT'S ENCHANT (the 15.535.29 tables' ActionGiantBufferCollectFriends and ActionGiantBufferBuff; state.rs
/// `enchant_pass` and `launch_due_enchants`, combat.rs `enchant_bonus`; calibration enchant.*). The unit looks for
/// friends after `first_ms`, sends each friend it picks a homing projectile, and the friend it lands on deals a bonus on
/// every `period`-th attack while the enchant lasts. Distances SUBTILES, times ms, amounts level-1 figures, multipliers
/// per mille.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnchantDef {
    /// ActionDelay: from the unit's creation to its first look.
    pub first_ms: i32,
    /// Cooldown: from a launch to the next look.
    pub cooldown_ms: i32,
    /// MaxFriendlyTroops: how many friends may hold this unit's enchant at once.
    pub max_targets: u8,
    /// DistanceToGetTargets: the pick's centre reach, before calibration enchant.PICK_REACH's extra.
    pub pick_radius: i32,
    /// DistanceToBuff: the reach checked when the projectile lands (enchant.BUFF_RANGE_CHECK).
    pub buff_radius: i32,
    /// BuffDelay: read by enchant.LAUNCH_DELAY.
    pub buff_delay_ms: i32,
    /// The projectile's Speed, raw (the unit of every projectile Speed column).
    pub bolt_speed: i32,
    /// AttackAmount: the bonus lands on every `period`-th attack (enchant.BONUS_ATTACKS).
    pub period: u8,
    /// AddedDamage and AddedCrownTowerDamage: the bonus, level-1 figures.
    pub added: i32,
    pub added_crown: i32,
    /// FinishIfInstigatorDies: how long the enchant outlives the unit that gave it (enchant.INSTIGATOR_DEATH).
    pub finish_ms: i32,
    /// DamageMultiplierPerUnitNames and Values, in table order: a row name and its per mille.
    pub multipliers: Vec<(String, i32)>,
    /// The multipliers resolved against the loaded cards (`CardDb::from_json_str`): (CardDb index, direct per mille,
    /// spark per mille), sorted by index. An index not listed takes 1000 for both.
    pub per_attacker: Vec<(u16, i32, i32)>,
    /// The rows the tables tag NO_GIANTBUFFER_CHEF_ENCHANTMENT, by name, and the loaded cards they are (sorted
    /// CardDb indices): never picked.
    pub excluded_units: Vec<String>,
    pub excluded: Vec<u16>,
}

impl EnchantDef {
    /// The (direct, spark) per mille of attacker card `idx`: its entry in `per_attacker`, or 1000 for both.
    pub fn per_mille(&self, idx: u16) -> (i32, i32) {
        match self.per_attacker.binary_search_by_key(&idx, |e| e.0) {
            Ok(k) => (self.per_attacker[k].1, self.per_attacker[k].2),
            Err(_) => (PER_MILLE, PER_MILLE),
        }
    }
}

/// A whole bonus: a multiplier of 1000 per mille.
pub const PER_MILLE: i32 = 1000;

/// The three mechanic classes an `enchant_friends` block stands for, sorted.
const ENCHANT_CLASSES: [&str; 3] = ["ActionGiantBufferBuff", "ActionGiantBufferBuffVisual", "ActionGiantBufferCollectFriends"];

/// cards.json `enchant_friends`, every field nullable.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawEnchantFriends {
    collect: RawEnchantCollect,
    projectile: RawEnchantBolt,
    enchant: RawEnchantBuff,
    excluded_units: Vec<String>,
    classes: Vec<String>,
}

/// cards.json `enchant_friends.collect`: the ActionGiantBufferCollectFriends row.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawEnchantCollect {
    action_delay_ms: Option<i32>,
    cooldown_ms: Option<i32>,
    max_targets: Option<i32>,
    pick_radius_milli: Option<i32>,
    buff_radius_milli: Option<i32>,
    buff_delay_ms: Option<i32>,
}

/// cards.json `enchant_friends.projectile`: the row the enchant travels on.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawEnchantBolt {
    speed: Option<i32>,
}

/// cards.json `enchant_friends.enchant`: the ActionGiantBufferBuff row.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawEnchantBuff {
    attack_amount: Option<i32>,
    added_damage: Option<i32>,
    added_crown_tower_damage: Option<i32>,
    finish_if_instigator_dies_ms: Option<i32>,
    multipliers: Vec<(String, i32)>,
}

/// The enchant an `enchant_friends` block names, or the reason it is refused. The graph must be exactly the enchant
/// and its cosmetic hooks (ActionGroup, ActionPlayEffect) and spawn nothing; the block must stand for exactly the three
/// mechanic classes; every parameter must be present and in range.
fn enchant_of(raw: &RawEnchantFriends, graph: &Option<RawActionGraph>) -> Result<EnchantDef, String> {
    let refuse = |why: String| Err(format!("the unit's enchant: {why}; not simulated"));
    let Some(g) = graph else { return refuse("no action graph carries it".into()) };
    let read = |c: &str| ENCHANT_CLASSES.contains(&c) || matches!(c, "ActionGroup" | "ActionPlayEffect");
    if !g.spawns.is_empty() || g.class_types.iter().any(|c| !read(c)) {
        return refuse(format!("the graph runs more than the enchant ({})", g.class_types.join(", ")));
    }
    let mut classes = raw.classes.clone();
    classes.sort();
    if classes != ENCHANT_CLASSES {
        return refuse(format!("the block stands for {}", raw.classes.join(", ")));
    }
    let need = |v: Option<i32>, what: &str| v.ok_or_else(|| format!("the unit's enchant has no {what}; not simulated"));
    let (c, e) = (&raw.collect, &raw.enchant);
    let max_targets = need(c.max_targets, "MaxFriendlyTroops")?;
    let first_ms = need(c.action_delay_ms, "ActionDelay")?;
    let cooldown_ms = need(c.cooldown_ms, "Cooldown")?;
    let pick = need(c.pick_radius_milli, "DistanceToGetTargets")?;
    let buff = need(c.buff_radius_milli, "DistanceToBuff")?;
    let buff_delay_ms = need(c.buff_delay_ms, "BuffDelay")?;
    let bolt_speed = need(raw.projectile.speed, "projectile Speed")?;
    let period = need(e.attack_amount, "AttackAmount")?;
    let added = need(e.added_damage, "AddedDamage")?;
    let added_crown = need(e.added_crown_tower_damage, "AddedCrownTowerDamage")?;
    let finish_ms = need(e.finish_if_instigator_dies_ms, "FinishIfInstigatorDies")?;
    if !(1..=4).contains(&max_targets) {
        return refuse(format!("{max_targets} friends at once"));
    }
    if first_ms < 0 || cooldown_ms <= 0 || buff_delay_ms < 0 || finish_ms < 0 {
        return refuse(format!("times {first_ms} / {cooldown_ms} / {buff_delay_ms} / {finish_ms} ms"));
    }
    if pick <= 0 || buff < pick {
        return refuse(format!("pick reach {pick} and buff reach {buff}"));
    }
    if bolt_speed <= 0 || !(1..=255).contains(&period) || added < 0 || added_crown < 0 {
        return refuse(format!("speed {bolt_speed}, every {period} attacks, bonus {added} / {added_crown}"));
    }
    if let Some((n, m)) = e.multipliers.iter().find(|(_, m)| !(0..=PER_MILLE).contains(m)) {
        return refuse(format!("multiplier {m} for {n}"));
    }
    Ok(EnchantDef {
        first_ms,
        cooldown_ms,
        max_targets: max_targets as u8,
        pick_radius: milli(pick),
        buff_radius: milli(buff),
        buff_delay_ms,
        bolt_speed,
        period: period as u8,
        added,
        added_crown,
        finish_ms,
        multipliers: e.multipliers.clone(),
        per_attacker: Vec::new(),
        excluded_units: raw.excluded_units.clone(),
        excluded: Vec::new(),
    })
}

/// THE HEALTH-THRESHOLD TRANSFORMATION (the 15.535.29 tables' ActionRunActionAtHealth whose action is an
/// ActionChangeGameObjectData, run directly or from an ActionGroup; state.rs `health_triggers`, `rebind_unit`).
/// When the unit's hp reaches `pct` percent of its max hp it becomes the row `unit`, in place: the same entity,
/// its hp and max hp kept (calibration transform.*). The Cannon Cart breaks into its cannon at once; the Goblin
/// Demolisher becomes its kamikaze form 100 ms later, its target reset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TransformDef {
    /// HealthPercentages: the threshold, percent of max hp.
    pub pct: i32,
    /// The row the unit becomes: a `summon_only` CardDb index, u16::MAX until `from_json_str` resolves it.
    pub unit: u16,
    /// ResetTarget on the ChangeGameObjectData action: the unit drops its target and its attack.
    pub reset_target: bool,
    /// From the trigger to the change, ms: the change's SubActionsDelay in its group, 0 when it runs directly.
    pub delay_ms: i32,
}

/// cards.json `transform_at_hp`, every field nullable (tools/extract_cards.py `transform_at_hp`).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawTransform {
    pct: Option<i32>,
    into: Option<String>,
    reset_target: Option<bool>,
    group_delays_ms: Option<Vec<i32>>,
    at: Option<i32>,
    noop_spawns: Option<Vec<String>>,
}

/// The transformation a `transform_at_hp` block names, and the name of the row it becomes; or the reason it is
/// refused. The graph must be the health trigger, the change and its cosmetic hooks (ActionGroup, ActionPlayEffect),
/// and spawn nothing but the block's taunt cancels (the engine has no taunt, so a cancel does nothing); any other
/// graph is refused with the message every unscripted graph gets (`refuse_action_mechanic`).
///
/// THE DELAY. SubActionsDelay can be read as each sub-action's offset from the group's start or as the gap after
/// the one before it. The two agree when every entry before the change is 0 (the Goblin Demolisher's [0, 100], the
/// change second: 100 both ways), and only then is the group read here; a group whose readings disagree is refused.
fn transform_of(raw: &RawTransform, graph: &Option<RawActionGraph>) -> Result<(TransformDef, String), String> {
    let refuse = |why: String| Err(format!("the unit's transformation: {why}; not simulated"));
    let Some(g) = graph else { return refuse("no action graph carries it".into()) };
    let noops = raw.noop_spawns.clone().unwrap_or_default();
    #[cfg(not(clash_plant = "transform_block_unchecked"))]
    {
        let has = |c: &str| g.class_types.iter().any(|x| x == c);
        let known = g
            .class_types
            .iter()
            .all(|c| matches!(c.as_str(), "ActionRunActionAtHealth" | "ActionChangeGameObjectData" | "ActionGroup" | "ActionPlayEffect" | "ActionSpawn"));
        let spawns_listed = g.spawns.iter().all(|s| noops.contains(s)) && noops.iter().all(|s| g.spawns.contains(s));
        if !known || !spawns_listed || !has("ActionRunActionAtHealth") || !has("ActionChangeGameObjectData") {
            refuse_action_mechanic(graph, "the unit")?;
            return refuse(format!("the graph is not a transformation ({})", g.class_types.join(", ")));
        }
    }
    #[cfg(clash_plant = "transform_block_unchecked")]
    let _ = (&noops, g); // PLANT: any graph that carries a transformation block loads.
    let pct = raw.pct.filter(|p| (1..=99).contains(p)).ok_or_else(|| format!("the unit's transformation: HealthPercentages {:?} is not a percent from 1 to 99; not simulated", raw.pct))?;
    let into = raw.into.clone().filter(|n| !n.is_empty()).ok_or("the unit's transformation names no character; not simulated")?;
    let delays = raw.group_delays_ms.clone().unwrap_or_default();
    let at = raw.at.unwrap_or(0);
    let delay_ms = if delays.is_empty() {
        if at != 0 {
            return refuse(format!("its place {at} in a group with no delays"));
        }
        0
    } else {
        let k = usize::try_from(at).ok().filter(|k| *k < delays.len()).ok_or_else(|| format!("the unit's transformation: its place {at} is outside its group of {}; not simulated", delays.len()))?;
        if delays.iter().any(|d| *d < 0) {
            return refuse(format!("SubActionsDelay {delays:?} holds a negative delay"));
        }
        if delays[..k].iter().any(|d| *d != 0) {
            return Err(format!("the transformation's SubActionsDelay {delays:?} reads two ways; not simulated"));
        }
        delays[k]
    };
    Ok((TransformDef { pct, unit: u16::MAX, reset_target: raw.reset_target.unwrap_or(false), delay_ms }, into))
}

/// cards.json `parry`, every field nullable (tools/extract_cards.py `parry`; names exactly as it writes them).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawParry {
    counter_cooldown_ms: Option<i32>,
    deploy_active: Option<bool>,
    damage_scalar_pct: Option<i32>,
    defense_scalar_pct: Option<i32>,
    root_delays_ms: Option<Vec<i32>>,
    counter_at: Option<i32>,
    self_delays_ms: Option<Vec<i32>>,
    self_forced_ms: Option<i32>,
    self_tag_ms: Option<i32>,
    instigator_delays_ms: Option<Vec<i32>>,
    stun_at: Option<i32>,
    reflect_at: Option<i32>,
    stun: Option<RawBuff>,
    stun_time_ms: Option<i32>,
    reflect_level_scaling: Option<bool>,
}

/// The classes a counter's graph may hold: the counter, its three groups, the forced animation, the cooldown tag,
/// the stun's spawn, the reflect's damage and cosmetic effects.
const PARRY_CLASSES: [&str; 7] = [
    "ActionCounter",
    "ActionDealDamage",
    "ActionGroup",
    "ActionPlayEffect",
    "ActionRunForcedAnimationOnce",
    "ActionSpawn",
    "ActionWithDuration",
];

/// The counter a `parry` block names, or the reason it is refused. The graph must hold the counter's classes and
/// nothing else (`PARRY_CLASSES`) and spawn exactly the stun row; any other graph is refused with the message every
/// unscripted graph gets (`refuse_action_mechanic`). Then every column the engine reads must be there and in the one
/// shape it runs: the counter and the self group act at once (every root and self delay 0), the reflect is not
/// level-scaled (its damage type's EnableLevelScaling false), and the cooldown tag lasts exactly the Cooldown (which
/// of two different figures is the cooldown would be a guess).
fn parry_of(raw: &RawParry, graph: &Option<RawActionGraph>, buffs: &mut BuffTable) -> Result<ParryDef, String> {
    let refuse = |why: String| Err(format!("the unit's counter: {why}; not simulated"));
    let Some(g) = graph else { return refuse("no action graph carries it".into()) };
    let stun_name = raw.stun.as_ref().and_then(|b| b.name.clone()).unwrap_or_default();
    #[cfg(not(clash_plant = "parry_shape_unchecked"))]
    {
        let known = g.class_types.iter().all(|c| PARRY_CLASSES.contains(&c.as_str()));
        let spawns_stun = g.spawns.len() == 1 && g.spawns[0] == format!("BuffType:{stun_name}");
        if !known || !spawns_stun || !g.class_types.iter().any(|c| c == "ActionCounter") {
            refuse_action_mechanic(graph, "the unit")?;
            return refuse(format!("the graph is not a counter ({})", g.class_types.join(", ")));
        }
    }
    #[cfg(clash_plant = "parry_shape_unchecked")]
    let _ = (g, &stun_name, PARRY_CLASSES); // PLANT: any graph that carries a counter block loads.
    let cooldown_ms = match raw.counter_cooldown_ms {
        Some(c) if c > 0 => c,
        other => return refuse(format!("Cooldown {other:?} is not a positive time")),
    };
    let ready_at_deploy = raw.deploy_active.ok_or("the unit's counter: no DeployActive column; not simulated")?;
    let taken_pct = match raw.defense_scalar_pct {
        Some(p @ 0..=100) => p,
        other => return refuse(format!("DefenseScalar {other:?} is not a percent from 0 to 100")),
    };
    let reflect_pct = match raw.damage_scalar_pct {
        Some(p) if p >= 0 => p,
        other => return refuse(format!("DamageScalar {other:?} is not a percent")),
    };
    for (what, d) in [("the counter's own group", &raw.root_delays_ms), ("the self group", &raw.self_delays_ms)] {
        if d.as_ref().is_some_and(|d| d.iter().any(|x| *x != 0)) {
            return refuse(format!("{what} has a SubActionsDelay {:?} that is not 0", d.as_ref().unwrap()));
        }
    }
    let group = raw.instigator_delays_ms.clone().unwrap_or_default();
    if group.is_empty() || group.len() > 4 {
        return refuse(format!("an instigator group of {} actions", group.len()));
    }
    if group.iter().any(|d| *d < 0) {
        return refuse(format!("the instigator group's SubActionsDelay {group:?} holds a negative delay"));
    }
    let place = |at: Option<i32>, what: &str| -> Result<u8, String> {
        at.and_then(|k| u8::try_from(k).ok())
            .filter(|k| (*k as usize) < group.len())
            .ok_or_else(|| format!("the unit's counter: its {what} is not in its group of {}; not simulated", group.len()))
    };
    let stun_at = place(raw.stun_at, "stun")?;
    let reflect_at = place(raw.reflect_at, "reflect")?;
    if stun_at == reflect_at {
        return refuse("its stun and its reflect are one action".into());
    }
    let rb = raw.stun.as_ref().ok_or("the unit's counter spawns no buff; not simulated")?;
    if rb.convert("the unit's counter stun")?.pulses() {
        return refuse("its stun pulses damage or healing".into());
    }
    let stun = buffs.apply(rb, raw.stun_time_ms, "the unit's counter stun")?;
    if raw.reflect_level_scaling != Some(false) {
        return refuse(format!("a reflect whose level scaling is {:?} has no reading here", raw.reflect_level_scaling));
    }
    if raw.self_tag_ms != Some(cooldown_ms) {
        return refuse(format!("the cooldown tag lasts {:?} and the Cooldown is {cooldown_ms}", raw.self_tag_ms));
    }
    let self_lock_ms = raw.self_forced_ms.filter(|m| *m >= 0).ok_or("the unit's counter: its forced animation has no ForcedDuration; not simulated")?;
    let mut delays_ms = [0i32; 4];
    delays_ms[..group.len()].copy_from_slice(&group);
    // `counter_at` names the counter's place in its own group; every entry there is 0 (checked above), so it
    // moves nothing, and it is read here so a block without it is refused like any other half-blank block.
    raw.counter_at.filter(|k| *k >= 0).ok_or("the unit's counter: no place in its group; not simulated")?;
    Ok(ParryDef {
        cooldown_ms,
        ready_at_deploy,
        taken_pct,
        reflect_pct,
        self_lock_ms,
        stun,
        delays_ms,
        group_len: group.len() as u8,
        stun_at,
        reflect_at,
    })
}

/// Err when `graph` scripts a mechanic this loader does not read.
fn refuse_action_mechanic(graph: &Option<RawActionGraph>, what: &str) -> Result<(), String> {
    match graph {
        Some(g) if g.mechanic.unwrap_or(false) => Err(format!(
            "{what} runs an action graph this loader does not read ({}{})",
            g.class_types.join(", "),
            if g.spawns.is_empty() { String::new() } else { format!("; spawns {}", g.spawns.join(", ")) }
        )),
        _ => Ok(()),
    }
}

/// cards.json `jump` block, every field nullable (the extractor writes the whole
/// block whenever JumpEnabled is set).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawJump {
    height_raw: Option<i32>,
    speed: Option<i32>,
}

/// cards.json `dash` block, every field nullable (the extractor writes the whole block
/// whenever DashDamage and DashMaxRange are set, and `speed` / `constant_time_ms` /
/// `landing_time_ms` on the 15.535 rows only).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawDash {
    damage: Option<i32>,
    min_range_milli: Option<i32>,
    max_range_milli: Option<i32>,
    radius_milli: Option<i32>,
    cooldown_ms: Option<i32>,
    immune_to_damage_time_ms: Option<i32>,
    pushback_milli: Option<i32>,
    /// None: the key is absent (the 2018 file). Some(None): present and blank.
    #[serde(deserialize_with = "present")]
    speed: Option<Option<i32>>,
    constant_time_ms: Option<i32>,
    landing_time_ms: Option<i32>,
}

/// A field whose ABSENCE means something other than a blank: Some(value) whenever the key is
/// there, null included; `default` (None) when it is not.
fn present<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Option<i32>>, D::Error> {
    Option::<i32>::deserialize(d).map(Some)
}

/// cards.json `charge` block, every field nullable (the extractor writes the whole
/// block whenever DamageSpecial is set; field names exactly as it writes them).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawCharge {
    damage_special: Option<i32>,
    charge_range_raw: Option<i32>,
    charge_speed_multiplier_percent: Option<i32>,
}

/// cards.json `spawner` block, every field nullable (the extractor writes the
/// whole block whenever SpawnCharacter is set).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSpawner {
    character: Option<String>,
    number: Option<i32>,
    interval_ms: Option<i32>,
    start_time_ms: Option<i32>,
    pause_time_ms: Option<i32>,
    limit: Option<i32>,
    radius_milli: Option<i32>,
    /// SpawnAttach (15.535 only, written where set): the block is an ATTACHED RIDER
    /// (`convert_attach`), not a periodic spawner.
    attach: Option<bool>,
    /// SpawnCharacter2: a second periodic unit (the Super Witch's Bat), refused at the top of `convert`. Written on the
    /// 15.535 rows only.
    character2: Option<String>,
}

/// cards.json `death_spawn` block.
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct RawDeathSpawn {
    character: Option<String>,
    count: Option<i32>,
    radius_milli: Option<i32>,
    deploy_time_ms: Option<i32>,
}

/// Which mechanic of a card needs a unit loaded (`CardDb::from_json_str`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UnitUse {
    /// A spell projectile's SpawnCharacter (Goblin Barrel).
    Spell,
    /// The Spawn* block.
    Spawner,
    /// The DeathSpawn* block.
    DeathSpawn,
    /// DeathAreaEffect: an area effect the death releases. NOT a unit -- it resolves
    /// against the file's `area_effect_objects` table into the card's own
    /// `death_area_effect` block, and a card whose area the loader refuses goes the
    /// unloadable way (rejected after its push) with the area's reason.
    DeathAreaEffect,
    /// SummonCharacterSecond (Goblin Gang's Spear Goblins, the Rascals' Girls).
    SecondSummon,
    /// DeathSpawnProjectile: a PROJECTILE the death releases. Not a unit -- it resolves against
    /// the file's `projectiles` table into the card's own `death_projectile` block, and its
    /// SpawnCharacter is then the card's `DeathProjectileRelease` need.
    DeathProjectile,
    /// The SpawnCharacter of a death projectile (the Phoenix's PhoenixEgg): a unit, loaded like a
    /// spawner's, and the one unit that may itself carry a periodic spawner (its egg hatches).
    DeathProjectileRelease,
    /// The AreaEffectObject a card IS when it spawns its own character (`deploy_area_effect`). Not
    /// a unit: resolved against `area_effect_objects`, like a death area effect.
    DeployAreaEffect,
    /// SpawnAreaObject (`spawn_area_effect`). Not a unit: resolved against `area_effect_objects`.
    SpawnAreaEffect,
    /// A spell's SummonCharacter (`SpellShape::Summon`, or the `Fuse` a bottle row becomes).
    SpellSummon,
    /// A troop projectile's SpawnAreaEffectObject: an AREA, resolved like `DeathAreaEffect` into
    /// `CardDef::projectile_area`.
    ProjectileArea,
    /// The unit a life-state controller releases (`LifeStateDef::unit`, the Goblin Hut's SpearGoblin_Dummy).
    LifeState,
    /// SpawnPathfindMorph: the row a tunneller leaves where it comes up (`SpawnPathfindDef::morph`, the
    /// Goblin Drill's building). A unit, loaded from `units` like a death spawn's; never the card of
    /// the same name (the GoblinDrill CARD's row is the dig), which the name rule below already says.
    Morph,
    /// An attached rider (`AttachDef::unit`, the Ram Rider's rider), whose row must be a shape the
    /// rider law covers (`rider_shape`).
    Attach,
    /// Member k >= 1 of a deploy at explicit offsets (`CardDef::summon_members`; the Three Musketeers' second and
    /// third): a unit, loaded like a spawner's. Member 0 is the card itself and needs nothing.
    SummonMember(u8),
    /// Form k of a variant card (`SpellShape::Variant`; the Spirit Empress). Not a unit: a CARD, resolved by its
    /// internal name after the unit loop (`CardDb::from_json_str`), never through `units`.
    VariantForm(u8),
    /// The death-spawn unit of buff index `.0` (`BuffDef::death_spawn`: the Mother Witch's VoodooHog, the Goblin
    /// Curse's GoblinCurseGoblin), a need of every card that can hang that buff. Resolved onto the buff table's row,
    /// which every such card shares, not onto the card.
    BuffDeathSpawn(u16),
    /// The row a transformation turns the unit into (`TransformDef::unit`: the Cannon Cart's BrokenCannon, the
    /// Goblin Demolisher's kamikaze form). The one use under which a troop row may carry a LifeTime.
    Transform,
    /// The unit of entry k of a scheduled area (`SpellShape::ScheduledArea`: the Graveyard's Skeletons, the Suspicious
    /// Bush's goblins), resolved onto the first of the card's spell objects whose entry k is still unresolved.
    Scheduled(u8),
}

impl UnitUse {
    /// A need resolved against one of the file's own tables into a block of the card (an area or a
    /// projectile), not loaded as a unit: it adds no summon-only record, and the units IT needs
    /// are queued at the end of its owner's level of the worklist.
    fn is_table(self) -> bool {
        matches!(self, UnitUse::DeathAreaEffect | UnitUse::ProjectileArea | UnitUse::DeathProjectile | UnitUse::DeployAreaEffect | UnitUse::SpawnAreaEffect)
    }
}

/// A record's needs in the order the worklist takes them: the TABLE needs first (`UnitUse::is_table`),
/// then the units, each group in the converter's order. A table need adds no summon-only record and
/// queues its own units at the end of its level, so this order numbers no record differently; what
/// it changes is that a card refused by a table row is refused BEFORE any unit of its is loaded, so
/// a refused card leaves no summon-only record behind (the SuperLavaHound, refused for its chained
/// FireWallProjectile, would otherwise load SuperLavaHound2 and renumber every later record).
/// Plant `unit_needs_first` (tests/spawn_chain.rs) keeps the converter's order.
fn table_needs_first(needs: UnitNeeds) -> UnitNeeds {
    // PLANT unit_needs_first leaves the converter's order (a unit need before a table need).
    #[cfg(not(clash_plant = "unit_needs_first"))]
    let needs = {
        let (mut tables, units): (UnitNeeds, UnitNeeds) = needs.into_iter().partition(|(w, _)| w.is_table());
        tables.extend(units);
        tables
    };
    needs
}

/// The units a converter's record needs loaded: (which mechanic, unit name), for
/// `CardDb::from_json_str` to resolve.
type UnitNeeds = Vec<(UnitUse, String)>;

/// THE LONGEST SPAWN CHAIN the loader follows (card -> unit -> unit ...): a record first loaded
/// at this depth of the worklist may not need units of its own. A policy, not a measurement: the
/// longest chain a 15.535.29 card carries is two deep (the Elixir Golem: ElixirGolem2 ->
/// ElixirGolem4; the Goblin Drill: its building -> Goblin), and four leaves room without
/// letting a cycle of rows run away. `CardDb::check_levels` walks no deeper.
pub const MAX_CHAIN_DEPTH: u8 = 4;

/// A BLOCK OF A CARD THAT PUTS ANOTHER RECORD ON THE BOARD, as `CardDb::unit_refs`
/// names it. The death area effect is not one: it is an area, not a unit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnitRef {
    /// A spell projectile's SpawnCharacter (Goblin Barrel): `SpellShape::Projectile`'s
    /// `spawn`, the one block that carries a level index.
    SpellRelease,
    /// The Spawn* block (`CardDef::spawner`).
    Spawner,
    /// The DeathSpawn* block (`CardDef::death_spawn`).
    DeathSpawn,
    /// SummonCharacterSecond (`FormationDef::second_summon`).
    SecondSummon,
    /// A death projectile's SpawnCharacter (`CardDef::death_projectile`'s `SpellShape::Projectile`
    /// `spawn`, the Phoenix's egg), with the projectile row's level index. The one block whose
    /// unit may itself put a unit on the board (a periodic spawner), which `check_levels` and
    /// py.rs `ids_of_indices` follow one level down.
    DeathProjectile,
    /// A spell's summoned unit (`SpellShape::Summon`).
    SpellSummon,
    /// A life-state controller's unit (`LifeStateDef::unit`).
    LifeState,
    /// The row a tunneller leaves where it comes up (`SpawnPathfindDef::morph`, the Goblin Drill's
    /// building).
    Morph,
    /// An attached rider (`CardDef::attach`, the Ram Rider's rider).
    Attach,
    /// Member k of a deploy at explicit offsets (`CardDef::summon_members`): member 0 is the card's own index, the
    /// others their own records.
    SummonMember(u8),
    /// Form k of a variant card (`SpellShape::Variant`): a registered card, which the play deploys as itself.
    VariantForm(u8),
    /// The death spawn of a buff the card hangs (`BuffDef::death_spawn`; the Mother Witch's VoodooHog, the Goblin
    /// Curse's GoblinCurseGoblin), one entry per buff (`CardDb::card_buffs`). The unit is released for the side
    /// the buff says, usually the caster's, where the dying unit fell.
    BuffDeathSpawn,
    /// The row a transformation turns the unit into (`TransformDef::unit`). The entity stays the same one, so what
    /// this block puts on the board is the row, not a new unit.
    Transform,
    /// Entry k of a scheduled area (`SpellShape::ScheduledArea`) in the card's spell, death area or projectile area:
    /// the Graveyard's Skeletons, the Suspicious Bush's goblins.
    Scheduled(u8),
}

impl UnitRef {
    /// The block as a refusal names it.
    pub fn block_name(self) -> &'static str {
        match self {
            UnitRef::SpellRelease => "a spell release",
            UnitRef::Spawner => "a periodic spawner",
            UnitRef::DeathSpawn => "a death spawn",
            UnitRef::SecondSummon => "a second summon",
            UnitRef::DeathProjectile => "a death projectile",
            UnitRef::SpellSummon => "a spell summon",
            UnitRef::LifeState => "a life-state controller",
            UnitRef::Morph => "an underground morph",
            UnitRef::Attach => "an attached rider",
            UnitRef::SummonMember(_) => "a summon member",
            UnitRef::VariantForm(_) => "a variant form",
            UnitRef::BuffDeathSpawn => "a buff's death spawn",
            UnitRef::Transform => "a transformation",
            UnitRef::Scheduled(_) => "a scheduled spawn",
        }
    }
}

#[derive(Deserialize)]
struct RawCardsFile {
    /// cards.json `version` (tools/extract_cards.py): which table this is (`CardDb::version`).
    /// Absent: empty.
    #[serde(default)]
    version: String,
    cards: Vec<RawCard>,
    #[serde(default)]
    towers: Vec<RawCard>,
    /// Every characters/buildings row by name. Only units a simulable spell spawns
    /// are loaded (as `summon_only` cards).
    #[serde(default)]
    units: BTreeMap<String, serde_json::Value>,
    /// cards.json `rarities`: the file's own rarities.csv (level counts, relative
    /// levels, ladders). Absent: the shipped 2018 table.
    #[serde(default)]
    rarities: BTreeMap<String, RawRarity>,
    /// cards.json `area_effect_objects`: every area_effect_objects row by name, the
    /// same shape a spell's `area_effect_object` block carries. Only the ones a
    /// card's DeathAreaEffect names are read (a spell carries its own inline). A
    /// file without the table leaves this empty, and every card with a
    /// DeathAreaEffect is then REFUSED BY NAME -- never silently run without it.
    #[serde(default)]
    area_effect_objects: BTreeMap<String, RawAreaEffect>,
    /// cards.json `projectiles`: every projectiles row by name, the shape a spell's
    /// `projectile` block carries. Only the rows a unit's CustomFirstProjectile names
    /// (`CardDef::custom_first_projectile`) and the ones a card's DeathSpawnProjectile names
    /// (`convert_death_projectile`) are read, each parsed when it is named, so a row this loader
    /// never reads cannot refuse the file; a card's own Projectile arrives inline. A file without
    /// the table leaves this empty, and a CustomFirstProjectile or a DeathSpawnProjectile that
    /// names a row it cannot find refuses the card by name.
    #[serde(default)]
    projectiles: BTreeMap<String, serde_json::Value>,
    /// cards.json `globals` (15.535 only; tools/extract_cards.py `globals_block`): the named globals.csv rows of
    /// the file's own vintage the loader reads, by name (`CardGlobals`). Absent in the 2018 file and the
    /// fallback set, which then load no card that needs one.
    #[serde(default)]
    globals: BTreeMap<String, serde_json::Value>,
}

/// THE TABLE'S OWN GLOBALS THE LOADER READS (cards.json `globals`), typed. Part of the card fingerprint
/// (state.rs `cards_fingerprint`): a snapshot saved against other globals is stale.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CardGlobals {
    /// MIRROR_LEVEL_OFFSET: how many levels above the Mirror its copy is played (1 in 15.535.29). None when
    /// the file carries no such row: the Mirror card is then refused.
    pub mirror_level_offset: Option<i32>,
    /// The Clone's rules, the eleven CLONE_* rows (`CLONE_GLOBALS`), each None when the file carries no such row:
    /// the Clone card is then refused (`clone_shape`). The three numbers first, then the eight booleans, in
    /// `CLONE_GLOBALS` order.
    pub clone_numbers: [Option<i32>; 3],
    pub clone_flags: [Option<bool>; 8],
}

/// THE CLONE'S GLOBALS (globals.csv, 15.535.29), in `CardGlobals::clone_numbers` then `clone_flags` order.
pub const CLONE_GLOBALS: [&str; 11] = [
    "CLONE_LEVEL_OFFSET",
    "CLONE_DISTANCE_X",
    "CLONE_DISTANCE_Y",
    "CLONE_PRESERVE_SHIELD",
    "CLONE_CLONED_UNITS",
    "CLONE_MOVE_PARENT",
    "CLONE_DEATH_SPAWN_UNITS",
    "CLONE_DEATH_SPAWN_BUILDINGS",
    "CLONE_RESET_TARGET",
    "CLONE_RESET_CHARGE",
    "CLONE_INHERIT_CHARGE",
];

impl CardGlobals {
    /// The typed globals of a cards.json `globals` map. A named row whose value is not the type the loader
    /// reads refuses the whole file: a global is never guessed.
    fn from_map(m: &BTreeMap<String, serde_json::Value>) -> Result<CardGlobals, String> {
        let int = |name: &str| -> Result<Option<i32>, String> {
            match m.get(name) {
                None => Ok(None),
                Some(v) => v.as_i64().and_then(|x| i32::try_from(x).ok()).map(Some).ok_or_else(|| format!("cards.json globals.{name} = {v} is not an integer")),
            }
        };
        let flag = |name: &str| -> Result<Option<bool>, String> {
            match m.get(name) {
                None => Ok(None),
                Some(v) => v.as_bool().map(Some).ok_or_else(|| format!("cards.json globals.{name} = {v} is not a boolean")),
            }
        };
        let mut clone_numbers = [None; 3];
        for (k, slot) in clone_numbers.iter_mut().enumerate() {
            *slot = int(CLONE_GLOBALS[k])?;
        }
        let mut clone_flags = [None; 8];
        for (k, slot) in clone_flags.iter_mut().enumerate() {
            *slot = flag(CLONE_GLOBALS[3 + k])?;
        }
        Ok(CardGlobals { mirror_level_offset: int("MIRROR_LEVEL_OFFSET")?, clone_numbers, clone_flags })
    }

    /// A CLONE_* number by name, None when the file does not carry it (or `name` is not one).
    pub fn clone_number(&self, name: &str) -> Option<i32> {
        CLONE_GLOBALS[..3].iter().position(|n| *n == name).and_then(|k| self.clone_numbers[k])
    }

    /// A CLONE_* boolean by name, None when the file does not carry it (or `name` is not one).
    pub fn clone_flag(&self, name: &str) -> Option<bool> {
        CLONE_GLOBALS[3..].iter().position(|n| *n == name).and_then(|k| self.clone_flags[k])
    }
}

/// WHAT A CONVERTER MAY READ BEYOND ITS OWN ROW: the file's shared tables, by
/// reference. One argument threaded through `convert`, `convert_spell` and
/// `convert_area_effect`, so a row that names another table's row resolves it the
/// same way from every caller. The fallback towers pass their own file's, which is
/// empty.
struct LoadCtx<'a> {
    /// cards.json `area_effect_objects`, by name (`RawCardsFile::area_effect_objects`).
    aeos: &'a BTreeMap<String, RawAreaEffect>,
    /// cards.json `units`, by name (`RawCardsFile::units`): the unit row's `raw` block
    /// carries the columns the typed card record does not (MultipleProjectiles,
    /// CustomFirstProjectile, MultipleTargets, AllTargetsHit), which `convert` reads.
    units: &'a BTreeMap<String, serde_json::Value>,
    /// cards.json `projectiles`, by name (`RawCardsFile::projectiles`).
    projectiles: &'a BTreeMap<String, serde_json::Value>,
    /// cards.json `globals`, typed (`CardGlobals`).
    globals: &'a CardGlobals,
}

/// One cards.json `rarities` entry (tools/extract_cards.py `rarity_table`).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawRarity {
    level_count: Option<i32>,
    relative_level: Option<i32>,
    /// Entry L-1 is the percent at rarity-local level L (entry 0 is 100).
    multiplier_percent_by_level: Option<Vec<i32>>,
}

/// cards.json `level_scaling` block, the fields this loader reads.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawLevelScaling {
    multiplier_percent_by_level: Option<Vec<i32>>,
    base_level: Option<i32>,
    reading: Option<String>,
}

/// The calibration.json combat.STAT_BASE_LEVEL candidates this loader implements
/// (`level_table_of`): the object's own rarity from its local level 1 (the block
/// then carries `base_level`) and the card's rarity from its local level 1 (what an
/// absent `reading` means: the 2018 file, the fallback set).
const LEVEL_BASE_READINGS: [&str; 2] = ["object_rarity_local_1", "card_rarity_local_1"];

/// cards.json projectile object, the fields a SPELL reads (troop attacks read
/// `RawProjectileObj`).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSpellProjectile {
    name: Option<String>,
    speed: Option<i32>,
    damage: Option<i32>,
    crown_tower_damage_percent: Option<i32>,
    radius_milli: Option<i32>,
    aoe_to_air: Option<bool>,
    aoe_to_ground: Option<bool>,
    only_enemies: Option<bool>,
    pushback_milli: Option<i32>,
    pushback_all: Option<bool>,
    maximum_targets: Option<i32>,
    projectile_radius_milli: Option<i32>,
    projectile_radius_y_milli: Option<i32>,
    projectile_range_milli: Option<i32>,
    min_distance_milli: Option<i32>,
    spawn_character: Option<String>,
    spawn_character_count: Option<i32>,
    spawn_character_deploy_time_ms: Option<i32>,
    spawn_character_level_index: Option<i32>,
    spawn_area_effect_object: Option<String>,
    target_buff: Option<serde_json::Value>,
    buff_time_ms: Option<i32>,
    spawn_projectile: Option<Box<RawSpellProjectile>>,
    /// projectiles.csv SpawnCount and Scatter: read on a troop shot's SpawnProjectile row
    /// (`spark_of`). Written by tools/extract_cards.py on the 15.535 rows only.
    spawn_count: Option<i32>,
    scatter: Option<String>,
    action_graph: Option<RawActionGraph>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawBuff {
    name: Option<String>,
    speed_multiplier_raw: Option<i32>,
    hit_speed_multiplier_raw: Option<i32>,
    spawn_speed_multiplier_raw: Option<i32>,
    damage_per_second: Option<i32>,
    hit_frequency_ms: Option<i32>,
    heal_per_second: Option<i32>,
    damage_reduction: Option<i32>,
    damage_multiplier: Option<i32>,
    crown_tower_damage_percent: Option<i32>,
    no_effect_to_crown_towers: Option<bool>,
    building_damage_percent: Option<i32>,
    enable_stacking: Option<bool>,
    attract_percentage: Option<i32>,
    /// character_buffs HitTickFromSource (`BuffDef::hit_tick_from_source`).
    hit_tick_from_source: Option<bool>,
    /// character_buffs ControlledByParent (`BuffDef::controlled_by_parent`); not extracted yet.
    controlled_by_parent: Option<bool>,
    /// character_buffs DeathSpawn and its columns (`BuffDef::death_spawn`); 15.535 only.
    death_spawn: Option<RawBuffDeathSpawn>,
    /// character_buffs IgnoreBuildings (`BuffDef::ignore_buildings`); 15.535 only.
    ignore_buildings: Option<bool>,
    /// character_buffs CrownTowerDamagePerHit (`BuffDef::crown_hit`); 15.535 only.
    crown_tower_damage_per_hit: Option<i32>,
    /// character_buffs Clone and NotCloned (`BuffDef::clone`, `BuffDef::not_cloned`); 15.535 only, written where set.
    clone: Option<bool>,
    not_cloned: Option<bool>,
}

/// cards.json buff `death_spawn` block, every field nullable (the extractor writes the whole block whenever
/// DeathSpawn is set).
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct RawBuffDeathSpawn {
    character: Option<String>,
    count: Option<i32>,
    is_enemy: Option<bool>,
    deploy_delay: Option<bool>,
    same_location: Option<bool>,
    other_buff_death_spawn_allowed: Option<bool>,
}

impl RawBuff {
    /// The columns the engine runs, or the reason it will not run this row. A buff
    /// whose mechanic is DamageReduction / DamageMultiplier / AttractPercentage is
    /// REFUSED rather than loaded without them, so a card can never run as a
    /// weaker card (the loader's rule everywhere else).
    fn convert(&self, what: &str) -> Result<BuffDef, String> {
        let name = self.name.clone().unwrap_or_default();
        // AttractPercentage came off this list on 2026-09-23: it is the Tornado's pull
        // and it is now measured and implemented (status.ATTRACT_LAW, state.rs
        // `phase_path16402`). The other two stay, and the rule behind the list is
        // unchanged -- a card is REFUSED rather than run without a mechanic it carries,
        // so nothing here may be removed before the mechanic exists.
        for (col, set) in [
            ("DamageReduction", self.damage_reduction.is_some()),
            ("DamageMultiplier", self.damage_multiplier.is_some()),
        ] {
            if set {
                return Err(format!("{what}: buff {name} carries {col}, which is not simulated"));
            }
        }
        let def = BuffDef {
            speed_pct: self.speed_multiplier_raw.unwrap_or(0),
            hit_speed_pct: self.hit_speed_multiplier_raw.unwrap_or(0),
            spawn_speed_pct: self.spawn_speed_multiplier_raw.unwrap_or(0),
            damage_per_second: self.damage_per_second.unwrap_or(0),
            heal_per_second: self.heal_per_second.unwrap_or(0),
            hit_frequency_ms: self.hit_frequency_ms.unwrap_or(0),
            crown_pct: crown(self.crown_tower_damage_percent),
            building_pct: self.building_damage_percent.unwrap_or(PERCENT_I32),
            no_effect_to_crown_towers: self.no_effect_to_crown_towers.unwrap_or(false),
            enable_stacking: self.enable_stacking.unwrap_or(false),
            attract_pct: self.attract_percentage.unwrap_or(0),
            hit_tick_from_source: self.hit_tick_from_source.unwrap_or(false),
            controlled_by_parent: self.controlled_by_parent.unwrap_or(false),
            death_spawn: match &self.death_spawn {
                None => None,
                Some(ds) => Some(buff_death_spawn(ds, &name, what)?),
            },
            ignore_buildings: self.ignore_buildings.unwrap_or(false),
            crown_hit: self.crown_tower_damage_per_hit.unwrap_or(0),
            clone_hold: self.clone.unwrap_or(false),
            not_cloned: self.not_cloned.unwrap_or(false),
        };
        // CrownTowerDamagePerHit replaces a PULSE's crown-tower damage (state.rs `buff_pulse_pass`); on a buff that
        // does not pulse it would have nothing to replace.
        if def.crown_hit != 0 && !def.pulses() {
            return Err(format!("{what}: buff {name} carries CrownTowerDamagePerHit {} but deals no damage over time; not simulated", def.crown_hit));
        }
        if def.crown_hit < 0 {
            return Err(format!("{what}: buff {name}: CrownTowerDamagePerHit {} is negative", def.crown_hit));
        }
        if def.is_inert() {
            // A row with no multiplier, no damage and no heal is a MARKER (Invisible,
            // Clone, a filter) whose mechanic lives in an action graph.
            return Err(format!("{what}: buff {name} has no multiplier, damage or heal: its mechanic is not in the columns"));
        }
        if def.pulses() && def.hit_frequency_ms <= 0 {
            return Err(format!("{what}: buff {name} pulses damage or healing with no HitFrequency"));
        }
        Ok(def)
    }
}

/// A buff row's death spawn (`BuffDeathSpawn`), its unit still unresolved (u16::MAX: `BuffTable::set_death_unit`
/// fills it once the loader has loaded the unit), or the reason it is refused. Accepted: one unit (a blank
/// DeathSpawnCount reads one, as a unit's does), with OtherBuffDeathSpawnAllowed set, so the carrier's own death spawn
/// still happens beside it (the engine runs both, state.rs `phase_reap`).
fn buff_death_spawn(ds: &RawBuffDeathSpawn, name: &str, what: &str) -> Result<BuffDeathSpawn, String> {
    let unit = ds.character.as_deref().filter(|c| !c.is_empty()).ok_or_else(|| format!("{what}: buff {name}: a death spawn with no DeathSpawn"))?;
    let count = ds.count.unwrap_or(1);
    if count != 1 {
        return Err(format!("{what}: buff {name}: a buff death spawn of {count} units ({unit}) has no measured layout; not simulated"));
    }
    if ds.other_buff_death_spawn_allowed != Some(true) {
        return Err(format!("{what}: buff {name}: without OtherBuffDeathSpawnAllowed the carrier's own death spawn would be suppressed; not simulated"));
    }
    Ok(BuffDeathSpawn {
        unit: u16::MAX,
        count,
        for_other_side: ds.is_enemy.unwrap_or(false),
        deploy_delay: ds.deploy_delay.unwrap_or(false),
        same_location: ds.same_location.unwrap_or(false),
    })
}

/// A buff table built while the cards load: every distinct `BuffDef` the file uses,
/// once. `CardDb::buffs` is this vector, and every `BuffApply` (on a card, a
/// projectile or a spell) is an index into it.
#[derive(Default)]
pub(crate) struct BuffTable {
    defs: Vec<BuffDef>,
    /// Every name that interned to each index, in load order. Several, where two buffs
    /// carry identical columns: `intern` merges by VALUE, so one index can stand for
    /// more than one row of the card data.
    names: Vec<Vec<String>>,
    /// The NAME of each index's death-spawn unit (`BuffDef::death_spawn`), for the loader to load it
    /// (`death_needs`, `set_death_unit`); None on every buff without one. Part of an index's identity:
    /// two rows whose columns agree but whose death units differ never merge.
    death_names: Vec<Option<String>>,
}

/// `def` with its death-spawn unit set to the unresolved u16::MAX: what `BuffTable::intern` compares, so a row
/// interned after its death unit was resolved still finds the index of the same row interned before.
fn death_unit_masked(def: &BuffDef) -> BuffDef {
    let mut d = *def;
    if let Some(ds) = d.death_spawn.as_mut() {
        ds.unit = u16::MAX;
    }
    d
}

impl BuffTable {
    /// Whether buff `idx` stops the walk: its SpeedMultiplier composes to 0. That is the three -100 columns of a
    /// stun and a freeze, and also the Ronin's counter stun (speed -100, hit speed -95), which stops the walk and
    /// leaves the attack clock running (status.FULL_STOP_BUFF_IS_STUN). The own-side area refusal reads only the walk.
    fn stops(&self, idx: u16) -> bool {
        crate::status::compose([self.defs[idx as usize]].iter(), crate::status::Sel::Speed, 100) == 0
    }

    /// The definitions, and for each one EVERY name that interned to it joined with
    /// `|`. Joined rather than reduced to the first, because the first would silently
    /// rename the rest: a viewer told `Freeze` for an index that is also `ZapFreeze`
    /// draws the second as the first and nothing says so.
    fn into_parts(self) -> (Vec<BuffDef>, Vec<String>) {
        let names = self.names.into_iter().map(|n| n.join("|")).collect();
        (self.defs, names)
    }

    fn intern(&mut self, def: BuffDef, name: &str, death: Option<&str>) -> Result<u16, String> {
        let key = death_unit_masked(&def);
        #[cfg(not(clash_plant = "buff_interns_without_death_unit"))]
        let found = (0..self.defs.len()).find(|&k| death_unit_masked(&self.defs[k]) == key && self.death_names[k].as_deref() == death);
        #[cfg(clash_plant = "buff_interns_without_death_unit")]
        let found = {
            let _ = death; // PLANT (regression): two rows whose death units differ merge into one index.
            (0..self.defs.len()).find(|&k| death_unit_masked(&self.defs[k]) == key)
        };
        if let Some(i) = found {
            if !name.is_empty() && !self.names[i].iter().any(|n| n == name) {
                self.names[i].push(name.to_string());
            }
            return Ok(i as u16);
        }
        if self.defs.len() >= u16::MAX as usize {
            return Err("more distinct buffs than the table can index".into());
        }
        self.defs.push(def);
        self.names.push(if name.is_empty() { Vec::new() } else { vec![name.to_string()] });
        self.death_names.push(death.map(str::to_string));
        Ok((self.defs.len() - 1) as u16)
    }

    /// Intern `raw` with `time_ms` as a `BuffApply`, refusing a buff with no time.
    fn apply(&mut self, raw: &RawBuff, time_ms: Option<i32>, what: &str) -> Result<BuffApply, String> {
        let def = raw.convert(what)?;
        let time_ms = time_ms.filter(|t| *t > 0).ok_or_else(|| format!("{what}: buff {} without BuffTime", raw.name.clone().unwrap_or_default()))?;
        let death = raw.death_spawn.as_ref().and_then(|d| d.character.as_deref());
        Ok(BuffApply { buff: self.intern(def, raw.name.as_deref().unwrap_or(""), death)?, time_ms })
    }

    /// The death-spawn units the buffs `used` release, as (buff index, unit name), each index once: the loader's
    /// needs for a card that can hang those buffs (`CardDb::from_json_str`).
    fn death_needs(&self, used: &[u16]) -> Vec<(u16, String)> {
        let mut out: Vec<(u16, String)> = Vec::new();
        for &b in used {
            if let Some(Some(n)) = self.death_names.get(b as usize) {
                if !out.iter().any(|(k, _)| *k == b) {
                    out.push((b, n.clone()));
                }
            }
        }
        out
    }

    /// Buff `b`'s death spawn releases the loaded unit `u`.
    fn set_death_unit(&mut self, b: u16, u: u16) {
        if let Some(ds) = self.defs.get_mut(b as usize).and_then(|d| d.death_spawn.as_mut()) {
            ds.unit = u;
        }
    }

    /// Whether buff `b` has a death spawn whose unit was never loaded.
    fn death_unresolved(&self, b: u16) -> bool {
        self.defs.get(b as usize).and_then(|d| d.death_spawn).is_some_and(|ds| ds.unit == u16::MAX)
    }

    /// Every death spawn whose unit was never loaded is dropped: no loaded card hangs that buff (a card that does
    /// was refused already, `CardDb::from_json_str`), so nothing can reach it.
    fn drop_unresolved_deaths(&mut self) {
        for d in &mut self.defs {
            if d.death_spawn.is_some_and(|ds| ds.unit == u16::MAX) {
                d.death_spawn = None;
            }
        }
    }

    /// Every buff index some name of `name`'s row interned to.
    fn indices_named(&self, name: &str) -> Vec<u16> {
        (0..self.names.len()).filter(|&k| self.names[k].iter().any(|n| n == name)).map(|k| k as u16).collect()
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawAreaEffect {
    name: Option<String>,
    life_duration_ms: Option<i32>,
    radius_milli: Option<i32>,
    hit_speed_ms: Option<i32>,
    damage: Option<i32>,
    crown_tower_damage_percent: Option<i32>,
    no_effect_to_crown_towers: Option<bool>,
    buff: Option<RawBuff>,
    buff_time_ms: Option<i32>,
    only_enemies: Option<bool>,
    only_own_troops: Option<bool>,
    hits_ground: Option<bool>,
    hits_air: Option<bool>,
    ignore_buildings: Option<bool>,
    pushback_milli: Option<i32>,
    maximum_targets: Option<i32>,
    projectile: Option<serde_json::Value>,
    spawn_character: Option<String>,
    action_graph: Option<RawActionGraph>,
    /// area_effect_objects CapBuffTimeToAreaEffectTime (`SpellHit::caps_buff_time`).
    cap_buff_time_to_area_effect_time: Option<bool>,
    /// area_effect_objects ControlsBuff (`SpellHit::controls_buff`); not extracted yet.
    controls_buff: Option<bool>,
    /// SpawnAreaEffectObject: the NAME of the one-shot area this area makes on its first update.
    spawn_area_effect_object: Option<String>,
    /// BuffNumber: how many of the buff one application stacks (1, or blank, on every row read).
    buff_number: Option<i32>,
    /// HitBiggestTargets (`StrikeDef`; 15.535 only).
    hit_biggest_targets: Option<bool>,
    /// What the area's OnStartingAction runs, in order (tools/extract_cards.py `action_schedule`; 15.535 only): read
    /// by `area_spawns_area` (the Goblin Curse's area makes its curse circle).
    schedule: Option<RawSchedule>,
    /// What the area's OnHitAction runs on each unit it hits (15.535 only): read by `on_hit_buffs` (the curse
    /// circle hangs two buffs).
    on_hit: Option<RawSchedule>,
    /// SpawnTime: how long the unit the area's projectile releases deploys, as the area row says it (the Royal
    /// Delivery's 250; `centre_strike_shape` holds it against the projectile's SpawnCharacterDeployTime).
    spawn_time_ms: Option<i32>,
    /// A striking area whose strikes are an action (tools/extract_cards.py `strike_area_block`; 15.535 only, written
    /// where the row is one): the Vines' ranked catches, the Void's laser ball (`strike_area_shape`).
    strike_area: Option<RawStrikeArea>,
    /// area_effect_objects Clone, and the ActionClone its OnHitAction runs (tools/extract_cards.py
    /// `clone_action_block`; 15.535 only, written where set): `clone_shape`.
    clone: Option<bool>,
    clone_action: Option<RawCloneAction>,
}

/// cards.json `strike_area` (tools/extract_cards.py `strike_area_block`), every field nullable: the fields of both
/// kinds, each read by its own kind.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawStrikeArea {
    kind: Option<String>,
    start_delay_ms: Option<i32>,
    filter: Option<RawTargetFilter>,
    // ranked_catches
    catch_offsets_ms: Option<Vec<i32>>,
    once_per_target: Option<bool>,
    selection_mode: Option<String>,
    radius_milli: Option<i32>,
    air_to_ground: Option<RawAirToGround>,
    options: Option<Vec<RawBuff>>,
    option_time_ms: Option<Vec<i32>>,
    // laser_ball
    first_hit_delay_ms: Option<i32>,
    hit_frequency_ms: Option<i32>,
    detection_radius_milli: Option<i32>,
    max_units_per_list: Option<Vec<i32>>,
    tiers: Option<Vec<RawStrikeTier>>,
}

/// One tier of a laser ball: its buff, the buff's SpawnTime and AddAsIndividualBuff.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawStrikeTier {
    buff: Option<RawBuff>,
    time_ms: Option<i32>,
    add_as_individual_buff: Option<bool>,
}

/// A catch's ActionAirToGround (cards.json `strike_area.air_to_ground`).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawAirToGround {
    transition_ms: Option<i32>,
    total_ms: Option<i32>,
    abort_if_instigator_dies: Option<bool>,
    singleton: Option<bool>,
    allow_is_ground_tag_on_idle: Option<bool>,
}

/// A target filter (game_object_filters.toml; cards.json `strike_area.filter`), every flag nullable: a blank flag
/// filters nothing (spells.TARGET_FILTER_ABSENT_FLAG). The block's filter_if_no_hitpoint_component is not read: every
/// entity the engine makes has hitpoints.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawTargetFilter {
    name: Option<String>,
    match_team_enemy: Option<bool>,
    match_team_own: Option<bool>,
    match_type_characters: Option<bool>,
    filter_buildings: Option<bool>,
    filter_summoner: Option<bool>,
    filter_princess_towers: Option<bool>,
    filter_underground: Option<bool>,
    filter_hidden: Option<bool>,
    filter_invisible: Option<bool>,
    filter_flying: Option<bool>,
    filter_cloning: Option<bool>,
    filter_dash_immune: Option<bool>,
    tags: Option<Vec<String>>,
}

/// cards.json `clone_action` (tools/extract_cards.py `clone_action_block`). Its card_data_for_stats (the card whose
/// stat rows the client shows) is not read.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawCloneAction {
    on_cloned: Option<RawOnCloned>,
}

/// What the ActionClone runs on each unit it copies (its OnClonedAction, one ActionSpawn).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawOnCloned {
    spawn_type: Option<String>,
    spawn: Option<String>,
    buff: Option<RawBuff>,
    spawn_time_ms: Option<i32>,
}

/// cards.json `schedule` / `on_hit` (tools/extract_cards.py `action_schedule`): the entries an action runs, in order.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSchedule {
    root: Option<String>,
    entries: Vec<RawScheduleEntry>,
}

/// One entry of a `RawSchedule` (tools/extract_cards.py `schedule_entry`), every field nullable.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawScheduleEntry {
    delay_ms: Option<i32>,
    class: Option<String>,
    /// The class only plays an effect, a sound or an animation.
    cosmetic: Option<bool>,
    /// An ActionSpawn's SpawnType (AreaEffectType, BuffType, CharacterType) and SpawnData.
    spawn_type: Option<String>,
    spawn: Option<String>,
    /// An ActionSpawn's SpawnTime, ms: a spawned buff's time.
    spawn_time_ms: Option<i32>,
    /// A BuffType spawn's buff row.
    buff: Option<RawBuff>,
    /// Every class and column the extractor's reader does not understand.
    unread: Vec<String>,
    /// The action's own name (None for an inline table): named in a scheduled area's refusals.
    action: Option<String>,
    /// UseDeploy: the unit a scheduled area puts down deploys (`scheduled_area` takes no entry without it).
    use_deploy: Option<bool>,
    /// DeployTime, ms: how long that unit deploys, in place of its own DeployTime (the Graveyard's 500).
    deploy_time_ms: Option<i32>,
    /// XPositionExpression / YPositionExpression, each read into one form (tools/extract_cards.py `X_EXPR`, `Y_EXPR`).
    x: Option<RawPosExpr>,
    y: Option<RawPosExpr>,
    /// RelativeX / RelativeY (the Suspicious Bush's goblins).
    relative: Option<RawRelative>,
}

impl RawScheduleEntry {
    fn is_cosmetic(&self) -> bool {
        self.cosmetic.unwrap_or(false)
    }
}

/// One position expression of a schedule entry (`RawScheduleEntry::x`, `y`): its form and its offset, millitiles.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawPosExpr {
    form: Option<String>,
    offset_milli: Option<i32>,
}

/// A schedule entry's RelativeX / RelativeY, raw.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawRelative {
    x: Option<i32>,
    y: Option<i32>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSpell {
    first_projectile: Option<RawSpellProjectile>,
    area_effect_object: Option<RawAreaEffect>,
    spell_as_deploy: Option<bool>,
    can_place_on_buildings: Option<bool>,
    can_deploy_on_enemy_side: Option<bool>,
    projectile_waves: Option<i32>,
    projectile_wave_interval_ms: Option<i32>,
    duration_seconds: Option<i32>,
    /// spells_other Radius: the spell's own area (the volley's coverage).
    radius_milli: Option<i32>,
    /// spells_other MultipleProjectiles: arrows per wave (Arrows: 15 in 2018, 10 in 15.535).
    multiple_projectiles: Option<i32>,
    /// spells_other SummonCharacter / SummonNumber, on a spell with no projectile and no area of its
    /// own (Rage's bottle, the Heal Spirit; 15.535 only).
    summon: Option<RawSummon>,
    /// spells_other Mirror (15.535 only; written only on the Mirror's row): `SpellShape::Mirror`.
    mirror: Option<bool>,
    /// spells_other Options on a LogicBattleSpellVariantData row (15.535 only; the Spirit Empress):
    /// `SpellShape::Variant`.
    variant: Option<RawVariant>,
}

/// cards.json `spell.summon`.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawSummon {
    character: Option<String>,
    count: Option<i32>,
}

/// cards.json `spell.variant` (tools/extract_cards.py `variant_block`).
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawVariant {
    options: Vec<RawVariantOption>,
    use_projected_time_summon: Option<bool>,
    mirror_uses_root_spell: Option<bool>,
}

/// cards.json `spell.variant.options[k]`, every field nullable.
#[derive(Deserialize, Default)]
#[serde(default)]
struct RawVariantOption {
    trigger_milli: Option<i32>,
    precast_pending_ms: Option<i32>,
    card: Option<String>,
}

/// The most options a variant card may carry (the Spirit Empress ships 2).
const MAX_VARIANT_OPTIONS: usize = 4;

/// THE VARIANT CARD (`SpellShape::Variant`) a `spell.variant` block describes, and its forms as unit needs
/// (`UnitUse::VariantForm`), or the reason it is refused. Accepted: the form chosen at the play (UseProjectedTimeSummon
/// TRUE), a Mirror copying the played form (MirrorUsesRootSpell FALSE), 1 to MAX_VARIANT_OPTIONS options whose
/// triggers strictly descend, each with a trigger, a PrecastPendingTime and a form. Strictly descending makes "the first
/// option whose trigger is met" and "the highest trigger met" one rule.
fn convert_variant(mut def: CardDef, v: RawVariant) -> Result<(CardDef, UnitNeeds), String> {
    if v.use_projected_time_summon != Some(true) {
        return Err("a variant card whose form is not chosen at the play (UseProjectedTimeSummon not TRUE) is not simulated".into());
    }
    if v.mirror_uses_root_spell != Some(false) {
        return Err("a variant card a Mirror replays as the root card (MirrorUsesRootSpell not FALSE) is not simulated".into());
    }
    if v.options.is_empty() || v.options.len() > MAX_VARIANT_OPTIONS {
        return Err(format!("a variant card with {} options is not simulated", v.options.len()));
    }
    let (mut options, mut needs, mut prev) = (Vec::new(), Vec::new(), i32::MAX);
    for (k, o) in v.options.into_iter().enumerate() {
        let t = o.trigger_milli.filter(|t| *t > 0).ok_or_else(|| format!("variant option {k} without AvailableManaTrigger"))?;
        if t >= prev {
            return Err(format!("variant option {k}: triggers not strictly descending ({prev}, {t})"));
        }
        prev = t;
        let pre = o.precast_pending_ms.filter(|p| *p >= 0).ok_or_else(|| format!("variant option {k} without PrecastPendingTime"))?;
        let name = o.card.filter(|n| !n.is_empty()).ok_or_else(|| format!("variant option {k} without SpellData"))?;
        options.push(VariantOption { trigger_milli: t, precast_pending_ms: pre, card: u16::MAX });
        needs.push((UnitUse::VariantForm(k as u8), name));
    }
    def.spell = Some(SpellDef { shape: SpellShape::Variant { options }, placement: SpellPlacement::Anywhere });
    Ok((def, needs))
}

/// One row group of rarities.csv.
#[derive(Clone, Debug)]
pub struct RarityRow {
    pub name: String,
    pub level_count: i32,
    pub relative_level: i32,
    /// PowerLevelMultiplier column, percent. Entry i scales local level i + 2.
    pub multipliers: Vec<i32>,
}

/// Minimal RFC-4180-ish splitter: Supercell CSVs quote strings and never embed
/// newlines, which is all this needs to handle.
fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = line.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if in_q && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => in_q = !in_q,
            ',' if !in_q => out.push(std::mem::take(&mut cur)),
            _ => cur.push(ch),
        }
    }
    out.push(cur);
    out
}

/// Parse a Supercell CSV: header row, type row, then data rows. Returns
/// (header, rows).
pub fn parse_supercell_csv(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let mut lines = text.lines().map(|l| l.trim_start_matches('\u{feff}')).filter(|l| !l.trim().is_empty());
    let header = lines.next().map(split_csv_line).unwrap_or_default();
    let _types = lines.next();
    (header, lines.map(split_csv_line).collect())
}

pub fn parse_rarities(text: &str) -> Result<Vec<RarityRow>, String> {
    let (h, rows) = parse_supercell_csv(text);
    let col = |n: &str| h.iter().position(|c| c == n).ok_or_else(|| format!("rarities.csv: no column {n}"));
    let (c_name, c_count, c_rel, c_mult) =
        (col("Name")?, col("LevelCount")?, col("RelativeLevel")?, col("PowerLevelMultiplier")?);
    let mut out: Vec<RarityRow> = Vec::new();
    for r in rows {
        let get = |i: usize| r.get(i).map(|s| s.trim()).unwrap_or("");
        if !get(c_name).is_empty() {
            let num = |i: usize| get(i).parse::<i32>().map_err(|e| format!("rarities.csv {}: {e}", get(c_name)));
            out.push(RarityRow {
                name: get(c_name).to_string(),
                level_count: num(c_count)?,
                relative_level: num(c_rel)?,
                multipliers: Vec::new(),
            });
        }
        let m = get(c_mult);
        if !m.is_empty() {
            let v = m.parse::<i32>().map_err(|e| format!("rarities.csv multiplier: {e}"))?;
            out.last_mut().ok_or("rarities.csv: multiplier before any rarity")?.multipliers.push(v);
        }
    }
    Ok(out)
}

/// The shipped rarity table, parsed once.
pub fn shipped_rarities() -> &'static [RarityRow] {
    static CELL: OnceLock<Vec<RarityRow>> = OnceLock::new();
    CELL.get_or_init(|| parse_rarities(RARITIES_CSV).expect("shipped rarities.csv must parse"))
}

#[derive(Clone, Debug)]
pub struct CardDb {
    pub cards: Vec<CardDef>,
    /// EVERY DISTINCT BUFF the file uses, once (status.rs `BuffDef`); a `BuffApply`
    /// anywhere -- `CardDef::attack_buff`, `SpellHit::buff` -- is an index into it,
    /// and so is `BuffSlot::id` minus one. Part of the card fingerprint
    /// (`state.rs fingerprint_debug`): a snapshot saved against different buff
    /// numbers is stale, exactly as one saved against different card numbers is.
    pub buffs: Vec<BuffDef>,
    /// THE NAMES OF `buffs`, index for index, as the card data spells them. NOT one
    /// name per buff: the loader interns by VALUE, so buffs whose columns are identical
    /// share an index, and this holds every name that landed there joined with `|`
    /// (`Freeze|ZapFreeze`). Display only, and deliberately NOT part of the card
    /// fingerprint: a name changes nothing the engine does, so it must not stale a
    /// snapshot.
    pub buff_names: Vec<String>,
    by_name: BTreeMap<String, u16>,
    pub source: CardSource,
    /// Cards present in the input the engine cannot simulate, with why.
    pub rejected: Vec<(String, String)>,
    /// True when the input lacked KingTower/PrincessTower and fallback tower
    /// definitions were appended.
    pub towers_from_fallback: bool,
    rarities: Vec<RarityRow>,
    /// The file's `version`: "cards-15535.1" (cards.json), "cards-2018.1" (cards-2018.json),
    /// "fallback", or empty when the file names none. A ledger overlay that corrects one table
    /// checks it before it applies (calibration cards.CLIENT16402_VALUES value.table; state.rs
    /// `with_card_values`). Not part of the card fingerprint: the values are.
    pub version: String,
    /// The file's own globals the loader reads (`CardGlobals`; cards.json `globals`).
    pub globals: CardGlobals,
}

pub const KING_TOWER: &str = "KingTower";
pub const PRINCESS_TOWER: &str = "PrincessTower";

/// A CardDef with every stat zeroed, for spells (which have no unit of their own).
fn stat_less(name: String, rarity: String, elixir: i32) -> CardDef {
    CardDef {
        unit_name: name.clone(),
        name,
        kind: CardKind::Spell,
        elixir,
        rarity,
        hitpoints: 0,
        damage: 0,
        hit_speed_ms: 0,
        load_time_ms: 0,
        speed: 0,
        range: 0,
        sight_range: 0,
        collision_radius: 0,
        mass: None,
        deploy_time_ms: 0,
        attacks_air: false,
        attacks_ground: false,
        target_only_buildings: false,
        flying_height: 0,
        area_damage_radius: 0,
        projectile: None,
        count: 1,
        shield_hitpoints: 0,
        crown_tower_damage_percent: 100,
        death_damage: 0,
        death_damage_radius: 0,
        self_as_aoe_center: false,
        lifetime_ms: None,
        level_table: None,
        no_deploy_size: None,
        ignore_pushback: false,
        spell: None,
        summon_only: false,
        stop_movement_after_ms: 0,
        wait_ms: 0,
        hide: None,
        spawner: None,
        death_spawn: None,
        charge: None,
        jump: None,
        level_base: None,
        formation: FormationDef::default(),
        projectile_start_radius: 0,
        kamikaze: false,
        attack_buff: None,
        projectile_homing: false,
        death_area_effect: None,
        death_spawn_pushback: false,
        dash: None,
        reflect: None,
        range_shot: None,
        multiple_projectiles: 1,
        custom_first_projectile: None,
        multiple_targets: 1,
        all_targets_hit: false,
        deploy_projectile: None,
        load_first_hit: false,
        variable_damage: None,
        attack_pushback: 0,
        special: None,
        death_projectile: None,
        deploy_area_effect: None,
        spawn_area_effect: None,
        hovering: false,
        minimum_range: 0,
        spark: None,
        projectile_area: None,
        life_state: None,
        invisible_when_idle: None,
        spawn_pathfind: None,
        can_deploy_on_enemy_side: false,
        mana: None,
        omit_from_starting_hand: false,
        attach: None,
        target_only_troops: false,
        deprioritize_buff: None,
        summon_members: None,
        summon_offsets_x_mirrored: false,
        attack_select: None,
        ignore_buffs: Vec::new(),
        attack_buff_first: false,
        enchant: None,
        transform_at_hp: None,
        parry: None,
        kamikaze_time_ms: 0,
        death_pushback: 0,
        ignore_clone: false,
    }
}

/// Effective crown percent from a cards.json object: the extractor already wrote
/// the effective value (100 + negative raw); blank means 100.
fn crown(v: Option<i32>) -> i32 {
    v.unwrap_or(100)
}

fn knockback(pushback_milli: Option<i32>, all: Option<bool>) -> Option<KnockbackDef> {
    match pushback_milli {
        Some(d) if d > 0 => Some(KnockbackDef { distance: milli(d), all: all.unwrap_or(false) }),
        _ => None,
    }
}

/// ONE AREA EFFECT, whatever puts it on the ground. Two things release one in this
/// data: a spell card's AreaEffectObject (Zap, Freeze, Poison) and a unit's
/// DeathAreaEffect (the Ice Golem's FreezeIceGolemite). The record is the same
/// `area_effect_objects` row either way, so it is read once, here, and both callers
/// get the same `SpellShape` and the same refusals -- an area the loader cannot run
/// as a spell it cannot run as a death release either.
///
/// Accepts exactly the two shapes `SpellShape` implements (a one-shot disc and a
/// pulsing one) and REFUSES every other area with the reason, so an area whose
/// mechanic is not simulated can never run as a simpler one.
///
/// Returns the shape and the units it needs loaded, like every converter (neither
/// accepted shape releases one, so the list is empty).
fn convert_area_effect(aeo: &RawAreaEffect, buffs: &mut BuffTable, ctx: &LoadCtx) -> Result<(SpellShape, UnitNeeds), String> {
    // AN AREA WHOSE ONE ACTION MAKES ANOTHER AREA (the Goblin Curse) is recognised first: its action is what it does.
    if let Some(got) = area_spawns_area(aeo, buffs, ctx) {
        return got;
    }
    // A STRIKING AREA WHOSE STRIKES ARE AN ACTION (the Vines, the Void; `strike_area_shape`): the extractor writes the
    // block only for a row whose action it read whole, so the block, not the action graph, says what the area does.
    if let Some(sa) = &aeo.strike_area {
        return strike_area_shape(aeo, sa, buffs).map(|d| (SpellShape::Strikes(Box::new(d)), Vec::new()));
    }
    // THE CLONE (`clone_shape`), the same way.
    if let Some(got) = clone_shape(aeo, buffs, ctx) {
        return got;
    }
    // AN AREA WHOSE ONE ACTION PUTS DOWN A BOTTLE (the Lumberjack's death area) is that bottle's fuse.
    if let Some(got) = area_spawns_bottle(aeo, buffs, ctx) {
        return got;
    }
    // AN AREA WHOSE ACTIONS PUT UNITS DOWN ON A SCHEDULE (the Graveyard, the Suspicious Bush's death area).
    if let Some(got) = scheduled_area(aeo) {
        return got;
    }
    let what = aeo.name.clone().unwrap_or_default();
    refuse_action_mechanic_but_on_hit(aeo, &format!("area effect {what}"))?;
    area_effect_shape(aeo, buffs, ctx)
}

/// AN AREA THAT LANDS NOTHING OF ITS OWN: it hits neither ground nor air, and it carries no Buff, Pushback,
/// Projectile, SpawnCharacter, MaximumTargets, child area, positive HitSpeed or HitBiggestTargets. Its own Damage is
/// not read here (`inert_own_damage`). The clause every area whose action is its mechanic must meet
/// (`area_spawns_area`, `area_spawns_bottle`, `scheduled_area`).
fn area_lands_nothing(aeo: &RawAreaEffect) -> bool {
    !aeo.hits_ground.unwrap_or(false)
        && !aeo.hits_air.unwrap_or(false)
        && aeo.buff.is_none()
        && aeo.pushback_milli.is_none()
        && aeo.projectile.as_ref().map_or(true, serde_json::Value::is_null)
        && aeo.spawn_character.is_none()
        && aeo.maximum_targets.is_none()
        && aeo.spawn_area_effect_object.is_none()
        && aeo.hit_speed_ms.map_or(true, |h| h <= 0)
        && aeo.hit_biggest_targets != Some(true)
}

/// spells.AREA_DAMAGE_WITHOUT_HIT_FLAGS = inert: an area that hits neither ground nor air deals no Damage of its own.
/// True for an area with a Damage column and neither hit flag (the Suspicious Bush's death area, Damage 100). Measured
/// on client 15.535.29: that area hurt nothing, an enemy 1067 from the death point and a tower 3075 away, in 9 runs.
fn inert_own_damage(aeo: &RawAreaEffect) -> bool {
    #[cfg(not(clash_plant = "bush_dummy_damage_loaded"))]
    let inert = aeo.damage.is_some() && !aeo.hits_ground.unwrap_or(false) && !aeo.hits_air.unwrap_or(false);
    #[cfg(clash_plant = "bush_dummy_damage_loaded")]
    let inert = {
        let _ = aeo;
        false // PLANT: the Damage column is read as damage the area deals, and the area is refused.
    };
    inert
}

/// Is the area's action graph exactly what its schedule reads: rooted only at the schedule's OnStartingAction, of
/// the classes the schedule's entries carry (and ActionGroup), spawning only what those entries spawn? A graph with
/// anything more (an OnHitAction, a class the schedule does not list) is not the schedule, and the area is refused.
fn graph_is_its_schedule(aeo: &RawAreaEffect, sched: &RawSchedule) -> bool {
    let Some(g) = &aeo.action_graph else { return true };
    let root_ok = g.roots.iter().all(|(k, v)| k == "OnStartingAction" && sched.root.as_deref() == Some(v.as_str()));
    let classes: Vec<&str> = sched.entries.iter().filter_map(|e| e.class.as_deref()).collect();
    let class_ok = g.class_types.iter().all(|c| c == "ActionGroup" || classes.contains(&c.as_str()));
    let spawns: Vec<String> = sched
        .entries
        .iter()
        .filter(|e| !e.is_cosmetic())
        .filter_map(|e| Some(format!("{}:{}", e.spawn_type.as_deref()?, e.spawn.as_deref()?)))
        .collect();
    let spawn_ok = g.spawns.iter().all(|s| spawns.contains(s));
    root_ok && class_ok && spawn_ok
}

/// THE BOTTLE a `units` row is (`Hitpointless::Bottle`: Rage's RageBottle, the Lumberjack's RageBarbarianBottle): its
/// fuse (the row's DeployTime) and the name of the area its death leaves. None for any other row. The one classifier
/// both paths that turn a bottle into a `SpellShape::Fuse` read (a spell's summon, `CardDb::from_json_str`; an area's
/// spawn, `area_spawns_bottle`), so the two cannot drift.
fn bottle_of(units: &BTreeMap<String, serde_json::Value>, name: &str) -> Option<(i32, String)> {
    let mut v = units.get(name)?.clone();
    let kind = match v.get("source_table").and_then(|t| t.as_str()) {
        Some("buildings") => "building",
        _ => "troop",
    };
    if let Some(obj) = v.as_object_mut() {
        obj.insert("kind".into(), serde_json::Value::String(kind.into()));
        obj.entry("count").or_insert(serde_json::Value::from(1));
    }
    match hitpointless_building(&serde_json::from_value::<RawCard>(v).ok()?)? {
        Hitpointless::Bottle { fuse_ms, area } => Some((fuse_ms, area)),
        Hitpointless::DeathBomb { .. } => None,
    }
}

/// AN AREA WHOSE ONE ACTION PUTS DOWN A BOTTLE (the Lumberjack's death area RageBarbarianDummyForSpawn: LifeDuration
/// 50, no hit, one ActionSpawn of the bottle row RageBarbarianBottle, which is a DeployTime of 500 and the death area
/// BarbarianRage). The area is COLLAPSED into the bottle's fuse over the bottle's area (`SpellShape::Fuse { fuse_ms,
/// then }`), the shape the Rage's bottle already is: no RageBarbarianBottle record is loaded. When a death leaves it,
/// the fuse starts under calibration spells.DEATH_FUSE_START (state.rs `phase_reap`).
///
/// None, and the caller's refusal stands, unless ALL hold:
///   1. the area's schedule has exactly one entry that is not cosmetic: an ActionSpawn of a CharacterType at delay 0,
///      with no position and nothing unread;
///   2. the area lands nothing (`area_lands_nothing`; a Damage column must be inert, `inert_own_damage`) and names no
///      OnHitAction, and its action graph is its schedule (`graph_is_its_schedule`);
///   3. the spawned row is a bottle (`bottle_of`).
///
/// Then the bottle's area is read by the non-collapsing path and must be one the engine runs.
fn area_spawns_bottle(aeo: &RawAreaEffect, buffs: &mut BuffTable, ctx: &LoadCtx) -> Option<Result<(SpellShape, UnitNeeds), String>> {
    // PLANT death_bottle_unread (tests/lumberjack.rs): the area stays an action graph the loader does not read.
    #[cfg(clash_plant = "death_bottle_unread")]
    if aeo.schedule.is_some() {
        return None;
    }
    let sched = aeo.schedule.as_ref()?;
    let live: Vec<&RawScheduleEntry> = sched.entries.iter().filter(|e| !e.is_cosmetic()).collect();
    let [e] = live.as_slice() else { return None };
    let positioned = e.x.is_some() || e.y.is_some() || e.relative.is_some();
    if e.class.as_deref() != Some("ActionSpawn") || e.spawn_type.as_deref() != Some("CharacterType") || e.delay_ms.unwrap_or(0) != 0 || !e.unread.is_empty() || positioned {
        return None;
    }
    if !area_lands_nothing(aeo) || (aeo.damage.is_some() && !inert_own_damage(aeo)) || aeo.on_hit.is_some() || !graph_is_its_schedule(aeo, sched) {
        return None;
    }
    let unit = e.spawn.clone()?;
    let (fuse_ms, area) = bottle_of(ctx.units, &unit)?;
    let what = aeo.name.clone().unwrap_or_default();
    let got = (|| {
        let c = ctx.aeos.get(&area).ok_or_else(|| format!("area effect {what} puts down {unit}, whose area {area} has no area_effect_objects record"))?;
        refuse_action_mechanic_but_on_hit(c, &format!("area effect {area}")).map_err(|e| format!("area effect {what} puts down {unit}: {e}"))?;
        let (then, needs) = area_effect_shape(c, buffs, ctx).map_err(|e| format!("area effect {what} puts down {unit}: {e}"))?;
        Ok((SpellShape::Fuse { fuse_ms, then: Box::new(then) }, needs))
    })();
    Some(got)
}

/// THE MOST ENTRIES A SCHEDULED AREA TAKES: its released entries are one bit each of the spell object's `fired` mask
/// (spell.rs `SpellMotion::Scheduled`). The Graveyard's twelve are the most any row ships.
pub const MAX_SCHEDULED_SPAWNS: usize = 32;

/// AN AREA WHOSE ACTIONS PUT UNITS DOWN ON A SCHEDULE (`SpellShape::ScheduledArea`): the Graveyard's
/// Graveyard_rework (LifeDuration 9000; twelve ActionSpawnToLocation entries of Graveyard_rework_Skeleton, delays 2200
/// to 8200, each at an offset its two position expressions give, DeployTime 500) and the Suspicious Bush's death area
/// SuspiciousBush_DummyAEO (LifeDuration 1000; two BushGoblins at 675 and 625, RelativeX -1 and +1). Measured on client
/// 15.535.29: 120 of 120 Graveyard Skeletons on the cast tick plus floor(delay / 50), 12 of 12 Bush pairs one tick
/// apart.
///
/// None, and the caller's refusal stands, unless the area's schedule has at least one entry that is not cosmetic and
/// every such entry is an ActionSpawnToLocation of a CharacterType. Then the area is refused, naming it, unless:
///   1. it lands nothing (`area_lands_nothing`; a Damage column must be inert, `inert_own_damage`), names no
///      OnHitAction, and its action graph is its schedule (`graph_is_its_schedule`);
///   2. it has a LifeDuration, and every entry is due inside it (delay below the life);
///   3. every entry reads whole: nothing unread, UseDeploy set, and either both position expressions or a RelativeX
///      with no RelativeY (RelativeY is set on no row; not simulated);
///   4. it has at most MAX_SCHEDULED_SPAWNS entries.
///
/// Returns the shape and one need per entry (`UnitUse::Scheduled(k)`), in schedule order.
fn scheduled_area(aeo: &RawAreaEffect) -> Option<Result<(SpellShape, UnitNeeds), String>> {
    let sched = aeo.schedule.as_ref()?;
    #[allow(unused_mut)]
    let mut live: Vec<&RawScheduleEntry> = sched.entries.iter().filter(|e| !e.is_cosmetic()).collect();
    if live.is_empty() || !live.iter().all(|e| e.class.as_deref() == Some("ActionSpawnToLocation") && e.spawn_type.as_deref() == Some("CharacterType")) {
        return None;
    }
    // PLANT schedule_entries_dedup: one entry per distinct action, the way the action graph lists its classes.
    #[cfg(clash_plant = "schedule_entries_dedup")]
    {
        let mut seen: Vec<Option<String>> = Vec::new();
        live.retain(|e| {
            let fresh = !seen.contains(&e.action);
            seen.push(e.action.clone());
            fresh
        });
    }
    let what = aeo.name.clone().unwrap_or_default();
    let refuse = |why: String| -> Option<Result<(SpellShape, UnitNeeds), String>> { Some(Err(format!("area effect {what}: {why}; not simulated"))) };
    if !area_lands_nothing(aeo) || (aeo.damage.is_some() && !inert_own_damage(aeo)) {
        return refuse("an area whose actions put units down also lands something of its own".into());
    }
    if aeo.on_hit.is_some() || !graph_is_its_schedule(aeo, sched) {
        return refuse("its action graph runs more than its spawn schedule".into());
    }
    let Some(life_ms) = aeo.life_duration_ms.filter(|l| *l > 0) else {
        return refuse("a spawn schedule with no LifeDuration".into());
    };
    if live.len() > MAX_SCHEDULED_SPAWNS {
        return refuse(format!("a spawn schedule of {} entries (at most {MAX_SCHEDULED_SPAWNS})", live.len()));
    }
    let mut schedule = Vec::with_capacity(live.len());
    let mut needs: UnitNeeds = Vec::with_capacity(live.len());
    for (k, e) in live.iter().enumerate() {
        let name = e.action.clone().unwrap_or_else(|| format!("entry {k}"));
        if !e.unread.is_empty() {
            return refuse(format!("its action {name} carries {}", e.unread.join(", ")));
        }
        let Some(unit) = e.spawn.clone() else { return refuse(format!("its action {name} names no unit")) };
        if e.use_deploy != Some(true) {
            return refuse(format!("its action {name} puts {unit} down without UseDeploy"));
        }
        let delay_ms = e.delay_ms.unwrap_or(0);
        if delay_ms < 0 || delay_ms >= life_ms {
            return refuse(format!("its action {name} is due at {delay_ms} ms, outside the area's life of {life_ms}"));
        }
        if e.deploy_time_ms.is_some_and(|d| d < 0) {
            return refuse(format!("its action {name} has a negative DeployTime"));
        }
        let form = |p: &Option<RawPosExpr>, want: &str| p.as_ref().filter(|p| p.form.as_deref() == Some(want)).and_then(|p| p.offset_milli);
        let offset = match (&e.x, &e.y, &e.relative) {
            (Some(_), Some(_), None) => match (form(&e.x, "nearer_wall_mirror"), form(&e.y, "team_y_direction")) {
                (Some(dx), Some(dy)) => SpawnOffset::MirroredToWall { dx: milli(dx), dy: milli(dy) },
                _ => return refuse(format!("its action {name} has a position expression of a form this loader does not read")),
            },
            (None, None, Some(r)) if r.y.unwrap_or(0) == 0 => SpawnOffset::Relative { x: r.x.unwrap_or(0), y: 0 },
            (None, None, Some(_)) => return refuse(format!("its action {name} sets a RelativeY")),
            _ => return refuse(format!("its action {name} places {unit} by neither both position expressions nor a RelativeX")),
        };
        schedule.push(ScheduledSpawn { delay_ms, unit: u16::MAX, deploy_time_ms: e.deploy_time_ms, offset });
        needs.push((UnitUse::Scheduled(k as u8), unit));
    }
    Some(Ok((SpellShape::ScheduledArea { life_ms, schedule }, needs)))
}

/// `refuse_action_mechanic` for an area, except a graph that is exactly the area's readable OnHitAction: an
/// ActionGroup of BuffType spawns, which `on_hit_buffs` reads into the area's hit. Such a graph is accepted whether the
/// extractor calls it a mechanic or not (today it walks named actions only, so the Goblin Curse circle's inline buffs
/// leave it cosmetic; a walker that followed inline tables would call it one).
fn refuse_action_mechanic_but_on_hit(aeo: &RawAreaEffect, what: &str) -> Result<(), String> {
    if let (Some(g), Some(oh)) = (&aeo.action_graph, &aeo.on_hit) {
        let spawns: Vec<String> = oh
            .entries
            .iter()
            .filter(|e| !e.is_cosmetic() && e.spawn_type.as_deref() == Some("BuffType"))
            .filter_map(|e| e.spawn.as_ref().map(|s| format!("BuffType:{s}")))
            .collect();
        let only_its_hit = g.class_types.iter().all(|c| matches!(c.as_str(), "ActionGroup" | "ActionSpawn")) && g.spawns.iter().all(|s| spawns.contains(s));
        let roots_on_hit = g.roots.keys().all(|k| k == "OnHitAction");
        if only_its_hit && roots_on_hit {
            return Ok(());
        }
    }
    refuse_action_mechanic(&aeo.action_graph, what)
}

/// AN AREA WHOSE ONE ACTION MAKES ANOTHER AREA (the Goblin Curse: an area of LifeDuration 6000 that hits nothing, whose
/// OnStartingAction spawns the curse circle GoblinCurseBase at once). The parent is COLLAPSED into a zero fuse over the
/// child (`SpellShape::Fuse { fuse_ms: 0, then }`): the fuse releases the child on the cast tick's own update, and the
/// child acts from the next tick with its whole life (spells.AREA_SPAWNED_AREA_START). Measured on client 15.535.29 (five
/// casts): the curse circle first applies one tick after the cast and last on the 120th tick after it; the parent's own
/// Damage never lands (spells.AREA_DAMAGE_WITHOUT_HIT_FLAGS).
///
/// None, and the caller's refusal stands, unless ALL hold:
///   1. the area's schedule has exactly one entry that is not cosmetic: an ActionSpawn of an AreaEffectType at delay 0,
///      with nothing unread (the global Lightning's spawn carries an ActionDelay, which is unread: that row stays
///      refused as before);
///   2. the parent lands nothing: it hits neither ground nor air, and it carries no Buff, Pushback, Projectile,
///      SpawnCharacter, MaximumTargets, child area, positive HitSpeed or HitBiggestTargets. Its own Damage is not read.
///
/// Then the child is read by the non-collapsing path, and must be a one-shot or pulsing area that does not outlive its
/// parent, else the parent is refused with the reason.
fn area_spawns_area(aeo: &RawAreaEffect, buffs: &mut BuffTable, ctx: &LoadCtx) -> Option<Result<(SpellShape, UnitNeeds), String>> {
    let sched = aeo.schedule.as_ref()?;
    let live: Vec<&RawScheduleEntry> = sched.entries.iter().filter(|e| !e.is_cosmetic()).collect();
    let [e] = live.as_slice() else { return None };
    if e.class.as_deref() != Some("ActionSpawn") || e.spawn_type.as_deref() != Some("AreaEffectType") || e.delay_ms.unwrap_or(0) != 0 || !e.unread.is_empty() {
        return None;
    }
    if !area_lands_nothing(aeo) {
        return None;
    }
    let root = sched.root.clone().unwrap_or_default();
    let cname = e.spawn.clone().unwrap_or_default();
    Some(collapse_into_child(aeo, &root, &cname, buffs, ctx))
}

/// The collapse `area_spawns_area` recognised: parent `aeo`, whose action `root` spawns area `cname`.
fn collapse_into_child(aeo: &RawAreaEffect, root: &str, cname: &str, buffs: &mut BuffTable, ctx: &LoadCtx) -> Result<(SpellShape, UnitNeeds), String> {
    let what = aeo.name.clone().unwrap_or_default();
    let c = ctx.aeos.get(cname).ok_or_else(|| format!("area effect {what}'s action {root} spawns {cname}, which has no area_effect_objects record"))?;
    // The child by the NON-collapsing path: its own graph (accepted when it is its readable hit), then its shape.
    refuse_action_mechanic_but_on_hit(c, &format!("area effect {cname}")).map_err(|e| format!("area effect {what} spawns {cname}: {e}"))?;
    let (child, needs) = area_effect_shape(c, buffs, ctx).map_err(|e| format!("area effect {what} spawns {cname}: {e}"))?;
    if !matches!(child, SpellShape::PulsingAreaEffect { .. } | SpellShape::AreaEffect { .. }) {
        return Err(format!("area effect {what} spawns {cname}, which is neither a one-shot nor a pulsing area; not simulated"));
    }
    if c.life_duration_ms.unwrap_or(0) > aeo.life_duration_ms.unwrap_or(0) {
        return Err(format!("area effect {what} spawns {cname}, which outlives its parent; not simulated"));
    }
    #[cfg(not(clash_plant = "curse_parent_as_disc"))]
    let shape = SpellShape::Fuse { fuse_ms: 0, then: Box::new(child) };
    #[cfg(clash_plant = "curse_parent_as_disc")]
    let shape = {
        // PLANT: the parent read as a one-shot disc of its own Damage on both air and ground; the child is lost.
        let _ = child;
        SpellShape::AreaEffect {
            hit: SpellHit {
                damage: aeo.damage.unwrap_or(0),
                crown_pct: crown(aeo.crown_tower_damage_percent),
                radius: milli(aeo.radius_milli.unwrap_or(0)),
                hits_air: true,
                hits_ground: true,
                only_enemies: aeo.only_enemies.unwrap_or(false),
                only_own_troops: false,
                ignore_buildings: false,
                no_effect_to_crown_towers: false,
                knockback: None,
                buff: None,
                buff2: None,
                caps_buff_time: false,
                controls_buff: false,
            },
        }
    };
    Ok((shape, needs))
}

/// THE BUFFS AN AREA'S OnHitAction HANGS on each unit it hits, as (`buff`, `buff2`) of its `SpellHit`, or the reason the
/// area is refused. None, None for an area whose row names no OnHitAction. Accepted: every entry that is not cosmetic
/// is an ActionSpawn of a BuffType at delay 0 with nothing unread, at most two of them and at most one that pulses, on
/// an area whose own Buff column is blank. The one that pulses goes in `buff` (the spell object's `pulse` is that
/// buff's), else the first; each lasts its entry's SpawnTime.
fn on_hit_buffs(aeo: &RawAreaEffect, buffs: &mut BuffTable) -> Result<(Option<BuffApply>, Option<BuffApply>), String> {
    let what = aeo.name.clone().unwrap_or_default();
    let names_on_hit = aeo.action_graph.as_ref().is_some_and(|g| g.roots.contains_key("OnHitAction"));
    let Some(oh) = &aeo.on_hit else {
        if names_on_hit {
            return Err(format!("area effect {what} applies its hit through an action this loader does not read"));
        }
        return Ok((None, None));
    };
    let root = oh.root.clone().unwrap_or_default();
    let refuse = |why: &str| Err(format!("area effect {what}: its hit action {root} {why}; not simulated"));
    if aeo.buff.is_some() {
        return refuse("hangs its buffs beside the area's own Buff");
    }
    let mut got: Vec<BuffApply> = Vec::new();
    for e in oh.entries.iter().filter(|e| !e.is_cosmetic()) {
        if e.class.as_deref() != Some("ActionSpawn") || e.spawn_type.as_deref() != Some("BuffType") || e.delay_ms.unwrap_or(0) != 0 || !e.unread.is_empty() {
            return refuse(&format!("runs {} ({})", e.class.clone().unwrap_or_default(), e.unread.join(", ")));
        }
        let b = e.buff.as_ref().ok_or_else(|| format!("area effect {what}: its hit action {root} spawns buff {} with no buff row", e.spawn.clone().unwrap_or_default()))?;
        got.push(buffs.apply(b, e.spawn_time_ms, &format!("area effect {what}'s hit action"))?);
    }
    if got.len() > 2 {
        return refuse(&format!("hangs {} buffs", got.len()));
    }
    let pulsing: Vec<usize> = (0..got.len()).filter(|&k| buffs.defs[got[k].buff as usize].pulses()).collect();
    if pulsing.len() > 1 {
        return refuse("hangs two buffs that pulse");
    }
    let first = pulsing.first().copied().unwrap_or(0);
    let buff = got.get(first).copied();
    let buff2 = got.iter().enumerate().find(|(k, _)| *k != first).map(|(_, b)| *b);
    Ok((buff, buff2))
}

/// THE AREA EFFECT A CARD IS, when its character is what the area spawns (spells_characters
/// AreaEffectObject whose OnStartingAction is an ActionSpawn of the card's own character: the
/// Electro Wizard's ElectroWizardZap, the Ice Wizard's IceWizardCold; `CardDef::deploy_area_effect`).
/// That spawn IS the deploy the engine already runs, so it is the one action the graph may carry:
/// a graph of exactly one ActionSpawn of `unit_name` is accepted, and any other graph is refused
/// as `convert_area_effect` refuses every mechanic graph. The rest is read as a spell's area is.
fn convert_deploy_area_effect(aeo: &RawAreaEffect, unit_name: &str, buffs: &mut BuffTable, ctx: &LoadCtx) -> Result<(SpellShape, UnitNeeds), String> {
    let what = aeo.name.clone().unwrap_or_default();
    let own_spawn = format!("CharacterType:{unit_name}");
    let only_its_spawn = aeo.action_graph.as_ref().is_some_and(|g| g.class_types == ["ActionSpawn"] && g.spawns == [own_spawn.as_str()]);
    if !only_its_spawn {
        refuse_action_mechanic(&aeo.action_graph, &format!("deploy area effect {what}"))?;
    }
    area_effect_shape(aeo, buffs, ctx)
}

/// THE AREA EFFECT A UNIT PUTS DOWN WHERE IT APPEARS (characters SpawnAreaObject;
/// `CardDef::spawn_area_effect`: the Battle Healer's BattleHealerSpawnHeal). Its action graph is
/// refused like any area's. The heal lands on the releaser's own troops (SpellHit
/// `only_own_troops`), which is what the row says and what was measured on client 15.535.29 -- a
/// damaged friendly Knight 2,000 from her gained the heal, the control without her gained nothing.
fn convert_spawn_area_effect(aeo: &RawAreaEffect, buffs: &mut BuffTable, ctx: &LoadCtx) -> Result<(SpellShape, UnitNeeds), String> {
    let what = aeo.name.clone().unwrap_or_default();
    refuse_action_mechanic(&aeo.action_graph, &format!("spawn area effect {what}"))?;
    area_effect_shape(aeo, buffs, ctx)
}

/// The shape of an area effect whose action graph the caller has accepted (`convert_area_effect`,
/// `convert_deploy_area_effect`, `convert_spawn_area_effect`). An own-troop area (OnlyOwnTroops)
/// is read from every caller: the filter is `SpellHit::only_own_troops`.
fn area_effect_shape(aeo: &RawAreaEffect, buffs: &mut BuffTable, ctx: &LoadCtx) -> Result<(SpellShape, UnitNeeds), String> {
    let what = aeo.name.clone().unwrap_or_default();
    // A STRIKING AREA is recognised first: its HitSpeed and its Projectile are its strikes, not a pulse.
    if aeo.hit_biggest_targets == Some(true) {
        return strike_shape(aeo, buffs).map(|d| (SpellShape::Strikes(Box::new(d)), Vec::new()));
    }
    // AN AREA WITH A PROJECTILE ROW AND NO HitBiggestTargets strikes its own centre (the Royal Delivery): its
    // HitSpeed times the strike, and the projectile is what the strike delivers (`centre_strike_shape`).
    if aeo.projectile.as_ref().is_some_and(|p| !p.is_null()) {
        return centre_strike_shape(aeo).map(|(d, needs)| (SpellShape::Strikes(Box::new(d)), needs));
    }
    // THE BUFFS THE AREA'S OnHitAction HANGS (the Goblin Curse's circle: its damage and slow, and its mark), read
    // before the pulse's own refusals, which they answer.
    let (hit_buff, hit_buff2) = on_hit_buffs(aeo, buffs)?;
    // A PULSING area effect (HitSpeed set) stands on the ground and re-applies its
    // buff; a one-shot one (HitSpeed blank) applies once. Implemented for a
    // pulse that carries a BUFF and nothing else -- Poison and
    // Earthquake. A pulse that deals its own Damage column every HitSpeed
    // (Tornado, WarmAOE) is a second mechanic and stays refused. The striking areas
    // (Lightning, the Royal Delivery, the Vines, the Void) never get here: each is
    // recognised by its own shape first.
    let pulse_ms = aeo.hit_speed_ms.filter(|h| *h > 0);
    if pulse_ms.is_some() {
        if aeo.damage.is_some() {
            return Err(format!("pulsing area effect {what} deals its own Damage every HitSpeed; not simulated"));
        }
        if aeo.buff.is_none() && hit_buff.is_none() {
            return Err(format!("pulsing area effect {what} pulses no buff: its mechanic is not in the columns"));
        }
    }
    let only_own = aeo.only_own_troops.unwrap_or(false);
    if only_own && aeo.only_enemies.unwrap_or(false) {
        return Err(format!("area effect {what} is both OnlyEnemies and OnlyOwnTroops"));
    }
    if aeo.buff_number.is_some_and(|n| n != 1) {
        return Err(format!("area effect {what}: BuffNumber {} is not simulated", aeo.buff_number.unwrap_or(0)));
    }
    if aeo.maximum_targets.is_some() || aeo.projectile.as_ref().is_some_and(|p| !p.is_null()) || aeo.spawn_character.is_some() {
        return Err(format!("area effect {what} with targets / projectile / spawn is not simulated"));
    }
    // AN AREA EFFECT THAT DOES NOTHING THIS LOADER READS is an action graph
    // (15.535 Graveyard_rework: no Damage, no Buff, no Pushback; its Skeletons are
    // an OnStartingAction script this loader does not run). Running it as a
    // zero-damage Zap would be a different card.
    if aeo.damage.is_none() && aeo.buff.is_none() && hit_buff.is_none() && aeo.pushback_milli.is_none() {
        return Err(format!("area effect {what} carries no damage, buff or pushback: its mechanic is an action graph this loader does not read"));
    }
    if !aeo.hits_ground.unwrap_or(false) && !aeo.hits_air.unwrap_or(false) {
        return Err(format!("area effect {what} hits neither ground nor air"));
    }
    // THE AREA'S BUFF, whatever class it is: the stun-class special
    // case is gone -- a -100 / -100 / -100 row is just a buff whose composed speed
    // is 0, and `apply_effects` turns that into the hold every stun already used.
    let area_buff = match &aeo.buff {
        None => hit_buff,
        Some(b) => Some(buffs.apply(b, aeo.buff_time_ms, &format!("area effect {what}"))?),
    };
    // AN OWN-SIDE AREA WHOSE BUFF IS A FULL STOP is refused: the 2018 Clone row would otherwise load
    // as "hold your own troops", a different card (its copy is an action this loader does not run).
    #[cfg(not(clash_plant = "own_full_stop_area_loads"))]
    if only_own && area_buff.is_some_and(|b| buffs.stops(b.buff)) {
        return Err(format!("own-troop area effect {what} holds its own side still; its mechanic is not in the columns"));
    }
    let hit = SpellHit {
        damage: aeo.damage.unwrap_or(0),
        crown_pct: crown(aeo.crown_tower_damage_percent),
        radius: milli(aeo.radius_milli.ok_or_else(|| format!("area effect {what} without radius"))?),
        hits_air: aeo.hits_air.unwrap_or(false),
        hits_ground: aeo.hits_ground.unwrap_or(false),
        only_enemies: aeo.only_enemies.unwrap_or(false),
        only_own_troops: only_own,
        ignore_buildings: aeo.ignore_buildings.unwrap_or(false),
        no_effect_to_crown_towers: aeo.no_effect_to_crown_towers.unwrap_or(false),
        knockback: knockback(aeo.pushback_milli, None),
        buff: area_buff,
        buff2: hit_buff2,
        caps_buff_time: aeo.cap_buff_time_to_area_effect_time.unwrap_or(false),
        controls_buff: aeo.controls_buff.unwrap_or(false),
    };
    // THE CHILD (SpawnAreaEffectObject): a one-shot area with no child of its own, born on a PULSING
    // parent's first update. A child on a one-shot parent, or a child of another shape, is refused.
    let child = match &aeo.spawn_area_effect_object {
        None => None,
        Some(cname) => {
            if pulse_ms.is_none() {
                return Err(format!("one-shot area effect {what} spawns area effect {cname}; not simulated"));
            }
            let c = ctx.aeos.get(cname).ok_or_else(|| format!("area effect {what} spawns {cname}, which has no area_effect_objects record"))?;
            let (cs, _) = convert_area_effect(c, buffs, ctx).map_err(|e| format!("area effect {what} spawns {cname}: {e}"))?;
            if !matches!(cs, SpellShape::AreaEffect { .. }) {
                return Err(format!("area effect {what} spawns area effect {cname} whose shape is not a one-shot area; not simulated"));
            }
            Some(Box::new(cs))
        }
    };
    let shape = match pulse_ms {
        None => {
            let _ = aeo.life_duration_ms; // one-shot: applied once whatever its life (see SpellShape)
            SpellShape::AreaEffect { hit }
        }
        Some(hit_speed_ms) => {
            let life_ms = aeo
                .life_duration_ms
                .filter(|l| *l > 0)
                .ok_or_else(|| format!("pulsing area effect {what} without LifeDuration"))?;
            SpellShape::PulsingAreaEffect { hit, life_ms, hit_speed_ms, child }
        }
    };
    Ok((shape, Vec::new()))
}

/// The SPELL half of the loader. Accepts exactly the shapes `SpellShape` implements
/// and REJECTS every other spell with the reason, so a card whose mechanic is not
/// simulated can never run as a different, simpler card.
///
/// SIMULATED (2018 data): Fireball, Arrows, Rocket, Goblin Barrel (Projectile);
/// Zap, Freeze (AreaEffect); The Log (Rolling). REJECTED, with why: Rage, Poison,
/// Heal, Tornado (pulsing area effects), Lightning (an area effect firing targeted
/// projectiles), Graveyard (spawning area effect), Clone (own-troop buff), Mirror
/// (no mechanic in the 2018 data). Freeze and Rocket are not in the thin slice; they run
/// because their data has exactly the implemented shape, and Freeze's 4 s life is
/// where calibration spells.ONE_SHOT_AREA_EFFECT_APPLICATION is LOW confidence.
/// The 15.535 table's Mirror (`spell.mirror`) and Spirit Empress (`spell.variant`) load
/// as `SpellShape::Mirror` and `SpellShape::Variant`, which are never cast; its Clone
/// (the area's `clone_action`) as `SpellShape::Clone`, and its Vines and Void (the
/// area's `strike_area`) as `SpellShape::Strikes`.
///
/// Returns the card, and the units it needs loaded: which mechanic, unit name
/// (resolved to an index by `CardDb::from_json_str`, which loads each unit).
fn convert_spell(raw: RawCard, buffs: &mut BuffTable, ctx: &LoadCtx) -> Result<(CardDef, UnitNeeds), String> {
    let mut spell = raw.spell.unwrap_or_default();
    let mut def = stat_less(raw.name.clone(), raw.rarity.clone().ok_or("missing rarity")?, raw.elixir.unwrap_or(0));
    (def.level_table, def.level_base) = level_table_of(raw.level_scaling)?;
    // OmitFromStartingHand (the Mirror): the deal rule reads the card (state.rs `try_new`, economy.OMIT_FROM_STARTING_HAND).
    def.omit_from_starting_hand = raw.omit_from_starting_hand.unwrap_or(false);
    if spell.duration_seconds.is_some() {
        return Err("spell DurationSeconds is not simulated".into());
    }
    let proj: Option<RawSpellProjectile> = match raw.projectile {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => Some(serde_json::from_value(v).map_err(|e| format!("projectile: {e}"))?),
    };
    // THE VARIANT CARD AND THE MIRROR carry no object of their own: state.rs `resolve_play` turns playing one into
    // playing another card. A row that carries one of them beside a projectile, an area or a summon is a shape this
    // loader does not read, and is refused.
    let own_object = proj.is_some() || spell.first_projectile.is_some() || spell.area_effect_object.is_some() || spell.summon.is_some();
    if let Some(v) = spell.variant.take() {
        if own_object || spell.mirror == Some(true) {
            return Err("a variant card that also carries a mechanic of its own is not simulated".into());
        }
        return convert_variant(def, v);
    }
    if spell.mirror == Some(true) {
        if own_object {
            return Err("a Mirror that also carries a mechanic of its own is not simulated".into());
        }
        // The copy's level is the Mirror's own plus the table's MIRROR_LEVEL_OFFSET (`CardGlobals`): a file that does
        // not carry the row cannot say it, and the card is refused rather than given a number.
        match ctx.globals.mirror_level_offset {
            Some(n) if n >= 1 => {}
            Some(n) => return Err(format!("a Mirror whose MIRROR_LEVEL_OFFSET is {n} is not simulated")),
            None => return Err("a Mirror needs the table's MIRROR_LEVEL_OFFSET (cards.json `globals`), which this file does not carry".into()),
        }
        def.spell = Some(SpellDef { shape: SpellShape::Mirror, placement: SpellPlacement::Anywhere });
        return Ok((def, Vec::new()));
    }
    let as_deploy = spell.spell_as_deploy.unwrap_or(false);
    let placement_for = |spawns: bool| {
        if as_deploy && !spell.can_deploy_on_enemy_side.unwrap_or(false) {
            SpellPlacement::TroopTerritory { on_buildings: spell.can_place_on_buildings.unwrap_or(false) }
        } else if spawns {
            SpellPlacement::AnywhereButWater
        } else {
            SpellPlacement::Anywhere
        }
    };
    let mut units: UnitNeeds = Vec::new();
    let shape = if let Some(aeo) = spell.area_effect_object {
        let what = aeo.name.clone().unwrap_or_default();
        if proj.is_some() || spell.first_projectile.is_some() {
            return Err(format!("spell with both an area effect ({what}) and a projectile is not simulated"));
        }
        let (shape, needs) = convert_area_effect(&aeo, buffs, ctx)?;
        units.extend(needs);
        SpellDef { shape, placement: placement_for(false) }
    } else {
        // DAMAGE CARRIER (docs/spell-spec.md): CustomFirstProjectile when present (the
        // 2018 Arrows pattern -- `projectile` is then the damage-less ArrowsSpellDeco and
        // MultipleProjectiles is visual); otherwise Projectile itself.
        let carrier = match (spell.first_projectile, proj) {
            (Some(f), _) => f,
            (None, Some(p)) => p,
            (None, None) => {
                // A SPELL THAT SUMMONS (Rage's bottle, the Heal Spirit): the unit is resolved by
                // `CardDb::from_json_str` (UnitUse::SpellSummon), which turns a bottle row into this
                // spell's `Fuse` and fills `unit` for anything else.
                let Some(sm) = spell.summon.as_ref() else {
                    return Err("spell with no projectile and no area effect: no mechanic in the data".into());
                };
                let unit = sm.character.clone().ok_or("spell summon without a character")?;
                let count = sm.count.unwrap_or(1);
                if count != 1 {
                    return Err(format!("a spell summoning {count} units is not simulated"));
                }
                units.push((UnitUse::SpellSummon, unit));
                def.spell = Some(SpellDef { shape: SpellShape::Summon { unit: u16::MAX, count }, placement: placement_for(false) });
                return Ok((def, units));
            }
        };
        let what = carrier.name.clone().unwrap_or_default();
        refuse_action_mechanic(&carrier.action_graph, &format!("projectile {what}"))?;
        if let Some(roll) = &carrier.spawn_projectile {
            refuse_action_mechanic(&roll.action_graph, &format!("rolling projectile {}", roll.name.clone().unwrap_or_default()))?;
        }
        if carrier.maximum_targets.is_some() || carrier.spawn_area_effect_object.is_some() {
            return Err(format!("projectile {what} with a target cap or an area effect is not simulated"));
        }
        // TargetBuff + BuffTime (the Snowball's IceWizardSlowDown 3000 ms): the buff
        // rides the impact and lands on everything the splash lands on, under
        // calibration status.TARGET_BUFF_ON_SPLASH.
        let target_buff = match &carrier.target_buff {
            None => None,
            Some(v) if v.is_null() => None,
            Some(v) => {
                let b: RawBuff = serde_json::from_value(v.clone()).map_err(|e| format!("projectile {what} TargetBuff: {e}"))?;
                Some(buffs.apply(&b, carrier.buff_time_ms, &format!("projectile {what}"))?)
            }
        };
        let speed = carrier.speed.filter(|s| *s > 0).ok_or_else(|| format!("projectile {what} without speed"))?;
        if let Some(roll) = carrier.spawn_projectile {
            let rname = roll.name.clone().unwrap_or_default();
            if !as_deploy {
                return Err(format!("{what} releases {rname} but the spell is not SpellAsDeploy"));
            }
            if carrier.damage.is_some() || carrier.spawn_character.is_some() {
                return Err(format!("airborne {what} carries damage or units; not simulated"));
            }
            let range = roll.projectile_range_milli.filter(|r| *r > 0).ok_or_else(|| format!("{rname} has no ProjectileRange: not a rolling projectile"))?;
            if roll.maximum_targets.is_some() || roll.spawn_projectile.is_some() || roll.target_buff.as_ref().is_some_and(|b| !b.is_null()) {
                return Err(format!("rolling {rname} with targets / a second projectile / buffs is not simulated"));
            }
            // A ROLL THAT RELEASES UNITS where it stops. A BLANK SpawnCharacterCount is ONE unit on a roll: the
            // 15.535.29 Barbarian Barrel's row leaves it blank, and every cast measured (4 of 4 on the 16.402
            // corpus, the 15.535.29 scenario runs) released exactly one Barbarian. Not the Goblin Barrel's rule,
            // which refuses a blank count on a projectile (its row ships 3).
            #[cfg(not(clash_plant = "roll_spawn_unread"))]
            let roll_spawn = roll.spawn_character.clone();
            #[cfg(clash_plant = "roll_spawn_unread")]
            let roll_spawn: Option<String> = None; // PLANT: the roll's SpawnCharacter dropped.
            let spawn = match roll_spawn {
                None => None,
                Some(unit) => {
                    let count = match roll.spawn_character_count {
                        None => 1,
                        Some(c) if c >= 1 => c,
                        Some(c) => return Err(format!("rolling {rname}: SpawnCharacterCount {c} out of range")),
                    };
                    units.push((UnitUse::Spell, unit));
                    Some(SpawnDef {
                        unit: u16::MAX, // resolved by from_json_str
                        count,
                        deploy_time_ms: roll.spawn_character_deploy_time_ms,
                        level_index: roll.spawn_character_level_index,
                    })
                }
            };
            let hit = SpellHit {
                damage: roll.damage.ok_or_else(|| format!("rolling {rname} without damage"))?,
                crown_pct: crown(roll.crown_tower_damage_percent),
                radius: 0,
                hits_air: roll.aoe_to_air.unwrap_or(false),
                hits_ground: roll.aoe_to_ground.unwrap_or(false),
                only_enemies: roll.only_enemies.unwrap_or(false),
                only_own_troops: false,
                ignore_buildings: false,
                no_effect_to_crown_towers: false,
                knockback: knockback(roll.pushback_milli, roll.pushback_all),
                buff: None,
                buff2: None,
                caps_buff_time: false,
                controls_buff: false,
            };
            SpellDef {
                shape: SpellShape::Rolling {
                    airborne_speed: speed,
                    airborne_min_distance: milli(carrier.min_distance_milli.unwrap_or(0)),
                    speed: roll.speed.filter(|s| *s > 0).ok_or_else(|| format!("rolling {rname} without speed"))?,
                    range: milli(range),
                    half_width: milli(roll.projectile_radius_milli.filter(|r| *r > 0).ok_or_else(|| format!("rolling {rname} without ProjectileRadius"))?),
                    half_depth: milli(roll.projectile_radius_y_milli.unwrap_or(0)),
                    hit,
                    spawn,
                },
                placement: placement_for(false),
            }
        } else {
            // THE WAVE'S DISC (calibration spells.WAVE_AREA_MODEL = single_disc_one_hit_
            // per_wave). A VOLLEY -- MultipleProjectiles > 1 -- spreads its arrows over
            // the SPELL's Radius, so that is the disc each wave hits once: the 15.535
            // Arrows fires 10 homing 1400-radius arrows per wave over a 3500 spell
            // radius (the 2018 row's carrier radius equalled the spell's, 4000, so
            // that file reads the same either way). A single projectile's disc is its
            // own radius. The ten_subareas_* candidates would model each arrow.
            let volley = spell.multiple_projectiles.unwrap_or(1);
            if volley < 1 {
                return Err(format!("MultipleProjectiles {volley} out of range"));
            }
            let disc = || -> Result<i32, String> {
                if volley > 1 {
                    spell.radius_milli.filter(|r| *r > 0).ok_or_else(|| format!("projectile {what}: a volley of {volley} with no spell Radius to spread over"))
                } else {
                    carrier.radius_milli.filter(|r| *r > 0).ok_or_else(|| format!("projectile {what} deals damage but has no radius"))
                }
            };
            let hit = match carrier.damage {
                None => None,
                Some(d) => Some(SpellHit {
                    damage: d,
                    crown_pct: crown(carrier.crown_tower_damage_percent),
                    radius: milli(disc()?),
                    hits_air: carrier.aoe_to_air.unwrap_or(false),
                    hits_ground: carrier.aoe_to_ground.unwrap_or(false),
                    only_enemies: carrier.only_enemies.unwrap_or(false),
                    only_own_troops: false,
                    ignore_buildings: false,
                    no_effect_to_crown_towers: false,
                    knockback: knockback(carrier.pushback_milli, carrier.pushback_all),
                    buff: target_buff,
                    buff2: None,
                    caps_buff_time: false,
                    controls_buff: false,
                }),
            };
            let spawn = match carrier.spawn_character.clone() {
                None => None,
                Some(unit) => {
                    let count = carrier.spawn_character_count.filter(|c| *c > 0).ok_or_else(|| format!("{what} spawns {unit} with no count"))?;
                    units.push((UnitUse::Spell, unit));
                    Some(SpawnDef {
                        unit: u16::MAX, // resolved by from_json_str
                        count,
                        deploy_time_ms: carrier.spawn_character_deploy_time_ms,
                        level_index: carrier.spawn_character_level_index,
                    })
                }
            };
            if hit.is_none() && spawn.is_none() {
                return Err(format!("projectile {what} neither deals damage nor spawns units"));
            }
            let waves = spell.projectile_waves.unwrap_or(1);
            let wave_interval_ms = spell.projectile_wave_interval_ms.unwrap_or(0);
            if waves < 1 || wave_interval_ms < 0 {
                return Err(format!("ProjectileWaves {waves} / interval {wave_interval_ms} out of range"));
            }
            let spawns = spawn.is_some();
            SpellDef { shape: SpellShape::Projectile { speed, hit, waves, wave_interval_ms, spawn }, placement: placement_for(spawns) }
        }
    };
    def.spell = Some(shape);
    Ok((def, units))
}

/// (the ladder, the unified level it is entered from) of a cards.json
/// `level_scaling` block (module doc, LEVEL SCALING). A `reading` this loader does
/// not implement refuses the card; `object_rarity_local_1` needs its `base_level`.
fn level_table_of(v: Option<serde_json::Value>) -> Result<(Option<Vec<i32>>, Option<i32>), String> {
    let Some(v) = v.filter(|v| !v.is_null()) else { return Ok((None, None)) };
    let ls: RawLevelScaling = serde_json::from_value(v).map_err(|e| format!("level_scaling: {e}"))?;
    let base = match ls.reading.as_deref() {
        None => None,
        Some("card_rarity_local_1") => None,
        Some("object_rarity_local_1") => Some(ls.base_level.filter(|b| *b >= 1).ok_or("level_scaling: object_rarity_local_1 without a base_level >= 1")?),
        Some(other) => return Err(format!("level_scaling reading {other:?} is not implemented (candidates: {LEVEL_BASE_READINGS:?})")),
    };
    if let (Some(t), Some(_)) = (&ls.multiplier_percent_by_level, base) {
        if t.is_empty() {
            return Err("level_scaling: an empty ladder".into());
        }
    }
    Ok((ls.multiplier_percent_by_level, base))
}

/// (card, display name, the units it needs loaded: which mechanic, unit name).
type Converted = (CardDef, Option<String>, UnitNeeds);

/// The SPAWNER half of the loader: the `spawner` block is all-or-nothing. A block
/// with a SpawnCharacter needs SpawnNumber and SpawnPauseTime (a Witch or hut row
/// with only one of them is a data error, not a spawner with a guessed cadence);
/// one without a character but with any other column set is refused too. THE
/// GAME'S OWN BLANK: a SpawnCharacter with BOTH SpawnNumber and SpawnPauseTime
/// blank is no periodic spawner at all -- the 2018 SkeletonContainer row ships
/// `SpawnCharacter = Skeleton` with both blank (and death-spawns its 8 Skeletons
/// through DeathSpawn*), so the pair-blank reading is the file's, not a guess.
/// Returns the def with `unit` unresolved (u16::MAX) and the unit's name.
fn convert_spawner(raw: Option<RawSpawner>) -> Result<Option<(SpawnerDef, String)>, String> {
    let Some(b) = raw else { return Ok(None) };
    let Some(unit) = b.character else {
        if b.number.is_some() || b.pause_time_ms.is_some() || b.interval_ms.is_some() || b.start_time_ms.is_some() || b.limit.is_some() || b.radius_milli.is_some() {
            return Err("spawner block with Spawn* columns but no SpawnCharacter".into());
        }
        return Ok(None);
    };
    if b.number.is_none() && b.pause_time_ms.is_none() {
        return Ok(None);
    }
    let number = b.number.ok_or_else(|| format!("spawner {unit}: no SpawnNumber"))?;
    let pause_time_ms = b.pause_time_ms.ok_or_else(|| format!("spawner {unit}: no SpawnPauseTime"))?;
    let interval_ms = b.interval_ms.unwrap_or(0);
    if number < 1 || pause_time_ms <= 0 || interval_ms < 0 || b.start_time_ms.is_some_and(|t| t < 0) || b.limit.is_some_and(|l| l < 1) || b.radius_milli.is_some_and(|r| r < 0) {
        return Err(format!(
            "spawner {unit}: SpawnNumber {number} / SpawnPauseTime {pause_time_ms} / SpawnInterval {interval_ms} / SpawnStartTime {:?} / SpawnLimit {:?} / SpawnRadius {:?} out of range",
            b.start_time_ms, b.limit, b.radius_milli
        ));
    }
    Ok(Some((
        SpawnerDef {
            unit: u16::MAX,
            number,
            interval_ms,
            start_time_ms: b.start_time_ms,
            pause_time_ms,
            limit: b.limit,
            radius: b.radius_milli.map(milli),
            source: SpawnerSource::Columns,
            to_location: None,
            emit_deploy_ms: None,
        },
        unit,
    )))
}

/// THE ATTACHED-RIDER HALF of the Spawn* block (SpawnAttach set; `AttachDef`): the riders a
/// mount carries, or the reason the block is refused. SpawnNumber is needed. A periodic cadence
/// beside SpawnAttach (SpawnPauseTime, SpawnInterval, SpawnStartTime, SpawnLimit) is a shape no
/// row ships, and is refused. A SpawnRadius (the Goblin Giant's 900) puts the riders on an arc
/// behind the mount (calibration rider.OFFSET_LAW; formation.rs `rider_arc_offset`). A SpawnNumber
/// above 1 with no SpawnRadius is refused: the riders would all stand on the mount's centre, a
/// layout no row ships and nothing measured.
/// Returns the def with `unit` unresolved (u16::MAX) and the rider's name.
fn convert_attach(b: RawSpawner) -> Result<(AttachDef, String), String> {
    let unit = b.character.ok_or("an attached-rider block with no SpawnCharacter")?;
    if b.pause_time_ms.is_some() || b.interval_ms.is_some() || b.start_time_ms.is_some() || b.limit.is_some() {
        return Err(format!("attached rider {unit}: a periodic cadence (SpawnPauseTime, SpawnInterval, SpawnStartTime or SpawnLimit) is not simulated"));
    }
    let number = b.number.filter(|n| *n >= 1).ok_or_else(|| format!("attached rider {unit}: no SpawnNumber"))?;
    let radius = match b.radius_milli {
        None => None,
        Some(r) if r > 0 => Some(milli(r)),
        Some(r) => return Err(format!("attached rider {unit}: SpawnRadius {r} out of range")),
    };
    if number != 1 && radius.is_none() {
        return Err(format!("attached rider {unit}: {number} riders on one mount (SpawnNumber {number}) with no SpawnRadius is not simulated"));
    }
    if number > 8 {
        return Err(format!("attached rider {unit}: SpawnNumber {number} out of range"));
    }
    Ok((AttachDef { unit: u16::MAX, number, radius }, unit))
}

/// THE RIDER ROWS THE RIDER LAW COVERS (`UnitUse::Attach`): a troop with no movement of its own to
/// run, which leaves nothing on the board but one unit where it dies, its dismount. `leaves` says
/// the record puts down a unit, an area or a projectile of its own besides its death spawn (its
/// needs, when it loads; its resolved blocks, when another path loaded it first). The dismount
/// comes down where calibration rider.DISMOUNT_POINT says (state.rs `phase_reap`): the Goblin
/// Giant's Spear Goblins each leave one Spear Goblin. A rider that flies (the Spear Goblins'
/// FlyingHeight 4000) is taken: it is carried and never walks, so its height decides only who may
/// target it, and under rider.TARGETABLE_WHILE_ATTACHED = untargetable_immune nothing does. A rider
/// row outside this is refused with the reason.
fn rider_shape(c: &CardDef, leaves: bool, unit: &str) -> Result<(), String> {
    let refuse = |why: String| Err(format!("units.{unit}: an attached rider that {why} is not simulated"));
    if c.kind != CardKind::Troop {
        return refuse("is a building".into());
    }
    if leaves || c.death_damage > 0 {
        return refuse("leaves something on the board of its own besides its dismount (a spawn, an area, a projectile or a death blow)".into());
    }
    if let Some(ds) = c.death_spawn.filter(|ds| ds.count != 1) {
        return refuse(format!("leaves {} units where it dies", ds.count));
    }
    if c.charge.is_some() || c.jump.is_some() || c.dash.is_some() || c.special.is_some() {
        return refuse("moves on its own (a charge, a leap, a dash or a hook)".into());
    }
    if c.kamikaze {
        return refuse("dies on its own hit".into());
    }
    Ok(())
}

/// The DEATH-SPAWN half of the loader. A blank DeathSpawnCount is ONE: Balloon and
/// RageBarbarian ship DeathSpawnCharacter with no count in the 2018 data and in
/// 15.535 alike and the game drops exactly one object, so the blank is the game's
/// own default (a column semantic, like a blank SpawnInterval), not a guess. Their
/// units cannot load anyway (`from_json_str`), so both cards end up rejected -- but
/// AFTER the push, which keeps the format-3 card list (state.rs `migrate_v3`)
/// intact: the earlier loader ran the Balloon as a plain flyer.
fn convert_death_spawn(raw: Option<RawDeathSpawn>) -> Result<Option<(DeathSpawnDef, String)>, String> {
    let Some(b) = raw else { return Ok(None) };
    let Some(unit) = b.character else {
        if b.count.is_some() || b.radius_milli.is_some() || b.deploy_time_ms.is_some() {
            return Err("death_spawn block with DeathSpawn* columns but no DeathSpawnCharacter".into());
        }
        return Ok(None);
    };
    let count = b.count.unwrap_or(1);
    if count < 1 || b.radius_milli.is_some_and(|r| r < 0) || b.deploy_time_ms.is_some_and(|d| d < 0) {
        return Err(format!("death_spawn {unit}: DeathSpawnCount {count} / DeathSpawnRadius {:?} / DeathSpawnDeployTime {:?} out of range", b.radius_milli, b.deploy_time_ms));
    }
    Ok(Some((DeathSpawnDef { unit: u16::MAX, count, radius: b.radius_milli.map(milli), deploy_time_ms: b.deploy_time_ms }, unit)))
}

/// The CHARGE half of the loader: the `charge` block is all-or-nothing. A block with
/// any of ChargeRange, DamageSpecial or ChargeSpeedMultiplier missing or non-positive
/// is a data error (a Prince with no run-up length would silently run as a plain
/// melee unit, which is the defect this mechanic replaces), never a default. Only a
/// TROOP charges: the accumulator counts locomotion, and a building never walks.
fn convert_charge(raw: Option<RawCharge>, kind: CardKind) -> Result<Option<ChargeDef>, String> {
    let Some(b) = raw else { return Ok(None) };
    let need = |v: Option<i32>, what: &str| match v {
        Some(x) if x > 0 => Ok(x),
        Some(x) => Err(format!("charge: {what} {x} is not positive")),
        None => Err(format!("charge: no {what}")),
    };
    let damage_special = need(b.damage_special, "DamageSpecial")?;
    let range_raw = need(b.charge_range_raw, "ChargeRange")?;
    let speed_multiplier_percent = need(b.charge_speed_multiplier_percent, "ChargeSpeedMultiplier")?;
    if kind != CardKind::Troop {
        return Err(format!("charge block on a {kind:?}; only a troop charges"));
    }
    Ok(Some(ChargeDef { range_raw, damage_special, speed_multiplier_percent }))
}

/// The JUMP half of the loader: the `jump` block is all-or-nothing. A JumpEnabled
/// card with no JumpSpeed would leap at nothing per tick and never land (the landing
/// test divides by it), so a missing or non-positive JumpSpeed is a data error;
/// JumpHeight is kept as loaded (the arc is not modelled, but the column is the
/// card's). Only a TROOP jumps: the hop is a property of the walk.
fn convert_jump(raw: Option<RawJump>, kind: CardKind) -> Result<Option<JumpDef>, String> {
    let Some(b) = raw else { return Ok(None) };
    let speed = match b.speed {
        Some(x) if x > 0 => x,
        Some(x) => return Err(format!("jump: JumpSpeed {x} is not positive")),
        None => return Err("jump: no JumpSpeed".into()),
    };
    let height_raw = match b.height_raw {
        Some(x) if x >= 0 => x,
        Some(x) => return Err(format!("jump: JumpHeight {x} is negative")),
        None => return Err("jump: no JumpHeight".into()),
    };
    if kind != CardKind::Troop {
        return Err(format!("jump block on a {kind:?}; only a troop jumps"));
    }
    Ok(Some(JumpDef { speed, height_raw }))
}

/// The DASH half of the loader (`DashDef`). The block is all-or-nothing on the numbers the
/// dash cannot run without: DashDamage, DashMinRange, DashMaxRange, DashCooldown and
/// JumpSpeed. A block missing one is a data error, never a default: a Bandit with no dash
/// speed would stand and never arrive. Two exceptions, both named:
///   - a block with no `speed` KEY at all is the 2018 file's (the extractor writes the
///     dash's motion on the 15.535 rows only, keeping that file byte-identical), and loads
///     no dash, as that table did before the dash was read;
///   - DashRadius, DashPushBack, DashImmuneToDamageTime, DashConstantTime and
///     DashLandingTime are blank on one of the two loaded rows each, and blank means "none".
///
/// Only a TROOP dashes: the dash replaces a walk.
fn convert_dash(raw: Option<RawDash>, kind: CardKind) -> Result<Option<DashDef>, String> {
    let Some(b) = raw else { return Ok(None) };
    #[cfg(not(clash_plant = "dash_unread"))]
    let Some(speed) = b.speed else { return Ok(None) };
    #[cfg(clash_plant = "dash_unread")]
    let Some(speed) = b.speed.filter(|_| false) else { return Ok(None) }; // PLANT: the loader drops the block, so no unit dashes.
    let need = |v: Option<i32>, what: &str| match v {
        Some(x) if x > 0 => Ok(x),
        Some(x) => Err(format!("dash: {what} {x} is not positive")),
        None => Err(format!("dash: no {what}")),
    };
    let opt = |v: Option<i32>, what: &str| match v {
        Some(x) if x > 0 => Ok(Some(x)),
        Some(x) => Err(format!("dash: {what} {x} is not positive")),
        None => Ok(None),
    };
    let damage = need(b.damage, "DashDamage")?;
    let min_range = milli(need(b.min_range_milli, "DashMinRange")?);
    let max_range = milli(need(b.max_range_milli, "DashMaxRange")?);
    let cooldown_ms = need(b.cooldown_ms, "DashCooldown")?;
    let speed = need(speed, "JumpSpeed")?;
    if min_range > max_range {
        return Err(format!("dash: DashMinRange {} is beyond DashMaxRange {}", b.min_range_milli.unwrap_or(0), b.max_range_milli.unwrap_or(0)));
    }
    if kind != CardKind::Troop {
        return Err(format!("dash block on a {kind:?}; only a troop dashes"));
    }
    Ok(Some(DashDef {
        damage,
        min_range,
        max_range,
        cooldown_ms,
        radius: opt(b.radius_milli, "DashRadius")?.map(milli),
        pushback_raw: opt(b.pushback_milli, "DashPushBack")?,
        immune_ms: opt(b.immune_to_damage_time_ms, "DashImmuneToDamageTime")?,
        speed,
        constant_time_ms: opt(b.constant_time_ms, "DashConstantTime")?,
        landing_time_ms: opt(b.landing_time_ms, "DashLandingTime")?,
    }))
}

/// THE DAMAGE RAMP half of the loader: the `variable_damage` block is all-or-nothing. A
/// block missing any of the four columns, or with a negative one, is a data error (an
/// Inferno with no second stage would run as a card that never ramps), never a default.
fn convert_variable_damage(raw: Option<RawVariableDamage>) -> Result<Option<VariableDamageDef>, String> {
    let Some(b) = raw else { return Ok(None) };
    let need = |v: Option<i32>, what: &str| match v {
        Some(x) if x >= 0 => Ok(x),
        Some(x) => Err(format!("variable_damage: {what} {x} is negative")),
        None => Err(format!("variable_damage: no {what}")),
    };
    Ok(Some(VariableDamageDef {
        damage2: need(b.damage2, "VariableDamage2")?,
        damage3: need(b.damage3, "VariableDamage3")?,
        time1_ms: need(b.time1_ms, "VariableDamageTime1")?,
        time2_ms: need(b.time2_ms, "VariableDamageTime2")?,
    }))
}

/// THE HOOK SPECIAL half of the loader: the `special` block is all-or-nothing on
/// SpecialRange, SpecialLoadTime, the special projectile's Speed and its DragMargin
/// (SpecialMinRange blank reads as no minimum). Only a TROOP hooks: the measured law
/// stands a unit still and drags a ground troop to it, and the building case (DragSelfSpeed)
/// is not measured. A special projectile that carries damage or a buff is not the hook the
/// law describes, and is refused rather than run as one.
fn convert_special(raw: Option<RawSpecial>, kind: CardKind) -> Result<Option<SpecialDef>, String> {
    let Some(b) = raw else { return Ok(None) };
    let need = |v: Option<i32>, what: &str| match v {
        Some(x) if x > 0 => Ok(x),
        Some(x) => Err(format!("special: {what} {x} is not positive")),
        None => Err(format!("special: no {what}")),
    };
    let range = milli(need(b.range_milli, "SpecialRange")?);
    let min_range = milli(b.min_range_milli.unwrap_or(0).max(0));
    let load_time_ms = need(b.load_time_ms, "SpecialLoadTime")?;
    let p = b.projectile.ok_or("special: no ProjectileSpecial row")?;
    let projectile_speed = need(p.speed, "the special projectile's Speed")?;
    if p.damage.is_some_and(|d| d != 0) || p.target_buff.is_some() {
        return Err("special: the special projectile carries damage or a buff; not simulated".into());
    }
    let drag_margin = match b.drag_margin_milli {
        Some(x) if x >= 0 => milli(x),
        Some(x) => return Err(format!("special: DragMargin {x} is negative")),
        None => return Err("special: no DragMargin".into()),
    };
    if kind != CardKind::Troop {
        return Err(format!("special block on a {kind:?}; only a troop hooks"));
    }
    Ok(Some(SpecialDef { range, min_range, load_time_ms, projectile_speed, drag_margin }))
}

/// WHAT A BUILDING ROW WITH NO HITPOINTS IS, when it is a shape this loader reads
/// (`hitpointless_building`). Such a row cannot be born as an entity, so each shape
/// loads as something else; a row that is none of them goes to the ordinary loader,
/// which refuses it by name (`missing hitpoints`).
#[derive(Clone, PartialEq, Eq, Debug)]
enum Hitpointless {
    /// A DEATH BOMB (`convert_death_bomb`): DeployTime, DeathDamage and
    /// DeathDamageRadius, all three positive, and no other block.
    DeathBomb { fuse_ms: i32, damage: i32, radius_milli: i32 },
    /// A DEATH BOMB WITH A DEATH SPAWN (the Skeleton Barrel's container): the death bomb's three
    /// columns and a DeathSpawn block, and no other block. When its fuse ends it hits, and it also
    /// releases its own death spawn (`convert_death_bomb` with the spawn; state.rs `release_fuse_end`).
    BombWithDeathSpawn { fuse_ms: i32, damage: i32, radius_milli: i32 },
    /// A BOTTLE (Rage's RageBottle): DeployTime and a DeathAreaEffect, and no other block. Only a
    /// spell summon releases one; it becomes that spell's `SpellShape::Fuse`.
    Bottle { fuse_ms: i32, area: String },
}

/// Which `Hitpointless` shape `raw` is, or None.
fn hitpointless_building(raw: &RawCard) -> Option<Hitpointless> {
    // THE SHAPE, ALL OF IT. A row that misses one clause is not a bomb and falls
    // through to the ordinary loader, which refuses it by name and says why: a
    // half-recognised bomb that loaded and behaved wrongly would be worse than a
    // refusal.
    let bottle = match (raw.deploy_time_ms, raw.death_damage, raw.death_damage_radius_milli, &raw.death_area_effect) {
        (Some(f), None, None, Some(area)) if f > 0 => Some((f, area.clone())),
        _ => None,
    };
    let bomb = match (raw.deploy_time_ms, raw.death_damage, raw.death_damage_radius_milli) {
        (Some(f), Some(d), Some(r)) if f > 0 && d > 0 && r > 0 => Some((f, d, r)),
        _ => None,
    };
    if bottle.is_none() && bomb.is_none() {
        return None;
    }
    let blank = raw.kind == CardKind::Building
        && raw.hitpoints.is_none()
        && raw.damage.is_none()
        && raw.hit_speed_ms.is_none()
        && raw.range_milli.is_none()
        && raw.lifetime_ms.is_none()
        && raw.speed.unwrap_or(0) == 0
        && raw.shield_hitpoints.unwrap_or(0) == 0
        && !matches!(&raw.projectile, Some(v) if !v.is_null())
        && raw.spawner.is_none()
        // A death spawn only beside the bomb's own three columns (the container). PLANT
        // container_not_a_bomb (tests/skeleton_barrel.rs): the earlier clause, so the container
        // falls to the ordinary loader and is refused on its hitpoints.
        && {
            #[cfg(not(clash_plant = "container_not_a_bomb"))]
            let spawn_ok = raw.death_spawn.is_none() || bomb.is_some();
            #[cfg(clash_plant = "container_not_a_bomb")]
            let spawn_ok = raw.death_spawn.is_none();
            spawn_ok
        }
        && (raw.death_area_effect.is_none() || bottle.is_some())
        && raw.death_spawn_projectile.is_none()
        && raw.deploy_area_effect.is_none()
        && raw.spawn_area_object.is_none()
        && raw.action_graph.is_none()
        && raw.spawn_pathfind.is_none()
        && raw.charge.is_none()
        && raw.jump.is_none()
        && raw.buff_on_damage.is_none()
        && raw.second_summon.is_none()
        && !raw.hides_when_not_attacking.unwrap_or(false)
        && !raw.kamikaze.unwrap_or(false);
    if !blank {
        return None;
    }
    match (bottle, bomb) {
        (Some((fuse_ms, area)), _) => Some(Hitpointless::Bottle { fuse_ms, area }),
        (None, Some((fuse_ms, damage, radius_milli))) if raw.death_spawn.is_some() => Some(Hitpointless::BombWithDeathSpawn { fuse_ms, damage, radius_milli }),
        (None, Some((fuse_ms, damage, radius_milli))) => Some(Hitpointless::DeathBomb { fuse_ms, damage, radius_milli }),
        (None, None) => None,
    }
}

/// A DEATH BOMB: the thing a dying Balloon, Giant Skeleton or Bomb Tower leaves
/// where it fell, which goes off a while later. Built from the three columns
/// `hitpointless_building` found on `raw`.
///
/// WHAT THE DATA CARRIES. `units.BalloonBomb`, `units.GiantSkeletonBomb` and
/// `units.BombTowerBomb` are BUILDING rows with no hitpoints, no damage, no hit
/// speed, no range, no speed and no LifeTime. Three columns are the whole row:
/// DeployTime 3000, DeathDamage (94 / 269 / 87 at level 1) and DeathDamageRadius
/// 3000. A row shaped like that is not a unit at all -- a unit with no hitpoints
/// cannot be born, which is why this loader refused all three, and with them the
/// three cards whose death leaves one. The file holds eight such rows; three are
/// reachable from a card today.
///
/// WHAT IT IS INSTEAD: one area hit, at the point where the parent died, on a
/// timer. So the row loads as an IMPACT WITH A DELAY rather than as an entity --
/// `summon_only` plus a `spell`, the pair `CardDef::death_bomb_fuse_ms` reads --
/// and state.rs `phase_reap` hands it to the same spell object an Arrows wave
/// waits in (`SpellMotion::Flight` with a delay, aimed at the point it is already
/// standing on). It is untargetable and collides with nothing because it is not on
/// the board at all, which is what a bomb with no hitpoints and no collision
/// behaviour can be held to.
///
/// THE IMPACT IS THE ENGINE'S OWN DEATH DAMAGE, DELAYED. The `SpellHit` below is
/// exactly what `phase_reap` already passes to `combat::splash` for an ordinary
/// DeathDamage row -- enemies only, both air and ground per the row's own columns,
/// crown towers at the row's percent, buildings included, no knockback -- so the
/// bomb is the same effect with a fuse on it, not a second damage law.
///
/// THE FUSE IS DeployTime, AND THAT IS MEASURED, not read off the column name.
/// 16.402 capture 20260920-083112, both seats (data/derived/replay/20260920-083112-
/// A and -B, entity key 28): a level-11 Balloon is last recorded alive on tick 1259
/// at (7349, 27895), and the King Tower 1987 native units away loses exactly 240
/// hitpoints on tick 1321 -- 61 ticks, 3050 ms, later -- with no unit of the
/// Balloon's own side within 4500 native of that tower on any tick from 1315 to
/// 1323. 240 is BalloonBomb's DeathDamage 94 on the Common ladder at unified level
/// 11 (256 %, floored: 240.64); the Balloon's own attack is 250 x 256 % = 640, so
/// the hit is the bomb's and nothing else's. That separates the two readings this
/// file could have taken: DeployTime as the fuse puts the damage 3000 ms after the
/// death, and the measured death-spawn default (calibration
/// spawner.DEATH_SPAWN_DEPLOY_TIME_DEFAULT = zero, which is about how long a
/// spawned UNIT takes to wake and has nothing to say about a bomb) would have put
/// it on the death tick. The corpus says 61 ticks, so `phase_reap` takes the row's
/// own DeployTime and never the death-spawn default.
///
/// DeathPushBack (cards.json `death_pushback_milli`; GiantSkeletonBomb 1800, the Skeleton
/// Barrel's container 1000) is loaded as `CardDef::death_pushback` and run on the bomb's
/// hit under knockback.DEATH_PUSHBACK (spell.rs `step_spells`). The shipped arm runs it on a
/// container alone, so the Giant Skeleton's bomb still damages without shoving.
///
/// A CONTAINER (`with_spawn`: `Hitpointless::BombWithDeathSpawn`, the Skeleton Barrel's
/// SkeletonContainerNew) also carries its row's DeathSpawn block. When the fuse ends the
/// bomb hits and releases that death spawn where it stands (spell.rs `FuseEnd`, state.rs
/// `release_fuse_end`), so the unit is returned as the container's own need, one level
/// down the unit worklist.
fn convert_death_bomb(raw: &RawCard, fuse_ms: i32, damage: i32, radius_milli: i32, with_spawn: bool) -> Result<(CardDef, UnitNeeds), String> {
    let crown_pct = crown(raw.crown_tower_damage_percent);
    let radius = milli(radius_milli);
    let (level_table, level_base) = level_table_of(raw.level_scaling.clone())?;
    let mut c = stat_less(raw.name.clone(), raw.rarity.clone().ok_or("missing rarity")?, 0);
    // The row IS a building row and says so; `summon_only` (set by the caller once
    // the unit is registered) is what keeps it out of every catalogue.
    c.kind = CardKind::Building;
    c.deploy_time_ms = fuse_ms;
    c.death_damage = damage;
    c.death_damage_radius = radius;
    c.crown_tower_damage_percent = crown_pct;
    c.level_table = level_table;
    c.level_base = level_base;
    c.death_pushback = match raw.death_pushback_milli {
        Some(x) if x < 0 => return Err(format!("death_pushback_milli {x} < 0")),
        Some(x) => milli(x),
        None => 0,
    };
    let mut needs: UnitNeeds = Vec::new();
    if with_spawn {
        let (ds, unit) = convert_death_spawn(raw.death_spawn.clone())?.ok_or("a container's death_spawn block names no DeathSpawnCharacter")?;
        c.death_spawn = Some(ds);
        // A blank (or a 2018 row, which never carries the key) is false, as in `convert`.
        #[cfg(not(clash_plant = "death_spawn_pushback_unread"))]
        {
            c.death_spawn_pushback = raw.death_spawn_pushback.unwrap_or(false);
        }
        needs.push((UnitUse::DeathSpawn, unit));
    }
    c.spell = Some(SpellDef {
        // `speed` is unread: `spell::step_spells` advances from the impact point to
        // the impact point, and `path::advance` arrives on a zero-length leg
        // whatever the speed. `waves` / `wave_interval_ms` are the cast-time wave
        // stagger, which no bomb has; the fuse rides on the ONE object `phase_reap`
        // makes, as its `delay_ms`.
        shape: SpellShape::Projectile {
            speed: 0,
            hit: Some(SpellHit {
                damage,
                crown_pct,
                radius,
                hits_air: raw.attacks_air.unwrap_or(true),
                hits_ground: raw.attacks_ground.unwrap_or(true),
                only_enemies: true,
                only_own_troops: false,
                ignore_buildings: false,
                no_effect_to_crown_towers: false,
                knockback: None,
                buff: None,
                buff2: None,
                caps_buff_time: false,
                controls_buff: false,
            }),
            waves: 1,
            wave_interval_ms: 0,
            spawn: None,
        },
        // Unread: a bomb is never cast, so it is never placed. `Anywhere` is the
        // inert value; `state.rs check_position` only ever sees catalogue cards.
        placement: SpellPlacement::Anywhere,
    });
    Ok((c, needs))
}

/// A DEATH PROJECTILE (characters DeathSpawnProjectile, a `projectiles` row; the Phoenix's
/// PhoenixFireball: Damage 64, Radius 2500, SpawnCharacter PhoenixEgg), read into the
/// `SpellShape::Projectile` a cast projectile is, so its arrival is one (spell.rs `step_spells`:
/// the impact, then the release). Returns the block and its release as the dying card's need.
///
/// WHAT IS READ, and what refuses the card instead of running as a plainer one: its Damage (at
/// the dying card's level, over Radius, air and ground and OnlyEnemies as the row says, crown
/// towers at its percent, Pushback as a spell projectile's), a TargetBuff with its BuffTime, and
/// its SpawnCharacter. A projectile that chains another projectile, caps its targets, leaves an
/// area effect or runs a mechanic graph is refused, and so is one that neither deals damage nor
/// releases a unit.
///
/// TWO BLANKS READ AS THE GAME'S, measured on client 15.535.29's Phoenix scenarios: a blank
/// SpawnCharacterCount is ONE (one egg in all three runs; a blank DeathSpawnCount is one too,
/// `convert_death_spawn`), and a blank SpawnCharacterDeployTime is ZERO -- the egg hatched 76
/// ticks after it appeared, its SpawnStartTime of 3800 ms, which under the shipped
/// spawner.START_TIME_ORIGIN = from_activation needs the egg active on its first tick (its own
/// DeployTime of 1000 would put the hatch at 96). `speed` is kept and unread: the projectile
/// stands on the point it is aimed at (state.rs `phase_reap`).
fn convert_death_projectile(p: &RawSpellProjectile, buffs: &mut BuffTable) -> Result<(SpellDef, UnitNeeds), String> {
    let what = p.name.clone().unwrap_or_default();
    refuse_action_mechanic(&p.action_graph, &format!("death projectile {what}"))?;
    if p.spawn_projectile.is_some() || p.maximum_targets.is_some() || p.spawn_area_effect_object.is_some() {
        return Err(format!("death projectile {what} with a chained projectile, a target cap or an area effect is not simulated"));
    }
    let target_buff = match &p.target_buff {
        None => None,
        Some(v) if v.is_null() => None,
        Some(v) => {
            let b: RawBuff = serde_json::from_value(v.clone()).map_err(|e| format!("death projectile {what} TargetBuff: {e}"))?;
            Some(buffs.apply(&b, p.buff_time_ms, &format!("death projectile {what}"))?)
        }
    };
    let hit = match p.damage {
        None if target_buff.is_some() => return Err(format!("death projectile {what} carries a TargetBuff and no damage; not simulated")),
        None => None,
        Some(d) => Some(SpellHit {
            damage: d,
            crown_pct: crown(p.crown_tower_damage_percent),
            radius: milli(p.radius_milli.filter(|r| *r > 0).ok_or_else(|| format!("death projectile {what} deals damage but has no radius"))?),
            hits_air: p.aoe_to_air.unwrap_or(false),
            hits_ground: p.aoe_to_ground.unwrap_or(false),
            only_enemies: p.only_enemies.unwrap_or(false),
            only_own_troops: false,
            ignore_buildings: false,
            no_effect_to_crown_towers: false,
            knockback: knockback(p.pushback_milli, p.pushback_all),
            buff: target_buff,
            buff2: None,
            caps_buff_time: false,
            controls_buff: false,
        }),
    };
    let mut units: UnitNeeds = Vec::new();
    let spawn = match p.spawn_character.clone() {
        None => None,
        Some(unit) => {
            let count = p.spawn_character_count.unwrap_or(1);
            if count < 1 || p.spawn_character_deploy_time_ms.is_some_and(|d| d < 0) {
                return Err(format!("death projectile {what}: SpawnCharacterCount {count} / SpawnCharacterDeployTime {:?} out of range", p.spawn_character_deploy_time_ms));
            }
            units.push((UnitUse::DeathProjectileRelease, unit));
            Some(SpawnDef {
                unit: u16::MAX, // resolved by from_json_str
                count,
                deploy_time_ms: Some(p.spawn_character_deploy_time_ms.unwrap_or(0)),
                level_index: p.spawn_character_level_index,
            })
        }
    };
    if hit.is_none() && spawn.is_none() {
        return Err(format!("death projectile {what} neither deals damage nor releases a unit"));
    }
    let shape = SpellShape::Projectile { speed: p.speed.unwrap_or(0), hit, waves: 1, wave_interval_ms: 0, spawn };
    // Unread, as a death bomb's: a death is not a cast and has no tap to validate.
    Ok((SpellDef { shape, placement: SpellPlacement::Anywhere }, units))
}

fn convert(raw: RawCard, buffs: &mut BuffTable, ctx: &LoadCtx) -> Result<Converted, String> {
    if raw.kind == CardKind::Spell {
        let display = raw.display_name.clone();
        #[cfg(clash_plant = "spells_rejected")]
        {
            // PLANT (regression): a loader that refuses every spell.
            let _ = display;
            return Err("spells are not simulated yet".into());
        }
        #[allow(unreachable_code)]
        return convert_spell(raw, buffs, ctx).map(|(c, units)| (c, display, units));
    }
    // A HITPOINT-LESS BUILDING (a death bomb) is not a unit and is not loaded like
    // one: no hitpoints, no hit speed and no range to require of it
    // (`hitpointless_building`).
    if let Some(shape) = hitpointless_building(&raw) {
        let (c, needs) = match shape {
            Hitpointless::DeathBomb { fuse_ms, damage, radius_milli } => convert_death_bomb(&raw, fuse_ms, damage, radius_milli, false)?,
            // A container's death spawn is its own need, one level down (the Skeleton Barrel's Skeletons).
            Hitpointless::BombWithDeathSpawn { fuse_ms, damage, radius_milli } => convert_death_bomb(&raw, fuse_ms, damage, radius_milli, true)?,
            // A bottle is a spell summon's `Fuse` (`CardDb::from_json_str`, UnitUse::SpellSummon),
            // never a unit on the board.
            Hitpointless::Bottle { .. } => return Err("a hitpoint-less building whose death leaves an area is a spell summon's bottle; only a spell summon releases one".into()),
        };
        return Ok((c, raw.display_name.clone(), needs));
    }
    // A SECOND PERIODIC UNIT (SpawnCharacter2, the Super Witch's Bat) is not simulated. Refused here, before anything
    // of the row is interned, so a refused row adds no buff to the table and moves no other card's buff index.
    if let Some(c2) = raw.spawner.as_ref().and_then(|s| s.character2.as_deref()) {
        return Err(format!("spawner SpawnCharacter2 {c2}: a second periodic unit is not simulated"));
    }
    // THE ACTION BLOCKS. A unit may run an action graph only when the graph is exactly one block the loader reads: the
    // Goblin Hut's controller (a `life_state_spawner` block, `life_state_of`), the Furnace's interval spawner (an
    // `interval_spawner` block, `interval_spawner_of`), the Three Musketeers' attack selector (an `attack_select` block,
    // `attack_select_of`), the Rune Giant's enchant (an `enchant_friends` block, `enchant_of`), a health-threshold
    // transformation (a `transform_at_hp` block, `transform_of`) or a counter (a `parry` block, `parry_of`), each with
    // its cosmetic hooks. Every other graph is refused as before, and so is a row that carries more than one block.
    let blocks: Vec<&str> = [
        (raw.life_state_spawner.is_some(), "a life-state controller"),
        (raw.interval_spawner.is_some(), "an interval spawner"),
        (raw.attack_select.is_some(), "an attack selector"),
        (raw.enchant_friends.is_some(), "an enchant"),
        (raw.transform_at_hp.is_some(), "a transformation"),
        (raw.parry.is_some(), "a counter"),
    ]
    .into_iter()
    .filter_map(|(has, what)| has.then_some(what))
    .collect();
    if blocks.len() > 1 {
        let (last, first) = blocks.split_last().expect("more than one");
        return Err(format!("the unit carries more than one action block ({} and {last}); not simulated", first.join(", ")));
    }
    if blocks.is_empty() {
        refuse_action_mechanic(&raw.action_graph, "the unit")?;
    }
    let life_state = raw.life_state_spawner.as_ref().map(|ls| life_state_of(ls, &raw.action_graph)).transpose()?;
    let interval = raw.interval_spawner.as_ref().map(|iv| interval_spawner_of(iv, &raw.action_graph)).transpose()?;
    let attack_select = raw.attack_select.as_ref().map(|sel| attack_select_of(sel, &raw.action_graph)).transpose()?;
    let enchant = raw.enchant_friends.as_ref().map(|ef| enchant_of(ef, &raw.action_graph)).transpose()?;
    let transform = raw.transform_at_hp.as_ref().map(|tr| transform_of(tr, &raw.action_graph)).transpose()?;
    let parry = raw.parry.as_ref().map(|p| parry_of(p, &raw.action_graph, buffs)).transpose()?;
    // THE UNDERGROUND SPAWN WALK: read, or refused with the shape it has (`spawn_pathfind_of`). The
    // morph target is a need of the card, loaded from `units` by the unit loop.
    let can_deploy_on_enemy_side = raw.can_deploy_on_enemy_side.unwrap_or(false);
    let spawn_pathfind = spawn_pathfind_of(&raw.spawn_pathfind, can_deploy_on_enemy_side, "the unit")?;
    let tunnels = spawn_pathfind.is_some();
    // INVISIBLE WHEN IDLE: the idle time, with area damage still landing (AllowAreaDmgWhenInvisible) -- the one
    // reading the engine runs (target.rs `can_target`); a row that would keep area damage off is refused. A BLANK idle
    // time is read on a kamikaze with no KamikazeTime alone (the Suspicious Bush), as 0: its first hit is its death, so
    // it is never revealed and the time is never read. Measured on client 15.535.29: nothing targeted a Bush from its
    // first frame, and each died on its hit. Any other blank time is refused: that unit would be revealed by its hit
    // and would need the time.
    let dies_on_its_hit = raw.kamikaze.unwrap_or(false) && raw.kamikaze_time_ms.is_none();
    let invisible_when_idle = match &raw.idle_invisibility {
        None => None,
        Some(iv) => {
            if !iv.area_damage_when_invisible.unwrap_or(false) {
                return Err("an invisibility that keeps area damage off is not simulated".into());
            }
            match iv.time_ms {
                Some(t) if t > 0 => Some(t),
                _ if dies_on_its_hit => Some(0),
                _ => return Err("an invisibility with no BuffWhenNotAttackingTime on a unit that survives its hit".into()),
            }
        }
    };
    let need = |v: Option<i32>, what: &str| v.ok_or_else(|| format!("missing {what}"));
    let mut damage = raw.damage;
    let mut attack_buff: Option<BuffApply> = None;
    let mut attack_buff_first = false;
    let mut projectile_homing = false;
    let mut projectile_name: Option<String> = None;
    let mut range_shot: Option<RangeShotDef> = None;
    let mut spark: Option<SparkDef> = None;
    let mut projectile_area_need: Option<String> = None;
    let projectile = match raw.projectile {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Object(o)) => {
            let p: RawProjectileObj =
                serde_json::from_value(serde_json::Value::Object(o)).map_err(|e| format!("projectile: {e}"))?;
            // A ranged attack's damage lives on its PROJECTILE row (characters.csv
            // ships no Damage for Archer, Musketeer, Wizard, ...; the towers'
            // building rows ship none either). When the projectile carries a
            // damage it is authoritative, whatever the card-level field says.
            // Taking the card-level damage first and falling back to the
            // projectile only when it is null is WRONG: on a card whose character
            // row and projectile row disagree, that order fires the character
            // number. Pinned by mechanics::ranged_damage_comes_from_the_
            // projectile_and_lands_on_arrival.
            #[cfg(not(clash_plant = "character_damage_wins"))]
            if p.damage.is_some() {
                damage = p.damage;
            }
            #[cfg(clash_plant = "character_damage_wins")]
            if damage.is_none() {
                // PLANT: the pre-fix precedence.
                damage = p.damage;
            }
            // TargetBuff + BuffTime: the Ice Spirit's Freeze (1100 ms), the Ice
            // Wizard's IceWizardSlowDown (2500 ms). It goes on the CARD rather than on
            // ProjectileDef, because ProjectileDef's Debug is inside the format-3 card
            // fingerprint (state.rs migrate_v3) and CardDef's tail is not.
            if let Some(b) = &p.target_buff {
                attack_buff = Some(buffs.apply(b, p.buff_time_ms, "the unit's projectile")?);
            }
            // ApplyBuffBeforeDamage (the Mother Witch's): the buff lands before the damage
            // (status.APPLY_BUFF_BEFORE_DAMAGE).
            attack_buff_first = p.apply_buff_before_damage.unwrap_or(false);
            projectile_homing = p.homing.unwrap_or(true);
            projectile_name = p.name.clone();
            range_shot = range_shot_of(&p);
            spark = p.spawn_projectile.as_ref().and_then(spark_of);
            #[cfg(not(clash_plant = "projectile_area_unread"))]
            if let Some(a) = &p.spawn_area_effect_object {
                projectile_area_need = Some(a.clone());
            }
            Some(ProjectileDef {
                speed: p.speed.ok_or("projectile without speed")?,
                radius: milli(p.radius_milli.unwrap_or(0)),
            })
        }
        Some(other) => return Err(format!("projectile given as {other} carries no speed; need an object")),
    };
    let (level_table, level_base) = level_table_of(raw.level_scaling)?;
    // THE CARD'S KIND IS THE ROW IT PUTS ON THE BOARD (cards.json `kind`), and the table it is listed in
    // (`card_table_kind`, written only where the two differ) is a record. The one pair the extractor writes is a
    // building card whose unit is a troop (the Furnace); any other pair is a shape nothing here was written for.
    if let Some(t) = raw.card_table_kind {
        if (t, raw.kind) != (CardKind::Building, CardKind::Troop) {
            return Err(format!("a {t:?} card whose unit is a {:?} is not simulated", raw.kind));
        }
    }
    #[cfg(not(clash_plant = "card_table_kind"))]
    let kind = raw.kind;
    #[cfg(clash_plant = "card_table_kind")]
    let kind = raw.card_table_kind.unwrap_or(raw.kind); // PLANT (regression): the kind of the table the card is listed in.
    // Spells do not reach here: `convert_spell` above handles them
    // (plant: spells_rejected).
    // A NON-ATTACKING BUILDING ships no Range at all: the 2018 huts (Tombstone,
    // GoblinHut, BarbarianHut, FirespiritHut) have no Damage, no Projectile and a
    // blank Range (their HitSpeed 10000 is inert). Loaded as range 0 / sight 0 AND
    // attacks nothing (attacks_ground / attacks_air forced false below): the
    // edge-to-edge range test (fixed.rs in_range_edge adds the target's radius)
    // would otherwise acquire a unit standing inside the footprint -- a Goblin
    // Barrel's goblin on a Tombstone -- and run a zero-damage attack cycle. Any
    // card WITH a damage source still needs its range; requiring range_milli of
    // every non-spell card refused every spawner building.
    let attacks = raw.damage.is_some() || projectile.is_some();
    // Buildings only: a damage-less TROOP row (the Skeleton Barrel, which attacks for 0
    // and dies by its KamikazeTime) keeps its columns as loaded, so the format-3 card
    // fingerprint (state.rs migrate_v3) still reproduces.
    let inert_building = kind == CardKind::Building && !attacks;
    let range = match raw.range_milli {
        Some(r) => r,
        None if inert_building => 0,
        None => return Err("missing range_milli".into()),
    };
    // BuffOnDamage: a unit whose own hit buffs its victim without a projectile (the
    // Electro Wizard's ZapFreeze, 500 ms). A card that ships this beside a projectile
    // TargetBuff names ONE buff twice when the two are the same row for the same time (the
    // Mother Witch's VoodooCurse, 5000 ms both): one attack buff. Two different buffs would
    // be a shape this loader does not read.
    if let Some(bod) = &raw.buff_on_damage {
        let b = bod.buff.as_ref().ok_or("buff_on_damage without a buff")?;
        let got = buffs.apply(b, bod.time_ms, "the unit's BuffOnDamage")?;
        #[cfg(not(clash_plant = "attack_buff_pair_refused"))]
        let same = attack_buff == Some(got);
        #[cfg(clash_plant = "attack_buff_pair_refused")]
        let same = false; // PLANT (regression): a TargetBuff and a BuffOnDamage naming one buff are refused as two.
        if attack_buff.is_some() && !same {
            return Err("the unit carries both a projectile TargetBuff and a BuffOnDamage; not simulated".into());
        }
        attack_buff = Some(got);
    }
    // THE SPAWN* BLOCK: a periodic spawner, or, with SpawnAttach, the riders the unit carries
    // (`convert_attach`). A building carrying riders is a shape no row ships.
    let (spawner, attach) = match raw.spawner {
        Some(b) if b.attach == Some(true) => (None, Some(convert_attach(b)?)),
        other => (convert_spawner(other)?, None),
    };
    if attach.is_some() && kind != CardKind::Troop {
        return Err("an attached rider on a building is not simulated".into());
    }
    // THE PERIODIC SPAWNER: the Spawn* columns, or the interval spawner read above (a `SpawnerDef` of source
    // ActionInterval, so the one spawner pass runs both). A row with both, or with riders beside an interval
    // spawner, is a shape nothing here was written for.
    let spawner = match (spawner, interval) {
        (Some(_), Some(_)) => return Err("the unit carries a Spawn* block and an interval spawner; not simulated".into()),
        (None, Some(_)) if attach.is_some() => return Err("the unit carries attached riders and an interval spawner; not simulated".into()),
        (a, b) => a.or(b),
    };
    let mana = convert_mana(raw.mana, kind)?;
    // TargetOnlyTroops (the Ram Rider's rider), never beside TargetOnlyBuildings.
    let target_only_troops = raw.target_only_troops.unwrap_or(false);
    if target_only_troops && raw.target_only_buildings.unwrap_or(false) {
        return Err("a unit that targets only troops and only buildings".into());
    }
    // IgnoreTargetsWithBuff is read with DeprioritizeTargetsWithBuff only: the Ram Rider's rider
    // ranks snared troops last. The rows that set it alone (a tower and a neutral unit of the event
    // modes) ignore the buff's carriers outright, which no loaded row does, and are refused rather
    // than run as rows that rank them. Interned by value, so it is the index the bola's TargetBuff
    // already has.
    let deprioritize_buff = match (&raw.ignore_targets_with_buff, raw.deprioritize_targets_with_buff.unwrap_or(false)) {
        (None, false) => None,
        (Some(b), true) => {
            let def = b.convert("the unit's IgnoreTargetsWithBuff")?;
            let death = b.death_spawn.as_ref().and_then(|d| d.character.as_deref());
            Some(buffs.intern(def, b.name.as_deref().unwrap_or(""), death)?)
        }
        (Some(_), false) => return Err("a unit that ignores every target carrying a buff (IgnoreTargetsWithBuff without DeprioritizeTargetsWithBuff) is not simulated".into()),
        (None, true) => return Err("DeprioritizeTargetsWithBuff with no IgnoreTargetsWithBuff".into()),
    };
    let death_spawn = convert_death_spawn(raw.death_spawn)?;
    let charge = convert_charge(raw.charge, kind)?;
    let jump = convert_jump(raw.jump, kind)?;
    let dash = convert_dash(raw.dash, kind)?;
    let reflect = convert_reflect(raw.reflected_attack, buffs)?;
    // The two blocks and the one column the special attacks read (combat.VARIABLE_DAMAGE,
    // combat.SPECIAL_HOOK, knockback.ATTACK_PUSHBACK). Each is inert under its key's old
    // arm; a half-blank block refuses the card like any other.
    let variable_damage = convert_variable_damage(raw.variable_damage)?;
    let special = convert_special(raw.special, kind)?;
    let attack_pushback = match raw.attack_pushback_milli {
        Some(x) if x < 0 => return Err(format!("attack_pushback_milli {x} < 0")),
        Some(x) => milli(x),
        None => 0,
    };
    let mut units: Vec<(UnitUse, String)> = Vec::new();
    if let Some((_, name)) = &life_state {
        units.push((UnitUse::LifeState, name.clone()));
    }
    if let Some((_, name)) = &transform {
        units.push((UnitUse::Transform, name.clone()));
    }
    if let Some((_, u)) = &spawner {
        units.push((UnitUse::Spawner, u.clone()));
    }
    if let Some((_, u)) = &attach {
        units.push((UnitUse::Attach, u.clone()));
    }
    if let Some((_, u)) = &death_spawn {
        units.push((UnitUse::DeathSpawn, u.clone()));
    }
    if let Some(aeo) = &raw.death_area_effect {
        units.push((UnitUse::DeathAreaEffect, aeo.clone()));
    }
    // THE PROJECTILE'S AREA: one area per card at most, so `spell::shape_of` stays one answer.
    if let Some(a) = projectile_area_need {
        if raw.death_area_effect.is_some() {
            return Err("a card whose death and projectile both leave areas is not simulated".into());
        }
        units.push((UnitUse::ProjectileArea, a));
    }
    // THE SECOND SUMMON: all-or-nothing like the other blocks (a character without a
    // count, or a count without a character, is a data error). An overlay-resolved
    // card (ThreeMusketeers: SummonCharactersList with its own offsets table, the
    // layout formation.rs does not model) keeps its list inside `count` and its
    // `second_summon` is the extractor's carrier for the other entries, so it is
    // not a SummonCharacterSecond and is left unread here.
    let second_summon = match (&raw.second_summon, raw.summon_resolution.is_some()) {
        (None, _) | (Some(RawSecondSummon { character: None, count: None }), _) => None,
        (Some(_), true) => None,
        (Some(RawSecondSummon { character: Some(u), count: Some(n) }), false) => {
            if *n < 1 {
                return Err(format!("second_summon {u}: count {n} < 1"));
            }
            units.push((UnitUse::SecondSummon, u.clone()));
            Some(SecondSummonDef { unit: u16::MAX, count: *n })
        }
        (Some(b), false) => return Err(format!("second_summon block half blank: {:?} / {:?}", b.character, b.count)),
    };
    // THE ATTACK SELECTOR'S ranged entry is the row's own Projectile: a selector on a row with none is a shape the
    // extractor does not write.
    if attack_select.is_some() && projectile.is_none() {
        return Err("the unit's attack selector: no projectile for its ranged entry; not simulated".into());
    }
    // THE DEPLOY AT EXPLICIT OFFSETS (`summon_members`; the Three Musketeers): all-or-nothing like the other blocks.
    // One member per summon (`count`), member 0 the card's own row, every member with its character and both offsets;
    // members 1.. are unit needs (`UnitUse::SummonMember`), member 0 is filled with the card's own index at its push.
    // A line layout or a second summon beside the members is a layout nobody measured, refused.
    let summon_members = match &raw.summon_members {
        None => None,
        Some(ms) => {
            let n = raw.count.unwrap_or(1);
            // One member at an offset is a shape no row ships (a single summon stands on its tap).
            if ms.len() < 2 || ms.len() > u8::MAX as usize || ms.len() as i32 != n {
                return Err(format!("summon_members: {} members for a card of {n}", ms.len()));
            }
            if raw.summon_width_milli.is_some_and(|w| w != 0) || second_summon.is_some() {
                return Err("summon_members beside a SummonWidth line or a second summon is not simulated".into());
            }
            let mut out = Vec::with_capacity(ms.len());
            for (k, m) in ms.iter().enumerate() {
                let c = m.character.clone().filter(|c| !c.is_empty()).ok_or_else(|| format!("summon_members[{k}]: no character"))?;
                let (Some(offset_x), Some(offset_y)) = (m.offset_x_milli, m.offset_y_milli) else {
                    return Err(format!("summon_members[{k}] {c}: an offset is blank"));
                };
                if k == 0 {
                    if raw.summon_character.as_deref() != Some(c.as_str()) {
                        return Err(format!("summon_members[0] {c} is not the card's own unit {:?}", raw.summon_character));
                    }
                } else {
                    units.push((UnitUse::SummonMember(k as u8), c));
                }
                out.push(SummonMemberDef { unit: u16::MAX, offset_x, offset_y });
            }
            Some(out)
        }
    };
    // A CARD THAT TRAVELS UNDER GROUND is played as ONE unit that walks to the tap and comes up there (state.rs
    // `enqueue`, `spawn_tunneller`, `surface`), and that is the shape measured: the Miner, a troop of one, and the
    // Goblin Drill, a building whose dig morphs into it. Anything else beside the walk would be played as a different
    // card, so it is refused: a count other than 1 (members at explicit offsets need a count of 2 or more, so this
    // refuses them too), a second summon, attached riders, and a building with no morph, which would walk and come up
    // as a troop.
    if let Some((_, morph)) = &spawn_pathfind {
        let count = raw.count.unwrap_or(1);
        let beside = if count != 1 {
            Some(format!("a count of {count}"))
        } else if second_summon.is_some() {
            Some("a second summon".to_string())
        } else if attach.is_some() {
            Some("attached riders".to_string())
        } else if kind == CardKind::Building && morph.is_none() {
            Some("a building with no SpawnPathfindMorph".to_string())
        } else {
            None
        };
        if let Some(what) = beside {
            return Err(format!("{what} beside the underground walk is not simulated"));
        }
    }
    // The death projectile, the deploy area and the spawn area: each a NAME resolved against the
    // file's own tables by `from_json_str`, after every unit need above (`death_area_effect`'s way).
    if let Some(p) = &raw.death_spawn_projectile {
        units.push((UnitUse::DeathProjectile, p.clone()));
    }
    if let Some(a) = &raw.deploy_area_effect {
        units.push((UnitUse::DeployAreaEffect, a.clone()));
    }
    if let Some(a) = &raw.spawn_area_object {
        units.push((UnitUse::SpawnAreaEffect, a.clone()));
    }
    if let Some((_, Some(m))) = &spawn_pathfind {
        units.push((UnitUse::Morph, m.clone()));
    }
    let nonneg = |v: Option<i32>, what: &str| match v {
        Some(x) if x < 0 => Err(format!("{what} {x} < 0")),
        Some(x) => Ok(x),
        None => Ok(0),
    };
    let formation = FormationDef {
        summon_radius: milli(nonneg(raw.summon_radius_milli, "summon_radius_milli")?),
        summon_width: milli(nonneg(raw.summon_width_milli, "summon_width_milli")?),
        summon_deploy_delay_ms: nonneg(raw.summon_deploy_delay_ms, "summon_deploy_delay_ms")?,
        summon_deploy_delay_second_ms: nonneg(raw.summon_deploy_delay_second_ms, "summon_deploy_delay_second_ms")?,
        spawn_radius: milli(nonneg(raw.spawn_radius_milli, "spawn_radius_milli")?),
        spawn_angle_shift_deg: raw.spawn_angle_shift_deg.unwrap_or(0),
        second_summon,
        spawn_max_angle_deg: nonneg(raw.spawn_max_angle_deg, "spawn_max_angle_deg")?,
    };
    // KAMIKAZE (combat.KAMIKAZE_DEATH = at_fire): the death ON the fire. A DELAYED one
    // (KamikazeTime, the Skeleton Barrel's 500 ms) is not that death: `kamikaze` stays
    // false for it and the delay is carried as `kamikaze_time_ms`, which
    // combat.KAMIKAZE_TIME runs (state.rs `kamikaze_drain`: the unit drains from its first
    // fire). Under that key's not_taken arm the column is not read, and the unit keeps
    // attacking, as the engine did before the key.
    let kamikaze = raw.kamikaze.unwrap_or(false) && raw.kamikaze_time_ms.unwrap_or(0) <= 0;
    let kamikaze_time_ms = if raw.kamikaze.unwrap_or(false) { raw.kamikaze_time_ms.filter(|t| *t > 0).unwrap_or(0) } else { 0 };
    let death_pushback = match raw.death_pushback_milli {
        Some(x) if x < 0 => return Err(format!("death_pushback_milli {x} < 0")),
        Some(x) => milli(x),
        None => 0,
    };
    let no_deploy_size = match raw.no_deploy_size_tiles {
        Some([w, h]) if w > 0 && h > 0 => Some(Vec2::new(tiles(w), tiles(h))),
        Some(other) => return Err(format!("no_deploy_size_tiles {other:?} is not two positive tile counts")),
        None => None,
    };
    let display = raw.display_name.clone();
    // The hide block is all-or-nothing: a hiding card ships both timers or is
    // refused; a non-hiding card ships neither (a stray timer is a data error, not
    // a mechanic to guess at). Only a BUILDING hides: the machinery keys on
    // `EntityKind::Building` too, so a hiding troop would load and then never hide.
    let hide = match (raw.hides_when_not_attacking.unwrap_or(false), raw.hide_time_ms, raw.up_time_ms) {
        (false, None, None) => None,
        (false, h, u) => return Err(format!("hide_time_ms {h:?} / up_time_ms {u:?} on a card that does not hide")),
        (true, Some(h), Some(u)) if h >= 0 && u >= 0 && kind == CardKind::Building => Some(HideDef { hide_time_ms: h, up_time_ms: u }),
        (true, h, u) => {
            return Err(format!("hides_when_not_attacking needs non-negative hide_time_ms and up_time_ms on a building; got {h:?} / {u:?} on a {kind:?}"))
        }
    };
    // THE UNIT ROW'S RAW COLUMNS. cards.json carries MultipleProjectiles,
    // CustomFirstProjectile, MultipleTargets and AllTargetsHit only in the unit row's `raw`
    // block (`units.<unit>.raw`), not in the typed card record, so they are read from
    // there, keyed by the row the card puts on the board (`unit_name`). A file without the
    // block (or a unit with no row) reads them all blank. Loaded whatever the calibration
    // says: the combat.MULTIPLE_PROJECTILES / CUSTOM_FIRST_PROJECTILE / MULTIPLE_TARGETS
    // arms decide whether anything reads them.
    let unit_raw = ctx.units.get(raw.summon_character.as_deref().unwrap_or(raw.name.as_str())).and_then(|u| u.get("raw"));
    let raw_count = |col: &str| -> Result<i32, String> {
        match unit_raw.and_then(|r| r.get(col)) {
            None | Some(serde_json::Value::Null) => Ok(1),
            Some(v) => v.as_i64().and_then(|n| i32::try_from(n).ok()).filter(|n| *n >= 1).ok_or_else(|| format!("{col} {v} is not a count of at least 1")),
        }
    };
    let multiple_projectiles = raw_count("MultipleProjectiles")?;
    let multiple_targets = raw_count("MultipleTargets")?;
    let all_targets_hit = unit_raw.and_then(|r| r.get("AllTargetsHit")).and_then(serde_json::Value::as_bool).unwrap_or(false);
    // A CustomFirstProjectile naming the row's own Projectile (the Hunter) is no second row.
    let custom_first_projectile = match unit_raw.and_then(|r| r.get("CustomFirstProjectile")).and_then(serde_json::Value::as_str) {
        None => None,
        Some(name) if projectile_name.as_deref() == Some(name) => None,
        Some(name) => Some(custom_shot_of(name, ctx.projectiles)?),
    };
    let deploy_projectile = match raw.deploy_projectile {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => {
            if raw.death_area_effect.is_some() {
                return Err("the unit carries both a deploy projectile and a death area effect; not simulated (spell.rs `shape_of` names one)".into());
            }
            Some(convert_deploy_projectile(v)?)
        }
    };
    Ok((CardDef {
        unit_name: raw.summon_character.clone().unwrap_or_else(|| raw.name.clone()),
        name: raw.name,
        kind,
        elixir: raw.elixir.unwrap_or(0),
        rarity: raw.rarity.ok_or("missing rarity")?,
        hitpoints: need(raw.hitpoints, "hitpoints")?,
        damage: damage.unwrap_or(0),
        // THE INERT BUILDING THAT PRODUCES (the Elixir Collector): a building with no damage source and a `mana`
        // block ships no HitSpeed and needs none (the Target and Attack passes skip a hit speed of 0), so it loads
        // as 0. Every other row still needs the column: the 2018 Elixir Collector, whose file carries no `mana`,
        // stays refused, and so does a building that produces nothing.
        hit_speed_ms: match raw.hit_speed_ms {
            Some(h) => h,
            None if inert_building && mana.is_some() => 0,
            None => return Err("missing hit_speed_ms".into()),
        },
        load_time_ms: raw.load_time_ms.unwrap_or(0),
        speed: raw.speed.unwrap_or(0),
        range: milli(range),
        // Towers ship no separate sight: they see exactly as far as they shoot.
        sight_range: milli(raw.sight_range_milli.unwrap_or(range)),
        collision_radius: milli(need(raw.collision_radius_milli, "collision_radius_milli")?),
        mass: raw.mass,
        deploy_time_ms: raw.deploy_time_ms.unwrap_or(0),
        attacks_air: raw.attacks_air.unwrap_or(false) && !inert_building,
        attacks_ground: raw.attacks_ground.unwrap_or(true) && !inert_building,
        target_only_buildings: raw.target_only_buildings.unwrap_or(false),
        flying_height: raw.flying_height.unwrap_or(0),
        area_damage_radius: milli(raw.area_damage_radius_milli.unwrap_or(0)),
        projectile,
        count: raw.count.unwrap_or(1).max(1),
        shield_hitpoints: raw.shield_hitpoints.unwrap_or(0),
        crown_tower_damage_percent: raw.crown_tower_damage_percent.unwrap_or(100),
        death_damage: raw.death_damage.unwrap_or(0),
        death_damage_radius: milli(raw.death_damage_radius_milli.unwrap_or(0)),
        self_as_aoe_center: raw.self_as_aoe_center.unwrap_or(false),
        lifetime_ms: raw.lifetime_ms,
        level_table,
        no_deploy_size,
        stop_movement_after_ms: raw.stop_movement_after_ms.unwrap_or(0),
        wait_ms: raw.wait_ms.unwrap_or(0),
        ignore_pushback: raw.ignore_pushback.unwrap_or(false),
        spell: None,
        summon_only: false,
        hide,
        spawner: spawner.map(|(d, _)| d),
        death_spawn: death_spawn.map(|(d, _)| d),
        charge,
        jump,
        level_base,
        formation,
        projectile_start_radius: milli(nonneg(raw.projectile_start_radius_milli, "projectile_start_radius_milli")?),
        kamikaze,
        attack_buff,
        projectile_homing,
        // Resolved by `CardDb::from_json_str` against the file's `area_effect_objects`
        // table, with the card's `death_area_effect` name (pushed on `units` above).
        death_area_effect: None,
        // A blank (or a 2018 row, which never carries the key) is false.
        #[cfg(not(clash_plant = "death_spawn_pushback_unread"))]
        death_spawn_pushback: raw.death_spawn_pushback.unwrap_or(false),
        #[cfg(clash_plant = "death_spawn_pushback_unread")]
        death_spawn_pushback: false, // PLANT: the loader drops the column, so no row slides.
        dash,
        reflect,
        range_shot,
        multiple_projectiles,
        custom_first_projectile,
        multiple_targets,
        all_targets_hit,
        deploy_projectile,
        load_first_hit: raw.load_first_hit.unwrap_or(false),
        variable_damage,
        attack_pushback,
        special,
        // Resolved by `CardDb::from_json_str` (the names pushed on `units` above).
        death_projectile: None,
        deploy_area_effect: None,
        spawn_area_effect: None,
        hovering: raw.hovering.unwrap_or(false),
        // MinimumRange: a blank (every row but the Mortar family's) is none.
        #[cfg(not(clash_plant = "minimum_range_unread"))]
        minimum_range: milli(nonneg(raw.minimum_range_milli, "minimum_range_milli")?),
        #[cfg(clash_plant = "minimum_range_unread")]
        minimum_range: 0, // PLANT (regression): the loader drops the column, so the Mortar shoots at its own feet.
        spark,
        projectile_area: None,
        life_state: life_state.map(|(d, _)| d),
        invisible_when_idle,
        // Its morph resolved by `CardDb::from_json_str` (the name pushed on `units` above).
        spawn_pathfind: spawn_pathfind.map(|(d, _)| d),
        // Read with `spawn_pathfind` alone: a card that does not tunnel keeps false.
        can_deploy_on_enemy_side: can_deploy_on_enemy_side && tunnels,
        mana,
        omit_from_starting_hand: raw.omit_from_starting_hand.unwrap_or(false),
        // Resolved by `CardDb::from_json_str` (the name pushed on `units` above).
        attach: attach.map(|(d, _)| d),
        target_only_troops,
        deprioritize_buff,
        // Member 0's unit is the card's own index, filled at its push (`CardDb::from_json_str`); the others resolve
        // with the unit needs pushed above.
        summon_members,
        summon_offsets_x_mirrored: raw.summon_offsets_x_mirrored.unwrap_or(false),
        attack_select,
        // Resolved by `CardDb::from_json_str` once every buff is interned (the names in `RawCard::ignore_buffs`).
        ignore_buffs: Vec::new(),
        attack_buff_first,
        // Its multipliers and exclusions are resolved against the loaded cards by `CardDb::from_json_str`.
        enchant,
        // Resolved by `CardDb::from_json_str` (the row's name pushed on `units` above).
        transform_at_hp: transform.map(|(d, _)| d),
        parry,
        kamikaze_time_ms,
        death_pushback,
        ignore_clone: raw.ignore_clone.unwrap_or(false),
    }, display, units))
}

/// The straight-to-range block of a troop's projectile row (`RangeShotDef`), or None when
/// the row lacks a positive ProjectileRange or a positive ProjectileRadius.
fn range_shot_of(p: &RawProjectileObj) -> Option<RangeShotDef> {
    let range = p.projectile_range_milli.filter(|r| *r > 0)?;
    let reach = p.projectile_radius_milli.filter(|r| *r > 0)?;
    Some(RangeShotDef {
        range: milli(range),
        reach: milli(reach),
        hits_air: p.aoe_to_air.unwrap_or(false),
        hits_ground: p.aoe_to_ground.unwrap_or(false),
        only_enemies: p.only_enemies.unwrap_or(false),
        knockback: knockback(p.pushback_milli, p.pushback_all),
        pingpong_ms: p.pingpong_visual_time_ms.filter(|t| *t > 0),
        check_collisions: p.check_collisions.unwrap_or(false),
        start_extra: milli(p.projectile_start_extra_radius_milli.unwrap_or(0).max(0)),
        random_delay_ms: p.random_delay_ms.unwrap_or(0).max(0),
    })
}

/// A unit's CustomFirstProjectile row, `name`, from the file's `projectiles` table
/// (`CustomShotDef`). Refused by name when the table has no such row, and when the row
/// carries a mechanic the shot does not run (a spawn, a target cap, a pushback, a buff):
/// a card is refused rather than run without a mechanic it carries.
fn custom_shot_of(name: &str, table: &BTreeMap<String, serde_json::Value>) -> Result<CustomShotDef, String> {
    let v = table
        .get(name)
        .ok_or_else(|| format!("CustomFirstProjectile {name} has no projectiles record in cards.json (the file lists {})", table.len()))?;
    let p: RawSpellProjectile = serde_json::from_value(v.clone()).map_err(|e| format!("CustomFirstProjectile {name}: {e}"))?;
    refuse_action_mechanic(&p.action_graph, &format!("CustomFirstProjectile {name}"))?;
    if p.spawn_character.is_some()
        || p.spawn_projectile.is_some()
        || p.spawn_area_effect_object.is_some()
        || p.maximum_targets.is_some()
        || p.pushback_milli.is_some_and(|d| d > 0)
        || p.target_buff.as_ref().is_some_and(|b| !b.is_null())
    {
        return Err(format!("CustomFirstProjectile {name} carries a spawn, a target cap, a pushback or a buff; not simulated"));
    }
    Ok(CustomShotDef {
        speed: p.speed.filter(|s| *s > 0).ok_or_else(|| format!("CustomFirstProjectile {name} without speed"))?,
        damage: p.damage.unwrap_or(0),
        radius: milli(p.radius_milli.unwrap_or(0)),
        hits_air: p.aoe_to_air.unwrap_or(false),
        hits_ground: p.aoe_to_ground.unwrap_or(false),
        crown_pct: crown(p.crown_tower_damage_percent),
    })
}

/// A troop shot's SpawnProjectile row (cards.json `projectile.spawn_projectile`) as the sparks it
/// releases where the shot lands (`SparkDef`), or None when the row is not the measured shape: a
/// positive SpawnCount with Scatter "Line", a Speed, a Damage, a positive ProjectileRange and
/// ProjectileRadius, and no spawn, target cap, area effect, pushback, buff or scripted action of
/// its own. None rather than a refusal, so the loaded card set is the same under both arms of
/// combat.SPAWN_PROJECTILE: a card whose row reads None fires its shot as it does today.
/// SpawnCount and Scatter are written by tools/extract_cards.py on the 15.535 rows only, so every
/// row reads None until cards.json carries them.
fn spark_of(v: &serde_json::Value) -> Option<SparkDef> {
    let p: RawSpellProjectile = serde_json::from_value(v.clone()).ok()?;
    if p.scatter.as_deref() != Some("Line")
        || p.spawn_character.is_some()
        || p.spawn_projectile.is_some()
        || p.spawn_area_effect_object.is_some()
        || p.maximum_targets.is_some()
        || p.pushback_milli.is_some_and(|d| d > 0)
        || p.target_buff.as_ref().is_some_and(|b| !b.is_null())
        || p.action_graph.is_some()
    {
        return None;
    }
    Some(SparkDef {
        count: p.spawn_count.filter(|n| *n > 0)?,
        speed: p.speed.filter(|s| *s > 0)?,
        damage: p.damage?,
        crown_pct: crown(p.crown_tower_damage_percent),
        range: milli(p.projectile_range_milli.filter(|r| *r > 0)?),
        reach: milli(p.projectile_radius_milli.filter(|r| *r > 0)?),
        hits_air: p.aoe_to_air.unwrap_or(false),
        hits_ground: p.aoe_to_ground.unwrap_or(false),
        only_enemies: p.only_enemies.unwrap_or(false),
    })
}

/// A troop card's deploy projectile (cards.json `deploy_projectile`) as the impact it lands
/// as (`CardDef::deploy_projectile`): a `SpellShape::Projectile` with one wave and its hit.
/// `speed` is carried and unread: the blow is released at the unit's own position and
/// lands on a zero-length leg (state.rs `spawn_now`, as a death bomb does). Refused when
/// the row carries no damage or no radius, or a mechanic the impact does not run (a
/// spawn, a target cap, an area effect, a buff).
fn convert_deploy_projectile(v: serde_json::Value) -> Result<SpellDef, String> {
    let p: RawSpellProjectile = serde_json::from_value(v).map_err(|e| format!("deploy_projectile: {e}"))?;
    let what = p.name.clone().unwrap_or_default();
    refuse_action_mechanic(&p.action_graph, &format!("deploy projectile {what}"))?;
    if p.spawn_character.is_some()
        || p.spawn_projectile.is_some()
        || p.spawn_area_effect_object.is_some()
        || p.maximum_targets.is_some()
        || p.target_buff.as_ref().is_some_and(|b| !b.is_null())
    {
        return Err(format!("deploy projectile {what} with a spawn, a target cap, an area effect or a buff is not simulated"));
    }
    let damage = p.damage.ok_or_else(|| format!("deploy projectile {what} carries no damage"))?;
    let radius = p.radius_milli.filter(|r| *r > 0).ok_or_else(|| format!("deploy projectile {what} deals damage but has no radius"))?;
    Ok(SpellDef {
        shape: SpellShape::Projectile {
            speed: p.speed.unwrap_or(0).max(0),
            hit: Some(SpellHit {
                damage,
                crown_pct: crown(p.crown_tower_damage_percent),
                radius: milli(radius),
                hits_air: p.aoe_to_air.unwrap_or(false),
                hits_ground: p.aoe_to_ground.unwrap_or(false),
                only_enemies: p.only_enemies.unwrap_or(false),
                only_own_troops: false,
                ignore_buildings: false,
                no_effect_to_crown_towers: false,
                knockback: knockback(p.pushback_milli, p.pushback_all),
                buff: None,
                buff2: None,
                caps_buff_time: false,
                controls_buff: false,
            }),
            waves: 1,
            wave_interval_ms: 0,
            spawn: None,
        },
        // Unread: a deploy blow is never cast, so it is never placed.
        placement: SpellPlacement::Anywhere,
    })
}

impl CardDb {
    /// Parse a cards.json document (schema at the bottom of this file).
    pub fn from_json_str(s: &str, source: CardSource) -> Result<CardDb, String> {
        let file: RawCardsFile = serde_json::from_str(s).map_err(|e| format!("cards.json: {e}"))?;
        // THE FILE'S OWN RARITIES when it carries them (15.535: Champion exists, a
        // Rare has 14 levels), else the shipped 2018 table. Never mixed.
        let rarities = if file.rarities.is_empty() {
            shipped_rarities().to_vec()
        } else {
            file.rarities
                .iter()
                .map(|(name, r)| {
                    let table = r.multiplier_percent_by_level.clone().ok_or_else(|| format!("rarities.{name}: no multiplier_percent_by_level"))?;
                    let level_count = r.level_count.filter(|n| *n >= 1).ok_or_else(|| format!("rarities.{name}: no level_count"))?;
                    if table.len() < level_count as usize || table.first() != Some(&(PERCENT as i32)) {
                        return Err(format!("rarities.{name}: ladder {table:?} does not start at 100 and cover {level_count} levels"));
                    }
                    Ok(RarityRow {
                        name: name.clone(),
                        level_count,
                        relative_level: r.relative_level.filter(|l| *l >= 0).ok_or_else(|| format!("rarities.{name}: no relative_level"))?,
                        multipliers: table[1..].to_vec(),
                    })
                })
                .collect::<Result<Vec<_>, String>>()?
        };
        let mut buffs = BuffTable::default();
        let globals = CardGlobals::from_map(&file.globals)?;
        let mut db = CardDb {
            cards: Vec::new(),
            buffs: Vec::new(),
            buff_names: Vec::new(),
            by_name: BTreeMap::new(),
            source,
            rejected: Vec::new(),
            towers_from_fallback: false,
            rarities,
            version: file.version.clone(),
            globals: globals.clone(),
        };
        // `buffs` is filled from the table once every card has been converted
        // (`db.buffs = buffs.defs` below): a CardDef holds indices, never the rows.
        let ctx = LoadCtx { aeos: &file.area_effect_objects, units: &file.units, projectiles: &file.projectiles, globals: &globals };
        let mut spawns: Vec<(u16, UnitUse, String)> = Vec::new();
        // Cards refused AFTER their push, each with its reason (filled from here on; see below).
        let mut unloadable: Vec<(u16, String)> = Vec::new();
        // Each loaded record's IgnoreBuff names, resolved once every buff is interned (below).
        let mut ignore_names: Vec<(u16, Vec<String>)> = Vec::new();
        for raw in file.cards.into_iter().chain(file.towers) {
            let name = raw.name.clone();
            let ignore = raw.ignore_buffs.clone().unwrap_or_default();
            match convert(raw, &mut buffs, &ctx) {
                Ok((c, display, units)) => {
                    if !db.rarities.iter().any(|r| r.name == c.rarity) {
                        db.rejected.push((name, format!("rarity {} not in rarities.csv", c.rarity)));
                        continue;
                    }
                    db.push(c, display)?;
                    let idx = (db.cards.len() - 1) as u16;
                    // A deploy at explicit offsets: member 0 is the card's own row.
                    if let Some(m) = db.cards[idx as usize].summon_members.as_mut().and_then(|ms| ms.first_mut()) {
                        m.unit = idx;
                    }
                    for (which, unit) in table_needs_first(units) {
                        spawns.push((idx, which, unit));
                    }
                    // THE DEATH SPAWNS OF THE BUFFS THE CARD HANGS (the Mother Witch's VoodooHog, the Goblin Curse's
                    // goblin): each a need of this card, after its other needs, so every other card's units keep
                    // their breadth-first numbering. Each card that hangs the buff has its own need; a unit that
                    // fails refuses every one of them.
                    #[cfg(not(clash_plant = "buff_death_spawn_unloaded"))]
                    for (b, unit) in buffs.death_needs(&db.cards[idx as usize].hung_buffs()) {
                        spawns.push((idx, UnitUse::BuffDeathSpawn(b), unit));
                    }
                    if !ignore.is_empty() {
                        ignore_names.push((idx, ignore));
                    }
                }
                Err(e) => db.rejected.push((name, e)),
            }
        }
        // SPAWNED UNITS. A spell that releases units, a spawner and a death spawn need
        // those units' stats; they are `units` records, not cards (no Goblin card
        // exists: the card is Goblins; no Skeleton card: Skeletons, Tombstone and the
        // Witch all spawn the one `Skeleton` record). Each is loaded ONCE, as a
        // summon_only card, after every real card so no card index moves -- one unit
        // table for all three mechanics. A card whose unit cannot be loaded is REJECTED
        // (unregistered again, with the unit's reason), never left pointing at nothing:
        // the 2018 SkeletonBalloon goes this way (its SkeletonContainer, a building with no
        // hitpoints and no DeathDamage, refused on `hitpoints` before its own 8-Skeleton
        // death spawn is even reached), and so does the 2018 MovingCannon (its death spawn
        // BrokenCannon is a troop with a LifeTime; the 15.535.29 Cannon Cart reaches its
        // BrokenCannon, a building there, through a transformation instead).
        // NOT every hitpoint-less "unit" is a refusal: a DEATH BOMB (BalloonBomb,
        // GiantSkeletonBomb, BombTowerBomb) is a timed impact rather than an entity
        // and loads as one -- `convert_death_bomb`, and `phase_reap`, which leaves it
        // where the parent died instead of spawning it. The 15.535.29 Skeleton Barrel's
        // SkeletonContainerNew is a death bomb that also carries a death spawn (a
        // container): its Skeletons are its own need, one level down.
        //
        // THE UNITS LOAD FROM A WORKLIST, BREADTH FIRST: every need of every card in
        // `spawns` order, then the needs of the units loaded for them, one level
        // deeper, and so on. The order matters because the summon-only records are
        // NUMBERED in the order they load, and those numbers are inside the card
        // fingerprint (state.rs `cards_fingerprint`): all first-level units in
        // first-need order, then theirs (tests/unit_refs.rs
        // `summon_only_numbering_is_breadth_first`, tests/spawn_chain.rs).
        //
        // A SPAWN CHAIN LOADS: a unit whose own row puts units on the board (the Goblin
        // Drill's building, with its Goblin spawner and its two death Goblins; the Elixir
        // Golem's ElixirGolem2, which death-spawns ElixirGolem4) has those units queued one
        // level deeper, as its OWN needs, down to MAX_CHAIN_DEPTH. A chain is as loadable as
        // its weakest link: after the worklist, every record that reaches a record that could
        // not load, at any depth, is refused too (the fixpoint below), so no playable card
        // keeps a block whose unit lost its own.
        let mut unit_idx: BTreeMap<String, Result<u16, String>> = BTreeMap::new();
        // A VARIANT CARD'S FORMS ARE CARDS, NOT UNITS: they never enter the worklist, and are resolved by card name
        // after it and its cleanup (below), so no summon-only record is ever made for one.
        let (variant_needs, spawns): (Vec<_>, Vec<_>) = spawns.into_iter().partition(|(_, w, _)| matches!(w, UnitUse::VariantForm(_)));
        let mut work: VecDeque<(u16, UnitUse, String, u8)> = spawns.into_iter().map(|(owner, which, unit)| (owner, which, unit, 0)).collect();
        while let Some((spell_idx, which, unit, depth)) = work.pop_front() {
            // A RECORD ALREADY REFUSED loads nothing more: it keeps the one reason it has, and it
            // leaves no summon-only record behind (its needs come table first, so a refusal by a
            // table row lands before any of its units loads: `table_needs_first`).
            if unloadable.iter().any(|(i, _)| *i == spell_idx) {
                continue;
            }
            if which == UnitUse::DeathAreaEffect || which == UnitUse::ProjectileArea {
                // A DEATH AREA EFFECT IS NOT A UNIT. It is an `area_effect_objects`
                // row the death leaves standing where the unit stood, so it is
                // resolved here against the file's own table and stored on the card
                // as a `SpellDef` -- the same record, the same acceptance and the
                // same engine path a Zap goes down (`convert_area_effect`).
                //
                // A cards.json whose table has no row under this name is a file this
                // loader cannot read the card from: refused BY NAME, loudly, never
                // run as a card whose death does nothing. (The table is part of
                // cards.json, so a clone with no raw card data still resolves it.)
                let got = match ctx.aeos.get(&unit) {
                    Some(aeo) => convert_area_effect(aeo, &mut buffs, &ctx).map(|(shape, needs)| (SpellDef { shape, placement: SpellPlacement::Anywhere }, needs)),
                    None => Err(format!("no area_effect_objects record in cards.json (the file lists {})", ctx.aeos.len())),
                };
                match got {
                    Ok((def, needs)) => {
                        let card = &mut db.cards[spell_idx as usize];
                        if which == UnitUse::DeathAreaEffect {
                            card.death_area_effect = Some(def);
                        } else {
                            card.projectile_area = Some(def);
                        }
                        // The units the area releases are the DYING card's needs, at
                        // its own level of the worklist: queued behind that level's
                        // needs and ahead of every deeper one already waiting, so the
                        // order stays breadth first (no accepted area has any yet).
                        let at = work.iter().position(|q| q.3 > depth).unwrap_or(work.len());
                        for (k, (w, u)) in needs.into_iter().enumerate() {
                            work.insert(at + k, (spell_idx, w, u, depth));
                        }
                    }
                    Err(e) => {
                        let what = if which == UnitUse::DeathAreaEffect { "death area effect" } else { "projectile area" };
                        unloadable.push((spell_idx, format!("{what} {unit}: {e}")))
                    }
                }
                continue;
            }
            if matches!(which, UnitUse::DeathProjectile | UnitUse::DeployAreaEffect | UnitUse::SpawnAreaEffect) {
                // THE DEATH PROJECTILE, THE DEPLOY AREA AND THE SPAWN AREA ARE NOT UNITS either:
                // each is a row of one of the file's own tables, resolved here and stored on the
                // card as a `SpellDef`, exactly as a death area effect is, and a name the table
                // does not carry, or a row this loader cannot run, refuses the card BY NAME. Their
                // units (a death projectile's SpawnCharacter) are the card's needs at its own level
                // of the worklist. ONE SPELL OBJECT NAMES ONE SHAPE (spell.rs `shape_of`), so a
                // card that already carries a spell, a death area or another of these is refused.
                // A card already refused by an earlier need keeps that one reason: a second would
                // list it twice among the rejected (SuperLavaHound, refused for its death spawn's
                // chain, would also be refused for its chained FireWallProjectile).
                if unloadable.iter().any(|(i, _)| *i == spell_idx) {
                    continue;
                }
                let label = match which {
                    UnitUse::DeathProjectile => "death projectile",
                    UnitUse::DeployAreaEffect => "deploy area effect",
                    _ => "spawn area effect",
                };
                let c = &db.cards[spell_idx as usize];
                let blocks = [
                    c.spell.is_some(),
                    c.death_area_effect.is_some(),
                    c.deploy_projectile.is_some(),
                    c.death_projectile.is_some(),
                    c.deploy_area_effect.is_some(),
                    c.spawn_area_effect.is_some(),
                ];
                let members = c.count.max(1) + c.formation.second_summon.map_or(0, |s| s.count);
                let own_unit = c.unit_name.clone();
                let got: Result<(SpellDef, UnitNeeds), String> = if blocks.contains(&true) {
                    Err("the card already carries a spell, an area or a projectile, and one spell object names one".into())
                } else {
                    match which {
                        UnitUse::DeathProjectile => match file.projectiles.get(&unit) {
                            Some(v) => serde_json::from_value::<RawSpellProjectile>(v.clone())
                                .map_err(|e| e.to_string())
                                .and_then(|p| convert_death_projectile(&p, &mut buffs)),
                            None => Err(format!("no projectiles record in cards.json (the file lists {})", file.projectiles.len())),
                        },
                        // The area a play IS acts once per play: taken on a one-member card alone.
                        UnitUse::DeployAreaEffect if members > 1 => Err(format!("a deploy area effect on a card of {members} members is not simulated")),
                        UnitUse::DeployAreaEffect => match ctx.aeos.get(&unit) {
                            Some(aeo) => convert_deploy_area_effect(aeo, &own_unit, &mut buffs, &ctx).map(|(shape, needs)| (SpellDef { shape, placement: SpellPlacement::Anywhere }, needs)),
                            None => Err(format!("no area_effect_objects record in cards.json (the file lists {})", ctx.aeos.len())),
                        },
                        _ => match ctx.aeos.get(&unit) {
                            Some(aeo) => convert_spawn_area_effect(aeo, &mut buffs, &ctx).map(|(shape, needs)| (SpellDef { shape, placement: SpellPlacement::Anywhere }, needs)),
                            None => Err(format!("no area_effect_objects record in cards.json (the file lists {})", ctx.aeos.len())),
                        },
                    }
                };
                match got {
                    Ok((def, needs)) => {
                        let card = &mut db.cards[spell_idx as usize];
                        match which {
                            UnitUse::DeathProjectile => card.death_projectile = Some(def),
                            UnitUse::DeployAreaEffect => card.deploy_area_effect = Some(def),
                            _ => card.spawn_area_effect = Some(def),
                        }
                        let at = work.iter().position(|q| q.3 > depth).unwrap_or(work.len());
                        for (k, (w, u)) in needs.into_iter().enumerate() {
                            work.insert(at + k, (spell_idx, w, u, depth));
                        }
                    }
                    Err(e) => unloadable.push((spell_idx, format!("{label} {unit}: {e}"))),
                }
                continue;
            }
            if which == UnitUse::SpellSummon {
                // A BOTTLE IS NOT A UNIT (`hitpointless_building`): its row becomes the spell's own
                // `Fuse`, and the area its death leaves becomes what the fuse releases. Anything else
                // is a real unit and goes the ordinary way below, then `Summon.unit` is filled.
                if let Some((fuse_ms, area)) = bottle_of(&file.units, &unit) {
                    let got = match ctx.aeos.get(&area) {
                        Some(a) => convert_area_effect(a, &mut buffs, &ctx).map(|(sh, _)| sh),
                        None => Err(format!("{area} has no area_effect_objects record")),
                    };
                    match got {
                        Ok(then) => {
                            if let Some(sd) = db.cards[spell_idx as usize].spell.as_mut() {
                                sd.shape = SpellShape::Fuse { fuse_ms, then: Box::new(then) };
                            }
                        }
                        Err(e) => unloadable.push((spell_idx, format!("summon {unit}: {e}"))),
                    }
                    continue;
                }
            }
            let got = unit_idx
                .entry(unit.clone())
                .or_insert_with(|| {
                    // A NAME A CARD ALREADY HOLDS. A card serves as the unit only when
                    // the row it puts on the board IS this one (`CardDef::unit_name`):
                    // the FirespiritHut spawns `FireSpirits`, which is also the
                    // playable card's name, and that card's row is that character's
                    // row plus a count -- one table, no duplicate stats. A card of the
                    // name whose own row is a DIFFERENT one (a Ram Rider card deploys
                    // the Ram, not the rider) does not serve: the `units` row loads,
                    // registered as `units.<Name>` so every name still resolves to
                    // one record, with `unit_name` keeping the row's own name. A spell
                    // or another unit under the name is a collision.
                    let taken = match db.by_name.get(&unit).copied() {
                        None => false,
                        Some(existing) => {
                            let c = db.get(existing);
                            if c.spell.is_some() || c.summon_only {
                                return Err(format!("spawned unit {unit} collides with a card name"));
                            }
                            #[cfg(not(any(clash_plant = "unit_by_card_name", clash_plant = "unit_never_card")))]
                            let serves = c.unit_name == unit;
                            #[cfg(clash_plant = "unit_by_card_name")]
                            let serves = true; // PLANT (regression): any troop or building card of the name serves.
                            #[cfg(clash_plant = "unit_never_card")]
                            let serves = false; // PLANT (regression): no card serves; the name always loads a `units` row.
                            if serves {
                                // A card that tunnels is played, never spawned: only a deploy starts the walk.
                                if c.spawn_pathfind.is_some() {
                                    return Err(format!("spawned unit {unit} is a card that travels underground; only a played one is simulated"));
                                }
                                return Ok(existing);
                            }
                            true
                        }
                    };
                    let mut v = file.units.get(&unit).cloned().ok_or_else(|| format!("spawned unit {unit} has no units record"))?;
                    let kind = match v.get("source_table").and_then(|t| t.as_str()) {
                        Some("buildings") => "building",
                        _ => "troop",
                    };
                    let obj = v.as_object_mut().ok_or_else(|| format!("units.{unit} is not an object"))?;
                    obj.insert("kind".into(), serde_json::Value::String(kind.into()));
                    obj.entry("count").or_insert(serde_json::Value::from(1));
                    let raw: RawCard = serde_json::from_value(v).map_err(|e| format!("units.{unit}: {e}"))?;
                    // A SPAWNED UNIT THAT TUNNELS is refused: only a played card starts the walk, at its
                    // owner's King (the one other row that tunnels, GoblinDrill_EV1_Dig, is an evolution).
                    if raw.spawn_pathfind.as_ref().is_some_and(|p| p.speed.is_some() || p.morph.is_some()) {
                        return Err(format!("units.{unit} travels underground; only a played card is simulated doing that"));
                    }
                    let ignore = raw.ignore_buffs.clone().unwrap_or_default();
                    let (mut c, _, nested) = convert(raw, &mut buffs, &ctx).map_err(|e| format!("units.{unit}: {e}"))?;
                    // A SPAWNED UNIT THAT HANGS A BUFF WHOSE CARRIER RELEASES A UNIT WHEN IT DIES would put units on the
                    // board in turn. Other spawn chains load (down to MAX_CHAIN_DEPTH), but a buff's death spawn is
                    // queued as a need of a played card only (the card loop above), so such a unit is refused.
                    if let Some(b) = c.hung_buffs().into_iter().find(|b| buffs.defs[*b as usize].death_spawn.is_some()) {
                        return Err(format!("units.{unit} hangs buff {}, whose carrier releases a unit when it dies; a spawned unit's buff death spawn is not simulated", buffs.names[b as usize].join("|")));
                    }
                    // A SPAWNED UNIT'S PROJECTILE AREA (the Heal Spirit's heal) is an area, not a unit: it
                    // resolves here, onto the unit's own card, and only unit needs remain a chain.
                    // A SPAWNED UNIT'S DEATH PROJECTILE is not a unit either when it only deals damage (the
                    // Goblin Demolisher's kamikaze form blasts where it dies): it resolves here too, onto the
                    // unit's own card, under the one-shape rule. One that releases a unit (the Phoenix's egg
                    // shape) is still a chain.
                    let mut chain: UnitNeeds = Vec::new();
                    for (w, u) in nested {
                        if w == UnitUse::ProjectileArea {
                            let a = ctx.aeos.get(&u).ok_or_else(|| format!("units.{unit}: its projectile area {u} has no area_effect_objects record"))?;
                            let (shape, _) = convert_area_effect(a, &mut buffs, &ctx).map_err(|e| format!("units.{unit}: projectile area {u}: {e}"))?;
                            c.projectile_area = Some(SpellDef { shape, placement: SpellPlacement::Anywhere });
                        } else if w == UnitUse::DeathProjectile {
                            // PLANT spawned_death_projectile_unread (tests/transform.rs): the need is dropped, so
                            // the kamikaze form dies with nothing following under every arm.
                            #[cfg(clash_plant = "spawned_death_projectile_unread")]
                            continue;
                            #[allow(unreachable_code)]
                            {
                                let v = ctx.projectiles.get(&u).ok_or_else(|| format!("units.{unit}: its death projectile {u} has no projectiles record"))?;
                                let p: RawSpellProjectile = serde_json::from_value(v.clone()).map_err(|e| format!("units.{unit}: death projectile {u}: {e}"))?;
                                let (def, needs) = convert_death_projectile(&p, &mut buffs).map_err(|e| format!("units.{unit}: death projectile {u}: {e}"))?;
                                if !needs.is_empty() {
                                    chain.push((w, u));
                                    continue;
                                }
                                let shapes = [
                                    c.spell.is_some(),
                                    c.death_area_effect.is_some(),
                                    c.deploy_projectile.is_some(),
                                    c.death_projectile.is_some(),
                                    c.deploy_area_effect.is_some(),
                                    c.spawn_area_effect.is_some(),
                                    c.projectile_area.is_some(),
                                ];
                                if shapes.contains(&true) {
                                    return Err(format!("units.{unit}: death projectile {u}: the unit already carries a spell, an area or a projectile, and one spell object names one"));
                                }
                                c.death_projectile = Some(def);
                            }
                        } else {
                            chain.push((w, u));
                        }
                    }
                    let nested = chain;
                    // AN ATTACHED RIDER'S ROW must be one the rider law covers (`rider_shape`), and says
                    // why before any unit of its own loads (a dismount's death spawn would otherwise
                    // load as a chain below).
                    if which == UnitUse::Attach {
                        rider_shape(&c, nested.iter().any(|(w, _)| *w != UnitUse::DeathSpawn), &unit)?;
                    }
                    // A TRANSFORMATION TARGET CARRIES NO STATE ITS SPAWN OR DEPLOY WOULD START: the entity
                    // keeps its own and is not deployed again (state.rs `rebind_unit`), so a row whose block
                    // needs either would run without it. Refused by the block; the Cannon Cart's
                    // BrokenCannon and the Goblin Demolisher's kamikaze form carry none of them.
                    if which == UnitUse::Transform {
                        let block = [
                            (c.hide.is_some(), "a hide"),
                            (c.spawner.is_some(), "a periodic spawner"),
                            (c.life_state.is_some(), "a life-state controller"),
                            (c.charge.is_some(), "a charge"),
                            (c.jump.is_some(), "a jump"),
                            (c.dash.is_some(), "a dash"),
                            (c.special.is_some(), "a special"),
                            (c.invisible_when_idle.is_some(), "an idle invisibility"),
                            (c.deploy_projectile.is_some(), "a deploy projectile"),
                            (nested.iter().any(|(w, _)| *w == UnitUse::DeployAreaEffect), "a deploy area effect"),
                            (nested.iter().any(|(w, _)| *w == UnitUse::SpawnAreaEffect), "a spawn area effect"),
                            (c.transform_at_hp.is_some(), "a transformation of its own"),
                            (c.parry.is_some(), "a counter"),
                            (c.attach.is_some(), "attached riders"),
                            (c.mana.is_some_and(|m| m.collect.is_some()), "an elixir payout"),
                            (c.enchant.is_some(), "an enchant"),
                        ]
                        .into_iter()
                        .find(|(has, _)| *has);
                        if let Some((_, what)) = block {
                            return Err(format!("units.{unit}: a transformation into a row with {what} is not simulated"));
                        }
                    }
                    // A UNIT'S OWN UNITS LOAD, one level deeper (the Goblin Drill's building's Goblins,
                    // the Elixir Golem's ElixirGolem4, the Phoenix egg's PhoenixNoRespawn), down to
                    // MAX_CHAIN_DEPTH: a record first loaded at that depth may not need units of its own.
                    // PLANT chain_refused (tests/spawn_chain.rs) keeps the earlier refusal: every chain
                    // but a death projectile's hatching egg refused.
                    #[cfg(clash_plant = "chain_refused")]
                    if let Some((_, first)) = nested.first() {
                        let hatches = which == UnitUse::DeathProjectileRelease && nested.iter().all(|(w, _)| *w == UnitUse::Spawner);
                        if !hatches {
                            return Err(format!("units.{unit} itself spawns units ({first}); a spawn chain is not simulated"));
                        }
                    }
                    if let Some((_, first)) = nested.first() {
                        if depth >= MAX_CHAIN_DEPTH {
                            return Err(format!(
                                "units.{unit} itself spawns units ({first}) {} levels below a card, deeper than the {MAX_CHAIN_DEPTH} this loader follows",
                                depth + 1
                            ));
                        }
                    }
                    // A DEATH PROJECTILE'S SPAWNCHARACTER STANDS STILL (the PhoenixEgg). Measured on client
                    // 15.535.29: the new Phoenix appears exactly (0, +1100) from where the egg appeared, 76 ticks
                    // later, so the egg never walks. Its row's Speed 40 is not a walk: the row also sets
                    // GameTagsToSet NO_MOVE_ALLOW_ATTRACT, which no other row carries.
                    if which == UnitUse::DeathProjectileRelease {
                        c.speed = 0;
                    }
                    // A TROOP WITH A LIFETIME loads as a transformation target alone (the Goblin Demolisher's
                    // kamikaze form, lifetime.TROOP_LIFETIME). Reached any other way it is refused as before:
                    // the 2018 MovingCannon's death spawn BrokenCannon is one. The refusal stays inside this
                    // cached closure, so a refused unit is never pushed and no later summon-only slot moves;
                    // the cache is by name, so a name's first use decides (no 15.535.29 row is requested both
                    // ways).
                    #[cfg(not(clash_plant = "troop_lifetime_refusal_lifted"))]
                    let refused = c.kind == CardKind::Troop && c.lifetime_ms.is_some() && which != UnitUse::Transform;
                    #[cfg(clash_plant = "troop_lifetime_refusal_lifted")]
                    let refused = false; // PLANT (regression): a troop with a LifeTime loads whatever reaches it.
                    if refused {
                        return Err(format!("units.{unit} is a troop with a LifeTime; not simulated"));
                    }
                    if !db.rarities.iter().any(|r| r.name == c.rarity) {
                        return Err(format!("units.{unit}: rarity {} not in rarities.csv", c.rarity));
                    }
                    c.summon_only = true;
                    if taken {
                        c.name = format!("units.{unit}");
                    }
                    db.push(c, None)?;
                    let idx = (db.cards.len() - 1) as u16;
                    if !ignore.is_empty() {
                        ignore_names.push((idx, ignore));
                    }
                    // The loaded unit's own needs, one level deeper, table needs first (`table_needs_first`).
                    work.extend(table_needs_first(nested).into_iter().map(|(w, u)| (idx, w, u, depth + 1)));
                    Ok(idx)
                })
                .clone();
            // A rider row another block loaded first (so its load above did not check it as a
            // rider) is held to the same shape, read off the record it became.
            let got = match got {
                Ok(u) if which == UnitUse::Attach => {
                    let r = db.get(u);
                    let leaves = db.unit_refs(u).iter().any(|(p, _, _)| *p != UnitRef::DeathSpawn) || r.death_area_effect.is_some() || r.death_projectile.is_some() || r.spawn_area_effect.is_some();
                    rider_shape(r, leaves, &unit).map(|()| u)
                }
                other => other,
            };
            match got {
                // THE MORPH TARGET IS A BUILDING WITH HITPOINTS (the one measured: the Goblin Drill's
                // 1313-hp building). A morph into a troop, or into a hitpoint-less death bomb, is a
                // shape nothing measured: the card is refused.
                Ok(u) if which == UnitUse::Morph && (db.cards[u as usize].kind != CardKind::Building || db.cards[u as usize].death_bomb_fuse_ms().is_some()) => {
                    unloadable.push((spell_idx, format!("units.{unit}: an underground morph into anything but a building with hitpoints is not simulated")));
                }
                // A transformation turns the unit into a ROW the card owns, never into a playable card: the name of
                // a card that serves as the unit (`CardDef::unit_name`) is refused here.
                Ok(u) if which == UnitUse::Transform && !db.cards[u as usize].summon_only => {
                    unloadable.push((spell_idx, format!("units.{unit}: a transformation into a playable card is not simulated")));
                }
                Ok(u) => {
                    let card = &mut db.cards[spell_idx as usize];
                    match which {
                        UnitUse::Spell => {
                            if let Some(sp) = card.spell.as_mut().and_then(|d| d.shape.release_slot()).and_then(|s| s.as_mut()) {
                                sp.unit = u;
                            }
                        }
                        UnitUse::Spawner => card.spawner.as_mut().expect("spawner block present").unit = u,
                        UnitUse::DeathSpawn => card.death_spawn.as_mut().expect("death_spawn block present").unit = u,
                        UnitUse::SecondSummon => card.formation.second_summon.as_mut().expect("second_summon present").unit = u,
                        UnitUse::DeathProjectileRelease => {
                            if let Some(SpellDef { shape: SpellShape::Projectile { spawn: Some(sp), .. }, .. }) = card.death_projectile.as_mut() {
                                sp.unit = u;
                            }
                        }
                        UnitUse::SpellSummon => {
                            if let Some(SpellDef { shape: SpellShape::Summon { unit: su, .. }, .. }) = card.spell.as_mut() {
                                *su = u;
                            }
                        }
                        UnitUse::LifeState => card.life_state.as_mut().expect("life_state present").unit = u,
                        UnitUse::Morph => card.spawn_pathfind.as_mut().expect("spawn_pathfind present").morph = Some(u),
                        UnitUse::Attach => card.attach.as_mut().expect("attach block present").unit = u,
                        UnitUse::SummonMember(k) => {
                            card.summon_members.as_mut().expect("summon_members present")[k as usize].unit = u;
                        }
                        // The buff's row, which every card that hangs it shares, not the card.
                        UnitUse::BuffDeathSpawn(b) => buffs.set_death_unit(b, u),
                        UnitUse::Transform => card.transform_at_hp.as_mut().expect("transform present").unit = u,
                        // Onto the first of the card's spell objects whose entry k is still unresolved: the spell, the
                        // death area, the projectile area, the order their needs are queued in.
                        UnitUse::Scheduled(k) => {
                            let slot = [card.spell.as_mut(), card.death_area_effect.as_mut(), card.projectile_area.as_mut()]
                                .into_iter()
                                .flatten()
                                .filter_map(|d| d.shape.schedule_mut())
                                .filter_map(|s| s.get_mut(k as usize))
                                .find(|e| e.unit == u16::MAX);
                            if let Some(e) = slot {
                                e.unit = u;
                            }
                        }
                        UnitUse::DeathAreaEffect | UnitUse::DeathProjectile | UnitUse::DeployAreaEffect | UnitUse::SpawnAreaEffect | UnitUse::ProjectileArea => {
                            unreachable!("never resolved here: resolved against the file's tables above")
                        }
                        UnitUse::VariantForm(_) => unreachable!("a variant form never enters the worklist"),
                    }
                }
                Err(e) => unloadable.push((spell_idx, e)),
            }
        }
        // A CARD THAT HANGS A BUFF WHOSE DEATH UNIT WAS NEVER LOADED FOR IT is refused, never run as a card whose curse
        // leaves nothing (a buff it hangs through a block resolved above, whose need no card pushed). Then every death
        // spawn still unresolved belongs to a buff no loaded card hangs, and is dropped.
        for idx in 0..db.cards.len() as u16 {
            if unloadable.iter().any(|(i, _)| *i == idx) {
                continue;
            }
            if let Some(b) = db.cards[idx as usize].hung_buffs().into_iter().find(|b| buffs.death_unresolved(*b)) {
                unloadable.push((idx, format!("buff {} releases a unit when its carrier dies, and that unit was not loaded for this card; not simulated", buffs.names[b as usize].join("|"))));
            }
        }
        buffs.drop_unresolved_deaths();
        // IgnoreBuff, resolved now that every buff is interned: each name becomes the indices it interned to. A name no
        // buff carries is dropped (the towers' event buff). A name whose index also stands for a row the list does not
        // name is refused: interning merges rows by value, so the immunity would reach a buff the row never listed.
        for (idx, names) in ignore_names {
            if unloadable.iter().any(|(i, _)| *i == idx) {
                continue;
            }
            let mut ids: Vec<u16> = Vec::new();
            let mut why: Option<String> = None;
            for n in &names {
                for b in buffs.indices_named(n) {
                    if let Some(other) = buffs.names[b as usize].iter().find(|x| !names.contains(x)) {
                        why = Some(format!("IgnoreBuff {n} shares its row with {other}; the immunity would be ambiguous"));
                    }
                    if !ids.contains(&b) {
                        ids.push(b);
                    }
                }
            }
            ids.sort_unstable();
            match why {
                Some(w) => unloadable.push((idx, w)),
                None => db.cards[idx as usize].ignore_buffs = ids,
            }
        }
        // THE BUFF TABLE AS IT STANDS NOW, so `unit_refs` sees the buffs' death spawns in the scan below and in the
        // cleanup after it. The final assignment is `into_parts`, after the fallback towers.
        db.buffs = buffs.defs.clone();
        // A DEATH BOMB IS IMPLEMENTED ON THE DEATH-SPAWN PATH ALONE (state.rs
        // `phase_reap`, which leaves a timed impact instead of a spawn). Any other
        // block that named one (`unit_refs`: a periodic spawner, a spell release, a
        // second summon) would reach `spawn_now` with a hitpoint-less record and put a
        // thing on the board that dies the moment it is looked at. No shipped row
        // does -- BalloonBomb, GiantSkeletonBomb and BombTowerBomb are named by
        // DeathSpawnCharacter and by nothing else -- and a row that starts to is
        // REFUSED rather than run wrong.
        for idx in 0..db.cards.len() as u16 {
            let from_other_path = db
                .unit_refs(idx)
                .into_iter()
                .filter(|(path, _, _)| *path != UnitRef::DeathSpawn)
                .find(|(_, u, _)| db.cards.get(*u as usize).and_then(CardDef::death_bomb_fuse_ms).is_some());
            if let Some((path, u, _)) = from_other_path {
                let name = db.cards[u as usize].name.clone();
                unloadable.push((idx, format!("{name} is a death bomb, which only a death spawn releases; {} cannot", path.block_name())));
            }
        }
        // A CHAIN IS AS LOADABLE AS ITS WEAKEST LINK. A record whose own need failed is refused
        // above, against itself; every record that REACHES it through `unit_refs`, at any depth,
        // is refused here too, until nothing more changes (a fixpoint, so the order the roots
        // were loaded in does not decide which of them is refused). Two cards that share a unit
        // whose own unit fails are both refused, not only the one that loaded it first. The
        // reason names the unit reached, in the `units.<row>` form of a direct refusal. A variant
        // card's forms are not links of a chain: they are cards, still unresolved here (u16::MAX)
        // and resolved by name after this cleanup, which refuses the variant card itself when a
        // form does not load.
        {
            let mut broken: std::collections::BTreeSet<u16> = unloadable.iter().map(|(i, _)| *i).collect();
            #[allow(unused_mut)]
            let mut first_only = false;
            loop {
                let before = broken.len();
                for idx in 0..db.cards.len() as u16 {
                    if broken.contains(&idx) {
                        continue;
                    }
                    let reached = db
                        .unit_refs(idx)
                        .into_iter()
                        .filter(|(path, _, _)| !matches!(path, UnitRef::VariantForm(_)))
                        .find(|(_, u, _)| *u == u16::MAX || broken.contains(u));
                    if let Some((path, u, _)) = reached {
                        let why = match db.cards.get(u as usize) {
                            None => format!("{} names a unit that never resolved", path.block_name()),
                            Some(c) => {
                                let first = unloadable.iter().find(|(i, _)| *i == u).map(|(_, w)| w.clone()).unwrap_or_default();
                                format!("units.{}: {first}", c.unit_name)
                            }
                        };
                        unloadable.push((idx, why));
                        broken.insert(idx);
                        // PLANT chain_failure_first_root_only (tests/spawn_chain.rs): only the first
                        // record found reaching the failure is refused; the others keep the block.
                        #[cfg(clash_plant = "chain_failure_first_root_only")]
                        {
                            first_only = true;
                            break;
                        }
                    }
                }
                if first_only || broken.len() == before {
                    break;
                }
            }
        }
        for (spell_idx, why) in unloadable {
            // Keep indices stable: the card stays in `cards` but is unregistered and
            // listed as rejected, so no name resolves to it. Its unit blocks are
            // DROPPED: nothing may keep pointing at the unresolved u16::MAX, because a
            // board entity of this card is still reachable through a format-3
            // snapshot (state.rs migrate_v3 keeps these five in the format-3 card
            // list, where they were plain units) and phase_reap / check_levels would
            // index cards[65535] at its death. Dropped, it runs
            // as the plain unit format 3 ran it.
            db.clear_unit_refs(spell_idx);
            // A SUMMON-ONLY RECORD refused in a chain (its own unit could not load) is not a row of
            // the file's card list: its blocks go and its name is unregistered, and the refusal is
            // listed against every card that reaches it (the fixpoint above), not against it, so
            // `rejected` keeps meaning "a card of the file that is not simulable".
            if db.cards[spell_idx as usize].summon_only {
                db.by_name.retain(|_, i| *i != spell_idx);
                continue;
            }
            let name = db.cards[spell_idx as usize].name.clone();
            // PLANT census_admits_one (tests/loadable_census.rs): a card rejected here
            // is KEPT -- registered, its blocks dropped, running as the plain unit --
            // the way a lifted refusal admits a row nobody planned for. One row per
            // table whose status no other test pins in that table, so only the check
            // over every row sees it move: the 15.535.29 SuperHogRider (its spawner's
            // SantaPresent is a building with no hitpoints) and the 2018 MovingCannon (its
            // BrokenCannon is a troop with a LifeTime). The 15.535.29 MovingCannon loads
            // (its BrokenCannon is a transformation target there) and never gets here; the
            // 15.535.29 ElixirGolem, the row before, is pinned by tests/spawn_chain.rs now.
            #[cfg(clash_plant = "census_admits_one")]
            if name == "SuperHogRider" || name == "MovingCannon" {
                continue;
            }
            db.by_name.retain(|_, i| *i != spell_idx);
            db.rejected.push((name, why));
        }
        // A VARIANT CARD'S FORMS (`UnitUse::VariantForm`), after the unit loop and its cleanup, so a form the loop
        // rejected is already unregistered. A FORM IS A CARD: SpellData names a spells_characters / spells_buildings
        // row, so it resolves by the card's INTERNAL name alone -- never a `units` record, never a display alias -- and
        // it must be a troop or building card, not a spell, not a summon-only record, not another variant. Then the
        // forms of one card must share one CardKind, each trigger must be its form's cost in thousandths, and the card's
        // own elixir its first form's (so the catalogue's `elixir` is the first option's cost). Any failure rejects the
        // card with its reason; the table's own consistency is also gated by tools/check_data.py.
        let mut variant_rejects: Vec<(u16, String)> = Vec::new();
        for (vidx, which, name) in variant_needs {
            let UnitUse::VariantForm(k) = which else { unreachable!("partitioned on VariantForm") };
            if variant_rejects.iter().any(|(i, _)| *i == vidx) || db.index(&db.cards[vidx as usize].name) != Some(vidx) {
                continue;
            }
            #[cfg(not(clash_plant = "variant_form_via_units"))]
            let form = db.index(&name).filter(|&f| db.get(f).name == name);
            // PLANT (regression): a summon-only record of the name's unit serves ahead of the card.
            #[cfg(clash_plant = "variant_form_via_units")]
            let form = db.cards.iter().position(|c| c.summon_only && c.unit_name == name).map(|i| i as u16).or_else(|| db.index(&name));
            match form {
                None => variant_rejects.push((vidx, format!("variant option {k} {name} is not a loadable card"))),
                Some(f) if db.get(f).spell.is_some() || (db.get(f).summon_only && cfg!(not(clash_plant = "variant_form_via_units"))) => {
                    variant_rejects.push((vidx, format!("variant option {k} {name} is not a troop or building card")))
                }
                Some(f) => {
                    if let Some(SpellDef { shape: SpellShape::Variant { options }, .. }) = db.cards[vidx as usize].spell.as_mut() {
                        options[k as usize].card = f;
                    }
                }
            }
        }
        for vidx in 0..db.cards.len() as u16 {
            let Some(opts) = db.get(vidx).variant() else { continue };
            if variant_rejects.iter().any(|(i, _)| *i == vidx) || opts.iter().any(|o| o.card == u16::MAX) {
                continue;
            }
            let c = db.get(vidx);
            let first = db.get(opts[0].card);
            let why = if opts.iter().any(|o| db.get(o.card).kind != first.kind) {
                Some("variant forms of different kinds are not simulated".to_string())
            } else if let Some(o) = opts.iter().find(|o| o.trigger_milli != db.get(o.card).elixir * 1000) {
                Some(format!("a variant trigger {} that is not its form {}'s cost is not simulated", o.trigger_milli, db.get(o.card).name))
            } else if c.elixir != first.elixir {
                Some(format!("a variant card of {} elixir whose first form costs {} is not simulated", c.elixir, first.elixir))
            } else {
                None
            };
            if let Some(why) = why {
                variant_rejects.push((vidx, why));
            }
        }
        for (vidx, why) in variant_rejects {
            db.clear_unit_refs(vidx);
            let name = db.cards[vidx as usize].name.clone();
            db.by_name.retain(|_, i| *i != vidx);
            db.rejected.push((name, why));
        }
        if db.index(KING_TOWER).is_none() || db.index(PRINCESS_TOWER).is_none() {
            let fb: RawCardsFile = serde_json::from_str(FALLBACK_TOWERS_JSON).expect("fallback towers parse");
            let fb_globals = CardGlobals::default();
            let fb_ctx = LoadCtx { aeos: &fb.area_effect_objects, units: &fb.units, projectiles: &fb.projectiles, globals: &fb_globals };
            for raw in fb.cards {
                if db.index(&raw.name).is_none() {
                    let (c, d, _) = convert(raw, &mut buffs, &fb_ctx)?;
                    db.push(c, d)?;
                }
            }
            db.towers_from_fallback = true;
        }
        db.resolve_enchants(&ctx);
        let (defs, names) = buffs.into_parts();
        db.buffs = defs;
        db.buff_names = names;
        Ok(db)
    }

    /// THE ENCHANT'S MULTIPLIERS AND EXCLUSIONS, resolved against the loaded cards (`EnchantDef::per_attacker`,
    /// `EnchantDef::excluded`), for every card that carries an enchant. A multiplier names the row that DELIVERS the
    /// damage: a character row (the Electro Wizard) or a projectile row (the Hunter's pellet, the Firecracker's spark).
    /// So an attacker card is matched through the unit it puts on the board (`CardDef::unit_name`), never through its
    /// own name: its direct hits by the unit row's CustomFirstProjectile (when it differs from its Projectile), its
    /// Projectile, then the unit row itself, the first listed name giving the per mille; its sparks by the Projectile
    /// row's SpawnProjectile. The TriWizards card (unit ElectroWizard) takes the Electro Wizard's 500. Only registered
    /// cards and summon-only units are resolved; spells deal no attack.
    fn resolve_enchants(&mut self, ctx: &LoadCtx) {
        let live: BTreeSet<u16> = self.by_name.values().copied().collect();
        let str_of = |v: Option<&serde_json::Value>| v.and_then(serde_json::Value::as_str).map(str::to_string);
        for owner in 0..self.cards.len() {
            let Some(def) = self.cards[owner].enchant.as_ref() else { continue };
            let table: BTreeMap<&str, i32> = def.multipliers.iter().map(|(n, m)| (n.as_str(), *m)).collect();
            let mut per_attacker: Vec<(u16, i32, i32)> = Vec::new();
            let mut excluded: Vec<u16> = Vec::new();
            for &k in &live {
                let c = &self.cards[k as usize];
                if c.spell.is_some() {
                    continue;
                }
                if def.excluded_units.contains(&c.unit_name) {
                    excluded.push(k);
                }
                let raw = ctx.units.get(&c.unit_name).and_then(|u| u.get("raw"));
                let projectile = str_of(raw.and_then(|r| r.get("Projectile")));
                let custom = str_of(raw.and_then(|r| r.get("CustomFirstProjectile"))).filter(|n| Some(n) != projectile.as_ref());
                #[cfg(not(clash_plant = "enchant_multiplier_by_card_name"))]
                let direct_names: Vec<String> = custom.into_iter().chain(projectile.clone()).chain(std::iter::once(c.unit_name.clone())).collect();
                #[cfg(clash_plant = "enchant_multiplier_by_card_name")]
                let direct_names: Vec<String> = {
                    let _ = custom;
                    vec![c.name.clone()] // PLANT: the card's own name, which names no row.
                };
                let spark_name = projectile.as_deref().and_then(|p| ctx.projectiles.get(p)).and_then(|p| str_of(p.get("spawn_projectile").and_then(|s| s.get("name"))));
                let direct = direct_names.iter().find_map(|n| table.get(n.as_str()).copied());
                let spark = spark_name.as_deref().and_then(|n| table.get(n).copied());
                if direct.is_some() || spark.is_some() {
                    per_attacker.push((k, direct.unwrap_or(PER_MILLE), spark.unwrap_or(PER_MILLE)));
                }
            }
            drop(table);
            let def = self.cards[owner].enchant.as_mut().expect("the enchant read above");
            def.per_attacker = per_attacker;
            def.excluded = excluded;
        }
    }

    /// The unified level a unit released by spell `spell_idx` at unified `level` has.
    ///
    /// SpawnCharacterLevelIndex, READING CHOSEN: an offset added to the SPELL's
    /// rarity-local level, giving the unit's rarity-local level, converted back to
    /// unified. The other reading ("the same unified level as the spell") gives the
    /// identical answer for every shipped row, because every shipped index equals its
    /// rarity's RelativeLevel (Goblin Barrel: Epic, 5) -- docs/spell-spec.md. With no
    /// index, the unit takes the spell's unified level. A DEATH PROJECTILE's release
    /// (`CardDef::death_projectile`, the Phoenix's egg) is read the same way off the dying
    /// card: a card carries a spell or a death projectile, never both.
    pub fn spawn_level(&self, spell_idx: u16, level: i32) -> Result<i32, String> {
        let c = self.get(spell_idx);
        let Some(sp) = c.spell.as_ref().or(c.death_projectile.as_ref()).and_then(|d| d.shape.release()) else {
            return Err(format!("{} releases no units", c.name));
        };
        #[cfg(clash_plant = "spawn_level_local_one")]
        {
            // PLANT: the released unit at its rarity's local level 1.
            let r = self.rarity(&self.get(sp.unit).rarity).map(|r| r.relative_level).unwrap_or(0);
            let _ = level;
            return Ok(r + 1);
        }
        #[allow(unreachable_code)]
        self.unit_level(spell_idx, sp.unit, sp.level_index, level)
    }

    /// The unified level of unit `unit` produced by card `owner` at unified `level`
    /// under an optional level index (the `spawn_level` rule). The Spawn* blocks DO
    /// carry one in the 2018 data -- SpawnCharacterLevelIndex is 2 on every hut, 5 on
    /// the Witch, 8 on the DarkWitch, 2 on the BattleRam -- but the extractor does
    /// not carry it into cards.json, so the spawner and death-spawn callers pass
    /// None and every unit takes the owner's unified level. That is exact today:
    /// every shipped index equals the owner's rarity RelativeLevel and every spawned
    /// unit is Common, so `level - own + ix + theirs == level`. OPEN: carry the
    /// column (tools/extract_cards.py `spawner.level_index`) and pass it here as the
    /// Goblin Barrel path does. Err if that level does not exist for the unit's rarity:
    /// never clamped.
    pub fn unit_level(&self, owner: u16, unit: u16, level_index: Option<i32>, level: i32) -> Result<i32, String> {
        if unit == u16::MAX {
            return Err(format!("{}: spawned unit unresolved (the card was rejected after its push)", self.get(owner).name));
        }
        let c = self.get(owner);
        let u = self.get(unit);
        let out = match level_index {
            None => level,
            Some(ix) => {
                let own = self.rarity(&c.rarity).ok_or_else(|| format!("{}: unknown rarity", c.name))?.relative_level;
                let theirs = self.rarity(&u.rarity).ok_or_else(|| format!("{}: unknown rarity", u.name))?.relative_level;
                level - own + ix + theirs
            }
        };
        self.level_multiplier(unit, out)?;
        Ok(out)
    }

    /// Level of the units card `idx`'s SPAWNER emits at unified `level`.
    pub fn spawner_level(&self, idx: u16, level: i32) -> Result<i32, String> {
        let sp = self.get(idx).spawner.ok_or_else(|| format!("{} has no spawner", self.get(idx).name))?;
        self.unit_level(idx, sp.unit, None, level)
    }

    /// Level of the unit card `idx`'s SPELL SUMMON puts down at unified `level` (the spell's own).
    pub fn summon_level(&self, idx: u16, level: i32) -> Result<i32, String> {
        match &self.get(idx).spell {
            Some(SpellDef { shape: SpellShape::Summon { unit, .. }, .. }) => self.unit_level(idx, *unit, None, level),
            _ => Err(format!("{} summons no unit", self.get(idx).name)),
        }
    }

    /// Level of the units card `idx`'s DEATH SPAWN leaves at unified `level`.
    pub fn death_spawn_level(&self, idx: u16, level: i32) -> Result<i32, String> {
        let ds = self.get(idx).death_spawn.ok_or_else(|| format!("{} has no death spawn", self.get(idx).name))?;
        self.unit_level(idx, ds.unit, None, level)
    }

    /// Every level a card at unified `level` can put on the board exists: the card's
    /// own, and the level of every unit `unit_refs` names, and of every unit THOSE put
    /// on the board, down the whole chain (the Goblin Drill's building and its Goblins;
    /// the Elixir Golem's ElixirGolem2 and ElixirGolem4; the Phoenix's egg and the Phoenix
    /// it hatches), each at the level its parent gives it. Called at deck validation and
    /// every scenario / debug spawn, so a spawn later in the tick loop can never fail on
    /// a level. A record already checked at a level is not checked again, so a chain that
    /// comes back on itself ends.
    pub fn check_levels(&self, idx: u16, level: i32) -> Result<(), String> {
        let mut seen: Vec<(u16, i32)> = Vec::new();
        self.check_levels_at(idx, level, &mut seen)
    }

    /// `check_levels` below one record: its level, then each unit it names at the level it gives
    /// that unit, recursively. `seen` holds every (record, level) already checked on this walk.
    fn check_levels_at(&self, idx: u16, level: i32, seen: &mut Vec<(u16, i32)>) -> Result<(), String> {
        if seen.contains(&(idx, level)) {
            return Ok(());
        }
        seen.push((idx, level));
        self.level_multiplier(idx, level)?;
        for (path, unit, level_index) in self.unit_refs(idx) {
            let at = match path {
                UnitRef::SpellRelease => self.spawn_level(idx, level)?,
                UnitRef::Spawner
                | UnitRef::DeathSpawn
                | UnitRef::SecondSummon
                | UnitRef::DeathProjectile
                | UnitRef::SpellSummon
                | UnitRef::LifeState
                | UnitRef::Morph
                | UnitRef::Attach
                | UnitRef::SummonMember(_)
                | UnitRef::BuffDeathSpawn
                | UnitRef::Transform
                | UnitRef::Scheduled(_) => self.unit_level(idx, unit, level_index, level)?,
                // A variant's form is a card of its own, played at the same unified level: checked whole (its own
                // units included) by the walk below, at that level. A form is never itself a variant (the loader
                // refuses one).
                UnitRef::VariantForm(_) => {
                    if unit == u16::MAX {
                        return Err(format!("{}: variant form unresolved (the card was rejected after its push)", self.get(idx).name));
                    }
                    level
                }
            };
            // PLANT check_levels_one_deep (tests/spawn_chain.rs): the earlier body, which
            // checked a unit's own units only under a death projectile, one level down.
            #[cfg(clash_plant = "check_levels_one_deep")]
            {
                if path == UnitRef::DeathProjectile {
                    for (_, sub, sub_index) in self.unit_refs(unit) {
                        self.unit_level(unit, sub, sub_index, at)?;
                    }
                }
                continue;
            }
            #[allow(unreachable_code)]
            self.check_levels_at(unit, at, seen)?;
        }
        Ok(())
    }

    /// EVERY UNIT CARD `idx` CAN PUT ON THE BOARD: (the block, the unit's CardDb
    /// index, the block's level index), one entry per unit-producing block, in a fixed
    /// order. A unit that never resolved reads u16::MAX (a card rejected after its
    /// push, before its blocks are dropped).
    ///
    /// THE ONE ENUMERATION. Level validation (`check_levels`), the catalogue
    /// attribution (py.rs `ids_of_indices`), the death-bomb path check and the
    /// rejected-card cleanup (`clear_unit_refs`, both in `from_json_str`), and the
    /// replay harness's rooting (examples/replay_parity/harness.rs `Roots::new`) read
    /// this and nothing else, so a block that puts a record on the board is added here
    /// once and reaches all five; five hand lists used to have to agree.
    /// tests/unit_refs.rs holds it against the CardDef fields and each record's Debug
    /// text.
    pub fn unit_refs(&self, idx: u16) -> Vec<(UnitRef, u16, Option<i32>)> {
        let c = self.get(idx);
        let mut out = Vec::new();
        if let Some(sp) = c.spell.as_ref().and_then(|d| d.shape.release()) {
            // PLANT unit_refs_skips_new_paths also drops a centre-aimed strike's delivery.
            #[cfg(clash_plant = "unit_refs_skips_new_paths")]
            let skip = matches!(c.spell.as_ref().map(|d| &d.shape), Some(SpellShape::Strikes(_)));
            #[cfg(not(clash_plant = "unit_refs_skips_new_paths"))]
            let skip = false;
            if !skip {
                out.push((UnitRef::SpellRelease, sp.unit, sp.level_index));
            }
        }
        if let Some(sp) = c.spawner {
            out.push((UnitRef::Spawner, sp.unit, None));
        }
        if let Some(ds) = c.death_spawn {
            out.push((UnitRef::DeathSpawn, ds.unit, None));
        }
        // PLANT unit_refs_skips_new_paths (tests/unit_refs.rs) drops this block: the
        // enumeration misses a path, and every reader with it.
        #[cfg(not(clash_plant = "unit_refs_skips_new_paths"))]
        if let Some(ss) = c.formation.second_summon {
            out.push((UnitRef::SecondSummon, ss.unit, None));
        }
        if let Some(SpellDef { shape: SpellShape::Projectile { spawn: Some(sp), .. }, .. }) = &c.death_projectile {
            out.push((UnitRef::DeathProjectile, sp.unit, sp.level_index));
        }
        if let Some(SpellDef { shape: SpellShape::Summon { unit, .. }, .. }) = &c.spell {
            out.push((UnitRef::SpellSummon, *unit, None));
        }
        if let Some(ls) = c.life_state {
            out.push((UnitRef::LifeState, ls.unit, None));
        }
        if let Some(SpawnPathfindDef { morph: Some(m), .. }) = c.spawn_pathfind {
            out.push((UnitRef::Morph, m, None));
        }
        // PLANT unit_refs_skips_attach (tests/unit_refs.rs) drops the attached rider the same way.
        #[cfg(not(clash_plant = "unit_refs_skips_attach"))]
        if let Some(at) = c.attach {
            out.push((UnitRef::Attach, at.unit, None));
        }
        if let Some(ms) = &c.summon_members {
            for (k, m) in ms.iter().enumerate() {
                out.push((UnitRef::SummonMember(k as u8), m.unit, None));
            }
        }
        // PLANT unit_refs_skips_new_paths drops the variant forms too.
        #[cfg(not(clash_plant = "unit_refs_skips_new_paths"))]
        if let Some(opts) = c.variant() {
            for (k, o) in opts.iter().enumerate() {
                out.push((UnitRef::VariantForm(k as u8), o.card, None));
            }
        }
        // The death spawn of every buff the card hangs (`CardDef::hung_buffs`), one entry per buff. The buff's row
        // holds the unit, so the entry is read off the buff table.
        #[cfg(not(clash_plant = "unit_refs_skips_new_paths"))]
        for b in c.hung_buffs() {
            if let Some(ds) = self.buffs.get(b as usize).and_then(|d| d.death_spawn) {
                out.push((UnitRef::BuffDeathSpawn, ds.unit, None));
            }
        }
        // LAST, so no earlier block's place in the list moves. PLANT unit_refs_skips_new_paths drops it too.
        #[cfg(not(clash_plant = "unit_refs_skips_new_paths"))]
        if let Some(t) = c.transform_at_hp {
            out.push((UnitRef::Transform, t.unit, None));
        }
        // After it, so no earlier block's place moves: every entry of a scheduled area in the card's spell, death area or
        // projectile area (the Graveyard's Skeletons, the Suspicious Bush's goblins). PLANT unit_refs_skips_scheduled
        // (tests/scheduled_area.rs, tests/unit_refs.rs) drops them.
        #[cfg(not(clash_plant = "unit_refs_skips_scheduled"))]
        for d in [&c.spell, &c.death_area_effect, &c.projectile_area].into_iter().flatten() {
            if let Some(entries) = d.shape.schedule() {
                for (k, e) in entries.iter().enumerate() {
                    out.push((UnitRef::Scheduled(k as u8), e.unit, None));
                }
            }
        }
        out
    }

    /// Is record `unit` the building some card's tunneller leaves where it comes up
    /// (`SpawnPathfindDef::morph`)? Such a record is created by that surfacing alone, which puts
    /// its SpawnAreaObject down (state.rs `surface`); `spawn_now` asks this so it does not put it
    /// down a second time.
    pub fn is_morph_target(&self, unit: u16) -> bool {
        self.cards.iter().any(|c| c.spawn_pathfind.is_some_and(|s| s.morph == Some(unit)))
    }

    /// DROP EVERY UNIT BLOCK of card `idx` that `unit_refs` names, and its death area
    /// effect, death projectile, deploy area and spawn area: the cleanup of a card rejected
    /// after its push. A card rejected for any reason runs, where it can still be reached at
    /// all, exactly as it ran before its blocks resolved.
    fn clear_unit_refs(&mut self, idx: u16) {
        for (path, _, _) in self.unit_refs(idx) {
            let card = &mut self.cards[idx as usize];
            match path {
                UnitRef::SpellRelease => {
                    if let Some(slot) = card.spell.as_mut().and_then(|d| d.shape.release_slot()) {
                        *slot = None;
                    }
                }
                UnitRef::Spawner => card.spawner = None,
                UnitRef::DeathSpawn => card.death_spawn = None,
                UnitRef::SecondSummon => card.formation.second_summon = None,
                UnitRef::DeathProjectile => card.death_projectile = None,
                // A summon with no unit summons nothing: the spell goes with it.
                UnitRef::SpellSummon => card.spell = None,
                UnitRef::LifeState => card.life_state = None,
                // A tunneller whose building never resolved does not tunnel: the walk goes with it.
                UnitRef::Morph => card.spawn_pathfind = None,
                UnitRef::Attach => card.attach = None,
                UnitRef::SummonMember(_) => card.summon_members = None,
                // A variant with no forms plays nothing: the spell goes with it, as a summon's does.
                UnitRef::VariantForm(_) => card.spell = None,
                // The unit is on the buff's row, which other cards share: the card holds nothing to drop.
                UnitRef::BuffDeathSpawn => {}
                UnitRef::Transform => card.transform_at_hp = None,
                // A spell whose schedule lost a unit goes whole (a death or projectile area goes below).
                UnitRef::Scheduled(_) => card.spell = None,
            }
        }
        let card = &mut self.cards[idx as usize];
        card.death_area_effect = None;
        card.death_projectile = None;
        card.deploy_area_effect = None;
        card.spawn_area_effect = None;
        card.projectile_area = None;
    }

    /// Register a card under its internal name, and under its display name
    /// too when that does not collide ("Archers" and "Archer" both resolve).
    fn push(&mut self, c: CardDef, display: Option<String>) -> Result<(), String> {
        if self.by_name.contains_key(&c.name) {
            return Err(format!("duplicate card {}", c.name));
        }
        let idx = u16::try_from(self.cards.len()).map_err(|_| "too many cards")?;
        self.by_name.insert(c.name.clone(), idx);
        if let Some(d) = display {
            self.by_name.entry(d).or_insert(idx);
        }
        self.cards.push(c);
        Ok(())
    }

    /// Read data/derived/cards.json from the repository this crate was built in.
    pub fn load_repo() -> Result<CardDb, String> {
        CardDb::load_repo_file("cards.json")
    }

    /// Read another vintage's file from data/derived/ (tools/extract_cards.py
    /// `--vintage 2018` writes cards-2018.json beside cards.json): what a fixture
    /// recorded against that vintage loads (tests/stacked_tie.rs).
    pub fn load_repo_file(name: &str) -> Result<CardDb, String> {
        let path = format!("{}/../../data/derived/{name}", env!("CARGO_MANIFEST_DIR"));
        let s = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
        CardDb::from_json_str(&s, CardSource::DerivedJson)
    }

    /// cards.json if it exists, else the FALLBACK set. Check `source`.
    pub fn load_repo_or_fallback() -> CardDb {
        CardDb::load_repo().unwrap_or_else(|_| CardDb::fallback())
    }

    /// FALLBACK: Knight, Giant, Archers, Minions + both towers, 2018 CSV level-1
    /// values. For building and testing only.
    pub fn fallback() -> CardDb {
        CardDb::from_json_str(FALLBACK_CARDS_JSON, CardSource::Fallback).expect("fallback cards parse")
    }

    #[inline]
    pub fn index(&self, name: &str) -> Option<u16> {
        self.by_name.get(name).copied()
    }

    #[inline]
    pub fn get(&self, idx: u16) -> &CardDef {
        &self.cards[idx as usize]
    }

    /// The lowest unified level at which EVERY rarity in the loaded rarities.csv
    /// has a card level: max(RelativeLevel) + 1 (9 on the 2018 table, where
    /// Legendary starts). Derived from the table, never typed in.
    ///
    /// WHY IT EXISTS: unified level 1 is not a valid level for a Rare, Epic or
    /// Legendary card, so defaulting a deck to it fails in `try_new` with
    /// "Musketeer (Rare) has no level 1" as soon as the deck is non-empty. Callers
    /// that want "the lowest level every card in the catalogue has" want this.
    pub fn lowest_level_valid_for_every_rarity(&self) -> i32 {
        self.rarities
            .iter()
            .filter(|r| r.level_count > 0)
            .map(|r| r.relative_level + 1)
            .max()
            .unwrap_or(1)
    }

    pub fn rarity(&self, name: &str) -> Option<&RarityRow> {
        self.rarities.iter().find(|r| r.name == name)
    }

    /// Percent multiplier for a card at a unified level, or Err if the level
    /// does not exist for that rarity. Never clamps: a silently clamped level is
    /// a silently wrong battle.
    pub fn level_multiplier(&self, idx: u16, level: i32) -> Result<i32, String> {
        let c = self.get(idx);
        let r = self.rarity(&c.rarity).ok_or_else(|| format!("{}: unknown rarity", c.name))?;
        // The CARD's rarity bounds the playable levels...
        let local = level - r.relative_level;
        if local < 1 || local > r.level_count {
            return Err(format!(
                "{} ({}) has no level {level} (local {local}, valid 1..={})",
                c.name, c.rarity, r.level_count
            ));
        }
        // ...and the ladder is entered from the record's base level: the OBJECT's
        // rarity's local 1 when cards.json says so (`level_base`), else the card's
        // (module doc, LEVEL SCALING; calibration combat.STAT_BASE_LEVEL).
        #[cfg(not(clash_plant = "level_card_rarity_local"))]
        let base = c.level_base.unwrap_or(r.relative_level + 1);
        #[cfg(clash_plant = "level_card_rarity_local")]
        // PLANT: the earlier arithmetic, the card's rarity from its local 1
        // whatever the block says (a 15.535 Rare Hog Rider at 11 = 663 x 212 %).
        let base = r.relative_level + 1;
        let step = level - base;
        if step < 0 {
            return Err(format!("{}: level {level} is below its base level {base}", c.name));
        }
        if step == 0 && c.level_table.is_none() {
            return Ok(PERCENT as i32);
        }
        let short = || format!("{}: multiplier table too short for level {level} (base {base})", c.name);
        match c.level_table.as_deref() {
            Some(by_level) => by_level.get(step as usize).copied().ok_or_else(short),
            None => r.multipliers.get((step - 1) as usize).copied().ok_or_else(short),
        }
    }

    /// A level-1 stat at a unified level. Truncating integer division -- see
    /// module doc.
    pub fn scaled(&self, idx: u16, level: i32, base: i32) -> Result<i32, String> {
        let m = self.level_multiplier(idx, level)?;
        #[cfg(clash_plant = "pow11_level")]
        {
            // PLANT: the folklore 1.1^(L-1) scaling, in integers.
            let _ = m;
            let c = self.get(idx);
            let local = level - self.rarity(&c.rarity).map(|r| r.relative_level).unwrap_or(0);
            let n = (local - 1).max(0) as u32;
            return Ok(((base as i128) * 11i128.pow(n) / 10i128.pow(n)) as i32);
        }
        #[allow(unreachable_code)]
        Ok(Self::scale(base, m))
    }

    #[inline]
    pub fn scale(base: i32, multiplier_percent: i32) -> i32 {
        (((base as i64) * (multiplier_percent as i64)) / PERCENT) as i32
    }

    /// THIS CARD DATA WITH SOME VALUES REPLACED (calibration cards.CLIENT16402_VALUES =
    /// client16402; state.rs `with_card_values`): each named card's named column takes the
    /// value given, as a level-1 base, so level scaling applies to it as to the table's value.
    /// cards.json is not touched. Called only on the table the values correct (value.table, the
    /// 15.535.29 extraction; `with_card_values` checks `version`), so on that table it is REFUSED,
    /// never skipped: a card this data does not load, and a column the card does not carry
    /// (hitpoints on a spell, a projectile's damage on a card with no projectile, a crown-tower
    /// percent on a card with no hit to carry it, an area HitSpeed on a card that is not a striking
    /// area). Setting a value twice gives the same data, so a battle built from data that already
    /// carries the values is unchanged.
    pub fn with_values(&self, values: &[CardValue]) -> Result<CardDb, String> {
        let mut db = self.clone();
        for v in values {
            let what = format!("cards.CLIENT16402_VALUES {}.{:?}", v.card, v.column);
            let idx = db.index(&v.card).ok_or_else(|| format!("{what}: no card of that name is loaded"))?;
            let c = &mut db.cards[idx as usize];
            match v.column {
                CardColumn::Hitpoints => {
                    if c.kind == CardKind::Spell {
                        return Err(format!("{what}: a spell has no hitpoints"));
                    }
                    c.hitpoints = v.value;
                }
                // A ranged card's damage IS its projectile row's Damage (`convert`).
                CardColumn::ProjectileDamage => {
                    if c.projectile.is_none() {
                        return Err(format!("{what}: the card fires no projectile"));
                    }
                    c.damage = v.value;
                }
                // The raw column is a delta from 100 (cards.json keeps the effective percent,
                // 100 + the raw value): -77 is a 23 % share.
                CardColumn::CrownTowerDamagePercent => {
                    let pct = PERCENT_I32 + v.value;
                    match c.spell.as_mut().map(|d| &mut d.shape) {
                        Some(SpellShape::Projectile { hit: Some(h), .. })
                        | Some(SpellShape::AreaEffect { hit: h })
                        | Some(SpellShape::PulsingAreaEffect { hit: h, .. })
                        | Some(SpellShape::Rolling { hit: h, .. }) => h.crown_pct = pct,
                        Some(SpellShape::Projectile { hit: None, .. }) => return Err(format!("{what}: the spell deals no damage")),
                        Some(SpellShape::Strikes(d)) => {
                            d.hit.crown_pct = pct;
                            // A centre-aimed strike's damage lands through its delivery's hit.
                            if let Some(SpellShape::Projectile { hit: Some(h), .. }) = d.delivery.as_deref_mut() {
                                h.crown_pct = pct;
                            }
                        }
                        Some(SpellShape::Fuse { .. } | SpellShape::Summon { .. } | SpellShape::Mirror | SpellShape::Variant { .. } | SpellShape::ScheduledArea { .. } | SpellShape::Clone { .. }) => {
                            return Err(format!("{what}: the spell's own object carries no crown-tower share"))
                        }
                        None => c.crown_tower_damage_percent = pct,
                    }
                }
                // A striking area's HitSpeed (Lightning: 460 in the 15.535.29 tables, 500 on client 16.402). Its gaps
                // are rebuilt by the tables' own rule (`strike_gaps`) against its LifeDuration; the clock that reads
                // them is spells.STRIKE_TIMER_LEFTOVER's.
                CardColumn::AreaHitSpeed => {
                    let Some(SpellShape::Strikes(d)) = c.spell.as_mut().map(|d| &mut d.shape) else {
                        return Err(format!("{what}: only a striking area's HitSpeed is replaced (Lightning)"));
                    };
                    if v.value <= 0 {
                        return Err(format!("{what}: {} is not a HitSpeed", v.value));
                    }
                    let gaps_ms = strike_gaps(v.value, d.life_ms);
                    if gaps_ms.is_empty() {
                        return Err(format!("{what}: a {} ms HitSpeed never strikes inside a {} ms LifeDuration", v.value, d.life_ms));
                    }
                    d.gaps_ms = gaps_ms;
                }
            }
        }
        Ok(db)
    }
}

/// ONE CARD VALUE A LEDGER OVERLAY REPLACES (calibration cards.CLIENT16402_VALUES; `CardDb::
/// with_values`): the card by its name in this data, the column, and the value as the card data
/// ships it (a level-1 base; the crown-tower percent raw).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CardValue {
    pub card: String,
    pub column: CardColumn,
    pub value: i32,
}

/// The columns cards.CLIENT16402_VALUES may replace, by the name the ledger gives them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CardColumn {
    /// Hitpoints: the card's own base.
    Hitpoints,
    /// The Damage of the card's projectile row (the Bomber's bomb).
    ProjectileDamage,
    /// CrownTowerDamagePercent, raw: every hit a spell carries, or the card's own.
    CrownTowerDamagePercent,
    /// The HitSpeed of a striking area's row, ms (Lightning); its strike gaps follow it (`strike_gaps`).
    AreaHitSpeed,
}

impl CardColumn {
    pub fn from_ledger_name(s: &str) -> Option<CardColumn> {
        match s {
            "Hitpoints" => Some(CardColumn::Hitpoints),
            "ProjectileDamage" => Some(CardColumn::ProjectileDamage),
            "CrownTowerDamagePercent" => Some(CardColumn::CrownTowerDamagePercent),
            "AreaHitSpeed" => Some(CardColumn::AreaHitSpeed),
            _ => None,
        }
    }
}

impl CardValue {
    /// cards.CLIENT16402_VALUES value.values, `{card: {column: value}}`, as a list in card-name
    /// order (serde_json's map is sorted, not the ledger's order; each entry names its own card
    /// and column, so the order changes nothing). Refused at load: a missing or malformed block, a
    /// column name the overlay does not know, a value that is not an i32. Card names are checked
    /// where the list is applied (`CardDb::with_values`), against the table the values correct.
    pub fn list_from_ledger(v: &serde_json::Value) -> Result<Vec<CardValue>, String> {
        let key = "cards.CLIENT16402_VALUES.value.values";
        let cards = v
            .pointer("/cards/CLIENT16402_VALUES/value/values")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| format!("{key}: an object of card names is required"))?;
        let mut out = Vec::new();
        for (card, cols) in cards {
            let cols = cols.as_object().ok_or_else(|| format!("{key}.{card}: an object of column names is required"))?;
            for (col, val) in cols {
                let column = CardColumn::from_ledger_name(col).ok_or_else(|| format!("{key}.{card}.{col}: not a column the overlay replaces (Hitpoints, ProjectileDamage, CrownTowerDamagePercent, AreaHitSpeed)"))?;
                let value = val
                    .as_i64()
                    .and_then(|x| i32::try_from(x).ok())
                    .ok_or_else(|| format!("{key}.{card}.{col}: {val} is not an i32"))?;
                out.push(CardValue { card: card.clone(), column, value });
            }
        }
        Ok(out)
    }

    /// cards.CLIENT16402_VALUES value.table: the `version` of the one card table the values
    /// correct (`CardDb::version`). Refused at load when missing or empty: the values would
    /// then apply to no table, silently.
    pub fn table_from_ledger(v: &serde_json::Value) -> Result<String, String> {
        v.pointer("/cards/CLIENT16402_VALUES/value/table")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| "cards.CLIENT16402_VALUES.value.table: the version of the card table the values correct is required".to_string())
    }
}

/// FALLBACK tower definitions (2018 buildings.csv / projectiles.csv, level 1;
/// NoDeploySizeW/H from the same buildings.csv rows).
/// PrincessTower damage comes from TowerPrincessProjectile, KingTower's from
/// KingProjectile; neither building row ships its own Damage.
const FALLBACK_TOWERS_JSON: &str = r#"{ "version": "fallback", "cards": [
 { "name":"KingTower", "kind":"building", "elixir":0, "rarity":"Common",
   "hitpoints":2400, "damage":50, "hit_speed_ms":1000, "load_time_ms":500, "speed":0,
   "range_milli":7000, "sight_range_milli":7000, "collision_radius_milli":1400,
   "deploy_time_ms":0, "attacks_air":true, "attacks_ground":true,
   "projectile":{"speed":1000}, "count":1, "crown_tower_damage_percent":100,
   "no_deploy_size_tiles":[18,16] },
 { "name":"PrincessTower", "kind":"building", "elixir":0, "rarity":"Common",
   "hitpoints":1400, "damage":50, "hit_speed_ms":800, "load_time_ms":0, "speed":0,
   "range_milli":7500, "sight_range_milli":7500, "collision_radius_milli":1000,
   "deploy_time_ms":0, "attacks_air":true, "attacks_ground":true,
   "projectile":{"speed":600}, "count":1, "crown_tower_damage_percent":100,
   "no_deploy_size_tiles":[11,21] }
]}"#;

/// FALLBACK troop set (2018 characters.csv level-1 values; elixir from the
/// card list). Archer/Minion damage is their projectile's Damage.
const FALLBACK_CARDS_JSON: &str = r#"{ "version": "fallback", "cards": [
 { "name":"Knight", "kind":"troop", "elixir":3, "rarity":"Common",
   "hitpoints":660, "damage":75, "hit_speed_ms":1100, "load_time_ms":700, "speed":60,
   "range_milli":1000, "sight_range_milli":5500, "collision_radius_milli":500, "mass":6,
   "deploy_time_ms":1000, "attacks_air":false, "attacks_ground":true,
   "target_only_buildings":false, "flying_height":0, "count":1 },
 { "name":"Giant", "kind":"troop", "elixir":5, "rarity":"Rare",
   "hitpoints":1900, "damage":120, "hit_speed_ms":1500, "load_time_ms":1000, "speed":45,
   "range_milli":1250, "sight_range_milli":7500, "collision_radius_milli":750, "mass":18,
   "deploy_time_ms":1000, "attacks_air":false, "attacks_ground":true,
   "target_only_buildings":true, "flying_height":0, "count":1 },
 { "name":"Archers", "kind":"troop", "elixir":3, "rarity":"Common",
   "hitpoints":120, "damage":41, "hit_speed_ms":1200, "load_time_ms":1100, "speed":60,
   "range_milli":5000, "sight_range_milli":5500, "collision_radius_milli":500, "mass":3,
   "deploy_time_ms":1000, "attacks_air":true, "attacks_ground":true,
   "target_only_buildings":false, "flying_height":0, "count":2,
   "projectile":{"speed":600} },
 { "name":"Minions", "kind":"troop", "elixir":3, "rarity":"Common",
   "hitpoints":90, "damage":40, "hit_speed_ms":1000, "load_time_ms":500, "speed":90,
   "range_milli":2000, "sight_range_milli":5500, "collision_radius_milli":500, "mass":2,
   "deploy_time_ms":1000, "attacks_air":true, "attacks_ground":true,
   "target_only_buildings":false, "flying_height":1500, "count":3,
   "projectile":{"speed":1000} }
]}"#;

// cards.json SCHEMA (produced by the data partition, consumed here):
// { "version": "...", "provenance": {...},
//   "cards": [ { "name":"Knight", "kind":"troop|building|spell", "elixir":3, "rarity":"Common",
//     "hitpoints":1400, "damage":167, "hit_speed_ms":1200, "load_time_ms":1000, "speed":60,
//     "range_milli":1600, "sight_range_milli":5500, "collision_radius_milli":500, "mass":6,
//     "deploy_time_ms":1000, "attacks_air":false, "attacks_ground":true, "target_only_buildings":false,
//     "flying_height":0, "area_damage_radius_milli":0, "projectile":null, "count":1,
//     "shield_hitpoints":0, "crown_tower_damage_percent":100, "level_scaling":{...} } ] }
// What this loader actually reads beyond that: a top-level "towers" array (same record
// shape), "display_name" (registered as an alias), "death_damage",
// "death_damage_radius_milli", "self_as_aoe_center", "lifetime_ms", projectile as an
// object {"speed", "damage", "radius_milli"}, and level_scaling.multiplier_percent_by_level
// (entry L-1 = percent at local level L), and "no_deploy_size_tiles": [W, H] on towers.
// Also "hides_when_not_attacking", "hide_time_ms", "up_time_ms" (buildings;
// all three or none, see `convert`); the "spawner" block {character, number,
// interval_ms, start_time_ms, pause_time_ms, limit, radius_milli} and the "death_spawn" block
// {character, count, radius_milli, deploy_time_ms} (`convert_spawner` / `convert_death_spawn`;
// the named unit is loaded from "units" like a spell's). A building with no damage source may
// omit "range_milli" (the huts). Also the "charge" block {damage_special,
// charge_range_raw, charge_speed_multiplier_percent} (`convert_charge`; all three or the card is
// refused; troops only), and the "jump" block {height_raw, speed} on JumpEnabled rows
// (`convert_jump`; both or the card is refused; troops only), and the "dash" block {damage,
// min_range_milli, max_range_milli, radius_milli, cooldown_ms, immune_to_damage_time_ms,
// pushback_milli, speed, constant_time_ms, landing_time_ms} on rows with DashMaxRange
// (`convert_dash`; no "speed" key loads no dash; troops only). Also "death_area_effect":
// the NAME of a row in the top-level "area_effect_objects" map, whose records have the
// shape a spell's inline "area_effect_object" block has {name, life_duration_ms,
// radius_milli, hit_speed_ms, damage, crown_tower_damage_percent, buff, buff_time_ms,
// only_enemies, only_own_troops, hits_ground, hits_air, ignore_buildings, pushback_milli,
// maximum_targets, projectile, spawn_character, action_graph}; a named row the map does
// not carry refuses the card (`convert_area_effect`). Also "death_spawn_pushback", a
// boolean beside the "death_spawn" block (15.535 rows only; absent reads false), and the
// "reflected_attack" block {damage, crown_tower_damage, radius_milli, buff, buff_duration_ms},
// present only on a row that sets a ReflectedAttack column (`convert_reflect`; damage and
// radius or the card is refused, the buff with its duration or neither). Also
// "load_first_hit" (a boolean; absent reads false), "attack_pushback_milli" (absent reads
// 0), the "variable_damage" block {damage2, damage3, time1_ms, time2_ms}
// (`convert_variable_damage`; all four or the card is refused) and the "special" block
// {range_milli, min_range_milli, load_time_ms, projectile, drag_margin_milli}
// (`convert_special`; troops only). The last three are not written by tools/extract_cards.py
// yet, and each reads as absent until it is. Also three NAMES, each absent unless the
// extractor writes it: "death_spawn_projectile" (a row of the top-level "projectiles" map, the
// shape a spell's "projectile" block has; `convert_death_projectile`), "deploy_area_effect" and
// "spawn_area_object" (rows of "area_effect_objects"; `convert_deploy_area_effect`,
// `convert_spawn_area_effect`). Also "minimum_range_milli" (MinimumRange; null reads as none).
// Also the "spawn_pathfind" block {speed, morph} (`spawn_pathfind_of`: a positive speed, and on
// a played card "can_deploy_on_enemy_side" true, or the card is refused; a units row that
// carries it is refused), "morph" the NAME of a "units" row loaded as the card's building.
// Also "attach" in the "spawner" block (SpawnAttach: the block is an attached rider,
// `convert_attach`), and on a rider row "target_only_troops" and "ignore_targets_with_buff" (a buff
// row) with "deprioritize_targets_with_buff" (`CardDef::deprioritize_buff`; the buff alone refuses
// the row); each is absent unless the 15.535 row sets it.
// Also, on the 15.535 rows only: "summon_members" [{character, offset_x_milli, offset_y_milli}] with
// "summon_offsets_x_mirrored" (a SummonCharactersList card's explicit offsets; all-or-nothing), the
// "attack_select" block {melee_range_milli, melee_ground_only, melee_damage, melee_index, ranged_index}
// (`attack_select_of`; the one shape or the card is refused), a spell's "mirror" (true on the Mirror
// alone) and "variant" block {options: [{trigger_milli, precast_pending_ms, card}],
// use_projected_time_summon, mirror_uses_root_spell} (`convert_variant`), and a top-level "globals"
// map of the table's own globals.csv rows the loader reads (`CardGlobals`).
// On the 15.535 rows only (absent reads blank): "ignore_buffs" (IgnoreBuff, a list of buff names;
// `CardDef::ignore_buffs`), the spawner block's "character2" (SpawnCharacter2, refused), a projectile's
// "apply_buff_before_damage" (`CardDef::attack_buff_first`), an area's "schedule" and "on_hit" (what its
// OnStartingAction and OnHitAction run, {root, entries: [{delay_ms, action, class, cosmetic, spawn_type, spawn,
// spawn_time_ms, buff, unread}]}; `area_spawns_area`, `on_hit_buffs`) and "spawn_time_ms" (SpawnTime;
// `centre_strike_shape`), and a buff's "death_spawn" block {character, count, is_enemy, deploy_delay,
// same_location, other_buff_death_spawn_allowed}, "ignore_buildings" and "crown_tower_damage_per_hit"
// (`RawBuff::convert`).
// Unknown fields are ignored.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_volley_disc_is_the_spells_radius_and_the_single_carriers_its_own() {
        // calibration spells.WAVE_AREA_MODEL = single_disc_one_hit_per_wave, pinned on a
        // synthetic row whose two radii are APART (the shipped 2018 Arrows had them
        // equal, 4000 = 4000, so no shipped-data test could tell the readings apart):
        // a volley (MultipleProjectiles > 1) hits the SPELL's
        // radius per wave, a single projectile its carrier's, and a volley with no
        // spell radius to spread over is refused.
        let row = |multiple: &str, spell_radius: &str| {
            format!(
                r#"{{"cards":[{{"name":"Volley","kind":"spell","elixir":3,"rarity":"Common",
                "spell":{{"radius_milli":{spell_radius},"multiple_projectiles":{multiple},"projectile_waves":3,"projectile_wave_interval_ms":200}},
                "projectile":{{"name":"VolleyArrow","speed":1100,"damage":48,"homing":true,"radius_milli":1400,"aoe_to_air":true,"aoe_to_ground":true,"only_enemies":true}}}}]}}"#
            )
        };
        let disc = |text: &str| -> Result<i32, String> {
            let db = CardDb::from_json_str(text, CardSource::DerivedJson).unwrap();
            let idx = db.index("Volley").ok_or_else(|| format!("{:?}", db.rejected))?;
            match &db.get(idx).spell.as_ref().unwrap().shape {
                SpellShape::Projectile { hit: Some(h), waves, .. } => {
                    assert_eq!(*waves, 3);
                    Ok(h.radius)
                }
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(disc(&row("10", "3500")), Ok(milli(3500)), "a volley: the spell's radius");
        assert_eq!(disc(&row("1", "3500")), Ok(milli(1400)), "a single projectile: the carrier's radius");
        assert_eq!(disc(&row("null", "3500")), Ok(milli(1400)), "no column: one projectile");
        let refused = disc(&row("10", "null")).unwrap_err();
        assert!(refused.contains("no spell Radius"), "{refused}");
    }

    #[test]
    fn rarity_table_parses_from_supercell_csv() {
        let r = shipped_rarities();
        let common = r.iter().find(|x| x.name == "Common").unwrap();
        assert_eq!(common.relative_level, 0);
        assert_eq!(&common.multipliers[..4], &[110, 121, 133, 146]);
        let rare = r.iter().find(|x| x.name == "Rare").unwrap();
        assert_eq!(rare.relative_level, 2);
        assert_eq!(rare.level_count, 11);
    }

    #[test]
    fn hog_rider_ladder_is_exact_and_not_pow_1_1() {
        // Hog Rider: Rare, 800 HP at rarity-local level 1 = unified level 3.
        let db = CardDb::from_json_str(
            r#"{"cards":[{"name":"HogRider","kind":"troop","rarity":"Rare","hitpoints":800,
            "hit_speed_ms":1500,"range_milli":800,"collision_radius_milli":600}]}"#,
            CardSource::DerivedJson,
        )
        .unwrap();
        let i = db.index("HogRider").unwrap();
        let got: Vec<i32> =
            (3..=10).map(|lvl| db.scaled(i, lvl, 800).unwrap()).collect();
        assert_eq!(got, vec![800, 880, 968, 1064, 1168, 1280, 1408, 1544]);
        assert!(db.level_multiplier(i, 2).is_err(), "rares do not exist below unified level 3");
        assert!(db.level_multiplier(i, 14).is_err());
    }

    #[test]
    fn fallback_is_labelled_and_has_towers() {
        let db = CardDb::fallback();
        assert_eq!(db.source, CardSource::Fallback);
        for n in ["Knight", "Giant", "Archers", "Minions", KING_TOWER, PRINCESS_TOWER] {
            assert!(db.index(n).is_some(), "{n}");
        }
        assert!(db.get(db.index("Minions").unwrap()).is_flying());
    }

    #[test]
    fn absent_towers_are_filled_and_flagged() {
        let db = CardDb::from_json_str(
            r#"{"cards":[{"name":"Knight","kind":"troop","rarity":"Common","hitpoints":1,
            "hit_speed_ms":1,"range_milli":1,"collision_radius_milli":1}]}"#,
            CardSource::DerivedJson,
        )
        .unwrap();
        assert!(db.towers_from_fallback);
        assert!(db.index(KING_TOWER).is_some());
    }

    #[test]
    fn spells_and_bad_cards_are_rejected_not_silently_dropped() {
        // Spells load; a spell whose data carries NO mechanic (here: no
        // projectile, no area effect) is refused, with a reason.
        let db = CardDb::from_json_str(
            r#"{"cards":[{"name":"Zap","kind":"spell","rarity":"Common"},
            {"name":"NoHp","kind":"troop","rarity":"Common","hit_speed_ms":1,"range_milli":1,"collision_radius_milli":1}]}"#,
            CardSource::DerivedJson,
        )
        .unwrap();
        assert_eq!(db.rejected.len(), 2);
        assert!(db.index("Zap").is_none());
        assert!(db.rejected[0].1.contains("no mechanic"), "{:?}", db.rejected[0]);
    }

    /// THE UNIT-NAME RULE, the resolution half: a card serves as a spawned unit only
    /// when the row it puts on the board IS that unit's (`CardDef::unit_name`). The Ram
    /// Rider card is named `RamRider` and deploys the `Ram`; its rider is the `units`
    /// row `RamRider`, reached through the Ram's attached-rider block (`AttachDef`), so
    /// resolving the rider by name alone hands back the Ram. Synthetic rows, the two
    /// records told apart by their hitpoints.
    /// Plant: unit_by_card_name (any troop or building card of the name serves).
    #[test]
    fn the_rider_resolves_to_its_own_row_not_the_card() {
        let db = CardDb::from_json_str(
            r#"{"cards":[{"name":"RamRider","kind":"troop","elixir":5,"rarity":"Legendary","summon_character":"Ram",
            "hitpoints":1500,"hit_speed_ms":1800,"range_milli":500,"collision_radius_milli":750,
            "spawner":{"character":"RamRider","number":1,"attach":true}}],
            "units":{"RamRider":{"name":"RamRider","rarity":"Legendary","hitpoints":232,"hit_speed_ms":1100,
            "range_milli":5000,"collision_radius_milli":500}}}"#,
            CardSource::DerivedJson,
        )
        .unwrap();
        let card = db.index("RamRider").unwrap_or_else(|| panic!("RamRider refused: {:?}", db.rejected));
        assert!(db.get(card).spawner.is_none(), "the SpawnAttach block was read as a periodic spawner");
        let rider = db.get(card).attach.expect("the attached-rider block resolved").unit;
        assert_ne!(rider, card, "the rider resolved to the card, whose row is the Ram");
        let r = db.get(rider);
        assert_eq!((r.hitpoints, r.summon_only, r.unit_name.as_str()), (232, true, "RamRider"), "the rider is not the units row");
        assert_eq!((db.get(card).hitpoints, db.get(card).unit_name.as_str()), (1500, "Ram"));
    }

    /// ...and the half that must not move: a card whose row IS the unit's still serves,
    /// one table and no second record. The 2018 Furnace spawns `FireSpirits`, the Fire
    /// Spirits card's own SummonCharacter. The synthetic file ships no `units` row of
    /// that name, so a rule that stopped serving the card would refuse the hut instead;
    /// the shipped 2018 file must resolve the same way.
    /// Plant: unit_never_card (no card serves; the hut is refused here, and in the 2018
    /// file its spawner no longer names the Fire Spirits card).
    #[test]
    fn the_old_shortcut_still_serves_firespirits() {
        let db = CardDb::from_json_str(
            r#"{"cards":[{"name":"FireSpirits","kind":"troop","elixir":2,"rarity":"Common","summon_character":"FireSpirits",
            "hitpoints":90,"hit_speed_ms":300,"range_milli":2000,"collision_radius_milli":400,"count":3},
            {"name":"FirespiritHut","kind":"building","elixir":4,"rarity":"Rare","hitpoints":570,"hit_speed_ms":10000,
            "collision_radius_milli":1000,"lifetime_ms":50000,
            "spawner":{"character":"FireSpirits","number":2,"interval_ms":500,"pause_time_ms":9400}}]}"#,
            CardSource::DerivedJson,
        )
        .unwrap();
        let hut = db.index("FirespiritHut").unwrap_or_else(|| panic!("FirespiritHut refused: {:?}", db.rejected));
        assert_eq!(db.get(hut).spawner.expect("the spawner resolved").unit, db.index("FireSpirits").unwrap());
        assert!(db.cards.iter().all(|c| !c.summon_only), "a card that serves as the unit was loaded a second time");
        let shipped = CardDb::load_repo_file("cards-2018.json").expect("cards-2018.json (tools/extract_cards.py --vintage 2018)");
        let hut = shipped.index("FirespiritHut").unwrap_or_else(|| panic!("2018 FirespiritHut refused: {:?}", shipped.rejected.iter().find(|(n, _)| n == "FirespiritHut")));
        assert_eq!(shipped.get(hut).spawner.expect("the 2018 hut's spawner").unit, shipped.index("FireSpirits").unwrap(), "2018: the Furnace does not spawn the Fire Spirits card");
    }

    /// THE UNIT-NAME RULE, the record half: a `units` row that loads under a name a card
    /// already holds is registered as `units.<Name>`, keeping its `unit_name`, so every
    /// name resolves to ONE record and the card list never prints two records under one
    /// name. The Goblin Drill card deploys `GoblinDrillDig`, which morphs into the
    /// `units` row `GoblinDrill` -- the card's own name. The morph is not simulated yet;
    /// a death spawn stands in for it (the name rule is the same for every block). A
    /// spell of the unit's name is still a collision. Synthetic rows.
    /// Plant: unit_by_card_name.
    #[test]
    fn a_morph_target_named_like_its_card_is_not_the_card() {
        let db = CardDb::from_json_str(
            r#"{"cards":[{"name":"GoblinDrill","kind":"building","elixir":4,"rarity":"Epic","summon_character":"GoblinDrillDig",
            "hitpoints":2560,"hit_speed_ms":1000,"collision_radius_milli":1000,
            "death_spawn":{"character":"GoblinDrill","count":1}},
            {"name":"Zap","kind":"spell","elixir":2,"rarity":"Common",
            "spell":{"area_effect_object":{"name":"ZapArea","radius_milli":2500,"damage":75,"hits_ground":true,"hits_air":true,"only_enemies":true}}},
            {"name":"Sparky","kind":"troop","elixir":6,"rarity":"Legendary","hitpoints":1200,"hit_speed_ms":4000,
            "range_milli":5000,"collision_radius_milli":750,"death_spawn":{"character":"Zap","count":1}}],
            "units":{"GoblinDrill":{"name":"GoblinDrill","source_table":"buildings","rarity":"Epic","hitpoints":1313,
            "hit_speed_ms":1000,"collision_radius_milli":600,"lifetime_ms":9000}}}"#,
            CardSource::DerivedJson,
        )
        .unwrap();
        let card = db.index("GoblinDrill").unwrap_or_else(|| panic!("GoblinDrill refused: {:?}", db.rejected));
        let unit = db.get(card).death_spawn.expect("the stand-in block resolved").unit;
        assert_ne!(unit, card, "the morph target resolved to the card itself");
        let u = db.get(unit);
        assert_eq!(
            (u.name.as_str(), u.unit_name.as_str(), u.hitpoints, u.kind, u.summon_only),
            ("units.GoblinDrill", "GoblinDrill", 1313, CardKind::Building, true)
        );
        assert_eq!(db.index("units.GoblinDrill"), Some(unit));
        assert_eq!(db.get(card).hitpoints, 2560, "the name no longer reaches the card");
        let mut names: Vec<&str> = db.cards.iter().map(|c| c.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), db.cards.len(), "two records share a name");
        let why = &db.rejected.iter().find(|(n, _)| n == "Sparky").expect("Sparky is refused").1;
        assert_eq!(why, "spawned unit Zap collides with a card name");
    }

    /// The thin slice's five spells load from the REAL cards.json with the shapes the
    /// data implies, every number read back from the file (never typed here), and the
    /// spells whose mechanic is not simulated are refused with a reason.
    /// Plant: spells_rejected (a loader that refuses every spell).
    #[test]
    fn shipped_spells_load_with_the_shapes_their_data_implies() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
        let doc: serde_json::Value = serde_json::from_str(&text).unwrap();
        let db = CardDb::from_json_str(&text, CardSource::DerivedJson).unwrap();
        let card = |n: &str| doc["cards"].as_array().unwrap().iter().find(|c| c["name"] == n).unwrap().clone();
        let int = |v: &serde_json::Value| v.as_i64().unwrap() as i32;
        let spell = |n: &str| db.get(db.index(n).unwrap_or_else(|| panic!("{n} not simulable: {:?}", db.rejected))).spell.clone().unwrap();

        let fb = card("Fireball");
        match spell("Fireball") {
            SpellDef { shape: SpellShape::Projectile { speed, hit: Some(h), waves: 1, wave_interval_ms: 0, spawn: None }, placement: SpellPlacement::Anywhere } => {
                assert_eq!(speed, int(&fb["projectile"]["speed"]));
                assert_eq!(h.damage, int(&fb["projectile"]["damage"]));
                assert_eq!(h.radius, milli(int(&fb["projectile"]["radius_milli"])));
                assert_eq!(h.crown_pct, int(&fb["projectile"]["crown_tower_damage_percent"]));
                assert_eq!(h.knockback, Some(KnockbackDef { distance: milli(int(&fb["projectile"]["pushback_milli"])), all: false }));
                assert!(h.hits_air && h.hits_ground && h.only_enemies);
            }
            other => panic!("Fireball: {other:?}"),
        }
        // Arrows: the damage carrier is the CustomFirstProjectile when the row has one
        // (2018: the deco `projectile` carries no damage), else the Projectile (15.535);
        // the waves are the row's (2018: none = 1; 15.535: 3 x 200 ms); the disc is the
        // SPELL's Radius for a volley (MultipleProjectiles > 1, both vintages), which
        // in 2018 equals the carrier's 4000 and in 15.535 is 3500 over 1400 arrows.
        let ar = card("Arrows");
        let carrier = if ar["spell"]["first_projectile"].is_object() { ar["spell"]["first_projectile"].clone() } else { ar["projectile"].clone() };
        let want_waves = ar["spell"]["projectile_waves"].as_i64().map_or(1, |w| w as i32);
        let want_interval = ar["spell"]["projectile_wave_interval_ms"].as_i64().map_or(0, |w| w as i32);
        let want_disc = if ar["spell"]["multiple_projectiles"].as_i64().unwrap_or(1) > 1 { int(&ar["spell"]["radius_milli"]) } else { int(&carrier["radius_milli"]) };
        match spell("Arrows") {
            SpellDef { shape: SpellShape::Projectile { speed, hit: Some(h), waves, wave_interval_ms, spawn: None }, placement: SpellPlacement::Anywhere } => {
                assert_eq!(speed, int(&carrier["speed"]));
                assert_eq!(h.damage, int(&carrier["damage"]));
                assert_eq!(h.radius, milli(want_disc));
                assert_eq!((waves, wave_interval_ms), (want_waves, want_interval));
                assert_eq!(h.knockback, None);
            }
            other => panic!("Arrows: {other:?}"),
        }
        let zp = card("Zap");
        match spell("Zap") {
            SpellDef { shape: SpellShape::AreaEffect { hit }, placement: SpellPlacement::Anywhere } => {
                let aeo = &zp["spell"]["area_effect_object"];
                assert_eq!(hit.damage, int(&aeo["damage"]));
                // the stun is a BUFF now (SpellHit::buff): the ZapFreeze row, for
                // BuffTime ms, whose three -100 columns compose to a full stop.
                let b = hit.buff.expect("Zap carries its ZapFreeze buff");
                assert_eq!(b.time_ms, int(&aeo["buff_time_ms"]));
                let def = db.buffs[b.buff as usize];
                assert_eq!(def.speed_pct, int(&aeo["buff"]["speed_multiplier_raw"]));
                assert_eq!(crate::status::compose([def].iter(), crate::status::Sel::Speed, 100), 0, "ZapFreeze is a full stop");
                assert_eq!(hit.radius, milli(int(&aeo["radius_milli"])));
                assert_eq!(hit.crown_pct, int(&aeo["crown_tower_damage_percent"]));
            }
            other => panic!("Zap: {other:?}"),
        }
        let lg = card("Log");
        match spell("Log") {
            SpellDef { shape: SpellShape::Rolling { airborne_speed, airborne_min_distance, speed, range, half_width, half_depth, hit, spawn: None }, placement: SpellPlacement::TroopTerritory { on_buildings: true } } => {
                let roll = &lg["projectile"]["spawn_projectile"];
                assert_eq!(airborne_speed, int(&lg["projectile"]["speed"]));
                assert_eq!(airborne_min_distance, milli(int(&lg["projectile"]["min_distance_milli"])));
                assert_eq!(speed, int(&roll["speed"]));
                assert_eq!(range, milli(int(&roll["projectile_range_milli"])));
                assert_eq!(half_width, milli(int(&roll["projectile_radius_milli"])));
                assert_eq!(half_depth, milli(int(&roll["projectile_radius_y_milli"])));
                assert_eq!(hit.damage, int(&roll["damage"]));
                assert_eq!(hit.knockback, Some(KnockbackDef { distance: milli(int(&roll["pushback_milli"])), all: true }));
                assert!(hit.hits_ground && !hit.hits_air);
            }
            other => panic!("Log: {other:?}"),
        }
        let gb = card("GoblinBarrel");
        match spell("GoblinBarrel") {
            SpellDef { shape: SpellShape::Projectile { hit: None, spawn: Some(sp), .. }, placement: SpellPlacement::AnywhereButWater } => {
                let unit = db.get(sp.unit);
                assert_eq!(unit.name, gb["spell"]["spawn"]["character"].as_str().unwrap());
                assert!(unit.summon_only && unit.kind == CardKind::Troop);
                assert_eq!(unit.rarity, doc["units"][unit.name.as_str()]["rarity"].as_str().unwrap());
                assert_eq!(sp.count, int(&gb["spell"]["spawn"]["count"]));
                assert_eq!(sp.deploy_time_ms, Some(int(&gb["spell"]["spawn"]["deploy_time_ms"])));
                // Level: identical to the barrel's unified level under both readings.
                let i = db.index("GoblinBarrel").unwrap();
                let lvl = db.lowest_level_valid_for_every_rarity();
                assert_eq!(db.spawn_level(i, lvl), Ok(lvl));
            }
            other => panic!("GoblinBarrel: {other:?}"),
        }
        assert_eq!(db.get(db.index("Giant").unwrap()).ignore_pushback, doc["cards"].as_array().unwrap().iter().find(|c| c["name"] == "Giant").unwrap()["ignore_pushback"].as_bool().unwrap());
        // What is NOT simulated is refused out loud. Poison loads -- a pulsing area
        // effect whose mechanic is a BUFF (with Earthquake and the Snowball). Rage and
        // Heal load too, as a spell summon (tests/spell_summon.rs pins their shapes), the Graveyard as a scheduled
        // area (tests/scheduled_area.rs) and the Clone as its own shape (tests/clone.rs). The event's GlobalClone,
        // whose area runs an action graph the loader does not read, is still refused.
        let n = "GlobalClone";
        assert!(db.index(n).is_none(), "{n} must not be simulable");
        assert!(db.rejected.iter().any(|(r, _)| r == n), "{n} not listed as rejected");
        assert!(matches!(spell("Graveyard").shape, SpellShape::ScheduledArea { .. }), "Graveyard: {:?}", spell("Graveyard"));
        assert!(matches!(spell("Clone").shape, SpellShape::Clone { .. }), "Clone: {:?}", spell("Clone"));
        // The Mirror and the Spirit Empress load as the two shapes that are never cast
        // (tests/mirror_card.rs and tests/variant_card.rs pin what playing them does).
        assert_eq!(spell("Mirror").shape, SpellShape::Mirror);
        assert!(matches!(spell("MergeMaiden").shape, SpellShape::Variant { .. }), "MergeMaiden: {:?}", spell("MergeMaiden"));
        // THE PULSING AREA EFFECTS, and the buff that IS their mechanic.
        for n in ["Poison", "Earthquake"] {
            match spell(n) {
                SpellDef { shape: SpellShape::PulsingAreaEffect { hit, life_ms, hit_speed_ms, .. }, .. } => {
                    let aeo = &card(n)["spell"]["area_effect_object"];
                    assert_eq!(life_ms, int(&aeo["life_duration_ms"]));
                    assert_eq!(hit_speed_ms, int(&aeo["hit_speed_ms"]));
                    assert_eq!(hit.damage, 0, "{n}: a pulsing area deals no damage of its own");
                    let b = hit.buff.unwrap_or_else(|| panic!("{n} carries no buff"));
                    let def = db.buffs[b.buff as usize];
                    assert_eq!(b.time_ms, int(&aeo["buff_time_ms"]));
                    assert_eq!(def.damage_per_second, int(&aeo["buff"]["damage_per_second"]));
                    assert_eq!(def.hit_frequency_ms, int(&aeo["buff"]["hit_frequency_ms"]));
                    assert_eq!(def.speed_pct, int(&aeo["buff"]["speed_multiplier_raw"]));
                }
                other => panic!("{n}: {other:?}"),
            }
        }
        // The Snowball: a projectile whose TargetBuff rides its splash.
        match spell("Snowball") {
            SpellDef { shape: SpellShape::Projectile { hit: Some(hit), .. }, .. } => {
                let p = &card("Snowball")["projectile"];
                let b = hit.buff.expect("the Snowball carries its slow");
                assert_eq!(b.time_ms, int(&p["buff_time_ms"]));
                assert_eq!(db.buffs[b.buff as usize].speed_pct, int(&p["target_buff"]["speed_multiplier_raw"]));
                assert!(hit.knockback.is_some(), "the Snowball pushes too");
            }
            other => panic!("Snowball: {other:?}"),
        }
        // A troop's own TargetBuff / BuffOnDamage lands on CardDef::attack_buff.
        for (n, key) in [("IceSpirits", "projectile"), ("IceWizard", "projectile"), ("ElectroWizard", "buff_on_damage")] {
            let c = card(n);
            let b = db.get(db.index(n).unwrap()).attack_buff.unwrap_or_else(|| panic!("{n} carries no attack buff"));
            let (raw, time) = if key == "projectile" {
                (&c["projectile"]["target_buff"], &c["projectile"]["buff_time_ms"])
            } else {
                (&c["buff_on_damage"]["buff"], &c["buff_on_damage"]["time_ms"])
            };
            assert_eq!(b.time_ms, int(time), "{n} BuffTime");
            assert_eq!(db.buffs[b.buff as usize].speed_pct, int(&raw["speed_multiplier_raw"]), "{n} SpeedMultiplier");
        }
        // No summon-only unit is a playable card name collision.
        assert!(db.cards.iter().filter(|c| c.summon_only).count() >= 1);
    }

    /// combat.REFLECT_ATTACK's data (`ReflectDef`), on synthetic rows so it holds whatever
    /// cards.json carries: a whole block loads with its radius in SUBTILES and its buff interned;
    /// a row without the block has none; a block missing its damage or its radius, or a buff
    /// without its duration, refuses the card rather than running it as a weaker one.
    #[test]
    fn a_reflected_attack_block_loads_whole_or_refuses_the_card() {
        let row = |name: &str, block: &str| {
            format!(
                r#"{{"name":"{name}","kind":"troop","rarity":"Common","hitpoints":1000,"hit_speed_ms":1800,
                "range_milli":1200,"collision_radius_milli":750{block}}}"#
            )
        };
        let freeze = r#"{"name":"ZapFreeze","speed_multiplier_raw":-100,"hit_speed_multiplier_raw":-100,"spawn_speed_multiplier_raw":-100}"#;
        let text = format!(
            r#"{{"cards":[{},{},{},{},{}]}}"#,
            row("Whole", &format!(r#","reflected_attack":{{"damage":75,"crown_tower_damage":38,"radius_milli":2000,"buff":{freeze},"buff_duration_ms":500}}"#)),
            row("Plain", ""),
            row("NoDamage", r#","reflected_attack":{"radius_milli":2000}"#),
            row("NoRadius", r#","reflected_attack":{"damage":75}"#),
            row("NoDuration", &format!(r#","reflected_attack":{{"damage":75,"radius_milli":2000,"buff":{freeze}}}"#)),
        );
        let db = CardDb::from_json_str(&text, CardSource::DerivedJson).unwrap();
        let r = db.get(db.index("Whole").expect("the whole block loads")).reflect.expect("and carries its reflect");
        assert_eq!((r.damage, r.crown_tower_damage, r.radius), (75, Some(38), milli(2000)));
        let b = r.buff.expect("the buff rides the reflect");
        assert_eq!(b.time_ms, 500);
        assert_eq!(db.buffs[b.buff as usize].speed_pct, -100);
        assert_eq!(db.get(db.index("Plain").expect("a plain row loads")).reflect, None);
        for (n, why) in [("NoDamage", "no ReflectedAttackDamage"), ("NoRadius", "no ReflectedAttackRadius"), ("NoDuration", "without BuffTime")] {
            assert!(db.index(n).is_none(), "{n} loaded");
            let e = &db.rejected.iter().find(|(name, _)| name == n).unwrap_or_else(|| panic!("{n} was not rejected")).1;
            assert!(e.contains(why), "{n}: {e}");
        }
    }
}
