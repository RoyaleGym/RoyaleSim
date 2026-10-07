//! A PULL AMONG CONTACTS -- calibration collision.ATTRACT_CONTACT_MEAN, state.rs `phase_path16402` (the walk step's
//! `pull`) and move16402.rs `move_towards_extra`.
//!
//! THE READING (client 15.535.29, 32 of 32 pulled ground-troop steps with an overlapping troop neighbour): each pulling
//! source is one entry of the contact accumulator, so the step is the walk plus tdiv(pull + the pushes, sources +
//! contacts); with no contact the pull is applied whole (the Tornado's 432 on a Hog Rider). The engine's old arm added
//! the pull after the contact mean (0 of 32): sp-form-Wizard-hero-s0's Musketeer pulled into a Knight came 131 closer to
//! it than the client's in three ticks.
//!
//! The scene: a Blue Knight on (9000, 9000) walking north, a second Blue Knight held at (9000, 10300), a Red Tornado cast
//! 900 north of the first on its fourth tick: the first is pulled whole for a tick or two (no contact), then into the
//! second. The first Knight's step per tick under both arms:
//!   the pull-only steps agree (above 150: the pull is whole, not capped);
//!   the arms part on the first tick the two touch.
//!
//! PLANT (regression): attract_after_mean -> `a_pulled_unit_takes_the_pull_into_its_contact_mean_under_client15535_in_mean` red.
//!   RUSTFLAGS='--cfg clash_plant="attract_after_mean"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   attract_contact_mean
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AttractContactMean, BattleConfig, BattleState, Calib};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const HELD_AT: (i32, i32) = (9000, 10300);

/// The first Knight's step per tick (native) over the Tornado's life, under `arm`.
fn steps(arm: AttractContactMean) -> Vec<(i32, i32)> {
    let mut cfg: BattleConfig = config();
    cfg.calib.attract_contact_mean = arm;
    let mut s = BattleState::new(0, cfg);
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", at((9000, 9000)), None).expect("the Knight");
    let held = s.scenario_spawn_now(Team::Blue, "Knight", at(HELD_AT), None).expect("the held Knight");
    for _ in 0..3 {
        assert!(s.debug_set_pos(held, at(HELD_AT)));
        s.tick();
    }
    let p = s.entity(knight).expect("the Knight").pos;
    s.spawn_unit(Team::Red, "Tornado", Vec2::new(p.x, p.y + 900 * K), None).expect("cast the Tornado");
    let mut out = Vec::new();
    for _ in 0..24 {
        assert!(s.debug_set_pos(held, at(HELD_AT)));
        let a = s.entity(knight).expect("the scene drifted: the Knight died").pos;
        s.tick();
        let b = s.entity(knight).expect("the scene drifted: the Knight died").pos;
        out.push((b.x / K - a.x / K, b.y / K - a.y / K));
    }
    out
}

/// Plant: attract_after_mean.
#[test]
fn a_pulled_unit_takes_the_pull_into_its_contact_mean_under_client15535_in_mean() {
    let new = steps(AttractContactMean::Client15535InMean);
    let old = steps(AttractContactMean::AfterMean);
    let part = (0..new.len()).find(|&k| new[k] != old[k]).expect("NOT VACUOUS: the arms never part (no pulled step with a contact)");
    let len = |v: (i32, i32)| isqrt(i64::from(v.0) * i64::from(v.0) + i64::from(v.1) * i64::from(v.1));
    assert!(new[..part].iter().any(|&v| len(v) > 150), "the scene drifted: no whole pull before the contact ({new:?})");
    // The held Knight stands north: its push on the pulled one is south, the pull north; in the mean the pull is halved
    // with the push, so the step north is shorter than the after-mean's walk + capped push + whole pull.
    assert!(new[part].1 < old[part].1, "client15535_in_mean: the step on the first contact tick is not shorter north ({:?} vs {:?})", new[part], old[part]);
}

#[test]
fn the_shipped_arm_adds_the_pull_after_the_mean() {
    assert_eq!(Calib::shipped().attract_contact_mean, AttractContactMean::AfterMean);
}
