//! movement.DEPLOY_WATER_EDGE (state.rs `phase_path16402_for`, the walk branch's water clamp on a deploying unit): a
//! deploying flyer pushed by contact crosses the water's edge; a deploying ground unit stops on the bank.
//!
//! Measured on client 16.402 (parity r69, the ob6 live set): units deploying on both frames of a move into a water cell,
//! flying 38, ground 6 of 101,000 moves.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! deploy_water_edge`):
//!   - deploy_flyer_clamped -> `under_client16402_ground_only_a_deploying_flyer_pushed_at_the_river_crosses_it` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::state::{BattleConfig, BattleState, DeployWaterEdge};
use royalesim::Team;

/// A Blue `card` played on the near bank's last dry row at the arena's middle (the river all along x 9000) with a Blue
/// `pusher` held overlapping it from the south, under movement.DEPLOY_WATER_EDGE = `arm`: the played unit's greatest y over
/// its deploy, and the river's first row (subtiles).
fn pushed_at_the_river(card: &str, pusher: &str, arm: DeployWaterEdge) -> (i32, i32) {
    let mut cfg: BattleConfig = config();
    cfg.calib.deploy_water_edge = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let a = s.arena().clone();
    let at = royalesim::fixed::Vec2::new(a.width / 2, a.water_y_min - a.cell / 2);
    let push_at = royalesim::fixed::Vec2::new(at.x, at.y - a.cell);
    let pushed = s.scenario_spawn_now(Team::Blue, pusher, push_at, None).expect("the pusher");
    s.spawn_unit(Team::Blue, card, at, None).expect("the play");
    let mut top = i32::MIN;
    for _ in 0..18 {
        assert!(s.debug_set_pos(pushed, push_at));
        s.tick();
        if let Some(e) = find_live(&s, Team::Blue, card).first() {
            assert!(e.deploying, "the scene drifted: the {card} deployed");
            top = top.max(e.pos.y);
        }
    }
    (top, a.water_y_min)
}

/// client16402_ground_only: a deploying Baby Dragon pushed north by a Mega Minion overlapping it from the south crosses
/// the river's first row; under every_deploying_unit (the vacuity check) it stops on the bank, and so does a deploying
/// Knight pushed by a Knight under the new arm. Plant: deploy_flyer_clamped.
#[test]
fn under_client16402_ground_only_a_deploying_flyer_pushed_at_the_river_crosses_it() {
    let (top, river) = pushed_at_the_river("BabyDragon", "MegaMinion", DeployWaterEdge::Client16402GroundOnly);
    assert!(top >= river, "client16402_ground_only: the deploying Baby Dragon stopped short of the river ({top} < {river})");
    let (top, river) = pushed_at_the_river("BabyDragon", "MegaMinion", DeployWaterEdge::EveryDeployingUnit);
    assert!(top < river, "every_deploying_unit (the vacuity check): the deploying Baby Dragon crossed ({top} >= {river})");
    let (top, river) = pushed_at_the_river("Knight", "Knight", DeployWaterEdge::Client16402GroundOnly);
    assert!(top < river, "client16402_ground_only: a deploying Knight crossed onto the water ({top} >= {river})");
}
