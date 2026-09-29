//! THE GRAVEYARD: an area whose actions put units down on a schedule (card.rs `scheduled_area`,
//! `SpellShape::ScheduledArea`; spell.rs `step_spells`, the Scheduled arm, and `delay_ticks`; state.rs `phase_projectile`
//! and `scheduled_point`; calibration actions.SUB_ACTIONS_DELAY, actions.SUB_TICK_DELAY_ROUNDING,
//! actions.TEAM_Y_DIRECTION, spells.SCHEDULED_SPAWN_INVALID_POINT).
//!
//! THE ROWS (the 15.535.29 tables): the Graveyard's area Graveyard_rework (Radius 4000, LifeDuration 9000, no Damage, no
//! hit flags) runs a group of twelve ActionSpawnToLocation entries over eight named actions, each putting down one
//! Graveyard_rework_Skeleton with UseDeploy and DeployTime 500 at X = x + dx s, Y = y - dy t (s = -1 past the arena's
//! centre line, else +1; t = team_y_direction), at SubActionsDelay 2200, 2700, 3300, 3800, 4400, 4900, 5500, 6000, 6500,
//! 7100, 7600 and 8200.
//!
//! THE LAW, measured on client 15.535.29 (six casts, 72 Skeletons; C the cast tick, the first tick run after the play,
//! k = 0 below):
//!   - Skeleton k comes on C + delay_k / 50: C + 44, 54, 66, 76, 88, 98, 110, 120, 130, 142, 152 and 164 (120 of 120);
//!   - a side-0 cast at (4500, 21500) puts them at (1000, 21500), (2000, 24000), (4500, 18000), (8000, 21500),
//!     (7000, 19000), (1000, 21500), (4500, 18000), (2000, 19000), (8000, 21500), (4500, 25000), (7000, 24000),
//!     (2000, 24000): team_y_direction is -1 for side 0 (actions.TEAM_Y_DIRECTION = against_forward). The tenth is
//!     inside the princess tower and is created on its slot;
//!   - a cast past the centre line mirrors the x offsets; a side-1 cast is the rotation of a side-0 cast (12 of 12);
//!   - a slot off the arena comes down at x 250, its y kept, and a slot on the river is kept there
//!     (spells.SCHEDULED_SPAWN_INVALID_POINT = clamp_keep_water);
//!   - a Skeleton deploys its action's 500 (S..S + 9 deploying, S + 10 active, S + 11 its first step, S its first
//!     frame), and an enemy may target it from S + 7 (targeting.SPAWNED_UNIT_ACQUIRE_DELAY);
//!   - the area hits nothing.
//!
//! WHAT IS PINNED, each with its precondition (level 11):
//!   1. the loader reads the Graveyard as a scheduled area of twelve entries, and `unit_refs` names each entry;
//!   2. a side-0 cast at (4500, 21500): each Skeleton on its tick and on its slot, under actions.SUB_ACTIONS_DELAY =
//!      from_group_start (and cumulative loses every entry after the third past the life);
//!   3. a side-0 cast at (13500, 21500) mirrors the x offsets; a side-1 cast at (13500, 10500) is the rotation; under
//!      actions.TEAM_Y_DIRECTION = forward the side-0 cast's y offsets turn over;
//!   4. the first Skeleton of a cast deploys 10 ticks, first steps on S + 11 and is acquirable from S + 7;
//!   5. a side-0 cast at (1500, 16500): five slots off the arena at x 250, two of them and two more on the river,
//!      kept there under clamp_keep_water and put on land under clamp_then_eject_to_land;
//!   6. the area hits nothing: two Knights inside it through C + 43 play as in the battle without the cast;
//!   7. a battle saved with a Graveyard on the board resumes as the same battle, and the area's clock is state.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test scheduled_area`):
//!   * `schedule_entries_dedup` -- one entry per distinct action (8, not 12): (1), (2), (3) and (5) go red.
//!   * `unit_refs_skips_scheduled` -- `unit_refs` names no entry: (1) goes red (and tests/unit_refs.rs).
//!   * `schedule_cumulative` -- the delays read as gaps whatever the key: (2), (3) and (5) go red.
//!   * `nearer_wall_unmirrored` -- the x offset never mirrored: (3) goes red.
//!   * `scheduled_spawn_unit_deploy_time` -- the Skeleton deploys its own 1000: (4) goes red.
//!   * `scheduled_acquire_delay_dropped` -- the Skeleton is a target from its first frame: (4) goes red.
//!   * `hash_skips_schedule_clock` -- the area's clock is not hashed: (7) goes red.
//!
//! (6) has no plant of its own: no arm of the engine lets a scheduled area land anything (the Graveyard's row carries no
//! Damage, Buff or Pushback), so it pins the property against a change that would.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{SpawnOffset, SpellPlacement, SpellShape, UnitRef};
use royalesim::fixed::{milli, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::SpellMotion;
use royalesim::state::{BattleConfig, BattleState, ScheduledSpawnInvalidPoint, ScheduledUnitFirstUpdate, SubActionsDelay, SubTickDelayRounding, TeamYDirection};
use royalesim::{EntityId, Team};

/// The level of the measured casts.
const LEVEL: i32 = 11;

/// The Skeleton row the Graveyard puts down (a summon-only record: no card holds the name).
const SKELETON: &str = "Graveyard_rework_Skeleton";

/// Each entry's tick after the cast, C + delay / 50.
const TICKS: [u32; 12] = [44, 54, 66, 76, 88, 98, 110, 120, 130, 142, 152, 164];

/// The measured slots of a side-0 cast at (4500, 21500), native.
const SLOTS: [(i32, i32); 12] = [
    (1000, 21500),
    (2000, 24000),
    (4500, 18000),
    (8000, 21500),
    (7000, 19000),
    (1000, 21500),
    (4500, 18000),
    (2000, 19000),
    (8000, 21500),
    (4500, 25000),
    (7000, 24000),
    (2000, 24000),
];

/// The shipped config at level 11, asserting the arms this file pins.
fn shipped() -> BattleConfig {
    let mut c = config();
    c.card_level = [LEVEL, LEVEL];
    c.tower_level = [LEVEL, LEVEL];
    // The slots and the schedule were pinned with each unit standing on its slot on its first frame (spawner.
    // SCHEDULED_UNIT_FIRST_UPDATE = next_tick); the shipped first update on the creation tick pushes the ones that
    // are born overlapping a body.
    c.calib.scheduled_unit_first_update = ScheduledUnitFirstUpdate::NextTick;
    assert_eq!(c.calib.sub_actions_delay, SubActionsDelay::FromGroupStart, "the shipped actions.SUB_ACTIONS_DELAY");
    assert_eq!(c.calib.sub_tick_delay_rounding, SubTickDelayRounding::FloorFromCreation, "the shipped actions.SUB_TICK_DELAY_ROUNDING");
    assert_eq!(c.calib.team_y_direction, TeamYDirection::AgainstForward, "the shipped actions.TEAM_Y_DIRECTION");
    assert_eq!(c.calib.scheduled_spawn_invalid_point, ScheduledSpawnInvalidPoint::ClampKeepWater, "the shipped spells.SCHEDULED_SPAWN_INVALID_POINT");
    c
}

/// A native point as engine subtiles.
fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// An engine point as native units.
fn native(p: Vec2) -> (i32, i32) {
    (p.x / K, p.y / K)
}

/// A Graveyard cast by `team` at `tap` (native): every Skeleton it puts down, as (k after C, its native point on its
/// first frame), in creation order, over C .. C + `ticks` - 1.
fn skeletons(cfg: BattleConfig, team: Team, tap: (i32, i32), ticks: u32) -> Vec<(u32, (i32, i32))> {
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(team, "Graveyard", at(tap), None).expect("cast the Graveyard");
    let mut seen: Vec<EntityId> = Vec::new();
    let mut out = Vec::new();
    for k in 0..ticks {
        s.tick();
        let mut fresh: Vec<(u32, EntityId, Vec2)> = s.entities().filter(|e| e.team == team && e.card == SKELETON && !seen.contains(&e.id)).map(|e| (e.team_seq, e.id, e.pos)).collect();
        fresh.sort_by_key(|f| f.0);
        for (_, id, p) in fresh {
            seen.push(id);
            out.push((k, native(p)));
        }
    }
    out
}

/// TICKS paired with `slots`.
fn scheduled(slots: [(i32, i32); 12]) -> Vec<(u32, (i32, i32))> {
    TICKS.iter().copied().zip(slots).collect()
}

// ---------------------------------------------------------------------------
// (1)

/// Plants: schedule_entries_dedup, unit_refs_skips_scheduled.
#[test]
fn the_loader_reads_the_graveyard_as_a_scheduled_area_of_twelve_entries() {
    let s = BattleState::new(0, shipped());
    let db = s.cards();
    let g = db.index("Graveyard").unwrap_or_else(|| panic!("Graveyard refused: {:?}", db.rejected.iter().find(|(n, _)| n == "Graveyard")));
    let spell = db.get(g).spell.as_ref().expect("the Graveyard loads as a spell");
    assert_eq!(spell.placement, SpellPlacement::Anywhere, "anywhere, the river and the towers included");
    let SpellShape::ScheduledArea { life_ms, schedule } = &spell.shape else { panic!("Graveyard: {:?}", spell.shape) };
    assert_eq!(*life_ms, 9000, "LifeDuration");
    assert_eq!(schedule.len(), 12, "twelve entries, the repeats kept: {schedule:?}");
    let delays: Vec<i32> = schedule.iter().map(|e| e.delay_ms).collect();
    assert_eq!(delays, vec![2200, 2700, 3300, 3800, 4400, 4900, 5500, 6000, 6500, 7100, 7600, 8200], "SubActionsDelay");
    // The offsets as the table's expressions write them, against the file itself.
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("cards.json parses");
    let row = doc["cards"].as_array().expect("a cards array").iter().find(|c| c["name"] == "Graveyard").expect("the Graveyard row");
    let entries = row["spell"]["area_effect_object"]["schedule"]["entries"].as_array().expect("the area's schedule");
    assert_eq!(entries.len(), 12, "the file's schedule");
    let skel = db.index(SKELETON).expect("the Skeleton row loads");
    for (k, (e, raw)) in schedule.iter().zip(entries).enumerate() {
        let (dx, dy) = (raw["x"]["offset_milli"].as_i64().unwrap() as i32, raw["y"]["offset_milli"].as_i64().unwrap() as i32);
        assert_eq!(e.offset, SpawnOffset::MirroredToWall { dx: milli(dx), dy: milli(dy) }, "entry {k}'s offset");
        assert_eq!(e.deploy_time_ms, Some(500), "entry {k}'s DeployTime");
        assert_eq!(e.unit, skel, "entry {k}'s unit");
    }
    let u = db.get(skel);
    assert!(u.summon_only && u.unit_name == SKELETON, "the Skeleton is a summon-only record: {} / {}", u.name, u.unit_name);
    assert_eq!(db.scaled(skel, LEVEL, u.hitpoints), Ok(81), "32 hp at level 11");
    let want: Vec<(UnitRef, u16, Option<i32>)> = (0..12).map(|k| (UnitRef::Scheduled(k as u8), skel, None)).collect();
    assert_eq!(db.unit_refs(g), want, "unit_refs names every entry");
    db.check_levels(g, LEVEL).expect("the Graveyard and its Skeletons at level 11");
}

// ---------------------------------------------------------------------------
// (2)

/// Plant: schedule_cumulative.
#[test]
fn each_skeleton_comes_on_its_tick_at_its_slot() {
    assert_eq!(skeletons(shipped(), Team::Blue, (4500, 21500), 170), scheduled(SLOTS), "a side-0 cast at (4500, 21500): (k, slot)");
    // actions.SUB_ACTIONS_DELAY = cumulative reads the delays as gaps: 2200, 4900 and 8200 fall inside the 9000 ms life,
    // and every later entry is lost with the area.
    let mut c = shipped();
    c.calib.sub_actions_delay = SubActionsDelay::Cumulative;
    let got = skeletons(c, Team::Blue, (4500, 21500), 200);
    assert_eq!(got, vec![(44, SLOTS[0]), (98, SLOTS[1]), (164, SLOTS[2])], "cumulative: (k, slot)");
}

// ---------------------------------------------------------------------------
// (3)

/// Plant: nearer_wall_unmirrored.
#[test]
fn a_cast_past_the_centre_line_mirrors_and_a_side_1_cast_is_the_rotation() {
    let mirrored = SLOTS.map(|(x, y)| (18000 - x, y));
    assert_eq!(skeletons(shipped(), Team::Blue, (13500, 21500), 170), scheduled(mirrored), "a side-0 cast at (13500, 21500): the x offsets mirrored");
    let rotated = SLOTS.map(|(x, y)| (18000 - x, 32000 - y));
    assert_eq!(skeletons(shipped(), Team::Red, (13500, 10500), 170), scheduled(rotated), "a side-1 cast at (13500, 10500): the rotation");
    // actions.TEAM_Y_DIRECTION = forward: the side-0 cast's y offsets turn over about the centre.
    let mut c = shipped();
    c.calib.team_y_direction = TeamYDirection::Forward;
    let turned = SLOTS.map(|(x, y)| (x, 43000 - y));
    assert_eq!(skeletons(c, Team::Blue, (4500, 21500), 170), scheduled(turned), "forward: the y offsets on the other side");
}

// ---------------------------------------------------------------------------
// (4)

/// Plants: scheduled_spawn_unit_deploy_time, scheduled_acquire_delay_dropped.
#[test]
fn the_first_skeleton_deploys_ten_ticks_and_is_acquirable_from_its_eighth_frame() {
    // A side-0 cast on its own side, where no enemy stands: the first Skeleton on (1000, 9500), 2700 from the second.
    let mut s = BattleState::new(0, shipped());
    s.spawn_unit(Team::Blue, "Graveyard", at((4500, 9500)), None).expect("cast the Graveyard");
    for _ in 0..TICKS[0] {
        s.tick();
    }
    assert!(s.entities().all(|e| e.card != SKELETON), "a Skeleton before C + 44");
    s.tick(); // S = C + 44
    let first = s.entities().find(|e| e.card == SKELETON).map(|e| (e.id, e.pos)).expect("the first Skeleton on C + 44");
    let born = s.tick_count() - 1;
    let v = s.entity(first.0).unwrap();
    assert_eq!(native(v.pos), (1000, 9500), "the first slot");
    assert!(v.deploying && v.deploy_ms == 500, "deploying its action's DeployTime: {} ms", v.deploy_ms);
    assert_eq!(v.acquirable_from, born + 7, "an enemy may target it from S + 7");
    let mut deploying = vec![v.deploying];
    let mut last = v.pos;
    let mut first_step = None;
    for t in 1..=11u32 {
        s.tick();
        let v = s.entity(first.0).unwrap_or_else(|| panic!("the scene drifted: the Skeleton died on S + {t}"));
        deploying.push(v.deploying);
        if first_step.is_none() && v.pos != last {
            first_step = Some(t);
        }
        last = v.pos;
    }
    assert_eq!(deploying, (0..=11u32).map(|t| t < 10).collect::<Vec<_>>(), "deploying on S .. S + 9");
    assert_eq!(first_step, Some(11), "the first step on S + 11");
}

// ---------------------------------------------------------------------------
// (5)

/// Plants: schedule_entries_dedup, schedule_cumulative.
#[test]
fn a_slot_off_the_arena_is_clamped_and_a_slot_on_the_river_is_kept() {
    let s = BattleState::new(0, shipped());
    let (river, bridge_end) = ((250, 16500), (5000, 16500));
    for p in [river, bridge_end] {
        assert!(!s.arena().is_passable_ground(at(p)), "the scene drifted: {p:?} is not on the river");
    }
    for p in [(250, 19000), (250, 14000)] {
        assert!(s.arena().is_passable_ground(at(p)), "the scene drifted: {p:?} is not on land");
    }
    // A side-0 cast at (1500, 16500): slots 1, 2, 6, 8 and 12 fall off the arena's left edge.
    let kept = [river, (250, 19000), (1500, 13000), bridge_end, (4000, 14000), river, (1500, 13000), (250, 14000), bridge_end, (1500, 20000), (4000, 19000), (250, 19000)];
    assert_eq!(skeletons(shipped(), Team::Blue, (1500, 16500), 170), scheduled(kept), "clamp_keep_water: (k, slot)");
    // clamp_then_eject_to_land puts the four river slots on land and leaves the others.
    let mut c = shipped();
    c.calib.scheduled_spawn_invalid_point = ScheduledSpawnInvalidPoint::ClampThenEjectToLand;
    let got = skeletons(c, Team::Blue, (1500, 16500), 170);
    assert_eq!(got.len(), 12, "twelve Skeletons: {got:?}");
    for (k, ((tick, p), want)) in got.iter().zip(kept).enumerate() {
        assert_eq!(*tick, TICKS[k], "entry {k}'s tick");
        if want == river || want == bridge_end {
            // Ejected to the first subtile past the bank's half-cell boundary (arena.rs `nearest_passable_ground`),
            // which the native point this test reads rounds back onto the boundary itself: land within one subtile.
            let q = at(*p);
            let on_land = [(0, 0), (0, 1), (0, -1), (1, 0), (-1, 0)].iter().any(|&(dx, dy)| s.arena().is_passable_ground(Vec2::new(q.x + dx, q.y + dy)));
            assert!(on_land, "entry {k} left on the river at {p:?}");
            assert_ne!(*p, want, "entry {k} not moved");
        } else {
            assert_eq!(*p, want, "entry {k} on land moved");
        }
    }
}

// ---------------------------------------------------------------------------
// (6)

/// One Knight on one tick: its hp, its native point, the buffs it carries.
type KnightRow = (i32, (i32, i32), usize);

/// A Blue and a Red Knight near (9000, 12500), and with `cast` a Blue Graveyard there; per tick C .. C + 43, each
/// Knight's row.
fn knights_inside(cast: bool) -> Vec<[KnightRow; 2]> {
    let mut s = BattleState::new(0, shipped());
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Knight", at((8000, 12000)), None), (Team::Red, "Knight", at((10000, 13000)), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    if cast {
        s.spawn_unit(Team::Blue, "Graveyard", at((9000, 12500)), None).expect("cast the Graveyard");
    }
    let mut out = Vec::new();
    for _ in 0..TICKS[0] {
        s.tick();
        if cast {
            let area: Vec<&royalesim::spell::Spell> = s.spells().iter().filter(|sp| matches!(sp.motion, SpellMotion::Scheduled { .. })).collect();
            assert_eq!(area.len(), 1, "the Graveyard's one object stands");
            assert_eq!(area[0].damage, 0, "the area deals nothing");
        }
        let row = [ids[0], ids[1]].map(|id| s.entity(id).map_or((0, (0, 0), 0), |v| (v.hp, native(v.pos), v.buffs.iter().filter(|b| b.id != 0).count())));
        out.push(row);
    }
    out
}

/// No plant (the module doc).
#[test]
fn the_graveyard_hits_nothing() {
    let with = knights_inside(true);
    let without = knights_inside(false);
    assert_eq!(with, without, "the two Knights inside the Graveyard through C + 43, against the battle without it");
    assert!(with.iter().all(|r| r[0].2 == 0), "the own Knight carries a buff");
    assert!(with.iter().any(|r| r[0].0 < with[0][0].0 || r[1].0 < with[0][1].0), "vacuous: the Knights never fought, so a changed hp would not show");
}

// ---------------------------------------------------------------------------
// (7)

/// Plant: hash_skips_schedule_clock.
#[test]
fn a_battle_with_a_graveyard_resumes_as_the_same_battle_and_its_clock_is_state() {
    let mut s = BattleState::new(0, shipped());
    s.spawn_unit(Team::Blue, "Graveyard", at((4500, 9500)), None).expect("cast the Graveyard");
    for _ in 0..50 {
        s.tick();
    }
    let area = s.spells().iter().find_map(|sp| match sp.motion {
        SpellMotion::Scheduled { born, fired, .. } => Some((born, fired)),
        _ => None,
    });
    let (born, fired) = area.expect("the Graveyard's object stands on C + 49");
    assert_eq!(fired, 1, "the first entry released by C + 49, the second not yet");
    let mut r = BattleState::load(&s.save()).expect("the save loads");
    for t in 0..130u32 {
        assert_eq!(r.state_hash(), s.state_hash(), "the resumed battle left the original {t} ticks after the save");
        s.tick();
        r.tick();
    }
    assert_eq!(s.entities().filter(|e| e.card == SKELETON).count(), r.entities().filter(|e| e.card == SKELETON).count(), "the Skeletons");
    // The clock: a save edited only in the creation tick, or only in the released entries, fails the load's check.
    let mut s = BattleState::new(0, shipped());
    s.spawn_unit(Team::Blue, "Graveyard", at((4500, 9500)), None).expect("cast the Graveyard");
    for _ in 0..50 {
        s.tick();
    }
    let motion = |v: &mut serde_json::Value| -> usize {
        v["spells"].as_array().expect("the spells").iter().position(|sp| sp["motion"].get("Scheduled").is_some()).expect("the saved Graveyard")
    };
    assert!(
        edit_is_hashed(&s, |v| {
            let i = motion(v);
            v["spells"][i]["motion"]["Scheduled"]["born"] = serde_json::json!(born + 1);
        }),
        "the area's creation tick is not hashed"
    );
    assert!(
        edit_is_hashed(&s, |v| {
            let i = motion(v);
            v["spells"][i]["motion"]["Scheduled"]["fired"] = serde_json::json!(fired | 2);
        }),
        "the area's released entries are not hashed"
    );
}
