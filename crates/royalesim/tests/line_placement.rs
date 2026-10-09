//! A LINE'S PLACE (formation.rs `line_centre`; state.rs `formation_members_with`): the Royal Recruits' six in a line on
//! side 0, their centre where the client put it.
//!
//! THE MEASUREMENTS (client 15.535.29, Oracle's sp-rrtap-*: 59 accepted taps, x 2500 to 17000, y 3500 to 11500): each
//! line's centre on its first frame (the mean of its two middle members' x, of all six y). The two exact ties of the
//! princess rows ((4500, 6500) and (13500, 6500)) are left out: `line_centre` takes the other of the two, and no ring
//! order fits all five such taps.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! line_placement`):
//!   - line_centre_on_tap -> `the_royal_recruits_line_stands_where_the_client_put_it` red;
//!   - line_tap_relocated -> `a_line_put_down_stands_where_the_client_put_it` red (a tap on a princess box relocated
//!     first);
//!   - line_footprint_unjudged -> `a_line_is_taken_on_its_own_princess_box_and_refused_on_an_enemy_building` red;
//!   - line_centre_ring_walk -> `the_exact_ties_go_where_the_client_put_them_under_client15535_interleaved_first_ring`
//!     red;
//!   - line_back_row_closed -> `a_line_tapped_on_the_back_row_stands_there` red (formation.LINE_KING_BACK_ROW).
//!
//! formation.LINE_CENTRE_SEARCH = client15535_interleaved_first_ring (the client's search, read by Oracle on client
//! 15.535.29 and checked on the whole sweep, 245 of 246 runs) places the 13 exact ties where the client put them, which
//! the engine's ring_corner_walk does not.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, LineCentreSearch, LineKingBackRow};
use royalesim::Team;

/// (tap, the centre the client put the line on), native.
const TAPS: [((i32, i32), (i32, i32)); 57] = [
    ((2500, 3500), (6499, 4500)),
    ((2500, 4000), (6499, 4500)),
    ((2500, 4500), (6499, 4500)),
    ((2500, 5000), (6499, 4500)),
    ((2500, 5500), (6499, 4500)),
    ((2500, 6000), (6499, 4500)),
    ((2500, 6500), (6499, 4500)),
    ((2500, 7000), (6499, 8500)),
    ((2500, 7500), (6499, 8500)),
    ((2500, 8000), (6499, 8500)),
    ((2500, 8500), (6499, 8500)),
    ((2500, 11500), (6499, 11500)),
    ((3500, 4000), (6499, 4500)),
    ((3500, 4500), (6499, 4500)),
    ((3500, 5000), (6499, 4500)),
    ((3500, 5500), (6499, 4500)),
    ((3500, 6000), (6499, 4500)),
    ((3500, 7000), (6499, 8500)),
    ((3500, 7500), (6499, 8500)),
    ((3500, 8000), (6499, 8500)),
    ((3500, 8500), (6499, 8500)),
    ((4500, 3500), (6499, 4500)),
    ((4500, 4000), (6499, 4500)),
    ((4500, 4500), (6499, 4500)),
    ((4500, 5000), (6499, 4500)),
    ((4500, 5500), (6499, 4500)),
    ((4500, 6000), (6499, 4500)),
    ((4500, 7000), (6499, 8500)),
    ((4500, 7500), (6499, 8500)),
    ((4500, 8000), (6499, 8500)),
    ((4500, 8500), (6499, 8500)),
    ((4500, 11500), (6499, 11500)),
    ((6500, 4000), (6499, 4500)),
    ((6500, 4500), (6499, 4500)),
    ((6500, 5000), (6499, 4500)),
    ((6500, 5500), (6499, 4500)),
    ((6500, 6000), (6499, 4500)),
    ((6500, 7000), (6499, 8500)),
    ((6500, 7500), (6499, 8500)),
    ((6500, 8000), (6499, 8500)),
    ((6500, 8500), (6499, 8500)),
    ((9000, 4500), (11500, 4500)),
    ((9000, 5000), (11500, 4500)),
    ((9000, 5500), (11500, 4500)),
    ((9000, 6000), (9500, 8500)),
    ((9000, 7000), (8500, 8500)),
    ((9000, 7500), (8500, 8500)),
    ((9000, 8000), (9500, 8500)),
    ((9000, 8500), (9500, 8500)),
    ((13500, 3500), (11500, 4500)),
    ((13500, 11500), (11500, 11500)),
    ((15500, 3500), (11500, 4500)),
    ((15500, 6500), (11500, 8500)),
    ((15500, 11500), (11500, 11500)),
    ((17000, 3500), (11500, 4500)),
    ((17000, 6500), (11500, 8500)),
    ((17000, 11500), (11500, 11500)),
];

/// THE SAME TAPS PUT DOWN (`spawn_unit`, the replay's scenario spawn, which resolves a tap as a play does): the line's own
/// law reads the raw tap, with no troop relocation off a tower first.
#[test]
fn a_line_put_down_stands_where_the_client_put_it() {
    let mut wrong = Vec::new();
    for ((x, y), want) in TAPS {
        let mut s = BattleState::new(7, config());
        past_deploy_lockout(&mut s);
        s.spawn_unit(Team::Blue, "RoyalRecruits", Vec2::new(x * K, y * K), None).expect("the line");
        s.tick();
        let members: Vec<Vec2> = find_live(&s, Team::Blue, "RoyalRecruits").iter().map(|e| e.pos).collect();
        assert_eq!(members.len(), 6, "six recruits");
        let mut xs: Vec<i32> = members.iter().map(|m| m.x / K).collect();
        xs.sort();
        let cy = members.iter().map(|m| m.y / K).sum::<i32>() / 6;
        let got = ((xs[2] + xs[3]) / 2, cy);
        if got != want {
            wrong.push(((x, y), want, got));
        }
    }
    assert!(wrong.is_empty(), "taps whose line stands elsewhere put down (tap, client, here): {wrong:?}");
}

#[test]
fn the_royal_recruits_line_stands_where_the_client_put_it() {
    let s = BattleState::new(7, config());
    let mut wrong = Vec::new();
    for ((x, y), want) in TAPS {
        let members = s.formation_preview(Team::Blue, "RoyalRecruits", Vec2::new(x * K, y * K)).expect("the line");
        assert_eq!(members.len(), 6, "six recruits");
        let mut xs: Vec<i32> = members.iter().map(|m| m.1.x / K).collect();
        xs.sort();
        let cy = members.iter().map(|m| m.1.y / K).sum::<i32>() / 6;
        let got = ((xs[2] + xs[3]) / 2, cy);
        if got != want {
            wrong.push(((x, y), want, got));
        }
    }
    assert!(wrong.is_empty(), "taps whose line stands elsewhere (tap, client, here): {wrong:?}");
}

/// A LINE'S TAP IS JUDGED AS A TROOP'S (state.rs `check_position`, `resolve_point_judged`): taken on its own princess
/// tower's box, where a troop's tap is moved off the box (Oracle's sp-rrtap-*: the client took such taps), and refused
/// on an enemy building, as a Knight's tap is.
#[test]
fn a_line_is_taken_on_its_own_princess_box_and_refused_on_an_enemy_building() {
    use royalesim::state::DeployError;
    let mut cfg = config();
    const DECK: [&str; 8] = ["RoyalRecruits", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
    let deck: Vec<String> = DECK.iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    let mut s = BattleState::try_new(7, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let free = Vec2::new(5000 * K, 5000 * K);
    for card in ["RoyalRecruits", "Knight"] {
        assert_eq!(s.check_deploy(Team::Blue, card, free), Ok(()), "the scene: {card} playable on free ground");
    }
    let at = Vec2::new(9000 * K, 12500 * K);
    s.scenario_spawn_now(Team::Red, "Cannon", at, None).expect("a red Cannon on Blue's half");
    for card in ["RoyalRecruits", "Knight"] {
        assert_eq!(s.check_deploy(Team::Blue, card, at), Err(DeployError::Occupied), "{card} tapped on the red Cannon");
    }
    let own_box = Vec2::new(3500 * K, 6500 * K);
    assert_eq!(s.check_deploy(Team::Blue, "RoyalRecruits", own_box), Ok(()), "a line on its own princess box");
}

/// A native point.
type Point = (i32, i32);
/// A tap and the line centre the client put it on.
type Tap = (Point, Point);

/// The 13 exact ties of the sweep (tap, the centre the client put the line on), native: the two candidates equally near
/// the tap, the client's order taking one, ring_corner_walk the other.
const TIES: [Tap; 13] = [
    ((4000, 6500), (6499, 8500)),
    ((4250, 6500), (6499, 8500)),
    ((4500, 6500), (6499, 8500)),
    ((4750, 6500), (6499, 8500)),
    ((5000, 6500), (6499, 8500)),
    ((5250, 6500), (6499, 8500)),
    ((5500, 6500), (6499, 8500)),
    ((5750, 6500), (6499, 8500)),
    ((10000, 6250), (10500, 8500)),
    ((13000, 6500), (11500, 4500)),
    ((13250, 6500), (11500, 4500)),
    ((13500, 6500), (11500, 4500)),
    ((13750, 6500), (11500, 4500)),
];

/// The line's centre for each of `taps` under `arm` (formation_preview), as the measurements read it.
fn centres(arm: LineCentreSearch, taps: &[Tap]) -> Vec<(Point, Point, Point)> {
    let mut cfg: BattleConfig = config();
    cfg.calib.line_centre_search = arm;
    let s = BattleState::new(7, cfg);
    taps.iter()
        .map(|&((x, y), want)| {
            let members = s.formation_preview(Team::Blue, "RoyalRecruits", Vec2::new(x * K, y * K)).expect("the line");
            let mut xs: Vec<i32> = members.iter().map(|m| m.1.x / K).collect();
            xs.sort();
            let cy = members.iter().map(|m| m.1.y / K).sum::<i32>() / 6;
            ((x, y), want, ((xs[2] + xs[3]) / 2, cy))
        })
        .collect()
}

/// Plant: line_centre_ring_walk.
#[test]
fn the_exact_ties_go_where_the_client_put_them_under_client15535_interleaved_first_ring() {
    // NOT VACUOUS: the engine's walk takes the other of each tie.
    let old = centres(LineCentreSearch::RingCornerWalk, &TIES);
    assert!(old.iter().all(|(_, want, got)| got != want), "ring_corner_walk: a tie went where the client put it: {old:?}");
    let new = centres(LineCentreSearch::Client15535InterleavedFirstRing, &TIES);
    let wrong: Vec<_> = new.iter().filter(|(_, want, got)| got != want).collect();
    assert!(wrong.is_empty(), "client15535_interleaved_first_ring: ties placed elsewhere (tap, client, here): {wrong:?}");
    // and every other measured tap stays where it was
    let rest = centres(LineCentreSearch::Client15535InterleavedFirstRing, &TAPS);
    let moved: Vec<_> = rest.iter().filter(|(_, want, got)| got != want).collect();
    assert!(moved.is_empty(), "client15535_interleaved_first_ring: measured taps placed elsewhere: {moved:?}");
}

/// Side 1 under formation.LINE_FRAME = client15535_y_reflection (sp-rrtap-s1-*, client 15.535.29): the client's search
/// runs in absolute coordinates, so a side 1 tap goes through `line_centre_client`'s y-mirror and back. Both orders agree
/// on these two taps: the pin guards that round trip (a wrong map back lays the line on the other half), not the order.
#[test]
fn a_side_1_line_goes_where_the_client_put_it_under_client15535_interleaved_first_ring() {
    let mut cfg: BattleConfig = config();
    cfg.calib.line_centre_search = LineCentreSearch::Client15535InterleavedFirstRing;
    cfg.calib.line_frame = royalesim::state::LineFrame::Client15535YReflection;
    let s = BattleState::new(7, cfg);
    let mut wrong = Vec::new();
    for ((x, y), want) in [((15500, 28500), (11500, 27499)), ((2500, 28500), (6499, 27499))] {
        let members = s.formation_preview(Team::Red, "RoyalRecruits", Vec2::new(x * K, y * K)).expect("the line");
        assert_eq!(members.len(), 6, "six recruits");
        let mut xs: Vec<i32> = members.iter().map(|m| m.1.x / K).collect();
        xs.sort();
        let got = ((xs[2] + xs[3]) / 2, members.iter().map(|m| m.1.y / K).sum::<i32>() / 6);
        if got != want {
            wrong.push(((x, y), want, got));
        }
    }
    assert!(wrong.is_empty(), "side 1 taps whose line stands elsewhere (tap, client, here): {wrong:?}");
}

/// formation.LINE_KING_BACK_ROW = client16402_open (parity's r62, item C): a line tapped on the back row behind the king
/// stands there. Client 16.402 (the live population): 10 of 10 Royal Recruits back-row taps, the six on y 250 / 750
/// centred on the tap (9500: x 2500 to 16500; 8500: 1499 to 15499, the left half's ground offset), where the engine moved
/// the line four tiles up. Under both searches; the closed arm moves it (vacuity). Plant: line_back_row_closed.
#[test]
fn a_line_tapped_on_the_back_row_stands_there() {
    for search in [LineCentreSearch::RingCornerWalk, LineCentreSearch::Client15535InterleavedFirstRing] {
        let centre = |arm: LineKingBackRow, (x, y): Point| -> Point {
            let mut cfg: BattleConfig = config();
            cfg.calib.line_centre_search = search;
            cfg.calib.line_king_back_row = arm;
            let s = BattleState::new(7, cfg);
            let members = s.formation_preview(Team::Blue, "RoyalRecruits", Vec2::new(x * K, y * K)).expect("the line");
            assert_eq!(members.len(), 6, "six recruits");
            let mut xs: Vec<i32> = members.iter().map(|m| m.1.x / K).collect();
            xs.sort();
            ((xs[2] + xs[3]) / 2, members.iter().map(|m| m.1.y / K).sum::<i32>() / 6)
        };
        for (tap, want) in [((9500, 500), (9500, 500)), ((8500, 500), (8499, 500))] {
            let closed = centre(LineKingBackRow::Closed, tap);
            assert!(closed.1 >= 4000, "{search:?} closed: the line is moved off the back row (vacuity) {closed:?}");
            assert_eq!(centre(LineKingBackRow::Client16402Open, tap), want, "{search:?} client16402_open: the line stands on the tap {tap:?}");
        }
        // the measured 15.535.29 taps (y 3500 up) stand where they stood
        for ((x, y), want) in TAPS {
            {
                assert_eq!(centre(LineKingBackRow::Client16402Open, (x, y)), centre(LineKingBackRow::Closed, (x, y)), "{search:?}: tap {:?} (client {want:?}) moved by the back row", (x, y));
            }
        }
    }
}
