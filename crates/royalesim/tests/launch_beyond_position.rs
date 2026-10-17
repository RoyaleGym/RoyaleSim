//! targeting.LAUNCH_BEYOND_POSITION: where a crown tower's target stands when its launch is judged against the tower's
//! reach (target.rs `decide`, `launch_judged_after_move`; state.rs `phase_attack_for`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, every crown tower fire tick whose target stood just past
//! reach at the tick's end: the tower let it go on the next tick, 59 of 59, 8 of them inside reach at the tick's start.
//!
//! THE SCENE: a red Knight held at (8500, 13100), 8,920 from Blue's right princess tower (reach 7,500 + 1,000 + 500 =
//! 9,000), until the tower fires at it; then put at (8400, 13180), 9,046 off (past reach, inside reach + 500), and one
//! tick run. The scene runs targeting.CHASE_HOLD_SCOPE at its 15.535.29 arm (client15535_walkers_only, what a 15.535.29
//! replay runs with this key): under the shipped every_holder the chase hold keeps a target past the tower's round sight
//! within the chase limit (here 6,680 on max(|dx|, |dy|), under 8,000) on both arms, so neither would part.
//! WHAT IS PINNED (the tower's target after that tick):
//!   1. client15535_after_move: the tower has let the Knight go (its launch judged on the Knight's new point);
//!   2. start_of_tick (the old arm, the vacuity check): it holds the Knight (launched inside reach, held to reach + 500);
//!   3. the shipped value is start_of_tick (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! A BUILDING (client16402_after_move_buildings_and_towers, a candidate): a Blue Cannon fires at a red Knight held 80 inside
//! its reach (Range + both radii), the Knight then put 40 past it and one tick run: the new arm lets it go;
//! client15535_after_move (towers only, the vacuity check) holds it.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test launch_beyond_position`):
//!   * `launch_judged_on_launch_tick` -- the new arm still judges the launch on the launch tick: (1) goes red.
//!   * `launch_judged_towers_only` -- client16402_after_move_buildings_and_towers still leaves a building out: the Cannon's
//!     test goes red.
//!   * `tower_hold_ignores_cycle_start` -- targeting.TOWER_HOLD_HIT_STARTED = client16402_after_first_tick still holds to
//!     reach + 500 one tick into the cycle: its test goes red.
//!
//! targeting.TOWER_HOLD_HIT_STARTED = client16402_after_first_tick (client 16.402, parity r76, live13: 30 of 30 let-gos one tick
//! into the cycle, the engine kept 28 of 29): the same scene, the Knight kept inside `extra` ticks after the launch, then put
//! past the reach.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::{AttackPhase, EntityKind};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, ChaseHoldScope, LaunchBeyondPosition, TowerHoldHitStarted};
use royalesim::Team;

fn n(x: i32, y: i32) -> Vec2 {
    Vec2::new(x * K, y * K)
}

/// Under `arm`: whether Blue's right princess tower still targets the Knight on the tick after its launch, the Knight put
/// past its reach between the two ticks.
fn tower_holds(arm: LaunchBeyondPosition) -> bool {
    let mut cfg: BattleConfig = config();
    cfg.calib.launch_beyond_position = arm;
    cfg.calib.chase_hold_scope = ChaseHoldScope::Client15535WalkersOnly;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(3, cfg);
    past_deploy_lockout(&mut s);
    let tower = s
        .entities()
        .filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower)
        .max_by_key(|e| e.pos.x)
        .map(|e| e.id)
        .expect("Blue's right princess tower");
    let (inside, past) = (n(8500, 13100), n(8400, 13180));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", inside, None).expect("the Knight");
    for _ in 0..200 {
        assert!(s.debug_set_pos(knight, inside));
        let max = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_hp(knight, max));
        s.tick();
        let t = s.entity(tower).expect("the tower");
        if t.target == Some(knight) && t.attack_phase == AttackPhase::Cooldown {
            // The tower fired on this tick, its Knight inside reach at the tick's start and at its end.
            assert!(s.debug_set_pos(knight, past));
            s.tick();
            return s.entity(tower).expect("the tower").target == Some(knight);
        }
    }
    panic!("{arm:?}: the scene drifted: the tower never fired at the Knight");
}

/// Plant: launch_judged_on_launch_tick.
#[test]
fn a_tower_lets_go_of_a_target_past_reach_after_its_launch_under_client15535_after_move() {
    assert!(!tower_holds(LaunchBeyondPosition::Client15535AfterMove), "new: the tower held the Knight past its reach");
}

#[test]
fn the_old_value_holds_it() {
    assert!(tower_holds(LaunchBeyondPosition::StartOfTick), "old: the tower let the Knight go (vacuous otherwise)");
}

#[test]
fn the_shipped_value_is_client16402_after_move_buildings_and_towers() {
    assert_eq!(Calib::shipped().launch_beyond_position, LaunchBeyondPosition::Client16402AfterMoveBuildingsAndTowers, "parity r70");
}

/// Under `arm`: whether a Blue Cannon still targets a red Knight on the tick after it fires at it, the Knight held 80 inside
/// its reach (Range + both radii, straight ahead of it) until the shot and put 40 past it between the two ticks.
fn cannon_holds(arm: LaunchBeyondPosition) -> bool {
    let mut cfg: BattleConfig = config();
    cfg.calib.launch_beyond_position = arm;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(3, cfg);
    past_deploy_lockout(&mut s);
    // On Blue's half, its reach straight ahead short of the river (y 15000).
    let at = n(12000, 6000);
    let cannon = s.scenario_spawn_now(Team::Blue, "Cannon", at, None).expect("the Cannon");
    let reach = card_stat(&s, "Cannon").range + s.entity(cannon).expect("the Cannon").radius + card_stat(&s, "Knight").collision_radius;
    let (inside, past) = (Vec2::new(at.x, at.y + reach - 80 * K), Vec2::new(at.x, at.y + reach + 40 * K));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", inside, None).expect("the Knight");
    for _ in 0..200 {
        assert!(s.debug_set_pos(knight, inside));
        let max = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_hp(knight, max));
        s.tick();
        let c = s.entity(cannon).expect("the Cannon");
        if c.target == Some(knight) && c.attack_phase == AttackPhase::Cooldown {
            assert!(s.debug_set_pos(knight, past));
            s.tick();
            return s.entity(cannon).expect("the Cannon").target == Some(knight);
        }
    }
    panic!("{arm:?}: the scene drifted: the Cannon never fired at the Knight");
}

/// targeting.LAUNCH_BEYOND_POSITION = client16402_after_move_buildings_and_towers: the Cannon lets the Knight go after its
/// launch, as a crown tower does; under client15535_after_move (towers only, the vacuity check) it holds it; and the tower's
/// scene is the same under both. Plant: launch_judged_towers_only.
#[test]
fn a_building_lets_go_of_a_target_past_reach_after_its_launch_under_client16402_after_move_buildings_and_towers() {
    assert!(!cannon_holds(LaunchBeyondPosition::Client16402AfterMoveBuildingsAndTowers), "new: the Cannon held the Knight past its reach");
    assert!(cannon_holds(LaunchBeyondPosition::Client15535AfterMove), "towers only: the Cannon let the Knight go (vacuous otherwise)");
    assert!(!tower_holds(LaunchBeyondPosition::Client16402AfterMoveBuildingsAndTowers), "new: the tower held the Knight past its reach");
}

/// Under targeting.TOWER_HOLD_HIT_STARTED = `arm` (the shipped launch judgement): whether Blue's right princess tower still
/// targets the Knight one tick after it is put past the reach, the Knight kept inside for `extra` ticks after the launch tick.
fn held_at_cycle(arm: TowerHoldHitStarted, extra: u32) -> bool {
    let mut cfg: BattleConfig = config();
    cfg.calib.tower_hold_hit_started = arm;
    cfg.calib.chase_hold_scope = ChaseHoldScope::Client15535WalkersOnly;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(3, cfg);
    past_deploy_lockout(&mut s);
    let tower = s
        .entities()
        .filter(|e| e.team == Team::Blue && e.kind == EntityKind::PrincessTower)
        .max_by_key(|e| e.pos.x)
        .map(|e| e.id)
        .expect("Blue's right princess tower");
    let (inside, past) = (n(8500, 13100), n(8400, 13180));
    let knight = s.scenario_spawn_now(Team::Red, "Knight", inside, None).expect("the Knight");
    let hold = |s: &mut BattleState, p: Vec2| {
        assert!(s.debug_set_pos(knight, p));
        let max = s.entity(knight).expect("the Knight").max_hp;
        assert!(s.debug_set_hp(knight, max));
    };
    for _ in 0..200 {
        hold(&mut s, inside);
        s.tick();
        let t = s.entity(tower).expect("the tower");
        if t.target == Some(knight) && t.attack_phase == AttackPhase::Cooldown {
            for _ in 0..extra {
                hold(&mut s, inside);
                s.tick();
            }
            hold(&mut s, past);
            s.tick();
            return s.entity(tower).expect("the tower").target == Some(knight);
        }
    }
    panic!("{arm:?}: the scene drifted: the tower never fired at the Knight");
}

/// targeting.TOWER_HOLD_HIT_STARTED = client16402_after_first_tick: past the reach on the tick after the launch both arms let
/// the Knight go (the shipped targeting.LAUNCH_BEYOND_POSITION judges the launch there); on the cycle's next tick the new arm
/// lets it go, where every_tick (the vacuity check) holds it; from the one after both hold. Plant:
/// tower_hold_ignores_cycle_start.
#[test]
fn under_client16402_after_first_tick_a_princess_tower_lets_go_early_in_its_cycle() {
    assert!(!held_at_cycle(TowerHoldHitStarted::EveryTick, 0), "every_tick: the launch's judgement held the Knight");
    assert!(!held_at_cycle(TowerHoldHitStarted::Client16402AfterFirstTick, 0), "client16402_after_first_tick: the tower held the Knight on the launch's next tick");
    assert!(held_at_cycle(TowerHoldHitStarted::EveryTick, 1), "every_tick: the tower let the Knight go one tick into its cycle");
    assert!(!held_at_cycle(TowerHoldHitStarted::Client16402AfterFirstTick, 1), "client16402_after_first_tick: the tower held the Knight past its reach one tick into its cycle");
    assert!(held_at_cycle(TowerHoldHitStarted::Client16402AfterFirstTick, 2), "client16402_after_first_tick: a started cycle let the Knight go");
}
