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
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test launch_beyond_position`):
//!   * `launch_judged_on_launch_tick` -- the new arm still judges the launch on the launch tick: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::entity::{AttackPhase, EntityKind};
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, Calib, ChaseHoldScope, LaunchBeyondPosition};
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
fn the_shipped_value_is_start_of_tick() {
    assert_eq!(Calib::shipped().launch_beyond_position, LaunchBeyondPosition::StartOfTick);
}
