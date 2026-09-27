//! targeting.VARIABLE_DAMAGE_WALK_REACH, read off the engine: the reach a walking Inferno Dragon walks to and stops at.
//!
//! THE LAW, measured on the 16.402 corpus and on client 15.535.29: an Inferno Dragon (Range 3500, CollisionRadius 500,
//! a row that sets VariableDamage2) that walks after a target stops, and starts its attack, on the first tick that
//! starts with the target's centre within Range + the TARGET's radius, and its goal cell lies within Range of the
//! target's centre. Once it stands, its reach is Range + both radii again, as for every other unit. Every other flyer
//! walks to Range + its radius and stops at Range + both radii.
//!
//! The scene is tests/test_variable_damage_walk_reach.py's, the client 15.535.29 sweep scene: a Blue Inferno Dragon at
//! (9500, 11500) and a Red Knight (radius 500) walking down the right lane from (14000, 16500). WHAT IS PINNED, each
//! with its precondition:
//!   1. client16402_no_own_radius_walking: the Dragon stands for the first time on a tick that starts with the Knight
//!      within Range + the Knight's radius, and a tick of the chase started with the Knight inside Range + both radii;
//!   2. client16402_no_own_radius_walking: once standing, it does not walk on a tick that starts with the Knight within
//!      Range + both radii, and the Knight stands in that band from a standing Dragon on some tick (an implementation
//!      that uses the short reach always is refused);
//!   3. client16402_no_own_radius_walking: every goal cell the walking Dragon holds lies within Range (and one Knight
//!      step) of the Knight's centre; under the old value some lies beyond;
//!   4. range_plus_both_radii: the Dragon stops with the Knight in the band (today's engine);
//!   5. both values: a Baby Dragon (the same Range and radius, no VariableDamage2) stops with the Knight in the band;
//!   6. the arm's reach: under client16402_no_own_radius_walking the Inferno Dragon's row walks with no own radius,
//!      and the Mighty Miner's (a ground row that also sets VariableDamage2, unmeasured) keeps its own under both
//!      values;
//!   7. the shipped value is range_plus_both_radii.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test variable_damage_walk_reach`):
//!   * `walk_reach_keeps_own_radius` -- the new value still adds the walker's own radius: (1), (3) and (6) go red.
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, Calib, VariableDamageWalkReach};
use royalesim::target::walking_own_radius;
use royalesim::Team;

const NEW: VariableDamageWalkReach = VariableDamageWalkReach::Client16402NoOwnRadiusWalking;
const OLD: VariableDamageWalkReach = VariableDamageWalkReach::RangePlusBothRadii;

const DRAGON_AT: (i32, i32) = (9500, 11500);
const KNIGHT_AT: (i32, i32) = (14000, 16500);
const TICKS: u32 = 150;
/// How far past Range a goal cell may lie from the Knight's centre as this test reads it: the pass may choose it
/// around the Knight's moved position, one Knight step (60 native at its speed) from the start of the tick, and the
/// test measures from the nearer of the two.
const GOAL_SLACK: i64 = 100;

/// One tick: the Knight's START-of-tick centre distance from the flyer (native), whether the flyer moved on the tick,
/// whether it targets the Knight after it, and the goal cell its route names after it with the Knight's start- and
/// end-of-tick positions (native), when it has one.
struct Row {
    tick: u32,
    d: i64,
    moved: bool,
    on_knight: bool,
    goal: Option<((i64, i64), (i64, i64), (i64, i64))>,
}

/// The flyer's Range and radius and the Knight's radius, native, from the loaded cards.
struct Reach {
    range: i64,
    own: i64,
    knight: i64,
}

impl Reach {
    fn short(&self) -> i64 {
        self.range + self.knight
    }
    fn long(&self) -> i64 {
        self.range + self.own + self.knight
    }
}

fn native(v: Vec2) -> (i64, i64) {
    ((v.x / K) as i64, (v.y / K) as i64)
}

fn dist(a: (i64, i64), b: (i64, i64)) -> i64 {
    isqrt((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2))
}

fn walk(arm: VariableDamageWalkReach, flyer: &str) -> (Vec<Row>, Reach) {
    let mut cfg = config();
    cfg.calib.variable_damage_walk_reach = arm;
    let mut s = BattleState::new(0, cfg);
    let at = |p: (i32, i32)| Vec2::new(p.0 * K, p.1 * K);
    let dragon = s.scenario_spawn_now(Team::Blue, flyer, at(DRAGON_AT), None).unwrap_or_else(|e| panic!("spawn {flyer}: {e:?}"));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT_AT), None).unwrap_or_else(|e| panic!("spawn Knight: {e:?}"));
    let reach = {
        let (f, k) = (s.entity(dragon).expect("the flyer stands"), s.entity(knight).expect("the Knight stands"));
        Reach { range: (card_stat(&s, flyer).range / K) as i64, own: (f.radius / K) as i64, knight: (k.radius / K) as i64 }
    };
    let mut rows = Vec::new();
    for tick in 1..=TICKS {
        let (Some(f), Some(k)) = (s.entity(dragon), s.entity(knight)) else { break };
        let (f0, k0) = (native(f.pos), native(k.pos));
        s.tick();
        let (Some(f), Some(k)) = (s.entity(dragon), s.entity(knight)) else { break };
        let goal = f.route.first().map(|c| (native(*c), k0, native(k.pos)));
        rows.push(Row { tick, d: dist(f0, k0), moved: native(f.pos) != f0, on_knight: f.target == Some(knight), goal });
    }
    (rows, reach)
}

/// The index of the first row on which the flyer stands with the Knight as its target, with the scene's preconditions.
fn first_stand(rows: &[Row], r: &Reach, what: &str) -> usize {
    let chase: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].on_knight).collect();
    assert!(!chase.is_empty(), "{what}: the scene drifted: the flyer never targeted the Knight");
    assert!(rows[chase[0]].d > r.long(), "{what}: the scene drifted: the Knight was taken already inside the reach");
    let i = chase.into_iter().find(|&i| !rows[i].moved);
    i.unwrap_or_else(|| panic!("{what}: the scene drifted: the flyer never stood with the Knight as its target"))
}

#[test]
fn a_walking_inferno_dragon_stops_only_inside_range_plus_the_target_radius() {
    let (rows, r) = walk(NEW, "InfernoDragon");
    let i = first_stand(&rows, &r, "new");
    assert!(
        rows[..=i].iter().any(|w| w.on_knight && w.d > r.short() && w.d <= r.long()),
        "new: the scene drifted: no tick of the chase started with the Knight inside Range + both radii"
    );
    assert!(rows[i].d <= r.short(), "new: on {} the Dragon stopped {} from the Knight, outside Range + its radius ({})", rows[i].tick, rows[i].d, r.short());
    let stood: Vec<u32> = rows[..i].iter().filter(|w| w.on_knight && w.d > r.short() && !w.moved).map(|w| w.tick).collect();
    assert!(stood.is_empty(), "new: the Dragon stood outside Range + the Knight's radius on {stood:?}");
}

#[test]
fn a_standing_inferno_dragon_keeps_its_place_inside_both_radii() {
    let (rows, r) = walk(NEW, "InfernoDragon");
    let i = first_stand(&rows, &r, "new");
    let after: Vec<&Row> = (i + 1..rows.len()).filter(|&j| rows[j].on_knight && !rows[j - 1].moved && rows[j].d <= r.long()).map(|j| &rows[j]).collect();
    assert!(after.iter().any(|w| w.d > r.short()), "new: the scene drifted: the Knight never stood in the band from a standing Dragon");
    let walked: Vec<u32> = after.iter().filter(|w| w.moved).map(|w| w.tick).collect();
    assert!(walked.is_empty(), "new: a standing Dragon walked with the Knight inside Range + both radii on {walked:?}");
}

#[test]
fn a_walking_inferno_dragons_goal_cell_lies_within_range_of_its_target() {
    let beyond = |rows: &[Row], r: &Reach| -> Vec<(u32, i64)> {
        rows.iter()
            .filter(|w| w.on_knight && w.moved)
            .filter_map(|w| w.goal.map(|(c, k0, k1)| (w.tick, dist(c, k0).min(dist(c, k1)))))
            .filter(|&(_, g)| g > r.range + GOAL_SLACK)
            .collect()
    };
    let (rows, r) = walk(OLD, "InfernoDragon");
    assert!(!beyond(&rows, &r).is_empty(), "old: the scene drifted: every goal cell lay within Range of the Knight, so this looks at nothing");
    let (rows, r) = walk(NEW, "InfernoDragon");
    assert!(rows.iter().any(|w| w.on_knight && w.moved && w.goal.is_some()), "new: the scene drifted: the Dragon never walked with a goal cell");
    let far = beyond(&rows, &r);
    assert!(far.is_empty(), "new: goal cells beyond Range + {GOAL_SLACK} of the Knight (tick, distance): {far:?}");
}

#[test]
fn the_old_value_stops_inside_both_radii() {
    let (rows, r) = walk(OLD, "InfernoDragon");
    let i = first_stand(&rows, &r, "old");
    assert!(rows[i].d > r.short() && rows[i].d <= r.long(), "old: on {} the Dragon stopped {} from the Knight, not in ({}, {}]", rows[i].tick, rows[i].d, r.short(), r.long());
}

#[test]
fn a_baby_dragon_stops_inside_both_radii_under_both_values() {
    for arm in [NEW, OLD] {
        let (rows, r) = walk(arm, "BabyDragon");
        let i = first_stand(&rows, &r, "control");
        assert!(rows[i].d > r.short() && rows[i].d <= r.long(), "{arm:?}: on {} the Baby Dragon stopped {} from the Knight, not in ({}, {}]", rows[i].tick, rows[i].d, r.short(), r.long());
    }
}

#[test]
fn the_new_value_reaches_the_inferno_dragons_row_and_not_the_mighty_miners() {
    let s = BattleState::new(0, config());
    let (dragon, miner) = (card_stat(&s, "InfernoDragon"), card_stat(&s, "MightyMiner"));
    assert!(dragon.variable_damage.is_some() && dragon.is_flying(), "the card data moved: the Inferno Dragon is not a flyer with VariableDamage2");
    assert!(miner.variable_damage.is_some() && !miner.is_flying(), "the card data moved: the Mighty Miner is not a ground row with VariableDamage2");
    let own = 500 * K;
    for arm in [NEW, OLD] {
        let mut cfg = config();
        cfg.calib.variable_damage_walk_reach = arm;
        assert_eq!(walking_own_radius(&cfg.calib, miner, own), own, "{arm:?}: the Mighty Miner's walking reach dropped its own radius");
        let want = if arm == NEW { 0 } else { own };
        assert_eq!(walking_own_radius(&cfg.calib, dragon, own), want, "{arm:?}: the Inferno Dragon's walking reach");
    }
}

#[test]
fn the_shipped_value_is_the_old_one() {
    assert_eq!(Calib::shipped().variable_damage_walk_reach, OLD);
}
