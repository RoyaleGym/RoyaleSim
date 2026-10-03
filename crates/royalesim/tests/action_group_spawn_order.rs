//! spawner.ACTION_GROUP_SPAWN_ORDER: the order an action group's same-tick spawns are created in (state.rs `drill_pass`,
//! the Evo Goblin Drill's hide pair).
//!
//! THE LAW, measured on client 15.535.29 (the ledger has the rows): the hide pair's Goblin at x - 500 (Spawn_Goblin2,
//! listed second) is created before the one at x + 500 (Spawn_Goblin1), 6 of 6.
//!
//! THE SCENE (tests/evo_goblin_drill.rs's): the form's building put down on (9000, 10000) on blue's side; after its
//! first regular Goblin its hitpoints are set to 67 %, so it goes under at 66 % and puts down its pair a tick later.
//! WHAT IS PINNED, with its precondition (the pair at 8500 and 9500):
//!   1. client15535_reversed: the Goblin at 8500 has the lower team_seq;
//!   2. listed (the old arm, the vacuity check): the one at 9500 has;
//!   3. the shipped value is listed (a 15.535.29 replay runs the new arm: tests/replay_parity.rs pins it).
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test action_group_spawn_order`):
//!   * `action_spawns_listed` -- the new arm still creates them in the listed order: (1) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{ActionGroupSpawnOrder, BattleConfig, BattleState, Calib};
use royalesim::{EntityId, Team};

/// Under `arm`: the hide pair's x (native) in creation order (team_seq ascending).
fn hide_pair(arm: ActionGroupSpawnOrder) -> Vec<i32> {
    let mut cfg: BattleConfig = config();
    cfg.calib.action_group_spawn_order = arm;
    cfg.decks = [vec!["GoblinDrill".into(), "Knight".into()], vec!["Knight".into()]];
    cfg.forms = [vec![1, 0], Vec::new()];
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let d: EntityId = s.scenario_spawn_now(Team::Blue, "units.GoblinDrill_EV1", Vec2::new(9000 * K, 10000 * K), None).expect("the drill's building");
    let mut set = false;
    for _ in 0..200 {
        s.tick();
        let mut mine: Vec<(u32, i32, i32)> = s.entities().filter(|e| e.spawned_by == Some(d)).map(|e| (e.team_seq, e.pos.x / K, e.pos.y / K)).collect();
        if !set && !mine.is_empty() {
            assert!(s.debug_set_hp(d, 880));
            set = true;
            continue;
        }
        mine.retain(|g| g.2 == 10000 && (g.1 == 8500 || g.1 == 9500));
        if mine.len() == 2 {
            mine.sort();
            return mine.into_iter().map(|g| g.1).collect();
        }
    }
    panic!("{arm:?}: the scene drifted: no hide pair at 8500 and 9500");
}

#[test]
fn a_hide_pairs_x_minus_500_goblin_is_created_first_under_client15535_reversed() {
    assert_eq!(hide_pair(ActionGroupSpawnOrder::Client15535Reversed), vec![8500, 9500], "new: the x + 500 Goblin was created first");
}

#[test]
fn the_old_value_creates_them_in_the_listed_order() {
    assert_eq!(hide_pair(ActionGroupSpawnOrder::Listed), vec![9500, 8500], "old: the x - 500 Goblin was created first");
}

#[test]
fn the_shipped_value_is_listed() {
    assert_eq!(Calib::shipped().action_group_spawn_order, ActionGroupSpawnOrder::Listed);
}
