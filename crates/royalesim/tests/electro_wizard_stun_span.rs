//! status.HIT_STUN_SPAN (state.rs `apply_effects`; combat.rs `fire`'s BuffHit::hit_span): how long a stun a unit's direct hit
//! delivers holds.
//!
//! THE LAW, client16402_full_span_from_next_tick (client 16.402, parity's r74 census of the live11 set: crown towers held by
//! an Electro Wizard's hit resumed on the zap + 11 in 24 of 29, the engine's on + 10): the stun holds BuffTime and a tick.
//!
//! THE SCENE: a Blue Electro Wizard held on (9000, 18000) zapping a Red Knight held 3,000 north (hp kept full): the ticks after
//! the first zap on which the Knight's stun timer runs.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="hit_stun_from_landing"' CARGO_TARGET_DIR=target/plant cargo test --test
//! electro_wizard_stun_span`): the new arm still holds BuffTime: the test goes red.
//!
//! THE LANDED REFLECT, client16402_full_span_landed_reflect (parity r75, live13: crown towers after an Electro Giant's reflect
//! on a landed shot resumed on the zap + 11 in 40 of 41, the engine's on + 10 in 41 of 42): a Blue Cannon held on (9000,
//! 10000) shooting a Red Electro Giant held 3300 north (tests/reflect_ranged.rs's scene); the ticks the Cannon's stun runs.
//! PLANT landed_reflect_stun_from_landing: the new arm still holds BuffTime on that path.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, HitStunSpan, ReflectRanged};
use royalesim::Team;

/// Under `arm`: the consecutive ticks, from the first on which the Knight's stun timer is set, on which it is above 0.
fn held_ticks(arm: HitStunSpan) -> u32 {
    let mut cfg = config();
    cfg.calib.hit_stun_span = arm;
    let mut s = BattleState::new(0, cfg);
    let (w_at, k_at) = (Vec2::new(9000 * K, 18000 * K), Vec2::new(9000 * K, 21000 * K));
    let wiz = s.scenario_spawn_now(Team::Blue, "ElectroWizard", w_at, None).expect("the Electro Wizard");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).expect("the Knight");
    let hold = |s: &mut BattleState| {
        let (hw, hk) = (s.entity(wiz).expect("him").max_hp, s.entity(knight).expect("the Knight").max_hp);
        assert!(s.debug_set_pos(wiz, w_at) && s.debug_set_hp(wiz, hw) && s.debug_set_pos(knight, k_at) && s.debug_set_hp(knight, hk));
    };
    let mut n = 0;
    let mut started = false;
    for _ in 0..200 {
        hold(&mut s);
        s.tick();
        let st = s.entity(knight).expect("the Knight").stun_ms;
        if st > 0 {
            started = true;
            n += 1;
        } else if started {
            return n;
        }
    }
    panic!("the scene drifted: the Knight was never stunned, or never freed");
}

/// status.HIT_STUN_SPAN = client16402_full_span_from_next_tick: the Electro Wizard's stun holds one tick longer than under
/// from_landing_tick (the vacuity check: its BuffTime's ticks). Plant: hit_stun_from_landing.
#[test]
fn under_client16402_full_span_from_next_tick_an_electro_wizard_stun_holds_a_tick_longer() {
    let old = held_ticks(HitStunSpan::FromLandingTick);
    assert!(old >= 5, "the scene drifted: the stun held {old} ticks");
    assert_eq!(held_ticks(HitStunSpan::Client16402FullSpanFromNextTick), old + 1, "client16402_full_span_from_next_tick: not one tick longer than {old}");
}

/// Under `arm`: the consecutive ticks, from the first on which the Cannon's stun timer is set, on which it is above 0 (the
/// reflect its landed shot draws from the Electro Giant).
fn landed_reflect_held_ticks(arm: HitStunSpan) -> u32 {
    let mut cfg = config();
    cfg.calib.hit_stun_span = arm;
    cfg.calib.reflect_ranged = ReflectRanged::Client16402LandingEdge;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let (cp, gp) = (Vec2::new(9000 * K, 10000 * K), Vec2::new(9000 * K, 13300 * K));
    let cannon = s.scenario_spawn_now(Team::Blue, "Cannon", cp, None).expect("the Cannon");
    let giant = s.scenario_spawn_now(Team::Red, "ElectroGiant", gp, None).expect("the Electro Giant");
    let mut n = 0;
    let mut started = false;
    for _ in 0..200 {
        let (hc, hg) = (s.entity(cannon).expect("the Cannon").max_hp, s.entity(giant).expect("the Giant").max_hp);
        assert!(s.debug_set_pos(cannon, cp) && s.debug_set_hp(cannon, hc) && s.debug_set_pos(giant, gp) && s.debug_set_hp(giant, hg));
        s.tick();
        let st = s.entity(cannon).expect("the Cannon").stun_ms;
        if st > 0 {
            started = true;
            n += 1;
        } else if started {
            return n;
        }
    }
    panic!("the scene drifted: the Cannon was never stunned, or never freed");
}

/// status.HIT_STUN_SPAN = client16402_full_span_landed_reflect: the reflect a landed shot draws holds the Cannon one tick
/// longer than under the shipped client16402_full_span_from_next_tick, whose code never reached this path (the vacuity
/// check). Plant: landed_reflect_stun_from_landing.
#[test]
fn under_client16402_full_span_landed_reflect_a_landed_shots_reflect_holds_a_tick_longer() {
    let old = landed_reflect_held_ticks(HitStunSpan::Client16402FullSpanFromNextTick);
    assert!(old >= 5, "the scene drifted: the reflect's stun held {old} ticks");
    assert_eq!(landed_reflect_held_ticks(HitStunSpan::FromLandingTick), old, "the shipped arm moved the landed reflect");
    assert_eq!(
        landed_reflect_held_ticks(HitStunSpan::Client16402FullSpanLandedReflect),
        old + 1,
        "client16402_full_span_landed_reflect: not one tick longer than {old}"
    );
}
