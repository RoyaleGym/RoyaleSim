//! pathfinding.GOAL_TARGET_POSITION and pathfinding.FLYER_GOAL_WATER, read off the engine's own pass.
//!
//! A FLYING chaser's route is its goal cell alone (state.rs `phase_path16402`), so every tick the flyer walks, its
//! route names the cell the pass chose. Each test recomputes that choice with path16402.rs `choose_goal_cell` from the
//! positions the rule says the pass reads, under each value of the key, and holds the engine's cell to the key's.
//!
//! THE LAWS, measured on client 15.535.29:
//!   * GOAL_TARGET_POSITION = creation_order: the target centre a chaser's goal is chosen around is the one the
//!     creation-order move pass holds at the chaser's turn: the target's MOVED position when it was created before the
//!     chaser, its start-of-tick position when it was created after. start_of_tick reads the start of the tick always.
//!   * FLYER_GOAL_WATER = not_demoted: a flyer's goal choice ranks a water cell like dry ground; demoted ranks it below,
//!     as for a ground chaser.
//!
//! WHAT IS PINNED, each with a floor on the ticks where the two readings part, so no test passes on a scene where the
//! key cannot matter:
//!   1. a Mega Minion created AFTER a walking Knight picks its cell from the Knight's moved position under
//!      creation_order and from its start-of-tick position under start_of_tick;
//!   2. one created BEFORE the Knight reads the start-of-tick position under creation_order too (an implementation that
//!      reads the moved position always is refused);
//!   3. a Mega Minion over the river heads for the nearest cell in reach, water or not, under not_demoted, and for the
//!      nearest DRY cell under demoted. It is created before its Knight, so GOAL_TARGET_POSITION reads the same position
//!      under either value and cannot move this result.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test goal_target`):
//!   * `goal_target_start_of_tick` -- creation_order reads the start of the tick: (1) goes red.
//!   * `flyer_water_demoted` -- not_demoted demotes water for a flyer too: (3) goes red.
mod common;

use common::*;
use royalesim::arena::Arena;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, FlyerGoalWater, GoalTargetPosition};
use royalesim::{path16402, path2026, EntityId, Team};

/// Fewest ticks where the two readings choose different cells, before a scene counts.
const MIN_SPLIT: usize = 3;

fn native(v: Vec2) -> (i32, i32) {
    (v.x / K, v.y / K)
}

/// The goal cell `choose_goal_cell` gives a flyer at `actor` chasing a GROUND target at `target` (native units),
/// with no building box in reach (every scene here stands clear of the towers).
fn expected(actor: (i32, i32), target: (i32, i32), reach: i32, water_demoted: bool) -> (i32, i32) {
    let arena = Arena::shipped();
    let costs = path2026::costs16402(&Calib::shipped());
    let t = path2026::terrain16402(&arena, &costs);
    let occ = vec![0; (arena.cols * arena.rows) as usize];
    path16402::choose_goal_cell(&t, &occ, actor, target, reach, path2026::avoid_buildings16402(false), costs.building, water_demoted)
        .expect("a cell in reach")
}

/// What one run saw: per walked tick, (the flyer's start-of-tick position, the Knight's start-of-tick and end-of-tick
/// positions, the cell the engine's route names), all native.
type Row = ((i32, i32), (i32, i32), (i32, i32), (i32, i32));
type Seen = Vec<Row>;

/// A Blue Mega Minion at `flyer_at` chasing a Red Knight at `knight_at`, the Knight created first when `knight_first`,
/// under the two keys, for `ticks` ticks. Only the ticks the flyer walks toward the Knight are kept.
fn chase(goal: GoalTargetPosition, water: FlyerGoalWater, knight_at: (i32, i32), flyer_at: (i32, i32), knight_first: bool, ticks: u32) -> (Seen, i32) {
    let mut cfg = config();
    cfg.calib.goal_target_position = goal;
    cfg.calib.flyer_goal_water = water;
    let mut s = BattleState::new(0, cfg);
    let at = |p: (i32, i32)| Vec2::new(p.0 * K, p.1 * K);
    let spawn = |s: &mut BattleState, team: Team, card: &str, p: (i32, i32)| -> EntityId {
        s.scenario_spawn_now(team, card, at(p), None).unwrap_or_else(|e| panic!("spawn {card}: {e:?}"))
    };
    let (knight, flyer) = if knight_first {
        let k = spawn(&mut s, Team::Red, "Knight", knight_at);
        (k, spawn(&mut s, Team::Blue, "MegaMinion", flyer_at))
    } else {
        let f = spawn(&mut s, Team::Blue, "MegaMinion", flyer_at);
        (spawn(&mut s, Team::Red, "Knight", knight_at), f)
    };
    let reach = {
        let f = s.entity(flyer).expect("the flyer stands");
        (card_stat(&s, "MegaMinion").range + f.radius) / K
    };
    let mut seen = Vec::new();
    for _ in 0..ticks {
        let (Some(f), Some(k)) = (s.entity(flyer), s.entity(knight)) else { break };
        let (f0, k0) = (native(f.pos), native(k.pos));
        s.tick();
        let (Some(f), Some(k)) = (s.entity(flyer), s.entity(knight)) else { break };
        if f.deploying || f.target != Some(knight) || f.route.is_empty() {
            continue;
        }
        let c = native(f.route[0]);
        seen.push((f0, k0, native(k.pos), (c.0 / path16402::CELL, c.1 / path16402::CELL)));
    }
    (seen, reach)
}

/// Ticks on which the engine's cell is not `want`'s, and ticks on which `want` and `other` part.
fn misses(seen: &Seen, reach: i32, want: impl Fn(&Row, i32) -> (i32, i32), other: impl Fn(&Row, i32) -> (i32, i32)) -> (usize, usize) {
    let bad = seen.iter().filter(|r| r.3 != want(r, reach)).count();
    let split = seen.iter().filter(|r| want(r, reach) != other(r, reach)).count();
    (bad, split)
}

// The first scene, as on the client: a Red Knight on the blue half walking to the blue left princess tower, and a Blue
// flyer inland of it, so its reach circle never touches the river.
const KNIGHT_AT: (i32, i32) = (8500, 12000);
const FLYER_AT: (i32, i32) = (5500, 14000);

#[test]
fn a_flyer_created_after_its_target_reads_the_targets_moved_position_under_creation_order() {
    let moved = |r: &Row, reach| expected(r.0, r.2, reach, true);
    let start = |r: &Row, reach| expected(r.0, r.1, reach, true);
    let (seen, reach) = chase(GoalTargetPosition::CreationOrder, FlyerGoalWater::Demoted, KNIGHT_AT, FLYER_AT, true, 120);
    assert!(seen.len() >= 20, "the flyer walked after the Knight on only {} ticks", seen.len());
    let (bad, split) = misses(&seen, reach, moved, start);
    assert!(split >= MIN_SPLIT, "the moved and start-of-tick readings part on only {split} ticks, so this looked at nothing");
    assert_eq!(bad, 0, "creation_order: {bad} of {} ticks do not head for the cell chosen from the Knight's MOVED position", seen.len());
    let (seen, reach) = chase(GoalTargetPosition::StartOfTick, FlyerGoalWater::Demoted, KNIGHT_AT, FLYER_AT, true, 120);
    let (bad, split) = misses(&seen, reach, start, moved);
    assert!(split >= MIN_SPLIT, "start_of_tick: the readings part on only {split} ticks");
    assert_eq!(bad, 0, "start_of_tick: {bad} of {} ticks do not head for the cell chosen from the Knight's start-of-tick position", seen.len());
}

#[test]
fn a_flyer_created_before_its_target_reads_the_start_of_tick_position_under_either_value() {
    let moved = |r: &Row, reach| expected(r.0, r.2, reach, true);
    let start = |r: &Row, reach| expected(r.0, r.1, reach, true);
    for goal in [GoalTargetPosition::CreationOrder, GoalTargetPosition::StartOfTick] {
        let (seen, reach) = chase(goal, FlyerGoalWater::Demoted, KNIGHT_AT, FLYER_AT, false, 120);
        assert!(seen.len() >= 20, "{goal:?}: the flyer walked after the Knight on only {} ticks", seen.len());
        let (bad, split) = misses(&seen, reach, start, moved);
        assert!(split >= MIN_SPLIT, "{goal:?}: the readings part on only {split} ticks, so this looked at nothing");
        assert_eq!(bad, 0, "{goal:?}: a flyer created first read the Knight's moved position on {bad} of {} ticks", seen.len());
    }
}

// The water scene: a Red Knight just north of the river on the red half, a Blue flyer created first, out over the
// river to its left. The nearest cells in the flyer's reach are water.
const WATER_KNIGHT_AT: (i32, i32) = (8000, 17600);
const WATER_FLYER_AT: (i32, i32) = (4000, 16000);

#[test]
fn a_flyer_over_the_river_ranks_water_like_dry_ground_only_under_not_demoted() {
    let wet = |r: &Row, reach| expected(r.0, r.1, reach, false);
    let dry = |r: &Row, reach| expected(r.0, r.1, reach, true);
    let (seen, reach) = chase(GoalTargetPosition::CreationOrder, FlyerGoalWater::NotDemoted, WATER_KNIGHT_AT, WATER_FLYER_AT, false, 80);
    assert!(seen.len() >= 10, "the flyer walked after the Knight on only {} ticks", seen.len());
    let (bad, split) = misses(&seen, reach, wet, dry);
    assert!(split >= MIN_SPLIT, "the water readings part on only {split} ticks, so the river was never in reach");
    assert_eq!(bad, 0, "not_demoted: {bad} of {} ticks do not head for the nearest cell in reach", seen.len());
    let (seen, reach) = chase(GoalTargetPosition::CreationOrder, FlyerGoalWater::Demoted, WATER_KNIGHT_AT, WATER_FLYER_AT, false, 80);
    let (bad, split) = misses(&seen, reach, dry, wet);
    assert!(split >= MIN_SPLIT, "demoted: the water readings part on only {split} ticks");
    assert_eq!(bad, 0, "demoted: {bad} of {} ticks do not head for the nearest DRY cell in reach", seen.len());
}

#[test]
fn both_keys_ship_at_their_new_values() {
    let c = Calib::shipped();
    assert_eq!(c.goal_target_position, GoalTargetPosition::CreationOrder);
    assert_eq!(c.flyer_goal_water, FlyerGoalWater::NotDemoted);
}
