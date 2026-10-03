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
//!       catalogue order. Spell motions and card kinds go out by name the same way
//!       (`SPELL_MOTIONS`, `CARD_KINDS`), and every positional row publishes its
//!       columns (`ENTITY_FIELDS`, `PROJECTILE_FIELDS`, `SPELL_FIELDS`, `CATALOGUE_FIELDS`),
//!       so a decoder refuses a mismatch at construction instead of reading a shifted row.
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
//! ABILITY BUTTONS
//!     A command slot in [HAND_SIZE, HAND_SIZE + ABILITY_BUTTONS) presses ability
//!     button slot - HAND_SIZE. A side's buttons are its heroes (the deck entries marked
//!     form 2 in `reset(..., forms=)`, in deck order), then its champion (a deck entry
//!     whose card has a button of its own, the Golden Knight). Its x and y are not
//!     read. The verdict is TOO_EARLY, GAME_OVER and NOT_ENOUGH_ELIXIR as for a
//!     deploy, then NO_HERO (no button there, or nothing alive behind it),
//!     ABILITY_SPENT (a hero's one charge is used) and ABILITY_NOT_READY (a champion's
//!     charge is out: its chain runs, or the charge has not come back) (state.rs
//!     `check_ability_button`). A deploying hero may press. One
//!     command per team per step still holds, so a press and a deploy of one team
//!     never share a step.
//!
//! SPELLS
//!     The catalogue carries every simulable spell (the thin slice's Fireball, Arrows,
//!     Zap, The Log, Goblin Barrel; Rocket and Freeze load too because their data has an
//!     implemented shape). `catalogue_json`'s kind code is the card's DEPLOY RULE
//!     (state.rs `deploy_rule`, the engine's one definition), numbered to match
//!     protocol.py `Placement` where a Placement exists:
//!         0 TROOP  1 BUILDING  2 SPELL (anywhere)  3 ROLLING (troop territory, over
//!         buildings: the Log, the Barbarian Barrel)  4 SPELL_NOT_ON_WATER (anywhere except
//!         water: Goblin Barrel).
//!     A spell in troop territory that keeps a troop's footprint rule (Heal: it may not
//!     stand on a building or a crown tower; a tap on an own one is moved off it as a
//!     troop's tap is, placement.SPELL_AS_DEPLOY_TAPS, and a tap nothing moves is refused)
//!     reports 0, because it is placed exactly as a troop is.
//!     Code 4 is protocol.py's SPELL_NOT_ON_WATER (a Python mask without it would offer
//!     the river to a Goblin Barrel that the engine refuses as WATER).
//!         5 TUNNEL: a card that travels under ground (the Miner, the Goblin Drill) goes
//!           down anywhere but water (placement.SPAWN_PATHFIND_TERRITORY) with its KIND's
//!           footprint rule, a troop's or a building's; the catalogue's card_kind says which.
//!         6 MIRROR: its tap follows the placement of the card it copies (`mirror_target`).
//!     All three are in the default catalogue (`card_names=None`), since RoyaleGym maps
//!     codes 5 and 6; a decoder that maps only codes 0 to 4 refuses a catalogue holding
//!     them, and a `card_names` list without them leaves them out. A VARIANT card (the Spirit Empress) reports its first form's
//!     code and row, and its forms in the catalogue's 10th element; what each hand slot
//!     costs right now is each player's `hand_costs` in `state_json`.
//!     Spell rows report count 0, radius 0, flying false, hitpoints 0 (protocol.py
//!     CardInfo: "0 for spells").
//!     A unit a spell RELEASES (the Goblin of a Goblin Barrel) is not a card: it is
//!     never in the catalogue, and `state_json` reports it under the catalogue id of
//!     the spell that releases it.
//!     `state_json` additionally carries, per entity, two row elements
//!     [stun_ticks, knockback_ticks] (ENTITY_FIELDS's columns 12 and 13, which
//!     RoyaleGym decodes; the row runs on to `level` and `mount_uid`)
//!     (ticks remaining, rounded up; under the shipped knockback ladder the ticks the
//!     ladder still runs, `knock_ticks_left`); and a top-level "spells" array of rows
//!         [team, card_id, motion, x, y, aim_x, aim_y, delay_ticks, travelled, length, hits, ticks_flown]
//!     ticks_flown: for a FLIGHT spell, the ticks it has moved (0 while it waits out delay_ticks); 0 for every other
//!     motion. The column is last, so a reader of the first eleven keeps working.
//!     motion 0 flight / 1 airborne (the Log before it lands) / 2 rolling / 3 area
//!     effect; (x, y) the current centre; aim the landing point (flight, airborne) or
//!     the roll's end point (rolling) or the centre (area); distances in subtiles.
//!
//! EVOLVED AND HERO FORMS
//!     `reset(..., forms=None)` gives each deck entry a form: two lists (blue, red) parallel to `decks`, 0 the base
//!     card, 1 its evolution, 2 its hero form; None plays every card as itself. The catalogue lists base cards only: a
//!     form is never a card of its own (`card_names` naming one raises), and its units, spells and shots report under
//!     the base card's id.
//!     An evolved entry puts its evolution down once its basic plays reach the form's cycles (two for most forms, so
//!     every third play; the card data can name another count). `state_json` gives each player an "evo" list, one row
//!     per evolved deck card in deck order:
//!         [card_id, plays since its last evolved play, 1 when its next play from the hand is evolved else 0, cycles]
//!     where cycles is the basic plays the form needs before an evolved one (state.rs `EvoCounter::cycles`), so
//!     plays / cycles is the progress towards the next evolved play. A reader of three columns keeps working.
//!     and each evolved unit's `status_flags` bit 3 (8).
//!     A hero entry puts its hero form down on every play and is one of its side's ability buttons (ABILITY
//!     BUTTONS). `state_json` gives each player an "abilities" list, one row per button, and each hero unit's
//!     `status_flags` bit 4 (16). The catalogue's `hero` column is the button's elixir.
//!
//! UNITS AND THEIR LEVELS
//!     Every entity row ends in `level` (ENTITY_FIELDS) and `mount_uid`. `level` is the unified level the entity plays
//!     at, and `mount_uid` the uid of the unit a rider rides (the Ram Rider's rider on its ram, as `rider_states` pairs
//!     them), -1 for every other entity. The level: a played unit's card
//!     level, a Mirror's copy that plus one, a Clone's copy the level spells.CLONE_LEVEL gives it (the Clone's), a
//!     unit another puts down its parent's, a crown tower its tower level. `unit_hitpoints(card_id, level)` lists
//!     every unit a card puts on the board as (role, unit name, hitpoints) at the level each takes, the card's own
//!     row first; the roles are UNIT_ROLES (`unit_hitpoint_rows` says which block is which). An entity row's `card_id`
//!     is the card whose play put the unit down, all the way down its chain: a Tombstone's Skeletons report the
//!     Tombstone and a Witch's the Witch, the Tri-Wizards' Electro and Ice Wizards the Tri-Wizards. A unit whose
//!     producer has no id in this catalogue reports the first catalogue card that can make it (`ids_of_indices`).
//!     A projectile's `firer_card_id` stays the firing unit's own card.
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
use crate::card::{CardDb, CardKind, UnitRef, KING_TOWER, PRINCESS_TOWER};
use crate::entity::EntityKind;
use crate::fixed::Vec2;
use crate::spell::SpellMotion;
use crate::state::{deploy_rule, BattleConfig, BattleState, Calib, DeployError, Outcome, SpawnPathfindDestination, ABILITY_BUTTONS, HAND_SIZE};
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
    // pair with it, and the launch recoil's old arm that pairs with that knockback, so
    // one kwarg reaches five fields.
    "path_search",
    "knock_law",
    "knock_stacking",
    "knock_zero_vector",
    "attack_pushback",
    "formation_ground_y_clamp",
    "formation_ground_deploy_point",
    // No kwarg: `calibration_overrides` reaches it, {"targeting.FIRST_TOWER_PICK": "client_spawn_lane_own_frame"}
    // (RoyaleGym rust_engine.py SYMMETRIC_ARMS selects its arms that way).
    "first_tower_pick",
    // No kwarg either: {"placement.TROOP_TOWER_TAPS": "closed_block"} (SYMMETRIC_ARMS names it too). The shipped
    // half-open king block is judged in absolute coordinates.
    "placement_troop_tower_taps",
    // Battle::new's `death_spawn_pushback` kwarg. The measured slide (client_ring_slide, shipped since the round-6 flip)
    // lays its ring in the ABSOLUTE frame for both seats (client 15.535.29, 8 runs), so a Red death is not the rotation
    // of a Blue one; not_read is its seat-symmetric old arm.
    "death_spawn_pushback",
    // Battle::new's `tap_snap` kwarg. The measured tile centre (client16402_tile_centre) snaps a tap on a tile
    // BOUNDARY up for one seat and down for the other, so a scene's boundary tap is not the rotation of its twin;
    // none, the raw tap, is the seat-symmetric old arm.
    "placement_tap_snap",
    // No kwarg: {"placement.SNAP_EVEN_CORNER": "placer_frame"}. The shipped absolute floors an even box's tap in the
    // arena's frame (client 15.535.29), so a 2x2 building is not the rotation of its twin; placer_frame is symmetric.
    "placement_snap_even",
    // No kwarg: {"placement.TROOP_BUILDING_TAPS": "not_relocated"}. The shipped as_tower_tap moves a tap off an own
    // building by the axis push, measured in arena coordinates; not_relocated is symmetric.
    "placement_troop_building_taps",
    // No kwarg: {"spells.ROLLING_HIT_SHAPE": "rect_vs_circle_edge"}. The shipped client15535_max_y_edge_open opens the
    // swept rectangle's max-y edge in ARENA coordinates (client 15.535.29), so a Log touching a victim exactly with its
    // front edge misses it rolling +y and hits it rolling -y; rect_vs_circle_edge is the same for both seats.
    "rolling_hit_shape",
];
pub const EMBEDDED_ARENA_JSON: &str = include_str!("../../../data/derived/arena.json");

/// THE OTHER TWO FILES THIS CRATE COMPILES IN, exposed for the same reason as the two
/// above and only after they had gone unwatched for months.
///
/// Five files are `include_str!`ed: calibration.json (state.rs), arena.json (arena.rs),
/// rarities.csv (card.rs), globals.csv (state.rs) and, since the wheels, cards-15.535.json
/// (card.rs `EMBEDDED_CARDS_JSON`, the table a wheel runs). Of the first four, only the first two were ever
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
/// THE DEFAULT CATALOGUE'S FIRST 133 CARDS, in the order RoyaleSim 087c060 (ship20) listed them. A catalogue id is a
/// card's place in the catalogue, so `Battle(card_names=None)` lists these first, in this order, and appends every other
/// loadable card after them in card-table order: a card that begins to load (a row the loader used to refuse, or a new
/// row) gets the next id and moves no other card's. A name here that no longer loads drops out, which does move the
/// ids after it; the list is never edited, only grown past.
pub const CATALOGUE_ORDER: &[&str] = &[
    "Knight", "Archer", "Goblins", "Giant", "Pekka", "Minions", "Balloon", "Witch", "Barbarians", "Golem",
    "Skeletons", "Valkyrie", "SkeletonArmy", "Bomber", "Musketeer", "BabyDragon", "Prince", "Wizard", "MiniPekka",
    "SpearGoblins", "GiantSkeleton", "HogRider", "MinionHorde", "IceWizard", "RoyalGiant", "SkeletonWarriors",
    "Princess", "DarkPrince", "ThreeMusketeers", "LavaHound", "IceSpirits", "FireSpirits", "Miner", "ZapMachine",
    "Bowler", "RageBarbarian", "BattleRam", "InfernoDragon", "IceGolemite", "MegaMinion", "BlowdartGoblin",
    "GoblinGang", "ElectroWizard", "AngryBarbarians", "Hunter", "AxeMan", "Assassin", "RoyalRecruits", "DarkWitch",
    "Bats", "Ghost", "RamRider", "MiniSparkys", "Rascals", "MovingCannon", "MegaKnight", "SkeletonBalloon",
    "DartBarrell", "Wallbreakers", "RoyalHogs", "GoblinGiant", "Fisherman", "EliteArcher", "ElectroDragon",
    "Firecracker", "MightyMiner", "ElixirGolem", "BattleHealer", "SkeletonKing", "ArcherQueen", "GoldenKnight",
    "SuperIceGolemite", "Monk", "SuperArcher", "RoyalRecruits_Chess", "SkeletonDragons", "SuperHogRiderTerry",
    "WitchMother", "ElectroSpirit", "ElectroGiant", "PrinceBuff", "Phoenix", "TriWizards", "GoblinDemolisher",
    "GoblinMachine", "SuspiciousBush", "SuperKnight", "SkeletonWarriors_SpookyChess", "GiantBuffer", "Berserker",
    "MergeMaiden_Normal", "MergeMaiden_Mounted", "Ronin", "Cannon", "GoblinHut", "Mortar", "InfernoTower",
    "BombTower", "BarbarianHut", "Tesla", "Elixir Collector", "Xbow", "Tombstone", "FirespiritHut",
    "BarbarianLauncher", "GoblinCage", "GoblinDrill", "GoblinPartyHut", "Fireball", "Arrows", "Rage", "Rocket",
    "GoblinBarrel", "Freeze", "Mirror", "Lightning", "Zap", "Poison", "Graveyard", "Log", "Tornado", "Clone",
    "Earthquake", "BarbLog", "Heal", "Snowball", "RoyalDelivery", "WarmSpell", "DarkMagic", "GoblinCurse",
    "MergeMaiden", "Vines", "MinionGiant",
];

pub const DEPLOY_REASONS: [&str; 19] = [
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
    // NOTHING_TO_MIRROR, index 14: a Mirror played before its side has played anything it could copy
    // (state.rs `resolve_play`, match.MIRROR_RECORD).
    "NOTHING_TO_MIRROR",
    // An ability button's own reasons (state.rs `check_ability_button`), 15 to 17: nothing alive behind the button; a
    // champion's charge out (its chain runs, or the charge has not come back; a hero never gets it: a deploying hero
    // may press, measured on client 15.535.29); a hero's one charge used.
    "NO_HERO",
    "ABILITY_NOT_READY",
    "ABILITY_SPENT",
    // CARD_PENDING, index 18: a card or button with a delayed command waiting (state.rs `command_delay_ticks`).
    "CARD_PENDING",
];

/// THE ENTITY ROW'S FIELDS, in exactly the order `state_json` writes them, named as
/// protocol.py `EntityState` names them. Published so a decoder can REFUSE a mismatch
/// instead of trusting one: rows are positional, two of the trailing fields are adjacent
/// ints, and a swap would decode without error and be drawn with confidence. The length
/// is pinned to the serializer by a test in this file, so this is the half that cannot
/// fall behind -- DEPLOY_REASONS showed what the unpinned half does.
pub const ENTITY_FIELDS: [&str; 23] = [
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
    // added 2026-09-25: the engine's own status bits (entity.rs `Entities::status_flags`):
    // bit 0 underground, bit 1 invisible to enemies, bit 2 hidden by its own hide; bit 3 an evolved unit;
    // bit 4 a hero unit
    "status_flags",
    // added 2026-09-28: the unified level the entity plays at (state.rs `EntityView::level`): a played unit's card
    // level, a Mirror's copy that plus one, a Clone's copy the Clone's (spells.CLONE_LEVEL), a unit another puts down
    // its parent's, a crown tower its tower level. Always reported (never -1).
    "level",
    // added 2026-09-28: the uid of the unit this one rides (a rider on its mount, entity.rs `attached_to`, the pairs
    // `rider_states` gives), -1 for an entity that rides nothing.
    "mount_uid",
];

/// THE PROJECTILE ROW'S FIELDS, in `state_json`'s order (its `projectiles` key). Same
/// reason as ENTITY_FIELDS, and pinned to the serializer the same way.
pub const PROJECTILE_FIELDS: [&str; 8] = ["team", "x", "y", "aim_x", "aim_y", "target_uid", "splash", "firer_card_id"];

/// THE SPELL ROW'S FIELDS, in `state_json`'s order (its `spells` key), named as protocol.py
/// `SpellState` names them. Same reason as ENTITY_FIELDS: a spell column added here and not
/// there would otherwise be dropped without a word. Pinned to the serializer by a test here.
pub const SPELL_FIELDS: [&str; 12] = ["team", "card_id", "motion", "x", "y", "aim_x", "aim_y", "delay_ticks", "travelled", "length", "hits", "ticks_flown"];

/// THE SPELL MOTIONS BY NAME, index = the `motion` code a spell row carries, named as
/// protocol.py `SpellMotion` names them, so a decoder refuses a code it has no name for at
/// construction instead of drawing it as something else. A new motion is APPENDED here with
/// its code; codes are never renumbered.
pub const SPELL_MOTIONS: [&str; 8] = ["FLIGHT", "AIRBORNE", "ROLLING", "AREA", "PULSING", "FUSE", "STRIKES", "SCHEDULED"];
const MOTION_FLIGHT: u8 = 0;
const MOTION_AIRBORNE: u8 = 1;
const MOTION_ROLLING: u8 = 2;
const MOTION_AREA: u8 = 3;
const MOTION_PULSING: u8 = 4;
const MOTION_FUSE: u8 = 5;
const MOTION_STRIKES: u8 = 6;
const MOTION_SCHEDULED: u8 = 7;

/// THE CATALOGUE ROW'S FIELDS, in `catalogue_json`'s order, named as protocol.py `CardInfo`
/// names them where it has the field (`placement` is the kind code, the card's deploy rule;
/// `card_kind` is a name from CARD_KINDS; `variants` a variant card's forms, null on every other
/// card). A variant card (the Spirit Empress) shows its first form's row with its own elixir. `hero`: the elixir its
/// hero form's button costs, null on a card without a loaded hero form. `champion`: true when a deck entry of the card
/// gets a champion's ability button (card.rs `CardDb::is_champion`). A catalogue row may grow at its end; a decoder
/// takes the columns it knows by position.
pub const CATALOGUE_FIELDS: [&str; 12] = ["name", "placement", "elixir", "count", "radius", "flying", "hitpoints", "footprint_tiles", "card_kind", "variants", "hero", "champion"];

/// WHAT A CARD IS, by name (card.rs `CardKind`), the catalogue's `card_kind` column. The
/// `placement` code says where a card may be played; it does not say what the card is, and
/// nothing makes the two agree.
pub const CARD_KINDS: [&str; 3] = ["TROOP", "BUILDING", "SPELL"];

/// A card kind's name in CARD_KINDS.
pub fn card_kind_name(kind: CardKind) -> &'static str {
    match kind {
        CardKind::Troop => CARD_KINDS[0],
        CardKind::Building => CARD_KINDS[1],
        CardKind::Spell => CARD_KINDS[2],
    }
}

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
    // The library's one implementation (state.rs Calib::shipped_with_overrides), which the
    // replay harness's --calibration-override uses too; the compiled-in ledger is the same
    // file as EMBEDDED_CALIBRATION_JSON.
    Calib::shipped_with_overrides(overrides)
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
            // 14: a Mirror with nothing to copy is refused until its side plays, a reason a caller acts on.
            DeployError::NothingToMirror => 14,
            DeployError::NoHero => 15,
            DeployError::AbilityNotReady => 16,
            DeployError::AbilitySpent => 17,
            // 18: a card or button with a delayed command waiting (`command_delay_ticks`); it clears when that runs.
            DeployError::CardPending => 18,
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

/// A `Battle::build` refusal: `Value` reaches Python as ValueError, `Runtime` as RuntimeError (`Battle::new`).
enum BuildError {
    Value(String),
    Runtime(String),
}

impl From<BuildError> for PyErr {
    fn from(e: BuildError) -> PyErr {
        match e {
            BuildError::Value(m) => PyValueError::new_err(m),
            BuildError::Runtime(m) => PyRuntimeError::new_err(m),
        }
    }
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
    /// An override of calibration spawner.DEATH_SPAWN_PUSHBACK for every battle this object starts (None = the
    /// ledger's value).
    death_spawn_pushback: Option<crate::state::DeathSpawnPushback>,
    /// An override of calibration placement.TAP_SNAP for every battle this object starts (None = the ledger's value).
    tap_snap: Option<crate::state::TapSnap>,
    /// The unified card level and the tower level every battle this object starts runs (`level` / `tower_level`;
    /// None = the lowest level valid for every rarity, and the tower level = the card level).
    level: Option<i32>,
    tower_level: Option<i32>,
    /// An EXPERIMENT's whole calibration (`calibration_overrides`), in place of the
    /// ledger's for every battle this object starts. None = the ledger.
    calib: Option<Calib>,
    /// What was overridden, parsed, for `calibration_overrides()` and for every frame.
    calib_overrides: BTreeMap<String, serde_json::Value>,
    /// THE COMMAND DELAY every battle this object starts runs (`BattleConfig::command_delay_ticks`, Blue then Red;
    /// `set_command_delay_ticks`). [0, 0], the default, runs every command at once.
    command_delay_ticks: [u32; 2],
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
/// A waiting command as `pending_commands` returns it: (kind, what, x, y, ticks left, cost).
pub type PendingRow = (String, i64, i32, i32, u32, i32);

pub fn apply_commands(
    s: &mut BattleState,
    commands: &[Command],
    id_of_idx: &[i32],
) -> Result<Vec<CommandOutcome>, String> {
    let tick = s.tick_count();
    // A button slot reports its deck entry's base card.
    let card_id = |s: &BattleState, team: i64, slot: i64| match (team_of(team), usize::try_from(slot)) {
        (Some(t), Ok(k)) if button_of(k).is_some() => s.ability_buttons(t).get(k - HAND_SIZE).map_or(-1, |b| id_of_idx[b.base as usize]),
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
        // AN ABILITY BUTTON (`button_of`): the press, reported at the hero's position.
        if let Some(b) = button_of(slot as usize) {
            let hero = s.press_ability_button(t, b).map_err(|e| format!("press validated OK was then refused: {e:?}"))?;
            let at = s.entity(hero).map_or(Vec2::new(x, y), |e| e.pos);
            out[k].3 = at.x;
            out[k].4 = at.y;
            continue;
        }
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

/// THE ABILITY BUTTON a command slot presses: slot HAND_SIZE + k is button k for k < ABILITY_BUTTONS (state.rs
/// `press_ability_button`; its x and y are not read). None for a hand slot and past the buttons.
fn button_of(slot: usize) -> Option<usize> {
    (HAND_SIZE..HAND_SIZE + ABILITY_BUTTONS).contains(&slot).then(|| slot - HAND_SIZE)
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
    if let Some(b) = button_of(slot as usize) {
        return reason_of(&s.check_ability_button(team, b));
    }
    reason_of(&s.check_deploy_slot(team, slot as usize, Vec2::new(x, y)))
}

/// The catalogue kind code of card `idx` (see the module doc, SPELLS). From
/// `state::deploy_rule`, so the code the mask is built from is the rule the engine
/// deploys by.
pub fn kind_code(cards: &CardDb, calib: &Calib, idx: u16) -> u8 {
    let c = cards.get(idx);
    // THE MIRROR (6): its tap is judged by the placement of the card it copies (match.MIRROR_PLACEMENT), which the
    // mask learns from `mirror_target`. A VARIANT card (the Spirit Empress): its first form's code; the loader holds
    // every form to one CardKind. A TUNNELLER (5): a troop or a building whose rule is anywhere but water.
    if c.is_mirror() {
        return KIND_MIRROR;
    }
    if let Some(opts) = c.variant() {
        return kind_code(cards, calib, opts[0].card);
    }
    match (c.kind, deploy_rule(calib, c)) {
        (CardKind::Troop | CardKind::Building, (Territory::AnywhereButWater, _)) => KIND_TUNNEL,
        (CardKind::Troop, _) => 0,
        (CardKind::Building, _) => 1,
        // A spell in troop territory that keeps a troop's footprint rule (Heal: `on_buildings` false) may not stand on a
        // building or a crown tower, as a troop may not: a tap on an own one is moved off it as a troop's tap is
        // (placement.SPELL_AS_DEPLOY_TAPS), and a tap nothing moves is refused. So it takes a troop's code. By the rule,
        // not the name: code 3 alone cannot tell it from the Log, which may land on buildings.
        #[cfg(not(clash_plant = "footprint_spell_kind_rolling"))]
        (CardKind::Spell, (Territory::EnemyTowerRects, true)) => 0,
        (CardKind::Spell, (Territory::EnemyTowerRects, _)) => 3,
        (CardKind::Spell, (Territory::AnywhereButWater, _)) => 4,
        (CardKind::Spell, _) => 2,
    }
}

/// The catalogue kind code of the Mirror (module doc, SPELLS).
pub const KIND_MIRROR: u8 = 6;

/// The catalogue kind code of a card that travels under ground to its tap (module doc, SPELLS).
pub const KIND_TUNNEL: u8 = 5;

/// Catalogue id per CardDb index: the catalogue position, or for a unit a catalogue
/// card puts on the board the id of the FIRST such card, or -1.
pub fn ids_of_indices(cards: &CardDb, catalogue: &[u16]) -> Vec<i32> {
    let mut id_of_idx = vec![-1; cards.cards.len()];
    for (cid, idx) in catalogue.iter().enumerate() {
        id_of_idx[*idx as usize] = cid as i32;
    }
    // A summon-only unit reports under the FIRST catalogue card that can produce it,
    // through any block `CardDb::unit_refs` names: a spell's release, a periodic
    // spawner's, a death spawn's or a second summon's unit (one Skeleton record serves
    // Skeletons, Tombstone and Witch alike). One enumeration, because a hand list here
    // once missed the formation's SECOND summon (the Rascals' RascalGirl beside the
    // RascalBoy): it reported card -1 in every entity row and firer -1 on every shot --
    // which the projectile export's contract reserves for a crown tower, so a viewer
    // drew RascalGirls' shots as tower bolts flying from mid-field.
    //
    // DOWN THE WHOLE CHAIN, ONE LEVEL AT A TIME: every catalogue card's own units first, then
    // the units THOSE put on the board, and so on (the Goblin Drill's building, then its
    // Goblins; the Elixir Golem's ElixirGolem2, then ElixirGolem4; the Phoenix's egg, then the
    // Phoenix it hatches). A unit reached deeper reports under the card at the top of the
    // first chain that reached it, and a whole level is done before the next, so no nearer
    // unit's id moves. A unit that is a record reached already is not walked again (a chain
    // that comes back on itself ends), and a catalogue card reached as a unit keeps its own id
    // and is not walked through: its own units are its own.
    let mut frontier: Vec<(i32, u16)> = catalogue.iter().enumerate().map(|(cid, idx)| (cid as i32, *idx)).collect();
    // A HERO FORM reports under its base card's id (a form-2 deck entry of the base plays it), and its units with it:
    // the forms join the first level after the catalogue's own cards, so no id reached before them moves.
    for &(base, form) in &cards.hero_forms {
        if let Some(cid) = catalogue.iter().position(|i| *i == base) {
            if id_of_idx[form as usize] == -1 {
                id_of_idx[form as usize] = cid as i32;
            }
            frontier.push((cid as i32, form));
        }
    }
    // AN EVOLVED FORM, the same way: its base card's id (a deck marks the base evolved, and every few plays of it put
    // the form down), and its units with it (the Evo Barbarians' Barbarian_EV1, the Evo Skeleton Army's General).
    for &(base, _, form) in &cards.forms {
        if let Some(cid) = catalogue.iter().position(|i| *i == base) {
            if id_of_idx[form as usize] == -1 {
                id_of_idx[form as usize] = cid as i32;
            }
            frontier.push((cid as i32, form));
        }
    }
    let mut walked: Vec<bool> = vec![false; cards.cards.len()];
    while !frontier.is_empty() {
        let mut next: Vec<(i32, u16)> = Vec::new();
        for (cid, idx) in frontier {
            // Beside the blocks `unit_refs` names, the two SPELLS a form casts of its own: the Evo Goblin Barrel's decoy
            // (`EvoDef::mirror`, whose release is its GoblinDummies) and the Hero Barbarian Barrel's re-roll log
            // (`ReRollDef::log`). A spell row reports its id in the spells list.
            let c = cards.get(idx);
            let decoy = c.evo.as_ref().and_then(|v| v.mirror);
            let log = match c.ability.as_ref().map(|a| &a.effect) {
                Some(crate::card::AbilityEffect::ReRoll(r)) => Some(r.log),
                _ => None,
            };
            for u in cards.unit_refs(idx).into_iter().map(|(_, u, _)| u).chain(decoy).chain(log) {
                // A card whose unit could not be loaded is rejected (unregistered, its
                // unit index unresolved) and never in a catalogue that came from names;
                // the by-index default catalogue below skips it too.
                let Some(slot) = id_of_idx.get_mut(u as usize) else { continue };
                if *slot == -1 {
                    *slot = cid;
                }
                if !catalogue.contains(&u) && !walked[u as usize] {
                    walked[u as usize] = true;
                    next.push((*slot, u));
                }
            }
        }
        // PLANT ids_one_level (tests/spawn_chain.rs): the first level only, so a unit a
        // loaded unit puts on the board reports card -1.
        #[cfg(clash_plant = "ids_one_level")]
        next.clear();
        frontier = next;
    }
    // AN EVOLVED FORM reports under its base card's id: its units, the barrage's bombs, the snipe's shots.
    for &(base, _, form) in &cards.forms {
        if id_of_idx.get(form as usize) == Some(&-1) {
            id_of_idx[form as usize] = id_of_idx[base as usize];
        }
    }
    id_of_idx
}

/// `Battle.catalogue_json` without Python: one row per card, its columns named by
/// CATALOGUE_FIELDS [name, kind code, elixir, count, radius, flying, hitpoints at `level`,
/// footprint tiles, card kind].
pub fn catalogue_rows(cards: &CardDb, calib: &Calib, catalogue: &[u16], level: i32) -> Result<String, String> {
    let mut out = String::from("[");
    for (k, idx) in catalogue.iter().enumerate() {
        let c = cards.get(*idx);
        let kind = kind_code(cards, calib, *idx);
        // A VARIANT card (the Spirit Empress) describes its FIRST form (the table's display row, and the form a full
        // bar plays): its count, radius, flight, hitpoints and kind. Its `elixir` stays its own, which the loader holds
        // to the first form's.
        let shown = c.variant().map_or(*idx, |opts| opts[0].card);
        let s = cards.get(shown);
        let (count, radius, flying, hp) = match s.kind {
            CardKind::Spell => (0, 0, false, 0),
            _ => (s.count, s.collision_radius, s.is_flying(), cards.scaled(shown, level, s.hitpoints)?),
        };
        if k > 0 {
            out.push(',');
        }
        let name = serde_json::to_string(&c.name).expect("string serializes");
        // The 8th element: the side of the card's placement footprint in TILES, or
        // null for a card that is not a building. A mask can then test a tap
        // before the building exists (calibration placement.FOOTPRINT_TILES).
        // A card that tunnels into a building (the Goblin Drill, whose card row is
        // its 0-radius dig) is placed on the BUILDING's footprint under
        // placement.SPAWN_PATHFIND_DESTINATION = client_tile_centre_morph_footprint,
        // so its row gives the morph's, as state.rs `building_placement` places it.
        let footprint = match s.kind {
            CardKind::Building => {
                let radius = match s.spawn_pathfind.and_then(|p| p.morph) {
                    Some(m) if calib.spawn_pathfind_destination == SpawnPathfindDestination::ClientTileCentreMorphFootprint => cards.get(m).collision_radius,
                    _ => s.collision_radius,
                };
                crate::arena::placement_tiles(radius).to_string()
            }
            _ => "null".to_string(),
        };
        // The 9th element: what the card IS (CARD_KINDS), which the kind code above does not say; a variant card's is
        // its forms'.
        let card_kind = card_kind_name(s.kind);
        // The 10th element: a variant card's forms, [[trigger_milli, form catalogue id or -1, form elixir], ...] in
        // the table's order (descending triggers), so a consumer without an engine can price the play from the elixir
        // it counts (match.VARIANT_TRIGGER_COMPARE); null on every other card.
        let variants = match c.variant() {
            None => "null".to_string(),
            Some(opts) => {
                let rows: Vec<String> = opts
                    .iter()
                    .map(|o| format!("[{},{},{}]", o.trigger_milli, catalogue.iter().position(|i| *i == o.card).map_or(-1, |p| p as i64), cards.get(o.card).elixir))
                    .collect();
                format!("[{}]", rows.join(","))
            }
        };
        // The 11th element: the elixir the card's hero form's button costs (a form-2 deck entry plays that form), null
        // on every card without one.
        let hero = cards.form_card(*idx, crate::card::FORM_HERO).and_then(|f| cards.get(f).ability.as_ref()).map_or("null".to_string(), |a| a.cost.to_string());
        // The 12th: whether the card is a champion (`CardDb::is_champion`).
        let champion = cards.is_champion(*idx);
        let _ = write!(out, "[{name},{kind},{},{count},{radius},{flying},{hp},{footprint},\"{card_kind}\",{variants},{hero},{champion}]", c.elixir);
    }
    out.push(']');
    Ok(out)
}

/// THE ROLES `unit_hitpoint_rows` names, in the order a card's own row and its blocks come.
pub const UNIT_ROLES: [&str; 5] = ["own", "second_summon", "spawn", "death_spawn", "release"];

/// `Battle.unit_hitpoints` without Python: EVERY UNIT CARD `idx` PUTS ON THE BOARD played at unified `level`, as
/// (role, unit name, hitpoints), each unit at the level it takes (`CardDb::unit_level`, `CardDb::spawn_level`, the
/// walk `CardDb::check_levels` makes) and its hitpoints on its own ladder there (`CardDb::scaled` on `cards`, which the
/// caller hands in as the battle's card data: the card-value overlay applied, `Calib::card_data`).
///
/// The card's OWN row comes first: ("own", its unit's row name, its hitpoints at `level`), the unit the catalogue's
/// hitpoints column shows. A variant card (the Spirit Empress) has one own row per form, in its forms' order; a spell
/// has none (it puts no unit of its own down: a Goblin Barrel lists its Goblins, a Fireball nothing). Then, down the
/// whole chain in `CardDb::unit_refs` order (each record's own units right after it), one row per unit, its role the
/// block that puts it down:
///   * second_summon: a second summon, a member at an offset, an attached rider, a deploy spawn area's unit (the Tri
///     Wizards' Electro Wizard and Ice Wizard, at the card's level);
///   * spawn: a periodic spawner's, a life-state controller's wave, the building a tunneller leaves;
///   * death_spawn: a death spawn, a death projectile's release, a death area's schedule, a hung buff's death spawn;
///   * release: a spell's release or summon, a spell's or a shot's scheduled area.
///
/// A unit reached twice at one level by one role is listed once (a Tombstone's Skeleton is a spawn and a death spawn:
/// two rows). Not listed: a death bomb (a timed impact, never a unit on the board; a container's units are), a
/// transformation's row (the unit keeps its own hitpoints when it turns), and a hung buff's death spawn under
/// status.BUFF_DEATH_SPAWN_LEVEL = victim_level (its level is the dying enemy's). Err, naming the level, for a level the
/// card's ladder lacks, and for any unit of the chain whose ladder lacks the level it takes.
pub fn unit_hitpoint_rows(cards: &CardDb, calib: &Calib, idx: u16, level: i32) -> Result<Vec<(&'static str, String, i32)>, String> {
    cards.level_multiplier(idx, level).map_err(|e| format!("level {level}: {e}"))?;
    let c = cards.get(idx);
    let owns: Vec<u16> = match c.variant() {
        Some(opts) => opts.iter().map(|o| o.card).collect(),
        None => vec![idx],
    };
    let mut out: Vec<(&'static str, String, i32)> = Vec::new();
    let mut walked: Vec<(u16, i32)> = Vec::new();
    for &own in &owns {
        let o = cards.get(own);
        walked.push((own, level));
        if o.kind != CardKind::Spell && o.death_bomb_fuse_ms().is_none() {
            let hp = cards.scaled(own, level, o.hitpoints).map_err(|e| format!("level {level}: {e}"))?;
            out.push((UNIT_ROLES[0], o.unit_name.clone(), hp));
        }
    }
    for own in owns {
        unit_rows_below(cards, calib, own, level, &mut walked, &mut out).map_err(|e| format!("level {level}: {e}"))?;
    }
    Ok(out)
}

/// `unit_hitpoint_rows` below record `idx` at `level`: each unit it names, at the level it gives that unit, then that
/// unit's own, once per (record, level) walked.
fn unit_rows_below(cards: &CardDb, calib: &Calib, idx: u16, level: i32, walked: &mut Vec<(u16, i32)>, out: &mut Vec<(&'static str, String, i32)>) -> Result<(), String> {
    let c = cards.get(idx);
    // A scheduled entry is its spell's (or its shot's) release, or its death area's death spawn: one block holds them.
    let scheduled_role = if c.death_area_effect.as_ref().and_then(|d| d.shape.schedule()).is_some() { "death_spawn" } else { "release" };
    let source_level = calib.buff_death_spawn_level == crate::state::BuffDeathSpawnLevel::SourceLevel;
    for (path, unit, level_index) in cards.unit_refs(idx) {
        let role = match path {
            UnitRef::SecondSummon | UnitRef::SummonMember(_) | UnitRef::Attach | UnitRef::DeploySpawn(_) => Some("second_summon"),
            UnitRef::Spawner | UnitRef::LifeState | UnitRef::Morph | UnitRef::AbilityUnit | UnitRef::EvoUnit(_) => Some("spawn"),
            UnitRef::DeathSpawn | UnitRef::DeathProjectile => Some("death_spawn"),
            UnitRef::BuffDeathSpawn => source_level.then_some("death_spawn"),
            UnitRef::SpellRelease | UnitRef::SpellSummon => Some("release"),
            UnitRef::Scheduled(_) => Some(scheduled_role),
            // The same entity in another row: no unit of its own, but what that row puts down is walked.
            UnitRef::Transform => None,
            // A form is an own row (`unit_hitpoint_rows`).
            UnitRef::VariantForm(_) => continue,
        };
        if role.is_none() && path == UnitRef::BuffDeathSpawn {
            continue;
        }
        let at = match path {
            UnitRef::SpellRelease => cards.spawn_level(idx, level)?,
            _ => cards.unit_level(idx, unit, level_index, level)?,
        };
        // A block that names the record itself at its own level (a deploy's member 0, the Tri Wizards' TriWizard) is
        // that record's row, already listed.
        if (unit, at) == (idx, level) {
            continue;
        }
        let u = cards.get(unit);
        if let Some(role) = role {
            if u.death_bomb_fuse_ms().is_none() {
                let row = (role, u.unit_name.clone(), cards.scaled(unit, at, u.hitpoints)?);
                if !out.contains(&row) {
                    out.push(row);
                }
            }
        }
        if !walked.contains(&(unit, at)) {
            walked.push((unit, at));
            unit_rows_below(cards, calib, unit, at, walked, out)?;
        }
    }
    Ok(())
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
    // The regen's multiple the next tick runs (state.rs `elixir_multiplier`: 1, 2, or 3 from 60 s into overtime).
    let rate = s.elixir_multiplier();
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
        rate,
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
        // THE HAND'S COSTS (state.rs `hand_costs`: what each slot's play debits now, -1 where no play resolves) and the
        // card a Mirror would copy (`mirror_target`, its catalogue id or -1): keyed, so a decoder that does not know
        // them drops them (the module doc's msgspec rule).
        let hc = s.hand_costs(team);
        let mirror_target = s.mirror_target(team).map_or(-1, |i| id_of_idx.get(i as usize).copied().unwrap_or(-1));
        let _ = write!(o, "],\"hand_costs\":[{},{},{},{}],\"mirror_target\":{mirror_target}", hc[0], hc[1], hc[2], hc[3]);
        // THE SIDE'S WAITING COMMANDS (state.rs `pending_commands`, `BattleConfig::command_delay_ticks`), keyed like the
        // two above: [kind, what, x, y, ticks left, cost] each, kind 0 a play (what: the catalogue card id, x y the tap)
        // and 1 a press (what: the button's action slot). The elixir above is the bar's state, which a waiting command
        // has not touched (measured on the live client: it changes when the command runs); `pending_cost` is what the
        // waiting commands have spoken for. Empty and 0 with no delay.
        let now = s.tick_count();
        let pend = s.pending_commands(team);
        let _ = write!(o, ",\"pending_cost\":{},\"pending\":[", pend.iter().map(|c| c.cost).sum::<i32>());
        for (k, c) in pend.iter().enumerate() {
            let left = c.due.saturating_sub(now);
            let (kind, what, x, y) = match c.kind {
                crate::state::CommandKind::Deploy { card, pos } => (0, id_of_idx.get(card as usize).copied().unwrap_or(-1) as i64, pos.x, pos.y),
                crate::state::CommandKind::Ability { button } => (1, (HAND_SIZE + button) as i64, 0, 0),
            };
            let _ = write!(o, "{}[{kind},{what},{x},{y},{left},{}]", if k > 0 { "," } else { "" }, c.cost);
        }
        o.push(']');
        // THE EVOLVED DECK CARDS (module doc, EVOLVED AND HERO FORMS): [card id, plays, next play evolved, cycles], keyed like the
        // two above.
        o.push_str(",\"evo\":[");
        for (k, c) in s.evo_counters(team).iter().enumerate() {
            if k > 0 {
                o.push(',');
            }
            let id = id_of_idx.get(c.card as usize).copied().unwrap_or(-1);
            let _ = write!(o, "[{id},{},{},{}]", c.plays, u8::from(c.next_evolved()), c.cycles);
        }
        o.push(']');
        // THE SIDE'S ABILITY BUTTONS (state.rs `ability_buttons`), one row per button in button order (the form-2 deck
        // entries, then the champion entries): [available, spent, cost, card_id, cooldown_ticks] -- available 1 when a
        // press would be taken but for the elixir, spent 1 when a hero has used its one charge (a champion's comes back,
        // so it is never spent), cost the press's elixir, card_id the button's base card, cooldown_ticks the ticks until
        // a champion's used charge is back (0 otherwise). Keyed, so a decoder that does not know it drops it.
        o.push_str(",\"abilities\":[");
        for (k, b) in s.ability_buttons(team).iter().enumerate() {
            if k > 0 {
                o.push(',');
            }
            let card_id = id_of_idx.get(b.base as usize).copied().unwrap_or(-1);
            let _ = write!(o, "[{},{},{},{card_id},{}]", u8::from(b.available), u8::from(b.spent), b.cost, b.cooldown_ticks);
        }
        o.push(']');
        let _ = write!(
            o,
            ",\"next_card\":{next},\"crowns\":{},\"tower_hp\":[{},{},{}],\"tower_max_hp\":[{},{},{}],\"king_active\":{}}}",
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
            // THE PRODUCING CARD (entity.rs `source`): a unit reports the card whose play put it down, so a
            // Tombstone's Skeletons report the Tombstone, not the first card that can make a Skeleton. The static
            // first-producer id (`ids_of_indices`) where the producer has no id in this catalogue.
            None => match id_of_idx.get(e.source as usize).copied() {
                Some(by) if by >= 0 => (by, -1),
                _ => (id_of_idx[e.card_idx as usize], -1),
            },
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
        let mount_uid = e.attached_to.and_then(|m| s.entity(m)).map(|m| (m.team_seq as i64) * 2 + m.team as i64).unwrap_or(-1);
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
            "[{uid},{ti},{},{card_id},{slot},{},{},{},{},{},{},{},{},{},{footprint},{target_uid},{},[{},{}],{},{buffs},{},{},{mount_uid}]",
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
            e.status_flags,
            e.level,
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
            SpellMotion::Flight { pos, aim, delay_ms, .. } => (MOTION_FLIGHT, *pos, *aim, *delay_ms, 0, 0, 0),
            SpellMotion::Airborne { pos, aim, .. } => (MOTION_AIRBORNE, *pos, *aim, 0, 0, 0, 0),
            SpellMotion::Rolling { pos, travelled, len, hit } => (MOTION_ROLLING, *pos, Vec2::new(pos.x, pos.y + fwd * (len - travelled)), 0, *travelled, *len, hit.len()),
            // a capturing ball (the Evo Giant Snowball's): a rolling spell whose hits are its captives
            SpellMotion::CaptureRoll { pos, travelled, captives, .. } => (MOTION_ROLLING, *pos, *pos, 0, *travelled, 0, captives.len()),
            SpellMotion::Area { pos } => (MOTION_AREA, *pos, *pos, 0, 0, 0, 0),
            // a pulsing area: `delay_ms` carries its remaining life, so the viewer can
            // draw a Poison cloud shrinking rather than a one-frame flash.
            SpellMotion::Pulsing(p) => (MOTION_PULSING, p.pos, p.pos, p.life_ms, 0, 0, 0),
            // a bottle: its delay is the fuse still to run
            SpellMotion::Fuse { pos, ms } => (MOTION_FUSE, *pos, *pos, *ms, 0, 0, 0),
            // a striking area: its delay is the time to its next strike, its hits the enemies struck
            SpellMotion::Strikes { pos, next_ms, k, struck, .. } => (MOTION_STRIKES, *pos, *pos, *next_ms, *k as i32, 0, struck.len()),
            // a scheduled area (the Graveyard): its travelled is the number of entries it has put down
            SpellMotion::Scheduled { pos, fired, .. } => (MOTION_SCHEDULED, *pos, *pos, 0, fired.count_ones() as i32, 0, 0),
            // an area riding on a unit (a hero's button): a pulsing area whose centre moves, its delay its life left
            SpellMotion::Attached { pos, life_ms, .. } => (MOTION_PULSING, *pos, *pos, *life_ms, 0, 0, 0),
        };
        let _ = write!(
            o,
            "[{},{card_id},{motion},{},{},{},{},{},{travelled},{len},{hits},{}]",
            sp.team as u8,
            pos.x,
            pos.y,
            aim.x,
            aim.y,
            ceil_div(delay_ms.max(0) as i64, tick_ms),
            if motion == MOTION_FLIGHT { sp.flown } else { 0 },
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
        // -1 A CROWN TOWER, decided from the firer's own card and never from a failed
        // lookup: -1 had meant "id_of_idx has no entry", which is also every unit the
        // catalogue failed to map, and read as "a tower fired this" for all of them.
        // -2 anything else without an id: NOT RECORDED (restored from a snapshot older
        // than the field) or a card the catalogue does not carry.
        let firer = match p.firer_card {
            Some(c) if matches!(cards.get(c).name.as_str(), KING_TOWER | PRINCESS_TOWER) => -1,
            Some(c) => match id_of_idx.get(c as usize).copied() {
                Some(id) if id >= 0 => id,
                _ => -2,
            },
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
    /// THE CONSTRUCTOR'S BODY (`new`, whose doc says what each argument selects), with no Python type in it, so the
    /// crate's own unit tests can build a `Battle` without linking libpython: the extension-module feature leaves those
    /// symbols to the interpreter that loads the module, which a Linux test binary does not have.
    #[allow(clippy::too_many_arguments)]
    fn build(
        card_names: Option<Vec<String>>,
        slot_of_k: [[i32; 3]; 2],
        path_search: Option<String>,
        ground_y_clamp: Option<String>,
        ground_deploy_point: Option<String>,
        calibration_overrides: Option<BTreeMap<String, String>>,
        death_spawn_pushback: Option<String>,
        level: Option<i32>,
        tower_level: Option<i32>,
        tap_snap: Option<String>,
    ) -> Result<Self, BuildError> {
        let (calib, calib_overrides) = match calibration_overrides {
            Some(m) if !m.is_empty() => {
                let (c, parsed) = overridden_calib(&m).map_err(BuildError::Value)?;
                (Some(c), parsed)
            }
            _ => (None, BTreeMap::new()),
        };
        let path_search = match path_search.as_deref() {
            None => None,
            Some(name) => Some(
                crate::state::PathSearch::from_calibration_name(name)
                    .ok_or_else(|| BuildError::Value(format!("path_search {name:?} has no engine implementation")))?,
            ),
        };
        let ground_y_clamp = match ground_y_clamp.as_deref() {
            None => None,
            Some(name) => Some(
                crate::state::GroundYClamp::from_calibration_name(name)
                    .ok_or_else(|| BuildError::Value(format!("ground_y_clamp {name:?} has no engine implementation")))?,
            ),
        };
        let ground_deploy_point = match ground_deploy_point.as_deref() {
            None => None,
            Some(name) => Some(
                crate::state::GroundDeployPoint::from_calibration_name(name)
                    .ok_or_else(|| BuildError::Value(format!("ground_deploy_point {name:?} has no engine implementation")))?,
            ),
        };
        let death_spawn_pushback = match death_spawn_pushback.as_deref() {
            None => None,
            Some(name) => Some(
                crate::state::DeathSpawnPushback::from_calibration_name(name)
                    .ok_or_else(|| BuildError::Value(format!("death_spawn_pushback {name:?} has no engine implementation")))?,
            ),
        };
        let tap_snap = match tap_snap.as_deref() {
            None => None,
            Some(name) => Some(
                crate::state::TapSnap::from_calibration_name(name)
                    .ok_or_else(|| BuildError::Value(format!("tap_snap {name:?} has no engine implementation")))?,
            ),
        };
        let db = CardDb::load_repo().map_err(|e| BuildError::Runtime(format!("cards.json: {e}")))?;
        let is_tower = |n: &str| n == KING_TOWER || n == PRINCESS_TOWER;
        let catalogue: Vec<u16> = match card_names {
            Some(names) => names
                .iter()
                .map(|n| {
                    let i = db.index(n).ok_or_else(|| {
                        let why = db.rejected.iter().find(|(r, _)| r == n).map(|(_, w)| w.as_str()).unwrap_or("not in cards.json");
                        BuildError::Value(format!("card {n:?} is not simulable: {why}"))
                    })?;
                    if is_tower(&db.get(i).name) {
                        return Err(BuildError::Value(format!("{n:?} is a crown tower, not a card")));
                    }
                    if db.get(i).form_of.is_some() {
                        return Err(BuildError::Value(format!("{n:?} is a hero form: name its base card and mark it with form 2 in reset's forms")));
                    }
                    if db.get(i).summon_only {
                        return Err(BuildError::Value(format!("{n:?} is a unit a spell releases, not a card")));
                    }
                    if db.is_form(i) {
                        return Err(BuildError::Value(format!("{n:?} is an evolved form: list its base card and mark it in reset's forms")));
                    }
                    Ok(i)
                })
                .collect::<Result<_, BuildError>>()?,
            // Every REGISTERED non-tower, non-summon card: a card rejected after its
            // push (its spawned unit could not load) is in `cards` but not by name. The
            // Mirror (code 6) and the tunnellers (code 5) are in it (module doc, SPELLS).
            // In CATALOGUE_ORDER first, every other one after (its doc).
            None => {
                let loadable: Vec<u16> = (0..db.cards.len() as u16)
                    .filter(|i| {
                        let c = db.get(*i);
                        !is_tower(&c.name)
                            && !c.summon_only
                            && c.evo.is_none()
                            && db.index(&c.name) == Some(*i)
                    })
                    .collect();
                let pinned: Vec<u16> = CATALOGUE_ORDER.iter().filter_map(|n| loadable.iter().copied().find(|i| db.get(*i).name == *n)).collect();
                let rest = loadable.iter().copied().filter(|i| !pinned.contains(i));
                pinned.iter().copied().chain(rest).collect()
            }
        };
        let mut seen = vec![false; db.cards.len()];
        for idx in &catalogue {
            if std::mem::replace(&mut seen[*idx as usize], true) {
                return Err(BuildError::Value(format!("card {:?} listed twice", db.get(*idx).name)));
            }
        }
        let card_lvl = level.unwrap_or_else(|| db.lowest_level_valid_for_every_rarity());
        for idx in &catalogue {
            db.check_levels(*idx, card_lvl).map_err(|e| BuildError::Value(format!("level {card_lvl}: {e}")))?;
        }
        let tower_lvl = tower_level.unwrap_or(card_lvl);
        for name in [KING_TOWER, PRINCESS_TOWER] {
            let idx = db.index(name).ok_or_else(|| BuildError::Runtime(format!("cards.json has no {name}")))?;
            db.check_levels(idx, tower_lvl).map_err(|e| BuildError::Value(format!("tower_level {tower_lvl}: {e}")))?;
        }
        let id_of_idx = ids_of_indices(&db, &catalogue);
        Ok(Battle { cards: Arc::new(db), catalogue, id_of_idx, slot_of_k, path_search, ground_y_clamp, ground_deploy_point, death_spawn_pushback, tap_snap, level, tower_level, calib, calib_overrides, command_delay_ticks: [0, 0], state: None })
    }

    /// `catalogue_json`'s body, with no Python type in it (`build` says why).
    fn catalogue_text(&self) -> Result<String, String> {
        let calib = self.battle_calib()?;
        let cards = calib.card_data(self.cards.clone())?;
        #[cfg(clash_plant = "catalogue_reads_shipped_calib")]
        let calib = crate::state::Calib::shipped(); // PLANT: the rows read the shipped ledger.
        catalogue_rows(&cards, &calib, &self.catalogue, self.level())
    }

    fn s(&self) -> PyResult<&BattleState> {
        self.state.as_ref().ok_or_else(|| PyRuntimeError::new_err("Battle.reset() has not been called"))
    }

    fn s_mut(&mut self) -> PyResult<&mut BattleState> {
        self.state.as_mut().ok_or_else(|| PyRuntimeError::new_err("Battle.reset() has not been called"))
    }

    fn level(&self) -> i32 {
        self.level.unwrap_or_else(|| self.cards.lowest_level_valid_for_every_rarity())
    }

    fn tower_lvl(&self) -> i32 {
        self.tower_level.unwrap_or_else(|| self.level())
    }

    /// The calibration every battle this object starts runs (`selected_calib`).
    fn battle_calib(&self) -> Result<Calib, String> {
        selected_calib(self.calib.as_ref(), self.path_search, self.ground_y_clamp, self.ground_deploy_point, self.death_spawn_pushback, self.tap_snap)
    }
}

/// THE CALIBRATION A `Battle` RUNS: the experiment's (`calibration_overrides`) or the ledger's, then the arms the
/// constructor's keywords select. `path_search` = trace_fitted_astar selects the seat-symmetric PAIR: the
/// frame-planned search and the fixed-distance knockback (knockback.DISPLACEMENT_LAW = fixed_distance with
/// vector_sum and caster_forward, tests/common `symmetric_config()`). With it goes knockback.ATTACK_PUSHBACK's old
/// arm, none: the launch recoil IS the 16.402 ladder, which has no code under fixed_distance, so the loader refuses
/// ladder_away_from_target there. The result is checked as the loader checks a ledger (`Calib::validate`), so no
/// keyword can hand a battle a calibration the file could not.
pub fn selected_calib(
    base: Option<&Calib>,
    path_search: Option<crate::state::PathSearch>,
    ground_y_clamp: Option<crate::state::GroundYClamp>,
    ground_deploy_point: Option<crate::state::GroundDeployPoint>,
    death_spawn_pushback: Option<crate::state::DeathSpawnPushback>,
    tap_snap: Option<crate::state::TapSnap>,
) -> Result<Calib, String> {
    let mut c = base.cloned().unwrap_or_else(Calib::shipped);
    if let Some(ps) = path_search {
        c.path_search = ps;
        if ps == crate::state::PathSearch::TraceFittedAstar {
            c.knock_law = crate::state::KnockLaw::FixedDistance;
            c.knock_stacking = crate::state::KnockStacking::VectorSum;
            c.knock_zero_vector = crate::state::KnockZeroVector::CasterForward;
            #[cfg(not(clash_plant = "symmetric_selection_keeps_attack_recoil"))]
            {
                c.attack_pushback = crate::state::AttackPushback::None;
            }
        }
    }
    if let Some(gc) = ground_y_clamp {
        c.formation_ground_y_clamp = gc;
    }
    if let Some(gd) = ground_deploy_point {
        c.formation_ground_deploy_point = gd;
    }
    if let Some(dp) = death_spawn_pushback {
        c.death_spawn_pushback = dp;
    }
    if let Some(ts) = tap_snap {
        c.placement_tap_snap = ts;
    }
    #[cfg(not(clash_plant = "symmetric_selection_unchecked"))]
    c.validate()?;
    Ok(c)
}

#[pymethods]
impl Battle {
    /// `card_names`: the catalogue, in card-id order (None = every simulable
    /// non-tower card in cards.json order, the Mirror (code 6) and the cards that
    /// travel under ground (the Miner, the Goblin Drill, code 5) included; module doc,
    /// SPELLS). `slot_of_k[team][k]` names engine tower
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
    /// parity, the water teleport's tie), so the seat-symmetric arm is the pair. The
    /// launch recoil (knockback.ATTACK_PUSHBACK) runs on that ladder, so it goes too:
    /// under this selection a Sparky and a Firecracker do not recoil (`selected_calib`).
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
    ///
    /// `death_spawn_pushback`: None = the ledger's spawner.DEATH_SPAWN_PUSHBACK. Its measured arm,
    /// "client_ring_slide", lays the slide's ring in the ARENA's frame for both seats (measured on side 0),
    /// so a Red death is not the rotation of a Blue one. "not_read" is the rotation-symmetric arm a rotation
    /// gate wants. Callers pass the first five by position.
    ///
    /// `level`: the unified card level of both sides, for every battle this object starts; None = the lowest level
    /// valid for every rarity (11 on 15.535.29). `tower_level`: the crown towers' level; None = `level`. Checked
    /// here against every catalogue card (and the units it makes) and the two towers, so a level a card's ladder
    /// lacks is refused now, not at a spawn.
    ///
    /// `tap_snap`: None = the ledger's placement.TAP_SNAP. Its measured arm, "client16402_tile_centre", takes a troop
    /// or spell tap at its tile's centre, so a tap on a tile BOUNDARY snaps up for one seat and down for the other and
    /// a scene's boundary tap is not the rotation of its twin (the client's taps are tile indices, never on one).
    /// "none", the raw tap, is the arm a rotation gate wants. This is the LAST argument, after `level` and
    /// `tower_level`.
    #[new]
    #[pyo3(signature = (card_names, slot_of_k, path_search = None, ground_y_clamp = None, ground_deploy_point = None, calibration_overrides = None, death_spawn_pushback = None, level = None, tower_level = None, tap_snap = None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        card_names: Option<Vec<String>>,
        slot_of_k: [[i32; 3]; 2],
        path_search: Option<String>,
        ground_y_clamp: Option<String>,
        ground_deploy_point: Option<String>,
        calibration_overrides: Option<BTreeMap<String, String>>,
        death_spawn_pushback: Option<String>,
        level: Option<i32>,
        tower_level: Option<i32>,
        tap_snap: Option<String>,
    ) -> PyResult<Self> {
        Ok(Self::build(card_names, slot_of_k, path_search, ground_y_clamp, ground_deploy_point, calibration_overrides, death_spawn_pushback, level, tower_level, tap_snap)?)
    }

    /// The catalogue as JSON rows [name, kind code, elixir, count, radius, flying,
    /// hitpoints at the card level battles run at]. Kind codes: module doc, SPELLS.
    /// The hitpoints are the card data this object's battles run: cards.CLIENT16402_VALUES
    /// under the calibration they run under (`reset`), so a row agrees with a spawn. The
    /// kind codes and footprints are read under that calibration too (a Goblin Drill's
    /// footprint follows placement.SPAWN_PATHFIND_DESTINATION), overrides included.
    fn catalogue_json(&self) -> PyResult<String> {
        self.catalogue_text().map_err(PyValueError::new_err)
    }

    /// EVERY UNIT A CARD PUTS ON THE BOARD, WITH ITS HITPOINTS: `unit_hitpoints(card_id, level)` for the catalogue's
    /// `card_id` played at unified `level`, a list of (role, unit name, hitpoints) tuples. The card's own row comes
    /// first, ("own", its unit's row name, the hitpoints the catalogue shows at that level); then every unit it puts
    /// down, down the whole chain, each at the level it takes, role one of "second_summon", "spawn", "death_spawn" or
    /// "release" (module-level `unit_hitpoint_rows` says which block is which, and what is left out). The hitpoints are
    /// this object's battles' card data: the calibration they run under, cards.CLIENT16402_VALUES included, as
    /// `catalogue_json`'s are. ValueError for an unknown card id, and for a level the card's ladder (or a unit's in
    /// its chain) lacks, naming the level. Needs no `reset`.
    fn unit_hitpoints(&self, card_id: i64, level: i32) -> PyResult<Vec<(String, String, i32)>> {
        let idx = usize::try_from(card_id)
            .ok()
            .and_then(|c| self.catalogue.get(c))
            .copied()
            .ok_or_else(|| PyValueError::new_err(format!("unknown card id {card_id}")))?;
        let calib = self.battle_calib().map_err(PyValueError::new_err)?;
        let cards = calib.card_data(self.cards.clone()).map_err(PyValueError::new_err)?;
        let rows = unit_hitpoint_rows(&cards, &calib, idx, level).map_err(PyValueError::new_err)?;
        Ok(rows.into_iter().map(|(r, n, hp)| (r.to_string(), n, hp)).collect())
    }

    /// The unified card level every battle from this object uses (the constructor's `level`).
    fn card_level(&self) -> i32 {
        self.level()
    }

    /// The crown towers' level every battle from this object uses (the constructor's `tower_level`, else `level`).
    fn tower_level(&self) -> i32 {
        self.tower_lvl()
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
    /// `forms`: [blue, red], each empty or parallel to its deck: 0 the base card, 1 its evolution, 2 its hero form
    /// (module doc, EVOLVED AND HERO FORMS; state.rs `BattleConfig::forms`, which checks them). None plays every card
    /// as itself.
    /// `levels`: [blue, red], each empty or parallel to its deck: the unified level of each deck card (a real deck's
    /// cards each have their own). `tower_levels`: [blue, red], each side's crown towers' level. None plays both sides
    /// at the object's `card_level()` and `tower_level()`, as before.
    #[pyo3(signature = (seed, decks, shuffle, start_tick, elixir_milli, tower_hp, spawns, forms = None, levels = None, tower_levels = None))]
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
        forms: Option<Vec<Vec<u8>>>,
        levels: Option<Vec<Vec<i32>>>,
        tower_levels: Option<Vec<i32>>,
    ) -> PyResult<()> {
        if decks.len() != 2 {
            return Err(PyValueError::new_err("decks must be [blue, red]"));
        }
        let mut forms: [Vec<u8>; 2] = match forms {
            None => [Vec::new(), Vec::new()],
            Some(f) if f.len() == 2 => [f[0].clone(), f[1].clone()],
            Some(_) => return Err(PyValueError::new_err("forms must be [blue, red]")),
        };
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
        cfg.card_level = [self.level(); 2];
        cfg.tower_level = [self.tower_lvl(); 2];
        match levels {
            None => {}
            Some(l) if l.len() == 2 => {
                for (t, side) in l.into_iter().enumerate() {
                    if !side.is_empty() && side.len() != decks[t].len() {
                        return Err(PyValueError::new_err(format!(
                            "levels[{t}] has {} entries for a deck of {}: one level per deck card, or none",
                            side.len(),
                            decks[t].len()
                        )));
                    }
                    cfg.deck_levels[t] = side;
                }
            }
            Some(_) => return Err(PyValueError::new_err("levels must be [blue, red]")),
        }
        match tower_levels {
            None => {}
            Some(l) if l.len() == 2 => cfg.tower_level = [l[0], l[1]],
            Some(_) => return Err(PyValueError::new_err("tower_levels must be [blue, red]")),
        }
        // THE EXPERIMENT'S CALIBRATION with the keywords' arms on top (set_calib carries the model fields with it).
        cfg.set_calib(self.battle_calib().map_err(PyValueError::new_err)?);
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
                // Each entry's form travels with its card.
                for f in forms.iter_mut().filter(|f| f.len() == n) {
                    *f = perm.iter().map(|k| f[*k]).collect();
                }
                cfg.shuffle_decks = false;
            }
            other => return Err(PyValueError::new_err(format!("unknown shuffle mode {other}"))),
        }
        cfg.decks = named;
        cfg.forms = forms;
        cfg.command_delay_ticks = self.command_delay_ticks;
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
    /// THE COMMAND DELAY, in ticks, for every battle this object starts from the next `reset` on (Blue, Red): a play or
    /// a button press accepted on tick T runs on T + k, checked again in full then; the hand and the elixir change
    /// only when it runs; a waiting card or button reads reason 18 (CardPending), and a waiting command's cost counts
    /// against the side's elixir. 0 runs every command at once (the default). Measured on the live client: 21-22.
    fn set_command_delay_ticks(&mut self, blue: u32, red: u32) {
        self.command_delay_ticks = [blue, red];
    }

    /// The command delay the next `reset` uses, (Blue, Red).
    fn command_delay_ticks(&self) -> (u32, u32) {
        (self.command_delay_ticks[0], self.command_delay_ticks[1])
    }

    /// `team`'s commands accepted and not run yet, in the order they run: (kind, what, x, y, ticks left, cost), kind
    /// "deploy" (what: the catalogue card id, x and y the tap) or "ability" (what: the button's action slot, x = y =
    /// 0). Empty with no delay.
    fn pending_commands(&self, team: i64) -> PyResult<Vec<PendingRow>> {
        let s = self.s()?;
        let t = team_of(team).ok_or_else(|| PyValueError::new_err(format!("team {team}")))?;
        let now = s.tick_count();
        Ok(s.pending_commands(t)
            .into_iter()
            .map(|c| {
                let left = c.due.saturating_sub(now);
                match c.kind {
                    crate::state::CommandKind::Deploy { card, pos } => ("deploy".to_string(), self.id_of_idx[card as usize] as i64, pos.x, pos.y, left, c.cost),
                    crate::state::CommandKind::Ability { button } => ("ability".to_string(), (HAND_SIZE + button) as i64, 0, 0, left, c.cost),
                }
            })
            .collect())
    }

    /// The delayed commands that ran at the top of the last tick: (team, kind, reason), reason 0 for a command that
    /// ran and a deploy reason code for one refused then.
    fn commands_run(&self) -> PyResult<Vec<(i64, String, u8)>> {
        let s = self.s()?;
        Ok(s.commands_run()
            .iter()
            .map(|r| {
                let kind = match r.command.kind {
                    crate::state::CommandKind::Deploy { .. } => "deploy",
                    crate::state::CommandKind::Ability { .. } => "ability",
                };
                let reason = reason_of(&r.result.clone().map(|_| ()));
                (r.command.team as i64, kind.to_string(), reason)
            })
            .collect())
    }

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

    /// THE MEMBERS A DEPLOY WOULD LAY: (unit name, x, y, deploy ms) for `card_name` played by
    /// `team` at (x, y) SUBTILES, in creation order, under this battle's calibration
    /// (formation.LAYOUT, DEPLOY_STAGGER, GROUND_Y_CLAMP, GROUND_DEPLOY_POINT). Pure: nothing is
    /// deployed. The point is taken as given (no snap, no relocation): the formation law alone.
    fn formation_preview(&self, team: i64, card_name: &str, x: i32, y: i32) -> PyResult<Vec<(String, i32, i32, i32)>> {
        let s = self.s()?;
        let t = match team {
            0 => Team::Blue,
            1 => Team::Red,
            _ => return Err(PyValueError::new_err("team must be 0 or 1")),
        };
        let members = s.formation_preview(t, card_name, Vec2::new(x, y)).map_err(|e| PyValueError::new_err(format!("{e:?}")))?;
        Ok(members.into_iter().map(|(name, p, ms)| (name, p.x, p.y, ms)).collect())
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

    /// What each of `team`'s four hand slots would debit if played now, whole elixir (state.rs `hand_costs`: the one
    /// resolution the play itself uses), -1 where no play resolves from the slot. A Mirror's slot reads the copy's
    /// cost plus its own; a variant card's the form its elixir chooses. The same list is each player's `hand_costs`
    /// in `state_json`.
    fn hand_costs(&self, team: i64) -> PyResult<Vec<i32>> {
        let t = team_of(team).ok_or_else(|| PyValueError::new_err(format!("team {team} is not 0 or 1")))?;
        Ok(self.s()?.hand_costs(t).to_vec())
    }

    /// The catalogue id of the card a Mirror played by `team` now would copy (state.rs `mirror_target`), -1 when
    /// there is none. The same id is each player's `mirror_target` in `state_json`.
    fn mirror_target(&self, team: i64) -> PyResult<i32> {
        let t = team_of(team).ok_or_else(|| PyValueError::new_err(format!("team {team} is not 0 or 1")))?;
        Ok(self.s()?.mirror_target(t).map_or(-1, |i| self.id_of_idx.get(i as usize).copied().unwrap_or(-1)))
    }

    /// HIDE (Tesla): the protocol uids of every live entity that is
    /// under ground right now (entity.rs `HideState::Hidden`: untargetable and, under
    /// calibration hide.HIDDEN_IMMUNE_TO_DAMAGE, immune). The same state is bit 2 of each
    /// entity row's `status_flags`; this accessor stays for the viewer and the tests.
    /// Rising buildings are not listed (they are targetable per
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

    /// ENCHANTS (the Rune Giant's): `(uid, source_uid, count, finish_ms)` for every live entity that carries an
    /// enchant -- the Rune Giant that gave it (-1 once he is gone), the attacks it has made since (its bonus lands on
    /// every AttackAmount-th, calibration enchant.BONUS_ATTACKS), and the ms it has left after his death (-1 while he
    /// lives). `state_json` is unchanged; a viewer reads this beside it.
    fn enchant_states(&self) -> PyResult<Vec<(i64, i64, u32, i32)>> {
        let s = self.s()?;
        Ok(s.entities()
            .filter_map(|e| {
                let sl = e.enchant?;
                let source = s.entity(sl.source).map_or(-1, |g| (g.team_seq as i64) * 2 + g.team as i64);
                Some(((e.team_seq as i64) * 2 + e.team as i64, source, sl.count, sl.finish_ms))
            })
            .collect())
    }

    /// RUNE GIANTS: `(uid, state, ms, picks)` for every live entity whose card carries the enchant -- state 0 not
    /// started, 1 waiting out ActionDelay or Cooldown, 2 looking, 3 launching; `ms` its clock; `picks` the friends its
    /// pending launch goes to. `state_json` is unchanged; a viewer reads this beside it.
    fn enchant_timers(&self) -> PyResult<Vec<(i64, u8, i32, u32)>> {
        let s = self.s()?;
        let cards = s.cards();
        Ok(s.entities()
            .filter(|e| cards.get(e.card_idx).enchant.is_some())
            .map(|e| ((e.team_seq as i64) * 2 + e.team as i64, e.enchant_state, e.enchant_ms, s.enchant_picks(e.id).len() as u32))
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

    /// RIDERS (the Ram Rider's rider; card.rs `AttachDef`): `(uid, mount uid)` for every live
    /// attached rider whose mount lives. A rider stands where its mount stood a tick before and,
    /// under calibration rider.TARGETABLE_WHILE_ATTACHED = untargetable_immune, nothing targets
    /// or touches it. `state_json` carries the same link as every entity row's last column, `mount_uid`; this
    /// list is the riders alone.
    fn rider_states(&self) -> PyResult<Vec<(i64, i64)>> {
        let s = self.s()?;
        Ok(s.entities()
            .filter_map(|e| {
                let m = e.attached_to?;
                let mount = s.entity(m)?;
                Some(((e.team_seq as i64) * 2 + e.team as i64, (mount.team_seq as i64) * 2 + mount.team as i64))
            })
            .collect())
    }

    /// COPIES AND GROUNDED FLIERS (the Clone, the Vines): `(uid, cloned, grounded_ms)` for every live entity that is a
    /// copy the Clone made (or a copy's death spawn), or that a Vines catch holds to the ground for `grounded_ms` more.
    /// A copy reports its original's card; the client's own records carry the Clone card's id for it. `state_json` is
    /// unchanged; a viewer reads this beside it.
    fn clone_states(&self) -> PyResult<Vec<(i64, bool, i32)>> {
        let s = self.s()?;
        Ok(s.entities()
            .filter(|e| e.cloned || e.grounded_ms > 0)
            .map(|e| ((e.team_seq as i64) * 2 + e.team as i64, e.cloned, e.grounded_ms))
            .collect())
    }

    /// TRACE-DIFF ENTRY POINT: every live troop as
    /// `(uid, card, x, y, deploy_ms, speed, [(col, row), ...])`, positions in
    /// SUBTILES and the route as half-tile CELLS in the order the engine stores it
    /// (GOAL-FIRST under PathModel::Oracle2026, so element 0 is the goal -- the
    /// same layout the live game publishes in `path_nodes`).
    ///
    /// WHY IT EXISTS: tools/oracle_diff.py steps this engine beside a recorded 15.535.29
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

    /// DEBUG ONLY: each live troop's contact state as the 16.402 move pass left it, one row per
    /// troop in slot order: (uid, avoidance offset, segment direction x, y, facing x, y). The uid is
    /// `debug_units`' element 0 and `state_json`'s; the offset is move16402.rs `Contact::offset`
    /// between ticks, the segment direction the frozen direction toward the route's last node
    /// ((0, 0) with none), the facing the unit's heading, both of length 256.
    ///
    /// WHY IT EXISTS: the contact law is exact on recorded client 15.535.29 crowds when it is fed
    /// the client's own state, so where the engine parts from a recording the first of these fields
    /// to part names the input that went wrong. `debug_units` keeps its seven fields, which callers
    /// unpack by position; this is a separate row.
    #[allow(clippy::type_complexity)] // a debug tuple row; the shape is documented above
    fn debug_contact(&self) -> PyResult<Vec<(i64, i32, i32, i32, i32, i32)>> {
        let s = self.s()?;
        Ok(s.entities()
            .filter(|e| e.kind == EntityKind::Troop)
            .map(|e| ((e.team_seq as i64) * 2 + e.team as i64, e.avoid_offset, e.seg_dir.x, e.seg_dir.y, e.facing.x, e.facing.y))
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
/// update order, native units, with an optional 16th, avoid_static (0 when absent).
/// Returns (x, y, dir_x, dir_y, offset, reached, popped_waypoint).
/// The body array of `contact_step16402` / `pushback_step16402`, decoded.
fn bodies16402(bodies: &[Vec<i64>], me: usize) -> PyResult<Vec<crate::move16402::Body>> {
    use crate::move16402 as ml;
    let mut out = Vec::with_capacity(bodies.len());
    for (k, b) in bodies.iter().enumerate() {
        if b.len() != 15 && b.len() != 16 {
            return Err(PyValueError::new_err("each body needs 15 fields, or 16 with avoid_static"));
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
            avoid_static: b.get(15).is_some_and(|v| *v != 0),
            // the array is given in update order, which is creation order: its position is the tie-break
            seq: k as u32,
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

/// The data/ folder of the checkout this extension was built in. Exported only by a `checkout-data` build (a build
/// from a checkout): `royalesim.data_dir()` uses it when it still holds derived/cards.json. A wheel does not export it,
/// so an installed wheel's `data_dir()` is always its own data/.
#[cfg_attr(not(feature = "checkout-data"), allow(dead_code))]
pub const BUILD_DATA_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data");

/// Which card table a Battle constructed now would load: `"file:<path>"` (the build checkout's
/// data/derived/cards.json) or `"embedded"` (the copy compiled into this extension, what an installed wheel runs).
#[pyfunction]
fn card_table_source() -> PyResult<String> {
    let db = CardDb::load_repo().map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
    Ok(match db.source {
        crate::card::CardSource::DerivedJson => format!("file:{}", CardDb::repo_file_path("cards.json")),
        crate::card::CardSource::Embedded => "embedded".to_string(),
        crate::card::CardSource::Fallback => "fallback".to_string(),
    })
}

/// Register the bindings on the `royalesim` module.
pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Battle>()?;
    m.add_function(wrap_pyfunction!(contact_step16402, m)?)?;
    m.add_function(wrap_pyfunction!(start_pushback16402, m)?)?;
    m.add_function(wrap_pyfunction!(pushback_step16402, m)?)?;
    m.add("DEPLOY_REASONS", DEPLOY_REASONS.to_vec())?;
    m.add("ENTITY_FIELDS", ENTITY_FIELDS.to_vec())?;
    m.add("UNIT_ROLES", UNIT_ROLES.to_vec())?;
    m.add("PROJECTILE_FIELDS", PROJECTILE_FIELDS.to_vec())?;
    m.add("SPELL_FIELDS", SPELL_FIELDS.to_vec())?;
    m.add("SPELL_MOTIONS", SPELL_MOTIONS.to_vec())?;
    m.add("CATALOGUE_FIELDS", CATALOGUE_FIELDS.to_vec())?;
    m.add("CARD_KINDS", CARD_KINDS.to_vec())?;
    m.add("EMBEDDED_CALIBRATION_JSON", EMBEDDED_CALIBRATION_JSON)?;
    m.add("EMBEDDED_ARENA_JSON", EMBEDDED_ARENA_JSON)?;
    m.add("EMBEDDED_RARITIES_CSV", EMBEDDED_RARITIES_CSV)?;
    m.add("EMBEDDED_GLOBALS_CSV", EMBEDDED_GLOBALS_CSV)?;
    m.add("EMBEDDED_CARDS_JSON", crate::card::EMBEDDED_CARDS_JSON)?;
    #[cfg(feature = "checkout-data")]
    m.add("BUILD_DATA_DIR", BUILD_DATA_DIR)?;
    m.add_function(wrap_pyfunction!(card_table_source, m)?)?;
    m.add("SNAPSHOT_FORMAT", crate::state::SNAPSHOT_FORMAT)?;
    m.add("HAND_SIZE", HAND_SIZE)?;
    m.add("ABILITY_BUTTONS", ABILITY_BUTTONS)?;
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

    /// THE DEFAULT CATALOGUE KEEPS ITS IDS: its first cards are CATALOGUE_ORDER, in that order, every other loadable card
    /// after them, so a card that begins to load takes the next id and moves no other.
    #[test]
    fn the_default_catalogue_lists_catalogue_order_first() {
        let Ok(b) = Battle::build(None, [[0, 1, 2], [0, 1, 2]], None, None, None, None, None, None, None, None) else { panic!("the default Battle was refused") };
        let names: Vec<&str> = b.catalogue.iter().map(|i| b.cards.get(*i).name.as_str()).collect();
        assert!(names.len() >= CATALOGUE_ORDER.len(), "the catalogue lost cards: {} of {}", names.len(), CATALOGUE_ORDER.len());
        assert_eq!(&names[..CATALOGUE_ORDER.len()], CATALOGUE_ORDER, "the first ids moved");
    }

    /// EVERY REASON CODE HAS A NAME: `reason_of` over every DeployError variant lands inside DEPLOY_REASONS. The
    /// array is length-annotated and grows by editing two places; TOO_EARLY and CARD_PENDING each once grew one half.
    #[test]
    fn every_reason_code_has_a_name() {
        use crate::state::DeployError as E;
        let all = [
            E::GameOver,
            E::TooEarly { tick: 0, until: 1 },
            E::UnknownCard(String::new()),
            E::UnsupportedCard(String::new(), String::new()),
            E::NotInHand,
            E::BadSlot,
            E::EmptySlot,
            E::NotEnoughElixir { have: 0, need: 1 },
            E::OutOfArena,
            E::Water,
            E::NoDeploy,
            E::OutOfTerritory,
            E::Occupied,
            E::InvalidLevel(String::new()),
            E::NothingToMirror,
            E::NoHero,
            E::AbilityNotReady,
            E::AbilitySpent,
            E::CardPending,
        ];
        for e in all {
            let code = super::reason_of(&Err(e.clone())) as usize;
            assert!(code < super::DEPLOY_REASONS.len(), "{e:?} returns {code}, past DEPLOY_REASONS");
        }
        assert_eq!(super::DEPLOY_REASONS[super::reason_of(&Err(E::CardPending)) as usize], "CARD_PENDING");
    }

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
        // barrel_anywhere_incl_water (kind codes follow `deploy_rule`), footprint_spell_kind_rolling
        // (Heal, a troop-territory spell with a troop's footprint rule, reports the Log's 3).
        let db = cards();
        let calib = crate::state::Calib::shipped();
        let catalogue: Vec<u16> = (0..db.cards.len() as u16).filter(|i| db.get(*i).name != KING_TOWER && db.get(*i).name != PRINCESS_TOWER && !db.get(*i).summon_only).collect();
        let rows: serde_json::Value = serde_json::from_str(&catalogue_rows(&db, &calib, &catalogue, db.lowest_level_valid_for_every_rarity()).unwrap()).unwrap();
        let row = |n: &str| rows.as_array().unwrap().iter().find(|r| r[0] == n).unwrap_or_else(|| panic!("{n} not in the catalogue")).clone();
        for (n, code) in [("Fireball", 2), ("Arrows", 2), ("Zap", 2), ("Log", 3), ("BarbLog", 3), ("Heal", 0), ("GoblinBarrel", 4), ("Knight", 0), ("Cannon", 1)] {
            assert_eq!(row(n)[1], code, "{n} kind code");
        }
        assert_eq!((row("Fireball")[3].as_i64(), row("Fireball")[6].as_i64()), (Some(0), Some(0)), "spell rows: count 0, hitpoints 0");
        assert!(rows.as_array().unwrap().iter().all(|r| r[0] != "Goblin"), "a released unit is not a card");
        let ids = ids_of_indices(&db, &catalogue);
        let goblin = db.cards.iter().position(|c| c.summon_only && c.name == "Goblin").unwrap();
        let barrel_id = catalogue.iter().position(|i| db.get(*i).name == "GoblinBarrel").unwrap() as i32;
        assert_eq!(ids[goblin], barrel_id);
        // A battle with a Goblin, a stunned unit and spells in flight. The Zap is aimed at the Knight's exact
        // start point: placement.TAP_SNAP's old arm, none (the shipped tile-centre snap moves the tap, and the
        // walking Knight is out of it).
        let mut cfg = battle_config(&db, &["Knight", "Archer", "Knight", "Archer", "Giant", "Knight", "Archer", "Knight"]);
        cfg.calib.placement_tap_snap = crate::state::TapSnap::None;
        let mut s = BattleState::new(7, cfg);
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

    #[test]
    fn every_row_but_a_crown_tower_has_a_card_id_in_the_default_catalogue() {
        // NO UNIT REPORTS -1: every row a battle can put on the board (an evolved form and the units it makes, a hero
        // form and its ability's units, every summon down a chain) has the id of a catalogue card that can put it down
        // (`ids_of_indices`), so a reader that refuses -1 (RoyaleGym's card_ids) reads every entity. Train lost half of
        // a conversion's matches to the evolutions' units reporting -1. The catalogue is the one `Battle::new` builds
        // by default.
        let db = cards();
        let loadable: Vec<u16> = (0..db.cards.len() as u16)
            .filter(|i| {
                let c = db.get(*i);
                c.name != KING_TOWER && c.name != PRINCESS_TOWER && !c.summon_only && c.evo.is_none() && db.index(&c.name) == Some(*i)
            })
            .collect();
        let pinned: Vec<u16> = CATALOGUE_ORDER.iter().filter_map(|n| loadable.iter().copied().find(|i| db.get(*i).name == *n)).collect();
        let catalogue: Vec<u16> = pinned.iter().copied().chain(loadable.iter().copied().filter(|i| !pinned.contains(i))).collect();
        let ids = ids_of_indices(&db, &catalogue);
        // A card the loader rejected after its push stays in `cards` but never in a catalogue, so nothing plays it.
        let rejected = |i: usize, c: &crate::card::CardDef| !c.summon_only && c.evo.is_none() && db.index(&c.name) != Some(i as u16);
        for (i, c) in db.cards.iter().enumerate().filter(|(i, c)| rejected(*i, c)) {
            assert!(db.rejected.iter().any(|(n, _)| *n == c.name), "{} is out of the catalogue and not rejected (row {i})", c.name);
        }
        let missing: Vec<&str> = db
            .cards
            .iter()
            .enumerate()
            .filter(|(i, c)| ids[*i] == -1 && c.name != KING_TOWER && c.name != PRINCESS_TOWER && !rejected(*i, c))
            .map(|(_, c)| c.name.as_str())
            .collect();
        assert_eq!(missing, Vec::<&str>::new(), "{} rows report -1", missing.len());
        // An evolved form and its units report the base card: the Evo Barbarians' Barbarian_EV1 the Barbarians.
        let barbarians = catalogue.iter().position(|i| db.get(*i).name == "Barbarians").unwrap() as i32;
        // (Its unit Barbarian_EV1 is the Evo Battle Ram's death spawn as well, so the table, first producer first, gives
        // it the Battle Ram's; a battle labels each by its producer, the form, whose id is the base card's.)
        let form = db.cards.iter().position(|c| c.name == "Barbarians_EV1").expect("no Barbarians_EV1 row");
        assert_eq!(ids[form], barbarians);
    }

    #[test]
    fn a_unit_reports_the_card_whose_play_put_it_down() {
        // THE PRODUCING CARD (entity.rs `source`): one unit type several cards make reports the card that made it, not
        // the first catalogue card that can (`ids_of_indices`, which gives every Skeleton the Witch's id and every
        // Barbarian the Battle Ram's). A Tombstone's Skeletons, through its waves and its death, against a red
        // Witch's;
        // a Barbarian Hut's Barbarians; the Tri-Wizards' Electro and Ice Wizards, which are cards of their own; a
        // Firespirit Hut's spirits; the Goblin Drill's Goblins (the Goblin Barrel's by the static table).
        let db = cards();
        let catalogue: Vec<u16> = (0..db.cards.len() as u16).filter(|i| db.get(*i).name != KING_TOWER && db.get(*i).name != PRINCESS_TOWER && !db.get(*i).summon_only).collect();
        let ids = ids_of_indices(&db, &catalogue);
        let id = |n: &str| catalogue.iter().position(|i| db.get(*i).name == n).unwrap_or_else(|| panic!("{n} not in the catalogue")) as i64;
        let unit = |n: &str| db.cards.iter().position(|c| c.name == n).unwrap_or_else(|| panic!("no {n} row"));
        assert_eq!(ids[unit("Skeleton")], id("Witch") as i32, "the static table this test is about: a Skeleton is the Witch's");
        let mut s = battle(&db, &["Knight", "Archer", "Knight", "Archer", "Giant", "Knight", "Archer", "Knight"]);
        let at = |x: i32, y: i32| Vec2::new(crate::fixed::tiles(x), crate::fixed::tiles(y));
        // Towers nothing here can take down: the run must last the Tombstone's whole life (a win at tick 575 ended
        // the battle with it still standing).
        for team in [Team::Blue, Team::Red] {
            for k in 0..3 {
                s.scenario_set_tower_hp(team, k, 1_000_000).unwrap();
            }
        }
        for (card, x, y) in [("Tombstone", 3, 5), ("BarbarianHut", 14, 7), ("TriWizards", 9, 3), ("FirespiritHut", 3, 9), ("GoblinDrill", 9, 22)] {
            s.spawn_unit(Team::Blue, card, at(x, y), None).unwrap_or_else(|e| panic!("{card}: {e:?}"));
        }
        s.spawn_unit(Team::Red, "Witch", at(14, 28), None).unwrap();
        // A Skeleton Balloon on one hitpoint over a red princess tower: its death's container (a spell, not a unit)
        // lets out Skeletons that report the balloon.
        s.scenario_spawn_now(Team::Blue, "SkeletonBalloon", at(4, 24), Some(1)).unwrap();
        let mut seen: BTreeMap<(u8, String), std::collections::BTreeSet<i64>> = BTreeMap::new();
        let tombstone_dead = |s: &BattleState| !s.entities().any(|e| e.card == "Tombstone");
        let mut after_tomb = 0;
        for t in 0..2000 {
            s.tick();
            assert_eq!(s.outcome(), None, "the battle ended at tick {t}");
            if t % 5 == 0 || tombstone_dead(&s) {
                let v: serde_json::Value = serde_json::from_str(&state_json_text(&s, &db, &ids, &[[0, 1, 2], [0, 2, 1]], &BTreeMap::new()).unwrap()).unwrap();
                let rows = v["entities"].as_array().unwrap();
                let views: Vec<_> = s.entities().collect();
                assert_eq!(rows.len(), views.len());
                for (r, e) in rows.iter().zip(&views) {
                    seen.entry((e.team as u8, e.card.to_string())).or_default().insert(r[3].as_i64().unwrap());
                }
            }
            if tombstone_dead(&s) {
                after_tomb += 1;
                if after_tomb > 20 {
                    break;
                }
            }
        }
        assert!(
            tombstone_dead(&s),
            "the Tombstone outlived the run: tick {}, outcome {:?}, its hp {:?}, blue entities {:?}",
            s.tick_count(),
            s.outcome(),
            s.entities().filter(|e| e.card == "Tombstone").map(|e| e.hp).collect::<Vec<_>>(),
            s.entities().filter(|e| e.team == Team::Blue).map(|e| e.card.to_string()).collect::<Vec<_>>()
        );
        let got = |n: &str| seen.get(&(Team::Blue as u8, n.to_string())).cloned().unwrap_or_default();
        let want = |ns: &[&str]| ns.iter().map(|n| id(n)).collect::<std::collections::BTreeSet<i64>>();
        assert_eq!(got("Skeleton"), want(&["Tombstone", "SkeletonBalloon"]), "the Tombstone's Skeletons, its death's among them, and the balloon's: {seen:?}");
        assert_eq!(seen.get(&(Team::Red as u8, "Skeleton".to_string())), Some(&want(&["Witch"])), "the red Witch's: {seen:?}");
        assert_eq!(got("Barbarian"), want(&["BarbarianHut"]), "{seen:?}");
        assert_eq!(got("ElectroWizard"), want(&["TriWizards"]), "{seen:?}");
        assert_eq!(got("IceWizard"), want(&["TriWizards"]), "{seen:?}");
        assert_eq!(got("FireSpirits"), want(&["FirespiritHut"]), "{seen:?}");
        assert_eq!(got("Goblin"), want(&["GoblinDrill"]), "{seen:?}");
        // A snapshot keeps the producer.
        let v: serde_json::Value = serde_json::from_str(&state_json_text(&s, &db, &ids, &[[0, 1, 2], [0, 2, 1]], &BTreeMap::new()).unwrap()).unwrap();
        let back = reload(&db, &s);
        let again: serde_json::Value = serde_json::from_str(&state_json_text(&back, &db, &ids, &[[0, 1, 2], [0, 2, 1]], &BTreeMap::new()).unwrap()).unwrap();
        assert_eq!(again["entities"], v["entities"], "a reloaded battle reports the same cards");
    }

    fn battle(db: &Arc<CardDb>, deck: &[&str]) -> BattleState {
        BattleState::new(7, battle_config(db, deck))
    }

    fn battle_config(db: &Arc<CardDb>, deck: &[&str]) -> BattleConfig {
        let mut cfg = BattleConfig::with_cards(CardDb::clone(db));
        cfg.cards = db.clone();
        let d: Vec<String> = deck.iter().map(|s| s.to_string()).collect();
        cfg.decks = [d.clone(), d];
        cfg
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
            // 21 ticks a step: a refill period (1000 ms) and one, so the hand refill timer has a card in every slot
            // again by the next step's commands.
            for _ in 0..21 {
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
        // on the board, a named buff with time left, a spell in play, and a Tesla hidden by
        // its own hide in the status bits.
        let db = cards();
        let ids = catalogue_without(&db, &[]);
        let musketeer = ids[db.index("Musketeer").unwrap() as usize];
        assert!(musketeer >= 0);
        let tesla = ids[db.index("Tesla").unwrap() as usize];
        assert!(tesla >= 0);
        let mut s = battle(&db, &["Musketeer", "Knight", "Archer", "Giant", "Minions", "Cannon", "Tesla", "Zap"]);
        let at = |x: i32, y: i32| Vec2::new(crate::fixed::tiles(x), crate::fixed::tiles(y));
        // a red Knight inside the blue left princess tower's range: the TOWER's shots
        s.scenario_spawn_now(Team::Red, "Knight", at(3, 9), None).unwrap();
        // mid-field, out of every tower's range: the MUSKETEER's shots
        s.scenario_spawn_now(Team::Blue, "Musketeer", at(9, 14), None).unwrap();
        s.scenario_spawn_now(Team::Red, "Giant", at(9, 18), None).unwrap();
        // a blue Tesla far from every enemy: it goes under ground by its own hide
        s.scenario_spawn_now(Team::Blue, "Tesla", at(16, 2), None).unwrap();
        // a Poison on the red Knight: a BUFF with a name and time left
        s.spawn_unit(Team::Blue, "Poison", at(3, 9), None).unwrap();
        let (mut tower_shot, mut troop_shot, mut resolved_target, mut named_buff) = (false, false, false, false);
        let (mut spell_row, mut hidden_tesla) = (false, false);
        let status = ENTITY_FIELDS.iter().position(|f| *f == "status_flags").unwrap();
        let level = ENTITY_FIELDS.iter().position(|f| *f == "level").unwrap();
        let mut levels_read = 0usize;
        for _ in 0..200 {
            s.tick();
            let v: serde_json::Value =
                serde_json::from_str(&state_json_text(&s, &db, &ids, &[[0, 1, 2], [0, 2, 1]], &BTreeMap::new()).unwrap()).unwrap();
            assert!(v.get("calibration_overrides").is_none(), "an unmodified engine's frame claims overrides");
            let ents = v["entities"].as_array().unwrap();
            let uids: Vec<i64> = ents.iter().map(|r| r[0].as_i64().unwrap()).collect();
            // The engine's own hide state, per uid, for the status bits to be held against.
            let hidden: Vec<i64> = s.entities().filter(|e| e.hidden).map(|e| (e.team_seq as i64) * 2 + e.team as i64).collect();
            // The engine's own level per uid, for the level column to be held against.
            let engine_level: BTreeMap<i64, i32> = s.entities().map(|e| ((e.team_seq as i64) * 2 + e.team as i64, e.level)).collect();
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
                // status_flags: reported (never -1), bits 0 and 1 unset while no card goes
                // underground or invisible, bit 2 exactly the engine's hidden set.
                let bits = r[status].as_i64().unwrap();
                assert!((0..8).contains(&bits), "status_flags {bits} is not three bits: {r:?}");
                assert_eq!(bits & 3, 0, "a unit reports underground or invisible, which no loaded card is: {r:?}");
                assert_eq!(bits & 4 != 0, hidden.contains(&r[0].as_i64().unwrap()), "status bit 2 disagrees with the engine's hide: {r:?}");
                if bits & 4 != 0 && r[3].as_i64() == Some(tesla as i64) {
                    hidden_tesla = true;
                }
                // level: reported (never -1) and the engine's own level of that entity.
                assert_eq!(r[level].as_i64(), engine_level.get(&r[0].as_i64().unwrap()).map(|l| *l as i64), "the level column is not the entity's level: {r:?}");
                assert!(r[level].as_i64().unwrap() >= 1, "a level below 1: {r:?}");
                levels_read += 1;
                for b in r[19].as_array().unwrap() {
                    let (name, ms) = (b[0].as_str().unwrap(), b[1].as_i64().unwrap());
                    if name.split('|').any(|n| n == "Poison") && ms > 0 {
                        named_buff = true;
                    }
                }
            }
            for sp in v["spells"].as_array().unwrap() {
                let sp = sp.as_array().unwrap();
                assert_eq!(sp.len(), SPELL_FIELDS.len(), "a spell row is not SPELL_FIELDS long: {sp:?}");
                let motion = sp[2].as_u64().unwrap() as usize;
                assert!(motion < SPELL_MOTIONS.len(), "motion {motion} has no name in SPELL_MOTIONS: {sp:?}");
                spell_row = true;
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
        assert!(spell_row, "no spell row in 200 ticks, so SPELL_FIELDS was held against nothing");
        assert!(hidden_tesla, "the Tesla never reported status bit 2 in 200 ticks");
        assert!(levels_read > 0, "no entity row was read, so the level column was held against nothing");
    }

    #[test]
    fn the_protocol_name_lists_match_what_the_rows_carry() {
        // SPELL_MOTIONS by code: each code the serializer writes indexes its own name.
        for (code, name) in [(MOTION_FLIGHT, "FLIGHT"), (MOTION_AIRBORNE, "AIRBORNE"), (MOTION_ROLLING, "ROLLING"), (MOTION_AREA, "AREA"), (MOTION_PULSING, "PULSING"), (MOTION_FUSE, "FUSE"), (MOTION_STRIKES, "STRIKES"), (MOTION_SCHEDULED, "SCHEDULED")] {
            assert_eq!(SPELL_MOTIONS[code as usize], name);
        }
        // Every catalogue row is CATALOGUE_FIELDS long, and its card_kind is the card's own
        // kind, named from CARD_KINDS -- a variant card's is its first form's, whose row it
        // shows; today the kind code bands agree with it.
        let db = cards();
        let calib = crate::state::Calib::shipped();
        let catalogue: Vec<u16> = (0..db.cards.len() as u16).filter(|i| db.get(*i).name != KING_TOWER && db.get(*i).name != PRINCESS_TOWER && !db.get(*i).summon_only).collect();
        let rows: serde_json::Value = serde_json::from_str(&catalogue_rows(&db, &calib, &catalogue, db.lowest_level_valid_for_every_rarity()).unwrap()).unwrap();
        let kind_col = CATALOGUE_FIELDS.iter().position(|f| *f == "card_kind").unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for (r, idx) in rows.as_array().unwrap().iter().zip(&catalogue) {
            let r = r.as_array().unwrap();
            assert_eq!(r.len(), CATALOGUE_FIELDS.len(), "a catalogue row is not CATALOGUE_FIELDS long: {r:?}");
            let kind = r[kind_col].as_str().unwrap();
            let shown = db.get(*idx).variant().map_or(*idx, |opts| opts[0].card);
            assert_eq!(kind, card_kind_name(db.get(shown).kind), "{r:?}");
            let code = r[1].as_u64().unwrap();
            let band = match code {
                0 => "TROOP",
                1 => "BUILDING",
                _ => "SPELL",
            };
            // A spell placed as a troop is (troop territory with a troop's footprint rule: Heal) reports a troop's 0.
            let troop_placed_spell = kind == "SPELL" && code == 0 && matches!(deploy_rule(&calib, db.get(shown)), (Territory::EnemyTowerRects, true));
            // A tunneller (5) keeps its kind's footprint rule, a troop's or a building's, which card_kind names.
            let tunneller = code == u64::from(KIND_TUNNEL) && (kind == "TROOP" || kind == "BUILDING");
            assert!(kind == band || troop_placed_spell || tunneller, "{r:?}: card_kind {kind}, kind code band {band}");
            seen.insert(kind);
        }
        assert_eq!(seen.into_iter().collect::<Vec<_>>(), vec!["BUILDING", "SPELL", "TROOP"], "the catalogue should carry every kind");
    }

    #[test]
    fn the_mirror_and_the_variant_card_carry_their_rules_in_the_catalogue_and_their_prices_in_state_json() {
        // The Mirror's kind code is its own (6). A variant card shows its first form's row with its own elixir, and its
        // forms in the 10th element. Each player in state_json carries what each hand slot costs now and the card a
        // Mirror would copy, both read from the one resolution the play uses (state.rs `resolve_play`).
        let db = cards();
        let calib = crate::state::Calib::shipped();
        let catalogue: Vec<u16> = (0..db.cards.len() as u16)
            .filter(|i| {
                let c = db.get(*i);
                c.name != KING_TOWER && c.name != PRINCESS_TOWER && !c.summon_only && db.index(&c.name) == Some(*i)
            })
            .collect();
        let rows: serde_json::Value = serde_json::from_str(&catalogue_rows(&db, &calib, &catalogue, db.lowest_level_valid_for_every_rarity()).unwrap()).unwrap();
        let row = |n: &str| rows.as_array().unwrap().iter().find(|r| r[0] == n).unwrap_or_else(|| panic!("{n} not in the catalogue")).clone();
        let id = |n: &str| catalogue.iter().position(|i| db.get(*i).name == n).unwrap_or_else(|| panic!("{n} not in the catalogue")) as i64;
        let variants = CATALOGUE_FIELDS.iter().position(|f| *f == "variants").unwrap();
        assert_eq!(row("Mirror")[1], KIND_MIRROR);
        assert!(row("Mirror")[variants].is_null() && row("Knight")[variants].is_null(), "only a variant card carries forms");
        let (mm, mounted) = (row("MergeMaiden"), row("MergeMaiden_Mounted"));
        assert_eq!(mm[variants], serde_json::json!([[6000, id("MergeMaiden_Mounted"), 6], [3000, id("MergeMaiden_Normal"), 3]]));
        assert_eq!(mm[2], 6, "the variant card's own elixir");
        for col in [1, 3, 4, 5, 6, 7, 8] {
            assert_eq!(mm[col], mounted[col], "the variant card shows its first form's {}", CATALOGUE_FIELDS[col]);
        }
        // state_json: the prices move with the elixir and with the plays.
        let ids = ids_of_indices(&db, &catalogue);
        let slots = [[0, 1, 2], [0, 2, 1]];
        let player = |s: &BattleState| -> serde_json::Value {
            let v: serde_json::Value = serde_json::from_str(&state_json_text(s, &db, &ids, &slots, &BTreeMap::new()).unwrap()).unwrap();
            v["players"][0].clone()
        };
        let mut s = battle(&db, &["Knight", "MergeMaiden", "Archer", "Giant", "Mirror", "Fireball", "Valkyrie", "HogRider"]);
        while s.tick_count() < calib.deploy_lockout_ticks.max(0) as u32 {
            s.tick();
        }
        s.scenario_set_elixir_milli(Team::Blue, 5000);
        let p = player(&s);
        assert_eq!(p["hand_costs"], serde_json::json!([3, 3, 3, 5]), "the Knight, the Empress at 5 elixir (her Normal form), the Archer, the Giant: {p}");
        assert_eq!(p["mirror_target"], -1, "nothing played yet");
        s.scenario_set_elixir_milli(Team::Blue, 10000);
        assert_eq!(player(&s)["hand_costs"][1], 6, "at 10 elixir the Empress plays her Mounted form");
        s.deploy_slot(Team::Blue, 0, Vec2::new(crate::fixed::tiles(9), crate::fixed::tiles(8))).unwrap();
        let p = player(&s);
        assert_eq!(p["hand"][0], id("Mirror"), "the Mirror came up from the queue: {p}");
        assert_eq!(p["mirror_target"], id("Knight"));
        assert_eq!(p["hand_costs"][0], 4, "a Mirror of the Knight costs 3 + 1");
    }

    #[test]
    fn a_named_goblin_drill_reports_its_buildings_footprint() {
        // The Goblin Drill's card row is its 0-radius dig, and the engine places the 2x2 building the dig morphs into
        // (placement.SPAWN_PATHFIND_DESTINATION = client_tile_centre_morph_footprint, state.rs `building_placement`).
        // A catalogue that names the card gives that building's footprint, not the dig's.
        let db = cards();
        let calib = crate::state::Calib::shipped();
        let drill = db.index("GoblinDrill").unwrap_or_else(|| panic!("the Goblin Drill is refused: {:?}", db.rejected.iter().find(|(n, _)| n == "GoblinDrill")));
        let morph = db.get(drill).spawn_pathfind.and_then(|p| p.morph).expect("the dig morphs into its building");
        let dig = crate::arena::placement_tiles(db.get(drill).collision_radius);
        let building = crate::arena::placement_tiles(db.get(morph).collision_radius);
        assert_ne!(dig, building, "vacuous: the dig and its building give one footprint");
        assert_eq!(building, 2, "the building's 2x2 box");
        let fp = CATALOGUE_FIELDS.iter().position(|f| *f == "footprint_tiles").unwrap();
        let rows: serde_json::Value = serde_json::from_str(&catalogue_rows(&db, &calib, &[drill], db.lowest_level_valid_for_every_rarity()).unwrap()).unwrap();
        assert_eq!(rows[0][fp], building, "the Goblin Drill's footprint_tiles");
    }

    /// A `Battle` as Python builds one, with `overrides` as `calibration_overrides`. Its error is dropped unread:
    /// reading a Python error needs the interpreter, which these tests do not start.
    fn battle_with(names: &[&str], overrides: &[(&str, &str)]) -> Battle {
        let m: BTreeMap<String, String> = overrides.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let names = Some(names.iter().map(|n| n.to_string()).collect());
        let Ok(b) = Battle::build(names, [[0, 1, 2], [0, 1, 2]], None, None, None, (!m.is_empty()).then_some(m), None, None, None, None) else { panic!("the Battle was refused") };
        b
    }

    /// THE CATALOGUE IS THE BATTLE'S. `catalogue_json` reports the card data and the placement a battle from the
    /// same object runs, overrides included. It used to read the shipped ledger for the rows, so with
    /// placement.SPAWN_PATHFIND_DESTINATION overridden to ordinary_ground_deploy_point a named Goblin Drill reported
    /// its building's 2-tile footprint while the engine placed it on its dig's 1 tile. The engine's answer is read
    /// from the engine (`building_placement`), under the calibration `Calib::shipped_with_overrides` builds, and the
    /// two arms must give different footprints or the check could not see the defect.
    /// Plant: catalogue_reads_shipped_calib.
    #[test]
    fn a_named_goblin_drill_reports_the_footprint_the_battle_places() {
        let key = "placement.SPAWN_PATHFIND_DESTINATION";
        let fp = CATALOGUE_FIELDS.iter().position(|f| *f == "footprint_tiles").unwrap();
        let mut placed = Vec::new();
        for arm in ["client_tile_centre_morph_footprint", "ordinary_ground_deploy_point"] {
            let value = format!("\"{arm}\"");
            let b = battle_with(&["GoblinDrill", "Knight"], &[(key, value.as_str())]);
            let Ok(text) = b.catalogue_text() else { panic!("{arm}: catalogue_json failed") };
            let rows: serde_json::Value = serde_json::from_str(&text).unwrap();
            let reported = rows[0][fp].as_i64().expect("the Drill's row has a footprint");
            let m: BTreeMap<String, String> = [(key.to_string(), value.clone())].into_iter().collect();
            let (calib, _) = Calib::shipped_with_overrides(&m).unwrap();
            let mut cfg = BattleConfig::with_cards(CardDb::clone(&b.cards));
            cfg.set_calib(calib);
            let s = BattleState::new(0, cfg);
            let drill = s.cards().index("GoblinDrill").unwrap();
            let (_, rect) = s.building_placement(Team::Blue, drill, Vec2::new(crate::fixed::tiles(9), crate::fixed::tiles(8))).expect("the tap is legal");
            let tiles = i64::from((rect.max.x - rect.min.x) / crate::fixed::tiles(1));
            assert_eq!(reported, tiles, "{arm}: the catalogue reports {reported} tiles, the battle places the Drill on {tiles}");
            placed.push(tiles);
        }
        assert_ne!(placed[0], placed[1], "vacuous: both arms place the Drill on one footprint");
    }

    /// THE SEAT-SYMMETRIC ARMS ARE A CALIBRATION THE LOADER ACCEPTS. `path_search` = trace_fitted_astar puts the
    /// knockback on fixed_distance, and knockback.ATTACK_PUSHBACK = ladder_away_from_target (shipped) has no code
    /// there: the loader refuses that pairing. The selection used to be written after the ledger was read, past the
    /// loader's check, so under it a Sparky ran the 16.402 recoil ladder. It now selects ATTACK_PUSHBACK's old arm,
    /// none, with the rest, and the result is checked as the loader checks a ledger (`Calib::validate`), for the
    /// shipped ledger and for an experiment's, with the arms RoyaleGym's symmetric engine selects.
    /// Plant: symmetric_selection_keeps_attack_recoil.
    #[test]
    fn the_symmetric_selection_is_a_calibration_the_loader_accepts() {
        use crate::state::{AttackPushback, DeathSpawnPushback, GroundDeployPoint, GroundYClamp, KnockLaw, PathSearch, TapSnap};
        let m: BTreeMap<String, String> = [("targeting.FIRST_TOWER_PICK".to_string(), "\"client_spawn_lane_own_frame\"".to_string())].into_iter().collect();
        let (experiment, _) = Calib::shipped_with_overrides(&m).unwrap();
        for base in [None, Some(&experiment)] {
            let c = selected_calib(base, Some(PathSearch::TraceFittedAstar), Some(GroundYClamp::DeployColumnRangeOwnFrame), Some(GroundDeployPoint::None), Some(DeathSpawnPushback::NotRead), Some(TapSnap::None))
                .unwrap_or_else(|e| panic!("the symmetric selection was refused: {e}"));
            assert_eq!((c.knock_law, c.attack_pushback), (KnockLaw::FixedDistance, AttackPushback::None));
            c.validate().unwrap_or_else(|e| panic!("the symmetric selection is a calibration the loader refuses: {e}"));
        }
        // The shipped ledger itself pairs the ladder with the recoil, so the selection above changed something.
        assert_eq!(Calib::shipped().attack_pushback, AttackPushback::LadderAwayFromTarget, "vacuous: the recoil is not shipped");
    }

    /// A REFUSED PAIRING IS REFUSED ON EVERY PATH: in a ledger (`Calib::from_json`, through an override), in the
    /// calibration a `Battle` selects (`selected_calib`), and in a battle built from a hand-edited `BattleConfig`
    /// (`BattleState::try_new`). The pairing: the recoil ladder under the fixed-distance knockback.
    /// Plants: symmetric_selection_unchecked (the selection), battle_calib_unchecked (the battle).
    #[test]
    fn a_refused_pairing_is_refused_on_every_path() {
        use crate::state::{AttackCycle, KnockLaw, KnockStacking, PathSearch};
        let m: BTreeMap<String, String> =
            [("knockback.DISPLACEMENT_LAW", "\"fixed_distance\""), ("knockback.STACKING", "\"vector_sum\"")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let Err(e) = Calib::shipped_with_overrides(&m) else { panic!("the ledger path accepted the recoil ladder under fixed_distance") };
        assert!(e.contains("ATTACK_PUSHBACK"), "the ledger path refused it for another reason: {e}");
        // The selection: a base whose own pairing is refused and which the keyword does not touch.
        let mut hand = Calib::shipped();
        hand.attack_cycle = AttackCycle::WindupLoadTime;
        let Err(e) = selected_calib(Some(&hand), Some(PathSearch::TraceFittedAstar), None, None, None, None) else { panic!("the selection accepted a refused pairing") };
        assert!(e.contains("ATTACK_CYCLE"), "the selection refused it for another reason: {e}");
        // A hand-edited battle config.
        let mut cfg = BattleConfig::with_cards(CardDb::clone(&cards()));
        cfg.calib.knock_law = KnockLaw::FixedDistance;
        cfg.calib.knock_stacking = KnockStacking::VectorSum;
        let Err(e) = BattleState::try_new(0, cfg) else { panic!("a battle ran the recoil ladder under fixed_distance") };
        assert!(e.contains("ATTACK_PUSHBACK"), "the battle refused it for another reason: {e}");
    }

    /// AN EVOLVED CARD IN THE BINDING: the default catalogue lists base cards only and a named form is refused; the
    /// counter reaches `state_json` ("evo": [card id, plays, next play evolved, cycles]); the form's unit and its bombs report
    /// under the base card's id, the unit with status bit 3.
    #[test]
    fn an_evolved_cannon_reports_under_its_base_card() {
        let Ok(b) = Battle::build(None, [[0, 1, 2], [0, 1, 2]], None, None, None, None, None, None, None, None) else { panic!("the default Battle was refused") };
        assert!(b.catalogue.iter().all(|i| b.cards.get(*i).evo.is_none()), "a form in the default catalogue");
        assert!(Battle::build(Some(vec!["Cannon_EV1".into()]), [[0, 1, 2], [0, 1, 2]], None, None, None, None, None, None, None, None).is_err(), "a named form was taken");
        let db = b.cards.clone();
        let ids = ids_of_indices(&db, &b.catalogue);
        let (cannon, form) = (db.index("Cannon").unwrap(), db.index("Cannon_EV1").unwrap());
        let id = ids[cannon as usize];
        assert!(id >= 0);
        assert_eq!(ids[form as usize], id, "the form reports under its base card");
        let mut cfg = BattleConfig::with_cards(CardDb::clone(&db));
        cfg.cards = db.clone();
        cfg.decks = [vec!["Cannon".into()], vec!["Knight".into()]];
        cfg.forms = [vec![1], Vec::new()];
        let mut s = BattleState::new(7, cfg);
        while s.tick_count() < s.config().calib.deploy_lockout_ticks.max(0) as u32 {
            s.tick();
        }
        let json = |s: &BattleState| -> serde_json::Value { serde_json::from_str(&state_json_text(s, &db, &ids, &[[0, 1, 2], [0, 2, 1]], &BTreeMap::new()).unwrap()).unwrap() };
        for (k, x) in [6, 9, 12].into_iter().enumerate() {
            assert_eq!(json(&s)["players"][0]["evo"], serde_json::json!([[id, k, u8::from(k == 2), 2]]), "before play {}", k + 1);
            s.scenario_set_elixir_milli(Team::Blue, 10000);
            s.deploy_slot(Team::Blue, 0, Vec2::new(crate::fixed::tiles(x), crate::fixed::tiles(10))).unwrap();
            s.tick();
            // The hand refill timer (`refill_hands`): slot 0 holds a card again before the next play.
            while s.hand_card(Team::Blue, 0).is_err() {
                s.tick();
            }
        }
        let v = json(&s);
        assert_eq!(v["players"][0]["evo"], serde_json::json!([[id, 0, 0, 2]]));
        assert_eq!(v["players"][1]["evo"], serde_json::json!([]));
        let flags = ENTITY_FIELDS.iter().position(|f| *f == "status_flags").unwrap();
        let evolved: Vec<_> = v["entities"].as_array().unwrap().iter().filter(|r| r[flags].as_i64().unwrap() & 8 != 0).collect();
        assert_eq!(evolved.len(), 1, "one evolved unit");
        assert_eq!(evolved[0][3], id);
        assert_eq!(v["spells"].as_array().unwrap().iter().filter(|r| r[1] == id).count(), 9, "the nine bombs under the Cannon's id");
        assert_eq!(catalogue_violation(&reload(&db, &s), &ids), Ok(()));
    }
}
