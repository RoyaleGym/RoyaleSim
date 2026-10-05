//! combat.TIMED_JUMP_BLOW_STOP: whether a timed jump (a dash with DashConstantTime: the Mega Knight's) moves on past its
//! blow (state.rs `phase_path16402_for`, the dash's DashConstantTime arm).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, every Mega Knight jump in the captures (6): steps of 250,
//! the blow on the entry + 16, and no move after it in any. sp-form-MegaKnight-evo-s0 t1025: a goal 4,591 off, beyond 17
//! steps; the blow on t1041 at (11608, 15914) over the river, the jumper put on land on (11358, 14664) that tick and
//! standing to t1044, where the engine flew on to (11749, 16249) and landed on t1045.
//!
//! THE SCENE (tests/dash_attack.rs's): a Blue Mega Knight at (9500, 12500), a Red Giant hitting the blue right princess
//! tower at (14731, 9439). On the tick of the jump's entry (the trigger + 17) the Giant is first moved 700 farther along
//! the line from the Mega Knight, so the jump's goal lies about 600 past 17 steps of 250. WHAT IS PINNED:
//!   1. both arms: 17 moves of 240 to 250 from the entry, the last on the blow (the entry + 16);
//!   2. client15535_stops_at_blow: no move on the entry + 17 to + 19; flies_to_goal (the engine's, the vacuity check)
//!      moves on the entry + 17.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="timed_jump_flies_past_blow"' CARGO_TARGET_DIR=target/plant cargo test --test
//! timed_jump_blow`): the new arm flies on to the goal: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, TimedJumpBlowStop};
use royalesim::Team;

const MK_AT: (i32, i32) = (9500, 12500);
const GIANT_AT: (i32, i32) = (14731, 9439);
/// DashMaxRange 5000 + the Giant's 750, and the entry the trigger + 17 (tests/dash_attack.rs).
const MK_TRIGGER: i64 = 5750;
const MK_ENTRY: u32 = 17;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

fn dist(a: Vec2, b: Vec2) -> i64 {
    let (dx, dy) = ((a.x / K - b.x / K) as i64, (a.y / K - b.y / K) as i64);
    isqrt(dx * dx + dy * dy)
}

/// The Mega Knight's moves from the jump's entry, 21 ticks, under `arm`.
fn jump(arm: TimedJumpBlowStop) -> Vec<i64> {
    let mut cfg = config();
    cfg.calib.timed_jump_blow_stop = arm;
    let mut s = BattleState::new(0, cfg);
    let ids = s
        .scenario_spawn_batch(&[(Team::Red, "Giant", at(GIANT_AT), None), (Team::Blue, "MegaKnight", at(MK_AT), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (g, mk) = (ids[0], ids[1]);
    // the trigger: the first tick whose start-of-tick centre distance is within it
    let mut triggered = false;
    for _ in 0..120 {
        let d = dist(s.entity(mk).expect("the Mega Knight").pos, s.entity(g).expect("the Giant").pos);
        s.tick();
        if d <= MK_TRIGGER {
            triggered = true;
            break;
        }
    }
    assert!(triggered, "{arm:?}: the scene drifted: the Mega Knight never came within its trigger");
    for _ in 1..MK_ENTRY {
        s.tick();
    }
    // the entry's tick: the Giant 700 farther along the line, so the goal lies past 17 steps
    let (m, gp) = (s.entity(mk).unwrap().pos, s.entity(g).unwrap().pos);
    let n = dist(m, gp);
    let (dx, dy) = ((gp.x / K - m.x / K) as i64, (gp.y / K - m.y / K) as i64);
    let far = Vec2::new(gp.x + (dx * 700 / n) as i32 * K, gp.y + (dy * 700 / n) as i32 * K);
    assert!(s.debug_set_pos(g, far));
    assert!(n + 700 - 1500 > 17 * 250 + 200, "{arm:?}: the scene drifted: the goal ({} off) is within 17 steps", n + 700 - 1500);
    let mut prev = s.entity(mk).unwrap().pos;
    (0..21)
        .map(|_| {
            s.tick();
            let p = s.entity(mk).expect("the Mega Knight lives").pos;
            let step = dist(prev, p);
            prev = p;
            step
        })
        .collect()
}

/// Plant: timed_jump_flies_past_blow.
#[test]
fn a_timed_jump_stops_at_its_blow_under_client15535_stops_at_blow() {
    let old = jump(TimedJumpBlowStop::FliesToGoal);
    assert!(old[..17].iter().all(|x| (240..=250).contains(x)), "flies_to_goal: the jump's first 17 moves: {old:?}");
    // NOT VACUOUS: the engine's arm flies on past the blow.
    assert!(old[17] > 0, "flies_to_goal: the jump stood on the entry + 17: {old:?}");
    let new = jump(TimedJumpBlowStop::Client15535StopsAtBlow);
    assert!(new[..17].iter().all(|x| (240..=250).contains(x)), "client15535_stops_at_blow: the jump's first 17 moves: {new:?}");
    assert_eq!(&new[17..20], &[0, 0, 0], "client15535_stops_at_blow: the jumper moved past its blow: {new:?}");
}
