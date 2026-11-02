//! combat.STRAIGHT_SHOT_RANGE_END (combat.rs `step_straight`): whether a one-way straight shot tests the step that reaches its
//! range point.
//!
//! THE LAW, measured on client 16.402 (parity r80, the r76 census re-checked on 0.1.42, live13): the crown towers that only the
//! last segment of a Magic Archer arrow reaches (10800 .. 11000 from the archer) were hit 9 of 9 times, on the fire + 9; the
//! engine's `gone_before_aim`, whose arrow is gone on the step that would reach its aim, hit 1 of the 8 traced.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test straight_shot_range_end`):
//!   * `straight_aim_untested` -- the new arm's shot is still gone untested: the client16402_aim_point_tested test red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, StraightShotRangeEnd};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// A Blue Magic Archer held at ARCHER firing along +x at a Red Knight held 6000 from it, on dry ground short of the river with
/// Blue's princess towers down (its king wakes; its reach stops short of y 12000): no tower reaches the line.
const ARCHER: (i32, i32) = (1000, 13000);
const KNIGHT: (i32, i32) = (7000, 13000);

/// Under combat.STRAIGHT_SHOT_RANGE_END = `arm`, the scene with a Red unit `extra` held at `at` (if any) on 10,000 hp (an arrow
/// would kill it at its own): the archer's arrows' (last point, aim) as each left the board, and `extra`'s hp lost over 100 ticks.
fn run(arm: StraightShotRangeEnd, extra: Option<(&str, (i32, i32))>) -> (Vec<(Vec2, Vec2)>, i32) {
    let mut cfg = config();
    cfg.calib.straight_shot_range_end = arm;
    let mut s = BattleState::new(0, cfg);
    for t in s.tower_ids(Team::Blue).iter().skip(1).flatten().copied().collect::<Vec<_>>() {
        assert!(s.debug_set_hp(t, 0), "a Blue princess tower down");
    }
    s.tick();
    s.tick();
    let ma = s.scenario_spawn_now(Team::Blue, "EliteArcher", n(ARCHER.0, ARCHER.1), None).expect("the Magic Archer");
    let kn = s.scenario_spawn_now(Team::Red, "Knight", n(KNIGHT.0, KNIGHT.1), None).expect("the Knight");
    let ex: Option<(EntityId, Vec2)> = extra.map(|(c, p)| (s.scenario_spawn_now(Team::Red, c, n(p.0, p.1), None).expect("the extra"), n(p.0, p.1)));
    let mut last: Vec<(Vec2, Vec2)> = Vec::new();
    let mut seen: Vec<(usize, Vec2, Vec2)> = Vec::new();
    let mut lost = 0;
    for _ in 0..100 {
        assert!(s.debug_set_pos(ma, n(ARCHER.0, ARCHER.1)) && s.debug_set_hp(ma, 10_000));
        let full = s.entity(kn).expect("the Knight").max_hp;
        assert!(s.debug_set_pos(kn, n(KNIGHT.0, KNIGHT.1)) && s.debug_set_hp(kn, full));
        let before = ex.map(|(id, p)| {
            assert!(s.debug_set_pos(id, p) && s.debug_set_hp(id, 10_000), "the extra held");
            10_000
        });
        s.tick();
        if let (Some(b), Some((id, _))) = (before, ex) {
            lost += b - s.entity(id).map_or(0, |v| v.hp);
        }
        let now: Vec<(usize, Vec2, Vec2)> = s.projectiles().iter().enumerate().filter(|(_, q)| q.firer == Some(ma)).map(|(k, q)| (k, q.pos, q.aim)).collect();
        for &(_, p, a) in &seen {
            if !now.iter().any(|&(_, _, a2)| a2 == a) {
                last.push((p, a));
            }
        }
        seen = now;
    }
    (last, lost)
}

/// combat.STRAIGHT_SHOT_RANGE_END = client16402_aim_point_tested (parity r80, live13: 9 of 9 tower hits only the last step of
/// the arrow reaches). A probe reads the old arm's last tested point P and the aim A; a Red Skeleton (radius 500) is then held
/// 100 beyond P's reach on the line, within A's. Plant: straight_aim_untested.
#[test]
fn under_client16402_aim_point_tested_the_arrow_hits_a_unit_at_its_range_point() {
    let (ends, _) = run(StraightShotRangeEnd::GoneBeforeAim, None);
    let &(p, a) = ends.first().expect("the scene drifted: no arrow left the board");
    assert_eq!((p.y, a.y), (ARCHER.1 * K, ARCHER.1 * K), "the scene drifted: the arrow left its row");
    let (px, ax) = (p.x / K, a.x / K);
    let reach = 250 + 500;
    let x = px + reach + 100;
    assert!(x <= ax + reach - 50, "the scene drifted: the last tested point {px} is too near the aim {ax}");
    let (_, new) = run(StraightShotRangeEnd::Client16402AimPointTested, Some(("Skeleton", (x, ARCHER.1))));
    assert!(new > 0, "client16402_aim_point_tested: the arrow's aim point did not reach the Skeleton at {x} (aim {ax})");
    // NOT VACUOUS: gone_before_aim's arrow is gone before it reaches the Skeleton
    let (_, old) = run(StraightShotRangeEnd::GoneBeforeAim, Some(("Skeleton", (x, ARCHER.1))));
    assert_eq!(old, 0, "gone_before_aim: the arrow reached the Skeleton at {x} (last point {px})");
}
