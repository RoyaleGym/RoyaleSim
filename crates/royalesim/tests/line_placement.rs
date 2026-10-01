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
//!     first).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
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
