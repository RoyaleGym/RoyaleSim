//! status.ATTRACT_WATER_EDGE: whether a ground unit's step with a pull in it stops at a water cell's edge (state.rs
//! `phase_path16402`'s move pass, `pull_edge`; move16402.rs `grid_move`'s flag).
//!
//! THE READING (client 15.535.29, sp-form-Valkyrie-evo-s0): the Evo Valkyrie's riding tornado pulled a Skeleton walking the
//! river bank, the Musketeer and the Knight on the bridge toward her; each stood on a water cell's edge (y 17,000, or the
//! bridge's x 13,500) every tick the pull carried it at the river, 31 unit-ticks, and went on along the edge.
//!
//! The scene: a red Knight put down deployed on the bank at (11000, 17400), a blue Tornado cast 1,400 south of it, in the
//! river. Pinned: the Knight's lowest y over the pull; under client15535_pull_stops it is the edge, 17,000, on several
//! ticks; under walk_rule the pull carries it into the river.
//!
//! PLANT (regression): pull_water_edge_ignored -> `a_pulled_ground_unit_stops_at_the_water` red.
//!   RUSTFLAGS='--cfg clash_plant="pull_water_edge_ignored"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test attract_water_edge
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AttractWaterEdge, BattleConfig, BattleState, Calib};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// (the Knight's lowest y over the 30 ticks from the cast, native; the ticks it stood on y 17,000).
fn scene(arm: AttractWaterEdge) -> (i32, usize) {
    let mut cfg: BattleConfig = config();
    cfg.calib.attract_water_edge = arm;
    let mut s = BattleState::new(0, cfg);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(11000, 17400), None).expect("the Knight");
    s.tick();
    s.spawn_unit(Team::Blue, "Tornado", n(11000, 16000), None).expect("cast the Tornado");
    let (mut low, mut on_edge) = (i32::MAX, 0);
    for _ in 0..30 {
        s.tick();
        let y = s.entity(knight).expect("the scene drifted: the Knight died").pos.y / K;
        low = low.min(y);
        if y == 17000 {
            on_edge += 1;
        }
    }
    (low, on_edge)
}

/// Plant: pull_water_edge_ignored.
#[test]
fn a_pulled_ground_unit_stops_at_the_water() {
    let (low, on_edge) = scene(AttractWaterEdge::Client15535PullStops);
    assert_eq!(low, 17000, "client15535_pull_stops: the Knight's lowest y");
    assert!(on_edge >= 5, "client15535_pull_stops: the Knight stood on the edge on {on_edge} ticks");
}

#[test]
fn the_old_arm_pulls_it_into_the_river() {
    let (low, _) = scene(AttractWaterEdge::WalkRule);
    assert!(low < 17000, "walk_rule: the Knight's lowest y {low} (vacuous: the pull never reached the river)");
}

#[test]
fn the_shipped_arm_is_the_walk_rule() {
    assert_eq!(Calib::shipped().attract_water_edge, AttractWaterEdge::WalkRule);
}
