//! hide.SHOT_AT_HIDING_BUILDING: whether a shot fired at a hiding building (the Tesla) while it was up lands on it once it
//! has gone under (combat.rs `step_projectiles`, `shot_passes_hide`).
//!
//! THE READING (client 15.535.29, 2 of 2): Oracle's sp-tesla-hide-10/-12/-60, a Bomber's bomb fired at the Tesla on
//! t166, the Tesla under on t177 after killing the Bomber, the bomb's 225 on it on t179; ub-gh7-tesla-7382, a Spear
//! Goblin's spear landing on the Tesla's under-going tick. The engine dropped both (hide.HIDDEN_IMMUNE_TO_DAMAGE).
//!
//! The scene: a blue Tesla, a red Bomber walking in; on the tick its first bomb at the Tesla is in flight the Bomber is
//! put at 0 hitpoints, so the Tesla's target is gone and it goes under while the bomb flies.
//!
//! PLANT (regression): hiding_shot_dropped -> `a_bomb_in_flight_lands_on_the_tesla_gone_under` red.
//!   RUSTFLAGS='--cfg clash_plant="hiding_shot_dropped"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!   shot_at_hiding_building
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::HideState;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, ShotAtHidingBuilding};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// (the Tesla's hp loss on the tick the bomb landed, whether it was under then).
fn scene(arm: ShotAtHidingBuilding) -> (i32, bool) {
    let mut cfg = config();
    cfg.calib.shot_at_hiding_building = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.spawn_unit(Team::Blue, "Tesla", n(14000, 14000), None).expect("the Tesla");
    for _ in 0..30 {
        s.tick();
    }
    let tesla: EntityId = find_live(&s, Team::Blue, "Tesla").first().expect("the Tesla").id;
    let bomber = s.scenario_spawn_now(Team::Red, "Bomber", n(14500, 19500), None).expect("the Bomber");
    let mut fired = false;
    for _ in 0..200 {
        s.tick();
        if s.projectiles().iter().any(|p| p.team == Team::Red && p.target == tesla) {
            fired = true;
            break;
        }
    }
    assert!(fired, "the scene drifted: the Bomber never fired at the Tesla");
    assert!(s.debug_set_hp(bomber, 0));
    for _ in 0..60 {
        let before = s.entity(tesla).expect("the Tesla").hp;
        let flying = s.projectiles().iter().any(|p| p.target == tesla);
        s.tick();
        if flying && !s.projectiles().iter().any(|p| p.target == tesla) {
            let t = s.entity(tesla).expect("the Tesla");
            return (before - t.hp, t.hide_state == HideState::Hidden);
        }
    }
    panic!("the scene drifted: the bomb never landed");
}

/// Plant: hiding_shot_dropped.
#[test]
fn a_bomb_in_flight_lands_on_the_tesla_gone_under() {
    let (loss, under) = scene(ShotAtHidingBuilding::Client15535Lands);
    assert!(under, "the scene drifted: the Tesla was not under when the bomb landed");
    assert!(loss > 100, "client15535_lands: the Tesla lost {loss} on the bomb's tick");
    let (old, old_under) = scene(ShotAtHidingBuilding::Dropped);
    assert!(old_under, "the scene drifted under the old arm");
    assert!(old < 10, "dropped: the hidden Tesla lost {old}, more than its drain");
}

#[test]
fn the_shipped_arm_drops_it() {
    assert_eq!(Calib::shipped().shot_at_hiding_building, ShotAtHidingBuilding::Dropped);
}
