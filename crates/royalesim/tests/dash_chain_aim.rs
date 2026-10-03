//! combat.DASH_CHAIN_AIM: the point a dash chain's dash (the Golden Knight's) steps toward (state.rs
//! `phase_path16402_for`, the chain dash; `ChainRun::aim`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, the Golden Knight's dashes head for the centre of the dash's
//! cell (his path on the aiming tick is that one node): on a chain's first dash the 500-cell holding the point own radius
//! plus the target's radius short of the target's centre on the line to him, not the target's centre: the engine's dash
//! arithmetic toward that point gives the client's steps exactly, 43 of 45 over 11 dashes.
//!
//! THE SCENE: a Blue Golden Knight held on (3294, 9000) until he stands, a red Giant held on (3420, 14000), 5,002
//! ahead and right of him, in his charge circle; pressed, he aims at the Giant and dashes from the next tick. The point
//! 800 + 750 short of the Giant on the line to him, (3381, 12451), lies in the cell centred on (3250, 12250), left of
//! him; the Giant's centre lies right of him.
//!
//! WHAT IS PINNED, and the plant that turns it red (dash_aims_target_centre):
//!   1. client15535_goal_cell: his first dash step goes left (x falls), toward the dash cell's centre; under
//!      target_centre (the engine's, the vacuity check) it goes right, toward the Giant's centre.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DashChainAim};
use royalesim::Team;

const DECK: [&str; 8] = ["GoldenKnight", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// His first dash step (native): the first tick after the press he moves more than 300.
fn first_dash_step(arm: DashChainAim) -> (i32, i32) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|s| s.to_string()).collect(), DECK.iter().map(|s| s.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.dash_chain_aim = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let gk = s.scenario_spawn_now(Team::Blue, "GoldenKnight", n(3294, 9000), None).expect("the Golden Knight");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", n(3420, 14000), None).expect("the Giant");
    for _ in 0..40 {
        assert!(s.debug_set_pos(gk, n(3294, 9000)) && s.debug_set_pos(giant, n(3420, 14000)));
        s.tick();
    }
    assert!(!s.entity(gk).expect("he lives").deploying, "the scene drifted: he never stood up");
    assert!(s.debug_set_pos(gk, n(3294, 9000)) && s.debug_set_pos(giant, n(3420, 14000)));
    s.press_ability_button(Team::Blue, 0).expect("the press is taken");
    for _ in 0..20 {
        assert!(s.debug_set_pos(giant, n(3420, 14000)));
        let before = s.entity(gk).expect("he lives").pos;
        s.tick();
        let after = s.entity(gk).expect("he lives").pos;
        let d = ((after.x - before.x) / K, (after.y - before.y) / K);
        if d.0 * d.0 + d.1 * d.1 > 300 * 300 {
            return d;
        }
    }
    panic!("the scene drifted: he never dashed");
}

/// Plant: dash_aims_target_centre.
#[test]
fn a_chain_dash_heads_for_its_goal_cell_under_client15535_goal_cell() {
    let d = first_dash_step(DashChainAim::Client15535GoalCell);
    assert!(d.1 > 300 && d.0 < 0, "client15535_goal_cell: his first dash step {d:?} does not head left for the goal cell's centre");
    // NOT VACUOUS: the engine's arm heads right, for the Giant's centre.
    let d = first_dash_step(DashChainAim::TargetCentre);
    assert!(d.1 > 300 && d.0 > 0, "target_centre: his first dash step {d:?} does not head right for the Giant's centre");
}
