//! knockback.LADDER_PATH_REQUEST: when a unit a knockback ladder carries, holding no route, asks for one (state.rs
//! `phase_path16402_for`'s pushback tick).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, every knockback ladder in the records: of the units holding
//! no route going in that walked and planned during the ladder, 87 of 111 planned on their first walking frame;
//! sp-champ-Monk-nopress-s0 t252-t265, a Knight knocked out of its attack, walked that route on at the ladder's end.
//!
//! THE SCENE: on Blue's bank a blue Knight fights a red Knight (attacking: no route); a red Fireball cast on the blue
//! Knight knocks it back out of its reach. WHAT IS PINNED (the ladder's ticks: the blue Knight's steps shrinking by
//! 15..35 a tick from 100 or more):
//!   1. client15535_on_ladder: the blue Knight holds a route on a tick of its ladder;
//!   2. ladder_end (the old arm, the vacuity check): it holds none on any of them;
//!   3. the shipped value is ladder_end (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test ladder_path_request`):
//!   * `ladder_plans_at_end` -- the new arm still plans only when the ladder ends: (1) goes red.
//!   * `ladder_plan_segment_unfrozen` -- pathfinding.LADDER_PLAN_SEGMENT's new arm leaves the plan's segment to the
//!     walk: (4) goes red.
//!   * `ladder_goal_moved_replans` -- client15535_on_ladder_goal_held replans a moved goal for the same target: (5) goes
//!     red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, LadderPathRequest, LadderPlanSegment};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The blue Knight's route lengths on its ladder's ticks under `arm` (empty when no ladder ran).
fn ladder_routes(arm: LadderPathRequest) -> Vec<usize> {
    let mut cfg: BattleConfig = config();
    cfg.calib.ladder_path_request = arm;
    cfg.decks = [vec!["Knight".into(), "Zap".into()], vec!["Knight".into(), "Fireball".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(3, cfg);
    past_deploy_lockout(&mut s);
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Knight", n(9000, 12000), None), (Team::Red, "Knight", n(9000, 13300), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let blue = ids[0];
    for _ in 0..60 {
        s.tick();
    }
    let at = s.entity(blue).expect("the blue Knight").pos;
    assert!(s.entity(blue).expect("the blue Knight").route.is_empty(), "{arm:?}: the scene drifted: the blue Knight holds a route while it fights");
    s.spawn_unit(Team::Red, "Fireball", at, None).expect("the Fireball");
    let mut rows: Vec<(Vec2, usize)> = Vec::new();
    for _ in 0..90 {
        s.tick();
        let Some(k) = s.entity(blue) else { break };
        rows.push((k.pos, k.route.len()));
    }
    let step: Vec<i64> = (1..rows.len())
        .map(|j| {
            let (a, b) = (rows[j - 1].0, rows[j].0);
            let (dx, dy) = (((b.x - a.x) / K) as i64, ((b.y - a.y) / K) as i64);
            isqrt(dx * dx + dy * dy)
        })
        .collect();
    // The ladder: the first step of 100 or more, then each 15..35 shorter than the one before.
    let Some(j) = step.iter().position(|&v| v >= 100) else { return Vec::new() };
    let mut k = j;
    while k + 1 < step.len() && (15..=35).contains(&(step[k] - step[k + 1])) {
        k += 1;
    }
    (j..=k).map(|m| rows[m + 1].1).collect()
}

/// Plant: ladder_plans_at_end.
#[test]
fn a_knocked_unit_with_no_route_plans_on_its_ladder_under_client15535_on_ladder() {
    let r = ladder_routes(LadderPathRequest::Client15535OnLadder);
    assert!(r.len() >= 4, "the scene drifted: no ladder of four or more ticks ({r:?})");
    assert!(r.iter().any(|&l| l > 0), "new: no route on any tick of the ladder: {r:?}");
}

#[test]
fn the_old_value_plans_only_when_the_ladder_ends() {
    let r = ladder_routes(LadderPathRequest::LadderEnd);
    assert!(r.len() >= 4, "the scene drifted: no ladder of four or more ticks ({r:?})");
    assert!(r.iter().all(|&l| l == 0), "old: a route on a tick of the ladder (vacuous otherwise): {r:?}");
}

#[test]
fn the_shipped_value_is_ladder_end() {
    assert_eq!(Calib::shipped().ladder_path_request, LadderPathRequest::LadderEnd);
}

/// (4) pathfinding.LADDER_PLAN_SEGMENT (client 15.535.29: 133 of 133 ladder-tick plans froze their segment on the plan
/// tick from the start-of-tick point): the scene under client15535_on_ladder; on the tick the blue Knight's ladder plan
/// lands (its route empty before, held after): its segment, and the direction (256 long) from its point before that tick
/// toward the route's next node.
fn plan_tick_segment(arm: LadderPlanSegment) -> ((i32, i32), (i32, i32)) {
    let mut cfg: BattleConfig = config();
    cfg.calib.ladder_path_request = LadderPathRequest::Client15535OnLadder;
    cfg.calib.ladder_plan_segment = arm;
    cfg.decks = [vec!["Knight".into(), "Zap".into()], vec!["Knight".into(), "Fireball".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(3, cfg);
    past_deploy_lockout(&mut s);
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Knight", n(9000, 12000), None), (Team::Red, "Knight", n(9000, 13300), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let blue = ids[0];
    for _ in 0..60 {
        s.tick();
    }
    let at = s.entity(blue).expect("the blue Knight").pos;
    s.spawn_unit(Team::Red, "Fireball", at, None).expect("the Fireball");
    for _ in 0..90 {
        let (p0, had) = {
            let k = s.entity(blue).expect("the blue Knight");
            (k.pos, !k.route.is_empty())
        };
        s.tick();
        let k = s.entity(blue).expect("the blue Knight");
        if !had && k.push_active && !k.route.is_empty() {
            let node = *k.route.last().expect("a node");
            let mut d = ((node.x - p0.x) / K, (node.y - p0.y) / K);
            let len = isqrt(i64::from(d.0) * i64::from(d.0) + i64::from(d.1) * i64::from(d.1)).max(1);
            d = ((i64::from(d.0) * 256 / len) as i32, (i64::from(d.1) * 256 / len) as i32);
            return ((k.seg_dir.x, k.seg_dir.y), d);
        }
    }
    panic!("{arm:?}: the scene drifted: no plan on a ladder tick");
}

/// Plant: ladder_plan_segment_unfrozen.
#[test]
fn a_ladder_tick_plan_freezes_its_segment_at_once_under_client15535_plan_tick() {
    let (seg, d) = plan_tick_segment(LadderPlanSegment::Client15535PlanTick);
    assert!((seg.0 - d.0).abs() <= 1 && (seg.1 - d.1).abs() <= 1, "client15535_plan_tick: the segment {seg:?} is not toward the next node from the start of the tick {d:?}");
    // NOT VACUOUS: walk_tick leaves it unfrozen on the plan tick.
    let (old, _) = plan_tick_segment(LadderPlanSegment::WalkTick);
    assert_eq!(old, (0, 0), "walk_tick: the plan tick's segment is frozen");
}

/// (5) knockback.LADDER_PATH_REQUEST = client15535_on_ladder_goal_held (client 15.535.29: 0 of 91 same-target ladders
/// changed their goal mid-ladder): the scene with the red Knight 1,700 off (in reach before the Fireball, out of it after
/// its push, so the blue Knight plans on its ladder), set 1,500 to one side or the other on every tick of the blue
/// Knight's ladder after its first plan (its goal cell moves, its target stays): the route's last node on each such tick.
fn goals_on_ladder(arm: LadderPathRequest) -> Vec<Vec2> {
    let mut cfg: BattleConfig = config();
    cfg.calib.ladder_path_request = arm;
    cfg.decks = [vec!["Knight".into(), "Zap".into()], vec!["Knight".into(), "Fireball".into()]];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(3, cfg);
    past_deploy_lockout(&mut s);
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Knight", n(9000, 12000), None), (Team::Red, "Knight", n(9000, 13700), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (blue, red) = (ids[0], ids[1]);
    let full = s.entity(red).expect("the red Knight").max_hp;
    for _ in 0..60 {
        s.tick();
    }
    let at = s.entity(blue).expect("the blue Knight").pos;
    s.spawn_unit(Team::Red, "Fireball", at, None).expect("the Fireball");
    let (mut out, mut planned, mut side) = (Vec::new(), false, 1);
    for _ in 0..90 {
        // The red Knight kept whole: set aside it stands in a blue princess tower's reach, beside the blue Knight's hits.
        assert!(s.debug_set_hp(red, full));
        if planned {
            let p = s.entity(red).expect("the red Knight").pos;
            side = -side;
            assert!(s.debug_set_pos(red, Vec2::new(9000 * K + side * 1500 * K, p.y)));
        }
        s.tick();
        let k = s.entity(blue).expect("the blue Knight");
        if k.push_active && !k.route.is_empty() {
            if planned {
                assert_eq!(k.target, Some(red), "{arm:?}: the scene drifted: the blue Knight let the red one go");
                out.push(*k.route.first().expect("a node"));
            }
            planned = true;
        } else if planned && !k.push_active {
            break;
        }
    }
    assert!(out.len() >= 3, "{arm:?}: the scene drifted: {} ladder ticks after the first plan", out.len());
    out
}

/// Plant: ladder_goal_moved_replans.
#[test]
fn a_knocked_unit_holds_its_routes_goal_for_the_same_target_under_client15535_on_ladder_goal_held() {
    let new = goals_on_ladder(LadderPathRequest::Client15535OnLadderGoalHeld);
    assert!(new.windows(2).all(|w| w[0] == w[1]), "client15535_on_ladder_goal_held: the goal moved mid-ladder: {new:?}");
    // NOT VACUOUS: client15535_on_ladder replans the moved goal on a ladder tick.
    let old = goals_on_ladder(LadderPathRequest::Client15535OnLadder);
    assert!(old.windows(2).any(|w| w[0] != w[1]), "client15535_on_ladder: the goal never moved: {old:?}");
}
