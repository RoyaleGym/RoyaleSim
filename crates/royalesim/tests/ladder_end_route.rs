//! A KNOCKBACK LADDER'S END AND THE ROUTE (calibration knockback.LADDER_END_ROUTE; state.rs `phase_path16402_for`, the
//! pushback tick). Measured on client 15.535.29 over every knockback ladder in its records: the route a unit held before
//! the ladder was the route it walked after it 135 times of 154 (11 took a new target, 8 attacked). The engine dropped it
//! the tick the ladder ended and replanned (client16402_dropped, shipped; the 16.402 corpus's one ladder with a route
//! does not separate the arms).
//!
//! The scene: a blue Knight walking its route on red's half toward the princess tower; a red Fireball knocks it back.
//! On the tick its ladder ends its route is the one it held when the ladder began under client15535_kept, and empty
//! under client16402_dropped.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! ladder_end_route`): ladder_end_route_dropped -> `a_knockback_ladder_leaves_the_route_under_the_15535_arm` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, LadderEndRoute};
use royalesim::Team;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Fireball scene under `arm`: (the route the Knight held when its ladder began, its route on the tick it ended).
fn ladder(arm: LadderEndRoute) -> (Vec<Vec2>, Vec<Vec2>) {
    let mut cfg = config();
    let deck: Vec<String> = ["Knight", "Fireball", "Archers", "Musketeer", "Giant", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.ladder_end_route = arm;
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let knight = s.scenario_spawn_now(Team::Blue, "Knight", at(14500, 17500), None).expect("the Knight");
    for _ in 0..80 {
        s.tick();
        let k = s.entity(knight).expect("the Knight");
        if !k.deploying && !k.route.is_empty() && k.pos.y > 18000 * K {
            break;
        }
    }
    let p = s.entity(knight).expect("the Knight").pos;
    s.spawn_unit(Team::Red, "Fireball", Vec2::new(p.x, p.y + 1000 * K), None).expect("the Fireball");
    let mut held = s.entity(knight).unwrap().route.to_vec();
    let mut began: Option<Vec<Vec2>> = None;
    for _ in 0..80 {
        s.tick();
        let k = s.entity(knight).expect("the Knight lives");
        match (&began, k.push_active) {
            (None, true) => began = Some(held.clone()),
            (Some(b), false) => return (b.clone(), k.route.to_vec()),
            _ => {}
        }
        held = k.route.to_vec();
    }
    panic!("the scene drifted: the Knight's ladder never ran its course (began: {})", began.is_some());
}

#[test]
fn a_knockback_ladder_leaves_the_route_under_the_15535_arm() {
    let (before, after) = ladder(LadderEndRoute::Client15535Kept);
    assert!(!before.is_empty(), "the scene drifted: the Knight held no route when the ladder began");
    assert_eq!(after, before, "client15535_kept: the route on the tick the ladder ended");
    let (before, after) = ladder(LadderEndRoute::Client16402Dropped);
    assert!(!before.is_empty(), "the scene drifted under client16402_dropped");
    assert!(after.is_empty(), "client16402_dropped (shipped): the route is dropped the tick the ladder ends ({} nodes)", after.len());
}
