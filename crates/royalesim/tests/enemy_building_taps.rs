//! A TROOP TAPPED ON AN ENEMY BUILDING'S BOX (item 324; placement.ENEMY_BUILDING_TAPS, state.rs
//! `relocate_off_own_crown_tower` and `building_box`): under client15535_half_tile_box_ring a troop tap whose one-tile box
//! shares positive area with an enemy building's placement box is moved off it by the ring search alone (no axis push), and
//! every building's box is anchored at its point rounded down to the half-tile grid.
//!
//! THE MEASUREMENT (client 15.535.29, sp-il-2c29; 2 of 2 taps on an enemy box, both moved): side 0's Minions tapped on (4500,
//! 14500) by side 1's Hero Musketeer turret on (3898, 13725) stood on (5500, 14500) (the box x 2000..5000; a centred one,
//! 2398..5398, would have sent them to 6500); its Evo Barbarians tapped on (3500, 14500) by the turret on (4269, 16457), on
//! the left bridge, were born round (3500, 13500), where the axis push puts the tile on 14000.
//!
//! THE SCENE: a red Cannon (CollisionRadius 600, a 3-tile box like the turret's) laid on the turret's point, and a blue
//! Knight laid by the scenario path (`spawn_unit`, a play's resolution) on the tap; its creation point a tick later.
//!
//! WHAT IS PINNED: (1) the Cannon on (3898, 13725), the Knight tapped on (4500, 14500): on (5500, 14500) under the new arm,
//! as tapped under the shipped ignored; (2) the Cannon on (4269, 16457), the Knight tapped on (3500, 14500): on (3500,
//! 13500) under the new arm, as tapped under ignored.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test enemy_building_taps`):
//!   * `enemy_building_taps_unread` -- an enemy box moves nothing: (1) and (2) go red;
//!   * `enemy_box_centred` -- every box centred on its point: (1) goes red (the Knight on 6500);
//!   * `enemy_box_axis_push` -- an enemy box takes the axis push: (2) goes red (the Knight on 14000).
//!   * `tile_box_half_tile_anchor` -- client15535_tile_box_ring anchors at the half-tile: (3) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, EnemyBuildingTaps};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The blue Knight's creation point (native) when tapped on `tap` with a red Cannon on `cannon`.
fn knight_at(arm: EnemyBuildingTaps, cannon: (i32, i32), tap: (i32, i32)) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.placement_enemy_building_taps = arm;
    let mut s = BattleState::new(0, cfg);
    let c = s.scenario_spawn_now(Team::Red, "Cannon", at(cannon), None).expect("the red Cannon");
    let cp = s.entity(c).expect("the red Cannon").pos;
    assert_eq!((cp.x / K, cp.y / K), cannon, "the scene drifted: the Cannon was not laid on its point");
    s.spawn_unit(Team::Blue, "Knight", at(tap), None).expect("the blue Knight");
    s.tick();
    let k = s.entities().find(|e| e.team == Team::Blue && e.card == "Knight").expect("the blue Knight stands");
    (k.pos.x / K, k.pos.y / K)
}

fn near(p: (i32, i32), q: (i32, i32)) -> bool {
    (p.0 - q.0).abs() <= 50 && (p.1 - q.1).abs() <= 50
}

/// Within 300 of `q` (a unit laid on a building's body is pushed off it on its first update).
fn about(p: (i32, i32), q: (i32, i32)) -> bool {
    (p.0 - q.0).abs() <= 300 && (p.1 - q.1).abs() <= 300
}

/// (1) Plants: enemy_building_taps_unread, enemy_box_centred.
#[test]
fn a_tap_on_an_off_grid_enemy_box_lands_on_the_first_clear_tile() {
    let (cannon, tap) = ((3898, 13725), (4500, 14500));
    let new = knight_at(EnemyBuildingTaps::Client15535HalfTileBoxRing, cannon, tap);
    assert!(near(new, (5500, 14500)), "client15535_half_tile_box_ring: the Knight stood on {new:?}, not (5500, 14500)");
    let old = knight_at(EnemyBuildingTaps::Ignored, cannon, tap);
    assert!(about(old, tap), "ignored: the Knight stood on {old:?}, not where tapped");
}

/// (2) Plants: enemy_building_taps_unread, enemy_box_axis_push.
#[test]
fn a_tap_on_an_enemy_box_takes_the_ring_search_not_the_axis_push() {
    let (cannon, tap) = ((4269, 16457), (3500, 14500));
    let new = knight_at(EnemyBuildingTaps::Client15535HalfTileBoxRing, cannon, tap);
    assert!(near(new, (3500, 13500)), "client15535_half_tile_box_ring: the Knight stood on {new:?}, not (3500, 13500)");
    let old = knight_at(EnemyBuildingTaps::Ignored, cannon, tap);
    assert!(about(old, tap), "ignored: the Knight stood on {old:?}, not where tapped");
}

#[test]
fn the_shipped_arm_reads_own_boxes_alone() {
    assert_eq!(Calib::shipped().placement_enemy_building_taps, EnemyBuildingTaps::Ignored);
}

/// (3) placement.ENEMY_BUILDING_TAPS = client15535_tile_box_ring (client 15.535.29, Oracle's turret battery: 72 of 72 taps):
/// a red Cannon on (3302, 14725) takes x 1000..4000 (its low corner floor1000(3302 - 1500)), so a Knight tapped on (4500, 14500)
/// stands where tapped; the half-tile anchor (3000) takes 1500..4500 and moves him off. sp-il-2c29's two taps (1) and (2) land
/// as under client15535_half_tile_box_ring. Plant: tile_box_half_tile_anchor.
#[test]
fn under_client15535_tile_box_ring_a_box_takes_its_tiles_from_the_grid() {
    let (cannon, tap) = ((3302, 14725), (4500, 14500));
    let new = knight_at(EnemyBuildingTaps::Client15535TileBoxRing, cannon, tap);
    assert!(about(new, tap), "client15535_tile_box_ring: the Knight stood on {new:?}, not where tapped");
    let old = knight_at(EnemyBuildingTaps::Client15535HalfTileBoxRing, cannon, tap);
    assert!(!about(old, tap), "client15535_half_tile_box_ring (the vacuity check): the Knight stood where tapped too ({old:?})");
    assert!(near(knight_at(EnemyBuildingTaps::Client15535TileBoxRing, (3898, 13725), (4500, 14500)), (5500, 14500)), "(1) under the new arm");
    assert!(near(knight_at(EnemyBuildingTaps::Client15535TileBoxRing, (4269, 16457), (3500, 14500)), (3500, 13500)), "(2) under the new arm");
}
