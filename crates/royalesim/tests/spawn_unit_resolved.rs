//! `spawn_unit_resolved`: a troop at an OBSERVED creation point (a live capture's, which the client already resolved)
//! goes down on that point under placement.TAP_SNAP = client16402_tile_centre, where `spawn_unit` snaps the tap to its
//! tile centre and lays a single ground unit on the deploy point beside it. A spell goes down as through `spawn_unit`.
//! Under the old arm, none, the two are the same. The replay harness plays a corpus troop row through it
//! (examples/replay_parity/harness.rs), so the snap does not move a point the capture recorded.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, TapSnap};
use royalesim::Team;

fn native(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Where `card` played by Blue at `at` is laid (its first pending member), through `spawn_unit_resolved` when
/// `observed`, else `spawn_unit`, under `snap`.
fn laid(snap: TapSnap, card: &str, at: Vec2, observed: bool) -> Vec2 {
    let mut cfg = config();
    cfg.calib.placement_tap_snap = snap;
    let mut s = BattleState::new(1, cfg);
    if observed {
        s.spawn_unit_resolved(Team::Blue, card, at, None).expect("the observed point is taken");
    } else {
        s.spawn_unit(Team::Blue, card, at, None).expect("the tap is taken");
    }
    s.pending_spawns().into_iter().find(|(t, c, _)| *t == Team::Blue && s.cards().get(*c).name == card).map(|(_, _, p)| p).expect("nothing was laid")
}

#[test]
fn an_observed_troop_point_is_not_snapped_again() {
    let at = native(6321, 10789);
    assert_eq!(laid(TapSnap::TileCentre, "Knight", at, true), at, "the observed point moved");
    assert_ne!(laid(TapSnap::TileCentre, "Knight", at, false), at, "vacuous: spawn_unit did not snap this tap either");
}

#[test]
fn under_the_old_arm_the_two_lay_one_point() {
    let at = native(6321, 10789);
    assert_eq!(laid(TapSnap::None, "Knight", at, true), laid(TapSnap::None, "Knight", at, false));
}

#[test]
fn a_spell_goes_down_as_through_spawn_unit() {
    let at = native(4321, 16789);
    assert_eq!(laid(TapSnap::TileCentre, "Fireball", at, true), laid(TapSnap::TileCentre, "Fireball", at, false));
}
