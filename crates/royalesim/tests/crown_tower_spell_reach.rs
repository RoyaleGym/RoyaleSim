//! spells.CROWN_TOWER_SPELL_REACH: the shape a spell's impact reaches a crown tower by (parity, round 9 item 35).
//!
//! Under client_square_1000_strict a crown tower is a square of half-side 1000 native round its centre, and an impact
//! reaches it when the distance from its point to that square is strictly below its radius; under aoe_hit_test, the
//! old arm, it is a disc read by spells.AOE_HIT_TEST (edge_inclusive: radius plus the tower's collision radius).
//! Each scene below is the client's, and the two arms disagree on each:
//!   - a Fireball (2500) at offset (2000, 3000) from a princess tower: square distance 2,236, a hit on the client; the
//!     disc misses (3,606 > 3,500);
//!   - a Rocket (2000) at offset (0, 3000): 2,000 is not below 2,000, a miss on the client; the disc hits;
//!   - a Zap (2500) at offset (-3500, -500) from a king tower: 2,500, a miss on the client; the disc hits.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test crown_tower_spell_reach`):
//!   crown_tower_spell_disc  a crown tower is a disc to every spell, whatever the arm:
//!                           the_client_arm_reaches_a_tower_by_its_square goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::spell::in_crown_square;
use royalesim::state::{BattleState, Calib, CrownTowerSpellReach};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The hp the tower standing at native (x, y) loses to Blue's `spell` cast at offset (dx, dy) from it, under `arm`.
fn tower_loss(arm: CrownTowerSpellReach, card: &str, tower: (i32, i32), off: (i32, i32)) -> i32 {
    let mut cfg = config();
    cfg.calib.crown_tower_spell_reach = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at = n(tower.0, tower.1);
    let hp = |s: &BattleState| s.entities().find(|e| e.team == Team::Red && e.pos == at).map(|e| e.hp).expect("the tower stands there");
    let before = hp(&s);
    s.spawn_unit(Team::Blue, card, n(tower.0 + off.0, tower.1 + off.1), None).expect("the cast");
    for _ in 0..400 {
        s.tick();
    }
    before - hp(&s)
}

const PRINCESS: (i32, i32) = (3500, 25500);
const KING: (i32, i32) = (9000, 29000);

#[test]
fn the_shipped_arm_is_client_square_1000_strict() {
    assert_eq!(Calib::shipped().crown_tower_spell_reach, CrownTowerSpellReach::Square1000Strict);
}

#[test]
fn the_square_is_strict_and_the_same_for_both_kinds() {
    let t = n(0, 0);
    assert!(in_crown_square(n(2000, 3000), t, 2500 * K), "the Fireball's 2,236");
    assert!(!in_crown_square(n(0, 3000), t, 2000 * K), "the Rocket's 2,000 is on the edge, not inside");
    assert!(!in_crown_square(n(-3500, -500), t, 2500 * K), "the Zap's 2,500 is on the edge, not inside");
    assert!(in_crown_square(n(0, 2999), t, 2000 * K), "one inside the edge");
    assert!(in_crown_square(n(1000, 1000), t, 1), "a point on the square");
}

#[test]
fn the_client_arm_reaches_a_tower_by_its_square() {
    let arm = CrownTowerSpellReach::Square1000Strict;
    assert!(tower_loss(arm, "Fireball", PRINCESS, (2000, -3000)) > 0, "the Fireball at (2000, 3000) hits the princess tower");
    assert_eq!(tower_loss(arm, "Rocket", PRINCESS, (0, -3000)), 0, "the Rocket at (0, 3000) misses the princess tower");
    assert_eq!(tower_loss(arm, "Zap", KING, (-3500, -500)), 0, "the Zap at (-3500, -500) misses the king");
}

#[test]
fn the_old_arm_reads_the_tower_as_a_disc() {
    // Not vacuous: the old arm lands each scene the other way.
    let arm = CrownTowerSpellReach::AoeHitTest;
    assert_eq!(tower_loss(arm, "Fireball", PRINCESS, (2000, -3000)), 0, "the disc misses the Fireball at (2000, 3000)");
    assert!(tower_loss(arm, "Rocket", PRINCESS, (0, -3000)) > 0, "the disc takes the Rocket at (0, 3000)");
    assert!(tower_loss(arm, "Zap", KING, (-3500, -500)) > 0, "the disc takes the Zap at (-3500, -500)");
}
