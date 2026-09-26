//! BattleState and the tick loop.
//!
//! THE LOOP IS DRIVEN BY lib.rs::TICK_PHASES (calibration match.TICK_ORDER)
//!     `tick()` iterates that array and dispatches on each phase. Reordering the
//!     array reorders the engine; nothing else encodes the order. The array is the
//!     order measured on the live 16.402 captures: every ATTACK update for every
//!     entity, then the MOVE updates one after the other in creation order, then
//!     the per-unit character update (the deploy countdown) after the move pass.
//!     lib.rs::LEGACY_TICK_PHASES is the order before that measurement (Move before
//!     Attack, the countdown in Upkeep), kept runnable under match.TICK_ORDER =
//!     legacy_move_before_attack.
//!
//! WHAT RUNS WHERE (the shipped order)
//!     Upkeep     elixir accrual (the deploy countdown too, under the legacy order)
//!     Status     king activation timer; building lifetimes (stun/slow timers only
//!                under status.BUFF_EXPIRY_TICK_ALIGNMENT = one_tick_short)
//!     Spawn      pending spawns (deploys since last tick) materialise; accepted
//!                spell casts become spell objects (spell.rs); then every periodic
//!                SPAWNER past its deploy time ticks its timer and queues the units
//!                that are due (spawner_pass; they materialise NEXT tick)
//!     Target     every entity decides its target from start-of-phase state
//!     Attack     windups advance; hits go to the damage buffer / projectile list
//!                (a charged unit's hit is DamageSpecial and consumes the charge;
//!                under combat.REFLECT_ATTACK = client_reflect_stun a melee hit on
//!                a reflecting unit buffers its answer on the attacker and lands
//!                the answer's stun at once, `reflect_melee_hit`).
//!                BEFORE the move: a unit in range at the start of the tick winds
//!                up and does not step this tick; one whose target is gone or out
//!                of range walks this tick (the 1 -> 2 / 2 -> 1 rules)
//!     Path       every unit proposes a movement delta from start-of-phase state;
//!                under the 16.402 locomotion the whole sequential move pass in
//!                creation order (phase_path16402), a unit dying this tick dropped
//!                from the scans of the movers after it (movement.DYING_UNIT_VISIBILITY)
//!     Move       deltas applied, then collision separation (buffered; frame-planned
//!                arms only); then every CHARGE card's run-up gains from its own
//!                walk (charge_pass); then THE DEPLOY COUNTDOWN (deploy_countdown,
//!                after the move pass, as measured): a unit whose
//!                deploy time ends this tick stood still this tick and walks next
//!     Projectile projectiles advance; arrivals go to the damage buffer. Spells
//!                advance: impacts write damage / knockback / stun buffers, landing
//!                Goblin Barrels queue their units
//!     Resolve    the damage buffer is applied in one pass; deaths are queued; then
//!                stun timers tick, the stun buffer merges (max) and the knockback
//!                buffer sums and moves the SURVIVORS (knockback.DISPLACEMENT_LAW =
//!                fixed_distance) or ARMS THE LADDER on them (client16402,
//!                shipped: the first buffered push per unit, the rest refused while
//!                it runs; the steps come in the Path phase from the next tick on)
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
use crate::entity::{AttackPhase, DashState, EntityKind, Entities, HideState, SpatialHash, SpawnInit};
use crate::fixed::{isqrt, Vec2, SUBTILE};
use crate::path::{self, FrameWorld, NavRequest, Obstacle, UnitBlocker};
use crate::path2026;
use crate::status::{BuffSlot, Sel};
use crate::move16402;
use crate::path16402;
use crate::jump16402;
use crate::target::{self, TargetCtx, TargetDecision, TowerSightReading, TowerTable};
use crate::{EntityId, PathModel, Phase, PushModel, Rng, Team, LEGACY_TICK_PHASES, TICK_PHASES};
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
    /// Which attackers `preserve_target_if_hit_started` reaches: every one (the ledger value `true`),
    /// or only those whose card fires a projectile (the value "projectile_attackers_only"; `locks_target`,
    /// target.rs `decide`). Added after SNAPSHOT_FORMAT 20; the `default` is `AllAttackers`, what a battle
    /// saved before it actually ran.
    #[serde(default = "preserve_target_scope_default")]
    pub preserve_target_scope: PreserveTargetScope,
    pub xpos_based_tower_targeting: bool,
    pub melee_range_limit: i32,
    #[serde(with = "push_model_serde")]
    pub push_model: PushModel,
    pub separation_iterations: i32,
    pub footprint_model: FootprintModel,
    #[serde(with = "path_model_serde")]
    pub path_model: PathModel,
    /// pathfinding.REPATH_INTERVAL_TICKS. `None` = no periodic replan, which is
    /// what was measured on client 15.535.29 (150 structural recomputes over 31 859
    /// path-ticks with no common period); the pre-2026 models then replan only when
    /// their route empties or their goal moves. A positive value re-arms the old
    /// folklore cadence for those models; the 2026 model ignores it entirely and
    /// uses pathfinding.REPLAN_TRIGGERS.
    pub repath_interval_ticks: Option<i32>,
    /// pathfinding.GOAL_TARGET_POSITION: which centre of its target a chaser picks its goal cell
    /// around (`phase_path16402`). Added after SNAPSHOT_FORMAT 20; the `default` is the old arm,
    /// what a battle saved before it actually ran.
    #[serde(default = "goal_target_position_default")]
    pub goal_target_position: GoalTargetPosition,
    /// pathfinding.FLYER_GOAL_WATER: whether a FLYING chaser's goal choice ranks a water cell below
    /// dry ground, as a ground chaser's does (path16402.rs `choose_goal_cell`). Added after
    /// SNAPSHOT_FORMAT 20; the `default` is the old arm.
    #[serde(default = "flyer_goal_water_default")]
    pub flyer_goal_water: FlyerGoalWater,

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
    /// PATHFINDING_WATER_COST (7): what a HOVERING or JumpEnabled mover pays for a
    /// water cell in place of BLOCKED. Read under
    /// `PATH_SEARCH = client16402` for every mover with a card.rs `JumpDef`
    /// (path16402.rs `cell_cost_for`); the trace-fitted arm keeps water impassable.
    pub path_cost_water: i32,
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
    /// priced 50 and pushed) -- 615/616 live node sequences. `trace_fitted_astar` is
    /// the earlier model fitted to traces (goal set, Chebyshev h, the
    /// TIE_BREAK / HEURISTIC_FORM / OCCLUDED_CELL_TREATMENT knobs), kept runnable as
    /// the refuted arm: 168/345.
    pub path_search: PathSearch,
    /// movement.JUMP_WATER_HOP -- what a JumpEnabled troop does at the water once its
    /// search priced it at WATER_COST. `client16402`: the leap (jump16402.rs;
    /// `phase_path16402`), measured step for step on the five live hops of the
    /// 16.402 corpus. `walk_priced_water`: no hop -- the unit
    /// walks the cost-7 water at its Speed, the naive reading of the cost field the
    /// live captures refute, kept runnable as the foil.
    pub jump_water_hop: JumpWaterHop,
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
    /// targeting.CENTRE_LANE_FRAME -- the frame the default-tower lane is decided in
    /// (target.rs `default_tower`).
    pub centre_lane_frame: CentreLaneFrame,
    /// status.ATTRACT_LAW -- the base speed the AttractPercentage column scales
    /// (state.rs `phase_path16402`).
    pub attract_base: AttractBase,
    /// status.ATTRACT_WHILE_HELD.
    pub attract_while_held: AttractWhileHeld,
    /// match.DEPLOY_LOCKOUT_TICKS -- ticks from the start of the match during which every
    /// deploy is refused (state.rs `check_deploy_slot`).
    pub deploy_lockout_ticks: i32,
    /// match.TICK_ORDER -- which phase list `tick()` runs (lib.rs `TICK_PHASES`,
    /// the measured order, or `LEGACY_TICK_PHASES`) and, with it, where the
    /// deploy countdown runs (`deploy_countdown`: after Move, or in Upkeep).
    pub tick_order: TickOrder,
    /// movement.DYING_UNIT_VISIBILITY -- whether a troop that dies this tick is
    /// dropped from the scans of the movers after it in the sequential move pass
    /// (`doomed_mask`, `phase_path16402`), or seen by every mover.
    pub dying_unit_visibility: DyingUnitVisibility,
    /// globals.csv MANA_SPEED_UP_WHEN_REMAINING_SECONDS (not in calibration.json).
    pub mana_speed_up_remaining_s: i32,
    /// calibration.json arena.TERRITORY_MODEL. There is no troop pocket depth past
    /// the far bank: the shipped NoDeploySize rects are the mechanic (arena.rs TROOP
    /// TERRITORY). Lives in Calib so a snapshot carries it -- a restored battle must
    /// not change its deploy rule.
    pub territory_model: TerritoryModel,
    /// placement.SNAP_EVEN_CORNER. Added after SNAPSHOT_FORMAT 20; `default` so an
    /// older snapshot still deserializes, into the arm that ships.
    #[serde(default = "placement_snap_even_default")]
    pub placement_snap_even: PlacementSnapEven,
    /// placement.ILLEGAL_TAP. Added after SNAPSHOT_FORMAT 20. The `default` is
    /// `Refuse`, which is what a battle saved before this key actually ran.
    #[serde(default = "placement_illegal_tap_default")]
    pub placement_illegal_tap: PlacementIllegalTap,
    /// placement.TAP_SNAP. Added after SNAPSHOT_FORMAT 20; the `default` is `None`, what a
    /// battle saved before it actually ran.
    #[serde(default = "tap_snap_default")]
    pub placement_tap_snap: TapSnap,
    /// placement.TROOP_TOWER_TAPS. Added after SNAPSHOT_FORMAT 20; the `default` is
    /// `ClosedBlock`, what a battle saved before it actually ran.
    #[serde(default = "troop_tower_taps_default")]
    pub placement_troop_tower_taps: TroopTowerTaps,
    /// spells.ILLEGAL_SPELL_TAP. Added after SNAPSHOT_FORMAT 20; the `default` is `Refuse`,
    /// what a battle saved before it actually ran.
    #[serde(default = "illegal_spell_tap_default")]
    pub illegal_spell_tap: IllegalSpellTap,
    /// movement.ATTACKING_UNIT_MOVEMENT. Added after SNAPSHOT_FORMAT 20. The `default`
    /// is `Frozen`, which is what a battle saved before this key actually ran.
    #[serde(default = "attacking_unit_movement_default")]
    pub attacking_unit_movement: AttackingUnitMovement,
    /// movement.ATTACK_FACING: where a unit in its attack state faces (`phase_attack`). Added after
    /// SNAPSHOT_FORMAT 20; the `default` is the old arm, what a battle saved before it actually ran.
    #[serde(default = "attack_facing_default")]
    pub attack_facing: AttackFacing,
    /// targeting.DOOMED_TARGET_DROP: whether a projectile attacker drops a target the shots in flight
    /// will kill (`phase_target`, target.rs `can_target`). Added after SNAPSHOT_FORMAT 20; the
    /// `default` is the old arm, what a battle saved before it actually ran.
    #[serde(default = "doomed_target_drop_default")]
    pub doomed_target_drop: DoomedTargetDrop,
    /// movement.DEPLOYING_HEADING. Added after SNAPSHOT_FORMAT 20. The `default` is
    /// `Zeroed`, which is what a battle saved before this key actually ran.
    #[serde(default = "deploying_heading_default")]
    pub deploying_heading: DeployingHeading,
    /// movement.WAITING_HEADING. Added after SNAPSHOT_FORMAT 20. The `default` is `Kept`,
    /// which is what a battle saved before this key actually ran.
    #[serde(default = "waiting_heading_default")]
    pub waiting_heading: WaitingHeading,
    /// pathfinding.ZERO_STEP_WAYPOINT_TEST. Added after SNAPSHOT_FORMAT 20. The `default` is
    /// `Skipped`, which is what a battle saved before this key actually ran.
    #[serde(default = "zero_step_waypoint_test_default")]
    pub zero_step_waypoint_test: ZeroStepWaypointTest,
    /// spawner.RELEASE_TIMING. Added after SNAPSHOT_FORMAT 20. The `default` is
    /// `NextSpawnPhase`, which is what a battle saved before this key actually ran. Read
    /// through `BattleState::release_timing`, so the regression plant can force the old arm.
    #[serde(default = "release_timing_default")]
    pub release_timing: ReleaseTiming,
    /// combat.POST_KILL_RETARGET_WAIT (value.arm, value.units, value.ticks). Added after
    /// SNAPSHOT_FORMAT 20; the `default` is `None`, what a battle saved before it actually ran.
    #[serde(default = "post_kill_wait_default")]
    pub post_kill_wait: PostKillWait,
    /// The UNIT names the wait applies to (value.units), matched against a unit's
    /// `CardDef::unit_name` (the game's unit name, not the card's).
    #[serde(default)]
    pub post_kill_wait_units: Vec<String>,
    /// The loss-to-next-target interval, ticks (value.ticks; 6 measured).
    #[serde(default)]
    pub post_kill_wait_ticks: i32,
    /// value.attack_finish_override_units: the UNIT names that never wait under the
    /// attack-finish arm, matched against `CardDef::unit_name`.
    #[serde(default)]
    pub post_kill_wait_override_units: Vec<String>,
    /// combat.DASH_ATTACK: whether a unit with a dash block (card.rs `DashDef`) stands, then dashes
    /// into its target (`phase_path16402`). Added after SNAPSHOT_FORMAT 20; the `default` is the old
    /// arm, what a battle saved before it actually ran.
    #[serde(default = "dash_attack_default")]
    pub dash_attack: DashAttack,
    /// combat.REFLECT_ATTACK: whether a unit whose card carries a reflect (card.rs `ReflectDef`,
    /// the Electro Giant) answers a melee hit on it (`reflect_melee_hit`). Added after
    /// SNAPSHOT_FORMAT 20; the `default` is `NotRead`, what a battle saved before it actually ran.
    #[serde(default = "reflect_attack_default")]
    pub reflect_attack: ReflectAttack,

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
    /// knockback.DISPLACEMENT_LAW -- how a landed push moves the unit (spell.rs
    /// `impact` / `roll` write the matching `Knock` variant; `apply_effects` and the
    /// Path / Move phases consume it).
    pub knock_law: KnockLaw,
    /// knockback.STACKING -- two pushes on one unit (the same tick, or while a
    /// ladder runs). Paired with the law in `from_json`: each law implements one.
    pub knock_stacking: KnockStacking,
    /// pathfinding.MAX_PUSHBACK_LENGTH, NATIVE millitiles: the cap on one push's
    /// length `L` (move16402.rs `start_pushback`). Datamined; consumed by the ladder
    /// arm only.
    pub max_pushback_length: i32,
    /// knockback.DURATION_MS (the fixed_distance arm only: the ladder sets its own
    /// duration from the length).
    pub knock_duration_ms: i32,
    /// knockback.ZERO_VECTOR_DIRECTION.
    pub knock_zero_vector: KnockZeroVector,
    /// knockback.ATTACK_RESET.
    pub knock_attack_reset: KnockAttackReset,
    /// knockback.AFFECTS_DEPLOYING_UNITS.
    pub knock_affects_deploying: bool,
    /// knockback.DIRECTION_ROLLING.
    pub knock_direction_rolling: RollDirection,
    /// spells.PROJECTILE_SPAWN_FORMATION. Before this field the key was read only to refuse
    /// anything but engine_grid; the `default` is that arm.
    #[serde(default = "projectile_spawn_formation_default")]
    pub projectile_spawn_formation: ProjectileSpawnFormation,
    /// knockback.ROLLING_CONTACT_RADIUS, native: the Log's contact radius the
    /// radial_from_contact_point arm reads. `default` 0: read only under that arm, which no
    /// battle saved before this field ran.
    #[serde(default)]
    pub knock_rolling_contact_radius: i32,
    /// status.STUN_ATTACK_TIMER_MODEL.
    pub stun_attack_timer: StunTimerModel,
    /// status.STUN_RETARGET_ON_RESUME.
    pub stun_retarget_on_resume: bool,
    /// status.RESUME_RETARGET_WINDUP.
    pub resume_retarget_windup: ResumeWindup,
    /// combat.RETARGET_PROGRESS.
    pub retarget_progress: RetargetProgress,
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
    /// spawner.TIMER_LEFTOVER: whether a reload replaces the timer's overshoot or is
    /// added to it. Added after SNAPSHOT_FORMAT 20; the `default` is `Dropped`, what a
    /// battle saved before it actually ran.
    #[serde(default = "timer_leftover_default")]
    pub spawner_timer_leftover: TimerLeftover,
    /// spawner.DEATH_SPAWN_AT_EMISSION_POINT (value.arm, value.units): whether a listed
    /// spawner puts its death spawn on its own emission point. Added after SNAPSHOT_FORMAT
    /// 20; the `default` is `None`, what a battle saved before it actually ran.
    #[serde(default = "death_at_emission_default")]
    pub death_spawn_at_emission: DeathAtEmission,
    /// The UNIT names that rule applies to (value.units), matched against the dying
    /// unit's `CardDef::unit_name`.
    #[serde(default)]
    pub death_spawn_at_emission_units: Vec<String>,
    /// spawner.SPAWN_POINT: where a spawner's units appear.
    pub spawner_spawn_point: SpawnPoint,
    /// spawner.STUN_PAUSES_SPAWNER.
    pub spawner_stun_pauses: bool,
    /// spawner.DEATH_SPAWN_RADIUS_DEFAULT: the radius when DeathSpawnRadius is blank.
    pub death_spawn_radius_default: DeathSpawnRadius,
    /// spawner.DEATH_SPAWN_DEPLOY_TIME_DEFAULT: the deploy time when the block is blank.
    pub death_spawn_deploy_default: DeathSpawnDeploy,
    /// spawner.EMISSION_TIMING: where `spawner_pass` runs and whether its units are
    /// created at once or queued for the next tick. Read through
    /// `BattleState::emission_timing`, never directly, so the regression plant can
    /// force the old arm.
    pub spawner_emission_timing: SpawnerEmission,
    /// spawner.SPAWNED_DEPLOY_TIME: the deploy timer a periodic spawner's unit gets.
    pub spawner_spawned_deploy_time: SpawnedDeploy,

    // --- lifetime: calibration.json lifetime.*.
    /// lifetime.HP_DECAY: what LifeTime does to a building's hitpoints.
    pub lifetime_hp_decay: LifetimeDecay,

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
    // --- crown towers. combat.TOWER_HITPOINT_LADDER:
    // how the two tower records scale with the TOWER level (`BattleState::new` ->
    // `spawn_now`, the only site; `tower_multiplier_percent`).
    /// combat.TOWER_HITPOINT_LADDER: the regime.
    pub tower_ladder: TowerLadder,
    /// combat.TOWER_HITPOINT_LADDER value.cap_level: the last tower level stepped at
    /// the per-KING/TOWER rate; every level above it steps at the AT/AFTER_TOURNAMENTCAP
    /// rate (2018 globals TOURNAMENT_MAX_EXP_LEVEL = 9; 15.535 TOWER_SCALING_START_EXP_LEVEL = 9).
    pub tower_ladder_cap_level: i32,
    /// globals.csv HITPOINT_INCREASE_PERCENT_PER_KING_LEVEL / _PER_TOWER_LEVEL /
    /// _AT_TOURNAMENTCAP (the two AT/AFTER king and tower rates are one number in both
    /// vintages) and the three DAMAGE_ counterparts, read from the embedded globals.csv.
    pub tower_hp_pct: TowerPercents,
    pub tower_dmg_pct: TowerPercents,

    // --- the summon formation (formation.rs). Each is one calibration.json
    // formation.* key; a candidate with no implementation is refused in from_json.
    // Read by `formation_members`.
    /// formation.LAYOUT: where a card's N summons stand around the tap.
    pub formation_layout: FormationLayout,
    /// formation.DEPLOY_STAGGER: whether member k waits k x SummonDeployDelay.
    pub formation_deploy_stagger: DeployStagger,
    /// formation.STAGGER_WAIT: whether a member waiting out that stagger can be targeted and
    /// moved. Added after SNAPSHOT_FORMAT 20; the `default` is `Deploying`, what a battle saved
    /// before it actually ran.
    #[serde(default = "stagger_wait_default")]
    pub formation_stagger_wait: StaggerWait,
    /// formation.GROUND_Y_CLAMP: the ground member's y clamp to the tap column's
    /// deployable rows.
    pub formation_ground_y_clamp: GroundYClamp,
    /// formation.GROUND_DEPLOY_POINT: the one-unit offsets a GROUND summon's deploy
    /// point carries. Added in SNAPSHOT_FORMAT 19; `default` (`None`, the offset a
    /// format-3 battle never had) so an older snapshot still deserializes.
    #[serde(default = "ground_deploy_point_none")]
    pub formation_ground_deploy_point: GroundDeployPoint,
    /// globals.csv LOGIC_LANE_ID_BASED_DEPLOY_SEQUENCE (TRUE in both vintages): the
    /// lane mirror of the ring (formation.rs `member_offset`).
    pub lane_id_based_deploy_sequence: bool,

    // --- the reach and the attack cycle. Each is one calibration.json key; a
    // candidate with no implementation is refused in from_json.
    /// targeting.ATTACK_RANGE_RULE: whether the attacker's own radius is part of
    /// its reach (target.rs `in_attack_range`).
    pub attack_range_rule: AttackRangeRule,
    /// combat.ATTACK_CYCLE: the measured progress counter or the old windup.
    pub attack_cycle: AttackCycle,
    /// charge.CHARGED_HIT_TIMING: the charged snap.
    pub charged_hit_timing: ChargedHitTiming,
    /// combat.PROJECTILE_LAUNCH: where a projectile is born and when it first steps.
    pub projectile_launch: ProjectileLaunch,
    /// spawner.DEATH_SPAWN_LAYOUT: where a death spawn's units appear.
    pub death_spawn_layout: DeathSpawnLayout,
    /// spawner.DEATH_SPAWN_PUSHBACK: whether a dying unit whose row sets DeathSpawnPushback
    /// (card.rs `CardDef::death_spawn_pushback`) lays its death spawn on the small fixed ring
    /// and slides it out, instead of by DEATH_SPAWN_LAYOUT. Added after SNAPSHOT_FORMAT 20;
    /// the `default` is `NotRead`, what a battle saved before it actually ran.
    #[serde(default = "death_spawn_pushback_default")]
    pub death_spawn_pushback: DeathSpawnPushback,
    /// targeting.SPAWNED_UNIT_ACQUIRE_DELAY: whether a troop a death spawn creates waits out
    /// its first 7 ticks before an enemy may target it (`BattleState::delay_acquisition`;
    /// target.rs `can_target`). Added after SNAPSHOT_FORMAT 20; the `default` is `None`, what a
    /// battle saved before it actually ran.
    #[serde(default = "spawned_unit_acquire_delay_default")]
    pub spawned_unit_acquire_delay: SpawnedUnitAcquireDelay,
    /// spawner.SPAWNED_FIRST_STEP: whether a unit created after the tick's passes (a periodic
    /// emission at the end of Move, a death spawn at the end of Reap) takes its first update on
    /// that tick (`BattleState::first_update`). Added after SNAPSHOT_FORMAT 20; the `default` is
    /// `None`, what a battle saved before it actually ran.
    #[serde(default = "spawned_first_step_default")]
    pub spawned_first_step: SpawnedFirstStep,

    // --- status effects (status.rs). Each is one calibration.json
    // key; a candidate with no implementation is refused in from_json.
    /// movement.BUFF_SPEED_COMPOSITION: how several SpeedMultipliers combine.
    pub buff_speed_composition: BuffComposition,
    /// combat.HIT_SPEED_BUFF: whether the attack progress counter is scaled by the
    /// HitSpeedMultiplier composition.
    pub hit_speed_buff: HitSpeedBuff,
    /// status.FULL_STOP_BUFF_IS_STUN: a buff whose composed speed is 0 drives `stun_ms`.
    pub full_stop_buff_is_stun: FullStopBuff,
    /// status.BUFF_PULSE_AMOUNT: what one DamagePerSecond / HealPerSecond pulse is worth.
    pub buff_pulse_amount: PulseAmount,
    /// status.BUFF_PULSE_TIMING: when a pulsing buff's first pulse falls.
    pub buff_pulse_timing: PulseTiming,
    /// status.TARGET_BUFF_ON_SPLASH: who a projectile's TargetBuff lands on.
    pub target_buff_on_splash: TargetBuffScope,
    /// spells.PULSING_AREA_EFFECT: when a standing area effect applies.
    pub pulsing_area_effect: PulsingArea,
    /// movement.STOMP_PAUSE_SCHEDULE: the buffable millisecond clock, or the tick index.
    pub stomp_schedule: StompSchedule,
}

impl Calib {
    /// Does a started swing lock this card's target (targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED)? Under
    /// `true` every attacker's does, under "projectile_attackers_only" only an attacker whose card fires a
    /// projectile, and under `false` none.
    #[inline]
    pub fn locks_target(&self, card: &CardDef) -> bool {
        #[cfg(not(clash_plant = "preserve_scope_ignored"))]
        let scoped = self.preserve_target_scope == PreserveTargetScope::ProjectileAttackersOnly;
        #[cfg(clash_plant = "preserve_scope_ignored")]
        let scoped = false; // PLANT (regression): every attacker's swing locks under the scoped value too.
        self.preserve_target_if_hit_started && (!scoped || card.projectile.is_some())
    }

    /// Does an entity in `phase` stand still and keep its target locked as an
    /// attacking unit? Under the measured cycle every attacking unit (the hit tick
    /// included) stands and the move pass skips it; under the old windup only the
    /// windup held (a cooldown walked after a target that left range).
    #[inline]
    pub fn attack_holds(&self, phase: AttackPhase) -> bool {
        match self.attack_cycle {
            AttackCycle::ProgressCredit => phase != AttackPhase::Idle,
            AttackCycle::WindupLoadTime => phase == AttackPhase::Windup,
        }
    }
}

/// The three per-level percents of one tower stat (combat.TOWER_HITPOINT_LADDER).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TowerPercents {
    pub king: i32,
    pub princess: i32,
    pub after_cap: i32,
}

/// A calibration enum: the registry string names each variant, and an unknown string
/// is refused at load rather than run as some other candidate.
/// The `serde` default for `Calib::formation_ground_deploy_point`: a snapshot from
/// before SNAPSHOT_FORMAT 19 ran without the offset.
fn ground_deploy_point_none() -> GroundDeployPoint {
    GroundDeployPoint::None
}

/// The `serde` default for `Calib::placement_snap_even`: the arm that ships.
fn placement_snap_even_default() -> PlacementSnapEven {
    PlacementSnapEven::PlacerFrame
}

/// The `serde` default for `Calib::placement_illegal_tap`: a snapshot saved before
/// the footprint was modelled ran with no relocation at all, so it restores into
/// the behaviour it actually had rather than into the one that ships now.
fn placement_illegal_tap_default() -> PlacementIllegalTap {
    PlacementIllegalTap::Refuse
}

fn tap_snap_default() -> TapSnap {
    TapSnap::None
}

fn troop_tower_taps_default() -> TroopTowerTaps {
    TroopTowerTaps::ClosedBlock
}

fn illegal_spell_tap_default() -> IllegalSpellTap {
    IllegalSpellTap::Refuse
}

fn attacking_unit_movement_default() -> AttackingUnitMovement {
    AttackingUnitMovement::Frozen
}

fn attack_facing_default() -> AttackFacing {
    AttackFacing::Kept
}

fn doomed_target_drop_default() -> DoomedTargetDrop {
    DoomedTargetDrop::Keep
}

fn deploying_heading_default() -> DeployingHeading {
    DeployingHeading::Zeroed
}

fn waiting_heading_default() -> WaitingHeading {
    WaitingHeading::Kept
}

fn zero_step_waypoint_test_default() -> ZeroStepWaypointTest {
    ZeroStepWaypointTest::Skipped
}

fn projectile_spawn_formation_default() -> ProjectileSpawnFormation {
    ProjectileSpawnFormation::EngineGrid
}

fn release_timing_default() -> ReleaseTiming {
    ReleaseTiming::NextSpawnPhase
}

fn post_kill_wait_default() -> PostKillWait {
    PostKillWait::None
}

fn goal_target_position_default() -> GoalTargetPosition {
    GoalTargetPosition::StartOfTick
}

fn flyer_goal_water_default() -> FlyerGoalWater {
    FlyerGoalWater::Demoted
}

fn reflect_attack_default() -> ReflectAttack {
    ReflectAttack::NotRead
}

fn stagger_wait_default() -> StaggerWait {
    StaggerWait::Deploying
}

fn timer_leftover_default() -> TimerLeftover {
    TimerLeftover::Dropped
}

fn death_at_emission_default() -> DeathAtEmission {
    DeathAtEmission::None
}

fn death_spawn_pushback_default() -> DeathSpawnPushback {
    DeathSpawnPushback::NotRead
}

fn spawned_unit_acquire_delay_default() -> SpawnedUnitAcquireDelay {
    SpawnedUnitAcquireDelay::None
}

fn preserve_target_scope_default() -> PreserveTargetScope {
    PreserveTargetScope::AllAttackers
}

fn spawned_first_step_default() -> SpawnedFirstStep {
    SpawnedFirstStep::None
}

fn dash_attack_default() -> DashAttack {
    DashAttack::None
}

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
    /// targeting.CENTRE_LANE_FRAME: which frame decides a unit's lane for default tower
    /// targeting, and which way the centre line itself falls.
    ///
    /// NEITHER ARM IS MEASURED AND NEITHER IS REFUTED, whatever an earlier version of
    /// these comments said. The two rules disagree at exactly one point -- a unit standing
    /// on the centre line on side 0 -- which no tap can produce, and 310,150 corpus
    /// unit-ticks contain no tick where they differ. The ledger entry is `hypothesis` and
    /// says the evidence does not separate them; these comments say the same, so the code
    /// cannot contradict its own ledger.
    CentreLaneFrame {
        /// The ENGINE frame, with `x * 2 >= width` going right, so both seats send a unit
        /// standing exactly on the centre line to the same engine-right tower.
        EngineFrameTieRight = "engine_frame_tie_right",
        /// The attacker's own frame, with the centre going own-left. SHIPPED, because it
        /// is the expression the targeting code hard-coded before this key existed: the
        /// key changed nothing at the value it ships with.
        OwnFrameTieLeft = "own_frame_tie_left",
    }
);
calib_enum!(
    /// status.ATTRACT_LAW: what the AttractPercentage column is a percentage OF. Both
    /// arms are the same arithmetic over a different base speed, which is the whole of
    /// the disagreement the corpus settles.
    AttractBase {
        /// The victim's EFFECTIVE speed: its stored speed through every buff in force and
        /// the charge multiplier (`effective_speed`). REFUTED by a Raged Knight, pulled
        /// 215.9 native per tick where this arm gives trunc(78 * 360 / 100) = 280, and by
        /// an Ice-Wizard-slowed one at 215.7 where it gives 147.
        EffectiveSpeed = "effective_speed",
        /// The victim's STORED speed, no buffs: `Entities::speed`, which for a stomp card
        /// is the Speed column ALREADY stomp-corrected at load (card.rs,
        /// S = floor(Speed * (Stop + Wait) / Stop)). MEASURED on both clients: the Giant
        /// of 20260920-081819 is pulled 187 = tdiv(52 * 360, 100), 52 being its stored
        /// speed, and Raged, slowed, frozen and stunned Knights all at about 216 =
        /// tdiv(60 * 360, 100).
        ///
        /// AN EARLIER VERSION OF THESE COMMENTS called this arm "before the stomp
        /// correction" and REFUTED by that same Giant at 162 = tdiv(45 * 360, 100). That
        /// confused the raw Speed COLUMN (45) with the stored speed this arm reads (52):
        /// for an unbuffed, uncharged unit the two arms were always the same number, so
        /// the Giant never separated them. Rage is the first victim that does.
        BaseSpeed = "base_speed",
    }
);
calib_enum!(
    /// status.ATTRACT_WHILE_HELD: whether a pulling area effect moves a unit whose own
    /// movement is held -- stunned, frozen, or held by its attack under the `frozen` arm
    /// of movement.ATTACKING_UNIT_MOVEMENT.
    AttractWhileHeld {
        /// MEASURED: stunned 216.1, frozen 215.4, attacking a tower 215.5 -- each the
        /// unbuffed Knight's full pull. The hold stops the unit walking; it does not
        /// anchor it.
        Pulled = "pulled",
        /// What the engine did before it was measured: the hold `continue`d past the
        /// move pass that applies the pull, so a held victim was never moved.
        Skipped = "skipped",
    }
);
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
    /// placement.SNAP_EVEN_CORNER: which corner of the tapped tile an EVEN-sized
    /// building's centre takes. Odd sizes take the tile's centre and have no such
    /// choice.
    ///
    /// UNMEASURED. Every even-sized placement in the recordings was made by one
    /// seat, so the two arms fit them equally. The seat-symmetric one ships,
    /// because choosing the other would give a shared policy a one-tile,
    /// seat-dependent offset on no evidence. Deciding observation: a Tesla placed
    /// by the seat that defends high y.
    PlacementSnapEven {
        /// The placer's own lower-left corner, so the two seats mirror.
        PlacerFrame = "placer_frame",
        /// The arena's own lower-left corner, whoever places it.
        Absolute = "absolute",
    }
);
calib_enum!(
    /// placement.ILLEGAL_TAP: what happens to a building tap whose POINT is legal
    /// but whose footprint does not fit.
    ///
    /// The recordings are unambiguous that the game moves it rather than refusing:
    /// of 64 recorded taps, 39 were relocated. They do NOT separate the two
    /// relocating arms, because every relocation they show moves 1 or 2 rings and
    /// the two orders agree below ring 3 (a ring-r diagonal is r * sqrt(2) away
    /// while a ring-(r+1) axial cell is only r + 1). Deciding observation: a tap
    /// whose only fits are 3 or more rings away, with a ring-r diagonal fit and a
    /// ring-(r+1) axial fit both available.
    PlacementIllegalTap {
        /// Square rings outward from the tapped tile; inside the first ring that
        /// holds a fit, the smallest Euclidean distance to the tap wins.
        RelocateFirstFittingRing = "relocate_first_fitting_ring",
        /// The smallest Euclidean distance to the tap over every fitting tile
        /// within the search bound, whichever ring it sits in.
        RelocateNearestOverall = "relocate_nearest_overall",
        /// Refuse, as the engine did before the footprint was modelled.
        Refuse = "refuse",
    }
);
calib_enum!(
    /// placement.TAP_SNAP -- where a troop or spell tap is taken to be.
    TapSnap {
        /// Today's engine: the raw tap (a single unit on its exact subtile, a spell on it).
        None = "none",
        /// Measured (seven single-Knight scenarios on client 15.535.29, the 16.402 corpus's spell
        /// casts): the PLAIN tile centre, tile x 1000 + 500 on both axes; a single GROUND unit
        /// then takes formation.GROUND_DEPLOY_POINT like a ring does.
        TileCentre = "client16402_tile_centre",
    }
);
calib_enum!(
    /// placement.TROOP_TOWER_TAPS -- a troop tap at the owner's own crown tower.
    TroopTowerTaps {
        /// Today's engine: the king block closed on every edge, a tap overlapping the tower
        /// laid where tapped, a single unit not clamped.
        ClosedBlock = "closed_block",
        /// Measured on 48 scenario casts on client 15.535.29: the king block half-open in absolute
        /// coordinates; a troop tap whose tile overlaps an alive own crown tower relocated by
        /// the building ring search; side 1's ground clamp on a single unit too.
        HalfOpenRelocate = "client16402_half_open_relocate",
    }
);
calib_enum!(
    /// spells.ILLEGAL_SPELL_TAP -- a spell tapped outside its territory (the Log).
    IllegalSpellTap {
        /// Today's engine: refused, OUT_OF_TERRITORY.
        Refuse = "refuse",
        /// Measured on client 15.535.29: clamped back along its tile column to the first legal
        /// tile (the own-half boundary row, or the pocket's edge with a princess down).
        ClampToLegalEdge = "client16402_clamp_to_legal_edge",
    }
);
calib_enum!(
    /// knockback.DISPLACEMENT_LAW.
    KnockLaw {
        /// THE SHIPPED LAW, measured step for step on the live Giant's ladder
        /// (calibration knockback.DISPLACEMENT_LAW): a landed push aims a
        /// target `min(Pushback, MAX_PUSHBACK_LENGTH)` away from the source and arms a
        /// speed ladder `25n, 25(n-1), ..., 25, 0, -25` that replaces the walk tick by
        /// tick (move16402.rs). The unit ends `25n(n-1)/2 - 25` short of the target's
        /// distance when the ladder covers it (a 1800 push moves 1600 with the 250
        /// step cap on its first tick).
        Client16402 = "client16402",
        /// The earlier engine: exactly Pushback, instantly (DURATION_MS 0) or
        /// spread evenly over DURATION_MS, through spell.rs `settle`.
        FixedDistance = "fixed_distance",
    }
);
calib_enum!(
    /// knockback.STACKING.
    KnockStacking {
        /// fixed_distance: the displacements of one tick are summed per unit.
        VectorSum = "vector_sum",
        /// client16402: a push is refused while the unit's ladder is active, so the
        /// first push of a tick lands and every later one -- the same tick or a later
        /// tick of the ladder -- is refused outright (a hypothesis paired with the
        /// ladder; no capture lands two pushes on one unit).
        FirstWinsWhileActive = "first_wins_while_active",
    }
);
calib_enum!(
    /// knockback.ZERO_VECTOR_DIRECTION -- the direction of a push whose victim stands
    /// exactly on the source point.
    KnockZeroVector {
        /// Along the caster's forward axis (+y for Blue, -y for Red): commutes with
        /// the seat rotation.
        CasterForward = "caster_forward",
        /// The shipped rule (a hypothesis: no push in the corpus lands on a unit's
        /// centre): `dx = -1 or +1` by the parity of a per-unit id, `dy = 0`, ABSOLUTE
        /// x -- the engine reads the parity of `team_seq` (entity.rs: the sanctioned
        /// id-like quantity, equal for twins), so twins go the same absolute way.
        /// Not seat-symmetric, like the search.
        Client16402XByIdParity = "client16402_x_by_id_parity",
        /// No push at all (nothing armed when `d == 0` under the ladder).
        NoPush = "none",
    }
);
calib_enum!(
    /// knockback.ATTACK_RESET -- what a LANDED push does to the victim's attack
    /// (state.rs `apply_effects`; the hold while the ladder runs is `phase_attack` /
    /// target.rs through `Entities::knocked`).
    KnockAttackReset {
        /// MEASURED on the live 16.402 captures (capture 20260920-081819-B: a Bomber
        /// between hits at tick 2020, a Knight mid-windup at 3535): the push
        /// interrupts the attack whatever its phase -- state 1 through the ladder, the
        /// swing counter zeroed, the load timer back to LoadTime on the hit tick, and a
        /// FRESH LoadTime windup on re-entering range after the ladder; the target kept.
        /// Windup or Cooldown -> Idle here.
        ResetAttackKeepTarget = "reset_attack_keep_target",
        /// The community reading this file shipped before the captures: a windup in
        /// progress returns to Idle, a cooldown is untouched (it freezes through the
        /// ladder and resumes). Kept runnable as the foil.
        ResetWindupKeepTarget = "reset_windup_keep_target",
        /// The windup reset with the target dropped as well.
        ResetWindupClearTarget = "reset_windup_clear_target",
    }
);
calib_enum!(
    /// combat.TOWER_HITPOINT_LADDER -- how the KingTower / PrincessTower records scale
    /// with the tower level (`BattleState::new` -> `spawn_now` -> `tower_multiplier_percent`).
    TowerLadder {
        /// MEASURED on the live 16.402 captures (tests/fixtures/live_levels.json:
        /// king 3312 / 4824 and princess 2030 / 3052 at tower levels 6 / 11; the princess
        /// hit for 109 = 50 x 218 % on the Prince of capture 20260920-003751): the percent
        /// steps ONE LEVEL AT A
        /// TIME with an integer floor after each step, `pct(1) = 100`,
        /// `pct(L) = floor(pct(L-1) x (100 + rate) / 100)`, rate = the per-KING (7) or
        /// per-TOWER (8) globals percent up to `cap_level` and the AT/AFTER_TOURNAMENTCAP
        /// percent (10) above it; the stat is `base x pct / 100`. Reproduces the known
        /// king table 2400, 2568, 2736, 2904, 3096, 3312, ..., 4824, ..., 7032 exactly.
        GlobalsPercentPerLevelCompoundFloor = "globals_percent_per_level_compound_floor",
        /// The card ladder of the record's rarity (Common: x 256 % at 11 -> 6144 / 3584),
        /// what the engine ran before the towers were measured. Kept runnable as the foil.
        CommonCardLadder = "common_card_ladder",
    }
);
calib_enum!(
    /// spells.PROJECTILE_SPAWN_FORMATION -- how a landing spell lays the units it releases.
    /// ring_at_projectile_radius is a listed candidate with no implementation: refused at load.
    ProjectileSpawnFormation {
        /// The centred grid (`formation_grid`), the earlier guess.
        EngineGrid = "engine_grid",
        /// The formation ring (`formation::member_offset`) of `count` touching units of the
        /// released unit's collision radius, one member forward: client 15.535.29's Goblin Barrel
        /// (0, 577), (+-499, -288).
        CountRingTight = "count_ring_tight",
    }
);
calib_enum!(
    /// knockback.DIRECTION_ROLLING.
    RollDirection {
        RadialFromCentre = "radial_from_projectile_centre",
        TravelDirection = "travel_direction",
        /// Away from a source on the roll axis, one disc-sum (knockback.ROLLING_CONTACT_RADIUS
        /// plus the victim's radius) behind the victim along the caster's forward axis: fitted
        /// to the four recorded Log pushes at 4.8 RMS of 520 (the 16.402 corpus).
        RadialFromContactPoint = "radial_from_contact_point",
    }
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
    /// combat.RETARGET_PROGRESS -- what a unit's swing does when its target changes.
    RetargetProgress {
        /// Every change of target restarts the swing.
        ResetAlways = "reset_always",
        /// Replacing a dead target is not a switch: the swing runs on.
        KeepWhenDead = "keep_when_dead",
        /// As keep_when_dead, and a switch from a live target to one already in attack range keeps the
        /// swing too: its progress runs on and the next hit lands on the new target on the running cycle.
        /// Measured on client 15.535.29: a Knight whose Hog Rider leaves its reach mid-swing takes a Cannon in
        /// reach on the next tick and hits it on the tick its swing at the Hog would have landed. A switch
        /// to a target out of range still clears the swing.
        KeepWhenDeadOrInReach = "keep_when_dead_or_in_reach",
    }
);
calib_enum!(
    /// Which attackers targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED reaches (`Calib::locks_target`).
    PreserveTargetScope {
        /// Every attacker: a started swing locks its target to Range +
        /// LOGIC_CANCEL_HIT_FROM_LONG_DISTANCE_RANGE.
        AllAttackers = "all_attackers",
        /// Only an attacker whose card fires a projectile (troops, buildings, crown towers). It holds its
        /// target, on every tick, while the target's start-of-tick centre distance is within Range + both radii
        /// + target::PROJECTILE_HOLD_BEYOND_REACH and it has not just launched at it from beyond its reach;
        /// otherwise it rescans, without cancelling its shot. A direct striker's swing does not lock: it keeps
        /// a target within Range + LOGIC_RANGE_EXTENSION_TO_KEEP_TARGET and rescans past it.
        ProjectileAttackersOnly = "projectile_attackers_only",
    }
);

/// One live PULLING area effect (status.ATTRACT_LAW), reduced to what `phase_path16402`'s
/// attract pre-pass needs: where it is, how far it reaches, whose it is, how hard it pulls,
/// and the four eligibility flags its own damage test already uses. Named rather than left
/// as the eight-wide tuple it was, which no reader could keep in order.
struct AttractSource {
    pos: Vec2,
    radius: i32,
    team: Team,
    pct: i32,
    hits_air: bool,
    hits_ground: bool,
    ignore_buildings: bool,
    only_enemies: bool,
}
calib_enum!(
    /// status.BUFF_EXPIRY_TICK_ALIGNMENT.
    BuffExpiry { CeilFromNextTick = "ceil_from_next_tick", OneTickShort = "one_tick_short" }
);
calib_enum!(
    /// movement.BUFF_SPEED_COMPOSITION -- how the SpeedMultipliers a unit is carrying
    /// combine (status.rs `compose`).
    BuffComposition {
        /// The strongest speed-up and the strongest slow, in that order, each a
        /// truncating percent (the single-buff case measured on the live raged Ice
        /// Golem, 52 -> 67).
        StrongestUpAndDown = "strongest_up_and_down",
        /// movement.BUFF_SPEED_RULE's single-buff reading: floor(S x m / 100) for the
        /// FIRST multiplier found. Identical with at most one buff on the unit.
        SingleMultiplierFloor = "single_multiplier_floor",
    }
);
calib_enum!(
    /// combat.HIT_SPEED_BUFF -- whether the attack progress counter advances by the
    /// composed HitSpeedMultiplier or by TICK_MS.
    HitSpeedBuff { ProgressScaled = "progress_scaled", None = "none" }
);
calib_enum!(
    /// status.FULL_STOP_BUFF_IS_STUN -- a buff whose composed speed is 0 (all three
    /// -100 columns) sets the engine's one hold timer, so every existing
    /// status.STUN_* key keeps its meaning.
    FullStopBuff { StunTimer = "stun_timer", BuffOnly = "buff_only" }
);
calib_enum!(
    /// status.BUFF_PULSE_AMOUNT -- what one pulse of a DamagePerSecond / HealPerSecond
    /// buff is worth (status.rs `BuffDef::pulse_base`).
    PulseAmount { PerSecondTimesFrequency = "per_second_times_frequency", PerPulse = "per_pulse" }
);
calib_enum!(
    /// status.BUFF_PULSE_TIMING -- when a pulsing buff's FIRST pulse falls.
    PulseTiming { AfterFirstPeriod = "after_first_period", OnApplication = "on_application" }
);
calib_enum!(
    /// status.TARGET_BUFF_ON_SPLASH -- who a projectile's TargetBuff lands on.
    TargetBuffScope { WholeSplash = "whole_splash", PrimaryTargetOnly = "primary_target_only" }
);
calib_enum!(
    /// spells.PULSING_AREA_EFFECT -- when a standing area effect applies.
    PulsingArea { FromLanding = "hit_speed_period_from_landing", Delayed = "hit_speed_period_delayed" }
);
calib_enum!(
    /// targeting.DOOMED_TARGET_DROP -- what an attacker does with a target that the shots already in
    /// flight at it will kill.
    DoomedTargetDrop {
        /// Keeps it like any other target.
        Keep = "keep",
        /// Measured on client 15.535.29: an attacker whose card fires a projectile (troops, buildings
        /// and crown towers) and that has not launched a shot at its target since acquiring it drops
        /// the target on the tick after it is doomed, cancelling its windup, and does not take it back
        /// while it lives. Doomed: the shots in flight at it cover its hitpoints, and the one that lands
        /// last does so within target.rs DOOMED_ETA_LIMIT_MS. An attacker with no projectile, and one
        /// that has fired at the target, keeps it.
        ProjectileAttackers = "projectile_attackers",
        /// As projectile_attackers, except that the fired-at exemption covers keeping only: a rescan
        /// never takes a doomed unit, even one the attacker has shot at (target.rs `can_target`,
        /// `keeping`). Measured on client 15.535.29: after a launch from beyond reach at a doomed
        /// target, the re-evaluation dropped it in 4 of 4 cases and took it back in none.
        ProjectileAttackersRescan = "projectile_attackers_rescan",
    }
);
impl DoomedTargetDrop {
    /// Whether doomed targets are dropped at all (every arm but keep).
    pub fn drops(self) -> bool {
        self != DoomedTargetDrop::Keep
    }
}
calib_enum!(
    /// movement.ATTACK_FACING -- where a unit faces while it is in its attack state. A walking
    /// unit faces along its route under either value (the move pass writes that).
    AttackFacing {
        /// The heading of its last walking tick stays through the attack.
        Kept = "kept",
        /// Measured on client 15.535.29: on every tick in its attack state it faces its target, the
        /// move law's integer normalize (length 256) of target - self on the start-of-tick positions.
        TowardTarget = "toward_target",
    }
);
calib_enum!(
    /// movement.ATTACKING_UNIT_MOVEMENT -- what happens to a unit whose attack phase
    /// holds it. Both arms agree it does not WALK and does not steer; they differ on
    /// whether the contact scans still reach it.
    AttackingUnitMovement {
        /// The earlier arm, shipped until 2026-09-24: the move pass skips the unit
        /// entirely, so it is not separated from anything and a neighbour cannot push it
        /// out. The corpus rules it out: of the 11 755 attacking ticks where a unit
        /// overlaps a neighbour it moves on 8 401, against 3.6 per cent of the 117 183
        /// ticks where it is clear.
        Frozen = "frozen",
        /// The measured arm, shipped since 2026-09-24: no walk, no avoidance steer, no
        /// stomp clock, and the separation scan and offset decay run as they do for an
        /// in-range unit. The route is cleared, as it is on any transition to attacking
        /// (SPEC 5.3).
        SeparationOnly = "separation_only"
    }
);
calib_enum!(
    /// movement.DEPLOYING_HEADING -- what a DEPLOYING neighbour's heading counts for in a
    /// walker's avoidance vote, where a moving neighbour facing the walker's way (dot > 0)
    /// is not a blocker.
    DeployingHeading {
        /// The measured arm: a deploying unit keeps the forward heading it spawned with,
        /// so a same-facing walker is not deflected by it.
        Kept = "kept",
        /// The earlier reading: a deploying unit's heading is zeroed, the dot is 0, and
        /// the walker counts it as a blocker and steers round it.
        Zeroed = "zeroed"
    }
);
calib_enum!(
    /// movement.WAITING_HEADING -- what the heading of a member still WAITING OUT ITS STAGGER
    /// (formation.STAGGER_WAIT's measured arm, entity.rs `stagger_ms` > 0) counts for in a
    /// walker's avoidance vote. A deploying member is DEPLOYING_HEADING's, not this key's.
    WaitingHeading {
        /// The engine before this key flipped: a waiting member is a deploying unit here too (DEPLOYING_HEADING).
        Kept = "kept",
        /// Measured on client 15.535.29's walking summon-push scenarios: the waiting member's
        /// heading is zeroed, so a walker counts it as a blocker and steps away from it.
        Zeroed = "zeroed"
    }
);
calib_enum!(
    /// pathfinding.ZERO_STEP_WAYPOINT_TEST -- whether a unit walking its route on a tick whose
    /// step is zero (a stomp pause) still freezes a new segment and runs the reached test
    /// (pathfinding.WAYPOINT_ARRIVE_RULE). A knockback ladder tick is not a walk tick and is
    /// not this key's: `pushback_step` has no reached test under either value.
    ZeroStepWaypointTest {
        /// The engine before this key: both wait for a tick with a nonzero speed.
        Skipped = "skipped",
        /// Measured on client 15.535.29 and client 16.402: a zero-step walk tick pops a
        /// reached node and refreezes the segment from the unmoved position.
        Run = "run"
    }
);
calib_enum!(
    /// movement.STOMP_PAUSE_SCHEDULE -- the stomp pause's clock.
    StompSchedule {
        /// A millisecond clock (entity.rs `stomp_clock`), whose per-tick advance is
        /// the composed speed buff: 50 unbuffed, 65 under Rage (measured on the live
        /// raged Golem, 168 of 169 walking ticks).
        MsClock = "ms_clock",
        /// The measured tick-index form (entity.rs `move_ticks`), identical unbuffed.
        TickIndexMod = "k_plus_1_times_tick_ms_mod_period_strictly_greater_than_stop",
    }
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
    /// spawner.TIMER_LEFTOVER -- what an emission's reload does with the timer's value,
    /// which is 0 or below when the spawner emits (`spawner_pass`).
    TimerLeftover {
        /// Today's engine: the reload replaces it, so a blank-start spawner's first
        /// overshoot (0 to -TICK_MS on its activation tick) is dropped.
        Dropped = "dropped",
        /// Measured on the 16.402 corpus (Tombstone, Barbarian Hut): the reload is
        /// added to it, so the overshoot carries into the next wait.
        Carried = "client16402_carried",
    }
);
calib_enum!(
    /// spawner.DEATH_SPAWN_AT_EMISSION_POINT -- where a LISTED spawner's death spawn appears.
    DeathAtEmission {
        /// Today's engine: DEATH_SPAWN_RADIUS_DEFAULT and DEATH_SPAWN_LAYOUT for every unit.
        None = "none",
        /// Measured on the 16.402 corpus for the listed units: every member on the point
        /// the unit's spawner emits at, all together. Unlisted units keep today's rules.
        MeasuredList = "client16402_measured_list",
    }
);
calib_enum!(
    /// spawner.SPAWN_POINT -- the spawner's centre plus its own collision radius (or
    /// SpawnRadius when set) along the owner's forward axis, or its centre.
    SpawnPoint {
        InFrontAtOwnRadius = "in_front_toward_enemy_at_own_radius",
        AtCentre = "at_centre",
        /// client16402_measured with one change: a set SpawnAngleShift's ring turns at the spawner's facing ROUNDED
        /// to a whole degree (formation.rs `rounded_degree`), where the measured arm takes the 1024 table's argmax.
        /// Measured on client 15.535.29: 7 of 7 Night Witch two-Bat emissions exact, against 6 of 7.
        ClientRoundedFacingDegree = "client_rounded_facing_degree",
        /// THE MEASURED LAW, and it splits on whether the card sets SpawnRadius.
        ///
        /// BLANK SpawnRadius: the emission is FORWARD, at the two circles' TANGENT,
        /// the spawner's own radius plus the spawned unit's. A Tombstone's Skeleton
        /// leaves at 1000 + 500 = 1500, which 849 of its 879 recorded emissions sit
        /// within 250 of, against 0 of 879 for either older arm. The Barbarian Hut
        /// agrees independently by the same arithmetic.
        ///
        /// SpawnRadius SET: the emission is on a RING of that radius and is NOT
        /// forward at all. The Witch (2000) has a median centre distance of 1940 and
        /// a median FORWARD offset of 74; the Dark Witch (1500), 1490 and 120. The
        /// older arm put this case forward too, so it was wrong in DIRECTION there
        /// rather than in magnitude.
        ///
        /// THE RING'S ANGLE IS TWO LAWS, which is why this is one arm and not two:
        /// a card with a blank SpawnAngleShift lays its ring out in the ABSOLUTE
        /// frame (the Witch's four Skeletons sit on 0/90/180/270 every wave while
        /// her own facing spans 68 to 110 degrees), and a card that sets one lays it
        /// out relative to its own FACING (the Dark Witch's two Bats hold 91/280,
        /// 87/277, 91/268 to her facing while their absolute angles swing 130
        /// degrees).
        Client16402Measured = "client16402_measured"
    }
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
    /// spawner.EMISSION_TIMING -- where the periodic spawner pass runs and when the
    /// unit it emits comes into existence.
    SpawnerEmission {
        /// Measured (44 live Tombstone deploys, 2 BarbarianHut deploys): the pass
        /// runs in the Move phase right after `deploy_countdown` -- the same place as
        /// the countdown, after the move pass -- and CREATES its units in that same
        /// tick, so a Tombstone's first Skeleton exists on the Tombstone's own
        /// deploy-end tick, as the recordings show.
        MovePhaseImmediate = "move_phase_immediate",
        /// The earlier engine: the pass ran at the end of the Spawn phase and pushed
        /// PendingSpawns that materialised in the NEXT tick's Spawn phase. Two ticks
        /// late on every live deploy, because the activation happened after that
        /// tick's Spawn.
        SpawnPhaseNextTick = "spawn_phase_next_tick",
    }
);
calib_enum!(
    /// spawner.RELEASE_TIMING -- when units RELEASED by an event come into existence: a
    /// spell's SpawnCharacter (the Goblin Barrel's goblins) and a death spawn. Not the
    /// periodic spawner (spawner.EMISSION_TIMING) and not a deploy.
    ReleaseTiming {
        /// Measured on both clients: the units exist on the event's own frame and are
        /// INERT on it -- no deploy countdown and no step until the next tick. Created at
        /// the end of the tick's Reap phase, after the dead are despawned and before the
        /// hash is rebuilt, in release order: the tick's spell releases, then its deaths.
        EndOfEventPhase = "end_of_event_phase",
        /// The earlier convention: queued, and created in the NEXT tick's Spawn phase,
        /// where the countdown also runs -- one tick late on every release.
        NextSpawnPhase = "next_spawn_phase",
    }
);
calib_enum!(
    /// combat.POST_KILL_RETARGET_WAIT -- what a unit does in the ticks after its target dies.
    PostKillWait {
        /// The engine before the wait: the next Target phase takes the next target (the loss + 1).
        None = "none",
        /// Measured on both clients for the LISTED units: held as attacking with no target and
        /// standing still, the attack timer frozen and zeroed on the loss + 5, the next target
        /// taken on the loss + 6 even with another enemy already in range. Unlisted units take
        /// the next target on the loss + 1, as under `None`. It shipped first; the condition,
        /// `AttackFinish`, explains the list and replaced it as the shipped arm.
        MeasuredList = "client16402_measured_list",
        /// The CONDITION (measured per event on the 16.402 corpus, 1,138 of 1,152): the wait
        /// is skipped when (a) the unit is on value.attack_finish_override_units (six of the
        /// 15.535 OverrideAttackFinishTime units; the column also marks the Little Prince and a
        /// hero form, see the ledger's open), (b) its attack progress is 0 at the loss, or (c) its
        /// card has a projectile and its victim was doomed on its last live tick
        /// (entity.rs `target_doomed`); every other unit waits, whatever its name. It is the
        /// shipped arm (tests/post_kill_wait.rs pins it).
        AttackFinish = "client16402_attack_finish",
    }
);
calib_enum!(
    /// combat.DASH_ATTACK -- whether a unit whose card has a dash block (the Bandit, the Mega Knight)
    /// dashes into its target.
    DashAttack {
        /// The dash block is not read: the unit walks into range and attacks like any melee unit.
        None = "none",
        /// Measured on client 15.535.29 (the Bandit against a Knight, a Giant and a princess tower;
        /// the Mega Knight against a Giant). A unit walking after its target stands from the first
        /// tick whose start-of-tick centre distance is at most DashMaxRange + the target's radius,
        /// unless that target was nearer than DashMinRange edge to edge when first seen (it walks in).
        /// It enters the dash DashCooldown / 50 - 1 ticks later and fixes its goal: the centre of the
        /// 500-cell holding the point (own radius + target radius) short of the target. A dash with
        /// no DashConstantTime (the Bandit) is still on that first tick, then moves in two half-steps
        /// of JumpSpeed / 2 a tick, stopping after the first whose edge gap to the target's
        /// start-of-tick position is within its Range; that tick deals DashDamage and ends the dash.
        /// A dash with one (the Mega Knight) moves JumpSpeed a tick to the goal, deals DashDamage
        /// over DashRadius DashConstantTime / 50 ticks after the entry and ends 4 ticks after that.
        /// Damage on a dash whose row sets DashImmuneToDamageTime is discarded while it dashes and
        /// for that long after. The attack cycle then restarts from its load time. Run in the
        /// 16.402 move pass only.
        ClientDash = "client_dash",
    }
);
calib_enum!(
    /// pathfinding.GOAL_TARGET_POSITION -- the target centre a chaser's goal cell is chosen around
    /// (the cells within its Range + own CollisionRadius of that centre, the nearest one to the
    /// chaser winning; path16402.rs `choose_goal_cell`).
    GoalTargetPosition {
        /// The target's position at the start of the tick, for every chaser.
        StartOfTick = "start_of_tick",
        /// Measured on client 15.535.29: the target's centre as the creation-order move pass holds
        /// it at the chaser's turn, so already moved this tick when the target was created before
        /// the chaser and not yet moved when it was created after. Flyers and ground chasers alike.
        CreationOrder = "creation_order",
    }
);
calib_enum!(
    /// pathfinding.FLYER_GOAL_WATER -- how a FLYING chaser's goal choice ranks a water cell. A ground
    /// chaser ranks water below dry ground under either value.
    FlyerGoalWater {
        /// Below dry ground, as for a ground chaser: the flyer heads for the nearest DRY cell in reach.
        Demoted = "demoted",
        /// Measured on client 15.535.29: like dry ground, so the flyer heads for the nearest cell in
        /// reach, over the river or not. A cell a building boxes stays below both.
        NotDemoted = "not_demoted",
    }
);
calib_enum!(
    /// combat.REFLECT_ATTACK -- what a unit whose card carries a reflect (card.rs `ReflectDef`:
    /// the Electro Giant) does when a melee hit lands on it.
    ReflectAttack {
        /// Today's engine: nothing. The ReflectedAttack columns load and are not run.
        NotRead = "not_read",
        /// Measured on client 15.535.29 (its Electro Giant scenario): each melee hit landed on
        /// him by an attacker whose edge is within ReflectedAttackRadius of his centre is
        /// answered IN THE SAME TICK by ReflectedAttackDamage at his level on the attacker (a
        /// level-11 Knight takes 192) and ReflectedAttackBuff for ReflectedAttackBuffDuration,
        /// a stun that holds the attacker's attack progress for 9 ticks without resetting it
        /// (the Knight's hit period goes from 24 ticks to 33). A shot is never answered: the
        /// measured princess towers fired from beyond the reach, and a ranged attacker or a
        /// tower inside it is the ledger entry's open item (`reflect_melee_hit`).
        ClientReflectStun = "client_reflect_stun",
    }
);
calib_enum!(
    /// spawner.SPAWNED_DEPLOY_TIME -- whether a periodic spawner's unit serves its
    /// own DeployTime. Measured zero on 1230 live emissions.
    SpawnedDeploy { Zero = "zero", UnitOwnDeployTime = "unit_own_deploy_time" }
);
calib_enum!(
    /// lifetime.HP_DECAY -- what a building's LifeTime does to its hitpoints.
    LifetimeDecay {
        /// Measured (92.2 % of 31113 live building frames exact, every residual a
        /// hit the building took): the building bleeds
        /// `((max_hp * 100000) / LifeTime_ms) / 20` hundredths of a hitpoint every
        /// tick past its deploy end and dies when the pool runs out.
        LinearDrain = "linear_drain",
        /// The earlier engine: full hp for the whole life, then one hit for
        /// everything it has when LifeTime is up.
        ExpiryHit = "expiry_hit",
    }
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
    /// movement.JUMP_WATER_HOP -- see `Calib::jump_water_hop`.
    JumpWaterHop {
        Client16402 = "client16402",
        WalkPricedWater = "walk_priced_water",
    }
);
calib_enum!(
    /// match.TICK_ORDER -- see `Calib::tick_order` and lib.rs `TICK_PHASES`.
    TickOrder {
        /// The measured order: Target, Attack, Path, Move, the countdown after Move.
        Client16402 = "client16402",
        /// The earlier order: Path, Move, Attack, the countdown in Upkeep.
        LegacyMoveBeforeAttack = "legacy_move_before_attack",
    }
);
calib_enum!(
    /// movement.DYING_UNIT_VISIBILITY -- see `Calib::dying_unit_visibility`.
    DyingUnitVisibility {
        /// Seen by the movers before it in creation order, dropped for the ones
        /// after it (measured 99 : 5 / 35 : 0).
        CreationOrderBeforeVictim = "creation_order_before_victim",
        /// Seen by every mover for the whole pass (the earlier engine).
        WholeTick = "whole_tick",
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

calib_enum!(
    /// formation.LAYOUT -- see `BattleState::formation_members` and formation.rs.
    FormationLayout {
        /// The ring / line / spiral of formation.rs, computed in the owner's frame;
        /// measured on every clean multi-unit formation of the live 16.402 corpus
        /// (78 groups member by member, 197 members exact).
        Client16402 = "client16402",
        /// The earlier engine: a centred square grid of one collision diameter
        /// (`formation_grid`), the second summon on the same grid. Kept runnable as
        /// the refuted arm (500-1500 native off on every swarm).
        EngineGrid = "engine_grid",
    }
);
calib_enum!(
    /// formation.DEPLOY_STAGGER -- see formation.rs `stagger_ms`.
    DeployStagger {
        /// Member k >= 1 waits k x SummonDeployDelay ms (the second summon's j-th
        /// member (j + 1) x SummonDeployDelaySecond when the first is blank; a
        /// summoned building the flat delay) before its DeployTime starts; measured
        /// on the corpus' deploy-end ticks (Goblins 200 ms apart, the Rascals' Girls
        /// on the second delay).
        Client16402 = "client16402",
        /// Every member deploys on the same tick (the earlier engine).
        None = "none",
    }
);
calib_enum!(
    /// formation.STAGGER_WAIT -- what a member is while it waits out its DEPLOY_STAGGER wait
    /// (entity.rs `stagger_ms` > 0).
    StaggerWait {
        /// Today's engine: the wait is part of the deploy, so a waiting member is targeted and
        /// moved by the separation scan like any deploying unit.
        Deploying = "deploying",
        /// Measured on the 16.402 corpus: no attacker can target it (target.rs `can_target`) and
        /// the 16.402 move pass leaves it where it is. Its body still stands in the way, so a unit
        /// overlapping it is pushed off it. When the wait ends it is an ordinary deploying unit.
        Client16402 = "client16402_untargetable_immovable",
    }
);
calib_enum!(
    /// formation.GROUND_Y_CLAMP -- see `BattleState::ground_y_range`.
    GroundYClamp {
        /// The measured per-side range: a ground member's absolute y clamped into
        /// [lowest deployable row's near edge, highest one's centre] of the tap's
        /// tile column for side 0 and, for side 1, [lowest row's centre - 1, highest
        /// row's near edge] -- which is not the rotation of side 0's (one native unit
        /// LOOSER at the river, a full row shorter at the back edge) -- unless the
        /// range spans half the arena or more. Measured live on both of side 1's
        /// bounds: the Red Goblins of capture 20260918-164951-B hold their rear pair
        /// on 31000, not the 31261 the rotated formula gives, and those of capture
        /// 20260918-121158 t2439 stand on the river bound's 17499, not the rotation's
        /// 17500. Side 0's range is the formula's; no clean corpus group pins it.
        Client16402DeployColumnRange = "client16402_deploy_column_range",
        /// Side 0's formula in the OWNER's frame for both seats: the seat-symmetric
        /// arm (tests/common `symmetric_config`), within one native unit of the
        /// measurement at the river and a Red back-row deploy's rear members up to
        /// 750 native short of where the corpus holds them.
        DeployColumnRangeOwnFrame = "deploy_column_range_own_frame",
        /// No clamp: a member past the bank is water-ejected like a release.
        None = "none",
    }
);

calib_enum!(
    /// formation.GROUND_DEPLOY_POINT -- see `BattleState::formation_members`.
    GroundDeployPoint {
        /// The measured one-unit offsets a GROUND summon's deploy point carries and
        /// a flying one does not: absolute x one lower when the tap is on the
        /// arena's LEFT half (either seat), absolute y one lower when the owner is
        /// side 1 (either half). Applied to the point the ring is laid around, so
        /// the column clamp's own bounds are untouched by it.
        Client16402OneUnit = "client16402_one_unit",
        /// No offset: the ring is laid around the tap itself, which is what a FLYING
        /// summon measures on both seats and in both halves. The seat-symmetric arm
        /// (tests/common `symmetric_config`).
        None = "none",
    }
);

calib_enum!(
    /// spawner.DEATH_SPAWN_LAYOUT -- see `BattleState::death_spawn_points`.
    DeathSpawnLayout {
        /// The ring on the dying unit's FACING (measured on the live Battle Rams):
        /// member k at DeathSpawnRadius from the death point, at the facing rotated
        /// by SpawnAngleShift + k x 360 / count, through formation.rs's sine table;
        /// water-ejected like a release.
        FacingRing = "facing_ring",
        /// The engine grid around the death point, pulled back onto the radius (the
        /// earlier engine; refuted on the Battle Ram's Barbarians, 300-500 native off).
        EngineGridWithinRadius = "engine_grid_within_radius",
        /// facing_ring with two changes, measured on client 15.535.29 (17 Battle Ram deaths): the ring lies at the
        /// facing's angle ROUNDED to a whole degree (formation.rs `rounded_degree`; 13 of 17 exact, the other 4
        /// a raged or cursed Ram one degree off), member k at the radius through the sine table at that degree +
        /// SpawnAngleShift + k x 360 / count; and every member starts with the dying unit's heading (17 of 17), the
        /// facing normalized to length 256, not its side's forward.
        FacingRingRounded = "facing_ring_rounded",
    }
);
calib_enum!(
    /// spawner.DEATH_SPAWN_PUSHBACK -- see `BattleState::death_spawn_points` (`fixed_slide_ring`) and the
    /// slide in the Path phase (`move16402::death_slide_to`).
    DeathSpawnPushback {
        /// Today's engine: the column is not acted on, and every death spawn is laid by
        /// spawner.DEATH_SPAWN_LAYOUT.
        NotRead = "not_read",
        /// For a dying unit whose row sets DeathSpawnPushback: member k of n is born
        /// `move16402::DEATH_SLIDE_START` (250) native from the death point at the fixed
        /// angle -(k + 1) x 360 / n in the native frame (0 = +x), whatever the parent's
        /// heading; each Path phase then moves it `move16402::DEATH_SLIDE_STEP` (250)
        /// straight out, neither walking nor attacking, until it is exactly DeathSpawnRadius
        /// from the death point, and its ordinary update starts the next tick. Measured on
        /// client 16.402 (3 Golem deaths, 1 Lava Hound death, side 0): the Golemites'
        /// radius per tick 250, 650, 900, 1150, 1400, 1500 and 250, 601, 851, 1101, 1351,
        /// 1500. A row that leaves the column blank (the Battle Ram) keeps
        /// DEATH_SPAWN_LAYOUT.
        ClientRingSlide = "client_ring_slide",
    }
);
calib_enum!(
    /// targeting.SPAWNED_UNIT_ACQUIRE_DELAY -- see `BattleState::delay_acquisition` (the one
    /// setter) and target.rs `can_target` (the one reader).
    SpawnedUnitAcquireDelay {
        /// The engine before this key flipped: a death spawn is an ordinary target from the first Target phase
        /// after it appears (the tick after its first frame under spawner.RELEASE_TIMING =
        /// end_of_event_phase).
        None = "none",
        /// A TROOP created by a death spawn (a troop's or a building's DeathSpawnCharacter)
        /// is nobody's target before its 8th frame: F being its first tick, the Target phase
        /// of F + 7 (`target::ACQUIRE_DELAY_TICKS`) is the first that may give it to an
        /// enemy. Measured on client 15.535.29: 35 death spawns first targeted on exactly
        /// F + 7 and none on F + 1 to F + 6 (the Goblin Cage's Brawler, the Golemites of both
        /// seats). It is not the DeployDelay column, which the Golemite row lacks. Targeted at
        /// once, as measured: a hand-played troop (on its first frame), a Tombstone's periodic
        /// Skeleton (on F + 1) and the Goblin Drill's building (on F + 1; not a death spawn).
        /// The engine exempts more than was measured: every periodic spawner's troops and
        /// every building (`delay_acquisition`). Area damage still lands on the unit meanwhile,
        /// because it is not a target scan; that is the engine's reading, not a measurement.
        Client8thFrame = "client_8th_frame",
    }
);
calib_enum!(
    /// spawner.SPAWNED_FIRST_STEP -- see `BattleState::first_update`.
    SpawnedFirstStep {
        /// The engine before this key flipped: a unit created after the tick's passes stands where it was created
        /// until the next tick's passes.
        None = "none",
        /// It takes its whole first update on the tick it is created: it acquires, enters its
        /// attack when a target is in range, and otherwise takes one move step through the
        /// ordinary walk and contact law against the units already on the board. Measured on
        /// client 15.535.29: the first Skeleton of 8 of 8 Tombstone waves stands one Skeleton step
        /// from the emission point on its first frame, and a dying Tombstone's four Skeletons
        /// stand together on one point one step past it.
        SameTick = "client16402_same_tick",
    }
);
calib_enum!(
    /// targeting.ATTACK_RANGE_RULE -- see target.rs `in_attack_range`.
    AttackRangeRule {
        /// Range + the attacker's CollisionRadius + the target's, centre to centre
        /// (measured: the live Prince stops 3135 native from the princess tower's
        /// centre, the first step inside 1600 + 600 + 1000).
        RangePlusBothRadii = "range_plus_both_radii",
        /// The earlier engine: Range + the target's radius only (every unit walked
        /// its own radius too far). Kept runnable as the refuted arm.
        RangePlusTargetRadius = "range_plus_target_radius",
    }
);
calib_enum!(
    /// combat.ATTACK_CYCLE -- see combat.rs `attack_step`.
    AttackCycle {
        /// The measured progress counter: a fresh cycle is credited LoadTime, hits
        /// land on every multiple of HitSpeed, the load timer's remainder is taken
        /// off a re-entry's credit (combat.rs `attack_step_progress`).
        ProgressCredit = "progress_credit",
        /// The earlier engine: a LoadTime windup, the hit, then HitSpeed - LoadTime
        /// of cooldown. Refuted on every live first hit; kept runnable.
        WindupLoadTime = "windup_load_time",
    }
);
calib_enum!(
    /// charge.CHARGED_HIT_TIMING -- see combat.rs `attack_step_progress`.
    ChargedHitTiming {
        /// The progress counter snaps to the next HitSpeed multiple: the charged hit
        /// lands on the first attack pass that finds the target in range (measured:
        /// the live Prince's tower is already hit on its first attacking frame).
        FirstAttackPassNoWindup = "first_attack_pass_no_windup",
        /// No snap: the charged unit hits on the ordinary first-hit tick of
        /// combat.ATTACK_CYCLE (the LoadTime windup under windup_load_time, the
        /// earlier engine).
        AfterLoadTimeWindup = "after_load_time_windup",
    }
);
calib_enum!(
    /// combat.PROJECTILE_LAUNCH -- see combat.rs `fire` / `step_projectiles`.
    ProjectileLaunch {
        /// Born ProjectileStartRadius from the attacker toward the target, first step
        /// the next tick (measured on the live tower arrows).
        StartRadiusNextTick = "start_radius_next_tick",
        /// Born at the attacker's centre and stepped in the fire tick's own
        /// Projectile phase (the earlier engine).
        AttackerCentreSameTick = "attacker_centre_same_tick",
    }
);

/// combat.DASH_ATTACK = client_dash: a dash with a DashConstantTime (the Mega Knight's) ends this many
/// ticks after its blow. Measured on client 15.535.29 on the Mega Knight only (2 of 2 jumps: the
/// dash state lasts 20 ticks from the entry, the blow on the entry + 16); no formula from its row
/// (DashConstantTime 800, DashLandingTime 300) gives the 4, so it is a named constant.
pub const DASH_BLOW_TO_END_TICKS: u32 = 4;

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

/// targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED's value: `true` or `false` for every attacker, or the string
/// "projectile_attackers_only" (the lock on, scoped to projectile attackers).
fn preserve_value(v: &Value) -> Result<(bool, PreserveTargetScope), String> {
    let path = ["targeting", "LOGIC_PRESERVE_TARGET_IF_HIT_STARTED", "value"];
    match at(v, &path)? {
        Value::Bool(b) => Ok((*b, PreserveTargetScope::AllAttackers)),
        Value::String(s) if s.as_str() == "projectile_attackers_only" => Ok((true, PreserveTargetScope::ProjectileAttackersOnly)),
        other => Err(format!("calibration.json: {} is {other}, not true, false or \"projectile_attackers_only\"", path.join("."))),
    }
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

/// A globals.csv BooleanValue by name ("TRUE" / "FALSE"; a blank is FALSE, the
/// Supercell loader's default).
fn globals_bool(name: &str) -> Result<bool, String> {
    let (h, rows) = crate::card::parse_supercell_csv(GLOBALS_CSV);
    let c_name = h.iter().position(|c| c == "Name").ok_or("globals.csv: no Name")?;
    let c_bool = h.iter().position(|c| c == "BooleanValue").ok_or("globals.csv: no BooleanValue")?;
    let row = rows.iter().find(|r| r.get(c_name).map(|s| s.as_str()) == Some(name)).ok_or_else(|| format!("globals.csv: {name} missing"))?;
    match row.get(c_bool).map(|s| s.trim()).unwrap_or("") {
        "TRUE" | "true" => Ok(true),
        "" | "FALSE" | "false" => Ok(false),
        other => Err(format!("globals.csv: {name} BooleanValue {other:?} is not a boolean")),
    }
}

/// THE TOWER LEVEL MULTIPLIER (calibration combat.TOWER_HITPOINT_LADDER =
/// globals_percent_per_level_compound_floor): `pct(1) = 100` and, for every level
/// up to `level`, `pct(L) = floor(pct(L - 1) x (100 + rate(L)) / 100)` with
/// `rate(L)` the record's own per-level percent (`pct.king` for the KingTower,
/// `pct.princess` for the PrincessTower) while `L <= cap_level` and `pct.after_cap`
/// above it. Integer all the way (a 64-bit product); the caller applies it as
/// `base x pct / 100` (`CardDb::scale`). MEASURED on the live 16.402 towers (the
/// four hitpoint rows of tests/fixtures/live_levels.json and the princess tower's
/// 109 damage on the Prince of capture 20260920-003751) and equal to the
/// known king table at every level 1..15 (tests/levels.rs).
pub fn tower_multiplier_percent(calib: &Calib, kind: EntityKind, level: i32, pct: TowerPercents) -> Result<i32, String> {
    let per_level = match kind {
        EntityKind::KingTower => pct.king,
        EntityKind::PrincessTower => pct.princess,
        other => return Err(format!("tower_multiplier_percent on a {other:?}")),
    };
    if level < 1 {
        return Err(format!("tower level {level} is not a level"));
    }
    let mut p: i64 = 100;
    for l in 2..=level {
        let rate = if l <= calib.tower_ladder_cap_level { per_level } else { pct.after_cap };
        p = p * (100 + rate as i64) / 100;
    }
    i32::try_from(p).map_err(|_| format!("tower level {level}: multiplier overflow"))
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
        // half-cell must be exactly that, or the node encoding the recorded paths use
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
        only(&v, &["movement", "DEPLOY_TIMING", "value"], "spawn_anchored_full_first_step")?;
        // status.FULL_STOP_BUFF_IS_STUN: `buff_only` would re-attach every
        // status.STUN_PAUSES_* key to the composition instead of the timer, and is
        // not implemented -- refused rather than run as `stun_timer`.
        only(&v, &["status", "FULL_STOP_BUFF_IS_STUN", "value"], "stun_timer")?;
        // status.BUFF_PULSE_AMOUNT / spells.PULSING_AREA_EFFECT: only one arm each
        // is written (status.rs `BuffDef::pulse_base`, spell.rs `SpellMotion::Pulsing`).
        only(&v, &["status", "BUFF_PULSE_AMOUNT", "value"], "per_second_times_frequency")?;
        // status.BUFF_STACKING: `per_source_slot` needs the buff's source as part of
        // its identity, which no entity column carries.
        only(&v, &["status", "BUFF_STACKING", "value"], "one_slot_per_buff_row")?;
        only(&v, &["spells", "PULSING_AREA_EFFECT", "value"], "hit_speed_period_from_landing")?;
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
            preserve_target_if_hit_started: preserve_value(&v)?.0,
            preserve_target_scope: preserve_value(&v)?.1,
            xpos_based_tower_targeting: boolean(&v, &["targeting", "LOGIC_XPOS_BASED_TOWER_TARGETING", "value"])?,
            melee_range_limit: m(int(&v, &["targeting", "MELEE_RANGE_LIMIT", "value"])?),
            push_model,
            separation_iterations: int(&v, &["collision", "SEPARATION_ITERATIONS", "value"])?,
            footprint_model,
            path_model,
            goal_target_position: pick(&v, &["pathfinding", "GOAL_TARGET_POSITION", "value"], GoalTargetPosition::from_calibration_name)?,
            flyer_goal_water: pick(&v, &["pathfinding", "FLYER_GOAL_WATER", "value"], FlyerGoalWater::from_calibration_name)?,
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
            path_cost_water: cost_of("water")?,
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
            jump_water_hop: pick(&v, &["movement", "JUMP_WATER_HOP", "value"], JumpWaterHop::from_calibration_name)?,
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
            attract_base: pick(&v, &["status", "ATTRACT_LAW", "value"], AttractBase::from_calibration_name)?,
            attract_while_held: pick(&v, &["status", "ATTRACT_WHILE_HELD", "value"], AttractWhileHeld::from_calibration_name)?,
            centre_lane_frame: pick(&v, &["targeting", "CENTRE_LANE_FRAME", "value"], CentreLaneFrame::from_calibration_name)?,
            deploy_lockout_ticks: int(&v, &["match", "DEPLOY_LOCKOUT_TICKS", "value"])?,
            tick_order: pick(&v, &["match", "TICK_ORDER", "value"], TickOrder::from_calibration_name)?,
            dying_unit_visibility: pick(&v, &["movement", "DYING_UNIT_VISIBILITY", "value"], DyingUnitVisibility::from_calibration_name)?,
            mana_speed_up_remaining_s,
            territory_model,
            placement_snap_even: pick(&v, &["placement", "SNAP_EVEN_CORNER", "value"], PlacementSnapEven::from_calibration_name)?,
            placement_illegal_tap: pick(&v, &["placement", "ILLEGAL_TAP", "value"], PlacementIllegalTap::from_calibration_name)?,
            placement_tap_snap: pick(&v, &["placement", "TAP_SNAP", "value"], TapSnap::from_calibration_name)?,
            placement_troop_tower_taps: pick(&v, &["placement", "TROOP_TOWER_TAPS", "value"], TroopTowerTaps::from_calibration_name)?,
            illegal_spell_tap: pick(&v, &["spells", "ILLEGAL_SPELL_TAP", "value"], IllegalSpellTap::from_calibration_name)?,
            attacking_unit_movement: pick(&v, &["movement", "ATTACKING_UNIT_MOVEMENT", "value"], AttackingUnitMovement::from_calibration_name)?,
            attack_facing: pick(&v, &["movement", "ATTACK_FACING", "value"], AttackFacing::from_calibration_name)?,
            doomed_target_drop: pick(&v, &["targeting", "DOOMED_TARGET_DROP", "value"], DoomedTargetDrop::from_calibration_name)?,
            deploying_heading: pick(&v, &["movement", "DEPLOYING_HEADING", "value"], DeployingHeading::from_calibration_name)?,
            waiting_heading: pick(&v, &["movement", "WAITING_HEADING", "value"], WaitingHeading::from_calibration_name)?,
            zero_step_waypoint_test: pick(&v, &["pathfinding", "ZERO_STEP_WAYPOINT_TEST", "value"], ZeroStepWaypointTest::from_calibration_name)?,
            release_timing: pick(&v, &["spawner", "RELEASE_TIMING", "value"], ReleaseTiming::from_calibration_name)?,
            post_kill_wait: pick(&v, &["combat", "POST_KILL_RETARGET_WAIT", "value", "arm"], PostKillWait::from_calibration_name)?,
            post_kill_wait_units: v
                .pointer("/combat/POST_KILL_RETARGET_WAIT/value/units")
                .and_then(Value::as_array)
                .ok_or("combat.POST_KILL_RETARGET_WAIT.value.units: a list of unit names is required")?
                .iter()
                .map(|u| u.as_str().map(str::to_string).ok_or("combat.POST_KILL_RETARGET_WAIT.value.units: every entry is a unit name"))
                .collect::<Result<Vec<_>, _>>()?,
            post_kill_wait_ticks: int(&v, &["combat", "POST_KILL_RETARGET_WAIT", "value", "ticks"])?,
            post_kill_wait_override_units: v
                .pointer("/combat/POST_KILL_RETARGET_WAIT/value/attack_finish_override_units")
                .and_then(Value::as_array)
                .ok_or("combat.POST_KILL_RETARGET_WAIT.value.attack_finish_override_units: a list of unit names is required")?
                .iter()
                .map(|u| u.as_str().map(str::to_string).ok_or("combat.POST_KILL_RETARGET_WAIT.value.attack_finish_override_units: every entry is a unit name"))
                .collect::<Result<Vec<_>, _>>()?,
            dash_attack: pick(&v, &["combat", "DASH_ATTACK", "value"], DashAttack::from_calibration_name)?,
            reflect_attack: pick(&v, &["combat", "REFLECT_ATTACK", "value"], ReflectAttack::from_calibration_name)?,
            projectile_speed_to_subtiles_per_tick: int(&v, &["time", "PROJECTILE_SPEED_TO_SUBTILES_PER_TICK", "value"])?,
            crown_rounding: pick(&v, &["combat", "CROWN_TOWER_DAMAGE_ROUNDING", "value"], CrownRounding::from_calibration_name)?,
            aoe_hit_test: pick(&v, &["spells", "AOE_HIT_TEST", "value"], AoeHitTest::from_calibration_name)?,
            spell_as_deploy_launch: pick(&v, &["spells", "SPELL_AS_DEPLOY_LAUNCH_MODEL", "value"], LaunchModel::from_calibration_name)?,
            rolling_hit_shape: pick(&v, &["spells", "ROLLING_HIT_SHAPE", "value"], RollHitShape::from_calibration_name)?,
            spawning_spell_water: pick(&v, &["spells", "SPAWNING_SPELL_WATER_RULE", "value"], SpawnWaterRule::from_calibration_name)?,
            knock_law: pick(&v, &["knockback", "DISPLACEMENT_LAW", "value"], KnockLaw::from_calibration_name)?,
            knock_stacking: pick(&v, &["knockback", "STACKING", "value"], KnockStacking::from_calibration_name)?,
            max_pushback_length: int(&v, &["pathfinding", "MAX_PUSHBACK_LENGTH", "value"])?,
            knock_duration_ms: int(&v, &["knockback", "DURATION_MS", "value"])?,
            knock_zero_vector: pick(&v, &["knockback", "ZERO_VECTOR_DIRECTION", "value"], KnockZeroVector::from_calibration_name)?,
            knock_attack_reset: pick(&v, &["knockback", "ATTACK_RESET", "value"], KnockAttackReset::from_calibration_name)?,
            knock_affects_deploying: boolean(&v, &["knockback", "AFFECTS_DEPLOYING_UNITS", "value"])?,
            knock_direction_rolling: pick(&v, &["knockback", "DIRECTION_ROLLING", "value"], RollDirection::from_calibration_name)?,
            projectile_spawn_formation: pick(&v, &["spells", "PROJECTILE_SPAWN_FORMATION", "value"], ProjectileSpawnFormation::from_calibration_name)?,
            knock_rolling_contact_radius: int(&v, &["knockback", "ROLLING_CONTACT_RADIUS", "value"])?,
            stun_attack_timer: pick(&v, &["status", "STUN_ATTACK_TIMER_MODEL", "value"], StunTimerModel::from_calibration_name)?,
            stun_retarget_on_resume: boolean(&v, &["status", "STUN_RETARGET_ON_RESUME", "value"])?,
            resume_retarget_windup: pick(&v, &["status", "RESUME_RETARGET_WINDUP", "value"], ResumeWindup::from_calibration_name)?,
            retarget_progress: pick(&v, &["combat", "RETARGET_PROGRESS", "value"], RetargetProgress::from_calibration_name)?,
            buff_expiry: pick(&v, &["status", "BUFF_EXPIRY_TICK_ALIGNMENT", "value"], BuffExpiry::from_calibration_name)?,
            same_buff_reapply: pick(&v, &["status", "SAME_BUFF_REAPPLY", "value"], BuffReapply::from_calibration_name)?,
            buff_speed_composition: pick(&v, &["movement", "BUFF_SPEED_COMPOSITION", "value"], BuffComposition::from_calibration_name)?,
            hit_speed_buff: pick(&v, &["combat", "HIT_SPEED_BUFF", "value"], HitSpeedBuff::from_calibration_name)?,
            full_stop_buff_is_stun: pick(&v, &["status", "FULL_STOP_BUFF_IS_STUN", "value"], FullStopBuff::from_calibration_name)?,
            buff_pulse_amount: pick(&v, &["status", "BUFF_PULSE_AMOUNT", "value"], PulseAmount::from_calibration_name)?,
            buff_pulse_timing: pick(&v, &["status", "BUFF_PULSE_TIMING", "value"], PulseTiming::from_calibration_name)?,
            target_buff_on_splash: pick(&v, &["status", "TARGET_BUFF_ON_SPLASH", "value"], TargetBuffScope::from_calibration_name)?,
            pulsing_area_effect: pick(&v, &["spells", "PULSING_AREA_EFFECT", "value"], PulsingArea::from_calibration_name)?,
            stomp_schedule: pick(&v, &["movement", "STOMP_PAUSE_SCHEDULE", "value"], StompSchedule::from_calibration_name)?,
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
            spawner_timer_leftover: pick(&v, &["spawner", "TIMER_LEFTOVER", "value"], TimerLeftover::from_calibration_name)?,
            death_spawn_at_emission: pick(&v, &["spawner", "DEATH_SPAWN_AT_EMISSION_POINT", "value", "arm"], DeathAtEmission::from_calibration_name)?,
            death_spawn_at_emission_units: v
                .pointer("/spawner/DEATH_SPAWN_AT_EMISSION_POINT/value/units")
                .and_then(Value::as_array)
                .ok_or("spawner.DEATH_SPAWN_AT_EMISSION_POINT.value.units: a list of unit names is required")?
                .iter()
                .map(|u| u.as_str().map(str::to_string).ok_or("spawner.DEATH_SPAWN_AT_EMISSION_POINT.value.units: every entry is a unit name"))
                .collect::<Result<Vec<_>, _>>()?,
            spawner_spawn_point: pick(&v, &["spawner", "SPAWN_POINT", "value"], SpawnPoint::from_calibration_name)?,
            spawner_stun_pauses: boolean(&v, &["spawner", "STUN_PAUSES_SPAWNER", "value"])?,
            death_spawn_radius_default: pick(&v, &["spawner", "DEATH_SPAWN_RADIUS_DEFAULT", "value"], DeathSpawnRadius::from_calibration_name)?,
            death_spawn_deploy_default: pick(&v, &["spawner", "DEATH_SPAWN_DEPLOY_TIME_DEFAULT", "value"], DeathSpawnDeploy::from_calibration_name)?,
            spawner_emission_timing: pick(&v, &["spawner", "EMISSION_TIMING", "value"], SpawnerEmission::from_calibration_name)?,
            spawner_spawned_deploy_time: pick(&v, &["spawner", "SPAWNED_DEPLOY_TIME", "value"], SpawnedDeploy::from_calibration_name)?,
            lifetime_hp_decay: pick(&v, &["lifetime", "HP_DECAY", "value"], LifetimeDecay::from_calibration_name)?,
            charge_range_unit: pick(&v, &["charge", "CHARGE_RANGE_UNIT", "value"], ChargeRangeUnit::from_calibration_name)?,
            charge_accumulator: pick(&v, &["charge", "ACCUMULATOR", "value"], ChargeAccumulator::from_calibration_name)?,
            charge_multiplier_meaning: pick(&v, &["charge", "MULTIPLIER_MEANING", "value"], ChargeMultiplier::from_calibration_name)?,
            charge_progress_on_stop: pick(&v, &["charge", "PROGRESS_ON_STOP", "value"], ChargeStopRule::from_calibration_name)?,
            charge_reset_on_attack: boolean(&v, &["charge", "RESET_ON_ATTACK", "value"])?,
            charge_reset_on_stun: boolean(&v, &["charge", "RESET_ON_STUN", "value"])?,
            charge_reset_on_knockback: boolean(&v, &["charge", "RESET_ON_KNOCKBACK", "value"])?,
            charge_reset_on_retarget: boolean(&v, &["charge", "RESET_ON_RETARGET", "value"])?,
            charge_special_level_scaling: pick(&v, &["charge", "SPECIAL_LEVEL_SCALING", "value"], ChargeLevelScaling::from_calibration_name)?,
            tower_ladder: pick(&v, &["combat", "TOWER_HITPOINT_LADDER", "value", "regime"], TowerLadder::from_calibration_name)?,
            tower_ladder_cap_level: int(&v, &["combat", "TOWER_HITPOINT_LADDER", "value", "cap_level"])?,
            tower_hp_pct: TowerPercents {
                king: globals_number("HITPOINT_INCREASE_PERCENT_PER_KING_LEVEL")?,
                princess: globals_number("HITPOINT_INCREASE_PERCENT_PER_TOWER_LEVEL")?,
                after_cap: globals_number("HITPOINT_INCREASE_PERCENT_PER_TOWER_LEVEL_AFTER_TOURNAMENTCAP")?,
            },
            tower_dmg_pct: TowerPercents {
                // combat.KING_DAMAGE_PERCENT_PER_LEVEL, not the globals row: the embedded 2018
                // globals.csv says 7 and the 16.402 king hits at 8 (the 15.535 globals' value;
                // 109 per hit at level 11, never 100). The king's HITPOINT rate, 7, is right and
                // still read from the globals.
                king: int(&v, &["combat", "KING_DAMAGE_PERCENT_PER_LEVEL", "value"])?,
                princess: globals_number("DAMAGE_INCREASE_PERCENT_PER_TOWER_LEVEL")?,
                after_cap: globals_number("DAMAGE_INCREASE_PERCENT_PER_TOWER_LEVEL_AFTER_TOURNAMENTCAP")?,
            },
            formation_layout: pick(&v, &["formation", "LAYOUT", "value"], FormationLayout::from_calibration_name)?,
            formation_deploy_stagger: pick(&v, &["formation", "DEPLOY_STAGGER", "value"], DeployStagger::from_calibration_name)?,
            formation_stagger_wait: pick(&v, &["formation", "STAGGER_WAIT", "value"], StaggerWait::from_calibration_name)?,
            formation_ground_y_clamp: pick(&v, &["formation", "GROUND_Y_CLAMP", "value"], GroundYClamp::from_calibration_name)?,
            formation_ground_deploy_point: pick(&v, &["formation", "GROUND_DEPLOY_POINT", "value"], GroundDeployPoint::from_calibration_name)?,
            lane_id_based_deploy_sequence: globals_bool("LOGIC_LANE_ID_BASED_DEPLOY_SEQUENCE")?,
            attack_range_rule: pick(&v, &["targeting", "ATTACK_RANGE_RULE", "value"], AttackRangeRule::from_calibration_name)?,
            attack_cycle: pick(&v, &["combat", "ATTACK_CYCLE", "value"], AttackCycle::from_calibration_name)?,
            charged_hit_timing: pick(&v, &["charge", "CHARGED_HIT_TIMING", "value"], ChargedHitTiming::from_calibration_name)?,
            projectile_launch: pick(&v, &["combat", "PROJECTILE_LAUNCH", "value"], ProjectileLaunch::from_calibration_name)?,
            death_spawn_layout: pick(&v, &["spawner", "DEATH_SPAWN_LAYOUT", "value"], DeathSpawnLayout::from_calibration_name)?,
            death_spawn_pushback: pick(&v, &["spawner", "DEATH_SPAWN_PUSHBACK", "value"], DeathSpawnPushback::from_calibration_name)?,
            spawned_unit_acquire_delay: pick(&v, &["targeting", "SPAWNED_UNIT_ACQUIRE_DELAY", "value"], SpawnedUnitAcquireDelay::from_calibration_name)?,
            spawned_first_step: pick(&v, &["spawner", "SPAWNED_FIRST_STEP", "value"], SpawnedFirstStep::from_calibration_name)?,
        };
        // combat.TOWER_HITPOINT_LADDER: the four AT/AFTER_TOURNAMENTCAP rates are one
        // number in the shipped globals (10); the engine reads the one it names above
        // and refuses a globals.csv where they part rather than pick silently.
        for (name, want) in [
            ("HITPOINT_INCREASE_PERCENT_PER_KING_LEVEL_AFTER_TOURNAMENTCAP", c.tower_hp_pct.after_cap),
            ("HITPOINT_INCREASE_PERCENT_PER_TOWER_LEVEL_AT_TOURNAMENTCAP", c.tower_hp_pct.after_cap),
            ("HITPOINT_INCREASE_PERCENT_PER_KING_LEVEL_AT_TOURNAMENTCAP", c.tower_hp_pct.after_cap),
            ("DAMAGE_INCREASE_PERCENT_PER_KING_LEVEL_AFTER_TOURNAMENTCAP", c.tower_dmg_pct.after_cap),
            ("DAMAGE_INCREASE_PERCENT_PER_TOWER_LEVEL_AT_TOURNAMENTCAP", c.tower_dmg_pct.after_cap),
            ("DAMAGE_INCREASE_PERCENT_PER_KING_LEVEL_AT_TOURNAMENTCAP", c.tower_dmg_pct.after_cap),
        ] {
            let got = globals_number(name)?;
            if got != want {
                return Err(format!("globals.csv: {name} = {got} parts from the one AT/AFTER_TOURNAMENTCAP rate {want} the tower ladder reads (combat.TOWER_HITPOINT_LADDER)"));
            }
        }
        if c.tower_ladder_cap_level < 1 {
            return Err(format!("combat.TOWER_HITPOINT_LADDER.value.cap_level = {} is not a tower level", c.tower_ladder_cap_level));
        }
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
        // knockback.STACKING is implemented PER LAW: the ladder's gate refuses a push
        // while a ladder runs and never sums, and the fixed-distance slide sums and
        // never refuses. The other two pairings have no code and are refused here
        // rather than run as the nearest thing.
        match (c.knock_law, c.knock_stacking) {
            (KnockLaw::Client16402, KnockStacking::FirstWinsWhileActive) | (KnockLaw::FixedDistance, KnockStacking::VectorSum) => {}
            (law, st) => return Err(format!("knockback.STACKING = {st:?} has no engine implementation under knockback.DISPLACEMENT_LAW = {law:?}")),
        }
        only(&v, &["knockback", "WATER_RESOLUTION", "value"], "eject_to_nearest_land")?;
        // spawner.LIMIT_RULE: one implemented arm (the `only()` rule); the other
        // candidate is refused, never mapped. (DEATH_SPAWN_LAYOUT has two arms:
        // `death_spawn_layout` above.)
        only(&v, &["spawner", "LIMIT_RULE", "value"], "skip_unit_keep_cadence")?;
        // combat.KAMIKAZE_DEATH: the one implemented arm (the death at the fire,
        // measured on the melee Battle Ram; the projectile case is a hypothesis
        // under the same name).
        only(&v, &["combat", "KAMIKAZE_DEATH", "value"], "at_fire")?;
        if c.projectile_speed_to_subtiles_per_tick <= 0 || c.knock_duration_ms < 0 || c.max_pushback_length <= 0 {
            return Err("calibration.json: non-positive projectile speed / pushback cap or negative knockback duration".into());
        }
        if c.tick_ms <= 0
            || c.mana_regen_ms_1x <= 0
            || c.mana_regen_ms_2x <= 0
            || c.repath_interval_ticks.is_some_and(|r| r <= 0)
        {
            return Err("calibration.json: non-positive tick / regen / repath value".into());
        }
        if c.diag_den <= 0 || c.diag_num <= 0 || c.path_cost_road <= 0 || c.path_cost_default <= 0 || c.path_cost_water <= 0 {
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

    /// THE SHIPPED LEDGER WITH SOME VALUES REPLACED, for a run that judges a candidate without
    /// editing the file every session's engine reads. `overrides` maps `section.KEY` to the value
    /// as JSON text. Refused: a key the ledger does not have (an override cannot add one), a key
    /// with no `value`, a value that is not JSON, and a ledger the engine then cannot load.
    /// Returns the calibration and the parsed values applied, so a run can say what it ran. ONE
    /// implementation, used by the Python binding's `calibration_overrides` and the replay
    /// harness's `--calibration-override`.
    pub fn shipped_with_overrides(overrides: &std::collections::BTreeMap<String, String>) -> Result<(Calib, std::collections::BTreeMap<String, Value>), String> {
        let mut doc: Value = serde_json::from_str(CALIBRATION_JSON).map_err(|e| format!("the compiled-in ledger: {e}"))?;
        let mut parsed = std::collections::BTreeMap::new();
        for (path, raw) in overrides {
            let (section, key) = path.split_once('.').ok_or_else(|| format!("{path:?}: an override is named `section.KEY`"))?;
            let entry = doc
                .get_mut(section)
                .and_then(|sec| sec.get_mut(key))
                .and_then(|e| e.as_object_mut())
                .ok_or_else(|| format!("{path:?} is not a key in the ledger, and an override cannot add one"))?;
            if !entry.contains_key("value") {
                return Err(format!("{path:?} has no `value` to override"));
            }
            let v: Value = serde_json::from_str(raw).map_err(|e| format!("{path:?}: {raw:?} is not JSON ({e}); pass json.dumps(value)"))?;
            entry.insert("value".to_string(), v.clone());
            parsed.insert(path.clone(), v);
        }
        let calib = Calib::from_json(&doc.to_string()).map_err(|e| format!("the overridden ledger does not load: {e}"))?;
        Ok((calib, parsed))
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
    /// Install `c` as this config's calibration, with the three fields BattleConfig copies OUT
    /// of a calibration at construction following it -- else an override of a model key would
    /// change the calibration and not the model that runs.
    pub fn set_calib(&mut self, c: Calib) {
        self.path_model = c.path_model;
        self.push_model = c.push_model;
        self.footprint_model = c.footprint_model;
        self.calib = c;
    }

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
    /// THE MATCH HAS NOT OPENED YET (match.DEPLOY_LOCKOUT_TICKS). The real engine
    /// refuses every deploy until tick 90 and this engine used to accept them from tick
    /// 0, so anything driving it could open with a play no client could make.
    TooEarly { tick: u32, until: u32 },
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
    /// The formation member's DEPLOY_STAGGER wait, ms, already inside `deploy_ms`
    /// (entity.rs `stagger_ms`); 0 for everything that is not a staggered member.
    #[serde(default)]
    stagger_ms: i32,
    /// The death-spawn slide this member starts with (spawner.DEATH_SPAWN_PUSHBACK =
    /// client_ring_slide; entity.rs `death_slide_centre` / `death_slide_radius`): the death
    /// point, WORLD subtiles, and the radius it stops at, subtiles. 0 / (0, 0) on every other
    /// spawn. Added after SNAPSHOT_FORMAT 20; `default`, the no-slide value a queue saved
    /// before it held.
    #[serde(default)]
    slide_centre: Vec2,
    #[serde(default)]
    slide_radius: i32,
    /// A release the enemy target scan waits for (targeting.SPAWNED_UNIT_ACQUIRE_DELAY): true
    /// on every death-spawn member, false on everything else. It says what KIND of spawn this
    /// is, nothing more: the arm and the unit's kind are decided once, when the unit is
    /// created, by `BattleState::delay_acquisition`, which every creation site calls for an
    /// entry that carries it. A later release path the rule reaches (a Goblin Hut's waves, a
    /// Graveyard, a Suspicious Bush) sets it here and changes nothing else; a container whose
    /// release is its own death spawn is already a death-spawn member. Added after
    /// SNAPSHOT_FORMAT 20; `default`, the value a queue saved before it held.
    #[serde(default)]
    acquire_delay: bool,
    /// A release that takes its first update on the tick it is created (spawner.SPAWNED_FIRST_STEP;
    /// `BattleState::first_update`, called from `materialise_released`): true on a death-spawn member
    /// whose parent's row does not carry DeathSpawnPushback, false on everything else. A periodic
    /// emission does not need it: `create_emissions` gives the units it creates their first update
    /// itself. Added after SNAPSHOT_FORMAT 20; `default`, the value a queue saved before it held.
    #[serde(default)]
    first_update: bool,
    /// The heading the unit starts with, instead of its side's forward: a death-spawn member under
    /// spawner.DEATH_SPAWN_LAYOUT = facing_ring_rounded takes the dying unit's. None on everything else. Added after
    /// SNAPSHOT_FORMAT 20; `default`, the value a queue saved before it held.
    #[serde(default)]
    facing: Option<Vec2>,
}

/// The one death spawn's ring, as `BattleState::death_spawn_points` needs it: the
/// block's count and radius, the spawned unit's own radius and whether it flies, and
/// the ring's base direction (the dying unit's, `phase_reap`) with its
/// SpawnAngleShift. `slide`: the small fixed ring of spawner.DEATH_SPAWN_PUSHBACK =
/// client_ring_slide instead of DEATH_SPAWN_LAYOUT (`fixed_slide_ring`).
#[derive(Clone, Copy, Debug)]
struct DeathSpawnRing {
    count: i32,
    unit_radius: i32,
    flying: bool,
    facing: Vec2,
    angle_shift_deg: i32,
    radius: i32,
    slide: bool,
}

/// WHERE A DEATH SPAWN WHOSE ROW SETS DeathSpawnPushback IS BORN (calibration
/// spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide; `BattleState::death_spawn_points`, which
/// water-ejects these points like every other layout's).
///
/// Member k of n is born `move16402::DEATH_SLIDE_START` (250) native from the death point
/// `pos` at the FIXED angle -(k + 1) x 360 / n degrees in the native frame (0 = +x, 90 = +y):
/// the two Golemites at 180 and 0, the six Pups at 300, 240, 180, 120, 60 and 0. Measured on
/// client 16.402 (3 Golem deaths, one of them walking at 134 degrees, and 1 Lava Hound death,
/// all side 0): the angles do not turn with the dying unit's heading, and they are the same
/// on both halves of the arena (deaths at x 7114, 11384, 14254 and 3292). The Path phase then
/// slides each member out to DeathSpawnRadius (`phase_path16402`; `move16402::death_slide_to`).
///
/// SIDE 1 IS NOT MEASURED. This lays the same absolute ring for both seats, so a Red death is
/// not the seat rotation of a Blue one: for an even count the member SET rotates onto itself,
/// but the creation order does not (Red's first member sits on -x where the rotation would
/// put it on +x).
///
/// The angles are `fixed_ring_offset`'s. A `radius` (subtiles) at or inside the start radius
/// -- none on a loaded card -- is where the members are born, with nothing left to slide
/// (`phase_reap` gives them no slide).
fn fixed_slide_ring(pos: Vec2, count: i32, radius: i32) -> Vec<Vec2> {
    use crate::fixed::SUBTILE_PER_MILLITILE as K;
    let n = count.max(1);
    let start = move16402::DEATH_SLIDE_START.min(radius.max(0) / K);
    (0..n)
        .map(|k| {
            let (dx, dy) = fixed_ring_offset(k, n, start);
            pos.add(Vec2::new(dx * K, dy * K))
        })
        .collect()
}

/// THE FIXED RING'S ANGLE LAW, one member: the NATIVE offset of member `k` of `n` (k in
/// creation order) at `r` native from the death point, at the angle -(k + 1) x 360 / n
/// degrees in the native frame (0 = +x, 90 = +y), whatever the dying unit's heading and
/// seat. Integer degrees through formation.rs's sine table, each axis truncated toward zero.
/// A count that does not divide 360 is outside the measurement: its degree is rounded away
/// from zero, which keeps the last member on +x.
///
/// Kept apart from the start radius (`fixed_slide_ring`), the slide and the
/// DeathSpawnPushback flag (`phase_reap`), so that a layout on the same measured angles at
/// another radius and without the slide can lay its members from here. Which member takes which angle is
/// the engine's reading: the measurement gives the members in key order, not shown to be
/// creation order (tests/death_spawn_pushback.rs pins it, marked open).
fn fixed_ring_offset(k: i32, n: i32, r: i32) -> (i32, i32) {
    let n = n.max(1);
    // -(k + 1) x 360 / n, rounded away from zero when n does not divide 360
    #[cfg(not(clash_plant = "death_ring_angle_sign"))]
    let deg = -(((k + 1) * 360 + n - 1) / n);
    #[cfg(clash_plant = "death_ring_angle_sign")]
    let deg = ((k + 1) * 360 + n - 1) / n; // PLANT: the ring runs the other way round, member k at +(k + 1) x 360 / n.
    (r * crate::formation::sin1024(deg + 90) / 1024, r * crate::formation::sin1024(deg) / 1024)
}

/// ONE TICK OF THE DEATH-SPAWN SLIDE under a FRAME-PLANNED path arm (spawner.DEATH_SPAWN_PUSHBACK
/// = client_ring_slide; `phase_path_2026`, `phase_path`): the delta, subtiles, that moves
/// entity `i` move16402::DEATH_SLIDE_STEP further out from its death point, never past its
/// radius (`move16402::death_slide_to`), and whether that ends the slide. A plain radial step:
/// these arms have no contact law inside the pass (the Move phase's separation runs after
/// it), so the measured first-step split of `phase_path16402` is not reproduced here.
fn frame_planned_slide(e: &Entities, i: usize) -> (Vec2, bool) {
    use crate::fixed::SUBTILE_PER_MILLITILE as K;
    let (p, c) = (e.pos[i], e.death_slide_centre[i]);
    let ((x, y), done) = move16402::death_slide_to((p.x, p.y), (c.x, c.y), e.death_slide_radius[i], move16402::DEATH_SLIDE_STEP * K);
    #[cfg(clash_plant = "frame_planned_slide_ignored")]
    let ((x, y), done) = ((p.x, p.y), false); // PLANT (regression): the frame-planned arms hold a sliding member where it was born.
    (Vec2::new(x, y).sub(p), done)
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
    /// The direction this entity faces, as a vector rather than an angle (there is no
    /// floating point here). Read by the spawner's ring law, which lays a set
    /// SpawnAngleShift out relative to it, and by any test that has to show a scene
    /// actually turned a spawner rather than assuming it did.
    pub facing: Vec2,
    /// The contact push applied on the tick just run, after the mean and the 150 cap, and
    /// how many neighbours produced it. (0, 0) and 0 on a tick where nothing overlapped,
    /// which is a real answer rather than a missing one: a parity trace draws what the
    /// contact law DID beside what the recording shows, and a position column cannot tell
    /// a push the wrong way from a push too far.
    pub push_applied: Vec2,
    pub push_neighbours: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub shield: i32,
    pub radius: i32,
    pub flying: bool,
    pub deploying: bool,
    pub target: Option<EntityId>,
    pub attack_phase: AttackPhase,
    pub team_seq: u32,
    /// The attack progress counter (combat.ATTACK_CYCLE = progress_credit) or the
    /// ms elapsed in the current attack phase (windup_load_time).
    pub attack_ms: i32,
    /// The load timer (progress_credit; entity.rs `attack_load_ms`).
    pub attack_load_ms: i32,
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
    /// THE BUFF LIST (status.rs): every slot, empty ones included. `id` is the index
    /// into `BattleState::cards().buffs` plus one.
    pub buffs: &'a [crate::status::BuffSlot],
    /// The stomp clock in ms (movement.STOMP_PAUSE_SCHEDULE = ms_clock).
    pub stomp_clock: i32,
    /// This entity's speed THROUGH its buffs and its charge, subtiles per tick -- the
    /// figure the Path phase steps with (`effective_speed`), against `speed`, the
    /// unbuffed column.
    pub speed_now: i32,
    /// Will rescan on the first unstunned Target phase (status.STUN_RETARGET_ON_RESUME).
    pub retarget_on_resume: bool,
    /// ms of knockback slide remaining, and the displacement still to apply
    /// (knockback.DISPLACEMENT_LAW = fixed_distance).
    pub knock_ms: i32,
    pub knock_rem: Vec2,
    /// The knockback ladder (client16402; entity.rs): active, the speed
    /// still to run down (native units per tick) and the target point (NATIVE units).
    pub push_active: bool,
    pub push_speed: i32,
    pub push_target: Vec2,
    /// Mid river-jump (movement state 5 in the captures; entity.rs `jumping`): the route is the
    /// single landing node and the unit leaps at its card's JumpSpeed.
    pub jumping: bool,
    /// Hide state (Tesla; `Up` on everything else) and its timer (entity.rs
    /// `HideState` says what the timer means in each state).
    pub hide_state: HideState,
    pub hide_ms: i32,
    /// `hide_state == Hidden`: under ground, untargetable, immune (hide.*).
    pub hidden: bool,
    /// The protocol's status bits (entity.rs `Entities::status_flags`; py.rs ENTITY_FIELDS
    /// `status_flags`): bit 0 underground, bit 1 invisible to enemies, bit 2 hidden.
    pub status_flags: i32,
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
    /// THE DEATH-SPAWN SLIDE (spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide; entity.rs
    /// `death_slide_centre` / `death_slide_radius`): the death point the unit is sliding
    /// away from, world subtiles, and the radius it stops at, subtiles. 0 / (0, 0) on
    /// every unit that is not sliding, which is every unit under the shipped `not_read`.
    pub death_slide_centre: Vec2,
    pub death_slide_radius: i32,
    /// THE ACQUIRE DELAY (targeting.SPAWNED_UNIT_ACQUIRE_DELAY = client_8th_frame; entity.rs
    /// `acquirable_from`): the first tick whose Target phase may give this unit to an enemy.
    /// Its own first tick + 7 on a troop a death spawn created; 0 on every other unit, and on
    /// every unit under `none`.
    pub acquirable_from: u32,
    /// The avoidance offset the 16.402 move pass carries between ticks (move16402.rs `Contact::offset`):
    /// multiples of 10 in [-190, 190], 0 when the unit is not steering round a blocker.
    pub avoid_offset: i32,
    /// The frozen segment direction of the 16.402 move pass (entity.rs `seg_dir`): the direction toward
    /// the route's last node, fixed when the segment starts; (0, 0) with no segment.
    pub seg_dir: Vec2,
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
    /// This tick's delta of the entity is a KNOCKBACK LADDER step, not a walk
    /// (`phase_path16402`); `charge_pass` reads it so no accumulator counts it.
    pushed: Vec<bool>,
    /// This tick's delta of the entity is a RIVER-JUMP step (state 5, the landing
    /// tick included): the charge tail neither adds nor resets in state 5, so
    /// `charge_pass` leaves the progress alone.
    jumped: Vec<bool>,
    /// The selected pathfinder's grid, shared by every unit this tick
    /// (PATH_SEARCH = client16402).
    grid16402: Option<Grid16402>,
    /// THE DOOMED MASK (calibration movement.DYING_UNIT_VISIBILITY): per entity,
    /// whether the damage buffered before the move pass kills it this tick
    /// (`doomed_mask`). A read-only derivation, rebuilt every tick; nothing
    /// writes hp from it.
    doomed: Vec<bool>,
    /// spawner.SPAWNED_FIRST_STEP: the buildings that died in this Reap and left a death spawn that takes its
    /// first update, native (x, y, radius, side): `first_update`'s avoidance-only blockers. Filled and drained
    /// inside one `phase_reap`, so it never outlives the phase.
    dying_blockers: Vec<(i32, i32, i32, u8)>,
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
    /// The occluder lists `occ_cur` and `occ_prev` were stamped from; `None` while an array
    /// is still the initial zeros. Kept only so a snapshot can rebuild both arrays.
    src_cur: Option<Vec<path16402::Occluder>>,
    src_prev: Option<Vec<path16402::Occluder>>,
}

/// What a snapshot keeps of the 16.402 path grid: its epochs and the occluder lists behind its
/// two occlusion arrays (`None` for an array still at its initial zeros). The arrays are not
/// scratch the way the rest of `Scratch` is: they carry history. At a building change the refresh
/// hands `occ_cur` on to `occ_prev`, and the SAMEPATH test reads `occ_prev` on that tick and, for
/// a unit whose replan waited the change out (held by an attack or a post-kill wait), ticks
/// later. A grid rebuilt empty on load handed on zeros instead, at the first building change
/// after the load as at any other, so a resumed battle could walk a different path.
/// Not part of `state_hash`: the arrays are a function of these lists and of the arena and
/// calibration, and the hash has never covered the grid.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct GridSaved {
    epochs: [u32; 2],
    cur: Option<Vec<path16402::Occluder>>,
    prev: Option<Vec<path16402::Occluder>>,
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
            src_cur: None,
            src_prev: None,
        }
    }

    fn saved(&self) -> GridSaved {
        GridSaved { epochs: self.epochs, cur: self.src_cur.clone(), prev: self.src_prev.clone() }
    }

    /// The grid a snapshot describes: both arrays re-stamped from their saved lists by the
    /// same `occlusion` a refresh calls, so they equal the live grid's cell for cell.
    fn restore(arena: &Arena, calib: &Calib, saved: GridSaved) -> Grid16402 {
        let mut g = Grid16402::new(arena, calib);
        let n = g.occ_cur.len();
        let stamp = |list: &Option<Vec<path16402::Occluder>>| match list {
            Some(o) => path16402::occlusion(&g.terrain, o, &g.costs),
            None => vec![0; n],
        };
        let (cur, prev) = (stamp(&saved.cur), stamp(&saved.prev));
        g.occ_cur = cur;
        g.occ_prev = prev;
        g.epochs = saved.epochs;
        g.src_cur = saved.cur;
        g.src_prev = saved.prev;
        g
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
        self.src_prev = self.src_cur.replace(occluders);
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
/// How far, in tiles, a relocated building tap may move. 30 spans the arena from
/// any tile of it (18 x 32 tiles), so on this map the bound never decides
/// anything; it exists so the search terminates on a map where it could.
pub const PLACEMENT_SEARCH_RINGS: i32 = 30;

/// The tile offsets of the square ring at Chebyshev distance `r`, in a fixed
/// order. Only the ORDER is a choice: it breaks a tie between two fitting tiles
/// at the same distance, and the recordings leave exactly one such tie. This
/// order is symmetric about both axes, so mirroring the ring gives the ring back.
fn ring_offsets(r: i32) -> Vec<(i32, i32)> {
    let mut out = Vec::with_capacity((8 * r) as usize);
    for dx in -r..=r {
        out.push((dx, -r));
        out.push((dx, r));
    }
    for dy in -r + 1..=r - 1 {
        out.push((-r, dy));
        out.push((r, dy));
    }
    out
}

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
    /// Units released this tick under spawner.RELEASE_TIMING = end_of_event_phase,
    /// created at the end of Reap. Empty between ticks, so not in a snapshot.
    released: Vec<PendingSpawn>,
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
    /// The lifetime drain's remainder, in HUNDREDTHS of a hitpoint, per entity index
    /// (`lifetime_drain_per_tick`). Meaningless (0) on
    /// anything without a LifeTime and under lifetime.HP_DECAY = expiry_hit.
    lifetime_acc: Vec<i32>,
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

/// ONE BUFF APPLICATION LANDING on entity `i` (status.rs): `buff` (a `CardDb::buffs` index) for
/// `time_ms`, with `pulse_amount` per pulse. ONE SLOT PER BUFF ROW (status.BUFF_STACKING): it
/// refreshes the slot that already holds that row or takes a free one, and a unit already
/// carrying MAX_BUFFS_PER_ENTITY distinct rows drops it. Returns whether the row is a FULL STOP
/// that also drives the hold timer (status.FULL_STOP_BUFF_IS_STUN), whether or not a slot took
/// it, for the caller's stun merge. The one implementation behind `apply_effects` (the effect
/// buffer, drained in Resolve) and `reflect_melee_hit` (the attack pass).
fn land_buff(e: &mut Entities, table: &[crate::status::BuffDef], c: &Calib, i: usize, buff: u16, time_ms: i32, pulse_amount: i32) -> bool {
    let Some(def) = table.get(buff as usize).copied() else { return false };
    // status.FULL_STOP_BUFF_IS_STUN: a buff whose composed speed is 0 (the -100 / -100 / -100
    // rows: ZapFreeze, Freeze, ContinueFreeze) also drives the engine's one hold timer, so every
    // status.STUN_* key keeps its meaning and a Zap, a Freeze spell and an Ice Spirit all hold
    // alike.
    let full_stop = c.full_stop_buff_is_stun == FullStopBuff::StunTimer && crate::status::compose([def].iter(), Sel::Speed, 100) == 0;
    // status.BUFF_PULSE_TIMING: the pulse clock starts a whole period out, so an area that
    // refreshes its buff four times a second still pulses once a second.
    let pulse_ms = match c.buff_pulse_timing {
        PulseTiming::AfterFirstPeriod => def.hit_frequency_ms.max(0),
        PulseTiming::OnApplication => 0,
    };
    let slots = e.buff_slots_mut(i);
    let id = buff + 1;
    match slots.iter().position(|s| s.id == id) {
        Some(k) => {
            // A REFRESH keeps the pulse clock running: the buff did not restart, it was
            // extended (status.SAME_BUFF_REAPPLY).
            slots[k].ms = match c.same_buff_reapply {
                BuffReapply::RefreshMax => slots[k].ms.max(time_ms),
                BuffReapply::Replace => time_ms,
            };
            slots[k].pulse_amount = pulse_amount;
        }
        None => {
            if let Some(k) = slots.iter().position(|s| s.is_empty()) {
                slots[k] = BuffSlot { id, ms: time_ms, pulse_ms, pulse_amount };
            }
        }
    }
    full_stop
}

/// A HOLD OF `ms` LANDING on entity `i`: merged into `stun_ms` by status.SAME_BUFF_REAPPLY, the
/// target lock released for the resume rescan (status.STUN_RETARGET_ON_RESUME), the attack cycle
/// paused or reset (status.STUN_ATTACK_TIMER_MODEL) and the charge cleared (charge.RESET_ON_STUN).
/// The one implementation behind `apply_effects` and `reflect_melee_hit`.
fn land_stun(e: &mut Entities, c: &Calib, i: usize, ms: i32) {
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
    // CHARGE (calibration charge.RESET_ON_STUN): the stun clears the charge and the run-up.
    // Its own columns; nothing else here reads them.
    if c.charge_reset_on_stun {
        e.charged[i] = false;
        e.charge_progress[i] = 0;
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
            released: Vec::new(),
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
            lifetime_acc: Vec::new(),
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
        // CROWN TOWERS (calibration combat.TOWER_HITPOINT_LADDER): the two tower
        // records scale with the TOWER level on the game's own per-level regime, not
        // the card ladder of their (Common) rarity -- measured 4824 / 3052 at tower
        // level 11 against the ladder's 6144 / 3584. Hitpoints and damage each on
        // their globals percent (`tower_multiplier_percent`).
        #[cfg(not(clash_plant = "tower_card_ladder"))]
        let tower = matches!(kind, EntityKind::KingTower | EntityKind::PrincessTower)
            && self.cfg.calib.tower_ladder == TowerLadder::GlobalsPercentPerLevelCompoundFloor;
        #[cfg(clash_plant = "tower_card_ladder")]
        let tower = false; // PLANT (regression): the earlier engine's towers on the card ladder (6144 / 3584 at 11).
        let (hp, damage) = if tower {
            let calib = &self.cfg.calib;
            (
                CardDb::scale(c.hitpoints, tower_multiplier_percent(calib, kind, level, calib.tower_hp_pct)?),
                CardDb::scale(c.damage, tower_multiplier_percent(calib, kind, level, calib.tower_dmg_pct)?),
            )
        } else {
            (scaled(c.hitpoints)?, scaled(c.damage)?)
        };
        let id = self.ents.spawn(SpawnInit {
            team,
            kind,
            card,
            level,
            pos,
            hp,
            shield: scaled(c.shield_hitpoints)?,
            damage,
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
        if self.lifetime_acc.len() <= i {
            self.lifetime_acc.resize(i + 1, 0);
        }
        self.lifetime_acc[i] = 0;
        #[cfg(clash_plant = "acquire_delay_every_unit")]
        self.delay_acquisition(i); // PLANT: every new troop waits, hand-played and periodic included.
        // A hiding building (or a spawner) with no deploy time at all is "deployed" now.
        if self.ents.deploy_ms[i] == 0 {
            self.on_deployed(i);
        }
        Ok(id)
    }

    /// THE MOMENT AN ENTITY'S DEPLOY TIME ENDS: the hide machinery takes its start
    /// state and a spawner is ACTIVATED. Called from `deploy_countdown` on the tick
    /// deploy_ms reaches 0 -- which runs at the END of Move under match.TICK_ORDER =
    /// client16402 (the deploy countdown after the move pass, as the captures show) and
    /// in Upkeep under legacy_move_before_attack -- from `spawn_now` / phase_spawn for a
    /// zero deploy time, and from the scenario setup path, which skips the deploy
    /// timer.
    fn on_deployed(&mut self, i: usize) {
        self.hide_on_deployed(i);
        self.spawner_activate(i);
    }

    /// spawner.EMISSION_TIMING in force. The regression plant `spawner_first_wave_late`
    /// forces the earlier arm whatever the ledger says -- the spawner pass back
    /// in the Spawn phase, its units queued for the next tick -- so tests/spawner.rs's
    /// tick alignment and the Tombstone rows of the replay harness go red under it.
    #[inline]
    fn release_timing(&self) -> ReleaseTiming {
        #[cfg(clash_plant = "release_deferred")]
        {
            return ReleaseTiming::NextSpawnPhase; // PLANT (regression): every release a tick late.
        }
        #[allow(unreachable_code)]
        self.cfg.calib.release_timing
    }

    /// A RELEASED unit (spawner.RELEASE_TIMING): queued for the next Spawn phase, or held
    /// for the end of this tick's Reap phase (`materialise_released`).
    fn release(&mut self, p: PendingSpawn) {
        match self.release_timing() {
            ReleaseTiming::NextSpawnPhase => self.spawn_queue.push(p),
            ReleaseTiming::EndOfEventPhase => self.released.push(p),
        }
    }

    /// The tick's released units, created in release order with the same per-unit steps
    /// `phase_spawn` gives a queued one -- except that nothing counts down or steps: they
    /// are created after every phase that would.
    fn materialise_released(&mut self) {
        let mut fresh: Vec<usize> = Vec::new();
        for p in std::mem::take(&mut self.released) {
            let kind = match self.cfg.cards.get(p.card).kind {
                CardKind::Building => EntityKind::Building,
                CardKind::Troop => EntityKind::Troop,
                CardKind::Spell => unreachable!("a release is a unit, never a spell"),
            };
            let id = self.spawn_now(p.team, p.card, p.level, p.pos, kind).expect("level validated at release");
            self.ents.spawned_by[id.index as usize] = p.owner;
            self.ents.stagger_ms[id.index as usize] = p.stagger_ms;
            self.ents.death_slide_centre[id.index as usize] = p.slide_centre;
            self.ents.death_slide_radius[id.index as usize] = p.slide_radius;
            if let Some(f) = p.facing {
                self.ents.facing[id.index as usize] = f;
            }
            if p.acquire_delay {
                self.delay_acquisition(id.index as usize);
            }
            if let Some(d) = p.deploy_ms {
                self.ents.deploy_ms[id.index as usize] = d;
                if d == 0 {
                    self.on_deployed(id.index as usize);
                }
            }
            if p.first_update {
                fresh.push(id.index as usize);
            }
        }
        // spawner.SPAWNED_FIRST_STEP: the death spawns take their first update now.
        #[cfg(not(clash_plant = "first_step_siblings_push"))]
        let apart = true;
        #[cfg(clash_plant = "first_step_siblings_push")]
        let apart = false; // PLANT (regression): the members push each other on the first step.
        let blockers = std::mem::take(&mut self.scratch.dying_blockers);
        self.first_update(&fresh, apart, &blockers);
    }

    /// THE ONE SETTER of targeting.SPAWNED_UNIT_ACQUIRE_DELAY: entity `i`, created this tick
    /// from a release that carries `PendingSpawn::acquire_delay`, becomes acquirable on its own
    /// first tick + `target::ACQUIRE_DELAY_TICKS` -- F + 7, its 8th frame -- when the arm is
    /// client_8th_frame and the unit is a TROOP. target.rs `can_target` refuses it to every
    /// enemy before then. Nothing else is held: it walks, takes targets and attacks as it
    /// would, and area damage lands on it.
    ///
    /// F IS THE UNIT'S OWN FIRST TICK (`spawn_tick`), not its parent's death: under
    /// spawner.RELEASE_TIMING = end_of_event_phase the members exist on the death frame, under
    /// next_spawn_phase one tick later, and either way the first enemy target lands 7 ticks
    /// after the first frame the unit is on the board, which is what was measured.
    ///
    /// Called from every creation site (`materialise_released`, `phase_spawn`, the spawner
    /// pass) for an entry that carries the flag, which today is a death spawn's alone. A later
    /// release the rule reaches (a Goblin Hut's waves, a Graveyard's and a Suspicious Bush's
    /// troops, all measured at F + 7 on client 15.535.29 and none loadable yet) sets the flag
    /// on its PendingSpawn and comes through here. The Skeleton Barrel's Skeletons, measured at
    /// F + 7 as well, are its container's own DeathSpawnCharacter: if the container loads as a
    /// unit with a death spawn, `phase_reap` flags them and nothing more is needed.
    ///
    /// NEVER FLAGGED: a hand-played troop, a periodic spawner's troop and a building. Measured
    /// on client 15.535.29 for three of them: a hand-played troop is targeted on its first
    /// frame, a Tombstone's periodic Skeleton on F + 1 and the Goblin Drill's building (not a
    /// death spawn) on F + 1. The rest is the engine's reading and is not measured: the troops
    /// of every other periodic spawner (the Barbarian Hut's and the Witch's among them), and a
    /// building a death spawn creates (no loaded row has one). The Barbarian Hut is the open
    /// case: its row carries the same SpawnCharacter and SpawnInterval columns as the
    /// Tombstone's, whose Skeletons are targeted on F + 1, and as the Goblin Hut's, whose waves
    /// wait for F + 7.
    ///
    /// WITH THE DEATH-SPAWN SLIDE (spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide) the two
    /// are independent columns set on the same creation: the slide keeps the member from
    /// TAKING a target until it reaches DeathSpawnRadius (a Golemite's last slide step is on
    /// F + 5), this keeps every enemy from taking IT until F + 7. Neither reads the other.
    fn delay_acquisition(&mut self, i: usize) {
        #[cfg(not(clash_plant = "acquire_delay_ignores_arm"))]
        let on = self.cfg.calib.spawned_unit_acquire_delay == SpawnedUnitAcquireDelay::Client8thFrame;
        #[cfg(clash_plant = "acquire_delay_ignores_arm")]
        let on = true; // PLANT: the delay runs under `none` as well.
        #[cfg(not(clash_plant = "acquire_delay_on_buildings"))]
        let troop = self.ents.kind[i] == EntityKind::Troop;
        #[cfg(clash_plant = "acquire_delay_on_buildings")]
        let troop = true; // PLANT: a death-spawned building waits too.
        if on && troop {
            self.ents.acquirable_from[i] = self.ents.spawn_tick[i] + target::ACQUIRE_DELAY_TICKS;
        }
    }

    fn emission_timing(&self) -> SpawnerEmission {
        #[cfg(clash_plant = "spawner_first_wave_late")]
        {
            return SpawnerEmission::SpawnPhaseNextTick; // PLANT (regression): two ticks late.
        }
        #[allow(unreachable_code)]
        self.cfg.calib.spawner_emission_timing
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
    /// NOT READ HERE (the data ships them; the ring arm of DEATH_SPAWN_LAYOUT and a
    /// flank spawn would need them): SpawnAngleShift (DarkWitch 90: the Bats on her
    /// flanks; BattleRam 180: the Barbarians behind), DeathSpawnMinRadius
    /// (SkeletonContainer 100). DeathSpawnPushback (Golem, LavaHound and, in the 2018
    /// table only, DarkWitch true) is a death-spawn column and only the death spawn reads
    /// it (spawner.DEATH_SPAWN_PUSHBACK, `death_spawn_points`).
    fn spawn_point(&self, i: usize, sp: &SpawnerDef) -> Vec2 {
        let c = self.ents.pos[i];
        match self.cfg.calib.spawner_spawn_point {
            SpawnPoint::AtCentre => c,
            SpawnPoint::InFrontAtOwnRadius => {
                let d = sp.radius.unwrap_or(self.ents.radius[i]);
                Vec2::new(c.x, c.y + spell::forward_dy(self.ents.team[i]) * d)
            }
            // The measured arm's BLANK-SpawnRadius case: forward at the tangent of
            // the two circles. The SpawnRadius case does not go through here at all,
            // because it is a ring over the whole wave rather than one point the
            // formation is laid out around (`measured_ring_points`).
            SpawnPoint::Client16402Measured | SpawnPoint::ClientRoundedFacingDegree => {
                let unit_r = self.cfg.cards.get(sp.unit).collision_radius;
                let d = self.ents.radius[i] + unit_r;
                Vec2::new(c.x, c.y + spell::forward_dy(self.ents.team[i]) * d)
            }
        }
    }

    /// The measured arm's SpawnRadius case: where each of a wave's `n` units stands,
    /// on a ring of `sp.radius` around the spawner, in creation order.
    ///
    /// The angle is two laws (SpawnPoint::Client16402Measured). A blank
    /// SpawnAngleShift lays the ring out in the ABSOLUTE frame; a set one lays it out
    /// relative to the spawner's own facing. Nothing in the table ships an explicit 0,
    /// so 0 stands for blank here -- a claim about the DATA, held by
    /// `test_no_entry_ships_an_explicit_zero_angle_shift` in tests/test_card_reads.py.
    /// The day an entry ships a real 0 that test goes red, because this branch would
    /// then lay that card's ring out in the wrong frame with nothing to say so.
    ///
    /// Returns None when the card leaves SpawnRadius blank, which is the other case.
    fn measured_ring_points(&self, i: usize, sp: &SpawnerDef, n: i32) -> Option<Vec<Vec2>> {
        let radius = sp.radius?;
        let c = self.ents.pos[i];
        let shift = self.cfg.cards.get(self.ents.card[i]).formation.spawn_angle_shift_deg;
        // The spawner's facing in degrees, measured the same way the ring is. Only
        // consulted when the card SETS a shift; the blank case must not move with it.
        let facing_deg = if shift == 0 {
            0
        } else if self.cfg.calib.spawner_spawn_point == SpawnPoint::ClientRoundedFacingDegree {
            crate::formation::rounded_degree(self.ents.facing[i])
        } else {
            let f = self.ents.facing[i];
            // The facing as a whole number of degrees, in the same convention the ring
            // is drawn in: angle a points at (cos a, sin a). There is no integer atan2
            // here and none is needed, because the ring only resolves to the degree, so
            // the angle is the one whose direction the facing agrees with most. Ties
            // go to the lower degree, which keeps it a function of the facing alone.
            (0..360)
                .max_by_key(|d| {
                    let cos = crate::formation::sin1024(*d + 90) as i64;
                    let sin = crate::formation::sin1024(*d) as i64;
                    (cos * f.x as i64 + sin * f.y as i64, -d)
                })
                .unwrap_or(0)
        };
        let step = if n > 0 { 360 / n } else { 0 };
        Some(
            (0..n.max(1))
                .map(|k| {
                    let a = facing_deg + shift + step * k;
                    let x = c.x + (radius as i64 * crate::formation::sin1024(a + 90) as i64 / 1024) as i32;
                    let y = c.y + (radius as i64 * crate::formation::sin1024(a) as i64 / 1024) as i32;
                    Vec2::new(x, y)
                })
                .collect(),
        )
    }

    /// THE SPAWNER PASS: every periodic spawner past its deploy time ticks its timer
    /// by TICK_MS and, at <= 0, emits the unit(s) that are due. Decided from
    /// START-OF-PASS state (the count a SpawnLimit compares against, the positions),
    /// collected, sorted by (team, the spawner's team_seq, unit index) and only then
    /// emitted (`create_emissions`), so the creation order -- and therefore team_seq --
    /// is canonical and slot-free.
    ///
    /// WHERE IT RUNS (calibration spawner.EMISSION_TIMING): under
    /// `move_phase_immediate` the pass runs at the end of Move, right after
    /// `deploy_countdown` (which match.TICK_ORDER = client16402 places after the move
    /// pass), for the first time on the tick the deploy timer reaches zero, and its
    /// units EXIST in that tick -- a Tombstone's first Skeleton on the Tombstone's own
    /// deploy-end tick, as the recordings show (38 of 44 exact-tick live deploys at
    /// delta 0). Under `spawn_phase_next_tick` (the earlier engine) it ran at the end
    /// of the Spawn phase and pushed PendingSpawns: two ticks late on every one.
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
    /// TICK ALIGNMENT (tests/spawner.rs pins it): under the shipped arm an activation
    /// at tick A with start time S creates the first unit in tick A + ceil(S / TICK_MS)
    /// -- in tick A itself when S is blank or 0, because the pass runs after the
    /// countdown in the same Move phase and decrements the timer once there; a live
    /// Witch (SpawnStartTime 1000) has her first Skeletons 19 ticks after her
    /// deploy-end tick, which is that formula. (Under `spawn_phase_next_tick` it was
    /// A + max(1, ceil(S / TICK_MS)) + 1, the one-tick PendingSpawn latency on top of
    /// a pass that had already run for tick A.) With pause P the next wave's first
    /// unit follows the last one by
    /// ceil(P / TICK_MS) ticks; a SpawnInterval I separates a wave's units by
    /// ceil(I / TICK_MS) ticks -- under the shipped spawner.TIMER_LEFTOVER = dropped. Under
    /// client16402_carried the activation tick's overshoot (-TICK_MS) is carried, so the first
    /// gap after a blank-start activation is ceil((I - TICK_MS) / TICK_MS). SpawnLimit (spawner.LIMIT_RULE = skip_unit_keep_cadence):
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
            // Under the measured arm a card that SETS SpawnRadius puts its wave on a
            // RING around the spawner rather than in a formation around one forward
            // point, so the ring replaces the layout instead of feeding it. A card
            // that leaves SpawnRadius blank returns None here and takes the old path
            // with the new forward point.
            let ring = if matches!(self.cfg.calib.spawner_spawn_point, SpawnPoint::Client16402Measured | SpawnPoint::ClientRoundedFacingDegree) {
                self.measured_ring_points(i, &sp, sp.number)
            } else {
                None
            };
            let grid = match ring {
                Some(points) => points,
                None if sp.interval_ms == 0 => self.formation_points(e.team[i], sp.number, unit.collision_radius, unit.is_flying(), point),
                None => self.formation_points(e.team[i], 1, unit.collision_radius, unit.is_flying(), point),
            };
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
                    // spawner.SPAWNED_DEPLOY_TIME: the game's emitted unit is born
                    // walking (measured on 1230 live emissions), so it carries no
                    // deploy timer; the old arm gave it the unit's own DeployTime.
                    let deploy_ms = match self.cfg.calib.spawner_spawned_deploy_time {
                        SpawnedDeploy::Zero => Some(0),
                        SpawnedDeploy::UnitOwnDeployTime => None,
                    };
                    emissions.push((e.team[i], e.team_seq[i], k, PendingSpawn { team: e.team[i], card: sp.unit, level, pos, deploy_ms, owner: Some(e.id_of(i)), stagger_ms: 0, slide_centre: Vec2::default(), slide_radius: 0, acquire_delay: false, first_update: false, facing: None }));
                    k += 1;
                }
                left -= 1;
                let reload = if left > 0 {
                    sp.interval_ms
                } else {
                    match self.cfg.calib.spawner_pause_anchor {
                        PauseAnchor::AfterLastUnit => sp.pause_time_ms,
                        PauseAnchor::AfterFirstUnit => (sp.pause_time_ms - (sp.number - 1) * sp.interval_ms).max(0),
                    }
                };
                // spawner.TIMER_LEFTOVER: `ms` is the overshoot here, 0 or below. The
                // measured arm adds the reload to it, so a blank-start spawner's first gap
                // is one tick shorter (Tombstone A, A + 9, A + 79 in the corpus); a start
                // time fires at exactly 0 and has nothing to carry. Within one pass the sum
                // stays at or below 0 while a SpawnInterval-0 wave is still emitting.
                ms = match self.cfg.calib.spawner_timer_leftover {
                    TimerLeftover::Dropped => reload,
                    TimerLeftover::Carried => ms + reload,
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
        self.create_emissions(emissions);
    }

    /// EMISSIONS ONTO THE BOARD: the units a pass decided to emit, each tagged (team,
    /// the emitter's team_seq, k), put in that canonical order before anything is
    /// created, so the creation order -- and therefore team_seq -- is slot-free
    /// whichever emitter, and whichever pass, produced them. Under
    /// spawner.EMISSION_TIMING = spawn_phase_next_tick they are queued for the next
    /// Spawn phase; under move_phase_immediate they are created now, each with its
    /// owner, its stagger and the deploy override (activated at once on a zero one),
    /// and the spatial hash is rebuilt once if anything was emitted.
    fn create_emissions(&mut self, mut emissions: Vec<(Team, u32, u32, PendingSpawn)>) {
        emissions.sort_by_key(|(t, seq, k, _)| (*t as u8, *seq, *k));
        if self.emission_timing() == SpawnerEmission::SpawnPhaseNextTick {
            self.spawn_queue.extend(emissions.into_iter().map(|(_, _, _, p)| p));
            return;
        }
        // move_phase_immediate: the unit EXISTS in this tick's state (a live
        // Tombstone's first Skeleton is on the board on the deploy-end tick). The
        // engine still decides the whole pass from start-of-phase state and creates in
        // the canonical (team, the spawner's team_seq, k) order rather than
        // interleaving the creations with the pass, so the order stays slot-free.
        if emissions.is_empty() {
            return;
        }
        let mut fresh: Vec<usize> = Vec::new();
        for (_, _, _, p) in emissions {
            let kind = match self.cfg.cards.get(p.card).kind {
                CardKind::Building => EntityKind::Building,
                CardKind::Troop => EntityKind::Troop,
                CardKind::Spell => continue, // a spawner's unit is never a spell (card.rs refuses it)
            };
            let Ok(id) = self.spawn_now(p.team, p.card, p.level, p.pos, kind) else { continue };
            let i = id.index as usize;
            fresh.push(i);
            self.ents.spawned_by[i] = p.owner;
            self.ents.stagger_ms[i] = p.stagger_ms;
            self.ents.death_slide_centre[i] = p.slide_centre;
            self.ents.death_slide_radius[i] = p.slide_radius;
            if let Some(f) = p.facing {
                self.ents.facing[i] = f;
            }
            // No periodic emission carries the flag today: a Tombstone's periodic Skeleton is
            // born targetable, as measured, and the other loaded spawners follow it here,
            // unmeasured. A Goblin Hut's waves will carry it when that card loads, and then this
            // line is their setter call.
            if p.acquire_delay {
                self.delay_acquisition(i);
            }
            if let Some(d) = p.deploy_ms {
                self.ents.deploy_ms[i] = d;
                if d == 0 {
                    self.on_deployed(i);
                }
            }
        }
        self.hash.rebuild(&self.ents);
        // spawner.SPAWNED_FIRST_STEP: the units this pass emitted take their first update now.
        self.first_update(&fresh, true, &[]);
    }

    /// THE FIRST UPDATE OF A UNIT CREATED AFTER THE TICK'S PASSES (spawner.SPAWNED_FIRST_STEP =
    /// client16402_same_tick): a periodic emission at the end of Move (`create_emissions`, under
    /// spawner.EMISSION_TIMING = move_phase_immediate) and a death spawn at the end of Reap
    /// (`materialise_released`, under spawner.RELEASE_TIMING = end_of_event_phase). A unit created
    /// in the Spawn phase, before the passes, already takes its first update on its creation tick
    /// and is never passed here.
    ///
    /// THE LAW, measured on client 15.535.29: on its first frame the unit has taken its whole
    /// first update. With an enemy in range it has acquired it and entered its attack (on the 16.402
    /// corpus, 18 of 20 such units read attack progress LoadTime + 50 there, combat.ATTACK_CYCLE's
    /// entry credit); otherwise it has taken one move step through the ordinary walk and contact
    /// law, against the units already on the board at their current positions. The first Skeleton
    /// of 8 of 8 Tombstone waves stands one step from the emission point; a wave's second member
    /// steps less, pushed by the first. A dying Tombstone's four Skeletons stand together on one
    /// point one step past its emission point: the members of one death do not push each other on
    /// that step, and the dying parent is already gone.
    ///
    /// So this runs the tick's own Target, Attack and Path passes for `fresh` alone, in that order,
    /// and writes the positions as the Move phase does. `apart` (every caller): each member steps
    /// with the other members of its batch hidden from its scans, as the four death Skeletons do.
    /// A batch is what one call creates, so the engine also reads two deaths on one tick, and two
    /// spawners emitting on one tick, as one batch; neither is measured, and no loaded spawner
    /// emits two units on one tick. Otherwise they step in creation order, each seeing the
    /// others' bodies as the move pass does.
    ///
    /// THE DYING BUILDING STEERS THE FIRST STEP: `blockers`, the buildings whose death released these
    /// units, stand in the avoidance scan as static blockers, and the separation scan does not see them.
    /// So a Goblin Cage's Brawler, born on the cage's centre, reads avoidance offset -190 on its first
    /// frame (the scan's -200 and one decay). The scan in each birth's creation-tick update predicts the
    /// first-frame offset, sign included, for 30 of 30 births beside a dying building on client 15.535.29
    /// (the Goblin Cage's Brawler 3, the Goblin Drill's goblins 13, the Tombstone's Skeletons 14). A dying
    /// TROOP is not a blocker here, because troops disagree: the Elixir Golem's halves read the +-190 of a
    /// blocker (42 of 48) and the Battle Ram's Barbarians read 0 (24 of 25). A DeathSpawnPushback row's
    /// members take no first update at all.
    ///
    /// ONLY UNDER THE 16.402 MODEL (PathSearch::Client16402 and match.TICK_ORDER = client16402):
    /// the step is that model's move pass, and under the legacy tick order the Attack phase runs
    /// after Move anyway. Troops only: a building takes no step.
    fn first_update(&mut self, fresh: &[usize], apart: bool, blockers: &[(i32, i32, i32, u8)]) {
        #[cfg(not(clash_plant = "first_step_unread"))]
        let on = self.cfg.calib.spawned_first_step == SpawnedFirstStep::SameTick;
        #[cfg(clash_plant = "first_step_unread")]
        let on = false; // PLANT (regression): the new arm stands on the creation point.
        if !on || !self.arm16402() || self.tick_order() != TickOrder::Client16402 {
            return;
        }
        let fresh: Vec<usize> = fresh.iter().copied().filter(|&i| self.ents.alive[i] && self.ents.kind[i] == EntityKind::Troop).collect();
        if fresh.is_empty() {
            return;
        }
        #[cfg(not(clash_plant = "first_step_walks_only"))]
        {
            self.phase_target_for(Some(&fresh));
            self.phase_attack_for(Some(&fresh));
        }
        let batches: Vec<(Vec<usize>, Vec<usize>)> = if apart {
            fresh.iter().map(|&i| (vec![i], fresh.iter().copied().filter(|&j| j != i).collect())).collect()
        } else {
            vec![(fresh.clone(), Vec::new())]
        };
        for (movers, unseen) in batches {
            self.phase_path16402_for(Some(&movers), &unseen, blockers);
            // the Move phase's position write under the 16.402 contact law: the delta IS the
            // position write, clamped to the arena
            let (w, h) = (self.cfg.arena.width, self.cfg.arena.height);
            for &i in &movers {
                let d = self.scratch.deltas.get(i).copied().unwrap_or_default();
                if d != Vec2::default() {
                    let old = self.ents.pos[i];
                    self.ents.pos[i] = Vec2::new((old.x + d.x).clamp(0, w), (old.y + d.y).clamp(0, h));
                }
            }
        }
        self.hash.rebuild(&self.ents);
    }

    /// WHERE A DEATH SPAWN'S UNITS APPEAR (calibration spawner.DEATH_SPAWN_LAYOUT).
    ///
    /// facing_ring (shipped, MEASURED on the two live Battle Ram deaths of the
    /// corpus: the Barbarians at +-600 = DeathSpawnRadius on the axis from the death
    /// point to the ram's target, (539, 263) / (-539, -263) on a ram whose tower lay
    /// at 26.5 degrees): member k at `radius` from the death point in the direction
    /// `facing` (the caller passes the direction to the target, else the unit's
    /// facing) rotated by SpawnAngleShift + k x 360 / count, the rotation through
    /// formation.rs `sin1024`; a unit with neither faces its seat's forward. The
    /// count-2 case is the measured one; the rotation for more members is the
    /// ring's natural reading and a hypothesis (no corpus death spawn has three).
    ///
    /// engine_grid_within_radius (the earlier engine): the engine
    /// formation grid around the death point in the OWNER's frame (`formation_grid`,
    /// the same code as a deploy and a release), each point pulled back onto
    /// `radius` when the grid reaches past it (a frame-free radial scaling: exact
    /// under the rotation). A zero radius stacks them on the point; the Move phase's
    /// coincident-push rule spreads them in the owner's frame.
    ///
    /// Both arms water-eject each point like a release.
    ///
    /// Under spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide a row that sets
    /// DeathSpawnPushback (the Golem, the Lava Hound) takes neither arm: `ring.slide` lays
    /// its members on the small fixed ring of `fixed_slide_ring`, whatever the layout key
    /// says, and they slide out from there (`phase_path16402`).
    fn death_spawn_points(&self, team: Team, pos: Vec2, ring: DeathSpawnRing) -> Vec<Vec2> {
        let DeathSpawnRing { count, unit_radius, flying, facing, angle_shift_deg, radius, slide } = ring;
        let arena = &self.cfg.arena;
        let r = radius.max(0) as i64;
        let points: Vec<Vec2> = match self.cfg.calib.death_spawn_layout {
            #[cfg(not(clash_plant = "death_ring_seat_rotated"))]
            _ if slide => fixed_slide_ring(pos, count, radius),
            // PLANT: Red's ring is the seat rotation of Blue's (turned 180 degrees about the death point).
            #[cfg(clash_plant = "death_ring_seat_rotated")]
            _ if slide => fixed_slide_ring(pos, count, radius).into_iter().map(|p| if team == Team::Red { pos.add(pos).sub(p) } else { p }).collect(),
            DeathSpawnLayout::FacingRing => {
                let n = count.max(1);
                let u = if facing == Vec2::default() {
                    match team {
                        Team::Blue => Vec2::new(0, 256),
                        Team::Red => Vec2::new(0, -256),
                    }
                } else {
                    facing
                };
                let ulen = isqrt(u.len2()).max(1);
                (0..n)
                    .map(|k| {
                        let deg = angle_shift_deg + k * 360 / n;
                        let (sn, cs) = (crate::formation::sin1024(deg) as i64, crate::formation::sin1024(deg + 90) as i64);
                        // rotate the unit facing by deg, scale to r: (ux cos - uy sin, ux sin + uy cos)
                        let rx = ((u.x as i64) * cs - (u.y as i64) * sn) * r / (ulen * 1024);
                        let ry = ((u.x as i64) * sn + (u.y as i64) * cs) * r / (ulen * 1024);
                        pos.add(Vec2::new(rx as i32, ry as i32))
                    })
                    .collect()
            }
            DeathSpawnLayout::FacingRingRounded => {
                let n = count.max(1);
                let u = if facing == Vec2::default() {
                    match team {
                        Team::Blue => Vec2::new(0, 256),
                        Team::Red => Vec2::new(0, -256),
                    }
                } else {
                    facing
                };
                #[cfg(not(clash_plant = "death_ring_unrounded"))]
                let a = crate::formation::rounded_degree(u);
                #[cfg(clash_plant = "death_ring_unrounded")]
                let a = crate::formation::rounded_degree(u) + 1; // PLANT (regression): a degree off the rounding.
                (0..n)
                    .map(|k| {
                        let deg = a + angle_shift_deg + k * 360 / n;
                        let rx = r * crate::formation::sin1024(deg + 90) as i64 / 1024;
                        let ry = r * crate::formation::sin1024(deg) as i64 / 1024;
                        pos.add(Vec2::new(rx as i32, ry as i32))
                    })
                    .collect()
            }
            DeathSpawnLayout::EngineGridWithinRadius => self
                .formation_grid(team, count, unit_radius, flying, pos)
                .into_iter()
                .map(|p| {
                    let off = p.sub(pos);
                    let d2 = off.len2();
                    if d2 > r * r {
                        let len = isqrt(d2).max(1);
                        pos.add(Vec2::new(((off.x as i64) * r / len) as i32, ((off.y as i64) * r / len) as i32))
                    } else {
                        p
                    }
                })
                .collect(),
        };
        points
            .into_iter()
            .map(|q| {
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

    /// THE TICK ORDER IN FORCE (calibration match.TICK_ORDER): the measured
    /// order, or the legacy one. The regression plant `phase_order` forces the
    /// legacy order whatever the ledger says -- Move before Attack and the deploy
    /// countdown in Upkeep, the earlier engine -- so that
    /// tests/tick_order.rs and every phase-traced battle go red under it.
    #[inline]
    fn tick_order(&self) -> TickOrder {
        #[cfg(clash_plant = "phase_order")]
        {
            return TickOrder::LegacyMoveBeforeAttack; // PLANT (regression): the earlier order.
        }
        #[allow(unreachable_code)]
        self.cfg.calib.tick_order
    }

    /// Advance one logic tick by running the phase list match.TICK_ORDER selects
    /// (lib.rs `TICK_PHASES`, or `LEGACY_TICK_PHASES`) in order.
    pub fn tick(&mut self) {
        if self.outcome.is_some() {
            return;
        }
        let phases = match self.tick_order() {
            TickOrder::Client16402 => TICK_PHASES,
            TickOrder::LegacyMoveBeforeAttack => LEGACY_TICK_PHASES,
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
        if self.tick_order() == TickOrder::LegacyMoveBeforeAttack {
            // the legacy order matures deploy timers here, before the walk
            self.deploy_countdown();
        }
    }

    /// THE DEPLOY COUNTDOWN -- the per-unit character update, which runs AFTER the
    /// move pass as measured (calibration match.TICK_ORDER): every deploying
    /// unit's timer loses TICK_MS, and the one
    /// that reaches 0 is deployed (`on_deployed`: hide start state, spawner
    /// activation). Run at the end of the Move phase under the shipped order, so
    /// a unit whose timer ends this tick was still deploying when Path looked at it
    /// (the 4 -> 1 transition takes effect after the move: 398 / 445 stood still)
    /// and walks the next tick -- which is spawn + DeployTime / TICK_MS, exactly
    /// where movement.DEPLOY_TIMING measured the first full-length step, because
    /// the countdown now also runs on the spawn tick itself (the new unit sat out
    /// that tick's move pass). Under the legacy order it runs in Upkeep, before the
    /// walk, and the unit walks on the tick its timer ends. A stun pauses it
    /// either way (status.STUN_PAUSES_DEPLOY_TIMER).
    fn deploy_countdown(&mut self) {
        let dt = self.cfg.calib.tick_ms;
        let paused_by_stun = self.cfg.calib.stun_pauses_deploy;
        for i in 0..self.ents.capacity() {
            if self.ents.alive[i] && self.ents.deploy_ms[i] > 0 && !(paused_by_stun && self.ents.stun_ms[i] > 0) {
                self.ents.deploy_ms[i] = (self.ents.deploy_ms[i] - dt).max(0);
                self.ents.stagger_ms[i] = (self.ents.stagger_ms[i] - dt).max(0);
                if self.ents.deploy_ms[i] == 0 {
                    self.on_deployed(i);
                }
            }
        }
    }

    /// The hold timer and every buff slot tick down by one tick. Where this runs is
    /// calibration status.BUFF_EXPIRY_TICK_ALIGNMENT: in Resolve just before new
    /// buffs land (ceil_from_next_tick: a D-ms buff applied in tick N holds ticks
    /// N+1..N+ceil(D/dt)) or at the start of Status (one_tick_short). ONE alignment for both, because `stun_ms` is now
    /// derived from a full-stop buff (status.FULL_STOP_BUFF_IS_STUN) and the two
    /// would part if they expired on different ticks.
    fn tick_status_timers(&mut self) {
        let dt = self.cfg.calib.tick_ms;
        for i in 0..self.ents.capacity() {
            if !self.ents.alive[i] {
                continue;
            }
            self.ents.stun_ms[i] = (self.ents.stun_ms[i] - dt).max(0);
            for slot in self.ents.buff_slots_mut(i) {
                if slot.is_empty() {
                    continue;
                }
                slot.ms -= dt;
                if slot.ms <= 0 {
                    *slot = BuffSlot::default();
                }
            }
        }
    }

    /// THE DAMAGE-OVER-TIME AND HEAL PULSES of every buff on the board, into this
    /// tick's damage buffer (Status phase, so they resolve with the tick's other
    /// damage rather than a tick late). Each slot carries its own clock and its own
    /// already-level-scaled amount; the amount is negative for a heal, and a heal
    /// never takes a unit above its max hp.
    ///
    /// The per-pulse figure is `BuffDef::pulse_base` scaled by the caster
    /// (status.BUFF_PULSE_AMOUNT); the crown-tower percent and BuildingDamagePercent
    /// of the BUFF apply to it, not the spell's. A pulse that would deal 0 after
    /// those percents still consumes its period.
    fn buff_pulse_pass(&mut self) {
        let dt = self.cfg.calib.tick_ms;
        let rounding = self.cfg.calib.crown_rounding;
        let table = &self.cfg.cards.buffs;
        for i in 0..self.ents.capacity() {
            if !self.ents.alive[i] || self.ents.hp[i] <= 0 {
                continue;
            }
            let kind = self.ents.kind[i];
            let id = self.ents.id_of(i);
            let a = i * crate::status::MAX_BUFFS_PER_ENTITY;
            for k in 0..crate::status::MAX_BUFFS_PER_ENTITY {
                let slot = self.ents.buffs[a + k];
                if slot.is_empty() || slot.pulse_amount == 0 {
                    continue;
                }
                let Some(def) = table.get(slot.id as usize - 1) else { continue };
                if def.no_effect_to_crown_towers && kind.is_crown_tower() {
                    continue;
                }
                let mut left = slot.pulse_ms - dt;
                let mut fired = 0;
                while left <= 0 {
                    fired += 1;
                    left += def.hit_frequency_ms.max(dt);
                }
                self.ents.buffs[a + k].pulse_ms = left;
                if fired == 0 {
                    continue;
                }
                let amount = slot.pulse_amount * fired;
                if amount > 0 {
                    // A BUILDING takes BuildingDamagePercent (Earthquake 350) and a
                    // crown tower CrownTowerDamagePercent (Poison 23); a crown tower is
                    // a building too, and the percent for it is the crown one, so it
                    // wins. THE TWO PERCENTS TAKE DIFFERENT ROUTES: `damage_against`
                    // is the CROWN-TOWER reduction and returns `amount` untouched for
                    // anything else, so an ordinary building's percent has to be
                    // applied here. An earlier version let one call handle both,
                    // which silently dropped the Earthquake's 350 and dealt a Cannon
                    // a Poison-sized 32 a pulse
                    // (`tests/status.rs::an_earthquake_deals_a_building_its_own_percent`).
                    // Buff damage is non-negative and the percent is non-negative, so
                    // the building scale is a plain truncating division.
                    let dealt = if kind.is_crown_tower() {
                        crate::combat::damage_against(kind, amount, def.crown_pct, rounding)
                    } else if kind == EntityKind::Building {
                        (amount as i64 * def.building_pct as i64 / 100) as i32
                    } else {
                        amount
                    };
                    if dealt > 0 {
                        self.dmg.hits.push(Hit { target: id, amount: dealt, ignores_hide: false });
                    }
                } else {
                    // A HEAL, capped at the missing hitpoints. It is applied here rather
                    // than buffered, because the damage buffer is a buffer of DAMAGE and
                    // a negative hit would make every shield and death test read a sign.
                    let room = (self.ents.max_hp[i] - self.ents.hp[i]).max(0);
                    self.ents.hp[i] += room.min(-amount);
                }
            }
        }
    }

    /// `lifetime_drain_per_tick` for a live entity, in HUNDREDTHS of a hitpoint: 0
    /// when the entity is gone, its card has no LifeTime, or lifetime.HP_DECAY is not
    /// `linear_drain`. The number every caller needs to say how long a building lives
    /// (it empties after ceil(max_hp x 100 / drain) ticks past its deploy end).
    pub fn lifetime_drain(&self, id: EntityId) -> i32 {
        if self.cfg.calib.lifetime_hp_decay != LifetimeDecay::LinearDrain || !self.ents.is_alive(id) {
            return 0;
        }
        self.lifetime_drain_per_tick(id.index as usize).unwrap_or(0)
    }

    /// THE PER-TICK LIFETIME DRAIN of entity `i`, in HUNDREDTHS of a hitpoint, or
    /// None when its card has no LifeTime (lifetime.HP_DECAY = linear_drain).
    ///
    /// The rate is derived once from the building's max hitpoints and its LifeTime:
    ///
    /// > drain = ((maxHitpoints * 100000) / LifeTime_ms) / 20
    ///
    /// truncated to hundredths of a hitpoint BEFORE it is accumulated -- that is what
    /// makes it fit the recordings (92.2 % of 31113 live building frames exact, where
    /// the exact fraction max_hp x k / LifeTime_ticks recomputed every tick fits 46 %).
    /// A level-11 Tesla (1182 hp, LifeTime 25000) gets 118200000 / 25000 = 4728, then
    /// 4728 / 20 = 236 hundredths per tick: 2.36, not the 2.364 the exact fraction
    /// gives, so the Tesla outlives its own column by one tick (the live Teslas die on
    /// tick 501 of a 500-tick LifeTime). `lifetime_ms` carries the column (buildings
    /// only; the loader refuses a troop that ships one).
    ///
    /// A rate of 0 (a building whose max hp is under LifeTime_ms / 5000 -- no shipped
    /// card) never crosses a hitpoint, so nothing drains and nothing expires.
    fn lifetime_drain_per_tick(&self, i: usize) -> Option<i32> {
        let life = (*self.lifetime_ms.get(i)?)?;
        if life <= 0 {
            return None;
        }
        Some((self.ents.max_hp[i] as i64 * 100_000 / life as i64 / 20) as i32)
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
        self.buff_pulse_pass();
        let lifetime_paused = self.cfg.calib.stun_pauses_building_lifetime;
        #[cfg(not(clash_plant = "lifetime_expiry_hit"))]
        let decay = self.cfg.calib.lifetime_hp_decay;
        // PLANT (regression): the earlier engine, which kept a building at full hp
        // for its whole LifeTime and killed it in one hit at the end.
        #[cfg(clash_plant = "lifetime_expiry_hit")]
        let decay = LifetimeDecay::ExpiryHit;
        for i in 0..self.ents.capacity() {
            if !self.ents.alive[i] {
                continue;
            }
            if lifetime_paused && self.ents.stun_ms[i] > 0 {
                continue;
            }
            // ignores_hide: a Tesla whose time is up dies under ground too, and a
            // hidden one drains under ground too (combat.rs `resolve` drops every
            // other hit on a Hidden building; the live 2.36 hp/tick was measured on
            // Teslas that spent their lives hidden).
            #[cfg(not(clash_plant = "expiry_respects_hide"))]
            let ignores_hide = true;
            #[cfg(clash_plant = "expiry_respects_hide")]
            let ignores_hide = false; // PLANT: a hidden Tesla lives for ever.
            match decay {
                LifetimeDecay::LinearDrain => {
                    // THE DRAIN (lifetime.HP_DECAY = linear_drain, measured): the
                    // hundredths accumulator gains the per-tick rate and every whole
                    // hitpoint it crosses comes off the hp pool as a hit -- so damage
                    // and the drain share one pool and a death by drain goes through
                    // the normal death path (a Tombstone that runs out still spawns
                    // its four Skeletons). It starts at the deploy end: a live
                    // building's hp is at max on every frame of its deploy window.
                    if self.ents.deploy_ms[i] > 0 {
                        continue;
                    }
                    let Some(rate) = self.lifetime_drain_per_tick(i) else { continue };
                    if self.lifetime_acc.len() <= i {
                        self.lifetime_acc.resize(i + 1, 0);
                    }
                    let acc = self.lifetime_acc[i] + rate;
                    let whole = acc / 100;
                    self.lifetime_acc[i] = acc - whole * 100;
                    if whole > 0 {
                        self.dmg.hits.push(Hit { target: self.ents.id_of(i), amount: whole, ignores_hide });
                    }
                }
                LifetimeDecay::ExpiryHit => {
                    if let Some(Some(left)) = self.lifetime_ms.get_mut(i) {
                        *left -= dt;
                        if *left <= 0 {
                            // Expiry is a hit for everything it has, so it goes through
                            // the same buffer and death path as any other damage.
                            let amount = self.ents.hp[i].max(0).saturating_add(self.ents.shield[i].max(0)).max(1);
                            self.dmg.hits.push(Hit { target: self.ents.id_of(i), amount, ignores_hide });
                            self.lifetime_ms[i] = None;
                        }
                    }
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
            self.ents.stagger_ms[id.index as usize] = p.stagger_ms;
            self.ents.death_slide_centre[id.index as usize] = p.slide_centre;
            self.ents.death_slide_radius[id.index as usize] = p.slide_radius;
            if let Some(f) = p.facing {
                self.ents.facing[id.index as usize] = f;
            }
            // A death spawn queued under spawner.RELEASE_TIMING = next_spawn_phase: its first
            // tick is this one, and the delay counts from it.
            #[cfg(not(clash_plant = "acquire_delay_dropped_in_queue"))]
            if p.acquire_delay {
                self.delay_acquisition(id.index as usize);
            }
            if let Some(d) = p.deploy_ms {
                self.ents.deploy_ms[id.index as usize] = d;
                if d == 0 {
                    self.on_deployed(id.index as usize);
                }
            }
        }
        self.hash.rebuild(&self.ents);
        if self.emission_timing() == SpawnerEmission::SpawnPhaseNextTick {
            // The earlier arm: periodic spawners emit into the (now empty) queue, so
            // their units materialise NEXT tick. The shipped arm runs the pass in
            // Move instead (spawner.EMISSION_TIMING).
            self.spawner_pass();
        }
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
                tick: self.tick,
                doomed: &[],
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
        self.phase_target_for(None);
    }

    /// The Target phase, for every unit (`only` None) or for a first update's fresh units alone
    /// (`first_update`), which leaves the hide pass to the tick's own phase.
    fn phase_target_for(&mut self, only: Option<&[usize]>) {
        self.hash.rebuild(&self.ents);
        let mut decisions = std::mem::take(&mut self.scratch.decisions);
        let mut nb = std::mem::take(&mut self.scratch.nb);
        decisions.clear();
        if only.is_none() {
            self.hide_pass(&mut nb);
        }
        // targeting.DOOMED_TARGET_DROP = projectile_attackers: who is doomed by the shots in flight at
        // the tick's start, before any decision reads it.
        let doomed_drop: Vec<bool> = if self.cfg.calib.doomed_target_drop.drops() {
            combat::doomed_by_shots_in_flight(&self.ents, &self.projectiles, self.cfg.calib.crown_rounding, self.cfg.calib.tick_ms, target::DOOMED_ETA_LIMIT_MS)
        } else {
            Vec::new()
        };
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
                tick: self.tick,
                doomed: &doomed_drop,
            };
            for i in 0..self.ents.capacity() {
                if self.ents.alive[i] && self.cfg.cards.get(self.ents.card[i]).hit_speed_ms > 0 && only.map_or(true, |o| o.contains(&i)) {
                    decisions.push((i, target::decide(&ctx, i, &mut nb)));
                }
            }
        }
        let carry = self.cfg.calib.resume_retarget_windup == ResumeWindup::Carry;
        let retarget_resets_charge = self.cfg.calib.charge_reset_on_retarget;
        let keep_cycle_when_dead =
            matches!(self.cfg.calib.retarget_progress, RetargetProgress::KeepWhenDead | RetargetProgress::KeepWhenDeadOrInReach);
        #[cfg(not(clash_plant = "reach_switch_resets_swing"))]
        let keep_cycle_in_reach = self.cfg.calib.retarget_progress == RetargetProgress::KeepWhenDeadOrInReach;
        #[cfg(clash_plant = "reach_switch_resets_swing")]
        let keep_cycle_in_reach = false; // PLANT (regression): a switch to a target in reach restarts the swing.
        let calib = &self.cfg.calib;
        let wait_mode = self.cfg.calib.post_kill_wait;
        let wait_arm = wait_mode != PostKillWait::None;
        let wait_ticks = self.cfg.calib.post_kill_wait_ticks.max(1) as i16;
        let wait_units = &self.cfg.calib.post_kill_wait_units;
        let override_units = &self.cfg.calib.post_kill_wait_override_units;
        let cards = &self.cfg.cards;
        // THE DOOMED TEST (client16402_attack_finish): the homing projectiles in flight at the
        // tick's start -- the state the previous tick ended in -- summed per target, level-scaled
        // as fired, against the target's hitpoints. From every source; a non-homing shot (a
        // Bomber's bomb, a Princess arrow) never counts, as the corpus shows. Crown scaling is not
        // applied: untested, and immaterial on the corpus.
        let doomed_now: Vec<bool> = if wait_mode == PostKillWait::AttackFinish {
            let mut pending = vec![0i64; self.ents.capacity()];
            for p in &self.projectiles {
                if p.firer_card.is_some_and(|c| cards.get(c).projectile_homing) && self.ents.is_alive(p.target) {
                    pending[p.target.index as usize] += p.damage as i64;
                }
            }
            (0..self.ents.capacity()).map(|j| pending[j] > 0 && pending[j] >= self.ents.hp[j] as i64).collect()
        } else {
            Vec::new()
        };
        for &(i, d) in &decisions {
            let e = &mut self.ents;
            // combat.POST_KILL_RETARGET_WAIT, either waiting arm. The loss L is the victim's death
            // tick (the first frame whose target reads none); this Target phase, L + 1, is the first
            // to find the target dead. A unit the arm makes wait (a LISTED unit under
            // client16402_measured_list; under the shipped client16402_attack_finish, one that none
            // of (a)-(c) below frees) then holds with no target -- the
            // decision below is not taken, whatever it found, an enemy already in range included --
            // for L + 1 .. L + 5, its attack timer zeroed on L + 5, and takes this phase's decision
            // on L + 6. The hold keeps it out of the walk (the Path phase holds a waiting unit like
            // an attacking one) and freezes its timers (phase_attack).
            // client16402_attack_finish: while the target lives, remember whether it is doomed, so
            // that at the loss this holds its last live tick.
            if wait_mode == PostKillWait::AttackFinish {
                if let Some(t) = e.target[i].filter(|t| e.is_alive(*t)) {
                    e.target_doomed[i] = doomed_now[t.index as usize];
                }
            }
            if wait_arm {
                if e.retarget_wait[i] > 0 {
                    e.retarget_wait[i] -= 1;
                    if e.retarget_wait[i] > 0 {
                        if e.retarget_wait[i] == 1 {
                            e.attack_phase[i] = AttackPhase::Idle;
                            e.attack_ms[i] = 0;
                        }
                        e.target[i] = None;
                        continue;
                    }
                } else if e.target[i].is_some_and(|t| !e.is_alive(t))
                    && match wait_mode {
                        PostKillWait::MeasuredList => wait_units.iter().any(|u| *u == cards.get(e.card[i]).unit_name),
                        // (a) an OverrideAttackFinishTime card, (b) progress 0 at the loss (the engine's
                        // progress runs on after a hit, as the game's does: a Knight reads 1200 at its
                        // kill), (c) a projectile card whose victim was doomed: any one skips the wait.
                        PostKillWait::AttackFinish => {
                            let c = cards.get(e.card[i]);
                            !override_units.contains(&c.unit_name) && e.attack_ms[i] != 0 && !(c.projectile.is_some() && e.target_doomed[i])
                        }
                        PostKillWait::None => false,
                    }
                {
                    e.retarget_wait[i] = wait_ticks - 1;
                    e.target[i] = None;
                    e.target_locked[i] = false;
                    if e.retarget_wait[i] > 0 {
                        continue;
                    }
                }
            }
            let changed = e.target[i] != d.target;
            // the target this unit had AND STILL HAS: `None` once it is dead, which is the
            // distinction both rules below turn on.
            let was = e.target[i].filter(|t| e.is_alive(*t));
            // CHARGE (calibration charge.RESET_ON_RETARGET, shipped false): switching
            // from one LIVE target to a DIFFERENT one clears the charge and the run-up.
            // Acquiring a first target, or replacing a dead one, is not a switch: a
            // Prince whose Skeleton dies under it and who then picks the tower was not
            // distracted, and under `true` it would otherwise lose its charge to every
            // kill it makes.
            if retarget_resets_charge && was.is_some() && d.target.is_some() && was != d.target {
                e.charged[i] = false;
                e.charge_progress[i] = 0;
            }
            if d.resumed {
                e.retarget_on_resume[i] = false;
            }
            // status.RESUME_RETARGET_WINDUP = carry: a paused windup follows the unit to
            // the target its resume rescan picked, instead of being cancelled.
            let carried = d.resumed && carry && !d.cancel_attack;
            // combat.RETARGET_PROGRESS = keep_when_dead: REPLACING A CORPSE IS NOT A SWITCH,
            // the same distinction the charge rule above already draws, applied to the attack
            // cycle. Under `reset_always` a unit whose victim died threw away the time since
            // its last shot and restarted, so a princess tower killing one-shot skeletons
            // reloaded in 19 ticks against its own HitSpeed of 16 -- once per target change,
            // which against a swarm is once per shot. The cost is the elapsed time MINUS the
            // LoadTime the fresh cycle credits back (combat.ATTACK_CYCLE), so it fell on
            // short-LoadTime shooters only: the tower's LoadTime is 0 and it paid the lot, a
            // Musketeer's 300 ms covered the gap and it paid nothing. BOTH PATHS INTO THE
            // RESET ARE SUPPRESSED, not just the `changed` one: a locked unit whose target
            // dies arrives here with `cancel_attack` set by target.rs instead, and gating
            // only `changed` would have left the tower's own case untouched.
            let replaced_a_corpse = keep_cycle_when_dead && was.is_none() && d.target.is_some();
            // combat.RETARGET_PROGRESS = keep_when_dead_or_in_reach: a switch from a LIVE target to one already in
            // attack range this tick keeps the swing as well. Only the `changed` route is widened: a
            // `cancel_attack` from a broken lock still cancels.
            let switched_in_reach = keep_cycle_in_reach
                && was.is_some()
                && d.target.filter(|t| e.is_alive(*t) && Some(*t) != was).is_some_and(|t| {
                    let ti = t.index as usize;
                    target::in_attack_range(calib, e.pos[i], cards.get(e.card[i]).range, e.radius[i], e.pos[ti], e.radius[ti])
                });
            if !replaced_a_corpse
                && (d.cancel_attack || (changed && e.attack_phase[i] == AttackPhase::Windup && !carried && !switched_in_reach))
            {
                e.attack_phase[i] = AttackPhase::Idle;
                e.attack_ms[i] = 0;
                e.target_locked[i] = false;
            }
            if changed {
                e.fired_at[i] = None;
            }
            // The launch beyond reach has had its re-evaluation (target.rs `decide`).
            e.launched_beyond[i] = false;
            e.target[i] = d.target;
            if wait_mode == PostKillWait::AttackFinish {
                if let Some(t) = d.target.filter(|t| e.is_alive(*t)) {
                    e.target_doomed[i] = doomed_now[t.index as usize];
                }
            }
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

    /// THE DOOMED MASK (calibration movement.DYING_UNIT_VISIBILITY): which troops
    /// the damage buffered BEFORE the move pass kills this tick -- this tick's
    /// Attack-phase hits, the Status-phase lifetime expiries and the previous
    /// Reap's death damage, which are the hits that have landed before the move
    /// pass in the measured order (projectile arrivals and spells land after it,
    /// Phase::Projectile). Derived exactly as combat.rs `resolve` will apply it:
    /// dead targets and non-positive amounts skipped, hide immunity respected,
    /// a shield absorbs the tick's sum before hp is touched. Buildings are never
    /// masked (a dying building is seen by every mover, 3 / 3 on the corpus: nothing
    /// drops it inside the pass). A read: no hp is written here.
    /// Under `whole_tick` the mask is all false.
    fn doomed_mask(&mut self) {
        let cap = self.ents.capacity();
        let doomed = &mut self.scratch.doomed;
        doomed.clear();
        doomed.resize(cap, false);
        if self.cfg.calib.dying_unit_visibility == DyingUnitVisibility::WholeTick || self.dmg.hits.is_empty() {
            return;
        }
        let e = &self.ents;
        let hidden_immune = self.cfg.calib.hide_hidden_immune;
        let sums = &mut self.scratch.sums;
        sums.clear();
        sums.resize(cap, 0);
        for h in &self.dmg.hits {
            if !e.is_alive(h.target) || h.amount <= 0 {
                continue;
            }
            let t = h.target.index as usize;
            if hidden_immune && !h.ignores_hide && e.hide[t] == HideState::Hidden {
                continue;
            }
            sums[t] += h.amount as i64;
        }
        for i in 0..cap {
            if sums[i] > 0 && e.alive[i] && e.kind[i] == EntityKind::Troop && e.shield[i] <= 0 && (e.hp[i] as i64) - sums[i] <= 0 {
                doomed[i] = true;
            }
        }
    }

    /// THE 16.402 MOVEMENT UPDATE, as measured on client 16.402 (PATH_SEARCH =
    /// client16402; path16402.rs for the search and the replan gate, move16402.rs
    /// for the contact law; the evidence is in calibration.json).
    /// Ground troops are updated ONE AFTER THE OTHER in array order and
    /// each sees the units before it already moved: the
    /// `bodies` array is that view, and the resulting
    /// displacements are handed to `phase_move` as deltas so the engine's position
    /// write stays where it is. The order is `Entities::creation_seq` (calibration
    /// match.TICK_ORDER), and a troop the buffered damage kills this tick drops out
    /// of the array view at its own place in it (movement.DYING_UNIT_VISIBILITY,
    /// `doomed_mask`). Per unit:
    ///   0. mid-knockback (`push_active`): the PUSHBACK TICK replaces everything
    ///      below -- no path request, the separation scan while the speed is still
    ///      positive (and the water ejection), the countdown, `move_towards` the
    ///      ladder's target (move16402.rs `pushback_step`), the path dropped when it
    ///      ends;
    ///   1. held (stunned, knocked back, frozen): nothing;
    ///   2. the replan gate: search when there is no path, the goal
    ///      cell moved, or the own side's occluder set changed (SAMEPATH retention);
    ///   3. avoidance scan, offset decay, separation scan;
    ///   4. the step toward the current waypoint's centre at the unit's speed -- 0 while
    ///      deploying, attacking or stomp-paused, in which case only the collision mean
    ///      moves it -- with the avoidance rotation, the position write and the
    ///      reached test that pops the waypoint and refreezes the segment direction.
    fn phase_path16402(&mut self) {
        self.phase_path16402_for(None, &[], &[]);
    }

    /// combat.DASH_ATTACK's blows, landed after the move pass that decided them (the pass holds
    /// the entity table borrowed): a dash with no DashRadius hits its dash target alone, one with
    /// a radius every enemy it reaches from `centre`, both at DashDamage at the unit's level and
    /// through the crown-tower percent of its ordinary hit. Resolved with the tick's other hits.
    fn land_dash_blows(&mut self, blows: Vec<(usize, Option<EntityId>, Vec2)>) {
        for (a, target, centre) in blows {
            let card = self.cfg.cards.get(self.ents.card[a]);
            let Some(d) = card.dash else { continue };
            let amount = self.cfg.cards.scaled(self.ents.card[a], self.ents.level[a], d.damage).expect("level validated at spawn");
            let pct = card.crown_tower_damage_percent;
            match d.radius {
                None => {
                    if let Some(t) = target.filter(|t| self.ents.is_alive(*t)) {
                        let kind = self.ents.kind[t.index as usize];
                        let amount = combat::damage_against(kind, amount, pct, self.cfg.calib.crown_rounding);
                        self.dmg.hits.push(Hit { target: t, amount, ignores_hide: false });
                    }
                }
                Some(r) => combat::splash(
                    &self.ents,
                    &self.hash,
                    self.ents.team[a],
                    centre,
                    r,
                    card.attacks_air,
                    card.attacks_ground,
                    amount,
                    pct,
                    self.cfg.calib.crown_rounding,
                    &mut self.dmg,
                    &mut self.scratch.nb,
                ),
            }
        }
    }

    /// A dash that has ended (combat.DASH_ATTACK): the attack cycle restarts from its load time, so
    /// the first hit lands HitSpeed / 50 - 1 ticks later (measured on client 15.535.29: on the
    /// first tick out of the dash the load timer reads LoadTime, on the next the attack progress
    /// reads 100, then +50 a tick), and damage is discarded for DashImmuneToDamageTime more.
    fn end_dashes(&mut self, ended: Vec<usize>) {
        let tk = self.cfg.calib.tick_ms.max(1);
        for i in ended {
            let card = self.cfg.cards.get(self.ents.card[i]);
            self.ents.attack_phase[i] = AttackPhase::Idle;
            self.ents.attack_ms[i] = 0;
            #[cfg(not(clash_plant = "dash_keeps_the_cycle"))]
            {
                self.ents.attack_load_ms[i] = card.load_time_ms.max(0);
            }
            self.ents.target_locked[i] = false;
            self.ents.dash_immune_until[i] = match card.dash.and_then(|d| d.immune_ms) {
                Some(ms) => self.tick + (ms / tk) as u32,
                None => 0,
            };
        }
    }

    /// The 16.402 movement update, for every troop (`only` None) or for a first update's fresh
    /// units alone (`first_update`). Those step against the board as the tick's pass left it: the
    /// obstacle set, the grid and the doomed mask are the pass's own and are not rebuilt, and a unit
    /// the mask dooms, which the pass dropped at its own place, is hidden from their scans, as are
    /// the `unseen` ones. `blockers` (native x, y, radius, side) join the board as bodies that are
    /// collidable but not alive: the avoidance scan sees them as static blockers, the separation scan
    /// skips them. The per-tick diagnostics of every other unit stay as the pass wrote them.
    fn phase_path16402_for(&mut self, only: Option<&[usize]>, unseen: &[usize], blockers: &[(i32, i32, i32, u8)]) {
        use crate::fixed::SUBTILE_PER_MILLITILE as K;
        if only.is_none() {
            self.build_obstacles();
            self.doomed_mask();
        }
        let mut g = self.scratch.grid16402.take().unwrap_or_else(|| Grid16402::new(&self.cfg.arena, &self.cfg.calib));
        g.refresh(self.scratch.occluder_epoch, &self.scratch.obstacles[0]);
        let cap = self.ents.capacity();
        // THE TORNADO'S ATTRACT (status.ATTRACT_LAW): one vector per entity, computed
        // from the START-OF-TICK positions, before anything moves and before the entity
        // arrays are borrowed.
        //
        // A PRE-PASS RATHER THAN A LOOKUP INSIDE THE LOOP, for a reason the measurement
        // forces. The move pass runs in creation order, so a pull that read live
        // positions would make one unit's displacement depend on who moved first. The
        // corpus says the direction is taken from the tick-start position: at 12 native
        // from the centre a 59-native pre-move rewrites it completely, and "walk first,
        // then pull" has no legal walk on 9 of the Knight's 11 close ticks while this
        // order has one on 11 of 11.
        //
        // THE SOURCE IS THE LIVE AREA EFFECT, NEVER THE BUFF SLOT. With BuffTime 500 ms
        // and CapBuffTimeToAreaEffectTime false, the last application outlives the area
        // by ten ticks -- and the Giant's displacement is exactly (0, 0) on two of them.
        let attract: Vec<(i32, i32)> = {
            let sources: Vec<AttractSource> = self
                .spells
                .iter()
                .filter_map(|s| {
                    let crate::spell::SpellMotion::Pulsing(p) = &s.motion else { return None };
                    if p.life_ms <= 0 {
                        return None;
                    }
                    let Some(crate::card::SpellDef {
                        shape: crate::card::SpellShape::PulsingAreaEffect { hit, .. }, ..
                    }) = &self.cfg.cards.get(s.card).spell
                    else {
                        return None;
                    };
                    let pct = match hit.buff {
                        Some(b) => self.cfg.cards.buffs[b.buff as usize].attract_pct,
                        None => 0,
                    };
                    if pct == 0 {
                        return None;
                    }
                    Some(AttractSource {
                        pos: p.pos,
                        radius: hit.radius,
                        team: s.team,
                        pct,
                        hits_air: hit.hits_air,
                        hits_ground: hit.hits_ground,
                        ignore_buildings: hit.ignore_buildings,
                        only_enemies: hit.only_enemies,
                    })
                })
                .collect();
            let mut out = vec![(0i32, 0i32); cap];
            for (i, slot) in out.iter_mut().enumerate() {
                if sources.is_empty() || !self.ents.alive[i] {
                    continue;
                }
                // THE BASE IS `effective_speed` UNCONDITIONALLY, not the walk's speed for
                // this tick: the Giant was pulled at its full 187 on both of the two ticks
                // its stomp pause had the walk at zero.
                let s_native = match self.cfg.calib.attract_base {
                    AttractBase::EffectiveSpeed => self.effective_speed(i) / K,
                    AttractBase::BaseSpeed => self.ents.speed[i] / K,
                };
                if s_native <= 0 {
                    continue;
                }
                let flying = self.ents.flying[i];
                let building = self.ents.kind[i] != EntityKind::Troop;
                let mut acc = (0i32, 0i32);
                for a in &sources {
                    if a.only_enemies && self.ents.team[i] == a.team {
                        continue;
                    }
                    if (flying && !a.hits_air) || (!flying && !a.hits_ground) || (building && a.ignore_buildings) {
                        continue;
                    }
                    // ELIGIBILITY IN SUBTILES, against the same centre and the same
                    // `spells.AOE_HIT_TEST` the damage uses, so a unit the area effect
                    // HITS is exactly a unit it PULLS and the two cannot drift apart.
                    let edge = match self.cfg.calib.aoe_hit_test {
                        AoeHitTest::EdgeInclusive => self.ents.radius[i],
                        AoeHitTest::CentreInRadius => 0,
                    };
                    let (dx, dy) = ((a.pos.x - self.ents.pos[i].x) as i64, (a.pos.y - self.ents.pos[i].y) as i64);
                    let reach = (a.radius + edge) as i64;
                    if dx * dx + dy * dy > reach * reach {
                        continue;
                    }
                    // THE STEP IN NATIVE, the units the move pass works in. normalize_to
                    // truncates both axes, which is the measured rounding: ceil is
                    // refuted by the Giant's 187.2 landing on 187 twice.
                    let l = move16402::tdiv(s_native * a.pct, 100);
                    let mut v = ((a.pos.x - self.ents.pos[i].x) / K, (a.pos.y - self.ents.pos[i].y) / K);
                    move16402::normalize_to(&mut v, l);
                    acc.0 += v.0;
                    acc.1 += v.1;
                }
                *slot = acc;
            }
            out
        };
        let mut deltas = std::mem::take(&mut self.scratch.deltas);
        deltas.clear();
        deltas.resize(cap, Vec2::default());
        let mut walk_step = std::mem::take(&mut self.scratch.walk_step);
        walk_step.clear();
        walk_step.resize(cap, 0);
        let mut pushed = std::mem::take(&mut self.scratch.pushed);
        pushed.clear();
        pushed.resize(cap, false);
        let mut jumped = std::mem::take(&mut self.scratch.jumped);
        jumped.clear();
        jumped.resize(cap, false);
        let mut routes = std::mem::take(&mut self.ents.route);
        let mut goals = std::mem::take(&mut self.ents.route_goal);
        let mut planned = std::mem::take(&mut self.ents.last_plan_tick);
        let mut segs = std::mem::take(&mut self.ents.seg_dir);
        let mut kticks = std::mem::take(&mut self.ents.move_ticks);
        let mut clocks = std::mem::take(&mut self.ents.stomp_clock);
        // This tick's stomp-clock advance per entity, taken before `ents` is
        // borrowed: it reads the buff list and the calibration, not the walk.
        let advances: Vec<i32> = (0..cap).map(|i| self.stomp_advance(i)).collect();
        let mut facing = std::mem::take(&mut self.ents.facing);
        let mut offsets = std::mem::take(&mut self.ents.avoid_offset);
        // DIAGNOSTIC, cleared every tick so a stale push from an earlier tick cannot be
        // read as this one's. Zero is a real answer here -- most ticks nothing overlaps --
        // so the reader tells absent from zero by whether the row carries the field at all.
        let mut push_applied = std::mem::take(&mut self.ents.push_applied);
        let mut push_neighbours = std::mem::take(&mut self.ents.push_neighbours);
        match only {
            None => {
                push_applied.iter_mut().for_each(|p| *p = Vec2::default());
                push_neighbours.iter_mut().for_each(|c| *c = 0);
            }
            Some(o) => {
                for &i in o {
                    push_applied[i] = Vec2::default();
                    push_neighbours[i] = 0;
                }
            }
        }
        let mut push_speed = std::mem::take(&mut self.ents.push_speed);
        let mut push_active = std::mem::take(&mut self.ents.push_active);
        let mut jumping = std::mem::take(&mut self.ents.jumping);
        // spawner.DEATH_SPAWN_PUSHBACK: the death-spawn slide's state, zeroed here the tick
        // a member reaches its radius.
        let mut slide_c = std::mem::take(&mut self.ents.death_slide_centre);
        let mut slide_r = std::mem::take(&mut self.ents.death_slide_radius);
        // combat.DASH_ATTACK = client_dash: the dash's state, written only here. The blows and the
        // ended dashes are applied after the pass (`land_dash_blows`, `end_dashes`).
        let mut dash_state = std::mem::take(&mut self.ents.dash_state);
        let mut dash_mark = std::mem::take(&mut self.ents.dash_mark);
        let mut dash_goal = std::mem::take(&mut self.ents.dash_goal);
        let mut dash_target = std::mem::take(&mut self.ents.dash_target);
        let mut dash_blocked = std::mem::take(&mut self.ents.dash_blocked);
        let mut dash_immune = std::mem::take(&mut self.ents.dash_immune_until);
        let mut dash_blows: Vec<(usize, Option<EntityId>, Vec2)> = Vec::new();
        let mut dash_ended: Vec<usize> = Vec::new();
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
                tick: self.tick,
                doomed: &[],
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
                    let held = e.held(&self.cfg.cards.buffs, i) || e.knock_ms[i] > 0;
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
                        // a unit mid river-jump is skipped by every neighbour's scans
                        // (jump16402.rs), and so is a dashing one (combat.DASH_ATTACK)
                        collidable: alive && !held && !jumping[i] && dash_state[i] != DashState::Dashing,
                        offset: offsets[i],
                        dir: (facing[i].x, facing[i].y),
                        // states 8/0/2/10 and a busy special attack zero the dot
                        // product: an attacking unit does not steer its neighbours by
                        // heading. A DEPLOYING unit (state 4, not on that list) keeps
                        // its forward heading under movement.DEPLOYING_HEADING = kept;
                        // `zeroed` is the earlier reading, which counted it as a
                        // blocker and turned a same-facing walker 72 degrees.
                        // movement.WAITING_HEADING = zeroed: a member still waiting out its stagger is a
                        // blocker, whatever DEPLOYING_HEADING says of a deploying one.
                        heading_counts: e.attack_phase[i] == AttackPhase::Idle
                            && (e.deploy_ms[i] == 0 || calib.deploying_heading == DeployingHeading::Kept)
                            && !(calib.waiting_heading == WaitingHeading::Zeroed && calib.formation_stagger_wait == StaggerWait::Client16402 && e.stagger_ms[i] > 0),
                    }
                })
                .collect();
            let is_water = |c: i32, r: i32| arena.cell_bits(c, r) & arena.bit_water != 0;
            let cell_bits = |c: i32, r: i32| arena.cell_bits(c, r);
            // arena.json marks LANE_LEFT, LANE_RIGHT, NO_DEPLOY and WATER, and none of
            // them blocks a unit from STANDING where it already is; only the water bit is
            // tested here (knockback.WATER_RESOLUTION not_modelled)
            let blocked_mask: u8 = 0;
            let mut scratch: Vec<usize> = Vec::new();
            // THE UPDATE ORDER IS CREATION ORDER: on every frame pair of the live
            // corpus the units move in the order they were spawned (calibration
            // match.TICK_ORDER). The engine's slots are reused, so the pass is ordered
            // by `creation_seq`, the per-battle creation counter, not by (spawn tick,
            // slot), which put a unit that reused a freed low slot ahead of older units
            // spawned the same tick.
            let mut order: Vec<usize> = (0..cap).filter(|&i| e.alive[i] && e.kind[i] == EntityKind::Troop).collect();
            #[cfg(not(clash_plant = "slot_order_move_pass"))]
            order.sort_by_key(|&i| e.creation_seq[i]);
            #[cfg(clash_plant = "slot_order_move_pass")]
            order.sort_by_key(|&i| (e.spawn_tick[i], i)); // PLANT (regression): the pre-counter order, a reused slot jumps the queue.
            let doomed = &self.scratch.doomed;
            if let Some(o) = only {
                order.retain(|i| o.contains(i));
                for (j, b) in bodies.iter_mut().enumerate() {
                    if unseen.contains(&j) || doomed.get(j).copied().unwrap_or(false) {
                        b.collidable = false;
                    }
                }
                for &(x, y, r, side) in blockers {
                    bodies.push(move16402::Body {
                        x,
                        y,
                        start_x: x,
                        start_y: y,
                        side,
                        r,
                        mass: 0,
                        air: false,
                        mover: false,
                        alive: false,
                        collidable: true,
                        offset: 0,
                        dir: (0, 0),
                        heading_counts: false,
                    });
                }
            }
            for i in order {
                if doomed.get(i).copied().unwrap_or(false) {
                    // ---- A UNIT DYING THIS TICK (calibration movement.DYING_UNIT_VISIBILITY
                    // = creation_order_before_victim; `doomed_mask`): the movers before it
                    // saw it at its start-of-tick position; here, at its own place in the
                    // pass, it is dropped -- no walk, no push of its own, and invisible to
                    // every scan after this point (the INFERRED reading of "processed at
                    // the victim's own place in the move pass"; the hp write and the
                    // despawn stay in Resolve and Reap).
                    bodies[i].collidable = false;
                    continue;
                }
                let deploying = e.deploy_ms[i] > 0;
                let flying = e.flying[i];
                if push_active[i] {
                    // a push ends a dash where it stands (unmeasured: combat.DASH_ATTACK's open list)
                    if dash_state[i] != DashState::None {
                        if dash_state[i] == DashState::Dashing {
                            dash_ended.push(i);
                        }
                        dash_state[i] = DashState::None;
                    }
                    // ---- 0. THE PUSHBACK TICK (taken before any path request or walk
                    // while the ladder is armed -- and before the stun / freeze hold
                    // below: the ladder tests no hold and no state, so a stunned unit
                    // is still carried by it; whether the real game carries a stunned
                    // or frozen unit through its ladder is the engine's guess, recorded
                    // on knockback.DISPLACEMENT_LAW's open list -- no capture has a
                    // stunned unit mid-ladder)
                    let mut con = move16402::Contact { acc: (0, 0), count: 0, offset: offsets[i] };
                    let mut rem = push_speed[i];
                    if rem > 0 {
                        // the separation scan while the speed is still positive (the
                        // NO_CHECKCOLLISIONS tag is not modelled: no card here carries it)
                        move16402::separation_scan(&index, &bodies, i, &mut con, &mut scratch);
                        // a GROUND unit standing on a blocked or water cell is put on
                        // the nearest land first (knockback.WATER_RESOLUTION =
                        // eject_to_nearest_land)
                        let (x, y) = (bodies[i].x, bodies[i].y);
                        if !flying && move16402::blocked_or_water(x, y, arena.cols, arena.rows, arena.bit_water, blocked_mask, cell_bits) {
                            let (nx, ny) = move16402::nearest_land(x, y, arena.cols, arena.rows, is_water);
                            bodies[i].x = nx;
                            bodies[i].y = ny;
                        }
                    }
                    let tgt = (e.push_target[i].x, e.push_target[i].y);
                    // the requested step handed to the charge tail (`charge_pass`):
                    // `d = max(1, dist)`, `min(speed, d, 250)` from the PRE-move position
                    // and the post-decrement speed (no lower clamp: the back-step tick
                    // hands it -25)
                    let d_pre = move16402::distance(bodies[i].x, bodies[i].y, tgt.0, tgt.1).max(1);
                    let m = move16402::pushback_step_extra((bodies[i].x, bodies[i].y), tgt, &mut rem, &mut con, (segs[i].x, segs[i].y), deploying && !flying, is_water, arena.cols, arena.rows, attract[i]);
                    walk_step[i] = rem.min(d_pre).min(250);
                    // the facing is not updated and the offset neither scanned nor
                    // decayed: `offsets[i]` stays what it was
                    debug_assert!(m.dir.is_none());
                    bodies[i].x = m.x;
                    bodies[i].y = m.y;
                    deltas[i] = Vec2::new(m.x * K, m.y * K).sub(e.pos[i]);
                    pushed[i] = true;
                    push_speed[i] = rem;
                    push_active[i] = rem >= 0;
                    if rem < 0 {
                        // the path is dropped the tick the ladder ends; the replan gate
                        // finds it empty next tick
                        routes[i].clear();
                        goals[i] = None;
                        segs[i] = Vec2::default();
                    }
                    continue;
                }
                // ---- THE DEATH-SPAWN SLIDE (spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide;
                // the member was born on `fixed_slide_ring`'s small ring). It REPLACES the
                // walk -- no path request, no avoidance scan, no offset decay, no facing change,
                // no stomp clock -- with a step of move16402::DEATH_SLIDE_STEP straight out from
                // the death point, never past DeathSpawnRadius, plus the contact law's
                // separation mean in the same position write (move16402.rs `death_slide_step`).
                //
                // HERE, at the member's own place in the creation-order move pass, because that
                // is what gives the measured first step: on a Golem death the first-created
                // Golemite finds its sibling still 500 away (radii 500 + 500) and takes the
                // 150 cap on top of its 250 (250 -> 650), the second finds the first already
                // moved, 900 away, and takes 101 (250 -> 601); from then on the pair is out of
                // contact and steps 250 a tick to exactly 1500 on the fifth tick, both of them,
                // as measured on client 16.402. The members exist on the death frame and are
                // inert there (spawner.RELEASE_TIMING), so the first slide step is the next
                // tick's. A slide in a phase of its own before or after the pass would have to
                // invent that 150 / 101 split, or lose it.
                //
                // The tick the step reaches the radius the slide ends, and the member's
                // ordinary update starts on the next tick. Taken after the knockback ladder (a
                // push landed on a sliding member runs first and the slide waits) and before
                // the holds, as the ladder is: whether a stun or a freeze stops the slide is
                // not measured, and a held member is not collidable, so it slides alone. A
                // fixed-distance knockback (knockback.DISPLACEMENT_LAW = fixed_distance,
                // `knock_ms`) makes the slide wait too: the Move phase slides the member and the
                // frozen hold below keeps it out of this pass, as the frame-planned arms'
                // `knocked` test does, so no tick carries both.
                #[cfg(not(clash_plant = "death_slide_never"))]
                let sliding = slide_r[i] > 0 && e.knock_ms[i] == 0;
                #[cfg(clash_plant = "death_slide_never")]
                let sliding = false; // PLANT (regression): the member walks from where it was born.
                if sliding {
                    let mut con = move16402::Contact { acc: (0, 0), count: 0, offset: offsets[i] };
                    move16402::separation_scan(&index, &bodies, i, &mut con, &mut scratch);
                    let c = slide_c[i];
                    let (m, done) = move16402::death_slide_step(
                        (bodies[i].x, bodies[i].y),
                        (c.x / K, c.y / K),
                        slide_r[i] / K,
                        &mut con,
                        deploying && !flying,
                        is_water,
                        arena.cols,
                        arena.rows,
                        attract[i],
                    );
                    // not a walk: the charge accumulator reads no step
                    walk_step[i] = 0;
                    push_applied[i] = Vec2::new(m.push.0, m.push.1);
                    push_neighbours[i] = m.push_count;
                    bodies[i].x = m.x;
                    bodies[i].y = m.y;
                    deltas[i] = Vec2::new(m.x * K, m.y * K).sub(e.pos[i]);
                    if done {
                        slide_r[i] = 0;
                        slide_c[i] = Vec2::default();
                    }
                    continue;
                }
                // formation.STAGGER_WAIT = client16402_untargetable_immovable: A MEMBER STILL
                // WAITING OUT ITS STAGGER DOES NOT MOVE -- no walk, no avoidance, no separation
                // scan (1083 of 1083 corpus frame pairs inside the wait are still). Its body stays
                // collidable, so a unit overlapping it is pushed off it as today. After the
                // pushback branch, so a knockback keeps today's behaviour, and a pull is applied
                // alone as the held branch below applies it: both are unmeasured on a waiting
                // member and keep today's reach.
                if calib.formation_stagger_wait == StaggerWait::Client16402 && e.stagger_ms[i] > 0 {
                    if attract[i] != (0, 0) {
                        let (nx, ny) = move16402::grid_move(bodies[i].x, bodies[i].y, attract[i].0, attract[i].1, deploying && !flying, &is_water, arena.cols, arena.rows);
                        bodies[i].x = nx;
                        bodies[i].y = ny;
                        deltas[i] = Vec2::new(nx * K, ny * K).sub(e.pos[i]);
                    }
                    continue;
                }
                // A FREEZE AND AN ATTACK ARE NOT THE SAME HOLD. A frozen or knocked
                // unit is out of the pass entirely. An attacking one does not WALK, and
                // under the measured arm the contact scans still reach it -- the same
                // scans this pass already runs for a unit that is merely in range.
                let phase_hold = calib.attack_holds(e.attack_phase[i]) || e.retarget_wait[i] > 0;
                let frozen = e.held(&self.cfg.cards.buffs, i) || e.knock_ms[i] > 0;
                if frozen || (phase_hold && calib.attacking_unit_movement == AttackingUnitMovement::Frozen) {
                    // a stun, a freeze or a knockback ends a dash where it stands (unmeasured:
                    // combat.DASH_ATTACK's open list); an attacking unit is not dashing
                    if frozen && dash_state[i] != DashState::None {
                        if dash_state[i] == DashState::Dashing {
                            dash_ended.push(i);
                        }
                        dash_state[i] = DashState::None;
                    }
                    if jumping[i] {
                        // a movement hold on a jumper: the leap is cancelled where it
                        // stands and the unit replans when the hold ends (UNVERIFIED: no
                        // capture has a stunned jumper; calibration movement.JUMP_WATER_HOP)
                        jumping[i] = false;
                        routes[i].clear();
                        goals[i] = None;
                        segs[i] = Vec2::default();
                    }
                    // status.ATTRACT_WHILE_HELD = pulled: THE HOLD STOPS THE WALK, NOT THE
                    // PULL. The pull is otherwise applied inside the move pass this branch
                    // skips, so a stunned, frozen or attack-held victim was never moved --
                    // while every walking victim was, which is why nothing noticed. Applied
                    // alone here: no walk, no avoidance, no separation (a held unit is not
                    // collidable), through the same grid clamp as any other displacement.
                    if calib.attract_while_held == AttractWhileHeld::Pulled && attract[i] != (0, 0) {
                        let (nx, ny) = move16402::grid_move(bodies[i].x, bodies[i].y, attract[i].0, attract[i].1, false, &is_water, arena.cols, arena.rows);
                        bodies[i].x = nx;
                        bodies[i].y = ny;
                        deltas[i] = Vec2::new(nx * K, ny * K).sub(e.pos[i]);
                    }
                    continue; // held: the freeze holds the whole unit (battle F sc5)
                }
                let card: &CardDef = self.cfg.cards.get(e.card[i]);
                let team = e.team[i];
                let goal_id = e.target[i].filter(|t| e.is_alive(*t)).or_else(|| target::default_tower(&ctx, i));
                let epoch = self.scratch.occluder_epoch[team as usize];
                let actor = (bodies[i].x, bodies[i].y);
                let node_centre = |p: Vec2| (p.x / K, p.y / K);
                // ---- THE DASH (combat.DASH_ATTACK = client_dash; card.rs `DashDef`), measured on client
                // 15.535.29 on the Bandit and the Mega Knight. Distances on the start-of-tick positions
                // (`e.pos`), which this pass has not moved.
                //
                // TRIGGER. A unit walking after a live target records it the first time it sees it, and
                // whether it was then nearer than DashMinRange edge to edge (walked into, no dash). From
                // the next tick, the first tick whose centre distance is at most DashMaxRange + the
                // target's radius starts the stand. Three Bandits and a Mega Knight put down inside that
                // distance moved on their first active frame + 17 and + 18: the trigger falls on the
                // frame after first sight, and on the first-sight frame itself the unit already stands (a
                // Mega Knight put down 4,805 from its target stood from its first active tick). A dash's
                // end forgets its target, so the next sight of it is a first sight: a unit whose dash
                // ended in melee sees it inside DashMinRange and walks in.
                //
                // STAND. Until the entry, DashCooldown / 50 - 1 ticks after the trigger (the Bandit's 800
                // and the Mega Knight's 900: 13 of 13 dashes and 2 of 2 jumps), the unit's walk runs at
                // speed 0: its route, contact scans and facing go on as on a stomp pause's tick, and it
                // does not step. A target that dies or is replaced, or an attack begun, ends the stand.
                //
                // ENTRY. The goal is fixed: the centre of the 500-cell holding the point (own radius +
                // target radius) short of the target's centre on the line (7 of 7 dashes head at it within
                // 0.1 degree; the Mega Knight lands on it). The unit stops being collidable, and with a
                // DashImmuneToDamageTime it discards damage from here (`dash_immune_until`).
                let mut dash_stand = false;
                if let Some(d) = card.dash.filter(|_| calib.dash_attack == DashAttack::ClientDash) {
                    let tk = calib.tick_ms.max(1);
                    let live = |t: Option<EntityId>| t.filter(|t| e.is_alive(*t));
                    let d2 = |ti: usize| {
                        let (dx, dy) = ((e.pos[ti].x - e.pos[i].x) as i64, (e.pos[ti].y - e.pos[i].y) as i64);
                        dx * dx + dy * dy
                    };
                    if dash_state[i] == DashState::None {
                        let walking = !deploying && e.attack_phase[i] == AttackPhase::Idle && e.retarget_wait[i] == 0;
                        if let (true, Some(t)) = (walking, live(e.target[i])) {
                            let ti = t.index as usize;
                            let near = (d.min_range + e.radius[i] + e.radius[ti]) as i64;
                            #[cfg(not(clash_plant = "dash_trigger_centre"))]
                            let reach = (d.max_range + e.radius[ti]) as i64;
                            #[cfg(clash_plant = "dash_trigger_centre")]
                            let reach = d.max_range as i64; // PLANT: DashMaxRange alone, without the target's radius.
                            let within = d2(ti) <= reach * reach;
                            if dash_target[i] != Some(t) {
                                dash_target[i] = Some(t);
                                dash_blocked[i] = d2(ti) < near * near;
                                // first sight already within the trigger distance: it stands this tick too
                                #[cfg(not(clash_plant = "dash_first_sight_walks"))]
                                {
                                    dash_stand = within && !dash_blocked[i];
                                }
                            } else if !dash_blocked[i] && within {
                                dash_state[i] = DashState::Standing;
                                dash_mark[i] = self.tick + ((d.cooldown_ms / tk).max(1) - 1) as u32;
                            }
                        }
                    } else if dash_state[i] == DashState::Standing
                        && (live(e.target[i]).is_none() || e.target[i] != dash_target[i] || e.attack_phase[i] != AttackPhase::Idle)
                    {
                        dash_state[i] = DashState::None;
                    }
                    if dash_state[i] == DashState::Standing && self.tick >= dash_mark[i] {
                        let ti = dash_target[i].map_or(i, |t| t.index as usize);
                        let (tx, ty) = (e.pos[ti].x / K, e.pos[ti].y / K);
                        let (dx, dy) = ((actor.0 - tx) as i64, (actor.1 - ty) as i64);
                        let rr = ((e.radius[i] + e.radius[ti]) / K) as i64;
                        let n = isqrt(dx * dx + dy * dy);
                        let (px, py) = if n == 0 { (tx, ty) } else { (tx + (dx * rr / n) as i32, ty + (dy * rr / n) as i32) };
                        let c = path16402::CELL;
                        dash_goal[i] = Vec2::new((px.div_euclid(c) * c + c / 2) * K, (py.div_euclid(c) * c + c / 2) * K);
                        dash_state[i] = DashState::Dashing;
                        dash_mark[i] = self.tick;
                        if d.immune_ms.is_some() {
                            dash_immune[i] = u32::MAX;
                        }
                        bodies[i].collidable = false;
                        routes[i].clear();
                        goals[i] = None;
                        segs[i] = Vec2::default();
                        // The Bandit's first dash tick is still (13 of 13, step 0 and the heading kept);
                        // the Mega Knight moves on its. Which column decides it is not separable with two
                        // cards: the Mega Knight's row has DashConstantTime, the Bandit's does not.
                        #[cfg(not(clash_plant = "dash_first_tick_moves"))]
                        let still = d.constant_time_ms.is_none();
                        #[cfg(clash_plant = "dash_first_tick_moves")]
                        let still = false; // PLANT: the Bandit moves on its entry tick.
                        if still {
                            walk_step[i] = 0;
                            continue;
                        }
                    }
                    // a first sight within the trigger stood above; a running stand stands too
                    dash_stand = dash_stand || dash_state[i] == DashState::Standing;
                    if dash_state[i] == DashState::Dashing {
                        // THE MOVE, straight at the goal's centre, never past it, with no scan.
                        //
                        // No DashConstantTime (the Bandit): two half-steps of JumpSpeed / 2 a tick, each
                        // followed by the Range test against the target's start-of-tick position; the first
                        // within ends the dash with the blow (its last move is about 250 in 9 of 13 dashes,
                        // about 500 in 4, ending at an edge of 540.8-738.9; the target loses DashDamage at
                        // the unit's level, 389 at 11, on that tick, 13 of 13). A goal reached out of range,
                        // or a target gone, ends it with no blow.
                        //
                        // A DashConstantTime (the Mega Knight): JumpSpeed a tick to the goal (about 250,
                        // resting on the goal's centre), the blow DashConstantTime / 50 ticks after the entry
                        // over DashRadius (537 on the Giant at level 11 in both jumps; the radius and the
                        // DashPushBack are open), and the end DASH_BLOW_TO_END_TICKS after the blow.
                        let goal = (dash_goal[i].x / K, dash_goal[i].y / K);
                        let step = |p: (i32, i32), len: i32| {
                            let mut v = (goal.0 - p.0, goal.1 - p.1);
                            if move16402::normalize_to(&mut v, len) <= len {
                                goal
                            } else {
                                (p.0 + v.0, p.1 + v.1)
                            }
                        };
                        let mut p = actor;
                        let mut ended = false;
                        match d.constant_time_ms {
                            None => match live(dash_target[i]) {
                                None => ended = true,
                                Some(t) => {
                                    let ti = t.index as usize;
                                    #[cfg(not(clash_plant = "dash_whole_steps"))]
                                    let (parts, len) = (2, d.speed / 2);
                                    #[cfg(clash_plant = "dash_whole_steps")]
                                    let (parts, len) = (1, d.speed); // PLANT: one whole step a tick, one Range test.
                                    for _ in 0..parts {
                                        p = step(p, len);
                                        let at = Vec2::new(p.0 * K, p.1 * K);
                                        if target::in_attack_range(calib, at, card.range, e.radius[i], e.pos[ti], e.radius[ti]) {
                                            dash_blows.push((i, Some(t), at));
                                            ended = true;
                                            break;
                                        }
                                        if p == goal {
                                            ended = true;
                                            break;
                                        }
                                    }
                                }
                            },
                            Some(ct) => {
                                p = step(p, d.speed);
                                let blow = dash_mark[i] + (ct / tk) as u32;
                                if self.tick == blow {
                                    dash_blows.push((i, live(dash_target[i]), Vec2::new(p.0 * K, p.1 * K)));
                                }
                                ended = self.tick >= blow + DASH_BLOW_TO_END_TICKS;
                            }
                        }
                        let mut v = (p.0 - actor.0, p.1 - actor.1);
                        if move16402::normalize_to(&mut v, 256) != 0 {
                            facing[i] = Vec2::new(v.0, v.1);
                            bodies[i].dir = v;
                        }
                        bodies[i].x = p.0;
                        bodies[i].y = p.1;
                        deltas[i] = Vec2::new(p.0 * K, p.1 * K).sub(e.pos[i]);
                        walk_step[i] = 0;
                        if ended {
                            dash_state[i] = DashState::None;
                            // the dash is spent: the target's next sight is a first sight
                            dash_target[i] = None;
                            dash_ended.push(i);
                            bodies[i].collidable = true;
                        }
                        continue;
                    }
                }
                if jumping[i] {
                    // ---- THE LEAP (state 5 in the captures; jump16402.rs): no path
                    // request, no avoidance scan but the offset decays, no separation
                    // scan, then move_towards the single node's centre at JumpSpeed with
                    // the facing set, and the landing test on the post-move position.
                    let Some(jump) = card.jump else {
                        debug_assert!(false, "jumping set on a card without a jump block");
                        jumping[i] = false;
                        continue;
                    };
                    let mut con = move16402::Contact { acc: (0, 0), count: 0, offset: offsets[i] };
                    move16402::decay_offset(&mut con); // unconditional
                    // JumpSpeed raw -- no buff, no charge multiplier
                    let speed = jump.speed;
                    let aim = match routes[i].last() {
                        Some(&p) => node_centre(p),
                        // the list was dropped under it (a pushback's back-step tick):
                        // it aims like a walker with no nodes
                        None => match goal_id {
                            Some(gid) => {
                                let gi = gid.index as usize;
                                if target::in_attack_range(calib, e.pos[i], card.range, e.radius[i], e.pos[gi], e.radius[gi]) {
                                    actor
                                } else {
                                    move16402::direct_aim(actor, (e.pos[gi].x / K, e.pos[gi].y / K), (card.range + e.radius[i]) / K)
                                }
                            }
                            None => actor,
                        },
                    };
                    walk_step[i] = 0;
                    jumped[i] = true;
                    if segs[i] == Vec2::default() && routes[i].last().is_some() {
                        let sg = move16402::segment_dir(actor.0, actor.1, aim);
                        segs[i] = Vec2::new(sg.0, sg.1);
                    }
                    // the ordinary step law at JumpSpeed, facing set
                    let m = move16402::move_towards(actor, aim.0, aim.1, speed, true, &mut con, (segs[i].x, segs[i].y), false, is_water, arena.cols, arena.rows);
                    offsets[i] = con.offset;
                    push_applied[i] = Vec2::new(m.push.0, m.push.1);
                    push_neighbours[i] = m.push_count;
                    if let Some(d) = m.dir {
                        facing[i] = Vec2::new(d.0, d.1);
                        bodies[i].dir = d;
                    }
                    bodies[i].x = m.x;
                    bodies[i].y = m.y;
                    bodies[i].offset = con.offset;
                    deltas[i] = Vec2::new(m.x * K, m.y * K).sub(e.pos[i]);
                    if jump16402::landed((m.x, m.y), aim, speed) {
                        // landed: the path request this triggers runs in the next Path
                        // phase here (one tick later than the capture's same-tick publish;
                        // the first walk step falls on the same tick either way)
                        jumping[i] = false;
                        routes[i].clear();
                        goals[i] = None;
                        segs[i] = Vec2::default();
                        bodies[i].collidable = true; // state 1 for the units updated after it
                    }
                    continue;
                }
                // ---- 2. the replan gate (walking units with a target only)
                //
                // A phase-held unit is ATTACKING by definition and skips this gate: it
                // asks for no path, and its route is cleared the same way the in-range
                // branch below clears one (SPEC 5.3).
                let mut attacking = phase_hold;
                let mut target_abs: Option<(i32, i32)> = None;
                let mut feasible = true;
                // The route is cleared, as on any transition to attacking (SPEC 5.3).
                // NOT clearing it was tried, on the argument that a held unit is already
                // attacking and a fresh replan after every push is what diverges. The
                // corpus says otherwise: 50.6 per cent within 250 against 50.5 with the
                // clear, so the replan is not the cost. The cost is the push itself
                // taking the unit out of range.
                if phase_hold {
                    routes[i].clear();
                    goals[i] = None;
                    segs[i] = Vec2::default();
                }
                if let (false, false, Some(gid)) = (deploying, phase_hold, goal_id) {
                    let gi = gid.index as usize;
                    if target::in_attack_range(calib, e.pos[i], card.range, e.radius[i], e.pos[gi], e.radius[gi]) {
                        // SPEC 5.3: the path is cleared on the transition to attacking
                        routes[i].clear();
                        goals[i] = None;
                        segs[i] = Vec2::default();
                        attacking = true;
                    } else {
                        // pathfinding.GOAL_TARGET_POSITION: the target centre the goal cell is
                        // chosen around. creation_order reads it as this pass holds it at the
                        // chaser's turn -- `bodies`, where every troop created before this one has
                        // already moved and every later one has not (a building does not move
                        // here) -- and start_of_tick reads the tick's starting position.
                        #[cfg(not(clash_plant = "goal_target_start_of_tick"))]
                        let as_held = calib.goal_target_position == GoalTargetPosition::CreationOrder;
                        #[cfg(clash_plant = "goal_target_start_of_tick")]
                        let as_held = false; // PLANT (regression): the new arm reads the start of the tick.
                        let target = if as_held { (bodies[gi].x, bodies[gi].y) } else { (e.pos[gi].x / K, e.pos[gi].y / K) };
                        target_abs = Some(target);
                        let reach = (card.range + e.radius[i]) / K;
                        // pathfinding.FLYER_GOAL_WATER: whether this chaser's goal choice ranks water
                        // below dry ground. A ground chaser always does.
                        #[cfg(not(clash_plant = "flyer_water_demoted"))]
                        let water_demoted = !(flying && calib.flyer_goal_water == FlyerGoalWater::NotDemoted);
                        #[cfg(clash_plant = "flyer_water_demoted")]
                        let water_demoted = true; // PLANT (regression): the new arm demotes water for a flyer too.
                        let goal_cell = path16402::choose_goal_cell(
                            &g.terrain,
                            &g.occ_cur,
                            actor,
                            target,
                            reach,
                            path2026::avoid_buildings16402(e.flying[gi]),
                            g.costs.building,
                            water_demoted,
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
                                        // a JumpEnabled mover prices water at WATER_COST
                                        let jumper = card.jump.is_some();
                                        let cost = |c: i32, r: i32| path16402::cell_cost_for(terrain, occ, c, r, jumper);
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
                    // THE STOMP CLOCK (movement.STOMP_PAUSE_SCHEDULE). Both arms run
                    // here and both counters advance, so the key is switchable on one
                    // tree; only the chosen one decides the pause. The clock's advance
                    // is the composed SPEED buff (`stomp_advance`): 50 unbuffed, 65
                    // under Rage, 0 while frozen -- and a frozen unit never reaches
                    // this line anyway (it is held above).
                    let k = kticks[i];
                    kticks[i] = k.saturating_add(1);
                    let (clock, hit) = path2026::stomp_clock_step(clocks[i], advances[i], card.stop_movement_after_ms, card.wait_ms);
                    clocks[i] = clock;
                    match calib.stomp_schedule {
                        StompSchedule::MsClock => hit,
                        StompSchedule::TickIndexMod => path2026::stomp_paused(calib.tick_ms, card.stop_movement_after_ms, card.wait_ms, k),
                    }
                } else {
                    false
                };
                // the native S through every buff in force -- `effective_speed` is
                // the one place a speed buff enters (the charge today), and it
                // returns `speed` itself for every card without a buff, so this arm
                // is unchanged for the whole contact corpus
                let native_speed = if paused || dash_stand { 0 } else { self.effective_speed(i) / K };
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
                // pathfinding.ZERO_STEP_WAYPOINT_TEST = run: a unit walking its route keeps the
                // waypoint bookkeeping on a zero-step tick (a stomp pause). An attacking or deploying
                // unit aims at itself and is left out: its reached test would pass every tick.
                #[cfg(not(clash_plant = "zero_step_waypoint_skipped"))]
                let zero_step_runs = calib.zero_step_waypoint_test == ZeroStepWaypointTest::Run;
                #[cfg(clash_plant = "zero_step_waypoint_skipped")]
                let zero_step_runs = false; // PLANT (regression): a paused walker tests nothing.
                let walks_route = routes[i].last().is_some() && !deploying && !attacking;
                let bookkeeping = speed > 0 || (zero_step_runs && walks_route);
                if segs[i] == Vec2::default() && routes[i].last().is_some() && bookkeeping {
                    // a new segment's direction is frozen from the position toward
                    // the last node when the segment starts
                    let s = move16402::segment_dir(actor.0, actor.1, node_centre(*routes[i].last().unwrap()));
                    segs[i] = Vec2::new(s.0, s.1);
                }
                let set_dir = speed > 0 || (aim != actor);
                let m = move16402::move_towards_extra(
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
                    attract[i],
                );
                offsets[i] = con.offset;
                push_applied[i] = Vec2::new(m.push.0, m.push.1);
                push_neighbours[i] = m.push_count;
                if let Some(d) = m.dir {
                    facing[i] = Vec2::new(d.0, d.1);
                    bodies[i].dir = d;
                }
                bodies[i].x = m.x;
                bodies[i].y = m.y;
                bodies[i].offset = con.offset;
                deltas[i] = Vec2::new(m.x * K, m.y * K).sub(e.pos[i]);
                // the reached test pops the last node and refreezes the segment
                if m.reached && !routes[i].is_empty() && bookkeeping {
                    routes[i].pop();
                    segs[i] = match routes[i].last() {
                        Some(&n) => {
                            let s = move16402::segment_dir(m.x, m.y, node_centre(n));
                            Vec2::new(s.0, s.1)
                        }
                        None => Vec2::default(),
                    };
                    // ---- THE HOP TRIGGER (jump16402.rs): a
                    // JumpEnabled card whose NEXT waypoint is now a water cell replaces
                    // the rest of its list with the one landing node, refreshes the
                    // segment from the post-move position and enters state 5
                    if let (Some(_), JumpWaterHop::Client16402) = (card.jump, calib.jump_water_hop) {
                        #[cfg(clash_plant = "jump_never_hops")]
                        let hop: Option<(i32, i32)> = None; // PLANT (regression): the water walked at cost 7, no leap.
                        #[cfg(not(clash_plant = "jump_never_hops"))]
                        let hop = {
                            let cells: Vec<(i32, i32)> = routes[i].iter().map(|&p| arena.subtile_to_half(p)).collect();
                            jump16402::landing_node(&cells, is_water)
                        };
                        if let Some((lc, lr)) = hop {
                            // the list becomes the one landing cell, the segment is
                            // refreshed from the post-move position, and the leap begins
                            routes[i] = vec![arena.half_to_subtile_center(lc, lr)];
                            let sg = move16402::segment_dir(m.x, m.y, jump16402::cell_centre(lc, lr));
                            segs[i] = Vec2::new(sg.0, sg.1);
                            jumping[i] = true;
                            bodies[i].collidable = false; // state 5 for the units updated after it
                        }
                    }
                }
            }
        }
        self.ents.route = routes;
        self.ents.route_goal = goals;
        self.ents.last_plan_tick = planned;
        self.ents.seg_dir = segs;
        self.ents.move_ticks = kticks;
        self.ents.stomp_clock = clocks;
        self.ents.facing = facing;
        self.ents.avoid_offset = offsets;
        self.ents.push_applied = push_applied;
        self.ents.push_neighbours = push_neighbours;
        self.ents.push_speed = push_speed;
        self.ents.push_active = push_active;
        self.ents.jumping = jumping;
        self.ents.death_slide_centre = slide_c;
        self.ents.death_slide_radius = slide_r;
        self.ents.dash_state = dash_state;
        self.ents.dash_mark = dash_mark;
        self.ents.dash_goal = dash_goal;
        self.ents.dash_target = dash_target;
        self.ents.dash_blocked = dash_blocked;
        self.ents.dash_immune_until = dash_immune;
        self.land_dash_blows(dash_blows);
        self.end_dashes(dash_ended);
        self.scratch.deltas = deltas;
        self.scratch.walk_step = walk_step;
        self.scratch.pushed = pushed;
        self.scratch.jumped = jumped;
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
        let base = self.buffed_speed(i);
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

    /// ENTITY `i`'s SPEED THROUGH ITS BUFFS, before the charge multiplier
    /// (movement.BUFF_SPEED_COMPOSITION: the composition runs on the Speed column
    /// and the ChargeSpeedMultiplier applies to its result). The composition works
    /// on the NATIVE speed
    /// S, not on the stored subtiles-per-tick figure, because the truncation is the
    /// whole point: a raged Ice Golem walks at tdiv(130 x 52, 100) = 67, and
    /// composing the stored 52 x SPEED_TO_SUBTILES_PER_TICK would round somewhere
    /// else. An unbuffed unit gets its own speed back, exactly.
    fn buffed_speed(&self, i: usize) -> i32 {
        let base = self.ents.speed[i];
        if self.ents.buff_slots(i).iter().all(|s| s.is_empty()) {
            return base;
        }
        let spt = self.cfg.calib.speed_to_subtiles_per_tick.max(1);
        debug_assert_eq!(base % spt, 0, "a stored speed is S x SPEED_TO_SUBTILES_PER_TICK exactly");
        let native = base / spt;
        let table = &self.cfg.cards.buffs;
        let out = match self.cfg.calib.buff_speed_composition {
            BuffComposition::StrongestUpAndDown => self.ents.buffed(table, i, Sel::Speed, native),
            // movement.BUFF_SPEED_RULE's single-buff reading: floor(S x m / 100) for
            // the FIRST non-zero multiplier on the unit, with the column's own
            // convention (a positive is the absolute percent, Rage 130; a negative is
            // the delta, IceWizardSlowDown -30 = 70 %). Identical to the composition
            // while a unit carries at most one buff, which is every corpus frame.
            BuffComposition::SingleMultiplierFloor => match self.ents.buffs_of(table, i).map(|b| b.speed_pct).find(|m| *m != 0) {
                Some(m) => native * if m > 0 { m } else { (100 + m).max(0) } / 100,
                None => native,
            },
        };
        out * spt
    }

    /// THE STOMP CLOCK'S ADVANCE for entity `i` on a tick it walks, in ms
    /// (movement.STOMP_PAUSE_SCHEDULE = ms_clock): `tdiv(compose(Speed, 100), 2)`
    /// -- 50 unbuffed, 65 under
    /// Rage, 0 while frozen. The buff scales the CLOCK, not the period table, which
    /// is why a raged stomp card's pause group drifts and can run four ticks long.
    fn stomp_advance(&self, i: usize) -> i32 {
        let dt = self.cfg.calib.tick_ms;
        if self.ents.buff_slots(i).iter().all(|s| s.is_empty()) {
            return dt;
        }
        self.ents.buffed(&self.cfg.cards.buffs, i, Sel::Speed, 2 * dt) / 2
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
        // the ladder's zero-speed and back-step ticks: progress AND charge cleared
        let mut ladder_reset: Vec<usize> = Vec::new();
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
                tick: self.tick,
                doomed: &[],
            };
            for i in 0..cap {
                if !e.alive[i] || e.kind[i] != EntityKind::Troop {
                    continue;
                }
                let Some(ch) = self.cfg.cards.get(e.card[i]).charge else { continue };
                if self.scratch.pushed.get(i).copied().unwrap_or(false) {
                    // ---- A PUSHBACK TICK: the charge tail runs on the ladder's
                    // requested step (`walk_step`, min(speed, dist, 250)). A step > 0 on
                    // a unit in state 1 adds tdiv(step x 1000, ChargeRange) to a run-up
                    // below 10000, and reaching 10000 fires the charge; a charged unit
                    // stays charged. Otherwise -- the ZERO-speed tick and the BACK-STEP
                    // tick (step 0 / -25), or a unit not in state 1 -- the run-up AND the
                    // charge are cleared, WHATEVER the progress was (a unit in state 5
                    // or 8 is exempt). MEASURED: the victim's state is 1 through the
                    // ladder (the Giant of capture 20260918-122757.b1, ticks
                    // 1216..1223; the Bomber and the Knight of capture
                    // 20260920-081819-B, attacking when hit, state 1 from the first
                    // ladder tick). The engine's state 1 stand-in for a pushed unit: not
                    // deploying (state 4) and not stunned (RESET_ON_STUN already zeroed
                    // it). charge.RESET_ON_KNOCKBACK = true is this tail; `false` skips
                    // it on ladder ticks (the run-up and the charge held across the
                    // ladder, the foil).
                    if !c.charge_reset_on_knockback {
                        continue;
                    }
                    if e.jumping[i] {
                        // a ladder running in place of a leap: the unit is still state 5,
                        // and the tail neither adds (state != 1) nor resets (states 5 and
                        // 8 are exempt) -- no capture exercises this branch
                        continue;
                    }
                    let l = self.scratch.walk_step.get(i).copied().unwrap_or(0);
                    let state1 = e.deploy_ms[i] == 0 && e.stun_ms[i] == 0;
                    if l > 0 && state1 {
                        if !e.charged[i] {
                            // the tail's own unit under the client16402 accumulator; the
                            // displacement foils count the ladder's requested step in
                            // THEIR unit (subtiles of walk, or a tick of moving), so a
                            // ladder tick reads to them as a walk of that length
                            let gain = match c.charge_accumulator {
                                ChargeAccumulator::Client16402ProgressPermille => move16402::tdiv(l.saturating_mul(1000), ch.range_raw.max(1)),
                                ChargeAccumulator::TimeMoving => c.tick_ms,
                                ChargeAccumulator::WalkDeltaLength | ChargeAccumulator::WalkDeltaTowardTarget | ChargeAccumulator::NetMoveLength => l.saturating_mul(K),
                            };
                            due.push((i, gain, self.charge_need(i, ch)));
                        }
                    } else {
                        ladder_reset.push(i);
                    }
                    continue;
                }
                if e.charged[i] {
                    continue;
                }
                if client16402 && self.scratch.jumped.get(i).copied().unwrap_or(false) {
                    // a river-jump tick: the charge tail neither adds (not a walking
                    // tick) nor resets -- the run-up is held across the leap (the
                    // Prince's hop replays under this rule)
                    continue;
                }
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
        for i in ladder_reset {
            self.ents.charged[i] = false;
            self.ents.charge_progress[i] = 0;
        }
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
    /// and the game's avoidance term is unmeasured (it sets `avoidance_offset` to
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
        let mut clocks = std::mem::take(&mut self.ents.stomp_clock);
        // This tick's stomp-clock advance per entity, taken before `ents` is
        // borrowed: it reads the buff list and the calibration, not the walk.
        let advances: Vec<i32> = (0..cap).map(|i| self.stomp_advance(i)).collect();
        let mut slide_done: Vec<usize> = Vec::new();
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
                tick: self.tick,
                doomed: &[],
            };
            for i in 0..cap {
                if !e.alive[i] || e.kind[i] != EntityKind::Troop {
                    continue;
                }
                // spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide: the death-spawn slide
                // replaces the walk (`frame_planned_slide`).
                if e.death_sliding(i) && !e.knocked(i) {
                    let (d, done) = frame_planned_slide(e, i);
                    deltas[i] = d;
                    if done {
                        slide_done.push(i);
                    }
                    continue;
                }
                if e.speed[i] <= 0 {
                    continue;
                }
                if e.deploy_ms[i] > 0 || e.held(&self.cfg.cards.buffs, i) || e.knocked(i) || calib.attack_holds(e.attack_phase[i]) || e.retarget_wait[i] > 0 {
                    continue;
                }
                let card: &CardDef = self.cfg.cards.get(e.card[i]);
                let goal_id = match e.target[i].filter(|t| e.is_alive(*t)).or_else(|| target::default_tower(&ctx, i)) {
                    Some(g) => g,
                    None => continue,
                };
                let gi = goal_id.index as usize;
                if target::in_attack_range(calib, e.pos[i], card.range, e.radius[i], e.pos[gi], e.radius[gi]) {
                    // SPEC 5.3: the path is cleared on the transition to attacking,
                    // without reaching the goal node. Over 46 304 recorded ticks not
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
                    target_flying: e.flying[gi],
                    jumper: card.jump.is_some(),
                    ignore: if e.kind[gi].is_building() { Some(goal_id) } else { None },
                };
                if req.flying {
                    // Air units do not use the grid at all: they fly to the target.
                    // UNMEASURED -- no flying unit appears in the 15.535.29 corpus.
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
                // The game never shows this case because its own attack predicate
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
                let (clock, hit) = path2026::stomp_clock_step(clocks[i], advances[i], card.stop_movement_after_ms, card.wait_ms);
                clocks[i] = clock;
                let paused = match calib.stomp_schedule {
                    StompSchedule::MsClock => hit,
                    StompSchedule::TickIndexMod => path2026::stomp_paused(calib.tick_ms, card.stop_movement_after_ms, card.wait_ms, k),
                };
                if paused {
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
        self.ents.stomp_clock = clocks;
        self.scratch.deltas = deltas;
        self.end_death_slides(slide_done);
    }

    /// The death-spawn slides that reached their radius this Path phase end
    /// (spawner.DEATH_SPAWN_PUSHBACK; the frame-planned arms): the member's ordinary update
    /// starts on the next tick.
    fn end_death_slides(&mut self, done: Vec<usize>) {
        for i in done {
            self.ents.death_slide_radius[i] = 0;
            self.ents.death_slide_centre[i] = Vec2::default();
        }
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
        let mut slide_done: Vec<usize> = Vec::new();
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
                tick: self.tick,
                doomed: &[],
            };
            // None = the measured "no periodic replan" (calibration
            // pathfinding.REPATH_INTERVAL_TICKS). For these pre-2026 models that
            // leaves the route-empty and goal-moved triggers below, which is
            // strictly closer to the 15.535.29 measurements than the old folklore 10-tick cadence.
            let repath = calib.repath_interval_ticks.map(|r| r.max(1) as u32);
            for i in 0..cap {
                if !e.alive[i] || e.kind[i] != EntityKind::Troop {
                    continue;
                }
                // spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide: the death-spawn slide
                // replaces the walk (`frame_planned_slide`).
                if e.death_sliding(i) && !e.knocked(i) {
                    let (d, done) = frame_planned_slide(e, i);
                    deltas[i] = d;
                    if done {
                        slide_done.push(i);
                    }
                    continue;
                }
                if e.speed[i] <= 0 {
                    continue;
                }
                if e.deploy_ms[i] > 0 || e.held(&self.cfg.cards.buffs, i) || e.knocked(i) || calib.attack_holds(e.attack_phase[i]) || e.retarget_wait[i] > 0 {
                    continue;
                }
                let card: &CardDef = self.cfg.cards.get(e.card[i]);
                let goal_id = match e.target[i].filter(|t| e.is_alive(*t)).or_else(|| target::default_tower(&ctx, i)) {
                    Some(g) => g,
                    None => continue,
                };
                let gi = goal_id.index as usize;
                if target::in_attack_range(calib, e.pos[i], card.range, e.radius[i], e.pos[gi], e.radius[gi]) {
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
                    target_flying: e.flying[gi],
                    jumper: card.jump.is_some(),
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
        self.end_death_slides(slide_done);
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
        let client16402 = self.arm16402();
        if !client16402 {
            // under the 16.402 locomotion the ladder ran inside phase_path16402, in
            // the move pass; here it runs where the slides do
            self.step_pushback_ladders();
        }
        // The charge accumulator's "before" (charge_pass): captured AFTER the slides,
        // so a knockback displacement never counts as a walk.
        self.scratch.pre.clear();
        self.scratch.pre.extend_from_slice(&self.ents.pos);
        let arena = &self.cfg.arena;
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
        if self.tick_order() == TickOrder::Client16402 {
            // the deploy countdown after the move pass, as measured
            self.deploy_countdown();
        }
        if self.emission_timing() == SpawnerEmission::MovePhaseImmediate {
            // The spawner pass runs here, right after the countdown, for the first
            // time on the tick the deploy timer reaches zero, and creates its units in
            // this tick (spawner.EMISSION_TIMING, measured on 44 live Tombstones).
            self.spawner_pass();
        }
    }

    fn phase_attack(&mut self) {
        self.phase_attack_for(None);
    }

    /// The Attack phase, for every unit or for a first update's fresh units alone.
    fn phase_attack_for(&mut self, only: Option<&[usize]>) {
        for i in 0..self.ents.capacity() {
            if !self.ents.alive[i] || self.cfg.cards.get(self.ents.card[i]).hit_speed_ms <= 0 || !only.map_or(true, |o| o.contains(&i)) {
                continue;
            }
            #[cfg(clash_plant = "inline_damage")]
            if self.ents.hp[i] <= 0 {
                // PLANT: units killed earlier in this same loop do not swing.
                continue;
            }
            let e = &self.ents;
            // stunned, mid-slide or mid-ladder: the attack timers freeze (a landed
            // push already reset a running windup in `apply_effects`)
            // ... and a unit in its post-kill retarget wait (combat.POST_KILL_RETARGET_WAIT),
            // and a death-spawn member still sliding out (spawner.DEATH_SPAWN_PUSHBACK), which
            // neither walks nor attacks until the slide ends. That last term is belt and
            // braces, not a gate: a sliding member is born idle with no target and target.rs
            // `decide` gives it none until the slide ends, so both attack cycles already leave
            // it idle, and removing the term turns no test red.
            let held = e.held(&self.cfg.cards.buffs, i) || e.knocked(i) || e.retarget_wait[i] > 0 || e.death_sliding(i);
            // Under ground or coming up: no attack (target.rs `decide` already gave it
            // no target; this keeps a windup from advancing under the
            // time_since_last_shot arm, where a building can go under mid-swing).
            // Mid-leap (state 5): the attack side of a leap is unmeasured
            // (jump16402.rs); the engine holds the swing and keeps the unit targetable.
            // Mid-dash (combat.DASH_ATTACK): the swing holds; the dash ends in a fresh cycle.
            let can_act = e.deploy_ms[i] == 0
                && !held
                && !e.jumping[i]
                && e.dash_state[i] != DashState::Dashing
                && e.hide[i] == HideState::Up
                && (e.kind[i] != EntityKind::KingTower || self.king_active[e.team[i] as usize]);
            let step = combat::attack_step(e, &self.cfg.cards, &self.cfg.calib, i, can_act);
            self.ents.attack_phase[i] = step.phase;
            // movement.ATTACK_FACING = toward_target: a unit in its attack state faces its target on
            // every tick, the move law's integer normalize (length 256) of target - self in native units.
            // Attack runs before Move, so these are the start-of-tick positions; the move pass writes
            // the facing of walking units only, so this one stands for the tick.
            #[cfg(not(clash_plant = "attack_facing_kept"))]
            let face = self.cfg.calib.attack_facing == AttackFacing::TowardTarget;
            #[cfg(clash_plant = "attack_facing_kept")]
            let face = false; // PLANT (regression): the new arm keeps the walking heading.
            if face && step.phase != AttackPhase::Idle {
                if let Some(t) = self.ents.target[i].filter(|t| self.ents.is_alive(*t)) {
                    use crate::fixed::SUBTILE_PER_MILLITILE as K;
                    let (me, it) = (self.ents.pos[i], self.ents.pos[t.index as usize]);
                    let mut v = (it.x / K - me.x / K, it.y / K - me.y / K);
                    if crate::move16402::normalize_to(&mut v, 256) != 0 {
                        self.ents.facing[i] = Vec2::new(v.0, v.1);
                    }
                }
            }
            self.ents.attack_ms[i] = step.ms;
            self.ents.attack_load_ms[i] = step.load_ms;
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
                let locks = self.cfg.calib.locks_target(self.cfg.cards.get(self.ents.card[i]));
                self.ents.target_locked[i] = locks && step.phase == AttackPhase::Windup;
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
                    &mut self.effects,
                    &mut self.projectiles,
                    &mut self.scratch.nb,
                );
                // targeting.DOOMED_TARGET_DROP = projectile_attackers: a projectile attacker has now
                // launched at its target, so it keeps that target even once it is doomed.
                if self.cfg.calib.doomed_target_drop.drops() && self.cfg.cards.get(self.ents.card[i]).projectile.is_some() {
                    self.ents.fired_at[i] = Some(t);
                }
                // targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED = "projectile_attackers_only": a projectile
                // attacker that launched at its target from beyond its reach re-evaluates on the next tick
                // (measured on client 15.535.29: three let-go frames right after such a launch). Start-of-tick
                // positions: Attack runs before Move.
                let card = self.cfg.cards.get(self.ents.card[i]);
                if self.cfg.calib.preserve_target_scope == PreserveTargetScope::ProjectileAttackersOnly && card.projectile.is_some() {
                    let ti = t.index as usize;
                    #[cfg(not(clash_plant = "launch_beyond_ignored"))]
                    let beyond = self.ents.is_alive(t)
                        && !target::in_attack_range(&self.cfg.calib, self.ents.pos[i], card.range, self.ents.radius[i], self.ents.pos[ti], self.ents.radius[ti]);
                    #[cfg(clash_plant = "launch_beyond_ignored")]
                    let beyond = false; // PLANT (regression): a launch beyond reach does not end the hold.
                    self.ents.launched_beyond[i] = beyond;
                }
                // combat.REFLECT_ATTACK = client_reflect_stun: a melee hit that landed on a unit
                // carrying a reflect is answered here, in the attacker's own pass, right after
                // `fire` wrote it (`reflect_melee_hit` says why here and not in Resolve).
                #[cfg(not(clash_plant = "reflect_ignores_key"))]
                let reflect_on = self.cfg.calib.reflect_attack == ReflectAttack::ClientReflectStun;
                #[cfg(clash_plant = "reflect_ignores_key")]
                let reflect_on = true; // PLANT (regression): the reflect runs whatever the key says.
                if reflect_on {
                    self.reflect_melee_hit(i, t);
                }
                // CHARGE (calibration charge.RESET_ON_ATTACK): the LANDED hit -- this
                // completed windup, not entering range and not a cancelled swing --
                // consumes the charge (`fire` read `charged` for its damage just above).
                // `charged` and `charge_progress` are two different things: the stop
                // rule in `charge_pass` touches only the latter.
                // The charged SNAP (charge.CHARGED_HIT_TIMING = first_attack_pass_no_windup)
                // consumes the charge itself, so the hit it raised consumes the charge
                // whatever RESET_ON_ATTACK says.
                if (self.cfg.calib.charge_reset_on_attack || step.charge_snapped) && self.ents.charged[i] {
                    self.ents.charged[i] = false;
                    self.ents.charge_progress[i] = 0;
                }
                // KAMIKAZE (calibration combat.KAMIKAZE_DEATH = at_fire): a Kamikaze
                // unit dies on the tick its hit lands (the live Battle Ram is gone on
                // the frame its one hit lands, its Barbarians deploying), so a Battle
                // Ram lands its one hit and breaks into its Barbarians. Written as a
                // self-hit into the buffer: the death resolves with every other hit of
                // the tick and the death spawn follows in Reap.
                if self.cfg.cards.get(self.ents.card[i]).kamikaze {
                    let me = self.ents.id_of(i);
                    let all = self.ents.hp[i].max(0) + self.ents.shield[i].max(0);
                    self.dmg.hits.push(Hit { target: me, amount: all, ignores_hide: false });
                }
                #[cfg(clash_plant = "inline_damage")]
                for h in self.dmg.hits.drain(..) {
                    // PLANT (regression): apply each hit inline at the swing instead of
                    // through the tick's hit buffer, which makes damage order-dependent
                    // and the state hash depend on entity order.
                    if self.ents.is_alive(h.target) {
                        self.ents.hp[h.target.index as usize] -= h.amount;
                    }
                }
            }
        }
    }

    /// THE REFLECT (calibration combat.REFLECT_ATTACK = client_reflect_stun; card.rs
    /// `ReflectDef`), for attacker `a`'s hit on `target`, which `fire` has just written.
    ///
    /// WHO ANSWERS. Every unit the hit landed on whose card carries a reflect -- the one target,
    /// or the victim list `splash` leaves in `scratch.nb` for a splash hitter -- answers the
    /// ATTACKER when the attacker's EDGE is within the reflect's radius of its centre (the
    /// measured Knight stood 2,008 centre to centre: ReflectedAttackRadius 2000 plus a collision
    /// radius). A MELEE hit only: a projectile is answered neither at its launch nor at its
    /// arrival. The measured shots, the princess towers', came from beyond the reach; a ranged
    /// attacker or a tower inside it is the ledger entry's open item, and ReflectAttackCrownTowerDamage
    /// is carried for it and read by nothing. The reach is read where the hit LANDS, and a swing
    /// under way lands on a target that has walked out of range (combat.rs, the hit-started
    /// test), so a hit can land from beyond the reach and go unanswered.
    ///
    /// WHAT THE ANSWER IS. ReflectedAttackDamage at the reflecting unit's level, into the damage
    /// buffer like any hit, so it resolves in this tick's Resolve with the hit it answers (the
    /// Knight's -192 falls on the frame of its -202). And ReflectedAttackBuff, landed HERE AND
    /// NOW through the same slot and hold `apply_effects` uses (`land_buff`, `land_stun`), not
    /// buffered for Resolve.
    ///
    /// WHY NOW AND NOT IN RESOLVE: THE PERIOD 33. The measured Knight's attack progress holds for
    /// 9 ticks after its hit and then runs on from where it stood, so its cycle of 24 ticks
    /// becomes 33. Landed in the attack pass of the hit tick N, the 500 ms (10 ticks) is first
    /// counted down later in tick N itself (in Resolve under status.BUFF_EXPIRY_TICK_ALIGNMENT =
    /// ceil_from_next_tick, which ships; at the next Status under one_tick_short) and is gone
    /// after its tenth decrement, so the attack passes of N+1..N+9 are held and N+10's runs: 9
    /// under either alignment. Through the effect buffer the buff would land in Resolve AFTER
    /// that tick's decrement and the shipped alignment would hold N+1..N+10, a period of 34,
    /// which the measurement refutes (plant `reflect_stun_buffered`). The attacker's own pass for
    /// tick N is over by now, and no other unit's attack step reads its hold, so landing it here
    /// changes nothing else this pass computes.
    fn reflect_melee_hit(&mut self, a: usize, target: EntityId) {
        let card = self.cfg.cards.get(self.ents.card[a]);
        // PLANTS (regression): reflect_answers_shots drops this return alone, so a shot is
        // answered at its launch from inside the reach; reflect_every_attack drops it and the
        // reach test below together.
        #[cfg(not(clash_plant = "reflect_answers_shots"))]
        #[cfg(not(clash_plant = "reflect_every_attack"))]
        if card.projectile.is_some() {
            return;
        }
        let single = [target.index];
        let victims: &[u32] = if card.projectile.is_none() && card.area_damage_radius > 0 { self.scratch.nb.as_slice() } else { single.as_slice() };
        let me = self.ents.id_of(a);
        for &v in victims {
            let v = v as usize;
            let e = &self.ents;
            if !e.alive[v] || e.hide[v] == HideState::Hidden {
                continue;
            }
            #[cfg(not(clash_plant = "reflect_ignores_victim_card"))]
            let reflect = self.cfg.cards.get(e.card[v]).reflect;
            // PLANT (regression): every melee hit is answered as the Electro Giant's would be,
            // whatever the victim's card carries.
            #[cfg(clash_plant = "reflect_ignores_victim_card")]
            let reflect = self.cfg.cards.cards.iter().find_map(|c| c.reflect);
            let Some(r) = reflect else { continue };
            // PLANTS (regression): reflect_ignores_reach drops this test alone, so a hit from any
            // distance is answered; reflect_every_attack drops it and the shot return above.
            #[cfg(not(clash_plant = "reflect_ignores_reach"))]
            #[cfg(not(clash_plant = "reflect_every_attack"))]
            if !crate::fixed::in_range_edge(e.pos[v], e.pos[a], r.radius, e.radius[a]) {
                continue;
            }
            #[cfg(not(clash_plant = "reflect_damage_unscaled"))]
            let amount = self.cfg.cards.scaled(e.card[v], e.level[v], r.damage).expect("level validated at spawn");
            #[cfg(clash_plant = "reflect_damage_unscaled")]
            let amount = r.damage; // PLANT (regression): the level-1 column, 75 where the Knight takes 192.
            self.dmg.hits.push(Hit { target: me, amount, ignores_hide: false });
            let Some(b) = r.buff else { continue };
            #[cfg(not(clash_plant = "reflect_stun_buffered"))]
            if land_buff(&mut self.ents, &self.cfg.cards.buffs, &self.cfg.calib, a, b.buff, b.time_ms, 0) {
                land_stun(&mut self.ents, &self.cfg.calib, a, b.time_ms);
            }
            // PLANT (regression): the stun through the effect buffer, landed in Resolve.
            #[cfg(clash_plant = "reflect_stun_buffered")]
            self.effects.buffs.push(crate::status::BuffHit { target: me, buff: b.buff, time_ms: b.time_ms, pulse_amount: 0 });
        }
    }

    fn phase_projectile(&mut self) {
        combat::step_projectiles(&self.ents, &self.hash, &self.cfg.calib, &mut self.projectiles, &mut self.dmg, &mut self.effects, &mut self.scratch.nb);
        // No early return on an empty spell list: stepping no spell is a no-op, and what
        // the phase hands on (spell.rs `SpellOut`) need not come from a spell.
        let mut out = spell::SpellOut::default();
        {
            let ctx = spell::SpellCtx { ents: &self.ents, hash: &self.hash, cards: &self.cfg.cards, calib: &self.cfg.calib };
            spell::step_spells(&ctx, &mut self.spells, &mut self.dmg, &mut self.effects, &mut out, &mut self.scratch.nb);
        }
        // DRAINED HERE, in `SpellOut`'s documented order, and nothing is kept past the
        // phase. Destructured without `..`, so a field added to the bundle does not
        // compile until it has a consumer below.
        let spell::SpellOut { released, mut born, mut launched, areas, clones, fuse_ends } = out;
        for r in released {
            // spawner.RELEASE_TIMING: on this frame and inert on it, or queued for the next
            // Spawn phase under the earlier convention.
            let unit = self.cfg.cards.get(r.unit);
            let points = match self.cfg.calib.projectile_spawn_formation {
                ProjectileSpawnFormation::EngineGrid => self.formation_points(r.team, r.count, unit.collision_radius, unit.is_flying(), r.pos),
                ProjectileSpawnFormation::CountRingTight => self.release_ring_points(r.team, r.count, r.unit, r.pos),
            };
            for p in points {
                self.release(PendingSpawn { team: r.team, card: r.unit, level: r.level, pos: p, deploy_ms: r.deploy_ms, owner: None, stagger_ms: 0, slide_centre: Vec2::default(), slide_radius: 0, acquire_delay: false, first_update: false, facing: None });
            }
        }
        // Spell objects made by spell objects, appended after every spell has stepped,
        // so they first act next tick.
        self.spells.append(&mut born);
        // A landing object's area effect: cast at its point and appended after them, so
        // it too first applies next tick (as a death's area effect does, `phase_reap`).
        for a in areas {
            let v = spell::cast(&self.cfg.cards, &self.cfg.calib, &self.cfg.arena, a.team, a.card, a.level, a.pos).expect("released area level validated at deploy");
            self.spells.extend(v);
        }
        // Projectiles fired by spell objects: this tick's `step_projectiles` has run, so
        // they first step next tick.
        self.projectiles.append(&mut launched);
        // Nothing writes these two yet, and the engine has no law yet for where a
        // container's units stand or what a copy inherits. An order reaching here would
        // be lost in silence, so it stops the battle instead.
        assert!(fuse_ends.is_empty(), "a fuse end reached phase_projectile, which cannot release its death spawn yet");
        assert!(clones.is_empty(), "a clone order reached phase_projectile, which cannot hand it to Reap yet");
    }

    /// THE KNOCKBACK LADDER UNDER THE FRAME-PLANNED PATH ARMS (knockback.DISPLACEMENT_LAW
    /// = client16402 with pathfinding.PATH_SEARCH = trace_fitted_astar or a
    /// pre-2026 model): the same countdown and step as `phase_path16402` runs inside
    /// the sequential move update (move16402.rs `pushback_step`), with the
    /// contact that those arms do not have -- no separation scan, no avoidance
    /// rotation (their offset is always 0) -- and the position through spell.rs
    /// `settle` instead of the grid write, so their own invariants (no water, no
    /// footprint) keep holding. The path drop at the ladder's end is the same.
    fn step_pushback_ladders(&mut self) {
        use crate::fixed::SUBTILE_PER_MILLITILE as K;
        let arena = &self.cfg.arena;
        let mut moved = false;
        let cap = self.ents.capacity();
        // the charge tail's inputs (`charge_pass`): which units took a ladder tick
        // and the requested step of each -- the frame-planned arms' counterpart of
        // what phase_path16402 records
        let pushed = &mut self.scratch.pushed;
        pushed.clear();
        pushed.resize(cap, false);
        let walk_step = &mut self.scratch.walk_step;
        walk_step.clear();
        walk_step.resize(cap, 0);
        for i in 0..cap {
            if !self.ents.alive[i] || !self.ents.push_active[i] {
                continue;
            }
            let e = &mut self.ents;
            let mut con = move16402::Contact::default();
            let mut rem = e.push_speed[i];
            let old = e.pos[i];
            let u = (old.x / K, old.y / K);
            let tgt = (e.push_target[i].x, e.push_target[i].y);
            let d_pre = move16402::distance(u.0, u.1, tgt.0, tgt.1).max(1);
            let m = move16402::pushback_step(u, tgt, &mut rem, &mut con, (0, 0), false, |_, _| false, arena.cols, arena.rows);
            pushed[i] = true;
            walk_step[i] = rem.min(d_pre).min(250);
            let desired = Vec2::new(m.x * K, m.y * K);
            e.pos[i] = spell::settle(arena, &self.scratch.obstacles[0], e.team[i], e.radius[i], e.flying[i], old, desired);
            e.push_speed[i] = rem;
            e.push_active[i] = rem >= 0;
            if rem < 0 {
                e.route[i].clear();
                e.route_goal[i] = None;
                e.seg_dir[i] = Vec2::default();
            }
            moved = true;
        }
        if moved {
            self.hash.rebuild(&self.ents);
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
    /// the damage pass. The stun merge is commutative (max), and so is the
    /// fixed_distance knockback sum. THE LADDER IS NOT: a push is refused while the
    /// unit's ladder is active, so the first `Knock::Push` of a unit in the buffer
    /// lands and the rest are refused (knockback.STACKING = first_wins_while_active)
    /// -- buffer order is spell order, which is cast order.
    fn apply_effects(&mut self) {
        let fx = std::mem::take(&mut self.effects);
        if fx.knocks.is_empty() && fx.stuns.is_empty() && fx.buffs.is_empty() {
            self.effects = fx;
            return;
        }
        let c = self.cfg.calib.clone();
        let cap = self.ents.capacity();
        // A HIDDEN building is out of reach of every effect too (knockback never moved
        // a building; a stun on it is a no-op).
        let survivor = |e: &Entities, id: EntityId| e.is_alive(id) && e.hp[id.index as usize] > 0 && e.hide[id.index as usize] != HideState::Hidden;
        // BUFFS (status.rs), before the stun merge, because a FULL-STOP buff feeds
        // it. ONE SLOT PER BUFF ROW (status.BUFF_STACKING): an application either
        // refreshes the slot that already holds that row or takes a free one, and
        // EnableStacking is a NAMED GAP -- stacking needs the buff's SOURCE as part of
        // its identity, and without that a Poison cloud would stack with ITSELF every
        // time its area re-applies (LifeDuration / HitSpeed = 32 copies of one Poison).
        // A unit already carrying MAX_BUFFS_PER_ENTITY distinct rows drops the new one;
        // no shipped combination of buffs reaches four on one unit, so the cap is never met.
        let mut stun_new = vec![0i32; cap];
        for b in &fx.buffs {
            if !survivor(&self.ents, b.target) {
                continue;
            }
            let i = b.target.index as usize;
            // The slot and the full-stop test are `land_buff`'s, shared with the reflect's
            // stun (`reflect_melee_hit`), which lands the same way in the attack pass.
            if land_buff(&mut self.ents, &self.cfg.cards.buffs, &c, i, b.buff, b.time_ms, b.pulse_amount) {
                stun_new[i] = stun_new[i].max(b.time_ms);
            }
        }
        // Stuns: max per target. Replace vs refresh decides only what the EXISTING timer
        // contributes; several new stuns in one tick always merge by max.
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
            land_stun(&mut self.ents, &c, i, ms);
        }
        // Knockbacks. fixed_distance: sum per target, then one move per unit.
        // client16402: arm the ladder on the first push per unit (the
        // `Some(None)` of `sum`: landed, no displacement to apply here).
        let mut sum = vec![None::<Option<Vec2>>; cap];
        for k in &fx.knocks {
            let id = k.id();
            if !survivor(&self.ents, id) {
                continue;
            }
            let i = id.index as usize;
            match *k {
                spell::Knock::Displacement(_, d) => {
                    let s = sum[i].get_or_insert(Some(Vec2::default()));
                    let s = s.get_or_insert(Vec2::default());
                    #[cfg(not(clash_plant = "knock_last_wins"))]
                    {
                        *s = s.add(d);
                    }
                    #[cfg(clash_plant = "knock_last_wins")]
                    {
                        *s = d; // PLANT: the last buffered push replaces the others (buffer order matters).
                    }
                }
                spell::Knock::Push { src, strength, caster, .. } => {
                    if self.arm_ladder(i, src, strength, caster, sum[i].is_some()) {
                        sum[i] = Some(None);
                    }
                }
            }
        }
        let mut moved = false;
        for (i, s) in sum.iter().enumerate() {
            let Some(d) = *s else { continue };
            let e = &mut self.ents;
            // ATTACK (calibration knockback.ATTACK_RESET): MEASURED on two ladders
            // landing on attacking units (capture 20260920-081819-B:
            // a Bomber between hits, swing counter 3500 -> 0 at tick 2021; a
            // Knight mid-windup, load 300 -> 700 on the hit tick 3535): the push
            // interrupts the attack whatever its phase, the unit is state 1 through the
            // ladder and starts a FRESH LoadTime windup on re-entering range after it,
            // the target kept. reset_attack_keep_target: Windup or Cooldown -> Idle;
            // the two reset_windup_* foils leave a cooldown running (frozen by
            // `Entities::knocked` while the ladder runs, resumed after).
            #[cfg(not(clash_plant = "knockback_keeps_windup"))]
            let resets = match c.knock_attack_reset {
                KnockAttackReset::ResetAttackKeepTarget => e.attack_phase[i] != AttackPhase::Idle,
                KnockAttackReset::ResetWindupKeepTarget | KnockAttackReset::ResetWindupClearTarget => e.attack_phase[i] == AttackPhase::Windup,
            };
            #[cfg(clash_plant = "knockback_keeps_windup")]
            let resets = false; // PLANT: a push leaves the windup running.
            if resets {
                e.attack_phase[i] = AttackPhase::Idle;
                e.attack_ms[i] = 0;
                e.target_locked[i] = false;
                // THE LOAD TIMER with it (the same measurement): the Knight's load
                // reads 700 = LoadTime on the hit tick itself, not the 300 it had
                // left. Under combat.ATTACK_CYCLE = progress_credit the timer is what
                // a re-entry's credit is taken off, so a reset that left it running
                // would give the pushed unit a shorter windup than the capture shows.
                // The windup arm never reads the column.
                e.attack_load_ms[i] = self.cfg.cards.get(e.card[i]).load_time_ms.max(0);
            }
            // CHARGE (calibration charge.RESET_ON_KNOCKBACK): a push that LANDS clears
            // the charge and the run-up. A sum exists here only for a victim spell.rs
            // `pushable` accepted, and it refuses IgnorePushback without PushbackAll --
            // so a Fireball never reaches a Prince, DarkPrince or BattleRam (all three
            // ship IgnorePushback) and only The Log does. That asymmetry is a
            // PREDICTION of the shipped data, not a defect.
            //
            // UNDER THE client16402 LAW the landing itself touches no charge field: the
            // clearing is the charge tail's own, run inside every pushback tick -- the
            // run-up ADVANCES on the positive-speed ladder ticks (state 1, step > 0) and
            // the zero-speed tick and the back-step tick zero it and clear the charge
            // whatever the progress was -- and every projectile ladder has both, so the
            // charge is gone when the ladder ends (`charge_pass`). The slide arm keeps
            // the landing-tick clear.
            if c.charge_reset_on_knockback && d.is_some() {
                e.charged[i] = false;
                e.charge_progress[i] = 0;
            }
            if c.knock_attack_reset == KnockAttackReset::ResetWindupClearTarget {
                e.target[i] = None;
                e.target_locked[i] = false;
            }
            let Some(d) = d else { continue }; // the ladder is armed; its steps come in the Path phase
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
        fx.buffs.clear();
        self.effects = fx;
    }

    /// ARM THE KNOCKBACK LADDER on entity `i` (knockback.DISPLACEMENT_LAW =
    /// client16402): the gate a projectile push passes, then move16402.rs
    /// `start_pushback`. `landed_this_tick` is an earlier push of the same Resolve
    /// on the same unit, which armed it already. Returns whether the push landed.
    ///
    /// THE GATE: a ladder already active -> refused (knockback.STACKING =
    /// first_wins_while_active); IgnorePushback and no PushbackAll -> refused
    /// (spell.rs `pushable` decided it before buffering, as it decides
    /// AFFECTS_DEPLOYING_UNITS); the NO_PUSHBACK tag is assumed clear for every corpus
    /// card, like the contact law's tags. The engine keeps knockback.ATTACK_RESET's
    /// windup reset, applied by the caller when the push lands.
    fn arm_ladder(&mut self, i: usize, src: Vec2, strength: i32, caster: Team, landed_this_tick: bool) -> bool {
        use crate::fixed::SUBTILE_PER_MILLITILE as K;
        let c = &self.cfg.calib;
        let e = &mut self.ents;
        if e.push_active[i] || landed_this_tick {
            return false;
        }
        let zero_dir = match c.knock_zero_vector {
            KnockZeroVector::CasterForward => Some((0, spell::forward_dy(caster))),
            KnockZeroVector::Client16402XByIdParity => Some((if e.team_seq[i] & 1 == 1 { -1 } else { 1 }, 0)),
            KnockZeroVector::NoPush => None,
        };
        let pos = (e.pos[i].x / K, e.pos[i].y / K);
        let Some(start) = move16402::start_pushback(pos, (src.x, src.y), strength, c.max_pushback_length, zero_dir) else {
            return false;
        };
        #[cfg(clash_plant = "knockback_instant_slide")]
        {
            // PLANT (regression): the earlier displacement -- the whole length at
            // once, no ladder, no back-step, no path drop.
            let _ = start.speed;
            let old = e.pos[i];
            let desired = Vec2::new(start.target.0 * K, start.target.1 * K);
            e.pos[i] = spell::settle(&self.cfg.arena, &self.scratch.obstacles[0], e.team[i], e.radius[i], e.flying[i], old, desired);
            return true;
        }
        #[allow(unreachable_code)]
        {
            e.push_target[i] = Vec2::new(start.target.0, start.target.1);
            e.push_speed[i] = start.speed;
            e.push_active[i] = true;
            true
        }
    }

    fn phase_resolve(&mut self) {
        let out = combat::resolve(&mut self.ents, &mut self.dmg, &mut self.scratch.sums, self.cfg.calib.hide_hidden_immune, self.tick);
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
        // as released units (spawner.RELEASE_TIMING: on the death frame and inert on it, or
        // queued for the next Spawn phase), placed by `death_spawn_points` in the owner's frame, at the
        // owner's team and level, with the block's deploy time or the calibration
        // default. Collected and sorted by (team, the dead entity's team_seq, unit
        // index) before the push, so the queue order is canonical.
        //
        // A DEATH FIRES EVERY BLOCK IT CARRIES, INDEPENDENTLY: the death spawn here,
        // the death AREA EFFECT and the death DAMAGE disc below. The Ice Golem ships
        // the last two together (a 2000-millitile disc of 33 and a 2000-millitile
        // area that carries a slow and no damage of its own) and the Super Ice Golem
        // ships them with different radii, different damage and different crown
        // percents -- two columns, one death, neither one the other's carrier.
        let mut spawned: Vec<(Team, u32, u32, PendingSpawn)> = Vec::new();
        self.scratch.dying_blockers.clear();
        #[cfg(not(clash_plant = "death_spawn_dropped"))]
        for id in &deaths {
            let i = id.index as usize;
            let card = self.cfg.cards.get(self.ents.card[i]);
            let Some(ds) = card.death_spawn else { continue };
            let unit = self.cfg.cards.get(ds.unit);
            let level = self.cfg.cards.death_spawn_level(self.ents.card[i], self.ents.level[i]).expect("death spawn level validated at deploy");
            // A DEATH BOMB (card.rs `convert_death_bomb`: the Balloon's, the Giant
            // Skeleton's, the Bomb Tower's) is not spawned, because it is not a unit:
            // it has no hitpoints, nothing can target it and it stands in nobody's
            // way. It is one area hit on a timer, left exactly where the parent fell
            // and released as the spell object an Arrows wave already waits in --
            // `SpellMotion::Flight` with a delay, aimed at the point it starts from,
            // so it arrives on the first tick after the fuse runs out and applies the
            // same enemies-only splash `phase_reap` gives an ordinary DeathDamage row.
            // THE FUSE IS THE BOMB ROW'S OWN DeployTime and NOT `deploy_ms` below:
            // DEATH_SPAWN_DEPLOY_TIME_DEFAULT is measured `zero`, which says how fast
            // a spawned UNIT wakes up and would fire this bomb on the death tick. The
            // corpus separates them -- a Balloon's bomb lands 61 ticks after its last
            // live frame, not on it (card.rs `convert_death_bomb`).
            // ONE bomb, whatever DeathSpawnCount says: all three rows leave it blank
            // (read as 1), and a ring of N discs on one point is a layout the data
            // does not ask for and nothing has measured. A row that asked for more
            // would need `death_spawn_points` and its own reading.
            if let Some(fuse_ms) = unit.death_bomb_fuse_ms() {
                let base = unit.death_damage;
                let damage = self.cfg.cards.scaled(ds.unit, level, base).expect("death bomb level validated at deploy");
                let (team, pos) = (self.ents.team[i], self.ents.pos[i]);
                self.spells.push(Spell {
                    team,
                    card: ds.unit,
                    level,
                    damage,
                    pulse: 0,
                    motion: spell::SpellMotion::Flight { pos, aim: pos, frac: Vec2::default(), delay_ms: fuse_ms },
                });
                continue;
            }
            let radius = ds.radius.unwrap_or(match self.cfg.calib.death_spawn_radius_default {
                DeathSpawnRadius::OwnCollisionRadius => self.ents.radius[i],
                DeathSpawnRadius::Zero => 0,
            });
            let deploy_ms = ds.deploy_time_ms.or(match self.cfg.calib.death_spawn_deploy_default {
                DeathSpawnDeploy::UnitOwnDeployTime => None,
                DeathSpawnDeploy::Zero => Some(0),
            });
            let team = self.ents.team[i];
            // THE RING'S AXIS: the direction from the death point to the dying unit's
            // TARGET (the two live rams: the Barbarians' axis at 26.0 / 84.1 degrees
            // against 26.5 / 83.7 to the tower they were hitting, where the last
            // movement direction was 20.8 / 89.6), else the unit's facing, else the
            // seat's forward.
            let facing = match self.ents.target[i].filter(|t| self.ents.is_alive(*t)) {
                Some(t) => {
                    let d = self.ents.pos[t.index as usize].sub(self.ents.pos[i]);
                    if d == Vec2::default() { self.ents.facing[i] } else { d }
                }
                None => self.ents.facing[i],
            };
            let shift = card.formation.spawn_angle_shift_deg;
            // spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide: a dying unit whose ROW sets
            // DeathSpawnPushback (card.rs `death_spawn_pushback`: the Golem and the Lava Hound
            // among the loaded cards; the Battle Ram leaves it blank) lays its members on the
            // small fixed ring (`death_spawn_points` -> `fixed_slide_ring`), and each carries
            // the slide out to DeathSpawnRadius that the Path phase runs. Every other row, and
            // every row under the shipped not_read, keeps DEATH_SPAWN_LAYOUT.
            #[cfg(not(any(clash_plant = "death_ring_slide_facing", clash_plant = "death_ring_slide_ignores_flag", clash_plant = "death_ring_slide_ignores_arm")))]
            let slide = self.cfg.calib.death_spawn_pushback == DeathSpawnPushback::ClientRingSlide && card.death_spawn_pushback;
            #[cfg(clash_plant = "death_ring_slide_facing")]
            let slide = false; // PLANT (regression): a flagged row keeps the facing ring under the new arm.
            #[cfg(clash_plant = "death_ring_slide_ignores_flag")]
            let slide = self.cfg.calib.death_spawn_pushback == DeathSpawnPushback::ClientRingSlide; // PLANT: the Battle Ram slides too.
            #[cfg(clash_plant = "death_ring_slide_ignores_arm")]
            let slide = card.death_spawn_pushback; // PLANT: the flagged rows slide under not_read as well.
            // ONLY A TROOP DEATH SPAWN takes the ring and the slide: the slide runs, and ends,
            // in the Path phase's troop loops alone, so a building laid on the small ring would
            // keep its slide for ever and never take a target. No loaded row pairs the flag
            // with a building (every flagged row in the 15.535 table spawns troops); such a row
            // keeps DEATH_SPAWN_LAYOUT. Plant death_slide_on_a_building drops this line.
            #[cfg(not(clash_plant = "death_slide_on_a_building"))]
            let slide = slide && unit.kind == CardKind::Troop;
            let ring = DeathSpawnRing { count: ds.count, unit_radius: unit.collision_radius, flying: unit.is_flying(), facing, angle_shift_deg: shift, radius, slide };
            // spawner.DEATH_SPAWN_AT_EMISSION_POINT = client16402_measured_list: a LISTED unit
            // with a spawner puts every member on the point its periodic units come out at
            // (`spawn_point`, through the one-member formation its timed waves use), all
            // together. Unlisted units, and a listed one without a spawner, keep the ring.
            let emission = match self.spawner_of(i) {
                Some(sp) if self.cfg.calib.death_spawn_at_emission == DeathAtEmission::MeasuredList && self.cfg.calib.death_spawn_at_emission_units.contains(&card.unit_name) => {
                    let point = self.spawn_point(i, &sp);
                    Some(self.formation_points(team, 1, unit.collision_radius, unit.is_flying(), point)[0])
                }
                _ => None,
            };
            let points = match emission {
                Some(p) => vec![p; ds.count.max(1) as usize],
                None => self.death_spawn_points(team, self.ents.pos[i], ring),
            };
            // The slide each member of the fixed ring carries: from the death point out to
            // DeathSpawnRadius, subtiles. None when the radius is at or inside the ring's own
            // start radius (the members are born on it: nothing to slide), and none on any
            // other layout.
            let (slide_centre, slide_radius) = if slide && emission.is_none() && radius > crate::fixed::milli(move16402::DEATH_SLIDE_START) {
                (self.ents.pos[i], radius)
            } else {
                (Vec2::default(), 0)
            };
            // spawner.SPAWNED_FIRST_STEP: a death spawn takes its first update on its death frame,
            // except the members of a DeathSpawnPushback row, whose first movement is the slide:
            // measured on client 16.402, a Golem's Golemites exist on the death frame and are inert
            // there, whatever this key says. Read off the row, not the slide's arm, so a Golemite
            // laid by DEATH_SPAWN_LAYOUT under not_read is not stepped either.
            #[cfg(not(clash_plant = "first_step_moves_pushback_spawns"))]
            let first_update = !card.death_spawn_pushback;
            #[cfg(clash_plant = "first_step_moves_pushback_spawns")]
            let first_update = true; // PLANT (regression): a Golemite steps on its death frame.
            // A DYING BUILDING STAYS IN ITS MEMBERS' FIRST AVOIDANCE SCAN, as a static blocker, and out of
            // their separation push (`first_update`). A dying troop does not, because the troops disagree on
            // client 15.535.29: the Elixir Golem's halves read the +-190 of a blocker (42 of 48), the Battle
            // Ram's Barbarians read 0 where its body would block (24 of 25).
            #[cfg(not(clash_plant = "first_step_parent_gone"))]
            let blocks = first_update && self.ents.kind[i].is_building();
            #[cfg(clash_plant = "first_step_parent_gone")]
            let blocks = false; // PLANT (regression): the dying building is gone from the scan.
            if blocks {
                use crate::fixed::SUBTILE_PER_MILLITILE as K;
                let p = self.ents.pos[i];
                self.scratch.dying_blockers.push((p.x / K, p.y / K, self.ents.radius[i] / K, self.ents.team[i] as u8));
            }
            // spawner.DEATH_SPAWN_LAYOUT = facing_ring_rounded: the members start with the dying unit's heading, the
            // ring's axis normalized to 256 in native units (measured on client 15.535.29: (28, -254) for a Ram dying
            // on a tower). Only where that ring was laid: not on the slide, not at the emission point.
            #[cfg(not(clash_plant = "death_ring_members_face_forward"))]
            let keeps_heading = self.cfg.calib.death_spawn_layout == DeathSpawnLayout::FacingRingRounded && !slide && emission.is_none();
            #[cfg(clash_plant = "death_ring_members_face_forward")]
            let keeps_heading = false; // PLANT (regression): the members face their side's forward.
            let member_facing = if keeps_heading && facing != Vec2::default() {
                use crate::fixed::SUBTILE_PER_MILLITILE as K;
                let mut v = (facing.x / K, facing.y / K);
                if crate::move16402::normalize_to(&mut v, 256) != 0 { Some(Vec2::new(v.0, v.1)) } else { None }
            } else {
                None
            };
            for (k, p) in points.into_iter().enumerate() {
                spawned.push((team, self.ents.team_seq[i], k as u32, PendingSpawn { team, card: ds.unit, level, pos: p, deploy_ms, owner: None, stagger_ms: 0, slide_centre, slide_radius, acquire_delay: true, first_update, facing: member_facing }));
            }
        }
        spawned.sort_by_key(|(t, seq, k, _)| (*t as u8, *seq, *k));
        for (_, _, _, p) in spawned {
            self.release(p);
        }
        // A death releases its area effect (card.rs `death_area_effect`) into the same
        // spell list a cast goes into, so the disc applies in the NEXT tick's
        // Projectile phase -- the same tick the death damage buffered below resolves,
        // because Reap runs after Resolve. Collected first and appended once: the
        // death queue's order is `combat::resolve`'s canonical one, and the borrow of
        // `cfg` ends before `spells` is touched.
        let mut released: Vec<spell::Spell> = Vec::new();
        for id in &deaths {
            let i = id.index as usize;
            let idx = self.ents.card[i];
            if self.cfg.cards.get(idx).death_area_effect.is_none() {
                continue;
            }
            released.extend(
                spell::cast(&self.cfg.cards, &self.cfg.calib, &self.cfg.arena, self.ents.team[i], idx, self.ents.level[i], self.ents.pos[i])
                    .expect("death area effect level validated at deploy"),
            );
        }
        self.spells.append(&mut released);
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
        // RELEASED UNITS (spawner.RELEASE_TIMING = end_of_event_phase): after the dead
        // are despawned, so a freed slot can be reused without any later read of it, and
        // before the rebuild, so the hash holds them.
        self.materialise_released();
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

    // A deploy goes through `formation_members` (calibration formation.LAYOUT); the
    // centred grid stays as `formation_grid` for the engine_grid arm, the releases
    // and the death spawns.

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

    /// A spell RELEASE laid by the formation ring (spells.PROJECTILE_SPAWN_FORMATION =
    /// count_ring_tight): `count` members of `unit`, radius its collision radius, in the
    /// caster's frame and lane (the same `member_offset` a deploy uses), clamped half a cell
    /// inside the arena, and a ground member still on water put on the nearest land.
    fn release_ring_points(&self, team: Team, count: i32, unit: u16, pos: Vec2) -> Vec<Vec2> {
        use crate::fixed::SUBTILE_PER_MILLITILE as K;
        let arena = &self.cfg.arena;
        let u = self.cfg.cards.get(unit);
        let own = arena.to_frame(team, pos);
        let layout = crate::formation::Layout {
            primaries: count.max(1),
            seconds: 0,
            radius: u.collision_radius / K,
            width: 0,
            angle_shift: 0,
            lane: crate::formation::nearest_lane(arena, own),
            lane_mirror: self.cfg.calib.lane_id_based_deploy_sequence,
        };
        let tap = Vec2::new(own.x / K, own.y / K);
        let (w, h) = (arena.width / K, arena.height / K);
        let margin = arena.cell / K / 2;
        (0..count.max(1))
            .map(|k| {
                let p = tap.add(crate::formation::member_offset(layout, k));
                let p = Vec2::new(p.x.clamp(margin, w - margin), p.y.clamp(margin, h - margin));
                let abs = arena.from_frame(team, Vec2::new(p.x * K, p.y * K));
                if u.is_flying() || arena.is_passable_ground(abs) { abs } else { arena.nearest_passable_ground(abs, team).unwrap_or(abs) }
            })
            .collect()
    }

    /// THE ENGINE GRID: a centred grid, first row toward the enemy, spacing one
    /// collision diameter, in the team's own frame (Red gets the ROTATED offset
    /// (-dx, -dy), so sibling k sits in the same own-frame place for both seats and
    /// team_seq follows own-left-to-right for both. A y-reflected offset (dx, -dy)
    /// instead makes sibling order follow ENGINE x for both teams, one of the
    /// reasons a policy shared by both seats desyncs on multi-unit deploys (80 of
    /// 144)). A GUESS, and for deploys a refuted one (calibration formation.LAYOUT =
    /// engine_grid, the runnable foil: the corpus shows a ring, formation.rs); still
    /// the shipped layout of a spell release (spells.PROJECTILE_SPAWN_FORMATION) and
    /// a death spawn (spawner.DEATH_SPAWN_LAYOUT).
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
            self.spawn_queue.push(PendingSpawn { team, card: idx, level, pos, deploy_ms: None, owner: None, stagger_ms: 0, slide_centre: Vec2::default(), slide_radius: 0, acquire_delay: false, first_update: false, facing: None });
            return;
        }
        for m in self.formation_members(team, idx, level, pos) {
            self.spawn_queue.push(m);
        }
    }

    /// A DEPLOY'S MEMBERS: card, level, point and deploy timer for each of the
    /// SummonNumber primaries and the SummonCharacterSecondCount second summons of
    /// card `idx` tapped at `pos` by `team`, in creation order (member k = queue
    /// order = team_seq order), under calibration formation.LAYOUT / DEPLOY_STAGGER /
    /// GROUND_Y_CLAMP. Pure; `formation_preview` exposes it for the tests.
    ///
    /// client16402 (formation.rs): the tap goes into the OWNER'S frame and native
    /// units, member k's offset is `member_offset` (the ring, the line, the spiral;
    /// the lane mirror on the owner's-frame lane), the point is clamped (the ground
    /// column range, then [250, W - 250] x [250, H - 250] native), rotated back for
    /// Red and water-ejected for a ground unit that still stands on the river.
    /// Every member of a multi-unit deploy stands on a NATIVE point (a multiple of
    /// 18 subtiles: the resolution every measured position has); a single summon
    /// keeps the exact subtile tap. The stagger is `stagger_ms` added to the unit's
    /// own DeployTime.
    ///
    /// engine_grid: the earlier `formation_grid` over all the members.
    fn formation_members(&self, team: Team, idx: u16, level: i32, pos: Vec2) -> Vec<PendingSpawn> {
        use crate::fixed::SUBTILE_PER_MILLITILE as K;
        let cards = &self.cfg.cards;
        let card = cards.get(idx);
        let fd = card.formation;
        let n = card.count.max(1);
        let second = fd.second_summon.filter(|d| d.unit != u16::MAX);
        let s = second.map_or(0, |d| d.count.max(0));
        let total = n + s;
        let unit_of = |k: i32| if k < n { idx } else { second.expect("k >= n only with a second summon").unit };
        let level_of = |k: i32| {
            if k < n {
                level
            } else {
                cards.unit_level(idx, unit_of(k), None, level).expect("second summon level validated at check_levels")
            }
        };
        let calib = &self.cfg.calib;
        let stagger = |k: i32| match calib.formation_deploy_stagger {
            DeployStagger::Client16402 => {
                crate::formation::stagger_ms(k, n, fd.summon_deploy_delay_ms, fd.summon_deploy_delay_second_ms, card.kind == CardKind::Building)
            }
            DeployStagger::None => 0,
        };
        let member = |k: i32, p: Vec2| {
            let unit = unit_of(k);
            let delay = stagger(k);
            // The stagger only exists for a unit with a DeployTime.
            let own = cards.get(unit).deploy_time_ms;
            let deploy_ms = if delay > 0 && own > 0 { Some(own + delay) } else { None };
            let stagger_ms = if deploy_ms.is_some() { delay } else { 0 };
            PendingSpawn { team, card: unit, level: level_of(k), pos: p, deploy_ms, owner: None, stagger_ms, slide_centre: Vec2::default(), slide_radius: 0, acquire_delay: false, first_update: false, facing: None }
        };
        #[cfg(not(clash_plant = "formation_grid_legacy"))]
        let layout = calib.formation_layout;
        #[cfg(clash_plant = "formation_grid_legacy")]
        let layout = FormationLayout::EngineGrid; // PLANT (regression): the square grid on every swarm.
        match layout {
            FormationLayout::EngineGrid => {
                let flying = card.is_flying();
                self.formation_grid(team, total, card.collision_radius, flying, pos).into_iter().enumerate().map(|(k, p)| member(k as i32, p)).collect()
            }
            FormationLayout::Client16402 => {
                if total == 1 {
                    // A SINGLE GROUND UNIT under placement.TAP_SNAP (formation.GROUND_DEPLOY_POINT,
                    // which a ring already takes) or placement.TROOP_TOWER_TAPS (raised to its
                    // column's BACK bound when it stands behind it: measured on client 15.535.29 for
                    // side 1, a Knight at own (8500, 500) stands on own 1000). Nothing else moves a
                    // single unit: the column's front bound and the passable-ground ejection were
                    // never measured for one, and a scenario unit placed in the enemy half
                    // (spawn_unit) keeps its exact point.
                    let single_point = calib.placement_tap_snap == TapSnap::TileCentre && calib.formation_ground_deploy_point == GroundDeployPoint::Client16402OneUnit;
                    let single_clamp = calib.placement_troop_tower_taps == TroopTowerTaps::HalfOpenRelocate;
                    if cards.get(unit_of(0)).is_flying() || !(single_point || single_clamp) {
                        return vec![member(0, pos)];
                    }
                    let arena = &self.cfg.arena;
                    let own = arena.to_frame(team, pos);
                    let tap = Vec2::new(own.x / K, own.y / K);
                    let mut p = tap;
                    if single_point {
                        let dy = if team == Team::Red { 1 } else { 0 };
                        let dx = if pos.x < arena.width / 2 { if team == Team::Red { 1 } else { -1 } } else { 0 };
                        p = Vec2::new(p.x + dx, p.y + dy);
                    }
                    if single_clamp && !single_point {
                        match self.ground_y_range(team, idx, tap) {
                            Some((lo, _)) if p.y < lo => p.y = lo,
                            _ => return vec![member(0, pos)],
                        }
                    } else if single_clamp {
                        if let Some((lo, _)) = self.ground_y_range(team, idx, tap) {
                            p.y = p.y.max(lo);
                        }
                    }
                    let abs = arena.from_frame(team, Vec2::new(p.x * K, p.y * K));
                    let abs = if arena.is_passable_ground(abs) { abs } else { arena.nearest_passable_ground(abs, team).unwrap_or(abs) };
                    return vec![member(0, abs)];
                }
                let arena = &self.cfg.arena;
                let own = arena.to_frame(team, pos);
                // SummonRadius, else the primary's CollisionRadius overridden by its
                // SpawnRadius when set (the Skeleton Warriors' 923 ring: SpawnRadius
                // 800 scaled).
                let radius = if fd.summon_radius != 0 {
                    fd.summon_radius
                } else if fd.spawn_radius != 0 {
                    fd.spawn_radius
                } else {
                    card.collision_radius
                };
                let layout = crate::formation::Layout {
                    primaries: n,
                    seconds: s,
                    radius: radius / K,
                    width: fd.summon_width / K,
                    angle_shift: fd.spawn_angle_shift_deg,
                    lane: crate::formation::nearest_lane(arena, own),
                    lane_mirror: calib.lane_id_based_deploy_sequence,
                };
                let tap = Vec2::new(own.x / K, own.y / K);
                // formation.GROUND_DEPLOY_POINT: a GROUND summon's ring is laid
                // around a point one native unit off the tap -- x when the tap is on
                // the arena's LEFT half, y when the owner is side 1 -- and a FLYING
                // one's is laid around the tap itself. Measured, both seats. In the
                // OWNER's frame an absolute -1 is -1 for Blue and +1 for Red, so the
                // two offsets are signed by the frame, not by the rule.
                let ground_tap = match calib.formation_ground_deploy_point {
                    GroundDeployPoint::None => tap,
                    GroundDeployPoint::Client16402OneUnit => {
                        let dy = if team == Team::Red { 1 } else { 0 };
                        let dx = if pos.x < arena.width / 2 { if team == Team::Red { 1 } else { -1 } } else { 0 };
                        Vec2::new(tap.x + dx, tap.y + dy)
                    }
                };
                let y_range = match calib.formation_ground_y_clamp {
                    GroundYClamp::Client16402DeployColumnRange | GroundYClamp::DeployColumnRangeOwnFrame => self.ground_y_range(team, idx, tap),
                    GroundYClamp::None => None,
                };
                // The arena-bounds clamp, native: half a cell inside every edge.
                let (w, h) = (arena.width / K, arena.height / K);
                let margin = arena.cell / K / 2;
                (0..total)
                    .map(|k| {
                        let unit = cards.get(unit_of(k));
                        let off = crate::formation::member_offset(layout, k);
                        let mut p = if unit.is_flying() { tap } else { ground_tap }.add(off);
                        if let (Some((lo, hi)), false) = (y_range, unit.is_flying()) {
                            // y <= lo -> lo, else min(y, hi).
                            p.y = if p.y <= lo { lo } else { p.y.min(hi) };
                        }
                        p = Vec2::new(p.x.clamp(margin, w - margin), p.y.clamp(margin, h - margin));
                        let abs = arena.from_frame(team, Vec2::new(p.x * K, p.y * K));
                        let abs = if unit.is_flying() || arena.is_passable_ground(abs) { abs } else { arena.nearest_passable_ground(abs, team).unwrap_or(abs) };
                        member(k, abs)
                    })
                    .collect()
            }
        }
    }

    /// The tap column's deployable y range for a ground summon of card `idx` by
    /// `team`, OWN-FRAME NATIVE units, or None when no clamp applies: the deploy
    /// mask over the tile grid is scanned down the tap's tile column, keeping `lo =
    /// min(row x 1000)` and `hi = max(row x 1000 + 500)` over the deployable rows
    /// (side 0's formula; side 1's is the same range seen from the top, one native
    /// unit and a whole back row apart, which the owner's frame does not carry:
    /// calibration formation.GROUND_Y_CLAMP). The pair is dropped once
    /// `hi - lo >= H / 2`, the range reaching past the river after a tower has
    /// fallen. Side 1 keeps `lo = min(row x 1000 + 500) - 1` and
    /// `hi = max(row x 1000)` in absolute rows under the shipped arm (the Red
    /// Goblins of capture 20260918-164951-B tapped on (3500, 30500) hold their rear
    /// pair on 31000, not 31261; those of capture 20260918-121158 t2439 stand on the
    /// river bound's 17499, not the own-frame arm's 17500). The mask
    /// is the engine's own troop TERRITORY per tile centre (arena.rs
    /// `territory_zone`: the river band and the enemy tower rects, not the
    /// tilemap's NO_DEPLOY corners -- the live Goblin Gang on (3500, 1500) stands a
    /// Spear Goblin on y 346 inside that corner strip).
    fn ground_y_range(&self, team: Team, idx: u16, tap_own_native: Vec2) -> Option<(i32, i32)> {
        use crate::fixed::SUBTILE_PER_MILLITILE as K;
        let arena = &self.cfg.arena;
        let card = self.cfg.cards.get(idx);
        let (territory, _) = deploy_rule(&self.cfg.calib, card);
        let rects = self.enemy_no_deploy_rects(team);
        let tile = arena.cell * 2 / K; // 1000 native
        let h = arena.height / K;
        let col = tap_own_native.x / tile;
        let rows = h / tile;
        // The deployable rows of the tap's column, OWN-FRAME row numbers.
        let deployable: Vec<i32> = (0..rows)
            .filter(|&r| {
                let centre_own = Vec2::new((col * tile + tile / 2) * K, (r * tile + tile / 2) * K);
                arena.territory_zone(arena.from_frame(team, centre_own), team, territory, &rects).is_ok()
            })
            .collect();
        let (lo, hi) = match (self.cfg.calib.formation_ground_y_clamp, team) {
            (GroundYClamp::None, _) => return None,
            // Side 0's formula: lo over the rows' near edges, hi over their centres.
            // Own frame = absolute for Blue.
            (GroundYClamp::DeployColumnRangeOwnFrame, _) | (GroundYClamp::Client16402DeployColumnRange, Team::Blue) => {
                (deployable.iter().map(|r| r * tile).min()?, deployable.iter().map(|r| r * tile + tile / 2).max()?)
            }
            // Side 1's formula as measured: lo over the rows' centres, minus one, hi
            // over their near edges -- in ABSOLUTE rows, then turned into the owner's
            // frame (y_own = H - y_abs).
            (GroundYClamp::Client16402DeployColumnRange, Team::Red) => {
                let abs_rows = deployable.iter().map(|r| rows - 1 - r);
                let lo_abs = abs_rows.clone().map(|r| r * tile + tile / 2).min()? - 1;
                let hi_abs = abs_rows.map(|r| r * tile).max()?;
                (h - hi_abs, h - lo_abs)
            }
        };
        if hi - lo >= h / 2 {
            return None; // the column reaches past the river
        }
        Some((lo, hi))
    }

    /// TEST HOOK: the members `formation_members` would queue for a deploy of
    /// `card_name` by `team` at `pos`, as (unit card name, absolute point, the
    /// deploy timer the entity starts with in ms), in creation order.
    pub fn formation_preview(&self, team: Team, card_name: &str, pos: Vec2) -> Result<Vec<(String, Vec2, i32)>, DeployError> {
        let idx = self.simulable(card_name)?;
        let level = self.cfg.card_level[team as usize];
        self.cfg.cards.check_levels(idx, level).map_err(DeployError::InvalidLevel)?;
        Ok(self
            .formation_members(team, idx, level, pos)
            .into_iter()
            .map(|m| (self.cfg.cards.get(m.card).name.clone(), m.pos, m.deploy_ms.unwrap_or(self.cfg.cards.get(m.card).deploy_time_ms)))
            .collect())
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
        let half_open = card.kind == CardKind::Troop && self.cfg.calib.placement_troop_tower_taps == TroopTowerTaps::HalfOpenRelocate;
        let zone = if half_open {
            self.cfg.arena.deploy_zone_king_half_open(pos, team, territory, &rects)
        } else {
            self.cfg.arena.deploy_zone(pos, team, territory, &rects)
        };
        match zone {
            // spells.ILLEGAL_SPELL_TAP = clamp: a spell outside its territory is clamped, not refused.
            Err(crate::arena::ZoneError::OutOfTerritory)
                if card.kind == CardKind::Spell && self.cfg.calib.illegal_spell_tap == IllegalSpellTap::ClampToLegalEdge && self.clamp_spell_tap(team, idx, pos).is_some() => {}
            other => other?,
        }
        // A BUILDING IS JUDGED BY ITS FOOTPRINT, NOT BY THE TAP POINT. The tap
        // above decides territory and the cell rules; the box below decides
        // whether the building fits, and where it ends up.
        if card.kind == CardKind::Building {
            return match self.building_placement(team, idx, pos) {
                Some(_) => Ok(()),
                None => Err(DeployError::Occupied),
            };
        }
        #[cfg(clash_plant = "flying_ignores_buildings")]
        if card.is_flying() {
            // PLANT (regression): an air-unit exemption from the footprint check.
            return Ok(());
        }
        // A troop or a spell-released unit: no extra radius, because only a
        // building carried one and a building no longer takes this path.
        // Under placement.TROOP_TOWER_TAPS a tap on an own crown tower is MOVED off it, so the
        // footprint is judged where the troop will stand.
        let at = if half_open { self.resolve_point(team, idx, pos) } else { pos };
        if footprint_rule && self.footprint_covers(at, 0) {
            return Err(DeployError::Occupied);
        }
        Ok(())
    }

    /// Where a building tapped at `tap` actually lands, and the tile box it takes.
    /// `None` when nothing legal is within the search bound.
    ///
    /// PURE. `check_deploy` asks it for the verdict and `deploy_slot` asks it for
    /// the point, so the two cannot answer differently.
    ///
    /// The tap is snapped first (`Arena::snap_placement`), then the box is judged
    /// whole: inside the arena, off water and no-deploy cells, inside the placer's
    /// territory, and sharing no POSITIVE AREA with an alive tower or building.
    /// Flush contact is legal. If the snapped box does not fit, the tap is
    /// relocated under calibration placement.ILLEGAL_TAP.
    ///
    /// The placement box is NOT a collision shape: a troop may stand inside one,
    /// and `collision.BUILDING_FOOTPRINT_MODEL` still decides what movement sees.
    /// WHERE A PLAY GOES DOWN (pure; the play path and `spawn_unit` share it). A building:
    /// `building_placement`. A spell outside its territory under spells.ILLEGAL_SPELL_TAP =
    /// clamp: clamped back along its column. Then, under placement.TAP_SNAP, the plain tile
    /// centre. Then a troop under placement.TROOP_TOWER_TAPS: moved off an own crown tower.
    pub fn resolve_point(&self, team: Team, idx: u16, pos: Vec2) -> Vec2 {
        let card = self.cfg.cards.get(idx);
        let calib = &self.cfg.calib;
        if card.kind == CardKind::Building {
            return self.building_placement(team, idx, pos).map_or(pos, |(c, _)| c);
        }
        let mut p = pos;
        if card.kind == CardKind::Spell && calib.illegal_spell_tap == IllegalSpellTap::ClampToLegalEdge {
            let (territory, _) = deploy_rule(calib, card);
            let rects = self.enemy_no_deploy_rects(team);
            if matches!(self.cfg.arena.deploy_zone(p, team, territory, &rects), Err(crate::arena::ZoneError::OutOfTerritory)) {
                if let Some(c) = self.clamp_spell_tap(team, idx, p) {
                    p = c;
                }
            }
        }
        if calib.placement_tap_snap == TapSnap::TileCentre {
            let t = self.cfg.arena.cell * 2;
            p = Vec2::new(p.x.div_euclid(t) * t + t / 2, p.y.div_euclid(t) * t + t / 2);
        }
        if card.kind == CardKind::Troop && calib.placement_troop_tower_taps == TroopTowerTaps::HalfOpenRelocate {
            p = self.relocate_off_own_crown_tower(team, idx, p);
        }
        p
    }

    /// spells.ILLEGAL_SPELL_TAP = clamp: the first tile centre, stepping back along the tap's
    /// column toward the caster, that the spell's territory accepts; None if none does.
    fn clamp_spell_tap(&self, team: Team, idx: u16, tap: Vec2) -> Option<Vec2> {
        let arena = &self.cfg.arena;
        let (territory, _) = deploy_rule(&self.cfg.calib, self.cfg.cards.get(idx));
        let rects = self.enemy_no_deploy_rects(team);
        let t = arena.cell * 2;
        let own = arena.to_frame(team, tap);
        let (col, row0) = (own.x.div_euclid(t), own.y.div_euclid(t));
        (0..=row0).rev().map(|r| arena.from_frame(team, Vec2::new(col * t + t / 2, r * t + t / 2))).find(|&c| arena.deploy_zone(c, team, territory, &rects).is_ok())
    }

    /// placement.TROOP_TOWER_TAPS: a troop tap whose SNAPPED one-tile box shares positive area
    /// with an alive OWN crown tower's placement box is moved exactly as a one-tile building
    /// would be: `building_placement`'s snap, ring search from the snapped tile, order, fit
    /// (the troop's territory and every building's box) and distance to the tap
    /// (placement.ILLEGAL_TAP, placement.SNAP_EVEN). Otherwise the tap, unsnapped. Searching
    /// from the raw tap instead sends a tap at own (8500, 1499) sideways, because its
    /// straight-back candidate (8500, 499) overhangs the arena's back edge; measured on
    /// client 15.535.29 it lands straight back, on (8499, 500).
    fn relocate_off_own_crown_tower(&self, team: Team, idx: u16, tap: Vec2) -> Vec2 {
        let arena = &self.cfg.arena;
        let e = &self.ents;
        let own: Vec<Rect> = self.towers[team as usize]
            .iter()
            .flatten()
            .filter(|id| e.is_alive(**id))
            .map(|id| {
                let i = id.index as usize;
                Arena::placement_box(e.pos[i], crate::arena::placement_tiles(e.radius[i]))
            })
            .collect();
        let snapped = match self.cfg.calib.placement_snap_even {
            PlacementSnapEven::PlacerFrame => arena.snap_placement(team, tap, 1),
            PlacementSnapEven::Absolute => arena.snap_placement(Team::Blue, tap, 1),
        };
        let on_own_tower = own.iter().any(|t| Arena::placement_box(snapped, 1).overlaps_open(t));
        if !on_own_tower || self.cfg.calib.placement_illegal_tap == PlacementIllegalTap::Refuse {
            return tap;
        }
        let (territory, _) = deploy_rule(&self.cfg.calib, self.cfg.cards.get(idx));
        let fits = |c: Vec2| {
            let b = Arena::placement_box(c, 1);
            arena.box_zone(b, team, territory).is_ok() && !self.box_hits_a_building(b)
        };
        // The candidates are `building_placement`'s ring, but a TIE between two equally near
        // tiles goes to the first in COLUMN-MAJOR order in the placer's frame (columns from
        // its low x, each from its low y). Measured on client 15.535.29, three ties fit it and
        // no other simple order: side 0's own (10500, 1500) takes the tile behind over the one
        // to its +x, side 1's own (7500, 1500) the tile to its -x over the one behind, side 1's
        // own (10500, 1500) the tile behind over the one to its +x. The ring order the building
        // search uses (`ring_offsets`) gets the second wrong.
        let tile = crate::fixed::tiles(1);
        let mut best: Option<(i64, Vec2)> = None;
        for r in 1..=PLACEMENT_SEARCH_RINGS {
            let ring = (-r..=r).flat_map(|dx| (-r..=r).map(move |dy| (dx, dy))).filter(|&(dx, dy)| dx.abs().max(dy.abs()) == r);
            for (dx, dy) in ring {
                let step = Vec2::new(dx * tile, dy * tile);
                let c = match self.cfg.calib.placement_snap_even {
                    PlacementSnapEven::PlacerFrame => arena.from_frame(team, arena.to_frame(team, snapped).add(step)),
                    PlacementSnapEven::Absolute => snapped.add(step),
                };
                if !fits(c) {
                    continue;
                }
                let d = c.dist2(tap);
                if best.map_or(true, |(b, _)| d < b) {
                    best = Some((d, c));
                }
            }
            if best.is_some() && self.cfg.calib.placement_illegal_tap == PlacementIllegalTap::RelocateFirstFittingRing {
                break;
            }
        }
        best.map_or(tap, |(_, c)| c)
    }

    pub fn building_placement(&self, team: Team, idx: u16, tap: Vec2) -> Option<(Vec2, Rect)> {
        let card = self.cfg.cards.get(idx);
        if card.kind != CardKind::Building {
            return None;
        }
        let n = crate::arena::placement_tiles(card.collision_radius);
        let arena = &self.cfg.arena;
        let (territory, _) = deploy_rule(&self.cfg.calib, card);
        // THE TAP POINT'S OWN RULES FIRST. Relocation rescues a tap whose POINT is
        // legal and whose BOX does not fit; a tap that is out of the arena, on
        // water, on a no-deploy cell or outside the placer's territory is refused,
        // and every recorded relocation is inside the placer's own half. Without
        // this the query answers for taps `check_deploy` refuses, and a caller
        // that trusts it would show a landing the engine will not build.
        let rects = self.enemy_no_deploy_rects(team);
        if arena.deploy_zone(tap, team, territory, &rects).is_err() {
            return None;
        }
        let snapped = match self.cfg.calib.placement_snap_even {
            PlacementSnapEven::PlacerFrame => arena.snap_placement(team, tap, n),
            PlacementSnapEven::Absolute => arena.snap_placement(Team::Blue, tap, n),
        };
        let fits = |centre: Vec2| -> bool {
            let b = Arena::placement_box(centre, n);
            arena.box_zone(b, team, territory).is_ok() && !self.box_hits_a_building(b)
        };
        if fits(snapped) {
            return Some((snapped, Arena::placement_box(snapped, n)));
        }
        if self.cfg.calib.placement_illegal_tap == PlacementIllegalTap::Refuse {
            return None;
        }
        // Square rings outward from the snapped tile. Inside a ring, the smallest
        // squared distance to the TAP wins; ties go to the candidate found first,
        // and the scan order is built in the placer's frame so the two seats
        // mirror. `RelocateFirstFittingRing` stops at the end of the first ring
        // that held a fit; `RelocateNearestOverall` keeps looking to the bound.
        let tile = crate::fixed::tiles(1);
        let mut best: Option<(i64, Vec2)> = None;
        for r in 1..=PLACEMENT_SEARCH_RINGS {
            for (dx, dy) in ring_offsets(r) {
                let step = Vec2::new(dx * tile, dy * tile);
                let candidate = match self.cfg.calib.placement_snap_even {
                    PlacementSnapEven::PlacerFrame => arena.from_frame(team, arena.to_frame(team, snapped).add(step)),
                    PlacementSnapEven::Absolute => snapped.add(step),
                };
                if !fits(candidate) {
                    continue;
                }
                let dist = candidate.dist2(tap);
                let better = match best {
                    None => true,
                    Some((b, _)) => dist < b,
                };
                if better {
                    best = Some((dist, candidate));
                }
            }
            if best.is_some() && self.cfg.calib.placement_illegal_tap == PlacementIllegalTap::RelocateFirstFittingRing {
                break;
            }
        }
        best.map(|(_, c)| (c, Arena::placement_box(c, n)))
    }

    /// Does `b` share positive area with an alive tower's or building's own box?
    fn box_hits_a_building(&self, b: Rect) -> bool {
        let e = &self.ents;
        e.live_indices().any(|i| {
            e.kind[i].is_building() && {
                let n = crate::arena::placement_tiles(e.radius[i]);
                b.overlaps_open(&Arena::placement_box(e.pos[i], n))
            }
        })
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
        // THE OPENING LOCKOUT, and it belongs HERE rather than in `deploy_slot` because
        // this is the pure query both entry points ask first: a check the acting path did
        // not make is a second answer that can disagree with the one the engine acted on,
        // which the comment on `deploy_slot` already warns about for building placement.
        // `.max(0) as u32` rather than a cast: a negative lockout in the ledger means
        // "no lockout" instead of a threshold that wraps to four billion ticks.
        let until = self.cfg.calib.deploy_lockout_ticks.max(0) as u32;
        if self.tick < until {
            return Err(DeployError::TooEarly { tick: self.tick, until });
        }
        if self.outcome.is_some() {
            return Err(DeployError::GameOver);
        }
        let idx = self.hand_card(team, slot)?;
        self.check_elixir(team, idx)?;
        self.check_position(team, idx, pos)
    }

    /// Play a card from hand by name (the first slot holding it). Validation is
    /// immediate; the units materialise in the next tick's Spawn phase. Returns where the
    /// card went down, which for a building is not always the tap (`deploy_slot`).
    pub fn deploy(&mut self, team: Team, card_name: &str, pos: Vec2) -> Result<Vec2, DeployError> {
        self.check_deploy(team, card_name, pos)?;
        let idx = self.simulable(card_name)?;
        let slot = self.players[team as usize].hand.iter().position(|c| *c == idx).ok_or(DeployError::NotInHand)?;
        self.deploy_slot(team, slot, pos)
    }

    /// Play the card in `slot`. The played card goes to the back of the queue and
    /// the queue's front takes the slot.
    /// Returns WHERE THE CARD WENT DOWN: the relocated centre for a building whose
    /// footprint did not fit the tap, the tap itself otherwise. The caller is given the
    /// point rather than asked to query for it again, because a second query is a second
    /// answer that can disagree with the one the engine acted on.
    pub fn deploy_slot(&mut self, team: Team, slot: usize, pos: Vec2) -> Result<Vec2, DeployError> {
        self.check_deploy_slot(team, slot, pos)?;
        let idx = self.hand_card(team, slot)?;
        // A BUILDING STANDS WHERE ITS FOOTPRINT FITS, not on the tap. The same
        // pure query decided the verdict above, so the two cannot disagree: if it
        // said yes it returns a point here.
        let pos = self.resolve_point(team, idx, pos);
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
        Ok(pos)
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
        let (kind, flying) = (card.kind, card.is_flying());
        // The play path's resolution (snap, relocation) for troops and spells; a building
        // stays where it was put, as before.
        let pos = if kind == CardKind::Building { pos } else { self.resolve_point(team, idx, pos) };
        if kind == CardKind::Spell {
            // A cast, at any in-bounds point (tests aim spells where no player could).
            self.enqueue(team, idx, level, pos);
            return Ok(());
        }
        if !flying && !self.cfg.arena.is_passable_ground(pos) {
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
            facing: e.facing[i],
            push_applied: e.push_applied[i],
            push_neighbours: e.push_neighbours[i],
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
            attack_load_ms: e.attack_load_ms[i],
            deploy_ms: e.deploy_ms[i],
            target_locked: e.target_locked[i],
            speed: e.speed[i],
            move_frac: e.move_frac[i],
            route: &e.route[i],
            stun_ms: e.stun_ms[i],
            buffs: e.buff_slots(i),
            stomp_clock: e.stomp_clock[i],
            speed_now: self.effective_speed(i),
            retarget_on_resume: e.retarget_on_resume[i],
            knock_ms: e.knock_ms[i],
            knock_rem: e.knock_rem[i],
            push_active: e.push_active[i],
            push_speed: e.push_speed[i],
            push_target: e.push_target[i],
            jumping: e.jumping[i],
            hide_state: e.hide[i],
            hide_ms: e.hide_ms[i],
            hidden: e.hide[i] == HideState::Hidden,
            status_flags: e.status_flags(i),
            spawn_ms: e.spawn_ms[i],
            spawn_wave_left: e.spawn_wave_left[i],
            spawned_by: e.spawned_by[i],
            charged: e.charged[i],
            charge_progress: e.charge_progress[i],
            effective_speed: self.effective_speed(i),
            death_slide_centre: e.death_slide_centre[i],
            death_slide_radius: e.death_slide_radius[i],
            acquirable_from: e.acquirable_from[i],
            avoid_offset: e.avoid_offset[i],
            seg_dir: e.seg_dir[i],
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
        let (free, counters, created) = e.allocator_state();
        h.u32(free.len() as u32);
        for f in free {
            h.u32(*f);
        }
        h.u32(counters[0]);
        h.u32(counters[1]);
        if !legacy_v3 {
            h.u32(created);
        }
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
            // `slow_ms`, a timer nothing ever set, was replaced by the buff list
            // below. The format-3 hash still needs the zero it always
            // contributed in this position, or `legacy_v3` stops reproducing.
            if legacy_v3 {
                h.i32(0);
            }
            if !legacy_v3 {
                // THE BUFF LIST (status.rs): every slot, empty ones included, so a
                // buff landing in a different slot is a different state. The stomp
                // clock rides with it.
                for slot in e.buff_slots(i) {
                    h.u32(slot.id as u32);
                    h.i32(slot.ms);
                    h.i32(slot.pulse_ms);
                    h.i32(slot.pulse_amount);
                }
                h.i32(e.stomp_clock[i]);
                h.bool(e.retarget_on_resume[i]);
                if self.cfg.calib.post_kill_wait != PostKillWait::None {
                    h.i32(e.retarget_wait[i] as i32);
                }
                if self.cfg.calib.post_kill_wait == PostKillWait::AttackFinish {
                    h.bool(e.target_doomed[i]);
                }
                if self.cfg.calib.preserve_target_scope == PreserveTargetScope::ProjectileAttackersOnly {
                    h.bool(e.launched_beyond[i]);
                }
                if self.cfg.calib.doomed_target_drop.drops() {
                    let f = e.fired_at[i];
                    h.bool(f.is_some());
                    h.u32(f.map_or(0, |t| t.index));
                    h.u32(f.map_or(0, |t| t.generation));
                }
                if self.cfg.calib.formation_stagger_wait == StaggerWait::Client16402 {
                    h.i32(e.stagger_ms[i]);
                }
                // spawner.DEATH_SPAWN_PUSHBACK: only while a slide runs, so a battle with none
                // (every battle under the shipped not_read) hashes as it did before the columns.
                if e.death_slide_radius[i] > 0 {
                    h.vec(e.death_slide_centre[i]);
                    h.i32(e.death_slide_radius[i]);
                }
                // targeting.SPAWNED_UNIT_ACQUIRE_DELAY: only while the delay can still refuse a
                // scan (`acquire_delayed` from the next tick on), so a battle with none -- every
                // battle under the old arm, `none` -- hashes as it did before the column, and a
                // delay that has run out hashes like one that never was, which is what it is.
                if e.acquire_delayed(i, self.tick) {
                    h.u32(e.acquirable_from[i]);
                }
                // combat.DASH_ATTACK = client_dash's columns, written under that arm only.
                if self.cfg.calib.dash_attack == DashAttack::ClientDash {
                    h.u32(e.dash_state[i] as u32);
                    h.u32(e.dash_mark[i]);
                    h.vec(e.dash_goal[i]);
                    h.opt_id(e.dash_target[i]);
                    h.bool(e.dash_blocked[i]);
                    h.u32(e.dash_immune_until[i]);
                }
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
                h.vec(e.push_target[i]);
                h.i32(e.push_speed[i]);
                h.bool(e.push_active[i]);
                h.bool(e.jumping[i]);
                h.u32(e.creation_seq[i]);
                h.i32(e.attack_load_ms[i]);
                // PLANT hash_line_unconditional (tests/hash_continuity.rs): a new column
                // hashed for every alive entity at its neutral value, where a hash line
                // must be written only when the value is not neutral. Every battle plays
                // exactly as before and every state_hash VALUE moves. Inside the
                // `!legacy_v3` block, so a migrated format-3 snapshot's self-check is
                // untouched.
                #[cfg(clash_plant = "hash_line_unconditional")]
                h.u32(0);
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
            if !legacy_v3 {
                h.i32(self.lifetime_acc.get(i).copied().unwrap_or(0));
            }
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
            if !legacy_v3 {
                h.bool(p.fresh);
            }
        }
        #[cfg(not(clash_plant = "hash_skips_spells"))]
        if !legacy_v3 {
            h.u32(self.spells.len() as u32);
            for s in &self.spells {
                h.u32(s.team as u32);
                h.u32(s.card as u32);
                h.i32(s.level);
                h.i32(s.damage);
                h.i32(s.pulse);
                match &s.motion {
                    spell::SpellMotion::Pulsing(p) => {
                        h.u32(4);
                        h.vec(p.pos);
                        h.i32(p.life_ms);
                        h.i32(p.next_ms);
                    }
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
            for k in &self.effects.knocks {
                let (id, d, extra) = match *k {
                    spell::Knock::Displacement(id, d) => (id, d, (0, 0)),
                    spell::Knock::Push { id, src, strength, caster } => (id, src, (strength, 1 + caster as i32)),
                };
                h.i32(extra.0);
                h.i32(extra.1);
                h.id(id);
                h.vec(d);
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
                if self.cfg.calib.formation_stagger_wait == StaggerWait::Client16402 {
                    h.i32(s.stagger_ms);
                }
                if s.slide_radius > 0 {
                    h.vec(s.slide_centre);
                    h.i32(s.slide_radius);
                }
                // The flag is set on every queued death spawn whatever the arm, and acted on
                // only under client_8th_frame (`delay_acquisition`): hashed under that arm
                // alone, like `stagger_ms` under its own.
                if self.cfg.calib.spawned_unit_acquire_delay == SpawnedUnitAcquireDelay::Client8thFrame {
                    h.bool(s.acquire_delay);
                }
                // The same for spawner.SPAWNED_FIRST_STEP's flag, acted on only under
                // client16402_same_tick (`materialise_released`).
                if self.cfg.calib.spawned_first_step == SpawnedFirstStep::SameTick {
                    h.bool(s.first_update);
                }
                // spawner.DEATH_SPAWN_LAYOUT = facing_ring_rounded's member heading, set only under that arm.
                if self.cfg.calib.death_spawn_layout == DeathSpawnLayout::FacingRingRounded {
                    h.bool(s.facing.is_some());
                    if let Some(f) = s.facing {
                        h.vec(f);
                    }
                }
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
/// 10: the knockback ladder -- Calib gained knock_law, knock_stacking and
///    max_pushback_length; Entities gained push_target, push_speed and push_active;
///    `EffectBuffer.knocks` became `Vec<spell::Knock>`.
/// 11: the river jump -- Calib gained path_cost_water and jump_water_hop; Entities
///    gained jumping; CardDef gained jump (the card fingerprint moves).
/// 12: the tick order -- Calib gained tick_order and dying_unit_visibility; Entities
///    gained creation_seq; the snapshot gained creation_counter.
/// 13: the 15.535.29 card data -- CardDb takes its rarities from cards.json (a
///    Champion exists, a Rare has 14 levels) and CardDef gained level_base (the card
///    fingerprint moves; every snapshot saved against the 2018 cards.json is stale
///    on the stats alone).
/// 14: the tower ladder and the knockback attack reset -- Calib gained tower_ladder /
///    tower_ladder_cap_level / tower_hp_pct / tower_dmg_pct
///    (combat.TOWER_HITPOINT_LADDER) and the knockback.ATTACK_RESET candidate
///    reset_attack_keep_target; no new Entities column (a format-3 battle keeps its
///    towers' saved hitpoints: the ladder is read at BattleState::new only, and
///    migrate_v3 gives it the old attack reset and the card-ladder regime it ran
///    under).
/// 15: the summon formation -- Calib gained formation_layout / formation_deploy_stagger
///    / formation_ground_y_clamp / lane_id_based_deploy_sequence (formation.*);
///    CardDef gained `formation` (the card fingerprint moves); no new Entities column
///    (the stagger rides the deploy timer, a second summon is its own card). A
///    format-3 battle keeps the engine grid on one tick (migrate_v3).
/// 16: the reach and the attack cycle -- Calib gained attack_range_rule / attack_cycle
///    / charged_hit_timing / projectile_launch (targeting.ATTACK_RANGE_RULE,
///    combat.ATTACK_CYCLE, charge.CHARGED_HIT_TIMING, combat.PROJECTILE_LAUNCH);
///    Entities gained attack_load_ms; Projectile gained fresh; CardDef gained
///    projectile_start_radius and kamikaze (the card fingerprint moves); Calib also
///    gained death_spawn_layout (spawner.DEATH_SPAWN_LAYOUT's facing_ring arm). A
///    format-3 battle keeps the old reach, the old windup, the LoadTime charged hit,
///    the centre-born projectile and the grid death spawn (migrate_v3).
/// 17: the lifetime drain -- BattleState gained lifetime_acc (the drain's remainder in
///    hundredths of a hitpoint, lifetime.HP_DECAY = linear_drain) and Calib gained
///    lifetime_hp_decay / spawner_emission_timing / spawner_spawned_deploy_time
///    (spawner.EMISSION_TIMING, spawner.SPAWNED_DEPLOY_TIME). A format-3 battle
///    resumes with an empty accumulator.
/// 18: status effects -- Calib gained buff_speed_composition / hit_speed_buff /
///    full_stop_buff_is_stun / buff_pulse_amount / buff_pulse_timing /
///    target_buff_on_splash / pulsing_area_effect / stomp_schedule
///    (movement.BUFF_SPEED_COMPOSITION, combat.HIT_SPEED_BUFF, the four status.* keys,
///    spells.PULSING_AREA_EFFECT, movement.STOMP_PAUSE_SCHEDULE); Entities LOST
///    `slow_ms` (a timer nothing read) and gained `buffs` (MAX_BUFFS_PER_ENTITY slots
///    each) and `stomp_clock`; `Projectile` gained buff / pulse; `EffectBuffer` gained
///    `buffs`; `Spell` gained `pulse` and `SpellMotion` gained `Pulsing`; CardDef
///    gained `attack_buff` and `SpellHit` swapped `stun_ms` for `buff` (the card
///    fingerprint moves, and it now covers CardDb's buff table). A format-3 battle
///    keeps the tick-index stomp schedule and the unbuffed attack advance
///    (migrate_v3).
/// 19: the ground summon's deploy point -- Calib gained formation_ground_deploy_point
///    (formation.GROUND_DEPLOY_POINT, the measured one-unit offsets a ground ring's
///    centre carries and a flying one's does not). No new Entities column and no card
///    fingerprint move: it changes where a summon is LAID, not what it is. The field
///    carries a serde default so a format-18 snapshot still deserializes, and a
///    format-3 battle laid every ring on the tap itself (migrate_v3).
/// 20: the death area effect -- CardDef gained `death_area_effect`, the
///    area_effect_objects row a unit's death leaves standing (the Ice Golem's
///    FreezeIceGolemite), so the card fingerprint moves. No new Calib key, no new
///    Entities column and no new snapshot field: the release is an ordinary `Spell`
///    in the list format 4 already saves, born in Reap instead of Spawn. The
///    format-3 fingerprint is untouched: migrate_v3 strips the new field with the
///    rest of the post-format-3 tail, and no card's index moves (a card that was
///    rejected for its area and is now loaded stays where it sat in `cards`).
/// 20, unchanged, the death-spawn slide (spawner.DEATH_SPAWN_PUSHBACK): Calib gained
///    death_spawn_pushback (serde default not_read), Entities gained death_slide_centre /
///    death_slide_radius and PendingSpawn gained slide_centre / slide_radius (serde
///    default no slide, the columns sized on load, all hashed only while a slide runs),
///    so a format-20 blob saved before them still deserializes and hashes as it did.
///    A blob saved by this build carries the new fields at their neutral values, so its
///    BYTES differ from an earlier build's even under the shipped not_read; its
///    state_hash does not. CardDef gained `death_spawn_pushback`, so the card fingerprint
///    moves: a snapshot saved by an earlier build is refused as saved against other card
///    data. migrate_v3 strips the field with the rest of the post-format-3 tail and runs a
///    migrated battle at not_read; no card's index moves.
/// 20, unchanged, the acquire delay (targeting.SPAWNED_UNIT_ACQUIRE_DELAY): Calib gained
///    spawned_unit_acquire_delay (serde default none), Entities gained acquirable_from
///    (serde default 0, sized on load, hashed only while it is in the future) and
///    PendingSpawn gained acquire_delay (serde default false, hashed only under
///    client_8th_frame), so a format-20 blob saved before them still deserializes and hashes
///    as it did. A blob saved by this build carries the new fields, so its BYTES differ from
///    an earlier build's even under the shipped none; its state_hash does not. No card
///    fingerprint move: CardDef is untouched. migrate_v3 runs a migrated battle at none.
///    WITHIN FORMAT 20 the card fingerprint has since moved with no bump, once per
///    change: CardDef gained `unit_name`, `projectile_homing`, `death_spawn_pushback`, `dash`
///    (combat.DASH_ATTACK) and `reflect` (combat.REFLECT_ATTACK), and the buff table's rows
///    gained `attract_pct` (which also made the Tornado loadable). So a format-20 blob saved
///    before any of them is refused as saved against different card data, never run on the
///    new cards; that refusal, not the format number, is what says it is stale.
pub const SNAPSHOT_FORMAT: u32 = 20;

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
    /// Added in SNAPSHOT_FORMAT 17. `default` so the format-3 migration (which
    /// predates it) still deserializes; `load_with` resizes it to the entity table.
    #[serde(default)]
    lifetime_acc: Vec<i32>,
    mana_unit: i64,
    mana_rate: [i64; 2],
    /// The 16.402 path grid's history (`GridSaved`). Added after SNAPSHOT_FORMAT 20. `default`
    /// (None) for a snapshot saved before it, or by a battle whose path model builds no grid:
    /// that battle resumes with a fresh grid, as every load did before this field.
    #[serde(default)]
    grid16402: Option<GridSaved>,
    state_hash: u64,
}

fn fingerprint_debug<T: std::fmt::Debug>(v: &T) -> u64 {
    let mut h = Fnv::new();
    h.bytes(format!("{v:?}").as_bytes());
    h.finish()
}

/// The card fingerprint a snapshot carries: the cards AND the buff table they index.
/// Without the table a snapshot saved against a Freeze of 4000 ms and
/// one saved against a Freeze of 400 ms would agree, because a `BuffApply` prints an
/// index, not the row.
fn cards_fingerprint(cards: &CardDb) -> u64 {
    fingerprint_debug(&(&cards.cards, &cards.buffs))
}

/// MIGRATE A FORMAT-3 SNAPSHOT TO FORMAT 4, in place, as JSON. Returns the format-3
/// card index -> current CardDb index table (applied by `load_with` AFTER the saved
/// hash is reproduced).
///
/// WHY IT EXISTS: a format-3 snapshot is a battle nobody can re-record -- the
/// state was reached by playing, not by construction -- so the loader migrates one
/// rather than refusing it. NOTHING IN THE SUITE NOW EXERCISES THIS PATH: the
/// format-3 fixture that did was retired on 2026-09-21 (tests/stacked_tie.rs says
/// why and what went with it), so the self-check below is the only thing standing
/// between a wrong migration and a battle that runs anyway. Format 4 (spells)
/// changed three things under format 3:
///   1. NEW FIELDS. Filled with their NEUTRAL values -- the values under which format
///      4 runs a format-3 battle exactly as format 3 did: no spells, no effects, no
///      knockback, no resume flags, no deploy overrides; and the three Calib keys a
///      format-3 battle already consumed under another name keep format 3's
///      behaviour (troop projectile speed = the troop speed key; crown rounding =
///      Floor, the pre-registry truncation; stun expiry = one_tick_short, the old
///      Status-phase decrement; since format 12 the tick order = the legacy
///      Move-before-Attack list with the countdown in Upkeep, and dying units seen
///      for the whole pass). Every other new Calib key only affects spells and
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
                // ~~... charge~~ -- format 11 added `jump` after it.
                // ~~... jump~~ -- format 13 added `level_base` after it.
                // ~~... level_base~~ -- format 15 added `formation` after it.
                // ~~... formation~~ -- format 16 added `projectile_start_radius` and `kamikaze` after it.
                // ~~... kamikaze~~ -- format 18 added `attack_buff` after them.
                // ~~... attack_buff~~ -- format 20 added `death_area_effect` after it.
                // `projectile_homing` (combat.POST_KILL_RETARGET_WAIT's condition, also after
                // format 3) is declared between `attack_buff` and `death_area_effect` and was
                // missing from this string, which no card's Debug text could then end with: it
                // is in its declared place now.
                // ~~... death_area_effect~~ -- the death-spawn slide (still format 20) added
                // `death_spawn_pushback` after it.
                // ~~... death_spawn_pushback~~ -- the dash (still format 20) added `dash` after it.
                // ~~... dash~~ -- the reflect (still format 20) added `reflect` after it.
                // That keeps the strip itself working and does NOT make a format-3 blob load:
                // `unit_name`, declared second, is in the head this leaves, and format 3 never
                // printed it, so the rebuilt text cannot match a format-3 fingerprint and every
                // such blob is refused below as saved against different card data.
                let tail = format!(
                    ", ignore_pushback: {}, spell: None, summon_only: false, stop_movement_after_ms: {}, wait_ms: {}, hide: {:?}, spawner: {:?}, death_spawn: {:?}, charge: {:?}, jump: {:?}, level_base: {:?}, formation: {:?}, projectile_start_radius: {}, kamikaze: {}, attack_buff: {:?}, projectile_homing: {}, death_area_effect: {:?}, death_spawn_pushback: {}, dash: {:?}, reflect: {:?} }}",
                    c.ignore_pushback, c.stop_movement_after_ms, c.wait_ms, c.hide, c.spawner, c.death_spawn, c.charge, c.jump, c.level_base, c.formation, c.projectile_start_radius, c.kamikaze, c.attack_buff, c.projectile_homing, c.death_area_effect, c.death_spawn_pushback, c.dash, c.reflect
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
    o.insert("cards_fingerprint".into(), Value::from(cards_fingerprint(cards)));
    // 1. Calib.
    let calib = o.get_mut("calib").and_then(Value::as_object_mut).ok_or_else(|| bad("no calib"))?;
    let troop_speed = calib.get("speed_to_subtiles_per_tick").cloned().ok_or_else(|| bad("no speed_to_subtiles_per_tick"))?;
    let mut shipped = serde_json::to_value(Calib::shipped()).map_err(|e| e.to_string())?;
    let sh = shipped.as_object_mut().expect("Calib serializes to an object");
    sh.insert("projectile_speed_to_subtiles_per_tick".into(), troop_speed);
    sh.insert("crown_rounding".into(), serde_json::to_value(CrownRounding::Floor).map_err(|e| e.to_string())?);
    sh.insert("buff_expiry".into(), serde_json::to_value(BuffExpiry::OneTickShort).map_err(|e| e.to_string())?);
    // FORMAT 12: a format-3 battle ran the earlier tick order (Move before Attack,
    // the deploy countdown in Upkeep) and saw every dying unit for the whole pass;
    // it keeps both, as it keeps the other three (the same rule as crown_rounding).
    sh.insert("tick_order".into(), serde_json::to_value(TickOrder::LegacyMoveBeforeAttack).map_err(|e| e.to_string())?);
    sh.insert("dying_unit_visibility".into(), serde_json::to_value(DyingUnitVisibility::WholeTick).map_err(|e| e.to_string())?);
    // FORMAT 14: a format-3 battle scaled its towers on the card ladder and reset a
    // windup only on a landed push; it keeps both (the same rule).
    sh.insert("tower_ladder".into(), serde_json::to_value(TowerLadder::CommonCardLadder).map_err(|e| e.to_string())?);
    sh.insert("knock_attack_reset".into(), serde_json::to_value(KnockAttackReset::ResetWindupKeepTarget).map_err(|e| e.to_string())?);
    // FORMAT 15: a format-3 battle laid every deploy on the engine grid, on one tick,
    // with no column clamp; it keeps all three (the same rule).
    sh.insert("formation_layout".into(), serde_json::to_value(FormationLayout::EngineGrid).map_err(|e| e.to_string())?);
    sh.insert("formation_deploy_stagger".into(), serde_json::to_value(DeployStagger::None).map_err(|e| e.to_string())?);
    sh.insert("formation_ground_y_clamp".into(), serde_json::to_value(GroundYClamp::None).map_err(|e| e.to_string())?);
    // FORMAT 19: a format-3 battle laid a ground ring on the tap itself, with none of
    // the one-unit offsets the live client gives it; it keeps that (the same rule).
    sh.insert("formation_ground_deploy_point".into(), serde_json::to_value(GroundDeployPoint::None).map_err(|e| e.to_string())?);
    // FORMAT 16: a format-3 battle reached Range + the target's radius, wound up for
    // LoadTime (its charged hits too), and bore every projectile at the attacker's
    // centre stepping at once; it keeps all four (the same rule).
    sh.insert("attack_range_rule".into(), serde_json::to_value(AttackRangeRule::RangePlusTargetRadius).map_err(|e| e.to_string())?);
    sh.insert("attack_cycle".into(), serde_json::to_value(AttackCycle::WindupLoadTime).map_err(|e| e.to_string())?);
    sh.insert("charged_hit_timing".into(), serde_json::to_value(ChargedHitTiming::AfterLoadTimeWindup).map_err(|e| e.to_string())?);
    sh.insert("projectile_launch".into(), serde_json::to_value(ProjectileLaunch::AttackerCentreSameTick).map_err(|e| e.to_string())?);
    sh.insert("death_spawn_layout".into(), serde_json::to_value(DeathSpawnLayout::EngineGridWithinRadius).map_err(|e| e.to_string())?);
    // The death-spawn slide: a format-3 battle laid every death spawn by the layout key and
    // slid none; it keeps that whatever the ledger ships (the same rule).
    sh.insert("death_spawn_pushback".into(), serde_json::to_value(DeathSpawnPushback::NotRead).map_err(|e| e.to_string())?);
    // The acquire delay: a format-3 battle let every death spawn be targeted from the tick
    // after it appeared; it keeps that whatever the ledger ships (the same rule).
    sh.insert("spawned_unit_acquire_delay".into(), serde_json::to_value(SpawnedUnitAcquireDelay::None).map_err(|e| e.to_string())?);
    for (k, val) in sh.iter() {
        calib.entry(k.clone()).or_insert_with(|| val.clone());
    }
    // 1. Entities.
    let ents = o.get_mut("ents").and_then(Value::as_object_mut).ok_or_else(|| bad("no ents"))?;
    let n = ents.get("alive").and_then(Value::as_array).map(Vec::len).ok_or_else(|| bad("no ents.alive"))?;
    let zero = serde_json::to_value(Vec2::default()).map_err(|e| e.to_string())?;
    let zero2 = serde_json::to_value(Vec2::default()).map_err(|e| e.to_string())?;
    let zero3 = serde_json::to_value(Vec2::default()).map_err(|e| e.to_string())?;
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
        // FORMAT 10: no ladder runs (format 3 had no knockback at all).
        ("push_target", zero3),
        ("push_speed", Value::from(0)),
        ("push_active", Value::Bool(false)),
        // FORMAT 11: nobody is mid-leap (format 3 walked every Hog Rider to a bridge).
        ("jumping", Value::Bool(false)),
        // FORMAT 16: no load timer runs (the windup arm never reads it).
        ("attack_load_ms", Value::from(0)),
        // FORMAT 18: nobody carries a buff and no stomp clock has started. Format 3
        // had neither, and both fills are what "not buffed yet" means -- an empty
        // slot has id 0, and the clock is the one a fresh walker starts with. The
        // format-3 hash self-check below proves the state is unchanged (`legacy_v3`
        // hashes neither column and keeps the zero `slow_ms` used to contribute).
        ("stomp_clock", Value::from(0)),
    ] {
        if ents.insert(k.into(), Value::Array(vec![fill; n])).is_some() {
            return Err(bad(&format!("already has ents.{k}")));
        }
    }
    // FORMAT 18: the buff list is MAX_BUFFS_PER_ENTITY slots PER entity in one flat
    // column, so its length is a multiple of the slot count, not the slot count.
    {
        let empty = serde_json::to_value(crate::status::BuffSlot::default()).map_err(|e| e.to_string())?;
        if ents.insert("buffs".into(), Value::Array(vec![empty; n * crate::status::MAX_BUFFS_PER_ENTITY])).is_some() {
            return Err(bad("already has ents.buffs"));
        }
        // ~~`slow_ms`~~ -- the column is gone (format 18); a format-3 snapshot still
        // carries it and serde ignores what no field claims.
        ents.remove("slow_ms");
    }
    {
        // FORMAT 12: the creation order (entity.rs `creation_seq`) -- format 3 never
        // recorded it, so every slot takes its rank under the ordering the sequential
        // move pass used before the counter existed, (spawn_tick, slot), and the
        // counter continues from the slot count. The pass is the only reader, and
        // the format-3 battle ran a frame-planned arm that has no such pass.
        let ticks: Vec<u64> = ents
            .get("spawn_tick")
            .and_then(Value::as_array)
            .ok_or_else(|| bad("no ents.spawn_tick"))?
            .iter()
            .map(|t| t.as_u64().ok_or_else(|| bad("ents.spawn_tick entry")))
            .collect::<Result<_, _>>()?;
        if ticks.len() != n {
            return Err(bad("ents.spawn_tick length"));
        }
        let mut by_creation: Vec<usize> = (0..n).collect();
        by_creation.sort_by_key(|&i| (ticks[i], i));
        let mut seq = vec![Value::from(0u32); n];
        for (rank, &i) in by_creation.iter().enumerate() {
            seq[i] = Value::from(rank as u32);
        }
        if ents.insert("creation_seq".into(), Value::Array(seq)).is_some() {
            return Err(bad("already has ents.creation_seq"));
        }
        if ents.insert("creation_counter".into(), Value::from(n as u32)).is_some() {
            return Err(bad("already has ents.creation_counter"));
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
            cards_fingerprint: cards_fingerprint(&c.cards),
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
            lifetime_acc: self.lifetime_acc.clone(),
            mana_unit: self.mana_unit,
            mana_rate: self.mana_rate,
            grid16402: self.scratch.grid16402.as_ref().map(Grid16402::saved),
            state_hash: self.state_hash(),
        };
        #[cfg(clash_plant = "save_drops_path_grid")]
        let snap = Snapshot { grid16402: None, ..snap }; // PLANT: the path grid's history is lost.
        #[cfg(clash_plant = "save_drops_rng")]
        let snap = Snapshot { rng: Rng::new(0), ..snap };
        #[cfg(clash_plant = "save_drops_knockback")]
        let snap = {
            // PLANT: knockback slides are lost across a save.
            let mut snap = snap;
            snap.ents.knock_ms.iter_mut().for_each(|m| *m = 0);
            snap.ents.knock_rem.iter_mut().for_each(|r| *r = Vec2::default());
            snap.ents.push_active.iter_mut().for_each(|a| *a = false);
            snap.ents.push_speed.iter_mut().for_each(|v| *v = 0);
            snap
        };
        #[cfg(clash_plant = "save_drops_death_slide")]
        let snap = {
            // PLANT: the death-spawn slides are lost across a save.
            let mut snap = snap;
            snap.ents.death_slide_radius.iter_mut().for_each(|r| *r = 0);
            snap.ents.death_slide_centre.iter_mut().for_each(|c| *c = Vec2::default());
            snap
        };
        #[cfg(clash_plant = "save_drops_acquire_delay")]
        let snap = {
            // PLANT: the acquire delays are lost across a save.
            let mut snap = snap;
            snap.ents.acquirable_from.iter_mut().for_each(|t| *t = 0);
            snap
        };
        #[cfg(clash_plant = "save_drops_queued_acquire_delay")]
        let snap = {
            // PLANT: a queued death spawn loses its acquire delay across a save.
            let mut snap = snap;
            snap.spawn_queue.iter_mut().for_each(|p| p.acquire_delay = false);
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
        let (mut snap, remap): (Snapshot, Option<Vec<u16>>) = match format {
            SNAPSHOT_FORMAT => (serde_json::from_slice(bytes).map_err(|e| format!("snapshot: {e}"))?, None),
            3 => {
                let mut v: Value = serde_json::from_slice(bytes).map_err(|e| format!("snapshot: {e}"))?;
                let remap = migrate_v3(&mut v, &cards)?;
                (serde_json::from_value(v).map_err(|e| format!("snapshot (migrated from format 3): {e}"))?, Some(remap))
            }
            other => return Err(format!("snapshot format {other} != engine format {SNAPSHOT_FORMAT}")),
        };
        if remap.is_none() && snap.cards_fingerprint != cards_fingerprint(&cards) {
            return Err("snapshot was saved against different card data".into());
        }
        if snap.arena_fingerprint != fingerprint_debug(&arena) {
            return Err("snapshot was saved against a different arena".into());
        }
        let n = snap.ents.capacity();
        // The two diagnostic vectors are `serde(default)`, so a snapshot saved before they
        // existed arrives with them EMPTY while every other column has `n` entries. The Move
        // pass indexes them on the next tick, so they are sized here rather than at first use.
        // Their content is not restored and does not need to be: they describe the tick just
        // run, and a restored battle has not run one.
        snap.ents.push_applied.resize(n, Vec2::default());
        snap.ents.push_neighbours.resize(n, 0);
        snap.ents.retarget_wait.resize(n, 0);
        snap.ents.target_doomed.resize(n, false);
        snap.ents.fired_at.resize(n, None);
        snap.ents.launched_beyond.resize(n, false);
        snap.ents.stagger_ms.resize(n, 0);
        snap.ents.death_slide_centre.resize(n, Vec2::default());
        snap.ents.death_slide_radius.resize(n, 0);
        snap.ents.acquirable_from.resize(n, 0);
        snap.ents.dash_state.resize(n, DashState::None);
        snap.ents.dash_mark.resize(n, 0);
        snap.ents.dash_goal.resize(n, Vec2::default());
        snap.ents.dash_target.resize(n, None);
        snap.ents.dash_blocked.resize(n, false);
        snap.ents.dash_immune_until.resize(n, 0);
        if snap.lifetime_acc.len() > n
            || snap.lifetime_ms.len() > n
            || snap.ents.card.iter().any(|c| (*c as usize) >= cards.cards.len())
            // A saved spell must run SOME shape under this card data. Asked through
            // spell.rs `shape_of`, the one resolution `cast` and `step_spells` use,
            // so a death's area release -- an ordinary `Spell` under the DYING
            // card's index, which carries no `spell` of its own -- is not read as a
            // corrupt snapshot on the tick it is in the air.
            || snap.spells.iter().any(|s| cards.cards.get(s.card as usize).map_or(true, |c| crate::spell::shape_of(c).is_none()))
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
            released: Vec::new(),
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
            lifetime_acc: snap.lifetime_acc,
            mana_unit: snap.mana_unit,
            mana_rate: snap.mana_rate,
            scratch: Scratch::default(),
            phase_trace: None,
        };
        s.scratch.grid16402 = snap.grid16402.map(|g| Grid16402::restore(&s.cfg.arena, &s.cfg.calib, g));
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
        // The needle spans a line break (the value alone is not unique), so it is
        // matched against a copy with the line endings normalised: .gitattributes
        // checks the ledger out with LF, but a working tree carried over from a CRLF
        // checkout compiles \r\n into the include_str! and a \n needle would then
        // miss -- leaving the edit unmade and the assertion below the only thing
        // between that and a silent pass.
        let json = CALIBRATION_JSON.replace("\r\n", "\n");
        let edited = json.replacen(
            "\"value\": 1000,\n      \"units\": \"native arena units (1 tile = 2 cells)\"",
            "\"value\": 3,\n      \"units\": \"native arena units (1 tile = 2 cells)\"",
            1,
        );
        assert_ne!(edited, json, "edit did not apply; the JSON layout changed");
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
        // past match.DEPLOY_LOCKOUT_TICKS: before it every slot answers TooEarly and none of
        // the reasons this test is about -- Occupied, BadSlot -- is ever reached
        while s.tick < s.cfg.calib.deploy_lockout_ticks.max(0) as u32 {
            s.tick();
        }
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
