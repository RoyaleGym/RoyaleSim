//! combat.EVO_CHAIN_HOP_FIRST_STEP: on which tick an Evo Electro Dragon's hop takes its first step (combat.rs
//! `step_projectiles`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, sp-f4-ed3-s0 t1163: the shot on a Knight and its first hop on
//! an Ice Golem 1,201 off (a step is 2,000) landed on the same tick, where the engine's hop landed a tick later.
//!
//! THE SCENE (tests/evo_chain_hop_reach.rs's): the evolved dragon held at (14650, 15000), a red Knight held 3,499 above
//! him and a second 1,200 west of it, both held at full hitpoints. WHAT IS PINNED (the ticks each Knight is first hit):
//!   1. client15535_creation_tick: the second Knight is hit on the tick the first is;
//!   2. next_tick (the old arm, the vacuity check): a tick later;
//!   3. the shipped value is next_tick (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test evo_chain_hop_first_step`):
//!   * `evo_hop_steps_next_tick` -- the new arm's hops still first step next tick: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, EvoChainHopFirstStep};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`: (the tick the first Knight is first hit, the tick the second is), counted from the dragon's put-down.
fn first_hits(arm: EvoChainHopFirstStep) -> (u32, u32) {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["ElectroDragon".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.evo_chain_hop_first_step = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let towers: Vec<_> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    let at = n(14650, 15000);
    s.spawn_unit(Team::Blue, "ElectroDragon_EV1", at, None).expect("the Electro Dragon");
    s.tick();
    let ed = find_live(&s, Team::Blue, "ElectroDragon_EV1").first().expect("the Electro Dragon").id;
    let (kp, k2p) = (n(14650, 18499), n(14650 - 1200, 18499));
    let k1 = s.scenario_spawn_now(Team::Red, "Knight", kp, None).expect("the first red Knight");
    let k2 = s.scenario_spawn_now(Team::Red, "Knight", k2p, None).expect("the second red Knight");
    let (mut h1, mut h2) = (None, None);
    for k in 0..260u32 {
        assert!(s.debug_set_pos(ed, at) && s.debug_set_pos(k1, kp) && s.debug_set_pos(k2, k2p));
        let (t1, t2) = (s.entity(k1).expect("k1").max_hp, s.entity(k2).expect("k2").max_hp);
        assert!(s.debug_set_hp(k1, t1) && s.debug_set_hp(k2, t2));
        s.tick();
        if h1.is_none() && s.entity(k1).expect("k1").hp < t1 {
            h1 = Some(k);
        }
        if h2.is_none() && s.entity(k2).expect("k2").hp < t2 {
            h2 = Some(k);
        }
        if let (Some(a), Some(b)) = (h1, h2) {
            return (a, b);
        }
    }
    panic!("{arm:?}: the scene drifted: hits {h1:?} / {h2:?} within 260 ticks");
}

/// Plant: evo_hop_steps_next_tick.
#[test]
fn a_first_hop_within_a_step_lands_on_its_shots_tick_under_client15535_creation_tick() {
    let (a, b) = first_hits(EvoChainHopFirstStep::Client15535CreationTick);
    assert_eq!(b, a, "new: the hop landed {} ticks after the shot", i64::from(b) - i64::from(a));
}

#[test]
fn the_old_value_lands_it_a_tick_later() {
    let (a, b) = first_hits(EvoChainHopFirstStep::NextTick);
    assert_eq!(b, a + 1, "old: the hop landed {} ticks after the shot (vacuous otherwise)", i64::from(b) - i64::from(a));
}

#[test]
fn the_shipped_value_is_next_tick() {
    assert_eq!(Calib::shipped().evo_chain_hop_first_step, EvoChainHopFirstStep::NextTick);
}
