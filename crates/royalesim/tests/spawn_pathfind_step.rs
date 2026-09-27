//! movement.SPAWN_PATHFIND_STEP: how one tick's step of a unit under ground is taken (state.rs `tunnel_step`), the
//! step at its creation (movement.SPAWN_PATHFIND_START) included.
//!
//! THE LAW (client_250_substeps), measured on client 16.402 (capture 20260920-083112, the seat that saw every first
//! frame: the five tunnels of three Goblin Drills and two Miners) and on client 15.535.29 (37 Miner and Goblin Drill
//! tunnel runs over 20 distinct routes): a tick's SpawnPathfindSpeed is taken in sub-steps of at most 250
//! (move16402.rs `TUNNEL_SUBSTEP`), each straight at the route's next node and shortened to it when it is nearer, and
//! after each sub-step that node is dropped when it is within the speed + 1. It reproduces 1956 of 1956 recorded tunnel
//! frames exactly, 42 of 42 creation points (the Miner 611.7 from its King heading west or south and 646.2 heading
//! north or east; the dig 289.4 and 299.2) and 41 of 41 surfacing ticks. The old arm one_step reproduces none of the
//! frames exactly, 21 of the creation points and 23 of the surfacing ticks.
//!
//! WHAT IS PINNED:
//!   1. a Miner played to (3500, 1500): its first frame (7777, 3235), the next five (7127, 3241), (6477, 3243),
//!      (5827, 3245), (5240, 2976), (4718, 2594), and up on (3500, 1500) on the seventh (client 16.402 tick 2947;
//!      client 15.535.29 the same numbers);
//!   2. a Miner played on its own King's tile: first frame (9235, 1777), up on (8500, 500) the tick after (client
//!      16.402 tick 1203): its route's first node lies 354 from its King, which one_step stops at;
//!   3. the Goblin Drill played to (3500, 23500): (9178, 3569), (9208, 3866), (9235, 4164), (9241, 4464);
//!   4. one_step is the engine before the key: the Miner of (1) first stands on (8101, 3249) and comes up on the eighth
//!      frame.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="tunnel_substep_whole_speed"' CARGO_TARGET_DIR=target/plant cargo test --test
//! spawn_pathfind_step`): the sub-step arm takes the whole speed in one piece: (1), (2) and (3) go red.
//!
//! NOT RUN when written (no build): the expected frames are the recordings', and a line-for-line emulation of this arm
//! on the engine's own routes reproduces them.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, SpawnPathfindStep};
use royalesim::Team;

/// Both hands hold the Miner and the Goblin Drill (the deck's first four, unshuffled).
const DECK: [&str; 8] = ["Miner", "GoblinDrill", "Knight", "Archer", "Giant", "Minions", "Musketeer", "Valkyrie"];

fn board(step: SpawnPathfindStep) -> BattleState {
    let mut cfg: BattleConfig = config();
    cfg.calib.spawn_pathfind_step = step;
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let deck: Vec<String> = DECK.iter().map(|s| s.to_string()).collect();
    cfg.decks = [deck.clone(), deck];
    cfg.shuffle_decks = false;
    let mut s = BattleState::new(5, cfg);
    past_deploy_lockout(&mut s);
    s.scenario_set_elixir_milli(Team::Blue, 10_000);
    assert_eq!(&s.hand(Team::Blue)[..2], &["Miner", "GoblinDrill"], "the scene drifted: the hand");
    s
}

/// Play `card` for Blue at `at` (native) and read its unit's native position on each of the next `n` frames it
/// exists (the dig until it leaves its building).
fn walk(step: SpawnPathfindStep, card: &str, at: (i32, i32), n: usize) -> Vec<(i32, i32)> {
    let mut s = board(step);
    s.deploy(Team::Blue, card, Vec2::new(at.0 * K, at.1 * K)).expect("the play is taken");
    let mut out = Vec::new();
    for _ in 0..(n + 40) {
        s.tick();
        if let Some(e) = find_live(&s, Team::Blue, card).first() {
            out.push((e.pos.x / K, e.pos.y / K));
        } else if !out.is_empty() {
            break;
        }
        if out.len() == n {
            break;
        }
    }
    out
}

/// Plant: tunnel_substep_whole_speed.
#[test]
fn a_miner_played_west_walks_the_recorded_tunnel() {
    let got = walk(SpawnPathfindStep::Client250Substeps, "Miner", (3500, 1500), 7);
    assert_eq!(got, [(7777, 3235), (7127, 3241), (6477, 3243), (5827, 3245), (5240, 2976), (4718, 2594), (3500, 1500)]);
}

/// Plant: tunnel_substep_whole_speed.
#[test]
fn a_miner_played_on_its_kings_tile_comes_up_the_tick_after_its_first_frame() {
    let got = walk(SpawnPathfindStep::Client250Substeps, "Miner", (8500, 1500), 2);
    assert_eq!(got, [(9235, 1777), (8500, 500)]);
}

/// Plant: tunnel_substep_whole_speed.
#[test]
fn the_goblin_drills_dig_walks_the_recorded_tunnel() {
    let got = walk(SpawnPathfindStep::Client250Substeps, "GoblinDrill", (3500, 23500), 4);
    assert_eq!(got, [(9178, 3569), (9208, 3866), (9235, 4164), (9241, 4464)]);
}

#[test]
fn one_step_is_the_engine_before_the_key() {
    let got = walk(SpawnPathfindStep::OneStep, "Miner", (3500, 1500), 8);
    assert_eq!(&got[..2], &[(8101, 3249), (7451, 3249)], "{got:?}");
    assert_ne!(got[6], (3500, 1500), "one_step is still under ground on the seventh frame: {got:?}");
    assert_eq!(got[7], (3500, 1500), "one_step comes up on the eighth frame: {got:?}");
}
