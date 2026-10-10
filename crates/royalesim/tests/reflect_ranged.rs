//! THE ELECTRO GIANT'S REFLECT ON A SHOT (state.rs `reflect_landed_shot`; combat.REFLECT_RANGED = client16402_landing_edge):
//! a homing shot that lands on him is answered when its firer's edge is within ReflectedAttackRadius (2000) of his edge.
//! Measured on client 16.402 (the live population, parity's r62 item M: 136 truth reflect events).
//!
//! The scene: a Blue Cannon held on (9000, 10000) shooting a Red Electro Giant held 3300 north, (9000, 13300): the edge gap
//! 3300 - 600 - 750 = 1950, inside the new reach and outside the old one's 2000 + 600 of his centre. The Cannon's hitpoints
//! after 120 ticks, under both arms (its decay the same in both).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! reflect_ranged`): reflect_ranged_unread -> `a_cannon_shooting_an_electro_giant_is_answered_on_its_landing` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, ReflectRanged};
use royalesim::Team;

fn n(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn cannon_hp(arm: ReflectRanged) -> i32 {
    let mut cfg = config();
    cfg.calib.reflect_ranged = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let (cp, gp) = (n((9000, 10000)), n((9000, 13300)));
    let cannon = s.scenario_spawn_now(Team::Blue, "Cannon", cp, None).expect("the Cannon");
    let giant = s.scenario_spawn_now(Team::Red, "ElectroGiant", gp, None).expect("the Electro Giant");
    for _ in 0..120 {
        if s.entity(giant).is_some() {
            let full = s.entity(giant).expect("the Giant").max_hp;
            assert!(s.debug_set_pos(giant, gp));
            assert!(s.debug_set_hp(giant, full));
        }
        if s.entity(cannon).is_some() {
            assert!(s.debug_set_pos(cannon, cp));
        }
        s.tick();
    }
    s.entity(cannon).map_or(0, |e| e.hp)
}

/// Plant: reflect_ranged_unread.
#[test]
fn a_cannon_shooting_an_electro_giant_is_answered_on_its_landing() {
    let old = cannon_hp(ReflectRanged::MeleeOnly);
    let new = cannon_hp(ReflectRanged::Client16402LandingEdge);
    assert!(new < old, "client16402_landing_edge: the Cannon takes the reflect on its shots' landings ({new} against {old})");
}
