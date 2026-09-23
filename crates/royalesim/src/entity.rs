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
    /// Spirit's projectile and an Electro Wizard's hit all reach one hold.
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
    /// byte-level trace diff against the oracle is trivial. One model runs per
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
    /// oracle corpus cannot say whether `k` also advances while the unit is stunned
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

    /// Held by a knockback: mid-slide under the fixed_distance arm (`knock_ms`), or
    /// mid-ladder under the 16.402 one (`push_active`). The one predicate every
    /// "does not walk, does not attack" site reads, so the two arms cannot part.
    #[inline]
    pub fn knocked(&self, i: usize) -> bool {
        self.knock_ms[i] > 0 || self.push_active[i]
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

    /// Is entity `i` HELD -- stunned, or frozen by a buff whose composed speed is 0?
    /// The one predicate the walk, the attack and the stomp clock read.
    #[inline]
    pub fn held(&self, table: &[BuffDef], i: usize) -> bool {
        self.stun_ms[i] > 0 || self.buffed(table, i, Sel::Speed, 100) == 0
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
            self.spawned_by.push(None);
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
