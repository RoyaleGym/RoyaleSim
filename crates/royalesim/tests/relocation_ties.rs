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
//! client15535_interleaved_ring, the client's ring order (read by Oracle on client 15.535.29), keeps those five and takes
//! the tie the arena order misses: sp-form-Tesla-evo-s0's second Tesla (2x2), tapped on (15500, 2500) with the first on
//! (15000, 2000), stood on (16000, 4000), +1 +2 tiles from its corner, where the arena order takes (15000, 4000) (both
//! 2,500,000 from the tap, as (17000, 2000) and (17000, 3000) are).
//!
//! PLANT (regression): relocation_ties_mirrored -> `the_client_arm_ties_in_the_arena_order` red;
//! relocation_ties_from_snapped -> `a_tie_on_a_tile_edge_is_seen_from_the_tap` red;
//! relocation_ties_clockwise -> `a_tesla_tapped_on_a_tesla_stands_where_the_client_put_it_under_client15535_interleaved_ring`
//! red.
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

/// Where a Tesla tapped on `tap` lands under `arm`, a Blue Tesla standing on `standing`.
fn tesla_landing(arm: RelocationTieOrder, standing: (i32, i32), tap: (i32, i32)) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.relocation_tie_order = arm;
    let deck: Vec<String> = ["Tesla", "Cannon", "Knight", "Archers", "Giant", "Musketeer", "Fireball", "Arrows"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    let mut s = BattleState::try_new(7, cfg).expect("the decks load");
    let at = Vec2::new(standing.0 * K, standing.1 * K);
    s.spawn_unit(Team::Blue, "Tesla", at, None).expect("the first Tesla");
    s.tick();
    assert!(s.entities().any(|e| e.team == Team::Blue && e.card == "Tesla" && e.pos == at), "the scene drifted: no Tesla on {standing:?}");
    let idx = s.config().cards.index("Tesla").expect("the Tesla in the catalogue");
    let (c, _) = s.building_placement(Team::Blue, idx, Vec2::new(tap.0 * K, tap.1 * K)).expect("the tap is relocated, not refused");
    (c.x / K, c.y / K)
}

/// sp-form-Tesla-evo-s0 (client 15.535.29): the second Tesla tapped on the first. Plant: relocation_ties_clockwise.
#[test]
fn a_tesla_tapped_on_a_tesla_stands_where_the_client_put_it_under_client15535_interleaved_ring() {
    // NOT VACUOUS: the arena order takes (15000, 4000).
    assert_eq!(tesla_landing(RelocationTieOrder::Client15535ArenaClockwise, (15000, 2000), (15500, 2500)), (15000, 4000), "client15535_arena_clockwise");
    assert_eq!(tesla_landing(RelocationTieOrder::Client15535InterleavedRing, (15000, 2000), (15500, 2500)), (16000, 4000), "client15535_interleaved_ring");
}

/// client15535_interleaved_ring keeps the five ties client15535_arena_clockwise was measured on.
#[test]
fn the_interleaved_ring_keeps_the_arena_orders_measured_ties() {
    let arm = RelocationTieOrder::Client15535InterleavedRing;
    assert_eq!(landing(arm, Team::Blue, (1500, 13500)), (1500, 12500));
    assert_eq!(landing(arm, Team::Blue, (16500, 13500)), (16500, 12500));
    assert_eq!(landing(arm, Team::Red, (1500, 18500)), (1500, 19500));
    assert_eq!(landing(arm, Team::Red, (16500, 18500)), (15500, 18500));
    assert_eq!(landing_of("Cannon", arm, Team::Blue, (9000, 14500)), (8500, 13500));
}
