//! THE SUPER WITCH EVENT CARD (item 295; card.rs `SpawnerDef::unit2`, state.rs `spawner_pass`, entity.rs `spawn_waves`): a
//! second periodic unit whose waves alternate with the first's.
//!
//! THE MEASUREMENT (client 15.535.29, sp-event-SuperWitch-s0, level 11): 7 of 7 waves of 4 on one tick, Skeletons on waves 1,
//! 3, 5, 7 and Bats on 2, 4, 6, the first 19 ticks after her deploy end and then every 100, on the S, W, N, E ring of 2000.
//!
//! WHAT IS PINNED: a lone Blue Super Witch held in place: her first four waves are Skeletons, Bats, Skeletons, Bats, four
//! each, 100 ticks apart, the first 19 ticks after her deploy ends.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test super_witch`):
//!   * `second_spawner_never_alternates` -- every wave is Skeletons: red;
//!   * `second_spawner_refused` -- the card is refused again: red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::{BattleConfig, BattleState};
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

/// Her first four waves: (ticks after her deploy end, the unit, how many).
fn waves() -> Vec<(u32, String, usize)> {
    let mut cfg: BattleConfig = config();
    cfg.card_level = [11, 11];
    cfg.tower_level = [11, 11];
    let mut s = BattleState::new(7, cfg);
    past_deploy_lockout(&mut s);
    let at_w = at((14500, 14500));
    let w = s.scenario_spawn_now(Team::Blue, "SuperWitch", at_w, None).expect("the Super Witch loads and stands");
    let mut ready: Option<u32> = None;
    let mut seen: Vec<EntityId> = Vec::new();
    let mut out: Vec<(u32, String, usize)> = Vec::new();
    for _ in 0..500 {
        assert!(s.debug_set_pos(w, at_w));
        s.tick();
        let t = s.tick_count() - 1;
        if ready.is_none() && !s.entity(w).expect("the Super Witch").deploying {
            ready = Some(t);
        }
        let fresh: Vec<(EntityId, String)> = s.entities().filter(|e| e.spawned_by == Some(w) && !seen.contains(&e.id)).map(|e| (e.id, e.card.to_string())).collect();
        if let Some((_, first)) = fresh.first() {
            assert!(fresh.iter().all(|(_, c)| c == first), "one wave, one unit: {fresh:?}");
            out.push((t - ready.expect("a wave before her deploy ended"), first.clone(), fresh.len()));
            seen.extend(fresh.iter().map(|(id, _)| *id));
        }
        if out.len() == 4 {
            break;
        }
    }
    out
}

/// Plants: second_spawner_never_alternates, second_spawner_refused.
#[test]
fn her_waves_alternate_skeletons_and_bats_100_ticks_apart() {
    let w = waves();
    let units: Vec<&str> = w.iter().map(|x| x.1.as_str()).collect();
    assert_eq!(units, ["Skeleton", "Bat", "Skeleton", "Bat"], "her first four waves: {w:?}");
    assert!(w.iter().all(|x| x.2 == 4), "four a wave: {w:?}");
    let ticks: Vec<u32> = w.iter().map(|x| x.0).collect();
    assert_eq!(ticks, [19, 119, 219, 319], "the first 19 ticks after her deploy end, then every 100: {w:?}");
}
