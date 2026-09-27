//! targeting.CHASE_DROP_KNOCKED_TARGET, read off the engine: whether a troop holds a troop target that slides under a
//! knockback past the chase-drop limit (targeting.CHASE_DROP_RANGE = client_sight_minus_1000; target.rs `decide`).
//!
//! THE LAW, measured on the 16.402 corpus: 3 of 3 pushes that carried a held troop across the chase limit left the
//! holder holding it, all three by the holder's own boulder. In 20260920-081051 a Bowler attacking a Bomber pushed it
//! from 5,499 to 5,767 (limit 5,750) with its boulder, kept it through the slide and after, stood, and launched its
//! next boulder one HitSpeed after the first (ticks 928 and 978). Under client_holds_knocked the sliding target is
//! held through the slide (neither the chase drop nor a rescan lets it go); after it, the rules for a target past the
//! limit apply. A push by anything else is inferred and not pinned here.
//!
//! The scenes are tests/test_chase_drop_knocked.py's: a blue Bowler at (14735, 17126) and a red Bomber at (13669,
//! 22260), that battle's geometry, with combat.RANGE_PROJECTILE = straight_to_range (the boulder pushes only there);
//! and, as the control, a red Knight at (14500, 12500) and a blue Hog Rider at (14500, 9500) that outruns it up the
//! lane. WHAT IS PINNED, each with its precondition:
//!   1. client_holds_knocked: the Bomber crosses the limit while sliding, and from its first boulder to its second the
//!      Bowler holds the Bomber, stands, and launches the second one HitSpeed / 50 ticks after the first;
//!   2. drops_knocked: on that crossing the Bowler lets the Bomber go (today's engine);
//!   3. both values: the Hog Rider, never pushed, is let go on the tick it crosses the Knight's limit;
//!   4. the shipped value is drops_knocked.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test chase_drop_knocked`):
//!   * `chase_drop_knocked_dropped` -- client_holds_knocked still lets a sliding target go: (1) goes red.
mod common;

use common::*;
use royalesim::entity::AttackPhase;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, ChaseDropKnocked, ChaseDropRange, RangeProjectile};
use royalesim::{EntityId, Team};

const NEW: ChaseDropKnocked = ChaseDropKnocked::ClientHoldsKnocked;
const OLD: ChaseDropKnocked = ChaseDropKnocked::DropsKnocked;

const BOWLER_AT: (i32, i32) = (14735, 17126);
const BOMBER_AT: (i32, i32) = (13669, 22260);
const KNIGHT_AT: (i32, i32) = (14500, 12500);
const HOG_AT: (i32, i32) = (14500, 9500);
/// targeting.CHASE_DROP_RANGE's CHASE_DROP_SHORT_OF_SIGHT, native.
const SHORT_OF_SIGHT: i64 = 1000;

fn with_arm(arm: ChaseDropKnocked) -> BattleConfig {
    let mut cfg = config();
    cfg.calib.chase_drop_range = ChaseDropRange::ClientSightMinus1000;
    cfg.calib.range_projectile = RangeProjectile::StraightToRange;
    cfg.calib.chase_drop_knocked = arm;
    cfg
}

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// The chase-drop limit of `holder` on `other`, subtiles: SightRange + both collision radii - 1000 native.
fn limit(s: &BattleState, holder: EntityId, other: EntityId) -> i64 {
    let (h, o) = (s.entity(holder).unwrap(), s.entity(other).unwrap());
    let sight = s.config().cards.get(h.card_idx).sight_range as i64;
    sight + h.radius as i64 + o.radius as i64 - SHORT_OF_SIGHT * K as i64
}

/// One tick: the start-of-tick max(|dx|, |dy|) of the two centres (subtiles), and after the tick the holder's target,
/// position and attack phase, and whether the other unit is sliding.
struct Row {
    tick: u32,
    m: i64,
    target: Option<EntityId>,
    pos: Vec2,
    phase: AttackPhase,
    other_sliding: bool,
}

fn run(arm: ChaseDropKnocked, holder: (Team, &str, (i32, i32)), other: (Team, &str, (i32, i32)), ticks: u32) -> (EntityId, i64, Vec<Row>) {
    let mut s = BattleState::new(0, with_arm(arm));
    let ids = s
        .scenario_spawn_batch(&[(holder.0, holder.1, at(holder.2), None), (other.0, other.1, at(other.2), None)])
        .unwrap_or_else(|(k, e)| panic!("spawn {k}: {e:?}"));
    let (h, o) = (ids[0], ids[1]);
    let lim = limit(&s, h, o);
    let mut rows = Vec::new();
    for tick in 1..=ticks {
        let (Some(hv), Some(ov)) = (s.entity(h), s.entity(o)) else { break };
        let d = ov.pos.sub(hv.pos);
        let m = (d.x as i64).abs().max((d.y as i64).abs());
        s.tick();
        let (Some(hv), Some(ov)) = (s.entity(h), s.entity(o)) else { break };
        rows.push(Row { tick, m, target: hv.target, pos: hv.pos, phase: hv.attack_phase, other_sliding: ov.push_active || ov.knock_ms > 0 });
    }
    (o, lim, rows)
}

fn bowler(arm: ChaseDropKnocked) -> (EntityId, i64, Vec<Row>) {
    run(arm, (Team::Blue, "Bowler", BOWLER_AT), (Team::Red, "Bomber", BOMBER_AT), 70)
}

/// The first row whose start-of-tick distance is past `lim` after one within it.
fn crossing(rows: &[Row], lim: i64) -> Option<usize> {
    (1..rows.len()).find(|&i| rows[i - 1].m <= lim && rows[i].m > lim)
}

#[test]
fn a_target_pushed_past_the_limit_is_kept() {
    let (bomber, lim, rows) = bowler(NEW);
    let i = crossing(&rows, lim).unwrap_or_else(|| panic!("the scene drifted: the Bomber never crossed the Bowler's limit {}", lim / K as i64));
    assert!(rows[i].other_sliding, "the scene drifted: the Bomber crossed the limit on {} without sliding", rows[i].tick);
    let launches: Vec<usize> = (0..rows.len()).filter(|&t| rows[t].phase == AttackPhase::Cooldown).collect();
    let first = *launches.first().expect("the scene drifted: the Bowler never launched");
    assert!(first < i, "the scene drifted: no boulder before the crossing on {}", rows[i].tick);
    assert!(rows[first..i].iter().all(|r| r.target == Some(bomber)), "the scene drifted: the Bowler lost the Bomber before the crossing");
    let second = launches.iter().copied().find(|&t| t > first);
    let end = second.unwrap_or(rows.len() - 1);
    let let_go: Vec<u32> = rows[i..=end].iter().filter(|r| r.target != Some(bomber)).map(|r| r.tick).collect();
    assert!(
        let_go.is_empty(),
        "the Bomber slid past the limit on {} (start-of-tick {} > {} native) and the Bowler let it go on {let_go:?}",
        rows[i].tick,
        rows[i].m / K as i64,
        lim / K as i64
    );
    let moved: Vec<u32> = rows[first..=end].iter().filter(|r| r.pos != rows[first].pos).map(|r| r.tick).collect();
    assert!(moved.is_empty(), "the Bowler walked on {moved:?} while it held the Bomber");
    let cards = cards();
    let hit_speed = cards.get(cards.index("Bowler").expect("Bowler in cards.json")).hit_speed_ms as usize / 50;
    assert_eq!(
        second.map(|t| rows[t].tick),
        Some(rows[first].tick + hit_speed as u32),
        "the Bowler's boulders came on {} and {:?}, not one HitSpeed apart",
        rows[first].tick,
        second.map(|t| rows[t].tick)
    );
}

#[test]
fn the_old_value_lets_the_sliding_target_go() {
    let (bomber, lim, rows) = bowler(OLD);
    let i = crossing(&rows, lim).expect("the scene drifted: the Bomber never crossed the limit");
    assert!(rows[i].other_sliding, "the scene drifted: the crossing on {} is not a slide", rows[i].tick);
    assert_ne!(rows[i].target, Some(bomber), "the old value kept the sliding Bomber on {}", rows[i].tick);
}

#[test]
fn a_runner_that_is_not_sliding_is_let_go_on_the_edge() {
    for arm in [NEW, OLD] {
        let (hog, lim, rows) = run(arm, (Team::Red, "Knight", KNIGHT_AT), (Team::Blue, "HogRider", HOG_AT), 120);
        let i = crossing(&rows, lim).unwrap_or_else(|| panic!("{arm:?}: the scene drifted: the Hog Rider never crossed {}", lim / K as i64));
        assert!(rows.iter().all(|r| !r.other_sliding), "{arm:?}: the scene drifted: the Hog Rider was pushed");
        assert_eq!(rows[i - 1].target, Some(hog), "{arm:?}: the scene drifted: the Knight did not hold the Hog Rider");
        assert_ne!(rows[i].target, Some(hog), "{arm:?}: the Hog Rider crossed the limit on {} and the Knight kept it", rows[i].tick);
    }
}

#[test]
fn the_shipped_value_is_the_old_one() {
    assert_eq!(Calib::shipped().chase_drop_knocked, OLD);
}
