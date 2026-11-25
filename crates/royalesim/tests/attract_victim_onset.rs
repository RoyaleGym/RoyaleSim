//! WHICH UNITS A PULL MOVES ON A TICK -- calibration status.ATTRACT_VICTIM_ONSET, state.rs `phase_path16402_for` (the
//! attract pre-pass), `spawn_with`, `phase_reap` and `deploy_countdown` (Entities::attract_from).
//!
//! THE READING (client16635_seen_last_update, shipped at its old arm same_tick): a pulling area moves on tick t only the
//! units its update on t - 1 saw, alive and not waiting out a formation stagger at that tick's move pass. A unit made in
//! the Spawn phase is first pulled on its creation tick + 1; one made after the tick's main pre-pass (a release, a death
//! spawn, a cube, a Move-phase spawner unit), and a member whose stagger ran out, on that tick + 2. Measured on client
//! 16.635 (parity r100, the oracle's il-mg-16635 set: 21 of 21 waiting Bats first pulled on b + 2m + 1, 10 of 10 deploys
//! on b + 1, 6 of 6 Barbarian Barrel Barbarians on r + 2). Under same_tick every live unit in reach is pulled, a unit made
//! this tick and a waiting member included.
//!
//! THE SCENES (status.ATTRACT_ONSET = client16402_area_plus_1, shipped: a Tornado cast on D pulls on D + 2 to D + 22):
//!   1. a red Tornado on (9000, 12000) at level 6, and six ticks on a blue Bats at level 14 on (9000, 8000), 4,000 short
//!      of its centre (the levels keep the Bats alive through the Tornado's pulse). Its five members, ranked by team_seq,
//!      wait 100 m ms (SummonDeployDelay 100) from their creation tick b. The control is the same battle with no Tornado;
//!   2. a blue Barbarian Barrel cast at (9000, 6000), whose Barbarian the roll releases on r (read off the control with
//!      no Tornado), and a red Tornado cast on r - 8, 2,000 beyond the roll's end.
//!
//! WHAT IS PINNED:
//!   1. client16635_seen_last_update: each member stands on the control's point on b, and member m's first pulled tick
//!      (its first step of at least 0.45 of the pull toward the centre) is b + 2m + 1; under same_tick (the vacuity
//!      check) every member is pulled on b;
//!   2. client16635_seen_last_update: the Barbarian stands on the control's point on r and r + 1 and parts from it on
//!      r + 2; under same_tick (the vacuity check) on r;
//!   3. a battle under the new arm saved with a member's attract_from set resumes identically, and one under the old arm
//!      saves no attract_from;
//!   4. the shipped value is same_tick.
//!
//! PLANT (regression):
//!   * `attract_victim_same_tick` -- the new arm still pulls a unit made this tick, a waiting member and a release on its
//!     next tick: (1) and (2) go red.
//!     RUSTFLAGS='--cfg clash_plant="attract_victim_same_tick"' CARGO_TARGET_DIR=target/plant cargo test --profile
//!     gate --test attract_victim_onset
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::EntityKind;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{AttractOnset, AttractVictimOnset, BattleConfig, BattleState, Calib};
use royalesim::{EntityId, Team};

/// The Tornado's centre and the Bats' tap in scene 1, native.
const CENTRE: (i32, i32) = (9000, 12000);
const BATS_AT: (i32, i32) = (9000, 8000);
/// A Bat's pull a tick, native: its speed 120 at the Tornado's 360 %.
const PULL: i64 = 120 * 360 / 100;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// An engine point in native units.
fn native(p: Vec2) -> (i32, i32) {
    (p.x / K, p.y / K)
}

/// The shipped config under status.ATTRACT_VICTIM_ONSET = `arm`, the area pulling on D + 2 to D + 22.
fn cfg(arm: AttractVictimOnset) -> BattleConfig {
    let mut c = config();
    c.calib.attract_onset = AttractOnset::Client16402AreaPlus1;
    c.calib.attract_victim_onset = arm;
    c
}

/// Scene 1's battle up to the Bats' play: (the battle, D).
fn bats_battle(arm: AttractVictimOnset, tornado: bool) -> (BattleState, u32) {
    let mut s = BattleState::new(0, cfg(arm));
    let d = s.tick_count();
    if tornado {
        s.spawn_unit(Team::Red, "Tornado", at(CENTRE), Some(6)).expect("cast the Tornado");
    }
    for _ in 0..6 {
        s.tick();
    }
    s.spawn_unit(Team::Blue, "Bats", at(BATS_AT), Some(14)).expect("play the Bats");
    (s, d)
}

/// The blue troops (the Bats' members), ranked by team_seq.
fn members(s: &BattleState) -> Vec<EntityId> {
    let mut v: Vec<(u32, EntityId)> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::Troop).map(|e| (e.team_seq, e.id)).collect();
    v.sort();
    v.into_iter().map(|(_, id)| id).collect()
}

/// Ticks `s` until the members exist: (b, their ids by rank).
fn until_born(s: &mut BattleState) -> (u32, Vec<EntityId>) {
    for _ in 0..20 {
        s.tick();
        let m = members(s);
        if !m.is_empty() {
            assert_eq!(m.len(), 5, "the scene drifted: {} Bats members", m.len());
            return (s.tick_count() - 1, m);
        }
    }
    panic!("the scene drifted: no Bats in 20 ticks");
}

/// Scene 1: (D, b, per member by rank its point on each tick from b through D + 24).
fn bats(arm: AttractVictimOnset, tornado: bool) -> (u32, u32, Vec<Vec<Vec2>>) {
    let (mut s, d) = bats_battle(arm, tornado);
    let (b, ids) = until_born(&mut s);
    let mut rows: Vec<Vec<Vec2>> = vec![Vec::new(); ids.len()];
    loop {
        for (r, id) in rows.iter_mut().zip(&ids) {
            r.push(s.entity(*id).unwrap_or_else(|| panic!("the scene drifted: a Bat died by tick {}", s.tick_count() - 1)).pos);
        }
        if s.tick_count() > d + 24 {
            break;
        }
        s.tick();
    }
    (d, b, rows)
}

/// Is the step from `p` to `q` at least 0.45 of a Bat's pull toward the Tornado's centre? Not more: member 4 stands nearest
/// the centre, member 3's pull runs into it and shoves it 34 % of a pull on b + 8, and from b + 9 the two touch, so its
/// pulled steps are 54 % and less (a lone member's are 93 to 99 %).
fn pulled_step(p: Vec2, q: Vec2) -> bool {
    let (p, q) = (native(p), native(q));
    let (sx, sy) = (i64::from(q.0 - p.0), i64::from(q.1 - p.1));
    let (cx, cy) = (i64::from(CENTRE.0 - p.0), i64::from(CENTRE.1 - p.1));
    let len = isqrt(cx * cx + cy * cy).max(1);
    (sx * cx + sy * cy) * 100 >= PULL * 45 * len
}

/// The first tick after b on which a member whose points from b are `row` takes a pulled step, if any.
fn first_pull(b: u32, row: &[Vec2]) -> Option<u32> {
    row.windows(2).position(|w| pulled_step(w[0], w[1])).map(|k| b + k as u32 + 1)
}

/// status.ATTRACT_VICTIM_ONSET = client16635_seen_last_update (client 16.635, parity r100, the oracle's il-mg-16635 set:
/// 21 of 21 waiting Bats first pulled on b + 2m + 1, 10 of 10 deploys on b + 1): each Bats member stands on the control's
/// point on its creation tick b, member 0 is first pulled on b + 1 and member m on b + 2m + 1, the tick after its wait
/// ends plus one; under same_tick (the vacuity check) all five are pulled on b. Plant: attract_victim_same_tick.
#[test]
fn the_new_arm_pulls_a_deploy_from_its_first_frame_plus_1_and_each_waiting_bat_from_its_staggers_end_plus_1() {
    let (d, b, control) = bats(AttractVictimOnset::Client16635SeenLastUpdate, false);
    let (d1, b1, new) = bats(AttractVictimOnset::Client16635SeenLastUpdate, true);
    assert_eq!((d1, b1), (d, b), "the scene: one timeline");
    assert!(b >= d + 2 && b + 9 <= d + 22, "the scene drifted: the Bats are born on {b}, the Tornado pulls on {} to {}", d + 2, d + 22);
    for (m, (row, ctl)) in new.iter().zip(&control).enumerate() {
        assert_eq!(row[0], ctl[0], "client16635_seen_last_update: member {m} pulled on its creation tick {b}");
        assert_eq!(first_pull(b, row), Some(b + 2 * m as u32 + 1), "client16635_seen_last_update: member {m}'s first pull (b {b})");
    }
    let (_, _, old) = bats(AttractVictimOnset::SameTick, true);
    let (_, _, old_control) = bats(AttractVictimOnset::SameTick, false);
    for (m, (row, ctl)) in old.iter().zip(&old_control).enumerate() {
        assert_ne!(row[0], ctl[0], "same_tick (the vacuity check): member {m} not pulled on its creation tick {b}");
    }
}

/// Scene 2: one blue Barbarian Barrel cast at (9000, 6000) before the first tick, and when `tornado` is Some((D, p)) a red
/// Tornado cast at p on D: (the release tick r, the Barbarian's point on r and the 7 ticks after).
fn barbarian(arm: AttractVictimOnset, tornado: Option<(u32, Vec2)>) -> (u32, Vec<Vec2>) {
    let mut s = BattleState::new(0, cfg(arm));
    s.spawn_unit(Team::Blue, "BarbLog", at((9000, 6000)), None).expect("cast the Barbarian Barrel");
    for _ in 0..80 {
        if let Some((_, p)) = tornado.filter(|(d, _)| *d == s.tick_count()) {
            s.spawn_unit(Team::Red, "Tornado", p, None).expect("cast the Tornado");
        }
        s.tick();
        let barbs: Vec<EntityId> = s.entities().filter(|e| e.team == Team::Blue && e.card == "Barbarian").map(|e| e.id).collect();
        if let Some(&id) = barbs.first() {
            assert_eq!(barbs.len(), 1, "the scene drifted: the barrel released {} Barbarians", barbs.len());
            let r = s.tick_count() - 1;
            let mut rows = Vec::new();
            for _ in 0..8 {
                rows.push(s.entity(id).expect("the Barbarian lives").pos);
                s.tick();
            }
            return (r, rows);
        }
    }
    panic!("the scene drifted: no Barbarian in 80 ticks");
}

/// status.ATTRACT_VICTIM_ONSET = client16635_seen_last_update (client 16.635, parity r100, the oracle's il-mg-16635 set:
/// 6 of 6 Barbarian Barrel Barbarians first pulled on their release r + 2, the engine's on r): the Barbarian, made after
/// its release tick's pre-pass, stands on the control's point on r and r + 1 and is pulled off it on r + 2; under
/// same_tick (the vacuity check) its creation tick's first update pulls it on r. Plant: attract_victim_same_tick.
#[test]
fn a_released_barbarian_is_pulled_from_its_release_plus_2() {
    let (r, control) = barbarian(AttractVictimOnset::Client16635SeenLastUpdate, None);
    let d = r.saturating_sub(8);
    assert!(r >= d + 2, "the scene drifted: the barrel released on {r}");
    let centre = control[0].add(Vec2::new(0, 2000 * K));
    let parted = |rows: &[Vec2]| rows.iter().zip(&control).position(|(a, b)| a != b).map(|k| r + k as u32);
    let (r1, new) = barbarian(AttractVictimOnset::Client16635SeenLastUpdate, Some((d, centre)));
    assert_eq!(r1, r, "the scene: one release tick");
    assert_eq!(parted(&new), Some(r + 2), "client16635_seen_last_update: the Barbarian's first pull (released on {r}, the Tornado cast on {d})");
    let (r2, old) = barbarian(AttractVictimOnset::SameTick, Some((d, centre)));
    assert_eq!(r2, r, "the scene: one release tick");
    assert_eq!(parted(&old), Some(r), "same_tick (the vacuity check): the Barbarian's first pull (released on {r})");
}

/// Scene 1 under the new arm, saved after the tick b + 1 (member 1's wait ended in its Reap, so its attract_from is set),
/// resumes identically to D + 24, every member's point and the hash; the same battle under same_tick saves no
/// attract_from.
#[test]
fn a_battle_under_the_new_arm_resumes_identically_after_a_save() {
    let (mut s, d) = bats_battle(AttractVictimOnset::Client16635SeenLastUpdate, true);
    let (b, ids) = until_born(&mut s);
    while s.tick_count() < b + 2 {
        s.tick();
    }
    let bytes = s.save();
    assert!(String::from_utf8_lossy(&bytes).contains("\"attract_from\""), "the scene drifted: no attract_from is set on b + 2 ({b})");
    let mut back = BattleState::load(&bytes).expect("the save loads");
    assert_eq!(back.state_hash(), s.state_hash(), "the hash right after the restore");
    while s.tick_count() <= d + 24 {
        s.tick();
        back.tick();
        let t = s.tick_count() - 1;
        assert_eq!(back.state_hash(), s.state_hash(), "tick {t} after the restore");
        for id in &ids {
            assert_eq!(back.entity(*id).map(|e| e.pos), s.entity(*id).map(|e| e.pos), "member {id:?} on tick {t}");
        }
    }
    let (mut old, _) = bats_battle(AttractVictimOnset::SameTick, true);
    let (b_old, _) = until_born(&mut old);
    while old.tick_count() < b_old + 2 {
        old.tick();
    }
    assert!(!String::from_utf8_lossy(&old.save()).contains("attract_from"), "same_tick saved an attract_from");
}

/// status.ATTRACT_VICTIM_ONSET ships at same_tick (client16635_seen_last_update is a candidate until scored).
#[test]
fn the_shipped_value_is_same_tick() {
    assert_eq!(Calib::shipped().attract_victim_onset, AttractVictimOnset::SameTick);
    assert_eq!(config().calib.attract_victim_onset, AttractVictimOnset::SameTick);
}
