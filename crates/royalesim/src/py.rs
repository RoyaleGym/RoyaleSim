//! PyO3 bindings: the Rust side of RoyaleGym's `royalegym/rust_engine.py::RustEngine`,
//! which implements protocol.py's `Engine` over `BattleState`.
//!
//! WHY THIS SHAPE
//!     * ONE Rust call per env step. `step(commands, ticks)` validates, applies and
//!       runs every tick without returning to Python, with the GIL released while
//!       ticking, so a vectorised Python env on threads is not serialised by it.
//!     * BULK STATE AS ONE BYTE STRING. `state_json()` writes protocol.py's
//!       `BattleState` as JSON in one pass; Python decodes it with msgspec's typed
//!       decoder (C, no intermediate dicts). Building 40 `EntityState` objects from
//!       Rust through PyO3 attribute sets would cost more than the tick.
//!     * NO PROTOCOL NUMBER IS COPIED HERE. Deploy reasons are returned as indices
//!       into `DEPLOY_REASONS`, a list of protocol.py `DeployStatus` NAMES; Python
//!       maps names to its enum. Tower-slot naming (which engine lane is a team's
//!       own-LEFT under protocol.py's 180-degree seat rotation) is a table the
//!       adapter derives from positions and passes in. Card ids are the adapter's
//!       catalogue order.
//!
//! FRAMES
//!     Positions cross this boundary untouched: the engine frame of protocol.py
//!     (Blue defends low y, origin at Blue's back-left corner, subtiles) is this
//!     crate's frame. tests/test_rust_engine.py cross-checks that claim against
//!     MockEngine (tower positions, passability, deploy legality).
//!
//! TERRITORY
//!     The engine decides troop territory from the alive enemy crown towers'
//!     NoDeploySize rects (arena.rs TROOP TERRITORY). `tower_no_deploy_rects()`
//!     hands the adapter the exact rects, and `territory_model()` the registry
//!     name, so the Python mask can be built from the engine's numbers. There is
//!     no pocket-depth argument and no shipped pocket-depth constant: the rects
//!     are the mechanic (arena.rs TROOP TERRITORY).
//!
//! SIMULTANEOUS COMMANDS
//!     Accepted deploys are applied in the canonical order (team, then hand slot),
//!     never in the order of the command list, so `step([blue, red])` and
//!     `step([red, blue])` are the same battle (state_hash included).
//!
//! SPELLS
//!     The catalogue carries every simulable spell (the thin slice's Fireball, Arrows,
//!     Zap, The Log, Goblin Barrel; Rocket and Freeze load too because their data has an
//!     implemented shape). `catalogue_json`'s kind code is the card's DEPLOY RULE
//!     (state.rs `deploy_rule`, the engine's one definition), numbered to match
//!     protocol.py `Placement` where a Placement exists:
//!         0 TROOP  1 BUILDING  2 SPELL (anywhere)  3 ROLLING (troop territory, over
//!         buildings)  4 SPELL_NOT_ON_WATER (anywhere except water: Goblin Barrel).
//!     Code 4 has NO protocol.py Placement yet -- the Python mask must add one or
//!     it will offer the river to a Goblin Barrel that the engine refuses as WATER.
//!     Spell rows report count 0, radius 0, flying false, hitpoints 0 (protocol.py
//!     CardInfo: "0 for spells").
//!     A unit a spell RELEASES (the Goblin of a Goblin Barrel) is not a card: it is
//!     never in the catalogue, and `state_json` reports it under the catalogue id of
//!     the spell that releases it.
//!     `state_json` additionally carries, as trailing data the protocol decoder
//!     ignores (msgspec 0.21.1 drops extra array elements and unknown keys, which
//!     this repo measures rather than assumes): per entity, two more row elements
//!     [stun_ticks, knockback_ticks]
//!     (ticks remaining, rounded up; under the shipped knockback ladder the ticks the
//!     ladder still runs, `knock_ticks_left`); and a top-level "spells" array of rows
//!         [team, card_id, motion, x, y, aim_x, aim_y, delay_ticks, travelled, length, hits]
//!     motion 0 flight / 1 airborne (the Log before it lands) / 2 rolling / 3 area
//!     effect; (x, y) the current centre; aim the landing point (flight, airborne) or
//!     the roll's end point (rolling) or the centre (area); distances in subtiles.
//!
//! WHAT IT CANNOT DO (raises instead of guessing)
//!     `crowns_from_destroyed_towers = false` (crowns are derived from destroyed
//!     towers every tick) is refused in rust_engine.py.

// pyo3 0.22's #[pymethods] expansion converts PyErr into PyErr, which clippy on
// Rust 1.98 reports on every fallible method signature; the lint is about macro
// output, not this code.
#![allow(clippy::useless_conversion)]
#![allow(unexpected_cfgs)]

use crate::arena::Arena;
use crate::arena::Territory;
use crate::card::{CardDb, CardKind, SpellDef, SpellShape, KING_TOWER, PRINCESS_TOWER};
use crate::entity::EntityKind;
use crate::fixed::Vec2;
use crate::spell::SpellMotion;
use crate::state::{deploy_rule, BattleConfig, BattleState, Calib, DeployError, Outcome, HAND_SIZE};
use crate::{Rng, Team};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::fmt::Write as _;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The calibration.json and arena.json this extension was COMPILED with
/// (state.rs / arena.rs `include_str!` the same files). rust_engine.py compares
/// them with the files on disk and refuses to run a stale build, because a
/// rebuilt-later calibration is otherwise a silently different battle.
pub const EMBEDDED_CALIBRATION_JSON: &str = include_str!("../../../data/calibration.json");

/// EVERY `Calib` field a seat-symmetry gate has to be able to turn off, by its serde
/// name. A key whose shipped arm is deliberately asymmetric -- because the game is --
/// is useless to a rotation gate unless the gate can select the symmetric arm, and
/// `Battle::new` is the only way a Python caller can. tests/mirror.rs
/// `every_asymmetric_calib_key_is_selectable_from_python` diffs `symmetric_config()`
/// against `config()` and asserts the difference is exactly this list, so a new
/// asymmetric key cannot ship without its selector: that check has been added because
/// the clamp shipped without one, and then the ground deploy point did it again.
pub const SYMMETRY_SELECTABLE_CALIB_FIELDS: &[&str] = &[
    // `path_search` selects the frame-planned search AND the fixed-distance knockback
    // pair with it, so one kwarg reaches four fields.
    "path_search",
    "knock_law",
    "knock_stacking",
    "knock_zero_vector",
    "formation_ground_y_clamp",
    "formation_ground_deploy_point",
];
pub const EMBEDDED_ARENA_JSON: &str = include_str!("../../../data/derived/arena.json");

/// THE OTHER TWO FILES THIS CRATE COMPILES IN, exposed for the same reason as the two
/// above and only after they had gone unwatched for months.
///
/// Four files are `include_str!`ed: calibration.json (state.rs), arena.json (arena.rs),
/// rarities.csv (card.rs) and globals.csv (state.rs). Only the first two were ever
/// compared with the disk. Each of the other two has exactly the property that took the
/// whole workspace's engine down twice on 2026-09-22 -- compiled into the binary, editable
/// without a rebuild, and nothing notices -- and theirs is the WORSE failure, because a
/// stale arena or a stale rarity table is a silently different battle rather than a loud
/// refusal to construct.
pub const EMBEDDED_RARITIES_CSV: &str = include_str!("../../../data/raw/retroroyale-2018/csv_logic/rarities.csv");
pub const EMBEDDED_GLOBALS_CSV: &str = include_str!("../../../data/raw/retroroyale-2018/csv_logic/globals.csv");

/// protocol.py `DeployStatus` names, indexed by the reason codes this module
/// returns. ENGINE_ERROR is not a protocol status: it marks a DeployError that a
/// slot-indexed command cannot produce, and Python raises on it.
pub const DEPLOY_REASONS: [&str; 14] = [
    "OK",
    "BAD_TEAM",
    "BAD_SLOT",
    "EMPTY_SLOT",
    "NOT_ENOUGH_ELIXIR",
    "OUT_OF_ARENA",
    "WATER",
    "NO_DEPLOY",
    "OUT_OF_TERRITORY",
    "OCCUPIED",
    "GAME_OVER",
    "DUPLICATE_TEAM",
    "ENGINE_ERROR",
    // TOO_EARLY, index 13, added 2026-09-23 with match.DEPLOY_LOCKOUT_TICKS. The array
    // is length-annotated, so adding the reason code in `reason_of` without adding its
    // NAME here ran off the end of this list and gym's table -- which is derived from
    // it, correctly -- raised IndexError. The exhaustive match caught the Rust half and
    // nothing caught this half, because a `[&str; N]` grows by editing two places.
    "TOO_EARLY",
];

/// THE ENTITY ROW'S FIELDS, in exactly the order `state_json` writes them, named as
/// protocol.py `EntityState` names them. Published so a decoder can REFUSE a mismatch
/// instead of trusting one: rows are positional, two of the trailing fields are adjacent
/// ints, and a swap would decode without error and be drawn with confidence. The length
/// is pinned to the serializer by a test in this file, so this is the half that cannot
/// fall behind -- DEPLOY_REASONS showed what the unpinned half does.
pub const ENTITY_FIELDS: [&str; 20] = [
    "uid",
    "team",
    "kind",
    "card_id",
    "tower_slot",
    "x",
    "y",
    "hp",
    "max_hp",
    "radius",
    "flying",
    "deploy_ticks",
    "stun_ticks",
    "knockback_ticks",
    "footprint",
    // added 2026-09-24, for a viewer that shows what a unit is doing and what is on it
    "target_uid",
    "attack_phase",
    "facing",
    "shield",
    "buffs",
];

/// THE PROJECTILE ROW'S FIELDS, in `state_json`'s order (its `projectiles` key). Same
/// reason as ENTITY_FIELDS, and pinned to the serializer the same way.
pub const PROJECTILE_FIELDS: [&str; 8] = ["team", "x", "y", "aim_x", "aim_y", "target_uid", "splash", "firer_card_id"];

/// AN EXPERIMENT'S CALIBRATION: the compiled-in ledger with some `value`s replaced.
///
/// For separating one change from another -- the same battle with one rule reverted --
/// without editing `data/calibration.json`, which every session's engine reads, and whose
/// edit is a stale window for all of them. Keys are `section.KEY`; values are JSON
/// (`json.dumps(value)` from Python), so `0` is a number and `"reset_always"` (quoted)
/// an arm name, and nothing has to guess which was meant.
///
/// It REFUSES rather than adapts. A key the ledger does not have is an error, because an
/// override cannot add one: a typo would otherwise run the shipped value and report the
/// experiment done. So is any value `Calib::from_json` rejects, such as an arm name with
/// no implementation.
fn overridden_calib(overrides: &BTreeMap<String, String>) -> Result<(Calib, BTreeMap<String, serde_json::Value>), String> {
    let mut doc: serde_json::Value = serde_json::from_str(EMBEDDED_CALIBRATION_JSON).map_err(|e| format!("the compiled-in ledger: {e}"))?;
    let mut parsed = BTreeMap::new();
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
        let v: serde_json::Value =
            serde_json::from_str(raw).map_err(|e| format!("{path:?}: {raw:?} is not JSON ({e}); pass json.dumps(value)"))?;
        entry.insert("value".to_string(), v.clone());
        parsed.insert(path.clone(), v);
    }
    let calib = Calib::from_json(&doc.to_string()).map_err(|e| format!("the overridden ledger does not load: {e}"))?;
    Ok((calib, parsed))
}

const R_OK: u8 = 0;
const R_BAD_TEAM: u8 = 1;
const R_DUPLICATE_TEAM: u8 = 11;

/// Exhaustive on purpose: a new DeployError variant fails to compile here
/// instead of silently reporting OK or a wrong reason.
fn reason_of(r: &Result<(), DeployError>) -> u8 {
    match r {
        Ok(()) => R_OK,
        Err(e) => match e {
            DeployError::BadSlot => 2,
            DeployError::EmptySlot => 3,
            DeployError::NotEnoughElixir { .. } => 4,
            DeployError::OutOfArena => 5,
            DeployError::Water => 6,
            DeployError::NoDeploy => 7,
            DeployError::OutOfTerritory => 8,
            DeployError::Occupied => 9,
            DeployError::GameOver => 10,
            // 13, its own code rather than the 12 catch-all: "the match has not opened
            // yet" is a reason a caller can act on by waiting, and the others in 12 are
            // not. This arm exists because the match above is exhaustive on purpose and
            // refused to compile without it, which is the comment above doing its job.
            DeployError::TooEarly { .. } => 13,
            DeployError::UnknownCard(_)
            | DeployError::UnsupportedCard(..)
            | DeployError::NotInHand
            | DeployError::InvalidLevel(_) => 12,
        },
    }
}

fn team_of(t: i64) -> Option<Team> {
    match t {
        0 => Some(Team::Blue),
        1 => Some(Team::Red),
        _ => None,
    }
}

fn ceil_div(a: i64, b: i64) -> i64 {
    (a + b - 1) / b
}

/// One battle behind the Python `Engine` protocol.
#[pyclass(module = "royalesim")]
pub struct Battle {
    cards: Arc<CardDb>,
    /// Catalogue card id -> CardDb index.
    catalogue: Vec<u16>,
    /// CardDb index -> catalogue card id, -1 when not in the catalogue.
    id_of_idx: Vec<i32>,
    /// [team][engine tower k] -> protocol TowerSlot. Supplied by the adapter.
    slot_of_k: [[i32; 3]; 2],
    /// An override of calibration pathfinding.PATH_SEARCH for every battle this
    /// object starts (None = the ledger's value).
    path_search: Option<crate::state::PathSearch>,
    /// An override of calibration formation.GROUND_Y_CLAMP for every battle this
    /// object starts (None = the ledger's value).
    ground_y_clamp: Option<crate::state::GroundYClamp>,
    ground_deploy_point: Option<crate::state::GroundDeployPoint>,
    /// An EXPERIMENT's whole calibration (`calibration_overrides`), in place of the
    /// ledger's for every battle this object starts. None = the ledger.
    calib: Option<Calib>,
    /// What was overridden, parsed, for `calibration_overrides()` and for every frame.
    calib_overrides: BTreeMap<String, serde_json::Value>,
    state: Option<BattleState>,
}

/// Why a restored battle cannot run behind this catalogue, or Ok. Every card the
/// battle can still produce -- a hand slot, the cycle queue, a pending spawn, a
/// non-tower entity on the board -- must have a catalogue id, or `state_json`
/// would report it as card -1 and the Python side would see a card that does not
/// exist. Crown towers are never catalogue cards and are exempt.
///
/// WHY IT EXISTS: checking hand slots alone is not enough -- a snapshot's cycle
/// queue, pending spawns and board entities can each carry a card the catalogue
/// does not have, and each would surface on the Python side as card -1.
pub fn catalogue_violation(s: &BattleState, id_of_idx: &[i32]) -> Result<(), String> {
    let missing = |i: u16| id_of_idx.get(i as usize).map_or(true, |c| *c < 0);
    let name = |i: u16| s.cards().get(i).name.clone();
    for team in [Team::Blue, Team::Red] {
        for slot in 0..HAND_SIZE {
            if let Ok(i) = s.hand_card(team, slot) {
                if missing(i) {
                    return Err(format!("snapshot hand card {} not in catalogue", name(i)));
                }
            }
        }
        #[cfg(not(clash_plant = "load_skips_queue_check"))]
        for i in s.queue_cards(team) {
            if missing(i) {
                return Err(format!("snapshot queue card {} not in catalogue", name(i)));
            }
        }
    }
    for (_, i, _) in s.pending_spawns() {
        if missing(i) {
            return Err(format!("snapshot pending spawn {} not in catalogue", name(i)));
        }
    }
    let towers = [s.tower_ids(Team::Blue), s.tower_ids(Team::Red)];
    for e in s.entities() {
        if towers[e.team as usize].contains(&Some(e.id)) {
            continue;
        }
        if missing(e.card_idx) {
            return Err(format!("snapshot board entity {} not in catalogue", e.card));
        }
    }
    Ok(())
}

/// Validate every command against the CURRENT state, then apply the accepted ones
/// in CANONICAL order (team, then slot). Returns (card_id, reason, tick) per
/// command, in INPUT order. A second accepted command for one team is
/// DUPLICATE_TEAM, decided in input order (the protocol's rule). Pure Rust (no
/// Python types) so the order-independence test runs under `cargo test`.
///
/// Applying them in INPUT order instead would make the spawn queue -- and so every
/// entity slot index and the state hash -- depend on whether Blue or Red was listed
/// first, for plays the protocol calls simultaneous.
/// WHAT ONE COMMAND GOT BACK: the card id it was played from, the reason code, the tick it
/// was evaluated on, and the RESOLVED x and y -- where the card actually went down, which for
/// a relocated building is not the tap. Named rather than repeated, because it appears in two
/// signatures and a five-tuple written twice is a five-tuple that can drift in one place.
pub type CommandOutcome = (i32, u8, u32, i32, i32);

/// The tap a caller made: team, hand slot, x, y.
pub type Command = (i64, i64, i32, i32);

pub fn apply_commands(
    s: &mut BattleState,
    commands: &[Command],
    id_of_idx: &[i32],
) -> Result<Vec<CommandOutcome>, String> {
    let tick = s.tick_count();
    let card_id = |s: &BattleState, team: i64, slot: i64| match (team_of(team), usize::try_from(slot)) {
        (Some(t), Ok(k)) => s.hand_card(t, k).map(|i| id_of_idx[i as usize]).unwrap_or(-1),
        _ => -1,
    };
    // Validation first, all against the same state: both teams' plays are
    // simultaneous even though they are applied one after the other.
    // The last two elements are WHERE THE CARD WENT DOWN. They start as the tap and are
    // overwritten with the resolved point as each accepted deploy is applied, so a refused
    // command reports the point it asked for and an accepted building reports the point it
    // actually took.
    let mut out: Vec<(i32, u8, u32, i32, i32)> = commands
        .iter()
        .map(|&(team, slot, x, y)| (card_id(s, team, slot), check_command(s, team, slot, x, y), tick, x, y))
        .collect();
    let mut seen = [false; 2];
    let mut accepted = Vec::with_capacity(2);
    for (k, &(team, slot, x, y)) in commands.iter().enumerate() {
        if out[k].1 != R_OK {
            continue;
        }
        let t = team as usize;
        if seen[t] {
            out[k].1 = R_DUPLICATE_TEAM;
            continue;
        }
        seen[t] = true;
        accepted.push((k, team, slot, x, y));
    }
    #[cfg(not(clash_plant = "command_order_matters"))]
    accepted.sort_by_key(|&(_, team, slot, _, _)| (team, slot));
    for (k, team, slot, x, y) in accepted {
        let t = team_of(team).expect("validated");
        // Resolved IN THE ORDER THE DEPLOYS HAPPEN, because one team's building changes
        // where the other team's tap in the same step can fit.
        let landed = s
            .deploy_slot(t, slot as usize, Vec2::new(x, y))
            .map_err(|e| format!("deploy validated OK was then refused: {e:?}"))?;
        out[k].3 = landed.x;
        out[k].4 = landed.y;
    }
    Ok(out)
}

/// The reason code a single command gets against `s` (protocol.py's order:
/// GAME_OVER before BAD_TEAM).
fn check_command(s: &BattleState, team: i64, slot: i64, x: i32, y: i32) -> u8 {
    if s.is_done() {
        return reason_of(&Err(DeployError::GameOver));
    }
    let Some(team) = team_of(team) else { return R_BAD_TEAM };
    if slot < 0 {
        return reason_of(&Err(DeployError::BadSlot));
    }
    reason_of(&s.check_deploy_slot(team, slot as usize, Vec2::new(x, y)))
}

/// The catalogue kind code of card `idx` (see the module doc, SPELLS). From
/// `state::deploy_rule`, so the code the mask is built from is the rule the engine
/// deploys by.
pub fn kind_code(cards: &CardDb, calib: &Calib, idx: u16) -> u8 {
    let c = cards.get(idx);
    match (c.kind, deploy_rule(calib, c)) {
        (CardKind::Troop, _) => 0,
        (CardKind::Building, _) => 1,
        (CardKind::Spell, (Territory::EnemyTowerRects, _)) => 3,
        (CardKind::Spell, (Territory::AnywhereButWater, _)) => 4,
        (CardKind::Spell, _) => 2,
    }
}

/// Catalogue id per CardDb index: the catalogue position, or for a released unit the
/// id of the FIRST catalogue spell that releases it, or -1.
pub fn ids_of_indices(cards: &CardDb, catalogue: &[u16]) -> Vec<i32> {
    let mut id_of_idx = vec![-1; cards.cards.len()];
    for (cid, idx) in catalogue.iter().enumerate() {
        id_of_idx[*idx as usize] = cid as i32;
    }
    // A summon-only unit reports under the FIRST catalogue card that can produce it:
    // a spell's release, a periodic spawner's or a death spawn's unit (one Skeleton
    // record serves Skeletons, Tombstone and Witch alike).
    for (cid, idx) in catalogue.iter().enumerate() {
        let c = cards.get(*idx);
        let mut units: Vec<u16> = Vec::new();
        if let Some(SpellDef { shape: SpellShape::Projectile { spawn: Some(sp), .. }, .. }) = &c.spell {
            units.push(sp.unit);
        }
        units.extend(c.spawner.map(|sp| sp.unit));
        units.extend(c.death_spawn.map(|ds| ds.unit));
        for u in units {
            // A card whose unit could not be loaded is rejected (unregistered, its
            // unit index unresolved) and never in a catalogue that came from names;
            // the by-index default catalogue below skips it too.
            if (u as usize) < id_of_idx.len() && id_of_idx[u as usize] == -1 {
                id_of_idx[u as usize] = cid as i32;
            }
        }
    }
    id_of_idx
}

/// `Battle.catalogue_json` without Python: rows [name, kind code, elixir, count,
/// radius, flying, hitpoints at `level`].
pub fn catalogue_rows(cards: &CardDb, calib: &Calib, catalogue: &[u16], level: i32) -> Result<String, String> {
    let mut out = String::from("[");
    for (k, idx) in catalogue.iter().enumerate() {
        let c = cards.get(*idx);
        let kind = kind_code(cards, calib, *idx);
        let (count, radius, flying, hp) = match c.kind {
            CardKind::Spell => (0, 0, false, 0),
            _ => (c.count, c.collision_radius, c.is_flying(), cards.scaled(*idx, level, c.hitpoints)?),
        };
        if k > 0 {
            out.push(',');
        }
        let name = serde_json::to_string(&c.name).expect("string serializes");
        // The 8th element: the side of the card's placement footprint in TILES, or
        // null for a card that is not a building. A mask can then test a tap
        // before the building exists (calibration placement.FOOTPRINT_TILES).
        let footprint = match c.kind {
            CardKind::Building => crate::arena::placement_tiles(c.collision_radius).to_string(),
            _ => "null".to_string(),
        };
        let _ = write!(out, "[{name},{kind},{},{count},{radius},{flying},{hp},{footprint}]", c.elixir);
    }
    out.push(']');
    Ok(out)
}

/// `Battle.state_json` without Python (protocol.py `BattleState` as JSON text).
pub fn state_json_text(
    s: &BattleState,
    cards: &CardDb,
    id_of_idx: &[i32],
    slot_of_k: &[[i32; 3]; 2],
    overrides: &BTreeMap<String, serde_json::Value>,
) -> Result<String, String> {
    let c = &s.config().calib;
    let tick_ms = c.tick_ms as i64;
    let regular_ms = (c.regular_time_s as i64) * 1000;
    let elapsed = (s.tick_count() as i64) * tick_ms;
    let double = s.is_overtime() || regular_ms - elapsed <= (c.mana_speed_up_remaining_s as i64) * 1000;
    let winner = match s.outcome() {
        None => -1,
        Some(Outcome::Winner(t)) => t as i32,
        Some(Outcome::Draw) => 2,
    };
    let tower_lvl = s.config().tower_level;
    let mut o = String::with_capacity(4096);
    let _ = write!(
        o,
        "{{\"tick\":{},\"tick_ms\":{},\"regular_ticks\":{},\"overtime_ticks\":{},\"elixir_rate\":{},\"overtime\":{},\"game_over\":{},\"winner\":{},\"players\":[",
        s.tick_count(),
        tick_ms,
        ceil_div(regular_ms, tick_ms),
        ceil_div((c.overtime_s as i64) * 1000, tick_ms),
        if double { 2 } else { 1 },
        s.is_overtime(),
        s.is_done(),
        winner,
    );
    let crowns = s.crowns();
    for (ti, team) in [Team::Blue, Team::Red].into_iter().enumerate() {
        if ti > 0 {
            o.push(',');
        }
        let (mana, unit) = s.elixir_raw(team);
        let _ = write!(o, "{{\"team\":{ti},\"elixir_milli\":{},\"hand\":[", mana * 1000 / unit);
        for slot in 0..HAND_SIZE {
            if slot > 0 {
                o.push(',');
            }
            let id = s.hand_card(team, slot).map(|i| id_of_idx[i as usize]).unwrap_or(-1);
            let _ = write!(o, "{id}");
        }
        let next = s.next_card(team).and_then(|n| cards.index(n)).map(|i| id_of_idx[i as usize]).unwrap_or(-1);
        let hp = s.tower_hp(team);
        let mut by_slot = [0i32; 3];
        let mut max_by_slot = [0i32; 3];
        let tower_entities = s.tower_ids(team);
        for (k, tower_hp) in hp.iter().enumerate() {
            let slot = slot_of_k[ti][k] as usize;
            by_slot[slot] = *tower_hp;
            // THE TOWER'S OWN max_hp, not the card ladder's. A crown tower is
            // spawned on the tower hitpoint ladder, so reading the card ladder
            // here reported a maximum the tower never had: a full king came out
            // 6144 against its real 4824, and a fraction of full read 0.829 at
            // kickoff. A FALLEN tower has no entity left, so it keeps the ladder
            // value, which is the maximum it had while it stood.
            let from_entity = tower_entities[k].and_then(|id| s.entity(id)).map(|e| e.max_hp);
            max_by_slot[slot] = match from_entity {
                Some(m) => m,
                None => {
                    let card = if k == 0 { KING_TOWER } else { PRINCESS_TOWER };
                    let idx = cards.index(card).expect("towers exist");
                    cards.scaled(idx, tower_lvl[ti], cards.get(idx).hitpoints)?
                }
            };
        }
        let _ = write!(
            o,
            "],\"next_card\":{next},\"crowns\":{},\"tower_hp\":[{},{},{}],\"tower_max_hp\":[{},{},{}],\"king_active\":{}}}",
            crowns[ti],
            by_slot[0],
            by_slot[1],
            by_slot[2],
            max_by_slot[0],
            max_by_slot[1],
            max_by_slot[2],
            s.king_active(team),
        );
    }
    o.push_str("],\"entities\":[");
    let ids = [s.tower_ids(Team::Blue), s.tower_ids(Team::Red)];
    let mut first = true;
    for e in s.entities() {
        if !first {
            o.push(',');
        }
        first = false;
        let ti = e.team as usize;
        let tower_k = ids[ti].iter().position(|t| *t == Some(e.id));
        let (card_id, slot) = match tower_k {
            Some(k) => (-1, slot_of_k[ti][k]),
            None => (id_of_idx[e.card_idx as usize], -1),
        };
        // uid: the entity's spawn ordinal within its team, interleaved by team.
        // Never reused (team counters only grow) and equal for mirror twins up
        // to the team bit -- unlike the slot index, which is reused.
        let uid = (e.team_seq as i64) * 2 + ti as i64;
        // The 15th element: the placement footprint as a CLOSED box
        // [x0, y0, x1, y1] in subtiles, or null for anything that is not a
        // building or a crown tower. It is where the thing was PUT, not a
        // collision shape: troops stand inside these boxes routinely
        // (calibration placement.FOOTPRINT_TILES).
        let footprint = if e.kind.is_building() {
            let n = crate::arena::placement_tiles(e.radius);
            let b = crate::arena::Arena::placement_box(e.pos, n);
            format!("[{},{},{},{}]", b.min.x, b.min.y, b.max.x, b.max.y)
        } else {
            "null".to_string()
        };
        // ENTITY_FIELDS 15..20. The target as the ROWS' OWN uid, so a viewer joins the
        // two without a second id space, and -1 once it is gone. Buffs by NAME from the
        // card data, `|`-joined where one engine buff stands for several (card.rs
        // `CardDb::buff_names`), with the milliseconds left.
        let target_uid = e.target.and_then(|t| s.entity(t)).map(|t| (t.team_seq as i64) * 2 + t.team as i64).unwrap_or(-1);
        let mut buffs = String::from("[");
        for b in e.buffs.iter().filter(|b| b.id > 0) {
            if buffs.len() > 1 {
                buffs.push(',');
            }
            let name = cards.buff_names.get(b.id as usize - 1).map(String::as_str).unwrap_or("");
            let _ = write!(buffs, "[{},{}]", serde_json::Value::from(name), b.ms);
        }
        buffs.push(']');
        let _ = write!(
            o,
            "[{uid},{ti},{},{card_id},{slot},{},{},{},{},{},{},{},{},{},{footprint},{target_uid},{},[{},{}],{},{buffs}]",
            e.kind as u8,
            e.pos.x,
            e.pos.y,
            e.hp,
            e.max_hp,
            e.radius,
            e.flying,
            ceil_div(e.deploy_ms as i64, tick_ms),
            ceil_div(e.stun_ms as i64, tick_ms),
            knock_ticks_left(&e, tick_ms),
            e.attack_phase as u8,
            e.facing.x,
            e.facing.y,
            e.shield,
        );
    }
    o.push_str("],\"spells\":[");
    for (k, sp) in s.spells().iter().enumerate() {
        if k > 0 {
            o.push(',');
        }
        let card_id = id_of_idx.get(sp.card as usize).copied().unwrap_or(-1);
        let fwd = crate::spell::forward_dy(sp.team);
        let (motion, pos, aim, delay_ms, travelled, len, hits) = match &sp.motion {
            SpellMotion::Flight { pos, aim, delay_ms, .. } => (0, *pos, *aim, *delay_ms, 0, 0, 0),
            SpellMotion::Airborne { pos, aim, .. } => (1, *pos, *aim, 0, 0, 0, 0),
            SpellMotion::Rolling { pos, travelled, len, hit } => (2, *pos, Vec2::new(pos.x, pos.y + fwd * (len - travelled)), 0, *travelled, *len, hit.len()),
            SpellMotion::Area { pos } => (3, *pos, *pos, 0, 0, 0, 0),
            // a pulsing area: `delay_ms` carries its remaining life, so the viewer can
            // draw a Poison cloud shrinking rather than a one-frame flash.
            SpellMotion::Pulsing(p) => (4, p.pos, p.pos, p.life_ms, 0, 0, 0),
        };
        let _ = write!(
            o,
            "[{},{card_id},{motion},{},{},{},{},{},{travelled},{len},{hits}]",
            sp.team as u8,
            pos.x,
            pos.y,
            aim.x,
            aim.y,
            ceil_div(delay_ms.max(0) as i64, tick_ms),
        );
    }
    // PROJECTILE_FIELDS, one row per projectile in flight.
    o.push_str("],\"projectiles\":[");
    for (k, p) in s.projectiles().iter().enumerate() {
        if k > 0 {
            o.push(',');
        }
        // -1 once the target is gone: the projectile flies on to `aim`
        let target_uid = s.entity(p.target).map(|t| (t.team_seq as i64) * 2 + t.team as i64).unwrap_or(-1);
        // -1 a crown tower, which is not a catalogue card (the entity rows' own
        // convention); -2 NOT RECORDED, a projectile restored from a snapshot older than
        // the field, kept apart so "unknown" never reads as "a tower fired this"
        let firer = match p.firer_card {
            Some(c) => id_of_idx.get(c as usize).copied().unwrap_or(-1),
            None => -2,
        };
        let _ = write!(o, "[{},{},{},{},{},{target_uid},{},{firer}]", p.team as u8, p.pos.x, p.pos.y, p.aim.x, p.aim.y, p.splash);
    }
    o.push(']');
    if !overrides.is_empty() {
        // EVERY FRAME OF AN OVERRIDDEN BATTLE SAYS SO. `build_digest` hashes the
        // COMPILED-IN ledger, so it reads the same with or without an override; a frame
        // that did not carry this could be quoted as the shipped engine's behaviour and
        // nothing on it would contradict the quote.
        o.push_str(",\"calibration_overrides\":");
        o.push_str(&serde_json::to_string(overrides).map_err(|e| e.to_string())?);
    }
    o.push('}');
    Ok(o)
}

impl Battle {
    fn s(&self) -> PyResult<&BattleState> {
        self.state.as_ref().ok_or_else(|| PyRuntimeError::new_err("Battle.reset() has not been called"))
    }

    fn s_mut(&mut self) -> PyResult<&mut BattleState> {
        self.state.as_mut().ok_or_else(|| PyRuntimeError::new_err("Battle.reset() has not been called"))
    }

    fn level(&self) -> i32 {
        self.cards.lowest_level_valid_for_every_rarity()
    }

}

#[pymethods]
impl Battle {
    /// `card_names`: the catalogue, in card-id order (None = every simulable
    /// non-tower card in cards.json order). `slot_of_k[team][k]` names engine tower
    /// k (0 king, 1 engine-Left princess, 2 engine-Right) as a protocol TowerSlot.
    ///
    /// `path_search`: None = the ledger's pathfinding.PATH_SEARCH (the measured
    /// 16.402 arm, client16402, which is NOT seat-symmetric); "trace_fitted_astar"
    /// selects the frame-planned arm whose routes are exact rotations of each
    /// other -- what the env layer's rotation-mirror gates run under. That
    /// selection ALSO puts the knockback on the fixed-distance slide
    /// (knockback.DISPLACEMENT_LAW = fixed_distance with vector_sum and
    /// caster_forward, tests/common `symmetric_config()`): the shipped ladder is the
    /// game's and has two absolute-frame points (the zero-vector direction by id
    /// parity, the water teleport's tie), so the seat-symmetric arm is the pair.
    ///
    /// `ground_y_clamp`: None = the ledger's formation.GROUND_Y_CLAMP, the measured
    /// arm "client16402_deploy_column_range". It holds every GROUND member of a
    /// multi-unit summon inside the tap column's deployable y range, and that range
    /// is measured PER SIDE: side 1's is not the rotation of side 0's -- one native
    /// unit tighter at the river, half a row shorter at the back edge -- which is
    /// what the game does. The corpus settles it: a Red rear pair stands 261 native
    /// units from where the rotated formula would put it, and only the per-side
    /// range predicts the side it is actually on. A multi-unit ground card's members
    /// are therefore not the rotation of their twin's, on purpose; flying members
    /// and single-unit cards are untouched by the clamp and are.
    /// `ground_deploy_point`: None = the ledger's formation.GROUND_DEPLOY_POINT, the
    /// measured arm "client16402_one_unit". A GROUND summon's ring is laid on a point
    /// one native unit off the tap -- x when the tap is on the arena's LEFT half,
    /// either seat; y when the owner is side 1, either half -- and a FLYING summon's
    /// on the tap itself. Measured on the corpus, and it is why a multi-unit ground
    /// card's members are not the rotation of their twin's even where the clamp never
    /// bites. "none" lays every ring on the tap, which is what a flying summon
    /// measures on both seats; a rotation gate wants it for the same reason it wants
    /// "deploy_column_range_own_frame".
    ///
    /// "deploy_column_range_own_frame" reads side 0's formula in the OWNER's frame
    /// for both seats. It is not the game's clamp; it exists so that a rotation gate
    /// can measure the seat symmetry of EVERYTHING ELSE without the per-side range
    /// answering for the whole battle (tests/common `symmetric_config()` selects it
    /// for the Rust seat-symmetry gates, and the env layer's symmetric engine wants
    /// the same). "none" drops the clamp entirely.
    #[new]
    #[pyo3(signature = (card_names, slot_of_k, path_search = None, ground_y_clamp = None, ground_deploy_point = None, calibration_overrides = None))]
    fn new(
        card_names: Option<Vec<String>>,
        slot_of_k: [[i32; 3]; 2],
        path_search: Option<String>,
        ground_y_clamp: Option<String>,
        ground_deploy_point: Option<String>,
        calibration_overrides: Option<BTreeMap<String, String>>,
    ) -> PyResult<Self> {
        let (calib, calib_overrides) = match calibration_overrides {
            Some(m) if !m.is_empty() => {
                let (c, parsed) = overridden_calib(&m).map_err(PyValueError::new_err)?;
                (Some(c), parsed)
            }
            _ => (None, BTreeMap::new()),
        };
        let path_search = match path_search.as_deref() {
            None => None,
            Some(name) => Some(
                crate::state::PathSearch::from_calibration_name(name)
                    .ok_or_else(|| PyValueError::new_err(format!("path_search {name:?} has no engine implementation")))?,
            ),
        };
        let ground_y_clamp = match ground_y_clamp.as_deref() {
            None => None,
            Some(name) => Some(
                crate::state::GroundYClamp::from_calibration_name(name)
                    .ok_or_else(|| PyValueError::new_err(format!("ground_y_clamp {name:?} has no engine implementation")))?,
            ),
        };
        let ground_deploy_point = match ground_deploy_point.as_deref() {
            None => None,
            Some(name) => Some(
                crate::state::GroundDeployPoint::from_calibration_name(name)
                    .ok_or_else(|| PyValueError::new_err(format!("ground_deploy_point {name:?} has no engine implementation")))?,
            ),
        };
        let db = CardDb::load_repo().map_err(|e| PyRuntimeError::new_err(format!("cards.json: {e}")))?;
        let is_tower = |n: &str| n == KING_TOWER || n == PRINCESS_TOWER;
        let catalogue: Vec<u16> = match card_names {
            Some(names) => names
                .iter()
                .map(|n| {
                    let i = db.index(n).ok_or_else(|| {
                        let why = db.rejected.iter().find(|(r, _)| r == n).map(|(_, w)| w.as_str()).unwrap_or("not in cards.json");
                        PyValueError::new_err(format!("card {n:?} is not simulable: {why}"))
                    })?;
                    if is_tower(&db.get(i).name) {
                        return Err(PyValueError::new_err(format!("{n:?} is a crown tower, not a card")));
                    }
                    if db.get(i).summon_only {
                        return Err(PyValueError::new_err(format!("{n:?} is a unit a spell releases, not a card")));
                    }
                    Ok(i)
                })
                .collect::<PyResult<_>>()?,
            // Every REGISTERED non-tower, non-summon card: a card rejected after its
            // push (its spawned unit could not load) is in `cards` but not by name.
            None => (0..db.cards.len() as u16).filter(|i| !is_tower(&db.get(*i).name) && !db.get(*i).summon_only && db.index(&db.get(*i).name) == Some(*i)).collect(),
        };
        let mut seen = vec![false; db.cards.len()];
        for idx in &catalogue {
            if std::mem::replace(&mut seen[*idx as usize], true) {
                return Err(PyValueError::new_err(format!("card {:?} listed twice", db.get(*idx).name)));
            }
        }
        let id_of_idx = ids_of_indices(&db, &catalogue);
        Ok(Battle { cards: Arc::new(db), catalogue, id_of_idx, slot_of_k, path_search, ground_y_clamp, ground_deploy_point, calib, calib_overrides, state: None })
    }

    /// The catalogue as JSON rows [name, kind code, elixir, count, radius, flying,
    /// hitpoints at the card level battles run at]. Kind codes: module doc, SPELLS.
    fn catalogue_json(&self) -> PyResult<String> {
        catalogue_rows(&self.cards, &crate::state::Calib::shipped(), &self.catalogue, self.level()).map_err(PyValueError::new_err)
    }

    /// The unified card and tower level every battle from this object uses.
    fn card_level(&self) -> i32 {
        self.level()
    }

    /// WHICH SOURCE THIS EXTENSION WAS BUILT FROM: `(commit, tree)`, where `tree`
    /// is "clean", "dirty" or "unknown". Stamped by build.rs at compile time.
    ///
    /// The extension is loaded from the venv with no checkout around it, so without
    /// this nothing can say whether the running CODE is the code in a commit. The
    /// data side was already answerable, because the ledger is embedded and compared
    /// against the file on disk; this is the other half.
    ///
    /// "dirty" covers TRACKED modified files, which is what makes a build
    /// unreproducible in practice. An untracked file the build read would not show.
    /// "unknown" means git could not be consulted at build time and must be treated
    /// as unknown rather than as clean.
    #[staticmethod]
    fn provenance() -> (&'static str, &'static str) {
        (env!("ROYALESIM_BUILD_COMMIT"), env!("ROYALESIM_BUILD_TREE"))
    }

    /// calibration.json arena.TERRITORY_MODEL as compiled into this build.
    #[staticmethod]
    fn territory_model() -> &'static str {
        match crate::state::Calib::shipped().territory_model {
            crate::arena::TerritoryModel::EnemyTowerNoDeployRects => "enemy_tower_no_deploy_rects",
        }
    }

    /// Every crown tower's closed NoDeploySize rect, `[team][k] = [x0, y0, x1, y1]`
    /// in subtiles, engine k order (king, engine-Left, engine-Right), whether or not
    /// the tower is alive. A `team` troop may not be placed inside the rect of any
    /// ALIVE tower of the other team (and never in the river band). Sizes are this
    /// object's cards.json `no_deploy_size_tiles`; centres are the engine's tower
    /// positions.
    fn tower_no_deploy_rects(&self) -> PyResult<Vec<Vec<[i32; 4]>>> {
        let a = Arena::shipped();
        let size = |n: &str| {
            self.cards
                .index(n)
                .and_then(|i| self.cards.get(i).no_deploy_size)
                .ok_or_else(|| PyRuntimeError::new_err(format!("{n} has no no_deploy_size_tiles in cards.json")))
        };
        let (ks, ps) = (size(KING_TOWER)?, size(PRINCESS_TOWER)?);
        Ok([Team::Blue, Team::Red]
            .iter()
            .map(|t| {
                let centres = [
                    (a.king_tower_pos(*t), ks),
                    (a.princess_tower_pos(*t, crate::arena::Lane::Left), ps),
                    (a.princess_tower_pos(*t, crate::arena::Lane::Right), ps),
                ];
                centres
                    .iter()
                    .map(|(c, sz)| {
                        let r = Arena::no_deploy_rect(*c, *sz);
                        [r.min.x, r.min.y, r.max.x, r.max.y]
                    })
                    .collect()
            })
            .collect())
    }

    /// Engine tower centres [[x, y]; 3] per team, k order (king, Left, Right), for
    /// the adapter to derive `slot_of_k` from positions rather than from belief.
    #[staticmethod]
    fn tower_positions() -> Vec<Vec<(i32, i32)>> {
        let a = Arena::shipped();
        [Team::Blue, Team::Red]
            .iter()
            .map(|t| {
                let k = a.king_tower_pos(*t);
                let l = a.princess_tower_pos(*t, crate::arena::Lane::Left);
                let r = a.princess_tower_pos(*t, crate::arena::Lane::Right);
                vec![(k.x, k.y), (l.x, l.y), (r.x, r.y)]
            })
            .collect()
    }

    /// Ground passability (`Arena::is_passable_ground`) at every half-cell
    /// centre, row-major [hy][hx], 1 = passable. Cross-checked against arena.json.
    #[staticmethod]
    fn passable_half_cells() -> Vec<Vec<u8>> {
        let a = Arena::shipped();
        (0..a.rows)
            .map(|r| (0..a.cols).map(|c| u8::from(a.is_passable_ground(a.half_to_subtile_center(c, r)))).collect())
            .collect()
    }

    /// First `n` outputs of `Rng::new(seed).next_u32()` (parity with mock_engine.Pcg32).
    #[staticmethod]
    fn rng_stream(seed: u64, n: usize) -> Vec<u32> {
        let mut r = Rng::new(seed);
        (0..n).map(|_| r.next_u32()).collect()
    }

    /// Start a battle. `decks`: two lists of catalogue ids. `shuffle`: protocol
    /// ShuffleMode (0 none, 1 independent, 2 mirrored). `tower_hp[team][k]` in
    /// ENGINE k order (the adapter maps TowerSlot), 0 = destroyed. `spawns`:
    /// (team, card_id, x, y, hp or -1), materialised in the canonical order
    /// (team, own-frame y, own-frame x, card name, starting hp), never list order
    /// (state.rs `scenario_spawn_batch`). A bad spec is reported by its list entry.
    #[pyo3(signature = (seed, decks, shuffle, start_tick, elixir_milli, tower_hp, spawns))]
    #[allow(clippy::too_many_arguments)]
    fn reset(
        &mut self,
        seed: u64,
        decks: Vec<Vec<i64>>,
        shuffle: u8,
        start_tick: u32,
        elixir_milli: Option<Vec<i64>>,
        tower_hp: Option<Vec<Vec<i32>>>,
        spawns: Vec<(i64, i64, i32, i32, i32)>,
    ) -> PyResult<()> {
        if decks.len() != 2 {
            return Err(PyValueError::new_err("decks must be [blue, red]"));
        }
        let name_of = |cid: i64| -> PyResult<String> {
            usize::try_from(cid)
                .ok()
                .and_then(|c| self.catalogue.get(c))
                .map(|i| self.cards.get(*i).name.clone())
                .ok_or_else(|| PyValueError::new_err(format!("unknown card id {cid}")))
        };
        let mut named: [Vec<String>; 2] = [Vec::new(), Vec::new()];
        for (t, d) in decks.iter().enumerate() {
            named[t] = d.iter().map(|c| name_of(*c)).collect::<PyResult<_>>()?;
        }
        let mut cfg = BattleConfig::with_cards(CardDb::clone(&self.cards));
        cfg.cards = self.cards.clone();
        if let Some(c) = &self.calib {
            // THE EXPERIMENT'S CALIBRATION, applied before the narrow overrides below so
            // they still layer on top. The three fields BattleConfig copies OUT of the
            // calibration follow it, or an override of a model key would change the
            // calibration and not the model that runs.
            cfg.path_model = c.path_model;
            cfg.push_model = c.push_model;
            cfg.footprint_model = c.footprint_model;
            cfg.calib = c.clone();
        }
        if let Some(ps) = self.path_search {
            cfg.calib.path_search = ps;
            if ps == crate::state::PathSearch::TraceFittedAstar {
                // the seat-symmetric arm is the PAIR: the frame-planned search and the
                // fixed-distance knockback (tests/common symmetric_config())
                cfg.calib.knock_law = crate::state::KnockLaw::FixedDistance;
                cfg.calib.knock_stacking = crate::state::KnockStacking::VectorSum;
                cfg.calib.knock_zero_vector = crate::state::KnockZeroVector::CasterForward;
            }
        }
        if let Some(gc) = self.ground_y_clamp {
            cfg.calib.formation_ground_y_clamp = gc;
        }
        if let Some(gd) = self.ground_deploy_point {
            cfg.calib.formation_ground_deploy_point = gd;
        }
        match shuffle {
            0 => cfg.shuffle_decks = false,
            1 => cfg.shuffle_decks = true,
            2 => {
                // MIRRORED: one permutation for both decks. BattleConfig has no such
                // mode, so it is drawn here from the same generator and seed the
                // engine uses, by the same Fisher-Yates as state.rs and
                // mock_engine._shuffle. The battle Rng is then Rng::new(seed)
                // unconsumed; the engine draws nothing else from it, so no outcome
                // depends on that offset.
                let n = named[0].len();
                if named[1].len() != n {
                    return Err(PyValueError::new_err("MIRRORED shuffle needs equal deck sizes"));
                }
                let mut perm: Vec<usize> = (0..n).collect();
                let mut rng = Rng::new(seed);
                for i in (1..n).rev() {
                    let j = rng.below((i + 1) as u32) as usize;
                    perm.swap(i, j);
                }
                for d in named.iter_mut() {
                    *d = perm.iter().map(|k| d[*k].clone()).collect();
                }
                cfg.shuffle_decks = false;
            }
            other => return Err(PyValueError::new_err(format!("unknown shuffle mode {other}"))),
        }
        cfg.decks = named;
        let mut s = BattleState::try_new(seed, cfg).map_err(PyValueError::new_err)?;
        if start_tick > 0 {
            s.scenario_set_tick(start_tick);
        }
        if let Some(e) = elixir_milli {
            if e.len() != 2 {
                return Err(PyValueError::new_err("elixir_milli must have two entries"));
            }
            s.scenario_set_elixir_milli(Team::Blue, e[0]);
            s.scenario_set_elixir_milli(Team::Red, e[1]);
        }
        if let Some(hp) = tower_hp {
            if hp.len() != 2 || hp.iter().any(|r| r.len() != 3) {
                return Err(PyValueError::new_err("tower_hp must be [team][3]"));
            }
            for (row, team) in hp.iter().zip([Team::Blue, Team::Red]) {
                for (k, v) in row.iter().enumerate() {
                    s.scenario_set_tower_hp(team, k, *v).map_err(PyValueError::new_err)?;
                }
            }
        }
        // One batch, so the engine spawns in its canonical order and the LIST
        // order cannot reach team_seq (state.rs `scenario_spawn_batch`).
        let mut specs: Vec<(Team, String, Vec2, Option<i32>)> = Vec::with_capacity(spawns.len());
        for &(team, cid, x, y, hp) in &spawns {
            let t = team_of(team).ok_or_else(|| PyValueError::new_err(format!("bad spawn team {team}")))?;
            specs.push((t, name_of(cid)?, Vec2::new(x, y), (hp >= 0).then_some(hp)));
        }
        let refs: Vec<(Team, &str, Vec2, Option<i32>)> = specs.iter().map(|(t, n, p, h)| (*t, n.as_str(), *p, *h)).collect();
        s.scenario_spawn_batch(&refs).map_err(|(k, e)| {
            let (_, name, p, _) = refs[k];
            PyValueError::new_err(format!("spawn {name} at ({}, {}): {e:?}", p.x, p.y))
        })?;
        self.state = Some(s);
        Ok(())
    }

    /// PURE: the reason code `step` would give this command alone.
    fn check_deploy(&self, team: i64, slot: i64, x: i32, y: i32) -> PyResult<u8> {
        let s = self.s()?;
        Ok(check_command(s, team, slot, x, y))
    }

    /// PURE: where a building tapped at (x, y) would actually END UP, and the tile
    /// box it would take. Returns `(cx, cy, [x0, y0, x1, y1])`, or None when the
    /// tap is refused or the card is not a building.
    ///
    /// A building tap is snapped to the tile grid and, when its footprint does not
    /// fit, MOVED to a nearby legal tile rather than refused (calibration
    /// placement.ILLEGAL_TAP). So a mask that only asks "is this legal" cannot
    /// tell the caller where the building lands; this answers that, and `deploy`
    /// uses the same query, so the two cannot disagree.
    fn building_placement(&self, team: i64, card_name: &str, x: i32, y: i32) -> PyResult<Option<(i32, i32, Vec<i32>)>> {
        let s = self.s()?;
        let t = match team {
            0 => Team::Blue,
            1 => Team::Red,
            _ => return Err(PyValueError::new_err("team must be 0 or 1")),
        };
        let idx = self.cards.index(card_name).ok_or_else(|| PyValueError::new_err(format!("unknown card {card_name}")))?;
        Ok(s.building_placement(t, idx, Vec2::new(x, y)).map(|(c, b)| (c.x, c.y, vec![b.min.x, b.min.y, b.max.x, b.max.y])))
    }

    /// `apply_commands` (validate against the current state; apply accepted
    /// deploys in canonical (team, slot) order), then run up to `ticks` ticks with
    /// the GIL released, stopping at game over. Returns (card_id, reason, tick
    /// evaluated, resolved x, resolved y) per command, in input order.
    ///
    /// THE LAST TWO ARE WHERE THE CARD WENT DOWN, and they are not always the tap: a
    /// building whose footprint does not fit is MOVED to a nearby legal tile rather than
    /// refused (placement.ILLEGAL_TAP). A reward or a log keyed on the tapped point
    /// therefore describes a point with nothing on it. They are trailing elements on
    /// purpose, so a caller unpacking three keeps working.
    fn step(&mut self, py: Python<'_>, commands: Vec<Command>, ticks: u32) -> PyResult<Vec<CommandOutcome>> {
        let id_of_idx = self.id_of_idx.clone();
        let s = self.s_mut()?;
        let out = apply_commands(s, &commands, &id_of_idx).map_err(PyRuntimeError::new_err)?;
        py.allow_threads(|| {
            for _ in 0..ticks {
                if s.is_done() {
                    break;
                }
                s.tick();
            }
        });
        Ok(out)
    }

    /// protocol.py `BattleState` as JSON bytes (EntityState rows are arrays), with the
    /// spell extensions of the module doc (SPELLS).
    fn state_json<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let s = self.s()?;
        let o = state_json_text(s, &self.cards, &self.id_of_idx, &self.slot_of_k, &self.calib_overrides).map_err(PyValueError::new_err)?;
        Ok(PyBytes::new_bound(py, o.as_bytes()))
    }

    /// The calibration overrides this object applies to every battle it starts, as
    /// `{"section.KEY": json}`; empty for an unmodified engine. `build_digest` cannot
    /// report these -- it hashes the compiled-in ledger -- so this, and the
    /// `calibration_overrides` key on every frame, is where an experiment says it is one.
    fn calibration_overrides(&self) -> BTreeMap<String, String> {
        self.calib_overrides.iter().map(|(k, v)| (k.clone(), v.to_string())).collect()
    }

    /// Exact snapshot (state.rs `save`).
    fn save<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        Ok(PyBytes::new_bound(py, &self.s()?.save()))
    }

    /// Restore a snapshot against this object's card data. Every card in a hand,
    /// the queue, the pending spawns or on the board (crown towers aside) must be in
    /// the catalogue (`catalogue_violation`). A snapshot carries its own Calib, so
    /// a territory model other than this build's is refused too.
    fn load(&mut self, blob: &[u8]) -> PyResult<()> {
        let s = BattleState::load_with(blob, self.cards.clone(), Arena::shipped()).map_err(PyValueError::new_err)?;
        catalogue_violation(&s, &self.id_of_idx).map_err(PyValueError::new_err)?;
        if s.config().calib.territory_model != crate::state::Calib::shipped().territory_model {
            return Err(PyValueError::new_err("snapshot territory model differs from this build's"));
        }
        self.state = Some(s);
        Ok(())
    }

    fn state_hash(&self) -> PyResult<u64> {
        Ok(self.s()?.state_hash())
    }

    /// HIDE (Tesla): the protocol uids of every live entity that is
    /// under ground right now (entity.rs `HideState::Hidden`: untargetable and, under
    /// calibration hide.HIDDEN_IMMUNE_TO_DAMAGE, immune). Deliberately a SEPARATE
    /// accessor: the positional EntityState rows of `state_json` keep their shape,
    /// and the Python protocol grows a `hidden` field only when RoyaleGym is ready
    /// to read one. Rising buildings are not listed (they are targetable per
    /// hide.TARGETABLE_WHILE_RISING and take damage); `hide_states` has them.
    fn hidden_uids(&self) -> PyResult<Vec<i64>> {
        let s = self.s()?;
        Ok(s.entities().filter(|e| e.hidden).map(|e| (e.team_seq as i64) * 2 + e.team as i64).collect())
    }

    /// SPAWNERS: `(uid, ms_to_next, wave_left)` for every live entity whose card is a
    /// periodic spawner (Tombstone, GoblinHut, Witch, ...): ms until its next unit is
    /// queued (state.rs `spawner_pass`; a negative or zero value while deploying means
    /// "at activation") and the units of the current wave still to come. `state_json`
    /// is unchanged; a viewer reads this beside it.
    fn spawner_states(&self) -> PyResult<Vec<(i64, i32, i32)>> {
        let s = self.s()?;
        let cards = s.cards();
        Ok(s.entities()
            .filter(|e| cards.get(e.card_idx).spawner.is_some())
            .map(|e| ((e.team_seq as i64) * 2 + e.team as i64, e.spawn_ms, e.spawn_wave_left))
            .collect())
    }

    /// CHARGE (Prince, DarkPrince, BattleRam): `(uid, charged, progress)`
    /// for every live entity whose card charges -- `charged` 1 once the run-up is
    /// complete (the unit walks at ChargeSpeedMultiplier and its next landed hit is
    /// DamageSpecial), `progress` the run-up accumulated so far, RAW (its unit is
    /// calibration charge.ACCUMULATOR's: subtiles, or ms; never divide it here).
    /// `state_json` is unchanged; a viewer reads this beside it.
    fn charge_states(&self) -> PyResult<Vec<(i64, u8, i32)>> {
        let s = self.s()?;
        let cards = s.cards();
        Ok(s.entities()
            .filter(|e| cards.get(e.card_idx).charge.is_some())
            .map(|e| ((e.team_seq as i64) * 2 + e.team as i64, u8::from(e.charged), e.charge_progress))
            .collect())
    }

    /// HIDE: `(uid, state, ms)` for every live entity whose card hides -- state 0 Up,
    /// 1 Hidden, 2 Rising (entity.rs `HideState`), `ms` the state's timer in ms
    /// (UpTimeMs left while Rising; HideTimeMs left while Up; 0 while Hidden).
    fn hide_states(&self) -> PyResult<Vec<(i64, u8, i32)>> {
        let s = self.s()?;
        let cards = s.cards();
        Ok(s.entities()
            .filter(|e| cards.get(e.card_idx).hide.is_some())
            .map(|e| ((e.team_seq as i64) * 2 + e.team as i64, e.hide_state as u8, e.hide_ms))
            .collect())
    }

    /// TRACE-DIFF ENTRY POINT: every live troop as
    /// `(uid, card, x, y, deploy_ms, speed, [(col, row), ...])`, positions in
    /// SUBTILES and the route as half-tile CELLS in the order the engine stores it
    /// (GOAL-FIRST under PathModel::Oracle2026, so element 0 is the goal -- the
    /// same layout the live game publishes in `path_nodes`).
    ///
    /// WHY IT EXISTS: tools/oracle_diff.py steps this engine beside an offline-oracle
    /// trace and prints the per-tick position error and the path cells. Without the
    /// cells a divergence cannot be attributed to the search rather than to the
    /// locomotion law. `state_json` is the protocol surface and does not carry
    /// routes; this is deliberately separate and debug-only.
    #[allow(clippy::type_complexity)] // a debug tuple row; the shape is documented above
    fn debug_units(&self) -> PyResult<Vec<(i64, String, i32, i32, i32, i32, Vec<(i32, i32)>)>> {
        let s = self.s()?;
        let a = s.arena();
        Ok(s.entities()
            .filter(|e| e.kind == EntityKind::Troop)
            .map(|e| {
                let cells = e.route.iter().map(|p| a.subtile_to_half(*p)).collect();
                ((e.team_seq as i64) * 2 + e.team as i64, e.card.to_string(), e.pos.x, e.pos.y, e.deploy_ms, e.speed, cells)
            })
            .collect())
    }

    /// TEST ENTRY POINT: move a live entity (by protocol uid) by (dx, dy) subtiles. Used
    /// by the Python plants to prove the determinism tests see a one-subtile change.
    fn debug_nudge(&mut self, uid: i64, dx: i32, dy: i32) -> PyResult<bool> {
        let s = self.s_mut()?;
        let hit = s.entities().find(|e| (e.team_seq as i64) * 2 + e.team as i64 == uid).map(|e| (e.id, e.pos));
        Ok(match hit {
            Some((id, p)) => s.debug_set_pos(id, Vec2::new(p.x + dx, p.y + dy)),
            None => false,
        })
    }
}

/// The ticks a knockback still holds the entity: the slide's remaining ms in ticks
/// (knockback.DISPLACEMENT_LAW = fixed_distance), or the ladder's ticks to run --
/// one per speed value down to 0 and the back-step tick (client16402;
/// move16402.rs `PUSHBACK_DECEL`).
fn knock_ticks_left(e: &crate::state::EntityView<'_>, tick_ms: i64) -> i64 {
    if e.push_active {
        (e.push_speed / crate::move16402::PUSHBACK_DECEL) as i64 + 1
    } else {
        ceil_div(e.knock_ms as i64, tick_ms)
    }
}

/// TEST ENTRY POINT: one unit's contact-and-step update under the measured 16.402
/// law (move16402.rs), on a world handed in as plain tuples. It exists so an
/// independent implementation of the same law -- the reference Python one that
/// reproduces 99.24 % of the live unit-ticks -- can be diffed against this crate on
/// randomly generated worlds.
///
/// `bodies`: [x, y, start_x, start_y, side, r, mass, air, mover, alive, collidable,
/// offset, dir_x, dir_y, heading_counts] (15 integers, bools as 0/1) per entity in
/// update order, native units. Returns (x, y, dir_x, dir_y, offset, reached,
/// popped_waypoint).
/// The body array of `contact_step16402` / `pushback_step16402`, decoded.
fn bodies16402(bodies: &[Vec<i64>], me: usize) -> PyResult<Vec<crate::move16402::Body>> {
    use crate::move16402 as ml;
    let mut out = Vec::with_capacity(bodies.len());
    for b in bodies {
        if b.len() != 15 {
            return Err(PyValueError::new_err("each body needs 15 fields"));
        }
        out.push(ml::Body {
            x: b[0] as i32,
            y: b[1] as i32,
            start_x: b[2] as i32,
            start_y: b[3] as i32,
            side: b[4] as u8,
            r: b[5] as i32,
            mass: b[6] as i32,
            air: b[7] != 0,
            mover: b[8] != 0,
            alive: b[9] != 0,
            collidable: b[10] != 0,
            offset: b[11] as i32,
            dir: (b[12] as i32, b[13] as i32),
            heading_counts: b[14] != 0,
        });
    }
    if me >= out.len() {
        return Err(PyValueError::new_err("me out of range"));
    }
    Ok(out)
}

#[pyfunction]
#[pyo3(signature = (bodies, me, aim, speed, set_dir, seg, offset, deploying, attacking, waypoint))]
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn contact_step16402(
    bodies: Vec<Vec<i64>>,
    me: usize,
    aim: (i32, i32),
    speed: i32,
    set_dir: bool,
    seg: (i32, i32),
    offset: i32,
    deploying: bool,
    attacking: bool,
    waypoint: Option<(i32, i32)>,
) -> PyResult<(i32, i32, i32, i32, i32, bool, bool)> {
    use crate::move16402 as ml;
    let bodies = bodies16402(&bodies, me)?;
    let arena = Arena::shipped();
    let index = ml::Index::new(arena.cols, arena.rows);
    let mut scratch = Vec::new();
    let mut con = ml::Contact { acc: (0, 0), count: 0, offset };
    let mut popped = false;
    if !attacking {
        popped = ml::avoidance_scan(&index, &bodies, me, &mut con, waypoint, false, &mut scratch);
    }
    ml::decay_offset(&mut con);
    ml::separation_scan(&index, &bodies, me, &mut con, &mut scratch);
    let u = bodies[me];
    let m = ml::move_towards(
        (u.x, u.y),
        aim.0,
        aim.1,
        speed,
        set_dir,
        &mut con,
        seg,
        deploying,
        |c, r| arena.cell_bits(c, r) & arena.bit_water != 0,
        arena.cols,
        arena.rows,
    );
    let dir = m.dir.unwrap_or(u.dir);
    Ok((m.x, m.y, dir.0, dir.1, con.offset, m.reached, popped))
}

/// TEST ENTRY POINT: the arming of a knockback ladder (move16402.rs
/// `start_pushback`) on plain numbers, so an independent implementation can be
/// diffed against it: `(pos, src, strength, max_len, zero_dir)` -> `((tx, ty), v0)`
/// or None when nothing is armed.
#[pyfunction]
#[pyo3(signature = (pos, src, strength, max_len, zero_dir))]
fn start_pushback16402(pos: (i32, i32), src: (i32, i32), strength: i32, max_len: i32, zero_dir: Option<(i32, i32)>) -> Option<((i32, i32), i32)> {
    crate::move16402::start_pushback(pos, src, strength, max_len, zero_dir).map(|p| (p.target, p.speed))
}

/// TEST ENTRY POINT: one PUSHBACK TICK of unit `me` under the measured 16.402 law,
/// exactly as state.rs `phase_path16402` step 0 runs it: the separation scan and
/// the water ejection while `remaining > 0`, the countdown, the step toward
/// `target` with the avoidance `offset` rotating it. `bodies` as for
/// `contact_step16402`. Returns `(x, y, remaining, active, offset)`,
/// `active = remaining >= 0` after the tick.
#[pyfunction]
#[pyo3(signature = (bodies, me, target, remaining, offset, deploying))]
fn pushback_step16402(bodies: Vec<Vec<i64>>, me: usize, target: (i32, i32), remaining: i32, offset: i32, deploying: bool) -> PyResult<(i32, i32, i32, bool, i32)> {
    use crate::move16402 as ml;
    let mut bodies = bodies16402(&bodies, me)?;
    let arena = Arena::shipped();
    let index = ml::Index::new(arena.cols, arena.rows);
    let mut scratch = Vec::new();
    let mut con = ml::Contact { acc: (0, 0), count: 0, offset };
    let mut rem = remaining;
    let is_water = |c: i32, r: i32| arena.cell_bits(c, r) & arena.bit_water != 0;
    if rem > 0 {
        ml::separation_scan(&index, &bodies, me, &mut con, &mut scratch);
        let (x, y) = (bodies[me].x, bodies[me].y);
        // the engine's arena encoding: the water bit from arena.json, and no blocked
        // mask, because nothing there is blocked for this purpose (state.rs
        // phase_path16402)
        if !bodies[me].air && ml::blocked_or_water(x, y, arena.cols, arena.rows, arena.bit_water, 0, |c, r| arena.cell_bits(c, r)) {
            let (nx, ny) = ml::nearest_land(x, y, arena.cols, arena.rows, is_water);
            bodies[me].x = nx;
            bodies[me].y = ny;
        }
    }
    let u = bodies[me];
    let m = ml::pushback_step((u.x, u.y), target, &mut rem, &mut con, (0, 0), deploying && !u.air, is_water, arena.cols, arena.rows);
    Ok((m.x, m.y, rem, rem >= 0, con.offset))
}

/// Register the bindings on the `royalesim` module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Battle>()?;
    m.add_function(wrap_pyfunction!(contact_step16402, m)?)?;
    m.add_function(wrap_pyfunction!(start_pushback16402, m)?)?;
    m.add_function(wrap_pyfunction!(pushback_step16402, m)?)?;
    m.add("DEPLOY_REASONS", DEPLOY_REASONS.to_vec())?;
    m.add("ENTITY_FIELDS", ENTITY_FIELDS.to_vec())?;
    m.add("PROJECTILE_FIELDS", PROJECTILE_FIELDS.to_vec())?;
    m.add("EMBEDDED_CALIBRATION_JSON", EMBEDDED_CALIBRATION_JSON)?;
    m.add("EMBEDDED_ARENA_JSON", EMBEDDED_ARENA_JSON)?;
    m.add("EMBEDDED_RARITIES_CSV", EMBEDDED_RARITIES_CSV)?;
    m.add("EMBEDDED_GLOBALS_CSV", EMBEDDED_GLOBALS_CSV)?;
    m.add("SNAPSHOT_FORMAT", crate::state::SNAPSHOT_FORMAT)?;
    m.add("HAND_SIZE", HAND_SIZE)?;
    // The troop territory rule this build deploys with (calibration.json
    // arena.TERRITORY_MODEL); Battle.tower_no_deploy_rects() gives its numbers.
    m.add("TERRITORY_MODEL", Battle::territory_model())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    //! The two binding-level invariants, tested without Python: the logic lives in
    //! `catalogue_violation` and `apply_commands`, which `Battle.load` and
    //! `Battle.step` call (grep the call sites; nothing else is in between).
    use super::*;
    use crate::state::BattleConfig;

    fn cards() -> Arc<CardDb> {
        Arc::new(CardDb::load_repo().expect("data/derived/cards.json (run tools/extract_cards.py)"))
    }

    /// id_of_idx for a catalogue of every simulable non-tower card except `missing`.
    fn catalogue_without(db: &CardDb, missing: &[&str]) -> Vec<i32> {
        let mut ids = vec![-1; db.cards.len()];
        let mut next = 0;
        for (i, c) in db.cards.iter().enumerate() {
            if c.name != KING_TOWER && c.name != PRINCESS_TOWER && !missing.contains(&c.name.as_str()) {
                ids[i] = next;
                next += 1;
            }
        }
        ids
    }

    #[test]
    fn spells_are_in_the_catalogue_with_their_deploy_rule_and_show_up_in_state_json() {
        // The default catalogue carries the thin slice's five spells (and no released
        // unit); each spell row's kind code is its deploy rule; a Goblin on the board
        // reports its barrel's id; state_json carries stun / knockback ticks and the
        // spell rows, and still parses as JSON. Plants: log_territory_anywhere,
        // barrel_anywhere_incl_water (kind codes follow `deploy_rule`).
        let db = cards();
        let calib = crate::state::Calib::shipped();
        let catalogue: Vec<u16> = (0..db.cards.len() as u16).filter(|i| db.get(*i).name != KING_TOWER && db.get(*i).name != PRINCESS_TOWER && !db.get(*i).summon_only).collect();
        let rows: serde_json::Value = serde_json::from_str(&catalogue_rows(&db, &calib, &catalogue, db.lowest_level_valid_for_every_rarity()).unwrap()).unwrap();
        let row = |n: &str| rows.as_array().unwrap().iter().find(|r| r[0] == n).unwrap_or_else(|| panic!("{n} not in the catalogue")).clone();
        for (n, code) in [("Fireball", 2), ("Arrows", 2), ("Zap", 2), ("Log", 3), ("GoblinBarrel", 4), ("Knight", 0), ("Cannon", 1)] {
            assert_eq!(row(n)[1], code, "{n} kind code");
        }
        assert_eq!((row("Fireball")[3].as_i64(), row("Fireball")[6].as_i64()), (Some(0), Some(0)), "spell rows: count 0, hitpoints 0");
        assert!(rows.as_array().unwrap().iter().all(|r| r[0] != "Goblin"), "a released unit is not a card");
        let ids = ids_of_indices(&db, &catalogue);
        let goblin = db.cards.iter().position(|c| c.summon_only && c.name == "Goblin").unwrap();
        let barrel_id = catalogue.iter().position(|i| db.get(*i).name == "GoblinBarrel").unwrap() as i32;
        assert_eq!(ids[goblin], barrel_id);
        // A battle with a Goblin, a stunned unit and spells in flight.
        let mut s = battle(&db, &["Knight", "Archer", "Knight", "Archer", "Giant", "Knight", "Archer", "Knight"]);
        let at = |x: i32, y: i32| Vec2::new(crate::fixed::tiles(x), crate::fixed::tiles(y));
        s.spawn_unit(Team::Blue, "GoblinBarrel", at(9, 8), None).unwrap();
        s.scenario_spawn_now(Team::Red, "Knight", at(9, 20), None).unwrap();
        for _ in 0..40 {
            s.tick();
        }
        s.spawn_unit(Team::Blue, "Zap", at(9, 20), None).unwrap();
        s.spawn_unit(Team::Blue, "Fireball", at(9, 25), None).unwrap();
        s.tick();
        let v: serde_json::Value = serde_json::from_str(&state_json_text(&s, &db, &ids, &[[0, 1, 2], [0, 2, 1]], &BTreeMap::new()).unwrap()).unwrap();
        let ents = v["entities"].as_array().unwrap();
        // THE ROW LENGTH IS ENTITY_FIELDS'S, not a literal: the constant is what a decoder
        // checks its own field order against, so this is what keeps the two in step.
        assert!(ents.iter().all(|r| r.as_array().unwrap().len() == ENTITY_FIELDS.len()));
        // THE FOOTPRINT SPLITS BY KIND, and both halves are asserted against the
        // kind column rather than against the footprint itself. `is_null() || len
        // == 4` alone cannot fail: an engine that emitted null for everything would
        // satisfy it, which is what the first version of this did.
        let (boxed, bare): (Vec<_>, Vec<_>) = ents.iter().partition(|r| r[2].as_i64() != Some(0));
        assert!(!boxed.is_empty() && !bare.is_empty(), "this scene needs both a troop and a building for the split to mean anything");
        for r in &boxed {
            let b = r[14].as_array().unwrap_or_else(|| panic!("a building or crown tower carries no box: {r}"));
            assert_eq!(b.len(), 4, "a footprint is a closed box of four subtile bounds: {r}");
        }
        for r in &bare {
            assert!(r[14].is_null(), "a TROOP carries a footprint box, and the placement box is not a collision shape: {r}");
        }
        let gob: Vec<_> = ents.iter().filter(|r| r[3] == barrel_id).collect();
        assert_eq!(gob.len(), 3, "the barrel's Goblins report the barrel's card id");
        assert!(ents.iter().any(|r| r[12].as_i64().unwrap() > 0), "no entity reports stun ticks");
        let spells = v["spells"].as_array().unwrap();
        assert!(spells.iter().any(|r| r[1] == catalogue.iter().position(|i| db.get(*i).name == "Fireball").unwrap() as i64 && r[2] == 0), "the Fireball in flight is not reported: {spells:?}");
        assert_eq!(catalogue_violation(&reload(&db, &s), &ids), Ok(()), "a Goblin on the board is covered by its barrel's id");
    }

    fn battle(db: &Arc<CardDb>, deck: &[&str]) -> BattleState {
        let mut cfg = BattleConfig::with_cards(CardDb::clone(db));
        cfg.cards = db.clone();
        let d: Vec<String> = deck.iter().map(|s| s.to_string()).collect();
        cfg.decks = [d.clone(), d];
        BattleState::new(7, cfg)
    }

    fn reload(db: &Arc<CardDb>, s: &BattleState) -> BattleState {
        BattleState::load_with(&s.save(), db.clone(), Arena::shipped()).expect("round trip")
    }

    #[test]
    fn load_refuses_a_card_outside_the_catalogue_in_hand_queue_spawns_or_board() {
        // Plant: load_skips_queue_check (the pre-fix code checked hand slots only).
        let db = cards();
        let full = catalogue_without(&db, &[]);
        let deck = ["Knight", "Archer", "Knight", "Archer", "Giant", "Knight", "Archer", "Knight"];
        let mut s = battle(&db, &deck);
        assert_eq!(catalogue_violation(&reload(&db, &s), &full), Ok(()), "baseline must be clean");
        // Giant sits in the QUEUE only (deck position 5), never in a hand slot.
        assert!(s.hand(Team::Blue).iter().all(|c| *c != "Giant"));
        let err = catalogue_violation(&reload(&db, &s), &catalogue_without(&db, &["Giant"]));
        assert!(matches!(&err, Err(m) if m.contains("queue")), "queue card outside the catalogue passed: {err:?}");
        // A hand card.
        let err = catalogue_violation(&reload(&db, &s), &catalogue_without(&db, &["Archer"]));
        assert!(matches!(&err, Err(m) if m.contains("hand")), "{err:?}");
        // A pending spawn (placed, not yet materialised): a card in no deck at all.
        s.spawn_unit(Team::Red, "Musketeer", Vec2::new(crate::fixed::tiles(9), crate::fixed::tiles(22)), None).unwrap();
        let no_musk = catalogue_without(&db, &["Musketeer"]);
        let err = catalogue_violation(&reload(&db, &s), &no_musk);
        assert!(matches!(&err, Err(m) if m.contains("pending spawn")), "{err:?}");
        // On the board.
        s.tick();
        assert!(s.pending_spawns().is_empty());
        let err = catalogue_violation(&reload(&db, &s), &no_musk);
        assert!(matches!(&err, Err(m) if m.contains("board entity")), "{err:?}");
        // Crown towers are never catalogue cards and must not trip it.
        assert_eq!(catalogue_violation(&reload(&db, &s), &full), Ok(()));
    }

    #[test]
    fn simultaneous_commands_do_not_depend_on_their_list_order() {
        // step([blue, red]) and step([red, blue]) must be one battle. Plant:
        // command_order_matters (accepted deploys applied in list order).
        let db = cards();
        let ids = catalogue_without(&db, &[]);
        let deck = ["Archer", "Minions", "SkeletonArmy", "Knight", "Giant", "Archer", "Minions", "Musketeer"];
        let (mut a, mut b) = (battle(&db, &deck), battle(&db, &deck));
        let arena = Arena::shipped();
        let mut accepted = 0;
        for step in 0..40u32 {
            let x = [crate::fixed::tiles(4), crate::fixed::tiles(9), crate::fixed::tiles(14)][(step % 3) as usize];
            let blue = (0i64, (step % 4) as i64, x, crate::fixed::tiles(11));
            let r = arena.rotate(Vec2::new(blue.2, blue.3));
            let red = (1i64, ((step + 1) % 4) as i64, r.x, r.y);
            for st in [&mut a, &mut b] {
                // Full elixir both sides every step, identically in both battles, so
                // most commands are accepted and the order question is asked often.
                st.scenario_set_elixir_milli(Team::Blue, 10_000);
                st.scenario_set_elixir_milli(Team::Red, 10_000);
            }
            let oa = apply_commands(&mut a, &[blue, red], &ids).unwrap();
            let ob = apply_commands(&mut b, &[red, blue], &ids).unwrap();
            assert_eq!((oa[0], oa[1]), (ob[1], ob[0]), "step {step}: per-command verdicts differ");
            accepted += oa.iter().filter(|o| o.1 == R_OK).count();
            for _ in 0..8 {
                a.tick();
                b.tick();
                assert_eq!(a.state_hash(), b.state_hash(), "step {step} tick {}: command list order changed the battle", a.tick_count());
            }
        }
        assert!(accepted >= 50, "vacuous: only {accepted} deploys accepted");
    }

    /// The build stamp has a SHAPE even when git could not be consulted, so a consumer
    /// can tell "unknown" from a commit rather than getting an empty string. It cannot
    /// check the stamp is TRUE here: that needs a build from a known tree, and the
    /// install is what carries it.
    #[test]
    fn the_build_stamp_is_a_commit_or_says_it_does_not_know() {
        let (commit, tree) = (env!("ROYALESIM_BUILD_COMMIT"), env!("ROYALESIM_BUILD_TREE"));
        assert!(
            commit == "unknown" || (commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit())),
            "build commit is neither a sha nor \"unknown\": {commit:?}"
        );
        assert!(
            matches!(tree, "clean" | "dirty" | "unknown"),
            "build tree state is not one of the three: {tree:?}"
        );
    }

    #[test]
    fn a_calibration_override_changes_only_what_it_names_and_refuses_what_it_cannot() {
        let shipped = Calib::shipped();
        let one = |k: &str, v: &str| {
            let mut m = BTreeMap::new();
            m.insert(k.to_string(), v.to_string());
            overridden_calib(&m)
        };
        // THE ROUND TRIP LOSES NOTHING. Overriding a key with the value it already has
        // must give back the shipped calibration exactly; if it did not, every experiment
        // would be compared against a baseline other than the one it names.
        let lockout = shipped.deploy_lockout_ticks;
        assert_ne!(lockout, 0, "the shipped lockout is 0, so this test could not see an override land");
        let (same, _) = one("match.DEPLOY_LOCKOUT_TICKS", &lockout.to_string()).unwrap();
        assert_eq!(same, shipped, "a no-op override changed the calibration");
        // IT TAKES EFFECT, AND ON THAT KEY ALONE: put the one field back and the rest
        // must already be the shipped calibration.
        let (zero, parsed) = one("match.DEPLOY_LOCKOUT_TICKS", "0").unwrap();
        assert_eq!(zero.deploy_lockout_ticks, 0);
        let mut back = zero.clone();
        back.deploy_lockout_ticks = lockout;
        assert_eq!(back, shipped, "overriding one key moved another");
        assert_eq!(parsed["match.DEPLOY_LOCKOUT_TICKS"], serde_json::Value::from(0));
        // an arm by name
        let (arm, _) = one("combat.RETARGET_PROGRESS", "\"reset_always\"").unwrap();
        assert_eq!(arm.retarget_progress, crate::state::RetargetProgress::ResetAlways);
        assert_ne!(shipped.retarget_progress, crate::state::RetargetProgress::ResetAlways);
        // AND IT REFUSES, WITH A REASON, EVERYTHING IT CANNOT HONOUR. A typo that ran the
        // shipped value would report an experiment done that never happened.
        for (k, v, why) in [
            ("match.NOT_A_KEY", "0", "not a key in the ledger"),
            ("nodot", "0", "section.KEY"),
            ("match.DEPLOY_LOCKOUT_TICKS", "zero", "is not JSON"),
            ("combat.RETARGET_PROGRESS", "\"no_such_arm\"", "does not load"),
        ] {
            let e = one(k, v).expect_err(&format!("{k}={v} was accepted"));
            assert!(e.contains(why), "{k}={v} refused for the wrong reason: {e}");
        }
    }

    #[test]
    fn the_export_rows_match_their_field_lists_and_every_new_field_carries_something() {
        // THE LENGTH PIN. ENTITY_FIELDS and PROJECTILE_FIELDS are what a decoder refuses
        // a mismatch against, so a row that grew without its list, or a list that grew
        // without its row, has to fail here rather than decode as a shifted row elsewhere.
        //
        // AND NOT ONLY THE LENGTH. A row of the right length full of -1s and empty lists
        // passes a length check, so every new field must be SEEN carrying real data in
        // this scene: a tower's shot and a troop's shot, a target that resolves to a uid
        // on the board, a named buff with time left.
        let db = cards();
        let ids = catalogue_without(&db, &[]);
        let musketeer = ids[db.index("Musketeer").unwrap() as usize];
        assert!(musketeer >= 0);
        let mut s = battle(&db, &["Musketeer", "Knight", "Archer", "Giant", "Minions", "Cannon", "Tesla", "Zap"]);
        let at = |x: i32, y: i32| Vec2::new(crate::fixed::tiles(x), crate::fixed::tiles(y));
        // a red Knight inside the blue left princess tower's range: the TOWER's shots
        s.scenario_spawn_now(Team::Red, "Knight", at(3, 9), None).unwrap();
        // mid-field, out of every tower's range: the MUSKETEER's shots
        s.scenario_spawn_now(Team::Blue, "Musketeer", at(9, 14), None).unwrap();
        s.scenario_spawn_now(Team::Red, "Giant", at(9, 18), None).unwrap();
        // a Poison on the red Knight: a BUFF with a name and time left
        s.spawn_unit(Team::Blue, "Poison", at(3, 9), None).unwrap();
        let (mut tower_shot, mut troop_shot, mut resolved_target, mut named_buff) = (false, false, false, false);
        for _ in 0..200 {
            s.tick();
            let v: serde_json::Value =
                serde_json::from_str(&state_json_text(&s, &db, &ids, &[[0, 1, 2], [0, 2, 1]], &BTreeMap::new()).unwrap()).unwrap();
            assert!(v.get("calibration_overrides").is_none(), "an unmodified engine's frame claims overrides");
            let ents = v["entities"].as_array().unwrap();
            let uids: Vec<i64> = ents.iter().map(|r| r[0].as_i64().unwrap()).collect();
            for r in ents {
                let r = r.as_array().unwrap();
                assert_eq!(r.len(), ENTITY_FIELDS.len(), "an entity row is not ENTITY_FIELDS long: {r:?}");
                let phase = r[16].as_i64().unwrap();
                assert!((0..=2).contains(&phase), "attack_phase {phase} is not idle, windup or cooldown");
                assert_eq!(r[17].as_array().map(|f| f.len()), Some(2), "facing is not an [x, y] pair: {r:?}");
                let target = r[15].as_i64().unwrap();
                if target >= 0 {
                    assert!(uids.contains(&target), "target_uid {target} names nothing on the board");
                    resolved_target = true;
                }
                for b in r[19].as_array().unwrap() {
                    let (name, ms) = (b[0].as_str().unwrap(), b[1].as_i64().unwrap());
                    if name.split('|').any(|n| n == "Poison") && ms > 0 {
                        named_buff = true;
                    }
                }
            }
            for p in v["projectiles"].as_array().unwrap() {
                let p = p.as_array().unwrap();
                assert_eq!(p.len(), PROJECTILE_FIELDS.len(), "a projectile row is not PROJECTILE_FIELDS long: {p:?}");
                match p[7].as_i64().unwrap() {
                    -1 => tower_shot = true,
                    id if id == musketeer as i64 => troop_shot = true,
                    -2 => panic!("a projectile fired in this battle reports its firer as NOT RECORDED"),
                    _ => {}
                }
            }
        }
        assert!(tower_shot, "no projectile in 200 ticks reported a crown tower as its firer");
        assert!(troop_shot, "no projectile in 200 ticks reported the Musketeer as its firer");
        assert!(resolved_target, "no entity in 200 ticks reported a target that is on the board");
        assert!(named_buff, "no entity in 200 ticks carried a buff named Poison with time left");
    }
}
