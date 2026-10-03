//! combat.LOST_TARGET_SWING: what a cancelled lock does to a swing whose unit takes another target in reach on the same
//! tick (state.rs `phase_target`, the swing's reset).
//!
//! THE LAW, measured on client 15.535.29 (parity's attack_switch_census.py): a unit in its attack whose target changed from
//! one still alive ran its progress on, 198 of 199, among them 15 of 15 switches off an Evo Minion Horde minion its hit had
//! just turned ghost (sp-form-MinionHorde-evo-s0 t688: a Musketeer 1,250 -> 1,300 onto the next minion).
//!
//! The scene: Blue's evolved Minion Horde (six minions) held on (9000, 13000), a Red Musketeer held 5,000 north of them
//! (in reach of every minion); its first shot turns the minion it hits ghost (invisible), and on the next Target phase it
//! takes another minion in reach. WHAT IS PINNED, each with its precondition (the switch is to another minion, the
//! Musketeer in its attack on both ticks):
//!   1. client15535_runs_on_in_reach: on the switch tick its progress is the tick before's + 50;
//!   2. cancelled (the old arm, the vacuity check): it is not (the swing restarted);
//!   3. the shipped value is cancelled (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test lost_target_swing`):
//!   * `lost_target_swing_cancelled` -- the new arm's cancel still resets the swing: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, LostTargetSwing};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const AT: (i32, i32) = (9000, 13000);
const MUSKETEER_AT: (i32, i32) = (9000, 18000);
/// The ticks allowed for the switch.
const WATCH: usize = 120;

/// Under `arm`: the Musketeer's progress on the tick before its switch to a second minion and on the switch tick.
fn switch(arm: LostTargetSwing) -> (i32, i32) {
    let mut cfg: BattleConfig = config();
    cfg.calib.lost_target_swing = arm;
    cfg.decks = [vec!["MinionHorde".into(), "Knight".into()], vec!["Musketeer".into(), "Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "MinionHorde_EV1", n(AT.0, AT.1), None).expect("the horde");
    s.tick();
    let minions: Vec<(EntityId, Vec2)> = find_live(&s, Team::Blue, "MinionHorde_EV1").iter().map(|e| (e.id, e.pos)).collect();
    assert_eq!(minions.len(), 6, "six minions");
    let mp = n(MUSKETEER_AT.0, MUSKETEER_AT.1);
    let m = s.scenario_spawn_now(Team::Red, "Musketeer", mp, None).expect("a red Musketeer");
    let mtop = s.entity(m).expect("the Musketeer").max_hp;
    let mut last: Option<(Option<EntityId>, i32, AttackPhase)> = None;
    for _ in 0..WATCH {
        for (id, p) in &minions {
            if s.entity(*id).is_some() {
                assert!(s.debug_set_pos(*id, *p));
            }
        }
        assert!(s.debug_set_pos(m, mp) && s.debug_set_hp(m, mtop));
        s.tick();
        let e = s.entity(m).expect("the Musketeer");
        if let Some((Some(was), before, phase)) = last {
            if e.target.is_some() && e.target != Some(was) && minions.iter().any(|(id, _)| Some(*id) == e.target) {
                assert!(phase != AttackPhase::Idle && e.attack_phase != AttackPhase::Idle, "{arm:?}: the scene drifted: the Musketeer was not in its attack across the switch");
                assert!(s.entity(was).is_some(), "{arm:?}: the scene drifted: the minion it let go is dead, not hidden");
                return (before, e.attack_ms);
            }
        }
        last = Some((e.target, e.attack_ms, e.attack_phase));
    }
    panic!("{arm:?}: the scene drifted: the Musketeer never switched minions");
}

#[test]
fn a_swing_runs_on_onto_a_target_in_reach_after_its_target_turns_ghost() {
    let (before, after) = switch(LostTargetSwing::Client15535RunsOnInReach);
    assert_eq!(after, before + 50, "new: the swing did not run on across the switch ({before} -> {after})");
}

#[test]
fn the_old_value_restarts_the_swing() {
    let (before, after) = switch(LostTargetSwing::Cancelled);
    assert_ne!(after, before + 50, "old: the swing ran on across the switch ({before} -> {after})");
}

#[test]
fn the_shipped_value_is_cancelled() {
    assert_eq!(Calib::shipped().lost_target_swing, LostTargetSwing::Cancelled);
}
