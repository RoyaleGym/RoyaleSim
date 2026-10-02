//! THE CHASE LIMIT HOLDS A TARGET PAST ROUND SIGHT (targeting.CHASE_DROP_RANGE = client_sight_minus_1000; target.rs
//! `decide`, `held_past_sight`). Measured on client 15.535.29: a walker holding a target that stands past its round sight
//! (centre distance > SightRange + both radii) but inside the chase-drop limit (max |dx|, |dy| <= SightRange + both radii
//! - 1000) kept it 158 of 159 times for a troop target and 25 of 25 for a building; the engine's rescan, which finds
//! nothing past round sight, lost it (sp-champ-SkeletonKing-s0 t177).
//!
//! The scene: a blue Knight held on (5000, 10000) takes a red Knight held 3,000 off on the diagonal; the red Knight is
//! then put 4,800 off on both axes, 6,788 away (round sight 5,500 + 500 + 500 = 6,500; the limit 5,500 on the square).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! chase_hold`): chase_lost_past_round_sight -> `a_target_past_round_sight_inside_the_chase_limit_is_kept` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::Team;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

#[test]
fn a_target_past_round_sight_inside_the_chase_limit_is_kept() {
    let mut cfg = config();
    let deck: Vec<String> = ["Knight", "Archers", "Giant", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let (me, near, far) = ((5000, 10000), (7100, 12100), (9800, 14800));
    let blue = s.scenario_spawn_now(Team::Blue, "Knight", at(me.0, me.1), None).expect("the blue Knight");
    let red = s.scenario_spawn_now(Team::Red, "Knight", at(near.0, near.1), None).expect("the red Knight");
    let mut took = false;
    for _ in 0..60 {
        assert!(s.debug_set_pos(blue, at(me.0, me.1)));
        assert!(s.debug_set_pos(red, at(near.0, near.1)));
        s.tick();
        if s.entity(blue).expect("the blue Knight").target == Some(red) {
            took = true;
            break;
        }
    }
    assert!(took, "the scene drifted: the blue Knight never took the red Knight");
    for k in 0..5 {
        assert!(s.debug_set_pos(blue, at(me.0, me.1)));
        assert!(s.debug_set_pos(red, at(far.0, far.1)));
        s.tick();
        assert_eq!(s.entity(blue).expect("the blue Knight").target, Some(red), "tick {k} past round sight, inside the limit: the blue Knight let the red one go");
    }
}
