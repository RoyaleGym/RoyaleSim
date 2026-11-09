//! A FIRECRACKER SPARK ON A BUILDING'S SQUARE (combat.rs `straight_hits`; combat.STRAIGHT_SHOT_BUILDING_REACH =
//! client16635_sparks_too): a spark reaches a building or a crown tower by its square, half-side its radius, as the other
//! one-way straight shots do under client16402_every_straight_shot, which keeps the circle for a spark. Measured on client
//! 16.635 (parity r92, live15n): the sparks' hit ticks on a building of 11 of 11 rocket landings (7 at a princess tower, 3
//! at a King, 1 at a Cannon) read off the square, none off the circle.
//!
//! The scene: a Blue Firecracker held on (10300, 21300) shooting a Red Knight held on (12800, 23800), 700 below and 700 left of
//! the corner of Red's princess tower on (14500, 25500): 990 from its square, inside a landing spark's 400 + 650, and 2,404
//! from its centre, beyond 400 + 650 + its radius 1,000. Under the new arm the tower loses its first spark on the landing
//! tick; under client16402_every_straight_shot only once the sparks have flown at it.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! spark_building_square`): spark_building_circle -> `a_spark_meets_a_towers_square_on_its_landing_tick` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, SparkLandingReach, SpawnProjectile, StraightShotBuildingReach};
use royalesim::Team;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// (the landing tick, the tick the tower first loses hitpoints) over 150 ticks of the scene, under `arm`.
fn first_hits(arm: StraightShotBuildingReach) -> (Option<u32>, Option<u32>) {
    let mut cfg = config();
    cfg.calib.straight_shot_building_reach = arm;
    cfg.calib.spawn_projectile = SpawnProjectile::ClientSparkFan;
    cfg.calib.spark_landing_reach = SparkLandingReach::Client16402StartExtra;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let (fp, kp, tp) = (n((10300, 21300)), n((12800, 23800)), n((14500, 25500)));
    let tower = s.entities().find(|e| e.team == Team::Red && e.kind.is_crown_tower() && e.pos == tp).map(|e| e.id).expect("Red's princess tower on (14500, 25500)");
    let fc = s.scenario_spawn_now(Team::Blue, "Firecracker", fp, None).expect("the Firecracker");
    let kn = s.scenario_spawn_now(Team::Red, "Knight", kp, None).expect("the Knight");
    let (mut landing, mut first) = (None, None);
    for _ in 0..150 {
        for (id, p) in [(fc, fp), (kn, kp)] {
            if let Some(e) = s.entity(id) {
                let hp = e.max_hp;
                assert!(s.debug_set_pos(id, p));
                assert!(s.debug_set_hp(id, hp));
            }
        }
        let tower_hp = s.entity(tower).expect("the tower stands").hp;
        let knight_hp = s.entity(kn).expect("the Knight stands").hp;
        s.tick();
        if landing.is_none() && s.entity(kn).map_or(0, |e| e.hp) < knight_hp {
            landing = Some(s.tick_count());
        }
        if first.is_none() && s.entity(tower).map_or(0, |e| e.hp) < tower_hp {
            first = Some(s.tick_count());
        }
        if landing.is_some() && first.is_some() {
            break;
        }
    }
    (landing, first)
}

/// Plant: spark_building_circle.
#[test]
fn a_spark_meets_a_towers_square_on_its_landing_tick() {
    let (l0, t0) = first_hits(StraightShotBuildingReach::Client16402EveryStraightShot);
    let l0 = l0.expect("client16402_every_straight_shot: the rocket lands on the Knight");
    let t0 = t0.expect("client16402_every_straight_shot: the sparks that fly at the tower reach its circle");
    assert!(t0 > l0, "client16402_every_straight_shot: the landing point is beyond the circle, so the tower's first spark comes in flight ({t0} after {l0}; vacuity)");
    let (l1, t1) = first_hits(StraightShotBuildingReach::Client16635SparksToo);
    let l1 = l1.expect("client16635_sparks_too: the rocket lands on the Knight");
    assert_eq!(t1, Some(l1), "client16635_sparks_too: the landing point is within a landing spark's reach of the tower's square, so the tower loses its first spark on the landing tick");
}
