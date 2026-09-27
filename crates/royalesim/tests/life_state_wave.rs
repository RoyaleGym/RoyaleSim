//! spawner.LIFE_STATE_FIRST_UPDATE and spawner.LIFE_STATE_WAVE_POINT, read off the engine: a Goblin Hut's wave takes no
//! update on its creation tick (state.rs `life_state_pass`), and its point is the line to the aim scaled to SpawnOffset
//! first and turned after (`life_wave`).
//!
//! THE LAW, measured on client 15.535.29 (169 of 169 waves in 55 battery runs) and on the 16.402 corpus (79 of 79 rows
//! whose creation tick is recorded; a row is one seat's recording, and the 79 are about 66 distinct waves in 10
//! battles): the controller creates each wave SpawnOffset (1200) from the hut's centre, inside
//! the hut's circle, and on its first frame the wave stands there with avoidance offset 0; the contact law pushes it out
//! from the next frame, and its first avoidance scan reads +-190 there. Today's engine gives the wave its first update
//! on the creation tick, so its first frame already stands about 1,350 out with +-190. The point: the client scales the
//! line to the aim to 1200 first, each axis truncated toward zero, then turns it by the 1024 sine table, each axis
//! truncated toward zero (169 of 169, and 76 of 79 rows on the corpus); today's one division fits 43 and 20.
//!
//! The scene is tests/test_life_state_wave.py's: a blue hut at (9000, 7000) and a red Cannon at (14000, 12000), in the
//! hut's reach from the start, 7071 from it. The line (5000, 5000) puts the first wave, on the lower-y side, at
//! (10086, 7506) by the client's arithmetic and at (10087, 7507) by the one division. WHAT IS PINNED:
//!   1. client16402_next_tick: on its first frame the first wave stands 1195 to 1200 from the hut with offset 0, and on
//!      its second frame at least 1340 out with +-190;
//!   2. creation_tick: on its first frame it already stands at least 1340 out with +-190 (today's engine);
//!   3. client16402_normalise_then_rotate, with client16402_next_tick: the first wave stands at (10086, 7506);
//!   4. one_division, with client16402_next_tick (so the first frame is unpushed; not today's engine): at
//!      (10087, 7507);
//!   5. the shipped values are the old arms.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test life_state_wave`):
//!   * `life_wave_first_update` -- the new arm still gives the wave its creation-tick update: (1) and (3) go red.
//!   * `life_point_one_division` -- the new arm keeps the one-division arithmetic: (3) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, LifeStateFirstUpdate, LifeStateWavePoint};
use royalesim::Team;

const HUT_AT: (i32, i32) = (9000, 7000);
const CANNON_AT: (i32, i32) = (14000, 12000);
/// SpawnOffset of the hut's controller, native.
const OFFSET: i64 = 1200;
/// One contact push at the 150 cap, less a margin for the truncations, native.
const PUSHED: i64 = 1340;
const TICKS: u32 = 60;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn with_arms(first: LifeStateFirstUpdate, point: LifeStateWavePoint) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.life_state_first_update = first;
    cfg.calib.life_state_wave_point = point;
    cfg
}

/// One frame of the first wave: its position in native units, its distance from the hut's centre (native, floored)
/// and its avoidance offset.
#[derive(Debug)]
struct Frame {
    pos: (i32, i32),
    dist: i64,
    offset: i32,
}

/// The first wave's first two frames, a blue hut played at HUT_AT with the red Cannon set down first.
fn first_frames(first: LifeStateFirstUpdate, point: LifeStateWavePoint) -> (Frame, Frame) {
    let mut s = BattleState::new(0, with_arms(first, point));
    s.scenario_spawn_now(Team::Red, "Cannon", at(CANNON_AT), None).expect("spawn the Cannon");
    s.spawn_unit(Team::Blue, "GoblinHut", at(HUT_AT), None).expect("play the hut");
    let mut rows: Vec<Frame> = Vec::new();
    let mut wave = None;
    for _ in 0..TICKS {
        s.tick();
        if wave.is_none() {
            wave = s.entities().find(|v| v.card == "SpearGoblin_Dummy").map(|v| v.id);
        }
        let Some(id) = wave else { continue };
        let v = s.entities().find(|v| v.id == id).expect("the first wave lives through its second frame");
        let pos = (v.pos.x / K, v.pos.y / K);
        let (dx, dy) = ((pos.0 - HUT_AT.0) as i64, (pos.1 - HUT_AT.1) as i64);
        rows.push(Frame { pos, dist: isqrt(dx * dx + dy * dy), offset: v.avoid_offset });
        if rows.len() == 2 {
            let second = rows.pop().unwrap();
            let first = rows.pop().unwrap();
            return (first, second);
        }
    }
    panic!("the scene drifted: no wave in {TICKS} ticks");
}

/// Plant: life_wave_first_update.
#[test]
fn a_wave_stands_on_its_creation_point_on_its_first_frame() {
    let (f0, f1) = first_frames(LifeStateFirstUpdate::NextTick, LifeStateWavePoint::OneDivision);
    assert!(
        (1195..=OFFSET).contains(&f0.dist) && f0.offset == 0,
        "client16402_next_tick: on its first frame the wave stands {} from the hut with offset {} ({:?}); on its \
         creation point it would stand 1195-1200 out with offset 0",
        f0.dist,
        f0.offset,
        f0.pos
    );
    assert!(
        f1.dist >= PUSHED && f1.offset.abs() == 190,
        "client16402_next_tick: on its second frame the wave stands {} out with offset {}; its first push and scan \
         come there",
        f1.dist,
        f1.offset
    );
}

#[test]
fn the_old_first_update_arm_is_todays_engine() {
    let (f0, _) = first_frames(LifeStateFirstUpdate::CreationTick, LifeStateWavePoint::OneDivision);
    assert!(
        f0.dist >= PUSHED && f0.offset.abs() == 190,
        "creation_tick: on its first frame the wave stands {} out with offset {}; today it is pushed and scanned there",
        f0.dist,
        f0.offset
    );
}

/// Plants: life_point_one_division, life_wave_first_update.
#[test]
fn the_wave_point_is_normalised_then_rotated() {
    let (f0, _) = first_frames(LifeStateFirstUpdate::NextTick, LifeStateWavePoint::NormaliseThenRotate);
    assert_eq!(f0.pos, (10086, 7506), "client16402_normalise_then_rotate: the first wave's point (one division: (10087, 7507))");
}

/// The old point arm's arithmetic, read on the wave's creation point: FIRST_UPDATE is set to the new arm so the first
/// frame is unpushed. This is not today's engine, whose wave is already pushed on its first frame.
#[test]
fn the_one_division_arm_puts_the_unpushed_wave_on_its_point() {
    let (f0, _) = first_frames(LifeStateFirstUpdate::NextTick, LifeStateWavePoint::OneDivision);
    assert_eq!(f0.pos, (10087, 7507), "one_division: the first wave's point");
}

#[test]
fn both_keys_ship_at_their_old_arms() {
    let c = Calib::shipped();
    assert_eq!(c.life_state_first_update, LifeStateFirstUpdate::CreationTick);
    assert_eq!(c.life_state_wave_point, LifeStateWavePoint::OneDivision);
}
