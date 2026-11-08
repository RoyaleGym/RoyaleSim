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
use royalesim::state::{BattleState, Calib, LoadFirstHitCorpseSwitch, LoadFirstHitHold, LoadFirstHitKillWait, LoadFirstHitLeave};
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
    assert_eq!(Calib::shipped().load_first_hit_leave, LoadFirstHitLeave::Client15535RefundAtDeath);
}

/// Under combat.LOAD_FIRST_HIT_HOLD = `arm`: a blue Sparky at (9000, 6500), charged with no enemy in sight (its load timer 0),
/// is Zapped on its point; a red Knight (100,000 hp) is put 4,000 ahead on the Zap's first held tick and held there. (its load
/// timer on every held tick, its progress on its first tick in its attack after the hold).
fn zapped_sparky(arm: LoadFirstHitHold) -> (Vec<i32>, i32) {
    let mut cfg = config();
    cfg.calib.load_first_hit_hold = arm;
    let mut s = BattleState::new(3, cfg);
    past_deploy_lockout(&mut s);
    let sparky = s.scenario_spawn_now(Team::Blue, "ZapMachine", at((9000, 6500)), None).expect("the Sparky");
    run_until(&mut s, 200, |s| s.entity(sparky).is_some_and(|v| v.attack_load_ms == 0));
    assert_eq!(s.entity(sparky).unwrap().attack_load_ms, 0, "the scene drifted: the Sparky never charged");
    let p = s.entity(sparky).unwrap().pos;
    s.spawn_unit(Team::Red, "Zap", p, None).expect("the Zap");
    let (mut loads, mut knight) = (Vec::new(), None);
    for _ in 0..120 {
        if let Some(k) = knight {
            assert!(s.debug_set_pos(k, at((9000, 10500))));
        }
        s.tick();
        let v = s.entity(sparky).expect("the Sparky");
        if v.stun_ms > 0 {
            loads.push(v.attack_load_ms);
            if knight.is_none() {
                knight = Some(s.scenario_spawn_now(Team::Red, "Knight", at((9000, 10500)), Some(100_000)).expect("the Knight"));
            }
        } else if !loads.is_empty() && v.attack_ms > 0 {
            return (loads, v.attack_ms);
        }
    }
    panic!("{arm:?}: the scene drifted: held {} ticks, no attack after", loads.len());
}

/// combat.LOAD_FIRST_HIT_HOLD = client16402_hold_recharges (client 16.402, parity r82, live13: the timer read LoadTime on the
/// first held tick in 41 of 41 Sparky holds; it entered at progress 100 after). Plant: hold_keeps_sparky_charge.
#[test]
fn under_client16402_hold_recharges_a_zapped_sparky_charges_again() {
    let (loads, entry) = zapped_sparky(LoadFirstHitHold::Client16402HoldRecharges);
    assert!(loads.len() >= 8, "the scene drifted: the Zap held the Sparky {} ticks", loads.len());
    assert!(loads.iter().all(|&l| l == 3000), "client16402_hold_recharges: the held Sparky's timer reads LoadTime: {loads:?}");
    assert!(entry <= 300, "client16402_hold_recharges: it enters charging again: {entry}");
    // NOT VACUOUS: held keeps its charge through the hold and fires at once after it
    let (l0, e0) = zapped_sparky(LoadFirstHitHold::Held);
    assert!(l0.iter().all(|&l| l == 0) && e0 >= 3000, "held: {l0:?}, entry {e0}");
}

/// Since parity r82 (held before it; client 15.535.29 keeps held).
#[test]
fn the_shipped_load_first_hit_hold_is_client16402_hold_recharges() {
    assert_eq!(Calib::shipped().load_first_hit_hold, LoadFirstHitHold::Client16402HoldRecharges);
}


/// Under combat.LOAD_FIRST_HIT_CORPSE_SWITCH = `arm`: the kill scene with a second red Knight (BIG hp) held beside the first, in
/// the Sparky's reach: (its progress on the death tick, its (progress, load timer) on the tick after, the tick of its first shot).
fn corpse_switch(arm: LoadFirstHitCorpseSwitch) -> (i32, (i32, i32), u32) {
    let mut cfg = config();
    cfg.calib.load_first_hit_corpse_switch = arm;
    cfg.calib.load_first_hit_kill_wait = LoadFirstHitKillWait::Client15535Skipped;
    let mut s = BattleState::new(3, cfg);
    let sparky = s.scenario_spawn_now(Team::Blue, "ZapMachine", at((9000, 6500)), None).expect("the Sparky");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at((9000, 10500)), Some(100_000)).expect("the Knight");
    let second = s.scenario_spawn_now(Team::Red, "Knight", at((9600, 10500)), Some(100_000)).expect("the second Knight");
    let mut ticks_in = 0;
    for _ in 0..200 {
        assert!(s.debug_set_pos(second, at((9600, 10500))));
        if s.entity(knight).is_some() {
            assert!(s.debug_set_pos(knight, at((9000, 10500))));
        }
        if ticks_in == 4 {
            assert!(s.debug_set_hp(knight, 1));
            s.spawn_unit(Team::Blue, "Zap", at((9000, 10200)), None).expect("the Zap");
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
    assert!(s.entity(second).is_some(), "the scene drifted: the Zap killed the second Knight");
    let p = s.entity(sparky).expect("the Sparky").attack_ms;
    let shots = |s: &BattleState| s.projectiles().iter().filter(|q| q.firer == Some(sparky)).count();
    let known = shots(&s);
    assert!(s.debug_set_pos(second, at((9600, 10500))));
    s.tick();
    let v = s.entity(sparky).expect("the Sparky");
    let after = (v.attack_ms, v.attack_load_ms);
    for _ in 0..120 {
        assert!(s.debug_set_pos(second, at((9600, 10500))));
        s.tick();
        if shots(&s) > known {
            return (p, after, s.tick_count() - 1);
        }
    }
    panic!("{arm:?}: the scene drifted: no shot at the second Knight (death-tick progress {p}, after {after:?})");
}

/// combat.LOAD_FIRST_HIT_CORPSE_SWITCH = client16635_refund_and_reenter (client 16.635, parity r90, live15n 20261103-174813-A: 5
/// of 5 Sparky corpse switches re-entered with the timer at LoadTime): the Sparky whose Knight dies mid-windup with a second
/// Knight in reach enters fresh on it, its load timer at LoadTime, where under kept_in_reach (the vacuity check) its swing runs
/// on with the timer as it was; the shots part. Plant: lfh_corpse_switch_kept.
#[test]
fn under_client16635_refund_and_reenter_a_sparky_enters_fresh_on_a_corpses_replacement_in_reach() {
    let lt = config().cards.get(config().cards.index("ZapMachine").expect("the Sparky's row")).load_time_ms;
    let (p0, old, old_shot) = corpse_switch(LoadFirstHitCorpseSwitch::KeptInReach);
    let (p1, new, new_shot) = corpse_switch(LoadFirstHitCorpseSwitch::Client16635RefundAndReenter);
    assert_eq!(p0, p1, "the scene drifted between the arms");
    assert!(old.0 > p0, "kept_in_reach (the vacuity check): the swing did not run on ({old:?}, death-tick progress {p0})");
    assert_eq!(new.1, lt, "client16635_refund_and_reenter: the timer on the re-entry is not LoadTime ({new:?}, death-tick progress {p1})");
    assert_ne!((new, new_shot), (old, old_shot), "the arms do not part: (progress, load) {new:?}, shot {new_shot}");
}
