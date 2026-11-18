//! status.STUN_CLEARS_TARGET = client16635_kill_before_late_clear: a unit whose target dies on the tick a hit's stun lands on it
//! keeps the dead target past the H + 1 clear, so the post-kill check sees the kill and its wait follows.
//!
//! Plant: stun_kill_cleared (`under_client16635_kill_before_late_clear_a_kill_on_the_stuns_tick_starts_the_post_kill_wait` red).
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, StunClearsTarget};
use royalesim::Team;

/// A red Knight held at (9000, 21000) and a blue Electro Wizard held 3,000 off from the start (his shots stun the Knight about
/// every hit speed), and a blue Skeleton put 900 ahead of the Knight on tick `wait` and held there at 1 hitpoint (the Knight's
/// first hit kills it), under status.STUN_CLEARS_TARGET = `arm`; the Knight released on the Skeleton's death. Returns (the
/// Skeleton's death tick, whether a stun landed on the Knight on that tick, his first step after that stun's end).
fn stun_on_kill(arm: StunClearsTarget, wait: u32) -> (Option<u32>, bool, Option<u32>) {
    let mut cfg = config();
    cfg.calib.stun_clears_target = arm;
    let mut s = BattleState::new(0, cfg);
    let (w_at, k_at, sk_at) = (Vec2::new(9000 * K, 18000 * K), Vec2::new(9000 * K, 21000 * K), Vec2::new(9000 * K, 20100 * K));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", k_at, None).expect("the Knight");
    let wiz = s.scenario_spawn_now(Team::Blue, "ElectroWizard", w_at, None).expect("the Electro Wizard");
    let mut skel = None;
    let (mut death, mut landed, mut free_step) = (None, false, None);
    let (mut last, mut stun_prev) = (k_at, 0);
    for t in 0..400u32 {
        if t == wait {
            skel = Some(s.scenario_spawn_now(Team::Blue, "Skeleton", sk_at, None).expect("the Skeleton"));
        }
        if s.entity(wiz).is_some() {
            let hw = s.entity(wiz).expect("him").max_hp;
            assert!(s.debug_set_pos(wiz, w_at) && s.debug_set_hp(wiz, hw));
        }
        if let Some(sk) = skel.filter(|sk| s.entity(*sk).is_some()) {
            assert!(s.debug_set_pos(sk, sk_at) && s.debug_set_hp(sk, 1));
        }
        if death.is_none() {
            let hk = s.entity(knight).expect("the Knight").max_hp;
            assert!(s.debug_set_pos(knight, k_at) && s.debug_set_hp(knight, hk));
            last = k_at;
        }
        s.tick();
        let Some(k) = s.entity(knight) else { break };
        let stun_rose = k.stun_ms > stun_prev;
        stun_prev = k.stun_ms;
        if death.is_none() && skel.is_some_and(|sk| s.entity(sk).is_none()) {
            death = Some(t);
            landed = stun_rose;
            if !landed {
                break;
            }
            last = k.pos;
            continue;
        }
        if death.is_some() {
            if k.stun_ms == 0 && k.pos != last {
                free_step = Some(t);
                break;
            }
            last = k.pos;
        }
    }
    (death, landed, free_step)
}

/// status.STUN_CLEARS_TARGET = client16635_kill_before_late_clear (client 16.635, parity r94, live15n 20261103-164637-A and
/// 20261103-190440-A: 2 of 2 guards whose target died on the tick a hit's stun landed on them served the post-kill wait and
/// walked at the stun's end + 5; the engine cleared the target at H + 1 and walked at + 1): a Knight whose Skeleton dies on
/// the tick the Electro Wizard's stun lands on him walks later after his stun than under client16402_hit_hold_next_tick (the
/// vacuity check: no wait). Plant: stun_kill_cleared.
#[test]
fn under_client16635_kill_before_late_clear_a_kill_on_the_stuns_tick_starts_the_post_kill_wait() {
    let old = StunClearsTarget::Client16402HitHoldNextTick;
    let wait = (0..160u32)
        .find(|&w| {
            let (d, landed, _) = stun_on_kill(old, w);
            d.is_some() && landed
        })
        .expect("the scene drifted: no Skeleton delay puts the Knight's kill on a tick the Electro Wizard's stun lands on him");
    let (d_old, _, o) = stun_on_kill(old, wait);
    let (d_new, landed, n) = stun_on_kill(StunClearsTarget::Client16635KillBeforeLateClear, wait);
    assert!(d_new == d_old && landed, "the scene: the two arms differ before the kill ({d_old:?} / {d_new:?}, landed {landed})");
    let (o, n) = (o.expect("client16402_hit_hold_next_tick: the Knight never walked"), n.expect("client16635_kill_before_late_clear: the Knight never walked"));
    assert!(n > o, "client16635_kill_before_late_clear: the Knight walked on tick {n}, not after client16402_hit_hold_next_tick's {o} (no post-kill wait; wait {wait})");
}
