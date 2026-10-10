//! A FIRECRACKER SPARK'S LANDING-TICK REACH (combat.rs `release_sparks`; combat.SPARK_LANDING_REACH =
//! client16402_start_extra): on the tick the rocket lands each spark hits an enemy within ProjectileRadius (400) +
//! ProjectileStartExtraRadius (650) + its radius of the landing point. Measured on client 16.402 (the live population,
//! parity's r62 item L, 455 shots): enemies 1000-1500 from the landing point lose five sparks' damage on that tick.
//!
//! The scene: a Blue Firecracker held on (9000, 14000) shooting a Red Knight held on (9000, 18000); a second Red Knight held
//! 1300 to its side, (10300, 18000), off every spark line (the fan turns 32 degrees at most from the rocket's line). Within
//! 400 + 650 + 500 of the landing point under the new arm, beyond 400 + 500 under the old.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! spark_landing_reach`): spark_landing_reach_narrow -> `a_spark_hits_beside_its_landing_point_on_the_landing_tick` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, SparkLandingReach, SpawnProjectile};
use royalesim::Team;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The side Knight's hitpoints lost over 150 ticks of the scene, under `arm`.
fn side_loss(arm: SparkLandingReach) -> i32 {
    let mut cfg = config();
    cfg.calib.spark_landing_reach = arm;
    cfg.calib.spawn_projectile = SpawnProjectile::ClientSparkFan;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let (fp, ap, bp) = (n((9000, 14000)), n((9000, 18000)), n((10300, 18000)));
    let fc = s.scenario_spawn_now(Team::Blue, "Firecracker", fp, None).expect("the Firecracker");
    let a = s.scenario_spawn_now(Team::Red, "Knight", ap, None).expect("the target");
    let b = s.scenario_spawn_now(Team::Red, "Knight", bp, None).expect("the side Knight");
    let full = s.entity(b).expect("the side Knight").max_hp;
    for _ in 0..150 {
        for (id, p) in [(fc, fp), (a, ap), (b, bp)] {
            if s.entity(id).is_some() {
                assert!(s.debug_set_pos(id, p));
            }
        }
        if let Some(e) = s.entity(a) {
            let hp = e.max_hp;
            assert!(s.debug_set_hp(a, hp));
        }
        s.tick();
    }
    full - s.entity(b).map_or(0, |e| e.hp)
}

/// Plant: spark_landing_reach_narrow.
#[test]
fn a_spark_hits_beside_its_landing_point_on_the_landing_tick() {
    assert_eq!(side_loss(SparkLandingReach::ProjectileRadius), 0, "projectile_radius: the side Knight off every spark line is never hit (vacuity)");
    assert!(side_loss(SparkLandingReach::Client16402StartExtra) > 0, "client16402_start_extra: the side Knight 1300 from the landing point is hit on the landing tick");
}
