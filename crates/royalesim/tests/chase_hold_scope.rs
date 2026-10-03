//! targeting.CHASE_HOLD_SCOPE: which holders keep a troop target past round sight when the rescan finds nothing in sight
//! (target.rs `decide`, the hold past round sight).
//!
//! THE LAW, measured on client 15.535.29 (parity's attack_leave_census.py): a troop leaving its attack holding a troop
//! past its round sight, nothing else in sight, lets it go for its crown tower (16 of 19), where a walker keeps it
//! (targeting.CHASE_HOLD_PAST_LIMIT's measurements). sp-form-RoyalHogs-evo-s0 t1268: a Musketeer whose shot at an Evo
//! Royal Hog left from beyond reach took Blue's tower.
//!
//! Read under targeting.CHASE_HOLD_PAST_LIMIT = client15535_troops_kept, the hold's 15.535.29 arm (under the shipped
//! inside_only every holder lets a troop past the chase-drop limit go, so the arms would not part).
//!
//! The scene: a Red Musketeer on (9000, 20000) and a Blue Knight held 6,900 below it (inside its reach, 7,000); once the
//! Musketeer is in its attack the Knight is held 7,250 below (past reach and round sight, inside the projectile hold,
//! 7,500), and the Musketeer's next shot leaves from beyond reach. WHAT IS PINNED, each with its preconditions (the
//! Musketeer is in its attack on the Knight when it is moved, and launches at it from there):
//!   1. client15535_walkers_only: on the tick after that launch the Musketeer holds anything but the Knight;
//!   2. every_holder: it holds the Knight (the old arm);
//!   3. the shipped value is every_holder (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test chase_hold_scope`):
//!   * `chase_hold_while_attacking` -- the new arm still holds for a holder in its attack: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, ChaseHoldPastLimit, ChaseHoldScope};
use royalesim::{EntityId, Team};

const MUSKETEER_AT: (i32, i32) = (9000, 20000);
const NEAR: i32 = 6900;
const FAR: i32 = 7250;
/// The ticks allowed for each step of the scene.
const STEP: u32 = 80;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

fn shots(s: &BattleState, m: EntityId) -> usize {
    s.projectiles().iter().filter(|p| p.firer == Some(m)).count()
}

/// Holds the Knight on `at`, topped up, for one tick.
fn tick_held(s: &mut BattleState, k: EntityId, at: Vec2) {
    assert!(s.debug_set_pos(k, at));
    let top = s.entity(k).expect("the Knight").max_hp;
    assert!(s.debug_set_hp(k, top));
    s.tick();
}

/// Under `arm`: the Musketeer's target on the tick after its launch from beyond reach, and the Knight.
fn scene(arm: ChaseHoldScope) -> (Option<EntityId>, EntityId) {
    let mut cfg = config();
    cfg.calib.chase_hold_scope = arm;
    cfg.calib.chase_hold_past_limit = ChaseHoldPastLimit::Client15535TroopsKept;
    let mut s = BattleState::new(0, cfg);
    let m = s.scenario_spawn_now(Team::Red, "Musketeer", n(MUSKETEER_AT.0, MUSKETEER_AT.1), None).expect("a red Musketeer");
    let (near, far) = (n(MUSKETEER_AT.0, MUSKETEER_AT.1 - NEAR), n(MUSKETEER_AT.0, MUSKETEER_AT.1 - FAR));
    let k = s.scenario_spawn_now(Team::Blue, "Knight", near, None).expect("a blue Knight");
    let mut attacking = false;
    for _ in 0..STEP {
        tick_held(&mut s, k, near);
        let e = s.entity(m).expect("the Musketeer");
        if e.target == Some(k) && e.attack_phase != AttackPhase::Idle {
            attacking = true;
            break;
        }
    }
    assert!(attacking, "{arm:?}: the scene drifted: the Musketeer never stood in its attack on the Knight");
    let mut launched = false;
    for _ in 0..STEP {
        let before = shots(&s, m);
        tick_held(&mut s, k, far);
        let e = s.entity(m).expect("the Musketeer");
        if shots(&s, m) > before {
            assert_eq!(e.target, Some(k), "{arm:?}: the scene drifted: the launch was at another target");
            launched = true;
            break;
        }
        assert_eq!(e.target, Some(k), "{arm:?}: the scene drifted: the Musketeer let the Knight go before its launch");
    }
    assert!(launched, "{arm:?}: the scene drifted: no launch from beyond reach");
    tick_held(&mut s, k, far);
    (s.entity(m).expect("the Musketeer").target, k)
}

#[test]
fn a_holder_whose_launch_left_from_beyond_reach_lets_a_target_past_round_sight_go() {
    let (now, k) = scene(ChaseHoldScope::Client15535WalkersOnly);
    assert_ne!(now, Some(k), "new: the Musketeer still holds the Knight past round sight after its launch from beyond reach");
}

#[test]
fn the_old_value_keeps_the_target_past_round_sight() {
    let (now, k) = scene(ChaseHoldScope::EveryHolder);
    assert_eq!(now, Some(k), "old: the Musketeer let the Knight go");
}

#[test]
fn the_shipped_value_is_every_holder() {
    assert_eq!(Calib::shipped().chase_hold_scope, ChaseHoldScope::EveryHolder);
}
