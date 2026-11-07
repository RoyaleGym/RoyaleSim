//! THE CHASE LIMIT HOLDS A TARGET PAST ROUND SIGHT (targeting.CHASE_DROP_RANGE = client_sight_minus_1000; target.rs
//! `decide`, `held_past_sight`). Measured on client 15.535.29: a walker holding a target that stands past its round sight
//! (centre distance > SightRange + both radii) but inside the chase-drop limit (max |dx|, |dy| <= SightRange + both
//! radii, less 1000) kept it 158 of 159 times for a troop target and 25 of 25 for a building; the engine's rescan, which finds
//! nothing past round sight, lost it (sp-champ-SkeletonKing-s0 t177).
//!
//! The scene: a blue Knight held on (5000, 10000) takes a red Knight held 3,000 off on the diagonal; the red Knight is
//! then put 4,800 off on both axes, 6,788 away (round sight 5,500 + 500 + 500 = 6,500; the limit 5,500 on the square).
//!
//! A target taken past the limit while still in round sight (the limit is a square, the sight a circle: straight
//! along x, 5,800 is past the 5,500 limit and inside the 6,500 sight) has never been inside it, so the chase drop never
//! lets it go; past round sight a troop holds it too under targeting.CHASE_HOLD_PAST_LIMIT = client15535_troops_kept
//! (client 15.535.29: 135 of 139 such holds kept; sp-champ-LittlePrince-s0 t190), and lets it go under the shipped
//! inside_only (the 16.402 corpus: 3 of 4 let go).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! chase_hold`): chase_lost_past_round_sight -> `a_target_past_round_sight_inside_the_chase_limit_is_kept` red;
//! chase_held_only_inside_limit -> `a_target_never_inside_the_limit_is_kept_past_round_sight` red.
#![allow(unexpected_cfgs)]
// THE c4 FLIP (parity r66): this file's scenes were written on the engine before it, so they run its old arms
// (common::config_pre_flip); each flipped key's own tests read both arms.
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, ChaseHoldBackwards};
use royalesim::Team;

fn at(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

#[test]
fn a_target_past_round_sight_inside_the_chase_limit_is_kept() {
    let mut cfg = config_pre_flip();
    let deck: Vec<String> = ["Knight", "Archers", "Giant", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let (me, near, far) = ((5000, 10000), (7100, 12100), (9800, 14800));
    let blue = s.scenario_spawn_now(Team::Blue, "Knight", at(me.0, me.1), None).expect("the blue Knight");
    let red = s.scenario_spawn_now(Team::Red, "Knight", at(near.0, near.1), None).expect("the red Knight");
    let mut took = false;
    for _ in 0..60 {
        assert!(s.debug_set_pos(blue, at(me.0, me.1)));
        assert!(s.debug_set_pos(red, at(near.0, near.1)));
        s.tick();
        if s.entity(blue).expect("the blue Knight").target == Some(red) {
            took = true;
            break;
        }
    }
    assert!(took, "the scene drifted: the blue Knight never took the red Knight");
    for k in 0..5 {
        assert!(s.debug_set_pos(blue, at(me.0, me.1)));
        assert!(s.debug_set_pos(red, at(far.0, far.1)));
        s.tick();
        assert_eq!(s.entity(blue).expect("the blue Knight").target, Some(red), "tick {k} past round sight, inside the limit: the blue Knight let the red one go");
    }
}

#[test]
fn a_target_never_inside_the_limit_is_kept_past_round_sight() {
    past_limit_scene(royalesim::state::ChaseHoldPastLimit::Client15535TroopsKept, true);
}

/// The shipped arm, inside_only, lets the same target go past the limit (the 16.402 corpus).
#[test]
fn the_shipped_arm_lets_a_never_inside_target_go_past_the_limit() {
    assert_eq!(royalesim::state::Calib::shipped().chase_hold_past_limit, royalesim::state::ChaseHoldPastLimit::InsideOnly);
    past_limit_scene(royalesim::state::ChaseHoldPastLimit::InsideOnly, false);
}

/// A blue Knight takes a red Knight 5,800 off along x (past the square limit, in the round sight), which then stands
/// 6,700 off (past both) for 5 ticks: is it kept every tick (`kept`) or let go on the first?
fn past_limit_scene(arm: royalesim::state::ChaseHoldPastLimit, kept: bool) {
    let mut cfg = config_pre_flip();
    cfg.calib.chase_hold_past_limit = arm;
    let deck: Vec<String> = ["Knight", "Archers", "Giant", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    // straight along x: 5,800 off is past the square limit (5,500) and inside the round sight (6,500); 6,700 is past both
    let (me, near, far) = ((5000, 10000), (10800, 10000), (11700, 10000));
    let blue = s.scenario_spawn_now(Team::Blue, "Knight", at(me.0, me.1), None).expect("the blue Knight");
    let red = s.scenario_spawn_now(Team::Red, "Knight", at(near.0, near.1), None).expect("the red Knight");
    let mut took = false;
    for _ in 0..60 {
        assert!(s.debug_set_pos(blue, at(me.0, me.1)));
        assert!(s.debug_set_pos(red, at(near.0, near.1)));
        s.tick();
        if s.entity(blue).expect("the blue Knight").target == Some(red) {
            took = true;
            break;
        }
    }
    assert!(took, "the scene drifted: the blue Knight never took the red Knight past the limit");
    for k in 0..5 {
        assert!(s.debug_set_pos(blue, at(me.0, me.1)));
        assert!(s.debug_set_pos(red, at(far.0, far.1)));
        s.tick();
        let held = s.entity(blue).expect("the blue Knight").target == Some(red);
        if kept {
            assert!(held, "{arm:?}: tick {k} past round sight and the limit, never inside: the blue Knight let the red one go");
        } else {
            assert!(!held, "{arm:?}: tick {k} past round sight and the limit: the blue Knight kept the red one");
            return;
        }
    }
}

/// The scene of `a_target_past_round_sight_inside_the_chase_limit_is_kept` under targeting.CHASE_HOLD_BACKWARDS = `arm`: of the
/// 5 ticks the red Knight stands past the blue one's round sight (inside the chase limit), those on which it is still held.
fn held_past_round_sight(arm: ChaseHoldBackwards) -> usize {
    let mut cfg = config_pre_flip();
    cfg.calib.chase_hold_backwards = arm;
    held_in(cfg, "Knight", (5000, 10000), (7100, 12100), (9800, 14800))
}

/// A blue `card` held on `me` takes a red Knight held on `near`; the red Knight is then put on `far`: of the next 5 ticks, those
/// on which the blue unit still holds it.
fn held_in(mut cfg: royalesim::state::BattleConfig, card: &str, me: (i32, i32), near: (i32, i32), far: (i32, i32)) -> usize {
    let deck: Vec<String> = ["Knight", "Archers", "Giant", "Musketeer", "Fireball", "Arrows", "Minions", "Zap"].iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(0, cfg).expect("the decks load");
    past_deploy_lockout(&mut s);
    let blue = s.scenario_spawn_now(Team::Blue, card, at(me.0, me.1), None).expect("the blue unit");
    let red = s.scenario_spawn_now(Team::Red, "Knight", at(near.0, near.1), None).expect("the red Knight");
    let took = (0..60).any(|_| {
        assert!(s.debug_set_pos(blue, at(me.0, me.1)));
        assert!(s.debug_set_pos(red, at(near.0, near.1)));
        s.tick();
        s.entity(blue).expect("the blue unit").target == Some(red)
    });
    assert!(took, "the scene drifted: the blue {card} never took the red Knight");
    (0..5)
        .filter(|_| {
            assert!(s.debug_set_pos(blue, at(me.0, me.1)));
            assert!(s.debug_set_pos(red, at(far.0, far.1)));
            s.tick();
            s.entity(blue).expect("the blue unit").target == Some(red)
        })
        .count()
}

/// targeting.CHASE_HOLD_BACKWARDS = client16402_backwards_path (client 16.402, parity r86, live14: 110 of 122 unflagged melee holds
/// past round sight let go, 223 of 230 flagged kept; the client's search keeps such a target only while the holder's path to it
/// goes backwards): the straight chase of the scene above never goes backwards, so the new arms let the red Knight go on the first
/// tick past round sight, where chase_limit keeps it all 5. Plant: chase_hold_backwards_unread.
#[test]
fn under_client16402_backwards_path_a_straight_chase_lets_a_target_past_round_sight_go() {
    assert_eq!(held_past_round_sight(ChaseHoldBackwards::Client16402BackwardsPath), 0, "client16402_backwards_path: held on a straight path");
    assert_eq!(held_past_round_sight(ChaseHoldBackwards::Client16402BackwardsPathStored), 0, "client16402_backwards_path_stored: held on a straight path");
    // NOT VACUOUS: chase_limit holds it inside the |dy| limit
    assert_eq!(held_past_round_sight(ChaseHoldBackwards::ChaseLimit), 5, "chase_limit: let go inside the limit");
}

/// Under the shipped search (pathfinding.PATH_SEARCH = client16402) and targeting.CHASE_HOLD_BACKWARDS = `arm`: a blue Knight held
/// on (9000, 12500) takes a red Knight held across the river on (9000, 17500), 5,000 away (in round sight 6,500); its route goes
/// round by a bridge, a waypoint farther from the red Knight than it is. The red Knight is then put on (4300, 17200), 6,647
/// away (past round sight, inside the chase limit's 5,500 square): of the next 5 ticks, those on which it is still held.
fn held_across_the_river(arm: ChaseHoldBackwards) -> usize {
    let mut cfg = config();
    cfg.calib.chase_hold_backwards = arm;
    assert_eq!(cfg.calib.path_search, royalesim::state::PathSearch::Client16402, "the scene drifted: not the shipped search");
    held_in(cfg, "Knight", (9000, 12500), (9000, 17500), (4300, 17200))
}

/// targeting.CHASE_HOLD_BACKWARDS = client16402_backwards_path under the SHIPPED search (parity r87: r86 wrote the flag only in
/// the older search, so under client16402 every melee walker past round sight let go, live14's two Skeleton swarms among them):
/// the blue Knight's route to the red one across the river went backwards, so it holds it all 5 ticks past round sight;
/// _stored rewrites the flag on the straight replan to the new point and lets go; chase_limit holds it inside the limit (the
/// vacuity check). Plant: backwards_flag_unplanned.
#[test]
fn under_client16402_backwards_path_the_shipped_search_holds_a_target_whose_route_went_backwards() {
    assert_eq!(held_across_the_river(ChaseHoldBackwards::Client16402BackwardsPath), 5, "client16402_backwards_path: let go after a backwards route");
    assert!(held_across_the_river(ChaseHoldBackwards::Client16402BackwardsPathStored) < 5, "client16402_backwards_path_stored: held on after a straight replan");
    assert_eq!(held_across_the_river(ChaseHoldBackwards::ChaseLimit), 5, "chase_limit (the vacuity check): let go inside the limit");
}

/// targeting.CHASE_HOLD_BACKWARDS = client16402_flyers_let_go (parity r87: the flyers' half of client16402_backwards_path, live14
/// 15 of 15 flyers past round sight let go): the scene above with a blue Minion lets the red Knight go on the first tick past
/// round sight, where chase_limit holds it; a Knight keeps chase_limit's count. Plant: chase_hold_backwards_unread.
#[test]
fn under_client16402_flyers_let_go_a_flyer_lets_a_target_past_round_sight_go_and_a_walker_keeps_the_chase_limit() {
    let minion = |arm: ChaseHoldBackwards| {
        let mut cfg = config_pre_flip();
        cfg.calib.chase_hold_backwards = arm;
        held_in(cfg, "Minions", (5000, 10000), (7475, 12475), (9800, 14800))
    };
    let kept = minion(ChaseHoldBackwards::ChaseLimit);
    assert!(kept > 0, "chase_limit (the vacuity check): the Minion let go at once");
    assert_eq!(minion(ChaseHoldBackwards::Client16402FlyersLetGo), 0, "client16402_flyers_let_go: the Minion held past round sight (chase_limit {kept})");
    assert_eq!(held_past_round_sight(ChaseHoldBackwards::Client16402FlyersLetGo), held_past_round_sight(ChaseHoldBackwards::ChaseLimit), "client16402_flyers_let_go: a walker left chase_limit");
}
