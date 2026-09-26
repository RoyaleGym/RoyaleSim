//! THE GOBLIN HUT'S LIFE-STATE CONTROLLER (card.rs `LifeStateDef`; state.rs `life_state_pass`, `life_wakers`,
//! `life_wave`).
//!
//! THE LAW, measured on client 15.535.29 (35 runs; the side rule on 161 of 161 waves) and on the 16.402 corpus (68 of
//! 68 waves over 32 huts):
//!   - ActionDelay (1000) counts down from the tick the hut is created, its deploy beside it; the tick after, the hut
//!     looks: an enemy in reach releases a wave at once and the next is due 43 ticks later, then every 44 while an
//!     enemy is in reach on the due tick; a due tick with none puts it to sleep, and the next enemy wakes it at once;
//!   - enemy troops wake it at centre - r <= Range + the hut's radius (7000), enemy buildings 228 further
//!     (spawner.LIFE_STATE_WAKE_REACH), air and ground (spawner.LIFE_STATE_WAKE_TARGETS);
//!   - a wave stands 1200 from the hut's centre, 20 degrees to one side of the line to its aim: the even waves on the
//!     lower-y point, or the larger-x one when the two are level within 14 native, the odd waves on the other.
//!
//! WHAT IS PINNED, each with its precondition:
//!   1. the loader reads the hut's controller and gives it no periodic spawner;
//!   2. an enemy building in reach from the start: waves on F + 20, F + 63 and F + 107 (F the hut's creation tick);
//!   3. an enemy straight ahead of the hut (the two points level): the first wave on the larger-x side, the second on
//!      the other;
//!   4. an air troop wakes it;
//!   5. the controller's clock is state: two saves differing only in it hash differently.
//!
//! PLANTS (`RUSTFLAGS='--cfg clash_plant="NAME"' CARGO_TARGET_DIR=target/plant cargo test --test goblin_hut`):
//!   * `life_first_interval_44` -- a whole interval after the first look's wave: (2) goes red on F + 64.
//!   * `life_no_alternation` -- every wave on the even side: (3) goes red.
//!   * `life_ground_only` -- an air troop wakes nothing: (4) goes red.
//!   * `hash_skips_life_state` -- the controller is not hashed: (5) goes red.
#![allow(unexpected_cfgs)]
mod common;

use common::*;
use royalesim::fixed::{Vec2, SUBTILE_PER_MILLITILE as K};
use royalesim::state::BattleState;
use royalesim::{EntityId, Team};

fn at(p: (i32, i32)) -> Vec2 {
    Vec2::new(p.0 * K, p.1 * K)
}

const HUT_AT: (i32, i32) = (9000, 7000);

#[test]
fn the_loader_reads_the_huts_controller_and_no_spawner() {
    let s = BattleState::new(0, config());
    let hut = card_stat(&s, "GoblinHut");
    assert!(hut.spawner.is_none(), "the hut's waves come from its controller, not a periodic spawner");
    let ls = hut.life_state.expect("the hut carries its life-state controller");
    assert_eq!((ls.number, ls.action_delay_ms, ls.interval_ms, ls.offset, ls.offset_angle_deg), (1, 1000, 2200, 1200 * K, 20));
    let unit = s.cards().get(ls.unit);
    assert_eq!(unit.name, "SpearGoblin_Dummy");
    assert!(unit.summon_only, "the wave's unit is released, not played");
}

/// A Blue hut played at HUT_AT with the red `enemy` set down at `enemy_at` first; per tick after the play (k = 0 is
/// the hut's creation tick), the new wave goblins: (k, id, position).
fn waves(enemy: &str, enemy_at: (i32, i32), ticks: u32) -> Vec<(u32, EntityId, Vec2)> {
    let mut s = BattleState::new(0, config());
    s.scenario_spawn_now(Team::Red, enemy, at(enemy_at), None).expect("spawn the enemy");
    s.spawn_unit(Team::Blue, "GoblinHut", at(HUT_AT), None).expect("play the hut");
    let mut seen: Vec<EntityId> = Vec::new();
    let mut out = Vec::new();
    for k in 0..ticks {
        s.tick();
        for v in s.entities().filter(|v| v.card == "SpearGoblin_Dummy") {
            if !seen.contains(&v.id) {
                seen.push(v.id);
                out.push((k, v.id, v.pos));
            }
        }
    }
    out
}

/// Plant: life_first_interval_44.
#[test]
fn a_building_in_reach_from_the_start_gives_waves_on_f_plus_20_63_and_107() {
    // A red Cannon 7750 north of the hut: centre - r = 7150, beyond the troops' 7000 and within the buildings' 7228,
    // and 6150 edge to edge from the hut, out of its own range.
    let got: Vec<u32> = waves("Cannon", (HUT_AT.0, HUT_AT.1 + 7750), 110).iter().map(|w| w.0).collect();
    assert_eq!(got.get(..3), Some(&[20, 63, 107][..]), "the wave ticks: {got:?}");
}

/// Plant: life_no_alternation.
#[test]
fn level_points_put_the_first_wave_on_the_larger_x_side_and_the_second_on_the_other() {
    let got = waves("Cannon", (HUT_AT.0, HUT_AT.1 + 7750), 70);
    assert!(got.len() >= 2, "the scene drifted: {} waves", got.len());
    let hx = HUT_AT.0 * K;
    assert!(got[0].2.x > hx && got[1].2.x < hx, "the first two waves at x {} and {} (the hut at {})", got[0].2.x / K, got[1].2.x / K, HUT_AT.0);
}

/// Plant: life_ground_only.
#[test]
fn an_air_troop_wakes_the_hut() {
    let got = waves("Minions", (HUT_AT.0, HUT_AT.1 + 6500), 25);
    assert_eq!(got.first().map(|w| w.0), Some(20), "the first look's wave: {got:?}");
}

/// Plant: hash_skips_life_state.
#[test]
fn the_controllers_clock_is_state() {
    let mut s = BattleState::new(0, config());
    s.spawn_unit(Team::Blue, "GoblinHut", at(HUT_AT), None).expect("play the hut");
    for _ in 0..5 {
        s.tick();
    }
    let hut = s.entities().find(|v| v.card == "GoblinHut").map(|v| v.id).expect("the hut stands");
    let bytes = s.save();
    let mut v: serde_json::Value = serde_json::from_slice(&bytes).expect("a snapshot is JSON");
    let col = v["ents"]["life_ms"].as_array_mut().expect("the snapshot carries the controller's clock");
    let i = hut.index as usize;
    let n = col[i].as_i64().expect("a clock");
    col[i] = serde_json::Value::from(n + 1);
    let edited = serde_json::to_vec(&v).unwrap();
    let a = BattleState::load(&bytes).expect("the save loads");
    let b = BattleState::load(&edited).expect("the edited save loads");
    assert_ne!(a.state_hash(), b.state_hash(), "two states differing only in the hut's clock hash alike");
}
