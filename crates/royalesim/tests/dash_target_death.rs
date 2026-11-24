//! combat.DASH_TARGET_DEATH = client16635_half_step_end: where a dash with no DashConstantTime (the Bandit's, the Boss
//! Bandit's) whose target is gone ends (state.rs `dash_runs_to_goal`, `dash_half_step_end`, `dash_short_of_goal`; the move
//! pass's dash and `dash_ends_now`).
//!
//! THE LAW, measured on client 16.635 (parity r99, the oracle's il-mg-16635 set: 3 of 3 Bandit and Boss Bandit runs whose
//! target was gone mid-dash and whose last tick began 500 to 749 from the goal moved 250 on it and ended there, where the
//! engine moved 500): the run goes on toward its goal in half-steps of JumpSpeed / 2, with no range test and no blow, and
//! ends on the first half-step that leaves its goal less than a whole step (JumpSpeed, 500) off, so every run ends 250 to
//! 499 short of it. Under client16635_runs_to_goal the end test runs after both half-steps only: a last tick that began
//! 500 to 749 off moves 500 and ends 0 to 249 short.
//!
//! THE SCENE: tests/dash_attack.rs's Bandit at its Knight (BANDIT_AT, KNIGHT_AT); the Knight put to 0 hitpoints after the
//! Bandit's first dash move; three ticks on (its target gone) the Bandit is put back on its own dash line 1025 + 50k from
//! its goal, k = 0 to 9, so the run's last tick begins at every 50 of the 500 step: 525 to 725 off for k = 0 to 4, 775 to
//! 975 for k = 5 to 9. The goal is the engine's law written out (`dash_goal`: the centre of the 500-cell holding the point
//! both radii short of the Knight's centre on the line to the Bandit), from the dash's entry tick, checked against the
//! first dash move.
//!
//! WHAT IS PINNED: under client16635_half_step_end every run ends 250 to 499 from its goal, and each last tick that began
//! 500 to 749 off moves 245 to 255; under client16635_runs_to_goal (the vacuity check) such a run moves 490 or more on its
//! last tick and ends within 250 of the goal; under both every move before the last is a whole step.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! dash_target_death`):
//!   * `dash_half_step_end_whole_tick` -- the new arm tests the end after both half-steps only:
//!     `under_client16635_half_step_end_a_bandit_whose_target_dies_ends_on_the_half_step` red (its runs end within 250).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DashAttack, DashTargetDeath};
use royalesim::Team;

/// tests/dash_attack.rs's scene: a red Knight that walks to the blue right princess tower, a blue Bandit after it.
const KNIGHT_AT: (i32, i32) = (14231, 9500);
const BANDIT_AT: (i32, i32) = (8200, 6200);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// A subtile point in native units.
fn native(p: Vec2) -> (i64, i64) {
    ((p.x / K) as i64, (p.y / K) as i64)
}

/// The native distance between two native points, truncated (move16402.rs `distance`).
fn dist(a: (i64, i64), b: (i64, i64)) -> i64 {
    let (dx, dy) = (a.0 - b.0, a.1 - b.1);
    isqrt(dx * dx + dy * dy)
}

/// THE DASH GOAL (state.rs `dash_goal_native`, written out), native: the centre of the 500-cell holding the point `radii`
/// short of the target's centre `t` on the line to the dasher's `p`.
fn dash_goal(p: (i64, i64), t: (i64, i64), radii: i64) -> (i64, i64) {
    let n = dist(p, t);
    let (gx, gy) = if n == 0 { t } else { (t.0 + (p.0 - t.0) * radii / n, t.1 + (p.1 - t.1) * radii / n) };
    (gx.div_euclid(500) * 500 + 250, gy.div_euclid(500) * 500 + 250)
}

/// One run from the Bandit's put-back point: (its distance from the goal at the tick's start, its move) on each tick of
/// the run, the last its end tick, and the distance from the goal it ended at.
#[derive(Debug)]
struct Run {
    ticks: Vec<(i64, i64)>,
    end_off: i64,
}

/// The Bandit's dash at its Knight under combat.DASH_TARGET_DEATH = `arm`, the Knight put to 0 hitpoints after the
/// Bandit's first dash move; three ticks on the Bandit is put on its dash line `d` from its goal. Its run from there: every
/// tick that moves a half-step or a whole one, to the first that moves neither (its walk).
#[allow(clippy::type_complexity)]
fn gone_run(arm: DashTargetDeath, d: i64) -> Run {
    let mut cfg = config();
    cfg.calib.dash_attack = DashAttack::ClientDash;
    cfg.calib.dash_target_death = arm;
    let mut s = BattleState::new(0, cfg);
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Knight", at(KNIGHT_AT), None), (Team::Blue, "Assassin", at(BANDIT_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (k, a) = (ids[0], ids[1]);
    let radii = (s.entity(a).expect("the Bandit").radius + s.entity(k).expect("the Knight").radius) as i64 / K as i64;
    // The entry tick is still; its start-of-tick points give the goal, and the next tick is the first dash move.
    let mut entry: Option<((i64, i64), (i64, i64), i64)> = None;
    let mut goal = None;
    for _ in 0..200 {
        let (a0, k0) = (native(s.entity(a).expect("the Bandit").pos), native(s.entity(k).expect("the Knight").pos));
        s.tick();
        let a1 = native(s.entity(a).expect("the Bandit").pos);
        let step = dist(a0, a1);
        if step >= 400 {
            let (pa, pk, pstep) = entry.expect("the scene drifted: the Bandit dashed on its first tick");
            assert_eq!(pstep, 0, "the scene drifted: the Bandit moved on its dash's entry tick");
            let g = dash_goal(pa, pk, radii);
            let gained = dist(a0, g) - dist(a1, g);
            assert!((490..=505).contains(&gained), "the written-out goal {g:?} is not the dash's: its first move took {gained} off it");
            goal = Some(g);
            break;
        }
        entry = Some((a0, k0, step));
    }
    let goal = goal.expect("the Bandit never dashed");
    assert!(s.debug_set_hp(k, 0), "the Knight stands at the dash");
    for _ in 0..3 {
        s.tick();
    }
    assert!(s.entity(k).is_none(), "the scene drifted: the Knight outlived its 0 hitpoints");
    let p = native(s.entity(a).expect("the Bandit").pos);
    let n = dist(p, goal);
    assert!(n > d + 500, "the scene drifted: the Bandit was only {n} from its goal when its Knight was gone");
    let put = (goal.0 + (p.0 - goal.0) * d / n, goal.1 + (p.1 - goal.1) * d / n);
    assert!(s.debug_set_pos(a, at((put.0 as i32, put.1 as i32))), "the Bandit lives");
    let (mut ticks, mut end_off) = (Vec::new(), None);
    for _ in 0..12 {
        let a0 = native(s.entity(a).expect("the Bandit").pos);
        s.tick();
        let step = dist(a0, native(s.entity(a).expect("the Bandit").pos));
        let dash_move = (240..=256).contains(&step) || (485..=505).contains(&step);
        if !dash_move {
            end_off = Some(dist(a0, goal));
            break;
        }
        ticks.push((dist(a0, goal), step));
    }
    Run { ticks, end_off: end_off.expect("the scene drifted: the run never ended") }
}

/// combat.DASH_TARGET_DEATH = client16635_half_step_end (client 16.635, parity r99, the oracle's il-mg-16635 set: 2dc95574
/// t596, 4d00ed2b t3430, 011d9189 t3565, their last ticks 538, 732 and 652 off, moved 250 and ended): every run ends 250
/// to 499 from its goal, a last tick that began 500 to 749 off on one half-step; under client16635_runs_to_goal (the
/// vacuity check) those runs take a whole step on it and end within 250. Plant: dash_half_step_end_whole_tick.
#[test]
fn under_client16635_half_step_end_a_bandit_whose_target_dies_ends_on_the_half_step() {
    let mut half = 0;
    for k in 0..10 {
        let d = 1025 + 50 * k;
        let new = gone_run(DashTargetDeath::Client16635HalfStepEnd, d);
        assert!(new.ticks.len() >= 2, "client16635_half_step_end from {d}: a one-tick run: {new:?}");
        let (last_off, last_move) = *new.ticks.last().expect("a run");
        let before = &new.ticks[..new.ticks.len() - 1];
        assert!(before.iter().all(|t| t.1 >= 490), "client16635_half_step_end from {d}: a part-step before the last: {new:?}");
        assert!((250..500).contains(&new.end_off), "client16635_half_step_end from {d}: the run ended {} from its goal: {new:?}", new.end_off);
        if (500..750).contains(&last_off) {
            half += 1;
            assert!((245..=255).contains(&last_move), "client16635_half_step_end from {d}: its last tick began {last_off} off and moved {last_move}: {new:?}");
        }
    }
    assert!(half >= 3, "the scene drifted: only {half} runs' last tick began 500 to 749 from the goal");
    // NOT VACUOUS: the shipped arm tests the end after both half-steps, so those runs take a whole step and end within 250.
    let mut whole = 0;
    for k in 0..10 {
        let d = 1025 + 50 * k;
        let old = gone_run(DashTargetDeath::Client16635RunsToGoal, d);
        assert!(old.ticks.iter().all(|t| t.1 >= 490), "client16635_runs_to_goal from {d}: a part-step: {old:?}");
        let (last_off, _) = *old.ticks.last().expect("a run");
        if (500..750).contains(&last_off) && old.end_off < 250 {
            whole += 1;
        }
    }
    assert!(whole >= 1, "client16635_runs_to_goal (the vacuity check): no run ended within 250 of its goal");
}
