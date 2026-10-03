//! targeting.GHOST_PAIR_FIRST_FRAME: whether an Evo Ghost's pair (card.rs `GhostDef`) may be an enemy's target on its
//! first frame (state.rs `phase_spawn`, `ghost_pair_unit`).
//!
//! THE READING (client 15.535.29): sp-ghost-ab-s0 t1057, a pair put down 1,999 from Red's left princess tower, which took
//! the Ghost 2,770 away on that tick and held it, where the engine's towers took the pair members and killed them (the
//! scene's first divergence); earlier pairs were taken from their 2nd frame (sp-ghost-summons-s0 t830).
//!
//! The scene: a red Musketeer with nothing else in sight; a blue pair member put down 4,000 ahead of it, across the
//! river on Blue's bank.
//!
//! PLANT (regression): ghost_pair_first_frame_targetable -> `a_ghost_pair_is_no_target_on_its_first_frame` red.
//!   RUSTFLAGS='--cfg clash_plant="ghost_pair_first_frame_targetable"' CARGO_TARGET_DIR=target/plant cargo test --profile
//!   gate --test ghost_pair_first_frame
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, GhostPairFirstFrame};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Whether the Musketeer held the pair member as its target on the member's first frame, and on its second.
fn scene(arm: GhostPairFirstFrame) -> (bool, bool) {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Ghost".into(), "Knight".into()], vec!["Musketeer".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.ghost_pair_first_frame = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", n(9000, 18500), None).expect("the Musketeer");
    for _ in 0..30 {
        assert!(s.debug_set_pos(musk, n(9000, 18500)));
        s.tick();
    }
    assert!(s.entity(musk).expect("the Musketeer").target.is_none(), "the scene drifted: the Musketeer has a target before the pair");
    s.spawn_unit(Team::Blue, "Ghost_EV1_Summon_Left", n(9000, 14500), None).expect("the pair member");
    assert!(s.debug_set_pos(musk, n(9000, 18500)));
    s.tick();
    let member = find_live(&s, Team::Blue, "Ghost_EV1_Summon_Left").first().expect("the member is on the board").id;
    let first = s.entity(musk).expect("the Musketeer").target == Some(member);
    assert!(s.debug_set_pos(musk, n(9000, 18500)));
    s.tick();
    let second = s.entity(musk).expect("the Musketeer").target == Some(member);
    (first, second)
}

/// Plant: ghost_pair_first_frame_targetable.
#[test]
fn a_ghost_pair_is_no_target_on_its_first_frame() {
    assert_eq!(scene(GhostPairFirstFrame::Client15535Untargetable), (false, true), "client15535_untargetable: (frame 1, frame 2)");
    assert!(scene(GhostPairFirstFrame::Targetable).0, "targetable: the Musketeer did not take it on its first frame (vacuous)");
}

#[test]
fn the_shipped_arm_lets_it_be_taken() {
    assert_eq!(Calib::shipped().ghost_pair_first_frame, GhostPairFirstFrame::Targetable);
}
