//! combat.KAMIKAZE_LAUNCH_PASS = client15535_launcher_reads_start: a kamikaze whose own launch is due in its turn reads one
//! that launched earlier in the sequential pass at its start hitpoints, so it launches at it too (state.rs
//! `phase_target_attack_sequential`, `launcher`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, two enemy spirits holding each other with both launches due on
//! one tick launched both, 3 of 3 scored (sp-il-2142 t1850, sp-ghost-ab-s0 and sp-ghost-summons-s0 t792).
//!
//! THE SCENE (match.TICK_ORDER = client_sequential_strike): a red Ice Spirit set down first at (9000, 12500), then a blue
//! one at (9000, 9400), 3,100 apart: each takes the other and both start their attack on one tick. On the tick the red one
//! (created first, the earlier turn) is gone, is the blue one gone too?
//!
//! WHAT IS PINNED, and the plant that turns it red (launcher_finds_launched_gone):
//!   1. client15535_launcher_reads_start: the blue spirit is gone on the red one's launch tick;
//!   2. client15535_gone_at_launch (the vacuity check): it still stands that tick;
//!   3. buffered (the engine's): it is gone too;
//!   4. a holder that is not a launching kamikaze still lets the launched one go under the new arm (the Knight of
//!      tests/kamikaze_launch_pass.rs takes the Skeleton on the launch tick).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, KamikazeLaunchPass, TickOrder};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`: whether the blue spirit stands on the tick the red one is first gone.
fn blue_stands_on_red_launch(arm: KamikazeLaunchPass) -> bool {
    let mut cfg = config();
    cfg.calib.kamikaze_launch_pass = arm;
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    let mut s = BattleState::new(3, cfg);
    let red = s.scenario_spawn_now(Team::Red, "IceSpirits", n(9000, 12500), None).expect("the red Ice Spirit");
    let blue = s.scenario_spawn_now(Team::Blue, "IceSpirits", n(9000, 9400), None).expect("the blue Ice Spirit");
    let mut held = false;
    for _ in 0..200 {
        s.tick();
        if s.entity(red).is_none() {
            assert!(held, "{arm:?}: the scene drifted: the spirits never held each other");
            return s.entity(blue).is_some();
        }
        held |= s.entity(red).expect("red").target == Some(blue) && s.entity(blue).is_some_and(|b| b.target == Some(red));
    }
    panic!("{arm:?}: the scene drifted: the red Ice Spirit never launched");
}

/// The Knight's target after a lone Ice Spirit's launch tick (tests/kamikaze_launch_pass.rs's scene), and the Skeleton.
fn knight_target_on_launch(arm: KamikazeLaunchPass) -> (Option<EntityId>, EntityId) {
    let mut cfg = config();
    cfg.calib.kamikaze_launch_pass = arm;
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    let mut s = BattleState::new(3, cfg);
    let spirit = s.scenario_spawn_now(Team::Red, "IceSpirits", n(9000, 10500), None).expect("the Ice Spirit");
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", n(9000, 8000), None).expect("the Knight");
    let skeleton = s.scenario_spawn_now(Team::Red, "Skeletons", n(11000, 11000), None).expect("the Skeleton");
    for _ in 0..200 {
        assert!(s.debug_set_pos(skeleton, n(11000, 11000)), "the scene drifted: the Skeleton is gone");
        s.tick();
        if s.entity(spirit).is_none() {
            return (s.entity(knight).expect("the Knight").target, skeleton);
        }
    }
    panic!("the scene drifted: the Ice Spirit never launched");
}

/// Plant: launcher_finds_launched_gone.
#[test]
fn a_launching_spirit_launches_at_one_that_launched_before_it_under_client15535_launcher_reads_start() {
    assert!(!blue_stands_on_red_launch(KamikazeLaunchPass::Client15535LauncherReadsStart), "new: the blue spirit did not launch");
    // NOT VACUOUS: under the arm before it the later spirit finds its target gone and stands.
    assert!(blue_stands_on_red_launch(KamikazeLaunchPass::Client15535GoneAtLaunch), "gone_at_launch: the blue spirit launched");
}

#[test]
fn the_engines_arm_launches_both() {
    assert!(!blue_stands_on_red_launch(KamikazeLaunchPass::Buffered), "buffered: the blue spirit did not launch");
}

#[test]
fn a_holder_that_is_no_launching_kamikaze_still_lets_it_go() {
    let (t, skeleton) = knight_target_on_launch(KamikazeLaunchPass::Client15535LauncherReadsStart);
    assert_eq!(t, Some(skeleton), "new: the Knight's target on the launch tick");
}
