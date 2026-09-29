//! spells.BUILDING_SPELL_REACH (spell.rs `impact`, `in_square`): the shape a spell's impact reaches an ordinary
//! building by.
//!
//! The law, read on client 15.535.29 (item 57): a Fireball that landed 3,162 from a Cannon's centre killed it -- the
//! Cannon's square of half-side 600 (its collision radius) was 2,433 away, inside the Fireball's 2,500, where the disc
//! (2,500 + 600 = 3,100, edge-inclusive) is not. The crown towers' square (spells.CROWN_TOWER_SPELL_REACH) has the same
//! shape and the same strict test.
//!
//! The scenes: Red's Cannon at (9500, 22500); Blue's Fireball at a point off its corner. Pinned:
//!   1. client_square_radius_strict: a Fireball 3,162 from the Cannon's centre along its diagonal (the square 2,314
//!      away) hits it; aoe_hit_test misses it;
//!   2. a troop keeps the disc under both arms: a Knight at the Cannon's place is missed from the same point;
//!   3. the square is strict: a Fireball whose distance to the square is exactly its radius misses under the new arm;
//!   4. the shipped value is client_square_radius_strict, since the round-12 flip.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test building_spell_reach`):
//!   building_spell_disc   an ordinary building is a disc under the new arm too: (1) goes red.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, BuildingSpellReach, Calib, TapSnap};
use royalesim::Team;

const AT: (i32, i32) = (9500, 22500);

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Red's `victim` at AT, Blue's Fireball cast at `cast`: the hp the victim lost to it (against a control battle with no
/// Fireball, so a Cannon's own decay does not count).
fn lost(arm: BuildingSpellReach, victim: &str, cast: (i32, i32)) -> i32 {
    lost_with(arm, victim, cast, None)
}

/// `lost`, with a Blue building put down at `anchor` first: a Red troop at AT takes it as its target and stands
/// attacking it, so the troop is where the Fireball lands against, not walking.
fn lost_with(arm: BuildingSpellReach, victim: &str, cast: (i32, i32), anchor: Option<(&str, (i32, i32))>) -> i32 {
    let mut cfg = config();
    cfg.calib.building_spell_reach = arm;
    // the cast lands on its exact point
    cfg.calib.placement_tap_snap = TapSnap::None;
    let mut s = BattleState::new(1, cfg);
    past_deploy_lockout(&mut s);
    if let Some((card, at)) = anchor {
        s.scenario_spawn_now(Team::Blue, card, n(at), None).expect("the anchor");
    }
    let id = s.scenario_spawn_now(Team::Red, victim, n(AT), None).expect("the victim");
    let mut control = s.clone();
    s.spawn_unit(Team::Blue, "Fireball", n(cast), None).expect("the Fireball");
    // Until the tick after the Fireball lands (its object gone), no longer: what the victim does later is not the cast's.
    let mut flew = false;
    for _ in 0..200 {
        s.tick();
        control.tick();
        if !s.spells().is_empty() {
            flew = true;
        } else if flew {
            // the landing tick: the victim still stands where it was put (a hit's push steps from the next tick)
            let stood = s.entity(id).map_or(true, |e| e.pos == n(AT));
            assert!(stood, "the scene drifted: the {victim} left its place before the Fireball landed");
            s.tick();
            control.tick();
            break;
        }
    }
    assert!(flew, "the scene drifted: no Fireball flew");
    control.entity(id).map_or(0, |e| e.hp) - s.entity(id).map_or(0, |e| e.hp)
}

#[test]
fn a_fireball_off_a_cannons_corner_hits_it_under_the_square_and_misses_it_under_the_disc() {
    // 2,236 along each axis: 3,162 from the centre, the square (half-side 600) 1,636 x sqrt 2 = 2,314 away.
    let cast = (AT.0 + 2236, AT.1 - 2236);
    assert!(lost(BuildingSpellReach::SquareRadiusStrict, "Cannon", cast) > 0, "the square: the Fireball hits the Cannon");
    assert_eq!(lost(BuildingSpellReach::AoeHitTest, "Cannon", cast), 0, "the disc: the Fireball misses the Cannon");
}

#[test]
fn a_troop_keeps_the_disc_under_both_arms() {
    // A Red Knight standing at AT, attacking a Blue Cannon 1,500 below it, and a Fireball 3,162 off its diagonal: the
    // disc (2,500 + 500) misses it, where a square of half-side 500 (2,455 away) would not.
    let cast = (AT.0 + 2236, AT.1 + 2236);
    let anchor = Some(("Cannon", (AT.0, AT.1 - 1500)));
    for arm in [BuildingSpellReach::SquareRadiusStrict, BuildingSpellReach::AoeHitTest] {
        assert_eq!(lost_with(arm, "Knight", cast, anchor), 0, "{arm:?}: a Knight at the Cannon's place is hit from 3,162");
    }
    // The control: the same Knight is hit from just inside the disc, so the scene can hit a troop at all.
    assert!(lost_with(BuildingSpellReach::SquareRadiusStrict, "Knight", (AT.0 + 2100, AT.1 + 2100), anchor) > 0, "the scene drifted: the Knight is not hit from 2,970");
}

#[test]
fn the_square_is_strict() {
    // Straight along x: the square's side is 600 from the centre, so a point 3,100 away is exactly 2,500 (the
    // Fireball's radius) from the square, and misses; 3,099 hits.
    assert_eq!(lost(BuildingSpellReach::SquareRadiusStrict, "Cannon", (AT.0 + 3100, AT.1)), 0, "on the square's reach: a miss");
    assert!(lost(BuildingSpellReach::SquareRadiusStrict, "Cannon", (AT.0 + 3099, AT.1)) > 0, "just inside: a hit");
}

#[test]
fn the_shipped_value_is_the_square_since_the_round_12_flip() {
    assert_eq!(Calib::shipped().building_spell_reach, BuildingSpellReach::SquareRadiusStrict);
}
