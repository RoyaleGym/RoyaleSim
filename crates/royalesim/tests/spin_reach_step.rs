//! combat.SPIN_REACH_STEP: what a Hero Valkyrie does on the tick after her chain reaches its target with no next link
//! (state.rs `spin_pass`, `SpinRun::step_at`, `spin_steps_once` in the move pass's in-reach branch, and its walking branch
//! under client16635_one_more_chase_step, `spin_chase`).
//!
//! THE LAW, measured on client 16.635 (parity r99, the oracle's il-mg-16635 set: 26 of 26 such reach endings with her
//! centre farther than her Range plus the target's radius from it stepped 118 to 120 straight at the target on the next
//! tick and stood from the tick after; the engine stood at once): on the next tick she takes one more walk step at her
//! speed then (the pending buff's 200 %: 120) straight at the target she holds, though it stands in her reach; from the
//! tick after she stands under the spin's NO_ATTACK. The exception: a reach that leaves her centre within her Range plus
//! the target's radius of its centre stands at once. The engine's arm, stands, stands from the tick after the reach.
//!
//! THE STEP'S DIRECTION, measured on client 16.635 (parity r100, the same set: 24 of 26 reach endings stepped within 0.62
//! degrees at the goal cell around the target, at her Range plus her radius, or at its route's next node; straight at the
//! target 3): client16635_one_more_chase_step takes the same step as a chaser's, along the route to that goal cell, not
//! straight at the target's centre.
//!
//! THE SCENE: 16.635's table; blue's princess towers down; combat.SPIN_TARGETS = client16635_ground_characters; a red
//! Cannon 3000 to the hero's right, its hp topped up every tick, and no other red unit near; the hero held 40 ticks, then
//! pressed: her chain begins on the Cannon. R is the reach: the first frame after which her speed falls from the chain
//! buff's 150 to the pending buff's 120. The exception's scene: a red Knight held 6500 to her right until the press, then
//! held 1500 to her right (her Range and its radius 1700): her chain begins on it and reaches it at once.
//!
//! WHAT IS PINNED: under client16635_one_more_step her move on R + 1 is 115 to 125, within 1 degree of straight at the
//! Cannon, and 0 on R + 2 and R + 3; under stands (the vacuity check) she does not move on R + 1. Under
//! client16635_one_more_chase_step her move on R + 1 is 115 to 125, within 1 degree of a node of the route she walked in
//! her chain (the move pass plans it again on R + 1 and pops the node she steps at, so she holds none after R) and more
//! than 5 degrees off straight at the Cannon, and 0 on R + 2 and R + 3. With the Knight 1500 off she does not move
//! after the press's first frame under any of the three arms.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --profile gate --test
//! spin_reach_step`):
//!   * `spin_reach_step_stands` -- spin_pass never marks the step under the new arm:
//!     `under_client16635_one_more_step_she_steps_once_after_the_last_reach` red (she stands on R + 1).
//!   * `spin_reach_step_straight` -- the move pass's `spin_chase` is false under client16635_one_more_chase_step, so she
//!     steps straight at the Cannon: `under_client16635_one_more_chase_step_her_step_walks_her_route_to_the_goal_cell`
//!     red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::card::{CardDb, FORM_HERO};
use royalesim::entity::EntityKind;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, CardTable, SpinReachStep, SpinTargets};
use royalesim::{EntityId, Team};

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

const DECK: [&str; 8] = ["Valkyrie", "Knight", "Archer", "Giant", "Musketeer", "HogRider", "Fireball", "Zap"];
const HERO: (i32, i32) = (9000, 13000);
const CANNON: (i32, i32) = (12000, 13000);
const KNIGHT_FAR: (i32, i32) = (15500, 13000);
const KNIGHT_NEAR: (i32, i32) = (10500, 13000);

/// A level-11 battle on 16.635's table under combat.SPIN_REACH_STEP = `arm` (combat.SPIN_TARGETS =
/// client16635_ground_characters, every other key shipped), the Hero Valkyrie in blue's deck, past the opening lockout,
/// blue's princess towers down.
fn battle(arm: SpinReachStep) -> BattleState {
    let mut cfg = BattleConfig::with_cards(CardDb::load_table(CardTable::Client160635007Nov03).expect("the 16.635 table"));
    cfg.calib.card_table = CardTable::Client160635007Nov03;
    cfg.calib.spin_reach_step = arm;
    cfg.calib.spin_targets = SpinTargets::Client16635GroundCharacters;
    let deck: Vec<String> = DECK.iter().map(|c| c.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.forms = [vec![FORM_HERO, 0, 0, 0, 0, 0, 0, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::try_new(7, cfg).unwrap_or_else(|e| panic!("the deck does not load: {e}"));
    let lockout = s.config().calib.deploy_lockout_ticks as u32;
    s.scenario_set_tick(lockout);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    let towers: Vec<EntityId> = s.entities().filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower).map(|e| e.id).collect();
    for t in towers {
        assert!(s.debug_set_hp(t, 0));
    }
    s.tick();
    s
}

/// One frame after the press: her point, her speed (native a tick), her target and her route's next waypoint (engine
/// subtiles) after it.
struct Frame {
    at: Vec2,
    speed: i32,
    target: Option<EntityId>,
    next: Option<Vec2>,
}

/// Under `arm`: a red `enemy` held on `before` (its hp topped up every tick) while the hero is held 40 ticks on HERO, then
/// on `after` from the press on; the press's first `count` frames (frame 0 its first), and the enemy's id.
fn frames(arm: SpinReachStep, enemy: &str, before: (i32, i32), after: (i32, i32), count: usize) -> (Vec<Frame>, EntityId) {
    let mut s = battle(arm);
    let foe = s.scenario_spawn_now(Team::Red, enemy, n(before.0, before.1), None).expect("the red unit");
    s.spawn_unit(Team::Blue, "Valkyrie_hero", n(HERO.0, HERO.1), None).expect("the hero");
    s.tick();
    let hero = find_live(&s, Team::Blue, "Valkyrie_hero")[0].id;
    let top = s.entity(foe).expect("the red unit").max_hp;
    let hold = |s: &mut BattleState, at: (i32, i32)| {
        assert!(s.debug_set_pos(foe, n(at.0, at.1)) && s.debug_set_hp(foe, top), "the red unit lives");
    };
    for _ in 0..40 {
        hold(&mut s, before);
        assert!(s.debug_set_pos(hero, n(HERO.0, HERO.1)));
        s.tick();
    }
    assert!(s.entities().all(|e| !e.deploying), "the scene is still deploying");
    hold(&mut s, after);
    assert!(s.debug_set_pos(hero, n(HERO.0, HERO.1)));
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    s.press_ability_button(Team::Blue, 0).expect("the press");
    let mut out = Vec::new();
    for _ in 0..count {
        hold(&mut s, after);
        s.tick();
        let h = s.entity(hero).expect("the hero");
        out.push(Frame { at: h.pos, speed: h.effective_speed / K, target: h.target, next: h.route.last().copied() });
    }
    (out, foe)
}

/// Under `arm`, the Cannon scene: R (the press's frame after which her chain buff was off: her speed fell from 150 to
/// 120), and the press's frames.
fn reach_frames(arm: SpinReachStep) -> (usize, Vec<Frame>) {
    let (f, _) = frames(arm, "Cannon", CANNON, CANNON, 30);
    let speeds: Vec<i32> = f.iter().map(|x| x.speed).collect();
    let r = (1..f.len())
        .find(|&k| f[k - 1].speed >= 140 && f[k].speed < 140)
        .unwrap_or_else(|| panic!("the scene drifted: her chain buff never came off: speeds {speeds:?}"));
    assert!(r + 3 < f.len(), "the scene drifted: the reach came on frame {r} of {}", f.len());
    (r, f)
}

/// The native vector from `a` to `b` (engine subtiles).
fn vec_to(a: Vec2, b: Vec2) -> (i64, i64) {
    (((b.x - a.x) / K) as i64, ((b.y - a.y) / K) as i64)
}

/// tan(1 degree), in millionths.
const TAN_1_DEG: i64 = 17_455;
/// tan(5 degrees), in millionths.
const TAN_5_DEG: i64 = 87_489;

/// Whether the angle between `a` and `b` is below the one whose tangent is `tan` millionths: their dot product positive,
/// and the size of their cross product x 1,000,000 below `tan` x the dot. Integers, as the engine.
fn within(a: (i64, i64), b: (i64, i64), tan: i64) -> bool {
    let dot = a.0 * b.0 + a.1 * b.1;
    let cross = (a.0 * b.1 - a.1 * b.0).abs();
    dot > 0 && cross * 1_000_000 < tan * dot
}

/// Under `arm`, the Cannon scene: R (`reach_frames`), her moves (native dx, dy) on R + 1, R + 2 and R + 3, and the vector
/// from her point after R to the Cannon.
fn after_reach(arm: SpinReachStep) -> (usize, [(i64, i64); 3], (i64, i64)) {
    let (r, f) = reach_frames(arm);
    let moves = [vec_to(f[r].at, f[r + 1].at), vec_to(f[r + 1].at, f[r + 2].at), vec_to(f[r + 2].at, f[r + 3].at)];
    (r, moves, vec_to(f[r].at, n(CANNON.0, CANNON.1)))
}

/// combat.SPIN_REACH_STEP = client16635_one_more_step (client 16.635, parity r99, the oracle's il-mg-16635 set: b20e731c
/// t605, her last reach a Cannon, the client stepped 120 at it and stood, the engine stood at once): on R + 1 she steps 115
/// to 125 within 1 degree of straight at the Cannon, then stands; under stands (the vacuity check) she does not move on
/// R + 1. It is the vacuity check of client16635_one_more_chase_step's step too. Plant: spin_reach_step_stands.
#[test]
fn under_client16635_one_more_step_she_steps_once_after_the_last_reach() {
    let (r0, old, _) = after_reach(SpinReachStep::Stands);
    // NOT VACUOUS: the engine's arm stands from the tick after the reach.
    assert_eq!(old[0], (0, 0), "stands: she moved on R + 1 (R the press's frame {r0})");
    let (r, new, to) = after_reach(SpinReachStep::Client16635OneMoreStep);
    assert_eq!(r, r0, "her chain reaches the Cannon on the same frame under both arms");
    let len = isqrt(new[0].0 * new[0].0 + new[0].1 * new[0].1);
    assert!((115..=125).contains(&len), "client16635_one_more_step: her move on R + 1 is {len} {:?}, not her pending 120", new[0]);
    assert!(
        within(new[0], to, TAN_1_DEG),
        "client16635_one_more_step: her move on R + 1 {:?} is not within 1 degree of straight at the Cannon ({to:?} off)",
        new[0]
    );
    assert_eq!([new[1], new[2]], [(0, 0); 2], "client16635_one_more_step: she stands on R + 2 and R + 3");
}

/// The exception: a reach that leaves her centre within her Range plus the target's radius of its centre (a red Knight held
/// 1500 to her right from the press: 1200 and its 500) stands at once under every arm (stands, client16635_one_more_step
/// and client16635_one_more_chase_step): from the press's first frame she does not move.
#[test]
fn a_reach_within_her_range_and_its_radius_stands_at_once_under_both_arms() {
    for arm in [SpinReachStep::Stands, SpinReachStep::Client16635OneMoreStep, SpinReachStep::Client16635OneMoreChaseStep] {
        let (f, knight) = frames(arm, "Knight", KNIGHT_FAR, KNIGHT_NEAR, 8);
        assert_eq!(f[2].target, Some(knight), "{arm:?}: the scene drifted: the Knight is not her target");
        let moved: Vec<i32> = (1..f.len()).map(|k| f[k].at.dist(f[k - 1].at) / K).collect();
        assert!(moved.iter().all(|&m| m == 0), "{arm:?}: she moved after the press's first frame: {moved:?}");
    }
}

/// combat.SPIN_REACH_STEP = client16635_one_more_chase_step (client 16.635, parity r100, the oracle's il-mg-16635 set: 24
/// of 26 reach endings stepped within 0.62 degrees at the goal cell around the target, at her Range plus her radius, or at
/// its route's next node; straight at the target 3): on R + 1 she steps 115 to 125 within 1 degree of a node of her route
/// to the goal cell around the Cannon (the route she walked in her chain, frames 0 to R: she holds none after R, as the
/// move pass plans it again on R + 1 and pops the node she steps at; the node lies about 14 degrees off her axis), more
/// than 5 degrees off straight at the Cannon, then stands on R + 2 and R + 3. Under client16635_one_more_step (the vacuity check,
/// `under_client16635_one_more_step_she_steps_once_after_the_last_reach`) the same step goes within 1 degree of straight
/// at it. Plant: spin_reach_step_straight.
#[test]
fn under_client16635_one_more_chase_step_her_step_walks_her_route_to_the_goal_cell() {
    let (r0, _, _) = after_reach(SpinReachStep::Client16635OneMoreStep);
    let (r, f) = reach_frames(SpinReachStep::Client16635OneMoreChaseStep);
    assert_eq!(r, r0, "her chain reaches the Cannon on the same frame under both stepping arms");
    let mut nodes: Vec<Vec2> = f[..r].iter().filter_map(|x| x.next).collect();
    nodes.dedup();
    assert!(!nodes.is_empty(), "the scene drifted: she walked no route in her chain (R the press's frame {r})");
    let step = vec_to(f[r].at, f[r + 1].at);
    let to_cannon = vec_to(f[r].at, n(CANNON.0, CANNON.1));
    let to_next = nodes
        .iter()
        .map(|&p| vec_to(f[r].at, p))
        .find(|&v| within(step, v, TAN_1_DEG))
        .unwrap_or_else(|| panic!("client16635_one_more_chase_step: her move on R + 1 {step:?} is not within 1 degree of a node of her route ({nodes:?})"));
    assert!(
        !within(to_next, to_cannon, TAN_5_DEG),
        "the scene drifted: her waypoint ({to_next:?} off) lies within 5 degrees of the Cannon ({to_cannon:?} off)"
    );
    let len = isqrt(step.0 * step.0 + step.1 * step.1);
    assert!((115..=125).contains(&len), "client16635_one_more_chase_step: her move on R + 1 is {len} {step:?}, not her pending 120");
    assert!(
        !within(step, to_cannon, TAN_5_DEG),
        "client16635_one_more_chase_step: her move on R + 1 {step:?} is within 5 degrees of straight at the Cannon ({to_cannon:?} off)"
    );
    let after = [vec_to(f[r + 1].at, f[r + 2].at), vec_to(f[r + 2].at, f[r + 3].at)];
    assert_eq!(after, [(0, 0); 2], "client16635_one_more_chase_step: she stands on R + 2 and R + 3");
}

/// Since parity r99 for clients 16.402 and 16.635 (stands before it; client 15.535.29 keeps stands).
#[test]
fn the_shipped_arm_is_client16635_one_more_step() {
    assert_eq!(
        royalesim::state::Calib::shipped().spin_reach_step,
        royalesim::state::SpinReachStep::Client16635OneMoreStep
    );
    assert_eq!(config().calib.spin_reach_step, royalesim::state::SpinReachStep::Client16635OneMoreStep);
}
