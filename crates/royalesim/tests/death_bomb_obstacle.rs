//! pathfinding.DEATH_BOMB_OBSTACLE: what a death bomb on its fuse is to route planning (state.rs `build_obstacles`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, every route planned during a fuse that a straight line would
//! have taken through the bomb's cells went round them, 3 of 3 (sp-bridge-Giant-split-s0: the Giant's route one column
//! left of a dead Bomb Tower's bomb, where the engine's went straight through).
//!
//! THE SCENE: a red Bomb Tower on (3500, 22500) killed, its bomb on its fuse; a blue Giant put down on (3250, 19500) two
//! ticks later, under the left princess tower's lane; its x over its next 50 ticks.
//!
//! WHAT IS PINNED:
//!   1. the scene: the Bomb Tower is gone before the Giant is placed, and under the old arm the Giant walks north up to
//!      the bomb's rows (past y 21500 in its 50 ticks) within 60 of x 3250 (its straight route);
//!   2. client15535_fused_building: the Giant leaves x 3250 by more than 150 on the way (its route goes round the bomb);
//!      none (the engine's, the vacuity check): it does not.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="death_bomb_never_blocks"' CARGO_TARGET_DIR=target/plant cargo test --test death_bomb_obstacle`):
//!   * `death_bomb_never_blocks` -- the new arm still plans through the bomb: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeathBombObstacle};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Under `arm`: the Giant's largest |x - 3250| and its largest y over its first 50 ticks (native).
fn giant(arm: DeathBombObstacle) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.death_bomb_obstacle = arm;
    let mut s = BattleState::new(0, cfg);
    let tower = s.scenario_spawn_now(Team::Red, "BombTower", at((3500, 22500)), None).expect("the Bomb Tower");
    s.tick();
    assert!(s.debug_set_hp(tower, 0));
    s.tick();
    s.tick();
    assert!(s.entity(tower).is_none(), "the scene drifted: the Bomb Tower still stands");
    let g = s.scenario_spawn_now(Team::Blue, "Giant", at((3250, 19500)), None).expect("the Giant");
    let (mut off, mut far) = (0, 0);
    for _ in 0..50 {
        s.tick();
        let Some(e) = s.entity(g) else { break };
        off = off.max((e.pos.x / K - 3250).abs());
        far = far.max(e.pos.y / K);
    }
    (off, far)
}

/// Plant: death_bomb_never_blocks.
#[test]
fn a_route_planned_during_a_fuse_goes_round_the_bomb_under_client15535_fused_building() {
    let (old_off, old_far) = giant(DeathBombObstacle::None);
    // NOT VACUOUS: without the obstacle the Giant walks straight up its lane past the bomb's row.
    assert!(old_far > 21500 && old_off <= 60, "the scene drifted: none walked off x 3250 ({old_off}) or short of y 21500 ({old_far})");
    let (new_off, _) = giant(DeathBombObstacle::Client15535FusedBuilding);
    assert!(new_off > 150, "client15535_fused_building: the Giant never left x 3250 by more than 150 ({new_off})");
}
