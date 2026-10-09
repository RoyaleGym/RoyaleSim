//! A VICTIM DOOMED ONLY BY A LATE SHOT (item 320; combat.POST_KILL_DOOM_ETA, state.rs `phase_target_with`'s doomed test):
//! combat.POST_KILL_RETARGET_WAIT's clause (c) frees an attacker whose target died doomed by the homing shots in flight at
//! it. Under client_within_600 the last of those shots must also land within target.rs DOOMED_ETA_LIMIT_MS (600 ms); a
//! victim whose covering shot is further out is not doomed, and its attacker serves the wait.
//!
//! THE MEASUREMENT (client 15.535.29's raw frames, 8,261 target losses of attackers in their attack): doomed with the last
//! shot within 600 ms, crown towers were freed 4,610 of 4,693 times; doomed with it later, they waited 35 of 35 (600 ms: 31
//! freed, 0 waits; 650 ms: 0 freed, 15 waits). sp-h2-s0 t308: a Minion took a tower's arrow 700 ms out on its last live
//! tick, and the tower held no target t309..t314. The 16.402 corpus agrees (7 of 7 later than 600 waited).
//!
//! THE SCENE: the blue left princess tower shoots a red Giant held at the edge of its reach, its hitpoints put back each
//! tick; a second red Giant is held out of reach. A probe reads each tower arrow's launch and landing ticks. The scene is
//! then run again to a chosen tick L, before which the first Giant's hitpoints are set to 1 and a blue Zap is cast on it
//! (its damage lands in L's Resolve, after the Target phase: the Giant dies in L's Reap, still a target in L's Target phase)
//! and the second is moved into the tower's reach, clear of the Zap: (A) L the tick after an arrow's launch (650 ms or more still to fly as L begins),
//! (B) L the arrow's landing tick (it lands within L).
//!
//! WHAT IS PINNED: under client_within_600 the tower holds no target on L + 1 in (A), and takes the second Giant on L + 1 in
//! (B); under the shipped ignored it takes the second Giant on L + 1 in both.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test post_kill_doom_eta`):
//!   * `post_kill_doom_eta_ignored` -- the doom ignores when the shots land: (A) under client_within_600 goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::arena::Lane;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, PostKillDoomEta, PostKillWait};
use royalesim::{EntityId, Team};

/// Native points: the first Giant at the edge of the left princess tower's reach, the second far off, then in reach.
const G1: (i32, i32) = (3000, 14800);
const G2_FAR: (i32, i32) = (14500, 22000);
const G2_NEAR: (i32, i32) = (5000, 11500);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

struct Run {
    /// Per tick k: the tower's arrows at the first Giant in flight at k's start.
    arrows: Vec<usize>,
    /// Per tick k: the tower's target after k.
    target: Vec<Option<EntityId>>,
    g2: EntityId,
}

fn scene(arm: PostKillDoomEta, kill: Option<usize>, ticks: usize) -> Run {
    let mut cfg = config();
    cfg.calib.post_kill_doom_eta = arm;
    let mut s = BattleState::new(0, cfg);
    let pt = s.arena().princess_tower_pos(Team::Blue, Lane::Left);
    let tower = s.entities().find(|e| e.team == Team::Blue && e.kind.is_crown_tower() && e.pos == pt).map(|e| e.id).expect("the blue left princess tower");
    let g1 = s.scenario_spawn_now(Team::Red, "Giant", at(G1), None).expect("the first Giant");
    let g2 = s.scenario_spawn_now(Team::Red, "Giant", at(G2_FAR), None).expect("the second Giant");
    let full = s.entity(g1).expect("the first Giant").max_hp;
    let (mut arrows, mut target) = (Vec::new(), Vec::new());
    for k in 0..ticks {
        let near = kill.is_some_and(|l| k >= l);
        assert!(s.debug_set_pos(g2, at(if near { G2_NEAR } else { G2_FAR })));
        assert!(s.debug_set_hp(g2, s.entity(g2).expect("the second Giant").max_hp));
        if s.entity(g1).is_some() {
            assert!(s.debug_set_pos(g1, at(G1)));
            assert!(s.debug_set_hp(g1, if kill == Some(k) { 1 } else { full }));
            if kill == Some(k) {
                s.spawn_unit(Team::Blue, "Zap", at(G1), None).expect("the Zap");
            }
        }
        arrows.push(s.projectiles().iter().filter(|p| p.firer == Some(tower) && p.target == g1).count());
        s.tick();
        target.push(s.entity(tower).expect("the tower stands").target);
    }
    Run { arrows, target, g2 }
}

/// The probe: (A) the tick after an arrow's launch, past tick 60, whose arrow's launch-to-landing span is 14 ticks or more
/// (the engine counts 13 of them still to fly as L begins: 650 ms); (B) that arrow's
/// landing tick. An arrow launched on k is in flight from k + 1's start; one landing on k is gone from k + 1's start.
fn probe() -> (usize, usize) {
    let r = scene(PostKillDoomEta::Ignored, None, 400);
    let n = &r.arrows;
    assert!(n.iter().all(|&c| c <= 1), "the scene drifted: two tower arrows in flight at once: {n:?}");
    let launch = (61..n.len() - 1).find(|&k| n[k] == 0 && n[k + 1] == 1).expect("the scene drifted: the tower never shot the first Giant");
    let land = (launch + 1..n.len() - 1).find(|&k| n[k + 1] == 0).expect("the scene drifted: the arrow never landed");
    assert!(land - launch >= 14, "the scene drifted: the tower's arrow spans {} ticks, under 14 (650 ms to fly as L begins)", land - launch);
    (launch + 1, land)
}

/// The tower's target on L + 1, with the first Giant dying in L's Reap (zapped at 1 hitpoint).
fn after(arm: PostKillDoomEta, l: usize) -> (Option<EntityId>, EntityId) {
    let r = scene(arm, Some(l), l + 3);
    (r.target[l + 1], r.g2)
}

/// Plant: post_kill_doom_eta_ignored.
#[test]
fn a_victim_doomed_only_by_a_shot_landing_later_than_600_ms_leaves_its_attacker_waiting() {
    let (late, soon) = probe();
    let (t, _) = after(PostKillDoomEta::ClientWithin600, late);
    assert_eq!(t, None, "client_within_600: the tower was freed though its arrow lands later than 600 ms");
    let (t, g2) = after(PostKillDoomEta::ClientWithin600, soon);
    assert_eq!(t, Some(g2), "client_within_600: the tower was not freed by its arrow landing within the tick");
    // NOT VACUOUS: the shipped arm frees it in both.
    let (t, g2) = after(PostKillDoomEta::Ignored, late);
    assert_eq!(t, Some(g2), "ignored: the tower waited though its arrow covers the Giant");
    let (t, g2) = after(PostKillDoomEta::Ignored, soon);
    assert_eq!(t, Some(g2), "ignored: the tower waited though its arrow lands within the tick");
}

#[test]
fn the_shipped_arm_ignores_when_the_shots_land() {
    assert_eq!(Calib::shipped().post_kill_doom_eta, PostKillDoomEta::Ignored);
    assert_eq!(Calib::shipped().post_kill_wait, PostKillWait::AttackFinish, "the doomed test is the attack-finish wait's");
}
