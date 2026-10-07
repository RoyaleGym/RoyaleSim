//! targeting.LAUNCH_BEYOND_KEEP (item 308; target.rs `decide`): what a projectile troop keeps after a launch at its target
//! from beyond its reach (Range + both radii).
//!
//! Client 15.535.29 (tools keep_band_census.py over the scenario fixtures): leaving its attack, 8 of 8 kept a target 1 to
//! 25 past the reach and 5 of 5 let one 26 to 100 past go; sp-il-2c295ac9 t2649, a Hero Musketeer that launched 4 past her
//! reach kept her target. The engine's launch flag ended the keep and her rescan found nothing within her sight.
//!
//! The scene: a Blue Musketeer and a Red Giant held on their points; the Musketeer takes the Giant 50 inside her reach,
//! then the Giant is held 10 past it (inside the keep's 25) through her first launch. Under client15535_plain_keep she
//! holds the Giant on the tick after the launch; under rescan she lets it go (nothing else in her sight). Plant:
//! launch_beyond_drops_plain_keep.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, LaunchBeyondKeep};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`: the Musketeer's target on the tick after her first launch is the Giant.
fn keeps_after_launch(arm: LaunchBeyondKeep) -> bool {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.launch_beyond_keep = arm;
    let mut s = BattleState::new(3, cfg);
    let m_at = n(9000, 12500);
    let ids = s
        .scenario_spawn_batch(&[(Team::Blue, "Musketeer", m_at, None), (Team::Red, "Giant", n(9000, 19600), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (m, g) = (ids[0], ids[1]);
    let reach = {
        let (mv, gv) = (s.entity(m).unwrap(), s.entity(g).unwrap());
        (s.config().cards.get(mv.card_idx).range + mv.radius + gv.radius) / K
    };
    let inside = n(9000, 12500 + reach - 50);
    let past = n(9000, 12500 + reach + 10);
    let mut attacking = false;
    for _ in 0..200 {
        assert!(s.debug_set_pos(m, m_at));
        assert!(s.debug_set_pos(g, if attacking { past } else { inside }));
        s.tick();
        let me = s.entity(m).expect("the Musketeer");
        if !attacking && me.target == Some(g) && me.attack_ms > 0 {
            attacking = true;
        }
        if attacking && s.projectiles().iter().any(|p| p.firer == Some(m)) {
            assert!(s.debug_set_pos(m, m_at));
            assert!(s.debug_set_pos(g, past));
            s.tick();
            return s.entity(m).expect("the Musketeer").target == Some(g);
        }
    }
    panic!("the scene drifted: no launch (attacking {attacking})");
}

/// Plant: launch_beyond_drops_plain_keep.
#[test]
fn under_client15535_plain_keep_a_launch_10_past_reach_keeps_the_target() {
    assert!(keeps_after_launch(LaunchBeyondKeep::Client15535PlainKeep), "client15535_plain_keep: the Musketeer let the Giant 10 past her reach go");
    // NOT VACUOUS: the engine's rescan finds nothing in her sight and lets it go.
    assert!(!keeps_after_launch(LaunchBeyondKeep::Rescan), "rescan: the Musketeer kept the Giant 10 past her reach");
}
