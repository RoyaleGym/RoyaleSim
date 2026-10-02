//! A HOMING SHOT STARTS PAST A NEAR TARGET -- calibration combat.LAUNCH_PAST_TARGET, combat.rs `launch_point`.
//!
//! THE READING (client 15.535.29, parity's launch census over the scenario and IL projectile tracks): a shot whose source
//! stands nearer its target than its ProjectileStartRadius at the launch tick's start appears the whole radius out
//! along the aim, past the target, and flies back: the Hero Musketeer's near shots 6 of 6 at 1800 x the aim + 300 x her
//! side's forward (sp-il-04cb t1204: 952 from Skeleton 58, the shot appears 1,070 past it and lands on t1206), the
//! Musketeer's 1 of 1 (156 off, the shot 449 out), a Blowdart Goblin's 1 of 1. The shipped arm, clamped, starts it on
//! the target's distance, so it lands a tick early when the radius is a step or more past the target.
//!
//! THE SCENE: a blue Musketeer (ProjectileStartRadius 450, homing) and a red Knight held 300 from her along y; her first
//! shot's point on its fire tick (it is born there and first steps on the next tick, combat.PROJECTILE_LAUNCH).
//!
//! PLANT (regression): launch_clamped_past_target -> `a_homing_shot_starts_its_radius_out_past_a_near_target` red.
//!   RUSTFLAGS='--cfg clash_plant="launch_clamped_past_target"' CARGO_TARGET_DIR=target/plant cargo test --profile gate
//!   --test launch_past_target
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, LaunchPastTarget};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The first shot's distance from the Musketeer on its fire tick.
fn first_shot(arm: LaunchPastTarget) -> i64 {
    let mut cfg = config();
    cfg.calib.launch_past_target = arm;
    let deck: Vec<String> = ["Musketeer", "Knight", "Archers", "Giant", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let (me, them) = ((9000, 12000), (9000, 12300));
    let musk = s.scenario_spawn_now(Team::Blue, "Musketeer", at(me), None).expect("the Musketeer");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at(them), None).expect("the Knight");
    for _ in 0..80 {
        assert!(s.debug_set_pos(musk, at(me)));
        assert!(s.debug_set_pos(knight, at(them)));
        s.tick();
        if let Some(p) = s.projectiles().iter().find(|p| p.firer == Some(musk)) {
            let (dx, dy) = ((p.pos.x / K - me.0) as i64, (p.pos.y / K - me.1) as i64);
            return isqrt(dx * dx + dy * dy);
        }
    }
    panic!("the scene drifted: no shot of hers in 80 ticks");
}

/// Plant: launch_clamped_past_target.
#[test]
fn a_homing_shot_starts_its_radius_out_past_a_near_target() {
    let d = first_shot(LaunchPastTarget::Client15535HomingUnclamped);
    assert!((448..=451).contains(&d), "client15535_homing_unclamped: the shot starts {d} out, not her 450 past the Knight at 300");
}

#[test]
fn the_shipped_arm_starts_it_no_farther_than_the_target() {
    assert_eq!(Calib::shipped().launch_past_target, LaunchPastTarget::Clamped);
    let d = first_shot(LaunchPastTarget::Clamped);
    assert!((298..=301).contains(&d), "clamped: the shot starts {d} out, not on the Knight's 300");
}
