//! combat.TROOP_SPLASH_CROWN_REACH = client16635_crown_square_strict: a unit's splash reaches a crown tower by its square, as a
//! spell's area does.
//!
//! Plant: troop_splash_crown_disc (`under_client16635_crown_square_strict_an_ice_wizard_splash_beside_a_tower_reaches_it` red).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, TroopSplashCrownReach};
use royalesim::Team;

/// Red's princess tower at (14500, 25500); a red Knight held at (-2290, -1150) from its centre (2562 off: past the Ice Wizard's
/// splash 1500 + the tower's 1000; 1298 from its square) and a blue Ice Wizard held 4000 south of the Knight (both kept at
/// full hitpoints), under combat.TROOP_SPLASH_CROWN_REACH = `arm`: the hitpoints the tower loses in 120 ticks.
fn splash_beside_tower(arm: TroopSplashCrownReach) -> i32 {
    let mut cfg = config();
    cfg.calib.troop_splash_crown_reach = arm;
    let mut s = BattleState::new(0, cfg);
    let tower = s.entities().find(|v| v.team == Team::Red && v.card == "PrincessTower" && v.pos.x / K == 14500).map(|v| v.id).expect("red's right princess tower");
    let tpos = s.entity(tower).expect("the tower").pos;
    assert_eq!((tpos.x / K, tpos.y / K), (14500, 25500), "the scene drifted: the tower's point");
    let kat = Vec2::new((14500 - 2290) * K, (25500 - 1150) * K);
    let wat = Vec2::new(kat.x, kat.y - 4000 * K);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kat, None).expect("the Knight");
    let wiz = s.scenario_spawn_now(Team::Blue, "IceWizard", wat, None).expect("the Ice Wizard");
    let t0 = s.entity(tower).expect("the tower").hp;
    for _ in 0..120u32 {
        for (id, at) in [(knight, kat), (wiz, wat)] {
            if let Some(e) = s.entity(id) {
                let m = e.max_hp;
                assert!(s.debug_set_pos(id, at) && s.debug_set_hp(id, m));
            }
        }
        s.tick();
    }
    t0 - s.entity(tower).expect("the tower stands").hp
}

/// combat.TROOP_SPLASH_CROWN_REACH = client16635_crown_square_strict (client 16.635, parity r95, the oracle's il-mg-16635 set: an Ice
/// Wizard's splash 2617 from a princess tower's centre, 1293 from its square, took it): the Ice Wizard's shots on a Knight
/// standing beside the tower take hitpoints from it; under disc (the vacuity check) none. Plant: troop_splash_crown_disc.
#[test]
fn under_client16635_crown_square_strict_an_ice_wizard_splash_beside_a_tower_reaches_it() {
    assert_eq!(splash_beside_tower(TroopSplashCrownReach::Disc), 0, "disc (the vacuity check): the splash reached the tower");
    assert!(splash_beside_tower(TroopSplashCrownReach::Client16635CrownSquareStrict) > 0, "client16635_crown_square_strict: the splash beside the tower took nothing from it");
}

/// Since parity r96 for clients 16.402 and 16.635 (disc before it; client 15.535.29 keeps disc).
#[test]
fn the_shipped_arm_is_client16635_crown_square_strict() {
    assert_eq!(
        royalesim::state::Calib::shipped().troop_splash_crown_reach,
        royalesim::state::TroopSplashCrownReach::Client16635CrownSquareStrict
    );
    assert_eq!(config().calib.troop_splash_crown_reach, royalesim::state::TroopSplashCrownReach::Client16635CrownSquareStrict);
}
