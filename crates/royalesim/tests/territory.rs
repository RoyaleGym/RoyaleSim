//! TROOP TERRITORY -- the shipped NoDeploySize rects, enumerated.
//!
//! WHY IT EXISTS: the pocket that opens after a princess tower falls is usually
//! modelled as an unsourced depth (12 half-rows, or 8.5 tiles). It is not a depth:
//! buildings.csv's NoDeploySizeW/H fits four arena landmarks exactly (arena.rs TROOP
//! TERRITORY; calibration.json arena.TERRITORY_MODEL). This file checks the
//! engine's verdict at EVERY half-cell centre and corner, for both teams, before
//! and after each princess tower falls, against the rule written out here from
//! its definition -- not against the engine's own helper.
//!
//! THE RULE, AS SPECIFIED (and re-derived below):
//!   out of arena (not strictly inside)      -> OutOfArena
//!   touches a WATER cell                    -> Water
//!   touches a NO_DEPLOY cell                -> NoDeploy
//!   touches a river-band row                -> OutOfTerritory   (unsourced, kept)
//!   inside the CLOSED NoDeploySize rect of an ALIVE ENEMY crown tower -> OutOfTerritory
//!   touches (closed) a building footprint   -> Occupied
//!
//! PLANT: `territory_ignores_king_rect` drops the enemy king's rect from the
//! engine's list; the pocket then runs to the enemy king block and this file
//! must go red.
//!
//! WHAT IT CANNOT CATCH: whether the live 2026 game uses these sizes (the
//! registry's promotion rule), and points off the half-cell lattice other than
//! the +-1 subtile probes at rect edges.
mod common;

use royalesim::arena::Rect;
use royalesim::card::{KING_TOWER, PRINCESS_TOWER};
use royalesim::fixed::Vec2;
use royalesim::state::{footprint_of, BattleState, DeployError};
use royalesim::Team;
use common::*;

/// A battle where both hands hold a Knight in slot 0 and elixir is full, with
/// the given ENGINE tower slots (1 = engine-Left, 2 = engine-Right) of each team
/// destroyed before the battle.
fn board(down: &[(Team, usize)]) -> BattleState {
    let mut cfg = config();
    let deck: Vec<String> = ["Knight", "Archer", "Giant", "Minions", "Knight", "Archer", "Giant", "Minions"].iter().map(|s| s.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    let mut s = BattleState::new(3, cfg);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    for (team, k) in down {
        s.scenario_set_tower_hp(*team, *k, 0).unwrap();
    }
    assert_eq!(s.hand(Team::Blue)[0], "Knight");
    s
}

/// The alive enemy crown towers' rects, built from tower POSITIONS and cards.json
/// sizes (read, not copied): centre +- size / 2.
fn expected_rects(s: &BattleState, team: Team) -> Vec<Rect> {
    let enemy = team.other();
    s.tower_ids(enemy)
        .iter()
        .enumerate()
        .filter_map(|(k, id)| {
            let v = s.entity((*id)?)?;
            let name = if k == 0 { KING_TOWER } else { PRINCESS_TOWER };
            assert_eq!(v.card, name);
            let size = card_stat(s, name).no_deploy_size.expect("crown tower size in cards.json");
            Some(Rect { min: Vec2::new(v.pos.x - size.x / 2, v.pos.y - size.y / 2), max: Vec2::new(v.pos.x + size.x / 2, v.pos.y + size.y / 2) })
        })
        .collect()
}

/// The specified verdict for a troop at `p` (see the module doc).
fn expected(s: &BattleState, team: Team, p: Vec2) -> Result<(), DeployError> {
    let a = s.arena();
    if !(p.x > 0 && p.y > 0 && p.x < a.width && p.y < a.height) {
        return Err(DeployError::OutOfArena);
    }
    // Cells touched, closed on both sides, written out independently of arena.rs.
    let span = |v: i32, n: i32| {
        let i = v.div_euclid(a.cell);
        let lo = if v % a.cell == 0 { i - 1 } else { i };
        (lo.max(0), i.min(n - 1))
    };
    let (x0, x1) = span(p.x, a.cols);
    let (y0, y1) = span(p.y, a.rows);
    let mut bits = 0u8;
    for r in y0..=y1 {
        for c in x0..=x1 {
            bits |= a.cell_bits(c, r);
        }
    }
    if bits & a.bit_water != 0 {
        return Err(DeployError::Water);
    }
    if bits & a.bit_no_deploy != 0 {
        return Err(DeployError::NoDeploy);
    }
    let (river_lo, river_hi) = (a.water_y_min / a.cell, a.water_y_max / a.cell - 1);
    if (y0..=y1).any(|r| r >= river_lo && r <= river_hi) {
        return Err(DeployError::OutOfTerritory);
    }
    if expected_rects(s, team).iter().any(|r| p.x >= r.min.x && p.x <= r.max.x && p.y >= r.min.y && p.y <= r.max.y) {
        return Err(DeployError::OutOfTerritory);
    }
    let on_building = s.entities().filter(|e| e.kind.is_building()).any(|e| footprint_of(s, e.id).unwrap().covers_disc(p, 0));
    if on_building {
        return Err(DeployError::Occupied);
    }
    Ok(())
}

/// Every half-cell centre and corner, plus +-1 subtile around every expected rect edge.
fn probes(s: &BattleState, team: Team) -> Vec<Vec2> {
    let a = s.arena();
    let mut out = Vec::new();
    for gy in 0..=2 * a.rows {
        for gx in 0..=2 * a.cols {
            out.push(Vec2::new(gx * a.cell / 2, gy * a.cell / 2));
        }
    }
    for r in expected_rects(s, team) {
        for gy in 0..=a.rows {
            let y = gy * a.cell + a.cell / 2;
            for x in [r.min.x - 1, r.min.x, r.min.x + 1, r.max.x - 1, r.max.x, r.max.x + 1] {
                out.push(Vec2::new(x, y));
            }
        }
        for gx in 0..=a.cols {
            let x = gx * a.cell + a.cell / 2;
            for y in [r.min.y - 1, r.min.y, r.min.y + 1, r.max.y - 1, r.max.y, r.max.y + 1] {
                out.push(Vec2::new(x, y));
            }
        }
    }
    out
}

const STATES: [&[(Team, usize)]; 6] = [
    &[],
    &[(Team::Red, 1)],
    &[(Team::Red, 2)],
    &[(Team::Red, 1), (Team::Red, 2)],
    &[(Team::Blue, 1)],
    &[(Team::Blue, 2), (Team::Blue, 1)],
];

#[test]
fn troop_deploy_legality_matches_the_rect_model_everywhere() {
    let mut checked = 0usize;
    let mut legal = 0usize;
    for down in STATES {
        let s = board(down);
        for team in [Team::Blue, Team::Red] {
            for p in probes(&s, team) {
                let got = s.check_deploy_slot(team, 0, p);
                let want = expected(&s, team, p);
                assert_eq!(got, want, "towers down {down:?}: {team:?} Knight at {p:?}");
                checked += 1;
                legal += usize::from(got.is_ok());
            }
        }
    }
    println!("territory: {checked} probes, {legal} legal");
    assert!(checked > 50_000 && legal > 10_000 && checked - legal > 10_000, "vacuous: {checked} probes, {legal} legal");
}

/// Half-rows (from the far bank outward, in the team's own frame) holding at least
/// one legal troop cell centre on the ENGINE side `left_side`.
fn pocket_rows(s: &BattleState, team: Team, left_side: bool) -> Vec<i32> {
    let a = s.arena();
    let far_bank_row = a.water_y_max / a.cell; // first own-frame row past the river
    (far_bank_row..a.rows)
        .filter(|own_row| {
            let row = match team {
                Team::Blue => *own_row,
                Team::Red => a.rows - 1 - own_row,
            };
            (0..a.cols).any(|c| {
                let p = a.half_to_subtile_center(c, row);
                ((p.x * 2 < a.width) == left_side) && s.check_deploy_slot(team, 0, p).is_ok()
            })
        })
        .map(|r| r - far_bank_row)
        .collect()
}

#[test]
fn the_pocket_is_eight_half_rows_past_the_far_bank() {
    // Blue takes Red's engine-Left princess: Blue's pocket opens on the engine-left
    // side only, from the far bank to the enemy king rect edge (y = 21): 8 half-rows.
    // An unsourced 12-half-row depth would put it two tiles deeper.
    let s = board(&[(Team::Red, 1)]);
    assert_eq!(pocket_rows(&s, Team::Blue, true), (0..8).collect::<Vec<_>>());
    assert_eq!(pocket_rows(&s, Team::Blue, false), Vec::<i32>::new(), "the other side stays closed");
    let none = board(&[]);
    assert_eq!(pocket_rows(&none, Team::Blue, true), Vec::<i32>::new(), "no pocket while every tower stands");
    // Red takes Blue's engine-Right princess, which is Red's own-LEFT: Red's
    // pocket opens on the ENGINE-right side, 8 half-rows, as the rotation demands.
    let r = board(&[(Team::Blue, 2)]);
    assert_eq!(pocket_rows(&r, Team::Red, false), (0..8).collect::<Vec<_>>());
    assert_eq!(pocket_rows(&r, Team::Red, true), Vec::<i32>::new());
    // Both princesses down: the whole band across the width (8 half-rows), and
    // still nothing inside the king rect.
    let both = board(&[(Team::Red, 1), (Team::Red, 2)]);
    assert_eq!(pocket_rows(&both, Team::Blue, true), (0..8).collect::<Vec<_>>());
    assert_eq!(pocket_rows(&both, Team::Blue, false), (0..8).collect::<Vec<_>>());
}

#[test]
fn territory_is_rotation_invariant() {
    // Blue at p with Red's engine tower k down == Red at rotate(p) with Blue's
    // engine tower (3 - k) down (engine-Left and engine-Right swap under the
    // rotation), at every probe.
    let mut checked = 0usize;
    for (blue_down, red_down) in [(vec![], vec![]), (vec![(Team::Red, 1)], vec![(Team::Blue, 2)]), (vec![(Team::Red, 2)], vec![(Team::Blue, 1)])] {
        let sb = board(&blue_down);
        let sr = board(&red_down);
        for p in probes(&sb, Team::Blue) {
            let q = sb.arena().rotate(p);
            assert_eq!(sb.check_deploy_slot(Team::Blue, 0, p), sr.check_deploy_slot(Team::Red, 0, q), "Blue {p:?} / Red {q:?}");
            checked += 1;
        }
    }
    assert!(checked > 10_000);
}

#[test]
fn buildings_stay_own_half_after_a_princess_falls() {
    let s = board(&[(Team::Red, 1)]);
    let pocket = t(350, 1900);
    assert_eq!(s.check_deploy_slot(Team::Blue, 0, pocket), Ok(()), "a troop may use the pocket");
    let mut cfg = config();
    let deck: Vec<String> = ["Cannon", "Tesla", "Cannon", "Tesla", "Cannon", "Tesla", "Cannon", "Tesla"].iter().map(|s| s.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    let mut b = BattleState::new(3, cfg);
    b.scenario_set_elixir_milli(Team::Blue, 10_000);
    b.scenario_set_tower_hp(Team::Red, 1, 0).unwrap();
    assert_eq!(b.check_deploy_slot(Team::Blue, 0, pocket), Err(DeployError::OutOfTerritory), "a building may not");
    assert_eq!(b.check_deploy_slot(Team::Blue, 0, t(350, 1000)), Ok(()));
}
