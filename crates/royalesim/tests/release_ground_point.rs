//! A LANDING SPELL'S GROUND UNITS ARE LAID AROUND THE LANDING POINT MOVED ONE NATIVE UNIT (state.rs
//! `release_ring_points`; spells.RELEASE_GROUND_POINT = client16402_one_unit), as formation.GROUND_DEPLOY_POINT moves a ground
//! summon's: absolute x one lower on the arena's left half, absolute y one lower for a side-1 owner. Measured on client 16.402
//! (the live population, parity's r62 item B): a Goblin Barrel on a left princess tower releases around x 3499, not 3500.
//!
//! The scene: one Goblin Barrel per seat and half, on open ground, the Goblins' first frame read under both arms. With no
//! other unit near, the new arm's three Goblins are the old arm's moved by exactly that unit.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! release_ground_point`): release_ground_point_unread -> `a_goblin_barrel_releases_one_unit_off_its_landing_point` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, ReleaseGroundPoint};
use royalesim::Team;

const DECK: [&str; 8] = ["GoblinBarrel", "BarbLog", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Zap"];

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Goblins of `team`'s Goblin Barrel landing on `at` (native), sorted, on their first frame, under `arm`.
fn goblins(arm: ReleaseGroundPoint, team: Team, at: (i32, i32)) -> Vec<(i32, i32)> {
    let mut cfg = config();
    cfg.calib.release_ground_point = arm;
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), DECK.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.spawn_unit(team, "GoblinBarrel", n(at), None).expect("the barrel");
    for _ in 0..200 {
        s.tick();
        let mut g: Vec<(i32, i32)> = s.entities().filter(|e| e.team == team && e.card == "Goblin").map(|e| (e.pos.x / K, e.pos.y / K)).collect();
        if !g.is_empty() {
            assert_eq!(g.len(), 3, "the scene drifted: {g:?}");
            g.sort();
            return g;
        }
    }
    panic!("the scene drifted: no Goblin appeared");
}

#[test]
fn a_goblin_barrel_releases_one_unit_off_its_landing_point() {
    // (team, landing point, the absolute move): Blue on the left half x - 1; Blue on the right half none; Red (side 1) on
    // the right half y - 1; Red on the left half both.
    for (team, at, d) in [
        (Team::Blue, (4500, 20500), (-1, 0)),
        (Team::Blue, (13500, 20500), (0, 0)),
        (Team::Red, (13500, 11500), (0, -1)),
        (Team::Red, (4500, 11500), (-1, -1)),
    ] {
        let old = goblins(ReleaseGroundPoint::None, team, at);
        let new = goblins(ReleaseGroundPoint::Client16402OneUnit, team, at);
        let moved: Vec<(i32, i32)> = old.iter().map(|&(x, y)| (x + d.0, y + d.1)).collect();
        assert_eq!(new, moved, "{team:?} on {at:?}: the Goblins are the old arm's moved by {d:?} (old {old:?})");
    }
}

/// The Barbarian of `team`'s Barbarian Barrel rolled from `at` (native), on its first frame, under `arm`.
fn barbarian(arm: ReleaseGroundPoint, team: Team, at: (i32, i32)) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.release_ground_point = arm;
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), DECK.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.spawn_unit(team, "BarbLog", n(at), None).expect("the barrel");
    for _ in 0..200 {
        s.tick();
        if let Some(e) = s.entities().find(|e| e.team == team && e.card == "Barbarian") {
            return (e.pos.x / K, e.pos.y / K);
        }
    }
    panic!("the scene drifted: no Barbarian appeared");
}

/// A ROLLING spell's release is not shifted (r62 item B; client 16.402, the live population: the Barbarian Barrel's Barbarian
/// at the log's end to the native unit on the left half, 54 of 54, where the shifted engine stood one off). Plant:
/// release_ground_point_shifts_rolling.
#[test]
fn a_barbarian_barrels_barbarian_is_released_on_its_point() {
    for (team, at) in [(Team::Blue, (3500, 9500)), (Team::Red, (3500, 22500))] {
        assert_eq!(
            barbarian(ReleaseGroundPoint::Client16402OneUnit, team, at),
            barbarian(ReleaseGroundPoint::None, team, at),
            "{team:?} on {at:?}: the Barbarian where the unshifted arm puts it"
        );
    }
}
