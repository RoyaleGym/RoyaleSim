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
//!   3. the ring's angle is two laws, discriminated HERE by whether the ring moves when
//!      the spawner's facing moves: a blank SpawnAngleShift does not follow the facing,
//!      a set one does. Which fixed frame the blank case uses is NOT settled by this
//!      file and no test here claims it: the Witch's ring is 4-fold, so it is invariant
//!      under 90 degrees and cannot separate an absolute frame from the OWNER's forward
//!      frame. That is open in the ledger's promotion_rules, point (3), and needs a
//!      spawner whose ring count does not divide 360 into multiples of 90;
//!   4. the two seats mirror;
//!   5. the older arms still behave as they did, because they remain selectable.
//!
//! The 0-means-blank reading of SpawnAngleShift is a claim about the DATA, so it is
//! checked here rather than assumed.

mod common;

use common::*;
use royalesim::fixed::Vec2;
use royalesim::state::{BattleConfig, BattleState, Calib, SpawnPoint, SpawnedFirstStep};
use royalesim::Team;

/// The shipped config with spawner.SPAWNED_FIRST_STEP = none, for every scene in this file. It shadows
/// `common::config`. These scenes read where a spawner's unit, or a death spawn's, is CREATED, and under
/// the shipped client16402_same_tick the unit has already taken its first step (or entered its attack)
/// on its first frame. Under none it stands on the point it was created at. The first step is its own
/// key, pinned in tests/spawned_first_step.rs.
fn config() -> BattleConfig {
    let mut cfg = common::config();
    cfg.calib.spawned_first_step = SpawnedFirstStep::None;
    cfg
}

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

/// One wave of `card`, with an enemy put down at `bait` so the spawner turns to face it.
///
/// Returns the spawner's FACING at the moment it emits and the wave's offsets FROM the
/// spawner, both read on the emission tick. The offsets are what the two angle laws differ
/// about, and the facing is how a test shows the scene really turned her: two scenes whose
/// facings match cannot say anything about a law that reads the facing, and would pass
/// while comparing a scene with itself.
fn wave_facing_bait(card: &str, unit: &str, bait: Vec2) -> (Vec2, Vec<(i32, i32)>) {
    let mut s = bare(measured());
    let id = s.scenario_spawn_now(Team::Blue, card, spot(), None).expect("the spawner goes down");
    s.scenario_spawn_now(Team::Red, "Knight", bait, None).expect("the bait goes down");
    for _ in 0..400 {
        s.tick();
        let seen = find_live(&s, Team::Blue, unit);
        if !seen.is_empty() {
            let e = s.entity(id).expect("the spawner is alive when it emits");
            let mut off: Vec<(i32, i32)> =
                seen.iter().map(|u| (u.pos.x - e.pos.x, u.pos.y - e.pos.y)).collect();
            off.sort_unstable();
            return (e.facing, off);
        }
    }
    panic!("{card} never emitted a {unit} with the bait at {bait:?}");
}

/// One scene: where the spawner stood facing, and every point it emitted at.
type FacingAndPoints = (Vec2, Vec<(i32, i32)>);

/// Two scenes that differ only in which side of the spawner the enemy is on.
fn left_and_right(card: &str, unit: &str) -> (FacingAndPoints, FacingAndPoints) {
    (wave_facing_bait(card, unit, t(200, 900)), wave_facing_bait(card, unit, t(1600, 900)))
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
    let forward = -royalesim::arena::Arena::own_side_dy(Team::Blue);
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

/// The blank half of the angle law: the ring does not follow the spawner's facing.
///
/// This test used to be named for both halves and ran only this one. The other half --
/// that a card SETTING a shift does follow its facing -- is now its own test below, with
/// its own scene, because a name covering two claims while exercising one is the same
/// defect as a suite reporting a property it never creates the state for.
#[test]
fn a_blank_angle_shift_does_not_follow_the_spawners_facing() {
    let s = bare(measured());
    let witch_shift = card_stat(&s, "Witch").formation.spawn_angle_shift_deg;
    let dark_shift = card_stat(&s, "DarkWitch").formation.spawn_angle_shift_deg;
    drop(s);
    assert_eq!(witch_shift, 0, "the Witch's SpawnAngleShift is blank, which this arm reads as 0");
    assert_ne!(dark_shift, 0, "the Dark Witch sets one, which is what makes her the other law");

    // In the recordings the Witch's four Skeletons sit on 0/90/180/270 every wave, so
    // each one is on an axis through her: exactly one of its two offsets is zero. That
    // is the axis claim without a single angle being computed.
    // THE CONTROL, and it is the same pair of scenes the set-shift test below uses: turning
    // her must NOT move a blank-shift ring. Without this the test only said the ring sits on
    // the axes, which a facing-relative law would also satisfy whenever she happens to face
    // along one.
    let ((f_left, left), (f_right, right)) = left_and_right("Witch", "Skeleton");
    assert_ne!(
        f_left, f_right,
        "the two scenes did not turn her, so this control compares a scene with itself"
    );
    assert_eq!(
        left, right,
        "her ring moved when her facing did, so a blank SpawnAngleShift is following the facing"
    );

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

/// The SET half of the angle law, which the test above used to be named for and never ran.
///
/// A card that sets SpawnAngleShift lays its ring out relative to its own facing, so a scene
/// that turns the spawner must turn the ring with it. The two scenes differ only in which
/// side the enemy stands on, so the ring moving is the facing law and nothing else.
///
/// WHAT THIS DOES NOT CLAIM. It does not check the ring lands at any particular angle. The
/// Dark Witch emits two Bats and a 2-fold ring is invariant under 180 degrees, so an angle
/// assertion here would be weaker than it looked. Moving WITH the facing is the property
/// that separates this law from the blank one, and it is the property asserted.
#[test]
fn a_set_angle_shift_follows_the_spawners_facing() {
    let s = bare(measured());
    let shift = card_stat(&s, "DarkWitch").formation.spawn_angle_shift_deg;
    drop(s);
    assert_ne!(shift, 0, "the Dark Witch must SET a shift or this test is about the other law");

    let ((f_left, left), (f_right, right)) = left_and_right("DarkWitch", "Bat");
    assert_ne!(
        f_left, f_right,
        "the two scenes did not turn her, so they compare a scene with itself: facing {f_left:?}"
    );
    assert_ne!(
        left, right,
        "her ring did not move when her facing did, so a set SpawnAngleShift is not following          the facing: {left:?} against {right:?}"
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
