//! A TROOP TAP BETWEEN TWO EQUALLY NEAR RELOCATIONS -- calibration placement.TROOP_RELOCATION_TIE_ORDER, state.rs
//! `ring_nearest_fit`.
//!
//! THE READING (client 15.535.29, Oracle's sp-bt2 scenes): a Knight tapped on an own Cannon's or Elixir Collector's box
//! by the river, where the axis push (placement.TOWER_TAP_PUSH) runs into the water, is moved by the ring search to one
//! of two fits equally near the tap, two tiles to -x and +x. All 10 such taps, both seats, went to arena -x: side 1's
//! (14500, 17500) by a Cannon on (14500, 18500) stood on (12500, 17499), and (3500, 18500), a Collector's centre, on
//! (1499, 18499), over +y and +x; side 0's (14500, 14500) by a Cannon on (14500, 13500) on (12500, 14500). The shipped
//! ring walk, built in the placer's frame, takes side 1's +x.
//!
//! PLANT (regression): troop_relocation_ties_walked -> `the_client_arm_ties_in_the_arena_order` red.
//!   RUSTFLAGS='--cfg clash_plant="troop_relocation_ties_walked"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test troop_relocation_ties
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, TroopRelocationTieOrder};
use royalesim::Team;

/// Where a Knight tapped on `tap` by `team` is resolved to under `arm`, with `building` (its card, its point) of the
/// same side standing.
fn landing(arm: TroopRelocationTieOrder, team: Team, building: (&str, (i32, i32)), tap: (i32, i32)) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.troop_relocation_tie_order = arm;
    // The catalogue holds the decks' cards: the buildings ride in both.
    let deck: Vec<String> = ["Elixir Collector", "Cannon", "Knight", "Archers", "Giant", "Musketeer", "Fireball", "Arrows"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    let mut s = BattleState::try_new(7, cfg).expect("the decks load");
    let (card, at) = building;
    let at = Vec2::new(at.0 * K, at.1 * K);
    s.spawn_unit(team, card, at, None).unwrap_or_else(|e| panic!("{card}: {e:?}"));
    s.tick();
    assert!(s.entities().any(|e| e.team == team && s.cards().get(e.card_idx).name == card && e.pos == at), "the scene drifted: no {card} on its point");
    let idx = s.cards().index("Knight").expect("the Knight in the catalogue");
    let p = s.resolve_point(team, idx, Vec2::new(tap.0 * K, tap.1 * K));
    (p.x / K, p.y / K)
}

/// Plant: troop_relocation_ties_walked.
#[test]
fn the_client_arm_ties_in_the_arena_order() {
    let arm = TroopRelocationTieOrder::Client15535ArenaClockwise;
    assert_eq!(landing(arm, Team::Red, ("Cannon", (14500, 18500)), (14500, 17500)).0, 12500, "side 1 by its Cannon: -x");
    assert_eq!(landing(arm, Team::Red, ("Elixir Collector", (3500, 18500)), (3500, 18500)).0, 1500, "side 1 on its Collector's centre: -x over +y and +x");
    assert_eq!(landing(arm, Team::Blue, ("Cannon", (14500, 13500)), (14500, 14500)).0, 12500, "side 0 by its Cannon: -x");
}

#[test]
fn the_shipped_arm_mirrors_the_seats() {
    assert_eq!(Calib::shipped().troop_relocation_tie_order, TroopRelocationTieOrder::PlacerFrameFirstFound);
    let arm = TroopRelocationTieOrder::PlacerFrameFirstFound;
    // NOT VACUOUS: the placer's frame takes side 1's +x.
    assert_eq!(landing(arm, Team::Red, ("Cannon", (14500, 18500)), (14500, 17500)).0, 16500, "side 1 by its Cannon: the placer's -x");
    assert_eq!(landing(arm, Team::Red, ("Elixir Collector", (3500, 18500)), (3500, 18500)).0, 5500, "side 1 on its Collector's centre");
    assert_eq!(landing(arm, Team::Blue, ("Cannon", (14500, 13500)), (14500, 14500)).0, 12500, "side 0 by its Cannon: the arms agree");
}
