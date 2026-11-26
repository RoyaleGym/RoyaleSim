//! placement.TAP_REFUSAL = client16635_own_cell: WHICH TROOP AND BUILDING TAPS ARE REFUSED, AND WHERE THE REST GO
//! (arena.rs `own_cell_refusal`; state.rs `own_cell_taps`, `own_cell_troop_tap`, `check_position`, `building_placement`).
//!
//! THE MEASUREMENT (client 16.635.1, its own battle engine run headless): a Cannon, a Knight and the Archers tapped on
//! every tile centre of the arena, 576 points per grid, with no princess tower down, the left, the right or both, Blue and
//! Red. The client refused the same 72 points in every grid, each on its own WATER or NO_DEPLOY half-cell, and placed every
//! other point, moving an illegal one to the nearest legal point: out of a standing princess tower's rect, the enemy
//! king's, the enemy half, a bridge edge, a cell beside a king block. The touched-cell rule refuses most of them.
//!
//! WHAT IS PINNED: (1) under client 16.635's placement (`placement16635`) the engine refuses exactly the client's 72 points, for a Cannon and a
//! Knight; (2) the Cannon and the Knight land where the client put them, Blue and Red, a princess tower down or none; (3)
//! the touched-cell rule refuses those taps; (4) the shipped value is unchanged, and the client's values load.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test tap_refusal16635`):
//!   * `tap_refusal_touched_cells` -- the new arm still refuses by the touched cells: (1) and (2) go red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, TapRefusal};
use royalesim::Team;
use std::collections::BTreeMap;

/// The client's refused tile centres, every grid alike: (y, the x's refused on that row).
const REFUSED: [(i32, &[i32]); 12] = [
    (500, &[500, 1500, 2500, 3500, 4500, 12500, 13500, 14500, 15500, 16500, 17500]),
    (1500, &[7500, 8500, 9500]),
    (2500, &[7500, 8500, 9500]),
    (3500, &[7500, 8500, 9500]),
    (14500, &[500, 17500]),
    (15500, &[500, 1500, 4500, 5500, 6500, 7500, 8500, 9500, 10500, 11500, 12500, 15500, 16500, 17500]),
    (16500, &[500, 1500, 4500, 5500, 6500, 7500, 8500, 9500, 10500, 11500, 12500, 15500, 16500, 17500]),
    (17500, &[500, 17500]),
    (27500, &[7500, 8500, 9500]),
    (28500, &[7500, 8500, 9500]),
    (29500, &[7500, 8500, 9500]),
    (31500, &[500, 1500, 2500, 3500, 4500, 12500, 13500, 14500, 15500, 16500, 17500]),
];

const RIGHT: (i32, i32) = (14500, 25500);
const BLUE_RIGHT: (i32, i32) = (14500, 6500);

/// (card, side, princess towers destroyed, tap, where the client's unit stood three ticks after the play).
type Landing = (&'static str, Team, &'static [(i32, i32)], (i32, i32), (i32, i32));
const LANDINGS: [Landing; 20] = [
    ("Cannon", Team::Blue, &[RIGHT], (6500, 22500), (10500, 19500)),
    ("Cannon", Team::Blue, &[RIGHT], (500, 30500), (10500, 19500)),
    ("Cannon", Team::Blue, &[RIGHT], (16500, 30500), (16500, 19500)),
    ("Cannon", Team::Blue, &[RIGHT], (14500, 21500), (14500, 19500)),
    ("Cannon", Team::Blue, &[RIGHT], (2500, 15500), (2500, 13500)),
    ("Cannon", Team::Blue, &[RIGHT], (5500, 500), (5500, 2500)),
    ("Cannon", Team::Blue, &[RIGHT], (10500, 2500), (12500, 2500)),
    ("Cannon", Team::Blue, &[RIGHT], (14500, 16500), (14500, 18500)),
    ("Cannon", Team::Blue, &[], (9500, 20500), (9500, 13500)),
    ("Cannon", Team::Blue, &[], (12500, 27500), (12500, 13500)),
    ("Cannon", Team::Red, &[BLUE_RIGHT], (9500, 4500), (10500, 12500)),
    ("Cannon", Team::Red, &[BLUE_RIGHT], (2500, 9500), (10500, 12500)),
    ("Knight", Team::Blue, &[RIGHT], (10500, 27500), (10500, 20500)),
    ("Knight", Team::Blue, &[RIGHT], (10500, 29500), (10500, 20500)),
    ("Knight", Team::Blue, &[RIGHT], (13500, 16500), (13500, 17500)),
    ("Knight", Team::Blue, &[], (5500, 500), (5499, 1500)),
    ("Knight", Team::Blue, &[RIGHT], (9500, 30500), (9500, 20500)),
    ("Knight", Team::Blue, &[], (2500, 16500), (2499, 14500)),
    ("Knight", Team::Red, &[BLUE_RIGHT], (10500, 4500), (10500, 11499)),
    ("Knight", Team::Red, &[BLUE_RIGHT], (13500, 15500), (13500, 14499)),
];

/// Client 16.635's placement and card table, the overrides these tests run: its tap rule, its illegal-tap move, and the
/// card table its grids were read on.
fn placement16635() -> BTreeMap<String, String> {
    [
        ("placement.TAP_REFUSAL", "\"client16635_own_cell\""),
        ("placement.ILLEGAL_TROOP_TAP", "\"client16635_nearest_legal_tile\""),
        ("cards.CARD_TABLE", "\"client160635007_20261103\""),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

/// A battle under client 16.635's placement, `arm` for placement.TAP_REFUSAL, with the princess towers at `down`
/// destroyed and past the opening lockout. The Cannon and the Knight are in both hands.
fn battle(arm: TapRefusal, down: &[(i32, i32)]) -> BattleState {
    let mut cfg = config();
    cfg.calib = Calib::shipped_with_overrides(&placement16635()).expect("the 16.635 placement loads").0;
    cfg.calib.tap_refusal = arm;
    let deck: Vec<String> = ["Cannon", "Knight", "Archers", "Giant", "Musketeer", "Minions", "Fireball", "Arrows"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.shuffle_decks = false;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    for &(x, y) in down {
        let t = s.entities().find(|v| v.card == "PrincessTower" && v.pos.x / K == x && v.pos.y / K == y).map(|v| v.id).expect("the princess tower");
        assert!(s.debug_set_hp(t, 0));
    }
    for _ in 0..100 {
        s.tick();
    }
    for &(x, y) in down {
        assert!(!s.entities().any(|v| v.card == "PrincessTower" && v.pos.x / K == x && v.pos.y / K == y), "the scene drifted: ({x}, {y}) stands");
    }
    s
}

/// Where `side`'s `card` tapped on `tap` stands three ticks after the play (native), or None if refused.
fn landing(s: &BattleState, side: Team, card: &str, tap: (i32, i32)) -> Option<(i32, i32)> {
    let mut c = s.clone();
    let before: Vec<_> = s.entities().map(|v| v.id).collect();
    let at = c.deploy(side, card, Vec2::new(tap.0 * K, tap.1 * K)).ok()?;
    if card == "Cannon" {
        return Some((at.x / K, at.y / K));
    }
    for _ in 0..3 {
        c.tick();
    }
    let stood = c.entities().find(|v| v.team == side && v.card == card && !before.contains(&v.id)).map(|v| (v.pos.x / K, v.pos.y / K));
    stood
}

/// (1) Plant: tap_refusal_touched_cells.
#[test]
fn only_the_clients_72_points_are_refused() {
    let s = battle(TapRefusal::Client16635OwnCell, &[]);
    for card in ["Cannon", "Knight"] {
        let mut wrong = Vec::new();
        for y in (500..32000).step_by(1000) {
            for x in (500..18000).step_by(1000) {
                let refused = REFUSED.iter().any(|(ry, xs)| *ry == y && xs.contains(&x));
                if landing(&s, Team::Blue, card, (x, y)).is_none() != refused {
                    wrong.push((x, y, refused));
                }
            }
        }
        assert!(wrong.is_empty(), "{card}: (x, y, the client refused) where the engine differs: {wrong:?}");
    }
}

/// (2) Plant: tap_refusal_touched_cells.
#[test]
fn a_moved_tap_lands_where_the_client_put_it() {
    for (card, side, down, tap, want) in LANDINGS {
        let s = battle(TapRefusal::Client16635OwnCell, down);
        assert_eq!(landing(&s, side, card, tap), Some(want), "{card} {side:?} tapped on {tap:?}, towers down {down:?}");
    }
}

/// (3) The vacuity check: the touched-cell rule refuses every one of those taps.
#[test]
fn touched_cells_refuses_them() {
    let mut right = Vec::new();
    for (card, side, down, tap, want) in LANDINGS {
        let s = battle(TapRefusal::TouchedCells, down);
        if landing(&s, side, card, tap) == Some(want) {
            right.push((card, tap));
        }
    }
    assert!(right.is_empty(), "touched_cells already lays these as the client: {right:?}");
}

/// (4)
#[test]
fn the_shipped_value_is_unchanged_and_the_clients_values_load() {
    assert_eq!(Calib::shipped().tap_refusal, TapRefusal::TouchedCells);
    let c = Calib::shipped_with_overrides(&placement16635()).expect("the 16.635 placement loads").0;
    assert_eq!(c.tap_refusal, TapRefusal::Client16635OwnCell);
}
