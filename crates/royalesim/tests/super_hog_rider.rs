//! THE SUPER HOG RIDER EVENT CARD (item 301; card.rs `bottle_body` with no body, `boost_shape`, `SpellShape::Boost`;
//! spell.rs the pickup's step; an area's Invisible-alone buff through `apply_invisible`): every 4000 ms from its
//! activation + 3000 it puts down a present (SantaPresent: a 1000 ms bottle with no body) whose pickup area, once an own
//! troop reaches it, puts down a 500 ms bottle whose area makes its own troops invisible for 3000 ms.
//!
//! THE MEASUREMENT (client 15.535.29, sp-event-SuperHogRider-s0, level 11): the red tower and Knight let the Hog go on
//! t343 and the tower took it back 60 ticks later, four times, 80 ticks apart: 37 ticks after each present's emission
//! (activation + 60 + 80k).
//!
//! WHAT IS PINNED: a lone Blue Super Hog Rider held 1500 in front of the red right princess tower: the tower lets it go on
//! its deploy end + 96 (its activation + 97 = the present's emission + 37) and every 80 ticks after, and takes it back 60
//! ticks after each.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test super_hog_rider`):
//!   * `bottle_without_body_refused` -- the present is refused again: red;
//!   * `boost_never_picked` -- no troop picks a present up: never let go, red;
//!   * `area_invisible_refused` -- the invisibility area's buff is refused: red;
//!   * `boost_unread` -- the pickup area goes the ordinary way and is refused: red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const HOG: (i32, i32) = (14500, 22500);

/// Plants: bottle_without_body_refused, boost_never_picked, area_invisible_refused, boost_unread.
#[test]
fn the_tower_lets_the_hog_go_37_ticks_after_each_present_for_60() {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(15, cfg);
    past_deploy_lockout(&mut s);
    let hog = s.scenario_spawn_now(Team::Blue, "SuperHogRider", at(HOG), None).expect("the Super Hog Rider loads and stands");
    let tower: EntityId = s
        .entities()
        .find(|e| e.team == Team::Red && e.kind == royalesim::entity::EntityKind::PrincessTower && e.pos.x > at((9000, 0)).x)
        .expect("the red right princess tower")
        .id;
    let mut ready: Option<u32> = None;
    let mut held_prev = false;
    let (mut drops, mut takes): (Vec<u32>, Vec<u32>) = (Vec::new(), Vec::new());
    for _ in 0..330 {
        assert!(s.debug_set_pos(hog, at(HOG)));
        let full = s.entity(hog).expect("the Hog").max_hp;
        assert!(s.debug_set_hp(hog, full));
        s.tick();
        let t = s.tick_count() - 1;
        if ready.is_none() && !s.entity(hog).expect("the Hog").deploying {
            ready = Some(t);
        }
        let held = s.entity(tower).is_some_and(|e| e.target == Some(hog));
        match (held_prev, held) {
            (true, false) => drops.push(t),
            (false, true) if !drops.is_empty() => takes.push(t),
            _ => {}
        }
        held_prev = held;
    }
    let r = ready.expect("its deploy ended");
    let d: Vec<u32> = drops.iter().map(|t| t - r).collect();
    assert_eq!(&d[..3], [96, 176, 256], "let go 37 ticks after each present (deploy end + 59 + 80k): {d:?}");
    let w: Vec<u32> = drops.iter().zip(takes.iter()).map(|(a, b)| b - a).collect();
    assert!(w.len() >= 3 && w.iter().all(|x| *x == 60), "taken back 60 ticks after each (InvisibilityTemp 3000): {w:?}");
}
