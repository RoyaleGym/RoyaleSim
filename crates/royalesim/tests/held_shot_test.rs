//! combat.HELD_SHOT_TEST: whether a Hunter's pellet held at its launch point by its row's RandomDelay tests for hits
//! while it stands (combat.rs `step_straight`, the `hold` branch).
//!
//! THE LAW, measured on client 15.535.29 (the ledger has the rows): it tests nothing while it stands (647 of 647 pellets;
//! newborn Golemites within reach of standing pellets were not hit).
//!
//! THE SCENE: Blue's Hunter at (9000, 10000) and a red Giant at (9000, 13000) it shoots; a red Knight kept far away.
//! When the Hunter's volley is made, the Knight is put 400 behind a standing pellet (away from the pellet's aim: within
//! its reach of 300 + the Knight's radius where it stands, past it once a step of 550 has moved it on), and one tick
//! runs. WHAT IS PINNED:
//!   1. client15535_untested: the Knight loses nothing that tick;
//!   2. tested (the old arm, the vacuity check): the Knight loses hitpoints that tick;
//!   3. the shipped value is tested (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="held_shot_tested"' CARGO_TARGET_DIR=target/plant cargo test --test
//! held_shot_test`):
//!   * `held_shot_tested` -- the new arm still tests the standing pellet: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, HeldShotTest};
use royalesim::Team;

const BLUE: [&str; 8] = ["Hunter", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];
const RED: [&str; 8] = ["Giant", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"];

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Knight's hitpoints lost on the tick after the volley under `arm`, put 400 behind a standing pellet.
fn knight_loss(arm: HeldShotTest) -> i32 {
    let mut cfg = config();
    cfg.decks = [BLUE.iter().map(|c| c.to_string()).collect(), RED.iter().map(|c| c.to_string()).collect()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.held_shot_test = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let hunter = s.scenario_spawn_now(Team::Blue, "Hunter", n(9000, 10000), None).expect("the Hunter");
    s.scenario_spawn_now(Team::Red, "Giant", n(9000, 13000), None).expect("the Giant");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(16000, 30000), None).expect("the Knight");
    for _ in 0..200 {
        s.tick();
        let standing = s.projectiles().iter().find(|p| p.firer == Some(hunter) && p.straight.as_ref().is_some_and(|st| st.hold > 0)).map(|p| (p.pos, p.aim));
        if let Some((pos, aim)) = standing {
            let (dx, dy) = (i64::from(aim.x - pos.x), i64::from(aim.y - pos.y));
            let len = isqrt(dx * dx + dy * dy).max(1);
            let back = Vec2::new(pos.x - (dx * 400 * i64::from(K) / len) as i32, pos.y - (dy * 400 * i64::from(K) / len) as i32);
            assert!(s.debug_set_pos(knight, back));
            let before = s.entity(knight).expect("the Knight").hp;
            s.tick();
            return before - s.entity(knight).expect("the Knight").hp;
        }
    }
    panic!("{arm:?}: the scene drifted: the Hunter made no held pellet in 200 ticks");
}

/// Plant: held_shot_tested.
#[test]
fn a_standing_pellet_hits_nothing_under_client15535_untested() {
    assert_eq!(knight_loss(HeldShotTest::Client15535Untested), 0, "new: a standing pellet hit the Knight behind it");
}

#[test]
fn the_old_value_hits_from_where_it_stands() {
    assert!(knight_loss(HeldShotTest::Tested) > 0, "old: the standing pellet missed the Knight behind it (vacuous otherwise)");
}

#[test]
fn the_shipped_value_is_tested() {
    assert_eq!(Calib::shipped().held_shot_test, HeldShotTest::Tested);
}
