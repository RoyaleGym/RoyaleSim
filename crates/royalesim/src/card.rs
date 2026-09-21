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
//!     floor). UNVERIFIED -- calibration combat.DAMAGE_ARITHMETIC is a guess; the
//!     Hog ladder is exact under every rounding mode, so it cannot decide.
//!
//! LEVEL NUMBERING
//!     `level` arguments are the unified "king-level" scale (Common 1..13).
//!     A Rare at unified level 11 is rarity-local level 11 - RelativeLevel(2) = 9.
//!
//! FALLBACK
//!     `CardDb::fallback()` is a tiny hardcoded set for when cards.json does not
//!     exist. Its numbers are FALLBACK values copied from the 2018 CSV data and
//!     must never be mistaken for the live game. `CardDb::source` says which you
//!     got, so no caller can end up on the fallback silently.
#![allow(unexpected_cfgs)]

use crate::fixed::{milli, tiles, Vec2};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::OnceLock;

const RARITIES_CSV: &str = include_str!("../../../data/raw/retroroyale-2018/csv_logic/rarities.csv");

/// Multipliers in rarities.csv are percentages.
const PERCENT: i64 = 100;

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
    /// IgnoreBuildings (area effects): troops only.
    pub ignore_buildings: bool,
    /// NoEffectToCrownTowers (area effects).
    pub no_effect_to_crown_towers: bool,
    pub knockback: Option<KnockbackDef>,
    /// A STUN-class buff's duration in ms (0 = no stun). See `SpellShape::AreaEffect`.
    pub stun_ms: i32,
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
    /// SpellAsDeploy airborne projectile that releases a rolling projectile (The Log).
    /// All distances SUBTILES; speeds raw.
    Rolling { airborne_speed: i32, airborne_min_distance: i32, speed: i32, range: i32, half_width: i32, half_depth: i32, hit: SpellHit },
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SpellDef {
    pub shape: SpellShape,
    pub placement: SpellPlacement,
}

/// One card, in engine units. Distances are SUBTILES; times are ms.
#[derive(Clone, Debug)]
pub struct CardDef {
    pub name: String,
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
    /// Buildings expire after this long. Modelled as removal at expiry; the real
    /// game's linear hp decay over the lifetime is NOT modelled.
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
    /// THE SPEED COLUMN IS NOT THE SPEED FOR THESE CARDS. The offline oracle's
    /// per-tick displacement is the FASTER figure -- Giant 52 and Golem 54 while
    /// both ship Speed 45 -- so the engine reads Speed through
    /// calibration.json movement.STOMP_SPEED_RULE, never raw. `move_speed()` is
    /// the only correct way to turn these three numbers into a speed.
    pub stop_movement_after_ms: i32,
    pub wait_ms: i32,
    // ^ DECLARED LAST ON PURPOSE. state.rs `migrate_v3` rebuilds the FORMAT-3 card
    // fingerprint by stripping the fields added after format 3 off the END of this
    // struct's Debug text, so a new field anywhere else silently retires the
    // format-3 fixture (tests/stacked_tie.rs, which cannot be regenerated).
}

impl CardDef {
    #[inline]
    pub fn is_flying(&self) -> bool {
        self.flying_height > 0
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
}

#[derive(Deserialize)]
struct RawCardsFile {
    cards: Vec<RawCard>,
    #[serde(default)]
    towers: Vec<RawCard>,
    /// Every characters/buildings row by name. Only units a simulable spell spawns
    /// are loaded (as `summon_only` cards).
    #[serde(default)]
    units: BTreeMap<String, serde_json::Value>,
}

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
    spawn_projectile: Option<Box<RawSpellProjectile>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct RawBuff {
    name: Option<String>,
    speed_multiplier_raw: Option<i32>,
    hit_speed_multiplier_raw: Option<i32>,
    spawn_speed_multiplier_raw: Option<i32>,
    damage_per_second: Option<i32>,
    heal_per_second: Option<i32>,
    damage_reduction: Option<i32>,
    damage_multiplier: Option<i32>,
    attract_percentage: Option<i32>,
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
    by_name: BTreeMap<String, u16>,
    pub source: CardSource,
    /// Cards present in the input the engine cannot simulate, with why.
    pub rejected: Vec<(String, String)>,
    /// True when the input lacked KingTower/PrincessTower and fallback tower
    /// definitions were appended.
    pub towers_from_fallback: bool,
    rarities: Vec<RarityRow>,
}

pub const KING_TOWER: &str = "KingTower";
pub const PRINCESS_TOWER: &str = "PrincessTower";

/// A CardDef with every stat zeroed, for spells (which have no unit of their own).
fn stat_less(name: String, rarity: String, elixir: i32) -> CardDef {
    CardDef {
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

/// The SPELL half of the loader. Accepts exactly the shapes `SpellShape` implements
/// and REJECTS every other spell with the reason, so a card whose mechanic is not
/// simulated can never run as a different, simpler card.
///
/// SIMULATED (2018 data): Fireball, Arrows, Rocket, Goblin Barrel (Projectile);
/// Zap, Freeze (AreaEffect); The Log (Rolling). REJECTED, with why: Rage, Poison,
/// Heal, Tornado (pulsing area effects), Lightning (an area effect firing targeted
/// projectiles), Graveyard (spawning area effect), Clone (own-troop buff), Mirror
/// (no mechanic in the data). Freeze and Rocket are not in the thin slice; they run
/// because their data has exactly the implemented shape, and Freeze's 4 s life is
/// where calibration spells.ONE_SHOT_AREA_EFFECT_APPLICATION is LOW confidence.
///
/// Returns the card, and the name of the unit it spawns (resolved to an index by
/// `CardDb::from_json_str`, which loads that unit).
fn convert_spell(raw: RawCard) -> Result<(CardDef, Option<String>), String> {
    let spell = raw.spell.unwrap_or_default();
    let mut def = stat_less(raw.name.clone(), raw.rarity.clone().ok_or("missing rarity")?, raw.elixir.unwrap_or(0));
    def.level_table = level_table_of(raw.level_scaling)?;
    if spell.duration_seconds.is_some() {
        return Err("spell DurationSeconds is not simulated".into());
    }
    let proj: Option<RawSpellProjectile> = match raw.projectile {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => Some(serde_json::from_value(v).map_err(|e| format!("projectile: {e}"))?),
    };
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
    let mut spawn_name = None;
    let shape = if let Some(aeo) = spell.area_effect_object {
        let what = aeo.name.clone().unwrap_or_default();
        if proj.is_some() || spell.first_projectile.is_some() {
            return Err(format!("spell with both an area effect ({what}) and a projectile is not simulated"));
        }
        if aeo.hit_speed_ms.is_some() {
            return Err(format!("pulsing area effect {what} (HitSpeed set) is not simulated"));
        }
        if aeo.only_own_troops.unwrap_or(false) {
            return Err(format!("own-troop area effect {what} is not simulated"));
        }
        if aeo.maximum_targets.is_some() || aeo.projectile.as_ref().is_some_and(|p| !p.is_null()) || aeo.spawn_character.is_some() {
            return Err(format!("area effect {what} with targets / projectile / spawn is not simulated"));
        }
        let stun_ms = match aeo.buff {
            None => 0,
            Some(b) => {
                // STUN class: all three raw multipliers -100 (docs/spell-spec.md, STUN
                // CLASS). Anything else needs buff machinery that does not exist.
                let stun = b.speed_multiplier_raw == Some(-100)
                    && b.hit_speed_multiplier_raw == Some(-100)
                    && b.spawn_speed_multiplier_raw == Some(-100)
                    && b.damage_per_second.is_none()
                    && b.heal_per_second.is_none()
                    && b.damage_reduction.is_none()
                    && b.damage_multiplier.is_none()
                    && b.attract_percentage.is_none();
                if !stun {
                    return Err(format!("area effect {what}: buff {} is not a stun-class buff", b.name.unwrap_or_default()));
                }
                aeo.buff_time_ms.filter(|t| *t > 0).ok_or_else(|| format!("area effect {what}: stun buff without BuffTime"))?
            }
        };
        let hit = SpellHit {
            damage: aeo.damage.unwrap_or(0),
            crown_pct: crown(aeo.crown_tower_damage_percent),
            radius: milli(aeo.radius_milli.ok_or_else(|| format!("area effect {what} without radius"))?),
            hits_air: aeo.hits_air.unwrap_or(false),
            hits_ground: aeo.hits_ground.unwrap_or(false),
            only_enemies: aeo.only_enemies.unwrap_or(false),
            ignore_buildings: aeo.ignore_buildings.unwrap_or(false),
            no_effect_to_crown_towers: aeo.no_effect_to_crown_towers.unwrap_or(false),
            knockback: knockback(aeo.pushback_milli, None),
            stun_ms,
        };
        let _ = aeo.life_duration_ms; // one-shot: applied once whatever its life (see SpellShape)
        SpellDef { shape: SpellShape::AreaEffect { hit }, placement: placement_for(false) }
    } else {
        // DAMAGE CARRIER (docs/spell-spec.md): CustomFirstProjectile when present (the
        // 2018 Arrows pattern -- `projectile` is then the damage-less ArrowsSpellDeco and
        // MultipleProjectiles is visual); otherwise Projectile itself.
        let carrier = match (spell.first_projectile, proj) {
            (Some(f), _) => f,
            (None, Some(p)) => p,
            (None, None) => return Err("spell with no projectile and no area effect: no mechanic in the data".into()),
        };
        let what = carrier.name.clone().unwrap_or_default();
        if carrier.maximum_targets.is_some() || carrier.spawn_area_effect_object.is_some() || carrier.target_buff.as_ref().is_some_and(|b| !b.is_null()) {
            return Err(format!("projectile {what} with target cap / area effect / target buff is not simulated"));
        }
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
            if roll.maximum_targets.is_some() || roll.spawn_character.is_some() || roll.spawn_projectile.is_some() || roll.target_buff.as_ref().is_some_and(|b| !b.is_null()) {
                return Err(format!("rolling {rname} with targets / spawns / buffs is not simulated"));
            }
            let hit = SpellHit {
                damage: roll.damage.ok_or_else(|| format!("rolling {rname} without damage"))?,
                crown_pct: crown(roll.crown_tower_damage_percent),
                radius: 0,
                hits_air: roll.aoe_to_air.unwrap_or(false),
                hits_ground: roll.aoe_to_ground.unwrap_or(false),
                only_enemies: roll.only_enemies.unwrap_or(false),
                ignore_buildings: false,
                no_effect_to_crown_towers: false,
                knockback: knockback(roll.pushback_milli, roll.pushback_all),
                stun_ms: 0,
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
                },
                placement: placement_for(false),
            }
        } else {
            let hit = match carrier.damage {
                None => None,
                Some(d) => Some(SpellHit {
                    damage: d,
                    crown_pct: crown(carrier.crown_tower_damage_percent),
                    radius: milli(carrier.radius_milli.filter(|r| *r > 0).ok_or_else(|| format!("projectile {what} deals damage but has no radius"))?),
                    hits_air: carrier.aoe_to_air.unwrap_or(false),
                    hits_ground: carrier.aoe_to_ground.unwrap_or(false),
                    only_enemies: carrier.only_enemies.unwrap_or(false),
                    ignore_buildings: false,
                    no_effect_to_crown_towers: false,
                    knockback: knockback(carrier.pushback_milli, carrier.pushback_all),
                    stun_ms: 0,
                }),
            };
            let spawn = match carrier.spawn_character.clone() {
                None => None,
                Some(unit) => {
                    let count = carrier.spawn_character_count.filter(|c| *c > 0).ok_or_else(|| format!("{what} spawns {unit} with no count"))?;
                    spawn_name = Some(unit);
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
    Ok((def, spawn_name))
}

fn level_table_of(v: Option<serde_json::Value>) -> Result<Option<Vec<i32>>, String> {
    Ok(match v {
        Some(serde_json::Value::Object(o)) => match o.get("multiplier_percent_by_level") {
            Some(v) => Some(serde_json::from_value::<Vec<i32>>(v.clone()).map_err(|e| format!("level_scaling: {e}"))?),
            None => None,
        },
        _ => None,
    })
}

/// (card, display name, name of the unit it spawns).
type Converted = (CardDef, Option<String>, Option<String>);

fn convert(raw: RawCard) -> Result<Converted, String> {
    if raw.kind == CardKind::Spell {
        let display = raw.display_name.clone();
        #[cfg(clash_plant = "spells_rejected")]
        {
            // PLANT (regression): a loader that refuses every spell.
            let _ = display;
            return Err("spells are not simulated yet".into());
        }
        #[allow(unreachable_code)]
        return convert_spell(raw).map(|(c, spawn)| (c, display, spawn));
    }
    let need = |v: Option<i32>, what: &str| v.ok_or_else(|| format!("missing {what}"));
    let mut damage = raw.damage;
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
            Some(ProjectileDef {
                speed: p.speed.ok_or("projectile without speed")?,
                radius: milli(p.radius_milli.unwrap_or(0)),
            })
        }
        Some(other) => return Err(format!("projectile given as {other} carries no speed; need an object")),
    };
    let level_table = level_table_of(raw.level_scaling)?;
    let kind = raw.kind;
    // Spells do not reach here: `convert_spell` above handles them
    // (plant: spells_rejected).
    let range = need(raw.range_milli, "range_milli")?;
    let no_deploy_size = match raw.no_deploy_size_tiles {
        Some([w, h]) if w > 0 && h > 0 => Some(Vec2::new(tiles(w), tiles(h))),
        Some(other) => return Err(format!("no_deploy_size_tiles {other:?} is not two positive tile counts")),
        None => None,
    };
    let display = raw.display_name.clone();
    Ok((CardDef {
        name: raw.name,
        kind,
        elixir: raw.elixir.unwrap_or(0),
        rarity: raw.rarity.ok_or("missing rarity")?,
        hitpoints: need(raw.hitpoints, "hitpoints")?,
        damage: damage.unwrap_or(0),
        hit_speed_ms: need(raw.hit_speed_ms, "hit_speed_ms")?,
        load_time_ms: raw.load_time_ms.unwrap_or(0),
        speed: raw.speed.unwrap_or(0),
        range: milli(range),
        // Towers ship no separate sight: they see exactly as far as they shoot.
        sight_range: milli(raw.sight_range_milli.unwrap_or(range)),
        collision_radius: milli(need(raw.collision_radius_milli, "collision_radius_milli")?),
        mass: raw.mass,
        deploy_time_ms: raw.deploy_time_ms.unwrap_or(0),
        attacks_air: raw.attacks_air.unwrap_or(false),
        attacks_ground: raw.attacks_ground.unwrap_or(true),
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
    }, display, None))
}

impl CardDb {
    /// Parse a cards.json document (schema at the bottom of this file).
    pub fn from_json_str(s: &str, source: CardSource) -> Result<CardDb, String> {
        let file: RawCardsFile = serde_json::from_str(s).map_err(|e| format!("cards.json: {e}"))?;
        let rarities = shipped_rarities().to_vec();
        let mut db = CardDb {
            cards: Vec::new(),
            by_name: BTreeMap::new(),
            source,
            rejected: Vec::new(),
            towers_from_fallback: false,
            rarities,
        };
        let mut spawns: Vec<(u16, String)> = Vec::new();
        for raw in file.cards.into_iter().chain(file.towers) {
            let name = raw.name.clone();
            match convert(raw) {
                Ok((c, display, spawn)) => {
                    if !db.rarities.iter().any(|r| r.name == c.rarity) {
                        db.rejected.push((name, format!("rarity {} not in rarities.csv", c.rarity)));
                        continue;
                    }
                    db.push(c, display)?;
                    if let Some(unit) = spawn {
                        spawns.push(((db.cards.len() - 1) as u16, unit));
                    }
                }
                Err(e) => db.rejected.push((name, e)),
            }
        }
        // SPAWNED UNITS. A spell that releases units needs those units' stats; they are
        // `units` records, not cards (no Goblin card exists: the card is Goblins). Each is
        // loaded once, as a summon_only card, after every real card so no card index
        // moves. A spell whose unit cannot be loaded is REJECTED (removed again), never
        // left pointing at nothing.
        let mut unit_idx: BTreeMap<String, Result<u16, String>> = BTreeMap::new();
        let mut unloadable: Vec<(u16, String)> = Vec::new();
        for (spell_idx, unit) in &spawns {
            let got = unit_idx
                .entry(unit.clone())
                .or_insert_with(|| {
                    if db.by_name.contains_key(unit) {
                        return Err(format!("spawned unit {unit} collides with a card name"));
                    }
                    let mut v = file.units.get(unit).cloned().ok_or_else(|| format!("spawned unit {unit} has no units record"))?;
                    let kind = match v.get("source_table").and_then(|t| t.as_str()) {
                        Some("buildings") => "building",
                        _ => "troop",
                    };
                    let obj = v.as_object_mut().ok_or_else(|| format!("units.{unit} is not an object"))?;
                    obj.insert("kind".into(), serde_json::Value::String(kind.into()));
                    obj.entry("count").or_insert(serde_json::Value::from(1));
                    let raw: RawCard = serde_json::from_value(v).map_err(|e| format!("units.{unit}: {e}"))?;
                    let (mut c, _, nested) = convert(raw).map_err(|e| format!("units.{unit}: {e}"))?;
                    if nested.is_some() {
                        return Err(format!("units.{unit} itself spawns units; not simulated"));
                    }
                    if !db.rarities.iter().any(|r| r.name == c.rarity) {
                        return Err(format!("units.{unit}: rarity {} not in rarities.csv", c.rarity));
                    }
                    c.summon_only = true;
                    db.push(c, None)?;
                    Ok((db.cards.len() - 1) as u16)
                })
                .clone();
            match got {
                Ok(u) => {
                    if let Some(SpellDef { shape: SpellShape::Projectile { spawn: Some(sp), .. }, .. }) = db.cards[*spell_idx as usize].spell.as_mut() {
                        sp.unit = u;
                    }
                }
                Err(e) => unloadable.push((*spell_idx, e)),
            }
        }
        for (spell_idx, why) in unloadable {
            // Keep indices stable: the card stays in `cards` but is unregistered and
            // listed as rejected, so no name resolves to it.
            let name = db.cards[spell_idx as usize].name.clone();
            db.by_name.retain(|_, i| *i != spell_idx);
            db.rejected.push((name, why));
        }
        if db.index(KING_TOWER).is_none() || db.index(PRINCESS_TOWER).is_none() {
            let fb: RawCardsFile = serde_json::from_str(FALLBACK_TOWERS_JSON).expect("fallback towers parse");
            for raw in fb.cards {
                if db.index(&raw.name).is_none() {
                    let (c, d, _) = convert(raw)?;
                    db.push(c, d)?;
                }
            }
            db.towers_from_fallback = true;
        }
        Ok(db)
    }

    /// The unified level a unit released by spell `spell_idx` at unified `level` has.
    ///
    /// SpawnCharacterLevelIndex, READING CHOSEN: an offset added to the SPELL's
    /// rarity-local level, giving the unit's rarity-local level, converted back to
    /// unified. The other reading ("the same unified level as the spell") gives the
    /// identical answer for every shipped row, because every shipped index equals its
    /// rarity's RelativeLevel (Goblin Barrel: Epic, 5) -- docs/spell-spec.md. With no
    /// index, the unit takes the spell's unified level.
    pub fn spawn_level(&self, spell_idx: u16, level: i32) -> Result<i32, String> {
        let c = self.get(spell_idx);
        let Some(SpellDef { shape: SpellShape::Projectile { spawn: Some(sp), .. }, .. }) = &c.spell else {
            return Err(format!("{} releases no units", c.name));
        };
        let unit = self.get(sp.unit);
        #[cfg(clash_plant = "spawn_level_local_one")]
        {
            // PLANT: the released unit at its rarity's local level 1.
            let r = self.rarity(&self.get(sp.unit).rarity).map(|r| r.relative_level).unwrap_or(0);
            let _ = level;
            return Ok(r + 1);
        }
        #[allow(unreachable_code)]
        let out = match sp.level_index {
            None => level,
            Some(ix) => {
                let own = self.rarity(&c.rarity).ok_or_else(|| format!("{}: unknown rarity", c.name))?.relative_level;
                let theirs = self.rarity(&unit.rarity).ok_or_else(|| format!("{}: unknown rarity", unit.name))?.relative_level;
                level - own + ix + theirs
            }
        };
        self.level_multiplier(sp.unit, out)?;
        Ok(out)
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
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json");
        let s = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
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
        let local = level - r.relative_level;
        if local < 1 || local > r.level_count {
            return Err(format!(
                "{} ({}) has no level {level} (local {local}, valid 1..={})",
                c.name, c.rarity, r.level_count
            ));
        }
        if local == 1 {
            return Ok(PERCENT as i32);
        }
        let short = || format!("{}: multiplier table too short for local level {local}", c.name);
        match c.level_table.as_deref() {
            Some(by_level) => by_level.get((local - 1) as usize).copied().ok_or_else(short),
            None => r.multipliers.get((local - 2) as usize).copied().ok_or_else(short),
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
// Unknown fields are ignored.

#[cfg(test)]
mod tests {
    use super::*;

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
        // Arrows: the damage carrier is the CustomFirstProjectile, not the deco.
        let ar = card("Arrows");
        match spell("Arrows") {
            SpellDef { shape: SpellShape::Projectile { speed, hit: Some(h), waves: 1, spawn: None, .. }, placement: SpellPlacement::Anywhere } => {
                assert_eq!(speed, int(&ar["spell"]["first_projectile"]["speed"]));
                assert_eq!(h.damage, int(&ar["spell"]["first_projectile"]["damage"]));
                assert_eq!(h.radius, milli(int(&ar["spell"]["first_projectile"]["radius_milli"])));
                assert_eq!(h.knockback, None);
            }
            other => panic!("Arrows: {other:?}"),
        }
        let zp = card("Zap");
        match spell("Zap") {
            SpellDef { shape: SpellShape::AreaEffect { hit }, placement: SpellPlacement::Anywhere } => {
                let aeo = &zp["spell"]["area_effect_object"];
                assert_eq!(hit.damage, int(&aeo["damage"]));
                assert_eq!(hit.stun_ms, int(&aeo["buff_time_ms"]));
                assert_eq!(hit.radius, milli(int(&aeo["radius_milli"])));
                assert_eq!(hit.crown_pct, int(&aeo["crown_tower_damage_percent"]));
            }
            other => panic!("Zap: {other:?}"),
        }
        let lg = card("Log");
        match spell("Log") {
            SpellDef { shape: SpellShape::Rolling { airborne_speed, airborne_min_distance, speed, range, half_width, half_depth, hit }, placement: SpellPlacement::TroopTerritory { on_buildings: true } } => {
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
        // What is NOT simulated is refused out loud.
        for n in ["Rage", "Poison", "Lightning", "Graveyard", "Tornado", "Clone", "Heal", "Mirror"] {
            assert!(db.index(n).is_none(), "{n} must not be simulable");
            assert!(db.rejected.iter().any(|(r, _)| r == n), "{n} not listed as rejected");
        }
        // No summon-only unit is a playable card name collision.
        assert!(db.cards.iter().filter(|c| c.summon_only).count() >= 1);
    }
}
