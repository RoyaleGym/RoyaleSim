//! A UNIT SERVING ITS POST-KILL WAIT DOES NOT STEER ITS NEIGHBOURS BY HEADING, on every tick of the wait (state.rs
//! `serves_no_wait`, read by move16402.rs `Body::heading_counts`).
//!
//! THE LAW: the avoidance scan (move16402.rs `avoidance_scan`) skips a moving neighbour heading the same way as the
//! walker (a positive dot product of the two facings), except that states 8/0/2/10 zero a neighbour's dot product, so a
//! neighbour that is not walking blocks whatever its facing. combat.POST_KILL_RETARGET_WAIT's unit keeps its attacking
//! state (2) with no target through the wait and walks (1) only with its next target.
//!
//! MEASURED on client 15.535.29 (sp-hogs-musk-s0): four Royal Hogs lose their Cannon on t281 and read state 2 with no
//! target through t286, walking with the princess tower on t287; a Skeleton whose look circle meets a Hog on t286 turns
//! +190 there ((+90, -11) in the client). The engine zeroed the wait's attack timer on t286 by setting its phase Idle, and
//! the Skeleton walked straight on (the scene's first divergence).
//!
//! THE SCENE: a Blue Knight at (9000, 12500), out of every crown tower's reach, kills a Red Skeleton 900 north of it and
//! serves its wait; a Red Giant is held at (9000, 14800), so that the Knight's facing stays north and its next target lies
//! north. A second Blue Knight, the walker, is put down at (8800, 10500) on tick `d` and walks north after the Skeleton,
//! then the Giant: the same way the waiting Knight faces. The sweep over `d` lands the first tick on which the walker's
//! look circle meets the waiting Knight (from the start-of-tick positions, as the scan reads them) on each tick of the
//! wait, the last one included.
//!
//! WHAT IS PINNED: on that first tick, inside the wait, the walker's avoidance offset turns (+-190 after the tick's decay),
//! the waiting Knight being the only body in its look circle; the sweep reaches the wait's last tick and an earlier one.
//!
//! PLANTS (regression):
//!   - waiting_heading_counts -> `a_waiting_unit_blocks_a_walker_on_every_tick_of_its_wait` red (the wait's last tick).
//!     RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//!     post_kill_wait_heading
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

const KNIGHT: (i32, i32) = (9000, 12500);
const WALKER: (i32, i32) = (8800, 10500);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// One tick of the scene, after the tick: the waiting Knight's target, whether the walker's look circle met the Knight at
/// the tick's start (and nothing else), and the walker's avoidance offset after the tick.
#[derive(Clone, Copy, Debug)]
struct Row {
    target: Option<EntityId>,
    met: Option<bool>,
    offset: i32,
}

/// Whether the look circle of `w` (its position plus its facing, radius min(R, 500), native) overlaps the body of `o`.
fn looks_at(s: &BattleState, w: EntityId, o: EntityId) -> bool {
    let (Some(a), Some(b)) = (s.entity(w), s.entity(o)) else { return false };
    let look = (a.pos.x / K + a.facing.x, a.pos.y / K + a.facing.y);
    let r = (a.radius / K).min(500) + b.radius / K;
    let (dx, dy) = ((b.pos.x / K - look.0) as i64, (b.pos.y / K - look.1) as i64);
    dx * dx + dy * dy < (r as i64) * (r as i64)
}

/// The scene with the walker put down on tick `d`; also the tick of the Skeleton's death (L, the first tick after which it
/// is gone).
fn scene(d: usize) -> (Vec<Row>, usize) {
    let mut s = BattleState::new(0, config());
    let k = s.scenario_spawn_now(Team::Blue, "Knight", at(KNIGHT), None).expect("the Knight");
    let sk = s.scenario_spawn_now(Team::Red, "Skeletons", at((KNIGHT.0, KNIGHT.1 + 900)), None).expect("the Skeleton");
    let g = s.scenario_spawn_now(Team::Red, "Giant", at((9000, 14800)), None).expect("the Giant");
    let mut w = None;
    let mut rows = Vec::new();
    let mut dead = None;
    for t in 0..90 {
        if t == d {
            w = Some(s.scenario_spawn_now(Team::Blue, "Knight", at(WALKER), None).expect("the walker"));
        }
        s.debug_set_pos(g, at((9000, 14800)));
        // the walker's look circle at the tick's start: the Knight, and nothing else
        let met = w.map(|w| looks_at(&s, w, k) && !looks_at(&s, w, sk) && !looks_at(&s, w, g));
        s.tick();
        if dead.is_none() && s.entity(sk).is_none() {
            dead = Some(t);
        }
        let kv = s.entity(k).expect("the scene drifted: the Knight died");
        let offset = w.and_then(|w| s.entity(w)).map_or(0, |v| v.avoid_offset);
        rows.push(Row { target: kv.target, met, offset });
    }
    (rows, dead.expect("the scene drifted: the Knight never killed the Skeleton"))
}

#[test]
fn a_waiting_unit_blocks_a_walker_on_every_tick_of_its_wait() {
    let mut reached = Vec::new();
    for d in 0..40 {
        let (rows, l) = scene(d);
        // the wait: no target on L + 1 .. L + 5, the next one (the Giant) on L + 6
        for (j, r) in rows.iter().enumerate().take(l + 6).skip(l + 1) {
            assert_eq!(r.target, None, "d {d}: the scene drifted: the Knight holds a target on {j} inside its wait (L = {l})");
        }
        assert!(rows[l + 6].target.is_some(), "d {d}: the scene drifted: no next target on L + 6 = {}", l + 6);
        let Some(t0) = rows.iter().position(|r| r.met == Some(true)) else { continue };
        if !(l + 1..=l + 5).contains(&t0) {
            continue;
        }
        assert!(rows[..t0].iter().all(|r| r.offset == 0), "d {d}: the scene drifted: the walker steered before its look circle met the Knight on {t0}");
        assert_eq!(rows[t0].offset.abs(), 190, "d {d}: the walker's look circle meets the waiting Knight on {t0} (L + {}), \
             its wait's tick {}: it blocks it, as a unit in state 2 does", t0 - l, t0 - l);
        reached.push(t0 - l);
    }
    assert!(reached.contains(&5), "the sweep never met the Knight on its wait's last tick (L + 5): {reached:?}");
    assert!(reached.iter().any(|x| *x < 5), "the sweep never met the Knight earlier in its wait: {reached:?}");
}
