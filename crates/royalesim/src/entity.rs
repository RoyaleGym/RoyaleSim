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
    /// Load time running; the hit lands when it completes.
    Windup = 1,
    /// Hit landed; waiting out the rest of hit_speed.
    Cooldown = 2,
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

    pub team: Vec<Team>,
    pub kind: Vec<EntityKind>,
    pub card: Vec<u16>,
    pub level: Vec<i32>,
    pub team_seq: Vec<u32>,
    pub spawn_tick: Vec<u32>,

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
    /// ms elapsed in the current attack phase.
    pub attack_ms: Vec<i32>,

    /// ms of deploy time remaining (0 = active).
    pub deploy_ms: Vec<i32>,
    pub stun_ms: Vec<i32>,
    pub slow_ms: Vec<i32>,
    /// Set when a stun lands (calibration status.STUN_RETARGET_ON_RESUME): on the
    /// first Target phase with stun_ms == 0 the unit rescans ignoring target lock and
    /// keep-target hysteresis, then clears it.
    pub retarget_on_resume: Vec<bool>,
    /// Knockback displacement still to apply, WORLD subtiles (knockback.DURATION_MS > 0
    /// only; an instant knockback never lands here).
    pub knock_rem: Vec<Vec2>,
    /// ms of knockback slide remaining. While > 0 the unit neither walks nor attacks.
    pub knock_ms: Vec<i32>,

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

    /// The free list and per-team counters, for hashing.
    pub fn allocator_state(&self) -> (&[u32], [u32; 2]) {
        (&self.free, self.team_counter)
    }

    /// Allocate a slot. Reuses the most recently freed slot (LIFO), so slot
    /// assignment is a pure function of the spawn/despawn history.
    pub fn spawn(&mut self, s: SpawnInit) -> EntityId {
        let seq = self.team_counter[s.team as usize];
        self.team_counter[s.team as usize] += 1;
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
            self.deploy_ms[i] = s.deploy_ms;
            self.stun_ms[i] = 0;
            self.slow_ms[i] = 0;
            self.retarget_on_resume[i] = false;
            self.knock_rem[i] = Vec2::default();
            self.knock_ms[i] = 0;
            self.move_frac[i] = Vec2::default();
            self.route[i].clear();
            self.route_goal[i] = None;
            self.last_plan_tick[i] = s.spawn_tick;
            self.seg_dir[i] = Vec2::default();
            self.move_ticks[i] = 0;
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
            self.deploy_ms.push(s.deploy_ms);
            self.stun_ms.push(0);
            self.slow_ms.push(0);
            self.retarget_on_resume.push(false);
            self.knock_rem.push(Vec2::default());
            self.knock_ms.push(0);
            self.move_frac.push(Vec2::default());
            self.route.push(Vec::new());
            self.route_goal.push(None);
            self.last_plan_tick.push(s.spawn_tick);
            self.seg_dir.push(Vec2::default());
            self.move_ticks.push(0);
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
