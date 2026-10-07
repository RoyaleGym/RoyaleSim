//! A FALLEN LANE'S BRIDGE TAKES A TROOP -- calibration arena.TERRITORY_MODEL = enemy_tower_no_deploy_rects_open_bridge
//! (arena.rs `Territory::EnemyTowerRects::open_bridge`, `territory_zone`; state.rs `deploy_rule`).
//!
//! THE EVIDENCE. 252k IL_Replay ladder matches (RoyaleTraining's BRIDGE COUNT, one-tile landing cells): no troop or
//! building lands on river rows 15-16 by a side that took no crown (0 of 10,840) or in the lane whose princess stands,
//! and after a fall 5,058 river-row plays land on the bridge columns. Client 15.535.29 (sp-il-04cb t3211): Red Skeletons
//! tapped on the left bridge after Blue's left princess fell were created there.
//!
//! WHAT IS PINNED, Blue Knights at level 11, Red's left princess killed:
//!   1. before the fall every bridge tap is refused (the standing princess's rect reaches the far bank);
//!   2. after it the left bridge takes the Knight on both river rows, on the bridge's middle tile column: taps at x 3000
//!      and 3500 stand at x 3500, as the client's did (Oracle's client taps, 2026-10-07, taps:bridge-after: taps at
//!      3000, 3500 and 4000 on rows 15500 and 16500 stood at (3499, row), the 1 under a left-lane heading; its tap at
//!      4000 is the one this file does not pin: the engine snaps it to 4500, the bridge's water edge). The right bridge
//!      (its princess standing) and a river cell off the bridges are refused as before;
//!   3. the closed arm (enemy_tower_no_deploy_rects) refuses the fallen lane's bridge too, so (2) reads the key.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! bridge_after_fall`):
//!   * `bridge_closed_after_fall` -- the open arm keeps the river band closed: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::arena::TerritoryModel;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, DeployError};
use royalesim::entity::EntityKind;
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// A battle of Blue Knights against Red Knights under `model`, past the deploy lockout, Red's LEFT princess killed when
/// `fallen`.
fn battle(model: TerritoryModel, fallen: bool) -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Knight".into()], vec!["Knight".into()]];
    cfg.card_level = [11, 11];
    cfg.calib.territory_model = model;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    if fallen {
        let left = s
            .entities()
            .find(|e| e.team == Team::Red && e.kind == EntityKind::PrincessTower && e.pos.x < n(9000, 0).x)
            .map(|e| e.id)
            .expect("Red's left princess");
        assert!(s.debug_set_hp(left, 0));
        s.tick();
        assert!(s.entity(left).is_none(), "Red's left princess is down");
        assert_eq!(s.check_deploy(Team::Blue, "Knight", n(3500, 18500)), Ok(()), "the pocket behind the fallen lane opens");
    }
    s
}

const SHIPPED: TerritoryModel = TerritoryModel::EnemyTowerNoDeployRectsOpenBridge;
/// Taps on the left bridge, each with the point the client stood the Knight on (Oracle's taps:bridge-after).
const LEFT_BRIDGE: [((i32, i32), (i32, i32)); 4] = [
    ((3000, 15500), (3500, 15500)),
    ((3500, 15500), (3500, 15500)),
    ((3000, 16500), (3500, 16500)),
    ((3500, 16500), (3500, 16500)),
];
const RIGHT_BRIDGE: [(i32, i32); 2] = [(14500, 15500), (14500, 16500)];

#[test]
fn the_shipped_model_is_the_open_bridge() {
    assert_eq!(config().calib.territory_model, SHIPPED, "calibration arena.TERRITORY_MODEL");
}

#[test]
fn before_the_fall_every_bridge_tap_is_refused() {
    let s = battle(SHIPPED, false);
    for (x, y) in LEFT_BRIDGE.iter().map(|(tap, _)| tap).chain(&RIGHT_BRIDGE) {
        assert_eq!(s.check_deploy(Team::Blue, "Knight", n(*x, *y)), Err(DeployError::OutOfTerritory), "({x}, {y})");
    }
}

/// Plant: bridge_closed_after_fall.
#[test]
fn after_the_fall_its_bridge_takes_a_troop_where_it_was_tapped() {
    for ((x, y), (sx, sy)) in LEFT_BRIDGE {
        let mut s = battle(SHIPPED, true);
        assert_eq!(s.check_deploy(Team::Blue, "Knight", n(x, y)), Ok(()), "({x}, {y}) on the fallen lane's bridge");
        assert_eq!(s.deploy(Team::Blue, "Knight", n(x, y)), Ok(n(sx, sy)), "({x}, {y}) stands where the client stood it");
    }
    let s = battle(SHIPPED, true);
    for (x, y) in RIGHT_BRIDGE {
        assert_eq!(s.check_deploy(Team::Blue, "Knight", n(x, y)), Err(DeployError::OutOfTerritory), "({x}, {y}): its princess stands");
    }
    assert_eq!(s.check_deploy(Team::Blue, "Knight", n(5500, 15500)), Err(DeployError::Water), "off the bridges");
}

#[test]
fn the_closed_model_refuses_the_fallen_lanes_bridge() {
    let s = battle(TerritoryModel::EnemyTowerNoDeployRects, true);
    for ((x, y), _) in LEFT_BRIDGE {
        assert_eq!(s.check_deploy(Team::Blue, "Knight", n(x, y)), Err(DeployError::OutOfTerritory), "({x}, {y})");
    }
}
