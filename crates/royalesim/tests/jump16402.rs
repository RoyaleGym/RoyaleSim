//! THE RIVER JUMP -- calibration movement.JUMP_WATER_HOP = client16402, the
//! JumpEnabled water hop as measured on the five hops of live capture 20260920-002736
//! (Hog Rider, Prince, three Royal Hogs; client 16.402). Implementation: path16402.rs
//! `cell_cost_for` (water at WATER_COST for a jumper), path2026.rs
//! `avoid_buildings16402` (the goal-cell box demotion off for a FLYING target),
//! jump16402.rs `landing_node` / `landed`, state.rs `phase_path16402` (the trigger
//! after the reached pop, the leap in place of the walk while `jumping`).
//!
//! WHAT IS PINNED, each from the data (tests/fixtures/oracle2026/client16402_jumps.json,
//! tools/make_client16402_jump_fixture.py) and the law, never a pasted number:
//!   1. every live hop replays from its trigger frame through the leap arithmetic
//!      alone -- position for position, and the landing on the tick the client
//!      landed, the third Royal Hog's ON a water cell included;
//!   2. the Hog Rider end to end through the shipped engine: placed where the client
//!      placed it, it reproduces every published node list and every position from
//!      its first step through the leap, the landing and fifteen ticks of the fresh
//!      path (the cost-7 water route across cells (24, 30)..(26, 32), the hop from the
//!      bank, the replan from the bridge cell);
//!   3. the Prince likewise, with the live client's own row (the 2018 cards.json
//!      Prince has no jump block and another Range) -- and the run-up HELD across the
//!      leap: the speed doubles on the 43rd WALKING tick, six ticks after the landing,
//!      as the client's did;
//!   4. the cost field: from the Hog's placement a jumper's route crosses water cells
//!      where a walker's does not, and the fixture gate (oracle2026.rs G6) is untouched
//!      because no corpus mover jumps;
//!   5. the goal cell: a ground unit whose target is a FLYING unit over a building box
//!      takes the boxed cell when it is nearer; a ground target keeps the demotion;
//!   6. a jumper whose next node is a bridge cell never hops (the first Royal Hog);
//!   7. determinism and a save / load mid-leap that reproduces every later tick;
//!   8. the walk_priced_water foil walks the water at Speed and never enters state 5;
//!   9. THE SHIPPED DATA carries the jump blocks: every test above
//!      writes the fixture's row over the card, so a cards.json regenerated without
//!      the `jump` block (the main tree's, until it re-runs the extractor) would pass
//!      them while the shipped Hog Rider walked to the bridge -- this one reads
//!      data/derived/cards.json's own Hog Rider and Prince through the loader, and
//!      the Hog's block against the fixture's 15.535 row, and the Hog leaps from the
//!      fixture's Hog Rider placement on the SHIPPED card alone.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="<name>"' CARGO_TARGET_DIR=target/plant cargo test
//! --test jump16402`): goal_ignores_flying_target red on (5) and green on every
//! oracle2026.rs gate; jumper_pays_blocked red on (2), (3), (4), (7), (8);
//! jump_never_hops red on (2), (3), (7); jump_never_lands red on (1), (2), (3), (7).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::arena::Arena;
use royalesim::card::{CardDb, JumpDef, KING_TOWER, PRINCESS_TOWER};
use royalesim::fixed::{milli, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::path::{FrameWorld, NavRequest, Obstacle};
use royalesim::state::{BattleConfig, BattleState, Calib, JumpWaterHop};
use royalesim::{jump16402, move16402, path2026, path16402, EntityId, Team};

const FIXTURE: &str = include_str!("fixtures/oracle2026/client16402_jumps.json");
// the cases the end-to-end gates run on (`<capture>:<Card>:<k>`)
const HOG: &str = "20260920-002736-B:HogRider:0";
const PRINCE: &str = "20260920-002736-B:Prince:0";
const ROYAL_HOG_BRIDGE: &str = "20260920-002736-B:RoyalHog:0";

#[derive(serde::Deserialize, Clone)]
struct Frame {
    tick: i32,
    pos: [i32; 2],
    state: i32,
    nodes: Vec<[i32; 2]>,
}

#[derive(serde::Deserialize, Clone)]
struct Case {
    name: String,
    card: String,
    card_15535: serde_json::Value,
    side: i32,
    spawn_native: [i32; 2],
    target_native: [i32; 2],
    hop_tick: Option<i32>,
    landing_tick: Option<i32>,
    hop_node: Option<[i32; 2]>,
    frames: Vec<Frame>,
}

#[derive(serde::Deserialize)]
struct Fixture {
    towers: Vec<serde_json::Value>,
    cases: Vec<Case>,
}

fn fixture() -> Fixture {
    serde_json::from_str(FIXTURE).expect("client16402_jumps.json parses")
}

fn native(p: Vec2) -> (i32, i32) {
    assert_eq!((p.x % K, p.y % K), (0, 0), "a position is a whole number of native units: {p:?}");
    (p.x / K, p.y / K)
}

fn sub(n: i32) -> i32 {
    n * K
}

fn stat(c: &Case, col: &str) -> i32 {
    c.card_15535[col].as_i64().unwrap_or_else(|| panic!("{}: card_15535.{col}", c.name)) as i32
}

/// The card a case walks as: cards.json's row with the live client's own Range /
/// CollisionRadius / Speed / jump block (`card_15535`) written over it. Identical to
/// cards.json for the Hog Rider (the 2018 row IS the live row); the 2018 Prince has
/// Range 1850 / CollisionRadius 650 and no jump block, and the live one 1600 / 600 /
/// {4000, 160}.
fn config_for(c: &Case) -> BattleConfig {
    let mut db: CardDb = cards();
    let idx = db.index(&c.card).unwrap_or_else(|| panic!("{} is not simulable", c.card)) as usize;
    let card = &mut db.cards[idx];
    card.range = milli(stat(c, "Range"));
    card.collision_radius = milli(stat(c, "CollisionRadius"));
    card.speed = stat(c, "Speed");
    card.sight_range = milli(stat(c, "SightRange"));
    assert!(c.card_15535["JumpEnabled"].as_bool().unwrap(), "{}: not a JumpEnabled row", c.name);
    card.jump = Some(JumpDef { speed: stat(c, "JumpSpeed"), height_raw: stat(c, "JumpHeight") });
    if let Some(ch) = card.charge.as_mut() {
        ch.range_raw = stat(c, "ChargeRange");
        ch.speed_multiplier_percent = stat(c, "ChargeSpeedMultiplier");
    }
    BattleConfig::with_cards(db)
}

fn team_of(c: &Case) -> Team {
    if c.side == 1 {
        Team::Red
    } else {
        Team::Blue
    }
}

/// The engine's cells of a route, goal first, as the client publishes them.
fn cells_of(arena: &Arena, route: &[Vec2]) -> Vec<[i32; 2]> {
    route
        .iter()
        .map(|&p| {
            let (c, r) = arena.subtile_to_half(p);
            [c, r]
        })
        .collect()
}

/// Spawn the case's unit where the client placed it and run until it first moves;
/// returns the battle at that tick and the unit's id. The trace's first frame with a
/// displacement is aligned to it (deploy timing is measured elsewhere).
fn spawn_and_walk(c: &Case) -> (BattleState, EntityId) {
    let cfg = config_for(c);
    let mut s = BattleState::new(11, cfg);
    let team = team_of(c);
    let pos = Vec2::new(sub(c.spawn_native[0]), sub(c.spawn_native[1]));
    let id = s.scenario_spawn_now(team, &c.card, pos, None).unwrap();
    assert_eq!(native(s.entity(id).unwrap().pos), (c.spawn_native[0], c.spawn_native[1]));
    for _ in 0..200 {
        let before = s.entity(id).unwrap().pos;
        s.tick();
        if s.entity(id).unwrap().pos != before {
            return (s, id);
        }
    }
    panic!("{}: the unit never moved", c.name);
}

/// Replay the case through the engine from its first moving frame and compare every
/// later frame's position and published list; returns the engine tick of each fixture
/// frame's `state` 5 for the caller's own checks.
fn replay(c: &Case) -> Vec<(i32, bool)> {
    let arena = Arena::shipped();
    let (mut s, id) = spawn_and_walk(c);
    let first_move = c.frames.iter().position(|f| f.pos != c.frames[0].pos).expect("a frame with a displacement");
    assert!(first_move >= 1, "{}: the trace starts moving", c.name);
    let t0 = c.frames[first_move].tick;
    let e0 = s.tick_count() as i32;
    let mut states = Vec::new();
    let mut checked = 0;
    for f in &c.frames[first_move..] {
        let want_tick = e0 + (f.tick - t0);
        while (s.tick_count() as i32) < want_tick {
            s.tick();
        }
        let e = s.entity(id).unwrap_or_else(|| panic!("{}: the unit died at trace tick {}", c.name, f.tick));
        assert_eq!(native(e.pos), (f.pos[0], f.pos[1]), "{}: position at trace tick {} (engine tick {want_tick})", c.name, f.tick);
        assert_eq!(e.jumping, f.state == 5, "{}: state at trace tick {}", c.name, f.tick);
        // the published list: the engine's route is the client's, except on the
        // landing tick, where the client already published the fresh path and the
        // engine's request runs one Path phase later (calibration
        // movement.JUMP_WATER_HOP open item 4)
        if Some(f.tick) != c.landing_tick {
            assert_eq!(cells_of(&arena, e.route), f.nodes, "{}: published list at trace tick {}", c.name, f.tick);
        } else {
            assert!(e.route.is_empty(), "{}: the landing tick drops the leap's node", c.name);
        }
        states.push((f.tick, e.jumping));
        checked += 1;
    }
    assert!(checked >= 25, "{}: only {checked} frames compared", c.name);
    states
}

// ---------------------------------------------------------------------------
// 1. the leap arithmetic alone, every live hop

#[test]
fn every_live_hop_replays_from_its_trigger_frame() {
    // JumpSpeed from the live rows (identical on every JumpEnabled card of 15.535);
    // the leap is moveTowards at that speed toward the single node's centre from the
    // trigger frame's position, landing when two JumpSpeeds short (jump16402.rs)
    let arena = Arena::shipped();
    let is_water = |c: i32, r: i32| arena.cell_bits(c, r) & arena.bit_water != 0;
    let mut hops = 0;
    for c in fixture().cases.iter().filter(|c| c.hop_tick.is_some()) {
        let speed = stat(c, "JumpSpeed");
        let node = c.hop_node.unwrap();
        let centre = jump16402::cell_centre(node[0], node[1]);
        let start = c.frames.iter().position(|f| Some(f.tick) == c.hop_tick).unwrap();
        let f0 = &c.frames[start];
        assert_eq!(f0.state, 5);
        assert_eq!(f0.nodes, vec![node], "{}: the trigger frame carries the single node", c.name);
        // the third Royal Hog's landing node is a bridge cell; the others too --
        // and every one is the first dry node past the water run of the list the
        // walk was consuming (the frame before the trigger)
        let before = &c.frames[start - 1];
        let list: Vec<(i32, i32)> = before.nodes.iter().map(|n| (n[0], n[1])).collect();
        // the walk popped `before`'s last node on the trigger tick; the scan runs on
        // what remains
        assert_eq!(jump16402::landing_node(&list[..list.len() - 1], is_water), Some((node[0], node[1])), "{}: the landing node from the popped list", c.name);
        let mut pos = (f0.pos[0], f0.pos[1]);
        let mut seg = move16402::segment_dir(pos.0, pos.1, centre);
        let mut landed_at = None;
        for f in &c.frames[start + 1..] {
            // ticks the capture does not carry are stepped through silently
            let mut tick = c.frames[c.frames.iter().position(|x| x.tick == f.tick).unwrap() - 1].tick;
            while tick < f.tick {
                let mut con = move16402::Contact::default();
                move16402::decay_offset(&mut con);
                let m = move16402::move_towards(pos, centre.0, centre.1, speed, true, &mut con, seg, false, is_water, arena.cols, arena.rows);
                pos = (m.x, m.y);
                seg = move16402::segment_dir(pos.0, pos.1, centre);
                tick += 1;
                if jump16402::landed(pos, centre, speed) {
                    landed_at = Some(tick);
                    break;
                }
            }
            assert_eq!(pos, (f.pos[0], f.pos[1]), "{}: leap position at trace tick {}", c.name, f.tick);
            if landed_at.is_some() {
                break;
            }
            assert_eq!(f.state, 5, "{}: still leaping at trace tick {}", c.name, f.tick);
        }
        assert_eq!(landed_at, c.landing_tick, "{}: the landing tick", c.name);
        hops += 1;
    }
    assert_eq!(hops, 5, "five live hops in the fixture (Hog, Prince, three Royal Hogs)");
}

// ---------------------------------------------------------------------------
// 2. / 3. end to end through the engine

#[test]
fn the_hog_rider_reproduces_through_the_engine() {
    let fx = fixture();
    let c = fx.cases.iter().find(|c| c.name == HOG).unwrap();
    let states = replay(c);
    let leaping: Vec<i32> = states.iter().filter(|(_, j)| *j).map(|(t, _)| *t).collect();
    assert_eq!(leaping.first().copied(), c.hop_tick);
    assert_eq!(leaping.last().map(|t| t + 1), c.landing_tick);
    assert!(leaping.len() >= 20, "the Hog's leap spans {} frames", leaping.len());
}

#[test]
fn the_prince_reproduces_through_the_engine_with_its_run_up_held_across_the_leap() {
    let fx = fixture();
    let c = fx.cases.iter().find(|c| c.name == PRINCE).unwrap();
    // the engine's Prince charges (cards.json); the live Prince walked 36 ticks, leapt
    // 22 and doubled its speed on the 6th walking tick after landing: 42 walking ticks
    // x 240 permille = the 43rd-tick doubling of charge.ACCUMULATOR, so the leap neither
    // added nor reset -- the positions after the landing carry the doubled steps
    let states = replay(c);
    let land = c.landing_tick.unwrap();
    let after: Vec<&Frame> = c.frames.iter().filter(|f| f.tick > land).collect();
    let steps: Vec<i32> = after.windows(2).map(|w| (w[1].pos[1] - w[0].pos[1]).abs()).collect();
    // the fixture itself shows the doubling within the fifteen ticks after the landing
    let slow = steps.iter().filter(|&&d| d < 100).count();
    let fast = steps.iter().filter(|&&d| d >= 100).count();
    assert!(slow >= 3 && fast >= 3, "the trace after the landing shows both walking speeds: {steps:?}");
    assert!(states.iter().any(|(_, j)| *j));
}

// ---------------------------------------------------------------------------
// 4. the cost field

/// The fixture's towers as the grid's occluders, both sides, in the mover's frame.
fn tower_obstacles(fx: &Fixture, arena: &Arena, db: &CardDb, team: Team) -> Vec<Obstacle> {
    let king = db.get(db.index(KING_TOWER).unwrap()).collision_radius;
    let princess = db.get(db.index(PRINCESS_TOWER).unwrap()).collision_radius;
    fx.towers
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let (x, y) = (t["x"].as_i64().unwrap() as i32, t["y"].as_i64().unwrap() as i32);
            let r = if x == 9000 { king } else { princess };
            let centre = arena.to_frame(team, Vec2::new(sub(x), sub(y)));
            Obstacle {
                id: EntityId { index: i as u32, generation: 0 },
                shape: royalesim::arena::Shape::Circle { c: centre, r },
                radius: r,
                key: (0, 0, 0, 0, i as u32),
                ally: true,
            }
        })
        .collect()
}

fn plan(fx: &Fixture, c: &Case, jumper: bool, reach_native: i32) -> Vec<[i32; 2]> {
    let arena = Arena::shipped();
    let calib = Calib::shipped();
    let db = cards();
    let team = team_of(c);
    let obstacles = tower_obstacles(fx, &arena, &db, team);
    let world = FrameWorld { arena: &arena, obstacles: &obstacles };
    let req = NavRequest {
        #[cfg(clash_plant = "reflection_bridge_tie")]
        red: team == Team::Red,
        team,
        pos: arena.to_frame(team, Vec2::new(sub(c.spawn_native[0]), sub(c.spawn_native[1]))),
        goal: arena.to_frame(team, Vec2::new(sub(c.target_native[0]), sub(c.target_native[1]))),
        radius: 0,
        sight: 0,
        step: 0,
        reach: sub(reach_native),
        flying: false,
        target_flying: false,
        jumper,
        ignore: None,
    };
    let (cells, ok) = path2026::plan_cells(&world, &calib, &req);
    assert!(ok);
    cells
        .into_iter()
        .map(|(col, row)| match team {
            Team::Blue => [col, row],
            Team::Red => [arena.cols - 1 - col, arena.rows - 1 - row],
        })
        .collect()
}

#[test]
fn a_jumper_routes_across_priced_water_where_a_walker_takes_the_bridge() {
    let fx = fixture();
    let arena = Arena::shipped();
    let is_water = |n: &[i32; 2]| arena.cell_bits(n[0], n[1]) & arena.bit_water != 0;
    let c = fx.cases.iter().find(|c| c.name == HOG).unwrap();
    let reach = stat(c, "Range") + stat(c, "CollisionRadius");
    let jumper = plan(&fx, c, true, reach);
    let walker = plan(&fx, c, false, reach);
    // the Hog's first published list: the engine's chain minus the node the client
    // had popped on the publishing tick
    let published = &c.frames.iter().find(|f| !f.nodes.is_empty()).unwrap().nodes;
    assert!(jumper.len() >= published.len() && jumper[..published.len()] == published[..], "the jumper's route is the Hog's first path:\n {jumper:?}\n {published:?}");
    let wet: Vec<&[i32; 2]> = jumper.iter().filter(|n| is_water(n)).collect();
    assert!(wet.len() >= 3, "the jumper's route crosses the river off the bridge: {jumper:?}");
    assert!(walker.iter().all(|n| !is_water(n)), "a walker's route never enters water: {walker:?}");
    assert_ne!(jumper, walker);
    // the same cost table, read through the ledger, prices the two
    let costs = path2026::costs16402(&Calib::shipped());
    assert!(costs.water < costs.default && costs.blocked > costs.default, "{costs:?}");
    let t = path2026::terrain16402(&arena, &costs);
    let occ = vec![0; (arena.cols * arena.rows) as usize];
    let w = wet[0];
    assert_eq!(path16402::cell_cost_for(&t, &occ, w[0], w[1], true), costs.water);
    assert_eq!(path16402::cell_cost_for(&t, &occ, w[0], w[1], false), costs.blocked);
}

// ---------------------------------------------------------------------------
// 5. the goal cell and a flying target

#[test]
fn a_ground_unit_chasing_a_flying_target_takes_the_boxed_cell() {
    // a Tombstone-sized building (R 1000) with the target hovering just beyond its
    // far edge and the mover below: the nearest in-reach cell to the mover lies
    // inside the box. Ground target: the demotion sends the goal to the nearest
    // UNBOXED in-reach cell. Flying target: the box is ignored and the nearest cell
    // wins.
    let arena = Arena::shipped();
    let costs = path2026::costs16402(&Calib::shipped());
    let t = path2026::terrain16402(&arena, &costs);
    let building = (9250, 19000);
    let target = (9250, 20250);
    let box_r = 1000;
    let occ = path16402::occlusion(&t, &[path16402::Occluder { x: building.0, y: building.1, r: box_r }], &costs);
    let mover = (9250, 12250);
    let reach = 1700;
    let ground = path16402::choose_goal_cell(&t, &occ, mover, target, reach, path2026::avoid_buildings16402(false), costs.building, true).unwrap();
    let flying = path16402::choose_goal_cell(&t, &occ, mover, target, reach, path2026::avoid_buildings16402(true), costs.building, true).unwrap();
    let boxed = |(c, r): (i32, i32)| occ[(r * arena.cols + c) as usize] >= costs.building;
    assert!(boxed(flying), "flying target: the goal is the nearest in-reach cell, inside the box: {flying:?}");
    assert!(!boxed(ground), "ground target: the box demotion keeps the goal outside: {ground:?}");
    let d = |(c, r): (i32, i32)| path16402::dist2_capped(c * 500 + 250, r * 500 + 250, mover.0, mover.1);
    assert!(d(flying) < d(ground), "the boxed cell is the nearer one");
    // and through the request: only the target's flag flips it
    let calib = Calib::shipped();
    let obstacles = vec![Obstacle {
        id: EntityId { index: 0, generation: 0 },
        shape: royalesim::arena::Shape::Circle { c: Vec2::new(sub(building.0), sub(building.1)), r: sub(box_r) },
        radius: sub(box_r),
        key: (0, 0, 0, 0, 0),
        ally: false,
    }];
    let world = FrameWorld { arena: &arena, obstacles: &obstacles };
    let req = |target_flying: bool| NavRequest {
        #[cfg(clash_plant = "reflection_bridge_tie")]
        red: false,
        team: Team::Blue,
        pos: Vec2::new(sub(mover.0), sub(mover.1)),
        goal: Vec2::new(sub(target.0), sub(target.1)),
        radius: 0,
        sight: 0,
        step: 0,
        reach: sub(reach),
        flying: false,
        target_flying,
        jumper: false,
        ignore: None,
    };
    let (to_flying, ok1) = path2026::plan_cells(&world, &calib, &req(true));
    let (to_ground, ok2) = path2026::plan_cells(&world, &calib, &req(false));
    assert!(ok1 && ok2);
    assert_eq!(to_flying[0], flying);
    assert_eq!(to_ground[0], ground);
    assert_ne!(to_flying[0], to_ground[0]);
}

// ---------------------------------------------------------------------------
// 6. the bridge walk

#[test]
fn a_jumper_whose_next_node_is_a_bridge_cell_walks_across() {
    // the first Royal Hog reached the bridge column before the river and never
    // entered state 5; here the Hog Rider, placed on the right bridge's column, does
    // the same through the engine
    let fx = fixture();
    let arena = Arena::shipped();
    let rh = fx.cases.iter().find(|c| c.name == ROYAL_HOG_BRIDGE).unwrap();
    assert!(rh.hop_tick.is_none() && rh.frames.iter().all(|f| f.state != 5));
    let bridge_col = rh.frames.iter().flat_map(|f| f.nodes.iter()).find(|n| (30..=33).contains(&n[1])).unwrap()[0];
    let c = fx.cases.iter().find(|c| c.name == HOG).unwrap();
    let mut s = BattleState::new(3, config_for(c));
    let x = bridge_col * 500 + 250;
    let id = s.scenario_spawn_now(Team::Blue, &c.card, Vec2::new(sub(x), sub(c.spawn_native[1])), None).unwrap();
    let mut crossed = false;
    for _ in 0..400 {
        s.tick();
        let e = s.entity(id).unwrap();
        assert!(!e.jumping, "a bridge walker never leaps");
        let (_, row) = arena.subtile_to_half(e.pos);
        if row > 33 {
            crossed = true;
            break;
        }
    }
    assert!(crossed, "the Hog walked the bridge");
}

// ---------------------------------------------------------------------------
// 7. determinism, save / load mid-leap

#[test]
fn the_leap_is_deterministic_and_survives_a_save_mid_air() {
    let fx = fixture();
    let c = fx.cases.iter().find(|c| c.name == HOG).unwrap();
    let (mut s, id) = spawn_and_walk(c);
    let mut twin = s.clone();
    for _ in 0..200 {
        s.tick();
        twin.tick();
        assert_eq!(s.state_hash(), twin.state_hash());
        if s.entity(id).unwrap().jumping {
            break;
        }
    }
    assert!(s.entity(id).unwrap().jumping, "vacuous: the Hog never leapt");
    for _ in 0..5 {
        s.tick();
    }
    assert!(s.entity(id).unwrap().jumping);
    let bytes = s.save();
    let mut loaded = BattleState::load_with(&bytes, std::sync::Arc::new(config_for(c).cards.as_ref().clone()), Arena::shipped()).expect("a mid-leap snapshot loads");
    assert!(loaded.entity(id).unwrap().jumping, "the snapshot carries the leap");
    for _ in 0..60 {
        s.tick();
        loaded.tick();
        assert_eq!(s.state_hash(), loaded.state_hash());
    }
    assert!(!s.entity(id).unwrap().jumping, "landed by now");
}

// ---------------------------------------------------------------------------
// 8. the foil

#[test]
fn the_walk_priced_water_foil_walks_the_river_at_speed() {
    let fx = fixture();
    let c = fx.cases.iter().find(|c| c.name == HOG).unwrap();
    let arena = Arena::shipped();
    let mut cfg = config_for(c);
    cfg.calib.jump_water_hop = JumpWaterHop::WalkPricedWater;
    let mut s = BattleState::new(11, cfg);
    let pos = Vec2::new(sub(c.spawn_native[0]), sub(c.spawn_native[1]));
    let id = s.scenario_spawn_now(Team::Blue, &c.card, pos, None).unwrap();
    let mut wet_ticks = 0;
    let mut max_step = 0;
    let mut prev = s.entity(id).unwrap().pos;
    for _ in 0..400 {
        s.tick();
        let e = s.entity(id).unwrap();
        assert!(!e.jumping, "the foil never leaps");
        if arena.is_water(e.pos) {
            wet_ticks += 1;
        }
        max_step = max_step.max(e.pos.sub(prev).len() / K);
        prev = e.pos;
        let (_, row) = arena.subtile_to_half(e.pos);
        if row > 33 {
            break;
        }
    }
    let speed = stat(c, "Speed");
    assert!(wet_ticks >= 10, "the foil walks through the water cells: {wet_ticks} wet ticks");
    assert!(max_step <= speed, "at its Speed, never JumpSpeed: max step {max_step}");
}

#[test]
fn the_shipped_cards_json_carries_the_jump_blocks_and_the_shipped_hog_leaps() {
    // (9). No fixture row written over the card here: the loader's own view of
    // data/derived/cards.json (tools/extract_cards.py --vintage 15.535.29). The
    // fixture's 15.535 Hog Rider row is the reference the block must equal.
    let db: CardDb = cards();
    let fx = fixture();
    let c = fx.cases.iter().find(|c| c.name == HOG).unwrap();
    for name in ["HogRider", "Prince", "DarkPrince"] {
        let idx = db.index(name).unwrap_or_else(|| panic!("{name} is not simulable: {:?}", db.rejected));
        let jump = db.get(idx).jump.unwrap_or_else(|| panic!("the shipped cards.json {name} has no `jump` block: regenerate it (tools/extract_cards.py --vintage 15.535.29)"));
        assert!(jump.speed > 0 && jump.height_raw > 0, "{name}: {jump:?}");
    }
    let hog = db.get(db.index("HogRider").unwrap()).jump.unwrap();
    assert_eq!(hog, JumpDef { speed: stat(c, "JumpSpeed"), height_raw: stat(c, "JumpHeight") }, "the shipped Hog Rider's block against the 15.535 row of the fixture");
    // And the shipped card leaps: from the fixture's placement (the river centre), with
    // nothing written over it.
    let mut s = BattleState::new(3, BattleConfig::with_cards(db));
    let id = s.scenario_spawn_now(Team::Blue, "HogRider", Vec2::new(sub(c.spawn_native[0]), sub(c.spawn_native[1])), None).unwrap();
    let mut leapt = false;
    for _ in 0..200 {
        s.tick();
        if s.entity(id).unwrap().jumping {
            leapt = true;
            break;
        }
    }
    assert!(leapt, "the shipped Hog Rider never entered state 5 from the river centre");
}
