//! A DIRECT SHOT OF A ROW THAT IS NOT HOMING -- calibration combat.DIRECT_NON_HOMING_SHOT, combat.rs `fire`,
//! `direct_shot_fixed`, `launch_point`.
//!
//! THE READING (client 16.635, parity r101, the oracle's il-mg-16635 set): every shot of the walking Furnace and of its
//! evolution (Speed 600, ProjectileStartRadius 2000, no splash, not Homing) is born 2000 out along the line to its target's
//! start-of-tick point, past a nearer target too (1385 of 1385), keeps that point as its aim, and lands on the tick its
//! 600 steps from there reach it, on its target wherever the target then stands (1384 of 1385 landing ticks; 1074 of 1075
//! live targets hit). The old arm, follows_target, starts the shot no farther than its target and re-aims it every tick,
//! so it lands a tick early on a near target and on one walking at the Furnace.
//!
//! THE SCENES: a blue Furnace and a red Giant held in place, the Giant 1100 from it along y (near, inside the radius) or
//! 4000 along x (far); in the far scene the Giant is moved 100 toward the Furnace on every tick after the fire.
//!
//! PLANTS:
//!   * `direct_shot_follows` -- the new arm's direct shot is still born no farther than its target and follows it:
//!     `under_client16635_fixed_past_target_a_near_target_is_shot_from_past_it` and
//!     `under_client16635_fixed_past_target_the_shot_keeps_its_aim_on_a_walking_target` go red.
//!   RUSTFLAGS='--cfg clash_plant="direct_shot_follows"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test direct_non_homing_shot
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DirectNonHomingShot};
use royalesim::Team;

const FURNACE: (i32, i32) = (9000, 12000);
const NEAR: (i32, i32) = (9000, 13100);
const FAR: (i32, i32) = (13000, 12000);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Furnace's first shot: its distance from the Furnace on its fire tick, its aim then and one tick later, whether it
/// keeps its aim, and its flight (the ticks from the fire to the first tick it is gone).
struct Shot {
    born_at: i64,
    aim_fire: Vec2,
    aim_later: Vec2,
    fixed: bool,
    flight: u32,
}

/// A blue Furnace at FURNACE and a red Giant at `giant`, both held there until the Furnace fires; after the fire the Giant
/// is moved `walk` toward the Furnace (along -x) on every tick.
fn shoot(arm: DirectNonHomingShot, giant: (i32, i32), walk: i32) -> Shot {
    let mut cfg = config();
    cfg.calib.direct_non_homing_shot = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    let furnace = s.scenario_spawn_now(Team::Blue, "FirespiritHut", at(FURNACE), None).expect("the Furnace");
    let g = s.scenario_spawn_now(Team::Red, "Giant", at(giant), None).expect("the Giant");
    let mine = |s: &BattleState| s.projectiles().iter().find(|p| p.firer == Some(furnace)).cloned();
    for _ in 0..200 {
        assert!(s.debug_set_pos(furnace, at(FURNACE)));
        assert!(s.debug_set_pos(g, at(giant)));
        s.tick();
        let Some(p) = mine(&s) else { continue };
        let (dx, dy) = ((p.pos.x / K - FURNACE.0) as i64, (p.pos.y / K - FURNACE.1) as i64);
        let (born_at, aim_fire, fixed) = (isqrt(dx * dx + dy * dy), p.aim, p.fixed);
        let mut aim_later = p.aim;
        let mut here = giant;
        for k in 1..20u32 {
            here.0 -= walk;
            assert!(s.debug_set_pos(furnace, at(FURNACE)));
            assert!(s.debug_set_pos(g, at(here)));
            s.tick();
            let Some(q) = mine(&s) else {
                return Shot { born_at, aim_fire, aim_later, fixed, flight: k };
            };
            if k == 1 {
                aim_later = q.aim;
            }
        }
        panic!("the Furnace's shot never landed");
    }
    panic!("the scene drifted: no Furnace shot in 200 ticks");
}

/// Plant: direct_shot_follows.
#[test]
fn under_client16635_fixed_past_target_a_near_target_is_shot_from_past_it() {
    let new = shoot(DirectNonHomingShot::Client16635FixedPastTarget, NEAR, 0);
    assert!((1998..=2001).contains(&new.born_at), "client16635_fixed_past_target: the shot is born {} out, not its 2000 past the Giant at 1100", new.born_at);
    assert_eq!(new.flight, 2, "client16635_fixed_past_target: 900 back to the Giant is two 600 steps (the client's F + 2)");
    // Vacuity: the old arm starts it on the Giant and lands it on the next tick.
    let old = shoot(DirectNonHomingShot::FollowsTarget, NEAR, 0);
    assert!((1098..=1101).contains(&old.born_at), "follows_target: the shot is born {} out, not on the Giant at 1100", old.born_at);
    assert_eq!(old.flight, 1, "follows_target: born on the Giant, the shot lands on the next tick");
}

/// Plant: direct_shot_follows.
#[test]
fn under_client16635_fixed_past_target_the_shot_keeps_its_aim_on_a_walking_target() {
    let new = shoot(DirectNonHomingShot::Client16635FixedPastTarget, FAR, 100);
    assert!((1998..=2001).contains(&new.born_at), "the shot is born {} out, not its 2000", new.born_at);
    assert!(new.fixed, "client16635_fixed_past_target: the Furnace's shot keeps its aim");
    assert_eq!(new.aim_fire, at(FAR), "the aim is the Giant's point at the start of the fire tick");
    assert_eq!(new.aim_later, new.aim_fire, "client16635_fixed_past_target: the aim stays put while the Giant walks");
    assert_eq!(new.flight, 4, "client16635_fixed_past_target: 2000 to the fire tick's point is four 600 steps");
    // Vacuity: the old arm re-aims it at the walking Giant, which shortens the flight.
    let old = shoot(DirectNonHomingShot::FollowsTarget, FAR, 100);
    assert!(!old.fixed, "follows_target: the shot follows its target");
    assert_ne!(old.aim_later, old.aim_fire, "follows_target: the aim moved with the Giant");
    assert!(old.flight < new.flight, "follows_target: the shot at a Giant walking at the Furnace lands earlier ({} against {})", old.flight, new.flight);
}

#[test]
fn the_shipped_arm_is_follows_target() {
    assert_eq!(Calib::shipped().direct_non_homing_shot, DirectNonHomingShot::FollowsTarget);
}
