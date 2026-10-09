//! A LOG TAPPED ON THE WATER (item 318; spells.ILLEGAL_SPELL_TAP = client16402_clamp_to_legal_edge, state.rs
//! `clamps_spell_refusal`): under the clamp, a spell placed in troop territory and tapped on a water tile is cast from the
//! first legal tile back along its column, as one tapped on the enemy half is. A spell whose own rule refuses water (the
//! Goblin Barrel, anywhere but water) is still refused there.
//!
//! THE MEASUREMENT (client 15.535.29, Oracle's log-reloc scenes): a blue Log tapped on the water at (11156, 16000) rolls
//! from (11500, 14500), side 1's from (11500, 17500), as from a tap on the own half's boundary row (the rolling
//! projectile's first point, read directly). The engine refused the play as water, and its scenario path cast the Log on
//! the water tile, where it hit the Knight 9 and 5 ticks early.
//!
//! WHAT IS PINNED: (1) under the clamp a blue Log played at (11156, 16000) is accepted, and a red Knight walking down from
//! (9500, 19499) is first hurt on the same tick as by a Log played at (11500, 14500); laid there by the scenario path
//! (`spawn_unit`, the replay harness's), the same; under the shipped refuse the play is refused. (2) Under the clamp a
//! Goblin Barrel played there is still refused.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test spell_clamp_water`):
//!   * `spell_clamp_skips_water` -- a water tap is not clamped: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeployError, IllegalSpellTap};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const WATER: (i32, i32) = (11156, 16000);
const BOUNDARY: (i32, i32) = (11500, 14500);

/// A red Knight from (9500, 19499); 59 ticks later Blue casts `card` at `tap`, played (`deploy`) or laid by the scenario
/// path (`spawn_unit`). The cast's verdict, and the tick the Knight is first hurt.
fn cast(arm: IllegalSpellTap, card: &str, tap: (i32, i32), play: bool) -> (Result<(), DeployError>, Option<u32>) {
    let mut cfg = config();
    let deck: Vec<String> = ["Log", "GoblinBarrel", "Knight", "Arrows", "Zap", "Fireball", "Giant", "Cannon"].iter().map(|x| x.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.calib.illegal_spell_tap = arm;
    let mut s = BattleState::new(2, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let kn = s.scenario_spawn_now(Team::Red, "Knight", at((9500, 19499)), None).expect("the Knight");
    let full = s.entity(kn).expect("the Knight").max_hp;
    for _ in 0..59 {
        s.tick();
    }
    let r = if play { s.deploy(Team::Blue, card, at(tap)).map(|_| ()) } else { s.spawn_unit(Team::Blue, card, at(tap), None) };
    if r.is_err() {
        return (r, None);
    }
    for _ in 0..80 {
        s.tick();
        if s.entity(kn).is_some_and(|e| e.hp < full) {
            return (r, Some(s.tick_count()));
        }
    }
    (r, None)
}

/// (1) Plant: spell_clamp_skips_water.
#[test]
fn a_log_tapped_on_the_water_is_cast_from_the_boundary_row() {
    let clamp = IllegalSpellTap::ClampToLegalEdge;
    let (ok, edge) = cast(clamp, "Log", BOUNDARY, true);
    assert!(ok.is_ok(), "the boundary-row Log was refused: {ok:?}");
    let edge = edge.expect("the boundary-row Log never hurt the Knight");
    let (played, hit) = cast(clamp, "Log", WATER, true);
    assert!(played.is_ok(), "the water-tapped Log was refused under the clamp: {played:?}");
    assert_eq!(hit, Some(edge), "the water-tapped Log's first hit on the Knight (the boundary row's: {edge})");
    let (laid, laid_hit) = cast(clamp, "Log", WATER, false);
    assert!(laid.is_ok(), "the scenario path refused the water-tapped Log: {laid:?}");
    assert_eq!(laid_hit, Some(edge), "the scenario path's water-tapped Log's first hit (the boundary row's: {edge})");
    assert!(cast(IllegalSpellTap::Refuse, "Log", WATER, true).0.is_err(), "the shipped refuse accepted a Log on the water");
}

/// (2)
#[test]
fn a_goblin_barrel_on_the_water_is_still_refused_under_the_clamp() {
    let (r, _) = cast(IllegalSpellTap::ClampToLegalEdge, "GoblinBarrel", WATER, true);
    assert_eq!(r, Err(DeployError::Water), "a Goblin Barrel tapped on the water");
}
