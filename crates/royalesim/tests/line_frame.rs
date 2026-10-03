//! formation.LINE_FRAME: the frame a side 1 line's place is looked for in (state.rs `formation_members_with`, formation.rs
//! `line_centre`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, sp-rrtap-s1-15500-28500: Red's Royal Recruits tapped on
//! (15500, 28500) stood on y 27249 / 27749 (own y 4500), as side 0's tapped on (15500, 3500) did; the rotation refuses own
//! (6500, 4500) (its ground point on the king's zone's edge) and the engine laid them 4000 back, y 23249 / 23749.
//!
//! THE SCENE: that tap, the six members' ys read on the play's first frame.
//!
//! WHAT IS PINNED, and the plant that turns it red (line_frame_rotation):
//!   1. client15535_y_reflection: every member within 300 of y 27500; rotation (the engine's, the vacuity check): within
//!      300 of y 23500.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, LineFrame};
use royalesim::Team;

/// The Royal Recruits' ys (native) after Red's play on (15500, 28500).
fn ys(arm: LineFrame) -> Vec<i32> {
    let mut cfg: BattleConfig = config();
    cfg.calib.line_frame = arm;
    let deck: Vec<String> = ["RoyalRecruits", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|s| s.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(15, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s.deploy(Team::Red, "RoyalRecruits", Vec2::new(15500 * K, 28500 * K)).expect("the play");
    let mut out = Vec::new();
    for _ in 0..30 {
        s.tick();
        out = s.entities().filter(|e| e.team == Team::Red && e.card.starts_with("RoyalRecruit")).map(|e| e.pos.y / K).collect();
        if out.len() >= 6 {
            break;
        }
    }
    assert_eq!(out.len(), 6, "the scene drifted: six recruits ({out:?})");
    out
}

/// Plant: line_frame_rotation.
#[test]
fn a_side_1_line_is_placed_in_the_y_reflection_under_client15535_y_reflection() {
    let rot = ys(LineFrame::Rotation);
    // NOT VACUOUS: the rotation lays the line 4000 back.
    assert!(rot.iter().all(|y| (y - 23500).abs() <= 300), "rotation: {rot:?}");
    let refl = ys(LineFrame::Client15535YReflection);
    assert!(refl.iter().all(|y| (y - 27500).abs() <= 300), "client15535_y_reflection: {refl:?}");
}
