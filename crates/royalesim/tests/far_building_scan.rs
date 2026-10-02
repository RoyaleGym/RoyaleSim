//! A BUILDINGS-ONLY WALKER'S SCAN IGNORES A FAR BUILDING (target.rs `BUILDING_SCAN_DX`, `scan_with`): a player building
//! more than 6750 across in x is no candidate, however near by centre. Measured on client 15.535.29 (Oracle's 18
//! building scenes): taken at |dx| up to 6231, never at 7231 or more while it stood in sight and nearer than the tower;
//! a Hog Rider (sp-il-925e) never at 6811 to 6972 and taken at 6697: the cut-off in [6697, 6811).
//!
//! The scene, Oracle's right-lane geometry: Blue's Giant held at (14731, 17500), just over the river on the right lane,
//! the red princess tower 8003 away; a red Cannon at (8500, 18500), |dx| 6231, 6311 away, or at (7500, 18500), |dx|
//! 7231, 7300 away. Both are in sight (7500 and both radii) and nearer than the tower.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! far_building_scan`): building_scan_dx_unbounded -> `the_giant_ignores_a_cannon_more_than_7000_across` red;
//! building_scan_dx_7000 -> `the_giant_ignores_a_cannon_6811_across` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

const BLUE: [&str; 8] = ["Giant", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const RED: [&str; 8] = ["Cannon", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const GIANT: (i32, i32) = (14731, 17500);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Giant held on GIANT, a red Cannon on `cannon`: its target on each of `ticks` ticks after both are down.
fn targets(cannon: (i32, i32), ticks: u32) -> (Vec<Option<EntityId>>, EntityId) {
    targets_at(GIANT, cannon, ticks)
}

/// `targets` with the Giant held on `giant`.
fn targets_at(giant: (i32, i32), cannon: (i32, i32), ticks: u32) -> (Vec<Option<EntityId>>, EntityId) {
    let mut cfg = config();
    cfg.decks = [BLUE.iter().map(|c| c.to_string()).collect(), RED.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let c = s.scenario_spawn_now(Team::Red, "Cannon", n(cannon), None).expect("the Cannon");
    let g = s.scenario_spawn_now(Team::Blue, "Giant", n(giant), None).expect("the Giant");
    let mut out = Vec::new();
    for _ in 0..ticks {
        assert!(s.debug_set_pos(g, n(giant)));
        s.tick();
        out.push(s.entity(g).expect("the Giant").target);
    }
    (out, c)
}

#[test]
fn the_giant_takes_a_cannon_6231_across() {
    let (t, cannon) = targets((8500, 18500), 60);
    assert!(t.contains(&Some(cannon)), "the Cannon at |dx| 6231 (in sight, nearer) never taken: {t:?}");
}

#[test]
fn the_giant_ignores_a_cannon_more_than_7000_across() {
    let (t, cannon) = targets((7500, 18500), 60);
    assert!(t.iter().any(|x| x.is_some()), "the scene drifted: the Giant took nothing: {t:?}");
    assert!(!t.contains(&Some(cannon)), "the Cannon at |dx| 7231 was taken: {t:?}");
}

/// The Hog Rider's band (sp-il-925e): the Giant held 6811 across from the Cannon at (7500, 18500) (in sight, 6884 away;
/// the red princess tower 8002 away) ignores it. Plant: building_scan_dx_7000.
#[test]
fn the_giant_ignores_a_cannon_6811_across() {
    let (t, cannon) = targets_at((7500 + 6811, 17500), (7500, 18500), 60);
    assert!(t.iter().any(|x| x.is_some()), "the scene drifted: the Giant took nothing: {t:?}");
    assert!(!t.contains(&Some(cannon)), "the Cannon at |dx| 6811 was taken: {t:?}");
}

/// ... and 6697 across (6771 away) takes it.
#[test]
fn the_giant_takes_a_cannon_6697_across() {
    let (t, cannon) = targets_at((7500 + 6697, 17500), (7500, 18500), 60);
    assert!(t.contains(&Some(cannon)), "the Cannon at |dx| 6697 (in sight, nearer) never taken: {t:?}");
}
