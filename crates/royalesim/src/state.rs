//! BattleState and the tick loop.
//!
//! THE LOOP IS DRIVEN BY lib.rs::TICK_PHASES
//!     `tick()` iterates that array and dispatches on each phase. Reordering the
//!     array reorders the engine; nothing else encodes the order.
//!
//! WHAT RUNS WHERE
//!     Upkeep     elixir accrual; deploy timers mature
//!     Status     king activation timer; building lifetimes (stun/slow timers only
//!                under status.BUFF_EXPIRY_TICK_ALIGNMENT = one_tick_short)
//!     Spawn      pending spawns (deploys since last tick) materialise; accepted
//!                spell casts become spell objects (spell.rs); then every periodic
//!                SPAWNER past its deploy time ticks its timer and queues the units
//!                that are due (spawner_pass; they materialise NEXT tick)
//!     Target     every entity decides its target from start-of-phase state
//!     Path       every unit proposes a movement delta from start-of-phase state
//!     Move       deltas applied, then collision separation (buffered); then every
//!                CHARGE card's run-up gains from its own walk (charge_pass)
//!     Attack     windups advance; hits go to the damage buffer / projectile list
//!                (a charged unit's hit is DamageSpecial and consumes the charge)
//!     Projectile projectiles advance; arrivals go to the damage buffer. Spells
//!                advance: impacts write damage / knockback / stun buffers, landing
//!                Goblin Barrels queue their units
//!     Resolve    the damage buffer is applied in one pass; deaths are queued; then
//!                stun timers tick, the stun buffer merges (max) and the knockback
//!                buffer sums and moves the SURVIVORS
//!     Reap       queued deaths: death damage is BUFFERED (lands next Resolve),
//!                death spawns are QUEUED (materialise next Spawn), crown towers
//!                are recorded, slots are freed
//!     Judge      crowns, three-crown win, regulation/overtime/draw
//!
//! CALIBRATION
//!     Every physics constant is read from data/calibration.json into `Calib`.
//!     The one match constant it lacks (MANA_SPEED_UP_WHEN_REMAINING_SECONDS) is
//!     read from the shipped globals.csv.
#![allow(unexpected_cfgs)]

use crate::arena::{Arena, FootprintModel, Lane, Rect, Shape, Territory, TerritoryModel};
use crate::card::{CardDb, CardDef, CardKind, ChargeDef, SpawnerDef, SpellPlacement, KING_TOWER, PRINCESS_TOWER};
use crate::collide::{self, CollideScratch};
use crate::combat::{self, CrownRounding, DamageBuffer, Hit, Projectile};
use crate::spell::{self, EffectBuffer, Spell};
use crate::entity::{AttackPhase, EntityKind, Entities, HideState, SpatialHash, SpawnInit};
use crate::fixed::{isqrt, Vec2, SUBTILE};
use crate::path::{self, FrameWorld, NavRequest, Obstacle, UnitBlocker};
use crate::path2026;
use crate::move16402;
use crate::path16402;
use crate::target::{self, TargetCtx, TargetDecision, TowerSightReading, TowerTable};
use crate::{EntityId, PathModel, Phase, PushModel, Rng, Team, TICK_PHASES};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, OnceLock};

const CALIBRATION_JSON: &str = include_str!("../../../data/calibration.json");
const GLOBALS_CSV: &str = include_str!("../../../data/raw/retroroyale-2018/csv_logic/globals.csv");

// ---------------------------------------------------------------------------
// calibration

/// Every constant the engine reads from calibration.json. Distances are
/// converted to SUBTILES at load; times stay in ms.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Calib {
    pub tick_ms: i32,
    pub speed_to_subtiles_per_tick: i32,
    pub add_character_range_to_radius: bool,
    pub extra_sight_range_to_crown_towers: i32,
    pub extra_sight_range_to_building: i32,
    pub range_extension_to_keep_target: i32,
    pub cancel_hit_from_long_distance_range: i32,
    pub preserve_target_if_hit_started: bool,
    pub xpos_based_tower_targeting: bool,
    pub melee_range_limit: i32,
    #[serde(with = "push_model_serde")]
    pub push_model: PushModel,
    pub separation_iterations: i32,
    pub footprint_model: FootprintModel,
    #[serde(with = "path_model_serde")]
    pub path_model: PathModel,
    /// pathfinding.REPATH_INTERVAL_TICKS. `None` = no periodic replan, which is
    /// what the offline oracle measured (150 structural recomputes over 31 859
    /// path-ticks with no common period); the pre-2026 models then replan only when
    /// their route empties or their goal moves. A positive value re-arms the old
    /// folklore cadence for those models; the 2026 model ignores it entirely and
    /// uses pathfinding.REPLAN_TRIGGERS.
    pub repath_interval_ticks: Option<i32>,

    // --- the 2026 pathfinder and locomotion law. Every key here is measured
    // against the offline trace corpus; see path2026.rs and calibration.json.
    /// pathfinding.PATHFINDING_COSTS.value.*, per half-tile cell ENTERED.
    pub path_cost_default: i32,
    pub path_cost_road: i32,
    /// PATHFINDING_BLOCKED_COST. READ SO THE REGISTRY CANNOT SILENTLY DISAGREE, and
    /// deliberately no longer wired to a branch: it used to price bit-16 terrain
    /// under OCCLUDED_CELL_TREATMENT = cost_50, which made the deploy-restricted
    /// arena-edge strips walkable as a side effect of a statement about BUILDINGS.
    /// Bit 16 is now unconditionally impassable outside a box and takes
    /// `path_cost_building` inside one (path2026.rs `CostField`). It is the same 50
    /// either way, so nothing about the shipped numbers turns on this.
    ///
    /// WIRED AGAIN under `PATH_SEARCH = client16402`: the 16.402 cell cost is
    /// BLOCKED for a water cell a walker asks about, and those cells are pushed on
    /// the heap (path16402.rs `Terrain`).
    pub path_cost_blocked: i32,
    pub path_cost_building: i32,
    pub path_cost_heuristic: i32,
    /// pathfinding.DIAGONAL_COST_RATIO: a diagonal step pays `cost * num / den`.
    pub diag_num: i32,
    pub diag_den: i32,
    /// pathfinding.WAYPOINT_ARRIVE_RADIUS, converted to SUBTILES.
    pub waypoint_arrive_radius: i32,
    /// pathfinding.WAYPOINT_ARRIVE_RULE.
    pub waypoint_arrive_rule: WaypointArriveRule,
    /// pathfinding.OCCLUDED_CELL_TREATMENT -- BUILDING OCCLUSION BOXES ONLY. Never
    /// water (pathfinding.WATER_RULE_GROUND pins that to impassable on its own) and,
    /// since the 16.402 cross-check, never bit-16 terrain either: `cost_50` is a
    /// statement about buildings, and it used to make the arena-edge strips walkable
    /// as a side effect (path2026.rs `CostField::terrain`).
    pub occluded_cells: OccludedCells,
    /// pathfinding.HEURISTIC_FORM -- MEASURED chebyshev_over_goal_set on the live
    /// 16.402 corpus (192/292 exact node sequences against 78/292 for the octile
    /// hypothesis this used to pin). Read by path2026.rs `heuristic_weights`.
    pub heuristic_form: HeuristicForm,
    /// pathfinding.TIE_BREAK -- a flagged placeholder, not a finding. READ BY
    /// path2026.rs `neighbours()`: it selects the neighbour order the A* expands in.
    /// Only reaches the search under `PATH_SEARCH = trace_fitted_astar`.
    pub tie_break: TieBreak,
    /// pathfinding.PATH_SEARCH -- WHICH SEARCH `path2026::plan_cells` runs.
    /// `client16402` is the search measured on client 16.402 (path16402.rs: single
    /// goal cell, x10/x14 step costs, f-only binary heap, N S W E NW SW SE NE, water
    /// priced 50 and pushed) -- 435/438 live node sequences. `trace_fitted_astar` is
    /// the earlier model fitted to traces (goal set, Chebyshev h, the
    /// TIE_BREAK / HEURISTIC_FORM / OCCLUDED_CELL_TREATMENT knobs), kept runnable as
    /// the refuted arm: 168/345.
    pub path_search: PathSearch,
    pub mana_regen_ms_1x: i32,
    pub mana_regen_ms_2x: i32,
    pub start_mana: i32,
    pub max_mana: i32,
    pub king_activate_time_ms: i32,
    pub battle_start_cooldown_ms: i32,
    pub regular_time_s: i32,
    pub overtime_s: i32,
    pub three_crown_instant_win: bool,
    /// match.OVERTIME_TIEBREAK -- how a match still level on crowns when overtime
    /// runs out is decided (state.rs `overtime_tiebreak`).
    pub overtime_tiebreak: OvertimeTiebreak,
    /// globals.csv MANA_SPEED_UP_WHEN_REMAINING_SECONDS (not in calibration.json).
    pub mana_speed_up_remaining_s: i32,
    /// calibration.json arena.TERRITORY_MODEL. There is no troop pocket depth past
    /// the far bank: the shipped NoDeploySize rects are the mechanic (arena.rs TROOP
    /// TERRITORY). Lives in Calib so a snapshot carries it -- a restored battle must
    /// not change its deploy rule.
    pub territory_model: TerritoryModel,

    // --- spells (docs/spell-spec.md). Each is one calibration.json key; a value
    // with no implementation is refused in from_json.
    /// time.PROJECTILE_SPEED_TO_SUBTILES_PER_TICK (every projectile, troop or spell).
    pub projectile_speed_to_subtiles_per_tick: i32,
    /// combat.CROWN_TOWER_DAMAGE_ROUNDING.
    pub crown_rounding: CrownRounding,
    /// spells.AOE_HIT_TEST.
    pub aoe_hit_test: AoeHitTest,
    /// spells.SPELL_AS_DEPLOY_LAUNCH_MODEL.
    pub spell_as_deploy_launch: LaunchModel,
    /// spells.ROLLING_HIT_SHAPE.
    pub rolling_hit_shape: RollHitShape,
    /// spells.SPAWNING_SPELL_WATER_RULE.
    pub spawning_spell_water: SpawnWaterRule,
    /// knockback.DURATION_MS.
    pub knock_duration_ms: i32,
    /// knockback.ZERO_VECTOR_DIRECTION.
    pub knock_zero_vector: KnockZeroVector,
    /// knockback.ATTACK_RESET.
    pub knock_attack_reset: KnockAttackReset,
    /// knockback.AFFECTS_DEPLOYING_UNITS.
    pub knock_affects_deploying: bool,
    /// knockback.DIRECTION_ROLLING.
    pub knock_direction_rolling: RollDirection,
    /// status.STUN_ATTACK_TIMER_MODEL.
    pub stun_attack_timer: StunTimerModel,
    /// status.STUN_RETARGET_ON_RESUME.
    pub stun_retarget_on_resume: bool,
    /// status.RESUME_RETARGET_WINDUP.
    pub resume_retarget_windup: ResumeWindup,
    /// status.BUFF_EXPIRY_TICK_ALIGNMENT.
    pub buff_expiry: BuffExpiry,
    /// status.SAME_BUFF_REAPPLY.
    pub same_buff_reapply: BuffReapply,
    /// status.STUN_PAUSES_DEPLOY_TIMER.
    pub stun_pauses_deploy: bool,
    /// status.STUN_PAUSES_BUILDING_LIFETIME.
    pub stun_pauses_building_lifetime: bool,
    /// status.STUN_PAUSES_KING_ACTIVATION.
    pub stun_pauses_king_activation: bool,

    // --- hide (Tesla). Each is one calibration.json hide.* key;
    // a candidate with no implementation is refused in from_json.
    /// hide.STARTS_HIDDEN: the state a hiding building takes the moment its deploy
    /// timer ends (Hidden, or Up with a fresh hide countdown).
    pub hide_starts_hidden: bool,
    /// hide.RISE_TRIGGER.
    pub hide_rise_trigger: RiseTrigger,
    /// hide.TARGETABLE_WHILE_RISING.
    pub hide_targetable_while_rising: bool,
    /// hide.HIDDEN_IMMUNE_TO_DAMAGE (combat.rs `resolve`).
    pub hide_hidden_immune: bool,
    /// hide.HIDE_DELAY_MEANING.
    pub hide_delay_meaning: HideDelayMeaning,

    // --- spawner (periodic spawners and death spawn). Each is one
    // calibration.json spawner.* key; a candidate with no implementation is refused
    // in from_json.
    /// spawner.FIRST_WAVE: the timer a spawner with a blank SpawnStartTime starts with.
    pub spawner_first_wave: FirstWave,
    /// spawner.START_TIME_ORIGIN: whether SpawnStartTime counts from activation
    /// (deploy time over) or from placement.
    pub spawner_start_time_origin: StartTimeOrigin,
    /// spawner.PAUSE_ANCHOR: where SpawnPauseTime is measured from in a timed wave.
    pub spawner_pause_anchor: PauseAnchor,
    /// spawner.SPAWN_POINT: where a spawner's units appear.
    pub spawner_spawn_point: SpawnPoint,
    /// spawner.STUN_PAUSES_SPAWNER.
    pub spawner_stun_pauses: bool,
    /// spawner.DEATH_SPAWN_RADIUS_DEFAULT: the radius when DeathSpawnRadius is blank.
    pub death_spawn_radius_default: DeathSpawnRadius,
    /// spawner.DEATH_SPAWN_DEPLOY_TIME_DEFAULT: the deploy time when the block is blank.
    pub death_spawn_deploy_default: DeathSpawnDeploy,

    // --- charge (Prince, DarkPrince, BattleRam). Each is one
    // calibration.json charge.* key; a candidate with no implementation is refused
    // in from_json. Read by `charge_pass`, `effective_speed`, `phase_attack`,
    // `apply_effects`, `phase_target` and combat.rs `fire`.
    /// charge.CHARGE_RANGE_UNIT: what ChargeRange's raw number is.
    pub charge_range_unit: ChargeRangeUnit,
    /// charge.ACCUMULATOR: what the run-up counts.
    pub charge_accumulator: ChargeAccumulator,
    /// charge.MULTIPLIER_MEANING: what ChargeSpeedMultiplier multiplies.
    pub charge_multiplier_meaning: ChargeMultiplier,
    /// charge.PROGRESS_ON_STOP: what a tick without a walk does to partial progress.
    pub charge_progress_on_stop: ChargeStopRule,
    /// charge.RESET_ON_ATTACK: the landed hit consumes the charge.
    pub charge_reset_on_attack: bool,
    /// charge.RESET_ON_STUN: a stun landing clears charge and progress.
    pub charge_reset_on_stun: bool,
    /// charge.RESET_ON_KNOCKBACK: a knockback LANDING (one that moves the unit) clears both.
    pub charge_reset_on_knockback: bool,
    /// charge.RESET_ON_RETARGET: switching between two live targets clears both.
    pub charge_reset_on_retarget: bool,
    /// charge.SPECIAL_LEVEL_SCALING: how DamageSpecial scales with level.
    pub charge_special_level_scaling: ChargeLevelScaling,
}

/// A calibration enum: the registry string names each variant, and an unknown string
/// is refused at load rather than run as some other candidate.
macro_rules! calib_enum {
    ($(#[$m:meta])* $name:ident { $($(#[$vm:meta])* $variant:ident = $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
        pub enum $name {
            $($(#[$vm])* $variant),+
        }
        impl $name {
            pub fn from_calibration_name(s: &str) -> Option<Self> {
                match s {
                    $($s => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }
    };
}

calib_enum!(
    /// spells.AOE_HIT_TEST.
    AoeHitTest { EdgeInclusive = "edge_inclusive", CentreInRadius = "centre_in_radius" }
);
calib_enum!(
    /// spells.SPELL_AS_DEPLOY_LAUNCH_MODEL.
    LaunchModel {
        AirborneFromBehind = "airborne_from_behind_lands_on_tap",
        AirborneFromTapForward = "airborne_from_tap_forward",
        InstantRollAtTap = "instant_roll_at_tap",
    }
);
calib_enum!(
    /// spells.ROLLING_HIT_SHAPE.
    RollHitShape { RectVsCircleEdge = "rect_vs_circle_edge", RectContainsCentre = "rect_contains_centre" }
);
calib_enum!(
    /// spells.SPAWNING_SPELL_WATER_RULE.
    SpawnWaterRule { RefuseTouchingWater = "refuse_touching_water", Anywhere = "anywhere" }
);
calib_enum!(
    /// match.OVERTIME_TIEBREAK. The rule as the community documents it (not yet measured): the side whose
    /// weakest standing crown tower is weaker LOSES; an exact tie stays a Draw.
    OvertimeTiebreak {
        /// Compare each side's minimum alive crown-tower hp in absolute points.
        LowestTowerHpAbsolute = "lowest_tower_hp_absolute",
        /// Compare each side's minimum alive crown-tower hp as a fraction of its max,
        /// in exact integer arithmetic (cross-multiplied, no rounding).
        LowestTowerHpFraction = "lowest_tower_hp_fraction",
        /// The earlier engine: a level match past overtime is a Draw.
        NoneDraw = "none_draw",
    }
);
calib_enum!(
    /// knockback.ZERO_VECTOR_DIRECTION.
    KnockZeroVector { CasterForward = "caster_forward", NoPush = "none" }
);
calib_enum!(
    /// knockback.ATTACK_RESET.
    KnockAttackReset { ResetWindupKeepTarget = "reset_windup_keep_target", ResetWindupClearTarget = "reset_windup_clear_target" }
);
calib_enum!(
    /// knockback.DIRECTION_ROLLING.
    RollDirection { RadialFromCentre = "radial_from_projectile_centre", TravelDirection = "travel_direction" }
);
calib_enum!(
    /// status.STUN_ATTACK_TIMER_MODEL.
    StunTimerModel { Pause = "pause", Reset = "reset" }
);
calib_enum!(
    /// status.RESUME_RETARGET_WINDUP.
    ResumeWindup { Cancel = "cancel", Carry = "carry" }
);
calib_enum!(
    /// status.BUFF_EXPIRY_TICK_ALIGNMENT.
    BuffExpiry { CeilFromNextTick = "ceil_from_next_tick", OneTickShort = "one_tick_short" }
);
calib_enum!(
    /// status.SAME_BUFF_REAPPLY.
    BuffReapply { RefreshMax = "refresh_max", Replace = "replace" }
);
calib_enum!(
    /// hide.RISE_TRIGGER -- what wakes a hidden building (target.rs
    /// `enemy_in_wake_range`): a targetable enemy within its SIGHT (the community
    /// reading; Tesla's sight equals its range, so the two agree on it today) or
    /// within its ATTACK range.
    RiseTrigger { EnemyInSightRange = "enemy_in_sight_range", EnemyInAttackRange = "enemy_in_attack_range" }
);
calib_enum!(
    /// hide.HIDE_DELAY_MEANING -- what HideTimeMs counts down from: consecutive
    /// Target phases with no live target (the countdown resets while it has one), or
    /// the building's last shot (state.rs phase_attack resets it on fire; a kept but
    /// out-of-range target then does not keep it up).
    HideDelayMeaning { IdleTimeWithoutTarget = "idle_time_without_target", TimeSinceLastShot = "time_since_last_shot" }
);
calib_enum!(
    /// spawner.FIRST_WAVE -- a spawner whose SpawnStartTime is blank fires its first
    /// wave at activation (timer 0: queued in the activation tick's Spawn phase) or
    /// one full SpawnPauseTime later.
    FirstWave { AfterStartTimeOrImmediately = "first_wave_after_start_time_or_immediately", AfterOnePause = "first_wave_after_one_pause" }
);
calib_enum!(
    /// spawner.START_TIME_ORIGIN -- a set SpawnStartTime counts from ACTIVATION (the
    /// tick the deploy timer reaches 0; the timer is loaded with the raw column) or
    /// from PLACEMENT (loaded with max(0, SpawnStartTime - DeployTime): the deploy
    /// window already consumed that much of it). Every 15.535 spawning troop ships
    /// SpawnStartTime == DeployTime, which under from_placement is "first wave at
    /// deploy end"; the 2018 Witch (1000 / 1000) and DarkWitch (1500 / 1000) part the
    /// arms by 20 ticks.
    StartTimeOrigin { FromActivation = "from_activation", FromPlacement = "from_placement" }
);
calib_enum!(
    /// spawner.PAUSE_ANCHOR -- SpawnPauseTime counts from the last unit of a timed
    /// wave (the timer is reloaded with the pause after it) or from its first (the
    /// pause less the wave's own length, floored at 0).
    PauseAnchor { AfterLastUnit = "after_last_unit_of_wave", AfterFirstUnit = "after_first_unit_of_wave" }
);
calib_enum!(
    /// spawner.SPAWN_POINT -- the spawner's centre plus its own collision radius (or
    /// SpawnRadius when set) along the owner's forward axis, or its centre.
    SpawnPoint { InFrontAtOwnRadius = "in_front_toward_enemy_at_own_radius", AtCentre = "at_centre" }
);
calib_enum!(
    /// spawner.DEATH_SPAWN_RADIUS_DEFAULT -- a blank DeathSpawnRadius means the dying
    /// entity's own collision radius, or zero.
    DeathSpawnRadius { OwnCollisionRadius = "own_collision_radius", Zero = "zero" }
);
calib_enum!(
    /// spawner.DEATH_SPAWN_DEPLOY_TIME_DEFAULT -- a blank DeathSpawnDeployTime means the
    /// unit's own DeployTime, or zero.
    DeathSpawnDeploy { UnitOwnDeployTime = "unit_own_deploy_time", Zero = "zero" }
);
calib_enum!(
    /// charge.CHARGE_RANGE_UNIT -- the unit of the raw ChargeRange column (Prince 250,
    /// BattleRam 300), converted to a SUBTILE run-up in `charge_need`. Every other
    /// distance column in characters.csv is millitiles, but 250 millitiles (0.25
    /// tiles) is not a run-up anyone has seen; centitiles (2.5 tiles) is the
    /// community figure. A TIME reading (centiseconds) is algebraically the same
    /// thing for the three shipped cards (all Speed 60 = 1 tile/s) and is expressed
    /// through charge.ACCUMULATOR = time_moving, not here.
    ChargeRangeUnit { Centitiles = "centitiles", Millitiles = "millitiles" }
);
calib_enum!(
    /// charge.ACCUMULATOR -- what the run-up counts (state.rs `charge_pass`). All five
    /// are frame-free scalars of the unit's OWN requested step, proposed walk or
    /// before/after positions, so no seat frame enters. Identical for a free-walking
    /// unit on a straight lane; they part on a diagonal walk (per-axis truncation
    /// shortens the step), a body-blocked or pushed unit (net movement is not the
    /// requested step), and a walk that is not toward the target.
    ChargeAccumulator {
        /// The 16.402 reading: `progress += tdiv(L x 1000, ChargeRange)` per walking
        /// tick with `L = min(S, dist, 250)` the REQUESTED step in native units
        /// (the step move16402::move_towards asks for), threshold 10000 permille
        /// (`>= 10000` charges; a non-walking tick resets to 0). CHARGE_RANGE_UNIT
        /// plays no part under this arm.
        Client16402ProgressPermille = "client16402_progress_permille",
        /// The length of the walk the Path phase proposed this tick.
        WalkDeltaLength = "walk_delta_length",
        /// Its projection onto the direction to the target (or default tower), floored at 0.
        WalkDeltaTowardTarget = "walk_delta_toward_target",
        /// The length of the NET move (walk, clamp, separation and pushes together).
        NetMoveLength = "net_move_length",
        /// TICK_MS per tick with a non-zero proposed walk; the threshold is the time
        /// an unbuffed unit of the card takes to cover the run-up.
        TimeMoving = "time_moving",
    }
);
calib_enum!(
    /// charge.MULTIPLIER_MEANING -- what ChargeSpeedMultiplier multiplies
    /// (`effective_speed`; `charge_need`).
    ChargeMultiplier {
        /// The walking speed, once the run-up is complete (the visible Prince charge).
        MovementWhenCharged = "movement_when_charged",
        /// The walking speed from the first step, run-up included.
        MovementAlways = "movement_always",
        /// The rate the run-up fills at (the threshold divided by it); the speed never changes.
        AccumulationRate = "accumulation_rate",
    }
);
calib_enum!(
    /// charge.PROGRESS_ON_STOP -- a tick on which the unit does not walk (attacking,
    /// stunned, deploying, blocked with no proposed walk) zeroes the partial run-up
    /// or leaves it. Only `charge_progress`: a completed charge is never undone by
    /// standing still.
    ChargeStopRule { Reset = "reset", Hold = "hold" }
);
calib_enum!(
    /// charge.SPECIAL_LEVEL_SCALING -- DamageSpecial at level L (combat.rs `fire`).
    /// The two agree at level 1 (DamageSpecial = 2 x Damage on every row) and part
    /// under truncation from local level 2 on: Prince 539 against 2 x 269 = 538.
    ChargeLevelScaling {
        /// `CardDb::scaled(DamageSpecial)`: a level-1 stat like every other.
        ScaleSpecialBase = "scale_special_base",
        /// The entity's scaled Damage times DamageSpecial / Damage.
        TwiceScaledDamage = "twice_scaled_damage",
    }
);
calib_enum!(
    /// pathfinding.WAYPOINT_ARRIVE_RULE. `SegmentProjection` is the measured rule
    /// (16 errors over 34 644 tick pairs); `EuclidPostMove` is spec rule 7.5 as
    /// written (156). See path2026.rs `arrived`.
    WaypointArriveRule { SegmentProjection = "segment_projection", EuclidPostMove = "euclid_post_move" }
);
calib_enum!(
    /// pathfinding.OCCLUDED_CELL_TREATMENT. MEASURED `cost_50` on the live 16.402
    /// corpus: 97 of 785 paths cross a building box interior, which a hard block
    /// makes infeasible (456 infeasible without the goal exemption, 97 with it) and
    /// `PATHFINDING_BUILDING_COST = 50` leaves at 6 failures. `block` is the refuted
    /// 15.535 reading, kept runnable.
    ///
    /// It reaches BUILDING BOXES ONLY. Not water: the two used to share one branch
    /// in `CostField::terrain`, so `cost_50` here silently made the river traversable
    /// and overrode WATER_RULE_GROUND, which `from_json` refuses to read any other
    /// way. Not bit-16 terrain either, for the same reason one key may not override
    /// another -- bit-16 is now pinned impassable in `terrain` and the box cost
    /// simply takes precedence over it where they overlap.
    OccludedCells { Block = "block", Cost50 = "cost_50" }
);
calib_enum!(
    /// pathfinding.HEURISTIC_FORM -- the metric `h` uses, always minimised over the
    /// GOAL SET (path2026.rs `goal_set_heuristic`; the goal-set treatment is a
    /// separate, still-measured claim and neither value changes it).
    ///
    /// `chebyshev_over_goal_set` is the measured one; `octile_over_goal_set` is the
    /// hypothesis it replaced, kept runnable so the ledger's candidate list names
    /// arms that exist. The other two names the ledger used to carry
    /// (`octile_to_target_cell_minus_reach`, `none`) were dropped from it because no
    /// arm implements them -- `pick` refuses any name not listed here.
    HeuristicForm {
        ChebyshevOverGoalSet = "chebyshev_over_goal_set",
        OctileOverGoalSet = "octile_over_goal_set",
    }
);
calib_enum!(
    /// pathfinding.PATH_SEARCH -- see `Calib::path_search`.
    PathSearch {
        Client16402 = "client16402",
        TraceFittedAstar = "trace_fitted_astar",
    }
);
calib_enum!(
    /// pathfinding.TIE_BREAK -- the neighbour order path2026.rs `neighbours()`
    /// expands in. UNVERIFIED and known to be insufficient (spec 3.6). These three
    /// are the three the engine implements, so `pick` refuses any other name; the
    /// ledger's `candidates` list is kept in step with them.
    TieBreak {
        OrthoFirstPlaceholder = "ortho_first_placeholder",
        RowMajor = "rowmajor",
        DiagFirst = "diag_first",
    }
);

fn pick<T>(v: &Value, path: &[&str], parse: fn(&str) -> Option<T>) -> Result<T, String> {
    let s = string(v, path)?;
    parse(s).ok_or_else(|| format!("{} = {s} has no engine implementation", path.join(".")))
}

/// A key whose ONE implemented candidate is `only`: read, and refused if it names
/// another. The registry lists the alternatives; the engine does not run them.
fn only(v: &Value, path: &[&str], only: &str) -> Result<(), String> {
    let s = string(v, path)?;
    if s == only {
        Ok(())
    } else {
        Err(format!("{} = {s} has no engine implementation (only {only})", path.join(".")))
    }
}

fn at<'a>(v: &'a Value, path: &[&str]) -> Result<&'a Value, String> {
    let mut cur = v;
    for k in path {
        cur = cur.get(*k).ok_or_else(|| format!("calibration.json: missing {}", path.join(".")))?;
    }
    Ok(cur)
}

fn int(v: &Value, path: &[&str]) -> Result<i32, String> {
    at(v, path)?
        .as_i64()
        .and_then(|x| i32::try_from(x).ok())
        .ok_or_else(|| format!("calibration.json: {} is not an i32", path.join(".")))
}

fn boolean(v: &Value, path: &[&str]) -> Result<bool, String> {
    at(v, path)?.as_bool().ok_or_else(|| format!("calibration.json: {} is not a bool", path.join(".")))
}

fn string<'a>(v: &'a Value, path: &[&str]) -> Result<&'a str, String> {
    at(v, path)?.as_str().ok_or_else(|| format!("calibration.json: {} is not a string", path.join(".")))
}

fn globals_number(name: &str) -> Result<i32, String> {
    let (h, rows) = crate::card::parse_supercell_csv(GLOBALS_CSV);
    let c_name = h.iter().position(|c| c == "Name").ok_or("globals.csv: no Name")?;
    let c_num = h.iter().position(|c| c == "NumberValue").ok_or("globals.csv: no NumberValue")?;
    rows.iter()
        .find(|r| r.get(c_name).map(|s| s.as_str()) == Some(name))
        .and_then(|r| r.get(c_num))
        .and_then(|s| s.trim().parse::<i32>().ok())
        .ok_or_else(|| format!("globals.csv: {name} missing"))
}

impl Calib {
    pub fn from_json(s: &str) -> Result<Calib, String> {
        let v: Value = serde_json::from_str(s).map_err(|e| format!("calibration.json: {e}"))?;
        let m = crate::fixed::milli;
        let push = string(&v, &["collision", "PUSH_MODEL", "value"])?;
        let push_model = match push {
            "mass_weighted" => PushModel::MassWeighted,
            "speed_weighted" => PushModel::SpeedWeighted,
            "equal_split" => PushModel::EqualSplit,
            other => return Err(format!("PUSH_MODEL {other} has no engine implementation")),
        };
        let fp = string(&v, &["collision", "BUILDING_FOOTPRINT_MODEL", "value"])?;
        let footprint_model = FootprintModel::from_calibration_name(fp)
            .ok_or_else(|| format!("BUILDING_FOOTPRINT_MODEL {fp} unknown"))?;
        let alg = string(&v, &["pathfinding", "ALGORITHM", "value"])?;
        let path_model = match alg {
            "lane_flow_with_local_avoidance" => PathModel::LaneSnap,
            // MEASURED: the live game's weighted grid A*, path2026.rs.
            "weighted_grid_astar" => PathModel::Oracle2026,
            // The pre-measurement community reading, kept runnable and refuted.
            "weighted_grid_astar_community" => PathModel::GridAStar,
            "post2025_diagonal_with_lookahead" => PathModel::DiagonalLookahead,
            other => return Err(format!("pathfinding.ALGORITHM {other} has no engine implementation")),
        };
        // The pathfinding cost table is datamined; HOW it is applied is measured
        // (calibration pathfinding.PATHFINDING_COSTS.application). Read both.
        let cost_of = |k: &str| -> Result<i32, String> {
            int(&v, &["pathfinding", "PATHFINDING_COSTS", "value", k])
        };
        let diag_num = int(&v, &["pathfinding", "DIAGONAL_COST_RATIO", "value", "num"])?;
        let diag_den = int(&v, &["pathfinding", "DIAGONAL_COST_RATIO", "value", "den"])?;
        // One pathfinding cell is CELL_SIZE_NATIVE millitiles; the engine's arena
        // half-cell must be exactly that, or the node encoding the oracle publishes
        // does not concern the engine's grid.
        let cell_native = int(&v, &["pathfinding", "CELL_SIZE_NATIVE", "value"])?;
        let arena_cell = crate::arena::Arena::shipped().cell;
        if m(cell_native) != arena_cell {
            return Err(format!(
                "pathfinding.CELL_SIZE_NATIVE {cell_native} is {} subtiles, but the arena half-tile cell is {arena_cell}",
                m(cell_native)
            ));
        }
        only(&v, &["pathfinding", "PATH_NODE_ENCODING", "value"], "row_major_36_goal_first_centre_plus_250")?;
        only(&v, &["pathfinding", "PATH_GOAL_RULE", "value"], "first_cell_within_range_plus_own_collision_radius")?;
        only(&v, &["pathfinding", "OCCLUSION_MODEL", "value"], "halfopen_aabb_collision_radius_no_mover_pad_goal_exempt")?;
        only(&v, &["pathfinding", "WATER_RULE_GROUND", "value"], "impassable")?;
        only(&v, &["movement", "HEADING_LAW", "value"], "norm256_floored_isqrt_pre_move")?;
        only(&v, &["movement", "POSITION_ROUNDING", "value"], "truncate_toward_zero_per_axis_no_carry")?;
        only(&v, &["movement", "STOMP_SPEED_RULE", "value"], "speed_times_stop_plus_wait_over_stop")?;
        only(&v, &["movement", "STOMP_PAUSE_SCHEDULE", "value"], "k_plus_1_times_tick_ms_mod_period_strictly_greater_than_stop")?;
        only(&v, &["movement", "DEPLOY_TIMING", "value"], "spawn_anchored_full_first_step")?;
        only(&v, &["movement", "CONTACT_DOMAIN", "value"], "isolated_unit_only")?;
        {
            // REPLAN_TRIGGERS is a SET, and the engine implements exactly this set.
            let want = ["goal_cell_changed", "friendly_building_set_changed"];
            let got: Vec<String> = at(&v, &["pathfinding", "REPLAN_TRIGGERS", "value"])?
                .as_array()
                .ok_or("calibration.json: pathfinding.REPLAN_TRIGGERS.value is not an array")?
                .iter()
                .map(|x| x.as_str().unwrap_or("?").to_string())
                .collect();
            if got != want {
                return Err(format!("pathfinding.REPLAN_TRIGGERS {got:?} has no engine implementation (only {want:?})"));
            }
        }
        let mana_speed_up_remaining_s = match at(&v, &["match", "MANA_SPEED_UP_WHEN_REMAINING_SECONDS", "value"]) {
            Ok(x) => x.as_i64().map(|x| x as i32).ok_or("MANA_SPEED_UP_WHEN_REMAINING_SECONDS not int")?,
            Err(_) => globals_number("MANA_SPEED_UP_WHEN_REMAINING_SECONDS")?,
        };
        let terr = string(&v, &["arena", "TERRITORY_MODEL", "value"])?;
        let territory_model = TerritoryModel::from_calibration_name(terr)
            .ok_or_else(|| format!("arena.TERRITORY_MODEL {terr} has no engine implementation"))?;
        let c = Calib {
            tick_ms: int(&v, &["time", "TICK_MS", "value"])?,
            speed_to_subtiles_per_tick: int(&v, &["time", "SPEED_TO_SUBTILES_PER_TICK", "value"])?,
            add_character_range_to_radius: boolean(&v, &["targeting", "ADD_CHARACTER_RANGE_TO_RADIUS", "value"])?,
            extra_sight_range_to_crown_towers: m(int(&v, &["targeting", "EXTRA_SIGHT_RANGE_TO_CROWN_TOWERS", "value"])?),
            extra_sight_range_to_building: m(int(&v, &["targeting", "EXTRA_SIGHT_RANGE_TO_BUILDING", "value"])?),
            range_extension_to_keep_target: m(int(&v, &["targeting", "LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET", "value"])?),
            cancel_hit_from_long_distance_range: m(int(
                &v,
                &["targeting", "LOGIC_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE", "value"],
            )?),
            preserve_target_if_hit_started: boolean(
                &v,
                &["targeting", "LOGIC_PRESERVE_TARGET_IF_HIT_STARTED", "value"],
            )?,
            xpos_based_tower_targeting: boolean(&v, &["targeting", "LOGIC_XPOS_BASED_TOWER_TARGETING", "value"])?,
            melee_range_limit: m(int(&v, &["targeting", "MELEE_RANGE_LIMIT", "value"])?),
            push_model,
            separation_iterations: int(&v, &["collision", "SEPARATION_ITERATIONS", "value"])?,
            footprint_model,
            path_model,
            repath_interval_ticks: match at(&v, &["pathfinding", "REPATH_INTERVAL_TICKS", "value"])? {
                Value::Null => None,
                x => Some(
                    x.as_i64()
                        .and_then(|n| i32::try_from(n).ok())
                        .ok_or("calibration.json: pathfinding.REPATH_INTERVAL_TICKS is neither null nor an i32")?,
                ),
            },
            path_cost_default: cost_of("default")?,
            path_cost_road: cost_of("road")?,
            path_cost_blocked: cost_of("blocked")?,
            path_cost_building: cost_of("building")?,
            path_cost_heuristic: cost_of("defaultheuristic")?,
            diag_num,
            diag_den,
            waypoint_arrive_radius: m(int(&v, &["pathfinding", "WAYPOINT_ARRIVE_RADIUS", "value"])?),
            waypoint_arrive_rule: pick(&v, &["pathfinding", "WAYPOINT_ARRIVE_RULE", "value"], WaypointArriveRule::from_calibration_name)?,
            occluded_cells: pick(&v, &["pathfinding", "OCCLUDED_CELL_TREATMENT", "value"], OccludedCells::from_calibration_name)?,
            heuristic_form: pick(&v, &["pathfinding", "HEURISTIC_FORM", "value"], HeuristicForm::from_calibration_name)?,
            tie_break: pick(&v, &["pathfinding", "TIE_BREAK", "value"], TieBreak::from_calibration_name)?,
            path_search: pick(&v, &["pathfinding", "PATH_SEARCH", "value"], PathSearch::from_calibration_name)?,
            mana_regen_ms_1x: int(&v, &["match", "MANA_REGEN_MS_1X", "value"])?,
            mana_regen_ms_2x: int(&v, &["match", "MANA_REGEN_MS_2X", "value"])?,
            start_mana: int(&v, &["match", "START_MANA", "value"])?,
            max_mana: int(&v, &["match", "MAX_MANA", "value"])?,
            king_activate_time_ms: int(&v, &["match", "KING_ACTIVATE_TIME_MS", "value"])?,
            battle_start_cooldown_ms: int(&v, &["match", "LOGIC_BATTLE_START_COOLDOWN_MS", "value"])?,
            regular_time_s: int(&v, &["match", "REGULAR_TIME_S", "value"])?,
            overtime_s: int(&v, &["match", "OVERTIME_S", "value"])?,
            three_crown_instant_win: boolean(&v, &["match", "THREE_CROWN_INSTANT_WIN", "value"])?,
            overtime_tiebreak: pick(&v, &["match", "OVERTIME_TIEBREAK", "value"], OvertimeTiebreak::from_calibration_name)?,
            mana_speed_up_remaining_s,
            territory_model,
            projectile_speed_to_subtiles_per_tick: int(&v, &["time", "PROJECTILE_SPEED_TO_SUBTILES_PER_TICK", "value"])?,
            crown_rounding: pick(&v, &["combat", "CROWN_TOWER_DAMAGE_ROUNDING", "value"], CrownRounding::from_calibration_name)?,
            aoe_hit_test: pick(&v, &["spells", "AOE_HIT_TEST", "value"], AoeHitTest::from_calibration_name)?,
            spell_as_deploy_launch: pick(&v, &["spells", "SPELL_AS_DEPLOY_LAUNCH_MODEL", "value"], LaunchModel::from_calibration_name)?,
            rolling_hit_shape: pick(&v, &["spells", "ROLLING_HIT_SHAPE", "value"], RollHitShape::from_calibration_name)?,
            spawning_spell_water: pick(&v, &["spells", "SPAWNING_SPELL_WATER_RULE", "value"], SpawnWaterRule::from_calibration_name)?,
            knock_duration_ms: int(&v, &["knockback", "DURATION_MS", "value"])?,
            knock_zero_vector: pick(&v, &["knockback", "ZERO_VECTOR_DIRECTION", "value"], KnockZeroVector::from_calibration_name)?,
            knock_attack_reset: pick(&v, &["knockback", "ATTACK_RESET", "value"], KnockAttackReset::from_calibration_name)?,
            knock_affects_deploying: boolean(&v, &["knockback", "AFFECTS_DEPLOYING_UNITS", "value"])?,
            knock_direction_rolling: pick(&v, &["knockback", "DIRECTION_ROLLING", "value"], RollDirection::from_calibration_name)?,
            stun_attack_timer: pick(&v, &["status", "STUN_ATTACK_TIMER_MODEL", "value"], StunTimerModel::from_calibration_name)?,
            stun_retarget_on_resume: boolean(&v, &["status", "STUN_RETARGET_ON_RESUME", "value"])?,
            resume_retarget_windup: pick(&v, &["status", "RESUME_RETARGET_WINDUP", "value"], ResumeWindup::from_calibration_name)?,
            buff_expiry: pick(&v, &["status", "BUFF_EXPIRY_TICK_ALIGNMENT", "value"], BuffExpiry::from_calibration_name)?,
            same_buff_reapply: pick(&v, &["status", "SAME_BUFF_REAPPLY", "value"], BuffReapply::from_calibration_name)?,
            stun_pauses_deploy: boolean(&v, &["status", "STUN_PAUSES_DEPLOY_TIMER", "value"])?,
            stun_pauses_building_lifetime: boolean(&v, &["status", "STUN_PAUSES_BUILDING_LIFETIME", "value"])?,
            stun_pauses_king_activation: boolean(&v, &["status", "STUN_PAUSES_KING_ACTIVATION", "value"])?,
            hide_starts_hidden: boolean(&v, &["hide", "STARTS_HIDDEN", "value"])?,
            hide_rise_trigger: pick(&v, &["hide", "RISE_TRIGGER", "value"], RiseTrigger::from_calibration_name)?,
            hide_targetable_while_rising: boolean(&v, &["hide", "TARGETABLE_WHILE_RISING", "value"])?,
            hide_hidden_immune: boolean(&v, &["hide", "HIDDEN_IMMUNE_TO_DAMAGE", "value"])?,
            hide_delay_meaning: pick(&v, &["hide", "HIDE_DELAY_MEANING", "value"], HideDelayMeaning::from_calibration_name)?,
            spawner_first_wave: pick(&v, &["spawner", "FIRST_WAVE", "value"], FirstWave::from_calibration_name)?,
            spawner_start_time_origin: pick(&v, &["spawner", "START_TIME_ORIGIN", "value"], StartTimeOrigin::from_calibration_name)?,
            spawner_pause_anchor: pick(&v, &["spawner", "PAUSE_ANCHOR", "value"], PauseAnchor::from_calibration_name)?,
            spawner_spawn_point: pick(&v, &["spawner", "SPAWN_POINT", "value"], SpawnPoint::from_calibration_name)?,
            spawner_stun_pauses: boolean(&v, &["spawner", "STUN_PAUSES_SPAWNER", "value"])?,
            death_spawn_radius_default: pick(&v, &["spawner", "DEATH_SPAWN_RADIUS_DEFAULT", "value"], DeathSpawnRadius::from_calibration_name)?,
            death_spawn_deploy_default: pick(&v, &["spawner", "DEATH_SPAWN_DEPLOY_TIME_DEFAULT", "value"], DeathSpawnDeploy::from_calibration_name)?,
            charge_range_unit: pick(&v, &["charge", "CHARGE_RANGE_UNIT", "value"], ChargeRangeUnit::from_calibration_name)?,
            charge_accumulator: pick(&v, &["charge", "ACCUMULATOR", "value"], ChargeAccumulator::from_calibration_name)?,
            charge_multiplier_meaning: pick(&v, &["charge", "MULTIPLIER_MEANING", "value"], ChargeMultiplier::from_calibration_name)?,
            charge_progress_on_stop: pick(&v, &["charge", "PROGRESS_ON_STOP", "value"], ChargeStopRule::from_calibration_name)?,
            charge_reset_on_attack: boolean(&v, &["charge", "RESET_ON_ATTACK", "value"])?,
            charge_reset_on_stun: boolean(&v, &["charge", "RESET_ON_STUN", "value"])?,
            charge_reset_on_knockback: boolean(&v, &["charge", "RESET_ON_KNOCKBACK", "value"])?,
            charge_reset_on_retarget: boolean(&v, &["charge", "RESET_ON_RETARGET", "value"])?,
            charge_special_level_scaling: pick(&v, &["charge", "SPECIAL_LEVEL_SCALING", "value"], ChargeLevelScaling::from_calibration_name)?,
        };
        // hide.HIDDEN_OCCLUDES_PATH: the one implemented candidate is `true` (a hidden
        // footprint still occludes the path grid and blocks deploys -- the grid never
        // looks at the hide state). `false` needs the pathfinder's code and is refused,
        // the `only()` rule for a boolean key.
        if !boolean(&v, &["hide", "HIDDEN_OCCLUDES_PATH", "value"])? {
            return Err("hide.HIDDEN_OCCLUDES_PATH = false has no engine implementation (only true)".into());
        }
        // Single-implementation keys: read so the registry cannot silently disagree.
        only(&v, &["spells", "LAUNCH_POINT", "value"], "caster_king_tower_centre")?;
        only(&v, &["spells", "WAVE_AREA_MODEL", "value"], "single_disc_one_hit_per_wave")?;
        only(&v, &["spells", "ONE_SHOT_AREA_EFFECT_APPLICATION", "value"], "first_update_only")?;
        only(&v, &["spells", "PROJECTILE_SPAWN_FORMATION", "value"], "engine_grid")?;
        only(&v, &["knockback", "DISPLACEMENT_LAW", "value"], "fixed_distance")?;
        only(&v, &["knockback", "STACKING", "value"], "vector_sum")?;
        only(&v, &["knockback", "WATER_RESOLUTION", "value"], "eject_to_nearest_land")?;
        // spawner.LIMIT_RULE / DEATH_SPAWN_LAYOUT: one implemented arm each (the
        // `only()` rule); the other candidate is refused, never mapped.
        only(&v, &["spawner", "LIMIT_RULE", "value"], "skip_unit_keep_cadence")?;
        only(&v, &["spawner", "DEATH_SPAWN_LAYOUT", "value"], "engine_grid_within_radius")?;
        if c.projectile_speed_to_subtiles_per_tick <= 0 || c.knock_duration_ms < 0 {
            return Err("calibration.json: non-positive projectile speed or negative knockback duration".into());
        }
        if c.tick_ms <= 0
            || c.mana_regen_ms_1x <= 0
            || c.mana_regen_ms_2x <= 0
            || c.repath_interval_ticks.is_some_and(|r| r <= 0)
        {
            return Err("calibration.json: non-positive tick / regen / repath value".into());
        }
        if c.diag_den <= 0 || c.diag_num <= 0 || c.path_cost_road <= 0 || c.path_cost_default <= 0 {
            return Err("calibration.json: non-positive pathfinding cost or diagonal ratio".into());
        }
        Ok(c)
    }

    /// The shipped calibration.json, parsed once per process.
    pub fn shipped() -> Calib {
        static CELL: OnceLock<Calib> = OnceLock::new();
        CELL.get_or_init(|| Calib::from_json(CALIBRATION_JSON).expect("shipped calibration.json must parse"))
            .clone()
    }
}

// ---------------------------------------------------------------------------
// config

#[derive(Clone, Debug)]
pub struct BattleConfig {
    pub calib: Calib,
    pub arena: Arena,
    pub cards: Arc<CardDb>,
    pub path_model: PathModel,
    pub push_model: PushModel,
    pub footprint_model: FootprintModel,
    pub tower_sight_reading: TowerSightReading,
    /// Card names per team (Blue, Red). Up to 8; the first 4 start in hand.
    pub decks: [Vec<String>; 2],
    /// Unified card level per team.
    pub card_level: [i32; 2],
    pub tower_level: [i32; 2],
    /// Shuffle decks from the battle Rng at start.
    pub shuffle_decks: bool,
    /// Spatial hash bucket, subtiles.
    pub bucket_subtiles: i32,
}

impl BattleConfig {
    /// Everything from calibration.json and the shipped arena, the given cards,
    /// no decks, decks unshuffled, and every card AND tower at the lowest unified
    /// level every rarity has (`CardDb::lowest_level_valid_for_every_rarity`, 9 on
    /// the 2018 table).
    ///
    /// Level 1 is NOT a usable default: unified level 1 does not exist for
    /// Rare/Epic/Legendary, so `try_new` rejects any deck holding one. Towers take
    /// the same level so troops and towers are on one scale.
    ///
    /// WHY 9 IS A REASONABLE DEFAULT (a DEFAULT, not a measurement). Unified level 9
    /// under this data's rarity-relative table (rarities.csv RelativeLevel: Common 0,
    /// Rare 2, Epic 5, Legendary 8 -- read from the file) is Common 9 / Rare 7 /
    /// Epic 4 / Legendary 1, which is the pre-2021 tournament-standard level cap:
    /// the tournament standard of the data's own ~2018 vintage (community-known
    /// rule, not verified from any file in this repo). The live 2026 game's levels
    /// differ, and no measurement of them exists here.
    /// A different level is one BattleConfig field away.
    pub fn with_cards(cards: CardDb) -> BattleConfig {
        let level = cards.lowest_level_valid_for_every_rarity();
        let calib = Calib::shipped();
        BattleConfig {
            path_model: calib.path_model,
            push_model: calib.push_model,
            footprint_model: calib.footprint_model,
            calib,
            arena: Arena::shipped(),
            cards: Arc::new(cards),
            tower_sight_reading: TowerSightReading::UnitsSeeTowersFarther,
            decks: [Vec::new(), Vec::new()],
            card_level: [level, level],
            tower_level: [level, level],
            shuffle_decks: false,
            bucket_subtiles: SUBTILE,
        }
    }
}

// ---------------------------------------------------------------------------
// public result types

/// Cards in hand. A game rule, not a physics constant (protocol.py HAND_SIZE).
pub const HAND_SIZE: usize = 4;

/// Why a deploy was refused.
///
/// The position variants say WHY, in the order `check_deploy` tests them, and
/// correspond one-to-one by NAME with protocol.py `DeployStatus` (OUT_OF_ARENA,
/// WATER, NO_DEPLOY, OUT_OF_TERRITORY, OCCUPIED; BAD_SLOT, EMPTY_SLOT). The action
/// mask tests compare reason codes, not just accept/refuse, so a mask that is
/// right for the wrong reason is caught. A single `InvalidPosition` for every
/// position refusal cannot tell water from a building from the wrong half, which
/// makes any disagreement with the mask undiagnosable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeployError {
    GameOver,
    UnknownCard(String),
    UnsupportedCard(String, String),
    NotInHand,
    /// Hand slot index >= HAND_SIZE.
    BadSlot,
    /// Slot index valid but the hand has no card there.
    EmptySlot,
    NotEnoughElixir { have: i32, need: i32 },
    OutOfArena,
    Water,
    NoDeploy,
    OutOfTerritory,
    /// On (or touching) a building's footprint.
    Occupied,
    InvalidLevel(String),
}

impl From<crate::arena::ZoneError> for DeployError {
    fn from(z: crate::arena::ZoneError) -> Self {
        match z {
            crate::arena::ZoneError::OutOfArena => DeployError::OutOfArena,
            crate::arena::ZoneError::Water => DeployError::Water,
            crate::arena::ZoneError::NoDeploy => DeployError::NoDeploy,
            crate::arena::ZoneError::OutOfTerritory => DeployError::OutOfTerritory,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Outcome {
    Winner(Team),
    Draw,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PlayerState {
    /// Elixir in units of 1/mana_unit elixir.
    pub mana: i64,
    pub hand: Vec<u16>,
    pub queue: VecDeque<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
struct PendingSpawn {
    team: Team,
    card: u16,
    level: i32,
    pos: Vec2,
    /// Deploy time override (a Goblin Barrel's SpawnCharacterDeployTime); None = the
    /// card's own. For a SPELL card the entry is a cast, not a unit (phase_spawn).
    deploy_ms: Option<i32>,
    /// The periodic spawner that emitted this unit (entity.rs `spawned_by`, for
    /// SpawnLimit); None for a deploy, a spell release and a death spawn.
    owner: Option<EntityId>,
}

/// A read-only view of one entity, for observation builders.
#[derive(Clone, Copy, Debug)]
pub struct EntityView<'a> {
    pub id: EntityId,
    pub team: Team,
    pub kind: EntityKind,
    pub card: &'a str,
    /// Index of `card` in the CardDb (for bindings that must not look names up).
    pub card_idx: u16,
    pub pos: Vec2,
    pub hp: i32,
    pub max_hp: i32,
    pub shield: i32,
    pub radius: i32,
    pub flying: bool,
    pub deploying: bool,
    pub target: Option<EntityId>,
    pub attack_phase: AttackPhase,
    pub team_seq: u32,
    /// ms elapsed in the current attack phase.
    pub attack_ms: i32,
    /// ms of deploy time remaining.
    pub deploy_ms: i32,
    pub target_locked: bool,
    /// Subtiles per tick.
    pub speed: i32,
    /// Sub-subtile movement carry, in the entity's TEAM frame (so mirror twins
    /// carry identical values).
    pub move_frac: Vec2,
    /// Planned waypoints (world coordinates), next first.
    pub route: &'a [Vec2],
    /// ms of stun remaining.
    pub stun_ms: i32,
    /// Will rescan on the first unstunned Target phase (status.STUN_RETARGET_ON_RESUME).
    pub retarget_on_resume: bool,
    /// ms of knockback slide remaining, and the displacement still to apply.
    pub knock_ms: i32,
    pub knock_rem: Vec2,
    /// Hide state (Tesla; `Up` on everything else) and its timer (entity.rs
    /// `HideState` says what the timer means in each state).
    pub hide_state: HideState,
    pub hide_ms: i32,
    /// `hide_state == Hidden`: under ground, untargetable, immune (hide.*).
    pub hidden: bool,
    /// Periodic spawner (card.rs `SpawnerDef`; 0 / 0 on every other entity): ms until
    /// its next emission (entity.rs `spawn_ms`) and the units of the current wave
    /// still to come (`spawn_wave_left`, 0 between waves).
    pub spawn_ms: i32,
    pub spawn_wave_left: i32,
    /// The spawner that emitted this unit, if any (SpawnLimit bookkeeping).
    pub spawned_by: Option<EntityId>,
    /// CHARGE (card.rs `ChargeDef`; false / 0 on every other entity): the run-up is
    /// complete, and the run-up accumulated so far (entity.rs `charge_progress`,
    /// in the unit calibration charge.ACCUMULATOR selects -- RAW, not divided).
    pub charged: bool,
    pub charge_progress: i32,
    /// The subtile step this entity takes on a walking tick RIGHT NOW: `speed`
    /// through every speed buff in force (`BattleState::effective_speed` -- the
    /// charge today; rage and slow when they exist). Equal to `speed` for every
    /// unbuffed entity.
    pub effective_speed: i32,
}

#[derive(Default, Clone, Debug)]
struct Scratch {
    nb: Vec<u32>,
    sums: Vec<i64>,
    decisions: Vec<(usize, TargetDecision)>,
    deltas: Vec<Vec2>,
    obstacles: [Vec<Obstacle>; 2],
    /// Hash of each team's OWN buildings as the path grid sees them; a change is
    /// replan trigger 2 (calibration pathfinding.REPLAN_TRIGGERS).
    occluder_epoch: [u32; 2],
    blockers: Vec<UnitBlocker>,
    collide: CollideScratch,
    /// Every entity's position at the start of the Move phase's walk (AFTER the
    /// knockback slides), for the charge accumulator's net-move reading.
    pre: Vec<Vec2>,
    /// The requested step of every entity's walk this tick in NATIVE units
    /// (`L = min(speed, dist, 250)`, the step move16402::move_towards asks for),
    /// for the charge accumulator's client16402 reading; 0 on a tick with no walk.
    walk_step: Vec<i32>,
    /// The selected pathfinder's grid, shared by every unit this tick
    /// (PATH_SEARCH = client16402).
    grid16402: Option<Grid16402>,
}

/// The 16.402 path grid (path16402.rs): the static terrain, this tick's occlusion
/// array and the previous one. The measured rule rebuilds the occlusion layer from
/// scratch before every tick and keeps the previous one; here it is rebuilt when
/// either team's occluder epoch
/// moves, which is the only time the stamps change, and the previous array is the
/// one from before that change -- the pair the SAMEPATH test compares.
#[derive(Clone, Debug)]
struct Grid16402 {
    terrain: path16402::Terrain,
    costs: path16402::Costs,
    occ_cur: Vec<i32>,
    occ_prev: Vec<i32>,
    epochs: [u32; 2],
    pf: path16402::PathFinder,
}

impl Grid16402 {
    fn new(arena: &Arena, calib: &Calib) -> Grid16402 {
        let costs = path2026::costs16402(calib);
        let terrain = path2026::terrain16402(arena, &costs);
        let n = (arena.cols * arena.rows) as usize;
        Grid16402 {
            terrain,
            costs,
            occ_cur: vec![0; n],
            occ_prev: vec![0; n],
            epochs: [u32::MAX, u32::MAX],
            pf: path16402::PathFinder::new(arena.cols, arena.rows, costs.heuristic),
        }
    }

    /// Re-stamp when the building set changed. `obstacles` is Blue's list, which is
    /// the absolute one (Blue's frame is the identity) and holds BOTH sides.
    fn refresh(&mut self, epochs: [u32; 2], obstacles: &[Obstacle]) {
        if self.epochs == epochs {
            return;
        }
        use crate::fixed::SUBTILE_PER_MILLITILE as K;
        let occluders: Vec<path16402::Occluder> = obstacles
            .iter()
            .map(|o| {
                let c = o.shape.center();
                path16402::Occluder { x: c.x / K, y: c.y / K, r: o.radius / K }
            })
            .collect();
        std::mem::swap(&mut self.occ_prev, &mut self.occ_cur);
        self.occ_cur = path16402::occlusion(&self.terrain, &occluders, &self.costs);
        self.epochs = epochs;
    }
}

/// THE DEPLOY RULE OF A CARD: the territory it is placed under, and whether a
/// building footprint refuses it (Occupied). The one definition; `check_position`
/// applies it and py.rs reports it to the Python action mask, so the two cannot drift.
///
/// Troops: outside every alive enemy crown tower's NoDeploySize rect (arena.rs), and
/// refused on buildings. Buildings: own half, refused on (touching) buildings.
/// Spells by their data (card.rs SpellPlacement): anywhere, and over buildings
/// freely -- but NOT every spell is `Anywhere`. The Log (SpellAsDeploy without
/// CanDeployOnEnemySide) takes the troop territory, CanPlaceOnBuildings lifts the
/// footprint refusal, and a unit-releasing spell (Goblin Barrel) refuses water under
/// calibration spells.SPAWNING_SPELL_WATER_RULE.
pub fn deploy_rule(calib: &Calib, card: &CardDef) -> (Territory, bool) {
    let placement = card.spell.as_ref().map(|s| s.placement);
    let territory = match (card.kind, calib.territory_model) {
        (CardKind::Building, _) => Territory::OwnHalf,
        (CardKind::Troop, TerritoryModel::EnemyTowerNoDeployRects) => Territory::EnemyTowerRects,
        (CardKind::Spell, _) => match (placement, calib.spawning_spell_water) {
            #[cfg(not(clash_plant = "log_territory_anywhere"))]
            (Some(SpellPlacement::TroopTerritory { .. }), _) => Territory::EnemyTowerRects,
            #[cfg(not(clash_plant = "barrel_anywhere_incl_water"))]
            (Some(SpellPlacement::AnywhereButWater), SpawnWaterRule::RefuseTouchingWater) => Territory::AnywhereButWater,
            _ => Territory::Anywhere,
        },
    };
    let footprint_rule = match placement {
        None => true,
        Some(SpellPlacement::TroopTerritory { on_buildings }) => !on_buildings,
        Some(_) => false,
    };
    (territory, footprint_rule)
}

// ---------------------------------------------------------------------------
// the state

#[derive(Clone, Debug)]
pub struct BattleState {
    cfg: BattleConfig,
    ents: Entities,
    hash: SpatialHash,
    rng: Rng,
    players: [PlayerState; 2],
    dmg: DamageBuffer,
    projectiles: Vec<Projectile>,
    /// Live spell objects (spell.rs), in cast order.
    spells: Vec<Spell>,
    /// Knockback and stun buffers: written in Projectile, drained in Resolve.
    effects: EffectBuffer,
    spawn_queue: Vec<PendingSpawn>,
    death_queue: Vec<EntityId>,
    tick: u32,
    crowns: [u8; 2],
    towers: TowerTable,
    towers_down: [[bool; 3]; 2],
    /// ms since the king's activation trigger, if triggered.
    king_wake_ms: [Option<i32>; 2],
    king_active: [bool; 2],
    overtime: bool,
    outcome: Option<Outcome>,
    lifetime_ms: Vec<Option<i32>>,
    mana_unit: i64,
    mana_rate: [i64; 2],
    scratch: Scratch,
    phase_trace: Option<Vec<Phase>>,
}

/// path::YieldKey of entity i: seat-invariant by construction -- no team and no
/// slot index. team_seq IS in it (last): it is the spawn ordinal within the
/// unit's own team, equal for a unit and its rotated twin, and it is what tells
/// two FORMATION SIBLINGS apart. Without it -- (spawn tick, card, level, hp) alone
/// -- every sibling of one deploy holds the same key and the colinear tie-break in
/// path.rs can never tell two siblings apart. Equal keys are NOT limited to exact
/// mirror twins.
#[inline]
fn yield_key(e: &Entities, i: usize) -> path::YieldKey {
    #[cfg(clash_plant = "sibling_yield_key")]
    {
        // PLANT (regression): siblings share a key again.
        return (e.spawn_tick[i], e.card[i], e.level[i], e.hp[i], 0);
    }
    #[allow(unreachable_code)]
    (e.spawn_tick[i], e.card[i], e.level[i], e.hp[i], e.team_seq[i])
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 {
        a.abs()
    } else {
        gcd(b, a % b)
    }
}

impl BattleState {
    /// Build a battle; panics with the reason if the config is invalid.
    pub fn new(seed: u64, config: BattleConfig) -> BattleState {
        BattleState::try_new(seed, config).unwrap_or_else(|e| panic!("invalid BattleConfig: {e}"))
    }

    pub fn try_new(seed: u64, config: BattleConfig) -> Result<BattleState, String> {
        let cards = config.cards.clone();
        let c = &config.calib;
        let r1 = c.mana_regen_ms_1x as i64;
        let r2 = c.mana_regen_ms_2x as i64;
        // One elixir = lcm(regen_1x, regen_2x) units, so both rates are exact
        // integers per tick and elixir never accumulates rounding error.
        let mana_unit = r1 / gcd(r1, r2) * r2;
        let per_tick = |regen: i64| (c.tick_ms as i64) * (c.max_mana as i64) * (mana_unit / regen);
        let mana_rate = [per_tick(r1), per_tick(r2)];

        let mut rng = Rng::new(seed);
        let mut players: Vec<PlayerState> = Vec::with_capacity(2);
        for t in 0..2 {
            let mut deck: Vec<u16> = Vec::new();
            for name in &config.decks[t] {
                let idx = cards.index(name).ok_or_else(|| format!("deck card {name} unknown"))?;
                // The card's own level and every unit it can release, spawn or leave
                // behind: nothing on the tick path may fail on a level.
                cards.check_levels(idx, config.card_level[t])?;
                if cards.get(idx).summon_only {
                    return Err(format!("{name} is a spawned unit, not a playable card"));
                }
                deck.push(idx);
            }
            if config.shuffle_decks {
                for i in (1..deck.len()).rev() {
                    let j = rng.below((i + 1) as u32) as usize;
                    deck.swap(i, j);
                }
            }
            let hand: Vec<u16> = deck.iter().take(HAND_SIZE).copied().collect();
            let queue: VecDeque<u16> = deck.iter().skip(HAND_SIZE).copied().collect();
            players.push(PlayerState { mana: (c.start_mana as i64) * mana_unit, hand, queue });
        }
        let players: [PlayerState; 2] = [players.remove(0), players.remove(0)];

        let arena = config.arena.clone();
        let mut s = BattleState {
            hash: SpatialHash::new(arena.width, arena.height, config.bucket_subtiles),
            cfg: config,
            ents: Entities::new(),
            rng,
            players,
            dmg: DamageBuffer::default(),
            projectiles: Vec::new(),
            spells: Vec::new(),
            effects: EffectBuffer::default(),
            spawn_queue: Vec::new(),
            death_queue: Vec::new(),
            tick: 0,
            crowns: [0, 0],
            towers: [[None; 3]; 2],
            towers_down: [[false; 3]; 2],
            king_wake_ms: [None, None],
            king_active: [false, false],
            overtime: false,
            outcome: None,
            lifetime_ms: Vec::new(),
            mana_unit,
            mana_rate,
            scratch: Scratch::default(),
            phase_trace: None,
        };

        let king = cards.index(KING_TOWER).ok_or("no KingTower card")?;
        let princess = cards.index(PRINCESS_TOWER).ok_or("no PrincessTower card")?;
        for idx in [king, princess] {
            if cards.get(idx).no_deploy_size.is_none() {
                return Err(format!(
                    "{} has no no_deploy_size_tiles: troop territory cannot be decided (regenerate cards.json)",
                    cards.get(idx).name
                ));
            }
        }
        for team in [Team::Blue, Team::Red] {
            let lvl = s.cfg.tower_level[team as usize];
            let kpos = arena.king_tower_pos(team);
            s.towers[team as usize][0] = Some(s.spawn_now(team, king, lvl, kpos, EntityKind::KingTower)?);
            // Princesses spawn OWN-LEFT FIRST, so a tower's team_seq names the same
            // own-frame tower for both seats (Blue's own-left is engine Left, Red's
            // is engine Right). The table slot stays the ENGINE lane (k 1 = Left).
            // Spawning Left then Right for BOTH teams instead gives Red's own-left
            // tower team_seq 2 and Blue's team_seq 1, so team_seq stops being
            // rotation-invariant.
            let order = match team {
                Team::Blue => [Lane::Left, Lane::Right],
                Team::Red => [Lane::Right, Lane::Left],
            };
            for lane in order {
                let pos = arena.princess_tower_pos(team, lane);
                s.towers[team as usize][1 + lane as usize] =
                    Some(s.spawn_now(team, princess, lvl, pos, EntityKind::PrincessTower)?);
            }
        }
        s.hash.rebuild(&s.ents);
        Ok(s)
    }

    fn spawn_now(&mut self, team: Team, card: u16, level: i32, pos: Vec2, kind: EntityKind) -> Result<EntityId, String> {
        let cards = self.cfg.cards.clone();
        let c = cards.get(card);
        let scaled = |b: i32| cards.scaled(card, level, b);
        let id = self.ents.spawn(SpawnInit {
            team,
            kind,
            card,
            level,
            pos,
            hp: scaled(c.hitpoints)?,
            shield: scaled(c.shield_hitpoints)?,
            damage: scaled(c.damage)?,
            death_damage: scaled(c.death_damage)?,
            radius: c.collision_radius,
            mass: c.mass,
            // move_speed(), not `speed`: a stomp card's Speed column is not its
            // speed (card.rs, calibration movement.STOMP_SPEED_RULE).
            //
            // ONLY UNDER THE MODEL THAT PAUSES. The raised speed is only correct
            // together with movement.STOMP_PAUSE_SCHEDULE, which lives in
            // `phase_path_2026`; the three pre-2026 models never pause, so giving
            // them the raised speed made a Giant walk at 52 instead of 45 for ever
            // -- a 15 % speed-up of every stomp card, in models that are not the
            // measured one and are not supposed to change.
            speed: match self.cfg.path_model {
                PathModel::Oracle2026 => c.move_speed(),
                _ => c.speed,
            } * self.cfg.calib.speed_to_subtiles_per_tick,
            flying: c.is_flying(),
            deploy_ms: c.deploy_time_ms,
            spawn_tick: self.tick,
        });
        let i = id.index as usize;
        if self.lifetime_ms.len() <= i {
            self.lifetime_ms.resize(i + 1, None);
        }
        self.lifetime_ms[i] = if kind == EntityKind::Building { c.lifetime_ms } else { None };
        // A hiding building (or a spawner) with no deploy time at all is "deployed" now.
        if self.ents.deploy_ms[i] == 0 {
            self.on_deployed(i);
        }
        Ok(id)
    }

    /// THE MOMENT AN ENTITY'S DEPLOY TIME ENDS: the hide machinery takes its start
    /// state and a spawner is ACTIVATED. Called from phase_upkeep on the tick
    /// deploy_ms reaches 0, from `spawn_now` / phase_spawn for a zero deploy time,
    /// and from the scenario setup path, which skips the deploy timer.
    fn on_deployed(&mut self, i: usize) {
        self.hide_on_deployed(i);
        self.spawner_activate(i);
    }

    /// Entity `i`'s spawner block, if its card has one.
    #[inline]
    fn spawner_of(&self, i: usize) -> Option<SpawnerDef> {
        self.cfg.cards.get(self.ents.card[i]).spawner
    }

    /// ACTIVATION of a periodic spawner (calibration spawner.FIRST_WAVE and
    /// spawner.START_TIME_ORIGIN): the timer is loaded with SpawnStartTime -- less
    /// the card's DeployTime, floored at 0, under from_placement -- or, blank, with 0
    /// (the first unit is queued in this tick's Spawn phase if it has not run yet,
    /// else the next one's) or with one SpawnPauseTime; no wave is in progress.
    fn spawner_activate(&mut self, i: usize) {
        let Some(sp) = self.spawner_of(i) else { return };
        self.ents.spawn_wave_left[i] = 0;
        let c = &self.cfg.calib;
        self.ents.spawn_ms[i] = match (sp.start_time_ms, c.spawner_first_wave) {
            (Some(t), _) => match c.spawner_start_time_origin {
                StartTimeOrigin::FromActivation => t,
                StartTimeOrigin::FromPlacement => (t - self.cfg.cards.get(self.ents.card[i]).deploy_time_ms).max(0),
            },
            (None, FirstWave::AfterStartTimeOrImmediately) => 0,
            (None, FirstWave::AfterOnePause) => sp.pause_time_ms,
        };
    }

    /// Live units spawner `i` owns (entity.rs `spawned_by`), for SpawnLimit. A pure
    /// count over the entity arrays: order-free.
    fn owned_count(&self, i: usize) -> i32 {
        let id = self.ents.id_of(i);
        self.ents.live_indices().filter(|&j| self.ents.spawned_by[j] == Some(id)).count() as i32
    }

    /// WHERE SPAWNER `i` EMITS (calibration spawner.SPAWN_POINT): its centre plus a
    /// distance along the OWNER's forward axis -- SpawnRadius when the column is
    /// set, else its own collision radius -- or its centre. The contact law then
    /// slides a unit that overlaps the spawner's circle out of it, as it does every
    /// unit (move16402.rs separation, capped at 150 native per tick, deploying units
    /// included; under the trace-fitted arm collide.rs's static pass, in one tick).
    /// NOT READ (the data ships them; the ring arm of DEATH_SPAWN_LAYOUT and a
    /// flank spawn would need them): SpawnAngleShift (DarkWitch 90: the Bats on her
    /// flanks; BattleRam 180: the Barbarians behind), DeathSpawnPushback (Golem,
    /// LavaHound, DarkWitch true), DeathSpawnMinRadius (SkeletonContainer 100).
    fn spawn_point(&self, i: usize, sp: &SpawnerDef) -> Vec2 {
        let c = self.ents.pos[i];
        match self.cfg.calib.spawner_spawn_point {
            SpawnPoint::AtCentre => c,
            SpawnPoint::InFrontAtOwnRadius => {
                let d = sp.radius.unwrap_or(self.ents.radius[i]);
                Vec2::new(c.x, c.y + spell::forward_dy(self.ents.team[i]) * d)
            }
        }
    }

    /// THE SPAWNER PASS (Spawn phase, after the queue has drained): every periodic
    /// spawner past its deploy time ticks its timer by TICK_MS and, at <= 0, queues
    /// the unit(s) that are due. Decided from START-OF-PHASE state (the count a
    /// SpawnLimit compares against, the positions), collected, sorted by (team, the
    /// spawner's team_seq, unit index) and only then pushed, so the queue order --
    /// which is the materialisation order and therefore team_seq -- is canonical
    /// and slot-free.
    ///
    /// TIMER LAW (entity.rs `spawn_ms` / `spawn_wave_left`): with `left == 0` a
    /// wave begins (left = SpawnNumber); each emission takes one off and reloads
    /// the timer with SpawnInterval while the wave has units left, else with the
    /// pause (calibration spawner.PAUSE_ANCHOR). SpawnInterval 0 emits the whole
    /// wave in one pass, gridded around the spawn point (the same formation as a
    /// Goblin Barrel release); a timed wave's units each appear AT the point. At
    /// most one wave starts per pass; a reload that is already due (0) fires next
    /// tick. Under spawner.STUN_PAUSES_SPAWNER a stunned spawner skips the pass.
    ///
    /// TICK ALIGNMENT (tests/spawner.rs pins it): an activation at tick A with start
    /// time S materialises the first unit in tick A + max(1, ceil(S / TICK_MS)); a
    /// unit queued in tick E materialises in tick E + 1 (the one-tick latency of every
    /// PendingSpawn); with pause P the next wave's first unit follows the last one by
    /// ceil(P / TICK_MS) ticks; a SpawnInterval I separates a wave's units by
    /// ceil(I / TICK_MS) ticks. SpawnLimit (spawner.LIMIT_RULE = skip_unit_keep_cadence):
    /// a unit that would exceed the limit is skipped and the cadence keeps running.
    fn spawner_pass(&mut self) {
        #[cfg(clash_plant = "spawner_never_fires")]
        {
            // PLANT (regression): the earlier engine, whose loader never read
            // the Spawn* columns -- a hut is an inert building.
            return;
        }
        #[allow(unreachable_code)]
        let dt = self.cfg.calib.tick_ms;
        let stun_pauses = self.cfg.calib.spawner_stun_pauses;
        let cards = self.cfg.cards.clone();
        // (team, spawner team_seq, k, the spawn) and the per-spawner timer results.
        let mut emissions: Vec<(Team, u32, u32, PendingSpawn)> = Vec::new();
        let mut timers: Vec<(usize, i32, i32)> = Vec::new();
        for i in 0..self.ents.capacity() {
            let e = &self.ents;
            if !e.alive[i] || e.deploy_ms[i] > 0 {
                continue;
            }
            let Some(sp) = self.spawner_of(i) else { continue };
            if stun_pauses && e.stun_ms[i] > 0 {
                continue;
            }
            let mut ms = e.spawn_ms[i] - dt;
            let mut left = e.spawn_wave_left[i];
            if ms > 0 {
                timers.push((i, ms, left));
                continue;
            }
            let unit = cards.get(sp.unit);
            let level = cards.spawner_level(e.card[i], e.level[i]).expect("spawner level validated at deploy");
            let point = self.spawn_point(i, &sp);
            let grid = if sp.interval_ms == 0 { self.formation_points(e.team[i], sp.number, unit.collision_radius, unit.is_flying(), point) } else { self.formation_points(e.team[i], 1, unit.collision_radius, unit.is_flying(), point) };
            let mut room = sp.limit.map(|l| l - self.owned_count(i));
            let mut started = false;
            let mut k = 0u32;
            loop {
                if left == 0 {
                    if started {
                        break;
                    }
                    started = true;
                    left = sp.number;
                }
                let j = (sp.number - left).max(0) as usize;
                if room.map_or(true, |r| r > 0) {
                    room = room.map(|r| r - 1);
                    let pos = grid[j.min(grid.len() - 1)];
                    emissions.push((e.team[i], e.team_seq[i], k, PendingSpawn { team: e.team[i], card: sp.unit, level, pos, deploy_ms: None, owner: Some(e.id_of(i)) }));
                    k += 1;
                }
                left -= 1;
                ms = if left > 0 {
                    sp.interval_ms
                } else {
                    match self.cfg.calib.spawner_pause_anchor {
                        PauseAnchor::AfterLastUnit => sp.pause_time_ms,
                        PauseAnchor::AfterFirstUnit => (sp.pause_time_ms - (sp.number - 1) * sp.interval_ms).max(0),
                    }
                };
                if ms > 0 {
                    break;
                }
            }
            timers.push((i, ms, left));
        }
        for (i, ms, left) in timers {
            self.ents.spawn_ms[i] = ms;
            self.ents.spawn_wave_left[i] = left;
        }
        emissions.sort_by_key(|(t, seq, k, _)| (*t as u8, *seq, *k));
        self.spawn_queue.extend(emissions.into_iter().map(|(_, _, _, p)| p));
    }

    /// WHERE A DEATH SPAWN'S UNITS APPEAR (calibration spawner.DEATH_SPAWN_LAYOUT =
    /// engine_grid_within_radius): the engine formation grid around the death point
    /// in the OWNER's frame (`formation_grid`, the same code as a deploy and a
    /// release), each point pulled back onto `radius` when the grid reaches past it
    /// (a frame-free radial scaling: exact under the rotation), then water-ejected
    /// like a release. A zero radius stacks them on the point; the Move phase's
    /// coincident-push rule spreads them in the owner's frame.
    fn death_spawn_points(&self, team: Team, count: i32, unit_radius: i32, flying: bool, pos: Vec2, radius: i32) -> Vec<Vec2> {
        let arena = &self.cfg.arena;
        let r = radius.max(0) as i64;
        self.formation_grid(team, count, unit_radius, flying, pos)
            .into_iter()
            .map(|p| {
                let off = p.sub(pos);
                let d2 = off.len2();
                let q = if d2 > r * r {
                    let len = isqrt(d2).max(1);
                    pos.add(Vec2::new(((off.x as i64) * r / len) as i32, ((off.y as i64) * r / len) as i32))
                } else {
                    p
                };
                if flying || arena.is_passable_ground(q) {
                    q
                } else {
                    arena.nearest_passable_ground(q, team).unwrap_or(q)
                }
            })
            .collect()
    }

    /// Does entity `i`'s card hide (card.rs `HideDef`) -- and is it a building, the
    /// only kind the machinery runs for?
    #[inline]
    fn hides(&self, i: usize) -> Option<crate::card::HideDef> {
        if self.ents.kind[i] == EntityKind::Building {
            self.cfg.cards.get(self.ents.card[i]).hide
        } else {
            None
        }
    }

    /// THE MOMENT A HIDING BUILDING'S DEPLOY TIME ENDS (calibration hide.STARTS_HIDDEN):
    /// it goes under, or it stands up with a full hide countdown. Called from
    /// phase_upkeep on the tick the deploy timer reaches 0, from `spawn_now` for a
    /// card with no deploy time, and from the scenario setup path, which skips the
    /// deploy timer. During the deploy window it is `Up`: a deploying Tesla is
    /// targetable and damageable like a deploying Cannon, and cannot act because
    /// `deploy_ms > 0` already blocks every action.
    fn hide_on_deployed(&mut self, i: usize) {
        let Some(h) = self.hides(i) else { return };
        #[cfg(not(clash_plant = "tesla_always_up"))]
        let starts_hidden = self.cfg.calib.hide_starts_hidden;
        #[cfg(clash_plant = "tesla_always_up")]
        let starts_hidden = false; // PLANT (regression): the earlier engine, never under.
        if starts_hidden {
            self.ents.hide[i] = HideState::Hidden;
            self.ents.hide_ms[i] = 0;
        } else {
            self.ents.hide[i] = HideState::Up;
            self.ents.hide_ms[i] = h.hide_time_ms;
        }
    }

    // -----------------------------------------------------------------------
    // the loop

    /// Advance one logic tick by running TICK_PHASES in order.
    pub fn tick(&mut self) {
        if self.outcome.is_some() {
            return;
        }
        #[cfg(not(clash_plant = "phase_order"))]
        let phases = TICK_PHASES;
        #[cfg(clash_plant = "phase_order")]
        let phases = {
            // PLANT: attack before movement.
            let mut p = TICK_PHASES;
            p.swap(5, 6);
            p
        };
        #[cfg(clash_profile)]
        let mut k = 0usize;
        for phase in phases {
            if let Some(t) = self.phase_trace.as_mut() {
                t.push(phase);
            }
            #[cfg(clash_profile)]
            let t0 = std::time::Instant::now();
            match phase {
                Phase::Upkeep => self.phase_upkeep(),
                Phase::Status => self.phase_status(),
                Phase::Spawn => self.phase_spawn(),
                Phase::Target => self.phase_target(),
                Phase::Path => match self.cfg.path_model {
                    // The measured 2026 model has its own per-tick loop: a
                    // goal-first route, event-driven replans and a locomotion law
                    // with no fractional carry (path2026.rs); under the measured
                    // 16.402 search the whole movement update is the one
                    // measured there (phase_path16402).
                    PathModel::Oracle2026 if self.cfg.calib.path_search == PathSearch::Client16402 => self.phase_path16402(),
                    PathModel::Oracle2026 => self.phase_path_2026(),
                    _ => self.phase_path(),
                },
                Phase::Move => self.phase_move(),
                Phase::Attack => self.phase_attack(),
                Phase::Projectile => self.phase_projectile(),
                Phase::Resolve => self.phase_resolve(),
                Phase::Reap => self.phase_reap(),
                Phase::Judge => self.phase_judge(),
            }
            #[cfg(clash_profile)]
            {
                profile::add(k, t0.elapsed().as_nanos());
                k += 1;
            }
        }
        self.tick += 1;
    }

    fn elapsed_ms(&self, ticks: u32) -> i64 {
        (ticks as i64) * (self.cfg.calib.tick_ms as i64)
    }

    fn phase_upkeep(&mut self) {
        let c = &self.cfg.calib;
        let elapsed = self.elapsed_ms(self.tick);
        let regular = (c.regular_time_s as i64) * 1000;
        let double = self.overtime || regular - elapsed <= (c.mana_speed_up_remaining_s as i64) * 1000;
        let rate = self.mana_rate[usize::from(double)];
        let cap = (c.max_mana as i64) * self.mana_unit;
        for p in self.players.iter_mut() {
            p.mana = (p.mana + rate).min(cap);
        }
        let dt = c.tick_ms;
        let paused_by_stun = c.stun_pauses_deploy;
        for i in 0..self.ents.capacity() {
            if self.ents.alive[i] && self.ents.deploy_ms[i] > 0 && !(paused_by_stun && self.ents.stun_ms[i] > 0) {
                self.ents.deploy_ms[i] = (self.ents.deploy_ms[i] - dt).max(0);
                if self.ents.deploy_ms[i] == 0 {
                    self.on_deployed(i);
                }
            }
        }
    }

    /// Stun and slow timers tick down by one tick. Where this runs is calibration
    /// status.BUFF_EXPIRY_TICK_ALIGNMENT: in Resolve just before new stuns land
    /// (ceil_from_next_tick: a D-ms stun applied in tick N holds ticks N+1..N+ceil(D/dt))
    /// or at the start of Status (one_tick_short).
    fn tick_status_timers(&mut self) {
        let dt = self.cfg.calib.tick_ms;
        for i in 0..self.ents.capacity() {
            if self.ents.alive[i] {
                self.ents.stun_ms[i] = (self.ents.stun_ms[i] - dt).max(0);
                self.ents.slow_ms[i] = (self.ents.slow_ms[i] - dt).max(0);
            }
        }
    }

    fn phase_status(&mut self) {
        let dt = self.cfg.calib.tick_ms;
        #[cfg(not(clash_plant = "stun_decrement_at_status_start"))]
        let short = self.cfg.calib.buff_expiry == BuffExpiry::OneTickShort;
        #[cfg(clash_plant = "stun_decrement_at_status_start")]
        let short = true; // PLANT (regression): the shipped pre-spell decrement, one tick short.
        if short {
            self.tick_status_timers();
        }
        let lifetime_paused = self.cfg.calib.stun_pauses_building_lifetime;
        for i in 0..self.ents.capacity() {
            if !self.ents.alive[i] {
                continue;
            }
            if lifetime_paused && self.ents.stun_ms[i] > 0 {
                continue;
            }
            if let Some(Some(left)) = self.lifetime_ms.get_mut(i) {
                *left -= dt;
                if *left <= 0 {
                    // Expiry is a hit for everything it has, so it goes through
                    // the same buffer and death path as any other damage.
                    let amount = self.ents.hp[i].max(0).saturating_add(self.ents.shield[i].max(0)).max(1);
                    // ignores_hide: a Tesla whose time is up dies under ground too
                    // (combat.rs `resolve` drops every other hit on a Hidden building).
                    #[cfg(not(clash_plant = "expiry_respects_hide"))]
                    let ignores_hide = true;
                    #[cfg(clash_plant = "expiry_respects_hide")]
                    let ignores_hide = false; // PLANT: a hidden Tesla lives for ever.
                    self.dmg.hits.push(Hit { target: self.ents.id_of(i), amount, ignores_hide });
                    self.lifetime_ms[i] = None;
                }
            }
        }
        // KING_ACTIVATE_TIME_MS, reading chosen: the delay from the trigger
        // (king damaged, or a princess tower lost) to the king starting to
        // target. What it really delays is open in calibration.json.
        for t in 0..2 {
            let king_stunned = self.towers[t][0].is_some_and(|k| self.ents.is_alive(k) && self.ents.stun_ms[k.index as usize] > 0);
            if self.cfg.calib.stun_pauses_king_activation && king_stunned {
                continue;
            }
            if let Some(ms) = self.king_wake_ms[t].as_mut() {
                *ms += dt;
                if *ms >= self.cfg.calib.king_activate_time_ms {
                    self.king_active[t] = true;
                }
            }
        }
    }

    fn phase_spawn(&mut self) {
        #[allow(unused_mut)]
        let mut queue = std::mem::take(&mut self.spawn_queue);
        #[cfg(clash_plant = "unseeded_spawn_order")]
        {
            // PLANT (determinism gate): materialise pending spawns in the
            // iteration order of a std HashSet, whose SipHash keys are random per
            // instance. Slot assignment then differs between two runs of one seed.
            let order: std::collections::HashSet<usize> = (0..queue.len()).collect();
            let q = queue.clone();
            queue = order.into_iter().map(|k| q[k]).collect();
        }
        for p in queue {
            let kind = match self.cfg.cards.get(p.card).kind {
                CardKind::Building => EntityKind::Building,
                CardKind::Troop => EntityKind::Troop,
                CardKind::Spell => {
                    // An accepted cast becomes its spell objects now, in queue order --
                    // which is the canonical (team, slot) deploy order.
                    let cast = spell::cast(&self.cfg.cards, &self.cfg.calib, &self.cfg.arena, p.team, p.card, p.level, p.pos)
                        .expect("spell level validated at enqueue");
                    self.spells.extend(cast);
                    continue;
                }
            };
            let id = self.spawn_now(p.team, p.card, p.level, p.pos, kind).expect("level validated at enqueue");
            self.ents.spawned_by[id.index as usize] = p.owner;
            if let Some(d) = p.deploy_ms {
                self.ents.deploy_ms[id.index as usize] = d;
                if d == 0 {
                    self.on_deployed(id.index as usize);
                }
            }
        }
        self.hash.rebuild(&self.ents);
        // Periodic spawners emit into the (now empty) queue: next tick's units.
        self.spawner_pass();
    }

    /// THE HIDE STATE MACHINE (entity.rs `HideState`), one step per tick for every
    /// hiding building past its deploy time, from START-OF-PHASE state and before
    /// any entity decides its target, so every decision this tick sees one
    /// consistent hide state and a building that goes under is dropped by its
    /// attackers in the same phase (target.rs `can_target`).
    ///
    ///   Hidden --[targetable enemy in wake range (hide.RISE_TRIGGER)]--> Rising, timer = UpTimeMs
    ///   Rising --[timer -= TICK_MS; timer <= 0]--> Up, timer = HideTimeMs
    ///   Up     --[no live target at phase start (hide.HIDE_DELAY_MEANING = idle_time_without_target):
    ///             timer -= TICK_MS; timer <= 0]--> Hidden
    ///          --[live target]--> timer = HideTimeMs
    ///   Up     under time_since_last_shot: timer -= TICK_MS every tick, reset by phase_attack on
    ///          fire; Hidden when timer < 0 (strict; see the match arm for why).
    ///
    /// TICK ALIGNMENT (tests/hide.rs pins it): a building that enters Rising in tick
    /// R is Up in tick R + ceil(UpTimeMs / TICK_MS) and may target in that same
    /// Target phase; a building whose target is gone from the Target phase of tick T
    /// on is Hidden in tick T + ceil(HideTimeMs / TICK_MS) - 1 (the T-th observation
    /// counts). Timers are decremented HERE and nowhere else. Building lifetime keeps
    /// running in every state (phase_status; community, calibration hide.$comment).
    fn hide_pass(&mut self, nb: &mut Vec<u32>) {
        let dt = self.cfg.calib.tick_ms;
        let mut next: Vec<(usize, HideState, i32)> = Vec::new();
        {
            let ctx = TargetCtx {
                ents: &self.ents,
                hash: &self.hash,
                cards: &self.cfg.cards,
                arena: &self.cfg.arena,
                calib: &self.cfg.calib,
                reading: self.cfg.tower_sight_reading,
                towers: &self.towers,
                king_active: self.king_active,
            };
            let e = &self.ents;
            for i in 0..e.capacity() {
                if !e.alive[i] || e.deploy_ms[i] > 0 {
                    continue;
                }
                let Some(h) = self.hides(i) else { continue };
                #[cfg(clash_plant = "tesla_always_up")]
                {
                    // PLANT (regression): the earlier engine -- the building is
                    // simply up, whatever the timers say.
                    let _ = h;
                    next.push((i, HideState::Up, 0));
                    continue;
                }
                #[allow(unreachable_code)]
                match e.hide[i] {
                    HideState::Hidden => {
                        if target::enemy_in_wake_range(&ctx, i, nb) {
                            next.push((i, HideState::Rising, h.up_time_ms));
                        }
                    }
                    HideState::Rising => {
                        let left = e.hide_ms[i] - dt;
                        if left <= 0 {
                            next.push((i, HideState::Up, h.hide_time_ms));
                        } else {
                            next.push((i, HideState::Rising, left));
                        }
                    }
                    HideState::Up => {
                        let has_target = e.target[i].is_some_and(|t| e.is_alive(t) && e.hp[t.index as usize] > 0);
                        let left = e.hide_ms[i] - dt;
                        let (st, ms) = match self.cfg.calib.hide_delay_meaning {
                            // A live target re-arms the countdown; HideTimeMs of no target
                            // (the countdown REACHING 0) sends it under.
                            HideDelayMeaning::IdleTimeWithoutTarget if has_target => (HideState::Up, h.hide_time_ms),
                            HideDelayMeaning::IdleTimeWithoutTarget if left <= 0 => (HideState::Hidden, 0),
                            HideDelayMeaning::IdleTimeWithoutTarget => (HideState::Up, left),
                            // Counts from the last shot (phase_attack re-arms it on fire) and
                            // sends it under on the first Target phase MORE than HideTimeMs
                            // after it -- STRICT: a building whose HitSpeed equals its
                            // HideTimeMs (Tesla, 800 = 800) fires its next shot on the very
                            // tick the countdown reaches 0, and that shot re-arms it. With
                            // `<= 0` it would go under before every second shot.
                            HideDelayMeaning::TimeSinceLastShot if left < 0 => (HideState::Hidden, 0),
                            HideDelayMeaning::TimeSinceLastShot => (HideState::Up, left),
                        };
                        next.push((i, st, ms));
                    }
                }
            }
        }
        for (i, st, ms) in next {
            self.ents.hide[i] = st;
            self.ents.hide_ms[i] = ms;
        }
    }

    fn phase_target(&mut self) {
        self.hash.rebuild(&self.ents);
        let mut decisions = std::mem::take(&mut self.scratch.decisions);
        let mut nb = std::mem::take(&mut self.scratch.nb);
        decisions.clear();
        self.hide_pass(&mut nb);
        {
            let ctx = TargetCtx {
                ents: &self.ents,
                hash: &self.hash,
                cards: &self.cfg.cards,
                arena: &self.cfg.arena,
                calib: &self.cfg.calib,
                reading: self.cfg.tower_sight_reading,
                towers: &self.towers,
                king_active: self.king_active,
            };
            for i in 0..self.ents.capacity() {
                if self.ents.alive[i] && self.cfg.cards.get(self.ents.card[i]).hit_speed_ms > 0 {
                    decisions.push((i, target::decide(&ctx, i, &mut nb)));
                }
            }
        }
        let carry = self.cfg.calib.resume_retarget_windup == ResumeWindup::Carry;
        let retarget_resets_charge = self.cfg.calib.charge_reset_on_retarget;
        for &(i, d) in &decisions {
            let e = &mut self.ents;
            let changed = e.target[i] != d.target;
            // CHARGE (calibration charge.RESET_ON_RETARGET, shipped false): switching
            // from one LIVE target to a DIFFERENT one clears the charge and the run-up.
            // Acquiring a first target, or replacing a dead one, is not a switch: a
            // Prince whose Skeleton dies under it and who then picks the tower was not
            // distracted, and under `true` it would otherwise lose its charge to every
            // kill it makes.
            if retarget_resets_charge {
                let was = e.target[i].filter(|t| e.is_alive(*t));
                if was.is_some() && d.target.is_some() && was != d.target {
                    e.charged[i] = false;
                    e.charge_progress[i] = 0;
                }
            }
            if d.resumed {
                e.retarget_on_resume[i] = false;
            }
            // status.RESUME_RETARGET_WINDUP = carry: a paused windup follows the unit to
            // the target its resume rescan picked, instead of being cancelled.
            let carried = d.resumed && carry && !d.cancel_attack;
            if d.cancel_attack || (changed && e.attack_phase[i] == AttackPhase::Windup && !carried) {
                e.attack_phase[i] = AttackPhase::Idle;
                e.attack_ms[i] = 0;
                e.target_locked[i] = false;
            }
            e.target[i] = d.target;
        }
        self.scratch.decisions = decisions;
        self.scratch.nb = nb;
    }

    fn build_obstacles(&mut self) {
        let model = self.cfg.footprint_model;
        let arena = &self.cfg.arena;
        let [blue, red] = &mut self.scratch.obstacles;
        blue.clear();
        red.clear();
        for i in self.ents.live_indices() {
            let kind = self.ents.kind[i];
            if !kind.is_building() {
                continue;
            }
            let king_of = if kind == EntityKind::KingTower { Some(self.ents.team[i]) } else { None };
            let shape = arena.building_shape(model, self.ents.pos[i], self.ents.radius[i], king_of);
            let id = self.ents.id_of(i);
            // `key` and `ally` make path::obstacle_key a total order, so two
            // buildings stacked on one centre never fall back to this (slot) order.
            let key = yield_key(&self.ents, i);
            let owner = self.ents.team[i];
            let radius = self.ents.radius[i];
            blue.push(Obstacle { id, shape, radius, key, ally: owner == Team::Blue });
            red.push(Obstacle { id, shape: shape.rotated(arena.width, arena.height), radius, key, ally: owner == Team::Red });
        }
        // THE FRIENDLY-OCCLUDER EPOCH, per team: a hash of that team's OWN
        // buildings as they enter the path grid (calibration
        // pathfinding.OCCLUSION_MODEL). Replan trigger 2 is "a friendly building
        // enters the world" and it fires on the tick the entity first exists, with
        // zero lag (repath_Giant: command at 160, entity at 161, path replanned in
        // that same frame), so the engine compares this against the epoch each unit
        // planned under rather than watching for spawn events.
        //
        // STILL FRIENDLY-ONLY, THOUGH THE OCCLUDER SET IS NOT ANY MORE. The 16.402
        // cross-check moved the OCCLUSION set to both sides (path2026.rs
        // `CostField::for_mover`) and measured nothing at all about the replan
        // TRIGGER on live data -- "the repath trigger on live data" is item 15 of
        // its open list, and building REMOVAL is item 11 of the measurements'. So an
        // enemy building appearing changes what the next plan would return without
        // arming trigger 2 by itself; the goal-cell trigger usually replans anyway,
        // and inventing a trigger nothing measured would be worse than the gap.
        // Recorded in calibration pathfinding.REPLAN_TRIGGERS `limits`.
        //
        // Hashed in FRAME coordinates, over friendly obstacles only, so a unit and
        // its rotated twin compute the SAME epoch: the two lists are built in one
        // pass in slot order and each red entry is the rotation of its blue one.
        for (t, list) in [Team::Blue, Team::Red].into_iter().zip(self.scratch.obstacles.iter()) {
            let mut h = Fnv::new();
            for o in list.iter().filter(|o| o.ally) {
                let c = o.shape.center();
                h.i32(c.x);
                h.i32(c.y);
                h.i32(o.radius);
            }
            self.scratch.occluder_epoch[t as usize] = h.finish() as u32;
        }
    }

    /// THE 16.402 MOVEMENT UPDATE, as measured on client 16.402 (PATH_SEARCH =
    /// client16402; path16402.rs for the search and the replan gate, move16402.rs
    /// for the contact law; the evidence is in calibration.json).
    /// Ground troops are updated ONE AFTER THE OTHER in array order and
    /// each sees the units before it already moved: the
    /// `bodies` array is that view, and the resulting
    /// displacements are handed to `phase_move` as deltas so the engine's position
    /// write stays where it is. Per unit:
    ///   1. held (stunned, knocked back, frozen): nothing;
    ///   2. the replan gate: search when there is no path, the goal
    ///      cell moved, or the own side's occluder set changed (SAMEPATH retention);
    ///   3. avoidance scan, offset decay, separation scan;
    ///   4. the step toward the current waypoint's centre at the unit's speed -- 0 while
    ///      deploying, attacking or stomp-paused, in which case only the collision mean
    ///      moves it -- with the avoidance rotation, the position write and the
    ///      reached test that pops the waypoint and refreezes the segment direction.
    fn phase_path16402(&mut self) {
        use crate::fixed::SUBTILE_PER_MILLITILE as K;
        self.build_obstacles();
        let mut g = self.scratch.grid16402.take().unwrap_or_else(|| Grid16402::new(&self.cfg.arena, &self.cfg.calib));
        g.refresh(self.scratch.occluder_epoch, &self.scratch.obstacles[0]);
        let cap = self.ents.capacity();
        let mut deltas = std::mem::take(&mut self.scratch.deltas);
        deltas.clear();
        deltas.resize(cap, Vec2::default());
        let mut walk_step = std::mem::take(&mut self.scratch.walk_step);
        walk_step.clear();
        walk_step.resize(cap, 0);
        let mut routes = std::mem::take(&mut self.ents.route);
        let mut goals = std::mem::take(&mut self.ents.route_goal);
        let mut planned = std::mem::take(&mut self.ents.last_plan_tick);
        let mut segs = std::mem::take(&mut self.ents.seg_dir);
        let mut kticks = std::mem::take(&mut self.ents.move_ticks);
        let mut facing = std::mem::take(&mut self.ents.facing);
        let mut offsets = std::mem::take(&mut self.ents.avoid_offset);
        {
            let e = &self.ents;
            let arena = &self.cfg.arena;
            let calib = &self.cfg.calib;
            let ctx = TargetCtx {
                ents: e,
                hash: &self.hash,
                cards: &self.cfg.cards,
                arena,
                calib,
                reading: self.cfg.tower_sight_reading,
                towers: &self.towers,
                king_active: self.king_active,
            };
            // EVERY ENTITY AS THE CONTACT LAW SEES IT: every alive entity, troops and
            // buildings and towers, in native units. Positions are updated in place as
            // units move; `start_*` is the start-of-tick position the neighbour
            // grouping uses.
            let index = move16402::Index::new(arena.cols, arena.rows);
            let mut bodies: Vec<move16402::Body> = (0..cap)
                .map(|i| {
                    let alive = e.alive[i];
                    let (x, y) = (e.pos[i].x / K, e.pos[i].y / K);
                    let held = e.stun_ms[i] > 0 || e.knock_ms[i] > 0;
                    move16402::Body {
                        x,
                        y,
                        start_x: x,
                        start_y: y,
                        side: e.team[i] as u8,
                        r: if alive { e.radius[i] / K } else { 0 },
                        mass: move16402::loaded_mass(e.mass[i].unwrap_or(0), e.radius[i] / K),
                        air: e.flying[i],
                        mover: e.kind[i] == EntityKind::Troop,
                        alive,
                        collidable: alive && !held,
                        offset: offsets[i],
                        dir: (facing[i].x, facing[i].y),
                        // states 8/0/2/10 and a busy special attack zero the dot
                        // product; here: attacking or deploying units do not steer
                        // their neighbours by heading
                        heading_counts: e.deploy_ms[i] == 0 && e.attack_phase[i] == AttackPhase::Idle,
                    }
                })
                .collect();
            let is_water = |c: i32, r: i32| arena.cell_bits(c, r) & arena.bit_water != 0;
            let mut scratch: Vec<usize> = Vec::new();
            // The update order is CREATION order: on every frame pair of the live
            // corpus the units move in the order they were spawned. The engine's
            // slots are reused, so order by (spawn tick, slot) instead.
            let mut order: Vec<usize> = (0..cap).filter(|&i| e.alive[i] && e.kind[i] == EntityKind::Troop).collect();
            order.sort_by_key(|&i| (e.spawn_tick[i], i));
            for i in order {
                if e.stun_ms[i] > 0 || e.knock_ms[i] > 0 || e.attack_phase[i] == AttackPhase::Windup {
                    continue; // held: the freeze holds the whole unit (battle F sc5)
                }
                let card: &CardDef = self.cfg.cards.get(e.card[i]);
                let team = e.team[i];
                let deploying = e.deploy_ms[i] > 0;
                let flying = e.flying[i];
                let goal_id = e.target[i].filter(|t| e.is_alive(*t)).or_else(|| target::default_tower(&ctx, i));
                let epoch = self.scratch.occluder_epoch[team as usize];
                let actor = (bodies[i].x, bodies[i].y);
                // ---- 2. the replan gate (walking units with a target only)
                let mut attacking = false;
                let mut target_abs: Option<(i32, i32)> = None;
                let mut feasible = true;
                if let (false, Some(gid)) = (deploying, goal_id) {
                    let gi = gid.index as usize;
                    if target::in_attack_range(calib, e.pos[i], card.range, e.pos[gi], e.radius[gi]) {
                        // SPEC 5.3: the path is cleared on the transition to attacking
                        routes[i].clear();
                        goals[i] = None;
                        segs[i] = Vec2::default();
                        attacking = true;
                    } else {
                        let target = (e.pos[gi].x / K, e.pos[gi].y / K);
                        target_abs = Some(target);
                        let reach = (card.range + e.radius[i]) / K;
                        let goal_cell = path16402::choose_goal_cell(
                            &g.terrain,
                            &g.occ_cur,
                            actor,
                            target,
                            reach,
                            path2026::AVOID_BUILDINGS_16402,
                            g.costs.building,
                        );
                        let cell_v = goal_cell.map(|(c, r)| Vec2::new(c, r));
                        let due = routes[i].is_empty() || goals[i] != cell_v || planned[i] != epoch;
                        if due && flying {
                            // AIR: no search at all -- the list is the goal cell alone
                            // and the unit flies straight at it
                            routes[i] = match goal_cell {
                                Some((gc, gr)) => vec![arena.half_to_subtile_center(gc, gr)],
                                None => Vec::new(),
                            };
                            feasible = goal_cell.is_some();
                            segs[i] = Vec2::default();
                            goals[i] = cell_v;
                            planned[i] = epoch;
                        } else if due {
                            let only_occlusion = !routes[i].is_empty() && goals[i] == cell_v;
                            let cols = arena.cols;
                            let old_idx: Vec<i32> = routes[i]
                                .iter()
                                .map(|&p| {
                                    let (c, r) = arena.subtile_to_half(p);
                                    r * cols + c
                                })
                                .collect();
                            match goal_cell {
                                None => {
                                    feasible = false;
                                    routes[i].clear();
                                    segs[i] = Vec2::default();
                                }
                                Some((gc, gr)) => {
                                    let (sc, sr) = (actor.0 / path16402::CELL, actor.1 / path16402::CELL);
                                    if (sc, sr) == (gc, gr) {
                                        routes[i].clear();
                                        segs[i] = Vec2::default();
                                    } else {
                                        let terrain = &g.terrain;
                                        let occ = &g.occ_cur;
                                        let cost = |c: i32, r: i32| path16402::cell_cost(terrain, occ, c, r);
                                        let chain: Vec<i32> = g.pf.find_path(sc, sr, gc, gr, true, &cost).to_vec();
                                        if chain.is_empty() {
                                            feasible = false;
                                            routes[i].clear();
                                            segs[i] = Vec2::default();
                                        } else if only_occlusion
                                            && !path16402::path_touches_changed_occlusion(&g.occ_prev, &g.occ_cur, &old_idx, &chain)
                                        {
                                            // SAMEPATH: the change touched neither list
                                        } else {
                                            routes[i] = chain.iter().map(|&n| arena.half_to_subtile_center(n % cols, n / cols)).collect();
                                            segs[i] = Vec2::default();
                                        }
                                    }
                                }
                            }
                            goals[i] = cell_v;
                            planned[i] = epoch;
                        }
                    }
                }
                // ---- 3. the contact scans
                let mut con = move16402::Contact { acc: (0, 0), count: 0, offset: offsets[i] };
                let node_centre = |p: Vec2| (p.x / K, p.y / K);
                let waypoint = if routes[i].len() >= 2 { routes[i].last().map(|&p| node_centre(p)) } else { None };
                if !attacking {
                    // The scan runs for walking and deploying units, not for an
                    // attacking one: an attacking unit is masked out of it. A CHARGED
                    // unit skips lighter movers and every mover heading its way --
                    // `charged` is exactly that flag (charge_pass sets it at 10000
                    // permille) and is false for life on every card without a charge
                    // block.
                    let pop = move16402::avoidance_scan(&index, &bodies, i, &mut con, waypoint, e.charged[i], &mut scratch);
                    if pop {
                        routes[i].pop();
                        segs[i] = Vec2::default();
                    }
                }
                move16402::decay_offset(&mut con);
                move16402::separation_scan(&index, &bodies, i, &mut con, &mut scratch);
                // ---- 4. the step (move16402::move_towards)
                let paused = if !deploying && !attacking && !routes[i].is_empty() {
                    let k = kticks[i];
                    kticks[i] = k.saturating_add(1);
                    path2026::stomp_paused(calib.tick_ms, card.stop_movement_after_ms, card.wait_ms, k)
                } else {
                    false
                };
                // the native S through every buff in force -- `effective_speed` is
                // the one place a speed buff enters (the charge today), and it
                // returns `speed` itself for every card without a buff, so this arm
                // is unchanged for the whole contact corpus
                let native_speed = if paused { 0 } else { self.effective_speed(i) / K };
                let (aim, speed) = match routes[i].last() {
                    Some(&p) if !deploying && !attacking => (node_centre(p), native_speed),
                    None if !deploying && !attacking && feasible => match target_abs {
                        // THE DIRECT AIM: with an empty list and a target out of
                        // range the unit walks at the point `reach` away from the
                        // target on the line to itself (move16402::direct_aim)
                        Some(t) => (move16402::direct_aim(actor, t, (card.range + e.radius[i]) / K), native_speed),
                        None => (actor, 0),
                    },
                    _ => (actor, 0),
                };
                // the requested step `L = min(speed, dist, 250)` of move_towards, the
                // quantity the charge accumulator counts under the shipped arm --
                // NOT the displacement (charge_pass, ACCUMULATOR = client16402_progress_permille)
                walk_step[i] = speed.min(move16402::distance(actor.0, actor.1, aim.0, aim.1).max(1)).min(250);
                if segs[i] == Vec2::default() && routes[i].last().is_some() && speed > 0 {
                    // a new segment's direction is frozen from the position toward
                    // the last node when the segment starts
                    let s = move16402::segment_dir(actor.0, actor.1, node_centre(*routes[i].last().unwrap()));
                    segs[i] = Vec2::new(s.0, s.1);
                }
                let set_dir = speed > 0 || (aim != actor);
                let m = move16402::move_towards(
                    actor,
                    aim.0,
                    aim.1,
                    speed,
                    set_dir,
                    &mut con,
                    (segs[i].x, segs[i].y),
                    deploying,
                    is_water,
                    arena.cols,
                    arena.rows,
                );
                offsets[i] = con.offset;
                if let Some(d) = m.dir {
                    facing[i] = Vec2::new(d.0, d.1);
                    bodies[i].dir = d;
                }
                bodies[i].x = m.x;
                bodies[i].y = m.y;
                bodies[i].offset = con.offset;
                deltas[i] = Vec2::new(m.x * K, m.y * K).sub(e.pos[i]);
                // the reached test pops the last node and refreezes the segment
                if m.reached && !routes[i].is_empty() && speed > 0 {
                    routes[i].pop();
                    segs[i] = match routes[i].last() {
                        Some(&n) => {
                            let s = move16402::segment_dir(m.x, m.y, node_centre(n));
                            Vec2::new(s.0, s.1)
                        }
                        None => Vec2::default(),
                    };
                }
            }
        }
        self.ents.route = routes;
        self.ents.route_goal = goals;
        self.ents.last_plan_tick = planned;
        self.ents.seg_dir = segs;
        self.ents.move_ticks = kticks;
        self.ents.facing = facing;
        self.ents.avoid_offset = offsets;
        self.scratch.deltas = deltas;
        self.scratch.walk_step = walk_step;
        self.scratch.grid16402 = Some(g);
    }

    /// THE SUBTILE STEP ENTITY `i` TAKES ON A WALKING TICK: `ents.speed[i]` (the
    /// base, `Speed x time.SPEED_TO_SUBTILES_PER_TICK`, stomp-adjusted at spawn)
    /// through every speed buff in force. THE SINGLE HOOK FOR EVERY SPEED BUFF --
    /// the charge today (calibration charge.MULTIPLIER_MEANING; card.rs
    /// `ChargeDef`), rage and slow when they exist -- so a buff is one arm here and
    /// nothing else has to know. Both path models read it at every site that turns
    /// a speed into a step or sizes a probe by one; `ents.speed[i]` itself is never
    /// written (collide.rs PushModel::SpeedWeighted weights by the BASE speed, and
    /// no source says a charging Prince pushes harder).
    ///
    /// THE LAW IS MEASURED (calibration movement.BUFF_SPEED_RULE, live 16.402):
    /// S_buffed = floor(S x mult / 100) on the NATIVE S (the raw Speed column after
    /// the stomp rule), not on the subtile figure. Every stored speed is exactly
    /// S x spt, so `(speed / spt) x mult / 100 x spt` recovers S without loss
    /// (debug-asserted; tests/charge.rs pins it over every card), and for a unit
    /// with no buff in force the result is bit-identical to `speed` -- which is what
    /// keeps tests/oracle2026.rs, tests/mirror.rs and the contact gates exact.
    ///
    /// THE COMPOSITION THE STATUS PASS (rage, freeze, slow) IS TO IMPLEMENT HERE,
    /// in this order (the 16.402 model; a capture with two buffs of one sign in
    /// force on one unit is what would discriminate it from plain stacking):
    ///   1. a unit that cannot move -- a no-move tag, a dash cooldown, a post-attack
    ///      stop or a pushback still in flight -> 0; the walking states go on;
    ///   2. `maxpos = max(100, every positive SpeedMultiplier in force)`,
    ///      `maxneg = max(0, -(every negative one))`, zeros skipped;
    ///   3. `S' = tdiv(max(0, min(100, 100 - maxneg)) x tdiv(maxpos x S, 100), 100)`
    ///      (Rage 130 -> tdiv(130 x S, 100); Freeze -100 -> 0; a slow -35 -> 65 %;
    ///      buffs of one sign never stack, the largest wins);
    ///   4. then `x ChargeSpeedMultiplier / 100` when the charge progress is at
    ///      10000 permille -- the arm implemented below (calibration
    ///      charge.MULTIPLIER_MEANING).
    ///
    /// `tdiv` truncates toward zero; every step is on the native S and the result
    /// is turned into subtiles only at the very end, exactly as done below.
    #[inline]
    pub(crate) fn effective_speed(&self, i: usize) -> i32 {
        let base = self.ents.speed[i];
        let Some(ch) = self.cfg.cards.get(self.ents.card[i]).charge else { return base };
        let on = match self.cfg.calib.charge_multiplier_meaning {
            ChargeMultiplier::MovementWhenCharged => self.ents.charged[i],
            ChargeMultiplier::MovementAlways => true,
            ChargeMultiplier::AccumulationRate => false,
        };
        if !on {
            return base;
        }
        let spt = self.cfg.calib.speed_to_subtiles_per_tick.max(1);
        debug_assert_eq!(base % spt, 0, "a stored speed is S x SPEED_TO_SUBTILES_PER_TICK exactly");
        let native = (base / spt) as i64;
        ((native * (ch.speed_multiplier_percent as i64) / 100) as i32) * spt
    }

    /// The run-up threshold of entity `i`'s charge, in the ACCUMULATOR's own unit
    /// (subtiles, or ms under time_moving): ChargeRange through
    /// charge.CHARGE_RANGE_UNIT, divided by the multiplier under
    /// charge.MULTIPLIER_MEANING = accumulation_rate, and under time_moving turned
    /// into the ms an UNBUFFED unit of this card takes to walk it -- which makes
    /// time_moving identical to walk_delta_length for a free straight walk and
    /// different the moment the unit is blocked, pushed or walking a diagonal. That
    /// identity is the honest statement of what the shipped data can decide.
    fn charge_need(&self, i: usize, ch: ChargeDef) -> i32 {
        let c = &self.cfg.calib;
        let need = match (c.charge_accumulator, c.charge_range_unit) {
            // the 16.402 accumulator counts permille of the raw ChargeRange and
            // charges at 10000 (`progress <= 9999` keeps adding)
            (ChargeAccumulator::Client16402ProgressPermille, _) => 10_000,
            (_, ChargeRangeUnit::Centitiles) => crate::fixed::centi(ch.range_raw),
            (_, ChargeRangeUnit::Millitiles) => crate::fixed::milli(ch.range_raw),
        };
        let need = if c.charge_multiplier_meaning == ChargeMultiplier::AccumulationRate {
            ((need as i64) * 100 / (ch.speed_multiplier_percent.max(1) as i64)) as i32
        } else {
            need
        };
        let need = match c.charge_accumulator {
            ChargeAccumulator::TimeMoving => {
                let base = self.ents.speed[i].max(1) as i64;
                ((need as i64) * (c.tick_ms as i64) / base) as i32
            }
            _ => need,
        };
        need.max(1)
    }

    /// CHARGE ACCUMULATION (calibration charge.*; card.rs `ChargeDef`; the state is
    /// entity.rs `charge_progress` / `charged`). Runs at the END of the Move phase,
    /// after separation, once per tick, over every live troop whose card charges
    /// and is not charged yet. Each entity reads only its OWN proposed walk
    /// (`scratch.deltas`: what the Path phase asked for, locomotion only) and its
    /// OWN before/after positions (`scratch.pre` is captured AFTER the knockback
    /// slides, so a slide never counts as a walk); nothing here reads or writes
    /// another entity's charge, so the pass is order-independent by construction.
    ///
    /// A tick with no gain applies charge.PROGRESS_ON_STOP to the run-up; a tick
    /// with a gain adds it, and reaching the threshold (`charge_need`) sets
    /// `charged` and zeroes the run-up. `charged` itself is cleared only by the
    /// consumers named on the entity field, never here.
    ///
    /// MIRROR SAFETY BY CONSTRUCTION: every accumulator is a SCALAR of vectors that
    /// rotate together under the seat rotation -- a length, a dot product over a
    /// length, a tick count -- so no frame enters. "Progress along engine +y" is
    /// the one reading that would need `arena.to_frame`, and it is the seat bug in
    /// a new costume; it is not offered.
    fn charge_pass(&mut self) {
        use crate::fixed::SUBTILE_PER_MILLITILE as K;
        let c = &self.cfg.calib;
        let client16402 = self.arm16402();
        let cap = self.ents.capacity();
        let mut due: Vec<(usize, i32, i32)> = Vec::new();
        {
            let e = &self.ents;
            let ctx = TargetCtx {
                ents: e,
                hash: &self.hash,
                cards: &self.cfg.cards,
                arena: &self.cfg.arena,
                calib: c,
                reading: self.cfg.tower_sight_reading,
                towers: &self.towers,
                king_active: self.king_active,
            };
            for i in 0..cap {
                if !e.alive[i] || e.kind[i] != EntityKind::Troop || e.charged[i] {
                    continue;
                }
                let Some(ch) = self.cfg.cards.get(e.card[i]).charge else { continue };
                let walk = self.scratch.deltas.get(i).copied().unwrap_or_default();
                let pre = self.scratch.pre.get(i).copied().unwrap_or(e.pos[i]);
                let gain = match c.charge_accumulator {
                    ChargeAccumulator::Client16402ProgressPermille => {
                        // `L` is move_towards' requested step in native units: exact
                        // under the 16.402 arm (phase_path16402 records it), the
                        // proposed walk's length in native units under the other two
                        // path models (which have no requested-step notion)
                        let l = if client16402 { self.scratch.walk_step.get(i).copied().unwrap_or(0) } else { walk.len() / K };
                        move16402::tdiv(l.max(0).saturating_mul(1000), ch.range_raw.max(1))
                    }
                    ChargeAccumulator::WalkDeltaLength => walk.len(),
                    ChargeAccumulator::WalkDeltaTowardTarget => {
                        // The direction the Path phase steered by: the live target,
                        // else the default tower (the same resolution it used).
                        let goal = e.target[i].filter(|t| e.is_alive(*t)).or_else(|| target::default_tower(&ctx, i));
                        match goal {
                            Some(g) => {
                                let d = e.pos[g.index as usize].sub(pre);
                                let len = isqrt(d.len2());
                                if len == 0 {
                                    walk.len()
                                } else {
                                    let dot = (walk.x as i64) * (d.x as i64) + (walk.y as i64) * (d.y as i64);
                                    (dot / len).max(0) as i32
                                }
                            }
                            None => walk.len(),
                        }
                    }
                    ChargeAccumulator::NetMoveLength => e.pos[i].sub(pre).len(),
                    ChargeAccumulator::TimeMoving => {
                        if walk == Vec2::default() {
                            0
                        } else {
                            c.tick_ms
                        }
                    }
                };
                due.push((i, gain, self.charge_need(i, ch)));
            }
        }
        let stop = c.charge_progress_on_stop;
        for (i, gain, need) in due {
            let e = &mut self.ents;
            if gain <= 0 {
                if stop == ChargeStopRule::Reset {
                    e.charge_progress[i] = 0;
                }
                continue;
            }
            e.charge_progress[i] = e.charge_progress[i].saturating_add(gain);
            #[cfg(clash_plant = "charge_never_ready")]
            let need = i32::MAX; // PLANT (regression): the earlier engine, a Prince that never charges.
            if e.charge_progress[i] >= need {
                e.charged[i] = true;
                e.charge_progress[i] = 0;
            }
        }
    }

    /// THE 2026 PER-TICK UPDATE for ground troops (spec section 7; path2026.rs).
    ///
    /// The order is the measured one and permuting it does not reproduce the
    /// traces:
    ///   1. deploying / stunned / knocked / winding up -> nothing
    ///   2. re-derive the goal cell from the current target
    ///   3. goal cell moved, or the friendly-occluder set changed -> replan the
    ///      WHOLE remaining route (never a spliced local bypass)
    ///   4. stomp pause -> skip 5 and 6 (locomotion only; an external push still
    ///      moves the unit, which is why this gates the delta and not the write)
    ///   5. heading := norm256(centre(route.last()) - PRE-move position)
    ///   6. position += truncated step, remainder discarded
    ///   7. at most ONE waypoint consumed, on the POST-move position
    ///
    /// NOT MODELLED HERE: avoidance, crowd separation and combat pushback
    /// (calibration movement.CONTACT_DOMAIN). `path::avoid_units` is deliberately
    /// NOT applied -- it is this engine's own invention for the pre-2026 models,
    /// and the oracle's avoidance term is unmeasured (it sets `avoidance_offset` to
    /// +-190 and then walks it by +-10 per tick in a way no decay explains).
    /// Applying a made-up deflection here would corrupt the one law that IS
    /// measured.
    fn phase_path_2026(&mut self) {
        self.build_obstacles();
        let cap = self.ents.capacity();
        let mut deltas = std::mem::take(&mut self.scratch.deltas);
        deltas.clear();
        deltas.resize(cap, Vec2::default());
        let mut routes = std::mem::take(&mut self.ents.route);
        let mut goals = std::mem::take(&mut self.ents.route_goal);
        let mut planned = std::mem::take(&mut self.ents.last_plan_tick);
        let mut segs = std::mem::take(&mut self.ents.seg_dir);
        let mut kticks = std::mem::take(&mut self.ents.move_ticks);
        {
            let e = &self.ents;
            let arena = &self.cfg.arena;
            let calib = &self.cfg.calib;
            let ctx = TargetCtx {
                ents: e,
                hash: &self.hash,
                cards: &self.cfg.cards,
                arena,
                calib,
                reading: self.cfg.tower_sight_reading,
                towers: &self.towers,
                king_active: self.king_active,
            };
            for i in 0..cap {
                if !e.alive[i] || e.kind[i] != EntityKind::Troop || e.speed[i] <= 0 {
                    continue;
                }
                if e.deploy_ms[i] > 0 || e.stun_ms[i] > 0 || e.knock_ms[i] > 0 || e.attack_phase[i] == AttackPhase::Windup {
                    continue;
                }
                let card: &CardDef = self.cfg.cards.get(e.card[i]);
                let goal_id = match e.target[i].filter(|t| e.is_alive(*t)).or_else(|| target::default_tower(&ctx, i)) {
                    Some(g) => g,
                    None => continue,
                };
                let gi = goal_id.index as usize;
                if target::in_attack_range(calib, e.pos[i], card.range, e.pos[gi], e.radius[gi]) {
                    // SPEC 5.3: the path is cleared on the transition to attacking,
                    // without reaching the goal node. Over 46 304 oracle ticks not
                    // one has behavior_state == 2 with a live path -- the clear and
                    // the state change are the same event.
                    routes[i].clear();
                    goals[i] = None;
                    segs[i] = Vec2::default();
                    continue;
                }
                let team = e.team[i];
                let world = FrameWorld { arena, obstacles: &self.scratch.obstacles[team as usize] };
                let pos = arena.to_frame(team, e.pos[i]);
                let req = NavRequest {
                    #[cfg(clash_plant = "reflection_bridge_tie")]
                    red: team == Team::Red,
                    team,
                    pos,
                    goal: arena.to_frame(team, e.pos[gi]),
                    radius: e.radius[i],
                    sight: card.sight_range,
                    step: e.speed[i],
                    // SPEC 6.1: Range + the MOVER's own CollisionRadius, to the
                    // target's CENTRE. Not the sum of both radii (0/140) and not the
                    // footprint edge (0/140).
                    reach: card.range + e.radius[i],
                    flying: e.flying[i],
                    ignore: if e.kind[gi].is_building() { Some(goal_id) } else { None },
                };
                if req.flying {
                    // Air units do not use the grid at all: they fly to the target.
                    // UNMEASURED -- no flying unit appears in the oracle corpus.
                    // The route is stored in WORLD coordinates like every other
                    // route; only the arithmetic happens in the frame.
                    routes[i] = vec![arena.from_frame(team, req.goal)];
                    goals[i] = None;
                    segs[i] = Vec2::default();
                    let dir = path2026::norm256(req.goal.sub(pos));
                    let step = path2026::step_delta(e.speed[i], dir);
                    deltas[i] = arena.from_frame(team, pos.add(step)).sub(e.pos[i]);
                    continue;
                }
                let epoch = self.scratch.occluder_epoch[team as usize];
                // Did the search say a route EXISTS? Only meaningful on a tick that
                // planned -- and that is enough, because `due` is true whenever the
                // route is empty, so every tick that has to interpret an empty route
                // is a tick that just planned.
                let mut feasible = true;
                let cell = path2026::trigger_goal_cell(arena, pos, req.goal, e.radius[gi], req.reach);
                let cell_v = Vec2::new(cell.0, cell.1);
                let due = routes[i].is_empty() || goals[i] != Some(cell_v) || planned[i] != epoch;
                if due {
                    let (route, ok) = path2026::plan_waypoints(&world, calib, &req);
                    feasible = ok;
                    routes[i] = route.into_iter().map(|p| arena.from_frame(team, p)).collect();
                    goals[i] = Some(cell_v);
                    planned[i] = epoch;
                    // A new segment: its direction is frozen on the next heading
                    // (spec 7.7 freezes it at CONSUMPTION, and a replan assigns a
                    // node the same way a consumption does).
                    segs[i] = Vec2::default();
                }
                // CLOSING THE LAST GAP -- an engine-consistency fallback, NOT a
                // measured rule. The path stops at the first cell within
                // `Range + own CollisionRadius` of the target's centre (spec 6.1)
                // while this engine starts attacking within
                // `Range + TARGET CollisionRadius` (targeting.ADD_CHARACTER_RANGE_
                // TO_RADIUS). When the mover's radius is the bigger of the two -- a
                // Giant (750) walking up to a Cannon (600) -- the goal cell is
                // OUTSIDE attack range and the unit would stand there for ever.
                //
                // The oracle never shows this case because its own attack predicate
                // is wider than its goal rule: measured on the six walk traces, the
                // first attacking tick is at
                // `Range + own CollisionRadius + target CollisionRadius` of the
                // tower centre (Knight 2656 of 2700, Giant 2917 of 2950, Golem 2481
                // of 2500, HogRider 2397 of 2400, MiniPekka 2193 of 2250), so it is
                // always in range before it reaches the goal cell. Recorded in the
                // ledger under targeting.ADD_CHARACTER_RANGE_TO_RADIUS; until that
                // key is re-measured and promoted, this walks the remaining gap
                // straight at the target under the same locomotion law.
                //
                // IT APPLIES TO THE "ARRIVED" EMPTY ROUTE ONLY. An empty route also
                // means "no route exists" -- a unit walled in by its OWN buildings,
                // which is reachable: two friendly Cannons cover both bridge
                // corridors. Treating that as "arrived" walked the unit straight at
                // its target THROUGH cells its own cost field refuses, i.e. out of
                // the measured model altogether, for as long as the wall stood. A
                // unit with no route holds position instead, and starts moving again
                // the moment a replan trigger fires (the wall falls, or the goal
                // cell moves).
                let node = match routes[i].last() {
                    Some(&node_w) => arena.to_frame(team, node_w),
                    None if feasible => req.goal,
                    None => continue,
                };
                // SPEC 7.4: the stomp pause gates this unit's own locomotion. `k` is
                // its moving-tick index -- 0 on its first walking tick, and
                // advancing on the PAUSE ticks too, because the schedule is a
                // function of `k` and the pauses are what it selects.
                //
                // THE INCREMENT IS BEFORE THE PAUSE TEST ON PURPOSE, and the live
                // 16.402 corpus forces it: spec 7.4's wording is
                // ambiguous, and under the other reading -- `k` advancing only on
                // ticks that actually MOVE -- the counter sticks at the first pause
                // and the unit never moves again. With this reading the Ice Golem's
                // pauses are an exact period-11 progression in absolute tick number
                // (Stop 470 + Wait 80 = 550 ms = 11 ticks, runs of 10 moving ticks)
                // and the Giant's are period 740 ms with 13-tick runs.
                let k = kticks[i];
                kticks[i] = k.saturating_add(1);
                if path2026::stomp_paused(calib.tick_ms, card.stop_movement_after_ms, card.wait_ms, k) {
                    continue;
                }
                let dir = path2026::norm256(node.sub(pos));
                if segs[i].x == 0 && segs[i].y == 0 {
                    segs[i] = dir;
                }
                let step = path2026::step_delta(self.effective_speed(i), dir);
                let after = pos.add(step);
                deltas[i] = arena.from_frame(team, after).sub(e.pos[i]);
                // SPEC 7.5: at most ONE node per tick, on the POST-move position.
                if !routes[i].is_empty()
                    && path2026::arrived(calib.waypoint_arrive_rule, calib.waypoint_arrive_radius, node, after, segs[i])
                {
                    routes[i].pop();
                    // The next segment's direction is frozen HERE, from the POST-move
                    // position -- the engine's own path_segment_direction, exact on
                    // the walk traces with zero exceptions.
                    segs[i] = match routes[i].last() {
                        Some(&n) => path2026::norm256(arena.to_frame(team, n).sub(after)),
                        None => Vec2::default(),
                    };
                }
            }
        }
        self.ents.route = routes;
        self.ents.route_goal = goals;
        self.ents.last_plan_tick = planned;
        self.ents.seg_dir = segs;
        self.ents.move_ticks = kticks;
        self.scratch.deltas = deltas;
    }

    fn phase_path(&mut self) {
        self.build_obstacles();
        let cap = self.ents.capacity();
        let mut deltas = std::mem::take(&mut self.scratch.deltas);
        deltas.clear();
        deltas.resize(cap, Vec2::default());
        let mut routes = std::mem::take(&mut self.ents.route);
        let mut goals = std::mem::take(&mut self.ents.route_goal);
        let mut fracs = std::mem::take(&mut self.ents.move_frac);
        let mut planned = std::mem::take(&mut self.ents.last_plan_tick);
        let mut nb = std::mem::take(&mut self.scratch.nb);
        let mut blockers = std::mem::take(&mut self.scratch.blockers);
        {
            let e = &self.ents;
            let arena = &self.cfg.arena;
            let calib = &self.cfg.calib;
            let pf = path::pathfinder_for(self.cfg.path_model);
            let ctx = TargetCtx {
                ents: e,
                hash: &self.hash,
                cards: &self.cfg.cards,
                arena,
                calib,
                reading: self.cfg.tower_sight_reading,
                towers: &self.towers,
                king_active: self.king_active,
            };
            // None = the measured "no periodic replan" (calibration
            // pathfinding.REPATH_INTERVAL_TICKS). For these pre-2026 models that
            // leaves the route-empty and goal-moved triggers below, which is
            // strictly closer to the oracle than the old folklore 10-tick cadence.
            let repath = calib.repath_interval_ticks.map(|r| r.max(1) as u32);
            for i in 0..cap {
                if !e.alive[i] || e.kind[i] != EntityKind::Troop || e.speed[i] <= 0 {
                    continue;
                }
                if e.deploy_ms[i] > 0 || e.stun_ms[i] > 0 || e.knock_ms[i] > 0 || e.attack_phase[i] == AttackPhase::Windup {
                    continue;
                }
                let card: &CardDef = self.cfg.cards.get(e.card[i]);
                let goal_id = match e.target[i].filter(|t| e.is_alive(*t)).or_else(|| target::default_tower(&ctx, i)) {
                    Some(g) => g,
                    None => continue,
                };
                let gi = goal_id.index as usize;
                if target::in_attack_range(calib, e.pos[i], card.range, e.pos[gi], e.radius[gi]) {
                    routes[i].clear();
                    continue;
                }
                let team = e.team[i];
                let world = FrameWorld { arena, obstacles: &self.scratch.obstacles[team as usize] };
                let goal_w = e.pos[gi];
                // The step this unit takes THIS tick through every speed buff (the
                // charge): the probe `path::avoid_units` sizes by `req.step`, the
                // neighbour query that must cover the probe, and the distance walked
                // all read it. Leaving any of the three on the base speed makes a
                // charging Prince steer as though it were half as fast.
                let eff = self.effective_speed(i);
                let mut req = NavRequest {
                    #[cfg(clash_plant = "reflection_bridge_tie")]
                    red: team == Team::Red,
                    team,
                    pos: arena.to_frame(team, e.pos[i]),
                    goal: arena.to_frame(team, goal_w),
                    radius: e.radius[i],
                    sight: card.sight_range,
                    step: eff,
                    // Only PathModel::Oracle2026 reads `reach`; the three models
                    // below walk to the target's position and stop on the attack
                    // predicate instead of truncating the path.
                    reach: 0,
                    #[cfg(not(clash_plant = "air_uses_grid"))]
                    flying: e.flying[i],
                    #[cfg(clash_plant = "air_uses_grid")]
                    flying: false, // PLANT: air units planned as ground.
                    ignore: if e.kind[gi].is_building() { Some(goal_id) } else { None },
                };
                let moved_goal = goals[i].map_or(true, |g| g.dist2(goal_w) > (arena.cell as i64) * (arena.cell as i64));
                let due = routes[i].is_empty() || moved_goal || repath.is_some_and(|r| self.tick >= planned[i] + r);
                if due {
                    routes[i] = pf.plan(&world, &req).into_iter().map(|p| arena.from_frame(team, p)).collect();
                    goals[i] = Some(goal_w);
                    planned[i] = self.tick;
                } else if let Some(last) = routes[i].last_mut() {
                    // Between re-plans the final leg follows a moving target.
                    *last = goal_w;
                }
                // Troops this unit may have to walk around (path::avoid_units),
                // in its frame. Its own target is never a blocker: a unit walks
                // up to what it means to hit. Query radius covers the probe
                // (2 steps + clearance) plus both radii.
                let query = e.radius[i] * 2 + self.hash.max_radius() + eff * 2;
                self.hash.neighbours_within(e, e.pos[i], query, &mut nb);
                blockers.clear();
                for &j in nb.iter() {
                    let j = j as usize;
                    if j == i || e.kind[j] != EntityKind::Troop || e.flying[j] != e.flying[i] || Some(e.id_of(j)) == e.target[i] {
                        continue;
                    }
                    blockers.push(UnitBlocker {
                        pos: arena.to_frame(team, e.pos[j]),
                        radius: e.radius[j],
                        key: yield_key(e, j),
                        ally: e.team[j] == team,
                    });
                }
                let my_key = yield_key(e, i);
                // Follow the route. `move_frac` is carried in the TEAM FRAME.
                let mut amount = eff;
                let mut cur = req.pos;
                let mut frac = fracs[i];
                for _ in 0..4 {
                    let Some(&wp_w) = routes[i].first() else { break };
                    let wp = arena.to_frame(team, wp_w);
                    req.pos = cur;
                    let aim = pf.steer(&world, &req, wp);
                    let aim = path::avoid_units(&world, &req, my_key, aim, &blockers);
                    let (np, left) = path::advance(cur, aim, amount, &mut frac);
                    cur = np;
                    if np == wp && routes[i].len() > 1 {
                        routes[i].remove(0);
                        amount = left;
                        if amount > 0 {
                            continue;
                        }
                    }
                    break;
                }
                fracs[i] = frac;
                deltas[i] = arena.from_frame(team, cur).sub(e.pos[i]);
            }
        }
        self.scratch.nb = nb;
        self.scratch.blockers = blockers;
        self.ents.route = routes;
        self.ents.route_goal = goals;
        self.ents.move_frac = fracs;
        self.ents.last_plan_tick = planned;
        self.scratch.deltas = deltas;
    }

    /// Whether the Path phase is the measured 16.402 move update
    /// (`phase_path16402`: pathfinding.PATH_SEARCH = client16402 under the
    /// 2026 model), the shipped arm.
    #[inline]
    fn arm16402(&self) -> bool {
        self.cfg.path_model == PathModel::Oracle2026 && self.cfg.calib.path_search == PathSearch::Client16402
    }

    fn phase_move(&mut self) {
        self.step_knock_slides();
        // The charge accumulator's "before" (charge_pass): captured AFTER the slides,
        // so a knockback displacement never counts as a walk.
        self.scratch.pre.clear();
        self.scratch.pre.extend_from_slice(&self.ents.pos);
        let arena = &self.cfg.arena;
        let client16402 = self.arm16402();
        for i in 0..self.ents.capacity() {
            if !self.ents.alive[i] {
                continue;
            }
            let d = self.scratch.deltas.get(i).copied().unwrap_or_default();
            if d == Vec2::default() {
                continue;
            }
            let old = self.ents.pos[i];
            let new = Vec2::new((old.x + d.x).clamp(0, arena.width), (old.y + d.y).clamp(0, arena.height));
            // Under the 16.402 contact law the delta IS the measured position write
            // (move16402.rs grid_move, water edges included); nothing ejects a pushed
            // unit from the river, because the game does not either.
            self.ents.pos[i] = if self.ents.flying[i] || client16402 { new } else { collide::dry_position(arena, old, new) };
        }
        if !client16402 {
            // under the 16.402 contact law the separation impulse already
            // happened inside phase_path16402
            collide::separate(
                &mut self.ents,
                &mut self.hash,
                &self.cfg.arena,
                &self.scratch.obstacles[0],
                self.cfg.push_model,
                self.cfg.calib.separation_iterations,
                &mut self.scratch.collide,
            );
        }
        self.charge_pass();
    }

    fn phase_attack(&mut self) {
        let preserve = self.cfg.calib.preserve_target_if_hit_started;
        for i in 0..self.ents.capacity() {
            if !self.ents.alive[i] || self.cfg.cards.get(self.ents.card[i]).hit_speed_ms <= 0 {
                continue;
            }
            #[cfg(clash_plant = "inline_damage")]
            if self.ents.hp[i] <= 0 {
                // PLANT: units killed earlier in this same loop do not swing.
                continue;
            }
            let e = &self.ents;
            let held = e.stun_ms[i] > 0 || e.knock_ms[i] > 0;
            // Under ground or coming up: no attack (target.rs `decide` already gave it
            // no target; this keeps a windup from advancing under the
            // time_since_last_shot arm, where a building can go under mid-swing).
            let can_act = e.deploy_ms[i] == 0
                && !held
                && e.hide[i] == HideState::Up
                && (e.kind[i] != EntityKind::KingTower || self.king_active[e.team[i] as usize]);
            let step = combat::attack_step(e, &self.cfg.cards, &self.cfg.calib, i, can_act);
            self.ents.attack_phase[i] = step.phase;
            self.ents.attack_ms[i] = step.ms;
            // hide.HIDE_DELAY_MEANING = time_since_last_shot: a shot re-arms the hide
            // countdown (its own column, in its own iteration).
            if step.fired_at.is_some() && self.cfg.calib.hide_delay_meaning == HideDelayMeaning::TimeSinceLastShot {
                if let Some(h) = self.hides(i) {
                    self.ents.hide_ms[i] = h.hide_time_ms;
                }
            }
            // A stunned (or sliding) unit is NOT re-locked every tick: the stun released
            // its lock so it can retarget on resume (status.STUN_RETARGET_ON_RESUME).
            // Setting `target_locked = preserve && Windup` unconditionally instead
            // re-locks a stunned unit in Windup on every stunned tick
            // (docs/spell-spec.md, Zap step 9).
            #[cfg(clash_plant = "relock_while_stunned")]
            let held = false; // PLANT (regression): the ungated re-lock.
            if !held {
                self.ents.target_locked[i] = preserve && step.phase == AttackPhase::Windup;
            }
            if let Some(t) = step.fired_at {
                combat::fire(
                    &self.ents,
                    &self.hash,
                    &self.cfg.cards,
                    &self.cfg.calib,
                    i,
                    t,
                    &mut self.dmg,
                    &mut self.projectiles,
                    &mut self.scratch.nb,
                );
                // CHARGE (calibration charge.RESET_ON_ATTACK): the LANDED hit -- this
                // completed windup, not entering range and not a cancelled swing --
                // consumes the charge (`fire` read `charged` for its damage just above).
                // `charged` and `charge_progress` are two different things: the stop
                // rule in `charge_pass` touches only the latter.
                if self.cfg.calib.charge_reset_on_attack && self.ents.charged[i] {
                    self.ents.charged[i] = false;
                    self.ents.charge_progress[i] = 0;
                }
                #[cfg(clash_plant = "inline_damage")]
                for h in self.dmg.hits.drain(..) {
                    // PLANT: the predecessor's inline damage.
                    if self.ents.is_alive(h.target) {
                        self.ents.hp[h.target.index as usize] -= h.amount;
                    }
                }
            }
        }
    }

    fn phase_projectile(&mut self) {
        combat::step_projectiles(&self.ents, &self.hash, self.cfg.calib.crown_rounding, &mut self.projectiles, &mut self.dmg, &mut self.scratch.nb);
        if self.spells.is_empty() {
            return;
        }
        let mut released = Vec::new();
        {
            let ctx = spell::SpellCtx { ents: &self.ents, hash: &self.hash, cards: &self.cfg.cards, calib: &self.cfg.calib };
            spell::step_spells(&ctx, &mut self.spells, &mut self.dmg, &mut self.effects, &mut released, &mut self.scratch.nb);
        }
        for r in released {
            // Deferred like every spawn: the units materialise in the next Spawn phase.
            let unit = self.cfg.cards.get(r.unit);
            for p in self.formation_points(r.team, r.count, unit.collision_radius, unit.is_flying(), r.pos) {
                #[cfg(clash_plant = "barrel_instant_spawn")]
                {
                    // PLANT: released units appear on the landing tick, skipping the queue.
                    let id = self.spawn_now(r.team, r.unit, r.level, p, EntityKind::Troop).expect("plant spawn");
                    if let Some(d) = r.deploy_ms {
                        self.ents.deploy_ms[id.index as usize] = d;
                    }
                    continue;
                }
                #[allow(unreachable_code)]
                self.spawn_queue.push(PendingSpawn { team: r.team, card: r.unit, level: r.level, pos: p, deploy_ms: r.deploy_ms, owner: None });
            }
        }
    }

    /// Advance every knockback slide (knockback.DURATION_MS > 0) by one tick: an even
    /// share of the remaining displacement, the last tick taking the remainder.
    fn step_knock_slides(&mut self) {
        let dt = self.cfg.calib.tick_ms;
        let mut moved = false;
        for i in 0..self.ents.capacity() {
            if !self.ents.alive[i] || self.ents.knock_ms[i] <= 0 {
                continue;
            }
            let left = self.ents.knock_ms[i];
            let rem = self.ents.knock_rem[i];
            let step = if left <= dt {
                rem
            } else {
                Vec2::new(((rem.x as i64) * (dt as i64) / (left as i64)) as i32, ((rem.y as i64) * (dt as i64) / (left as i64)) as i32)
            };
            let old = self.ents.pos[i];
            let team = self.ents.team[i];
            self.ents.pos[i] =
                spell::settle(&self.cfg.arena, &self.scratch.obstacles[0], team, self.ents.radius[i], self.ents.flying[i], old, old.add(step));
            self.ents.knock_rem[i] = rem.sub(step);
            self.ents.knock_ms[i] = (left - dt).max(0);
            if self.ents.knock_ms[i] == 0 {
                self.ents.knock_rem[i] = Vec2::default();
            }
            moved = true;
        }
        if moved {
            self.hash.rebuild(&self.ents);
        }
    }

    /// Apply this tick's stun and knockback buffers to the entities that SURVIVED
    /// the damage pass. Both merges are commutative (max; vector sum), so the order the
    /// buffers were filled in cannot reach the battle.
    fn apply_effects(&mut self) {
        let fx = std::mem::take(&mut self.effects);
        if fx.knocks.is_empty() && fx.stuns.is_empty() {
            self.effects = fx;
            return;
        }
        let c = self.cfg.calib.clone();
        let cap = self.ents.capacity();
        // A HIDDEN building is out of reach of every effect too (knockback never moved
        // a building; a stun on it is a no-op).
        let survivor = |e: &Entities, id: EntityId| e.is_alive(id) && e.hp[id.index as usize] > 0 && e.hide[id.index as usize] != HideState::Hidden;
        // Stuns: max per target. Replace vs refresh decides only what the EXISTING timer
        // contributes; several new stuns in one tick always merge by max.
        let mut stun_new = vec![0i32; cap];
        for &(id, ms) in &fx.stuns {
            if survivor(&self.ents, id) {
                let i = id.index as usize;
                stun_new[i] = stun_new[i].max(ms);
            }
        }
        for (i, &ms) in stun_new.iter().enumerate() {
            if ms <= 0 {
                continue;
            }
            let e = &mut self.ents;
            #[cfg(not(clash_plant = "stun_replace"))]
            let reapply = c.same_buff_reapply;
            #[cfg(clash_plant = "stun_replace")]
            let reapply = BuffReapply::Replace; // PLANT: a short stun cuts a long one.
            e.stun_ms[i] = match reapply {
                BuffReapply::RefreshMax => e.stun_ms[i].max(ms),
                BuffReapply::Replace => ms,
            };
            if c.stun_retarget_on_resume {
                e.target_locked[i] = false;
                e.retarget_on_resume[i] = true;
            }
            #[cfg(not(clash_plant = "stun_resets_attack"))]
            let model = c.stun_attack_timer;
            #[cfg(clash_plant = "stun_resets_attack")]
            let model = StunTimerModel::Reset; // PLANT: crforge's pre-2017 reset.
            if model == StunTimerModel::Reset {
                e.attack_phase[i] = AttackPhase::Idle;
                e.attack_ms[i] = 0;
                e.target_locked[i] = false;
            }
            // CHARGE (calibration charge.RESET_ON_STUN): the stun clears the charge and
            // the run-up. Its own columns; nothing else in this loop reads them.
            if c.charge_reset_on_stun {
                e.charged[i] = false;
                e.charge_progress[i] = 0;
            }
        }
        // Knockbacks: sum per target, then one move per unit.
        let mut sum = vec![None::<Vec2>; cap];
        for &(id, d) in &fx.knocks {
            if survivor(&self.ents, id) {
                let s = sum[id.index as usize].get_or_insert(Vec2::default());
                #[cfg(not(clash_plant = "knock_last_wins"))]
                {
                    *s = s.add(d);
                }
                #[cfg(clash_plant = "knock_last_wins")]
                {
                    *s = d; // PLANT: the last buffered push replaces the others (buffer order matters).
                }
            }
        }
        let mut moved = false;
        for (i, s) in sum.iter().enumerate() {
            let Some(d) = *s else { continue };
            let e = &mut self.ents;
            #[cfg(not(clash_plant = "knockback_keeps_windup"))]
            let resets = e.attack_phase[i] == AttackPhase::Windup;
            #[cfg(clash_plant = "knockback_keeps_windup")]
            let resets = false; // PLANT: a push leaves the windup running.
            if resets {
                e.attack_phase[i] = AttackPhase::Idle;
                e.attack_ms[i] = 0;
                e.target_locked[i] = false;
            }
            // CHARGE (calibration charge.RESET_ON_KNOCKBACK): a push that LANDS clears
            // the charge and the run-up. A sum exists here only for a victim spell.rs
            // `pushable` accepted, and it refuses IgnorePushback without PushbackAll --
            // so a Fireball never reaches a Prince, DarkPrince or BattleRam (all three
            // ship IgnorePushback) and only The Log does. That asymmetry is a
            // PREDICTION of the shipped data, not a defect.
            if c.charge_reset_on_knockback {
                e.charged[i] = false;
                e.charge_progress[i] = 0;
            }
            if c.knock_attack_reset == KnockAttackReset::ResetWindupClearTarget {
                e.target[i] = None;
                e.target_locked[i] = false;
            }
            if c.knock_duration_ms > 0 {
                e.knock_rem[i] = e.knock_rem[i].add(d);
                e.knock_ms[i] = c.knock_duration_ms;
            } else {
                let old = e.pos[i];
                let team = e.team[i];
                e.pos[i] = spell::settle(&self.cfg.arena, &self.scratch.obstacles[0], team, e.radius[i], e.flying[i], old, old.add(d));
                moved = true;
            }
        }
        if moved {
            self.hash.rebuild(&self.ents);
        }
        let mut fx = fx;
        fx.knocks.clear();
        fx.stuns.clear();
        self.effects = fx;
    }

    fn phase_resolve(&mut self) {
        let out = combat::resolve(&mut self.ents, &mut self.dmg, &mut self.scratch.sums, self.cfg.calib.hide_hidden_immune);
        for t in 0..2 {
            if out.king_hit[t] && self.king_wake_ms[t].is_none() {
                self.king_wake_ms[t] = Some(0);
            }
        }
        self.death_queue = out.deaths;
        #[cfg(not(clash_plant = "stun_decrement_at_status_start"))]
        if self.cfg.calib.buff_expiry == BuffExpiry::CeilFromNextTick {
            self.tick_status_timers();
        }
        self.apply_effects();
    }

    fn phase_reap(&mut self) {
        let deaths = std::mem::take(&mut self.death_queue);
        // DEATH SPAWN (card.rs `DeathSpawnDef`): every death that reaches this queue --
        // hp <= 0 from any hit, the lifetime expiry hit included -- leaves its units
        // as PendingSpawns (they materialise in the NEXT tick's Spawn phase, like a
        // release), placed by `death_spawn_points` in the owner's frame, at the
        // owner's team and level, with the block's deploy time or the calibration
        // default. Collected and sorted by (team, the dead entity's team_seq, unit
        // index) before the push, so the queue order is canonical. Death damage
        // (below) lands as well: both effects, independently.
        let mut spawned: Vec<(Team, u32, u32, PendingSpawn)> = Vec::new();
        #[cfg(not(clash_plant = "death_spawn_dropped"))]
        for id in &deaths {
            let i = id.index as usize;
            let card = self.cfg.cards.get(self.ents.card[i]);
            let Some(ds) = card.death_spawn else { continue };
            let unit = self.cfg.cards.get(ds.unit);
            let level = self.cfg.cards.death_spawn_level(self.ents.card[i], self.ents.level[i]).expect("death spawn level validated at deploy");
            let radius = ds.radius.unwrap_or(match self.cfg.calib.death_spawn_radius_default {
                DeathSpawnRadius::OwnCollisionRadius => self.ents.radius[i],
                DeathSpawnRadius::Zero => 0,
            });
            let deploy_ms = ds.deploy_time_ms.or(match self.cfg.calib.death_spawn_deploy_default {
                DeathSpawnDeploy::UnitOwnDeployTime => None,
                DeathSpawnDeploy::Zero => Some(0),
            });
            let team = self.ents.team[i];
            for (k, p) in self.death_spawn_points(team, ds.count, unit.collision_radius, unit.is_flying(), self.ents.pos[i], radius).into_iter().enumerate() {
                spawned.push((team, self.ents.team_seq[i], k as u32, PendingSpawn { team, card: ds.unit, level, pos: p, deploy_ms, owner: None }));
            }
        }
        spawned.sort_by_key(|(t, seq, k, _)| (*t as u8, *seq, *k));
        self.spawn_queue.extend(spawned.into_iter().map(|(_, _, _, p)| p));
        for id in &deaths {
            let i = id.index as usize;
            let card = self.cfg.cards.get(self.ents.card[i]);
            if self.ents.death_damage[i] > 0 && card.death_damage_radius > 0 {
                combat::splash(
                    &self.ents,
                    &self.hash,
                    self.ents.team[i],
                    self.ents.pos[i],
                    card.death_damage_radius,
                    true,
                    true,
                    self.ents.death_damage[i],
                    card.crown_tower_damage_percent,
                    self.cfg.calib.crown_rounding,
                    &mut self.dmg,
                    &mut self.scratch.nb,
                );
            }
            let team = self.ents.team[i] as usize;
            if let Some(slot) = self.towers[team].iter().position(|t| *t == Some(*id)) {
                self.towers_down[team][slot] = true;
                if slot != 0 && self.king_wake_ms[team].is_none() {
                    self.king_wake_ms[team] = Some(0);
                }
            }
        }
        for id in deaths {
            self.ents.despawn(id);
        }
        self.hash.rebuild(&self.ents);
    }

    fn phase_judge(&mut self) {
        for t in 0..2 {
            let enemy = 1 - t;
            let down = self.towers_down[enemy];
            self.crowns[t] = if down[0] { 3 } else { u8::from(down[1]) + u8::from(down[2]) };
        }
        let c = &self.cfg.calib;
        let by_crowns = |cr: [u8; 2]| match cr[0].cmp(&cr[1]) {
            std::cmp::Ordering::Greater => Some(Outcome::Winner(Team::Blue)),
            std::cmp::Ordering::Less => Some(Outcome::Winner(Team::Red)),
            std::cmp::Ordering::Equal => None,
        };
        let king_down = self.towers_down[0][0] || self.towers_down[1][0];
        if king_down && c.three_crown_instant_win {
            self.outcome = Some(by_crowns(self.crowns).unwrap_or(Outcome::Draw));
            return;
        }
        let elapsed = self.elapsed_ms(self.tick + 1);
        let regular = (c.regular_time_s as i64) * 1000;
        let overtime_end = regular + (c.overtime_s as i64) * 1000;
        if self.overtime {
            if let Some(o) = by_crowns(self.crowns) {
                self.outcome = Some(o);
            } else if elapsed >= overtime_end {
                // The tie-break past overtime: calibration match.OVERTIME_TIEBREAK.
                self.outcome = Some(self.overtime_tiebreak());
            }
        } else if elapsed >= regular {
            if let Some(o) = by_crowns(self.crowns) {
                self.outcome = Some(o);
            } else if c.overtime_s > 0 {
                self.overtime = true;
            } else {
                self.outcome = Some(Outcome::Draw);
            }
        }
    }

    /// The verdict when overtime runs out with the crowns level (calibration
    /// match.OVERTIME_TIEBREAK). Each side's key is its WEAKEST standing crown
    /// tower; the side with the weaker weakest tower loses. Both keys are scalars
    /// computed in the side's own tower table, so nothing here depends on the
    /// frame and the rotation symmetry holds by construction (tests/mirror.rs).
    /// An exact tie is a Draw under every candidate; `none_draw` never looks.
    fn overtime_tiebreak(&self) -> Outcome {
        let rule = self.cfg.calib.overtime_tiebreak;
        if rule == OvertimeTiebreak::NoneDraw {
            return Outcome::Draw;
        }
        // (hp, max_hp) of the weakest alive crown tower per side. A side with no
        // standing tower cannot reach here (its king is down, a 3-crown win), so
        // an empty side is treated as the weakest possible.
        let weakest = |t: usize| -> Option<(i64, i64)> {
            self.towers[t]
                .iter()
                .flatten()
                .filter(|id| self.ents.is_alive(**id))
                .map(|id| {
                    let i = id.index as usize;
                    (self.ents.hp[i].max(0) as i64, self.ents.max_hp[i].max(1) as i64)
                })
                .min_by(|a, b| match rule {
                    OvertimeTiebreak::LowestTowerHpFraction => (a.0 * b.1).cmp(&(b.0 * a.1)),
                    _ => a.0.cmp(&b.0),
                })
        };
        let (blue, red) = (weakest(0), weakest(1));
        let ord = match (blue, red) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (Some(_), None) => std::cmp::Ordering::Greater,
            (Some(b), Some(r)) => match rule {
                OvertimeTiebreak::LowestTowerHpFraction => (b.0 * r.1).cmp(&(r.0 * b.1)),
                _ => b.0.cmp(&r.0),
            },
        };
        match ord {
            std::cmp::Ordering::Greater => Outcome::Winner(Team::Blue),
            std::cmp::Ordering::Less => Outcome::Winner(Team::Red),
            std::cmp::Ordering::Equal => Outcome::Draw,
        }
    }

    // -----------------------------------------------------------------------
    // actions

    /// The closed NoDeploySize rect of `team`'s crown tower `k` (0 king, 1 engine-
    /// Left, 2 engine-Right), or None if that tower is destroyed. Centred on the
    /// tower entity; size from its card (cards.json no_deploy_size_tiles).
    pub fn tower_no_deploy_rect(&self, team: Team, k: usize) -> Option<Rect> {
        let id = self.towers[team as usize].get(k).copied().flatten()?;
        if !self.ents.is_alive(id) {
            return None;
        }
        let i = id.index as usize;
        let size = self.cfg.cards.get(self.ents.card[i]).no_deploy_size?;
        Some(Arena::no_deploy_rect(self.ents.pos[i], size))
    }

    /// The rects a `team` troop may not be placed in: every ALIVE enemy crown tower's.
    fn enemy_no_deploy_rects(&self, team: Team) -> Vec<Rect> {
        (0..3).filter_map(|k| self.tower_no_deploy_rect(team.other(), k)).collect()
    }

    /// Positions for a multi-unit card: a centred grid, first row toward the
    /// enemy, spacing one collision diameter. FORMATION IS A GUESS (cards.json's
    /// summon_radius_milli is null for most cards).
    ///
    /// THE LAYOUT IS IN THE TEAM'S OWN FRAME: Red gets the ROTATED offset
    /// (-dx, -dy), so sibling k sits in the same own-frame place for both seats and
    /// team_seq follows own-left-to-right for both. A y-reflected offset
    /// (dx, -dy) instead makes sibling order follow ENGINE x for both teams, and is
    /// one of the reasons a policy shared by both seats desyncs on multi-unit
    /// deploys (80 of 144).
    fn formation(&self, team: Team, card: &CardDef, pos: Vec2) -> Vec<Vec2> {
        self.formation_grid(team, card.count, card.collision_radius, card.is_flying(), pos)
    }

    /// Units RELEASED by a landing spell (calibration spells.PROJECTILE_SPAWN_FORMATION
    /// = engine_grid): the troop formation around the landing point, with the unit's own
    /// radius. A point that is still not ground -- only possible when
    /// spells.SPAWNING_SPELL_WATER_RULE lets the spell land on water -- is ejected to
    /// the nearest land, so a ground unit never materialises on the river.
    fn formation_points(&self, team: Team, count: i32, radius: i32, flying: bool, pos: Vec2) -> Vec<Vec2> {
        let arena = &self.cfg.arena;
        #[cfg(clash_plant = "formation_ignores_water")]
        let flying = true; // PLANT: released units kept even on water.
        self.formation_grid(team, count, radius, flying, pos)
            .into_iter()
            .map(|p| if flying || arena.is_passable_ground(p) { p } else { arena.nearest_passable_ground(p, team).unwrap_or(p) })
            .collect()
    }

    fn formation_grid(&self, team: Team, count: i32, radius: i32, flying: bool, pos: Vec2) -> Vec<Vec2> {
        let n = count.max(1);
        if n == 1 {
            return vec![pos];
        }
        let mut cols = isqrt(n as i64) as i32;
        if cols * cols < n {
            cols += 1;
        }
        let rows = (n + cols - 1) / cols;
        let spacing = radius * 2;
        let arena = &self.cfg.arena;
        (0..n)
            .map(|k| {
                let row = k / cols;
                let col = k % cols;
                let in_row = cols.min(n - row * cols);
                let dx = (2 * col - (in_row - 1)) * spacing / 2;
                let dy = ((rows - 1) - 2 * row) * spacing / 2;
                let frame_off = Vec2::new(dx, dy);
                #[cfg(not(clash_plant = "reflection_formation"))]
                let off = match team {
                    Team::Blue => frame_off,
                    Team::Red => Vec2::new(-frame_off.x, -frame_off.y),
                };
                #[cfg(clash_plant = "reflection_formation")]
                let off = match team {
                    Team::Blue => frame_off,
                    Team::Red => Vec2::new(frame_off.x, -frame_off.y), // PLANT: the y-reflected offset.
                };
                let p = pos.add(off);
                let p = Vec2::new(p.x.clamp(0, arena.width), p.y.clamp(0, arena.height));
                #[cfg(clash_plant = "formation_ignores_water")]
                let flying = true; // PLANT: formation points kept even on water.
                if flying || arena.is_passable_ground(p) {
                    p
                } else {
                    pos
                }
            })
            .collect()
    }

    fn enqueue(&mut self, team: Team, idx: u16, level: i32, pos: Vec2) {
        let card = self.cfg.cards.get(idx).clone();
        if card.kind == CardKind::Spell {
            // One entry: the cast. phase_spawn turns it into spell objects.
            self.spawn_queue.push(PendingSpawn { team, card: idx, level, pos, deploy_ms: None, owner: None });
            return;
        }
        for p in self.formation(team, &card, pos) {
            self.spawn_queue.push(PendingSpawn { team, card: idx, level, pos: p, deploy_ms: None, owner: None });
        }
    }

    fn simulable(&self, name: &str) -> Result<u16, DeployError> {
        match self.cfg.cards.index(name) {
            Some(i) => Ok(i),
            None => match self.cfg.cards.rejected.iter().find(|(n, _)| n == name) {
                Some((n, why)) => Err(DeployError::UnsupportedCard(n.clone(), why.clone())),
                None => Err(DeployError::UnknownCard(name.to_string())),
            },
        }
    }

    /// Would a disc of radius `extra` at `pos` overlap or touch a building's
    /// footprint (closed, see `Shape::covers_disc`)? Pure; `deploy` and
    /// `check_deploy` share it so a query can never disagree with the command.
    /// `extra` is 0 for a troop (its centre may not be on a building) and the
    /// card's collision radius for a building (footprints may not touch).
    fn footprint_covers(&self, pos: Vec2, extra: i32) -> bool {
        let model = self.cfg.footprint_model;
        let arena = &self.cfg.arena;
        let e = &self.ents;
        #[cfg(clash_plant = "check_deploy_ignores_buildings")]
        return false;
        #[allow(unreachable_code)]
        e.live_indices().any(|i| {
            e.kind[i].is_building() && {
                let king_of = if e.kind[i] == EntityKind::KingTower { Some(e.team[i]) } else { None };
                arena.building_shape(model, e.pos[i], e.radius[i], king_of).covers_disc(pos, extra)
            }
        })
    }

    /// The position half of the verdict for card `idx`, shared by every deploy
    /// path. Territory by card kind: troops outside every alive enemy crown tower's
    /// NoDeploySize rect (and off the river band), buildings own half only
    /// (protocol.py Placement.TROOP / BUILDING; spells are not simulated, and
    /// `Anywhere` is wired for the day they are). No rolling-projectile card
    /// (The Log) is simulable yet; when one is, it takes the troop rule.
    ///
    /// FLYING TROOPS ARE REFUSED ON BUILDINGS TOO -- there is no air-unit exemption
    /// (`!card.is_flying() && footprint_covers(pos)`), and nothing sources one.
    /// Evidence (2018 data, so evidence not spec): the
    /// CanPlaceOnBuildings column of spells_*.csv is set on exactly two cards, Log
    /// and Tornado, and on none of the ten flying troops in spells_characters.csv
    /// (Minions, MinionHorde, Bats, MegaMinion, BabyDragon, InfernoDragon, Balloon,
    /// LavaHound, SkeletonBalloon, DartBarrell). protocol.py's mask already refused
    /// them, so this also removes an engine/mask disagreement.
    fn check_position(&self, team: Team, idx: u16, pos: Vec2) -> Result<(), DeployError> {
        let card = self.cfg.cards.get(idx);
        let (territory, footprint_rule) = deploy_rule(&self.cfg.calib, card);
        #[cfg(not(clash_plant = "territory_ignores_king_rect"))]
        let rects = self.enemy_no_deploy_rects(team);
        #[cfg(clash_plant = "territory_ignores_king_rect")]
        let rects: Vec<Rect> = (1..3).filter_map(|k| self.tower_no_deploy_rect(team.other(), k)).collect(); // PLANT
        self.cfg.arena.deploy_zone(pos, team, territory, &rects)?;
        let extra = if card.kind == CardKind::Building { card.collision_radius } else { 0 };
        #[cfg(clash_plant = "flying_ignores_buildings")]
        if card.is_flying() {
            // PLANT (regression): an air-unit exemption from the footprint check.
            return Ok(());
        }
        if footprint_rule && self.footprint_covers(pos, extra) {
            return Err(DeployError::Occupied);
        }
        Ok(())
    }

    /// Elixir half of the verdict.
    fn check_elixir(&self, team: Team, idx: u16) -> Result<(), DeployError> {
        let t = team as usize;
        let card = self.cfg.cards.get(idx);
        let need = (card.elixir as i64) * self.mana_unit;
        if self.players[t].mana < need {
            return Err(DeployError::NotEnoughElixir {
                have: (self.players[t].mana / self.mana_unit) as i32,
                need: card.elixir,
            });
        }
        Ok(())
    }

    /// PURE query: exactly the verdict `deploy` would give this play right now,
    /// with no mutation (the Python protocol's `check_deploy`). `deploy` calls
    /// this first, so the two cannot drift apart.
    pub fn check_deploy(&self, team: Team, card_name: &str, pos: Vec2) -> Result<(), DeployError> {
        if self.outcome.is_some() {
            return Err(DeployError::GameOver);
        }
        let idx = self.simulable(card_name)?;
        let slot = self.players[team as usize].hand.iter().position(|c| *c == idx).ok_or(DeployError::NotInHand)?;
        self.check_deploy_slot(team, slot, pos)
    }

    /// The card in a hand slot, or why there is none.
    pub fn hand_card(&self, team: Team, slot: usize) -> Result<u16, DeployError> {
        if slot >= HAND_SIZE {
            return Err(DeployError::BadSlot);
        }
        self.players[team as usize].hand.get(slot).copied().ok_or(DeployError::EmptySlot)
    }

    /// PURE query by hand slot (the protocol's `DeployCommand` names a slot,
    /// not a card name). Check order is protocol.py's: game over, slot, elixir,
    /// out of arena, water, no-deploy, territory, occupied.
    pub fn check_deploy_slot(&self, team: Team, slot: usize, pos: Vec2) -> Result<(), DeployError> {
        if self.outcome.is_some() {
            return Err(DeployError::GameOver);
        }
        let idx = self.hand_card(team, slot)?;
        self.check_elixir(team, idx)?;
        self.check_position(team, idx, pos)
    }

    /// Play a card from hand by name (the first slot holding it). Validation is
    /// immediate; the units materialise in the next tick's Spawn phase.
    pub fn deploy(&mut self, team: Team, card_name: &str, pos: Vec2) -> Result<(), DeployError> {
        self.check_deploy(team, card_name, pos)?;
        let idx = self.simulable(card_name)?;
        let slot = self.players[team as usize].hand.iter().position(|c| *c == idx).ok_or(DeployError::NotInHand)?;
        self.deploy_slot(team, slot, pos)
    }

    /// Play the card in `slot`. The played card goes to the back of the queue and
    /// the queue's front takes the slot.
    pub fn deploy_slot(&mut self, team: Team, slot: usize, pos: Vec2) -> Result<(), DeployError> {
        self.check_deploy_slot(team, slot, pos)?;
        let idx = self.hand_card(team, slot)?;
        let t = team as usize;
        let need = (self.cfg.cards.get(idx).elixir as i64) * self.mana_unit;
        let level = self.cfg.card_level[t];
        let p = &mut self.players[t];
        p.mana -= need;
        p.queue.push_back(idx);
        match p.queue.pop_front() {
            Some(next) => p.hand[slot] = next,
            None => {
                p.hand.remove(slot);
            }
        }
        self.enqueue(team, idx, level, pos);
        Ok(())
    }

    /// Scenario/test API: place a card's units ignoring hand, elixir and deploy
    /// zones (a ground unit still may not be placed on water). Materialises in
    /// the next Spawn phase like a deploy.
    pub fn spawn_unit(&mut self, team: Team, card_name: &str, pos: Vec2, level: Option<i32>) -> Result<(), DeployError> {
        if self.outcome.is_some() {
            return Err(DeployError::GameOver);
        }
        let idx = self.simulable(card_name)?;
        let level = level.unwrap_or(self.cfg.card_level[team as usize]);
        self.cfg.cards.check_levels(idx, level).map_err(DeployError::InvalidLevel)?;
        let card = self.cfg.cards.get(idx);
        if !self.cfg.arena.in_bounds(pos) {
            return Err(DeployError::OutOfArena);
        }
        if card.kind == CardKind::Spell {
            // A cast, at any in-bounds point (tests aim spells where no player could).
            self.enqueue(team, idx, level, pos);
            return Ok(());
        }
        if !card.is_flying() && !self.cfg.arena.is_passable_ground(pos) {
            return Err(DeployError::Water);
        }
        self.enqueue(team, idx, level, pos);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // scenario setup
    //
    // WHY IT EXISTS: the Python protocol's `reset(seed, MatchSetup)` can start a
    // battle mid-game (curriculum): a clock offset, elixir overrides, towers
    // damaged or already destroyed, units already on the board. These entry points are
    // that and nothing else; call them before the first tick. Each keeps the
    // derived fields consistent (crowns, king activation, the overtime flag), so
    // a set-up battle looks like one that got there by playing.

    /// Start the clock at `tick`. At or past regulation the battle is in overtime.
    pub fn scenario_set_tick(&mut self, tick: u32) {
        self.tick = tick;
        let c = &self.cfg.calib;
        self.overtime = c.overtime_s > 0 && self.elapsed_ms(tick) >= (c.regular_time_s as i64) * 1000;
    }

    /// Set a team's elixir in thousandths, clamped to [0, MAX_MANA] and floored to
    /// the internal unit.
    pub fn scenario_set_elixir_milli(&mut self, team: Team, milli: i64) {
        let c = &self.cfg.calib;
        let capped = milli.clamp(0, (c.max_mana as i64) * 1000);
        self.players[team as usize].mana = capped * self.mana_unit / 1000;
    }

    /// Set a crown tower's hp (`k`: 0 king, 1 engine-Left princess, 2 engine-Right --
    /// ENGINE lanes for both teams, so Red's k 1 is its own-RIGHT tower).
    /// hp <= 0 destroys a princess as if it fell before the battle began: the slot
    /// is freed, the crown counts, and the owner's king is ALREADY active (the
    /// activation delay elapsed in the past). A king cannot start destroyed.
    pub fn scenario_set_tower_hp(&mut self, team: Team, k: usize, hp: i32) -> Result<(), String> {
        let t = team as usize;
        let id = self.towers[t].get(k).copied().flatten().ok_or_else(|| format!("no tower slot {k}"))?;
        if hp > 0 {
            if !self.ents.is_alive(id) {
                return Err(format!("tower {team:?}/{k} is already destroyed"));
            }
            self.ents.hp[id.index as usize] = hp;
            return Ok(());
        }
        if k == 0 {
            return Err("a battle cannot start with a destroyed king tower".into());
        }
        if self.ents.despawn(id) {
            self.towers_down[t][k] = true;
            self.king_wake_ms[t] = Some(self.cfg.calib.king_activate_time_ms);
            self.king_active[t] = true;
            for side in 0..2 {
                let down = self.towers_down[1 - side];
                self.crowns[side] = if down[0] { 3 } else { u8::from(down[1]) + u8::from(down[2]) };
            }
            self.hash.rebuild(&self.ents);
        }
        Ok(())
    }

    /// Put ONE entity of a card on the board now, already deployed (no formation,
    /// no deploy timer), optionally at a given hp. Ignores hand, elixir and zones;
    /// a ground unit still may not stand on water.
    ///
    /// ONE AT A TIME, CALL ORDER DECIDES `team_seq`. For a whole MatchSetup use
    /// `scenario_spawn_batch`, which makes the order canonical.
    pub fn scenario_spawn_now(&mut self, team: Team, card_name: &str, pos: Vec2, hp: Option<i32>) -> Result<EntityId, DeployError> {
        let (idx, _) = self.setup_spawn_check(team, card_name, pos)?;
        let id = self.setup_spawn_place(team, idx, pos, hp)?;
        self.hash.rebuild(&self.ents);
        Ok(id)
    }

    /// Validate one setup spawn: (card index, the hp it would start with).
    fn setup_spawn_check(&self, team: Team, card_name: &str, pos: Vec2) -> Result<(u16, i32), DeployError> {
        let idx = self.simulable(card_name)?;
        let level = self.cfg.card_level[team as usize];
        let card = self.cfg.cards.get(idx);
        if card.kind == CardKind::Spell {
            return Err(DeployError::UnsupportedCard(card.name.clone(), "a spell is not a board unit; it cannot be a setup spawn".into()));
        }
        if !self.cfg.arena.in_bounds(pos) {
            return Err(DeployError::OutOfArena);
        }
        if !card.is_flying() && !self.cfg.arena.is_passable_ground(pos) {
            return Err(DeployError::Water);
        }
        // Level last: the order the one-at-a-time path always reported in (the
        // level used to fail inside spawn_now, after the position checks).
        let full_hp = self.cfg.cards.scaled(idx, level, card.hitpoints).map_err(DeployError::InvalidLevel)?;
        self.cfg.cards.check_levels(idx, level).map_err(DeployError::InvalidLevel)?;
        Ok((idx, full_hp))
    }

    /// Materialise one validated setup spawn (no hash rebuild).
    fn setup_spawn_place(&mut self, team: Team, idx: u16, pos: Vec2, hp: Option<i32>) -> Result<EntityId, DeployError> {
        let level = self.cfg.card_level[team as usize];
        let kind = if self.cfg.cards.get(idx).kind == CardKind::Building { EntityKind::Building } else { EntityKind::Troop };
        let id = self.spawn_now(team, idx, level, pos, kind).map_err(DeployError::InvalidLevel)?;
        let i = id.index as usize;
        self.ents.deploy_ms[i] = 0;
        self.on_deployed(i);
        if let Some(h) = hp {
            self.ents.hp[i] = h;
        }
        Ok(id)
    }

    /// Put a whole MatchSetup's spawns on the board now, in a CANONICAL order, so
    /// the order of the list cannot reach the battle.
    ///
    /// WHY IT EXISTS: `team_seq` -- the spawn ordinal within a team -- is the last
    /// component of every seat-invariant tie-break (target key, coincident push,
    /// `YieldKey`, `obstacle_key`). Spawning a setup in LIST order makes the list a
    /// hidden input: a rotation-mirrored MatchSetup whose Red spawns are listed in
    /// reverse (a full-hp Knight and a 300-hp Knight stacked on one point) desyncs
    /// at tick 1, because the coincident push sends the lower team_seq to its
    /// own-left.
    ///
    /// THE ORDER: every spec is validated first, in list order (the error names
    /// the first bad one by list index). Then they are spawned sorted by
    ///   (team, own-frame y, own-frame x, card NAME, starting hp)
    /// -- all rotation-invariant, and all expressible without engine indices (card
    /// name, not card index, so a second engine can reproduce it from the protocol
    /// alone). Specs equal on all of it are the same card at the same point with
    /// the same hp and team: interchangeable, so their relative order cannot matter.
    /// Sorting the teams apart also makes SLOT order list-independent, so the whole
    /// state (and `state_hash`) is a function of the spawn MULTISET.
    ///
    /// Returns ids in INPUT order. Plant: setup_spawn_list_order.
    pub fn scenario_spawn_batch(&mut self, spawns: &[(Team, &str, Vec2, Option<i32>)]) -> Result<Vec<EntityId>, (usize, DeployError)> {
        let mut checked = Vec::with_capacity(spawns.len());
        for (k, &(team, name, pos, hp)) in spawns.iter().enumerate() {
            let (idx, full_hp) = self.setup_spawn_check(team, name, pos).map_err(|e| (k, e))?;
            checked.push((idx, hp.unwrap_or(full_hp)));
        }
        #[allow(unused_mut)]
        let mut order: Vec<usize> = (0..spawns.len()).collect();
        #[cfg(not(clash_plant = "setup_spawn_list_order"))]
        {
            let arena = &self.cfg.arena;
            #[cfg(clash_plant = "setup_spawn_team_list_order")]
            let first_red = spawns.iter().position(|s| s.0 == Team::Red).unwrap_or(0);
            #[cfg(clash_plant = "setup_spawn_team_list_order")]
            let first_blue = spawns.iter().position(|s| s.0 == Team::Blue).unwrap_or(0);
            order.sort_by(|&a, &b| {
                let key = |k: usize| {
                    let (team, name, pos, _) = spawns[k];
                    let f = arena.to_frame(team, pos);
                    #[cfg(not(clash_plant = "setup_spawn_team_list_order"))]
                    let team_rank = team as u8 as usize;
                    // PLANT: teams in list order of first appearance -- team_seq stays
                    // canonical, but SLOT order (and so state_hash) follows the list.
                    #[cfg(clash_plant = "setup_spawn_team_list_order")]
                    let team_rank = if team == Team::Red { first_red } else { first_blue };
                    (team_rank, f.y, f.x, name, checked[k].1)
                };
                key(a).cmp(&key(b))
            });
        }
        let mut ids = vec![EntityId::default(); spawns.len()];
        for k in order {
            let (team, _, pos, hp) = spawns[k];
            ids[k] = self.setup_spawn_place(team, checked[k].0, pos, hp).map_err(|e| (k, e))?;
        }
        self.hash.rebuild(&self.ents);
        Ok(ids)
    }

    // -----------------------------------------------------------------------
    // observation accessors

    pub fn tick_count(&self) -> u32 {
        self.tick
    }
    pub fn time_ms(&self) -> i64 {
        self.elapsed_ms(self.tick)
    }
    pub fn crowns(&self) -> [u8; 2] {
        self.crowns
    }
    pub fn is_done(&self) -> bool {
        self.outcome.is_some()
    }
    pub fn outcome(&self) -> Option<Outcome> {
        self.outcome
    }
    pub fn winner(&self) -> Option<Team> {
        match self.outcome {
            Some(Outcome::Winner(t)) => Some(t),
            _ => None,
        }
    }
    pub fn is_overtime(&self) -> bool {
        self.overtime
    }
    pub fn config(&self) -> &BattleConfig {
        &self.cfg
    }
    pub fn arena(&self) -> &Arena {
        &self.cfg.arena
    }
    pub fn cards(&self) -> &CardDb {
        &self.cfg.cards
    }
    /// Whole elixir.
    pub fn elixir(&self, team: Team) -> i32 {
        (self.players[team as usize].mana / self.mana_unit) as i32
    }
    /// (raw mana, units per elixir) for fractional observations.
    pub fn elixir_raw(&self, team: Team) -> (i64, i64) {
        (self.players[team as usize].mana, self.mana_unit)
    }
    pub fn hand(&self, team: Team) -> Vec<&str> {
        self.players[team as usize].hand.iter().map(|i| self.cfg.cards.get(*i).name.as_str()).collect()
    }
    pub fn next_card(&self, team: Team) -> Option<&str> {
        self.players[team as usize].queue.front().map(|i| self.cfg.cards.get(*i).name.as_str())
    }
    /// The whole cycle queue (CardDb indices, front first).
    pub fn queue_cards(&self, team: Team) -> Vec<u16> {
        self.players[team as usize].queue.iter().copied().collect()
    }
    /// Deploys accepted but not yet materialised: (team, CardDb index, position).
    pub fn pending_spawns(&self) -> Vec<(Team, u16, Vec2)> {
        self.spawn_queue.iter().map(|p| (p.team, p.card, p.pos)).collect()
    }
    /// [king, left princess, right princess]; 0 once destroyed.
    pub fn tower_hp(&self, team: Team) -> [i32; 3] {
        let mut out = [0; 3];
        for (k, t) in self.towers[team as usize].iter().enumerate() {
            if let Some(id) = t {
                if self.ents.is_alive(*id) {
                    out[k] = self.ents.hp[id.index as usize].max(0);
                }
            }
        }
        out
    }
    pub fn tower_ids(&self, team: Team) -> [Option<EntityId>; 3] {
        self.towers[team as usize]
    }
    pub fn king_active(&self, team: Team) -> bool {
        self.king_active[team as usize]
    }
    pub fn projectiles(&self) -> &[Projectile] {
        &self.projectiles
    }
    /// Live spell objects (in flight, rolling, or an area effect about to apply).
    pub fn spells(&self) -> &[Spell] {
        &self.spells
    }
    pub fn entity(&self, id: EntityId) -> Option<EntityView<'_>> {
        if !self.ents.is_alive(id) {
            return None;
        }
        Some(self.view(id.index as usize))
    }
    fn view(&self, i: usize) -> EntityView<'_> {
        let e = &self.ents;
        EntityView {
            id: e.id_of(i),
            team: e.team[i],
            kind: e.kind[i],
            card: &self.cfg.cards.get(e.card[i]).name,
            card_idx: e.card[i],
            pos: e.pos[i],
            hp: e.hp[i],
            max_hp: e.max_hp[i],
            shield: e.shield[i],
            radius: e.radius[i],
            flying: e.flying[i],
            deploying: e.deploy_ms[i] > 0,
            target: e.target[i],
            attack_phase: e.attack_phase[i],
            team_seq: e.team_seq[i],
            attack_ms: e.attack_ms[i],
            deploy_ms: e.deploy_ms[i],
            target_locked: e.target_locked[i],
            speed: e.speed[i],
            move_frac: e.move_frac[i],
            route: &e.route[i],
            stun_ms: e.stun_ms[i],
            retarget_on_resume: e.retarget_on_resume[i],
            knock_ms: e.knock_ms[i],
            knock_rem: e.knock_rem[i],
            hide_state: e.hide[i],
            hide_ms: e.hide_ms[i],
            hidden: e.hide[i] == HideState::Hidden,
            spawn_ms: e.spawn_ms[i],
            spawn_wave_left: e.spawn_wave_left[i],
            spawned_by: e.spawned_by[i],
            charged: e.charged[i],
            charge_progress: e.charge_progress[i],
            effective_speed: self.effective_speed(i),
        }
    }
    /// Live entities in slot order.
    pub fn entities(&self) -> impl Iterator<Item = EntityView<'_>> + '_ {
        self.ents.live_indices().map(move |i| self.view(i))
    }
    pub fn live_count(&self) -> usize {
        self.ents.live_count()
    }

    /// Record the phases each tick runs (for the phase-order test).
    pub fn set_phase_trace(&mut self, on: bool) {
        self.phase_trace = if on { Some(Vec::new()) } else { None };
    }
    pub fn phase_trace(&self) -> Option<&[Phase]> {
        self.phase_trace.as_deref()
    }

    /// Test/scenario entry point: overwrite an entity's hp.
    pub fn debug_set_hp(&mut self, id: EntityId, hp: i32) -> bool {
        if !self.ents.is_alive(id) {
            return false;
        }
        self.ents.hp[id.index as usize] = hp;
        true
    }

    /// Test/scenario entry point: overwrite an entity's charge state (entity.rs
    /// `charge_progress` / `charged`), for the hash-sensitivity gate and for
    /// scenarios that start with a charged unit.
    pub fn debug_set_charge(&mut self, id: EntityId, progress: i32, charged: bool) -> bool {
        if !self.ents.is_alive(id) {
            return false;
        }
        let i = id.index as usize;
        self.ents.charge_progress[i] = progress;
        self.ents.charged[i] = charged;
        true
    }

    /// Test/scenario entry point: teleport an entity. No water or footprint check --
    /// the next Move phase resolves whatever this creates, which is the point
    /// when a test uses it to perturb a state by one subtile.
    pub fn debug_set_pos(&mut self, id: EntityId, pos: Vec2) -> bool {
        if !self.ents.is_alive(id) {
            return false;
        }
        self.ents.pos[id.index as usize] = pos;
        self.hash.rebuild(&self.ents);
        true
    }

    // -----------------------------------------------------------------------
    // hashing

    /// 64-bit FNV-1a over every piece of simulation-relevant state, including
    /// the Rng bit-state, the allocator, pending queues and the selected
    /// strategy models. Scratch buffers and the phase trace are excluded: they
    /// are rebuilt from state and cannot change an outcome.
    pub fn state_hash(&self) -> u64 {
        self.hash_state(false)
    }

    /// `state_hash`, or with `legacy_v3` the hash SNAPSHOT_FORMAT 3 computed: the
    /// same byte stream without the fields format 4 added (per-entity
    /// retarget_on_resume / knock_rem / knock_ms, the spell list, the effect buffers,
    /// PendingSpawn.deploy_ms). Exists only so a migrated format-3 snapshot can be
    /// self-checked against the hash it was saved with (`load_with`); every field it
    /// skips is proven neutral by that check passing.
    fn hash_state(&self, legacy_v3: bool) -> u64 {
        let mut h = Fnv::new();
        h.u32(self.tick);
        h.bytes(&self.crowns);
        for row in &self.towers_down {
            for d in row {
                h.bool(*d);
            }
        }
        for row in &self.towers {
            for t in row {
                h.opt_id(*t);
            }
        }
        for t in 0..2 {
            h.i32(self.king_wake_ms[t].unwrap_or(-1));
            h.bool(self.king_active[t]);
        }
        h.bool(self.overtime);
        h.u32(match self.outcome {
            None => 0,
            Some(Outcome::Draw) => 1,
            Some(Outcome::Winner(Team::Blue)) => 2,
            Some(Outcome::Winner(Team::Red)) => 3,
        });
        #[cfg(not(clash_plant = "hash_skips_rng"))]
        h.bytes(serde_json::to_string(&self.rng).expect("rng serializes").as_bytes());
        h.u32(self.cfg.path_model as u32);
        h.u32(self.cfg.push_model as u32);
        h.u32(self.cfg.footprint_model as u32);
        h.u32(self.cfg.tower_sight_reading as u32);
        for p in &self.players {
            h.i64(p.mana);
            h.u32(p.hand.len() as u32);
            for c in &p.hand {
                h.u32(*c as u32);
            }
            h.u32(p.queue.len() as u32);
            for c in &p.queue {
                h.u32(*c as u32);
            }
        }
        let e = &self.ents;
        let (free, counters) = e.allocator_state();
        h.u32(free.len() as u32);
        for f in free {
            h.u32(*f);
        }
        h.u32(counters[0]);
        h.u32(counters[1]);
        h.u32(e.capacity() as u32);
        for i in 0..e.capacity() {
            h.u32(e.generation[i]);
            h.bool(e.alive[i]);
            if !e.alive[i] {
                continue;
            }
            h.u32(e.team[i] as u32);
            h.u32(e.kind[i] as u32);
            h.u32(e.card[i] as u32);
            h.i32(e.level[i]);
            h.u32(e.team_seq[i]);
            h.u32(e.spawn_tick[i]);
            h.vec(e.pos[i]);
            h.i32(e.hp[i]);
            h.i32(e.max_hp[i]);
            h.i32(e.shield[i]);
            h.i32(e.damage[i]);
            h.i32(e.death_damage[i]);
            h.i32(e.radius[i]);
            h.i32(e.mass[i].unwrap_or(-1));
            h.i32(e.speed[i]);
            h.bool(e.flying[i]);
            h.opt_id(e.target[i]);
            h.bool(e.target_locked[i]);
            h.u32(e.attack_phase[i] as u32);
            h.i32(e.attack_ms[i]);
            h.i32(e.deploy_ms[i]);
            h.i32(e.stun_ms[i]);
            h.i32(e.slow_ms[i]);
            if !legacy_v3 {
                h.bool(e.retarget_on_resume[i]);
                h.vec(e.knock_rem[i]);
                h.i32(e.knock_ms[i]);
                h.vec(e.seg_dir[i]);
                h.u32(e.move_ticks[i]);
                h.u32(e.hide[i] as u32);
                h.i32(e.hide_ms[i]);
                h.i32(e.spawn_ms[i]);
                h.i32(e.spawn_wave_left[i]);
                h.opt_id(e.spawned_by[i]);
                h.i32(e.charge_progress[i]);
                h.bool(e.charged[i]);
                h.vec(e.facing[i]);
                h.i32(e.avoid_offset[i]);
            }
            h.vec(e.move_frac[i]);
            h.u32(e.route[i].len() as u32);
            for p in &e.route[i] {
                h.vec(*p);
            }
            match e.route_goal[i] {
                Some(g) => {
                    h.bool(true);
                    h.vec(g);
                }
                None => h.bool(false),
            }
            h.u32(e.last_plan_tick[i]);
            h.i32(self.lifetime_ms.get(i).copied().flatten().unwrap_or(-1));
        }
        h.u32(self.projectiles.len() as u32);
        for p in &self.projectiles {
            h.u32(p.team as u32);
            h.vec(p.pos);
            h.id(p.target);
            h.vec(p.aim);
            h.i32(p.speed);
            h.i32(p.damage);
            h.i32(p.crown_pct);
            h.i32(p.splash);
            h.bool(p.hits_air);
            h.bool(p.hits_ground);
            h.vec(p.frac);
        }
        #[cfg(not(clash_plant = "hash_skips_spells"))]
        if !legacy_v3 {
            h.u32(self.spells.len() as u32);
            for s in &self.spells {
                h.u32(s.team as u32);
                h.u32(s.card as u32);
                h.i32(s.level);
                h.i32(s.damage);
                match &s.motion {
                    spell::SpellMotion::Flight { pos, aim, frac, delay_ms } => {
                        h.u32(0);
                        h.vec(*pos);
                        h.vec(*aim);
                        h.vec(*frac);
                        h.i32(*delay_ms);
                    }
                    spell::SpellMotion::Airborne { pos, aim, frac, roll_start, roll_len } => {
                        h.u32(1);
                        h.vec(*pos);
                        h.vec(*aim);
                        h.vec(*frac);
                        h.vec(*roll_start);
                        h.i32(*roll_len);
                    }
                    spell::SpellMotion::Rolling { pos, travelled, len, hit } => {
                        h.u32(2);
                        h.vec(*pos);
                        h.i32(*travelled);
                        h.i32(*len);
                        h.u32(hit.len() as u32);
                        for id in hit {
                            h.id(*id);
                        }
                    }
                    spell::SpellMotion::Area { pos } => {
                        h.u32(3);
                        h.vec(*pos);
                    }
                }
            }
        }
        if !legacy_v3 {
            h.u32(self.effects.knocks.len() as u32);
            for (id, d) in &self.effects.knocks {
                h.id(*id);
                h.vec(*d);
            }
            h.u32(self.effects.stuns.len() as u32);
            for (id, ms) in &self.effects.stuns {
                h.id(*id);
                h.i32(*ms);
            }
        }
        h.u32(self.spawn_queue.len() as u32);
        for s in &self.spawn_queue {
            h.u32(s.team as u32);
            h.u32(s.card as u32);
            h.i32(s.level);
            h.vec(s.pos);
            if !legacy_v3 {
                h.i32(s.deploy_ms.unwrap_or(-1));
                h.opt_id(s.owner);
            }
        }
        h.u32(self.dmg.hits.len() as u32);
        for x in &self.dmg.hits {
            h.id(x.target);
            h.i32(x.amount);
        }
        h.u32(self.death_queue.len() as u32);
        h.finish()
    }
}


// ---------------------------------------------------------------------------
// save / load
//
// WHY IT EXISTS: the Python Engine protocol's save_state() -> bytes and
// load_state(blob) ("exact round trip, including RNG state: load then step must
// equal having never saved"). Used for curriculum starts, search and replays.
//
// FORMAT: serde_json of `Snapshot`. JSON rather than a binary codec because the
// crate already depends on serde_json and a 40-entity snapshot is tens of kB;
// tests/save_load.rs prints the measured size and timings. If search ever needs
// millions of snapshots, a binary codec is a local change here.
//
// WHAT IS NOT IN IT: the card database and the arena (large, static). The
// snapshot carries FINGERPRINTS of both; loading against different data is an
// error, never a silently different battle. The SpatialHash and scratch buffers
// are rebuilt: they are derived from entity state and carry nothing across ticks.
//
// SELF-CHECK: the snapshot records state_hash() at save time and load refuses a
// snapshot whose rebuilt state hashes differently.

/// Bump on any change to `Snapshot`.
/// 2: Calib gained pocket_depth_half_rows.
/// 3: Calib lost pocket_depth_half_rows and gained territory_model; Red's princess
///    towers spawn own-left first.
/// 4: spells -- Calib gained the spells/knockback/status keys; Entities gained
///    retarget_on_resume, knock_rem, knock_ms; PendingSpawn gained deploy_ms; the
///    snapshot gained `spells` and `effects`.
/// 5: the 2026 pathfinder -- Calib gained repath_interval_ticks as an Option plus
///    the pathfinding/movement keys; Entities gained seg_dir and move_ticks;
///    PathModel gained Oracle2026 (code 3).
/// 6: hide (Tesla) -- Calib gained the hide.* keys; Entities gained hide and
///    hide_ms; combat::Hit gained ignores_hide (serde default false). The SAME
///    number was also used for the measured 16.402 search and contact law --
///    Calib gained path_search; Entities gained facing and avoid_offset -- so a
///    "format 6" blob of either kind is refused below like any other stale format.
/// 7: spawners -- Calib gained the spawner.* keys; Entities gained spawn_ms,
///    spawn_wave_left and spawned_by; PendingSpawn gained owner.
/// 8: charge -- Calib gained the charge.* keys; Entities gained charge_progress and
///    charged; CardDef gained charge (the card fingerprint moves).
/// 9: the union of 6 (both readings), 7 and 8: Calib carries hide.*, spawner.*,
///    charge.* AND path_search; Entities carry hide, hide_ms, spawn_ms,
///    spawn_wave_left, spawned_by, charge_progress, charged, facing and avoid_offset.
pub const SNAPSHOT_FORMAT: u32 = 9;

mod push_model_serde {
    use crate::PushModel;
    pub fn serialize<S: serde::Serializer>(m: &PushModel, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(match m {
            PushModel::MassWeighted => 0,
            PushModel::SpeedWeighted => 1,
            PushModel::EqualSplit => 2,
        })
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<PushModel, D::Error> {
        match <u8 as serde::Deserialize>::deserialize(d)? {
            0 => Ok(PushModel::MassWeighted),
            1 => Ok(PushModel::SpeedWeighted),
            2 => Ok(PushModel::EqualSplit),
            x => Err(serde::de::Error::custom(format!("unknown PushModel code {x}"))),
        }
    }
}

mod path_model_serde {
    use crate::PathModel;
    pub fn serialize<S: serde::Serializer>(m: &PathModel, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(match m {
            PathModel::LaneSnap => 0,
            PathModel::GridAStar => 1,
            PathModel::DiagonalLookahead => 2,
            PathModel::Oracle2026 => 3,
        })
    }
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<PathModel, D::Error> {
        match <u8 as serde::Deserialize>::deserialize(d)? {
            0 => Ok(PathModel::LaneSnap),
            1 => Ok(PathModel::GridAStar),
            2 => Ok(PathModel::DiagonalLookahead),
            3 => Ok(PathModel::Oracle2026),
            x => Err(serde::de::Error::custom(format!("unknown PathModel code {x}"))),
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Snapshot {
    format: u32,
    cards_fingerprint: u64,
    arena_fingerprint: u64,
    calib: Calib,
    #[serde(with = "path_model_serde")]
    path_model: PathModel,
    #[serde(with = "push_model_serde")]
    push_model: PushModel,
    footprint_model: FootprintModel,
    tower_sight_reading: TowerSightReading,
    decks: [Vec<String>; 2],
    card_level: [i32; 2],
    tower_level: [i32; 2],
    shuffle_decks: bool,
    bucket_subtiles: i32,
    ents: Entities,
    rng: Rng,
    players: [PlayerState; 2],
    dmg: DamageBuffer,
    projectiles: Vec<Projectile>,
    spells: Vec<Spell>,
    effects: EffectBuffer,
    spawn_queue: Vec<PendingSpawn>,
    death_queue: Vec<EntityId>,
    tick: u32,
    crowns: [u8; 2],
    towers: TowerTable,
    towers_down: [[bool; 3]; 2],
    king_wake_ms: [Option<i32>; 2],
    king_active: [bool; 2],
    overtime: bool,
    outcome: Option<Outcome>,
    lifetime_ms: Vec<Option<i32>>,
    mana_unit: i64,
    mana_rate: [i64; 2],
    state_hash: u64,
}

fn fingerprint_debug<T: std::fmt::Debug>(v: &T) -> u64 {
    let mut h = Fnv::new();
    h.bytes(format!("{v:?}").as_bytes());
    h.finish()
}

/// MIGRATE A FORMAT-3 SNAPSHOT TO FORMAT 4, in place, as JSON. Returns the format-3
/// card index -> current CardDb index table (applied by `load_with` AFTER the saved
/// hash is reproduced).
///
/// WHY IT EXISTS: the fixture behind tests/stacked_tie.rs is a format-3 snapshot
/// that cannot be regenerated without the random-game driver that found it, and
/// format 4 (spells) changed three things under it:
///   1. NEW FIELDS. Filled with their NEUTRAL values -- the values under which format
///      4 runs a format-3 battle exactly as format 3 did: no spells, no effects, no
///      knockback, no resume flags, no deploy overrides; and the three Calib keys a
///      format-3 battle already consumed under another name keep format 3's
///      behaviour (troop projectile speed = the troop speed key; crown rounding =
///      Floor, the pre-registry truncation; stun expiry = one_tick_short, the old
///      Status-phase decrement). Every other new Calib key only affects spells and
///      takes the shipped value.
///   2. CARD INDICES. Format 3's CardDb refused every spell and had no summon-only
///      units, so its indices are this CardDb's non-spell, non-summon cards in order.
///   3. THE CARD FINGERPRINT. Recomputed as format 3 computed it (the same Debug text
///      without the three CardDef fields format 4 added) on that filtered list, and
///      refused on mismatch -- so the troop stats are proven the ones it was saved
///      against.
///
/// THE PROOF THAT IT CHANGED NOTHING is `load_with`'s self-check: the format-3 hash
/// (`hash_state(true)`) of the migrated state must equal the hash saved in the file.
fn migrate_v3(v: &mut Value, cards: &CardDb) -> Result<Vec<u16>, String> {
    let bad = |what: &str| format!("format-3 snapshot: {what}");
    // Format 3's loader also refused every card without a Range column -- the
    // non-attacking spawner buildings (Tombstone, GoblinHut, BarbarianHut,
    // FirespiritHut), now loaded with range 0 and no damage source.
    let refused_by_v3 = |c: &CardDef| c.spell.is_some() || c.summon_only || (c.kind == CardKind::Building && c.range == 0 && c.damage == 0 && c.projectile.is_none());
    let old: Vec<usize> = (0..cards.cards.len()).filter(|&i| !refused_by_v3(&cards.cards[i])).collect();
    let debug_v3: String = {
        let items: Vec<String> = old
            .iter()
            .map(|&i| {
                let c = &cards.cards[i];
                // Every CardDef field added after format 3, in declaration order.
                // Not just ignore_pushback/spell/summon_only: format 5 added the
                // two stomp columns, which is why card.rs declares them LAST.
                // ~~... wait_ms~~ -- format 6 added `hide` (card.rs declares it last).
                // ~~... hide~~ -- format 7 added `spawner` and `death_spawn` after it.
                // ~~... death_spawn~~ -- format 8 added `charge` after them.
                let tail = format!(
                    ", ignore_pushback: {}, spell: None, summon_only: false, stop_movement_after_ms: {}, wait_ms: {}, hide: {:?}, spawner: {:?}, death_spawn: {:?}, charge: {:?} }}",
                    c.ignore_pushback, c.stop_movement_after_ms, c.wait_ms, c.hide, c.spawner, c.death_spawn, c.charge
                );
                let d = format!("{c:?}");
                d.strip_suffix(&tail).map(|head| format!("{head} }}")).ok_or_else(|| bad("CardDef Debug layout changed; the v3 fingerprint cannot be rebuilt"))
            })
            .collect::<Result<_, _>>()?;
        format!("[{}]", items.join(", "))
    };
    let mut fp = Fnv::new();
    fp.bytes(debug_v3.as_bytes());
    if v.get("cards_fingerprint").and_then(Value::as_u64) != Some(fp.finish()) {
        return Err("snapshot was saved against different card data (format-3 fingerprint)".into());
    }
    let o = v.as_object_mut().ok_or_else(|| bad("not an object"))?;
    o.insert("format".into(), Value::from(SNAPSHOT_FORMAT));
    o.insert("cards_fingerprint".into(), Value::from(fingerprint_debug(&cards.cards)));
    // 1. Calib.
    let calib = o.get_mut("calib").and_then(Value::as_object_mut).ok_or_else(|| bad("no calib"))?;
    let troop_speed = calib.get("speed_to_subtiles_per_tick").cloned().ok_or_else(|| bad("no speed_to_subtiles_per_tick"))?;
    let mut shipped = serde_json::to_value(Calib::shipped()).map_err(|e| e.to_string())?;
    let sh = shipped.as_object_mut().expect("Calib serializes to an object");
    sh.insert("projectile_speed_to_subtiles_per_tick".into(), troop_speed);
    sh.insert("crown_rounding".into(), serde_json::to_value(CrownRounding::Floor).map_err(|e| e.to_string())?);
    sh.insert("buff_expiry".into(), serde_json::to_value(BuffExpiry::OneTickShort).map_err(|e| e.to_string())?);
    for (k, val) in sh.iter() {
        calib.entry(k.clone()).or_insert_with(|| val.clone());
    }
    // 1. Entities.
    let ents = o.get_mut("ents").and_then(Value::as_object_mut).ok_or_else(|| bad("no ents"))?;
    let n = ents.get("alive").and_then(Value::as_array).map(Vec::len).ok_or_else(|| bad("no ents.alive"))?;
    let zero = serde_json::to_value(Vec2::default()).map_err(|e| e.to_string())?;
    let zero2 = serde_json::to_value(Vec2::default()).map_err(|e| e.to_string())?;
    for (k, fill) in [
        ("retarget_on_resume", Value::Bool(false)),
        ("knock_rem", zero),
        ("knock_ms", Value::from(0)),
        // FORMAT 5: no segment is being walked in a format-3 battle's first tick
        // after load, and zero is exactly "no segment" (path2026.rs `arrived`).
        ("seg_dir", zero2),
        ("move_ticks", Value::from(0)),
        // FORMAT 6: every format-3 entity is Up with no countdown running. A
        // format-3 Tesla (the scripted RED_DECK carries one) then goes under on its
        // first idle Target phase after load -- the hide machinery did not exist when
        // the fixture was saved, and this is the state that expresses "not hiding
        // yet" without inventing a timer.
        ("hide", serde_json::to_value(HideState::Up).map_err(|e| e.to_string())?),
        ("hide_ms", Value::from(0)),
        // FORMAT 7: no wave in progress and a timer already due, so a format-3
        // spawner (a Witch in the fixture's decks, if any) starts its cadence on the
        // first Spawn phase after load -- "not spawning yet" without inventing a
        // timer; no unit owes its existence to a spawner.
        ("spawn_ms", Value::from(0)),
        ("spawn_wave_left", Value::from(0)),
        ("spawned_by", Value::Null),
        // FORMAT 8: no run-up and no charge. Format 3 ran every Prince permanently
        // uncharged, so 0 / false reproduces its state exactly (the format-3 hash
        // self-check below proves it); from the first tick after load the Princes in
        // the fixture's decks accumulate like any freshly walking unit.
        ("charge_progress", Value::from(0)),
        ("charged", Value::Bool(false)),
        // FORMAT 6 (royalegym-v2's): the avoidance offset is 0 between ticks for a
        // unit that has not met anything; the facing is filled per team below.
        ("avoid_offset", Value::from(0)),
    ] {
        if ents.insert(k.into(), Value::Array(vec![fill; n])).is_some() {
            return Err(bad(&format!("already has ents.{k}")));
        }
    }
    {
        // FORMAT 6: facing = the team's initial facing (entity.rs `initial_facing`)
        let teams: Vec<Value> = ents.get("team").and_then(Value::as_array).cloned().ok_or_else(|| bad("no ents.team"))?;
        let mut facing = Vec::with_capacity(n);
        for t in teams.iter() {
            let team: Team = serde_json::from_value(t.clone()).map_err(|e| e.to_string())?;
            facing.push(serde_json::to_value(crate::entity::initial_facing(team)).map_err(|e| e.to_string())?);
        }
        if ents.insert("facing".into(), Value::Array(facing)).is_some() {
            return Err(bad("already has ents.facing"));
        }
    }
    // 1. Pending spawns, spells, effects.
    for p in o.get_mut("spawn_queue").and_then(Value::as_array_mut).ok_or_else(|| bad("no spawn_queue"))? {
        let entry = p.as_object_mut().ok_or_else(|| bad("spawn_queue entry"))?;
        entry.insert("deploy_ms".into(), Value::Null);
        entry.insert("owner".into(), Value::Null);
    }
    o.insert("spells".into(), Value::Array(Vec::new()));
    o.insert("effects".into(), serde_json::to_value(EffectBuffer::default()).map_err(|e| e.to_string())?);
    // 2. Card indices.
    Ok(old.iter().map(|&i| i as u16).collect())
}

/// data/derived/cards.json, loaded once per process, for `BattleState::load`.
fn repo_cards() -> Result<Arc<CardDb>, String> {
    static CELL: OnceLock<Result<Arc<CardDb>, String>> = OnceLock::new();
    CELL.get_or_init(|| CardDb::load_repo().map(Arc::new)).clone()
}

impl BattleState {
    /// Serialize the complete simulation state (see the section comment).
    pub fn save(&self) -> Vec<u8> {
        let c = &self.cfg;
        let snap = Snapshot {
            format: SNAPSHOT_FORMAT,
            cards_fingerprint: fingerprint_debug(&c.cards.cards),
            arena_fingerprint: fingerprint_debug(&c.arena),
            calib: c.calib.clone(),
            path_model: c.path_model,
            push_model: c.push_model,
            footprint_model: c.footprint_model,
            tower_sight_reading: c.tower_sight_reading,
            decks: c.decks.clone(),
            card_level: c.card_level,
            tower_level: c.tower_level,
            shuffle_decks: c.shuffle_decks,
            bucket_subtiles: c.bucket_subtiles,
            ents: self.ents.clone(),
            rng: self.rng,
            players: self.players.clone(),
            dmg: self.dmg.clone(),
            projectiles: self.projectiles.clone(),
            #[cfg(not(clash_plant = "save_drops_spells"))]
            spells: self.spells.clone(),
            #[cfg(clash_plant = "save_drops_spells")]
            spells: Vec::new(), // PLANT: spells in flight are lost across a save.
            effects: self.effects.clone(),
            spawn_queue: self.spawn_queue.clone(),
            death_queue: self.death_queue.clone(),
            tick: self.tick,
            crowns: self.crowns,
            towers: self.towers,
            towers_down: self.towers_down,
            king_wake_ms: self.king_wake_ms,
            king_active: self.king_active,
            overtime: self.overtime,
            outcome: self.outcome,
            lifetime_ms: self.lifetime_ms.clone(),
            mana_unit: self.mana_unit,
            mana_rate: self.mana_rate,
            state_hash: self.state_hash(),
        };
        #[cfg(clash_plant = "save_drops_rng")]
        let snap = Snapshot { rng: Rng::new(0), ..snap };
        #[cfg(clash_plant = "save_drops_knockback")]
        let snap = {
            // PLANT: knockback slides are lost across a save.
            let mut snap = snap;
            snap.ents.knock_ms.iter_mut().for_each(|m| *m = 0);
            snap.ents.knock_rem.iter_mut().for_each(|r| *r = Vec2::default());
            snap
        };
        serde_json::to_vec(&snap).expect("snapshot serializes")
    }

    /// Rebuild a battle from `save()` bytes against the repository's
    /// data/derived/cards.json and the shipped arena. Errors (never guesses) on a
    /// format mismatch, different card data or arena, or a failed hash self-check.
    pub fn load(bytes: &[u8]) -> Result<BattleState, String> {
        BattleState::load_with(bytes, repo_cards()?, Arena::shipped())
    }

    /// `load` against explicit card data and arena.
    pub fn load_with(bytes: &[u8], cards: Arc<CardDb>, arena: Arena) -> Result<BattleState, String> {
        #[derive(serde::Deserialize)]
        struct FormatOnly {
            format: u32,
        }
        let format = serde_json::from_slice::<FormatOnly>(bytes).map_err(|e| format!("snapshot: {e}"))?.format;
        // Format 3 is MIGRATED, never guessed: see `migrate_v3`. `remap` is then the
        // format-3 card index -> this CardDb's index, applied only after the saved
        // hash has been reproduced on the unmigrated indices.
        let (snap, remap): (Snapshot, Option<Vec<u16>>) = match format {
            SNAPSHOT_FORMAT => (serde_json::from_slice(bytes).map_err(|e| format!("snapshot: {e}"))?, None),
            3 => {
                let mut v: Value = serde_json::from_slice(bytes).map_err(|e| format!("snapshot: {e}"))?;
                let remap = migrate_v3(&mut v, &cards)?;
                (serde_json::from_value(v).map_err(|e| format!("snapshot (migrated from format 3): {e}"))?, Some(remap))
            }
            other => return Err(format!("snapshot format {other} != engine format {SNAPSHOT_FORMAT}")),
        };
        if remap.is_none() && snap.cards_fingerprint != fingerprint_debug(&cards.cards) {
            return Err("snapshot was saved against different card data".into());
        }
        if snap.arena_fingerprint != fingerprint_debug(&arena) {
            return Err("snapshot was saved against a different arena".into());
        }
        let n = snap.ents.capacity();
        if snap.lifetime_ms.len() > n
            || snap.ents.card.iter().any(|c| (*c as usize) >= cards.cards.len())
            || snap.spells.iter().any(|s| cards.cards.get(s.card as usize).map_or(true, |c| c.spell.is_none()))
            || snap.spawn_queue.iter().any(|p| (p.card as usize) >= cards.cards.len())
        {
            return Err("snapshot entity tables are inconsistent".into());
        }
        let hash = SpatialHash::new(arena.width, arena.height, snap.bucket_subtiles);
        let cfg = BattleConfig {
            calib: snap.calib,
            arena,
            cards,
            path_model: snap.path_model,
            push_model: snap.push_model,
            footprint_model: snap.footprint_model,
            tower_sight_reading: snap.tower_sight_reading,
            decks: snap.decks,
            card_level: snap.card_level,
            tower_level: snap.tower_level,
            shuffle_decks: snap.shuffle_decks,
            bucket_subtiles: snap.bucket_subtiles,
        };
        let mut s = BattleState {
            cfg,
            ents: snap.ents,
            hash,
            rng: snap.rng,
            players: snap.players,
            dmg: snap.dmg,
            projectiles: snap.projectiles,
            spells: snap.spells,
            effects: snap.effects,
            spawn_queue: snap.spawn_queue,
            death_queue: snap.death_queue,
            tick: snap.tick,
            crowns: snap.crowns,
            towers: snap.towers,
            towers_down: snap.towers_down,
            king_wake_ms: snap.king_wake_ms,
            king_active: snap.king_active,
            overtime: snap.overtime,
            outcome: snap.outcome,
            lifetime_ms: snap.lifetime_ms,
            mana_unit: snap.mana_unit,
            mana_rate: snap.mana_rate,
            scratch: Scratch::default(),
            phase_trace: None,
        };
        s.hash.rebuild(&s.ents);
        let got = s.hash_state(remap.is_some());
        #[cfg(not(clash_plant = "load_skips_selfcheck"))]
        if got != snap.state_hash {
            let what = if remap.is_some() { " (format-3 hash of the migrated state)" } else { "" };
            return Err(format!("snapshot self-check failed: saved hash {:#x}, rebuilt {got:#x}{what}", snap.state_hash));
        }
        let _ = got;
        if let Some(map) = remap {
            // The hash is proven; only now move the card indices to this CardDb's.
            let m = |c: &mut u16| *c = map[*c as usize];
            s.ents.card.iter_mut().for_each(m);
            for p in s.players.iter_mut() {
                p.hand.iter_mut().for_each(m);
                p.queue.iter_mut().for_each(m);
            }
            s.spawn_queue.iter_mut().for_each(|p| m(&mut p.card));
        }
        Ok(s)
    }

    /// The Python `load_state(blob)` shape: replace this battle's state with a
    /// snapshot, reusing this battle's card data and arena (which must match).
    pub fn restore(&mut self, bytes: &[u8]) -> Result<(), String> {
        let s = BattleState::load_with(bytes, self.cfg.cards.clone(), self.cfg.arena.clone())?;
        *self = s;
        Ok(())
    }
}

/// Wall-clock time per phase, compiled only with RUSTFLAGS='--cfg clash_profile'
/// (tests/throughput.rs reads it). Instrumentation, not simulation state: it
/// never feeds back into a battle, and it does not exist in a normal build.
#[cfg(clash_profile)]
pub mod profile {
    use std::cell::RefCell;
    thread_local! {
        static NANOS: RefCell<[u128; 11]> = const { RefCell::new([0; 11]) };
    }
    pub(crate) fn add(k: usize, ns: u128) {
        NANOS.with(|n| n.borrow_mut()[k] += ns);
    }
    /// Per-phase nanoseconds (TICK_PHASES order) since the last call; resets.
    pub fn take() -> [u128; 11] {
        NANOS.with(|n| std::mem::take(&mut *n.borrow_mut()))
    }
}

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    #[inline]
    fn bytes(&mut self, b: &[u8]) {
        for x in b {
            self.0 ^= *x as u64;
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }
    fn i64(&mut self, v: i64) {
        self.bytes(&v.to_le_bytes());
    }
    fn bool(&mut self, v: bool) {
        self.bytes(&[u8::from(v)]);
    }
    fn vec(&mut self, v: Vec2) {
        self.i32(v.x);
        self.i32(v.y);
    }
    fn id(&mut self, id: EntityId) {
        self.u32(id.index);
        self.u32(id.generation);
    }
    fn opt_id(&mut self, id: Option<EntityId>) {
        match id {
            Some(i) => {
                self.bool(true);
                self.id(i);
            }
            None => self.bool(false),
        }
    }
    fn finish(&self) -> u64 {
        self.0
    }
}

/// Where a building's footprint is, for callers outside the tick (tests,
/// observation builders).
pub fn footprint_of(state: &BattleState, id: EntityId) -> Option<Shape> {
    let v = state.entity(id)?;
    if !v.kind.is_building() {
        return None;
    }
    let king_of = if v.kind == EntityKind::KingTower { Some(v.team) } else { None };
    Some(state.cfg.arena.building_shape(state.cfg.footprint_model, v.pos, v.radius, king_of))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_calibration_loads_every_field() {
        let c = Calib::shipped();
        assert_eq!(c.tick_ms, 50);
        // 18, not 15: Speed is millitiles per 50 ms tick. A free per-unit integer
        // fit over every moving tick of every unit in the offline corpus has exactly
        // one survivor per card, and it IS the raw Speed column, so one Speed unit is
        // 18 subtiles per tick. 15 is the divide_by_60 hypothesis, which the corpus
        // refutes.
        assert_eq!(c.speed_to_subtiles_per_tick, 18);
        assert_eq!(c.extra_sight_range_to_crown_towers, crate::fixed::milli(2000));
        // There is no periodic replan timer at all -- 150 structural recomputes over
        // 31 859 path-ticks, with no common period. The folklore 10-tick cadence is
        // null in the ledger and the triggers are event-driven.
        assert_eq!(c.repath_interval_ticks, None);
        assert_eq!(c.mana_speed_up_remaining_s, 60);
        // pathfinding.ALGORITHM is measured, not assumed: it is the weighted grid
        // A*, not the folklore lane snap.
        assert_eq!(c.path_model, PathModel::Oracle2026);
        // The 2026 keys the pathfinder runs on, all read from the ledger.
        assert_eq!((c.path_cost_default, c.path_cost_road, c.path_cost_heuristic), (8, 5, 5));
        assert_eq!((c.diag_num, c.diag_den), (1414, 1000));
        assert_eq!(c.waypoint_arrive_radius, crate::fixed::milli(1000));
        assert_eq!(c.waypoint_arrive_rule, WaypointArriveRule::SegmentProjection);
        // Not a hard block: on the live 16.402 corpus 97 of 785 first paths cross a
        // building box interior, which a block makes infeasible. An occluded cell is
        // PATHFINDING_BUILDING_COST to enter, not a wall.
        assert_eq!(c.occluded_cells, OccludedCells::Cost50);
        assert_eq!(c.path_cost_building, 50);
        // Chebyshev, not octile: on the live 16.402 corpus, handed the client's own
        // goal cell, octile reproduces 78/292 exact node sequences and 5 * Chebyshev
        // over the same goal set 192/292.
        assert_eq!(c.heuristic_form, HeuristicForm::ChebyshevOverGoalSet);
        assert_eq!(c.tie_break, TieBreak::OrthoFirstPlaceholder);
    }

    #[test]
    fn charge_keys_load_their_shipped_arms_and_refuse_an_unimplemented_name() {
        let c = Calib::shipped();
        assert_eq!(c.charge_range_unit, ChargeRangeUnit::Centitiles);
        assert_eq!(c.charge_accumulator, ChargeAccumulator::Client16402ProgressPermille);
        assert_eq!(c.charge_multiplier_meaning, ChargeMultiplier::MovementWhenCharged);
        assert_eq!(c.charge_progress_on_stop, ChargeStopRule::Reset);
        assert!(c.charge_reset_on_attack && c.charge_reset_on_stun && c.charge_reset_on_knockback && !c.charge_reset_on_retarget);
        assert_eq!(c.charge_special_level_scaling, ChargeLevelScaling::ScaleSpecialBase);
        // A candidate the engine does not implement is refused at load, never mapped.
        let edited = CALIBRATION_JSON.replacen("\"value\": \"client16402_progress_permille\"", "\"value\": \"own_frame_forward_progress\"", 1);
        assert_ne!(edited, CALIBRATION_JSON, "edit did not apply; the JSON layout changed");
        let err = Calib::from_json(&edited).expect_err("an unimplemented ACCUMULATOR loaded");
        assert!(err.contains("charge.ACCUMULATOR") && err.contains("no engine implementation"), "{err}");
    }

    #[test]
    fn calibration_values_are_read_not_inlined() {
        // Change a value in the JSON and the loaded constant must follow.
        // REPATH_INTERVAL_TICKS is null (retired), so the probe uses the waypoint
        // arrive radius, which the 2026 locomotion law reads every tick.
        let edited = CALIBRATION_JSON.replacen(
            "\"value\": 1000,\n      \"units\": \"native arena units (1 tile = 2 cells)\"",
            "\"value\": 3,\n      \"units\": \"native arena units (1 tile = 2 cells)\"",
            1,
        );
        assert_ne!(edited, CALIBRATION_JSON, "edit did not apply; the JSON layout changed");
        assert_eq!(Calib::from_json(&edited).unwrap().waypoint_arrive_radius, crate::fixed::milli(3));
        // And the retired key still round-trips as an explicit null.
        assert_eq!(Calib::shipped().repath_interval_ticks, None);
    }

    #[test]
    fn slot_deploys_report_why_and_scenario_setup_is_consistent() {
        // Fallback cards: no building card, so this covers troops (ground and
        // flying) on tower footprints, slots, and the tower-destroyed setup entry point.
        let mut cfg = BattleConfig::with_cards(CardDb::fallback());
        let deck: Vec<String> =
            ["Minions", "Knight", "Giant", "Archers", "Archers", "Giant", "Knight", "Minions"].iter().map(|s| s.to_string()).collect();
        cfg.decks = [deck.clone(), deck];
        let mut s = BattleState::new(1, cfg);
        let a = s.arena().clone();
        let princess = a.princess_tower_pos(Team::Blue, Lane::Left);
        assert_eq!(s.hand_card(Team::Blue, 0).map(|i| s.cards().get(i).name.clone()), Ok("Minions".to_string()));
        assert_eq!(s.check_deploy_slot(Team::Blue, 0, princess), Err(DeployError::Occupied), "flying troops too");
        assert_eq!(s.check_deploy_slot(Team::Blue, 1, princess), Err(DeployError::Occupied));
        assert_eq!(s.check_deploy_slot(Team::Blue, HAND_SIZE, princess), Err(DeployError::BadSlot));
        let river = Vec2::new(a.width / 2, (a.water_y_min + a.water_y_max) / 2);
        assert_eq!(s.check_deploy_slot(Team::Blue, 1, river), Err(DeployError::Water));
        let enemy_pocket = Vec2::new(princess.x, a.water_y_max + a.cell / 2);
        assert_eq!(s.check_deploy_slot(Team::Blue, 1, enemy_pocket), Err(DeployError::OutOfTerritory));
        // Red's Left princess (engine lane Left) falls before the battle.
        s.scenario_set_tower_hp(Team::Red, 1, 0).unwrap();
        assert_eq!(s.crowns(), [1, 0]);
        assert!(s.king_active(Team::Red) && !s.king_active(Team::Blue));
        assert_eq!(s.tower_hp(Team::Red)[1], 0);
        assert_eq!(s.check_deploy_slot(Team::Blue, 1, enemy_pocket), Ok(()), "the pocket opens on that lane");
        assert!(s.scenario_set_tower_hp(Team::Blue, 0, 0).is_err(), "a king cannot start destroyed");
        // A slot deploy spends elixir and cycles exactly that slot.
        let before = s.hand(Team::Blue).iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let next = s.next_card(Team::Blue).map(|x| x.to_string());
        let knight_spot = Vec2::new(princess.x, princess.y - 3 * a.cell);
        s.deploy_slot(Team::Blue, 1, knight_spot).unwrap();
        let after = s.hand(Team::Blue);
        assert_eq!((after[0], after[2], after[3]), (before[0].as_str(), before[2].as_str(), before[3].as_str()));
        assert_eq!(Some(after[1].to_string()), next, "the queue front takes the played slot");
    }

    #[test]
    fn formation_siblings_have_distinct_yield_keys_and_towers_own_frame_team_seq() {
        // path::avoid_units breaks an exactly colinear meeting by YieldKey. Siblings
        // from one deploy share spawn tick, card, level and hp, so without team_seq
        // every one of them held the same key. Plant: sibling_yield_key.
        let mut s = BattleState::new(1, BattleConfig::with_cards(CardDb::load_repo().expect("cards.json")));
        let a = s.arena().clone();
        s.spawn_unit(Team::Blue, "SkeletonArmy", Vec2::new(a.width / 2, crate::fixed::tiles(10)), None).unwrap();
        s.tick();
        let keys: Vec<path::YieldKey> =
            s.ents.live_indices().filter(|&i| s.ents.kind[i] == EntityKind::Troop).map(|i| yield_key(&s.ents, i)).collect();
        assert!(keys.len() >= 10, "vacuous: {} skeletons", keys.len());
        let mut uniq = keys.clone();
        uniq.sort();
        uniq.dedup();
        assert_eq!(uniq.len(), keys.len(), "formation siblings share a YieldKey: {keys:?}");
        // The towers' team_seq names the same OWN-FRAME tower for both seats.
        let seq = |team: Team, k: usize| s.ents.team_seq[s.towers[team as usize][k].unwrap().index as usize];
        assert_eq!(seq(Team::Blue, 1), seq(Team::Red, 2), "own-left princess");
        assert_eq!(seq(Team::Blue, 2), seq(Team::Red, 1), "own-right princess");
        assert_eq!(seq(Team::Blue, 0), seq(Team::Red, 0), "king");
    }

    #[test]
    fn mana_is_exact_over_a_full_bar() {
        let cfg = BattleConfig::with_cards(CardDb::fallback());
        let mut s = BattleState::new(1, cfg);
        let c = Calib::shipped();
        // From START_MANA, (MAX-START) elixir at 1x takes exactly this many ticks.
        let ms = (c.max_mana - c.start_mana) as i64 * c.mana_regen_ms_1x as i64 / c.max_mana as i64;
        let ticks = ms / c.tick_ms as i64;
        for _ in 0..ticks - 1 {
            s.tick();
        }
        assert_eq!(s.elixir(Team::Blue), c.max_mana - 1);
        s.tick();
        assert_eq!(s.elixir(Team::Blue), c.max_mana);
    }
}
