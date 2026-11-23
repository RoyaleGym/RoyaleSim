//! combat.SPIN_CHAIN_END: what a Hero Valkyrie's chain does when it runs out while her spin goes on (state.rs
//! `spin_seek`).
//!
//! THE LAW, measured on client 16.635 (parity r98, the oracle's il-mg-16635 set: 21 re-begins in 13 battles after a chain
//! had run out, 19 on the first tick an unreached enemy's centre stood within 5500, none against): the chain's buff comes
//! off and it waits; while the spin lasts and links remain, the closest enemy in the circle she has not reached begins it
//! again at the top of the Path phase, the chain buff back on. The engine's arm, ends, keeps it over for the rest of the
//! spin.
//!
//! THE SCENE: blue's princess towers down; the hero held; a red Giant held 1500 to her right, in her reach (her chain's
//! first target, reached on the begin tick, so the chain runs out there); a red Knight held 6500 to her left, outside the
//! circle; the press (combat.SPIN_BEGIN = client15535_next_tick, combat.SPIN_TARGETS = troops). On the press's frame 10 (T)
//! the Knight is put 4000 to her left, inside the circle, and the hero is no longer held.
//!
//! WHAT IS PINNED: under client16635_waits_again her chain begins again on the Knight on T (150 a tick toward it, the
//! Knight her target); under ends (the vacuity check) she stands at the Giant in her reach under the spin's NO_ATTACK.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! spin_chain_end`):
//!   * `spin_chain_ends_for_good` -- the new arm's chain still ends for good:
//!     `under_client16635_waits_again_a_spent_chain_begins_again_on_a_newcomer` red (she stands).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::FORM_HERO;
use royalesim::entity::EntityKind;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, SpinBegin, SpinChainEnd, SpinTargets};
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

const HERO: (i32, i32) = (9000, 14000);
const GIANT: (i32, i32) = (10500, 14000);
const FAR: (i32, i32) = (2500, 14000);
const NEAR: (i32, i32) = (5000, 14000);
/// The press's frame (0 its first) on which the Knight comes into the circle.
const T: usize = 10;

/// Under combat.SPIN_CHAIN_END = `arm`: her step on T (native units: dx, dy, length), her target after it, and the Knight.
fn scene(arm: SpinChainEnd) -> ((i32, i32, i32), Option<EntityId>, EntityId) {
    let mut s = battle(|c| {
        c.calib.spin_chain_end = arm;
        c.calib.spin_targets = SpinTargets::Troops;
        c.calib.spin_begin = SpinBegin::Client15535NextTick;
    });
    s.spawn_unit(Team::Blue, "Valkyrie_hero", n(HERO.0, HERO.1), None).expect("the hero");
    let giant = s.scenario_spawn_now(Team::Red, "Giant", n(GIANT.0, GIANT.1), None).expect("a red Giant");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", n(FAR.0, FAR.1), None).expect("a red Knight");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Valkyrie_hero")[0].id;
    let tops = (s.entity(giant).expect("the Giant").max_hp, s.entity(knight).expect("the Knight").max_hp);
    let hold = |s: &mut BattleState, knight_at: (i32, i32), hero_too: bool| {
        if hero_too {
            assert!(s.debug_set_pos(hero, n(HERO.0, HERO.1)));
        }
        assert!(s.debug_set_pos(giant, n(GIANT.0, GIANT.1)) && s.debug_set_hp(giant, tops.0));
        assert!(s.debug_set_pos(knight, n(knight_at.0, knight_at.1)) && s.debug_set_hp(knight, tops.1));
    };
    for _ in 0..40 {
        hold(&mut s, FAR, true);
        s.tick();
    }
    assert!(s.entities().all(|e| !e.deploying), "the scene is still deploying");
    hold(&mut s, FAR, true);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut step = (0, 0, 0);
    let mut target = None;
    for k in 0..=T {
        if k < T {
            hold(&mut s, FAR, true);
        } else {
            hold(&mut s, NEAR, false);
        }
        let at = s.entity(hero).expect("the hero").pos;
        s.tick();
        if k == T {
            let h = s.entity(hero).expect("the hero");
            step = ((h.pos.x - at.x) / K, (h.pos.y - at.y) / K, h.pos.dist(at) / K);
            target = h.target;
        }
    }
    (step, target, knight)
}

/// Plant: spin_chain_ends_for_good.
#[test]
fn under_client16635_waits_again_a_spent_chain_begins_again_on_a_newcomer() {
    let (old, _, _) = scene(SpinChainEnd::Ends);
    // NOT VACUOUS: the engine's chain is over; she holds the Giant in her reach under the spin's NO_ATTACK.
    assert!(old.2 <= 5, "ends: she stands at the Giant on T: {old:?}");
    let (new, target, knight) = scene(SpinChainEnd::Client16635WaitsAgain);
    // 150 a tick (the chain buff's 250 %) toward the Knight's side; the step aims at her route's first cell centre, so it can
    // lean off the straight line (-106, 106 on this ground)
    assert!((148..=151).contains(&new.2) && new.0 < -100, "client16635_waits_again: 150 toward the Knight on T: {new:?}");
    assert_eq!(target, Some(knight), "client16635_waits_again: the Knight, the closest she has not reached, her chain's target on T");
}

/// Since parity r98 for clients 16.402 and 16.635 (ends before it; client 15.535.29 keeps ends).
#[test]
fn the_shipped_arm_is_client16635_waits_again() {
    assert_eq!(
        royalesim::state::Calib::shipped().spin_chain_end,
        royalesim::state::SpinChainEnd::Client16635WaitsAgain
    );
    assert_eq!(config().calib.spin_chain_end, royalesim::state::SpinChainEnd::Client16635WaitsAgain);
}
