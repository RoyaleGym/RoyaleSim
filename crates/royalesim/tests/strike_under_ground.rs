//! A STRIKE THAT LANDS AT ONCE SPARES A UNIT UNDER GROUND (match.TICK_ORDER = client_sequential_strike; state.rs
//! `land_strike`; combat.rs `land_at_once`, `untouchable_now`).
//!
//! Under the sequential order a direct strike lands during its unit's own turn (`land_at_once`), not in Resolve. It
//! keeps every immunity `resolve` has: the hide, the dash, an attached rider, and now a unit under ground under
//! movement.SPAWN_PATHFIND_BODY = untouchable, by the test `resolve` reads (`untouchable_now`). Before, a Valkyrie's
//! spin took hp off a Miner under ground under this order, and under the shipped order it did not.
//!
//! WHAT IS PINNED: `a_sequential_strike_spares_a_miner_under_ground`. A Valkyrie spins at a Knight with a Miner held
//! under ground at the Knight's centre. The Knight loses hp to every spin and the Miner none, under both orders.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test strike_under_ground`):
//!   * `strike_lands_under_ground` -- the strike that lands at once lands on a unit under ground: red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState, SpawnPathfindBody, TickOrder};
use royalesim::Team;

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Every card and tower at level 11.
fn level11(mut cfg: BattleConfig) -> BattleConfig {
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    cfg
}

/// The shipped arm of the key the scene stands on, asserted.
fn shipped() -> BattleConfig {
    let cfg = level11(config());
    assert_eq!(cfg.calib.spawn_pathfind_body, SpawnPathfindBody::Untouchable, "the shipped movement.SPAWN_PATHFIND_BODY");
    cfg
}

/// A Red Miner's tap on the Blue side, far from the scene's own route: the Miner is held under ground by hand.
const MINER_TAP: (i32, i32) = (9000, 9500);

// ---------------------------------------------------------------------------
// the sequential strike and a Miner under ground

/// A Blue Valkyrie and a Red Knight 1,500 apart, inside each other's reach. The Valkyrie spins on ticks 3, 33 and 63
/// of the 70, and the Knight lives through them.
const VALKYRIE_AT: (i32, i32) = (9000, 12000);
const KNIGHT3_AT: (i32, i32) = (9000, 13500);
const STRIKE_TICKS: u32 = 70;

/// The Knight's hp and the Miner's (hp, max hp, under ground) after each tick, under `order`. The Miner is played
/// before the first tick and held under ground at the Knight's centre: after every tick it is put back there. Its walk
/// steps it 650 a tick toward its route's next node, by the Red King, which it never reaches, so it never comes up.
fn spin(order: TickOrder) -> (Vec<i32>, Vec<(i32, i32, bool)>) {
    let mut cfg = shipped();
    cfg.calib.tick_order = order;
    let mut s = BattleState::new(0, cfg);
    s.scenario_spawn_now(Team::Blue, "Valkyrie", at(VALKYRIE_AT), None).expect("place the Valkyrie");
    let knight = s.scenario_spawn_now(Team::Red, "Knight", at(KNIGHT3_AT), None).expect("place the Knight");
    s.spawn_unit(Team::Red, "Miner", at(MINER_TAP), None).expect("play the Miner");
    let (mut hp, mut miner) = (Vec::new(), Vec::new());
    for _ in 0..STRIKE_TICKS {
        s.tick();
        let k = s.entity(knight).expect("the scene drifted: the Knight died");
        hp.push(k.hp);
        let m = find_live(&s, Team::Red, "Miner");
        let m = m.first().expect("the Miner is on the board");
        miner.push((m.hp, m.max_hp, m.tunnel_dest.is_some()));
        let (id, to) = (m.id, k.pos);
        assert!(s.debug_set_pos(id, to));
    }
    (hp, miner)
}

/// Plant: strike_lands_under_ground.
#[test]
fn a_sequential_strike_spares_a_miner_under_ground() {
    for order in [TickOrder::Client16402, TickOrder::ClientSequentialStrike] {
        let (knight, miner) = spin(order);
        let spins = knight.windows(2).filter(|w| w[1] < w[0]).count();
        assert!(spins >= 3, "{order:?}: the scene drifted: the Knight was struck {spins} times");
        assert!(miner.iter().all(|m| m.2), "{order:?}: the scene drifted: the Miner came up");
        let lost = miner.iter().map(|m| m.1 - m.0).max().unwrap_or(0);
        assert_eq!(lost, 0, "{order:?}: the Miner under ground at the Knight's centre lost {lost} hitpoints to the spins");
    }
}
