//! combat.EVO_CHAIN_SHOT_LAUNCH: the tick an Evo Electro Dragon's shot leaves on (state.rs `evo_after_fire`, the chain's
//! `ChainHop::wait`).
//!
//! THE LAW, measured on client 15.535.29 (parity's ed_launch_census.py): the base Electro Dragon's shot is first seen on
//! his fire frame (34 of 34), the evolved one's after it (21 of 21): it leaves a tick later, and lands a tick later.
//!
//! The scene (tests/evo_chain_hop_wait.rs's): Blue's evolved Electro Dragon held on (14650, 15000), a red Knight held
//! 3499 north of him, topped up. WHAT IS PINNED:
//!   1. both values fire on the same tick (the attack itself is untouched);
//!   2. client15535_next_tick: the Knight's first hit comes one tick after fire_tick's;
//!   3. the shipped value is fire_tick (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test evo_chain_shot_launch`):
//!   * `evo_shot_on_fire_tick` -- the new arm's shot still leaves on the fire tick: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, EvoChainShotLaunch};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`: the tick the dragon first fires (his load timer rises) and the tick the Knight is first hit.
fn first_shot(arm: EvoChainShotLaunch) -> (u32, u32) {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["ElectroDragon".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.evo_chain_shot_launch = arm;
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
    let kp = n(14650, 18499);
    let knight = s.scenario_spawn_now(Team::Red, "Knight", kp, None).expect("a red Knight");
    let (mut fired, mut hit) = (None, None);
    let mut load = s.entity(ed).expect("the dragon").attack_load_ms;
    for _ in 0..200 {
        assert!(s.debug_set_pos(ed, at) && s.debug_set_pos(knight, kp));
        let top = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_hp(knight, top));
        s.tick();
        let now = s.entity(ed).expect("the dragon").attack_load_ms;
        if fired.is_none() && now > load {
            fired = Some(s.tick_count());
        }
        load = now;
        if hit.is_none() && s.entity(knight).expect("the Knight").hp < top {
            hit = Some(s.tick_count());
            break;
        }
    }
    (fired.unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the dragon never fired")), hit.unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the Knight was never hit")))
}

#[test]
fn an_evo_electro_dragons_shot_lands_a_tick_later_under_client15535_next_tick() {
    let (old_fire, old_hit) = first_shot(EvoChainShotLaunch::FireTick);
    let (new_fire, new_hit) = first_shot(EvoChainShotLaunch::Client15535NextTick);
    assert_eq!(new_fire, old_fire, "the dragon's fire moved: {old_fire} -> {new_fire}");
    assert_eq!(new_hit, old_hit + 1, "new: the Knight's first hit on {new_hit}, fire_tick's on {old_hit}");
}

#[test]
fn the_shipped_value_is_fire_tick() {
    assert_eq!(Calib::shipped().evo_chain_shot_launch, EvoChainShotLaunch::FireTick);
}
