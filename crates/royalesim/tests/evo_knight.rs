//! THE EVO KNIGHT (tools/extract_cards.py `data_only_block`; card.rs the evolution's data; its idle buff is
//! `IdleBuffDef::own`, combat.rs `idle_on`, `damage_reduction_of`), at level 11.
//!
//! THE MEASUREMENT (sp-form-Knight-evo-s0): the evolved Knight took a Musketeer's 217 as 86 and a Knight's 202 as 80
//! (its BuffWhenNotAttacking Knight_Fortify_EV1: DamageReduction 60, truncated) while it had not hit, walking or in its
//! first swing, and 217 and 202 in full once it was hitting. BuffWhenNotAttackingUseAttackRange is not read.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_knight`):
//!   - evo_data_dropped -> `a_musketeers_217_lands_as_86_while_it_has_not_hit` red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn battle() -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.decks = [vec!["Knight".into(), "Archer".into()], vec!["Musketeer".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

#[test]
fn a_musketeers_217_lands_as_86_while_it_has_not_hit() {
    // The evolved Knight held on blue's side, a red Musketeer held 4500 ahead (out of every blue tower's reach, the
    // Knight never reaching it): each of its shots takes 86 off the Knight, 217 x 40 / 100.
    let mut s = battle();
    s.spawn_unit(Team::Blue, "Knight_EV1", n(9000, 10000), None).expect("the Knight");
    // The Knight deployed and idle (its buff up) before the Musketeer comes.
    for _ in 0..60 {
        s.tick();
    }
    let musk = s.scenario_spawn_now(Team::Red, "Musketeer", n(9000, 14500), None).expect("a red Musketeer");
    s.tick();
    let knight = find_live(&s, Team::Blue, "Knight_EV1")[0].id;
    let mut losses = Vec::new();
    for _ in 0..140 {
        assert!(s.debug_set_pos(knight, n(9000, 10000)));
        assert!(s.debug_set_pos(musk, n(9000, 14500)));
        let before = s.entity(knight).expect("the Knight").hp;
        s.tick();
        let after = s.entity(knight).expect("the Knight").hp;
        if after < before {
            losses.push(before - after);
        }
    }
    assert!(losses.len() >= 3, "three shots in 140 ticks: {losses:?}");
    assert!(losses.iter().all(|d| *d == 86), "each 217 lands as 86: {losses:?}");
}
