//! targeting.DOOMED_DROP_SWING, read off the engine: what dropping a doomed target does to the attacker's swing.
//!
//! THE LAW, measured on the 16.402 corpus: a projectile attacker attacking a target that the shots in flight doom, and
//! that has not fired at it, drops it on the next tick (targeting.DOOMED_TARGET_DROP). The drop is a switch away from a
//! live target, so combat.RETARGET_PROGRESS decides what it does to the swing: when the new target already stands in
//! the attacker's reach the attack progress runs on and the attacker fires on the old cycle (57 of 57 corpus switches,
//! 31 of 31 on client 15.535.29), and when it does not the attacker walks with progress 0 (44 of 44 troops). Today's
//! engine (cancel) cancels the swing in both cases.
//!
//! THE SCENE (tests/test_doomed_drop_swing.py plays the same one through the Python API): a blue Minion at (9000, 8800),
//! a red Knight X at (9000, 13000), a second red Knight Y at (9800, 13100) with 3000 hp, and a blue Musketeer at
//! (9000, 6000). The Minion takes X (the nearer), starts its swing, and the Musketeer's shot at X is in flight while the
//! Minion has not fired. With X at 60 hp that shot dooms X, and the Minion drops it for Y, which then stands in its reach.
//! The control gives X 2000 hp: nothing dooms it and the Minion fires at it; that launch tick is the old cycle.
//! Measured on this engine at 126992a (cancel): the switch on tick 15, the control's launch on 17, the doomed run's
//! launch at Y on 31. The out-of-reach scene moves the Minion to (9000, 9000) and Y to (10600, 13000): Y stands 3780
//! from it at the switch (reach 3500), and the Minion walks.
//!
//! Pinned here, each with its preconditions checked:
//!   1. client_keep_in_reach: the Minion switches to Y in reach with its swing running and fires at Y on the control's
//!      launch tick;
//!   2. cancel: the same switch restarts the swing, so the launch at Y comes later than the control's;
//!   3. both arms: a switch to an enemy out of reach cancels the swing (the Minion walks, progress 0);
//!   4. the shipped value is the new arm.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test doomed_drop_swing`):
//!   * `doomed_drop_cancels_swing` -- the new arm still cancels the swing of a doomed drop: (1) goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DoomedDropSwing, DoomedTargetDrop, EqualDistanceTie, RetargetProgress};
use royalesim::target::in_attack_range;
use royalesim::{EntityId, Team};

/// One tick's state of the spawned units, by spawn index.
struct Tick {
    alive: Vec<bool>,
    target: Vec<Option<EntityId>>,
    phase: Vec<Option<AttackPhase>>,
    progress: Vec<i32>,
    pos: Vec<Vec2>,
    radius: Vec<i32>,
    shots_at: Vec<usize>,
}

type Spawn = (Team, &'static str, (i32, i32), Option<i32>);

const M: usize = 0;
const X: usize = 1;
const Y: usize = 2;

fn scene(minion: (i32, i32), y: (i32, i32), x_hp: i32) -> [Spawn; 4] {
    [
        (Team::Blue, "Minions", minion, None),
        (Team::Red, "Knight", (9000, 13000), Some(x_hp)),
        (Team::Red, "Knight", y, Some(3000)),
        (Team::Blue, "Musketeer", (9000, 6000), None),
    ]
}
const IN_REACH: ((i32, i32), (i32, i32)) = ((9000, 8800), (9800, 13100));
const OUT_OF_REACH: ((i32, i32), (i32, i32)) = ((9000, 9000), (10600, 13000));
const DOOMED_HP: i32 = 60;
const CONTROL_HP: i32 = 2000;

/// Spawn from tick 200 under `arm` and run `ticks` ticks; returns the ids, the state after each tick (index 0 is before
/// the first) and the Minion's range.
fn play(arm: DoomedDropSwing, spawns: &[Spawn], ticks: u32) -> (Vec<EntityId>, Vec<Tick>, i32) {
    let mut cfg = config();
    cfg.calib.doomed_drop_swing = arm;
    // The Minion's two targets stand at one distance from it: the scenes were written on the tie by the lower
    // own-frame x (targeting.EQUAL_DISTANCE_TIE = own_frame_low_x), which picks X; the shipped higher-x tie picks Y.
    cfg.calib.equal_distance_tie = EqualDistanceTie::OwnFrameLowX;
    let range = cfg.cards.cards.iter().find(|c| c.name == "Minions").expect("Minions is simulable").range;
    let mut s = BattleState::new(0, cfg);
    s.scenario_set_tick(200);
    let specs: Vec<(Team, &str, Vec2, Option<i32>)> = spawns.iter().map(|&(t, c, p, h)| (t, c, Vec2::new(p.0 * K, p.1 * K), h)).collect();
    let ids = s.scenario_spawn_batch(&specs).unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let snap = |s: &BattleState| Tick {
        alive: ids.iter().map(|id| s.entity(*id).is_some()).collect(),
        target: ids.iter().map(|id| s.entity(*id).and_then(|e| e.target)).collect(),
        phase: ids.iter().map(|id| s.entity(*id).map(|e| e.attack_phase)).collect(),
        progress: ids.iter().map(|id| s.entity(*id).map_or(0, |e| e.attack_ms)).collect(),
        pos: ids.iter().map(|id| s.entity(*id).map_or(Vec2::default(), |e| e.pos)).collect(),
        radius: ids.iter().map(|id| s.entity(*id).map_or(0, |e| e.radius)).collect(),
        shots_at: ids.iter().map(|id| s.projectiles().iter().filter(|p| p.target == *id).count()).collect(),
    };
    let mut out = vec![snap(&s)];
    for _ in 0..ticks {
        s.tick();
        out.push(snap(&s));
    }
    (ids, out, range)
}

/// The Minion's launch ticks (its attack phase reads Cooldown, the tick its shot leaves) and the target on each.
fn launches(run: &[Tick]) -> Vec<(usize, Option<EntityId>)> {
    (1..run.len()).filter(|&t| run[t].phase[M] == Some(AttackPhase::Cooldown)).map(|t| (t, run[t].target[M])).collect()
}

/// The switch tick S: the Minion names X on S - 1 and Y on S while X still stands; its preconditions checked.
fn switch(run: &[Tick], ids: &[EntityId]) -> usize {
    let s = (1..run.len())
        .find(|&t| run[t - 1].target[M] == Some(ids[X]) && run[t].target[M] == Some(ids[Y]))
        .expect("the Minion never switched from X to Y: the scene drifted");
    assert!(run[s].alive[X], "precondition: X died on {s}, so this is a kill, not a doomed drop");
    assert_eq!(run[s - 1].phase[M], Some(AttackPhase::Windup), "precondition: the Minion was not in its swing on {}", s - 1);
    assert!(launches(&run[..s]).is_empty(), "precondition: the Minion fired before the switch on {s}");
    assert!(run[s - 1].shots_at[X] > 0, "precondition: no shot flew at X on {}, so nothing doomed it", s - 1);
    s
}

fn reach(run: &[Tick], t: usize, range: i32) -> bool {
    let c = Calib::shipped();
    in_attack_range(&c, run[t].pos[M], range, run[t].radius[M], run[t].pos[Y], run[t].radius[Y])
}

#[test]
fn a_doomed_drop_to_an_enemy_in_reach_keeps_the_swing() {
    let (minion, y) = IN_REACH;
    let (_, control, _) = play(DoomedDropSwing::ClientKeepInReach, &scene(minion, y, CONTROL_HP), 40);
    let old = launches(&control).first().copied().expect("the control Minion never fired").0;
    let (ids, run, range) = play(DoomedDropSwing::ClientKeepInReach, &scene(minion, y, DOOMED_HP), 40);
    let s = switch(&run, &ids);
    assert!(reach(&run, s - 1, range), "precondition: Y was not in the Minion's reach on {} (start of {s})", s - 1);
    assert!(s < old, "precondition: the switch on {s} is not before the control's launch on {old}");
    assert!(
        run[s].progress[M] > run[s - 1].progress[M] && run[s].phase[M] == Some(AttackPhase::Windup),
        "the swing restarted on the switch {s}: progress {} -> {}, phase {:?}",
        run[s - 1].progress[M],
        run[s].progress[M],
        run[s].phase[M]
    );
    let first = launches(&run).first().copied().expect("the Minion never fired");
    assert_eq!(first, (old, Some(ids[Y])), "the Minion fired at {:?} on {} and the control on {old}", first.1, first.0);
}

#[test]
fn the_old_arm_restarts_the_swing() {
    let (minion, y) = IN_REACH;
    let (_, control, _) = play(DoomedDropSwing::Cancel, &scene(minion, y, CONTROL_HP), 45);
    let old = launches(&control).first().copied().expect("the control Minion never fired").0;
    let (ids, run, range) = play(DoomedDropSwing::Cancel, &scene(minion, y, DOOMED_HP), 45);
    let s = switch(&run, &ids);
    assert!(reach(&run, s - 1, range), "precondition: Y was not in the Minion's reach on {}", s - 1);
    let first = launches(&run).first().copied().expect("the Minion never fired");
    assert_eq!(first.1, Some(ids[Y]), "the Minion's first shot went elsewhere");
    assert!(first.0 > old, "cancel: the Minion fired on {} and the control on {old}; the swing was kept", first.0);
}

#[test]
fn a_doomed_drop_to_an_enemy_out_of_reach_cancels_the_swing_under_both_arms() {
    let (minion, y) = OUT_OF_REACH;
    for arm in [DoomedDropSwing::Cancel, DoomedDropSwing::ClientKeepInReach] {
        let (ids, run, range) = play(arm, &scene(minion, y, DOOMED_HP), 40);
        let s = switch(&run, &ids);
        assert!(!reach(&run, s - 1, range), "precondition ({arm:?}): Y was in the Minion's reach on {}", s - 1);
        assert_eq!(run[s].phase[M], Some(AttackPhase::Idle), "{arm:?}: the Minion kept a swing on {s} for a target out of reach");
        assert_eq!(run[s].progress[M], 0, "{arm:?}: progress {} on {s}", run[s].progress[M]);
    }
}

#[test]
fn the_shipped_value_is_the_new_arm_and_the_laws_it_leans_on_ship() {
    let c = Calib::shipped();
    assert_eq!(c.doomed_drop_swing, DoomedDropSwing::ClientKeepInReach);
    // The new arm is read through these two; the scenes above assume both.
    assert_eq!(c.doomed_target_drop, DoomedTargetDrop::ProjectileAttackersWalkDrop);
    assert_eq!(c.retarget_progress, RetargetProgress::KeepWhenDeadOrInReach);
}
