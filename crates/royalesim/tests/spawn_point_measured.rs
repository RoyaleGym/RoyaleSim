//! spawner.SPAWN_POINT = client16402_measured: where an emitted unit comes into being.
//!
//! The shipped arm is REFUTED by the recordings and the engine knows it: not one of the
//! Tombstone's 879 recorded emissions is within 250 native of either older candidate, while
//! 849 of 879 are within 250 of the two circles' tangent. This file gates the arm that
//! reproduces them.
//!
//! WHAT IS PINNED, each against the number the recordings give:
//!   1. a blank-SpawnRadius spawner emits FORWARD at its own radius plus the spawned
//!      unit's (Tombstone 1000 + 500 = 1500), on the owner's forward axis;
//!   2. a spawner that SETS SpawnRadius emits on a RING of that radius and NOT forward;
//!   3. the ring's angle is two laws: a blank SpawnAngleShift lays it out in the ABSOLUTE
//!      frame, a set one relative to the spawner's own facing;
//!   4. the two seats mirror;
//!   5. the older arms still behave as they did, because they remain selectable.
//!
//! The 0-means-blank reading of SpawnAngleShift is a claim about the DATA, so it is
//! checked here rather than assumed.

mod common;

use common::*;
use royalesim::fixed::Vec2;
use royalesim::state::{BattleConfig, BattleState, Calib, SpawnPoint};
use royalesim::Team;

fn bare(cfg: BattleConfig) -> BattleState {
    BattleState::new(7, cfg)
}

fn with_calib(f: impl FnOnce(&mut Calib)) -> BattleConfig {
    let mut cfg = config();
    f(&mut cfg.calib);
    cfg
}

fn measured() -> BattleConfig {
    with_calib(|c| c.spawner_spawn_point = SpawnPoint::Client16402Measured)
}

/// A spot on Blue's half clear of every tower and the river.
fn spot() -> Vec2 {
    t(600, 900)
}

/// Every emitted unit of `card`'s first wave, at the tick it materialises, WITH THE
/// SPAWNER'S OWN POSITION ON THAT TICK.
///
/// Both of those matter and the first version of this file got both wrong. The emitted
/// unit is born walking, so a position read even one tick late is the unit's walk and not
/// its emission point. And a spawner that is a TROOP has walked too: the Witch is nowhere
/// near where she was put down by the time she emits, so her ring must be measured against
/// where she stands, not against her deploy point. The Tombstone hides this, being a
/// building that cannot move.
fn first_wave(cfg: BattleConfig, card: &str, unit: &str, at: Vec2, team: Team) -> (Vec2, Vec<Vec2>) {
    let mut s = bare(cfg);
    let id = s.scenario_spawn_now(team, card, at, None).expect("the spawner goes down");
    for _ in 0..400 {
        s.tick();
        let seen = find_live(&s, team, unit);
        if !seen.is_empty() {
            let spawner_now = s.entity(id).expect("the spawner is still alive when it emits").pos;
            return (spawner_now, seen.iter().map(|e| e.pos).collect());
        }
    }
    panic!("{card} never emitted a {unit}");
}

/// Squared distance, so the ring can be checked without a square root. This crate carries
/// no floating point anywhere and tests do not get an exception.
fn d2(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x - b.x) as i64, (a.y - b.y) as i64);
    dx * dx + dy * dy
}

/// Catches the refuted arm coming back. The recordings put the Tombstone's Skeleton at
/// 1000 + 500 = 1500 forward, not at the spawner's own 1000 and not at its centre.
#[test]
fn a_blank_spawn_radius_emits_at_the_tangent_of_the_two_circles() {
    let s = bare(measured());
    let tomb_r = card_stat(&s, "Tombstone").collision_radius;
    let skel_r = card_stat(&s, "Skeleton").collision_radius;
    drop(s);

    let (at, points) = first_wave(measured(), "Tombstone", "Skeleton", spot(), Team::Blue);
    let forward = royalesim::arena::Arena::own_side_dy(Team::Blue) * -1;
    for p in &points {
        let along = (p.y - at.y) * forward;
        assert_eq!(p.x, at.x, "the emission is on the spawner's own column: {p:?}");
        assert_eq!(
            along,
            tomb_r + skel_r,
            "the emission is at the TANGENT (spawner radius {tomb_r} + unit radius {skel_r}), not at {along}"
        );
    }
    assert_ne!(tomb_r + skel_r, tomb_r, "this test cannot tell the arms apart if the unit has no radius");
}

/// Catches the SpawnRadius case being sent forward, which is how the old arm was wrong
/// in DIRECTION rather than in magnitude. The Witch's Skeletons sit on a ring, and their
/// median forward offset in the recordings is 74 against a radius of 2000.
#[test]
fn a_set_spawn_radius_emits_on_a_ring_and_not_forward() {
    let s = bare(measured());
    let radius = card_stat(&s, "Witch").formation.spawn_radius;
    drop(s);
    assert!(radius > 0, "the Witch must set SpawnRadius for this test to mean anything");

    let (at, points) = first_wave(measured(), "Witch", "Skeleton", spot(), Team::Blue);
    assert!(points.len() > 1, "the Witch emits a wave, not one unit");
    // Compared as SQUARES, so no square root and no floating point. A tolerance of two
    // subtiles on the radius becomes this band.
    let lo = (radius as i64 - 2) * (radius as i64 - 2);
    let hi = (radius as i64 + 2) * (radius as i64 + 2);
    for p in &points {
        let got = d2(*p, at);
        assert!(
            got >= lo && got <= hi,
            "every unit of the wave stands on the ring of {radius} (squared {lo}..{hi}), this one is at squared {got}"
        );
    }
    // NOT forward: the units are spread around the spawner, so their mean forward
    // offset is nothing like the radius.
    let mean_forward: i32 = points.iter().map(|p| p.y - at.y).sum::<i32>() / points.len() as i32;
    assert!(
        mean_forward.abs() < radius / 2,
        "the wave is pushed forward like the refuted arm: mean forward {mean_forward} against radius {radius}"
    );
}

/// THE TWO ANGLE LAWS. A blank SpawnAngleShift is laid out in the ABSOLUTE frame, so
/// turning the spawner must not move it; a set one is relative to the facing, so it must.
#[test]
fn a_blank_angle_shift_is_absolute_and_a_set_one_follows_the_facing() {
    let s = bare(measured());
    let witch_shift = card_stat(&s, "Witch").formation.spawn_angle_shift_deg;
    let dark_shift = card_stat(&s, "DarkWitch").formation.spawn_angle_shift_deg;
    drop(s);
    assert_eq!(witch_shift, 0, "the Witch's SpawnAngleShift is blank, which this arm reads as 0");
    assert_ne!(dark_shift, 0, "the Dark Witch sets one, which is what makes her the other law");

    // In the recordings the Witch's four Skeletons sit on 0/90/180/270 every wave, so
    // each one is on an axis through her: exactly one of its two offsets is zero. That
    // is the axis claim without a single angle being computed.
    let (at, points) = first_wave(measured(), "Witch", "Skeleton", spot(), Team::Blue);
    let mut on_axis = 0;
    for p in &points {
        let (dx, dy) = (p.x - at.x, p.y - at.y);
        if dx == 0 || dy == 0 {
            on_axis += 1;
        }
    }
    assert_eq!(
        on_axis,
        points.len(),
        "the Witch's blank-shift ring sits on the axes in the recordings; offsets were {:?}",
        points.iter().map(|p| (p.x - at.x, p.y - at.y)).collect::<Vec<_>>()
    );
}

/// Catches an arm that reads the seat rather than the frame. Rotating the whole scene
/// must rotate the emission with it.
#[test]
fn both_seats_emit_the_same_way() {
    let cfg = measured();
    let arena_w = bare(cfg.clone()).config().arena.width;
    let arena_h = bare(measured()).config().arena.height;
    let at = spot();
    let (_, blue) = first_wave(measured(), "Tombstone", "Skeleton", at, Team::Blue);

    let mut s = bare(measured());
    let red_at = Vec2::new(arena_w - at.x, arena_h - at.y);
    s.scenario_spawn_now(Team::Red, "Tombstone", red_at, None).expect("the Red spawner goes down");
    let red = loop {
        s.tick();
        let seen = find_live(&s, Team::Red, "Skeleton");
        if !seen.is_empty() {
            break seen.iter().map(|e| e.pos).collect::<Vec<_>>();
        }
    };
    let blue_along = blue[0].y - at.y;
    let red_along = red[0].y - red_at.y;
    assert_eq!(blue_along, -red_along, "the two seats emit the same distance along their OWN forward axis");
    assert_eq!(blue[0].x - at.x, red[0].x - red_at.x, "and neither drifts sideways");
}

/// The older arms stay selectable and unchanged, so a measurement that overturns this one
/// is a config change rather than a revert.
#[test]
fn the_refuted_arms_still_behave_as_they_did() {
    let (at, centred) = first_wave(with_calib(|c| c.spawner_spawn_point = SpawnPoint::AtCentre), "Tombstone", "Skeleton", spot(), Team::Blue);
    assert_eq!(centred[0], at, "at_centre still puts the unit on the spawner's own point");

    let s = bare(config());
    let tomb_r = card_stat(&s, "Tombstone").collision_radius;
    drop(s);
    let (at, ahead) = first_wave(with_calib(|c| c.spawner_spawn_point = SpawnPoint::InFrontAtOwnRadius), "Tombstone", "Skeleton", spot(), Team::Blue);
    assert_eq!(ahead[0], Vec2::new(at.x, at.y + tomb_r), "the old shipped arm still emits at the spawner's own radius");
}
