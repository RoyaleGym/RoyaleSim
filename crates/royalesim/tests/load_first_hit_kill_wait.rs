//! combat.LOAD_FIRST_HIT_KILL_WAIT: whether a LoadFirstHit unit (the Sparky) serves combat.POST_KILL_RETARGET_WAIT's wait
//! when its target dies (state.rs `phase_target`, the wait's clause (d)).
//!
//! THE READING (client 15.535.29, every Sparky whose target died mid-swing, 4 of 4: sp-il-8b9b t880 and t959, sp-il-04cb
//! t1761 and t1870): it took its next target on the tick after the loss and stepped on the next; under the shipped wait
//! (client16402_attack_finish: progress not 0, no override, the victim not doomed by its own shot) the engine's stood 6
//! ticks. sp-il-8b9b t959: the Sparky's Cannon struck down by others mid-windup; the engine's Sparky reached its next
//! target 5 ticks late and its shot killed the Musketeer late (the scene's first divergence, t992).
//!
//! The scene: a blue Sparky locks red Knight A 4,000 away, red Knight B stands 1,100 behind A; five ticks into the
//! windup A falls to 0 hitpoints; run at match.TICK_ORDER = client_sequential_strike, as a 15.535.29 capture is.
//!
//! PLANT (regression): load_first_hit_kill_waits -> `a_sparky_whose_target_dies_mid_swing_takes_the_next_at_once` red.
//!   RUSTFLAGS='--cfg clash_plant="load_first_hit_kill_waits"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test load_first_hit_kill_wait
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, LoadFirstHitKillWait, TickOrder};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const A: (i32, i32) = (9000, 10500);
const B: (i32, i32) = (9000, 11600);

/// The ticks from A's fall to the Sparky's first tick on B (1 = the tick after the fall's).
fn scene(arm: LoadFirstHitKillWait) -> usize {
    let mut cfg = config();
    cfg.calib.load_first_hit_kill_wait = arm;
    // the 15.535.29 harness's match.TICK_ORDER (examples/replay_parity/harness.rs CLIENT15535_ARMS), where the corpus
    // Sparkies waited; under the shipped order the debug fall starts no wait under either arm
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    let mut s = BattleState::new(3, cfg);
    let sparky = s.scenario_spawn_now(Team::Blue, "ZapMachine", at((9000, 6500)), None).expect("the Sparky");
    let a = s.scenario_spawn_now(Team::Red, "Knight", at(A), Some(100_000)).expect("Knight A");
    let b: EntityId = s.scenario_spawn_now(Team::Red, "Knight", at(B), Some(100_000)).expect("Knight B");
    let mut ticks_in = 0;
    for _ in 0..200 {
        assert!(s.debug_set_pos(a, at(A)));
        assert!(s.debug_set_pos(b, at(B)));
        s.tick();
        let v = s.entity(sparky).expect("the Sparky");
        if v.attack_ms > 0 {
            assert_eq!(v.target, Some(a), "the scene drifted: the Sparky's swing is not on Knight A");
            ticks_in += 1;
            if ticks_in == 5 {
                break;
            }
        }
    }
    assert_eq!(ticks_in, 5, "the scene drifted: the Sparky never locked Knight A");
    let v = s.entity(sparky).expect("the Sparky");
    assert!(v.attack_ms < s.config().cards.get(v.card_idx).hit_speed_ms, "the scene drifted: it fired before the fall");
    assert!(s.debug_set_hp(a, 0));
    for k in 1..=12 {
        assert!(s.debug_set_pos(b, at(B)), "Knight B is gone");
        s.tick();
        if s.entity(sparky).expect("the Sparky").target == Some(b) {
            return k;
        }
    }
    panic!("the Sparky never took Knight B within 12 ticks of A's fall");
}

/// Plant: load_first_hit_kill_waits.
#[test]
fn a_sparky_whose_target_dies_mid_swing_takes_the_next_at_once() {
    assert_eq!(scene(LoadFirstHitKillWait::Client15535Skipped), 1, "client15535_skipped: the tick of the Sparky's first lock on B");
    assert!(scene(LoadFirstHitKillWait::Waits) >= 6, "waits: the Sparky served no post-kill wait");
}

#[test]
fn the_shipped_arm_waits() {
    assert_eq!(Calib::shipped().load_first_hit_kill_wait, LoadFirstHitKillWait::Waits);
}
