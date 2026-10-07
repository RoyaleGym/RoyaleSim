//! spells.CLONE_SLIDE_ROUTE (item 314; state.rs `step_knock_slides`): what a unit's route does when the Clone's slide ends.
//!
//! Client 15.535.29 (every Clone slide of the ub-cl battery and sp-event-GlobalClone-s0, 31 slides): one path node through
//! the slide, none on its last tick, a fresh route on the next. sp-event-GlobalClone-s0 t258: a Knight slid past its next
//! cell walked to the cell after it, where the engine walked the old segment back toward the cell behind it.
//!
//! The scene: a lone Blue Knight walking for Red's tower, a Clone cast on it once it walks a route. On the original's last
//! slide tick (C + 10, the tick its 10th 125 lands) client15535_dropped leaves it no route and plans one on C + 11; kept
//! keeps the route it walked. Plant: clone_slide_route_kept.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, CloneSlideRoute};
use royalesim::Team;

/// Under `arm`: the original's route length on the frame before the cast, on C + 10 and on C + 11, and its moves on C + 1
/// to C + 10 (native).
fn slide_route(arm: CloneSlideRoute) -> (usize, usize, usize, Vec<i32>) {
    let mut cfg = config();
    cfg.calib.clone_slide_route = arm;
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let k = s.scenario_spawn_now(Team::Blue, "Knight", Vec2::new(9000 * K, 9000 * K), None).expect("the Knight");
    for _ in 0..40 {
        s.tick();
        if s.entity(k).is_some_and(|e| !e.deploying && e.route.len() > 1) {
            break;
        }
    }
    let before = s.entity(k).expect("the Knight").route.len();
    assert!(before > 1, "the scene drifted: the Knight walks no route");
    let at = s.entity(k).unwrap().pos;
    s.spawn_unit(Team::Blue, "Clone", at, None).expect("the Clone");
    s.tick();
    assert!(s.entities().any(|e| e.cloned), "the scene drifted: no copy on C");
    let mut prev = s.entity(k).unwrap().pos;
    let mut steps = Vec::new();
    for _ in 0..10 {
        s.tick();
        let p = s.entity(k).expect("the Knight").pos;
        steps.push((p.y - prev.y) / K);
        prev = p;
    }
    let last = s.entity(k).unwrap().route.len();
    s.tick();
    (before, last, s.entity(k).unwrap().route.len(), steps)
}

/// Plant: clone_slide_route_kept.
#[test]
fn the_slides_last_tick_drops_the_route_under_client15535_dropped() {
    let (before, last, next, steps) = slide_route(CloneSlideRoute::Client15535Dropped);
    assert!(steps.iter().all(|d| *d == 125), "the scene drifted: the original did not slide 10 x 125: {steps:?}");
    assert_eq!(last, 0, "client15535_dropped: a route on the slide's last tick (it held {before} before)");
    assert!(next > 0, "client15535_dropped: no fresh route on the tick after the slide");
}

/// The vacuity check: the engine's arm keeps it.
#[test]
fn kept_walks_the_old_route_on() {
    let (before, last, _, _) = slide_route(CloneSlideRoute::Kept);
    assert!(last > 0 && last <= before, "kept: the route on the slide's last tick is {last}, before it {before}");
}

#[test]
fn the_shipped_value_is_kept() {
    assert_eq!(royalesim::state::Calib::shipped().clone_slide_route, CloneSlideRoute::Kept);
}
