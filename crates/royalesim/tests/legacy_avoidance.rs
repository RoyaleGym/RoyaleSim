//! THE PRE-2026 PATH MODELS' AVOIDANCE LEAVES OUT WHAT NOTHING MEETS (PathModel::LaneSnap, GridAStar and
//! DiagonalLookahead; state.rs `phase_path`, path.rs `avoid_units`).
//!
//! The three models steer a walker around a troop ahead of it (of its own air or ground). They took every such troop
//! near the walker as a blocker, including a unit under ground (movement.SPAWN_PATHFIND_BODY = untouchable: no unit
//! meets it; entity.rs `underground`) and an attached rider (it stands on its mount and pushes nothing; entity.rs
//! `attached`). The shipped move pass and the contact law leave both out, and now so does the avoidance.
//!
//! WHAT IS PINNED, under each of the three models:
//!   1. `a_legacy_walker_does_not_step_around_a_miner_under_ground`: a Knight walks up x 9250 at a Cannon while a Red
//!      Miner tunnels down the same line at it. It takes the same steps as with no Miner. Before, it stepped aside
//!      from tick 15.
//!   2. `a_legacy_walker_does_not_step_around_an_attached_rider`: a Balloon flies past a deploying Goblin Giant,
//!      through the reach of the two Spear Goblins on its back, which fly. It takes the same steps as past a deploying
//!      Giant, which carries no rider. Before, it turned away from the riders on its first step. Taking the riders off
//!      the board would be no control: a rider that dies leaves an ordinary Spear Goblin where it stood.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test legacy_avoidance`):
//!   * `legacy_walk_avoids_untouchable` -- the avoidance keeps a unit under ground and an attached rider as
//!     blockers: (1) and (2) red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, SpawnPathfindBody};
use royalesim::{PathModel, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Native distance between two world points.
fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// Every card and tower at level 11.
fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

/// The shipped arm of the key the scenes stand on, asserted.
fn shipped() -> BattleConfig {
    let cfg = level11(config());
    assert_eq!(cfg.calib.spawn_pathfind_body, SpawnPathfindBody::Untouchable, "the shipped movement.SPAWN_PATHFIND_BODY");
    cfg
}

/// A card's collision radius, native.
fn radius(s: &BattleState, card: &str) -> i64 {
    (card_stat(s, card).collision_radius / K) as i64
}

/// A Red Miner's tap on the Blue side. Its walk from the Red King runs straight down x 9249 to y 21601, 650 a tick,
/// before it turns for the bridge.
const MINER_TAP: (i32, i32) = (9000, 9500);

// ---------------------------------------------------------------------------
// (1) a Miner under ground

const WALKER_AT: (i32, i32) = (9250, 20000);
/// The walker's target, straight ahead up the Miner's line and inside its sight.
const WALK_TARGET_AT: (i32, i32) = (9250, 25000);
/// The Miner is played before this tick; played before any of ticks 0 to 15 it met the walker head-on.
const MINER_MEETS: u32 = 5;
const WALK_TICKS: u32 = 60;

/// The walker's position after each tick under `model`, and the closest a Miner under ground came to it.
fn walk(model: PathModel, miner_played: Option<u32>) -> (Vec<Vec2>, Option<i64>) {
    let mut cfg = shipped();
    cfg.path_model = model;
    let mut s = BattleState::new(0, cfg);
    let walker = s.scenario_spawn_now(Team::Blue, "Knight", at(WALKER_AT), None).expect("place the Knight");
    s.scenario_spawn_now(Team::Red, "Cannon", at(WALK_TARGET_AT), None).expect("place the Cannon");
    let (mut steps, mut closest) = (Vec::new(), None::<i64>);
    for t in 0..WALK_TICKS {
        if miner_played == Some(t) {
            s.spawn_unit(Team::Red, "Miner", at(MINER_TAP), None).expect("play the Miner");
        }
        s.tick();
        let w = s.entity(walker).expect("the scene drifted: the Knight died").pos;
        steps.push(w);
        for m in find_live(&s, Team::Red, "Miner").iter().filter(|m| m.tunnel_dest.is_some()) {
            let d = dist(m.pos, w);
            closest = Some(closest.map_or(d, |c| c.min(d)));
        }
    }
    (steps, closest)
}

/// Plant: legacy_walk_avoids_untouchable.
#[test]
fn a_legacy_walker_does_not_step_around_a_miner_under_ground() {
    for model in [PathModel::LaneSnap, PathModel::GridAStar, PathModel::DiagonalLookahead] {
        let (alone, _) = walk(model, None);
        let (with, closest) = walk(model, Some(MINER_MEETS));
        let s = BattleState::new(0, shipped());
        let (r_knight, r_miner) = (radius(&s, "Knight"), radius(&s, "Miner"));
        let closest = closest.expect("the scene drifted: no Miner under ground");
        assert!(closest < r_knight + r_miner, "{model:?}: the scene drifted: the Miner under ground passed {closest} from the walker");
        let first = alone.iter().zip(&with).position(|(a, b)| a != b);
        assert_eq!(first, None, "{model:?}: the walker stepped around a Miner under ground: {:?}", first.map(|k| (k, alone[k], with[k])));
    }
}

// ---------------------------------------------------------------------------
// (2) an attached rider

/// A Red mount deploying on the Blue side, where it stands still through its deploy: a Goblin Giant, whose two Spear
/// Goblins ride on its back and fly, or a Giant, a troop with no rider. It faces the Blue side, so the riders sit
/// about 830 behind it.
const MOUNT_AT: (i32, i32) = (8750, 10500);
/// A Blue Balloon flies east at a Red Cannon, 1,300 past the mount's centre: through the riders' reach. A ground
/// mount is no blocker for a flier either way.
const FLIER_AT: (i32, i32) = (7300, 11800);
const FLIER_TARGET_AT: (i32, i32) = (13300, 11800);

/// The flier's position after each tick of `mount`'s deploy under `model`, and where the mount's riders stood.
fn fly_past(model: PathModel, mount: &str) -> (Vec<Vec2>, Vec<(Vec2, i32)>) {
    let mut cfg = shipped();
    cfg.path_model = model;
    let mut s = BattleState::new(0, cfg);
    s.spawn_unit(Team::Red, mount, at(MOUNT_AT), None).expect("play the mount");
    let flier = s.scenario_spawn_now(Team::Blue, "Balloon", at(FLIER_AT), None).expect("place the Balloon");
    s.scenario_spawn_now(Team::Red, "Cannon", at(FLIER_TARGET_AT), None).expect("place the Cannon");
    let (mut steps, mut riders) = (Vec::new(), Vec::new());
    loop {
        s.tick();
        let m = find_live(&s, Team::Red, mount);
        let m = m.first().expect("the mount is on the board");
        if !m.deploying {
            break;
        }
        riders = s.entities().filter(|e| e.attached_to == Some(m.id)).map(|e| (e.pos, e.radius)).collect();
        steps.push(s.entity(flier).expect("the Balloon").pos);
    }
    (steps, riders)
}

/// Plant: legacy_walk_avoids_untouchable.
#[test]
fn a_legacy_walker_does_not_step_around_an_attached_rider() {
    let s = BattleState::new(0, shipped());
    let r_flier = card_stat(&s, "Balloon").collision_radius;
    for model in [PathModel::LaneSnap, PathModel::GridAStar, PathModel::DiagonalLookahead] {
        let (alone, none) = fly_past(model, "Giant");
        let (with, riders) = fly_past(model, "GoblinGiant");
        assert!(none.is_empty() && riders.len() == 2, "{model:?}: the scene drifted: the mounts carry {} and {} riders", none.len(), riders.len());
        assert!(alone.len() >= 15 && with.len() == alone.len(), "{model:?}: the scene drifted: the deploys ran {} and {} ticks", alone.len(), with.len());
        let first = alone.iter().zip(&with).position(|(a, b)| a != b);
        assert_eq!(first, None, "{model:?}: the flier stepped around a rider: {:?}", first.map(|k| (k, alone[k], with[k])));
        // the path the flier takes past a riderless mount runs into a rider's reach (their radii summed)
        let into = alone.iter().flat_map(|w| riders.iter().map(move |(p, r)| ((r_flier + r) / K) as i64 - dist(*w, *p))).max().unwrap_or(i64::MIN);
        assert!(into > 0, "{model:?}: the scene drifted: the flier stayed {} out of a rider's reach", -into);
    }
}
