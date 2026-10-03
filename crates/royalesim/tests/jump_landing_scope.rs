//! movement.JUMP_LANDING_SCOPE, read off the engine: which contact passes of its landing tick L a river jump's lander
//! stays out of under movement.JUMP_LANDING_CONTACT = client_next_tick (state.rs `phase_path16402_for`, entity.rs
//! `landed_at`). move_pass, the engine's: the move pass alone, so a unit taking its first update after the pass on L (a
//! death spawn, an Evo Skeleton's copy) meets the lander as any body. client15535_whole_tick: every contact pass of L.
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, sp-il-925e t3365, an Evo Skeleton's copy made beside a Hog
//! Rider landing on that tick was pushed as if the Hog were not there.
//!
//! THE SCENE. jump_landing_contact's: a blue Hog Rider on the river centre (9500, 12500) leaps and lands; a probe run
//! reads its landing tick L and point P. The measured run puts a blue Elixir Golem down far behind, and on L - 1 moves it
//! 700 beyond P with 0 hitpoints, so it dies in L's Reap and its two halves (ElixirGolem2, no DeathSpawnPushback slide)
//! take their first update after L's move pass (spawner.SPAWNED_FIRST_STEP: a walk step through the contact law), each
//! within the lander's contact. The same run without the Hog is the reference. (A Golem's Golemites slide out instead,
//! spawner.DEATH_SPAWN_PUSHBACK, and meet no body in that slide's first tick: no scene for this key.)
//!
//! WHAT IS PINNED, and the plant that turns it red (landing_scope_move_pass_only):
//!   1. `a_lander_is_no_body_for_its_landing_ticks_first_updates_under_whole_tick`: under client15535_whole_tick the
//!      Elixir Golem's halves stand on L where they stand with no Hog; under move_pass the lander pushes one (the scene
//!      overlaps).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, JumpLandingScope};
use royalesim::Team;

const HOG_AT: (i32, i32) = (9500, 12500);
const GOLEM_AT: (i32, i32) = (3500, 9500);

fn with_arm(arm: JumpLandingScope) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.jump_landing_scope = arm;
    cfg
}

/// The tick count at which the Hog's leap ends, with its landing point (jump_landing_contact's probe).
fn probe(arm: JumpLandingScope) -> (u32, Vec2) {
    let mut s = BattleState::new(3, with_arm(arm));
    let hog = s.scenario_spawn_now(Team::Blue, "HogRider", Vec2::new(HOG_AT.0 * K, HOG_AT.1 * K), None).expect("the Hog");
    let mut leapt = false;
    for n in 1..300u32 {
        s.tick();
        let e = s.entity(hog).expect("the Hog is gone");
        if e.jumping {
            leapt = true;
        } else if leapt {
            return (n, e.pos);
        }
    }
    panic!("the Hog never leapt and landed from the river centre (leapt: {leapt})");
}

/// The Elixir Golem's halves' points on L, the Elixir Golem dying there 700 beyond the landing point, with or without the
/// Hog.
fn halves_on_landing_tick(arm: JumpLandingScope, with_hog: bool) -> Vec<Vec2> {
    let (landing, at) = probe(arm);
    let mut s = BattleState::new(3, with_arm(arm));
    let hog = if with_hog {
        Some(s.scenario_spawn_now(Team::Blue, "HogRider", Vec2::new(HOG_AT.0 * K, HOG_AT.1 * K), None).expect("the Hog"))
    } else {
        None
    };
    let golem = s.scenario_spawn_now(Team::Blue, "ElixirGolem", Vec2::new(GOLEM_AT.0 * K, GOLEM_AT.1 * K), None).expect("the Elixir Golem");
    for _ in 1..landing {
        s.tick();
    }
    if let Some(h) = hog {
        assert!(s.entity(h).is_some_and(|e| e.jumping), "the Hog is not in the air on L - 1");
    }
    assert!(s.debug_set_pos(golem, Vec2::new(at.x, at.y + 700 * K)), "the Elixir Golem is gone before L");
    assert!(s.debug_set_hp(golem, 0));
    s.tick();
    if let Some(h) = hog {
        let e = s.entity(h).expect("the Hog on L");
        assert!(!e.jumping, "the Elixir Golem moved the Hog's landing: it is still in the air on L");
        assert_eq!(e.pos, at, "the Elixir Golem moved the Hog's landing point");
    }
    assert!(s.entity(golem).is_none(), "the Elixir Golem did not die on L");
    let mut out: Vec<(u32, Vec2)> = s.entities().filter(|e| e.team == Team::Blue && e.card == "ElixirGolem2").map(|e| (e.id.index, e.pos)).collect();
    out.sort_by_key(|(i, _)| *i);
    assert_eq!(out.len(), 2, "the Elixir Golem's two halves on L: {out:?}");
    out.into_iter().map(|(_, p)| p).collect()
}

#[test]
fn a_lander_is_no_body_for_its_landing_ticks_first_updates_under_whole_tick() {
    let alone = halves_on_landing_tick(JumpLandingScope::Client15535WholeTick, false);
    let beside = halves_on_landing_tick(JumpLandingScope::Client15535WholeTick, true);
    assert_eq!(beside, alone, "client15535_whole_tick: the lander moved a half on its landing tick");
    // NOT VACUOUS: under the engine's arm the lander pushes one, so the scene overlaps.
    let old_alone = halves_on_landing_tick(JumpLandingScope::MovePass, false);
    let old_beside = halves_on_landing_tick(JumpLandingScope::MovePass, true);
    assert_eq!(old_alone, alone, "the arms differ with no lander");
    assert_ne!(old_beside, old_alone, "move_pass: the lander pushes no half; the scene does not overlap");
}
