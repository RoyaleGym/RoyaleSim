//! combat.CORPSE_SWITCH_REACH, read off the engine: under combat.RETARGET_PROGRESS = keep_when_dead(_or_in_reach),
//! whether replacing a dead target keeps a swing under way whatever the new target's range (state.rs `phase_target`,
//! `replaced_a_corpse`).
//!
//! THE LAW, measured on client 15.535.29 and the 16.402 corpus: among kills whose next target was named at once, 194 of
//! 196 in reach kept the swing and 23 of 23 out of reach dropped it (13 of them mid-swing). A Bowler whose boulder
//! killed a Skeleton walked on the next tick after a Knight out of its reach (20260920-081819, 1673 and 1674).
//!
//! The scene is tests/test_corpse_switch_reach.py's: a blue Bowler at (9500, 8000), a red Skeleton at (9500, 12500) and
//! a red Knight at (9500, 14700), with combat.RANGE_PROJECTILE = straight_to_range; the Bowler's first boulder kills the
//! Skeleton mid-swing and its next target is the Knight, out of reach. WHAT IS PINNED, each with its precondition:
//!   1. client_in_reach_only: on the tick the Bowler takes the Knight its swing is dropped (Idle, progress 0), and it
//!      walks;
//!   2. keeps_any: on that tick the swing runs on (the old arm);
//!   3. both values: a princess tower replacing its dead Skeletons by Skeletons in its range keeps its cadence
//!      (tests/test_retarget_cadence.py's swarm: kills at most 17 ticks apart, its HitSpeed 16);
//!   4. the shipped value is client_in_reach_only.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test corpse_switch_reach`):
//!   * `corpse_switch_keeps_any` -- client_in_reach_only still keeps the swing onto a target out of reach: (1) goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, CorpseSwitchReach, RangeProjectile};
use royalesim::{EntityId, Team};

const NEW: CorpseSwitchReach = CorpseSwitchReach::ClientInReachOnly;
const OLD: CorpseSwitchReach = CorpseSwitchReach::KeepsAny;

const BOWLER_AT: (i32, i32) = (9500, 8000);
const SKELETON_AT: (i32, i32) = (9500, 12500);
const KNIGHT_AT: (i32, i32) = (9500, 14700);
const TICKS: u32 = 40;

fn with_arm(arm: CorpseSwitchReach) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.range_projectile = RangeProjectile::StraightToRange;
    cfg.calib.corpse_switch_reach = arm;
    cfg
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The Bowler's reach on `other`, subtiles: Range + both collision radii.
fn reach(s: &BattleState, bowler: EntityId, other: EntityId) -> i64 {
    let (b, o) = (s.entity(bowler).unwrap(), s.entity(other).unwrap());
    s.config().cards.get(b.card_idx).range as i64 + b.radius as i64 + o.radius as i64
}

/// One tick after it ran: the Bowler's target, phase, progress and position; whether the first Skeleton lives; and the
/// start-of-tick centre distance (subtiles) and reach of whatever the Bowler targets after the tick.
struct Row {
    tick: u32,
    target: Option<EntityId>,
    phase: AttackPhase,
    progress: i32,
    pos: Vec2,
    first_alive: bool,
    target_gap: Option<(i64, i64)>,
}

fn run(arm: CorpseSwitchReach) -> (Vec<EntityId>, Vec<Row>) {
    let mut s = BattleState::new(0, with_arm(arm));
    let ids = s
        .scenario_spawn_batch(&[
            (Team::Blue, "Bowler", at(BOWLER_AT), None),
            (Team::Red, "Skeletons", at(SKELETON_AT), None),
            (Team::Red, "Knight", at(KNIGHT_AT), None),
        ])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (bowler, first) = (ids[0], ids[1]);
    let mut rows = Vec::new();
    for tick in 1..=TICKS {
        let before: Vec<(EntityId, Vec2)> = ids.iter().filter_map(|id| s.entity(*id).map(|v| (*id, v.pos))).collect();
        s.tick();
        let Some(b) = s.entity(bowler) else { break };
        // A target killed inside the tick is still named after it (the kill tick): it has no reach to report.
        let target_gap = b.target.and_then(|t| {
            s.entity(t)?;
            let (bp, tp) = (before.iter().find(|p| p.0 == bowler)?.1, before.iter().find(|p| p.0 == t)?.1);
            Some((isqrt(bp.dist2(tp)), reach(&s, bowler, t)))
        });
        rows.push(Row {
            tick,
            target: b.target,
            phase: b.attack_phase,
            progress: b.attack_ms,
            pos: b.pos,
            first_alive: s.entity(first).is_some(),
            target_gap,
        });
    }
    (ids, rows)
}

/// The kill (first row without the first Skeleton) after a launch at least two ticks earlier, and the first row from
/// the kill on that names `next`.
fn switch_row(rows: &[Row], next: EntityId, arm: CorpseSwitchReach) -> (usize, usize) {
    let launch = rows.iter().position(|r| r.phase == AttackPhase::Cooldown).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: no launch"));
    let kill = rows.iter().position(|r| !r.first_alive).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the Skeleton never died"));
    assert!(kill > launch + 1, "{arm:?}: the scene drifted: the kill on {} is not mid-swing (launch {})", rows[kill].tick, rows[launch].tick);
    assert!(rows[..kill].iter().all(|r| r.pos == rows[0].pos), "{arm:?}: the scene drifted: the Bowler moved before the kill");
    let sw = (kill..rows.len()).find(|&t| rows[t].target == Some(next)).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the Bowler never took its next target"));
    (kill, sw)
}

#[test]
fn a_kill_followed_by_a_target_out_of_reach_drops_the_swing() {
    let (ids, rows) = run(NEW);
    let (kill, sw) = switch_row(&rows, ids[2], NEW);
    let (gap, r) = rows[sw].target_gap.expect("the Knight's distance");
    assert!(gap > r, "the scene drifted: the Knight stood {} inside the Bowler's reach on {}", (r - gap) / K as i64, rows[sw].tick);
    assert_eq!(
        (rows[sw].phase, rows[sw].progress),
        (AttackPhase::Idle, 0),
        "the Skeleton died on {} mid-swing and the Bowler took the Knight {} past its reach on {} with its swing running",
        rows[kill].tick,
        (gap - r) / K as i64,
        rows[sw].tick
    );
    let next = rows.get(sw + 1).expect("a tick after the switch");
    assert_ne!(next.pos, rows[sw].pos, "the Bowler did not walk on {}", next.tick);
}

#[test]
fn the_old_value_keeps_the_swing_onto_a_target_out_of_reach() {
    let (ids, rows) = run(OLD);
    let (_, sw) = switch_row(&rows, ids[2], OLD);
    assert_eq!(rows[sw].phase, AttackPhase::Windup, "the old value dropped the swing on {}", rows[sw].tick);
}

#[test]
fn a_tower_replacing_its_victims_in_range_keeps_its_cadence_on_both_values() {
    // The swarm of tests/test_retarget_cadence.py: eight one-shot Skeletons in front of the blue left princess tower.
    let tile = 1000 * K;
    let spots: Vec<Vec2> = (0..8).map(|i| Vec2::new((2 * tile + tile / 2) + i * tile / 2, (8 * tile + tile / 2) + (i % 3) * (2 * tile / 5))).collect();
    for arm in [NEW, OLD] {
        let mut s = BattleState::new(0, with_arm(arm));
        let specs: Vec<(Team, &str, Vec2, Option<i32>)> = spots.iter().map(|p| (Team::Red, "Skeletons", *p, None)).collect();
        let ids = s.scenario_spawn_batch(&specs).unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
        let mut deaths = Vec::new();
        for tick in 1..=200u32 {
            let alive = ids.iter().filter(|id| s.entity(**id).is_some()).count();
            s.tick();
            let now = ids.iter().filter(|id| s.entity(**id).is_some()).count();
            deaths.extend(std::iter::repeat(tick).take(alive - now));
        }
        let gaps: Vec<u32> = deaths.windows(2).map(|w| w[1] - w[0]).skip(2).collect();
        assert!(gaps.len() >= 4, "{arm:?}: the scene drifted: too few kills {deaths:?}");
        assert!(gaps.iter().all(|g| *g <= 17), "{arm:?}: the tower's kills came {gaps:?} apart, slower than its cadence");
    }
}

#[test]
fn the_shipped_value_is_the_new_one() {
    assert_eq!(Calib::shipped().corpse_switch_reach, NEW);
}
