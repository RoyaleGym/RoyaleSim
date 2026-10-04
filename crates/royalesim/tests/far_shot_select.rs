//! combat.FAR_SHOT_SELECT_MOMENT: when the Evo Archer's power shot (card.rs `FarShotDef`) picks her arrow (state.rs
//! `phase_attack_for`, `select_attack`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, sp-form-Archer-evo-s0: 14 of 14 arrows took the far arrow
//! (140 at level 11) or the near one (112) as their target stood on the tick the archer's swing's load started.
//!
//! THE SCENE: an evolved Archer pair on Blue's bank, a red Knight put down 10,000 above them walking at them, Blue's
//! princess towers down. The Knight crosses her reach (4,500 + both radii) while she shoots, between a hit and the next
//! load start (from 18,200 to 19,900 the arms part on the fifth arrow; from 18,000 the crossing fell elsewhere and they
//! did not). Each arm's arrows are read off the Knight's hitpoint drops (140 far, 112 near). WHAT IS PINNED:
//!   1. client15535_at_load_start: the arms part on at least one arrow, and on every one that parts the new arm's is the
//!      near arrow (the Knight walking in: her load start sees it nearer than the hit before did);
//!   2. as_attack_select (the old arm, the vacuity check): its first arrow that parts is the far one;
//!   3. the shipped value is as_attack_select (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test far_shot_select`):
//!   * `far_shot_at_swing_start` -- the new arm still picks as ATTACK_SELECT_MOMENT does: the arms never part, (1) red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, FarShotSelectMoment};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The Knight's hitpoint drops (one per tick it is hit) over 200 ticks under `arm`.
fn knight_drops(arm: FarShotSelectMoment) -> Vec<i32> {
    let mut cfg: BattleConfig = config();
    cfg.calib.far_shot_select_moment = arm;
    cfg.decks = [vec!["Archers".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(5, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_tower_hp(Team::Blue, 1, 0).unwrap();
    s.scenario_set_tower_hp(Team::Blue, 2, 0).unwrap();
    s.spawn_unit(Team::Blue, "Archer_EV1", n(9000, 9000), None).expect("the form");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(9000, 19000), None).expect("the Knight");
    let mut hp = s.entity(knight).expect("the Knight").hp;
    let mut drops = Vec::new();
    for _ in 0..200 {
        s.tick();
        let Some(k) = s.entity(knight) else { break };
        if k.hp < hp {
            drops.push(hp - k.hp);
        }
        hp = k.hp;
    }
    drops
}

/// Plant: far_shot_at_swing_start.
#[test]
fn the_evo_archers_arrow_is_picked_at_her_load_start_under_client15535_at_load_start() {
    let (old, new) = (knight_drops(FarShotSelectMoment::AsAttackSelect), knight_drops(FarShotSelectMoment::Client15535AtLoadStart));
    assert!(new.len() >= 4, "the scene drifted: the Knight took {} hits", new.len());
    let parted: Vec<(i32, i32)> = old.iter().zip(&new).filter(|(o, w)| o != w).map(|(o, w)| (*o, *w)).collect();
    assert!(!parted.is_empty(), "new: no arrow parted from the old arm's: {old:?} / {new:?}");
    // The Knight walks in: the load start sees it nearer than the hit before, so a parted arrow is the near one.
    assert!(parted[0].1 < parted[0].0, "new: the first parted arrow is not the near one: {parted:?}");
}

#[test]
fn the_old_value_picks_at_the_hit_before() {
    let (old, new) = (knight_drops(FarShotSelectMoment::AsAttackSelect), knight_drops(FarShotSelectMoment::Client15535AtLoadStart));
    let first = old.iter().zip(&new).find(|(o, w)| o != w).map(|(o, _)| *o).expect("the scene drifted: the arms never parted");
    assert!(first > *new.iter().min().expect("hits"), "old: its first parted arrow is not the far one ({first}): {old:?}");
}

#[test]
fn the_shipped_value_is_as_attack_select() {
    assert_eq!(Calib::shipped().far_shot_select_moment, FarShotSelectMoment::AsAttackSelect);
}
