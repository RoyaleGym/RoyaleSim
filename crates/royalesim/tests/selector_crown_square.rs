//! spells.SELECTOR_REACH = client16635_crown_square_strict: how an action's selector (the Void's strikes, the Vines'
//! catches; spell.rs `selector_candidates`) reaches a crown tower.
//!
//! THE LAW, measured on client 16.635 (parity r101, the oracle's il-mg-16635 set): a Void cast 2000 across and 3000 along
//! from a princess tower's centre (3605 from it, 105 past the troop law's 2500 plus its radius 1000; its square of
//! half-side 1000 is 2236 away) struck it at each of its strikes, and a Vines so cast caught it, 2 of 2; the 17 other casts
//! that reached a crown tower agree under both readings. The crown square is the one an ordinary spell's impact reaches
//! (spells.CROWN_TOWER_SPELL_REACH, spells.CROWN_SQUARE_HALF_SIDE), strictly inside the radius.
//!
//! THE SCENE: Blue's Void cast at an offset from the Red left princess tower's centre, toward the arena's middle, with
//! nothing else on the board; the tower's hp losses in the 100 ticks after the cast. WHAT IS PINNED:
//!   1. client16635_crown_square_strict: cast 2000 across and 3000 along (its square 2236 away), the tower is struck;
//!   2. client16635_building_square_strict (the old arm, the vacuity check): the tower is never struck there;
//!   3. cast 3000 across and 3000 along (its square 2828 away), the tower is struck under neither arm;
//!   4. the shipped arm is client16635_building_square_strict.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test selector_crown_square`):
//!   * `selector_crown_circle` -- the new arm reads a crown tower's circle: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, SelectorReach};
use royalesim::Team;

/// Under spells.SELECTOR_REACH = `arm`: Blue's Void cast `off` (native: across, along, both toward the arena's middle) from
/// the Red left princess tower's centre, nothing else on the board. The tower's hp losses as (tick k after the cast, loss)
/// over the 100 ticks after it.
fn tower_losses(arm: SelectorReach, off: (i32, i32)) -> Vec<(u32, i32)> {
    let mut cfg = config();
    cfg.calib.selector_reach = arm;
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    let tower = s.tower_ids(Team::Red)[1].expect("the Red left princess tower");
    let p = s.entity(tower).expect("the tower").pos;
    let sx = if p.x < 9000 * K { 1 } else { -1 };
    let sy = if p.y < 16000 * K { 1 } else { -1 };
    let tap = Vec2::new(p.x + sx * off.0 * K, p.y + sy * off.1 * K);
    s.spawn_unit(Team::Blue, "DarkMagic", tap, None).expect("cast DarkMagic");
    let mut hp = s.entity(tower).expect("the tower").hp;
    let mut out = Vec::new();
    for k in 0..100u32 {
        s.tick();
        let now = s.entity(tower).map_or(0, |e| e.hp);
        if now != hp {
            out.push((k, hp - now));
        }
        hp = now;
    }
    out
}

/// Plant: selector_crown_circle.
#[test]
fn under_client16635_crown_square_strict_the_void_strikes_a_princess_tower_by_its_square() {
    let old = tower_losses(SelectorReach::Client16635BuildingSquareStrict, (2000, 3000));
    assert!(old.is_empty(), "client16635_building_square_strict (the vacuity check): the tower 3605 off was struck: {old:?}");
    let new = tower_losses(SelectorReach::Client16635CrownSquareStrict, (2000, 3000));
    assert!(!new.is_empty(), "client16635_crown_square_strict: the tower whose square is 2236 off was never struck");
    assert!(new.iter().all(|&(_, loss)| loss > 0), "client16635_crown_square_strict: a gain on the tower: {new:?}");
}

#[test]
fn a_tower_whose_square_is_out_of_reach_is_struck_under_neither_arm() {
    for arm in [SelectorReach::Client16635BuildingSquareStrict, SelectorReach::Client16635CrownSquareStrict] {
        let got = tower_losses(arm, (3000, 3000));
        assert!(got.is_empty(), "{arm:?}: the tower whose square is 2828 off was struck: {got:?}");
    }
}

/// Since parity r101 for clients 16.402 and 16.635 (client16635_building_square_strict before it; client 15.535.29
/// keeps radius_plus_target_radius).
#[test]
fn the_shipped_arm_is_client16635_crown_square_strict() {
    assert_eq!(
        royalesim::state::Calib::shipped().selector_reach,
        royalesim::state::SelectorReach::Client16635CrownSquareStrict
    );
    assert_eq!(config().calib.selector_reach, royalesim::state::SelectorReach::Client16635CrownSquareStrict);
}
