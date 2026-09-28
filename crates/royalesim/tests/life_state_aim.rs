//! spawner.LIFE_STATE_FIRST_LOOK_AIM and spawner.LIFE_STATE_AIM_REPICK, read off the engine: where a Goblin Hut's
//! first look reads its aim (state.rs `note_first_looks`, `life_aim_pos`), and when the hut re-picks the aim its waves
//! hold (`life_repick_lost_aim`). Both ship at their old arms, post_move and on_wave; every test names its arms.
//!
//! THE EVIDENCE, read off the 16.402 corpus with the engine's wave arithmetic (the ledger has the rows):
//!   - a first look on ActionDelay's end reads the aim's START-OF-TICK position: 5 first waves fit it only, none the
//!     post-move position only, 8 both; a hut waking from sleep reads the post-move one (3 only it, none the other);
//!   - a hut re-picks on the tick its aim dies or leaves its reach, and holds the new aim: one hut in one battle, whose
//!     next three waves fit only that rule.
//!
//! The scenes put a blue hut at (9000, 12500), north of its princess towers' reach, with red enemies north of it. A
//! wave's point is checked against the two candidate points of an aim (the line to it scaled to SpawnOffset, then
//! turned by +-SingleDeployOffsetAngle with the 1024 table, each step truncated toward zero:
//! spawner.LIFE_STATE_WAVE_POINT's shipped arithmetic), never against a copied number. WHAT IS PINNED:
//!   1. start_of_tick_on_first_wave: the first look's wave stands on a candidate of its aim's start-of-tick position
//!      (a Knight walking in reach), not of its post-move one; post_move: the other way round;
//!   2. start_of_tick_on_first_wave: a hut that slept and wakes to a walking Knight reads its post-move position;
//!   3. on_aim_lost: the aim dies between waves while a Cannon A is the nearest; a Cannon B is then moved nearer; the
//!      next two waves stand on A's candidates. on_wave: the next wave stands on B's;
//!   4. both keys ship at their old arms.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test life_state_aim`):
//!   * `life_first_look_post_move` -- the new arm reads the post-move position too: (1) goes red.
//!   * `life_repick_on_wave_only` -- the new arm re-picks only when a wave is due: (3) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::formation::sin1024;
use royalesim::state::{BattleConfig, BattleState, Calib, LifeStateAimRepick, LifeStateFirstLookAim};
use royalesim::{EntityId, Team};

const HUT_AT: (i32, i32) = (9000, 12500);

/// A point in native units.
type Point = (i64, i64);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn native(p: Vec2) -> (i64, i64) {
    ((p.x / K) as i64, (p.y / K) as i64)
}

fn with_arms(first: LifeStateFirstLookAim, repick: LifeStateAimRepick) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.life_state_first_look_aim = first;
    cfg.calib.life_state_aim_repick = repick;
    cfg
}

/// The two candidate points of a wave of the hut at `hut` aimed at `aim` (native), by the hut's own SpawnOffset and
/// SingleDeployOffsetAngle.
fn candidates(s: &BattleState, hut: (i64, i64), aim: (i64, i64)) -> [(i64, i64); 2] {
    let ls = card_stat(s, "GoblinHut").life_state.expect("the hut carries its controller");
    let off = (ls.offset / K) as i64;
    let (c, sn) = (sin1024(ls.offset_angle_deg + 90) as i64, sin1024(ls.offset_angle_deg) as i64);
    let (dx, dy) = (aim.0 - hut.0, aim.1 - hut.1);
    let n = isqrt(dx * dx + dy * dy).max(1);
    let (ux, uy) = (off * dx / n, off * dy / n);
    [(hut.0 + (ux * c - uy * sn) / 1024, hut.1 + (ux * sn + uy * c) / 1024), (hut.0 + (ux * c + uy * sn) / 1024, hut.1 + (uy * c - ux * sn) / 1024)]
}

/// One run: per tick k after the hut's play (k = 0 its creation tick), the watched enemies' positions after the tick,
/// and the waves as (k, point).
struct Run {
    s: BattleState,
    hut: (i64, i64),
    after: Vec<Vec<(i64, i64)>>,
    waves: Vec<(u32, (i64, i64))>,
}

/// A blue hut at HUT_AT, the red `enemies` set down first; `each(k, s)` runs before tick k.
fn run(cfg: BattleConfig, enemies: &[(&str, (i32, i32))], ticks: u32, mut each: impl FnMut(u32, &mut BattleState, &[EntityId])) -> Run {
    let mut s = BattleState::new(0, cfg);
    let ids: Vec<EntityId> = enemies.iter().map(|(card, p)| s.scenario_spawn_now(Team::Red, card, at(*p), None).expect("spawn an enemy")).collect();
    s.spawn_unit(Team::Blue, "GoblinHut", at(HUT_AT), None).expect("play the hut");
    let mut hut = None;
    let mut seen: Vec<EntityId> = Vec::new();
    let (mut after, mut waves) = (Vec::new(), Vec::new());
    for k in 0..ticks {
        each(k, &mut s, &ids);
        s.tick();
        // The play is created in the first tick's Spawn phase.
        hut = hut.or_else(|| s.entities().find(|v| v.card == "GoblinHut").map(|v| native(v.pos)));
        after.push(ids.iter().map(|id| s.entities().find(|v| v.id == *id).map_or((i64::MIN, i64::MIN), |v| native(v.pos))).collect());
        for v in s.entities().filter(|v| v.card == "SpearGoblin_Dummy") {
            if !seen.contains(&v.id) {
                seen.push(v.id);
                waves.push((k, native(v.pos)));
            }
        }
    }
    Run { s, hut: hut.expect("the hut stands"), after, waves }
}

/// A red Knight walking in the hut's reach when its first look comes: the look's wave, the Knight's start-of-tick
/// and post-move positions on the look tick.
fn first_look(first: LifeStateFirstLookAim) -> (Run, Point, Point, Point) {
    let r = run(with_arms(first, LifeStateAimRepick::OnWave), &[("Knight", (12000, 18000))], 40, |_, _, _| {});
    let &(k, wave) = r.waves.first().expect("the scene drifted: no first look's wave");
    assert_eq!(k, 20, "the scene drifted: the first wave came on {k}, not on the first look");
    let (start, moved) = (r.after[k as usize - 1][0], r.after[k as usize][0]);
    (r, wave, start, moved)
}

/// Plant: life_first_look_post_move.
#[test]
fn the_first_look_reads_the_start_of_tick_position_under_the_new_arm() {
    let (r, wave, start, moved) = first_look(LifeStateFirstLookAim::StartOfTickOnFirstWave);
    let (from_start, from_moved) = (candidates(&r.s, r.hut, start), candidates(&r.s, r.hut, moved));
    assert!(from_start.iter().all(|p| !from_moved.contains(p)), "vacuous: the Knight's two positions {start:?} and {moved:?} give a shared point");
    assert!(from_start.contains(&wave), "start_of_tick_on_first_wave: the first wave on {wave:?}, not on {from_start:?} (the Knight's start-of-tick {start:?}); post-move {from_moved:?}");
}

#[test]
fn the_first_look_reads_the_post_move_position_under_the_old_arm() {
    let (r, wave, _, moved) = first_look(LifeStateFirstLookAim::PostMove);
    let from_moved = candidates(&r.s, r.hut, moved);
    assert!(from_moved.contains(&wave), "post_move: the first wave on {wave:?}, not on {from_moved:?} (the Knight's post-move {moved:?})");
}

#[test]
fn a_wake_from_sleep_reads_the_post_move_position_under_the_new_arm() {
    // Nobody in reach on the first look (the hut sleeps); the Knight walks to the right bridge and into the reach.
    let r = run(with_arms(LifeStateFirstLookAim::StartOfTickOnFirstWave, LifeStateAimRepick::OnWave), &[("Knight", (10000, 22000))], 300, |_, _, _| {});
    let &(k, wave) = r.waves.first().expect("the scene drifted: the Knight never woke the hut");
    assert!(k > 21, "the scene drifted: the first wave on {k}, the first look's tick (the hut did not sleep)");
    let (start, moved) = (r.after[k as usize - 1][0], r.after[k as usize][0]);
    let (from_start, from_moved) = (candidates(&r.s, r.hut, start), candidates(&r.s, r.hut, moved));
    assert!(from_start.iter().all(|p| !from_moved.contains(p)), "vacuous: the Knight's two positions {start:?} and {moved:?} give a shared point");
    assert!(from_moved.contains(&wave), "a wake from sleep: the wave of {k} on {wave:?}, not on {from_moved:?} (the Knight's post-move {moved:?})");
}

/// Cannons north of the hut, out of their own range of it (centre more than 7100 away) and within the buildings'
/// wake reach (centre - 600 at most 7228): X (edge 6600) is the first aim; A (edge about 6862) is the nearest when X
/// dies before tick 30; B (edge about 7055) is moved to edge about 6778 before tick 40, nearer than A.
const X_AT: (i32, i32) = (9000, 19700);
const A_AT: (i32, i32) = (13000, 18800);
const B_AT: (i32, i32) = (4800, 18900);
const B_MOVED: (i32, i32) = (5000, 18700);

fn lost_aim(repick: LifeStateAimRepick) -> Run {
    run(with_arms(LifeStateFirstLookAim::PostMove, repick), &[("Cannon", X_AT), ("Cannon", A_AT), ("Cannon", B_AT)], 110, |k, s, ids| {
        if k == 30 {
            assert!(s.debug_set_hp(ids[0], 0), "the scene drifted: X is gone before tick 30");
        }
        if k == 40 {
            assert!(s.debug_set_pos(ids[2], at(B_MOVED)), "the scene drifted: B is gone before tick 40");
        }
    })
}

/// Plant: life_repick_on_wave_only.
#[test]
fn a_lost_aim_is_re_picked_on_the_tick_and_held_under_the_new_arm() {
    let r = lost_aim(LifeStateAimRepick::OnAimLost);
    assert_eq!(r.after[30][0], (i64::MIN, i64::MIN), "vacuous: X did not die on tick 30");
    let ticks: Vec<u32> = r.waves.iter().map(|w| w.0).collect();
    assert_eq!(ticks.get(..3), Some(&[20, 63, 107][..]), "the scene drifted: the wave ticks {ticks:?}");
    // Where the Cannons stand (a building's point may be snapped where it is set down): X before it dies, A and B
    // after B's move.
    let (x, a, b) = (candidates(&r.s, r.hut, r.after[20][0]), candidates(&r.s, r.hut, r.after[62][1]), candidates(&r.s, r.hut, r.after[62][2]));
    assert!(x.contains(&r.waves[0].1), "the first wave on {:?}, not on X's {x:?}", r.waves[0].1);
    assert!(a.iter().all(|p| !b.contains(p)), "vacuous: A and B give a shared point");
    for w in &r.waves[1..3] {
        assert!(a.contains(&w.1), "on_aim_lost: the wave of {} on {:?}, not on A's {a:?} (B's {b:?})", w.0, w.1);
    }
}

#[test]
fn a_lost_aim_is_re_picked_when_the_wave_is_due_under_the_old_arm() {
    let r = lost_aim(LifeStateAimRepick::OnWave);
    let b = candidates(&r.s, r.hut, r.after[62][2]);
    let w = r.waves.get(1).expect("the scene drifted: no second wave");
    assert_eq!(w.0, 63, "the scene drifted: the second wave on {}", w.0);
    assert!(b.contains(&w.1), "on_wave: the wave of 63 on {:?}, not on B's {b:?}, the nearest then", w.1);
}

#[test]
fn both_keys_ship_at_their_old_arms() {
    let c = Calib::shipped();
    assert_eq!(c.life_state_first_look_aim, LifeStateFirstLookAim::PostMove, "post_move ships (a hypothesis, scored by override)");
    assert_eq!(c.life_state_aim_repick, LifeStateAimRepick::OnWave, "on_wave ships (one hut in one battle)");
}
