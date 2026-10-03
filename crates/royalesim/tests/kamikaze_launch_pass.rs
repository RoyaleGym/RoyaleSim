//! combat.KAMIKAZE_LAUNCH_PASS: when a kamikaze's own death reaches the later units of the sequential pass (state.rs
//! `phase_attack_for`, the kamikaze's self-hit; `phase_target_attack_sequential`, `Scratch::launched`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, a kamikaze created before the deciding unit is gone for it on
//! its launch frame: 10 of 10 units taking a target passed over it, and 7 of 7 walkers holding it took another target on
//! that frame (sp-il-323a t1126).
//!
//! THE SCENE (match.TICK_ORDER = client_sequential_strike): a red Ice Spirit set down first at (9000, 10500), then a Blue
//! Knight at (9000, 8000) and a red Skeleton at (11000, 11000); the Knight takes the Ice Spirit (2,500 off, the Skeleton
//! 3,606) and walks at it; the Ice Spirit launches itself at the Knight. On the launch tick (the Ice Spirit gone after
//! it) the Knight's target is read.
//!
//! WHAT IS PINNED, and the plant that turns it red (kamikaze_launch_buffered):
//!   1. client15535_gone_at_launch: on the launch tick the Knight holds the Skeleton; under buffered (the engine's, the
//!      vacuity check) it still held the Ice Spirit in its turn, and holds no Skeleton.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, KamikazeLaunchPass, TickOrder};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Knight's target after the Ice Spirit's launch tick, and the Skeleton's id.
fn knight_target_on_launch(arm: KamikazeLaunchPass) -> (Option<EntityId>, EntityId) {
    let mut cfg = config();
    cfg.calib.kamikaze_launch_pass = arm;
    cfg.calib.tick_order = TickOrder::ClientSequentialStrike;
    let mut s = BattleState::new(3, cfg);
    let spirit = s.scenario_spawn_now(Team::Red, "IceSpirits", n(9000, 10500), None).expect("the Ice Spirit");
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", n(9000, 8000), None).expect("the Knight");
    let skeleton = s.scenario_spawn_now(Team::Red, "Skeletons", n(11000, 11000), None).expect("the Skeleton");
    let mut held = false;
    for _ in 0..200 {
        assert!(s.debug_set_pos(skeleton, n(11000, 11000)), "the scene drifted: the Skeleton is gone");
        s.tick();
        if s.entity(spirit).is_none() {
            assert!(held, "the scene drifted: the Knight never held the Ice Spirit");
            return (s.entity(knight).expect("the Knight").target, skeleton);
        }
        held |= s.entity(knight).expect("the Knight").target == Some(spirit);
    }
    panic!("the scene drifted: the Ice Spirit never launched");
}

/// Plant: kamikaze_launch_buffered.
#[test]
fn a_walker_holding_a_kamikaze_takes_its_next_target_on_the_launch_tick_under_client15535_gone_at_launch() {
    let (t, skeleton) = knight_target_on_launch(KamikazeLaunchPass::Client15535GoneAtLaunch);
    assert_eq!(t, Some(skeleton), "client15535_gone_at_launch: the Knight's target on the launch tick");
    // NOT VACUOUS: under the engine's arm it still held the Ice Spirit in its turn.
    let (t, skeleton) = knight_target_on_launch(KamikazeLaunchPass::Buffered);
    assert_ne!(t, Some(skeleton), "buffered: the Knight took the Skeleton on the launch tick");
}
