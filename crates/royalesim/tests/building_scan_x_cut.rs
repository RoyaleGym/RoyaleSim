//! A BUILDINGS-ONLY WALKER'S FAR-BUILDING CUT (target.rs `scan_with`; targeting.BUILDING_SCAN_X_CUT =
//! client16402_edge_6700_melee): a walker that attacks with a projectile has none (the Minion Giant takes a building it sees
//! whatever its |dx|), and a melee one ignores a building past 6700 of |dx| + its radius - the building's. Measured on
//! client 16.402 (the live population, parity's r62 item J): the Minion Giant's takes to |dx| 7908; the Royal Hog's,
//! the Balloon's and the Skeleton Balloon's take / keep edges at 6700 by the rule.
//!
//! The scene: a Blue walker held on (9000, 20000) on Red's half, a Red Cannon held due west of it at |dx| `dx` (nearer
//! than Red's left princess tower, 7778 away), and the walker's target after its deploy, under both arms.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! building_scan_x_cut`): building_scan_cut_centre -> `a_ranged_walker_takes_a_far_building_and_a_melee_one_cuts_at_its_edge`
//! red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, BuildingScanXCut};
use royalesim::Team;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Whether a Blue `walker` held on (9000, 20000) takes a Red Cannon held at (9000 - dx, 20000) within 60 ticks of its
/// deploy, under `arm`.
fn takes_the_cannon(arm: BuildingScanXCut, walker: &str, dx: i32) -> bool {
    let mut cfg = config();
    cfg.calib.building_scan_x_cut = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let me = n((9000, 20000));
    let at = n((9000 - dx, 20000));
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", at, None).expect("the Cannon");
    let w = s.scenario_spawn_now(Team::Blue, walker, me, None).expect("the walker");
    for _ in 0..60 {
        assert!(s.debug_set_pos(w, me));
        assert!(s.debug_set_pos(cannon, at));
        s.tick();
        if s.entity(w).and_then(|e| e.target) == Some(cannon) {
            return true;
        }
    }
    false
}

/// Plant: building_scan_cut_centre.
#[test]
fn a_ranged_walker_takes_a_far_building_and_a_melee_one_cuts_at_its_edge() {
    // The Minion Giant (a projectile) at |dx| 7000: cut under centre_6750, taken under the new arm.
    assert!(!takes_the_cannon(BuildingScanXCut::Centre6750, "MinionGiant", 7000), "centre_6750 cuts the Minion Giant at 7000 (vacuity)");
    assert!(takes_the_cannon(BuildingScanXCut::Client16402Edge6700Melee, "MinionGiant", 7000), "the Minion Giant takes a building it sees at 7000");
    // The Hog Rider (melee, radius 600) on a Cannon (600): the edge rule cuts past 6700, the old centre rule past 6750.
    assert!(takes_the_cannon(BuildingScanXCut::Client16402Edge6700Melee, "HogRider", 6700), "the Hog Rider takes at 6700");
    assert!(!takes_the_cannon(BuildingScanXCut::Client16402Edge6700Melee, "HogRider", 6720), "the Hog Rider cuts at 6720 by the edge rule");
    assert!(takes_the_cannon(BuildingScanXCut::Centre6750, "HogRider", 6720), "centre_6750 takes at 6720 (the arms differ there)");
}
