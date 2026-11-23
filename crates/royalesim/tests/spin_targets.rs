//! combat.SPIN_TARGETS: what a Hero Valkyrie's spin seeker and chain take (state.rs `spin_pick`).
//!
//! THE LAW, measured on client 16.635 (parity r98, the oracle's il-mg-16635 set: 7 of 7 spins whose nearest enemy ground
//! character in her 5500 circle was a building or a crown tower began or chained on it, 0 against): the seeker and the
//! chain take any enemy ground character by its centre, buildings and crown towers too; not a hidden building, a sleeping
//! King, or a member in its formation stagger or before its acquire time. The engine's arm, troops, takes troops alone.
//!
//! THE SCENE: blue's princess towers down; a red Cannon 4500 to the hero's right, its hp topped up every tick, and no red
//! troop anywhere; the hero held 40 ticks, then pressed (combat.SPIN_BEGIN = client15535_next_tick); her steps on the
//! press's first two frames.
//!
//! WHAT IS PINNED: under client16635_ground_characters her spin begins on the Cannon on the second frame (150 a tick, her
//! 60 at the chain buff's 250 %), the Cannon its target; under troops (the vacuity check) the seeker finds no one in the
//! circle and she walks at her own target at the pending buff's 200 % (120).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! spin_targets`):
//!   * `spin_takes_troops_only` -- the new arm still takes troops alone:
//!     `under_client16635_ground_characters_a_spin_begins_on_a_building_in_its_circle` red (she walks 120).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, SpinBegin, SpinTargets};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Valkyrie", "Knight", "Archer", "Giant", "Musketeer", "HogRider", "Fireball", "Zap"];

/// A level-11 battle past the opening lockout, the Hero Valkyrie in blue's deck, `f` applied to its config, and blue's
/// princess towers down (their shots would reach the scene; blue's King stays out of its reach).
fn battle(f: impl FnOnce(&mut BattleConfig)) -> BattleState {
    let mut cfg: BattleConfig = config();
    let deck: Vec<String> = DECK.iter().map(|n| n.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    f(&mut cfg);
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let towers: Vec<EntityId> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s
}

const HERO: (i32, i32) = (9000, 13000);
const CANNON: (i32, i32) = (13500, 13000);

/// Under combat.SPIN_TARGETS = `arm`: her steps (native units) on the press's first two frames, her target after the
/// second, and the Cannon.
fn steps(arm: SpinTargets) -> ([i32; 2], Option<EntityId>, EntityId) {
    let mut s = battle(|c| {
        c.calib.spin_targets = arm;
        c.calib.spin_begin = SpinBegin::Client15535NextTick;
    });
    let cannon = s.scenario_spawn_now(Team::Red, "Cannon", n(CANNON.0, CANNON.1), None).expect("a red Cannon");
    s.spawn_unit(Team::Blue, "Valkyrie_hero", n(HERO.0, HERO.1), None).expect("the hero");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Valkyrie_hero")[0].id;
    let top = s.entity(cannon).expect("the Cannon").max_hp;
    for _ in 0..40 {
        assert!(s.debug_set_pos(hero, n(HERO.0, HERO.1)) && s.debug_set_hp(cannon, top));
        s.tick();
    }
    assert!(s.entities().all(|e| !e.deploying), "the scene is still deploying");
    assert!(!s.entities().any(|e| e.team == Team::Red && e.kind == EntityKind::Troop), "the scene drifted: a red troop");
    assert!(s.debug_set_pos(hero, n(HERO.0, HERO.1)) && s.debug_set_hp(cannon, top));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut out = [0; 2];
    let mut target = None;
    for o in out.iter_mut() {
        let at = s.entity(hero).expect("the hero").pos;
        assert!(s.debug_set_hp(cannon, top));
        s.tick();
        let h = s.entity(hero).expect("the hero");
        *o = h.pos.dist(at) / K;
        target = h.target;
    }
    (out, target, cannon)
}

/// Plant: spin_takes_troops_only.
#[test]
fn under_client16635_ground_characters_a_spin_begins_on_a_building_in_its_circle() {
    let (old, _, _) = steps(SpinTargets::Troops);
    // NOT VACUOUS: the engine's arm takes no building; with no troop in the circle the seeker waits, and she walks at her
    // own target, the Cannon, at the pending buff's 200 %.
    assert!((115..=125).contains(&old[1]), "troops: the seeker waits, 120 on the second frame: {old:?}");
    let (new, target, cannon) = steps(SpinTargets::Client16635GroundCharacters);
    assert!(new[0] < 100, "client16635_ground_characters: her own walk on the press's first frame (combat.SPIN_BEGIN): {new:?}");
    assert!((148..=151).contains(&new[1]), "client16635_ground_characters: the spin begins on the Cannon, 150 on the second frame: {new:?}");
    assert_eq!(target, Some(cannon), "client16635_ground_characters: the Cannon, the one ground character in the circle, her chain's target");
}

/// Since parity r98 for clients 16.402 and 16.635 (troops before it; client 15.535.29 keeps troops).
#[test]
fn the_shipped_arm_is_client16635_ground_characters() {
    assert_eq!(
        royalesim::state::Calib::shipped().spin_targets,
        royalesim::state::SpinTargets::Client16635GroundCharacters
    );
    assert_eq!(config().calib.spin_targets, royalesim::state::SpinTargets::Client16635GroundCharacters);
}
