//! spawner.RELEASE_WATER_POINT: where three kinds of ground release on river water stand (state.rs `releases_keep_water`:
//! `spawner_pass`'s to-location wave, `death_spawn_points`, `phase_reap`'s buff death spawn).
//!
//! THE LAW, measured on client 16.635 (parity r96, the oracle's il-mg-16635 set): 11 of 11 ground units first seen on
//! river water before their battle's first divergence, in three paths, stood on the water and walked off it: 8 Furnace
//! spirits put SpawnToLocation ahead, 2 Battle Ram death Barbarians, 1 Goblin Curse goblin. The engine put each on the
//! nearest bank.
//!
//! WHAT IS PINNED: a Blue Battle Ram killed on its bank beside the left bridge's mouth (5000, 14600), heading up and left
//! onto the bridge: its ring's two Barbarians are laid ahead of and behind it on its heading, the one ahead over the water
//! right of the bridge. Under client16635_kept_on_water one stands on the water on its first frame; under
//! nearest_passable_ground (the vacuity check) both stand on land.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! release_water_point`):
//!   * `release_water_to_bank` -- the new arm still puts the releases on land: the test goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, ReleaseWaterPoint};
use royalesim::Team;

/// The Barbarians a Blue Battle Ram killed at (5000, 14600) under `arm` leaves, on their first frame: how many stand on water.
fn wet_barbarians(arm: ReleaseWaterPoint) -> usize {
    let mut cfg = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.release_water_point = arm;
    let mut s = BattleState::new(0, cfg);
    let at = Vec2::new(5000 * K, 14600 * K);
    assert!(s.arena().is_passable_ground(at), "the scene drifted: the kill point is not on the bank");
    let ram = s.scenario_spawn_now(Team::Blue, "BattleRam", at, None).expect("the Battle Ram");
    s.tick();
    assert!(s.debug_set_pos(ram, at) && s.debug_set_hp(ram, 0));
    for _ in 0..5 {
        s.tick();
        let barbs: Vec<Vec2> = s.entities().filter(|e| e.team == Team::Blue && e.card == "Barbarian").map(|e| e.pos).collect();
        if !barbs.is_empty() {
            assert_eq!(barbs.len(), 2, "the scene drifted: {} Barbarians", barbs.len());
            let arena = s.arena();
            return barbs.iter().filter(|p| !arena.is_passable_ground(**p)).count();
        }
    }
    panic!("the scene drifted: the Ram left no Barbarians");
}

/// spawner.RELEASE_WATER_POINT = client16635_kept_on_water (client 16.635, parity r96, the oracle's il-mg-16635 set: 11 of 11,
/// 2 of them Battle Ram Barbarians): the Barbarian laid ahead stands on the water; under nearest_passable_ground (the vacuity
/// check) on land. Plant: release_water_to_bank.
#[test]
fn under_client16635_kept_on_water_a_battle_rams_barbarian_stands_on_the_river() {
    assert_eq!(wet_barbarians(ReleaseWaterPoint::NearestPassableGround), 0, "nearest_passable_ground (the vacuity check): a Barbarian on water");
    assert_eq!(wet_barbarians(ReleaseWaterPoint::Client16635KeptOnWater), 1, "client16635_kept_on_water: the Barbarians on water");
}

/// Since parity r96 for clients 16.402 and 16.635 (nearest_passable_ground before it; client 15.535.29 keeps
/// nearest_passable_ground).
#[test]
fn the_shipped_arm_is_client16635_kept_on_water() {
    assert_eq!(
        royalesim::state::Calib::shipped().release_water_point,
        royalesim::state::ReleaseWaterPoint::Client16635KeptOnWater
    );
    assert_eq!(config().calib.release_water_point, royalesim::state::ReleaseWaterPoint::Client16635KeptOnWater);
}
