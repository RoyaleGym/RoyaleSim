//! movement.DEATH_BOMB_AVOIDANCE: what a death bomb on its fuse is to the move pass (state.rs `phase_path16402_for`).
//!
//! THE EVIDENCE (the ledger has the rows): client 15.535.29, every ground troop whose look circle met a fused bomb's
//! circle with no other body near started a turn on the next tick, 4 of 4 (sp-bridge-Giant-split-s0: a Giant walking
//! past a dead Bomb Tower's bomb turned -190 on t465 and steered round it); none of 76 whose circles stayed apart did.
//!
//! THE SCENE: a red Bomb Tower on (3500, 22500) killed, its bomb on its fuse; a blue Giant put down on (3250, 20500) two
//! ticks later, under the left princess tower's lane, walking north into the bomb's circle; its avoidance offset and its
//! distance to the bomb over the next 56 ticks (the fuse ends 60 ticks after the death).
//!
//! WHAT IS PINNED:
//!   1. the scene: the Bomb Tower is gone before the Giant is placed, and under none (the engine's, the vacuity check)
//!      the Giant comes within 1200 of the bomb (its circle and the bomb's overlap, so its look circle met the bomb's)
//!      with its offset 0 all the way;
//!   2. client15535_static_blocker: its offset leaves 0 while the bomb burns, starting at -190 or 190 (a turn round a
//!      static blocker), and it keeps farther from the bomb than under none.
//!
//! PLANT (`RUSTFLAGS='--cfg clash_plant="death_bomb_unavoided"' CARGO_TARGET_DIR=target/plant cargo test --test death_bomb_avoidance`):
//!   * `death_bomb_unavoided` -- the new arm's movers still walk through the bomb: (2) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{isqrt, Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleState, DeathBombAvoidance};
use royalesim::Team;

const BOMB: (i32, i32) = (3500, 22500);

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Under `arm`: the Giant's avoidance offset on each of its first 56 ticks, and its closest approach to the bomb (native).
fn giant(arm: DeathBombAvoidance) -> (Vec<i32>, i64) {
    let mut cfg = config();
    cfg.calib.death_bomb_avoidance = arm;
    let mut s = BattleState::new(0, cfg);
    let tower = s.scenario_spawn_now(Team::Red, "BombTower", at(BOMB), None).expect("the Bomb Tower");
    s.tick();
    assert!(s.debug_set_hp(tower, 0));
    s.tick();
    s.tick();
    assert!(s.entity(tower).is_none(), "the scene drifted: the Bomb Tower still stands");
    let g = s.scenario_spawn_now(Team::Blue, "Giant", at((3250, 20500)), None).expect("the Giant");
    let (mut offsets, mut near) = (Vec::new(), i64::MAX);
    for _ in 0..56 {
        s.tick();
        let Some(e) = s.entity(g) else { break };
        offsets.push(e.avoid_offset);
        let (dx, dy) = (i64::from(e.pos.x / K - BOMB.0), i64::from(e.pos.y / K - BOMB.1));
        near = near.min(isqrt(dx * dx + dy * dy));
    }
    (offsets, near)
}

/// Plant: death_bomb_unavoided.
#[test]
fn a_mover_steers_round_a_fused_bomb_under_client15535_static_blocker() {
    let (old, old_near) = giant(DeathBombAvoidance::None);
    // NOT VACUOUS: without the blocker the Giant walks into the bomb's circle unturned.
    assert!(old_near < 1200 && old.iter().all(|&o| o == 0), "the scene drifted: none came to {old_near} with offsets {old:?}");
    let (new, new_near) = giant(DeathBombAvoidance::Client15535StaticBlocker);
    let first = new.iter().copied().find(|&o| o != 0);
    assert!(matches!(first, Some(190) | Some(-190)), "client15535_static_blocker: no turn starting at 190 ({first:?}): {new:?}");
    assert!(new_near > old_near, "client15535_static_blocker: the Giant came as near as under none ({new_near} vs {old_near})");
}
