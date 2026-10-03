//! combat.SPIN_BEGIN: the first tick a Hero Valkyrie's spin may begin (state.rs `spin_seek`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, sp-form-Valkyrie-hero-s0: on the press's first frame P she
//! walks her own 60, from P + 1 150 a tick, her blows on P + 1 + 5k; the replay's engine spun from P, a tick early.
//!
//! THE SCENE: hero_valkyrie.rs's chain scene, a red Ice Golem 3000 ahead of her, held; she is held until the press; her
//! steps on the press's first two frames.
//!
//! WHAT IS PINNED, and the plant that turns it red (spin_begins_on_press):
//!   1. client15535_next_tick: the first frame's step is her own walk (well under 150), the second's 150; press_tick (the
//!      engine's, the vacuity check): 150 on the first.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, SpinBegin};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Her steps (native units) on the first two frames after the press.
fn steps(arm: SpinBegin) -> [i32; 2] {
    let mut cfg: BattleConfig = config();
    cfg.calib.spin_begin = arm;
    let deck: Vec<String> = ["Valkyrie", "Knight", "Archer", "Giant", "Musketeer", "HogRider", "Fireball", "Zap"].iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let golem = s.scenario_spawn_now(Team::Red, "IceGolemite", n(9000, 14000), None).expect("a red Ice Golem");
    s.spawn_unit(Team::Blue, "Valkyrie_hero", n(9000, 11000), None).expect("the hero");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Valkyrie_hero")[0].id;
    for _ in 0..40 {
        assert!(s.debug_set_pos(hero, n(9000, 11000)) && s.debug_set_pos(golem, n(9000, 14000)));
        s.tick();
    }
    assert!(s.debug_set_pos(hero, n(9000, 11000)));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut out = [0; 2];
    for o in out.iter_mut() {
        let at = s.entity(hero).expect("the hero").pos;
        assert!(s.debug_set_pos(golem, n(9000, 14000)));
        s.tick();
        *o = s.entity(hero).expect("the hero").pos.dist(at) / K;
    }
    out
}

/// Plant: spin_begins_on_press.
#[test]
fn a_spin_begins_the_tick_after_its_press_under_client15535_next_tick() {
    let now = steps(SpinBegin::PressTick);
    // NOT VACUOUS: the engine's arm spins on the press's first frame.
    assert!((148..=151).contains(&now[0]), "press_tick: 150 on the first frame: {now:?}");
    let next = steps(SpinBegin::Client15535NextTick);
    assert!(next[0] < 100, "client15535_next_tick: her own walk on the first frame, not the spin's 150: {next:?}");
    assert!((148..=151).contains(&next[1]), "client15535_next_tick: 150 from the second frame: {next:?}");
}
