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
//! THE GRID FOLLOWS THE FUSE (item 282): the path grid is re-stamped when a bomb starts or ends its fuse, not only when a
//! building changes. Pinned: 3. a troop's bomb (a Giant Skeleton's, no building changing) is gone round by a route planned
//! in its fuse; 4. a Bomb Tower's bomb, stamped by the tower's death, leaves the grid when it bursts: a Giant put down 75
//! ticks after the kill walks straight up x 3250 (client 15.535.29: routes cross a bomb's cells after the blast; sp-il-323a
//! t3811, a Giant Skeleton's bomb still in the engine's grid 508 ticks after it burst).
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="death_bomb_never_blocks"' CARGO_TARGET_DIR=target/plant cargo test --test death_bomb_obstacle`):
//!   * `death_bomb_never_blocks` -- the new arm still plans through the bomb: (2) goes red.
//!   * `bomb_never_restamps` -- the grid's key is the epochs alone: (3) and (4) go red.
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

/// Under `arm`, a red `parent` put on (3500, 22500) and killed on its second tick (a troop: no building changes), a blue
/// Giant put down on (3250, 19500) `wait` ticks after the kill: the Giant's largest |x - 3250| and its largest y over its
/// first 50 ticks (native).
fn giant_after(arm: DeathBombObstacle, parent: &str, wait: u32) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.death_bomb_obstacle = arm;
    let mut s = BattleState::new(0, cfg);
    // Played, so it is killed in its deploy: a troop does not step there, and its bomb lies on its point, across the
    // Giant's lane, as the Bomb Tower's does.
    s.spawn_unit(Team::Red, parent, at((3500, 22500)), None).expect("the bomb's parent");
    s.tick();
    let p = s.entities().find(|e| e.team == Team::Red && e.card == parent).expect("the bomb's parent").id;
    assert!(s.entity(p).is_some_and(|e| e.deploying), "the scene drifted: the {parent} is not deploying");
    assert!(s.debug_set_hp(p, 0));
    for _ in 0..wait {
        s.tick();
    }
    assert!(s.entity(p).is_none(), "the scene drifted: the {parent} still stands");
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

/// Plant: bomb_never_restamps.
#[test]
fn a_troops_bomb_is_gone_round_by_a_route_planned_in_its_fuse_under_client15535_fused_building() {
    let (old_off, old_far) = giant_after(DeathBombObstacle::None, "GiantSkeleton", 2);
    // NOT VACUOUS: without the obstacle the Giant walks straight up its lane past the bomb's row.
    assert!(old_far > 21500 && old_off <= 60, "the scene drifted: none walked off x 3250 ({old_off}) or short of y 21500 ({old_far})");
    let (new_off, _) = giant_after(DeathBombObstacle::Client15535FusedBuilding, "GiantSkeleton", 2);
    assert!(new_off > 150, "client15535_fused_building: the Giant never left x 3250 by more than 150 round a Giant Skeleton's bomb ({new_off})");
}

/// Plant: bomb_never_restamps.
#[test]
fn a_bomb_leaves_the_path_grid_when_it_bursts_under_client15535_fused_building() {
    // NOT VACUOUS: put down in the fuse, the Giant goes round the Bomb Tower's bomb (the scene's own detour).
    let (fuse_off, _) = giant_after(DeathBombObstacle::Client15535FusedBuilding, "BombTower", 2);
    assert!(fuse_off > 150, "the scene drifted: in the fuse the Giant never left x 3250 by more than 150 ({fuse_off})");
    // 75 ticks after the kill the bomb (a 3,000 ms fuse) has burst: the route goes straight up x 3250 again.
    let (off, far) = giant_after(DeathBombObstacle::Client15535FusedBuilding, "BombTower", 75);
    assert!(off <= 60 && far > 21500, "client15535_fused_building: after the burst the Giant still went round the bomb's cells (off {off}, far {far})");
}
