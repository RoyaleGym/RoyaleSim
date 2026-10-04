//! combat.LAUNCH_BEYOND_CANCEL_RANGE: whether a projectile attacker's due launch is made when its target stands far past
//! its reach (combat.rs `fire`): the projectile half of combat.HIT_BEYOND_CANCEL_RANGE.
//!
//! THE EVIDENCE (the ledger has the rows): client 16.402, 20260920-071744 t1983, both seats: a Lava Pup's swing at a
//! Goblin 2,393 past reach ended with its load timer reset and no projectile; 1,259 due launches stood within 633 past.
//!
//! THE SCENE: Blue's Minion held on (9000, 9000) and a red Knight held 3,000 above it (reach 2,500 + 500 + 500 = 3,500),
//! until the Minion's first shot leaves (its phase Cooldown); 20 ticks on (its next launch due on the 24th, HitSpeed
//! 1,200), the Knight is moved `d` above the Minion and held there for 60 ticks, on Blue's bank (the river starts at
//! 15,000). Red's towers stand out of the Minion's sight, so its scans find only the Knight. A launch is read off the
//! projectiles in flight (`Projectile::firer`), so Blue's tower arrows at the Knight do not count. WHAT IS PINNED:
//!   1. not_launched, d = 5,300 (1,800 past reach, past the cancel range of 1,500; inside the chase-drop limit, 5,500 on
//!      |dy|, past which its rescan would pass over a target walking away): the Minion's cycle reaches its next launch
//!      (Cooldown again) and no shot of its leaves;
//!   2. not_launched, d = 4,500 (1,000 past reach): its shot leaves (the cancel line is reach + 1,500, not reach);
//!   3. launched (the old arm, the vacuity check), d = 5,300: its shot leaves;
//!   4. the shipped value is launched.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="launch_beyond_cancel_launched"' CARGO_TARGET_DIR=target/plant cargo test --test
//! launch_beyond_cancel`):
//!   * `launch_beyond_cancel_launched` -- the new arm still launches past the cancel range: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, LaunchBeyondCancelRange};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`, the Knight moved `d` above the Minion before its second launch: (whether its cycle reached that launch,
/// whether a shot of the Minion's left after the move).
fn second_launch(arm: LaunchBeyondCancelRange, d: i32) -> (bool, bool) {
    let mut cfg = config();
    cfg.calib.launch_beyond_cancel_range = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(0, cfg);
    past_deploy_lockout(&mut s);
    let (m, near) = (n(9000, 9000), n(9000, 12000));
    let minion = s.scenario_spawn_now(Team::Blue, "Minions", m, None).expect("the Minion");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", near, None).expect("the Knight");
    let max = s.entity(knight).expect("the Knight").max_hp;
    let hold = |s: &mut BattleState, at: Vec2| {
        assert!(s.debug_set_pos(minion, m));
        assert!(s.debug_set_pos(knight, at));
        assert!(s.debug_set_hp(knight, max));
    };
    let mut first = None;
    for k in 0..200 {
        hold(&mut s, near);
        s.tick();
        let e = s.entity(minion).expect("the Minion");
        if e.target == Some(knight) && e.attack_phase == AttackPhase::Cooldown {
            first = Some(k);
            break;
        }
    }
    assert!(first.is_some(), "{arm:?}: the scene drifted: the Minion never shot at the Knight");
    // Out of the first launch's Cooldown and its shot landed; the next launch is due 24 ticks after the first.
    for _ in 0..20 {
        hold(&mut s, near);
        s.tick();
    }
    assert!(!s.projectiles().iter().any(|p| p.firer == Some(minion)), "{arm:?}: the scene drifted: the first shot still flies");
    let far = n(9000, 9000 + d);
    let (mut cycled, mut shot) = (false, false);
    for _ in 0..60 {
        hold(&mut s, far);
        s.tick();
        let e = s.entity(minion).expect("the Minion");
        assert_eq!(e.target, Some(knight), "{arm:?}: the scene drifted: the Minion let the Knight go at {d}");
        cycled |= e.attack_phase == AttackPhase::Cooldown;
        shot |= s.projectiles().iter().any(|p| p.firer == Some(minion));
    }
    (cycled, shot)
}

/// Plant: launch_beyond_cancel_launched.
#[test]
fn a_launch_past_the_cancel_range_is_not_made_under_not_launched() {
    let (cycled, shot) = second_launch(LaunchBeyondCancelRange::NotLaunched, 5300);
    assert!(cycled, "the scene drifted: the Minion's cycle never reached its next launch");
    assert!(!shot, "not_launched: the Minion launched at the Knight 1,800 past reach");
}

#[test]
fn a_launch_inside_the_cancel_range_is_made_under_not_launched() {
    let (cycled, shot) = second_launch(LaunchBeyondCancelRange::NotLaunched, 4500);
    assert!(cycled && shot, "not_launched: the Minion did not launch at the Knight 1,000 past reach (cycled {cycled})");
}

#[test]
fn the_old_value_launches() {
    let (cycled, shot) = second_launch(LaunchBeyondCancelRange::Launched, 5300);
    assert!(cycled && shot, "old: the Minion did not launch at the Knight 1,800 past reach (vacuous otherwise; cycled {cycled})");
}

#[test]
fn the_shipped_value_is_launched() {
    assert_eq!(Calib::shipped().launch_beyond_cancel_range, LaunchBeyondCancelRange::Launched);
}
