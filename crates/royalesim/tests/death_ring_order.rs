//! WHICH OF A FACING RING'S MEMBERS IS MADE FIRST -- calibration spawner.DEATH_SPAWN_RING_ORDER, state.rs
//! `death_spawn_points`, `ring_step_on`.
//!
//! THE READING: a dying Battle Ram lays its two Barbarians on the ring at its facing, SpawnAngleShift 180 apart from it
//! and from each other: one ahead, one behind. Client 15.535.29 made the one AHEAD first on 46 of 46 Battle Ram and Evo
//! Battle Ram deaths, and the 16.402 corpus on 2 of 2; the engine's member 0 stood at the shift, behind. The order is the
//! avoidance scan's tie (sp-ram-alone-s0 t378).
//!
//! The scene: a blue Battle Ram sent up its lane until it dies (on the princess tower or under its arrows); the
//! Barbarians it leaves, by creation order (`team_seq`).
//!
//! PLANT (regression): death_ring_member_zero_first -> `a_dying_battle_ram_makes_the_barbarian_ahead_first` red.
//!   RUSTFLAGS='--cfg clash_plant="death_ring_member_zero_first"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test death_ring_order
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DeathSpawnRingOrder};
use royalesim::Team;

/// The two Barbarians a blue Battle Ram leaves, (first made, second made), their y.
fn barbarians(arm: DeathSpawnRingOrder) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.death_spawn_ring_order = arm;
    let deck: Vec<String> = ["BattleRam", "Knight", "Archers", "Giant", "Musketeer", "Fireball", "Arrows", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(5, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    s.scenario_spawn_now(Team::Blue, "BattleRam", Vec2::new(3500 * K, 12000 * K), None).expect("the Battle Ram");
    for _ in 0..900 {
        s.tick();
        let mut b: Vec<(u32, i32)> = s.entities().filter(|e| e.team == Team::Blue && s.cards().get(e.card_idx).name == "Barbarian").map(|e| (e.team_seq, e.pos.y / K)).collect();
        if b.len() == 2 {
            b.sort();
            return (b[0].1, b[1].1);
        }
    }
    panic!("the scene drifted: the Battle Ram left no pair of Barbarians");
}

/// Plant: death_ring_member_zero_first.
#[test]
fn a_dying_battle_ram_makes_the_barbarian_ahead_first() {
    let (first, second) = barbarians(DeathSpawnRingOrder::ClientOneStepOn);
    assert!(first > second, "blue's first Barbarian at y {first} is not ahead of the second at {second}");
}

#[test]
fn the_old_arm_makes_the_one_behind_first() {
    assert_eq!(Calib::shipped().death_spawn_ring_order, DeathSpawnRingOrder::ClientOneStepOn);
    let (first, second) = barbarians(DeathSpawnRingOrder::MemberZeroAtShift);
    assert!(first < second, "the old arm's first Barbarian at y {first} is not behind the second at {second}");
}
