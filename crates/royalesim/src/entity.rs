//! Entity storage (struct-of-arrays, generational ids, free list) and the
//! uniform-grid spatial hash.
//!
//! WHY SoA
//!     Every hot loop touches two or three fields of every entity (position and
//!     radius for collision; position, team and hp for targeting). Keeping them in
//!     dense parallel arrays keeps those loops in cache.
//!
//! IDS ARE NOT SEMANTICS
//!     Slot index is an accident of spawn order: whichever deploy was processed
//!     first gets the lower index. Nothing in the engine may let it decide an
//!     outcome, or one seat wins every mirror trade for no reason but deploy
//!     order. Where a last-resort tie-break is unavoidable the engine uses
//!     `team_seq` (spawn ordinal within the entity's OWN team), which is equal for
//!     an entity and its mirror twin however the two teams' deploys interleave.
#![allow(unexpected_cfgs)]

use crate::fixed::Vec2;
use crate::status::{BuffDef, BuffSlot, Sel, MAX_BUFFS_PER_ENTITY};
use crate::{EntityId, Team};

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum EntityKind {
    Troop = 0,
    Building = 1,
    KingTower = 2,
    PrincessTower = 3,
}

impl EntityKind {
    #[inline]
    pub fn is_building(self) -> bool {
        !matches!(self, EntityKind::Troop)
    }
    #[inline]
    pub fn is_crown_tower(self) -> bool {
        matches!(self, EntityKind::KingTower | EntityKind::PrincessTower)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum AttackPhase {
    /// Not attacking.
    Idle = 0,
    /// Attacking with the swing under way: under combat.ATTACK_CYCLE =
    /// progress_credit, attacking with progress % HitSpeed >= 50 (the target lock,
    /// the Path phase's hold); under windup_load_time the LoadTime windup running.
    Windup = 1,
    /// Attacking, the hit landed this tick: under progress_credit still attacking
    /// (the unit stands; the next tick's range test gates the next cycle); under
    /// windup_load_time waiting out the rest of hit_speed.
    Cooldown = 2,
}

/// The hide state of a building whose card `hides_when_not_attacking` (Tesla;
/// card.rs `HideDef`; calibration.json `hide.*`). Every other entity is `Up` for
/// its whole life and the machinery never looks at it.
///
/// THE TIMER `Entities::hide_ms` MEANS ONE THING PER STATE: while `Rising` it is
/// the ms of UpTimeMs still to run; while `Up` it is the ms of HideTimeMs still
/// to run before the building goes back under (reset to HideTimeMs whenever the
/// building has a live target -- or, under hide.HIDE_DELAY_MEANING =
/// time_since_last_shot, whenever it fires); while `Hidden` it is 0. Both timers
/// are decremented by TICK_MS in the TARGET phase (state.rs `hide_pass`), once per
/// tick, before any entity decides its target.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum HideState {
    /// Above ground: targets, attacks, can be targeted and damaged.
    Up = 0,
    /// Under ground: no target, no attack, untargetable, immune to damage
    /// (hide.HIDDEN_IMMUNE_TO_DAMAGE), ignores stun and knockback.
    Hidden = 1,
    /// Coming up: no target, no attack; targetable per hide.TARGETABLE_WHILE_RISING;
    /// takes damage.
    Rising = 2,
}

/// Where a unit is in its DASH (calibration combat.DASH_ATTACK = client_dash; card.rs `DashDef`;
/// state.rs `phase_path16402`). `None` on every entity whose card has no dash block, and on every
/// entity under the shipped `none`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum DashState {
    /// Walking, attacking or anything else: no dash under way.
    #[default]
    None = 0,
    /// The stand before the dash: the unit does not move, and enters `Dashing` on the tick
    /// `dash_mark` names.
    Standing = 1,
    /// The dash itself: the unit moves at JumpSpeed toward `dash_goal`, runs no contact scan, is
    /// non-collidable for everyone and does not attack.
    Dashing = 2,
}

/// AN ENCHANT ON A UNIT (card.rs `EnchantDef`; `Entities::enchant`): who gave it, from which card and at which level
/// (the bonus is scaled on that card's ladder at that level, calibration enchant.BONUS_LEVEL_SCALING), the attacks the
/// unit has made since it took it (enchant.BONUS_ATTACKS reads it), and the ms it has left once the giver is gone (-1
/// while the giver lives; enchant.INSTIGATOR_DEATH).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct EnchantSlot {
    pub source: EntityId,
    pub card: u16,
    pub level: i32,
    pub count: u32,
    pub finish_ms: i32,
}

/// Everything needed to materialise one entity.
#[derive(Clone, Copy, Debug)]
pub struct SpawnInit {
    pub team: Team,
    pub kind: EntityKind,
    pub card: u16,
    pub level: i32,
    pub pos: Vec2,
    pub hp: i32,
    pub shield: i32,
    pub damage: i32,
    pub death_damage: i32,
    pub radius: i32,
    pub mass: Option<i32>,
    /// Subtiles per tick.
    pub speed: i32,
    pub flying: bool,
    pub deploy_ms: i32,
    pub spawn_tick: u32,
}

/// What an in-place transformation takes from the new row (`Entities::rebind`), computed as `spawn` would compute it
/// at the entity's level (state.rs `rebind_unit`).
#[derive(Clone, Copy, Debug)]
pub struct RebindInit {
    pub kind: EntityKind,
    pub card: u16,
    pub damage: i32,
    pub death_damage: i32,
    pub radius: i32,
    pub mass: Option<i32>,
    /// Subtiles per tick.
    pub speed: i32,
    pub flying: bool,
    /// The tick of the change: the new row plans its walk from here (`last_plan_tick`, as `spawn` sets it).
    pub tick: u32,
}

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct Entities {
    pub generation: Vec<u32>,
    pub alive: Vec<bool>,
    free: Vec<u32>,
    team_counter: [u32; 2],
    /// The next `creation_seq`: one per battle, never reset, never reused.
    creation_counter: u32,

    pub team: Vec<Team>,
    pub kind: Vec<EntityKind>,
    pub card: Vec<u16>,
    pub level: Vec<i32>,
    pub team_seq: Vec<u32>,
    pub spawn_tick: Vec<u32>,
    /// CREATION ORDER (calibration match.TICK_ORDER = client16402): the ordinal of
    /// this entity among every entity the battle has created, towers included --
    /// the order the live captures show the units moving in (the creation order,
    /// kept on every frame pair of the 16.402 corpus). Monotonic
    /// per battle and never reused, unlike the slot (LIFO free list) and unlike
    /// `spawn_tick` (shared by everything one Spawn phase materialises); distinct
    /// from `team_seq`, which counts within a team and is what the seat-invariant
    /// tie-breaks read. Read by state.rs `phase_path16402` to order the sequential
    /// move pass and by nothing that decides a tie between the two seats.
    pub creation_seq: Vec<u32>,

    pub pos: Vec<Vec2>,
    pub hp: Vec<i32>,
    pub max_hp: Vec<i32>,
    pub shield: Vec<i32>,
    pub damage: Vec<i32>,
    pub death_damage: Vec<i32>,
    pub radius: Vec<i32>,
    pub mass: Vec<Option<i32>>,
    pub speed: Vec<i32>,
    pub flying: Vec<bool>,

    pub target: Vec<Option<EntityId>>,
    pub target_locked: Vec<bool>,
    pub attack_phase: Vec<AttackPhase>,
    /// The attack progress counter (combat.ATTACK_CYCLE = progress_credit: the
    /// counter the captures show running across hits -- 1400, 2800, ... on a
    /// Prince) or the ms elapsed in the current attack phase (windup_load_time).
    pub attack_ms: Vec<i32>,
    /// The load timer (progress_credit only: the captures' second attack counter,
    /// set to LoadTime on every hit and every cycle entry, counting down 50 per
    /// tick to 0 in any state; its remainder is taken off the credit a re-entering
    /// unit gets). 0 for life under windup_load_time. Snapshot format 16
    /// (migrate_v3 fills 0).
    pub attack_load_ms: Vec<i32>,

    /// ms of deploy time remaining (0 = active).
    pub deploy_ms: Vec<i32>,
    /// THE HOLD TIMER. ms of stun / freeze left; while it runs the unit neither
    /// walks, attacks nor advances its stomp clock. It is DERIVED, not applied
    /// directly: every stun the data ships is a buff whose three
    /// multiplier columns are -100, and `apply_effects` sets this from such a buff
    /// under status.FULL_STOP_BUFF_IS_STUN -- so a Zap, a Freeze spell, an Ice
    /// Spirit's projectile and an Electro Wizard's hit all reach one hold. The Ronin's
    /// counter stun (speed -100, hit speed -95) does not set it under the shipped
    /// stun_timer_speed_and_hit_speed_zero: it stops the walk and slows the attack clock.
    pub stun_ms: Vec<i32>,
    /// THE BUFF LIST (status.rs; calibration status.*), `MAX_BUFFS_PER_ENTITY` slots
    /// per entity in one flat column: slot `k` of entity `i` is
    /// `buffs[i * MAX_BUFFS_PER_ENTITY + k]`, and an empty slot has `id == 0`. Read
    /// by `state.rs effective_speed` and `combat.rs attack_step` through
    /// `status::compose`, and by the Status phase for the damage / heal pulses.
    /// It replaced `slow_ms`, a timer nothing read.
    pub buffs: Vec<BuffSlot>,
    /// Set when a stun lands (calibration status.STUN_RETARGET_ON_RESUME): on the
    /// first Target phase with stun_ms == 0 the unit rescans ignoring target lock and
    /// keep-target hysteresis, then clears it.
    pub retarget_on_resume: Vec<bool>,
    /// Ticks left of the post-kill retarget wait (calibration combat.POST_KILL_RETARGET_WAIT):
    /// while > 0 the unit is held as attacking with no target and its attack timer frozen.
    /// `default` so a snapshot saved before it existed still loads; sized to the capacity on load
    /// (state.rs `load_with`), and 0 is also the truth for such a snapshot: nothing waited then.
    #[serde(default)]
    pub retarget_wait: Vec<i16>,
    /// Whether this unit's target was DOOMED when last seen alive: the damage of the homing
    /// projectiles flying at it, from every source, covered its hitpoints. Refreshed in the
    /// Target phase from the projectiles in flight at the tick's start, so after a loss it holds
    /// the victim's last live tick. Read under combat.POST_KILL_RETARGET_WAIT =
    /// client16402_attack_finish. `default` and sized on load like `retarget_wait`.
    #[serde(default)]
    pub target_doomed: Vec<bool>,
    /// The target this unit has LAUNCHED a projectile at since it acquired it, or None: set in the
    /// attack pass when a projectile attacker fires, cleared in the Target phase whenever the target
    /// changes. Read by target.rs `can_target` under targeting.DOOMED_TARGET_DROP =
    /// projectile_attackers (an attacker keeps a doomed target it has already shot at), and written
    /// under that arm only. `default` and sized on load like `target_doomed`.
    #[serde(default)]
    pub fired_at: Vec<Option<EntityId>>,
    /// This unit launched a projectile at its target from beyond its reach on the last tick: set in the attack
    /// pass, read and cleared by the next Target phase, which then rescans (target.rs `decide`). Written under
    /// targeting.LOGIC_PRESERVE_TARGET_IF_HIT_STARTED = "projectile_attackers_only" only, and hashed under it.
    /// `default` and sized on load like `fired_at`.
    #[serde(default)]
    pub launched_beyond: Vec<bool>,
    /// THE LIFE-STATE CONTROLLER (a Goblin Hut; state.rs `life_state_pass`): where it is (0 not started, 1 in its
    /// ActionDelay, 2 sleeping, 3 awake), the ms its clock has left, the enemy its waves aim at (sticky), and the
    /// waves it has released over its whole life (the side rule reads its parity). 0 / None on every other entity,
    /// hashed only for a card that carries the controller. `default` and sized on load like `launched_beyond`.
    #[serde(default)]
    pub life_state: Vec<u8>,
    #[serde(default)]
    pub life_ms: Vec<i32>,
    #[serde(default)]
    pub life_target: Vec<Option<EntityId>>,
    #[serde(default)]
    pub life_n: Vec<u32>,
    /// A SECOND PERIODIC UNIT'S WAVES (card.rs `SpawnerDef::unit2`, the Super Witch's): the waves its spawner has begun,
    /// whose parity picks the next wave's unit (state.rs `spawner_pass`). 0 on every other entity, hashed only for a
    /// spawner that carries a second unit. `default` and sized on load at 0.
    #[serde(default)]
    pub spawn_waves: Vec<u32>,
    /// A CHARMED UNIT'S HOME SIDE (status.rs `BuffDef::switch_team`, the Super Elite Archer's charm): Some while a charm
    /// holds it on the other side (`team` is then the other one), given back when the charm's slot clears (state.rs
    /// `tick_status_timers`). None on every other entity, hashed only when set. `default` and sized on load at None.
    #[serde(default)]
    pub home_team: Vec<Option<crate::Team>>,
    /// A PAIR CLONED IN ITS DEPLOY (spells.CLONE_HOLD_DEPLOY = client15535_covers_deploy_late_walk, state.rs
    /// `materialise_clones`): the first tick it walks and meets the contact law, the tick after its hold's end takes its
    /// targets (the move pass skips it before then, as a freed captive). 0 on every other entity, hashed only while it is
    /// ahead. `default` and sized on load at 0.
    #[serde(default)]
    pub walk_from: Vec<u32>,
    /// INVISIBLE WHEN IDLE (targeting.INVISIBILITY; target.rs `can_target`): the first tick of the window in which
    /// an enemy may target this unit, the tick after its last hit (state.rs, the attack pass); 0 before any hit.
    /// 0 on every other entity, hashed only for a card that carries the idle invisibility. `default` and sized on
    /// load like `launched_beyond`.
    #[serde(default)]
    pub reveal_from: Vec<u32>,
    /// THE ELIXIR PAYOUT TIMER (an Elixir Collector; state.rs `mana_pass`, economy.*): the ms left to the next
    /// payout, loaded with ManaGenerateTimeMs at the deploy end and held at 0 while a payout waits for its owner
    /// to fall below the cap. 0 on every other entity, hashed only for a card that produces. `default` and sized
    /// on load like `reveal_from`.
    #[serde(default)]
    pub mana_ms: Vec<i32>,
    /// AN ATTACHED RIDER (card.rs `AttachDef`; calibration rider.*): the mount this rider rides,
    /// set when state.rs `spawn_riders` creates it, and its offset from the mount's centre in the
    /// mount's facing frame (+y along the facing), subtiles, (0, 0) while the loader takes no
    /// SpawnRadius. None and (0, 0) on every other entity, hashed only on a rider. `default` and
    /// sized on load like `reveal_from`. Read through `attached`.
    #[serde(default)]
    pub attached_to: Vec<Option<EntityId>>,
    #[serde(default)]
    pub attach_offset: Vec<Vec2>,
    /// THE ATTACK SELECTOR (card.rs `AttackSelectDef`; the Three Musketeers): the AttackSequenceList entry chosen for
    /// the swing under way, 0 the row's projectile and 1 its melee entry (state.rs `select_attack`, combat.rs `fire`).
    /// 0 on every other entity, hashed only for a card that carries a selector. `default` and sized on load like
    /// `launched_beyond`.
    #[serde(default)]
    pub attack_seq: Vec<u8>,
    /// THE RUNE GIANT'S LOOK (card.rs `EnchantDef`; state.rs `enchant_pass`, `launch_due_enchants`): where it is (0 not
    /// started, 1 waiting out its ActionDelay or Cooldown, 2 looking, 3 launching), the ms its clock has left, and the
    /// friends its pending launch goes to. 0 / empty on every other entity, hashed only for a card that carries the
    /// enchant. `default` and sized on load like `reveal_from`.
    #[serde(default)]
    pub enchant_state: Vec<u8>,
    #[serde(default)]
    pub enchant_ms: Vec<i32>,
    #[serde(default)]
    pub enchant_picks: Vec<Vec<EntityId>>,
    /// THE ENCHANT THIS UNIT CARRIES (state.rs `apply_effects` puts it on when the projectile lands; combat.rs
    /// `enchant_bonus` reads it on every attack). None on every entity without one, which is every entity of a battle
    /// with no Rune Giant, and hashed only when Some. `default` and sized on load like `reveal_from`.
    #[serde(default)]
    pub enchant: Vec<Option<EnchantSlot>>,
    /// THE COUNTER'S COOLDOWN (card.rs `ParryDef`; calibration parry.*): ms until the counter is ready, 0 = ready.
    /// Counted down with the hold timer (state.rs `tick_status_timers`). 0 on every other entity, hashed only for a
    /// card that carries a counter. `default` and sized on load like `reveal_from`.
    #[serde(default)]
    pub parry_ms: Vec<i32>,
    /// THE IDLE BUFF'S RETURN (status.IDLE_BUFF; card.rs `IdleBuffDef`, combat.rs `idle_on`): the first tick on which
    /// the unit's BuffWhenNotAttacking is on again, written by state.rs `idle_buff_pass` after the unit's hit and on
    /// every tick it keeps attacking after it; 0 before its first hit (the buff is on). 0 on every other entity,
    /// hashed only for a card that carries an idle buff. `default` and sized on load like `reveal_from`.
    #[serde(default)]
    pub idle_back: Vec<u32>,
    /// targeting.CHASE_DROP_RANGE = client_sight_minus_1000: the troop this unit last let go of because it ran past
    /// the chase-drop limit (target.rs `decide`), which the unit's later scans admit only within that limit (`scan`).
    /// None otherwise, cleared when the unit takes that troop again, and None on every unit under the old arm.
    /// `default` and sized on load like `launched_beyond`.
    #[serde(default)]
    pub chase_dropped: Vec<Option<EntityId>>,
    /// targeting.CHASE_DROP_RANGE = client_sight_minus_1000, THE EDGE: the target this unit held WITHIN the chase-drop
    /// limit at its last Target phase, written by that phase for the target each decision leaves it holding (state.rs
    /// `phase_target_for`, target.rs `chase_inside`). `decide` lets a target go past the limit only when this names
    /// it, so a troop taken past the limit is walked after until it has been inside. None when the target stood past
    /// the limit or is not a troop, and None on every unit under the old arm. `default` and sized on load like
    /// `chase_dropped`.
    #[serde(default)]
    pub chase_inside: Vec<Option<EntityId>>,
    /// targeting.CHASE_DROP_WALKING_AWAY = client15535_growing_away: where this unit stood as the last Target phase read
    /// it (state.rs `chase_pass_end`), against which the next one reads its own step and the growth of its distances
    /// (target.rs `walks_away`). Its creation point until a Target phase has read it; written under that arm alone.
    /// `default` and sized on load from each unit's position.
    #[serde(default)]
    pub chase_last_pos: Vec<Vec2>,
    /// targeting.CHASE_RESCAN_PASS_OVER = client15535_receding_lane_walk: whether this unit walked into its last Target
    /// phase and left it holding no target, walking on for its tower (state.rs `chase_pass_end`); its rescans then pass
    /// over every troop past the chase-drop limit whose distance grew since (target.rs `scan_with`). False at creation
    /// and under every other arm. `default` and sized on load at false.
    #[serde(default)]
    pub chase_lane_walk: Vec<bool>,
    /// combat.LOAD_TIMER_TARGET_LOSS = client15535_stands_after_walk_loss: 1 on the tick this unit lost, while walking, the
    /// target it held (state.rs, the Target phase), 2 from its attack step on that tick until it takes a target again,
    /// while its load timer stands (combat.rs `attack_step_progress`); 0 otherwise and on every unit under the old arm.
    /// `default` and sized on load at 0.
    #[serde(default)]
    pub load_hold: Vec<u8>,
    /// combat.RETARGET_WAIT_WHILE_HELD = client16402_first_held_counts: this unit was held (`held`) on its post-kill wait's
    /// previous Target phase (state.rs `phase_target`), so a held phase now pauses the wait; false otherwise and on every
    /// unit under the other arms. `default` and sized on load at false.
    #[serde(default)]
    pub wait_held: Vec<bool>,
    /// movement.JUMP_LANDING_SCOPE = client15535_whole_tick: the tick this unit's river leap last ended, plus one (state.rs
    /// `phase_path16402_for`); the later contact passes of that tick leave it out. 0 otherwise and on every unit under the
    /// old arm. `default` and sized on load at 0.
    #[serde(default)]
    pub landed_at: Vec<u32>,
    /// THE COMBO'S COUNT (card.rs `ComboDef`; combat.ATTACK_COMBO, knockback.COMBO_PUSHBACK): the entry this unit's
    /// next hit deals, moved on after every hit (state.rs `phase_attack`) under either key's new arm, across
    /// targets. 0 on every unit without a combo and under both old arms. `default` and sized on load like
    /// `chase_inside`.
    #[serde(default)]
    pub combo_ix: Vec<u8>,
    /// targeting.FIRST_TOWER_PICK = client_spawn_lane: the lane bits (the tilemap's lane-left or lane-right bit) this
    /// troop took at its creation point (state.rs `spawn_now`, and `summon_lane_flip` for a summon member), and the
    /// last tick on which its default tower comes from that lane (target.rs `default_tower`): u32::MAX while it
    /// deploys, then its deploy-end tick + FIRST_PICK_LANE_WINDOW_MS / TICK_MS (state.rs `on_deployed`). 0 and 0 on
    /// every other entity and on every entity under the old arm. `default` and sized on load like `launched_beyond`.
    #[serde(default)]
    pub spawn_lane: Vec<u8>,
    #[serde(default)]
    pub lane_window_end: Vec<u32>,
    /// What is left of a formation member's DEPLOY_STAGGER wait, ms: set from PendingSpawn.stagger_ms
    /// when the member is created and counted down beside `deploy_ms`, so `deploy_ms - stagger_ms`
    /// stays the unit's own DeployTime. 0 for every unit that never staggered (a spell release and
    /// a death spawn take `deploy_ms` from their own columns, so `deploy_ms > DeployTime` cannot tell
    /// the wait). Read under formation.STAGGER_WAIT = client16402_untargetable_immovable. `default`
    /// and sized on load like `retarget_wait`.
    #[serde(default)]
    pub stagger_ms: Vec<i32>,
    /// THE DEATH-SPAWN SLIDE (calibration spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide;
    /// state.rs `fixed_slide_ring`, `phase_path16402`): the death point a death-spawned
    /// member slides away from, WORLD subtiles, and the radius it stops at (its parent's
    /// DeathSpawnRadius), subtiles. Set from PendingSpawn when the member is created; while
    /// `death_slide_radius > 0` the member neither walks nor attacks nor takes a target, the
    /// Path phase moves it straight out, and the tick it reaches the radius both are zeroed.
    /// (0, 0) / 0 on every unit that is not sliding, which is every unit under the shipped
    /// not_read. `default` and sized on load like `stagger_ms`.
    #[serde(default)]
    pub death_slide_centre: Vec<Vec2>,
    #[serde(default)]
    pub death_slide_radius: Vec<i32>,
    /// THE SLIDE'S LAST TICK (a container's member: state.rs `release_fuse_end`,
    /// `move16402::CONTAINER_SLIDE_TICKS`; a dying troop's member under spawner.DEATH_SLIDE_STOP =
    /// move_count: its move count, state.rs `slide_move_count`): the slide ends after its step on this
    /// tick, whether or not the member has reached its radius (`death_slide_capped`). 0 = no cap (a
    /// dying troop's slide under on_reach), and 0 on every unit that is not sliding. Hashed only while a
    /// slide runs and only when set.
    /// `default` and sized on load like `death_slide_radius`.
    #[serde(default)]
    pub death_slide_until: Vec<u32>,
    /// THE SLIDE'S FIXED END POINT (calibration spawner.DEATH_SLIDE_AIM = fixed_end_point; state.rs
    /// `slide_end_points`, move16402.rs `death_slide_toward`), WORLD subtiles: the death point + the member's ring
    /// direction x DeathSpawnRadius, fixed at birth, which each slide step aims at wherever a push has put the
    /// member. Set from PendingSpawn when the member is created, and only under that arm: (0, 0) under the shipped
    /// current_ray, which never reads it, and on every unit that is not sliding. Cleared with the slide. Hashed
    /// only while a slide runs under fixed_end_point. `default` and sized on load like `death_slide_until`.
    #[serde(default)]
    pub death_slide_end: Vec<Vec2>,
    /// THE DELAYED KAMIKAZE (card.rs `CardDef::kamikaze_time_ms`, calibration combat.KAMIKAZE_TIME;
    /// state.rs `kamikaze_drain`): the tick after the unit's first fire, the first tick of its drain in
    /// the Status phase (the fire's own tick drains in the attack pass). 0 before the fire and on every
    /// other entity; hashed only for a card with a KamikazeTime. `default` and sized on load like
    /// `reveal_from`.
    #[serde(default)]
    pub kamikaze_from: Vec<u32>,
    /// THE ACQUIRE DELAY (calibration targeting.SPAWNED_UNIT_ACQUIRE_DELAY = client_8th_frame;
    /// state.rs `delay_acquisition`, the one setter; target.rs `can_target`, the one reader):
    /// the first tick whose Target phase may give this unit to an enemy as its target. A troop
    /// created by a death spawn is born with its own first tick + `target::ACQUIRE_DELAY_TICKS`
    /// (7), so it is first targeted on its 8th frame. 0 on every other unit, and on every unit
    /// under `none`. A value in the past is inert (`acquire_delayed`), so the
    /// state hash reads it only while it is in the future. `default` and sized on load like
    /// `stagger_ms`.
    #[serde(default)]
    pub acquirable_from: Vec<u32>,
    /// THE DASH (calibration combat.DASH_ATTACK = client_dash; card.rs `DashDef`; state.rs
    /// `phase_path16402`, the one writer). `dash_state` is where the unit is in it. `dash_mark` is
    /// the tick it enters (Standing) or entered (Dashing) the dash state. `dash_goal` is the goal
    /// cell's centre, subtiles, fixed on that entry. `dash_target` is the target the dash rules
    /// refer to, recorded the first time the unit was seen walking after it, and `dash_blocked`
    /// says that target was then nearer than DashMinRange (edge to edge), so it is walked into.
    /// `dash_immune_until`: damage landing on this unit on any tick before it is discarded
    /// (combat.rs `resolve`). All `default` and sized on load like `acquirable_from`, written
    /// under client_dash only, and hashed under it.
    #[serde(default)]
    pub dash_state: Vec<DashState>,
    #[serde(default)]
    pub dash_mark: Vec<u32>,
    #[serde(default)]
    pub dash_goal: Vec<Vec2>,
    #[serde(default)]
    pub dash_target: Vec<Option<EntityId>>,
    #[serde(default)]
    pub dash_blocked: Vec<bool>,
    #[serde(default)]
    pub dash_immune_until: Vec<u32>,
    /// THE SPECIAL'S LOAD (calibration combat.SPECIAL_HOOK = client_hook_drag; card.rs
    /// `SpecialDef`; state.rs `special_step`): ms of SpecialLoadTime this unit still stands
    /// before it throws its special projectile. 0 when it is not loading, which is every
    /// unit under the shipped not_read. `default` and sized on load like `acquirable_from`.
    #[serde(default)]
    pub special_ms: Vec<i32>,
    /// The unit this unit's special is on, from the trigger to the end of the drag: through
    /// the load, the flight of the hook and the drag. While it is Some the unit's ordinary
    /// attack does not run and it stands. None otherwise. `default` and sized on load.
    #[serde(default)]
    pub special_on: Vec<Option<EntityId>>,
    /// The unit whose hook landed on this one and is dragging it (state.rs `step_hook_drags`).
    /// While it is Some this unit neither walks nor attacks (`knocked`) and each Move phase
    /// steps it toward that unit. None otherwise. `default` and sized on load.
    #[serde(default)]
    pub hooked_by: Vec<Option<EntityId>>,
    /// combat.HOOK_RELEASE = client_idle_tick: this unit's drag stopped on the last Move phase and it stays held
    /// (`hooked_by` kept) for the tick after (state.rs `step_hook_drags`). False otherwise and on every unit under the old
    /// arm. `default` and sized on load.
    #[serde(default)]
    pub drag_idle: Vec<bool>,
    /// THE UNDERGROUND WALK (movement.SPAWN_PATHFIND_STATES; state.rs `phase_tunnel`): the DESTINATION of a
    /// unit still under ground, WORLD subtiles, from its birth at its owner's King until the tick it comes up
    /// (the Miner, the Goblin Drill's dig). While Some its deploy timer is frozen, it moves only in
    /// `phase_tunnel`, and under movement.SPAWN_PATHFIND_BODY = untouchable nothing targets, hits, buffs,
    /// pushes or meets it. None on every other unit. `default` and sized on load like `hooked_by`; hashed
    /// only while Some.
    #[serde(default)]
    pub tunnel_dest: Vec<Option<Vec2>>,
    /// THE AIR-TO-GROUND WINDOW (a Vines catch; spell.rs `catch`, calibration spells.AIR_TO_GROUND_WINDOW): ms a caught
    /// flier is still a ground unit for every reader of `in_air` (targeting, a hit's air filter). Counted down with the
    /// hold timer (state.rs `tick_status_timers`). 0 on every other entity, hashed only when positive. `default` and
    /// sized on load like `tunnel_dest`.
    #[serde(default)]
    pub grounded_ms: Vec<i32>,
    /// A COPY THE CLONE MADE (state.rs `materialise_clones`), or a death spawn of one: the Clone never copies it again,
    /// and its death spawns are copies (calibration spells.CLONE_DEATH_SPAWNS). False on every other entity, hashed
    /// only when set. `default` and sized on load like `tunnel_dest`.
    #[serde(default)]
    pub cloned: Vec<bool>,
    /// Knockback displacement still to apply, WORLD subtiles (knockback.DURATION_MS > 0
    /// only; an instant knockback never lands here).
    pub knock_rem: Vec<Vec2>,
    /// DIAGNOSTIC, written by the Move pass and read by nothing the engine decides with:
    /// the contact push applied on the tick just run (after the mean and the 150 cap) and
    /// the number of neighbours that produced it. Both are (0, 0) and 0 on a tick where
    /// nothing overlapped, which is a real answer rather than a missing one. They exist so
    /// a parity trace can draw what the contact law DID beside what the recording shows,
    /// which a position column cannot distinguish from what it wanted.
    /// `default` so a snapshot saved before these existed still loads. They are resized to
    /// the entity capacity on load (state.rs `load_with`), because an empty vector here
    /// would be indexed by the Move pass on the next tick.
    #[serde(default)]
    pub push_applied: Vec<Vec2>,
    #[serde(default)]
    pub push_neighbours: Vec<i32>,
    /// ms of knockback slide remaining. While > 0 the unit neither walks nor attacks.
    pub knock_ms: Vec<i32>,
    /// THE KNOCKBACK LADDER (calibration knockback.DISPLACEMENT_LAW =
    /// client16402; move16402.rs `start_pushback` / `pushback_step`): the target
    /// point (NATIVE units, not subtiles), the speed still to run down (native units
    /// per tick; may be 0 while active -- the tick it goes negative is the 25-unit
    /// back-step) and the active flag. While `push_active` the unit takes the ladder's step
    /// instead of its walk, holds its attack like a `knock_ms` slide and stays
    /// collidable (its separation scan runs while `push_speed > 0`). Zero / false on
    /// every entity under the fixed_distance arm, whose slide lives in `knock_rem`
    /// / `knock_ms` above.
    pub push_target: Vec<Vec2>,
    pub push_speed: Vec<i32>,
    pub push_active: Vec<bool>,
    /// THE RIVER JUMP (calibration movement.JUMP_WATER_HOP = client16402;
    /// jump16402.rs; card.rs `JumpDef`): movement state 5 in the captures. While set the
    /// unit's route is the single landing node, it moves at JumpSpeed toward that
    /// node's centre every Path phase, requests no path, runs no contact scan and is
    /// non-collidable for everyone; it clears itself on the landing tick (the route
    /// is dropped with it and replanned next tick). False on every entity whose card
    /// has no jump block.
    pub jumping: Vec<bool>,
    /// Hide state (Tesla). `Up` for every entity whose card does not hide.
    pub hide: Vec<HideState>,
    /// The hide timer; its meaning per state is on `HideState`.
    pub hide_ms: Vec<i32>,
    /// PERIODIC SPAWNER (card.rs `SpawnerDef`; state.rs `spawner_pass`): ms until
    /// this entity's next emission. Decremented by TICK_MS once per tick in the
    /// SPAWN phase, after the queue drains, only while the entity is past its deploy
    /// time (and, under spawner.STUN_PAUSES_SPAWNER, not stunned). At <= 0 a unit is
    /// queued (it materialises in the NEXT tick's Spawn phase) and the timer is
    /// reloaded: SpawnInterval while the wave has units left, else SpawnPauseTime.
    /// Meaningless (0) on an entity whose card has no spawner.
    pub spawn_ms: Vec<i32>,
    /// Units of the CURRENT wave still to emit (0 between waves; SpawnNumber when
    /// a wave starts). With SpawnInterval 0 the whole wave goes in one pass.
    pub spawn_wave_left: Vec<i32>,
    /// The spawner that emitted this unit, for SpawnLimit (count of live units it
    /// owns). None for a deploy, a spell release and a death spawn.
    pub spawned_by: Vec<Option<EntityId>>,
    /// THE PRODUCING CARD (a `CardDb` index), for DISPLAY: the card whose play put this entity on the board, down
    /// its whole chain -- a Tombstone's Skeletons and its death Skeletons, a Barbarian Hut's Barbarians, the Goblin
    /// Drill's Goblins, the Tri-Wizards' Electro and Ice Wizards, the Barbarian Barrel's Barbarian (state.rs
    /// `spawn_with`, which takes it from `BattleState::spawn_source`). Its own card on a deployed unit and on
    /// anything nothing passed a producer to. Read by the bindings' card label (py.rs `state_json_text`) and by
    /// one rule, spawner.DEATH_SPAWN_ROUTE's container test (state.rs phase_path16402: a building's or a flyer's
    /// release keeps the engine's route); not hashed (fixed at creation, as the card is). `default` and sized on
    /// load with NO_CARD, which reads as its own card (`producer`).
    #[serde(default)]
    pub source: Vec<u16>,
    /// CHARGE (card.rs `ChargeDef`; state.rs `charge_pass`, Move phase, after
    /// separation): the run-up accumulated so far, in the unit calibration
    /// charge.ACCUMULATOR selects (subtiles of locomotion, or ms of moving ticks).
    /// Gains only on ticks the unit WALKED (a knockback slide never counts); a tick
    /// with no walk applies charge.PROGRESS_ON_STOP; zeroed the tick `charged`
    /// becomes true. Meaningless (0) on an entity whose card has no charge block.
    pub charge_progress: Vec<i32>,
    /// The run-up is complete: the unit moves at ChargeSpeedMultiplier percent of
    /// its speed (charge.MULTIPLIER_MEANING) and its next landed hit deals
    /// DamageSpecial. Consumed by that hit (charge.RESET_ON_ATTACK), a stun
    /// (RESET_ON_STUN), a landed knockback (RESET_ON_KNOCKBACK) or a retarget
    /// (RESET_ON_RETARGET); never by merely standing still.
    pub charged: Vec<bool>,

    /// Sub-subtile movement carry, 1/65536 subtile units (see path::advance).
    pub move_frac: Vec<Vec2>,
    /// Planned waypoints. NEXT FIRST for the three pre-2026 models; GOAL FIRST,
    /// popped from the back, for PathModel::Oracle2026 -- which is the layout the
    /// live game publishes (calibration pathfinding.PATH_NODE_ENCODING), so a
    /// byte-level trace diff against the recorded paths is trivial. One model runs per
    /// battle, so the two conventions never share a route.
    pub route: Vec<Vec<Vec2>>,
    /// Goal the route was planned for, in the unit's TEAM FRAME. The pre-2026
    /// models store the target's POSITION; Oracle2026 stores the goal CELL as
    /// (col, row) -- what it replans on (calibration pathfinding.REPLAN_TRIGGERS).
    pub route_goal: Vec<Option<Vec2>>,
    /// Pre-2026 models: the tick the route was planned, for the periodic cadence.
    /// Oracle2026: the FRIENDLY-OCCLUDER EPOCH the route was planned against, so
    /// that a friendly building entering or leaving the world replans on the tick
    /// it happens, with no cadence at all.
    pub last_plan_tick: Vec<u32>,
    /// PathModel::Oracle2026 only: the 1/256 direction of the segment currently
    /// being walked, frozen when the waypoint was assigned, in the TEAM FRAME.
    ///
    /// It is the trace's `path_segment_direction` field, and the waypoint
    /// consumption predicate is a projection on it (calibration
    /// pathfinding.WAYPOINT_ARRIVE_RULE). Zero when no segment is being walked.
    pub seg_dir: Vec<Vec2>,
    /// PathModel::Oracle2026 only: the unit's MOVING-TICK INDEX `k` -- 0 on the
    /// first tick it walks, and never reset (calibration
    /// movement.STOMP_PAUSE_SCHEDULE, which is a function of `k` alone).
    ///
    /// A COUNTER, not `tick - spawn_tick - deploy`: the schedule's phase is pinned
    /// to the unit's own first step, and deriving it from the clock makes it depend
    /// on exactly when the deploy timer expires relative to the Path phase. The
    /// 15.535.29 corpus cannot say whether `k` also advances while the unit is stunned
    /// or attacking (no trace has a stomp card do either mid-walk), so this counts
    /// ticks the unit spends WALKING, which is the reading the name carries.
    pub move_ticks: Vec<u32>,
    /// THE STOMP CLOCK, milliseconds (movement.STOMP_PAUSE_SCHEDULE = ms_clock). It
    /// advances by `tdiv(compose(Speed, 100), 2)` on every tick the unit WALKS -- 50
    /// unbuffed, 65 under Rage -- and carries its remainder when it passes
    /// StopMovementAfterMS + WaitMS, so a buffed unit's pause group drifts. Unbuffed
    /// it equals `(move_ticks + 1) * TICK_MS` reduced mod the period, which is why
    /// the two candidates of that key agree on every unbuffed corpus tick. 0 for
    /// life on a card with no StopMovementAfterMS, and under the `k_plus_1...` arm.
    pub stomp_clock: Vec<i32>,
    /// PATH_SEARCH = client16402 only: the unit's FACING, a length-256
    /// integer vector in NATIVE orientation. Set by the
    /// step from the pre-move heading; read by the avoidance scan as the
    /// look-ahead direction (move16402.rs). A fresh unit faces the enemy: (0, 256)
    /// for Blue, (0, -256) for Red, as the live towers and spawns show.
    pub facing: Vec<Vec2>,
    /// PATH_SEARCH = client16402 only: the avoidance offset, a
    /// multiple of 10 in [-190, 190] between ticks (move16402.rs `Contact`).
    pub avoid_offset: Vec<i32>,
}

/// The facing a unit is born with: toward the enemy along y (live towers and fresh
/// spawns carry (0, 256) on native side 0 and (0, -256) on side 1).
#[inline]
pub fn initial_facing(team: Team) -> Vec2 {
    match team {
        Team::Blue => Vec2::new(0, 256),
        Team::Red => Vec2::new(0, -256),
    }
}

impl Entities {
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.alive.len()
    }

    #[inline]
    pub fn is_alive(&self, id: EntityId) -> bool {
        let i = id.index as usize;
        i < self.alive.len() && self.alive[i] && self.generation[i] == id.generation
    }

    /// Alive, and -- when `struck` (calibration match.TICK_ORDER = client_sequential_strike) -- not
    /// struck down to 0 hp earlier in this tick's target-and-strike pass. A direct strike's damage
    /// lands at once there, and its victim stays in the table until Reap, so every later unit of
    /// the pass reads it as dead through this (state.rs `phase_target_for`) and through target.rs
    /// `can_target`'s hp test. With `struck` false it is `is_alive`.
    #[inline]
    pub fn standing(&self, id: EntityId, struck: bool) -> bool {
        self.is_alive(id) && (!struck || self.hp[id.index as usize] > 0)
    }

    /// Held by a knockback: mid-slide under the fixed_distance arm (`knock_ms`), or
    /// mid-ladder under the 16.402 one (`push_active`). The one predicate every
    /// "does not walk, does not attack" site reads, so the two arms cannot part.
    /// A unit being dragged by a hook (combat.SPECIAL_HOOK, `hooked_by`) is held the same
    /// way: it neither walks nor attacks and keeps its target while the drag runs.
    #[inline]
    pub fn knocked(&self, i: usize) -> bool {
        self.knock_ms[i] > 0 || self.push_active[i] || self.hooked_by[i].is_some()
    }

    /// An ATTACHED RIDER whose mount lives (card.rs `AttachDef`; `attached_to`): it never walks,
    /// is never pushed and pushes nothing, and stands where its mount stood a tick before
    /// (state.rs `carry_riders`). The one predicate the move passes, the contact law and the
    /// rider rules read, so a rider whose mount left the board without dying (no path does that
    /// today; a dying mount takes its riders with it, rider.DIES_WITH_MOUNT) falls back to an
    /// ordinary unit instead of standing frozen. False on every entity that is not a rider.
    #[inline]
    pub fn attached(&self, i: usize) -> bool {
        self.attached_to.get(i).copied().flatten().is_some_and(|m| self.is_alive(m))
    }

    /// Mid death-spawn slide (calibration spawner.DEATH_SPAWN_PUSHBACK = client_ring_slide;
    /// `death_slide_radius`): the member neither walks nor attacks and takes no target until
    /// the slide ends. Read beside `knocked` at the same "does not walk, does not attack"
    /// sites (target.rs `decide`, state.rs `phase_attack` and the three Path arms). False on
    /// every entity under the shipped not_read.
    #[inline]
    pub fn death_sliding(&self, i: usize) -> bool {
        self.death_slide_radius[i] > 0
    }

    /// Has entity `i`'s slide run out of ticks on `tick` (`death_slide_until`: a container's member, or a dying
    /// troop's under spawner.DEATH_SLIDE_STOP = move_count)? Its step on that tick is its last, whether or not it
    /// reached its radius. Measured on client 15.535.29: a container's members move on T + 13 to T + 16 and never
    /// after, the last-created one resting at 1301 of its 1480. False on every slide with no cap (a dying troop's
    /// under on_reach). PLANT container_slide_uncapped (tests/skeleton_barrel.rs): never, so every member slides on
    /// to its radius.
    #[inline]
    pub fn death_slide_capped(&self, i: usize, tick: u32) -> bool {
        #[cfg(clash_plant = "container_slide_uncapped")]
        {
            let _ = (i, tick);
            return false;
        }
        #[allow(unreachable_code)]
        {
            self.death_slide_until[i] != 0 && tick >= self.death_slide_until[i]
        }
    }

    /// Not yet acquirable as a target in the Target phase of `tick` (calibration
    /// targeting.SPAWNED_UNIT_ACQUIRE_DELAY; `acquirable_from`). The one predicate: target.rs
    /// `can_target` and the state hash both read it. False on every entity under the shipped
    /// `none`.
    #[inline]
    pub fn acquire_delayed(&self, i: usize, tick: u32) -> bool {
        self.acquirable_from[i] > tick
    }

    /// Damage landing on entity `i` in the Resolve phase of `tick` is discarded: it is dashing, or
    /// its dash ended within DashImmuneToDamageTime (calibration combat.DASH_ATTACK;
    /// `dash_immune_until`). False on every entity under the shipped `none`.
    #[inline]
    pub fn dash_immune(&self, i: usize, tick: u32) -> bool {
        self.dash_immune_until.get(i).is_some_and(|&u| tick < u)
    }

    /// Entity `i`'s buff slots, empty ones included.
    #[inline]
    pub fn buff_slots(&self, i: usize) -> &[BuffSlot] {
        let a = i * MAX_BUFFS_PER_ENTITY;
        &self.buffs[a..a + MAX_BUFFS_PER_ENTITY]
    }

    #[inline]
    pub fn buff_slots_mut(&mut self, i: usize) -> &mut [BuffSlot] {
        let a = i * MAX_BUFFS_PER_ENTITY;
        &mut self.buffs[a..a + MAX_BUFFS_PER_ENTITY]
    }

    /// Empty every slot of entity `i` (both spawn arms; a reused slot must not
    /// inherit the buffs of whoever stood there before).
    #[inline]
    pub fn clear_buffs(&mut self, i: usize) {
        for slot in self.buff_slots_mut(i) {
            *slot = BuffSlot::default();
        }
    }

    /// The `BuffDef`s entity `i` is carrying, for `status::compose`. `table` is
    /// `CardDb::buffs`.
    #[inline]
    pub fn buffs_of<'a>(&'a self, table: &'a [BuffDef], i: usize) -> impl Iterator<Item = &'a BuffDef> + 'a {
        self.buff_slots(i).iter().filter(|s| !s.is_empty()).filter_map(move |s| table.get(s.id as usize - 1))
    }

    /// `value` put through entity `i`'s buffs on column `sel` (status.rs `compose`).
    #[inline]
    pub fn buffed(&self, table: &[BuffDef], i: usize, sel: Sel, value: i32) -> i32 {
        crate::status::compose(self.buffs_of(table, i), sel, value)
    }

    /// Is entity `i` HELD -- stunned, or frozen by its buffs? The one predicate the walk, the attack and the stomp
    /// clock read. Frozen is status.FULL_STOP_BUFF_IS_STUN's: a composed speed of 0 under stun_timer (and under the
    /// refused buff_only); a composed speed AND hit speed of 0 under stun_timer_speed_and_hit_speed_zero, so a unit
    /// under the Ronin's counter stun (speed -100, hit speed -95) is not held: it stays in the move pass and the
    /// collision set with a step of 0, and its attack clock runs at the composed hit speed. On every -100 / -100 /
    /// -100 row both terms hold, and the hold timer runs anyway.
    #[inline]
    pub fn held(&self, table: &[BuffDef], i: usize, arm: crate::state::FullStopBuff) -> bool {
        if self.stun_ms[i] > 0 {
            return true;
        }
        let stopped = self.buffed(table, i, Sel::Speed, 100) == 0;
        match arm {
            crate::state::FullStopBuff::SpeedAndHitSpeedZero => {
                #[cfg(not(clash_plant = "split_stop_is_stun"))]
                let clock_stopped = self.buffed(table, i, Sel::HitSpeed, 100) == 0;
                // PLANT (regression, tests/parry.rs): the split row holds the unit as a stun would.
                #[cfg(clash_plant = "split_stop_is_stun")]
                let clock_stopped = true;
                stopped && clock_stopped
            }
            crate::state::FullStopBuff::StunTimer | crate::state::FullStopBuff::BuffOnly => stopped,
        }
    }

    /// Is entity `i` IN THE AIR for targeting and for a hit's air filter: a flier, unless a Vines catch holds it to the
    /// ground (`grounded_ms`; calibration spells.AIR_TO_GROUND_WINDOW). The one predicate target.rs `can_target`, spell.rs
    /// `eligible`, combat.rs `splash` and the straight shots, and the area pull read; the walk, the collision and the
    /// flight height keep reading `flying`. `flying` exactly on every entity no catch has grounded.
    #[inline]
    pub fn in_air(&self, i: usize) -> bool {
        #[cfg(not(clash_plant = "grounding_ignored"))]
        let grounded = self.grounded_ms.get(i).is_some_and(|&g| g > 0);
        #[cfg(clash_plant = "grounding_ignored")]
        let grounded = false; // PLANT: a caught flier stays in the air.
        self.flying[i] && !grounded
    }

    /// Is entity `i` travelling UNDER the arena (a Miner's or a Goblin Drill's way to its
    /// tap; `tunnel_dest`)? The one predicate: the Path phase, the untouchable body's readers
    /// and `status_flags` bit 0 all read it.
    #[inline]
    pub fn underground(&self, i: usize) -> bool {
        self.tunnel_dest.get(i).is_some_and(Option::is_some)
    }

    /// THE STATUS BITS of entity `i` that the entity table alone decides, as the protocol's
    /// `status_flags` column reports them (py.rs ENTITY_FIELDS): bit 0 `underground`, bit 2
    /// under ground by its own hide (`HideState::Hidden`: a Tesla with nothing to shoot), bit 9 a
    /// flier a Vines catch holds to the ground (`flying` and not `in_air`: what targeting reads).
    /// Bit 1, invisible to enemies, needs the ledger, the card table and the tick, so the
    /// export adds it from target.rs `invisible_at` (state.rs `view`). Each bit is the
    /// predicate the engine itself acts on, so an observation built from it reads what the
    /// battle does, never a second derivation of it.
    pub fn status_flags(&self, i: usize) -> i32 {
        let mut bits = 0;
        if self.underground(i) {
            bits |= 1;
        }
        if self.hide[i] == HideState::Hidden {
            bits |= 4;
        }
        if self.flying[i] && !self.in_air(i) {
            bits |= 512;
        }
        bits
    }

    #[inline]
    pub fn id_of(&self, index: usize) -> EntityId {
        EntityId { index: index as u32, generation: self.generation[index] }
    }

    /// Live slot indices in ascending order.
    pub fn live_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.alive.iter().enumerate().filter(|(_, a)| **a).map(|(i, _)| i)
    }

    pub fn live_count(&self) -> usize {
        self.alive.iter().filter(|a| **a).count()
    }

    /// The free list, the per-team counters and the creation counter, for hashing.
    pub fn allocator_state(&self) -> (&[u32], [u32; 2], u32) {
        (&self.free, self.team_counter, self.creation_counter)
    }

    /// Allocate a slot. Reuses the most recently freed slot (LIFO), so slot
    /// assignment is a pure function of the spawn/despawn history.
    pub fn spawn(&mut self, s: SpawnInit) -> EntityId {
        let seq = self.team_counter[s.team as usize];
        self.team_counter[s.team as usize] += 1;
        let created = self.creation_counter;
        self.creation_counter += 1;
        let idx = if let Some(i) = self.free.pop() {
            let i = i as usize;
            self.generation[i] = self.generation[i].wrapping_add(1);
            self.alive[i] = true;
            self.team[i] = s.team;
            self.kind[i] = s.kind;
            self.card[i] = s.card;
            self.level[i] = s.level;
            self.team_seq[i] = seq;
            self.spawn_tick[i] = s.spawn_tick;
            self.creation_seq[i] = created;
            self.pos[i] = s.pos;
            self.chase_last_pos[i] = s.pos;
            self.chase_lane_walk[i] = false;
            self.hp[i] = s.hp;
            self.max_hp[i] = s.hp;
            self.shield[i] = s.shield;
            self.damage[i] = s.damage;
            self.death_damage[i] = s.death_damage;
            self.radius[i] = s.radius;
            self.mass[i] = s.mass;
            self.speed[i] = s.speed;
            self.flying[i] = s.flying;
            self.target[i] = None;
            self.target_locked[i] = false;
            self.attack_phase[i] = AttackPhase::Idle;
            self.attack_ms[i] = 0;
            self.attack_load_ms[i] = 0;
            self.deploy_ms[i] = s.deploy_ms;
            self.stun_ms[i] = 0;
            self.clear_buffs(i);
            self.retarget_on_resume[i] = false;
            self.retarget_wait[i] = 0;
            self.target_doomed[i] = false;
            self.fired_at[i] = None;
            self.launched_beyond[i] = false;
            self.life_state[i] = 0;
            self.life_ms[i] = 0;
            self.life_target[i] = None;
            self.life_n[i] = 0;
            self.spawn_waves[i] = 0;
            self.home_team[i] = None;
            self.walk_from[i] = 0;
            self.reveal_from[i] = 0;
            self.mana_ms[i] = 0;
            self.attached_to[i] = None;
            self.attach_offset[i] = Vec2::default();
            self.attack_seq[i] = 0;
            self.enchant_state[i] = 0;
            self.enchant_ms[i] = 0;
            self.enchant_picks[i].clear();
            self.enchant[i] = None;
            self.parry_ms[i] = 0;
            self.idle_back[i] = 0;
            self.chase_dropped[i] = None;
            self.chase_inside[i] = None;
            self.load_hold[i] = 0;
            self.wait_held[i] = false;
            self.landed_at[i] = 0;
            self.combo_ix[i] = 0;
            self.spawn_lane[i] = 0;
            self.lane_window_end[i] = 0;
            self.stagger_ms[i] = 0;
            self.death_slide_centre[i] = Vec2::default();
            self.death_slide_radius[i] = 0;
            self.death_slide_until[i] = 0;
            self.death_slide_end[i] = Vec2::default();
            self.kamikaze_from[i] = 0;
            self.acquirable_from[i] = 0;
            self.dash_state[i] = DashState::None;
            self.dash_mark[i] = 0;
            self.dash_goal[i] = Vec2::default();
            self.dash_target[i] = None;
            self.dash_blocked[i] = false;
            self.dash_immune_until[i] = 0;
            self.special_ms[i] = 0;
            self.special_on[i] = None;
            self.hooked_by[i] = None;
            self.drag_idle[i] = false;
            self.tunnel_dest[i] = None;
            self.grounded_ms[i] = 0;
            self.cloned[i] = false;
            self.knock_rem[i] = Vec2::default();
            self.push_applied[i] = Vec2::default();
            self.push_neighbours[i] = 0;
            self.knock_ms[i] = 0;
            self.push_target[i] = Vec2::default();
            self.push_speed[i] = 0;
            self.push_active[i] = false;
            self.jumping[i] = false;
            self.hide[i] = HideState::Up;
            self.hide_ms[i] = 0;
            self.spawn_ms[i] = 0;
            self.spawn_wave_left[i] = 0;
            self.spawned_by[i] = None;
            self.source[i] = s.card;
            self.charge_progress[i] = 0;
            self.charged[i] = false;
            self.move_frac[i] = Vec2::default();
            self.route[i].clear();
            self.route_goal[i] = None;
            self.last_plan_tick[i] = s.spawn_tick;
            self.seg_dir[i] = Vec2::default();
            self.move_ticks[i] = 0;
            self.stomp_clock[i] = 0;
            self.facing[i] = initial_facing(s.team);
            self.avoid_offset[i] = 0;
            i
        } else {
            self.generation.push(0);
            self.alive.push(true);
            self.team.push(s.team);
            self.kind.push(s.kind);
            self.card.push(s.card);
            self.level.push(s.level);
            self.team_seq.push(seq);
            self.spawn_tick.push(s.spawn_tick);
            self.creation_seq.push(created);
            self.pos.push(s.pos);
            self.chase_last_pos.push(s.pos);
            self.chase_lane_walk.push(false);
            self.hp.push(s.hp);
            self.max_hp.push(s.hp);
            self.shield.push(s.shield);
            self.damage.push(s.damage);
            self.death_damage.push(s.death_damage);
            self.radius.push(s.radius);
            self.mass.push(s.mass);
            self.speed.push(s.speed);
            self.flying.push(s.flying);
            self.target.push(None);
            self.target_locked.push(false);
            self.attack_phase.push(AttackPhase::Idle);
            self.attack_ms.push(0);
            self.attack_load_ms.push(0);
            self.deploy_ms.push(s.deploy_ms);
            self.stun_ms.push(0);
            for _ in 0..MAX_BUFFS_PER_ENTITY {
                self.buffs.push(BuffSlot::default());
            }
            self.retarget_on_resume.push(false);
            self.retarget_wait.push(0);
            self.target_doomed.push(false);
            self.fired_at.push(None);
            self.launched_beyond.push(false);
            self.life_state.push(0);
            self.life_ms.push(0);
            self.life_target.push(None);
            self.life_n.push(0);
            self.reveal_from.push(0);
            self.mana_ms.push(0);
            self.attached_to.push(None);
            self.attach_offset.push(Vec2::default());
            self.attack_seq.push(0);
            self.enchant_state.push(0);
            self.enchant_ms.push(0);
            self.enchant_picks.push(Vec::new());
            self.enchant.push(None);
            self.parry_ms.push(0);
            self.idle_back.push(0);
            self.chase_dropped.push(None);
            self.chase_inside.push(None);
            self.load_hold.push(0);
            self.wait_held.push(false);
            self.landed_at.push(0);
            self.combo_ix.push(0);
            self.spawn_lane.push(0);
            self.lane_window_end.push(0);
            self.stagger_ms.push(0);
            self.death_slide_centre.push(Vec2::default());
            self.death_slide_radius.push(0);
            self.death_slide_until.push(0);
            self.death_slide_end.push(Vec2::default());
            self.kamikaze_from.push(0);
            self.acquirable_from.push(0);
            self.dash_state.push(DashState::None);
            self.dash_mark.push(0);
            self.dash_goal.push(Vec2::default());
            self.dash_target.push(None);
            self.dash_blocked.push(false);
            self.dash_immune_until.push(0);
            self.special_ms.push(0);
            self.special_on.push(None);
            self.hooked_by.push(None);
            self.drag_idle.push(false);
            self.tunnel_dest.push(None);
            self.grounded_ms.push(0);
            self.cloned.push(false);
            self.knock_rem.push(Vec2::default());
            self.push_applied.push(Vec2::default());
            self.push_neighbours.push(0);
            self.knock_ms.push(0);
            self.push_target.push(Vec2::default());
            self.push_speed.push(0);
            self.push_active.push(false);
            self.jumping.push(false);
            self.hide.push(HideState::Up);
            self.hide_ms.push(0);
            self.spawn_ms.push(0);
            self.spawn_wave_left.push(0);
            self.spawn_waves.push(0);
            self.home_team.push(None);
            self.walk_from.push(0);
            self.spawned_by.push(None);
            self.source.push(s.card);
            self.charge_progress.push(0);
            self.charged.push(false);
            self.move_frac.push(Vec2::default());
            self.route.push(Vec::new());
            self.route_goal.push(None);
            self.last_plan_tick.push(s.spawn_tick);
            self.seg_dir.push(Vec2::default());
            self.move_ticks.push(0);
            self.stomp_clock.push(0);
            self.facing.push(initial_facing(s.team));
            self.avoid_offset.push(0);
            self.alive.len() - 1
        };
        self.id_of(idx)
    }

    /// Free a slot. The generation bumps on reuse, so stale ids stay dead.
    pub fn despawn(&mut self, id: EntityId) -> bool {
        if !self.is_alive(id) {
            return false;
        }
        let i = id.index as usize;
        self.alive[i] = false;
        self.target[i] = None;
        self.route[i].clear();
        self.free.push(id.index);
        true
    }

    /// THE IN-PLACE TRANSFORMATION (state.rs `rebind_unit`): entity `i` takes the new row's columns (`RebindInit`) and
    /// stays the same entity. Every column is on one of three lists, and a column added to `Entities` must join one:
    ///   KEPT, what makes it the same entity: generation, alive, team, level, team_seq, spawn_tick, creation_seq, pos,
    ///     hp, max_hp, shield, buffs, stun_ms, retarget_on_resume, spawned_by, source, spawn_lane, lane_window_end, facing,
    ///     acquirable_from, reveal_from, parry_ms (0 on both rows: the loader refuses a transformation into a row with
    ///     a counter, and a row with a counter carries no other action block), idle_back (0 on both rows: the loader
    ///     refuses a transformation into a row with an idle buff, and the Super Knight transforms into nothing), tunnel_dest (None: a units row that
    ///     tunnels is refused), a rider's mount and offset (attached_to, attach_offset), the enchant a Rune Giant gave
    ///     it (enchant), a Vines catch's air-to-ground window (grounded_ms), whether it is a copy the Clone made
    ///     (cloned), and a troop's knockback (knock_rem, knock_ms, the ladder, push_applied, push_neighbours,
    ///     hooked_by);
    ///   THE CALLER'S, because the calibration decides them: the deploy timer (transform.REDEPLOY) and the target and
    ///     attack columns (`reset_attack`, transform.ATTACK_STATE; the attack selector's entry, attack_seq, with them);
    ///   FROM THE NEW ROW (`RebindInit`) or RESET to what `spawn` gives a new entity: the old row's walk (route,
    ///     route_goal, seg_dir, move_frac, move_ticks, last_plan_tick, avoid_offset, stomp_clock), its row-bound
    ///     state (charge, jump, dash, special, hide, spawner, life-state controller, elixir payout, the Rune Giant's
    ///     look, a delayed kamikaze's first fire), a formation member's stagger and a death-spawn slide with its
    ///     cap, and a building's knockback (a building is not
    ///     moved). The loader refuses a target row whose own attached riders, payout or enchant would need its spawn
    ///     or deploy to start them (card.rs, the unit loop).
    pub fn rebind(&mut self, i: usize, r: RebindInit) {
        self.card[i] = r.card;
        self.kind[i] = r.kind;
        self.damage[i] = r.damage;
        self.death_damage[i] = r.death_damage;
        self.radius[i] = r.radius;
        self.mass[i] = r.mass;
        self.speed[i] = r.speed;
        self.flying[i] = r.flying;
        self.route[i].clear();
        self.route_goal[i] = None;
        self.seg_dir[i] = Vec2::default();
        self.move_frac[i] = Vec2::default();
        self.move_ticks[i] = 0;
        self.last_plan_tick[i] = r.tick;
        self.avoid_offset[i] = 0;
        self.stomp_clock[i] = 0;
        self.charge_progress[i] = 0;
        self.charged[i] = false;
        self.jumping[i] = false;
        self.dash_state[i] = DashState::None;
        self.dash_mark[i] = 0;
        self.dash_goal[i] = Vec2::default();
        self.dash_target[i] = None;
        self.dash_blocked[i] = false;
        self.dash_immune_until[i] = 0;
        self.special_ms[i] = 0;
        self.special_on[i] = None;
        self.hide[i] = HideState::Up;
        self.hide_ms[i] = 0;
        self.spawn_ms[i] = 0;
        self.spawn_wave_left[i] = 0;
        self.spawn_waves[i] = 0;
        self.home_team[i] = None;
        self.walk_from[i] = 0;
        self.life_state[i] = 0;
        self.life_ms[i] = 0;
        self.life_target[i] = None;
        self.life_n[i] = 0;
        self.stagger_ms[i] = 0;
        self.death_slide_centre[i] = Vec2::default();
        self.death_slide_radius[i] = 0;
        self.death_slide_until[i] = 0;
        self.death_slide_end[i] = Vec2::default();
        self.kamikaze_from[i] = 0;
        self.mana_ms[i] = 0;
        self.enchant_state[i] = 0;
        self.enchant_ms[i] = 0;
        self.enchant_picks[i].clear();
        if r.kind.is_building() {
            self.knock_rem[i] = Vec2::default();
            self.knock_ms[i] = 0;
            self.push_target[i] = Vec2::default();
            self.push_speed[i] = 0;
            self.push_active[i] = false;
            self.push_applied[i] = Vec2::default();
            self.push_neighbours[i] = 0;
            self.hooked_by[i] = None;
            self.drag_idle[i] = false;
        }
    }

    /// Entity `i`'s target and attack back to a new entity's: no target, no lock, idle, both counters 0, no post-kill
    /// wait and none of the per-target marks (a transformation that resets its target, state.rs `rebind_unit`).
    pub fn reset_attack(&mut self, i: usize) {
        self.target[i] = None;
        self.target_locked[i] = false;
        self.attack_phase[i] = AttackPhase::Idle;
        self.attack_ms[i] = 0;
        self.attack_load_ms[i] = 0;
        self.retarget_wait[i] = 0;
        self.target_doomed[i] = false;
        self.fired_at[i] = None;
        self.launched_beyond[i] = false;
        self.chase_dropped[i] = None;
        self.chase_inside[i] = None;
        self.load_hold[i] = 0;
        self.wait_held[i] = false;
        self.landed_at[i] = 0;
        self.attack_seq[i] = 0;
    }

    /// The card whose play put entity `i` on the board (`source`), its own card where none was recorded.
    pub fn producer(&self, i: usize) -> u16 {
        match self.source.get(i).copied() {
            Some(c) if c != u16::MAX => c,
            _ => self.card[i],
        }
    }

    /// Largest collision radius among live entities (bounds neighbour queries).
    pub fn max_radius(&self) -> i32 {
        self.live_indices().map(|i| self.radius[i]).max().unwrap_or(0)
    }
}

/// Uniform grid over the arena. Buckets are built by scanning slots in
/// ascending index order, and every query sorts its output by index, so what
/// comes out is a pure function of the entity arrays, never of bucket layout.
#[derive(Clone, Debug)]
pub struct SpatialHash {
    bucket: i32,
    cols: i32,
    rows: i32,
    /// CSR layout: bucket b holds items[starts[b]..starts[b+1]].
    starts: Vec<u32>,
    items: Vec<u32>,
    max_radius: i32,
}

impl SpatialHash {
    /// `bucket` is the cell size in subtiles (default 1 tile). Space outside the
    /// arena clamps into the border buckets.
    pub fn new(width: i32, height: i32, bucket: i32) -> Self {
        let bucket = bucket.max(1);
        let cols = (width + bucket - 1) / bucket + 1;
        let rows = (height + bucket - 1) / bucket + 1;
        SpatialHash {
            bucket,
            cols,
            rows,
            starts: vec![0; (cols * rows + 1) as usize],
            items: Vec::new(),
            max_radius: 0,
        }
    }

    #[inline]
    fn cell_coord(&self, v: i32, n: i32) -> i32 {
        (v.div_euclid(self.bucket)).clamp(0, n - 1)
    }

    #[inline]
    fn bucket_of(&self, p: Vec2) -> usize {
        (self.cell_coord(p.y, self.rows) * self.cols + self.cell_coord(p.x, self.cols)) as usize
    }

    /// Rebuild from scratch. O(n + buckets); called whenever positions change.
    pub fn rebuild(&mut self, ents: &Entities) {
        let nb = (self.cols * self.rows) as usize;
        for s in self.starts.iter_mut() {
            *s = 0;
        }
        let mut max_r = 0;
        for i in ents.live_indices() {
            let b = self.bucket_of(ents.pos[i]);
            self.starts[b + 1] += 1;
            max_r = max_r.max(ents.radius[i]);
        }
        for b in 0..nb {
            self.starts[b + 1] += self.starts[b];
        }
        self.items.clear();
        self.items.resize(self.starts[nb] as usize, 0);
        let mut fill: Vec<u32> = self.starts[..nb].to_vec();
        for i in ents.live_indices() {
            let b = self.bucket_of(ents.pos[i]);
            self.items[fill[b] as usize] = i as u32;
            fill[b] += 1;
        }
        self.max_radius = max_r;
    }

    /// Largest radius seen at the last rebuild.
    #[inline]
    pub fn max_radius(&self) -> i32 {
        self.max_radius
    }

    /// Slot indices of live entities whose CENTRE lies within `radius` of `p`,
    /// ascending by index. Appends into `out` after clearing it.
    pub fn neighbours_within(&self, ents: &Entities, p: Vec2, radius: i32, out: &mut Vec<u32>) {
        out.clear();
        let r = radius.max(0);
        let x0 = self.cell_coord(p.x.saturating_sub(r), self.cols);
        let x1 = self.cell_coord(p.x.saturating_add(r), self.cols);
        let y0 = self.cell_coord(p.y.saturating_sub(r), self.rows);
        let y1 = self.cell_coord(p.y.saturating_add(r), self.rows);
        let r2 = (r as i64) * (r as i64);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                let b = (cy * self.cols + cx) as usize;
                for &i in &self.items[self.starts[b] as usize..self.starts[b + 1] as usize] {
                    if ents.pos[i as usize].dist2(p) <= r2 {
                        out.push(i);
                    }
                }
            }
        }
        out.sort_unstable();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixed::tiles;

    fn init(team: Team, x: i32, y: i32) -> SpawnInit {
        SpawnInit {
            team,
            kind: EntityKind::Troop,
            card: 0,
            level: 1,
            pos: Vec2::new(x, y),
            hp: 100,
            shield: 0,
            damage: 1,
            death_damage: 0,
            radius: 9000,
            mass: Some(1),
            speed: 0,
            flying: false,
            deploy_ms: 0,
            spawn_tick: 0,
        }
    }

    #[test]
    fn generational_ids_do_not_resurrect() {
        let mut e = Entities::new();
        let a = e.spawn(init(Team::Blue, 0, 0));
        assert!(e.despawn(a));
        let b = e.spawn(init(Team::Red, 0, 0));
        assert_eq!(a.index, b.index, "slot reused");
        assert!(!e.is_alive(a), "stale handle must stay dead");
        assert!(e.is_alive(b));
        assert_eq!(e.team_seq[b.index as usize], 0, "team_seq counts per team");
    }

    #[test]
    fn hash_matches_brute_force() {
        let mut e = Entities::new();
        let mut rng = crate::Rng::new(99);
        for k in 0..300 {
            let t = if k % 2 == 0 { Team::Blue } else { Team::Red };
            e.spawn(init(t, rng.range(-5000, tiles(18) + 5000), rng.range(-5000, tiles(32) + 5000)));
        }
        let mut h = SpatialHash::new(tiles(18), tiles(32), tiles(1));
        h.rebuild(&e);
        let mut out = Vec::new();
        for _ in 0..200 {
            let p = Vec2::new(rng.range(0, tiles(18)), rng.range(0, tiles(32)));
            let r = rng.range(0, tiles(4));
            h.neighbours_within(&e, p, r, &mut out);
            let brute: Vec<u32> = e
                .live_indices()
                .filter(|&i| e.pos[i].dist2(p) <= (r as i64) * (r as i64))
                .map(|i| i as u32)
                .collect();
            assert_eq!(out, brute);
        }
    }
}
