//! A SPARKY LEAVING ITS ATTACK BEFORE IT FIRES -- calibration combat.LOAD_FIRST_HIT_LEAVE, combat.rs
//! `attack_step_progress`.
//!
//! THE READING (client 15.535.29, every Sparky leave before a fire): a LoadFirstHit unit's lock resets its load timer to
//! LoadTime and credits its progress with the charge it walked with (combat.LOAD_FIRST_HIT); if it leaves the attack
//! before the attack fires, its load timer reads LoadTime less its progress, floored at 0: the windup is given back.
//! sp-il-8b9b: charged (timer 0 since t860), it locked an Ice Spirit on t871 at progress 3050, lost it to a slap on t880
//! (progress 3450 the tick before; load -450, then 0), locked the Ice Golem on t891 at progress 3050 again and fired on
//! t910. sp-il-04cb t1870: progress 1500, load 1500. The engine's timer ran on from the reset (2550 at t880), so its
//! relock started at 1050 and its shot came some 40 ticks late, il-8b9b's first divergence.
//!
//! The scene: a blue Sparky locks a red Knight 4,000 away; five ticks into the windup the Knight is carried out of its
//! sight (the slap's case: the target lost, not killed), and on the next tick back into reach.
//!
//! THE DEATH TICK (item 321, client15535_refund_at_death): a target that dies after the Sparky's own turn on tick t is given
//! up on t itself, the windup back on t, where the Target phase finds it dead on t + 1: sp-il-8b9b t959 (its Cannon killed
//! by a unit created after it: load 500 = 3000 - 2500 on t959, 450 on t960; the engine 500 on t960, a tick late from
//! there), sp-il-04cb t1761 (zapped after its turn). The kill scene: the same lock, and five ticks into the windup the
//! Knight, its hitpoints set to 1, is zapped (the Zap's damage lands in Resolve, after every unit's turn).
//!
//! PLANT (regression): load_first_hit_leave_runs_on -> `a_leaving_sparky_gets_its_windup_back` red.
//!   RUSTFLAGS='--cfg clash_plant="load_first_hit_leave_runs_on"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test load_first_hit_leave
//! PLANT: refund_at_death_counts_now -> `a_sparky_whose_target_died_after_its_turn_is_refunded_on_the_death_tick` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, LoadFirstHitKillWait, LoadFirstHitLeave};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// (the Sparky's progress on the tick before the leave, its load timer on the leave tick, its progress on the tick it
/// locks the Knight again).
fn scene(arm: LoadFirstHitLeave) -> (i32, i32, i32) {
    let mut cfg = config();
    cfg.calib.load_first_hit_leave = arm;
    let mut s = BattleState::new(3, cfg);
    let sparky = s.scenario_spawn_now(Team::Blue, "ZapMachine", at((9000, 6500)), None).expect("the Sparky");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 10500)), Some(100_000)).expect("the Knight");
    let mut ticks_in = 0;
    let mut before = 0;
    for _ in 0..200 {
        assert!(s.debug_set_pos(knight, at((9000, 10500))));
        s.tick();
        let v = s.entity(sparky).expect("the Sparky");
        if v.attack_ms > 0 {
            ticks_in += 1;
            before = v.attack_ms;
            if ticks_in == 5 {
                break;
            }
        }
    }
    assert_eq!(ticks_in, 5, "the scene drifted: the Sparky never locked the Knight");
    assert!(before < s.config().cards.get(s.entity(sparky).unwrap().card_idx).hit_speed_ms, "the scene drifted: it fired before the leave");
    assert!(s.debug_set_pos(knight, at((9000, 28000))));
    s.tick();
    let v = s.entity(sparky).expect("the Sparky");
    assert!(v.target != Some(knight) && v.attack_ms == 0, "the scene drifted: the Sparky kept the far Knight ({:?}, {})", v.target, v.attack_ms);
    let load = v.attack_load_ms;
    let mut entry = 0;
    for _ in 0..60 {
        assert!(s.debug_set_pos(knight, at((9000, 10500))));
        s.tick();
        let v = s.entity(sparky).expect("the Sparky");
        if v.attack_ms > 0 {
            entry = v.attack_ms;
            break;
        }
    }
    assert!(entry > 0, "the scene drifted: the Sparky never locked the Knight again");
    (before, load, entry)
}

/// The kill scene: (the Sparky's progress on the death tick, its load timer on the tick after, which finds the Knight dead).
fn kill_scene(arm: LoadFirstHitLeave) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.load_first_hit_leave = arm;
    // The Sparky serves no post-kill wait (the 15.535.29 lane's combat.LOAD_FIRST_HIT_KILL_WAIT), so it leaves on the tick
    // after the death; under the shipped arm it would hold there frozen.
    cfg.calib.load_first_hit_kill_wait = LoadFirstHitKillWait::Client15535Skipped;
    let mut s = BattleState::new(3, cfg);
    let sparky = s.scenario_spawn_now(Team::Blue, "ZapMachine", at((9000, 6500)), None).expect("the Sparky");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 10500)), Some(100_000)).expect("the Knight");
    let mut ticks_in = 0;
    for _ in 0..200 {
        assert!(s.debug_set_pos(knight, at((9000, 10500))));
        if ticks_in == 4 {
            assert!(s.debug_set_hp(knight, 1));
            s.spawn_unit(Team::Blue, "Zap", at((9000, 10500)), None).expect("the Zap");
        }
        s.tick();
        if s.entity(sparky).expect("the Sparky").attack_ms > 0 {
            ticks_in += 1;
            if ticks_in == 5 {
                break;
            }
        }
    }
    assert_eq!(ticks_in, 5, "the scene drifted: the Sparky never wound up on the Knight");
    assert!(s.entity(knight).is_none(), "the scene drifted: the Knight outlived the Zap");
    let p = s.entity(sparky).expect("the Sparky").attack_ms;
    s.tick();
    let v = s.entity(sparky).expect("the Sparky");
    assert!(v.attack_ms == 0 && v.target != Some(knight), "the scene drifted: the Sparky did not leave on the tick after ({}, {:?})", v.attack_ms, v.target);
    (p, v.attack_load_ms)
}

/// Plant: refund_at_death_counts_now.
#[test]
fn a_sparky_whose_target_died_after_its_turn_is_refunded_on_the_death_tick() {
    let lt = config().cards.get(config().cards.index("ZapMachine").expect("the Sparky's row")).load_time_ms;
    let (p, load) = kill_scene(LoadFirstHitLeave::Client15535RefundAtDeath);
    assert!(lt - p > config().calib.tick_ms, "vacuous: the windup {p} leaves no timer to count ({lt})");
    assert_eq!(load, (lt - p - config().calib.tick_ms).max(0), "refund_at_death: the timer a tick after the death tick (progress {p})");
    // NOT VACUOUS: windup_refunded refunds from the tick that finds it dead.
    let (p, old) = kill_scene(LoadFirstHitLeave::Client15535WindupRefunded);
    assert_eq!(old, (lt - p).max(0), "windup_refunded: the timer on the tick that finds the target dead (progress {p})");
}

/// Plant: load_first_hit_leave_runs_on.
#[test]
fn a_leaving_sparky_gets_its_windup_back() {
    let lt = config().cards.get(config().cards.index("ZapMachine").expect("the Sparky's row")).load_time_ms;
    let (before, load, entry) = scene(LoadFirstHitLeave::Client15535WindupRefunded);
    assert_eq!(load, (lt - before).max(0), "the leave tick's load timer (progress {before} the tick before)");
    let (_, old_load, old_entry) = scene(LoadFirstHitLeave::RunsOn);
    assert!(old_load > load, "vacuous: the old arm's timer {old_load} is not above the refunded {load}");
    assert!(entry > old_entry, "the relock's progress {entry} is not above the old arm's {old_entry}");
}

#[test]
fn the_shipped_arm_runs_the_timer_on() {
    assert_eq!(Calib::shipped().load_first_hit_leave, LoadFirstHitLeave::RunsOn);
}
