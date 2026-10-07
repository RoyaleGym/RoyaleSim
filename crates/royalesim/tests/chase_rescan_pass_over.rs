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
//!   2. under client15535_receding_lane_walk a fresh Knight on the same point (its first pick) takes the Giant;
//!   5. (plant recede_reads_newborn) a Giant put down on the tick before the decisive one never recedes: both arms take
//!      it (client 15.535.29, item 313: sp-form-Tombstone-hero-nopress-s0 t154, the hero Tombstone's first Skeleton,
//!      born the tick before, taken by a Red Skeleton whose own push grew |dy|).
//!
//! client15535_receding_or_behind (receding_behind_census.py: a troop behind past the limit taken in 0 of 1,326 rescans,
//! sp-il-04cb t1222 and t2931) -- plant rescan_behind_taken:
//!   3. the Giant held BEHIND the walking Knight past its limit, |dy| as the tick before: client15535_receding_lane_walk
//!      (the vacuity check) takes it, client15535_receding_or_behind holds no target;
//!   4. the same Giant AHEAD of the Knight: both take it.
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

/// The walking Red Knight's target after the decisive tick, the Blue Giant held `dy` (native, arena y; Red walks to -y,
/// so +dy is behind it) from the Knight past its limit, |dy| unchanged from the tick before.
fn held_at(arm: ChaseRescanPassOver, behind: bool) -> (Option<EntityId>, EntityId) {
    let mut s = BattleState::new(0, with_arm(arm));
    past_deploy_lockout(&mut s);
    let lim = limit_on(&s, "Giant");
    let dy = if behind { lim + 300 } else { -(lim + 300) };
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(KNIGHT.0, KNIGHT.1), None).expect("the Knight");
    let far = n(KNIGHT.0 + 9000, KNIGHT.1 + dy);
    let giant = s.scenario_spawn_now(Team::Blue, "Giant", far, None).expect("the Giant");
    for _ in 0..4 {
        assert!(s.debug_set_pos(knight, n(KNIGHT.0, KNIGHT.1)) && s.debug_set_pos(giant, far));
        s.tick();
        assert_eq!(s.entity(knight).expect("the Knight").target, None, "the scene drifted: the Knight holds a target before the Giant comes near");
    }
    assert!(s.debug_set_pos(knight, n(KNIGHT.0, KNIGHT.1)));
    // In round sight (|dx| 400) and past the limit by 300, |dy| as the tick before.
    assert!(s.debug_set_pos(giant, n(KNIGHT.0 + 400, KNIGHT.1 + dy)));
    s.tick();
    (s.entity(knight).expect("the Knight").target, giant)
}

/// Plant: rescan_behind_taken.
#[test]
fn a_rescan_passes_over_a_troop_behind_past_the_limit_under_client15535_receding_or_behind() {
    let (old, giant) = held_at(ChaseRescanPassOver::Client15535RecedingLaneWalk, true);
    // NOT VACUOUS: the receding arm takes a troop behind whose |dy| did not grow.
    assert_eq!(old, Some(giant), "client15535_receding_lane_walk: the Knight does not take the Giant behind it");
    let (new, _) = held_at(ChaseRescanPassOver::Client15535RecedingOrBehind, true);
    assert_eq!(new, None, "client15535_receding_or_behind: the Knight took the Giant behind it");
    let (ahead, giant) = held_at(ChaseRescanPassOver::Client15535RecedingOrBehind, false);
    assert_eq!(ahead, Some(giant), "client15535_receding_or_behind: the Knight passed over the Giant ahead of it");
}

#[test]
fn the_receding_cases_hold_under_client15535_receding_or_behind() {
    let (new, _, _) = rescan(ChaseRescanPassOver::Client15535RecedingOrBehind, false);
    assert_eq!(new, None, "client15535_receding_or_behind: the Knight took the receding Giant");
}

/// (5) The walking Red Knight's target after the decisive tick, a Blue Giant AHEAD of it (Red walks to -y) past its limit,
/// |dy| 100 more than on the tick before: put down on the tick before the decisive one when `newborn`, before the hold
/// otherwise (`rescan`'s scene).
fn ahead_rescan(arm: ChaseRescanPassOver, newborn: bool) -> (Option<EntityId>, EntityId) {
    let mut s = BattleState::new(0, with_arm(arm));
    past_deploy_lockout(&mut s);
    let lim = limit_on(&s, "Giant");
    let far = n(KNIGHT.0 + 9000, KNIGHT.1 - lim - 200);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(KNIGHT.0, KNIGHT.1), None).expect("the Knight");
    let early = (!newborn).then(|| s.scenario_spawn_now(Team::Blue, "Giant", far, None).expect("the Giant"));
    for _ in 0..3 {
        assert!(s.debug_set_pos(knight, n(KNIGHT.0, KNIGHT.1)) && early.map_or(true, |g| s.debug_set_pos(g, far)));
        s.tick();
        assert_eq!(s.entity(knight).expect("the Knight").target, None, "the scene drifted: the Knight holds a target before the Giant comes near");
    }
    let giant = early.unwrap_or_else(|| s.scenario_spawn_now(Team::Blue, "Giant", far, None).expect("the Giant"));
    assert!(s.debug_set_pos(knight, n(KNIGHT.0, KNIGHT.1)) && s.debug_set_pos(giant, far));
    s.tick();
    assert_eq!(s.entity(knight).expect("the Knight").target, None, "the scene drifted: the Knight took the Giant far across");
    assert!(s.debug_set_pos(knight, n(KNIGHT.0, KNIGHT.1)));
    // In round sight (|dx| 400), past the limit by 300, |dy| 100 more than the tick before.
    assert!(s.debug_set_pos(giant, n(KNIGHT.0 + 400, KNIGHT.1 - lim - 300)));
    s.tick();
    (s.entity(knight).expect("the Knight").target, giant)
}

/// Plant: recede_reads_newborn.
#[test]
fn a_troop_born_the_tick_before_never_recedes() {
    for arm in [ChaseRescanPassOver::Client15535RecedingLaneWalk, ChaseRescanPassOver::Client15535RecedingOrBehind] {
        // NOT VACUOUS: the same Giant on the board from before the hold recedes and is passed over.
        let (old, _) = ahead_rescan(arm, false);
        assert_eq!(old, None, "{arm:?}: the Knight took a receding Giant ahead of it");
        let (took, giant) = ahead_rescan(arm, true);
        assert_eq!(took, Some(giant), "{arm:?}: the Knight passed over a Giant born the tick before");
    }
}
