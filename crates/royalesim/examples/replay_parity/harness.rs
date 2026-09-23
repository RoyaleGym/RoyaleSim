//! The whole-battle replay-parity harness: play a recorded real battle's SCRIPT
//! through the engine and score the engine's entities against the recording's TRUTH,
//! entity by entity, tick by tick.
//!
//! Shared by `examples/replay_parity.rs` (the CLI: one fixture, or every fixture plus
//! the aggregate) and `tests/replay_parity.rs` (the committed sample and the harness
//! invariants) through `#[path]`, so the test runs exactly the code the report runs.
//!
//! THE FIXTURE is tools/make_replay_fixture.py's JSON (its docstring is the schema and
//! the side convention: native side 0 = Blue, native millitiles x 18 = subtiles, no
//! rotation). This module does not read captures.
//!
//! HOW A DEPLOY IS ISSUED, AND WHY THE TIMING MATCHES THE RECORDING. A truth group's
//! tick is the first frame its entities EXIST with the deploy timer running (a truth row
//! whose `state` column is a deploy code, `TRUTH_DEPLOY_STATES`), and the first step
//! follows on spawn + DeployTime / TICK_MS
//! (calibration movement.DEPLOY_TIMING; tests/tick_order.rs). `BattleState::spawn_unit`
//! enqueues a card's units -- the card's own formation, the card's own deploy timer,
//! a per-call level, no hand and no elixir -- and they materialise in the NEXT tick's
//! Spawn phase, where the deploy countdown also runs for the first time (after the
//! move pass, as the captures show: match.TICK_ORDER). So a deploy recorded at tick T is issued
//! when `tick_count() == T - 1`, and after that `tick()` the engine's frame T holds the
//! units with DeployTime - TICK_MS left, exactly as the recording's frame T does.
//! Not `deploy` (a tap: hand, elixir, deploy zones -- none of which the recording
//! carries and all of which the game already enforced) and not `scenario_spawn_now` (ONE
//! entity, no formation, NO deploy timer: it would walk 20 ticks early).
//!
//! MATCHING. A sim entity is reduced to its ROOT card: the card that was deployed to
//! put it there -- itself for a deployed card, its spawner's card for a spawner
//! emission (`spawned_by`), the card the harness deployed on that tick for a SECOND
//! SUMMON (the Goblin Gang's Spear Goblins; card.rs `FormationDef`), the
//! card of the unit that died within the last few ticks nearest to it for a death
//! spawn, the spell for a released unit. The truth carries
//! the root as `card_id` already (a Tombstone's Skeletons carry 27000009). Within one
//! (side, root) both sides are cut into GROUPS -- the entities that first appear on
//! one tick together (a formation, a wave, a death spawn) -- and `pair_groups` pairs a
//! truth group with the sim entities that appeared within `PAIR_WINDOW_TICKS` of it,
//! preferring a sim group of the SAME SIZE, then the nearest in time; members inside a
//! pair are assigned by least total distance so a formation is not crossed. A sim
//! emission the truth has no group for within the window stays unmatched (extra)
//! instead of shifting every later pair of that root. Towers pair by (side, slot).
//!
//! SCORING is integer-only. A UNIT-TICK is one matched pair (or unmatched entity) on
//! one truth frame tick where at least one side has the unit alive. Position error is
//! in NATIVE units (subtiles / 18). The deploy-phase frames (the truth in state 4 or
//! 11, or the sim still deploying: both stationary at the spawn point) are counted
//! apart (`deploy_ticks`) so a "within 250" can be read over the moving frames alone,
//! and so are the frames after the engine declared the battle over
//! (`after_engine_end`: the sim state is frozen there).
#![allow(dead_code)]
#![allow(unexpected_cfgs)]

use royalesim::card::{CardDb, CardKind, SpellShape};
use royalesim::entity::{AttackPhase, EntityKind};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------------------
// the fixture

pub const FORMAT: &str = "replay-fixture-1";

/// Position tolerances, native units. Named so the report's columns are one list.
pub const TOLERANCES_NATIVE: [i32; 3] = [250, 500, 1000];
/// A pair whose position error passes this is the battle's first divergence.
pub const DIVERGENCE_NATIVE: i32 = 1000;
/// The tolerance at which a divergence is said to have STARTED (the cause is read there).
pub const ONSET_NATIVE: i32 = 250;
/// How many ticks back a death is looked for when a death spawn is rooted.
pub const DEATH_SPAWN_LOOKBACK: u32 = 3;
/// How many ticks back a spell cast is looked for when a released unit is rooted.
pub const SPELL_RELEASE_LOOKBACK: u32 = 200;
/// An alive mismatch whose pair had an hp disagreement within this many ticks before
/// it is read as attack-timing (the damage arrived at different times), not death.
pub const HP_HISTORY_TICKS: u32 = 100;
/// An alive mismatch becomes the battle's first divergence once it has lasted this
/// many scored frames (dated from its first): a spawn or a death a frame or two apart
/// is timing inside the recording's own frame gaps (up to 12 ticks), not a different
/// battle. Every mismatched frame still counts in the score.
pub const ALIVE_MISMATCH_MIN_FRAMES: u32 = 3;
/// A sim entity pairs with a truth group only when it first appeared within this many
/// ticks of the group (5 s): wider than the largest spawn-timing gap measured on the
/// corpus (the Battle Ram's death spawn 64 ticks late on the sample: the spawner
/// timing gap) and, with the same-size preference, enough to keep a spawner's waves (2
/// per wave, 70 ticks apart for a Tombstone) from crossing its death spawn (4).
pub const PAIR_WINDOW_TICKS: u32 = 100;
/// The TIGHT walk tolerance, native units: one thousandth of a tile is the truth's own
/// resolution and the engine's subtile -> native floor loses under 1, so a walk that
/// is bit-exact scores 0-1 here; the slowest walker (speed 45: 37 native per tick)
/// moves more than this in one tick, so one tick of lag in the deploy timing, the
/// first step or the charge onset fails every walking frame.
pub const WALK_TIGHT_NATIVE: i32 = 20;
/// Truth behaviour states of a unit that has not started: 4 = deploying, 11 = the
/// summon delay of a staggered formation member (the recording's state codes).
pub const TRUTH_DEPLOY_STATES: [i32; 2] = [4, 11];

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Frame {
    pub blue_native_side: i32,
    pub transform: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Ticks {
    pub first: u32,
    pub last: u32,
    pub frames: u32,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Tower {
    pub slot: usize,
    pub side: i32,
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    pub max_hp: i32,
    pub level: i32,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct CardLevels {
    pub mode: Option<i32>,
    #[serde(default)]
    pub per_card: BTreeMap<String, i32>,
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct Deck {
    #[serde(default)]
    pub deploy_order: Vec<String>,
    #[serde(default)]
    pub recorded: Vec<String>,
    #[serde(default)]
    pub padding: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Deploy {
    /// The spawn tick (the maker's refined estimate; its docstring, DEPLOY POSITION
    /// AND TICK).
    pub tick: u32,
    /// The first frame tick the group was seen on (>= tick when frames were missed).
    #[serde(default)]
    pub first_seen: Option<u32>,
    #[serde(default)]
    pub tick_evidence: Option<String>,
    pub side: i32,
    pub card: Option<String>,
    pub card_id: i64,
    pub kind: String,
    pub level: Option<i32>,
    pub count: i32,
    #[serde(default)]
    pub keys: Vec<i64>,
    pub pos: [i32; 2],
    pub source: String,
    #[serde(default)]
    pub timing: Option<String>,
    #[serde(default)]
    pub first_seen_gap: u32,
    #[serde(default)]
    pub families: Vec<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct TruthEntity {
    pub key: i64,
    pub side: i32,
    pub card_id: i64,
    pub card: Option<String>,
    pub role: String,
    pub unit: Option<String>,
    pub level: i32,
    pub max_hp: i32,
    pub t0: usize,
    pub n: usize,
    pub x: Vec<serde_json::Value>,
    pub y: Vec<serde_json::Value>,
    pub hp: Vec<serde_json::Value>,
    pub target: Vec<serde_json::Value>,
    pub path_n: Vec<serde_json::Value>,
    pub state: Vec<serde_json::Value>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Truth {
    pub ticks: Vec<u32>,
    pub entities: Vec<TruthEntity>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Fixture {
    pub format: String,
    pub capture: String,
    /// FNV-1a 64 of the data/derived/cards.json the maker classified the truth
    /// against (`cards_json_hash`); the report notes a mismatch with the engine's.
    #[serde(default)]
    pub cards_json_fnv1a64: Option<String>,
    pub frame: Frame,
    pub truth_stride: u32,
    pub playable: bool,
    #[serde(default)]
    pub unplayable_reasons: Vec<String>,
    pub ticks: Option<Ticks>,
    #[serde(default)]
    pub towers: Vec<Tower>,
    #[serde(default)]
    pub tower_level: BTreeMap<String, Option<i32>>,
    #[serde(default)]
    pub card_levels: BTreeMap<String, CardLevels>,
    #[serde(default)]
    pub decks: BTreeMap<String, Deck>,
    #[serde(default)]
    pub deploys: Vec<Deploy>,
    pub truth: Option<Truth>,
}

impl Fixture {
    pub fn from_str(s: &str) -> Result<Fixture, String> {
        let f: Fixture = serde_json::from_str(s).map_err(|e| format!("fixture: {e}"))?;
        if f.format != FORMAT {
            return Err(format!("fixture format {} is not {FORMAT}", f.format));
        }
        Ok(f)
    }

    pub fn load(path: &str) -> Result<Fixture, String> {
        let s = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
        Fixture::from_str(&s)
    }

    /// The last tick the harness plays: the truth's last frame tick.
    pub fn last_tick(&self) -> u32 {
        self.truth.as_ref().and_then(|t| t.ticks.last().copied()).unwrap_or(0)
    }
}

/// Decode one RLE column ([value, run, value, run, ...]; a null value is an absent frame).
fn decode_rle(col: &[serde_json::Value], n: usize) -> Result<Vec<Option<i64>>, String> {
    let mut out = Vec::with_capacity(n);
    for pair in col.chunks(2) {
        if pair.len() != 2 {
            return Err("odd RLE column".into());
        }
        let v = if pair[0].is_null() { None } else { Some(pair[0].as_i64().ok_or("RLE value is not an integer")?) };
        let run = pair[1].as_u64().ok_or("RLE run is not an integer")? as usize;
        out.extend(std::iter::repeat(v).take(run));
    }
    if out.len() != n {
        return Err(format!("RLE column decodes to {} frames, entity says {n}", out.len()));
    }
    Ok(out)
}

/// One entity's truth row on one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// Native units.
    pub x: i32,
    pub y: i32,
    pub hp: i32,
    /// The target's generation key, -1 for none.
    pub target: i64,
    pub path_n: i32,
    pub state: i32,
}

/// The truth decoded: frame ticks, and per entity its rows by frame index.
pub struct TruthTable {
    pub ticks: Vec<u32>,
    /// tick -> frame index
    pub index_of: BTreeMap<u32, usize>,
    pub entities: Vec<TruthEntity>,
    /// entity index -> (first frame index, rows)
    pub rows: Vec<(usize, Vec<Option<Row>>)>,
    pub key_to_entity: BTreeMap<i64, usize>,
}

impl TruthTable {
    pub fn decode(t: &Truth) -> Result<TruthTable, String> {
        let mut rows = Vec::with_capacity(t.entities.len());
        let mut key_to_entity = BTreeMap::new();
        for (k, e) in t.entities.iter().enumerate() {
            let x = decode_rle(&e.x, e.n)?;
            let y = decode_rle(&e.y, e.n)?;
            let hp = decode_rle(&e.hp, e.n)?;
            let tg = decode_rle(&e.target, e.n)?;
            let pn = decode_rle(&e.path_n, e.n)?;
            let st = decode_rle(&e.state, e.n)?;
            let mut r = Vec::with_capacity(e.n);
            for i in 0..e.n {
                r.push(match (x[i], y[i], hp[i], tg[i], pn[i], st[i]) {
                    (Some(x), Some(y), Some(hp), Some(tg), Some(pn), Some(st)) => Some(Row { x: x as i32, y: y as i32, hp: hp as i32, target: tg, path_n: pn as i32, state: st as i32 }),
                    _ => None,
                });
            }
            if e.t0 + e.n > t.ticks.len() {
                return Err(format!("entity {} runs past the tick list", e.key));
            }
            rows.push((e.t0, r));
            if key_to_entity.insert(e.key, k).is_some() {
                return Err(format!("duplicate truth key {}", e.key));
            }
        }
        let index_of = t.ticks.iter().enumerate().map(|(i, t)| (*t, i)).collect();
        Ok(TruthTable { ticks: t.ticks.clone(), index_of, entities: t.entities.clone(), rows, key_to_entity })
    }

    /// Keep only the frames with tick < `cut` (the `--prefix` play).
    pub fn truncate(&mut self, cut: u32) {
        let n = self.ticks.iter().take_while(|t| **t < cut).count();
        self.ticks.truncate(n);
        self.index_of.retain(|t, _| *t < cut);
        let mut keep = Vec::new();
        let mut rows = Vec::new();
        for (k, e) in self.entities.iter().enumerate() {
            let (t0, r) = &self.rows[k];
            if *t0 >= n {
                continue;
            }
            let len = r.len().min(n - t0);
            keep.push(e.clone());
            rows.push((*t0, r[..len].to_vec()));
        }
        self.entities = keep;
        self.rows = rows;
        self.key_to_entity = self.entities.iter().enumerate().map(|(k, e)| (e.key, k)).collect();
    }

    /// The row of entity `k` on frame index `fi`, if it was on the board.
    #[inline]
    pub fn row(&self, k: usize, fi: usize) -> Option<Row> {
        let (t0, r) = &self.rows[k];
        if fi < *t0 || fi >= t0 + r.len() {
            return None;
        }
        r[fi - t0]
    }

    /// First frame index the entity is on the board.
    #[inline]
    pub fn first_index(&self, k: usize) -> usize {
        self.rows[k].0
    }
}

// ---------------------------------------------------------------------------
// playability

/// Why a fixture cannot be played, and -- for a card the engine cannot load -- the
/// first tick that card is deployed, before which the battle IS playable (`--prefix`).
#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct Unplayable {
    pub why: String,
    pub cut: Option<u32>,
}

/// The shortest prefix worth scoring, ticks: a cut before this many ticks of truth
/// is not played even under `--prefix`.
pub const MIN_PREFIX_TICKS: u32 = 100;

/// The engine's word on the fixture: every deploy card must resolve in the CardDb, and
/// the fixture maker's own structural reasons stand.
pub fn playability(f: &Fixture, db: &CardDb) -> Result<(), Vec<Unplayable>> {
    let mut why: Vec<Unplayable> = Vec::new();
    for r in &f.unplayable_reasons {
        // the maker's card-loadability reasons are re-derived below with their cut
        // tick; its structural ones (mid-battle start, unknown id, no frames) stand
        let is_card = f.deploys.iter().any(|d| d.card.as_deref().is_some_and(|c| r.starts_with(&format!("{c}: "))));
        if !is_card {
            why.push(Unplayable { why: r.clone(), cut: None });
        }
    }
    let mut seen = BTreeSet::new();
    for d in &f.deploys {
        let Some(name) = d.card.as_deref() else {
            why.push(Unplayable { why: format!("deploy at tick {} has no card name (id {})", d.tick, d.card_id), cut: None });
            continue;
        };
        if !seen.insert(name.to_string()) {
            continue;
        }
        if db.index(name).is_none() {
            let cut = f.deploys.iter().filter(|e| e.card.as_deref() == Some(name)).map(|e| e.tick).min();
            match db.rejected.iter().find(|(n, _)| n == name) {
                Some((_, r)) => why.push(Unplayable { why: format!("{name}: {r}"), cut }),
                None => why.push(Unplayable { why: format!("{name}: not in the engine card set"), cut }),
            }
        }
    }
    if f.truth.is_none() {
        why.push(Unplayable { why: "no truth".into(), cut: None });
    }
    if why.is_empty() {
        Ok(())
    } else {
        why.sort_by(|a, b| a.why.cmp(&b.why));
        why.dedup();
        Err(why)
    }
}

/// The tick before which an unplayable fixture can still be played: the earliest
/// unloadable deploy, when every reason is a card and the prefix is long enough.
pub fn prefix_cut(f: &Fixture, why: &[Unplayable]) -> Option<u32> {
    if why.iter().any(|u| u.cut.is_none()) {
        return None;
    }
    let cut = why.iter().filter_map(|u| u.cut).min()?;
    let first = f.truth.as_ref().and_then(|t| t.ticks.first().copied())?;
    (cut >= first + MIN_PREFIX_TICKS).then_some(cut)
}

/// The card census the fixture maker reads: what the engine's loader accepts.
#[derive(Serialize)]
pub struct Census {
    /// Where the CardDb came from (the `CardSource` variant name).
    pub cards_source: String,
    /// FNV-1a 64 of data/derived/cards.json as read from disk (`cards_json_hash`), so
    /// a census built against another cards.json is told apart by the maker.
    pub cards_json_fnv1a64: Option<String>,
    pub loadable: Vec<String>,
    pub rejected: BTreeMap<String, String>,
    pub summon_only: Vec<String>,
}

pub fn census(db: &CardDb) -> Census {
    let mut loadable = Vec::new();
    let mut summon_only = Vec::new();
    for (i, c) in db.cards.iter().enumerate() {
        if db.index(&c.name) != Some(i as u16) {
            continue;
        }
        if c.summon_only {
            summon_only.push(c.name.clone());
        } else {
            loadable.push(c.name.clone());
        }
    }
    Census {
        cards_source: format!("{:?}", db.source),
        cards_json_fnv1a64: cards_json_hash().ok(),
        loadable,
        rejected: db.rejected.iter().cloned().collect(),
        summon_only,
    }
}

/// FNV-1a 64 (the same function tools/make_replay_fixture.py `fnv1a64` computes), as
/// 16 hex digits. Chosen because both sides can compute it without a dependency.
pub fn fnv1a64(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// The hash of the repo's data/derived/cards.json as it is on disk.
pub fn cards_json_hash() -> Result<String, String> {
    let path = format!("{}/data/derived/cards.json", repo_root());
    let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    Ok(fnv1a64(&bytes))
}

// ---------------------------------------------------------------------------
// the replay

fn team_of(side: i32) -> Team {
    if side == 0 {
        Team::Blue
    } else {
        Team::Red
    }
}

fn side_of(team: Team) -> i32 {
    match team {
        Team::Blue => 0,
        Team::Red => 1,
    }
}

#[inline]
pub fn to_native(p: Vec2) -> (i32, i32) {
    (p.x / SUBTILE_PER_MILLITILE, p.y / SUBTILE_PER_MILLITILE)
}

#[inline]
pub fn from_native(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * SUBTILE_PER_MILLITILE, y * SUBTILE_PER_MILLITILE)
}

/// Distance between a sim position and a truth position, NATIVE units (floor).
#[inline]
pub fn native_dist(sim: Vec2, truth: (i32, i32)) -> i32 {
    let d2 = sim.dist2(from_native(truth.0, truth.1));
    (isqrt(d2) / SUBTILE_PER_MILLITILE as i64) as i32
}

#[derive(Serialize, Clone, Debug)]
pub struct DeployIssue {
    pub tick: u32,
    pub side: i32,
    pub card: String,
    /// `tick_count()` when spawn_unit was called.
    pub issued_at: u32,
    pub result: Result<(), String>,
}

/// A sim entity the harness saw, reduced to what matching needs.
#[derive(Clone, Debug)]
pub struct SimEntity {
    pub id: EntityId,
    pub team: Team,
    pub card: String,
    /// The root card (module doc), or the tower name.
    pub root: String,
    pub root_how: &'static str,
    pub first_tick: u32,
    pub first_pos: Vec2,
    pub team_seq: u32,
    pub tower_slot: Option<usize>,
}

#[derive(Serialize, Clone, Debug)]
pub struct Pair {
    pub truth_key: i64,
    pub side: i32,
    pub root: String,
    pub sim_index: u32,
    pub sim_generation: u32,
    pub sim_card: String,
    pub root_how: String,
    pub truth_first_tick: u32,
    pub sim_first_tick: u32,
}

/// Integer score counters. Every fraction the report prints is a ratio of two of these.
#[derive(Serialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct Score {
    /// both_alive + alive_mismatch + missing_in_sim + extra_in_sim.
    pub unit_ticks: u64,
    pub both_alive: u64,
    pub within: [u64; 3],
    pub hp_exact: u64,
    /// Sum of |hp delta| over both-alive unit-ticks.
    pub hp_abs_delta: u64,
    pub target_match: u64,
    pub path_n_match: u64,
    /// Matched pairs on ticks where exactly one side has the unit alive.
    pub alive_mismatch: u64,
    /// Truth entities with no sim counterpart, per alive tick.
    pub missing_in_sim: u64,
    /// Sim entities with no truth counterpart, per alive tick.
    pub extra_in_sim: u64,
    /// Both-alive unit-ticks of an ISOLATED WALK: the truth walking (state 1) at a
    /// tower or at nothing with its hp still full -- a unit nobody has touched yet,
    /// so the position error is the walk law's alone (spawn point, path, step,
    /// charge, stomp). The sample test pins a floor on it.
    pub walk_ticks: u64,
    pub walk_within: [u64; 3],
    /// Isolated-walk unit-ticks within `WALK_TIGHT_NATIVE`: the bit-exact walks.
    pub walk_tight: u64,
    /// Both-alive unit-ticks on which the truth was still deploying
    /// (`TRUTH_DEPLOY_STATES`) or the sim was: both stationary at the spawn point,
    /// so a position match there says nothing about movement.
    pub deploy_ticks: u64,
    pub deploy_within: [u64; 3],
    /// Unit-ticks on frames after the engine declared the battle over (its state is
    /// frozen from there; the recording goes on).
    pub after_engine_end: u64,
}

impl Score {
    pub fn add(&mut self, o: &Score) {
        self.unit_ticks += o.unit_ticks;
        self.both_alive += o.both_alive;
        for k in 0..3 {
            self.within[k] += o.within[k];
            self.walk_within[k] += o.walk_within[k];
            self.deploy_within[k] += o.deploy_within[k];
        }
        self.hp_exact += o.hp_exact;
        self.hp_abs_delta += o.hp_abs_delta;
        self.target_match += o.target_match;
        self.path_n_match += o.path_n_match;
        self.alive_mismatch += o.alive_mismatch;
        self.missing_in_sim += o.missing_in_sim;
        self.extra_in_sim += o.extra_in_sim;
        self.walk_ticks += o.walk_ticks;
        self.walk_tight += o.walk_tight;
        self.deploy_ticks += o.deploy_ticks;
        self.after_engine_end += o.after_engine_end;
    }

    /// Integer permille of `num` over `den` (0 when den is 0).
    pub fn permille(num: u64, den: u64) -> u64 {
        (num * 1000).checked_div(den).unwrap_or(0)
    }

    /// Unit-ticks that are not deploy-phase frames.
    pub fn moving_ticks(&self) -> u64 {
        self.unit_ticks - self.deploy_ticks
    }

    /// Both-alive unit-ticks within tolerance `k`, the deploy-phase frames left out.
    pub fn moving_within(&self, k: usize) -> u64 {
        self.within[k] - self.deploy_within[k]
    }

    pub fn consistent(&self) -> Result<(), String> {
        if self.within[0] > self.within[1] || self.within[1] > self.within[2] || self.within[2] > self.both_alive {
            return Err(format!("within {:?} not nested under both_alive {}", self.within, self.both_alive));
        }
        if self.walk_within[0] > self.walk_within[1] || self.walk_within[1] > self.walk_within[2] || self.walk_within[2] > self.walk_ticks || self.walk_ticks > self.both_alive {
            return Err(format!("walk_within {:?} not nested under walk_ticks {} <= both_alive {}", self.walk_within, self.walk_ticks, self.both_alive));
        }
        if self.walk_tight > self.walk_within[0] {
            return Err(format!("walk_tight {} exceeds walk_within[0] {}", self.walk_tight, self.walk_within[0]));
        }
        if self.deploy_within[0] > self.deploy_within[1] || self.deploy_within[1] > self.deploy_within[2] || self.deploy_within[2] > self.deploy_ticks || self.deploy_ticks > self.both_alive {
            return Err(format!("deploy_within {:?} not nested under deploy_ticks {} <= both_alive {}", self.deploy_within, self.deploy_ticks, self.both_alive));
        }
        for k in 0..3 {
            if self.deploy_within[k] > self.within[k] {
                return Err(format!("deploy_within[{k}] exceeds within[{k}]"));
            }
        }
        if self.hp_exact > self.both_alive || self.target_match > self.both_alive || self.path_n_match > self.both_alive {
            return Err("hp_exact / target_match / path_n_match exceed both_alive".into());
        }
        if self.unit_ticks != self.both_alive + self.alive_mismatch + self.missing_in_sim + self.extra_in_sim {
            return Err("unit_ticks is not the sum of its parts".into());
        }
        if self.after_engine_end > self.unit_ticks {
            return Err("after_engine_end exceeds unit_ticks".into());
        }
        Ok(())
    }
}

/// The first divergence of a battle and the harness's reading of its cause.
#[derive(Serialize, Clone, Debug)]
pub struct Divergence {
    pub tick: u32,
    pub truth_key: i64,
    pub side: i32,
    pub card: String,
    pub families: Vec<String>,
    pub what: String,
    /// walking / contact / spawn / death / attack-timing / knockback / status
    pub cause: String,
    /// The tick the pair's position error first passed ONSET_NATIVE on the run that
    /// ended in this divergence (the cause is read there).
    pub onset_tick: u32,
    pub detail: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Report {
    pub fixture: String,
    pub capture: String,
    pub playable: bool,
    pub unplayable_reasons: Vec<String>,
    /// Some(tick): only the truth before this tick was played (`Options::prefix`).
    pub prefix_until: Option<u32>,
    pub last_tick: u32,
    pub engine_end_tick: Option<u32>,
    pub deploys: Vec<DeployIssue>,
    pub pairs: Vec<Pair>,
    /// Entities on each side, matched or not.
    pub truth_entities: usize,
    pub sim_entities: usize,
    pub unmatched_truth: Vec<(i64, String)>,
    pub unmatched_sim: Vec<(u32, u32, String)>,
    pub score: Score,
    /// `score` without the six crown towers' rows (the towers stand still and are
    /// on every frame, so they carry most unit-ticks).
    pub score_no_towers: Score,
    pub per_card: BTreeMap<String, Score>,
    pub first_divergence: Option<Divergence>,
    pub card_families: BTreeMap<String, Vec<String>>,
    /// Per-card level deviations from the side mode (the engine takes the per-call
    /// level, so none is lost; listed so the report can say what it played).
    pub level_deviations: Vec<String>,
    /// The fixture's cards.json hash and the engine's; `notes` says when they differ.
    pub cards_json_fixture: Option<String>,
    pub cards_json_engine: Option<String>,
    pub notes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trace: Vec<TraceRow>,
}

pub struct Options {
    pub seed: u64,
    /// Score every N-th truth frame (1 = all).
    pub stride: usize,
    /// Keep a per-pair per-tick trace in the report (large; for reading one battle).
    pub trace: bool,
    /// Play an unplayable fixture up to its first unloadable deploy (`prefix_cut`).
    pub prefix: bool,
    /// Run the corpus under a CANDIDATE of movement.ATTACKING_UNIT_MOVEMENT rather than
    /// the shipped value. The corpus is the instrument that can judge that key, because
    /// the defect it is about is a position error over many ticks -- and judging it this
    /// way never edits the ledger that every other session's engine reads.
    pub attacking_movement: Option<royalesim::state::AttackingUnitMovement>,
}

impl Default for Options {
    fn default() -> Self {
        Options { seed: 0, stride: 1, trace: false, prefix: false, attacking_movement: None }
    }
}

/// One matched pair on one frame tick, for `Options::trace`.
#[derive(Serialize, Clone, Debug)]
pub struct TraceRow {
    pub tick: u32,
    pub key: i64,
    pub card: String,
    /// truth x, y, hp, state, path_n, target key
    pub truth: Option<[i64; 6]>,
    /// sim x, y (native), hp, attacking, path_n, target sim index (-1 none)
    pub sim: Option<[i64; 6]>,
    /// THE CONTACT STEP the engine took on this tick: applied push dx, dy in native units
    /// (after the mean and the 150 cap) and the number of neighbours that produced it.
    ///
    /// ITS OWN FIELD RATHER THAN THREE MORE SLOTS ON `sim`, because (0, 0, 0) is a REAL
    /// value on most ticks and absence has to look different from it. A reader tells them
    /// apart by whether the row carries `push` at all, never by reading a zero -- the same
    /// distinction that already bit this format once, where a max_hp of 0 means "not in
    /// this source" and an hp bar is drawn only when 0 <= hp < max_hp.
    ///
    /// The recording cannot have this: it is the engine saying what it did, against the
    /// recording's observed positions, which is the other half of the comparison.
    pub push: Option<[i64; 3]>,
    /// The unit's COLLISION RADIUS in native units, from the engine.
    ///
    /// Here because a reader that draws contact needs it and the recording does not carry
    /// it: viser's contact ring recomputes overlaps from positions and radii, and on a
    /// parity trace every unit arrived with radius 0, so the ring was EMPTY on every tick
    /// of the one source it exists for. Against the neighbour count that reads as "ring 0,
    /// engine 1" wherever the engine saw anything -- a stream of false findings in exactly
    /// the place the instrument was pointed. Per unit rather than per card, because a
    /// summoned unit has its own radius and the row's `card` is the root card that produced
    /// it.
    pub radius: Option<i32>,
    pub dist: Option<i32>,
}

/// Which unit card (summon_only) each spawning card puts out, for rooting.
struct Roots {
    death_spawn_of: BTreeMap<u16, Vec<u16>>,
    spell_release_of: BTreeMap<u16, Vec<u16>>,
    /// SummonCharacterSecond units (the Goblin Gang's Spear Goblins, the Rascals'
    /// Girls): rooted to the card the harness itself deployed on that tick
    /// (card.rs `FormationDef::second_summon`).
    second_summon_of: BTreeMap<u16, Vec<u16>>,
}

impl Roots {
    fn new(db: &CardDb) -> Roots {
        let mut death_spawn_of: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        let mut spell_release_of: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        let mut second_summon_of: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        for (i, c) in db.cards.iter().enumerate() {
            if let Some(d) = &c.death_spawn {
                death_spawn_of.entry(d.unit).or_default().push(i as u16);
            }
            if let Some(sp) = &c.spell {
                if let SpellShape::Projectile { spawn: Some(s), .. } = &sp.shape {
                    spell_release_of.entry(s.unit).or_default().push(i as u16);
                }
            }
            if let Some(d) = &c.formation.second_summon {
                second_summon_of.entry(d.unit).or_default().push(i as u16);
            }
        }
        Roots { death_spawn_of, spell_release_of, second_summon_of }
    }
}

/// Build the engine config a fixture asks for.
pub fn config_for(f: &Fixture, db: CardDb) -> Result<(BattleConfig, Vec<String>), String> {
    config_for_with(f, db, None)
}

/// As `config_for`, with one calibration candidate overridden for this run.
pub fn config_for_with(
    f: &Fixture,
    db: CardDb,
    attacking_movement: Option<royalesim::state::AttackingUnitMovement>,
) -> Result<(BattleConfig, Vec<String>), String> {
    let mut cfg = BattleConfig::with_cards(db);
    if let Some(arm) = attacking_movement {
        cfg.calib.attacking_unit_movement = arm;
    }
    let mut notes = Vec::new();
    for side in 0..2 {
        let key = side.to_string();
        let lv = f.card_levels.get(&key).and_then(|c| c.mode);
        let tl = f.tower_level.get(&key).copied().flatten();
        let level = lv.or(tl).ok_or_else(|| format!("side {side}: no card level and no tower level recorded"))?;
        cfg.card_level[side] = level;
        cfg.tower_level[side] = tl.unwrap_or(level);
        if let Some(cl) = f.card_levels.get(&key) {
            for (card, l) in &cl.per_card {
                if *l != level {
                    notes.push(format!("side {side} {card} at level {l} (side mode {level})"));
                }
            }
        }
        let deck = f.decks.get(&key).cloned().unwrap_or_default();
        let mut names: Vec<String> = Vec::new();
        for n in deck.deploy_order.iter().chain(deck.padding.iter()) {
            if names.len() >= 8 {
                break;
            }
            if cfg.cards.index(n).is_some() && !names.contains(n) {
                names.push(n.clone());
            } else if !names.contains(n) {
                notes.push(format!("side {side}: deck card {n} not loadable, left out of the engine deck"));
            }
        }
        cfg.decks[side] = names;
    }
    cfg.shuffle_decks = false;
    Ok((cfg, notes))
}

/// Play the fixture and score it.
pub fn replay(f: &Fixture, db: &CardDb, register: &BTreeMap<String, Vec<String>>, opts: &Options) -> Result<Report, String> {
    let fixture_name = f.capture.split(".native.oracle").next().unwrap_or(&f.capture).to_string();
    let mut report = Report {
        fixture: fixture_name,
        capture: f.capture.clone(),
        playable: true,
        unplayable_reasons: Vec::new(),
        prefix_until: None,
        last_tick: f.last_tick(),
        engine_end_tick: None,
        deploys: Vec::new(),
        pairs: Vec::new(),
        truth_entities: 0,
        sim_entities: 0,
        unmatched_truth: Vec::new(),
        unmatched_sim: Vec::new(),
        score: Score::default(),
        score_no_towers: Score::default(),
        per_card: BTreeMap::new(),
        first_divergence: None,
        card_families: BTreeMap::new(),
        level_deviations: Vec::new(),
        cards_json_fixture: f.cards_json_fnv1a64.clone(),
        cards_json_engine: cards_json_hash().ok(),
        notes: Vec::new(),
        trace: Vec::new(),
    };
    if let (Some(a), Some(b)) = (&report.cards_json_fixture, &report.cards_json_engine) {
        if a != b {
            report.notes.push(format!("fixture classified against cards.json {a}, the engine loaded {b}: rerun tools/make_replay_fixture.py"));
        }
    }
    let mut cut: Option<u32> = None;
    if let Err(why) = playability(f, db) {
        report.unplayable_reasons = why.iter().map(|u| u.why.clone()).collect();
        cut = if opts.prefix { prefix_cut(f, &why) } else { None };
        if cut.is_none() {
            report.playable = false;
            return Ok(report);
        }
    }
    let mut truth = TruthTable::decode(f.truth.as_ref().ok_or("no truth")?)?;
    if let Some(c) = cut {
        truth.truncate(c);
        report.prefix_until = Some(c);
        report.last_tick = truth.ticks.last().copied().unwrap_or(0);
    }
    let (cfg, notes) = config_for_with(f, db.clone(), opts.attacking_movement)?;
    report.level_deviations = notes;
    let roots = Roots::new(db);
    let mut s = BattleState::try_new(opts.seed, cfg)?;
    // tower hp as recorded on the first frame
    for t in &f.towers {
        let team = team_of(t.side);
        // A FIXTURE MUST NOT BE ABLE TO PANIC THE HARNESS. This indexed straight into the
        // tower table and a fixture numbering its six towers 0..5 GLOBALLY, rather than
        // 0..2 per side, took it out of bounds -- "index out of bounds: the len is 3 but
        // the index is 3", which says nothing about slots, sides or the fixture that
        // caused it. The harness reads files it did not write; a bad one earns a
        // diagnosis, not a stack trace.
        let slots = s.tower_hp(team);
        if t.slot >= slots.len() {
            return Err(format!(
                "fixture tower slot {} is out of range for side {}: this engine has {} crown towers per side, numbered 0..{}. SLOTS ARE PER SIDE, not global -- six towers numbered 0..5 across both sides lands exactly here.",
                t.slot,
                t.side,
                slots.len(),
                slots.len() - 1
            ));
        }
        let have = slots[t.slot];
        if have != t.hp {
            s.scenario_set_tower_hp(team, t.slot, t.hp)?;
        }
    }

    // -- sim entity registry, filled as entities appear
    let mut sim: Vec<SimEntity> = Vec::new();
    let mut sim_index_of: BTreeMap<(u32, u32), usize> = BTreeMap::new();
    let mut recent_deaths: Vec<(u32, Team, u16, Vec2)> = Vec::new();
    let mut spell_casts: Vec<(u32, Team, u16)> = Vec::new();
    // (tick the units exist from, team, card) of every troop deploy the harness issued
    let mut deploys_issued: Vec<(u32, Team, u16)> = Vec::new();
    let mut seen_alive: BTreeSet<(u32, u32)> = BTreeSet::new();
    let mut deploys_by_tick: BTreeMap<u32, Vec<&Deploy>> = BTreeMap::new();
    for d in f.deploys.iter().filter(|d| cut.map_or(true, |c| d.tick < c)) {
        deploys_by_tick.entry(d.tick).or_default().push(d);
    }
    let last_tick = report.last_tick;

    // -- register what is on the board now (towers at tick 0)
    let register_new = |s: &BattleState, tick: u32, sim: &mut Vec<SimEntity>, sim_index_of: &mut BTreeMap<(u32, u32), usize>, seen_alive: &mut BTreeSet<(u32, u32)>, recent_deaths: &[(u32, Team, u16, Vec2)], spell_casts: &[(u32, Team, u16)], deploys_issued: &[(u32, Team, u16)]| {
        let mut new: Vec<_> = s.entities().filter(|e| !seen_alive.contains(&(e.id.index, e.id.generation))).collect();
        new.sort_by_key(|e| (e.team as u8, e.team_seq, e.id.index));
        for e in new {
            seen_alive.insert((e.id.index, e.id.generation));
            let card = db.get(e.card_idx);
            let mut tower_slot = None;
            let (root, how): (String, &'static str) = if matches!(e.kind, EntityKind::KingTower | EntityKind::PrincessTower) {
                let ids = s.tower_ids(e.team);
                tower_slot = ids.iter().position(|t| *t == Some(e.id));
                (e.card.to_string(), "tower")
            } else if !card.summon_only {
                (e.card.to_string(), "deployed")
            } else if let Some(cidx) = roots.second_summon_of.get(&e.card_idx).and_then(|parents| {
                // a second summon: the card the harness deployed for this team on this
                // very tick (its members exist from the same Spawn phase)
                deploys_issued.iter().rev().find(|(t, team, c)| *t == tick && *team == e.team && parents.contains(c)).map(|(_, _, c)| *c)
            }) {
                (db.get(cidx).name.clone(), "second-summon")
            } else if let Some(owner) = e.spawned_by {
                match sim_index_of.get(&(owner.index, owner.generation)) {
                    Some(&k) => (sim[k].root.clone(), "spawner"),
                    None => (e.card.to_string(), "spawner-unknown"),
                }
            } else {
                // a death spawn: the nearest recent death of a card that death-spawns this unit
                let parents = roots.death_spawn_of.get(&e.card_idx);
                let mut best: Option<(i64, u16)> = None;
                if let Some(parents) = parents {
                    for (t, team, cidx, pos) in recent_deaths.iter().rev() {
                        if *team != e.team || tick.saturating_sub(*t) > DEATH_SPAWN_LOOKBACK || !parents.contains(cidx) {
                            continue;
                        }
                        let d2 = pos.dist2(e.pos);
                        if best.map_or(true, |b| d2 < b.0) {
                            best = Some((d2, *cidx));
                        }
                    }
                }
                if let Some((_, cidx)) = best {
                    (db.get(cidx).name.clone(), "death-spawn")
                } else {
                    let spells = roots.spell_release_of.get(&e.card_idx);
                    let cast = spells.and_then(|sp| spell_casts.iter().rev().find(|(t, team, cidx)| *team == e.team && tick.saturating_sub(*t) <= SPELL_RELEASE_LOOKBACK && sp.contains(cidx)));
                    match cast {
                        Some((_, _, cidx)) => (db.get(*cidx).name.clone(), "spell-release"),
                        None => (e.card.to_string(), "unrooted"),
                    }
                }
            };
            sim_index_of.insert((e.id.index, e.id.generation), sim.len());
            sim.push(SimEntity { id: e.id, team: e.team, card: e.card.to_string(), root, root_how: how, first_tick: tick, first_pos: e.pos, team_seq: e.team_seq, tower_slot });
        }
    };
    register_new(&s, 0, &mut sim, &mut sim_index_of, &mut seen_alive, &recent_deaths, &spell_casts, &deploys_issued);

    // -- alive-by-tick bookkeeping for the sim: (sim index -> (tick -> snapshot))
    // Snapshots are kept only on truth frame ticks (every `stride`-th), which is all the
    // scorer reads; the position on the tick before a frame is kept for the onset read.
    let frame_ticks: BTreeSet<u32> = truth.ticks.iter().copied().step_by(opts.stride.max(1)).collect();
    let mut snaps: BTreeMap<u32, BTreeMap<usize, Snap>> = BTreeMap::new();
    let snapshot = |s: &BattleState, sim_index_of: &BTreeMap<(u32, u32), usize>| -> BTreeMap<usize, Snap> {
        s.entities()
            .filter_map(|e| {
                sim_index_of.get(&(e.id.index, e.id.generation)).map(|k| {
                    (
                        *k,
                        Snap {
                            pos: e.pos,
                            hp: e.hp,
                            target: e.target,
                            path_n: e.route.len() as i32,
                            deploying: e.deploying,
                            attacking: e.attack_phase != AttackPhase::Idle,
                            pushed: e.push_active || e.knock_ms > 0,
                            stunned: e.stun_ms > 0,
                            jumping: e.jumping,
                            radius: e.radius,
                            push: e.push_applied,
                            push_neighbours: e.push_neighbours,
                        },
                    )
                })
            })
            .collect()
    };
    if frame_ticks.contains(&0) {
        snaps.insert(0, snapshot(&s, &sim_index_of));
    }

    // -- play
    let mut tick = 0u32;
    while tick < last_tick {
        // deploys recorded at tick + 1 are issued now (module doc)
        #[cfg(not(clash_plant = "replay_deploys_one_tick_late"))]
        let due = tick + 1;
        // PLANT (regression): issue a deploy on its recorded tick instead of the tick
        // before, so the engine's units exist one tick after the recording's.
        #[cfg(clash_plant = "replay_deploys_one_tick_late")]
        let due = tick;
        if let Some(list) = deploys_by_tick.get(&due) {
            for d in list {
                let team = team_of(d.side);
                let name = d.card.clone().unwrap_or_default();
                let pos = from_native(d.pos[0], d.pos[1]);
                let r = s.spawn_unit(team, &name, pos, d.level);
                if r.is_ok() {
                    if let Some(idx) = db.index(&name) {
                        if db.get(idx).kind == CardKind::Spell {
                            spell_casts.push((tick + 1, team, idx));
                        } else {
                            deploys_issued.push((tick + 1, team, idx));
                        }
                    }
                }
                report.deploys.push(DeployIssue { tick: d.tick, side: d.side, card: name, issued_at: s.tick_count(), result: r.map_err(|e| format!("{e:?}")) });
            }
        }
        let alive_before: Vec<(EntityId, Team, u16, Vec2)> = s.entities().map(|e| (e.id, e.team, e.card_idx, e.pos)).collect();
        if s.is_done() {
            if report.engine_end_tick.is_none() {
                report.engine_end_tick = Some(tick);
            }
            // the engine refuses to tick a finished battle; the clock still advances for scoring
            tick += 1;
            if frame_ticks.contains(&tick) {
                snaps.insert(tick, snapshot(&s, &sim_index_of));
            }
            continue;
        }
        s.tick();
        tick += 1;
        debug_assert_eq!(s.tick_count(), tick);
        for (id, team, cidx, pos) in alive_before {
            if s.entity(id).is_none() {
                recent_deaths.push((tick, team, cidx, pos));
            }
        }
        recent_deaths.retain(|(t, ..)| tick - *t <= DEATH_SPAWN_LOOKBACK);
        register_new(&s, tick, &mut sim, &mut sim_index_of, &mut seen_alive, &recent_deaths, &spell_casts, &deploys_issued);
        if frame_ticks.contains(&tick) {
            snaps.insert(tick, snapshot(&s, &sim_index_of));
        }
    }
    if s.is_done() && report.engine_end_tick.is_none() {
        report.engine_end_tick = Some(tick);
    }

    // -- matching
    // truth entities by (side, root)
    let mut truth_groups: BTreeMap<(i32, String), Vec<usize>> = BTreeMap::new();
    for (k, e) in truth.entities.iter().enumerate() {
        if e.role == "unknown_object" {
            // an object cards.json does not derive from the card (the maker's
            // docstring): the engine has nothing to pair it with; it scores as missing
            continue;
        }
        let root = e.card.clone().unwrap_or_else(|| format!("id{}", e.card_id));
        truth_groups.entry((e.side, root)).or_default().push(k);
    }
    for v in truth_groups.values_mut() {
        v.sort_by_key(|&k| (truth.first_index(k), truth.entities[k].key));
    }
    let mut sim_groups: BTreeMap<(i32, String), Vec<usize>> = BTreeMap::new();
    for (k, e) in sim.iter().enumerate() {
        sim_groups.entry((side_of(e.team), e.root.clone())).or_default().push(k);
    }
    for v in sim_groups.values_mut() {
        v.sort_by_key(|&k| (sim[k].first_tick, sim[k].team_seq, sim[k].id.index));
    }
    let mut matched_sim: BTreeSet<usize> = BTreeSet::new();
    let mut truth_to_sim: BTreeMap<usize, usize> = BTreeMap::new();
    for ((side, root), tks) in &truth_groups {
        let sks = sim_groups.get(&(*side, root.clone())).cloned().unwrap_or_default();
        if root == "KingTower" || root == "PrincessTower" {
            // towers by (side, slot): slot from the fixture's tower list by key
            for &tk in tks {
                let key = truth.entities[tk].key;
                let slot = f.towers.iter().find(|t| t.side == *side && truth_tower_key(f, &truth, t) == Some(key)).map(|t| t.slot);
                if let Some(slot) = slot {
                    if let Some(&sk) = sks.iter().find(|&&sk| sim[sk].tower_slot == Some(slot) && !matched_sim.contains(&sk)) {
                        matched_sim.insert(sk);
                        truth_to_sim.insert(tk, sk);
                    }
                }
            }
            continue;
        }
        // same-tick groups on both sides, paired inside the window (module doc)
        let mut truth_groups_of: Vec<(u32, Vec<usize>)> = Vec::new();
        for &tk in tks {
            let t = truth.ticks[truth.first_index(tk)];
            match truth_groups_of.last_mut() {
                Some((lt, v)) if *lt == t => v.push(tk),
                _ => truth_groups_of.push((t, vec![tk])),
            }
        }
        let mut sim_pools: Vec<(u32, Vec<usize>)> = Vec::new();
        for &sk in &sks {
            let t = sim[sk].first_tick;
            match sim_pools.last_mut() {
                Some((lt, v)) if *lt == t => v.push(sk),
                _ => sim_pools.push((t, vec![sk])),
            }
        }
        let tg: Vec<(u32, Vec<Option<Vec2>>)> = truth_groups_of.iter().map(|(t, v)| (*t, v.iter().map(|&tk| truth.row(tk, truth.first_index(tk)).map(|r| from_native(r.x, r.y))).collect())).collect();
        let sp: Vec<(u32, Vec<Vec2>)> = sim_pools.iter().map(|(t, v)| (*t, v.iter().map(|&sk| sim[sk].first_pos).collect())).collect();
        for (gi, mi, pi, si) in pair_groups(&tg, &sp, PAIR_WINDOW_TICKS) {
            let (tk, sk) = (truth_groups_of[gi].1[mi], sim_pools[pi].1[si]);
            if matched_sim.contains(&sk) {
                continue;
            }
            matched_sim.insert(sk);
            truth_to_sim.insert(tk, sk);
        }
    }
    for (tk, sk) in &truth_to_sim {
        let e = &truth.entities[*tk];
        report.pairs.push(Pair {
            truth_key: e.key,
            side: e.side,
            root: e.card.clone().unwrap_or_default(),
            sim_index: sim[*sk].id.index,
            sim_generation: sim[*sk].id.generation,
            sim_card: sim[*sk].card.clone(),
            root_how: sim[*sk].root_how.to_string(),
            truth_first_tick: truth.ticks[truth.first_index(*tk)],
            sim_first_tick: sim[*sk].first_tick,
        });
    }
    report.pairs.sort_by_key(|p| p.truth_key);
    report.truth_entities = truth.entities.len();
    report.sim_entities = sim.len();
    let sim_to_truth: BTreeMap<usize, usize> = truth_to_sim.iter().map(|(t, s)| (*s, *t)).collect();
    for (k, e) in truth.entities.iter().enumerate() {
        if !truth_to_sim.contains_key(&k) {
            report.unmatched_truth.push((e.key, e.card.clone().unwrap_or_default()));
        }
    }
    for (k, e) in sim.iter().enumerate() {
        if !matched_sim.contains(&k) {
            report.unmatched_sim.push((e.id.index, e.id.generation, e.root.clone()));
        }
    }

    // -- scoring
    let families_of = |card: &str| register.get(card).cloned().unwrap_or_default();
    let mut per_card: BTreeMap<String, Score> = BTreeMap::new();
    let mut total = Score::default();
    let mut divergence: Option<Divergence> = None;
    let mut pair_state: BTreeMap<usize, PairState> = BTreeMap::new();
    // unmatched truth / sim entities: (first tick, frames) of their alive run
    let mut missing_run: BTreeMap<usize, (u32, u32)> = BTreeMap::new();
    let mut extra_run: BTreeMap<usize, (u32, u32)> = BTreeMap::new();
    let is_tower_key = |key: i64| f.towers.iter().any(|t| truth_tower_key(f, &truth, t) == Some(key));
    for (fi, &t) in truth.ticks.iter().enumerate().step_by(opts.stride.max(1)) {
        let Some(snap) = snaps.get(&t) else { continue };
        let after_end = report.engine_end_tick.is_some_and(|e| t > e) as u64;
        // matched pairs
        for (tk, sk) in &truth_to_sim {
            let e = &truth.entities[*tk];
            let root = e.card.clone().unwrap_or_default();
            let truth_row = truth.row(*tk, fi);
            let sim_row = snap.get(sk).copied();
            let sc = per_card.entry(root.clone()).or_default();
            let st = pair_state.entry(*tk).or_default();
            if truth_row.is_some() || sim_row.is_some() {
                sc.after_engine_end += after_end;
            }
            if opts.trace && (truth_row.is_some() || sim_row.is_some()) {
                let tr = truth_row.map(|r| [r.x as i64, r.y as i64, r.hp as i64, r.state as i64, r.path_n as i64, r.target]);
                let sr = sim_row.map(|r| {
                    let (x, y) = to_native(r.pos);
                    let tg = r.target.and_then(|id| sim_index_of.get(&(id.index, id.generation))).map(|k| *k as i64).unwrap_or(-1);
                    [x as i64, y as i64, r.hp as i64, r.attacking as i64, r.path_n as i64, tg]
                });
                let dist = match (truth_row, sim_row) {
                    (Some(a), Some(b)) => Some(native_dist(b.pos, (a.x, a.y))),
                    _ => None,
                };
                let push = sim_row.map(|r| [r.push.x as i64, r.push.y as i64, r.push_neighbours as i64]);
                let radius = sim_row.map(|r| r.radius);
                report.trace.push(TraceRow { tick: t, key: e.key, card: root.clone(), truth: tr, sim: sr, push, radius, dist });
            }
            match (truth_row, sim_row) {
                (Some(tr), Some(sr)) => {
                    sc.unit_ticks += 1;
                    sc.both_alive += 1;
                    let d = native_dist(sr.pos, (tr.x, tr.y));
                    let walking = tr.state == 1 && (tr.target < 0 || is_tower_key(tr.target)) && tr.hp == e.max_hp;
                    let deploying = TRUTH_DEPLOY_STATES.contains(&tr.state) || sr.deploying;
                    if walking {
                        sc.walk_ticks += 1;
                        if d <= WALK_TIGHT_NATIVE {
                            sc.walk_tight += 1;
                        }
                    }
                    if deploying {
                        sc.deploy_ticks += 1;
                    }
                    for (k, tol) in TOLERANCES_NATIVE.iter().enumerate() {
                        if d <= *tol {
                            sc.within[k] += 1;
                            if walking {
                                sc.walk_within[k] += 1;
                            }
                            if deploying {
                                sc.deploy_within[k] += 1;
                            }
                        }
                    }
                    if sr.hp == tr.hp {
                        sc.hp_exact += 1;
                    } else {
                        st.2 = Some(t);
                    }
                    sc.hp_abs_delta += (sr.hp - tr.hp).unsigned_abs() as u64;
                    // target by matched key
                    let truth_target_sim: Option<Option<EntityId>> = if tr.target < 0 {
                        Some(None)
                    } else {
                        truth.key_to_entity.get(&tr.target).and_then(|tk2| truth_to_sim.get(tk2)).map(|sk2| Some(sim[*sk2].id))
                    };
                    if truth_target_sim == Some(sr.target) {
                        sc.target_match += 1;
                    }
                    if sr.path_n == tr.path_n {
                        sc.path_n_match += 1;
                    }
                    st.3 = None;
                    if d > ONSET_NATIVE {
                        if st.1.is_none() {
                            st.1 = Some(t);
                        }
                    } else {
                        st.1 = None;
                    }
                    if d > DIVERGENCE_NATIVE && divergence.is_none() {
                        let onset = st.1.unwrap_or(t);
                        let onset_snap = snaps.get(&onset).and_then(|m| m.get(sk).copied()).unwrap_or(sr);
                        let onset_truth = truth.index_of.get(&onset).and_then(|ofi| truth.row(*tk, *ofi)).unwrap_or(tr);
                        let cause = read_cause(&onset_snap, &onset_truth, &truth, *tk, truth.index_of.get(&onset).copied().unwrap_or(fi), &snap_neighbours(snap, *sk), st.0);
                        divergence = Some(Divergence {
                            tick: t,
                            truth_key: e.key,
                            side: e.side,
                            card: root.clone(),
                            families: families_of(&root),
                            what: format!("position error {d} native (> {DIVERGENCE_NATIVE}) on {} {}", e.role, e.unit.as_deref().unwrap_or("?")),
                            cause: cause.0.to_string(),
                            onset_tick: onset,
                            detail: cause.1,
                        });
                    }
                    st.0 = Some(sr.hp - tr.hp);
                }
                (Some(_), None) | (None, Some(_)) => {
                    sc.unit_ticks += 1;
                    sc.alive_mismatch += 1;
                    let run = match st.3 {
                        Some((t0, n)) => (t0, n + 1),
                        None => (t, 1),
                    };
                    st.3 = Some(run);
                    if divergence.is_none() && run.1 >= ALIVE_MISMATCH_MIN_FRAMES {
                        let hp_recent = st.2.is_some_and(|h| t.saturating_sub(h) <= HP_HISTORY_TICKS);
                        let (what, cause) = match (truth_row, sim_row) {
                            (Some(_), None) => ("alive in the truth, gone or not yet spawned in the sim", if hp_recent { "attack-timing" } else { "death" }),
                            _ => ("alive in the sim, gone in the truth", if hp_recent { "attack-timing" } else { "death" }),
                        };
                        // a pair whose sim member has not appeared yet is a spawn-timing gap
                        let cause = if sim_row.is_none() && sim[*sk].first_tick > run.0 { "spawn" } else { cause };
                        divergence = Some(Divergence {
                            tick: run.0,
                            truth_key: e.key,
                            side: e.side,
                            card: root.clone(),
                            families: families_of(&root),
                            what: format!("{what} (for {} frames by tick {t})", run.1),
                            cause: cause.to_string(),
                            onset_tick: run.0,
                            detail: format!("last both-alive hp delta {:?}, last hp disagreement at {:?}", st.0, st.2),
                        });
                    }
                }
                (None, None) => {}
            }
        }
        // unmatched truth entities alive on this frame
        for (tk, e) in truth.entities.iter().enumerate() {
            if truth_to_sim.contains_key(&tk) || truth.row(tk, fi).is_none() {
                continue;
            }
            let root = e.card.clone().unwrap_or_default();
            let sc = per_card.entry(root.clone()).or_default();
            sc.unit_ticks += 1;
            sc.missing_in_sim += 1;
            sc.after_engine_end += after_end;
            let run = missing_run.entry(tk).or_insert((t, 0));
            run.1 += 1;
            if divergence.is_none() && run.1 >= ALIVE_MISMATCH_MIN_FRAMES {
                divergence = Some(Divergence {
                    tick: run.0,
                    truth_key: e.key,
                    side: e.side,
                    card: root.clone(),
                    families: families_of(&root),
                    what: format!("truth entity {} ({}, {:?}) has no sim counterpart", e.key, e.role, e.unit),
                    cause: "spawn".into(),
                    onset_tick: run.0,
                    detail: "unmatched in the sim".into(),
                });
            }
        }
        // unmatched sim entities alive on this frame
        for sk in snap.keys() {
            if sim_to_truth.contains_key(sk) {
                continue;
            }
            let root = sim[*sk].root.clone();
            let sc = per_card.entry(root.clone()).or_default();
            sc.unit_ticks += 1;
            sc.extra_in_sim += 1;
            sc.after_engine_end += after_end;
            let run = extra_run.entry(*sk).or_insert((t, 0));
            run.1 += 1;
            if divergence.is_none() && run.1 >= ALIVE_MISMATCH_MIN_FRAMES {
                divergence = Some(Divergence {
                    tick: run.0,
                    truth_key: -1,
                    side: side_of(sim[*sk].team),
                    card: root.clone(),
                    families: families_of(&root),
                    what: format!("sim entity {} ({}, rooted by {}) has no truth counterpart", sim[*sk].card, sim[*sk].id.index, sim[*sk].root_how),
                    cause: "spawn".into(),
                    onset_tick: run.0,
                    detail: "unmatched in the truth".into(),
                });
            }
        }
    }
    let mut no_towers = Score::default();
    for (card, sc) in &per_card {
        total.add(sc);
        if !is_tower_card(card) {
            no_towers.add(sc);
        }
    }
    report.score_no_towers = no_towers;
    for root in per_card.keys() {
        report.card_families.insert(root.clone(), families_of(root));
    }
    report.score = total;
    report.per_card = per_card;
    report.first_divergence = divergence;
    Ok(report)
}

/// What the scorer remembers about one pair between frames: the hp delta on the last
/// both-alive frame, the onset tick of the position-error run above ONSET_NATIVE now
/// in progress, the last frame the hp disagreed on, and the alive-mismatch run in
/// progress (its first tick, its length in frames).
#[derive(Clone, Copy, Debug, Default)]
struct PairState(Option<i32>, Option<u32>, Option<u32>, Option<(u32, u32)>);

/// One sim entity on one frame tick, as the scorer reads it.
#[derive(Clone, Copy, Debug)]
pub struct Snap {
    pos: Vec2,
    hp: i32,
    target: Option<EntityId>,
    path_n: i32,
    deploying: bool,
    attacking: bool,
    pushed: bool,
    stunned: bool,
    jumping: bool,
    radius: i32,
    /// The contact push applied on the tick just run and the neighbour count behind it.
    /// Carried per tick rather than derived, because the engine is the only thing that
    /// knows it: a position delta cannot say whether a unit was pushed or walked.
    push: Vec2,
    push_neighbours: i32,
}

// ---------------------------------------------------------------------------
// matching and cause helpers

/// The truth key of the tower a fixture tower record describes: the truth entity
/// with card_id -1 whose first row is at that tower's position.
fn truth_tower_key(_f: &Fixture, truth: &TruthTable, t: &Tower) -> Option<i64> {
    truth.entities.iter().enumerate().find_map(|(k, e)| {
        if e.card_id != -1 || e.side != t.side {
            return None;
        }
        let r = truth.row(k, truth.first_index(k))?;
        (r.x == t.x && r.y == t.y).then_some(e.key)
    })
}

/// The two crown-tower cards (card.rs KING_TOWER / PRINCESS_TOWER).
pub fn is_tower_card(card: &str) -> bool {
    card == royalesim::card::KING_TOWER || card == royalesim::card::PRINCESS_TOWER
}

/// Groups this size and under are paired by the exact least-total-distance
/// assignment (every permutation tried); larger ones greedily by nearest pair.
pub const EXACT_ASSIGNMENT_MAX: usize = 7;

/// Pair truth points with sim points: (truth index, sim index) pairs, one per point
/// on the smaller side, minimising the total distance (exact up to
/// `EXACT_ASSIGNMENT_MAX`, greedy nearest beyond). A truth point of `None` (absent
/// on its first frame) pairs last, by order.
pub fn assign(truth: &[Option<Vec2>], sim: &[Vec2]) -> Vec<(usize, usize)> {
    let n = truth.len().min(sim.len());
    if n == 0 {
        return Vec::new();
    }
    let d = |gi: usize, si: usize| -> i64 { truth[gi].map_or(i64::MAX / 4, |p| p.dist2(sim[si])) };
    if truth.len() <= EXACT_ASSIGNMENT_MAX && sim.len() <= EXACT_ASSIGNMENT_MAX {
        // permutations of the larger side's indices, take the first n
        let (big, small, truth_big) = if truth.len() >= sim.len() { (truth.len(), sim.len(), true) } else { (sim.len(), truth.len(), false) };
        let mut perm: Vec<usize> = (0..big).collect();
        let mut best: Option<(i64, Vec<usize>)> = None;
        permute(&mut perm, 0, &mut |p| {
            let mut total = 0i64;
            for (k, &pk) in p.iter().enumerate().take(small) {
                total = total.saturating_add(if truth_big { d(pk, k) } else { d(k, pk) });
            }
            if best.as_ref().map_or(true, |b| total < b.0) {
                best = Some((total, p[..small].to_vec()));
            }
        });
        let (_, p) = best.expect("at least one permutation");
        return (0..small).map(|k| if truth_big { (p[k], k) } else { (k, p[k]) }).collect();
    }
    let mut cands: Vec<(i64, usize, usize)> = Vec::new();
    for gi in 0..truth.len() {
        for si in 0..sim.len() {
            cands.push((d(gi, si), gi, si));
        }
    }
    cands.sort();
    let mut used_t = BTreeSet::new();
    let mut used_s = BTreeSet::new();
    let mut out = Vec::new();
    for (_, gi, si) in cands {
        if used_t.contains(&gi) || used_s.contains(&si) {
            continue;
        }
        used_t.insert(gi);
        used_s.insert(si);
        out.push((gi, si));
    }
    out.sort();
    out
}

/// Pair the same-tick GROUPS of one (side, root): `truth` = (first tick, members'
/// first positions, None when absent on that frame) and `sim` = (first tick, members'
/// first positions), both in tick order. For each truth group in turn the sim pools
/// within `window` ticks are ranked: a pool whose free members number exactly the
/// group's size first, then the nearest in time, then the earlier; members are taken
/// from the ranked pools until the group is full and assigned to the group's members
/// by `assign`. Returns (truth group, member, sim pool, member). Sim members no group
/// claims stay unmatched; so do truth members with no free sim member in the window.
pub fn pair_groups(truth: &[(u32, Vec<Option<Vec2>>)], sim: &[(u32, Vec<Vec2>)], window: u32) -> Vec<(usize, usize, usize, usize)> {
    let mut free: Vec<Vec<usize>> = sim.iter().map(|(_, v)| (0..v.len()).collect()).collect();
    let mut out = Vec::new();
    for (gi, (t, pts)) in truth.iter().enumerate() {
        let n = pts.len();
        if n == 0 {
            continue;
        }
        let mut cands: Vec<(bool, u32, u32, usize)> = free
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.is_empty())
            .map(|(pi, f)| (pi, f.len(), sim[pi].0))
            .filter(|(_, _, st)| st.abs_diff(*t) <= window)
            .map(|(pi, len, st)| (len != n, st.abs_diff(*t), st, pi))
            .collect();
        cands.sort();
        let mut taken: Vec<(usize, usize)> = Vec::new();
        for (_, _, _, pi) in cands {
            for &si in &free[pi] {
                if taken.len() >= n {
                    break;
                }
                taken.push((pi, si));
            }
            if taken.len() >= n {
                break;
            }
        }
        if taken.is_empty() {
            continue;
        }
        let sim_pts: Vec<Vec2> = taken.iter().map(|&(pi, si)| sim[pi].1[si]).collect();
        for (mi, k) in assign(pts, &sim_pts) {
            let (pi, si) = taken[k];
            free[pi].retain(|&x| x != si);
            out.push((gi, mi, pi, si));
        }
    }
    out
}

fn permute(p: &mut Vec<usize>, k: usize, f: &mut dyn FnMut(&[usize])) {
    if k == p.len() {
        f(p);
        return;
    }
    for i in k..p.len() {
        p.swap(k, i);
        permute(p, k + 1, f);
        p.swap(k, i);
    }
}

/// Sim entities within contact range of `sk` on this snapshot.
fn snap_neighbours(snap: &BTreeMap<usize, Snap>, sk: usize) -> Vec<(usize, i64)> {
    let Some(me) = snap.get(&sk) else { return Vec::new() };
    snap.iter()
        .filter(|(k, _)| **k != sk)
        .filter_map(|(k, o)| {
            let need = (me.radius + o.radius) as i64 + SUBTILE_PER_MILLITILE as i64 * 100;
            let d2 = me.pos.dist2(o.pos);
            (d2 <= need * need).then_some((*k, d2))
        })
        .collect()
}

/// The cause heuristic (module doc of the CLI lists the vocabulary).
fn read_cause(sr: &Snap, tr: &Row, truth: &TruthTable, tk: usize, fi: usize, neighbours: &[(usize, i64)], last_hp_delta: Option<i32>) -> (&'static str, String) {
    if tr.state == 4 || sr.deploying {
        return ("spawn", format!("truth state {} / sim deploying {}", tr.state, sr.deploying));
    }
    if sr.pushed {
        return ("knockback", "sim knockback in progress".into());
    }
    if sr.stunned {
        return ("status", "sim stunned".into());
    }
    if tr.state == 5 || sr.jumping {
        return ("walking", "river jump".into());
    }
    let truth_attacking = tr.state == 2;
    if truth_attacking != sr.attacking {
        return ("attack-timing", format!("truth state {} vs sim attacking {}", tr.state, sr.attacking));
    }
    if last_hp_delta.is_some_and(|d| d != 0) {
        return ("attack-timing", format!("hp already differed by {}", last_hp_delta.unwrap()));
    }
    // truth neighbours: any other truth entity within ~1.5 tiles on the same frame
    let truth_near = truth.entities.iter().enumerate().any(|(k, _)| {
        k != tk
            && truth.row(k, fi).is_some_and(|o| {
                let dx = (o.x - tr.x) as i64;
                let dy = (o.y - tr.y) as i64;
                dx * dx + dy * dy <= 1500 * 1500
            })
    });
    if !neighbours.is_empty() || truth_near {
        return ("contact", format!("sim neighbours {} / truth neighbour {}", neighbours.len(), truth_near));
    }
    ("walking", format!("free walk, path nodes truth {} vs sim {}", tr.path_n, sr.path_n))
}

// ---------------------------------------------------------------------------
// aggregation and rendering

#[derive(Serialize, Clone, Debug, Default)]
pub struct Aggregate {
    pub fixtures_played: usize,
    pub fixtures_prefix: usize,
    pub fixtures_unplayable: usize,
    pub score: Score,
    pub score_no_towers: Score,
    pub per_card: BTreeMap<String, Score>,
    pub per_family: BTreeMap<String, Score>,
    pub causes: BTreeMap<String, usize>,
    pub causes_per_family: BTreeMap<String, BTreeMap<String, usize>>,
}

pub fn aggregate(reports: &[Report]) -> Aggregate {
    let mut a = Aggregate::default();
    for r in reports {
        if !r.playable {
            a.fixtures_unplayable += 1;
            continue;
        }
        a.fixtures_played += 1;
        if r.prefix_until.is_some() {
            a.fixtures_prefix += 1;
        }
        a.score.add(&r.score);
        a.score_no_towers.add(&r.score_no_towers);
        for (card, sc) in &r.per_card {
            a.per_card.entry(card.clone()).or_default().add(sc);
            for fam in r.card_families.get(card).into_iter().flatten() {
                a.per_family.entry(fam.clone()).or_default().add(sc);
            }
        }
        if let Some(d) = &r.first_divergence {
            *a.causes.entry(d.cause.clone()).or_default() += 1;
            for fam in &d.families {
                *a.causes_per_family.entry(fam.clone()).or_default().entry(d.cause.clone()).or_default() += 1;
            }
        }
    }
    a
}

fn pct(num: u64, den: u64) -> String {
    let pm = Score::permille(num, den);
    format!("{}.{}%", pm / 10, pm % 10)
}

pub fn score_row(name: &str, sc: &Score) -> String {
    format!(
        "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
        name,
        sc.unit_ticks,
        pct(sc.within[0], sc.unit_ticks),
        pct(sc.moving_within(0), sc.moving_ticks()),
        pct(sc.within[1], sc.unit_ticks),
        pct(sc.within[2], sc.unit_ticks),
        pct(sc.hp_exact, sc.unit_ticks),
        pct(sc.target_match, sc.unit_ticks),
        pct(sc.path_n_match, sc.unit_ticks),
        pct(sc.alive_mismatch + sc.missing_in_sim + sc.extra_in_sim, sc.unit_ticks),
        pct(sc.walk_within[0], sc.walk_ticks),
        pct(sc.walk_tight, sc.walk_ticks),
    )
}

/// `<=250 moving` leaves the deploy-phase frames out of both numerator and
/// denominator; `walk <=250` / `walk <=20` are over the isolated-walk unit-ticks.
pub const SCORE_HEADER: &str = "| | unit-ticks | <=250 | <=250 moving | <=500 | <=1000 | hp exact | target | path n | alive/missing/extra | walk <=250 | walk <=20 |\n|---|---|---|---|---|---|---|---|---|---|---|---|";

pub fn render_fixture_markdown(r: &Report) -> String {
    let mut out = String::new();
    out.push_str(&format!("### {}\n\n", r.fixture));
    if !r.playable {
        out.push_str("UNPLAYABLE:\n");
        for w in &r.unplayable_reasons {
            out.push_str(&format!("- {w}\n"));
        }
        return out;
    }
    let ok = r.deploys.iter().filter(|d| d.result.is_ok()).count();
    if let Some(c) = r.prefix_until {
        out.push_str(&format!("PREFIX: played up to tick {c} (then: {})\n\n", r.unplayable_reasons.join("; ")));
    }
    out.push_str(&format!(
        "deploys {} ({} accepted), pairs {}, unmatched truth {}, unmatched sim {}, truth last tick {}, engine end {:?}\n\n",
        r.deploys.len(),
        ok,
        r.pairs.len(),
        r.unmatched_truth.len(),
        r.unmatched_sim.len(),
        r.last_tick,
        r.engine_end_tick
    ));
    out.push_str(SCORE_HEADER);
    out.push('\n');
    out.push_str(&score_row("all", &r.score));
    out.push('\n');
    out.push_str(&score_row("all but towers", &r.score_no_towers));
    out.push('\n');
    for (card, sc) in &r.per_card {
        out.push_str(&score_row(card, sc));
        out.push('\n');
    }
    match &r.first_divergence {
        Some(d) => out.push_str(&format!(
            "\nfirst divergence: tick {} side {} {} (key {}) -- {}; cause **{}** (onset tick {}: {}); families {}\n",
            d.tick,
            d.side,
            d.card,
            d.truth_key,
            d.what,
            d.cause,
            d.onset_tick,
            d.detail,
            d.families.join(", ")
        )),
        None => out.push_str("\nno divergence\n"),
    }
    for d in r.deploys.iter().filter(|d| d.result.is_err()) {
        out.push_str(&format!("- deploy refused: tick {} side {} {}: {:?}\n", d.tick, d.side, d.card, d.result));
    }
    for n in &r.level_deviations {
        out.push_str(&format!("- level: {n}\n"));
    }
    for n in &r.notes {
        out.push_str(&format!("- NOTE: {n}\n"));
    }
    if let Some(e) = r.engine_end_tick {
        out.push_str(&format!("- engine end at tick {e}: {} unit-ticks scored after it against a frozen engine state\n", r.score.after_engine_end));
    }
    out
}

pub fn render_aggregate_markdown(a: &Aggregate, reports: &[Report]) -> String {
    let mut out = String::new();
    out.push_str(&format!("fixtures played {} ({} of them as prefixes), unplayable {}\n\n", a.fixtures_played, a.fixtures_prefix, a.fixtures_unplayable));
    let stale = reports.iter().filter(|r| r.playable && !r.notes.is_empty()).count();
    if stale > 0 {
        out.push_str(&format!("NOTE: {stale} played fixtures carry notes (a cards.json hash mismatch: see each fixture's .parity.md)\n\n"));
    }
    out.push_str(&format!(
        "unit-ticks after the engine's own end of battle: {} of {} (all), {} of {} (no towers)\n\n",
        a.score.after_engine_end, a.score.unit_ticks, a.score_no_towers.after_engine_end, a.score_no_towers.unit_ticks
    ));
    out.push_str(SCORE_HEADER);
    out.push('\n');
    out.push_str(&score_row("ALL", &a.score));
    out.push('\n');
    out.push_str(&score_row("ALL but towers", &a.score_no_towers));
    out.push('\n');
    out.push_str("\n#### per family\n\n");
    out.push_str(SCORE_HEADER);
    out.push('\n');
    let mut fams: Vec<_> = a.per_family.iter().collect();
    fams.sort_by_key(|(_, s)| std::cmp::Reverse(s.unit_ticks));
    for (fam, sc) in fams {
        out.push_str(&score_row(fam, sc));
        out.push('\n');
    }
    out.push_str("\n#### per card\n\n");
    out.push_str(SCORE_HEADER);
    out.push('\n');
    let mut cards: Vec<_> = a.per_card.iter().collect();
    cards.sort_by_key(|(_, s)| std::cmp::Reverse(s.unit_ticks));
    for (card, sc) in cards {
        out.push_str(&score_row(card, sc));
        out.push('\n');
    }
    out.push_str("\n#### first-divergence causes\n\n| cause | battles |\n|---|---|\n");
    let mut causes: Vec<_> = a.causes.iter().collect();
    causes.sort_by_key(|(c, n)| (std::cmp::Reverse(**n), (*c).clone()));
    for (c, n) in causes {
        out.push_str(&format!("| {c} | {n} |\n"));
    }
    out.push_str("\n#### first divergence per battle\n\n`root card` is the card that was deployed (a Tombstone's Skeleton reads Tombstone; `what` names the entity).\n\n| fixture | played to | unit-ticks (no towers) | <=250 | tick | side | root card | cause | onset | what |\n|---|---|---|---|---|---|---|---|---|---|\n");
    for r in reports.iter().filter(|r| r.playable) {
        let played = match r.prefix_until {
            Some(c) => format!("prefix < {c}"),
            None => format!("{}", r.last_tick),
        };
        let nt = &r.score_no_towers;
        let within = pct(nt.within[0], nt.unit_ticks);
        match &r.first_divergence {
            Some(d) => out.push_str(&format!("| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n", r.fixture, played, nt.unit_ticks, within, d.tick, d.side, d.card, d.cause, d.onset_tick, d.what)),
            None => out.push_str(&format!("| {} | {} | {} | {} | - | - | - | none | - | no divergence |\n", r.fixture, played, nt.unit_ticks, within)),
        }
    }
    out.push_str("\n#### unplayable\n\n| fixture | why |\n|---|---|\n");
    for r in reports.iter().filter(|r| !r.playable) {
        out.push_str(&format!("| {} | {} |\n", r.fixture, r.unplayable_reasons.join("; ")));
    }
    out
}

/// The register's path in the repo (generated, gitignored: `python
/// tools/mechanic_register.py` writes it).
pub fn register_path() -> String {
    format!("{}/data/derived/mechanic_register.json", repo_root())
}

/// The register, or -- when the file is absent -- no families at all, with a note.
/// The families only label the report's per-family table and the divergences, so a
/// tree without the generated file still plays and scores every fixture.
pub fn load_register_or_empty(path: &str) -> (BTreeMap<String, Vec<String>>, Option<String>) {
    match load_register(path) {
        Ok(r) => (r, None),
        Err(e) => (BTreeMap::new(), Some(format!("{e}: no mechanic families (run python tools/mechanic_register.py)"))),
    }
}

/// data/derived/mechanic_register.json -> card name -> family names.
pub fn load_register(path: &str) -> Result<BTreeMap<String, Vec<String>>, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&s).map_err(|e| format!("{path}: {e}"))?;
    let mut out = BTreeMap::new();
    if let Some(cards) = v.get("cards").and_then(|c| c.as_object()) {
        for (name, rec) in cards {
            let fams: Vec<String> = rec.get("families").and_then(|f| f.as_object()).map(|f| f.keys().cloned().collect()).unwrap_or_default();
            out.insert(name.clone(), fams);
        }
    }
    Ok(out)
}

pub fn repo_root() -> String {
    format!("{}/../..", env!("CARGO_MANIFEST_DIR"))
}
