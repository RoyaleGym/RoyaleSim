//! targeting.CHASE_RESCAN_PASS_OVER: which rescans pass over an enemy troop past the chase-drop limit (target.rs
//! `scan_with`, `recedes_from_lane_walk`; state.rs `chase_pass_end`, entity.rs `chase_lane_walk`), under
//! targeting.CHASE_DROP_WALKING_AWAY = client15535_growing_away and CHASE_DROP_MEASURE = client15535_lane_dy.
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, receding_rescan_census.py: a unit walking for its tower passes
//! over the nearest troop in its round sight past the limit when that troop's |dy| grew, whatever the troop does, about
//! 600 rescans; a unit's first pick out of its deploy takes it (ub-sd14-a2 t307).
//!
//! THE SCENE: a Red Knight walking for Blue's tower with nothing in sight, held on one point; a Blue Giant held far across;
//! on the decisive tick the Giant is put in the Knight's round sight past its limit, |dy| larger than the tick before.
//!
//! WHAT IS PINNED, and the plant that turns it red (chase_rescan_drop_tick_only):
//!   1. drop_tick (the engine's, the vacuity check): the Knight takes the Giant; client15535_receding_lane_walk: it holds
//!      no target;
//!   2. under client15535_receding_lane_walk a fresh Knight on the same point (its first pick) takes the Giant.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, ChaseDropMeasure, ChaseDropRange, ChaseDropWalkingAway, ChaseRescanPassOver, Calib, TickOrder};
use royalesim::{EntityId, Team};

fn with_arm(arm: ChaseRescanPassOver) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.chase_drop_range = ChaseDropRange::ClientSightMinus1000;
    cfg.calib.chase_drop_measure = ChaseDropMeasure::Client15535LaneDy;
    cfg.calib.chase_drop_walking_away = ChaseDropWalkingAway::Client15535GrowingAway;
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    cfg.calib.chase_rescan_pass_over = arm;
    cfg
}

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// A Knight's chase-drop limit on `card`, native: SightRange + both radii - 1000.
fn limit_on(s: &BattleState, card: &str) -> i32 {
    let cards = &s.config().cards;
    let k = cards.get(cards.index("Knight").expect("Knight loads"));
    let c = cards.get(cards.index(card).expect("the card loads"));
    (k.sight_range + k.collision_radius + c.collision_radius) / K - 1000
}

const KNIGHT: (i32, i32) = (3500, 20000);

/// The scene: (the walking Knight's target after the decisive tick, a fresh Knight's when `fresh`).
fn rescan(arm: ChaseRescanPassOver, fresh: bool) -> (Option<EntityId>, Option<EntityId>, EntityId) {
    let mut s = BattleState::new(0, with_arm(arm));
    past_deploy_lockout(&mut s);
    let lim = limit_on(&s, "Giant");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(KNIGHT.0, KNIGHT.1), None).expect("the Knight");
    let far = n(KNIGHT.0 + 9000, KNIGHT.1 + lim + 200);
    let giant = s.scenario_spawn_now(Team::Blue, "Giant", far, None).expect("the Giant");
    for _ in 0..4 {
        assert!(s.debug_set_pos(knight, n(KNIGHT.0, KNIGHT.1)) && s.debug_set_pos(giant, far));
        s.tick();
        assert_eq!(s.entity(knight).expect("the Knight").target, None, "the scene drifted: the Knight holds a target before the Giant comes near");
    }
    assert!(s.debug_set_pos(knight, n(KNIGHT.0, KNIGHT.1)));
    // In round sight (|dx| 400) and past the limit by 300, |dy| 100 more than the tick before.
    assert!(s.debug_set_pos(giant, n(KNIGHT.0 + 400, KNIGHT.1 + lim + 300)));
    let other = if fresh { Some(s.scenario_spawn_now(Team::Red, "Knight", n(KNIGHT.0 - 100, KNIGHT.1), None).expect("the fresh Knight")) } else { None };
    s.tick();
    (s.entity(knight).expect("the Knight").target, other.and_then(|k| s.entity(k).expect("the fresh Knight").target), giant)
}

/// Plant: chase_rescan_drop_tick_only.
#[test]
fn a_unit_walking_for_its_tower_passes_over_a_receding_troop_under_client15535_receding_lane_walk() {
    let (old, _, giant) = rescan(ChaseRescanPassOver::DropTick, false);
    // NOT VACUOUS: the engine's arm takes the Giant past the limit at plain sight.
    assert_eq!(old, Some(giant), "drop_tick: the Knight does not take the Giant");
    let (new, _, _) = rescan(ChaseRescanPassOver::Client15535RecedingLaneWalk, false);
    assert_eq!(new, None, "client15535_receding_lane_walk: the Knight took the receding Giant");
}

#[test]
fn a_first_pick_takes_a_receding_troop_under_client15535_receding_lane_walk() {
    let (_, fresh, giant) = rescan(ChaseRescanPassOver::Client15535RecedingLaneWalk, true);
    assert_eq!(fresh, Some(giant), "client15535_receding_lane_walk: a fresh Knight's first pick passed over the Giant");
}

#[test]
fn the_shipped_value_is_drop_tick() {
    assert_eq!(Calib::shipped().chase_rescan_pass_over, ChaseRescanPassOver::DropTick);
}
