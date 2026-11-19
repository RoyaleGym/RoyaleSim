//! combat.PINGPONG_CLOCK: how a pingpong throw's clock runs (combat.rs `step_straight`'s pingpong branch,
//! `Straight::rate_ms`); its thrower is held while it is out (`throwers_out`).
//!
//! THE LAW, measured on client 16.635 (parity r96, the oracle's il-mg-16635 set: 223 Executioner throws in 10 battles): the
//! axe's clock runs at the thrower's composed hit-speed tick read at the throw, not one tick a tick: 202 of 202 axes thrown
//! at 50 lived 31 frames (the thrower stepping again on the throw + 32 in 183 of 186), 21 of 21 thrown at 35 (a -30 % slow)
//! lived 44 (stepping again on + 45, 14 of 14), 1 of 1 thrown at 65 (Rage) lived 25. The slowed axes' points follow the
//! whole-degree sine of the ms elapsed.
//!
//! WHAT IS PINNED, on a Blue Executioner held at (9000, 10000) throwing at a Red Giant held at (9000, 14000):
//!   1. with a Red Snowball's slow on him at his first throw, the axe lives 44 frames under client16635_thrower_hit_speed and
//!      31 under ticks (the vacuity check: his advance into the throw tick is 35, the slow is on);
//!   2. with no slow both arms' axes live 31 frames.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! pingpong_clock`):
//!   * `pingpong_clock_unscaled` -- the new arm's clock runs at TICK_MS whatever the thrower's buffs: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, PingpongClock, TapSnap};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The scene under `arm`, a Red Snowball cast on the Executioner's point first when `snow`: (the frames his first axe is in
/// the projectile list, his progress advance into the tick it is thrown).
fn first_throw(arm: PingpongClock, snow: bool) -> (u32, i32) {
    let mut cfg = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.placement_tap_snap = TapSnap::None;
    cfg.calib.pingpong_clock = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let (eat, gat) = (n(9000, 10000), n(9000, 14000));
    let exe = s.scenario_spawn_now(Team::Blue, "Executioner", eat, None).expect("the Executioner");
    if snow {
        // the Snowball flies ~24 ticks from Red's King: its slow must be on him before his first throw, so the Giant comes after
        s.spawn_unit(Team::Red, "Snowball", eat, None).expect("the Snowball");
        let mut flying = false;
        for _ in 0..60 {
            assert!(s.debug_set_pos(exe, eat));
            s.tick();
            if !s.spells().is_empty() {
                flying = true;
            } else if flying {
                break;
            }
        }
        assert!(flying && s.spells().is_empty(), "the scene drifted: the Snowball never landed");
    }
    let giant = s.scenario_spawn_now(Team::Red, "Giant", gat, None).expect("the Giant");
    let axe_out = |s: &BattleState| s.projectiles().iter().any(|p| p.team == Team::Blue && p.straight.as_ref().is_some_and(|st| st.period > 0));
    let mut prev = s.entity(exe).expect("the Executioner").attack_ms;
    let (mut life, mut adv) = (0u32, None);
    for _ in 0..300 {
        assert!(s.debug_set_pos(exe, eat) && s.debug_set_pos(giant, gat));
        s.tick();
        let now = s.entity(exe).expect("the Executioner").attack_ms;
        if axe_out(&s) {
            if adv.is_none() {
                adv = Some(now - prev);
            }
            life += 1;
        } else if let Some(a) = adv {
            return (life, a);
        }
        prev = now;
    }
    panic!("the scene drifted: no axe came and went (life {life})");
}

/// combat.PINGPONG_CLOCK = client16635_thrower_hit_speed (client 16.635, parity r96, the oracle's il-mg-16635 set: 21 of 21
/// axes thrown at 35 lived 44 frames, 202 of 202 at 50 lived 31): under a Snowball's slow the axe lives 44 frames; under
/// ticks (the vacuity check) 31; with no slow 31 under both. Plant: pingpong_clock_unscaled.
#[test]
fn under_client16635_thrower_hit_speed_a_slowed_executioners_axe_lives_44_frames() {
    let (old, adv) = first_throw(PingpongClock::Ticks, true);
    assert_eq!(adv, 35, "the scene drifted: his advance into the throw is {adv}, the slow is not on");
    assert_eq!(old, 31, "ticks (the vacuity check): the slowed axe lived {old} frames");
    let (new, adv) = first_throw(PingpongClock::Client16635ThrowerHitSpeed, true);
    assert_eq!(adv, 35, "the scene drifted: his advance into the throw is {adv}");
    assert_eq!(new, 44, "client16635_thrower_hit_speed: the slowed axe lived {new} frames");
    assert_eq!(first_throw(PingpongClock::Ticks, false).0, 31, "ticks: an unslowed axe");
    assert_eq!(first_throw(PingpongClock::Client16635ThrowerHitSpeed, false).0, 31, "client16635_thrower_hit_speed: an unslowed axe");
}
