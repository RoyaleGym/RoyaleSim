//! status.STUN_CLEARS_TARGET = client16635_kill_before_late_clear: a unit whose target dies on the tick a hit's stun lands on it
//! keeps the dead target past the H + 1 clear, so the post-kill check sees the kill and its wait follows.
//!
//! Plant: stun_kill_cleared (`under_client16635_kill_before_late_clear_a_kill_on_the_stuns_tick_starts_the_post_kill_wait` red).
//!
//! status.STUN_CLEARS_TARGET = client16635_troop_kill_before_late_clear: the same for a troop alone; a tower is cleared as
//! before. Plant: stun_kill_first_towers
//! (`under_client16635_troop_kill_before_late_clear_a_tower_that_kills_on_the_stuns_tick_keeps_its_clock` red).
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

/// Blue's left princess tower, a red Skeleton put at (3500, 9000) and held there at 1 hitpoint (the tower's first scan takes it,
/// its next arrow kills it), and a red Electro Wizard put at (3500, 11000) on tick `wait`, stunning it about every hit speed, under
/// status.STUN_CLEARS_TARGET = `arm`: (the Skeleton's death tick, whether a stun landed on the tower on it, the tower's attack
/// clock on each of the 40 ticks after).
fn tower_stun_on_kill(arm: StunClearsTarget, wait: u32) -> (Option<u32>, bool, Vec<i32>) {
    let mut cfg = config();
    cfg.calib.stun_clears_target = arm;
    let mut s = BattleState::new(0, cfg);
    let tower = s
        .entities()
        .filter(|e| e.team == Team::Blue && e.kind == royalesim::entity::EntityKind::PrincessTower)
        .min_by_key(|e| e.pos.x)
        .expect("blue's left princess tower")
        .id;
    let (w_at, sk_at) = (Vec2::new(3500 * K, 11000 * K), Vec2::new(3500 * K, 9000 * K));
    // The Skeleton first: the tower's first scan takes it (a tower keeps a live target in reach, so with the Wizard first it
    // takes the Skeleton only on a stun's resume and kills it 14 to 31 ticks into his 36-tick cycle, never on a stun tick).
    let skel = Some(s.scenario_spawn_now(Team::Red, "Skeleton", sk_at, None).expect("the Skeleton"));
    let mut wiz = None;
    let (mut death, mut landed, mut clock) = (None, false, Vec::new());
    let mut stun_prev = 0;
    for t in 0..400u32 {
        if t == wait {
            wiz = Some(s.scenario_spawn_now(Team::Red, "ElectroWizard", w_at, None).expect("the Electro Wizard"));
        }
        if let Some(w) = wiz.filter(|w| s.entity(*w).is_some()) {
            let hw = s.entity(w).expect("him").max_hp;
            assert!(s.debug_set_pos(w, w_at) && s.debug_set_hp(w, hw));
        }
        if let Some(sk) = skel.filter(|sk| s.entity(*sk).is_some()) {
            assert!(s.debug_set_pos(sk, sk_at) && s.debug_set_hp(sk, 1));
        }
        s.tick();
        let Some(tw) = s.entity(tower) else { break };
        let stun_rose = tw.stun_ms > stun_prev;
        stun_prev = tw.stun_ms;
        if death.is_none() && skel.is_some_and(|sk| s.entity(sk).is_none()) {
            death = Some(t);
            landed = stun_rose;
            if !landed {
                break;
            }
            continue;
        }
        if death.is_some() {
            clock.push(tw.attack_ms);
            if clock.len() == 40 {
                break;
            }
        }
    }
    (death, landed, clock)
}

/// status.STUN_CLEARS_TARGET = client16635_troop_kill_before_late_clear (client 16.635, parity r96, the oracle's il-mg-16635 set:
/// c0739445 t1117, a princess tower that killed on the tick a reflect stunned it kept its clock and retook a target on the
/// stun's end + 1): a tower whose Skeleton dies on the tick the Electro Wizard's stun lands on it runs its clock after as
/// under client16402_hit_hold_next_tick; under client16635_kill_before_late_clear (the vacuity check) its post-kill wait
/// changes it. Plant: stun_kill_first_towers.
#[test]
fn under_client16635_troop_kill_before_late_clear_a_tower_that_kills_on_the_stuns_tick_keeps_its_clock() {
    let old = StunClearsTarget::Client16402HitHoldNextTick;
    let wait = (0..160u32)
        .find(|&w| {
            let (d, landed, c) = tower_stun_on_kill(old, w);
            d.is_some() && landed && c.len() == 40
        })
        .expect("the scene drifted: no Skeleton delay puts the tower's kill on a tick the Electro Wizard's stun lands on it");
    let (d_old, _, c_old) = tower_stun_on_kill(old, wait);
    let (d_all, _, c_all) = tower_stun_on_kill(StunClearsTarget::Client16635KillBeforeLateClear, wait);
    let (d_troop, landed, c_troop) = tower_stun_on_kill(StunClearsTarget::Client16635TroopKillBeforeLateClear, wait);
    assert!(d_all == d_old && d_troop == d_old && landed, "the scene: the arms differ before the kill");
    assert_ne!(c_all, c_old, "client16635_kill_before_late_clear (the vacuity check): the tower's clock ran as under client16402_hit_hold_next_tick");
    assert_eq!(c_troop, c_old, "client16635_troop_kill_before_late_clear: the tower's clock left client16402_hit_hold_next_tick's");
}

/// Since parity r96 for clients 16.402 and 16.635 (client16402_hit_hold_next_tick since parity r82, client_cleared
/// before it; client 15.535.29 keeps client_cleared).
#[test]
fn the_shipped_arm_is_client16635_troop_kill_before_late_clear() {
    assert_eq!(
        royalesim::state::Calib::shipped().stun_clears_target,
        royalesim::state::StunClearsTarget::Client16635TroopKillBeforeLateClear
    );
    assert_eq!(config().calib.stun_clears_target, royalesim::state::StunClearsTarget::Client16635TroopKillBeforeLateClear);
}
