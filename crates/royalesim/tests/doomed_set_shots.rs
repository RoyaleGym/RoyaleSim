//! targeting.DOOMED_SET_SHOTS: which shots in flight the doomed set counts (combat.rs `shots_in_flight_at`; state.rs
//! `homing_only`).
//!
//! THE EVIDENCE (the ledger has the rows): both clients' own pending_damage leaves a non-homing shot out (a Bomber's bomb,
//! a Princess's arrows): 0 on 2,590 of 2,689 16.402 corpus frames and 2,309 of 2,532 15.535.29 ones with only such shots
//! in flight; 20260918-115249.b1 t3013, three Archers kept a Goblin Hut a Bomber's bomb would kill.
//!
//! THE SCENE: Blue's Bomber held on (9000, 10500) and a red Knight held 4,000 above it, on Blue's bank and out of every Blue
//! tower's reach (9,708 from a princess tower, reach 9,000), until the Bomber's bomb leaves (its phase Cooldown); the
//! Knight then set to 1 hitpoint (the bomb in flight covers it) and Blue's Musketeer put down on (9000, 9500), 5,000 from
//! the Knight (inside its reach of 7,000), with nothing else of Red's in its sight. WHAT IS PINNED (whether the Musketeer
//! takes the Knight in the 5 ticks after, the bomb still flying):
//!   1. client_homing_only: it takes it (the bomb dooms nothing);
//!   2. every_shot (the old arm, the vacuity check): it does not (a projectile attacker passes over a doomed unit);
//!   3. the shipped value is every_shot (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="doomed_set_counts_every_shot"' CARGO_TARGET_DIR=target/plant cargo test --test
//! doomed_set_shots`):
//!   * `doomed_set_counts_every_shot` -- the new arm's doomed set still counts the bomb: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, DoomedSetShots};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`: whether the Musketeer put down beside the Bomber takes the Knight while the bomb flies.
fn musketeer_takes_the_knight(arm: DoomedSetShots) -> bool {
    let mut cfg = config();
    cfg.calib.doomed_set_shots = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    let (b, k) = (n(9000, 10500), n(9000, 14500));
    let bomber = s.scenario_spawn_now(Team::Blue, "Bomber", b, None).expect("the Bomber");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", k, None).expect("the Knight");
    let max = s.entity(knight).expect("the Knight").max_hp;
    let mut launched = false;
    for _ in 0..200 {
        assert!(s.debug_set_pos(bomber, b));
        assert!(s.debug_set_pos(knight, k));
        assert!(s.debug_set_hp(knight, max));
        s.tick();
        let e = s.entity(bomber).expect("the Bomber");
        if e.target == Some(knight) && e.attack_phase == AttackPhase::Cooldown {
            launched = true;
            break;
        }
    }
    assert!(launched, "{arm:?}: the scene drifted: the Bomber never threw at the Knight");
    assert!(s.debug_set_hp(knight, 1));
    let musketeer = s.scenario_spawn_now(Team::Blue, "Musketeer", n(9000, 9500), None).expect("the Musketeer");
    for _ in 0..5 {
        assert!(s.debug_set_pos(bomber, b));
        assert!(s.debug_set_pos(knight, k));
        s.tick();
        assert!(s.entity(knight).is_some(), "{arm:?}: the scene drifted: the bomb landed within 5 ticks");
        if s.entity(musketeer).expect("the Musketeer").target == Some(knight) {
            return true;
        }
    }
    false
}

/// Plant: doomed_set_counts_every_shot.
#[test]
fn a_bomb_in_flight_dooms_nothing_under_client_homing_only() {
    assert!(musketeer_takes_the_knight(DoomedSetShots::ClientHomingOnly), "new: the Musketeer passed over the Knight a bomb would kill");
}

#[test]
fn the_old_value_counts_the_bomb() {
    assert!(!musketeer_takes_the_knight(DoomedSetShots::EveryShot), "old: the Musketeer took the doomed Knight (vacuous otherwise)");
}

#[test]
fn the_shipped_value_is_every_shot() {
    assert_eq!(Calib::shipped().doomed_set_shots, DoomedSetShots::EveryShot);
}
