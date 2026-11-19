//! placement.SPELL_AS_DEPLOY_NO_DEPLOY_CELLS: whether a spell placed as a deploy that may stand on buildings (the Barbarian
//! Barrel, the Log, the Royal Delivery) is refused on its own half's tilemap NO_DEPLOY cells (state.rs `no_deploy_cells_open`).
//!
//! THE LAW, measured on client 16.635 (parity r97, the oracle's il-mg-16635 set): 52 of 52 Barbarian Barrels tapped on cells
//! touching an own NO_DEPLOY cell (the king block, the back-row edge strips, the river corners) were cast, each Barbarian
//! released 4500 ahead of the tap.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! spell_no_deploy_cells`):
//!   * `spell_no_deploy_cells_closed` -- the new arm still refuses those cells: both under_ tests go red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeployError, SpellAsDeployNoDeployCells as A};
use royalesim::Team;

const DECK: [&str; 8] = ["BarbLog", "Heal", "Knight", "Archers", "Giant", "Musketeer", "Minions", "Zap"];

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle(arm: A) -> BattleState {
    let mut cfg = config();
    cfg.calib.spell_as_deploy_no_deploy_cells = arm;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    s
}

/// Blue's taps on own NO_DEPLOY cells (the king block's row, a river corner, the back edge strip) and Red's rotated twins.
const TAPS: [(Team, (i32, i32)); 6] =
    [(Team::Blue, (8500, 4500)), (Team::Blue, (500, 14500)), (Team::Blue, (2500, 500)), (Team::Red, (9500, 27500)), (Team::Red, (17500, 17500)), (Team::Red, (15500, 31500))];

/// placement.SPELL_AS_DEPLOY_NO_DEPLOY_CELLS = client16635_can_place_on_buildings (client 16.635, parity r97, the oracle's
/// il-mg-16635 set: 52 of 52): a Barbarian Barrel tapped on its own NO_DEPLOY cells is taken on both seats; under refuse (the
/// vacuity check) each is refused NO_DEPLOY. The Heal (CanPlaceOnBuildings false), water and the enemy half still refuse.
/// Plant: spell_no_deploy_cells_closed.
#[test]
fn under_client16635_can_place_on_buildings_a_barbarian_barrel_is_taken_on_its_own_no_deploy_cells() {
    let old = battle(A::Refuse);
    let new = battle(A::Client16635CanPlaceOnBuildings);
    for (team, (x, y)) in TAPS {
        assert_eq!(old.check_deploy(team, "BarbLog", n(x, y)), Err(DeployError::NoDeploy), "refuse (the vacuity check): {team:?} at ({x}, {y})");
        assert_eq!(new.check_deploy(team, "BarbLog", n(x, y)), Ok(()), "client16635_can_place_on_buildings: {team:?} at ({x}, {y})");
    }
    assert_eq!(new.check_deploy(Team::Blue, "Heal", n(8500, 4500)), Err(DeployError::NoDeploy), "the Heal keeps the NO_DEPLOY test");
    assert_eq!(new.check_deploy(Team::Blue, "BarbLog", n(8500, 16000)), Err(DeployError::Water), "water still refuses");
    assert_eq!(new.check_deploy(Team::Red, "BarbLog", n(9500, 16000)), Err(DeployError::Water), "water still refuses Red");
    assert_eq!(new.check_deploy(Team::Blue, "BarbLog", n(8500, 20500)), Err(DeployError::OutOfTerritory), "the enemy half still refuses");
}

/// The Blue Barbarians standing after a Barbarian Barrel tapped on (8500, 4500), 80 ticks on, under `arm`.
fn rolled(arm: A) -> (Result<Vec2, DeployError>, Vec<(i32, i32)>) {
    let mut s = battle(arm);
    let got = s.deploy(Team::Blue, "BarbLog", n(8500, 4500));
    let mut seen = Vec::new();
    for _ in 0..80 {
        s.tick();
        seen = s.entities().filter(|e| e.team == Team::Blue && e.card.contains("Barbarian")).map(|e| (e.pos.x / K, e.pos.y / K)).collect();
        if !seen.is_empty() {
            break;
        }
    }
    (got, seen)
}

/// placement.SPELL_AS_DEPLOY_NO_DEPLOY_CELLS = client16635_can_place_on_buildings (client 16.635, il-mg 226ef901 t1491: tapped
/// on (8500, 4500), the Barbarian at (8500, 9000)): the barrel is played from the king block and its Barbarian stands 4500 ahead;
/// under refuse (the vacuity check) the play is refused and no Barbarian comes. Plant: spell_no_deploy_cells_closed.
#[test]
fn under_client16635_can_place_on_buildings_the_barrel_rolls_from_the_king_block() {
    let (got, seen) = rolled(A::Refuse);
    assert_eq!(got, Err(DeployError::NoDeploy), "refuse (the vacuity check): the play");
    assert!(seen.is_empty(), "refuse (the vacuity check): a Barbarian came {seen:?}");
    let (got, seen) = rolled(A::Client16635CanPlaceOnBuildings);
    assert!(got.is_ok(), "client16635_can_place_on_buildings: the play was refused {got:?}");
    assert_eq!(seen.len(), 1, "client16635_can_place_on_buildings: the Barbarians {seen:?}");
    let (x, y) = seen[0];
    assert!((8450..=8550).contains(&x) && (8500..=9500).contains(&y), "client16635_can_place_on_buildings: the Barbarian at ({x}, {y})");
}
