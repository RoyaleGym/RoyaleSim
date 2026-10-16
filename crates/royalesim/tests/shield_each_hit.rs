//! combat.SHIELD_TICK_SUM (combat.rs `resolve`): how a tick's hits on a shielded unit meet its shield.
//!
//! THE LAW, client16402_each_hit (client 16.402, parity's r74 census of the live11 set: of 140 shielded lives the truth's first
//! loss was earlier in 6, 3 of them with two or more hits on the breaking tick, the drop equal to the later hit; no first loss
//! a partial hit): each hit meets the shield in the order landed, the breaking hit absorbed whole, the tick's later hits on
//! hitpoints. tick_sum (the engine's) sums the tick's hits and lets a live shield absorb the sum.
//!
//! THE SCENE: a Red Knight given a 50 shield, two Blue Zaps cast on it on one gap (both land on the next tick).
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="shield_takes_the_tick_sum"' CARGO_TARGET_DIR=target/plant cargo test --test
//! shield_each_hit`): the new arm still sums: the test goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, ShieldTickSum};
use royalesim::Team;

/// Under `arm`: (the Knight's hp loss, its shield after) on the tick two Zaps land on it.
fn two_zaps(arm: ShieldTickSum) -> (i32, i32) {
    let mut cfg = config();
    cfg.calib.shield_tick_sum = arm;
    let mut s = BattleState::new(0, cfg);
    let at = Vec2::new(9000 * K, 22000 * K);
    let k = s.scenario_spawn_now(Team::Red, "Knight", at, None).expect("the Knight");
    s.tick();
    assert!(s.debug_set_shield(k, 50));
    let hp0 = s.entity(k).expect("the Knight").hp;
    let p = s.entity(k).expect("the Knight").pos;
    s.spawn_unit(Team::Blue, "Zap", p, None).expect("a Zap");
    s.spawn_unit(Team::Blue, "Zap", p, None).expect("another Zap");
    for _ in 0..6 {
        s.tick();
        let v = s.entity(k).expect("the Knight");
        if v.shield < 50 {
            return (hp0 - v.hp, v.shield);
        }
    }
    panic!("the scene drifted: the Zaps never reached the Knight");
}

/// combat.SHIELD_TICK_SUM = client16402_each_hit: the first Zap breaks the 50 shield and is absorbed whole; the second reaches
/// hitpoints. Under tick_sum (the vacuity check) the shield absorbs both. Plant: shield_takes_the_tick_sum.
#[test]
fn under_client16402_each_hit_the_hit_after_the_breaking_one_reaches_hitpoints() {
    let (old_loss, old_shield) = two_zaps(ShieldTickSum::TickSum);
    assert_eq!((old_loss, old_shield), (0, 0), "tick_sum (the vacuity check): the shield did not absorb both Zaps");
    let (new_loss, new_shield) = two_zaps(ShieldTickSum::Client16402EachHit);
    assert_eq!(new_shield, 0, "client16402_each_hit: the shield not broken");
    assert!(new_loss > 0, "client16402_each_hit: the second Zap did not reach hitpoints ({new_loss})");
}
