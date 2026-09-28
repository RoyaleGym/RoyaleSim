//! targeting.TARGET_RANK_DISTANCE, read off the engine: the distance a scan ranks the enemies in sight by (target.rs
//! `key`).
//!
//! THE LAW, measured on the 16.402 corpus: a troop walking to its crown tower takes the nearest enemy in sight on the
//! first tick that enemy's start-of-tick CENTRE distance is below the tower's (577 of 578 walking switches). In
//! 20260918-124946, on tick 941, a Goblin 6,578.4 from a Cannon and 6,840.8 from its princess tower took the Cannon.
//! Ranked by centre minus the candidate's radius (the old arm) the tower scores 5,840.8 and the Cannon 5,978.4, and
//! the Goblin walks on.
//!
//! The scenes are tests/test_target_rank_distance.py's: the Cannon where it stood in that battle, (9500, 9500), and a
//! red Knight (SightRange 5500 and radius 500, as a Goblin's) set down at (4000, 14500) or (3500, 14500), walking to the
//! blue left princess tower at (3500, 6500). WHAT IS PINNED, each with its precondition:
//!   1. client16402_centre: on the first tick that starts with the Cannon inside the Knight's sight sum, nearer than
//!      the tower by centre and farther by centre minus radius, the Knight targets the Cannon;
//!   2. centre_minus_target_radius: on every tick the tower's key is the smaller, the Knight keeps the tower;
//!   3. both values: from x 3500 the Knight sees the Cannon while the tower is nearer by centre, and never takes it (an
//!      implementation where any enemy in sight beats the tower is refused);
//!   4. the shipped value is client16402_centre (centre_minus_target_radius, the old arm, shipped until parity scored
//!      the flip).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test target_rank_distance`):
//!   * `rank_centre_minus_radius` -- client16402_centre still ranks by centre minus the candidate's radius: (1) goes
//!     red.
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, TargetRankDistance};
use royalesim::{EntityId, Team};

const NEW: TargetRankDistance = TargetRankDistance::Client16402Centre;
const OLD: TargetRankDistance = TargetRankDistance::CentreMinusTargetRadius;

/// At x 4000 the Knight first sees the Cannon nearer than the tower by centre; at x 3500, farther.
const KNIGHT_SWITCH: (i32, i32) = (4000, 14500);
const KNIGHT_CONTROL: (i32, i32) = (3500, 14500);
const CANNON_AT: (i32, i32) = (9500, 9500);
const TOWER_AT: (i32, i32) = (3500, 6500);
/// The Knight's SightRange + its radius + the Cannon's, native.
const SIGHT_SUM: i64 = 5500 + 500 + 600;
const TOWER_R: i64 = 1000;
const CANNON_R: i64 = 600;
const TICKS: u32 = 160;

fn with_arm(arm: TargetRankDistance) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.target_rank_distance = arm;
    cfg
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// A squared subtile distance as native units, for messages.
fn native(d2: i64) -> i64 {
    isqrt(d2) / K as i64
}

/// One tick: the Cannon's and the tower's START-of-tick squared centre distances from the Knight (subtiles), and the
/// Knight's target after the tick.
struct Row {
    tick: u32,
    dc2: i64,
    dt2: i64,
    target: Option<EntityId>,
}

impl Row {
    fn in_sight(&self) -> bool {
        self.dc2 <= (SIGHT_SUM * K as i64).pow(2)
    }
    /// The Cannon is the nearer by centre distance.
    fn centre_takes_cannon(&self) -> bool {
        self.dc2 < self.dt2
    }
    /// The tower's centre - radius is the smaller (the old arm keeps the tower).
    fn edge_keeps_tower(&self) -> bool {
        isqrt(self.dt2) - TOWER_R * (K as i64) < isqrt(self.dc2) - CANNON_R * (K as i64)
    }
}

/// A red Knight at `knight_at` and the blue Cannon, under `arm`: (the Cannon, the blue left princess tower, the rows).
fn walk(arm: TargetRankDistance, knight_at: (i32, i32)) -> (EntityId, EntityId, Vec<Row>) {
    let mut s = BattleState::new(0, with_arm(arm));
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at(knight_at), None), (Team::Blue, "Cannon", at(CANNON_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (knight, cannon) = (ids[0], ids[1]);
    let tower = s
        .tower_ids(Team::Blue)
        .into_iter()
        .flatten()
        .find(|t| s.entity(*t).is_some_and(|v| v.pos == at(TOWER_AT)))
        .expect("the blue left princess tower stands at (3500, 6500)");
    let mut rows = Vec::new();
    for tick in 1..=TICKS {
        let (Some(k), Some(c), Some(t)) = (s.entity(knight), s.entity(cannon), s.entity(tower)) else { break };
        let (dc2, dt2) = (k.pos.dist2(c.pos), k.pos.dist2(t.pos));
        s.tick();
        let Some(k) = s.entity(knight) else { break };
        rows.push(Row { tick, dc2, dt2, target: k.target });
    }
    (cannon, tower, rows)
}

/// The first row that starts with the Cannon inside the Knight's sight sum. Before it the Knight holds its tower.
fn entry(rows: &[Row], tower: EntityId, what: &str) -> usize {
    let first = rows
        .iter()
        .position(Row::in_sight)
        .unwrap_or_else(|| panic!("{what}: the scene drifted: the Cannon never came into sight"));
    assert!(
        first > 0 && rows[..first].iter().all(|r| r.target == Some(tower)),
        "{what}: the scene drifted: the Knight did not walk to its tower until the Cannon came into sight"
    );
    first
}

#[test]
fn a_walker_takes_an_enemy_nearer_than_its_tower_by_centre() {
    let (cannon, tower, rows) = walk(NEW, KNIGHT_SWITCH);
    let r = &rows[entry(&rows, tower, "client16402_centre")];
    assert!(
        r.centre_takes_cannon() && r.edge_keeps_tower(),
        "the scene drifted: on {} the Cannon stands {} and the tower {}; the two rankings agree",
        r.tick,
        native(r.dc2),
        native(r.dt2)
    );
    assert_eq!(
        r.target,
        Some(cannon),
        "on {} the Cannon is {} from the Knight and the tower {} (centre), but the Knight does not take the Cannon",
        r.tick,
        native(r.dc2),
        native(r.dt2)
    );
}

#[test]
fn the_old_value_keeps_the_tower() {
    let (cannon, tower, rows) = walk(OLD, KNIGHT_SWITCH);
    let i = entry(&rows, tower, "centre_minus_target_radius");
    let split: Vec<&Row> = rows[i..].iter().filter(|r| r.in_sight() && r.edge_keeps_tower()).collect();
    assert!(!split.is_empty(), "the scene drifted: the tower's key is never the smaller with the Cannon in sight");
    let taken: Vec<u32> = split.iter().filter(|r| r.target == Some(cannon)).map(|r| r.tick).collect();
    assert!(taken.is_empty(), "the Knight took the Cannon on {taken:?} while the tower's key was the smaller");
}

#[test]
fn an_enemy_in_sight_but_farther_than_the_tower_is_not_taken() {
    for arm in [NEW, OLD] {
        let (cannon, tower, rows) = walk(arm, KNIGHT_CONTROL);
        let i = entry(&rows, tower, "control");
        let seen: Vec<&Row> = rows[i..].iter().filter(|r| r.in_sight()).collect();
        assert!(
            seen.iter().all(|r| !r.centre_takes_cannon()),
            "{arm:?}: the scene drifted: the Cannon was nearer than the tower"
        );
        let taken: Vec<u32> = seen.iter().filter(|r| r.target == Some(cannon)).map(|r| r.tick).collect();
        assert!(
            taken.is_empty(),
            "{arm:?}: the Knight took a Cannon in sight but farther than its tower, on {taken:?}"
        );
    }
}

#[test]
fn the_shipped_value_is_client16402_centre() {
    assert_eq!(Calib::shipped().target_rank_distance, NEW);
}


/// 20260918-124946 tick 1923: a Tombstone's Skeleton at (4151, 10363) between two Goblins 2,503.95 and 2,503.99 away,
/// (2022, 9045) and (4338, 7866). Their subtile distances both root to 45,071; the client takes the nearer. Here a red
/// Knight between two blue Knights on those points, on the first tick it takes one of them.
#[test]
fn of_two_enemies_whose_integer_distances_tie_the_nearer_is_taken() {
    const RED: (i32, i32) = (4151, 10363);
    const NEAR: (i32, i32) = (2022, 9045);
    const FAR: (i32, i32) = (4338, 7866);
    let mut s = BattleState::new(0, with_arm(NEW));
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at(RED), None), (Team::Blue, "Knight", at(NEAR), None), (Team::Blue, "Knight", at(FAR), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (red, near, far) = (ids[0], ids[1], ids[2]);
    for _ in 0..TICKS {
        let (Some(r), Some(n), Some(f)) = (s.entity(red), s.entity(near), s.entity(far)) else { panic!("a Knight died") };
        let (dn2, df2) = (r.pos.dist2(n.pos), r.pos.dist2(f.pos));
        s.tick();
        let target = s.entity(red).and_then(|r| r.target);
        if target == Some(near) || target == Some(far) {
            assert!(isqrt(dn2) == isqrt(df2) && dn2 < df2, "the scene drifted: the two distances ({dn2}, {df2}) do not tie at their root");
            assert_eq!(target, Some(near), "the Knight took the farther of two enemies whose integer distances tie");
            return;
        }
    }
    panic!("the red Knight took neither blue Knight");
}
