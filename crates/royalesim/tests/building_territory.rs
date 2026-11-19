//! placement.BUILDING_TERRITORY = enemy_tower_no_deploy_rects: a building may go down in an opened pocket (an enemy princess
//! down), as a troop may.
//!
//! Plant: building_territory_own_half (`under_enemy_tower_no_deploy_rects_a_cannon_goes_down_in_the_opened_pocket` red).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, BuildingTerritory};
use royalesim::Team;

/// Blue's Cannon tapped at (3500, 18500), past the river in Red's left lane, with Red's left princess tower down (or standing,
/// `down` false), under placement.BUILDING_TERRITORY = `arm`: whether `building_placement` puts it down.
fn pocket_cannon(arm: BuildingTerritory, down: bool) -> bool {
    let mut cfg = config();
    cfg.calib.building_territory = arm;
    let mut s = BattleState::new(0, cfg);
    if down {
        let t = s.entities().find(|v| v.team == Team::Red && v.card == "PrincessTower" && v.pos.x / K == 3500).map(|v| v.id).expect("red's left princess");
        assert!(s.debug_set_hp(t, 0));
        s.tick();
        s.tick();
        assert!(s.entity(t).is_none(), "the scene drifted: the princess still stands");
    }
    let idx = s.cards().index("Cannon").expect("the Cannon");
    s.building_placement(Team::Blue, idx, Vec2::new(3500 * K, 18500 * K)).is_some()
}

/// placement.BUILDING_TERRITORY = enemy_tower_no_deploy_rects (client 16.635, parity r95, the oracle's il-mg-16635 set: 3 of 3
/// buildings played in an opened pocket put down there): Blue's Cannon goes down in Red's opened left pocket; under own_half
/// (the vacuity check) it is refused, and with the princess standing both refuse it. Plant: building_territory_own_half.
#[test]
fn under_enemy_tower_no_deploy_rects_a_cannon_goes_down_in_the_opened_pocket() {
    assert!(!pocket_cannon(BuildingTerritory::OwnHalf, true), "own_half (the vacuity check): the Cannon went down past the river");
    assert!(!pocket_cannon(BuildingTerritory::EnemyTowerNoDeployRects, false), "enemy_tower_no_deploy_rects: the Cannon went down inside a standing princess's rect");
    assert!(pocket_cannon(BuildingTerritory::EnemyTowerNoDeployRects, true), "enemy_tower_no_deploy_rects: the Cannon was refused in the opened pocket");
}
