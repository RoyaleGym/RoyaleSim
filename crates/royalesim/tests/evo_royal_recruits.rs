//! THE EVO ROYAL RECRUITS (card.rs `EvoDef::charge_after_shield`; state.rs the charge's gains), at level 11.
//!
//! Read off the table (characters_evo Recruit_EV1; character_buffs_evo RecruitsCharge_EV1), not measured: the second
//! play of an evolved Royal Recruits entry is the form (DarkElixirCost 1); a recruit's ShieldLostAction lands for good a
//! buff whose OverrideChargeRange (250) is its charge's range, beside the row's ChargeSpeedMultiplier (200) and
//! DamageSpecial (104): it runs up and charges only once its shield is gone. The client's scene (sp-form-RoyalRecruits-
//! evo-s0) has its recruits lose their shields in melee, where no run-up shows.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! evo_royal_recruits`):
//!   - charge_before_shield_loss -> `a_recruit_runs_up_only_once_its_shield_is_gone` red.
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
    cfg.decks = [vec!["RoyalRecruits".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    s
}

#[test]
fn the_second_play_is_the_evolved_recruits() {
    let mut s = battle();
    assert_eq!(s.evo_counters(Team::Blue)[0].cycles, 1, "the form's DarkElixirCost");
    let next = |s: &BattleState| -> String {
        let slot = s.hand(Team::Blue).iter().position(|c| *c == "RoyalRecruits").expect("the recruits in hand");
        let p = s.resolve_play(Team::Blue, slot).expect("a play resolves");
        s.cards().get(p.card).name.clone()
    };
    assert_eq!(next(&s), "RoyalRecruits", "the first play is basic");
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.deploy(Team::Blue, "RoyalRecruits", n(9500, 5500)).expect("the play");
    for _ in 0..20 {
        s.tick();
    }
    assert_eq!(next(&s), "RoyalRecruits_EV1", "the second play is the form");
}

#[test]
fn a_recruit_runs_up_only_once_its_shield_is_gone() {
    // The evolved recruits walking on their own side with no enemy near: 60 ticks with their shields and no run-up; one
    // recruit's shield set to 0, and it charges within 80 ticks while the others still have none.
    let mut s = battle();
    s.spawn_unit(Team::Blue, "RoyalRecruits_EV1", n(9500, 6000), None).expect("the recruits");
    s.tick();
    let ids: Vec<_> = find_live(&s, Team::Blue, "RoyalRecruits_EV1").iter().map(|e| e.id).collect();
    assert_eq!(ids.len(), 6, "six recruits");
    for _ in 0..60 {
        s.tick();
        for id in &ids {
            let e = s.entity(*id).expect("a recruit");
            assert!(e.shield > 0, "its shield holds");
            assert!(e.charge_progress == 0 && !e.charged, "no run-up with the shield");
        }
    }
    assert!(s.debug_set_shield(ids[0], 0));
    let mut charged = None;
    for k in 0..80 {
        s.tick();
        if charged.is_none() && s.entity(ids[0]).expect("the recruit").charged {
            charged = Some(k);
        }
        assert!(!s.entity(ids[1]).expect("another recruit").charged, "a shielded one never charges");
    }
    assert!(charged.is_some(), "the recruit without its shield charges");
}
