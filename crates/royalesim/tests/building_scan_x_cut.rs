//! A BUILDINGS-ONLY WALKER'S FAR-BUILDING CUT (target.rs `scan_with`; targeting.BUILDING_SCAN_X_CUT =
//! client16402_edge_6700_melee): a walker that attacks with a projectile has none (the Minion Giant takes a building it sees
//! whatever its |dx|), and a melee one ignores a building past 6700 of |dx| + its radius - the building's. Measured on
//! client 16.402 (the live population, parity's r62 item J): the Minion Giant's takes to |dx| 7908; the Royal Hog's,
//! the Balloon's and the Skeleton Balloon's take / keep edges at 6700 by the rule.
//!
//! The scene: a Blue walker held on (9000, 20000) on Red's half, a Red Cannon held due west of it at |dx| `dx` (nearer
//! than Red's left princess tower, 7778 away), and the walker's target after its deploy, under both arms.
//!
//! targeting.BUILDING_SCAN_X_CUT = client_sight_clip_side (shipped, both clients): the walker's own SightClipSide column,
//! |dx| > SightRange - SightClipSide + both radii (the Hog Rider 6700, the Royal Giant 6850, the Ram Rider's Ram no cut).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! building_scan_x_cut`): building_scan_cut_centre -> `a_ranged_walker_takes_a_far_building_and_a_melee_one_cuts_at_its_edge`
//! red; sight_clip_side_unread -> `under_client_sight_clip_side_a_walker_cuts_at_its_own_clip` red.
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

/// The shipped table with each walker's SightClipSide as the 16.402 and 15.535.29 rows set it (`sight_clip_side_milli`, Sim's
/// extractor, 0.1.28): the Hog Rider 4000, the Royal Giant 2000, the Ram Rider's Ram none.
fn with_clip() -> royalesim::state::BattleConfig {
    let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/derived/cards.json")).expect("cards.json");
    let mut v: serde_json::Value = serde_json::from_str(&text).expect("cards.json parses");
    for (name, clip) in [("HogRider", 4000), ("RoyalGiant", 2000)] {
        if let Some(c) = v["cards"].as_array_mut().expect("cards").iter_mut().find(|c| c["name"] == name) {
            c["sight_clip_side_milli"] = serde_json::Value::from(clip);
        }
        if let Some(u) = v["units"].get_mut(name) {
            u["sight_clip_side_milli"] = serde_json::Value::from(clip);
        }
    }
    let db = royalesim::card::CardDb::from_json_str(&v.to_string(), royalesim::card::CardSource::DerivedJson).expect("the edited table parses");
    royalesim::state::BattleConfig::with_cards(db)
}

/// `takes_the_cannon` on `cfg`.
fn takes_on(mut cfg: royalesim::state::BattleConfig, arm: BuildingScanXCut, walker: &str, dx: i32) -> bool {
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

/// targeting.BUILDING_SCAN_X_CUT = client_sight_clip_side (parity's r65 analysis: 4755 scans on client 15.535.29 and 861 on
/// 16.402, 0 wrong): the Hog Rider (sight 9500, clip 4000, radius 600) on a Cannon (600) takes at 6700 and cuts at 6720; the
/// Royal Giant (7500, 2000, 750) takes at 6850 and cuts at 6870, where client16402_edge_6700_melee (a projectile walker: no
/// cut) takes it (the vacuity check); the Ram Rider's Ram (no clip) takes at 7000. Plant: sight_clip_side_unread.
#[test]
fn under_client_sight_clip_side_a_walker_cuts_at_its_own_clip() {
    let arm = BuildingScanXCut::ClientSightClipSide;
    assert!(takes_on(with_clip(), arm, "HogRider", 6700), "the Hog Rider takes at 6700");
    assert!(!takes_on(with_clip(), arm, "HogRider", 6720), "the Hog Rider cuts at 6720");
    assert!(takes_on(with_clip(), arm, "RoyalGiant", 6850), "the Royal Giant takes at 6850");
    assert!(!takes_on(with_clip(), arm, "RoyalGiant", 6870), "the Royal Giant cuts at 6870");
    assert!(takes_on(with_clip(), BuildingScanXCut::Client16402Edge6700Melee, "RoyalGiant", 6870), "edge_6700_melee: the Royal Giant takes at 6870 (vacuity)");
    assert!(takes_on(with_clip(), arm, "RamRider", 7000), "the Ram Rider's Ram (no clip) takes at 7000");
}
