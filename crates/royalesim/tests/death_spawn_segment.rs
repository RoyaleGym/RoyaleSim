//! spawner.DEATH_SPAWN_SEGMENT: where the first segment of a sliding death-spawn member's birth route is frozen (state.rs
//! `phase_path16402`, its slide branch, the `at_birth` plan of spawner.DEATH_SPAWN_ROUTE = client_at_birth).
//!
//! THE EVIDENCE (the ledger has the rows): both clients' frames carry the birth route's segment from the death point toward
//! its next node on the birth frame and through the slide, and the walk's reached test on the first walking tick pops a
//! node the member slid across (client 15.535.29: 25 of 25 Golemites whose goal cell held).
//!
//! THE SCENE: tests/death_spawn_route.rs's: Blue's Golem put down on (12500, 22500), on Red's side with Red's towers
//! standing, and set to 0 hitpoints; its two Golemites slide out. WHAT IS PINNED:
//!   1. client_birth_frozen: on each Golemite's slide ticks after its first it holds a frozen segment, the same for the
//!      pair (both planned from the death point toward one route's next node), unchanged through the slide;
//!   2. refrozen (the old arm, the vacuity check): no Golemite holds a segment while it slides;
//!   3. the shipped value is refrozen (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="death_spawn_segment_refrozen"' CARGO_TARGET_DIR=target/plant cargo test --test
//! death_spawn_segment`):
//!   * `death_spawn_segment_refrozen` -- the new arm still leaves the segment to the walk: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DeathSpawnRoute, DeathSpawnSegment};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Each Golemite's segments on its slide ticks after its first (the death frame is inert) under `arm`.
fn slide_segments(arm: DeathSpawnSegment) -> Vec<Vec<Vec2>> {
    let mut cfg = config();
    cfg.calib.death_spawn_route = DeathSpawnRoute::ClientAtBirth;
    cfg.calib.death_spawn_segment = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    let golem = s.scenario_spawn_now(Team::Blue, "Golem", n(12500, 22500), None).expect("the Golem");
    s.tick();
    assert!(s.debug_set_hp(golem, 0));
    let mut segs: std::collections::BTreeMap<u32, Vec<Vec2>> = Default::default();
    let mut slid = std::collections::BTreeSet::new();
    for _ in 0..30 {
        s.tick();
        for e in s.entities().filter(|e| e.card == "Golemite" && e.team == Team::Blue) {
            let k = e.id.index;
            if e.death_slide_radius > 0 {
                if slid.insert(k) {
                    continue;
                }
                segs.entry(k).or_default().push(e.seg_dir);
            }
        }
    }
    assert_eq!(segs.len(), 2, "{arm:?}: the scene drifted: {} Golemites slid", segs.len());
    segs.into_values().collect()
}

/// Plant: death_spawn_segment_refrozen.
#[test]
fn a_golemite_keeps_its_birth_segment_through_the_slide_under_client_birth_frozen() {
    let g = slide_segments(DeathSpawnSegment::ClientBirthFrozen);
    for (k, s) in g.iter().enumerate() {
        assert!(!s.is_empty() && s[0] != Vec2::default(), "new: Golemite {k} slid with no frozen segment: {s:?}");
        assert!(s.iter().all(|v| *v == s[0]), "new: Golemite {k}'s segment moved in the slide: {s:?}");
    }
    assert_eq!(g[0][0], g[1][0], "new: the pair's segments differ (both are frozen from the death point)");
}

#[test]
fn the_old_value_freezes_nothing_while_sliding() {
    assert!(
        slide_segments(DeathSpawnSegment::Refrozen).iter().all(|s| s.iter().all(|v| *v == Vec2::default())),
        "old: a Golemite held a segment while sliding (vacuous otherwise)"
    );
}

#[test]
fn the_shipped_value_is_refrozen() {
    assert_eq!(Calib::shipped().death_spawn_segment, DeathSpawnSegment::Refrozen);
}
