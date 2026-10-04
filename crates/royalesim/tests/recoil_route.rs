//! knockback.RECOIL_ROUTE: what an Evo Battle Ram's recoil (knockback.ATTACK_PUSHBACK) does to the route it charged on
//! (state.rs: the launch after `attack_recoil`; `phase_path16402_for`'s pushback tick).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, every Evo Battle Ram impact in the records (16 in 11
//! scenes): the route read empty on the impact frame and a fresh one, from the recoil's first point to the same goal
//! cell, on the next frame, 16 of 16; sp-ram-w-Knight-3500-s0 t993-t1007 walked that fresh route's next cell where the
//! engine walked its kept one.
//!
//! THE SCENE (tests/evo_battle_ram.rs's): the evolved ram put down at (3500, 12500) charges a red Elixir Collector at
//! (3500, 20500), hits it, lives and recoils (AttackPushBack 2000). WHAT IS PINNED:
//!   1. client15535_replanned: on the hit's frame the ram's route is a fresh plan from the recoil's first point, longer
//!      than the one it held the frame before (the ground the recoil took back is in it);
//!   2. kept (the old arm, the vacuity check): the route on the hit's frame is the one it held the frame before;
//!   3. the shipped value is kept (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test recoil_route`):
//!   * `recoil_route_kept` -- the new arm's launch keeps the route, so nothing replans: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, RecoilRoute};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The ram's route on the frame before its first hit on the Collector and on the hit's frame, under `arm`.
fn routes_round_the_hit(arm: RecoilRoute) -> (Vec<Vec2>, Vec<Vec2>) {
    let mut cfg: BattleConfig = config();
    cfg.calib.recoil_route = arm;
    cfg.decks = [vec!["BattleRam".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    s.scenario_spawn_now(Team::Red, "Elixir Collector", n(3500, 20500), None).expect("the Collector");
    s.spawn_unit(Team::Blue, "BattleRam_EV1", n(3500, 12500), None).expect("the form");
    s.tick();
    let ram = find_live(&s, Team::Blue, "BattleRam_EV1")[0].id;
    let col = find_live(&s, Team::Red, "Elixir Collector")[0].id;
    let mut before: Vec<Vec2> = Vec::new();
    let mut hp = s.entity(col).expect("the Collector").hp;
    for _ in 0..220 {
        let route: Vec<Vec2> = s.entity(ram).expect("the ram").route.to_vec();
        s.tick();
        let now = s.entity(col).map_or(0, |e| e.hp);
        if hp - now > 100 {
            let after: Vec<Vec2> = s.entity(ram).expect("the ram lives through its hit").route.to_vec();
            before = route;
            return (before, after);
        }
        hp = now;
    }
    panic!("{arm:?}: the scene drifted: the ram never hit the Collector ({} cells held)", before.len());
}

/// Plant: recoil_route_kept.
#[test]
fn the_rams_recoil_plans_a_fresh_route_under_client15535_replanned() {
    let (before, after) = routes_round_the_hit(RecoilRoute::Client15535Replanned);
    assert!(!before.is_empty(), "the scene drifted: the ram held no route before its hit");
    assert!(after.len() > before.len(), "new: the route on the hit's frame is not a fresh plan from the recoil's point ({} cells, {} before)", after.len(), before.len());
}

#[test]
fn the_old_value_keeps_the_route_through_the_recoil() {
    let (before, after) = routes_round_the_hit(RecoilRoute::Kept);
    assert!(!before.is_empty(), "the scene drifted: the ram held no route before its hit");
    assert_eq!(after, before, "old: the route changed on the hit's frame (vacuous otherwise)");
}

#[test]
fn the_shipped_value_is_kept() {
    assert_eq!(Calib::shipped().recoil_route, RecoilRoute::Kept);
}
