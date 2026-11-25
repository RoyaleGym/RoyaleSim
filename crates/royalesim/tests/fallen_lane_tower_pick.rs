//! targeting.FALLEN_LANE_TOWER_PICK, read off the engine: the crown tower a troop walks to once its spawn lane's enemy
//! princess tower is down (target.rs `default_tower`).
//!
//! THE LAW, measured on the 16.402 corpus: with one enemy princess tower down, a troop that picks a crown tower to walk
//! to (none in sight) takes the king when the princess tower of its SPAWN lane is the one down, wherever it stands (5 of
//! 5 such picks, all by troops standing beside the standing princess tower). The old arm takes the tower of the
//! troop's current x once targeting.FIRST_TOWER_PICK's window is over.
//!
//! THE THIRD ARM, measured on client 16.635 (parity r100, the oracle's il-mg-16635 set: the 12 troops that separate it
//! from client16402_spawn_lane_king all took their lane's tower, 259 ticks against 0): client16635_king_distance. After
//! the window, with ONE enemy princess tower standing, a troop takes it when it is its spawn lane's and its octagonal
//! length (native) squared is below its exact squared centre distance to the king (target.rs `princess_nearer_than_king`);
//! otherwise the king. Both standing: by x, as before.
//!
//! The scene is tests/test_fallen_lane_tower_pick.py's: the Red left princess tower down, a Blue Baby Dragon created at
//! (8500, 13000) (spawn lane left) chasing a Red Cannon of 60 hp at (12500, 18000) on its own half, out of every tower's
//! reach. It kills it at x 9800 or so, long after the window, and then walks to its default tower. The heading from the
//! kill to the first tick it names a target says which one. WHAT IS PINNED, each with its precondition (the kill after
//! the window, at x > W/2, and at least 20 walked ticks):
//!   1. client16402_spawn_lane_king and client16635_king_distance: it heads for the king, under both of
//!      FIRST_TOWER_PICK's spawn-lane arms (client_spawn_lane, the shipped one, and client_spawn_lane_own_frame, which
//!      agrees with it on every Blue unit);
//!   2. current_x: it heads for the right princess tower (the old arm);
//!   3. all three values: a Baby Dragon created at (9500, 13000), its spawn lane's tower standing, heads for the right
//!      princess tower (an implementation that sends every troop to the king once any princess tower is down is
//!      refused; under client16635_king_distance the right tower is nearer than the king);
//!   4. all three values: with both princess towers standing, the one created at 8500 heads for the right princess
//!      tower;
//!   5. under FIRST_TOWER_PICK = current_x, which keeps no spawn lane, the new value reads nothing: the right princess;
//!   6. the shipped value is client16635_king_distance (client16402_spawn_lane_king from parity r4 to r100);
//!   7. client16635_king_distance: with the RIGHT princess tower down, the Baby Dragon created at 8500 (spawn lane left)
//!      heads for the left princess tower from its kill at x 9800 or so (the tower's octagonal length about 12,900, the
//!      king's distance about 13,800); client16402_spawn_lane_king and current_x send it to the king (the vacuity check);
//!   8. `princess_nearer_than_king` on the measured 1-unit margins: a troop at (5068, 10196) or (4290, 14142) keeps the
//!      Red right princess tower (14500, 25500), one at (5032, 10243) or (4292, 14082) takes the king (9000, 29000); the
//!      exact distance on both sides would give the tower at the last two (the vacuity check).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test fallen_lane_tower_pick`):
//!   * `fallen_lane_by_x` -- the new value still takes the tower of the current x: (1) goes red.
//!   * `king_distance_by_x` -- client16635_king_distance falls through to the pick by x: (7) goes red (slot 0, not 1).
//!   * `king_distance_true_metric` -- `princess_nearer_than_king` reads the tower's exact distance: (8) goes red.
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, FallenLaneTowerPick, FirstTowerPick};
use royalesim::target::princess_nearer_than_king;
use royalesim::Team;

const NEW: FallenLaneTowerPick = FallenLaneTowerPick::Client16402SpawnLaneKing;
const OLD: FallenLaneTowerPick = FallenLaneTowerPick::CurrentX;
/// The parity r100 candidate (client 16.635).
const KING: FallenLaneTowerPick = FallenLaneTowerPick::Client16635KingDistance;

const FLYER_LEFT: (i32, i32) = (8500, 13000);
const FLYER_RIGHT: (i32, i32) = (9500, 13000);
const CANNON_AT: (i32, i32) = (12500, 18000);
const CANNON_HP: i32 = 60;
const HALF_WIDTH: i64 = 9000;
/// FIRST_TOWER_PICK's window, in ticks after the spawn (the scenario spawn has no deploy).
const WINDOW_TICKS: u32 = 10;
const TICKS: u32 = 140;

fn native(v: Vec2) -> (i64, i64) {
    ((v.x / K) as i64, (v.y / K) as i64)
}

/// The Red crown tower slot (0 king, 1 left, 2 right) the flyer's walk after the kill points at, the kill tick, the
/// flyer's x there and how many ticks it walked before naming a target.
fn heading(arm: FallenLaneTowerPick, flyer_at: (i32, i32), left_down: bool) -> (usize, u32, i64, usize) {
    heading_under(arm, config().calib.first_tower_pick, flyer_at, left_down)
}

/// `heading` with FIRST_TOWER_PICK set too.
fn heading_under(arm: FallenLaneTowerPick, first: FirstTowerPick, flyer_at: (i32, i32), left_down: bool) -> (usize, u32, i64, usize) {
    heading_down(arm, first, flyer_at, left_down.then_some(1))
}

/// `heading_under` with the Red princess tower of slot `down` (1 left, 2 right) down, or none.
fn heading_down(arm: FallenLaneTowerPick, first: FirstTowerPick, flyer_at: (i32, i32), down: Option<usize>) -> (usize, u32, i64, usize) {
    let mut cfg = config();
    cfg.calib.fallen_lane_tower_pick = arm;
    cfg.calib.first_tower_pick = first;
    let mut s = BattleState::new(0, cfg);
    if let Some(k) = down {
        s.scenario_set_tower_hp(Team::Red, k, 0).expect("the Red princess tower goes down");
    }
    let at = |p: (i32, i32)| Vec2::new(p.0 * K, p.1 * K);
    let flyer = s.scenario_spawn_now(Team::Blue, "BabyDragon", at(flyer_at), None).unwrap_or_else(|e| panic!("spawn: {e:?}"));
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", at(CANNON_AT), Some(CANNON_HP)).unwrap_or_else(|e| panic!("spawn: {e:?}"));
    let towers: Vec<(usize, (i64, i64))> = s
        .tower_ids(Team::Red)
        .iter()
        .enumerate()
        .filter_map(|(k, id)| (*id).and_then(|id| s.entity(id)).map(|v| (k, native(v.pos))))
        .collect();
    let mut kill: Option<(u32, (i64, i64))> = None;
    let mut path: Vec<(i64, i64)> = Vec::new();
    for tick in 1..=TICKS {
        s.tick();
        let f = s.entity(flyer).expect("the flyer lives through the scene");
        let pos = native(f.pos);
        match kill {
            None => {
                if s.entity(cannon).is_none() {
                    kill = Some((tick, pos));
                    path.push(pos);
                }
            }
            Some(_) => {
                if f.target.is_some() {
                    break;
                }
                path.push(pos);
            }
        }
    }
    let (kill_tick, kill_at) = kill.expect("the scene drifted: the Cannon never died");
    assert!(kill_tick > WINDOW_TICKS, "the scene drifted: the kill on {kill_tick} fell inside the lane window");
    assert!(kill_at.0 > HALF_WIDTH, "the scene drifted: at the kill the flyer stood at x {}, left of the centre", kill_at.0);
    assert!(path.len() >= 20, "the scene drifted: the flyer walked only {} ticks to its default tower", path.len());
    let (p0, p1) = (path[0], path[path.len() - 1]);
    let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
    // The walk's heading against the direction to each tower: the dot product over the tower's distance, in
    // thousandths (the heading's own length is common to every tower, so it drops out). Integers, as everywhere here.
    let score = |t: (i64, i64)| {
        let (tx, ty) = (t.0 - p0.0, t.1 - p0.1);
        (dx * tx + dy * ty) * 1000 / isqrt(tx * tx + ty * ty).max(1)
    };
    let best = towers.iter().max_by_key(|t| score(t.1)).expect("a Red crown tower stands").0;
    (best, kill_tick, kill_at.0, path.len())
}

#[test]
fn a_troop_whose_spawn_lane_tower_is_down_walks_to_the_king() {
    assert!(config().calib.first_tower_pick.spawn_lane(), "the shipped FIRST_TOWER_PICK keeps no spawn lane: this case reads nothing");
    for first in [FirstTowerPick::ClientSpawnLane, FirstTowerPick::ClientSpawnLaneOwnFrame] {
        for arm in [NEW, KING] {
            let (slot, t, x, n) = heading_under(arm, first, FLYER_LEFT, true);
            assert_eq!(slot, 0, "{arm:?} ({first:?}): created at x {} with the left tower down, it killed its target on {t} at x {x} and walked {n} ticks toward tower slot {slot}, not the king", FLYER_LEFT.0);
        }
    }
}

#[test]
fn without_a_spawn_lane_the_new_value_reads_nothing() {
    let (slot, _, x, _) = heading_under(NEW, FirstTowerPick::CurrentX, FLYER_LEFT, true);
    assert_eq!(slot, 2, "new under FIRST_TOWER_PICK = current_x: after the kill at x {x} the flyer walked toward tower slot {slot}, not the right princess");
}

#[test]
fn the_old_value_walks_to_the_tower_of_the_current_x() {
    let (slot, _, x, _) = heading(OLD, FLYER_LEFT, true);
    assert_eq!(slot, 2, "old: after the kill at x {x} the flyer walked toward tower slot {slot}, not the right princess");
}

#[test]
fn a_troop_whose_spawn_lane_tower_stands_walks_to_the_tower_of_its_x() {
    for arm in [NEW, OLD, KING] {
        let (slot, _, _, _) = heading(arm, FLYER_RIGHT, true);
        assert_eq!(slot, 2, "{arm:?}: created at x {}, it walked toward tower slot {slot}, not the right princess", FLYER_RIGHT.0);
    }
}

#[test]
fn with_both_princess_towers_standing_the_value_does_not_matter() {
    for arm in [NEW, OLD, KING] {
        let (slot, _, _, _) = heading(arm, FLYER_LEFT, false);
        assert_eq!(slot, 2, "{arm:?}: with both towers standing it walked toward tower slot {slot}, not the right princess");
    }
}

// targeting.FALLEN_LANE_TOWER_PICK ships client16635_king_distance since parity r100 (client16402_spawn_lane_king before
// it): its pin is the last test, the_shipped_arm_is_client16635_king_distance.

/// targeting.FALLEN_LANE_TOWER_PICK = client16635_king_distance (client 16.635, parity r100, the oracle's il-mg-16635 set:
/// the 12 troops that separate it from client16402_spawn_lane_king took their lane's tower): with the Red RIGHT princess
/// tower down, the Baby Dragon created at (8500, 13000), spawn lane left, kills the Cannon at x 9800 or so and heads for
/// the left princess tower, nearer it than the king; under client16402_spawn_lane_king and current_x (the vacuity check)
/// the pick by its x sends it to the king. Plant: king_distance_by_x.
#[test]
fn under_client16635_king_distance_a_troop_nearer_its_lane_tower_than_the_king_walks_to_it() {
    let first = FirstTowerPick::ClientSpawnLane;
    let (slot, t, x, n) = heading_down(KING, first, FLYER_LEFT, Some(2));
    assert_eq!(slot, 1, "client16635_king_distance: with the right tower down it killed its target on {t} at x {x} and walked {n} ticks toward tower slot {slot}, not the left princess");
    // NOT VACUOUS: the other values take the king by its x.
    for arm in [NEW, OLD] {
        let (slot, _, x, _) = heading_down(arm, first, FLYER_LEFT, Some(2));
        assert_eq!(slot, 0, "{arm:?} (the vacuity check): with the right tower down, after the kill at x {x} it walked toward tower slot {slot}, not the king");
    }
}

/// targeting.FALLEN_LANE_TOWER_PICK = client16635_king_distance's metric (client 16.635, parity r100: an Ice Wizard of
/// c8f5ea00, Blue, against the Red right princess tower (14500, 25500) and the Red king (9000, 29000)): the tower's
/// octagonal length squared against the king's exact squared distance, on 1-unit margins. It kept the tower at
/// (5068, 10196) and (4290, 14142) and took the king at (5032, 10243) and (4292, 14082). The exact distance on both sides
/// (the vacuity check) gives the tower at the last two. Plant: king_distance_true_metric.
#[test]
fn under_client16635_king_distance_the_tower_reads_its_octagonal_length_against_the_kings_exact_distance() {
    let at = |p: (i32, i32)| Vec2::new(p.0 * K, p.1 * K);
    let (p, k) = ((14500, 25500), (9000, 29000));
    for (u, tower) in [((5068, 10196), true), ((4290, 14142), true), ((5032, 10243), false), ((4292, 14082), false)] {
        assert_eq!(princess_nearer_than_king(at(u), at(p), at(k)), tower, "client16635_king_distance: from {u:?} the pick is not the {}", if tower { "tower" } else { "king" });
    }
    // NOT VACUOUS: the exact distance on both sides takes the tower at the last two.
    let exact = |u: (i64, i64)| {
        let (dx, dy) = (i64::from(p.0) - u.0, i64::from(p.1) - u.1);
        let (kx, ky) = (i64::from(k.0) - u.0, i64::from(k.1) - u.1);
        let d = isqrt(dx * dx + dy * dy);
        d * d < kx * kx + ky * ky
    };
    assert!(exact((5032, 10243)) && exact((4292, 14082)), "the vacuity check: the exact distance does not take the tower at (5032, 10243) and (4292, 14082)");
}

/// Since parity r100 for clients 16.402 and 16.635 (client16402_spawn_lane_king before it; client 15.535.29 keeps
/// client16402_spawn_lane_king).
#[test]
fn the_shipped_arm_is_client16635_king_distance() {
    assert_eq!(
        royalesim::state::Calib::shipped().fallen_lane_tower_pick,
        royalesim::state::FallenLaneTowerPick::Client16635KingDistance
    );
    assert_eq!(config().calib.fallen_lane_tower_pick, royalesim::state::FallenLaneTowerPick::Client16635KingDistance);
}
