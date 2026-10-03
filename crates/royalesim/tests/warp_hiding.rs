//! combat.WARP_HIDING: from when a warping hero (the Hero Mega Minion, card.rs `WarpDef`) is hidden (state.rs `warp_pass`,
//! `land_warps`; its strike buff is invisible, target.rs `invisible_at`).
//!
//! THE EVIDENCE (the ledger has the rows), client 15.535.29, LOW: sp-form-MegaMinion-hero-s0's Musketeer, holding the hero
//! and inside its reach and sight, let it go on the tick after the hero's first warp step.
//!
//! THE SCENE (tests/hero_mega_minion.rs's): Blue's princess towers down, a red Knight held 3000 ahead of the hero and a
//! red Skeleton held 7200 off (its pick); pressed, the hero casts, stands a tick and warps. WHAT IS PINNED:
//!   1. client15535_first_step: the hero carries the invisible bit (status_flags bit 1) from the tick of its first warp
//!      step;
//!   2. arrival (the old arm, the vacuity check): it does not on that tick;
//!   3. both: it carries it on its arrival on the Skeleton's centre;
//!   4. the shipped value is arrival (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test warp_hiding`):
//!   * `warp_hidden_on_arrival` -- the new arm still hides it on the arrival alone: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, WarpHiding};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["MegaMinion", "Knight", "Archer", "Giant", "Musketeer", "Minions", "Fireball", "Zap"];
const AT: (i32, i32) = (9000, 9000);
/// status_flags bit 1: invisible to enemies (entity.rs `Entities::status_flags`).
const INVISIBLE: i32 = 2;

/// Under `arm`: whether the hero is invisible on the tick of its first warp step, and on its arrival.
fn warp(arm: WarpHiding) -> (bool, bool) {
    let mut cfg: BattleConfig = config();
    cfg.calib.warp_hiding = arm;
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.scenario_set_elixir_milli(Team::Red, 10_000);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s.tick();
    let reds: Vec<(royalesim::EntityId, Vec2)> = [("Knight", (AT.0, AT.1 + 3000)), ("Skeleton", (AT.0 - 4320, AT.1 + 5760))]
        .iter()
        .map(|(card, p)| (s.scenario_spawn_now(Team::Red, card, n(p.0, p.1), None).expect("a red unit"), n(p.0, p.1)))
        .collect();
    s.spawn_unit(Team::Blue, "MegaMinion_hero", n(AT.0, AT.1), None).expect("the hero");
    s.tick();
    let hero = find_live(&s, Team::Blue, "MegaMinion_hero")[0].id;
    for _ in 0..40 {
        assert!(s.debug_set_pos(hero, n(AT.0, AT.1)));
        for (id, p) in &reds {
            assert!(s.debug_set_pos(*id, *p));
        }
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let pick = reds[1].0;
    let mut prev = s.entity(hero).expect("the hero").pos;
    let (mut first, mut arrival) = (None, None);
    for _ in 0..30 {
        for (id, p) in reds.iter().filter(|_| arrival.is_none()) {
            if s.entity(*id).is_some() {
                assert!(s.debug_set_pos(*id, *p));
            }
        }
        s.tick();
        let h = s.entity(hero).expect("the hero");
        let hidden = h.status_flags & INVISIBLE != 0;
        if first.is_none() && h.pos != prev {
            first = Some(hidden);
        }
        if arrival.is_none() && s.entity(pick).is_some_and(|t| t.pos == h.pos) {
            arrival = Some(hidden);
        }
        prev = h.pos;
    }
    (first.unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the hero never warped")), arrival.unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the hero never stood on its pick")))
}

#[test]
fn a_warping_hero_is_hidden_from_its_first_step_under_client15535_first_step() {
    let (first, arrival) = warp(WarpHiding::Client15535FirstStep);
    assert!(first, "new: the hero is not hidden on its first warp step");
    assert!(arrival, "new: the hero is not hidden on its arrival");
}

#[test]
fn the_old_value_hides_it_on_its_arrival_alone() {
    let (first, arrival) = warp(WarpHiding::Arrival);
    assert!(!first, "old: the hero is hidden on its first warp step already");
    assert!(arrival, "old: the hero is not hidden on its arrival");
}

#[test]
fn the_shipped_value_is_arrival() {
    assert_eq!(Calib::shipped().warp_hiding, WarpHiding::Arrival);
}
