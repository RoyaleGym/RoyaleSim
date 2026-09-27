//! THE UNDERGROUND WALK (movement.SPAWN_PATHFIND_STATES; state.rs `phase_tunnel`, `tunnel_step`, `surface`): the Miner
//! and the Goblin Drill.
//!
//! THE LAW, measured on client 16.402 (capture 20260920-083112, both seats) and on client 15.535.29 (the Miner and
//! Goblin Drill tunnel runs):
//!   - the play creates ONE unit on its owner's King centre, which walks under ground at SpawnPathfindSpeed (the
//!     Miner 650, the Drill's dig 300) toward the cell that holds its destination;
//!   - nothing targets it or hits it on the way;
//!   - the Miner comes up as itself on its tap's tile centre and deploys there as an ordinary single troop: 19
//!     deploying frames, S..S + 18;
//!   - the Drill's dig leaves its BUILDING on the frame after its last one, on the building's 2x2 footprint
//!     ((3500, 23500) -> (3000, 23000)), at 1307 of 1313 hitpoints at level 11 (one drain step at its creation), and
//!     that building deploys 20 frames, drains from F + 21 and is gone on F + 220, leaving two Goblins at +-(500, 0);
//!   - its GoblinDrillDamage lands on the building's first frame: 84 at level 11;
//!   - a tunnelling card goes down on the enemy side, never on water.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads both walks, and the Drill's building from its `units` row, not its card row;
//!   2. the Miner: born on its King, two steps out on its first frame, every step under ground a full 650, and up on
//!      the tile centre it was played at, the same entity, deploying 19 frames;
//!   3. the creation step: the first frame is more than one step out under king_centre_step_at_creation, at most one
//!      under king_centre_no_creation_step;
//!   4. no enemy targets a Miner under ground within a princess tower's reach, and a Zap on it takes nothing;
//!   5. the Miner and the Drill go down on the enemy side and behind the enemy King, not on water; a Knight does not;
//!   6. the Drill's destination is its building's footprint, and an own King-box tap goes to the forward of two
//!      equally near places, on both sides;
//!   7. the dig's route is the flat-cost search to its destination's cell, not a walker's;
//!   8. the dig and its building never share a frame; the building stands on the destination at 1307 / 1313;
//!   9. the building deploys 20 frames at 1307, loses its first drain step on F + 21, is gone on F + 220 and leaves its
//!      two Goblins on the x axis;
//!  10. GoblinDrillDamage takes 84 from a Knight within its reach on the building's first frame;
//!  11. a tunneller's destination is state, and a save mid-walk replays the walk;
//!  12. under not_modelled a tunnelling card is refused at the deck and at every play.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test spawn_pathfind`):
//!   * `tunnel_at_card_speed` -- the walk at the card's walking speed: (2) and (3) go red.
//!   * `tunnel_targetable` -- a unit under ground is an ordinary troop to the board: (4) goes red.
//!   * `tunnel_territory_own_half` -- a tunnelling card keeps its kind's territory: (5) goes red.
//!   * `drill_footprint_from_dig` -- the Drill is placed on its dig's 0-radius footprint: (6) and (8) go red.
//!   * `tunnel_walker_costs` -- the dig's route priced as a walker's, lane bonus included: (7) goes red.
//!   * `drill_surfaces_as_dig` -- the dig comes up as itself, no building: (8), (9) and (10) go red.
//!   * `morph_birth_full_hp` -- the building comes up at full hitpoints: (8) and (9) go red.
//!   * `death_ring_facing` -- the listed units keep the facing ring: (9) goes red.
//!   * `hash_skips_tunnel` -- the destination is not hashed: (11) goes red.
//!
//! OPEN, not pinned: the first step's geometry (the engine's first frame is not the measured one; ledger
//! movement.SPAWN_PATHFIND_START), which of the Drill's two Goblins takes -x, the Drill's periodic Goblin timing, and
//! the push GoblinDrillDamage gives.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::CardKind;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::path16402::{self, PathFinder, CELL};
use royalesim::path2026;
use royalesim::state::{BattleConfig, BattleState, DeployError, SpawnPathfind, SpawnPathfindStart};
use royalesim::{EntityId, Team};

/// Native units to a world point.
fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Native distance between two world points.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// Both hands hold the Miner and the Goblin Drill (the deck's first four, unshuffled).
const DECK: [&str; 8] = ["Miner", "GoblinDrill", "Knight", "Archer", "Giant", "Minions", "Musketeer", "Valkyrie"];

/// Every card and tower at level 11, the level the measurements were taken at.
fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

/// A battle past the opening lockout with both hands holding the two tunnelling cards and full elixir.
fn board_with(cfg: BattleConfig) -> BattleState {
    let mut cfg = level11(cfg);
    let deck: Vec<String> = DECK.iter().map(|s| s.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.shuffle_decks = false;
    let mut s = BattleState::new(5, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    assert_eq!(&s.hand(Team::Blue)[..2], &["Miner", "GoblinDrill"], "the scene drifted: the hand");
    s
}

fn board() -> BattleState {
    board_with(config())
}

/// One live unit read after a tick.
#[derive(Clone, Copy, Debug)]
struct Snap {
    tick: u32,
    id: EntityId,
    pos: Vec2,
    dest: Option<Vec2>,
    hp: i32,
    max_hp: i32,
    deploying: bool,
}

/// The one live `team` unit of `card`, if any.
fn snap(s: &BattleState, team: Team, card: &str) -> Option<Snap> {
    let v = find_live(s, team, card);
    assert!(v.len() <= 1, "more than one {card}");
    v.first().map(|e| Snap { tick: s.tick_count(), id: e.id, pos: e.pos, dest: e.tunnel_dest, hp: e.hp, max_hp: e.max_hp, deploying: e.deploying })
}

/// Tick `ticks` times, reading the one `team` `card` after each tick it exists.
fn follow(s: &mut BattleState, team: Team, card: &str, ticks: u32) -> Vec<Snap> {
    let mut out = Vec::new();
    for _ in 0..ticks {
        s.tick();
        if let Some(f) = snap(s, team, card) {
            out.push(f);
        }
    }
    out
}

/// The record name of the building the Goblin Drill's dig leaves (a summon-only record; its name is taken by the
/// card, whose row is the dig).
fn drill_building(s: &BattleState) -> String {
    let m = card_stat(s, "GoblinDrill").spawn_pathfind.and_then(|p| p.morph).expect("the dig morphs");
    s.cards().get(m).name.clone()
}

/// The red princess tower nearest `p`: (its id, its centre).
fn red_princess_near(s: &BattleState, p: Vec2) -> (EntityId, Vec2) {
    s.entities().filter(|v| v.team == Team::Red && v.card == "PrincessTower").map(|v| (v.id, v.pos)).min_by_key(|v| dist(v.1, p)).expect("a red princess tower")
}

// ---------------------------------------------------------------------------
// (1)

#[test]
fn the_loader_reads_both_walks_and_the_drills_building_from_its_units_row() {
    let s = BattleState::new(0, config());
    let miner = card_stat(&s, "Miner");
    let sp = miner.spawn_pathfind.expect("the Miner tunnels");
    assert_eq!((sp.speed, sp.morph), (650, None), "SpawnPathfindSpeed, no morph");
    assert!(miner.can_deploy_on_enemy_side, "CanDeployOnEnemySide");
    let drill = card_stat(&s, "GoblinDrill");
    let sp = drill.spawn_pathfind.expect("the Goblin Drill's dig tunnels");
    assert_eq!(sp.speed, 300, "the dig's SpawnPathfindSpeed");
    assert!(drill.can_deploy_on_enemy_side, "CanDeployOnEnemySide");
    let m = sp.morph.expect("the dig morphs into its building");
    let b = s.cards().get(m);
    // the building is the `units` row (513 hp, radius 500, a LifeTime), never the card row of the same name, which
    // carries the dig (1000 hp, radius 0)
    assert_eq!((b.kind, b.unit_name.as_str(), b.hitpoints, b.summon_only), (CardKind::Building, "GoblinDrill", 513, true), "the building record");
    assert_ne!(b.hitpoints, drill.hitpoints, "vacuous: the dig and the building carry one hitpoint figure");
    assert!(b.spawn_area_effect.is_some(), "its SpawnAreaObject, GoblinDrillDamage");
    assert!(b.spawner.is_some(), "its periodic Goblin");
    let ds = b.death_spawn.expect("its two death Goblins");
    assert_eq!(ds.count, 2);
}

// ---------------------------------------------------------------------------
// (2)

/// Plant: tunnel_at_card_speed.
#[test]
fn a_miner_walks_under_ground_at_650_and_comes_up_on_the_tile_centre_it_was_played_at() {
    let mut s = board();
    let king = s.config().arena.king_tower_pos(Team::Blue);
    let dest = s.deploy(Team::Blue, "Miner", at((3500, 1500))).expect("play the Miner");
    assert_eq!(dest, at((3500, 1500)), "the tap resolves to its own tile centre, with no single-troop x - 1");
    let frames = follow(&mut s, Team::Blue, "Miner", 60);
    let first = *frames.first().expect("the Miner never appeared");
    assert_eq!(first.dest, Some(dest), "born under ground, bound for the tap");
    assert!(frames.iter().all(|f| f.id == first.id), "the Miner that comes up is not the one that went down");
    // born on its King, two steps out on its first frame
    let d0 = dist(first.pos, king);
    assert!(d0 > 650 && d0 <= 1300, "the first frame stands {d0} from the King: not two tunnel steps out");
    // every step under ground is a full SpawnPathfindSpeed: each aims past the nodes it has reached
    let under: Vec<Snap> = frames.iter().copied().take_while(|f| f.dest.is_some()).collect();
    let steps: Vec<i64> = under.windows(2).map(|w| dist(w[0].pos, w[1].pos)).collect();
    assert!(steps.len() >= 3, "the scene drifted: the walk is {} frames", under.len());
    assert!(steps.iter().all(|&d| (640..=650).contains(&d)), "a step under ground is not a full 650: {steps:?}");
    assert!(under.iter().all(|f| f.hp == f.max_hp), "the Miner lost hitpoints under ground");
    // up on the tap, the same entity, an ordinary single-troop deploy from there
    let up = frames.iter().position(|f| f.dest.is_none()).expect("the Miner never came up");
    assert_eq!(frames[up].tick, frames[up - 1].tick + 1, "a frame is missing around the surfacing");
    assert_eq!(frames[up].pos, dest, "the Miner came up off its destination");
    assert_eq!(card_stat(&s, "Miner").deploy_time_ms, 1000, "the scene drifted: the Miner's DeployTime");
    let deploying: Vec<bool> = frames[up..up + 20].iter().map(|f| f.deploying).collect();
    let mut want = vec![true; 19];
    want.push(false);
    assert_eq!(deploying, want, "deploying on S..S + 18 and not on S + 19 (client 15.535.29, both halves)");
    assert!(frames[up..up + 19].iter().all(|f| f.pos == dest), "the Miner moved while it deployed");
}

// ---------------------------------------------------------------------------
// (3)

/// Plant: tunnel_at_card_speed.
#[test]
fn the_creation_step_puts_the_first_frame_more_than_one_step_out() {
    let first = |start: SpawnPathfindStart| -> i64 {
        let mut cfg = config();
        cfg.calib.spawn_pathfind_start = start;
        let mut s = board_with(cfg);
        let king = s.config().arena.king_tower_pos(Team::Blue);
        s.deploy(Team::Blue, "Miner", at((3500, 1500))).expect("play the Miner");
        let f = follow(&mut s, Team::Blue, "Miner", 3);
        let f = f.first().expect("the Miner appeared");
        assert!(f.dest.is_some(), "the scene drifted: the Miner is up on its first frame");
        dist(f.pos, king)
    };
    let with = first(SpawnPathfindStart::KingCentreStepAtCreation);
    let without = first(SpawnPathfindStart::KingCentreNoCreationStep);
    assert!(without <= 650, "with no creation step the first frame is one step out at most, not {without}");
    assert!(with > 650 && with <= 1300, "with the creation step the first frame is two steps out, not {with}");
}

// ---------------------------------------------------------------------------
// (4)

/// Plant: tunnel_targetable.
#[test]
fn nothing_targets_or_hits_a_miner_under_ground() {
    let mut s = board();
    let dest = s.deploy(Team::Blue, "Miner", at((3500, 21500))).expect("the Miner goes down on the enemy side");
    let (_, tower) = red_princess_near(&s, dest);
    let (mut under, mut in_reach, mut zapped) = (0, 0, false);
    for _ in 0..80 {
        s.tick();
        let Some(m) = snap(&s, Team::Blue, "Miner") else { continue };
        if m.dest.is_none() {
            break;
        }
        under += 1;
        assert_eq!(m.hp, m.max_hp, "the Miner lost hitpoints under ground on tick {}", m.tick);
        let by: Vec<&str> = s.entities().filter(|v| v.team == Team::Red && v.target == Some(m.id)).map(|v| v.card).collect();
        assert!(by.is_empty(), "tick {}: {by:?} target a Miner under ground", m.tick);
        if dist(m.pos, tower) <= 7000 {
            in_reach += 1;
        }
        if under == 3 {
            // a Zap where it stands: it lands next tick, one step (650) on, well inside the Zap's radius
            s.spawn_unit(Team::Red, "Zap", m.pos, None).expect("cast Zap on the Miner");
            zapped = true;
        }
    }
    assert!(zapped && under > 4, "the scene drifted: the walk is {under} frames");
    assert!(in_reach >= 2, "the scene drifted: the Miner was under ground within a princess tower's reach on {in_reach} frames");
}

// ---------------------------------------------------------------------------
// (5)

/// Plant: tunnel_territory_own_half.
#[test]
fn a_tunnelling_card_goes_down_on_the_enemy_side_but_not_on_water() {
    let s = board();
    for (card, p) in [("Miner", (3500, 21500)), ("GoblinDrill", (3500, 23500)), ("Miner", (9500, 31500)), ("Miner", (3500, 16500))] {
        s.check_deploy(Team::Blue, card, at(p)).unwrap_or_else(|e| panic!("{card} at {p:?} (enemy side, behind the enemy King, the bridge) refused: {e:?}"));
    }
    let water = at((9500, 16500));
    assert!(!s.config().arena.is_passable_ground(water), "the scene drifted: (9500, 16500) is land");
    for card in ["Miner", "GoblinDrill"] {
        assert!(s.check_deploy(Team::Blue, card, water).is_err(), "{card} goes down on water");
    }
    // a Knight keeps its own territory: the tunnelling rule is the card's, not the side's
    assert!(s.check_deploy(Team::Blue, "Knight", at((3500, 21500))).is_err(), "the scene drifted: a Knight goes down on the enemy side");
}

// ---------------------------------------------------------------------------
// (6)

/// Plant: drill_footprint_from_dig.
#[test]
fn the_drills_destination_is_its_buildings_footprint() {
    let s = board();
    let idx = s.cards().index("GoblinDrill").expect("the Goblin Drill loads");
    // client 16.402: the building's 2x2 box snapped to a tile corner, where the dig's own 0 radius would keep the tap
    assert_eq!(s.resolve_point(Team::Blue, idx, at((3500, 23500))), at((3000, 23000)), "an enemy-side tap");
    // an own King-box tap is moved off the box; (6000, 2000) and (6000, 1000) are equally near and the forward one
    // wins in the placer's frame (client 16.402 for side 0, client 15.535.29 for side 1)
    assert_eq!(s.resolve_point(Team::Blue, idx, at((8500, 1500))), at((6000, 2000)), "side 0's King-box tap");
    assert_eq!(s.resolve_point(Team::Red, idx, at((8500, 30500))), at((6000, 30000)), "side 1's King-box tap");
}

// ---------------------------------------------------------------------------
// (7)

/// Plant: tunnel_walker_costs.
#[test]
fn the_digs_route_is_the_flat_cost_search_to_its_destinations_cell() {
    let mut s = board();
    let dest = s.deploy(Team::Blue, "GoblinDrill", at((3500, 23500))).expect("play the Drill");
    let arena = s.config().arena.clone();
    let calib = s.config().calib.clone();
    let king = arena.king_tower_pos(Team::Blue);
    let cell = |p: Vec2| ((p.x / K).div_euclid(CELL), (p.y / K).div_euclid(CELL));
    let (start, goal) = (cell(king), cell(dest));
    assert_eq!(goal, (6, 46), "the goal is the cell holding the destination (client 16.402: node 1662)");
    // the two searches over the same grid: the spawn walk's prices, and a walker's (the lane bonus on the roads)
    let costs = path2026::costs16402(&calib);
    let terrain = path2026::terrain16402(&arena, &costs);
    // a fresh finder for each search, as the dig's first plan is the battle's first spawn-walk search
    let flat = PathFinder::new(arena.cols, arena.rows, costs.heuristic)
        .find_path(start.0, start.1, goal.0, goal.1, true, &|c, r| path16402::cell_cost_spawn_pathfind(&terrain, c, r, costs.default))
        .to_vec();
    let occ = vec![0; (arena.cols * arena.rows) as usize];
    let lane = PathFinder::new(arena.cols, arena.rows, costs.heuristic).find_path(start.0, start.1, goal.0, goal.1, true, &|c, r| path16402::cell_cost_for(&terrain, &occ, c, r, false)).to_vec();
    let keep = flat.len().saturating_sub(4);
    assert!(keep > 20, "the scene drifted: a route of {} cells", flat.len());
    assert!(lane.get(..keep) != flat.get(..keep), "the scene cannot see the plant: the walker's route is the flat one up to its first cells");
    s.tick();
    let v = find_live(&s, Team::Blue, "GoblinDrill").first().map(|v| (v.route.to_vec(), v.tunnel_dest)).expect("the dig appeared");
    assert_eq!(v.1, Some(dest), "the dig is bound for the destination");
    let cols = arena.cols;
    let got: Vec<i32> = v.0.iter().map(|&p| cell(p)).map(|(c, r)| r * cols + c).collect();
    assert!(got.len() >= keep, "the dig dropped {} of its {} route cells on its first frame", flat.len() - got.len(), flat.len());
    assert_eq!(got[..], flat[..got.len()], "the dig's route (goal first) is not the flat-cost search's");
}

// ---------------------------------------------------------------------------
// (8)

/// Plants: drill_surfaces_as_dig, morph_birth_full_hp, drill_footprint_from_dig.
#[test]
fn the_dig_leaves_its_building_on_the_destination_the_frame_after_its_last() {
    let mut s = board();
    let dest = s.deploy(Team::Blue, "GoblinDrill", at((3500, 23500))).expect("play the Drill");
    let bname = drill_building(&s);
    let (mut dig_last, mut building) = (None, None);
    for _ in 0..200 {
        s.tick();
        let dig = snap(&s, Team::Blue, "GoblinDrill");
        let b = snap(&s, Team::Blue, &bname);
        assert!(!(dig.is_some() && b.is_some()), "the dig and its building share frame {}", s.tick_count());
        if let Some(d) = dig {
            assert!(d.dest.is_some(), "the dig stands above ground on {}", d.tick);
            dig_last = Some(d.tick);
        }
        if b.is_some() {
            building = b;
            break;
        }
    }
    let b = building.expect("the building never came up");
    let dig_last = dig_last.expect("the dig never walked");
    assert_eq!(b.tick, dig_last + 1, "the building appears on the dig's last frame + 1");
    assert_eq!(b.pos, dest, "the building stands off the destination");
    assert_eq!(b.dest, None, "the building is not under ground");
    // 513 x 256% at level 11, one drain step ((1313 x 100000 / 10000) / 20 = 656 hundredths, 6 whole) at its creation
    assert_eq!((b.max_hp, b.hp), (1313, 1307), "the building's first frame (client 16.402, 3 of 3)");
    assert!(b.deploying, "the building deploys from its first frame");
}

// ---------------------------------------------------------------------------
// (9)

/// Plants: drill_surfaces_as_dig, morph_birth_full_hp, death_ring_facing.
#[test]
fn the_building_deploys_20_frames_drains_from_f_plus_21_and_leaves_its_pair_on_the_x_axis() {
    let mut s = board();
    // own side, in the open, where nothing reaches it
    let dest = s.deploy(Team::Blue, "GoblinDrill", at((9000, 10000))).expect("play the Drill");
    let bname = drill_building(&s);
    let goblin = {
        let m = card_stat(&s, "GoblinDrill").spawn_pathfind.and_then(|p| p.morph).expect("the dig morphs");
        let ds = s.cards().get(m).death_spawn.expect("the building's death spawn");
        s.cards().get(ds.unit).name.clone()
    };
    let mut first = None;
    for _ in 0..120 {
        s.tick();
        if let Some(b) = snap(&s, Team::Blue, &bname) {
            first = Some(b);
            break;
        }
    }
    let b = first.expect("the building never came up");
    assert_eq!(b.pos, dest, "the building stands off the destination");
    let mut life = vec![b];
    let mut goblins_before: Vec<EntityId> = Vec::new();
    let mut gone = None;
    for _ in 0..260 {
        goblins_before = find_live(&s, Team::Blue, &goblin).iter().map(|v| v.id).collect();
        s.tick();
        match s.entity(b.id) {
            Some(v) => life.push(Snap { tick: s.tick_count(), id: v.id, pos: v.pos, dest: v.tunnel_dest, hp: v.hp, max_hp: v.max_hp, deploying: v.deploying }),
            None => {
                gone = Some(s.tick_count());
                break;
            }
        }
    }
    let gone = gone.expect("the building outlived its LifeTime");
    let deploying = life.iter().take_while(|f| f.deploying).count();
    assert_eq!(deploying, 20, "deploying frames F..F + 19 (client 16.402, 3 of 3)");
    let hp: Vec<i32> = life[..=21].iter().map(|f| f.hp).collect();
    assert!(hp[..=20].iter().all(|&h| h == 1307), "the building's hp moved before F + 21: {hp:?}");
    assert!(hp[21] < 1307, "no drain step on F + 21: {hp:?}");
    assert_eq!(gone, b.tick + 220, "gone on F + 220 (client 16.402, one full life)");
    // the pair: new Goblins on the vanishing frame, on the x axis at +-500 (a first update may nudge them a little);
    // a wave Goblin comes out at +1000 on y and is not one of them
    let death_point = life.last().expect("a frame").pos;
    let pair: Vec<(i64, i64)> = find_live(&s, Team::Blue, &goblin)
        .iter()
        .filter(|v| !goblins_before.contains(&v.id))
        .map(|v| (((v.pos.x - death_point.x) / K) as i64, ((v.pos.y - death_point.y) / K) as i64))
        .filter(|&(_, dy)| dy.abs() <= 200)
        .collect();
    assert_eq!(pair.len(), 2, "not two new Goblins on the x axis of the death point: {pair:?}");
    let mut xs: Vec<i64> = pair.iter().map(|p| p.0).collect();
    xs.sort();
    assert!((-700..=-300).contains(&xs[0]) && (300..=700).contains(&xs[1]), "the pair is not at +-500 on x: {pair:?}");
}

// ---------------------------------------------------------------------------
// (10)

/// Plant: drill_surfaces_as_dig.
#[test]
fn goblin_drill_damage_lands_on_the_buildings_first_frame() {
    // run 1: the building's first frame F
    let first_frame = || -> u32 {
        let mut s = board();
        s.deploy(Team::Blue, "GoblinDrill", at((3500, 23500))).expect("play the Drill");
        let bname = drill_building(&s);
        for _ in 0..200 {
            s.tick();
            if snap(&s, Team::Blue, &bname).is_some() {
                return s.tick_count();
            }
        }
        panic!("the building never came up");
    };
    let f = first_frame();
    // run 2, the same battle: a red Knight put down on F - 5 beside the destination, still deploying on F, where the
    // dig cannot see it (it is under ground and untouchable)
    let mut s = board();
    let dest = s.deploy(Team::Blue, "GoblinDrill", at((3500, 23500))).expect("play the Drill");
    while s.tick_count() < f - 5 {
        s.tick();
    }
    s.spawn_unit(Team::Red, "Knight", at((4500, 22000)), None).expect("put the Knight down");
    let mut hp = Vec::new();
    while s.tick_count() < f {
        s.tick();
        let k = snap(&s, Team::Red, "Knight").expect("the Knight stands");
        hp.push((s.tick_count(), k.hp, k.max_hp, dist(k.pos, dest)));
    }
    let (t, h, max, d) = *hp.last().expect("a frame");
    assert_eq!(t, f, "the scene drifted: the building's first frame");
    assert!(snap(&s, Team::Blue, &drill_building(&s)).is_some(), "the scene drifted: the building is not up on F");
    let (_, before, _, _) = hp[hp.len() - 2];
    assert_eq!(before, max, "the scene drifted: the Knight was hit before F");
    assert!(d <= 2000, "the scene drifted: the Knight stands {d} from the destination");
    // Damage 33 at the building's level, 11: floor(33 x 256%) = 84 (client 15.535.29; 58 at level 7)
    assert_eq!(max - h, 84, "GoblinDrillDamage on the building's first frame");
}

// ---------------------------------------------------------------------------
// (11)

/// Plant: hash_skips_tunnel.
#[test]
fn a_tunnellers_destination_is_state() {
    let mut s = board();
    s.deploy(Team::Blue, "Miner", at((3500, 21500))).expect("play the Miner");
    for _ in 0..4 {
        s.tick();
    }
    let m = snap(&s, Team::Blue, "Miner").expect("the Miner walks");
    assert!(m.dest.is_some(), "the scene drifted: the Miner is up already");
    let bytes = s.save();
    let mut v: serde_json::Value = serde_json::from_slice(&bytes).expect("a snapshot is JSON");
    let col = v["ents"]["tunnel_dest"].as_array_mut().expect("the snapshot carries the destination");
    let d = &mut col[m.id.index as usize];
    let x = d["x"].as_i64().expect("the destination is a point");
    d["x"] = serde_json::Value::from(x + 1000 * K as i64);
    let edited = serde_json::to_vec(&v).unwrap();
    let a = BattleState::load(&bytes).expect("the save loads");
    let b = BattleState::load(&edited).expect("the edited save loads");
    assert_ne!(a.state_hash(), b.state_hash(), "two states differing only in a tunneller's destination hash alike");
}

#[test]
fn a_save_mid_walk_replays_the_walk() {
    let mut s = board();
    s.deploy(Team::Blue, "GoblinDrill", at((3500, 23500))).expect("play the Drill");
    for _ in 0..10 {
        s.tick();
    }
    assert!(snap(&s, Team::Blue, "GoblinDrill").is_some_and(|d| d.dest.is_some()), "the scene drifted: the dig is not walking");
    let mut t = BattleState::load(&s.save()).expect("the save loads");
    let bname = drill_building(&s);
    for _ in 0..120 {
        s.tick();
        t.tick();
        assert_eq!(s.state_hash(), t.state_hash(), "the loaded battle parts from the saved one on tick {}", s.tick_count());
    }
    assert!(snap(&s, Team::Blue, &bname).is_some(), "the scene drifted: the building never came up");
}

// ---------------------------------------------------------------------------
// (12)

#[test]
fn not_modelled_refuses_a_tunnelling_card_at_the_deck_and_at_every_play() {
    let mut cfg = level11(config());
    cfg.calib.spawn_pathfind = SpawnPathfind::NotModelled;
    let mut decked = cfg.clone();
    let deck: Vec<String> = DECK.iter().map(|s| s.to_string()).collect();
    decked.decks = [deck.clone(), deck];
    match BattleState::try_new(0, decked) {
        Ok(_) => panic!("a deck holding the Miner was accepted under not_modelled"),
        Err(e) => assert!(e.contains("travels underground"), "{e}"),
    }
    let mut s = BattleState::new(0, cfg);
    for card in ["Miner", "GoblinDrill"] {
        let got = s.spawn_unit(Team::Blue, card, at((3500, 1500)), None);
        assert!(matches!(got, Err(DeployError::UnsupportedCard(..))), "{card} under not_modelled: {got:?}");
    }
}
