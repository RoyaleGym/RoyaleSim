//! spawner.THROWN_UNIT_DEATH_BLOW: whether the death blows of the tick a throw lands reach the unit it puts down (state.rs
//! `throw_pass`, `Scratch::thrown`; `phase_reap`).
//!
//! THE LAW, measured on client 15.535.29 (the ledger has the rows): the Hero Balloon's trooper landing on an Ice Golemite
//! its landing blow killed kept 473 of 473 through the golem's death blow (sp-balloon-g4500-s0 t781, -g6000-s0 t760).
//!
//! THE SCENE (tests/hero_balloon.rs's): Blue's level-11 Hero Balloon held at (10060, 8820), a red Ice Golemite held at
//! (11722, 14451); the press; the golem set to 200 hp before every tick after it (in Blue's right tower's reach, a tower
//! shot does not kill it, where at 50 one did before the landing), so the landing blow (263) kills it (its death blow on
//! its death tick, combat.DEATH_DAMAGE_TICK = client15535_death_tick). WHAT IS PINNED:
//!   1. client15535_unreached: the trooper stands at its full hp on its landing tick and the next two, the golem gone on
//!      the landing tick;
//!   2. reached (the old arm, the vacuity check): it has lost hp by then;
//!   3. the shipped value is reached (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test thrown_unit_death_blow`):
//!   * `thrown_unit_blown` -- the new arm's blow still reaches the trooper: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, DeathDamageTick, ThrownUnitDeathBlow};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Balloon", "Knight", "Archer", "Giant", "Valkyrie", "HogRider", "Fireball", "Zap"];
const BALLOON: (i32, i32) = (10060, 8820);
const GOLEM: (i32, i32) = (11722, 14451);

/// Under `arm`: the trooper's hp and max hp on its landing tick and the two after, and whether the golem is gone on the
/// landing tick.
fn trooper(arm: ThrownUnitDeathBlow) -> (Vec<(i32, i32)>, bool) {
    let mut cfg: BattleConfig = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.thrown_unit_death_blow = arm;
    cfg.calib.death_damage_tick = DeathDamageTick::Client15535DeathTick;
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.spawn_unit(Team::Blue, "Balloon_hero", n(BALLOON.0, BALLOON.1), None).expect("the hero");
    s.spawn_unit_resolved(Team::Red, "IceGolemite", n(GOLEM.0, GOLEM.1), None).expect("the golem");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Balloon_hero")[0].id;
    let golem = s.entities().find(|e| e.team == Team::Red && e.card == "IceGolemite").expect("the golem").id;
    let hold = |s: &mut BattleState| {
        if s.entity(hero).is_some() {
            assert!(s.debug_set_pos(hero, n(BALLOON.0, BALLOON.1)));
        }
        if s.entity(golem).is_some() {
            assert!(s.debug_set_pos(golem, n(GOLEM.0, GOLEM.1)));
        }
    };
    for _ in 0..40 {
        hold(&mut s);
        s.tick();
    }
    s.press_ability_button(Team::Blue, 0).expect("the press");
    for _ in 0..60 {
        hold(&mut s);
        assert!(s.debug_set_hp(golem, 200), "the scene drifted: the golem died before the landing");
        s.tick();
        let trooper = s.entities().find(|e| e.card == "SkeletonTrooper").map(|e| e.id);
        if let Some(t) = trooper {
            let gone = s.entity(golem).is_none();
            let mut rows = Vec::new();
            for k in 0..3 {
                if k > 0 {
                    s.tick();
                }
                let e = s.entity(t).expect("the trooper");
                rows.push((e.hp, e.max_hp));
            }
            return (rows, gone);
        }
    }
    panic!("{arm:?}: the scene drifted: no trooper landed");
}

/// Plant: thrown_unit_blown.
#[test]
fn a_thrown_trooper_is_not_reached_by_its_landing_ticks_death_blow_under_client15535_unreached() {
    let (rows, gone) = trooper(ThrownUnitDeathBlow::Client15535Unreached);
    assert!(gone, "the scene drifted: the golem outlived the landing tick");
    assert!(rows.iter().all(|(hp, max)| hp == max), "new: the trooper lost hp: {rows:?}");
}

#[test]
fn the_old_value_lets_the_blow_reach_it() {
    let (rows, gone) = trooper(ThrownUnitDeathBlow::Reached);
    assert!(gone, "the scene drifted: the golem outlived the landing tick");
    assert!(rows.iter().any(|(hp, max)| hp < max), "old: the trooper lost nothing (vacuous otherwise): {rows:?}");
}

#[test]
fn the_shipped_value_is_reached() {
    assert_eq!(Calib::shipped().thrown_unit_death_blow, ThrownUnitDeathBlow::Reached);
}
