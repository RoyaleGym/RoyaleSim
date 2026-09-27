//! THE FURNACE: A TROOP WITH AN INTERVAL SPAWNER (card.rs `interval_spawner_of`, `SpawnerSource::ActionInterval`;
//! state.rs `spawner_activate`, `spawner_pass`, `to_location_point`; spawner.INTERVAL_START_ORIGIN,
//! SPAWN_TO_LOCATION_OFFSET, ACTION_SPAWNER_SPAWN_SPEED).
//!
//! THE LAW, measured on client 16.402 (captures 20260920-071056 and 20260920-071744, two Furnaces at level 11):
//!   - the card is a spells_buildings row whose SummonCharacter, Furnace_rework, is a walking, shooting characters
//!     row: a troop on every frame;
//!   - its first Fire Spirit comes 38 ticks after its first frame (2 of 2) and then one every 100 ticks (7 of 7),
//!     walking or attacking: the 15.535.29 tables' ActionInterval (StartCounterAt 1950, Interval 5000);
//!   - the spirit appears at the Furnace's centre plus (0, +1500) in the owner's forward frame (MirroredY 3 read as
//!     half tiles), not turned by the Furnace's facing (7 of 9; the other 2 inside a crown tower's footprint);
//!   - the spirit deploys for the action's DeployTime 500, 10 frames, not its own 1000 (9 of 9);
//!   - the spirit takes the Furnace's level.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the card loads as a troop with an interval spawner of the Fire Spirits card, and walks;
//!   2. the first spirit on F + 38 (F the Furnace's first frame), then F + 138 and F + 238, for Blue and for Red;
//!   3. the first spirit stands exactly (0, +1500) from the Furnace for Blue and (0, -1500) for Red, while the Furnace
//!      faces off the y axis;
//!   4. the spirit deploys for 10 frames;
//!   5. the spirit's hitpoints are the Fire Spirits row at the Furnace's level;
//!   6. no drain: the Furnace's hitpoints stay whole while nothing hits it;
//!   7. both INTERVAL_START_ORIGIN arms give F + 38 at DeployTime 1000 (the corpus cannot separate them);
//!   8. the loader takes the interval block only with exactly its graph, and refuses every other shape.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test furnace`):
//!   * `card_table_kind` -- the loader takes the kind of the table the card is listed in: (1) red.
//!   * `interval_spawner_never_fires` -- the block loads and never runs: (2), (3), (4) and (5) red; the Spawn*
//!     spawners' gates (tests/spawner.rs) stay green.
//!   * `to_location_by_facing` -- the offset turned by the Furnace's facing: (3) red.
//!   * `action_spawn_unit_deploy` -- the spirit deploys for its own 1000: (4) red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, CardKind, CardSource, SpawnerSource};
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, IntervalStart};
use royalesim::{EntityId, Team};

const FURNACE: &str = "FirespiritHut";
const SPIRIT: &str = "FireSpirits";

/// A blue Furnace on its own half, left of the centre line, native units: it walks to the left bridge, off the y
/// axis. Red's is the seat rotation of it.
const AT: (i32, i32) = (7000, 9000);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Level 11, the level every figure here was measured at.
fn cfg() -> BattleConfig {
    let mut c = config();
    c.card_level = [11, 11];
    c.tower_level = [11, 11];
    c
}

/// One tick of a Furnace run: k (0 = the Furnace's first frame), the Furnace's position and facing (None once it is
/// gone), and the spirits of its team that appeared on this tick.
struct Tick {
    k: u32,
    furnace: Option<(EntityId, Vec2, Vec2)>,
    born: Vec<(EntityId, Vec2)>,
}

/// A Furnace of `team` played at AT (Blue) or its rotation (Red), `ticks` ticks.
fn run(c: BattleConfig, team: Team, ticks: u32) -> (BattleState, Vec<Tick>) {
    let mut s = BattleState::new(0, c);
    let p = if team == Team::Blue { at(AT) } else { mirror(&s, at(AT)) };
    s.spawn_unit(team, FURNACE, p, None).expect("play the Furnace");
    let mut seen: Vec<EntityId> = Vec::new();
    let mut out = Vec::new();
    for k in 0..ticks {
        s.tick();
        let furnace = s.entities().find(|v| v.card == FURNACE && v.team == team).map(|v| (v.id, v.pos, v.facing));
        let mut born = Vec::new();
        for v in s.entities().filter(|v| v.card == SPIRIT && v.team == team) {
            if !seen.contains(&v.id) {
                seen.push(v.id);
                born.push((v.id, v.pos));
            }
        }
        out.push(Tick { k, furnace, born });
    }
    (s, out)
}

fn emission_ticks(t: &[Tick]) -> Vec<u32> {
    t.iter().filter(|x| !x.born.is_empty()).map(|x| x.k).collect()
}

/// 1. Plant: card_table_kind.
#[test]
fn the_furnace_is_a_troop_card_with_an_interval_spawner() {
    let s = BattleState::new(0, cfg());
    let db = s.cards();
    let c = card_stat(&s, FURNACE);
    assert_eq!(c.kind, CardKind::Troop, "the Furnace puts a troop on the board");
    assert_eq!(c.unit_name, "Furnace_rework");
    assert_eq!((c.speed, c.deploy_time_ms, c.lifetime_ms), (60, 1000, None), "Furnace_rework: Speed 60, DeployTime 1000, no LifeTime");
    assert!(c.death_spawn.is_none() && c.life_state.is_none(), "no death spawn, no life-state controller");
    let sp = c.spawner.expect("the interval spawner");
    assert_eq!(sp.source, SpawnerSource::ActionInterval);
    assert_eq!((sp.number, sp.interval_ms, sp.start_time_ms, sp.pause_time_ms), (1, 0, Some(1950), 5000), "StartCounterAt 1950, Interval 5000, one unit");
    assert_eq!((sp.limit, sp.radius, sp.to_location, sp.emit_deploy_ms), (None, None, Some((0, 3)), Some(500)), "MirroredX 0, MirroredY 3, DeployTime 500");
    assert_eq!(Some(sp.unit), db.index(SPIRIT), "the unit is the Fire Spirits card's own row, loaded once");
    let (_, t) = run(cfg(), Team::Blue, 60);
    let first = t.iter().find_map(|x| x.furnace).expect("the Furnace stands");
    let last = t.last().and_then(|x| x.furnace).expect("the Furnace stands");
    let (s2, _) = run(cfg(), Team::Blue, 1);
    assert_eq!(s2.entities().find(|v| v.card == FURNACE).map(|v| v.kind), Some(EntityKind::Troop));
    assert_ne!(first.1, last.1, "the Furnace did not walk");
}

/// 2. Plant: interval_spawner_never_fires.
#[test]
fn first_spirit_38_ticks_after_the_first_frame_then_every_100() {
    for team in [Team::Blue, Team::Red] {
        let (_, t) = run(cfg(), team, 250);
        let got = emission_ticks(&t);
        // By tick 238 the Furnace may be under a crown tower's fire; one that did not live into a tick emits nothing on it
        // (a death comes after the Move phase's pass, so a Furnace alive at the end of tick k - 1 emits on k).
        let want: Vec<u32> = [38, 138, 238].into_iter().filter(|k| t[*k as usize - 1].furnace.is_some()).collect();
        assert!(want.len() >= 2, "{team:?}: the Furnace died before its second spirit; the scene tests too little");
        assert_eq!(got, want, "{team:?}: the emission ticks");
        assert!(t.iter().all(|x| x.born.len() <= 1), "{team:?}: more than one spirit on one tick");
    }
}

/// 3. Plant: to_location_by_facing.
#[test]
fn the_spirit_stands_1500_owner_forward_whatever_the_facing() {
    for (team, dy) in [(Team::Blue, 1500), (Team::Red, -1500)] {
        let (_, t) = run(cfg(), team, 40);
        let x = &t[38];
        let (_, fpos, facing) = x.furnace.expect("the Furnace stands");
        let (_, spos) = *x.born.first().expect("the first spirit");
        // The precondition: the Furnace faces off the y axis, so a reading turned by its facing would not give (0, dy).
        assert!(facing.x != 0 && facing.y != 0, "{team:?}: the Furnace faces along an axis ({facing:?}); the scene tests nothing");
        assert_eq!(spos.sub(fpos), Vec2::new(0, dy * K), "{team:?}: the spirit's offset from the Furnace, native {:?}", (spos.sub(fpos).x / K, spos.sub(fpos).y / K));
    }
}

/// 4. Plant: action_spawn_unit_deploy.
#[test]
fn the_spirit_deploys_on_the_actions_500() {
    let mut s = BattleState::new(0, cfg());
    s.spawn_unit(Team::Blue, FURNACE, at(AT), None).unwrap();
    let mut spirit: Option<EntityId> = None;
    let mut deploying = 0;
    for _ in 0..70 {
        s.tick();
        if spirit.is_none() {
            spirit = s.entities().find(|v| v.card == SPIRIT && v.team == Team::Blue).map(|v| v.id);
        }
        if let Some(v) = spirit.and_then(|id| s.entity(id)) {
            deploying += usize::from(v.deploying);
        }
    }
    assert!(spirit.is_some(), "no spirit");
    assert_eq!(deploying, 10, "the spirit's deploying frames (the action's DeployTime 500; its own row says 1000)");
}

/// 5. The spirit takes the Furnace's level.
#[test]
fn the_spirit_takes_the_furnaces_level() {
    let (s, t) = run(cfg(), Team::Blue, 40);
    let (id, _) = *t[38].born.first().expect("the first spirit");
    let db = s.cards();
    let row = db.get(db.index(SPIRIT).unwrap());
    let want = db.scaled(db.index(SPIRIT).unwrap(), 11, row.hitpoints).unwrap();
    // The 15.535.29 row (85) at level 11 is 217; the 16.402 corpus reads 215, the known Fire Spirits drift between the
    // two clients' tables (tests/levels.rs `known_misses`).
    assert_eq!(want, 217);
    assert_eq!(s.entity(id).map(|v| v.max_hp), Some(want), "the spirit at the Furnace's level 11");
}

/// 6. No drain on a troop.
#[test]
fn the_furnace_does_not_drain() {
    let (s, t) = run(cfg(), Team::Blue, 120);
    let id = t.iter().find_map(|x| x.furnace).map(|f| f.0).expect("the Furnace");
    let v = s.entity(id).expect("the Furnace stands");
    assert_eq!((v.hp, v.max_hp), (727, 727), "284 at level 11 (x256 %) is 727, as measured, and nothing drains it");
}

/// 7. spawner.INTERVAL_START_ORIGIN: the two arms agree at DeployTime 1000.
#[test]
fn both_start_origins_give_f_plus_38_at_deploy_time_1000() {
    for arm in [IntervalStart::PlacementCounter, IntervalStart::ActivationDeployTime] {
        let mut c = cfg();
        c.calib.interval_start_origin = arm;
        let (_, t) = run(c, Team::Blue, 45);
        assert_eq!(emission_ticks(&t).first(), Some(&38), "{arm:?}");
    }
}

/// 8. The loader's side, on a synthetic file: a Spirit card and Ovens carrying the block in several shapes.
#[test]
fn the_loader_takes_the_interval_block_only_with_exactly_its_graph() {
    let graph = |classes: &str, spawns: &str| format!(r#""action_graph":{{"class_types":[{classes}],"spawns":[{spawns}],"mechanic":true}}"#);
    let good = graph(r#""ActionInterval","ActionPlayEffect","ActionSpawnToLocation""#, r#""CharacterType:Spirit""#);
    let block = |affected: bool, tags: &str| {
        format!(
            r#""interval_spawner":{{"start_counter_at_ms":1950,"interval_ms":5000,"affected_by_spawn_speed":{affected},"pause_tags":[{tags}],"character":"Spirit","deploy_time_ms":500,"mirrored_x":0,"mirrored_y":3}}"#
        )
    };
    let oven = |name: &str, extra: &str| {
        format!(r#"{{"name":"{name}","kind":"troop","elixir":4,"rarity":"Common","hitpoints":284,"damage":70,"hit_speed_ms":1700,"range_milli":5500,"collision_radius_milli":600,"speed":60,"deploy_time_ms":1000,{extra}}}"#)
    };
    let rows = [
        r#"{"name":"Spirit","kind":"troop","elixir":2,"rarity":"Common","hitpoints":85,"damage":81,"hit_speed_ms":300,"range_milli":2500,"collision_radius_milli":400,"speed":120,"deploy_time_ms":1000}"#.to_string(),
        oven("Oven", &format!("{good},{}", block(true, r#""NO_SUMMON""#))),
        oven("GraphOnly", &good),
        oven("ExtraClass", &format!("{},{}", graph(r#""ActionCounter","ActionInterval","ActionSpawnToLocation""#, r#""CharacterType:Spirit""#), block(true, ""))),
        oven("OtherSpawn", &format!("{},{}", graph(r#""ActionInterval","ActionSpawnToLocation""#, r#""CharacterType:Imp""#), block(true, ""))),
        oven("Unaffected", &format!("{good},{}", block(false, ""))),
        oven("Tagged", &format!("{good},{}", block(true, r#""SOMETHING_ELSE""#))),
        oven("BothBlocks", &format!(r#"{good},{},"spawner":{{"character":"Spirit","number":1,"pause_time_ms":5000}}"#, block(true, ""))),
    ];
    let db = CardDb::from_json_str(&format!(r#"{{"cards":[{}]}}"#, rows.join(",")), CardSource::DerivedJson).expect("the file parses");
    let oven = db.get(db.index("Oven").unwrap_or_else(|| panic!("Oven refused: {:?}", db.rejected)));
    let sp = oven.spawner.expect("the interval spawner");
    assert_eq!((sp.source, Some(sp.unit)), (SpawnerSource::ActionInterval, db.index("Spirit")));
    let why = |n: &str| db.rejected.iter().find(|(r, _)| r == n).map(|(_, w)| w.clone()).unwrap_or_else(|| panic!("{n} loaded"));
    assert!(why("GraphOnly").starts_with("the unit runs an action graph this loader does not read"), "{}", why("GraphOnly"));
    for (n, text) in [
        ("ExtraClass", "runs more than the interval"),
        ("OtherSpawn", "rather than the block's one character"),
        ("Unaffected", "SpawnSpeed"),
        ("Tagged", "pause tag SOMETHING_ELSE"),
        ("BothBlocks", "a Spawn* block and an interval spawner"),
    ] {
        assert!(why(n).contains(text), "{n}: {}", why(n));
    }
}
