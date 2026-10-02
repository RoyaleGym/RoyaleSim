//! A BUILDING TAP BETWEEN TWO EQUALLY NEAR RELOCATIONS -- calibration placement.RELOCATION_TIE_ORDER, state.rs
//! `building_placement`.
//!
//! THE READING (client 15.535.29, the sp-esk-bank scenes): an Elixir Collector (3x3) tapped beside a river corner, whose
//! box overlaps the corner's NO_DEPLOY tile, is moved one tile in one of two directions equally near the tap. Blue
//! (1500, 13500) -> (1500, 12500) and (16500, 13500) -> (16500, 12500), both -y; Red (1500, 18500) -> (1500, 19500), +y,
//! and (16500, 18500) -> (15500, 18500), -x. All four are the first in the fixed arena order -y, -x, +y, +x (the order
//! placement.TOWER_TAP_PUSH measured on troop taps); the shipped ring order, built in the placer's frame so the seats
//! mirror, takes +y for the last. The order is seen from the TAP: a Blue Cannon (3x3) tapped on a tile edge,
//! (9000, 14500), its box over the river, between (8500, 13500) and (9500, 13500) took (8500, 13500) in 10 of 10 captures
//! (the bldtaps scenes, sp-m3-barrage2-s0); seen from the snapped tile (9500, 14500) the order takes -y, (9500, 13500).
//!
//! PLANT (regression): relocation_ties_mirrored -> `the_client_arm_ties_in_the_arena_order` red;
//! relocation_ties_from_snapped -> `a_tie_on_a_tile_edge_is_seen_from_the_tap` red.
//!   RUSTFLAGS='--cfg clash_plant="relocation_ties_mirrored"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test relocation_ties
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, RelocationTieOrder};
use royalesim::Team;

fn landing(arm: RelocationTieOrder, team: Team, tap: (i32, i32)) -> (i32, i32) {
    landing_of("Elixir Collector", arm, team, tap)
}

/// Where `card`, a building, tapped on `tap` by `team` lands under `arm`.
fn landing_of(card: &str, arm: RelocationTieOrder, team: Team, tap: (i32, i32)) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.relocation_tie_order = arm;
    // The catalogue holds the decks' cards (and what they make): the buildings ride in both.
    let deck: Vec<String> = ["Elixir Collector", "Cannon", "Knight", "Archers", "Giant", "Musketeer", "Fireball", "Arrows"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    let s = BattleState::try_new(7, cfg).expect("the decks load");
    let idx = s.config().cards.index(card).expect("the building in the catalogue");
    let (c, _) = s.building_placement(team, idx, Vec2::new(tap.0 * K, tap.1 * K)).expect("the tap is relocated, not refused");
    (c.x / K, c.y / K)
}

/// Plant: relocation_ties_mirrored.
#[test]
fn the_client_arm_ties_in_the_arena_order() {
    let arm = RelocationTieOrder::Client15535ArenaClockwise;
    assert_eq!(landing(arm, Team::Blue, (1500, 13500)), (1500, 12500));
    assert_eq!(landing(arm, Team::Blue, (16500, 13500)), (16500, 12500));
    assert_eq!(landing(arm, Team::Red, (1500, 18500)), (1500, 19500));
    assert_eq!(landing(arm, Team::Red, (16500, 18500)), (15500, 18500), "Red's right corner: -x before +y in the arena order");
}

#[test]
fn the_shipped_arm_mirrors_the_seats() {
    assert_eq!(Calib::shipped().relocation_tie_order, RelocationTieOrder::PlacerFrameFirstFound);
    let arm = RelocationTieOrder::PlacerFrameFirstFound;
    assert_eq!(landing(arm, Team::Blue, (16500, 13500)), (16500, 12500));
    assert_eq!(landing(arm, Team::Red, (16500, 18500)), (16500, 19500), "the placer's frame takes its back, +y");
}

/// A Blue Cannon tapped on a tile edge, (9000, 14500): its two equally near fits (8500, 13500) and (9500, 13500) sit at
/// (-500, -1000) and (+500, -1000) from the tap, and the first going clockwise from -y is the client's, 10 of 10. Plant:
/// relocation_ties_from_snapped (from the snapped tile (9500, 14500) the order takes -y).
#[test]
fn a_tie_on_a_tile_edge_is_seen_from_the_tap() {
    let arm = RelocationTieOrder::Client15535ArenaClockwise;
    assert_eq!(landing_of("Cannon", arm, Team::Blue, (9000, 14500)), (8500, 13500));
}
