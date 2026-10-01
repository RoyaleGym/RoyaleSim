//! A HELD WALKER KEEPS ITS ROUTE (state.rs `phase_path16402_for`, the held unit's speed-0 update: "its route waits out
//! the hold as it is"). Measured on client 15.535.29 over every walking unit that stood still five ticks or more with a
//! route (a stun or a freeze): the route it walked on after the hold was the one it held before, 119 of 121 units that
//! target buildings and 93 of 109 that target troops (the others took a new target). sp-il-db5f t262: a Hog Rider
//! zapped on its walk to the princess tower keeps its four cells through the 10-tick stun and walks them on.
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
fn a_zapped_hog_rider_keeps_its_route_through_the_stun() {
    let mut cfg = config();
    let deck: Vec<String> = ["HogRider", "Knight", "Archers", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    // On red's half, the princess tower 7,500 ahead and in its sight: it walks a route to the tower.
    let hog = s.scenario_spawn_now(Team::Blue, "HogRider", at(14500, 18000), None).expect("the Hog Rider");
    let mut before = Vec::new();
    for _ in 0..60 {
        s.tick();
        let h = s.entity(hog).expect("the Hog Rider");
        if !h.deploying && !h.route.is_empty() && h.pos.y > 18500 * K {
            before = h.route.clone();
            break;
        }
    }
    assert!(!before.is_empty(), "the scene drifted: the Hog Rider never walked a route");
    let p = s.entity(hog).unwrap().pos;
    s.spawn_unit(Team::Red, "Zap", p, None).expect("the Zap");
    let (mut held, mut log) = (0, Vec::new());
    let mut last = before;
    let mut kept = Vec::new();
    for _ in 0..20 {
        let was = s.entity(hog).expect("the Hog Rider").pos;
        s.tick();
        let h = s.entity(hog).expect("the Hog Rider lives");
        log.push((h.pos == was, h.route.len()));
        if h.pos == was {
            if held == 0 {
                kept = last.clone(); // the route on the last tick it walked
            }
            held += 1;
            assert_eq!(h.route, kept, "held tick {held}: the route the Hog Rider held on its last walking tick ({log:?})");
        } else if held > 0 {
            break;
        }
        last = h.route.clone();
    }
    assert!(held >= 5, "the scene drifted: the Zap held the Hog Rider {held} ticks ({log:?})");
}
