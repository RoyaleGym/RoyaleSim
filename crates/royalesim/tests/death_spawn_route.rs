//! spawner.DEATH_SPAWN_ROUTE: when a sliding death-spawn member (a DeathSpawnPushback row's: the Golem's Golemites)
//! plans its first route (state.rs `phase_path16402`, its slide branch).
//!
//! THE EVIDENCE (the ledger has the rows): both clients, a Golem's Golemites carry one route from their first frame, the
//! same for the pair (16.402: 6 of 6; 15.535.29: 50 of 50), and walk it from where the slide leaves them.
//!
//! THE SCENE: Blue's Golem put down on (12500, 22500), on Red's side with Red's towers standing, and set to 0 hitpoints;
//! its two Golemites slide out over the next ticks. WHAT IS PINNED:
//!   1. client_at_birth: on each Golemite's first slide step (the tick after the death frame, where the members are
//!      inert) it holds a route, the pair's routes are one list (planned from the death point), and each still holds it
//!      (or what is left of it) on the tick its slide ends;
//!   2. slide_end (the old arm, the vacuity check): no Golemite holds a route while it slides;
//!   3. the shipped value is client_at_birth, on both clients (Sim's ruling, 2026-10-04).
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="death_spawn_route_at_slide_end"' CARGO_TARGET_DIR=target/plant cargo test --test
//! death_spawn_route`):
//!   * `death_spawn_route_at_slide_end` -- the new arm still plans when the slide ends: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DeathSpawnRoute};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// One Golemite's slide under `arm`: (its route on its first slide tick, its route on the tick the slide ends, whether it
/// held a route on any slide tick).
type Slide = (Vec<Vec2>, Vec<Vec2>, bool);

fn golemites(arm: DeathSpawnRoute) -> Vec<Slide> {
    let mut cfg = config();
    cfg.calib.death_spawn_route = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    let golem = s.scenario_spawn_now(Team::Blue, "Golem", n(12500, 22500), None).expect("the Golem");
    s.tick();
    assert!(s.debug_set_hp(golem, 0));
    let mut first: std::collections::BTreeMap<u32, Vec<Vec2>> = Default::default();
    let mut last: std::collections::BTreeMap<u32, Vec<Vec2>> = Default::default();
    let mut held: std::collections::BTreeMap<u32, bool> = Default::default();
    let mut slid = std::collections::BTreeSet::new();
    for _ in 0..30 {
        s.tick();
        for e in s.entities().filter(|e| e.card == "Golemite" && e.team == Team::Blue) {
            let k = e.id.index;
            if e.death_slide_radius > 0 {
                // The death frame (the member's first) is inert: its first slide step is the next tick's.
                if slid.insert(k) {
                    continue;
                }
                first.entry(k).or_insert_with(|| e.route.to_vec());
                *held.entry(k).or_insert(false) |= !e.route.is_empty();
                last.insert(k, e.route.to_vec());
            }
        }
    }
    assert_eq!(slid.len(), 2, "{arm:?}: the scene drifted: {} Golemites slid", slid.len());
    slid.iter().map(|k| (first.get(k).cloned().unwrap_or_default(), last.get(k).cloned().unwrap_or_default(), held.get(k).copied().unwrap_or(false))).collect()
}

/// Plant: death_spawn_route_at_slide_end.
#[test]
fn a_sliding_golemite_plans_where_it_is_born_under_client_at_birth() {
    let g = golemites(DeathSpawnRoute::ClientAtBirth);
    for (k, (first, last, _)) in g.iter().enumerate() {
        assert!(!first.is_empty(), "new: Golemite {k} held no route on its first slide tick");
        assert!(first.ends_with(last) && !last.is_empty(), "new: Golemite {k} lost its route in the slide: {first:?} -> {last:?}");
    }
    assert_eq!(g[0].0, g[1].0, "new: the pair's routes differ");
}

#[test]
fn the_old_value_plans_nothing_while_sliding() {
    assert!(golemites(DeathSpawnRoute::SlideEnd).iter().all(|g| !g.2), "old: a Golemite held a route while sliding (vacuous otherwise)");
}

#[test]
fn the_shipped_value_is_client_at_birth() {
    assert_eq!(Calib::shipped().death_spawn_route, DeathSpawnRoute::ClientAtBirth);
}
