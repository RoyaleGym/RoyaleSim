//! THE DYING TROOP PARENT STEERS ITS SPLIT (spawner.SPAWNED_FIRST_STEP = client16402_same_tick; state.rs `phase_reap`'s
//! `dying_blockers`, `first_update`).
//!
//! THE LAW, measured: a death spawn's creation-tick update runs the avoidance scan with its dying parent still on the
//! board as a static blocker (seen by the avoidance scan, not by the separation push), a troop parent as well as a
//! building, on the parent's own layer (the scan skips a body of the other one). The scan sets the offset to +-200
//! and one decay leaves +-190 on the first frame; the parent is gone on the next tick, so the offset only decays, 10 a
//! tick, and the walk step turns back toward the goal as it does. On client 15.535.29 the rule predicts the
//! first-frame offset of the Elixir Golem's halves that walk on their first frame (42 of 42) and of the Battle Ram's
//! Barbarians (33 of 33, scanned along the Ram's heading, which they carry); the Goblin Giant's riders, flying rows,
//! leave their ground Spear Goblins at 0 (12 of 12). On the 16.402 corpus the one Elixir Golem split,
//! 20260920-090204-A/B tick 1260, is reproduced to the native unit: each half steps out, away from the dying Golem,
//! then turns back in at a full step. Without the parent as a blocker the engine turned them in from the first frame,
//! they reached the princess tower first, and the tower fell 3 ticks early.
//!
//! WHAT IS PINNED, each with the precondition that makes it bite:
//!   1. the corpus split: an ElixirGolem1 in its attack on the Red princess tower at the corpus death point, killed;
//!      its halves' first six frames are the client's, to the native unit, with the offsets -190, -180, ... and
//!      +190, +180, ...;
//!   2. the shape in the open: an ElixirGolem1 killed walking up its lane; over the first four frames each half steps
//!      out from the death point with its x step shrinking every tick, at a full step, its offset decaying from 190;
//!   3. the Battle Ram's Barbarians read +-190 on their first frame while they deploy, and 0 once they walk: the
//!      blocker never shows in their positions;
//!   4. a Goblin Giant's riders' Spear Goblins read 0 on their first frame: the riders are flying rows, and a
//!      blocker stands on its parent's layer.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test elixir_golem_split`):
//!   * `first_step_troop_parent_gone` -- a dying troop is not a blocker (the engine before this law): (1), (2) and (3)
//!     go red.
//!   * `first_step_blocker_ground` -- every blocker stands on the ground layer, a flying parent's too: (4) goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

fn native(v: Vec2) -> (i32, i32) {
    (v.x / K, v.y / K)
}

/// `name`'s Blue units that were not in `before`, by creation order.
fn born(s: &BattleState, name: &str, before: &[EntityId]) -> Vec<EntityId> {
    let mut v: Vec<(u32, EntityId)> = find_live(s, Team::Blue, name).iter().filter(|e| !before.contains(&e.id)).map(|e| (e.team_seq, e.id)).collect();
    v.sort();
    v.into_iter().map(|(_, id)| id).collect()
}

/// Kill `id` and tick once: `child`'s new Blue units on the death frame, by creation order.
fn kill(s: &mut BattleState, id: EntityId, child: &str) -> Vec<EntityId> {
    let before: Vec<EntityId> = find_live(s, Team::Blue, child).iter().map(|e| e.id).collect();
    assert!(s.debug_set_hp(id, 0));
    s.tick();
    assert!(s.entity(id).is_none(), "scene: the parent did not die");
    born(s, child, &before)
}

/// Each member's (native position, avoidance offset) on the death frame and the next `ticks - 1` frames.
fn tracks(s: &mut BattleState, ids: &[EntityId], ticks: usize) -> Vec<Vec<((i32, i32), i32)>> {
    let mut t = vec![Vec::new(); ids.len()];
    for k in 0..ticks {
        if k > 0 {
            s.tick();
        }
        for (m, id) in ids.iter().enumerate() {
            let e = s.entity(*id).expect("the member lives");
            t[m].push((native(e.pos), e.avoid_offset));
        }
    }
    t
}

// ---------------------------------------------------------------------------
// (1)

/// The client's tracks of the two halves, 20260920-090204-A (side 0), ticks 1261-1266: the halves on -x and on +x of
/// the ElixirGolem1 that died at (3273, 23116) in its attack on the side-1 princess tower at (3500, 25500).
const CLIENT_LEFT: [(i32, i32); 6] = [(2475, 23152), (2443, 23202), (2415, 23256), (2394, 23313), (2380, 23371), (2375, 23431)];
const CLIENT_RIGHT: [(i32, i32); 6] = [(4083, 23115), (4130, 23151), (4172, 23193), (4208, 23241), (4238, 23293), (4260, 23350)];

/// Plant: first_step_troop_parent_gone.
#[test]
fn the_corpus_split_steps_out_and_turns_in_to_the_native_unit() {
    let mut s = BattleState::new(3, config());
    let g = s.scenario_spawn_now(Team::Blue, "ElixirGolem", Vec2::new(3273 * K, 23116 * K), None).expect("the Elixir Golem");
    for _ in 0..12 {
        s.tick();
    }
    // the corpus Golem died standing in its attack on the princess tower
    let e = s.entity(g).expect("the Elixir Golem lives");
    assert_eq!(native(e.pos), (3273, 23116), "scene: the Elixir Golem moved off the corpus death point");
    let tower = e.target.map(|t| native(s.entity(t).expect("its target lives").pos));
    assert_eq!(tower, Some((3500, 25500)), "scene: the Elixir Golem's target is not the corpus princess tower");
    assert_ne!(e.attack_phase, AttackPhase::Idle, "scene: the Elixir Golem is not in its attack");
    let halves = kill(&mut s, g, "ElixirGolem2");
    assert_eq!(halves.len(), 2, "the ElixirGolem1 leaves two halves");
    let t = tracks(&mut s, &halves, 6);
    let (left, right) = if t[0][0].0 .0 < t[1][0].0 .0 { (&t[0], &t[1]) } else { (&t[1], &t[0]) };
    let pos = |tr: &Vec<((i32, i32), i32)>| tr.iter().map(|f| f.0).collect::<Vec<_>>();
    let off = |tr: &Vec<((i32, i32), i32)>| tr.iter().map(|f| f.1).collect::<Vec<_>>();
    assert_eq!(pos(left), CLIENT_LEFT, "the -x half's first six frames (offsets {:?})", off(left));
    assert_eq!(pos(right), CLIENT_RIGHT, "the +x half's first six frames (offsets {:?})", off(right));
    assert_eq!(off(left), [-190, -180, -170, -160, -150, -140], "the -x half's avoidance offset");
    assert_eq!(off(right), [190, 180, 170, 160, 150, 140], "the +x half's avoidance offset");
}

// ---------------------------------------------------------------------------
// (2)

/// Plant: first_step_troop_parent_gone.
#[test]
fn in_the_open_each_half_steps_out_and_turns_back_in_at_a_full_step() {
    let mut s = BattleState::new(3, config());
    // walking up its lane, far from every building and every other unit
    let g = s.scenario_spawn_now(Team::Blue, "ElixirGolem", Vec2::new(3500 * K, 10000 * K), None).expect("the Elixir Golem");
    for _ in 0..6 {
        s.tick();
    }
    let at = native(s.entity(g).expect("the Elixir Golem walks").pos);
    let f = s.entity(g).unwrap().facing;
    assert!(f.y > 4 * f.x.abs(), "scene: the Elixir Golem faces {f:?}, off the y axis");
    let halves = kill(&mut s, g, "ElixirGolem2");
    assert_eq!(halves.len(), 2, "the ElixirGolem1 leaves two halves");
    let t = tracks(&mut s, &halves, 4);
    for tr in &t {
        let out = if tr[0].0 .0 < at.0 { -1 } else { 1 };
        let what = if out < 0 { "the -x half" } else { "the +x half" };
        // born on the ring at +-750 on x; its first step already points out (the engine without the blocker steps in)
        assert!(out * (tr[0].0 .0 - at.0) > 780, "{what} did not step out on its first frame: {tr:?}");
        let steps: Vec<(i32, i32)> = tr.windows(2).map(|w| (w[1].0 .0 - w[0].0 .0, w[1].0 .1 - w[0].0 .1)).collect();
        for (k, st) in steps.iter().enumerate() {
            assert!(out * st.0 > 0, "{what}'s step {} does not point out: {steps:?}", k + 1);
            let len = isqrt((st.0 as i64).pow(2) + (st.1 as i64).pow(2));
            // ElixirGolem2 walks at Speed 60: a full step is 60 native a tick, whatever the turn
            assert!((58..=62).contains(&len), "{what}'s step {} is not a full step: {steps:?}", k + 1);
        }
        for w in steps.windows(2) {
            assert!(w[1].0.abs() < w[0].0.abs(), "{what}'s x steps do not shrink: {steps:?}");
        }
        let offs: Vec<i32> = tr.iter().map(|f| f.1).collect();
        assert_eq!(offs, [190, 180, 170, 160].map(|o| out * o), "{what}'s avoidance offset (the -x half steers to -x, as in the client)");
    }
}

// ---------------------------------------------------------------------------
// (3)

/// Plant: first_step_troop_parent_gone.
#[test]
fn the_battle_rams_barbarians_read_the_blocker_while_they_deploy() {
    let mut s = BattleState::new(3, config());
    let r = s.scenario_spawn_now(Team::Blue, "BattleRam", Vec2::new(3500 * K, 10000 * K), None).expect("the Battle Ram");
    for _ in 0..6 {
        s.tick();
    }
    let barbs = kill(&mut s, r, "Barbarian");
    assert_eq!(barbs.len(), 2, "the Battle Ram leaves two Barbarians");
    let mut offs: Vec<i32> = barbs.iter().map(|b| s.entity(*b).unwrap().avoid_offset).collect();
    assert!(barbs.iter().all(|b| s.entity(*b).unwrap().deploying), "the Barbarians deploy on their first frame (DeathSpawnDeployTime 1000)");
    offs.sort();
    assert_eq!(offs, [-190, 190], "the Barbarians' first-frame avoidance offsets");
    // the offset decays through the deploy: gone by the first walking frame
    for _ in 0..40 {
        s.tick();
        let walking: Vec<i32> = barbs.iter().filter_map(|b| s.entity(*b)).filter(|e| !e.deploying).map(|e| e.avoid_offset).collect();
        if walking.len() == 2 {
            assert_eq!(walking, [0, 0], "the Barbarians' offset on their first walking frame");
            return;
        }
    }
    panic!("scene: the Barbarians never left their deploy");
}

// ---------------------------------------------------------------------------
// (4)

/// Plant: first_step_blocker_ground.
#[test]
fn a_goblin_giants_riders_spear_goblins_are_not_steered() {
    let mut s = BattleState::new(3, config());
    let g = s.scenario_spawn_now(Team::Blue, "GoblinGiant", Vec2::new(9000 * K, 10000 * K), None).expect("the Goblin Giant");
    for _ in 0..6 {
        s.tick();
    }
    let before: Vec<EntityId> = find_live(&s, Team::Blue, "SpearGoblin").iter().map(|e| e.id).collect();
    assert!(s.debug_set_hp(g, 0));
    s.tick();
    assert!(s.entity(g).is_none(), "scene: the Goblin Giant did not die");
    let goblins = born(&s, "SpearGoblin", &before);
    assert_eq!(goblins.len(), 2, "the two riders come down as Spear Goblins");
    let offs: Vec<i32> = goblins.iter().map(|b| s.entity(*b).unwrap().avoid_offset).collect();
    assert_eq!(offs, [0, 0], "the Spear Goblins' first-frame avoidance offsets");
}
