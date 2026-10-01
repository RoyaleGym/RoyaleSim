//! THE DEATH BLOW'S TICK (calibration combat.DEATH_DAMAGE_TICK; state.rs `phase_reap`, `land_death_blow`).
//!
//! Measured (parity's death_damage_tick reading): client 15.535.29, an enemy within a dying Golem's or Ice Golemite's
//! death radius loses the death damage on the death frame, 122 of 122 (Golem 30, Ice Golemite 92); the 16.402 corpus on
//! the frame after, 9 of 10. The ledger ships next_tick (16.402's); replay_parity runs a 15.535.29 capture at
//! client15535_death_tick.
//!
//! The scene: a red Golem at its last hitpoint beside a blue Knight on blue's half, out of every tower's reach; the
//! Golem is set to 0 before a tick, so it dies on that tick.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! death_damage_tick`): death_blow_next_tick -> `the_15535_arm_lands_the_blow_on_the_death_tick` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeathDamageTick};
use royalesim::{EntityId, Team};

const DECK: [&str; 8] = ["Knight", "Golem", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Knight's hitpoints at the end of the Golem's death tick and of the tick after, under `arm`; and the blow's size.
fn knight_after_death(arm: DeathDamageTick) -> (i32, i32, i32) {
    let mut cfg = config();
    cfg.decks = [DECK.iter().map(|c| c.to_string()).collect(), DECK.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.death_damage_tick = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let golem: EntityId = s.scenario_spawn_now(Team::Red, "Golem", at(9000, 13500), None).expect("the Golem");
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", at(9000, 12000), None).expect("the Knight");
    // The Knight at full hitpoints when the Golem falls: no blow of the Golem's has landed (both still deploying).
    let full = s.entity(knight).expect("the Knight").max_hp;
    assert!(s.debug_set_hp(knight, full));
    assert!(s.debug_set_hp(golem, 0));
    s.tick();
    assert!(s.entity(golem).is_none(), "the scene drifted: the Golem did not die on the tick");
    let on = s.entity(knight).expect("the Knight").hp;
    s.tick();
    let after = s.entity(knight).expect("the Knight").hp;
    (full - on, full - after, full)
}

#[test]
fn the_15535_arm_lands_the_blow_on_the_death_tick() {
    let (on, after, _) = knight_after_death(DeathDamageTick::Client15535DeathTick);
    assert!(on > 0, "client 15.535.29's arm: the Knight lost nothing on the Golem's death tick");
    assert_eq!(after, on, "the blow lands once");
}

#[test]
fn the_shipped_arm_lands_it_on_the_tick_after() {
    let (on, after, _) = knight_after_death(DeathDamageTick::NextTick);
    assert_eq!(on, 0, "next_tick (16.402's): the Knight lost {on} on the Golem's death tick");
    let (seq_on, _, _) = knight_after_death(DeathDamageTick::Client15535DeathTick);
    assert_eq!(after, seq_on, "the same blow, a tick later");
}
