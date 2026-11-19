//! spells.BARRAGE_LANDING_PHASE: when an Evo Cannon's barrage bomb due this tick lands against the tick's attack pass
//! (state.rs `land_barrage_early`, `land_barrage_before_attack`).
//!
//! THE LAW, measured on client 16.635 (parity r96, the oracle's il-mg-16635 set, 450 battles): on the tick a bomb's damage
//! lands, the units it kills make no attack update: 22 of 22 barrage-killed units with a hit due on their death tick did not
//! strike (20 battles). Their attackers took their next target on the loss + 6 (10 of 10), so the bomb lands after the Target
//! phase. Every other killer's victims still struck on their death tick (melee strikes 288 of 313, crown-tower shots 259 of
//! 276, troop shots 179 of 186, Fireball 23 of 24, Arrows 11 of 11, Snowball 9 of 9).
//!
//! WHAT IS PINNED, on tests/barrage_reach.rs's Blue Evo Cannon barrage (a bomb on (9000, 18000), landing I + 26): a Red
//! Skeleton held at (9000, 17900), striking a Blue Golem held at (9000, 17200) (it strikes nothing back), set down at the
//! offset whose strike, under after_attack_before_move, lands on the bomb's tick L:
//!   1. under client16635_before_attack the Golem loses nothing on L; under after_attack_before_move (the vacuity check) it
//!      loses the Skeleton's hit; the bomb kills the Skeleton on L under both.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! barrage_landing_phase`):
//!   * `barrage_kill_after_attack` -- the new arm's bombs still land after the attack pass and hold no victim: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BarrageLandingPhase, BattleState, TapSnap};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// The scene with the Skeleton set down `off` ticks after I - 40 under `arm`: (the Golem's hitpoints lost on the tick the
/// Skeleton died, that tick after I), or None when the Skeleton outlived I + 40.
fn landing(arm: BarrageLandingPhase, off: u32) -> Option<(i32, u32)> {
    let mut cfg = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg.calib.placement_tap_snap = TapSnap::None;
    cfg.calib.barrage_landing_phase = arm;
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let golem = s.scenario_spawn_now(Team::Blue, "Golem", n(9000, 17200), None).expect("the Golem");
    let start = s.tick_count();
    let mut skel = None;
    let mut play = None;
    for k in 0..90u32 {
        if k == off {
            skel = Some(s.scenario_spawn_now(Team::Red, "Skeleton", n(9000, 17900), None).expect("the Skeleton"));
        }
        if k == 40 {
            play = Some(s.tick_count());
            s.spawn_unit(Team::Blue, "Cannon_EV1", n(9000, 9500), None).expect("the Evo Cannon");
        }
        assert!(s.debug_set_pos(golem, n(9000, 17200)));
        if let Some(sk) = skel {
            if s.entity(sk).is_some() {
                assert!(s.debug_set_pos(sk, n(9000, 17900)));
            }
        }
        let before = s.entity(golem).expect("the Golem").hp;
        let alive = skel.is_some_and(|sk| s.entity(sk).is_some());
        s.tick();
        if alive && skel.is_some_and(|sk| s.entity(sk).is_none()) {
            let lost = before - s.entity(golem).expect("the Golem").hp;
            let p = play?;
            return Some((lost, s.tick_count() - 1 - p));
        }
    }
    let _ = start;
    None
}

/// spells.BARRAGE_LANDING_PHASE = client16635_before_attack (client 16.635, parity r96, the oracle's il-mg-16635 set: 22 of 22
/// barrage-killed units with a hit due did not strike on their death tick): the Skeleton the bomb kills does not strike the
/// Golem on the bomb's tick; under after_attack_before_move (the vacuity check) it does. Plant: barrage_kill_after_attack.
#[test]
fn under_client16635_before_attack_a_unit_a_barrage_bomb_kills_does_not_strike_on_its_death_tick() {
    let off = (0..30)
        .find(|&o| landing(BarrageLandingPhase::AfterAttackBeforeMove, o).is_some_and(|(lost, at)| lost > 0 && (24..=30).contains(&at)))
        .expect("the scene drifted: no offset puts the Skeleton's strike on the bomb's tick under after_attack_before_move");
    let (old, l_old) = landing(BarrageLandingPhase::AfterAttackBeforeMove, off).expect("the bomb kills the Skeleton");
    let (new, l_new) = landing(BarrageLandingPhase::Client16635BeforeAttack, off).expect("the bomb kills the Skeleton");
    assert_eq!(l_new, l_old, "the scene: the bomb's tick under both arms");
    assert!(old > 0, "after_attack_before_move (the vacuity check): no strike on the bomb's tick");
    assert_eq!(new, 0, "client16635_before_attack: the Skeleton struck {new} on the bomb's tick I + {l_new}");
}
