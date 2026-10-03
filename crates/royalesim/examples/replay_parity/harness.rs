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
//! WHERE THE POINT IS RESOLVED. The client resolves a tap on the board of the tick it was
//! TAPPED, some 25 ticks before the units appear. A corpus troop row whose placements-log tap
//! is older than its issue tick has its point resolved on the tap tick's board and is played
//! from there (`resolve_tick`); every other row is resolved when it is issued. A scenario
//! BUILDING row is laid from its tap as a play would lay it (`scenario_building_tap`,
//! `BattleState::spawn_unit_tapped`), since `spawn_unit` keeps a building where it was put.
//!
//! MATCHING. A sim entity is reduced to its ROOT card: the card that was deployed to
//! put it there -- itself for a deployed card, its spawner's card for a spawner
//! emission (`spawned_by`), the card the harness deployed on that tick for a SECOND
//! SUMMON (the Goblin Gang's Spear Goblins; card.rs `FormationDef`), the card the
//! harness deployed within `DEPLOY_AREA_LOOKBACK` ticks for a unit its deploy spawn
//! area puts down (the Tri Wizards' three; card.rs `CardDef::deploy_spawn_area`), the
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

use royalesim::card::{CardDb, CardKind, UnitRef, FORM_EVOLUTION, FORM_HERO};
use royalesim::entity::{AttackPhase, EntityKind};
use royalesim::fixed::{isqrt, Vec2, SUBTILE, SUBTILE_PER_MILLITILE};
use royalesim::state::{BattleConfig, BattleState, DeployError};
use royalesim::{EntityId, Team};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

// ---------------------------------------------------------------------------
// the fixture

pub const FORMAT: &str = "replay-fixture-1";

/// THE ONE DEPLOY-TICK CONVENTION THIS HARNESS PLAYS: a deploy's `tick` is the first frame its
/// effect exists in the game (a troop's units on the board, a spell's first frame), and the
/// harness issues it at `tick - 1` (module doc). On 2026-09-24 the 15.535.29 scenario fixtures were briefly
/// relabelled to the ISSUE tick; the harness read those labels under this convention and issued
/// every scenario deploy one tick early, and a scenario scored 180 of 180 ticks within 250 with
/// 0 exact, which read as a fix. Nothing in a fixture said which convention it used, so nothing
/// could refuse it. Now a fixture must say, and `Fixture::from_str` refuses one that does not.
pub const DEPLOY_TICK_CONVENTION: &str = "first_effect_frame";

/// The trace's truth target when the frame recorded none (`Row::target` is `None`).
pub const TRUTH_TARGET_UNRECORDED: i64 = -3;

/// Position tolerances, native units. Named so the report's columns are one list.
pub const TOLERANCES_NATIVE: [i32; 3] = [250, 500, 1000];
/// A pair whose position error passes this is the battle's first divergence.
pub const DIVERGENCE_NATIVE: i32 = 1000;
/// The tolerance at which a divergence is said to have STARTED (the cause is read there).
pub const ONSET_NATIVE: i32 = 250;
/// How many ticks back a death is looked for when a death spawn is rooted.
pub const DEATH_SPAWN_LOOKBACK: u32 = 3;
/// How many ticks back a death is looked for when a SCHEDULED DEATH AREA's unit is rooted (card.rs `scheduled_area`:
/// the Suspicious Bush's SuspiciousBush_DummyAEO, LifeDuration 1000, its two BushGoblins at 625 and 675 ms): the area's
/// longest life in the tables, 1,000 ms. At DEATH_SPAWN_LOOKBACK the goblins, 13 and 14 ticks after the bush's death,
/// were rooted to no card and paired with nothing (sweep-SuspiciousBush: 210 unit-ticks).
pub const SCHEDULED_DEATH_LOOKBACK: u32 = 20;
/// THE ROOT OF A BUFF'S DEATH SPAWN (the Mother Witch's hog, the Goblin Curse's goblin; card.rs `UnitRef::BuffDeathSpawn`):
/// the recording reports its card as -1, and the fixture maker names that "cardless_unit" (sweep-WitchMother,
/// sweep-GoblinCurse). Rooted by the dying unit's card, it was paired with nothing (234 and 20 unit-ticks).
pub const CARDLESS_ROOT: &str = "cardless_unit";
/// How many ticks back a spell cast is looked for when a released unit is rooted.
pub const SPELL_RELEASE_LOOKBACK: u32 = 200;
/// How many ticks back a deploy is looked for when a unit its deploy spawn area puts down is rooted: the Tri Wizards'
/// TriWizardSpawn lives 500 ms (10 ticks), and its units come 5 and 7 ticks after the play.
pub const DEPLOY_AREA_LOOKBACK: u32 = 20;
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
    /// The tick the deploy was ISSUED on, where the generator knows it (the 15.535.29 scenario
    /// emitter does; a corpus capture does not). When present it must be `tick - 1`, the
    /// tick this harness issues on; `Fixture::from_str` refuses a disagreement rather than
    /// letting one of the two labels shift the play.
    #[serde(default)]
    pub issued_tick: Option<u32>,
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
    /// The tap, native, as the 15.535.29 scenario emitter writes it: an [x, y] pair. A corpus fixture writes
    /// an object here, `{"tick", "native", "cycled"}` from the placements log, which this harness does not play
    /// from (`play_point`); its `tick` is the board a troop's point is resolved on (`resolve_tick`).
    #[serde(default)]
    pub tap: Option<serde_json::Value>,
    /// Where `pos` came from, as the scenario emitter labels it (observed_spawn, observed_spawn_centroid,
    /// tap_request_no_unit_observed, ...). Absent from a corpus fixture.
    #[serde(default)]
    pub pos_source: Option<String>,
    /// THE DESTINATION of a deploy of a card that travels underground (cards.json `spawn_pathfind`: the Miner,
    /// the Goblin Drill), native, as the fixture maker (tools/make_replay_fixture.py `tunnel_destinations`)
    /// reads it off the truth: where the Miner turns deploying after its tunnel, where the Drill's building
    /// first stands. Absent on every other deploy, and on a tunnel whose surfacing the frames do not hold.
    #[serde(default)]
    pub destination: Option<[i32; 2]>,
    /// A MIRROR play (`kind` "mirror", card Mirror): the card it copied, which is what the capture shows and what this
    /// harness plays (`mirror_play`). Absent on every other deploy.
    #[serde(default)]
    pub mirrored: Option<Mirrored>,
    /// THE FORM THE ROW PUT DOWN, as the fixture maker reads it off its units' card class (tools/make_replay_fixture.py
    /// `deploy_form`): "base", "ev1" (an evolved play: class 13) or "hero" (class 203). Absent from a fixture made
    /// before forms were read, whose rows all play their base card.
    #[serde(default)]
    pub form: Option<String>,
    /// The form's own card (Cannon_EV1, Musketeer_hero), which an "ev1" or "hero" row spawns (`deploy_play`). On a
    /// "base" row of a VARIANT card, the unit card the play put down (MergeMaiden_Mounted), which it spawns too.
    #[serde(default)]
    pub form_row: Option<String>,
}

/// The card a Mirror play copied, as the fixture maker names it (tools/make_replay_fixture.py `mirror_plays`).
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Mirrored {
    pub card: Option<String>,
    #[serde(default)]
    pub card_id: i64,
}

/// WHAT A DEPLOY ROW PLAYS through `spawn_unit`, which has no hand, no elixir and no play history: its own card, or
/// for a MIRROR play (`kind` "mirror") the card it copied, at the row's level -- the copy's, one above the copied
/// card's, as the capture shows it. The engine refuses the Mirror card itself there (state.rs
/// `refuse_unplaced_play`); its hand play (`resolve_play`) is what the unit tests measure. A mirror row whose copy is
/// not named is played as its card and refused, never guessed.
pub fn mirror_play(d: &Deploy) -> String {
    match (d.kind.as_str(), d.mirrored.as_ref().and_then(|m| m.card.clone())) {
        ("mirror", Some(copied)) => copied,
        _ => d.card.clone().unwrap_or_default(),
    }
}

/// A row's `form` for an evolved play, for a hero play and for a play of the card itself (`Deploy::form`).
pub const ROW_FORM_EVOLVED: &str = "ev1";
pub const ROW_FORM_HERO: &str = "hero";
pub const ROW_FORM_BASE: &str = "base";
/// A PRESS OF A HERO'S BUTTON is a row of its own: `kind` "ability", `card` the hero's base card, issued at `tick - 1`
/// as a deploy is. The press takes the side's button of that card (`BattleState::press_ability_button`), which exists
/// when the deck marks the card a hero (`deck_form`).
pub const KIND_ABILITY: &str = "ability";

/// WHAT A ROW SPAWNS: an evolved or hero row its form's own card, which loads as a card of its own (a hero play has
/// no counter to wait on, and `spawn_unit` has no play history for the evolution counter to read); every other row
/// `mirror_play`. A row whose form the engine does not load plays its base card (`config_for_with` notes it).
///
/// A VARIANT ROW plays the form its `form_row` names: a "base" row of a VARIANT card (card.rs `variant`: the Merge
/// Maiden, whose form is chosen by the elixir at the play) whose `form_row` is one of that card's forms. `spawn_unit`
/// has no elixir to choose by and refuses the variant card itself, naming its forms to place instead
/// (`refuse_unplaced_play`); the scenario maker names the form the truth shows (MergeMaiden_Mounted), which is also
/// the card the truth labels its unit by. A form_row naming anything else, and a Mirror row (its copy named by
/// `mirrored`, read by `mirror_play`), play as before. Plant: replay_refuses_a_variant_row.
pub fn deploy_play(d: &Deploy, db: &CardDb) -> String {
    match (d.form.as_deref(), d.form_row.as_deref()) {
        (Some(ROW_FORM_EVOLVED | ROW_FORM_HERO), Some(row)) if db.index(row).is_some() => row.to_string(),
        #[cfg(not(clash_plant = "replay_refuses_a_variant_row"))]
        (Some(ROW_FORM_BASE), Some(row)) if is_variant_row(d, row, db) => row.to_string(),
        _ => mirror_play(d),
    }
}

/// A row of a variant card whose `form_row` is one of the card's forms (`deploy_play`).
fn is_variant_row(d: &Deploy, row: &str, db: &CardDb) -> bool {
    let Some(own) = d.card.as_deref().and_then(|c| db.index(c)) else {
        return false;
    };
    db.get(own).variant().is_some_and(|opts| opts.iter().any(|o| db.get(o.card).name == row))
}

/// THE CARD A FORM'S UNITS ARE SCORED AS: the truth names an evolved or hero unit by its base card (its card id is
/// class 13 or 203, its name the base card's), so a form's card roots to its base (CardDb `forms`, `form_of`).
pub fn base_of_form(db: &CardDb, name: &str) -> String {
    let Some(i) = db.index(name) else {
        return name.to_string();
    };
    let base = db.forms.iter().find(|(_, _, f)| *f == i).map(|(b, _, _)| *b).or(db.get(i).form_of);
    base.map_or_else(|| name.to_string(), |b| db.get(b).name.clone())
}

/// THE FORM A DECK ENTRY IS MARKED WITH (`BattleConfig::forms`): FORM_HERO when a row of its side plays the card's hero
/// form, else FORM_EVOLUTION when one plays its evolution, else 0; 0 as well when the engine loads no such form.
pub fn deck_form(f: &Fixture, side: usize, name: &str, db: &CardDb) -> u8 {
    let mut form = 0;
    for d in f.deploys.iter().filter(|d| d.side as usize == side && d.card.as_deref() == Some(name)) {
        match d.form.as_deref() {
            Some(ROW_FORM_HERO) => form = FORM_HERO,
            Some(ROW_FORM_EVOLVED) if form == 0 => form = FORM_EVOLUTION,
            _ => {}
        }
    }
    match db.index(name) {
        Some(base) if db.form_card(base, form).is_some() => form,
        _ => 0,
    }
}

/// THE POINT A DEPLOY IS PLAYED AT, native.
///
/// A deploy that carries a `destination` (a card that tunnels) is played THERE: its `pos` is the tunnel's
/// first frame, next to its owner's King, and the engine puts the unit at its King itself and walks it to the
/// point it is played at (state.rs `phase_tunnel`). Plant: replay_plays_the_tunnel_start.
///
/// A scenario fixture's `pos` is what was SEEN. For a multi-unit troop that is the members' centroid
/// (pos_source observed_spawn_centroid), which lies off the tile the formation was laid around (the Rascals'
/// sits 238 below it). For a spell request no unit was seen at, it is the raw request
/// (tap_request_no_unit_observed), which the game plays at its tile centre. Both are played here at the TAP's
/// tile centre, and the engine's own placement then does what it does for the side, a side-1 ground
/// formation's one-unit offset included.
///
/// Every other deploy is played at `pos` as before, and so is every corpus deploy: a corpus fixture carries no
/// pos_source, and its taps are what a reader saw, not what the client resolved.
///
/// Measured on the client 15.535.29 sweep, within 250 from the tap:
///   - Rascals 941 -> 1,106 of 1,106;
///   - RoyalRecruits_Chess 2,380 -> 2,650 of 2,650;
///   - SkeletonWarriors_SpookyChess 2,230 -> 2,457 of 2,457.
///
/// Plant: replay_plays_the_centroid.
pub fn play_point(d: &Deploy) -> [i32; 2] {
    #[cfg(clash_plant = "replay_plays_the_centroid")]
    {
        return d.pos; // PLANT (regression): every deploy at what was seen.
    }
    #[cfg(not(clash_plant = "replay_plays_the_tunnel_start"))]
    if let Some(dest) = d.destination {
        return dest;
    }
    #[allow(unreachable_code)]
    let tile = SUBTILE / SUBTILE_PER_MILLITILE;
    let tapped = d.tap.as_ref().and_then(|t| {
        let a = t.as_array()?;
        let (x, y) = (a.first()?.as_i64()? as i32, a.get(1)?.as_i64()? as i32);
        Some([x.div_euclid(tile) * tile + tile / 2, y.div_euclid(tile) * tile + tile / 2])
    });
    match (d.pos_source.as_deref(), tapped) {
        (Some("observed_spawn_centroid" | "tap_request_no_unit_observed"), Some(t)) => t,
        _ => d.pos,
    }
}

/// THE RAW TAP A SCENARIO TROOP ROW IS PLAYED AT, native, under placement.TAP_SNAP = client16402_tile_centre: the row's
/// `tap` when it is an [x, y] pair (a scenario row), the card it plays is a troop and it carries no tunnel
/// `destination`. The engine snaps it to the very tile `play_point` names (the maker's `pos` for a single unit, the
/// tap's tile centre for a group), and its relocation off an own crown tower then reads the RAW tap, as the law says
/// (placement.TOWER_TAP_PUSH, `axis_push`). Played at the snapped `pos`, a tap on a tile line reached the push as that
/// tile's centre: Oracle's line tap (4500, 7000) on the side-0 princess box became (4500, 7500), a +x/+y tie, and went
/// +y to (4499, 8500) where the client put the Knight on (5499, 6500). None under placement.TAP_SNAP = none, where
/// the engine does not snap and the maker's `pos` is played as before.
///
/// Plant: replay_plays_the_snapped_tap.
pub fn scenario_troop_tap(s: &BattleState, db: &CardDb, d: &Deploy) -> Option<[i32; 2]> {
    #[cfg(clash_plant = "replay_plays_the_snapped_tap")]
    {
        let _ = (s, db, d);
        return None; // PLANT (regression): the row played at the maker's snapped pos, the push reading it as raw.
    }
    #[allow(unreachable_code)]
    if s.config().calib.placement_tap_snap != royalesim::state::TapSnap::TileCentre || d.destination.is_some() {
        return None;
    }
    let a = d.tap.as_ref()?.as_array()?;
    let (x, y) = (i32::try_from(a.first()?.as_i64()?).ok()?, i32::try_from(a.get(1)?.as_i64()?).ok()?);
    let troop = db.index(&deploy_play(d, db)).is_some_and(|i| db.get(i).kind == CardKind::Troop);
    troop.then_some([x, y])
}

/// THE TICK WHOSE BOARD A DEPLOY'S POINT IS RESOLVED ON, when that is not the issue tick (`tick - 1`).
///
/// The client resolves a tap -- snaps it, moves it off a tile it may not stand on -- when the player taps, and the
/// units appear 22 to 75 ticks later (median 25 over the 623 tapped troop rows of the 16.402 corpus). A law that
/// reads the board therefore reads the board of the TAP tick. Measured on the corpus: a troop tapped on the tile of
/// a Rage its own side cast 1 to 4 ticks before is laid one tile over, though the bottle is gone (C + 9) long before
/// the troop appears (C + 25 to C + 28): 20260918-134739 and 20260919-182539 (both seats) and 20260918-133849. The
/// same tile tapped 30 or more ticks after a cast is laid as tapped (20260919-182539 t532 and t1094, 20260919-143305).
///
/// A troop row of a corpus fixture carries its placements-log tap as an object with a `tick`; when that tick is before
/// the issue tick, the row's play point is resolved on that tick's board (`resolve_on_board`) and played, at the
/// issue tick, from the resolved point. Every other row (a scenario row, whose tap is an [x, y] pair and whose issue
/// tick is its tap; a spell; a building, which a corpus row plays where it was seen and a scenario row from its tap
/// (`scenario_building_tap`); a row with no tap) is resolved at its issue as before. `spawn_unit` resolves the point
/// again at the issue; a point already resolved is legal there unless the board changed under it, and then the issue
/// tick's law applies to it.
///
/// Plant: replay_resolves_at_issue.
pub fn resolve_tick(d: &Deploy) -> Option<u32> {
    #[cfg(clash_plant = "replay_resolves_at_issue")]
    {
        return None; // PLANT (regression): every deploy resolved on its issue tick's board.
    }
    #[allow(unreachable_code)]
    let tap = d.tap.as_ref()?.as_object()?;
    let t = u32::try_from(tap.get("tick")?.as_u64()?).ok()?;
    (d.kind == "troop" && t.saturating_add(1) < d.tick).then_some(t)
}

/// The point `spawn_unit` would put deploy `d` at on the board `s` holds NOW: `play_point`, through the engine's
/// own placement resolution (state.rs `resolve_point`: the snap, the relocation off a tile the troop may not take).
/// A corpus troop row (`observed_row`) takes `resolve_observed_point`: its point is the capture's, which the snap
/// must not move again. None for a card this CardDb does not hold.
pub fn resolve_on_board(s: &BattleState, db: &CardDb, d: &Deploy) -> Option<Vec2> {
    let idx = db.index(&deploy_play(d, db))?;
    let pp = scenario_troop_tap(s, db, d).unwrap_or_else(|| play_point(d));
    let p = from_native(pp[0], pp[1]);
    let team = team_of(d.side);
    Some(if observed_row(d, db) { s.resolve_observed_point(team, idx, p) } else { s.resolve_point(team, idx, p) })
}

/// THE TAP A SCENARIO BUILDING ROW IS LAID FROM, native: the row's `tap` when it is an [x, y] pair (a scenario row), the
/// card it plays (`deploy_play`) is a building, and it carries no tunnel `destination` (a surfacing the client already
/// resolved, `play_point`). The harness lays it through `spawn_unit_tapped`, which resolves the tap as a play's
/// (`building_placement`), where `spawn_unit` leaves a building where it was put and stacked a second Cannon on the
/// first. The RAW tap, not the row's snapped `pos`: the ring search ties on the distance to it (a Cannon tapped at
/// (9000, 14500), its box over the river, stands on (8500, 13500) in the client; from (9500, 14500) the search picks
/// (9500, 13500)). A card that tunnels into a building is one: the Goblin Drill of sweep-GoblinDrill, tapped at (9500,
/// 21500), surfaces on (9000, 21000), its building's 2x2 footprint (placement.SPAWN_PATHFIND_DESTINATION). None for
/// every other row: a corpus building stands where it was seen.
///
/// Plant: replay_stacks_scenario_buildings.
pub fn scenario_building_tap(d: &Deploy, db: &CardDb) -> Option<[i32; 2]> {
    #[cfg(clash_plant = "replay_stacks_scenario_buildings")]
    {
        let _ = (d, db);
        return None; // PLANT (regression): every building row through spawn_unit, stacked where it was put.
    }
    #[allow(unreachable_code)]
    let a = d.tap.as_ref()?.as_array()?;
    let (x, y) = (i32::try_from(a.first()?.as_i64()?).ok()?, i32::try_from(a.get(1)?.as_i64()?).ok()?);
    let building = db.index(&deploy_play(d, db)).is_some_and(|i| db.get(i).kind == CardKind::Building);
    (building && d.destination.is_none()).then_some([x, y])
}

/// ISSUE deploy row `d` of `team`, playing card `name` at `pos` (its `play_point`, or the point it was resolved on its
/// tap tick): a scenario building row from its tap (`scenario_building_tap`, `spawn_unit_tapped`), a corpus troop row
/// seen where it stood (`observed_row`, `spawn_unit_resolved`), every other row through `spawn_unit`.
pub fn issue_row(s: &mut BattleState, db: &CardDb, d: &Deploy, team: Team, name: &str, pos: Vec2) -> Result<(), DeployError> {
    if let Some([x, y]) = scenario_building_tap(d, db) {
        return s.spawn_unit_tapped(team, name, from_native(x, y), d.level);
    }
    if observed_row(d, db) {
        s.spawn_unit_resolved(team, name, pos, d.level)
    } else {
        s.spawn_unit(team, name, pos, d.level)
    }
}

/// A CORPUS TROOP row whose point the capture SAW: its `tap` is no [x, y] pair (a live capture's) and its `source`
/// is an observed point (the maker's "centroid" or "creation_point", or "laid_point", a single seen after its creation
/// tick put back on the tile point it was created on), which the client already resolved; it goes down through
/// `spawn_unit_resolved`. A corpus row played from its logged tap (source "tap_tile" or
/// "recovered_tile": a group laid around the tile), a spell row (an approximate landing point, which the snap puts
/// on the tile the cast was aimed at) and every scenario row (a tap) do not.
pub fn observed_row(d: &Deploy, db: &CardDb) -> bool {
    let corpus_row = !d.tap.as_ref().is_some_and(|t| t.is_array());
    let seen = matches!(d.source.as_str(), "centroid" | "creation_point" | "laid_point");
    corpus_row && seen && db.index(&deploy_play(d, db)).is_some_and(|i| db.get(i).kind == CardKind::Troop)
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

/// The card table a fixture's truth ran, as its generator names it (the scenario oracle's `card_table`). The corpus
/// maker writes none.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct CardTable {
    /// The client version whose card data the recorded battle ran ("15.535.29" on every scenario fixture).
    #[serde(default)]
    pub game_version: Option<String>,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Fixture {
    pub format: String,
    /// What every deploy's `tick` means; must be `DEPLOY_TICK_CONVENTION`. Optional here
    /// only so that its absence reaches `from_str` and is refused with a sentence, not a
    /// serde error.
    #[serde(default)]
    pub deploy_tick_convention: Option<String>,
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
    /// The client whose card data the truth ran (`own_client_card_values`). Absent on a corpus fixture.
    #[serde(default)]
    pub card_table: Option<CardTable>,
    /// The client's battle generator's state after each frame (`RngColumn`), where the fixture's maker recorded it.
    #[serde(default)]
    pub rng: Option<RngColumn>,
}

/// THE CLIENT'S BATTLE GENERATOR AS RECORDED (Oracle's specials fixtures): its algorithm, its state after the first frame,
/// and every later frame on which it changed, as (frame tick, state). Frame ticks, not the issued + 1 deploy labels.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct RngColumn {
    pub algorithm: String,
    pub first: [u32; 2],
    #[serde(default)]
    pub changes: Vec<[u32; 2]>,
    #[serde(default)]
    pub note: Option<String>,
}

/// The one algorithm the engine's client generator runs (state.rs `client_rnd`, client 15.535.29's xorshift32), named
/// in a fixture's `rng.algorithm` by its tail: the generator and the client version.
pub const RNG_ALGORITHM_TAIL: &str = "_xorshift32_v150535029";

impl RngColumn {
    /// The state after frame `tick`: the latest recorded at or before it; None before the first frame or under another
    /// algorithm.
    pub fn state_after(&self, tick: u32) -> Option<u32> {
        if !self.algorithm.ends_with(RNG_ALGORITHM_TAIL) || tick < self.first[0] {
            return None;
        }
        let mut st = self.first[1];
        for c in &self.changes {
            if c[0] <= tick {
                st = c[1];
            }
        }
        Some(st)
    }
}

impl Fixture {
    pub fn from_str(s: &str) -> Result<Fixture, String> {
        let f: Fixture = serde_json::from_str(s).map_err(|e| format!("fixture: {e}"))?;
        if f.format != FORMAT {
            return Err(format!("fixture format {} is not {FORMAT}", f.format));
        }
        match f.deploy_tick_convention.as_deref() {
            Some(DEPLOY_TICK_CONVENTION) => {}
            Some(other) => {
                return Err(format!(
                    "fixture deploy_tick_convention {other:?} is not {DEPLOY_TICK_CONVENTION:?}; this harness issues a deploy at tick - 1 so that its units exist on `tick`, which is right for that convention only"
                ))
            }
            None => {
                return Err(format!(
                    "fixture declares no deploy_tick_convention; played anyway, a label meaning anything but {DEPLOY_TICK_CONVENTION:?} would shift every deploy without a sign (regenerate it, or add the field if its generator's `tick` is the first effect frame)"
                ))
            }
        }
        for d in &f.deploys {
            if let Some(issued) = d.issued_tick {
                if issued + 1 != d.tick {
                    return Err(format!(
                        "deploy of {} (side {}) labelled tick {} carries issued_tick {issued}; under {DEPLOY_TICK_CONVENTION:?} it must be tick - 1",
                        d.card.as_deref().unwrap_or("?"),
                        d.side,
                        d.tick
                    ));
                }
            }
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
    /// The target's generation key, -1 for none; `None` when the frame did not RECORD one.
    /// The 15.535.29 scenario fixtures leave it null on 1,688 of 70,843 present unit-frames, mostly while
    /// deploying but 64 of them attacking, so null is "not recorded", never "no target".
    pub target: Option<i64>,
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
                // ON THE BOARD IS DECIDED BY POSITION AND HIT POINTS. A missing TARGET made the
                // whole row absent, so every scenario unit read "gone in the truth" for its
                // deploy frames and every fixture's first divergence was a death at t101 that
                // never happened. The target is an attribute of a present unit, not evidence
                // of presence.
                r.push(match (x[i], y[i], hp[i], pn[i], st[i]) {
                    (Some(x), Some(y), Some(hp), Some(pn), Some(st)) => Some(Row { x: x as i32, y: y as i32, hp: hp as i32, target: tg[i], path_n: pn as i32, state: st as i32 }),
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

/// THE TRUTH ENTITIES NO SCORE COUNTS (by index): no hitpoints at all (max_hp below 0) and the target of no entity on
/// any frame of the scene. The Hero Tombstone's deploy makes one on its point, NO_DAMAGE, UNTARGETABLE and
/// NO_CHECKCOLLISIONS, from the deploy to some 36 ticks after its press (client 15.535.29, card 203000088); the engine
/// runs it as code and carries no entity, so it took no pair (`a_truth_entity_with_no_hitpoints_takes_no_pair`) and
/// scored its whole life as missing (6.9 % of the nine hero-Tombstone scenes' non-tower unit-ticks). Sim's ruling
/// (2026-10-02): skip it in the harness, never in the engine, and print the count (`Report::unscored_dummies`). An entity
/// with no hitpoints that something does target stays scored: then it is a game object the engine owes.
pub fn unscored_dummies(truth: &TruthTable) -> BTreeSet<usize> {
    #[cfg(clash_plant = "replay_scores_a_dummy")]
    {
        let _ = truth;
        return BTreeSet::new(); // PLANT (regression): every dummy is scored as missing again.
    }
    #[cfg(not(clash_plant = "replay_scores_a_dummy"))]
    {
        let targeted: BTreeSet<i64> = truth.rows.iter().flat_map(|(_, r)| r.iter().flatten().filter_map(|w| w.target)).collect();
        truth.entities.iter().enumerate().filter(|(_, e)| e.max_hp < 0 && !targeted.contains(&e.key)).map(|(k, _)| k).collect()
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
    /// `tick_count()` of the board the play point was resolved on, when that was not the issue tick's
    /// (`resolve_tick`); None when it was resolved at the issue.
    pub resolved_on: Option<u32>,
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
    /// Its hit points on the tick it was registered (its full hp but for a hit that very tick): the pairing's tie-break
    /// between members on one point (`assign_tied`).
    pub first_hp: i32,
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
    /// Both-alive unit-ticks whose truth frame recorded no target, so no target was
    /// compared. The report's target column divides by unit_ticks and so counts them as
    /// unmatched, as it did when those ticks read as the unit being absent; subtract them
    /// from the denominator for the rate over compared ticks.
    pub target_unrecorded: u64,
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
        self.target_unrecorded += o.target_unrecorded;
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
        if self.hp_exact > self.both_alive || self.target_match + self.target_unrecorded > self.both_alive || self.path_n_match > self.both_alive {
            return Err("hp_exact / target_match + target_unrecorded / path_n_match exceed both_alive".into());
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
    /// THE LAST TICK SCORED when the battle ended in both (`towers_down_agree`): the engine's end. None when the
    /// engine's battle did not end, or ended where the client's did not; every frame is scored then.
    pub score_until: Option<u32>,
    pub deploys: Vec<DeployIssue>,
    pub pairs: Vec<Pair>,
    /// Entities on each side, matched or not.
    pub truth_entities: usize,
    pub sim_entities: usize,
    pub unmatched_truth: Vec<(i64, String)>,
    pub unmatched_sim: Vec<(u32, u32, String)>,
    /// THE TRUTH ENTITIES NOT SCORED (`unscored_dummies`): no hitpoints (max_hp below 0) and the target of no entity on
    /// any frame of the scene, so the engine carries no counterpart by design (the Hero Tombstone's visual dummy).
    /// Listed (key, card) so the exclusion stays visible: they are in neither `unmatched_truth` nor any score.
    pub unscored_dummies: Vec<(i64, String)>,
    /// THE FRAMES NOT SCORED AS TICKS: frames whose tick is the frame's before it, skipped by the scoring loop, so each
    /// truth tick is scored once. The client's recording repeats its last tick after the match ends (sp-hogs-musk-s0:
    /// tick 931, the king tower's fall, on 5,170 frames); scored per frame, each copy counted the fallen tower, which
    /// the engine removes, as a unit-tick lost. Counted here so the skip stays visible.
    pub repeated_frames: u64,
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
    /// The run's `--calibration-override`s as given (`section.KEY` -> JSON text); empty when
    /// the run is the shipped ledger. A score quoted from this report names its arm by this.
    pub calibration_overrides: BTreeMap<String, String>,
    /// THE CLIENT WHOSE CARD VALUES THE BATTLE RAN when it is not the ledger's (`own_client_card_values`): the fixture's
    /// card_table.game_version, and cards.CLIENT16402_VALUES ran at arm none. None: the ledger's arm, or the run's
    /// override of the key.
    pub card_values_client: Option<String>,
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
    /// Any overridable ledger key, `section.KEY` -> the value as JSON text
    /// (`--calibration-override`), applied before `attacking_movement`. The corpus judges a
    /// candidate like for like and the ledger every session reads stays as it is. The report's
    /// notes name every override, so a score says which arm it was taken on.
    pub calibration_overrides: BTreeMap<String, String>,
}

impl Default for Options {
    fn default() -> Self {
        Options { seed: 0, stride: 1, trace: false, prefix: false, attacking_movement: None, calibration_overrides: BTreeMap::new() }
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
    /// sim x, y (native), hp, attacking, path_n, target TRUTH KEY (-1 none, -2 targeting
    /// an entity the pairing never matched).
    ///
    /// SLOT 5 IS COMPARABLE WITH `truth`'s AND SLOT 3 IS NOT. The target is mapped into the
    /// truth's key space so the two columns answer the same question. `state` and
    /// `attacking` are different quantities that happen to share slot 3, and a reader
    /// comparing them is comparing nothing -- that one is not fixed here, only named.
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
    /// THE SIM'S HEADING AND AVOIDANCE OFFSET after this tick: facing x, y (length 256, the HEADING_LAW's integer
    /// normalize) and the avoidance offset (`EntityView::avoid_offset`). The recording carries the client's
    /// (`movement_direction`, `avoidance_offset`), so a reader can tell which way each engine steered round the same
    /// blocker from the same position. 20260918-124946's Ice Golem on t2302 and 20260918-122757.b2's Goblins on t1168
    /// turn the other way from identical positions, and the trace could not say why. Its own field, like `push`: absent
    /// is not zero.
    pub heading: Option<[i64; 3]>,
    /// THE SIM'S ATTACK CLOCKS after this tick: its attack progress and its load timer (`EntityView::attack_ms`,
    /// `attack_load_ms`). The recording carries the client's (`attack_progress_ms`, `attack_load_timer_ms`), so a reader
    /// can tell on which tick each engine's swing started and fired (sp-f4-ed-s0: the evolved Electro Dragon's shots one
    /// tick early from identical positions, which the trace could not place). Its own field, like `push`: absent is not
    /// zero.
    pub attack: Option<[i64; 2]>,
    /// THE SIM'S ROUTE after this tick (`EntityView::route`, goal first): its next waypoint and its goal-most node (native
    /// centres) and its node count, [0, 0, 0, 0, 0] with no route. The recording carries the client's (`path_nodes`,
    /// goal first, as half-tile cell ids), so a reader can tell where each engine's walk turns and what a dash aims at
    /// (sp-form-Tombstone-hero-s0 t194: the engine's red Skeleton turned on the bridge two ticks before the client's from
    /// points 4 apart, which the trace could not place). Its own field, like `push`: absent is not zero.
    pub route: Option<[i64; 5]>,
}

/// Which unit card (summon_only) each spawning card puts out, for rooting.
struct Roots {
    death_spawn_of: BTreeMap<u16, Vec<u16>>,
    spell_release_of: BTreeMap<u16, Vec<u16>>,
    /// SummonCharacterSecond units (the Goblin Gang's Spear Goblins, the Rascals'
    /// Girls): rooted to the card the harness itself deployed on that tick
    /// (card.rs `FormationDef::second_summon`).
    second_summon_of: BTreeMap<u16, Vec<u16>>,
    /// A hero button's unit (the Hero Musketeer's turret) -> the hero forms whose button puts it down: rooted to the
    /// hero's base card, as the truth names it.
    ability_of: BTreeMap<u16, Vec<u16>>,
    /// A unit an evolved form's own mechanic puts down (the Evo Royal Ghost's pair) -> the forms that do: rooted to the
    /// form's base card, as the truth names it.
    evo_unit_of: BTreeMap<u16, Vec<u16>>,
    /// A unit a card's deploy spawn area puts down (the Tri Wizards' TriWizard, Electro Wizard and Ice Wizard; card.rs
    /// `CardDef::deploy_spawn_area`) -> the cards whose area does: rooted to the card the harness deployed for that team
    /// within `DEPLOY_AREA_LOOKBACK` ticks, as the truth names all three by the card played.
    deploy_area_of: BTreeMap<u16, Vec<u16>>,
    /// A summon-only record that itself puts units on the board (the Goblin Drill's building, the
    /// Elixir Golem's ElixirGolem2) -> the playable card at the top of its chain, so a unit its death
    /// releases is rooted to that card and not to the summon-only record.
    card_of: BTreeMap<u16, u16>,
    /// A buff's death spawn: rooted to CARDLESS_ROOT, as the recording names it (`buff_root`).
    buff_death_spawn: BTreeSet<u16>,
    /// A scheduled death area's unit (the Suspicious Bush's goblins) -> the cards whose death area puts it down: rooted to
    /// the nearest recent death of one, looked for SCHEDULED_DEATH_LOOKBACK ticks back (`lookback`).
    scheduled_death_of: BTreeMap<u16, Vec<u16>>,
}

impl Roots {
    fn new(db: &CardDb) -> Roots {
        let mut death_spawn_of: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        let mut spell_release_of: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        let mut second_summon_of: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        let mut ability_of: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        let mut evo_unit_of: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        let mut deploy_area_of: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        let mut buff_death_spawn: BTreeSet<u16> = BTreeSet::new();
        let mut scheduled_death_of: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        // Every block card.rs `CardDb::unit_refs` names, matched without a wildcard: a
        // block added there does not compile here until it is rooted.
        for i in 0..db.cards.len() as u16 {
            for (path, unit, _) in db.unit_refs(i) {
                let of = match path {
                    UnitRef::DeathSpawn => &mut death_spawn_of,
                    // A death projectile's release (the Phoenix's egg) comes out of a death
                    // too, a tick after it: rooted to the nearest recent death, as a death spawn.
                    UnitRef::DeathProjectile => &mut death_spawn_of,
                    // A tunneller's building (the Goblin Drill's) appears on the tick its dig goes,
                    // where the dig came up: rooted to that disappearance, as a death spawn.
                    UnitRef::Morph => &mut death_spawn_of,
                    // A buff's death spawn (the Mother Witch's hog, the Goblin Curse's goblin) comes out of a death too,
                    // but belongs to the side OPPOSITE the dying unit's (the caster's), and the client reports its card
                    // as -1: rooted to CARDLESS_ROOT (`buff_root`).
                    UnitRef::BuffDeathSpawn => {
                        buff_death_spawn.insert(unit);
                        continue;
                    }
                    // a spell summon's unit (the Heal Spirit) is put down by its spell, as a release is
                    UnitRef::SpellRelease | UnitRef::SpellSummon => &mut spell_release_of,
                    UnitRef::SecondSummon => &mut second_summon_of,
                    // A spawner's unit is rooted through the entity that emitted it
                    // (`spawned_by`), not by card.
                    UnitRef::Spawner => continue,
                    // a life-state controller's wave is emitted by its hut, rooted like a spawner's
                    UnitRef::LifeState => continue,
                    // an attached rider is rooted through the mount it rides (`attached_to`), not by card
                    UnitRef::Attach => continue,
                    // a deploy's members at explicit offsets (the Three Musketeers' second and third) exist from the
                    // Spawn phase of the deploy the harness issued, as a second summon's do
                    UnitRef::SummonMember(_) => &mut second_summon_of,
                    // a variant card's form is a card of its own, deployed as itself: rooted as "deployed"
                    UnitRef::VariantForm(_) => continue,
                    // a transformation keeps the entity: it was rooted when it appeared, and its root stays
                    UnitRef::Transform => continue,
                    // a scheduled area's units: a spell's (the Graveyard's Skeletons) are put down by its spell, as a
                    // release is; a death area's (the Suspicious Bush's goblins) come out of a death, as a death spawn
                    UnitRef::Scheduled(_) if db.get(i).spell.is_some() => &mut spell_release_of,
                    UnitRef::Scheduled(_) => &mut scheduled_death_of,
                    // a hero button's unit (the Hero Musketeer's turret) comes from a press (a row of kind "ability"):
                    // rooted to its hero
                    UnitRef::AbilityUnit => &mut ability_of,
                    // an evolved form's own unit (the Evo Royal Ghost's pair) comes from the form's hit: rooted to it
                    UnitRef::EvoUnit(_) => &mut evo_unit_of,
                    // a deploy spawn area's units (the Tri Wizards' three) come ticks after the play that cast it
                    UnitRef::DeploySpawn(_) => &mut deploy_area_of,
                };
                of.entry(unit).or_default().push(i);
            }
        }
        // The top of each summon-only record's chain: the first playable card, in CardDb order, whose
        // blocks reach it, level by level (py.rs `ids_of_indices` attributes a unit the same way).
        let mut card_of: BTreeMap<u16, u16> = BTreeMap::new();
        let mut frontier: Vec<(u16, u16)> = (0..db.cards.len() as u16)
            .filter(|i| !db.get(*i).summon_only && db.index(&db.get(*i).name) == Some(*i))
            .map(|i| (i, i))
            .collect();
        while !frontier.is_empty() {
            let mut next = Vec::new();
            for (top, idx) in frontier {
                for (_, u, _) in db.unit_refs(idx) {
                    if (u as usize) < db.cards.len() && db.get(u).summon_only && !card_of.contains_key(&u) {
                        card_of.insert(u, top);
                        next.push((top, u));
                    }
                }
            }
            frontier = next;
        }
        Roots { death_spawn_of, spell_release_of, second_summon_of, ability_of, evo_unit_of, deploy_area_of, card_of, buff_death_spawn, scheduled_death_of }
    }

    /// CARDLESS_ROOT for a buff's death spawn, else None.
    fn buff_root(&self, idx: u16) -> Option<&'static str> {
        // PLANT (regression) replay_roots_buff_spawn_by_unit: a buff's death spawn is rooted as before, by its own card.
        #[cfg(clash_plant = "replay_roots_buff_spawn_by_unit")]
        return None;
        #[allow(unreachable_code)]
        {
            self.buff_death_spawn.contains(&idx).then_some(CARDLESS_ROOT)
        }
    }

    /// The cards whose death puts down unit `idx`, each with how many ticks back its death is looked for: a death
    /// spawn's parent, DEATH_SPAWN_LOOKBACK; a scheduled death area's card, SCHEDULED_DEATH_LOOKBACK. None when no card's
    /// death puts it down.
    fn death_parents(&self, idx: u16) -> Option<Vec<(u16, u32)>> {
        // PLANT (regression) replay_scheduled_lookback_short: a scheduled death area's unit is looked for 3 ticks back.
        #[cfg(not(clash_plant = "replay_scheduled_lookback_short"))]
        let scheduled = SCHEDULED_DEATH_LOOKBACK;
        #[cfg(clash_plant = "replay_scheduled_lookback_short")]
        let scheduled = DEATH_SPAWN_LOOKBACK;
        let mut out: Vec<(u16, u32)> = self.death_spawn_of.get(&idx).into_iter().flatten().map(|&p| (p, DEATH_SPAWN_LOOKBACK)).collect();
        // BOTH MAPS: a unit that is one card's death spawn and another's scheduled death area's unit (the Skeleton: the
        // Tombstone's death spawn, the Hero Tombstone's monster's death area's four) is looked for among both. Read from
        // the death spawn map alone whenever it named the unit, the monster's Skeletons rooted to nothing
        // (sp-hero2-Tombstone-death-s0: four unmatched pairs from t390, its first divergence).
        // PLANT (regression) replay_scheduled_parents_shadowed: the scheduled map read only for a unit no death spawns.
        #[cfg(not(clash_plant = "replay_scheduled_parents_shadowed"))]
        let shadowed = false;
        #[cfg(clash_plant = "replay_scheduled_parents_shadowed")]
        let shadowed = !out.is_empty();
        if !shadowed {
            out.extend(self.scheduled_death_of.get(&idx).into_iter().flatten().map(|&p| (p, scheduled)));
        }
        if out.is_empty() {
            None
        } else {
            Some(out)
        }
    }

    /// The playable card a record roots to: itself when it is one, else the top of its chain.
    fn card(&self, idx: u16) -> u16 {
        self.card_of.get(&idx).copied().unwrap_or(idx)
    }
}

/// `Roots::buff_root` for the unit named `unit` (tests).
pub fn buff_death_spawn_root(db: &CardDb, unit: &str) -> Option<&'static str> {
    db.index(unit).and_then(|i| Roots::new(db).buff_root(i))
}

/// `Roots::death_parents` for the unit named `unit` (tests): the parent cards' names and the longest lookback.
pub fn death_spawn_parents(db: &CardDb, unit: &str) -> Option<(Vec<String>, u32)> {
    let roots = Roots::new(db);
    let i = db.index(unit)?;
    roots.death_parents(i).map(|p| (p.iter().map(|(c, _)| db.get(*c).name.clone()).collect(), p.iter().map(|(_, l)| *l).max().unwrap_or(0)))
}

/// `Roots::death_parents` for the unit named `unit` (tests): each parent card's name with its own lookback.
pub fn death_parent_lookbacks(db: &CardDb, unit: &str) -> Vec<(String, u32)> {
    let roots = Roots::new(db);
    db.index(unit).and_then(|i| roots.death_parents(i)).map_or(Vec::new(), |p| p.iter().map(|(c, l)| (db.get(*c).name.clone(), *l)).collect())
}

/// The containers still holding their units (spell.rs `FuseEnd`): every live spell object of a death bomb that carries
/// a death spawn, as (team, record, point), less one that has released its units ahead of its hit
/// (`royalesim::spell::FUSE_RELEASED`). A container that leaves this list between two ticks released its units then.
fn containers_waiting(s: &BattleState) -> Vec<(Team, u16, Vec2)> {
    let db = s.cards();
    s.spells()
        .iter()
        .filter(|sp| db.get(sp.card).death_bomb_fuse_ms().is_some() && db.get(sp.card).death_spawn.is_some())
        .filter_map(|sp| match sp.motion {
            royalesim::spell::SpellMotion::Flight { aim, delay_ms, .. } if delay_ms != royalesim::spell::FUSE_RELEASED => Some((sp.team, sp.card, aim)),
            _ => None,
        })
        .collect()
}

/// The ledger key whose values are one client's (calibration cards.CLIENT16402_VALUES).
pub const CARD_VALUES_KEY: &str = "cards.CLIENT16402_VALUES";
/// The client whose card values that key's arm client16402 carries.
pub const CARD_VALUES_CLIENT: &str = "16.402";

/// A CAPTURE RUNS THE CARD VALUES OF THE CLIENT THAT RECORDED IT. cards.CLIENT16402_VALUES ships client16402: the 16.402
/// client's values (the Fire Spirits' Hitpoints 84, the Bomber's projectile Damage 83, ...) replace the 15.535.29
/// tables' (85, 88) in every battle. A fixture whose truth another client recorded -- every scenario fixture names
/// card_table.game_version 15.535.29 -- was played on the tables' values, so replayed on the 16.402 ones its hp differs
/// from the first hit or the first frame whatever the mechanics do: the client 15.535.29 sweep's Fire Spirits scene is
/// created at 217 hp and the engine made it 215, and its Bomber's bombs take 225 off the Knight where the engine took
/// 212. Returns that fixture's client version, and the battle runs the tables' values (arm none). None -- the shipped
/// arm -- for a fixture that names no client (the corpus maker's) or names 16.402, and for a run that overrides the key
/// itself (`--calibration-override`), whose arm is the run's.
pub fn own_client_card_values(f: &Fixture, overrides: &BTreeMap<String, String>) -> Option<String> {
    if overrides.contains_key(CARD_VALUES_KEY) {
        return None;
    }
    let v = f.card_table.as_ref()?.game_version.as_deref()?;
    #[cfg(not(clash_plant = "replay_card_values_by_ledger"))]
    let other = v != CARD_VALUES_CLIENT && !v.starts_with(&format!("{CARD_VALUES_CLIENT}."));
    #[cfg(clash_plant = "replay_card_values_by_ledger")]
    let other = false; // PLANT: the ledger's arm for every capture, whatever client recorded it.
    other.then(|| v.to_string())
}

/// THE MECHANICS CLIENT 15.535.29 RUNS APART FROM THE LEDGER'S (the 16.402 client's), as (`section.KEY`, JSON value): a
/// capture that client recorded (card_table.game_version) runs each, unless the run overrides its key.
///   movement.DYING_UNIT_VISIBILITY = client_doomed_static: a troop whose death is settled before the move pass is a
///   static obstacle to every avoidance scan. Measured on client 15.535.29: Oracle's sp-order-{kvm,vmk}-{1500,2500}-s0
///   scenes (the killer created before and after the mover) exact to their end under it and departing at the first kill
///   under whole_tick; sp-f4-furnace-s0 t267, sp-hogs-cannon-s0 t285, sp-ram-kill-s0 t309 exact. The 16.402 corpus keeps
///   whole_tick (20260918-122757.b2 t1067; 31 of 31 first effects of the arm on 9 battles nearer under whole_tick).
///   match.TICK_ORDER = client_sequential_strike: the Target and Attack phases one pass in creation order, a direct strike
///   landing at once, so a walker created after a melee striker turns for its next goal on the kill frame. Measured on
///   client 15.535.29 over every death with one landed blow: walkers created after the striker 114 of 124 (unit deaths)
///   and 8 of 8 (towers near) on the kill frame, before it 72 of 75 and 2 of 2 a frame later; an attacker's post-kill
///   wait ends on the sixth frame either way (187 and 376). The 16.402 corpus keeps client16402 (the ledger's
///   measured_16402_2026_09_26: 104 of 112 a frame later where the arm puts all 112 on the kill frame).
///   combat.DEATH_DAMAGE_TICK = client15535_death_tick: a dying unit's death blow lands on its death tick. Measured on
///   client 15.535.29: an enemy within a dying Golem's or Ice Golemite's death radius loses the damage on the death
///   frame, 122 of 122. The 16.402 corpus keeps next_tick (9 of 10 on the frame after).
///   knockback.LADDER_END_ROUTE = client15535_kept: a knockback ladder's end leaves the unit's route as it was (135 of 154
///   ladders on client 15.535.29). The 16.402 corpus's one ladder with a route does not separate the arms.
///   movement.HELD_FACING = client15535_toward_waypoint: a held unit with a route turns its facing toward its next
///   waypoint (143 of 163 held facing turns on client 15.535.29). The 16.402 corpus is not measured for it.
///   collision.HELD_UNIT_AVOIDANCE = scanned: a held unit starts avoidance offsets at the walking rate on client 15.535.29
///   (41 of 5,442 held ticks against 0.90% walking); the 16.402 corpus keeps masked (0 of 2,024).
///   movement.DOOMED_OWN_UPDATE = client15535_kamikaze_stays: a kamikaze doomed before the move pass stays where the tick
///   found it (a Battle Ram killed by its own hit lays its Barbarians on its last point, 29 of 29 on client 15.535.29).
///   movement.KAMIKAZE_DEATH_CONTACT = client15535_avoided_not_pushed: a dying kamikaze pushes nobody on its death tick and
///   is still steered round (18 of 18 overlapping neighbours exact on the death tick under it on client 15.535.29).
///   targeting.CHASE_HOLD_PAST_LIMIT = client15535_troops_kept: a troop keeps a target past its round sight past the
///   chase-drop limit when the pair was never inside it (135 of 139 on client 15.535.29; the 16.402 corpus let 3 of 4 go).
///   combat.LAUNCH_PAST_TARGET = client15535_homing_unclamped: a homing shot starts its whole ProjectileStartRadius out,
///   past a nearer target (the Hero Musketeer's near shots 6 of 6 on client 15.535.29).
///   placement.RELOCATION_TIE_ORDER = client15535_arena_clockwise: equally near relocations of a building tap go to the
///   first in the arena order -y, -x, +y, +x seen from the tap, for both seats (4 of 4 Elixir Collector taps and a
///   Cannon tapped on a tile edge, 10 of 10, on client 15.535.29).
///   targeting.KNOCKED_TARGET_HOLD = client15535_sight_keep: a knocked unit keeps a target that is not a crown tower only
///   within its sight + both radii + 25 (9 of 9 let go past it on client 15.535.29, none within it).
///   targeting.SLAP_FLIGHT_TARGETABILITY = client15535_airborne: a unit in a Hero Giant's slap flight is a target only for
///   attackers that attack air (5 of 5 ground-only holders let it go, 6 of 6 air ones kept it on client 15.535.29).
///   spawner.EVO_COPY_COUNT = client15535_at_hit: an Evo Skeletons group's room for a copy is read at the hit, before the
///   tick's deaths (6 of 6 hits on a group of 8 with a member dying that tick made none on client 15.535.29).
///   spawner.CONTAINER_BURST_PUSH = client15535_contact_push: a container's members are laid around the point the contact
///   law pushes it to (a Skeleton 217 off, (-130, +73), exact on client 15.535.29); their slides end around its own.
///   movement.AVOIDANCE_OBSTACLE_TAG = client15535_unpushed_obstacle: a row's AVOIDANCE_AS_OBSTACLE tag (the Evo Skeleton
///   Army's General) makes its carrier unpushed, still pushing, and a static avoidance obstacle (93 of 93 deploy ticks
///   unmoved).
///   combat.RANDOM_DELAY_STREAM = client15535_battle_stream: a RandomDelay shot's delay draws from the client's battle
///   generator, which the replay syncs (56 of 56 Hunter volleys on client 15.535.29).
///   combat.LOAD_FIRST_HIT_LEAVE = client15535_windup_refunded: a Sparky leaving its attack before it fires gets the
///   entry's windup back (its load timer LoadTime less its progress; every Sparky leave on client 15.535.29).
///   placement.ILLEGAL_TROOP_TAP = client15535_clamp_to_legal_edge: a troop tapped outside its territory goes down on the
///   first legal tile back along its column (4 of 4 bridge taps, 3 of 3 enemy-half singles on client 15.535.29).
///   spawner.DEATH_RING_AXIS = client15535_unit_heading: a death ring's degree is read off its members' heading, the
///   direction normalized to 256 (7 of 7 Battle Ram deaths where it rounds apart from the raw direction).
///   combat.PASS_KILL_CHASE = client15535_chaser_reads_pass: an attacker chasing its target out of reach reads a kill
///   earlier in the sequential pass and takes its next target at once (19 of 25 created after the striker).
///   movement.KNOCKED_DOOMED_AVOIDANCE = client15535_knocked_mover: a doomed troop mid-knockback (an Evo Cannon bomb's
///   victim) is no static obstacle to the avoidance scan (8 of 8 scanners kept their offset).
///   knockback.TROOP_DEATH_PUSHBACK = client15535_ladder: a dying Golem or Golemite pushes the units its death blow hits
///   on the knockback ladder of its DeathPushBack (43 of 43 enemies in reach).
///   movement.STRUCK_CONTACT_ORDER = client15535_after_striker: a troop struck down in the sequential pass is a body to
///   the movers created before its striker only, and stays when its striker came before it.
///   transform.FALL_GROUNDING = client15535_late_kept_walk: a falling Evo Royal Hog leaves the air on the tick after its
///   landing tick, after the Target phase, keeping its walk (ground-only enemies took it two ticks after, 4 of 4).
///   combat.CAGE_CAPTIVE_SHOTS = client15535_before_shots_hidden_after_snap: an Evo Goblin Cage drags its captive before
///   the tick's shots step and hides it the tick after the snap (a tower's arrow landed on every dragged captive).
///   combat.LOAD_FIRST_HIT_KILL_WAIT = client15535_skipped: a Sparky whose target dies mid-swing takes its next target on
///   the tick after (4 of 4), serving no post-kill wait.
///   targeting.EQUAL_DISTANCE_TIE = client15535_later_created: of two enemies at one distance the later created is taken
///   (every exact troop tie, 5 of 5).
///   hide.SHOT_AT_HIDING_BUILDING = client15535_lands: a shot fired at a Tesla while it was up lands on it after it goes
///   under (2 of 2).
///   combat.DASH_CHAIN_IMMUNITY = client15535_whole_chain: a Golden Knight in his chain takes no hit (7 of 7 dropped).
///   targeting.GHOST_PAIR_FIRST_FRAME = client15535_untargetable: an Evo Ghost's pair is no one's target on its first frame.
///   combat.EVO_CHAIN_HOP_WAIT = client15535_two_ticks: an Evo Electro Dragon's hops after the first wait 2 ticks (67 of 77).
///   status.ATTRACT_WATER_EDGE = client15535_pull_stops: a ground unit's pulled step stops at a water cell's edge.
///   status.ATTRACT_ONSET = client_next_tick: a pulling area moves its victims on the tick after each of its own ticks
///   (the Tornado's D + 1..D + 21, the Evo Valkyrie's tornado H + 2..H + 11, every tick of both).
///   movement.CAST_HOLD_HEADING = client15535_not_counted: a unit in its own ability's cast steers no neighbour by its
///   heading (a walker passing the Skeleton King turned on his cast's first frame; read, LOW).
///   spawner.DEATH_BOMB_TIMING_SCOPE = client15535_every_bomb: a plain death bomb lands on its fuse's last tick (5 of 5).
///   knockback.DEATH_PUSHBACK = every_death_bomb_ladder: a Giant Skeleton's bomb pushes the troops it hits (1 of 1).
///   targeting.CHASE_DROP_MEASURE = client15535_lane_dy: the chase-drop limit is measured on |dy| (41 of 42 drops; 19
///   of 19 targets past it across alone kept).
///   targeting.CHASE_DROP_WALKING_AWAY = client15535_growing_away: the edge lets go of a troop that walked away as the
///   Target phase began with |dy| growing; only the drop tick's rescan passes over another one walking away.
///   targeting.SNIPE_REPICK = client15535_last_target_first: an Evo Musketeer's next snipe takes her last snipe's target
///   first (1 decisive re-pick).
///   movement.JUMP_LANDING_SCOPE = client15535_whole_tick: a river jump's lander is no body for its landing tick's first
///   updates either (1 of 1).
///   movement.HELD_WAYPOINT_TEST = client15535_run: a held unit runs its waypoint's reached test (120 of 120 pops, 3,111
///   of 3,111 keeps).
///   spawner.SPECTRAL_FIRST_UPDATE = client15535_same_tick: an Evo Skeleton Army Spectral takes its first update on the
///   tick it is made (43 of 43 hold a target on their first frame).
///   transform.DISMOUNT_LEAP_STEP = client15535_leap_lands_first: a Hero Dark Prince freed on its leap's last tick takes
///   that leap step before it dismounts (1 of 1).
///   transform.DISMOUNT_HOP_WATER = client15535_land_row_centre: its hop onto the river lands on the nearest land row's
///   centre (5 of 5).
///   combat.DASH_CHAIN_AIM = client15535_goal_cell: a dash chain's dash steps toward its target's goal cell centre (43 of
///   45 steps exact).
///   combat.KAMIKAZE_LAUNCH_PASS = client15535_gone_at_launch: a kamikaze's launch removes it for every later unit of the
///   sequential pass (10 of 10 pickers, 7 of 7 walking holders).
///   pathfinding.PRESS_ROUTE = client15535_replanned: a ground hero's press with CastTime 0 drops its route (22 of 22).
///   combat.SPIN_BEGIN = client15535_next_tick: a Hero Valkyrie's spin begins the tick after her button fires (1 press).
///   formation.LINE_FRAME = client15535_y_reflection: side 1's line is placed as side 0's at the same arena x (2 of 2).
///   placement.TROOP_RELOCATION_TIE_ORDER = client15535_arena_clockwise: a troop tap's equally near relocations go to the
///   arena order -y, -x, +y, +x seen from the tap, both seats (10 of 10).
///   spawner.FIRST_STEP_DYING_CONTACT = client15535_avoidance_only: a first update meets the units dying on its tick in
///   the avoidance scan alone (1 emission).
///   movement.CHAIN_LANDED_BODY = client15535_no_body_to_end: a dash chain's champion is no contact body from a landing to
///   his chain's end (3 of 3 last landings).
///   targeting.CHASE_RESCAN_PASS_OVER = client15535_receding_lane_walk: a unit walking for its tower passes over every
///   troop past the chase-drop limit whose |dy| grew (about 600 rescans).
///   combat.DIRECT_HIT_BUFF_COUNTDOWN = client15535_landing_tick: an instant hit's buff holds one tick fewer (17 of 17).
///   spawner.SOUL_POINT_BASE = client15535_post_move: a Skeleton King's copies are drawn around his post-move point (28 of
///   34).
///   targeting.WALKING_KEEP_REACH = client15535_walking_reach: a walking Inferno Dragon keeps its target only within the
///   reach it walks to (1 of 1; every other walking holder kept, 258 of 258).
///   combat.STRAIGHT_SHOT_BUILDING_REACH = client15535_rounded_square: a Hunter's pellet reaches a building's square, not
///   its circle (136 of 136 pellet ends).
pub const CLIENT15535_ARMS: &[(&str, &str)] = &[
    ("movement.DYING_UNIT_VISIBILITY", "\"client_doomed_static\""),
    ("match.TICK_ORDER", "\"client_sequential_strike\""),
    ("combat.DEATH_DAMAGE_TICK", "\"client15535_death_tick\""),
    ("knockback.LADDER_END_ROUTE", "\"client15535_kept\""),
    ("movement.HELD_FACING", "\"client15535_toward_waypoint\""),
    ("collision.HELD_UNIT_AVOIDANCE", "\"scanned\""),
    ("movement.DOOMED_OWN_UPDATE", "\"client15535_kamikaze_stays\""),
    ("movement.KAMIKAZE_DEATH_CONTACT", "\"client15535_avoided_not_pushed\""),
    ("targeting.CHASE_HOLD_PAST_LIMIT", "\"client15535_troops_kept\""),
    ("combat.LAUNCH_PAST_TARGET", "\"client15535_homing_unclamped\""),
    ("placement.RELOCATION_TIE_ORDER", "\"client15535_arena_clockwise\""),
    ("targeting.KNOCKED_TARGET_HOLD", "\"client15535_sight_keep\""),
    ("targeting.SLAP_FLIGHT_TARGETABILITY", "\"client15535_airborne\""),
    ("spawner.EVO_COPY_COUNT", "\"client15535_at_hit\""),
    ("spawner.CONTAINER_BURST_PUSH", "\"client15535_contact_push\""),
    ("movement.AVOIDANCE_OBSTACLE_TAG", "\"client15535_unpushed_obstacle\""),
    ("placement.ILLEGAL_TROOP_TAP", "\"client15535_clamp_to_legal_edge\""),
    ("combat.LOAD_FIRST_HIT_LEAVE", "\"client15535_windup_refunded\""),
    ("combat.RANDOM_DELAY_STREAM", "\"client15535_battle_stream\""),
    ("spawner.DEATH_RING_AXIS", "\"client15535_unit_heading\""),
    ("combat.PASS_KILL_CHASE", "\"client15535_chaser_reads_pass\""),
    ("movement.KNOCKED_DOOMED_AVOIDANCE", "\"client15535_knocked_mover\""),
    ("knockback.TROOP_DEATH_PUSHBACK", "\"client15535_ladder\""),
    ("movement.STRUCK_CONTACT_ORDER", "\"client15535_after_striker\""),
    ("transform.FALL_GROUNDING", "\"client15535_late_kept_walk\""),
    ("combat.CAGE_CAPTIVE_SHOTS", "\"client15535_before_shots_hidden_after_snap\""),
    ("combat.LOAD_FIRST_HIT_KILL_WAIT", "\"client15535_skipped\""),
    ("targeting.EQUAL_DISTANCE_TIE", "\"client15535_later_created\""),
    ("hide.SHOT_AT_HIDING_BUILDING", "\"client15535_lands\""),
    ("combat.DASH_CHAIN_IMMUNITY", "\"client15535_whole_chain\""),
    ("targeting.GHOST_PAIR_FIRST_FRAME", "\"client15535_untargetable\""),
    ("combat.EVO_CHAIN_HOP_WAIT", "\"client15535_two_ticks\""),
    ("status.ATTRACT_WATER_EDGE", "\"client15535_pull_stops\""),
    ("status.ATTRACT_ONSET", "\"client_next_tick\""),
    ("movement.CAST_HOLD_HEADING", "\"client15535_not_counted\""),
    ("spawner.DEATH_BOMB_TIMING_SCOPE", "\"client15535_every_bomb\""),
    ("knockback.DEATH_PUSHBACK", "\"every_death_bomb_ladder\""),
    ("targeting.CHASE_DROP_MEASURE", "\"client15535_lane_dy\""),
    ("targeting.CHASE_DROP_WALKING_AWAY", "\"client15535_growing_away\""),
    ("targeting.SNIPE_REPICK", "\"client15535_last_target_first\""),
    ("movement.JUMP_LANDING_SCOPE", "\"client15535_whole_tick\""),
    ("movement.HELD_WAYPOINT_TEST", "\"client15535_run\""),
    ("spawner.SPECTRAL_FIRST_UPDATE", "\"client15535_same_tick\""),
    ("transform.DISMOUNT_LEAP_STEP", "\"client15535_leap_lands_first\""),
    ("transform.DISMOUNT_HOP_WATER", "\"client15535_land_row_centre\""),
    ("combat.DASH_CHAIN_AIM", "\"client15535_goal_cell\""),
    ("combat.KAMIKAZE_LAUNCH_PASS", "\"client15535_gone_at_launch\""),
    ("combat.STRAIGHT_SHOT_BUILDING_REACH", "\"client15535_rounded_square\""),
    ("pathfinding.PRESS_ROUTE", "\"client15535_replanned\""),
    ("combat.SPIN_BEGIN", "\"client15535_next_tick\""),
    ("formation.LINE_FRAME", "\"client15535_y_reflection\""),
    ("placement.TROOP_RELOCATION_TIE_ORDER", "\"client15535_arena_clockwise\""),
    ("spawner.FIRST_STEP_DYING_CONTACT", "\"client15535_avoidance_only\""),
    ("movement.CHAIN_LANDED_BODY", "\"client15535_no_body_to_end\""),
    ("targeting.CHASE_RESCAN_PASS_OVER", "\"client15535_receding_lane_walk\""),
    ("combat.DIRECT_HIT_BUFF_COUNTDOWN", "\"client15535_landing_tick\""),
    ("spawner.SOUL_POINT_BASE", "\"client15535_post_move\""),
    ("targeting.WALKING_KEEP_REACH", "\"client15535_walking_reach\""),
];

/// The client version a capture names (card_table.game_version), when it is 15.535.29's.
fn capture_client15535(f: &Fixture) -> bool {
    f.card_table.as_ref().and_then(|c| c.game_version.as_deref()).is_some_and(|v| v == "15.535.29" || v.starts_with("15.535."))
}

/// Build the engine config a fixture asks for.
pub fn config_for(f: &Fixture, db: CardDb) -> Result<(BattleConfig, Vec<String>), String> {
    config_for_with(f, db, None, &BTreeMap::new())
}

/// As `config_for`, with one calibration candidate overridden for this run.
pub fn config_for_with(
    f: &Fixture,
    db: CardDb,
    attacking_movement: Option<royalesim::state::AttackingUnitMovement>,
    overrides: &BTreeMap<String, String>,
) -> Result<(BattleConfig, Vec<String>), String> {
    let mut cfg = BattleConfig::with_cards(db);
    let mut notes = Vec::new();
    // A 15.535.29 capture runs that client's own mechanics (`CLIENT15535_ARMS`), each unless the run overrides its key.
    let mut merged = overrides.clone();
    #[cfg(not(clash_plant = "replay_client_arms_unread"))]
    if capture_client15535(f) {
        for (k, v) in CLIENT15535_ARMS {
            if !overrides.contains_key(*k) {
                merged.insert((*k).to_string(), (*v).to_string());
                notes.push(format!("client 15.535.29 runs {k} = {v}"));
            }
        }
    }
    let overrides = &merged;
    if !overrides.is_empty() {
        let (c, applied) = royalesim::state::Calib::shipped_with_overrides(overrides)?;
        cfg.set_calib(c);
        for (k, v) in &applied {
            notes.push(format!("calibration override {k} = {v}"));
        }
    }
    if let Some(arm) = attacking_movement {
        cfg.calib.attacking_unit_movement = arm;
    }
    if own_client_card_values(f, overrides).is_some() {
        cfg.calib.card_values = royalesim::state::CardValuesArm::None;
    }
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
        let forms: Vec<u8> = names.iter().map(|n| deck_form(f, side, n, &cfg.cards)).collect();
        if forms.iter().any(|&m| m != 0) {
            cfg.forms[side] = forms;
        }
        for d in f.deploys.iter().filter(|d| d.side as usize == side) {
            if let (Some(form @ (ROW_FORM_EVOLVED | ROW_FORM_HERO)), Some(row)) = (d.form.as_deref(), d.form_row.as_deref()) {
                let note = format!("side {side}: form {row} does not load; its {form} rows play {}", mirror_play(d));
                if cfg.cards.index(row).is_none() && !notes.contains(&note) {
                    notes.push(note);
                }
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
        score_until: None,
        deploys: Vec::new(),
        pairs: Vec::new(),
        truth_entities: 0,
        sim_entities: 0,
        unmatched_truth: Vec::new(),
        unmatched_sim: Vec::new(),
        unscored_dummies: Vec::new(),
        repeated_frames: 0,
        score: Score::default(),
        score_no_towers: Score::default(),
        per_card: BTreeMap::new(),
        first_divergence: None,
        card_families: BTreeMap::new(),
        level_deviations: Vec::new(),
        cards_json_fixture: f.cards_json_fnv1a64.clone(),
        cards_json_engine: cards_json_hash().ok(),
        notes: Vec::new(),
        calibration_overrides: BTreeMap::new(),
        card_values_client: None,
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
    let dummies = unscored_dummies(&truth);
    report.unscored_dummies = dummies.iter().map(|&k| (truth.entities[k].key, truth.entities[k].card.clone().unwrap_or_default())).collect();
    let (cfg, notes) = config_for_with(f, db.clone(), opts.attacking_movement, &opts.calibration_overrides)?;
    // config_for_with reports the level deviations and the applied overrides in one list; the
    // overrides are the run's ARM, not a level fact, so they go to `notes` and their own field.
    let (overrides, levels): (Vec<String>, Vec<String>) = notes.into_iter().partition(|n| n.starts_with("calibration override "));
    report.level_deviations = levels;
    report.notes.extend(overrides);
    report.calibration_overrides = opts.calibration_overrides.clone();
    report.card_values_client = own_client_card_values(f, &opts.calibration_overrides);
    if let Some(v) = &report.card_values_client {
        report.notes.push(format!("card values of client {v}, the fixture's: {CARD_VALUES_KEY} ran at arm none"));
    }
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
    let mut deploys_by_tick: BTreeMap<u32, Vec<(usize, &Deploy)>> = BTreeMap::new();
    // the rows resolved on an earlier board than their issue tick's, by that board's tick (`resolve_tick`), and
    // the point each was resolved to, by its index in `f.deploys`
    let mut resolve_on: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    let mut resolved: BTreeMap<usize, (u32, Vec2)> = BTreeMap::new();
    for (i, d) in f.deploys.iter().enumerate().filter(|(_, d)| cut.map_or(true, |c| d.tick < c)) {
        deploys_by_tick.entry(d.tick).or_default().push((i, d));
        if let Some(t) = resolve_tick(d) {
            resolve_on.entry(t).or_default().push(i);
        }
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
            // A spawner's emission whose unit is a playable card's own record (the Furnace's Fire Spirits: its interval
            // spawner puts down the card FireSpirits' unit) is rooted through its spawner, as a summon-only emission is
            // below. Rooted as "deployed" it was a FireSpirits deploy nobody played, and the recording's spirits (card
            // id the Furnace's) had no counterpart. Only an emission carries `spawned_by`; a deployed unit never does.
            let emitted_by = e.spawned_by.filter(|_| !card.summon_only).and_then(|o| sim_index_of.get(&(o.index, o.generation)).copied());
            let (root, how): (String, &'static str) = if matches!(e.kind, EntityKind::KingTower | EntityKind::PrincessTower) {
                let ids = s.tower_ids(e.team);
                tower_slot = ids.iter().position(|t| *t == Some(e.id));
                (e.card.to_string(), "tower")
            } else if let Some(heroes) = roots.ability_of.get(&e.card_idx).filter(|_| {
                // A SKELETON KING'S SOUL (state.rs `soul_pass`) is a copy (1 hitpoint) of the unit his button puts
                // down, and the recording names it by the King's card, as any unit a button puts down: it roots to its
                // hero, as an uncopied one does below. Rooted "Clone" it had no counterpart: 6 to 9 souls a side were
                // unmatched in each of sp-sk-souls-* and sp-champ-SkeletonKing-*. A copy the Clone spell made of such a
                // unit (its side cast a Clone within SPELL_RELEASE_LOOKBACK) stays "Clone".
                // PLANT replay_roots_a_soul_as_clone: a soul roots as "Clone", as any copy.
                e.cloned
                    && cfg!(not(clash_plant = "replay_roots_a_soul_as_clone"))
                    && !spell_casts.iter().any(|(t, team, c)| {
                        *team == e.team
                            && tick.saturating_sub(*t) <= SPELL_RELEASE_LOOKBACK
                            && matches!(db.get(*c).spell.as_ref().map(|sp| &sp.shape), Some(royalesim::card::SpellShape::Clone { .. }))
                    })
            }) {
                (db.get(heroes[0]).name.clone(), "ability")
            } else if e.cloned && cfg!(not(clash_plant = "replay_roots_a_copy_as_its_unit")) {
                // A CLONE'S COPY (state.rs `make_copy`; a copy's death spawn is one too, spells.CLONE_DEATH_SPAWNS) is rooted
                // to the Clone card. The recording names every copy by the Clone card's id (28000013), whatever unit it
                // copies, so the maker labels it "Clone", and rooted as its unit it had no counterpart: sweep-Clone's
                // copied Knight, sp-h8's copied hero Musketeer and sp-m5-clone's three copies were each an unmatched pair
                // (a truth "Clone" and a sim Knight or Musketeer), and none of their rows was scored.
                // PLANT replay_roots_a_copy_as_its_unit: a copy roots as the unit it copies.
                ("Clone".to_string(), "clone")
            } else if let Some(k) = emitted_by {
                #[cfg(not(clash_plant = "replay_roots_an_emitted_card_as_deployed"))]
                let root = (sim[k].root.clone(), "spawner");
                // PLANT (regression): an emission whose unit is a card's own record roots as that card's deploy.
                #[cfg(clash_plant = "replay_roots_an_emitted_card_as_deployed")]
                let root = {
                    let _ = k;
                    (e.card.to_string(), "deployed")
                };
                root
            } else if let Some(cidx) = roots.deploy_area_of.get(&e.card_idx).and_then(|parents| {
                // a unit a deploy spawn area puts down (the Tri Wizards' Electro Wizard, a playable card of its own): the
                // card deployed for this team within the lookback whose area puts it down, unless its own card was
                // deployed for this team on this very tick (then it is that play)
                if deploys_issued.iter().any(|(t, team, c)| *t == tick && *team == e.team && *c == e.card_idx) {
                    return None;
                }
                deploys_issued.iter().rev().find(|(t, team, c)| *t <= tick && tick - *t <= DEPLOY_AREA_LOOKBACK && *team == e.team && parents.contains(c)).map(|(_, _, c)| *c)
            }) {
                (db.get(cidx).name.clone(), "deploy-area")
            } else if !card.summon_only {
                (e.card.to_string(), "deployed")
            } else if let Some(base) = card.form_of {
                // a hero form loads summon-only; it is the card the row put down
                (db.get(base).name.clone(), "deployed")
            } else if let Some(heroes) = roots.ability_of.get(&e.card_idx) {
                (db.get(heroes[0]).name.clone(), "ability")
            } else if let Some(forms) = roots.evo_unit_of.get(&e.card_idx).filter(|forms| {
                // Only while a form of this team is on the field or has just died: the Evo Mortar's shot's unit is the
                // plain Goblin record, and a Goblin Drill's or a Goblin Barrel's Goblin is no Mortar's.
                // PLANT replay_roots_evo_unit_without_its_form (regression): every unit a form names roots to the form.
                cfg!(clash_plant = "replay_roots_evo_unit_without_its_form")
                    || s.entities().any(|o| o.team == e.team && forms.contains(&o.card_idx))
                    || recent_deaths.iter().any(|(t, team, c, _)| *team == e.team && forms.contains(c) && tick.saturating_sub(*t) <= DEATH_SPAWN_LOOKBACK)
            }) {
                // the Evo Royal Ghost's pair: the truth names each by the form's card (its base, `base_of_form`)
                (db.get(forms[0]).name.clone(), "evo-unit")
            } else if let Some(mount) = e.attached_to {
                // an attached rider: its mount's root (the mount is registered first, its team_seq
                // being the rider's minus one)
                match sim_index_of.get(&(mount.index, mount.generation)) {
                    Some(&k) => (sim[k].root.clone(), "rider"),
                    None => (e.card.to_string(), "rider-unknown"),
                }
            } else if let Some(cidx) = roots.second_summon_of.get(&e.card_idx).and_then(|parents| {
                // a second summon: the card the harness deployed for this team on this
                // very tick (its members exist from the same Spawn phase)
                deploys_issued.iter().rev().find(|(t, team, c)| *t == tick && *team == e.team && parents.contains(c)).map(|(_, _, c)| *c)
            }) {
                (db.get(cidx).name.clone(), "second-summon")
            } else if let Some(root) = roots.buff_root(e.card_idx) {
                // a buff's death spawn: named as the recording names it (CARDLESS_ROOT)
                (root.to_string(), "buff-death-spawn")
            } else if let Some(owner) = e.spawned_by {
                match sim_index_of.get(&(owner.index, owner.generation)) {
                    Some(&k) => (sim[k].root.clone(), "spawner"),
                    None => (e.card.to_string(), "spawner-unknown"),
                }
            } else {
                // a death spawn: the nearest recent death of a card that death-spawns this unit (a scheduled death
                // area's unit looked for over the area's schedule, `Roots::death_parents`)
                let parents = roots.death_parents(e.card_idx);
                let mut best: Option<(i64, u16)> = None;
                if let Some(parents) = parents {
                    for (t, team, cidx, pos) in recent_deaths.iter().rev() {
                        // each parent over its own lookback, the longer where a card is in both maps
                        let lookback = parents.iter().filter(|(p, _)| p == cidx).map(|(_, l)| *l).max();
                        if *team != e.team || lookback.map_or(true, |l| tick.saturating_sub(*t) > l) {
                            continue;
                        }
                        let d2 = pos.dist2(e.pos);
                        if best.map_or(true, |b| d2 < b.0) {
                            best = Some((d2, *cidx));
                        }
                    }
                }
                if let Some((_, cidx)) = best {
                    // a parent that is itself a unit of a chain (the Drill's building) roots to its card
                    (db.get(roots.card(cidx)).name.clone(), "death-spawn")
                } else {
                    let spells = roots.spell_release_of.get(&e.card_idx);
                    let cast = spells.and_then(|sp| spell_casts.iter().rev().find(|(t, team, cidx)| *team == e.team && tick.saturating_sub(*t) <= SPELL_RELEASE_LOOKBACK && sp.contains(cidx)));
                    match cast {
                        Some((_, _, cidx)) => {
                            // A form's decoy (card.rs `EvoDef::mirror`, the Evo Goblin Barrel's) roots to that form, whose
                            // base the recording names; the decoy card is in no form table, so `base_of_form` keeps it.
                            #[cfg(not(clash_plant = "replay_decoy_unrooted"))]
                            let c = db.cards.iter().position(|c| c.evo.as_ref().and_then(|v| v.mirror) == Some(*cidx)).map_or(*cidx, |f| f as u16);
                            #[cfg(clash_plant = "replay_decoy_unrooted")]
                            let c = *cidx;
                            (db.get(c).name.clone(), "spell-release")
                        }
                        None => (e.card.to_string(), "unrooted"),
                    }
                }
            };
            let root = base_of_form(db, &root);
            sim_index_of.insert((e.id.index, e.id.generation), sim.len());
            sim.push(SimEntity { id: e.id, team: e.team, card: e.card.to_string(), root, root_how: how, first_tick: tick, first_pos: e.pos, first_hp: e.hp, team_seq: e.team_seq, tower_slot });
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
                            facing: e.facing,
                            avoid: e.avoid_offset,
                            attack: [e.attack_ms, e.attack_load_ms],
                            route: match (e.route.last(), e.route.first()) {
                                (Some(&n), Some(&g)) => {
                                    let (n, g) = (to_native(n), to_native(g));
                                    [n.0, n.1, g.0, g.1, e.route.len() as i32]
                                }
                                _ => [0; 5],
                            },
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
        // rows tapped on this tick are resolved on this board (`resolve_tick`); nothing is issued here, so the
        // order against the issues below does not matter (spawn_unit only queues)
        if let Some(ix) = resolve_on.get(&tick) {
            for &i in ix {
                if let Some(p) = resolve_on_board(&s, db, &f.deploys[i]) {
                    resolved.insert(i, (tick, p));
                }
            }
        }
        if let Some(list) = deploys_by_tick.get(&due) {
            for (i, d) in list {
                let team = team_of(d.side);
                if d.kind == KIND_ABILITY {
                    let base = d.card.as_deref().and_then(|c| db.index(c));
                    let k = s.ability_buttons(team).iter().position(|b| Some(b.base) == base);
                    let r = match k {
                        Some(k) => s.press_ability_button(team, k).map(|_| ()),
                        None => Err(DeployError::NoHero),
                    };
                    let card = format!("{} ability", d.card.as_deref().unwrap_or("?"));
                    report.deploys.push(DeployIssue { tick: d.tick, side: d.side, card, issued_at: s.tick_count(), resolved_on: None, result: r.map_err(|e| format!("{e:?}")) });
                    continue;
                }
                let name = deploy_play(d, db);
                let (resolved_on, pos) = match resolved.get(i) {
                    Some(&(t, p)) => (Some(t), p),
                    None => {
                        let p = scenario_troop_tap(&s, db, d).unwrap_or_else(|| play_point(d));
                        (None, from_native(p[0], p[1]))
                    }
                };
                // A CORPUS TROOP row goes down as seen (`observed_row`, `spawn_unit_resolved`), so placement.TAP_SNAP
                // does not snap a point the client already resolved; a SCENARIO BUILDING row from its tap, resolved as
                // a play's (`scenario_building_tap`, `spawn_unit_tapped`); spell rows and other scenario rows through
                // `spawn_unit`.
                let r = issue_row(&mut s, db, d, team, &name, pos);
                if r.is_ok() {
                    if let Some(idx) = db.index(&name) {
                        if db.get(idx).kind == CardKind::Spell {
                            spell_casts.push((tick + 1, team, idx));
                            // THE EVO GOBLIN BARREL'S DECOY (card.rs `EvoDef::mirror`): the engine casts it with the form,
                            // so its GoblinDummies root to it (its base the Goblin Barrel, `base_of_form`), as the
                            // recording names them. Unrecorded, they rooted as themselves and paired with nothing.
                            // PLANT replay_decoy_unrooted.
                            #[cfg(not(clash_plant = "replay_decoy_unrooted"))]
                            if let Some(m) = db.get(idx).evo.as_ref().and_then(|v| v.mirror) {
                                spell_casts.push((tick + 1, team, m));
                            }
                        } else {
                            deploys_issued.push((tick + 1, team, idx));
                        }
                    }
                }
                report.deploys.push(DeployIssue { tick: d.tick, side: d.side, card: name, issued_at: s.tick_count(), resolved_on, result: r.map_err(|e| format!("{e:?}")) });
            }
        }
        let alive_before: Vec<(EntityId, Team, u16, Vec2)> = s.entities().map(|e| (e.id, e.team, e.card_idx, e.pos)).collect();
        let containers_before = containers_waiting(&s);
        // THE CLIENT'S GENERATOR (`RngColumn`): this tick starts from the state the client's had after the frame before,
        // so the engine's draws from it (state.rs `client_rnd`) are the client's.
        if let Some(st) = f.rng.as_ref().and_then(|r| r.state_after(tick)) {
            s.scenario_set_client_rng(st);
        }
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
        // A CONTAINER (a death bomb that carries a death spawn: the Skeleton Barrel's) releases its units when its fuse
        // ends, ticks after the death that left it. Its release is recorded as a death of the container at its point,
        // so its units root through it to the card at the top of the chain (`Roots::card`).
        let waiting_now = containers_waiting(&s);
        for c in containers_before {
            if !waiting_now.contains(&c) {
                recent_deaths.push((tick, c.0, c.1, c.2));
            }
        }
        // Kept as long as the longest lookback reads them (`Roots::death_parents`: a scheduled death area's units come up
        // to SCHEDULED_DEATH_LOOKBACK after the death). Pruned at DEATH_SPAWN_LOOKBACK, the Suspicious Bush's goblins,
        // 13 and 14 ticks after the bush's death, found no parent and were rooted to nothing (sweep-SuspiciousBush).
        #[cfg(not(clash_plant = "replay_recent_deaths_pruned_short"))]
        let kept = DEATH_SPAWN_LOOKBACK.max(SCHEDULED_DEATH_LOOKBACK);
        // PLANT (regression): the deaths are pruned at the death spawn's lookback.
        #[cfg(clash_plant = "replay_recent_deaths_pruned_short")]
        let kept = DEATH_SPAWN_LOOKBACK;
        recent_deaths.retain(|(t, ..)| tick - *t <= kept);
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
        // An entity with no hitpoints at all (max_hp below 0; 0 is "not in this source"): the Hero Tombstone's visual
        // dummy, NO_DAMAGE, UNTARGETABLE and NO_CHECKCOLLISIONS, a controller the engine runs as code. It is paired
        // with nothing, as an unknown object is: paired, it took the pair of its form's first Skeleton and shifted
        // every later one (sp-form-Tombstone-hero-s0).
        #[cfg(not(clash_plant = "replay_pairs_a_dummy"))]
        if e.max_hp < 0 {
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
        let th: Vec<Vec<Option<i32>>> = truth_groups_of.iter().map(|(_, v)| v.iter().map(|&tk| truth.row(tk, truth.first_index(tk)).map(|r| r.hp)).collect()).collect();
        let sh: Vec<Vec<i32>> = sim_pools.iter().map(|(_, v)| v.iter().map(|&sk| sim[sk].first_hp).collect()).collect();
        for (gi, mi, pi, si) in pair_groups_hp(&tg, &sp, &th, &sh, PAIR_WINDOW_TICKS) {
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
        if !truth_to_sim.contains_key(&k) && !dummies.contains(&k) {
            report.unmatched_truth.push((e.key, e.card.clone().unwrap_or_default()));
        }
    }
    for (k, e) in sim.iter().enumerate() {
        if !matched_sim.contains(&k) {
            report.unmatched_sim.push((e.id.index, e.id.generation, e.root.clone()));
        }
    }

    // -- scoring
    report.score_until = report.engine_end_tick.filter(|&e| towers_down_agree(f, &truth, &s, e));
    let families_of = |card: &str| register.get(card).cloned().unwrap_or_default();
    let mut per_card: BTreeMap<String, Score> = BTreeMap::new();
    let mut total = Score::default();
    let mut divergence: Option<Divergence> = None;
    let mut pair_state: BTreeMap<usize, PairState> = BTreeMap::new();
    // unmatched truth / sim entities: (first tick, frames) of their alive run
    let mut missing_run: BTreeMap<usize, (u32, u32)> = BTreeMap::new();
    let mut extra_run: BTreeMap<usize, (u32, u32)> = BTreeMap::new();
    let is_tower_key = |key: i64| f.towers.iter().any(|t| truth_tower_key(f, &truth, t) == Some(key));
    // ONE TICK, SCORED ONCE (`Report::repeated_frames`): a frame repeating the tick just scored is skipped.
    let mut scored_tick: Option<u32> = None;
    for (fi, &t) in truth.ticks.iter().enumerate().step_by(opts.stride.max(1)) {
        // PLANT (regression) replay_scores_repeated_frames: every frame is scored, a repeated tick as often as it repeats.
        #[cfg(not(clash_plant = "replay_scores_repeated_frames"))]
        if scored_tick == Some(t) {
            report.repeated_frames += 1;
            continue;
        }
        scored_tick = Some(t);
        let Some(snap) = snaps.get(&t) else { continue };
        if report.score_until.is_some_and(|u| t > u) {
            break;
        }
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
                // -3: the truth frame recorded no target. Not -1 (no target) and not the
                // sim column's -2 (a target the pairing never matched).
                let tr = truth_row.map(|r| [r.x as i64, r.y as i64, r.hp as i64, r.state as i64, r.path_n as i64, r.target.unwrap_or(TRUTH_TARGET_UNRECORDED)]);
                let sr = sim_row.map(|r| {
                    let (x, y) = to_native(r.pos);
                    // THE SIM'S TARGET IN THE TRUTH'S KEY SPACE. This used to emit the sim
                    // REGISTRY INDEX beside the truth's KEY, in the same column of the same
                    // row, so a reader comparing them positionally was comparing two id
                    // spaces -- and the parity session hit it: both archers read 4 while
                    // their positions clearly headed for a tower whose truth key is 5.
                    // -1 is "no target" and -2 is "targeting an entity the pairing never
                    // matched", which is NOT the same statement and must not collapse into
                    // it, by the same rule this struct already states for push.
                    let tg = match r.target {
                        None => -1,
                        Some(id) => sim_index_of
                            .get(&(id.index, id.generation))
                            .and_then(|sk| sim_to_truth.get(sk))
                            .map(|ti| truth.entities[*ti].key)
                            .unwrap_or(-2),
                    };
                    [x as i64, y as i64, r.hp as i64, r.attacking as i64, r.path_n as i64, tg]
                });
                let dist = match (truth_row, sim_row) {
                    (Some(a), Some(b)) => Some(native_dist(b.pos, (a.x, a.y))),
                    _ => None,
                };
                let push = sim_row.map(|r| [r.push.x as i64, r.push.y as i64, r.push_neighbours as i64]);
                let radius = sim_row.map(|r| r.radius);
                let heading = sim_row.map(|r| [r.facing.x as i64, r.facing.y as i64, r.avoid as i64]);
                let attack = sim_row.map(|r| [r.attack[0] as i64, r.attack[1] as i64]);
                let route = sim_row.map(|r| r.route.map(i64::from));
                report.trace.push(TraceRow { tick: t, key: e.key, card: root.clone(), truth: tr, sim: sr, push, radius, dist, heading, attack, route });
            }
            match (truth_row, sim_row) {
                (Some(tr), Some(sr)) => {
                    sc.unit_ticks += 1;
                    sc.both_alive += 1;
                    let d = native_dist(sr.pos, (tr.x, tr.y));
                    let walking = tr.state == 1 && matches!(tr.target, Some(t) if t < 0 || is_tower_key(t)) && tr.hp == e.max_hp;
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
                    match tr.target {
                        None => sc.target_unrecorded += 1,
                        Some(tt) => {
                            let truth_target_sim: Option<Option<EntityId>> = if tt < 0 {
                                Some(None)
                            } else {
                                truth.key_to_entity.get(&tt).and_then(|tk2| truth_to_sim.get(tk2)).map(|sk2| Some(sim[*sk2].id))
                            };
                            if truth_target_sim == Some(sr.target) {
                                sc.target_match += 1;
                            }
                        }
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
        // unmatched truth entities alive on this frame (a dummy, `unscored_dummies`, is not scored)
        for (tk, e) in truth.entities.iter().enumerate() {
            if truth_to_sim.contains_key(&tk) || truth.row(tk, fi).is_none() || dummies.contains(&tk) {
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
    /// The heading and the avoidance offset after the tick (`EntityView::facing`, `avoid_offset`).
    facing: Vec2,
    avoid: i32,
    /// The attack progress and the load timer after the tick (`EntityView::attack_ms`, `attack_load_ms`).
    attack: [i32; 2],
    /// The next waypoint, the goal-most node (native) and the node count after the tick (`EntityView::route`).
    route: [i32; 5],
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

/// DID THE CLIENT'S BATTLE END WHERE THE ENGINE'S DID? A capture runs on for 50-90 frames after its battle is over (the
/// King Tower still shown at 0 hp, units still animating), while the engine stops ticking at its end, so those frames
/// score as misses that no rule of the game made (the 16.402 corpus: 144043-A/B, 003751-A/B and 090825-B first
/// "diverge" on their last tick). A battle ends on its crown towers (a King Tower down, or regulation or overtime with
/// the crowns apart), so the client's is over on the engine's end tick when the same crown towers are down: in the
/// engine's final state, and on the truth's first frame at or after `end` (a tower shown at 0 hp is down). When they
/// differ, the two ends differ and nothing is cut: an engine that takes a King Tower early is scored for it.
pub fn towers_down_agree(f: &Fixture, truth: &TruthTable, s: &BattleState, end: u32) -> bool {
    let Some(fi) = truth.ticks.iter().position(|&t| t >= end) else {
        return false;
    };
    f.towers.iter().all(|t| {
        let engine_up = s.tower_ids(team_of(t.side)).get(t.slot).copied().flatten().and_then(|id| s.entity(id)).is_some_and(|e| e.hp > 0);
        let truth_up = truth_tower_key(f, truth, t)
            .and_then(|k| truth.key_to_entity.get(&k).copied())
            .and_then(|k| truth.row(k, fi))
            .is_some_and(|r| r.hp > 0);
        engine_up == truth_up
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
    assign_tied(truth, sim, &|_, _| 0)
}

/// As `assign`, with a TIE cost per pair (`tie(truth index, sim index)`) that decides only between assignments of equal
/// total distance. Members created on one point (a Ram and its Rider) are otherwise paired by order: 20260920-003751's
/// Ram (1766 hp) and Rider (593) were paired crosswise, and neither's hp matched on any tick. Nothing that distance
/// decides moves.
pub fn assign_tied(truth: &[Option<Vec2>], sim: &[Vec2], tie: &dyn Fn(usize, usize) -> i64) -> Vec<(usize, usize)> {
    let n = truth.len().min(sim.len());
    if n == 0 {
        return Vec::new();
    }
    let d = |gi: usize, si: usize| -> i64 { truth[gi].map_or(i64::MAX / 4, |p| p.dist2(sim[si])) };
    if truth.len() <= EXACT_ASSIGNMENT_MAX && sim.len() <= EXACT_ASSIGNMENT_MAX {
        // permutations of the larger side's indices, take the first n
        let (big, small, truth_big) = if truth.len() >= sim.len() { (truth.len(), sim.len(), true) } else { (sim.len(), truth.len(), false) };
        let mut perm: Vec<usize> = (0..big).collect();
        let mut best: Option<(i64, i64, Vec<usize>)> = None;
        permute(&mut perm, 0, &mut |p| {
            let mut total = 0i64;
            let mut ties = 0i64;
            for (k, &pk) in p.iter().enumerate().take(small) {
                let (gi, si) = if truth_big { (pk, k) } else { (k, pk) };
                total = total.saturating_add(d(gi, si));
                ties += tie(gi, si);
            }
            if best.as_ref().map_or(true, |b| (total, ties) < (b.0, b.1)) {
                best = Some((total, ties, p[..small].to_vec()));
            }
        });
        let (_, _, p) = best.expect("at least one permutation");
        return (0..small).map(|k| if truth_big { (p[k], k) } else { (k, p[k]) }).collect();
    }
    let mut cands: Vec<(i64, i64, usize, usize)> = Vec::new();
    for gi in 0..truth.len() {
        for si in 0..sim.len() {
            cands.push((d(gi, si), tie(gi, si), gi, si));
        }
    }
    cands.sort();
    let mut used_t = BTreeSet::new();
    let mut used_s = BTreeSet::new();
    let mut out = Vec::new();
    for (_, _, gi, si) in cands {
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
    pair_groups_hp(truth, sim, &[], &[], window)
}

/// As `pair_groups`, with each member's hp on its first frame (`truth_hp[group][member]`, None when absent) and
/// `sim_hp[pool][member]`: members are assigned by `assign_tied`, so where distance ties (members on one point) the
/// pairs whose first hp agree win.
pub fn pair_groups_hp(
    truth: &[(u32, Vec<Option<Vec2>>)],
    sim: &[(u32, Vec<Vec2>)],
    truth_hp: &[Vec<Option<i32>>],
    sim_hp: &[Vec<i32>],
    window: u32,
) -> Vec<(usize, usize, usize, usize)> {
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
        let sim_hps: Vec<Option<i32>> = taken.iter().map(|&(pi, si)| sim_hp.get(pi).and_then(|v| v.get(si)).copied()).collect();
        let tie = |mi: usize, k: usize| -> i64 {
            match (truth_hp.get(gi).and_then(|v| v.get(mi)).copied().flatten(), sim_hps[k]) {
                (Some(a), Some(b)) if a != b => 1,
                _ => 0,
            }
        };
        for (mi, k) in assign_tied(pts, &sim_pts, &tie) {
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
    if r.repeated_frames > 0 {
        out.push_str(&format!(
            "NOT SCORED AGAIN: {} truth frames repeat the tick before them (the client's frozen frames after the match ends); each tick is scored once\n\n",
            r.repeated_frames
        ));
    }
    if !r.unscored_dummies.is_empty() {
        let keys: Vec<String> = r.unscored_dummies.iter().map(|(k, c)| format!("{k} ({c})")).collect();
        out.push_str(&format!(
            "NOT SCORED: {} truth entities with no hitpoints that nothing targets (the engine carries none): {}\n\n",
            r.unscored_dummies.len(),
            keys.join(", ")
        ));
    }
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
        match r.score_until {
            Some(u) => out.push_str(&format!("- engine end at tick {e}: the client's battle is over too (the same crown towers down); scored through tick {u}\n")),
            None => out.push_str(&format!("- engine end at tick {e}: {} unit-ticks scored after it against a frozen engine state\n", r.score.after_engine_end)),
        }
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

/// REFUSE A PARITY MEASUREMENT TAKEN WITH A BINARY BUILT AGAINST A DIFFERENT LEDGER.
///
/// The compiled extension already does this: `RustEngine()` raises rather than run against
/// a calibration.json it was not built with, and that refusal is what tells every session
/// the ledger moved. THIS BINARY HAD NO SUCH CHECK, and it is the one that produces the
/// project's headline accuracy figure -- so every gate in the repo could be green,
/// `test_ledger_build_step` included, while a parity number was measured on an engine the
/// ledger no longer describes, and the number would look entirely normal.
///
/// It is not hypothetical. On 2026-09-23 the parity session was caught by it twice in one
/// session: once seeing a figure that had not moved when a change predicted it would, and
/// once seeing 27.3% where 21.7% was expected after a revert. Both times the tell was a
/// human recognising a number, not an instrument. A figure drifting to something
/// unfamiliar rather than to a remembered value would have been reported.
///
/// VALUES, NOT BYTES, and REFUSE rather than WARN. Values, because reworded prose must not
/// block a measurement -- the same rule `stale_build_differences` uses. Refuse, because a
/// parity figure from a stale engine is not a slightly wrong measurement of this engine,
/// it is an accurate measurement of a DIFFERENT one, and there is no number to salvage.
pub fn refuse_if_stale(root: &str) -> Result<(), String> {
    let path = format!("{root}/data/calibration.json");
    let disk = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let values = |src: &str, what: &str| -> Result<BTreeMap<String, serde_json::Value>, String> {
        let doc: serde_json::Value =
            serde_json::from_str(src).map_err(|e| format!("{what} calibration.json: {e}"))?;
        let mut out = BTreeMap::new();
        if let Some(sections) = doc.as_object() {
            for (section, entries) in sections {
                if let Some(entries) = entries.as_object() {
                    for (key, entry) in entries {
                        if let Some(v) = entry.get("value") {
                            out.insert(format!("{section}.{key}"), v.clone());
                        }
                    }
                }
            }
        }
        Ok(out)
    };
    let built = values(royalesim::py::EMBEDDED_CALIBRATION_JSON, "compiled-in")?;
    let now = values(&disk, "on-disk")?;
    let mut drifted: Vec<String> = Vec::new();
    for key in built.keys().chain(now.keys()).collect::<BTreeSet<_>>() {
        let (b, n) = (built.get(key), now.get(key));
        if b != n {
            let show = |v: Option<&serde_json::Value>| {
                v.map_or("<absent>".to_string(), |v| v.to_string())
            };
            drifted.push(format!("  {key}: built {}, now {}", show(b), show(n)));
        }
    }
    if drifted.is_empty() {
        return Ok(());
    }
    Err(format!(
        "REFUSING TO MEASURE: this binary was built against a different calibration.json, so any figure it produced would describe an engine the ledger no longer names.
{}

Rebuild it, from crates/royalesim:
  cargo build --release --example replay_parity

Note that `maturin develop` does NOT rebuild this binary: it builds the wheel, and the two are separate artefacts from the same source.",
        drifted.join("
")
    ))
}
