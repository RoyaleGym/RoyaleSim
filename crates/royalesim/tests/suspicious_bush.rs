//! THE SUSPICIOUS BUSH: a kamikaze invisible for its whole life (card.rs `convert`, the blank idle time read as 0;
//! target.rs `invisible`), whose death area puts two goblins down on a schedule (card.rs `scheduled_area`; spell.rs
//! `step_spells`; state.rs `scheduled_point`; calibration actions.SUB_TICK_DELAY_ROUNDING, spawner.RELATIVE_SPAWN_OFFSET,
//! spells.AREA_DAMAGE_WITHOUT_HIT_FLAGS).
//!
//! THE ROWS (the 15.535.29 tables): SuspiciousBush is a Kamikaze with no KamikazeTime (Damage 0, Range 1600,
//! TargetOnlyBuildings) whose idle buff is an invisibility with a blank BuffWhenNotAttackingTime. Its death area
//! SuspiciousBush_DummyAEO (LifeDuration 1000, Damage 100, no hit flag, no Radius) runs a group of two
//! ActionSpawnToLocation entries of BushGoblin with UseDeploy and no DeployTime: RelativeX -1 at 675 ms and RelativeX +1
//! at 625 ms.
//!
//! THE LAW, measured on client 15.535.29 (12 bushes; D the first tick the bush is absent, P where it died):
//!   - nothing targets the bush from its first frame (a Knight in its place is targeted);
//!   - it dies on the tick after the first one whose post-move centre distance to its tower is at most 3100, and the
//!     tower loses nothing;
//!   - goblin A on D + 12 at P + (-500, 0) for side 0 and P + (+500, 0) for side 1, goblin B on D + 13 on the other
//!     side (actions.SUB_TICK_DELAY_ROUNDING = floor_from_creation; spawner.RELATIVE_SPAWN_OFFSET =
//!     half_tile_owner_left);
//!   - each goblin 337 hp at level 11, deploying its own 1000 ms, first targeted by an enemy on its 8th frame;
//!   - the death area's Damage 100 hurts nobody (spells.AREA_DAMAGE_WITHOUT_HIT_FLAGS = inert).
//!
//! WHAT IS PINNED, each with its precondition (level 11):
//!   1. the loader reads the bush (idle invisibility 0, kamikaze, the scheduled death area), and refuses a blank idle
//!      time on a unit that survives its hit;
//!   2. a red Musketeer in reach never targets a Blue bush while it deploys and walks, and targets a Knight played in
//!      its place;
//!   3. a side-0 bush walking at a red princess tower dies on the tick after it comes within 3100, the tower loses
//!      nothing, and goblins A and B come on D + 12 and D + 13 at P + (-500, 0) and P + (+500, 0), 337 hp, deploying
//!      1000 ms, the tower first targeting A on its 8th frame;
//!   4. the offsets turn with the owner (a side-1 bush) under half_tile_owner_left, and tiles_owner_frame puts them
//!      1000 out on the other sides;
//!   5. a bush a Zap kills mid-walk puts its goblins down on the same ticks, about the point it stepped to on its death
//!      tick; a red Knight beside it takes nothing from the death area.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test suspicious_bush`):
//!   * `bush_dummy_damage_loaded` -- the death area's Damage read as damage it deals, so the area and the bush are
//!     refused: (1) to (5) go red.
//!   * `invisible_targetable` (target.rs) -- invisibility not read: (2) goes red.
//!   * `sub_tick_delay_ceil` -- delays rounded up: (3), (4) and (5) go red (the goblins on D + 13 and D + 14).
//!   * `relative_offset_native` -- one native unit a unit of RelativeX: (3), (4) and (5) go red.
//!   * `schedule_cumulative` -- the delays read as gaps: (3), (4) and (5) go red (goblin A on D + 26).
//!   * `scheduled_acquire_delay_dropped` -- the goblins are targets from their first frame: (3) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardSource, SpawnOffset, SpellShape, UnitRef};
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, RelativeSpawnOffset, ScheduledUnitFirstUpdate, SubTickDelayRounding};
use royalesim::{EntityId, Team};

/// The level of the measured bushes.
const LEVEL: i32 = 11;

/// The goblin row the bush's death area puts down.
const GOBLIN: &str = "BushGoblin";

/// The bush's reach to a princess tower, native: Range 1600 + its radius 500 + the tower's 1000.
const REACH: i64 = 3100;

/// The shipped config at level 11, asserting the arms this file pins.
fn shipped() -> BattleConfig {
    let mut c = config();
    c.card_level = [LEVEL, LEVEL];
    c.tower_level = [LEVEL, LEVEL];
    // The goblins' points were pinned with each unit standing on its slot on its first frame (spawner.
    // SCHEDULED_UNIT_FIRST_UPDATE = next_tick); the shipped first update on the creation tick pushes the ones that
    // are born overlapping a body.
    c.calib.scheduled_unit_first_update = ScheduledUnitFirstUpdate::NextTick;
    assert_eq!(c.calib.sub_tick_delay_rounding, SubTickDelayRounding::FloorFromCreation, "the shipped actions.SUB_TICK_DELAY_ROUNDING");
    assert_eq!(c.calib.relative_spawn_offset, RelativeSpawnOffset::HalfTileOwnerLeft, "the shipped spawner.RELATIVE_SPAWN_OFFSET");
    c
}

/// A native point as engine subtiles.
fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The native distance between two engine points.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// `team`'s princess tower on the engine's left (x below the centre line) or right.
fn tower(s: &BattleState, team: Team, left: bool) -> (EntityId, Vec2) {
    let w = s.arena().width / 2;
    s.entities().find(|v| v.team == team && v.card == "PrincessTower" && (v.pos.x < w) == left).map(|v| (v.id, v.pos)).expect("a princess tower")
}

// ---------------------------------------------------------------------------
// (1)

/// Plant: bush_dummy_damage_loaded.
#[test]
fn the_loader_reads_the_bush_as_an_invisible_kamikaze_with_a_scheduled_death_area() {
    let s = BattleState::new(0, shipped());
    let db = s.cards();
    let b = db.index("SuspiciousBush").unwrap_or_else(|| panic!("SuspiciousBush refused: {:?}", db.rejected.iter().find(|(n, _)| n == "SuspiciousBush")));
    let c = db.get(b);
    assert_eq!(c.invisible_when_idle, Some(0), "a blank idle time on a kamikaze: invisible for its whole life");
    assert!(c.kamikaze, "Kamikaze with no KamikazeTime");
    let area = c.death_area_effect.as_ref().expect("the bush's death area");
    let SpellShape::ScheduledArea { life_ms, schedule } = &area.shape else { panic!("the death area: {:?}", area.shape) };
    assert_eq!(*life_ms, 1000, "LifeDuration");
    let goblin = db.index(GOBLIN).expect("the goblin row loads");
    let got: Vec<(i32, SpawnOffset, Option<i32>, u16)> = schedule.iter().map(|e| (e.delay_ms, e.offset, e.deploy_time_ms, e.unit)).collect();
    assert_eq!(got, vec![(675, SpawnOffset::Relative { x: -1, y: 0 }, None, goblin), (625, SpawnOffset::Relative { x: 1, y: 0 }, None, goblin)], "the two entries");
    let g = db.get(goblin);
    assert!(g.summon_only && g.unit_name == GOBLIN, "the goblin is a summon-only record: {} / {}", g.name, g.unit_name);
    assert_eq!(db.scaled(goblin, LEVEL, g.hitpoints), Ok(337), "132 hp at level 11");
    assert_eq!(db.unit_refs(b), vec![(UnitRef::Scheduled(0), goblin, None), (UnitRef::Scheduled(1), goblin, None)], "unit_refs names both entries");
    // A blank idle time on a unit that survives its hit is refused: the same file with the bush's Kamikaze off.
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let mut doc: serde_json::Value = serde_json::from_str(&text).expect("cards.json parses");
    for row in doc["cards"].as_array_mut().expect("a cards array") {
        if row["name"] == "SuspiciousBush" {
            row["kamikaze"] = serde_json::json!(false);
        }
    }
    let db = CardDb::from_json_str(&doc.to_string(), CardSource::DerivedJson).expect("the doctored file parses");
    let why = db.rejected.iter().find(|(n, _)| n == "SuspiciousBush").map(|(_, w)| w.as_str());
    assert_eq!(why, Some("an invisibility with no BuffWhenNotAttackingTime on a unit that survives its hit"), "the bush with its Kamikaze off");
}

// ---------------------------------------------------------------------------
// (2)

/// A Blue `card` played at (9000, 13000) and a red Musketeer set up at (9000, 17500); per tick after the play, the
/// Musketeer's target, whether it is the played unit, and the native distance between the two.
fn musketeer_on(card: &str) -> Vec<(bool, i64)> {
    let mut s = BattleState::new(0, shipped());
    let m = s.scenario_spawn_now(Team::Red, "Musketeer", at((9000, 17500)), None).expect("the Musketeer");
    s.spawn_unit(Team::Blue, card, at((9000, 13000)), None).expect("the play");
    s.tick();
    let unit = s.entities().find(|e| e.team == Team::Blue && e.card == card).map(|e| e.id).expect("the played unit");
    let mut out = Vec::new();
    for _ in 0..40 {
        let (mv, uv) = (s.entity(m).expect("the Musketeer lives"), s.entity(unit).expect("the played unit lives"));
        out.push((mv.target == Some(unit), dist(mv.pos, uv.pos)));
        s.tick();
    }
    out
}

/// Plant: invisible_targetable.
#[test]
fn nothing_targets_the_bush_and_a_knight_in_its_place_is_targeted() {
    let reach = (card_stat(&BattleState::new(0, shipped()), "Musketeer").range / K + 1000) as i64;
    let bush = musketeer_on("SuspiciousBush");
    assert!(bush.iter().any(|(_, d)| *d <= reach), "vacuous: the bush never came within the Musketeer's reach: {bush:?}");
    assert!(bush.iter().all(|(t, _)| !t), "the Musketeer targeted the bush: {bush:?}");
    let knight = musketeer_on("Knight");
    assert!(knight.iter().any(|(t, _)| *t), "the Musketeer never targeted a Knight in the bush's place: {knight:?}");
}

// ---------------------------------------------------------------------------
// (3) and (4)

/// What a bush's death leaves: D (the tick its death area was created, the tick it is gone after), P (the area's
/// point), and each goblin as (ticks after D, native offset from P, id), in creation order, over D + 1 .. D + 14.
struct Death {
    s: BattleState,
    d0: u32,
    p: Vec2,
    goblins: Vec<(u32, (i32, i32), EntityId)>,
}

/// Tick `s` until bush `bush` is gone (at most `max` ticks): its death tick D. `each` sees the battle after every tick
/// the bush lives through.
fn until_gone(s: &mut BattleState, bush: EntityId, max: u32, mut each: impl FnMut(&BattleState)) -> u32 {
    for _ in 0..max {
        s.tick();
        if s.entity(bush).is_none() {
            return s.tick_count() - 1;
        }
        each(s);
    }
    panic!("the scene drifted: the bush lives after {max} ticks");
}

/// The bush's death area and the goblins it puts down, from a battle whose bush of `team` died on `d0`.
fn after_death(mut s: BattleState, team: Team, d0: u32) -> Death {
    let areas: Vec<(Vec2, u32)> = s
        .spells()
        .iter()
        .filter(|sp| sp.team == team)
        .filter_map(|sp| match sp.motion {
            SpellMotion::Scheduled { pos, born, .. } => Some((pos, born)),
            _ => None,
        })
        .collect();
    assert_eq!(areas.len(), 1, "one death area after the bush's death");
    let (p, born) = areas[0];
    assert_eq!(born, d0, "the area is created on the death tick");
    let mut goblins = Vec::new();
    let mut seen: Vec<EntityId> = Vec::new();
    for k in 1..=14u32 {
        s.tick();
        for e in s.entities().filter(|e| e.team == team && e.card == GOBLIN) {
            if !seen.contains(&e.id) {
                seen.push(e.id);
                goblins.push((k, ((e.pos.x - p.x) / K, (e.pos.y - p.y) / K), e.id));
            }
        }
    }
    Death { s, d0, p, goblins }
}

/// A bush of `team` set up 5000 in front of the enemy princess tower on the engine's left (side 0) or right (side 1),
/// walking at it; run until its goblins are down. Also returns the tower and, per tick the bush lived, the tick and
/// whether the bush ended it within REACH of the tower's centre (exact, in engine subtiles, as the reach test is).
fn walk_into_reach(cfg: BattleConfig, team: Team) -> (Death, EntityId, Vec<(u32, bool)>) {
    let mut s = BattleState::new(0, cfg);
    let (t, tp) = tower(&s, team.other(), team == Team::Blue);
    let dy = if team == Team::Blue { -5000 * K } else { 5000 * K };
    let bush = s.scenario_spawn_now(team, "SuspiciousBush", Vec2::new(tp.x, tp.y + dy), None).expect("the bush");
    let mut walk = Vec::new();
    let d0 = until_gone(&mut s, bush, 120, |s| {
        let (b, tv) = (s.entity(bush).unwrap(), s.entity(t).expect("the tower stands"));
        assert_ne!(tv.target, Some(bush), "the tower targeted the invisible bush");
        let r = REACH * K as i64;
        walk.push((s.tick_count() - 1, b.pos.dist2(tp) <= r * r));
    });
    (after_death(s, team, d0), t, walk)
}

/// Plants: sub_tick_delay_ceil, relative_offset_native, schedule_cumulative, scheduled_acquire_delay_dropped.
#[test]
fn a_bush_dies_in_reach_and_leaves_two_goblins_on_the_twelfth_and_thirteenth_ticks() {
    let (mut d, t, walk) = walk_into_reach(shipped(), Team::Blue);
    let l = walk.iter().find(|(_, inside)| *inside).map(|(k, _)| *k).unwrap_or_else(|| panic!("the bush never came within {REACH}: {walk:?}"));
    assert_eq!(d.d0, l + 1, "the bush dies on the tick after the first one it ends within {REACH} of the tower");
    let goblins: Vec<(u32, (i32, i32))> = d.goblins.iter().map(|(k, o, _)| (*k, *o)).collect();
    assert_eq!(goblins, vec![(12, (-500, 0)), (13, (500, 0))], "goblin A on D + 12 at P + (-500, 0), B on D + 13 at P + (500, 0)");
    // On D + 14: the tower has lost nothing (the bush's hit is 0, and the goblins still deploy).
    let tv = d.s.entity(t).expect("the tower stands");
    assert_eq!(tv.hp, tv.max_hp, "the tower lost hp to the bush");
    let a = d.goblins[0].2;
    let v = d.s.entity(a).expect("goblin A lives on D + 14");
    assert_eq!((v.max_hp, v.hp, v.team), (337, 337, Team::Blue), "goblin A at level 11, full, for the bush's side");
    assert!(v.deploying && v.deploy_ms == 900, "goblin A deploys its own 1000 ms, two ticks of it gone on D + 14: {} ms", v.deploy_ms);
    assert_eq!(v.acquirable_from, d.d0 + 12 + 7, "an enemy may target goblin A from its 8th frame");
    assert_ne!(tv.target, Some(a), "the tower took goblin A before its 8th frame");
    // The tower first targets goblin A on D + 19, its 8th frame (goblin B's is D + 20).
    let mut first = None;
    for k in 15..=22u32 {
        d.s.tick();
        if first.is_none() && d.s.entity(t).and_then(|v| v.target) == Some(a) {
            first = Some(k);
        }
    }
    assert_eq!(first, Some(19), "the tower first targets goblin A on D + 19");
}

/// Plants: sub_tick_delay_ceil, relative_offset_native, schedule_cumulative.
#[test]
fn the_goblins_turn_with_the_owner_and_tiles_owner_frame_puts_them_1000_out() {
    let offsets = |arm: RelativeSpawnOffset, team: Team| -> Vec<(u32, (i32, i32))> {
        let mut c = shipped();
        c.calib.relative_spawn_offset = arm;
        walk_into_reach(c, team).0.goblins.iter().map(|(k, o, _)| (*k, *o)).collect()
    };
    let half = RelativeSpawnOffset::HalfTileOwnerLeft;
    let tiles = RelativeSpawnOffset::TilesOwnerFrame;
    assert_eq!(offsets(half, Team::Blue), vec![(12, (-500, 0)), (13, (500, 0))], "half_tile_owner_left, side 0");
    assert_eq!(offsets(half, Team::Red), vec![(12, (500, 0)), (13, (-500, 0))], "half_tile_owner_left, side 1: the rotation");
    assert_eq!(offsets(tiles, Team::Blue), vec![(12, (1000, 0)), (13, (-1000, 0))], "tiles_owner_frame, side 0");
    assert_eq!(offsets(tiles, Team::Red), vec![(12, (-1000, 0)), (13, (1000, 0))], "tiles_owner_frame, side 1");
}

// ---------------------------------------------------------------------------
// (5)

/// Plants: sub_tick_delay_ceil, relative_offset_native, schedule_cumulative.
#[test]
fn a_bush_a_zap_kills_mid_walk_leaves_its_goblins_about_its_last_step_and_hurts_nobody() {
    // On the red half, out of every Blue tower's reach: the bush walks at a red tower; a red Knight 1067 to its right
    // walks the other way; a red Zap kills the bush on its 6th walking tick.
    let mut s = BattleState::new(0, shipped());
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "SuspiciousBush", at((9000, 19000)), None), (Team::Red, "Knight", at((10067, 19000)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (bush, knight) = (ids[0], ids[1]);
    for _ in 0..5 {
        s.tick();
    }
    let before = s.entity(bush).expect("the bush walks").pos;
    s.spawn_unit(Team::Red, "Zap", before, None).expect("cast the Zap");
    let d0 = until_gone(&mut s, bush, 1, |_| {});
    let full = s.entity(knight).map(|v| v.max_hp).expect("the Knight");
    let knight_hp: Vec<i32> = {
        let mut hp = vec![s.entity(knight).map_or(0, |v| v.hp)];
        let mut probe = s.clone();
        for _ in 0..3 {
            probe.tick();
            hp.push(probe.entity(knight).map_or(0, |v| v.hp));
        }
        hp
    };
    let d = after_death(s, Team::Blue, d0);
    assert!(d.p != before, "the scene drifted: the bush did not step on its death tick");
    assert!(dist(d.p, before) <= 70, "the death point is the bush's step on its death tick, {} from its last frame", dist(d.p, before));
    let goblins: Vec<(u32, (i32, i32))> = d.goblins.iter().map(|(k, o, _)| (*k, *o)).collect();
    assert_eq!(goblins, vec![(12, (-500, 0)), (13, (500, 0))], "the same schedule about the point the bush stepped to");
    // The death area's Damage 100 lands on nobody: the red Knight beside the death point keeps its hp through D + 3.
    assert!(knight_hp.iter().all(|hp| *hp == full), "the red Knight lost hp by D + 3: {knight_hp:?}");
}
