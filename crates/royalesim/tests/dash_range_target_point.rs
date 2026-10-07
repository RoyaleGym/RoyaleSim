//! THE TARGET POINT A DASH'S RANGE TEST READS -- calibration combat.DASH_RANGE_TARGET_POINT, state.rs `phase_path16402`
//! (the dash half-steps).
//!
//! THE READING (client 15.535.29, 14 of 14 checkable dash ends): each half-step of a Bandit's dash tests Range + both radii
//! against its target's point in the move pass so far: moved when the target updated before the dasher, its start-of-tick
//! point otherwise. The engine's old arm reads the start-of-tick point (13 of 14: sp-champ-BossBandit-s0 t224, a Skeleton
//! walking at the Boss Bandit, which the client's dash met a half-step early).
//!
//! The scene: a Blue Bandit walking up the right lane at a Red Skeleton walking down it at him (Blue's princess towers
//! down). Once the Bandit stands for his dash, he is set, before every tick until his first move, straight behind the
//! Skeleton at his Range + both radii + a half-step + 15 (outside its own reach on him, 1,600, so it walks on, its goal
//! cell following him; a target set by hand instead walks a route planned before the move): on the first move's tick the
//! start-of-tick reading misses on the first half-step by 15 and takes the second; the pass reading meets the Skeleton
//! after its own step toward him on the first.
//!   the Skeleton created BEFORE the Bandit (it moves first in the pass): client15535_pass_point moves him one half-step
//!   on that tick, start_of_tick two;
//!   the Skeleton created AFTER the Bandit: both arms two (its pass point is its start-of-tick point at his turn).
//!
//! PLANT (regression): dash_range_reads_start -> `a_dash_meets_a_target_that_moved_first_in_the_pass_under_client15535_pass_point` red.
//!   RUSTFLAGS='--cfg clash_plant="dash_range_reads_start"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   dash_range_target_point
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DashRangeTargetPoint};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Bandit's move (native) on his dash's first moving tick, the Skeleton placed as the header says; `giant_first`: the
/// Skeleton created before the Bandit.
fn first_dash_move(arm: DashRangeTargetPoint, giant_first: bool) -> i64 {
    let mut cfg = config();
    cfg.calib.dash_range_target_point = arm;
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).expect("a tower");
    s.scenario_set_tower_hp(Team::Blue, 2, 0).expect("a tower");
    let (giant_at, bandit_at) = (at((14500, 18000)), at((14500, 8500)));
    // scenario_spawn_now in the order named (scenario_spawn_batch would sort the teams, Blue first).
    let (b, g) = if giant_first {
        let g = s.scenario_spawn_now(Team::Red, "Skeleton", giant_at, None).expect("the Skeleton");
        (s.scenario_spawn_now(Team::Blue, "Assassin", bandit_at, None).expect("the Bandit"), g)
    } else {
        let b = s.scenario_spawn_now(Team::Blue, "Assassin", bandit_at, None).expect("the Bandit");
        (b, s.scenario_spawn_now(Team::Red, "Skeleton", giant_at, None).expect("the Skeleton"))
    };
    let reach = {
        let idx = s.cards().index("Assassin").expect("the Bandit");
        s.cards().get(idx).range + s.entity(b).expect("the Bandit").radius + s.entity(g).expect("the Skeleton").radius
    };
    let mut walked = false;
    let mut standing = 0;
    for _ in 0..300 {
        let p0 = s.entity(b).expect("the Bandit").pos;
        let mut p0 = p0;
        if standing >= 2 {
            let t0 = s.entity(g).expect("the Skeleton").pos;
            p0 = Vec2::new(t0.x, t0.y - reach - (250 + 15) * K);
            assert!(s.debug_set_pos(b, p0));
        }
        s.tick();
        let p1 = s.entity(b).expect("the Bandit").pos;
        let moved = isqrt(p0.dist2(p1)) / K as i64;
        if moved == 0 {
            if walked {
                standing += 1;
            }
        } else if standing >= 2 {
            return moved;
        } else {
            walked = true;
            standing = 0;
        }
    }
    panic!("{arm:?}: the scene drifted: no dash");
}

/// Plant: dash_range_reads_start.
#[test]
fn a_dash_meets_a_target_that_moved_first_in_the_pass_under_client15535_pass_point() {
    let new = first_dash_move(DashRangeTargetPoint::Client15535PassPoint, true);
    assert!((240..=252).contains(&new), "client15535_pass_point: not one half-step on the first move ({new})");
    // NOT VACUOUS: start_of_tick takes the second half-step.
    let old = first_dash_move(DashRangeTargetPoint::StartOfTick, true);
    assert!((490..=502).contains(&old), "start_of_tick: not two half-steps on the first move ({old})");
}

#[test]
fn a_target_updated_after_the_dasher_reads_alike_under_both_arms() {
    assert_eq!(
        first_dash_move(DashRangeTargetPoint::Client15535PassPoint, false),
        first_dash_move(DashRangeTargetPoint::StartOfTick, false),
        "the arms part with the Skeleton created after the Bandit"
    );
}

#[test]
fn the_shipped_arm_reads_the_start_of_tick_point() {
    assert_eq!(Calib::shipped().dash_range_target_point, DashRangeTargetPoint::StartOfTick);
}
