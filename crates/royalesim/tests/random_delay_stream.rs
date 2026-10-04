//! A RANDOMDELAY SHOT'S DELAY DRAWS FROM THE CLIENT'S BATTLE GENERATOR -- calibration combat.RANDOM_DELAY_STREAM,
//! combat.rs `fire`, state.rs `client_draw`.
//!
//! THE READING (client 15.535.29, every complete Hunter volley in the captures, 56 of 56): a volley of 10 pellets draws
//! 19 times from the battle generator; in the fan's order each pellet draws U = rnd(RandomDelay 200) and first steps
//! 1 + ceil(U / 50) ticks after its creation, and one more draw comes before the next pellet. sp-f4-hunterG0 t444: from
//! 4014919791 the pellets first step 5, 4, 2, 5, 5, 3, 4, 5, 2, 4 ticks after the volley's creation (holds one less),
//! and the generator reads 3699668171 after.
//!
//! PLANT (regression): random_delay_engine_stream -> `a_volley_draws_its_delays_from_the_battle_stream` red.
//!   RUSTFLAGS='--cfg clash_plant="random_delay_engine_stream"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test random_delay_stream
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, RandomDelayStream};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The holds of a blue Hunter's first volley at a red Knight 3,000 away, the battle generator set to 4014919791 before
/// it; and the generator's state after the volley.
fn volley(arm: RandomDelayStream) -> (Vec<i32>, Option<u32>) {
    let mut cfg = config();
    cfg.calib.random_delay_stream = arm;
    let mut s = BattleState::new(3, cfg);
    let hunter = s.scenario_spawn_now(Team::Blue, "Hunter", at((9000, 6500)), None).expect("the Hunter");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 9500)), Some(100_000)).expect("the Knight");
    s.scenario_set_client_rng(4014919791);
    for _ in 0..200 {
        assert!(s.debug_set_pos(knight, at((9000, 9500))));
        s.tick();
        let mine: Vec<i32> = s.projectiles().iter().filter(|p| p.firer == Some(hunter)).filter_map(|p| p.straight.as_ref().map(|st| st.hold)).collect();
        if !mine.is_empty() {
            return (mine, s.client_rng_state());
        }
    }
    panic!("the scene drifted: the Hunter never fired");
}

/// Plant: random_delay_engine_stream.
#[test]
fn a_volley_draws_its_delays_from_the_battle_stream() {
    let (holds, after) = volley(RandomDelayStream::Client15535BattleStream);
    assert_eq!(holds, vec![4, 3, 1, 4, 4, 2, 3, 4, 1, 3], "the ten pellets' holds in the fan's order");
    assert_eq!(after, Some(3699668171), "the generator after the volley's 19 draws");
}

/// Both clients draw a volley's delays from the battle stream (client 15.535.29: 56 of 56 volleys; client 16.402: 9 of 9).
#[test]
fn the_shipped_arm_draws_from_the_battle_stream() {
    assert_eq!(Calib::shipped().random_delay_stream, RandomDelayStream::Client15535BattleStream);
}

/// The old arm (the vacuity check): the volley leaves the battle generator where it was.
#[test]
fn the_old_arm_leaves_the_battle_stream() {
    let (_, after) = volley(RandomDelayStream::EngineOwn);
    assert_eq!(after, Some(4014919791), "the old arm drew from the battle generator");
}
