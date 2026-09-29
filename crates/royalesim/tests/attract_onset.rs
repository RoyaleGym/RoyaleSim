//! THE TORNADO'S PULL MOVES A UNIT ONE TICK AFTER EACH OF THE AREA'S TICKS -- calibration status.ATTRACT_ONSET,
//! state.rs `phase_path16402` (the attract pre-pass) and spell.rs `step_spells` (`attract_lags`).
//!
//! THE READING (client_next_tick, shipped at its old arm area_first_tick): the pull is written on one tick and moves the
//! unit on the next, so the area's 21 ticks (its LifeDuration 1,050 ms) move a victim on the cast tick D + 1..D + 21,
//! the same 21 moves (status.ATTRACT_LAW's law each), where today's engine moves it on D..D + 20. The damage pulse
//! (D + 11) does not move. Read off client 15.535.29's sweep-Tornado (a Knight pulled on D + 1..D + 21, hit on D + 11,
//! in both engines) and the 16.402 corpus's 20260920-081819, where the client's collision accumulator holds the pull a
//! tick before the unit moves by it. On 16.402 the whole Tornado also starts a tick after its fixture date (pull
//! D + 2..D + 22, damage D + 12), which this key does not model (its ledger entry says why).
//!
//! THE SCENE: a Blue Knight put down deployed at (9000, 9000), walking north; a Red Tornado cast 1,000 north of it on
//! its third tick. A Knight walks 60 a tick and the pull adds 216, so a step above 120 is a pulled one.
//!
//! WHAT IS PINNED (k counts the ticks from the one whose Spawn phase casts the Tornado, D = k 0):
//!   1. area_first_tick: pulled steps on k 0..20, 21 of them;
//!   2. client_next_tick: pulled steps on k 1..21, 21 of them;
//!   3. both arms: the Knight's hp drops on the same tick (the pulse is the area's, not the pull's);
//!   4. the shipped value is area_first_tick.
//!
//! PLANT (regression):
//!   * `attract_onset_area_first_tick` -- the new arm pulls from the area's first tick: (2) goes red.
//!     RUSTFLAGS='--cfg clash_plant="attract_onset_area_first_tick"' CARGO_TARGET_DIR=target/plant cargo test --profile
//!     gate --test attract_onset
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AttractOnset, BattleConfig, BattleState, Calib};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// (the k of every step above 120 native, the k of every hp drop), over k 0..40.
fn pulled(arm: AttractOnset) -> (Vec<u32>, Vec<u32>) {
    let mut cfg: BattleConfig = config();
    cfg.calib.attract_onset = arm;
    let mut s = BattleState::new(0, cfg);
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", at((9000, 9000)), None).expect("the Knight");
    for _ in 0..3 {
        s.tick();
    }
    let p = s.entity(knight).unwrap().pos;
    s.spawn_unit(Team::Red, "Tornado", Vec2::new(p.x, p.y + 1000 * K), None).expect("cast the Tornado");
    let (mut steps, mut drops) = (Vec::new(), Vec::new());
    for k in 0..40u32 {
        let v = s.entity(knight).expect("the scene drifted: the Knight died");
        let (a, hp) = (v.pos, v.hp);
        s.tick();
        let v = s.entity(knight).expect("the scene drifted: the Knight died");
        let (dx, dy) = ((v.pos.x / K - a.x / K) as i64, (v.pos.y / K - a.y / K) as i64);
        if isqrt(dx * dx + dy * dy) > 120 {
            steps.push(k);
        }
        if v.hp < hp {
            drops.push(k);
        }
    }
    (steps, drops)
}

#[test]
fn the_old_arm_pulls_from_the_areas_first_tick() {
    let (steps, _) = pulled(AttractOnset::AreaFirstTick);
    assert_eq!(steps, (0..=20).collect::<Vec<u32>>(), "area_first_tick: the pulled steps");
}

/// Plant: attract_onset_area_first_tick.
#[test]
fn the_new_arm_pulls_one_tick_later_for_the_same_21_moves() {
    let (steps, _) = pulled(AttractOnset::ClientNextTick);
    assert_eq!(steps, (1..=21).collect::<Vec<u32>>(), "client_next_tick: the pulled steps");
}

#[test]
fn the_pulse_lands_on_the_same_tick_under_both_arms() {
    let (_, old) = pulled(AttractOnset::AreaFirstTick);
    let (_, new) = pulled(AttractOnset::ClientNextTick);
    assert_eq!(old, vec![11], "the scene drifted: the Tornado's one pulse");
    assert_eq!(old, new, "the pulse moved with the pull");
}

#[test]
fn the_shipped_value_is_area_first_tick() {
    assert_eq!(Calib::shipped().attract_onset, AttractOnset::AreaFirstTick);
}
